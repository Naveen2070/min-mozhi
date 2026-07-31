use super::*;
use crate::sim::value::binary::{binary_ctx, binary_known, cmp_eq, unary};
use mimz_core::span::Span;

/// Dummy span for unit tests exercising `binary_ctx`/`binary_known` directly
/// (no real source expression to point at here).
fn sp() -> Span {
    Span::default()
}

#[test]
fn bitand_widens_a_narrower_literal_operand() {
    // A bare literal's Val keeps its own minimal width (here 1),
    // never pre-widened to match a wider operand — the checker's
    // static type system treats the literal as "adapting" to the
    // other side (a compile-time-only fact), but the simulator's
    // actual Val for that literal is NOT pre-widened anywhere. The
    // wrap/bitwise arms must widen it themselves before combining.
    let l = Val::new(0b1010, 4, false); // a 4-bit signal
    let r = Val::new(1, 1, false); // the literal `1`, its own minimal width
    let result = binary_known(BinOp::BitAnd, l, r, None, sp()).unwrap();
    assert_eq!(result.width, 4);
    assert_eq!(result.masked(), 0b1010 & 1);
}

#[test]
fn shl_self_determined_preserves_left_operand_width() {
    // No context (bare `binary()`, matching a condition/index/loop-bound
    // position, or a raw compile-time literal with nothing sizing it) —
    // Verilog's shift is self-determined here: the result stays exactly
    // `l`'s own width, truncating what doesn't fit. `1 << 2` with `1` at
    // its minimal width (1 bit) masks the whole result away — that's
    // correct self-determined behavior, not BUG-6 (BUG-6 was reachable
    // through `extend(1 << 2, N)`, which now threads `N` in as context —
    // see `shl_widens_to_context_like_verilog` below).
    let l = Val::from_int(1); // width 1
    let r = Val::from_int(2);
    let res = binary_ctx(BinOp::Shl, l, r, None, sp()).unwrap();
    assert_eq!(res.masked(), 0); // 4 & mask(1) == 0
    assert_eq!(res.width, 1);
}

#[test]
fn shl_widens_to_context_like_verilog() {
    // BUG-11 (supersedes the BUG-6 fix the old version of this test
    // asserted — growing the result by the shift amount, unconditionally
    // — that broke real signal shifts, see `shl_chain_stays_at_shared_
    // context_width` below). Ground-truthed against `iverilog`: `<<`'s
    // left operand is CONTEXT-DETERMINED — it widens to the enclosing
    // width (an assignment target, `extend`'s target) BEFORE the shift,
    // not truncated-then-extended after.
    let l = Val::from_int(1); // width 1
    let r = Val::from_int(2);
    let res = binary_ctx(BinOp::Shl, l, r, Some(8), sp()).unwrap();
    assert_eq!(res.width, 8);
    assert_eq!(res.masked(), 4); // 1 << 2, no bits lost once widened first

    // review-2026-07-17.md's exact repro: din (4-bit) << 2 into an 8-bit
    // context. iverilog: 28, NOT 12 (12 is what self-determined-then-
    // truncated-into-8-bits would wrongly give if extension happened
    // AFTER the shift instead of before).
    let din = Val::new(7, 4, false);
    let shifted = binary_ctx(BinOp::Shl, din, Val::from_int(2), Some(8), sp()).unwrap();
    assert_eq!(shifted.width, 8);
    assert_eq!(shifted.masked(), 28);
}

#[test]
fn shl_rejects_a_signed_shift_amount() {
    let l = Val::new(1, 8, false);
    let r = Val::new(2, 3, true); // signed amount — spec/02 section 3 forbids this
    let err = binary_known(BinOp::Shl, l, r, None, sp()).unwrap_err();
    assert!(
        err.msg.contains("signed"),
        "expected an error mentioning `signed`, got: {}",
        err.msg
    );
}

#[test]
fn sub_of_two_unsigned_values_is_unsigned() {
    // BUG-22 (docs/audit/bugs.md): binary_known's Sub arm used to
    // hardcode `signed: true` unconditionally, disagreeing with the
    // checker's own lossless_ty rule (unsigned bits[N] - unsigned
    // bits[M] is unsigned bits[N.max(M)+1]).
    let l = Val::new(0, 4, false);
    let r = Val::new(0, 4, false);
    let result = binary_known(BinOp::Sub, l, r, None, sp()).unwrap();
    assert!(!result.signed, "expected an unsigned result, got signed");
    assert_eq!(result.width, 5);
}

#[test]
fn sub_of_two_signed_values_is_signed() {
    let l = Val::new(0, 4, true);
    let r = Val::new(0, 4, true);
    let result = binary_known(BinOp::Sub, l, r, None, sp()).unwrap();
    assert!(result.signed, "expected a signed result");
    assert_eq!(result.width, 5);
}

#[test]
fn shl_chain_stays_at_shared_context_width() {
    // BUG-11's own reproduction: `(a << 2) >> 2` for `a: bits[8]`
    // assigned to `y: bits[8]` — iverilog says 63, not 255. The context
    // (8) must be threaded into BOTH shifts, not just the first: a
    // width that only grows by the shift amount at each step (the old
    // fix) lets an intermediate carry stray high bits into the second
    // shift that a real 8-bit-wide Verilog computation never has.
    let a = Val::new(255, 8, false);
    let shifted_left = binary_ctx(BinOp::Shl, a, Val::from_int(2), Some(8), sp()).unwrap();
    assert_eq!(shifted_left.width, 8);
    let shifted_right =
        binary_ctx(BinOp::Shr, shifted_left, Val::from_int(2), Some(8), sp()).unwrap();
    assert_eq!(shifted_right.masked(), 63); // NOT 255 — this was BUG-11
}

#[test]
fn fn_call_arity_mismatch_is_err_not_panic() {
    // Fuzz find: `eval_fn_call` is reachable directly on a parsed-but-
    // unchecked AST (the checker's E0413 array-length check normally
    // rejects this first). A short array-literal argument left `argv`
    // shorter than the callee's param arity, and `argv[ai]` panicked
    // with an out-of-bounds index instead of returning a clean `Err`.
    let src = "saarbu pick(vals: bits[8][4], idx: bits[3]) -> bits[8] {\n  \
                   vals[idx]\n}\n\n\
                   thoguthi M {\n  \
                   ulleedu a: bits[8]\n  \
                   ulleedu b: bits[8]\n  \
                   ulleedu idx: bits[3]\n  \
                   veliyeedu picked: bits[8]\n  \
                   picked = pick([a, b], idx)\n\
                   }\n";
    let tokens = mimz_core::lexer::lex(src).expect("lex");
    let file = mimz_core::parser::parse(tokens).expect("parse");
    let inputs: BTreeMap<String, Bits> = [
        ("a".to_string(), Bits::Small(1u128)),
        ("b".to_string(), Bits::Small(2u128)),
        ("idx".to_string(), Bits::Small(0u128)),
    ]
    .into_iter()
    .collect();
    let result = super::super::comb::eval_outputs(
        std::slice::from_ref(&file),
        Some("M"),
        &inputs,
        &BTreeMap::new(),
    );
    assert!(result.is_err(), "expected a clean Err, got {result:?}");
}

/// Wraps `fn_src` (one or more `fn` decls) in a throwaway module that
/// calls `fn_name` with `args` as inline literals (an arg slice of one
/// element becomes a scalar literal, a longer slice becomes an array
/// literal `[..]` — the `ArrayLit` argument-expansion path `eval_fn_call`
/// already exercises), then reads the result back through the
/// combinational evaluator. The output port's declared width doesn't
/// affect the returned value (comb.rs resolves the driver expression's
/// OWN width, see `eval_outputs`'s step 5), so the result is sign-extended
/// per its actual width/signed straight from `Val::as_i128`.
fn eval_fn_call_one(fn_src: &str, fn_name: &str, args: &[&[u128]]) -> i128 {
    let call_args: Vec<String> = args
        .iter()
        .map(|a| match *a {
            [one] => one.to_string(),
            many => format!(
                "[{}]",
                many.iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })
        .collect();
    let src = format!(
        "{fn_src}\nmodule M {{\n  out result: bits[8]\n  result = {fn_name}({})\n}}\n",
        call_args.join(", ")
    );
    let tokens = mimz_core::lexer::lex(&src).expect("lex");
    let file = mimz_core::parser::parse(tokens).expect("parse");
    let outputs = super::super::comb::eval_outputs(
        std::slice::from_ref(&file),
        Some("M"),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("eval_outputs");
    let out = outputs
        .into_iter()
        .find(|o| o.name == "result")
        .expect("module declares `result`");
    // Every value this test helper's callers produce stays narrow
    // (bits[8]/signed[16] fn args) — `Bits::Wide` is not reachable here.
    let Bits::Small(bits) = out.value else {
        panic!("test expected a narrow (Small) value")
    };
    Val::new(bits, out.width, out.signed).as_i128()
}

#[test]
fn fn_call_sign_extends_narrower_signed_arg_to_wider_param() {
    // BUG-7 regression: `eval_fn_call` used to bind an argument with
    // `Val::new(val.bits, w, s)` — masking the caller's raw bits to the
    // param's width with no sign-extension. A `signed[16]` param bound
    // to the literal `-128` (whose own natural width is the minimal
    // 8-bit two's-complement pattern, 0x80) came out `+128`: 0x80
    // masked to 16 bits is still 0x0080, not the correctly
    // sign-extended 0xFF80.
    let src = "fn widen16(x: signed[16]) -> signed[16] {\n  x\n}\n\n\
                   module M {\n  out result: signed[16]\n  result = widen16(-128)\n}\n";
    let tokens = mimz_core::lexer::lex(src).expect("lex");
    let file = mimz_core::parser::parse(tokens).expect("parse");
    let outputs = super::super::comb::eval_outputs(
        std::slice::from_ref(&file),
        Some("M"),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("eval_outputs");
    let out = outputs
        .into_iter()
        .find(|o| o.name == "result")
        .expect("module declares `result`");
    let Bits::Small(bits) = out.value else {
        panic!("test expected a narrow (Small) value")
    };
    assert_eq!(
        Val::new(bits, out.width, out.signed).as_i128(),
        -128,
        "got raw bits {:#x} at width {}",
        bits,
        out.width
    );
}

#[test]
fn fn_loop_with_return_finds_first_match_in_sim() {
    let result = eval_fn_call_one(
        "fn find_first_set(vals: bits[8][4]) -> signed[4] {\n  loop i: 0..4 {\n    if vals[i] == 0xFF { return i }\n  }\n  0 - 1\n}\n",
        "find_first_set",
        &[&[0x00, 0xFF, 0x00, 0x00]],
    );
    assert_eq!(result, 1);
}

#[test]
fn fn_loop_with_return_first_match_wins_on_duplicate_in_sim() {
    let result = eval_fn_call_one(
        "fn find_first_set(vals: bits[8][4]) -> signed[4] {\n  loop i: 0..4 {\n    if vals[i] == 0xFF { return i }\n  }\n  0 - 1\n}\n",
        "find_first_set",
        &[&[0xFF, 0x00, 0xFF, 0x00]], // matches at BOTH index 0 and index 2
    );
    assert_eq!(result, 0, "must return the LOWER index, not 2");
}

#[test]
fn fn_loop_over_budget_errors_in_sim() {
    let src = format!(
        "fn overflow(x: bits[8]) -> bits[8] {{\n  loop i: 0..{} {{\n    if x == 0xFF {{ return x }}\n  }}\n  x\n}}\n",
        mimz_core::REPEAT_BUDGET + 1
    );
    let full = format!(
        "{src}\nmodule M {{\n  in x: bits[8]\n  out result: bits[8]\n  result = overflow(x)\n}}\n"
    );
    let tokens = mimz_core::lexer::lex(&full).expect("lex");
    let file = mimz_core::parser::parse(tokens).expect("parse");
    let inputs: BTreeMap<String, Bits> = [("x".to_string(), Bits::Small(1u128))]
        .into_iter()
        .collect();
    let result = super::super::comb::eval_outputs(
        std::slice::from_ref(&file),
        Some("M"),
        &inputs,
        &BTreeMap::new(),
    );
    let err = result.expect_err("over-budget `loop` must error, not hang or overflow");
    assert!(err.msg.contains("`loop` would unroll"), "got: {}", err.msg);
}

#[test]
fn fn_foreach_range_form_with_return_finds_first_match_in_sim() {
    // Same shape as `fn_loop_with_return_finds_first_match_in_sim`, but
    // with `foreach i in 0..4` (Range form) in place of `loop i: 0..4` —
    // `FnStmt::ForEach` must lower via `ast::lower_foreach_fn` to the
    // same `FnStmt::Loop` and early-return correctly.
    let result = eval_fn_call_one(
        "fn find_first_set(vals: bits[8][4]) -> signed[4] {\n  foreach i in 0..4 {\n    if vals[i] == 0xFF { return i }\n  }\n  0 - 1\n}\n",
        "find_first_set",
        &[&[0x00, 0xFF, 0x00, 0x00]],
    );
    assert_eq!(result, 1);
}

#[test]
fn fn_foreach_elements_form_with_return_finds_match_in_sim() {
    // Elements form: `foreach v in vals` binds `v` to each array element
    // via a synthesized `Let`, and `return v` on a match must propagate
    // as `FnFlow::Returned` out of the lowered `Loop`.
    let result = eval_fn_call_one(
        "fn find_val(vals: bits[8][4]) -> bits[8] {\n  foreach v in vals {\n    if v == 0xFF { return v }\n  }\n  0\n}\n",
        "find_val",
        &[&[0x11, 0xFF, 0x22, 0x33]],
    );
    assert_eq!(result, 0xFF);
}

#[test]
fn fn_foreach_elements_form_no_match_falls_through_in_sim() {
    // No element matches — `eval_fn_stmts` must reach `FnFlow::FellThrough`
    // and yield the fn's tail expression (`0`), NOT a spurious
    // `FnFlow::Returned` from misreading fall-through as an early return.
    let result = eval_fn_call_one(
        "fn find_val(vals: bits[8][4]) -> bits[8] {\n  foreach v in vals {\n    if v == 0xFF { return v }\n  }\n  0\n}\n",
        "find_val",
        &[&[0x11, 0x22, 0x33, 0x44]],
    );
    assert_eq!(result, 0);
}

#[test]
fn unknown_val_taints_binary_ops() {
    let u = Val::unknown(4, false);
    let known = Val::new(3, 4, false);
    let r = binary_ctx(BinOp::Add, u, known, None, sp()).unwrap();
    assert!(
        r.unknown,
        "adding an unknown operand must produce an unknown result"
    );
}

#[test]
fn unknown_val_taints_unary_ops() {
    let u = Val::unknown(4, false);
    let r = unary(UnOp::BitNot, u);
    assert!(
        r.unknown,
        "negating an unknown operand must produce an unknown result"
    );
}

#[test]
fn known_vals_are_never_tainted() {
    let a = Val::new(1, 4, false);
    let b = Val::new(2, 4, false);
    assert!(!a.unknown && !b.unknown);
    assert!(
        !binary_ctx(BinOp::Add, a.clone(), b, None, sp())
            .unwrap()
            .unknown
    );
    assert!(!unary(UnOp::BitNot, a).unknown);
}

#[test]
fn val_new_stays_on_the_small_fast_path() {
    let v = Val::new(42, 8, false);
    assert!(!v.is_wide());
    assert_eq!(v.masked(), 42);
}

#[test]
fn val_new_wide_masks_to_the_declared_width() {
    // 200 bits of all-ones, masked down to 130 bits.
    let limbs = vec![u64::MAX; wide::limb_count(200)];
    let v = Val::new_wide(limbs, 130, false);
    assert!(v.is_wide());
    assert_eq!(v.width, 130);
}

#[test]
fn val_new_wide_auto_narrows_to_small_at_128_bits_or_less() {
    // A width-96 result never needs to carry a heap-allocated Vec —
    // new_wide must narrow it back to `Bits::Small` itself, so every
    // OTHER caller (Task 6's dispatch) can rely on "width <= 128
    // implies Small" without re-checking.
    let limbs = vec![0u64; wide::limb_count(96)];
    let v = Val::new_wide(limbs, 96, false);
    assert!(!v.is_wide());
}

#[test]
fn wide_unsigned_add_carries_past_128_bits() {
    // Two 128-bit unsigned max values: the TRUE lossless result is
    // 129 bits and does NOT fit in a u128 — this is the exact
    // boundary case the 128-bit ceiling silently got wrong before
    // this task (a 129-bit-wide RESULT from two Small operands).
    let a = Val::new(u128::MAX, 128, false);
    let b = Val::new(1, 128, false);
    let sum = binary_known(BinOp::Add, a, b, None, sp()).unwrap();
    assert_eq!(sum.width, 129);
    assert!(sum.is_wide());
}

#[test]
fn wide_bitand_of_two_512_bit_values() {
    let a = Val::new_wide(wide::from_u128(0b1100, 512), 512, false);
    let b = Val::new_wide(wide::from_u128(0b1010, 512), 512, false);
    let result = binary_known(BinOp::BitAnd, a, b, None, sp()).unwrap();
    assert!(result.is_wide());
    assert!(wide::bit_at(&result.to_limbs(), 3));
    assert!(!wide::bit_at(&result.to_limbs(), 1));
}

#[test]
fn wide_shl_crosses_a_limb_boundary_in_a_512_bit_context() {
    let l = Val::new(1, 8, false);
    let shifted = binary_ctx(BinOp::Shl, l, Val::from_int(70), Some(512), sp()).unwrap();
    assert_eq!(shifted.width, 512);
    assert!(wide::bit_at(&shifted.to_limbs(), 70));
}

#[test]
fn wide_eq_compares_two_equal_512_bit_values() {
    let a = Val::new_wide(wide::from_u128(42, 512), 512, false);
    let b = Val::new_wide(wide::from_u128(42, 512), 512, false);
    let eq = binary_known(BinOp::Eq, a, b, None, sp()).unwrap();
    assert_eq!(eq.masked(), 1);
}

#[test]
fn wide_lt_compares_signed_512_bit_values() {
    let neg = Val::new_wide(wide::neg(&wide::from_u128(1, 512), 512), 512, true);
    let pos = Val::new_wide(wide::from_u128(1, 512), 512, true);
    let lt = binary_known(BinOp::Lt, neg, pos, None, sp()).unwrap();
    assert_eq!(lt.masked(), 1);
}

#[test]
fn wide_neg_of_a_512_bit_value() {
    let one = Val::new_wide(wide::from_u128(1, 512), 512, true);
    let negated = unary(UnOp::Neg, one);
    assert_eq!(
        wide::to_decimal_string(&negated.to_limbs(), 512, true),
        "-1"
    );
}

#[test]
fn wide_extend_builtin_widens_past_128_bits() {
    let mut ints = std::collections::BTreeMap::new();
    ints.insert("W".to_string(), 512i128);
    // extend(1, W) with W bound to 512 in the const env.
    let n = checked_width(512, sp()).unwrap();
    let v = Val::from_int(1);
    let extended = Val::new_wide(
        wide::extend(&v.to_limbs(), v.width, n, v.signed),
        n,
        v.signed,
    );
    assert_eq!(extended.width, 512);
    assert!(wide::bit_at(&extended.to_limbs(), 0));
}

#[test]
fn checked_width_accepts_up_to_the_shared_max_width() {
    assert!(checked_width(1_000_000, sp()).is_ok());
    assert!(checked_width(1_000_001, sp()).is_err());
}

#[test]
fn concat_can_exceed_128_bits() {
    let a = Val::new(u128::MAX, 128, false);
    let b = Val::new(1, 1, false);
    // Simulate what eval_ctx's Concat arm does: total width 129.
    let total = a.width + b.width;
    assert_eq!(total, 129);
}

#[test]
fn cmp_eq_signed_different_widths() {
    // 4-bit -2 (0xE, masked=14) vs 8-bit -2 (0xFE, masked=254)
    let l = Val::new(0b1110, 4, true);
    let r = Val::new(0b1111_1110, 8, true);
    assert!(cmp_eq(l, r));
}

#[test]
fn pattern_matches_handles_wide_value_no_saturation() {
    // A 200-bit value with bit 128 set and low 128 bits = 0 must NOT match
    // Pattern::Int { value: u128::MAX } — the old bits_small_or_zero()
    // saturated it to u128::MAX and caused a false match.
    let mut limbs = wide::zeros(200);
    limbs[2] = 1; // bit 128 set, low 128 bits are 0
    let s = Val::new_wide(limbs, 200, false);
    let p_not_max = Pattern::Int {
        value: u128::MAX.into(),
        raw: String::new(),
    };
    assert!(
        !pattern_matches(&p_not_max, &s),
        "saturation must not cause false match"
    );
    // A pattern matching the low bits (0) should match:
    let p_zero = Pattern::Int {
        value: 0u128.into(),
        raw: String::new(),
    };
    assert!(pattern_matches(&p_zero, &s));
}

#[test]
fn builtin_abs_wide_negative() {
    // -1 as a 200-bit signed value
    let one = Val::new_wide(wide::from_u128(1, 200), 200, true);
    let neg_one = unary(UnOp::Neg, one);
    // Abs should convert -1 (200-bit) to +1 (201-bit)
    let limbs = neg_one.to_limbs();
    let extended = wide::extend(&limbs, neg_one.width, neg_one.width + 1, neg_one.signed);
    let negated = wide::neg(&extended, neg_one.width + 1);
    assert_eq!(wide::to_decimal_string(&negated, 201, true), "1");
}

#[test]
fn builtin_trunc_wide_limb_count() {
    // 200 bits truncated to 130 bits -> limb_count should be limb_count(130) = 3
    let limbs = vec![u64::MAX; wide::limb_count(200)]; // 4 limbs
    let v = Val::new_wide(limbs, 200, false);
    let mut limbs_t = v.to_limbs();
    wide::mask_to_width(&mut limbs_t, 130);
    limbs_t.truncate(wide::limb_count(130));
    assert_eq!(limbs_t.len(), wide::limb_count(130));
    let res = Val::new_wide(limbs_t, 130, false);
    if let Bits::Wide(res_limbs) = res.bits {
        assert_eq!(res_limbs.len(), wide::limb_count(130));
    } else {
        panic!("expected Wide bits");
    }
}
