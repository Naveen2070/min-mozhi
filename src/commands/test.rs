// ------------------------------------------------------------------ test

use std::path::Path;
use std::process::ExitCode;

use mimz::ast::TopItem;
use mimz::sim::harness::{TestResult, run_test};
use mimz::sim::trace;
use mimz::{diag, lexer, morph, parser, project};

use super::helpers::{resolve_lang, trace_scope};
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
        Err(diags) => return out.one_file(&diags, &src, &path_str),
    };
    if let Some(w) = morph::flavor_mix_warning(&tokens) {
        eprint!("{}", diag::render_lang(&[w], &src, &path_str, flavor));
    }
    let file = match parser::parse(tokens) {
        Ok(f) => f,
        Err(diags) => return out.one_file(&diags, &src, &path_str),
    };

    let tests: Vec<_> = file
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
        match run_test(&file, &src, t) {
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
