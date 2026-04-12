use std::process::Command;

#[test]
fn test_no_args_shows_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .output()
        .expect("failed to run irontide");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage") || stderr.contains("usage"),
        "expected usage text, got: {stderr}"
    );
}

#[test]
fn test_help_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .arg("--help")
        .output()
        .expect("failed to run irontide");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("download"));
    assert!(stdout.contains("create"));
    assert!(stdout.contains("info"));
}

#[test]
fn test_download_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["download", "--help"])
        .output()
        .expect("failed to run irontide");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--output"));
    assert!(stdout.contains("--seed"));
    assert!(stdout.contains("--port"));
}

#[test]
fn test_create_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["create", "--help"])
        .output()
        .expect("failed to run irontide");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--tracker"));
    assert!(stdout.contains("--private"));
}

#[test]
fn test_info_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["info", "--help"])
        .output()
        .expect("failed to run irontide");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lower = stdout.to_lowercase();
    // M159: `info` is a dual-role command. The positional is `<SOURCE>`
    // (file path OR hash prefix), so help mentions `.torrent` and `hash`.
    assert!(
        lower.contains("source") || lower.contains(".torrent") || lower.contains("hash"),
        "expected file/hash wording in info help, got: {stdout}"
    );
}

#[test]
fn test_info_nonexistent_file_and_not_hex() {
    // A path-like argument that is neither a real file nor valid hex
    // must produce the disambiguated error surface (M159 Task 5).
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["info", "/tmp/nonexistent_torrent_test_12345.torrent"])
        .output()
        .expect("failed to run irontide");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("neither an existing file nor a valid hex prefix"),
        "expected disambiguation error, got: {stderr}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// M159 — new batch subcommand help-text coverage.
// These tests exercise only `--help` / `--version` output so they don't
// need a running daemon. The richer "spawn a daemon and hit it over
// HTTP" coverage lives in `commands_json.rs`.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_list_help_mentions_filter() {
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["list", "--help"])
        .output()
        .expect("failed to run irontide");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("filter"),
        "expected 'filter' in list help, got: {stdout}"
    );
}

#[test]
fn test_add_help_mentions_magnet() {
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["add", "--help"])
        .output()
        .expect("failed to run irontide");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lower = stdout.to_lowercase();
    assert!(
        lower.contains("magnet"),
        "expected 'magnet' in add help, got: {stdout}"
    );
}

#[test]
fn test_seed_help_exists() {
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["seed", "--help"])
        .output()
        .expect("failed to run irontide");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hash") || stdout.contains("HASH"),
        "expected HASH positional in seed help, got: {stdout}"
    );
}

#[test]
fn test_unseed_help_exists() {
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["unseed", "--help"])
        .output()
        .expect("failed to run irontide");
    assert!(output.status.success());
}

#[test]
fn test_pause_resume_rm_help_exists() {
    for sub in ["pause", "resume", "rm"] {
        let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
            .args([sub, "--help"])
            .output()
            .unwrap_or_else(|e| panic!("failed to run irontide {sub} --help: {e}"));
        assert!(
            output.status.success(),
            "irontide {sub} --help should succeed"
        );
    }
}

#[test]
fn test_daemon_help_mentions_api_port() {
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["daemon", "--help"])
        .output()
        .expect("failed to run irontide");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("api-port"),
        "expected 'api-port' in daemon help, got: {stdout}"
    );
}

#[test]
fn test_download_help_lacks_dead_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["download", "--help"])
        .output()
        .expect("failed to run irontide");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // M159 removed these flags — they should no longer appear in help.
    for dead_flag in ["--overwrite", "--initial-peers", "--disable-trackers"] {
        assert!(
            !stdout.contains(dead_flag),
            "expected {dead_flag} to be removed, got: {stdout}"
        );
    }
    // `--list` (short `-l`) was also removed; searching the long form
    // avoids false positives on the word "list" that appears elsewhere
    // in clap's help output.
    assert!(
        !stdout.contains("--list"),
        "expected --list to be removed, got: {stdout}"
    );
}

#[test]
fn test_global_api_url_flag_advertised() {
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["list", "--help"])
        .output()
        .expect("failed to run irontide");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // --api-url is marked `global = true`, so every subcommand's help
    // should include it.
    assert!(
        stdout.contains("--api-url"),
        "expected global --api-url in subcommand help, got: {stdout}"
    );
}

#[test]
fn test_info_file_mode_still_works_for_existing_torrent() {
    // M159 dual-role `info`: passing an actual .torrent file must
    // dispatch the file-inspection path and ignore --files/--peers/--json.
    let dir = std::env::temp_dir().join("irontide_info_file_mode_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let data_file = dir.join("payload.txt");
    std::fs::write(&data_file, b"hello m159").unwrap();
    let torrent_path = dir.join("m159.torrent");

    // Create the torrent first.
    let create_out = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args([
            "create",
            data_file.to_str().unwrap(),
            "-o",
            torrent_path.to_str().unwrap(),
            "-t",
            "http://example.invalid/announce",
        ])
        .output()
        .expect("failed to run irontide create");
    assert!(
        create_out.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&create_out.stderr)
    );
    assert!(torrent_path.exists());

    // info against the file should succeed.
    let info_out = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["info", torrent_path.to_str().unwrap()])
        .output()
        .expect("failed to run irontide info");
    assert!(
        info_out.status.success(),
        "info failed: {}",
        String::from_utf8_lossy(&info_out.stderr)
    );
    let stdout = String::from_utf8_lossy(&info_out.stdout);
    assert!(
        stdout.contains("payload.txt"),
        "expected filename in info output, got: {stdout}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_create_and_info_roundtrip() {
    let dir = std::env::temp_dir().join("torrent_cli_test_create");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let test_file = dir.join("test.txt");
    std::fs::write(&test_file, "hello torrent").unwrap();

    let torrent_path = dir.join("test.torrent");

    // Create torrent
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args([
            "create",
            test_file.to_str().unwrap(),
            "-o",
            torrent_path.to_str().unwrap(),
            "-t",
            "http://tracker.example.com/announce",
        ])
        .output()
        .expect("failed to run irontide create");
    assert!(
        output.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(torrent_path.exists());

    // Info on created torrent
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["info", torrent_path.to_str().unwrap()])
        .output()
        .expect("failed to run irontide info");
    assert!(
        output.status.success(),
        "info failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("test.txt"),
        "expected filename in info output, got: {stdout}"
    );
    assert!(
        stdout.contains("tracker.example.com"),
        "expected tracker in info output, got: {stdout}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
