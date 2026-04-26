//! Integration tests for `GET /api/v2/torrents/info?category=X` (M170 Lane D).
//!
//! Focus: the new `category=` filter in [`torrents::info`]. We seed a fresh
//! session with a pre-created category, add two torrents (one categorised,
//! one uncategorised), then poll the endpoint with each filter permutation.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use irontide::session::{SessionAddTorrentParams, SessionHandle, Settings};
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Every test gets its own resume/registry/download dir so parallel runs
/// can never observe each other's torrents or categories.
fn fresh_paths() -> (PathBuf, PathBuf) {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let resume_dir =
        std::env::temp_dir().join(format!("irontide-qbt-v2-info-cat-resume-{pid}-{n}"));
    let reg_path = std::env::temp_dir().join(format!("irontide-qbt-v2-info-cat-{pid}-{n}.toml"));
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
        .expect("cookie utf-8")
        .to_owned();
    let _ = resp.into_body().collect().await.expect("drain");
    cookie.split(';').next().expect("empty cookie").to_owned()
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
    let body = resp
        .into_body()
        .collect()
        .await
        .expect("drain")
        .to_bytes()
        .to_vec();
    serde_json::from_slice(&body).expect("json")
}

/// Add a magnet and wait for its category label to propagate on stats.
/// Lane A fires a post-add `set_category` task; it completes within a few
/// hundred ms on an unloaded box, so we poll up to 1 s.
async fn add_magnet_with_category(session: &SessionHandle, magnet: &str, category: Option<&str>) {
    let mut params = SessionAddTorrentParams::magnet(magnet);
    if let Some(name) = category {
        params = params.with_category(name);
    }
    let hash = session.add_torrent(params).await.expect("add magnet");
    if let Some(name) = category {
        for _ in 0..50 {
            if let Ok(stats) = session.torrent_stats(hash).await
                && stats.category.as_deref() == Some(name)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("category label did not propagate within 1s");
    }
}

// Two distinct magnets so we can tell them apart in the filter output.
const MAGNET_A: &str = "magnet:?xt=urn:btih:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa&dn=TorrentA";
const MAGNET_B: &str = "magnet:?xt=urn:btih:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb&dn=TorrentB";

#[tokio::test]
async fn category_filter_by_name_returns_only_matching() {
    let session = test_session().await;
    session
        .create_category("sonarr".to_string(), PathBuf::from("/tmp/sonarr-info-test"))
        .await
        .expect("create category");
    let router = build_router(session.clone());
    let sid = login(&router).await;

    add_magnet_with_category(&session, MAGNET_A, Some("sonarr")).await;
    add_magnet_with_category(&session, MAGNET_B, None).await;

    let v = get_json(&router, "/api/v2/torrents/info?category=sonarr", &sid).await;
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 1, "only the sonarr-tagged torrent should match");
    let cat = arr[0].get("category").and_then(Value::as_str).unwrap_or("");
    assert_eq!(cat, "sonarr");
}

#[tokio::test]
async fn category_filter_by_empty_string_returns_only_uncategorised() {
    let session = test_session().await;
    session
        .create_category(
            "sonarr".to_string(),
            PathBuf::from("/tmp/sonarr-info-empty"),
        )
        .await
        .expect("create category");
    let router = build_router(session.clone());
    let sid = login(&router).await;

    add_magnet_with_category(&session, MAGNET_A, Some("sonarr")).await;
    add_magnet_with_category(&session, MAGNET_B, None).await;

    let v = get_json(&router, "/api/v2/torrents/info?category=", &sid).await;
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 1, "only the uncategorised torrent should match");
    let cat = arr[0].get("category").and_then(Value::as_str).unwrap_or("");
    assert!(
        cat.is_empty(),
        "category on uncategorised torrent should serialise as empty string"
    );
}

#[tokio::test]
async fn category_filter_nonexistent_returns_empty_array() {
    let session = test_session().await;
    session
        .create_category("sonarr".to_string(), PathBuf::from("/tmp/sonarr-info-none"))
        .await
        .expect("create category");
    let router = build_router(session.clone());
    let sid = login(&router).await;

    add_magnet_with_category(&session, MAGNET_A, Some("sonarr")).await;

    // Filter names do not need to exist in the registry — qBt just returns
    // an empty array for anything that doesn't match a torrent's label.
    let v = get_json(&router, "/api/v2/torrents/info?category=radarr", &sid).await;
    assert_eq!(v.as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn no_category_param_returns_every_torrent() {
    let session = test_session().await;
    session
        .create_category("sonarr".to_string(), PathBuf::from("/tmp/sonarr-info-all"))
        .await
        .expect("create category");
    let router = build_router(session.clone());
    let sid = login(&router).await;

    add_magnet_with_category(&session, MAGNET_A, Some("sonarr")).await;
    add_magnet_with_category(&session, MAGNET_B, None).await;

    let v = get_json(&router, "/api/v2/torrents/info", &sid).await;
    assert_eq!(v.as_array().map(Vec::len), Some(2));
}
