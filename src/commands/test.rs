//! `mimz test <file>` — run `test "…" for M(…) { … }` blocks and report
//! pass/fail. Supports `--filter` for selective runs, `--trace` for per-cycle
//! console output. A failing `expect` prints a teaching-quality diff.

use std::io::IsTerminal;
use std::path::Path;
use std::process::ExitCode;

use mimz::ast::{self, TopItem};
use mimz::project;
use mimz::sim::harness::{TestResult, run_test_with_mode};
use mimz::sim::trace;

use super::helpers::{lib_std_dir, project_warnings, resolve_lang, resolve_sim_mode, trace_scope};
use crate::Output;

/// `mimz test <file>` — run the file's `test "…" for M(…) { … }` blocks and
/// report pass/fail. A failing `expect` prints a teaching-quality message (the
/// expression, the cycle, each side's value) and the command exits non-zero.
/// `--filter <substr>` runs only tests whose name contains `<substr>`;
/// `--trace` / `--trace=changes` (with `--verbose` / `--signals`) show a
/// per-cycle console trace for each test, off by default.
#[allow(clippy::too_many_arguments)]
pub(crate) fn test_file(
    path: &Path,
    filter: Option<String>,
    trace_style: Option<String>,
    verbose: bool,
    signals: Option<String>,
    extern_sim: &str,
    lang: Option<&str>,
    config_path: Option<&Path>,
    quiet: bool,
    debug: bool,
    emulate: bool,
    step: bool,
) -> ExitCode {
    use owo_colors::OwoColorize;

    let flavor = match resolve_lang(path, lang) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let out = Output::Human(flavor);
    if debug {
        eprintln!(
            "debug: loading project starting from entry {}",
            path.display()
        );
    }
    // Load imports too, so a module-under-test that instantiates a sub-module
    // from another file can be flattened.
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
        for f in &files {
            eprintln!("  - {}", f.path.display());
        }
    }
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
        if !quiet {
            match &filter {
                Some(f) => println!("no `test` blocks match `{f}` in {path_str}"),
                None => println!("no `test` blocks in {path_str}"),
            }
        }
        return ExitCode::SUCCESS;
    }

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let use_color = mimz::diag::is_color_enabled();

    // `live` gates real-time pacing/redraw: only when the caller asked for it
    // (`--emulate` or `--step`) AND stdout is an actual terminal (never in
    // CI/piped output). Computed once for the whole file, not per test.
    let is_tty = std::io::stdout().is_terminal();
    let live = (emulate || step) && is_tty;
    let mode = resolve_sim_mode(extern_sim);

    for t in tests {
        // Constructed unconditionally (even headless) so bind validation
        // always runs; a non-live host just no-ops every draw/pause.
        let host: Box<dyn mimz_sim::sim::EmulationHost> = Box::new(
            mimz::emulate::host::EmulateHost::new(t.name.clone(), live, step),
        );
        match run_test_with_mode(
            &asts,
            &src,
            t,
            host,
            live,
            step,
            trace_style.is_some(),
            mode,
        ) {
            Ok(o) => {
                let quit = o.quit;
                match &o.result {
                    TestResult::Pass => {
                        passed += 1;
                        if !quiet {
                            let s = if o.checks == 1 { "check" } else { "checks" };
                            let ok_str = if use_color {
                                "ok".green().bold().to_string()
                            } else {
                                "ok".to_string()
                            };
                            println!("{ok_str}   {} ({} {s})", o.name, o.checks);
                        }
                    }
                    TestResult::Fail(m) => {
                        failed += 1;
                        let fail_str = if use_color {
                            "FAIL".red().bold().to_string()
                        } else {
                            "FAIL".to_string()
                        };
                        println!("{fail_str} {}", o.name);
                        for line in m.lines() {
                            println!("       {line}");
                        }
                    }
                    TestResult::Skipped(reason) => {
                        skipped += 1;
                        if !quiet {
                            let skip_str = if use_color {
                                "SKIP".yellow().bold().to_string()
                            } else {
                                "SKIP".to_string()
                            };
                            println!("{skip_str} {} — {reason}", o.name);
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
                if quit {
                    if !quiet {
                        println!("(stepping aborted — remaining tests in this file skipped)");
                    }
                    break;
                }
            }
            Err(e) => {
                failed += 1;
                let fail_str = if use_color {
                    "FAIL".red().bold().to_string()
                } else {
                    "FAIL".to_string()
                };
                eprintln!("{fail_str} error in test \"{}\": {e}", t.name);
            }
        }
    }

    if !quiet || failed > 0 {
        let skip_suffix = if skipped > 0 {
            format!(", {skipped} skipped")
        } else {
            String::new()
        };
        println!("\n{passed} passed, {failed} failed{skip_suffix}");
    }
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
