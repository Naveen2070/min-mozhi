use super::{ident, w};
use crate::ast::{Arm, Expr, ExprKind, Pattern};
use crate::elaborate::{Design, Signal};
use crate::ir::{Bits, Cell, CellKind, Module, lower};
use crate::span::Span;
use std::collections::BTreeMap;

fn int_lit(value: u128) -> Expr {
    Expr {
        kind: ExprKind::Int {
            value: crate::bits::Bits::Small(value),
            raw: value.to_string(),
        },
        span: Span::default(),
    }
}

fn int_pattern(value: u128) -> Pattern {
    Pattern::Int {
        value: crate::bits::Bits::Small(value),
        raw: value.to_string(),
    }
}

fn find_port<'a>(module: &'a Module, name: &str) -> &'a Bits {
    &module
        .ports
        .iter()
        .find(|(n, ..)| n == name)
        .expect("port")
        .1
}

#[test]
fn lowers_if_expr_to_a_mux_cell() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "out".to_string(),
        Expr {
            kind: ExprKind::IfExpr {
                cond: Box::new(ident("sel")),
                then: Box::new(ident("a")),
                els: Box::new(ident("b")),
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "muxer".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "sel".into(),
                width: w(1),
            },
            Signal {
                name: "a".into(),
                width: w(8),
            },
            Signal {
                name: "b".into(),
                width: w(8),
            },
        ],
        outputs: vec![Signal {
            name: "out".into(),
            width: w(8),
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
        asserts: vec![],
        covers: vec![],
    };
    let module = lower(&design);

    let mux_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Mux)
        .collect();
    assert_eq!(mux_cells.len(), 1);
    let mux = mux_cells[0];
    assert_eq!(mux.pins["sel"].width(), 1);
    assert_eq!(mux.pins["a"].width(), 8);
    assert_eq!(mux.pins["b"].width(), 8);
    assert_eq!(mux.pins["out"].width(), 8);
    assert_eq!(*find_port(&module, "out"), mux.pins["out"]);
}

#[test]
fn lowers_match_with_int_arms_and_wildcard_to_chained_mux_eq() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "out".to_string(),
        Expr {
            kind: ExprKind::Match {
                scrutinee: Box::new(ident("x")),
                arms: vec![
                    Arm {
                        patterns: vec![int_pattern(0)],
                        value: int_lit(10),
                    },
                    Arm {
                        patterns: vec![int_pattern(1)],
                        value: int_lit(20),
                    },
                    Arm {
                        patterns: vec![Pattern::Wildcard],
                        value: int_lit(30),
                    },
                ],
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "matcher".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "x".into(),
            width: w(8),
        }],
        outputs: vec![Signal {
            name: "out".into(),
            width: w(8),
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
        asserts: vec![],
        covers: vec![],
    };
    let module = lower(&design);

    let eq_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Eq)
        .collect();
    assert_eq!(
        eq_cells.len(),
        2,
        "one Eq per literal arm, none for the wildcard"
    );

    let mux_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Mux)
        .collect();
    assert_eq!(mux_cells.len(), 2, "one Mux per non-last arm");

    // The design's `out` output is driven by the outermost (first-arm) mux.
    let out_bits = find_port(&module, "out");
    let outer_mux = mux_cells
        .iter()
        .find(|c| c.pins["out"] == *out_bits)
        .expect("outermost mux drives `out`");
    let inner_mux = mux_cells
        .iter()
        .find(|c| c.pins["out"] != *out_bits)
        .expect("the other mux is the inner one");

    // Outer mux's `b` (no-match fallthrough) traces to the inner mux's `out`.
    assert_eq!(outer_mux.pins["b"], inner_mux.pins["out"]);

    // Inner mux's `b` traces to the wildcard arm's constant value: a Const
    // cell's `out`, not another Mux/Eq.
    let const_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| matches!(c.kind, CellKind::Const { .. }))
        .collect();
    let wildcard_const = const_cells
        .iter()
        .find(|c| c.pins["out"] == inner_mux.pins["b"])
        .expect("inner mux's `b` pin traces to a Const cell (the wildcard arm's value)");
    let CellKind::Const { value } = &wildcard_const.kind else {
        unreachable!()
    };
    assert_eq!(value.bits, crate::bits::Bits::Small(30));

    // Pin down WHICH arm landed on the outer mux — this is the part that
    // distinguishes correct fold direction (arm 0 outermost/checked-first)
    // from an inverted one (arm 1 outermost): the outer mux's `a` must be
    // arm 0's value (Const 10), and its `sel` must be the Eq comparing
    // against arm 0's pattern (Const 0), not arm 1's (Const 1).
    let outer_a_const = const_cells
        .iter()
        .find(|c| c.pins["out"] == outer_mux.pins["a"])
        .expect("outer mux's `a` pin traces to a Const cell (arm 0's value)");
    let CellKind::Const {
        value: outer_a_value,
    } = &outer_a_const.kind
    else {
        unreachable!()
    };
    assert_eq!(
        outer_a_value.bits,
        crate::bits::Bits::Small(10),
        "outer mux's `a` should be arm 0's value (10), not arm 1's (20) — \
         a reversed fold would put arm 1 outermost instead"
    );

    let sel_const = const_cells
        .iter()
        .find(|c| {
            eq_cells
                .iter()
                .any(|eq| eq.pins["b"] == c.pins["out"] && eq.pins["out"] == outer_mux.pins["sel"])
        })
        .expect("outer mux's `sel` traces through an Eq cell to a Const cell (arm 0's pattern)");
    let CellKind::Const { value: sel_value } = &sel_const.kind else {
        unreachable!()
    };
    assert_eq!(
        sel_value.bits,
        crate::bits::Bits::Small(0),
        "outer mux's `sel` should compare against arm 0's pattern (0), not arm 1's (1) — \
         a reversed fold would check arm 1's pattern first"
    );
}

#[test]
fn lowers_match_with_int_mask_pattern_to_and_then_eq() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "out".to_string(),
        Expr {
            kind: ExprKind::Match {
                scrutinee: Box::new(ident("x")),
                arms: vec![
                    Arm {
                        patterns: vec![Pattern::IntMask {
                            value: 0b1000_0000,
                            mask: 0b1000_0000,
                            width: 8,
                            raw: "1???????".to_string(),
                        }],
                        value: int_lit(1),
                    },
                    Arm {
                        patterns: vec![Pattern::Wildcard],
                        value: int_lit(0),
                    },
                ],
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "masker".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "x".into(),
            width: w(8),
        }],
        outputs: vec![Signal {
            name: "out".into(),
            width: w(1),
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
        asserts: vec![],
        covers: vec![],
    };
    let module = lower(&design);

    let and_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::And)
        .collect();
    assert_eq!(and_cells.len(), 1, "one And cell for the mask");
    let eq_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Eq)
        .collect();
    assert_eq!(eq_cells.len(), 1, "one Eq cell comparing the masked result");

    // The And cell's output feeds the Eq cell's `a` input.
    assert_eq!(and_cells[0].pins["out"], eq_cells[0].pins["a"]);
    assert_eq!(and_cells[0].pins["a"].width(), 8);
    assert_eq!(and_cells[0].pins["b"].width(), 8);
}
