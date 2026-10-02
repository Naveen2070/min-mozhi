//! `mimz ir <file>` — lower a module to the Min-Mozhi IR, run the optimizer,
//! and print the result. The IR pipeline's first CLI caller; see
//! `docs/superpowers/specs/2026-10-01-ir-pipeline-cli-design.local.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mimz::ir::{self, Module};

use super::ir_pipeline::{PipelineOpts, lower_project};

/// `mimz ir <file>`: the shared pipeline (`ir_pipeline::lower_project`:
/// check -> elaborate -> lower -> validate -> optimize -> validate), then
/// print the IR text to stdout or `output`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ir_file(
    path: &Path,
    output: Option<PathBuf>,
    module: Option<String>,
    param: &str,
    no_opt: bool,
    sexpr: bool,
    panic: bool,
    stats: bool,
    lang: Option<&str>,
    config_path: Option<&Path>,
    quiet: bool,
    debug: bool,
) -> ExitCode {
    let l = match lower_project(&PipelineOpts {
        path,
        module,
        param,
        lang,
        config_path,
        no_opt,
        panic,
        keep_lowered: stats,
        debug,
    }) {
        Ok(l) => l,
        Err(code) => return code,
    };
    let m = l.module;
    if let Some(lowered) = &l.lowered {
        print_stats(lowered, l.rounds.map(|r| (&m, r)));
    }

    let text = if sexpr {
        ir::print_sexpr::print(&m)
    } else {
        ir::print_line::print(&m)
    };
    match output {
        Some(dest) => {
            if let Err(e) = std::fs::write(&dest, &text) {
                eprintln!("error: cannot write {}: {e}", dest.display());
                return ExitCode::FAILURE;
            }
            if !quiet {
                eprintln!("wrote {}", dest.display());
            }
        }
        None => print!("{text}"),
    }
    ExitCode::SUCCESS
}

/// Cells per kind, keyed by the `CellKind` variant name.
fn cell_counts(m: &Module) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for cell in &m.cells {
        // ponytail: the variant name is the Debug text up to its fields.
        let name: String = format!("{:?}", cell.kind)
            .chars()
            .take_while(|c| c.is_alphanumeric())
            .collect();
        *counts.entry(name).or_insert(0) += 1;
    }
    counts
}

/// The `--stats` table on stderr: one column as lowered, one optimized
/// (when the optimizer ran), then totals, nets and rounds.
fn print_stats(lowered: &Module, optimized: Option<(&Module, usize)>) {
    let before = cell_counts(lowered);
    let after = optimized.map(|(m, _)| cell_counts(m));
    let kinds: BTreeSet<&String> = before
        .keys()
        .chain(after.iter().flat_map(|a| a.keys()))
        .collect();
    let row = |label: &str, b: usize, a: Option<usize>| match a {
        Some(a) => eprintln!("{label:<12}{b:>8}{a:>11}"),
        None => eprintln!("{label:<12}{b:>8}"),
    };
    match &after {
        Some(_) => eprintln!("{:<12}{:>8}{:>11}", "cells", "lowered", "optimized"),
        None => eprintln!("{:<12}{:>8}", "cells", "lowered"),
    }
    for k in kinds {
        let a = after.as_ref().map(|a| a.get(k).copied().unwrap_or(0));
        row(k, before.get(k).copied().unwrap_or(0), a);
    }
    row(
        "total",
        lowered.cells.len(),
        optimized.map(|(m, _)| m.cells.len()),
    );
    row(
        "nets",
        lowered.nets.len(),
        optimized.map(|(m, _)| m.nets.len()),
    );
    if let Some((_, rounds)) = optimized {
        eprintln!("{:<12}{:>8}{rounds:>11}", "rounds", "");
    }
}
