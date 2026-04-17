//! HTMX-driven Web UI handlers.
//!
//! Endpoints:
//!
//! - `GET /webui/fragments/torrent-list` — HTML fragment of the torrent table
//! - `POST /webui/add-magnet` — add a magnet URI via form submission
//! - `POST /webui/torrents/{hash}/pause` / `/resume` — toggle a torrent's run state
//! - `DELETE /webui/torrents/{hash}` — remove a torrent from the session
//! - `POST /webui/torrents/{hash}/seed-mode?enabled=<bool>` — toggle seed-only mode
//! - Fallback — serve static assets from the embedded `irontide-webui-assets` crate
//!
//! Successful mutating actions respond with `HX-Trigger: refreshList` so that
//! the polling torrent-list fragment refreshes immediately. Failures produce an
//! `<p class="error-message">…</p>` HTML fragment that HTMX can swap into an
//! error-display target.

use askama::Template;
use askama_web::WebTemplateExt;
use axum::extract::{Path, Query, Request, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use irontide::session::TorrentState;
use serde::Deserialize;

use super::AppState;
use crate::error::ApiError;

// ---------------------------------------------------------------------------
// Template types
// ---------------------------------------------------------------------------

/// A single row in the torrent list table, with all values pre-formatted
/// so the template contains no formatting logic.
pub(crate) struct TorrentRow {
    pub name: String,
    pub size: String,
    pub progress: f64,
    pub progress_pct: String,
    pub down_rate: String,
    pub up_rate: String,
    pub seeds: usize,
    pub peers: usize,
    pub state: String,
    pub state_class: String,
    /// Hex-encoded SHA-1 info hash; the action-button endpoints key off this.
    pub info_hash: String,
    /// True when the torrent is in `TorrentState::Paused` — drives
    /// pause-vs-resume button selection in the template.
    pub is_paused: bool,
    /// Current user seed-mode flag; flips the seed-mode toggle label between
    /// "Seed" and "Unseed".
    pub user_seed_mode: bool,
}

/// Askama template that renders the torrent list as an HTML `<table>` fragment.
#[derive(Template)]
#[template(path = "torrent_list.html")]
pub(crate) struct TorrentListTemplate {
    pub torrents: Vec<TorrentRow>,
}

/// Askama template that renders the full detail page for a single torrent.
///
/// The Info panel is rendered server-side via `{% include "info_tab.html" %}`
/// so the first paint shows content without a round-trip, while the Files,
/// Trackers, and Peers panels are lazy-loaded via HTMX.
#[derive(Template)]
#[template(path = "torrent_detail.html")]
pub(crate) struct TorrentDetailTemplate {
    // ── Identity (shared with Info tab via include) ──
    /// Lowercase hex SHA-1 info hash. Used in every route/fragment URL.
    pub info_hash: String,
    /// Lowercase hex SHA-256 info hash, if the torrent is v2 or hybrid.
    pub info_hash_v2: Option<String>,
    /// Torrent display name (already HTML-safe through askama's default escaper).
    pub name: String,

    // ── Header (top of page) ──
    pub state: String,
    pub state_class: String,
    pub progress: f64,
    pub progress_pct: String,

    // ── Summary row ──
    pub down_rate: String,
    pub up_rate: String,
    pub eta: String,
    pub ratio: String,

    // ── Info-tab fields (included server-side) ──
    /// True when metadata has not been received yet — the Info tab renders a
    /// pending indicator and the size/pieces/private fields are hidden.
    pub metadata_pending: bool,
    pub total_size: String,
    pub piece_length: String,
    pub num_pieces: String,
    pub private: bool,
    pub download_path: String,
}

/// Askama template that renders ONLY the Info tab, as a standalone fragment.
/// Used by `GET /webui/fragments/torrent/{hash}/info` (Task 3).
#[derive(Template)]
#[template(path = "info_tab.html")]
pub(crate) struct InfoTabTemplate {
    pub info_hash: String,
    pub info_hash_v2: Option<String>,
    pub name: String,
    pub metadata_pending: bool,
    pub total_size: String,
    pub piece_length: String,
    pub num_pieces: String,
    pub private: bool,
    pub download_path: String,
}

/// Askama template that renders the settings form fragment, pre-populated
/// with the current session's values.
#[derive(Template)]
#[template(path = "settings_form.html")]
pub(crate) struct SettingsFormTemplate {
    pub listen_port: u16,
    pub download_dir: String,
    pub max_torrents: usize,
    pub max_peers_per_torrent: usize,
    pub download_rate_limit: u64,
    pub upload_rate_limit: u64,
    pub active_downloads: i32,
    pub active_seeds: i32,
    pub enable_dht: bool,
    pub enable_pex: bool,
    pub enable_lsd: bool,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build an empty `200 OK` response carrying `HX-Trigger: refreshList` so
/// that HTMX refreshes the torrent-list fragment on the client side.
fn refresh_response() -> Response {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "HX-Trigger",
        axum::http::HeaderValue::from_static("refreshList"),
    );
    (StatusCode::OK, headers, String::new()).into_response()
}

/// Render an HTML error fragment that HTMX can swap into an error-display
/// target. The status code is applied to the response so clients can
/// distinguish between validation and not-found cases.
fn error_fragment(status: StatusCode, message: &str) -> Response {
    (
        status,
        Html(format!(
            r#"<p class="error-message">{}</p>"#,
            html_escape(message)
        )),
    )
        .into_response()
}

/// Translate an [`ApiError`] into the HTML fragment the Web UI expects,
/// preserving the original HTTP status.
fn api_error_fragment(e: ApiError) -> Response {
    error_fragment(e.status, &e.message)
}

/// Map a torrent state label to a CSS class name for colour-coding.
fn state_css_class(state: &str) -> &'static str {
    match state {
        "downloading" => "downloading",
        "seeding" => "seeding",
        "complete" => "complete",
        "paused" => "paused",
        "stopped" => "stopped",
        "checking" => "checking",
        "fetching metadata" => "fetching",
        "seed only" => "seed-only",
        "sharing" => "sharing",
        _ => "unknown",
    }
}

/// Escape HTML special characters to prevent XSS in error messages.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(ch),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /webui/fragments/torrent-list`
///
/// Fetches the current torrent list from the session and renders it as an
/// HTML table fragment suitable for HTMX replacement.
pub async fn torrent_list_fragment(State(session): State<AppState>) -> impl IntoResponse {
    let summaries = match session.list_torrent_summaries().await {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(format!(
                    r#"<p class="error-message">{}</p>"#,
                    html_escape(&e.to_string())
                )),
            )
                .into_response();
        }
    };

    let rows: Vec<TorrentRow> = summaries
        .into_iter()
        .map(|t| {
            let state_label =
                irontide_format::format_state(&t.state, t.user_seed_mode).to_owned();
            let css_class = state_css_class(&state_label).to_owned();
            let is_paused = matches!(t.state, TorrentState::Paused);
            let user_seed_mode = t.user_seed_mode;
            let info_hash = t.info_hash.clone();
            TorrentRow {
                name: t.name,
                size: irontide_format::format_size(t.total_size),
                progress: t.progress,
                progress_pct: format!("{:.1}%", t.progress * 100.0),
                down_rate: irontide_format::format_rate(t.download_rate),
                up_rate: irontide_format::format_rate(t.upload_rate),
                seeds: t.num_seeds,
                peers: t.num_peers,
                state: state_label,
                state_class: css_class,
                info_hash,
                is_paused,
                user_seed_mode,
            }
        })
        .collect();

    let tmpl = TorrentListTemplate { torrents: rows };
    tmpl.into_web_template().into_response()
}

/// Form body for the add-magnet endpoint.
#[derive(serde::Deserialize)]
pub struct AddMagnetForm {
    uri: String,
}

/// `POST /webui/add-magnet`
///
/// Accepts a magnet URI from an HTML form and adds the torrent to the session.
///
/// On success, returns an empty body with an `HX-Trigger: refreshList` header
/// so that HTMX automatically refreshes the torrent list.
///
/// On failure, returns 422 with an HTML error message fragment.
pub async fn add_magnet_redirect(
    State(session): State<AppState>,
    axum::Form(form): axum::Form<AddMagnetForm>,
) -> Response {
    match session.add_magnet_uri(&form.uri).await {
        Ok(_) => refresh_response(),
        Err(e) => error_fragment(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string()),
    }
}

/// `POST /webui/torrents/{hash}/pause`
///
/// Pause an active torrent. On success, emits `HX-Trigger: refreshList` so
/// the torrent-list fragment refreshes and the button flips to "Resume".
/// On failure (invalid hash, unknown torrent), responds with an HTML
/// error fragment at the appropriate HTTP status.
pub async fn pause_action(
    State(session): State<AppState>,
    Path(hash): Path<String>,
) -> Response {
    let id = match crate::extractors::parse_info_hash(&hash) {
        Ok(id) => id,
        Err(e) => return api_error_fragment(e),
    };
    match session.pause_torrent(id).await {
        Ok(_) => refresh_response(),
        Err(e) => api_error_fragment(e.into()),
    }
}

/// `POST /webui/torrents/{hash}/resume`
///
/// Resume a paused torrent. Mirrors [`pause_action`] in response semantics.
pub async fn resume_action(
    State(session): State<AppState>,
    Path(hash): Path<String>,
) -> Response {
    let id = match crate::extractors::parse_info_hash(&hash) {
        Ok(id) => id,
        Err(e) => return api_error_fragment(e),
    };
    match session.resume_torrent(id).await {
        Ok(_) => refresh_response(),
        Err(e) => api_error_fragment(e.into()),
    }
}

/// `DELETE /webui/torrents/{hash}`
///
/// Remove a torrent from the session. Matches the v1 REST API's
/// `DELETE /api/v1/torrents/{hash}` semantics, but returns the HTMX
/// refresh header and an HTML error fragment on failure.
pub async fn delete_action(
    State(session): State<AppState>,
    Path(hash): Path<String>,
) -> Response {
    let id = match crate::extractors::parse_info_hash(&hash) {
        Ok(id) => id,
        Err(e) => return api_error_fragment(e),
    };
    match session.remove_torrent(id).await {
        Ok(_) => refresh_response(),
        Err(e) => api_error_fragment(e.into()),
    }
}

/// `GET /webui/fragments/settings`
///
/// Render the settings form fragment pre-populated with the current
/// session's values. The form PATCHes `/webui/settings` on submit
/// (handler in Task 7).
pub async fn settings_fragment(State(session): State<AppState>) -> Response {
    let s = match session.settings().await {
        Ok(s) => s,
        Err(e) => {
            return error_fragment(
                StatusCode::INTERNAL_SERVER_ERROR,
                &e.to_string(),
            );
        }
    };
    let tmpl = SettingsFormTemplate {
        listen_port: s.listen_port,
        download_dir: s.download_dir.to_string_lossy().into_owned(),
        max_torrents: s.max_torrents,
        max_peers_per_torrent: s.max_peers_per_torrent,
        download_rate_limit: s.download_rate_limit,
        upload_rate_limit: s.upload_rate_limit,
        active_downloads: s.active_downloads,
        active_seeds: s.active_seeds,
        enable_dht: s.enable_dht,
        enable_pex: s.enable_pex,
        enable_lsd: s.enable_lsd,
    };
    tmpl.into_web_template().into_response()
}

/// Query parameters for [`seed_mode_action`]. The button sends
/// `?enabled=true` or `?enabled=false` depending on the current flag.
#[derive(Deserialize)]
pub struct SeedModeQuery {
    pub enabled: bool,
}

/// Form body for [`patch_settings_webui`].
///
/// HTML checkbox inputs send their value only when checked — an unchecked
/// box is absent from the body entirely. We therefore receive checkboxes
/// as `Option<String>` and map presence to `true`.
#[derive(Deserialize)]
pub struct SettingsForm {
    pub listen_port: u16,
    pub download_dir: String,
    pub max_torrents: usize,
    pub max_peers_per_torrent: usize,
    pub download_rate_limit: u64,
    pub upload_rate_limit: u64,
    pub active_downloads: i32,
    pub active_seeds: i32,
    #[serde(default)]
    pub enable_dht: Option<String>,
    #[serde(default)]
    pub enable_pex: Option<String>,
    #[serde(default)]
    pub enable_lsd: Option<String>,
}

impl SettingsForm {
    fn into_patch(self) -> serde_json::Value {
        serde_json::json!({
            "listen_port": self.listen_port,
            "download_dir": self.download_dir,
            "max_torrents": self.max_torrents,
            "max_peers_per_torrent": self.max_peers_per_torrent,
            "download_rate_limit": self.download_rate_limit,
            "upload_rate_limit": self.upload_rate_limit,
            "active_downloads": self.active_downloads,
            "active_seeds": self.active_seeds,
            "enable_dht": self.enable_dht.is_some(),
            "enable_pex": self.enable_pex.is_some(),
            "enable_lsd": self.enable_lsd.is_some(),
        })
    }
}

/// `PATCH /webui/settings`
///
/// Apply a subset of the session's settings from the Web UI form using RFC
/// 7396 JSON Merge Patch. Emits `HX-Trigger: settingsSaved` on success so
/// the settings page can show a toast.
///
/// SECURITY: unauthenticated. Auth/CSRF deferred to M168 (qBt v2 auth
/// milestone) per the M166 engineering review.
pub async fn patch_settings_webui(
    State(session): State<AppState>,
    axum::Form(form): axum::Form<SettingsForm>,
) -> Response {
    let current = match session.settings().await {
        Ok(s) => s,
        Err(e) => return api_error_fragment(e.into()),
    };

    // Pipeline: current settings → JSON → merge with form patch → new JSON →
    // deserialize back to Settings → validate → apply.
    let mut target = match serde_json::to_value(&current) {
        Ok(v) => v,
        Err(e) => {
            return error_fragment(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to serialize current settings: {e}"),
            );
        }
    };
    let patch = form.into_patch();
    super::session::json_merge_patch(&mut target, &patch);

    let new_settings: irontide::session::Settings = match serde_json::from_value(target) {
        Ok(s) => s,
        Err(e) => return error_fragment(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    if let Err(e) = new_settings.validate() {
        return error_fragment(StatusCode::BAD_REQUEST, &e.to_string());
    }
    if let Err(e) = session.apply_settings(new_settings).await {
        return api_error_fragment(e.into());
    }

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "HX-Trigger",
        axum::http::HeaderValue::from_static("settingsSaved"),
    );
    (StatusCode::OK, headers, String::new()).into_response()
}

/// `POST /webui/torrents/{hash}/seed-mode?enabled=<bool>`
///
/// Flip the torrent's `user_seed_mode` flag. Emits `HX-Trigger: refreshList`
/// on success so the button's label and class swap between the two states.
pub async fn seed_mode_action(
    State(session): State<AppState>,
    Path(hash): Path<String>,
    Query(q): Query<SeedModeQuery>,
) -> Response {
    let id = match crate::extractors::parse_info_hash(&hash) {
        Ok(id) => id,
        Err(e) => return api_error_fragment(e),
    };
    match session.set_seed_mode(id, q.enabled).await {
        Ok(_) => refresh_response(),
        Err(e) => api_error_fragment(e.into()),
    }
}

// ---------------------------------------------------------------------------
// Detail-view handlers (M167)
// ---------------------------------------------------------------------------

/// `GET /webui/torrents/{hash}`
///
/// Render the full detail page for a single torrent. The Info tab is rendered
/// inline; Files / Trackers / Peers lazy-load via HTMX `hx-get` on their
/// tabpanel divs.
///
/// Returns 400 for malformed hashes, 404 when no torrent matches, and 200
/// with `text/html` otherwise. When metadata has not yet arrived
/// (`MetadataNotReady`), the page still renders with `metadata_pending=true`
/// so the user at least sees a state chip, a breadcrumb, and the info hash.
pub async fn torrent_detail(
    State(session): State<AppState>,
    Path(hash): Path<String>,
) -> Response {
    let id = match crate::extractors::parse_info_hash(&hash) {
        Ok(id) => id,
        Err(e) => return api_error_fragment(e),
    };

    // Stats tell us the torrent exists + its live state/rates. If this 404s
    // the torrent was never present (or was just removed); we surface that
    // unchanged so the client-side removed-banner handler can take over.
    let stats = match session.torrent_stats(id).await {
        Ok(s) => s,
        Err(e) => return api_error_fragment(e.into()),
    };

    let state_label = irontide_format::format_state(&stats.state, stats.user_seed_mode).to_owned();
    let state_class = state_css_class(&state_label).to_owned();
    let progress = stats.progress as f64;
    let progress_pct = format!("{:.1}%", progress * 100.0);

    let remaining = stats.total.saturating_sub(stats.total_done);
    let eta = irontide_format::format_eta(remaining, stats.download_rate).to_string();
    let ratio = irontide_format::format_ratio(stats.uploaded, stats.downloaded).to_string();

    let info_hash = id.to_hex();
    let info_hash_v2 = stats.info_hashes.v2.map(|v| v.to_hex());
    let name = stats.name.clone();

    // Info-tab fields come from torrent_info(). Degrade gracefully when
    // metadata has not arrived — the page still paints with the info hash
    // as the anchor identifier.
    let (metadata_pending, total_size, piece_length, num_pieces, private) =
        match session.torrent_info(id).await {
            Ok(info) => (
                false,
                irontide_format::format_size(info.total_length),
                irontide_format::format_size(info.piece_length),
                info.num_pieces.to_string(),
                info.private,
            ),
            Err(_) => (true, String::new(), String::new(), String::new(), false),
        };

    // Session-level default download dir — per-torrent dirs aren't exposed
    // on TorrentInfo in M167; good enough for the display.
    let download_path = match session.settings().await {
        Ok(s) => s.download_dir.to_string_lossy().into_owned(),
        Err(_) => String::from("(unknown)"),
    };

    let tmpl = TorrentDetailTemplate {
        info_hash,
        info_hash_v2,
        name,
        state: state_label,
        state_class,
        progress,
        progress_pct,
        down_rate: irontide_format::format_rate(stats.download_rate),
        up_rate: irontide_format::format_rate(stats.upload_rate),
        eta,
        ratio,
        metadata_pending,
        total_size,
        piece_length,
        num_pieces,
        private,
        download_path,
    };
    tmpl.into_web_template().into_response()
}

/// Fallback handler that serves static assets from the embedded
/// `irontide-webui-assets` crate.
///
/// Extension-less paths are mapped to their HTML equivalents:
/// `/` → `index.html`, `/settings` → `settings.html`. Unknown paths
/// return 404.
pub async fn serve_static(req: Request) -> impl IntoResponse {
    let raw = req.uri().path().trim_start_matches('/');
    let path = match raw {
        "" => "index.html",
        "settings" => "settings.html",
        other => other,
    };

    match irontide_webui_assets::get(path) {
        Some((content_type, data)) => Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            .body(axum::body::Body::from(data))
            .unwrap(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_template_escapes_hostile_name() {
        // Askama's default escaper protects every attacker-controlled field
        // rendered in HTML context. A regression here (e.g. a `|safe` slip)
        // would be silent without this test.
        let tmpl = TorrentDetailTemplate {
            info_hash: "aa".repeat(20),
            info_hash_v2: None,
            name: "<script>oops</script>".to_string(),
            state: "downloading".to_string(),
            state_class: "downloading".to_string(),
            progress: 0.5,
            progress_pct: "50.0%".to_string(),
            down_rate: "0 B/s".to_string(),
            up_rate: "0 B/s".to_string(),
            eta: "∞".to_string(),
            ratio: "0.00".to_string(),
            metadata_pending: false,
            total_size: "1 GB".to_string(),
            piece_length: "256 KB".to_string(),
            num_pieces: "4096".to_string(),
            private: false,
            download_path: "/tmp".to_string(),
        };
        let rendered = tmpl.render().expect("render detail template");
        assert!(
            !rendered.contains("<script>oops</script>"),
            "hostile name must be escaped: {rendered}"
        );
        // Askama 0.15's default HTML escaper uses numeric character references
        // (&#60; / &#62;) rather than named entities (&lt; / &gt;). Both are
        // valid HTML; the test asserts either form is present.
        let escaped_lt = rendered.contains("&lt;") || rendered.contains("&#60;");
        let escaped_gt = rendered.contains("&gt;") || rendered.contains("&#62;");
        assert!(
            escaped_lt && escaped_gt,
            "expected &lt;/&gt; or &#60;/&#62; in escaped output: {rendered}"
        );
    }

    #[test]
    fn torrent_row_carries_info_hash_and_state_flags() {
        // M166: per-row action buttons require the info hash (button target),
        // the paused flag (pause vs resume button selection), and the
        // user_seed_mode flag (seed-mode toggle label).
        let row = TorrentRow {
            name: "test.iso".to_string(),
            size: "1 GB".to_string(),
            progress: 0.0,
            progress_pct: "0.0%".to_string(),
            down_rate: "0 B/s".to_string(),
            up_rate: "0 B/s".to_string(),
            seeds: 0,
            peers: 0,
            state: "paused".to_string(),
            state_class: "paused".to_string(),
            info_hash: "aa".repeat(20),
            is_paused: true,
            user_seed_mode: false,
        };
        assert_eq!(row.info_hash.len(), 40);
        assert!(row.is_paused);
        assert!(!row.user_seed_mode);
    }
}
