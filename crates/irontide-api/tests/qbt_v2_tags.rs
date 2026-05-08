#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M175: integration test code — fixtures use bounded sizes that fit narrower types"
)]

//! Integration tests for M171 Lane C: qBt v2 tags CRUD + per-torrent
//! assignment.
//!
//! Every request walks the live middleware chain (`qbt_gate` ->
//! `require_sid` -> handler), so the tests exercise the same path real
//! `*arr` clients do. Each test boots an isolated session (unique
//! `resume_data_dir`, `category_registry_path`, and `tag_registry_path`)
//! so concurrent tests never share TOML state.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde::Serialize;
use serde_bytes::ByteBuf;
use serde_json::Value;
use tower::ServiceExt;

use irontide::session::{SessionAddTorrentParams, SessionHandle, Settings};
use irontide_api::routes::build_router;

// ── Fixtures ─────────────────────────────────────────────────────────

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Allocate a fresh `(resume_data_dir, category_registry_path,
/// tag_registry_path)` triple so parallel tests never share TOML state.
/// The counter is global + atomic across the test binary;
/// `std::process::id()` disambiguates parallel `cargo test` invocations
/// from different shells.
fn fresh_paths() -> (PathBuf, PathBuf, PathBuf) {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let resume_dir = std::env::temp_dir().join(format!("irontide-qbt-v2-tags-resume-{pid}-{n}"));
    let cat_path = std::env::temp_dir().join(format!("irontide-qbt-v2-tags-cat-{pid}-{n}.toml"));
    let tag_path = std::env::temp_dir().join(format!("irontide-qbt-v2-tags-{pid}-{n}.toml"));
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&cat_path);
    let _ = std::fs::remove_file(&tag_path);
    (resume_dir, cat_path, tag_path)
}

/// Build an isolated session with `qbt_compat.enabled = true` and all
/// discovery subsystems disabled (tests are deterministic and offline).
async fn session_with(resume_dir: PathBuf, cat_path: PathBuf, tag_path: PathBuf) -> SessionHandle {
    let mut settings = Settings {
        listen_port: 0,
        download_dir: PathBuf::from("/tmp"),
        enable_dht: false,
        enable_pex: false,
        enable_lsd: false,
        enable_upnp: false,
        enable_natpmp: false,
        resume_data_dir: Some(resume_dir),
        save_resume_interval_secs: 0,
        category_registry_path: Some(cat_path),
        tag_registry_path: Some(tag_path),
        ..Settings::default()
    };
    settings.qbt_compat.enabled = true;
    irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("start session")
}

async fn test_session() -> SessionHandle {
    let (resume_dir, cat_path, tag_path) = fresh_paths();
    session_with(resume_dir, cat_path, tag_path).await
}

async fn login(router: &axum::Router) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("username=admin&password=adminadmin"))
        .expect("build login");
    let resp = router.clone().oneshot(req).await.expect("login");
    assert_eq!(resp.status(), StatusCode::OK, "login failed");
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

async fn get(router: &axum::Router, uri: &str, cookie: &str) -> (StatusCode, Vec<u8>) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .expect("build GET");
    let resp = router.clone().oneshot(req).await.expect("GET");
    let status = resp.status();
    let body = resp
        .into_body()
        .collect()
        .await
        .expect("drain")
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
        .expect("build POST");
    let resp = router.clone().oneshot(req).await.expect("POST");
    let status = resp.status();
    let body = resp
        .into_body()
        .collect()
        .await
        .expect("drain")
        .to_bytes()
        .to_vec();
    (status, body)
}

/// Percent-encode form-unsafe bytes. Test-only, keeps the spec small.
fn form_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

// ── Minimal torrent fixture (for tests that need a real torrent) ────

#[derive(Serialize)]
struct TestTorrent {
    info: TestInfo,
}

#[derive(Serialize)]
struct TestInfo {
    length: u64,
    name: String,
    #[serde(rename = "piece length")]
    piece_length: u64,
    pieces: ByteBuf,
}

/// Build a minimal single-file torrent with no announce URL. The data is
/// deterministic so the info-hash is stable across test runs.
fn make_torrent(name: &str) -> Vec<u8> {
    let piece_length: u64 = 16_384;
    let data = vec![0xAB_u8; (piece_length as usize) * 2];
    let mut pieces = Vec::with_capacity(40);
    for chunk in data.chunks(piece_length as usize) {
        let h = irontide::core::sha1(chunk);
        pieces.extend_from_slice(h.as_bytes());
    }
    let t = TestTorrent {
        info: TestInfo {
            length: data.len() as u64,
            name: name.into(),
            piece_length,
            pieces: ByteBuf::from(pieces),
        },
    };
    irontide::bencode::to_bytes(&t).expect("bencode")
}

/// Add a torrent, wait until `torrent_stats` succeeds (i.e. the actor is
/// wired up), return its info-hash hex.
async fn add_and_wait(session: &SessionHandle, params: SessionAddTorrentParams) -> String {
    let hash = session.add_torrent(params).await.expect("add torrent");
    for _ in 0..50 {
        if session.torrent_stats(hash).await.is_ok() {
            return hash.to_hex();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("torrent stats never became queryable");
}

// ── Tests ─────────────────────────────────────────────────────────────

/// A fresh session with no prior state returns `[]` from `/torrents/tags`.
#[tokio::test]
async fn list_empty_when_no_tags() {
    let session = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (status, body) = get(&router, "/api/v2/torrents/tags", &sid).await;
    assert_eq!(status, StatusCode::OK);
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v, serde_json::json!([]));
}

/// `createTags` with valid names returns 200; `/torrents/tags` surfaces
/// the new names sorted alphabetically.
#[tokio::test]
async fn create_then_list_returns_both_tags() {
    let session = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/createTags",
        &sid,
        "tags=sonarr,kids",
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&router, "/api/v2/torrents/tags", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v, serde_json::json!(["kids", "sonarr"]));
}

/// Creating the same tag twice must still return 200 (qBt parity: lenient
/// create). The registry contains exactly one entry after the duplicate call.
#[tokio::test]
async fn create_duplicate_is_idempotent_no_error() {
    let session = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (status, _) = post_form(&router, "/api/v2/torrents/createTags", &sid, "tags=sonarr").await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = post_form(&router, "/api/v2/torrents/createTags", &sid, "tags=sonarr").await;
    assert_eq!(status, StatusCode::OK, "duplicate must not 409");

    let (_, body) = get(&router, "/api/v2/torrents/tags", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v, serde_json::json!(["sonarr"]));
}

/// Invalid tag names (space, path traversal) are silently dropped with a
/// WARN log but still return 200. Registry remains empty.
#[tokio::test]
async fn create_invalid_name_returns_200_with_warn() {
    let session = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    let body = format!("tags={}", form_encode("bad name"));
    let (status, _) = post_form(&router, "/api/v2/torrents/createTags", &sid, &body).await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&router, "/api/v2/torrents/tags", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v, serde_json::json!([]), "invalid name must not persist");
}

/// Deleting a non-existent tag returns 200 (idempotent).
#[tokio::test]
async fn delete_nonexistent_returns_200() {
    let session = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (status, _) = post_form(&router, "/api/v2/torrents/deleteTags", &sid, "tags=ghost").await;
    assert_eq!(status, StatusCode::OK);
}

/// `deleteTags` removes the named tags from both the registry AND any
/// torrents they're currently attached to.
#[tokio::test]
async fn delete_removes_from_list_and_from_torrents() {
    let session = test_session().await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    // Create two tags.
    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/createTags",
        &sid,
        "tags=sonarr,kids",
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Add a torrent and tag it with both.
    let bytes = make_torrent("del-test.bin");
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let body = format!("hashes={hash}&tags=sonarr,kids");
    let (status, _) = post_form(&router, "/api/v2/torrents/addTags", &sid, &body).await;
    assert_eq!(status, StatusCode::OK);

    // Delete one of the tags.
    let (status, _) = post_form(&router, "/api/v2/torrents/deleteTags", &sid, "tags=sonarr").await;
    assert_eq!(status, StatusCode::OK);

    // Registry only has "kids".
    let (_, body) = get(&router, "/api/v2/torrents/tags", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v, serde_json::json!(["kids"]));

    // Give the actor a beat to apply the strip via set_tags.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Torrent stats only have "kids".
    let id = irontide::core::Id20::from_hex(&hash).expect("hex roundtrip");
    let stats = session.torrent_stats(id).await.expect("stats");
    assert_eq!(stats.tags, vec!["kids".to_string()]);
}

/// Missing auth returns 403 (qBt convention: `Fails.` body on forbidden).
#[tokio::test]
async fn auth_required_403_without_sid() {
    let session = test_session().await;
    let router = build_router(session);

    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/torrents/tags")
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// When `qbt_compat.enabled = false`, the entire /api/v2/* surface
/// responds 404 via the `qbt_gate` middleware (security-through-invisibility).
#[tokio::test]
async fn qbt_compat_disabled_returns_404() {
    let (resume_dir, cat_path, tag_path) = fresh_paths();
    let mut settings = Settings {
        listen_port: 0,
        download_dir: PathBuf::from("/tmp"),
        enable_dht: false,
        enable_pex: false,
        enable_lsd: false,
        enable_upnp: false,
        enable_natpmp: false,
        resume_data_dir: Some(resume_dir),
        save_resume_interval_secs: 0,
        category_registry_path: Some(cat_path),
        tag_registry_path: Some(tag_path),
        ..Settings::default()
    };
    settings.qbt_compat.enabled = false;
    let session = irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("start session");
    let router = build_router(session);

    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/torrents/tags")
        .body(Body::empty())
        .expect("build GET");
    let resp = router.oneshot(req).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

/// Tag names are case-sensitive: `Sonarr` and `sonarr` are distinct.
#[tokio::test]
async fn case_sensitive_names() {
    let session = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/createTags",
        &sid,
        "tags=Sonarr,sonarr",
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&router, "/api/v2/torrents/tags", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v, serde_json::json!(["Sonarr", "sonarr"]));
}

/// The splitter accepts a mix of comma and newline separators in one
/// request - matches qBt parsing behaviour.
#[tokio::test]
async fn newline_and_comma_separated_both_accepted() {
    let session = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    // `a,b%0Ac` decodes to `a,b\nc` in the form value.
    let (status, _) = post_form(&router, "/api/v2/torrents/createTags", &sid, "tags=a,b%0Ac").await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&router, "/api/v2/torrents/tags", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v, serde_json::json!(["a", "b", "c"]));
}

/// Tags persist across a shutdown / restart round-trip. First session
/// creates them, second session (same `tag_registry_path`) lists them.
#[tokio::test]
async fn tags_persist_across_restart() {
    let (resume_dir, cat_path, tag_path) = fresh_paths();

    // First session: create the tags.
    {
        let session = session_with(resume_dir.clone(), cat_path.clone(), tag_path.clone()).await;
        let router = build_router(session.clone());
        let sid = login(&router).await;

        let (status, _) = post_form(
            &router,
            "/api/v2/torrents/createTags",
            &sid,
            "tags=sonarr,kids",
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        session.shutdown().await.expect("shutdown");
    }

    // Second session: same tag registry path, load from disk.
    {
        let session = session_with(resume_dir.clone(), cat_path.clone(), tag_path.clone()).await;
        let router = build_router(session.clone());
        let sid = login(&router).await;

        let (status, body) = get(&router, "/api/v2/torrents/tags", &sid).await;
        assert_eq!(status, StatusCode::OK);
        let v: Value = serde_json::from_slice(&body).expect("parse JSON");
        assert_eq!(
            v,
            serde_json::json!(["kids", "sonarr"]),
            "tags must survive restart"
        );

        session.shutdown().await.expect("shutdown");
    }

    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&cat_path);
    let _ = std::fs::remove_file(&tag_path);
}

/// `tags=` with an empty value is a 200 no-op (qBt parity).
#[tokio::test]
async fn empty_tags_param_is_noop_200() {
    let session = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (status, _) = post_form(&router, "/api/v2/torrents/createTags", &sid, "tags=").await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&router, "/api/v2/torrents/tags", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(v, serde_json::json!([]));
}

/// Names longer than the registry's max length (255 bytes) are rejected
/// by the validator; the wire layer still returns 200 (lenient create).
/// Registry stays empty.
#[tokio::test]
async fn max_tag_name_length_rejected() {
    let session = test_session().await;
    let router = build_router(session);
    let sid = login(&router).await;

    // 256 chars of `a`, one byte over the 255-byte limit.
    let long = "a".repeat(256);
    let body = format!("tags={long}");
    let (status, _) = post_form(&router, "/api/v2/torrents/createTags", &sid, &body).await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = get(&router, "/api/v2/torrents/tags", &sid).await;
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    assert_eq!(
        v,
        serde_json::json!([]),
        "over-length name must not persist"
    );
}

// ── E0 mandatory tests ───────────────────────────────────────────────

/// E0.1 (CRITICAL REGRESSION): adding a torrent with BOTH a category and
/// tags must result in both showing up on `stats.category` and `stats.tags`,
/// AND both must survive a `save_resume_state` / restart round-trip.
#[tokio::test]
async fn category_and_tags_coexist_and_persist() {
    // Two back-to-back sessions against the same paths, same pattern as
    // qbt_v2_add_tags_session.rs. Can't use `fresh_paths` since we want
    // to reuse across two sessions.
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let resume_dir = std::env::temp_dir().join(format!("irontide-m171-e0-1-resume-{pid}-{n}"));
    let cat_path = std::env::temp_dir().join(format!("irontide-m171-e0-1-cat-{pid}-{n}.toml"));
    let tag_path = std::env::temp_dir().join(format!("irontide-m171-e0-1-tag-{pid}-{n}.toml"));
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&cat_path);
    let _ = std::fs::remove_file(&tag_path);

    let magnet_uri = "magnet:?xt=urn:btih:4142434445464748494a4b4c4d4e4f5051525354&dn=E01";
    let expected_tags = vec!["sonarr".to_string(), "kids".to_string()];

    let info_hash = {
        let session = session_with(resume_dir.clone(), cat_path.clone(), tag_path.clone()).await;

        // Create the category first so add_torrent can resolve it.
        let _ = session
            .create_category("sonarr".into(), PathBuf::from("/tmp/sonarr-save"))
            .await;

        let params = SessionAddTorrentParams::magnet(magnet_uri)
            .with_category("sonarr")
            .with_tags(expected_tags.clone());
        let info_hash = session
            .add_torrent(params)
            .await
            .expect("add_torrent with category + tags should succeed");

        // Assert BOTH fields present on the first stats snapshot
        // (category is baked in at add-time in M170, tags in M171 A5).
        let stats = session.torrent_stats(info_hash).await.expect("stats");
        assert_eq!(
            stats.category,
            Some("sonarr".to_string()),
            "category must be present immediately"
        );
        assert_eq!(
            stats.tags, expected_tags,
            "tags must be present immediately"
        );

        // Force dirty flag + save resume.
        session.pause_torrent(info_hash).await.expect("pause");
        tokio::time::sleep(Duration::from_millis(50)).await;
        let saved = session.save_resume_state().await.expect("save resume");
        assert!(saved >= 1, "at least one torrent should have been saved");

        session.shutdown().await.expect("shutdown");
        info_hash
    };

    // Second session: assert both fields restored.
    {
        let session = session_with(resume_dir.clone(), cat_path.clone(), tag_path.clone()).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        let list = session.list_torrents().await.expect("list");
        assert!(
            list.contains(&info_hash),
            "torrent must be auto-restored on second session"
        );
        let stats = session.torrent_stats(info_hash).await.expect("stats");
        assert_eq!(
            stats.category,
            Some("sonarr".to_string()),
            "category must survive restart"
        );
        assert_eq!(stats.tags, expected_tags, "tags must survive restart");

        session.shutdown().await.expect("shutdown");
    }

    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&cat_path);
    let _ = std::fs::remove_file(&tag_path);
}

/// E0.12: deleting a tag must strip it from every torrent that had it.
#[tokio::test]
async fn delete_tag_clears_from_all_assigned_torrents() {
    let session = test_session().await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    // Create the tag.
    let (status, _) = post_form(&router, "/api/v2/torrents/createTags", &sid, "tags=x").await;
    assert_eq!(status, StatusCode::OK);

    // Add 5 torrents with distinct names (distinct info-hashes) and tag
    // each one with `x`. We don't need real content — 5 real torrent adds
    // walks the full happy path, including the add-time bake.
    let mut hashes = Vec::with_capacity(5);
    for i in 0..5_u8 {
        let bytes = make_torrent(&format!("e012-{i}.bin"));
        let params = SessionAddTorrentParams::bytes(bytes);
        let hash = add_and_wait(&session, params).await;
        hashes.push(hash);
    }

    // Bulk add tag `x` to all 5.
    let hash_blob = hashes.join("|");
    let body = format!("hashes={hash_blob}&tags=x");
    let (status, _) = post_form(&router, "/api/v2/torrents/addTags", &sid, &body).await;
    assert_eq!(status, StatusCode::OK);

    // Give the actor a beat to apply 5 set_tags commands.
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Confirm all 5 now carry the tag.
    for h in &hashes {
        let id = irontide::core::Id20::from_hex(h).expect("hex");
        let stats = session.torrent_stats(id).await.expect("stats");
        assert!(
            stats.tags.iter().any(|t| t == "x"),
            "torrent {h} should have tag 'x' before delete"
        );
    }

    // Delete tag `x`.
    let (status, _) = post_form(&router, "/api/v2/torrents/deleteTags", &sid, "tags=x").await;
    assert_eq!(status, StatusCode::OK);

    // Give the actor a beat to strip `x` from all 5.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Assert every torrent no longer has `x`.
    for h in &hashes {
        let id = irontide::core::Id20::from_hex(h).expect("hex");
        let stats = session.torrent_stats(id).await.expect("stats");
        assert!(
            !stats.tags.iter().any(|t| t == "x"),
            "torrent {h} must not have tag 'x' after delete, got {:?}",
            stats.tags
        );
    }
}

/// C2 verification: after tagging a torrent, its row in `/torrents/info`
/// has `tags` populated as a comma-joined string.
#[tokio::test]
async fn torrent_list_row_populates_tags_field() {
    let session = test_session().await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    // Create the tags first.
    let (status, _) = post_form(
        &router,
        "/api/v2/torrents/createTags",
        &sid,
        "tags=sonarr,kids",
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Add a torrent and tag it.
    let bytes = make_torrent("c2-list.bin");
    let params = SessionAddTorrentParams::bytes(bytes);
    let hash = add_and_wait(&session, params).await;

    let body = format!("hashes={hash}&tags=sonarr,kids");
    let (status, _) = post_form(&router, "/api/v2/torrents/addTags", &sid, &body).await;
    assert_eq!(status, StatusCode::OK);

    // Let set_tags settle.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // GET /torrents/info and find our row.
    let (status, body) = get(&router, "/api/v2/torrents/info", &sid).await;
    assert_eq!(status, StatusCode::OK);
    let v: Value = serde_json::from_slice(&body).expect("parse JSON");
    let rows = v.as_array().expect("info returns JSON array");
    let row = rows
        .iter()
        .find(|r| r["hash"].as_str() == Some(hash.as_str()))
        .expect("torrent row present in /torrents/info");

    // The `tags` field must be a comma-joined string. Order mirrors the
    // Vec<String> order — `add_tags_to_torrents` appends to existing tags
    // via `TorrentHandle::set_tags(union)` so we sort both sides to make
    // the assertion order-independent.
    let tags_str = row["tags"].as_str().expect("tags is a string");
    let mut got: Vec<&str> = tags_str.split(',').collect();
    got.sort_unstable();
    assert_eq!(
        got,
        vec!["kids", "sonarr"],
        "expected both tags joined by comma"
    );
}
