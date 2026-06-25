//! `mimz compile` — load the entry file and all transitive imports, build the
//! project symbol table, and emit one Verilog file (default: entry path with
//! `.v` extension). Optionally emits a self-checking testbench (`--testbench`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mimz::{ast, checker, diag, emit_verilog, project};

use super::helpers::{lib_std_dir, project_warnings, resolve_lang};
use crate::Output;

/// `mimz compile` — load the entry file and all transitive imports, build
/// the project symbol table, and emit one Verilog file (default: entry
/// path with `.v` extension).
#[allow(clippy::too_many_arguments)]
pub(crate) fn compile(
    path: &Path,
    output: Option<PathBuf>,
    emit_testbench: bool,
    json: bool,
    lang: Option<&str>,
    config_path: Option<&Path>,
    quiet: bool,
    debug: bool,
) -> ExitCode {
    let flavor = match resolve_lang(path, lang) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let out = Output::new(json, flavor);
    if debug {
        eprintln!(
            "debug: loading project starting from entry {}",
            path.display()
        );
    }
    // Phase timing (--debug): `load` fuses lex+parse+import-resolution, `check`
    // is the six checker passes, `emit` is transliterate + lower + Verilog text.
    // For a finer lex-vs-parse split, see the criterion harness (`cargo bench`).
    let t_load = Instant::now();
    let lib_std = lib_std_dir(path, config_path);
    let files = match project::load_project_with_lib(path, lib_std.as_deref()) {
        Ok(f) => f,
        Err(e) => return out.load_error(&e),
    };
    let load_ms = ms(t_load);
    if debug {
        eprintln!("debug: loaded {} project file(s)", files.len());
        for f in &files {
            eprintln!("  - {}", f.path.display());
        }
    }

    let mut asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    // Non-fatal warnings (W0001 mixed-flavor) ride alongside any stage errors,
    // and are surfaced on success too.
    let warnings = project_warnings(&files);
    let report_err = |errors: Vec<diag::Diag>| {
        let mut diags = warnings.clone();
        diags.extend(errors);
        out.project(&diags, &files)
    };

    let t_check = Instant::now();
    if let Err(errors) = checker::check(&asts) {
        return report_err(errors);
    }
    let check_ms = ms(t_check);

    // Tamil identifiers become readable ASCII (விளக்கு → villakku) —
    // checked against the original spelling above, emitted as Verilog
    // names below.
    let t_emit = Instant::now();
    emit_verilog::transliterate(&mut asts);

    let project = match emit_verilog::Project::from_files(&asts) {
        Ok(p) => p,
        Err(errors) => return report_err(errors),
    };
    let verilog = match emit_verilog::emit(&project, &asts) {
        Ok(v) => v,
        Err(errors) => return report_err(errors),
    };
    let emit_ms = ms(t_emit);
    if debug {
        eprintln!(
            "debug: timing — load {load_ms:.3}ms, check {check_ms:.3}ms, emit {emit_ms:.3}ms (total {:.3}ms)",
            load_ms + check_ms + emit_ms
        );
    }

    let out_path = output.unwrap_or_else(|| {
        let mut p = path.to_path_buf();
        p.set_extension("v");
        p
    });

    // Build the testbench (if requested) BEFORE writing any file, so a
    // testbench-emission error or an unusable output path aborts the command
    // cleanly instead of leaving a stray `.v` with no companion `_tb.v`.
    let mut testbench: Option<(PathBuf, String)> = None;
    let mut no_tests = false;
    if emit_testbench {
        let tests: Vec<&ast::TestDecl> = asts
            .iter()
            .flat_map(|f| {
                f.items.iter().filter_map(|i| match i {
                    ast::TopItem::Test(t) => Some(t),
                    _ => None,
                })
            })
            .collect();

        if tests.is_empty() {
            no_tests = true;
        } else {
            let tb_verilog = match emit_verilog::emit_testbench(&project, &tests) {
                Ok(v) => v,
                Err(errors) => return report_err(errors),
            };
            // `<out>.v` -> `<out>_tb.v`. A path with no file stem (e.g.
            // `--output ..`) can't yield a testbench name — fail cleanly.
            let Some(stem) = out_path.file_stem() else {
                eprintln!(
                    "error: cannot derive a testbench file name from `{}`",
                    out_path.display()
                );
                return ExitCode::FAILURE;
            };
            let mut name = stem.to_os_string();
            name.push("_tb");
            let mut tb_path = out_path.clone();
            tb_path.set_file_name(name);
            tb_path.set_extension("v");
            testbench = Some((tb_path, tb_verilog));
        }
    }

    if let Err(e) = std::fs::write(&out_path, &verilog) {
        eprintln!("error: cannot write `{}`: {e}", out_path.display());
        return ExitCode::FAILURE;
    }
    if let Some((tb_path, tb_verilog)) = &testbench {
        if let Err(e) = std::fs::write(tb_path, tb_verilog) {
            eprintln!("error: cannot write `{}`: {e}", tb_path.display());
            return ExitCode::FAILURE;
        }
    }

    // Success: surface any non-fatal warnings (json → the array, else stderr).
    if json {
        out.project(&warnings, &files);
    } else {
        if !warnings.is_empty() {
            eprint!("{}", project::render_diags_lang(&warnings, &files, flavor));
        }
        if !quiet {
            println!("compiled {} -> {}", path.display(), out_path.display());
            if let Some((tb_path, _)) = &testbench {
                println!(
                    "compiled {} -> {} (testbench)",
                    path.display(),
                    tb_path.display()
                );
            } else if no_tests {
                eprintln!(
                    "note: --emit-testbench had no effect — no `test` blocks found in {}",
                    path.display()
                );
            }
        }
    }
    ExitCode::SUCCESS
}

/// Milliseconds elapsed since `start`, as f64 (for the `--debug` timing line).
fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
