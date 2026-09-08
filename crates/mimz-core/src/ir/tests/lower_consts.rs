//! GAP-1 sub-gap (2026-09-08): module parameters and file-level `const`s
//! resolved as plain identifiers in `lower_expr`'s `ExprKind::Ident` arm.
//!
//! Both fixtures below reproduce a real example's exact shape (rather than
//! a synthetic minimal repro) so a regression here is caught in terms a
//! future reader can match straight back to `examples/*/fn_with_const.mimz`
//! and `examples/*/blinker.mimz`.

use super::{ident, w};
use crate::ast::{BinOp, Expr, ExprKind, FnParam, FuncDecl, Ident, Type};
use crate::elaborate::{Design, Signal};
use crate::ir::{Cell, CellKind, Module, lower};
use crate::span::Span;
use std::collections::{BTreeMap, HashMap};

fn id(name: &str) -> Ident {
    Ident {
        name: name.to_string(),
        span: Span::default(),
    }
}

fn fn_param(name: &str) -> FnParam {
    FnParam {
        name: id(name),
        ty: Type::Bit,
        span: Span::default(),
    }
}

fn binop(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    Expr {
        kind: ExprKind::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
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

/// A `Const` cell whose `out` pin equals `target`, or `None`.
fn const_cell_driving<'a>(module: &'a Module, target: &crate::ir::Bits) -> Option<&'a Cell> {
    module
        .cells
        .iter()
        .find(|c| matches!(c.kind, CellKind::Const { .. }) && c.pins["out"] == *target)
}

/// `examples/*/fn_with_const.mimz`'s exact shape: a file-level `const
/// SCALE: int = 3` referenced inside a `fn`'s body (`a >> SCALE`), called
/// from a module wire. Before this fix, lowering `SCALE` as an `Ident`
/// panicked ("no driver recorded for signal `SCALE`") because a file-level
/// const is neither a `locals` binding nor a comb-driven signal.
#[test]
fn fn_body_references_file_level_const_as_shift_amount() {
    let func = FuncDecl {
        name: id("scaled"),
        params: vec![fn_param("a")],
        ret: Type::Bit,
        stmts: vec![],
        tail: binop(BinOp::Shr, ident("a"), ident("SCALE")),
        span: Span::default(),
    };
    let mut funcs = HashMap::new();
    funcs.insert("scaled".to_string(), func);

    let mut comb = BTreeMap::new();
    comb.insert(
        "result".to_string(),
        Expr {
            kind: ExprKind::FnCall {
                name: id("scaled"),
                args: vec![ident("a")],
            },
            span: Span::default(),
        },
    );

    let mut consts = BTreeMap::new();
    consts.insert("SCALE".to_string(), 3);

    let design = Design {
        module: "Scaled".to_string(),
        consts,
        inputs: vec![Signal {
            name: "a".into(),
            width: w(8),
        }],
        outputs: vec![Signal {
            name: "result".into(),
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
    };

    // Must not panic (the GAP-1 sub-gap this test guards).
    let module = lower(&design);

    let shr_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Shr)
        .collect();
    assert_eq!(shr_cells.len(), 1, "one Shr cell for `a >> SCALE`");
    let shr = shr_cells[0];

    let a_bits = find_port(&module, "a").clone();
    assert_eq!(shr.pins["a"], a_bits, "shr's lhs traces to input `a`");

    let scale_const = const_cell_driving(&module, &shr.pins["b"])
        .expect("shr's shift-amount pin traces to a Const cell (the folded SCALE value)");
    let CellKind::Const { value } = &scale_const.kind else {
        unreachable!()
    };
    assert_eq!(value.bits, crate::bits::Bits::Small(3));
}

/// `examples/*/blinker.mimz`'s exact shape: a module parameter
/// (`module Blinker(LIMIT: int = 50000000)`) referenced directly in a
/// module body (`if cnt == LIMIT`). Before this fix, lowering `LIMIT` as an
/// `Ident` at module level (no `locals` in scope at all) panicked the same
/// way, since a module parameter is folded into the very same
/// `design.consts` map as a file-level const.
#[test]
fn module_body_references_module_parameter_directly() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "wrapped".to_string(),
        binop(BinOp::Eq, ident("cnt"), ident("LIMIT")),
    );

    let mut consts = BTreeMap::new();
    consts.insert("LIMIT".to_string(), 1_000_000);

    let design = Design {
        module: "Blinker".to_string(),
        consts,
        inputs: vec![Signal {
            name: "cnt".into(),
            width: w(26),
        }],
        outputs: vec![Signal {
            name: "wrapped".into(),
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
    };

    // Must not panic (the GAP-1 sub-gap this test guards).
    let module = lower(&design);

    let eq_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Eq)
        .collect();
    assert_eq!(eq_cells.len(), 1, "one Eq cell for `cnt == LIMIT`");
    let eq = eq_cells[0];

    let cnt_bits = find_port(&module, "cnt").clone();
    assert_eq!(eq.pins["a"], cnt_bits, "eq's lhs traces to input `cnt`");

    let limit_const = const_cell_driving(&module, &eq.pins["b"])
        .expect("eq's rhs pin traces to a Const cell (the folded LIMIT value)");
    let CellKind::Const { value } = &limit_const.kind else {
        unreachable!()
    };
    assert_eq!(value.bits, crate::bits::Bits::Small(1_000_000));
}
