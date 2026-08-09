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

use mimz::ast::{self, ExprKind, TopItem};
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
                // An enum-typed port's Verilog width is its total on-wire
                // size (tag + max payload) — the checker (already run by
                // `differential`, before this function is called) sets
                // `inferred_total_width` on the SAME `EnumDecl` this file's
                // own `TopItem::Enum` holds, via interior mutability.
                ast::Type::Named(qi) => file
                    .items
                    .iter()
                    .find_map(|i| match i {
                        TopItem::Enum(en) if en.name.name == qi.name.name => {
                            en.inferred_total_width.get()
                        }
                        _ => None,
                    })
                    .expect(
                        "module_ports: named port type must be a file-scope enum \
                             the checker has already sized",
                    ),
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
/// `module`: which module in `src` is the simulation top — required when
/// `src` declares more than one (`elaborate_project` cannot guess), `None`
/// for the (common) single-module case.
fn differential_clocked(src: &str, module: Option<&str>, held_inputs: &[(&str, u128)]) {
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

    let design = elaborate_project(std::slice::from_ref(&file), module, &BTreeMap::new())
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
    differential_clocked(src, None, &[("p0", 0), ("p1", 7), ("p2", 24)]);
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
// kind` (`self_determined.rs`) and `infer_kind`/`infer_call` (`kinds.rs`)
// are pure functions of the EXPRESSION alone, and are the exact same
// functions at every call site (`Concat`, `Replicate`, a comparison
// operand, a `$signed`/`$unsigned` argument) — there is one gate-and-
// classifier pair, not five. So the real risk for a new builtin is "was
// it classified at all in these two places" (BUG-29's own gap —
// `self_determined.rs` alone was NOT sufficient; BUG-41/BUG-42 are two
// further instances of the same class), not "was it tested in enough AST
// positions." One differential test per testable builtin, at the
// simplest position to construct (a `Concat` member), exercises the full
// shared mechanism. Replication's body uses the byte-identical code path
// (`expr.rs`'s `Concat`/`Replicate` arms are the same two calls in the
// same order) — already cross-checked for `extend`/`abs` by the
// BUG-28/29 tests above. A replication COUNT never carries a runtime
// builtin call at all: `replicate_ty` requires it compile-time-constant,
// and `index_expr`'s `consteval::eval` short-circuit folds it straight to
// a literal before emit ever reaches this code path — untestable by
// construction, for any builtin.
//
// Task 4 (`docs/plan/v0.2-correctness-remediation.local.md`): this file
// used to keep a SECOND, hand-maintained exhaustive match over `Builtin`
// here (`matrix_shape`/`ALL_BUILTINS`) purely to assert "every builtin
// was classified" — a parallel copy of the real gate-and-classifier
// match's own exhaustiveness (they are already `#[non_exhaustive]`-free,
// wildcard-free matches; the compiler already refuses to build on a
// 14th unclassified variant). It caught nothing that match doesn't
// already catch, and — being a shape descriptor, not a live check
// against `self_determined.rs`'s actual per-builtin RESULT — it could not
// have caught BUG-42 either: `Min`/`Max` were correctly bucketed
// `MatrixShape::Binary`, "expected" to need only a differential test, no
// closer to the real per-operand recursion bug than the classification
// this file already knew was fine. Deleted outright, along with the
// `ALL_BUILTINS.len() == 14` assert (the same "lying about coverage"
// problem GAP-5 already named for the position axis, now retired for the
// builtin axis too). The individual `matrix_*_in_concat_matches_icarus`
// tests below are the actual coverage and are unaffected — each pins one
// builtin's real classification against real Icarus, which no exhaustive-
// match assertion, past or present, could ever substitute for.
// ---------------------------------------------------------------------

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
fn matrix_encoding_of_tag_only_enum_in_concat_matches_icarus() {
    // `encoding(e)` renders as `$unsigned(...)`, exactly like `unsigned` —
    // classified `None` (no mismatch possible, same reasoning). `Light` has
    // 3 variants -> tag width clog2(3) = 2, no payload.
    //
    // An enum can only be exercised through the CLOCKED kernel path here —
    // `comb::eval_outputs` (what `differential` above uses) does not model
    // any enum-typed signal at all yet, port or wire (a separate,
    // pre-existing limitation, out of GAP-7's scope); the clocked path
    // (`elaborate_project`/`run`, what `differential_clocked` uses) already
    // handles an enum `reg` correctly — this is exactly
    // `examples/*/traffic_light.mimz`'s own working shape, with its state
    // additionally exposed via `encoding`.
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in b: bits[4]\n  out y: bits[6]\n  \
                enum Light { Red, Green, Blue }\n  \
                reg state: Light = Light.Red\n  \
                on rise(clk) {\n    state <- match state {\n      \
                Light.Red => Light.Green\n      Light.Green => Light.Blue\n      \
                Light.Blue => Light.Red\n    }\n  }\n  \
                y = { b, encoding(state) }\n}\n";
    differential_clocked(src, None, &[("b", 0b1010)]);
}

#[test]
fn matrix_encoding_of_payload_enum_in_concat_matches_icarus() {
    // `Packet` has 2 variants -> tag width 1; max payload 8 -> total 9. The
    // full tag+payload width is what `encoding` returns, not just the tag.
    // Same clocked-only reasoning as the tag-only case above; `p` toggles
    // between the two variants every cycle so both are exercised across
    // `differential_clocked`'s 8-cycle run.
    // `p` is a `wire`, not a `reg`: `consteval::eval` (the elaborator's
    // reg-reset-value folder) does not yet fold an `EnumConstruct`
    // expression, even with all-literal args, so `reg p: Packet =
    // Packet.Ctrl(0)` fails elaboration with "not a compile-time constant"
    // — a separate, narrow pre-existing gap, out of GAP-7's scope. A
    // combinational `wire` needs no reset value and sidesteps it entirely.
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  \
                in k: bits[4]\n  in v: bits[8]\n  in b: bits[4]\n  out y: bits[13]\n  \
                enum Packet { Ctrl(k: bits[4]), Data(v: bits[8]) }\n  \
                reg toggle: bit = 0\n  \
                on rise(clk) {\n    toggle <- toggle +% 1\n  }\n  \
                wire p: Packet = if toggle == 0 { Packet.Ctrl(k) } else { Packet.Data(v) }\n  \
                y = { b, encoding(p) }\n}\n";
    differential_clocked(
        src,
        None,
        &[("k", 0b0101), ("v", 0b00101101), ("b", 0b0110)],
    );
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
fn bug_42_min_max_mismatched_operand_matches_icarus() {
    // BUG-42 (docs/audit/bugs.md): `min`/`max` was classified `None` in
    // `verilog_self_determined_kind` on the reasoning that both operands
    // are same-width by the checker's own rule — true of mimz's widths,
    // false of Verilog's SELF-DETERMINED widths when an operand itself
    // renders as a mismatched sub-expression. `extend(p, 11)` renders as
    // the bare `(p)` (6 bits), not 11, so the ternary Verilog emits for
    // `min` self-determines to 6 bits, not mimz's 11 bits — Verilog then
    // zero-extends that narrower value into `y`'s 11 bits (0b110111 = 55)
    // instead of sign-extending mimz's own correct 11-bit result
    // (0b11111110111 = 2039).
    let src = "module Fuzz {\n  in p: signed[6]\n  out y: bits[11]\n  \
                y = unsigned(min(extend(p, 11), extend(p, 11)))\n}\n";
    // p = 0b110111 (-9 as signed[6]): min(-9, -9) sign-extended to 11
    // bits = 0b11111110111 = 2039.
    differential(src, &[("p", 0b110111)]);
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

#[test]
fn bug_35_shift_with_a_builtin_call_left_operand_in_a_concat_matches_icarus() {
    // BUG-35's own filed repro (`docs/audit/bugs.md`): a shift whose LEFT
    // OPERAND is a builtin call (`nand(p1)`), sitting inside a concat
    // member, was never hoisted — `nand`/`nor`/`xnor`/`min`/`max` fell
    // through `kind_is_inferrable`'s `_ => false` arm (and `infer_call`'s
    // matching `panic!` arm), so the ENCLOSING shift was treated as
    // "can't analyze" and skipped the hoist entirely, letting Verilog
    // compute it self-determined at `nand(p1)`'s own 1-bit width instead
    // of mimz's declared growth width.
    let src = "module Fuzz {\n  in p0: bits[7]\n  in p1: bits[9]\n  out y: bits[1]\n  \
                y = (extend(15736, 15) <= {((p0 *% p0) | extend(p1[8:7], 7)), \
                (nand(p1) << extend(5, 3))})\n}\n";
    // p0=55, p1=110 — the exact vector BUG-35's filing used (kernel said
    // y=1, real Icarus said y=0 before this fix).
    differential(src, &[("p0", 55), ("p1", 110)]);
}

#[test]
fn bug_36_trunc_of_a_concat_hoists_the_base_first() {
    // BUG-36's own filed repro shape (`docs/audit/bugs.md`, simplified —
    // the clock/reset/reg machinery in the original filing was incidental,
    // not load-bearing): `Builtin::Trunc` renders as an explicit
    // part-select `x[N-1:0]`, but only ever hoisted `x` when it was a
    // width-effect-binop/shift base (BUG-23/24's concern) — a concat base
    // rendered ungrouped as `{p0, extend(39, 7)}[(15)-1:0]`, which real
    // Verilog rejects outright (part-select only accepts a plain
    // identifier, the same BUG-20 grammar constraint `ExprKind::Slice`
    // already had to solve). `mimz check`/`mimz test` passed clean;
    // `mimz compile`'s own output failed to elaborate under `iverilog`.
    let src = "module Fuzz {\n  in p0: bits[11]\n  out y: bits[15]\n  \
                y = trunc({p0, extend(39, 7)}, 15)\n}\n";
    // p0 = 0b101_0101_0101 (1365) — any value exercises the syntax fix;
    // the value itself just needs to round-trip through both judges.
    differential(src, &[("p0", 0b101_0101_0101)]);
}

// ---------------------------------------------------------------------
// BUG-43 (docs/audit/bugs.md): a negative literal evaluated at its
// magnitude's own natural width. Not a self-determined-POSITION bug, but
// the same judge pair this file exists to run (our kernel vs. real
// Icarus on the emitted Verilog) is exactly what the fix has to satisfy
// — the emitter was already correct here and only the simulator moved,
// so a sim-only unit test (`crates/mimz-sim/src/sim/value/tests.rs`)
// cannot prove the two now agree.
// ---------------------------------------------------------------------

#[test]
fn bug_43_negative_literal_in_a_wire_matches_icarus() {
    // `-1` is the worst case of the family (`natural_width(1) == 1`, so
    // it negated inside ONE bit and came out `+1`) and the most common
    // negative constant in real RTL. Emitted as `assign w = (-1);`,
    // which Verilog sizes from the 8-bit context -> 255.
    let src = "module Fuzz {\n  in a: bits[8]\n  out y: bits[8]\n  \
               wire w: signed[8] = -1\n  y = unsigned(w) & a\n}\n";
    differential(src, &[("a", 0xFF)]);
}

#[test]
fn bug_43_negative_literal_comparison_matches_icarus() {
    // `q == -9` was silently FALSE in the simulator (it compared against
    // +7) and true in Verilog. `q` holds -9's 6-bit two's complement.
    let src = "module Fuzz {\n  in q: signed[6]\n  out y: bits[1]\n  \
               y = if q == -9 { 1 } else { 0 }\n}\n";
    differential(src, &[("q", 0b110111)]);
}

#[test]
fn bug_43_negative_literal_clamp_idiom_matches_icarus() {
    // The shape `showcase/pid_controller.mimz` ships (`max(-128,
    // min(total, 127))`), with a NON-power-of-two bound so the buggy
    // `2^natural_width(n) - n` rule and the correct value differ:
    // `natural_width(100) == 7`, so `-100` evaluated as `128 - 100 = 28`.
    let src = "module Fuzz {\n  in x: signed[16]\n  out y: signed[16]\n  \
               y = max(-100, min(x, 100))\n}\n";
    // x = -1000 (0xFC18): below the clamp floor, so the result is the
    // floor itself — the operand that was wrong.
    differential(src, &[("x", 0xFC18)]);
}

// ---------------------------------------------------------------------
// BUG-41 (docs/audit/bugs.md): `kind_is_inferrable`'s own wildcard
// (`expr.rs`) swallowed `FnCall`/`Field`/`IfExpr`/`Match`/`Index`, so the
// exhaustive `Builtin` classifier above (`self_determined.rs`) never even
// ran for an expression CONTAINING one of them as an operand — reopening
// BUG-28's exact defect through a different AST shape. Each test below is
// one of the review's filed reproductions
// (docs/audit/review-2026-08-07.md, Part 3, BUG-41).
// ---------------------------------------------------------------------

#[test]
fn bug_41_fn_call_operand_of_add_in_concat_matches_icarus() {
    // Repro ①: a `fn` call as an operand of a lossless `+` sitting as a
    // concat member. `ident4`'s return `Kind` used to be entirely
    // unclassified, so the enclosing `+` was declared "can't analyze" and
    // never hoisted, letting Verilog self-determine it at 4 bits instead
    // of mimz's grown 5.
    let src = "fn ident4(x: bits[4]) -> bits[4] {\n  x\n}\n\n\
                module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bits[9]\n  \
                y = { b, ident4(a) + a }\n}\n";
    // a=0b1111 (15), b=0b1010 (10) — the exact vector the review used.
    differential(src, &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_41_instance_port_operand_of_add_in_concat_matches_icarus() {
    // Repro ②: an instance output port (`s.q`) as an operand of the same
    // lossless `+` — same root cause, different unclassified shape.
    // `comb::eval_outputs` (`differential`, above) explicitly rejects any
    // module that instantiates a sub-module ("the evaluator does not
    // elaborate instances yet") — this needs the real engine
    // (`differential_clocked`), even though the design itself has no
    // registers of its own.
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[9]\n  let s = Sub() { x: a }\n  \
                y = { b, s.q + a }\n}\n\n\
                module Sub {\n  in x: bits[4]\n  out q: bits[4]\n  q = x\n}\n";
    differential_clocked(src, Some("Fuzz"), &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_41_if_expr_operand_of_add_in_concat_matches_icarus() {
    // Repro ③: an `if`/`else` expression as an operand of the same `+`.
    let src = "module Fuzz {\n  in cond: bit\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[9]\n  \
                y = { b, (if cond { a } else { b }) + a }\n}\n";
    differential(src, &[("cond", 1), ("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_41_mem_read_operand_of_add_in_concat_matches_icarus() {
    // Repro ④: a memory read (`m[addr]`) as an operand of the same `+`.
    // `m[0]` is written to its max 4-bit value on the first rising edge so
    // the growth bit the outer `+` needs is actually exercised (0+0 would
    // "match" trivially regardless of the bug).
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in raddr: bits[2]\n  \
                in b: bits[4]\n  out y: bits[9]\n  mem m: bits[4][4] = 0\n  \
                on rise(clk) {\n    m[0] <- 15\n  }\n  \
                y = { b, m[raddr] + m[raddr] }\n}\n";
    differential_clocked(src, None, &[("raddr", 0), ("b", 0b1010)]);
}

#[test]
fn bug_41_extend_of_a_fn_call_in_concat_matches_icarus() {
    // Repro ⑤: BUG-28 verbatim, reached through a `fn` call instead of a
    // bare identifier — the review found this reproduces the ORIGINAL
    // bug's exact wrong emission byte-for-byte.
    let src = "fn ident4(x: bits[4]) -> bits[4] {\n  x\n}\n\n\
                module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bits[12]\n  \
                y = { b, extend(ident4(a), 8) }\n}\n";
    differential(src, &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_43_negative_literal_in_a_reg_reset_matches_icarus() {
    // Reset values happened to be CORRECT before the fix (they take a
    // width-aware path), which is exactly why the bug went unnoticed —
    // the most visible use of a negative constant is the one that worked.
    // Pinned so the fix does not regress the path that was already right.
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[8]\n  \
               out y: bits[8]\n  reg r: signed[8] = -1\n  \
               on rise(clk) {\n    r <- r\n  }\n  y = unsigned(r) & a\n}\n";
    differential_clocked(src, None, &[("a", 0xFF)]);
}

#[test]
fn bug_44_trunc_of_a_signed_value_stays_signed_in_verilog() {
    // BUG-44 (`docs/audit/bugs.md`), minimized from clocked-fuzz seed
    // 202427830. `trunc(x, N)` KEEPS its operand's signedness — the
    // checker (`widths/ops/builtins.rs`: `Ty::Signed(_) => Ty::Signed(n)`),
    // the simulator (`value/fn_eval.rs`: `Val::new(.., v.signed)`) and the
    // emitter's own classifier (`kinds.rs`: `signed: base_signed`) all say
    // so. But it RENDERS as a Verilog part-select `x[N-1:0]`, and a
    // part-select is unconditionally UNSIGNED in Verilog-2005 (IEEE 1364-
    // 2005 section 5.1.7) even off a `signed` wire — so the emitted text
    // disagrees with all three. This is the same disagreement-between-two-
    // implementations-of-one-rule family as BUG-41/42, on the `signed`
    // half of `Kind` rather than the `width` half (which is why Task 3's
    // re-audit — reasoning only about width — kept `Trunc` as "no
    // mismatch possible").
    //
    // a = 236 = 0b11101100 (signed[8] = -20). trunc(a, 3) = 0b100, which
    // as signed[3] is -4; sign-extended to signed[6] that is 0b111100 =
    // 60. Zero-extending it instead (the pre-fix emission) gives 4.
    let src = "module Fuzz {\n  in a: signed[8]\n  out y: signed[6]\n  \
                y = extend(trunc(a, 3), 6)\n}\n";
    differential(src, &[("a", 236)]);
}

#[test]
fn bug_44_trunc_of_a_signed_value_as_a_multiply_operand() {
    // The shape the fuzz seed actually took: the lost signedness does not
    // just mis-extend, it makes the WHOLE surrounding Verilog expression
    // unsigned — mixing one unsigned operand into `*` demotes the multiply
    // (IEEE 1364-2005 section 5.1.7), so the sibling's own `$signed` is
    // discarded too.
    //
    // signed(extend(3, 3)) = 3, trunc(a, 3) = -4 -> 3 * -4 = -12, which as
    // bits[6] is 52. Pre-fix Verilog computed 3 * 4 = 12.
    let src = "module Fuzz {\n  in a: signed[8]\n  out y: bits[6]\n  \
                y = unsigned(signed(extend(3, 3)) * trunc(a, 3))\n}\n";
    differential(src, &[("a", 236)]);
}

// ---------------------------------------------------------------------
// BUG-48 (`docs/audit/bugs.md`) — `kinds::infer_kind` is now the SOLE
// gate (BUG-41 collapsed the old two-match hand-sync into one), but it
// still ends `_ => None`, and two of the arms BUG-41's fix added take an
// early `return None` for shapes narrower than what the checker actually
// accepts: `Field` only resolves a `Ident.field` base, not
// `Index.field` (an array-instance output port); `Slice` only
// const-folds a literal bound, not a `const`-valued one. Both are
// ordinary, already-shipped syntax (`examples/english/ripple_adder.mimz`
// uses `fa[i - 1].cout`) — same "unclassified shape -> silently skip the
// hoist" root cause as BUG-41, same surface, third round running.
// ---------------------------------------------------------------------

#[test]
fn bug_48_array_instance_port_operand_of_add_in_concat_matches_icarus() {
    // `s[0].q` is `Field { base: Index { base: Ident("s"), index: 0 } }` —
    // `infer_kind`'s `Field` arm required `base.kind == Ident` and
    // returned `None` for anything else, so the enclosing `+` was
    // declared "can't analyze" and never hoisted.
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[9]\n  repeat i: 0..1 {\n    let s[i] = Sub() { x: a }\n  }\n  \
                y = { b, s[0].q + a }\n}\n\n\
                module Sub {\n  in x: bits[4]\n  out q: bits[4]\n  q = x\n}\n";
    differential_clocked(src, Some("Fuzz"), &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_48_extend_of_an_array_instance_port_in_concat_matches_icarus() {
    // BUG-28 verbatim, reached through an array-instance port instead of
    // a bare identifier — byte-identical wrong emission to round 1's
    // Repro A and round 2's BUG-41 repro (5).
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[12]\n  repeat i: 0..1 {\n    let s[i] = Sub() { x: a }\n  }\n  \
                y = { b, extend(s[0].q, 8) }\n}\n\n\
                module Sub {\n  in x: bits[4]\n  out q: bits[4]\n  q = x\n}\n";
    differential_clocked(src, Some("Fuzz"), &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_48_const_bounded_slice_operand_of_add_in_concat_matches_icarus() {
    // `a[HI:0]` folds fine in the EMITTED TEXT (`a[3:0]`) — the checker's
    // own `slice_ty` already const-evaluates `HI` to accept the program —
    // but `infer_kind`'s `Slice` arm used the literal-only `const_fold`,
    // which sees an `Ident` for `HI` and returns `None`.
    let src = "module Fuzz {\n  const HI: int = 3\n  in a: bits[8]\n  in b: bits[4]\n  \
                out y: bits[9]\n  y = { b, a[HI:0] + a[HI:0] }\n}\n";
    differential(src, &[("a", 0b00001111), ("b", 0b1010)]);
}

// ---------------------------------------------------------------------
// BUG-49 (`docs/audit/bugs.md`) — the same `infer_kind` residue as
// BUG-48, reached through the OTHER hoist BUG-41's fix left gated on it
// (BUG-36's "hoist a composite `trunc` base to a named wire", since
// Verilog's part-select grammar only accepts an identifier). `mimz
// check`/`mimz test` both pass; `mimz compile` emits a part-select on a
// COMPOSITE expression, which Icarus rejects with a syntax error rather
// than a wrong value — `differential`/`differential_clocked`'s own
// iverilog BUILD step (not the value comparison after it) is the
// assertion here.
// ---------------------------------------------------------------------

#[test]
fn bug_49_trunc_of_an_array_instance_port_sum_elaborates() {
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  \
                out y: bits[3]\n  repeat i: 0..1 {\n    let s[i] = Sub() { x: a }\n  }\n  \
                y = trunc(s[0].q + a, 3)\n}\n\n\
                module Sub {\n  in x: bits[4]\n  out q: bits[4]\n  q = x\n}\n";
    differential_clocked(src, Some("Fuzz"), &[("a", 0b1111)]);
}

#[test]
fn bug_49_trunc_of_a_const_bounded_slice_sum_elaborates() {
    let src = "module Fuzz {\n  const HI: int = 3\n  in a: bits[8]\n  \
                out y: bits[3]\n  y = trunc(a[HI:0] + a[HI:0], 3)\n}\n";
    differential(src, &[("a", 0b00001111)]);
}

#[test]
fn bug_47_signed_right_shift_into_a_wider_assignment() {
    // BUG-47 (`docs/audit/bugs.md`), minimized from clocked-fuzz seed
    // 202428271, found by the v0.2 release gate at N=1000.
    //
    // Every prior hoist in this file guards a SELF-DETERMINED position
    // (concat member, comparison operand, `$signed`/`$unsigned` argument).
    // This one is CONTEXT-determined: the top level of an assignment RHS,
    // where Verilog widens the operand to the target's width BEFORE
    // evaluating. `expr.rs`'s own comment asserts that needs no hoist —
    // "a bare top-level `y = a -% b` needs no hoist, the assignment
    // target's own declared width already pins it correctly" — which holds
    // for every operator whose value depends only on its low bits, and
    // fails for `>>`, where the operand's WIDTH changes the VALUE: sign
    // extension shifts real bits down into the result.
    //
    // p1 = 0b1111 = -1 as signed[4]. mimz shifts within 4 bits:
    // 0b1111 >> 2 = 0b0011 = 3. Verilog sign-extends p1 to the assignment's
    // 20 bits first (0xFFFFF) and shifts that: 0x3FFFF = 262143.
    let src = "module Fuzz {\n  in p1: signed[4]\n  out y: signed[20]\n  \
                y = extend((p1 >> extend(2, 5)), 20)\n}\n";
    differential(src, &[("p1", 15)]);
}

#[test]
fn bug_47_signed_right_shift_with_a_composite_left_operand() {
    // Same defect with a composite left operand rather than a bare port —
    // pinned separately because the composite case is NOT hoisted either
    // (`assign y = (((p1 ^ p2) >> 5'd2));`), and because it very nearly
    // read as a passing case during diagnosis: with p2 = 8 the operand
    // `p1 ^ p2` is 0b0111 = +7, so sign extension adds zeros and both
    // sides agree. It only diverges once the operand is negative.
    //
    // p1 = 15 (-1), p2 = 0 -> p1 ^ p2 = 0b1111 = -1. mimz: 3. Verilog:
    // 262143.
    let src = "module Fuzz {\n  in p1: signed[4]\n  in p2: signed[4]\n  \
                out y: signed[20]\n  y = extend(((p1 ^ p2) >> extend(2, 5)), 20)\n}\n";
    differential(src, &[("p1", 15), ("p2", 0)]);
}

#[test]
fn bug_47_signed_right_shift_by_a_port_amount() {
    // The shift AMOUNT being a runtime port rather than a literal changes
    // nothing about the cause — the defect is the left operand's width, not
    // the right's. Kept because a fix that special-cases a constant shift
    // amount would pass the two tests above and still miss this.
    //
    // p1 = -1, shift by 8: mimz shifts 4 bits by 8 = 0. Verilog shifts the
    // sign-extended 20-bit 0xFFFFF by 8 = 0xFFF = 4095.
    let src = "module Fuzz {\n  in p1: signed[4]\n  in p2: signed[4]\n  \
                out y: signed[20]\n  y = extend((p1 >> unsigned(p2)), 20)\n}\n";
    differential(src, &[("p1", 15), ("p2", 8)]);
}

#[test]
fn bug_47_unsigned_right_shift_and_left_shift_stay_unhoisted() {
    // The other half of the boundary, pinned so a fix cannot over-reach.
    // An UNSIGNED right shift is safe (context extension is zero-fill, so
    // no new bits shift down) and a LEFT shift is safe (mimz grows the
    // width too, and the low bits agree). Both must keep matching — and
    // ideally without a hoisted wire, which is why `emit_verilog`'s own
    // golden files are the second half of this guard.
    let src = "module Fuzz {\n  in p1: signed[4]\n  out y: bits[20]\n  \
                y = extend((unsigned(p1) >> extend(2, 5)), 20)\n}\n";
    differential(src, &[("p1", 15)]);
    let src = "module Fuzz {\n  in p1: signed[4]\n  out y: signed[20]\n  \
                y = extend((p1 << extend(2, 2)), 20)\n}\n";
    differential(src, &[("p1", 15)]);
}

// ---------------------------------------------------------------------
// GAP-13 (`docs/audit/gaps.md`) — the position matrix has a `Builtin`
// axis (`self_determined.rs`'s own exhaustive match, plus the deleted-
// but-still-enforced `matrix_*` tests below) and no `ExprKind` axis. The
// gate that decides whether a hoist even runs (`kinds::infer_kind`) is
// not wildcard-free over `ExprKind` — round 3 found two shapes (BUG-48)
// that fell through it by hand, three rounds after BUG-41 found the
// first five the same way. These two tests close the only gaps THIS
// axis's own table (below) found that no existing test already covered
// — `Match` had no differential at all, and `Replicate` was only ever
// exercised as the OUTER container (its own body as the self-determined
// position), never as a NESTED operand needing `infer_kind` itself
// (mirroring BUG-36's `Concat`-in-`trunc` shape exactly).
// ---------------------------------------------------------------------

#[test]
fn shape_match_operand_of_add_in_concat_matches_icarus() {
    // `Match` is classified in `infer_kind` (the first arm to resolve
    // wins, same reasoning as `IfExpr`) but had no differential proving
    // it — the round-3 review swept it by hand and found it correct, but
    // nothing pinned that. Same shape as `bug_41_if_expr_operand_of_add`.
    let src = "module Fuzz {\n  in sel: bit\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[9]\n  \
                y = { b, (match sel {\n    true => a\n    false => b\n  }) + a }\n}\n";
    differential(src, &[("sel", 1), ("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn shape_replicate_nested_in_trunc_hoists_the_base() {
    // `Replicate` as a NESTED operand (not the outer self-determined
    // container itself) — mirrors `bug_36_trunc_of_a_concat_hoists_the_
    // base_first` exactly, one `ExprKind` over: `Builtin::Trunc`'s base
    // must be a plain identifier in Verilog's part-select grammar, so a
    // replication base needs the same hoist a concat base does.
    let src = "module Fuzz {\n  in p0: bits[3]\n  out y: bits[5]\n  \
                y = trunc({2{p0}}, 5)\n}\n";
    differential(src, &[("p0", 0b101)]);
}

/// GAP-13's own axis — exhaustive over `ExprKind`, no wildcard. Never
/// called (a compile-time-only property): its only job is that adding an
/// `ExprKind` variant without a line here fails the build, the same
/// enforcement the now-deleted `matrix_shape`/`ALL_BUILTINS` gave the
/// `Builtin` axis (Task 4 of `docs/plan/v0.2-correctness-remediation.
/// local.md` deleted those on the correct reasoning that the REAL
/// matches (`self_determined.rs`, `kinds::infer_call`) are themselves
/// wildcard-free over `Builtin` — this axis needs its own copy because
/// `kinds::infer_kind`'s real match is NOT wildcard-free over `ExprKind`
/// (`_ => None` at the end), so nothing else in the compiled program
/// enforces it. Each arm names either the test(s) that differentially
/// prove the shape hoists (or correctly doesn't) in a self-determined
/// position, or the reason it structurally cannot appear there at all.
#[allow(dead_code)]
fn expr_kind_self_determined_coverage(kind: &ExprKind) -> &'static str {
    match kind {
        ExprKind::Int { .. } => {
            "NotApplicable: bare literal — self_determined.rs's own \
             `Int => None` arm; Verilog's self-determined width for an \
             unsized literal already equals mimz's, nothing to compare"
        }
        ExprKind::Bool(_) => "NotApplicable: same reasoning as Int — `Bool => None`",
        ExprKind::Ident(_) => {
            "NotApplicable: a signal's declared width IS its Verilog \
             self-determined width, by definition — `Ident => None`"
        }
        ExprKind::Field { .. } => {
            "covered by bug_41_instance_port_operand_of_add_in_concat_matches_icarus \
             (plain instance) and bug_48_array_instance_port_operand_of_add_in_concat_matches_icarus \
             (array instance — the shape BUG-48 found unclassified)"
        }
        ExprKind::Unary { .. } => {
            "NotApplicable: every UnOp self-determines identically in mimz \
             and Verilog — operand-width for Neg/BitNot/LogicNot, always \
             exactly 1 bit for the three reductions (RedAnd/RedOr/RedXor), \
             in every context — self_determined.rs's own catch-all confirms \
             no arm ever differs. (kinds.rs's own Unary arm forwards the \
             inner Kind unchanged regardless of `op`, which is imprecise \
             for a reduction's width specifically — but since this arm \
             here is never compared against anything, that imprecision \
             is dead weight, not a reachable defect.)"
        }
        ExprKind::Binary { .. } => {
            "covered by bug_19_lossless_sub_in_a_concat_matches_icarus and \
             the wider bug_19/23/24/30/35/42/47 family"
        }
        ExprKind::IfExpr { .. } => {
            "covered by bug_41_if_expr_operand_of_add_in_concat_matches_icarus"
        }
        ExprKind::Match { .. } => "covered by shape_match_operand_of_add_in_concat_matches_icarus",
        ExprKind::Concat(_) => {
            "covered by bug_36_trunc_of_a_concat_hoists_the_base_first \
             (Concat as a NESTED operand, not the outer container)"
        }
        ExprKind::Replicate { .. } => {
            "covered by bug_28_extend_in_replication_matches_icarus (as the \
             outer self-determined container) and \
             shape_replicate_nested_in_trunc_hoists_the_base (as a nested operand)"
        }
        ExprKind::Index { .. } => {
            "covered by bug_41_mem_read_operand_of_add_in_concat_matches_icarus \
             (the mem-read branch — the interesting one, since it carries a \
             real element Kind); the plain bit-select branch is always \
             exactly 1 bit in both mimz and Verilog, so no mismatch is \
             possible there and no separate test is needed for it"
        }
        ExprKind::Slice { .. } => {
            "covered by bug_20_slice_of_a_composite_expression_matches_icarus \
             (literal bounds) and bug_48_const_bounded_slice_operand_of_add_in_concat_matches_icarus \
             (const-valued bounds — the shape BUG-48 found unclassified)"
        }
        ExprKind::Call { .. } => {
            "covered by the matrix_* family (one differential per Builtin) \
             plus self_determined.rs's own exhaustive Builtin match, which \
             fails the build on an unclassified Builtin independently of \
             this axis"
        }
        ExprKind::FnCall { .. } => {
            "covered by bug_41_fn_call_operand_of_add_in_concat_matches_icarus \
             and bug_41_extend_of_a_fn_call_in_concat_matches_icarus"
        }
        ExprKind::BundleLit(_) => {
            "NotApplicable: bundle-typed — never legal as a concat/\
             arithmetic/comparison/cast operand (not a `bits` type); the \
             checker rejects it upstream"
        }
        ExprKind::ArrayLit(_) => {
            "NotApplicable: array-typed — same reasoning as BundleLit, \
             checker-rejected upstream"
        }
        ExprKind::EnumConstruct { .. } => {
            "NotApplicable: an enum-typed value in a concat is rejected \
             outright by the checker (E0403, docs/audit/bugs.md BUG-31) \
             before this code ever runs"
        }
    }
}
