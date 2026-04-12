//! Integration tests for the `irontide config <action>` subcommands.
//!
//! Each test exercises the binary as a subprocess via `assert_cmd`. None
//! of these tests need a running daemon — the `config` subcommand tree
//! is purely local.

use assert_cmd::Command;
use tempfile::TempDir;

fn irontide() -> Command {
    Command::cargo_bin("irontide").expect("binary exists")
}

#[test]
fn config_path_prints_default() {
    let output = irontide()
        .args(["config", "path"])
        .output()
        .expect("run config path");

    assert!(output.status.success(), "config path should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim_end().ends_with("config.toml"),
        "expected path ending with config.toml, got: {stdout}"
    );
}

#[test]
fn config_path_respects_global_flag() {
    let output = irontide()
        .args(["--config", "/tmp/custom.toml", "config", "path"])
        .output()
        .expect("run config path with --config");

    assert!(output.status.success(), "config path should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("/tmp/custom.toml"),
        "expected /tmp/custom.toml in output, got: {stdout}"
    );
}

#[test]
fn config_show_prints_toml() {
    let output = irontide()
        .args(["config", "show"])
        .output()
        .expect("run config show");

    assert!(output.status.success(), "config show should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[session]"),
        "expected [session] in output, got: {stdout}"
    );
    assert!(
        stdout.contains("listen_port"),
        "expected listen_port in output, got: {stdout}"
    );
}

#[test]
fn config_init_creates_file() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.toml");

    irontide()
        .args([
            "--config",
            config_path.to_str().expect("utf-8 path"),
            "config",
            "init",
        ])
        .assert()
        .success();

    assert!(config_path.exists(), "config file should be created");
    let contents = std::fs::read_to_string(&config_path).expect("read config");
    assert!(
        contents.contains("[session]"),
        "created file should contain [session]"
    );
}

#[test]
fn config_init_refuses_overwrite() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.toml");
    std::fs::write(&config_path, "existing content").expect("write seed file");

    irontide()
        .args([
            "--config",
            config_path.to_str().expect("utf-8 path"),
            "config",
            "init",
        ])
        .assert()
        .failure();

    // Verify original content is preserved.
    let contents = std::fs::read_to_string(&config_path).expect("read config");
    assert_eq!(contents, "existing content");
}

#[test]
fn config_init_force_overwrites() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.toml");
    std::fs::write(&config_path, "existing content").expect("write seed file");

    irontide()
        .args([
            "--config",
            config_path.to_str().expect("utf-8 path"),
            "config",
            "init",
            "--force",
        ])
        .assert()
        .success();

    let contents = std::fs::read_to_string(&config_path).expect("read config");
    assert!(
        contents.contains("[session]"),
        "overwritten file should contain [session]"
    );
}

#[test]
fn config_validate_valid_file() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        "[session]\nlisten_port = 42020\n\n[limits]\nmax_peers_per_torrent = 64\n",
    )
    .expect("write config");

    let output = irontide()
        .args(["config", "validate", config_path.to_str().expect("utf-8")])
        .output()
        .expect("run validate");

    assert!(output.status.success(), "validate should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("valid"),
        "expected 'valid' in output, got: {stdout}"
    );
}

#[test]
fn config_validate_missing_file() {
    irontide()
        .args(["config", "validate", "/nonexistent/path/config.toml"])
        .assert()
        .failure();
}
