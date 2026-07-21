//! Lowers `sync.double_flop`/`sync.pulse` call sites into ordinary
//! `Reg`/`On`/`Wire` items — see `docs/superpowers/specs/2026-07-20-sync-cdc-design.local.md`
//! §6. Assumes checker-clean input (Tasks 2-3 already validated every call's
//! shape, domain, and placement) — same contract `sync_loop_lower`/
//! `foreach_lower` already have with their own checker passes.
//!
//! Hidden names are derived from the call site's own target name (the
//! `<-` target's name for `double_flop`, the wire's name for `pulse`),
//! not a global counter: the checker already guarantees module-wide name
//! uniqueness (E0003), so `__sync_<target>_*` is unique for free and stays
//! deterministic across repeated compiler runs (golden-file friendly) —
//! the same precedent `sync_loop_lower` already uses (namespacing off the
//! loop's own `name` field).

use super::{
    BinOp, Builtin, Edge, Expr, ExprKind, Ident, LValue, ModuleItem, OnBlock, SeqStmt, Type,
};

/// Rewrites every `sync.double_flop`/`sync.pulse` call site in `items` into
/// its lowered hidden `Reg`/`On` items plus a rewritten host `On`/`Wire`
/// item. Only scans DIRECT `items` — a call nested inside a `const if`
/// branch, `repeat`, `foreach`, or `sync loop` is out of scope for v1 (see
/// the spec's §8 open items / this plan's Global Constraints).
///
/// Called once, before any other pass sees the module — both
/// `emit_verilog::module::flatten_items` (Task 5) and `mimz-sim`'s
/// `elaborate_module` (Task 6) call this FIRST, then run their existing
/// `SyncLoop`/`ForEach` handling on the result unchanged (the two concerns
/// are orthogonal: this function never touches `SyncLoop`/`ForEach` items,
/// and vice versa).
pub fn expand_sync_prims(items: &[ModuleItem]) -> Vec<ModuleItem> {
    let mut out = Vec::new();
    for item in items {
        match item {
            ModuleItem::On(on) => {
                if let Some((hidden_regs, new_on)) = lower_double_flop_in_on_block(on) {
                    out.extend(hidden_regs);
                    out.push(ModuleItem::On(new_on));
                } else {
                    out.push(item.clone());
                }
            }
            ModuleItem::Wire { name, ty, init } => {
                if let ExprKind::Call {
                    func: Builtin::SyncPulse,
                    args,
                } = &init.kind
                {
                    out.extend(lower_pulse(name, ty, args));
                } else {
                    out.push(item.clone());
                }
            }
            _ => out.push(item.clone()),
        }
    }
    out
}

/// If `on`'s body contains one or more `sync.double_flop(...)` calls (as the
/// direct rhs of an `Assign` statement — checker-guaranteed placement, per
/// E0705), returns one hidden `Reg` item per call plus a rewritten copy of
/// `on` with each stage0 assign spliced in before its own rewritten user
/// assign. The checker allows any number of `sync.double_flop` calls in the
/// SAME on-block (each targeting a different `<-` destination) — `names.rs`'s
/// collision-prediction arm already declares one hidden name per call, so
/// this must lower every one of them, not just the first.
fn lower_double_flop_in_on_block(on: &OnBlock) -> Option<(Vec<ModuleItem>, OnBlock)> {
    let mut hidden_regs = Vec::new();
    let mut new_body = Vec::with_capacity(on.body.len());
    for stmt in &on.body {
        let SeqStmt::Assign { lhs, rhs } = stmt else {
            new_body.push(stmt.clone());
            continue;
        };
        let ExprKind::Call {
            func: Builtin::SyncDoubleFlop,
            args,
        } = &rhs.kind
        else {
            new_body.push(stmt.clone());
            continue;
        };
        let signal = args[0].clone();
        let sp = rhs.span;
        let stage0_name = format!("__sync_{}_stage0", lhs.base.name);

        hidden_regs.push(ModuleItem::Reg {
            name: Ident {
                name: stage0_name.clone(),
                span: sp,
            },
            ty: Type::Bit,
            reset: Expr {
                kind: ExprKind::Int {
                    value: 0,
                    raw: "0".into(),
                },
                span: sp,
            },
        });

        new_body.push(SeqStmt::Assign {
            lhs: LValue {
                base: Ident {
                    name: stage0_name.clone(),
                    span: sp,
                },
                index: None,
                span: sp,
            },
            rhs: signal,
        });
        new_body.push(SeqStmt::Assign {
            lhs: lhs.clone(),
            rhs: Expr {
                kind: ExprKind::Ident(stage0_name),
                span: sp,
            },
        });
    }

    if hidden_regs.is_empty() {
        return None;
    }

    Some((
        hidden_regs,
        OnBlock {
            clock: on.clock.clone(),
            edge: on.edge,
            body: new_body,
            span: on.span,
        },
    ))
}

/// Lowers one `sync.pulse(signal, src_clock, dst_clock)` wire initializer
/// into: 1 toggle reg + its own `on rise(src_clock)` block; 3 sync-stage
/// regs + its own `on rise(dst_clock)` block; and the rewritten `Wire`
/// itself, now driven combinationally by `stage1 ^ stage2`. Always
/// synthesizes BOTH `On` blocks fresh — never searches for/merges into an
/// existing same-clock block, mirroring `ast::sync_loop_lower`'s own
/// precedent of always adding its own dedicated `On` block.
fn lower_pulse(wire_name: &Ident, wire_ty: &Type, args: &[Expr]) -> Vec<ModuleItem> {
    let signal = args[0].clone();
    let ExprKind::Ident(src_clock) = &args[1].kind else {
        unreachable!("checker (E0702) guarantees a bare clock Ident here")
    };
    let ExprKind::Ident(dst_clock) = &args[2].kind else {
        unreachable!("checker (E0702) guarantees a bare clock Ident here")
    };
    let sp = wire_name.span;
    let base = &wire_name.name;
    let toggle_name = format!("__sync_{base}_toggle");
    let stage0_name = format!("__sync_{base}_stage0");
    let stage1_name = format!("__sync_{base}_stage1");
    let stage2_name = format!("__sync_{base}_stage2");

    let zero = || Expr {
        kind: ExprKind::Int {
            value: 0,
            raw: "0".into(),
        },
        span: sp,
    };
    let ident_expr = |n: &str| Expr {
        kind: ExprKind::Ident(n.to_string()),
        span: sp,
    };
    let ident = |n: &str| Ident {
        name: n.to_string(),
        span: sp,
    };
    let assign = |name: &str, rhs: Expr| SeqStmt::Assign {
        lhs: LValue {
            base: ident(name),
            index: None,
            span: sp,
        },
        rhs,
    };
    let reg = |name: &str| ModuleItem::Reg {
        name: ident(name),
        ty: Type::Bit,
        reset: zero(),
    };

    let toggle_reg = reg(&toggle_name);
    let toggle_on = ModuleItem::On(OnBlock {
        clock: ident(src_clock),
        edge: Edge::Rise,
        body: vec![assign(
            &toggle_name,
            Expr {
                kind: ExprKind::Binary {
                    op: BinOp::BitXor,
                    lhs: Box::new(ident_expr(&toggle_name)),
                    rhs: Box::new(signal),
                },
                span: sp,
            },
        )],
        span: sp,
    });

    let stage0_reg = reg(&stage0_name);
    let stage1_reg = reg(&stage1_name);
    let stage2_reg = reg(&stage2_name);
    let sync_on = ModuleItem::On(OnBlock {
        clock: ident(dst_clock),
        edge: Edge::Rise,
        body: vec![
            assign(&stage0_name, ident_expr(&toggle_name)),
            assign(&stage1_name, ident_expr(&stage0_name)),
            assign(&stage2_name, ident_expr(&stage1_name)),
        ],
        span: sp,
    });

    let rewritten_wire = ModuleItem::Wire {
        name: wire_name.clone(),
        ty: wire_ty.clone(),
        init: Expr {
            kind: ExprKind::Binary {
                op: BinOp::BitXor,
                lhs: Box::new(ident_expr(&stage1_name)),
                rhs: Box::new(ident_expr(&stage2_name)),
            },
            span: sp,
        },
    };

    vec![
        toggle_reg,
        toggle_on,
        stage0_reg,
        stage1_reg,
        stage2_reg,
        sync_on,
        rewritten_wire,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    fn ident(n: &str, sp: Span) -> Ident {
        Ident {
            name: n.into(),
            span: sp,
        }
    }
    fn ident_expr(n: &str, sp: Span) -> Expr {
        Expr {
            kind: ExprKind::Ident(n.into()),
            span: sp,
        }
    }
    fn call_expr(func: Builtin, args: Vec<Expr>, sp: Span) -> Expr {
        Expr {
            kind: ExprKind::Call { func, args },
            span: sp,
        }
    }

    #[test]
    fn double_flop_call_lowers_to_one_hidden_reg_and_a_rewritten_assign() {
        let sp = Span::new(0, 0);
        let items = vec![ModuleItem::On(OnBlock {
            clock: ident("clk_dst", sp),
            edge: Edge::Rise,
            body: vec![SeqStmt::Assign {
                lhs: LValue {
                    base: ident("slow_bit", sp),
                    index: None,
                    span: sp,
                },
                rhs: call_expr(
                    Builtin::SyncDoubleFlop,
                    vec![
                        ident_expr("fast_bit", sp),
                        ident_expr("clk_src", sp),
                        ident_expr("clk_dst", sp),
                    ],
                    sp,
                ),
            }],
            span: sp,
        })];
        let out = expand_sync_prims(&items);

        // Exactly one hidden Reg item added, plus the (rewritten) On block.
        assert_eq!(out.len(), 2);
        let ModuleItem::Reg { name, ty, .. } = &out[0] else {
            panic!("expected a hidden Reg item first, got {:?}", out[0])
        };
        assert!(name.name.starts_with("__sync"));
        assert!(name.name.ends_with("_stage0"));
        assert!(matches!(ty, Type::Bit));

        let ModuleItem::On(on) = &out[1] else {
            panic!("expected the On block second")
        };
        assert_eq!(
            on.body.len(),
            2,
            "hidden stage0 assign + rewritten user assign"
        );
        let SeqStmt::Assign {
            lhs: stage_lhs,
            rhs: stage_rhs,
        } = &on.body[0]
        else {
            panic!("expected the hidden stage0 assign first")
        };
        assert_eq!(stage_lhs.base.name, name.name);
        let ExprKind::Ident(stage_rhs_name) = &stage_rhs.kind else {
            panic!()
        };
        assert_eq!(stage_rhs_name, "fast_bit");

        let SeqStmt::Assign {
            lhs: user_lhs,
            rhs: user_rhs,
        } = &on.body[1]
        else {
            panic!("expected the rewritten user assign second")
        };
        assert_eq!(user_lhs.base.name, "slow_bit");
        let ExprKind::Ident(user_rhs_name) = &user_rhs.kind else {
            panic!("user assign's rhs must now be a plain Ident, not the Call")
        };
        assert_eq!(user_rhs_name, &name.name);
    }

    #[test]
    fn two_double_flop_calls_in_the_same_on_block_both_get_lowered() {
        // Regression: the checker allows multiple `sync.double_flop` calls
        // in one `on` block (each targeting a different `<-` destination) —
        // `lower_double_flop_in_on_block` must lower every one, not just the
        // first it finds.
        let sp = Span::new(0, 0);
        let items = vec![ModuleItem::On(OnBlock {
            clock: ident("clk_dst", sp),
            edge: Edge::Rise,
            body: vec![
                SeqStmt::Assign {
                    lhs: LValue {
                        base: ident("slow_a", sp),
                        index: None,
                        span: sp,
                    },
                    rhs: call_expr(
                        Builtin::SyncDoubleFlop,
                        vec![
                            ident_expr("fast_a", sp),
                            ident_expr("clk_src", sp),
                            ident_expr("clk_dst", sp),
                        ],
                        sp,
                    ),
                },
                SeqStmt::Assign {
                    lhs: LValue {
                        base: ident("slow_b", sp),
                        index: None,
                        span: sp,
                    },
                    rhs: call_expr(
                        Builtin::SyncDoubleFlop,
                        vec![
                            ident_expr("fast_b", sp),
                            ident_expr("clk_src", sp),
                            ident_expr("clk_dst", sp),
                        ],
                        sp,
                    ),
                },
            ],
            span: sp,
        })];
        let out = expand_sync_prims(&items);

        // 2 hidden Reg items + the (single, rewritten) On block.
        assert_eq!(out.len(), 3);
        let reg_names: Vec<&str> = out
            .iter()
            .filter_map(|it| match it {
                ModuleItem::Reg { name, .. } => Some(name.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            reg_names,
            vec!["__sync_slow_a_stage0", "__sync_slow_b_stage0"]
        );

        let ModuleItem::On(on) = &out[2] else {
            panic!("expected the On block last")
        };
        // Each call contributes a stage0 assign + a rewritten user assign.
        assert_eq!(on.body.len(), 4);
        let SeqStmt::Assign { lhs, .. } = &on.body[1] else {
            panic!("expected slow_a's rewritten assign second")
        };
        assert_eq!(lhs.base.name, "slow_a");
        let SeqStmt::Assign { lhs, .. } = &on.body[3] else {
            panic!("expected slow_b's rewritten assign fourth")
        };
        assert_eq!(lhs.base.name, "slow_b");
    }

    #[test]
    fn pulse_call_lowers_to_four_hidden_regs_two_on_blocks_and_a_rewritten_wire() {
        let sp = Span::new(0, 0);
        let items = vec![ModuleItem::Wire {
            name: ident("dst_pulse", sp),
            ty: Type::Bit,
            init: call_expr(
                Builtin::SyncPulse,
                vec![
                    ident_expr("src_pulse", sp),
                    ident_expr("clk_src", sp),
                    ident_expr("clk_dst", sp),
                ],
                sp,
            ),
        }];
        let out = expand_sync_prims(&items);

        // 1 toggle reg + 1 src on-block + 3 stage regs + 1 dst on-block + 1
        // rewritten wire = 7 items.
        assert_eq!(out.len(), 7);
        let reg_names: Vec<&str> = out
            .iter()
            .filter_map(|it| match it {
                ModuleItem::Reg { name, .. } => Some(name.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(reg_names.len(), 4);
        assert!(reg_names.iter().any(|n| n.ends_with("_toggle")));
        assert!(reg_names.iter().any(|n| n.ends_with("_stage0")));
        assert!(reg_names.iter().any(|n| n.ends_with("_stage1")));
        assert!(reg_names.iter().any(|n| n.ends_with("_stage2")));

        let on_blocks: Vec<&OnBlock> = out
            .iter()
            .filter_map(|it| match it {
                ModuleItem::On(on) => Some(on),
                _ => None,
            })
            .collect();
        assert_eq!(on_blocks.len(), 2);
        assert!(on_blocks.iter().any(|on| on.clock.name == "clk_src"));
        assert!(on_blocks.iter().any(|on| on.clock.name == "clk_dst"));

        let ModuleItem::Wire { init, .. } = out.last().expect("last item") else {
            panic!("expected the rewritten wire last")
        };
        let ExprKind::Binary { op, .. } = &init.kind else {
            panic!(
                "expected the rewritten wire's init to be a Binary XOR, got {:?}",
                init.kind
            )
        };
        assert_eq!(*op, BinOp::BitXor);
    }

    #[test]
    fn two_sync_prim_calls_in_one_module_get_distinct_hidden_names() {
        let sp = Span::new(0, 0);
        let items = vec![
            ModuleItem::On(OnBlock {
                clock: ident("clk_dst", sp),
                edge: Edge::Rise,
                body: vec![SeqStmt::Assign {
                    lhs: LValue {
                        base: ident("slow_a", sp),
                        index: None,
                        span: sp,
                    },
                    rhs: call_expr(
                        Builtin::SyncDoubleFlop,
                        vec![
                            ident_expr("fast_a", sp),
                            ident_expr("clk_src", sp),
                            ident_expr("clk_dst", sp),
                        ],
                        sp,
                    ),
                }],
                span: sp,
            }),
            ModuleItem::On(OnBlock {
                clock: ident("clk_dst", sp),
                edge: Edge::Rise,
                body: vec![SeqStmt::Assign {
                    lhs: LValue {
                        base: ident("slow_b", sp),
                        index: None,
                        span: sp,
                    },
                    rhs: call_expr(
                        Builtin::SyncDoubleFlop,
                        vec![
                            ident_expr("fast_b", sp),
                            ident_expr("clk_src", sp),
                            ident_expr("clk_dst", sp),
                        ],
                        sp,
                    ),
                }],
                span: sp,
            }),
        ];
        let out = expand_sync_prims(&items);
        let reg_names: Vec<&str> = out
            .iter()
            .filter_map(|it| match it {
                ModuleItem::Reg { name, .. } => Some(name.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(reg_names.len(), 2);
        assert_ne!(
            reg_names[0], reg_names[1],
            "each call site needs a distinct hidden name"
        );
    }
}
