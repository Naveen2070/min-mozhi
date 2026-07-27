use super::*;

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
