//! Integration tests for M171 preferences wiring.
//!
//! Covers D1a (`seed_time_limit_secs` / `inactive_seed_time_limit_secs` →
//! `max_seeding_time*` wire fields) and D2 (the remaining `FIXME(M171)`
//! markers wired to real Settings: `max_ratio_act`, `queueing_enabled`,
//! `create_subfolder_enabled`, `auto_tmm_enabled`).

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use irontide::session::Settings;
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

async fn enabled_router_with(
    customize: impl FnOnce(&mut Settings),
) -> (axum::Router, String) {
    let creds: (String, String);
    let session = {
        let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        let resume_dir = std::env::temp_dir().join(format!(
            "irontide-qbt-v2-prefs-m171-{}-{}",
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
        customize(&mut settings);
        creds = (
            settings.qbt_compat.username.clone(),
            settings.qbt_compat.password.clone(),
        );
        irontide::ClientBuilder::from_settings(settings)
            .start()
            .await
            .expect("failed to start test session")
    };
    let router = build_router(session);
    let form = format!("username={}&password={}", creds.0, creds.1);
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

async fn get_prefs(router: &axum::Router, sid: &str) -> serde_json::Value {
    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/app/preferences")
        .header(header::COOKIE, sid)
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
    serde_json::from_slice(&body).unwrap()
}

// ── D1a: seed-time limits ─────────────────────────────────────────────

#[tokio::test]
async fn preferences_max_seeding_time_disabled_when_none() {
    let (router, sid) = enabled_router_with(|s| {
        s.seed_time_limit_secs = None;
    })
    .await;
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("max_seeding_time_enabled").and_then(|b| b.as_bool()),
        Some(false)
    );
    assert_eq!(
        v.get("max_seeding_time").and_then(|i| i.as_i64()),
        Some(-1)
    );
}

#[tokio::test]
async fn preferences_max_seeding_time_seconds_to_minutes() {
    // 1 hour = 3600 seconds = 60 minutes on the wire.
    let (router, sid) = enabled_router_with(|s| {
        s.seed_time_limit_secs = Some(3600);
    })
    .await;
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("max_seeding_time_enabled").and_then(|b| b.as_bool()),
        Some(true)
    );
    assert_eq!(
        v.get("max_seeding_time").and_then(|i| i.as_i64()),
        Some(60),
        "qBt wire format is MINUTES, not seconds"
    );
}

#[tokio::test]
async fn preferences_max_inactive_seeding_time_disabled_when_none() {
    let (router, sid) = enabled_router_with(|s| {
        s.inactive_seed_time_limit_secs = None;
    })
    .await;
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("max_inactive_seeding_time_enabled")
            .and_then(|b| b.as_bool()),
        Some(false)
    );
    assert_eq!(
        v.get("max_inactive_seeding_time").and_then(|i| i.as_i64()),
        Some(-1)
    );
}

#[tokio::test]
async fn preferences_max_inactive_seeding_time_seconds_to_minutes() {
    // 30 minutes = 1800 seconds = 30 minutes on the wire.
    let (router, sid) = enabled_router_with(|s| {
        s.inactive_seed_time_limit_secs = Some(1800);
    })
    .await;
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("max_inactive_seeding_time_enabled")
            .and_then(|b| b.as_bool()),
        Some(true)
    );
    assert_eq!(
        v.get("max_inactive_seeding_time").and_then(|i| i.as_i64()),
        Some(30)
    );
}

// ── D2: max_ratio_act / queueing_enabled / create_subfolder / auto_tmm ──

#[tokio::test]
async fn preferences_max_ratio_act_default_is_pause() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("max_ratio_act").and_then(|s| s.as_str()),
        Some("pause")
    );
}

#[tokio::test]
async fn preferences_max_ratio_act_remove_round_trip() {
    use irontide::session::MaxRatioAction;
    let (router, sid) = enabled_router_with(|s| {
        s.max_ratio_action = MaxRatioAction::Remove;
    })
    .await;
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("max_ratio_act").and_then(|s| s.as_str()),
        Some("remove")
    );
}

#[tokio::test]
async fn preferences_max_ratio_act_enable_super_seeding_round_trip() {
    use irontide::session::MaxRatioAction;
    let (router, sid) = enabled_router_with(|s| {
        s.max_ratio_action = MaxRatioAction::EnableSuperSeeding;
    })
    .await;
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("max_ratio_act").and_then(|s| s.as_str()),
        Some("enable_super_seeding")
    );
}

#[tokio::test]
async fn preferences_reflects_real_settings_d2() {
    // Apply non-default values to all four D2 fields at once and verify
    // every one appears in the preferences response.
    use irontide::session::MaxRatioAction;
    let (router, sid) = enabled_router_with(|s| {
        s.max_ratio_action = MaxRatioAction::Remove;
        s.queueing_enabled = true;
        s.create_subfolder = false;
        s.auto_manage_torrents = true;
    })
    .await;
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("max_ratio_act").and_then(|s| s.as_str()),
        Some("remove")
    );
    assert_eq!(
        v.get("queueing_enabled").and_then(|b| b.as_bool()),
        Some(true)
    );
    assert_eq!(
        v.get("create_subfolder_enabled").and_then(|b| b.as_bool()),
        Some(false)
    );
    assert_eq!(
        v.get("auto_tmm_enabled").and_then(|b| b.as_bool()),
        Some(true)
    );
}
