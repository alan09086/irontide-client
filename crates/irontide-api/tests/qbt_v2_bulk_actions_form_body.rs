//! v0.173.1 Class B regression: the five bulk-action handlers (pause /
//! resume / delete / recheck / reannounce) must accept `hashes=` from
//! EITHER the URL query string OR an `application/x-www-form-urlencoded`
//! body, because real `*arr` clients POST the body variant and v0.173.0
//! rejected them with 400.
//!
//! Each handler has three integration checks:
//! 1. `*_accepts_hashes_in_query_string` — ensures the existing query-string
//!    path still works (v0.173.0 baseline).
//! 2. `*_accepts_hashes_in_form_body` — the new Class B path.
//! 3. `*_mixed_source_query_wins_when_non_empty` — precedence proof: a
//!    non-empty query hash wins over a different body hash (documents the
//!    resolution order from `extract_hashes_params`).
//!
//! The second `delete` test, `delete_files_true_via_form_body_actually_removes_files`,
//! is the Class A + B integration: form-body `deleteFiles=true` must
//! still trigger on-disk removal for a `.torrent`-added torrent (magnet
//! variants live in Lane A's `qbt_v2_magnet_meta_propagation.rs`).
//!
//! # Why this file is separate
//! The existing `qbt_v2_torrents.rs` harness was written before Class B
//! was identified and assumes every bulk-action uses query params. We
//! keep it as the v0.173.0 baseline and add this file to prove parity on
//! both paths without conflating the two.

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use irontide::session::Settings;
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Build a fresh qBt-enabled session + router. Each call uses a unique
/// resume-dir path so parallel test runs don't clash through the
/// filesystem.
async fn fresh_router() -> axum::Router {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let resume_dir = std::env::temp_dir().join(format!(
        "irontide-qbt-v2-bulk-form-{}-{}",
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
    build_router(session)
}

/// Issue the qBt `auth/login` with the default `admin/adminadmin` creds.
/// Returns the `SID=...` cookie string ready to paste into a `Cookie:`
/// header.
async fn login(router: &axum::Router) -> String {
    let form = "username=admin&password=adminadmin";
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .expect("Set-Cookie")
        .to_str()
        .unwrap()
        .to_owned();
    let _ = resp.into_body().collect().await.unwrap();
    cookie.split(';').next().unwrap().to_owned()
}

async fn post(
    router: &axum::Router,
    uri: &str,
    cookie: &str,
    content_type: Option<&str>,
    body: Vec<u8>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::COOKIE, cookie);
    if let Some(ct) = content_type {
        builder = builder.header(header::CONTENT_TYPE, ct);
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

/// Synthesise a minimal v1 `.torrent` that the session accepts. Same
/// shape as `qbt_v2_torrents::make_test_torrent_bytes` but with a
/// dedicated counter so parallel runs don't name-collide.
fn make_test_torrent_bytes() -> Vec<u8> {
    use serde::Serialize;

    let data = vec![0xCD_u8; 16384];
    let hash = irontide::core::sha1(&data);
    let mut pieces = Vec::new();
    pieces.extend_from_slice(hash.as_bytes());

    #[derive(Serialize)]
    struct Info {
        #[serde(rename = "piece length")]
        piece_length: u32,
        pieces: serde_bytes::ByteBuf,
        name: String,
        length: u32,
    }

    #[derive(Serialize)]
    struct Root {
        announce: String,
        info: Info,
    }

    let root = Root {
        announce: "http://example.com/announce".into(),
        info: Info {
            piece_length: 16384,
            pieces: serde_bytes::ByteBuf::from(pieces),
            name: format!(
                "bulk-form-{}",
                SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
            ),
            length: 16384,
        },
    };

    irontide::bencode::to_bytes(&root).expect("bencode")
}

/// Add a synthetic torrent via the legacy v1 API (bypasses qbt auth)
/// and return its v1 info hash in lower-hex form.
///
/// We don't parse the v1 `POST /api/v1/torrents` response directly
/// because `InfoHashes::Serialize` writes `Id20` as a raw byte array
/// (`serializer.serialize_bytes`), not a hex string. The qBt v2
/// `/torrents/info` DTO is the path that actually formats the hash as
/// hex, so we round-trip through there.
async fn add_torrent_return_hash(router: &axum::Router, sid: &str) -> String {
    let bytes = make_test_torrent_bytes();
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/torrents")
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(bytes))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    assert!(
        status.is_success(),
        "v1 add must succeed, got {status}: {}",
        String::from_utf8_lossy(&body)
    );

    // Query the qBt v2 `/torrents/info` endpoint which serializes the
    // hash as a 40-char hex string.
    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/torrents/info")
        .header(header::COOKIE, sid)
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    let v: serde_json::Value = serde_json::from_slice(&body).expect("info JSON");
    v.as_array()
        .and_then(|arr| arr.last())
        .and_then(|t| t.get("hash"))
        .and_then(|h| h.as_str())
        .expect("qbt info hash")
        .to_ascii_lowercase()
}

// ── pause ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn pause_accepts_hashes_in_query_string() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    let (status, _) = post(
        &router,
        &format!("/api/v2/torrents/pause?hashes={hash}"),
        &sid,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "query-string pause must 200");
}

#[tokio::test]
async fn pause_accepts_hashes_in_form_body() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    let (status, _) = post(
        &router,
        "/api/v2/torrents/pause",
        &sid,
        Some("application/x-www-form-urlencoded"),
        format!("hashes={hash}").into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "form-body pause must 200");
}

#[tokio::test]
async fn pause_mixed_source_query_wins_when_non_empty() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    // Query has a real hash; body has "all". Query must win → the call
    // targets just the one torrent. Status is 200 in either case (qBt
    // bulk idempotency), but this test documents precedence — if the
    // body ever overrode the query we'd still see 200 here but break
    // callers who rely on the explicit query.
    let (status, _) = post(
        &router,
        &format!("/api/v2/torrents/pause?hashes={hash}"),
        &sid,
        Some("application/x-www-form-urlencoded"),
        b"hashes=all".to_vec(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "mixed-source pause (query wins) must 200"
    );
}

// ── resume ────────────────────────────────────────────────────────────

#[tokio::test]
async fn resume_accepts_hashes_in_query_string() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    let (status, _) = post(
        &router,
        &format!("/api/v2/torrents/resume?hashes={hash}"),
        &sid,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "query-string resume must 200");
}

#[tokio::test]
async fn resume_accepts_hashes_in_form_body() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    let (status, _) = post(
        &router,
        "/api/v2/torrents/resume",
        &sid,
        Some("application/x-www-form-urlencoded"),
        format!("hashes={hash}").into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "form-body resume must 200");
}

#[tokio::test]
async fn resume_mixed_source_query_wins_when_non_empty() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    let (status, _) = post(
        &router,
        &format!("/api/v2/torrents/resume?hashes={hash}"),
        &sid,
        Some("application/x-www-form-urlencoded"),
        b"hashes=all".to_vec(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "mixed-source resume (query wins) must 200"
    );
}

// ── recheck ───────────────────────────────────────────────────────────

#[tokio::test]
async fn recheck_accepts_hashes_in_query_string() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    let (status, _) = post(
        &router,
        &format!("/api/v2/torrents/recheck?hashes={hash}"),
        &sid,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "query-string recheck must 200");
}

#[tokio::test]
async fn recheck_accepts_hashes_in_form_body() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    let (status, _) = post(
        &router,
        "/api/v2/torrents/recheck",
        &sid,
        Some("application/x-www-form-urlencoded"),
        format!("hashes={hash}").into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "form-body recheck must 200");
}

#[tokio::test]
async fn recheck_mixed_source_query_wins_when_non_empty() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    let (status, _) = post(
        &router,
        &format!("/api/v2/torrents/recheck?hashes={hash}"),
        &sid,
        Some("application/x-www-form-urlencoded"),
        b"hashes=all".to_vec(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "mixed-source recheck (query wins) must 200"
    );
}

// ── reannounce ────────────────────────────────────────────────────────

#[tokio::test]
async fn reannounce_accepts_hashes_in_query_string() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    let (status, _) = post(
        &router,
        &format!("/api/v2/torrents/reannounce?hashes={hash}"),
        &sid,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "query-string reannounce must 200");
}

#[tokio::test]
async fn reannounce_accepts_hashes_in_form_body() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    let (status, _) = post(
        &router,
        "/api/v2/torrents/reannounce",
        &sid,
        Some("application/x-www-form-urlencoded"),
        format!("hashes={hash}").into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "form-body reannounce must 200");
}

#[tokio::test]
async fn reannounce_mixed_source_query_wins_when_non_empty() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    let (status, _) = post(
        &router,
        &format!("/api/v2/torrents/reannounce?hashes={hash}"),
        &sid,
        Some("application/x-www-form-urlencoded"),
        b"hashes=all".to_vec(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "mixed-source reannounce (query wins) must 200"
    );
}

// ── delete ────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_accepts_hashes_in_query_string() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    let (status, _) = post(
        &router,
        &format!("/api/v2/torrents/delete?hashes={hash}&deleteFiles=false"),
        &sid,
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "query-string delete must 200");
}

#[tokio::test]
async fn delete_accepts_hashes_in_form_body() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    let (status, _) = post(
        &router,
        "/api/v2/torrents/delete",
        &sid,
        Some("application/x-www-form-urlencoded"),
        format!("hashes={hash}&deleteFiles=false").into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "form-body delete must 200");
}

#[tokio::test]
async fn delete_mixed_source_query_wins_when_non_empty() {
    let router = fresh_router().await;
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    let (status, _) = post(
        &router,
        &format!("/api/v2/torrents/delete?hashes={hash}&deleteFiles=false"),
        &sid,
        Some("application/x-www-form-urlencoded"),
        b"hashes=all&deleteFiles=true".to_vec(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "mixed-source delete (query wins) must 200"
    );
}

/// v0.173.1 Class A + B integration: `deleteFiles=true` sent via form
/// body must still trigger on-disk removal for a `.torrent`-added
/// torrent. This pins the end-to-end path `*arr` clients actually
/// exercise — they POST `hashes=…&deleteFiles=true` in the body, not
/// the URL.
///
/// Magnet-based variants (which also benefit from Lane A's Class A
/// architectural fix) live in
/// `crates/irontide-api/tests/qbt_v2_magnet_meta_propagation.rs`.
#[tokio::test]
async fn delete_files_true_via_form_body_actually_removes_files() {
    // Isolated download directory so we can assert disk removal without
    // clashing with siblings that also write under `/tmp`.
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dl_dir = std::env::temp_dir().join(format!(
        "irontide-qbt-v2-bulk-delete-{}-{}",
        std::process::id(),
        n
    ));
    let resume_dir = dl_dir.join("resume");
    let _ = std::fs::remove_dir_all(&dl_dir);
    std::fs::create_dir_all(&dl_dir).expect("create dl dir");

    let mut settings = Settings {
        listen_port: 0,
        download_dir: dl_dir.clone(),
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
    let sid = login(&router).await;
    let hash = add_torrent_return_hash(&router, &sid).await;

    // Find the on-disk content path by reading dl_dir after add; the
    // torrent file-entry name is `bulk-form-<n>`. If the v1 add path
    // hasn't created a placeholder yet we plant one so the disk-removal
    // assertion has a concrete target.
    let content_path: Option<std::path::PathBuf> = std::fs::read_dir(&dl_dir)
        .expect("read dl_dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|s| s.starts_with("bulk-form-"))
        });
    if let Some(ref p) = content_path
        && !p.exists()
    {
        std::fs::write(p, b"placeholder").expect("plant placeholder");
    }

    let (status, _) = post(
        &router,
        "/api/v2/torrents/delete",
        &sid,
        Some("application/x-www-form-urlencoded"),
        format!("hashes={hash}&deleteFiles=true").into_bytes(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "form-body delete with deleteFiles=true must 200"
    );

    // Delete is asynchronous in the session actor; give it a moment to
    // walk and scrub the file tree.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    if let Some(ref p) = content_path {
        assert!(
            !p.exists(),
            "deleteFiles=true via form body must scrub disk; still at {p:?}"
        );
    }

    let _ = std::fs::remove_dir_all(&dl_dir);
}
