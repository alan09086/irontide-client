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
pub mod categories;
pub mod files;
pub mod preferences;
pub mod response;
pub mod session_store;
pub mod state;
pub mod torrent_dto;
pub mod pieces;
pub mod torrents;
pub mod trackers;
pub mod webseeds;

use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};
use irontide::session::SessionHandle;

pub use response::{QbtError, QbtResponse};
pub use session_store::SessionStore;
pub use state::QbtState;

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
/// 404 when `qbt_compat.enabled = false` (security-through-invisibility).
///
/// `logout` lives on the unprotected router because real qBt returns 200 even
/// without a valid cookie — `require_sid` would otherwise reject it on a
/// missing/expired SID.
pub fn build_router(session: Arc<SessionHandle>) -> Router {
    // Session store TTL + max_sessions are fixed at router construction
    // (matches real qBt — restart to reconfigure). Runtime toggles of
    // qbt_compat.enabled take effect immediately because qbt_gate re-reads
    // settings per request.
    let store = Arc::new(SessionStore::new(
        std::time::Duration::from_secs(86_400),
        1024,
    ));
    let state = QbtState::new(session, store);

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
        .route("/api/v2/torrents/removeCategories", post(categories::remove));

    let torrent_details = Router::new()
        .route("/api/v2/torrents/trackers", get(trackers::list))
        .route("/api/v2/torrents/webseeds", get(webseeds::list))
        .route("/api/v2/torrents/pieceStates", get(pieces::states))
        .route("/api/v2/torrents/pieceHashes", get(pieces::hashes));

    // Lane C (M171) inserts here: `let torrent_tags    = Router::new().route("/api/v2/torrents/tags",     ...)...;`
    // Lane D (M171) inserts here: `let app_write       = Router::new().route("/api/v2/app/setPreferences", ...);`

    let protected = app_read
        .merge(torrent_core)
        .merge(category_routes)
        .merge(torrent_details)
        // Lane C (M171): .merge(torrent_tags)
        // Lane D (M171): .merge(app_write)
        .route_layer(from_fn_with_state(state.clone(), auth::require_sid))
        .with_state(state.clone());

    let unprotected = Router::new()
        .route("/api/v2/auth/login", post(auth::login))
        .route("/api/v2/auth/logout", post(auth::logout))
        .with_state(state.clone());

    Router::new()
        .merge(protected)
        .merge(unprotected)
        .route_layer(from_fn_with_state(state, auth::qbt_gate))
}
