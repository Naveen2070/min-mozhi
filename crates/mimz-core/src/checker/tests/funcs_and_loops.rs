use super::*;

// ---- no declarations inside `repeat` (E0303) ------------------------------

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
fn return_width_mismatch_is_e0804() {
    // `return`ing a widened value must also be caught by E0804, not just
    // a width mismatch in the tail.
    let src = "fn f(a: bits[8]) -> bits[8] {\n  if a[0] == 1 { return extend(a, 16) }\n  a\n}\nmodule M {\n  in a: bits[8]\n  out o: bits[8]\n  o = f(a)\n}\n";
    first_err(src, "E0804");
}

#[test]
fn return_width_match_is_accepted() {
    let src = "fn f(a: bits[8]) -> bits[8] {\n  if a[0] == 1 { return a }\n  a\n}\nmodule M {\n  in a: bits[8]\n  out o: bits[8]\n  o = f(a)\n}\n";
    check_one(src).expect("return type matches declared return type");
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

#[test]
fn fn_with_const_local_compiles_clean() {
    // A bare-literal local (`let n = 5`) infers as CtInt(5).  Before the fix,
    // the width pass left `inferred_width` at None and the emitter hit an
    // unreachable!().  After the fix, min_bits(5) = 3 is stored and the
    // emitter declares `reg [2:0] n`.
    check_one(
        "fn add_offset(a: bits[8]) -> bits[8] {\n  let n = 5\n  a +% n\n}\nmodule M {\n  in a: bits[8]\n  out result: bits[8]\n  result = add_offset(a)\n}\n",
    )
    .expect("fn with a bare-literal local compiles clean");
}

#[test]
fn unbound_name_inside_fn_return_is_rejected() {
    // A `return` expression is a real name-resolution site, not just the tail.
    let src = "fn f(a: bits[8]) -> bits[8] {\n  if a[0] == 1 { return unbound_thing }\n  a\n}\nmodule M {\n  in a: bits[8]\n  out o: bits[8]\n  o = f(a)\n}\n";
    assert!(
        any_code(src, "E0101"),
        "an unbound name inside a `return` expression is E0101"
    );
}

#[test]
fn fn_if_branch_names_are_resolved() {
    // A `let` bound before the `if` must be visible inside both branches
    // AND inside a `return` expression — this is the same flat-scope model
    // `on`-block `SeqStmt::If` already uses (no branch-local shadowing).
    let src = "fn f(a: bits[8]) -> bits[8] {\n  let x = a\n  if a[0] == 1 { return x }\n  x\n}\nmodule M {\n  in a: bits[8]\n  out o: bits[8]\n  o = f(a)\n}\n";
    check_one(src).expect("let-bound name is visible inside the if-branch return and tail");
}

#[test]
fn let_bound_only_inside_an_if_branch_does_not_leak_outside() {
    // `y` is bound only on the `a == 1` path — referencing it after the
    // `if` (a path where it was never bound) must be rejected. This is the
    // soundness gap found by the final whole-branch review: the checker
    // used to accept this, but the emitter reads an uninitialized register
    // on the `else` path and the simulator errors with "unknown name" when
    // that path is taken — the SAME source disagreeing across backends.
    let src = "fn f(a: bit) -> bits[8] {\n  if a == 1 { let y = 5 }\n  y\n}\nmodule M {\n  in a: bit\n  out o: bits[8]\n  o = f(a)\n}\n";
    assert!(
        any_code(src, "E0101"),
        "a let bound only inside an if-branch must not be visible after the if"
    );
}

#[test]
fn let_bound_only_inside_one_if_branch_is_not_visible_in_the_sibling_branch() {
    // `y` bound in `then` must not leak into `els`'s own check either —
    // the sibling branch is just as much "a path where `y` was never
    // bound" as the code after the `if`.
    let src = "fn f(a: bit) -> bits[8] {\n  if a == 1 { let y = 5 } else { return y }\n  0\n}\nmodule M {\n  in a: bit\n  out o: bits[8]\n  o = f(a)\n}\n";
    assert!(
        any_code(src, "E0101"),
        "a let bound in the then-branch must not be visible in the else-branch"
    );
}

#[test]
fn let_bound_only_inside_an_if_branch_is_not_visible_to_width_checking_outside_it() {
    // Same scope-leak class as the two name-resolution tests above, but
    // this one targets `check_fn_stmt_widths`'s OWN copy of the (now-fixed)
    // leaking scope model directly — regardless of which checker pass
    // catches it first, `y` referenced outside the branch that bound it
    // must not resolve to a valid, in-scope value.
    let src = "fn f(a: bit) -> bits[8] {\n  if a == 1 { let y = 5 }\n  y\n}\nmodule M {\n  in a: bit\n  out o: bits[8]\n  o = f(a)\n}\n";
    assert!(
        check_one(src).is_err(),
        "a branch-local let referenced outside its if must not compile clean"
    );
}

// ---- `loop` name resolution ------------------------------------------------

#[test]
fn fn_loop_variable_resolves_inside_its_own_body() {
    // Array-typed module ports are E0416 (see `array_typed_module_port_is_e0416`
    // below) — array types are `fn`-parameter only, so (unlike the brief's
    // literal draft) the caller assembles the array from scalar ports via an
    // array literal, exactly like the existing `fn_array_search` example does.
    let src = "fn find(vals: bits[8][4]) -> signed[4] {\n  loop i: 0..4 {\n    if vals[i] == 0xFF { return i }\n  }\n  0 - 1\n}\nmodule M {\n  in a: bits[8]\n  in b: bits[8]\n  in c: bits[8]\n  in d: bits[8]\n  out o: signed[4]\n  o = find([a, b, c, d])\n}\n";
    check_one(src).expect("loop variable `i` must resolve inside the loop body");
}

#[test]
fn seq_loop_variable_resolves_inside_on_block() {
    // Load-bearing: `i` is used in an arithmetic position (`vals0 +% i`), not
    // just referenced-and-discarded — an unbound `i` here is an unavoidable
    // E0101, so this fails if the `SeqStmt::Loop` arm's env-binding in
    // `names.rs` ever regresses to a no-op (unlike a body that never reads
    // `i` at all, which can't tell "bound" from "never bound").
    let src = "module M {\n  clock clk\n  reset rst\n  in vals0: bits[8]\n  reg acc: bits[8] = 0\n  on rise(clk) {\n    loop i: 0..1 {\n      acc <- vals0 +% i\n    }\n  }\n}\n";
    check_one(src).expect("loop variable must resolve inside an on-block loop body");
}

#[test]
fn fn_loop_variable_does_not_leak_outside_the_loop() {
    // Mirrors the `let`-leak test below, but for the loop VARIABLE itself
    // (the env shadow/remove cleanup), not a `let` inside the body.
    let src = "fn find(vals: bits[8][4]) -> signed[4] {\n  loop i: 0..4 {\n    if vals[i] == 0xFF { return i }\n  }\n  i\n}\nmodule M {\n  in a: bits[8]\n  in b: bits[8]\n  in c: bits[8]\n  in d: bits[8]\n  out o: signed[4]\n  o = find([a, b, c, d])\n}\n";
    assert!(
        any_code(src, "E0101"),
        "`i` is only bound inside the loop — it must not leak into the tail"
    );
}

#[test]
fn seq_loop_variable_does_not_leak_outside_the_loop() {
    let src = "module M {\n  clock clk\n  reset rst\n  in vals0: bits[8]\n  reg acc: bits[8] = 0\n  on rise(clk) {\n    loop i: 0..1 {\n      acc <- vals0\n    }\n    acc <- i\n  }\n}\n";
    assert!(
        any_code(src, "E0101"),
        "`i` is only bound inside the loop — it must not leak past it in the on-block"
    );
}

#[test]
fn fn_loop_local_let_does_not_leak_outside_the_loop() {
    let src = "fn f(a: bits[8]) -> bits[8] {\n  loop i: 0..1 {\n    let x = a\n  }\n  x\n}\nmodule M {\n  in a: bits[8]\n  out o: bits[8]\n  o = f(a)\n}\n";
    assert!(
        any_code(src, "E0101"),
        "`x` is only bound inside the loop body — it must not leak past it"
    );
}

#[test]
fn non_constant_seq_loop_bound_is_e0201() {
    // `loop`, like `repeat`, unrolls at compile time — its bounds must
    // const-evaluate. Mirrors `non_constant_repeat_bound_is_e0201` above.
    let src = "module M {\n  clock clk\n  reset rst\n  in x: bits[4]\n  reg acc: bit = 0\n  on rise(clk) {\n    loop i: 0..x {\n      acc <- 0\n    }\n  }\n}\n";
    let d = first_err(src, "E0201");
    assert!(d.msg.contains("not a compile-time constant"));
}

#[test]
fn non_constant_fn_loop_bound_is_e0201() {
    let src = "fn f(n: bits[4]) -> bit {\n  loop i: 0..n {\n    let x = i\n  }\n  0\n}\nmodule M {\n  in n: bits[4]\n  out o: bit\n  o = f(n)\n}\n";
    let d = first_err(src, "E0201");
    assert!(d.msg.contains("not a compile-time constant"));
}

#[test]
fn fn_loop_body_width_mismatch_is_checked() {
    // `vals` has 2 elements (indices 0..=1), but the loop runs `i: 0..3` —
    // an out-of-range constant index at `i == 2`. This is caught ONLY if the
    // width pass actually binds `i` to each sampled compile-time value while
    // walking the body (as `repeat` does for its own loop var): a bare
    // recursion that never binds `i` leaves `vals[i]`'s index type `Unknown`
    // (see `ident_ty`'s `cx.env` lookup), which silently skips the E0415
    // range check entirely — so this test is red under the old placeholder
    // arm (bare recursion, no env binding) and green only once sampling with
    // env insertion is added.
    let src = "fn f(vals: bits[8][2]) -> bits[8] {\n  loop i: 0..3 {\n    if i == 2 { return vals[i] }\n  }\n  0\n}\nmodule M {\n  in a: bits[8]\n  in b: bits[8]\n  out o: bits[8]\n  o = f([a, b])\n}\n";
    first_err(src, "E0415");
}

#[test]
fn fn_loop_width_bug_independent_of_loop_var_reports_once() {
    // The body's bug (`vals[5]` on a 2-element array) does NOT depend on `i`
    // at all — it is equally wrong on every sampled iteration. `FnStmt::Loop`
    // samples all of `0..3` (well under `MAX_REPEAT_CHECKS`), so a checker
    // that walks every sampled iteration unconditionally would emit THREE
    // E0415 diagnostics for the same bug. `ModuleItem::Repeat` and
    // `SeqStmt::Loop` both break out of their sampling loop after the first
    // iteration that adds a diagnostic; `FnStmt::Loop` must do the same —
    // this test is red (3 diagnostics) without that guard and green (1) with it.
    let src = "fn f(vals: bits[8][2]) -> bits[8] {\n  loop i: 0..3 {\n    let x = vals[5]\n  }\n  0\n}\nmodule M {\n  in a: bits[8]\n  in b: bits[8]\n  out o: bits[8]\n  o = f([a, b])\n}\n";
    let diags = errs(src);
    let count = diags.iter().filter(|d| d.code == Some("E0415")).count();
    assert_eq!(
        count, 1,
        "expected exactly one E0415 for a loop-var-independent bug, got {count}: {diags:?}"
    );
}
