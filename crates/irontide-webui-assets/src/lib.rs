//! Vendored static assets for the `IronTide` Web UI.
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
#[must_use]
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
            "js/col-resize.js",
            "js/theme.js",
            "css/pico.min.css",
            "css/tokens.css",
            "css/app.css",
            "settings.html",
        ] {
            assert!(
                get(path).is_some(),
                "expected embedded asset at {path} but it was not found"
            );
        }
    }

    /// M234 — tokens.css must declare both light and dark theme blocks
    /// keyed by the `data-theme` attribute, plus a `:root` block of
    /// shared status colours. A regression here would mean the browser
    /// can't render the active theme correctly.
    #[test]
    fn test_tokens_css_has_dark_and_light_blocks() {
        let (mime, bytes) = get("css/tokens.css").expect("tokens.css embedded");
        assert!(
            mime.starts_with("text/css"),
            "expected text/css mime, got {mime}"
        );
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains(":root[data-theme=\"dark\"]"),
            "tokens.css must contain :root[data-theme=\"dark\"] block"
        );
        assert!(
            content.contains(":root[data-theme=\"light\"]"),
            "tokens.css must contain :root[data-theme=\"light\"] block"
        );
        // Sanity: a few representative tokens must be present in both.
        for token in ["--bg-0", "--fg-0", "--accent"] {
            let count = content.matches(token).count();
            assert!(
                count >= 2,
                "tokens.css must declare {token} in both theme blocks \
                 (found {count} occurrence(s))"
            );
        }
        // Shared status tokens live in :root (not nested under a theme).
        assert!(
            content.contains("--status-downloading"),
            "tokens.css must declare --status-downloading at :root scope"
        );
    }

    /// M234 — both HTML pages must <link> the codegen'd tokens.css
    /// after pico.min.css so :root[data-theme] declarations win the
    /// cascade where they collide with Pico's own theme block.
    #[test]
    fn test_index_links_tokens_css() {
        let (_mime, bytes) = get("index.html").expect("index.html embedded");
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains("css/tokens.css"),
            "index.html must <link> /webui/static/css/tokens.css"
        );
        // FOUC-prevention boot snippet must read the localStorage key
        // before stylesheets paint.
        assert!(
            content.contains("irontide.webui.theme"),
            "index.html must contain the FOUC-prevention boot snippet \
             that reads localStorage.irontide.webui.theme"
        );
    }

    /// M234 — theme.js exposes the public surface and writes through
    /// localStorage. Substring assertion only; syntax errors are
    /// caught by manual dogfooding.
    #[test]
    fn test_theme_js_has_localstorage_and_public_api() {
        let (_mime, bytes) = get("js/theme.js").expect("theme.js embedded");
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains("localStorage"),
            "theme.js must persist preference via localStorage"
        );
        assert!(
            content.contains("irontide.webui.theme"),
            "theme.js must use the canonical storage key"
        );
        assert!(
            content.contains("window.irontideTheme"),
            "theme.js must expose window.irontideTheme"
        );
        // Toggle and set are the load-bearing entry points used by the
        // header button and the preferences <select>.
        for fn_name in ["toggle", "set:", "get:"] {
            assert!(
                content.contains(fn_name),
                "theme.js must expose {fn_name} on window.irontideTheme"
            );
        }
        // prefers-color-scheme listener must be wired for "auto" mode.
        assert!(
            content.contains("prefers-color-scheme"),
            "theme.js must listen for prefers-color-scheme changes"
        );
    }

    #[test]
    fn test_index_links_settings_and_ws_live() {
        let (_mime, bytes) = get("index.html").expect("index.html embedded");
        let content = String::from_utf8_lossy(&bytes);
        // M232 moved the Settings page to `/webui/preferences` (8-tab full
        // Preferences). The legacy `/settings` URL still resolves via a
        // meta-refresh redirect, but the nav points at the canonical target.
        assert!(
            content.contains("href=\"/webui/preferences\""),
            "index.html must expose a nav link to /webui/preferences"
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

    /// M235 — app.css must define `.col-resize-handle` with hover state and
    /// a 375px card-layout `@media` block keyed to `data-label`. Substring
    /// assertion only; visual correctness is verified in mobile dogfood.
    #[test]
    fn test_app_css_has_m235_col_resize_handle_and_card_layout() {
        let (_mime, bytes) = get("css/app.css").expect("app.css embedded");
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains(".col-resize-handle"),
            "app.css must style .col-resize-handle so drag handles are discoverable"
        );
        assert!(
            content.contains(".col-resize-handle:hover")
                || content.contains(".col-resize-handle:active"),
            "app.css must provide a hover/active state for .col-resize-handle"
        );
        assert!(
            content.contains("@media (max-width: 375px)"),
            "app.css must keep a 375px breakpoint block"
        );
        assert!(
            content.contains("data-label"),
            "app.css must reference data-label (card-layout ::before content)"
        );
        // The card-layout block must target all three detail-pane tables.
        for selector in [".peers-table", ".trackers-table", ".files-table"] {
            assert!(
                content.contains(selector),
                "app.css must style {selector} (M235 card layout)"
            );
        }
    }

    /// M235 — index.html must load `col-resize.js` so the M167-era
    /// column-resize feature activates. The new `.col-resize-handle`
    /// CSS provides the visible affordance assertion lives in
    /// `test_app_css_has_m235_col_resize_handle_and_card_layout`.
    #[test]
    fn test_index_html_links_col_resize_js() {
        let (_mime, bytes) = get("index.html").expect("index.html embedded");
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains("js/col-resize.js"),
            "index.html must load js/col-resize.js so column widths can be dragged"
        );
    }

    #[test]
    fn test_col_resize_js_has_storage_and_drag() {
        let (_mime, bytes) = get("js/col-resize.js").expect("col-resize.js embedded");
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains("localStorage"),
            "col-resize.js must persist widths to localStorage"
        );
        assert!(
            content.contains("mousedown"),
            "col-resize.js must handle mousedown for drag initiation"
        );
        assert!(
            content.contains("col-resize-handle"),
            "col-resize.js must create drag handle elements"
        );
        assert!(
            content.contains("htmx:afterSwap"),
            "col-resize.js must re-apply widths after HTMX swaps"
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
