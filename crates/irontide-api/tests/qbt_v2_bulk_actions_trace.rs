//! v0.173.1 Class C regression: the five bulk-action handlers (pause /
//! resume / delete / recheck / reannounce) must log `warn!` events when
//! the underlying session returns an error, rather than silently
//! swallowing it (`let _ = ... .await`). v0.173.0 returned `200 OK`
//! with no log, making the shutdown-race bug invisible.
//!
//! Each handler has ONE `#[tracing_test::traced_test]` test that:
//! 1. Builds a qBt-enabled router with a LIVE session (so `qbt_gate`
//!    admits the request).
//! 2. POSTs to the bulk-action endpoint with a well-formed but
//!    unregistered info-hash (`0000...0000`), which drives the session
//!    to return `Err(Error::TorrentNotFound)` from the per-torrent
//!    command dispatch.
//! 3. Asserts the handler emitted `"{op}_torrent failed"` as a `warn!`
//!    tracing event AND the response is still `200 OK` (qBt bulk-
//!    idempotency — individual torrent errors must not fail the whole
//!    bulk action).
//!
//! # Why not shutdown
//! `qbt_gate` middleware queries `session.settings()` per request. If
//! we shut the session down before the POST, the gate sees `Err`, maps
//! to `false`, and returns 404 — the route never runs. Using a
//! non-existent hash on a live session isolates the bulk-handler error
//! path that Class C was hiding.

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use irontide::session::Settings;
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Build a fresh qBt-enabled session + router with a logged-in SID
/// cookie. The session stays alive for the whole test so `qbt_gate`
/// admits the request.
async fn setup_live() -> (axum::Router, String) {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let resume_dir = std::env::temp_dir().join(format!(
        "irontide-qbt-v2-trace-{}-{}",
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
    let session = irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("start test session");

    let router = build_router(session);

    let form = "username=admin&password=adminadmin";
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
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

async fn post(router: &axum::Router, uri: &str, cookie: &str) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    // Drain body so the listener doesn't complain.
    let _ = resp.into_body().collect().await.unwrap();
    status
}

/// A well-formed but unregistered info-hash. Parses as `Id20` (40 hex
/// chars) so `resolve_hashes` proceeds past its validation, but the
/// session has no torrent by this name → per-torrent command dispatch
/// returns `Err(Error::TorrentNotFound)`.
const UNKNOWN_HASH: &str = "0000000000000000000000000000000000000000";

// ── pause ─────────────────────────────────────────────────────────────

#[tracing_test::traced_test]
#[tokio::test]
async fn pause_logs_warn_when_session_returns_error() {
    let (router, sid) = setup_live().await;

    let status = post(
        &router,
        &format!("/api/v2/torrents/pause?hashes={UNKNOWN_HASH}"),
        &sid,
    )
    .await;
    // qBt bulk-idempotency: the client still sees 200 even though the
    // underlying session errored out with TorrentNotFound.
    assert_eq!(status, StatusCode::OK);
    assert!(
        logs_contain("pause_torrent failed"),
        "expected warn log 'pause_torrent failed' from handler"
    );
}

// ── resume ────────────────────────────────────────────────────────────

#[tracing_test::traced_test]
#[tokio::test]
async fn resume_logs_warn_when_session_returns_error() {
    let (router, sid) = setup_live().await;

    let status = post(
        &router,
        &format!("/api/v2/torrents/resume?hashes={UNKNOWN_HASH}"),
        &sid,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        logs_contain("resume_torrent failed"),
        "expected warn log 'resume_torrent failed' from handler"
    );
}

// ── recheck ───────────────────────────────────────────────────────────

#[tracing_test::traced_test]
#[tokio::test]
async fn recheck_logs_warn_when_session_returns_error() {
    let (router, sid) = setup_live().await;

    let status = post(
        &router,
        &format!("/api/v2/torrents/recheck?hashes={UNKNOWN_HASH}"),
        &sid,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        logs_contain("recheck_torrent failed"),
        "expected warn log 'recheck_torrent failed' from handler"
    );
}

// ── reannounce ────────────────────────────────────────────────────────

#[tracing_test::traced_test]
#[tokio::test]
async fn reannounce_logs_warn_when_session_returns_error() {
    let (router, sid) = setup_live().await;

    let status = post(
        &router,
        &format!("/api/v2/torrents/reannounce?hashes={UNKNOWN_HASH}"),
        &sid,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        logs_contain("reannounce_torrent failed"),
        "expected warn log 'reannounce_torrent failed' from handler"
    );
}

// ── delete ────────────────────────────────────────────────────────────

#[tracing_test::traced_test]
#[tokio::test]
async fn delete_logs_warn_when_session_returns_error() {
    let (router, sid) = setup_live().await;

    let status = post(
        &router,
        &format!("/api/v2/torrents/delete?hashes={UNKNOWN_HASH}&deleteFiles=false"),
        &sid,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        logs_contain("delete_torrent failed"),
        "expected warn log 'delete_torrent failed' from handler"
    );
}
