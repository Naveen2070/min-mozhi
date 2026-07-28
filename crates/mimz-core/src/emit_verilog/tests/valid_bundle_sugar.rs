use super::*;

#[test]
fn a_decimal_literal_past_128_bits_emits_correctly() {
    let src = "module M {\n  wire w: bits[200] = 1361129467683753853853498429727072845824\n  out y: bit\n  y = w[0]\n}\n";
    let out = emit_src(src);
    assert!(
        out.contains("1361129467683753853853498429727072845824"),
        "expected the full decimal literal in the emitted Verilog, got:\n{out}"
    );
}

#[test]
fn qq_unwrap_form_emits_a_ternary_on_validity() {
    let v = emit_src(
        "module M {\n  in c: bit\n  in d: bits[8]\n  out o: bits[8]\n  \
         wire raw: bits[8]? = { valid: c, data: d }\n  \
         wire safe: bits[8] = raw ?? 0\n  o = safe\n}\n",
    );
    assert!(
        v.contains("assign safe = (raw_valid ? raw_data : 0);")
            || v.contains("assign safe = raw_valid ? raw_data : 0;"),
        "expected a ternary keyed on raw_valid, got:\n{v}"
    );
}

#[test]
fn qq_unwrap_form_emits_a_ternary_via_drive() {
    let v = emit_src(
        "module M {\n  in c: bit\n  in d: bits[8]\n  out safe: bits[8]\n  \
         wire raw: bits[8]? = { valid: c, data: d }\n  \
         safe = raw ?? 0\n}\n",
    );
    assert!(
        v.contains("raw_valid ? raw_data : 0"),
        "expected the same ternary shape via a Drive statement, got:\n{v}"
    );
}

#[test]
fn qq_or_mux_form_emits_per_field_ternaries_at_wire_init() {
    let v = emit_src(
        "module M {\n  in c1: bit\n  in d1: bits[8]\n  in c2: bit\n  in d2: bits[8]\n  \
         out mv: bit\n  out md: bits[8]\n  \
         wire x: bits[8]? = { valid: c1, data: d1 }\n  \
         wire y: bits[8]? = { valid: c2, data: d2 }\n  \
         wire merged: bits[8]? = x ?? y\n  mv = merged.valid\n  md = merged.data\n}\n",
    );
    assert!(
        v.contains("assign merged_valid = (x_valid ? 1'b1 : y_valid);")
            || v.contains("assign merged_valid = x_valid ? 1'b1 : y_valid;"),
        "expected a per-field valid mux, got:\n{v}"
    );
    assert!(
        v.contains("x_valid ? x_data : y_data"),
        "expected a per-field data mux, got:\n{v}"
    );
}

#[test]
fn qq_or_mux_form_emits_per_field_ternaries_via_drive() {
    let v = emit_src(
        "module M {\n  in c1: bit\n  in d1: bits[8]\n  in c2: bit\n  in d2: bits[8]\n  \
         out merged: bits[8]?\n  \
         wire x: bits[8]? = { valid: c1, data: d1 }\n  \
         wire y: bits[8]? = { valid: c2, data: d2 }\n  \
         merged = x ?? y\n}\n",
    );
    assert!(v.contains("x_valid ? x_data : y_data"), "got:\n{v}");
}

#[test]
fn qq_or_mux_form_emits_per_field_ternaries_at_port_connection() {
    let v = emit_src(
        "module Child { in u: bits[8]? }\n\
         module M {\n  in c1: bit\n  in d1: bits[8]\n  in c2: bit\n  in d2: bits[8]\n  \
         wire x: bits[8]? = { valid: c1, data: d1 }\n  \
         wire y: bits[8]? = { valid: c2, data: d2 }\n  \
         let ch = Child() { u: x ?? y }\n}\n",
    );
    assert!(
        v.contains("x_valid ? x_data : y_data") || v.contains("x_valid ? 1'b1 : y_valid"),
        "expected the port connection to mux per-field, got:\n{v}"
    );
}

#[test]
fn qq_or_mux_form_expands_at_fn_call_site() {
    let v = emit_src(
        "bundle HasUART { tx: bit, rx: bit }\n\
         fn pick(u: bits[8]?) -> bit { u.valid }\n\
         module M {\n  in c1: bit\n  in d1: bits[8]\n  in c2: bit\n  in d2: bits[8]\n  \
         out o: bit\n  \
         wire x: bits[8]? = { valid: c1, data: d1 }\n  \
         wire y: bits[8]? = { valid: c2, data: d2 }\n  \
         o = pick(x ?? y)\n}\n",
    );
    // the call site must NOT pass `x ?? y` as one textual argument —
    // it must expand into per-field ternaries, one arg per callee field.
    assert!(
        !v.contains("pick((x") && !v.contains("pick(x ??"),
        "got:\n{v}"
    );
}

#[test]
fn qq_or_mux_chain_emits_valid_nested_verilog() {
    let v = emit_src(
        "module M {\n  in c1: bit\n  in d1: bits[8]\n  in c2: bit\n  in d2: bits[8]\n  \
         in c3: bit\n  in d3: bits[8]\n  out mv: bit\n  out md: bits[8]\n  \
         wire x: bits[8]? = { valid: c1, data: d1 }\n  \
         wire y: bits[8]? = { valid: c2, data: d2 }\n  \
         wire z: bits[8]? = { valid: c3, data: d3 }\n  \
         wire merged: bits[8]? = x ?? y ?? z\n  mv = merged.valid\n  md = merged.data\n}\n",
    );
    // The tell-tale sign of the old bug: a bundle-typed sub-expression
    // rendered as a plain signal with a scalar field-suffix glued onto
    // its closing paren, e.g. `(x_valid ? x_data : y)_valid`.
    assert!(
        !v.contains(")_valid") && !v.contains(")_data"),
        "broken nested Verilog:\n{v}"
    );
    assert!(
        v.to_lowercase().contains("assign merged_valid"),
        "got:\n{v}"
    );
}

#[test]
fn qq_or_mux_chain_expands_correctly_at_fn_call_site() {
    let v = emit_src(
        "fn pick(u: bits[8]?) -> bit { u.valid }\n\
         module M {\n  in c1: bit\n  in d1: bits[8]\n  in c2: bit\n  in d2: bits[8]\n  \
         in c3: bit\n  in d3: bits[8]\n  out o: bit\n  \
         wire x: bits[8]? = { valid: c1, data: d1 }\n  \
         wire y: bits[8]? = { valid: c2, data: d2 }\n  \
         wire z: bits[8]? = { valid: c3, data: d3 }\n  \
         o = pick(x ?? y ?? z)\n}\n",
    );
    assert!(
        !v.contains(")_valid") && !v.contains(")_data"),
        "broken nested Verilog at fn-call site:\n{v}"
    );
    // must not degrade to the unexpanded single-argument bug (the whole
    // `x ?? y ?? z` passed through as one literal textual argument)
    assert!(!v.contains("pick(x ??"), "got:\n{v}");
}

#[test]
fn user_bundle_shaped_like_valid_bundle_emits_same_as_builtin_sugar() {
    let sugar = emit_src(
        "module M {\n  in c: bit\n  in d: bits[8]\n  out o: bits[8]\n  \
         wire x: bits[8]? = { valid: c, data: d }\n  o = x ?? 0\n}\n",
    );
    let user_bundle = emit_src(
        "bundle MyOptional { valid: bit, data: bits[8] }\n\
         module M {\n  in c: bit\n  in d: bits[8]\n  out o: bits[8]\n  \
         wire x: MyOptional = { valid: c, data: d }\n  o = x ?? 0\n}\n",
    );
    assert_eq!(
        sugar, user_bundle,
        "a user bundle structurally identical to bits[8]? must emit identically"
    );
}
