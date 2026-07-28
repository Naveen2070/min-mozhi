use super::*;

#[test]
fn extern_instantiation_emits_only_the_instance_line_no_definition() {
    let v = emit_src(
        "extern module Pll(MULT: int = 2) {\n  \
         in clk_in: bit\n  out clk_out: bit\n  out locked: bit\n}\n\
         module M {\n  clock sysclk\n  out fast: bit\n  out ok: bit\n  \
         let u = Pll(MULT: 4) { clk_in: sysclk, clk_out: fast, locked: ok }\n}\n",
    );
    assert!(
        v.contains("Pll #(.MULT(4)) u ("),
        "expected an instantiation of the real name `Pll`, got:\n{v}"
    );
    assert!(
        !v.contains("module Pll"),
        "extern modules must never get their own definition emitted, got:\n{v}"
    );
}

#[test]
fn extern_instantiation_uses_the_alias_when_set() {
    let v = emit_src(
        "extern module Pll = \"PLL_HARD_IP_v2\" {\n  in clk_in: bit\n}\n\
         module M {\n  clock sysclk\n  let u = Pll() { clk_in: sysclk }\n}\n",
    );
    assert!(
        v.contains("PLL_HARD_IP_v2"),
        "expected the aliased real name in the instantiation, got:\n{v}"
    );
    assert!(
        !v.contains("Pll ("),
        "the Min-Mozhi-facing name must not leak into the instantiation text (only the alias should), got:\n{v}"
    );
}

#[test]
fn zero_length_array_param_runtime_index_is_a_clean_diag_not_a_panic() {
    // Regression: the same root cause as sim's `src/sim/comb.rs`
    // `zero_length_array_param_index_is_a_clean_err_not_a_panic` — a
    // zero-length array param is rejected by the checker's E0412 in
    // the normal `mimz compile` pipeline, but this emitter is also
    // exercised directly on unchecked ASTs (fuzzing). A RUNTIME index
    // (not const-foldable, so it reaches the ternary-chain builder)
    // used to underflow `len - 1` when computing the chain's default
    // (last-element) arm.
    let diags = emit_src_err(
        "fn first(vals: bits[8][0], i: bits[8]) -> bits[8] {\n  vals[i]\n}\n\nmodule M {\n  in a: bits[8]\n  out y: bits[8]\n  y = first(a, a)\n}\n",
    );
    assert!(
        diags.iter().any(|d| d.msg.contains("no elements to index")),
        "got: {diags:?}"
    );
}
