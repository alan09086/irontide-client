//! M171 A5: `AddTorrentParams::with_tags` — add-time tag baking.
//!
//! Pins the contract that tags supplied on an add call are written into
//! the `TorrentConfig` BEFORE the `TorrentActor` is spawned. The plan
//! explicitly rejects the post-add `tokio::spawn(handle.set_tags(...))`
//! shape used for `category` (M170), because it leaves the first
//! `stats()` snapshot tag-less and forces callers to poll. This test
//! guards that the first snapshot already carries the tags — no sleep,
//! no retry.
//!
//! This is a pure `SessionHandle` test — no HTTP / axum harness needed.
//! It reuses the resume / category-registry isolation helpers from the
//! M170 Lane D regression suite.
//!
//! Mirrors: `qbt_v2_add_source_regression.rs::session_handle_add_magnet_uri_still_works`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

// Use the M170 params struct (re-exported from the session crate under
// an aliased name because the facade owns its own `AddTorrentParams`).
use irontide::session::{
    SessionAddTorrentParams as AddTorrentParams, SessionHandle, Settings,
};

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fresh_paths(tag: &str) -> (PathBuf, PathBuf) {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let resume_dir = std::env::temp_dir().join(format!(
        "irontide-m171-add-tags-{tag}-resume-{pid}-{n}"
    ));
    let reg_path = std::env::temp_dir()
        .join(format!("irontide-m171-add-tags-{tag}-{pid}-{n}.toml"));
    let _ = std::fs::remove_dir_all(&resume_dir);
    let _ = std::fs::remove_file(&reg_path);
    (resume_dir, reg_path)
}

async fn test_session(tag: &str) -> SessionHandle {
    let (resume_dir, reg_path) = fresh_paths(tag);
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

    let params = AddTorrentParams::magnet(magnet_uri)
        .with_tags(vec!["sonarr".into(), "kids".into()]);
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
