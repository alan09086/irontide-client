//! Integration tests for the M167 Web UI detail page and fragment endpoints.
//!
//! Covers:
//!
//! - `GET /webui/torrents/{hash}` — the full detail page shell
//! - `GET /webui/fragments/torrent/{hash}/info` — Info tab fragment (Task 3)
//! - `GET /webui/fragments/torrent/{hash}/files` — Files tab fragment (Task 4)
//! - `GET /webui/fragments/torrent/{hash}/trackers` — Trackers tab (Task 6)
//! - `GET /webui/fragments/torrent/{hash}/peers` — Peers tab (Task 7)
//!
//! Uses the same `TempDir`-isolated session setup as `webui_actions.rs`.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tempfile::TempDir;
use tower::ServiceExt;

use irontide::session::Settings;
use irontide_api::routes::build_router;

const NONEXISTENT_HASH: &str = "0000000000000000000000000000000000000000";
const TEST_HASH: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";
const TEST_MAGNET: &str = "magnet:?xt=urn:btih:aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d&dn=test";

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

async fn body_text(response: axum::http::Response<Body>) -> String {
    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    String::from_utf8_lossy(&body).to_string()
}

// ---------------------------------------------------------------------------
// Detail page shell (Task 2)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn detail_page_valid_hash_renders_full_shell() {
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;

    let req = Request::get(format!("/webui/torrents/{hash}"))
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("detail");
    assert_eq!(response.status(), StatusCode::OK);
    let ct = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(ct.starts_with("text/html"), "expected text/html, got {ct}");

    let text = body_text(response).await;

    // Breadcrumb is the first user-facing affordance after the global nav.
    assert!(
        text.contains("Back to torrents"),
        "breadcrumb link missing from detail page: {text}"
    );
    // Heading renders the torrent name.
    assert!(
        text.contains("torrent-name"),
        "expected torrent-name element, got {text}"
    );
    // Tablist present with 4 tabs.
    assert!(
        text.contains(r#"role="tablist""#),
        "expected role=\"tablist\", got {text}"
    );
    for tab_label in ["Info", "Files", "Trackers", "Peers"] {
        assert!(
            text.contains(&format!(">{tab_label}</button>")),
            "missing tab button for {tab_label}"
        );
    }
    // Info panel is rendered inline (no HTMX round-trip on first paint).
    assert!(
        text.contains("Info hash (v1)"),
        "Info tab must be included inline on first paint: {text}"
    );
    // Info hash rendered as lowercase hex.
    assert!(
        text.contains(&hash),
        "detail page must render the info hash {hash} somewhere: {text}"
    );
}

#[tokio::test]
async fn detail_page_bad_hex_returns_400() {
    let (router, _tempdir) = test_router_isolated().await;
    let req = Request::get("/webui/torrents/not-a-hash")
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("detail");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn detail_page_unknown_hash_returns_404() {
    let (router, _tempdir) = test_router_isolated().await;
    let req = Request::get(format!("/webui/torrents/{NONEXISTENT_HASH}"))
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("detail");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn detail_page_tabs_have_aria_roles_and_tabindex() {
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;
    let req = Request::get(format!("/webui/torrents/{hash}"))
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("detail");
    let text = body_text(response).await;

    // WAI-ARIA: one selected tab has tabindex=0, the other three tabindex=-1.
    let selected = text.matches(r#"aria-selected="true""#).count();
    let unselected = text.matches(r#"aria-selected="false""#).count();
    assert_eq!(
        selected, 1,
        "exactly one tab must be aria-selected=true, got {selected}: {text}"
    );
    assert_eq!(
        unselected, 3,
        "three tabs must be aria-selected=false, got {unselected}: {text}"
    );
    assert!(
        text.contains(r#"tabindex="0""#) && text.contains(r#"tabindex="-1""#),
        "expected both tabindex=0 and tabindex=-1 on tabs, got {text}"
    );
    // Each tab must control a tabpanel — check aria-controls.
    for panel_id in ["panel-info", "panel-files", "panel-trackers", "panel-peers"] {
        assert!(
            text.contains(&format!(r#"aria-controls="{panel_id}""#)),
            "missing aria-controls={panel_id}: {text}"
        );
        assert!(
            text.contains(&format!(r#"id="{panel_id}""#)),
            "missing tabpanel id={panel_id}: {text}"
        );
    }
}

#[tokio::test]
async fn detail_page_lazy_panels_hx_get_urls_are_lowercase_hex() {
    // HTMX bracket-filter matches on hash equality — `refreshDetail[detail.hash=='<lower>']`.
    // The panel divs must embed the same lowercase form so the WS dispatcher
    // never misses a detail refresh.
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;
    let req = Request::get(format!("/webui/torrents/{hash}"))
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("detail");
    let text = body_text(response).await;

    for path in [
        format!("/webui/fragments/torrent/{hash}/files"),
        format!("/webui/fragments/torrent/{hash}/trackers"),
        format!("/webui/fragments/torrent/{hash}/peers"),
    ] {
        assert!(
            text.contains(&format!(r#"hx-get="{path}""#)),
            "expected hx-get={path} on one of the lazy panels: {text}"
        );
    }
    // Bracket-filter must use lowercase hash for WS dispatch match.
    assert!(
        text.contains(&format!("detail.hash=='{hash}'")),
        "lazy panel hx-trigger must filter on lowercase hash: {text}"
    );
}
