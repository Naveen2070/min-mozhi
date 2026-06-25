//! Safety-contract verification: every diagnostic fixture must contain its
//! declared code with a help line, and every example must compile clean
//! (no false positives from the checker).

use std::path::Path;

use mimz::project::LoadError;
use mimz::{ast, checker, project};

use super::{Rate, Safety, all_example_files, fixtures};

/// The safety contract, via the lib (no subprocess): every fixture's
/// diagnostics contain its declared code WITH a help line, and every
/// example checks clean (the false-positive guard).
pub fn measure_safety() -> Safety {
    let mut failures = Vec::new();
    let mut fixture_rate = Rate {
        passed: 0,
        total: 0,
    };
    let mut help_rate = Rate {
        passed: 0,
        total: 0,
    };
    for (path, code) in fixtures() {
        fixture_rate.total += 1;
        help_rate.total += 1;
        let diags = check_diags(&path);
        let hit = diags.iter().find(|d| d.code == Some(code.as_str()));
        match hit {
            Some(d) => {
                fixture_rate.passed += 1;
                if d.help.is_some() {
                    help_rate.passed += 1;
                } else {
                    failures.push(format!(
                        "{code} fired without a help line: {}",
                        file_name(&path)
                    ));
                }
            }
            None => failures.push(format!(
                "fixture expected {code}, got [{}]: {}",
                diags
                    .iter()
                    .map(|d| d.code.unwrap_or("E????"))
                    .collect::<Vec<_>>()
                    .join(", "),
                file_name(&path)
            )),
        }
    }

    let mut clean = Rate {
        passed: 0,
        total: 0,
    };
    for path in all_example_files() {
        clean.total += 1;
        if check_diags(&path).is_empty() {
            clean.passed += 1;
        } else {
            failures.push(format!("false positive on example: {}", path.display()));
        }
    }

    Safety {
        fixtures: fixture_rate,
        help_lines: help_rate,
        clean_examples: clean,
        failures,
    }
}

/// All diagnostics for one file: load errors (lexer/parser) or checker
/// errors — empty means it checks clean.
fn check_diags(path: &Path) -> Vec<mimz::diag::Diag> {
    match project::load_project(path) {
        Ok(files) => {
            let asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
            checker::check(&asts).err().unwrap_or_default()
        }
        Err(LoadError::Source { diags, .. }) => diags,
        Err(LoadError::Io(_)) => Vec::new(),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
