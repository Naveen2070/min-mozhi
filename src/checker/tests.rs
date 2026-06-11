//! Checker unit tests — one per rule/error code, plus clean-pass cases.
//! Error-path tests assert on the CODE (stable contract) and message
//! substrings (loose, so wording can be polished).

use crate::diag::Diag;
use crate::{lexer, parser};

use super::check;

fn parse(src: &str) -> crate::ast::File {
    let toks = lexer::lex(src).expect("lexes");
    parser::parse(toks).expect("parses")
}

fn check_one(src: &str) -> Result<(), Vec<Diag>> {
    check(&[parse(src)])
}

fn errs(src: &str) -> Vec<Diag> {
    check_one(src).expect_err("expected checker errors")
}

/// First error must carry the expected code; returns it for further asserts.
fn first_err(src: &str, code: &str) -> Diag {
    let diags = errs(src);
    assert_eq!(
        diags[0].code,
        Some(code),
        "expected {code}, got {:?}: {}",
        diags[0].code,
        diags[0].msg
    );
    diags.into_iter().next().unwrap()
}

const COUNTER: &str = "module Counter(WIDTH: int = 8) {
  clock clk
  reset rst
  out count: bits[WIDTH]
  reg value: bits[WIDTH] = 0
  on rise(clk) {
    value <- value +% 1
  }
  count = value
}
";

#[test]
fn clean_module_passes() {
    check_one(COUNTER).expect("counter is clean");
}

#[test]
fn duplicate_module_across_files_is_e0001_in_the_right_file() {
    let files = [parse("module A {\n}\n"), parse("module A {\n}\n")];
    let diags = check(&files).expect_err("duplicate");
    assert_eq!(diags[0].code, Some("E0001"));
    assert_eq!(diags[0].file, Some(1), "second definition is the error");
}

#[test]
fn duplicate_signal_in_module_is_e0003() {
    let d = first_err(
        "module M {\n  in x: bit\n  out y: bit\n  wire x: bit = y\n  y = x\n}\n",
        "E0003",
    );
    assert!(d.msg.contains("declared twice"));
}

#[test]
fn duplicate_file_const_is_e0004() {
    first_err(
        "const N: int = 1\nconst N: int = 2\nmodule M {\n}\n",
        "E0004",
    );
}

#[test]
fn unknown_name_is_e0101_with_teaching_help() {
    let d = first_err("module M {\n  out y: bit\n  y = nope\n}\n", "E0101");
    assert!(d.msg.contains("nope"));
    assert!(d.help.unwrap().contains("spelling"));
}

#[test]
fn unknown_module_in_inst_is_e0102_and_mentions_import() {
    let d = first_err(
        "module M {\n  in a: bit\n  let u = Ghost() { a: a }\n}\n",
        "E0102",
    );
    assert!(d.help.unwrap().contains("import"));
}

#[test]
fn unknown_enum_variant_is_e0103_and_lists_variants() {
    let src = "module M {\n  out y: bit\n  enum S { A, B }\n  reg s: S = S.A\n  clock c\n  reset r\n  y = s == S.Z\n}\n";
    let d = first_err(src, "E0103");
    assert!(d.help.unwrap().contains("A, B"));
}

#[test]
fn reading_an_input_of_an_instance_is_e0104() {
    let src = "module Child {\n  in a: bit\n  out z: bit\n  z = a\n}\nmodule M {\n  in x: bit\n  out y: bit\n  let c = Child() { a: x }\n  y = c.a\n}\n";
    let d = first_err(src, "E0104");
    assert!(d.help.unwrap().contains("input"));
}

#[test]
fn field_on_a_wire_is_e0105() {
    first_err(
        "module M {\n  in x: bit\n  out y: bit\n  y = x.bit0\n}\n",
        "E0105",
    );
}

#[test]
fn unknown_param_in_inst_is_e0106_and_lists_params() {
    let src = "module Child(W: int = 1) {\n  out z: bit\n  z = 0\n}\nmodule M {\n  out y: bit\n  let c = Child(DEPTH: 4)\n  y = c.z\n}\n";
    let d = first_err(src, "E0106");
    assert!(d.help.unwrap().contains('W'));
}

#[test]
fn connecting_an_output_is_e0107() {
    let src = "module Child {\n  out z: bit\n  z = 1\n}\nmodule M {\n  in x: bit\n  out y: bit\n  let c = Child() { z: x }\n  y = c.z\n}\n";
    let d = first_err(src, "E0107");
    assert!(d.help.unwrap().contains('.'));
}

#[test]
fn assigning_an_input_is_e0108() {
    let d = first_err("module M {\n  in x: bit\n  x = 1\n}\n", "E0108");
    assert!(d.msg.contains("input"));
}

#[test]
fn on_rise_of_a_non_clock_is_e0109() {
    let src = "module M {\n  clock clk\n  reset rst\n  in x: bit\n  reg v: bit = 0\n  on rise(x) {\n    v <- 1\n  }\n}\n";
    first_err(src, "E0109");
}

#[test]
fn const_arithmetic_and_repeat_bounds_evaluate() {
    let src = "const N: int = 2 + 2\nmodule M {\n  out y: bits[N]\n  repeat i: 0..N {\n    y[i] = 0\n  }\n}\n";
    check_one(src).expect("const-driven repeat bounds are fine");
}

#[test]
fn non_constant_repeat_bound_is_e0201() {
    let src =
        "module M {\n  in x: bits[4]\n  out y: bits[4]\n  repeat i: 0..x {\n    y[i] = 0\n  }\n}\n";
    let d = first_err(src, "E0201");
    assert!(d.msg.contains("not a compile-time constant"));
}

#[test]
fn const_using_a_later_const_is_e0201() {
    first_err(
        "const A: int = B\nconst B: int = 1\nmodule M {\n}\n",
        "E0201",
    );
}

#[test]
fn const_overflow_is_e0202() {
    let src = "const HUGE: int = 170141183460469231731687303715884105727 + 1\nmodule M {\n}\n";
    first_err(src, "E0202");
}

#[test]
fn reg_without_reset_declaration_is_e0301() {
    let src = "module M {\n  clock clk\n  reg v: bit = 0\n  on rise(clk) {\n    v <- 1\n  }\n}\n";
    let d = first_err(src, "E0301");
    assert!(d.help.unwrap().contains("reset"));
}
