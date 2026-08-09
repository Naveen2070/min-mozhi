//! Verilog-2005 emitter — Phase 1, work item 5.
//!
//! Deliberately dumb and readable (architecture invariant #6): widths are
//! emitted symbolically (`[WIDTH-1:0]`), so module parameters pass straight
//! through to Verilog parameters with no const evaluation.
//!
//! Module layout:
//! - `mod.rs`      — `Project` symbol table, `emit` entry, `Emitter` state, shared helpers
//! - `module.rs`   — module shells, ports, instances, always-blocks
//! - `expr.rs`     — expression rendering (incl. match → ternary chains)
//! - `translit.rs` — Tamil → ASCII identifier pre-pass ([`transliterate`])
//!
//! Callers run [`transliterate`] on the ASTs first (the CLI does); the
//! emitter's own `check_ascii` is the backstop for anyone who skips it.
//! Not yet supported here (clean errors, not wrong output): `trunc` on
//! non-trivial expressions.

mod expr;
mod kinds;
mod module;
mod self_determined;
mod testbench;
mod translit;

#[cfg(test)]
mod tests;

pub use testbench::emit_testbench;
pub(crate) use translit::romanize;
pub use translit::transliterate;

use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::*;
use crate::checker::consteval::{self, Env};
use crate::diag::Diag;

/// Collect all user-function names directly called inside `expr`
/// (non-transitively). Mirrors `checker::funcs::collect_calls` —
/// kept local so the emitter doesn't couple to a private checker fn.
fn collect_fn_calls(expr: &Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::FnCall { name, args } => {
            if !out.contains(&name.name) {
                out.push(name.name.clone());
            }
            for a in args {
                collect_fn_calls(a, out);
            }
        }
        ExprKind::Unary { expr: e, .. } => collect_fn_calls(e, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_fn_calls(lhs, out);
            collect_fn_calls(rhs, out);
        }
        ExprKind::IfExpr { cond, then, els } => {
            collect_fn_calls(cond, out);
            collect_fn_calls(then, out);
            collect_fn_calls(els, out);
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_fn_calls(scrutinee, out);
            for arm in arms {
                collect_fn_calls(&arm.value, out);
            }
        }
        ExprKind::Concat(parts) => {
            for p in parts {
                collect_fn_calls(p, out);
            }
        }
        ExprKind::Replicate { count, parts } => {
            collect_fn_calls(count, out);
            for p in parts {
                collect_fn_calls(p, out);
            }
        }
        ExprKind::Index { base, index } => {
            collect_fn_calls(base, out);
            collect_fn_calls(index, out);
        }
        ExprKind::Slice { base, hi, lo } => {
            collect_fn_calls(base, out);
            collect_fn_calls(hi, out);
            collect_fn_calls(lo, out);
        }
        ExprKind::Call { args, .. } => {
            for a in args {
                collect_fn_calls(a, out);
            }
        }
        ExprKind::Field { base, .. } => collect_fn_calls(base, out),
        ExprKind::Int { .. } | ExprKind::Bool(_) | ExprKind::Ident(_) => {}
        ExprKind::BundleLit(inits) => {
            for fi in inits {
                collect_fn_calls(&fi.value, out);
            }
        }
        ExprKind::ArrayLit(elems) => {
            for e in elems {
                collect_fn_calls(e, out);
            }
        }
        ExprKind::EnumConstruct { args, .. } => {
            for a in args {
                collect_fn_calls(a, out);
            }
        }
    }
}

/// Collect the names of all user functions directly called by `decl`
/// (every statement + the tail, sorted and deduped for determinism).
pub(super) fn fn_direct_callees(decl: &FuncDecl) -> Vec<String> {
    let mut out = Vec::new();
    collect_fn_stmt_calls(&decl.stmts, &mut out);
    collect_fn_calls(&decl.tail, &mut out);
    out.sort();
    out.dedup();
    out
}

/// Walk a `fn`-body statement list for `collect_fn_calls` — mirrors
/// `checker::funcs::collect_fn_stmt_calls` (kept as a separate copy, same
/// as the pre-existing `direct_callees`/`collect_calls` duplication between
/// this file and the checker).
fn collect_fn_stmt_calls(stmts: &[FnStmt], out: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            FnStmt::Let(local) => collect_fn_calls(&local.value, out),
            FnStmt::If { cond, then, els } => {
                collect_fn_calls(cond, out);
                collect_fn_stmt_calls(then, out);
                if let Some(els) = els {
                    collect_fn_stmt_calls(els, out);
                }
            }
            FnStmt::Return(expr) => collect_fn_calls(expr, out),
            FnStmt::Loop { lo, hi, body, .. } => {
                collect_fn_calls(lo, out);
                collect_fn_calls(hi, out);
                collect_fn_stmt_calls(body, out);
            }
            // Which functions get called doesn't depend on whether/how
            // `foreach` gets lowered (substitution only touches `Ident`
            // reads of `var`, never `FnCall` nodes) — walk the raw `body`
            // directly, same as `Loop` above, no `ast::lower_foreach_fn`
            // needed. The Elements-form source is a plain array name (not
            // an expression), so unlike `Loop` there's no `hi`/`lo` pair to
            // walk for that variant.
            FnStmt::ForEach { source, body, .. } => {
                if let ForEachSource::Range { lo, hi } = source {
                    collect_fn_calls(lo, out);
                    collect_fn_calls(hi, out);
                }
                collect_fn_stmt_calls(body, out);
            }
            FnStmt::Error(_) => {}
        }
    }
}

/// Largest number of `repeat` iterations the emitter unrolls before erroring.
/// Defined once at the crate root and shared with the simulator's elaborator
/// (they MUST agree — see [`crate::REPEAT_BUDGET`]).
pub(crate) use crate::REPEAT_BUDGET;

/// Project-wide symbol table: every module, enum, and function by name,
/// borrowed from the parsed files. This is what lets `let u = Adder(...)` find
/// `Adder` regardless of which imported file defines it.
///
/// `modules`/`enums`/`bundles` are multimaps keyed by name, each entry
/// carrying every `(file_idx, decl)` that declares that name — reusing a
/// name across different files is legal (spec/02 section 1.5b); it is only
/// rejected within the SAME file. Resolve a reference with
/// [`Project::resolve_module`]/`resolve_enum`/`resolve_bundle`, which read
/// `QualIdent.resolved_file` (set by the checker/`project.rs`) to pick the
/// right one when a name is reused. `funcs` stays project-wide unique
/// (D-PKG-1) and is unaffected.
pub struct Project<'a> {
    /// All modules across the entry file + imports, by name.
    pub modules: HashMap<String, Vec<(usize, &'a Module)>>,
    /// All enums (file-level and module-level), by name.
    pub enums: HashMap<String, Vec<(usize, &'a EnumDecl)>>,
    /// All user-defined functions (file-level), by name. Used by the
    /// emitter to inject `function automatic` blocks into modules that
    /// call them.
    pub funcs: HashMap<String, &'a FuncDecl>,
    /// All file-level bundle declarations, by name. Consulted by the emitter
    /// to flatten bundle-typed ports/wires to individual Verilog signals.
    pub bundles: HashMap<String, Vec<(usize, &'a BundleDecl)>>,
    /// All `extern module` declarations, by name — mirrors `modules`.
    pub externs: HashMap<String, Vec<(usize, &'a ExternModule)>>,
}

impl<'a> Project<'a> {
    /// Build the table, rejecting a module/enum/bundle name reused within
    /// the SAME file (per-file uniqueness — spec/02 section 1.5b). The same
    /// name may legally appear in different files; resolving a reference
    /// between them is [`Project::resolve_module`]'s job at use-site.
    /// Diagnostics carry the index of the file holding the offending
    /// definition.
    pub fn from_files(files: &'a [File]) -> Result<Self, Vec<Diag>> {
        let mut modules: HashMap<String, Vec<(usize, &Module)>> = HashMap::new();
        let mut enums: HashMap<String, Vec<(usize, &EnumDecl)>> = HashMap::new();
        let mut funcs = HashMap::new();
        let mut bundles: HashMap<String, Vec<(usize, &BundleDecl)>> = HashMap::new();
        let mut externs: HashMap<String, Vec<(usize, &ExternModule)>> = HashMap::new();
        let mut diags = Vec::new();
        for (file_idx, file) in files.iter().enumerate() {
            for item in &file.items {
                match item {
                    TopItem::Module(m) => {
                        let entry = modules.entry(m.name.name.clone()).or_default();
                        if entry.iter().any(|&(f, _)| f == file_idx) {
                            diags.push(
                                Diag::new(
                                    m.name.span,
                                    format!(
                                        "module `{}` is defined twice in this file",
                                        m.name.name
                                    ),
                                )
                                .with_help(
                                    "module names are unique within one file (spec/02 section 1.5)",
                                )
                                .with_file(file_idx),
                            );
                        } else {
                            entry.push((file_idx, m));
                        }
                        for mi in &m.items {
                            if let ModuleItem::Enum(e) = mi {
                                enums
                                    .entry(e.name.name.clone())
                                    .or_default()
                                    .push((file_idx, e));
                            }
                        }
                    }
                    TopItem::Enum(e) => {
                        enums
                            .entry(e.name.name.clone())
                            .or_default()
                            .push((file_idx, e));
                    }
                    // Function declarations are injected per-using-module; no
                    // top-level Verilog emitted here (the checker already
                    // deduplicates them by name across the project).
                    TopItem::Func(f) => {
                        funcs.insert(f.name.name.clone(), f);
                    }
                    TopItem::Bundle(b) => {
                        bundles
                            .entry(b.name.name.clone())
                            .or_default()
                            .push((file_idx, b));
                    }
                    TopItem::Const(_) | TopItem::Test(_) | TopItem::Error(_) => {}
                    TopItem::ExternModule(em) => {
                        let entry = externs.entry(em.name.name.clone()).or_default();
                        if entry.iter().any(|&(f, _)| f == file_idx) {
                            diags.push(
                                Diag::new(
                                    em.name.span,
                                    format!(
                                        "extern module `{}` is defined more than once in this file",
                                        em.name.name
                                    ),
                                )
                                .with_help(
                                    "extern module names are unique within one file — rename one \
                                     of them (a different file may reuse this name; qualify the \
                                     reference with the import path if it becomes ambiguous, \
                                     spec/02 section 1.5b)",
                                )
                                .with_file(file_idx),
                            );
                        } else {
                            entry.push((file_idx, em));
                        }
                    }
                }
            }
        }

        // Mirror the checker's synthesized `__Valid`/`__ValidSigned` builtin
        // bundles (`ast::builtin_valid_bundles`, registered on the checker
        // side by `checker::symbols::build_symbols`) so the emitter's own
        // bundle-flattening code (port/wire declarations, bundle literals)
        // resolves these two names too, without a `.mimz` declaration ever
        // existing for them. `files.len()` — one past every real file
        // index — matches the checker's convention (see that function's doc
        // comment for why the checker specifically needs this exact index).
        let builtin_file = files.len();
        for decl in crate::ast::builtin_valid_bundles() {
            bundles
                .entry(decl.name.name.clone())
                .or_default()
                .push((builtin_file, decl));
        }

        if diags.is_empty() {
            Ok(Project {
                modules,
                enums,
                funcs,
                bundles,
                externs,
            })
        } else {
            Err(diags)
        }
    }

    /// Resolve a possibly-namespaced reference. The program already passed
    /// the checker by the time emit runs, so a `None` here (0 candidates,
    /// or a still-ambiguous/unmatched qualifier) means the checker SHOULD
    /// have already rejected this program — callers treat it exactly like
    /// today's "unknown" case (an unreachable-in-practice defensive path).
    pub fn resolve_module(&self, q: &QualIdent) -> Option<&'a Module> {
        Self::resolve(&self.modules, q).map(|(_, m)| m)
    }
    /// Like [`Self::resolve_module`], but also returns the declaring file's
    /// index — needed at instantiation sites to compute the SAME
    /// disambiguated Verilog identifier [`Self::verilog_module_name`] would
    /// give the module's own declaration header (see `module.rs::instance`).
    pub fn resolve_module_with_file(&self, q: &QualIdent) -> Option<(usize, &'a Module)> {
        Self::resolve(&self.modules, q)
    }
    /// Resolves a (possibly package-qualified) name to its `extern module`
    /// declaration.
    pub fn resolve_extern(&self, q: &QualIdent) -> Option<&'a ExternModule> {
        Self::resolve(&self.externs, q).map(|(_, e)| e)
    }
    /// Like [`Self::resolve_extern`], but also returns the declaring file's index.
    pub fn resolve_extern_with_file(&self, q: &QualIdent) -> Option<(usize, &'a ExternModule)> {
        Self::resolve(&self.externs, q)
    }
    /// Resolves an instantiation target against BOTH real modules and
    /// extern declarations — real modules take precedence if a name
    /// somehow exists in both maps (should be unreachable in a checked
    /// program; the checker's per-file uniqueness doesn't cross-check
    /// categories, so this is a defensive tie-break, not a real ambiguity
    /// rule).
    pub fn resolve_target_with_file(&self, q: &QualIdent) -> Option<(usize, ModuleTarget<'a>)> {
        if let Some((f, m)) = self.resolve_module_with_file(q) {
            Some((f, ModuleTarget::Real(m)))
        } else {
            self.resolve_extern_with_file(q)
                .map(|(f, e)| (f, ModuleTarget::Extern(e)))
        }
    }
    /// Resolves a (possibly package-qualified) name to its `enum` declaration.
    pub fn resolve_enum(&self, q: &QualIdent) -> Option<&'a EnumDecl> {
        Self::resolve(&self.enums, q).map(|(_, e)| e)
    }
    /// Resolves a (possibly package-qualified) name to its `bundle` declaration.
    pub fn resolve_bundle(&self, q: &QualIdent) -> Option<&'a BundleDecl> {
        Self::resolve(&self.bundles, q).map(|(_, b)| b)
    }
    fn resolve<T>(
        table: &HashMap<String, Vec<(usize, &'a T)>>,
        q: &QualIdent,
    ) -> Option<(usize, &'a T)> {
        let candidates = table.get(&q.name.name)?;
        if q.is_bare() {
            match candidates.as_slice() {
                [only] => Some(*only),
                _ => None, // 0 or ambiguous — checker already rejected this
            }
        } else {
            let target = q.resolved_file.get()?;
            candidates.iter().find(|&&(f, _)| f == target).copied()
        }
    }

    /// The Verilog identifier for `m`, disambiguated by its declaring
    /// `file` index ONLY when 2+ files declare the same name — the
    /// packages/namespacing same-name-across-files feature (spec/02
    /// section 1.5b). Every one of the pre-existing single-declaration
    /// examples gets back the bare name, byte-for-byte: this check is a
    /// per-name lookup, so it is a strict no-op whenever `name` has exactly
    /// one declaring file. `__f<file>` mirrors the same accepted-risk
    /// double-underscore separator `Emitter::inst_name` already uses to
    /// flatten `repeat` instance arrays: a user could in principle declare
    /// a module literally named e.g. `Fifo__f1`, but Min-Mozhi's identifier
    /// grammar places no restriction on leading/embedded underscores, so
    /// this is the same pre-existing, accepted risk class as `inst_name`'s
    /// `__<idx>`, not a new one.
    pub fn verilog_module_name(&self, file: usize, m: &Module) -> String {
        if self.modules.get(&m.name.name).is_some_and(|v| v.len() > 1) {
            format!("{}__f{file}", m.name.name)
        } else {
            m.name.name.clone()
        }
    }

    /// Look up an enum by bare name only, taking the first declaring file
    /// when the name is reused across files. Used at the handful of
    /// value-level sites (`Enum.Variant` field access, match-pattern
    /// bindings) that carry a plain `&str`/`Ident`, not a `QualIdent` — the
    /// grammar doesn't support qualifying those positions, so there is no
    /// ambiguity to detect; this mirrors the checker's own
    /// `Checker::lookup_enum` (same non-goal, same first-match behavior).
    pub fn first_enum(&self, name: &str) -> Option<&'a EnumDecl> {
        self.enums
            .get(name)
            .and_then(|v| v.first())
            .map(|&(_, e)| e)
    }
}

/// Emit all modules of all files into ONE Verilog source string (one `.v`
/// output per `mimz compile`, header comment included). Errors are
/// collected across modules — one bad module doesn't hide the others.
pub fn emit(project: &Project, files: &[File]) -> Result<String, Vec<Diag>> {
    let mut em = Emitter {
        project,
        out: String::new(),
        diags: Vec::new(),
        cur_file: 0,
        env: Env::new(),
        module_envs: HashMap::new(),
        repeat_budget: REPEAT_BUDGET,
        clog2_fn_used: false,
        emitting_port: false,
        funcs_used: Vec::new(),
        bundle_sigs: HashMap::new(),
        hoist_counter: 0,
        hoisted_decls: String::new(),
        cur_decls: Rc::default(),
        cover_ordinals: HashMap::new(),
    };
    em.out.push_str(&format!(
        "// Generated by mimz {} (edition {}) — Min-Mozhi (மின்மொழி). Do not edit.\n\n",
        crate::version::COMPILER_VERSION,
        crate::version::current().tag()
    ));
    // Pre-pass: every module's compile-time env (its FILE's consts plus
    // its own), keyed by (declaring file, module name) — NOT name alone:
    // two files may legally declare the same module name (spec/02 section
    // 1.5b), and a name-only key would let the second module's env
    // silently shadow (or be shadowed by) the first's. `instance()` needs
    // this to fold a CHILD's consts into its port widths — the parent's
    // Verilog knows nothing about a child's `const WIDTH` (and must never
    // substitute the parent's same-named const instead). Silent: the main
    // walk below re-evaluates the same consts and reports any errors once.
    for (file_idx, file) in files.iter().enumerate() {
        let file_env = fold_consts(
            Env::new(),
            file.items.iter().filter_map(|i| match i {
                TopItem::Const(c) => Some(c),
                _ => None,
            }),
        );
        for item in &file.items {
            if let TopItem::Module(m) = item {
                let menv = fold_consts(
                    file_env.clone(),
                    m.items.iter().filter_map(|i| match i {
                        ModuleItem::Const(c) => Some(c),
                        _ => None,
                    }),
                );
                em.module_envs
                    .entry((file_idx, m.name.name.clone()))
                    .or_insert(menv);
            }
        }
    }
    for (file_idx, file) in files.iter().enumerate() {
        em.cur_file = file_idx;
        // Compile-time constants fold to literals in the emitted Verilog
        // (they are `int`/`bool`, never hardware — spec/02 section 4).
        // File consts are visible to every module in the file; module
        // consts are layered on at the module and peeled back off after.
        let file_consts = em.eval_consts(
            Env::new(),
            file.items.iter().filter_map(|i| match i {
                TopItem::Const(c) => Some(c),
                _ => None,
            }),
        );
        em.env = file_consts;
        for item in &file.items {
            if let TopItem::Module(m) = item {
                em.module(m);
                em.out.push('\n');
            }
        }
    }
    if em.diags.is_empty() {
        Ok(em.out)
    } else {
        Err(em.diags)
    }
}

/// Emitter state: the symbol table to look up modules/enums, the growing
/// output text, and collected errors. Emission continues after an error
/// (output is discarded if any error was recorded).
struct Emitter<'a> {
    project: &'a Project<'a>,
    out: String,
    diags: Vec<Diag>,
    /// Index of the file whose modules are currently being emitted —
    /// stamped onto every diagnostic so errors in imported files render
    /// against the right source (see `project::render_diags`).
    cur_file: usize,
    /// Compile-time values in scope: file consts, then module consts, then
    /// enclosing `repeat` loop variables (pushed/popped per iteration). An
    /// identifier found here renders as its folded decimal literal; module
    /// parameters are deliberately ABSENT so they stay symbolic Verilog
    /// `parameter`s (the dumb-emitter invariant). See `expr_subst`.
    env: Env,
    /// Every module's own compile-time env (its file's consts + its
    /// module consts), built by the pre-pass in [`emit`]. Used when
    /// INSTANTIATING a module: the child's port-width expressions fold
    /// against the CHILD's constants, never the parent's. Keyed by
    /// `(declaring file, name)`, not name alone — two files may legally
    /// declare the same module name (spec/02 section 1.5b).
    module_envs: HashMap<(usize, String), Env>,
    /// Iterations of `repeat` left to unroll in the current pass before
    /// `ModuleItem::Repeat` errors — a runaway-bound backstop.
    repeat_budget: i128,
    /// Set when the current module emits `clog2(<symbolic param>)` in a body
    /// width — triggers injecting the Verilog-2005 `clog2` constant function at
    /// the top of the module body (reset per module).
    clog2_fn_used: bool,
    /// True while emitting the module HEADER's port widths. A `clog2(<param>)`
    /// there is an error: the constant function lives in the body and cannot
    /// forward-reference into the port list (reset per module).
    emitting_port: bool,
    /// User-defined functions used by the current module, in topological order
    /// (callees before callers). Populated transitively by `mark_fn_used` as
    /// `FnCall` nodes are rendered; injected at module-body top alongside
    /// `CLOG2_FN` (reset per module).
    funcs_used: Vec<String>,
    /// Bundle-typed signals in the current module: signal name → (bundle type
    /// reference, args). The bundle reference is the full `QualIdent` (not
    /// just its bare name) so a same-named bundle reused across files still
    /// resolves to the right declaration.
    /// Populated from flat items before emit_drives; cleared after.
    /// Lets emit_drives flatten `sigA = sigB` and `sig = { field: val }` drives.
    bundle_sigs: HashMap<String, (QualIdent, Vec<NamedArg>)>,
    /// Counter for `__mimz_sub_N` hoisted-wire names — Stage 4, Phase
    /// A1b (`docs/superpowers/specs/2026-07-19-emitter-self-determined-position-design.local.md`).
    /// Reset per module, alongside `clog2_fn_used`/`funcs_used`.
    hoist_counter: u32,
    /// Accumulated `wire [...] __mimz_sub_N; assign __mimz_sub_N = ...;`
    /// declarations for the CURRENT module, inserted at `fn_pos`
    /// alongside the existing `clog2`/user-`fn` injections. Reset per
    /// module.
    hoisted_decls: String,
    /// Every `Port`/`Wire`/`Reg` name of the CURRENT module, mapped to its
    /// resolved `Kind` — the "`flat_items_in_scope`" a self-determined
    /// hoist check needs (Stage 4, Phase A1b, Task 6). Built once per
    /// module by `Emitter::build_decls` right after `flatten_items`
    /// (`module()`), same timing as `bundle_sigs` above. Stays at its
    /// default `HashMap::new()` for every OTHER `Emitter` (the testbench
    /// emitter never calls `module()`, so it never gains signal `Kind`s —
    /// see `expr::kind_is_inferrable`'s doc for why an empty map here is
    /// exactly the right "don't hoist" answer there, not a bug).
    ///
    /// `Rc` for a borrow reason, not a sharing one (GAP-12). Every hoist site
    /// in `expr.rs` needs `&self.cur_decls` live across a `&mut self` call
    /// (`expr_subst`, `hoist_if_needed`), which the borrow checker forbids, so
    /// all 22 of them snapshot it first. As a bare `HashMap` that snapshot was
    /// a full deep clone **per expression node** — on the order of 64M entry
    /// clones for a module of 8,000 declarations and 8,000 assignments, and
    /// the reason `mimz compile` measured superlinear in module size. `Rc`
    /// makes the snapshot a refcount bump while keeping the semantics
    /// identical: the map is replaced wholesale per module (below) and never
    /// mutated in place, so a snapshot could never observe a later write
    /// either way.
    cur_decls: Rc<HashMap<String, crate::width_rules::Kind>>,
    /// Every `cover(...)` statement in the CURRENT module (module-item AND
    /// `on`-block form combined), mapped `span.start -> ordinal rank by
    /// source position`. Names each hidden hit-counter `__cover_{ordinal}_
    /// count` instead of `__cover_{span.start}_count` (GAP-6 follow-up,
    /// found via `tests/translate.rs`'s pretty-print round-trip test): a
    /// raw byte offset shifts on ANY reformat (pretty-print, `mimz
    /// translate`, keyword reskin) even when the statement's relative
    /// position among covers is unchanged, so it is not a stable
    /// register name across a semantically-identical re-emit. An ordinal
    /// rank is — pretty-printing never reorders statements. Built once per
    /// module by `Emitter::build_cover_ordinals`, same timing as
    /// `cur_decls` above.
    cover_ordinals: HashMap<usize, usize>,
}

/// Verilog-2005 constant function matching [`consteval::clog2_bits`] (floored at
/// 1). Injected once per module that sizes a body declaration with
/// `clog2(<parameter>)`, so the width tracks an overridden parameter.
const CLOG2_FN: &str = "    function integer clog2;\n\
\x20       input integer value;\n\
\x20       integer i;\n\
\x20       begin\n\
\x20           if (value <= 1) clog2 = 1;\n\
\x20           else begin\n\
\x20               clog2 = 0;\n\
\x20               for (i = value - 1; i > 0; i = i >> 1) clog2 = clog2 + 1;\n\
\x20           end\n\
\x20       end\n\
\x20   endfunction\n";

/// Fold `const` declarations onto `base` WITHOUT reporting failures —
/// the pre-pass twin of [`Emitter::eval_consts`] (the main walk
/// re-evaluates the same constants and owns the diagnostics).
fn fold_consts<'c>(mut base: Env, consts: impl Iterator<Item = &'c ConstDecl>) -> Env {
    for c in consts {
        if let Ok(v) = consteval::eval(&c.value, &base) {
            base.insert(c.name.name.clone(), v);
        }
    }
    base
}

impl Emitter<'_> {
    /// Record an error; empty `help` means no help line. The current
    /// file index is stamped automatically — emitter errors always point
    /// into the file being emitted (instance errors use the parent's
    /// spans, not the child's).
    fn err(&mut self, span: crate::span::Span, msg: impl Into<String>, help: &str) {
        let mut d = Diag::new(span, msg).with_file(self.cur_file);
        if !help.is_empty() {
            d = d.with_help(help.to_string());
        }
        self.diags.push(d);
    }

    /// Fold a sequence of `const` declarations onto `base`, returning the
    /// extended environment. Each const may use the ones before it (and
    /// anything in `base`) — same top-to-bottom rule as the checker. A
    /// const that doesn't fold is reported (the checker has usually said
    /// so already; this keeps direct-to-emitter callers honest).
    fn eval_consts<'c>(
        &mut self,
        mut base: Env,
        consts: impl Iterator<Item = &'c ConstDecl>,
    ) -> Env {
        for c in consts {
            match consteval::eval(&c.value, &base) {
                Ok(v) => {
                    base.insert(c.name.name.clone(), v);
                }
                Err(d) => self.diags.push(d.with_file(self.cur_file)),
            }
        }
        base
    }

    /// Evaluate a compile-time expression against the current env — used
    /// for `repeat` bounds and instance/lvalue indices, where the emitter
    /// genuinely needs the integer (to unroll, or to build a flat name).
    /// Reports and returns `None` if it doesn't fold. Narrows the checker's
    /// arbitrary-width `ConstVal` to `i128` via `to_i128_saturating` — every
    /// caller here wants a STRUCTURAL size (a repeat bound or an index),
    /// already capped far below `i128::MAX` by `REPEAT_BUDGET`/`MAX_WIDTH`,
    /// never a signal's own data value (BUG-13 layer 2).
    fn eval_const(&mut self, e: &Expr) -> Option<i128> {
        match consteval::eval(e, &self.env) {
            Ok(v) => Some(v.to_i128_saturating()),
            Err(d) => {
                self.diags.push(d.with_file(self.cur_file));
                None
            }
        }
    }

    /// The Verilog name of one instance: the plain name normally, or the
    /// flattened `fa__<idx>` when it is an array element inside `repeat`
    /// (double underscore to stay clear of user names). Its auto-wired
    /// outputs are then `fa__<idx>_<port>` — exactly what an indexed field
    /// read renders to in `expr.rs`.
    fn inst_name(&mut self, inst: &Inst) -> String {
        match &inst.index {
            Some(e) => {
                let i = self.eval_const(e).unwrap_or(0);
                format!("{}__{}", inst.name.name, i)
            }
            None => inst.name.name.clone(),
        }
    }

    /// Unroll one `repeat` block: evaluate its bounds, then run `body` once
    /// per iteration value with the loop variable bound in `env` (shadowed
    /// and restored, so nested loops nest cleanly). The half-open range
    /// `lo..hi` runs `lo..=hi-1`; an empty or reversed range emits nothing.
    /// Over-long ranges error against the per-pass budget.
    fn unroll(&mut self, r: &Repeat, body: fn(&mut Self, &[ModuleItem])) {
        let (Some(lo), Some(hi)) = (self.eval_const(&r.lo), self.eval_const(&r.hi)) else {
            return;
        };
        let count = (hi - lo).max(0);
        if count > self.repeat_budget {
            self.err(
                r.span,
                format!("`repeat` would unroll {count} times, over the limit of {REPEAT_BUDGET}"),
                "this is compile-time hardware generation, not a runtime loop — \
                 narrow the range (a datapath this wide is almost certainly a typo)",
            );
            return;
        }
        self.repeat_budget -= count;
        let mut i = lo;
        while i < hi {
            let shadowed = self
                .env
                .insert(r.var.name.clone(), consteval::ConstVal::from_i128(i));
            body(self, &r.items);
            match shadowed {
                Some(v) => self.env.insert(r.var.name.clone(), v),
                None => self.env.remove(&r.var.name),
            };
            i += 1;
        }
    }

    /// Verilog identifiers are ASCII-only; Tamil-script names (legal in
    /// Min-Mozhi) get a clean error here until a transliteration pass
    /// exists. Returns whether the name is usable.
    fn check_ascii(&mut self, id: &Ident) -> bool {
        if id.name.is_ascii() {
            true
        } else {
            self.err(
                id.span,
                format!(
                    "`{}` — a non-ASCII identifier reached the Verilog emitter",
                    id.name
                ),
                "Verilog identifiers are ASCII-only — run `emit_verilog::transliterate` \
                 on the ASTs before emitting (the `mimz` CLI does this automatically)",
            );
            false
        }
    }
}

/// Bits needed to encode `n` variants (≥ 1). Same function as the `clog2`
/// const-builtin — one source of truth so enum widths and `clog2(n)` agree.
fn clog2(n: usize) -> u32 {
    crate::checker::consteval::clog2_bits(n as u128)
}

/// Verilog localparam name for an enum variant: `State.Red` → `STATE_RED`.
fn enum_const(enum_name: &str, variant: &str) -> String {
    format!("{}_{}", enum_name.to_uppercase(), variant.to_uppercase())
}

/// Render an integer literal, preserving the writer's chosen base.
fn verilog_literal(value: &crate::bits::Bits, raw: &str) -> String {
    if let Some(bin) = raw.strip_prefix("0b") {
        format!("'b{bin}")
    } else if let Some(hex) = raw.strip_prefix("0x") {
        format!("'h{hex}")
    } else {
        let width = crate::bits::natural_width(value).max(1);
        crate::bits::bits_to_decimal_string(value, width, false)
    }
}
