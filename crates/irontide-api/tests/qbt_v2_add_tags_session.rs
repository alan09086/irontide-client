//! M171 A5+A6: `AddTorrentParams::with_tags` — add-time tag baking and
//! per-torrent resume-data round-trip.
//!
//! A5 pins the contract that tags supplied on an add call are written
//! into the `TorrentConfig` BEFORE the `TorrentActor` is spawned. The
//! plan explicitly rejects the post-add `tokio::spawn(handle.set_tags(...))`
//! shape used for `category` (M170), because it leaves the first
//! `stats()` snapshot tag-less and forces callers to poll. That first
//! block of tests guards the add-time bake — no sleep, no retry.
//!
//! A6 adds a single round-trip test that drives save → shutdown →
//! restart → auto-restore against the same isolated `resume_data_dir`
//! and asserts `stats.tags` on the restored torrent matches what was
//! set on the original add. Together A5 + A6 cover the full tag
//! lifecycle that feeds the qBt v2 `*arr` surface.
//!
//! These are pure `SessionHandle` tests — no HTTP / axum harness
//! needed. The isolation helpers mirror `qbt_v2_add_source_regression.rs`
//! and the M170 Lane D regression suite.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

// Use the M170 params struct (re-exported from the session crate under
// an aliased name because the facade owns its own `AddTorrentParams`).
use irontide::session::{SessionAddTorrentParams as AddTorrentParams, SessionHandle, Settings};

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fresh_paths(tag: &str) -> (PathBuf, PathBuf) {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let resume_dir =
        std::env::temp_dir().join(format!("irontide-m171-add-tags-{tag}-resume-{pid}-{n}"));
    let reg_path =
        std::env::temp_dir().join(format!("irontide-m171-add-tags-{tag}-{pid}-{n}.toml"));
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&reg_path);
    (resume_dir, reg_path)
}

async fn test_session(tag: &str) -> SessionHandle {
    let (resume_dir, reg_path) = fresh_paths(tag);
    session_with(resume_dir, reg_path).await
}

/// Build a session against an explicit `resume_data_dir` / `category_registry_path`.
///
/// A6 needs to construct two back-to-back sessions pointing at the same
/// resume directory, so the path parameters cannot be freshly allocated
/// inside this helper.
async fn session_with(resume_dir: PathBuf, reg_path: PathBuf) -> SessionHandle {
    let settings = Settings {
        listen_port: 0,
        download_dir: PathBuf::from("/tmp"),
        enable_dht: false,
        enable_lsd: false,
        enable_upnp: false,
        enable_natpmp: false,
        resume_data_dir: Some(resume_dir),
        save_resume_interval_secs: 0,
        category_registry_path: Some(reg_path),
        ..Settings::default()
    };
    irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("start session")
}

#[tokio::test]
async fn add_torrent_with_tags_populates_stats_deterministically() {
    let session = test_session("basic").await;

    // Magnet URI: 20-byte info hash, no trackers — parsed purely so we
    // can submit via `AddTorrentParams::magnet()`.
    let magnet_uri = "magnet:?xt=urn:btih:0102030405060708090a0b0c0d0e0f1011121314&dn=A5Tags";

    let params =
        AddTorrentParams::magnet(magnet_uri).with_tags(vec!["sonarr".into(), "kids".into()]);
    let info_hash = session
        .add_torrent(params)
        .await
        .expect("add_torrent with tags should succeed");

    // Deterministic — tags are in `TorrentConfig` at actor construction,
    // so the very first `stats()` snapshot already has them. No poll
    // loop, no sleep. If this ever flakes, the add-time bake has
    // regressed to a post-add spawn race.
    let stats = session
        .torrent_stats(info_hash)
        .await
        .expect("stats should succeed");

    assert_eq!(
        stats.tags,
        vec!["sonarr".to_string(), "kids".to_string()],
        "tags must be present on the first stats snapshot"
    );
}

#[tokio::test]
async fn add_torrent_without_tags_defaults_to_empty() {
    let session = test_session("empty").await;

    let magnet_uri = "magnet:?xt=urn:btih:1112131415161718191a1b1c1d1e1f2021222324&dn=NoTags";

    let params = AddTorrentParams::magnet(magnet_uri);
    let info_hash = session
        .add_torrent(params)
        .await
        .expect("add_torrent should succeed");

    let stats = session.torrent_stats(info_hash).await.expect("stats");
    assert!(
        stats.tags.is_empty(),
        "tags must default to empty when `with_tags` is not called: got {:?}",
        stats.tags
    );
}

#[tokio::test]
async fn add_torrent_with_empty_tags_vec_is_uncategorised() {
    // Calling `.with_tags(vec![])` is a valid explicit "no tags" form
    // that should behave identically to omitting the call.
    let session = test_session("empty-vec").await;

    let magnet_uri = "magnet:?xt=urn:btih:2122232425262728292a2b2c2d2e2f3031323334&dn=EmptyVec";

    let params = AddTorrentParams::magnet(magnet_uri).with_tags(Vec::new());
    let info_hash = session
        .add_torrent(params)
        .await
        .expect("add_torrent should succeed");

    let stats = session.torrent_stats(info_hash).await.expect("stats");
    assert!(
        stats.tags.is_empty(),
        "explicit `with_tags(vec![])` must yield empty tags: got {:?}",
        stats.tags
    );
}

// ── M171 Task A6: tags round-trip through `FastResumeData` ──────────────
//
// The save side bakes `TorrentConfig::tags` into `FastResumeData::tags`
// when `TorrentActor::build_resume_data()` / `build_stats()` fires. The
// load side (session-level auto-restore) threads `rd.tags` into a fresh
// `TorrentConfig` via `handle_add_magnet` / `handle_add_torrent` BEFORE
// the new `TorrentActor` spawns, matching the `AddTorrentParams::with_tags`
// semantics from A5.
//
// This test covers the full save → shutdown → restart → restore path by
// running two `SessionHandle`s in sequence against the same isolated
// `resume_data_dir`. We force `need_save_resume = true` via a pause
// (`transition_state(Paused)` flips the dirty flag), call the explicit
// `save_resume_state()` API for deterministic timing, then spin up a
// second session and assert `stats.tags` on the restored torrent matches
// the exact `Vec<String>` that went in.

#[tokio::test]
async fn tags_persist_across_session_restart() {
    // Use the same resume dir + category registry across both sessions.
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let resume_dir =
        std::env::temp_dir().join(format!("irontide-m171-add-tags-a6-resume-{pid}-{n}"));
    let reg_path = std::env::temp_dir().join(format!("irontide-m171-add-tags-a6-{pid}-{n}.toml"));
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&reg_path);

    let magnet_uri = "magnet:?xt=urn:btih:3132333435363738393a3b3c3d3e3f4041424344&dn=A6RoundTrip";
    let expected_tags = vec!["sonarr".to_string(), "kids".to_string()];

    let info_hash = {
        let session = session_with(resume_dir.clone(), reg_path.clone()).await;

        let params = AddTorrentParams::magnet(magnet_uri).with_tags(expected_tags.clone());
        let info_hash = session
            .add_torrent(params)
            .await
            .expect("add_torrent with tags should succeed");

        // Force the dirty flag via a pause → `transition_state` sets
        // `need_save_resume = true` unconditionally. Without this the
        // explicit `save_resume_state()` would no-op for a freshly
        // added magnet that has not yet cycled states.
        session
            .pause_torrent(info_hash)
            .await
            .expect("pause should succeed");

        // Give the actor a moment to process the pause command so that
        // by the time `save_resume_state` reads `stats.need_save_resume`
        // the dirty flag is visible.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let saved = session
            .save_resume_state()
            .await
            .expect("save_resume_state should succeed");
        assert!(
            saved >= 1,
            "at least one dirty torrent should have been saved, got {saved}"
        );

        // Verify the resume file was written to the isolated dir.
        let resume_path = resume_dir
            .join("torrents")
            .join(format!("{}.resume", info_hash.to_hex()));
        assert!(
            resume_path.exists(),
            "resume file should exist at {}",
            resume_path.display()
        );

        session.shutdown().await.expect("shutdown");
        info_hash
    };

    // Second session: auto-restore runs inside `SessionHandle::start()`
    // before the handle is returned, but still interleaves with the
    // actor loop — give it a small window to finish processing.
    {
        let session = session_with(resume_dir.clone(), reg_path.clone()).await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let list = session
            .list_torrents()
            .await
            .expect("list_torrents should succeed");
        assert!(
            list.contains(&info_hash),
            "torrent {} should be auto-restored in second session",
            info_hash.to_hex()
        );

        let stats = session
            .torrent_stats(info_hash)
            .await
            .expect("stats on restored torrent should succeed");
        assert_eq!(
            stats.tags, expected_tags,
            "restored stats.tags must match the tags supplied at add time"
        );

        session.shutdown().await.expect("shutdown");
    }

    // Explicit cleanup — `fresh_paths` tears down at the *start* of
    // each test via `remove_dir_all`, but A6 is the only test that
    // reuses its dir across two sessions, so we tidy up here too.
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&reg_path);
}
