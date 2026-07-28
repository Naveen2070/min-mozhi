use super::*;

#[test]
fn a_builtin_lowers_parenthesized_inside_a_larger_expression() {
    // `min(b, c)` must lower to a self-contained, fully-parenthesized ternary
    // so it composes correctly under a surrounding operator (here `&`) — no
    // precedence leak from the host expression into the built-in or back.
    let v = emit_src(
        "module M {\n  in a: bits[8]\n  in b: bits[8]\n  in c: bits[8]\n  out y: bits[8]\n  y = a & min(b, c)\n}\n",
    );
    assert!(
        v.contains("((b < c) ? (b) : (c))"),
        "min lowered + parenthesized:\n{v}"
    );
}

#[test]
fn repeat_unrolls_drives_with_folded_indices() {
    let v = emit_src(
        "module M {\n  in x: bits[4]\n  out y: bits[4]\n  repeat i: 0..4 {\n    y[i] = x[i]\n  }\n}\n",
    );
    for i in 0..4 {
        assert!(
            v.contains(&format!("assign y[{i}] = x[{i}];")),
            "missing y[{i}]\n{v}"
        );
    }
    assert!(!v.contains("y[4]"), "half-open range must stop at 3");
}

#[test]
fn repeat_var_folds_in_index_arithmetic() {
    let v = emit_src(
        "module M {\n  in x: bits[8]\n  out y: bits[8]\n  repeat i: 0..3 {\n    y[i + 1] = x[i]\n  }\n}\n",
    );
    assert!(v.contains("assign y[1] = x[0];"));
    assert!(v.contains("assign y[3] = x[2];"));
}

#[test]
fn empty_and_reversed_ranges_emit_nothing() {
    let empty =
        emit_src("module M {\n  out y: bits[4]\n  repeat i: 0..0 {\n    y[i] = 0\n  }\n}\n");
    assert!(!empty.contains("assign y"), "0..0 generates nothing");
    let reversed =
        emit_src("module M {\n  out y: bits[4]\n  repeat i: 4..0 {\n    y[i] = 0\n  }\n}\n");
    assert!(
        !reversed.contains("assign y"),
        "a reversed range generates nothing"
    );
}

#[test]
fn repeat_over_budget_errors_cleanly() {
    let diags =
        emit_src_err("module M {\n  out y: bits[4]\n  repeat i: 0..5000 {\n    y[0] = 0\n  }\n}\n");
    assert!(
        diags
            .iter()
            .any(|d| d.msg.contains("unroll") && d.msg.contains("limit")),
        "expected a budget error, got: {:?}",
        diags.iter().map(|d| &d.msg).collect::<Vec<_>>()
    );
}

#[test]
fn on_block_loop_unrolls_to_n_copies() {
    let v = emit_src(
        "module M {\n  in clk: bit\n  in v0: bits[8]\n  in v1: bits[8]\n  reg acc: bits[8] = 0\n  on rise(clk) {\n    loop i: 0..2 {\n      acc <- v0\n    }\n  }\n}\n",
    );
    // Two unrolled copies of the assignment inside the always block —
    // both textually present since `loop` is elaboration-time unrolling,
    // never a runtime loop.
    assert_eq!(
        v.matches("acc <= v0;").count(),
        2,
        "expected 2 unrolled copies:\n{v}"
    );
}

#[test]
fn on_block_loop_over_budget_is_rejected() {
    let src = format!(
        "module M {{\n  in clk: bit\n  in v0: bits[8]\n  reg acc: bits[8] = 0\n  on rise(clk) {{\n    loop i: 0..{} {{\n      acc <- v0\n    }}\n  }}\n}}\n",
        REPEAT_BUDGET + 1
    );
    let diags = emit_src_err(&src);
    assert!(
        diags
            .iter()
            .any(|d| d.msg.contains("`loop` would unroll") && d.msg.contains("limit")),
        "expected a budget error, got: {:?}",
        diags.iter().map(|d| &d.msg).collect::<Vec<_>>()
    );
}

#[test]
fn nested_repeat_folds_both_variables() {
    let v = emit_src(
        "module M {\n  out y: bits[4]\n  repeat i: 0..2 {\n    repeat j: 0..2 {\n      y[i] = j\n    }\n  }\n}\n",
    );
    // i and j both fold: the i=1, j=1 iteration drives `y[1] = 1`.
    assert!(v.contains("assign y[0] = 0;"));
    assert!(v.contains("assign y[1] = 1;"));
}

#[test]
fn repeat_instance_array_gets_flat_names() {
    let v = emit_src(
        "module Sub {\n  in a: bit\n  out o: bit\n  o = a\n}\n\
         module Top {\n  in x: bits[2]\n  out y: bits[2]\n  repeat i: 0..2 {\n    let u[i] = Sub() { a: x[i] }\n    y[i] = u[i].o\n  }\n}\n",
    );
    assert!(v.contains("Sub u__0 ("), "flat instance name u__0");
    assert!(v.contains("Sub u__1 ("), "flat instance name u__1");
    assert!(v.contains("wire u__0_o;"), "auto-wire for u[0].o");
    assert!(
        v.contains("assign y[0] = u__0_o;"),
        "indexed field read folds"
    );
    assert!(v.contains("assign y[1] = u__1_o;"));
}
