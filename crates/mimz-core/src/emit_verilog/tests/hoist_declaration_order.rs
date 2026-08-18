//! Round-7 plan Task 1 (GAP-18, `docs/audit/gaps.md`) — the hoist buffer's
//! flush point (`hoist_pos`) is a second scoping axis alongside
//! `hoist_unresolved`'s "which `decls` is in scope" (GAP-16): a hoisted
//! wire can resolve its `Kind` correctly and still land after its own use
//! if the render site that asked for it runs before the buffer is
//! flushed. `assert_hoists_declared_before_use` (`emit_verilog/mod.rs`)
//! is the runtime invariant that watches this; these tests pin its own
//! logic and prove it holds on BUG-66's own repro after Task 3's fix
//! (`tests/self_determined_regression.rs` carries the same repro's real
//! Icarus differentials).

use super::*;

#[test]
fn finds_both_hoist_name_families_and_ignores_lookalikes() {
    let line = "    Sub u (.d({b, __mimz_sub_1}), .q(__mimz_fn_sub_12));";
    assert_eq!(
        hoisted_names_in(line),
        vec!["__mimz_sub_1".to_string(), "__mimz_fn_sub_12".to_string()]
    );
    // A longer identifier that merely CONTAINS the prefix must not match —
    // no false positive from a user-chosen name that happens to embed it.
    assert!(hoisted_names_in("wire [7:0] user__mimz_sub_1x;").is_empty());
}

#[test]
fn task3_bug_66_a2_reg_reset_hoist_no_longer_fires_the_declaration_order_assert() {
    // BUG-66 A2 (docs/audit/review-2026-08-17.md Part 3.1): a `reg`'s own
    // reset value used to render BEFORE `hoist_pos` was captured
    // (`module/mod.rs`), so a composite reset needing a hoist declared its
    // wire AFTER the `initial r = ...;` line that already used it — this
    // exact source used to panic `assert_hoists_declared_before_use`
    // before Task 3's fix. Proves the fix in-process (fast, no `iverilog`
    // needed) that the earlier round of this test caught the bug with;
    // `tests/self_determined_regression.rs`'s `bug_66_*` differentials
    // additionally confirm real Icarus elaborates and computes the same
    // value.
    let v = emit_src(
        "module M {\n  in clk: bit\n  in a: bits[4]\n  in b: bits[4]\n  \
         reg r: bits[12] = { b, extend(a, 8) }\n  \
         on rise(clk) {\n    r <- { b, extend(a, 8) }\n  }\n}\n",
    );
    let wire_at = v
        .find("wire [7:0] __mimz_sub_1;")
        .expect("hoisted wire missing");
    let use_at = v
        .find("initial #0 r =")
        .expect("reg initial-seed line missing");
    assert!(
        wire_at < use_at,
        "hoisted wire must be declared before the `initial` line that uses it:\n{v}"
    );
}
