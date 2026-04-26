//! M171 Lane B5: belt-and-braces sweep of the auth / compat-gate
//! surface across all four B1-B4 endpoints.
//!
//! Each of B1-B4 already covers its own 403 / 404 / 400 paths — this
//! file duplicates those checks in a single unified harness so a
//! future middleware refactor breaks one per-endpoint test as well as
//! one cross-endpoint matrix test. The matrix form makes the failure
//! mode immediately obvious in CI output.
//!
//! Covered endpoints: `/trackers`, `/webseeds`, `/pieceStates`,
//! `/pieceHashes`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use irontide::session::{SessionHandle, Settings};
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fresh_paths() -> (PathBuf, PathBuf) {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let resume_dir = std::env::temp_dir().join(format!("irontide-qbt-v2-b5-resume-{pid}-{n}"));
    let reg_path = std::env::temp_dir().join(format!("irontide-qbt-v2-b5-{pid}-{n}.toml"));
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&reg_path);
    (resume_dir, reg_path)
}

fn default_settings() -> Settings {
    let (resume_dir, reg_path) = fresh_paths();
    let mut settings = Settings {
        listen_port: 0,
        download_dir: std::path::PathBuf::from("/tmp"),
        enable_dht: false,
        enable_pex: false,
        enable_lsd: false,
        enable_upnp: false,
        enable_natpmp: false,
        resume_data_dir: Some(resume_dir),
        save_resume_interval_secs: 0,
        category_registry_path: Some(reg_path),
        ..Settings::default()
    };
    settings.qbt_compat.enabled = true;
    settings
}

async fn start_session(settings: Settings) -> SessionHandle {
    irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("start session")
}

async fn login(router: &axum::Router) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("username=admin&password=adminadmin"))
        .expect("build login");
    let resp = router.clone().oneshot(req).await.expect("login");
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .expect("Set-Cookie")
        .to_str()
        .expect("utf-8")
        .to_owned();
    let _ = resp.into_body().collect().await.expect("drain");
    cookie.split(';').next().expect("cookie").to_owned()
}

/// The four Lane B endpoints — every one takes `?hash=` as a query
/// param. The matrix tests below iterate over this array rather than
/// listing per-endpoint copies.
const DETAIL_ENDPOINTS: &[&str] = &[
    "/api/v2/torrents/trackers",
    "/api/v2/torrents/webseeds",
    "/api/v2/torrents/pieceStates",
    "/api/v2/torrents/pieceHashes",
];

/// `?hash=` param that's well-formed hex but references no real torrent.
const UNKNOWN_HASH: &str = "0123456789abcdef0123456789abcdef01234567";

#[tokio::test]
async fn all_b1_b4_endpoints_require_sid_cookie() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());

    for endpoint in DETAIL_ENDPOINTS {
        let req = Request::builder()
            .method("GET")
            .uri(format!("{endpoint}?hash={UNKNOWN_HASH}"))
            .body(Body::empty())
            .expect("build GET");
        let resp = router.clone().oneshot(req).await.expect("GET");
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "{endpoint}: missing SID cookie must return 403"
        );
    }
}

#[tokio::test]
async fn all_b1_b4_endpoints_return_404_when_qbt_compat_disabled() {
    let mut settings = default_settings();
    settings.qbt_compat.enabled = false;
    let session = start_session(settings).await;
    let router = build_router(session.clone());

    for endpoint in DETAIL_ENDPOINTS {
        let req = Request::builder()
            .method("GET")
            .uri(format!("{endpoint}?hash={UNKNOWN_HASH}"))
            .body(Body::empty())
            .expect("build GET");
        let resp = router.clone().oneshot(req).await.expect("GET");
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "{endpoint}: disabled qbt_compat must return 404 (security-through-invisibility)"
        );
    }
}

#[tokio::test]
async fn all_b1_b4_endpoints_reject_malformed_hash() {
    let session = start_session(default_settings()).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    for endpoint in DETAIL_ENDPOINTS {
        let req = Request::builder()
            .method("GET")
            .uri(format!("{endpoint}?hash=not-a-hash"))
            .header(header::COOKIE, &sid)
            .body(Body::empty())
            .expect("build GET");
        let resp = router.clone().oneshot(req).await.expect("GET");
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "{endpoint}: malformed hash must return 400"
        );
    }
}
