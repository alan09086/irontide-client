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

pub mod app;
pub mod auth;
pub mod categories;
pub mod files;
pub mod preferences;
pub mod response;
pub mod session_store;
pub mod state;
pub mod torrent_dto;
pub mod torrents;

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
/// Composed of two layered sub-routers:
/// 1. `auth_router` — `/api/v2/auth/login` only; `qbt_gate` applied but NOT
///    `require_sid` (can't require a cookie to get a cookie).
/// 2. `protected_router` — everything else; both middlewares applied.
///
/// Both share a single `QbtState` (session handle + session store) so that
/// tokens issued via login are visible to other routes.
pub fn build_router(session: Arc<SessionHandle>) -> Router {
    // Build the session store with defaults. Live settings override at
    // request time via `qbt_gate`, but TTL/max_sessions are fixed at router
    // construction — matches real qBt (restart to reconfigure).
    // We read the *current* qbt_compat config synchronously via a blocking
    // round-trip at build time only if available; otherwise hardcoded safe
    // defaults (24h TTL, 1024 cap) are used. Runtime toggles via
    // `config set qbt_compat.enabled true` take effect immediately because
    // qbt_gate re-reads settings per request.
    let store = Arc::new(SessionStore::new(
        std::time::Duration::from_secs(86_400),
        1024,
    ));
    let state = QbtState::new(session, store);

    let protected = Router::new()
        .route("/api/v2/app/version", get(app::version))
        .route("/api/v2/app/webapiVersion", get(app::webapi_version))
        .route("/api/v2/app/buildInfo", get(app::build_info))
        .route("/api/v2/app/preferences", get(app::preferences))
        .route("/api/v2/torrents/categories", get(categories::list))
        .route("/api/v2/torrents/createCategory", post(categories::create))
        .route("/api/v2/torrents/editCategory", post(categories::edit))
        .route("/api/v2/torrents/removeCategories", post(categories::remove))
        .route("/api/v2/torrents/files", get(files::list))
        .route("/api/v2/torrents/info", get(torrents::info))
        .route("/api/v2/torrents/properties", get(torrents::properties))
        .route("/api/v2/torrents/add", post(torrents::add))
        .route("/api/v2/torrents/pause", post(torrents::pause))
        .route("/api/v2/torrents/resume", post(torrents::resume))
        .route("/api/v2/torrents/delete", post(torrents::delete))
        .route("/api/v2/torrents/recheck", post(torrents::recheck))
        .route("/api/v2/torrents/reannounce", post(torrents::reannounce))
        .route("/api/v2/transferInfo", get(torrents::transfer_info))
        .route_layer(from_fn_with_state(state.clone(), auth::require_sid))
        .with_state(state.clone());

    // logout is idempotent in real qBt — it must return 200 even without a
    // valid cookie. Keep it on the unprotected sub-router so require_sid
    // does not reject it on a missing/expired SID.
    let unprotected = Router::new()
        .route("/api/v2/auth/login", post(auth::login))
        .route("/api/v2/auth/logout", post(auth::logout))
        .with_state(state.clone());

    // Both sub-routers are gated by qbt_gate (applied AFTER with_state so the
    // middleware runs against the same state). The final router merges them.
    Router::new()
        .merge(protected)
        .merge(unprotected)
        .route_layer(from_fn_with_state(state, auth::qbt_gate))
}
