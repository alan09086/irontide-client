//! M218: blocking HTTP fetch for the Add Torrent → URL tab.
//!
//! Mirrors the structure of [`crate::update_checker`] (M209): a
//! `reqwest::blocking::Client` runs on a one-shot worker thread; results land
//! back on the Slint event loop via `weak.upgrade_in_event_loop`.
//!
//! The fetch is fronted by the SSRF validators from
//! [`irontide::url_guard`](irontide::url_guard) so private/loopback hosts and
//! IPv4-mapped IPv6 loopback (`::ffff:127.0.0.1`) are rejected before the
//! request leaves the process. The redirect policy is the same one tracker
//! and web-seed code use; a 30x to a private IP is blocked in-flight.

use std::io::Read as _;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use irontide::url_guard::{self, UrlGuardError, UrlSecurityConfig};
use parking_lot::Mutex;

use crate::app::{AddTorrentPreview, AddTorrentSource, AppState};
use crate::bridge;

/// Hard ceiling on `.torrent` blob size. Real-world torrents are KB-scale;
/// 10 MiB is a generous sanity cap that rejects abusive responses without
/// ever clipping a legitimate metadata file.
pub const MAX_TORRENT_BYTES: u64 = 10 * 1024 * 1024;

const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Errors returned by [`fetch_torrent_blocking`].
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// URL failed pre-flight validation (bad scheme, malformed, etc.).
    #[error("{0}")]
    UrlBlocked(String),

    /// `reqwest` returned a transport / build error (DNS, connect, TLS, etc.).
    #[error("HTTP error: {0}")]
    Http(String),

    /// Server returned a non-2xx status.
    #[error("HTTP status {0}")]
    Status(u16),

    /// Server advertised or sent a body bigger than [`MAX_TORRENT_BYTES`].
    #[error("response too large: {0} bytes (max {MAX_TORRENT_BYTES})")]
    TooLarge(u64),

    /// Response body doesn't look like a `.torrent` (wrong Content-Type AND
    /// first byte isn't `b'd'`, the BEP 3 bencode dict marker).
    #[error(
        "response does not look like a .torrent file (Content-Type: {ct:?}, first byte: {first:?})"
    )]
    NotTorrent {
        /// The response's `Content-Type` header, if any.
        ct: Option<String>,
        /// The first body byte, if the body was non-empty.
        first: Option<u8>,
    },
}

impl From<UrlGuardError> for FetchError {
    fn from(err: UrlGuardError) -> Self {
        Self::UrlBlocked(err.to_string())
    }
}

/// Blocking HTTP `GET` of a `.torrent` URL with SSRF guard + size cap +
/// content sniff.
///
/// Caller is responsible for running this off the Slint event-loop thread —
/// it blocks for up to [`FETCH_TIMEOUT`].
///
/// Steps (in order; each can short-circuit):
/// 1. Pre-flight SSRF + scheme validation via [`url_guard::validate_user_url`].
/// 2. Build a `reqwest::blocking::Client` with the shared redirect policy
///    from [`url_guard::build_redirect_policy`] (blocks public→private
///    redirects too).
/// 3. Issue the request; reject non-2xx status.
/// 4. Reject early if `Content-Length` already exceeds [`MAX_TORRENT_BYTES`].
/// 5. Stream the body via `Read::take(MAX + 1)` — never buffer more than
///    `MAX + 1` bytes. A server that lies about `Content-Length` and sends a
///    multi-gigabyte body will see the read cap, not OOM the host.
/// 6. Accept the body if either the `Content-Type` is
///    `application/x-bittorrent` / `application/octet-stream`, *or* the first
///    byte is `b'd'` (the BEP 3 bencoded dict marker — every valid `.torrent`
///    starts with it).
pub fn fetch_torrent_blocking(
    url: &str,
    config: UrlSecurityConfig,
) -> Result<Vec<u8>, FetchError> {
    // 1. Pre-flight validation (scheme, SSRF, IDNA).
    url_guard::validate_user_url(url, config)?;

    // 2. Build blocking client with shared SSRF redirect policy.
    let client = reqwest::blocking::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .redirect(url_guard::build_redirect_policy(config))
        .user_agent(concat!("irontide/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| FetchError::Http(e.to_string()))?;

    // 3. Issue GET.
    let mut resp = client
        .get(url)
        .send()
        .map_err(|e| FetchError::Http(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(FetchError::Status(status.as_u16()));
    }

    // 4. Content-Length pre-check (cheap; rejects obvious abuse before any
    //    body bytes flow).
    if let Some(len) = resp.content_length()
        && len > MAX_TORRENT_BYTES
    {
        return Err(FetchError::TooLarge(len));
    }

    // 5. Capture Content-Type before consuming the body.
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    // 6. Stream the body with a hard read cap. take(MAX + 1) means a hostile
    //    server cannot OOM the process — even with a lying Content-Length
    //    header, we read at most MAX + 1 bytes before erroring out.
    let mut body = Vec::new();
    let cap = MAX_TORRENT_BYTES.saturating_add(1);
    (&mut resp)
        .take(cap)
        .read_to_end(&mut body)
        .map_err(|e| FetchError::Http(e.to_string()))?;
    if body.len() as u64 > MAX_TORRENT_BYTES {
        return Err(FetchError::TooLarge(body.len() as u64));
    }

    // 7. Sniff: accept if Content-Type matches OR first byte is 'd'.
    let ct_ok = content_type.as_deref().is_some_and(|s| {
        s.starts_with("application/x-bittorrent") || s.starts_with("application/octet-stream")
    });
    let magic_ok = body.first() == Some(&b'd');
    if !ct_ok && !magic_ok {
        return Err(FetchError::NotTorrent {
            ct: content_type,
            first: body.first().copied(),
        });
    }

    Ok(body)
}

/// Parse `.torrent` bytes into an [`AddTorrentPreview`] keyed by
/// [`AddTorrentSource::UrlBytes`]. Thin wrapper around the bridge helper that
/// the file-from-disk path also uses — keeps preview construction in one place
/// so future schema changes (file-tree depth, folder rollup, etc.) touch a
/// single function.
fn preview_from_bytes(url: &str, bytes: Vec<u8>) -> Result<AddTorrentPreview, String> {
    let meta = irontide::core::torrent_from_bytes_any(&bytes)
        .map_err(|e| format!("invalid .torrent: {e}"))?;
    let source = AddTorrentSource::UrlBytes {
        url: url.to_string(),
        bytes,
    };
    Ok(bridge::build_preview_from_meta(&meta, source))
}

/// Spawn a one-shot worker thread that fetches `url`, parses the response into
/// an `AddTorrentPreview`, and commits the preview to `state` + UI on success.
///
/// On failure the preview is cleared and the dialog's preview-name field is
/// set to the error string so the user sees a contextual message inside the
/// dialog (in addition to a toast). The worker honours the
/// `add_torrent_url_generation` counter — if a newer fetch supersedes this
/// one before the result lands, the result is silently discarded.
pub fn spawn_torrent_url_fetch(
    weak: slint::Weak<crate::MainWindow>,
    state: Arc<Mutex<AppState>>,
    url: String,
) {
    // Capture our generation at spawn time + clone the Arc so the worker
    // can re-check on completion without holding the AppState lock.
    let (my_gen, gen_counter) = {
        let st = state.lock();
        let counter = st.add_torrent_url_generation.clone();
        let g = counter.fetch_add(1, Ordering::SeqCst) + 1;
        (g, counter)
    };

    // Snapshot the SSRF config from the GUI's settings layer. M218 keeps it
    // simple — always use defaults (ssrf_mitigation on, IDNA rejected). A
    // future milestone can route GUI preferences through here.
    let config = UrlSecurityConfig::default();

    let _ = std::thread::Builder::new()
        .name("torrent-url-fetch".into())
        .spawn(move || {
            let result = fetch_torrent_blocking(&url, config);
            // Discard if a newer fetch superseded us.
            if gen_counter.load(Ordering::SeqCst) != my_gen {
                return;
            }
            match result {
                Ok(bytes) => match preview_from_bytes(&url, bytes) {
                    Ok(preview) => {
                        let name = preview.name.clone();
                        let size_label = crate::format::format_size(preview.total_size);
                        let file_count = i32::try_from(preview.file_count).unwrap_or(i32::MAX);
                        let trackers = preview.trackers.clone();
                        let created_by = preview.created_by.clone().unwrap_or_default();
                        let file_rows = bridge::build_sendable_file_rows(&preview);

                        let file_exts = bridge::extract_file_extensions(&preview);
                        let tracker_list = bridge::extract_tracker_urls(&preview);
                        let suggested = bridge::suggest_category(&name, &file_exts, &tracker_list)
                            .unwrap_or_default();

                        // Re-check generation right before committing — minimises
                        // the race window between fetch completion and state write.
                        {
                            let mut st = state.lock();
                            if st.add_torrent_url_generation.load(Ordering::SeqCst) != my_gen {
                                return;
                            }
                            st.add_torrent_preview = Some(preview);
                        }
                        let _ = weak.upgrade_in_event_loop(move |win| {
                            win.set_add_torrent_preview_name(name.into());
                            win.set_add_torrent_preview_size(size_label.into());
                            win.set_add_torrent_preview_file_count(file_count);
                            win.set_add_torrent_preview_trackers(trackers.into());
                            win.set_add_torrent_preview_created_by(created_by.into());
                            win.set_add_torrent_suggested_category(suggested.into());
                            let model = slint::ModelRc::new(slint::VecModel::from(file_rows));
                            win.set_add_torrent_preview_files(model);
                        });
                    }
                    Err(parse_err) => {
                        commit_error(&weak, &state, my_gen, &parse_err);
                    }
                },
                Err(fetch_err) => {
                    commit_error(&weak, &state, my_gen, &fetch_err.to_string());
                }
            }
        });
}

/// Helper: clear the preview, set preview-name to an error string, and show a
/// toast. Honours the generation counter so a superseded error doesn't clobber
/// a fresh fetch's preview.
fn commit_error(
    weak: &slint::Weak<crate::MainWindow>,
    state: &Arc<Mutex<AppState>>,
    my_gen: u64,
    msg: &str,
) {
    {
        let mut st = state.lock();
        if st.add_torrent_url_generation.load(Ordering::SeqCst) != my_gen {
            return;
        }
        st.add_torrent_preview = None;
    }
    let msg_owned = format!("Fetch failed: {msg}");
    let msg_for_preview = msg_owned.clone();
    let _ = weak.upgrade_in_event_loop(move |win| {
        win.set_add_torrent_preview_name(msg_for_preview.into());
        win.set_add_torrent_preview_size(slint::SharedString::new());
        win.set_add_torrent_preview_file_count(0);
    });
    bridge::show_toast(weak, &msg_owned, true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use std::net::TcpListener;
    use std::thread;

    /// Spawn a one-shot HTTP server that returns `response` to the first
    /// connection it accepts, then closes. Returns the bound URL.
    fn spawn_one_shot_server(response: &'static [u8]) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
        let port = listener.local_addr().expect("local_addr").port();
        let url = format!("http://127.0.0.1:{port}/x.torrent");
        let handle = thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf);
                let _ = sock.write_all(response);
                let _ = sock.flush();
            }
        });
        (url, handle)
    }

    /// Permissive config that bypasses the SSRF pre-flight check, so we can
    /// hit 127.0.0.1 in tests without disabling all guards.
    fn test_config() -> UrlSecurityConfig {
        UrlSecurityConfig {
            ssrf_mitigation: false,
            allow_idna: true,
            validate_https_trackers: false,
        }
    }

    #[test]
    fn fetch_torrent_blocking_success_with_bencode_magic() {
        // Minimal bencoded dict body: `d4:spam4:eggse` (14 bytes).
        let response: &[u8] = b"HTTP/1.1 200 OK\r\n\
            Content-Type: application/octet-stream\r\n\
            Content-Length: 14\r\n\
            Connection: close\r\n\
            \r\n\
            d4:spam4:eggse";
        let (url, handle) = spawn_one_shot_server(response);
        let result = fetch_torrent_blocking(&url, test_config());
        handle.join().unwrap();
        let bytes = result.expect("fetch should succeed");
        assert_eq!(bytes, b"d4:spam4:eggse");
    }

    #[test]
    fn fetch_torrent_blocking_rejects_oversize_via_content_length() {
        // Server advertises 99999999 bytes — pre-check fires before body read.
        let response: &[u8] = b"HTTP/1.1 200 OK\r\n\
            Content-Type: application/octet-stream\r\n\
            Content-Length: 99999999\r\n\
            Connection: close\r\n\
            \r\n";
        let (url, handle) = spawn_one_shot_server(response);
        let result = fetch_torrent_blocking(&url, test_config());
        handle.join().unwrap();
        match result {
            Err(FetchError::TooLarge(n)) => assert_eq!(n, 99_999_999),
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[test]
    fn fetch_torrent_blocking_rejects_html_response() {
        // Content-Type: text/html, body starts with '<' — fails both sniff
        // checks.
        let response: &[u8] = b"HTTP/1.1 200 OK\r\n\
            Content-Type: text/html\r\n\
            Content-Length: 13\r\n\
            Connection: close\r\n\
            \r\n\
            <html></html>";
        let (url, handle) = spawn_one_shot_server(response);
        let result = fetch_torrent_blocking(&url, test_config());
        handle.join().unwrap();
        match result {
            Err(FetchError::NotTorrent { ct, first }) => {
                assert_eq!(ct.as_deref(), Some("text/html"));
                assert_eq!(first, Some(b'<'));
            }
            other => panic!("expected NotTorrent, got {other:?}"),
        }
    }

    #[test]
    fn fetch_torrent_blocking_rejects_status_404() {
        let response: &[u8] = b"HTTP/1.1 404 Not Found\r\n\
            Content-Length: 0\r\n\
            Connection: close\r\n\
            \r\n";
        let (url, handle) = spawn_one_shot_server(response);
        let result = fetch_torrent_blocking(&url, test_config());
        handle.join().unwrap();
        match result {
            Err(FetchError::Status(code)) => assert_eq!(code, 404),
            other => panic!("expected Status(404), got {other:?}"),
        }
    }

    #[test]
    fn fetch_torrent_blocking_pre_flight_blocks_localhost_by_default() {
        // With the default config (ssrf_mitigation on), localhost is rejected
        // before any TCP connect happens.
        let result = fetch_torrent_blocking(
            "http://127.0.0.1:1/x.torrent",
            UrlSecurityConfig::default(),
        );
        match result {
            Err(FetchError::UrlBlocked(_)) => {}
            other => panic!("expected UrlBlocked, got {other:?}"),
        }
    }

    #[test]
    fn fetch_torrent_blocking_pre_flight_blocks_file_scheme() {
        let result = fetch_torrent_blocking("file:///etc/passwd", test_config());
        match result {
            Err(FetchError::UrlBlocked(_)) => {}
            other => panic!("expected UrlBlocked, got {other:?}"),
        }
    }

    #[test]
    fn fetch_torrent_blocking_accepts_x_bittorrent_content_type() {
        // Content-Type alone is enough; first byte is non-'d' but still
        // accepted because the Content-Type is the authoritative .torrent MIME.
        let response: &[u8] = b"HTTP/1.1 200 OK\r\n\
            Content-Type: application/x-bittorrent\r\n\
            Content-Length: 5\r\n\
            Connection: close\r\n\
            \r\n\
            hello";
        let (url, handle) = spawn_one_shot_server(response);
        let result = fetch_torrent_blocking(&url, test_config());
        handle.join().unwrap();
        let bytes = result.expect("fetch should succeed");
        assert_eq!(bytes, b"hello");
    }
}
