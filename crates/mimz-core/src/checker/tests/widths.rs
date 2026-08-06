use super::*;

// ---- GAP-6: assert's condition (E0404) -------------------------------

#[test]
fn assert_condition_must_be_a_single_bit() {
    let src = "module M {\n  in a: bits[4]\n  out y: bit\n  assert(a)\n  y = 0\n}\n";
    let d = first_err(src, "E0404");
    assert!(d.help.is_some());
}

#[test]
fn a_well_typed_assert_in_a_module_body_checks_clean() {
    let src = "module M {\n  in a: bit\n  out y: bit\n  assert(a)\n  y = a\n}\n";
    check_one(src).expect("a single-bit assert condition must pass");
}

#[test]
fn a_well_typed_assert_inside_an_on_block_checks_clean() {
    let src = "module M {\n  clock clk\n  reset rst\n  in a: bit\n  out y: bit\n  reg r: bit = 0\n  \
               on rise(clk) {\n    assert(a)\n    r <- a\n  }\n  y = r\n}\n";
    check_one(src).expect("a single-bit assert condition inside on rise(clk) must pass");
}

#[test]
fn cover_condition_must_be_a_single_bit() {
    let src = "module M {\n  in a: bits[4]\n  out y: bit\n  cover(a)\n  y = 0\n}\n";
    let d = first_err(src, "E0404");
    assert!(d.help.is_some());
}

#[test]
fn a_well_typed_cover_in_a_module_body_checks_clean() {
    let src = "module M {\n  in a: bit\n  out y: bit\n  cover(a)\n  y = a\n}\n";
    check_one(src).expect("a single-bit cover condition must pass");
}

#[test]
fn a_well_typed_cover_inside_an_on_block_checks_clean() {
    let src = "module M {\n  clock clk\n  reset rst\n  in a: bit\n  out y: bit\n  reg r: bit = 0\n  \
               on rise(clk) {\n    cover(a)\n    r <- a\n  }\n  y = r\n}\n";
    check_one(src).expect("a single-bit cover condition inside on rise(clk) must pass");
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
    assert!(
        d.help.as_ref().unwrap().contains("clocks and resets"),
        "clock/reset E0403 keeps its own help: {:?}",
        d.help
    );
}

#[test]
fn enum_in_concat_is_e0403_with_enum_specific_help() {
    // BUG-31: `not_data`'s help used to hardcode the clock/reset text even
    // when the operand was an enum (or any other non-clock/reset "not data"
    // type) — actively misdirecting a learner. The help must name the
    // actual problem, not clocks/resets.
    let src = "enum S { A, B }\n\
               module M {\n  in a: bit\n  in s: S\n  out y: bits[2]\n  y = { a, s }\n}\n";
    let d = first_err(src, "E0403");
    assert!(d.msg.contains("enum"), "message: {:?}", d.msg);
    let help = d.help.as_ref().unwrap();
    assert!(
        !help.contains("clocks and resets"),
        "enum E0403 must not reuse the clock/reset help: {help:?}"
    );
    assert!(
        help.contains("symbolic"),
        "enum E0403 help should explain enums are symbolic, not numeric: {help:?}"
    );
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
fn a_wide_literal_fits_a_wide_declared_width() {
    // A 200-bit signal reset to a 130-bit literal must check clean — this
    // was E0405 ("literal is too large") under the old i128 cap, even
    // though 130 bits is nowhere near the checker's own MAX_WIDTH for the
    // declared signal width (BUG-13 layer 2).
    let src = "module M {\n  reg r: bits[200] = 1361129467683753853853498429727072845824\n  out y: bit\n  y = 0\n}\n";
    if let Err(diags) = check_one(src) {
        assert!(
            diags.iter().all(|d| d.code != Some("E0405")),
            "expected no E0405, got: {diags:?}"
        );
    }
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
fn huge_slice_bound_that_would_wrap_u32_is_still_e0406() {
    // Regression: `slice_ty` narrows the const bit-position bounds to
    // `u32` before calling `width_rules::slice_result`. A raw `as u32`
    // cast wraps modulo 2^32, so `2^32` (4294967296) would wrap to `0` —
    // a well-in-range value — and silently accept a bound that must be
    // rejected. The narrowing must saturate instead of wrap.
    let src =
        "module M {\n  in data: bits[8]\n  out y: bit\n  y = data[4294967296:4294967296]\n}\n";
    let d = first_err(src, "E0406");
    assert!(
        d.msg.contains("4294967296"),
        "names the real, unwrapped bound"
    );
    assert!(d.help.unwrap().contains("0..=7"));
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
    // BUG-30 (`docs/audit/bugs.md`): `<<` now GROWS by its shift amount
    // (here, `sr << 1` is `bits[WIDTH + 1]`, not `bits[WIDTH]`), so the
    // classic shift-register idiom needs an explicit `trunc` to drop the
    // bit that falls off the top — same as a real shift register's own
    // hardware behavior, just spelled out instead of implicit.
    let src = "module Sr(WIDTH: int = 8) {\n  clock clk\n  reset rst\n  in din: bit\n  out dout: bits[WIDTH]\n  reg sr: bits[WIDTH] = 0\n  on rise(clk) {\n    sr <- trunc(sr << 1, WIDTH) | extend(din, WIDTH)\n  }\n  dout = sr\n}\n";
    check_one(src).expect("the shift-register shape, widths made explicit");
}

#[test]
fn shift_register_without_the_trunc_no_longer_matches_widths() {
    // BUG-30: `sr << 1` is now `bits[WIDTH + 1]`, one bit wider than
    // `extend(din, WIDTH)`'s `bits[WIDTH]` — the naive pre-fix idiom is a
    // real width mismatch now, not a silent truncation.
    let src = "module Sr(WIDTH: int = 8) {\n  clock clk\n  reset rst\n  in din: bit\n  out dout: bits[WIDTH]\n  reg sr: bits[WIDTH] = 0\n  on rise(clk) {\n    sr <- (sr << 1) | extend(din, WIDTH)\n  }\n  dout = sr\n}\n";
    first_err(src, "E0402");
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
