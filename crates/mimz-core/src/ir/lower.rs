//! `elaborate::Design` -> `ir::Module` lowering.

use super::{Bits, Cell, CellKind, Module};
use crate::ast::{BinOp, Dir, Expr, ExprKind, UnOp};
use crate::elaborate::Design;
use std::collections::HashMap;

/// Lowering state threaded through one module's lowering: which
/// signal names have already been turned into `Bits`, memoized so a wire
/// referenced by more than one comb expression is lowered exactly once
/// (the AST checker already guarantees `comb` forms a DAG, so plain
/// memoized recursion terminates — no separate topo-sort needed).
struct LowerCtx<'a> {
    design: &'a Design,
    resolved: HashMap<String, Bits>,
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
        let bits = self.lower_expr(module, expr);
        self.resolved.insert(name.to_string(), bits.clone());
        bits
    }

    fn lower_expr(&mut self, module: &mut Module, e: &Expr) -> Bits {
        match &e.kind {
            ExprKind::Ident(name) => self.resolve(module, name),
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
                let a = self.lower_expr(module, lhs);
                let b = self.lower_expr(module, rhs);
                self.lower_binop(module, *op, a, b, e.span)
            }
            ExprKind::Unary { op, expr } => {
                let a = self.lower_expr(module, expr);
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
                    let bits = self.lower_expr(module, part);
                    ids.extend(bits.0);
                }
                Bits(ids)
            }
            // `base[hi:lo]`, both bounds inclusive. `hi`/`lo` always
            // const-fold (checker-enforced) — the same const_eval promoted
            // from mimz-sim in Task 1. No cell: a sub-range of existing
            // nets, not a new value.
            ExprKind::Slice { base, hi, lo } => {
                let base_bits = self.lower_expr(module, base);
                let hi_val = crate::value::const_eval(hi, &self.design.consts)
                    .expect("checker guarantees slice bounds const-fold")
                    as usize;
                let lo_val = crate::value::const_eval(lo, &self.design.consts)
                    .expect("checker guarantees slice bounds const-fold")
                    as usize;
                Bits(base_bits.0[lo_val..=hi_val].to_vec())
            }
            ExprKind::IfExpr { cond, then, els } => {
                let sel = self.lower_expr(module, cond);
                let a = self.lower_expr(module, then);
                let b = self.lower_expr(module, els);
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
            ExprKind::Match { scrutinee, arms } => {
                self.lower_match(module, scrutinee, arms, e.span)
            }
            other => unimplemented!(
                "expression form not yet lowered by Task 5/6 (see later tasks for \
                 field access, index): {other:?}"
            ),
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
    ) -> Bits {
        let scrutinee_bits = self.lower_expr(module, scrutinee);
        let n = arms.len();
        let mut acc = self.lower_expr(module, &arms[n - 1].value);
        for arm in arms[..n - 1].iter().rev() {
            let sel = self.lower_pattern_conds(module, &scrutinee_bits, &arm.patterns, span);
            let arm_value = self.lower_expr(module, &arm.value);
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
}

/// Lowers an elaborated `Design` into a `Module`.
pub fn lower(design: &Design) -> Module {
    let mut module = Module {
        name: design.module.clone(),
        ports: Vec::new(),
        cells: Vec::new(),
        nets: Vec::new(),
    };
    let mut ctx = LowerCtx {
        design,
        resolved: HashMap::new(),
    };

    for input in &design.inputs {
        let bits = module.alloc_bits(input.width.bits, Some(&input.name));
        ctx.resolved.insert(input.name.clone(), bits.clone());
        module.ports.push((input.name.clone(), bits, Dir::In));
    }
    for output in &design.outputs {
        let bits = ctx.resolve(&mut module, &output.name);
        module.ports.push((output.name.clone(), bits, Dir::Out));
    }
    // Force every wire to be lowered even if no output reads it (keeps
    // dead-wire diagnostics/validation meaningful; the optimizer's future
    // dead-signal-elimination pass is the place that actually removes it).
    for wire in &design.wires {
        ctx.resolve(&mut module, &wire.name);
    }
    module
}
