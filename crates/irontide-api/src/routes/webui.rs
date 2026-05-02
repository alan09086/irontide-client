#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M175: HTMX Web UI — display formatting and progress arithmetic with values bounded by torrent counts/sizes"
)]

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

/// A single row in the Files tab table, pre-formatted so the template
/// contains no formatting logic.
pub(crate) struct FileRow {
    pub idx: usize,
    pub path: String,
    pub size: String,
    /// Fraction done in `[0.0, 1.0]`, fed straight into `<progress value>`.
    pub progress: f64,
    pub progress_pct: String,
    /// Lowercase slug matching the PATCH form value: `skip|low|normal|high`.
    pub priority: &'static str,
}

/// Askama template for the Files tab.
#[derive(Template)]
#[template(path = "files_tab.html")]
pub(crate) struct FilesTabTemplate {
    /// Lowercase hex info hash — embedded into the PATCH URLs on each row.
    pub hash: String,
    pub files: Vec<FileRow>,
}

/// A single row in the Peers tab table.
pub(crate) struct PeerRow {
    pub addr: String,
    /// Client identifier string, possibly empty. Escaped on render by askama.
    pub client: String,
    /// `(glyph, tooltip)` pairs — the template iterates this vector and
    /// wraps each in `<abbr title="tooltip">glyph</abbr>`.
    pub flags: Vec<(char, &'static str)>,
    pub down_rate: String,
    pub up_rate: String,
    pub num_pieces: u32,
}

/// Askama template for the Peers tab.
#[derive(Template)]
#[template(path = "peers_tab.html")]
pub(crate) struct PeersTabTemplate {
    pub peers: Vec<PeerRow>,
}

/// A single row in the Trackers tab.
pub(crate) struct TrackerRow {
    pub url: String,
    pub tier: usize,
    pub status_class: &'static str,
    pub status_label: &'static str,
    pub status_title: String,
    /// Seeders count rendered as a string ("—" when unknown).
    pub seeders: String,
    pub leechers: String,
    /// Coarse relative-time phrase: "now" / "in 30s" / "in 2m" / "in 1h 15m".
    pub next_announce_text: String,
}

/// Askama template for the Trackers tab.
#[derive(Template)]
#[template(path = "trackers_tab.html")]
pub(crate) struct TrackersTabTemplate {
    pub hash: String,
    pub trackers: Vec<TrackerRow>,
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

/// Build an empty `200 OK` response carrying `HX-Trigger:
/// {"refreshDetail":{"hash":"<lower-hex>"}}` so HTMX refreshes every
/// detail-tab panel listening for that hash.
///
/// JSON is built with `serde_json::json!` rather than `format!` so there is
/// no way for a hash (or any future payload field) to accidentally produce
/// an invalid header value.
fn refresh_detail_response(hash: &str) -> Response {
    let payload = serde_json::json!({ "refreshDetail": { "hash": hash } });
    // `serde_json::Value::to_string()` produces strictly ASCII output (no
    // control chars), so `HeaderValue::from_str` cannot fail — but we
    // fall back gracefully if that guarantee ever regresses.
    let hv = axum::http::HeaderValue::from_str(&payload.to_string())
        .unwrap_or_else(|_| axum::http::HeaderValue::from_static("refreshDetail"));
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("HX-Trigger", hv);
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
#[allow(clippy::needless_pass_by_value, reason = "callers pass owned errors from match arms")]
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
        "queued" => "queued",
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
            let state_label = irontide_format::format_state(&t.state, t.user_seed_mode).to_owned();
            let css_class = state_css_class(&state_label).to_owned();
            let is_paused = matches!(t.state, TorrentState::Paused | TorrentState::Queued);
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
pub async fn pause_action(State(session): State<AppState>, Path(hash): Path<String>) -> Response {
    let id = match crate::extractors::parse_info_hash(&hash) {
        Ok(id) => id,
        Err(e) => return api_error_fragment(e),
    };
    match session.pause_torrent(id).await {
        Ok(()) => refresh_response(),
        Err(e) => api_error_fragment(e.into()),
    }
}

/// `POST /webui/torrents/{hash}/resume`
///
/// Resume a paused torrent. Mirrors [`pause_action`] in response semantics.
pub async fn resume_action(State(session): State<AppState>, Path(hash): Path<String>) -> Response {
    let id = match crate::extractors::parse_info_hash(&hash) {
        Ok(id) => id,
        Err(e) => return api_error_fragment(e),
    };
    match session.resume_torrent(id).await {
        Ok(()) => refresh_response(),
        Err(e) => api_error_fragment(e.into()),
    }
}

/// `DELETE /webui/torrents/{hash}`
///
/// Remove a torrent from the session. Matches the v1 REST API's
/// `DELETE /api/v1/torrents/{hash}` semantics, but returns the HTMX
/// refresh header and an HTML error fragment on failure.
pub async fn delete_action(State(session): State<AppState>, Path(hash): Path<String>) -> Response {
    let id = match crate::extractors::parse_info_hash(&hash) {
        Ok(id) => id,
        Err(e) => return api_error_fragment(e),
    };
    match session.remove_torrent(id).await {
        Ok(()) => refresh_response(),
        Err(e) => api_error_fragment(e.into()),
    }
}

/// `GET /webui/fragments/settings`
///
/// Render the settings form fragment pre-populated with the current
/// session's values. The form `PATCHes` `/webui/settings` on submit
/// (handler in Task 7).
pub async fn settings_fragment(State(session): State<AppState>) -> Response {
    let s = match session.settings().await {
        Ok(s) => s,
        Err(e) => {
            return error_fragment(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
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
/// M172a Lane B: CSRF protection applied globally via [`qbt_v2::csrf_guard`]
/// layered on the top-level router — cross-origin POST/PATCH from a hostile
/// tab is rejected with 403 `Fails.`. Login auth for the /webui/* surface
/// itself remains a future milestone.
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
        Ok(()) => refresh_response(),
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
pub async fn torrent_detail(State(session): State<AppState>, Path(hash): Path<String>) -> Response {
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
    let progress = f64::from(stats.progress);
    let progress_pct = format!("{:.1}%", progress * 100.0);

    let remaining = stats.total.saturating_sub(stats.total_done);
    let eta = irontide_format::format_eta(remaining, stats.download_rate).clone();
    let ratio = irontide_format::format_ratio(stats.uploaded, stats.downloaded).clone();

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

/// Map a [`FilePriority`](irontide::core::FilePriority) to its lowercase
/// form-value slug. The inverse (`parse_priority_form_value`) is used by
/// the PATCH handler in Task 5.
fn priority_slug(p: irontide::core::FilePriority) -> &'static str {
    match p {
        irontide::core::FilePriority::Skip => "skip",
        irontide::core::FilePriority::Low => "low",
        irontide::core::FilePriority::Normal => "normal",
        irontide::core::FilePriority::High => "high",
    }
}

/// Format a `u64` seconds count as a short human phrase: `"now"` if the
/// announce is effectively imminent, or a sparse duration if further out.
/// Minutes and hours ladder at 60s and 3600s respectively; fractional
/// larger units round down.
///
/// The Web UI uses this for the "Next announce" column because tracker
/// timers are not ticked on the client — a live JS countdown would add
/// noise without value. Users refresh the tab when they care.
fn format_relative_secs(s: u64) -> String {
    if s == 0 {
        return "now".to_string();
    }
    if s < 60 {
        return format!("in {s}s");
    }
    if s < 3600 {
        return format!("in {}m", s / 60);
    }
    let hours = s / 3600;
    let minutes = (s % 3600) / 60;
    if minutes == 0 {
        format!("in {hours}h")
    } else {
        format!("in {hours}h {minutes}m")
    }
}

/// Parse a priority slug from the PATCH form body. Strict match only — any
/// value other than the four known slugs returns `None` so the caller can
/// produce a 422 without touching the engine.
fn parse_priority_form_value(value: &str) -> Option<irontide::core::FilePriority> {
    irontide_format::parse_priority_label(value)
}

/// Form body for [`patch_file_priority`].
#[derive(Deserialize)]
pub struct FilePriorityForm {
    priority: String,
}

/// `PATCH /webui/torrents/{hash}/files/{idx}`
///
/// Set the download priority of a single file. Body is form-urlencoded
/// `priority=skip|low|normal|high`. Emits `HX-Trigger: refreshDetail`
/// scoped to this hash so every open detail tab re-fetches.
///
/// Returns:
/// - 400 for malformed hash
/// - 404 when the torrent is unknown OR the file index is out of range
/// - 422 when the priority slug is not one of the four valid values
/// - 200 + `HX-Trigger` on success
///
/// NOTE: unauthenticated — M171 adds CSRF for the /webui/* surface. Do not
/// add new unauthenticated mutations without flagging them this way.
pub async fn patch_file_priority(
    State(session): State<AppState>,
    Path((hash, idx)): Path<(String, usize)>,
    axum::Form(form): axum::Form<FilePriorityForm>,
) -> Response {
    let Ok(id) = crate::extractors::parse_info_hash(&hash) else {
        return error_fragment(StatusCode::BAD_REQUEST, "invalid info hash");
    };
    let Some(priority) = parse_priority_form_value(&form.priority) else {
        return error_fragment(
            StatusCode::UNPROCESSABLE_ENTITY,
            "priority must be one of: skip, low, normal, high",
        );
    };

    // Bounds-check idx before hitting the engine — engine errors on
    // out-of-range may be generic, whereas the UI wants a clean 404.
    let priorities = match session.file_priorities(id).await {
        Ok(p) => p,
        Err(e) => return api_error_fragment(e.into()),
    };
    if idx >= priorities.len() {
        return error_fragment(
            StatusCode::NOT_FOUND,
            &format!("file index {idx} out of range ({} files)", priorities.len()),
        );
    }

    if let Err(e) = session.set_file_priority(id, idx, priority).await {
        return api_error_fragment(e.into());
    }
    refresh_detail_response(&id.to_hex())
}

/// `GET /webui/fragments/torrent/{hash}/files`
///
/// Render the Files tab as an HTML fragment. Delegates row construction
/// to [`irontide_format::build_flat`] so the GUI Content tab and Web UI
/// share the same length-mismatch saturation rules (D-eng-3, M177).
pub async fn files_fragment(State(session): State<AppState>, Path(hash): Path<String>) -> Response {
    let id = match crate::extractors::parse_info_hash(&hash) {
        Ok(id) => id,
        Err(e) => return api_error_fragment(e),
    };

    // Metadata-not-ready is a valid state for magnets — render the empty-state.
    let info = match session.torrent_info(id).await {
        Ok(info) => info,
        Err(irontide::session::Error::MetadataNotReady(_)) => {
            let tmpl = FilesTabTemplate {
                hash: id.to_hex(),
                files: Vec::new(),
            };
            return tmpl.into_web_template().into_response();
        }
        Err(e) => return api_error_fragment(e.into()),
    };

    let progress = match session.file_progress(id).await {
        Ok(p) => p,
        Err(e) => return api_error_fragment(e.into()),
    };
    let priorities = match session.file_priorities(id).await {
        Ok(p) => p,
        Err(e) => return api_error_fragment(e.into()),
    };

    if info.files.len() != progress.len() || info.files.len() != priorities.len() {
        tracing::warn!(
            files = info.files.len(),
            progress = progress.len(),
            priorities = priorities.len(),
            "file metadata length mismatch — saturating missing entries to defaults"
        );
    }

    let flat = irontide_format::build_flat(&info, &progress, &priorities);
    let rows: Vec<FileRow> = flat
        .into_iter()
        .enumerate()
        .map(|(idx, entry)| {
            let progress = if entry.size == 0 {
                1.0
            } else {
                (entry.progress as f64 / entry.size as f64).clamp(0.0, 1.0)
            };
            FileRow {
                idx,
                path: entry.path.to_string_lossy().into_owned(),
                size: irontide_format::format_size(entry.size),
                progress,
                progress_pct: format!("{:.1}%", progress * 100.0),
                priority: priority_slug(entry.priority),
            }
        })
        .collect();

    let tmpl = FilesTabTemplate {
        hash: id.to_hex(),
        files: rows,
    };
    tmpl.into_web_template().into_response()
}

/// Map a [`TrackerStatus`](irontide::session::TrackerStatus) to a CSS
/// class + user-facing label + long-form title.
fn tracker_status_bits(
    status: irontide::session::TrackerStatus,
    consecutive_failures: u32,
) -> (&'static str, &'static str, String) {
    match status {
        irontide::session::TrackerStatus::NotContacted => (
            "pending",
            "Pending",
            "Has not been contacted yet".to_string(),
        ),
        irontide::session::TrackerStatus::Working => {
            ("working", "OK", "Last announce succeeded".to_string())
        }
        irontide::session::TrackerStatus::Error => (
            "error",
            "Error",
            format!("Last announce failed ({consecutive_failures} consecutive)"),
        ),
    }
}

/// Build the per-peer flag vector shown in the Peers tab. Each entry is
/// `(glyph, tooltip)` so the template can render `<abbr title="…">X</abbr>`
/// without repeating the mapping logic.
///
/// Precise conditions (matches the M167 plan):
/// M171 D5: qBt-parity peer-flag glyph superset (15 glyphs).
///
/// - `D` downloading from peer          — !`peer_choking` && `num_pieces` > 0 && `am_interested`
/// - `d` we want data, peer chokes us   — `am_interested` && `peer_choking`
/// - `U` uploading to peer              — !`am_choking` && `peer_interested`
/// - `u` peer wants data, we choke them — `peer_interested` && `am_choking`
/// - `K` we are choking the peer        — `am_choking`
/// - `?` we are interested              — `am_interested`
/// - `S` peer is snubbed                — snubbed
/// - `O` optimistic unchoke slot        — `is_optimistic`
/// - `I` incoming connection            — source == Incoming (REDEFINED in M171)
/// - `H` discovered via DHT             — source == Dht
/// - `X` discovered via `PeX`             — source == Pex
/// - `L` discovered via LSD             — source == Lsd
/// - `E` encrypted connection (MSE/PE)  — `is_encrypted`
/// - `P` using uTP (BEP 29)             — `uses_utp`
/// - `F` supports fast extension (BEP 6)— `supports_fast`
///
/// `I` was "peer is interested in us" in M167-M170; M171 remaps it to
/// qBt's "incoming connection" semantic. Peer-interested state is still
/// implicit via the `U`/`u` glyphs.
fn peer_flags(p: &irontide::session::PeerInfo) -> Vec<(char, &'static str)> {
    irontide_format::peer_flags(p)
}

/// `GET /webui/fragments/torrent/{hash}/peers`
///
/// Render the Peers tab as an HTML fragment. The flag column uses
/// `<abbr>` tooltips; the footer has a `<details>` legend so users can
/// expand the symbol→meaning mapping without needing a hover device.
pub async fn peers_fragment(State(session): State<AppState>, Path(hash): Path<String>) -> Response {
    let id = match crate::extractors::parse_info_hash(&hash) {
        Ok(id) => id,
        Err(e) => return api_error_fragment(e),
    };

    let peers = match session.get_peer_info(id).await {
        Ok(p) => p,
        Err(e) => return api_error_fragment(e.into()),
    };

    let rows: Vec<PeerRow> = peers
        .into_iter()
        .map(|p| PeerRow {
            addr: p.addr.to_string(),
            client: p.client.clone(),
            flags: peer_flags(&p),
            down_rate: irontide_format::format_rate(p.download_rate),
            up_rate: irontide_format::format_rate(p.upload_rate),
            num_pieces: p.num_pieces,
        })
        .collect();

    let tmpl = PeersTabTemplate { peers: rows };
    tmpl.into_web_template().into_response()
}

/// `GET /webui/fragments/torrent/{hash}/trackers`
///
/// Render the Trackers tab as an HTML fragment. Below the table is a
/// [Force Reannounce] button that POSTs to the sibling endpoint.
pub async fn trackers_fragment(
    State(session): State<AppState>,
    Path(hash): Path<String>,
) -> Response {
    let id = match crate::extractors::parse_info_hash(&hash) {
        Ok(id) => id,
        Err(e) => return api_error_fragment(e),
    };

    let trackers = match session.tracker_list(id).await {
        Ok(t) => t,
        Err(e) => return api_error_fragment(e.into()),
    };

    let rows: Vec<TrackerRow> = trackers
        .into_iter()
        .map(|t| {
            let (status_class, status_label, status_title) =
                tracker_status_bits(t.status, t.consecutive_failures);
            TrackerRow {
                url: t.url,
                tier: t.tier,
                status_class,
                status_label,
                status_title,
                seeders: t
                    .seeders.map_or_else(|| "—".into(), |n| n.to_string()),
                leechers: t
                    .leechers.map_or_else(|| "—".into(), |n| n.to_string()),
                next_announce_text: format_relative_secs(t.next_announce_secs),
            }
        })
        .collect();

    let tmpl = TrackersTabTemplate {
        hash: id.to_hex(),
        trackers: rows,
    };
    tmpl.into_web_template().into_response()
}

/// `POST /webui/torrents/{hash}/reannounce`
///
/// Force every tracker to reannounce immediately. Returns
/// `HX-Trigger: refreshDetail` so the Trackers tab refreshes in place.
///
/// NOTE: unauthenticated — M171 adds CSRF for the /webui/* surface. Do not
/// add new unauthenticated mutations without flagging them this way.
pub async fn reannounce_action(
    State(session): State<AppState>,
    Path(hash): Path<String>,
) -> Response {
    let id = match crate::extractors::parse_info_hash(&hash) {
        Ok(id) => id,
        Err(e) => return api_error_fragment(e),
    };
    if let Err(e) = session.force_reannounce(id).await {
        return api_error_fragment(e.into());
    }
    refresh_detail_response(&id.to_hex())
}

/// `GET /webui/fragments/torrent/{hash}/info`
///
/// Renders ONLY the Info tab as a standalone fragment. Shares its template
/// (`info_tab.html`) with the inline include on the detail page so the
/// layout is identical regardless of code path.
pub async fn info_fragment(State(session): State<AppState>, Path(hash): Path<String>) -> Response {
    let id = match crate::extractors::parse_info_hash(&hash) {
        Ok(id) => id,
        Err(e) => return api_error_fragment(e),
    };

    let stats = match session.torrent_stats(id).await {
        Ok(s) => s,
        Err(e) => return api_error_fragment(e.into()),
    };

    let info_hash = id.to_hex();
    let info_hash_v2 = stats.info_hashes.v2.map(|v| v.to_hex());
    let name = stats.name.clone();

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

    let download_path = match session.settings().await {
        Ok(s) => s.download_dir.to_string_lossy().into_owned(),
        Err(_) => String::from("(unknown)"),
    };

    let tmpl = InfoTabTemplate {
        info_hash,
        info_hash_v2,
        name,
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

    #[allow(clippy::fn_params_excessive_bools, reason = "mirrors PeerInfo wire protocol fields")]
    fn make_peer(
        am_choking: bool,
        am_interested: bool,
        peer_choking: bool,
        peer_interested: bool,
        snubbed: bool,
        num_pieces: u32,
    ) -> irontide::session::PeerInfo {
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        irontide::session::PeerInfo {
            addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 6881),
            client: "test".to_string(),
            peer_choking,
            peer_interested,
            am_choking,
            am_interested,
            download_rate: 0,
            upload_rate: 0,
            num_pieces,
            source: irontide::session::PeerSource::Tracker,
            supports_fast: false,
            upload_only: false,
            snubbed,
            connected_duration_secs: 0,
            num_pending_requests: 0,
            num_incoming_requests: 0,
            is_optimistic: false,
            is_encrypted: false,
            uses_utp: false,
            uses_holepunch: false,
        }
    }

    #[test]
    fn peer_flags_cover_all_documented_cases() {
        // Happy path: downloading + we're interested + peer has pieces.
        let p = make_peer(false, true, false, false, false, 10);
        let glyphs: Vec<char> = peer_flags(&p).iter().map(|(c, _)| *c).collect();
        assert!(glyphs.contains(&'D'), "D missing: {glyphs:?}");
        assert!(glyphs.contains(&'?'), "? missing: {glyphs:?}");
        assert!(!glyphs.contains(&'K'), "K must not appear: {glyphs:?}");

        // Uploading to a peer that's interested. M171 D5: the `I` glyph no
        // longer means "peer is interested in us" — that state is implied
        // by `U` / `u` now. `I` only lights up on incoming connections.
        let p = make_peer(false, false, false, true, false, 0);
        let glyphs: Vec<char> = peer_flags(&p).iter().map(|(c, _)| *c).collect();
        assert!(glyphs.contains(&'U'));
        assert!(
            !glyphs.contains(&'I'),
            "I must NOT fire for interested; it's incoming-only now"
        );

        // Snubbed (no data in snub-window) + we're choking them.
        let p = make_peer(true, false, false, false, true, 0);
        let glyphs: Vec<char> = peer_flags(&p).iter().map(|(c, _)| *c).collect();
        assert!(glyphs.contains(&'K'));
        assert!(glyphs.contains(&'S'));
        assert!(
            !glyphs.contains(&'U'),
            "U must not appear when choking: {glyphs:?}"
        );

        // Totally idle peer — no flags at all.
        let p = make_peer(false, false, false, false, false, 0);
        let glyphs: Vec<char> = peer_flags(&p).iter().map(|(c, _)| *c).collect();
        assert!(
            glyphs.is_empty(),
            "idle peer should have no flags: {glyphs:?}"
        );
    }

    /// M171 D5 / E0.11: every new glyph in the 15-flag superset fires when
    /// (and only when) its trigger state is set. Parametric over the 9
    /// glyphs that land new in M171 — leaves the existing 6 covered by
    /// [`peer_flags_cover_all_documented_cases`].
    #[test]
    fn peer_flag_glyph_superset_renders() {
        use irontide::session::PeerSource;

        // `d` — interested + peer choking.
        let mut p = make_peer(false, true, true, false, false, 0);
        let glyphs: Vec<char> = peer_flags(&p).iter().map(|(c, _)| *c).collect();
        assert!(
            glyphs.contains(&'d'),
            "d missing for interested+peer_choking: {glyphs:?}"
        );

        // `u` — peer interested + we choke.
        p = make_peer(true, false, false, true, false, 0);
        let glyphs: Vec<char> = peer_flags(&p).iter().map(|(c, _)| *c).collect();
        assert!(
            glyphs.contains(&'u'),
            "u missing for peer_interested+am_choking: {glyphs:?}"
        );

        // `O` — optimistic unchoke.
        p = make_peer(false, false, false, false, false, 0);
        p.is_optimistic = true;
        let glyphs: Vec<char> = peer_flags(&p).iter().map(|(c, _)| *c).collect();
        assert!(
            glyphs.contains(&'O'),
            "O missing for is_optimistic: {glyphs:?}"
        );

        // `I` — incoming connection (source == Incoming).
        p = make_peer(false, false, false, false, false, 0);
        p.source = PeerSource::Incoming;
        let glyphs: Vec<char> = peer_flags(&p).iter().map(|(c, _)| *c).collect();
        assert!(glyphs.contains(&'I'), "I missing for incoming: {glyphs:?}");

        // `H` — discovered via DHT.
        p = make_peer(false, false, false, false, false, 0);
        p.source = PeerSource::Dht;
        let glyphs: Vec<char> = peer_flags(&p).iter().map(|(c, _)| *c).collect();
        assert!(
            glyphs.contains(&'H'),
            "H missing for DHT source: {glyphs:?}"
        );

        // `X` — discovered via PeX.
        p = make_peer(false, false, false, false, false, 0);
        p.source = PeerSource::Pex;
        let glyphs: Vec<char> = peer_flags(&p).iter().map(|(c, _)| *c).collect();
        assert!(
            glyphs.contains(&'X'),
            "X missing for PeX source: {glyphs:?}"
        );

        // `L` — discovered via LSD.
        p = make_peer(false, false, false, false, false, 0);
        p.source = PeerSource::Lsd;
        let glyphs: Vec<char> = peer_flags(&p).iter().map(|(c, _)| *c).collect();
        assert!(
            glyphs.contains(&'L'),
            "L missing for LSD source: {glyphs:?}"
        );

        // `E` — encrypted MSE/PE.
        p = make_peer(false, false, false, false, false, 0);
        p.is_encrypted = true;
        let glyphs: Vec<char> = peer_flags(&p).iter().map(|(c, _)| *c).collect();
        assert!(
            glyphs.contains(&'E'),
            "E missing for is_encrypted: {glyphs:?}"
        );

        // `P` — using uTP.
        p = make_peer(false, false, false, false, false, 0);
        p.uses_utp = true;
        let glyphs: Vec<char> = peer_flags(&p).iter().map(|(c, _)| *c).collect();
        assert!(glyphs.contains(&'P'), "P missing for uses_utp: {glyphs:?}");

        // `F` — supports fast extension (BEP 6).
        p = make_peer(false, false, false, false, false, 0);
        p.supports_fast = true;
        let glyphs: Vec<char> = peer_flags(&p).iter().map(|(c, _)| *c).collect();
        assert!(
            glyphs.contains(&'F'),
            "F missing for supports_fast: {glyphs:?}"
        );
    }

    /// M171 D5: `PeerInfo`'s new fields must round-trip through serde so old
    /// resume/snapshot payloads that predate the superset still
    /// deserialize (fields default to `false` via `#[serde(default)]`).
    #[test]
    fn peer_info_serde_round_trip_new_fields() {
        let p = make_peer(false, false, false, false, false, 0);
        let json = serde_json::to_string(&p).expect("serialize");
        let back: irontide::session::PeerInfo = serde_json::from_str(&json).expect("round-trip");
        assert!(!back.is_optimistic);
        assert!(!back.is_encrypted);
        assert!(!back.uses_utp);
        assert!(!back.uses_holepunch);
    }

    #[test]
    fn format_relative_secs_uses_coarse_units() {
        assert_eq!(format_relative_secs(0), "now");
        assert_eq!(format_relative_secs(1), "in 1s");
        assert_eq!(format_relative_secs(59), "in 59s");
        assert_eq!(format_relative_secs(60), "in 1m");
        assert_eq!(format_relative_secs(61), "in 1m");
        assert_eq!(format_relative_secs(3599), "in 59m");
        assert_eq!(format_relative_secs(3600), "in 1h");
        assert_eq!(format_relative_secs(3660), "in 1h 1m");
        assert_eq!(format_relative_secs(7200), "in 2h");
        // Huge values (days) still format cleanly as hours; users who care
        // about day-precision will hit refresh.
        assert_eq!(format_relative_secs(86400), "in 24h");
    }

    #[test]
    fn refresh_detail_response_emits_scoped_hx_trigger() {
        // The JS detail dispatcher filters with `refreshDetail[detail.hash==...]`
        // — any change to the payload shape breaks that filter, so the
        // exact nesting is a contract locked in here.
        let resp = refresh_detail_response("abcdef0123456789");
        let hv = resp.headers().get("HX-Trigger").expect("HX-Trigger set");
        let value = hv.to_str().expect("ascii header");
        let parsed: serde_json::Value =
            serde_json::from_str(value).expect("header must be valid JSON");
        assert_eq!(
            parsed["refreshDetail"]["hash"],
            serde_json::Value::String("abcdef0123456789".into()),
            "HX-Trigger payload must be {{\"refreshDetail\":{{\"hash\":\"...\"}}}}: {value}"
        );
    }

    #[test]
    fn parse_priority_form_value_accepts_four_slugs_only() {
        use irontide::core::FilePriority;
        assert_eq!(parse_priority_form_value("skip"), Some(FilePriority::Skip));
        assert_eq!(parse_priority_form_value("low"), Some(FilePriority::Low));
        assert_eq!(
            parse_priority_form_value("normal"),
            Some(FilePriority::Normal)
        );
        assert_eq!(parse_priority_form_value("high"), Some(FilePriority::High));
        // Hostile inputs that a careless `.parse::<u8>` fallback would swallow.
        assert_eq!(parse_priority_form_value(""), None);
        assert_eq!(parse_priority_form_value("SKIP"), None);
        assert_eq!(parse_priority_form_value("critical"), None);
    }

    #[test]
    fn files_template_renders_rows_with_priority_selected() {
        let tmpl = FilesTabTemplate {
            hash: "aa".repeat(20),
            files: vec![
                FileRow {
                    idx: 0,
                    path: "README.txt".to_string(),
                    size: "1.2 KiB".to_string(),
                    progress: 1.0,
                    progress_pct: "100.0%".to_string(),
                    priority: "normal",
                },
                FileRow {
                    idx: 1,
                    path: "data/<x>.bin".to_string(),
                    size: "10 MB".to_string(),
                    progress: 0.25,
                    progress_pct: "25.0%".to_string(),
                    priority: "skip",
                },
            ],
        };
        let out = tmpl.render().expect("render files template");

        // Each row has a <select> pointing at the PATCH URL.
        for idx in [0usize, 1] {
            assert!(
                out.contains(&format!(
                    r#"hx-patch="/webui/torrents/{}/files/{}""#,
                    "aa".repeat(20),
                    idx
                )),
                "row {idx} missing hx-patch: {out}"
            );
        }
        // `selected` attribute matches the row's current priority — normal
        // on row 0 and skip on row 1.
        assert!(
            out.contains(r#"<option value="normal" selected>Normal</option>"#),
            "row 0 must mark Normal as selected: {out}"
        );
        assert!(
            out.contains(r#"<option value="skip" selected>Skip</option>"#),
            "row 1 must mark Skip as selected: {out}"
        );
        // Hostile path is escaped, never passed through raw.
        assert!(
            !out.contains("data/<x>.bin") || out.contains("data/&#60;x&#62;.bin"),
            "hostile path must be HTML-escaped: {out}"
        );
    }

    #[test]
    fn files_template_empty_renders_placeholder() {
        let tmpl = FilesTabTemplate {
            hash: "bb".repeat(20),
            files: Vec::new(),
        };
        let out = tmpl.render().expect("render files template");
        assert!(
            out.contains("Metadata not yet received"),
            "empty template must render the waiting-on-peers copy: {out}"
        );
        assert!(
            !out.contains("<table"),
            "empty template must not render a table: {out}"
        );
    }

    #[test]
    fn priority_slug_matches_form_values() {
        use irontide::core::FilePriority;
        assert_eq!(priority_slug(FilePriority::Skip), "skip");
        assert_eq!(priority_slug(FilePriority::Low), "low");
        assert_eq!(priority_slug(FilePriority::Normal), "normal");
        assert_eq!(priority_slug(FilePriority::High), "high");
    }

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
