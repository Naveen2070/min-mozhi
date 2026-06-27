//! The checker — semantic safety passes between parse and emit (Phase 1
//! work item 4). Current slices: project symbol tables + duplicate
//! detection (`symbols.rs`), const evaluation (`consteval.rs`), name
//! resolution + module-structure rules incl. instantiation completeness
//! (`names.rs`), width/type rules + match exhaustiveness (`widths/`),
//! single-driver + combinational-cycle rules (`drivers.rs`), and
//! clock-domain ownership (`clocks.rs`).
//!
//! Every checker diagnostic carries a stable error code (`E0101`) and the
//! index of the file it points into, so multi-file errors render against
//! the right source (`project::render_diags`). The code catalog lives in
//! docs/code/11-checker.md — new codes are added there in the same commit.

mod clocks;
pub(crate) mod consteval;
mod drivers;
mod names;
mod symbols;
#[cfg(test)]
mod tests;
mod widths;

use std::collections::HashMap;
use std::rc::Rc;

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
    ck.check_widths(); // every expression has the width its context needs
    ck.check_drivers(); // one driver per signal; combinational graph is a DAG
    ck.check_clocks(); // every reg owned by one clock; no cross-domain reads
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
    /// file-level function name -> (declaring file, node). Project-wide —
    /// function names are unique across the whole project (spec/02).
    funcs: HashMap<String, (usize, &'a ast::FuncDecl)>,
    /// Per file: const name -> evaluated value (consts are file-local;
    /// imports do NOT bring consts into scope).
    file_consts: Vec<HashMap<String, i128>>,
    /// module name -> its name table, built by pass 3 (`names.rs`) and
    /// reused by pass 4 (`widths.rs`). `Rc` so a pass can hold the scope
    /// while still reporting through `&mut self`.
    scopes: HashMap<String, Rc<names::Scope<'a>>>,
    diags: Vec<Diag>,
}

impl<'a> Checker<'a> {
    fn new(files: &'a [ast::File]) -> Self {
        Checker {
            files,
            modules: HashMap::new(),
            enums: HashMap::new(),
            funcs: HashMap::new(),
            file_consts: vec![HashMap::new(); files.len()],
            scopes: HashMap::new(),
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

    /// Like [`Self::err`], but also attaches structured `(token, value)` args for
    /// the localized catalog (`morph::fill` interpolates them into a localized
    /// template; the English `msg` already contains the same values). Pass the
    /// SAME values you `format!`'d into `msg`, under the token names the
    /// `messages.toml` template uses.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn err_args(
        &mut self,
        file: usize,
        span: Span,
        code: &'static str,
        msg: impl Into<String>,
        help: impl Into<String>,
        args: Vec<(&'static str, String)>,
    ) {
        let mut d = Diag::new(span, msg)
            .with_code(code)
            .with_file(file)
            .with_help(help);
        d.args = args;
        self.diags.push(d);
    }
}
