//! `mimz translate <file> [--to <flavor>] [--order code|thamizh]` — re-emit a
//! file in another keyword flavor and/or word order. With only `--to`, this is
//! a lossless keyword-only reskin (comments/layout preserved). With `--order`,
//! it parses to the AST and pretty-prints — canonical layout, comments dropped.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mimz::lexer::token::Flavor;
use mimz::{diag, project};

use crate::Output;

/// `mimz translate <file> [--to <flavor>] [--order code|thamizh]` — re-emit the
/// file in another keyword flavor and/or word order (default: stdout).
///
/// `--to` defaults to english. With no `--order`, this is the lossless
/// keyword-only reskin (comments/layout preserved). With `--order`, it parses
/// to the AST and pretty-prints in the requested order — canonical layout,
/// comments dropped (spec/04: trivia-preservation is the `--to` path).
#[allow(clippy::too_many_arguments)]
pub(crate) fn translate_file(
    path: &Path,
    to: Option<&str>,
    order: Option<&str>,
    romanize_names: bool,
    names_map: Option<&Path>,
    auto_names_map: bool,
    output: Option<PathBuf>,
    quiet: bool,
    debug: bool,
) -> ExitCode {
    if debug {
        eprintln!("debug: translating file {}", path.display());
    }
    let to = to.unwrap_or("english");
    let Some(flavor) = mimz::translate::parse_flavor(to) else {
        eprintln!("error: unknown flavor `{to}` — expected english, tanglish, or tamil");
        return ExitCode::FAILURE;
    };
    // `--romanize-names` (Tamil -> Latin) and `--names-map` (Latin -> Tamil) are
    // opposite directions; running both at once is a mistake, not a no-op.
    if romanize_names && names_map.is_some() {
        eprintln!(
            "error: --romanize-names and --names-map are opposite directions — use one (romanize to Latin, or restore Tamil from a map)"
        );
        return ExitCode::FAILURE;
    }
    if romanize_names && order.is_some() {
        eprintln!(
            "warning: --romanize-names applies to the keyword-only reskin; \
             ignored with --order"
        );
    }
    if names_map.is_some() && order.is_some() {
        eprintln!("warning: --names-map applies to the keyword-only reskin; ignored with --order");
    }
    let order = match order {
        None => None,
        Some("code") => Some(mimz::pretty::Order::Code),
        Some("thamizh") => Some(mimz::pretty::Order::Thamizh),
        Some(other) => {
            eprintln!("error: unknown order `{other}` — expected code or thamizh");
            return ExitCode::FAILURE;
        }
    };
    let src = match project::read_source(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Resolve the name-map for a reverse reskin: an explicit `--names-map` wins;
    // otherwise auto-discover the `<input>.names.json` sidecar (when present and
    // not disabled). Only the plain token reskin restores names — never `--order`
    // (AST path) or a forward `--romanize-names`. The bool records whether it was
    // auto-discovered, so we can tell the user.
    let names_for_restore: Option<(PathBuf, bool)> = if order.is_some() || romanize_names {
        None
    } else if let Some(p) = names_map {
        Some((p.to_path_buf(), false))
    } else if auto_names_map {
        let sidecar = names_sidecar(path);
        sidecar.is_file().then_some((sidecar, true))
    } else {
        None
    };

    // Produce the translated text. `--order` goes through the AST pretty-printer
    // (it can reorder clause heads); `--to` alone uses the trivia-preserving
    // token reskin. On a forward romanize, `captured_map` holds the sidecar.
    let render_err = |diags: &[diag::Diag]| {
        Output::Human(Flavor::English).one_file(diags, &src, &path.display().to_string())
    };
    let mut captured_map: Option<mimz::translate::NameMap> = None;
    let text = match order {
        Some(order) => {
            let tokens = match mimz::lexer::lex(&src) {
                Ok(t) => t,
                Err(diags) => return render_err(&diags),
            };
            match mimz::parser::parse(tokens) {
                Ok(file) => mimz::pretty::pretty_print(&file, flavor, order),
                Err(diags) => return render_err(&diags),
            }
        }
        None if names_for_restore.is_some() => {
            let (map_path, auto) = names_for_restore.as_ref().unwrap();
            let map = match load_name_map(map_path) {
                Ok(m) => m,
                Err(code) => return code,
            };
            if *auto && !quiet {
                eprintln!(
                    "note: restoring names from {} (auto-discovered; --no-names-map to disable)",
                    map_path.display()
                );
            }
            match mimz::translate::restore_with_map(&src, flavor, &map) {
                Ok(t) => t,
                Err(diags) => return render_err(&diags),
            }
        }
        None if romanize_names => match mimz::translate::romanize_with_map(&src, flavor) {
            Ok((t, map)) => {
                captured_map = Some(map);
                t
            }
            Err(diags) => return render_err(&diags),
        },
        None => match mimz::translate::translate(&src, flavor) {
            Ok(t) => t,
            Err(diags) => return render_err(&diags),
        },
    };

    match output {
        Some(out_path) => {
            if let Err(e) = std::fs::write(&out_path, &text) {
                eprintln!("error: cannot write `{}`: {e}", out_path.display());
                return ExitCode::FAILURE;
            }
            // Forward romanize with names to record: write the sidecar beside the
            // output (`<out>.names.json`) so the run is reversible via --names-map.
            if let Some(map) = captured_map.filter(|m| !m.names.is_empty()) {
                let sidecar = names_sidecar(&out_path);
                let json = match serde_json::to_string_pretty(&map) {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!("error: cannot serialize name map: {e}");
                        return ExitCode::FAILURE;
                    }
                };
                if let Err(e) = std::fs::write(&sidecar, json) {
                    eprintln!("error: cannot write name map `{}`: {e}", sidecar.display());
                    return ExitCode::FAILURE;
                }
                if !quiet {
                    println!("wrote name map {}", sidecar.display());
                }
            }
            if !quiet {
                println!(
                    "translated {} -> {} ({})",
                    path.display(),
                    out_path.display(),
                    mimz::translate::flavor_name(flavor)
                );
            }
            ExitCode::SUCCESS
        }
        None => {
            if !quiet && captured_map.is_some_and(|m| !m.names.is_empty()) {
                eprintln!(
                    "note: --romanize-names without -o does not write a name map — the round-trip back to Tamil won't be reversible"
                );
            }
            print!("{text}");
            ExitCode::SUCCESS
        }
    }
}

/// The sidecar path for a translate output: `<out>.names.json` (append, so
/// `foo.mimz` → `foo.mimz.names.json`, never replacing the existing extension).
fn names_sidecar(out: &Path) -> PathBuf {
    let mut name = out.as_os_str().to_owned();
    name.push(".names.json");
    PathBuf::from(name)
}

/// Read + parse a `--names-map` file into a [`mimz::translate::NameMap`], or print
/// a clean error and return the failing exit code.
fn load_name_map(path: &Path) -> Result<mimz::translate::NameMap, ExitCode> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read name map `{}`: {e}", path.display());
        ExitCode::FAILURE
    })?;
    let map: mimz::translate::NameMap = serde_json::from_str(&text).map_err(|e| {
        eprintln!("error: invalid name map `{}`: {e}", path.display());
        ExitCode::FAILURE
    })?;
    // Honor the format version: reject a map written by a newer/unknown format
    // rather than silently mis-restoring (the `version` field exists for this).
    if map.version != mimz::translate::NameMap::VERSION {
        eprintln!(
            "error: name map `{}` is format version {}, but this mimz understands version {} — regenerate it with --romanize-names",
            path.display(),
            map.version,
            mimz::translate::NameMap::VERSION
        );
        return Err(ExitCode::FAILURE);
    }
    Ok(map)
}
