// -------------------------------------------------------------- compile

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mimz::{ast, checker, diag, emit_verilog, project};

use super::helpers::{project_warnings, resolve_lang};
use crate::Output;

/// `mimz compile` — load the entry file and all transitive imports, build
/// the project symbol table, and emit one Verilog file (default: entry
/// path with `.v` extension).
pub(crate) fn compile(
    path: &Path,
    output: Option<PathBuf>,
    emit_testbench: bool,
    json: bool,
    lang: Option<&str>,
) -> ExitCode {
    let flavor = match resolve_lang(path, lang) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let out = Output::new(json, flavor);
    let files = match project::load_project(path) {
        Ok(f) => f,
        Err(e) => return out.load_error(&e),
    };
    let mut asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    // Non-fatal warnings (W0001 mixed-flavor) ride alongside any stage errors,
    // and are surfaced on success too.
    let warnings = project_warnings(&files);
    let report_err = |errors: Vec<diag::Diag>| {
        let mut diags = warnings.clone();
        diags.extend(errors);
        out.project(&diags, &files)
    };

    if let Err(errors) = checker::check(&asts) {
        return report_err(errors);
    }

    // Tamil identifiers become readable ASCII (விளக்கு → villakku) —
    // checked against the original spelling above, emitted as Verilog
    // names below.
    emit_verilog::transliterate(&mut asts);

    let project = match emit_verilog::Project::from_files(&asts) {
        Ok(p) => p,
        Err(errors) => return report_err(errors),
    };
    let verilog = match emit_verilog::emit(&project, &asts) {
        Ok(v) => v,
        Err(errors) => return report_err(errors),
    };

    let out_path = output.unwrap_or_else(|| {
        let mut p = path.to_path_buf();
        p.set_extension("v");
        p
    });
    if let Err(e) = std::fs::write(&out_path, &verilog) {
        eprintln!("error: cannot write `{}`: {e}", out_path.display());
        return ExitCode::FAILURE;
    }

    if emit_testbench {
        let tests: Vec<&ast::TestDecl> = asts
            .iter()
            .flat_map(|f| {
                f.items.iter().filter_map(|i| {
                    if let ast::TopItem::Test(t) = i {
                        Some(t)
                    } else {
                        None
                    }
                })
            })
            .collect();

        if !tests.is_empty() {
            let tb_verilog = match emit_verilog::emit_testbench(&project, &tests) {
                Ok(v) => v,
                Err(errors) => return report_err(errors),
            };

            let mut tb_path = out_path.clone();
            let mut name = tb_path.file_stem().unwrap().to_os_string();
            name.push("_tb");
            tb_path.set_file_name(name);
            tb_path.set_extension("v");

            if let Err(e) = std::fs::write(&tb_path, &tb_verilog) {
                eprintln!("error: cannot write `{}`: {e}", tb_path.display());
                return ExitCode::FAILURE;
            }
            if !json {
                println!(
                    "compiled {} -> {} (testbench)",
                    path.display(),
                    tb_path.display()
                );
            }
        }
    }

    // Success: surface any non-fatal warnings (json → the array, else stderr).
    if json {
        out.project(&warnings, &files);
    } else {
        if !warnings.is_empty() {
            eprint!("{}", project::render_diags_lang(&warnings, &files, flavor));
        }
        println!("compiled {} -> {}", path.display(), out_path.display());
    }
    ExitCode::SUCCESS
}
