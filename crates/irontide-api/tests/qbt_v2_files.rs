#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M175: integration test code — fixtures use bounded sizes that fit narrower types"
)]

//! Integration tests for qBt v2 `GET /api/v2/torrents/files?hash=X` (M170 Lane B).
//!
//! Each test boots an isolated session, constructs a fixture torrent directly
//! in bencode, adds it via the in-process session handle, then queries the
//! HTTP endpoint through the test router.

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde::Serialize;
use serde_json::Value;
use tower::ServiceExt;

use irontide::session::{SessionHandle, Settings};
use irontide_api::routes::build_router;

// ── Session + router fixtures ────────────────────────────────────────

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

async fn test_session(qbt_enabled: bool) -> SessionHandle {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let resume_dir = std::env::temp_dir().join(format!(
        "irontide-qbt-v2-files-{}-{}",
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
    let _ = resp.into_body().collect().await.expect("drain body");
    cookie.split(';').next().expect("empty cookie").to_owned()
}

async fn get(router: &axum::Router, uri: &str, cookie: Option<&str>) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }
    let req = builder.body(Body::empty()).expect("build GET request");
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

// ── Torrent fixture builder ──────────────────────────────────────────
//
// Builds synthetic v1 .torrent bytes with explicit control over files,
// piece_length, and per-file BEP 47 attributes. Piece hashes are all zeros —
// we never try to verify data in these tests, we only care about the shape of
// the /files response.

/// One file spec for [`make_torrent_bytes`].
struct FileSpec {
    /// Path components, joined by forward slash. Must be non-empty.
    path: Vec<String>,
    /// Length in bytes.
    length: u64,
    /// BEP 47 attribute (`Some("p")` = pad file).
    attr: Option<String>,
}

impl FileSpec {
    fn new(path: &[&str], length: u64) -> Self {
        Self {
            path: path.iter().map(|s| (*s).to_owned()).collect(),
            length,
            attr: None,
        }
    }

    fn pad(length: u64) -> Self {
        Self {
            path: vec![".pad".into(), format!("{length}")],
            length,
            attr: Some("p".into()),
        }
    }
}

/// Build a bencode-serialised multi-file v1 .torrent.
///
/// `total` is summed across `files`; `piece_length` drives how many zeroed
/// piece hashes are concatenated into the info dict.
fn make_torrent_bytes(name: &str, piece_length: u64, files: &[FileSpec]) -> Vec<u8> {
    #[derive(Serialize)]
    struct RawFile<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        attr: Option<&'a str>,
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

    let total: u64 = files.iter().map(|f| f.length).sum();
    let num_pieces = total.div_ceil(piece_length).max(1);
    let pieces = vec![0u8; (num_pieces as usize).saturating_mul(20)];

    let raw_files: Vec<RawFile<'_>> = files
        .iter()
        .map(|f| RawFile {
            attr: f.attr.as_deref(),
            length: f.length,
            path: &f.path,
        })
        .collect();

    let t = Torrent {
        info: Info {
            files: raw_files,
            name,
            piece_length,
            pieces: &pieces,
        },
    };
    irontide::bencode::to_bytes(&t).expect("bencode serialize")
}

/// Build a single-file v1 .torrent. Single-file mode uses `info.length`,
/// not `info.files`.
fn make_single_file_torrent(name: &str, piece_length: u64, length: u64) -> Vec<u8> {
    #[derive(Serialize)]
    struct Info<'a> {
        length: u64,
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

    let num_pieces = length.div_ceil(piece_length).max(1);
    let pieces = vec![0u8; (num_pieces as usize).saturating_mul(20)];

    let t = Torrent {
        info: Info {
            length,
            name,
            piece_length,
            pieces: &pieces,
        },
    };
    irontide::bencode::to_bytes(&t).expect("bencode serialize")
}

// ── Tests ────────────────────────────────────────────────────────────

/// Single-file torrent: response is a 1-element array with `piece_range`
/// spanning the whole file.
#[tokio::test]
async fn files_single_file_one_entry() {
    let session = test_session(true).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    // 1 MiB single file, 256 KiB pieces → 4 pieces total.
    let bytes = make_single_file_torrent("bigfile.bin", 262_144, 1_048_576);
    let hashes = session
        .add_torrent_bytes(&bytes)
        .await
        .expect("add single-file torrent");
    let hash = hashes.v1.expect("v1 hash").to_hex();

    let (status, body) = get(
        &router,
        &format!("/api/v2/torrents/files?hash={hash}"),
        Some(&sid),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr: Value = serde_json::from_slice(&body).expect("parse JSON");
    let arr = arr.as_array().expect("array response");
    assert_eq!(arr.len(), 1, "single-file torrent must yield 1 entry");

    let entry = &arr[0];
    assert_eq!(entry["index"], 0);
    assert_eq!(entry["name"], "bigfile.bin");
    assert_eq!(entry["size"], 1_048_576);
    assert_eq!(entry["priority"], 1);
    assert_eq!(entry["availability"], 0.0);
    // piece_range covers pieces 0..=3.
    assert_eq!(entry["piece_range"][0], 0);
    assert_eq!(entry["piece_range"][1], 3);
    assert_eq!(entry["is_seed"], false);
}

/// Multi-file torrent: returns N entries, `piece_range` values increase
/// monotonically across the file list.
#[tokio::test]
async fn files_multi_file_piece_range_monotonic() {
    let session = test_session(true).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    // Four files of 512 KiB each, 256 KiB pieces → 8 pieces total.
    // File 0: bytes 0..524288      → pieces (0, 1)
    // File 1: bytes 524288..1048576→ pieces (2, 3)
    // File 2: bytes 1048576..1572864 → pieces (4, 5)
    // File 3: bytes 1572864..2097152 → pieces (6, 7)
    let bytes = make_torrent_bytes(
        "four-file-set",
        262_144,
        &[
            FileSpec::new(&["a.mkv"], 524_288),
            FileSpec::new(&["b.mkv"], 524_288),
            FileSpec::new(&["c.mkv"], 524_288),
            FileSpec::new(&["d.mkv"], 524_288),
        ],
    );
    let hashes = session
        .add_torrent_bytes(&bytes)
        .await
        .expect("add multi-file torrent");
    let hash = hashes.v1.expect("v1 hash").to_hex();

    let (status, body) = get(
        &router,
        &format!("/api/v2/torrents/files?hash={hash}"),
        Some(&sid),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr: Value = serde_json::from_slice(&body).expect("parse JSON");
    let arr = arr.as_array().expect("array response");
    assert_eq!(arr.len(), 4);

    let mut last_first: i64 = -1;
    for (i, entry) in arr.iter().enumerate() {
        assert_eq!(entry["index"], i);
        let first = entry["piece_range"][0].as_i64().expect("first piece i64");
        let last = entry["piece_range"][1].as_i64().expect("last piece i64");
        assert!(first >= last_first, "piece_range[0] not monotonic at #{i}");
        assert!(last >= first, "piece_range inverted at #{i}");
        last_first = first;
    }

    // Specific boundary assertions for this fixture.
    assert_eq!(arr[0]["piece_range"], serde_json::json!([0, 1]));
    assert_eq!(arr[3]["piece_range"], serde_json::json!([6, 7]));
}

/// Progress starts at 0.0 for freshly-added torrents that haven't pulled any
/// bytes yet. For non-empty files the value must be 0.0; zero-length files
/// collapse to 1.0 (qBt parity — a 0-byte file is trivially "complete").
#[tokio::test]
async fn files_progress_zero_on_fresh_torrent() {
    let session = test_session(true).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let bytes = make_torrent_bytes(
        "progress-test",
        65_536,
        &[
            FileSpec::new(&["data.bin"], 131_072),
            FileSpec::new(&["empty.bin"], 0),
        ],
    );
    let hashes = session
        .add_torrent_bytes(&bytes)
        .await
        .expect("add torrent");
    let hash = hashes.v1.expect("v1 hash").to_hex();

    let (status, body) = get(
        &router,
        &format!("/api/v2/torrents/files?hash={hash}"),
        Some(&sid),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr: Value = serde_json::from_slice(&body).expect("parse JSON");
    let arr = arr.as_array().expect("array response");
    assert_eq!(arr.len(), 2);

    let data_progress = arr[0]["progress"].as_f64().expect("progress f64");
    let empty_progress = arr[1]["progress"].as_f64().expect("progress f64");
    assert!(
        (0.0..=1.0).contains(&data_progress),
        "progress must be in [0,1], got {data_progress}"
    );
    assert!(
        (data_progress - 0.0).abs() < f64::EPSILON,
        "fresh torrent progress on non-empty file must be 0.0, got {data_progress}"
    );
    assert!(
        (empty_progress - 1.0).abs() < f64::EPSILON,
        "zero-length file progress must be 1.0, got {empty_progress}"
    );
    assert_eq!(
        arr[1]["is_seed"], true,
        "zero-length file must report is_seed = true"
    );
    assert_eq!(
        arr[0]["is_seed"], false,
        "non-empty unfinished file must report is_seed = false"
    );
}

/// `piece_range` values match the arithmetic when pieces span file boundaries.
#[tokio::test]
async fn files_piece_range_exact_tuples() {
    let session = test_session(true).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    // Files [400, 300, 200] bytes, piece_length = 256.
    // Total = 900, num_pieces = 4.
    // File 0: bytes 0..400  → pieces (0, 1)
    // File 1: bytes 400..700 → pieces (1, 2)
    // File 2: bytes 700..900 → pieces (2, 3)
    let bytes = make_torrent_bytes(
        "piece-range-test",
        256,
        &[
            FileSpec::new(&["alpha.bin"], 400),
            FileSpec::new(&["beta.bin"], 300),
            FileSpec::new(&["gamma.bin"], 200),
        ],
    );
    let hashes = session
        .add_torrent_bytes(&bytes)
        .await
        .expect("add torrent");
    let hash = hashes.v1.expect("v1 hash").to_hex();

    let (status, body) = get(
        &router,
        &format!("/api/v2/torrents/files?hash={hash}"),
        Some(&sid),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr: Value = serde_json::from_slice(&body).expect("parse JSON");
    let arr = arr.as_array().expect("array response");
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0]["piece_range"], serde_json::json!([0, 1]));
    assert_eq!(arr[1]["piece_range"], serde_json::json!([1, 2]));
    assert_eq!(arr[2]["piece_range"], serde_json::json!([2, 3]));
}

/// An unknown hash returns 404.
#[tokio::test]
async fn files_unknown_hash_returns_404() {
    let session = test_session(true).await;
    let router = build_router(session);
    let sid = login(&router).await;

    let (status, _body) = get(
        &router,
        // Syntactically valid 40-char hex string with no matching torrent.
        "/api/v2/torrents/files?hash=0000000000000000000000000000000000000000",
        Some(&sid),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A magnet link that has not yet resolved metadata returns 404 — the hash
/// exists in the session, but `has_metadata == false`. *arr treats this as
/// "try again later" and retries.
#[tokio::test]
async fn files_magnet_pre_metadata_returns_404() {
    let session = test_session(true).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    // Private /8 address that will never answer — metadata never arrives
    // during the lifetime of this test. DHT/LSD/UPnP/NAT are all disabled
    // in the test session so we're not accidentally talking to the real
    // network either.
    let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=nometa";
    let hashes = session
        .add_magnet_uri(magnet)
        .await
        .expect("add magnet uri");
    let hash = hashes.v1.expect("v1 hash").to_hex();

    // No wait: we want the race-before-metadata case.
    let (status, _body) = get(
        &router,
        &format!("/api/v2/torrents/files?hash={hash}"),
        Some(&sid),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "magnet pre-metadata must 404"
    );
}

/// Non-ASCII file names are preserved verbatim in the JSON `name` field.
/// UTF-8 percent-encoding would silently break *arr's import path match.
#[tokio::test]
async fn files_unicode_path_name_roundtrips() {
    let session = test_session(true).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    let bytes = make_torrent_bytes(
        "unicode-test",
        32_768,
        &[FileSpec::new(
            // "Sönarr Tëst — S01E01.mkv" contains non-ASCII Latin-1 letters
            // (ö, ë), a Chinese character (测试), a Cyrillic letter (Д),
            // and an em dash — a healthy mix of multi-byte UTF-8 scalars.
            &["Sönarr Tëst 测试 Д — S01E01.mkv"],
            65_536,
        )],
    );
    let hashes = session
        .add_torrent_bytes(&bytes)
        .await
        .expect("add unicode torrent");
    let hash = hashes.v1.expect("v1 hash").to_hex();

    let (status, body) = get(
        &router,
        &format!("/api/v2/torrents/files?hash={hash}"),
        Some(&sid),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr: Value = serde_json::from_slice(&body).expect("parse JSON");
    let arr = arr.as_array().expect("array response");
    assert_eq!(arr.len(), 1);
    let name = arr[0]["name"].as_str().expect("name is string");
    assert_eq!(name, "Sönarr Tëst 测试 Д — S01E01.mkv");
}

/// BEP 47 pad files (attr == "p") are filtered out of the listing — *arr and
/// qBt treat padding as invisible. A torrent with a real file + a pad file
/// yields a 1-element array, not a 2-element array, and the real file keeps
/// index 0.
#[tokio::test]
async fn files_bep47_pad_files_filtered() {
    let session = test_session(true).await;
    let router = build_router(session.clone());
    let sid = login(&router).await;

    // Real file (400 bytes) + pad file (112 bytes, aligning to the next
    // 256-byte piece boundary) + real file (200 bytes).
    // Pad file has attr = "p".
    let bytes = make_torrent_bytes(
        "pad-test",
        256,
        &[
            FileSpec::new(&["alpha.bin"], 400),
            FileSpec::pad(112),
            FileSpec::new(&["beta.bin"], 200),
        ],
    );
    let hashes = session
        .add_torrent_bytes(&bytes)
        .await
        .expect("add pad-file torrent");
    let hash = hashes.v1.expect("v1 hash").to_hex();

    let (status, body) = get(
        &router,
        &format!("/api/v2/torrents/files?hash={hash}"),
        Some(&sid),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr: Value = serde_json::from_slice(&body).expect("parse JSON");
    let arr = arr.as_array().expect("array response");
    assert_eq!(
        arr.len(),
        2,
        "pad files must be filtered — expected 2 non-pad entries, got: {arr:?}"
    );
    assert_eq!(arr[0]["index"], 0);
    assert_eq!(arr[1]["index"], 1);
    assert_eq!(arr[0]["name"], "alpha.bin");
    assert_eq!(arr[1]["name"], "beta.bin");
}
