//! Pass: ban recursive functions (E0805).
//!
//! Builds a directed call-graph over the project's `fn` declarations and runs
//! a DFS to detect back-edges (cycles). Any function that is part of a
//! recursive cycle — direct (`fn a { a(...) }`) or mutual (`a → b → a`) —
//! gets E0805 at its name span.
//!
//! Hardware context: `fn` bodies are purely combinational and inlined at every
//! call site. Unbounded recursion would require infinite unrolling, which no
//! synthesis tool can handle. The fix is fixed-size repetition or a
//! parameterized module.

use std::collections::{HashMap, HashSet};

use crate::ast::{Expr, ExprKind, FnStmt, FuncDecl};
use crate::span::Span;

use super::Checker;

impl<'a> Checker<'a> {
    /// Detect call-graph cycles across all registered user functions (E0805).
    ///
    /// Called immediately after `build_symbols` so `self.funcs` is fully
    /// populated. Collects errors into `self.diags`; never panics.
    pub(super) fn check_func_cycles(&mut self) {
        // Build an owned call-graph so the DFS can borrow it without
        // lifetime entanglement with `self.funcs`.
        let graph: HashMap<String, Vec<String>> = self
            .funcs
            .iter()
            .map(|(name, (_, decl))| (name.clone(), direct_callees(decl)))
            .collect();

        let mut visited: HashSet<String> = HashSet::new();
        let mut culprits: Vec<String> = Vec::new();

        // Iteration order over a HashMap is not deterministic; sort for
        // stable diagnostic output (matches test expectations).
        let mut starts: Vec<String> = graph.keys().cloned().collect();
        starts.sort();

        for start in &starts {
            if !visited.contains(start) {
                let mut on_path: HashSet<String> = HashSet::new();
                dfs(start, &graph, &mut on_path, &mut visited, &mut culprits);
            }
        }

        // Emit one E0805 per culprit function; order is determined by DFS.
        for name in &culprits {
            if let Some((file, decl)) = self.funcs.get(name) {
                let file = *file;
                let span = decl.name.span;
                self.err(
                    file,
                    span,
                    "E0805",
                    format!("function `{name}` is part of a recursive cycle"),
                    "hardware cannot unroll unbounded recursion — \
                     restructure using fixed-size repetition or parameterized \
                     modules instead of recursion",
                );
            }
        }
    }

    /// E0812: an unconditional `return` followed by more statements in the
    /// SAME block is dead code. Deliberately narrow — an `if`/`else` where
    /// BOTH branches return, followed by more code, is NOT flagged (that
    /// would need full control-flow reachability analysis, which this
    /// check does not attempt; see the design spec's Checker section).
    pub(super) fn check_func_unreachable(&mut self) {
        // Sort for deterministic diagnostic order (matches test expectations,
        // same rationale as `check_func_cycles`'s sorted `starts`).
        let mut names: Vec<String> = self.funcs.keys().cloned().collect();
        names.sort();
        for name in names {
            let (file, decl) = self.funcs[&name];
            check_unreachable_after_return(&decl.stmts, file, self);
        }
    }
}

/// Walk one statement list looking for a `return` with more statements
/// after it in the SAME list. Recurses into `if`/`else` bodies (each is its
/// own list, checked independently) but does NOT look past an `if` to see
/// whether both its branches returned — that's the documented narrow scope.
fn check_unreachable_after_return(stmts: &[FnStmt], file: usize, ck: &mut Checker) {
    for (i, stmt) in stmts.iter().enumerate() {
        match stmt {
            FnStmt::Return(_) => {
                if let Some(next) = stmts.get(i + 1) {
                    let span = next_stmt_span(next);
                    ck.err(
                        file,
                        span,
                        "E0812",
                        "unreachable code after `return`",
                        "a `return` immediately ends the function on this path — \
                         remove the statement(s) after it, or move `return` later \
                         if it was meant to be conditional",
                    );
                }
                return; // only report once per list — the first return is what matters
            }
            FnStmt::If { then, els, .. } => {
                check_unreachable_after_return(then, file, ck);
                if let Some(els) = els {
                    check_unreachable_after_return(els, file, ck);
                }
            }
            FnStmt::Loop { body, .. } => {
                check_unreachable_after_return(body, file, ck);
            }
            // `foreach` is pure sugar over `loop` (see `ast::foreach_lower`'s
            // module doc comment), but this specific check needs no
            // `ast::lower_foreach_fn` call at all: dead-code-after-`return`
            // is a purely structural property of the statement list, and
            // `lower_foreach_fn`'s Elements-form substitution only ever
            // rewrites an `Ident(var)` read into `Index{arr, idx}` — it adds
            // no `Return`/`If`/`Loop` nodes and removes none, so walking the
            // RAW (unlowered) `body` finds exactly the same unreachable
            // statements a lowered walk would. This also means it's correct
            // to recurse unconditionally here, unlike `names.rs`/`widths`'s
            // arms: gating on `lower_foreach_fn` returning `Some` (i.e.
            // skipping when the Elements-form array doesn't resolve, E0417)
            // would wrongly suppress a genuine E0812 inside a body whose
            // `foreach` source happens to be invalid — an unrelated concern.
            FnStmt::ForEach { body, .. } => {
                check_unreachable_after_return(body, file, ck);
            }
            FnStmt::Let(_) | FnStmt::Error(_) => {}
        }
    }
}

/// The span to point E0812 at — the first unreachable statement's own span.
fn next_stmt_span(stmt: &FnStmt) -> Span {
    match stmt {
        FnStmt::Let(l) => l.span,
        FnStmt::If { cond, .. } => cond.span,
        FnStmt::Return(e) => e.span,
        FnStmt::Loop { span, .. } => *span,
        FnStmt::ForEach { span, .. } => *span,
        FnStmt::Error(s) => *s,
    }
}

/// Iterative-DFS cycle detection (Tarjan-style coloring without SCCs).
/// `on_path` is the current DFS stack (gray set); `visited` is the fully
/// processed set (black set). Any callee found in `on_path` is a back-edge
/// and its name is pushed to `culprits` (at most once).
fn dfs(
    node: &str,
    graph: &HashMap<String, Vec<String>>,
    on_path: &mut HashSet<String>,
    visited: &mut HashSet<String>,
    culprits: &mut Vec<String>,
) {
    on_path.insert(node.to_owned());

    if let Some(callees) = graph.get(node) {
        for callee in callees {
            if on_path.contains(callee.as_str()) {
                // Back-edge: callee is on the current stack → cycle.
                if !culprits.contains(callee) {
                    culprits.push(callee.clone());
                }
            } else if !visited.contains(callee.as_str()) && graph.contains_key(callee.as_str()) {
                dfs(callee, graph, on_path, visited, culprits);
            }
        }
    }

    on_path.remove(node);
    visited.insert(node.to_owned());
}

/// Collect the names of all user-defined functions called directly by `decl`
/// (walks every statement + the tail for `FnCall` nodes). Does not recurse
/// into callees — the caller builds the full graph first and DFS-traverses it.
fn direct_callees(decl: &FuncDecl) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    collect_fn_stmt_calls(&decl.stmts, &mut out);
    collect_calls(&decl.tail, &mut out);
    out.sort();
    out.dedup();
    out
}

/// Walk a `fn`-body statement list, pushing every distinct user-function
/// name called anywhere inside (a `let` value, an `if` condition/branch, or
/// a `return` expression) into `out`.
fn collect_fn_stmt_calls(stmts: &[FnStmt], out: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            FnStmt::Let(local) => collect_calls(&local.value, out),
            FnStmt::If { cond, then, els } => {
                collect_calls(cond, out);
                collect_fn_stmt_calls(then, out);
                if let Some(els) = els {
                    collect_fn_stmt_calls(els, out);
                }
            }
            FnStmt::Return(expr) => collect_calls(expr, out),
            FnStmt::Loop { lo, hi, body, .. } => {
                collect_calls(lo, out);
                collect_calls(hi, out);
                collect_fn_stmt_calls(body, out);
            }
            // Same "no lowering needed" reasoning as
            // `check_unreachable_after_return`'s `ForEach` arm above: an
            // `ExprKind::FnCall` node is neither introduced nor removed by
            // `lower_foreach_fn`'s `Ident(var)` -> `Index{arr, idx}`
            // substitution, so the raw body yields the identical call set.
            // For the Range form, `source`'s `lo`/`hi` are walked too,
            // mirroring `FnStmt::Loop` just above.
            FnStmt::ForEach { source, body, .. } => {
                if let crate::ast::ForEachSource::Range { lo, hi } = source {
                    collect_calls(lo, out);
                    collect_calls(hi, out);
                }
                collect_fn_stmt_calls(body, out);
            }
            FnStmt::Error(_) => {}
        }
    }
}

/// Walk an expression, pushing each distinct user-function name found in an
/// `ExprKind::FnCall` node into `out`. Mirrors the FnCall arms in
/// `drivers.rs` and `clocks.rs` (which also walk args for graph edges).
fn collect_calls(expr: &Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::FnCall { name, args } => {
            // note: Vec::contains is O(n) but fn counts are tiny
            if !out.contains(&name.name) {
                out.push(name.name.clone());
            }
            for a in args {
                collect_calls(a, out);
            }
        }
        ExprKind::Unary { expr: e, .. } => collect_calls(e, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_calls(lhs, out);
            collect_calls(rhs, out);
        }
        ExprKind::IfExpr { cond, then, els } => {
            collect_calls(cond, out);
            collect_calls(then, out);
            collect_calls(els, out);
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_calls(scrutinee, out);
            for arm in arms {
                collect_calls(&arm.value, out);
            }
        }
        ExprKind::Concat(parts) => {
            for p in parts {
                collect_calls(p, out);
            }
        }
        ExprKind::Replicate { count, parts } => {
            collect_calls(count, out);
            for p in parts {
                collect_calls(p, out);
            }
        }
        ExprKind::Index { base, index } => {
            collect_calls(base, out);
            collect_calls(index, out);
        }
        ExprKind::Slice { base, hi, lo } => {
            collect_calls(base, out);
            collect_calls(hi, out);
            collect_calls(lo, out);
        }
        ExprKind::Call { args, .. } => {
            for a in args {
                collect_calls(a, out);
            }
        }
        ExprKind::Field { base, .. } => collect_calls(base, out),
        // Leaves: no sub-expressions, no calls.
        ExprKind::Int { .. } | ExprKind::Bool(_) | ExprKind::Ident(_) => {}
        ExprKind::BundleLit(inits) => {
            for init in inits {
                collect_calls(&init.value, out);
            }
        }
        ExprKind::ArrayLit(elems) => {
            for e in elems {
                collect_calls(e, out);
            }
        }
        ExprKind::EnumConstruct { args, .. } => {
            for a in args {
                collect_calls(a, out);
            }
        }
    }
}
