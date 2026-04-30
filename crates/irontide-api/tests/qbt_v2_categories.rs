//! Integration tests for qBt v2 category CRUD (M170 Lane C).
//!
//! Each test boots an isolated session (unique `resume_data_dir` +
//! `category_registry_path`) so concurrent tests cannot see each other's
//! state. The session speaks HTTP via the `build_router` test router and is
//! driven through the same URL-encoded form bodies that Sonarr/Radarr send.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use irontide::session::{SessionAddTorrentParams, SessionHandle, Settings};
use irontide_api::routes::build_router;

// ── Fixtures ─────────────────────────────────────────────────────────

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Allocate a fresh `(resume_data_dir, category_registry_path)` pair so
/// parallel tests never share TOML state. The counter is global + atomic
/// across the test binary; `std::process::id()` disambiguates parallel
/// `cargo test` invocations from different shells on the same box.
fn fresh_paths() -> (PathBuf, PathBuf) {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let resume_dir = std::env::temp_dir().join(format!("irontide-qbt-v2-cat-resume-{pid}-{n}"));
    let reg_path = std::env::temp_dir().join(format!("irontide-qbt-v2-cat-{pid}-{n}.toml"));
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&reg_path);
    (resume_dir, reg_path)
}

async fn session_with(registry_path: PathBuf, resume_dir: PathBuf) -> SessionHandle {
    let mut settings = Settings {
        listen_port: 0,
        download_dir: std::path::PathBuf::from("/tmp"),
        enable_dht: false,
        enable_lsd: false,
        enable_upnp: false,
        enable_natpmp: false,
        resume_data_dir: Some(resume_dir),
        save_resume_interval_secs: 0,
        category_registry_path: Some(registry_path),
        ..Settings::default()
    };
    settings.qbt_compat.enabled = true;
    irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("failed to start test session")
}

/// Convenience wrapper: fresh paths + enabled qBt gate.
async fn test_session() -> (SessionHandle, PathBuf) {
    let (resume_dir, reg_path) = fresh_paths();
    let session = session_with(reg_path.clone(), resume_dir).await;
    (session, reg_path)
}

async fn login(router: &axum::Router) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("username=admin&password=adminadmin"))
        .expect("build login request");
    let resp = router
        .clone()
        .oneshot(req)
        .await
        .expect("login request failed");
    assert_eq!(resp.status(), StatusCode::OK, "login failed");
    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .expect("no Set-Cookie header")
        .to_str()
        .expect("cookie is not valid utf-8")
        .to_owned();
    let _ = resp.into_body().collect().await.expect("drain login body");
    cookie.split(';').next().expect("empty cookie").to_owned()
}

async fn get(router: &axum::Router, uri: &str, cookie: &str) -> (StatusCode, Vec<u8>) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .expect("build GET request");
    let resp = router
        .clone()
        .oneshot(req)
        .await
        .expect("GET request failed");
    let status = resp.status();
    let body = resp
        .into_body()
        .collect()
        .await
        .expect("drain body")
        .to_bytes()
        .to_vec();
    (status, body)
}

async fn post_form(
    router: &axum::Router,
    uri: &str,
    cookie: &str,
    body: &str,
) -> (StatusCode, Vec<u8>) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.to_owned()))
        .expect("build POST request");
    let resp = router
        .clone()
        .oneshot(req)
        .await
        .expect("POST request failed");
    let status = resp.status();
    let body = resp
        .into_body()
        .collect()
        .await
        .expect("drain body")
        .to_bytes()
        .to_vec();
    (status, body)
}

/// Percent-encode the bytes that would otherwise confuse
/// `application/x-www-form-urlencoded`. Test-only; keeps the spec small.
fn form_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => { use std::fmt::Write; let _ = write!(out, "%{b:02X}"); }
        }
    }
    out
}

fn create_body(name: &str, path: &str) -> String {
    format!(
        "category={}&savePath={}",
        form_encode(name),
        form_encode(path)
    )
}

// ── Tests ────────────────────────────────────────────────────────────

/// A fresh session with no prior state must return `{}` from
/// `/torrents/categories`.
#[tokio::test]
async fn list_empty_when_no_categories() {
    let (session, _reg) = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (status, body) = get(&router, "/api/v2/torrents/categories", &sid).await;
    assert_eq!(status, StatusCode::OK);
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v, serde_json::json!({}));
}

/// `createCategory` with valid fields returns 200; the new category shows
/// up on `/torrents/categories` with the same `savePath`.
#[tokio::test]
async fn create_category_success() {
    let (session, _reg) = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/createCategory",
        &sid,
        &create_body("sonarr", "/mnt/tv"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&router, "/api/v2/torrents/categories", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v["sonarr"]["name"], "sonarr");
    assert_eq!(v["sonarr"]["savePath"], "/mnt/tv");
}

/// Creating the same name twice returns 409 on the second call.
#[tokio::test]
async fn create_category_duplicate_returns_409() {
    let (session, _reg) = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    let body = create_body("sonarr", "/mnt/tv");
    let (status, _) = post_form(&router, "/api/v2/torrents/createCategory", &sid, &body).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = post_form(&router, "/api/v2/torrents/createCategory", &sid, &body).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

/// Invalid names (empty, `..` traversal, illegal chars) must 400.
#[tokio::test]
async fn create_category_invalid_name_returns_400() {
    let (session, _reg) = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    // Empty name → required-field error lands at 400.
    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/createCategory",
        &sid,
        "category=&savePath=/tmp/x",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // `..` traversal → validation error lands at 400.
    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/createCategory",
        &sid,
        &create_body("..", "/tmp/x"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Space in name (rejected by the validator).
    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/createCategory",
        &sid,
        &create_body("bad name", "/tmp/x"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// M170 treats nested names (`a/b/c`) as labels only — no directory is
/// materialised on disk from the category name. The `save_path` is used
/// verbatim as supplied.
#[tokio::test]
async fn create_category_nested_name_is_label_only() {
    let (session, _reg) = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    // Use a save_path under a unique tmp dir so the assertion doesn't
    // interact with any real filesystem state.
    let tmp_root = std::env::temp_dir().join(format!(
        "irontide-cat-nested-{}-{}",
        std::process::id(),
        SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let save_path = tmp_root.join("flat-label");
    let _ = std::fs::remove_dir_all(&tmp_root);

    let body = create_body("movies/4k", &save_path.to_string_lossy());
    let (status, _) = post_form(&router, "/api/v2/torrents/createCategory", &sid, &body).await;
    assert_eq!(status, StatusCode::OK);

    // Crucially: no "movies/4k" directory materialised anywhere.
    assert!(
        !save_path.join("movies").exists(),
        "nested name must not be expanded into directory prefix"
    );
    assert!(
        !save_path.join("movies/4k").exists(),
        "nested name must not materialise as directory hierarchy"
    );
}

/// `editCategory` with a fresh savePath updates the entry in-place.
#[tokio::test]
async fn edit_category_success() {
    let (session, _reg) = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/createCategory",
        &sid,
        &create_body("sonarr", "/mnt/tv-old"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/editCategory",
        &sid,
        &create_body("sonarr", "/mnt/tv-new"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&router, "/api/v2/torrents/categories", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v["sonarr"]["savePath"], "/mnt/tv-new");
}

/// Editing a non-existent category returns 404.
#[tokio::test]
async fn edit_category_missing_returns_404() {
    let (session, _reg) = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/editCategory",
        &sid,
        &create_body("ghost", "/tmp/x"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Removing a single category clears the list.
#[tokio::test]
async fn remove_categories_single() {
    let (session, _reg) = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/createCategory",
        &sid,
        &create_body("sonarr", "/mnt/tv"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/removeCategories",
        &sid,
        &format!("categories={}", form_encode("sonarr")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&router, "/api/v2/torrents/categories", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v, serde_json::json!({}));
}

/// qBt encodes multiple removals as a single form field with URL-encoded
/// newlines between names. Verify we split cleanly on `\n`.
#[tokio::test]
async fn remove_categories_multi() {
    let (session, _reg) = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    for name in ["sonarr", "radarr", "lidarr"] {
        let (status, _) = post_form(
            &router,
            "/api/v2/torrents/createCategory",
            &sid,
            &create_body(name, &format!("/mnt/{name}")),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "create {name} must succeed");
    }

    let joined = "sonarr\nradarr\nlidarr";
    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/removeCategories",
        &sid,
        &format!("categories={}", form_encode(joined)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&router, "/api/v2/torrents/categories", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v, serde_json::json!({}));
}

/// Removing names that were never registered is tolerated — returns 200
/// with no side effects on anything else.
#[tokio::test]
async fn remove_categories_unknown_tolerated() {
    let (session, _reg) = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    // Pre-seed one real category so we can confirm it survives the
    // unknown-only remove.
    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/createCategory",
        &sid,
        &create_body("sonarr", "/mnt/tv"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/removeCategories",
        &sid,
        &format!("categories={}", form_encode("ghost1\nghost2")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&router, "/api/v2/torrents/categories", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v["sonarr"]["savePath"], "/mnt/tv");
}

/// After removing a category, any torrent that was previously tagged with
/// it must have its `category` label cleared. We go through the in-process
/// session handle (not HTTP) to assert on `torrent_stats.category`.
#[tokio::test]
async fn remove_categories_clears_label_on_assigned_torrents() {
    let (session, _reg) = test_session().await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    // Pre-create category via the API.
    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/createCategory",
        &sid,
        &create_body("sonarr", "/tmp/sonarr-label-test"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Add a magnet tagged with the category (no need to resolve metadata —
    // the category label is recorded on the stats immediately).
    let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=BigBuckBunny";
    let params = SessionAddTorrentParams::magnet(magnet).with_category("sonarr");
    let hash = session
        .add_torrent(params)
        .await
        .expect("add magnet with category");

    // Lane A spawns a fire-and-forget that records the category on the
    // torrent after the add returns — give it a small window to run.
    let mut saw_category = false;
    for _ in 0..50 {
        let stats = session
            .torrent_stats(hash)
            .await
            .expect("torrent_stats pre-remove");
        if stats.category.as_deref() == Some("sonarr") {
            saw_category = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        saw_category,
        "category label should land on TorrentStats within 1s of add"
    );

    // Remove the category. The session must clear the label as a side
    // effect, not just drop it from the registry.
    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/removeCategories",
        &sid,
        &format!("categories={}", form_encode("sonarr")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Poll: the clear happens asynchronously inside the session actor.
    let mut cleared = false;
    for _ in 0..50 {
        let stats = session
            .torrent_stats(hash)
            .await
            .expect("torrent_stats post-remove");
        if stats.category.is_none() {
            cleared = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        cleared,
        "removing a category must clear the label on assigned torrents"
    );
}

/// Two concurrent `createCategory` calls for the same name: exactly one
/// wins with 200, exactly one loses with 409. Guards against a TOCTOU
/// race inside the session actor — if both saw "does not exist" before
/// either wrote, we'd get two 200s.
#[tokio::test]
async fn concurrent_create_same_name_returns_one_success() {
    let (session, _reg) = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    let body = create_body("sonarr", "/mnt/tv");
    let router_a = router.clone();
    let router_b = router.clone();
    let sid_a = sid.clone();
    let sid_b = sid.clone();
    let body_a = body.clone();
    let body_b = body.clone();

    let (res_a, res_b) = tokio::join!(
        tokio::spawn(async move {
            post_form(
                &router_a,
                "/api/v2/torrents/createCategory",
                &sid_a,
                &body_a,
            )
            .await
        }),
        tokio::spawn(async move {
            post_form(
                &router_b,
                "/api/v2/torrents/createCategory",
                &sid_b,
                &body_b,
            )
            .await
        }),
    );
    let (status_a, _) = res_a.expect("task a joined");
    let (status_b, _) = res_b.expect("task b joined");

    let mut statuses = [status_a, status_b];
    statuses.sort_by_key(axum::http::StatusCode::as_u16);
    assert_eq!(
        statuses,
        [StatusCode::OK, StatusCode::CONFLICT],
        "expected one 200 and one 409, got {statuses:?}"
    );
}

/// Categories must survive a session restart. We use the same registry
/// path for both runs; after the first drops, the second reloads the
/// TOML file and lists the previously-created entries.
#[tokio::test]
async fn registry_persists_across_session_restart() {
    let (resume_dir, reg_path) = fresh_paths();

    // First run: create two categories and drop the session.
    {
        let session = session_with(reg_path.clone(), resume_dir.clone()).await;
        let router = build_router(session);
        let sid = login(&router).await;

        for (name, path) in [("sonarr", "/mnt/tv"), ("radarr", "/mnt/movies")] {
            let (status, _) = post_form(
                &router,
                "/api/v2/torrents/createCategory",
                &sid,
                &create_body(name, path),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
        }

        // Writes inside the session actor are spawn_blocking'd after the
        // handler returns 200. Give the disk flush a brief window.
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Confirm the TOML was materialised before we tear down.
        assert!(
            reg_path.exists(),
            "category registry file must exist after create"
        );
    }

    // Second run: reload from the same TOML file.
    let session = session_with(reg_path.clone(), resume_dir).await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (_, body) = get(&router, "/api/v2/torrents/categories", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v["sonarr"]["savePath"], "/mnt/tv");
    assert_eq!(v["radarr"]["savePath"], "/mnt/movies");
}

/// A user who hand-edits `categories.toml` while the daemon is down must
/// see their additions on the next boot. Matches the plan's soft-recover
/// semantic — TOML is the source of truth on load.
#[tokio::test]
async fn hand_edited_toml_pre_seed_loads() {
    let (resume_dir, reg_path) = fresh_paths();

    // Write a valid TOML file before the session starts.
    let hand_edited = r#"version = 1

[categories.prowlarr]
save_path = "/mnt/indexers"

[categories.lidarr]
save_path = "/mnt/music"
"#;
    if let Some(parent) = reg_path.parent() {
        std::fs::create_dir_all(parent).expect("create registry parent");
    }
    std::fs::write(&reg_path, hand_edited).expect("write hand-edited toml");

    let session = session_with(reg_path, resume_dir).await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (status, body) = get(&router, "/api/v2/torrents/categories", &sid).await;
    assert_eq!(status, StatusCode::OK);
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v["prowlarr"]["savePath"], "/mnt/indexers");
    assert_eq!(v["lidarr"]["savePath"], "/mnt/music");
}

/// Malformed TOML is soft-recovered: boot continues with an empty
/// registry and the broken file is renamed to `.bak`. Daemon does not
/// panic and the API still serves `{}`.
#[tokio::test]
async fn malformed_toml_soft_recovers() {
    let (resume_dir, reg_path) = fresh_paths();

    let garbage = b"this is not valid toml!!! = [\nbad = ]";
    if let Some(parent) = reg_path.parent() {
        std::fs::create_dir_all(parent).expect("create registry parent");
    }
    std::fs::write(&reg_path, garbage).expect("write malformed toml");

    let session = session_with(reg_path.clone(), resume_dir).await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (status, body) = get(&router, "/api/v2/torrents/categories", &sid).await;
    assert_eq!(status, StatusCode::OK);
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v, serde_json::json!({}), "registry must start empty");

    // The broken file should have been renamed aside. Depending on the
    // original extension, the suffix could be either `.bak` (when the
    // original had a `.toml` suffix → `.toml.bak`) or a bare `.bak`.
    let toml_bak = reg_path.with_extension("toml.bak");
    let plain_bak = reg_path.with_extension("bak");
    assert!(
        toml_bak.exists() || plain_bak.exists(),
        "malformed file should have been renamed aside (expected one of {} or {} to exist)",
        toml_bak.display(),
        plain_bak.display(),
    );

    // The malformed content must still be retrievable from the .bak.
    let bak_path = if toml_bak.exists() {
        toml_bak
    } else {
        plain_bak
    };
    let bak_contents = std::fs::read(&bak_path).expect("read .bak");
    assert_eq!(
        bak_contents, garbage,
        "the .bak file must contain the original malformed content verbatim"
    );
}

/// qBt preserves case distinction; `Sonarr` and `sonarr` are two
/// independent categories. A case-insensitive fold on our side would
/// silently merge what qBt treats as separate, breaking any *arr stack
/// that happens to capitalise differently.
#[tokio::test]
async fn category_names_are_case_sensitive() {
    let (session, _reg) = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    for (name, path) in [("Sonarr", "/mnt/tv-upper"), ("sonarr", "/mnt/tv-lower")] {
        let (status, _) = post_form(
            &router,
            "/api/v2/torrents/createCategory",
            &sid,
            &create_body(name, path),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "create {name} must succeed");
    }

    let (_, body) = get(&router, "/api/v2/torrents/categories", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v["Sonarr"]["savePath"], "/mnt/tv-upper");
    assert_eq!(v["sonarr"]["savePath"], "/mnt/tv-lower");

    // Editing the lowercase one must not touch the uppercase one.
    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/editCategory",
        &sid,
        &create_body("sonarr", "/mnt/tv-lower-2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&router, "/api/v2/torrents/categories", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(
        v["Sonarr"]["savePath"], "/mnt/tv-upper",
        "uppercase variant must not be touched by the lowercase edit"
    );
    assert_eq!(v["sonarr"]["savePath"], "/mnt/tv-lower-2");
}
