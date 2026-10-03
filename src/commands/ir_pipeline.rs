//! The check -> elaborate -> lower -> validate -> optimize -> validate
//! pipeline shared by `mimz ir` and `mimz build`, with its failure reports
//! (IR limitation vs internal compiler error).

use std::path::Path;
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

/// What to lower and how.
pub(crate) struct PipelineOpts<'a> {
    pub(crate) path: &'a Path,
    pub(crate) module: Option<String>,
    pub(crate) param: &'a str,
    pub(crate) lang: Option<&'a str>,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) no_opt: bool,
    pub(crate) panic: bool,
    pub(crate) keep_lowered: bool,
    pub(crate) debug: bool,
}

/// A design lowered to the IR, with what a caller needs to report on it.
// ponytail: `files`/`flavor`/`top` are read by `mimz build` (synthesis v1
// phase 1, Task 6); drop this allow when it lands.
#[allow(dead_code)]
pub(crate) struct Lowered {
    pub(crate) files: Vec<LoadedFile>,
    pub(crate) flavor: Flavor,
    pub(crate) top: String,
    /// The module as lowered, before the optimizer (only with `keep_lowered`).
    pub(crate) lowered: Option<Module>,
    /// Lowered, validated, optimized (unless `no_opt`), validated again.
    pub(crate) module: Module,
    /// Optimizer rounds (`None` with `no_opt`).
    pub(crate) rounds: Option<usize>,
}

/// Loads, checks, elaborates and lowers `o.path`, then validates, optimizes
/// and validates again. Every failure has already been reported when this
/// returns `Err`.
pub(crate) fn lower_project(o: &PipelineOpts) -> Result<Lowered, ExitCode> {
    let flavor = resolve_lang(o.path, o.lang)?;
    let out = Output::Human(flavor);
    let lib_std = lib_std_dir(o.path, o.config_path)?;
    let files = match project::load_project_with_lib(o.path, lib_std.as_deref()) {
        Ok(f) => f,
        Err(e) => return Err(out.load_error(&e)),
    };
    if o.debug {
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
        return Err(ExitCode::FAILURE);
    }
    let params = parse_bindings(o.param, |s| parse_u128(s).map(|v| v as i128)).map_err(|e| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    })?;
    let design = elaborate::elaborate_project_with_mode(
        &asts,
        o.module.as_deref(),
        &params,
        elaborate::SimMode::Lower,
    )
    .map_err(|e| {
        eprint!(
            "{}",
            project::render_diags_lang(std::slice::from_ref(e.as_ref()), &files, flavor)
        );
        ExitCode::FAILURE
    })?;
    if o.debug {
        eprintln!("debug: lowering module `{}`", design.module);
    }

    let want_backtrace = o.debug || o.panic;
    let mut m = match failure::catch(Stage::Lower, want_backtrace, || ir::lower(&design)) {
        Ok(m) => m,
        Err(f) => return Err(report(f, &files, flavor, &design.module, o.panic, o.debug)),
    };
    check_valid(&m, "lowered", o.panic)?;
    let lowered = o.keep_lowered.then(|| m.clone());
    let mut rounds = None;
    if !o.no_opt {
        match failure::catch(Stage::Optimize, want_backtrace, || {
            ir::opt::optimize(&mut m)
        }) {
            Ok(r) => rounds = Some(r),
            Err(f) => return Err(report(f, &files, flavor, &design.module, o.panic, o.debug)),
        }
        check_valid(&m, "optimized", o.panic)?;
    }
    Ok(Lowered {
        files,
        flavor,
        top: design.module,
        lowered,
        module: m,
        rounds,
    })
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
pub(crate) fn report(
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
        (FailureKind::Internal, Stage::Emit) => "internal compiler error in the Verilog backend",
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
        eprintln!("  {e}");
    }
    eprintln!("  = help: {INTERNAL_HELP}");
    Err(ExitCode::FAILURE)
}
