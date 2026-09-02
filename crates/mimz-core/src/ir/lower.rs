//! `elaborate::Design` -> `ir::Module` lowering.

use super::{Bits, Cell, CellKind, Module};
use crate::ast::{BinOp, Dir, Expr, ExprKind, FnStmt, LValue, SeqStmt, Type, UnOp};
use crate::elaborate::Design;
use std::collections::{BTreeMap, HashMap};

/// An unsigned constant of an exact width — the shape every synthesized
/// literal in this file wants.
fn const_val(value: u128, width: u32) -> crate::checker::consteval::ConstVal {
    crate::checker::consteval::ConstVal {
        bits: crate::bits::Bits::Small(value),
        width,
        signed: false,
    }
}

/// The three synthetic `env` keys a memory's write port occupies while
/// `lower_seq_stmts` folds its `if`s. They live in the same key space as
/// register names, and the `__mem_` prefix keeps them out of the way of
/// real signal names.
fn mem_write_keys(mem: &str) -> (String, String, String) {
    (
        format!("__mem_wen_{mem}"),
        format!("__mem_waddr_{mem}"),
        format!("__mem_wdata_{mem}"),
    )
}

/// Lowering state threaded through one module's lowering: which
/// signal names have already been turned into `Bits`, memoized so a wire
/// referenced by more than one comb expression is lowered exactly once
/// (the AST checker already guarantees `comb` forms a DAG, so plain
/// memoized recursion terminates — no separate topo-sort needed).
struct LowerCtx<'a> {
    design: &'a Design,
    resolved: HashMap<String, Bits>,
    /// Per-memory `(raddr, rdata)` nets, allocated lazily on the first
    /// `m[addr]` read and picked up later by `lower`'s cell-emitting pass,
    /// which is the only place the `Mem` cell itself is pushed (it needs the
    /// read and write pin sets filled in together, and the write side isn't
    /// knowable until the writing process is walked).
    ///
    /// SINGLE READ PORT (acknowledged ceiling): one entry per memory. A
    /// second read at the SAME lowered address reuses this port for free; a
    /// second read at a different address panics rather than silently
    /// reading the wrong word. Multiple read ports need a
    /// `raddr0`/`rdata0`/... pin set the design doc doesn't define yet.
    mem_read: HashMap<String, (Bits, Bits)>,
    /// One TOP-LEVEL expression node's lowered result, keyed by the ADDRESS
    /// of its `Expr` node.
    ///
    /// `lower_seq_stmts` walks one shared process body once per target (once
    /// per register, once per memory write port), and `SeqStmt::If` lowers
    /// its condition on every one of those walks. Without memoization every
    /// walk re-lowers that condition from scratch, allocating fresh nets for
    /// anything past a bare `Ident` — which duplicates cells, and makes one
    /// source-level `m[0]` read look like two rival read ports to
    /// `mem_read`'s address comparison. `design` is borrowed immutably for
    /// the whole pass, and every node keyed here belongs to it (never to a
    /// temporary), so a node's address is a stable identity for "this exact
    /// expression site": re-encountering one returns its `Bits` with no
    /// re-lowering at all — nothing inside it, an inlined `fn` body
    /// included, gets a second chance to allocate.
    ///
    /// Only populated/consulted when `locals.is_none()`, i.e. never inside an
    /// inlined `fn` body, where one node legitimately means different values
    /// on different calls (`fn f(i) { ram[i] }` called as `f(a)` and `f(b)`).
    ///
    /// FUTURE RE-WALK MECHANISMS: the same caveat binds anything else that
    /// re-walks one body at `locals: None` with DIFFERING per-iteration
    /// semantics. Specifically, `on`-block `Loop`/`ForEach` unrolling
    /// (`unimplemented!` in `lower_seq_stmts` today) must either bypass this
    /// memo per iteration or scope it per iteration — otherwise every
    /// iteration silently reuses iteration 0's nets.
    expr_memo: HashMap<usize, Bits>,
}

impl<'a> LowerCtx<'a> {
    /// Resolves a signal name to its `Bits`, lowering its driving comb
    /// expression on first use if it's a wire/output (a plain input or a
    /// register's Q is already in `resolved` from module-scaffold time).
    fn resolve(&mut self, module: &mut Module, name: &str) -> Bits {
        if let Some(bits) = self.resolved.get(name) {
            return bits.clone();
        }
        let expr = self.design.comb.get(name).unwrap_or_else(|| {
            panic!("no driver recorded for signal `{name}` (checker should have caught this)")
        });
        // Top-level signal resolution is never inside a `fn` body, so there
        // are no call-local bindings in scope here.
        let bits = self.lower_expr(module, expr, None);
        self.resolved.insert(name.to_string(), bits.clone());
        bits
    }

    /// Lowers one expression to its `Bits`. `locals` carries the current
    /// `fn` call's param/`let` bindings (params zipped with lowered args,
    /// see the `FnCall` arm below) when lowering is happening INSIDE an
    /// inlined function body; `None` everywhere else (module-level
    /// wire/output/register lowering) — a plain `Ident` then always
    /// resolves as a module signal via `self.resolve`.
    ///
    /// At top level (`locals: None`) the whole thing is memoized on the
    /// node's address, so a body walked once per target lowers each of its
    /// expressions exactly once. See `LowerCtx::expr_memo` for why the
    /// `fn`-body path is deliberately excluded.
    fn lower_expr(
        &mut self,
        module: &mut Module,
        e: &Expr,
        locals: Option<&HashMap<String, Bits>>,
    ) -> Bits {
        let site = std::ptr::from_ref(e) as usize;
        if locals.is_none()
            && let Some(bits) = self.expr_memo.get(&site)
        {
            return bits.clone();
        }
        let result = match &e.kind {
            ExprKind::Ident(name) => {
                let local = locals.and_then(|l| l.get(name)).cloned();
                match local {
                    Some(bits) => bits,
                    None => self.resolve(module, name),
                }
            }
            ExprKind::Bool(b) => {
                let const_val = crate::checker::consteval::ConstVal {
                    bits: crate::bits::Bits::Small(*b as u128),
                    width: 1,
                    signed: false,
                };
                self.lower_const(module, &const_val, e.span)
            }
            ExprKind::Int { value, .. } => {
                let width = crate::bits::natural_width(value).max(1);
                let const_val = crate::checker::consteval::ConstVal {
                    bits: value.clone(),
                    width,
                    signed: false,
                };
                self.lower_const(module, &const_val, e.span)
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let a = self.lower_expr(module, lhs, locals);
                let b = self.lower_expr(module, rhs, locals);
                self.lower_binop(module, *op, a, b, e.span)
            }
            ExprKind::Unary { op, expr } => {
                let a = self.lower_expr(module, expr, locals);
                let (kind, out_width) = match op {
                    UnOp::Neg => (CellKind::Neg, a.width()),
                    UnOp::BitNot => (CellKind::Not, a.width()),
                    UnOp::LogicNot => (CellKind::LogicNot, 1),
                    UnOp::RedAnd => (CellKind::RedAnd, 1),
                    UnOp::RedOr => (CellKind::RedOr, 1),
                    UnOp::RedXor => (CellKind::RedXor, 1),
                };
                let out = module.alloc_bits(out_width, None);
                module.cells.push(Cell {
                    kind,
                    pins: [("a", a), ("out", out.clone())].into_iter().collect(),
                    span: e.span,
                });
                out
            }
            // `{a, b}` is Verilog-style: the first (source-order) part is
            // the MOST-significant. `ir::Bits` is index-0-is-LSB, so the
            // last part in source order goes at the low indices. No cell:
            // this is pure bit-vector reassembly, not a new value.
            ExprKind::Concat(parts) => {
                let mut ids = Vec::new();
                for part in parts.iter().rev() {
                    let bits = self.lower_expr(module, part, locals);
                    ids.extend(bits.0);
                }
                Bits(ids)
            }
            // `base[hi:lo]`, both bounds inclusive. `hi`/`lo` always
            // const-fold (checker-enforced) — the same const_eval promoted
            // from mimz-sim in Task 1. No cell: a sub-range of existing
            // nets, not a new value.
            ExprKind::Slice { base, hi, lo } => {
                let base_bits = self.lower_expr(module, base, locals);
                let hi_val = crate::value::const_eval(hi, &self.design.consts)
                    .expect("checker guarantees slice bounds const-fold")
                    as usize;
                let lo_val = crate::value::const_eval(lo, &self.design.consts)
                    .expect("checker guarantees slice bounds const-fold")
                    as usize;
                Bits(base_bits.0[lo_val..=hi_val].to_vec())
            }
            ExprKind::IfExpr { cond, then, els } => {
                let sel = self.lower_expr(module, cond, locals);
                let a = self.lower_expr(module, then, locals);
                let b = self.lower_expr(module, els, locals);
                let out_width = a.width().max(b.width());
                let out = module.alloc_bits(out_width, None);
                module.cells.push(Cell {
                    kind: CellKind::Mux,
                    pins: [("sel", sel), ("a", a), ("b", b), ("out", out.clone())]
                        .into_iter()
                        .collect(),
                    span: e.span,
                });
                out
            }
            // `base[i]` is dual-use: a full-width memory-word read when
            // `base` names a `design.mems` entry, a single-bit select
            // otherwise. Only the name tells them apart — the parser can't,
            // and the width checker branches on exactly this same test.
            ExprKind::Index { base, index } if self.indexed_mem(base).is_some() => {
                let (mem, width) = self.indexed_mem(base).unwrap();
                // A repeat encounter of THIS read node never gets here —
                // `expr_memo` intercepts it above. So reaching the
                // comparison below means a genuinely distinct read site (or
                // a re-inlined `fn` body, where the address really can
                // differ per call).
                let addr = self.lower_expr(module, index, locals);
                match self.mem_read.get(&mem).cloned() {
                    // A DIFFERENT read site landing on the same memory. Same
                    // address: share the one read port. Different address:
                    // that's the single-read-port ceiling (see `mem_read`) —
                    // panic like every other out-of-scope form in this file
                    // rather than silently reading the wrong word.
                    Some((prev, rdata)) => {
                        if prev != addr {
                            unimplemented!(
                                "memory `{mem}` is read at two different addresses, but \
                                 ir::lower models exactly one read port per memory (see \
                                 LowerCtx::mem_read); span: {:?}. Note this compares LOWERED \
                                 nets, so two DISTINCT read sites whose indices are textually \
                                 identical also trip it — a loud false positive beats silently \
                                 wrong hardware.",
                                e.span
                            );
                        }
                        rdata
                    }
                    None => {
                        let rdata = module.alloc_bits(width, Some(&mem));
                        self.mem_read.insert(mem, (addr, rdata.clone()));
                        rdata
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.lower_match(module, scrutinee, arms, e.span, locals)
            }
            ExprKind::FnCall { name, args } => {
                let func = self.design.funcs.get(&name.name).unwrap_or_else(|| {
                    panic!(
                        "unknown function `{}` (checker should have caught this)",
                        name.name
                    )
                });
                for param in &func.params {
                    if matches!(param.ty, Type::Array { .. }) {
                        unimplemented!(
                            "array-typed fn params not yet lowered by Task 9 (out of scope — a \
                             distinct, separately-tracked flattening concern, see emit_verilog's \
                             array-param handling); fn `{}`, param `{}`",
                            name.name,
                            param.name.name
                        );
                    }
                }
                let arg_bits: Vec<Bits> = args
                    .iter()
                    .map(|a| self.lower_expr(module, a, locals))
                    .collect();
                let mut call_locals: HashMap<String, Bits> = HashMap::new();
                for (param, bits) in func.params.iter().zip(arg_bits) {
                    call_locals.insert(param.name.name.clone(), bits);
                }
                self.lower_fn_stmts(module, &func.stmts, &func.tail, &call_locals)
            }
            other => unimplemented!(
                "expression form not yet lowered by Task 5/6 (see later tasks for \
                 field access; a bit-select `v[i]` on a plain vector — as opposed \
                 to the memory read Task 10 handles above — also still needs \
                 bit-level indexing machinery): {other:?}"
            ),
        };
        if locals.is_none() {
            self.expr_memo.insert(site, result.clone());
        }
        result
    }

    /// Classifies an assignment target. A bit-select write to a plain
    /// signal needs bit-level assignment logic no task implements yet —
    /// panic loudly rather than silently mis-widening the target.
    fn assign_target(&self, lhs: &LValue) -> Target {
        if lhs.index.is_none() {
            return Target::Signal(lhs.base.name.clone());
        }
        if self.design.mems.iter().any(|m| m.name == lhs.base.name) {
            return Target::MemWrite(lhs.base.name.clone());
        }
        unimplemented!(
            "bit-select LValue writes (`q[3] <- ...`) not yet lowered by Task 8; span: {:?}",
            lhs.span
        );
    }

    /// `(name, word width)` of the memory `base` indexes, if `base` is a
    /// bare identifier naming a `design.mems` entry.
    fn indexed_mem(&self, base: &Expr) -> Option<(String, u32)> {
        let ExprKind::Ident(name) = &base.kind else {
            return None;
        };
        self.design
            .mems
            .iter()
            .find(|m| m.name == *name)
            .map(|m| (m.name.clone(), m.width.bits))
    }

    /// Walks one `fn` body's statement list, evaluating against `locals`
    /// (params + `let`s bound so far). Mirrors
    /// `emit_verilog::module::funcs::emit_fn_stmts`'s continuation-passing
    /// shape: an unconditional `Return` short-circuits before any later
    /// statement or `tail` is ever reached, exactly like that renderer's
    /// `rest` threading — checker-guaranteed (E0812) so no reachability
    /// analysis is needed here either.
    fn lower_fn_stmts(
        &mut self,
        module: &mut Module,
        stmts: &[FnStmt],
        tail: &Expr,
        locals: &HashMap<String, Bits>,
    ) -> Bits {
        match stmts.split_first() {
            None => self.lower_expr(module, tail, Some(locals)),
            Some((FnStmt::Let(l), rest)) => {
                let v = self.lower_expr(module, &l.value, Some(locals));
                let mut locals2 = locals.clone();
                locals2.insert(l.name.name.clone(), v);
                self.lower_fn_stmts(module, rest, tail, &locals2)
            }
            Some((FnStmt::Return(e), _rest)) => self.lower_expr(module, e, Some(locals)),
            Some((FnStmt::If { cond, then, els }, rest)) => {
                let sel = self.lower_expr(module, cond, Some(locals));
                let then_full: Vec<FnStmt> = then.iter().chain(rest.iter()).cloned().collect();
                let then_val = self.lower_fn_stmts(module, &then_full, tail, locals);
                let els_slice: &[FnStmt] = els.as_deref().unwrap_or(&[]);
                let els_full: Vec<FnStmt> = els_slice.iter().chain(rest.iter()).cloned().collect();
                let else_val = self.lower_fn_stmts(module, &els_full, tail, locals);
                let out_width = then_val.width().max(else_val.width());
                let out = module.alloc_bits(out_width, None);
                module.cells.push(Cell {
                    kind: CellKind::Mux,
                    pins: [
                        ("sel", sel),
                        ("a", then_val),
                        ("b", else_val),
                        ("out", out.clone()),
                    ]
                    .into_iter()
                    .collect(),
                    span: cond.span,
                });
                out
            }
            Some((FnStmt::Loop { span, .. }, _)) | Some((FnStmt::ForEach { span, .. }, _)) => {
                unimplemented!(
                    "loop/foreach unrolling inside fn bodies not yet lowered by Task 9 \
                     (needs const-var-substitution machinery, same gap as Task 8's on-block \
                     Loop/ForEach); span: {span:?}"
                )
            }
            Some((FnStmt::Error(_), rest)) => self.lower_fn_stmts(module, rest, tail, locals),
        }
    }

    /// Emits a `CellKind::Const` cell and returns its `out` pin — the one
    /// path every literal (Task 5) and synthesized reset value (Task 8)
    /// goes through, so there's exactly one place a constant becomes a
    /// net.
    fn lower_const(
        &mut self,
        module: &mut Module,
        value: &crate::checker::consteval::ConstVal,
        span: crate::span::Span,
    ) -> Bits {
        let out = module.alloc_bits(value.width, None);
        module.cells.push(Cell {
            kind: CellKind::Const {
                value: value.clone(),
            },
            pins: [("out", out.clone())].into_iter().collect(),
            span,
        });
        out
    }

    fn lower_binop(
        &mut self,
        module: &mut Module,
        op: BinOp,
        a: Bits,
        b: Bits,
        span: crate::span::Span,
    ) -> Bits {
        let in_width = a.width().max(b.width());
        let (kind, out_width) = match op {
            BinOp::Add => (CellKind::Add, in_width + 1),
            BinOp::Sub => (CellKind::Sub, in_width + 1),
            BinOp::Mul => (CellKind::Mul, a.width() + b.width()),
            BinOp::AddWrap => (CellKind::AddWrap, in_width),
            BinOp::SubWrap => (CellKind::SubWrap, in_width),
            BinOp::MulWrap => (CellKind::MulWrap, in_width),
            BinOp::Shl => (CellKind::Shl, a.width()),
            BinOp::Shr => (CellKind::Shr, a.width()),
            BinOp::BitAnd => (CellKind::And, in_width),
            BinOp::BitOr => (CellKind::Or, in_width),
            BinOp::BitXor => (CellKind::Xor, in_width),
            BinOp::Eq => (CellKind::Eq, 1),
            BinOp::Ne => (CellKind::Ne, 1),
            BinOp::Lt => (CellKind::Lt, 1),
            BinOp::Le => (CellKind::Le, 1),
            other => unimplemented!(
                "binop not yet lowered by Task 5 (Gt/Ge/logical and/or land \
                 alongside their AST variants once checked against ast::BinOp's \
                 full list): {other:?}"
            ),
        };
        let out = module.alloc_bits(out_width, None);
        module.cells.push(Cell {
            kind,
            pins: [("a", a), ("b", b), ("out", out.clone())]
                .into_iter()
                .collect(),
            span,
        });
        out
    }

    /// Lowers `match scrutinee { arms }` as a reverse fold of nested `Mux`
    /// cells: the last arm (or any arm containing `Pattern::Wildcard`) is
    /// the unconditional default, folded in first, then earlier arms are
    /// wrapped around it in reverse declaration order so the FIRST matching
    /// pattern wins — matching `emit_verilog::expr::match_subst`'s
    /// `is_last || is_wild` priority exactly, so IR execution and Verilog
    /// output never disagree on tie-breaking.
    fn lower_match(
        &mut self,
        module: &mut Module,
        scrutinee: &Expr,
        arms: &[crate::ast::Arm],
        span: crate::span::Span,
        locals: Option<&HashMap<String, Bits>>,
    ) -> Bits {
        let scrutinee_bits = self.lower_expr(module, scrutinee, locals);
        let n = arms.len();
        let mut acc = self.lower_expr(module, &arms[n - 1].value, locals);
        for arm in arms[..n - 1].iter().rev() {
            let sel = self.lower_pattern_conds(module, &scrutinee_bits, &arm.patterns, span);
            let arm_value = self.lower_expr(module, &arm.value, locals);
            let out_width = arm_value.width().max(acc.width());
            let out = module.alloc_bits(out_width, None);
            module.cells.push(Cell {
                kind: CellKind::Mux,
                pins: [
                    ("sel", sel),
                    ("a", arm_value),
                    ("b", acc),
                    ("out", out.clone()),
                ]
                .into_iter()
                .collect(),
                span,
            });
            acc = out;
        }
        acc
    }

    /// One arm's patterns OR'd together into a single 1-bit selector.
    fn lower_pattern_conds(
        &mut self,
        module: &mut Module,
        scrutinee: &Bits,
        patterns: &[crate::ast::Pattern],
        span: crate::span::Span,
    ) -> Bits {
        let mut acc: Option<Bits> = None;
        for p in patterns {
            let cond = self.lower_pattern_eq(module, scrutinee, p, span);
            acc = Some(match acc {
                None => cond,
                Some(prev) => self.push_binary_cell(module, CellKind::LogicOr, prev, cond, 1, span),
            });
        }
        acc.expect("checker guarantees every match arm has at least one pattern")
    }

    /// Lowers a single pattern to the 1-bit "does `scrutinee` match this
    /// pattern" condition.
    fn lower_pattern_eq(
        &mut self,
        module: &mut Module,
        scrutinee: &Bits,
        p: &crate::ast::Pattern,
        span: crate::span::Span,
    ) -> Bits {
        use crate::ast::Pattern;
        match p {
            Pattern::Wildcard => {
                let cv = crate::checker::consteval::ConstVal {
                    bits: crate::bits::Bits::Small(1),
                    width: 1,
                    signed: false,
                };
                self.lower_const(module, &cv, span)
            }
            Pattern::Bool(b) => {
                let cv = crate::checker::consteval::ConstVal {
                    bits: crate::bits::Bits::Small(*b as u128),
                    width: scrutinee.width(),
                    signed: false,
                };
                let const_bits = self.lower_const(module, &cv, span);
                self.push_binary_cell(module, CellKind::Eq, scrutinee.clone(), const_bits, 1, span)
            }
            Pattern::Int { value, .. } => {
                let cv = crate::checker::consteval::ConstVal {
                    bits: value.clone(),
                    width: scrutinee.width(),
                    signed: false,
                };
                let const_bits = self.lower_const(module, &cv, span);
                self.push_binary_cell(module, CellKind::Eq, scrutinee.clone(), const_bits, 1, span)
            }
            Pattern::IntMask {
                value, mask, width, ..
            } => {
                // `(scrutinee & mask) == value`, both sized to the
                // pattern's own width.
                let mask_cv = crate::checker::consteval::ConstVal {
                    bits: crate::bits::Bits::Small(*mask),
                    width: *width,
                    signed: false,
                };
                let mask_bits = self.lower_const(module, &mask_cv, span);
                let masked = self.push_binary_cell(
                    module,
                    CellKind::And,
                    scrutinee.clone(),
                    mask_bits,
                    *width,
                    span,
                );
                let value_cv = crate::checker::consteval::ConstVal {
                    bits: crate::bits::Bits::Small(*value),
                    width: *width,
                    signed: false,
                };
                let value_bits = self.lower_const(module, &value_cv, span);
                self.push_binary_cell(module, CellKind::Eq, masked, value_bits, 1, span)
            }
            Pattern::Variant { .. } => unreachable!(
                "Pattern::Variant never reaches ir::lower — elaborate::rewrite.rs already rewrites every \
                 Variant pattern to Int (tag-only) or IntMask (tagged+payload) before Design exists; its own \
                 comment states \"the runtime evaluator never sees Pattern::Variant\""
            ),
        }
    }

    /// Shared 2-input/1-output cell constructor — used by the pattern-matching
    /// helpers above (Eq/And/LogicOr all have this exact shape).
    fn push_binary_cell(
        &mut self,
        module: &mut Module,
        kind: CellKind,
        a: Bits,
        b: Bits,
        out_width: u32,
        span: crate::span::Span,
    ) -> Bits {
        let out = module.alloc_bits(out_width, None);
        module.cells.push(Cell {
            kind,
            pins: [("a", a), ("b", b), ("out", out.clone())]
                .into_iter()
                .collect(),
            span,
        });
        out
    }

    /// Walks one `on`-block body, folding last-write-wins per-target
    /// assignment through `if`/`else`, mirroring
    /// `emit_verilog::module::seq::seq_stmts`'s two-pass structure
    /// (D-DEFAULT-3: every `Default` is seeded into `env` before any
    /// `Assign`/`If` is processed, so a conditional assign always wins over
    /// a default for the same target).
    fn lower_seq_stmts(
        &mut self,
        module: &mut Module,
        stmts: &[SeqStmt],
        env: &mut HashMap<String, Bits>,
    ) {
        for stmt in stmts {
            if let SeqStmt::Default { name, val, .. } = stmt {
                // Only targets the caller pre-seeded are tracked: one walk
                // of the body is done per register AND per memory write
                // port, and each walk must leave `env`'s key set untouched
                // or the branch merge below indexes a key that was never
                // seeded. Assignments to everything else are simply not
                // this walk's business.
                if env.contains_key(&name.name) {
                    // `on`-block register lowering is always top-level, never
                    // inside a `fn` body — no call-local bindings in scope.
                    let bits = self.lower_expr(module, val, None);
                    env.insert(name.name.clone(), bits);
                }
            }
        }
        for stmt in stmts {
            match stmt {
                SeqStmt::Assign { lhs, rhs } => match self.assign_target(lhs) {
                    Target::Signal(name) => {
                        if env.contains_key(&name) {
                            let bits = self.lower_expr(module, rhs, None);
                            env.insert(name, bits);
                        }
                    }
                    // `m[addr] <- v` drives three write-port values at once;
                    // they then fold through the `if` merge below exactly
                    // like any register's, so a conditional write comes out
                    // as `wen = Mux(cond, 1, 0)` for free.
                    Target::MemWrite(mem) => {
                        let (wen_k, waddr_k, wdata_k) = mem_write_keys(&mem);
                        if env.contains_key(&wen_k) {
                            // `.1` is the ranged form's second bound
                            // (`x[hi:lo] <- ..`); a memory write is always a
                            // whole-word `m[addr] <- v`, so the checker never
                            // lets a range reach a `design.mems` base.
                            let addr_expr = &lhs.index.as_ref().expect("MemWrite implies index").0;
                            let addr = self.lower_expr(module, addr_expr, None);
                            let data = self.lower_expr(module, rhs, None);
                            let one = self.lower_const(module, &const_val(1, 1), lhs.span);
                            env.insert(wen_k, one);
                            env.insert(waddr_k, addr);
                            env.insert(wdata_k, data);
                        }
                    }
                },
                SeqStmt::If { cond, then, els } => {
                    let sel = self.lower_expr(module, cond, None);
                    let mut then_env = env.clone();
                    self.lower_seq_stmts(module, then, &mut then_env);
                    let mut else_env = env.clone();
                    if let Some(else_stmts) = els {
                        self.lower_seq_stmts(module, else_stmts, &mut else_env);
                    }
                    let mut changed: Vec<String> =
                        then_env.keys().chain(else_env.keys()).cloned().collect();
                    changed.sort();
                    changed.dedup();
                    for name in changed {
                        let before = env[&name].clone();
                        let a = then_env
                            .get(&name)
                            .cloned()
                            .unwrap_or_else(|| before.clone());
                        let b = else_env.get(&name).cloned().unwrap_or(before);
                        if a == b {
                            env.insert(name, a);
                            continue;
                        }
                        let out_width = a.width().max(b.width());
                        let out = module.alloc_bits(out_width, None);
                        module.cells.push(Cell {
                            kind: CellKind::Mux,
                            pins: [
                                ("sel", sel.clone()),
                                ("a", a),
                                ("b", b),
                                ("out", out.clone()),
                            ]
                            .into_iter()
                            .collect(),
                            span: crate::span::Span::default(),
                        });
                        env.insert(name, out);
                    }
                }
                SeqStmt::Default { .. } => {} // already seeded above
                SeqStmt::Loop { span, .. } | SeqStmt::ForEach { span, .. } => {
                    unimplemented!(
                        "loop/foreach unrolling inside on-blocks not yet lowered by \
                         Task 8 (needs const-var-substitution machinery); span: {span:?}"
                    )
                }
                // assert/cover never synthesized (design doc decision);
                // Error is parser-recovery-only, unreachable on the
                // elaborated-Design path.
                SeqStmt::Assert(_) | SeqStmt::Cover(_) | SeqStmt::Error(_) => {}
            }
        }
    }
}

/// What an `on`-block assignment writes to. `LValue` is dual-use in the
/// same way `ExprKind::Index` is: `m[addr] <- v` (a memory write) and
/// `q[3] <- v` (a bit-select register write) are the same AST shape, and
/// only whether the base names a `design.mems` entry separates them.
enum Target {
    Signal(String),
    MemWrite(String),
}

/// Lowers an elaborated `Design` into a `Module`.
///
/// `design.asserts` / `design.covers` are intentionally never read here —
/// they're verification-only and never synthesized (design doc, "assert/
/// cover... dropped entirely at lowering").
pub fn lower(design: &Design) -> Module {
    let mut module = Module {
        name: design.module.clone(),
        ports: Vec::new(),
        cells: Vec::new(),
        nets: Vec::new(),
        extern_decls: BTreeMap::new(),
    };
    let mut ctx = LowerCtx {
        design,
        resolved: HashMap::new(),
        mem_read: HashMap::new(),
        expr_memo: HashMap::new(),
    };

    for input in &design.inputs {
        let bits = module.alloc_bits(input.width.bits, Some(&input.name));
        ctx.resolved.insert(input.name.clone(), bits.clone());
        module.ports.push((input.name.clone(), bits, Dir::In));
    }
    // Registers must exist in `ctx.resolved` before any comb expression that
    // reads a register's current value is lowered.
    for reg in &design.regs {
        let q_bits = module.alloc_bits(reg.width.bits, Some(&reg.name));
        ctx.resolved.insert(reg.name.clone(), q_bits);
    }
    // Extern-instance outputs (`design.unknown_signals`) are driverless by
    // design — no `comb` entry, so `ctx.resolve`'s panic path would fire on
    // them. Pre-populate their `Bits` here too, same as inputs/reg Qs above,
    // so both the wire-resolution loop below and the `BlackBox`-cell loop
    // (after it) — and any ordinary wire that happens to read one — see an
    // already-allocated net instead of a missing driver.
    for name in &design.unknown_signals {
        let width = design
            .wires
            .iter()
            .find(|w| w.name == *name)
            .unwrap_or_else(|| panic!("unknown_signals entry `{name}` has no matching wire"))
            .width
            .bits;
        let bits = module.alloc_bits(width, Some(name));
        ctx.resolved.insert(name.clone(), bits);
    }
    for output in &design.outputs {
        let bits = ctx.resolve(&mut module, &output.name);
        module.ports.push((output.name.clone(), bits, Dir::Out));
    }
    // Force every wire to be lowered even if no output reads it (keeps
    // dead-wire diagnostics/validation meaningful; the optimizer's future
    // dead-signal-elimination pass is the place that actually removes it).
    // Also assign the wire's name to all of its unnamed nets (a pure
    // arithmetic result's nets start with no name; assigning the wire's
    // name here makes the printer output readable: `out=sum[0:9]` instead
    // of `out={16,17,18,19,20,21,22,23,24}`).
    for wire in &design.wires {
        let bits = ctx.resolve(&mut module, &wire.name);
        for net_id in &bits.0 {
            if module.nets[net_id.0 as usize].name.is_none() {
                module.nets[net_id.0 as usize].name = Some(wire.name.clone());
            }
        }
    }

    // Extern-module instances (Task 11): one `BlackBox` cell per
    // `design.extern_instances` entry. Every port was already resolved
    // above — an input via its synthesized comb driver (just-run
    // wire-resolution loop), an output via the `unknown_signals`
    // pre-population — so this is a pure by-name pin lookup.
    //
    // `Cell::pins` keys are `&'static str` (every other cell kind's pin
    // names are literals baked into this file); a `BlackBox`'s pin names
    // are the extern module's own port names instead, known only at
    // lowering time. `Box::leak` is the standard way to mint a `'static`
    // str from a runtime `String` — ponytail: leaked bytes are bounded by
    // one design's extern-instance port count, not per-run growth, and a
    // compiler invocation is short-lived, so this never accumulates.
    for ext in &design.extern_instances {
        let pins: BTreeMap<&'static str, Bits> = ext
            .ports
            .iter()
            .map(|(port_name, sig)| {
                let bits = ctx.resolved.get(&sig.name).unwrap_or_else(|| {
                    panic!(
                        "extern instance port `{port_name}` (net `{}`) was not pre-resolved",
                        sig.name
                    )
                });
                let leaked: &'static str = Box::leak(port_name.clone().into_boxed_str());
                (leaked, bits.clone())
            })
            .collect();
        module.cells.push(Cell {
            kind: CellKind::BlackBox {
                module_name: ext.module_name.clone(),
            },
            pins,
            span: ext.span,
        });
        module.extern_decls.insert(
            ext.module_name.clone(),
            ext.ports
                .iter()
                .map(|(n, s)| (n.clone(), s.width.bits))
                .collect(),
        );
    }

    // Build each reg's D input from its driving `Process` and emit the Dff
    // cell — after wires/outputs are resolved, so any wire reading a
    // register's Q sees the net already allocated above.
    for reg in &design.regs {
        let q_bits = ctx.resolved[&reg.name].clone();
        if reg.clock.is_empty() {
            continue; // unassigned reg: holds its reset value forever, no Dff
        }
        let proc = design
            .procs
            .iter()
            .find(|p| p.clock == reg.clock && p.edge == reg.edge)
            .expect("checker guarantees exactly one process per (clock, edge) pair");
        let mut env: HashMap<String, Bits> = HashMap::new();
        env.insert(reg.name.clone(), q_bits.clone()); // unassigned path: keep current value
        ctx.lower_seq_stmts(&mut module, &proc.body, &mut env);
        let mut d_bits = env
            .remove(&reg.name)
            .expect("lower_seq_stmts always re-inserts every target it started with");

        if let Some(reset_name) = design.resets.first() {
            let reset_sel = ctx.resolve(&mut module, reset_name);
            let reset_const =
                ctx.lower_const(&mut module, &reg.reset, crate::span::Span::default());
            let out = module.alloc_bits(reg.width.bits, None);
            module.cells.push(Cell {
                kind: CellKind::Mux,
                pins: [
                    ("sel", reset_sel),
                    ("a", reset_const),
                    ("b", d_bits),
                    ("out", out.clone()),
                ]
                .into_iter()
                .collect(),
                span: crate::span::Span::default(),
            });
            d_bits = out;
        }

        let clock_bits = ctx.resolve(&mut module, &reg.clock);
        assert_eq!(clock_bits.width(), 1, "a clock signal is always 1 bit");
        module.cells.push(Cell {
            kind: CellKind::Dff {
                clock: clock_bits.0[0],
                edge: reg.edge,
            },
            pins: [("d", d_bits), ("q", q_bits)].into_iter().collect(),
            span: crate::span::Span::default(),
        });
    }

    // Memories take TWO passes. Pass A walks every writing process, which is
    // itself a place `m[addr]` reads are discovered (`ram[wa] <- ram[ra]`, or
    // a read of one memory inside another's writer) — the register pass never
    // reaches them, because its `env.contains_key` guard skips every
    // `MemWrite` statement. Only once all of that has run is `ctx.mem_read`
    // complete, so pass B is the one that emits the cells.
    let mut writes: HashMap<String, (Bits, Bits, Bits, Bits)> = HashMap::new();
    for mem in &design.mems {
        if mem.clock.is_empty() {
            continue; // a ROM: no writing `on` block, so no write port
        }
        let proc = design
            .procs
            .iter()
            .find(|p| p.clock == mem.clock && p.edge == mem.edge)
            .expect("checker guarantees exactly one process per (clock, edge) pair");
        let (wen_k, waddr_k, wdata_k) = mem_write_keys(&mem.name);
        // Seeded with "no write on this edge"; every `if` the write sits
        // under folds against that seed, so an unguarded write comes out as a
        // constant-1 `wen` and a guarded one as `Mux(cond, 1, 0)`.
        let mut env: HashMap<String, Bits> = HashMap::new();
        let seed_wen = ctx.lower_const(&mut module, &const_val(0, 1), crate::span::Span::default());
        let seed_addr = ctx.lower_const(
            &mut module,
            &const_val(0, crate::checker::consteval::clog2_bits(mem.depth)),
            crate::span::Span::default(),
        );
        let seed_data = ctx.lower_const(
            &mut module,
            &const_val(0, mem.width.bits),
            crate::span::Span::default(),
        );
        env.insert(wen_k.clone(), seed_wen);
        env.insert(waddr_k.clone(), seed_addr);
        env.insert(wdata_k.clone(), seed_data);
        ctx.lower_seq_stmts(&mut module, &proc.body, &mut env);
        let expect = "lower_seq_stmts always re-inserts every target it started with";
        let wen = env.remove(&wen_k).expect(expect);
        let waddr = env.remove(&waddr_k).expect(expect);
        let wdata = env.remove(&wdata_k).expect(expect);
        let clock = ctx.resolve(&mut module, &mem.clock);
        assert_eq!(clock.width(), 1, "a clock signal is always 1 bit");
        writes.insert(mem.name.clone(), (wen, waddr, wdata, clock));
    }

    // Pass B: one `Mem` cell per `design.mems` entry. Read and write
    // addresses are INDEPENDENT pins (`raddr`/`waddr`), matching the
    // simulator kernel, which reads combinationally from the pre-tick array
    // while a write lands in `next_mems` — a same-cycle read of the written
    // cell still sees the old value. Sharing one address bus would make the
    // read follow the write address whenever `wen` is high and diverge from
    // the kernel on the canonical register-file shape.
    for mem in &design.mems {
        let addr_width = crate::checker::consteval::clog2_bits(mem.depth);
        let (raddr, rdata) = match ctx.mem_read.get(&mem.name).cloned() {
            Some(pair) => pair,
            // Never read anywhere: the cell still exists (a write-only memory
            // is legal), it just has a constant read address and a read port
            // that goes nowhere.
            None => {
                let raddr = ctx.lower_const(
                    &mut module,
                    &const_val(0, addr_width),
                    crate::span::Span::default(),
                );
                let rdata = module.alloc_bits(mem.width.bits, Some(&mem.name));
                (raddr, rdata)
            }
        };
        let mut pins: BTreeMap<&'static str, Bits> =
            [("raddr", raddr), ("rdata", rdata)].into_iter().collect();

        match writes.remove(&mem.name) {
            Some((wen, waddr, wdata, clock)) => {
                pins.insert("waddr", waddr);
                pins.insert("wdata", wdata);
                pins.insert("wen", wen);
                pins.insert("clock", clock);
            }
            // A ROM: write port tied off, and no clock SIGNAL to point a
            // `clock` pin at. `CellKind::Mem` (unlike `Dff`) carries its
            // clock in `pins` rather than as a struct field, so "absent" is
            // directly expressible — leave the pin out rather than invent a
            // NetId for a clock that isn't there.
            None => {
                let waddr = ctx.lower_const(
                    &mut module,
                    &const_val(0, addr_width),
                    crate::span::Span::default(),
                );
                let wdata = ctx.lower_const(
                    &mut module,
                    &const_val(0, mem.width.bits),
                    crate::span::Span::default(),
                );
                let wen =
                    ctx.lower_const(&mut module, &const_val(0, 1), crate::span::Span::default());
                pins.insert("waddr", waddr);
                pins.insert("wdata", wdata);
                pins.insert("wen", wen);
            }
        }

        module.cells.push(Cell {
            kind: CellKind::Mem {
                depth: mem.depth,
                init: mem.init.clone(),
            },
            pins,
            span: crate::span::Span::default(),
        });
    }

    module
}
