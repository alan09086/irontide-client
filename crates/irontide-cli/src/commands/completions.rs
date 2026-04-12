use clap::CommandFactory as _;
use clap_complete::Shell;

use crate::cli_def::Cli;

/// Print shell completions for `shell` to stdout.
pub(crate) fn run(shell: Shell) -> i32 {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "irontide", &mut std::io::stdout());
    0
}
