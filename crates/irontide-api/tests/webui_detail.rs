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
use serde::Serialize;
use tempfile::TempDir;
use tower::ServiceExt;

use irontide::session::{SessionHandle, Settings};
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

// ---------------------------------------------------------------------------
// Info fragment (Task 3)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn info_fragment_renders_metadata_pending_for_magnet() {
    // A magnet add starts with no metadata, so the fragment renders the
    // info hash (always known) and a "Metadata pending" indicator.
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;

    let req = Request::get(format!("/webui/fragments/torrent/{hash}/info"))
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("info fragment");
    assert_eq!(response.status(), StatusCode::OK);
    let text = body_text(response).await;

    assert!(
        text.contains("Info hash (v1)"),
        "missing info-hash label: {text}"
    );
    assert!(
        text.contains(&hash),
        "info fragment must render the info hash {hash}: {text}"
    );
    assert!(
        text.contains("Metadata pending"),
        "magnet without peers must show pending indicator: {text}"
    );
}

#[tokio::test]
async fn info_fragment_unknown_hash_returns_404() {
    let (router, _tempdir) = test_router_isolated().await;
    let req = Request::get(format!("/webui/fragments/torrent/{NONEXISTENT_HASH}/info"))
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("info fragment");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn info_fragment_bad_hex_returns_400() {
    let (router, _tempdir) = test_router_isolated().await;
    let req = Request::get("/webui/fragments/torrent/not-a-hash/info")
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("info fragment");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// Peers fragment (Task 7)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn peers_fragment_renders_empty_state_for_fresh_magnet() {
    // A just-added magnet has no connected peers yet, so the fragment
    // renders "No peers yet" + the flag-legend <details>.
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;
    let req = Request::get(format!("/webui/fragments/torrent/{hash}/peers"))
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("peers fragment");
    assert_eq!(response.status(), StatusCode::OK);
    let text = body_text(response).await;
    assert!(
        text.contains("No peers yet"),
        "empty peers fragment must render the placeholder: {text}"
    );
    // The legend is a permanent affordance so users can look up symbol meanings.
    assert!(
        text.contains("Flag legend"),
        "flag legend must be present regardless of peer count: {text}"
    );
    for glyph in ['D', 'U', 'K', '?', 'I', 'S'] {
        assert!(
            text.contains(&format!("<strong>{glyph}</strong>")),
            "legend missing entry for {glyph}: {text}"
        );
    }
}

#[tokio::test]
async fn peers_fragment_unknown_hash_returns_404() {
    let (router, _tempdir) = test_router_isolated().await;
    let req = Request::get(format!("/webui/fragments/torrent/{NONEXISTENT_HASH}/peers"))
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("peers fragment");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn peers_fragment_bad_hex_returns_400() {
    let (router, _tempdir) = test_router_isolated().await;
    let req = Request::get("/webui/fragments/torrent/not-a-hash/peers")
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("peers fragment");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// Trackers fragment (Task 6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn trackers_fragment_renders_force_reannounce_button_even_when_empty() {
    // A magnet carries no trackers in its announce list by default, so the
    // table renders empty — but the Force Reannounce button must still be
    // present so the user can trigger a DHT reannounce if they want.
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;
    let req = Request::get(format!("/webui/fragments/torrent/{hash}/trackers"))
        .body(Body::empty())
        .expect("build request");
    let response = router
        .clone()
        .oneshot(req)
        .await
        .expect("trackers fragment");
    assert_eq!(response.status(), StatusCode::OK);
    let text = body_text(response).await;
    assert!(
        text.contains("Force Reannounce"),
        "Force Reannounce button must be present: {text}"
    );
    assert!(
        text.contains(&format!(r#"hx-post="/webui/torrents/{hash}/reannounce""#)),
        "Force Reannounce form must target the reannounce endpoint: {text}"
    );
}

#[tokio::test]
async fn trackers_fragment_unknown_hash_returns_404() {
    let (router, _tempdir) = test_router_isolated().await;
    let req = Request::get(format!(
        "/webui/fragments/torrent/{NONEXISTENT_HASH}/trackers"
    ))
    .body(Body::empty())
    .expect("build request");
    let response = router
        .clone()
        .oneshot(req)
        .await
        .expect("trackers fragment");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Files fragment (Task 4)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn files_fragment_renders_empty_state_for_magnet_without_metadata() {
    // A fresh magnet has no metadata, so the Files tab renders the empty
    // placeholder instead of a table.
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;
    let req = Request::get(format!("/webui/fragments/torrent/{hash}/files"))
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("files fragment");
    assert_eq!(response.status(), StatusCode::OK);
    let text = body_text(response).await;
    assert!(
        text.contains("Metadata not yet received"),
        "magnet without metadata must render empty-state copy: {text}"
    );
    assert!(
        !text.contains("<table"),
        "empty files fragment must not render the table: {text}"
    );
}

#[tokio::test]
async fn files_fragment_unknown_hash_returns_404() {
    let (router, _tempdir) = test_router_isolated().await;
    let req = Request::get(format!("/webui/fragments/torrent/{NONEXISTENT_HASH}/files"))
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("files fragment");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn files_fragment_bad_hex_returns_400() {
    let (router, _tempdir) = test_router_isolated().await;
    let req = Request::get("/webui/fragments/torrent/not-a-hash/files")
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("files fragment");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// Row-click navigation on the list (Task 10)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn torrent_list_name_is_link_to_detail_page() {
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;
    let req = Request::get("/webui/fragments/torrent-list")
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("list fragment");
    assert_eq!(response.status(), StatusCode::OK);
    let text = body_text(response).await;
    assert!(
        text.contains(&format!(
            r#"<a href="/webui/torrents/{hash}" class="torrent-name-link">"#
        )),
        "torrent name must be wrapped in a link to the detail page: {text}"
    );
}

#[tokio::test]
async fn detail_page_wires_removed_banner_on_404() {
    // The page-level response-error listener catches 404s from any
    // fragment and swaps in the banner. Verify the JS hook is present
    // — the actual DOM swap is client-side behaviour we'd dogfood.
    let (router, _tempdir) = test_router_isolated().await;
    let hash = seed_magnet(&router).await;
    let req = Request::get(format!("/webui/torrents/{hash}"))
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("detail");
    let text = body_text(response).await;

    assert!(
        text.contains("function swapRemovedBanner"),
        "detail page must define swapRemovedBanner: {text}"
    );
    assert!(
        text.contains("htmx:responseError"),
        "detail page must listen for htmx:responseError: {text}"
    );
    assert!(
        text.contains(r#"data-detail-hash="#),
        "body must carry data-detail-hash for ws-live suppression: {text}"
    );
    // The banner strips data-detail-hash so ws-live.js stops dispatching.
    assert!(
        text.contains("removeAttribute('data-detail-hash')"),
        "swapRemovedBanner must clear data-detail-hash: {text}"
    );
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

// ---------------------------------------------------------------------------
// M177: regression coverage for the irontide-format::build_flat refactor.
//
// Files-fragment rendering must stay byte-identical for the matched-length
// case after the Web UI files-row builder is refactored to consume the new
// shared `FlatFileEntry` / `build_flat` helper (D-eng-3). Two tests:
//
// 1. Three-file torrent → three rows, each with path + formatted size.
// 2. Same torrent → priority `<select>` dropdowns mark the current
//    priority as `selected` (proves `priority_slug` survives the rewrite).
//
// Both add a synthesised 3-file v1 torrent via the session handle so the
// fragment endpoint actually has metadata + file_progress + file_priorities
// to project. `seed_magnet`-only flows (used elsewhere in this file) hit
// the empty-state branch and don't exercise the refactor surface.
// ---------------------------------------------------------------------------

fn make_three_file_torrent_bytes() -> Vec<u8> {
    #[derive(Serialize)]
    struct RawFile<'a> {
        length: u64,
        path: &'a [String],
    }

    #[derive(Serialize)]
    struct Info<'a> {
        files: Vec<RawFile<'a>>,
        name: &'a str,
        #[serde(rename = "piece length")]
        piece_length: u64,
        #[serde(with = "serde_bytes")]
        pieces: &'a [u8],
    }

    #[derive(Serialize)]
    struct Torrent<'a> {
        info: Info<'a>,
    }

    let piece_length: u64 = 65_536;
    let total: u64 = 100 + 50_000 + 80_000;
    let num_pieces = total.div_ceil(piece_length).max(1);
    let pieces = vec![0u8; usize::try_from(num_pieces).unwrap_or(usize::MAX).saturating_mul(20)];

    let readme: Vec<String> = vec!["readme.txt".into()];
    let intro: Vec<String> = vec!["video".into(), "intro.mp4".into()];
    let bts: Vec<String> = vec!["video".into(), "extras".into(), "bts.mkv".into()];

    let raw_files: Vec<RawFile<'_>> = vec![
        RawFile {
            length: 100,
            path: &readme,
        },
        RawFile {
            length: 50_000,
            path: &intro,
        },
        RawFile {
            length: 80_000,
            path: &bts,
        },
    ];

    let t = Torrent {
        info: Info {
            files: raw_files,
            name: "m177-three-file",
            piece_length,
            pieces: &pieces,
        },
    };
    irontide::bencode::to_bytes(&t).expect("bencode serialize")
}

async fn make_three_file_router_and_hash() -> (axum::Router, TempDir, String) {
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
    let session: SessionHandle = irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("start test session");

    let bytes = make_three_file_torrent_bytes();
    let hashes = session
        .add_torrent_bytes(&bytes)
        .await
        .expect("add multi-file torrent");
    let hash = hashes.v1.expect("v1 hash").to_hex();
    let router = build_router(session);
    (router, dir, hash)
}

#[tokio::test]
async fn files_fragment_three_file_torrent_renders_all_rows_after_build_flat_refactor() {
    let (router, _tempdir, hash) = make_three_file_router_and_hash().await;

    let req = Request::get(format!("/webui/fragments/torrent/{hash}/files"))
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("files fragment");
    assert_eq!(response.status(), StatusCode::OK);
    let text = body_text(response).await;

    // Each file path appears in the rendered table.
    for path in ["readme.txt", "video/intro.mp4", "video/extras/bts.mkv"] {
        assert!(
            text.contains(path),
            "expected file path {path} in fragment, got {text}"
        );
    }
    // Sizes are formatted via `irontide_format::format_size`.
    // 100 B → "100 B"; 50_000 B → "48.8 KiB"; 80_000 B → "78.1 KiB".
    for size_label in ["100 B", "48.8 KiB", "78.1 KiB"] {
        assert!(
            text.contains(size_label),
            "expected size {size_label} in fragment, got {text}"
        );
    }
    // Three rows means three PATCH endpoints, one per file index.
    for idx in 0..3 {
        assert!(
            text.contains(&format!(
                r#"hx-patch="/webui/torrents/{hash}/files/{idx}""#
            )),
            "row {idx} missing hx-patch URL, got {text}"
        );
    }
}

#[tokio::test]
async fn files_fragment_three_file_torrent_priority_select_marks_normal_after_refactor() {
    let (router, _tempdir, hash) = make_three_file_router_and_hash().await;

    let req = Request::get(format!("/webui/fragments/torrent/{hash}/files"))
        .body(Body::empty())
        .expect("build request");
    let response = router.clone().oneshot(req).await.expect("files fragment");
    assert_eq!(response.status(), StatusCode::OK);
    let text = body_text(response).await;

    // Default priority for a freshly-added torrent is Normal — the
    // priority_slug helper must still emit `selected` on that option.
    let normal_selected_count = text
        .matches(r#"<option value="normal" selected>Normal</option>"#)
        .count();
    assert_eq!(
        normal_selected_count, 3,
        "expected three rows with `Normal` selected, got {normal_selected_count}: {text}"
    );
}
