use super::*;

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

#[test]
fn builtin_valid_bundle_resolves_by_name() {
    // No `bundle __Valid` declaration anywhere in this source — it must
    // still resolve, proving the synthesized registration works.
    let src = "module M {\n  in a: bit\n  in d: bits[8]\n  \
               out o: bit\n  \
               wire x: __Valid(N: 8) = { valid: a, data: d }\n  \
               o = x.valid\n}\n";
    check_one(src).expect("the compiler-synthesized __Valid bundle must resolve");
}

#[test]
fn builtin_valid_signed_bundle_resolves_by_name() {
    let src = "module M {\n  in a: bit\n  in d: signed[8]\n  \
               out o: bit\n  \
               wire x: __ValidSigned(N: 8) = { valid: a, data: d }\n  \
               o = x.valid\n}\n";
    check_one(src).expect("the compiler-synthesized __ValidSigned bundle must resolve");
}

#[test]
fn qq_unwrap_form_types_as_the_data_field_type() {
    let src = "module M {\n  in c: bit\n  in d: bits[8]\n  out o: bits[8]\n  \
               wire x: bits[8]? = { valid: c, data: d }\n  \
               o = x ?? 0\n}\n";
    check_one(src).expect("unwrap form (T? ?? plain T) must check clean");
}

#[test]
fn qq_or_mux_form_types_as_still_optional() {
    let src = "module M {\n  in c1: bit\n  in d1: bits[8]\n  \
               in c2: bit\n  in d2: bits[8]\n  out o: bit\n  \
               wire x: bits[8]? = { valid: c1, data: d1 }\n  \
               wire y: bits[8]? = { valid: c2, data: d2 }\n  \
               wire merged: bits[8]? = x ?? y\n  o = merged.valid\n}\n";
    check_one(src).expect("OR-mux form (T? ?? T?) must check clean, result stays T?");
}

#[test]
fn qq_lhs_not_optional_is_e0911() {
    let src = "module M {\n  in a: bits[8]\n  out o: bits[8]\n  \
               o = a ?? 0\n}\n"; // `a` is plain bits[8], not bits[8]?
    let d = first_err(src, "E0911");
    assert!(d.msg.to_lowercase().contains("valid-bundle") || d.msg.contains("??"));
}

#[test]
fn qq_rhs_wrong_width_is_e0912() {
    let src = "module M {\n  in c: bit\n  in d: bits[8]\n  out o: bits[4]\n  \
               wire x: bits[8]? = { valid: c, data: d }\n  \
               o = x ?? 0\n}\n"; // `o` forces the unwrap result to bits[4], x's data is bits[8]
    let d = first_err(src, "E0912");
    assert!(!d.msg.is_empty());
}

#[test]
fn builtin_valid_bundle_shows_as_surface_syntax_in_diagnostics() {
    // NOTE: this does NOT round-trip through E0912 (`??`'s RHS mismatch) —
    // that diagnostic names `data_ty` (already unwrapped to `bits[8]`, see
    // `coalesce_ty`/`qq_rhs_mismatch` in checker/widths/ops.rs), never the
    // outer `Ty::Bundle{name: "__Valid", ..}` itself, so a test built around
    // E0912 would trivially pass with no fix at all. Instead: assigning a
    // plain `bits[8]` value to a `bits[8]?`-typed wire is a genuine
    // bundle-vs-non-bundle mismatch, which DOES render the whole LHS type via
    // `show()` in `expect_ty`'s generic fallback (E0401) — this is the path
    // that actually leaked `` bundle `__Valid` `` before the fix.
    let src = "module M {\n  in a: bits[8]\n  wire x: bits[8]? = a\n}\n";
    let d = first_err(src, "E0401");
    assert!(
        !d.msg.contains("__Valid"),
        "internal builtin bundle name leaked into a diagnostic: {}",
        d.msg
    );
    assert!(
        d.msg.contains("bits[8]?"),
        "expected the surface `bits[8]?` syntax in the diagnostic, got: {}",
        d.msg
    );
}

#[test]
fn builtin_valid_bundle_bit_question_collapses_to_bit_in_diagnostics() {
    // Test the N=1 case: `__Valid` with N=1 must render as `bit?`, not `bits[1]?`.
    // Same mechanism as the `bits[8]?` test above: assigning a plain `bit` value
    // to a `bit?`-typed wire is a bundle-vs-non-bundle mismatch that renders the
    // whole LHS type via `show()` in E0401.
    let src = "module M {\n  in a: bit\n  wire x: bit? = a\n}\n";
    let d = first_err(src, "E0401");
    assert!(
        !d.msg.contains("__Valid"),
        "internal builtin bundle name leaked into a diagnostic: {}",
        d.msg
    );
    assert!(
        d.msg.contains("`bit?`"),
        "expected the surface `bit?` syntax in the diagnostic, got: {}",
        d.msg
    );
    assert!(
        !d.msg.contains("bits[1]"),
        "expected `bit?` not `bits[1]?` in the diagnostic, got: {}",
        d.msg
    );
}

#[test]
fn builtin_valid_signed_bundle_shows_as_surface_syntax_in_diagnostics() {
    // Test the `__ValidSigned` case: must render as `signed[N]?`.
    // Same mechanism as the `bits[N]?` tests above: assigning a plain `signed[8]`
    // value to a `signed[8]?`-typed wire is a bundle-vs-non-bundle mismatch that
    // renders the whole LHS type via `show()` in E0401.
    let src = "module M {\n  in a: signed[8]\n  wire x: signed[8]? = a\n}\n";
    let d = first_err(src, "E0401");
    assert!(
        !d.msg.contains("__ValidSigned"),
        "internal builtin bundle name leaked into a diagnostic: {}",
        d.msg
    );
    assert!(
        d.msg.contains("`signed[8]?`"),
        "expected the surface `signed[8]?` syntax in the diagnostic, got: {}",
        d.msg
    );
}

#[test]
fn qq_same_shaped_user_bundle_satisfies_a_valid_bundle_slot() {
    // Accepted, intentional consequence of desugaring to the existing
    // structural-bundle machinery (feature 2.9) — pinned as a regression
    // test per the design spec, not left as prose.
    let src = "bundle MyOptional { valid: bit, data: bits[8] }\n\
               module M {\n  in c: bit\n  in d: bits[8]\n  out o: bits[8]\n  \
               wire x: MyOptional = { valid: c, data: d }\n  \
               o = x ?? 0\n}\n";
    check_one(src)
        .expect("a user bundle shaped exactly like bits[8]? must satisfy ?? structurally");
}

#[test]
fn qq_lhs_missing_valid_field_is_e0911() {
    // `NoValid` has a `data` field but no `valid` field at all — not a
    // valid-bundle shape, must be rejected the same as any other
    // non-optional LHS (E0911), not silently accepted just because a
    // `data` field happens to exist.
    let src = "bundle NoValid { data: bits[8] }\n\
               module M {\n  in d: bits[8]\n  out o: bits[8]\n  \
               wire x: NoValid = { data: d }\n  \
               o = x ?? 0\n}\n";
    first_err(src, "E0911");
}

#[test]
fn qq_or_mux_rhs_with_extra_field_is_e0912() {
    // `Big` has `valid`/`data` matching a `bits[8]?` LHS, but also an
    // `extra` field — not an exactly-`{valid, data}`-shaped bundle, so the
    // OR-mux form must reject it (E0912), not accept it with a nonsensical
    // result shape that later lowering can't emit.
    let src = "bundle Big { valid: bit, data: bits[8], extra: bits[4] }\n\
               module M {\n  in c: bit\n  in d: bits[8]\n  in e: bits[4]\n  \
               out o: bit\n  \
               wire x: bits[8]? = { valid: c, data: d }\n  \
               wire y: Big = { valid: c, data: d, extra: e }\n  \
               wire merged: bits[8]? = x ?? y\n  o = merged.valid\n}\n";
    first_err(src, "E0912");
}

#[test]
fn bundle_field_typed_as_valid_bundle_sugar_is_rejected_e0807() {
    // `T?` desugars to Type::Bundle in the parser, and bundle fields already
    // reject Type::Bundle (nested bundles, E0807) — this must fire
    // automatically for `?`-sugar too, with zero new checker code.
    let src = "bundle Foo { x: bits[8]? }\nmodule M {\n  out o: bit\n  o = 0\n}\n";
    assert!(any_code(src, "E0807"));
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
