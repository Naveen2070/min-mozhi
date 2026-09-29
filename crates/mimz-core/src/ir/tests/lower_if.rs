//! `LowerCtx::lower_if`: an all-constant `if` sized to its declared target,
//! and a compile-time condition folded to its taken branch. See
//! `docs/superpowers/specs/2026-09-29-ir-lowering-validate-fixes-design.local.md`.

use super::{ident, lower_valid, w};
use crate::ast::{BinOp, Expr, ExprKind};
use crate::bits::Bits as CBits;
use crate::elaborate::{Design, Signal};
use crate::ir::exec::Executor;
use crate::ir::validate::validate;
use crate::ir::{CellKind, Module, lower};
use crate::span::Span;
use crate::value::Val;
use std::collections::BTreeMap;

fn run(module: &Module, inputs: &[(&str, u128, u32)], name: &str) -> Val {
    let mut ex = Executor::new(module);
    for &(port, value, width) in inputs {
        ex.set_input(port, Val::new(value, width, false));
    }
    ex.tick();
    ex.get_output(name)
}

#[test]
fn an_all_constant_if_is_sized_to_its_declared_port() {
    let module =
        lower_valid("module M {\n  in c: bit\n  out o: bits[8]\n  o = if c { 5 } else { 5 }\n}\n");
    for c in 0..2 {
        assert_eq!(run(&module, &[("c", c, 1)], "o").bits, CBits::Small(5));
    }
}

#[test]
fn an_all_constant_if_with_different_branches_keeps_both_values() {
    let module =
        lower_valid("module M {\n  in c: bit\n  out o: bits[8]\n  o = if c { 5 } else { 3 }\n}\n");
    assert_eq!(run(&module, &[("c", 1, 1)], "o").bits, CBits::Small(5));
    assert_eq!(run(&module, &[("c", 0, 1)], "o").bits, CBits::Small(3));
}

fn mux_count(module: &Module) -> usize {
    module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Mux)
        .count()
}

fn int(value: u128) -> Expr {
    Expr {
        kind: ExprKind::Int {
            value: CBits::Small(value),
            raw: value.to_string(),
        },
        span: Span::default(),
    }
}

/// `ripple_adder.mimz`'s bit-0 carry after `repeat` unrolling:
/// `if 0 == 0 { cin } else { fa__-1_cout }`. The dead branch names an
/// instance that does not exist, so nothing drives it.
fn ghost_design() -> Design {
    let cond = Expr {
        kind: ExprKind::Binary {
            op: BinOp::Eq,
            lhs: Box::new(int(0)),
            rhs: Box::new(int(0)),
        },
        span: Span::default(),
    };
    let mut comb = BTreeMap::new();
    comb.insert(
        "o".to_string(),
        Expr {
            kind: ExprKind::IfExpr {
                cond: Box::new(cond),
                then: Box::new(ident("cin")),
                els: Box::new(ident("fa__-1_cout")),
            },
            span: Span::default(),
        },
    );
    Design {
        module: "M".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "cin".into(),
            width: w(1),
        }],
        outputs: vec![Signal {
            name: "o".into(),
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

#[test]
fn a_constant_condition_never_lowers_its_dead_branch() {
    let module = lower(&ghost_design());

    assert_eq!(validate(&module), Vec::new());
    assert_eq!(mux_count(&module), 0);
    for cin in 0..2 {
        assert_eq!(
            run(&module, &[("cin", cin, 1)], "o").bits,
            CBits::Small(cin)
        );
    }
}

#[test]
fn a_constant_condition_over_two_signals_reads_the_taken_one() {
    let module = lower_valid(
        "module M(MODE: int = 1) {\n  in a: bits[8]\n  in b: bits[8]\n  out o: bits[8]\n  o = if MODE == 1 { a } else { b }\n}\n",
    );

    assert_eq!(mux_count(&module), 0);
    let port = |name: &str| {
        module
            .ports
            .iter()
            .find(|(n, ..)| n == name)
            .expect("port")
            .1
            .clone()
    };
    assert_eq!(port("o").nets, port("a").nets);
}
