//! GAP-1 Task 6, fix round 4 — the WIDTH half of the compile-time-constant
//! context disease.
//!
//! Rounds 1-3 closed the SIGN axis: a `Ty::CtInt` carries no signedness of
//! its own, so every consuming site has to apply its context's. This file
//! covers the identical problem on the WIDTH axis — a constant also carries
//! no WIDTH of its own, and `lower_expr` sizes it at its natural/arithmetic
//! width unless the site explicitly re-sizes it through `lower_expr_sized`.
//!
//! Every test here is a real source-level program run through the whole
//! lex -> parse -> check -> elaborate -> lower pipeline, asserted at VALUE
//! level through `ir::exec::Executor` — `validate()` alone never caught this
//! class (a `Mux`'s `a`/`b` pins were unchecked), which is exactly why it
//! survived three review rounds.

use super::lower_valid;
use crate::bits::Bits as CBits;
use crate::ir::exec::Executor;
use crate::value::Val;

fn out_of(src: &str, name: &str) -> CBits {
    let module = lower_valid(src);
    let mut ex = Executor::new(&module);
    ex.tick();
    ex.get_output(name).bits
}

/// F1: a wire/output driven by a CONSTANT sizes to the declared width but
/// used to drop the declared SIGNEDNESS — `LowerCtx::resolve` read
/// `s.width.bits` and never `s.width.signed`, so a `signed[8]` wire holding
/// `-1` sign-extended as if it were `bits[8]`.
#[test]
fn a_constant_driven_signed_wire_keeps_its_declared_signedness() {
    let src = "module M {\n  wire w: signed[8] = -1\n  out e: signed[16]\n  e = extend(w, 16)\n}\n";
    assert_eq!(
        out_of(src, "e"),
        CBits::Small(0xFFFF),
        "a `signed[8]` wire driven by -1 must sign-extend to 0xFFFF"
    );
}

/// Same site, compound-constant driver (`0 - 1`) rather than a bare literal —
/// proves the fix is on the declared-type stamp, not on one `Expr` shape.
#[test]
fn a_compound_constant_driven_signed_wire_keeps_its_declared_signedness() {
    let src =
        "module M {\n  wire w: signed[8] = 0 - 1\n  out e: signed[16]\n  e = extend(w, 16)\n}\n";
    assert_eq!(out_of(src, "e"), CBits::Small(0xFFFF));
}

/// F2a: an `if`-expression's branches were lowered with plain `lower_expr`,
/// so a constant branch kept its own natural width. `validate`'s `Mux` rule
/// only checked `sel`, so this was SILENT: `if s { a } else { -1 }` produced
/// the raw 1-bit literal `1`.
#[test]
fn an_if_expressions_constant_branch_sizes_to_its_sibling() {
    let src = "module M {\n  in s: bit\n  in a: signed[8]\n  out o: signed[8]\n  o = if s { a } else { -1 }\n}\n";
    let module = lower_valid(src);
    let mut ex = Executor::new(&module);
    ex.set_input("s", Val::new(0, 1, false));
    ex.set_input("a", Val::new(7, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("o").bits,
        CBits::Small(0xFF),
        "the `-1` branch must be sized/signed to its sibling's signed[8]"
    );
}

/// F2b: the same for a `match` arm — `lower_match` folds through the same
/// `push_mux_cell`, with the same plain-`lower_expr` arms.
#[test]
fn a_match_arms_constant_value_sizes_to_its_sibling() {
    let src = "module M {\n  in s: bits[2]\n  in a: signed[8]\n  out o: signed[8]\n  o = match s {\n    0 => a\n    _ => -1\n  }\n}\n";
    let module = lower_valid(src);
    let mut ex = Executor::new(&module);
    ex.set_input("s", Val::new(1, 2, false));
    ex.set_input("a", Val::new(7, 8, true));
    ex.tick();
    assert_eq!(ex.get_output("o").bits, CBits::Small(0xFF));
}

/// F3a: a sequential assignment's RHS was never sized/signed to the
/// REGISTER's declared type.
#[test]
fn a_registers_constant_rhs_sizes_to_the_registers_declared_type() {
    let src = "module M {\n  clock clk\n  reset rst\n  in en: bit\n  out o: signed[16]\n  reg q: signed[8] = 0\n  on rise(clk) {\n    if en { q <- -1 }\n  }\n  o = extend(q, 16)\n}\n";
    let module = lower_valid(src);
    let mut ex = Executor::new(&module);
    ex.set_input("rst", Val::new(0, 1, false));
    ex.set_input("en", Val::new(1, 1, false));
    ex.tick();
    assert_eq!(
        ex.get_output("o").bits,
        CBits::Small(0xFFFF),
        "`q <- -1` into a signed[8] register must store 0xFF, not the raw 1-bit literal"
    );
}

/// F3b: the same for a MEMORY write's data — sized/signed to the element
/// type, not left at the literal's natural width.
#[test]
fn a_memory_writes_constant_data_sizes_to_the_element_type() {
    let src = "module M {\n  clock clk\n  in en: bit\n  out o: signed[16]\n  mem m: signed[8][4] = 0\n  on rise(clk) {\n    if en { m[0] <- -1 }\n  }\n  o = extend(m[0], 16)\n}\n";
    let module = lower_valid(src);
    let mut ex = Executor::new(&module);
    ex.set_input("en", Val::new(1, 1, false));
    ex.tick();
    assert_eq!(ex.get_output("o").bits, CBits::Small(0xFFFF));
}

/// F3c: a `default` statement's value is the same RHS position.
#[test]
fn a_seq_default_values_constant_sizes_to_the_registers_declared_type() {
    let src = "module M {\n  clock clk\n  reset rst\n  in en: bit\n  in d: signed[8]\n  out o: signed[16]\n  reg q: signed[8] = 0\n  on rise(clk) {\n    default q <- -1\n    if en { q <- d }\n  }\n  o = extend(q, 16)\n}\n";
    let module = lower_valid(src);
    let mut ex = Executor::new(&module);
    ex.set_input("rst", Val::new(0, 1, false));
    ex.set_input("en", Val::new(0, 1, false));
    ex.set_input("d", Val::new(0, 8, true));
    ex.tick();
    assert_eq!(ex.get_output("o").bits, CBits::Small(0xFFFF));
}

/// F4: a register's RESET value was `lower_const`'d at the folded
/// `ConstVal`'s own natural width (`elaborate::module`'s `const_eval_wide`
/// stores it un-resized), so `reg q: signed[8] = -1` reset to the 1-bit
/// constant `1`.
#[test]
fn a_registers_reset_value_sizes_to_the_registers_declared_type() {
    let src = "module M {\n  clock clk\n  reset rst\n  in d: signed[8]\n  out o: signed[16]\n  reg q: signed[8] = -1\n  on rise(clk) { q <- d }\n  o = extend(q, 16)\n}\n";
    let module = lower_valid(src);
    let mut ex = Executor::new(&module);
    ex.set_input("rst", Val::new(1, 1, false));
    ex.set_input("d", Val::new(0, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("o").bits,
        CBits::Small(0xFFFF),
        "the reset value must be resized to the register's signed[8]"
    );
}

/// F5a: `Mul`'s literal-resize special case only matched a BARE
/// `ExprKind::Int`, so a negated literal fell through and the `Mul` cell got
/// mismatched pins (a loud `PortWidthMismatch` at `validate`).
#[test]
fn mul_by_a_negated_literal_sizes_it_to_the_other_operand() {
    let src = "module M {\n  in a: signed[8]\n  out o: signed[16]\n  o = a * -1\n}\n";
    let module = lower_valid(src);
    let mut ex = Executor::new(&module);
    ex.set_input("a", Val::new(2, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("o").bits,
        CBits::Small(0xFFFE),
        "2 * -1 in signed[16] is -2"
    );
}

/// F5b: the same hole for a named `const` operand.
#[test]
fn mul_by_a_named_const_sizes_it_to_the_other_operand() {
    let src = "const K: int = 3\nmodule M {\n  in a: bits[8]\n  out o: bits[16]\n  o = a * K\n}\n";
    let module = lower_valid(src);
    let mut ex = Executor::new(&module);
    ex.set_input("a", Val::new(5, 8, false));
    ex.tick();
    assert_eq!(ex.get_output("o").bits, CBits::Small(15));
}

/// F6: the const-foldable resize was gated on `requires_matched_ab`, which
/// deliberately excludes the LOSSLESS ops, so a constant operand of `+`/`-`
/// never got sized/signed to its sibling at all — the checker's
/// `adapt_lossless` gives it exactly that type.
#[test]
fn a_lossless_ops_constant_operand_sizes_and_signs_to_its_sibling() {
    let src = "module M {\n  in a: signed[8]\n  out e: signed[16]\n  e = extend(a + (-3), 16)\n}\n";
    let module = lower_valid(src);
    let mut ex = Executor::new(&module);
    ex.set_input("a", Val::new(0, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("e").bits,
        CBits::Small(0xFFFD),
        "0 + (-3) is -3, and extend of a signed[9] must sign-extend it"
    );
}

/// F6b: the wrapping family (`+%`/`-%`/`*%`) is `matched_ty` at the checker
/// but was ALSO absent from `requires_matched_ab`'s resize gate, so a
/// negative constant operand wrapped at its own 1-2 bit natural width.
#[test]
fn a_wrapping_ops_constant_operand_sizes_and_signs_to_its_sibling() {
    let src = "module M {\n  in a: signed[8]\n  out o: signed[8]\n  o = a +% (-1)\n}\n";
    let module = lower_valid(src);
    let mut ex = Executor::new(&module);
    ex.set_input("a", Val::new(0, 8, true));
    ex.tick();
    assert_eq!(
        ex.get_output("o").bits,
        CBits::Small(0xFF),
        "0 +% -1 must wrap to -1, not add the raw 1-bit literal 1"
    );
}

/// Round-4 systematic-pass finding: a SLICE write's RHS was lowered at the
/// literal's natural width and then spliced into the base's nets by
/// `clone_from_slice`, which PANICS on a length mismatch — `q[7:4] <- 3`
/// (a 2-bit literal into a 4-bit slot) never lowered at all.
#[test]
fn a_slice_writes_constant_rhs_sizes_to_the_slice_width() {
    let src = "module M {\n  clock clk\n  reset rst\n  in en: bit\n  out o: bits[8]\n  reg q: bits[8] = 0\n  on rise(clk) {\n    if en { q[7:4] <- 3 }\n  }\n  o = q\n}\n";
    let module = lower_valid(src);
    let mut ex = Executor::new(&module);
    ex.set_input("rst", Val::new(0, 1, false));
    ex.set_input("en", Val::new(1, 1, false));
    ex.tick();
    assert_eq!(ex.get_output("o").bits, CBits::Small(0x30));
}

/// Round-4 systematic-pass probe: a `fn`-body `let` bound to a bare
/// constant. `LocalLet` carries no type annotation, so the checker types
/// `m` as `Ty::CtInt` and lets it adapt to `v` in `v & m` — but in the IR
/// `m` is an already-lowered `Bits`, not an `Expr`, so `is_const_foldable`
/// cannot see through it and the resize never fires.
#[test]
fn a_fn_local_let_bound_to_a_constant_adapts_to_its_use_site() {
    let src = "fn mask(v: bits[8]) -> bits[8] {\n  let m = 1\n  v & m\n}\nmodule M {\n  in a: bits[8]\n  out y: bits[8]\n  y = mask(a)\n}\n";
    let module = lower_valid(src);
    let mut ex = Executor::new(&module);
    ex.set_input("a", Val::new(0xFF, 8, false));
    ex.tick();
    assert_eq!(ex.get_output("y").bits, CBits::Small(1));
}

/// Round-4 systematic-pass finding: a memory write's ADDRESS is the same
/// constant-context position as its data — it must be sized to the memory's
/// own address width, not the literal's natural width, or the `Mem` cell's
/// `waddr` pin disagrees with its `raddr` pins.
#[test]
fn a_memory_writes_constant_address_sizes_to_the_address_width() {
    let src = "module M {\n  clock clk\n  in en: bit\n  out o: bits[8]\n  mem m: bits[8][4] = 0\n  on rise(clk) {\n    if en { m[1] <- 9 }\n  }\n  o = m[1]\n}\n";
    let module = lower_valid(src);
    let mem = module
        .cells
        .iter()
        .find(|c| matches!(c.kind, crate::ir::CellKind::Mem { .. }))
        .expect("one Mem cell");
    assert_eq!(
        mem.pins["waddr"].width(),
        2,
        "waddr must be the memory's clog2(depth) address width"
    );
    let mut ex = Executor::new(&module);
    ex.set_input("en", Val::new(1, 1, false));
    ex.tick();
    assert_eq!(ex.get_output("o").bits, CBits::Small(9));
}
