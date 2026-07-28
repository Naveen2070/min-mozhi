use super::*;

#[test]
fn emitter_injects_function_called_only_from_a_return() {
    let src = "fn helper(a: bits[8]) -> bits[8] {\n  a\n}\nfn f(a: bits[8]) -> bits[8] {\n  if a[0] == 1 { return helper(a) }\n  0\n}\nmodule M {\n  in a: bits[8]\n  out o: bits[8]\n  o = f(a)\n}\n";
    let verilog = emit_src(src);
    assert!(verilog.contains("function automatic"));
    assert!(verilog.matches("function automatic").count() >= 2); // both helper and f
}

#[test]
fn fn_loop_with_return_finds_first_match() {
    let src = "fn find_first_set(vals: bits[8][4]) -> signed[4] {\n  loop i: 0..4 {\n    if vals[i] == 0xFF { return i }\n  }\n  0 - 1\n}\nmodule M {\n  in a: bits[8]\n  in b: bits[8]\n  in c: bits[8]\n  in d: bits[8]\n  out o: signed[4]\n  o = find_first_set([a, b, c, d])\n}\n";
    let verilog = emit_src(src);
    // Nested (not a flat sequence of independent assignments): iteration
    // 0's check must be the OUTERMOST `if`, so it structurally takes
    // priority over later iterations, matching return's existing CPS
    // lowering (see emit_fn_stmts's doc comment).
    assert_eq!(
        verilog.matches("if (").count(),
        4,
        "expected one nested if per iteration:\n{verilog}"
    ); // one nested if per iteration
}

#[test]
fn fn_loop_with_return_first_match_wins_on_duplicate() {
    // vals has 0xFF at BOTH index 0 and index 2 — must return 0, the LOWER
    // index, not 2. This is the one test that actually distinguishes true
    // first-match-wins from "returns some match": a bug in the
    // continuation-threading (e.g. iterating in the wrong order, or
    // flattening to independent assignments instead of nesting) would
    // silently return the HIGHER index here while the simulator (Task 9)
    // still returns the lower one.
    let src = "fn find_first_set(vals: bits[8][4]) -> signed[4] {\n  loop i: 0..4 {\n    if vals[i] == 0xFF { return i }\n  }\n  0 - 1\n}\nmodule M {\n  in a: bits[8]\n  in b: bits[8]\n  in c: bits[8]\n  in d: bits[8]\n  out o: signed[4]\n  o = find_first_set([a, b, c, d])\n}\n";
    let verilog = emit_src(src);
    // Structural proof of NESTING, not just emission order. A flattened
    // implementation (threading the loop's own `rest` into every
    // iteration instead of chaining each iteration to the NEXT one, then
    // concatenating the resulting if/else blocks as siblings) would ALSO
    // emit vals_0's check textually before vals_2's — both walk 0..4 in
    // order — so a mere "position of vals_0 < position of vals_2" check
    // cannot tell nested-correct apart from flattened-wrong, even though
    // the flattened shape computes the WRONG value at simulation time
    // (sibling ifs execute unconditionally in sequence, so the LAST
    // matching condition wins, not the first).
    //
    // The real discriminator: in the correct CPS lowering, iteration N's
    // `if` is embedded inside iteration N-1's `else` branch, so the text
    // "end else begin\n" is immediately followed by "if (" for every
    // iteration except the last (whose `else` holds the loop's tail
    // expression instead of another `if`). See
    // `flattened_loop_shape_fails_the_nesting_assertion` below for a
    // literal counter-example proving this assertion actually rejects
    // the flattened shape.
    let after_else: Vec<&str> = verilog.split("end else begin\n").skip(1).collect();
    assert_eq!(
        after_else.len(),
        4,
        "expected one `end else begin` per loop iteration (4 iterations):\n{verilog}"
    );
    let nests_into_if: Vec<bool> = after_else
        .iter()
        .map(|part| part.trim_start().starts_with("if ("))
        .collect();
    assert_eq!(
        nests_into_if,
        vec![true, true, true, false],
        "iterations 0..3 must each nest their else around the NEXT \
         iteration's if (only the last iteration's else should hold \
         the loop's tail expression instead of another if) - a \
         flattened/sibling-ifs emission would produce all-false here:\n{verilog}"
    );
}

/// Load-bearing check for the assertion above: run it against a literal
/// string shaped like what a FLATTENED (independent-copies) emitter would
/// produce for the same 4-iteration duplicate-match case - each
/// iteration's if/else closed completely before the next iteration's
/// if starts as a sibling statement, rather than nested in its else.
/// This is the exact bug the strengthened assertion above must catch;
/// this test proves it does.
#[test]
fn flattened_loop_shape_fails_the_nesting_assertion() {
    let flattened = "\
        if ((vals_0 == 'hFF)) begin\n\
        find_first_set = 0;\n\
        end else begin\n\
        find_first_set = (0 - 1);\n\
        end\n\
        if ((vals_1 == 'hFF)) begin\n\
        find_first_set = 1;\n\
        end else begin\n\
        find_first_set = (0 - 1);\n\
        end\n\
        if ((vals_2 == 'hFF)) begin\n\
        find_first_set = 2;\n\
        end else begin\n\
        find_first_set = (0 - 1);\n\
        end\n\
        if ((vals_3 == 'hFF)) begin\n\
        find_first_set = 3;\n\
        end else begin\n\
        find_first_set = (0 - 1);\n\
        end\n";
    let after_else: Vec<&str> = flattened.split("end else begin\n").skip(1).collect();
    assert_eq!(after_else.len(), 4);
    let nests_into_if: Vec<bool> = after_else
        .iter()
        .map(|part| part.trim_start().starts_with("if ("))
        .collect();
    assert_ne!(
        nests_into_if,
        vec![true, true, true, false],
        "the flattened shape must NOT satisfy the nesting assertion \
         used by fn_loop_with_return_first_match_wins_on_duplicate"
    );
    assert_eq!(
        nests_into_if,
        vec![false, false, false, false],
        "flattened/sibling ifs never immediately follow `end else begin`"
    );
}
