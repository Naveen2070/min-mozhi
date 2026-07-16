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

/// Lex, parse, check and emit a single (import-free) source string to
/// Verilog. A local, single-file stand-in for `crate::compile_string`
/// (which lives in the root crate's command runner, out of reach for
/// mimz-core's own tests) — same pipeline, minus `import` resolution.
fn compile_one(src: &str) -> Result<String, Vec<Diag>> {
    let mut asts = vec![parse(src)];
    check(&asts)?;
    crate::emit_verilog::transliterate(&mut asts);
    let project = crate::emit_verilog::Project::from_files(&asts)?;
    crate::emit_verilog::emit(&project, &asts)
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

/// Like [`first_err`], but takes a pre-built file slice instead of parsing
/// one string — needed for scenarios that are inherently multi-file (e.g.
/// cross-file ambiguity).
fn first_err_multi(files: &[crate::ast::File], code: &str) -> Diag {
    let diags = check(files).expect_err("expected checker errors");
    assert_eq!(
        diags[0].code,
        Some(code),
        "expected {code}, got {:?}: {}",
        diags[0].code,
        diags[0].msg
    );
    diags.into_iter().next().unwrap()
}

/// Like [`errs`], but takes a pre-built file slice instead of parsing one
/// string — needed for scenarios that are inherently multi-file.
fn errs_multi(files: &[crate::ast::File]) -> Vec<Diag> {
    check(files).expect_err("expected checker errors")
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
fn same_name_module_in_different_files_is_not_an_error_until_referenced() {
    let files = [
        parse("module A {\n  out y: bit\n  y = 0\n}\n"),
        parse("module A {\n  out z: bit\n  z = 0\n}\n"),
    ];
    check(&files)
        .expect("two files may each declare `A` — no ambiguity until something references it");
}

#[test]
fn ambiguous_bare_module_reference_is_e0110() {
    // Both files declare `Fifo`; the referencing file imports both and
    // uses the bare name — ambiguous.
    let a = parse("module Fifo {\n  out y: bit\n  y = 0\n}\n");
    let b = parse("module Fifo {\n  out z: bit\n  z = 0\n}\n");
    let user = parse("module M {\n  let u = Fifo() { }\n}\n");
    let d = first_err_multi(&[user, a, b], "E0110");
    assert!(d.help.unwrap().contains("qualify"));
}

#[test]
fn qualified_module_reference_resolves_unambiguously() {
    // Same setup, but the reference is qualified — must resolve cleanly.
    // Hand-wire: pretend this file's own import #0 resolved to file 2 (b).
    // (In the real pipeline, project.rs sets this from `import`.)
    let a = parse("module Fifo {\n  out y: bit\n  y = 0\n}\n");
    let b = parse("module Fifo {\n  out z: bit\n  z = 0\n}\n");
    let mut user = parse("module M {\n  let u = Fifo() { }\n}\n");
    if let crate::ast::TopItem::Module(m) = &mut user.items[0]
        && let crate::ast::ModuleItem::Inst(inst) = &mut m.items[0]
    {
        inst.module.path.push(crate::ast::Ident {
            name: "b".into(),
            span: inst.module.span,
        });
        inst.module.resolved_file.set(Some(2));
    }
    check(&[user, a, b]).expect("qualified reference resolves without ambiguity");
}

#[test]
fn qualified_reference_actually_resolves_via_a_real_import_path() {
    // This is the end-to-end mechanism test the test above never covered —
    // that one hand-set `QualIdent.resolved_file` directly. This one goes
    // through the real path: `user` has an actual `import b` statement and a
    // qualified `b.Fifo()` reference; only `Import.resolved_file` is set
    // (mimicking what `project::load_project` does at Task 3 — this test file
    // doesn't go through `project.rs`, so it sets that one Cell by hand, the
    // same way the parser leaves it `None` and only the loader fills it in).
    // Nothing here pokes `QualIdent.resolved_file` — the checker itself must
    // compute the match from `q.path` against `user`'s own `imports`.
    let a = parse("module Fifo {\n  out y: bit\n  y = 0\n}\n");
    let b = parse("module Fifo {\n  out z: bit\n  z = 0\n}\n");
    let user = parse("import b\n\nmodule M {\n  let u = b.Fifo() { }\n}\n");
    assert_eq!(user.imports.len(), 1, "sanity: `import b` parsed");
    // `Import.resolved_file` is a `Cell` — settable through a shared `&File`.
    user.imports[0].resolved_file.set(Some(2));
    check(&[user, a, b]).expect(
        "qualified reference must resolve via the real import match, not a hand-poked Cell",
    );
}

#[test]
fn qualified_reference_with_unmatched_path_is_e0111() {
    let a = parse("module Fifo {\n  out y: bit\n  y = 0\n}\n");
    let mut user = parse("module M {\n  let u = Fifo() { }\n}\n");
    if let crate::ast::TopItem::Module(m) = &mut user.items[0]
        && let crate::ast::ModuleItem::Inst(inst) = &mut m.items[0]
    {
        inst.module.path.push(crate::ast::Ident {
            name: "nope".into(),
            span: inst.module.span,
        });
        // resolved_file left None — no import matched this path.
    }
    first_err_multi(&[user, a], "E0111");
}

#[test]
fn qualified_reference_to_a_file_that_doesnt_declare_the_name_is_e0111() {
    // `wrongpkg` really is imported and really resolves to a real file —
    // but that file declares `NotFifo`, not `Fifo`. `Fifo` does exist
    // project-wide (in `right`), so this is NOT the "0 candidates anywhere"
    // case (E0102) and NOT the "path matches no import" case covered by
    // `qualified_reference_with_unmatched_path_is_e0111` above — the import
    // resolves cleanly, but the target file's own declarations don't
    // contain the name.
    let right = parse("module Fifo {\n  out y: bit\n  y = 0\n}\n");
    let wrongpkg = parse("module NotFifo {\n  out z: bit\n  z = 0\n}\n");
    let user = parse("import wrongpkg\n\nmodule M {\n  let u = wrongpkg.Fifo() { }\n}\n");
    assert_eq!(user.imports.len(), 1, "sanity: `import wrongpkg` parsed");
    // files: [user=0, right=1, wrongpkg=2] — the import matches and resolves
    // to file 2 (`wrongpkg`), which has no `Fifo`.
    user.imports[0].resolved_file.set(Some(2));
    first_err_multi(&[user, right, wrongpkg], "E0111");
}

#[test]
fn same_name_module_in_the_same_file_is_still_e0001() {
    let d = first_err("module A {\n}\nmodule A {\n}\n", "E0001");
    assert!(d.msg.contains("more than once") || d.msg.contains("twice"));
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
fn array_param_length_referencing_an_unbound_name_is_e0101() {
    let src = "fn f(vals: bits[8][unbound_thing]) -> bits[8] {\n  vals[0]\n}\nmodule M {\n  out o: bits[8]\n  o = f([1])\n}\n";
    assert!(any_code(src, "E0101"));
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
fn foreach_elements_form_on_scalar_is_e0417() {
    let src =
        "module M {\n  in a: bits[8]\n  out o: bits[8]\n  foreach x in a {\n    o = x\n  }\n}\n";
    let d = first_err(src, "E0417");
    assert!(d.msg.contains("not an array or mem type"));
}

#[test]
fn foreach_range_form_checks_clean() {
    // Regression fix: the original version of this test used an
    // array-typed `out` (`out lamps: bits[8][4]`) with a `wire lamps[i]:
    // ...` body — both invalid. Array-typed module-level ports/wires/regs
    // are unconditionally rejected (E0416 — see
    // `array_typed_module_port_is_e0416`/`array_typed_wire_is_e0416`
    // below), and `wire name[i]: ty = expr` isn't valid wire-declaration
    // syntax (only a bare identifier before `:`). Mirrors the known-good
    // `repeat`-based bit-indexed-drive pattern already used throughout
    // this file (e.g. `non_constant_repeat_bound_is_e0201` above).
    let src = "module M {\n  out y: bits[4]\n  foreach i in 0..4 {\n    y[i] = 0\n  }\n}\n";
    check_one(src).expect("foreach range form over a valid module must check clean");
}

#[test]
fn foreach_elements_form_checks_clean_over_mem() {
    // Regression fix: the original version of this test iterated an
    // array-TYPED `in` port (`in values: bits[8][8]`) — module-level
    // array-typed ports are unconditionally rejected by E0416 (see
    // `array_typed_module_port_is_e0416` below), so that source never
    // checked clean even before `foreach` existed. `mem` is the actual
    // array-like module-level signal this language supports (see
    // `ForEachSource::Elements`'s own doc comment and
    // `ast::foreach_lower::array_like_len`'s `ModuleItem::Mem` arm) —
    // reading `mem[idx]` combinationally is normal usage even inside an
    // `on` block's RHS (mem is only WRITE-restricted to `<-`).
    let src = "module M {\n  clock clk\n  reset rst\n  mem values: bits[8][8] = 0\n  reg acc: bits[11] = 0\n  on rise(clk) {\n    foreach v in values {\n      acc <- acc\n    }\n  }\n}\n";
    check_one(src).expect("foreach element form over a declared mem must check clean");
}

#[test]
fn foreach_elements_form_variable_resolves_inside_on_block() {
    // Same `mem`-not-array-port fix as `foreach_elements_form_checks_clean_over_mem` above.
    let src = "module M {\n  clock clk\n  reset rst\n  mem values: bits[8][8] = 0\n  reg acc: bits[8] = 0\n  on rise(clk) {\n    foreach v in values {\n      acc <- v\n    }\n  }\n}\n";
    check_one(src).expect("`v` must resolve inside the foreach body via substitution");
}

/// Proves the module-item-level Elements form (`ModuleItem::ForEach`,
/// `walk_items`'s arm) checks clean end-to-end: name resolution succeeds
/// (no E0417 — `values` is a declared `mem`) and there's no spurious
/// E0303 (`lower_foreach_item`'s Elements form substitutes `v` with
/// `values[idx]` throughout the body rather than synthesizing a `Wire`
/// declaration, so nothing is ever "declared inside a repeat"). A
/// single-element `mem` sidesteps the unrelated question of whether
/// combinationally driving `sum` from every unrolled iteration is
/// single-driver-clean (E0501, drivers.rs — a later pass this task
/// doesn't touch).
#[test]
fn foreach_elements_form_at_module_level_checks_clean() {
    let src = "module M {\n  mem values: bits[8][1] = 0\n  out sum: bits[8]\n  foreach v in values {\n    sum = v\n  }\n}\n";
    check_one(src).expect("module-item-level foreach elements form over a mem must check clean");
}

/// Proves the `fn`-body Elements form (`FnStmt::ForEach`,
/// `check_fn_stmt_names`'s arm) resolves `v`'s source against the `fn`'s
/// OWN array-typed parameter via `array_like_len_fn` (no module context
/// exists for a top-level `fn` — see `check_func_names`'s comment) —
/// mirrors the known-good `loop`-based array-param search pattern already
/// covered by `fn_loop_variable_resolves_inside_its_own_body` below
/// (`fn find(vals: bits[8][4]) -> ... { loop i: 0..4 { ... } }`).
#[test]
fn foreach_elements_form_in_fn_body_resolves_via_own_param() {
    let src = "fn find(vals: bits[8][4]) -> bits[8] {\n  foreach v in vals {\n    if v == 0xFF { return v }\n  }\n  0\n}\nmodule M {\n  in a: bits[8]\n  in b: bits[8]\n  in c: bits[8]\n  in d: bits[8]\n  out o: bits[8]\n  o = find([a, b, c, d])\n}\n";
    check_one(src)
        .expect("fn-body foreach elements form over the fn's own array param must check clean");
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

/// An earlier-declared instance forward-referencing a later-declared
/// instance's unknown output field must still get E0104, regardless of
/// declaration order (`collect_decls`: "declaration order in a module is
/// free"). Same forward-reference shape as `a_cycle_through_instances_is_e0504`,
/// but `i2` is declared and unambiguous — the only issue is the typo'd
/// field `i2.zzz`, which `i1` (declared first) reads before `i2`'s own
/// `check_inst` has run.
#[test]
fn forward_reference_to_unknown_output_field_is_e0104() {
    let src = "module Inv {\n  in d: bit\n  out q: bit\n  q = !d\n}\nmodule M {\n  out y: bit\n  let i1 = Inv() { d: i2.zzz }\n  let i2 = Inv() { d: 0 }\n  y = i1.q\n}\n";
    let d = first_err(src, "E0104");
    assert!(d.msg.contains("zzz"));
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

// ---- tagged-union payload types + arity (E0103/E0806) ---------------------

#[test]
fn tagged_enum_unknown_payload_type_is_e0103() {
    // A module-level enum with an unrecognized payload type triggers E0103.
    let src = "module M {\n  enum Packet { Read(addr: bogustype) }\n  out y: bit\n  y = 0\n}\n";
    let d = first_err(src, "E0103");
    assert!(
        d.msg.contains("bogustype"),
        "error names the unknown type: {}",
        d.msg
    );
}

#[test]
fn tagged_enum_toplevel_unknown_payload_type_is_e0103() {
    // A top-level enum (TopItem::Enum) with an unrecognized payload type triggers E0103.
    let src = "enum Packet { Read(addr: bogustype) }\nmodule M {\n  out y: bit\n  y = 0\n}\n";
    let d = first_err(src, "E0103");
    assert!(
        d.msg.contains("bogustype"),
        "error names the unknown type: {}",
        d.msg
    );
}

#[test]
fn tagged_pattern_arity_mismatch_is_e0806() {
    // Read has 1 payload field but the pattern provides 2 bindings.
    let src = "enum Packet { Read(addr: bits[8]) }\nmodule M {\n  in x: Packet\n  out y: bit\n  y = match x {\n    Packet.Read(a, b) => 0\n    _ => 0\n  }\n}\n";
    let d = first_err(src, "E0806");
    assert!(d.msg.contains("Read"), "error names the variant: {}", d.msg);
}

#[test]
fn tag_only_pattern_with_bindings_is_e0806() {
    // Foo.A has no payload fields; providing a binding is E0806 (0 expected, 1 got).
    let src = "module M {\n  enum Foo { A, B }\n  in s: Foo\n  out y: bit\n  y = match s {\n    Foo.A(x) => 0\n    Foo.B => 1\n  }\n}\n";
    let d = first_err(src, "E0806");
    assert!(d.msg.contains("A"), "error names the variant: {}", d.msg);
}

#[test]
fn valid_tagged_pattern_compiles_clean() {
    // Exactly 1 binding for a 1-field variant — should be clean through all passes.
    let src = "enum Packet { Read(addr: bits[8]) }\nmodule M {\n  in x: Packet\n  out y: bits[8]\n  y = match x {\n    Packet.Read(a) => a\n    _ => 0\n  }\n}\n";
    check_one(src).expect("valid tagged pattern with correct arity compiles clean");
}

// ---- enum variant construction: name/variant/arity (T2) -------------------

#[test]
fn enum_construct_unknown_enum_name() {
    first_err(
        "module M {\n  out y: bit\n  y = NoSuchEnum.Variant()\n}\n",
        "E0101",
    );
}

#[test]
fn enum_construct_unknown_variant_name() {
    let src = "enum State { Idle, Running }\n\
               module M {\n  out y: State\n  y = State.NoSuchVariant()\n}\n";
    first_err(src, "E0103");
}

#[test]
fn enum_construct_arity_mismatch_is_e0806() {
    let src = "enum Packet { Ctrl(k: bits[4]) }\n\
               module M {\n  in k: bits[4]\n  out y: Packet\n  y = Packet.Ctrl(k, k)\n}\n";
    first_err(src, "E0806");
}

#[test]
fn enum_construct_tag_only_with_extra_args_is_e0806() {
    let src = "enum State { Idle, Running }\n\
               module M {\n  in a: bit\n  out y: State\n  y = State.Idle(a)\n}\n";
    first_err(src, "E0806");
}

#[test]
fn enum_construct_recurses_into_args_for_name_resolution() {
    // The argument `nosuch` is itself an unresolvable name — must be
    // caught even though the OUTER construction (Packet.Ctrl) is valid.
    let src = "enum Packet { Ctrl(k: bits[4]) }\n\
               module M {\n  out y: Packet\n  y = Packet.Ctrl(nosuch)\n}\n";
    first_err(src, "E0101");
}

#[test]
fn match_arm_binding_field_width_resolves_against_enum_declaring_file_not_match_site() {
    // Regression: `inject_arm_bindings` used to resolve a payload field's
    // type against the MATCH SITE's file consts, not the enum's own
    // declaring file — so a field type like `bits[W]`, where `W` is a
    // const declared only alongside the enum, silently resolved to
    // Ty::Unknown at a different file's match site (no `W` in scope
    // there). The anti-cascade rule then let that Unknown absorb any
    // width mismatch on the bound value with no diagnostic at all. File 0
    // declares `Packet` with a const-sized field; file 1 matches it with
    // no local `W` and assigns the (4-bit) binding to an 8-bit output —
    // must be caught as E0401, not silently pass.
    let file_a = parse("const W: int = 4\nenum Packet { Ctrl(k: bits[W]) }\n");
    let file_b = parse(
        "module M {\n  in p: Packet\n  out y: bits[8]\n  \
         y = match p {\n    Packet.Ctrl(a) => a\n  }\n}\n",
    );
    first_err_multi(&[file_a, file_b], "E0401");
}

// ---- tagged-union width checker (T4) ----------------------------------------

#[test]
fn tagged_enum_total_width_is_tag_plus_max_payload() {
    // Packet has 2 variants → tag = 1 bit (clog2(2)).
    // Read payload = bits[32] = 32 bits; Write payload = bits[32] + bits[32] = 64 bits.
    // max_payload = 64, total = tag(1) + max_payload(64) = 65 bits → [64:0].
    let src = concat!(
        "enum Packet { Read(addr: bits[32]), Write(addr: bits[32], data: bits[32]) }\n",
        "module M {\n",
        "  in x: Packet\n",
        "  out addr: bits[32]\n",
        "  addr = match x {\n",
        "    Packet.Read(a) => a\n",
        "    Packet.Write(a, b) => a\n",
        "  }\n",
        "}\n",
    );
    let verilog = compile_one(src)
        .expect("tagged enum with max_payload=64, tag=1 (total 65 bits) compiles clean");
    assert!(
        verilog.contains("[64:0]"),
        "expected 65-bit enum port ([64:0]), got:\n{verilog}"
    );
}

#[test]
fn pattern_binding_types_match_payload_fields() {
    // Packet.Read has addr: bits[32].  After binding injection, `a` is bits[32].
    // Driving a bits[8] output from `a` must fail with E0401 (32 ≠ 8).
    // Before this fix `a` resolved to Unknown and silenced the error.
    let src = concat!(
        "enum Packet { Read(addr: bits[32]) }\n",
        "module M {\n",
        "  in x: Packet\n",
        "  out y: bits[8]\n",
        "  y = match x {\n",
        "    Packet.Read(a) => a\n",
        "    _ => 0\n",
        "  }\n",
        "}\n",
    );
    let d = first_err(src, "E0401");
    assert!(
        d.msg.contains("bits[32]") && d.msg.contains("bits[8]"),
        "error must name both widths: {}",
        d.msg
    );
}

#[test]
fn enum_payload_enum_type_is_e0807() {
    // A payload field whose type is another enum violates E0807.
    let src = concat!(
        "enum Inner { A, B }\n",
        "enum Outer { Var(x: Inner) }\n",
        "module M {\n",
        "  out y: bit\n",
        "  y = 0\n",
        "}\n",
    );
    let d = first_err(src, "E0807");
    assert!(
        d.msg.contains("x"),
        "error names the payload field: {}",
        d.msg
    );
}

#[test]
fn enum_payload_array_type_is_e0807() {
    // A payload field whose type is an array violates E0807 (not a bit-vector).
    let src = concat!(
        "enum V { A(a: bits[8][4], b: bits[8]) }\n",
        "module M {\n",
        "  out o: bit\n",
        "  o = 0\n",
        "}\n",
    );
    let d = first_err(src, "E0807");
    assert!(
        d.msg.contains("a"),
        "error names the payload field: {}",
        d.msg
    );
}

// ---- enum variant construction: arg widths + Ty::Enum inference (T3) ------

#[test]
fn enum_construct_wrong_arg_width_is_e0401() {
    let src = "enum Packet { Ctrl(k: bits[4]) }\n\
               module M {\n  in k: bits[8]\n  out y: Packet\n  y = Packet.Ctrl(k)\n}\n";
    first_err(src, "E0401");
}

#[test]
fn enum_construct_valid_use_checks_clean_and_infers_enum_ty() {
    // Also proves the constructed value is usable as an ordinary
    // enum-typed value: assigned to a `Packet`-typed output.
    let src = "enum Packet { Ctrl(k: bits[4]), Data(v: bits[8]) }\n\
               module M {\n  in k: bits[4]\n  out y: Packet\n  y = Packet.Ctrl(k)\n}\n";
    check_one(src).expect("valid EnumConstruct must check clean");
}

#[test]
fn enum_construct_literal_arg_adapts_to_field_width() {
    // An unsized literal argument adapts to the field's declared width,
    // same as any other typed boundary (fn call arg, port connection) —
    // must NOT trip E0401.
    let src = "enum Packet { Ctrl(k: bits[4]) }\n\
               module M {\n  out y: Packet\n  y = Packet.Ctrl(3)\n}\n";
    check_one(src).expect("a literal argument must adapt to the field width");
}

// ---- enum variant construction: emitter concat lowering (T5) --------------

#[test]
fn enum_construct_emits_tag_and_payload_concat() {
    // Packet has 2 variants (tag_w = 1), max payload = max(4, 8) = 8 bits,
    // total = 9 bits. Ctrl's own payload (k, 4 bits) is narrower than the
    // 8-bit max payload, so 4 zero-padding bits fill the low end.
    let v = compile_one(
        "enum Packet {\n  Ctrl(k: bits[4]),\n  Data(v: bits[8])\n}\n\
         module M {\n  in k: bits[4]\n  out y: Packet\n  y = Packet.Ctrl(k)\n}\n",
    )
    .expect("compiles clean");
    assert!(
        v.contains("1'd0, k, 4'd0"),
        "expected tag(0)+k+4-bit zero pad, got:\n{v}"
    );
}

#[test]
fn enum_construct_literal_arg_is_sized_to_field_width_in_concat() {
    // Regression: an unsized literal inside a `{}` concatenation defaults
    // to 32 bits per the Verilog LRM — `3` must be rendered `4'd3`, not a
    // bare `3`, or it silently overruns the 4-bit field into neighboring
    // tag/padding bits. Packet has exactly 1 variant (tag_w = 1, no
    // padding), so the concat is just the tag and the sized literal.
    let v = compile_one(
        "enum Packet {\n  Ctrl(k: bits[4])\n}\n\
         module M {\n  out y: Packet\n  y = Packet.Ctrl(3)\n}\n",
    )
    .expect("compiles clean");
    assert!(
        v.contains("{1'd0, 4'd3}"),
        "expected a 4-bit-sized literal, got:\n{v}"
    );
}

#[test]
fn enum_construct_negative_literal_arg_is_masked_and_sized_not_left_bare() {
    // Regression: the first fix only special-cased `ExprKind::Int`, missing
    // `-3` (parses as `Unary{Neg, Int(3)}`) and other constant-foldable
    // shapes — those fell through to `expr_subst`'s ordinary rendering, an
    // unsized `-3` inside a `{}` concat (invalid Verilog, and even if
    // accepted would default to 32 bits, silently corrupting the layout).
    // -3 in a 4-bit two's-complement field is 0b1101 = 13.
    let v = compile_one(
        "enum Packet {\n  Ctrl(k: signed[4])\n}\n\
         module M {\n  out y: Packet\n  y = Packet.Ctrl(-3)\n}\n",
    )
    .expect("compiles clean");
    assert!(
        v.contains("{1'd0, 4'd13}"),
        "expected -3 masked to its 4-bit two's-complement pattern (13), got:\n{v}"
    );
}

#[test]
fn enum_construct_tag_only_zero_args_emits_bare_tag() {
    let v = compile_one(
        "enum State {\n  Idle,\n  Running\n}\n\
         module M {\n  out y: State\n  y = State.Idle()\n}\n",
    )
    .expect("compiles clean");
    assert!(v.contains("y = "), "expected an assign for y, got:\n{v}");
}

// -------- E0808: OR-arm binding intersection --------

/// Enum with four variants — used across E0808 tests.
/// OR-pattern separator in this language is `,` (not `|`).
const OP: &str = concat!(
    "enum Op { Add(a: bits[8], b: bits[8]), Sub(a: bits[8], b: bits[8]),",
    " Big(x: bits[16]), Nop }\n",
);

#[test]
fn or_arm_same_names_same_widths_is_clean() {
    let src = format!(
        concat!(
            "{OP}module M {{\n",
            "  in cmd: Op\n",
            "  out o: bits[8]\n",
            "  o = match cmd {{\n",
            "    Op.Add(a, b), Op.Sub(a, b) => a\n",
            "    _ => 0\n",
            "  }}\n",
            "}}\n",
        ),
        OP = OP
    );
    check_one(&src).expect("identical OR-arm bindings compile clean");
}

#[test]
fn or_arm_three_alts_same_bindings_is_clean() {
    check_one(concat!(
        "enum T { A(x: bits[8]), B(x: bits[8]), C(x: bits[8]) }\n",
        "module M {\n",
        "  in cmd: T\n",
        "  out o: bits[8]\n",
        "  o = match cmd {\n",
        "    T.A(x), T.B(x), T.C(x) => x\n",
        "    _ => 0\n",
        "  }\n",
        "}\n",
    ))
    .expect("3-way OR-arm with identical bindings compiles clean");
}

#[test]
fn or_arm_different_names_is_e0808() {
    let src = format!(
        concat!(
            "{OP}module M {{\n",
            "  in cmd: Op\n",
            "  out o: bits[8]\n",
            "  o = match cmd {{\n",
            "    Op.Add(a, b), Op.Big(x) => a\n",
            "    _ => 0\n",
            "  }}\n",
            "}}\n",
        ),
        OP = OP
    );
    first_err(&src, "E0808");
}

#[test]
fn or_arm_tag_only_alt_is_e0808() {
    let src = format!(
        concat!(
            "{OP}module M {{\n",
            "  in cmd: Op\n",
            "  out o: bits[8]\n",
            "  o = match cmd {{\n",
            "    Op.Add(a, b), Op.Nop => a\n",
            "    _ => 0\n",
            "  }}\n",
            "}}\n",
        ),
        OP = OP
    );
    first_err(&src, "E0808");
}

#[test]
fn or_arm_subset_binding_is_e0808() {
    // Full(a,b) has arity 2, Half(a) has arity 1 — name-set mismatch → E0808.
    first_err(
        concat!(
            "enum Op2 { Full(a: bits[8], b: bits[8]), Half(a: bits[8]) }\n",
            "module M {\n",
            "  in cmd: Op2\n",
            "  out o: bits[8]\n",
            "  o = match cmd {\n",
            "    Op2.Full(a, b), Op2.Half(a) => a\n",
            "    _ => 0\n",
            "  }\n",
            "}\n",
        ),
        "E0808",
    );
}

#[test]
fn or_arm_width_mismatch_is_e0808() {
    first_err(
        concat!(
            "enum Op3 { Big(x: bits[16]), Small(x: bits[8]) }\n",
            "module M {\n",
            "  in cmd: Op3\n",
            "  out o: bits[8]\n",
            "  o = match cmd {\n",
            "    Op3.Big(x), Op3.Small(x) => x[7:0]\n",
            "    _ => 0\n",
            "  }\n",
            "}\n",
        ),
        "E0808",
    );
}

#[test]
fn e0809_default_target_not_reg() {
    first_err(
        "module M {\n  clock clk\n  wire w: bit = 0\n  on rise(clk) {\n    default w <- 0\n  }\n}\n",
        "E0809",
    );
}

#[test]
fn e0810_duplicate_default() {
    first_err(
        "module M {\n  clock clk\n  reset rst\n  reg r: bit = 0\n  on rise(clk) {\n    default r <- 0\n    default r <- 1\n  }\n}\n",
        "E0810",
    );
}

#[test]
fn e0811_const_if_condition_not_const() {
    first_err(
        "module M {\n  in a: bit\n  out b: bit\n  const if (a) {\n    wire extra: bit = 0\n  }\n  b = 0\n}\n",
        "E0811",
    );
}

#[test]
fn e0813_fn_let_shadow_width_mismatch() {
    // BUG-9: `x` is first 8 bits, then re-bound to 16 via `extend` — two
    // widths under one name can't share a single Verilog `reg` declaration.
    first_err(
        "fn bump(a: bits[8]) -> bits[16] {\n  let x = a\n  let x = extend(x, 16)\n  x\n}\n\
         module M {\n  in a: bits[8]\n  out y: bits[16]\n  y = bump(a)\n}\n",
        "E0813",
    );
}

#[test]
fn fn_let_shadow_same_width_stays_clean() {
    // The common fold/accumulator idiom (foreach_sum.mimz's `let acc = acc
    // +% v`) re-binds a name at the SAME width — must NOT trip E0813.
    let src = "fn bump(a: bits[8]) -> bits[8] {\n  let x = a\n  let x = x +% 1\n  x\n}\n\
               module M {\n  in a: bits[8]\n  out y: bits[8]\n  y = bump(a)\n}\n";
    check_one(src).expect("same-width fn-body let shadowing must check clean");
}

#[test]
fn fn_let_shadowing_a_param_at_a_different_width_is_e0813() {
    // Shadowing a PARAM (not just an earlier `let`) at a different width
    // is the same conflict — the param's own `input` port already claims
    // the name at its declared width.
    first_err(
        "fn bump(acc: bits[8]) -> bits[16] {\n  let acc = extend(acc, 16)\n  acc\n}\n\
         module M {\n  in a: bits[8]\n  out y: bits[16]\n  y = bump(a)\n}\n",
        "E0813",
    );
}

#[test]
fn or_arm_wildcard_not_binding_e0808() {
    let src = format!(
        concat!(
            "{OP}module M {{\n",
            "  in cmd: Op\n",
            "  out o: bits[8]\n",
            "  o = match cmd {{\n",
            "    Op.Add(a, b), _ => a\n",
            "    _ => 0\n",
            "  }}\n",
            "}}\n",
        ),
        OP = OP
    );
    first_err(&src, "E0808");
}

// ---- bundles: registration and field validation (E0906, E0909) ----------------

#[test]
fn bundle_duplicate_name_is_e0909() {
    first_err(
        "bundle Foo { valid: bit }\nbundle Foo { ready: bit }\nmodule Top { out z: bit\n  z = 0 }\n",
        "E0909",
    );
}

#[test]
fn bundle_clean_declaration_passes() {
    check_one("bundle Bus { valid: bit, data: bits[8] }\nmodule Top { out z: bit\n  z = 0 }\n")
        .expect("a well-formed bundle with concrete field types passes all passes");
}

#[test]
fn bundle_named_field_as_module_port_passes() {
    // Type::Named("Bus") where Bus is a registered bundle — must not emit E0103.
    check_one("bundle Bus { valid: bit }\nmodule Top { in x: Bus\n  out z: bit\n  z = 0 }\n")
        .expect("a bundle-typed module port resolves without E0103");
}

#[test]
fn bundle_unknown_parametric_type_in_field_is_e0906() {
    // Type::Bundle { name: "NoSuchBundle" } in a bundle field → E0906 (unknown bundle).
    first_err(
        "bundle Bad { x: NoSuchBundle(W: 32) }\nmodule Top { out z: bit\n  z = 0 }\n",
        "E0906",
    );
}

#[test]
fn bundle_nested_bundle_field_is_e0807() {
    // A field whose type is a known bundle name → E0807 (nested bundle, non-concrete type).
    first_err(
        "bundle Inner { v: bit }\nbundle Outer { x: Inner }\nmodule Top { out z: bit\n  z = 0 }\n",
        "E0807",
    );
}

#[test]
fn bundle_array_field_is_e0807() {
    // A field whose type is an array → E0807 (array not a concrete bundle field type).
    first_err(
        "bundle Bad { f: bits[8][4] }\nmodule Top { out z: bit\n  z = 0 }\n",
        "E0807",
    );
}

// ---- bundles: literal / destructure / nominal typing (E0901-E0903, E0907) ------

#[test]
fn bundle_literal_missing_field() {
    first_err(
        r#"
bundle Hs { valid: bit, data: bits[8] }
module Top {
  out dst: Hs
  dst = { valid: 1 }
}
"#,
        "E0901",
    );
}

#[test]
fn bundle_literal_unknown_field() {
    first_err(
        r#"
bundle Hs { valid: bit }
module Top {
  out dst: Hs
  dst = { valid: 1, extra: 0 }
}
"#,
        "E0902",
    );
}

#[test]
fn bundle_type_mismatch() {
    first_err(
        r#"
bundle A { valid: bit, data: bits[4] }
bundle B { valid: bit, data: bits[8] }
module Top {
  in x: A
  out y: B
  y = x
}
"#,
        "E0907",
    );
}

#[test]
fn structurally_compatible_bundles_check_clean_in_a_drive() {
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle SensorData { tx: bit, rx: bit }\n\
               module M {\n  in  a_tx: bit\n  in  a_rx: bit\n  \
               out b_tx: bit\n  out b_rx: bit\n  \
               wire a: SensorData = { tx: a_tx, rx: a_rx }\n  \
               out b: HasUART\n  \
               b = a\n  b_tx = b.tx\n  b_rx = b.rx\n}\n";
    check_one(src).expect("structurally-compatible differently-named bundles must check clean");
}

#[test]
fn structurally_compatible_bundle_with_extra_fields_checks_clean() {
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle SensorData { tx: bit, rx: bit, power: bit }\n\
               module M {\n  in  a_tx: bit\n  in  a_rx: bit\n  in a_pw: bit\n  \
               out b_tx: bit\n  \
               wire a: SensorData = { tx: a_tx, rx: a_rx, power: a_pw }\n  \
               out b: HasUART\n  \
               b = a\n  b_tx = b.tx\n}\n";
    check_one(src)
        .expect("a provided bundle with EXTRA fields beyond what's required must check clean");
}

#[test]
fn drive_bundle_missing_required_field_is_e0910() {
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle Partial { tx: bit }\n\
               module M {\n  in  a_tx: bit\n  out b_tx: bit\n  out b_rx: bit\n  \
               wire a: Partial = { tx: a_tx }\n  \
               out b: HasUART\n  \
               b = a\n  b_tx = b.tx\n  b_rx = b.rx\n}\n";
    let d = first_err(src, "E0910");
    assert!(
        d.msg.contains("rx"),
        "expected the missing field `rx` named in the message, got: {}",
        d.msg
    );
}

#[test]
fn drive_bundle_shared_field_wrong_width_is_e0907() {
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               bundle Wrong { tx: bits[4], rx: bit }\n\
               module M {\n  in  a_tx: bits[4]\n  in a_rx: bit\n  \
               out b_tx: bit\n  out b_rx: bit\n  \
               wire a: Wrong = { tx: a_tx, rx: a_rx }\n  \
               out b: HasUART\n  \
               b = a\n  b_tx = b.tx\n  b_rx = b.rx\n}\n";
    let d = first_err(src, "E0907");
    assert!(
        d.msg.contains("tx"),
        "expected the mismatched field `tx` named in the message, got: {}",
        d.msg
    );
}

#[test]
fn drive_bundle_same_name_regression_still_checks_clean() {
    // Regression: two bundle declarations can never share a name (E0909
    // forbids it), so "same name" always means "the same declaration" —
    // there is no "same name, still a mismatch" case to regress. This
    // proves the ORIGINAL same-name case (both sides ARE `HasUART`) still
    // checks clean exactly as before — the dedup didn't change the
    // trivial-self-compatibility case.
    let src = "bundle HasUART { tx: bit, rx: bit }\n\
               module M {\n  in  a_tx: bit\n  in a_rx: bit\n  \
               out b_tx: bit\n  out b_rx: bit\n  \
               wire a: HasUART = { tx: a_tx, rx: a_rx }\n  \
               out b: HasUART\n  \
               b = a\n  b_tx = b.tx\n  b_rx = b.rx\n}\n";
    check_one(src).expect("same-name bundle assignment must still check clean");
}

#[test]
fn bundle_destructure_duplicate_binding() {
    first_err(
        r#"
bundle Hs { valid: bit, ready: bit }
module Top {
  in bus: Hs
  let { valid, valid } = bus
}
"#,
        "E0903",
    );
}

#[test]
fn two_same_named_modules_each_get_their_own_driver_check() {
    // file A's `Fifo` has a real double-drive bug; file B's `Fifo` (same
    // name, different file) is clean. Before the (file,name) re-key, the
    // driver-safety cache keyed by bare name could return file A's — or
    // file B's — Summary for BOTH instantiations, either missing A's real
    // bug or (nondeterministically) flagging B's clean one.
    let a = parse("module Fifo {\n  out y: bit\n  y = 1\n  y = 0\n}\n"); // double-drive
    let b = parse("module Fifo {\n  out y: bit\n  y = 0\n}\n"); // clean
    let mut user = parse("module M {\n  let x = Fifo() { }\n  let z = Fifo() { }\n}\n");
    // Wire the two Insts to different files by hand (mirrors Task 5's
    // qualified-resolution tests — real end-to-end qualification is
    // exercised in Task 9's fixtures).
    if let crate::ast::TopItem::Module(m) = &mut user.items[0] {
        let mut insts = m.items.iter_mut().filter_map(|it| {
            if let crate::ast::ModuleItem::Inst(i) = it {
                Some(i)
            } else {
                None
            }
        });
        let x = insts.next().unwrap();
        x.module.path.push(crate::ast::Ident {
            name: "a".into(),
            span: x.module.span,
        });
        x.module.resolved_file.set(Some(1)); // -> file A (buggy)
        let z = insts.next().unwrap();
        z.module.path.push(crate::ast::Ident {
            name: "b".into(),
            span: z.module.span,
        });
        z.module.resolved_file.set(Some(2)); // -> file B (clean)
    }
    let diags = errs_multi(&[user, a, b]);
    assert!(
        diags.iter().any(|d| d.code == Some("E0501")),
        "file A's real double-drive bug must still be caught even though \
         file B declares a same-named, clean `Fifo`"
    );
}

#[test]
fn two_same_named_modules_each_get_their_own_width_check() {
    // `self.modules["Fifo"]` lists file-registration order: `a` (file 1)
    // registers before `b` (file 2), so `a` is whatever the OLD
    // canonical-filter/`.first()` code always resolved to. To get a
    // GENUINE red (not a pass-by-luck green), the real bug must live in
    // `b` (the second-registered, non-canonical file) — under the old
    // widths/mod.rs seeding loop + `.first()` worklist resolution, `b`'s
    // `Fifo` is never seeded at all, so its internal E0401 is silently
    // skipped regardless of which Fifo `M` actually instantiates.
    let a = parse("module Fifo {\n  out y: bits[4]\n  wire w: bits[4] = 0\n  y = w\n}\n"); // clean
    let b = parse("module Fifo {\n  out y: bits[4]\n  wire w: bits[2] = 0\n  y = w\n}\n"); // width mismatch
    let mut user = parse("module M {\n  let x = Fifo() { }\n  let z = Fifo() { }\n}\n");
    if let crate::ast::TopItem::Module(m) = &mut user.items[0] {
        let mut insts = m.items.iter_mut().filter_map(|it| {
            if let crate::ast::ModuleItem::Inst(i) = it {
                Some(i)
            } else {
                None
            }
        });
        let x = insts.next().unwrap();
        x.module.path.push(crate::ast::Ident {
            name: "a".into(),
            span: x.module.span,
        });
        x.module.resolved_file.set(Some(1)); // -> file A (clean)
        let z = insts.next().unwrap();
        z.module.path.push(crate::ast::Ident {
            name: "b".into(),
            span: z.module.span,
        });
        z.module.resolved_file.set(Some(2)); // -> file B (real E0401)
    }
    let diags = errs_multi(&[user, a, b]);
    assert!(
        diags.iter().any(|d| d.code == Some("E0401")),
        "file B's real width mismatch must still be caught even though \
         file A declares a same-named, clean `Fifo` that registers first"
    );
}

#[test]
fn two_same_named_modules_each_get_their_own_clock_check() {
    // Same file-registration-order concern as the width-check sibling
    // above: `check_clocks`'s old canonical filter compared each module
    // against `self.modules[name].first()`, i.e. always the
    // FIRST-registered file. The real cross-domain bug must live in the
    // second-registered file (`b`) to prove the filter's removal, not
    // just happen to land on whichever file the old filter already let
    // through.
    let a = parse(
        "module Fifo {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out ya: bit\n  out yb: bit\n  reg ra: bit = 0\n  reg rb: bit = 0\n  on rise(cka) {\n    ra <- a\n  }\n  on rise(ckb) {\n    rb <- a\n  }\n  ya = ra\n  yb = rb\n}\n",
    ); // clean: independent domains
    let b = parse(
        "module Fifo {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out yb: bit\n  reg ra: bit = 0\n  reg rb: bit = 0\n  on rise(cka) {\n    ra <- a\n  }\n  on rise(ckb) {\n    rb <- ra\n  }\n  yb = rb\n}\n",
    ); // real E0701: ckb-block reads cka-owned `ra`
    let mut user = parse("module M {\n  let x = Fifo() { a: 0 }\n  let z = Fifo() { a: 0 }\n}\n");
    if let crate::ast::TopItem::Module(m) = &mut user.items[0] {
        let mut insts = m.items.iter_mut().filter_map(|it| {
            if let crate::ast::ModuleItem::Inst(i) = it {
                Some(i)
            } else {
                None
            }
        });
        let x = insts.next().unwrap();
        x.module.path.push(crate::ast::Ident {
            name: "a".into(),
            span: x.module.span,
        });
        x.module.resolved_file.set(Some(1)); // -> file A (clean)
        let z = insts.next().unwrap();
        z.module.path.push(crate::ast::Ident {
            name: "b".into(),
            span: z.module.span,
        });
        z.module.resolved_file.set(Some(2)); // -> file B (real E0701)
    }
    let diags = errs_multi(&[user, a, b]);
    assert!(
        diags.iter().any(|d| d.code == Some("E0701")),
        "file B's real cross-domain read must still be caught even though \
         file A declares a same-named, clean `Fifo` that registers first"
    );
}

// ----- Task 15 sweep: regression tests for previously hand-verified-only -----
// ----- behaviors (see .superpowers/sdd/progress.md's "Minor (deferred..." -----
// ----- lines for Tasks 9 and 12).                                        -----

#[test]
fn overlapping_import_prefixes_disambiguate_correctly() {
    // Task 9's deferred gap: both `import a` and `import a.b` present in the
    // same file, with one reference down each path (`a.Fifo` and
    // `a.b.Fifo`). Previously only verified by a reviewer's hand-trace of
    // `resolve_via_imports`'s exact-length guard (`imp.path.len() ==
    // self.path.len()`) — this pins it down as a real test.
    //
    // Each file's `Fifo` has a differently-named input port (`x` vs `w`), so
    // a prefix-only match (e.g. `a.b.Fifo` incorrectly matching `import a`)
    // would connect the wrong port name and trip E0107 — this isn't a
    // tautological clean-pass, it actually exercises which target each
    // qualifier resolved to.
    let a = parse("module Fifo {\n  in x: bit\n  out y: bit\n  y = x\n}\n");
    let b = parse("module Fifo {\n  in w: bit\n  out z: bit\n  z = w\n}\n");
    let user = parse(
        "import a\nimport a.b\n\nmodule M {\n  let u1 = a.Fifo() { x: 0 }\n  \
         let u2 = a.b.Fifo() { w: 0 }\n}\n",
    );
    assert_eq!(user.imports.len(), 2, "sanity: both imports parsed");
    // files: [user=0, a=1, b=2].
    user.imports[0].resolved_file.set(Some(1)); // `import a` -> file 1
    user.imports[1].resolved_file.set(Some(2)); // `import a.b` -> file 2
    check(&[user, a, b]).expect(
        "a.Fifo() must resolve to file 1's Fifo (input `x`) and a.b.Fifo() \
         must resolve to file 2's Fifo (input `w`) — a prefix-only match \
         would misconnect one of them and trip E0107",
    );
}

#[test]
fn no_default_param_module_only_discovered_via_instantiation_still_gets_width_checked() {
    // Task 12's deferred gap: both existing "two same-named modules" width
    // tests use param-less `Fifo`s, so `check_widths`'s unconditional seed
    // loop discovers both files' configs on its own — `check_inst_widths`'s
    // `found.push((child.file, ...))` never gets exercised as the SOLE
    // discovery path. A module with a parameter that has no default is
    // skipped by the seed loop entirely (`default_binding` requires every
    // param to have a default) — the ONLY way its body ever gets checked is
    // via `found.push` threading the correct `child.file` back into the
    // worklist from an instantiation site.
    //
    // Two different files each declare `Fifo(WIDTH: int)` (no default): one
    // has a real internal width bug (independent of WIDTH's value), the
    // other is clean. `user` instantiates both via a qualified reference. If
    // `found.push` dropped or mis-threaded `child.file` (the original Task
    // 12 bug), the pushed `Config` would fail the `(file, name)` lookup in
    // `check_widths`'s worklist and the buggy file's body would silently
    // never be checked — this test would then wrongly pass as clean.
    let buggy = parse(
        "module Fifo(WIDTH: int) {\n  out y: bits[WIDTH]\n  wire w: bits[WIDTH + 1] = 0\n  \
         y = w\n}\n",
    );
    let clean = parse("module Fifo(WIDTH: int) {\n  out y: bits[WIDTH]\n  y = 0\n}\n");
    let user = parse(
        "import buggy\nimport clean\n\nmodule M {\n  \
         let a = buggy.Fifo(WIDTH: 4) { }\n  let b = clean.Fifo(WIDTH: 4) { }\n}\n",
    );
    assert_eq!(user.imports.len(), 2, "sanity: both imports parsed");
    // files: [user=0, buggy=1, clean=2].
    user.imports[0].resolved_file.set(Some(1));
    user.imports[1].resolved_file.set(Some(2));
    let diags = errs_multi(&[user, buggy, clean]);
    assert!(
        diags.iter().any(|d| d.code == Some("E0401")),
        "buggy.Fifo's internal width mismatch must be caught even though it \
         has no default parameter and is only reachable via instantiation \
         (not the seed loop): got {diags:?}"
    );
}

#[test]
fn two_same_named_modules_each_get_their_own_clock_check_reversed_order() {
    // Task 12's deferred gap: the sibling test above
    // (`two_same_named_modules_each_get_their_own_clock_check`) always
    // registers the CLEAN module first (file 1) and the buggy one second
    // (file 2) — matching the old canonical filter's `.first()` pick, so it
    // only proves the second-registered file gets its own check. It can't
    // tell that apart from a hypothetically different-but-still-wrong
    // implementation that always checks only the LAST-registered file
    // instead of every file independently. Reversing which file holds the
    // bug (buggy registers FIRST here, clean SECOND) closes that gap: an
    // "always check the last file" implementation would report no E0701
    // here (since the last-registered file, clean, has no bug) and this
    // test would catch that.
    let a = parse(
        "module Fifo {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out yb: bit\n  \
         reg ra: bit = 0\n  reg rb: bit = 0\n  on rise(cka) {\n    ra <- a\n  }\n  \
         on rise(ckb) {\n    rb <- ra\n  }\n  yb = rb\n}\n",
    ); // real E0701: ckb-block reads cka-owned `ra` — registers FIRST (file 1)
    let b = parse(
        "module Fifo {\n  clock cka\n  clock ckb\n  reset rst\n  in a: bit\n  out ya: bit\n  \
         out yb: bit\n  reg ra: bit = 0\n  reg rb: bit = 0\n  on rise(cka) {\n    ra <- a\n  }\n  \
         on rise(ckb) {\n    rb <- a\n  }\n  ya = ra\n  yb = rb\n}\n",
    ); // clean: independent domains — registers SECOND (file 2)
    let mut user = parse("module M {\n  let x = Fifo() { a: 0 }\n  let z = Fifo() { a: 0 }\n}\n");
    if let crate::ast::TopItem::Module(m) = &mut user.items[0] {
        let mut insts = m.items.iter_mut().filter_map(|it| {
            if let crate::ast::ModuleItem::Inst(i) = it {
                Some(i)
            } else {
                None
            }
        });
        let x = insts.next().unwrap();
        x.module.path.push(crate::ast::Ident {
            name: "a".into(),
            span: x.module.span,
        });
        x.module.resolved_file.set(Some(1)); // -> file A (real E0701)
        let z = insts.next().unwrap();
        z.module.path.push(crate::ast::Ident {
            name: "b".into(),
            span: z.module.span,
        });
        z.module.resolved_file.set(Some(2)); // -> file B (clean)
    }
    let diags = errs_multi(&[user, a, b]);
    assert!(
        diags.iter().any(|d| d.code == Some("E0701")),
        "file A's real cross-domain read must still be caught even though \
         it registers first and file B declares a same-named, clean `Fifo` \
         that registers second"
    );
}

#[test]
fn recursive_call_inside_return_is_e0805() {
    // The self-call is nested inside `if { return f(a) }` — only reachable
    // by walking `FuncDecl.stmts` (Task 1's statement-based fn body), not
    // the old flat `locals`/`body` fields.
    let src = "fn f(a: bits[8]) -> bits[8] {\n  if a[0] == 1 { return f(a) }\n  a\n}\nmodule M {\n  in a: bits[8]\n  out o: bits[8]\n  o = f(a)\n}\n";
    first_err(src, "E0805");
}

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
