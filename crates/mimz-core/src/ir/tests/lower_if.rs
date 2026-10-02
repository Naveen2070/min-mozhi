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

/// `src` through the real pipeline, elaborating `top` (the file has more than
/// one module), then `lower`.
fn lower_top(src: &str, top: &str) -> Module {
    let file = crate::parser::parse(crate::lexer::lex(src).expect("lexes")).expect("parses");
    crate::checker::check(std::slice::from_ref(&file)).expect("checks clean");
    let design = crate::elaborate::elaborate_project(
        std::slice::from_ref(&file),
        Some(top),
        &BTreeMap::new(),
    )
    .expect("elaborates");
    lower(&design)
}

#[test]
fn a_constant_taken_branch_never_lowers_a_dead_branch_that_names_nothing() {
    // At `i == 0` the dead branch reads `s[-1].y`, an instance that does not
    // exist. With no target width (a unary operand) `lower_if` used to lower
    // both branches to size the constant and panicked resolving `s__-1_y`.
    let module = lower_top(
        "module Pass {\n  in x: bit\n  out y: bit\n  y = x\n}\n\
         module Top {\n  in a: bits[4]\n  out o: bits[4]\n  repeat i: 0..4 {\n    \
         let s[i] = Pass() { x: a[i] }\n    \
         o[i] = !(if i == 0 { 0 } else { s[i - 1].y })\n  }\n}\n",
        "Top",
    );
    assert_eq!(validate(&module), Vec::new());
    // o[0] = !0 = 1, o[i] = !a[i - 1].
    assert_eq!(
        run(&module, &[("a", 0b0101, 4)], "o").bits,
        CBits::Small(0b0101)
    );
    assert_eq!(
        run(&module, &[("a", 0b1010, 4)], "o").bits,
        CBits::Small(0b1011)
    );
}

#[test]
fn a_constant_taken_branch_still_takes_a_real_dead_branchs_width() {
    // The checker types the `if` as both branches unified, so the `0` is
    // 8 bits wide and `~` gives 0xFF, as in Verilog. Lowering only the taken
    // branch would give the 1-bit `~0 = 1` instead.
    let module = lower_valid(
        "module M(K: int = 0) {\n  in a: bits[8]\n  out n: bits[8]\n  \
         n = ~(if K == 0 { 0 } else { a })\n}\n",
    );
    assert_eq!(
        run(&module, &[("a", 0x05, 8)], "n").bits,
        CBits::Small(0xFF)
    );
}

const NESTED_FNS: &str = "fn inner(a: bits[8], b: bits[8], k: bit) -> bits[8] {\n  let r = if k { a } else { b }\n  r\n}\nfn outer(a: bits[8], b: bits[8], s: bit) -> bits[8] {\n  let k = 0\n  inner(a, b, s)\n}\nmodule M {\n  in a: bits[8]\n  in b: bits[8]\n  in s: bit\n  out o: bits[8]\n  o = outer(a, b, s)\n}\n";

#[test]
fn a_callers_let_constant_never_decides_a_callees_condition() {
    // `outer`'s `let k = 0` must not leak into `inner`, whose `k` is a
    // parameter bound to the real signal `s`.
    let module = lower_valid(NESTED_FNS);
    let inputs = |s| [("a", 0x5A, 8), ("b", 0x3C, 8), ("s", s, 1)];
    assert_eq!(run(&module, &inputs(1), "o").bits, CBits::Small(0x5A));
    assert_eq!(run(&module, &inputs(0), "o").bits, CBits::Small(0x3C));
}

#[test]
fn a_callers_let_constant_never_folds_a_callees_let() {
    // Same leak through `FnStmt::Let`'s fold: `let r = if k { a } else { 0 }`
    // was recorded as the constant 0.
    let module = lower_valid(&NESTED_FNS.replace("else { b }", "else { 0 }"));
    let inputs = |s| [("a", 0x5A, 8), ("b", 0x3C, 8), ("s", s, 1)];
    assert_eq!(run(&module, &inputs(1), "o").bits, CBits::Small(0x5A));
    assert_eq!(run(&module, &inputs(0), "o").bits, CBits::Small(0));
}
