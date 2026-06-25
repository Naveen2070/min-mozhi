//! `mimz explain [--list] [<CODE>]` — print the long-form teaching text for a
//! diagnostic code, or list every code with `--list`. Backed by the library so
//! editors and the WASM playground share the same catalog.

use std::process::ExitCode;

/// `mimz explain [--list] [<CODE>]` — print the long-form teaching text for a
/// diagnostic code on stdout, or with `--list` print every code's one-line
/// summary. Unknown codes print a friendly message (listing valid codes) on
/// stderr. Backed by the lib so editors/WASM share the same catalog.
pub(crate) fn explain_code(code: Option<&str>, list: bool) -> ExitCode {
    if list {
        for (code, summary) in mimz::explain::list_all() {
            println!("{code}  {summary}");
        }
        return ExitCode::SUCCESS;
    }
    match code {
        Some(code) => match mimz::explain::explain(code) {
            Some(text) => {
                println!("{text}");
                ExitCode::SUCCESS
            }
            None => {
                eprintln!("error: no explanation for `{code}` — is it a real diagnostic code?");
                let codes: Vec<&str> = mimz::explain::codes().collect();
                eprintln!("known codes: {}", codes.join(", "));
                ExitCode::FAILURE
            }
        },
        None => {
            eprintln!("error: expected a diagnostic code or `--list`");
            ExitCode::FAILURE
        }
    }
}
