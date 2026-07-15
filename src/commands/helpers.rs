//! Shared helpers used by more than one command handler: config + error-language
//! resolution and project-wide warning collection.
//!
//! The `name=value` / `--sweep` / trace-scope parsers now live in the library
//! (`mimz::runner`, re-exported as `mimz::…`) so the CLI and the browser
//! playground share one implementation. They are re-exported here so the command
//! handlers keep importing them from `super::helpers` unchanged.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mimz::lexer::token::Flavor;
use mimz::project::LoadedFile;
use mimz::sim::elaborate::SimMode;
use mimz::{diag, lexer, morph, project};

// Argument parsers — single source in the library (`mimz::runner`).
pub(crate) use mimz::{parse_bindings, parse_sweep, parse_u128, sweep_vectors, trace_scope};

/// Milliseconds elapsed since `start`, as f64 (for the `--debug` timing lines
/// printed by `check` and `compile`).
pub(crate) fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

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

/// The on-disk standard-library directory configured by `[lib] std`, made
/// absolute relative to the governing `mimz.toml`. `None` when unset or no
/// config file exists. Errors in config resolution fall back to `None` (the
/// command re-resolves and reports config errors on its own path).
pub(crate) fn lib_std_dir(input: &Path, explicit_config: Option<&Path>) -> Option<PathBuf> {
    let (cfg, cfg_path) = mimz::config::Config::resolve_with_path(input, explicit_config).ok()?;
    let std = cfg.lib.std?;
    let base = cfg_path
        .as_deref()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| input.parent().map(Path::to_path_buf).unwrap_or_default());
    Some(base.join(std))
}

/// Parse `--extern-sim`/`mimz.toml`'s `extern_sim` into a [`SimMode`] for
/// `mimz sim`/`mimz test`. An unrecognized value (a typo) prints a warning
/// and falls back to `Warn` rather than hard-erroring — mirrors `main.rs`'s
/// `names_map` handling for the same "unrecognized config string" shape.
pub(crate) fn resolve_sim_mode(extern_sim: &str) -> SimMode {
    match extern_sim {
        "strict" => SimMode::Strict,
        "warn" => SimMode::Warn,
        other => {
            eprintln!(
                "warning: extern_sim = \"{other}\" is not recognized — use \"warn\" or \"strict\"; assuming \"warn\""
            );
            SimMode::Warn
        }
    }
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
