//! Integration tests for M171 Lane B `/api/v2/torrents/trackers`.
//!
//! Every request walks the live middleware chain (`qbt_gate` →
//! `require_sid` → handler), so the tests exercise the same path real
//! `*arr` clients do. The fixture is a minimal single-file v1 torrent —
//! metadata is already resolved on add, so the endpoint is queryable
//! without waiting on a magnet fetch.

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
    let resume_dir = std::env::temp_dir().join(format!(
        "irontide-qbt-v2-trackers-resume-{pid}-{n}"
    ));
    let reg_path =
        std::env::temp_dir().join(format!("irontide-qbt-v2-trackers-{pid}-{n}.toml"));
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&reg_path);
    (resume_dir, reg_path)
}

/// Build a session with qbt_compat enabled and all three discovery
/// subsystems (DHT/PeX/LSD) off by default so pseudo-tracker assertions
/// are deterministic. Individual tests override knobs via the returned
/// settings.
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
    #[serde(rename = "announce-list", skip_serializing_if = "Option::is_none")]
    announce_list: Option<Vec<Vec<String>>>,
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

/// Build a minimal single-file torrent with an optional announce URL.
fn make_torrent(name: &str, announce: Option<&str>) -> Vec<u8> {
    let piece_length: u64 = 16_384;
    let data = vec![0xCD_u8; (piece_length as usize) * 2];
    let mut pieces = Vec::with_capacity(40);
    for chunk in data.chunks(piece_length as usize) {
        let h = irontide::core::sha1(chunk);
        pieces.extend_from_slice(h.as_bytes());
    }
    let t = TestTorrent {
        announce: announce.map(str::to_owned),
        announce_list: None,
        info: TestInfo {
            length: data.len() as u64,
            name: name.into(),
            piece_length,
            pieces: ByteBuf::from(pieces),
        },
    };
    irontide::bencode::to_bytes(&t).expect("bencode")
}

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

// ── Tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn trackers_endpoint_returns_list_when_torrent_present() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let bytes = make_torrent("fixture.bin", Some("http://tracker.example/announce"));
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v2/torrents/trackers?hash={hash}"))
        .header(header::COOKIE, &sid)
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = resp.into_body().collect().await.expect("drain").to_bytes();
    let v: Value = serde_json::from_slice(&bytes).expect("json");
    let arr = v.as_array().expect("array");

    // Three pseudo-trackers always present + the real tracker we added.
    assert!(
        arr.len() >= 4,
        "expected at least 4 rows (3 pseudo + 1 real); got {}",
        arr.len()
    );
    assert_eq!(arr[0]["url"], "** [DHT] **");
    assert_eq!(arr[1]["url"], "** [PeX] **");
    assert_eq!(arr[2]["url"], "** [LSD] **");

    // The real tracker is somewhere past index 2.
    let real_urls: Vec<&str> = arr[3..]
        .iter()
        .filter_map(|row| row.get("url").and_then(Value::as_str))
        .collect();
    assert!(
        real_urls.contains(&"http://tracker.example/announce"),
        "real tracker URL missing from response: {real_urls:?}"
    );
}

#[tokio::test]
async fn trackers_endpoint_pseudo_trackers_emitted_first() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let bytes = make_torrent("fixture2.bin", None);
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v2/torrents/trackers?hash={hash}"))
        .header(header::COOKIE, &sid)
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = resp.into_body().collect().await.expect("drain").to_bytes();
    let v: Value = serde_json::from_slice(&bytes).expect("json");
    let arr = v.as_array().expect("array");
    assert!(arr.len() >= 3);
    // Pseudo-trackers are always `tier = -1`.
    for i in 0..3 {
        assert_eq!(
            arr[i]["tier"].as_i64().expect("tier i64"),
            -1,
            "pseudo-tracker {i} must have tier -1"
        );
    }
}

#[tokio::test]
async fn pseudo_trackers_reflect_disabled_state() {
    // All three subsystems off -> status 0 on each pseudo-tracker.
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let bytes = make_torrent("fixture3.bin", None);
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v2/torrents/trackers?hash={hash}"))
        .header(header::COOKIE, &sid)
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = resp.into_body().collect().await.expect("drain").to_bytes();
    let v: Value = serde_json::from_slice(&bytes).expect("json");
    let arr = v.as_array().expect("array");
    for (i, name) in ["** [DHT] **", "** [PeX] **", "** [LSD] **"]
        .iter()
        .enumerate()
    {
        assert_eq!(arr[i]["url"], *name);
        assert_eq!(
            arr[i]["status"].as_i64().expect("status i64"),
            0,
            "{name} should be status 0 when disabled"
        );
    }
}

#[tokio::test]
async fn trackers_endpoint_unknown_hash_returns_404() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    // 40 hex chars — valid shape, unknown torrent.
    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/torrents/trackers?hash=0123456789abcdef0123456789abcdef01234567")
        .header(header::COOKIE, &sid)
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn trackers_endpoint_invalid_hash_returns_400() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/torrents/trackers?hash=not-a-hash")
        .header(header::COOKIE, &sid)
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn trackers_endpoint_missing_auth_returns_403() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());

    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/torrents/trackers?hash=0123456789abcdef0123456789abcdef01234567")
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn trackers_endpoint_qbt_compat_disabled_returns_404() {
    // qbt_compat.enabled = false -> the entire /api/v2/* surface
    // responds 404 via the qbt_gate middleware.
    let mut settings = default_settings();
    settings.qbt_compat.enabled = false;
    let session = start_session(settings).await;
    let router = build_router(session.clone());

    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/torrents/trackers?hash=0123456789abcdef0123456789abcdef01234567")
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
