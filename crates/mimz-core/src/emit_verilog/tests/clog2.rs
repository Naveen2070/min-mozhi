use super::*;

#[test]
fn clog2_folds_into_the_port_width() {
    // clog2(9) = 4 bits → `output [3:0] o`. Proves the const-builtin folds to
    // the right VALUE in a width position, not just that it is accepted.
    let v = emit_src("module M {\n  out o: bits[clog2(9)]\n  o = 0\n}\n");
    // The emitter keeps a derived width in `(value)-1:0` form; the folded
    // `(4)` is the proof clog2(9) evaluated to 4.
    assert!(
        v.contains("[(4)-1:0] o"),
        "clog2(9) should size `o` to 4 bits ([(4)-1:0]):\n{v}"
    );
    // A folded literal `clog2` must not drag in the constant function.
    assert!(
        !v.contains("function integer clog2"),
        "a folded clog2 must not emit the function:\n{v}"
    );
}

#[test]
fn clog2_of_a_const_derives_the_width() {
    // DEPTH a `const` = 16 → clog2 = 4 → `[(4)-1:0] ptr`. Consts fold in the
    // emitted Verilog, so this is the supported parametric-width path.
    let v = emit_src(
        "module M {\n  const DEPTH: int = 16\n  out ptr: bits[clog2(DEPTH)]\n  ptr = 0\n}\n",
    );
    assert!(
        v.contains("[(4)-1:0] ptr"),
        "clog2(const 16) should size `ptr` to 4 bits ([(4)-1:0]):\n{v}"
    );
}

#[test]
fn clog2_of_a_parameter_in_a_body_width_emits_the_constant_function() {
    // A parameter stays symbolic, so the width tracks an override via the
    // injected Verilog-2005 `clog2` constant function.
    let v = emit_src(
        "module M(DEPTH: int = 16) {\n  out o: bit\n  wire w: bits[clog2(DEPTH)] = 0\n  o = 0\n}\n",
    );
    assert!(
        v.contains("function integer clog2"),
        "a parametric clog2 width must inject the constant function:\n{v}"
    );
    assert!(
        v.contains("[(clog2(DEPTH))-1:0] w"),
        "the width must call clog2(DEPTH) so an override is honored:\n{v}"
    );
}

#[test]
fn clog2_of_a_parameter_in_a_port_is_an_emit_error() {
    // A port width lives in the header, which the body-scoped function can't
    // reach — an honest error, never a wrong width.
    let diags =
        emit_src_err("module M(DEPTH: int = 16) {\n  out ptr: bits[clog2(DEPTH)]\n  ptr = 0\n}\n");
    assert!(
        diags.iter().any(|d| d.msg.contains("clog2")),
        "expected a clog2 emit error, got: {diags:?}"
    );
}
