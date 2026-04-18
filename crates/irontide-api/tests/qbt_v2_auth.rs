//! Integration tests for the qBittorrent WebUI v2 compatibility surface (M168).
//!
//! Covers `QbtResponse`, `QbtError`, `SessionStore`, cookie parsing, and the
//! `auth/login` / `auth/logout` endpoints plus the `qbt_gate` / `require_sid`
//! middleware chain.

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use irontide::session::Settings;
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Build an enabled-qbt_compat session with the given overrides applied.
async fn test_session_with_qbt(
    customize: impl FnOnce(&mut Settings),
) -> irontide::session::SessionHandle {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let resume_dir = std::env::temp_dir().join(format!(
        "irontide-qbt-v2-test-{}-{}",
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

    irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("failed to start test session")
}

async fn enabled_router() -> axum::Router {
    let session = test_session_with_qbt(|_| {}).await;
    build_router(session)
}

async fn disabled_router() -> axum::Router {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let resume_dir = std::env::temp_dir().join(format!(
        "irontide-qbt-v2-dis-{}-{}",
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&resume_dir);

    let settings = Settings {
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
    let session = irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("failed to start test session");
    build_router(session)
}

async fn send(router: &axum::Router, req: Request<Body>) -> axum::http::Response<Body> {
    router.clone().oneshot(req).await.expect("request failed")
}

async fn body_string(resp: axum::http::Response<Body>) -> (StatusCode, String, axum::http::HeaderMap) {
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp
        .into_body()
        .collect()
        .await
        .expect("body collect failed")
        .to_bytes();
    (
        status,
        String::from_utf8(body.to_vec()).expect("non-utf8 body"),
        headers,
    )
}

fn login_request(user: &str, pass: &str) -> Request<Body> {
    let form = format!("username={user}&password={pass}");
    Request::builder()
        .method("POST")
        .uri("/api/v2/auth/login")
        .header(
            header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(Body::from(form))
        .expect("build login request")
}

// ── Task 3: QbtResponse + QbtError Content-Type tests ─────────────────

#[tokio::test]
async fn qbt_response_ok_plaintext_body_and_ctype() {
    let router = enabled_router().await;
    let resp = send(&router, login_request("admin", "adminadmin")).await;
    let (status, body, headers) = body_string(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "Ok.");
    let ct = headers
        .get(header::CONTENT_TYPE)
        .expect("Content-Type header")
        .to_str()
        .unwrap();
    assert!(ct.starts_with("text/plain"), "got: {ct}");
}

#[tokio::test]
async fn qbt_response_forbidden_status_and_body() {
    let router = enabled_router().await;
    let resp = send(&router, login_request("admin", "wrongpassword")).await;
    let (status, body, _) = body_string(resp).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, "Fails.");
}

#[tokio::test]
async fn qbt_response_login_success_sets_sid_cookie_with_httponly_path_samesite() {
    let router = enabled_router().await;
    let resp = send(&router, login_request("admin", "adminadmin")).await;
    let set_cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .expect("Set-Cookie header")
        .to_str()
        .unwrap()
        .to_owned();
    // Drain body to avoid axum's debug warning
    let _ = resp.into_body().collect().await.unwrap();
    assert!(set_cookie.starts_with("SID="), "got: {set_cookie}");
    let lower = set_cookie.to_ascii_lowercase();
    assert!(lower.contains("httponly"), "got: {set_cookie}");
    assert!(lower.contains("path=/"), "got: {set_cookie}");
    assert!(lower.contains("samesite=lax"), "got: {set_cookie}");
}

#[tokio::test]
async fn qbt_response_plain_text_asserts_text_plain_utf8_ctype() {
    // app/version endpoint returns plain text; we need to be logged-in first.
    let router = enabled_router().await;
    let resp = send(&router, login_request("admin", "adminadmin")).await;
    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .expect("Set-Cookie header")
        .to_str()
        .unwrap()
        .to_owned();
    let _ = resp.into_body().collect().await.unwrap();
    // Grab just the "SID=..." token for the Cookie header.
    let sid = cookie.split(';').next().unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/app/version")
        .header(header::COOKIE, sid)
        .body(Body::empty())
        .unwrap();
    let resp = send(&router, req).await;
    let (status, body, headers) = body_string(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "v5.1.4");
    let ct = headers.get(header::CONTENT_TYPE).unwrap().to_str().unwrap();
    assert!(ct.contains("text/plain"), "got: {ct}");
    assert!(ct.contains("utf-8") || ct.contains("UTF-8"), "got: {ct}");
}

#[tokio::test]
async fn qbt_response_json_asserts_application_json_ctype() {
    let router = enabled_router().await;
    let resp = send(&router, login_request("admin", "adminadmin")).await;
    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let _ = resp.into_body().collect().await.unwrap();
    let sid = cookie.split(';').next().unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/app/buildInfo")
        .header(header::COOKIE, sid)
        .body(Body::empty())
        .unwrap();
    let resp = send(&router, req).await;
    let (status, _body, headers) = body_string(resp).await;
    assert_eq!(status, StatusCode::OK);
    let ct = headers.get(header::CONTENT_TYPE).unwrap().to_str().unwrap();
    assert!(ct.contains("application/json"), "got: {ct}");
}

// ── Task 5: auth/login, auth/logout, require_sid, qbt_gate tests ──────

#[tokio::test]
async fn auth_login_correct_creds_returns_ok_with_cookie() {
    let router = enabled_router().await;
    let resp = send(&router, login_request("admin", "adminadmin")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers().get(header::SET_COOKIE).is_some());
}

#[tokio::test]
async fn auth_login_wrong_creds_returns_fails_403_no_cookie() {
    let router = enabled_router().await;
    let resp = send(&router, login_request("admin", "wrongpassword")).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(resp.headers().get(header::SET_COOKIE).is_none());
    let (_, body, _) = body_string(resp).await;
    assert_eq!(body, "Fails.");
}

#[tokio::test]
async fn auth_login_missing_username_returns_400() {
    let router = enabled_router().await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("password=adminadmin"))
        .unwrap();
    let resp = send(&router, req).await;
    assert!(
        resp.status() == StatusCode::BAD_REQUEST
            || resp.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "got: {}",
        resp.status()
    );
}

#[tokio::test]
async fn auth_login_missing_password_returns_400() {
    let router = enabled_router().await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("username=admin"))
        .unwrap();
    let resp = send(&router, req).await;
    assert!(
        resp.status() == StatusCode::BAD_REQUEST
            || resp.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "got: {}",
        resp.status()
    );
}

#[tokio::test]
async fn auth_logout_with_valid_cookie_invalidates_and_returns_ok() {
    let router = enabled_router().await;
    let resp = send(&router, login_request("admin", "adminadmin")).await;
    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let _ = resp.into_body().collect().await.unwrap();
    let sid = cookie.split(';').next().unwrap();

    // Logout
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/auth/logout")
        .header(header::COOKIE, sid)
        .body(Body::empty())
        .unwrap();
    let resp = send(&router, req).await;
    let (status, body, _) = body_string(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "Ok.");

    // Subsequent requests with this cookie must be rejected.
    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/app/version")
        .header(header::COOKIE, sid)
        .body(Body::empty())
        .unwrap();
    let resp = send(&router, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn auth_logout_without_cookie_still_returns_ok_idempotent() {
    let router = enabled_router().await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/auth/logout")
        .body(Body::empty())
        .unwrap();
    let resp = send(&router, req).await;
    let (status, body, _) = body_string(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "Ok.");
}

#[tokio::test]
async fn auth_logout_with_expired_cookie_returns_ok() {
    let router = enabled_router().await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/auth/logout")
        .header(header::COOKIE, "SID=this-token-never-existed")
        .body(Body::empty())
        .unwrap();
    let resp = send(&router, req).await;
    let (status, body, _) = body_string(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "Ok.");
}

#[tokio::test]
async fn require_sid_missing_cookie_returns_403_fails() {
    let router = enabled_router().await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/app/version")
        .body(Body::empty())
        .unwrap();
    let resp = send(&router, req).await;
    let (status, body, _) = body_string(resp).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, "Fails.");
}

#[tokio::test]
async fn require_sid_malformed_cookie_returns_403() {
    let router = enabled_router().await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/app/version")
        .header(header::COOKIE, "GARBAGE")
        .body(Body::empty())
        .unwrap();
    let resp = send(&router, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn qbt_gate_disabled_returns_404_not_403() {
    let router = disabled_router().await;
    let resp = send(&router, login_request("admin", "adminadmin")).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ── Task 18: disabled-gate + middleware edge cases ────────────────────

#[tokio::test]
async fn qbt_routes_404_when_disabled_across_all_endpoints() {
    let router = disabled_router().await;
    // Every v2 endpoint must 404 when the gate is closed.
    let getters = [
        "/api/v2/app/version",
        "/api/v2/app/webapiVersion",
        "/api/v2/app/buildInfo",
        "/api/v2/app/preferences",
        "/api/v2/torrents/info",
        "/api/v2/torrents/properties?hash=0000000000000000000000000000000000000000",
        "/api/v2/torrents/categories",
        "/api/v2/transferInfo",
    ];
    for uri in getters {
        let req = Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let resp = send(&router, req).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "uri: {uri}");
    }
    let posters = [
        "/api/v2/auth/logout",
        "/api/v2/torrents/pause?hashes=all",
        "/api/v2/torrents/resume?hashes=all",
        "/api/v2/torrents/delete?hashes=all",
        "/api/v2/torrents/recheck?hashes=all",
        "/api/v2/torrents/reannounce?hashes=all",
        "/api/v2/torrents/add",
    ];
    for uri in posters {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let resp = send(&router, req).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "POST uri: {uri}");
    }
}

#[tokio::test]
async fn runtime_toggle_enabled_to_disabled_via_patch_settings() {
    // Start enabled → confirm 200 on a probe.
    let router = enabled_router().await;
    let resp = send(&router, login_request("admin", "adminadmin")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let _ = resp.into_body().collect().await.unwrap();
    let sid = cookie.split(';').next().unwrap();
    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/app/version")
        .header(header::COOKIE, sid)
        .body(Body::empty())
        .unwrap();
    let resp = send(&router, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Flip qbt_compat.enabled via PATCH /api/v1/session/settings.
    let patch = serde_json::json!({ "qbt_compat": { "enabled": false } });
    let req = Request::builder()
        .method("PATCH")
        .uri("/api/v1/session/settings")
        .header(header::CONTENT_TYPE, "application/merge-patch+json")
        .body(Body::from(patch.to_string()))
        .unwrap();
    let resp = send(&router, req).await;
    // The merge-patch endpoint may return 200 or 204 depending on status
    // conventions; accept any 2xx here.
    assert!(
        resp.status().is_success(),
        "patch settings failed: {}",
        resp.status()
    );
    let _ = resp.into_body().collect().await.unwrap();

    // After the flip, the gate must 404 everything — even with a cookie that
    // was valid a moment ago.
    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/app/version")
        .header(header::COOKIE, sid)
        .body(Body::empty())
        .unwrap();
    let resp = send(&router, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn empty_cookie_header_returns_403() {
    let router = enabled_router().await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/app/version")
        .header(header::COOKIE, "")
        .body(Body::empty())
        .unwrap();
    let resp = send(&router, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn cookie_with_similar_named_attribute_mysid_is_ignored() {
    // A cookie called MYSID (not SID) must NOT be mistaken for the real one.
    let router = enabled_router().await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/app/version")
        .header(header::COOKIE, "MYSID=not-a-real-token")
        .body(Body::empty())
        .unwrap();
    let resp = send(&router, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn multiple_cookies_extracts_correct_sid() {
    // Browsers often send multiple cookies in one Cookie header. Make sure
    // the CookieJar extractor picks SID out of the middle of the list.
    let router = enabled_router().await;
    let resp = send(&router, login_request("admin", "adminadmin")).await;
    let sid_value = resp
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("SID=")
        .to_owned();
    let _ = resp.into_body().collect().await.unwrap();

    let combined = format!("foo=bar; SID={sid_value}; tracking=xyz");
    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/app/version")
        .header(header::COOKIE, combined)
        .body(Body::empty())
        .unwrap();
    let resp = send(&router, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}
