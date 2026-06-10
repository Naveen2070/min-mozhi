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
fn chained_comparison_is_an_error() {
    let d = parse_err("module M {\n  in a: bit\n  out y: bit\n  y = a < a < a\n}\n");
    assert!(d[0].msg.contains("chained"));
}

#[test]
fn wire_if_without_else_teaches_about_latches() {
    let d = parse_err("module M {\n  in s: bit\n  out y: bit\n  y = if s { 1 }\n}\n");
    assert!(d[0].msg.contains("else"));
    assert!(d[0].help.as_ref().unwrap().contains("latch"));
}

#[test]
fn reg_without_reset_value_is_an_error() {
    let d = parse_err("module M {\n  clock clk\n  reset rst\n  reg v: bits[8]\n}\n");
    assert!(d[0].msg.contains("reset value"));
}

#[test]
fn assign_arrow_confusion_teaches() {
    let d = parse_err(
        "module M {\n  clock clk\n  reset rst\n  reg v: bits[8] = 0\n  on rise(clk) {\n    v = 1\n  }\n}\n",
    );
    assert!(d[0].help.as_ref().unwrap().contains("<-"));
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
