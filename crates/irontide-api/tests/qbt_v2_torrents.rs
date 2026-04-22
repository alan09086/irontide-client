//! Integration tests for qBt v2 torrent endpoints (M168 Tasks 10-14).

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use irontide::session::Settings;
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

async fn enabled_router_with(customize: impl FnOnce(&mut Settings)) -> (axum::Router, String) {
    let username: String;
    let session = {
        let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        let resume_dir =
            std::env::temp_dir().join(format!("irontide-qbt-v2-tor-{}-{}", std::process::id(), n));
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
        customize(&mut settings);
        username = settings.qbt_compat.username.clone();
        irontide::ClientBuilder::from_settings(settings)
            .start()
            .await
            .expect("failed to start test session")
    };
    let router = build_router(session);
    // M172a: default password_hash matches "adminadmin".
    let form = format!("username={username}&password=adminadmin");
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
    let sid = cookie.split(';').next().unwrap().to_owned();
    (router, sid)
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
    content_type: Option<&str>,
    body: Vec<u8>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method("POST").uri(uri);
    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }
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

/// Synthesise a minimal v1 .torrent that the session will accept.
fn make_test_torrent_bytes() -> Vec<u8> {
    use serde::Serialize;

    let data = vec![0xAB; 16384];
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
                "qbt-test-{}",
                SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
            ),
            length: 16384,
        },
    };

    irontide::bencode::to_bytes(&root).expect("bencode")
}

// ── Task 10: torrents/info ────────────────────────────────────────────

#[tokio::test]
async fn torrents_info_returns_json_array() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, body) = get(&router, "/api/v2/torrents/info", Some(&sid)).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.is_array());
}

#[tokio::test]
async fn torrents_info_empty_when_no_torrents() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (_, body) = get(&router, "/api/v2/torrents/info", Some(&sid)).await;
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn torrents_info_includes_all_torrents_by_default() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    // Add two torrents via the legacy v1 API (simplest path).
    let bytes = make_test_torrent_bytes();
    let bytes2 = make_test_torrent_bytes();
    for b in [bytes, bytes2] {
        let (st, _) = post(
            &router,
            "/api/v1/torrents",
            None, // v1 doesn't require qbt auth
            Some("application/octet-stream"),
            b,
        )
        .await;
        // D1.1 (M173 Lane C): harness glue — the v1 add path is a
        // pre-test fixture and should always succeed with a strict
        // 201 Created (per the v1 spec; the v2 surface translates to
        // 200 OK on its own add handler). The prior OK-or-client-error
        // assertion was masking regressions when v1 responses changed.
        assert_eq!(
            st,
            StatusCode::CREATED,
            "v1 add must return 201 Created; got {st}"
        );
    }
    let (_, body) = get(&router, "/api/v2/torrents/info", Some(&sid)).await;
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.as_array().unwrap().len() >= 1, "expected at least 1");
}

#[tokio::test]
async fn torrents_info_filter_all() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, body) = get(&router, "/api/v2/torrents/info?filter=all", Some(&sid)).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.is_array());
}

#[tokio::test]
async fn torrents_info_filter_downloading() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = get(
        &router,
        "/api/v2/torrents/info?filter=downloading",
        Some(&sid),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn torrents_info_filter_completed() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = get(
        &router,
        "/api/v2/torrents/info?filter=completed",
        Some(&sid),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn torrents_info_hashes_param_subsets_list() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    // No torrents → hashes= filter produces empty array.
    let (status, body) = get(
        &router,
        "/api/v2/torrents/info?hashes=0000000000000000000000000000000000000000",
        Some(&sid),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn torrents_info_sort_by_name_reverse() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = get(
        &router,
        "/api/v2/torrents/info?sort=name&reverse=true",
        Some(&sid),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

// ── Task 11: torrents/properties ──────────────────────────────────────

#[tokio::test]
async fn torrents_properties_with_valid_hash_returns_superset_fields() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let bytes = make_test_torrent_bytes();
    let (_, _) = post(
        &router,
        "/api/v1/torrents",
        None,
        Some("application/octet-stream"),
        bytes,
    )
    .await;

    // Fetch the list to pick up a real hash.
    let (_, list_body) = get(&router, "/api/v2/torrents/info", Some(&sid)).await;
    let arr: serde_json::Value = serde_json::from_slice(&list_body).unwrap();
    let hash = arr
        .as_array()
        .unwrap()
        .get(0)
        .and_then(|t| t.get("hash"))
        .and_then(|h| h.as_str())
        .unwrap_or("")
        .to_owned();
    if hash.is_empty() {
        // If v1 /torrents returned an error, skip this assertion; flagged in
        // end-to-end tests below.
        return;
    }

    let uri = format!("/api/v2/torrents/properties?hash={hash}");
    let (status, body) = get(&router, &uri, Some(&sid)).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    for key in [
        "save_path",
        "piece_size",
        "total_wasted",
        "total_uploaded",
        "total_downloaded",
        "up_limit",
        "dl_limit",
        "time_elapsed",
        "seeding_time",
        "nb_connections",
        "share_ratio",
        "addition_date",
        "peers",
        "seeds",
        "pieces_have",
        "pieces_num",
        "total_size",
    ] {
        assert!(v.get(key).is_some(), "missing key {key}");
    }
}

#[tokio::test]
async fn torrents_properties_with_unknown_hash_returns_404() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = get(
        &router,
        "/api/v2/torrents/properties?hash=1111111111111111111111111111111111111111",
        Some(&sid),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn torrents_properties_with_invalid_hex_returns_400() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = get(
        &router,
        "/api/v2/torrents/properties?hash=GARBAGE",
        Some(&sid),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn torrents_properties_requires_sid() {
    let (router, _) = enabled_router_with(|_| {}).await;
    let (status, _) = get(
        &router,
        "/api/v2/torrents/properties?hash=0000000000000000000000000000000000000000",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ── Task 12: torrents/add ─────────────────────────────────────────────

#[tokio::test]
async fn torrents_add_single_magnet_creates_torrent() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=BigBuckBunny";
    let body = format!("urls={}", urlencode(magnet));
    let (status, _) = post(
        &router,
        "/api/v2/torrents/add",
        Some(&sid),
        Some("application/x-www-form-urlencoded"),
        body.into_bytes(),
    )
    .await;
    // D1.2 (M173 Lane C): magnet adds always land synchronously. The
    // prior OK-or-client-error permissive assertion was masking
    // regressions in the add handler — tighten to a strict 200.
    assert_eq!(status, StatusCode::OK, "magnet add must return 200");
    // D1.2: registry-state assertion — the torrent must be visible on
    // `GET /torrents/info` by its lower-hex info hash (pre-metadata
    // magnets still show up, they just have empty names until resolved).
    let expected_hash = "dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c";
    let (st2, body2) = get(&router, "/api/v2/torrents/info", Some(&sid)).await;
    assert_eq!(st2, StatusCode::OK, "info lookup after add");
    let rows: serde_json::Value = serde_json::from_slice(&body2).expect("info JSON");
    let arr = rows.as_array().expect("info is array");
    let hashes: Vec<&str> = arr
        .iter()
        .filter_map(|row| row.get("hash").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        hashes.iter().any(|h| h.eq_ignore_ascii_case(expected_hash)),
        "torrent hash {expected_hash} missing from /torrents/info: {hashes:?}"
    );
}

#[tokio::test]
async fn torrents_add_multiple_magnets_newline_separated() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let m1_hash = "1111111111111111111111111111111111111111";
    let m2_hash = "2222222222222222222222222222222222222222";
    let m1 = format!("magnet:?xt=urn:btih:{m1_hash}");
    let m2 = format!("magnet:?xt=urn:btih:{m2_hash}");
    let body = format!("urls={}", urlencode(&format!("{m1}\n{m2}")));
    let (status, _) = post(
        &router,
        "/api/v2/torrents/add",
        Some(&sid),
        Some("application/x-www-form-urlencoded"),
        body.into_bytes(),
    )
    .await;
    // D1.3 (M173 Lane C): tighten from OK-or-client-error to 200.
    assert_eq!(status, StatusCode::OK, "multi-magnet add must return 200");
    // D1.3: both hashes must appear on the listing.
    let (st2, body2) = get(&router, "/api/v2/torrents/info", Some(&sid)).await;
    assert_eq!(st2, StatusCode::OK, "info lookup after multi-add");
    let rows: serde_json::Value = serde_json::from_slice(&body2).expect("info JSON");
    let arr = rows.as_array().expect("info is array");
    let hashes: Vec<&str> = arr
        .iter()
        .filter_map(|row| row.get("hash").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        hashes.iter().any(|h| h.eq_ignore_ascii_case(m1_hash)),
        "torrent 1 hash {m1_hash} missing: {hashes:?}"
    );
    assert!(
        hashes.iter().any(|h| h.eq_ignore_ascii_case(m2_hash)),
        "torrent 2 hash {m2_hash} missing: {hashes:?}"
    );
}

#[tokio::test]
async fn torrents_add_torrent_file_multipart_creates_torrent() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let torrent = make_test_torrent_bytes();
    let boundary = "----TestBoundary0xABCD";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"torrents\"; filename=\"test.torrent\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: application/x-bittorrent\r\n\r\n");
    body.extend_from_slice(&torrent);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let ct = format!("multipart/form-data; boundary={boundary}");
    let (status, _) = post(&router, "/api/v2/torrents/add", Some(&sid), Some(&ct), body).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn torrents_add_with_savepath_honors_per_torrent_dir() {
    // D1.4 (M173 Lane C): `savepath=` is wired through
    // `SessionAddTorrentParams::with_download_dir` — the stale FIXME M170
    // claim ("accept but don't wire") is out of date. Assert that the
    // override lands on `TorrentStats.save_path` via the properties
    // endpoint. The path need not exist on disk: it only gates writes,
    // not the add-torrent command.
    let (router, sid) = enabled_router_with(|_| {}).await;
    let hash_hex = "3333333333333333333333333333333333333333";
    let override_path = "/tmp/irontide-m173-lane-c-savepath";
    let body = format!(
        "urls={}&savepath={}",
        urlencode(&format!("magnet:?xt=urn:btih:{hash_hex}")),
        urlencode(override_path)
    );
    let (status, _) = post(
        &router,
        "/api/v2/torrents/add",
        Some(&sid),
        Some("application/x-www-form-urlencoded"),
        body.into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "magnet add with savepath");

    let (st2, body2) = get(
        &router,
        &format!("/api/v2/torrents/properties?hash={hash_hex}"),
        Some(&sid),
    )
    .await;
    assert_eq!(st2, StatusCode::OK, "properties lookup after savepath add");
    let props: serde_json::Value = serde_json::from_slice(&body2).expect("properties JSON");
    let save_path = props
        .get("save_path")
        .and_then(serde_json::Value::as_str)
        .expect("save_path in properties");
    assert_eq!(
        save_path, override_path,
        "TorrentStats.save_path must reflect the savepath= form override"
    );
}

#[tokio::test]
async fn torrents_add_with_paused_starts_paused() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let hash_hex = "4444444444444444444444444444444444444444";
    let body = format!(
        "urls={}&paused=true",
        urlencode(&format!("magnet:?xt=urn:btih:{hash_hex}"))
    );
    let (status, _) = post(
        &router,
        "/api/v2/torrents/add",
        Some(&sid),
        Some("application/x-www-form-urlencoded"),
        body.into_bytes(),
    )
    .await;
    // D1.5 (M173 Lane C): tighten from OK-or-client-error to strict 200
    // and assert the torrent visibly started in a paused state via the
    // qBt state string on `/torrents/info`. Retry briefly — the torrent
    // actor may need a moment to propagate the pause flag up to the
    // first stats snapshot.
    assert_eq!(status, StatusCode::OK, "paused add must return 200");
    let mut saw_paused_state = false;
    for _ in 0..50 {
        let (st2, body2) = get(&router, "/api/v2/torrents/info", Some(&sid)).await;
        assert_eq!(st2, StatusCode::OK, "info lookup after paused add");
        let rows: serde_json::Value = serde_json::from_slice(&body2).expect("info JSON");
        let arr = rows.as_array().expect("info is array");
        let state_for_hash = arr
            .iter()
            .find(|row| {
                row.get("hash")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|h| h.eq_ignore_ascii_case(hash_hex))
            })
            .and_then(|row| row.get("state").and_then(serde_json::Value::as_str))
            .map(str::to_owned);
        if let Some(state) = state_for_hash
            && state.starts_with("paused")
        {
            saw_paused_state = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(
        saw_paused_state,
        "paused=true should surface as qBt state starting with 'paused'"
    );
}

#[tokio::test]
async fn torrents_add_rejects_both_urls_and_file_empty_returns_400() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    // Empty urlencoded body (no urls field).
    let (status, _) = post(
        &router,
        "/api/v2/torrents/add",
        Some(&sid),
        Some("application/x-www-form-urlencoded"),
        b"".to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ── Task 13: torrent actions ──────────────────────────────────────────

#[tokio::test]
async fn torrents_pause_with_hashes_all_returns_ok() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, body) = post(
        &router,
        "/api/v2/torrents/pause?hashes=all",
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8(body).unwrap(), "Ok.");
}

#[tokio::test]
async fn torrents_pause_with_explicit_hash_returns_ok() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = post(
        &router,
        "/api/v2/torrents/pause?hashes=0000000000000000000000000000000000000000",
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn torrents_resume_with_hashes_all_returns_ok() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = post(
        &router,
        "/api/v2/torrents/resume?hashes=all",
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn torrents_resume_with_explicit_hash_returns_ok() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = post(
        &router,
        "/api/v2/torrents/resume?hashes=ffffffffffffffffffffffffffffffffffffffff",
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn torrents_recheck_with_all_returns_ok() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = post(
        &router,
        "/api/v2/torrents/recheck?hashes=all",
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn torrents_recheck_with_single_hash_returns_ok() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = post(
        &router,
        "/api/v2/torrents/recheck?hashes=dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c",
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn torrents_reannounce_with_all_returns_ok() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = post(
        &router,
        "/api/v2/torrents/reannounce?hashes=all",
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn torrents_reannounce_with_single_hash_returns_ok() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = post(
        &router,
        "/api/v2/torrents/reannounce?hashes=dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c",
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn torrents_delete_with_deletefiles_true_returns_ok() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = post(
        &router,
        "/api/v2/torrents/delete?hashes=dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&deleteFiles=true",
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn torrents_delete_with_deletefiles_false_returns_ok() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = post(
        &router,
        "/api/v2/torrents/delete?hashes=dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&deleteFiles=false",
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn torrents_delete_with_hashes_all_returns_ok() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = post(
        &router,
        "/api/v2/torrents/delete?hashes=all",
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn torrents_delete_with_unknown_hash_silently_skipped() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, _) = post(
        &router,
        "/api/v2/torrents/delete?hashes=ffffffffffffffffffffffffffffffffffffffff&deleteFiles=false",
        Some(&sid),
        None,
        Vec::new(),
    )
    .await;
    // Real qBt returns 200 even for unknown hashes — don't leak existence.
    assert_eq!(status, StatusCode::OK);
}

// ── Task 14: transferInfo ─────────────────────────────────────────────

#[tokio::test]
async fn transfer_info_json_shape() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (status, body) = get(&router, "/api/v2/transferInfo", Some(&sid)).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    for key in [
        "dl_info_speed",
        "dl_info_data",
        "up_info_speed",
        "up_info_data",
        "connection_status",
        "dht_nodes",
    ] {
        assert!(v.get(key).is_some(), "missing key {key}");
    }
}

#[tokio::test]
async fn transfer_info_speeds_reflect_session_stats() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (_, body) = get(&router, "/api/v2/transferInfo", Some(&sid)).await;
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // With no torrents, both speeds must be 0.
    assert_eq!(v.get("dl_info_speed").and_then(|n| n.as_u64()), Some(0));
    assert_eq!(v.get("up_info_speed").and_then(|n| n.as_u64()), Some(0));
}

#[tokio::test]
async fn transfer_info_dht_nodes_count_from_session() {
    let (router, sid) = enabled_router_with(|_| {}).await;
    let (_, body) = get(&router, "/api/v2/transferInfo", Some(&sid)).await;
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // DHT is disabled in test session; dht_nodes should be 0 (and a u64).
    let n = v.get("dht_nodes").and_then(|n| n.as_u64()).unwrap();
    assert_eq!(n, 0);
}

// ── Helpers ──────────────────────────────────────────────────────────

fn urlencode(s: &str) -> String {
    // Minimal RFC 3986 percent-encoding for testing purposes.
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
