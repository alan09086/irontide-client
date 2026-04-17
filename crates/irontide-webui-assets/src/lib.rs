//! Vendored static assets for the IronTide Web UI.
//!
//! Assets are compiled into the binary via `rust-embed` for zero-config
//! deployment. Call [`get`] to retrieve any asset by its relative path.

use mime_guess::MimeGuess;
use rust_embed::RustEmbed;

/// All files under `assets/` embedded into the binary at compile time.
#[derive(RustEmbed)]
#[folder = "assets/"]
struct Assets;

/// Retrieve an embedded asset by its path relative to the `assets/` directory.
///
/// Returns `Some((content_type, bytes))` when the file exists, or `None`
/// when the path is not found among the embedded files.
///
/// # Examples
///
/// ```
/// let (mime, bytes) = irontide_webui_assets::get("index.html").unwrap();
/// assert!(mime.starts_with("text/html"));
/// ```
pub fn get(path: &str) -> Option<(String, Vec<u8>)> {
    let file = Assets::get(path)?;
    let mime = MimeGuess::from_path(path)
        .first_or_octet_stream()
        .to_string();
    Some((mime, file.data.into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_html_embedded() {
        let result = get("index.html");
        assert!(result.is_some(), "index.html must be embedded");
        let (mime, bytes) = result.unwrap();
        assert!(
            mime.starts_with("text/html"),
            "expected text/html mime, got {mime}"
        );
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.starts_with("<!DOCTYPE"),
            "index.html must start with <!DOCTYPE, got: {:?}",
            &content[..content.len().min(20)]
        );
    }

    #[test]
    fn test_static_assets_present() {
        for path in [
            "js/htmx.min.js",
            "js/ws-live.js",
            "css/pico.min.css",
            "css/app.css",
            "settings.html",
        ] {
            assert!(
                get(path).is_some(),
                "expected embedded asset at {path} but it was not found"
            );
        }
    }

    #[test]
    fn test_index_links_settings_and_ws_live() {
        let (_mime, bytes) = get("index.html").expect("index.html embedded");
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains("href=\"/settings\""),
            "index.html must expose a nav link to /settings"
        );
        assert!(
            content.contains("js/ws-live.js"),
            "index.html must load js/ws-live.js"
        );
    }

    /// These assertions verify key behaviours by content substring — they
    /// will NOT catch a JavaScript syntax error. The authoritative check is
    /// the manual dogfooding smoke test (Task 9.5).
    #[test]
    fn test_ws_live_js_has_full_client() {
        let (_mime, bytes) = get("js/ws-live.js").expect("ws-live.js embedded");
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains("new WebSocket"),
            "ws-live.js must open a WebSocket connection"
        );
        assert!(
            content.contains("/api/v1/events"),
            "ws-live.js must target the /api/v1/events endpoint"
        );
        // C3 fix: filter to alerts to avoid refresh-on-every-heartbeat.
        assert!(
            content.contains("'alert'") || content.contains("\"alert\""),
            "ws-live.js must gate refreshList on alert messages, not stats"
        );
        // Trailing debounce to cap refresh rate at 1 Hz.
        assert!(
            content.contains("setTimeout") && content.contains("scheduleRefresh"),
            "ws-live.js must trailing-debounce refreshList dispatch"
        );
        // Exponential-backoff reconnect.
        assert!(
            content.contains("Math.min"),
            "ws-live.js must cap reconnect backoff (Math.min)"
        );
    }

    /// M167 additions: ws-live.js must export scheduleDetailRefresh,
    /// extractInfoHash, and setDetailPollCadence. Substring-check only —
    /// syntax errors are caught by manual dogfooding (Task 12).
    #[test]
    fn test_ws_live_js_has_detail_refresh_machinery() {
        let (_mime, bytes) = get("js/ws-live.js").expect("ws-live.js embedded");
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains("function scheduleDetailRefresh"),
            "ws-live.js must define scheduleDetailRefresh"
        );
        assert!(
            content.contains("function extractInfoHash"),
            "ws-live.js must define extractInfoHash"
        );
        assert!(
            content.contains("function setDetailPollCadence"),
            "ws-live.js must define setDetailPollCadence"
        );
        assert!(
            content.contains("refreshDetail"),
            "ws-live.js must reference the refreshDetail event name"
        );
        // setDetailPollCadence must call htmx.process on each panel so new
        // interval timers take effect — a silent regression here would
        // mean WS-down cadence never kicks in on the detail view.
        let start = content
            .find("function setDetailPollCadence")
            .expect("setDetailPollCadence present");
        // Look at the following ~800 chars; the function body is shorter.
        let window = &content[start..(start + 800).min(content.len())];
        assert!(
            window.contains("htmx.process"),
            "setDetailPollCadence must call htmx.process, got: {window}"
        );
        assert!(
            content.contains("toLowerCase"),
            "extractInfoHash must lowercase-normalize the hash"
        );
        assert!(
            content.contains("data-detail-hash"),
            "ws-live.js must consult body.data-detail-hash before dispatching"
        );
    }

    #[test]
    fn test_ws_live_js_toggles_polling_cadence() {
        let (_mime, bytes) = get("js/ws-live.js").expect("ws-live.js embedded");
        let content = String::from_utf8_lossy(&bytes);
        // Fast cadence (WS down) and slow cadence (WS up) must both appear.
        assert!(
            content.contains("every 2s"),
            "ws-live.js must reference the fast (2s) polling cadence"
        );
        assert!(
            content.contains("every 30s"),
            "ws-live.js must reference the slow (30s) polling cadence"
        );
        // The swap is done through hx-trigger on the torrent-list element,
        // and the element must be re-processed by HTMX afterwards.
        assert!(
            content.contains("torrent-list"),
            "ws-live.js must target the torrent-list element"
        );
        assert!(
            content.contains("htmx.process") || content.contains("hxProcess"),
            "ws-live.js must re-run htmx.process after swapping hx-trigger"
        );
    }
}
