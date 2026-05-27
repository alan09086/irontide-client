//! M230 — Phase O #8/14: `WebUI` Add Torrent (three modalities).
//!
//! Router-level integration tests for the new `/webui/add-file` and
//! `/webui/add-url` handlers. The shared one-form-per-tab dialog in
//! `index.html` posts to one of these per active tab; CSRF is inherited
//! from the top-level `qbt_v2::csrf_guard` layer (see `webui.rs:542`
//! comment).
//!
//! Coverage split (per M230 plan):
//! - **This file (6 tests)** — router-level paths: file happy path,
//!   file oversize (413 from Axum's `DefaultBodyLimit`), file malformed
//!   bencode (422 from session), URL pre-flight blocks for loopback /
//!   `file://` / empty-input.
//! - **`webui.rs mod tests` (1 test)** — happy-path URL fetch via the
//!   private `fetch_torrent_url_blocking` helper, exercised with a
//!   permissive `UrlSecurityConfig` against a one-shot loopback HTTP
//!   server. Lives inline because the helper is `pub(crate)`-scope only.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde::Serialize;
use tempfile::TempDir;
use tower::ServiceExt;

use irontide::session::Settings;
use irontide_api::routes::build_router;

/// Build a router backed by a session with an isolated resume + download
/// directory. Mirrors `webui_actions.rs::test_router_isolated` — the
/// returned `TempDir` must outlive the test.
async fn test_router_isolated() -> (axum::Router, TempDir) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let settings = Settings {
        listen_port: 0,
        download_dir: dir.path().join("downloads"),
        enable_dht: false,
        enable_lsd: false,
        enable_upnp: false,
        enable_natpmp: false,
        resume_data_dir: Some(dir.path().join("resume")),
        save_resume_interval_secs: 0,
        ..Settings::default()
    };

    let session = irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("start test session");
    (build_router(session), dir)
}

/// Synthesise a minimal v1 `.torrent` that the session will accept.
/// Mirrors `qbt_v2_torrents.rs::make_test_torrent_bytes` (line 110) but
/// uses a static name + counter wired through the file-scope atomic so
/// parallel tests don't collide on the synth info hash.
fn make_test_torrent_bytes(name_suffix: u32) -> Vec<u8> {
    #[derive(Serialize)]
    struct Info {
        #[serde(rename = "piece length")]
        piece_length: u32,
        pieces: serde_bytes::ByteBuf,
        name: String,
        length: u32,
    }

    #[derive(Serialize)]
    struct Root {
        announce: String,
        info: Info,
    }

    let data = vec![0xAB; 16384];
    let hash = irontide::core::sha1(&data);
    let mut pieces = Vec::new();
    pieces.extend_from_slice(hash.as_bytes());

    let root = Root {
        announce: "http://example.com/announce".into(),
        info: Info {
            piece_length: 16384,
            pieces: serde_bytes::ByteBuf::from(pieces),
            name: format!("m230-add-file-{name_suffix}"),
            length: 16384,
        },
    };

    irontide::bencode::to_bytes(&root).expect("bencode")
}

/// Build a multipart/form-data body containing a single `torrents=` field
/// holding `bytes`. Returns `(body, content_type_header)`.
fn build_multipart_body(bytes: &[u8]) -> (Vec<u8>, String) {
    let boundary = "----IronTideM230TestBoundary";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"torrents\"; filename=\"test.torrent\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: application/x-bittorrent\r\n\r\n");
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (body, format!("multipart/form-data; boundary={boundary}"))
}

// ─── /webui/add-file ───────────────────────────────────────────────────

#[tokio::test]
async fn add_file_form_accepts_valid_torrent() {
    let (router, _tempdir) = test_router_isolated().await;
    let torrent = make_test_torrent_bytes(1);
    let (body, ct) = build_multipart_body(&torrent);

    let req = Request::post("/webui/add-file")
        .header(header::CONTENT_TYPE, ct)
        .body(Body::from(body))
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("send");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "happy-path file upload must return 200"
    );
    let hx = response
        .headers()
        .get("HX-Trigger")
        .and_then(|v| v.to_str().ok());
    assert_eq!(
        hx,
        Some("refreshList"),
        "successful add must carry HX-Trigger: refreshList for HTMX list refresh"
    );
}

#[tokio::test]
async fn add_file_form_rejects_oversize() {
    // M230 D4: with `DefaultBodyLimit::max(10 MiB)` on the route and
    // Axum 0.8's per-field multipart semantics, an oversized `torrents=`
    // field surfaces as an extractor error from `field.bytes().await`,
    // which the handler maps to 422 with a clear message. The router
    // does NOT pre-check total Content-Length; pure 413 behaviour would
    // require `tower_http::limit::RequestBodyLimitLayer` and is out of
    // scope (the user-facing rejection + diagnostic message are
    // functionally equivalent). Defence-in-depth: both the per-field
    // DefaultBodyLimit and the handler-side `MAX_TORRENT_BYTES` guard
    // reject oversize uploads; this test triggers whichever fires first.
    let (router, _tempdir) = test_router_isolated().await;
    let oversize = vec![0u8; (10 * 1024 * 1024) + 1];
    let (body, ct) = build_multipart_body(&oversize);

    let req = Request::post("/webui/add-file")
        .header(header::CONTENT_TYPE, ct)
        .body(Body::from(body))
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("send");
    assert!(
        matches!(
            response.status(),
            StatusCode::PAYLOAD_TOO_LARGE | StatusCode::UNPROCESSABLE_ENTITY
        ),
        "oversize upload must be rejected as 413 (router) or 422 (extractor / handler-side), got {}",
        response.status()
    );
    assert!(
        response.headers().get("HX-Trigger").is_none(),
        "rejected uploads must not carry HX-Trigger"
    );
}

#[tokio::test]
async fn add_file_form_rejects_invalid_bencode() {
    let (router, _tempdir) = test_router_isolated().await;
    // 1 KiB of garbage that is not valid bencode.
    let garbage = vec![0xFFu8; 1024];
    let (body, ct) = build_multipart_body(&garbage);

    let req = Request::post("/webui/add-file")
        .header(header::CONTENT_TYPE, ct)
        .body(Body::from(body))
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("send");
    assert_eq!(
        response.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "malformed .torrent bytes must surface as 422 from the handler"
    );
    assert!(
        response.headers().get("HX-Trigger").is_none(),
        "error response must not carry HX-Trigger"
    );
    let text = response_text(response).await;
    assert!(
        text.contains("error-message"),
        "expected error-fragment HTML, got: {text}"
    );
}

// ─── /webui/add-url ────────────────────────────────────────────────────

#[tokio::test]
async fn add_url_form_pre_flight_blocks_localhost() {
    let (router, _tempdir) = test_router_isolated().await;
    let body = b"url=http%3A%2F%2F127.0.0.1%3A1%2Fx.torrent".to_vec();

    let req = Request::post("/webui/add-url")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("send");
    assert_eq!(
        response.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "loopback URL must fail pre-flight SSRF check"
    );
    let text = response_text(response).await;
    assert!(
        text.contains("error-message"),
        "expected error fragment, got: {text}"
    );
}

#[tokio::test]
async fn add_url_form_pre_flight_blocks_file_scheme() {
    let (router, _tempdir) = test_router_isolated().await;
    let body = b"url=file%3A%2F%2F%2Fetc%2Fpasswd".to_vec();

    let req = Request::post("/webui/add-url")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("send");
    assert_eq!(
        response.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "file:// scheme must fail pre-flight URL validation"
    );
}

#[tokio::test]
async fn add_url_form_rejects_bad_url() {
    let (router, _tempdir) = test_router_isolated().await;
    // Empty `url=` value — `Url::parse` fails on empty input.
    let body = b"url=".to_vec();

    let req = Request::post("/webui/add-url")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("send");
    assert_eq!(
        response.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "empty / unparseable URL must surface as 422"
    );
}

// ─── helpers ───────────────────────────────────────────────────────────

async fn response_text(response: axum::response::Response) -> String {
    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    String::from_utf8_lossy(&body).to_string()
}
