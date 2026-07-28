use super::*;

#[test]
fn bundle_typed_port_flattens_at_instantiation() {
    let v = emit_src(
        "bundle Handshake(W: int = 8) { valid: bit, data: bits[W] }\n\
         module Child(W: int = 8) {\n  \
         in  req: Handshake(W: W)\n  out rsp: Handshake(W: W)\n  \
         rsp = { valid: req.valid, data: req.data }\n}\n\
         module Parent {\n  \
         in  pv: bit\n  in  pd: bits[8]\n  out ov: bit\n  out od: bits[8]\n  \
         wire in_bus: Handshake(W: 8) = { valid: pv, data: pd }\n  \
         let c = Child(W: 8) { req: in_bus }\n  \
         wire out_bus: Handshake(W: 8) = c.rsp\n  \
         ov = out_bus.valid\n  od = out_bus.data\n}\n",
    );
    assert!(
        v.contains(
            "Child #(.W(8)) c (.req_valid(in_bus_valid), .req_data(in_bus_data), \
             .rsp_valid(c_rsp_valid), .rsp_data(c_rsp_data));"
        ),
        "expected per-field flattened connections, got:\n{v}"
    );
    assert!(
        v.contains("    wire c_rsp_valid;\n"),
        "expected a per-field output wire, got:\n{v}"
    );
    assert!(
        v.contains("    wire [7:0] c_rsp_data;\n"),
        "expected a per-field output wire with the resolved width, got:\n{v}"
    );
    assert!(
        !v.contains("wire c_rsp;\n"),
        "must not declare the old single-scalar (broken) wire, got:\n{v}"
    );
}

#[test]
fn bundle_typed_fn_param_flattens_to_per_field_inputs() {
    // BUG-10 (docs/audit/bugs.md): a bundle-typed `fn` parameter used to
    // hit `width()`'s `Type::Named` arm (hard error, bare form) or
    // `Type::Bundle` arm (silently 0-width, parametric form) instead of
    // flattening like module ports/wires do. Covers both forms: `u` is
    // bare `HasUART` (zero-param), `v` is parametric `Handshake(W: 4)`.
    let v = emit_src(
        "bundle HasUART { tx: bit, rx: bit }\n\
         bundle Handshake(W: int = 8) { valid: bit, data: bits[W] }\n\
         fn pick(u: HasUART, v: Handshake(W: 4)) -> bit { u.tx & v.valid }\n\
         module M {\n  \
         in  a_tx: bit\n  in a_rx: bit\n  in b_valid: bit\n  in b_data: bits[4]\n  \
         out o: bit\n  \
         wire a: HasUART = { tx: a_tx, rx: a_rx }\n  \
         wire b: Handshake(W: 4) = { valid: b_valid, data: b_data }\n  \
         o = pick(a, b)\n}\n",
    );
    assert!(
        v.contains("        input u_tx;\n        input u_rx;\n"),
        "expected per-field flattened inputs for the bare bundle param, got:\n{v}"
    );
    assert!(
        v.contains("        input v_valid;\n        input [3:0] v_data;\n"),
        "expected per-field flattened inputs for the parametric bundle param, got:\n{v}"
    );
    assert!(
        !v.contains("input u;\n") && !v.contains("input v;\n"),
        "must not declare the old single-scalar (broken) input, got:\n{v}"
    );
    assert!(
        v.contains("pick = (u_tx & v_valid);\n") || v.contains("pick = u_tx & v_valid;\n"),
        "the function body already refers to the flattened field names, got:\n{v}"
    );
    assert!(
        v.contains("pick(a_tx, a_rx, b_valid, b_data)"),
        "the call site must expand the bundle-typed arguments into the \
         callee's flattened field names, got:\n{v}"
    );
}

#[test]
fn bare_bundle_typed_fn_return_is_a_diagnostic_not_invalid_verilog() {
    // BUG-10 returns (docs/audit/bugs.md): a bundle-typed `fn` return has
    // no flattening strategy (a Verilog `function` can only return one
    // value) — must be a real diagnostic, not silent invalid output.
    // Bare (unparametrized) form — this used to fall through to a
    // misleading "not a declared enum" message; now gets its own clear
    // one.
    // `identity` must actually be CALLED — the emitter only renders a
    // `fn` (and so only reaches `width_subst` on its return type) when
    // something references it.
    let diags = emit_src_err(
        "bundle HasUART { tx: bit, rx: bit }\n\
         fn identity(u: HasUART) -> HasUART { u }\n\
         module M {\n  in a_tx: bit\n  in a_rx: bit\n  out o: bit\n  \
         wire a: HasUART = { tx: a_tx, rx: a_rx }\n  \
         wire b: HasUART = identity(a)\n  o = b.tx\n}\n",
    );
    assert!(
        diags
            .iter()
            .any(|d| d.msg.contains("cannot return a bundle-typed value")),
        "expected a bundle-return diagnostic, got: {diags:?}"
    );
}

#[test]
fn parametric_bundle_typed_fn_return_is_a_diagnostic_not_invalid_verilog() {
    // Same as above, parametric form — this is the shape that used to
    // silently emit invalid Verilog with NO diagnostic at all (an empty
    // return-type string from `width_subst`'s old `Type::Bundle { .. }
    // => String::new()` arm), unlike the bare form which at least
    // hard-errored, if with a confusing message.
    let diags = emit_src_err(
        "bundle Handshake(W: int = 8) { valid: bit, data: bits[W] }\n\
         fn identity(v: Handshake(W: 4)) -> Handshake(W: 4) { v }\n\
         module M {\n  in a_valid: bit\n  in a_data: bits[4]\n  out o: bit\n  \
         wire a: Handshake(W: 4) = { valid: a_valid, data: a_data }\n  \
         wire b: Handshake(W: 4) = identity(a)\n  o = b.valid\n}\n",
    );
    assert!(
        diags
            .iter()
            .any(|d| d.msg.contains("cannot return a bundle-typed value")),
        "expected a bundle-return diagnostic, got: {diags:?}"
    );
}

#[test]
fn bundle_port_forwarding_a_module_parameter_stays_symbolic() {
    // `Handshake(W: W)` forwards Child's OWN parameter `W` into the
    // bundle's param. Child's `W` is a genuine Verilog `parameter` and
    // is never folded to a literal (the module is emitted once,
    // generically — Verilog's own elaboration specializes it per
    // instantiation), so the bundle field's width must stay symbolic
    // too, not silently fall back to Handshake's own unrelated default.
    let v = emit_src(
        "bundle Handshake(W: int = 8) { valid: bit, data: bits[W] }\n\
         module Child(W: int = 8) {\n  \
         in  req: Handshake(W: W)\n  out rsp: Handshake(W: W)\n  \
         rsp = { valid: req.valid, data: req.data }\n}\n",
    );
    assert!(
        v.contains("input wire [(W)-1:0] req_data,"),
        "expected the port width to track Child's own parameter symbolically, got:\n{v}"
    );
    assert!(
        v.contains("output wire [(W)-1:0] rsp_data"),
        "expected the port width to track Child's own parameter symbolically, got:\n{v}"
    );
    assert!(
        !v.contains("[7:0]"),
        "must not silently fold to Handshake's own unrelated default width, got:\n{v}"
    );
}

#[test]
fn bundle_port_forwarding_a_module_parameter_resolves_per_instance() {
    // At the INSTANTIATION site (unlike the header, which stays
    // symbolic — see the sibling test above), `Handshake(W: W)`'s
    // forwarded `W` must resolve against THIS instance's own concrete
    // argument (`Child(W: 16)`), producing the actual override width —
    // not the bundle's own default (8) and not a bare symbolic `W`
    // (which would reference a nonexistent identifier in the parent's
    // scope).
    let v = emit_src(
        "bundle Handshake(W: int = 8) { valid: bit, data: bits[W] }\n\
         module Child(W: int = 8) {\n  \
         in  req: Handshake(W: W)\n  out rsp: Handshake(W: W)\n  \
         rsp = { valid: req.valid, data: req.data }\n}\n\
         module Parent {\n  \
         in  pv: bit\n  in  pd: bits[16]\n  out ov: bit\n  out od: bits[16]\n  \
         wire in_bus: Handshake(W: 16) = { valid: pv, data: pd }\n  \
         let c = Child(W: 16) { req: in_bus }\n  \
         wire out_bus: Handshake(W: 16) = c.rsp\n  \
         ov = out_bus.valid\n  od = out_bus.data\n}\n",
    );
    assert!(
        v.contains("    wire [15:0] c_rsp_data;\n"),
        "expected the instance's own W=16 override to produce a concrete \
         16-bit wire, got:\n{v}"
    );
    assert!(
        v.contains(
            "Child #(.W(16)) c (.req_valid(in_bus_valid), .req_data(in_bus_data), \
             .rsp_valid(c_rsp_valid), .rsp_data(c_rsp_data));"
        ),
        "expected per-field flattened connections at the overridden width, got:\n{v}"
    );
}
