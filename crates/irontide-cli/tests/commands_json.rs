//! M159 Task 5 — integration smoke tests for the batch subcommands.
//!
//! Each test spawns its own `irontide daemon` on a free port, runs the
//! CLI binary as a subprocess against it, and asserts on the JSON /
//! exit-code surface. The goal is a narrow safety net — enough to catch
//! obvious wire breakage when future milestones touch the daemon or
//! dispatch glue — not an exhaustive semantic suite.
//!
//! Dropping a `DaemonHandle` kills the child daemon, which also releases
//! the TCP port and cleans the temp download directory.
//!
//! ## Flakiness notes
//!
//! The daemon takes ~500 ms to bind the API socket from a cold start.
//! `setup_daemon()` polls stderr for the `listening on` line with a
//! 15-second deadline. If a test times out regularly on slow CI, the
//! cleanest fix is to mark the offending case `#[ignore]` and file a
//! follow-up — do **not** bump the deadline beyond 30 seconds.

use std::io::{BufRead as _, BufReader};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// Maximum time we'll wait for `irontide daemon` to print its `listening on`
/// banner before failing the test.
const DAEMON_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);

/// A running `irontide daemon` child process. Dropping this struct sends
/// SIGKILL to the child (and waits for it) so tests never leak a daemon.
struct DaemonHandle {
    child: Child,
    port: u16,
    _tempdir: TempDir,
    /// Drains the daemon's stderr pipe in the background. Keeping the read
    /// end of the pipe open is critical: if it were dropped after reading the
    /// startup banner, the daemon would receive SIGPIPE the next time it logs
    /// anything and die mid-request — causing "daemon unreachable" failures on
    /// operations that trigger logging (e.g. `add` but not `list`).
    _stderr_drain: JoinHandle<()>,
}

impl DaemonHandle {
    /// Build the `--api-url http://127.0.0.1:<port>` prefix.
    fn api_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for DaemonHandle {
    fn drop(&mut self) {
        // `kill` succeeds if the child is already dead — we don't care
        // about the return value here, just that the process is gone.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Find an ephemeral TCP port by binding to `:0` and immediately
/// dropping the listener. There is a narrow race window where another
/// process could grab the port before the daemon does, but in practice
/// it's well under the flakiness floor of spawning a child daemon.
fn pick_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local_addr").port();
    drop(listener);
    port
}

/// Spawn `irontide daemon` on a free port and wait until it advertises
/// a listening socket via stderr.
fn setup_daemon() -> DaemonHandle {
    let port = pick_free_port();
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let resume_dir = tempdir.path().join("resume");

    let mut child = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args([
            "daemon",
            "--api-port",
            &port.to_string(),
            "--api-bind",
            "127.0.0.1",
            "--download-dir",
            tempdir.path().to_str().expect("tempdir utf-8"),
            "--resume-dir",
            resume_dir.to_str().expect("resume dir utf-8"),
            "--no-dht",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn irontide daemon");

    // Read the daemon's stderr one line at a time until we see the
    // `listening on` banner, with a hard deadline.
    let stderr = child.stderr.take().expect("child stderr pipe");
    let deadline = Instant::now() + DAEMON_STARTUP_TIMEOUT;
    let mut reader = BufReader::new(stderr);
    let mut buf = String::new();
    loop {
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!(
                "irontide daemon did not start within {:?} (port {port})",
                DAEMON_STARTUP_TIMEOUT
            );
        }
        buf.clear();
        match reader.read_line(&mut buf) {
            Ok(0) => {
                // EOF before the banner — child almost certainly died.
                let _ = child.kill();
                panic!("irontide daemon exited before banner (port {port})");
            }
            Ok(_) => {
                if buf.contains("listening on") {
                    break;
                }
            }
            Err(e) => {
                let _ = child.kill();
                panic!("reading daemon stderr failed: {e}");
            }
        }
    }

    // Keep reading stderr so the pipe's read end stays open. If we drop the
    // BufReader here instead, the daemon gets SIGPIPE on its next log write.
    let drain = std::thread::spawn(move || {
        let mut buf = String::new();
        loop {
            buf.clear();
            match reader.read_line(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });

    DaemonHandle {
        child,
        port,
        _tempdir: tempdir,
        _stderr_drain: drain,
    }
}

/// Run `irontide --api-url http://127.0.0.1:<port> <args...>` and return
/// the captured output. Panics on spawn failure, but leaves exit-code
/// inspection to the caller.
fn run_cli(daemon: &DaemonHandle, args: &[&str]) -> std::process::Output {
    let url = daemon.api_url();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_irontide"));
    cmd.arg("--api-url").arg(&url);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.stdin(Stdio::null())
        .output()
        .expect("run irontide subcommand")
}

/// Parse the binary's stdout as JSON, panicking on malformed input with
/// a helpful error message that includes the raw output.
fn parse_json(out: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        let stderr = String::from_utf8_lossy(&out.stderr);
        panic!("expected JSON stdout, got: {stdout}\nstderr: {stderr}\nerror: {e}");
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_list_empty_daemon() {
    let daemon = setup_daemon();
    let out = run_cli(&daemon, &["list", "--json"]);
    assert!(
        out.status.success(),
        "list failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value = parse_json(&out);
    assert!(value.is_array(), "expected array, got: {value}");
    assert_eq!(value.as_array().map(Vec::len), Some(0));
}

#[test]
fn test_add_magnet() {
    let daemon = setup_daemon();
    let magnet = "magnet:?xt=urn:btih:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa&dn=test";
    let out = run_cli(&daemon, &["add", magnet, "--json"]);
    assert!(
        out.status.success(),
        "add failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value = parse_json(&out);
    let hash = value
        .get("info_hash")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert_eq!(hash.len(), 40, "expected 40-char hash, got: {value}");
}

#[test]
fn test_add_list_roundtrip() {
    let daemon = setup_daemon();
    let magnet = "magnet:?xt=urn:btih:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb&dn=rt1";
    let add_out = run_cli(&daemon, &["add", magnet, "--json"]);
    assert!(add_out.status.success());

    let list_out = run_cli(&daemon, &["list", "--json"]);
    assert!(list_out.status.success());
    let list = parse_json(&list_out);
    let arr = list.as_array().expect("expected array");
    assert_eq!(arr.len(), 1, "expected 1 torrent, got {list}");
    let first = &arr[0];
    assert_eq!(
        first.get("info_hash").and_then(|v| v.as_str()),
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    );
}

#[test]
fn test_pause_resume_roundtrip() {
    let daemon = setup_daemon();
    let magnet = "magnet:?xt=urn:btih:cccccccccccccccccccccccccccccccccccccccc&dn=rt2";
    let add = run_cli(&daemon, &["add", magnet, "--json"]);
    assert!(add.status.success());

    let pause = run_cli(
        &daemon,
        &[
            "pause",
            "cccccccccccccccccccccccccccccccccccccccc",
            "--json",
        ],
    );
    assert!(
        pause.status.success(),
        "pause failed: stderr={}",
        String::from_utf8_lossy(&pause.stderr)
    );
    let pause_val = parse_json(&pause);
    assert_eq!(
        pause_val.get("action").and_then(|v| v.as_str()),
        Some("paused")
    );

    let resume = run_cli(
        &daemon,
        &[
            "resume",
            "cccccccccccccccccccccccccccccccccccccccc",
            "--json",
        ],
    );
    assert!(resume.status.success());
    let resume_val = parse_json(&resume);
    assert_eq!(
        resume_val.get("action").and_then(|v| v.as_str()),
        Some("resumed")
    );
}

#[test]
fn test_seed_unseed_roundtrip() {
    let daemon = setup_daemon();
    let magnet = "magnet:?xt=urn:btih:dddddddddddddddddddddddddddddddddddddddd&dn=rt3";
    let add = run_cli(&daemon, &["add", magnet, "--json"]);
    assert!(add.status.success());

    let seed = run_cli(
        &daemon,
        &["seed", "dddddddddddddddddddddddddddddddddddddddd", "--json"],
    );
    assert!(
        seed.status.success(),
        "seed failed: stderr={}",
        String::from_utf8_lossy(&seed.stderr)
    );

    // `info --json` should now report `user_seed_mode: true`.
    let info = run_cli(
        &daemon,
        &["info", "dddddddddddddddddddddddddddddddddddddddd", "--json"],
    );
    assert!(
        info.status.success(),
        "info failed: stderr={}",
        String::from_utf8_lossy(&info.stderr)
    );
    let info_val = parse_json(&info);
    let stats = info_val
        .get("stats")
        .or(Some(&info_val))
        .expect("stats object");
    let user_seed = stats
        .get("user_seed_mode")
        .and_then(serde_json::Value::as_bool);
    assert_eq!(
        user_seed,
        Some(true),
        "expected user_seed_mode=true, got: {info_val}"
    );

    let unseed = run_cli(
        &daemon,
        &[
            "unseed",
            "dddddddddddddddddddddddddddddddddddddddd",
            "--json",
        ],
    );
    assert!(unseed.status.success());

    let info2 = run_cli(
        &daemon,
        &["info", "dddddddddddddddddddddddddddddddddddddddd", "--json"],
    );
    assert!(info2.status.success());
    let info2_val = parse_json(&info2);
    let stats2 = info2_val
        .get("stats")
        .or(Some(&info2_val))
        .expect("stats object");
    let user_seed2 = stats2
        .get("user_seed_mode")
        .and_then(serde_json::Value::as_bool);
    assert_eq!(
        user_seed2,
        Some(false),
        "expected user_seed_mode=false, got: {info2_val}"
    );
}

#[test]
fn test_rm_torrent() {
    let daemon = setup_daemon();
    let magnet = "magnet:?xt=urn:btih:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee&dn=rm";
    let add = run_cli(&daemon, &["add", magnet, "--json"]);
    assert!(add.status.success());

    let rm = run_cli(
        &daemon,
        &["rm", "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", "--json"],
    );
    assert!(
        rm.status.success(),
        "rm failed: stderr={}",
        String::from_utf8_lossy(&rm.stderr)
    );

    let list = run_cli(&daemon, &["list", "--json"]);
    assert!(list.status.success());
    let value = parse_json(&list);
    let arr = value.as_array().expect("array");
    assert!(arr.is_empty(), "expected empty after rm, got: {value}");
}

#[test]
fn test_nonexistent_hash_returns_error() {
    let daemon = setup_daemon();
    let out = run_cli(
        &daemon,
        &[
            "pause",
            "0000000000000000000000000000000000000000",
            "--json",
        ],
    );
    assert!(
        !out.status.success(),
        "expected non-zero exit, got: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn test_daemon_unreachable_exits_3() {
    // Grab a free port but don't bind a daemon to it — the CLI should
    // classify the error as `DaemonUnreachable` and exit with code 3.
    let port = pick_free_port();
    let url = format!("http://127.0.0.1:{port}");
    let out = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["--api-url", &url, "list", "--json"])
        .stdin(Stdio::null())
        .output()
        .expect("run irontide list");
    let code = out.status.code().unwrap_or(-1);
    assert_eq!(
        code,
        3,
        "expected exit 3, got {code}. stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn test_info_hex_prefix_dispatches_to_daemon() {
    // Add a torrent, then inspect it via a short (non-40-char) hex prefix.
    // If the dispatcher wrongly treated a short hex string as a file
    // path, the command would fail with a path-error. The success here
    // proves the "not a file + is hex" arm goes to the daemon.
    let daemon = setup_daemon();
    let magnet = "magnet:?xt=urn:btih:ffffffffffffffffffffffffffffffffffffffff&dn=prefix";
    let add = run_cli(&daemon, &["add", magnet, "--json"]);
    assert!(add.status.success());

    let info = run_cli(&daemon, &["info", "ffffffff", "--json"]);
    assert!(
        info.status.success(),
        "info with prefix failed: stderr={}",
        String::from_utf8_lossy(&info.stderr)
    );
    // Parse JSON but only assert it parses — the exact schema is
    // owned by `progress::render_json` and not part of M159's contract.
    let _ = parse_json(&info);
}
