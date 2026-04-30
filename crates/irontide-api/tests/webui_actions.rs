//! Integration tests for HTMX-driven Web UI action handlers.
//!
//! Covers the per-row action buttons introduced in M166:
//!
//! - `POST /webui/torrents/{hash}/pause`
//! - `POST /webui/torrents/{hash}/resume`
//! - `DELETE /webui/torrents/{hash}`
//! - `POST /webui/torrents/{hash}/seed-mode?enabled=<bool>`
//!
//! Each test creates an isolated session backed by a `TempDir` resume
//! directory (see MEMORY.md `feedback_irontide_resume_test_isolation.md`) so
//! parallel runs do not collide in the shared XDG state dir.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tempfile::TempDir;
use tower::ServiceExt;

use irontide::session::Settings;
use irontide_api::routes::build_router;

/// A 40-char hex hash that does not correspond to any real torrent.
const NONEXISTENT_HASH: &str = "0000000000000000000000000000000000000000";

/// The hash used by `TEST_MAGNET`.
const TEST_HASH: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";

/// Canonical test magnet (info hash matches `TEST_HASH`).
const TEST_MAGNET: &str = "magnet:?xt=urn:btih:aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d&dn=test";

/// Build a router backed by a session with an isolated resume directory.
///
/// The returned [`TempDir`] must be held for the lifetime of the test — if
/// dropped, the temp directory is deleted and in-flight saves may fail.
async fn test_router_isolated() -> (axum::Router, TempDir) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let settings = Settings {
        listen_port: 0,
        download_dir: dir.path().join("downloads"),
        enable_dht: false,
        enable_lsd: false,
        enable_upnp: false,
        enable_natpmp: false,
        resume_data_dir: Some(dir.path().join("resume")),
        save_resume_interval_secs: 0,
        ..Settings::default()
    };

    let session = irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("start test session");
    (build_router(session), dir)
}

/// Add a single magnet-based torrent to the session so subsequent action
/// calls have something to target. Returns the hex info hash.
async fn seed_magnet(router: &axum::Router) -> String {
    let body_json = serde_json::json!({ "uri": TEST_MAGNET });
    let req = Request::post("/api/v1/torrents")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&body_json).expect("serialize"),
        ))
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("add magnet");
    assert_eq!(response.status(), StatusCode::CREATED, "magnet add failed");
    TEST_HASH.to_string()
}

// ---------------------------------------------------------------------------
// Pause
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pause_existing_torrent_emits_hx_trigger() {
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;

    let req = Request::post(format!("/webui/torrents/{hash}/pause"))
        .body(Body::empty())
        .expect("build pause request");
    let response = router.clone().oneshot(req).await.expect("pause");

    assert_eq!(response.status(), StatusCode::OK, "pause should return 200");
    let hx = response
        .headers()
        .get("HX-Trigger")
        .and_then(|v| v.to_str().ok());
    assert_eq!(
        hx,
        Some("refreshList"),
        "pause response must carry HX-Trigger: refreshList"
    );
}

#[tokio::test]
async fn pause_nonexistent_returns_error_fragment_without_trigger() {
    let (router, _tempdir) = test_router_isolated().await;

    let req = Request::post(format!("/webui/torrents/{NONEXISTENT_HASH}/pause"))
        .body(Body::empty())
        .expect("build pause request");
    let response = router.clone().oneshot(req).await.expect("pause");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(
        response.headers().get("HX-Trigger").is_none(),
        "error response must not emit refreshList"
    );
    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("error-message"),
        "error fragment should contain class=\"error-message\""
    );
}

// ---------------------------------------------------------------------------
// Resume
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resume_existing_torrent_emits_hx_trigger() {
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;

    // Pause first so resume has a visible state transition.
    let req = Request::post(format!("/webui/torrents/{hash}/pause"))
        .body(Body::empty())
        .expect("pause");
    let _ = router.clone().oneshot(req).await.expect("pause");

    let req = Request::post(format!("/webui/torrents/{hash}/resume"))
        .body(Body::empty())
        .expect("build resume request");
    let response = router.clone().oneshot(req).await.expect("resume");

    assert_eq!(response.status(), StatusCode::OK);
    let hx = response
        .headers()
        .get("HX-Trigger")
        .and_then(|v| v.to_str().ok());
    assert_eq!(hx, Some("refreshList"));
}

#[tokio::test]
async fn resume_nonexistent_returns_error_fragment_without_trigger() {
    let (router, _tempdir) = test_router_isolated().await;

    let req = Request::post(format!("/webui/torrents/{NONEXISTENT_HASH}/resume"))
        .body(Body::empty())
        .expect("build resume request");
    let response = router.clone().oneshot(req).await.expect("resume");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(response.headers().get("HX-Trigger").is_none());
    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("error-message"),
        "resume 404 should emit HTML fragment with class=\"error-message\", got: {text}"
    );
}

#[tokio::test]
async fn invalid_hash_returns_bad_request_fragment() {
    let (router, _tempdir) = test_router_isolated().await;

    let req = Request::post("/webui/torrents/not-a-hash/pause")
        .body(Body::empty())
        .expect("build pause request");
    let response = router.clone().oneshot(req).await.expect("pause");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(response.headers().get("HX-Trigger").is_none());
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_existing_torrent_removes_row_and_emits_trigger() {
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;

    // Sanity: the fragment should show the torrent before delete.
    let req = Request::get("/webui/fragments/torrent-list")
        .body(Body::empty())
        .expect("build list request");
    let response = router.clone().oneshot(req).await.expect("list");
    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(
        !text.contains("No torrents yet"),
        "precondition: list should contain the seeded torrent, got {text}"
    );

    // DELETE should emit refreshList.
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/webui/torrents/{hash}"))
        .body(Body::empty())
        .expect("build delete request");
    let response = router.clone().oneshot(req).await.expect("delete");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("HX-Trigger")
            .and_then(|v| v.to_str().ok()),
        Some("refreshList"),
    );

    // The list fragment should now render the empty-state markup.
    let req = Request::get("/webui/fragments/torrent-list")
        .body(Body::empty())
        .expect("build list request");
    let response = router.clone().oneshot(req).await.expect("list");
    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("No torrents yet"),
        "list should render empty state after delete, got {text}"
    );
}

#[tokio::test]
async fn delete_nonexistent_returns_error_fragment() {
    let (router, _tempdir) = test_router_isolated().await;

    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/webui/torrents/{NONEXISTENT_HASH}"))
        .body(Body::empty())
        .expect("build delete request");
    let response = router.clone().oneshot(req).await.expect("delete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(response.headers().get("HX-Trigger").is_none());
    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("error-message"),
        "delete 404 should emit HTML fragment, got {text}"
    );
}

// ---------------------------------------------------------------------------
// Seed-mode toggle
// ---------------------------------------------------------------------------

async fn fetch_fragment(router: &axum::Router) -> String {
    let req = Request::get("/webui/fragments/torrent-list")
        .body(Body::empty())
        .expect("build list request");
    let response = router.clone().oneshot(req).await.expect("list");
    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    String::from_utf8_lossy(&body).to_string()
}

#[tokio::test]
async fn seed_mode_enable_flips_flag_and_emits_trigger() {
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;

    // Precondition: fragment shows the "Enable seed" button (action-seed class).
    let fragment = fetch_fragment(&router).await;
    assert!(
        fragment.contains("action-seed"),
        "precondition: default user_seed_mode=false, got {fragment}"
    );

    // POST with enabled=true.
    let req = Request::post(format!("/webui/torrents/{hash}/seed-mode?enabled=true"))
        .body(Body::empty())
        .expect("build seed-mode request");
    let response = router.clone().oneshot(req).await.expect("seed-mode");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("HX-Trigger")
            .and_then(|v| v.to_str().ok()),
        Some("refreshList"),
    );

    // Fragment should now show the "Disable seed" button (action-unseed class).
    let fragment = fetch_fragment(&router).await;
    assert!(
        fragment.contains("action-unseed"),
        "user_seed_mode should be true after enable, got {fragment}"
    );
}

#[tokio::test]
async fn seed_mode_disable_flips_flag_back() {
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;

    // Enable first.
    let req = Request::post(format!("/webui/torrents/{hash}/seed-mode?enabled=true"))
        .body(Body::empty())
        .expect("build seed-mode request");
    let _ = router.clone().oneshot(req).await.expect("enable seed");

    // Now disable.
    let req = Request::post(format!("/webui/torrents/{hash}/seed-mode?enabled=false"))
        .body(Body::empty())
        .expect("build seed-mode request");
    let response = router.clone().oneshot(req).await.expect("disable seed");
    assert_eq!(response.status(), StatusCode::OK);

    let fragment = fetch_fragment(&router).await;
    assert!(
        fragment.contains("action-seed") && !fragment.contains("action-unseed"),
        "user_seed_mode should be false after disable, got {fragment}"
    );
}

#[tokio::test]
async fn seed_mode_nonexistent_returns_error_fragment() {
    let (router, _tempdir) = test_router_isolated().await;

    let req = Request::post(format!(
        "/webui/torrents/{NONEXISTENT_HASH}/seed-mode?enabled=true"
    ))
    .body(Body::empty())
    .expect("build seed-mode request");
    let response = router.clone().oneshot(req).await.expect("seed-mode");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(response.headers().get("HX-Trigger").is_none());
    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("error-message"),
        "seed-mode 404 should emit HTML fragment, got {text}"
    );
}

#[tokio::test]
async fn seed_mode_missing_query_returns_bad_request() {
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;

    // No ?enabled=... query parameter.
    let req = Request::post(format!("/webui/torrents/{hash}/seed-mode"))
        .body(Body::empty())
        .expect("build seed-mode request");
    let response = router.clone().oneshot(req).await.expect("seed-mode");

    assert!(
        response.status().is_client_error(),
        "missing query should 4xx, got {}",
        response.status()
    );
}

// ---------------------------------------------------------------------------
// Settings fragment (GET)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn settings_fragment_renders_current_values() {
    let (router, _tempdir) = test_router_isolated().await;

    let req = Request::get("/webui/fragments/settings")
        .body(Body::empty())
        .expect("build settings fragment request");
    let response = router
        .clone()
        .oneshot(req)
        .await
        .expect("settings fragment");
    assert_eq!(response.status(), StatusCode::OK);

    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let text = String::from_utf8_lossy(&body);

    assert!(
        text.contains("<form"),
        "settings fragment must render a form, got {text}"
    );
    assert!(
        text.contains("hx-patch=\"/webui/settings\""),
        "form must PATCH /webui/settings, got {text}"
    );
    for field in [
        "listen_port",
        "download_dir",
        "max_torrents",
        "max_peers_per_torrent",
        "download_rate_limit",
        "upload_rate_limit",
        "active_downloads",
        "active_seeds",
        "enable_dht",
        "enable_pex",
        "enable_lsd",
    ] {
        assert!(
            text.contains(&format!("name=\"{field}\"")),
            "settings form must expose field {field}, got {text}"
        );
    }
}

// ---------------------------------------------------------------------------
// Settings fragment (PATCH)
// ---------------------------------------------------------------------------

async fn fetch_settings_fragment(router: &axum::Router) -> String {
    let req = Request::get("/webui/fragments/settings")
        .body(Body::empty())
        .expect("build settings fragment request");
    let response = router
        .clone()
        .oneshot(req)
        .await
        .expect("settings fragment");
    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    String::from_utf8_lossy(&body).to_string()
}

/// Build a form-urlencoded body matching the fields in the settings form,
/// with `max_torrents` set to the caller-supplied value.
fn settings_form_body(max_torrents: usize) -> String {
    format!(
        "listen_port=0&download_dir=%2Ftmp&max_torrents={max_torrents}\
         &max_peers_per_torrent=50&download_rate_limit=0&upload_rate_limit=0\
         &active_downloads=-1&active_seeds=-1&enable_dht=on&enable_pex=on"
    )
}

#[tokio::test]
async fn patch_settings_round_trips_max_torrents() {
    let (router, _tempdir) = test_router_isolated().await;

    let req = Request::builder()
        .method("PATCH")
        .uri("/webui/settings")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(settings_form_body(42)))
        .expect("build patch request");
    let response = router.clone().oneshot(req).await.expect("patch settings");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("HX-Trigger")
            .and_then(|v| v.to_str().ok()),
        Some("settingsSaved"),
    );

    let fragment = fetch_settings_fragment(&router).await;
    assert!(
        fragment.contains("name=\"max_torrents\" min=\"1\"\n             value=\"42\"")
            || fragment.contains("value=\"42\""),
        "fragment should reflect the new max_torrents=42, got {fragment}"
    );
}

#[tokio::test]
async fn patch_settings_checkbox_absence_disables_flag() {
    let (router, _tempdir) = test_router_isolated().await;

    // Omit enable_lsd entirely — an HTML checkbox form does this on uncheck.
    let body = "listen_port=0&download_dir=%2Ftmp&max_torrents=64\
                &max_peers_per_torrent=50&download_rate_limit=0&upload_rate_limit=0\
                &active_downloads=-1&active_seeds=-1&enable_dht=on&enable_pex=on";
    let req = Request::builder()
        .method("PATCH")
        .uri("/webui/settings")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .expect("build patch request");
    let response = router.clone().oneshot(req).await.expect("patch");
    assert_eq!(response.status(), StatusCode::OK);

    // The rendered fragment should show enable_lsd without `checked`.
    let fragment = fetch_settings_fragment(&router).await;
    // Extract the enable_lsd input line and confirm it has no "checked".
    let lsd_line = fragment
        .lines()
        .find(|l| l.contains("name=\"enable_lsd\""))
        .expect("enable_lsd input present");
    assert!(
        !lsd_line.contains("checked"),
        "enable_lsd should be unchecked after form omitted it, line: {lsd_line}"
    );
}

#[tokio::test]
async fn patch_settings_malformed_body_returns_error_fragment() {
    let (router, _tempdir) = test_router_isolated().await;

    // Missing required fields — form deserialization must fail.
    let req = Request::builder()
        .method("PATCH")
        .uri("/webui/settings")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("listen_port=abc"))
        .expect("build patch request");
    let response = router.clone().oneshot(req).await.expect("patch");

    assert!(
        response.status().is_client_error(),
        "malformed form should 4xx, got {}",
        response.status()
    );
    assert!(response.headers().get("HX-Trigger").is_none());
}

// ---------------------------------------------------------------------------
// Settings page routing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn settings_page_served_at_slash_settings() {
    let (router, _tempdir) = test_router_isolated().await;

    let req = Request::get("/settings")
        .body(Body::empty())
        .expect("build settings request");
    let response = router.clone().oneshot(req).await.expect("settings");
    assert_eq!(response.status(), StatusCode::OK);
    let ct = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(ct.starts_with("text/html"), "expected text/html, got {ct}");

    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("IronTide — Settings"),
        "settings page should contain the title, got: {}",
        &text[..text.len().min(200)]
    );
}
