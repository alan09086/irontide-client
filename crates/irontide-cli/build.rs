use std::env;
use std::path::PathBuf;

// Include the CLI definition — this works because cli_def.rs only uses
// clap and std imports (no `crate::` references).
include!("src/cli_def.rs");

fn main() {
    // Only re-run if the CLI definition changes.
    println!("cargo:rerun-if-changed=src/cli_def.rs");

    let outdir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR not set"));
    let completions_dir = outdir.join("completions");
    std::fs::create_dir_all(&completions_dir).expect("failed to create completions directory");

    let mut cmd = <Cli as clap::CommandFactory>::command();

    for shell in [
        clap_complete::Shell::Bash,
        clap_complete::Shell::Zsh,
        clap_complete::Shell::Fish,
        clap_complete::Shell::Elvish,
        clap_complete::Shell::PowerShell,
    ] {
        clap_complete::generate_to(shell, &mut cmd, "irontide", &completions_dir)
            .expect("failed to generate shell completions");
    }
}
