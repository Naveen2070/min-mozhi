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
