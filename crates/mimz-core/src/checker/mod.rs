//! The checker — semantic safety passes between parse and emit (Phase 1
//! work item 4). Current slices: project symbol tables + duplicate
//! detection (`symbols.rs`), const evaluation (`consteval.rs`), name
//! resolution + module-structure rules incl. instantiation completeness
//! (`names.rs`), width/type rules + match exhaustiveness (`widths/`),
//! single-driver + combinational-cycle rules (`drivers.rs`),
//! ban-recursive-functions (`funcs.rs`), clock-domain ownership
//! (`clocks.rs`), and extern-module port-shape validation (`extern_module.rs`).
//!
//! Every checker diagnostic carries a stable error code (`E0101`) and the
//! index of the file it points into, so multi-file errors render against
//! the right source (`project::render_diags`). The code catalog lives in
//! docs/code/11-checker.md — new codes are added there in the same commit.

mod clocks;
pub mod consteval;
mod drivers;
mod extern_module;
mod funcs;
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
    ck.check_extern_modules(); // extern module ports must be scalar (E1302)
    ck.check_func_cycles(); // ban recursive fn call cycles (E0805)
    ck.check_func_unreachable(); // dead code after `return` (E0812)
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
    /// module name -> every (declaring file, node) sharing that name.
    /// Uniqueness is enforced per-file (symbols.rs); cross-file duplicates are
    /// legal and resolved (or flagged ambiguous) at use-site (names.rs).
    modules: HashMap<String, Vec<(usize, &'a ast::Module)>>,
    /// file-level enum name -> every (declaring file, node) sharing that name.
    enums: HashMap<String, Vec<(usize, &'a ast::EnumDecl)>>,
    /// file-level function name -> (declaring file, node). Project-wide —
    /// function names are unique across the whole project (spec/02) — OUT OF
    /// SCOPE for packages/namespacing (D-PKG-1); left untouched.
    funcs: HashMap<String, (usize, &'a ast::FuncDecl)>,
    /// file-level bundle name -> every (declaring file, node) sharing that name.
    bundles: HashMap<String, Vec<(usize, &'a ast::BundleDecl)>>,
    /// extern-module name -> every (declaring file, node) sharing that name.
    /// Uniqueness is enforced per-file (symbols.rs), mirroring `modules`.
    externs: HashMap<String, Vec<(usize, &'a ast::ExternModule)>>,
    /// Per file: const name -> evaluated value (consts are file-local;
    /// imports do NOT bring consts into scope). One extra always-empty slot
    /// past `files.len() - 1` backs the synthesized `__Valid`/`__ValidSigned`
    /// builtin bundles' file index (`build_symbols`) — `resolve_bundle_fields`
    /// (`widths/mod.rs`) indexes this array directly by a bundle's declaring
    /// file with no bounds check, so that index must stay in range.
    file_consts: Vec<HashMap<String, i128>>,
    /// (declaring file, module name) -> its name table, built by pass 3
    /// (`names.rs`) and reused by pass 4 (`widths.rs`) and pass 5
    /// (`drivers.rs`). Keyed by file so two same-named modules from
    /// different files each get their own scope — a bare-name key would
    /// silently return the wrong file's scope once cross-file name reuse
    /// is legal (spec/02 section 1.5b). `Rc` so a pass can hold the scope
    /// while still reporting through `&mut self`.
    scopes: HashMap<(usize, String), Rc<names::Scope<'a>>>,
    diags: Vec<Diag>,
}

impl<'a> Checker<'a> {
    fn new(files: &'a [ast::File]) -> Self {
        Checker {
            files,
            modules: HashMap::new(),
            enums: HashMap::new(),
            funcs: HashMap::new(),
            bundles: HashMap::new(),
            externs: HashMap::new(),
            file_consts: vec![HashMap::new(); files.len() + 1],
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
