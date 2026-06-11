//! The checker — semantic safety passes between parse and emit (Phase 1
//! work item 4). First slice: project symbol tables + duplicate detection
//! (`symbols.rs`), const evaluation (`consteval.rs`), name resolution +
//! module-structure rules (`names.rs`). Width rules, single-driver,
//! exhaustiveness, and clock ownership land as later slices.
//!
//! Every checker diagnostic carries a stable error code (`E0101`) and the
//! index of the file it points into, so multi-file errors render against
//! the right source (`project::render_diags`). The code catalog lives in
//! docs/code/11-checker.md — new codes are added there in the same commit.

mod consteval;
mod names;
mod symbols;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::ast;
use crate::diag::Diag;
use crate::span::Span;

/// Run all checker passes over a loaded project (entry file first, same
/// order as `project::load_project`). Collects ALL diagnostics — the
/// checker never stops at the first error.
pub fn check(files: &[ast::File]) -> Result<(), Vec<Diag>> {
    let mut ck = Checker::new(files);
    ck.build_symbols(); // project tables + project-wide duplicates
    ck.eval_consts(); // file-level consts, top to bottom
    ck.resolve_names(); // every name points at a declaration
    if ck.diags.is_empty() {
        Ok(())
    } else {
        Err(ck.diags)
    }
}

/// Shared state for one checker run. `mod.rs` owns the struct and the
/// diagnostic plumbing; each pass lives in its own file as a
/// `pub(super)` impl (house pattern, see docs/code/03-parser.md).
pub(super) struct Checker<'a> {
    files: &'a [ast::File],
    /// module name -> (declaring file, node). Project-wide (spec/02
    /// section 1.5: module names are unique across the project).
    modules: HashMap<String, (usize, &'a ast::Module)>,
    /// file-level enum name -> (declaring file, node). Project-wide —
    /// imports bring a file's enums into scope.
    enums: HashMap<String, (usize, &'a ast::EnumDecl)>,
    /// Per file: const name -> evaluated value (consts are file-local;
    /// imports do NOT bring consts into scope).
    file_consts: Vec<HashMap<String, i128>>,
    diags: Vec<Diag>,
}

impl<'a> Checker<'a> {
    fn new(files: &'a [ast::File]) -> Self {
        Checker {
            files,
            modules: HashMap::new(),
            enums: HashMap::new(),
            file_consts: vec![HashMap::new(); files.len()],
            diags: Vec::new(),
        }
    }

    /// Record one error. Every checker error has a code, a file, and a
    /// help line — the teaching contract (spec/01 G1) is not optional.
    pub(super) fn err(
        &mut self,
        file: usize,
        span: Span,
        code: &'static str,
        msg: impl Into<String>,
        help: impl Into<String>,
    ) {
        self.diags.push(
            Diag::new(span, msg)
                .with_code(code)
                .with_file(file)
                .with_help(help),
        );
    }
}
