//! Task 7: compile-time `loop`/`foreach` unrolling.
//!
//! - `FnStmt::Loop` / `FnStmt::ForEach` (the latter desugars to the former
//!   via `ast::lower_foreach_fn`, reusing `value::fn_eval`'s own on-the-spot
//!   lowering trick — `fn` bodies have no pre-lowering pass) — statement
//!   splicing into `lower_fn_stmts`'s existing recursion.
//! - `SeqStmt::Loop` — same statement-splicing idea, but `SeqStmt` has no
//!   `Let`-equivalent binding statement, so the loop var is threaded through
//!   the `locals: Option<&HashMap<String, Bits>>` channel `lower_expr`
//!   already accepts, reusing the exact `fn`-body binding mechanism instead
//!   of a second, `expr_memo`-corrupting side-channel.
//! - `SeqStmt::ForEach` is NOT unrolled here — `elaborate_module`'s
//!   `ModuleItem::On` arm always rewrites it to `SeqStmt::Loop` before a
//!   `Process` exists, so `ir::lower` never sees a raw one; see the
//!   regression test at the bottom.

use crate::ir::{CellKind, exec::Executor};
use crate::value::Val;

fn v(bits: u128, width: u32) -> Val {
    Val::new(bits, width, false)
}

// --- FnStmt::Loop (Range form) -----------------------------------------

/// `fn_array_search.mimz`'s `find_index`: `loop i: 0..4 { if vals[i] ==
/// target { return i } }`, tail `-1`. Exercises the statement-splicing
/// Mux-tree-priority behavior directly — `Return`'s arm ignores `_rest`, so
/// an earlier iteration's `return` cuts off every later iteration's
/// contribution, giving first-match-wins on a duplicate.
#[test]
fn fn_loop_range_form_first_match_wins() {
    let src = "\
fn find_index(vals: bits[8][4], target: bits[8]) -> signed[4] {
  loop i: 0..4 {
    if vals[i] == target { return i }
  }
  -1
}

module FindIndex {
  in a: bits[8]
  in b: bits[8]
  in c: bits[8]
  in d: bits[8]
  in target: bits[8]
  out idx: signed[4]
  idx = find_index([a, b, c, d], target)
}
";
    let module = super::lower_valid(src);
    let mut exec = Executor::new(&module);

    // No match: falls through to the tail, -1.
    exec.set_input("a", v(1, 8));
    exec.set_input("b", v(2, 8));
    exec.set_input("c", v(3, 8));
    exec.set_input("d", v(4, 8));
    exec.set_input("target", v(9, 8));
    exec.tick();
    let idx = exec.get_output("idx");
    assert_eq!(
        idx.bits,
        crate::bits::Bits::Small(0xF),
        "-1 as signed[4] two's complement"
    );

    // `b` AND `d` both equal `target` — the LOWER index (1) must win, not
    // the last one found.
    exec.set_input("a", v(1, 8));
    exec.set_input("b", v(9, 8));
    exec.set_input("c", v(3, 8));
    exec.set_input("d", v(9, 8));
    exec.set_input("target", v(9, 8));
    exec.tick();
    let idx = exec.get_output("idx");
    assert_eq!(
        idx.bits,
        crate::bits::Bits::Small(1),
        "duplicate match: lower index (1) wins"
    );
}

// --- FnStmt::ForEach (Elements form) -> FnStmt::Loop --------------------

/// `foreach_sum.mimz`'s `sum8`: `foreach v in values { let acc = acc +%
/// extend(v, 11) }`. Exercises `FnStmt::ForEach` -> `lower_foreach_fn` ->
/// `FnStmt::Loop` with an array-typed source, pure `let`-accumulation, no
/// `return`.
#[test]
fn fn_foreach_elements_form_accumulates_array() {
    let src = "\
fn sum8(values: bits[8][8], acc: bits[11]) -> bits[11] {
  foreach v in values {
    let acc = acc +% extend(v, 11)
  }
  acc
}

module ForeachSum {
  in  a: bits[8]
  in  b: bits[8]
  in  c: bits[8]
  in  d: bits[8]
  in  e: bits[8]
  in  f: bits[8]
  in  g: bits[8]
  in  h: bits[8]
  out total: bits[11]

  total = sum8([a, b, c, d, e, f, g, h], 0)
}
";
    let module = super::lower_valid(src);
    let mut exec = Executor::new(&module);
    for (name, val) in [
        ("a", 1u128),
        ("b", 2),
        ("c", 3),
        ("d", 4),
        ("e", 5),
        ("f", 6),
        ("g", 7),
        ("h", 8),
    ] {
        exec.set_input(name, v(val, 8));
    }
    exec.tick();
    let total = exec.get_output("total");
    assert_eq!(total.bits, crate::bits::Bits::Small(36), "1+2+...+8 = 36");
    assert_eq!(total.width, 11);
}

// --- SeqStmt::Loop --------------------------------------------------------

/// Structural proof that `loop i: 0..4 { .. }` inside an `on`-block
/// actually unrolled into 4 copies: every iteration's own loop-var binding
/// is an UNCONDITIONAL `lower_const` call (regardless of whether the body
/// even reads `i`), so 4 distinct `Const` cells at the loop's own bound
/// width (`natural_width(hi-1)` = `natural_width(3)` = 2 bits here) with
/// values 0,1,2,3 is exactly "4 iterations ran", independent of body
/// content.
#[test]
fn seq_loop_unrolls_into_four_const_bindings() {
    let src = "\
module M {
  clock clk
  reset rst
  reg q: bits[4] = 0
  on rise(clk) {
    loop i: 0..4 {
      q <- q
    }
  }
}
";
    let module = super::lower_valid(src);
    let mut loop_var_consts: Vec<u128> = module
        .cells
        .iter()
        .filter_map(|c| match &c.kind {
            CellKind::Const { value } if value.width == 2 => match &value.bits {
                crate::bits::Bits::Small(v) => Some(*v),
                _ => None,
            },
            _ => None,
        })
        .collect();
    loop_var_consts.sort_unstable();
    loop_var_consts.dedup();
    assert_eq!(
        loop_var_consts,
        vec![0, 1, 2, 3],
        "expected one distinct 2-bit Const per iteration (0..4), got {loop_var_consts:?}"
    );
}

/// The loop var used as a genuine RUNTIME operand (not just a const-fold
/// index) against a sibling of a DIFFERENT width: `acc: bits[8]`, `i` bound
/// at `natural_width(3) = 2` bits. Before this task's `local_consts` fix,
/// `is_const_foldable(i)` was false (a bare `locals`-bound identifier is
/// invisible to it), so `acc +% i` never resized `i` to `acc`'s width — the
/// resulting `AddWrap` cell reached `ir::exec`'s `arith` with mismatched
/// `a`/`b` pin widths and hit its `debug_assert_eq!` outright. This proves
/// both that lowering no longer panics AND that the computed value is
/// correct (not just "didn't crash").
#[test]
fn seq_loop_var_resizes_against_a_wider_sibling() {
    let src = "\
module M {
  clock clk
  reset rst
  reg acc: bits[8] = 0
  on rise(clk) {
    loop i: 0..4 {
      acc <- acc +% i
    }
  }
}
";
    let module = super::lower_valid(src);
    // Last-write-wins across the 4 unrolled iterations (an `on`-block has
    // no notion of reading your own new value mid-block — every iteration's
    // RHS reads the SAME pre-edge `acc` Q value), so the D input this
    // synthesizes is just the final iteration's `acc_Q +% 3`.
    let dff = module
        .cells
        .iter()
        .find(|c| matches!(c.kind, CellKind::Dff { .. }))
        .expect("exactly one Dff for `acc`");
    assert_eq!(
        dff.pins["d"].width(),
        8,
        "AddWrap output stays at acc's own width"
    );

    let mut exec = Executor::new(&module);
    exec.set_input("rst", v(0, 1));
    exec.tick(); // acc_Q (post-reset) = 0, D = 0 +% 3 = 3
    let acc = exec.get_output("acc");
    assert_eq!(acc.bits, crate::bits::Bits::Small(3));
    assert_eq!(acc.width, 8);
}

// --- SeqStmt::ForEach: proven unreachable, not just unimplemented -------

/// Regression: an `on`-block `foreach` lowers successfully end-to-end. If
/// `elaborate_module`'s `lower_foreach_in_seq` pre-pass is doing its job,
/// `ir::lower` never sees a raw `SeqStmt::ForEach` and this just passes; if
/// that pre-pass is ever bypassed or a future refactor routes around it,
/// this panics on the `unreachable!()` in `lower_seq_stmts`'s `ForEach`
/// arm — exactly the regression this test exists to catch.
#[test]
fn seq_foreach_syntax_lowers_via_preexisting_unroll_pass() {
    // Range form (`foreach i in lo..hi`) — the Elements form
    // (`foreach v in values`) needs an array-typed SOURCE, and module-level
    // ports/registers can't be array-typed in v0.2 (E0416; array types are
    // `fn`-parameter-only), so it has no on-block-legal fixture. Range form
    // still exercises the exact thing this test cares about: a raw
    // `SeqStmt::ForEach` node reaching `elaborate_module`'s pre-lowering
    // pass and coming out the other side as `SeqStmt::Loop`, never reaching
    // `ir::lower`'s own `unreachable!()` arm.
    let src = "\
module M {
  clock clk
  reset rst
  reg q: bits[4] = 0
  on rise(clk) {
    foreach i in 0..4 {
      q[i] <- 1
    }
  }
}
";
    let module = super::lower_valid(src);
    assert!(
        module
            .cells
            .iter()
            .any(|c| matches!(c.kind, CellKind::Dff { .. })),
        "expected the q register's Dff to have lowered"
    );
}
