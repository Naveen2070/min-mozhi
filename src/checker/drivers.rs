//! Pass 5 — single-driver and combinational-cycle rules (E0501–E0505).
//!
//! Enforces spec/02 section 6 rules 1 and 6: every `out`/`wire` is driven
//! exactly once (disjoint constant bit-ranges count as different targets,
//! so the Chaser idiom `led[i] = ...` per `repeat` iteration is legal),
//! every `reg` is assigned from exactly one `on` block, and the
//! combinational graph — including paths THROUGH child instances — is a
//! DAG. It also owns the assignment-kind rule (E0505): `<-` is for regs,
//! `=` is for wires/outs.
//!
//! Runs ONCE per module (driver structure is parameter-independent); the
//! default parameter binding supplies values for constant indices and
//! widths. Without a binding (defaultless params), indexed drives degrade
//! to `Extent::Unknown` — never a false conflict, never counted as
//! coverage.
//!
//! Through-instance cycles use a per-module **combinational summary**
//! (which outputs depend on which inputs), computed from the same graph
//! by reachability and memoized. Recursive instantiation yields an empty
//! summary (documented limitation, see docs/code/11-checker.md).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{
    Conn, Dir, Expr, ExprKind, Ident, Inst, LValue, Module, ModuleItem, SeqStmt, TopItem, Type,
};
use crate::span::Span;

use super::Checker;
use super::consteval::{self, Env};
use super::names::{Bind, Scope};

/// Total `repeat` iterations walked per module before degrading to one
/// unbound walk (keeps checking O(small) for huge generate ranges).
const REPEAT_BUDGET: i128 = 4096;

/// Which bits of a signal one drive statement claims.
#[derive(Clone, Copy)]
enum Extent {
    /// The whole signal (`y = ...`).
    Whole,
    /// `[lo, hi]` inclusive, constant bounds.
    Range(u128, u128),
    /// A runtime or out-of-budget index under a KNOWN binding — could be
    /// any bit, so it conflicts with everything (except `Unknown`).
    Dynamic,
    /// Unevaluable because the module has no parameter binding — never
    /// conflicts, never counts as coverage (no false positives).
    Unknown,
}

/// Do two drive sites claim an overlapping bit?
fn conflicts(a: &Extent, b: &Extent) -> bool {
    use Extent::*;
    match (a, b) {
        (Unknown, _) | (_, Unknown) => false,
        (Whole, _) | (_, Whole) => true,
        (Dynamic, _) | (_, Dynamic) => true,
        (Range(al, ah), Range(bl, bh)) => al <= bh && bl <= ah,
    }
}

/// One drive statement targeting (part of) a signal.
struct Site {
    extent: Extent,
    span: Span,
}

/// A node in the combinational graph. Instance outputs are pseudo-nodes
/// keyed by the constant array index (`fa[0].cout` and `fa[1].cout` are
/// DIFFERENT nodes — merging them would invent cycles in the legal
/// ripple-carry idiom).
#[derive(Clone, PartialEq, Eq, Hash)]
enum Node {
    Sig(String),
    InstOut {
        inst: String,
        index: Option<i128>,
        out: String,
    },
}

impl Node {
    fn show(&self) -> String {
        match self {
            Node::Sig(s) => s.clone(),
            Node::InstOut {
                inst,
                index: Some(i),
                out,
            } => format!("{inst}[{i}].{out}"),
            Node::InstOut {
                inst, index: None, ..
            } => format!("{inst}.{}", self.out_name()),
        }
    }
    fn out_name(&self) -> &str {
        match self {
            Node::Sig(s) => s,
            Node::InstOut { out, .. } => out,
        }
    }
}

/// `out port -> the input ports it depends on combinationally`.
type Summary = Rc<HashMap<String, HashSet<String>>>;

/// State for one module's driver analysis.
struct Dcx<'a> {
    file: usize,
    sc: Rc<Scope<'a>>,
    /// file consts + default param binding + module consts.
    env: Env,
    /// Whether a parameter binding exists (Unknown vs Dynamic extents).
    bound: bool,
    /// false while computing another module's summary — collect edges
    /// only, never report (the module's own check owns its errors).
    report: bool,
    sites: HashMap<String, Vec<Site>>,
    /// reg name -> `on`-block ids (the block's span start — stable per
    /// AST node, so per-iteration repeat walks don't invent new blocks).
    reg_blocks: HashMap<String, HashSet<usize>>,
    /// Signals already reported (or hit by E0505) — no cascades.
    poisoned: HashSet<String>,
    edges: HashMap<Node, HashSet<Node>>,
    /// Where each driven node was driven (for placing cycle errors).
    node_spans: HashMap<Node, Span>,
    /// Out ports and regs seen (declaration order), for E0502/E0503.
    outs: Vec<(&'a Ident, &'a Type)>,
    regs: Vec<&'a Ident>,
    /// Memories seen (declaration order), for the multi-writer check (E0503).
    /// Unlike regs, a memory with zero writers is a valid power-on-init ROM.
    mems: Vec<&'a Ident>,
    repeat_budget: i128,
}

impl<'a> Checker<'a> {
    /// Pass 5 entry: one analysis per canonical module, in file order.
    pub(super) fn check_drivers(&mut self) {
        let files = self.files;
        let mut summaries: HashMap<String, Summary> = HashMap::new();
        let mut in_progress: HashSet<String> = HashSet::new();
        for (file, f) in files.iter().enumerate() {
            for item in &f.items {
                let TopItem::Module(m) = item else { continue };
                let canonical = self
                    .modules
                    .get(&m.name.name)
                    .is_some_and(|&(_, c)| std::ptr::eq(c, m));
                if !canonical {
                    continue;
                }
                self.check_module_drivers(file, m, &mut summaries, &mut in_progress);
            }
        }
    }

    /// file consts + default binding (if any) + module consts. The bool
    /// says whether a binding exists (extent precision).
    fn driver_env(&mut self, file: usize, m: &'a Module) -> (Env, bool) {
        let mut env = self.file_consts[file].clone();
        let bound = match self.default_binding(file, m, false) {
            Some(binding) => {
                for (k, v) in binding {
                    env.insert(k, v);
                }
                true
            }
            None => m.params.is_empty(),
        };
        for item in &m.items {
            if let ModuleItem::Const(c) = item
                && let Ok(v) = consteval::eval(&c.value, &env)
            {
                env.insert(c.name.name.clone(), v);
            }
        }
        (env, bound)
    }

    fn check_module_drivers(
        &mut self,
        file: usize,
        m: &'a Module,
        summaries: &mut HashMap<String, Summary>,
        in_progress: &mut HashSet<String>,
    ) {
        let Some(sc) = self.scopes.get(&m.name.name).cloned() else {
            return;
        };
        let (env, bound) = self.driver_env(file, m);
        let mut dcx = Dcx {
            file,
            sc,
            env,
            bound,
            report: true,
            sites: HashMap::new(),
            reg_blocks: HashMap::new(),
            poisoned: HashSet::new(),
            edges: HashMap::new(),
            node_spans: HashMap::new(),
            outs: Vec::new(),
            regs: Vec::new(),
            mems: Vec::new(),
            repeat_budget: REPEAT_BUDGET,
        };
        self.collect_items(&mut dcx, &m.items, summaries, in_progress);
        self.report_conflicts(&mut dcx);
        self.report_coverage(&mut dcx);
        self.report_reg_blocks(&dcx);
        self.report_mem_blocks(&dcx);
        self.find_cycles(&mut dcx);
    }

    /// One walk collects drive sites, reg→on-block ownership, E0505
    /// kind violations, and the combinational edges.
    fn collect_items(
        &mut self,
        dcx: &mut Dcx<'a>,
        items: &'a [ModuleItem],
        summaries: &mut HashMap<String, Summary>,
        in_progress: &mut HashSet<String>,
    ) {
        for item in items {
            match item {
                ModuleItem::Port {
                    dir: Dir::Out,
                    name,
                    ty,
                } => {
                    if dcx.report && !dcx.outs.iter().any(|(n, _)| n.name == name.name) {
                        dcx.outs.push((name, ty));
                    }
                }
                ModuleItem::Reg { name, .. } => {
                    if dcx.report && !dcx.regs.iter().any(|n| n.name == name.name) {
                        dcx.regs.push(name);
                    }
                }
                ModuleItem::Mem { name, .. } => {
                    if dcx.report && !dcx.mems.iter().any(|n| n.name == name.name) {
                        dcx.mems.push(name);
                    }
                }
                ModuleItem::Wire { name, init, .. } => {
                    // The declaration IS the wire's one driver.
                    let node = Node::Sig(name.name.clone());
                    dcx.node_spans.entry(node.clone()).or_insert(name.span);
                    let mut reads = Vec::new();
                    self.expr_reads(dcx, init, &mut reads);
                    dcx.edges.entry(node).or_default().extend(reads);
                }
                ModuleItem::Drive { lhs, rhs } => self.drive(dcx, lhs, rhs),
                ModuleItem::On(on) => self.on_block(dcx, on.span.start, &on.body),
                ModuleItem::Inst(inst) => self.inst_edges(dcx, inst, summaries, in_progress),
                ModuleItem::Repeat(r) => {
                    let (Ok(lo), Ok(hi)) = (
                        consteval::eval(&r.lo, &dcx.env),
                        consteval::eval(&r.hi, &dcx.env),
                    ) else {
                        // Bounds unevaluable (reported by pass 3, or no
                        // binding): one unbound walk — extents degrade.
                        self.collect_items(dcx, &r.items, summaries, in_progress);
                        continue;
                    };
                    if hi - lo > dcx.repeat_budget {
                        // Over budget: one walk WITHOUT the loop variable,
                        // so indexed extents degrade to Dynamic/Unknown.
                        self.collect_items(dcx, &r.items, summaries, in_progress);
                        continue;
                    }
                    dcx.repeat_budget -= (hi - lo).max(0);
                    for v in lo..hi {
                        let shadowed = dcx.env.insert(r.var.name.clone(), v);
                        self.collect_items(dcx, &r.items, summaries, in_progress);
                        match shadowed {
                            Some(p) => dcx.env.insert(r.var.name.clone(), p),
                            None => dcx.env.remove(&r.var.name),
                        };
                    }
                }
                ModuleItem::ConstIf {
                    cond, then, els, ..
                } => {
                    let val = consteval::eval(cond, &dcx.env).unwrap_or(0);
                    let branch = if val != 0 {
                        then.as_slice()
                    } else {
                        els.as_deref().unwrap_or(&[])
                    };
                    self.collect_items(dcx, branch, summaries, in_progress);
                }
                ModuleItem::Port { .. }
                | ModuleItem::Clock(_)
                | ModuleItem::Reset { .. }
                | ModuleItem::Const(_)
                | ModuleItem::Enum(_)
                | ModuleItem::Error(_) => {}
                ModuleItem::BundleDestructure { .. } => {} // checker stub (T5)
            }
        }
    }

    /// A module-level `=` drive: site bookkeeping + E0501-on-wire +
    /// E0505-on-reg + combinational edges (the rhs AND the lhs indices —
    /// a demux select is a real input of the target).
    fn drive(&mut self, dcx: &mut Dcx<'a>, lhs: &'a LValue, rhs: &'a Expr) {
        let name = &lhs.base.name;
        match dcx.sc.names.get(name) {
            Some(Bind::Out) => {
                let extent = self.lvalue_extent(dcx, lhs);
                dcx.sites.entry(name.clone()).or_default().push(Site {
                    extent,
                    span: lhs.span,
                });
            }
            Some(Bind::Wire) => {
                if dcx.report && dcx.poisoned.insert(name.clone()) {
                    self.err(
                        dcx.file,
                        lhs.span,
                        "E0501",
                        format!("`{name}` already has a driver — its declaration"),
                        "a wire is driven exactly once, where it is declared \
                         (`wire name: type = expr`); to compute a different \
                         value, declare another wire (spec/02 section 6)",
                    );
                }
            }
            Some(Bind::Reg) => {
                if dcx.report && dcx.poisoned.insert(name.clone()) {
                    self.err(
                        dcx.file,
                        lhs.span,
                        "E0505",
                        format!("cannot drive reg `{name}` with `=`"),
                        "registers update with `<-` inside `on rise(clk)`; \
                         `=` is the combinational drive for wires and outputs \
                         (spec/02 section 1.2)",
                    );
                }
                return; // no edges/sites for a mis-kinded target
            }
            Some(Bind::Mem) => {
                if dcx.report && dcx.poisoned.insert(name.clone()) {
                    self.err(
                        dcx.file,
                        lhs.span,
                        "E0505",
                        format!("cannot write memory `{name}` with `=`"),
                        "memories are written with `<-` inside `on rise(clk)`; \
                         `=` is the combinational drive for wires and outputs \
                         (spec/02 section 1.2)",
                    );
                }
                return; // no edges/sites for a mis-kinded target
            }
            _ => return, // E0108/E0101 already reported by pass 3
        }
        let node = Node::Sig(name.clone());
        dcx.node_spans.entry(node.clone()).or_insert(lhs.span);
        let mut reads = Vec::new();
        self.expr_reads(dcx, rhs, &mut reads);
        if let Some((first, second)) = &lhs.index {
            self.expr_reads(dcx, first, &mut reads);
            if let Some(lo) = second {
                self.expr_reads(dcx, lo, &mut reads);
            }
        }
        dcx.edges.entry(node).or_default().extend(reads);
    }

    /// `on` body: reg ownership (E0503 bookkeeping) and the `<-`-to-
    /// combinational-signal kind error (E0505). Sequential assignments
    /// create NO combinational edges.
    fn on_block(&mut self, dcx: &mut Dcx<'a>, block_id: usize, body: &'a [SeqStmt]) {
        for s in body {
            match s {
                SeqStmt::Assign { lhs, .. } => {
                    let name = &lhs.base.name;
                    match dcx.sc.names.get(name) {
                        Some(Bind::Reg | Bind::Mem) => {
                            dcx.reg_blocks
                                .entry(name.clone())
                                .or_default()
                                .insert(block_id);
                        }
                        Some(b @ (Bind::Wire | Bind::Out))
                            if dcx.report && dcx.poisoned.insert(name.clone()) =>
                        {
                            let what = b.what();
                            self.err(
                                dcx.file,
                                lhs.span,
                                "E0505",
                                format!("cannot update `{name}` with `<-` — it is {what}"),
                                "`<-` is for registers inside `on` blocks; wires \
                                 and outputs are combinational — drive them once \
                                 with `=` at module level (spec/02 section 1.2)",
                            );
                        }
                        _ => {} // pass 3 owns the rest
                    }
                }
                SeqStmt::If { then, els, .. } => {
                    self.on_block(dcx, block_id, then);
                    if let Some(els) = els {
                        self.on_block(dcx, block_id, els);
                    }
                }
                SeqStmt::Default { name, .. } => {
                    if matches!(dcx.sc.names.get(&name.name), Some(Bind::Reg)) {
                        dcx.reg_blocks
                            .entry(name.name.clone())
                            .or_default()
                            .insert(block_id);
                    }
                }
                SeqStmt::Error(_) => {} // parse-recovery placeholder
            }
        }
    }

    /// One instantiation: pseudo-node edges. `inst.o` depends on the
    /// parent signals (and other instance outputs!) read by the conn
    /// exprs of every child input that `o` combinationally depends on.
    fn inst_edges(
        &mut self,
        dcx: &mut Dcx<'a>,
        inst: &'a Inst,
        summaries: &mut HashMap<String, Summary>,
        in_progress: &mut HashSet<String>,
    ) {
        let summary = self.comb_summary(&inst.module.name, summaries, in_progress);
        let index = inst
            .index
            .as_ref()
            .and_then(|e| consteval::eval(e, &dcx.env).ok());
        for (out, ins) in summary.iter() {
            let node = Node::InstOut {
                inst: inst.name.name.clone(),
                index,
                out: out.clone(),
            };
            dcx.node_spans.entry(node.clone()).or_insert(inst.span);
            let mut reads = Vec::new();
            for Conn { port, signal } in &inst.conns {
                if ins.contains(&port.name) {
                    self.expr_reads(dcx, signal, &mut reads);
                }
            }
            if !reads.is_empty() {
                dcx.edges.entry(node).or_default().extend(reads);
            }
        }
    }

    /// Every combinational read in `e`: wires, outs, INPUTS (terminal
    /// nodes — needed for the summaries), and instance outputs. Regs,
    /// clocks, resets, consts, params break combinational paths.
    fn expr_reads(&mut self, dcx: &mut Dcx<'a>, e: &'a Expr, out: &mut Vec<Node>) {
        match &e.kind {
            ExprKind::Int { .. } | ExprKind::Bool(_) => {}
            ExprKind::Ident(name) => {
                if let Some(Bind::Wire | Bind::Out | Bind::In) = dcx.sc.names.get(name) {
                    out.push(Node::Sig(name.clone()));
                }
            }
            ExprKind::Field { base, field } => {
                // `inst.out` / `inst[i].out` — same unwrap as pass 3.
                let (core, index) = match &base.kind {
                    ExprKind::Index { base: b, index } if matches!(b.kind, ExprKind::Ident(_)) => {
                        (b, Some(index.as_ref()))
                    }
                    _ => (base, None),
                };
                if let ExprKind::Ident(name) = &core.kind {
                    if let Some(Bind::Inst(_)) = dcx.sc.names.get(name) {
                        let idx = match index {
                            // Unevaluable array index: skip the edge
                            // (under-approximate — documented).
                            Some(e) => match consteval::eval(e, &dcx.env) {
                                Ok(v) => Some(v),
                                Err(_) => return,
                            },
                            None => None,
                        };
                        out.push(Node::InstOut {
                            inst: name.clone(),
                            index: idx,
                            out: field.name.clone(),
                        });
                    }
                    // Enum variants are constants — no edge.
                }
            }
            ExprKind::Unary { expr, .. } => self.expr_reads(dcx, expr, out),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr_reads(dcx, lhs, out);
                self.expr_reads(dcx, rhs, out);
            }
            ExprKind::IfExpr { cond, then, els } => {
                self.expr_reads(dcx, cond, out);
                self.expr_reads(dcx, then, out);
                self.expr_reads(dcx, els, out);
            }
            ExprKind::Match { scrutinee, arms } => {
                self.expr_reads(dcx, scrutinee, out);
                for arm in arms {
                    self.expr_reads(dcx, &arm.value, out);
                }
            }
            ExprKind::Concat(parts) => {
                for p in parts {
                    self.expr_reads(dcx, p, out);
                }
            }
            ExprKind::Replicate { count, parts } => {
                self.expr_reads(dcx, count, out);
                for p in parts {
                    self.expr_reads(dcx, p, out);
                }
            }
            ExprKind::Index { base, index } => {
                self.expr_reads(dcx, base, out);
                self.expr_reads(dcx, index, out);
            }
            ExprKind::Slice { base, hi, lo } => {
                self.expr_reads(dcx, base, out);
                self.expr_reads(dcx, hi, out);
                self.expr_reads(dcx, lo, out);
            }
            ExprKind::Call { args, .. } => {
                for a in args {
                    self.expr_reads(dcx, a, out);
                }
            }
            ExprKind::FnCall { args, .. } => {
                for a in args {
                    self.expr_reads(dcx, a, out);
                }
            }
            ExprKind::BundleLit(_) => todo!(),
        }
    }

    /// The bits a drive's lvalue claims, as precisely as the env allows.
    fn lvalue_extent(&mut self, dcx: &Dcx<'a>, lv: &'a LValue) -> Extent {
        let Some((first, second)) = &lv.index else {
            return Extent::Whole;
        };
        let fallback = if dcx.bound {
            Extent::Dynamic
        } else {
            Extent::Unknown
        };
        let f = consteval::eval(first, &dcx.env);
        match second {
            None => match f {
                Ok(v) if v >= 0 => Extent::Range(v as u128, v as u128),
                _ => fallback,
            },
            Some(lo) => match (f, consteval::eval(lo, &dcx.env)) {
                (Ok(h), Ok(l)) if l >= 0 && h >= l => Extent::Range(l as u128, h as u128),
                _ => fallback, // reversed/negative bounds: widths owns E0406
            },
        }
    }

    /// E0501 — overlapping drive sites on one signal (first conflict
    /// reported, then poisoned: a repeat would repeat the same message).
    fn report_conflicts(&mut self, dcx: &mut Dcx<'a>) {
        let mut errs: Vec<(String, Span)> = Vec::new();
        for (name, sites) in &dcx.sites {
            if dcx.poisoned.contains(name) {
                continue;
            }
            'outer: for (j, b) in sites.iter().enumerate() {
                for a in &sites[..j] {
                    if conflicts(&a.extent, &b.extent) {
                        errs.push((name.clone(), b.span));
                        break 'outer;
                    }
                }
            }
        }
        errs.sort_by_key(|(_, s)| s.start);
        for (name, span) in errs {
            dcx.poisoned.insert(name.clone());
            self.err(
                dcx.file,
                span,
                "E0501",
                format!("`{name}` has more than one driver"),
                "every output and wire is driven exactly once (spec/02 \
                 section 6) — merge the logic into one `=` (an `if`/`match` \
                 expression chooses between values), or drive disjoint bits \
                 (`x[hi:lo]`)",
            );
        }
    }

    /// E0502 — an out port with no driver at all, or (when the width is
    /// known and every site is a constant range) with undriven bits.
    fn report_coverage(&mut self, dcx: &mut Dcx<'a>) {
        for (name, ty) in &dcx.outs {
            if dcx.poisoned.contains(&name.name) {
                continue;
            }
            let sites = dcx.sites.get(&name.name).map(Vec::as_slice).unwrap_or(&[]);
            if sites.is_empty() {
                self.err(
                    dcx.file,
                    name.span,
                    "E0502",
                    format!("output `{}` is never driven", name.name),
                    "an undriven output floats — drive it with \
                     `name = expr` at module level (spec/02 section 6)",
                );
                continue;
            }
            // Full coverage check: only meaningful when every site is a
            // constant range and the width evaluates.
            if sites.iter().any(|s| !matches!(s.extent, Extent::Range(..))) {
                continue; // a Whole/Dynamic/Unknown site may cover everything
            }
            let width = match ty {
                Type::Bit => Some(1),
                Type::Bits(w) | Type::Signed(w) => consteval::eval(w, &dcx.env)
                    .ok()
                    .and_then(|v| u128::try_from(v).ok()),
                Type::Named(_) => None,
                Type::Bundle { .. } => todo!(),
            };
            let Some(width) = width else { continue };
            // A zero-width output is already an E0410 elsewhere; coverage
            // analysis is meaningless on it and would underflow the bound
            // below (`covered.len() - 1` on an empty vec).
            if width == 0 {
                continue;
            }
            let mut covered = vec![false; width.min(4096) as usize];
            for s in sites {
                if let Extent::Range(lo, hi) = s.extent {
                    for b in lo..=hi.min(covered.len() as u128 - 1) {
                        covered[b as usize] = true;
                    }
                }
            }
            if let Some(first_gap) = covered.iter().position(|c| !c) {
                self.err(
                    dcx.file,
                    name.span,
                    "E0502",
                    format!(
                        "output `{}` is only partially driven — bit {first_gap} \
                         has no driver",
                        name.name
                    ),
                    "every bit of an output needs exactly one driver — cover \
                     the missing bits, or drive the whole signal at once",
                );
            }
        }
    }

    /// E0503 — a reg owned by zero or by more than one `on` block.
    fn report_reg_blocks(&mut self, dcx: &Dcx<'a>) {
        for name in &dcx.regs {
            if dcx.poisoned.contains(&name.name) {
                continue;
            }
            let blocks = dcx
                .reg_blocks
                .get(&name.name)
                .map(HashSet::len)
                .unwrap_or(0);
            if blocks == 0 {
                self.err(
                    dcx.file,
                    name.span,
                    "E0503",
                    format!("reg `{}` is never assigned", name.name),
                    "it would hold its reset value forever — update it with \
                     `<-` inside an `on` block, or use a `wire` for a \
                     computed value (spec/02 section 6)",
                );
            } else if blocks > 1 {
                self.err(
                    dcx.file,
                    name.span,
                    "E0503",
                    format!(
                        "reg `{}` is assigned in more than one `on` block",
                        name.name
                    ),
                    "every reg is owned by exactly one `on` block (spec/02 \
                     section 6) — merge the assignments into one block",
                );
            }
        }
    }

    /// E0503 — a memory written from more than one `on` block. Unlike a reg,
    /// a memory with zero writers is valid (a power-on-init ROM).
    fn report_mem_blocks(&mut self, dcx: &Dcx<'a>) {
        for name in &dcx.mems {
            if dcx.poisoned.contains(&name.name) {
                continue;
            }
            let blocks = dcx
                .reg_blocks
                .get(&name.name)
                .map(HashSet::len)
                .unwrap_or(0);
            if blocks > 1 {
                self.err(
                    dcx.file,
                    name.span,
                    "E0503",
                    format!(
                        "memory `{}` is written in more than one `on` block",
                        name.name
                    ),
                    "a memory is owned by at most one `on` block (spec/02 \
                     section 6) — merge the writes into one block",
                );
            }
        }
    }

    /// `out -> ins` summary of a module, by reachability over its own
    /// combinational graph. Memoized; recursion yields an empty summary.
    fn comb_summary(
        &mut self,
        module: &str,
        summaries: &mut HashMap<String, Summary>,
        in_progress: &mut HashSet<String>,
    ) -> Summary {
        if let Some(s) = summaries.get(module) {
            return s.clone();
        }
        if in_progress.contains(module) {
            return Rc::new(HashMap::new()); // recursive instantiation
        }
        let Some(&(file, m)) = self.modules.get(module) else {
            return Rc::new(HashMap::new()); // E0102 already reported
        };
        let Some(sc) = self.scopes.get(module).cloned() else {
            return Rc::new(HashMap::new());
        };
        in_progress.insert(module.to_string());
        let (env, bound) = self.driver_env(file, m);
        let mut dcx = Dcx {
            file,
            sc,
            env,
            bound,
            report: false, // edges only — the module's own check reports
            sites: HashMap::new(),
            reg_blocks: HashMap::new(),
            poisoned: HashSet::new(),
            edges: HashMap::new(),
            node_spans: HashMap::new(),
            outs: Vec::new(),
            regs: Vec::new(),
            mems: Vec::new(),
            repeat_budget: REPEAT_BUDGET,
        };
        self.collect_items(&mut dcx, &m.items, summaries, in_progress);
        in_progress.remove(module);

        let ins: HashSet<&str> = dcx
            .sc
            .names
            .iter()
            .filter(|(_, b)| matches!(b, Bind::In))
            .map(|(n, _)| n.as_str())
            .collect();
        let mut summary = HashMap::new();
        for item in &m.items {
            let ModuleItem::Port {
                dir: Dir::Out,
                name,
                ..
            } = item
            else {
                continue;
            };
            let mut seen: HashSet<Node> = HashSet::new();
            let mut stack = vec![Node::Sig(name.name.clone())];
            let mut deps: HashSet<String> = HashSet::new();
            while let Some(n) = stack.pop() {
                if !seen.insert(n.clone()) {
                    continue;
                }
                if let Node::Sig(s) = &n
                    && ins.contains(s.as_str())
                {
                    deps.insert(s.clone());
                    continue;
                }
                if let Some(next) = dcx.edges.get(&n) {
                    stack.extend(next.iter().cloned());
                }
            }
            if !deps.is_empty() {
                summary.insert(name.name.clone(), deps);
            }
        }
        let summary: Summary = Rc::new(summary);
        summaries.insert(module.to_string(), summary.clone());
        summary
    }

    /// E0504 — three-color DFS over the module's combinational graph;
    /// a back-edge to a gray node is a cycle, reported with its path.
    fn find_cycles(&mut self, dcx: &mut Dcx<'a>) {
        const WHITE: u8 = 0;
        const GRAY: u8 = 1;
        const BLACK: u8 = 2;
        let mut color: HashMap<&Node, u8> = HashMap::new();
        let mut roots: Vec<&Node> = dcx.edges.keys().collect();
        roots.sort_by_key(|n| dcx.node_spans.get(n).map(|s| s.start).unwrap_or(0));
        let mut cycles: Vec<(String, Span)> = Vec::new();

        // Iterative DFS with an explicit path so the error can SHOW the loop.
        for root in roots {
            if color.get(root).copied().unwrap_or(WHITE) != WHITE {
                continue;
            }
            let mut path: Vec<&Node> = Vec::new();
            let mut stack: Vec<(&Node, bool)> = vec![(root, false)];
            while let Some((n, leaving)) = stack.pop() {
                if leaving {
                    color.insert(n, BLACK);
                    path.pop();
                    continue;
                }
                match color.get(n).copied().unwrap_or(WHITE) {
                    GRAY => {
                        // Back edge: the cycle is the path from n onward.
                        let start = path.iter().position(|p| *p == n).unwrap_or(0);
                        let mut names: Vec<String> =
                            path[start..].iter().map(|p| p.show()).collect();
                        names.push(n.show());
                        let span = dcx
                            .node_spans
                            .get(n)
                            .copied()
                            .unwrap_or_else(|| Span::new(0, 0));
                        cycles.push((names.join(" -> "), span));
                        continue;
                    }
                    BLACK => continue,
                    _ => {}
                }
                color.insert(n, GRAY);
                path.push(n);
                stack.push((n, true));
                if let Some(next) = dcx.edges.get(n) {
                    let mut next: Vec<&Node> = next.iter().collect();
                    next.sort_by_key(|m| dcx.node_spans.get(*m).map(|s| s.start).unwrap_or(0));
                    for m in next {
                        stack.push((m, false));
                    }
                }
            }
        }
        for (cycle, span) in cycles {
            self.err(
                dcx.file,
                span,
                "E0504",
                format!("combinational cycle: {cycle}"),
                "a value cannot depend on itself within the same instant — \
                 every feedback loop must pass through a register; break the \
                 cycle with a `reg` updated in an `on` block (spec/02 \
                 section 6)",
            );
        }
    }
}
