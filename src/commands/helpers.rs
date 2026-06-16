//! Shared helpers used by more than one command handler: config/language
//! resolution, project-wide warning collection, and the `name=value` binding
//! parsers (`eval`). Moved here verbatim from `main.rs` during the per-command
//! split — no logic changed.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use mimz::lexer::token::Flavor;
use mimz::project::LoadedFile;
use mimz::{diag, lexer, morph, project};

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

/// Parse `name=val,name=val` into a map, applying `val_parser` to each value.
/// An empty string is an empty map.
pub(crate) fn parse_bindings<T>(
    s: &str,
    val_parser: impl Fn(&str) -> Result<T, String>,
) -> Result<BTreeMap<String, T>, String> {
    let mut map = BTreeMap::new();
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (name, val) = part
            .split_once('=')
            .ok_or_else(|| format!("expected `name=value`, got `{part}`"))?;
        map.insert(name.trim().to_string(), val_parser(val.trim())?);
    }
    Ok(map)
}

/// Resolve the console-trace scope from the flags, shared by `sim` and `test`.
/// `--signals` (an explicit, validated subset) overrides `--verbose` (all
/// signals), which overrides the default (interface + state). An unknown
/// `--signals` name is a clean error naming `module`.
pub(crate) fn trace_scope(
    all: &[String],
    default: &[String],
    verbose: bool,
    signals: &Option<String>,
    module: &str,
) -> Result<Vec<String>, String> {
    match signals {
        Some(list) => {
            let chosen: Vec<String> = list
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
            for s in &chosen {
                if !all.iter().any(|n| n == s) {
                    return Err(format!(
                        "--signals names `{s}`, which is not a signal of `{module}`"
                    ));
                }
            }
            Ok(chosen)
        }
        None if verbose => Ok(all.to_vec()),
        None => Ok(default.to_vec()),
    }
}

/// Parse a `u128` literal in decimal, `0x` hex, or `0b` binary.
pub(crate) fn parse_u128(s: &str) -> Result<u128, String> {
    let parsed = if let Some(hex) = s.strip_prefix("0x") {
        u128::from_str_radix(hex, 16)
    } else if let Some(bin) = s.strip_prefix("0b") {
        u128::from_str_radix(bin, 2)
    } else {
        s.parse::<u128>()
    };
    parsed.map_err(|_| format!("`{s}` is not a number (use decimal, 0x.., or 0b..)"))
}
