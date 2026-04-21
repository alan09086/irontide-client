//! Integration tests for the qBt v2 `/api/v2/app/*` surface (M168 Task 6+7).

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use irontide::session::Settings;
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

async fn enabled_router_with(customize: impl FnOnce(&mut Settings)) -> (axum::Router, String) {
    // Capture the username from the customized Settings. The plaintext
    // "adminadmin" is hardcoded because M172a ships `password_hash` with the
    // pre-hashed default — callers who rotate the password must rotate the
    // hash (via `hash_qbt_password`) OR supply a legacy plaintext in the
    // same Settings customize closure and we'd need a different helper.
    let username: String;
    let session = {
        let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        let resume_dir =
            std::env::temp_dir().join(format!("irontide-qbt-v2-app-{}-{}", std::process::id(), n));
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
        username = settings.qbt_compat.username.clone();
        irontide::ClientBuilder::from_settings(settings)
            .start()
            .await
            .expect("failed to start test session")
    };
    let router = build_router(session);
    // M172a: default `password_hash` matches "adminadmin".
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

async fn get(
    router: &axum::Router,
    uri: &str,
    cookie: Option<&str>,
) -> (StatusCode, Vec<u8>, axum::http::HeaderMap) {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }
    let req = builder.body(Body::empty()).unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, body, headers)
}

// ── Task 6: app/version, app/webapiVersion, app/buildInfo tests ─────

#[tokio::test]
async fn app_version_plaintext_with_v_prefix() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, body, _) = get(&router, "/api/v2/app/version", Some(&sid)).await;
    assert_eq!(status, StatusCode::OK);
    let s = String::from_utf8(body).unwrap();
    assert!(s.starts_with('v'), "got: {s}");
    assert_eq!(s, "v5.1.4");
}

#[tokio::test]
async fn app_webapi_version_plaintext_no_prefix() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, body, _) = get(&router, "/api/v2/app/webapiVersion", Some(&sid)).await;
    assert_eq!(status, StatusCode::OK);
    let s = String::from_utf8(body).unwrap();
    assert!(
        !s.starts_with('v'),
        "webapi version must not start with v: {s}"
    );
    assert_eq!(s, "2.11.4");
}

#[tokio::test]
async fn app_build_info_json_shape_all_five_keys() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, body, _) = get(&router, "/api/v2/app/buildInfo", Some(&sid)).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    for key in ["qt", "libtorrent", "boost", "openssl", "bitness"] {
        assert!(v.get(key).is_some(), "missing key {key} in {v:?}");
    }
}

#[tokio::test]
async fn app_build_info_bitness_matches_usize_bits() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (_, body, _) = get(&router, "/api/v2/app/buildInfo", Some(&sid)).await;
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let bitness = v.get("bitness").and_then(|b| b.as_u64()).unwrap();
    let expected = (std::mem::size_of::<usize>() as u64) * 8;
    assert_eq!(bitness, expected);
}

#[tokio::test]
async fn app_version_requires_sid() {
    let (router, _) = enabled_router_with(|_| {}).await;
    let (status, _, _) = get(&router, "/api/v2/app/version", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn app_build_info_requires_sid() {
    let (router, _) = enabled_router_with(|_| {}).await;
    let (status, _, _) = get(&router, "/api/v2/app/buildInfo", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ── Task 7: app/preferences tests ─────────────────────────────────────

#[tokio::test]
async fn preferences_contains_all_arr_required_fields() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, body, _) = get(&router, "/api/v2/app/preferences", Some(&sid)).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // *arr mandatory fields — missing any one would crash Radarr's JSON
    // deserialiser at Test Connection.
    for key in [
        "save_path",
        "dht",
        "pex",
        "lsd",
        "upnp",
        "listen_port",
        "max_ratio",
        "max_ratio_enabled",
        "encryption",
        "web_ui_username",
        "max_ratio_act",
        "create_subfolder_enabled",
        "start_paused_enabled",
        "auto_tmm_enabled",
    ] {
        assert!(v.get(key).is_some(), "missing key {key} in preferences");
    }
}

#[tokio::test]
async fn preferences_save_path_maps_from_download_dir() {
    let (router, sid) = enabled_router_with(|s| {
        s.download_dir = std::path::PathBuf::from("/var/lib/irontide/dl");
    })
    .await;
    let (_, body, _) = get(&router, "/api/v2/app/preferences", Some(&sid)).await;
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        v.get("save_path").and_then(|s| s.as_str()),
        Some("/var/lib/irontide/dl")
    );
}

#[tokio::test]
async fn preferences_max_ratio_enabled_follows_seed_ratio_limit_presence() {
    // Case 1: no ratio limit — max_ratio_enabled must be false.
    let (router, sid) = enabled_router_with(|s| s.seed_ratio_limit = None).await;
    let (_, body, _) = get(&router, "/api/v2/app/preferences", Some(&sid)).await;
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        v.get("max_ratio_enabled").and_then(|b| b.as_bool()),
        Some(false)
    );
    assert_eq!(v.get("max_ratio").and_then(|r| r.as_f64()), Some(-1.0));

    // Case 2: ratio limit set — flag is true, value matches.
    let (router, sid) = enabled_router_with(|s| s.seed_ratio_limit = Some(2.5)).await;
    let (_, body, _) = get(&router, "/api/v2/app/preferences", Some(&sid)).await;
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        v.get("max_ratio_enabled").and_then(|b| b.as_bool()),
        Some(true)
    );
    assert!((v.get("max_ratio").and_then(|r| r.as_f64()).unwrap() - 2.5_f64).abs() < 1e-9);
}

#[tokio::test]
async fn preferences_encryption_maps_prefer_to_0() {
    use irontide::prelude::EncryptionMode;
    let (router, sid) = enabled_router_with(|s| s.encryption_mode = EncryptionMode::Enabled).await;
    let (_, body, _) = get(&router, "/api/v2/app/preferences", Some(&sid)).await;
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v.get("encryption").and_then(|n| n.as_u64()), Some(0));
}

#[tokio::test]
async fn preferences_encryption_maps_force_to_1() {
    use irontide::prelude::EncryptionMode;
    let (router, sid) = enabled_router_with(|s| s.encryption_mode = EncryptionMode::Forced).await;
    let (_, body, _) = get(&router, "/api/v2/app/preferences", Some(&sid)).await;
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v.get("encryption").and_then(|n| n.as_u64()), Some(1));
}

#[tokio::test]
async fn preferences_encryption_maps_disable_to_2() {
    use irontide::prelude::EncryptionMode;
    let (router, sid) = enabled_router_with(|s| s.encryption_mode = EncryptionMode::Disabled).await;
    let (_, body, _) = get(&router, "/api/v2/app/preferences", Some(&sid)).await;
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v.get("encryption").and_then(|n| n.as_u64()), Some(2));
}

#[tokio::test]
async fn preferences_non_utf8_download_dir_lossy_survives() {
    // On Linux, PathBuf is bytes-oriented — craft an invalid-UTF-8 path.
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let invalid = std::ffi::OsString::from_vec(vec![b'/', b't', 0x80, b'm', b'p']);
        let path = std::path::PathBuf::from(invalid);

        let (router, sid) = enabled_router_with(|s| s.download_dir = path.clone()).await;
        let (status, body, _) = get(&router, "/api/v2/app/preferences", Some(&sid)).await;
        assert_eq!(status, StatusCode::OK);
        // Lossy conversion inserts U+FFFD for the invalid byte.
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let save = v.get("save_path").and_then(|s| s.as_str()).unwrap();
        assert!(save.contains('t'), "got: {save}");
    }
    #[cfg(not(unix))]
    {
        // Platform without invalid-UTF-8 paths — trivially passing.
    }
}

#[tokio::test]
async fn preferences_requires_sid() {
    let (router, _) = enabled_router_with(|_| {}).await;
    let (status, _, _) = get(&router, "/api/v2/app/preferences", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ── Task 8: torrents/categories shim ──────────────────────────────────

#[tokio::test]
async fn categories_returns_empty_object_with_json_ctype() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, body, headers) = get(&router, "/api/v2/torrents/categories", Some(&sid)).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.is_object(), "expected JSON object, got: {v:?}");
    let obj = v.as_object().unwrap();
    assert!(obj.is_empty(), "expected empty map, got: {v:?}");
    let ct = headers.get(header::CONTENT_TYPE).unwrap().to_str().unwrap();
    assert!(ct.contains("application/json"), "got: {ct}");
}

#[tokio::test]
async fn categories_requires_sid() {
    let (router, _) = enabled_router_with(|_| {}).await;
    let (status, _, _) = get(&router, "/api/v2/torrents/categories", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn preferences_web_ui_username_echoes_config() {
    let (router, sid) = enabled_router_with(|s| {
        s.qbt_compat.username = "radarr-test".into();
    })
    .await;
    let (_, body, _) = get(&router, "/api/v2/app/preferences", Some(&sid)).await;
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        v.get("web_ui_username").and_then(|s| s.as_str()),
        Some("radarr-test")
    );
}
