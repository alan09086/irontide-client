#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M175: integration test code — fixtures use bounded sizes that fit narrower types"
)]

//! Integration tests for M170 additions to `/api/v2/torrents/properties`
//! (Lane D). Covers the four DTO fields populated from real
//! [`TorrentStats`](irontide::session::TorrentStats) state:
//! `save_path`, `created_by`, `creation_date`, `piece_size`.
//!
//! Each test synthesises a minimal v1 `.torrent` fixture with known
//! contents (including `created by` + `creation date` where relevant),
//! adds it through the in-process session handle, then asserts the JSON
//! shape of the `/properties` response.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde::Serialize;
use serde_bytes::ByteBuf;
use serde_json::Value;
use tower::ServiceExt;

use irontide::session::{SessionAddTorrentParams, SessionHandle, Settings};
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fresh_paths() -> (PathBuf, PathBuf) {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let resume_dir = std::env::temp_dir().join(format!("irontide-qbt-v2-props-resume-{pid}-{n}"));
    let reg_path = std::env::temp_dir().join(format!("irontide-qbt-v2-props-{pid}-{n}.toml"));
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&reg_path);
    (resume_dir, reg_path)
}

async fn test_session() -> SessionHandle {
    let (resume_dir, reg_path) = fresh_paths();
    let mut settings = Settings {
        listen_port: 0,
        download_dir: std::path::PathBuf::from("/tmp"),
        enable_dht: false,
        enable_lsd: false,
        enable_upnp: false,
        enable_natpmp: false,
        resume_data_dir: Some(resume_dir),
        save_resume_interval_secs: 0,
        category_registry_path: Some(reg_path),
        ..Settings::default()
    };
    settings.qbt_compat.enabled = true;
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

async fn get_json(router: &axum::Router, uri: &str, cookie: &str) -> Value {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .expect("build GET");
    let resp = router.clone().oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::OK, "uri {uri}");
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("drain")
        .to_bytes()
        .to_vec();
    serde_json::from_slice(&bytes).expect("json")
}

/// Minimal bencode fixture. `created_by` and `creation_date` sit at the
/// torrent top-level (not under `info`) per the BEP 3 torrent dict layout.
#[derive(Serialize)]
struct TestTorrent {
    #[serde(skip_serializing_if = "Option::is_none")]
    announce: Option<String>,
    #[serde(rename = "created by", skip_serializing_if = "Option::is_none")]
    created_by: Option<String>,
    #[serde(rename = "creation date", skip_serializing_if = "Option::is_none")]
    creation_date: Option<i64>,
    info: TestInfo,
}

#[derive(Serialize)]
struct TestInfo {
    length: u64,
    name: String,
    #[serde(rename = "piece length")]
    piece_length: u64,
    pieces: ByteBuf,
}

/// Build a minimal single-file torrent. `piece_length` is configurable so
/// tests can verify that `piece_size` in the response reflects the real
/// value (not the ~1KB block size we used to mistakenly report).
fn make_torrent(
    name: &str,
    piece_length: u64,
    created_by: Option<&str>,
    creation_date: Option<i64>,
) -> Vec<u8> {
    // File size must be a non-trivial multiple of piece_length so the
    // SHA-1 piece count is deterministic; we use 2 full pieces.
    let data = vec![0xCD_u8; (piece_length as usize) * 2];
    let mut pieces = Vec::with_capacity(40);
    for chunk in data.chunks(piece_length as usize) {
        let h = irontide::core::sha1(chunk);
        pieces.extend_from_slice(h.as_bytes());
    }
    let t = TestTorrent {
        announce: Some("http://example.com/announce".into()),
        created_by: created_by.map(str::to_owned),
        creation_date,
        info: TestInfo {
            length: data.len() as u64,
            name: name.into(),
            piece_length,
            pieces: ByteBuf::from(pieces),
        },
    };
    irontide::bencode::to_bytes(&t).expect("bencode")
}

/// Add a `.torrent` and block until a /properties response is available.
/// Stats become queryable shortly after `add_torrent` returns, so we poll
/// rather than sleep a blind duration.
async fn add_and_wait(session: &SessionHandle, params: SessionAddTorrentParams) -> String {
    let hash = session.add_torrent(params).await.expect("add torrent");
    for _ in 0..50 {
        if session.torrent_stats(hash).await.is_ok() {
            return hash.to_hex();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("torrent stats never became queryable");
}

#[tokio::test]
async fn save_path_populated_from_torrent_stats() {
    let session = test_session().await;
    // Pre-create a category with a distinct save_path so we know the
    // response isn't just echoing Settings.download_dir.
    session
        .create_category(
            "sonarr".to_string(),
            PathBuf::from("/tmp/irontide-m170-props-sonarr"),
        )
        .await
        .expect("create category");
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let bytes = make_torrent("single.bin", 16384, None, None);
    let params = SessionAddTorrentParams::bytes(bytes).with_category("sonarr");
    let hash = add_and_wait(&session, params).await;

    let v = get_json(
        &router,
        &format!("/api/v2/torrents/properties?hash={hash}"),
        &sid,
    )
    .await;
    let save_path = v
        .get("save_path")
        .and_then(Value::as_str)
        .expect("save_path string");
    assert_eq!(save_path, "/tmp/irontide-m170-props-sonarr");
}

#[tokio::test]
async fn created_by_populated_from_torrent_stats() {
    let session = test_session().await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let bytes = make_torrent("with-creator.bin", 16384, Some("mktorrent 1.1"), None);
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let v = get_json(
        &router,
        &format!("/api/v2/torrents/properties?hash={hash}"),
        &sid,
    )
    .await;
    let created_by = v
        .get("created_by")
        .and_then(Value::as_str)
        .expect("created_by string");
    assert_eq!(created_by, "mktorrent 1.1");
}

#[tokio::test]
async fn creation_date_populated_from_torrent_stats() {
    let session = test_session().await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let when: i64 = 1_700_000_000; // 2023-11-14 ~22:13 UTC — deterministic
    let bytes = make_torrent("with-date.bin", 16384, None, Some(when));
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let v = get_json(
        &router,
        &format!("/api/v2/torrents/properties?hash={hash}"),
        &sid,
    )
    .await;
    let cd = v
        .get("creation_date")
        .and_then(Value::as_i64)
        .expect("creation_date i64");
    assert_eq!(cd, when);
}

#[tokio::test]
async fn piece_size_populated_from_torrent_stats() {
    let session = test_session().await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    // 65_536 = 64 KB — distinct from both the 16384-default chunk size
    // and any multiple of it, so a regression in piece_size reporting
    // (e.g. returning block_size) would fail this assertion.
    let piece_len: u64 = 65_536;
    let bytes = make_torrent("big-pieces.bin", piece_len, None, None);
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let v = get_json(
        &router,
        &format!("/api/v2/torrents/properties?hash={hash}"),
        &sid,
    )
    .await;
    let ps = v
        .get("piece_size")
        .and_then(Value::as_u64)
        .expect("piece_size u64");
    assert_eq!(ps, piece_len);
}
