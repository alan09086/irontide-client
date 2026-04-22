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
