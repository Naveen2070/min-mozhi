use super::adder_design;
use crate::ast::{Expr, ExprKind};
use crate::elaborate::{Design, Signal};
use crate::ir::{CellKind, lower, validate};
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

/// GAP-1 residual Task 5: an ordering comparison on `signed` operands used to
/// lower to a `CellKind::Lt`/`Le` unit variant, and `ir::exec`'s `get_bits`
/// reconstructs every pin as UNSIGNED — so `signed(-1) < signed(1)` silently
/// answered FALSE (`-1`'s bit pattern `0xFF` being the largest unsigned 8-bit
/// value), disagreeing with both the AST kernel and `emit_verilog`'s
/// genuinely-signed `$signed(...)` render. The cell now carries the
/// signedness `lower` read off the source operands, and `exec` stamps it back
/// onto both `Val`s. `Eq`/`Ne` are deliberately untouched — equality compares
/// the same bit patterns either way.
#[test]
fn signed_ordering_comparisons_execute_with_the_right_sign() {
    use crate::ast::BinOp;
    use crate::elaborate::Width;
    use crate::ir::exec::Executor;

    /// `wire y = a OP b` over two 8-bit inputs of the given signedness.
    fn cmp_design(op: BinOp, signed: bool) -> Design {
        let width = Width { bits: 8, signed };
        let mut comb = BTreeMap::new();
        comb.insert(
            "y".to_string(),
            Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(super::ident("a")),
                    rhs: Box::new(super::ident("b")),
                },
                span: Span::default(),
            },
        );
        Design {
            module: "cmp".to_string(),
            consts: BTreeMap::new(),
            inputs: vec![
                Signal {
                    name: "a".into(),
                    width,
                },
                Signal {
                    name: "b".into(),
                    width,
                },
            ],
            outputs: vec![Signal {
                name: "y".into(),
                width: super::w(1),
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
        }
    }

    // `a = -1` (0xFF), `b = 1`. Signed: -1 < 1 and -1 <= 1, both TRUE.
    // Unsigned: 255 < 1 and 255 <= 1, both FALSE. Same bits, opposite answers
    // — which is exactly the bug this closes.
    for (op, expect_kind_is_lt) in [(BinOp::Lt, true), (BinOp::Le, false)] {
        for (signed, expected) in [(true, 1u128), (false, 0u128)] {
            let design = cmp_design(op, signed);
            let module = lower(&design);

            let kind = &module
                .cells
                .iter()
                .find(|c| matches!(c.kind, CellKind::Lt { .. } | CellKind::Le { .. }))
                .expect("the comparison lowered to a cell")
                .kind;
            let expected_kind = if expect_kind_is_lt {
                CellKind::Lt { signed }
            } else {
                CellKind::Le { signed }
            };
            assert_eq!(
                kind, &expected_kind,
                "the cell must record the operands' declared signedness"
            );

            let mut executor = Executor::new(&module);
            executor.set_input("a", crate::value::Val::new(0xFF, 8, false));
            executor.set_input("b", crate::value::Val::new(1, 8, false));
            executor.tick();
            assert_eq!(
                executor.get_output("y").bits,
                crate::bits::Bits::Small(expected),
                "{op:?} with signed={signed} on (-1, 1)"
            );
        }
    }
}

/// UPDATED by GAP-1 residual Task 3 (2026-09-08): `signed_x < 5` is now a
/// SIGNED cell, deliberately flipping this test's old assertions per its own
/// prior comment ("this test pins the boundary so that fix can flip this
/// assertion deliberately rather than by accident").
///
/// The checker types a bare literal as untyped `Ty::CtInt`, inheriting the
/// sized operand's type — so `5` there is conceptually `signed[8] 5`. Before
/// Task 3, `lower_expr`'s `Int` arm sized a literal at its own NATURAL width
/// regardless of context, so the `b` pin came out 3 bits against `a`'s 8, the
/// mismatched-width guard in `lower_binop` refused to mark the comparison
/// signed, and `x < 5` stayed an (incorrect, but at least not differently
/// wrong) unsigned cell. Task 3 added `lower_expr_sized`, which re-lowers a
/// literal-or-const-shaped operand at its sibling's width instead — so `5`
/// now lowers to an 8-bit constant, the widths match, and
/// `expr_is_definitely_signed` (which already recognized a plain signed
/// `Ident`) can trust the guard and mark the comparison signed, exactly
/// matching what the checker and `emit_verilog` both already meant by
/// `signed_x < 5`.
#[test]
fn a_literal_operand_is_sized_to_its_signed_siblings_width_and_the_comparison_is_signed() {
    use crate::ast::BinOp;
    use crate::elaborate::Width;
    use crate::ir::exec::Executor;

    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        Expr {
            kind: ExprKind::Binary {
                op: BinOp::Lt,
                lhs: Box::new(super::ident("a")),
                rhs: Box::new(Expr {
                    kind: ExprKind::Int {
                        value: crate::bits::Bits::Small(5),
                        raw: "5".to_string(),
                    },
                    span: Span::default(),
                }),
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "cmp_lit".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "a".into(),
            width: Width {
                bits: 8,
                signed: true,
            },
        }],
        outputs: vec![Signal {
            name: "y".into(),
            width: super::w(1),
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
    let cmp = module
        .cells
        .iter()
        .find(|c| matches!(c.kind, CellKind::Lt { .. }))
        .expect("the comparison lowered to a cell");
    assert_eq!(
        cmp.pins["a"].width(),
        cmp.pins["b"].width(),
        "the literal must be re-sized to its signed sibling's width, not kept at its own natural width"
    );
    assert_eq!(
        cmp.kind,
        CellKind::Lt { signed: true },
        "matched operand widths, with `a` declared signed, must produce a signed comparison"
    );

    // `1 < 5` — `5` now lowers to an 8-bit constant (not the 3-bit natural
    // width that would read as `-3` under a signed re-tag), so the signed
    // comparison still correctly answers true.
    let mut executor = Executor::new(&module);
    executor.set_input("a", crate::value::Val::new(1, 8, false));
    executor.tick();
    assert_eq!(executor.get_output("y").bits, crate::bits::Bits::Small(1));
}

/// Fix-round regression (reviewer C1): `-a < 200` for `a: signed[8]` must
/// stay an UNSIGNED cell.
///
/// The first cut of `expr_is_definitely_signed` recursed through any unary
/// op, so `-a` reported signed. But the checker GROWS negation (`Signed(n)`
/// -> `Signed(n + 1)`, `checker/widths/ops/mod.rs`) while `lower_expr`'s
/// `Neg` arm keeps `a.width()` — so the `a` pin is 8 bits against a checker
/// type of `Signed(9)`, the literal `200` lowers to its own 8-bit natural
/// width, and `lower_binop`'s equal-width guard was satisfied by two widths
/// that mean different things. Reading `200` as two's complement then makes
/// it `-56` and flips half the input domain. `expr_is_definitely_signed` now
/// only recognizes shapes whose lowered width IS a declared signal's width.
#[test]
fn a_negated_operand_keeps_the_comparison_unsigned() {
    use crate::ast::{BinOp, UnOp};
    use crate::elaborate::Width;
    use crate::ir::exec::Executor;

    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        Expr {
            kind: ExprKind::Binary {
                op: BinOp::Lt,
                lhs: Box::new(Expr {
                    kind: ExprKind::Unary {
                        op: UnOp::Neg,
                        expr: Box::new(super::ident("a")),
                    },
                    span: Span::default(),
                }),
                rhs: Box::new(Expr {
                    kind: ExprKind::Int {
                        value: crate::bits::Bits::Small(200),
                        raw: "200".to_string(),
                    },
                    span: Span::default(),
                }),
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "neg_cmp".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "a".into(),
            width: Width {
                bits: 8,
                signed: true,
            },
        }],
        outputs: vec![Signal {
            name: "y".into(),
            width: super::w(1),
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
    let cmp = module
        .cells
        .iter()
        .find(|c| matches!(c.kind, CellKind::Lt { .. }))
        .expect("the comparison lowered to a cell");
    assert_eq!(
        cmp.pins["a"].width(),
        cmp.pins["b"].width(),
        "the premise: the equal-width guard alone does NOT catch this — \
         `-a` lowers to 8 bits (the checker types it `Signed(9)`) and the \
         literal `200`'s natural width is also 8"
    );
    assert_eq!(
        cmp.kind,
        CellKind::Lt { signed: false },
        "a negated operand must not mark the comparison signed"
    );

    // Ground truth: `-a` is `Signed(9)`, so it spans -127..=128 and is ALWAYS
    // < 200 — every input should answer 1. A signed cell answers 0 for `a=0`
    // (it reads `200` as `-56`), which is the regression this pins.
    //
    // `a` in 1..=56 still answers 0 even now: that is the SEPARATE,
    // pre-existing `Neg` width divergence above (`lower` never grows the
    // negation), documented as its own open residual in `docs/audit/gaps.md`
    // and deliberately not fixed here. The inputs below avoid it.
    let mut executor = Executor::new(&module);
    for a in [0u128, 100, 200] {
        executor.set_input("a", crate::value::Val::new(a, 8, false));
        executor.tick();
        assert_eq!(
            executor.get_output("y").bits,
            crate::bits::Bits::Small(1),
            "-a < 200 must be true for a={a}"
        );
    }
}

/// GAP-1's headline shape: `signed(a) < signed(b)` over two signals DECLARED
/// unsigned. The cast is a free reinterpret (`lower`'s `SignedCast` arm
/// repoints at its argument's `Bits` and allocates nothing), so the pin stays
/// the declared 8-bit width and the equal-width guard is satisfied honestly.
/// Also pins the negative half of `expr_is_definitely_signed`'s `SignedCast`
/// arm: the cast only counts over a bare identifier, because that is the only
/// argument shape whose lowered width is provably a declared signal's width.
#[test]
fn a_signed_cast_over_an_identifier_makes_the_comparison_signed() {
    use crate::ast::{BinOp, Builtin};
    use crate::ir::exec::Executor;

    fn signed_cast(inner: Expr) -> Expr {
        Expr {
            kind: ExprKind::Call {
                func: Builtin::SignedCast,
                args: vec![inner],
            },
            span: Span::default(),
        }
    }

    // `y = signed(a) < signed(b)`; `z = signed(a +% b) < 200` — the second's
    // cast argument is NOT a bare identifier, so NEITHER side proves
    // signedness and it stays unsigned. (`a +% b` keeps its 8-bit width, so
    // the literal `200`'s own 8-bit natural width would otherwise satisfy the
    // equal-width guard — the same shape as the `-a < 200` hazard above.)
    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        Expr {
            kind: ExprKind::Binary {
                op: BinOp::Lt,
                lhs: Box::new(signed_cast(super::ident("a"))),
                rhs: Box::new(signed_cast(super::ident("b"))),
            },
            span: Span::default(),
        },
    );
    comb.insert(
        "z".to_string(),
        Expr {
            kind: ExprKind::Binary {
                op: BinOp::Lt,
                lhs: Box::new(signed_cast(Expr {
                    kind: ExprKind::Binary {
                        op: BinOp::AddWrap,
                        lhs: Box::new(super::ident("a")),
                        rhs: Box::new(super::ident("b")),
                    },
                    span: Span::default(),
                })),
                rhs: Box::new(Expr {
                    kind: ExprKind::Int {
                        value: crate::bits::Bits::Small(200),
                        raw: "200".to_string(),
                    },
                    span: Span::default(),
                }),
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "signed_cast_cmp".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "a".into(),
                width: super::w(8),
            },
            Signal {
                name: "b".into(),
                width: super::w(8),
            },
        ],
        outputs: vec![
            Signal {
                name: "y".into(),
                width: super::w(1),
            },
            Signal {
                name: "z".into(),
                width: super::w(1),
            },
        ],
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

    let kinds: Vec<bool> = module
        .cells
        .iter()
        .filter_map(|c| match c.kind {
            CellKind::Lt { signed } => Some(signed),
            _ => None,
        })
        .collect();
    assert_eq!(kinds.len(), 2);
    assert!(
        kinds.contains(&true),
        "signed(a) < signed(b) over bare identifiers must lower as SIGNED"
    );
    assert!(
        kinds.contains(&false),
        "signed(<non-identifier>) must stay unsigned — its lowered width is \
         not provably the checker's type width"
    );

    // a = 0xFF (-1), b = 0x01 (1): -1 < 1 is true under the signed reading,
    // 255 < 1 is false under the unsigned one.
    let mut executor = Executor::new(&module);
    executor.set_input("a", crate::value::Val::new(0xFF, 8, false));
    executor.set_input("b", crate::value::Val::new(1, 8, false));
    executor.tick();
    assert_eq!(executor.get_output("y").bits, crate::bits::Bits::Small(1));
}

#[test]
fn gt_lowers_to_a_gt_cell() {
    use crate::ast::BinOp;
    use crate::elaborate::Width;
    use crate::ir::exec::Executor;

    /// `wire y = a OP b` over two 8-bit inputs of the given signedness.
    fn cmp_design(op: BinOp, signed: bool) -> Design {
        let width = Width { bits: 8, signed };
        let mut comb = BTreeMap::new();
        comb.insert(
            "y".to_string(),
            Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(super::ident("a")),
                    rhs: Box::new(super::ident("b")),
                },
                span: Span::default(),
            },
        );
        Design {
            module: "cmp".to_string(),
            consts: BTreeMap::new(),
            inputs: vec![
                Signal {
                    name: "a".into(),
                    width,
                },
                Signal {
                    name: "b".into(),
                    width,
                },
            ],
            outputs: vec![Signal {
                name: "y".into(),
                width: super::w(1),
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
        }
    }

    // `a = -1` (0xFF), `b = 1`. Signed: -1 > 1 is FALSE. Unsigned: 255 > 1 is TRUE.
    for (signed, expected) in [(true, 0u128), (false, 1u128)] {
        let design = cmp_design(BinOp::Gt, signed);
        let module = lower(&design);

        let kind = &module
            .cells
            .iter()
            .find(|c| matches!(c.kind, CellKind::Gt { .. }))
            .expect("the comparison lowered to a Gt cell")
            .kind;
        let expected_kind = CellKind::Gt { signed };
        assert_eq!(
            kind, &expected_kind,
            "the cell must record the operands' declared signedness"
        );

        let mut executor = Executor::new(&module);
        executor.set_input("a", crate::value::Val::new(0xFF, 8, false));
        executor.set_input("b", crate::value::Val::new(1, 8, false));
        executor.tick();
        assert_eq!(
            executor.get_output("y").bits,
            crate::bits::Bits::Small(expected),
            "Gt with signed={signed} on (-1, 1)"
        );
    }
}

#[test]
fn ge_lowers_to_a_ge_cell() {
    use crate::ast::BinOp;
    use crate::elaborate::Width;
    use crate::ir::exec::Executor;

    /// `wire y = a OP b` over two 8-bit inputs of the given signedness.
    fn cmp_design(op: BinOp, signed: bool) -> Design {
        let width = Width { bits: 8, signed };
        let mut comb = BTreeMap::new();
        comb.insert(
            "y".to_string(),
            Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(super::ident("a")),
                    rhs: Box::new(super::ident("b")),
                },
                span: Span::default(),
            },
        );
        Design {
            module: "cmp".to_string(),
            consts: BTreeMap::new(),
            inputs: vec![
                Signal {
                    name: "a".into(),
                    width,
                },
                Signal {
                    name: "b".into(),
                    width,
                },
            ],
            outputs: vec![Signal {
                name: "y".into(),
                width: super::w(1),
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
        }
    }

    // `a = -1` (0xFF), `b = 1`. Signed: -1 >= 1 is FALSE. Unsigned: 255 >= 1 is TRUE.
    for (signed, expected) in [(true, 0u128), (false, 1u128)] {
        let design = cmp_design(BinOp::Ge, signed);
        let module = lower(&design);

        let kind = &module
            .cells
            .iter()
            .find(|c| matches!(c.kind, CellKind::Ge { .. }))
            .expect("the comparison lowered to a Ge cell")
            .kind;
        let expected_kind = CellKind::Ge { signed };
        assert_eq!(
            kind, &expected_kind,
            "the cell must record the operands' declared signedness"
        );

        let mut executor = Executor::new(&module);
        executor.set_input("a", crate::value::Val::new(0xFF, 8, false));
        executor.set_input("b", crate::value::Val::new(1, 8, false));
        executor.tick();
        assert_eq!(
            executor.get_output("y").bits,
            crate::bits::Bits::Small(expected),
            "Ge with signed={signed} on (-1, 1)"
        );
    }
}

#[test]
fn logic_and_lowers_to_a_logic_and_cell() {
    use crate::ast::BinOp;

    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        Expr {
            kind: ExprKind::Binary {
                op: BinOp::LogicAnd,
                lhs: Box::new(super::ident("a")),
                rhs: Box::new(super::ident("b")),
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "logic_and_mod".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "a".into(),
                width: super::w(8),
            },
            Signal {
                name: "b".into(),
                width: super::w(8),
            },
        ],
        outputs: vec![Signal {
            name: "y".into(),
            width: super::w(1),
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
    let logic_and_cells: Vec<_> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::LogicAnd)
        .collect();
    assert_eq!(logic_and_cells.len(), 1);
    assert_eq!(logic_and_cells[0].pins["out"].width(), 1);
}

#[test]
fn logic_or_lowers_to_a_logic_or_cell() {
    use crate::ast::BinOp;

    let mut comb = BTreeMap::new();
    comb.insert(
        "y".to_string(),
        Expr {
            kind: ExprKind::Binary {
                op: BinOp::LogicOr,
                lhs: Box::new(super::ident("a")),
                rhs: Box::new(super::ident("b")),
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "logic_or_mod".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "a".into(),
                width: super::w(8),
            },
            Signal {
                name: "b".into(),
                width: super::w(8),
            },
        ],
        outputs: vec![Signal {
            name: "y".into(),
            width: super::w(1),
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
    let logic_or_cells: Vec<_> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::LogicOr)
        .collect();
    assert_eq!(logic_or_cells.len(), 1);
    assert_eq!(logic_or_cells[0].pins["out"].width(), 1);
}

/// GAP-1 residual Task 3 regression: `debug_wrapper.mimz`'s exact shape — a
/// bare literal driving an output DIRECTLY (no binop in between), where the
/// output's declared width is wider than the literal's own natural width.
/// `debug_wrapper.mimz` reaches this through a `const if` branch (`dbg_out =
/// 0` inside one arm, folded away by elaboration before a `Design` exists —
/// see `docs/audit/gaps.md`'s new sub-gap entry), but the shape `resolve()`
/// actually has to fix is simpler than that: a `design.comb` entry that IS,
/// itself, a bare `ExprKind::Int`. Before this task, `resolve()` called
/// plain `lower_expr`, sizing `0` at its own natural width (1 bit) instead
/// of `dbg_out`'s declared 8 bits — `ir::validate::validate` reported
/// `PortWidthMismatch { port: "dbg_out", declared: 8, found: 1 }`.
#[test]
fn debug_wrapper_shaped_bare_literal_in_a_wire_driver_sizes_to_the_declared_output_width() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "dbg_out".to_string(),
        Expr {
            kind: ExprKind::Int {
                value: crate::bits::Bits::Small(0),
                raw: "0".to_string(),
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "debug_wrapper_shaped".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![],
        outputs: vec![Signal {
            name: "dbg_out".into(),
            width: super::w(8),
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
    let (_, out_bits, _) = module
        .ports
        .iter()
        .find(|(name, _, _)| name == "dbg_out")
        .expect("dbg_out port exists");
    assert_eq!(
        out_bits.width(),
        8,
        "the literal driver must be sized to the declared 8-bit port width"
    );
    assert_eq!(
        validate::validate(&module),
        Vec::new(),
        "must no longer report PortWidthMismatch {{ port: \"dbg_out\", declared: 8, found: 1 }}"
    );
}

/// GAP-1 residual Task 3 regression: `traffic_light.mimz`'s exact shape —
/// `red = state == State.Red`, an enum-typed 2-bit register compared
/// (`Eq`) against a bare enum-variant tag literal. `State`'s tag `0`
/// (`State.Red`, elaborated to a plain `ExprKind::Int` before a `Design`
/// exists — `Pattern::Variant` is rewritten before lowering, but a bare
/// enum-variant REFERENCE outside a pattern position is too) naturally
/// widths to 1 bit, narrower than `state`'s declared 2 bits. Before this
/// task, `ir::validate::validate` reported `WidthMismatch { pin: "b",
/// expected: 2, found: 1 }` on the `$eq` cell.
#[test]
fn traffic_light_shaped_enum_match_arms_size_their_tags_to_the_scrutinees_width() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "red".to_string(),
        Expr {
            kind: ExprKind::Binary {
                op: crate::ast::BinOp::Eq,
                lhs: Box::new(super::ident("state")),
                rhs: Box::new(Expr {
                    kind: ExprKind::Int {
                        value: crate::bits::Bits::Small(0),
                        raw: "0".to_string(),
                    },
                    span: Span::default(),
                }),
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "traffic_light_shaped".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "state".into(),
            width: super::w(2),
        }],
        outputs: vec![Signal {
            name: "red".into(),
            width: super::w(1),
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
    let eq_cell = module
        .cells
        .iter()
        .find(|c| c.kind == CellKind::Eq)
        .expect("the comparison lowered to an Eq cell");
    assert_eq!(
        eq_cell.pins["a"].width(),
        eq_cell.pins["b"].width(),
        "State.Red's tag must be sized to state's 2-bit width, not its own 1-bit natural width"
    );
    assert_eq!(
        validate::validate(&module),
        Vec::new(),
        "must no longer report WidthMismatch {{ pin: \"b\", expected: 2, found: 1 }}"
    );
}

/// GAP-1 residual Task 3 regression: `sync_loop_search.mimz`'s exact shape —
/// `ast::sync_loop_lower`'s desugared loop-done check, `cnt == hi - 1`,
/// where `hi - 1` is a genuine `BinOp::Sub` AST node over two literals (kept
/// unfolded by design, so the desugaring itself stays free of a `checker`
/// dependency), not a bare literal. This is the OPPOSITE direction from the
/// other two fixtures above: `lower_binop`'s ordinary lossless-Sub growth
/// formula (`in_width + 1`) sizes `8 - 1` to 5 bits regardless of both
/// operands being compile-time constants, WIDER than the 3-bit counter it's
/// compared against — so "re-lower whichever side is narrower" (the first
/// cut of this fix) picks the WRONG side here; only "re-lower whichever
/// side is const-foldable" (`is_const_foldable`) gets this right. Before
/// this task, `ir::validate::validate` reported `WidthMismatch { pin: "b",
/// expected: 3, found: 5 }` on the `$eq` cell.
#[test]
fn sync_loop_search_shaped_compile_time_subtraction_sizes_down_to_the_narrower_counter_width() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "done".to_string(),
        Expr {
            kind: ExprKind::Binary {
                op: crate::ast::BinOp::Eq,
                lhs: Box::new(super::ident("cnt")),
                rhs: Box::new(Expr {
                    kind: ExprKind::Binary {
                        op: crate::ast::BinOp::Sub,
                        lhs: Box::new(Expr {
                            kind: ExprKind::Int {
                                value: crate::bits::Bits::Small(8),
                                raw: "8".to_string(),
                            },
                            span: Span::default(),
                        }),
                        rhs: Box::new(Expr {
                            kind: ExprKind::Int {
                                value: crate::bits::Bits::Small(1),
                                raw: "1".to_string(),
                            },
                            span: Span::default(),
                        }),
                    },
                    span: Span::default(),
                }),
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "sync_loop_search_shaped".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "cnt".into(),
            width: super::w(3),
        }],
        outputs: vec![Signal {
            name: "done".into(),
            width: super::w(1),
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
    let eq_cell = module
        .cells
        .iter()
        .find(|c| c.kind == CellKind::Eq)
        .expect("the comparison lowered to an Eq cell");
    assert_eq!(
        eq_cell.pins["a"].width(),
        eq_cell.pins["b"].width(),
        "`8 - 1` must be sized down to cnt's 3-bit width, not its own 5-bit lossless-Sub-growth width"
    );
    assert_eq!(
        validate::validate(&module),
        Vec::new(),
        "must no longer report WidthMismatch {{ pin: \"b\", expected: 3, found: 5 }}"
    );
}

/// Review fix (code review, 2026-09-08): `lower_expr_sized`'s fallback arm
/// used to fold a compile-time-constant EXPRESSION via
/// `crate::value::const_eval`, whose `i128`-saturating narrowing does not
/// error past `i128`'s range — it silently returns `i128::MAX`/`MIN`
/// (`ConstVal::to_i128_saturating`'s own doc comment). `bits[200] w = 1 <<
/// 190;` is checker-legal (`Bits::Wide` exists exactly for a value like
/// this, BUG-13 layer 2), so the old fallback silently lowered a WRONG
/// constant with no error anywhere. Fixed by routing through
/// `crate::value::const_eval_wide` instead, which returns the checker's own
/// arbitrary-width `ConstVal` — exact regardless of magnitude.
#[test]
fn a_wide_compile_time_constant_expression_lowers_exactly_not_saturated_to_i128_max() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "big_const".to_string(),
        Expr {
            kind: ExprKind::Binary {
                op: crate::ast::BinOp::Shl,
                lhs: Box::new(Expr {
                    kind: ExprKind::Int {
                        value: crate::bits::Bits::Small(1),
                        raw: "1".to_string(),
                    },
                    span: Span::default(),
                }),
                rhs: Box::new(Expr {
                    kind: ExprKind::Int {
                        value: crate::bits::Bits::Small(190),
                        raw: "190".to_string(),
                    },
                    span: Span::default(),
                }),
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "wide_const_shaped".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![],
        outputs: vec![Signal {
            name: "big_const".into(),
            width: super::w(200),
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
    let const_cell = module
        .cells
        .iter()
        .find(|c| matches!(c.kind, CellKind::Const { .. }))
        .expect("`1 << 190` folds to one Const cell (resolve()'s declared-width path)");
    let CellKind::Const { value } = &const_cell.kind else {
        unreachable!("just matched CellKind::Const above")
    };
    assert_eq!(
        value.width, 200,
        "must be sized to the declared 200-bit port width"
    );
    // `1 << 190` has exactly one bit set, at position 190 — its minimal
    // (unsigned) representation is 191 bits, the direct, dependency-free
    // check that the VALUE (not just the width) survived intact.
    assert_eq!(
        crate::bits::natural_width(&value.bits),
        191,
        "the folded value must be exactly `1 << 190` (highest set bit at 190), not a corrupted \
         or saturated placeholder"
    );
    // The exact regression this pins: the old `i128`-saturating fallback
    // would have produced this specific wrong value.
    assert_ne!(
        value.bits,
        crate::bits::Bits::Small(i128::MAX as u128),
        "must not silently saturate to i128::MAX"
    );
    // Independent oracle: the checker's own const evaluator (which the
    // production code now also calls), resized the same sign-aware way
    // `ir::exec`'s `Const`-cell evaluation resizes a folded constant.
    let expr = &design.comb["big_const"];
    let folded = crate::value::const_eval_wide(expr, &design.consts).expect("1 << 190 const-folds");
    let expected = crate::value::from_const_at_width(&folded, 200, folded.signed).bits;
    assert_eq!(
        value.bits, expected,
        "must match the checker's own arbitrary-width fold, resized the same way execution would"
    );
    assert_eq!(validate::validate(&module), Vec::new());
}

/// Task 8: pins the empirical claim behind `lower_binop`'s
/// `BinOp::Coalesce => unreachable!(...)` arm — that `??` used at MODULE
/// level (wire declaration + assignment, in either of its two source forms)
/// never survives `elaborate_project` into a `Design`, so `ir::lower` never
/// sees a `BinOp::Coalesce` node to lower for that shape. Runs the real lex
/// -> parse -> check -> elaborate_project -> lower pipeline (no hand-built
/// `Design`, so a regression can't hide behind a synthetic fixture) over both
/// checker-fixture shapes from `checker::tests::bundles`
/// (`qq_unwrap_form_types_as_the_data_field_type`,
/// `qq_or_mux_form_types_as_still_optional`), asserting both that `lower()`
/// does not panic (the `unreachable!()` arm is genuinely never hit) and that
/// the elaborated `Design` itself contains no `Coalesce` anywhere (the
/// stronger, direct check — a `{design:?}` scan covers `comb`, `procs`,
/// `asserts`, and `covers` alike, not just the one field each fixture happens
/// to drive). Does NOT cover a bundle-typed `fn` parameter referenced bare
/// (not via `.field`) inside that fn's own body — see
/// `bare_bundle_typed_fn_param_coalesce_unwrap_is_eliminated` below for that
/// shape (fixed 2026-09-15).
#[test]
fn lower_coalesce_is_unreachable_for_both_source_forms() {
    fn elaborate_src(src: &str) -> Design {
        let file = crate::parser::parse(crate::lexer::lex(src).expect("lexes")).expect("parses");
        crate::checker::check(std::slice::from_ref(&file)).expect("checks clean");
        crate::elaborate::elaborate_project(std::slice::from_ref(&file), None, &BTreeMap::new())
            .expect("elaborates")
    }

    // Unwrap form: `raw ?? 0` (scalar result) — eliminated by `Rw::expr`'s
    // dedicated `Binary{Coalesce}` arm in `elaborate/rewrite.rs`.
    let unwrap_src = "module M {\n  in c: bit\n  in d: bits[8]\n  out o: bits[8]\n  \
                       wire x: bits[8]? = { valid: c, data: d }\n  \
                       o = x ?? 0\n}\n";
    let unwrap_design = elaborate_src(unwrap_src);
    assert!(
        !format!("{unwrap_design:?}").contains("Coalesce"),
        "unwrap form `x ?? 0` must not survive elaborate_project as a Coalesce node"
    );
    let _ = lower(&unwrap_design); // must not panic

    // OR-mux form: `x ?? y` (both sides and the result stay bundle-typed) —
    // eliminated earlier still, at bundle-typed signal-declaration time, by
    // `bundle_field_expr` in `elaborate/bundle.rs`.
    let or_mux_src = "module M {\n  in c1: bit\n  in d1: bits[8]\n  \
                       in c2: bit\n  in d2: bits[8]\n  out o: bit\n  \
                       wire x: bits[8]? = { valid: c1, data: d1 }\n  \
                       wire y: bits[8]? = { valid: c2, data: d2 }\n  \
                       wire merged: bits[8]? = x ?? y\n  o = merged.valid\n}\n";
    let or_mux_design = elaborate_src(or_mux_src);
    assert!(
        !format!("{or_mux_design:?}").contains("Coalesce"),
        "OR-mux form `x ?? y` must not survive elaborate_project as a Coalesce node"
    );
    let _ = lower(&or_mux_design); // must not panic
}

/// 2026-09-15 fix: a bundle-typed `fn` PARAMETER referenced BARE (not via
/// `.field`) inside the fn's own body, combined with the unwrap form of
/// `??`, used to panic in `ir::lower::resolve()` ("no driver recorded for
/// signal `h`") before `Coalesce` was ever reached — `flatten_bundle_refs_expr`
/// (`elaborate/bundle.rs`) only rewrote `param.field` reads, so a bare
/// `Ident("h")` inside `h ?? 0` was never flattened to `h_valid`/`h_data`
/// and a raw `Coalesce` node survived into `design.funcs`. Fixed by giving
/// `flatten_bundle_refs_expr` its own `Binary{Coalesce}` case mirroring
/// `Rw::expr`'s desugaring. Runs the real lex -> parse -> check ->
/// elaborate_project -> lower pipeline (same discipline as the sibling test
/// above), asserting `lower()` does not panic, the elaborated `Design`
/// contains no `Coalesce` node, and the lowered module validates cleanly.
/// The OR-mux form for a bundle-typed fn TAIL is a separate, still-open
/// gap (see `docs/audit/gaps.md`) — not exercised here.
#[test]
fn bare_bundle_typed_fn_param_coalesce_unwrap_is_eliminated() {
    let src = "bundle Handshake(W: int = 8) {\n  valid: bit\n  data: bits[W]\n}\n\
               fn get_or(h: Handshake(W: 8)) -> bits[8] {\n  h ?? 0\n}\n\
               module M {\n  in c: bit\n  in d: bits[8]\n  out y: bits[8]\n  \
               y = get_or({ valid: c, data: d })\n}\n";
    let file = crate::parser::parse(crate::lexer::lex(src).expect("lexes")).expect("parses");
    crate::checker::check(std::slice::from_ref(&file)).expect("checks clean");
    let design =
        crate::elaborate::elaborate_project(std::slice::from_ref(&file), None, &BTreeMap::new())
            .expect("elaborates");
    assert!(
        !format!("{design:?}").contains("Coalesce"),
        "bare bundle-param `h ?? 0` inside a fn body must not survive elaborate_project \
         as a Coalesce node"
    );
    let module = lower(&design); // must not panic
    assert_eq!(
        validate::validate(&module),
        Vec::new(),
        "lowered module must validate cleanly"
    );
}
