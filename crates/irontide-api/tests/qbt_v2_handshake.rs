//! End-to-end *arr handshake + usage-flow integration tests (M168 Tasks 16+17).
//!
//! These walk the same request sequence that Radarr/Sonarr issue against a
//! real qBt instance, on a single axum test router. If any of these fails,
//! the *arr "Test Connection" button would go red in production.

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use irontide::session::Settings;
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

async fn test_session(qbt_enabled: bool) -> irontide::session::SessionHandle {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let resume_dir =
        std::env::temp_dir().join(format!("irontide-qbt-v2-hs-{}-{}", std::process::id(), n));
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
    settings.qbt_compat.enabled = qbt_enabled;
    irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("failed to start test session")
}

async fn login(router: &axum::Router) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("username=admin&password=adminadmin"))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "login failed");
    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .expect("no Set-Cookie")
        .to_str()
        .unwrap()
        .to_owned();
    let _ = resp.into_body().collect().await.unwrap();
    cookie.split(';').next().unwrap().to_owned()
}

async fn get(router: &axum::Router, uri: &str, cookie: Option<&str>) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }
    let req = builder.body(Body::empty()).unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, body)
}

async fn post(
    router: &axum::Router,
    uri: &str,
    cookie: Option<&str>,
    ct: Option<&str>,
    body: Vec<u8>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method("POST").uri(uri);
    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }
    if let Some(c) = ct {
        builder = builder.header(header::CONTENT_TYPE, c);
    }
    let req = builder.body(Body::from(body)).unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, body)
}

// ── Task 16: handshake flow ───────────────────────────────────────────

/// The exact request sequence Radarr / Sonarr issue when you click
/// "Test Connection". All must succeed for the green-check icon.
#[tokio::test]
async fn test_arr_full_handshake() {
    let session = test_session(true).await;
    let router = build_router(session);

    // 1. POST /api/v2/auth/login
    let sid = login(&router).await;

    // 2. GET /api/v2/app/webapiVersion (with cookie)
    let (status, body) = get(&router, "/api/v2/app/webapiVersion", Some(&sid)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8(body).unwrap(), "2.11.4");

    // 3. GET /api/v2/app/preferences — *arr deserialises this to check fields.
    let (status, body) = get(&router, "/api/v2/app/preferences", Some(&sid)).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Spot-check that all required keys are present.
    for key in [
        "save_path",
        "dht",
        "pex",
        "listen_port",
        "max_ratio",
        "encryption",
        "web_ui_username",
    ] {
        assert!(v.get(key).is_some(), "handshake missing key {key}");
    }

    // 4. GET /api/v2/torrents/categories — *arr uses this to build dropdowns.
    let (status, body) = get(&router, "/api/v2/torrents/categories", Some(&sid)).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.is_object());

    // 5. POST /api/v2/auth/logout → invalidate cookie.
    let (status, body) = post(&router, "/api/v2/auth/logout", Some(&sid), None, Vec::new()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8(body).unwrap(), "Ok.");

    // 6. Subsequent webapiVersion with the now-dead cookie must 403.
    let (status, _) = get(&router, "/api/v2/app/webapiVersion", Some(&sid)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// If the daemon restarts, *arr sees 403 on its next poll and re-authenticates
/// transparently. Simulate via a second login after the first cookie is gone.
#[tokio::test]
async fn test_arr_handshake_with_expired_cookie_recovery() {
    let session = test_session(true).await;
    let router = build_router(session);

    let sid1 = login(&router).await;

    // Logout to simulate cookie expiry.
    let (status, _) = post(
        &router,
        "/api/v2/auth/logout",
        Some(&sid1),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // GET with old cookie → 403.
    let (status, _) = get(&router, "/api/v2/torrents/info", Some(&sid1)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Re-login → fresh cookie.
    let sid2 = login(&router).await;
    assert_ne!(sid1, sid2, "re-login must produce a fresh SID");

    // New cookie works.
    let (status, _) = get(&router, "/api/v2/torrents/info", Some(&sid2)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_arr_handshake_when_disabled_returns_404() {
    let session = test_session(false).await;
    let router = build_router(session);

    // Every /api/v2/* request must return 404 when qbt_compat disabled.
    let uris = [
        "/api/v2/auth/login",
        "/api/v2/app/version",
        "/api/v2/app/preferences",
        "/api/v2/torrents/info",
    ];
    for uri in uris {
        let (status, _) = if uri.contains("auth/login") {
            post(
                &router,
                uri,
                None,
                Some("application/x-www-form-urlencoded"),
                b"username=admin&password=adminadmin".to_vec(),
            )
            .await
        } else {
            get(&router, uri, None).await
        };
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "uri {uri} must 404 when disabled"
        );
    }
}

// ── Task 17: full usage flow — add / observe / pause / delete ─────────

#[tokio::test]
async fn test_arr_full_usage_flow() {
    let session = test_session(true).await;
    let router = build_router(session);
    let sid = login(&router).await;

    // Add a magnet via the qBt add endpoint.
    let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=BigBuckBunny";
    let body = {
        let mut b = String::new();
        b.push_str("urls=");
        // Simple percent-encoding; test only.
        for c in magnet.bytes() {
            match c {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    b.push(c as char);
                }
                _ => { use std::fmt::Write; let _ = write!(b, "%{c:02X}"); }
            }
        }
        b
    };
    let (status, _) = post(
        &router,
        "/api/v2/torrents/add",
        Some(&sid),
        Some("application/x-www-form-urlencoded"),
        body.into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "add magnet must succeed");

    // Give the session a moment to register the add.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // GET torrents/info shows the torrent (may take a tick for the info hash
    // to register; accept empty or populated).
    let (status, body) = get(&router, "/api/v2/torrents/info", Some(&sid)).await;
    assert_eq!(status, StatusCode::OK);
    let list: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let arr = list.as_array().expect("array");
    assert!(!arr.is_empty(), "expected at least one torrent after add");
    let hash = arr
        .first()
        .and_then(|t| t.get("hash"))
        .and_then(|h| h.as_str())
        .unwrap()
        .to_owned();
    assert_eq!(hash.len(), 40, "hash must be 40 hex chars");

    // Pause the torrent. v0.173.1 Class B: send `hashes=` via the
    // `application/x-www-form-urlencoded` body rather than the URL
    // query, matching the real `*arr` request shape. This one call is
    // deliberately routed through the new body-parse path so the
    // end-to-end *arr smoke test exercises Class B parity — the other
    // bulk-action calls below keep the query-string path for breadth.
    let (status, _) = post(
        &router,
        "/api/v2/torrents/pause",
        Some(&sid),
        Some("application/x-www-form-urlencoded"),
        format!("hashes={hash}").into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Give the actor a moment to apply the pause.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Verify state transitioned to a paused variant.
    let (_, body) = get(
        &router,
        &format!("/api/v2/torrents/info?hashes={hash}"),
        Some(&sid),
    )
    .await;
    let list: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let state = list
        .as_array()
        .and_then(|a| a.first())
        .and_then(|t| t.get("state"))
        .and_then(|s| s.as_str())
        .unwrap_or("");
    assert!(
        state == "pausedDL" || state == "pausedUP" || state.starts_with("paused"),
        "state after pause should be paused*, got: {state}"
    );

    // Resume.
    let (status, _) = post(
        &router,
        &format!("/api/v2/torrents/resume?hashes={hash}"),
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Delete.
    let (status, _) = post(
        &router,
        &format!("/api/v2/torrents/delete?hashes={hash}&deleteFiles=false"),
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Wait for removal.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // List should no longer contain this hash.
    let (_, body) = get(
        &router,
        &format!("/api/v2/torrents/info?hashes={hash}"),
        Some(&sid),
    )
    .await;
    let list: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        list.as_array().unwrap().len(),
        0,
        "torrent must be gone after delete"
    );
}

// ── M170: end-to-end *arr workflow (Lane D extension) ────────────────

/// Walks through the full *arr request sequence post-M170:
/// pre-create category → add with category → verify `save_path` inherits
/// the category's path → filter info by category → poll /files (skipped
/// if Lane B's route isn't registered yet) → delete with files=true.
///
/// Every step is a single oneshot — no real network, no background
/// tasks other than the session actor. The test is isolated from the
/// other handshake tests by using a per-process `SESSION_COUNTER` id.
#[tokio::test]
async fn end_to_end_m170_arr_workflow() {
    use irontide::session::SessionAddTorrentParams;

    // Fresh session with qbt_compat enabled + a category registry path
    // that this test owns exclusively.
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let resume_dir = std::env::temp_dir().join(format!("irontide-qbt-v2-hs-m170-resume-{pid}-{n}"));
    let reg_path = std::env::temp_dir().join(format!("irontide-qbt-v2-hs-m170-{pid}-{n}.toml"));
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&reg_path);

    let mut settings = irontide::session::Settings {
        listen_port: 0,
        download_dir: std::path::PathBuf::from("/tmp"),
        enable_dht: false,
        enable_lsd: false,
        enable_upnp: false,
        enable_natpmp: false,
        resume_data_dir: Some(resume_dir),
        save_resume_interval_secs: 0,
        category_registry_path: Some(reg_path),
        ..irontide::session::Settings::default()
    };
    settings.qbt_compat.enabled = true;
    let session = irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("start session");

    // 1. Pre-create `sonarr` category via the session API (the HTTP
    //    `/createCategory` route lands in Lane C, which may or may not
    //    be merged at the same time — using the session API keeps this
    //    test independent).
    let category_save_path = std::path::PathBuf::from("/tmp/irontide-hs-sonarr-e2e");
    session
        .create_category("sonarr".to_string(), category_save_path.clone())
        .await
        .expect("create sonarr category");

    let router = build_router(session.clone());
    let sid = login(&router).await;

    // 2. Add torrent with `category=sonarr` through the HTTP surface.
    let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Bunny";
    let mut body = String::from("urls=");
    for b in magnet.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                body.push(b as char);
            }
            _ => { use std::fmt::Write; let _ = write!(body, "%{b:02X}"); }
        }
    }
    body.push_str("&category=sonarr");
    let (status, _) = post(
        &router,
        "/api/v2/torrents/add",
        Some(&sid),
        Some("application/x-www-form-urlencoded"),
        body.into_bytes(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "add with category=sonarr must succeed"
    );

    // Wait for the torrent to appear + its category label to propagate.
    let mut hash = String::new();
    for _ in 0..50 {
        let (_, b) = get(&router, "/api/v2/torrents/info?category=sonarr", Some(&sid)).await;
        let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
        if let Some(row) = v.as_array().and_then(|a| a.first()) {
            hash = row
                .get("hash")
                .and_then(|h| h.as_str())
                .unwrap_or("")
                .to_owned();
            if !hash.is_empty() {
                // 3. Verify save_path inherits the category's path.
                let sp = row.get("save_path").and_then(|s| s.as_str()).unwrap_or("");
                assert_eq!(
                    sp,
                    category_save_path.to_string_lossy(),
                    "save_path should match category registry entry"
                );
                // 4. The filter itself is already implicitly verified
                //    (arr came from ?category=sonarr).
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(
        !hash.is_empty(),
        "torrent never appeared under category=sonarr"
    );

    // 5. v0.173.2 (A12): the soft `OK | NOT_FOUND` assertion that was here
    //    has been removed. The tight assertion (`OK` after metadata resolves)
    //    lives in tests/qbt_v2_magnet_meta_propagation.rs (A9). This e2e test
    //    remains focused on the M170 HTTP /torrents/add + category=sonarr flow.

    // 6. Delete with deleteFiles=true. For a magnet-only torrent with
    //    no resolved files, this still exercises the deleteFiles code
    //    path; the walker finds no FileMap entries and the download_dir
    //    is untouched.
    let (status, _) = post(
        &router,
        &format!("/api/v2/torrents/delete?hashes={hash}&deleteFiles=true"),
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 7. Confirm the torrent is gone from /info.
    for _ in 0..50 {
        let (_, b) = get(&router, "/api/v2/torrents/info", Some(&sid)).await;
        let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
        if v.as_array().map(Vec::len) == Some(0) {
            // Use SessionAddTorrentParams in the imports check — compile
            // guard so the facade re-export stays consumable from tests.
            let _ = SessionAddTorrentParams::magnet(magnet);
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("torrent never disappeared after delete");
}
