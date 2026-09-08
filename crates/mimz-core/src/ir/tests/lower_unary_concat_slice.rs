use super::{ident, w};
use crate::ast::{Expr, ExprKind, UnOp};
use crate::elaborate::{Design, Signal};
use crate::ir::{CellKind, lower};
use crate::span::Span;
use std::collections::BTreeMap;

/// Shared fixture for the `ExprKind::Index` (plain-vector bit-select) tests
/// below: one multi-bit input `a`, plus whatever `extra_inputs` the index
/// expression itself needs (e.g. a runtime index signal `i`), and one output
/// `bit` driven by `index_expr`.
fn index_design(index_expr: Expr, extra_inputs: Vec<Signal>) -> Design {
    let mut comb = BTreeMap::new();
    comb.insert(
        "bit".to_string(),
        Expr {
            kind: ExprKind::Index {
                base: Box::new(ident("a")),
                index: Box::new(index_expr),
            },
            span: Span::default(),
        },
    );
    let mut inputs = vec![Signal {
        name: "a".into(),
        width: w(8),
    }];
    inputs.extend(extra_inputs);
    Design {
        module: "indexer".to_string(),
        consts: BTreeMap::new(),
        inputs,
        outputs: vec![Signal {
            name: "bit".into(),
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
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    }
}

fn int_lit(value: u128) -> Expr {
    Expr {
        kind: ExprKind::Int {
            value: crate::bits::Bits::Small(value),
            raw: value.to_string(),
        },
        span: Span::default(),
    }
}

pub(super) fn unary_design(op: UnOp) -> Design {
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

#[test]
fn lowers_replicate_reuses_same_nets() {
    // `{3{a}}` for a 1-bit `a` produces a 3-bit `Bits` whose three nets are
    // all the SAME `NetId` (not freshly allocated, just reused).
    let mut comb = BTreeMap::new();
    comb.insert(
        "rep".to_string(),
        Expr {
            kind: ExprKind::Replicate {
                count: Box::new(int_lit(3)),
                parts: vec![ident("a")],
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "replicate_reuse".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "a".into(),
            width: w(1),
        }],
        outputs: vec![Signal {
            name: "rep".into(),
            width: w(3),
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
    let rep_bits = &module
        .ports
        .iter()
        .find(|(name, ..)| name == "rep")
        .expect("rep port")
        .1;
    assert_eq!(rep_bits.width(), 3);
    // All three nets should be the same NetId (reused from `a`), not three
    // different allocations.
    assert_eq!(rep_bits.0[0], rep_bits.0[1]);
    assert_eq!(rep_bits.0[1], rep_bits.0[2]);
}

#[test]
fn lowers_replicate_preserves_msb_first_ordering() {
    // `{2{a,b}}` for two 1-bit signals produces a 4-bit result with pattern
    // `[a,b,a,b]` in MSB-first order (source order is MSB, so the result's
    // index-0-is-LSB layout puts `a` at indices 0,2 and `b` at indices 1,3).
    let mut comb = BTreeMap::new();
    comb.insert(
        "rep".to_string(),
        Expr {
            kind: ExprKind::Replicate {
                count: Box::new(int_lit(2)),
                parts: vec![ident("a"), ident("b")],
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "replicate_order".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "a".into(),
                width: w(1),
            },
            Signal {
                name: "b".into(),
                width: w(1),
            },
        ],
        outputs: vec![Signal {
            name: "rep".into(),
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
    let rep_bits = &module
        .ports
        .iter()
        .find(|(name, ..)| name == "rep")
        .expect("rep port")
        .1;
    assert_eq!(rep_bits.width(), 4);
    // The loop processes parts.iter().rev() twice, so the order is:
    // iteration 1: b, a; iteration 2: b, a → final: [b,a,b,a] at indices 0-3.
    // In MSB-first/source order, this reads as {a,b,a,b}, but in LSB-first
    // indexing, 'b' (LSB of each pair) is at indices 0,2 and 'a' (MSB) at 1,3.
    assert_eq!(
        module.nets[rep_bits.0[0].0 as usize].name.as_deref(),
        Some("b")
    );
    assert_eq!(
        module.nets[rep_bits.0[1].0 as usize].name.as_deref(),
        Some("a")
    );
    assert_eq!(
        module.nets[rep_bits.0[2].0 as usize].name.as_deref(),
        Some("b")
    );
    assert_eq!(
        module.nets[rep_bits.0[3].0 as usize].name.as_deref(),
        Some("a")
    );
}

#[test]
fn lowers_constant_index_to_a_single_bit_repoint() {
    // `a[0]` for an 8-bit `a` produces a 1-bit `Bits` pointing at exactly
    // `a`'s bit-0 net — no cell, same "no cell: a sub-range of existing
    // nets" strategy as `ExprKind::Slice`.
    let design = index_design(int_lit(0), vec![]);
    let module = lower(&design);
    let a_bits = &module
        .ports
        .iter()
        .find(|(name, ..)| name == "a")
        .expect("a port")
        .1;
    let bit_bits = &module
        .ports
        .iter()
        .find(|(name, ..)| name == "bit")
        .expect("bit port")
        .1;
    assert_eq!(bit_bits.width(), 1);
    assert_eq!(bit_bits.0, vec![a_bits.0[0]]);
    // No cell of any kind was needed for a constant index.
    assert!(module.cells.is_empty());
}

#[test]
fn lowers_runtime_index_via_shr_and_a_zero_slice() {
    // `a[i]` with a runtime `i` composes `a >> i` (one `Shr` cell, output
    // width == `a`'s own width, unchanged by `i`) with bit 0 of that
    // shift's own result — net-index 0 of the `Shr` cell's `out` pins,
    // not a fresh cell of its own.
    let design = index_design(
        ident("i"),
        vec![Signal {
            name: "i".into(),
            width: w(3),
        }],
    );
    let module = lower(&design);
    let bit_bits = &module
        .ports
        .iter()
        .find(|(name, ..)| name == "bit")
        .expect("bit port")
        .1;
    assert_eq!(bit_bits.width(), 1);
    let shr_cells: Vec<_> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Shr)
        .collect();
    assert_eq!(shr_cells.len(), 1);
    let shr_out = &shr_cells[0].pins["out"];
    assert_eq!(shr_out.width(), 8, "Shr never grows past `a`'s own width");
    assert_eq!(bit_bits.0, vec![shr_out.0[0]]);
}
