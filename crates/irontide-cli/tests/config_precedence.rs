//! Integration tests for the configuration layering pipeline.
//!
//! These tests exercise the full binary (not internal functions) to
//! verify that defaults, TOML files, and environment variables merge
//! correctly when viewed through `irontide config show`.
//!
//! Because `assert_cmd` spawns a child process, setting environment
//! variables via `.env()` on the `Command` does not leak between tests
//! and is safe for parallel execution.

use std::io::Write as _;

use assert_cmd::Command;
use tempfile::TempDir;

fn irontide() -> Command {
    Command::cargo_bin("irontide").expect("binary exists")
}

/// Helper: run `irontide [--config <path>] config show` and return stdout
/// as a `String`. Panics if the command fails.
fn config_show(config_path: Option<&str>) -> String {
    let mut cmd = irontide();
    if let Some(path) = config_path {
        cmd.args(["--config", path]);
    }
    cmd.args(["config", "show"]);
    let output = cmd.output().expect("run config show");
    assert!(
        output.status.success(),
        "config show failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Helper: run `irontide --config <path> config show` with the given
/// environment variables set. Returns stdout as a `String`.
fn config_show_with_env(config_path: Option<&str>, env_vars: &[(&str, &str)]) -> String {
    let mut cmd = irontide();
    if let Some(path) = config_path {
        cmd.args(["--config", path]);
    }
    cmd.args(["config", "show"]);
    for &(key, val) in env_vars {
        cmd.env(key, val);
    }
    let output = cmd.output().expect("run config show");
    assert!(
        output.status.success(),
        "config show failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Write TOML content to a temporary config file and return the tmpdir
/// (keeps it alive) and the path string.
fn write_temp_config(content: &str) -> (TempDir, String) {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.toml");
    let mut file = std::fs::File::create(&config_path).expect("create config file");
    file.write_all(content.as_bytes())
        .expect("write config content");
    let path_str = config_path.to_str().expect("utf-8 path").to_owned();
    (dir, path_str)
}

// ─────────────────────────────────────────────────────────────────────────────
// Defaults
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn defaults_without_config_file() {
    // Point --config at a nonexistent file so the TOML layer is skipped
    // and no real user config leaks in.
    let stdout = config_show(Some("/tmp/irontide-test-no-such-file/config.toml"));
    assert!(
        stdout.contains("listen_port = 42020"),
        "expected default listen_port = 42020, got:\n{stdout}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// TOML file overrides
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn toml_file_overrides_defaults() {
    let (_dir, path) = write_temp_config("[session]\nlisten_port = 12345\n");

    let stdout = config_show(Some(&path));
    assert!(
        stdout.contains("listen_port = 12345"),
        "expected TOML override listen_port = 12345, got:\n{stdout}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Environment variable overrides
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn env_var_overrides_defaults() {
    let stdout = config_show_with_env(
        Some("/tmp/irontide-test-no-such-file/config.toml"),
        &[("IRONTIDE_LISTEN_PORT", "54321")],
    );
    assert!(
        stdout.contains("listen_port = 54321"),
        "expected env override listen_port = 54321, got:\n{stdout}"
    );
}

#[test]
fn env_var_overrides_toml_file() {
    let (_dir, path) = write_temp_config("[session]\nlisten_port = 11111\n");

    let stdout = config_show_with_env(Some(&path), &[("IRONTIDE_LISTEN_PORT", "22222")]);
    assert!(
        stdout.contains("listen_port = 22222"),
        "expected env to beat TOML (22222 > 11111), got:\n{stdout}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Missing / partial / empty TOML files
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn missing_toml_file_uses_defaults() {
    let stdout = config_show(Some("/tmp/irontide-test-nonexistent-42/config.toml"));
    assert!(
        stdout.contains("listen_port = 42020"),
        "expected defaults for missing file, got:\n{stdout}"
    );
}

#[test]
fn partial_toml_only_overrides_specified() {
    let (_dir, path) = write_temp_config("[limits]\nmax_peers_per_torrent = 64\n");

    let stdout = config_show(Some(&path));
    // listen_port should still be the default.
    assert!(
        stdout.contains("listen_port = 42020"),
        "expected default listen_port, got:\n{stdout}"
    );
    // max_peers_per_torrent should be overridden.
    assert!(
        stdout.contains("max_peers_per_torrent = 64"),
        "expected overridden max_peers_per_torrent = 64, got:\n{stdout}"
    );
}

#[test]
fn empty_toml_uses_defaults() {
    let (_dir, path) = write_temp_config("");

    let stdout = config_show(Some(&path));
    assert!(
        stdout.contains("listen_port = 42020"),
        "expected defaults for empty TOML, got:\n{stdout}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Error cases
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn toml_with_invalid_type_fails() {
    let (_dir, path) = write_temp_config("[session]\nlisten_port = \"not_a_number\"\n");

    irontide()
        .args(["--config", &path, "config", "show"])
        .assert()
        .failure();
}

// ─────────────────────────────────────────────────────────────────────────────
// Round-trip: init then validate, init then show
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn round_trip_init_then_validate() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.toml");
    let path_str = config_path.to_str().expect("utf-8 path");

    // Init must succeed.
    irontide()
        .args(["--config", path_str, "config", "init"])
        .assert()
        .success();

    // The generated file must validate cleanly.
    irontide()
        .args(["config", "validate", path_str])
        .assert()
        .success();
}

#[test]
fn config_show_after_init_matches_defaults() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.toml");
    let path_str = config_path.to_str().expect("utf-8 path");

    // Init creates a commented-out default file.
    irontide()
        .args(["--config", path_str, "config", "init"])
        .assert()
        .success();

    // Show with that file should produce the same values as defaults
    // (all keys are commented out so none override).
    let stdout = config_show(Some(path_str));
    assert!(
        stdout.contains("listen_port = 42020"),
        "init file should produce default listen_port, got:\n{stdout}"
    );
}
