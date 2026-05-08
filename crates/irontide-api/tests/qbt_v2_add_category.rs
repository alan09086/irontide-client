//! Integration tests for `POST /api/v2/torrents/add` category / savepath
//! resolution (M170 Lane D).
//!
//! Exercises the download-dir precedence rules:
//! 1. `savepath=...` wins if present.
//! 2. Else `category=X` resolves via the registry (409 if unknown).
//! 3. Else `settings.download_dir` is used.
//!
//! Tests drive the HTTP endpoint directly, not the lower-level session
//! API, so they also guard the multipart + URL-encoded parser that reads
//! the category / savepath / paused fields.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use irontide::session::{SessionHandle, Settings};
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fresh_paths(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let resume_dir =
        std::env::temp_dir().join(format!("irontide-qbt-v2-addcat-{tag}-resume-{pid}-{n}"));
    let reg_path =
        std::env::temp_dir().join(format!("irontide-qbt-v2-addcat-{tag}-{pid}-{n}.toml"));
    let default_dl =
        std::env::temp_dir().join(format!("irontide-qbt-v2-addcat-{tag}-default-{pid}-{n}"));
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&reg_path);
    let _ = std::fs::remove_dir_all(&default_dl);
    std::fs::create_dir_all(&default_dl).expect("create default dl dir");
    (resume_dir, reg_path, default_dl)
}

async fn session_with_default(
    default_dl: PathBuf,
    resume_dir: PathBuf,
    reg_path: PathBuf,
) -> SessionHandle {
    let mut settings = Settings {
        listen_port: 0,
        download_dir: default_dl,
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

async fn post_form(
    router: &axum::Router,
    uri: &str,
    cookie: &str,
    body: &str,
) -> (StatusCode, Vec<u8>) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.to_owned()))
        .expect("build POST");
    let resp = router.clone().oneshot(req).await.expect("POST");
    let status = resp.status();
    let body = resp
        .into_body()
        .collect()
        .await
        .expect("drain")
        .to_bytes()
        .to_vec();
    (status, body)
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

// Two distinct info-hashes so the test assertions are independent.
const MAGNET: &str = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Bunny";

/// URL-encode a path for a form body. We only need the handful of chars
/// tmpdir may produce (`/`, alnum, `-`, `_`).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// Poll `/torrents/info?category=X` until a row appears or we time out.
async fn await_info_row(router: &axum::Router, cookie: &str) -> Value {
    for _ in 0..50 {
        let v = get_json(router, "/api/v2/torrents/info", cookie).await;
        if let Some(arr) = v.as_array()
            && !arr.is_empty()
        {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("torrent never appeared in /info");
}

#[tokio::test]
async fn add_with_unknown_category_returns_409() {
    let (resume, reg, dl) = fresh_paths("unknown");
    let session = session_with_default(dl, resume, reg).await;
    let router = build_router(session);
    let sid = login(&router).await;

    let body = format!("urls={}&category=ghost", urlencode(MAGNET));
    let (status, resp_body) = post_form(&router, "/api/v2/torrents/add", &sid, &body).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let msg = String::from_utf8_lossy(&resp_body);
    assert!(
        msg.contains("ghost"),
        "409 body should mention the missing category name: got {msg:?}"
    );
}

#[tokio::test]
async fn add_with_known_category_uses_registry_save_path() {
    let (resume, reg, dl) = fresh_paths("known");
    let session = session_with_default(dl, resume, reg).await;
    session
        .create_category(
            "sonarr".to_string(),
            PathBuf::from("/tmp/irontide-m170-sonarr-known"),
        )
        .await
        .expect("create category");
    let router = build_router(session);
    let sid = login(&router).await;

    let body = format!("urls={}&category=sonarr", urlencode(MAGNET));
    let (status, _) = post_form(&router, "/api/v2/torrents/add", &sid, &body).await;
    assert_eq!(status, StatusCode::OK);

    let v = await_info_row(&router, &sid).await;
    let arr = v.as_array().unwrap();
    let sp = arr[0]
        .get("save_path")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(
        sp.starts_with("/tmp/irontide-m170-sonarr-known"),
        "expected category save_path, got {sp:?}"
    );
}

#[tokio::test]
async fn add_with_savepath_wins_over_category() {
    let (resume, reg, dl) = fresh_paths("savepath");
    let session = session_with_default(dl, resume, reg).await;
    session
        .create_category(
            "sonarr".to_string(),
            PathBuf::from("/tmp/irontide-m170-sonarr-ignored"),
        )
        .await
        .expect("create category");
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let explicit = "/tmp/irontide-m170-explicit";
    let body = format!(
        "urls={}&category=sonarr&savepath={}",
        urlencode(MAGNET),
        urlencode(explicit),
    );
    let (status, _) = post_form(&router, "/api/v2/torrents/add", &sid, &body).await;
    assert_eq!(status, StatusCode::OK);

    let v = await_info_row(&router, &sid).await;
    let arr = v.as_array().unwrap();
    let sp = arr[0]
        .get("save_path")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert_eq!(
        sp, explicit,
        "explicit savepath must beat category save_path"
    );

    // The category label is still recorded on the torrent.
    for _ in 0..50 {
        let v = get_json(&router, "/api/v2/torrents/info", &sid).await;
        let row = v.as_array().and_then(|a| a.first()).cloned().unwrap();
        if row.get("category").and_then(Value::as_str) == Some("sonarr") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("category label never propagated");
}

#[tokio::test]
async fn add_without_category_or_savepath_uses_default_download_dir() {
    let (resume, reg, dl) = fresh_paths("default");
    let dl_clone = dl.clone();
    let session = session_with_default(dl, resume, reg).await;
    let router = build_router(session);
    let sid = login(&router).await;

    let body = format!("urls={}", urlencode(MAGNET));
    let (status, _) = post_form(&router, "/api/v2/torrents/add", &sid, &body).await;
    assert_eq!(status, StatusCode::OK);

    let v = await_info_row(&router, &sid).await;
    let arr = v.as_array().unwrap();
    let sp = arr[0]
        .get("save_path")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert_eq!(
        sp,
        dl_clone.to_string_lossy(),
        "without category/savepath, save_path should equal Settings.download_dir"
    );
}

#[tokio::test]
async fn add_with_category_records_label_on_stats() {
    let (resume, reg, dl) = fresh_paths("labelled");
    let session = session_with_default(dl, resume, reg).await;
    session
        .create_category(
            "sonarr".to_string(),
            PathBuf::from("/tmp/irontide-m170-sonarr-labelled"),
        )
        .await
        .expect("create category");
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let body = format!("urls={}&category=sonarr", urlencode(MAGNET));
    let (status, _) = post_form(&router, "/api/v2/torrents/add", &sid, &body).await;
    assert_eq!(status, StatusCode::OK);

    // Lane A propagates the label via a fire-and-forget set_category task;
    // poll until it shows up on stats.
    for _ in 0..50 {
        let v = get_json(&router, "/api/v2/torrents/info", &sid).await;
        if let Some(row) = v.as_array().and_then(|a| a.first())
            && row.get("category").and_then(Value::as_str) == Some("sonarr")
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("category label never appeared on TorrentStats");
}
