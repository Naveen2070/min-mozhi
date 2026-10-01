//! `mimz ir <file>` — lower a module to the Min-Mozhi IR, run the optimizer,
//! and print the result. The IR pipeline's first CLI caller; see
//! `docs/superpowers/specs/2026-10-01-ir-pipeline-cli-design.local.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mimz::diag::Diag;
use mimz::ir::failure::{self, Failure, FailureKind, Stage};
use mimz::ir::{self, Module};
use mimz::lexer::token::Flavor;
use mimz::project::LoadedFile;
use mimz::sim::elaborate;
use mimz::{ast, checker, project};

use super::helpers::{lib_std_dir, parse_bindings, parse_u128, project_warnings, resolve_lang};
use crate::Output;

/// `mimz ir <file>`: check -> elaborate -> lower -> validate -> optimize ->
/// validate -> print the IR text to stdout or `output`.
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
    let flavor = match resolve_lang(path, lang) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let out = Output::Human(flavor);
    let lib_std = match lib_std_dir(path, config_path) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let files = match project::load_project_with_lib(path, lib_std.as_deref()) {
        Ok(f) => f,
        Err(e) => return out.load_error(&e),
    };
    if debug {
        eprintln!("debug: loaded {} project file(s)", files.len());
    }
    let asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    // Same gate as `mimz sim`: never lower a program the checker rejects.
    let mut diags = project_warnings(&files);
    if let Err(errors) = checker::check(&asts) {
        diags.extend(errors);
    }
    if !diags.is_empty() {
        eprint!("{}", project::render_diags_lang(&diags, &files, flavor));
    }
    if diags.iter().any(|d| d.is_error()) {
        return ExitCode::FAILURE;
    }
    let params = match parse_bindings(param, |s| parse_u128(s).map(|v| v as i128)) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let design = match elaborate::elaborate_project_with_mode(
        &asts,
        module.as_deref(),
        &params,
        elaborate::SimMode::Lower,
    ) {
        Ok(d) => d,
        Err(e) => {
            eprint!(
                "{}",
                project::render_diags_lang(std::slice::from_ref(e.as_ref()), &files, flavor)
            );
            return ExitCode::FAILURE;
        }
    };
    if debug {
        eprintln!("debug: lowering module `{}`", design.module);
    }

    let want_backtrace = debug || panic;
    let mut m = match failure::catch(Stage::Lower, want_backtrace, || ir::lower(&design)) {
        Ok(m) => m,
        Err(f) => return report(f, &files, flavor, &design.module, panic, debug),
    };
    if let Err(code) = check_valid(&m, "lowered", panic) {
        return code;
    }
    let lowered = stats.then(|| m.clone());
    let mut rounds = None;
    if !no_opt {
        match failure::catch(Stage::Optimize, want_backtrace, || {
            ir::opt::optimize(&mut m)
        }) {
            Ok(r) => rounds = Some(r),
            Err(f) => return report(f, &files, flavor, &design.module, panic, debug),
        }
        if let Err(code) = check_valid(&m, "optimized", panic) {
            return code;
        }
    }
    if let Some(lowered) = &lowered {
        print_stats(lowered, rounds.map(|r| (&m, r)));
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

const INTERNAL_HELP: &str = "an invariant the checker should guarantee broke. This is a \
compiler bug; please report it with this output. Rerun with `-d` for a backtrace, or \
`--panic` to crash.";

const LIMITATION_HELP: &str = "IR limitation. The IR cannot lower this construct yet. \
`mimz compile` (Verilog) is unaffected. Tracked under GAP-1 in docs/audit/gaps.md.";

/// Panic messages can be a whole `Expr` Debug dump; keep the report readable.
fn short(msg: &str, debug: bool) -> String {
    match msg.char_indices().nth(240) {
        Some((i, _)) if !debug => format!("{}... (full text with -d)", &msg[..i]),
        _ => msg.to_string(),
    }
}

/// Prints a caught lowering/optimizer panic as a diagnostic, then exits 1,
/// or re-raises it under `--panic` when it is a compiler bug.
fn report(
    f: Failure,
    files: &[LoadedFile],
    flavor: Flavor,
    top: &str,
    panic: bool,
    debug: bool,
) -> ExitCode {
    let header = match (f.kind, f.stage) {
        (FailureKind::Limitation, _) => "IR lowering does not support this design yet",
        (FailureKind::Internal, Stage::Lower) => "internal compiler error in IR lowering",
        (FailureKind::Internal, Stage::Optimize) => "internal compiler error in the IR optimizer",
    };
    let help = match f.kind {
        FailureKind::Limitation => LIMITATION_HELP,
        FailureKind::Internal => INTERNAL_HELP,
    };
    // ponytail: a span has no file index, so it is only trusted in a
    // single-file project (gaps.md: multi-file spans).
    match f.span.filter(|_| files.len() == 1) {
        Some(span) => eprint!(
            "{}",
            project::render_diags_lang(&[Diag::new(span, header).with_help(help)], files, flavor)
        ),
        None => {
            eprintln!("error: {header}");
            eprintln!("  = help: {help}");
            let why = if files.len() > 1 {
                ": multi-file project (see docs/audit/gaps.md)"
            } else {
                ""
            };
            eprintln!("  top module: {top}; source location not shown{why}");
        }
    }
    eprintln!("  panic: {}", short(&f.message, debug));
    if let Some(loc) = &f.location {
        eprintln!("  at:    {loc}");
    }
    // `--panic` captures a backtrace for the crash it may re-raise; a
    // limitation stays a clean error, so only `-d` shows it one.
    if let Some(bt) = f
        .backtrace
        .as_ref()
        .filter(|_| debug || f.kind == FailureKind::Internal)
    {
        eprintln!("  backtrace:\n{bt}");
    }
    if panic && f.kind == FailureKind::Internal {
        f.resume();
    }
    ExitCode::FAILURE
}

/// `validate` after a stage: lowering and the optimizer both promise a
/// `validate`-clean module, so a failure here is a compiler bug.
fn check_valid(m: &Module, what: &str, panic: bool) -> Result<(), ExitCode> {
    let errs = ir::validate::validate(m);
    if errs.is_empty() {
        return Ok(());
    }
    if panic {
        panic!("{what} IR fails validate: {errs:?}");
    }
    eprintln!("error: internal compiler error: {what} IR fails validate");
    for e in &errs {
        eprintln!("  {e:?}");
    }
    eprintln!("  = help: {INTERNAL_HELP}");
    Err(ExitCode::FAILURE)
}
