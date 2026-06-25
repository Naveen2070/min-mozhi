//! `mimz completions <shell>` — print a shell tab-completion script to stdout.
//! Generated straight from the clap command tree, so it always matches the
//! real subcommands and flags. Supports bash, zsh, fish, powershell, elvish.

use std::process::ExitCode;

use clap::CommandFactory;

/// `mimz completions <shell>` — print a shell tab-completion script to stdout.
/// Generated straight from the clap command tree (`crate::Cli`), so it always
/// matches the real subcommands/flags. Install per your shell, e.g.:
///   bash:       `mimz completions bash > /etc/bash_completion.d/mimz`
///   powershell: `mimz completions powershell >> $PROFILE`
pub(crate) fn completions(shell: clap_complete::Shell) -> ExitCode {
    let mut cmd = crate::Cli::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
    ExitCode::SUCCESS
}
