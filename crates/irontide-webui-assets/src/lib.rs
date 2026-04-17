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
}
