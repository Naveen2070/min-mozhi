//! `mimz lint` — run style and hygiene lint passes over a file and its imports.
//! All diagnostics are warnings (the command exits 0 unless loading fails).
//! Checks naming conventions, unused signals, width consistency, and more.

use std::path::Path;
use std::process::ExitCode;

use mimz::{ast, lint, project};

use super::helpers::{project_warnings, resolve_lang};
use crate::Output;

/// `mimz lint` — run style/hygiene lint passes over a file and its imports.
/// All diagnostics are warnings — the command always exits 0 unless loading
/// or lexing fails.
pub(crate) fn lint_file(
    path: &Path,
    json: bool,
    lang: Option<&str>,
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

    let files = match project::load_project(path) {
        Ok(f) => f,
        Err(e) => {
            return out.load_error(&e);
        }
    };

    if debug {
        eprintln!("debug: loaded {} project file(s)", files.len());
        for f in &files {
            eprintln!("  - {}", f.path.display());
        }
    }

    // Collect project-level warnings (W0001 mixed-flavor) + lint warnings.
    let mut diags = project_warnings(&files);
    let asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    let lint_diags = lint::lint(&asts);
    diags.extend(lint_diags);

    if json {
        return out.project(&diags, &files);
    }

    if !diags.is_empty() {
        eprint!("{}", project::render_diags_lang(&diags, &files, flavor));
    }

    if !quiet {
        let modules = asts
            .iter()
            .flat_map(|f| &f.items)
            .filter(|i| matches!(i, ast::TopItem::Module(_)))
            .count();
        println!(
            "lint: {} — {modules} module(s), {} file(s), {} warning(s)",
            path.display(),
            files.len(),
            diags.len()
        );
    }

    ExitCode::SUCCESS
}
