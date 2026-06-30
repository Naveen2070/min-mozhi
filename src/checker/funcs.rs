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

use crate::ast::{Expr, ExprKind, FuncDecl};

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
/// (walks `body` + each `local.value` for `FnCall` nodes). Does not recurse
/// into callees — the caller builds the full graph first and DFS-traverses it.
fn direct_callees(decl: &FuncDecl) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for local in &decl.locals {
        collect_calls(&local.value, &mut out);
    }
    collect_calls(&decl.body, &mut out);
    out.sort();
    out.dedup();
    out
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
        ExprKind::BundleLit(_) => todo!(),
    }
}
