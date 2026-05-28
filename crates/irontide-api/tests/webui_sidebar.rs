//! M231 — integration tests for the `WebUI` sidebar filter machinery.
//!
//! Covers the four filter axes (state / category / tag / tracker), the
//! sentinel-value handling (`uncategorised` / `untagged` / `no_tracker`),
//! OR-within-section and AND-across-section semantics, the OOB sidebar
//! fragment marker, graceful handling of invalid filter values, XSS
//! escaping of reflected filter values, and the chip-zero-count
//! visibility invariant.
//!
//! Each test creates an isolated session backed by a `TempDir` so
//! parallel runs cannot collide. Torrents are added via the session
//! handle directly so we can attach categories and tags at add time
//! (the M171 design holds category and tags inside `AddTorrentParams`;
//! there is no post-add public assignment for the category axis).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use irontide::core::Id20;
use irontide::session::{SessionAddTorrentParams, Settings};
use irontide_api::routes::build_router;
use tempfile::TempDir;
use tower::ServiceExt;

const MAGNET_A: &str = "magnet:?xt=urn:btih:1111111111111111111111111111111111111111&dn=alpha";
const HASH_A: &str = "1111111111111111111111111111111111111111";

const MAGNET_B: &str = "magnet:?xt=urn:btih:2222222222222222222222222222222222222222&dn=bravo";
const HASH_B: &str = "2222222222222222222222222222222222222222";

const MAGNET_C: &str = "magnet:?xt=urn:btih:3333333333333333333333333333333333333333&dn=charlie";
const HASH_C: &str = "3333333333333333333333333333333333333333";

const MAGNET_WITH_TRACKER: &str = "magnet:?xt=urn:btih:4444444444444444444444444444444444444444&dn=delta&tr=https%3A%2F%2Ftracker.foo.example%2Fannounce";
const HASH_WITH_TRACKER: &str = "4444444444444444444444444444444444444444";

async fn test_router_isolated() -> (axum::Router, irontide::session::SessionHandle, TempDir) {
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
        // Isolate the category + tag registries to this tempdir so
        // parallel tests do not collide with each other (and do not
        // contaminate the user's $XDG_CONFIG_HOME during test runs).
        category_registry_path: Some(dir.path().join("categories.toml")),
        tag_registry_path: Some(dir.path().join("tags.toml")),
        ..Settings::default()
    };

    let session = irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("start test session");
    (build_router(session.clone()), session, dir)
}

fn id20(hex: &str) -> Id20 {
    Id20::from_hex(hex).expect("parse hex hash")
}

/// Percent-encode RFC 3986 reserved characters for the XSS test path —
/// we cannot pull in `urlencoding` because it is not a dev-dep here.
fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

async fn get_fragment(router: &axum::Router, query: &str) -> (StatusCode, String) {
    let uri = if query.is_empty() {
        "/webui/fragments/torrent-list".to_string()
    } else {
        format!("/webui/fragments/torrent-list?{query}")
    };
    let req = Request::get(uri)
        .body(Body::empty())
        .expect("build fragment request");
    let response = router.clone().oneshot(req).await.expect("fragment");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

// ---------------------------------------------------------------------------
// Test 1 — state filter narrows results
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sidebar_state_filter_narrows_results() {
    let (router, session, _tempdir) = test_router_isolated().await;

    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_A))
        .await
        .expect("add A");
    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_B))
        .await
        .expect("add B");
    session.pause_torrent(id20(HASH_A)).await.expect("pause A");

    let (status, body) = get_fragment(&router, "state=paused").await;
    assert_eq!(status, StatusCode::OK, "fragment status: {body}");
    assert!(
        body.contains(HASH_A),
        "filtered fragment should contain HASH_A; body: {body}"
    );
    assert!(
        !body.contains(HASH_B),
        "filtered fragment must NOT contain HASH_B; body: {body}"
    );
}

// ---------------------------------------------------------------------------
// Test 2 — category filter OR within section
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sidebar_category_filter_or_within_section() {
    let (router, session, _tempdir) = test_router_isolated().await;

    session
        .create_category("movies".into(), std::path::PathBuf::from("/tmp/movies"))
        .await
        .expect("create movies");
    session
        .create_category("tv".into(), std::path::PathBuf::from("/tmp/tv"))
        .await
        .expect("create tv");
    session
        .create_category("music".into(), std::path::PathBuf::from("/tmp/music"))
        .await
        .expect("create music");

    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_A).with_category("movies"))
        .await
        .expect("add A movies");
    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_B).with_category("tv"))
        .await
        .expect("add B tv");
    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_C).with_category("music"))
        .await
        .expect("add C music");

    let (status, body) = get_fragment(&router, "category=movies,tv").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(HASH_A), "movies row missing: {body}");
    assert!(body.contains(HASH_B), "tv row missing: {body}");
    assert!(!body.contains(HASH_C), "music row must be excluded: {body}");
}

// ---------------------------------------------------------------------------
// Test 3 — tag filter with `untagged` sentinel
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sidebar_tag_filter_with_untagged_sentinel() {
    let (router, session, _tempdir) = test_router_isolated().await;

    let results = session.create_tags(vec!["hd".into()]).await;
    for r in &results {
        r.as_ref().expect("create tag");
    }

    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_A).with_tags(vec!["hd".into()]))
        .await
        .expect("add A tagged");
    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_B))
        .await
        .expect("add B untagged");

    let (status, body) = get_fragment(&router, "tag=untagged").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains(HASH_A),
        "tagged row must NOT appear under untagged filter; body: {body}"
    );
    assert!(
        body.contains(HASH_B),
        "untagged row should appear under untagged filter; body: {body}"
    );
}

// ---------------------------------------------------------------------------
// Test 4 — tracker filter strips URL to host
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sidebar_tracker_filter_strips_url_to_host() {
    let (router, session, _tempdir) = test_router_isolated().await;

    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_WITH_TRACKER))
        .await
        .expect("add tracker magnet");
    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_A))
        .await
        .expect("add no-tracker magnet");

    let (status, body) = get_fragment(&router, "tracker=tracker.foo.example").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(HASH_WITH_TRACKER),
        "tracker-host filter should match host, body: {body}"
    );
    assert!(
        !body.contains(HASH_A),
        "tracker-host filter must NOT match no-tracker row, body: {body}"
    );
}

// ---------------------------------------------------------------------------
// Test 5 — cross-section filter ANDs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sidebar_cross_section_filter_ands() {
    let (router, session, _tempdir) = test_router_isolated().await;

    session
        .create_category("movies".into(), std::path::PathBuf::from("/tmp/movies"))
        .await
        .expect("create movies");

    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_A).with_category("movies"))
        .await
        .expect("add A");
    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_B).with_category("movies"))
        .await
        .expect("add B");
    session.pause_torrent(id20(HASH_A)).await.expect("pause A");

    let (status, body) = get_fragment(&router, "state=paused&category=movies").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(HASH_A),
        "AND filter should match A (paused + movies): {body}"
    );
    assert!(
        !body.contains(HASH_B),
        "AND filter must exclude B (movies but not paused): {body}"
    );
}

// ---------------------------------------------------------------------------
// Test 6 — no filter returns all
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sidebar_no_filter_returns_all() {
    let (router, session, _tempdir) = test_router_isolated().await;

    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_A))
        .await
        .expect("add A");
    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_B))
        .await
        .expect("add B");
    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_C))
        .await
        .expect("add C");

    let (status, body) = get_fragment(&router, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(HASH_A), "all-rows must include A: {body}");
    assert!(body.contains(HASH_B), "all-rows must include B: {body}");
    assert!(body.contains(HASH_C), "all-rows must include C: {body}");
}

// ---------------------------------------------------------------------------
// Test 7 — OOB sidebar fragment marker present
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sidebar_oob_fragment_emits_sidebar_swap() {
    let (router, session, _tempdir) = test_router_isolated().await;
    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_A))
        .await
        .expect("add A");

    let (status, body) = get_fragment(&router, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(r#"id="sidebar-sections""#)
            && body.contains(r#"hx-swap-oob="innerHTML""#),
        "fragment must embed OOB sidebar shell, body: {body}"
    );
}

// ---------------------------------------------------------------------------
// Test 8 — invalid filter value is a no-op
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sidebar_invalid_filter_value_is_no_op() {
    let (router, session, _tempdir) = test_router_isolated().await;
    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_A))
        .await
        .expect("add A");
    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_B))
        .await
        .expect("add B");
    session.pause_torrent(id20(HASH_A)).await.expect("pause A");

    let (status, body) = get_fragment(&router, "state=garbage").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains(HASH_A) && !body.contains(HASH_B),
        "garbage-only filter should produce empty result set: {body}"
    );

    let (status, body) = get_fragment(&router, "state=garbage,paused").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(HASH_A),
        "OR with valid value should still match: {body}"
    );
    assert!(
        !body.contains(HASH_B),
        "OR with valid value should still exclude non-matching: {body}"
    );
}

// ---------------------------------------------------------------------------
// Test 9 — XSS escape of reflected filter values
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sidebar_filter_value_xss_escaped() {
    let (router, session, _tempdir) = test_router_isolated().await;
    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_A))
        .await
        .expect("add A");

    let raw = "<script>alert(1)</script>";
    let encoded = pct_encode(raw);
    let (status, body) = get_fragment(&router, &format!("category={encoded}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains(raw),
        "raw <script> tag must NOT appear in response body (XSS sink): {body}"
    );
}

// ---------------------------------------------------------------------------
// Test 10 — chip remains visible at zero count
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sidebar_chip_remains_visible_at_zero_count() {
    let (router, session, _tempdir) = test_router_isolated().await;

    session
        .create_category("movies".into(), std::path::PathBuf::from("/tmp/movies"))
        .await
        .expect("create movies");
    session
        .create_category("tv".into(), std::path::PathBuf::from("/tmp/tv"))
        .await
        .expect("create tv");

    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_A).with_category("movies"))
        .await
        .expect("add A");
    session
        .add_torrent(SessionAddTorrentParams::magnet(MAGNET_B).with_category("tv"))
        .await
        .expect("add B");

    let (status, body) = get_fragment(&router, "category=movies").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(HASH_A), "movies row should appear: {body}");
    assert!(!body.contains(HASH_B), "tv row must be filtered out: {body}");
    assert!(
        body.contains(r#"data-value="tv""#),
        "tv chip must still render so user can broaden: {body}"
    );
    assert!(
        body.contains(r#"data-count="0""#),
        "tv chip should render data-count=\"0\": {body}"
    );
}
