//! Common test helpers for qBt v2 integration tests.
//!
//! Integration test files under `tests/` are compiled as separate binaries,
//! so any helper shared between them must live in a sub-module like this
//! one (referenced via `mod common;` at the top of each test file). The
//! test runner does NOT compile `common/mod.rs` as a standalone test
//! binary — that would produce a spurious "test has no tests" warning.
//!
//! Each helper is marked `#[allow(dead_code)]` because a given test binary
//! usually uses only a subset of them; without the attribute rustc would
//! warn about unused helpers per-binary.

use std::time::Duration;

use irontide::session::{SessionAddTorrentParams, SessionHandle};

/// Add a torrent via the `SessionHandle` and poll until its stats are
/// queryable AND metadata has been resolved — i.e. piece count, file
/// layout, and save-path are all addressable on `TorrentStats`.
///
/// Returns the lower-hex v1 info hash on success. Panics if the torrent
/// never reaches a metadata-resolved state within the poll window (50 ×
/// 20 ms = 1 s), which is the generous upper bound for a `.torrent`
/// byte-source add on a non-loaded test runner.
///
/// The stricter `has_metadata` gate (vs. the laxer `torrent_stats().is_ok()`
/// that prior copies of this helper used in some test files) matters
/// because many downstream assertions — trackers list, webseeds list,
/// piece states — depend on metadata being present. Without the gate,
/// those assertions race the metadata-resolver and flake.
#[allow(dead_code)]
pub async fn add_and_wait(
    session: &SessionHandle,
    params: SessionAddTorrentParams,
) -> String {
    let hash = session.add_torrent(params).await.expect("add torrent");
    for _ in 0..50 {
        if let Ok(stats) = session.torrent_stats(hash).await
            && stats.has_metadata
        {
            return hash.to_hex();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("torrent metadata never resolved within the poll window");
}

// ─── v0.173.2 additions for A9 magnet-injection regression test ──────────

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use irontide_core::Id20;
use irontide_session::Settings;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(serde::Serialize)]
struct SynthInfoDict<'a> {
    length: u64,
    name: &'a str,
    #[serde(rename = "piece length")]
    piece_length: u64,
    pieces: serde_bytes::ByteBuf,
}

/// Build a single-file v1 info-dict bencode payload for `name` of
/// `length_bytes`. `pieces` is filled with SHA-1 of an all-zero piece,
/// repeated for the number of pieces. Sufficient for tests that never
/// read actual data — only assert metadata-dependent endpoints work
/// post-resolution.
///
/// **Caller invariant:** Within a single session, do not call with
/// identical `(name, length_bytes)` — the synth info hash will collide
/// and `add_torrent` will return `Error::DuplicateTorrent`. The
/// `SESSION_COUNTER`-based per-test isolation in `make_test_settings`
/// avoids this across tests.
#[allow(dead_code)]
fn build_synth_info_bytes(name: &str, length_bytes: u64) -> Vec<u8> {
    let piece_length: u64 = 16_384;
    let num_pieces = length_bytes.div_ceil(piece_length);
    let zero_piece_hash = irontide_core::sha1(&vec![0u8; piece_length as usize]);
    let mut pieces = Vec::with_capacity(20 * num_pieces as usize);
    for _ in 0..num_pieces {
        pieces.extend_from_slice(zero_piece_hash.as_bytes());
    }
    let info = SynthInfoDict {
        length: length_bytes,
        name,
        piece_length,
        pieces: serde_bytes::ByteBuf::from(pieces),
    };
    irontide_bencode::to_bytes(&info).expect("bencode synth info dict")
}

#[allow(dead_code)]
fn synth_info_hash(info_bytes: &[u8]) -> Id20 {
    irontide_core::sha1(info_bytes)
}

/// Build `Settings` with cleanup-safe per-test temp dirs. Caller can
/// mutate the returned `Settings` (e.g., `settings.qbt_compat.enabled = true`).
/// Returns the download dir so tests can assert file presence.
#[allow(dead_code)]
pub fn make_test_settings(suffix: &str) -> (Settings, PathBuf) {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let resume_dir = std::env::temp_dir().join(format!("irontide-{suffix}-resume-{pid}-{n}"));
    let dl_dir = std::env::temp_dir().join(format!("irontide-{suffix}-dl-{pid}-{n}"));
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_dir_all(&dl_dir);
    std::fs::create_dir_all(&dl_dir).expect("mkdir dl_dir");

    let settings = Settings {
        listen_port: 0,
        download_dir: dl_dir.clone(),
        enable_dht: false,
        enable_pex: false,
        enable_lsd: false,
        enable_upnp: false,
        enable_natpmp: false,
        resume_data_dir: Some(resume_dir),
        save_resume_interval_secs: 0,
        ..Settings::default()
    };
    (settings, dl_dir)
}

/// (A9 helper.) Add a magnet whose info hash matches the synthesised info
/// dict, inject the dict synchronously, return the hash. Assumes `name`
/// is URL-safe (alphanumeric + dot + hyphen + underscore — no spaces, `&`,
/// `?`). Tests pass `archlinux-...iso`-style names.
#[allow(dead_code)]
pub async fn inject_magnet_and_resolve_meta(
    session: &SessionHandle,
    name: &str,
    length_bytes: u64,
) -> Id20 {
    let info_bytes = build_synth_info_bytes(name, length_bytes);
    let hash = synth_info_hash(&info_bytes);

    let magnet = format!("magnet:?xt=urn:btih:{}&dn={}", hash.to_hex(), name);
    let params = SessionAddTorrentParams::magnet(magnet);
    let added = session.add_torrent(params).await.expect("add magnet");
    assert_eq!(added, hash, "session-assigned hash must match synth info hash");

    // Synchronous round-trip via test-util feature: returns only when
    // the actor has processed the inject.
    session
        .debug_inject_metadata(hash, info_bytes)
        .await
        .expect("debug_inject_metadata");

    // Belt-and-braces sanity check (the synchronous contract makes this
    // immediate; if it fails, debug_inject_metadata is broken).
    assert!(
        matches!(session.torrent_file(hash).await, Ok(Some(_))),
        "post-inject metadata must be Some — debug_inject_metadata contract violated"
    );
    hash
}
