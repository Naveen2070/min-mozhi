//! Parser unit tests — including the locked-in safety behaviors
//! (precedence trap, latch teaching, `=` vs `<-`).

use super::*;
use crate::lexer::lex;

fn parse_ok(src: &str) -> File {
    parse(lex(src).expect("lex error")).expect("parse error")
}

fn parse_err(src: &str) -> Vec<Diag> {
    match parse(lex(src).expect("lex error")) {
        Ok(_) => panic!("expected a parse error"),
        Err(d) => d,
    }
}

#[test]
fn parses_counter() {
    let f = parse_ok(
        "module Counter(WIDTH: int = 8) {\n  clock clk\n  reset rst\n  out count: bits[WIDTH]\n  reg value: bits[WIDTH] = 0\n  on rise(clk) {\n    value <- value +% 1\n  }\n  count = value\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    assert_eq!(m.name.name, "Counter");
    assert_eq!(m.items.len(), 6);
}

#[test]
fn parses_tanglish_counter_to_same_shape() {
    let f = parse_ok(
        "thoguthi Counter(WIDTH: int = 8) {\n  kadigaram clk\n  meetamai rst\n  veli count: bits[WIDTH]\n  nilai value: bits[WIDTH] = 0\n  pothu yetram(clk) {\n    value <- value +% 1\n  }\n  count = value\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    assert_eq!(m.name.name, "Counter");
    assert_eq!(m.items.len(), 6);
}

// ---- grammar engine: thamizh-order profile (spec/04, Phase 1.8) ----

#[test]
fn thamizh_order_on_block_parses_to_the_same_shape() {
    // `syntax thamizh` + the flipped clocked block `yetram(clk) pothu { }`
    // must build the SAME module as the code-order counter: 6 items, an
    // `on` block clocked by `clk`. The directive leaves no trace in the AST.
    let f = parse_ok(
        "ilakkanam thamizh\nthoguthi Counter(WIDTH: int = 8) {\n  kadigaram clk\n  meetamai rst\n  veli count: bits[WIDTH]\n  nilai value: bits[WIDTH] = 0\n  yetram(clk) pothu {\n    value <- value +% 1\n  }\n  count = value\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    assert_eq!(m.items.len(), 6);
    let on = m
        .items
        .iter()
        .find_map(|it| match it {
            ModuleItem::On(o) => Some(o),
            _ => None,
        })
        .expect("the flipped block must still parse as an `on` block");
    assert_eq!(on.clock.name, "clk");
    assert_eq!(
        on.body.len(),
        1,
        "the body (`value <- value +% 1`) survives"
    );
}

#[test]
fn english_syntax_thamizh_directive_also_selects_the_profile() {
    // Keyword flavor and word-order profile are orthogonal: the English
    // spelling `syntax thamizh` selects the same profile as `ilakkanam thamizh`.
    let f = parse_ok(
        "syntax thamizh\nmodule M {\n  clock clk\n  reg r: bit = 0\n  rise(clk) on {\n    r <- r\n  }\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    assert!(m.items.iter().any(|it| matches!(it, ModuleItem::On(_))));
}

#[test]
fn unknown_syntax_profile_is_e1112() {
    let d = parse_err("syntax wibble\nmodule M {\n  in a: bit\n}\n");
    assert!(d.iter().any(|e| e.code == Some("E1112")));
}

#[test]
fn flipped_on_block_needs_the_directive() {
    // Without `syntax thamizh`, a leading `rise(...)` is not a valid item.
    parse_err("module M {\n  clock clk\n  reg r: bit = 0\n  rise(clk) on {\n    r <- r\n  }\n}\n");
}

#[test]
fn deeply_nested_expression_errors_not_overflows() {
    // Security: a recursive-descent parser with no depth limit stack-overflows
    // (aborts the process) on `(((…)))`. The MAX_DEPTH guard must turn that into
    // a clean E1113. 2000 parens is far past the cap (64) and cheap to parse.
    let src = format!(
        "module M {{\n  out y: bit\n  y = {}1{}\n}}\n",
        "(".repeat(2000),
        ")".repeat(2000)
    );
    let d = parse_err(&src);
    assert!(d.iter().any(|e| e.code == Some("E1113")));
}

#[test]
fn deeply_nested_unary_errors_not_overflows() {
    // The prefix-operator chain `!!!!…x` recurses through `unary`, not `expr` —
    // its own guard must catch it too.
    let src = format!(
        "module M {{\n  out y: bit\n  y = {}1\n}}\n",
        "!".repeat(2000)
    );
    let d = parse_err(&src);
    assert!(d.iter().any(|e| e.code == Some("E1113")));
}

#[test]
fn stray_top_level_brace_does_not_hang() {
    // Regression: a stray `}` at file level (e.g. unbalanced braces from error
    // recovery) once spun `file()` forever — `sync_to_newline` stops at `}`
    // without consuming it. The loop must terminate with an error, not OOM.
    let d = parse_err("module M {\n  out y: bit\n  y = 0\n}\n}\n");
    assert!(d.iter().any(|e| e.code == Some("E1102")));
}

#[test]
fn rust_precedence_defuses_the_c_trap() {
    // x & 1 == 0 must parse as (x & 1) == 0
    let f = parse_ok("module M {\n  in x: bits[8]\n  out y: bit\n  y = x & 1 == 0\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let ModuleItem::Drive { rhs, .. } = &m.items[2] else {
        panic!()
    };
    let ExprKind::Binary { op, .. } = &rhs.kind else {
        panic!()
    };
    assert_eq!(*op, BinOp::Eq, "top of the tree must be `==`, not `&`");
}

#[test]
fn monotonic_chained_comparison_desugars_to_and() {
    // 0 <= x <= 7  →  (0 <= x) && (x <= 7); the shared `x` is read twice
    // (identical combinational value). The safe Python-style form (8.9).
    let f = parse_ok("module M {\n  in x: bits[8]\n  out y: bit\n  y = 0 <= x <= 7\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let ModuleItem::Drive { rhs, .. } = &m.items[2] else {
        panic!()
    };
    let ExprKind::Binary { op, lhs, rhs } = &rhs.kind else {
        panic!()
    };
    assert_eq!(*op, BinOp::LogicAnd, "a chain desugars to &&");
    let ExprKind::Binary { op: lop, .. } = &lhs.kind else {
        panic!("left of && is the first comparison")
    };
    let ExprKind::Binary { op: rop, .. } = &rhs.kind else {
        panic!("right of && is the second comparison")
    };
    assert_eq!(*lop, BinOp::Le);
    assert_eq!(*rop, BinOp::Le);
}

#[test]
fn mixed_direction_chain_is_an_error() {
    // `a < b > c` is the genuinely confusing form — still rejected.
    let d = parse_err("module M {\n  in a: bit\n  out y: bit\n  y = a < a > a\n}\n");
    assert_eq!(d[0].code, Some("E1109"));
    assert!(d[0].msg.contains("one direction"));
}

#[test]
fn equality_cannot_be_chained() {
    let d = parse_err("module M {\n  in a: bit\n  out y: bit\n  y = a == a == a\n}\n");
    assert_eq!(d[0].code, Some("E1109"));
}

#[test]
fn wire_if_without_else_teaches_about_latches() {
    let d = parse_err("module M {\n  in s: bit\n  out y: bit\n  y = if s { 1 }\n}\n");
    assert_eq!(d[0].code, Some("E1108"));
    assert!(d[0].msg.contains("else"));
    assert!(d[0].help.as_ref().unwrap().contains("latch"));
}

#[test]
fn reg_without_reset_value_is_an_error() {
    let d = parse_err("module M {\n  clock clk\n  reset rst\n  reg v: bits[8]\n}\n");
    assert_eq!(d[0].code, Some("E1104"));
    assert!(d[0].msg.contains("reset value"));
}

#[test]
fn assign_arrow_confusion_teaches() {
    let d = parse_err(
        "module M {\n  clock clk\n  reset rst\n  reg v: bits[8] = 0\n  on rise(clk) {\n    v = 1\n  }\n}\n",
    );
    assert_eq!(d[0].code, Some("E1106"));
    assert!(d[0].help.as_ref().unwrap().contains("<-"));
}

#[test]
fn every_parse_error_carries_a_code() {
    // The structural promise behind the E11xx retrofit: no parser
    // diagnostic ships codeless (the `error()` helper makes it
    // impossible; this locks the contract from the outside).
    let broken = [
        "module M {\n  out y: bit\n  y = if y { 1 }\n}\n",
        "garbage here\n",
        "module M {\n  out y: bit\n  enum E {\n  }\n  y = 0\n}\n",
        "module M {\n  out y: bit\n  y = nope(1)\n}\n",
    ];
    for src in broken {
        for d in parse_err(src) {
            assert!(
                d.code.is_some_and(|c| c.starts_with("E11")),
                "codeless or mis-blocked parse error: {}",
                d.msg
            );
        }
    }
}

#[test]
fn parses_repeat_and_const() {
    parse_ok(
        "const N: int = 8\nmodule M {\n  in e: bits[8]\n  out led: bits[8]\n  repeat i: 0..8 {\n    led[i] = e[i]\n  }\n}\n",
    );
}

#[test]
fn parses_test_block() {
    parse_ok(
        "test \"counts\" for Counter(WIDTH: 4) {\n  tick(clk)\n  expect count == 1\n  tick(clk, 3)\n  expect count == 4\n}\n",
    );
}
