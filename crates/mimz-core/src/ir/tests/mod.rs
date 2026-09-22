mod exec;
mod lower_array_fn_params;
mod lower_basic;
mod lower_binops;
mod lower_bitselect_write;
mod lower_blackbox;
mod lower_builtins;
mod lower_consts;
mod lower_ct_width;
mod lower_fn_inline;
mod lower_loops;
mod lower_mem;
mod lower_mux;
mod lower_regs;
mod lower_unary_concat_slice;
mod parse_line;
mod print_line;
mod print_sexpr;
mod validate;

use crate::ast::{Expr, ExprKind};
use crate::elaborate::{Design, Signal, Width};
use crate::span::Span;
use std::collections::BTreeMap;

/// Runs the real lex -> parse -> check -> elaborate_project pipeline over
/// `src`, returning the elaborated `Design`. A hand-built `Design` would
/// bypass `elaborate` entirely and prove nothing about bugs that live in
/// that pass — and, for GAP-1 Task 6's signedness regressions, nothing
/// about the declared `signed`/`bits` types the checker actually assigns.
pub(super) fn elaborate_src(src: &str) -> Design {
    let file = crate::parser::parse(crate::lexer::lex(src).expect("lexes")).expect("parses");
    crate::checker::check(std::slice::from_ref(&file)).expect("checks clean");
    crate::elaborate::elaborate_project(std::slice::from_ref(&file), None, &BTreeMap::new())
        .expect("elaborates")
}

/// `elaborate_src` + `lower`, for tests that only care about the final
/// `Module` (GAP-1 Task 6's own sign/width-divergence regressions).
pub(super) fn lower_ok(src: &str) -> crate::ir::Module {
    crate::ir::lower(&elaborate_src(src))
}

/// `lower_ok` + a `validate` assertion — every Task 6 regression below
/// wants BOTH "the netlist is well-formed" (finding 3's `WidthMismatch`)
/// and "it computes the right value" (findings 1/2/4), and a value probe
/// over an invalid module proves nothing.
pub(super) fn lower_valid(src: &str) -> crate::ir::Module {
    let module = lower_ok(src);
    let errs = crate::ir::validate::validate(&module);
    assert!(errs.is_empty(), "module must validate clean, got {errs:?}");
    module
}

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
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    }
}
