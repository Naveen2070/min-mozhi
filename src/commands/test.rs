// ------------------------------------------------------------------ test

use std::path::Path;
use std::process::ExitCode;

use mimz::ast::{self, TopItem};
use mimz::project;
use mimz::sim::harness::{TestResult, run_test};
use mimz::sim::trace;

use super::helpers::{project_warnings, resolve_lang, trace_scope};
use crate::Output;

/// `mimz test <file>` — run the file's `test "…" for M(…) { … }` blocks and
/// report pass/fail. A failing `expect` prints a teaching-quality message (the
/// expression, the cycle, each side's value) and the command exits non-zero.
/// `--filter <substr>` runs only tests whose name contains `<substr>`;
/// `--trace` / `--trace=changes` (with `--verbose` / `--signals`) show a
/// per-cycle console trace for each test, off by default.
pub(crate) fn test_file(
    path: &Path,
    filter: Option<String>,
    trace_style: Option<String>,
    verbose: bool,
    signals: Option<String>,
    lang: Option<&str>,
) -> ExitCode {
    let flavor = match resolve_lang(path, lang) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let out = Output::Human(flavor);
    // Load imports too, so a module-under-test that instantiates a sub-module
    // from another file can be flattened.
    let files = match project::load_project(path) {
        Ok(f) => f,
        Err(e) => return out.load_error(&e),
    };
    let asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    let warnings = project_warnings(&files);
    if !warnings.is_empty() {
        eprint!("{}", project::render_diags_lang(&warnings, &files, flavor));
    }
    let path_str = path.display().to_string();
    let src = files[0].src.clone();

    let tests: Vec<_> = asts[0]
        .items
        .iter()
        .filter_map(|i| match i {
            TopItem::Test(t) => Some(t),
            _ => None,
        })
        .filter(|t| filter.as_deref().is_none_or(|f| t.name.contains(f)))
        .collect();

    if tests.is_empty() {
        match &filter {
            Some(f) => println!("no `test` blocks match `{f}` in {path_str}"),
            None => println!("no `test` blocks in {path_str}"),
        }
        return ExitCode::SUCCESS;
    }

    let mut passed = 0usize;
    let mut failed = 0usize;
    for t in tests {
        match run_test(&asts, &src, t) {
            Ok(o) => {
                match &o.result {
                    TestResult::Pass => {
                        passed += 1;
                        let s = if o.checks == 1 { "check" } else { "checks" };
                        println!("ok   {} ({} {s})", o.name, o.checks);
                    }
                    TestResult::Fail(m) => {
                        failed += 1;
                        println!("FAIL {}", o.name);
                        for line in m.lines() {
                            println!("       {line}");
                        }
                    }
                }
                // Per-test console trace (opt-in).
                if let Some(style) = &trace_style {
                    let all: Vec<String> =
                        o.timeline.signals.iter().map(|s| s.name.clone()).collect();
                    match trace_scope(
                        &all,
                        &o.default_scope,
                        verbose,
                        &signals,
                        &o.timeline.module,
                    ) {
                        Ok(scope) => print!("{}", trace::render(&o.timeline, style, &scope)),
                        Err(e) => {
                            eprintln!("error: {e}");
                            return ExitCode::FAILURE;
                        }
                    }
                }
            }
            Err(e) => {
                failed += 1;
                eprintln!("error in test \"{}\": {e}", t.name);
            }
        }
    }

    println!("\n{passed} passed, {failed} failed");
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
