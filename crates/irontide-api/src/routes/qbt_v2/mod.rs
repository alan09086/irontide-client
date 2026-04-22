//! qBittorrent WebUI v2 compatibility layer (M168).
//!
//! Implements a subset of the qBt WebUI v2 HTTP API so that `*arr` clients
//! (Radarr, Sonarr, Prowlarr, Lidarr) can talk to IronTide as if it were
//! qBittorrent. Same pattern as CockroachDB speaking PostgreSQL wire protocol.
//!
//! # Middleware chain
//! ```text
//! request -> qbt_gate -> require_sid -> handler
//!            (404 if disabled)  (403 if no cookie)
//! ```
//! `auth/login` is registered on a separate sub-router so it skips
//! `require_sid` — you cannot require a cookie on the endpoint that issues
//! cookies.
//!
//! # Sub-router composition (M171 Lane A0)
//! The protected sub-router is built by merging per-concern mini-routers:
//! `app_read`, `torrent_core`, `category_routes`. Each M171 lane adds ONE
//! additional `.merge(...)` line rather than editing a 20-line `.route(...)`
//! chain — keeps merge conflicts across parallel worktrees minimal.

pub mod app;
pub mod auth;
pub mod brute_force;
pub mod categories;
pub mod files;
pub mod pieces;
pub mod preferences;
pub mod response;
pub mod security;
pub mod session_store;
pub mod state;
pub mod tags;
pub mod torrent_dto;
pub mod torrents;
pub mod trackers;
pub mod webseeds;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::connect_info::MockConnectInfo;
use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};
use irontide::session::SessionHandle;

pub use brute_force::{AdmissionDenied, AdmitGuard, BruteForceRegistry};
pub use response::{QbtError, QbtResponse};
pub use security::csrf_guard;
pub use session_store::SessionStore;
pub use state::{QbtState, default_argon2_permits, resolve_client_ip};

/// Build the qBt v2 sub-router along with the [`QbtState`] that backs it.
///
/// The state is returned so callers (e.g. the top-level `routes::build_router`)
/// can share its reverse-proxies RwLock with adjacent surfaces that also need
/// CSRF protection — most notably the `/webui/*` block (M172a Lane B).
pub fn build_router_with_state(session: Arc<SessionHandle>) -> (Router, QbtState) {
    let router_state = build_router_inner(session);
    (router_state.0, router_state.1)
}

/// Build the qBt v2 sub-router.
///
/// Composed of per-concern sub-routers merged together:
/// * `app_read` — `GET /api/v2/app/*` read-only endpoints
/// * `torrent_core` — the M168-M170 torrent endpoints + `/transferInfo`
/// * `category_routes` — M170 category CRUD
/// * (M171 adds `torrent_details`, `torrent_tags`, `app_write` as they land)
///
/// The merged `protected` router is then layered with `require_sid`, and
/// combined with an unprotected router for `auth/login` + `auth/logout`.
/// The whole thing is finally wrapped in `qbt_gate` so the surface returns
/// 404 when `qbt_compat.enabled = false` (explicit operator opt-out as of
/// v0.172.1 — see `QbtCompatSettings::enabled` docstring).
///
/// `logout` lives on the unprotected router because real qBt returns 200 even
/// without a valid cookie — `require_sid` would otherwise reject it on a
/// missing/expired SID.
pub fn build_router(session: Arc<SessionHandle>) -> Router {
    build_router_inner(session).0
}

fn build_router_inner(session: Arc<SessionHandle>) -> (Router, QbtState) {
    // Session store TTL + max_sessions are fixed at router construction
    // (matches real qBt — restart to reconfigure). Runtime toggles of
    // qbt_compat.enabled take effect immediately because qbt_gate re-reads
    // settings per request.
    let store = Arc::new(SessionStore::new(
        std::time::Duration::from_secs(86_400),
        1024,
    ));
    // M172a G2: default argon2 semaphore size — num_cpus*2 clamped [2,16].
    // Overrideable at runtime via `qbt_compat.max_concurrent_argon2_ops`,
    // but re-reading the setting would require piping the Settings snapshot
    // through router construction; instead we grab the upstream Settings
    // *once* here to honour any override, and leave live-reconfig to a
    // future milestone (requires rebuilding the Semaphore — design work).
    let argon2_permits = default_argon2_permits(None);
    // M172a Lane C: brute-force-ban registry. Capacity is fixed at router
    // construction — same caveat as the argon2 semaphore. Runtime changes
    // to `qbt_compat.brute_force_registry_capacity` only affect NEW
    // daemon instances, not live-reconfig (documented in
    // `classify_immediate`).
    let brute_force_capacity = brute_force::DEFAULT_REGISTRY_CAPACITY;
    let state = QbtState::new(session, store, argon2_permits, brute_force_capacity);

    // M172a Lane C: best-effort one-shot hydrator for the CIDR bypass
    // whitelist. `build_router` is sync (100+ call sites), so we spawn a
    // tokio task to fetch settings and seed `state.bypass_auth_subnet_whitelist`
    // from the operator's current configuration. Malformed CIDRs are
    // dropped silently — `Settings::validate` would have rejected them at
    // startup, so this path is only exercised under a hand-edited config
    // that bypassed validation. Subsequent `setPreferences` applies
    // refresh the whitelist via the shared `RwLock` so this is purely
    // "seed at boot".
    {
        let seed_state = state.clone();
        tokio::spawn(async move {
            if let Ok(settings) = seed_state.session.settings().await {
                let parsed: Vec<ipnet::IpNet> = settings
                    .qbt_compat
                    .bypass_auth_subnet_whitelist
                    .iter()
                    .filter_map(|s| s.parse().ok())
                    .collect();
                *seed_state.bypass_auth_subnet_whitelist.write() = parsed;
            }
        });
    }

    // M172a Lane B: seed the reverse-proxies list from the current Settings.
    // The build_router function is sync, so we spawn a best-effort task that
    // reads settings and populates the RwLock. Until this completes the list
    // is empty — equivalent to "no trusted proxies" — which is the safe
    // default anyway. Runtime updates via setPreferences invalidate the lock
    // through `SessionHandle::apply_settings_classified` (see session.rs).
    {
        let state_for_seed = state.clone();
        tokio::spawn(async move {
            if let Ok(settings) = state_for_seed.session.settings().await {
                let parsed: Vec<ipnet::IpNet> = settings
                    .qbt_compat
                    .web_ui_reverse_proxies_list
                    .iter()
                    .filter_map(|s| s.parse::<ipnet::IpNet>().ok())
                    .collect();
                *state_for_seed.reverse_proxies_list.write() = parsed;
            }
        });
    }

    let app_read = Router::new()
        .route("/api/v2/app/version", get(app::version))
        .route("/api/v2/app/webapiVersion", get(app::webapi_version))
        .route("/api/v2/app/buildInfo", get(app::build_info))
        .route("/api/v2/app/preferences", get(app::preferences));

    let torrent_core = Router::new()
        .route("/api/v2/torrents/info", get(torrents::info))
        .route("/api/v2/torrents/properties", get(torrents::properties))
        .route("/api/v2/torrents/files", get(files::list))
        .route("/api/v2/torrents/add", post(torrents::add))
        .route("/api/v2/torrents/pause", post(torrents::pause))
        .route("/api/v2/torrents/resume", post(torrents::resume))
        .route("/api/v2/torrents/delete", post(torrents::delete))
        .route("/api/v2/torrents/recheck", post(torrents::recheck))
        .route("/api/v2/torrents/reannounce", post(torrents::reannounce))
        .route("/api/v2/transferInfo", get(torrents::transfer_info));

    let category_routes = Router::new()
        .route("/api/v2/torrents/categories", get(categories::list))
        .route("/api/v2/torrents/createCategory", post(categories::create))
        .route("/api/v2/torrents/editCategory", post(categories::edit))
        .route(
            "/api/v2/torrents/removeCategories",
            post(categories::remove),
        );

    let torrent_details = Router::new()
        .route("/api/v2/torrents/trackers", get(trackers::list))
        .route("/api/v2/torrents/webseeds", get(webseeds::list))
        .route("/api/v2/torrents/pieceStates", get(pieces::states))
        .route("/api/v2/torrents/pieceHashes", get(pieces::hashes));

    let torrent_tags = Router::new()
        .route("/api/v2/torrents/tags", get(tags::list))
        .route("/api/v2/torrents/createTags", post(tags::create))
        .route("/api/v2/torrents/deleteTags", post(tags::delete))
        .route("/api/v2/torrents/addTags", post(tags::add_to_torrents))
        .route(
            "/api/v2/torrents/removeTags",
            post(tags::remove_from_torrents),
        );

    // Lane D (M171): setPreferences is the only `app` write endpoint today;
    // shutdown lands in a later milestone.
    let app_write = Router::new().route("/api/v2/app/setPreferences", post(app::set_preferences));

    let protected = app_read
        .merge(torrent_core)
        .merge(category_routes)
        .merge(torrent_details)
        .merge(torrent_tags)
        .merge(app_write)
        // M172a Lane B: CSRF guard sits outside `require_sid` so mutating
        // requests with a valid cookie but a cross-origin Origin still 403
        // (a browser-XSRF scenario against an authenticated session).
        .route_layer(from_fn_with_state(state.clone(), security::csrf_guard))
        .route_layer(from_fn_with_state(state.clone(), auth::require_sid))
        .with_state(state.clone());

    let unprotected = Router::new()
        .route("/api/v2/auth/login", post(auth::login))
        .route("/api/v2/auth/logout", post(auth::logout))
        // M172a Lane B: login itself is CSRF-protected — an Origin-mismatched
        // login from a hostile tab must not be allowed to plant a SID cookie.
        // Logout stays idempotent per qBt semantics, but routing the guard on
        // both keeps the behaviour consistent.
        .route_layer(from_fn_with_state(state.clone(), security::csrf_guard))
        .with_state(state.clone());

    // M172a C3: `ConnectInfo<SocketAddr>` is a required extractor on
    // `auth::login`. Production binds go through
    // `into_make_service_with_connect_info::<SocketAddr>()` in
    // `ApiServer::run`, which provides a real peer address; that path takes
    // precedence over `MockConnectInfo` per axum semantics. Test fixtures
    // that use `tower::ServiceExt::oneshot` skip the make-service layer —
    // the `MockConnectInfo` here injects a synthetic 0.0.0.0:0 fallback
    // so the test path doesn't hit a 500 on login. Never fires in
    // production because the make-service layer overrides it.
    let router = Router::new()
        .merge(protected)
        .merge(unprotected)
        .route_layer(from_fn_with_state(state.clone(), auth::qbt_gate))
        .layer(MockConnectInfo::<SocketAddr>(SocketAddr::from((
            [0_u8, 0, 0, 0],
            0,
        ))));
    (router, state)
}
