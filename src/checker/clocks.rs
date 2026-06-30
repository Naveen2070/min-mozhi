//! Pass 6 — clock-domain ownership (E0701, spec/02 section 6 rule 5).
//!
//! Every reg is owned by exactly one clock (its single `on` block's —
//! E0503 already enforces the single block). This pass colors every
//! combinational signal with the set of clock domains it derives from
//! and rejects, PER MODULE:
//!
//! - a read inside `on rise(clkB)` of anything derived from a different
//!   clock's reg (direct, or through any chain of wires), and
//! - a wire/out that combinationally mixes two domains.
//!
//! Cross-domain data movement needs the explicit `sync` construct
//! (Phase 2) — until then it is a compile error, never a silent
//! metastability hazard.
//!
//! Scope (the honest list): analysis is per-module — instance outputs
//! contribute NO domain, so a domain crossing routed through a child
//! module's combinational path is not seen (deferred-table row in
//! docs/code/11-checker.md). Single-clock modules trivially pass.

use std::collections::{HashMap, HashSet};

use crate::ast::{Expr, ExprKind, LValue, Module, ModuleItem, SeqStmt, TopItem};
use crate::span::Span;

use super::Checker;

/// Per-module state for the domain coloring.
struct Ccx<'a> {
    /// reg name -> the clock that owns it (its `on` block's clock).
    reg_clock: HashMap<&'a str, &'a str>,
    /// wire/out name -> the expressions that drive it (init, drive rhs,
    /// and drive lhs index expressions — a demux select feeds the value).
    drives: HashMap<&'a str, Vec<&'a Expr>>,
    /// wire/out name -> its declaration span (for the mixing error).
    decl_spans: HashMap<&'a str, Span>,
    /// Memoized domain sets; `None` marks in-progress (comb cycle —
    /// E0504 already reported it, treat as domain-free here).
    domains: HashMap<&'a str, Option<HashSet<&'a str>>>,
}

impl<'a> Checker<'a> {
    /// Pass 6 entry: one analysis per canonical module, in file order.
    pub(super) fn check_clocks(&mut self) {
        let files = self.files;
        for (file, f) in files.iter().enumerate() {
            for item in &f.items {
                let TopItem::Module(m) = item else { continue };
                let canonical = self
                    .modules
                    .get(&m.name.name)
                    .is_some_and(|&(_, c)| std::ptr::eq(c, m));
                if canonical {
                    self.check_module_clocks(file, m);
                }
            }
        }
    }

    fn check_module_clocks(&mut self, file: usize, m: &'a Module) {
        // Fast path: fewer than two clocks means every domain agrees.
        let mut clocks = 0usize;
        count_clocks(&m.items, &mut clocks);
        if clocks < 2 {
            return;
        }

        let mut cx = Ccx {
            reg_clock: HashMap::new(),
            drives: HashMap::new(),
            decl_spans: HashMap::new(),
            domains: HashMap::new(),
        };
        collect(&m.items, &mut cx);

        // A wire/out whose own derivation mixes two domains is already a
        // crossing, before anyone reads it.
        let names: Vec<&str> = cx.decl_spans.keys().copied().collect();
        for name in names {
            let doms = self.domains_of(&mut cx, name);
            if doms.len() > 1 {
                let mut list: Vec<&str> = doms.iter().copied().collect();
                list.sort_unstable();
                let span = cx.decl_spans[name];
                self.err(
                    file,
                    span,
                    "E0701",
                    format!("`{name}` mixes the clock domains `{}`", list.join("`, `")),
                    "a combinational signal must derive from one clock — \
                     cross-domain data needs the explicit `sync` construct \
                     (Phase 2); until then keep each domain's logic separate",
                );
            }
        }

        // Reads inside an `on` block must stay in that block's domain.
        for item in &m.items {
            let ModuleItem::On(on) = item else { continue };
            let mut reported: HashSet<&str> = HashSet::new();
            let mut reads: Vec<(&'a str, Span)> = Vec::new();
            body_reads(&on.body, &mut reads);
            for (name, span) in reads {
                if reported.contains(name) {
                    continue;
                }
                let doms = self.domains_of_any(&mut cx, name);
                if let Some(foreign) = doms.iter().find(|&&d| d != on.clock.name) {
                    reported.insert(name);
                    self.err(
                        file,
                        span,
                        "E0701",
                        format!(
                            "`{name}` belongs to clock `{foreign}`, but this \
                             block runs on `{}`",
                            on.clock.name
                        ),
                        "reading across clock domains causes metastability — \
                         it needs the explicit `sync` construct (Phase 2); \
                         until then move this logic into the owning clock's \
                         `on` block",
                    );
                }
            }
        }
    }

    /// Domain set of a NAME of any kind: a reg's is its owning clock;
    /// wires/outs are computed; everything else is domain-free.
    fn domains_of_any(&mut self, cx: &mut Ccx<'a>, name: &'a str) -> HashSet<&'a str> {
        if let Some(&clk) = cx.reg_clock.get(name) {
            return HashSet::from([clk]);
        }
        if cx.decl_spans.contains_key(name) {
            return self.domains_of(cx, name);
        }
        HashSet::new()
    }

    /// Domain set of a wire/out: the union over everything its driving
    /// expressions read, memoized. A comb cycle (already E0504) yields
    /// the empty set rather than recursing forever.
    fn domains_of(&mut self, cx: &mut Ccx<'a>, name: &'a str) -> HashSet<&'a str> {
        match cx.domains.get(name) {
            Some(Some(d)) => return d.clone(),
            Some(None) => return HashSet::new(), // in progress: cycle
            None => {}
        }
        cx.domains.insert(name, None);
        let mut acc: HashSet<&'a str> = HashSet::new();
        let exprs = cx.drives.get(name).cloned().unwrap_or_default();
        for e in exprs {
            let mut reads: Vec<(&'a str, Span)> = Vec::new();
            expr_reads(e, &mut reads);
            for (read, _) in reads {
                if let Some(&clk) = cx.reg_clock.get(read) {
                    acc.insert(clk);
                } else if cx.decl_spans.contains_key(read) {
                    acc.extend(self.domains_of(cx, read));
                }
            }
        }
        cx.domains.insert(name, Some(acc.clone()));
        acc
    }
}

/// Count `clock` declarations (recursing into `repeat` for robustness —
/// declarations there become E0303 in the repeat slice).
fn count_clocks(items: &[ModuleItem], n: &mut usize) {
    for item in items {
        match item {
            ModuleItem::Clock(_) => *n += 1,
            ModuleItem::Repeat(r) => count_clocks(&r.items, n),
            _ => {}
        }
    }
}

/// One walk: reg ownership, drive expressions, declaration spans.
fn collect<'a>(items: &'a [ModuleItem], cx: &mut Ccx<'a>) {
    for item in items {
        match item {
            ModuleItem::Port {
                dir: crate::ast::Dir::Out,
                name,
                ..
            } => {
                cx.decl_spans.entry(&name.name).or_insert(name.span);
            }
            ModuleItem::Wire { name, init, .. } => {
                cx.decl_spans.entry(&name.name).or_insert(name.span);
                cx.drives.entry(&name.name).or_default().push(init);
            }
            ModuleItem::Drive { lhs, rhs } => {
                cx.drives.entry(&lhs.base.name).or_default().push(rhs);
                if let Some((i, hi)) = &lhs.index {
                    cx.drives.entry(&lhs.base.name).or_default().push(i);
                    if let Some(hi) = hi {
                        cx.drives.entry(&lhs.base.name).or_default().push(hi);
                    }
                }
            }
            ModuleItem::On(on) => {
                let mut targets: Vec<&'a str> = Vec::new();
                body_targets(&on.body, &mut targets);
                for t in targets {
                    cx.reg_clock.entry(t).or_insert(&on.clock.name);
                }
            }
            ModuleItem::Repeat(r) => collect(&r.items, cx),
            _ => {}
        }
    }
}

/// Assignment targets in an `on` body (the regs this block owns).
fn body_targets<'a>(body: &'a [SeqStmt], out: &mut Vec<&'a str>) {
    for s in body {
        match s {
            SeqStmt::Assign { lhs, .. } => out.push(&lhs.base.name),
            SeqStmt::If { then, els, .. } => {
                body_targets(then, out);
                if let Some(els) = els {
                    body_targets(els, out);
                }
            }
            SeqStmt::Default { .. } => todo!("default not yet implemented"),
            SeqStmt::Error(_) => {} // parse-recovery placeholder
        }
    }
}

/// Every read in an `on` body: assignment rhs, lhs index expressions
/// (a demux select is a read), and `if` conditions.
fn body_reads<'a>(body: &'a [SeqStmt], out: &mut Vec<(&'a str, Span)>) {
    for s in body {
        match s {
            SeqStmt::Assign { lhs, rhs } => {
                lvalue_index_reads(lhs, out);
                expr_reads(rhs, out);
            }
            SeqStmt::If { cond, then, els } => {
                expr_reads(cond, out);
                body_reads(then, out);
                if let Some(els) = els {
                    body_reads(els, out);
                }
            }
            SeqStmt::Default { .. } => todo!("default not yet implemented"),
            SeqStmt::Error(_) => {} // parse-recovery placeholder
        }
    }
}

fn lvalue_index_reads<'a>(lhs: &'a LValue, out: &mut Vec<(&'a str, Span)>) {
    if let Some((i, hi)) = &lhs.index {
        expr_reads(i, out);
        if let Some(hi) = hi {
            expr_reads(hi, out);
        }
    }
}

/// Every name an expression reads, with the span to point the error at.
/// Instance outputs (`inst.out`) contribute nothing — cross-instance
/// domain tracking is deferred (module-local analysis).
fn expr_reads<'a>(e: &'a Expr, out: &mut Vec<(&'a str, Span)>) {
    match &e.kind {
        ExprKind::Ident(name) => out.push((name, e.span)),
        ExprKind::Field { .. } => {} // Enum.Variant or inst.out: no domain
        ExprKind::Int { .. } | ExprKind::Bool(_) => {}
        ExprKind::Unary { expr, .. } => expr_reads(expr, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            expr_reads(lhs, out);
            expr_reads(rhs, out);
        }
        ExprKind::IfExpr { cond, then, els } => {
            expr_reads(cond, out);
            expr_reads(then, out);
            expr_reads(els, out);
        }
        ExprKind::Match { scrutinee, arms } => {
            expr_reads(scrutinee, out);
            for arm in arms {
                expr_reads(&arm.value, out);
            }
        }
        ExprKind::Concat(parts) => {
            for p in parts {
                expr_reads(p, out);
            }
        }
        ExprKind::Replicate { count, parts } => {
            expr_reads(count, out);
            for p in parts {
                expr_reads(p, out);
            }
        }
        ExprKind::Index { base, index } => {
            expr_reads(base, out);
            expr_reads(index, out);
        }
        ExprKind::Slice { base, hi, lo } => {
            expr_reads(base, out);
            expr_reads(hi, out);
            expr_reads(lo, out);
        }
        ExprKind::Call { args, .. } => {
            for a in args {
                expr_reads(a, out);
            }
        }
        ExprKind::FnCall { args, .. } => {
            for a in args {
                expr_reads(a, out);
            }
        }
    }
}
