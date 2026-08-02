//! Stage 4, Phase A1b, Task 6 — regression tests for BUG-19 and BUG-20
//! (`docs/audit/bugs.md`): the emitter now hoists a self-determined-
//! position mismatch (concat/replicate member, comparison operand,
//! `$signed`/`$unsigned` argument — BUG-19) or a non-identifier slice
//! base (BUG-20) into a named `wire`/`assign` pair instead of emitting
//! Verilog text whose own width-inference rule disagrees with mimz's.
//!
//! Each test is one of BUG-19's own two filed repros or BUG-20's repro,
//! run against the SAME two independent judges every other Icarus
//! differential test here uses (`tests/icarus.rs`, `tests/
//! differential_fuzz.rs`): our own in-memory kernel (`mimz::sim::comb::
//! eval_outputs`) and real `iverilog`/`vvp`, on the EXACT input vector
//! each bug's original filing used. `differential`, below, is this
//! file's own one-fixed-vector counterpart to `differential_fuzz.rs`'s
//! `differential_fuzz_matches_icarus` (which runs many RANDOM vectors) —
//! there is no existing single-call helper of this exact shape anywhere
//! in the suite (`tests/icarus.rs` only has the many-testbench-file
//! machinery for the example corpus), so this factors out exactly the
//! fixed-vector slice of that pattern rather than inventing a new
//! Icarus-invocation path.

use std::collections::BTreeMap;

use mimz::ast::{self, TopItem};
use mimz::checker::consteval;
use mimz::sim::comb;
use mimz::sim::elaborate::elaborate_project;
use mimz::sim::run::{SimOpts, run};
use mimz::{checker, compile_string, diag, lexer, parser};

mod support;

/// `(name, width)` pairs — one per declared port, in source order.
type PortList = Vec<(String, u32)>;

/// Every declared `in`/`out` port's `(name, width)` from `file`'s sole
/// module, split by direction. Every test source below only ever
/// declares a literal `bits[N]`/`bit` width, so `consteval::eval` against
/// an empty env always resolves — no module ever has a `parameter` here.
fn module_ports(file: &ast::File) -> (PortList, PortList) {
    let m = file
        .items
        .iter()
        .find_map(|i| match i {
            TopItem::Module(m) => Some(m),
            _ => None,
        })
        .expect("test source must declare exactly one module");
    let mut ins = Vec::new();
    let mut outs = Vec::new();
    let empty_env = consteval::Env::new();
    for item in &m.items {
        if let ast::ModuleItem::Port { dir, name, ty } = item {
            let w = match ty {
                ast::Type::Bit => 1,
                ast::Type::Bits(e) | ast::Type::Signed(e) => consteval::eval(e, &empty_env)
                    .expect("literal width")
                    .to_i128_saturating()
                    as u32,
                other => panic!("module_ports: unsupported port type {other:?}"),
            };
            match dir {
                ast::Dir::In => ins.push((name.name.clone(), w)),
                ast::Dir::Out => outs.push((name.name.clone(), w)),
            }
        }
    }
    (ins, outs)
}

/// Compile `src` to Verilog once, then check that our own kernel and
/// real Icarus agree on every output, for the ONE input vector `inputs`
/// gives (`(port name, value)` pairs — every declared `in` port must be
/// covered). Skips (does not fail) when Icarus isn't installed, exactly
/// like every other Icarus differential test in this suite —
/// `support::require_iverilog` is the shared gate.
fn differential(src: &str, inputs: &[(&str, u128)]) {
    let Some(bin) = support::require_iverilog() else {
        return;
    };

    let tokens = lexer::lex(src)
        .unwrap_or_else(|e| panic!("unlexable:\n{src}\n{}", diag::render(&e, src, "test")));
    let file = parser::parse(tokens)
        .unwrap_or_else(|e| panic!("unparsable:\n{src}\n{}", diag::render(&e, src, "test")));
    if let Err(e) = checker::check(std::slice::from_ref(&file)) {
        panic!(
            "checker rejected:\n{src}\n{}",
            diag::render(&e, src, "test")
        );
    }

    let (inputs_meta, outputs_meta) = module_ports(&file);
    let input_map: BTreeMap<String, u128> =
        inputs.iter().map(|(n, v)| (n.to_string(), *v)).collect();
    assert_eq!(
        input_map.len(),
        inputs_meta.len(),
        "every declared `in` port must have a value in `inputs`"
    );

    // Our own kernel.
    let input_map_bits: BTreeMap<String, mimz::sim::value::Bits> = input_map
        .iter()
        .map(|(k, v)| (k.clone(), mimz::sim::value::Bits::Small(*v)))
        .collect();
    let outputs = comb::eval_outputs(
        std::slice::from_ref(&file),
        None,
        &input_map_bits,
        &BTreeMap::new(),
    )
    .unwrap_or_else(|e| panic!("our kernel rejected this program:\n{src}\n{}", e.msg));
    // Regression fixtures here are hand-picked small-width repros, so every
    // output value is always `Bits::Small` — narrow back to `u128` to
    // compare against Icarus's own u128-typed parsed output.
    let kernel_row: BTreeMap<String, u128> = outputs
        .into_iter()
        .map(|o| {
            let v = match o.value {
                mimz::sim::value::Bits::Small(v) => v,
                mimz::sim::value::Bits::Wide(_) => {
                    unreachable!("this test's fixtures are all narrow-width repros")
                }
            };
            (o.name, v)
        })
        .collect();

    // Real Icarus — a unique temp path (and `run_vvp` "example" tag) per
    // test so parallel `cargo test` runs never clash on the same temp
    // file (mirrors `differential_fuzz.rs`'s per-seed path).
    let tag = format!("{:x}", md5_ish(src));
    let path = std::env::temp_dir().join(format!("mimz_sdp_regression_{tag}.mimz"));
    std::fs::write(&path, src).unwrap();
    let design_v = support::compile_example(&path);

    let vectors = vec![input_map];
    let tb = support::comb_testbench("Fuzz", &[], &inputs_meta, &outputs_meta, &vectors);
    let example = format!("self-determined-position regression {tag}");
    let stdout = support::run_vvp(&bin, &example, &design_v, &tb);
    let icarus = support::parse_icarus(&stdout);
    let icarus_row = icarus.get(&0).expect("Icarus produced no row for vector 0");

    for (name, _) in &outputs_meta {
        let kernel_v = kernel_row[name];
        let icarus_v = icarus_row[name];
        assert_eq!(
            kernel_v, icarus_v,
            "output `{name}`: our kernel says {kernel_v} but Icarus says {icarus_v}\nsource:\n{src}"
        );
    }
}

/// Cycle-by-cycle counterpart to `differential`, for a clocked program
/// (`clock`/`reg`/`on rise`). Mirrors `tests/differential_fuzz.rs`'s
/// `differential_fuzz_clocked_matches_icarus` (and `tests/icarus.rs`'s
/// clocked half of `differential_m`) — our own kernel (`elaborate_project`
/// plus `run`, the exact engine behind `mimz sim`/`test`) vs. real Icarus,
/// via `support::clocked_testbench` — but for ONE fixed, hand-picked
/// held-input vector instead of many random/generated ones. There is no
/// existing single-call clocked-fixed-vector helper anywhere in the suite
/// either, exactly like `differential` above. Skips (does not fail) when
/// Icarus isn't installed.
fn differential_clocked(src: &str, held_inputs: &[(&str, u128)]) {
    let Some(bin) = support::require_iverilog() else {
        return;
    };

    let tokens = lexer::lex(src)
        .unwrap_or_else(|e| panic!("unlexable:\n{src}\n{}", diag::render(&e, src, "test")));
    let file = parser::parse(tokens)
        .unwrap_or_else(|e| panic!("unparsable:\n{src}\n{}", diag::render(&e, src, "test")));
    if let Err(e) = checker::check(std::slice::from_ref(&file)) {
        panic!(
            "checker rejected:\n{src}\n{}",
            diag::render(&e, src, "test")
        );
    }

    const CYCLES: u64 = 8;
    const RESET_CYCLES: u64 = 1;

    let held: BTreeMap<String, u128> = held_inputs
        .iter()
        .map(|(n, v)| (n.to_string(), *v))
        .collect();

    let design = elaborate_project(std::slice::from_ref(&file), None, &BTreeMap::new())
        .unwrap_or_else(|e| panic!("our kernel failed to elaborate:\n{src}\n{}", e.msg));
    assert_eq!(
        held.len(),
        design.inputs.len(),
        "every declared `in` port must have a value in `held_inputs`"
    );

    let clock = design
        .clocks
        .first()
        .expect("clocked test source must declare a clock")
        .clone();
    let reset = design.resets.first().cloned();

    let inputs_meta: Vec<(String, u32, u128)> = design
        .inputs
        .iter()
        .map(|s| (s.name.clone(), s.width.bits, held[&s.name]))
        .collect();
    let outputs_meta: Vec<(String, u32)> = design
        .outputs
        .iter()
        .map(|s| (s.name.clone(), s.width.bits))
        .collect();

    let opts = SimOpts {
        clock: None,
        inputs: held
            .iter()
            .map(|(k, v)| (k.clone(), mimz::sim::value::Bits::Small(*v)))
            .collect(),
        cycles: CYCLES,
        reset_cycles: RESET_CYCLES,
    };
    let tl = run(design, &opts).unwrap_or_else(|e| panic!("our kernel failed to run:\n{src}\n{e}"));

    let tag = format!("{:x}", md5_ish(src));
    let path = std::env::temp_dir().join(format!("mimz_sdp_regression_clocked_{tag}.mimz"));
    std::fs::write(&path, src).unwrap();
    let design_v = support::compile_example(&path);

    let tb = support::clocked_testbench(
        "Fuzz",
        &[],
        &clock,
        reset.as_deref(),
        &inputs_meta,
        &outputs_meta,
        CYCLES,
        RESET_CYCLES,
    );
    let example = format!("self-determined-position clocked regression {tag}");
    let stdout = support::run_vvp(&bin, &example, &design_v, &tb);
    let icarus = support::parse_icarus(&stdout);

    let mut compared = 0;
    for f in tl.frames.iter().filter(|f| f.cycle.is_some()) {
        let cyc = f.cycle.unwrap();
        let icarus_row = icarus
            .get(&cyc)
            .unwrap_or_else(|| panic!("Icarus produced no row for cycle {cyc}"));
        for (name, _) in &outputs_meta {
            let kernel_v = f.values[name].clone();
            let icarus_v = icarus_row[name];
            assert_eq!(
                kernel_v,
                mimz::sim::value::Bits::Small(icarus_v),
                "output `{name}` at cycle {cyc}: our kernel says {kernel_v:?} but Icarus says {icarus_v}\nsource:\n{src}"
            );
        }
        compared += 1;
    }
    assert!(compared > 0, "nothing was compared");
}

/// Cheap, deterministic, non-cryptographic tag for a unique-enough temp
/// file name — collisions across these 3 fixed test sources are not a
/// real concern, this only exists so parallel test runs never clash on
/// the SAME path.
fn md5_ish(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

#[test]
fn bug_19_lossless_sub_in_a_concat_matches_icarus() {
    // docs/audit/bugs.md BUG-19's first filed repro: a lossless `-` (which
    // mimz grows by one bit) rendered as a concat MEMBER, where Verilog's
    // own self-determined rule for that position gives it only the
    // operands' own (unmatched) width, not mimz's grown width.
    let src = "module Fuzz {\n  in p0: bits[6]\n  in p1: bits[15]\n  \
                in p2: bits[8]\n  out y: bits[31]\n  \
                y = {(p1 ^ extend(extend(1, 1), 15)), (extend(p2, 15) - p1)}\n}\n";
    // p0=55, p1=15470, p2=165 — the exact vector BUG-19's filing used.
    differential(src, &[("p0", 55), ("p1", 15470), ("p2", 165)]);
}

#[test]
fn bug_19_wrapping_sub_in_a_bitand_matches_icarus() {
    // docs/audit/bugs.md BUG-19's second filed repro (the +%/-% case found
    // during T2 v2).
    let src = "module Fuzz {\n  in p0: bits[15]\n  in p2: bits[3]\n  \
                out y: bits[18]\n  \
                y = ({p0, p2} & extend((extend(3, 3) -% p2), 18))\n}\n";
    // p0=7735, p2=5 — the exact vector BUG-19's filing used.
    differential(src, &[("p0", 7735), ("p2", 5)]);
}

#[test]
fn bug_20_slice_of_a_composite_expression_matches_icarus() {
    // docs/audit/bugs.md BUG-20's repro: slicing a non-identifier base —
    // Verilog's part-select grammar only accepts a plain signal name,
    // which `(p0 & p1)` is not.
    let src = "module Fuzz {\n  in p0: bits[4]\n  in p1: bits[4]\n  \
                out y: bits[2]\n  y = (p0 & p1)[1:0]\n}\n";
    differential(src, &[("p0", 0b1010), ("p1", 0b1100)]);
}

#[test]
fn bug_23_wrap_under_sibling_add_matches_icarus() {
    // docs/audit/bugs.md BUG-23's first filed repro (seed 12648435).
    let src = "module Fuzz {\n  in p0: signed[6]\n  in p1: signed[8]\n  \
                out y: bits[18]\n  \
                y = (extend(63, 7) + ({unsigned((extend(signed(extend(1, 1)), 6) ^ p0)), \
                {unsigned(p0), extend(21, 5)}} +% extend(63727, 17)))\n}\n";
    // p0=25, p1=208 — the exact vector BUG-23's filing used.
    differential(src, &[("p0", 25), ("p1", 208)]);
}

#[test]
fn bug_23_signed_wrap_operand_hoist_preserves_sign_extension() {
    // Signedness follow-up to BUG-23 (not a new numbered bug — the wrap
    // operand hoist added for BUG-23, `hoist_width_effect_operand` →
    // `hoist_slice_base_if_needed`, always declared the hoisted wire as
    // plain unsigned, even when the hoisted operand's own `Kind` is
    // signed. `p0 *% p1` here is a signed, wrapping direct operand of
    // the outer `+`, so it gets hoisted into a wire by that same BUG-23
    // mechanism; if that wire is unsigned, Verilog's "any unsigned
    // operand makes the whole expression unsigned" rule (LRM 5.5.1)
    // zero-extends it instead of sign-extending it once the surrounding
    // `+` is evaluated at its own (wider) context — changing the value.
    //
    // p0=-1, p1=1 (raw bits 0b11111, 0b00001): p0 *% p1 wraps to -1 in
    // 5 bits (0b11111). Correctly sign-extended into the 11-bit `+`
    // alongside p2=1, the sum is 1 + (-1) = 0. Zero-extended instead
    // (the bug), the wire reads as unsigned 31, giving 1 + 31 = 32.
    let src = "module Fuzz {\n  in p0: signed[5]\n  in p1: signed[5]\n  \
                in p2: signed[10]\n  out y: bits[11]\n  \
                y = unsigned(p2 + (p0 *% p1))\n}\n";
    differential(src, &[("p0", 0b11111), ("p1", 1), ("p2", 1)]);
}

#[test]
fn bug_23_wrap_under_sibling_add_inside_a_concat_matches_icarus() {
    // docs/audit/bugs.md BUG-23's second filed repro (seed 202427630,
    // the clocked case where the outer `+` IS a concat member and gets
    // hoisted by A1b's own mechanism, but the hoisted wire's contents
    // still connected the inner `-%` to the same wider context).
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in p0: bits[1]\n  \
                in p1: bits[3]\n  in p2: bits[5]\n  reg r0: bits[11] = 0\n  \
                reg r1: bits[13] = 0\n  out y: bits[26]\n  \
                on rise(clk) {\n    r0 <- extend(287, 11)\n    r1 <- extend(5643, 13)\n  }\n  \
                y = {(extend(1524, 14) | {r1, p0}), (p0[0:0] + (extend(extend(1, 1), 11) -% r0))}\n}\n";
    // held inputs p0=0, p1=7, p2=24 — the exact vector BUG-23's filing used.
    differential_clocked(src, &[("p0", 0), ("p1", 7), ("p2", 24)]);
}

#[test]
fn bug_23_wrap_directly_inside_a_concat_matches_icarus() {
    // Composability: a wrap operator directly inside a concat member —
    // BOTH the new width-effect hoist (Task 3, Step 2) and the
    // existing self-determined-mismatch check (A1b) are live at this
    // one call site. For a WRAP operator specifically, `infer_kind`
    // (`matched_result`) and `verilog_self_determined_kind` (also
    // `l.max(r)`, no growth) are provably always equal, so
    // `hoist_if_needed` is guaranteed to be a no-op here regardless —
    // this test can only confirm no infinite recursion and a correct
    // single wire for the wrap-in-concat shape specifically, matching
    // real Icarus. It does NOT exercise a real double-hoist (that
    // requires a LOSSLESS width-effect operand instead — see
    // `bug_19_lossless_sub_in_a_concat_hoists_exactly_one_wire` below,
    // which is the case that actually proves the double-hoist doesn't
    // occur).
    let src = "module Fuzz {\n  in p0: bits[4]\n  in p1: bits[4]\n  \
                out y: bits[6]\n  y = {p0[3:2], (p0 -% p1)}\n}\n";
    differential(src, &[("p0", 0b1010), ("p1", 0b1100)]);
}

#[test]
fn bug_19_lossless_sub_in_a_concat_hoists_exactly_one_wire() {
    // Task 3 finding (code review of `bug_23_wrap_directly_inside_a_concat_matches_icarus`,
    // above): that test only exercises a WRAP operator, for which
    // `hoist_if_needed` is provably always a no-op (see its updated
    // comment) — it can never catch a real double-hoist. A LOSSLESS
    // width-effect operand (`+`/`-`/`*`) sitting as a Concat member is
    // the case that actually can: it is matched by BOTH
    // `is_width_effect_binop` (BUG-23's own unconditional-on-shape
    // pre-hoist, `hoist_width_effect_operand`, which fires first and
    // replaces the operand's rendered text with a wire name) AND this
    // position's own self-determined-mismatch check (`hoist_if_needed`,
    // A1b) — whose mismatch was computed from the ORIGINAL AST node
    // (unaffected by the prior hoist) and so used to fire a SECOND
    // time, emitting a same-width alias of the first wire. Confirms
    // exactly ONE `__mimz_sub_` wire is emitted for this one operand
    // (not two).
    let src = "module Fuzz {\n  in p0: bits[8]\n  in p1: bits[15]\n  \
                out y: bits[16]\n  y = {(extend(p0, 15) - p1)}\n}\n";
    let v = compile_string(src).expect("lossless sub in concat should compile");
    let hoists = v.matches("assign __mimz_sub_").count();
    assert_eq!(
        hoists, 1,
        "expected exactly one hoisted wire for this operand (double-hoist regression), got:\n{v}"
    );
    // p0=165, p1=15470 — same vector BUG-19's own filing used for this shape.
    differential(src, &[("p0", 165), ("p1", 15470)]);
}

#[test]
fn bug_23_top_level_wrap_needs_no_hoist() {
    // Top-level exemption: a bare `y = a -% b` (no other operator
    // involved) must NOT emit a hoisted wire — same Verilog text as
    // before this plan, proving the skip-at-top-level case actually
    // skips (the assignment target's own declared width already pins
    // it correctly).
    let src = "module Fuzz {\n  in p0: bits[8]\n  in p1: bits[8]\n  \
                out y: bits[8]\n  y = (p0 -% p1)\n}\n";
    let v = compile_string(src).expect("bare top-level wrap should compile");
    assert!(
        !v.contains("__mimz_sub_"),
        "a bare top-level wrap operator should not be hoisted, got:\n{v}"
    );
    differential(src, &[("p0", 200), ("p1", 50)]);
}

#[test]
fn bug_24_shl_under_sibling_add_matches_icarus() {
    // docs/audit/bugs.md BUG-24's filed repro (seed 12648537, deep-N pass
    // N=500). `is_width_effect_binop` (`emit_verilog/expr.rs`) excluded
    // `Shl`/`Shr` on the mistaken assumption that a shift's value never
    // depends on the width it's computed at — but a shift's LEFT operand
    // is context-determined in real Verilog (widened to whatever ambient
    // context it sits in BEFORE the shift runs, same rule BUG-11 already
    // ground-truthed for the simulator). So `(p1 << extend(3, 4))` sitting
    // as a direct operand of the sibling `+` gets silently re-widened by
    // Verilog's own context propagation instead of staying pinned at its
    // own natural (14-bit) width, changing the shifted value.
    // Declared width is signed[33] — BUG-30 (`docs/audit/bugs.md`) makes
    // `<<` grow: `p1 << extend(3, 4)` grows by `extend`'s declared
    // `bits[4]` worst case (2^4 - 1 = 15) to signed[29]; `+` with `p1*p1`
    // (signed[28]) grows to signed[30]; `>> extend(0, 4)` doesn't grow;
    // the outer `<< extend(3, 2)` grows by 2^2 - 1 = 3 more, to signed[33].
    let src = "module Fuzz {\n  in p0: signed[12]\n  in p1: signed[14]\n  \
                out y: signed[33]\n  \
                y = ((((p1 * p1) + (p1 << extend(3, 4))) >> extend(0, 4)) << extend(3, 2))\n}\n";
    // p0=2024, p1=13855 — the exact vector BUG-24's filing used.
    differential(src, &[("p0", 2024), ("p1", 13855)]);
}

#[test]
fn bug_24_regression_shift_in_if_branch_stays_unhoisted() {
    // Regression guard for BUG-24's fix being applied too broadly on its
    // first pass: it added `Shl`/`Shr` to `is_width_effect_binop`
    // unconditionally, so `hoist_width_effect_operand` (called at
    // `IfExpr`'s `then`/`els`, `emit_verilog/expr.rs`) started hoisting a
    // shift BRANCH into its own narrow, bottom-up-inferred wire — but
    // `mimz-sim/src/sim/value.rs`'s `eval_ctx` `IfExpr` arm propagates the
    // SAME `expected_width` the whole `if`/`else` received into BOTH
    // branches, i.e. a shift branch here is CONTEXT-determined by
    // whatever the `if` itself sits in, not self-determined. This is the
    // exact shape that regressed `examples/english/shift.mimz` (BUG-6's
    // own guard) when the over-broad fix first shipped — same underlying
    // mechanism, through an `if` instead of `extend()`.
    let src = "module Fuzz {\n  in cond: bit\n  out y: bits[8]\n  \
                y = if cond { 1 << 3 } else { 0 }\n}\n";
    // cond=1: literal `1` (bottom-up width 1) widened to `y`'s 8-bit
    // assignment context BEFORE shifting gives 1 << 3 = 8. Hoisting the
    // `then` branch into its own 1-bit wire instead truncates `1 << 3` to
    // 0 (all bits shift out of a 1-bit register) before the ternary ever
    // runs — a real numeric divergence from Icarus.
    differential(src, &[("cond", 1)]);
}

#[test]
fn bug_28_extend_in_concat_matches_icarus() {
    // docs/audit/bugs.md BUG-28, Repro A: `extend(x, N)` renders as bare
    // `(x)` in Verilog — it relies entirely on the enclosing assignment's
    // context width to zero/sign-extend. A concat member has no such
    // context, so the padding bits are never materialized and every field
    // to the left silently shifts down. `self_determined.rs`'s exhaustive
    // `Builtin` match now reports `extend`'s own self-determined width
    // (the ARGUMENT's width, not N), so the emitter hoists it into a
    // `wire [N-1:0]` first.
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bits[12]\n  \
                y = { b, extend(a, 8) }\n}\n";
    // a=0b1111, b=0b1010 — the exact vector BUG-28's filing used.
    differential(src, &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_28_extend_in_replication_matches_icarus() {
    // docs/audit/bugs.md BUG-28, Repro B: same root cause as the concat
    // case, but in a replication body — another self-determined position.
    let src = "module Fuzz {\n  in a: bits[2]\n  out y: bits[8]\n  \
                y = {2{ extend(a, 4) }}\n}\n";
    // a=0b11 — the exact vector BUG-28's filing used.
    differential(src, &[("a", 0b11)]);
}

#[test]
fn bug_29_abs_in_concat_matches_icarus() {
    // docs/audit/bugs.md BUG-29: `abs(x)` renders to a ternary, which
    // Verilog self-determines at `max(operand widths)` — not at mimz's
    // grown `N+1` result. `Builtin::Abs` was entirely unclassified in
    // three places (`kind_is_inferrable`, `infer_call`, and this file's
    // `verilog_self_determined_kind`), so no hoist was ever attempted.
    let src = "module Fuzz {\n  in a: signed[4]\n  in b: bits[4]\n  out y: bits[9]\n  \
                y = { b, unsigned(abs(a)) }\n}\n";
    // a=-8 (raw 4-bit two's complement 0b1000), b=0b1010 — the exact
    // vector BUG-29's filing used.
    differential(src, &[("a", 0b1000), ("b", 0b1010)]);
}

#[test]
fn bug_24_regression_nested_shift_lhs_of_shift_stays_unhoisted() {
    // Regression guard for the other exclusion the over-broad BUG-24 fix
    // missed: when a shift's LEFT OPERAND is itself another shift,
    // `eval_ctx`'s `Binary` arm's `shift_ctx` gate is keyed on the OUTER
    // operator (`matches!(op, Shl | Shr)`), not the child's own kind — so
    // the OUTER shift's `expected_width` (here, `extend`'s target width,
    // threaded in because a shift's own type is LHS-preserving, so mimz's
    // static checker accepts this un-extended chain at only 3 bits) also
    // threads all the way down into the INNER shift's own left operand.
    // The inner shift must stay un-hoisted at this position, letting real
    // Verilog's ordinary context propagation reach it, instead of being
    // frozen at its own narrow bottom-up width.
    let src = "module Fuzz {\n  in p0: bits[3]\n  out y: bits[16]\n  \
                y = extend((p0 << 1) << 1, 16)\n}\n";
    // p0=5 (0b101): correctly widened to 16 bits before EITHER shift runs
    // (both shifts share the same 16-bit context, threaded down through
    // the nested-shift-LHS chain): 5 << 1 = 10, then 10 << 1 = 20.
    // Hoisting the inner `p0 << 1` into its own narrow 3-bit wire instead
    // truncates it to 2 (10 mod 8) before the outer shift ever runs; that
    // 2, then widened to 16 bits and shifted once more, gives 4 — a real
    // numeric divergence from Icarus (20 vs 4).
    differential(src, &[("p0", 5)]);
}

// ---------------------------------------------------------------------
// GAP-5's "position matrix" (docs/audit/gaps.md): every `Builtin`
// classified for its self-determined-position behavior, so a 14th builtin
// fails to compile here until classified — not just inside
// `self_determined.rs` itself.
//
// Architecture note, found while fixing BUG-29: `verilog_self_determined_
// kind`, `kind_is_inferrable`, and `hoist_if_needed` are pure functions of
// the EXPRESSION alone, and are the exact same three functions at every
// call site (`Concat`, `Replicate`, a comparison operand, a `$signed`/
// `$unsigned` argument) — there is one gate and one classifier, not five.
// So the real risk for a new builtin is "was it classified at all in these
// three places" (BUG-29's own gap — `self_determined.rs` alone was NOT
// sufficient), not "was it tested in enough AST positions." One
// differential test per testable builtin, at the simplest position to
// construct (a `Concat` member), exercises the full shared mechanism.
// Replication's body uses the byte-identical code path (`expr.rs`'s
// `Concat`/`Replicate` arms are the same two calls in the same order) —
// already cross-checked for `extend`/`abs` by the BUG-28/29 tests above.
// A replication COUNT never carries a runtime builtin call at all:
// `replicate_ty` requires it compile-time-constant, and `index_expr`'s
// `consteval::eval` short-circuit folds it straight to a literal before
// emit ever reaches this code path — untestable by construction, for any
// builtin.
// ---------------------------------------------------------------------

/// One `Builtin`'s operand shape, for building a minimal legal `Concat`
/// member around it. Each variant's own doc comment on `matrix_shape`
/// records the specific reasoning; this type only distinguishes the
/// shapes that need a differently-arity handwritten test below from the
/// ones that can never reach a rendered self-determined position at all.
enum MatrixShape {
    /// One `bits`/`signed` operand (`extend`/`trunc`/`signed`/`unsigned`/
    /// `abs`/`nand`/`nor`/`xnor`).
    Unary,
    /// Two same-width, same-kind operands (`min`/`max`).
    Binary,
    /// Never reaches a rendered self-determined position as a live
    /// sub-expression at all — compile-time-only, or lowered to items
    /// before emit.
    NotApplicable,
}

/// Exhaustive over `Builtin` (no wildcard arm) — a 14th variant is a
/// compile error here until classified.
fn matrix_shape(b: ast::Builtin) -> MatrixShape {
    use ast::Builtin::*;
    match b {
        Extend | Trunc | SignedCast | UnsignedCast | Abs | Nand | Nor | Xnor => MatrixShape::Unary,
        Min | Max => MatrixShape::Binary,
        // Compile-time only — the checker rejects it in a runtime value
        // position (`checker/widths/ops/builtins.rs`, E0407).
        Clog2 => MatrixShape::NotApplicable,
        // Lowered to items (registers/always blocks) before emit — never
        // an inline sub-expression.
        SyncDoubleFlop | SyncPulse => MatrixShape::NotApplicable,
    }
}

/// Every `Builtin` variant, by name — what the test below iterates.
/// `matrix_shape`'s own exhaustive match is the actual build-time guard;
/// this array is a second, independent check that nothing was classified
/// there but forgotten here.
const ALL_BUILTINS: &[ast::Builtin] = &[
    ast::Builtin::Extend,
    ast::Builtin::Trunc,
    ast::Builtin::SignedCast,
    ast::Builtin::UnsignedCast,
    ast::Builtin::Min,
    ast::Builtin::Max,
    ast::Builtin::Abs,
    ast::Builtin::Nand,
    ast::Builtin::Nor,
    ast::Builtin::Xnor,
    ast::Builtin::Clog2,
    ast::Builtin::SyncDoubleFlop,
    ast::Builtin::SyncPulse,
];

#[test]
fn every_builtin_is_classified_in_the_matrix() {
    assert_eq!(
        ALL_BUILTINS.len(),
        13,
        "a Builtin variant was added or removed without updating this matrix \
         (docs/audit/gaps.md GAP-5)"
    );
    for b in ALL_BUILTINS {
        let _ = matrix_shape(*b);
    }
}

#[test]
fn matrix_trunc_in_concat_matches_icarus() {
    // `trunc` renders as an explicit part-select `x[N-1:0]` — already
    // exactly N bits in Verilog, so `self_determined.rs` classifies it
    // `None` (no mismatch possible). This pins that classification against
    // real Icarus rather than leaving it as review-only prose.
    let src = "module Fuzz {\n  in a: bits[6]\n  in b: bits[4]\n  out y: bits[6]\n  \
                y = { b, trunc(a, 2) }\n}\n";
    // a=0b101011 (43): trunc to 2 bits keeps the low 2 (0b11=3). b=0b1010 (10).
    differential(src, &[("a", 0b101011), ("b", 0b1010)]);
}

#[test]
fn matrix_min_in_concat_matches_icarus() {
    // `min`/`max` render to a ternary whose operands are same-width by the
    // checker's own rule, so `max(operand widths) == N` — classified
    // `None`. Pinned against Icarus.
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  in c: bits[4]\n  \
                out y: bits[8]\n  y = { c, min(a, b) }\n}\n";
    // a=5, b=9, c=10: min(5,9)=5. y = (10<<4)|5.
    differential(src, &[("a", 5), ("b", 9), ("c", 10)]);
}

#[test]
fn matrix_max_in_concat_matches_icarus() {
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  in c: bits[4]\n  \
                out y: bits[8]\n  y = { c, max(a, b) }\n}\n";
    // a=5, b=9, c=10: max(5,9)=9. y = (10<<4)|9.
    differential(src, &[("a", 5), ("b", 9), ("c", 10)]);
}

#[test]
fn matrix_nand_in_concat_matches_icarus() {
    // Reductions are 1-bit on both sides regardless of operand width —
    // classified `None`. Pinned against Icarus.
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bits[5]\n  \
                y = { b, nand(a) }\n}\n";
    // a=0b1111: and-reduce=1, nand=0. b=0b1010 (10). y = (10<<1)|0 = 20.
    differential(src, &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn matrix_nor_in_concat_matches_icarus() {
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bits[5]\n  \
                y = { b, nor(a) }\n}\n";
    // a=0b0000: or-reduce=0, nor=1. b=0b1010 (10). y = (10<<1)|1 = 21.
    differential(src, &[("a", 0b0000), ("b", 0b1010)]);
}

#[test]
fn matrix_xnor_in_concat_matches_icarus() {
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bits[5]\n  \
                y = { b, xnor(a) }\n}\n";
    // a=0b1010: xor-reduce=1^0^1^0=0, xnor=1. b=0b0101 (5). y = (5<<1)|1 = 11.
    differential(src, &[("a", 0b1010), ("b", 0b0101)]);
}

#[test]
fn matrix_signed_unsigned_cast_roundtrip_in_concat_matches_icarus() {
    // `$signed`/`$unsigned`'s argument is self-determined at its own width
    // — same as mimz's own model — UNLESS the argument is itself a
    // mismatched sub-expression, caught by recursion rather than at this
    // arm. Nesting `unsigned(signed(a))` around a plain identifier (which
    // is never mismatched — `Ident` is the recursion's base case) confirms
    // the recursion through both cast arms stays a no-op when there is
    // nothing underneath to hoist, i.e. that widening `kind_is_inferrable`
    // to admit `Abs` didn't spuriously start hoisting this unrelated,
    // already-correct shape.
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bits[8]\n  \
                y = { b, unsigned(signed(a)) }\n}\n";
    // a=0b1011 (11), b=0b0101 (5). y = (5<<4)|11 = 91.
    differential(src, &[("a", 0b1011), ("b", 0b0101)]);
}

#[test]
fn bug_30_extend_of_a_shift_matches_a_named_wire_of_it() {
    // BUG-30's own filed repro (`docs/audit/bugs.md`): `extend(din << 2,
    // 8)` and a named `wire w: bits[N] = din << 2; extend(w, 8)` used to
    // produce DIFFERENT values from the IDENTICAL declared type `bits[4]`
    // — naming a subexpression silently changed the answer. Fixed by
    // making `<<` grow (Chisel's rule): `din << 2` is now `bits[6]`, wide
    // enough to hold every possible result, so both spellings agree
    // everywhere, matching the referential-transparency spec/01 promises.
    let src = "module Fuzz {\n  in din: bits[4]\n  \
                out direct: bits[8]\n  out named: bits[8]\n  \
                wire w: bits[6] = din << 2\n  \
                direct = extend(din << 2, 8)\n  \
                named  = extend(w, 8)\n}\n";
    let tokens = lexer::lex(src).unwrap_or_else(|e| panic!("unlexable: {e:?}"));
    let file = parser::parse(tokens).unwrap_or_else(|e| panic!("unparsable: {e:?}"));
    checker::check(std::slice::from_ref(&file)).unwrap_or_else(|e| {
        panic!(
            "checker rejected:\n{src}\n{}",
            diag::render(&e, src, "test")
        )
    });
    // din = 15 — BUG-30's own exact repro value (15 << 2 = 60, a 6-bit
    // value that BUG-30's old `bits[4]`-typed shift could not hold).
    let inputs: BTreeMap<String, mimz::sim::value::Bits> =
        [("din".to_string(), mimz::sim::value::Bits::Small(15))]
            .into_iter()
            .collect();
    let outputs = comb::eval_outputs(std::slice::from_ref(&file), None, &inputs, &BTreeMap::new())
        .unwrap_or_else(|e| panic!("our kernel rejected this program:\n{src}\n{}", e.msg));
    let row: BTreeMap<String, u128> = outputs
        .into_iter()
        .map(|o| {
            let v = match o.value {
                mimz::sim::value::Bits::Small(v) => v,
                mimz::sim::value::Bits::Wide(_) => unreachable!("narrow-width repro"),
            };
            (o.name, v)
        })
        .collect();
    assert_eq!(row["direct"], 60, "direct = extend(din << 2, 8)");
    assert_eq!(
        row["direct"], row["named"],
        "naming `din << 2` as `w` must not change the value (BUG-30)"
    );

    // Real hardware agrees too — same fixture, run through Icarus.
    differential(src, &[("din", 15)]);
}
