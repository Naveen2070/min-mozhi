//! Shared helpers used by more than one command handler: config + error-language
//! resolution and project-wide warning collection.
//!
//! The `name=value` / `--sweep` / trace-scope parsers now live in the library
//! (`mimz::runner`, re-exported as `mimz::…`) so the CLI and the browser
//! playground share one implementation. They are re-exported here so the command
//! handlers keep importing them from `super::helpers` unchanged.

use std::path::Path;
use std::process::ExitCode;

use mimz::lexer::token::Flavor;
use mimz::project::LoadedFile;
use mimz::{diag, lexer, morph, project};

// Argument parsers — single source in the library (`mimz::runner`).
pub(crate) use mimz::{parse_bindings, parse_sweep, parse_u128, sweep_vectors, trace_scope};

/// Resolve the `mimz.toml` governing `input` (explicit `--config` wins, else
/// walk up from the file), turning a parse error into a printed message + the
/// failing exit code.
pub(crate) fn resolve_config(
    input: &Path,
    explicit: Option<&Path>,
) -> Result<mimz::config::Config, ExitCode> {
    mimz::config::Config::resolve(input, explicit).map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    })
}

/// The error-message language for a command: an explicit `--lang` (validated,
/// erroring with `ExitCode::FAILURE` on an unknown value) else the entry file's
/// predominant keyword flavor (`morph::majority_flavor`). Majority detection is
/// best-effort — the command re-reads and reports any real I/O / lex failure
/// itself, so a file that cannot be read here simply defaults to English.
pub(crate) fn resolve_lang(path: &Path, lang: Option<&str>) -> Result<Flavor, ExitCode> {
    if let Some(s) = lang {
        return morph::parse_lang(s).ok_or_else(|| {
            eprintln!("error: unknown language `{s}` — expected english, tanglish, or tamil");
            ExitCode::FAILURE
        });
    }
    Ok(project::read_source(path)
        .ok()
        .and_then(|src| lexer::lex(&src).ok())
        .map(|toks| morph::majority_flavor(&toks))
        .unwrap_or(Flavor::English))
}

/// Non-fatal warnings for a loaded project (currently just the mixed-flavor
/// lint, W0001), each tagged with its file index. Re-lexes each already-loaded
/// source — cheap, and it lexed clean during `load_project`.
pub(crate) fn project_warnings(files: &[LoadedFile]) -> Vec<diag::Diag> {
    files
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            lexer::lex(&f.src)
                .ok()
                .and_then(|toks| morph::flavor_mix_warning(&toks))
                .map(|d| d.with_file(i))
        })
        .collect()
}
