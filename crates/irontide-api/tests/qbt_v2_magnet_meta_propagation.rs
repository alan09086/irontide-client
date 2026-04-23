//! v0.173.2 Class A regression: magnet-added torrents must expose
//! `/api/v2/torrents/files` once metadata resolves, AND `deleteFiles=true`
//! must actually remove files from disk.
//!
//! Architecturally fixed in v0.173.1 (TorrentEntry.meta deleted).
//! These tests lock in that the regression cannot silently re-land.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::{inject_magnet_and_resolve_meta, make_test_settings};
use irontide::session::SessionHandle;
use irontide_api::routes::build_router;

async fn fresh_session() -> (SessionHandle, std::path::PathBuf) {
    let (mut settings, dl_dir) = make_test_settings("qbt-v2-mp");
    settings.qbt_compat.enabled = true;
    let session = SessionHandle::start(settings)
        .await
        .expect("start session");
    (session, dl_dir)
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

async fn get(router: &axum::Router, uri: &str, cookie: &str) -> (StatusCode, Vec<u8>) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .expect("build GET");
    let resp = router.clone().oneshot(req).await.expect("GET");
    let status = resp.status();
    let body = resp.into_body().collect().await.expect("drain").to_bytes().to_vec();
    (status, body)
}

async fn post(router: &axum::Router, uri: &str, cookie: &str) -> (StatusCode, Vec<u8>) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .expect("build POST");
    let resp = router.clone().oneshot(req).await.expect("POST");
    let status = resp.status();
    let body = resp.into_body().collect().await.expect("drain").to_bytes().to_vec();
    (status, body)
}

#[tokio::test]
async fn magnet_files_endpoint_200_after_metadata_resolves() {
    let (session, _dl) = fresh_session().await;
    let hash = inject_magnet_and_resolve_meta(
        &session,
        "archlinux-2026.04.01-x86_64.iso",
        1_536_851_968,
    ).await;

    let router = build_router(session);
    let sid = login(&router).await;
    let (status, body) = get(
        &router,
        &format!("/api/v2/torrents/files?hash={}", hash.to_hex()),
        &sid,
    ).await;
    assert_eq!(
        status, StatusCode::OK,
        "/files must return 200 for magnet-added torrents post-metadata; body={:?}",
        String::from_utf8_lossy(&body)
    );
    let files: Vec<serde_json::Value> = serde_json::from_slice(&body).expect("json");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["name"].as_str(), Some("archlinux-2026.04.01-x86_64.iso"));
}

#[tokio::test]
async fn magnet_delete_files_actually_removes_files() {
    let (session, dl_dir) = fresh_session().await;
    let hash = inject_magnet_and_resolve_meta(
        &session,
        "archlinux-2026.04.01-x86_64.iso",
        16_384,
    ).await;
    let target = dl_dir.join("archlinux-2026.04.01-x86_64.iso");
    std::fs::write(&target, b"pretend-this-is-an-iso").expect("write fixture");
    assert!(target.exists());

    let router = build_router(session);
    let sid = login(&router).await;
    let (status, _) = post(
        &router,
        &format!("/api/v2/torrents/delete?hashes={}&deleteFiles=true", hash.to_hex()),
        &sid,
    ).await;
    assert_eq!(status, StatusCode::OK);

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert!(
        !target.exists(),
        "deleteFiles=true must remove on-disk file for magnet-added torrent; \
         still at {target:?}"
    );
}
