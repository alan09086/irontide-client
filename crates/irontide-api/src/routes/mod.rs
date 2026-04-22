//! HTTP route definitions for the torrent REST API.

pub mod events;
pub mod extended;
pub mod qbt_v2;
pub mod session;
pub mod torrents;
#[cfg(feature = "webui")]
pub mod webui;

// M172a Lane A: re-export the argon2 concurrency helper for integration
// tests and downstream tuning by alternate entry points.
pub use qbt_v2::default_argon2_permits;

use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::{any, delete, get, patch, post};
use irontide::session::SessionHandle;

/// Shared application state passed to every handler via axum's `State` extractor.
pub(crate) type AppState = Arc<SessionHandle>;

/// Build the axum router with all API routes.
///
/// Accepts a [`SessionHandle`] that is shared across all route handlers
/// via axum's state extraction.
pub fn build_router(session: SessionHandle) -> Router {
    let state: AppState = Arc::new(session);

    #[allow(unused_mut)]
    let mut router = Router::new()
        // -- Torrent routes --
        .route(
            "/api/v1/torrents",
            get(torrents::list_torrents).post(torrents::add_torrent),
        )
        .route(
            "/api/v1/torrents/{hash}",
            get(torrents::get_torrent).delete(torrents::delete_torrent),
        )
        .route(
            "/api/v1/torrents/{hash}/pause",
            post(torrents::pause_torrent),
        )
        .route(
            "/api/v1/torrents/{hash}/resume",
            post(torrents::resume_torrent),
        )
        .route(
            "/api/v1/torrents/{hash}/seed_mode",
            post(torrents::set_seed_mode),
        )
        // -- Session routes --
        .route("/api/v1/session/stats", get(session::get_stats))
        .route("/api/v1/session/counters", get(session::get_counters))
        .route(
            "/api/v1/session/settings",
            get(session::get_settings).patch(session::patch_settings),
        )
        .route("/api/v1/session/shutdown", post(session::shutdown))
        // -- Extended torrent routes --
        .route(
            "/api/v1/torrents/{hash}/info",
            get(extended::get_torrent_info),
        )
        .route("/api/v1/torrents/{hash}/peers", get(extended::get_peers))
        .route(
            "/api/v1/torrents/{hash}/trackers",
            get(extended::get_trackers),
        )
        .route(
            "/api/v1/torrents/{hash}/reannounce",
            post(extended::reannounce),
        )
        .route(
            "/api/v1/torrents/{hash}/files/{idx}/priority",
            patch(extended::set_file_priority),
        )
        // -- Peer ban management routes --
        .route("/api/v1/peers/banned", get(extended::get_banned_peers))
        .route("/api/v1/peers/ban", post(extended::ban_peer))
        .route("/api/v1/peers/ban/{ip}", delete(extended::unban_peer))
        // -- WebSocket event stream --
        .route("/api/v1/events", any(events::ws_events));

    // -- qBt WebUI v2 compatibility surface (M168) --
    // Registered BEFORE webui so the /api/v2/* routes are matched by the
    // qBt sub-router even when the generic webui fallback would otherwise
    // catch them. Enabled-by-default as of v0.172.1 (flipped from M168's
    // security-through-invisibility default). qbt_gate middleware still
    // returns 404 when `qbt_compat.enabled == false`, which is now an
    // explicit opt-out rather than the shipped default.
    //
    // M172a Lane B: the same [`QbtState`] backs both the qBt v2 routes and
    // the `/webui/*` CSRF guard so both surfaces consult the same trusted-
    // proxies RwLock.
    let (qbt_router, qbt_state) = qbt_v2::build_router_with_state(Arc::clone(&state));

    // -- Web UI routes (feature-gated) --
    #[cfg(feature = "webui")]
    {
        // IMPORTANT: register all /webui/* routes BEFORE the serve_static
        // fallback. serve_static catches "/anything" — any new route must
        // be declared first (M167 plan note).
        router = router
            .route(
                "/webui/fragments/torrent-list",
                get(webui::torrent_list_fragment),
            )
            .route(
                "/webui/fragments/torrent/{hash}/info",
                get(webui::info_fragment),
            )
            .route(
                "/webui/fragments/torrent/{hash}/files",
                get(webui::files_fragment),
            )
            .route(
                "/webui/torrents/{hash}/files/{idx}",
                patch(webui::patch_file_priority),
            )
            .route(
                "/webui/fragments/torrent/{hash}/trackers",
                get(webui::trackers_fragment),
            )
            .route(
                "/webui/torrents/{hash}/reannounce",
                post(webui::reannounce_action),
            )
            .route(
                "/webui/fragments/torrent/{hash}/peers",
                get(webui::peers_fragment),
            )
            .route("/webui/fragments/settings", get(webui::settings_fragment))
            .route("/webui/add-magnet", post(webui::add_magnet_redirect))
            .route("/webui/settings", patch(webui::patch_settings_webui))
            .route("/webui/torrents/{hash}/pause", post(webui::pause_action))
            .route("/webui/torrents/{hash}/resume", post(webui::resume_action))
            .route(
                "/webui/torrents/{hash}",
                get(webui::torrent_detail).delete(webui::delete_action),
            )
            .route(
                "/webui/torrents/{hash}/seed-mode",
                post(webui::seed_mode_action),
            )
            .fallback(webui::serve_static);
    }

    // M172a Lane B: wrap the whole `AppState`-typed router (torrents + webui
    // surfaces) in the CSRF guard. `route_layer` with a typed-state middleware
    // and the `with_state(state)` call below coexist because the layer only
    // closes over `QbtState`, not `AppState`.
    let router = router
        .route_layer(from_fn_with_state(qbt_state, qbt_v2::csrf_guard))
        .with_state(state);
    router.merge(qbt_router)
}
