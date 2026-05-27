#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M175 + M228: integration test code — fixtures use bounded sizes"
)]

//! M228: Integration tests for the two `FIXME(M171)` closures in
//! `crates/irontide-api/src/routes/qbt_v2/files.rs`:
//!
//! - `priority` is now sourced from `session.file_priorities(id)` and
//!   projected through `file_priority_to_qbt` (qBt v4.x `WebUI` v2 wire
//!   encoding: `Skip→0, Low|Normal→1, High→6`).
//! - `availability` is now sourced from `session.piece_availability(id)`
//!   and averaged via `compute_availability` over the file's piece range.
//!
//! These three tests cover the priority side of the wire surface (default
//! Normal, explicit Skip, explicit High). The fallback path
//! (priorities Vec shorter than file list) and the availability helper
//! are unit-tested inline in `files.rs::tests` since they exercise the
//! private helpers directly.

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde::Serialize;
use serde_json::Value;
use tower::ServiceExt;

use irontide::core::FilePriority;
use irontide::session::{SessionHandle, Settings};
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

async fn test_session() -> SessionHandle {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let resume_dir = std::env::temp_dir().join(format!(
        "irontide-qbt-v2-files-m228-{}-{}",
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&resume_dir);

    let mut settings = Settings {
        listen_port: 0,
        download_dir: std::path::PathBuf::from("/tmp"),
        enable_dht: false,
        enable_lsd: false,
        enable_upnp: false,
        enable_natpmp: false,
        resume_data_dir: Some(resume_dir),
        save_resume_interval_secs: 0,
        ..Settings::default()
    };
    settings.qbt_compat.enabled = true;
    irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("failed to start test session")
}

async fn login(router: &axum::Router) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("username=admin&password=adminadmin"))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .expect("Set-Cookie")
        .to_str()
        .unwrap()
        .to_owned();
    let _ = resp.into_body().collect().await.unwrap();
    cookie.split(';').next().unwrap().to_owned()
}

async fn get_files(router: &axum::Router, sid: &str, hash: &str) -> Value {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v2/torrents/files?hash={hash}"))
        .header(header::COOKIE, sid)
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
    serde_json::from_slice(&body).unwrap()
}

fn make_torrent_bytes(name: &str, piece_length: u64, files: &[(&str, u64)]) -> Vec<u8> {
    #[derive(Serialize)]
    struct RawFile<'a> {
        length: u64,
        path: &'a [String],
    }
    #[derive(Serialize)]
    struct Info<'a> {
        files: Vec<RawFile<'a>>,
        name: &'a str,
        #[serde(rename = "piece length")]
        piece_length: u64,
        #[serde(with = "serde_bytes")]
        pieces: &'a [u8],
    }
    #[derive(Serialize)]
    struct Torrent<'a> {
        info: Info<'a>,
    }

    let total: u64 = files.iter().map(|(_, len)| *len).sum();
    let num_pieces = total.div_ceil(piece_length).max(1);
    let pieces = vec![0u8; (num_pieces as usize).saturating_mul(20)];

    // Owned path-segment storage so RawFile can borrow.
    let path_segments: Vec<Vec<String>> = files
        .iter()
        .map(|(n, _)| vec![(*n).to_owned()])
        .collect();
    let raw_files: Vec<RawFile<'_>> = files
        .iter()
        .enumerate()
        .map(|(i, (_, len))| RawFile {
            length: *len,
            path: &path_segments[i],
        })
        .collect();

    let t = Torrent {
        info: Info {
            files: raw_files,
            name,
            piece_length,
            pieces: &pieces,
        },
    };
    irontide::bencode::to_bytes(&t).expect("bencode serialize")
}

// ── Priority round-trip tests ──────────────────────────────────────────

/// Default priority after `add_torrent_bytes` is `FilePriority::Normal`,
/// which projects to qBt wire `1` (qBt v4.x `WebUI` v2 convention).
#[tokio::test]
async fn m228_files_priority_default_normal_to_qbt_one() {
    let session = test_session().await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let bytes = make_torrent_bytes(
        "m228-default-prio",
        262_144,
        &[("a.bin", 524_288), ("b.bin", 524_288)],
    );
    let hashes = session.add_torrent_bytes(&bytes).await.unwrap();
    let info_hash = hashes.v1.expect("v1 hash");
    let hash = info_hash.to_hex();

    let arr = get_files(&router, &sid, &hash).await;
    let files = arr.as_array().expect("array");
    assert_eq!(files.len(), 2);
    assert_eq!(files[0]["priority"], 1, "default Normal must wire as 1");
    assert_eq!(files[1]["priority"], 1, "default Normal must wire as 1");
}

/// Setting `FilePriority::Skip` on a file projects to qBt wire `0`.
#[tokio::test]
async fn m228_files_priority_skip_to_qbt_zero() {
    let session = test_session().await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let bytes = make_torrent_bytes(
        "m228-skip-prio",
        262_144,
        &[("a.bin", 524_288), ("b.bin", 524_288)],
    );
    let hashes = session.add_torrent_bytes(&bytes).await.unwrap();
    let info_hash = hashes.v1.expect("v1 hash");

    session
        .set_file_priority(info_hash, 1, FilePriority::Skip)
        .await
        .expect("set file_priority Skip");

    let arr = get_files(&router, &sid, &info_hash.to_hex()).await;
    let files = arr.as_array().unwrap();
    assert_eq!(files[0]["priority"], 1, "file 0 stays Normal");
    assert_eq!(files[1]["priority"], 0, "file 1 Skip wires as 0");
}

/// Setting `FilePriority::High` on a file projects to qBt wire `6`.
#[tokio::test]
async fn m228_files_priority_high_to_qbt_six() {
    let session = test_session().await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let bytes = make_torrent_bytes(
        "m228-high-prio",
        262_144,
        &[("a.bin", 524_288), ("b.bin", 524_288)],
    );
    let hashes = session.add_torrent_bytes(&bytes).await.unwrap();
    let info_hash = hashes.v1.expect("v1 hash");

    session
        .set_file_priority(info_hash, 0, FilePriority::High)
        .await
        .expect("set file_priority High");

    let arr = get_files(&router, &sid, &info_hash.to_hex()).await;
    let files = arr.as_array().unwrap();
    assert_eq!(files[0]["priority"], 6, "file 0 High wires as 6");
    assert_eq!(files[1]["priority"], 1, "file 1 stays Normal (1)");
}
