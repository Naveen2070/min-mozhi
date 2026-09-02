use super::{ident, w};
use crate::ast::{Expr, ExprKind, UnOp};
use crate::elaborate::{Design, Signal};
use crate::ir::{CellKind, lower};
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

fn unary_design(op: UnOp) -> Design {
    let mut comb = BTreeMap::new();
    comb.insert(
        "out".to_string(),
        Expr {
            kind: ExprKind::Unary {
                op,
                expr: Box::new(ident("a")),
            },
            span: Span::default(),
        },
    );
    Design {
        module: "unary".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "a".into(),
            width: w(8),
        }],
        outputs: vec![],
        wires: vec![Signal {
            name: "out".into(),
            width: w(8),
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
    }
}

#[test]
fn lowers_bitnot_to_a_not_cell() {
    let design = unary_design(UnOp::BitNot);
    let module = lower(&design);
    let not_cells: Vec<_> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Not)
        .collect();
    assert_eq!(not_cells.len(), 1);
    assert_eq!(not_cells[0].pins["a"].width(), 8);
    assert_eq!(not_cells[0].pins["out"].width(), 8);
}

#[test]
fn lowers_redand_to_a_1bit_output() {
    let design = unary_design(UnOp::RedAnd);
    let module = lower(&design);
    let redand_cells: Vec<_> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::RedAnd)
        .collect();
    assert_eq!(redand_cells.len(), 1);
    assert_eq!(redand_cells[0].pins["a"].width(), 8);
    assert_eq!(redand_cells[0].pins["out"].width(), 1);
}

#[test]
fn lowers_concat_preserves_msb_first_source_order_as_lsb_first_bits() {
    // `cab` is declared as an output (not a plain wire) purely so its final
    // `Bits` show up in `module.ports`, where the test can inspect it —
    // `lower()` doesn't expose a public way to fetch a bare wire's `Bits`.
    let mut comb = BTreeMap::new();
    comb.insert(
        "cab".to_string(),
        Expr {
            kind: ExprKind::Concat(vec![ident("a"), ident("b")]),
            span: Span::default(),
        },
    );
    let design = Design {
        module: "concat".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "a".into(),
                width: w(4),
            },
            Signal {
                name: "b".into(),
                width: w(4),
            },
        ],
        outputs: vec![Signal {
            name: "cab".into(),
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
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    };
    let module = lower(&design);
    let cab_bits = &module
        .ports
        .iter()
        .find(|(name, ..)| name == "cab")
        .expect("cab port")
        .1;
    assert_eq!(cab_bits.width(), 8);
    for id in &cab_bits.0[0..4] {
        assert_eq!(module.nets[id.0 as usize].name.as_deref(), Some("b"));
    }
    for id in &cab_bits.0[4..8] {
        assert_eq!(module.nets[id.0 as usize].name.as_deref(), Some("a"));
    }
}

#[test]
fn lowers_slice_to_a_subrange() {
    // `lo_nibble` is declared as an output for the same reason as `cab`
    // above: it puts the lowered `Bits` where the test can read them back.
    let mut comb = BTreeMap::new();
    comb.insert(
        "lo_nibble".to_string(),
        Expr {
            kind: ExprKind::Slice {
                base: Box::new(ident("a")),
                hi: Box::new(int_lit(3)),
                lo: Box::new(int_lit(0)),
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "slicer".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "a".into(),
            width: w(8),
        }],
        outputs: vec![Signal {
            name: "lo_nibble".into(),
            width: w(4),
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
    let a_bits = &module
        .ports
        .iter()
        .find(|(name, ..)| name == "a")
        .expect("a port")
        .1;
    let lo_nibble_bits = &module
        .ports
        .iter()
        .find(|(name, ..)| name == "lo_nibble")
        .expect("lo_nibble port")
        .1;
    assert_eq!(lo_nibble_bits.width(), 4);
    assert_eq!(lo_nibble_bits.0, a_bits.0[0..4].to_vec());
}
