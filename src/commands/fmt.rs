// ------------------------------------------------------------------ fmt

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mimz::lexer::token::Flavor;
use mimz::{lexer, morph, project};

use crate::Output;

/// `mimz fmt <file>` — normalize the file's keyword flavor in place. Token-based
/// (via [`translate`](mimz::translate)), so comments, layout, identifiers, and
/// numbers are preserved byte-for-byte — only keyword spellings change. The
/// target flavor is `--to` if given, else the file's predominant flavor
/// (`morph::majority_flavor`). With `--strict`, a file that mixes keyword flavors
/// gets a warning first (mixing stays legal — spec/03, the learning path).
pub(crate) fn fmt_file(
    path: &Path,
    to: Option<&str>,
    strict: bool,
    output: Option<PathBuf>,
) -> ExitCode {
    let src = match project::read_source(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let path_str = path.display().to_string();
    let tokens = match lexer::lex(&src) {
        Ok(t) => t,
        Err(diags) => return Output::Human(Flavor::English).one_file(&diags, &src, &path_str),
    };
    // Target flavor: explicit `--to`, else the flavor the file mostly uses.
    let target = match to {
        Some(s) => match morph::parse_lang(s) {
            Some(f) => f,
            None => {
                eprintln!("error: unknown flavor `{s}` — expected english, tanglish, or tamil");
                return ExitCode::FAILURE;
            }
        },
        None => morph::majority_flavor(&tokens),
    };
    // `--strict` is the lint mode: it still normalizes (writes the fix), but a
    // mixed-flavor input is also reported and makes the command exit non-zero so
    // CI can flag it. Mixing stays legal under a plain `fmt`.
    let mut mixed = false;
    if strict {
        let used = morph::flavors_used(&tokens);
        if used.len() > 1 {
            mixed = true;
            let names: Vec<&str> = used
                .iter()
                .map(|&f| mimz::translate::flavor_name(f))
                .collect();
            eprintln!(
                "warning: `{path_str}` mixes keyword flavors ({}) — normalizing to {}",
                names.join(", "),
                mimz::translate::flavor_name(target)
            );
        }
    }
    let text = match mimz::translate::translate(&src, target) {
        Ok(t) => t,
        Err(diags) => return Output::Human(Flavor::English).one_file(&diags, &src, &path_str),
    };
    let out_path = output.unwrap_or_else(|| path.to_path_buf());
    // Write atomically: a sibling temp file then rename over the target, so an
    // interrupted or failing write can never truncate the file being formatted
    // (the common case is `fmt` overwriting its own input in place). The temp
    // name carries the PID so concurrent `fmt` runs don't collide.
    let mut tmp = out_path.clone().into_os_string();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    if let Err(e) = std::fs::write(&tmp, &text) {
        eprintln!("error: cannot write `{}`: {e}", tmp.display());
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::rename(&tmp, &out_path) {
        let _ = std::fs::remove_file(&tmp);
        eprintln!("error: cannot write `{}`: {e}", out_path.display());
        return ExitCode::FAILURE;
    }
    println!(
        "formatted {} ({})",
        out_path.display(),
        mimz::translate::flavor_name(target)
    );
    if mixed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
