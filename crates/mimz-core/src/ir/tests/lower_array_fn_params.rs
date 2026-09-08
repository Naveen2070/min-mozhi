//! GAP-1 residual Task 6: array-typed `fn` params (N-scalar flattening),
//! mirroring `fn_array_search.mimz`'s `pick(vals: bits[8][4], idx: bits[3])
//! -> bits[8] { vals[idx] }` shape — both the constant-index and
//! runtime-index element-access paths through `ExprKind::Index`.

use super::{ident, w};
use crate::ast::{Expr, ExprKind, FnParam, FuncDecl, Ident, Type};
use crate::elaborate::{Design, Signal};
use crate::ir::{Bits, Cell, CellKind, Module, lower};
use crate::span::Span;
use std::collections::{BTreeMap, HashMap};

fn id(name: &str) -> Ident {
    Ident {
        name: name.to_string(),
        span: Span::default(),
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

fn bits_ty(n: u128) -> Type {
    Type::Bits(Box::new(int_lit(n)))
}

fn array_ty(elem_width: u128, len: u128) -> Type {
    Type::Array {
        elem: Box::new(bits_ty(elem_width)),
        len: Box::new(int_lit(len)),
    }
}

fn fn_param(name: &str, ty: Type) -> FnParam {
    FnParam {
        name: id(name),
        ty,
        span: Span::default(),
    }
}

fn fn_call(name: &str, args: Vec<Expr>) -> Expr {
    Expr {
        kind: ExprKind::FnCall {
            name: id(name),
            args,
        },
        span: Span::default(),
    }
}

fn index_expr(base: Expr, index: Expr) -> Expr {
    Expr {
        kind: ExprKind::Index {
            base: Box::new(base),
            index: Box::new(index),
        },
        span: Span::default(),
    }
}

/// `abcd()` — the four-element array literal every test here calls its
/// array-param `fn` with, matching `fn_array_search.mimz`'s own
/// `find_index([a, b, c, d], target)` call-site shape.
fn abcd() -> Expr {
    Expr {
        kind: ExprKind::ArrayLit(vec![ident("a"), ident("b"), ident("c"), ident("d")]),
        span: Span::default(),
    }
}

fn design_with(func: FuncDecl, inputs: Vec<Signal>, call_expr: Expr) -> Design {
    let mut comb = BTreeMap::new();
    comb.insert("picked".to_string(), call_expr);
    let mut funcs = HashMap::new();
    funcs.insert(func.name.name.clone(), func);
    Design {
        module: "array_param_caller".to_string(),
        consts: BTreeMap::new(),
        inputs,
        outputs: vec![Signal {
            name: "picked".into(),
            width: w(8),
        }],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs,
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    }
}

fn abcd_inputs() -> Vec<Signal> {
    ["a", "b", "c", "d"]
        .into_iter()
        .map(|n| Signal {
            name: n.into(),
            width: w(8),
        })
        .collect()
}

fn find_port<'a>(module: &'a Module, name: &str) -> &'a Bits {
    &module
        .ports
        .iter()
        .find(|(n, ..)| n == name)
        .expect("port")
        .1
}

/// `fn third(vals: bits[8][4]) -> bits[8] { vals[2] }` called as
/// `third([a, b, c, d])` — a CONSTANT array index. Must trace straight to
/// `c`'s own `Bits` (the flattened `vals_2` local, element width, not a
/// single bit) with no cell at all, same "no cell: pure re-pointing"
/// strategy as the plain-vector constant case.
#[test]
fn lowers_constant_index_into_array_param_to_the_flattened_elements_bits() {
    let func = FuncDecl {
        name: id("third"),
        params: vec![fn_param("vals", array_ty(8, 4))],
        ret: bits_ty(8),
        stmts: vec![],
        tail: index_expr(ident("vals"), int_lit(2)),
        span: Span::default(),
    };
    let design = design_with(func, abcd_inputs(), fn_call("third", vec![abcd()]));
    let module = lower(&design);

    let c_bits = find_port(&module, "c").clone();
    let picked_bits = find_port(&module, "picked").clone();
    assert_eq!(picked_bits.width(), 8);
    assert_eq!(
        picked_bits, c_bits,
        "vals[2] must trace to c's own Bits (the 3rd flattened element)"
    );
    assert!(
        module.cells.is_empty(),
        "a constant array index needs no cell, exactly like a constant \
         plain-vector index"
    );
}

/// `fn pick(vals: bits[8][4], idx: bits[3]) -> bits[8] { vals[idx] }` called
/// as `pick([a, b, c, d], idx)` — a RUNTIME array index. Must lower to a
/// 3-deep `Eq`/`Mux` chain (`len - 1` pairs) over the four flattened
/// elements, checked in ascending index order with the LAST element as the
/// unconditional (out-of-range-clamping) default — mirrors
/// `fn_array_search.mimz`'s `pick` doc comment ("the emitter generates a
/// ternary-chain mux ... an out-of-range value falls through ... to the
/// last element").
#[test]
fn lowers_runtime_index_into_array_param_via_eq_mux_chain() {
    let func = FuncDecl {
        name: id("pick"),
        params: vec![
            fn_param("vals", array_ty(8, 4)),
            fn_param("idx", bits_ty(3)),
        ],
        ret: bits_ty(8),
        stmts: vec![],
        tail: index_expr(ident("vals"), ident("idx")),
        span: Span::default(),
    };
    let mut inputs = abcd_inputs();
    inputs.push(Signal {
        name: "idx".into(),
        width: w(3),
    });
    let design = design_with(func, inputs, fn_call("pick", vec![abcd(), ident("idx")]));
    let module = lower(&design);

    let idx_bits = find_port(&module, "idx").clone();
    let a_bits = find_port(&module, "a").clone();
    let b_bits = find_port(&module, "b").clone();
    let c_bits = find_port(&module, "c").clone();
    let d_bits = find_port(&module, "d").clone();
    let picked_bits = find_port(&module, "picked").clone();

    let eq_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Eq)
        .collect();
    assert_eq!(
        eq_cells.len(),
        3,
        "one Eq per non-default element (len - 1)"
    );
    for eq in &eq_cells {
        assert_eq!(
            eq.pins["a"], idx_bits,
            "every Eq compares the runtime index against a constant"
        );
    }

    let mux_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Mux)
        .collect();
    assert_eq!(
        mux_cells.len(),
        3,
        "one Mux per non-default element (len - 1)"
    );

    // The outermost Mux drives `picked` directly, and checks index 0 first
    // (a-vs-fallthrough), matching `find_index`'s documented "on a duplicate
    // match the LOWER index always wins" first-match priority.
    let outer_mux = mux_cells
        .iter()
        .find(|c| c.pins["out"] == picked_bits)
        .expect("outermost mux drives `picked`");
    assert_eq!(
        outer_mux.pins["a"], a_bits,
        "outer mux selects `a` (index 0)"
    );

    let const_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| matches!(c.kind, CellKind::Const { .. }))
        .collect();
    let outer_sel_const = const_cells
        .iter()
        .find(|cc| {
            eq_cells
                .iter()
                .any(|eq| eq.pins["b"] == cc.pins["out"] && eq.pins["out"] == outer_mux.pins["sel"])
        })
        .expect("outer mux's `sel` traces through an Eq to a Const (index 0)");
    let CellKind::Const {
        value: outer_sel_value,
    } = &outer_sel_const.kind
    else {
        unreachable!()
    };
    assert_eq!(outer_sel_value.bits, crate::bits::Bits::Small(0));

    // Walk inward: outer's `b` (fallthrough) is the next Mux, which selects
    // `b` (index 1) and compares against constant 1.
    let mid_mux = mux_cells
        .iter()
        .find(|c| c.pins["out"] == outer_mux.pins["b"])
        .expect("outer mux's fallthrough is the next Mux in the chain");
    assert_eq!(mid_mux.pins["a"], b_bits, "mid mux selects `b` (index 1)");
    let mid_sel_const = const_cells
        .iter()
        .find(|cc| {
            eq_cells
                .iter()
                .any(|eq| eq.pins["b"] == cc.pins["out"] && eq.pins["out"] == mid_mux.pins["sel"])
        })
        .expect("mid mux's `sel` traces through an Eq to a Const (index 1)");
    let CellKind::Const {
        value: mid_sel_value,
    } = &mid_sel_const.kind
    else {
        unreachable!()
    };
    assert_eq!(mid_sel_value.bits, crate::bits::Bits::Small(1));

    // Innermost Mux selects `c` (index 2) and compares against constant 2;
    // its own `b` (the unconditional default, no further Eq) is `d` itself
    // — index 3 is never compared against, matching the "clamp to last
    // element" out-of-range behaviour.
    let inner_mux = mux_cells
        .iter()
        .find(|c| c.pins["out"] == mid_mux.pins["b"])
        .expect("mid mux's fallthrough is the innermost Mux");
    assert_eq!(
        inner_mux.pins["a"], c_bits,
        "inner mux selects `c` (index 2)"
    );
    assert_eq!(
        inner_mux.pins["b"], d_bits,
        "inner mux's unconditional default is `d` (index 3, the last element) — \
         out-of-range indices clamp here, no further Eq/Mux needed"
    );
    let inner_sel_const = const_cells
        .iter()
        .find(|cc| {
            eq_cells
                .iter()
                .any(|eq| eq.pins["b"] == cc.pins["out"] && eq.pins["out"] == inner_mux.pins["sel"])
        })
        .expect("inner mux's `sel` traces through an Eq to a Const (index 2)");
    let CellKind::Const {
        value: inner_sel_value,
    } = &inner_sel_const.kind
    else {
        unreachable!()
    };
    assert_eq!(inner_sel_value.bits, crate::bits::Bits::Small(2));
}
