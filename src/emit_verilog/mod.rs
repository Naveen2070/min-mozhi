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
mod module;
mod testbench;
mod translit;

pub use testbench::emit_testbench;
pub(crate) use translit::romanize;
pub use translit::transliterate;

use std::collections::HashMap;

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
        ExprKind::BundleLit(_) => todo!(),
    }
}

/// Collect the names of all user functions directly called by `decl`
/// (locals + body, sorted and deduped for determinism).
pub(super) fn fn_direct_callees(decl: &FuncDecl) -> Vec<String> {
    let mut out = Vec::new();
    for local in &decl.locals {
        collect_fn_calls(&local.value, &mut out);
    }
    collect_fn_calls(&decl.body, &mut out);
    out.sort();
    out.dedup();
    out
}

/// Largest number of `repeat` iterations the emitter unrolls before erroring.
/// Defined once at the crate root and shared with the simulator's elaborator
/// (they MUST agree — see [`crate::REPEAT_BUDGET`]).
pub(crate) use crate::REPEAT_BUDGET;

/// Project-wide symbol table: every module, enum, and function by name,
/// borrowed from the parsed files. This is what lets `let u = Adder(...)` find
/// `Adder` regardless of which imported file defines it.
pub struct Project<'a> {
    /// All modules across the entry file + imports, by name.
    pub modules: HashMap<String, &'a Module>,
    /// All enums (file-level and module-level), by name.
    pub enums: HashMap<String, &'a EnumDecl>,
    /// All user-defined functions (file-level), by name. Used by the
    /// emitter to inject `function automatic` blocks into modules that
    /// call them.
    pub funcs: HashMap<String, &'a FuncDecl>,
}

impl<'a> Project<'a> {
    /// Build the table, rejecting duplicate module names (module names are
    /// project-unique — spec/02 section 1.5). Diagnostics carry the index
    /// of the file holding the offending definition.
    pub fn from_files(files: &'a [File]) -> Result<Self, Vec<Diag>> {
        let mut modules = HashMap::new();
        let mut enums = HashMap::new();
        let mut funcs = HashMap::new();
        let mut diags = Vec::new();
        for (file_idx, file) in files.iter().enumerate() {
            for item in &file.items {
                match item {
                    TopItem::Module(m) => {
                        if modules.insert(m.name.name.clone(), m).is_some() {
                            diags.push(
                                Diag::new(m.name.span, format!("module `{}` is defined twice", m.name.name))
                                    .with_help("module names must be unique across the whole project (spec/02 section 1.5)")
                                    .with_file(file_idx),
                            );
                        }
                        for mi in &m.items {
                            if let ModuleItem::Enum(e) = mi {
                                enums.insert(e.name.name.clone(), e);
                            }
                        }
                    }
                    TopItem::Enum(e) => {
                        enums.insert(e.name.name.clone(), e);
                    }
                    // Function declarations are injected per-using-module; no
                    // top-level Verilog emitted here (the checker already
                    // deduplicates them by name across the project).
                    TopItem::Func(f) => {
                        funcs.insert(f.name.name.clone(), f);
                    }
                    TopItem::Const(_) | TopItem::Test(_) | TopItem::Error(_) | TopItem::Bundle(_) => {}
                }
            }
        }
        if diags.is_empty() {
            Ok(Project {
                modules,
                enums,
                funcs,
            })
        } else {
            Err(diags)
        }
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
    };
    em.out.push_str(&format!(
        "// Generated by mimz {} (edition {}) — Min-Mozhi (மின்மொழி). Do not edit.\n\n",
        crate::version::COMPILER_VERSION,
        crate::version::current().tag()
    ));
    // Pre-pass: every module's compile-time env (its FILE's consts plus
    // its own), keyed by module name. `instance()` needs this to fold a
    // CHILD's consts into its port widths — the parent's Verilog knows
    // nothing about a child's `const WIDTH` (and must never substitute
    // the parent's same-named const instead). Silent: the main walk
    // below re-evaluates the same consts and reports any errors once.
    for file in files {
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
                em.module_envs.entry(m.name.name.clone()).or_insert(menv);
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
    /// against the CHILD's constants, never the parent's.
    module_envs: HashMap<String, Env>,
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
    /// Reports and returns `None` if it doesn't fold.
    fn eval_const(&mut self, e: &Expr) -> Option<i128> {
        match consteval::eval(e, &self.env) {
            Ok(v) => Some(v),
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
            let shadowed = self.env.insert(r.var.name.clone(), i);
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
fn verilog_literal(value: u128, raw: &str) -> String {
    if let Some(bin) = raw.strip_prefix("0b") {
        format!("'b{bin}")
    } else if let Some(hex) = raw.strip_prefix("0x") {
        format!("'h{hex}")
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser};

    fn parse(src: &str) -> File {
        parser::parse(lexer::lex(src).unwrap()).unwrap()
    }

    /// Emit one self-contained source (no imports) to Verilog text.
    fn emit_src(src: &str) -> String {
        let files = [parse(src)];
        let project = Project::from_files(&files).unwrap();
        emit(&project, &files).expect("emit should succeed")
    }

    /// Emit one source expecting failure; return the diagnostics.
    fn emit_src_err(src: &str) -> Vec<Diag> {
        let files = [parse(src)];
        let project = Project::from_files(&files).unwrap();
        emit(&project, &files).expect_err("emit should fail")
    }

    #[test]
    fn clog2_folds_into_the_port_width() {
        // clog2(9) = 4 bits → `output [3:0] o`. Proves the const-builtin folds to
        // the right VALUE in a width position, not just that it is accepted.
        let v = emit_src("module M {\n  out o: bits[clog2(9)]\n  o = 0\n}\n");
        // The emitter keeps a derived width in `(value)-1:0` form; the folded
        // `(4)` is the proof clog2(9) evaluated to 4.
        assert!(
            v.contains("[(4)-1:0] o"),
            "clog2(9) should size `o` to 4 bits ([(4)-1:0]):\n{v}"
        );
        // A folded literal `clog2` must not drag in the constant function.
        assert!(
            !v.contains("function integer clog2"),
            "a folded clog2 must not emit the function:\n{v}"
        );
    }

    #[test]
    fn clog2_of_a_const_derives_the_width() {
        // DEPTH a `const` = 16 → clog2 = 4 → `[(4)-1:0] ptr`. Consts fold in the
        // emitted Verilog, so this is the supported parametric-width path.
        let v = emit_src(
            "module M {\n  const DEPTH: int = 16\n  out ptr: bits[clog2(DEPTH)]\n  ptr = 0\n}\n",
        );
        assert!(
            v.contains("[(4)-1:0] ptr"),
            "clog2(const 16) should size `ptr` to 4 bits ([(4)-1:0]):\n{v}"
        );
    }

    #[test]
    fn clog2_of_a_parameter_in_a_body_width_emits_the_constant_function() {
        // A parameter stays symbolic, so the width tracks an override via the
        // injected Verilog-2005 `clog2` constant function.
        let v = emit_src(
            "module M(DEPTH: int = 16) {\n  out o: bit\n  wire w: bits[clog2(DEPTH)] = 0\n  o = 0\n}\n",
        );
        assert!(
            v.contains("function integer clog2"),
            "a parametric clog2 width must inject the constant function:\n{v}"
        );
        assert!(
            v.contains("[(clog2(DEPTH))-1:0] w"),
            "the width must call clog2(DEPTH) so an override is honored:\n{v}"
        );
    }

    #[test]
    fn clog2_of_a_parameter_in_a_port_is_an_emit_error() {
        // A port width lives in the header, which the body-scoped function can't
        // reach — an honest error, never a wrong width.
        let diags = emit_src_err(
            "module M(DEPTH: int = 16) {\n  out ptr: bits[clog2(DEPTH)]\n  ptr = 0\n}\n",
        );
        assert!(
            diags.iter().any(|d| d.msg.contains("clog2")),
            "expected a clog2 emit error, got: {diags:?}"
        );
    }

    #[test]
    fn on_fall_emits_negedge() {
        let v = emit_src(
            "module M {\n  clock clk\n  reset rst\n  out q: bit\n  reg r: bit = 0\n  on fall(clk) { r <- !r }\n  q = r\n}\n",
        );
        assert!(
            v.contains("always @(negedge clk)"),
            "`on fall` must lower to a negedge block:\n{v}"
        );
    }

    #[test]
    fn async_reset_widens_the_sensitivity_list() {
        let v = emit_src(
            "module M {\n  clock clk\n  async reset rst\n  out q: bit\n  reg r: bit = 0\n  on rise(clk) { r <- !r }\n  q = r\n}\n",
        );
        assert!(
            v.contains("always @(posedge clk or posedge rst)"),
            "`async reset` must add `or posedge rst` to the sensitivity list:\n{v}"
        );
    }

    #[test]
    fn a_sync_reset_stays_clock_only() {
        let v = emit_src(
            "module M {\n  clock clk\n  reset rst\n  out q: bit\n  reg r: bit = 0\n  on rise(clk) { r <- !r }\n  q = r\n}\n",
        );
        assert!(
            v.contains("always @(posedge clk) begin"),
            "a plain `reset` must NOT widen the sensitivity list:\n{v}"
        );
    }

    #[test]
    fn a_builtin_lowers_parenthesized_inside_a_larger_expression() {
        // `min(b, c)` must lower to a self-contained, fully-parenthesized ternary
        // so it composes correctly under a surrounding operator (here `&`) — no
        // precedence leak from the host expression into the built-in or back.
        let v = emit_src(
            "module M {\n  in a: bits[8]\n  in b: bits[8]\n  in c: bits[8]\n  out y: bits[8]\n  y = a & min(b, c)\n}\n",
        );
        assert!(
            v.contains("((b < c) ? (b) : (c))"),
            "min lowered + parenthesized:\n{v}"
        );
    }

    #[test]
    fn repeat_unrolls_drives_with_folded_indices() {
        let v = emit_src(
            "module M {\n  in x: bits[4]\n  out y: bits[4]\n  repeat i: 0..4 {\n    y[i] = x[i]\n  }\n}\n",
        );
        for i in 0..4 {
            assert!(
                v.contains(&format!("assign y[{i}] = x[{i}];")),
                "missing y[{i}]\n{v}"
            );
        }
        assert!(!v.contains("y[4]"), "half-open range must stop at 3");
    }

    #[test]
    fn repeat_var_folds_in_index_arithmetic() {
        let v = emit_src(
            "module M {\n  in x: bits[8]\n  out y: bits[8]\n  repeat i: 0..3 {\n    y[i + 1] = x[i]\n  }\n}\n",
        );
        assert!(v.contains("assign y[1] = x[0];"));
        assert!(v.contains("assign y[3] = x[2];"));
    }

    #[test]
    fn empty_and_reversed_ranges_emit_nothing() {
        let empty =
            emit_src("module M {\n  out y: bits[4]\n  repeat i: 0..0 {\n    y[i] = 0\n  }\n}\n");
        assert!(!empty.contains("assign y"), "0..0 generates nothing");
        let reversed =
            emit_src("module M {\n  out y: bits[4]\n  repeat i: 4..0 {\n    y[i] = 0\n  }\n}\n");
        assert!(
            !reversed.contains("assign y"),
            "a reversed range generates nothing"
        );
    }

    #[test]
    fn repeat_over_budget_errors_cleanly() {
        let diags = emit_src_err(
            "module M {\n  out y: bits[4]\n  repeat i: 0..5000 {\n    y[0] = 0\n  }\n}\n",
        );
        assert!(
            diags
                .iter()
                .any(|d| d.msg.contains("unroll") && d.msg.contains("limit")),
            "expected a budget error, got: {:?}",
            diags.iter().map(|d| &d.msg).collect::<Vec<_>>()
        );
    }

    #[test]
    fn nested_repeat_folds_both_variables() {
        let v = emit_src(
            "module M {\n  out y: bits[4]\n  repeat i: 0..2 {\n    repeat j: 0..2 {\n      y[i] = j\n    }\n  }\n}\n",
        );
        // i and j both fold: the i=1, j=1 iteration drives `y[1] = 1`.
        assert!(v.contains("assign y[0] = 0;"));
        assert!(v.contains("assign y[1] = 1;"));
    }

    #[test]
    fn repeat_instance_array_gets_flat_names() {
        let v = emit_src(
            "module Sub {\n  in a: bit\n  out o: bit\n  o = a\n}\n\
             module Top {\n  in x: bits[2]\n  out y: bits[2]\n  repeat i: 0..2 {\n    let u[i] = Sub() { a: x[i] }\n    y[i] = u[i].o\n  }\n}\n",
        );
        assert!(v.contains("Sub u__0 ("), "flat instance name u__0");
        assert!(v.contains("Sub u__1 ("), "flat instance name u__1");
        assert!(v.contains("wire u__0_o;"), "auto-wire for u[0].o");
        assert!(
            v.contains("assign y[0] = u__0_o;"),
            "indexed field read folds"
        );
        assert!(v.contains("assign y[1] = u__1_o;"));
    }

    #[test]
    fn module_const_folds_in_widths_and_emits_no_hardware() {
        let v = emit_src(
            "module M {\n  const N: int = 3\n  out y: bits[N]\n  repeat i: 0..N {\n    y[i] = 0\n  }\n}\n",
        );
        assert!(
            v.contains("[(3)-1:0] y"),
            "const N folds to 3 in the port width"
        );
        assert!(v.contains("assign y[2] = 0;"), "0..N runs to N-1");
        assert!(
            !v.contains("[(N)"),
            "the symbolic const name must not survive into widths"
        );
    }

    /// Like [`emit_src`], but with the transliteration pre-pass — the
    /// path the CLI takes.
    fn emit_src_translit(src: &str) -> String {
        let mut files = [parse(src)];
        transliterate(&mut files);
        let project = Project::from_files(&files).unwrap();
        emit(&project, &files).expect("emit should succeed")
    }

    #[test]
    fn tamil_identifiers_emit_as_romanized_verilog() {
        // Identifiers only — பதிவேடு etc. are KEYWORD spellings
        // (keywords.toml) and can never be identifiers.
        let v = emit_src_translit(
            "module விளக்கு {\n  clock மணி\n  reset மீள்\n  out ஒளி: bit\n  reg சுடர்: bit = 0\n  on rise(மணி) {\n    சுடர் <- !சுடர்\n  }\n  ஒளி = சுடர்\n}\n",
        );
        assert!(
            v.contains("module villakku ("),
            "module name romanizes:\n{v}"
        );
        assert!(v.contains("input wire manni"), "clock romanizes");
        assert!(v.contains("output wire olli"), "output romanizes");
        assert!(v.contains("reg sutar;"), "reg romanizes:\n{v}");
        assert!(
            v.contains("always @(posedge manni)"),
            "the on-block clock uses the SAME romanization"
        );
        // Only the generator banner COMMENT may carry Tamil; every line
        // of actual Verilog must be pure ASCII.
        for line in v.lines().filter(|l| !l.starts_with("//")) {
            assert!(line.is_ascii(), "non-ASCII outside a comment: {line}");
        }
    }

    #[test]
    fn colliding_romanizations_get_suffixes_and_ascii_names_are_safe() {
        // ந and ன both romanize to `n`; the user also owns plain ASCII
        // `nii` — first-seen Tamil name takes `nii_2`, the second `nii_3`.
        let v = emit_src_translit(
            "module M {\n  in nii: bit\n  in நீ: bit\n  in னீ: bit\n  out y: bit\n  y = nii ^ நீ ^ னீ\n}\n",
        );
        assert!(v.contains("input wire nii,"), "the ASCII name is untouched");
        assert!(v.contains("nii_2"), "first Tamil clash gets _2:\n{v}");
        assert!(v.contains("nii_3"), "second Tamil clash gets _3:\n{v}");
    }

    #[test]
    fn child_consts_fold_into_parent_auto_wires() {
        // The child's ports are sized by ITS OWN `const W`. The parent's
        // auto-wire for `u.y` must fold that const to a literal — the
        // symbolic name `W` does not exist in the parent's Verilog.
        // (Found 2026-06-12: `wire [(W)-1:0]` leaked and iverilog
        // rejected it — "Unable to bind parameter `W`".)
        let v = emit_src(
            "module C {\n  const W: int = 4\n  in a: bits[W]\n  out y: bits[W]\n  y = a\n}\n\
             module Top {\n  in x: bits[4]\n  out z: bits[4]\n  let u = C() { a: x }\n  z = u.y\n}\n",
        );
        assert!(
            v.contains("wire [(4)-1:0] u_y;"),
            "child const W must fold to 4 in the auto-wire:\n{v}"
        );
    }

    #[test]
    fn parent_const_never_substitutes_into_child_widths() {
        // Same const NAME, different values: the auto-wire must use the
        // CHILD's 4, never the parent's 8 — silently wrong hardware
        // otherwise.
        let v = emit_src(
            "module C {\n  const W: int = 4\n  in a: bits[W]\n  out y: bits[W]\n  y = a\n}\n\
             module Top {\n  const W: int = 8\n  in x: bits[4]\n  out z: bits[4]\n  let u = C() { a: x }\n  z = u.y\n}\n",
        );
        assert!(
            v.contains("wire [(4)-1:0] u_y;"),
            "the CHILD's W=4 sizes the wire, not the parent's W=8:\n{v}"
        );
    }

    /// Project-level diagnostics must say WHICH file they point into —
    /// `render_diags` uses this to pick the right source excerpt.
    #[test]
    fn diags_carry_the_file_index() {
        // Duplicate module: file 0 defines `A`, file 1 redefines it.
        let files = [parse("module A {\n}\n"), parse("module A {\n}\n")];
        let diags = Project::from_files(&files).err().expect("duplicate");
        assert_eq!(diags[0].file, Some(1), "error is in the second file");

        // Emitter error (non-ASCII identifier — transliteration is Phase C)
        // inside the second file.
        let files = [
            parse("module A {\n}\n"),
            parse("module B {\n  out மணி: bits[4]\n  மணி = 0\n}\n"),
        ];
        let project = Project::from_files(&files).unwrap();
        let diags = emit(&project, &files).expect_err("non-ASCII identifier unsupported");
        assert_eq!(diags[0].file, Some(1), "error is in the second file");
    }
}
