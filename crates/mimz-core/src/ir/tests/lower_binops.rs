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
