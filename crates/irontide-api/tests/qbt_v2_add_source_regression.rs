//! Regression tests guarding Lane A's conservative migration (M170).
//!
//! Lane A kept every legacy add entry point working as a thin wrapper
//! around the new `AddTorrentParams` flow. These tests pin that contract
//! from three directions:
//!
//! 1. `SessionHandle::add_magnet_uri` still works for CLI/batch adders.
//! 2. The legacy `/api/v1/torrents` POST still accepts magnet JSON + raw
//!    `.torrent` bytes.
//! 3. The qBt v2 `/torrents/add` multipart path still accepts a bare
//!    magnet submission (no category / no savepath / no paused) so the
//!    *arr connectivity test continues to succeed.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde::Serialize;
use serde_bytes::ByteBuf;
use tower::ServiceExt;

use irontide::session::{SessionHandle, Settings};
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fresh_paths(tag: &str) -> (PathBuf, PathBuf) {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let resume_dir =
        std::env::temp_dir().join(format!("irontide-qbt-v2-regression-{tag}-resume-{pid}-{n}"));
    let reg_path =
        std::env::temp_dir().join(format!("irontide-qbt-v2-regression-{tag}-{pid}-{n}.toml"));
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&reg_path);
    (resume_dir, reg_path)
}

async fn test_session(tag: &str) -> SessionHandle {
    let (resume_dir, reg_path) = fresh_paths(tag);
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

#[derive(Serialize)]
struct RegressionTorrent {
    announce: String,
    info: RegressionInfo,
}

#[derive(Serialize)]
struct RegressionInfo {
    length: u64,
    name: String,
    #[serde(rename = "piece length")]
    piece_length: u64,
    pieces: ByteBuf,
}

fn make_bytes(name: &str) -> Vec<u8> {
    let data = vec![0xAB_u8; 16384];
    let hash = irontide::core::sha1(&data);
    let t = RegressionTorrent {
        announce: "http://example.com/announce".into(),
        info: RegressionInfo {
            length: data.len() as u64,
            name: name.into(),
            piece_length: 16384,
            pieces: ByteBuf::from(hash.as_bytes().to_vec()),
        },
    };
    irontide::bencode::to_bytes(&t).expect("bencode")
}

#[tokio::test]
async fn session_handle_add_magnet_uri_still_works() {
    let session = test_session("cli-magnet").await;
    let uri = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Bunny";
    let hashes = session
        .add_magnet_uri(uri)
        .await
        .expect("legacy add_magnet_uri should not regress");
    assert!(hashes.v1.is_some(), "magnet parse should yield a v1 hash");

    // The CLI adder historically expects this returns without wiring any
    // category or explicit download_dir — confirm the resulting torrent
    // is uncategorised and uses Settings.download_dir.
    let id = hashes.v1.unwrap();
    let stats = session.torrent_stats(id).await.expect("stats");
    assert!(
        stats.category.is_none(),
        "legacy path must not assign a category"
    );
    assert_eq!(stats.save_path, "/tmp");
}

#[tokio::test]
async fn legacy_v1_torrents_post_accepts_magnet_json() {
    // The v1 `/api/v1/torrents` endpoint is an unauthenticated REST
    // surface used by the CLI batch adder + integration tests.
    let session = test_session("v1-magnet").await;
    let router = build_router(session);

    let body = serde_json::json!({
        "uri": "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Bunny",
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/torrents")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("build POST");
    let resp = router.clone().oneshot(req).await.expect("POST v1");
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "legacy v1 magnet POST must return 201"
    );
}

#[tokio::test]
async fn legacy_v1_torrents_post_accepts_torrent_bytes() {
    let session = test_session("v1-bytes").await;
    let router = build_router(session);

    let bytes = make_bytes("regression-bytes.bin");
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/torrents")
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(bytes))
        .expect("build POST");
    let resp = router.clone().oneshot(req).await.expect("POST v1 bytes");
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "legacy v1 .torrent POST must return 201"
    );
}

#[tokio::test]
async fn qbt_v2_add_magnet_form_without_optional_fields_still_works() {
    // The canonical *arr connectivity test: a bare magnet via the
    // URL-encoded form body with no category / savepath / paused fields.
    // Guards against the M170 parser rewrite breaking the single-field
    // happy path.
    let session = test_session("qbt-bare").await;
    let router = build_router(session);
    let sid = login(&router).await;

    let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Bunny";
    let mut body = String::from("urls=");
    for b in magnet.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                body.push(b as char);
            }
            _ => body.push_str(&format!("%{b:02X}")),
        }
    }
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/torrents/add")
        .header(header::COOKIE, &sid)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .expect("build POST");
    let resp = router.clone().oneshot(req).await.expect("qbt add");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "bare magnet add must succeed"
    );
}
