//! Lowers `ModuleItem::SyncLoop` into ordinary `Port`/`Reg`/`On`/`Drive`
//! items — a counter register + IDLE/RUNNING state machine spanning
//! `hi - lo` clock cycles. The checker validates the ORIGINAL `SyncLoop`
//! node directly (span-accurate diagnostics); the emitter and simulator
//! both call this ONE function and process its output through their
//! existing Port/Reg/On/Drive handling, unchanged — no new Verilog codegen
//! shape, no new simulator kernel dispatch.

use std::collections::HashMap;

use super::{
    Arm, BinOp, Builtin, Dir, Expr, ExprKind, FieldInit, ForEachSource, Ident, LValue, ModuleItem,
    OnBlock, Pattern, SeqStmt, SyncLoop, Type,
};

/// Lower one `sync loop` instance into 12 synthesized items, in this order:
/// 4 ports (`_start` in; `_done`, `_result`, `_running` out), 4 internal
/// regs (`_cnt`, `_running_r`, `_acc`, `_done_r`), 1 `on`-block implementing
/// the FSM, 3 drives connecting the internal regs to their output ports.
pub fn lower_sync_loop(sl: &SyncLoop) -> Vec<ModuleItem> {
    let sp = sl.span;
    let base = &sl.name.name;
    let ident = |n: String| Ident { name: n, span: sp };
    let ident_expr = |n: String| Expr {
        kind: ExprKind::Ident(n),
        span: sp,
    };
    let int_expr = |v: u128| Expr {
        kind: ExprKind::Int {
            value: v,
            raw: v.to_string(),
        },
        span: sp,
    };

    let start_name = format!("{base}_start");
    let done_name = format!("{base}_done");
    let result_name = format!("{base}_result");
    let running_name = format!("{base}_running");
    let cnt_name = format!("{base}_cnt");
    let running_r_name = format!("{base}_running_r");
    let acc_name = format!("{base}_acc");
    let done_r_name = format!("{base}_done_r");

    // Counter width: `bits[clog2(hi)]`. The counter holds the *live index
    // value* (it's `lo` at start, increments up to `hi - 1`), not just the
    // iteration count — so it must be sized to represent `hi - 1`, the
    // largest value it ever holds, regardless of `lo`. Reuses the existing
    // `clog2` builtin so this stays a pure AST-to-AST transform with no
    // const-eval dependency (keeps `ast` free of a dependency on `checker`).
    let range_span = sl.lo.span.join(sl.hi.span);
    let cnt_ty = Type::Bits(Box::new(Expr {
        kind: ExprKind::Call {
            func: Builtin::Clog2,
            args: vec![sl.hi.clone()],
        },
        span: range_span,
    }));

    // Rewrite the loop variable and accumulator name to their physical
    // register names (`i` -> `<name>_cnt`, `result` -> `<name>_acc`) so the
    // body, once spliced into the synthesized `on`-block, references real
    // signals with zero further special-casing anywhere downstream.
    let mut rename = HashMap::new();
    rename.insert(sl.var.name.clone(), cnt_name.clone());
    rename.insert(sl.result_name.name.clone(), acc_name.clone());
    let body: Vec<SeqStmt> = sl
        .body
        .iter()
        .map(|s| rename_seq_stmt(s, &rename))
        .collect();

    let assign = |name: &str, rhs: Expr| SeqStmt::Assign {
        lhs: LValue {
            base: ident(name.to_string()),
            index: None,
            span: sp,
        },
        rhs,
    };

    // `if cnt == hi - 1 { running_r <- 0; done_r <- 1 }
    //  else { cnt <- cnt + 1; done_r <- 0 }`
    let last_iter = SeqStmt::If {
        cond: Expr {
            kind: ExprKind::Binary {
                op: BinOp::Eq,
                lhs: Box::new(ident_expr(cnt_name.clone())),
                rhs: Box::new(Expr {
                    kind: ExprKind::Binary {
                        op: BinOp::Sub,
                        lhs: Box::new(sl.hi.clone()),
                        rhs: Box::new(int_expr(1)),
                    },
                    span: sp,
                }),
            },
            span: sp,
        },
        then: vec![
            assign(&running_r_name, int_expr(0)),
            assign(&done_r_name, int_expr(1)),
        ],
        els: Some(vec![
            assign(
                &cnt_name,
                Expr {
                    kind: ExprKind::Binary {
                        op: BinOp::Add,
                        lhs: Box::new(ident_expr(cnt_name.clone())),
                        rhs: Box::new(int_expr(1)),
                    },
                    span: sp,
                },
            ),
            assign(&done_r_name, int_expr(0)),
        ]),
    };

    let mut running_branch = vec![last_iter];
    running_branch.extend(body);

    // `if start { running_r <- 1; cnt <- lo; acc <- init }`
    let start_branch = SeqStmt::If {
        cond: ident_expr(start_name.clone()),
        then: vec![
            assign(&running_r_name, int_expr(1)),
            assign(&cnt_name, sl.lo.clone()),
            assign(&acc_name, sl.result_init.clone()),
        ],
        els: None,
    };

    // `if running_r { <running_branch> } else { done_r <- 0; <start_branch> }`
    let fsm = SeqStmt::If {
        cond: ident_expr(running_r_name.clone()),
        then: running_branch,
        els: Some(vec![assign(&done_r_name, int_expr(0)), start_branch]),
    };

    vec![
        ModuleItem::Port {
            dir: Dir::In,
            name: ident(start_name.clone()),
            ty: Type::Bit,
        },
        ModuleItem::Port {
            dir: Dir::Out,
            name: ident(done_name.clone()),
            ty: Type::Bit,
        },
        ModuleItem::Port {
            dir: Dir::Out,
            name: ident(result_name.clone()),
            ty: sl.result_ty.clone(),
        },
        ModuleItem::Port {
            dir: Dir::Out,
            name: ident(running_name.clone()),
            ty: Type::Bit,
        },
        ModuleItem::Reg {
            name: ident(cnt_name),
            ty: cnt_ty,
            reset: int_expr(0),
        },
        ModuleItem::Reg {
            name: ident(running_r_name.clone()),
            ty: Type::Bit,
            reset: int_expr(0),
        },
        ModuleItem::Reg {
            name: ident(acc_name.clone()),
            ty: sl.result_ty.clone(),
            reset: sl.result_init.clone(),
        },
        ModuleItem::Reg {
            name: ident(done_r_name.clone()),
            ty: Type::Bit,
            reset: int_expr(0),
        },
        ModuleItem::On(OnBlock {
            clock: sl.clock.clone(),
            edge: sl.edge,
            body: vec![fsm],
            span: sp,
        }),
        ModuleItem::Drive {
            lhs: LValue {
                base: ident(done_name),
                index: None,
                span: sp,
            },
            rhs: ident_expr(done_r_name),
        },
        ModuleItem::Drive {
            lhs: LValue {
                base: ident(result_name),
                index: None,
                span: sp,
            },
            rhs: ident_expr(acc_name),
        },
        ModuleItem::Drive {
            lhs: LValue {
                base: ident(running_name),
                index: None,
                span: sp,
            },
            rhs: ident_expr(running_r_name),
        },
    ]
}

fn rename_seq_stmt(s: &SeqStmt, rename: &HashMap<String, String>) -> SeqStmt {
    match s {
        SeqStmt::Assign { lhs, rhs } => SeqStmt::Assign {
            lhs: rename_lvalue(lhs, rename),
            rhs: rename_expr(rhs, rename),
        },
        SeqStmt::If { cond, then, els } => SeqStmt::If {
            cond: rename_expr(cond, rename),
            then: then.iter().map(|s| rename_seq_stmt(s, rename)).collect(),
            els: els
                .as_ref()
                .map(|b| b.iter().map(|s| rename_seq_stmt(s, rename)).collect()),
        },
        SeqStmt::Default { name, val, span } => SeqStmt::Default {
            name: rename_ident(name, rename),
            val: rename_expr(val, rename),
            span: *span,
        },
        SeqStmt::Loop {
            var,
            lo,
            hi,
            body,
            span,
        } => {
            // A nested bare `loop`'s own variable shadows an outer name of
            // the same spelling within its own body — same shadowing rule
            // `checker/names.rs` already applies to `repeat`/`loop` bodies.
            let mut inner = rename.clone();
            inner.remove(&var.name);
            SeqStmt::Loop {
                var: var.clone(),
                lo: rename_expr(lo, rename),
                hi: rename_expr(hi, rename),
                body: body.iter().map(|s| rename_seq_stmt(s, &inner)).collect(),
                span: *span,
            }
        }
        SeqStmt::ForEach {
            var,
            source,
            body,
            span,
        } => {
            // Same shadowing rule as `SeqStmt::Loop` above: a nested
            // `foreach`'s own `var` shadows an outer name of the same
            // spelling within its own body. `source` is evaluated in the
            // OUTER scope (before `var` exists), so it always uses the
            // unmodified `rename` map.
            let renamed_source = match source {
                ForEachSource::Range { lo, hi } => ForEachSource::Range {
                    lo: rename_expr(lo, rename),
                    hi: rename_expr(hi, rename),
                },
                ForEachSource::Elements(id) => ForEachSource::Elements(rename_ident(id, rename)),
            };
            let mut inner = rename.clone();
            inner.remove(&var.name);
            SeqStmt::ForEach {
                var: var.clone(),
                source: renamed_source,
                body: body.iter().map(|s| rename_seq_stmt(s, &inner)).collect(),
                span: *span,
            }
        }
        SeqStmt::Error(sp) => SeqStmt::Error(*sp),
    }
}

fn rename_lvalue(l: &LValue, rename: &HashMap<String, String>) -> LValue {
    LValue {
        base: rename_ident(&l.base, rename),
        index: l.index.as_ref().map(|(i, s)| {
            (
                rename_expr(i, rename),
                s.as_ref().map(|s| rename_expr(s, rename)),
            )
        }),
        span: l.span,
    }
}

fn rename_ident(id: &Ident, rename: &HashMap<String, String>) -> Ident {
    match rename.get(&id.name) {
        Some(new_name) => Ident {
            name: new_name.clone(),
            span: id.span,
        },
        None => id.clone(),
    }
}

fn rename_expr(e: &Expr, rename: &HashMap<String, String>) -> Expr {
    let kind = match &e.kind {
        ExprKind::Ident(n) => ExprKind::Ident(rename.get(n).cloned().unwrap_or_else(|| n.clone())),
        ExprKind::Int { .. } | ExprKind::Bool(_) => e.kind.clone(),
        ExprKind::Field { base, field } => ExprKind::Field {
            base: Box::new(rename_expr(base, rename)),
            field: field.clone(),
        },
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op: *op,
            expr: Box::new(rename_expr(expr, rename)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op: *op,
            lhs: Box::new(rename_expr(lhs, rename)),
            rhs: Box::new(rename_expr(rhs, rename)),
        },
        ExprKind::IfExpr { cond, then, els } => ExprKind::IfExpr {
            cond: Box::new(rename_expr(cond, rename)),
            then: Box::new(rename_expr(then, rename)),
            els: Box::new(rename_expr(els, rename)),
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(rename_expr(scrutinee, rename)),
            arms: arms
                .iter()
                .map(|a| {
                    // Each `Pattern::Variant`'s `bindings` are names scoped
                    // to this arm's `value` only (see `checker/names.rs`,
                    // which treats them as real per-arm bindings) — they
                    // shadow an outer name of the same spelling, same rule
                    // already applied to `SeqStmt::Loop` above.
                    let mut inner = rename.clone();
                    for p in &a.patterns {
                        if let Pattern::Variant { bindings, .. } = p {
                            for b in bindings {
                                inner.remove(&b.name);
                            }
                        }
                    }
                    Arm {
                        patterns: a.patterns.clone(),
                        value: rename_expr(&a.value, &inner),
                    }
                })
                .collect(),
        },
        ExprKind::Concat(parts) => {
            ExprKind::Concat(parts.iter().map(|p| rename_expr(p, rename)).collect())
        }
        ExprKind::Replicate { count, parts } => ExprKind::Replicate {
            count: Box::new(rename_expr(count, rename)),
            parts: parts.iter().map(|p| rename_expr(p, rename)).collect(),
        },
        ExprKind::Index { base, index } => ExprKind::Index {
            base: Box::new(rename_expr(base, rename)),
            index: Box::new(rename_expr(index, rename)),
        },
        ExprKind::Slice { base, hi, lo } => ExprKind::Slice {
            base: Box::new(rename_expr(base, rename)),
            hi: Box::new(rename_expr(hi, rename)),
            lo: Box::new(rename_expr(lo, rename)),
        },
        ExprKind::Call { func, args } => ExprKind::Call {
            func: *func,
            args: args.iter().map(|a| rename_expr(a, rename)).collect(),
        },
        ExprKind::FnCall { name, args } => ExprKind::FnCall {
            name: name.clone(),
            args: args.iter().map(|a| rename_expr(a, rename)).collect(),
        },
        ExprKind::BundleLit(fields) => ExprKind::BundleLit(
            fields
                .iter()
                .map(|f| FieldInit {
                    name: f.name.clone(),
                    value: rename_expr(&f.value, rename),
                    span: f.span,
                })
                .collect(),
        ),
        ExprKind::ArrayLit(items) => {
            ExprKind::ArrayLit(items.iter().map(|i| rename_expr(i, rename)).collect())
        }
    };
    Expr { kind, span: e.span }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Edge;
    use crate::span::Span;

    #[test]
    fn lower_produces_twelve_items_in_order() {
        let sp = Span::new(0, 0);
        let id = |n: &str| Ident {
            name: n.into(),
            span: sp,
        };
        let int = |v: u128| Expr {
            kind: ExprKind::Int {
                value: v,
                raw: v.to_string(),
            },
            span: sp,
        };
        let sl = SyncLoop {
            name: id("find_first"),
            clock: id("clk"),
            edge: Edge::Rise,
            var: id("i"),
            lo: int(0),
            hi: int(8),
            result_name: id("result"),
            result_ty: Type::Bit,
            result_init: int(0),
            body: vec![SeqStmt::Assign {
                lhs: LValue {
                    base: id("result"),
                    index: None,
                    span: sp,
                },
                rhs: Expr {
                    kind: ExprKind::Ident("i".into()),
                    span: sp,
                },
            }],
            span: sp,
        };
        let items = lower_sync_loop(&sl);
        assert_eq!(items.len(), 12);
        assert!(matches!(items[0], ModuleItem::Port { dir: Dir::In, .. }));
        let ModuleItem::On(on) = &items[8] else {
            panic!("item 8 must be the on-block")
        };
        assert_eq!(on.clock.name, "clk");
        // Confirm the body's `result <- i` was rewritten to `find_first_acc <- find_first_cnt`.
        let SeqStmt::If { els: Some(els), .. } = &on.body[0] else {
            panic!()
        };
        let SeqStmt::If { then, .. } = &els[1] else {
            panic!("expected the start branch")
        };
        // `then` here is the start_branch's own body; the running-branch's
        // spliced-in loop body (with the rename applied) is checked below.
        let _ = then;

        let SeqStmt::If {
            then: running_then, ..
        } = &on.body[0]
        else {
            panic!()
        };
        // running_then[0] is last_iter, running_then[1] is the spliced body.
        let SeqStmt::Assign { lhs, rhs } = &running_then[1] else {
            panic!("expected the renamed body assign")
        };
        assert_eq!(lhs.base.name, "find_first_acc");
        let ExprKind::Ident(rhs_name) = &rhs.kind else {
            panic!()
        };
        assert_eq!(rhs_name, "find_first_cnt");
    }

    /// Regression for the counter-width bug: with `lo != 0`, `clog2(hi - lo)`
    /// (the iteration *count*) is too narrow for the live index *value* the
    /// `_cnt` register actually holds (`lo` up to `hi - 1`). `lo=4, hi=12`
    /// pins the tight formula — `clog2(hi-lo)=clog2(8)=3` bits would be
    /// wrong (can't hold 11), `clog2(hi)=clog2(12)=4` bits is correct. The
    /// `lo=0` case in `lower_produces_twelve_items_in_order` can't catch
    /// this: `hi - 0 == hi`, so the buggy and fixed formulas coincide there.
    #[test]
    fn counter_width_is_clog2_hi_not_clog2_range_when_lo_nonzero() {
        let sp = Span::new(0, 0);
        let id = |n: &str| Ident {
            name: n.into(),
            span: sp,
        };
        let int = |v: u128| Expr {
            kind: ExprKind::Int {
                value: v,
                raw: v.to_string(),
            },
            span: sp,
        };
        let sl = SyncLoop {
            name: id("scan"),
            clock: id("clk"),
            edge: Edge::Rise,
            var: id("i"),
            lo: int(4),
            hi: int(12),
            result_name: id("result"),
            result_ty: Type::Bit,
            result_init: int(0),
            body: vec![],
            span: sp,
        };
        let items = lower_sync_loop(&sl);
        let ModuleItem::Reg { ty, .. } = &items[4] else {
            panic!("item 4 must be the _cnt reg")
        };
        let Type::Bits(width) = ty else {
            panic!("cnt reg must be Type::Bits")
        };
        let ExprKind::Call {
            func: Builtin::Clog2,
            args,
        } = &width.kind
        else {
            panic!("cnt width must be a clog2(...) call")
        };
        assert_eq!(args.len(), 1, "clog2 must take exactly one argument");
        let ExprKind::Int { value, .. } = &args[0].kind else {
            panic!("expected an int literal arg")
        };
        assert_eq!(
            *value, 12,
            "counter width must be clog2(hi) = clog2(12), got clog2({value})"
        );
    }

    /// Regression for the match-arm shadowing bug: a `Pattern::Variant`
    /// binding introduces a name scoped to that arm's `value` only
    /// (`checker/names.rs` treats it as a real per-arm binding) — it must
    /// shadow an outer name of the same spelling, exactly like the
    /// `SeqStmt::Loop` case already handles. Here the pattern binds `result`
    /// (same spelling as the accumulator) in arm 0; arm 1's wildcard binds
    /// nothing, so its reference to the real loop var `i` must still be
    /// rewritten to the physical counter register name.
    #[test]
    fn rename_expr_match_arm_binding_shadows_accumulator_name() {
        let sp = Span::new(0, 0);
        let id = |n: &str| Ident {
            name: n.into(),
            span: sp,
        };
        let int = |v: u128| Expr {
            kind: ExprKind::Int {
                value: v,
                raw: v.to_string(),
            },
            span: sp,
        };
        let sl = SyncLoop {
            name: id("scan"),
            clock: id("clk"),
            edge: Edge::Rise,
            var: id("i"),
            lo: int(0),
            hi: int(8),
            result_name: id("result"),
            result_ty: Type::Bit,
            result_init: int(0),
            body: vec![SeqStmt::Assign {
                lhs: LValue {
                    base: id("result"),
                    index: None,
                    span: sp,
                },
                rhs: Expr {
                    kind: ExprKind::Match {
                        scrutinee: Box::new(Expr {
                            kind: ExprKind::Ident("i".into()),
                            span: sp,
                        }),
                        arms: vec![
                            Arm {
                                // `Enum.Tag(result)` — binds a fresh local `result`,
                                // shadowing the outer accumulator name within this
                                // arm's value only.
                                patterns: vec![Pattern::Variant {
                                    enum_name: id("E"),
                                    variant: id("Tag"),
                                    bindings: vec![id("result")],
                                }],
                                value: Expr {
                                    kind: ExprKind::Ident("result".into()),
                                    span: sp,
                                },
                            },
                            Arm {
                                patterns: vec![Pattern::Wildcard],
                                value: Expr {
                                    kind: ExprKind::Ident("i".into()),
                                    span: sp,
                                },
                            },
                        ],
                    },
                    span: sp,
                },
            }],
            span: sp,
        };
        let items = lower_sync_loop(&sl);
        let ModuleItem::On(on) = &items[8] else {
            panic!("item 8 must be the on-block")
        };
        let SeqStmt::If {
            then: running_then, ..
        } = &on.body[0]
        else {
            panic!()
        };
        let SeqStmt::Assign { rhs, .. } = &running_then[1] else {
            panic!("expected the renamed body assign")
        };
        let ExprKind::Match { scrutinee, arms } = &rhs.kind else {
            panic!("expected the match expr")
        };

        // The scrutinee references the real loop var `i` -> must be renamed.
        let ExprKind::Ident(scrutinee_name) = &scrutinee.kind else {
            panic!()
        };
        assert_eq!(scrutinee_name, "scan_cnt");

        // Arm 0's `value` refers to the pattern-bound `result`, NOT the
        // accumulator -> must stay `result`, unrewritten.
        let ExprKind::Ident(arm0_name) = &arms[0].value.kind else {
            panic!()
        };
        assert_eq!(
            arm0_name, "result",
            "pattern-bound name must not be renamed to the accumulator register"
        );

        // Arm 1's `value` refers to the real loop var `i` -> must be renamed.
        let ExprKind::Ident(arm1_name) = &arms[1].value.kind else {
            panic!()
        };
        assert_eq!(arm1_name, "scan_cnt");
    }
}
