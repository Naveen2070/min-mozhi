use super::*;

// ---- unreachable code after `return` (E0812) ------------------------------

#[test]
fn unreachable_code_after_return_is_e0812() {
    let src = "fn f(a: bits[8]) -> bits[8] {\n  return a\n  let x = a\n  x\n}\nmodule M {\n  in a: bits[8]\n  out o: bits[8]\n  o = f(a)\n}\n";
    assert!(any_code(src, "E0812"));
}

#[test]
fn return_as_last_statement_before_tail_is_not_e0812() {
    // A `return` inside an `if`, with unrelated code after the `if` (not
    // after the `return` in the SAME block), is fine — this is the normal
    // guard-clause shape and must not be flagged.
    let src = "fn f(a: bits[8]) -> bits[8] {\n  if a[0] == 1 { return a }\n  a\n}\nmodule M {\n  in a: bits[8]\n  out o: bits[8]\n  o = f(a)\n}\n";
    check_one(src).expect("return inside an if, followed by unrelated code, is not E0812");
}

#[test]
fn fn_loop_body_return_followed_by_more_code_is_unreachable() {
    let src = "fn f(a: bits[8]) -> bits[8] {\n  loop i: 0..1 {\n    return a\n    let x = a\n  }\n  0\n}\nmodule M {\n  in a: bits[8]\n  out o: bits[8]\n  o = f(a)\n}\n";
    let err =
        check_one(src).expect_err("`let x` after `return` inside the loop body is unreachable");
    assert!(err.iter().any(|d| d.code == Some("E0812")));
}

#[test]
fn fn_loop_after_return_in_sibling_branch_is_not_flagged() {
    // Deliberately narrow scope (matches E0812's documented "no full
    // reachability analysis" rule): a `return` inside an `if`'s branch does
    // NOT make a `loop` that follows the `if` (at the outer level)
    // unreachable, since neither branch of the `if` is unconditional here.
    let src = "fn f(a: bits[8]) -> bits[8] {\n  if a[0] == 1 { return a }\n  loop i: 0..1 {\n    let x = a\n  }\n  0\n}\nmodule M {\n  in a: bits[8]\n  out o: bits[8]\n  o = f(a)\n}\n";
    check_one(src)
        .expect("a `loop` after a conditional (non-exhaustive) return is not unreachable");
}

// ---- array-typed fn params: element type + length (E0411/E0412) ----------

#[test]
fn array_param_with_bundle_element_type_is_e0411() {
    let src = "bundle B { a: bit }\nfn f(vals: B[4]) -> bit {\n  0\n}\nmodule M {\n  out o: bit\n  o = 0\n}\n";
    assert!(any_code(src, "E0411"));
}

#[test]
fn array_param_with_zero_length_is_e0412() {
    // No call site: `f`'s param types are resolved unconditionally by
    // `check_func_body_widths` regardless of whether `f` is ever called, so
    // E0412 fires from the declaration alone. A call site would need an
    // array-literal argument, which routes through `ExprKind::ArrayLit`
    // inference — Task 6's job, not yet wired up.
    let src =
        "fn f(vals: bits[8][0]) -> bits[8] {\n  0\n}\nmodule M {\n  out o: bits[8]\n  o = 0\n}\n";
    assert!(any_code(src, "E0412"));
}

// ---- array literals: type inference, arg-length, indexing (E0413/E0414/E0415) ----

#[test]
fn array_literal_infers_its_own_type() {
    let src = "fn f(vals: bits[8][4]) -> bits[8] {\n  vals[0]\n}\nmodule M {\n  out o: bits[8]\n  o = f([1, 2, 3, 4])\n}\n";
    assert!(check_one(src).is_ok(), "{:?}", errs(src));
}

#[test]
fn array_literal_with_mismatched_element_widths_is_e0414() {
    // This project has no `bits(W, V)` builtin (confirmed: no `Builtin`
    // variant of that shape in src/ast/expr.rs). Instead use the real
    // width-mismatch idiom already proven elsewhere in this file:
    // `extend(x, N)` on a `CtInt` literal fixes it to width `N` (see
    // `call_ty`'s `Builtin::Extend` arm, src/checker/widths/ops.rs).
    // `1` stays a bare, still-1-bit-wide `CtInt` here (the OTHER element in
    // the literal is what pins the array's element width, and any other
    // bare `CtInt` matches it unconditionally); `extend(1, 16)` is fixed at
    // `bits[16]` — 16 != 1, so the two elements visibly disagree.
    let src = "fn f(vals: bits[8][2]) -> bits[8] {\n  vals[0]\n}\nmodule M {\n  out o: bits[8]\n  o = f([1, extend(1, 16)])\n}\n";
    assert!(any_code(src, "E0414"));
}

#[test]
fn array_literal_argument_length_mismatch_is_e0413() {
    let src = "fn f(vals: bits[8][4]) -> bits[8] {\n  vals[0]\n}\nmodule M {\n  out o: bits[8]\n  o = f([1, 2, 3])\n}\n";
    assert!(any_code(src, "E0413"));
}

#[test]
fn array_param_forwarded_by_name_with_matching_type_is_accepted() {
    let src = "fn g(vals: bits[8][4]) -> bit {\n  0\n}\nfn f(vals: bits[8][4]) -> bit {\n  g(vals)\n}\nmodule M {\n  out o: bit\n  o = 0\n}\n";
    assert!(check_one(src).is_ok(), "{:?}", errs(src));
}

#[test]
fn array_param_forwarded_by_name_with_mismatched_length_is_rejected() {
    let src = "fn g(vals: bits[8][2]) -> bit {\n  0\n}\nfn f(vals: bits[8][4]) -> bit {\n  g(vals)\n}\nmodule M {\n  out o: bit\n  o = 0\n}\n";
    assert!(
        !errs(src).is_empty(),
        "expected a diagnostic for a length-mismatched array forward, got none"
    );
}

#[test]
fn constant_array_index_out_of_range_is_e0415() {
    let src = "fn f(vals: bits[8][4]) -> bits[8] {\n  vals[9]\n}\nmodule M {\n  out o: bits[8]\n  o = f([1, 2, 3, 4])\n}\n";
    assert!(any_code(src, "E0415"));
}

#[test]
fn runtime_array_index_is_accepted() {
    let src = "fn f(vals: bits[8][4], i: bits[2]) -> bits[8] {\n  vals[i]\n}\nmodule M {\n  in i: bits[2]\n  out o: bits[8]\n  o = f([1, 2, 3, 4], i)\n}\n";
    assert!(check_one(src).is_ok(), "{:?}", errs(src));
}

// ---- indexing a bare array literal directly is rejected (E0419) -----------
// BUG-57 (docs/audit/bugs.md): `[a,a,a][0]` used to be accepted here (an
// array literal's own `Ty::Array` looks identical to a named array's to the
// `Index` arm above) and PANIC in the emitter instead
// (`unreachable!("Task 8 or Task 9 wires this up")`, `emit_verilog/expr.rs`)
// — a literal-then-immediately-indexed array has no named binding to
// optimize away and was never actually implemented end-to-end.

#[test]
fn indexing_an_array_literal_directly_is_e0419() {
    let src = "module M {\n  in a: bits[4]\n  out z: bits[4]\n  z = [a, a, a][0]\n}\n";
    assert!(any_code(src, "E0419"));
}

#[test]
fn indexing_a_named_array_still_works_after_e0419() {
    // Regression guard: E0419 must fire on the LITERAL shape only, not on
    // every `Ty::Array`-typed `Index` — a named array (via a `fn` param)
    // stays accepted.
    let src = "fn f(vals: bits[8][4]) -> bits[8] {\n  vals[0]\n}\nmodule M {\n  out o: bits[8]\n  o = f([1, 2, 3, 4])\n}\n";
    assert!(check_one(src).is_ok(), "{:?}", errs(src));
}

// ---- module-level array signals are rejected (E0416) ----------------------
// Module-level port/wire/register arrays are an explicit non-goal (would need
// per-element driver-uniqueness checking) — array types are only supported
// for `fn` parameters. `fn`-parameter array tests above are unaffected: this
// check is wired into Port/Wire/Reg's `walk_items` arms only, never into `fn`
// param resolution.

#[test]
fn array_typed_module_port_is_e0416() {
    let src = "module M {\n  in vals: bits[8][4]\n  out o: bit\n  o = vals[0][0]\n}\n";
    assert!(any_code(src, "E0416"));
}

#[test]
fn array_typed_wire_is_e0416() {
    let src =
        "module M {\n  wire vals: bits[8][4] = [1, 2, 3, 4]\n  out o: bit\n  o = vals[0][0]\n}\n";
    assert!(any_code(src, "E0416"));
}

#[test]
fn array_typed_output_with_constant_indexed_drive_is_e0416_not_a_panic() {
    // Regression: an array-typed `out` with a single constant-range drive
    // site used to reach report_coverage's driver-coverage width match (an
    // `out` is the only site iterated there — `in`/`wire` never hit this
    // arm), which had no `Type::Array` arm and panicked via `unreachable!`
    // instead of surfacing E0416 from resolve_names.
    let src = "module M {\n  out vals: bits[8][4]\n  vals[0] = 1\n}\n";
    assert!(any_code(src, "E0416"));
}

#[test]
fn extern_module_duplicate_in_same_file_is_e1301() {
    let src = "extern module Pll { in clk_in: bit }\n\
               extern module Pll { in clk_in: bit }\n\
               module M { }\n";
    first_err(src, "E1301");
}

#[test]
fn extern_module_bundle_typed_port_is_e1302() {
    let src = "bundle B { x: bit }\n\
               extern module Pll { in b: B }\n\
               module M { }\n";
    first_err(src, "E1302");
}

#[test]
fn extern_module_array_typed_port_is_e1302() {
    let src = "extern module Pll { in vals: bits[8][4] }\nmodule M { }\n";
    first_err(src, "E1302");
}

#[test]
fn extern_module_scalar_ports_check_clean() {
    let src = "extern module Pll(MULT: int = 2) {\n  \
               doc: \"test\"\n  in clk_in: bit\n  out clk_out: bit\n  out locked: bit\n}\n\
               module M { }\n";
    check_one(src).expect("a scalar-only extern module must check clean");
}

// NOTE on the three tests below: the task brief's Step 1 sketch connected
// extern OUTPUT ports (`clk_out`, `locked`) inside the `{ conns }` block,
// but `check_inst` already rejects that for real modules too (E0107, see
// `connecting_an_output_is_e0107` above) — outputs are read back with
// `inst.field`, never connected. The sources below are corrected to match
// that existing, unchanged semantics: only the input is connected, outputs
// are read via `u.field`. Same correction applies to test 2's expected
// code (E0302, not reachable if an output were connected first — E0107
// would fire before the "missing input" check ever runs) and test 3's
// expected code (E0107 "has no input named", the real code `check_inst`
// emits for an unknown connection-name; E0104 is `inst_output`'s code for
// reading a nonexistent OUTPUT, a different call site).
#[test]
fn extern_instantiation_checks_clean_with_correct_connections() {
    // `clk_in` must be declared `clock` (not `in clk_in: bit`) to accept a
    // clock-typed signal — `bit` and `clock` are distinct types (`same()`
    // in widths/mod.rs), same rule real modules already follow. Task 5
    // wires up width-checking for extern instantiations; this fixture
    // predates that (Task 4 only checked names/arity) and connected a
    // clock signal to a `bit` port, which is a genuine E0401 once widths
    // are actually checked.
    let src = "extern module Pll(MULT: int = 2) {\n  \
               clock clk_in\n  out clk_out: bit\n  out locked: bit\n}\n\
               module M {\n  clock sysclk\n  out fast: bit\n  out ok: bit\n  \
               let u = Pll(MULT: 4) { clk_in: sysclk }\n  fast = u.clk_out\n  ok = u.locked\n}\n";
    check_one(src).expect("valid extern instantiation must check clean");
}

#[test]
fn extern_instantiation_missing_input_connection_is_reported() {
    let src = "extern module Pll { in clk_in: bit\n  out clk_out: bit }\n\
               module M {\n  out fast: bit\n  let u = Pll() { }\n  fast = u.clk_out\n}\n";
    first_err(src, "E0302");
}

#[test]
fn extern_instantiation_unknown_port_is_reported() {
    let src = "extern module Pll { in clk_in: bit }\n\
               module M {\n  in x: bit\n  let u = Pll() { nope: x }\n}\n";
    first_err(src, "E0107");
}

#[test]
fn extern_instantiation_wrong_width_connection_is_e0401() {
    let src = "extern module Pll { in clk_in: bit }\n\
               module M {\n  in wide: bits[4]\n  \
               let u = Pll() { clk_in: wide }\n}\n";
    first_err(src, "E0401");
}

#[test]
fn structurally_compatible_bundle_wire_binding_checks_clean() {
    // "Let bindings" in the design spec's Goals means a typed-value
    // binding from an expression — in this language that's `wire NAME:
    // TYPE = expr` (module-body local `let` has no type-annotation syntax
    // and was never meant to grow one for this feature; see the plan's
    // note on this call site). `wire`'s own init-expr check already routes
    // through `check_expr`'s generic fallback into `expect_ty`, so this
    // exercises the exact same arm `fn` args do, with zero grammar changes.
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle SensorData { tx: bit, rx: bit }\n\
               module M {\n  in a_tx: bit\n  in a_rx: bit\n  out b_tx: bit\n  out b_rx: bit\n  \
               wire a: SensorData = { tx: a_tx, rx: a_rx }\n  \
               wire b: HasUART = a\n  \
               b_tx = b.tx\n  b_rx = b.rx\n}\n";
    check_one(src).expect("a structurally-compatible wire binding must check clean");
}

#[test]
fn structurally_compatible_fn_arg_checks_clean() {
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle SensorData { tx: bit, rx: bit }\n\
               fn pick_tx(u: HasUART) -> bit { u.tx }\n\
               module M {\n  in  a_tx: bit\n  in a_rx: bit\n  out o: bit\n  \
               wire a: SensorData = { tx: a_tx, rx: a_rx }\n  \
               o = pick_tx(a)\n}\n";
    check_one(src).expect("a structurally-compatible fn argument must check clean");
}

#[test]
fn wire_binding_bundle_missing_field_is_e0910() {
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle Partial { tx: bit }\n\
               module M {\n  in a_tx: bit\n  out b_tx: bit\n  out b_rx: bit\n  \
               wire a: Partial = { tx: a_tx }\n  \
               wire b: HasUART = a\n  \
               b_tx = b.tx\n  b_rx = b.rx\n}\n";
    let d = first_err(src, "E0910");
    assert!(
        d.msg.contains("rx"),
        "expected field `rx` named, got: {}",
        d.msg
    );
}

#[test]
fn structurally_compatible_fn_return_checks_clean() {
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle SensorData { tx: bit, rx: bit }\n\
               fn as_uart(u: SensorData) -> HasUART { u }\n\
               module M {\n  in  a_tx: bit\n  in a_rx: bit\n  out b_tx: bit\n  \
               wire a: SensorData = { tx: a_tx, rx: a_rx }\n  \
               wire b: HasUART = as_uart(a)\n  b_tx = b.tx\n}\n";
    check_one(src).expect("a structurally-compatible fn return must check clean");
}

#[test]
fn fn_return_bundle_missing_field_is_e0910() {
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle Partial { tx: bit }\n\
               fn as_uart(u: Partial) -> HasUART { u }\n\
               module M {\n  in  a_tx: bit\n  out b_tx: bit\n  \
               wire a: Partial = { tx: a_tx }\n  \
               wire b: HasUART = as_uart(a)\n  b_tx = b.tx\n}\n";
    let d = first_err(src, "E0910");
    assert!(
        d.msg.contains("rx"),
        "expected field `rx` named, got: {}",
        d.msg
    );
}

#[test]
fn fn_return_same_name_bundle_regression_still_e0804() {
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle Other { tx: bit, rx: bit }\n\
               fn broken(u: HasUART) -> Other { u }\n\
               module M {\n  in  a_tx: bit\n  in a_rx: bit\n  out b_tx: bit\n  \
               wire a: HasUART = { tx: a_tx, rx: a_rx }\n  \
               wire b: Other = broken(a)\n  b_tx = b.tx\n}\n";
    // `HasUART` and `Other` are structurally compatible (identical fields),
    // so this must now check CLEAN, not E0804 — proving check_return_ty was
    // upgraded (this is the inverse of the old behavior, an intentional
    // behavior change, not a regression to preserve).
    check_one(src).expect(
        "HasUART and Other are structurally identical — this must check clean post-upgrade",
    );
}

#[test]
fn fn_return_bundle_shared_field_wrong_width_is_e0804() {
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle Wrong { tx: bits[4], rx: bit }\n\
               fn as_uart(w: Wrong) -> HasUART { w }\n\
               module M {\n  in  a_tx: bits[4]\n  in a_rx: bit\n  \
               out b_tx: bit\n  out b_rx: bit\n  \
               wire a: Wrong = { tx: a_tx, rx: a_rx }\n  \
               wire b: HasUART = as_uart(a)\n  b_tx = b.tx\n  b_rx = b.rx\n}\n";
    let d = first_err(src, "E0804");
    assert!(
        d.msg.contains("tx"),
        "expected the mismatched field `tx` named in the message, got: {}",
        d.msg
    );
}

#[test]
fn structurally_compatible_bundle_port_connection_checks_clean() {
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle SensorData { tx: bit, rx: bit }\n\
               module Child { in u: HasUART }\n\
               module M {\n  in  a_tx: bit\n  in a_rx: bit\n  \
               wire a: SensorData = { tx: a_tx, rx: a_rx }\n  \
               let c = Child() { u: a }\n}\n";
    check_one(src).expect("a structurally-compatible port connection must check clean");
}

#[test]
fn port_connection_bundle_missing_field_is_e0910() {
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle Partial { tx: bit }\n\
               module Child { in u: HasUART }\n\
               module M {\n  in  a_tx: bit\n  \
               wire a: Partial = { tx: a_tx }\n  \
               let c = Child() { u: a }\n}\n";
    let d = first_err(src, "E0910");
    assert!(
        d.msg.contains("rx"),
        "expected field `rx` named, got: {}",
        d.msg
    );
}

#[test]
fn port_connection_bundle_shared_field_wrong_width_is_e0401() {
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle Wrong { tx: bits[4], rx: bit }\n\
               module Child { in u: HasUART }\n\
               module M {\n  in  a_tx: bits[4]\n  in a_rx: bit\n  \
               wire a: Wrong = { tx: a_tx, rx: a_rx }\n  \
               let c = Child() { u: a }\n}\n";
    let d = first_err(src, "E0401");
    assert!(
        d.msg.contains("tx"),
        "expected field `tx` named, got: {}",
        d.msg
    );
}

#[test]
fn structural_match_composes_across_fn_return_and_port_connection() {
    // Integration test: ONE structurally-compatible bundle pair
    // (HasUART/SensorData) threaded through TWO of the four
    // `bundle_shape_match` call sites in the SAME design — a `fn` return
    // (`check_return_ty`, widths/mod.rs) followed by a module-port
    // connection (`check_inst_widths`, widths/insts.rs) — to prove the
    // call sites compose in one program, not just in isolation.
    //
    // `as_uart`'s body returns its `SensorData`-typed param `s` as the
    // declared `HasUART` return type: structural match #1 (check_return_ty).
    // The call's inferred type is the function's DECLARED return type
    // (`HasUART`), which is then fed into `Child`'s `u: SensorData` port:
    // structural match #2 (check_inst_widths), in the opposite direction.
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle SensorData { tx: bit, rx: bit }\n\
               fn as_uart(s: SensorData) -> HasUART { s }\n\
               module Child { in u: SensorData }\n\
               module M {\n  in a_tx: bit\n  in a_rx: bit\n  \
               wire s: SensorData = { tx: a_tx, rx: a_rx }\n  \
               wire h: HasUART = as_uart(s)\n  \
               let c = Child() { u: h }\n}\n";
    check_one(src).expect(
        "one structurally-compatible bundle pair threaded through a fn return \
         and a port connection in the same design must check clean",
    );
}

#[test]
fn drive_bundle_zero_required_fields_always_compatible() {
    // Edge case: `bundle Empty {}` has zero required fields, so
    // `bundle_shape_match`'s `for` loop over `required`'s fields is a
    // no-op — it must return `Compatible` no matter what the provided
    // bundle declares. Routed through the Drive-path call site (simplest),
    // mirroring `drive_bundle_same_name_regression_still_checks_clean`'s
    // shape.
    let src = "bundle Empty {}\n\
               bundle SensorData { tx: bit, rx: bit }\n\
               module M {\n  in a_tx: bit\n  in a_rx: bit\n  \
               wire a: SensorData = { tx: a_tx, rx: a_rx }\n  \
               out b: Empty\n  \
               b = a\n}\n";
    check_one(src).expect("a zero-required-field bundle must accept any provided bundle trivially");
}

#[test]
fn matched_ty_same_shaped_bundle_equality_passes() {
    // Regression: `matched_ty` (checker/widths/ops.rs) must check
    // `same(&a, &b)` BEFORE falling through to the scalar-only
    // `ty_to_kind_opt` conversion. Two identically-shaped bundles
    // compared with `==` are structurally equal (`same()` handles
    // Bundle-name-equality), so this must pass clean — a prior refactor
    // instead tried `ty_to_kind_opt` on both sides first, which returns
    // `None` for any Bundle/Array/Memory, wrongly falling into the
    // "cannot mix" (E0403) diagnostic for this exact-match case.
    let src = "bundle Bus { a: bit, b: bits[8] }\n\
               module Top {\n  in a: Bus\n  in b: Bus\n  out z: bit\n  \
               z = (a == b)\n}\n";
    check_one(src).expect("comparing two identically-shaped bundles with `==` must pass");
}
