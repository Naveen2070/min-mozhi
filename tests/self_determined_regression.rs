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
                ast::Type::Bits(e) | ast::Type::Signed(e) => {
                    consteval::eval(e, &empty_env).expect("literal width") as u32
                }
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
    .unwrap_or_else(|e| panic!("our kernel rejected this program:\n{src}\n{e}"));
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
        .unwrap_or_else(|e| panic!("our kernel failed to elaborate:\n{src}\n{e}"));
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
    let src = "module Fuzz {\n  in p0: signed[12]\n  in p1: signed[14]\n  \
                out y: signed[29]\n  \
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
