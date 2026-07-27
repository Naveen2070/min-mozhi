use super::*;

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
fn wide_match_with_a_past_128_bit_pattern_is_e0601_not_a_panic() {
    // BUG-13 layer 2 regression: a `bits[200]` scrutinee can hold a
    // `Bits::Wide` pattern value; the exhaustiveness scan must not assume
    // every `seen` entry is `Bits::Small` just because it's non-exhaustive.
    let src = "module M {\n  in v: bits[200]\n  in a: bit\n  out y: bit\n  y = match v {\n    1361129467683753853853498429727072845824 => a\n  }\n}\n";
    let d = first_err(src, "E0601");
    assert!(d.msg.contains("bits[200]"));
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
