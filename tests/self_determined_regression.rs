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
fn matrix_signed_unsigned_cast_recursion_catches_a_mismatched_operand_matches_icarus() {
    // Task 4 (round-4 plan, `docs/plan/v0.2-class-closure-round4.local.md`):
    // the test above's own comment admits it only proves the recursion is a
    // NO-OP when nothing underneath is mismatched — it never proved the
    // recursion actually CATCHES a real one, the exact gap BUG-42 shipped
    // from for `min`/`max`. `extend(a, 8)` renders as bare `(a)` (4 bits),
    // not 8, three levels under two casts — if either `SignedCast`'s or
    // `UnsignedCast`'s arm failed to recurse, this would render 4 bits
    // where mimz expects 8 and silently miscompile the same way BUG-52's
    // repros did.
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bits[12]\n  \
                y = { b, unsigned(signed(extend(a, 8))) }\n}\n";
    // a=0b1011 (11), b=0b0101 (5): extend/signed/unsigned round-trip 11
    // unchanged at 8 bits. y = (5<<8)|11 = 1291.
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

/// BUG-53's three shapes (`docs/audit/bugs.md`) are, per the review's own
/// table, rejected by `mimz sim` today (`error: unknown signal`, `S0125`,
/// `S0126`) even though `mimz check` accepts them — a SEPARATE, tracked
/// check/sim/emit disagreement, not this fix's job to close. So unlike
/// `differential_clocked` (which needs `elaborate_project`/`run` — our own
/// kernel — to succeed first), this compiles straight to Verilog and checks
/// ONE output value against real Icarus directly, the same "hand-computed
/// expected value" proof `tests/icarus.rs`'s own hand differentials use.
fn emitter_only_clocked_check(src: &str, held_inputs: &[(&str, u128)], expected: u128) {
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

    let (ins, outs) = module_ports(&file);
    let held: BTreeMap<String, u128> = held_inputs
        .iter()
        .map(|(n, v)| (n.to_string(), *v))
        .collect();
    assert_eq!(
        held.len(),
        ins.len(),
        "every declared `in` port must have a value in `held_inputs`"
    );
    let inputs_meta: Vec<(String, u32, u128)> =
        ins.iter().map(|(n, w)| (n.clone(), *w, held[n])).collect();

    const CYCLES: u64 = 4;
    const RESET_CYCLES: u64 = 1;

    let tag = format!("{:x}", md5_ish(src));
    let path = std::env::temp_dir().join(format!("mimz_bug53_{tag}.mimz"));
    std::fs::write(&path, src).unwrap();
    let design_v = support::compile_example(&path);

    let tb = support::clocked_testbench(
        "Fuzz",
        &[],
        "clk",
        Some("rst"),
        &inputs_meta,
        &outs,
        CYCLES,
        RESET_CYCLES,
    );
    let example = format!("bug-53 emitter-only regression {tag}");
    let stdout = support::run_vvp(&bin, &example, &design_v, &tb);
    let icarus = support::parse_icarus(&stdout);
    let last_cycle = *icarus.keys().max().expect("Icarus produced no rows");
    let row = &icarus[&last_cycle];
    let (out_name, _) = &outs[0];
    assert_eq!(
        row[out_name], expected,
        "output `{out_name}` at cycle {last_cycle}: Icarus says {}, expected {expected}\nsource:\n{src}",
        row[out_name]
    );
}

#[test]
fn bug_53_control_case_non_zero_base_identity_index_still_hoists() {
    // The review's own control case, which is what pins the diagnosis on
    // "offset vs. identity index", not "non-zero base": `s[i]` at a
    // `repeat i: 1..2` (base 1, not 0) still has the index expression
    // EQUAL to the loop counter — unlike `bug_53_offset_array_instance_
    // index_matches_icarus`'s `s[i + 1]` — so this shape already hoisted
    // correctly before this fix, and must keep doing so after it. Unlike
    // the three BUG-53 repros above, `mimz sim` DOES elaborate this shape
    // (only the offset/nested/const-if forms don't), so this uses the
    // full kernel-vs-Icarus differential, not the emitter-only check.
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[9]\n  repeat i: 1..2 {\n    let s[i] = Sub() { x: a }\n  }\n  \
                y = { b, s[1].q + a }\n}\n\n\
                module Sub {\n  in x: bits[4]\n  out q: bits[4]\n  q = x\n}\n";
    differential_clocked(src, Some("Fuzz"), &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_53_offset_array_instance_index_matches_icarus() {
    // `insert_repeat_instance_output_kinds` used to key every element by
    // the `repeat` LOOP COUNTER (`i`), not the instance's own `index`
    // EXPRESSION folded the same way `inst_name` (`emit_verilog/mod.rs`)
    // renders it — `s[i + 1]` diverges from `i` the moment the index isn't
    // the identity, so `decls` never had the key `s__1_q` that
    // `infer_kind`'s `Field` arm looked up, and the enclosing `+` was
    // never hoisted. `a = 0b1111`, `b = 0b1010` → 350 (`{b, a+a}`).
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[9]\n  repeat i: 0..1 {\n    let s[i + 1] = Sub() { x: a }\n  }\n  \
                y = { b, s[1].q + a }\n}\n\n\
                module Sub {\n  in x: bits[4]\n  out q: bits[4]\n  q = x\n}\n";
    emitter_only_clocked_check(src, &[("a", 0b1111), ("b", 0b1010)], 350);
}

#[test]
fn bug_53_nested_repeat_array_instance_matches_icarus() {
    // `insert_repeat_instance_output_kinds` only scanned `r.items` for a
    // direct `ModuleItem::Inst` — an instance one level deeper, inside a
    // NESTED `repeat`, was never keyed into `decls` at all, same missing-
    // recursion gap as `Concat as a NESTED operand, not the outer
    // container` elsewhere in this file, this time on the declaration
    // side rather than the render side.
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[9]\n  repeat i: 0..1 {\n    repeat j: 0..1 {\n      \
                let s[j] = Sub() { x: a }\n    }\n  }\n  \
                y = { b, s[0].q + a }\n}\n\n\
                module Sub {\n  in x: bits[4]\n  out q: bits[4]\n  q = x\n}\n";
    emitter_only_clocked_check(src, &[("a", 0b1111), ("b", 0b1010)], 350);
}

#[test]
fn bug_53_const_if_in_repeat_array_instance_matches_icarus() {
    // Same missing-recursion gap as the nested-`repeat` case above, this
    // time through a `const if` wrapping the instance instead of another
    // loop — `emit_instances` (real emission) already walks both shapes;
    // `insert_repeat_instance_output_kinds` (the `decls`-populating side)
    // did not.
    let src = "module Fuzz {\n  const DEBUG: int = 1\n  clock clk\n  reset rst\n  \
                in a: bits[4]\n  in b: bits[4]\n  out y: bits[9]\n  \
                repeat i: 0..1 {\n    const if (DEBUG) {\n      \
                let s[i] = Sub() { x: a }\n    }\n  }\n  \
                y = { b, s[0].q + a }\n}\n\n\
                module Sub {\n  in x: bits[4]\n  out q: bits[4]\n  q = x\n}\n";
    emitter_only_clocked_check(src, &[("a", 0b1111), ("b", 0b1010)], 350);
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

#[test]
fn shape_concat_operand_of_extend_in_a_concat_matches_icarus() {
    // Task 4 (round-4 plan, `docs/plan/v0.2-class-closure-round4.local.md`):
    // `bug_36_trunc_of_a_concat_hoists_the_base_first` — the coverage doc's
    // prior citation for `ExprKind::Concat`'s classifier arm — actually
    // exercises `Builtin::Trunc`'s OWN unconditional-`None` arm and its
    // separate `hoist_width_effect_operand` base-hoist, never
    // `verilog_self_determined_kind`'s `Concat => None` arm itself (that
    // arm is only reached via `self_determined_operand_width`'s recursion,
    // which `Trunc`'s arm never calls). This is the same "citation doesn't
    // exercise the claimed position" gap BUG-52 found for `IfExpr`/`Match`.
    // `extend(...)`'s argument DOES recurse via `self_determined_operand_
    // width`, so a concat as extend's argument is the position that
    // actually asks the `Concat` arm the question: does `{a,b}`'s own
    // self-determined width (8, the definite sum of its members — each
    // already hoisted independently, same reasoning `Min`/`Max` rely on)
    // equal mimz's own `Kind` for the same node? It does — Verilog
    // self-determines a concat's width as the exact sum of its members
    // regardless of context, so the mismatch this test proves is only
    // against `extend`'s wider target (12), which correctly still hoists.
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  in c: bits[4]\n  \
                out y: bits[16]\n  y = { c, extend({a, b}, 12) }\n}\n";
    // a=0b1111, b=0b0101, c=0b1010: {a,b}=0xF5 (8 bits), zero-extended to
    // 12 bits, prefixed with c -> 0xA0F5 (16 bits) = 41205.
    differential(src, &[("a", 0b1111), ("b", 0b0101), ("c", 0b1010)]);
}

#[test]
fn shape_replicate_operand_of_extend_in_a_concat_matches_icarus() {
    // Same fix as `shape_concat_operand_of_extend_in_a_concat_matches_icarus`,
    // for `ExprKind::Replicate`'s classifier arm — `shape_replicate_nested_
    // in_trunc_hoists_the_base` has the identical citation gap (exercises
    // `Trunc`'s own arm, not `Replicate`'s, since `Trunc` never recurses).
    let src = "module Fuzz {\n  in a: bits[3]\n  in c: bits[4]\n  \
                out y: bits[16]\n  y = { c, extend({2{a}}, 12) }\n}\n";
    // a=0b101: {2{a}}=0b101101 (0x2D, 6 bits), zero-extended to 12 bits,
    // prefixed with c=0b1010 -> 0xA02D (16 bits) = 41005.
    differential(src, &[("a", 0b101), ("c", 0b1010)]);
}

// ---------------------------------------------------------------------
// BUG-52 (`docs/audit/bugs.md`) — `verilog_self_determined_kind`
// (`self_determined.rs`), the CLASSIFIER half of the gate/classifier
// pair, ended `_ => None` over `ExprKind`. `kinds::infer_kind` (the
// GATE) became exhaustive in Task 3 of round 3's plan, but the
// classifier never did — every test above that places an `if`/`match`/
// unary shape inside a concat puts it as an operand of `+` first
// (`bug_41_if_expr_operand_of_add_in_concat_matches_icarus`,
// `shape_match_operand_of_add_in_concat_matches_icarus`), where the
// enclosing `+` triggers its OWN hoist and masks this entirely. These
// four place the shape as the concat/replication member ITSELF, the
// position the classifier is actually asked about.
// ---------------------------------------------------------------------

#[test]
fn bug_52_if_expr_as_a_concat_member_matches_icarus() {
    // `if s { extend(a,8) } else { extend(a,8) } }` renders each branch
    // as the bare `(a)` (4 bits), not mimz's grown 8 — byte-identical
    // wrong emission to BUG-28's Repro A, BUG-41's repro (5) and
    // BUG-48's repro 2, this time reached through a ternary instead of
    // a bare identifier/instance-port/fn-call.
    let src = "module Fuzz {\n  in s: bit\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[12]\n  \
                y = { b, if s { extend(a, 8) } else { extend(a, 8) } }\n}\n";
    differential(src, &[("s", 1), ("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_52_match_as_a_concat_member_matches_icarus() {
    // Same shape, through `match` instead of `if`/`else` — Verilog
    // renders a `match` as a chain of ternaries, same self-determined
    // rule as `IfExpr`.
    let src = "module Fuzz {\n  in s: bit\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[12]\n  \
                y = { b, (match s {\n    true => extend(a,8)\n    false => extend(a,8)\n  }) }\n}\n";
    differential(src, &[("s", 1), ("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_52_unary_not_of_an_extend_in_a_concat_matches_icarus() {
    // `~extend(a, 8)` renders as `(~(a))` — the bitwise-not of the bare
    // 4-bit `a`, not mimz's grown 8-bit value.
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[12]\n  y = { b, ~extend(a, 8) }\n}\n";
    differential(src, &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_52_if_expr_in_a_replication_body_matches_icarus() {
    // Same defect, self-determined position is a REPLICATION body
    // instead of a concat member — the classifier's `IfExpr` arm has to
    // cover both, since `verilog_self_determined_kind` is called from
    // every self-determined position, not just `Concat`.
    let src = "module Fuzz {\n  in s: bit\n  in a: bits[4]\n  \
                out y: bits[16]\n  \
                y = {2{ if s { extend(a,8) } else { extend(a,8) } }}\n}\n";
    differential(src, &[("s", 1), ("a", 0b1111)]);
}

#[test]
fn bug_55_signed_shift_right_inside_match_wildcard_arm_matches_icarus() {
    // Minimized from the deep-fuzz find (comb seed 12649355, index 925,
    // docs/audit/bugs.md BUG-55): a signed `>>` sitting in a `match`
    // arm, itself wrapped by `extend(...)` — a self-determined position
    // for the WHOLE match (BUG-52's own fix), but `p0 >> extend(14, 4)`
    // is BUG-47's exact defect one AST node deeper: without
    // `render_shift_ctx_operand` recursing into the match arm, Verilog
    // context-extends `p0` to the outer 16-bit assignment BEFORE
    // shifting, sign bits shift down, and Icarus disagrees with mimz's
    // own kernel (3 vs the correct 0 for this input). `p0 = 14429`
    // (signed[14], negative), `p3 = 2` (signed[4]) selects the `_` arm.
    let src = "module Fuzz {\n  in p0: signed[14]\n  in p3: signed[4]\n  \
                out y: signed[16]\n  \
                y = extend((match unsigned(p3) {\n    \
                0 => signed(extend(22, 14))\n    \
                1 => signed(extend(22, 14))\n    \
                _ => (p0 >> extend(14, 4))\n  }), 16)\n}\n";
    differential(src, &[("p0", 14429), ("p3", 2)]);
}

#[test]
fn bug_56_literal_nested_under_bitand_in_a_concat_matches_icarus() {
    // BUG-56's own filed repro (docs/audit/bugs.md): `a & 15` as a concat
    // member is checker-legal (the bare `15` adapts to `a`'s sibling type,
    // `Ty::Bits(4)`, so the member's own `Ty` is `Bits(4)`, not the
    // untyped `Ty::CtInt` the checker's E0405 rejects directly). But
    // `verilog_literal` used to render every literal token UNSIZED
    // (`'d15`/bare `15`), and real Icarus refuses to elaborate an
    // unsized-literal-bearing operand once it's nested inside `{...}` —
    // "Concatenation operand ... has indefinite width" — a hard
    // elaboration failure `differential`'s own Icarus-build-step assert
    // catches, the same class BUG-49 was.
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bits[8]\n  \
                y = { b, a & 15 }\n}\n";
    // a=0b1011 (11), b=0b0101 (5): 11 & 15 = 11. y = (5<<4)|11 = 91.
    differential(src, &[("a", 0b1011), ("b", 0b0101)]);
}

#[test]
fn bug_56_literal_nested_under_bitand_in_a_replication_body_matches_icarus() {
    // Same defect, one level further in — BUG-56's own filing pins this
    // shape too: `{2{ a & 15 }}` hits the identical "indefinite width"
    // Icarus error inside a replication body, not just a plain concat.
    let src = "module Fuzz {\n  in a: bits[4]\n  out y: bits[8]\n  \
                y = {2{ a & 15 }}\n}\n";
    // a=0b1010 (10): 10 & 15 = 10. y = (10<<4)|10 = 170.
    differential(src, &[("a", 0b1010)]);
}

#[test]
fn bug_58_negating_the_signed_minimum_matches_icarus() {
    // BUG-58's own filed repro (docs/audit/bugs.md): `-a` for `a:
    // signed[8]` is checker-typed `signed[9]` (`unary_ty`'s own lossless
    // `Signed(N+1)` rule, same "room for the MIN-value carry bit" `abs`
    // already gets) — the checker REQUIRES the wider destination, but the
    // kernel's `UnOp::Neg` used to keep `v.width` unchanged, so negating
    // `a`'s own minimum (`-128`) wrapped right back to `-128` instead of
    // the mathematically correct `128`. This is a CONTEXT-DETERMINED
    // position (a bare top-level assignment, not a concat member), so
    // real Icarus gets it right for free — `a` sign-extends to the 9-bit
    // destination BEFORE negating — and only the kernel disagreed with
    // its own type system.
    let src = "module Fuzz {\n  in a: signed[8]\n  out z: signed[9]\n  \
                z = -a\n}\n";
    // a = -128 (0x80 as an 8-bit two's-complement pattern): -(-128) = 128,
    // representable losslessly in signed[9] (range -256..255).
    differential(src, &[("a", 0x80)]);
}

/// GAP-13's own axis — exhaustive over `ExprKind`, no wildcard. Never
/// called (a compile-time-only property): its only job is that adding an
/// `ExprKind` variant without a line here fails the build, the same
/// enforcement the now-deleted `matrix_shape`/`ALL_BUILTINS` gave the
/// `Builtin` axis (Task 4 of `docs/plan/v0.2-correctness-remediation.
/// local.md` deleted those on the correct reasoning that the REAL
/// matches (`self_determined.rs`, `kinds::infer_call`) are themselves
/// wildcard-free over `Builtin`).
///
/// As of round 4 (BUG-52, docs/audit/bugs.md), BOTH real `ExprKind`
/// matches — `kinds::infer_kind` (Task 3, round 3) and
/// `self_determined::verilog_self_determined_kind` (Task 2, round 4) —
/// are themselves wildcard-free too, so in principle this axis could go
/// the way `matrix_shape`/`ALL_BUILTINS` did. It stays for now because
/// exhaustiveness alone does not prove an EXISTING arm's reasoning is
/// correct (Part 4 of `review-2026-08-10.md`: BUG-42/48/49/50/52 were all
/// an arm that was present but wrong, not missing) — this axis is where
/// each arm's own differential proof (or structural `NotApplicable`
/// reason) lives, one test per shape, independently of whether the
/// compiled match happens to be exhaustive today. Each arm below names
/// either the test(s) that differentially prove the shape hoists (or
/// correctly doesn't) in a self-determined position, or the reason it
/// structurally cannot appear there at all — and, per round 4's own
/// finding, that citation must name a test that actually exercises the
/// CLAIMED shape/position, not just any test with a superficially similar
/// name (`IfExpr`/`Match`/`Unary` below were wrong on exactly this point
/// through round 4).
#[allow(dead_code)]
fn expr_kind_self_determined_coverage(kind: &ExprKind) -> &'static str {
    match kind {
        ExprKind::Int { .. } => {
            "PARTIAL, BUG-56 (docs/audit/bugs.md, OPEN): true only when the \
             literal is the DIRECT concat/replicate member — the checker's \
             own concat-typing rule (`checker/widths/ops/mod.rs:697`, \
             E0405) rejects that shape pre-emit, so `self_determined.rs`'s \
             `Int => None` never actually has to answer for it. FALSE when \
             a literal is nested one level under an adapt-to-sibling \
             operator (`a & 15` as the concat member): `verilog_literal` \
             (`emit_verilog/mod.rs`) always renders an UNSIZED token \
             (`'b1111`/bare `15`), and real Icarus refuses to elaborate an \
             unsized-literal-bearing operand inside `{...}` — 'Concatenation \
             operand ... has indefinite width'. Confirmed against real \
             iverilog for both a concat member and a replication body; not \
             yet fixed or given its own differential (BUG-56's own fix task)."
        }
        ExprKind::Bool(_) => {
            "NotApplicable, verified — unlike `Int`, `expr.rs`'s own `Bool` \
             arm always renders a SIZED 1-bit literal (`1'b1`/`1'b0`, \
             `emit_verilog/expr.rs`'s `ExprKind::Bool` case), so BUG-56's \
             unsized-literal defect does not apply here; `Bool => None` \
             stands"
        }
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
            "covered by bug_52_unary_not_of_an_extend_in_a_concat_matches_icarus \
             (BUG-52, docs/audit/bugs.md) — a non-reduction unary op's \
             operand genuinely CAN differ: `~extend(a, 8)` renders as \
             `(~(a))`, self-determined at `a`'s bare 4 bits, not mimz's \
             grown 8. This arm previously read `NotApplicable`, citing \
             `self_determined.rs`'s own catch-all as confirmation — a \
             catch-all confirms nothing, it is the absence of an answer, \
             and that stale claim is exactly what let BUG-52 ship. The \
             three reductions (RedAnd/RedOr/RedXor) ARE always exactly 1 \
             bit in both mimz and Verilog regardless of operand, so no \
             mismatch is possible there and no separate test is needed \
             for that half."
        }
        ExprKind::Binary { .. } => {
            "covered by bug_19_lossless_sub_in_a_concat_matches_icarus and \
             the wider bug_19/23/24/30/35/42/47 family"
        }
        ExprKind::IfExpr { .. } => {
            "covered by bug_52_if_expr_as_a_concat_member_matches_icarus and \
             bug_52_if_expr_in_a_replication_body_matches_icarus — NOT \
             bug_41_if_expr_operand_of_add_in_concat_matches_icarus, this \
             arm's citation through round 4 (BUG-52, docs/audit/bugs.md): \
             that test places the `if` as an operand of `+` inside a \
             concat, a position where the enclosing `+`'s own hoist masks \
             this exact defect — it exercises `Binary`'s coverage above, \
             not `IfExpr`'s own, despite the name."
        }
        ExprKind::Match { .. } => {
            "covered by bug_52_match_as_a_concat_member_matches_icarus — \
             NOT shape_match_operand_of_add_in_concat_matches_icarus, this \
             arm's citation through round 4 — same wrong-position problem \
             as `IfExpr` immediately above (BUG-52, docs/audit/bugs.md)"
        }
        ExprKind::Concat(_) => {
            "covered by shape_concat_operand_of_extend_in_a_concat_matches_icarus \
             (Concat as a NESTED self-determined operand — recurses through \
             `self_determined_operand_width`, the position this arm's `None` \
             actually claims) — NOT bug_36_trunc_of_a_concat_hoists_the_base_first, \
             this arm's citation before round-4 Task 4's audit: that test \
             exercises `Builtin::Trunc`'s own unconditional-`None` arm and its \
             separate base-hoist mechanism, which never calls \
             `self_determined_operand_width` on the Concat at all, so it never \
             actually asked THIS arm anything (same wrong-position class BUG-52 \
             found for `IfExpr`/`Match`)"
        }
        ExprKind::Replicate { .. } => {
            "covered by bug_28_extend_in_replication_matches_icarus (as the \
             outer self-determined container) and \
             shape_replicate_operand_of_extend_in_a_concat_matches_icarus (as a \
             NESTED operand, recursing through `self_determined_operand_width`) \
             — NOT shape_replicate_nested_in_trunc_hoists_the_base, this arm's \
             citation before round-4 Task 4's audit for the nested case: same \
             wrong-position gap as Concat's arm immediately above (Trunc's own \
             arm never recurses into its base via `self_determined_operand_width`)"
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
             this axis. Round-4 Task 4: the RECURSING arms specifically \
             (`SignedCast`/`UnsignedCast`/`Encoding`, which forward to their \
             own argument's classification rather than answering \
             unconditionally) had only ever been proven a no-op over an \
             already-matching operand — matrix_signed_unsigned_cast_\
             recursion_catches_a_mismatched_operand_matches_icarus now also \
             proves the recursion CATCHES a real one (an `extend` nested \
             three levels under both casts), the same class BUG-42 shipped \
             from for `min`/`max`. `Encoding`'s own recursion is the \
             identical mechanism, closed WITHOUT a differential (structural \
             reasoning, checked, not assumed): its argument is always \
             enum-typed, and no enum-typed sub-expression can itself render \
             narrower than its own fixed width the way `extend(a,8)` does \
             for a `bits` value — `Ident`/`Field` render at a declared \
             wire's own width, `EnumConstruct` explicitly pads every field \
             with a SIZED literal to the enum's full tag+payload width \
             (`emit_verilog/expr.rs`, ~line 1098 — the same padding \
             discipline BUG-56's own fix generalizes), and an `IfExpr`/ \
             `Match` over the same enum `Ty` is already `IfExpr`/`Match`'s \
             own arm's concern (BUG-52). No shape exists to build the \
             `extend`-style mismatch `SignedCast` needed a test for."
        }
        ExprKind::FnCall { .. } => {
            "covered by bug_41_fn_call_operand_of_add_in_concat_matches_icarus \
             and bug_41_extend_of_a_fn_call_in_concat_matches_icarus"
        }
        ExprKind::BundleLit(_) => {
            "NotApplicable, verified — stronger than 'checker-rejected': \
             the `Type { field: val, ... }` literal syntax is PARSER- \
             restricted to a `Wire` init/`Drive` RHS (`ast`'s grammar), so \
             `ExprKind::BundleLit` cannot appear as a general sub- \
             expression at all — confirmed by 3 independent parse failures \
             (bare, parenthesized, and as a fn-call argument, all E1101) \
             during round-4 Task 4's audit. `expr.rs`'s own `BundleLit => \
             \"0\".into()` fallback for 'reached anyway' is accordingly \
             dead code for any parseable source, not merely checker-guarded."
        }
        ExprKind::ArrayLit(_) => {
            "NotApplicable for THIS axis, but the prior claim ('the checker \
             rejects it upstream') was false, not merely imprecise — found \
             during round-4 Task 4's audit and filed as BUG-56's sibling, \
             BUG-57 (docs/audit/bugs.md, OPEN): `mimz check` accepts \
             `[a,a,a][0]`, and `mimz compile` PANICS rendering it \
             (`unreachable!(\"Task 8 or Task 9 wires this up\")`) — at ANY \
             position, not specifically a self-determined one, which is why \
             this axis still reads NotApplicable rather than needing its \
             own differential here (there is nothing to hoist toward: \
             nothing renders, ever, self-determined position or not)."
        }
        ExprKind::EnumConstruct { .. } => {
            "NotApplicable: an enum-typed value in a concat is rejected \
             outright by the checker (E0403, docs/audit/bugs.md BUG-31) \
             before this code ever runs"
        }
    }
}

/// Round-4 plan Task 4's second coverage doc — the `Builtin` axis of the
/// CLASSIFIER (`self_determined::verilog_self_determined_kind`'s own `Call`
/// sub-match), exhaustive over `Builtin`, no wildcard. Same purpose and same
/// discipline as `expr_kind_self_determined_coverage` above: exhaustiveness
/// (already build-enforced independently, both here and in the real match)
/// proves a NEW builtin cannot ship unclassified — it says nothing about
/// whether an EXISTING arm's classification is actually correct, which is
/// what BUG-42 (`min`/`max`) shipped from and what this doc records the
/// proof of, one builtin at a time. A citation must exercise the position
/// the arm claims (BUG-52's own lesson) and, for a RECURSING arm
/// (`SignedCast`/`UnsignedCast`/`Encoding`), must additionally prove the
/// recursion CATCHES a real mismatch, not just that it's a no-op when
/// nothing is mismatched (BUG-42's own lesson, re-found for these three
/// arms during round-4 Task 4 — see `matrix_signed_unsigned_cast_
/// recursion_catches_a_mismatched_operand_matches_icarus`'s own doc comment).
#[allow(dead_code)]
fn builtin_self_determined_coverage(builtin: &ast::Builtin) -> &'static str {
    use ast::Builtin;
    match builtin {
        Builtin::Extend => {
            "covered by bug_28_extend_in_concat_matches_icarus (outer \
             container) and bug_28_extend_in_replication_matches_icarus \
             (replication body) — both place `extend(x, N)` directly as the \
             self-determined member, the position `Extend`'s arm's claim \
             (renders as bare `(x)`, self-determined at `x`'s own width) is \
             about"
        }
        Builtin::Abs => {
            "covered by bug_29_abs_in_concat_matches_icarus — direct concat \
             member, the position `Abs`'s ternary-rendering claim is about"
        }
        Builtin::Min | Builtin::Max => {
            "covered by matrix_min_in_concat_matches_icarus/matrix_max_in_\
             concat_matches_icarus (direct concat member, no mismatch — the \
             `None`-shaped case) AND bug_42_min_max_mismatched_operand_\
             matches_icarus (an `extend`-wrapped operand — the RECURSION \
             actually catching a mismatch, BUG-42's own fix and the \
             differential that proves it, not just the classification)"
        }
        Builtin::Trunc => {
            "covered by matrix_trunc_in_concat_matches_icarus — direct \
             concat member; `trunc` renders an explicit `x[N-1:0]` \
             part-select, already exactly N bits regardless of position, so \
             `None` needs no recursion proof the way `Min`/`Max` did"
        }
        Builtin::Nand | Builtin::Nor | Builtin::Xnor => {
            "covered by matrix_nand_in_concat_matches_icarus/matrix_nor_.../\
             matrix_xnor_... — direct concat member; a reduction is exactly \
             1 bit in both models regardless of operand width, so `None` \
             needs no recursion proof either"
        }
        Builtin::SignedCast | Builtin::UnsignedCast => {
            "covered by matrix_signed_unsigned_cast_roundtrip_in_concat_\
             matches_icarus (recursion is a correct no-op over an \
             already-matching plain-`Ident` operand) AND, as of round-4 \
             Task 4, matrix_signed_unsigned_cast_recursion_catches_a_\
             mismatched_operand_matches_icarus (an `extend` nested three \
             levels under both casts — the recursion actually CATCHES a \
             real mismatch, the proof the first test alone never gave, the \
             same gap class BUG-42 shipped from for `min`/`max`)"
        }
        Builtin::Encoding => {
            "covered by matrix_encoding_of_tag_only_enum_in_concat_matches_\
             icarus/matrix_encoding_of_payload_enum_in_concat_matches_icarus \
             (direct concat member) for the `None`-shaped no-mismatch case. \
             The recursion-catches-a-mismatch proof `SignedCast`/`UnsignedCast` \
             needed is NotApplicable here WITHOUT a differential — checked, \
             not assumed: `Encoding`'s argument is always enum-typed, and no \
             enum-typed sub-expression can itself render narrower than its \
             own fixed width the way `extend(a,8)` does for a `bits` value \
             (`Ident`/`Field` render at a declared wire's width; \
             `EnumConstruct` pads every field with a SIZED literal to the \
             enum's full fixed width, `emit_verilog/expr.rs` ~line 1098; an \
             `IfExpr`/`Match` branch is that arm's own concern, BUG-52) — \
             there is no shape here to build the `extend`-style mismatch \
             `SignedCast` needed a test for"
        }
        Builtin::Clog2 => {
            "NotApplicable, verified as a fact (round-4 Task 4): the \
             checker rejects `clog2` in any VALUE position with E0407 \
             (`checker/widths/ops/builtins.rs`, \"clog2 is a compile-time \
             built-in and has no value here\") — the only emit path that \
             ever renders a runtime `clog2(...)` call (a module-parameter \
             argument, `emit_verilog/expr.rs`) is width-specifier \
             evaluation (`bits[clog2(DEPTH)]`), never `expr()`/`expr_subst`, \
             the function every self-determined call site actually routes \
             through — so this arm can never be reached from a value \
             position `verilog_self_determined_kind` would ever be asked \
             about"
        }
        Builtin::SyncDoubleFlop | Builtin::SyncPulse => {
            "NotApplicable, verified as a fact (round-4 Task 4): grammar- \
             restricted to the direct target of an `On` drive \
             (`sync.double_flop`) or a `Wire` init (`sync.pulse`), and \
             lowered by `ast::sync_prim_lower::expand_sync_prims` before the \
             module body ever reaches `expr()` — backed by a load-bearing \
             `unreachable!()` in `expr.rs` if that pre-pass ever stops \
             covering every call site, so this is not merely documented but \
             actively enforced at runtime"
        }
    }
}

/// Round-4 plan Task 4's third coverage doc — the GATE half of the pair
/// (`kinds::infer_kind`'s own `ExprKind` match), exhaustive, no wildcard.
/// Different risk profile than the CLASSIFIER's two docs above, and this
/// doc says so per arm rather than assuming it: a wrong CLASSIFIER arm can
/// silently skip a needed hoist (BUG-52's whole class) because the failure
/// only shows up in the one narrow AST shape a self-determined-position
/// mismatch requires. A wrong GATE arm has a much shorter blast radius to
/// go unnoticed — `infer_kind`'s result sizes every hoisted wire directly
/// AND is what the classifier is compared against, so a wrong width here
/// tends to surface as a plain wrong VALUE in ANY differential/example/fuzz
/// test that exercises the shape at all, self-determined position or not —
/// which is why most arms below cite a direct unit test in `kinds.rs`
/// itself (the most precise evidence) rather than a position-matrix
/// differential; a few cite the self-determined suite where that's what
/// actually exercises the shape.
#[allow(dead_code)]
fn expr_kind_infer_kind_coverage(kind: &ExprKind) -> &'static str {
    match kind {
        ExprKind::Int { .. } => {
            "covered by kinds.rs's own literal_gets_its_minimal_width unit \
             test — direct, in-isolation proof of `min_width_for`'s value \
             on both a multi-bit and a single-bit (0) literal"
        }
        ExprKind::Bool(_) => {
            "NotApplicable for a differential: fixed `Kind{width:1, \
             signed:false}` unconditionally, no computation to get wrong; \
             exercised structurally by every test using a `bit` value \
             (e.g. bug_43_negative_literal_comparison_matches_icarus's own \
             `if`condition)"
        }
        ExprKind::Ident(_) => {
            "covered by kinds.rs's own ident_looks_up_declared_kind (found) \
             and ident_not_in_decls_is_none (absent -> None, the module- \
             parameter case `adapts_to_sibling` relies on) unit tests"
        }
        ExprKind::Unary { .. } => {
            "Verified sound for THIS axis (round-4 Task 4, batch 6): this \
             arm ignores `op` and forwards `inner`'s own `Kind` unchanged, \
             which matches the CLASSIFIER's own `Unary` arm (Task 2) for \
             every non-reduction op — Verilog's unary `-`/`~` really is \
             self-determined at the operand's width, confirmed by hand \
             against real Icarus. NOT the same claim as 'harmless': chasing \
             why the approximation is safe here found it mirrors a real bug \
             one layer down — the KERNEL's `UnOp::Neg` never applies the \
             checker's own lossless `+1` growth outside a literal, filed as \
             BUG-58 (docs/audit/bugs.md, OPEN) — so this arm is correct \
             company for the wrong reason on `Neg` specifically; the GATE \
             and the buggy kernel agree, not the GATE and the checker's own \
             type rule"
        }
        ExprKind::Binary { .. } => {
            "delegates to infer_binary, covered arm-by-arm below (this \
             function's own `BinOp` sub-match, mirroring how `ExprKind::\
             Call` delegates to `infer_call`/`builtin_infer_call_coverage`)"
        }
        ExprKind::Concat(_) => {
            "covered by kinds.rs's own concat_sums_part_widths (values sum \
             correctly) and concat_with_an_unresolvable_part_is_none (one \
             unresolvable part poisons the whole sum to None, rather than \
             silently treating it as zero-width) unit tests"
        }
        ExprKind::Replicate { .. } => {
            "covered by the differential suite, not a kinds.rs unit test: \
             BUG-50 (docs/audit/bugs.md) was exactly this arm returning a \
             too-narrow width (the per-iteration width instead of the \
             total), found building GAP-13's axis and fixed with \
             shape_replicate_nested_in_trunc_hoists_the_base and \
             bug_28_extend_in_replication_matches_icarus both depending on \
             the corrected total"
        }
        ExprKind::Slice { .. } => {
            "covered by bug_20_slice_of_a_composite_expression_matches_icarus \
             (literal bounds) and bug_48_const_bounded_slice_operand_of_add_\
             in_concat_matches_icarus (BUG-48: a `const`-valued bound must \
             fold through `slice_bound_fold`, not just a bare `Int` literal, \
             same class as BUG-56 for the opposite direction — a value that \
             folds in the EMITTED text but wasn't recognized as constant \
             here)"
        }
        ExprKind::Call { .. } => {
            "delegates to infer_call, covered arm-by-arm in \
             builtin_infer_call_coverage below"
        }
        ExprKind::Index { .. } => {
            "covered by kinds.rs's own index_on_a_plain_vector_is_one_bit, \
             index_on_a_memory_yields_the_element_kind (BUG-41: a mem read \
             must NOT collapse to 1 bit like an ordinary bit-select — the \
             two cases this arm must tell apart), and \
             index_on_an_unknown_name_is_none unit tests"
        }
        ExprKind::FnCall { .. } => {
            "covered by kinds.rs's own \
             fn_call_resolves_from_the_reserved_return_kind_key unit test, \
             plus bug_41_fn_call_operand_of_add_in_concat_matches_icarus \
             end-to-end"
        }
        ExprKind::Field { .. } => {
            "covered by kinds.rs's own \
             field_on_an_instance_resolves_from_the_mangled_port_key unit \
             test (plain-instance case) plus \
             bug_48_array_instance_port_operand_of_add_in_concat_matches_icarus \
             end-to-end (array-instance case — BUG-48's own unclassified \
             shape, `decls` keyed identically to `expr.rs`'s own rendering \
             as of BUG-53's fix)"
        }
        ExprKind::IfExpr { .. } => {
            "covered by kinds.rs's own if_expr_resolves_from_either_branch \
             and if_expr_is_none_when_neither_branch_resolves unit tests. \
             The 'first resolvable branch wins' shortcut \
             (`.or_else`) is sound only because the checker already \
             guarantees both branches the SAME `Ty` — not independently \
             re-verified this round (would need a case where two \
             differently-SHAPED expressions of the same checker `Ty` \
             produce different `infer_kind` results, which would be a \
             checker/emitter width-model disagreement bigger than this \
             axis, GAP-1's own territory)"
        }
        ExprKind::Match { .. } => {
            "NotApplicable for a dedicated unit test (no direct kinds.rs \
             test exists for this exact arm, unlike IfExpr's), but the same \
             `.find_map`-over-first-resolvable-arm shortcut as `IfExpr`, \
             same soundness argument, and exercised end-to-end by \
             shape_match_operand_of_add_in_concat_matches_icarus and \
             bug_52_match_as_a_concat_member_matches_icarus"
        }
        ExprKind::BundleLit(_) => {
            "NotApplicable, verified (round-4 Task 4, same finding as the \
             CLASSIFIER's identical arm above): the `Type { .. }` literal \
             syntax is PARSER-restricted to a `Wire` init/`Drive` RHS, so \
             `ExprKind::BundleLit` cannot appear as a general \
             sub-expression `infer_kind` would ever be called on"
        }
        ExprKind::ArrayLit(_) => {
            "`None` is correct regardless of BUG-57 (docs/audit/bugs.md): \
             this GATE arm safely declines to resolve a bare array literal \
             (`decls` has nothing to look an array literal's OWN `Kind` up \
             under) — BUG-57 is an EMITTER panic rendering `Index` on an \
             array literal, a different function entirely, not reachable \
             through this arm's own `None`"
        }
        ExprKind::EnumConstruct { .. } => {
            "NotApplicable: checker-rejected upstream in a `bits`-typed \
             self-determined position (E0403, BUG-31) the same as the \
             CLASSIFIER's identical arm; where it IS legal (a `reg`/`wire` \
             init of the enum's own type), this GATE is never consulted — \
             `build_decls` sizes the DECLARATION directly from the enum's \
             own `inferred_total_width`, not through `infer_kind`"
        }
    }
}

/// Round-4 plan Task 4's fourth coverage doc — `infer_binary`'s own
/// `BinOp` sub-match (`kinds.rs`), the GATE's other half of `ExprKind::
/// Binary`'s delegation. Exhaustive over `BinOp` (20 variants), no
/// wildcard — matching every other axis's own convention.
#[allow(dead_code)]
fn binop_infer_kind_coverage(op: &ast::BinOp) -> &'static str {
    use ast::BinOp;
    match op {
        BinOp::Shl | BinOp::Shr => {
            "covered end-to-end by the bug_24/bug_30/bug_34/bug_35 shift \
             family (docs/audit/bugs.md) — delegates to `width_rules::\
             shift_result`, this file's own module doc states it is a \
             second CALL SITE into shared rules, not a second \
             implementation, so a divergence here would be a shared-rule \
             bug visible everywhere `shift_result` is used, not just here"
        }
        BinOp::Add | BinOp::Sub => {
            "covered by kinds.rs's own lossless_add_grows_by_one_bit unit \
             test (`Sub` shares the same `adapted_lossless_operands` call \
             and `lossless_result` rule, not independently re-tested here) \
             plus bug_19_lossless_sub_in_a_concat_matches_icarus end-to-end"
        }
        BinOp::Mul => {
            "covered by kinds.rs's own \
             lossless_mul_with_a_module_parameter_adapts_to_the_sized_operand \
             unit test (BUG-46's own regression, a module `int` parameter \
             operand adapting to its sized sibling) PLUS the ordinary \
             (non-adapting) case, `p1 * p1` -> signed[28] inside \
             bug_24_shl_under_sibling_add_matches_icarus's own width chain — \
             not originally cited here (round-4 Task 4, batch 7): that test \
             predates this coverage doc and was written for BUG-24's shift \
             concern, but its own comment already states and verifies the \
             `Mul` width the GATE must agree with Icarus on"
        }
        BinOp::AddWrap
        | BinOp::SubWrap
        | BinOp::MulWrap
        | BinOp::BitAnd
        | BinOp::BitOr
        | BinOp::BitXor => {
            "covered by kinds.rs's own \
             wrap_add_with_a_narrower_bare_literal_adapts_to_the_sized_operand \
             unit test (literal on either side) — the other five in this \
             group share the identical `adapts_to_sibling`/`matched_result` \
             call, not independently re-tested here; end-to-end via \
             bug_19_wrapping_sub_in_a_bitand_matches_icarus (BitAnd) and \
             bug_23's own wrap family"
        }
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            "NotApplicable for a differential: fixed `Kind{width:1, \
             signed:false}` unconditionally regardless of either operand — \
             this function's own doc comment notes the retired \
             `kind_is_inferrable` used to eagerly require both operands \
             resolvable here even though neither is ever consulted; \
             exercised structurally by every comparison in the suite \
             (e.g. bug_43_negative_literal_comparison_matches_icarus)"
        }
        BinOp::LogicAnd | BinOp::LogicOr => {
            "NotApplicable for a differential: same fixed `Kind{width:1, \
             signed:false}` as the comparison family, same reasoning"
        }
        BinOp::Coalesce => {
            "NotApplicable, verified (round-4 Task 4): `??`'s result is \
             `checker::widths::ops::mod::coalesce_ty`'s own bundle-shaped \
             `Ty` (unwrap or OR-mux over a valid-bundle, per the \
             valid-bundle-sugar design spec), never `bits`-typed — the \
             checker forbids a non-`bits` value in any of the five \
             self-determined positions the same way it does `BundleLit`, \
             so this arm's own `None` (nothing to resolve, by design) can \
             never actually be asked for in a position this axis cares \
             about"
        }
    }
}

/// Round-4 plan Task 4's fifth and last coverage doc — `infer_call`'s own
/// `Builtin` match (`kinds.rs`), the GATE's `Builtin` axis. Exhaustive,
/// no wildcard. Same lower-risk profile as `expr_kind_infer_kind_coverage`
/// above (a wrong GATE width tends to surface as a plain wrong value in
/// any test exercising the shape, not just a self-determined-position
/// one) — only `Encoding` has a dedicated `kinds.rs` unit test; every
/// other arm's citation is the differential that exercises it end-to-end,
/// which proves the GATE and the emitted VALUE agree, even without
/// isolating `infer_call` the way a unit test would.
#[allow(dead_code)]
fn builtin_infer_call_coverage(builtin: &ast::Builtin) -> &'static str {
    use ast::Builtin;
    match builtin {
        Builtin::Extend | Builtin::Trunc => {
            "covered end-to-end by bug_28_extend_in_concat_matches_icarus/\
             bug_28_extend_in_replication_matches_icarus (`Extend`) and \
             matrix_trunc_in_concat_matches_icarus plus the bug_36/bug_44/\
             bug_46/bug_49 `Trunc` family (docs/audit/bugs.md) — both share \
             this one arm (`width = const_fold(args[1])`, `signed = \
             infer_kind(args[0]).signed`), so a divergence would break both \
             at once, not independently re-tested per-builtin here"
        }
        Builtin::SignedCast | Builtin::UnsignedCast => {
            "covered end-to-end by \
             matrix_signed_unsigned_cast_roundtrip_in_concat_matches_icarus \
             and matrix_signed_unsigned_cast_recursion_catches_a_mismatched_\
             operand_matches_icarus — both exercise this arm's `width = \
             infer_kind(args[0]).width` alongside the CLASSIFIER's own \
             recursion, since a wrong GATE width here would size the hoisted \
             wire wrong even when the classifier correctly detects a \
             mismatch"
        }
        Builtin::Encoding => {
            "covered by kinds.rs's own \
             encoding_kind_matches_its_argument_width_unsigned unit test — \
             the one Builtin arm with a direct, in-isolation GATE test — \
             plus matrix_encoding_of_tag_only_enum_in_concat_matches_icarus/\
             matrix_encoding_of_payload_enum_in_concat_matches_icarus \
             end-to-end"
        }
        Builtin::Abs => {
            "covered by bug_29_abs_in_concat_matches_icarus — this arm's \
             own `width = infer_kind(args[0]).width + 1` (BUG-29's own \
             fix, matching the checker's identical `Ty::Signed(n+1)` rule, \
             `checker/widths/ops/builtins.rs`) is what makes the ternary \
             Verilog renders for `abs` mismatch mimz's grown width, which \
             is the defect this test pins"
        }
        Builtin::Nand | Builtin::Nor | Builtin::Xnor => {
            "covered by matrix_nand_in_concat_matches_icarus/matrix_nor_.../\
             matrix_xnor_... — fixed `Kind{width:1, signed:false}` \
             regardless of operand, BUG-35's own fix (these three were \
             entirely unclassified before it, `kind_is_inferrable`'s \
             matching gap making any expression wrapping one of them \
             untouchable by the hoist machinery)"
        }
        Builtin::Min | Builtin::Max => {
            "covered by matrix_min_in_concat_matches_icarus/matrix_max_in_\
             concat_matches_icarus (no-mismatch case) and \
             bug_42_min_max_mismatched_operand_matches_icarus (BUG-42's own \
             fix — this arm's `adapts_to_sibling`/`is_ct_int_like` handling \
             for a literal or negated-literal operand, matching the \
             checker's own `matched_ty` call)"
        }
        Builtin::Clog2 => {
            "NotApplicable, verified as a fact (round-4 Task 4, same \
             finding as the CLASSIFIER's identical arm above): `clog2` is \
             checker-rejected in any VALUE position (E0407); this GATE is \
             never even invoked on a `Builtin::Clog2` call, since `infer_\
             kind`/`infer_call` only ever run on module-body VALUE \
             expressions the checker has already accepted"
        }
        Builtin::SyncDoubleFlop | Builtin::SyncPulse => {
            "NotApplicable, verified as a fact (round-4 Task 4, same \
             finding as the CLASSIFIER's identical arm above): lowered by \
             `ast::sync_prim_lower::expand_sync_prims` before the module \
             body is ever walked for `Kind` inference at all"
        }
    }
}
