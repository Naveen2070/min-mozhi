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
fn clog2_in_a_width_position_is_clean() {
    // clog2(9) = 4, so `o` is a legal bits[4].
    check_one("module M {\n  out o: bits[clog2(9)]\n  o = 0\n}\n")
        .expect("clog2 in a width is clean");
}

#[test]
fn clog2_of_a_module_const_is_clean() {
    // A pointer width derived from a `const` depth — the foldable path that
    // also emits (unlike an overridable parameter, see the emit tests).
    check_one("module M {\n  const DEPTH: int = 16\n  out ptr: bits[clog2(DEPTH)]\n  ptr = 0\n}\n")
        .expect("clog2 of a const is clean");
}

#[test]
fn clog2_of_zero_is_e0202() {
    first_err("module M {\n  out o: bits[clog2(0)]\n  o = 0\n}\n", "E0202");
}

#[test]
fn clog2_in_a_runtime_value_position_is_e0407() {
    // clog2 is compile-time only — it has no value in a drive RHS.
    first_err("module M {\n  out o: bits[8]\n  o = clog2(8)\n}\n", "E0407");
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

// ---- Pass 4: widths (E0401–E0410) ------------------------------------

#[test]
fn assignment_width_mismatch_is_e0401() {
    let d = first_err(
        "module M {\n  in a: bits[4]\n  out y: bits[8]\n  y = a\n}\n",
        "E0401",
    );
    assert!(d.msg.contains("bits[8]") && d.msg.contains("bits[4]"));
    assert!(d.help.unwrap().contains("extend"));
}

#[test]
fn plus_into_same_width_target_teaches_wrap_in_e0401() {
    let src = "module M {\n  clock clk\n  reset rst\n  reg value: bits[8] = 0\n  on rise(clk) {\n    value <- value + 1\n  }\n}\n";
    let d = first_err(src, "E0401");
    assert!(
        d.help.unwrap().contains("+%"),
        "must teach the wrap operator"
    );
}

#[test]
fn connection_width_mismatch_is_e0401_naming_the_port() {
    let src = "module Child {\n  in a: bits[8]\n  out z: bits[8]\n  z = a\n}\nmodule M {\n  in x: bits[4]\n  out y: bits[8]\n  let c = Child() { a: x }\n  y = c.z\n}\n";
    let d = first_err(src, "E0401");
    assert!(d.msg.contains("`a`"), "error names the child port");
}

#[test]
fn replication_width_is_count_times_inner() {
    check_one("module M {\n  in a: bits[4]\n  out y: bits[8]\n  y = {2{a}}\n}\n")
        .expect("{2{bits[4]}} is bits[8]");
    check_one("module M {\n  in a: bits[4]\n  out z: bits[12]\n  z = {3{a}}\n}\n")
        .expect("{3{bits[4]}} is bits[12]");
}

#[test]
fn replication_width_mismatch_is_e0401() {
    // {2{a}} of a bits[4] is bits[8] — assigning it to bits[4] is a width error.
    first_err(
        "module M {\n  in a: bits[4]\n  out y: bits[4]\n  y = {2{a}}\n}\n",
        "E0401",
    );
}

#[test]
fn a_non_constant_replication_count_is_e0201() {
    first_err(
        "module M {\n  in a: bits[4]\n  in n: bits[4]\n  out y: bits[8]\n  y = {n{a}}\n}\n",
        "E0201",
    );
}

#[test]
fn a_zero_replication_count_is_e0410() {
    first_err(
        "module M {\n  in a: bits[4]\n  out y: bits[4]\n  y = {0{a}}\n}\n",
        "E0410",
    );
}

#[test]
fn dont_care_pattern_must_match_the_scrutinee_width() {
    // `0b1??` is 3 bits — clean on bits[3], a width error on bits[4].
    check_one(
        "module M {\n  in s: bits[3]\n  out y: bit\n  y = match s {\n    0b1?? => true\n    _ => false\n  }\n}\n",
    )
    .expect("0b1?? matches a bits[3]");
    first_err(
        "module M {\n  in s: bits[4]\n  out y: bit\n  y = match s {\n    0b1?? => true\n    _ => false\n  }\n}\n",
        "E0409",
    );
}

#[test]
fn a_dont_care_match_still_needs_a_wildcard() {
    // Masked patterns earn no exhaustiveness credit, so even though `0b1??`
    // and `0b0??` together cover every 3-bit value, a `_` is still required.
    first_err(
        "module M {\n  in s: bits[3]\n  out y: bit\n  y = match s {\n    0b1?? => true\n    0b0?? => false\n  }\n}\n",
        "E0601",
    );
}

#[test]
fn a_dont_care_pattern_on_an_enum_is_e0409() {
    let src = "module M {\n  clock clk\n  reset rst\n  enum S { A, B }\n  reg s: S = S.A\n  out y: bit\n  on rise(clk) {\n    s <- s\n  }\n  y = match s {\n    0b1? => true\n    _ => false\n  }\n}\n";
    first_err(src, "E0409");
}

#[test]
fn min_max_take_two_same_width_operands() {
    check_one(
        "module M {\n  in a: bits[8]\n  in b: bits[8]\n  out y: bits[8]\n  y = max(a, b)\n}\n",
    )
    .expect("max of two bits[8] is bits[8]");
}

#[test]
fn min_of_mismatched_widths_is_e0402() {
    first_err(
        "module M {\n  in a: bits[4]\n  in b: bits[8]\n  out y: bits[8]\n  y = min(a, b)\n}\n",
        "E0402",
    );
}

#[test]
fn abs_of_signed_grows_one_bit() {
    // abs(signed[4]) is signed[5] (room for abs(MIN)).
    check_one("module M {\n  in a: signed[4]\n  out y: signed[5]\n  y = abs(a)\n}\n")
        .expect("abs grows to signed[N+1]");
}

#[test]
fn abs_of_unsigned_is_e0407() {
    first_err(
        "module M {\n  in a: bits[4]\n  out y: bits[4]\n  y = abs(a)\n}\n",
        "E0407",
    );
}

#[test]
fn nand_reduces_to_a_bit() {
    check_one("module M {\n  in a: bits[4]\n  out y: bit\n  y = nand(a)\n}\n")
        .expect("nand of bits[4] is a bit");
}

#[test]
fn nor_of_signed_is_e0403() {
    first_err(
        "module M {\n  in a: signed[4]\n  out y: bit\n  y = nor(a)\n}\n",
        "E0403",
    );
}

#[test]
fn max_with_a_literal_operand_adapts() {
    // A bare literal adapts to the sized side, like a comparison operand.
    check_one("module M {\n  in a: bits[8]\n  out y: bits[8]\n  y = max(a, 0)\n}\n")
        .expect("max(x, 0) adapts the literal to bits[8]");
}

#[test]
fn abs_of_a_literal_is_e0407() {
    first_err("module M {\n  out y: signed[4]\n  y = abs(3)\n}\n", "E0407");
}

#[test]
fn min_of_two_literals_is_e0407() {
    // Neither operand carries a width, so the result type is undefined.
    first_err(
        "module M {\n  out y: bits[8]\n  y = min(5, 10)\n}\n",
        "E0407",
    );
}

#[test]
fn nand_of_a_bare_bit_is_a_bit() {
    // A `bit` (not `bits[N]`) is a valid reduction operand — collapses to a bit.
    check_one("module M {\n  in a: bit\n  out y: bit\n  y = nand(a)\n}\n")
        .expect("nand of a bare bit is a bit");
}

#[test]
fn nested_abs_of_min_type_checks() {
    // min(signed[4], signed[4]) = signed[4]; abs(signed[4]) = signed[5].
    check_one(
        "module M {\n  in a: signed[4]\n  in b: signed[4]\n  out y: signed[5]\n  y = abs(min(a, b))\n}\n",
    )
    .expect("abs(min(a, b)) composes the type rules");
}

#[test]
fn min_of_two_abs_type_checks() {
    // abs(signed[4]) = signed[5] on both sides; min of equal widths = signed[5].
    check_one(
        "module M {\n  in x: signed[4]\n  in y: signed[4]\n  out z: signed[5]\n  z = min(abs(x), abs(y))\n}\n",
    )
    .expect("min(abs(x), abs(y)) composes the type rules");
}

#[test]
fn abs_grows_at_the_width_boundary() {
    // The largest abs that still fits: signed[127] → signed[128] (MAX_WIDTH).
    check_one("module M {\n  in a: signed[127]\n  out y: signed[128]\n  y = abs(a)\n}\n")
        .expect("abs(signed[127]) is signed[128]");
}

#[test]
fn bitwise_operand_mismatch_is_e0402() {
    let src = "module M {\n  in a: bits[4]\n  in b: bits[8]\n  out y: bits[8]\n  y = a & b\n}\n";
    let d = first_err(src, "E0402");
    assert!(d.help.unwrap().contains("extend"));
}

#[test]
fn wrapping_add_operand_mismatch_is_e0402() {
    let src = "module M {\n  in a: bits[4]\n  in b: bits[8]\n  out y: bits[8]\n  y = a +% b\n}\n";
    first_err(src, "E0402");
}

#[test]
fn signed_bits_mixing_is_e0403() {
    let src =
        "module M {\n  in a: bits[8]\n  in b: bits[8]\n  out y: bits[9]\n  y = signed(a) + b\n}\n";
    let d = first_err(src, "E0403");
    assert!(d.help.unwrap().contains("unsigned("));
}

#[test]
fn clock_in_a_data_expression_is_e0403() {
    let src = "module M {\n  clock clk\n  in x: bit\n  out y: bit\n  y = clk & x\n}\n";
    let d = first_err(src, "E0403");
    assert!(d.msg.contains("not data"));
}

#[test]
fn logical_and_on_a_bus_is_e0404() {
    let src = "module M {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bit\n  y = a && b\n}\n";
    let d = first_err(src, "E0404");
    assert!(
        d.help.unwrap().contains("!= 0"),
        "teaches how to make a bit"
    );
}

#[test]
fn literal_that_does_not_fit_is_e0405() {
    let d = first_err("module M {\n  out y: bits[4]\n  y = 300\n}\n", "E0405");
    assert!(d.msg.contains("300"));
    assert!(d.help.unwrap().contains("15"), "names the max that fits");
}

#[test]
fn negative_literal_in_unsigned_context_is_e0405() {
    let d = first_err("module M {\n  out y: bits[8]\n  y = -1\n}\n", "E0405");
    assert!(d.help.unwrap().contains("signed"));
}

#[test]
fn index_out_of_range_is_e0406() {
    let src = "module M {\n  in data: bits[8]\n  out y: bit\n  y = data[8]\n}\n";
    let d = first_err(src, "E0406");
    assert!(d.help.unwrap().contains("0..=7"));
}

#[test]
fn reversed_slice_is_e0406() {
    let src = "module M {\n  in data: bits[8]\n  out y: bits[4]\n  y = data[0:3]\n}\n";
    let d = first_err(src, "E0406");
    assert!(d.msg.contains("reversed"));
}

#[test]
fn extend_to_a_smaller_width_is_e0407() {
    let src = "module M {\n  in a: bits[8]\n  out y: bits[4]\n  y = extend(a, 4)\n}\n";
    let d = first_err(src, "E0407");
    assert!(d.help.unwrap().contains("trunc"));
}

#[test]
fn trunc_to_a_larger_width_is_e0407() {
    let src = "module M {\n  in a: bits[8]\n  out y: bits[16]\n  y = trunc(a, 16)\n}\n";
    let d = first_err(src, "E0407");
    assert!(d.help.unwrap().contains("extend"));
}

#[test]
fn negating_bits_is_e0407() {
    let src = "module M {\n  in a: bits[8]\n  out y: bits[9]\n  y = -a\n}\n";
    let d = first_err(src, "E0407");
    assert!(
        d.help.unwrap().contains("-%"),
        "teaches the wrap alternative"
    );
}

#[test]
fn if_arms_that_disagree_are_e0408() {
    let src = "module M {\n  in c: bit\n  in a: bits[4]\n  in b: bits[8]\n  out y: bit\n  y = (if c { a } else { b }) == a\n}\n";
    let d = first_err(src, "E0408");
    assert!(d.msg.contains("bits[4]") && d.msg.contains("bits[8]"));
}

#[test]
fn match_pattern_wider_than_scrutinee_is_e0409() {
    let src = "module M {\n  in op: bits[2]\n  in x: bit\n  out y: bit\n  y = match op {\n    0b100 => x\n    _ => x\n  }\n}\n";
    let d = first_err(src, "E0409");
    assert!(d.msg.contains("0b100"));
}

#[test]
fn match_on_signed_is_e0409() {
    let src = "module M {\n  in s: signed[4]\n  in x: bit\n  out y: bit\n  y = match s {\n    _ => x\n  }\n}\n";
    let d = first_err(src, "E0409");
    assert!(d.help.unwrap().contains("unsigned"));
}

#[test]
fn zero_width_is_e0410() {
    let d = first_err("module M {\n  out y: bits[0]\n  y = 0\n}\n", "E0410");
    assert!(d.help.unwrap().contains("at least one bit"));
}

#[test]
fn zero_width_output_with_indexed_drivers_does_not_panic() {
    // Regression (fuzz `lex_parse_compile`): a zero-width output — `!W` folds
    // to 0 — driven by per-bit `Range` sites reached the coverage check, where
    // `covered.len() as u128 - 1` underflowed on the empty vec. Must report
    // E0410, not panic.
    let src = "module M {\n  const W: int = 4\n  in a: bits[W]\n  out sum: bits[!W]\n  repeat i: 0..W {\n    sum[i] = a[i]\n  }\n}\n";
    first_err(src, "E0410");
}

#[test]
fn adder_growth_passes() {
    let src = "module Adder(WIDTH: int = 8) {\n  in a: bits[WIDTH]\n  in b: bits[WIDTH]\n  out sum: bits[WIDTH + 1]\n  sum = a + b\n}\n";
    check_one(src).expect("lossless + grows into the wider target");
}

#[test]
fn alu_match_arms_pass() {
    let src = "module Alu {\n  in a: bits[8]\n  in b: bits[8]\n  in op: bits[2]\n  out y: bits[8]\n  y = match op {\n    0b00 => a +% b\n    0b01 => a -% b\n    0b10 => a & b\n    _ => a | b\n  }\n}\n";
    check_one(src).expect("sized match arms against a sized target");
}

#[test]
fn enum_state_machine_passes() {
    let src = "module Fsm {\n  clock clk\n  reset rst\n  enum S { A, B }\n  reg state: S = S.A\n  reg timer: bits[8] = 0\n  out o: bit\n  on rise(clk) {\n    state <- match state {\n      S.A => S.B\n      S.B => S.A\n    }\n    timer <- match state {\n      S.A => 50\n      S.B => 0\n    }\n  }\n  o = state == S.B\n}\n";
    check_one(src).expect("enum regs, variant arms, literal arms that fit");
}

#[test]
fn register_file_passes() {
    // A `mem`: clocked indexed write under `we`, combinational indexed read.
    // No reset line needed — a memory power-on-inits itself.
    let src = "module RF {\n  clock clk\n  in we: bit\n  in waddr: bits[2]\n  in wdata: bits[8]\n  in raddr: bits[2]\n  out rdata: bits[8]\n  mem m: bits[8][4] = 0\n  on rise(clk) {\n    if we {\n      m[waddr] <- wdata\n    }\n  }\n  rdata = m[raddr]\n}\n";
    check_one(src).expect("a register file: indexed write + read, element-typed");
}

#[test]
fn a_non_constant_memory_depth_is_e0201() {
    let src = "module M {\n  in n: bits[4]\n  mem m: bits[8][n] = 0\n}\n";
    first_err(src, "E0201");
}

#[test]
fn a_zero_memory_depth_is_e0410() {
    let d = first_err("module M {\n  mem m: bits[8][0] = 0\n}\n", "E0410");
    assert!(d.msg.contains("depth"));
}

#[test]
fn a_memory_init_that_overflows_the_element_is_e0405() {
    first_err("module M {\n  mem m: bits[8][4] = 300\n}\n", "E0405");
}

#[test]
fn a_constant_address_past_the_depth_is_e0406() {
    let src = "module M {\n  out y: bits[8]\n  mem m: bits[8][4] = 0\n  y = m[4]\n}\n";
    let d = first_err(src, "E0406");
    assert!(d.msg.contains("address"));
}

#[test]
fn a_memory_inside_repeat_is_e0303() {
    let src = "module M {\n  repeat i: 0..2 {\n    mem m: bits[8][4] = 0\n  }\n}\n";
    first_err(src, "E0303");
}

#[test]
fn extend_of_a_bit_into_bitwise_passes() {
    let src = "module Sr(WIDTH: int = 8) {\n  clock clk\n  reset rst\n  in din: bit\n  out dout: bits[WIDTH]\n  reg sr: bits[WIDTH] = 0\n  on rise(clk) {\n    sr <- (sr << 1) | extend(din, WIDTH)\n  }\n  dout = sr\n}\n";
    check_one(src).expect("the shift-register shape, widths made explicit");
}

#[test]
fn comparison_with_a_const_passes() {
    let src = "const LIMIT: int = 50000000\nmodule Blink {\n  clock clk\n  reset rst\n  out led: bit\n  reg cnt: bits[26] = 0\n  reg state: bit = 0\n  on rise(clk) {\n    if cnt == LIMIT {\n      cnt <- 0\n      state <- state ^ 1\n    } else {\n      cnt <- cnt +% 1\n    }\n  }\n  led = state\n}\n";
    check_one(src).expect("consts adapt to the compared signal's width");
}

#[test]
fn defaultless_param_module_is_checked_per_instantiation() {
    let bad = "module C(W: int) {\n  in a: bits[W]\n  out z: bits[W]\n  z = a\n}\nmodule M {\n  in x: bits[8]\n  out y: bits[8]\n  let c = C(W: 4) { a: x }\n  y = c.z\n}\n";
    first_err(bad, "E0401");
    let good = "module C(W: int) {\n  in a: bits[W]\n  out z: bits[W]\n  z = a\n}\nmodule M {\n  in x: bits[8]\n  out y: bits[8]\n  let c = C(W: 8) { a: x }\n  y = c.z\n}\n";
    check_one(good).expect("the same module is clean under the right binding");
}

#[test]
fn repeat_index_out_of_range_at_the_last_iteration_is_e0406() {
    let src = "module M {\n  in data: bits[8]\n  out y: bits[9]\n  repeat i: 0..9 {\n    y[i] = data[i]\n  }\n}\n";
    let d = first_err(src, "E0406");
    assert!(
        d.msg.contains('8'),
        "the failing iteration's value is named"
    );
}

// ---- Pass 5: drivers (E0501–E0505) ------------------------------------

#[test]
fn driving_a_signal_twice_is_e0501() {
    let d = first_err(
        "module M {\n  in a: bit\n  out y: bit\n  y = a\n  y = a\n}\n",
        "E0501",
    );
    assert!(d.msg.contains("more than one driver"));
}

#[test]
fn driving_a_wire_after_its_declaration_is_e0501() {
    let src = "module M {\n  in a: bit\n  out y: bit\n  wire w: bit = a\n  w = a\n  y = w\n}\n";
    let d = first_err(src, "E0501");
    assert!(d.msg.contains("declaration"));
}

#[test]
fn overlapping_slice_drives_are_e0501() {
    let src = "module M {\n  in a: bits[4]\n  out y: bits[8]\n  y[3:0] = a\n  y[4:1] = a\n}\n";
    first_err(src, "E0501");
}

#[test]
fn an_undriven_output_is_e0502() {
    let src = "module M {\n  in a: bit\n  out y: bit\n  out z: bit\n  y = a\n}\n";
    let d = first_err(src, "E0502");
    assert!(d.msg.contains('z') && d.msg.contains("never driven"));
}

#[test]
fn a_partially_driven_output_is_e0502_naming_the_bit() {
    let src = "module M {\n  in a: bits[4]\n  out y: bits[8]\n  y[3:0] = a\n  y[6:4] = a[2:0]\n}\n";
    let d = first_err(src, "E0502");
    assert!(d.msg.contains('7'), "the undriven bit is named");
}

#[test]
fn a_reg_assigned_in_two_on_blocks_is_e0503() {
    let src = "module M {\n  clock clk\n  reset rst\n  out y: bit\n  reg v: bit = 0\n  on rise(clk) {\n    v <- 1\n  }\n  on rise(clk) {\n    v <- 0\n  }\n  y = v\n}\n";
    let d = first_err(src, "E0503");
    assert!(d.msg.contains("more than one"));
}

#[test]
fn a_reg_never_assigned_is_e0503() {
    let src = "module M {\n  clock clk\n  reset rst\n  out y: bit\n  reg v: bit = 0\n  y = v\n}\n";
    let d = first_err(src, "E0503");
    assert!(d.msg.contains("never assigned"));
}

#[test]
fn a_self_referential_wire_is_e0504() {
    let src = "module M {\n  out y: bit\n  wire w: bit = w\n  y = w\n}\n";
    let d = first_err(src, "E0504");
    assert!(d.msg.contains("w -> w"));
}

#[test]
fn a_two_wire_cycle_is_e0504_showing_the_path() {
    let src = "module M {\n  out y: bit\n  wire a: bit = b\n  wire b: bit = a\n  y = a\n}\n";
    let d = first_err(src, "E0504");
    assert!(d.msg.contains("->"), "path shown");
    assert!(d.help.unwrap().contains("reg"), "teaches the fix");
}

#[test]
fn a_cycle_through_instances_is_e0504() {
    let src = "module Inv {\n  in d: bit\n  out q: bit\n  q = !d\n}\nmodule M {\n  out y: bit\n  let i1 = Inv() { d: i2.q }\n  let i2 = Inv() { d: i1.q }\n  y = i1.q\n}\n";
    let d = first_err(src, "E0504");
    assert!(d.msg.contains("i1.q") && d.msg.contains("i2.q"));
}

#[test]
fn arrow_assignment_to_a_wire_is_e0505() {
    let src = "module M {\n  clock clk\n  in a: bit\n  out y: bit\n  wire w: bit = a\n  on rise(clk) {\n    w <- a\n  }\n  y = w\n}\n";
    let d = first_err(src, "E0505");
    assert!(d.help.unwrap().contains("registers"));
}

#[test]
fn combinational_drive_of_a_reg_is_e0505() {
    let src = "module M {\n  clock clk\n  reset rst\n  out y: bit\n  reg v: bit = 0\n  on rise(clk) {\n    v <- 1\n  }\n  v = 1\n  y = v\n}\n";
    let d = first_err(src, "E0505");
    assert!(d.help.unwrap().contains("<-"));
}

#[test]
fn disjoint_per_bit_drives_via_repeat_pass() {
    let src = "module M {\n  in data: bits[8]\n  out y: bits[8]\n  repeat i: 0..8 {\n    y[i] = data[i]\n  }\n}\n";
    check_one(src).expect("disjoint constant-index drives are one driver per bit");
}

#[test]
fn feedback_through_a_register_is_not_a_cycle() {
    let src = "module M {\n  clock clk\n  reset rst\n  out y: bit\n  reg v: bit = 0\n  wire next: bit = !v\n  on rise(clk) {\n    v <- next\n  }\n  y = v\n}\n";
    check_one(src).expect("a loop broken by a reg is the normal shape of hardware");
}

#[test]
fn repeat_instance_array_ripple_carry_is_not_a_cycle() {
    let src = "module FA {\n  in a: bit\n  in cin: bit\n  out s: bit\n  out cout: bit\n  s = a ^ cin\n  cout = a & cin\n}\nmodule M {\n  in x: bits[4]\n  out y: bits[4]\n  out c: bit\n  wire seed: bit = 0\n  repeat i: 0..4 {\n    let fa[i] = FA() { a: x[i], cin: if i == 0 { seed } else { fa[i-1].cout } }\n    y[i] = fa[i].s\n  }\n  c = fa[3].cout\n}\n";
    check_one(src).expect("fa[1] -> fa[0] is a chain, not a loop — per-index nodes");
}

#[test]
fn defaultless_module_with_param_indexed_drives_is_not_e0501() {
    let src = "module C(W: int) {\n  in a: bits[W]\n  out y: bits[W]\n  y[W - 1] = a[0]\n  y[0] = a[1]\n}\nmodule M {\n  in x: bits[2]\n  out y: bits[2]\n  let c = C(W: 2) { a: x }\n  y = c.y\n}\n";
    check_one(src).expect("unevaluable extents never conflict (no false positives)");
}

// ---- exhaustiveness (E0601/E0602) ----------------------------------------

#[test]
fn enum_match_covering_every_variant_needs_no_wildcard() {
    let src = "module M {\n  clock clk\n  reset rst\n  in go: bit\n  out y: bit\n  enum S { A, B, C }\n  reg s: S = S.A\n  on rise(clk) {\n    if go {\n      s <- S.B\n    }\n  }\n  y = match s {\n    S.A => 1\n    S.B => 0\n    S.C => 1\n  }\n}\n";
    check_one(src).expect("full variant coverage is exhaustive without `_` (v0.2.3 ruling)");
}

#[test]
fn enum_match_missing_a_variant_is_e0601_naming_it() {
    let src = "module M {\n  clock clk\n  reset rst\n  in go: bit\n  out y: bit\n  enum S { A, B, C }\n  reg s: S = S.A\n  on rise(clk) {\n    if go {\n      s <- S.B\n    }\n  }\n  y = match s {\n    S.A => 1\n    S.B => 0\n  }\n}\n";
    let d = first_err(src, "E0601");
    assert!(d.msg.contains("C"), "names the missing variant: {}", d.msg);
    assert!(d.help.unwrap().contains("_"));
}

#[test]
fn wildcard_after_full_enum_coverage_is_allowed() {
    let src = "module M {\n  clock clk\n  reset rst\n  in go: bit\n  out y: bit\n  enum S { A, B }\n  reg s: S = S.A\n  on rise(clk) {\n    if go {\n      s <- S.B\n    }\n  }\n  y = match s {\n    S.A => 1\n    S.B => 0\n    _ => 0\n  }\n}\n";
    check_one(src).expect("defensive `_` after full coverage is legal");
}

#[test]
fn duplicate_variant_pattern_is_e0602() {
    let src = "module M {\n  clock clk\n  reset rst\n  in go: bit\n  out y: bit\n  enum S { A, B }\n  reg s: S = S.A\n  on rise(clk) {\n    if go {\n      s <- S.B\n    }\n  }\n  y = match s {\n    S.A => 1\n    S.A => 0\n    _ => 0\n  }\n}\n";
    let d = first_err(src, "E0602");
    assert!(d.msg.contains("S.A"));
}

#[test]
fn arm_after_wildcard_is_e0602() {
    let src = "module M {\n  in sel: bits[2]\n  in a: bit\n  out y: bit\n  y = match sel {\n    _ => a\n    0 => a\n  }\n}\n";
    let d = first_err(src, "E0602");
    assert!(d.msg.contains("unreachable"));
}

#[test]
fn bits2_match_covering_all_four_values_passes() {
    let src = "module M {\n  in sel: bits[2]\n  in a: bit\n  in b: bit\n  out y: bit\n  y = match sel {\n    0 => a\n    1 => b\n    2 => a\n    3 => b\n  }\n}\n";
    check_one(src).expect("all 2^2 values covered — exhaustive without `_`");
}

#[test]
fn bits2_match_missing_a_value_is_e0601_naming_it() {
    let src = "module M {\n  in sel: bits[2]\n  in a: bit\n  in b: bit\n  out y: bit\n  y = match sel {\n    0 => a\n    1 => b\n    2 => a\n  }\n}\n";
    let d = first_err(src, "E0601");
    assert!(d.help.unwrap().contains('3'), "names the first gap");
}

#[test]
fn bit_match_missing_one_is_e0601() {
    let src =
        "module M {\n  in s: bit\n  in a: bit\n  out y: bit\n  y = match s {\n    0 => a\n  }\n}\n";
    let d = first_err(src, "E0601");
    assert!(d.help.unwrap().contains('1'));
}

#[test]
fn wide_match_without_wildcard_is_e0601() {
    let src = "module M {\n  in v: bits[8]\n  in a: bit\n  in b: bit\n  out y: bit\n  y = match v {\n    0 => a\n    1 => b\n  }\n}\n";
    let d = first_err(src, "E0601");
    assert!(d.msg.contains("bits[8]"));
}

#[test]
fn multi_pattern_arms_count_toward_coverage() {
    let src = "module M {\n  in sel: bits[2]\n  in a: bit\n  in b: bit\n  out y: bit\n  y = match sel {\n    0, 1 => a\n    2, 3 => b\n  }\n}\n";
    check_one(src).expect("`0, 1 =>` covers two values");
}

#[test]
fn duplicate_value_in_multi_pattern_arm_is_e0602() {
    let src = "module M {\n  in sel: bits[2]\n  in a: bit\n  out y: bit\n  y = match sel {\n    0, 0 => a\n    _ => a\n  }\n}\n";
    let d = first_err(src, "E0602");
    assert!(d.msg.contains("already covered"));
}

// ---- instantiation completeness (E0302) -----------------------------------

const FA2: &str = "module FA {\n  in a: bit\n  in b: bit\n  out s: bit\n  s = a ^ b\n}\n";

#[test]
fn unconnected_input_is_e0302_naming_it() {
    let src = format!(
        "{FA2}module M {{\n  in x: bit\n  out y: bit\n  let u = FA() {{ a: x }}\n  y = u.s\n}}\n"
    );
    let d = first_err(&src, "E0302");
    assert!(d.msg.contains('b'), "names the missing input: {}", d.msg);
}

#[test]
fn several_unconnected_inputs_are_listed_in_one_error() {
    let src = format!("{FA2}module M {{\n  out y: bit\n  let u = FA() {{}}\n  y = u.s\n}}\n");
    let d = first_err(&src, "E0302");
    assert!(d.msg.contains('a') && d.msg.contains('b'));
}

#[test]
fn clock_and_reset_ports_may_be_omitted() {
    let src = "module Tick {\n  clock clk\n  reset rst\n  out q: bit\n  reg v: bit = 0\n  on rise(clk) {\n    v <- !v\n  }\n  q = v\n}\nmodule M {\n  out y: bit\n  let u = Tick() {}\n  y = u.q\n}\n";
    check_one(src).expect("clock/reset connect implicitly by name — never E0302");
}

#[test]
fn connecting_an_input_twice_is_e0302() {
    let src = format!(
        "{FA2}module M {{\n  in x: bit\n  out y: bit\n  let u = FA() {{ a: x, a: x, b: x }}\n  y = u.s\n}}\n"
    );
    let d = first_err(&src, "E0302");
    assert!(d.msg.contains("twice"));
}

// ---- clock-domain ownership (E0701) ---------------------------------------

#[test]
fn two_clocks_with_separate_logic_pass() {
    let src = "module M {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out ya: bit\n  out yb: bit\n  reg ra: bit = 0\n  reg rb: bit = 0\n  on rise(cka) {\n    ra <- a\n  }\n  on rise(ckb) {\n    rb <- a\n  }\n  ya = ra\n  yb = rb\n}\n";
    check_one(src).expect("independent domains never touch — clean");
}

#[test]
fn reading_another_domains_reg_is_e0701() {
    let src = "module M {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out ya: bit\n  out yb: bit\n  reg ra: bit = 0\n  reg rb: bit = 0\n  on rise(cka) {\n    ra <- a\n  }\n  on rise(ckb) {\n    rb <- ra\n  }\n  ya = ra\n  yb = rb\n}\n";
    let d = first_err(src, "E0701");
    assert!(d.msg.contains("cka") && d.msg.contains("ckb"));
    assert!(d.help.unwrap().contains("sync"));
}

#[test]
fn cross_domain_through_a_wire_is_e0701() {
    let src = "module M {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out ya: bit\n  out yb: bit\n  reg ra: bit = 0\n  reg rb: bit = 0\n  wire w: bit = !ra\n  on rise(cka) {\n    ra <- a\n  }\n  on rise(ckb) {\n    rb <- w\n  }\n  ya = ra\n  yb = rb\n}\n";
    let d = first_err(src, "E0701");
    assert!(
        d.msg.contains("cka"),
        "the wire carries cka's domain: {}",
        d.msg
    );
}

#[test]
fn a_wire_mixing_two_domains_is_e0701() {
    let src = "module M {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out y: bit\n  reg ra: bit = 0\n  reg rb: bit = 0\n  wire w: bit = ra ^ rb\n  on rise(cka) {\n    ra <- a\n  }\n  on rise(ckb) {\n    rb <- a\n  }\n  y = w\n}\n";
    let d = first_err(src, "E0701");
    assert!(d.msg.contains("mixes"));
}

#[test]
fn same_domain_logic_under_two_declared_clocks_passes() {
    let src = "module M {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out y: bit\n  reg r1: bit = 0\n  reg r2: bit = 0\n  wire w: bit = r1 ^ r2\n  on rise(cka) {\n    r1 <- a\n    r2 <- w\n  }\n  y = w\n}\n";
    check_one(src).expect("everything lives on cka — the unused ckb changes nothing");
}

// ---- no declarations inside `repeat` (E0303) ------------------------------

/// True if any diagnostic carries `code` (E0303 may not be the FIRST error,
/// since a forbidden declaration also trips later passes).
fn any_code(src: &str, code: &str) -> bool {
    errs(src).iter().any(|d| d.code == Some(code))
}

#[test]
fn wire_inside_repeat_is_e0303() {
    let src = "module M {\n  out y: bits[4]\n  repeat i: 0..4 {\n    wire w: bit = 0\n    y[i] = w\n  }\n}\n";
    assert!(
        any_code(src, "E0303"),
        "a wire declared inside repeat is E0303"
    );
}

#[test]
fn reg_inside_repeat_is_e0303() {
    let src = "module M {\n  clock clk\n  reset rst\n  out y: bits[4]\n  repeat i: 0..4 {\n    reg r: bit = 0\n    y[i] = 0\n  }\n}\n";
    assert!(
        any_code(src, "E0303"),
        "a reg declared inside repeat is E0303"
    );
}

#[test]
fn on_block_inside_repeat_is_e0303() {
    let src = "module M {\n  clock clk\n  reset rst\n  out y: bits[4]\n  reg r: bit = 0\n  repeat i: 0..4 {\n    on rise(clk) {\n      r <- 1\n    }\n    y[i] = r\n  }\n}\n";
    assert!(
        any_code(src, "E0303"),
        "an `on` block inside repeat is E0303"
    );
}

#[test]
fn const_inside_repeat_is_e0303() {
    let src = "module M {\n  out y: bits[4]\n  repeat i: 0..4 {\n    const C: int = 1\n    y[i] = 0\n  }\n}\n";
    let d = errs(src)
        .into_iter()
        .find(|d| d.code == Some("E0303"))
        .expect("a const inside repeat is E0303");
    assert!(d.help.unwrap().contains("Declare the signal once outside"));
}

#[test]
fn repeat_with_only_drives_and_nested_repeat_is_clean() {
    // Drives and nested `repeat`s are the legal contents; each bit is
    // driven exactly once (i*2 + j covers 0..4).
    let src = "module M {\n  out y: bits[4]\n  repeat i: 0..2 {\n    repeat j: 0..2 {\n      y[i * 2 + j] = 0\n    }\n  }\n}\n";
    check_one(src).expect("a repeat that only generates hardware is clean");
}

// ---- user-defined functions: width (E0804) -----------------------------------

#[test]
fn fn_body_width_mismatch_is_e0804() {
    // Body returns bits[8], declared return is bits[4] — E0804.
    let d = first_err(
        "fn f(a: bits[8]) -> bits[4] { a }\nmodule M {\n  out o: bits[4]\n  o = f(0)\n}\n",
        "E0804",
    );
    assert!(
        d.msg.contains("bits[8]") && d.msg.contains("bits[4]"),
        "error names both widths: {}",
        d.msg
    );
}

#[test]
fn mac_function_type_checks_clean() {
    // mac: multiply-accumulate — body uses *% (same-width wrapping), return is bits[8].
    // Call site: mac(x, y) where x and y are bits[8]; result drives a bits[8] output.
    check_one(
        "fn mac(a: bits[8], b: bits[8]) -> bits[8] {\n  let prod = a *% b\n  prod\n}\nmodule M {\n  in x: bits[8]\n  in y: bits[8]\n  out z: bits[8]\n  z = mac(x, y)\n}\n",
    )
    .expect("mac body and call-site widths are clean, return bits[8] propagates");
}
