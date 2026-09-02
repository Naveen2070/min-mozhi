mod lower_basic;
mod lower_binops;
mod lower_fn_inline;
mod lower_mem;
mod lower_mux;
mod lower_regs;
mod lower_unary_concat_slice;

use crate::ast::{Expr, ExprKind};
use crate::elaborate::{Design, Signal, Width};
use crate::span::Span;
use std::collections::BTreeMap;

pub(super) fn w(bits: u32) -> Width {
    Width {
        bits,
        signed: false,
    }
}

pub(super) fn ident(name: &str) -> Expr {
    Expr {
        kind: ExprKind::Ident(name.to_string()),
        span: Span::default(),
    }
}

/// Shared fixture: `wire sum = a + b` over two 8-bit inputs, a lossless
/// 9-bit `sum` wire. Reused by later tasks' printer/parser tests
/// (Tasks 12-13) as well as this task's lowering tests.
pub(super) fn adder_design() -> Design {
    let mut comb = BTreeMap::new();
    comb.insert(
        "sum".to_string(),
        Expr {
            kind: ExprKind::Binary {
                op: crate::ast::BinOp::Add,
                lhs: Box::new(ident("a")),
                rhs: Box::new(ident("b")),
            },
            span: Span::default(),
        },
    );
    Design {
        module: "adder".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "a".into(),
                width: w(8),
            },
            Signal {
                name: "b".into(),
                width: w(8),
            },
        ],
        outputs: vec![],
        wires: vec![Signal {
            name: "sum".into(),
            width: w(9),
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
    }
}
