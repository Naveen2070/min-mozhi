// ---------------------------------------------------------------- check

use std::path::Path;
use std::process::ExitCode;

use mimz::{ast, checker, lexer, project};

use super::helpers::{lib_std_dir, project_warnings, resolve_lang};
use crate::Output;

/// `mimz check` — lex + parse + checker passes over the file AND its
/// imports (cross-file names must resolve), reporting all diagnostics.
/// With `--tokens` it stops after the lexer and dumps the token stream
/// instead (the standard way to debug lexer issues).
pub(crate) fn check(path: &Path, tokens: bool, json: bool, lang: Option<&str>) -> ExitCode {
    let flavor = match resolve_lang(path, lang) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let out = Output::new(json, flavor);
    if tokens {
        let src = match project::read_source(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        match lexer::lex(&src) {
            Ok(toks) => {
                for t in &toks {
                    println!("{:?} @ {}..{}", t.kind, t.span.start, t.span.end);
                }
            }
            Err(diags) => return out.one_file(&diags, &src, &path.display().to_string()),
        }
        return ExitCode::SUCCESS;
    }

    let lib_std = lib_std_dir(path, None);
    let files = match project::load_project_with_lib(path, lib_std.as_deref()) {
        Ok(f) => f,
        Err(e) => return out.load_error(&e),
    };
    let asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    // Non-fatal warnings (W0001 mixed-flavor) ride alongside any checker errors.
    let mut diags = project_warnings(&files);
    if let Err(errors) = checker::check(&asts) {
        diags.extend(errors);
    }
    let has_error = diags.iter().any(|d| d.is_error());
    if json {
        // Stable contract: stdout is ALWAYS a JSON array (warnings included, or
        // `[]`). Exit reflects severity.
        return out.project(&diags, &files);
    }
    if !diags.is_empty() {
        eprint!("{}", project::render_diags_lang(&diags, &files, flavor));
    }
    if has_error {
        return ExitCode::FAILURE;
    }
    let modules = asts
        .iter()
        .flat_map(|f| &f.items)
        .filter(|i| matches!(i, ast::TopItem::Module(_)))
        .count();
    let tests = asts
        .iter()
        .flat_map(|f| &f.items)
        .filter(|i| matches!(i, ast::TopItem::Test(_)))
        .count();
    println!(
        "OK: {} — {modules} module(s), {tests} test(s), {} file(s)",
        path.display(),
        files.len()
    );
    ExitCode::SUCCESS
}
