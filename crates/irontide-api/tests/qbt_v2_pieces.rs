//! Integration tests for M171 Lane B `/api/v2/torrents/pieceStates`
//! (B3) and `/api/v2/torrents/pieceHashes` (B4).
//!
//! A fresh magnet torrent has no metadata (piece count unknown), so
//! both endpoints must return 404 for it. A `.torrent`-sourced add
//! yields metadata immediately; that's the happy-path case.

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
    let resume_dir = std::env::temp_dir().join(format!(
        "irontide-qbt-v2-pieces-resume-{pid}-{n}"
    ));
    let reg_path =
        std::env::temp_dir().join(format!("irontide-qbt-v2-pieces-{pid}-{n}.toml"));
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

#[derive(Serialize)]
struct TestTorrent {
    #[serde(skip_serializing_if = "Option::is_none")]
    announce: Option<String>,
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

/// Build a single-file v1 torrent with `num_pieces` pieces of the given
/// piece length. Each piece is a separate deterministic byte, so
/// hash-dependent assertions are repeatable.
fn make_torrent(name: &str, piece_length: u64, num_pieces: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((piece_length as usize) * (num_pieces as usize));
    for p in 0..num_pieces {
        // Fill each piece with a unique byte so the SHA-1 is distinct.
        let byte = (p & 0xFF) as u8;
        data.extend(std::iter::repeat_n(byte, piece_length as usize));
    }
    let mut pieces = Vec::with_capacity(20 * num_pieces as usize);
    for chunk in data.chunks(piece_length as usize) {
        let h = irontide::core::sha1(chunk);
        pieces.extend_from_slice(h.as_bytes());
    }
    let t = TestTorrent {
        announce: None,
        info: TestInfo {
            length: data.len() as u64,
            name: name.into(),
            piece_length,
            pieces: ByteBuf::from(pieces),
        },
    };
    irontide::bencode::to_bytes(&t).expect("bencode")
}

async fn get_json(router: &axum::Router, uri: &str, cookie: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
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

// ── /pieceStates (B3) ─────────────────────────────────────────────────

#[tokio::test]
async fn piece_states_endpoint_returns_array_of_integers() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    // 4-piece torrent — small enough to keep the assertion simple.
    let bytes = make_torrent("fixture.bin", 16_384, 4);
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let (status, v) = get_json(
        &router,
        &format!("/api/v2/torrents/pieceStates?hash={hash}"),
        &sid,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 4, "expected 4 piece states");
    for (i, entry) in arr.iter().enumerate() {
        let code = entry.as_u64().expect("u64 piece state");
        assert!(
            code <= 2,
            "piece {i} state code out of range: got {code}, expected 0/1/2"
        );
    }
}

#[tokio::test]
async fn piece_states_unknown_hash_returns_404() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let (status, _) = get_json(
        &router,
        "/api/v2/torrents/pieceStates?hash=0123456789abcdef0123456789abcdef01234567",
        &sid,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn piece_states_404_pre_metadata() {
    // E0.9 mandatory — a fresh magnet with unresolved metadata returns
    // 404 (pieces are unknowable until metadata arrives).
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    // Submit a well-formed magnet URI. Because enable_dht/lsd/pex are
    // all disabled in the test settings, metadata will never resolve
    // for the duration of the test.
    let magnet =
        "magnet:?xt=urn:btih:aabbccddeeff00112233445566778899aabbccdd&dn=nometa";
    let params = SessionAddTorrentParams::magnet(magnet);
    let hash = session.add_torrent(params).await.expect("add magnet");

    let (status, _) = get_json(
        &router,
        &format!(
            "/api/v2/torrents/pieceStates?hash={}",
            hash.to_hex()
        ),
        &sid,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "pre-metadata torrent must return 404, not 200 []"
    );
}

#[tokio::test]
async fn piece_states_invalid_hash_returns_400() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let (status, _) = get_json(
        &router,
        "/api/v2/torrents/pieceStates?hash=not-a-hash",
        &sid,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn piece_states_missing_auth_returns_403() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());

    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/torrents/pieceStates?hash=0123456789abcdef0123456789abcdef01234567")
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn piece_states_qbt_compat_disabled_returns_404() {
    let mut settings = default_settings();
    settings.qbt_compat.enabled = false;
    let session = start_session(settings).await;
    let router = build_router(session.clone());

    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/torrents/pieceStates?hash=0123456789abcdef0123456789abcdef01234567")
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ── /pieceHashes (B4) ─────────────────────────────────────────────────

/// Recompute the expected SHA-1 hex hash for piece `idx` of the fixture
/// produced by [`make_torrent`]. The fixture fills each piece with the
/// byte `idx & 0xFF` repeated `piece_length` times, so tests can check
/// specific indices without needing to parse the torrent back out.
fn expected_sha1_hex_for(piece_idx: u32, piece_length: u64) -> String {
    let byte = (piece_idx & 0xFF) as u8;
    let data: Vec<u8> = std::iter::repeat_n(byte, piece_length as usize).collect();
    let h = irontide::core::sha1(&data);
    hex::encode(h.as_bytes())
}

#[tokio::test]
async fn piece_hashes_v1_returns_40_char_sha1_hex() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let piece_length: u64 = 16_384;
    let bytes = make_torrent("fixture.bin", piece_length, 3);
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let (status, v) = get_json(
        &router,
        &format!("/api/v2/torrents/pieceHashes?hash={hash}"),
        &sid,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 3, "3-piece torrent should yield 3 hashes");

    for (i, entry) in arr.iter().enumerate() {
        let s = entry.as_str().expect("hash string");
        assert_eq!(
            s.len(),
            40,
            "piece {i}: v1 hash must be 40-char SHA-1 hex"
        );
        assert!(
            s.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "piece {i}: hash must be lowercase hex: {s}"
        );
        assert_eq!(
            s,
            &expected_sha1_hex_for(i as u32, piece_length),
            "piece {i} hash mismatch"
        );
    }
}

#[tokio::test]
async fn piece_hashes_pagination_limits_response() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    // 100-piece torrent, `?offset=10&limit=20` → 20 hashes starting
    // at piece 10.
    let piece_length: u64 = 16_384;
    let bytes = make_torrent("paged.bin", piece_length, 100);
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let (status, v) = get_json(
        &router,
        &format!("/api/v2/torrents/pieceHashes?hash={hash}&offset=10&limit=20"),
        &sid,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 20, "expected 20 hashes under ?limit=20");

    // First hash in the page must correspond to piece index 10.
    let first = arr[0].as_str().expect("str");
    assert_eq!(first, expected_sha1_hex_for(10, piece_length));
}

#[tokio::test]
async fn piece_hashes_default_limit_includes_small_torrent() {
    // Under the default cap (4096) a small torrent returns its full
    // hash set untruncated.
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let piece_length: u64 = 16_384;
    let bytes = make_torrent("small.bin", piece_length, 100);
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let (status, v) = get_json(
        &router,
        &format!("/api/v2/torrents/pieceHashes?hash={hash}"),
        &sid,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 100, "100-piece torrent fits under default 4096 cap");
}

#[tokio::test]
async fn piece_hashes_offset_past_end_returns_empty() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let piece_length: u64 = 16_384;
    let bytes = make_torrent("offpast.bin", piece_length, 5);
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    // Only 5 hashes; offset=100 is well past the end.
    let (status, v) = get_json(
        &router,
        &format!("/api/v2/torrents/pieceHashes?hash={hash}&offset=100&limit=10"),
        &sid,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = v.as_array().expect("array");
    assert!(arr.is_empty(), "offset past end must return []");
}

#[tokio::test]
async fn piece_hashes_unknown_hash_returns_404() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let (status, _) = get_json(
        &router,
        "/api/v2/torrents/pieceHashes?hash=0123456789abcdef0123456789abcdef01234567",
        &sid,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn piece_hashes_invalid_hash_returns_400() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let (status, _) = get_json(
        &router,
        "/api/v2/torrents/pieceHashes?hash=not-a-hash",
        &sid,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn piece_hashes_missing_auth_returns_403() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());

    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/torrents/pieceHashes?hash=0123456789abcdef0123456789abcdef01234567")
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn piece_hashes_qbt_compat_disabled_returns_404() {
    let mut settings = default_settings();
    settings.qbt_compat.enabled = false;
    let session = start_session(settings).await;
    let router = build_router(session.clone());

    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/torrents/pieceHashes?hash=0123456789abcdef0123456789abcdef01234567")
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
