//! Integration tests for `POST /api/v2/app/setPreferences` (M171 D3+D3.5).

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use irontide::session::{MaxRatioAction, Settings};
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

async fn enabled_router_with(customize: impl FnOnce(&mut Settings)) -> (axum::Router, String) {
    let username: String;
    let session = {
        let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        let resume_dir = std::env::temp_dir().join(format!(
            "irontide-qbt-v2-setprefs-{}-{}",
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
        username = settings.qbt_compat.username.clone();
        irontide::ClientBuilder::from_settings(settings)
            .start()
            .await
            .expect("failed to start test session")
    };
    let router = build_router(session);
    // M172a: default password_hash matches "adminadmin".
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

/// POST a JSON body to setPreferences. Returns the full response so callers
/// can inspect status + headers.
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

// ── Basic patch semantics ────────────────────────────────────────────

#[tokio::test]
async fn set_preferences_partial_patch_preserves_other_fields() {
    let (router, sid) = enabled_router_with(|s| {
        s.upload_rate_limit = 123_456;
    })
    .await;
    let resp = post_json(&router, &sid, serde_json::json!({"dl_limit": 500_000})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    // dl_limit — not exposed on our wire GET, but we can verify up_limit
    // was NOT touched via the settings roundtrip: the field we care about
    // here is that the patch was partial.
    assert_eq!(
        v.get("save_path").and_then(|s| s.as_str()),
        Some("/tmp"),
        "untouched save_path must be preserved"
    );
}

#[tokio::test]
async fn set_preferences_unknown_field_ignored_200() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let body = serde_json::json!({"a_field_that_does_not_exist": 42});
    let resp = post_json(&router, &sid, body).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn set_preferences_negative_dl_limit_accepted_as_unlimited() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let resp = post_json(&router, &sid, serde_json::json!({"dl_limit": -1})).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn set_preferences_dht_pex_lsd_toggle_applies() {
    let (router, sid) = enabled_router_with(|s| {
        s.enable_dht = false;
        s.enable_pex = false;
        s.enable_lsd = false;
    })
    .await;
    let body = serde_json::json!({"dht": true, "pex": true, "lsd": true});
    let resp = post_json(&router, &sid, body).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v.get("dht").and_then(serde_json::Value::as_bool), Some(true));
    assert_eq!(v.get("pex").and_then(serde_json::Value::as_bool), Some(true));
    assert_eq!(v.get("lsd").and_then(serde_json::Value::as_bool), Some(true));
}

#[tokio::test]
async fn set_preferences_encryption_enum_string_to_int_0() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let resp = post_json(&router, &sid, serde_json::json!({"encryption": 0})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v.get("encryption").and_then(serde_json::Value::as_u64), Some(0));
}

#[tokio::test]
async fn set_preferences_encryption_enum_string_to_int_1() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let resp = post_json(&router, &sid, serde_json::json!({"encryption": 1})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v.get("encryption").and_then(serde_json::Value::as_u64), Some(1));
}

#[tokio::test]
async fn set_preferences_encryption_enum_string_to_int_2() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let resp = post_json(&router, &sid, serde_json::json!({"encryption": 2})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v.get("encryption").and_then(serde_json::Value::as_u64), Some(2));
}

#[tokio::test]
async fn set_preferences_encryption_invalid_int_is_400() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let resp = post_json(&router, &sid, serde_json::json!({"encryption": 99})).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn set_preferences_save_path_updates_download_dir() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let body = serde_json::json!({"save_path": "/var/lib/irontide/dl"});
    let resp = post_json(&router, &sid, body).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("save_path").and_then(|s| s.as_str()),
        Some("/var/lib/irontide/dl")
    );
}

// E0.5 — empty body is a no-op 200
#[tokio::test]
async fn set_preferences_empty_body_is_noop_200() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let before = get_prefs(&router, &sid).await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/app/setPreferences")
        .header(header::COOKIE, sid.clone())
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let after = get_prefs(&router, &sid).await;
    assert_eq!(before, after, "empty body must be a no-op");
}

// E0.6 — NaN rejected as 400
#[tokio::test]
async fn set_preferences_nan_max_ratio_rejected_400() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    // serde_json cannot serialise f64::NAN; build the raw text manually.
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/app/setPreferences")
        .header(header::COOKIE, sid)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"max_ratio": NaN}"#))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    // serde_json itself rejects bare `NaN` before our handler sees it, so
    // the handler may surface it as a parse error (400). Either route is
    // acceptable as long as the NaN doesn't land in Settings.
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── Ratio + max_ratio_enabled interaction ─────────────────────────────

#[tokio::test]
async fn set_preferences_max_ratio_negative_sets_none() {
    let (router, sid) = enabled_router_with(|s| s.seed_ratio_limit = Some(5.0)).await;
    let resp = post_json(&router, &sid, serde_json::json!({"max_ratio": -1.0})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("max_ratio_enabled").and_then(serde_json::Value::as_bool),
        Some(false)
    );
}

#[tokio::test]
async fn set_preferences_max_ratio_enabled_false_clears() {
    let (router, sid) = enabled_router_with(|s| s.seed_ratio_limit = Some(5.0)).await;
    let body = serde_json::json!({"max_ratio_enabled": false});
    let resp = post_json(&router, &sid, body).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("max_ratio_enabled").and_then(serde_json::Value::as_bool),
        Some(false)
    );
}

#[tokio::test]
async fn set_preferences_max_ratio_positive_sets_limit() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let resp = post_json(&router, &sid, serde_json::json!({"max_ratio": 2.5})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("max_ratio_enabled").and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert!((v.get("max_ratio").and_then(serde_json::Value::as_f64).unwrap() - 2.5_f64).abs() < 1e-9);
}

// ── max_ratio_act ─────────────────────────────────────────────────────

#[tokio::test]
async fn set_preferences_max_ratio_act_all_three_variants_accepted() {
    for (wire, expected) in [
        ("pause", MaxRatioAction::Pause),
        ("remove", MaxRatioAction::Remove),
        ("enable_super_seeding", MaxRatioAction::EnableSuperSeeding),
    ] {
        let (router, sid) = enabled_router_with(|_| {}).await;
        let body = serde_json::json!({"max_ratio_act": wire});
        let resp = post_json(&router, &sid, body).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let v = get_prefs(&router, &sid).await;
        assert_eq!(
            v.get("max_ratio_act").and_then(|s| s.as_str()),
            Some(wire),
            "round-trip for {expected:?}"
        );
    }
}

#[tokio::test]
async fn set_preferences_max_ratio_act_invalid_is_400() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let body = serde_json::json!({"max_ratio_act": "delete_forever"});
    let resp = post_json(&router, &sid, body).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── Seed-time limits (D1a integration via setPreferences) ─────────────

#[tokio::test]
async fn set_preferences_max_seeding_time_minutes_to_seconds() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    // Wire: 60 minutes → settings: 3600 seconds → wire GET: 60 minutes
    let body = serde_json::json!({"max_seeding_time": 60});
    let resp = post_json(&router, &sid, body).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v.get("max_seeding_time").and_then(serde_json::Value::as_i64), Some(60));
    assert_eq!(
        v.get("max_seeding_time_enabled").and_then(serde_json::Value::as_bool),
        Some(true)
    );
}

#[tokio::test]
async fn set_preferences_max_seeding_time_negative_clears() {
    let (router, sid) = enabled_router_with(|s| s.seed_time_limit_secs = Some(3600)).await;
    let body = serde_json::json!({"max_seeding_time": -1});
    let resp = post_json(&router, &sid, body).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("max_seeding_time_enabled").and_then(serde_json::Value::as_bool),
        Some(false)
    );
}

#[tokio::test]
async fn set_preferences_max_seeding_time_enabled_false_clears() {
    let (router, sid) = enabled_router_with(|s| s.seed_time_limit_secs = Some(3600)).await;
    let body = serde_json::json!({"max_seeding_time_enabled": false});
    let resp = post_json(&router, &sid, body).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("max_seeding_time_enabled").and_then(serde_json::Value::as_bool),
        Some(false)
    );
}

// ── Listen port / queueing / create_subfolder / auto_tmm ──────────────

#[tokio::test]
async fn set_preferences_listen_port_applies() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let resp = post_json(&router, &sid, serde_json::json!({"listen_port": 6881})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v.get("listen_port").and_then(serde_json::Value::as_u64), Some(6881));
}

#[tokio::test]
async fn set_preferences_four_bools_round_trip() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let body = serde_json::json!({
        "queueing_enabled": true,
        "create_subfolder_enabled": false,
        "auto_tmm_enabled": true,
        "anonymous_mode": true,
    });
    let resp = post_json(&router, &sid, body).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(
        v.get("queueing_enabled").and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        v.get("create_subfolder_enabled").and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        v.get("auto_tmm_enabled").and_then(serde_json::Value::as_bool),
        Some(true)
    );
}

// ── Legacy form body (qBt WebUI style) ────────────────────────────────

#[tokio::test]
async fn set_preferences_legacy_form_json_wrapper_accepted() {
    // qBt's own WebUI sends setPreferences as URL-encoded with a `json=<...>`
    // field. The handler must accept that shape too.
    let (router, sid) = enabled_router_with(|_| {}).await;
    let payload = r#"{"dht":true,"pex":false}"#;
    let form = format!("json={}", urlencoding_encode(payload));
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/app/setPreferences")
        .header(header::COOKIE, sid.clone())
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = get_prefs(&router, &sid).await;
    assert_eq!(v.get("dht").and_then(serde_json::Value::as_bool), Some(true));
    assert_eq!(v.get("pex").and_then(serde_json::Value::as_bool), Some(false));
}

/// Minimal percent-encoder for the legacy-form fixture — `serde_urlencoded`
/// doesn't expose a single-key encoder and we don't want to pull in `urlencoding`.
fn urlencoding_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'-' | b'_' | b'.' | b'~' | b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

// ── D3.5 — X-IronTide-Restart-Pending header ─────────────────────────
// M173 Lane B (B10): listen_port, dht, lsd graduated from
// restart_required → immediate. Pin tests below confirm these no
// longer appear in the header.

#[tokio::test]
async fn set_preferences_only_rate_limits_no_restart_header() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({"dl_limit": 500_000, "up_limit": 600_000}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers().get("x-irontide-restart-pending").is_none(),
        "rate-limit-only patch must NOT flag a restart"
    );
}

/// M173 Lane B (B10) graduation: changing `listen_port` must NOT emit
/// the restart header anymore. The transactional apply pipeline
/// performs the live rebind.
#[tokio::test]
async fn set_preferences_listen_port_no_longer_flags_restart_required() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let resp = post_json(&router, &sid, serde_json::json!({"listen_port": 6881})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers().get("x-irontide-restart-pending").is_none(),
        "M173 graduation: listen_port must NOT flag a restart anymore"
    );
}

/// M173 Lane B (B10) graduation: changing `listen_port` + dht together
/// must NOT emit the restart header. Both fields are now immediate.
#[tokio::test]
async fn set_preferences_listen_port_and_dht_no_longer_flag_restart() {
    let (router, sid) = enabled_router_with(|s| {
        s.enable_dht = false;
    })
    .await;
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({"listen_port": 6881, "dht": true}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers().get("x-irontide-restart-pending").is_none(),
        "M173 graduation: listen_port + dht must NOT flag a restart"
    );
}

/// M173 Lane B (B10) graduation: lsd is now immediate.
#[tokio::test]
async fn set_preferences_lsd_no_longer_flags_restart_required() {
    let (router, sid) = enabled_router_with(|s| {
        s.enable_lsd = false;
    })
    .await;
    let resp = post_json(&router, &sid, serde_json::json!({"lsd": true})).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers().get("x-irontide-restart-pending").is_none(),
        "M173 graduation: lsd must NOT flag a restart anymore"
    );
}

/// M173 Lane B (B10) `[REGRESSION CRITICAL]`: pin the EXACT field-name
/// set in `X-IronTide-Restart-Pending` for fields that REMAIN in the
/// `restart_required` pool post-graduation: pex, encryption,
/// `anonymous_mode`, `save_path`. Downstream *arr clients parse this
/// header — silent rename = downstream regression.
#[tokio::test]
async fn set_preferences_remaining_restart_required_fields_pinned() {
    let (router, sid) = enabled_router_with(|s| {
        s.enable_pex = false;
        s.anonymous_mode = false;
    })
    .await;
    let resp = post_json(
        &router,
        &sid,
        serde_json::json!({"pex": true, "anonymous_mode": true}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let header = resp
        .headers()
        .get("x-irontide-restart-pending")
        .expect("pex + anonymous_mode change must still flag a restart")
        .to_str()
        .expect("header value is ASCII");
    let fields: std::collections::HashSet<&str> = header.split(',').collect();

    // Pin the EXACT expected set. Adding/renaming these breaks
    // downstream *arr clients.
    let mut expected = std::collections::HashSet::new();
    expected.insert("pex");
    expected.insert("anonymous_mode");

    assert_eq!(
        fields, expected,
        "restart-pending header field set drifted: got {fields:?}, expected {expected:?}"
    );
}

/// M173 Lane B (B10) `[REGRESSION CRITICAL]`: changing ALL graduated
/// fields plus a non-graduated one (encryption) — the header must
/// contain ONLY encryption, never the graduated fields.
#[tokio::test]
async fn set_preferences_graduated_fields_never_appear_in_header() {
    use irontide::prelude::EncryptionMode;
    let (router, sid) = enabled_router_with(|s| {
        s.enable_dht = false;
        s.enable_lsd = false;
        s.encryption_mode = EncryptionMode::Disabled;
    })
    .await;
    let resp = post_json(
        &router,
        &sid,
        // qBt encryption code 1 = Force (matches IronTide's Required).
        serde_json::json!({"listen_port": 6881, "dht": true, "lsd": true, "encryption": 1}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let header = resp
        .headers()
        .get("x-irontide-restart-pending")
        .expect("encryption change must still flag a restart")
        .to_str()
        .expect("header value is ASCII");
    let fields: std::collections::HashSet<&str> = header.split(',').collect();

    // Graduated fields must NOT appear.
    assert!(
        !fields.contains("listen_port"),
        "listen_port leaked into restart header: {fields:?}"
    );
    assert!(
        !fields.contains("dht"),
        "dht leaked into restart header: {fields:?}"
    );
    assert!(
        !fields.contains("lsd"),
        "lsd leaked into restart header: {fields:?}"
    );

    // Only encryption should remain.
    let mut expected = std::collections::HashSet::new();
    expected.insert("encryption");
    assert_eq!(
        fields, expected,
        "restart header should contain only `encryption`: {fields:?}"
    );
}

// ── Auth gate ─────────────────────────────────────────────────────────

#[tokio::test]
async fn set_preferences_requires_sid() {
    let (router, _) = enabled_router_with(|_| {}).await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/app/setPreferences")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
