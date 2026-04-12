//! M159 Task 6 — surface-level integration tests for the `shell` subcommand.
//!
//! The parser itself is exercised by unit tests inside `src/repl.rs`
//! (they need access to the `pub(crate)` types `Cli`, `Command`,
//! and `parse_shell_line`, which an external integration test cannot
//! see). This file only verifies that:
//!
//! 1. `irontide shell --help` exits cleanly and the help text lists
//!    `shell` alongside the other subcommands.
//! 2. `irontide shell` with EOF on stdin (no daemon running) exits
//!    cleanly without panicking — the REPL should save its history,
//!    abort the background refresh task, and return 0.
//!
//! Anything beyond this (colour handling, prompt formatting, rustyline
//! line-editing behaviour) is impractical to test in a non-interactive
//! subprocess. See `src/repl.rs` for the parser test suite and the
//! Task 6 completion report for the manual smoke-test checklist.

use std::io::Write as _;
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn test_top_level_help_mentions_shell() {
    let output = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .arg("--help")
        .output()
        .expect("failed to run irontide");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("shell"),
        "expected `shell` in top-level help, got:\n{stdout}"
    );
}

#[test]
fn test_shell_eof_exits_cleanly() {
    // Run `irontide shell` with an unreachable daemon on a port we
    // know nothing is listening on, and close stdin immediately.
    // rustyline should see EOF on the very first readline, print a
    // blank line, and return 0. The whole test should finish in well
    // under a second.
    let mut child = Command::new(env!("CARGO_BIN_EXE_irontide"))
        .args(["--api-url", "http://127.0.0.1:1", "shell"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn irontide shell");

    // Close stdin to trigger EOF.
    if let Some(mut stdin) = child.stdin.take() {
        // Explicitly drop after a no-op write so the child definitely
        // sees the EOF (some rustyline builds buffer aggressively).
        let _ = stdin.write_all(b"");
        drop(stdin);
    }

    // Poll for exit with a short deadline.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            assert!(
                status.success(),
                "irontide shell exited non-zero: {status:?}"
            );
            return;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            panic!("irontide shell did not exit within 10s of EOF");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
