use super::*;

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
    // `2^127-1 + 1 = 2^127` used to overflow the old i128 ceiling; under
    // BUG-13 layer 2's MAX_WIDTH=1,000,000 ceiling it folds cleanly (only
    // 128 bits) — push well past the new ceiling instead.
    let src = "const HUGE: int = 1 << 1000001\nmodule M {\n}\n";
    first_err(src, "E0202");
}

#[test]
fn reg_without_reset_declaration_is_e0301() {
    let src = "module M {\n  clock clk\n  reg v: bit = 0\n  on rise(clk) {\n    v <- 1\n  }\n}\n";
    let d = first_err(src, "E0301");
    assert!(d.help.unwrap().contains("reset"));
}
