//! Round-7 plan Task 1 (GAP-18, `docs/audit/gaps.md`), widened by round-8
//! plan Task 2 — the hoist buffer's flush point (`hoist_pos`) is a second
//! scoping axis alongside `hoist_unresolved`'s "which `decls` is in scope"
//! (GAP-16): a hoisted wire can resolve its `Kind` correctly and still land
//! after its own use if the render site that asked for it runs before the
//! buffer is flushed. `assert_hoists_declared_before_use`
//! (`emit_verilog/mod.rs`) is the runtime invariant that watches this; these
//! tests pin its own logic and prove it holds on BUG-66's own repro after
//! Task 3's fix (`tests/self_determined_regression.rs` carries the same
//! repro's real Icarus differentials) — and, after round-8 Task 2, on
//! BUG-70's own repro too (an ORDINARY wire out of order, not a hoisted
//! name), which round-7's narrower `__mimz_*`-only scan could never see.

use super::*;

#[test]
fn identifiers_in_finds_whole_words_and_skips_radix_specifiers() {
    let line = "    Sub u (.d({b, __mimz_sub_1}), .q(__mimz_fn_sub_12));";
    assert_eq!(
        identifiers_in(line),
        vec![
            "Sub".to_string(),
            "u".to_string(),
            "d".to_string(),
            "b".to_string(),
            "__mimz_sub_1".to_string(),
            "q".to_string(),
            "__mimz_fn_sub_12".to_string(),
        ]
    );
    // A sized-literal radix specifier (`4'd10`) must not read back as an
    // identifier `d10` — it would collide with a real declared name purely
    // by coincidence otherwise. Bare digit runs (`4`, `8`, `1`) aren't
    // identifiers at all — Verilog names can't start with a digit — so they
    // never appear in the output either way.
    assert_eq!(
        identifiers_in("assign y = a + 4'd10 + 8'b1010 + 1'sb1;"),
        vec!["assign", "y", "a"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    );
}

#[test]
fn strip_instance_port_names_drops_only_the_dotted_port_half() {
    // `.d(...)` and `.q(...)` are the CHILD module's own port names — must
    // not survive into the scan; `b` and `__mimz_sub_1` (the actual signal
    // references inside the parens) must.
    let stripped = strip_instance_port_names("    Sub u (.d({b, __mimz_sub_1}), .q(u_q));");
    assert_eq!(stripped, "    Sub u (({b, __mimz_sub_1}), (u_q));");
}

#[test]
fn task2_widened_invariant_ignores_identifiers_inside_string_literals() {
    // Not currently reachable from a plain module body (no `$display` is
    // ever emitted there — only `--emit-testbench` output, which this
    // invariant doesn't scan) but guarded anyway: a string literal's own
    // text is never executable Verilog, so a name it happens to mention
    // must never count as a "use".
    assert_eq!(
        strip_string_literals(r#"$display("state is %0d", state);"#),
        r#"$display("            ", state);"#
    );
}

#[test]
fn task2_widened_invariant_ignores_port_names_in_instance_connections() {
    // BUG-70 construction 1 (review Appendix A.10), post-Task-1-fix: `Sub`'s
    // own port names (`d`, `q`) must never be read as references to a
    // same-named module-level signal by the widened scan — proves the
    // pipeline doesn't panic on ordinary instance-connection syntax.
    let v = emit_src(
        "module Sub {\n  in d: bits[12]\n  out q: bits[4]\n  q = d[3:0]\n}\n\n\
         module Top {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bits[4]\n  \
         let u1 = Sub() { d: { b, extend(a, 8) } }\n  \
         let u2 = Sub() { d: { b, extend(u1.q, 8) } }\n  \
         y = u2.q\n}\n",
    );
    assert!(v.contains("wire [(4)-1:0] u1_q;"));
    assert!(v.contains("wire [(4)-1:0] u2_q;"));
}

#[test]
fn task2_widened_invariant_does_not_false_positive_on_clog2_helper_name_collision() {
    // A REAL, previously-latent false-positive class the plan's own trap
    // list didn't name: `CLOG2_FN` (injected at `fn_pos`, before every
    // module-level declaration) declares its own local `input integer
    // value;` — colliding with an ORDINARY module-level `reg value` a real
    // shipped example already declares (`examples/english/counter.mimz`).
    // Without excluding the injected `function ... endfunction` block (a
    // separate Verilog scope) from the scan, this would panic: `value`
    // "used" inside `CLOG2_FN`'s own body lands textually before the
    // module's own `reg [...] value;` declaration.
    let v = emit_src(
        "module M {\n  clock clk\n  reset rst\n  \
         out n: bits[4]\n  reg value: bits[4] = 0\n  \
         on rise(clk) {\n    value <- value +% 1\n  }\n  \
         n = clog2(value)\n}\n",
    );
    assert!(v.contains("function integer clog2;"));
    assert!(v.contains("reg [(4)-1:0] value;"));
}

#[test]
#[should_panic(expected = "GAP-18")]
fn task2_widened_invariant_fires_on_bug_70_construction_1() {
    // Review Appendix A.10 / round-8 plan Task 2's own test list — a
    // hand-built fixture reproducing the PRE-Task-1 broken emitter output
    // (reverting Task 1's fix in-process is awkward; the plan's own
    // fallback is a fixture), since Task 1 already closes this in the real
    // pipeline. `u1_q` — an ORDINARY wire, not a `__mimz_*` hoisted name —
    // is used one line before its own declaration; round-7's narrow scan
    // was silent on exactly this shape (the review's own finding), and the
    // widened scan must not be.
    let module_text = "module Top (\n\
        \x20   input wire [(4)-1:0] a,\n\
        \x20   input wire [(4)-1:0] b,\n\
        \x20   output wire [(4)-1:0] y\n\
        );\n\
        \x20   wire [7:0] __mimz_sub_1;\n\
        \x20   assign __mimz_sub_1 = (a);\n\
        \x20   wire [7:0] __mimz_sub_2;\n\
        \x20   assign __mimz_sub_2 = (u1_q);\n\
        \x20   wire [(4)-1:0] u1_q;\n\
        \x20   Sub u1 (.d({b, __mimz_sub_1}), .q(u1_q));\n\
        \x20   wire [(4)-1:0] u2_q;\n\
        \x20   Sub u2 (.d({b, __mimz_sub_2}), .q(u2_q));\n\
        \x20   assign y = u2_q;\n\
        endmodule\n";
    assert_hoists_declared_before_use(module_text);
}

#[test]
#[should_panic(expected = "GAP-18")]
fn task2_widened_invariant_fires_on_bug_70_construction_2() {
    // Review Appendix A.11 / plan Task 2 — the same axis through the
    // `mem`-init render site instead of an instance-port connection, same
    // fixture-based approach as construction 1 above.
    let module_text = "module Top (\n\
        \x20   input wire [(4)-1:0] a,\n\
        \x20   input wire [(4)-1:0] b,\n\
        \x20   output wire [(12)-1:0] y\n\
        );\n\
        \x20   reg [(12)-1:0] m [0:(4)-1];\n\
        \x20   wire [7:0] __mimz_sub_1;\n\
        \x20   assign __mimz_sub_1 = (a);\n\
        \x20   wire [7:0] __mimz_sub_2;\n\
        \x20   assign __mimz_sub_2 = (u1_q);\n\
        \x20   initial #0 m[0] = {b, __mimz_sub_2};\n\
        \x20   wire [(4)-1:0] u1_q;\n\
        \x20   Sub u1 (.d({b, __mimz_sub_1}), .q(u1_q));\n\
        \x20   assign y = m[0];\n\
        endmodule\n";
    assert_hoists_declared_before_use(module_text);
}

#[test]
fn task3_bug_66_a2_reg_reset_hoist_no_longer_fires_the_declaration_order_assert() {
    // BUG-66 A2 (docs/audit/review-2026-08-17.md Part 3.1): a `reg`'s own
    // reset value used to render BEFORE `hoist_pos` was captured
    // (`module/mod.rs`), so a composite reset needing a hoist declared its
    // wire AFTER the `initial r = ...;` line that already used it — this
    // exact source used to panic `assert_hoists_declared_before_use`
    // before Task 3's fix. Proves the fix in-process (fast, no `iverilog`
    // needed) that the earlier round of this test caught the bug with;
    // `tests/self_determined_regression.rs`'s `bug_66_*` differentials
    // additionally confirm real Icarus elaborates and computes the same
    // value.
    let v = emit_src(
        "module M {\n  in clk: bit\n  in a: bits[4]\n  in b: bits[4]\n  \
         reg r: bits[12] = { b, extend(a, 8) }\n  \
         on rise(clk) {\n    r <- { b, extend(a, 8) }\n  }\n}\n",
    );
    let wire_at = v
        .find("wire [7:0] __mimz_sub_1;")
        .expect("hoisted wire missing");
    let use_at = v
        .find("initial #0 r =")
        .expect("reg initial-seed line missing");
    assert!(
        wire_at < use_at,
        "hoisted wire must be declared before the `initial` line that uses it:\n{v}"
    );
}
