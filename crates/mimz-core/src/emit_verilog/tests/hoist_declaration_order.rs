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

/// Build a throwaway `test_emitter` and hand it to `f`. The fixture texts
/// `assert_hoists_declared_before_use` is tested against below are hand-built
/// Verilog, not real emitter output — the `Project`/`Emitter` backing them is
/// irrelevant beyond giving `Emitter::err` somewhere to push a `Diag` and a
/// `cur_file` to stamp it with, so one minimal placeholder module serves
/// every call site here.
fn with_test_emitter<R>(f: impl FnOnce(&mut Emitter) -> R) -> R {
    let files = [parse(
        "module Placeholder {\n  in a: bit\n  out y: bit\n  y = a\n}\n",
    )];
    let project = Project::from_files(&files).unwrap();
    let mut em = test_emitter(&project);
    f(&mut em)
}

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
    with_test_emitter(|em| {
        assert_hoists_declared_before_use(em, module_text, crate::span::Span::new(0, 0));
    });
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
    with_test_emitter(|em| {
        assert_hoists_declared_before_use(em, module_text, crate::span::Span::new(0, 0));
    });
}

/// Round-8 plan Task 5: `assert_hoists_declared_before_use` used to be a
/// bare `debug_assert!`; this workspace ships `[profile.release]
/// debug-assertions = true` (`Cargo.toml`), so a violation aborted a real,
/// shipped `mimz compile` with a Rust panic instead of exiting non-zero
/// with a message — the same failure mode round-7 Task 2 removed from
/// `hoist_unresolved` (`module/ports.rs`), ten lines away, the same day.
///
/// No test compiled inside this crate can observe "a real `mimz` binary
/// takes the non-panicking branch" directly — `cfg!(test)` is a
/// per-compilation constant, and it is unconditionally `true` for THIS
/// binary (`cargo test -p mimz-core`), same as it always was for
/// `hoist_unresolved`'s own identical gate (round-7 Task 2 shipped with no
/// such test either, for the same reason — there is no in-repo precedent
/// to diverge from). What IS observable here: the fix pushes the `Diag`
/// BEFORE the `cfg!(test)`-gated `debug_assert!`, not after (the opposite
/// order from `hoist_unresolved`) — so even in this binary, where the
/// assert still panics loudly (proven by the two `#[should_panic]` tests
/// above, unchanged), the `Diag` a real binary would return was already
/// recorded on `em.diags` before that panic unwound. `catch_unwind` proves
/// both halves at once: the panic still fires here, AND the identical
/// `Diag` was genuinely constructed and pushed, not skipped by the branch
/// that differs between the two binaries.
#[test]
fn task5_declaration_order_violation_is_a_diagnostic_not_a_panic_outside_tests() {
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

    with_test_emitter(|em| {
        // `&mut *em` (a reborrow), not `em` itself — the closure only needs
        // unique access to the pointee for the duration of the call, so it
        // captures that reborrow rather than moving the outer `&mut
        // Emitter` binding. `em.diags` is readable again below once
        // `catch_unwind` returns, with no raw pointer/unsafe needed.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_hoists_declared_before_use(&mut *em, module_text, crate::span::Span::new(0, 0));
        }));
        assert!(
            result.is_err(),
            "expected this crate's own test binary (cfg!(test) == true) to still panic loudly, \
             same as the two should_panic tests above"
        );
        assert!(
            !em.diags.is_empty(),
            "the Diag must be pushed BEFORE the cfg!(test)-gated panic, not skipped by it — \
             this is what makes a real `mimz` binary (cfg!(test) == false there) exit with a \
             message instead of aborting, since it takes the identical code path minus the panic"
        );
        assert!(
            em.diags[0].msg.contains("GAP-18"),
            "expected the GAP-18 declaration-order message, got: {:?}",
            em.diags[0]
        );
    });
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
