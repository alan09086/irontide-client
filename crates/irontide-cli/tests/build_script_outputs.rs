//! Smoke test verifying that `build.rs` ran successfully and the CLI
//! definition is valid for shell completion generation.
//!
//! Checking the `target/debug/build/` directory for completion files is
//! fragile (the hash-based subdirectory name is unstable). Instead, we
//! verify the invariant that matters: the compiled binary can generate
//! completions at runtime, which proves the CLI definition is
//! well-formed and `build.rs` executed without errors.

use assert_cmd::Command;

fn irontide() -> Command {
    Command::cargo_bin("irontide").expect("binary exists")
}

#[test]
fn build_script_ran_successfully() {
    let output = irontide()
        .args(["completions", "bash"])
        .output()
        .expect("run completions");

    assert!(
        output.status.success(),
        "completions should work: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stdout.is_empty(),
        "completions should produce non-empty output"
    );
}
