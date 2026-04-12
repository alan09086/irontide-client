//! Integration tests for `irontide completions <shell>`.
//!
//! Each test invokes the binary and verifies that the generated shell
//! completion script contains a shell-specific marker string. Completions
//! are generated at runtime by `clap_complete` so these tests also serve
//! as a smoke test that the CLI definition is valid (clap panics at
//! build time or completion-generation time if it detects conflicts).

use assert_cmd::Command;

fn irontide() -> Command {
    Command::cargo_bin("irontide").expect("binary exists")
}

#[test]
fn completions_bash_produces_output() {
    let output = irontide()
        .args(["completions", "bash"])
        .output()
        .expect("run completions bash");

    assert!(output.status.success(), "bash completions should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("_irontide"),
        "bash completions should contain _irontide function, got: {stdout}"
    );
}

#[test]
fn completions_zsh_produces_output() {
    let output = irontide()
        .args(["completions", "zsh"])
        .output()
        .expect("run completions zsh");

    assert!(output.status.success(), "zsh completions should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("#compdef irontide"),
        "zsh completions should contain #compdef irontide, got: {stdout}"
    );
}

#[test]
fn completions_fish_produces_output() {
    let output = irontide()
        .args(["completions", "fish"])
        .output()
        .expect("run completions fish");

    assert!(output.status.success(), "fish completions should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("complete -c irontide"),
        "fish completions should contain 'complete -c irontide', got: {stdout}"
    );
}

#[test]
fn completions_elvish_produces_output() {
    let output = irontide()
        .args(["completions", "elvish"])
        .output()
        .expect("run completions elvish");

    assert!(output.status.success(), "elvish completions should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("irontide"),
        "elvish completions should mention irontide, got: {stdout}"
    );
}

#[test]
fn completions_powershell_produces_output() {
    let output = irontide()
        .args(["completions", "powershell"])
        .output()
        .expect("run completions powershell");

    assert!(
        output.status.success(),
        "powershell completions should exit 0"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("irontide"),
        "powershell completions should mention irontide, got: {stdout}"
    );
}

#[test]
fn completions_invalid_shell_fails() {
    irontide()
        .args(["completions", "notashell"])
        .assert()
        .failure();
}
