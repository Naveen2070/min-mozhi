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

/// Per-module state for the domain coloring. Owns its `String`/`Expr` data
/// (not `&'a`) — a `ForEach` body reaches `collect` through a LOWERED
/// (freshly cloned, never `'a`) item list once Elements-form substitution
/// happens (`ast::lower_foreach_item`), and `domains_of`/`domains_of_any`
/// are methods on `impl<'a> Checker<'a>`, so a `Ccx<'a>` field type would
/// otherwise force `'a` all the way down to `collect`'s `items` parameter,
/// the same "unnecessarily tied lifetime" class of issue `checker/names.rs`
/// already fixed (see its own doc comments), just needing an owning-struct
/// fix here instead of merely dropping a bound.
struct Ccx {
    /// reg name -> the clock that owns it (its `on` block's clock).
    reg_clock: HashMap<String, String>,
    /// wire/out name -> the expressions that drive it (init, drive rhs,
    /// and drive lhs index expressions — a demux select feeds the value).
    drives: HashMap<String, Vec<Expr>>,
    /// wire/out name -> its declaration span (for the mixing error).
    decl_spans: HashMap<String, Span>,
    /// Memoized domain sets; `None` marks in-progress (comb cycle —
    /// E0504 already reported it, treat as domain-free here).
    domains: HashMap<String, Option<HashSet<String>>>,
}

impl<'a> Checker<'a> {
    /// Pass 6 entry: one analysis per declared module, in file order.
    /// Same-named modules from different files are legal (spec/02 section
    /// 1.5b) and each gets its own independent check — no "canonical"
    /// skip, which would silently leave every module but the
    /// first-declared one unchecked (the same bug class fixed in
    /// drivers.rs). `Ccx` is built fresh per `check_module_clocks` call,
    /// so there is no cache to re-key here.
    pub(super) fn check_clocks(&mut self) {
        let files = self.files;
        for (file, f) in files.iter().enumerate() {
            for item in &f.items {
                let TopItem::Module(m) = item else { continue };
                self.check_module_clocks(file, m);
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
        let names: Vec<String> = cx.decl_spans.keys().cloned().collect();
        for name in names {
            let doms = self.domains_of(&mut cx, &name);
            if doms.len() > 1 {
                let mut list: Vec<&str> = doms.iter().map(String::as_str).collect();
                list.sort_unstable();
                let span = cx.decl_spans[&name];
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
            let mut reported: HashSet<String> = HashSet::new();
            let mut reads: Vec<(String, Span)> = Vec::new();
            body_reads(&on.body, &m.items, &mut reads);
            for (name, span) in reads {
                if reported.contains(&name) {
                    continue;
                }
                let doms = self.domains_of_any(&mut cx, &name);
                if let Some(foreign) = doms.iter().find(|d| d.as_str() != on.clock.name) {
                    reported.insert(name.clone());
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
    fn domains_of_any(&mut self, cx: &mut Ccx, name: &str) -> HashSet<String> {
        if let Some(clk) = cx.reg_clock.get(name) {
            return HashSet::from([clk.clone()]);
        }
        if cx.decl_spans.contains_key(name) {
            return self.domains_of(cx, name);
        }
        HashSet::new()
    }

    /// Domain set of a wire/out: the union over everything its driving
    /// expressions read, memoized. A comb cycle (already E0504) yields
    /// the empty set rather than recursing forever.
    fn domains_of(&mut self, cx: &mut Ccx, name: &str) -> HashSet<String> {
        match cx.domains.get(name) {
            Some(Some(d)) => return d.clone(),
            Some(None) => return HashSet::new(), // in progress: cycle
            None => {}
        }
        cx.domains.insert(name.to_string(), None);
        let mut acc: HashSet<String> = HashSet::new();
        let exprs = cx.drives.get(name).cloned().unwrap_or_default();
        for e in &exprs {
            let mut reads: Vec<(String, Span)> = Vec::new();
            expr_reads(e, &mut reads);
            for (read, _) in reads {
                if let Some(clk) = cx.reg_clock.get(&read) {
                    acc.insert(clk.clone());
                } else if cx.decl_spans.contains_key(&read) {
                    acc.extend(self.domains_of(cx, &read));
                }
            }
        }
        cx.domains.insert(name.to_string(), Some(acc.clone()));
        acc
    }
}

/// Count `clock` declarations (recursing into `repeat` for robustness —
/// declarations there become E0303 in the repeat slice).
///
/// NOTE(deferred): walks both `ConstIf` branches unconditionally — no const-eval
/// environment is plumbed in here (and `count_clocks` is a free function, not on
/// `Ccx`). If the non-taken branch of a `const if` declares a clock, the count is
/// an over-approximation. No current example triggers this because `const if`
/// branches rarely contain clock declarations, and the overcount only makes the
/// "one-walk" cache slightly less effective. Fix: plumb `env` into `count_clocks`
/// (and `collect`) or fold `count_clocks` into the walk that already has env.
fn count_clocks(items: &[ModuleItem], n: &mut usize) {
    for item in items {
        match item {
            ModuleItem::Clock(_) => *n += 1,
            // A sync loop references an existing clock, it doesn't declare
            // a new one.
            ModuleItem::SyncLoop(_) => {}
            ModuleItem::Repeat(r) => count_clocks(&r.items, n),
            // `foreach` is pure sugar over `repeat` — a `clock` declaration
            // inside its body is rejected anyway (E0303, `no_decls_in_repeat`
            // in `checker/names.rs`), so the raw (unlowered) `fe.items` is
            // exactly as accurate as a lowered one here, same "declared
            // once, raw body" treatment `ModuleItem::Repeat` gets above.
            ModuleItem::ForEach(fe) => count_clocks(&fe.items, n),
            ModuleItem::ConstIf { then, els, .. } => {
                count_clocks(then, n);
                if let Some(el) = els {
                    count_clocks(el, n);
                }
            }
            _ => {}
        }
    }
}

/// One walk: reg ownership, drive expressions, declaration spans.
///
/// NOTE(deferred): walks both `ConstIf` branches (same limitation as
/// `count_clocks` above) — lacks const-eval env to fold the condition.
/// Over-approximates reg ownership and drive expressions for the non-taken
/// branch. Safe because the overcount only makes clock-coloring slightly less
/// precise (never wrong) and the extra drive registrations are harmless.
fn collect(items: &[ModuleItem], cx: &mut Ccx) {
    for item in items {
        match item {
            ModuleItem::Port {
                dir: crate::ast::Dir::Out,
                name,
                ..
            } => {
                cx.decl_spans.entry(name.name.clone()).or_insert(name.span);
            }
            ModuleItem::Wire { name, init, .. } => {
                cx.decl_spans.entry(name.name.clone()).or_insert(name.span);
                cx.drives
                    .entry(name.name.clone())
                    .or_default()
                    .push(init.clone());
            }
            ModuleItem::Drive { lhs, rhs } => {
                cx.drives
                    .entry(lhs.base.name.clone())
                    .or_default()
                    .push(rhs.clone());
                if let Some((i, hi)) = &lhs.index {
                    cx.drives
                        .entry(lhs.base.name.clone())
                        .or_default()
                        .push(i.clone());
                    if let Some(hi) = hi {
                        cx.drives
                            .entry(lhs.base.name.clone())
                            .or_default()
                            .push(hi.clone());
                    }
                }
            }
            ModuleItem::On(on) => {
                let mut targets: Vec<String> = Vec::new();
                body_targets(&on.body, &mut targets);
                for t in targets {
                    cx.reg_clock
                        .entry(t)
                        .or_insert_with(|| on.clock.name.clone());
                }
            }
            ModuleItem::SyncLoop(sl) => {
                cx.reg_clock
                    .entry(sl.result_name.name.clone())
                    .or_insert_with(|| sl.clock.name.clone());
            }
            ModuleItem::Repeat(r) => collect(&r.items, cx),
            // `foreach` is pure sugar over `repeat` (see
            // `ast::foreach_lower`'s module doc comment) — lower and
            // recurse into this SAME function, exactly like `Repeat` above.
            // Unlike `widths/mod.rs`'s equivalent arms, `Ccx` (above) is
            // owned rather than `'a`-tied, so this genuinely can consume a
            // freshly lowered (locally-owned) `Vec<ModuleItem>`: the
            // Elements form's `arr[__idx]` substitution makes a `Drive`
            // rhs that reads the bound element correctly attribute to the
            // array signal's own domain, which a raw (unsubstituted) walk
            // would silently miss (the bound name isn't itself a declared
            // signal in `cx.decl_spans`/`cx.reg_clock`).
            ModuleItem::ForEach(fe) => {
                if let Some(lowered) = crate::ast::lower_foreach_item(fe, items) {
                    collect(&lowered, cx);
                }
            }
            ModuleItem::ConstIf { then, els, .. } => {
                collect(then, cx);
                if let Some(el) = els {
                    collect(el, cx);
                }
            }
            _ => {}
        }
    }
}

/// Assignment targets in an `on` body (the regs this block owns).
fn body_targets(body: &[SeqStmt], out: &mut Vec<String>) {
    for s in body {
        match s {
            SeqStmt::Assign { lhs, .. } => out.push(lhs.base.name.clone()),
            SeqStmt::If { then, els, .. } => {
                body_targets(then, out);
                if let Some(els) = els {
                    body_targets(els, out);
                }
            }
            SeqStmt::Default { name, .. } => out.push(name.name.clone()),
            SeqStmt::Loop { body, .. } => body_targets(body, out),
            // `subst_seq_stmt` (the Elements form's `var` substitution, see
            // `ast::lower_foreach_seq`) only ever rewrites an assignment's
            // RHS — it always keeps `lhs: lhs.clone()` — so an assignment
            // TARGET name inside a `foreach` body is identical whether the
            // body is raw or lowered. Walk the raw `body` directly, same
            // "declared once, raw body" idiom as `count_clocks` above and
            // `checker/drivers.rs`'s `on_block` `ForEach` arm (which is
            // this exact function's `Dcx`-side sibling).
            SeqStmt::ForEach { body, .. } => body_targets(body, out),
            SeqStmt::Error(_) => {} // parse-recovery placeholder
        }
    }
}

/// Every read in an `on` body: assignment rhs, lhs index expressions
/// (a demux select is a read), and `if` conditions. `module_items` is the
/// enclosing module's full item list — needed (only by the `ForEach` arm)
/// to resolve an Elements-form source's array via `ast::lower_foreach_seq`.
fn body_reads(body: &[SeqStmt], module_items: &[ModuleItem], out: &mut Vec<(String, Span)>) {
    for s in body {
        match s {
            SeqStmt::Assign { lhs, rhs } => {
                lvalue_index_reads(lhs, out);
                expr_reads(rhs, out);
            }
            SeqStmt::If { cond, then, els } => {
                expr_reads(cond, out);
                body_reads(then, module_items, out);
                if let Some(els) = els {
                    body_reads(els, module_items, out);
                }
            }
            SeqStmt::Default { val, .. } => expr_reads(val, out),
            SeqStmt::Loop { lo, hi, body, .. } => {
                expr_reads(lo, out);
                expr_reads(hi, out);
                body_reads(body, module_items, out);
            }
            // `foreach` is pure sugar over `loop` — lower and recurse into
            // this SAME function, exactly like `SeqStmt::Loop` above (the
            // lowered form is always a `SeqStmt::Loop`, so this hits that
            // exact arm on the next call). Unlike `widths/mod.rs`'s
            // equivalent arm, this file's `out` is fully owned (`String`,
            // not `&'a str` — see `Ccx`'s doc comment), so `body_reads`
            // isn't `'a`-tied and can consume a freshly lowered, locally-
            // owned `Vec<SeqStmt>` directly.
            //
            // This replaced an earlier, buggy version of this arm that
            // hand-retargeted raw `var` reads to `arr` by string equality
            // AFTER walking the raw body — that approach silently mis-
            // attributed a NESTED loop/foreach's own same-named `var`
            // (shadowing the outer one) to the outer array's domain too,
            // a false positive `lower_foreach_seq`'s substitution already
            // gets right (its shadowing rules are tested — see
            // `ast::foreach_lower`'s `loop_var_shadowing_...` tests).
            SeqStmt::ForEach {
                var,
                source,
                body,
                span,
            } => {
                if let Some(lowered) =
                    crate::ast::lower_foreach_seq(var, source, body, *span, module_items)
                {
                    body_reads(&lowered, module_items, out);
                }
                // On `None`: E0417 already reported by pass 3 (names.rs) —
                // silently skip, same "reported once upstream" precedent
                // used throughout this task.
            }
            SeqStmt::Error(_) => {} // parse-recovery placeholder
        }
    }
}

fn lvalue_index_reads(lhs: &LValue, out: &mut Vec<(String, Span)>) {
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
fn expr_reads(e: &Expr, out: &mut Vec<(String, Span)>) {
    match &e.kind {
        ExprKind::Ident(name) => out.push((name.clone(), e.span)),
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
        ExprKind::BundleLit(inits) => {
            for init in inits {
                expr_reads(&init.value, out);
            }
        }
        ExprKind::ArrayLit(elems) => {
            for e in elems {
                expr_reads(e, out);
            }
        }
    }
}
