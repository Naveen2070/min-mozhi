// -------------------------------------------------------------- explain

use std::process::ExitCode;

/// `mimz explain <CODE>` — print the long-form teaching text for a diagnostic
/// code on stdout, or a friendly "unknown code" message (listing the valid
/// ones) on stderr. Backed by the lib so editors/WASM share the same catalog.
pub(crate) fn explain_code(code: &str) -> ExitCode {
    match mimz::explain::explain(code) {
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
    }
}
