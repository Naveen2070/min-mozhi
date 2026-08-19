use super::*;

#[test]
fn structurally_matched_drive_emits_same_as_nominal_match() {
    // `wa`/`wb` (not `a`/`b`) — BUG-73 (docs/audit/bugs.md): a bundle
    // wire's own flattened field name (`{wire}_{field}`) must never
    // collide with an existing port; `a`/`b` here used to collide with the
    // ports `a_tx`/`a_rx`/`b_tx`/`b_rx`, which the emitter now correctly
    // rejects as a duplicate declaration.
    let nominal = emit_src(
        "bundle HasUART { tx: bit, rx: bit }\n\
         module M {\n  in  a_tx: bit\n  in a_rx: bit\n  out b_tx: bit\n  out b_rx: bit\n  \
         wire wa: HasUART = { tx: a_tx, rx: a_rx }\n  wire wb: HasUART = { tx: 0, rx: 0 }\n  \
         wb = wa\n  b_tx = wb.tx\n  b_rx = wb.rx\n}\n",
    );
    let structural = emit_src(
        "bundle HasUART { tx: bit, rx: bit }\n\
         bundle SensorData { tx: bit, rx: bit }\n\
         module M {\n  in  a_tx: bit\n  in a_rx: bit\n  out b_tx: bit\n  out b_rx: bit\n  \
         wire wa: SensorData = { tx: a_tx, rx: a_rx }\n  wire wb: HasUART = { tx: 0, rx: 0 }\n  \
         wb = wa\n  b_tx = wb.tx\n  b_rx = wb.rx\n}\n",
    );
    assert_eq!(
        nominal, structural,
        "a structurally-matched (differently-named) bundle Drive must emit \
         byte-identical Verilog to the same-name case — the emission layer \
         is field-name-driven, not type-driven"
    );
}

#[test]
fn structurally_matched_port_connection_emits_same_as_nominal_match() {
    // `wa` (not `a`) — BUG-73 (docs/audit/bugs.md): avoids the wire's own
    // flattened field name colliding with the `a_tx`/`a_rx` ports.
    let nominal = emit_src(
        "bundle HasUART { tx: bit, rx: bit }\n\
         module Child { in u: HasUART }\n\
         module M {\n  in  a_tx: bit\n  in a_rx: bit\n  \
         wire wa: HasUART = { tx: a_tx, rx: a_rx }\n  let c = Child() { u: wa }\n}\n",
    );
    let structural = emit_src(
        "bundle HasUART { tx: bit, rx: bit }\n\
         bundle SensorData { tx: bit, rx: bit }\n\
         module Child { in u: HasUART }\n\
         module M {\n  in  a_tx: bit\n  in a_rx: bit\n  \
         wire wa: SensorData = { tx: a_tx, rx: a_rx }\n  let c = Child() { u: wa }\n}\n",
    );
    assert_eq!(
        nominal, structural,
        "a structurally-matched port connection must emit byte-identical \
         Verilog to the same-name case"
    );
}

#[test]
fn structurally_matched_fn_arg_emits_same_as_nominal_match() {
    // BUG-10's param half is fixed (bundle-typed `fn` params now flatten
    // to per-field inputs, same as ports/wires — see
    // `bundle_typed_fn_param_flattens_to_per_field_inputs` above) — bare
    // zero-param bundles no longer need the old dummy-`W`-param
    // workaround. This test asserts feature 2.9's own invariant: the
    // emitted text does not vary with the bundle's declared NAME,
    // nominal or structural.
    // `wa` (not `a`) — BUG-73 (docs/audit/bugs.md): avoids the wire's own
    // flattened field name colliding with the `a_tx`/`a_rx` ports.
    let nominal = emit_src(
        "bundle HasUART { tx: bit, rx: bit }\n\
         fn pick_tx(u: HasUART) -> bit { u.tx }\n\
         module M {\n  in  a_tx: bit\n  in a_rx: bit\n  out o: bit\n  \
         wire wa: HasUART = { tx: a_tx, rx: a_rx }\n  o = pick_tx(wa)\n}\n",
    );
    let structural = emit_src(
        "bundle HasUART { tx: bit, rx: bit }\n\
         bundle SensorData { tx: bit, rx: bit }\n\
         fn pick_tx(u: HasUART) -> bit { u.tx }\n\
         module M {\n  in  a_tx: bit\n  in a_rx: bit\n  out o: bit\n  \
         wire wa: SensorData = { tx: a_tx, rx: a_rx }\n  o = pick_tx(wa)\n}\n",
    );
    assert_eq!(
        nominal, structural,
        "a structurally-matched (differently-named) bundle `fn` argument must \
         emit byte-identical Verilog to the same-name case"
    );
}

#[test]
fn structurally_matched_fn_return_is_a_diagnostic_same_as_nominal_match() {
    // BUG-10's PARAM half is fixed (see
    // `structurally_matched_fn_arg_emits_same_as_nominal_match` above);
    // the RETURN half now gets a real diagnostic instead of emitting
    // anything (invalid or otherwise) — a Verilog `function` can only
    // return ONE value, so there is no flatten-the-declaration fix the
    // way params got; the real fix (call-site inlining) is tracked as a
    // follow-up feature (`docs/plan/phase-2-ir-synthesis.md`), not done
    // here. This test used to assert nominal/structural forms emitted
    // BYTE-IDENTICAL (still invalid) Verilog via a dummy `W` param that
    // sidestepped the old hard-error path — that workaround is gone now
    // that the parametric form is ALSO rejected, so there is no longer
    // any output to compare. Repurposed to the still-meaningful
    // invariant this was really pinning: feature 2.9's structural vs.
    // nominal matching gets IDENTICAL treatment even for this unrelated
    // gap — both now hit the same diagnostic, neither dodges it.
    let nominal = emit_src_err(
        "bundle HasUART(W: int = 1) { tx: bit, rx: bit }\n\
         fn as_uart(u: HasUART(W: 1)) -> HasUART(W: 1) { u }\n\
         module M {\n  in  a_tx: bit\n  in a_rx: bit\n  out b_tx: bit\n  out b_rx: bit\n  \
         wire a: HasUART(W: 1) = { tx: a_tx, rx: a_rx }\n  \
         wire b: HasUART(W: 1) = as_uart(a)\n  b_tx = b.tx\n  b_rx = b.rx\n}\n",
    );
    let structural = emit_src_err(
        "bundle HasUART(W: int = 1) { tx: bit, rx: bit }\n\
         bundle SensorData(W: int = 1) { tx: bit, rx: bit }\n\
         fn as_uart(u: SensorData(W: 1)) -> HasUART(W: 1) { u }\n\
         module M {\n  in  a_tx: bit\n  in a_rx: bit\n  out b_tx: bit\n  out b_rx: bit\n  \
         wire a: SensorData(W: 1) = { tx: a_tx, rx: a_rx }\n  \
         wire b: HasUART(W: 1) = as_uart(a)\n  b_tx = b.tx\n  b_rx = b.rx\n}\n",
    );
    let msg = |diags: &[Diag]| {
        diags
            .iter()
            .find(|d| d.msg.contains("cannot return a bundle-typed value"))
            .unwrap_or_else(|| panic!("expected a bundle-return diagnostic, got: {diags:?}"))
            .msg
            .clone()
    };
    assert_eq!(
        msg(&nominal),
        msg(&structural),
        "a structurally-matched (differently-named) bundle `fn` return must \
         get the identical diagnostic as the same-name case"
    );
}
