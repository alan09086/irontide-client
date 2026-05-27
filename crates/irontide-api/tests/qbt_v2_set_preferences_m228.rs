//! M228: Integration tests for the 16 new setPreferences accept-fields
//! that wire M226's engine `Settings` expansion onto the qBt v2 wire.
//!
//! Each test POSTs a single-field patch and asserts the round-tripped GET
//! `/api/v2/app/preferences` payload reflects the change. This is the
//! simplest end-to-end contract: client writes, client reads, value matches
//! (with documented lossy compressions noted in-line).

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use irontide::session::Settings;
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

async fn enabled_router() -> (axum::Router, String) {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let resume_dir = std::env::temp_dir().join(format!(
        "irontide-qbt-v2-setprefs-m228-{}-{}",
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
    let username = settings.qbt_compat.username.clone();
    let session = irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("failed to start test session");
    let router = build_router(session);

    let form = format!("username={username}&password=adminadmin");
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form))
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
    let sid = cookie.split(';').next().unwrap().to_owned();
    (router, sid)
}

async fn post_json(
    router: &axum::Router,
    sid: &str,
    body: serde_json::Value,
) -> axum::http::Response<Body> {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/app/setPreferences")
        .header(header::COOKIE, sid)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    router.clone().oneshot(req).await.unwrap()
}

async fn get_prefs(router: &axum::Router, sid: &str) -> serde_json::Value {
    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/app/preferences")
        .header(header::COOKIE, sid)
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    serde_json::from_slice(&body).unwrap()
}

// ── 16 setPreferences round-trip tests ─────────────────────────────────

#[tokio::test]
async fn m228_set_preferences_accepts_notify_on_complete() {
    let (router, sid) = enabled_router().await;
    let resp = post_json(&router, &sid, serde_json::json!({"notify_on_complete": true})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["notify_on_complete"].as_bool(), Some(true));
}

#[tokio::test]
async fn m228_set_preferences_accepts_notify_on_error() {
    let (router, sid) = enabled_router().await;
    let resp = post_json(&router, &sid, serde_json::json!({"notify_on_error": true})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["notify_on_error"].as_bool(), Some(true));
}

#[tokio::test]
async fn m228_set_preferences_accepts_autorun_program_set_and_clear() {
    let (router, sid) = enabled_router().await;

    // Set
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({"autorun_program": "/usr/local/bin/notify.sh"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["autorun_program"].as_str(), Some("/usr/local/bin/notify.sh"));

    // Clear (empty string → None engine-side, empty string on GET projection)
    let resp = post_json(&router, &sid, serde_json::json!({"autorun_program": ""})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["autorun_program"].as_str(), Some(""));
}

#[tokio::test]
async fn m228_set_preferences_accepts_temp_path_enabled_combined() {
    // `temp_path_enabled=true` requires `incomplete_dir=Some(...)` per
    // Settings::validate; assert both fields land together in one POST.
    let (router, sid) = enabled_router().await;
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({"temp_path_enabled": true, "temp_path": "/var/incomplete"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["temp_path_enabled"].as_bool(), Some(true));
    assert_eq!(v["temp_path"].as_str(), Some("/var/incomplete"));
}

#[tokio::test]
async fn m228_set_preferences_accepts_temp_path_set_and_clear() {
    let (router, sid) = enabled_router().await;

    // Set (use_incomplete_dir defaults false, so validate doesn't require this)
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({"temp_path": "/var/incomplete-only"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["temp_path"].as_str(), Some("/var/incomplete-only"));

    // Clear
    let resp = post_json(&router, &sid, serde_json::json!({"temp_path": ""})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["temp_path"].as_str(), Some(""));
}

#[tokio::test]
async fn m228_set_preferences_accepts_add_skip_check() {
    let (router, sid) = enabled_router().await;
    let resp = post_json(&router, &sid, serde_json::json!({"add_skip_check": true})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["add_skip_check"].as_bool(), Some(true));
}

#[tokio::test]
async fn m228_set_preferences_accepts_incomplete_files_ext() {
    let (router, sid) = enabled_router().await;
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({"incomplete_files_ext": true}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["incomplete_files_ext"].as_bool(), Some(true));
}

#[tokio::test]
async fn m228_set_preferences_accepts_scan_dirs_v2_set_and_clear() {
    let (router, sid) = enabled_router().await;

    // Set — use /var/watched (absolute, not in H6 system-path DENY list).
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({"scan_dirs_v2": "/var/watched"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["scan_dirs_v2"].as_str(), Some("/var/watched"));

    // Clear
    let resp = post_json(&router, &sid, serde_json::json!({"scan_dirs_v2": ""})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["scan_dirs_v2"].as_str(), Some(""));
}

#[tokio::test]
async fn m228_set_preferences_accepts_auto_delete_mode_round_trip() {
    let (router, sid) = enabled_router().await;

    // 0 → false → 0 (stable)
    let resp = post_json(&router, &sid, serde_json::json!({"auto_delete_mode": 0})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        get_prefs(&router, &sid).await["auto_delete_mode"].as_i64(),
        Some(0)
    );

    // 2 → true → 2 (stable)
    let resp = post_json(&router, &sid, serde_json::json!({"auto_delete_mode": 2})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        get_prefs(&router, &sid).await["auto_delete_mode"].as_i64(),
        Some(2)
    );

    // 1 → true → 2 (lossy compression; documented in app.rs M228 docstring)
    let resp = post_json(&router, &sid, serde_json::json!({"auto_delete_mode": 1})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        get_prefs(&router, &sid).await["auto_delete_mode"].as_i64(),
        Some(2)
    );
}

#[tokio::test]
async fn m228_set_preferences_accepts_move_completed_enabled_combined() {
    // `move_completed_enabled=true` requires `move_completed_to=Some(...)`
    // per Settings::validate; assert both land together.
    let (router, sid) = enabled_router().await;
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({
            "move_completed_enabled": true,
            "save_path_completed": "/var/moved",
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["move_completed_enabled"].as_bool(), Some(true));
    assert_eq!(v["save_path_completed"].as_str(), Some("/var/moved"));
}

#[tokio::test]
async fn m228_set_preferences_accepts_save_path_completed_set_and_clear() {
    let (router, sid) = enabled_router().await;

    // Set (move_completed_enabled defaults false, so validate doesn't require this)
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({"save_path_completed": "/var/moved-only"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["save_path_completed"].as_str(), Some("/var/moved-only"));

    // Clear
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({"save_path_completed": ""}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["save_path_completed"].as_str(), Some(""));
}

#[tokio::test]
async fn m228_set_preferences_accepts_use_https() {
    let (router, sid) = enabled_router().await;
    let resp = post_json(&router, &sid, serde_json::json!({"use_https": true})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["use_https"].as_bool(), Some(true));
}

#[tokio::test]
async fn m228_set_preferences_accepts_current_network_interface_set_and_clear() {
    let (router, sid) = enabled_router().await;

    // Set
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({"current_network_interface": "eth0"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["current_network_interface"].as_str(), Some("eth0"));

    // Clear
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({"current_network_interface": ""}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["current_network_interface"].as_str(), Some(""));
}

#[tokio::test]
async fn m228_set_preferences_accepts_add_stopped_enabled_asymmetric_get_emits_start_paused() {
    // setPreferences accepts `add_stopped_enabled`; GET emits as
    // `start_paused_enabled` (the M226-wired name). This asymmetry is
    // intentional and matches qBt's real wire surface.
    let (router, sid) = enabled_router().await;
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({"add_stopped_enabled": true}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["start_paused_enabled"].as_bool(), Some(true));
    // Confirm the wire-symmetric name is NOT projected.
    assert!(v.get("add_stopped_enabled").is_none());
}

#[tokio::test]
async fn m228_set_preferences_accepts_preallocate_all_round_trip() {
    let (router, sid) = enabled_router().await;

    // true → Some(Full) → true on GET
    let resp = post_json(&router, &sid, serde_json::json!({"preallocate_all": true})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        get_prefs(&router, &sid).await["preallocate_all"].as_bool(),
        Some(true)
    );

    // false → None → false on GET (let derivation from storage_mode take over)
    let resp = post_json(&router, &sid, serde_json::json!({"preallocate_all": false})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        get_prefs(&router, &sid).await["preallocate_all"].as_bool(),
        Some(false)
    );
}

#[tokio::test]
async fn m228_set_preferences_accepts_ip_filter_auto_refresh() {
    let (router, sid) = enabled_router().await;
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({"ip_filter_auto_refresh": true}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v["ip_filter_auto_refresh"].as_bool(), Some(true));
}
