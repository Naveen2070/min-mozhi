use super::{ident, w};
use crate::ast::{Edge, Expr, ExprKind, Ident, LValue, SeqStmt};
use crate::checker::consteval::ConstVal;
use crate::elaborate::{Design, Process, Reg, Signal};
use crate::ir::{Cell, CellKind, Module, lower};
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

fn lvalue_index(name: &str, first: Expr, second: Option<Expr>) -> LValue {
    LValue {
        base: Ident {
            name: name.to_string(),
            span: Span::default(),
        },
        index: Some((first, second)),
        span: Span::default(),
    }
}

fn find_port<'a>(module: &'a Module, name: &str) -> &'a crate::ir::Bits {
    &module
        .ports
        .iter()
        .find(|(n, ..)| n == name)
        .expect("port")
        .1
}

fn find_dff(module: &Module) -> &Cell {
    let dffs: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| matches!(c.kind, CellKind::Dff { .. }))
        .collect();
    assert_eq!(dffs.len(), 1, "expected exactly one Dff cell");
    dffs[0]
}

/// One 8-bit register `q`, driven by `on rise(clk) { <one assign, given by caller> }`.
/// `d`/`idx` are declared as inputs so callers can build a bit-select or
/// slice write against them without repeating the whole Design each time.
fn design_with_assign(
    assign_rhs_name: &str,
    assign_lhs: LValue,
    extra_inputs: Vec<Signal>,
) -> Design {
    let mut inputs = vec![Signal {
        name: "clk".into(),
        width: w(1),
    }];
    inputs.extend(extra_inputs);
    Design {
        module: "bitsel".to_string(),
        consts: BTreeMap::new(),
        inputs,
        outputs: vec![],
        wires: vec![],
        regs: vec![Reg {
            name: "q".into(),
            width: w(8),
            reset: ConstVal {
                bits: crate::bits::Bits::Small(0),
                width: 8,
                signed: false,
            },
            clock: "clk".into(),
            edge: Edge::Rise,
        }],
        mems: vec![],
        comb: BTreeMap::new(),
        procs: vec![Process {
            clock: "clk".into(),
            edge: Edge::Rise,
            body: vec![SeqStmt::Assign {
                lhs: assign_lhs,
                rhs: ident(assign_rhs_name),
            }],
        }],
        clocks: vec!["clk".into()],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    }
}

/// `q[3] <- d` with a CONSTANT index: must be pure re-pointing (no new
/// cell beyond the Dff), with bit 3 of q's next value tracing exactly to
/// input `d`, and every other bit left untouched by that re-point.
#[test]
fn lowers_constant_bit_select_write_as_a_masked_merge() {
    let design = design_with_assign(
        "d",
        lvalue_index("q", int_lit(3), None),
        vec![Signal {
            name: "d".into(),
            width: w(1),
        }],
    );
    let module = lower(&design);
    let dff = find_dff(&module);
    assert_eq!(
        module.cells.len(),
        1,
        "a constant-index bit-select write must be pure re-pointing: only the Dff cell, no merge cell"
    );
    let d_bits = find_port(&module, "d").clone();
    assert_eq!(
        dff.pins["d"].nets[3], d_bits.nets[0],
        "bit 3 of q's next value traces directly to input d"
    );
    assert_ne!(
        dff.pins["d"].nets[0], d_bits.nets[0],
        "bit 0 must NOT be overwritten by d"
    );
}

/// `q[idx] <- d` with a RUNTIME index: one Eq + one merge Mux per bit
/// position (8 of each for an 8-bit q), no bit-level indexing primitive
/// beyond cells this codebase already has.
#[test]
fn lowers_runtime_bit_select_write_via_eq_mux_chain() {
    let design = design_with_assign(
        "d",
        lvalue_index("q", ident("idx"), None),
        vec![
            Signal {
                name: "d".into(),
                width: w(1),
            },
            Signal {
                name: "idx".into(),
                width: w(3),
            },
        ],
    );
    let module = lower(&design);
    let eq_count = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Eq)
        .count();
    let mux_count = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Mux)
        .count();
    assert_eq!(
        eq_count, 8,
        "one Eq per bit position for a runtime bit-select write"
    );
    assert_eq!(mux_count, 8, "one merge Mux per bit position");
}

/// `q[7:4] <- v` — a slice write. Bounds always const-fold (checker-enforced,
/// same guarantee a plain Slice read relies on), so this is pure re-pointing
/// too: no new cell, and the top 4 bits of q's next value trace exactly to
/// the 4-bit `v` input, with the bottom 4 bits untouched.
#[test]
fn lowers_constant_slice_write_as_a_masked_merge() {
    let design = design_with_assign(
        "v",
        lvalue_index("q", int_lit(7), Some(int_lit(4))),
        vec![Signal {
            name: "v".into(),
            width: w(4),
        }],
    );
    let module = lower(&design);
    let dff = find_dff(&module);
    assert_eq!(
        module.cells.len(),
        1,
        "a constant-bounds slice write must be pure re-pointing: only the Dff cell, no merge cell"
    );
    let v_bits = find_port(&module, "v").clone();
    for i in 0..4 {
        assert_eq!(
            dff.pins["d"].nets[4 + i],
            v_bits.nets[i],
            "bit {} of q's next value (in range [7:4]) traces to v's bit {}",
            4 + i,
            i
        );
    }
}
