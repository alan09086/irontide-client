//! /api/v1 regression smoke tests (M168 Task 19).
//!
//! Proves the /api/v1 surface has zero behavioural change whether
//! `qbt_compat` is enabled or disabled. If a v1 endpoint ever starts
//! responding differently under the M168 feature flag, these tests
//! break — fail loud rather than drift silently.

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use irontide::session::Settings;
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

async fn test_session_with_qbt(enabled: bool) -> irontide::session::SessionHandle {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let resume_dir =
        std::env::temp_dir().join(format!("irontide-qbt-v2-reg-{}-{}", std::process::id(), n));
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
    settings.qbt_compat.enabled = enabled;
    irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("failed to start test session")
}

async fn send(router: &axum::Router, req: Request<Body>) -> (StatusCode, Vec<u8>) {
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, body)
}

#[tokio::test]
async fn v1_list_torrents_unaffected_when_qbt_compat_enabled() {
    // Baseline (disabled): empty list with 200.
    let router_off = build_router(test_session_with_qbt(false).await);
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/torrents")
        .body(Body::empty())
        .unwrap();
    let (status_off, body_off) = send(&router_off, req).await;
    assert_eq!(status_off, StatusCode::OK);
    let v_off: serde_json::Value = serde_json::from_slice(&body_off).unwrap();
    assert!(v_off.is_array());
    assert_eq!(v_off.as_array().unwrap().len(), 0);

    // With qbt_compat enabled: behaviour must be byte-identical.
    let router_on = build_router(test_session_with_qbt(true).await);
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/torrents")
        .body(Body::empty())
        .unwrap();
    let (status_on, body_on) = send(&router_on, req).await;
    assert_eq!(status_on, StatusCode::OK);
    let v_on: serde_json::Value = serde_json::from_slice(&body_on).unwrap();
    assert_eq!(
        v_off, v_on,
        "v1 list must be identical under qbt_compat toggles"
    );
}

#[tokio::test]
async fn v1_patch_settings_unaffected_when_qbt_compat_disabled() {
    let router = build_router(test_session_with_qbt(false).await);

    // PATCH /api/v1/session/settings with an unrelated field; accepts even
    // when qbt_compat disabled (v1 is not gated by the qbt_gate middleware).
    let patch = serde_json::json!({ "enable_pex": false });
    let req = Request::builder()
        .method("PATCH")
        .uri("/api/v1/session/settings")
        .header(header::CONTENT_TYPE, "application/merge-patch+json")
        .body(Body::from(patch.to_string()))
        .unwrap();
    let (status, _) = send(&router, req).await;
    assert!(
        status.is_success(),
        "v1 patch settings must succeed regardless of qbt_compat: {status}"
    );

    // Confirm the setting took effect.
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/session/settings")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(&router, req).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        v.get("enable_pex").and_then(serde_json::Value::as_bool),
        Some(false)
    );
}
