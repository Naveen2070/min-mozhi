use super::adder_design;
use crate::ast::{Expr, ExprKind};
use crate::elaborate::{Design, Signal};
use crate::ir::{CellKind, lower};
use crate::span::Span;
use std::collections::BTreeMap;

#[test]
fn lowers_wire_add_of_two_inputs_to_an_add_cell() {
    let design = adder_design();
    let module = lower(&design);
    let add_cells: Vec<_> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Add)
        .collect();
    assert_eq!(add_cells.len(), 1);
    let add = add_cells[0];
    assert_eq!(add.pins["a"].width(), 8);
    assert_eq!(add.pins["b"].width(), 8);
    assert_eq!(add.pins["out"].width(), 9); // lossless add: N+1
}

#[test]
fn lowers_bool_literal_to_a_const_cell() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "flag".to_string(),
        Expr {
            kind: ExprKind::Bool(true),
            span: Span::default(),
        },
    );
    let design = Design {
        module: "flagger".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![],
        outputs: vec![],
        wires: vec![Signal {
            name: "flag".into(),
            width: super::w(1),
        }],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    };
    let module = lower(&design);
    let const_cells: Vec<_> = module
        .cells
        .iter()
        .filter(|c| matches!(c.kind, CellKind::Const { .. }))
        .collect();
    assert_eq!(const_cells.len(), 1);
    assert_eq!(const_cells[0].pins["out"].width(), 1);
    for cell in &module.cells {
        if !matches!(cell.kind, CellKind::Const { .. }) {
            assert!(
                !cell.pins.is_empty(),
                "non-Const cell {:?} has no pins",
                cell.kind
            );
        }
    }
}

#[test]
fn shl_grows_the_output_to_the_worst_case_width_not_the_input_width() {
    use crate::ast::BinOp;
    use crate::elaborate::{Design, Signal};
    use crate::ir::lower;
    use std::collections::BTreeMap;

    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        crate::ast::Expr {
            kind: crate::ast::ExprKind::Binary {
                op: BinOp::Shl,
                lhs: Box::new(super::ident("a")),
                rhs: Box::new(super::ident("b")),
            },
            span: crate::span::Span::default(),
        },
    );
    // `a: bits[2]`, `b: bits[2]` (shift amount 0..=3) -> worst-case growth is
    // `2^2 - 1 = 3`, so `y` must be `2 + 3 = 5` bits — NOT `2` bits (today's
    // bug: `lower_binop` sizes `out` at `a.width()` alone).
    let design = Design {
        module: "shl_mod".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "a".into(),
                width: super::w(2),
            },
            Signal {
                name: "b".into(),
                width: super::w(2),
            },
        ],
        outputs: vec![Signal {
            name: "y".into(),
            width: super::w(5),
        }],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    };
    let module = lower(&design);
    let (_, y_bits, _) = module.ports.iter().find(|(n, ..)| n == "y").unwrap();
    assert_eq!(y_bits.width(), 5);
    assert_eq!(crate::ir::validate::validate(&module), Vec::new());
}

#[test]
fn shl_with_a_compile_time_constant_amount_sizes_exactly_not_worst_case() {
    use crate::ast::BinOp;
    use crate::elaborate::{Design, Signal};
    use crate::ir::lower;
    use std::collections::BTreeMap;

    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        crate::ast::Expr {
            kind: crate::ast::ExprKind::Binary {
                op: BinOp::Shl,
                lhs: Box::new(super::ident("a")),
                rhs: Box::new(crate::ast::Expr {
                    kind: crate::ast::ExprKind::Int {
                        value: crate::bits::Bits::Small(2),
                        raw: "2".to_string(),
                    },
                    span: crate::span::Span::default(),
                }),
            },
            span: crate::span::Span::default(),
        },
    );
    // `a: bits[2] << 2` (a compile-time-constant amount) must size `y` at
    // the CHECKER's exact `a.width() + 2 = 4` bits, not the worst-case
    // `2 + (2^2 - 1) = 5` bits a runtime amount of the same pin width
    // would need (see `shl_grows_the_output_to_the_worst_case_width_not_
    // the_input_width` above, which pins that runtime case unchanged).
    let design = Design {
        module: "shl_const_mod".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "a".into(),
            width: super::w(2),
        }],
        outputs: vec![Signal {
            name: "y".into(),
            width: super::w(4),
        }],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    };
    let module = lower(&design);
    let (_, y_bits, _) = module.ports.iter().find(|(n, ..)| n == "y").unwrap();
    assert_eq!(y_bits.width(), 4);
    assert_eq!(crate::ir::validate::validate(&module), Vec::new());
}

/// GAP-1 residual Task 4: does Task 1's exact-when-constant `Shl` sizing
/// already make `ir::lower` + `ir::exec` agree with the AST kernel's fused
/// `value::binary::eval_shift_chain` on a multi-step shift chain, as a side
/// effect — without `ir::lower` ever fusing the chain into one width
/// computation the way `eval_shift_chain` does? Empirically: yes. Both sides
/// fold the SAME `width_rules::shift_result` step-by-step (the kernel folds
/// explicitly across the whole chain; `ir::lower` folds implicitly, because
/// each cell's `a.width()` IS the previous cell's already-exact output
/// width), so the running width and the running unsigned value march in
/// lockstep at every step. See `docs/audit/gaps.md`'s GAP-1 "fused shift
/// chains" sub-gap (now RESOLVED) and `value::binary::eval_shift_chain`'s
/// doc comment, which points back at this test.
#[test]
fn shift_chains_lowered_per_node_match_the_ast_kernels_fused_evaluation() {
    use crate::ast::BinOp;
    use crate::elaborate::{Design, Signal};
    use crate::ir::exec::Executor;
    use crate::ir::lower;
    use crate::value::{Resolver, Val, eval as kernel_eval};
    use std::collections::BTreeMap;

    fn lit(n: u128) -> Expr {
        Expr {
            kind: ExprKind::Int {
                value: crate::bits::Bits::Small(n),
                raw: n.to_string(),
            },
            span: Span::default(),
        }
    }
    fn shl(lhs: Expr, amount: u128) -> Expr {
        Expr {
            kind: ExprKind::Binary {
                op: BinOp::Shl,
                lhs: Box::new(lhs),
                rhs: Box::new(lit(amount)),
            },
            span: Span::default(),
        }
    }
    fn shr(lhs: Expr, amount: u128) -> Expr {
        Expr {
            kind: ExprKind::Binary {
                op: BinOp::Shr,
                lhs: Box::new(lhs),
                rhs: Box::new(lit(amount)),
            },
            span: Span::default(),
        }
    }

    struct FixedResolver {
        p2: Val,
    }
    impl Resolver for FixedResolver {
        fn signal(&mut self, name: &str) -> Result<Val, String> {
            match name {
                "p2" => Ok(self.p2.clone()),
                other => Err(format!("unknown signal `{other}` in this fixture")),
            }
        }
        fn ints(&self) -> &BTreeMap<String, i128> {
            static EMPTY: std::sync::OnceLock<BTreeMap<String, i128>> = std::sync::OnceLock::new();
            EMPTY.get_or_init(Default::default)
        }
    }

    // `p2` is unsigned `bits[8]` in every shape below; exhaustive over its
    // 256-value domain rather than a sampled edge table, since the whole
    // domain is cheap to walk.
    fn check(expr: Expr, y_width: u32) {
        let mut comb = BTreeMap::new();
        comb.insert("y".to_string(), expr.clone());
        let design = Design {
            module: "shift_chain_mod".to_string(),
            consts: BTreeMap::new(),
            inputs: vec![Signal {
                name: "p2".into(),
                width: super::w(8),
            }],
            outputs: vec![Signal {
                name: "y".into(),
                width: super::w(y_width),
            }],
            wires: vec![],
            regs: vec![],
            mems: vec![],
            comb,
            procs: vec![],
            clocks: vec![],
            resets: vec![],
            funcs: Default::default(),
            unknown_signals: Default::default(),
            extern_instances: vec![],
            asserts: vec![],
            covers: vec![],
        };
        let module = lower(&design);
        assert_eq!(crate::ir::validate::validate(&module), Vec::new());

        for p2 in 0u128..=255 {
            let mut executor = Executor::new(&module);
            executor.set_input("p2", Val::new(p2, 8, false));
            executor.tick();
            let ir_out = executor.get_output("y");

            let mut resolver = FixedResolver {
                p2: Val::new(p2, 8, false),
            };
            let kernel_out = kernel_eval(&mut resolver, &expr)
                .unwrap_or_else(|e| panic!("AST kernel eval failed for p2={p2}: {e:?}"));

            assert_eq!(
                ir_out, kernel_out,
                "ir::lower + ir::exec diverged from the AST kernel at p2={p2}"
            );
        }
    }

    // BUG-34's exact repro shape: `(p2 >> 4) << 7`. `Shr` never grows (out
    // stays 8 bits); `Shl` by a constant `7` sizes exactly (`8 + 7 = 15`).
    check(shl(shr(super::ident("p2"), 4), 7), 15);

    // Mirror: `(p2 << 3) >> 1`. `Shl` by constant `3` sizes exactly
    // (`8 + 3 = 11`); the trailing `Shr` never grows (stays 11 bits).
    check(shr(shl(super::ident("p2"), 3), 1), 11);

    // Three-step chain: `((p2 << 2) >> 3) << 4`. `8 + 2 = 10`, unchanged by
    // `>> 3` (10), then `10 + 4 = 14`.
    check(shl(shr(shl(super::ident("p2"), 2), 3), 4), 14);
}

#[test]
fn shl_result_feeding_a_matched_width_cell_validates_cleanly_when_amount_is_constant() {
    // GAP-1 residual repro: `(a << 2) & c` used to fail `ir::validate` with
    // a `WidthMismatch` because `ir::lower` always sized `Shl`'s `out` at
    // worst-case growth while the checker (and, post-fix, `ir::lower` too)
    // size it exactly for a compile-time-constant shift amount.
    use crate::ast::BinOp;
    use crate::elaborate::{Design, Signal};
    use crate::ir::lower;
    use std::collections::BTreeMap;

    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        crate::ast::Expr {
            kind: crate::ast::ExprKind::Binary {
                op: BinOp::BitAnd,
                lhs: Box::new(crate::ast::Expr {
                    kind: crate::ast::ExprKind::Binary {
                        op: BinOp::Shl,
                        lhs: Box::new(super::ident("a")),
                        rhs: Box::new(crate::ast::Expr {
                            kind: crate::ast::ExprKind::Int {
                                value: crate::bits::Bits::Small(2),
                                raw: "2".to_string(),
                            },
                            span: crate::span::Span::default(),
                        }),
                    },
                    span: crate::span::Span::default(),
                }),
                rhs: Box::new(super::ident("c")),
            },
            span: crate::span::Span::default(),
        },
    );
    let design = Design {
        module: "shl_and_mod".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "a".into(),
                width: super::w(2),
            },
            Signal {
                name: "c".into(),
                width: super::w(4),
            },
        ],
        outputs: vec![Signal {
            name: "y".into(),
            width: super::w(4),
        }],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    };
    let module = lower(&design);
    assert_eq!(crate::ir::validate::validate(&module), Vec::new());
}
