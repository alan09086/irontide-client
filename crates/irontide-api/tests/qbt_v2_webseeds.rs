//! Integration tests for M171 Lane B `/api/v2/torrents/webseeds`.
//!
//! Two critical invariants:
//! 1. The merge order is BEP 19 `url-list` first, BEP 17 `httpseeds`
//!    second (wire order — matches qBt 5.x behaviour).
//! 2. Torrents with no web seeds return an empty array (not a 404).

mod common;

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde::Serialize;
use serde_bytes::ByteBuf;
use serde_json::Value;
use tower::ServiceExt;

use common::add_and_wait;
use irontide::session::{SessionAddTorrentParams, SessionHandle, Settings};
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fresh_paths() -> (PathBuf, PathBuf) {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let resume_dir =
        std::env::temp_dir().join(format!("irontide-qbt-v2-webseeds-resume-{pid}-{n}"));
    let reg_path = std::env::temp_dir().join(format!("irontide-qbt-v2-webseeds-{pid}-{n}.toml"));
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&reg_path);
    (resume_dir, reg_path)
}

fn default_settings() -> Settings {
    let (resume_dir, reg_path) = fresh_paths();
    let mut settings = Settings {
        listen_port: 0,
        download_dir: std::path::PathBuf::from("/tmp"),
        enable_dht: false,
        enable_pex: false,
        enable_lsd: false,
        enable_upnp: false,
        enable_natpmp: false,
        resume_data_dir: Some(resume_dir),
        save_resume_interval_secs: 0,
        category_registry_path: Some(reg_path),
        ..Settings::default()
    };
    settings.qbt_compat.enabled = true;
    settings
}

async fn start_session(settings: Settings) -> SessionHandle {
    irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("start session")
}

async fn login(router: &axum::Router) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("username=admin&password=adminadmin"))
        .expect("build login");
    let resp = router.clone().oneshot(req).await.expect("login");
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .expect("Set-Cookie")
        .to_str()
        .expect("utf-8")
        .to_owned();
    let _ = resp.into_body().collect().await.expect("drain");
    cookie.split(';').next().expect("cookie").to_owned()
}

/// A minimal single-file torrent fixture that can carry BEP 17
/// `httpseeds` and BEP 19 `url-list` alongside the base metadata.
///
/// `url-list` can be a single string or a list; both serialisations
/// round-trip through IronTide's tolerant parser, but the "list" form
/// is what qBt itself emits, so we use that here.
#[derive(Serialize)]
struct TestTorrent {
    #[serde(skip_serializing_if = "Option::is_none")]
    announce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    httpseeds: Option<Vec<String>>,
    info: TestInfo,
    #[serde(rename = "url-list", skip_serializing_if = "Option::is_none")]
    url_list: Option<Vec<String>>,
}

#[derive(Serialize)]
struct TestInfo {
    length: u64,
    name: String,
    #[serde(rename = "piece length")]
    piece_length: u64,
    pieces: ByteBuf,
}

fn make_torrent(
    name: &str,
    url_list: Option<Vec<String>>,
    httpseeds: Option<Vec<String>>,
) -> Vec<u8> {
    let piece_length: u64 = 16_384;
    let data = vec![0xAE_u8; (piece_length as usize) * 2];
    let mut pieces = Vec::with_capacity(40);
    for chunk in data.chunks(piece_length as usize) {
        let h = irontide::core::sha1(chunk);
        pieces.extend_from_slice(h.as_bytes());
    }
    let t = TestTorrent {
        announce: None,
        httpseeds,
        info: TestInfo {
            length: data.len() as u64,
            name: name.into(),
            piece_length,
            pieces: ByteBuf::from(pieces),
        },
        url_list,
    };
    irontide::bencode::to_bytes(&t).expect("bencode")
}

async fn get_webseeds(router: &axum::Router, hash: &str, cookie: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v2/torrents/webseeds?hash={hash}"))
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .expect("build GET");
    let resp = router.clone().oneshot(req).await.expect("GET");
    let status = resp.status();
    let bytes = resp.into_body().collect().await.expect("drain").to_bytes();
    let v = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, v)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn webseeds_endpoint_returns_url_list() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let urls = vec![
        "http://cdn.example.com/files/".to_string(),
        "http://mirror.example.com/".to_string(),
    ];
    let bytes = make_torrent("fixture.bin", Some(urls.clone()), None);
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let (status, v) = get_webseeds(&router, &hash, &sid).await;
    assert_eq!(status, StatusCode::OK);
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 2);

    let returned: Vec<&str> = arr
        .iter()
        .filter_map(|row| row.get("url").and_then(Value::as_str))
        .collect();
    assert_eq!(returned, urls);
}

#[tokio::test]
async fn httpseeds_merged_with_url_list() {
    // E0.8 mandatory — BEP 17 `httpseeds` must merge with BEP 19 `url-list`
    // into a single list. qBt surfaces both under the same endpoint.
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let url_list = vec!["http://bep19.example.com/files/".to_string()];
    let httpseeds = vec!["http://bep17.example.com/seeds/".to_string()];
    let bytes = make_torrent(
        "merged.bin",
        Some(url_list.clone()),
        Some(httpseeds.clone()),
    );
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let (status, v) = get_webseeds(&router, &hash, &sid).await;
    assert_eq!(status, StatusCode::OK);
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 2, "both BEP 19 and BEP 17 URLs must appear");

    // Order: BEP 19 (url-list) before BEP 17 (httpseeds).
    let urls: Vec<&str> = arr
        .iter()
        .filter_map(|row| row.get("url").and_then(Value::as_str))
        .collect();
    assert_eq!(urls[0], "http://bep19.example.com/files/");
    assert_eq!(urls[1], "http://bep17.example.com/seeds/");
}

#[tokio::test]
async fn webseeds_endpoint_empty_when_no_seeds() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let bytes = make_torrent("no-seeds.bin", None, None);
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let (status, v) = get_webseeds(&router, &hash, &sid).await;
    assert_eq!(status, StatusCode::OK);
    let arr = v.as_array().expect("array");
    assert!(arr.is_empty(), "a torrent with no seeds must return []");
}

#[tokio::test]
async fn webseeds_endpoint_unknown_hash_returns_404() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let (status, _) = get_webseeds(&router, "0123456789abcdef0123456789abcdef01234567", &sid).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn webseeds_endpoint_invalid_hash_returns_400() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let (status, _) = get_webseeds(&router, "not-a-hash", &sid).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn webseeds_endpoint_missing_auth_returns_403() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());

    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/torrents/webseeds?hash=0123456789abcdef0123456789abcdef01234567")
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn webseeds_endpoint_qbt_compat_disabled_returns_404() {
    let mut settings = default_settings();
    settings.qbt_compat.enabled = false;
    let session = start_session(settings).await;
    let router = build_router(session.clone());

    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/torrents/webseeds?hash=0123456789abcdef0123456789abcdef01234567")
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
