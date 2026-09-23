//! Task 9: inlining user-defined `fn` calls at lowering time.

use super::{ident, w};
use crate::ast::{Expr, ExprKind, FnParam, FnStmt, FuncDecl, Ident, LocalLet, Type};
use crate::elaborate::{Design, Signal};
use crate::ir::{Bits, Cell, CellKind, Module, lower};
use crate::span::Span;
use std::cell::Cell as StdCell;
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

fn add_expr(lhs: Expr, rhs: Expr) -> Expr {
    Expr {
        kind: ExprKind::Binary {
            op: crate::ast::BinOp::Add,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
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

fn base_design(func: FuncDecl, out_expr: Expr) -> Design {
    let mut comb = BTreeMap::new();
    comb.insert("out".to_string(), out_expr);
    let mut funcs = HashMap::new();
    funcs.insert(func.name.name.clone(), func);
    Design {
        module: "caller".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "a".into(),
                width: w(8),
            },
            Signal {
                name: "sel".into(),
                width: w(1),
            },
        ],
        outputs: vec![Signal {
            name: "out".into(),
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

fn find_port<'a>(module: &'a Module, name: &str) -> &'a Bits {
    &module
        .ports
        .iter()
        .find(|(n, ..)| n == name)
        .expect("port")
        .1
}

/// Brief's Step 1 test: a tail-only function `fn double(x) { x + x }`
/// called from a wire. Must lower to exactly one `Add` cell with both
/// `a`/`b` pins tracing straight to input `a`'s `Bits` — pure inlining,
/// no `BlackBox`/call cell.
#[test]
fn inlines_tail_only_fn_call_as_a_single_add_cell() {
    let func = FuncDecl {
        name: id("double"),
        params: vec![fn_param("x")],
        ret: Type::Bit,
        stmts: vec![],
        tail: add_expr(ident("x"), ident("x")),
        span: Span::default(),
    };
    let design = base_design(func, fn_call("double", vec![ident("a")]));
    let module = lower(&design);

    let add_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Add)
        .collect();
    assert_eq!(
        add_cells.len(),
        1,
        "call should inline to exactly one Add cell"
    );
    let add = add_cells[0];
    let a_bits = find_port(&module, "a").clone();
    assert_eq!(add.pins["a"], a_bits, "lhs of x+x traces to input a");
    assert_eq!(add.pins["b"], a_bits, "rhs of x+x traces to input a");
    assert_eq!(*find_port(&module, "out"), add.pins["out"]);
}

/// A `let`-bound intermediate: `fn f(x) { let y = x + x; y }`. The `let`
/// is pure substitution — must produce no cell beyond the single `Add`
/// that `x + x` alone would produce.
#[test]
fn let_binding_in_fn_body_is_pure_substitution_no_extra_cell() {
    let func = FuncDecl {
        name: id("f"),
        params: vec![fn_param("x")],
        ret: Type::Bit,
        stmts: vec![FnStmt::Let(LocalLet {
            name: id("y"),
            value: add_expr(ident("x"), ident("x")),
            span: Span::default(),
            inferred_width: StdCell::new(None),
        })],
        tail: ident("y"),
        span: Span::default(),
    };
    let design = base_design(func, fn_call("f", vec![ident("a")]));
    let module = lower(&design);

    let add_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Add)
        .collect();
    assert_eq!(
        add_cells.len(),
        1,
        "the `let` binding must not add a cell beyond the one Add for x+x"
    );
    let add = add_cells[0];
    assert_eq!(*find_port(&module, "out"), add.pins["out"]);
}

/// An unconditional if/else where both branches `return` different
/// expressions: `fn f(x, sel) { if sel { return x + x } else { return x } }`.
/// Must produce exactly one `Mux` selecting between the two branch
/// results, `sel`'d on the right condition, plus the one `Add` cell for
/// the `then` branch.
#[test]
fn if_else_both_returning_produces_one_mux_selected_on_cond() {
    let func = FuncDecl {
        name: id("f"),
        params: vec![fn_param("x"), fn_param("sel")],
        ret: Type::Bit,
        stmts: vec![FnStmt::If {
            cond: ident("sel"),
            then: vec![FnStmt::Return(add_expr(ident("x"), ident("x")))],
            els: Some(vec![FnStmt::Return(ident("x"))]),
        }],
        // Unreachable (every path returns) — placeholder tail, never lowered.
        tail: ident("x"),
        span: Span::default(),
    };
    let design = base_design(func, fn_call("f", vec![ident("a"), ident("sel")]));
    let module = lower(&design);

    let mux_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Mux)
        .collect();
    assert_eq!(mux_cells.len(), 1, "one Mux for the if/else");
    let mux = mux_cells[0];
    let sel_bits = find_port(&module, "sel").clone();
    assert_eq!(
        mux.pins["sel"], sel_bits,
        "mux sel traces to the `sel` cond"
    );

    let add_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Add)
        .collect();
    assert_eq!(add_cells.len(), 1, "one Add cell for the then-branch x+x");
    assert_eq!(
        mux.pins["a"], add_cells[0].pins["out"],
        "mux `a` is the then branch (x+x)"
    );

    // `b` (the else branch, `x`) is narrower than `a` (`x+x`, which grows
    // one bit past `x`'s own width) — `push_mux_cell` widens the narrower
    // arm to match rather than wiring mismatched widths to the same `out`
    // pin (GAP-1: this exact bug, found via `enum_encoding.mimz`'s
    // all-constant `match` arms, produced `WidthMismatch`es `validate`
    // silently let through before). `b`'s original bits are still `x`,
    // zero-extended.
    let a_bits = find_port(&module, "a").clone();
    assert_eq!(mux.pins["b"].width(), mux.pins["a"].width());
    assert_eq!(
        &mux.pins["b"].nets[..a_bits.width() as usize],
        &a_bits.nets[..],
        "mux `b`'s low bits are still the else branch (x)"
    );
    assert_eq!(*find_port(&module, "out"), mux.pins["out"]);
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

/// Task 3 (2026-09-10 IR-base-gap-closure plan): the if/return mux tree
/// must size each literal branch to the function's DECLARED return width
/// (here `signed[4]`), not each literal's own natural width (`0` needs 1
/// bit, `7` needs 3 bits) — the third call site of the literal/const
/// context-sizing bug already fixed twice elsewhere via `lower_expr_sized`.
#[test]
fn fn_if_return_mux_sizes_literals_to_the_declared_return_width() {
    let func = FuncDecl {
        name: id("find_first_set"),
        params: vec![fn_param("sel")],
        ret: Type::Signed(Box::new(int_lit(4))),
        stmts: vec![FnStmt::If {
            cond: ident("sel"),
            then: vec![FnStmt::Return(int_lit(0))],
            els: Some(vec![FnStmt::Return(int_lit(7))]),
        }],
        // Unreachable (every path returns) — placeholder tail, never lowered.
        tail: ident("sel"),
        span: Span::default(),
    };
    let design = base_design(func, fn_call("find_first_set", vec![ident("sel")]));
    let module = lower(&design);

    let mux_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Mux)
        .collect();
    assert_eq!(mux_cells.len(), 1, "one Mux for the if/return");
    let mux = mux_cells[0];
    assert_eq!(
        mux.pins["out"].width(),
        4,
        "return-mux output must match the declared signed[4] width (4), not the widest literal's natural width (7 needs 3 bits)"
    );
}
