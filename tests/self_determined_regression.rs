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
use std::fs;
use std::path::PathBuf;

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
fn task5_comparison_operand_hoist_catches_a_mismatch_matches_icarus() {
    // Round-5 plan Task 5 (docs/plan/v0.2-class-closure-round5.local.md):
    // `self_determined.rs`'s `BinOp::Eq|Ne|Lt|Le|Gt|Ge => None` arm only
    // ever claimed the comparison's own RESULT is 1 bit both sides — true,
    // and not the same claim as "the operands need no test". The operands
    // are hoisted at a SEPARATE call site (`expr.rs`'s comparison arm,
    // both `lhs`/`rhs` individually) that had never been exercised by a
    // narrow-rendering operand here. `extend(a, 8)` renders as the bare
    // `(a)` (4 bits) — without the hoist, comparing it against `b`'s 8
    // bits would silently compare the wrong value.
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[8]\n  \
                out y: bit\n  y = (extend(a, 8) == b)\n}\n";
    // a=0b1111 (15), zero-extended to 8 bits = 15 = b: equal, y=1.
    differential(src, &[("a", 0b1111), ("b", 15)]);
}

#[test]
fn task7_comparison_rhs_operand_hoist_catches_a_mismatch_matches_icarus() {
    // Round-6 plan Task 7 (GAP-17, `docs/plan/v0.2-class-closure-round6.
    // local.md`): the comparison arm's `lhs`/`rhs` hoists are two SEPARATE
    // call sites in `expr.rs` (each with its own `hoist_if_needed` call),
    // and every existing differential here only ever put the narrow-
    // rendering operand on the LHS — `task5_comparison_operand_hoist_
    // catches_a_mismatch_matches_icarus`'s own `b` (RHS) is a bare
    // identifier, so it never exercised the RHS call site's OWN hoist,
    // only its (identical, by construction) doesn't-fire path. Swap sides:
    // `b` is now the bare-identifier LHS (doesn't-fire control) and
    // `extend(a, 8)` the RHS (fires).
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[8]\n  \
                out y: bit\n  y = (b == extend(a, 8))\n}\n";
    // Same vector as task5's test, sides swapped: still equal, y=1.
    differential(src, &[("a", 0b1111), ("b", 15)]);
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
fn bug_61_bit_select_of_an_extend_hoists_the_base_and_matches_icarus() {
    // docs/audit/bugs.md BUG-61: a bit-select's base is a Verilog grammar
    // constraint identical to BUG-20's slice base — `extend(a,8)[7]`
    // rendered `(a)[7]` before this fix, a syntax error under real
    // Verilog (`x[i]` only accepts a plain identifier). a=0b1111 (4 bits):
    // `extend(a,8)` is 0b00001111, bit 7 is 0.
    let src = "module Fuzz {\n  in a: bits[4]\n  out y: bit\n  y = extend(a, 8)[7]\n}\n";
    differential(src, &[("a", 0b1111)]);
}

#[test]
fn bug_61_bit_select_of_a_concat_hoists_the_base_and_matches_icarus() {
    // Same defect, a `Concat` base instead of a `Call`. a=0b1010 (4 bits),
    // b=0b0011 (4 bits): {a,b} = 0b10100011, bit 3 (0-indexed from the
    // LSB) is 0.
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bit\n  y = {a, b}[3]\n}\n";
    differential(src, &[("a", 0b1010), ("b", 0b0011)]);
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
fn task5_abs_operand_at_plain_top_level_matches_icarus() {
    // Round-5 plan Task 5: `Abs`'s render arm embeds its operand with no
    // hoist call, the same SHAPE BUG-60 needed a hoist for — checked
    // whether it's the same bug (it is not, an LRM distinction: a
    // ternary's branches are context-determined at plain top level,
    // unlike a reduction's operand, which is unconditionally
    // self-determined regardless of context). `a` is the widest-magnitude
    // signed[4] value so a wrong (unextended) computation would show.
    let src = "module Fuzz {\n  in a: signed[4]\n  out y: signed[9]\n  \
                y = abs(extend(a, 8))\n}\n";
    // a=-8 (min signed[4]): sign-extend to 8 bits = -8, abs = 8.
    differential(src, &[("a", 0b1000)]);
}

#[test]
fn task5_min_max_operand_at_plain_top_level_matches_icarus() {
    // Same check as `task5_abs_operand_at_plain_top_level_matches_icarus`,
    // for `Min`/`Max`'s identical ternary shape — the fresh-fuzz-found
    // BUG-42 repro (`bug_42_min_max_mismatched_operand_matches_icarus`)
    // places this INSIDE an `unsigned(...)` cast, not at a plain top-level
    // assignment; this is the position that citation doesn't cover.
    let src = "module Fuzz {\n  in p: signed[6]\n  out y: signed[11]\n  \
                y = min(extend(p, 11), extend(p, 11))\n}\n";
    // p=-9 (signed[6]): sign-extend to 11 bits = -9, min(-9,-9) = -9.
    differential(src, &[("p", 0b110111)]);
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
    //
    // Round-4 plan Task 8 (BUG-53's own check/sim/emit split): this used
    // `emitter_only_clocked_check` because `mimz sim` rejected the shape
    // outright — `elaborate/module.rs`'s repeat-body loop keyed the
    // array-instance's OWN NAME from the raw loop counter too, the exact
    // same defect one layer down. Now that the simulator folds `inst.index`
    // the same way, this runs through the REAL kernel via
    // `differential_clocked`, not just the emitted text.
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[9]\n  repeat i: 0..1 {\n    let s[i + 1] = Sub() { x: a }\n  }\n  \
                y = { b, s[1].q + a }\n}\n\n\
                module Sub {\n  in x: bits[4]\n  out q: bits[4]\n  q = x\n}\n";
    differential_clocked(src, Some("Fuzz"), &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_53_nested_repeat_array_instance_matches_icarus() {
    // `insert_repeat_instance_output_kinds` only scanned `r.items` for a
    // direct `ModuleItem::Inst` — an instance one level deeper, inside a
    // NESTED `repeat`, was never keyed into `decls` at all, same missing-
    // recursion gap as `Concat as a NESTED operand, not the outer
    // container` elsewhere in this file, this time on the declaration
    // side rather than the render side.
    //
    // Round-4 plan Task 8: `mimz sim` used to reject nested `repeat`
    // outright (S0125) even though the checker's own `no_decls_in_repeat`
    // (E0303) already treats it as legal. The simulator's repeat-body walk
    // now recurses into a nested `Repeat` the same way the outer worklist
    // already did, so this runs through the real kernel.
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[9]\n  repeat i: 0..1 {\n    repeat j: 0..1 {\n      \
                let s[j] = Sub() { x: a }\n    }\n  }\n  \
                y = { b, s[0].q + a }\n}\n\n\
                module Sub {\n  in x: bits[4]\n  out q: bits[4]\n  q = x\n}\n";
    differential_clocked(src, Some("Fuzz"), &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_53_const_if_in_repeat_array_instance_matches_icarus() {
    // Same missing-recursion gap as the nested-`repeat` case above, this
    // time through a `const if` wrapping the instance instead of another
    // loop — `emit_instances` (real emission) already walks both shapes;
    // `insert_repeat_instance_output_kinds` (the `decls`-populating side)
    // did not.
    //
    // Round-4 plan Task 8: `mimz sim` used to reject this shape too (S0126,
    // the repeat-body loop's catch-all) even though the checker already
    // treats `const if` inside `repeat` as legal. Fixing the repeat-body
    // walk alone surfaced a SECOND, sibling gap: `collect_inst_names`
    // recursed into a nested `Repeat` but not a `ConstIf`, so `s` was never
    // registered as a known instance name and the READ side (`s[0].q`)
    // failed with an unrelated "instance-port access is not supported"
    // error — fixed alongside, same recursion added there too.
    let src = "module Fuzz {\n  const DEBUG: int = 1\n  clock clk\n  reset rst\n  \
                in a: bits[4]\n  in b: bits[4]\n  out y: bits[9]\n  \
                repeat i: 0..1 {\n    const if (DEBUG) {\n      \
                let s[i] = Sub() { x: a }\n    }\n  }\n  \
                y = { b, s[0].q + a }\n}\n\n\
                module Sub {\n  in x: bits[4]\n  out q: bits[4]\n  q = x\n}\n";
    differential_clocked(src, Some("Fuzz"), &[("a", 0b1111), ("b", 0b1010)]);
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
fn bug_60_and_reduction_of_an_extend_in_a_concat_matches_icarus() {
    // docs/audit/bugs.md BUG-60: a reduction's OPERAND is a self-determined
    // position with no hoist site before this fix — `&extend(a, 8)` renders
    // as `(&(a))`, and-reducing the bare 4-bit `a` instead of `a` zero-
    // extended to 8. a=0b1111 (all ones — the divergence-triggering value),
    // b=0b1010: and-reduce(zext(a,8)) = 1 (all 8 bits set), y = (10<<1)|1 =
    // 21, not the pre-fix 20.
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[5]\n  y = { b, &extend(a, 8) }\n}\n";
    differential(src, &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_60_and_reduction_of_an_extend_in_a_replication_matches_icarus() {
    // Same defect, self-determined position is a REPLICATION body instead
    // of a concat member. a=0b1111: and-reduce(zext(a,8)) = 1, replicated
    // 3 times = 0b111 = 7, not the pre-fix 0.
    let src = "module Fuzz {\n  in a: bits[4]\n  \
                out y: bits[3]\n  y = {3{ &extend(a, 8) }}\n}\n";
    differential(src, &[("a", 0b1111)]);
}

#[test]
fn bug_60_and_reduction_of_an_extend_at_top_level_matches_icarus() {
    // BUG-60's sharpest repro: a PLAIN top-level assignment, no enclosing
    // concat/replication at all — the mismatch is entirely inside the
    // reduction's own operand. a=0b1111: and-reduce(zext(a,8)) = 1, not
    // the pre-fix 0.
    let src = "module Fuzz {\n  in a: bits[4]\n  out y: bit\n  y = &extend(a, 8)\n}\n";
    differential(src, &[("a", 0b1111)]);
}

#[test]
fn bug_60_and_reduction_of_a_bare_identifier_stays_unhoisted() {
    // Control: an and-reduction of an ALREADY-matching-width operand (a
    // bare identifier, not an `extend`) must NOT spuriously hoist — same
    // shape as `matrix_nand_in_concat_matches_icarus`'s existing bare-`a`
    // case, pinned here directly on the reduction rather than `nand`.
    let src = "module Fuzz {\n  in a: bits[4]\n  out y: bit\n  y = &a\n}\n";
    differential(src, &[("a", 0b1111)]);
}

#[test]
fn bug_60_nand_of_an_extend_matches_icarus() {
    // BUG-60's `Builtin::Nand` half — `nand` is the negated and-reduction,
    // so it diverges the same way `&` does. a=0b1111: nand(zext(a,8)) = 0
    // (all 8 bits set, and-reduce 1, negated 0), not the pre-fix 1.
    let src = "module Fuzz {\n  in a: bits[4]\n  out y: bit\n  y = nand(extend(a, 8))\n}\n";
    differential(src, &[("a", 0b1111)]);
}

#[test]
fn bug_60_or_reduction_of_a_negated_extend_matches_icarus() {
    // BUG-60's `~` + or-reduction repro: zero-padding flips to one-padding
    // under `~`, which perturbs `|`/`^` reductions too, not just `&`/nand.
    // a=0b1111: zext(a,8)=0b00001111, ~zext=0b11110000, or-reduce=1 — the
    // correct (kernel) value. Pre-fix, `~extend(a,8)` rendered unhoisted as
    // `(~(a))` over the bare 4-bit `a`: ~0b1111=0b0000, or-reduce=0. Icarus
    // on the pre-fix emission gives 0, disagreeing with the kernel's 1.
    let src = "module Fuzz {\n  in a: bits[4]\n  out y: bit\n  y = |(~extend(a, 8))\n}\n";
    differential(src, &[("a", 0b1111)]);
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
fn bug_59_fused_shift_chain_inside_an_if_branch_as_the_lhs_of_a_growing_shift_matches_icarus() {
    // Minimized from GAP-14's own nightly-depth gate re-run (comb seed
    // 12650993, index unrecorded — found running the 5000/5000 gate this
    // plan's own Task 1 mandates, docs/audit/bugs.md BUG-59): distinct
    // from BUG-52 (a width mismatch) and BUG-55 (a signed `>>` escaping
    // its branch) — here mimz's `Kind` for the WHOLE `if` already agrees
    // with Verilog's (`verilog_self_determined_kind`/`infer_kind` both
    // say 11 bits), so neither prior fix's mismatch check fires. The
    // VALUE still differs: mimz-sim's `eval_shift_chain` resolves the
    // `then` branch's fused `(a << b) >> c` bottom-up, in isolation, with
    // no ambient context (BUG-34's own design) — giving 384. Rendered
    // inline as the OUTER `<<`'s own LHS (un-hoisted), real Verilog
    // instead threads the outer assignment's full 14-bit grown context
    // straight through the ternary into that same inner `>>`, which
    // truncates differently at 14 bits than at 11 — giving 3968 for the
    // branch, then `3968 << 3` overflows the 14-bit destination,
    // wrapping to -1024 where the kernel computes 384 << 3 = 3072
    // (confirmed against real Icarus by hand, `docs/audit/bugs.md`).
    let src = "module Fuzz {\n  in s: bit\n  out y: signed[14]\n  \
                y = (if s {\n    \
                (signed(extend(12, 4)) << extend(7, 3)) >> extend(2, 3)\n  \
                } else {\n    signed(extend(1029, 11))\n  }) << extend(3, 2)\n}\n";
    differential(src, &[("s", 1)]);
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

#[test]
fn task7_symbolic_extend_base_hoist_when_base_is_composite_matches_icarus() {
    // Round-6 plan Task 7 (GAP-17): `try_widen_symbolic_extend` (`expr.rs`)
    // hoists `extend(x, W)`'s own OPERAND `x` into a named wire before
    // splicing it into the explicit widen text
    // (`{{(W)-(N){fill}}, named}`) whenever `x`'s rendered text isn't
    // already a plain identifier — its own `hoist_slice_base_if_needed`
    // call, `expr.rs:172`. Every existing BUG-62(b)/Task-3 differential
    // only ever widened a BARE port (`extend(a, W)`), which never reaches
    // this hoist at all (a bare identifier is already safe to splice
    // as-is). Force it with a composite base instead — a slice.
    let src = "module Fuzz(W: int = 8) {\n  in a: bits[8]\n  in c: bits[4]\n  \
                out y: bits[12]\n  y = { c, extend(a[3:0], W) }\n}\n";
    // a = 0b1011_0101 (181); a[3:0] = 0b0101 (5), zero-extended from 4 to
    // W=8 bits (still 5), prefixed with c = 0b1010 (10) -> 12 bits =
    // 0b1010_00000101 = 2565.
    differential(src, &[("a", 0b1011_0101), ("c", 0b1010)]);
}

// ---------------------------------------------------------------------
// Round-6 plan Tasks 2/4 (BUG-62①②③, BUG-63, `docs/audit/bugs.md`) fixed
// the `fn`-body emitter context — `render_fn_decl` now builds a real
// `decls` map from the `fn`'s own params/`let`s instead of leaving
// `infer_kind` looking at the ENCLOSING MODULE's map (which never has an
// entry for a `fn` parameter's name), and gave `fn`-body hoists their own
// function-local `reg` buffer instead of a module-scope `wire`. Both
// fixes were verified against real `iverilog` while landing (round-6
// plan's own status notes, `docs/plan/v0.2-class-closure-round6.local.md`)
// but — unlike every OTHER bug in this file — never pinned as a permanent
// regression here. Round-6 plan Task 7 (GAP-17) needs a real `fn`-body
// differential to cite for this axis anyway, so these four close that gap:
// the exact repro shapes BUG-62①②③/BUG-63 filed, run through the same
// `differential`/two-judge machinery as everything else in this file.
// ---------------------------------------------------------------------

#[test]
fn bug_62_reduction_of_an_extend_inside_a_fn_body_matches_icarus() {
    // BUG-62①: `fn`'s own `x` was never in `decls` before round-6 Task 2,
    // so `infer_kind` returned `None` for it and the reduction's operand
    // hoist (`expr.rs`'s `ExprKind::Unary` arm) silently fell through,
    // rendering `&(x)` — an AND-reduce over `x`'s bare 4 bits — instead of
    // `x` zero-extended to 8 first.
    let src = "fn allset(x: bits[4]) -> bit {\n  &extend(x, 8)\n}\n\n\
               module Fuzz {\n  in x: bits[4]\n  out y: bit\n  y = allset(x)\n}\n";
    // x = 0b1111: and-reduce(zero_extend(x,8)) = 0 (the top 4 bits are 0).
    // Pre-fix, and-reduce(x) over the bare 4 bits = 1 — the divergence.
    differential(src, &[("x", 0b1111)]);
}

#[test]
fn bug_62_negated_reduction_of_an_extend_inside_a_fn_body_matches_icarus() {
    // BUG-62②: same defect, through `Builtin::Nand`'s own operand hoist.
    let src = "fn nandit(x: bits[4]) -> bit {\n  nand(extend(x, 8))\n}\n\n\
               module Fuzz {\n  in x: bits[4]\n  out y: bit\n  y = nandit(x)\n}\n";
    // x = 0b1111: nand(zero_extend(x,8)) = 1. Pre-fix, nand(x) over the
    // bare 4 bits = 0 — the divergence.
    differential(src, &[("x", 0b1111)]);
}

#[test]
fn bug_62_extend_in_a_concat_inside_a_fn_body_matches_icarus() {
    // BUG-62③ — BUG-28's own founding divergence, alive inside a `fn`
    // body: the concat-member hoist (`expr.rs`'s `ExprKind::Concat` arm)
    // needs `x`'s `Kind`, which `decls` never had before round-6 Task 2.
    let src = "fn packit(x: bits[4], b: bits[4]) -> bits[12] {\n  \
               { b, extend(x, 8) }\n}\n\n\
               module Fuzz {\n  in x: bits[4]\n  in b: bits[4]\n  \
               out y: bits[12]\n  y = packit(x, b)\n}\n";
    // x=0b1111, b=0b1010: {b, zero_extend(x,8)} = 0b1010_00001111 = 2575.
    // Pre-fix (bare `x`, only 8 bits crammed into a 12-bit return, zero-
    // padded by the assignment) gave 175 — BUG-28's own byte-identical
    // wrong value, reached through a `fn` instead of a bare top-level
    // concat member.
    differential(src, &[("x", 0b1111), ("b", 0b1010)]);
}

#[test]
fn task8_trunc_of_a_composite_base_inside_a_fn_body_matches_icarus() {
    // Round-6 review Part 3.3 (`docs/audit/review-2026-08-15.md`): the
    // CLASSIFIER coverage doc's `Trunc` arm claimed "already exactly N
    // bits regardless of position", cited only to a BARE-IDENTIFIER base
    // (`matrix_trunc_in_concat_matches_icarus`'s `trunc(a, 2)`) — the exact
    // discriminating gap `Nand`/`Nor`/`Xnor`'s own arm was corrected for
    // one screen above (BUG-60's signature). The claim is true only where
    // the RENDER call site's own base-hoist (`hoist_slice_base_if_needed`,
    // `HOIST_CALL_SITES["trunc base"]`) actually fires — `bug_36`, above,
    // already pins that for a composite base at MODULE scope; this pins
    // the same shape inside a `fn` body, the context round-6 Tasks 2/4
    // made that hoist route through a function-local `reg` instead of a
    // module-scope wire. Before those tasks landed, this exact source was
    // a checker-clean `mimz check`/`mimz compile` exit 0 that Icarus
    // rejected as a syntax error (`(x)[(2)-1:0]` is not a valid
    // part-select base) — round-6 review's own repro ⑥.
    let src = "fn low2(x: bits[4]) -> bits[2] {\n  trunc(extend(x, 8), 2)\n}\n\n\
               module Fuzz {\n  in x: bits[4]\n  out y: bits[2]\n  y = low2(x)\n}\n";
    // x = 0b1111: zero_extend(x,8) = 0b0000_1111, low 2 bits = 0b11 = 3.
    differential(src, &[("x", 0b1111)]);
}

#[test]
fn bug_63_fn_param_shadowing_a_module_signal_reads_the_argument_matches_icarus() {
    // BUG-63: once Task 2 gives `fn pack`'s own `a` parameter a real
    // `decls` entry, a hoist inside `pack`'s body genuinely fires (Task
    // 2's whole point) — but if it shared the MODULE's `hoisted_decls`
    // buffer, it would emit a module-scope wire computed over the
    // MODULE's OWN `a` signal (name collision with the fn param), not the
    // caller's argument, and declared AFTER the function that reads it
    // (`function automatic` can't forward-reference module scope). Task
    // 4's fix hoists into a function-LOCAL `reg` instead — this proves
    // both halves: it elaborates, and it reads the ARGUMENT (`c`), not
    // the module's own `a` (deliberately given a DIFFERENT value so the
    // two would diverge if the wrong one were read).
    let src = "fn pack(a: bits[4], b: bits[4]) -> bits[12] {\n  \
               { b, extend(a, 8) }\n}\n\n\
               module Fuzz {\n  in a: bits[4]\n  in c: bits[4]\n  in b: bits[4]\n  \
               out y: bits[12]\n  y = pack(c, b)\n}\n";
    // Module's own `a` = 0b0001 (decoy, must NOT be read); the argument
    // `c` = 0b1111 is what `pack`'s own `a` parameter is bound to.
    // {b, zero_extend(c,8)} = 0b1010_00001111 = 2575. Reading the
    // module's `a` instead would give 0b1010_00000001 = 2561.
    differential(src, &[("a", 0b0001), ("c", 0b1111), ("b", 0b1010)]);
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
///
/// Rule (a′) (GAP-15, `docs/audit/gaps.md`; round-5 plan Task 2): round
/// 4's own audit applied the position-not-name rule above to nearly every
/// arm and then, on `Unary`'s reduction half, slipped back to a claim
/// about the operator's RESULT ("always exactly 1 bit ... no separate
/// test is needed") — BUG-60. An arm's text below is only acceptable when
/// it is EITHER a citation to a differential whose operand renders
/// NARROWER than its mimz width (never a bare identifier), OR a
/// `NotApplicable` naming a checker rule / grammar restriction / lowering
/// pass by CODE, OR (round-6 plan Task 8, the fourth (a′-2) category) a
/// `NotApplicable` naming a checked fact about what the emitter renders,
/// which must itself name the call site performing any hoist it leans on
/// and the condition under which that hoist fires. "No mismatch is
/// possible" alone, unattached to any of the three, is not a reason — see
/// `Unary`'s and `Index`'s own entries below for what the corrected text
/// looks like.
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
             three reductions (RedAnd/RedOr/RedXor) really ARE exactly 1 \
             bit in both mimz and Verilog — but round-5 review found this \
             arm's own OLD text ('no mismatch is possible there and no \
             separate test is needed') answered the RESULT-width question, \
             which this axis's own module doc has forbidden since BUG-42: \
             the reduction's OPERAND is a separate self-determined position \
             with its own rendered width, and `&extend(a,8)` diverges there \
             — BUG-60 (docs/audit/bugs.md, fixed). This arm's `None` for \
             the reduction ops is still correct — the RESULT stays `None` \
             — but the fix hoists the OPERAND at the render call site \
             (`expr.rs`'s `ExprKind::Unary` arm, not here), proven by \
             bug_60_and_reduction_of_an_extend_in_a_concat_matches_icarus \
             (narrow-rendering operand, per rule (a′)) and \
             bug_60_and_reduction_of_a_bare_identifier_stays_unhoisted \
             (control: an already-matching-width operand)."
        }
        ExprKind::Binary { .. } => {
            "covered by bug_19_lossless_sub_in_a_concat_matches_icarus and \
             the wider bug_19/23/24/30/35/42/47 family for the general case. \
             The COMPARISON sub-case (`Eq|Ne|Lt|Le|Gt|Ge`, this arm's own \
             `None`) is a narrower claim — the result really is always 1 \
             bit — that round-5 Task 5 found untested for the position it \
             actually matters at: `expr.rs`'s comparison arm hoists EACH \
             operand independently at its own render call site (not through \
             this arm's own `None`), and nothing had ever proven that hoist \
             catches a real mismatch rather than merely existing. Checked \
             empirically (real `mimz compile` + hand-read emission) before \
             adding a test — `extend(a,8) == b` correctly hoists into \
             `wire [7:0] __mimz_sub_1; assign __mimz_sub_1 = (a); assign y \
             = (__mimz_sub_1 == b);` — then pinned as \
             task5_comparison_operand_hoist_catches_a_mismatch_matches_icarus"
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
             real element Kind). The plain bit-select branch's RESULT is \
             genuinely always exactly 1 bit in both mimz and Verilog, which \
             this arm's `None` correctly answers — but round-5 review found \
             this citation had the SAME gap BUG-60 found for `Unary`'s \
             reduction arm: the bit-select's BASE is its own self-determined \
             position, unexamined by both the code and this axis until \
             BUG-61 (docs/audit/bugs.md, fixed) — `extend(a,8)[7]` rendered \
             `(a)[7]`, a syntax error, because nothing hoisted the base the \
             way `ExprKind::Slice` already did. Fixed by calling the same \
             `hoist_slice_base_if_needed` `Slice` uses, from `Index`'s \
             bit-select render arm too; proven by \
             bug_61_bit_select_of_an_extend_hoists_the_base_and_matches_icarus \
             and bug_61_bit_select_of_a_concat_hoists_the_base_and_matches_icarus"
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
///
/// Rule (a′) (GAP-15, `docs/audit/gaps.md`; round-5 plan Task 2): the
/// `Nand`/`Nor`/`Xnor` entry below made the identical BUG-52 mistake this
/// doc's own header warns against — its citation
/// (`matrix_nand_in_concat_matches_icarus`) places a BARE IDENTIFIER as
/// the operand, which cannot discriminate a "regardless of operand
/// width" claim by construction, since a bare identifier's rendered
/// width IS its mimz width (BUG-60). A citation for a `None` arm here
/// must be a differential whose operand renders NARROWER than its mimz
/// width; a `NotApplicable` must name a checker/grammar/lowering fact by
/// code, never a property of the operator — OR (round-6 plan Task 8, the
/// fourth (a′-2) category, added after `Trunc`'s own arm below was found
/// resting on exactly this gap — round-6 review Part 3.3) a checked fact
/// about what the emitter renders, naming the call site that performs any
/// hoist the claim leans on and the condition under which it fires.
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
             member, the position `Abs`'s ternary-rendering claim is about \
             — AND (round-5 plan Task 5)
             task5_abs_operand_at_plain_top_level_matches_icarus, the \
             position `expr.rs`'s own render arm's missing hoist call \
             looks structurally identical to BUG-60's; checked and found \
             sound (an LRM distinction — ternary branches are \
             context-determined at plain top level, unlike a reduction's \
             unconditionally self-determined operand), not assumed"
        }
        Builtin::Min | Builtin::Max => {
            "covered by matrix_min_in_concat_matches_icarus/matrix_max_in_\
             concat_matches_icarus (direct concat member, no mismatch — the \
             `None`-shaped case) AND bug_42_min_max_mismatched_operand_\
             matches_icarus (an `extend`-wrapped operand — the RECURSION \
             actually catching a mismatch, BUG-42's own fix and the \
             differential that proves it, not just the classification) AND \
             (round-5 plan Task 5)
             task5_min_max_operand_at_plain_top_level_matches_icarus (the \
             plain-top-level position, same check and same reason it's \
             sound as `Abs` above)"
        }
        Builtin::Trunc => {
            "covered by matrix_trunc_in_concat_matches_icarus for the \
             bare-identifier (no-mismatch) case — but, per (a′-2)'s fourth \
             category (a checked fact about what the emitter renders, \
             naming the call site and its firing condition), that citation \
             ALONE does not establish 'already exactly N bits regardless \
             of position': round-6 review Part 3.3 found this arm's prior \
             wording did exactly that, cited to `trunc(a, 2)`, and it is \
             FALSE for a composite base — `trunc(extend(x,8), 2)` inside a \
             `fn` emitted `(x)[(2)-1:0]`, a syntax error, before round-6 \
             Tasks 2/4 fixed it. `trunc` DOES render an explicit \
             `x[N-1:0]` part-select at exactly N bits, but ONLY because \
             the RENDER call site (`expr.rs`'s `Builtin::Trunc` arm, \
             `hoist_slice_base_if_needed`, named `HOIST_CALL_SITES['trunc \
             base']` in this file) hoists a composite base to a named wire \
             first — a fact about that call site, not a property of the \
             operator. Now proven narrow-rendering in BOTH contexts a \
             `NotApplicable`-shaped citation must name: module scope \
             (bug_36_trunc_of_a_concat_hoists_the_base_first) and `fn` \
             scope (task8_trunc_of_a_composite_base_inside_a_fn_body_\
             matches_icarus)"
        }
        Builtin::Nand | Builtin::Nor | Builtin::Xnor => {
            "covered by matrix_nand_in_concat_matches_icarus/matrix_nor_.../\
             matrix_xnor_... for the bare-identifier (no-mismatch) case — \
             but that citation alone was BUG-60's own gap (docs/audit/\
             bugs.md, fixed): a bare identifier's rendered width IS its \
             mimz width by definition, so it cannot discriminate a claim \
             about 'regardless of operand width'. The RESULT is genuinely \
             1 bit in both models regardless of operand width, so `None` \
             is still correct here — but the OPERAND is its own \
             self-determined position, hoisted at the render call site \
             (`expr.rs`'s `Builtin::Nand|Nor|Xnor` arms), proven now by \
             bug_60_nand_of_an_extend_matches_icarus (a narrow-rendering \
             `extend`-wrapped operand, per rule (a′))"
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
///
/// Rule (a′) applies here too (`kinds.rs`'s own module doc, GAP-15): an
/// arm's text claiming an approximation is "safe"/"harmless" needs a
/// checked fact behind it, not an assertion — `Unary`'s own entry below
/// is the worked example (chasing why "ignores `op`" is safe found
/// BUG-58 one layer down).
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
/// wildcard — matching every other axis's own convention. Rule (a′)
/// applies here too (`kinds.rs`'s own module doc, GAP-15) — round-4
/// batch 8's "provably variant-blind by construction" arms below (`Add`/
/// `Sub`, the six-way wrap group) already meet the bar: not "shares a
/// path, untested," but "the code cannot see which variant it's
/// classifying," checkable by reading `kinds.rs` directly.
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
             test, plus bug_19_lossless_sub_in_a_concat_matches_icarus \
             end-to-end. `Sub` is PROVABLY variant-blind here, not just \
             sharing a similar path (round-4 Task 4, batch 8): `infer_binary` \
             has one arm for `Add | Sub` and `op` is never read inside its \
             body — `adapted_lossless_operands(lhs, rhs, ..)` then \
             `lossless_result(l, r, false)`, verified by reading \
             kinds.rs:432-435 directly, so no `Sub`-specific divergence from \
             `Add` can exist in this GATE arm regardless of input"
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
             unit test (literal on either side), end-to-end via \
             bug_19_wrapping_sub_in_a_bitand_matches_icarus (BitAnd) and \
             bug_23's own wrap family. `SubWrap`/`MulWrap`/`BitOr`/`BitXor` \
             are PROVABLY variant-blind here (round-4 Task 4, batch 8), the \
             same upgrade `Sub`'s citation above just got: all six variants \
             share ONE `infer_binary` arm and `op` is never read inside its \
             body (kinds.rs:440-469) — `adapts_to_sibling` on each operand, \
             then either a direct passthrough or `matched_result(l, r)`, \
             identically regardless of which of the six operators is \
             actually being classified. A variant-specific bug here is not \
             merely untested, it is IMPOSSIBLE by construction — the only \
             way any of these four could diverge from `AddWrap`/`BitAnd` is \
             a bug in the shared function itself, which would show up in \
             all six identically and IS covered"
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
/// isolating `infer_call` the way a unit test would. Rule (a′) applies
/// here too (`kinds.rs`'s own module doc, GAP-15) — `Nand`/`Nor`/`Xnor`'s
/// entry below is a fixed `Kind{width:1}` regardless of operand, which is
/// a checked fact about THIS function's own code (`kinds.rs`'s arm body),
/// not an inference about the operator — the form a′ asks every
/// `NotApplicable`-shaped claim to take.
#[allow(dead_code)]
fn builtin_infer_call_coverage(builtin: &ast::Builtin) -> &'static str {
    use ast::Builtin;
    match builtin {
        Builtin::Extend | Builtin::Trunc => {
            "covered end-to-end by bug_28_extend_in_concat_matches_icarus/\
             bug_28_extend_in_replication_matches_icarus (`Extend`) and \
             matrix_trunc_in_concat_matches_icarus plus the bug_36/bug_44/\
             bug_46/bug_49 `Trunc` family (docs/audit/bugs.md). PROVABLY \
             variant-blind, not just sharing a similar path (round-4 Task 4, \
             batch 8): `infer_call` has one arm for `Extend | Trunc` and \
             `func` is never read inside its body (kinds.rs:497-504) — \
             `width = const_fold(args[1])`, `signed = \
             infer_kind(args[0]).signed`, identically regardless of which \
             builtin is being classified — so a `Trunc`-specific divergence \
             from `Extend` is impossible by construction here, only a \
             shared-function bug is, which both citations above would catch"
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

// =======================================================================
// Round-6 plan Task 7 (GAP-17, `docs/plan/v0.2-class-closure-round6.
// local.md`): the sixth coverage doc, and the first keyed by CALL SITE
// rather than by `ExprKind`/`Builtin` variant.
//
// Round 6's own central finding (`docs/audit/review-2026-08-15.md` Part
// 3.4): rule (a′), and the five coverage docs above, audit what an ARM
// answers (`infer_kind`/`verilog_self_determined_kind`'s own match
// statements). Ten of the fourteen live instances of this bug family
// since round 3 were never in an arm's answer — they were in the
// PLUMBING that feeds a hoist decision from that answer: a call site
// that forgot to hoist at all (BUG-46/49/59/60/61), a `decls` map that
// was empty or scoped to the wrong thing (BUG-53, BUG-62), a hoist that
// fired in the wrong SCOPE (BUG-63), or one that fired at the wrong TIME
// relative to its own use (BUG-45/63's ordering half). An arm can be
// exhaustive and every one of its answers correct while the call site
// reading that answer still ships a silent miscompile — that is
// precisely what round 5's own BUG-60 fix said out loud ("the classifier
// arms stay `None`, unchanged ... only the render call site needed the
// hoist"), and it is why this axis exists as its OWN doc instead of a
// sixth column bolted onto `expr_kind_self_determined_coverage` above.
//
// What changed since round 5 that makes "emitter context" a real axis
// (not just a nice-to-have): before round 6, `cur_decls` only ever held
// ONE thing — the enclosing module's own flattened signals — so every
// hoist call site had exactly one context to reason about. Round-6 Task 2
// gives a `fn` body and a testbench's own `Emitter` a REAL `decls` too
// (previously either the module's own, wrong, map, or empty), and Task 4
// gives a `fn`-body hoist its own function-local `reg` buffer instead of
// sharing the module's `wire`/`assign` one. Every entry below states
// which of these three contexts (module body / `fn` body / testbench)
// the position can occur in TODAY, not just which one happened to be
// tested when the call site was first written.
//
// Rule (a′) applies here identically to how it applies to an arm
// (`self_determined.rs`'s own module doc, GAP-15): a "fires" claim below
// is only acceptable when cited to a differential whose hoisted operand
// renders NARROWER than its mimz width (never a bare identifier) — and,
// per this axis's own addition, in the SPECIFIC emitter context claimed,
// not just "a similar-looking test exists somewhere". Several entries
// below say so explicitly where the honest answer is "proven in module
// body, architecturally identical but not separately pinned in fn body/
// testbench" — an unproven claim stated as proven is exactly the BUG-52/
// BUG-60 failure mode this whole family exists to stop making.
//
// Round-6 plan Task 8 (a′-2)'s fourth category applies here with special
// force, since this doc IS the call-site axis: an entry ANYWHERE in this
// file (or in `self_determined.rs`/`kinds.rs`) that rests its reason on
// "a hoist happens elsewhere" must name the specific `HoistCallSite.name`
// below that performs it and the condition under which it fires — "it's
// hoisted" alone, unnamed, is not a reason (`builtin_self_determined_
// coverage`'s old `Trunc` arm, round-6 review Part 3.3, is the worked
// counter-example: true only because `HOIST_CALL_SITES["trunc base"]`
// fires, false as an unqualified property of the operator).
//
// Exhaustiveness cannot be build-enforced here the way a `match` can (a
// call site is a source LOCATION, not a value the compiler can pattern-
// match on) — paired instead with `every_hoist_call_site_in_expr_rs_has_
// a_coverage_entry` below, a crude source-scan count that fails the
// moment `expr.rs` gains or loses a `hoist_if_needed`/`hoist_slice_base_
// if_needed`/`hoist_width_effect_operand` call without a matching entry
// here. `name` is a stable KEY (not a line number — those drift on any
// unrelated edit); it matches the exact `site` string `hoist_unresolved`
// receives where one exists, invented consistently otherwise.
// =======================================================================

/// One `hoist_if_needed`/`hoist_slice_base_if_needed`/
/// `hoist_width_effect_operand` call site in `emit_verilog/expr.rs`. See
/// this section's own module doc above for the discipline every
/// `coverage` string here is held to.
#[allow(dead_code)]
struct HoistCallSite {
    /// Stable name for this call site.
    name: &'static str,
    /// Which of the three functions this call reaches.
    via: &'static str,
    /// Position, emitter context(s), fires-differential, doesn't-fire
    /// control, and `None`-branch story, in that order.
    coverage: &'static str,
}

#[allow(dead_code)]
const HOIST_CALL_SITES: &[HoistCallSite] = &[
    HoistCallSite {
        name: "Unary reduction operand",
        via: "hoist_if_needed",
        coverage: "Position: a reduction's (`&`/`|`/`^`) OPERAND — always \
            self-determined in Verilog, distinct from the reduction's own \
            always-1-bit RESULT (`ExprKind::Unary`, expr.rs:589). Contexts: \
            module body (BUG-60's own filing), fn body (BUG-62①, `fn \
            allset(x) { &extend(x,8) }` — Task 2 gives the fn its own \
            decls), testbench (Task 2's DUT-decls install + Task 5's \
            hoisted-decls flush apply identically to an `expect`'s own \
            reduction, though no dedicated testbench differential isolates \
            THIS position specifically — `every_emitted_testbench_reports_\
            pass_under_vvp`, tests/icarus.rs, is the general net). Fires: \
            bug_60_and_reduction_of_an_extend_in_a_concat_matches_icarus \
            (module body) and bug_62_reduction_of_an_extend_inside_a_fn_\
            body_matches_icarus (fn body). Doesn't fire: bug_60_and_\
            reduction_of_a_bare_identifier_stays_unhoisted. None branch: \
            try_widen_symbolic_extend (Task 3) first, then hoist_unresolved(\
            \"Unary reduction operand\") (Task 1) — never silent after \
            those two land.",
    },
    HoistCallSite {
        name: "comparison LHS operand",
        via: "hoist_if_needed",
        coverage: "Position: `Eq|Ne|Lt|Le|Gt|Ge`'s left operand \
            (expr.rs:662) — hoisted independently of the RHS. Contexts: \
            module/fn/testbench (an `expect x == 0` renders through this \
            exact arm; no dedicated fn-body/testbench differential exists \
            for this specific operand). Fires + doesn't-fire together: \
            task5_comparison_operand_hoist_catches_a_mismatch_matches_\
            icarus (`extend(a,8)` LHS hoists; `b` RHS, a bare identifier, \
            doesn't — the pairing IS the over-hoist control, confirmed by \
            hand-reading the emission: exactly one `__mimz_sub` wire). \
            None branch: deliberately NOT routed through hoist_unresolved \
            (expr.rs:663-677's own comment) — a comparison's operands are \
            LRM-auto-widened (5.5.1) to match before comparing, so an \
            un-hoisted parameter-driven operand (`x == (DEPTH-1)`, \
            `std/fifo.mimz`, a working golden) already gets the right \
            VALUE from Verilog's own widening, in any emitter context.",
    },
    HoistCallSite {
        name: "comparison RHS operand",
        via: "hoist_if_needed",
        coverage: "Position: `Eq|Ne|Lt|Le|Gt|Ge`'s right operand \
            (expr.rs:684) — the mirror of `comparison LHS operand`, same \
            guard, opposite side. Fires + doesn't-fire: task7_comparison_\
            rhs_operand_hoist_catches_a_mismatch_matches_icarus (round-6 \
            Task 7 — no existing differential put the narrow operand on \
            the RHS before this; `extend(a,8)` RHS hoists, `b` LHS \
            doesn't, confirmed by hand-reading the emission). Contexts and \
            None branch: identical reasoning to `comparison LHS operand` \
            — same code shape, same LRM guarantee, independent of emitter \
            context.",
    },
    HoistCallSite {
        name: "concat member",
        via: "hoist_if_needed",
        coverage: "Position: each `{...}` member (ExprKind::Concat, \
            expr.rs:820) — Verilog fixes a concat's own width as the exact \
            sum of its members' rendered widths, so a member rendering \
            narrower than its mimz width must be hoisted. Contexts: \
            module body (BUG-19's founding case), fn body (BUG-62③ — \
            packit, BUG-28's own 2575-vs-175 divergence reached through a \
            `fn`), testbench (Task 2/5, no dedicated differential isolates \
            this position there specifically). Fires: bug_19_lossless_\
            sub_in_a_concat_matches_icarus (module), bug_62_extend_in_a_\
            concat_inside_a_fn_body_matches_icarus (fn). Doesn't fire: \
            `b` in any of the bug_60_*_in_a_concat tests — a plain \
            identifier concat member never spuriously hoists \
            (`hoist_if_needed`'s own is_plain_identifier early return, \
            ports.rs:558, applies uniformly regardless of call site). \
            None branch: try_widen_symbolic_extend (Task 3 — task7_\
            symbolic_extend_base_hoist_when_base_is_composite_matches_\
            icarus is exactly this position with the widen path forced) \
            then hoist_unresolved(\"concat member\") (Task 1).",
    },
    HoistCallSite {
        name: "replicate member",
        via: "hoist_if_needed",
        coverage: "Position: each part of a `{N{...}}` replication body \
            (ExprKind::Replicate, expr.rs:845) — same self-determined-\
            width rule as `concat member`, one construct over. Contexts: \
            module/fn/testbench — same story as `concat member`: no \
            dedicated fn-body/testbench differential exists for THIS \
            construct specifically, but it shares the identical \
            `hoist_if_needed`/`try_widen_symbolic_extend`/`hoist_\
            unresolved` code `concat member` already proves in both \
            contexts (this function has no separate branch for `Concat` \
            vs `Replicate` — both route through the same three calls). \
            Fires: round-7 plan Task 7 (review Part 9.2) replaced this \
            entry's own former citations — shape_replicate_operand_of_\
            extend_in_a_concat_matches_icarus (`extend({2{a}}, 12)`) \
            actually hoists at `concat member` (the MISMATCH there is the \
            whole replication vs `extend`'s target width, `a` alone is \
            already correct as the replication's own body) and bug_60_\
            and_reduction_of_an_extend_in_a_replication_matches_icarus \
            (`&({2{extend(a,8)}})`) hoists at `Unary reduction operand`, \
            neither reaching THIS site — confirmed by reading both \
            emissions. task7_replicate_member_hoists_a_composite_body_\
            matches_icarus (`{c, {2{extend(a, 8)}}}`) isolates the \
            REPLICATION BODY itself as the mismatch instead, confirmed by \
            hand-reading its own emission: exactly one hoisted wire, \
            `{c, {2{__mimz_sub_1}}}`. Doesn't fire: a bare-identifier \
            replication body shares `concat member`'s own \
            is_plain_identifier control architecturally (same code, same \
            guard). None branch: same as `concat member`.",
    },
    HoistCallSite {
        name: "signed-cast operand",
        via: "hoist_if_needed",
        coverage: "Position: `$signed(...)`'s own argument \
            (Builtin::SignedCast, expr.rs:1036). Contexts: module/fn/\
            testbench (always evaluated by plain `eval`, never `eval_ctx` \
            — self-determined regardless of context; no dedicated fn-\
            body/testbench differential isolates it there). Fires: \
            matrix_signed_unsigned_cast_recursion_catches_a_mismatched_\
            operand_matches_icarus. Doesn't fire: matrix_signed_unsigned_\
            cast_roundtrip_in_concat_matches_icarus's own no-mismatch \
            case. None branch: try_widen_symbolic_extend then hoist_\
            unresolved(\"signed-cast operand\").",
    },
    HoistCallSite {
        name: "unsigned-cast operand",
        via: "hoist_if_needed",
        coverage: "Position: `$unsigned(...)`'s own argument \
            (Builtin::UnsignedCast, expr.rs:1059) — mirrors `signed-cast \
            operand` exactly, opposite cast direction, identical code \
            shape. Fires/doesn't-fire: the same matrix_signed_unsigned_\
            cast_* pair (round-4 Task 4's own note: both directions \
            exercised by the same two tests). Contexts and None branch: \
            identical to `signed-cast operand`.",
    },
    HoistCallSite {
        name: "encoding operand",
        via: "hoist_if_needed",
        coverage: "Position: `Builtin::Encoding`'s own argument (an \
            enum-to-bits cast, expr.rs:1083). Round-7 plan Task 7 (review \
            Part 9.2) found this entry's former citation \
            (matrix_encoding_of_payload_enum_in_concat_matches_icarus) is \
            a bare identifier (`p`) — no mismatch, nothing to hoist, \
            proving nothing about this site. Looking for a REAL fires \
            case (not just a corrected citation) turned up a genuinely \
            open question, checked by hand rather than assumed: \
            `infer_kind`'s own `EnumConstruct` arm (kinds.rs) is `_ => \
            None` UNCONDITIONALLY — deliberate, since `EnumConstruct`'s \
            rendering already explicitly zero-pads to the enum's full \
            tag+payload width (confirmed by compiling `encoding(if ... { \
            Packet.Ctrl(k) } else { Packet.Data(v) })` and \
            `encoding(match sel { ... })` directly: both reach `encoding \
            operand` with `infer_kind` returning `None` — `IfExpr`/`Match` \
            recurse into their own arms, which are `EnumConstruct`s, \
            which are always `None`). So EVERY enum-typed non-identifier \
            expression this language can currently construct (a bare \
            `EnumConstruct`, or an `if`/`match` wrapping one) routes \
            through `hoist_unresolved`'s fallback here, never through \
            `hoist_if_needed`'s own `Some(k)` comparison — this is a \
            genuinely open, structurally-unreachable-today coverage gap, \
            not a citation that merely needed replacing. Fires: nothing \
            found; stated honestly rather than fabricated (round-6 Task \
            8's own (a′-2) rule). None branch: hoist_unresolved(\"encoding \
            operand\") — confirmed live above, not theoretical.",
    },
    HoistCallSite {
        name: "nand operand",
        via: "hoist_if_needed",
        coverage: "Position: `nand(...)`'s own argument — the negated-\
            reduction sibling of `Unary reduction operand` \
            (Builtin::Nand, expr.rs:1302). Contexts: module (BUG-60's own \
            nand repro), fn body (BUG-62②), testbench (general net only, \
            see `Unary reduction operand`). Fires: bug_60_nand_of_an_\
            extend_matches_icarus (module), bug_62_negated_reduction_of_\
            an_extend_inside_a_fn_body_matches_icarus (fn). Doesn't fire: \
            matrix_nand_in_concat_matches_icarus's own no-mismatch \
            (already-atomic-operand) case. None branch: try_widen_\
            symbolic_extend then hoist_unresolved(\"nand operand\").",
    },
    HoistCallSite {
        name: "nor operand",
        via: "hoist_if_needed",
        coverage: "Position: `nor(...)`'s own argument (Builtin::Nor, \
            expr.rs:1316) — identical shape to `nand operand`, opposite \
            polarity. Fires: round-7 plan Task 7 (review Part 9.2) — the \
            former citation, bug_60_or_reduction_of_a_negated_extend_\
            matches_icarus, is `|(~extend(a, 8))`: a `|` UNARY REDUCTION \
            over a negated extend, which hoists at `Unary reduction \
            operand` (expr.rs:589) and never reaches `Builtin::Nor`'s own \
            call site at all — confirmed by reading its emission. \
            task7_nor_operand_of_an_extend_matches_icarus \
            (`nor(extend(a, 8))`, the actual builtin call) isolates this \
            site instead, confirmed the same way (module body only — no \
            fn-body differential isolates `nor` specifically; the code \
            path is byte-identical to `nand operand`'s own fn-body-proven \
            one, `Nand`/`Nor`/`Xnor` sharing one `hoist_if_needed`/\
            `try_widen_symbolic_extend`/`hoist_unresolved` shape and \
            differing only in the render template). Doesn't fire: \
            matrix_nor_in_concat_matches_icarus's own no-mismatch case. \
            Contexts and None branch: same as `nand operand`.",
    },
    HoistCallSite {
        name: "xnor operand",
        via: "hoist_if_needed",
        coverage: "Position: `xnor(...)`'s own argument (Builtin::Xnor, \
            expr.rs:1330) — identical shape to `nand operand`/`nor \
            operand`. Fires: round-7 plan Task 7 (review Part 9.2) — the \
            former citation, matrix_xnor_in_concat_matches_icarus, is \
            `xnor(a)`: `a` bare, no mismatch, its OWN no-mismatch control \
            case, proving nothing about this site — confirmed by reading \
            its emission (`(~^(a))`, no hoist). task7_xnor_operand_of_an_\
            extend_matches_icarus (`xnor(extend(a, 8))`) isolates a real \
            mismatch instead, confirmed the same way — module body only, \
            same fn-body gap as `nor operand`. Doesn't fire: matrix_xnor_\
            in_concat_matches_icarus (unchanged, still the no-mismatch \
            control). Contexts and None branch: same as `nand operand`.",
    },
    HoistCallSite {
        name: "symbolic-extend base",
        via: "hoist_slice_base_if_needed",
        coverage: "Position: inside try_widen_symbolic_extend \
            (expr.rs:172) — hoists `extend(x, W)`'s own operand `x` into \
            a named wire before splicing it into the explicit `{{(W)-(N)\
            {fill}}, named}` widen text, whenever `x`'s rendered text \
            isn't already a plain identifier. Only reached when Task 3's \
            own condition holds (`W` doesn't const-fold) AND the calling \
            self-determined position's `infer_kind(extend(x,W))` was \
            `None`. Contexts: module body only — no fn-body/testbench \
            differential exercises a symbolic-width extend at all yet \
            (Task 6's fuzz generator only ever widens a bare port). \
            Fires: task7_symbolic_extend_base_hoist_when_base_is_\
            composite_matches_icarus (round-6 Task 7 — the only \
            differential forcing a COMPOSITE base here; every prior \
            BUG-62(b) repro used a bare port, which never reaches this \
            line at all, confirmed by hand-reading the emission of both). \
            Doesn't fire: BUG-62(b)'s own bare-port repros (`&extend(a,\
            W)`, `{b,extend(a,W)}`) — `is_plain_identifier(\"a\")` short-\
            circuits before this line runs. None branch: `k.width == 0` \
            or `infer_kind(x)` itself unresolvable falls through to the \
            CALLER's own hoist_unresolved (try_widen_symbolic_extend's \
            own doc comment: 'a residual case this doesn't attempt to \
            close') — no repro exercises that residual.",
    },
    HoistCallSite {
        name: "width-effect/shift operand's own hoist",
        via: "hoist_slice_base_if_needed",
        coverage: "Position: inside hoist_width_effect_operand's \
            `Some(kind)` arm (expr.rs:286) — once `child` is confirmed \
            hoistable (a lossless/wrap binop unconditionally, or a shift \
            when `allow_shift`) AND its `Kind` resolves, this line does \
            the actual wire-and-assign. Shared by every one of the four \
            `hoist_width_effect_operand` call sites' own `Some` path — \
            its fires/doesn't-fire citations ARE theirs, since the \
            `hoistable` gate one function up (expr.rs:272-273) decides \
            whether this line is ever reached at all, not this line \
            itself. Contexts: module/fn/testbench, same as those four. \
            None branch: NOT routed through hoist_unresolved by design \
            (expr.rs:288-304's own comment) — a module `int` parameter \
            inside a width-effect/shift child has no `Kind` by \
            construction (`Ty::CtInt`, not `bits[N]`), and Verilog's own \
            context growth is harmless there once the base growth is \
            lossless — proven by examples/*/shift.mimz's `extend(3 << \
            AMOUNT, 8)`, a working golden this fallback has always \
            covered.",
    },
    HoistCallSite {
        name: "self-determined if/match branch (render_shift_ctx_operand)",
        via: "hoist_slice_base_if_needed",
        coverage: "Position: inside render_shift_ctx_operand \
            (expr.rs:374) — when `child` is itself an `if`/`match` \
            sitting at a `!allow_shift` position (the LHS of an outer \
            shift, `allow_shift_lhs`) AND its own `Kind` resolves, hoists \
            the WHOLE if/match's rendered text — BUG-59's fix: a fused \
            shift chain hidden in a branch resolves bottom-up in the \
            kernel but would otherwise inherit the outer assignment's \
            grown context if left inline. Contexts: module body (the \
            only context any Shl/Shr-of-an-if-branch differential \
            exists in). Fires: bug_59_fused_shift_chain_inside_an_if_\
            branch_as_the_lhs_of_a_growing_shift_matches_icarus — \
            confirmed by hand-reading the emission: the whole `if` \
            hoists into one `wire signed [10:0] __mimz_sub_1`, its \
            branches' own internal shifts left un-hoisted (see `if-expr \
            then-branch`'s own doesn't-fire note). Doesn't fire: whenever \
            the position is instead self-determined (`allow_shift: \
            true`), this branch's own `!allow_shift` guard excludes it — \
            bug_55_signed_shift_right_inside_match_wildcard_arm_matches_\
            icarus's own match (sitting inside `extend(...)`) never \
            reaches this line at all, taking `match arm value`'s own \
            path instead (confirmed by hand-reading its emission: no \
            whole-match wire, only the wildcard arm's own `__mimz_sub_3`). \
            None branch: `infer_kind(child)` unresolvable falls through \
            to hoist_width_effect_operand (line 376) unchanged — no \
            repro exercises an unresolvable if/match Kind here.",
    },
    HoistCallSite {
        name: "bit-select base",
        via: "hoist_slice_base_if_needed",
        coverage: "Position: `x[i]`'s own base (ExprKind::Index, \
            expr.rs:901) — Verilog's bit-select grammar only accepts a \
            plain identifier (BUG-61). Contexts: module body (BUG-61's \
            own filing; no fn-body/testbench differential isolates this \
            position, though its `None`-arm code is identical to `slice \
            base`/`trunc base`, both partially fn-body-proven — none of \
            the three's proofs transfers automatically, since each is a \
            SEPARATE call site). Fires: bug_61_bit_select_of_an_extend_\
            hoists_the_base_and_matches_icarus, bug_61_bit_select_of_a_\
            concat_hoists_the_base_and_matches_icarus. Doesn't fire: \
            bug_53_control_case_non_zero_base_identity_index_still_\
            hoists's own bare-identifier base stays a plain part-select \
            — the identical is_plain_identifier early return every other \
            site here shares. None branch: hoist_unresolved(\"bit-select \
            base\", requires_named_wire: true).",
    },
    HoistCallSite {
        name: "slice base",
        via: "hoist_slice_base_if_needed",
        coverage: "Position: `x[hi:lo]`'s own base (ExprKind::Slice, \
            expr.rs:923) — same BUG-20 grammar constraint as `bit-select \
            base`, one construct over. Contexts: module body. Fires: \
            bug_20_slice_of_a_composite_expression_matches_icarus. \
            Doesn't fire: a bare-identifier slice base (e.g. `p0[3:0]`, \
            used throughout this file's own const-bounded-slice tests) \
            stays a plain part-select — is_plain_identifier's early \
            return applies uniformly. None branch: hoist_unresolved(\
            \"slice base\", requires_named_wire: true).",
    },
    HoistCallSite {
        name: "trunc base",
        via: "hoist_slice_base_if_needed",
        coverage: "Position: `trunc(x, N)`'s own base, rendered as an \
            explicit part-select `x[N-1:0]` (Builtin::Trunc, \
            expr.rs:1238) — same grammar constraint as `bit-select base`/\
            `slice base` (BUG-36). Contexts: module body. Fires: bug_36_\
            trunc_of_a_concat_hoists_the_base_first, shape_replicate_\
            nested_in_trunc_hoists_the_base. Doesn't fire: bug_44_trunc_\
            of_a_signed_value_stays_signed_in_verilog's own bare-\
            identifier base (`trunc(a, 3)`) stays a plain part-select — \
            the EXACT shape round-6's own review (Part 3.3) found \
            `builtin_self_determined_coverage`'s `Trunc` arm citing as \
            false-in-general ('regardless of position') while being \
            narrowly true for the CLASSIFIER question; here, at the \
            RENDER call site, it genuinely is the correct, tested \
            control. None branch: hoist_unresolved(\"trunc base\", \
            requires_named_wire: true) — `trunc(extend(x,8), 2)` inside \
            a `fn` (round 6's own review, Part 3.3) is exactly this \
            position with an unresolvable base (a symbolic-width \
            extend, since Task 3 only widens reduction/concat/cast/nand \
            positions, never a slice/trunc base) — no repro pins that \
            residual diagnostic path yet.",
    },
    HoistCallSite {
        name: "width-effect/shift child (render_shift_ctx_operand fallback)",
        via: "hoist_width_effect_operand",
        coverage: "Position: render_shift_ctx_operand's own final call \
            (expr.rs:376) — the generic entry every OTHER self-\
            determined position (concat/replicate member, cast/encoding/\
            nand/nor/xnor argument, comparison operand, extend's own \
            argument) routes a lossless/wrap/shift child through. \
            Contexts: module/fn/testbench (reached from any of the \
            eleven `hoist_if_needed` sites' own render_shift_ctx_operand \
            call, so its context list is the union of theirs). Fires: \
            bug_23_wrap_directly_inside_a_concat_matches_icarus, bug_24_\
            shl_under_sibling_add_matches_icarus. Doesn't fire: bug_23_\
            top_level_wrap_needs_no_hoist — the function's own doc \
            comment states it is never even called at the true top-level \
            statement-RHS render, where the assignment's own declared \
            width already pins the result. None branch: n/a — this call \
            always returns SOME text (either hoisted, or unchanged by \
            `width-effect/shift operand's own hoist`'s own None one \
            level down).",
    },
    HoistCallSite {
        name: "if-expr then-branch",
        via: "hoist_width_effect_operand",
        coverage: "Position: if_expr_subst's own `then` branch \
            (expr.rs:414). Contexts: module body (context-determined, \
            `allow_shift: false`, the ordinary top-level `if`) and self-\
            determined (`allow_shift: true`, when the whole `if` sits \
            inside e.g. extend()'s argument). Fires: round-7 plan Task 6 \
            (review Part 5) closed the open gap round-6 Task 7 correctly \
            left here — task6_if_expr_then_branch_wrap_add_operand_\
            inside_extend_matches_icarus and task6_if_expr_then_branch_\
            wrap_mul_operand_inside_extend_matches_icarus force a plain \
            (non-fused) `+%`/`*%` operand into the `then` branch of a \
            SELF-determined `if` (`c=1`, `a=0b1111`) — confirmed to \
            actually exercise THIS site by hand: reverting both hoist \
            calls fails all four (both branches') differentials, not \
            just `every_hoist_call_site_in_expr_rs_has_a_coverage_entry`'s \
            own count. bug_59_fused_shift_chain_inside_an_if_branch_as_\
            the_lhs_of_a_growing_shift_matches_icarus exercises the SAME \
            `then` position with a FUSED chain, but that gets caught one \
            function up instead (`self-determined if/match branch`, \
            confirmed by hand-reading its emission: no hoist happens \
            HERE for that test). Doesn't fire: bug_24_regression_shift_\
            in_if_branch_stays_unhoisted — the top-level, context-\
            determined `y = if cond {1<<3} else {0}` case, `allow_shift: \
            false`, correctly leaves the `then` branch unhoisted \
            (`hoistable` requires `allow_shift && is_shift_binop`, false \
            here); bug_59's own emission is a second doesn't-fire \
            witness (allow_shift: false there too, for a different \
            reason — see that entry). None branch: text unchanged — a \
            module `int` parameter inside this branch is BUG-30's own \
            documented-safe case (see `width-effect/shift operand's own \
            hoist`); no repro isolates it at THIS specific branch beyond \
            the general shift.mimz golden that entry cites.",
    },
    HoistCallSite {
        name: "if-expr else-branch",
        via: "hoist_width_effect_operand",
        coverage: "Position: if_expr_subst's own `els` branch \
            (expr.rs:416) — same code shape as `if-expr then-branch`, \
            opposite branch. Fires: round-7 plan Task 6, the same close \
            as `if-expr then-branch` one entry up — task6_if_expr_else_\
            branch_wrap_add_operand_inside_extend_matches_icarus and \
            task6_if_expr_else_branch_wrap_mul_operand_inside_extend_\
            matches_icarus force a plain `+%`/`*%` operand into the \
            `els` branch of a SELF-determined `if` (`c=0`, `a=0b1111`), \
            confirmed the same way: reverting the `els`-branch hoist call \
            alone fails these two without touching the `then`-branch \
            pair. Doesn't fire: bug_24_regression's own `else { 0 }` is a \
            bare literal, not a hoistable shape, so it proves nothing \
            about THIS branch's own hoistable path beyond trivially not \
            firing — a weaker witness than `then-branch`'s own control. \
            Contexts and None branch: identical to `if-expr then-branch`.",
    },
    HoistCallSite {
        name: "match arm value",
        via: "hoist_width_effect_operand",
        coverage: "Position: match_subst's own `arm.value` \
            (expr.rs:450), once per arm. Contexts: module body and \
            self-determined (the whole `match` sitting inside extend()'s \
            argument, `allow_shift: true`). Fires: bug_55_signed_shift_\
            right_inside_match_wildcard_arm_matches_icarus — a signed \
            `>>` in a wildcard arm, the match wrapped by `extend(...)`, \
            correctly hoisted into its own `__mimz_sub_3` so it doesn't \
            inherit the outer 16-bit context (confirmed by hand-reading \
            the emission). Doesn't fire: shape_match_operand_of_add_in_\
            concat_matches_icarus's own arms (`a`/`b`, bare identifiers \
            — not width-effect/shift shapes at all, so `hoistable` is \
            false regardless of `allow_shift`). None branch: same BUG-\
            30-documented-safe case as the other `hoist_width_effect_\
            operand` sites — no repro isolates an unresolvable-Kind \
            width-effect match arm specifically.",
    },
];

/// Round-6 plan Task 7 (GAP-17), widened by round-7 plan Task 7 (review
/// Part 4.2): the crude, build-enforced half of the exhaustiveness this
/// axis can't get from a `match`. Counts every `self.hoist_if_needed(`/
/// `self.hoist_slice_base_if_needed(`/`self.hoist_width_effect_operand(`
/// call TEXT appears ANYWHERE under `crates/mimz-core/src/emit_verilog/`
/// (every `.rs` file, not just `expr.rs`) and asserts it equals
/// `HOIST_CALL_SITES.len()` — a plain string count, not an AST walk,
/// deliberately: "crude, and enough" is round-6 Task 7's own stated bar,
/// and a source-scan the reader can verify by eye (`grep -rc`) is more
/// trustworthy than a parser this test would itself need proving correct.
/// Round 7's own review found the count was correct today (21 sites, all
/// in `expr.rs`) but noted the guard scanning `expr.rs` ALONE would miss
/// a hoist call added in a NEW emitter context/file — precisely the
/// failure mode this whole axis exists to catch (BUG-66/67/68 were all
/// new-context failures). Adding, removing, or renaming a hoist call site
/// ANYWHERE in `emit_verilog/` without updating `HOIST_CALL_SITES` above
/// now fails here.
#[test]
fn every_hoist_call_site_in_emit_verilog_has_a_coverage_entry() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("crates/mimz-core/src/emit_verilog");
    let patterns = [
        "self.hoist_if_needed(",
        "self.hoist_slice_base_if_needed(",
        "self.hoist_width_effect_operand(",
    ];
    let mut actual = 0;
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let src = fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
                actual += patterns
                    .iter()
                    .map(|pat| src.matches(pat).count())
                    .sum::<usize>();
            }
        }
    }
    assert_eq!(
        actual,
        HOIST_CALL_SITES.len(),
        "emit_verilog/ now has {actual} hoist_if_needed/hoist_slice_base_if_needed/\
         hoist_width_effect_operand call sites (across every .rs file, not just \
         expr.rs) but HOIST_CALL_SITES here has {} entries — every hoist call \
         site needs its own coverage entry (position, emitter context(s), \
         fires/doesn't-fire differentials, None-branch safety). Add or \
         remove an entry above to match.",
        HOIST_CALL_SITES.len()
    );
}

/// Sanity check on `HOIST_CALL_SITES` itself — every entry has a non-empty
/// name/coverage and a `via` naming one of the three functions this axis
/// covers, and no two entries share a `name` (the field the module doc
/// above treats as a stable key). Not exhaustiveness (that's the count
/// guard above) — just guards against a copy-paste entry left blank or
/// duplicated while editing the list.
#[test]
fn hoist_call_sites_are_well_formed_and_unique() {
    let mut seen = std::collections::HashSet::new();
    for site in HOIST_CALL_SITES {
        assert!(
            !site.name.is_empty(),
            "a HOIST_CALL_SITES entry has an empty name"
        );
        assert!(
            !site.coverage.is_empty(),
            "HOIST_CALL_SITES[{:?}] has empty coverage text",
            site.name
        );
        assert!(
            matches!(
                site.via,
                "hoist_if_needed" | "hoist_slice_base_if_needed" | "hoist_width_effect_operand"
            ),
            "HOIST_CALL_SITES[{:?}] names an unknown function {:?}",
            site.name,
            site.via
        );
        assert!(
            seen.insert(site.name),
            "HOIST_CALL_SITES has a duplicate name: {:?}",
            site.name
        );
    }
}

/// Round-7 plan Task 1 (GAP-18, `docs/audit/gaps.md`): the hoist buffer's
/// flush point is a second scoping axis alongside `hoist_unresolved`'s
/// "which `decls` is in scope" — a hoisted wire can resolve its `Kind`
/// correctly and still be declared after its own use, if the render site
/// that asked for it runs before the buffer is flushed (BUG-66). The
/// invariant enforcing this (`assert_hoists_declared_before_use`,
/// `crates/mimz-core/src/emit_verilog/mod.rs`) is a `debug_assert!` that
/// stays live in this workspace's release profile too (`[profile.release]
/// debug-assertions = true`), so a violation anywhere in the corpus below
/// surfaces as `mimz compile` aborting — `support::compile_example`'s own
/// `assert!(status.success(), ...)` turns that into a normal test failure,
/// backtrace and all, not a silent pass.
///
/// Walks `examples/` + `demo/` — 226 `.mimz` files, the exact count round
/// 7's own one-off machine check covered by hand (review Part 3.1, "zero
/// out-of-order references"). This reproduces that sweep as a real,
/// running regression instead of a one-time audit claim.
#[test]
fn task1_hoisted_wire_is_never_referenced_before_its_declaration() {
    let files = support::corpus_files();
    for path in &files {
        support::compile_example(path);
    }
}

/// Round-7 plan Task 2 (review Part 4.3, BUG-67/68): `hoist_unresolved`
/// (`crates/mimz-core/src/emit_verilog/module/ports.rs`) used to reach a
/// bare `debug_assert!(false, ...)` with NO `Diag` at all for a
/// `requires_named_wire: false` site (concat/reduction/cast/encoding
/// members) — combined with `[profile.release] debug-assertions = true`,
/// a SHIPPED `mimz compile` aborted with a Rust panic backtrace on this
/// exact checker-clean program (BUG-67's own repro — a nested `fn` call
/// whose return `Kind` `render_fn_decl`'s `decls` doesn't carry yet,
/// Task 4 isn't landed) instead of exiting non-zero with a diagnostic.
///
/// Deliberately spawns the REAL `mimz` binary rather than calling
/// `emit_src` in-process: the `debug_assert!` this task keeps for
/// development is gated on `cfg!(test)`, which is true inside the
/// `cargo test` binary (so an in-process call would still panic, by
/// design — reaching this fallback during development stays loud) but
/// false for the built CLI binary this spawns, in either profile. Only
/// spawning the binary proves what an actual `mimz compile` user sees.
#[test]
fn task2_unresolvable_concat_member_is_a_diagnostic_not_a_panic() {
    // Repointed at BUG-68's own repro (review Part 3.3, the testbench
    // hoisting-half): Task 4 (BUG-67) below fixed this test's ORIGINAL
    // source (a nested `fn` call inside another `fn`'s body), so it no
    // longer reaches `hoist_unresolved` at all — exactly the churn Task 1's
    // own `#[should_panic]` test hit when Task 3 landed. BUG-68 is still
    // open (Task 5 hasn't landed); when it does, THIS test will need a new
    // still-open repro in turn — `hoist_unresolved`'s fallback is meant to
    // become unreachable for every known shape as this plan's tasks land,
    // so any test targeting it necessarily borrows a temporarily-open bug.
    let src = "const BIG: int = 1\n\
               module Fuzz {\n  in a: bits[4]\n  const if (BIG == 1) {\n    out y: bits[4]\n  }\n  \
               y = a\n}\n\n\
               test \"t\" for Fuzz {\n  a = 0b1111\n  expect &extend(y, 8) == 0\n}\n";
    let path = std::env::temp_dir().join("mimz_task2_bug68_repro.mimz");
    fs::write(&path, src).unwrap();
    let out_v = std::env::temp_dir().join("mimz_task2_bug68_repro.v");
    let out = support::mimz()
        .arg("compile")
        .arg(&path)
        .arg("-o")
        .arg(&out_v)
        .arg("--emit-testbench")
        .output()
        .unwrap();
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(&out_v);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected `mimz compile --emit-testbench` to reject BUG-68's repro \
         (unresolvable reduction over a const-if-declared port), but it exited 0"
    );
    assert!(
        !stderr.contains("panicked at"),
        "expected a clean diagnostic, not a Rust panic:\n{stderr}"
    );
    assert!(
        stderr.contains("GAP-16"),
        "expected the GAP-16 diagnostic text, got:\n{stderr}"
    );
}

// ---------------------------------------------------------------------
// Round-7 plan Task 3 (BUG-66, GAP-18, review Part 3.1): a hoisted wire
// needed by an instance port connection, a `reg` reset value, or a `mem`
// init/depth expression used to be declared AFTER the line that already
// used it — all three render before `hoist_pos` is captured
// (`module/mod.rs`). `mimz check`/`compile` both exited 0; real Icarus
// refused every one of them ("declaration after use"). Task 3 routes
// these three sites' hoists through a second buffer
// (`pre_decl_hoisted_decls`) spliced right after the module's own
// wire/reg/mem declarations instead. Every differential below shares the
// same math (`a = 0b1111`, `b = 0b1010` → `{b, extend(a, 8)}` = 0xA0F =
// 2575), matching BUG-63's own value so a wrong-order-vs-wrong-value
// regression is equally visible either way.
// ---------------------------------------------------------------------

#[test]
fn bug_66_a1_instance_port_connection_hoist_matches_icarus() {
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[12]\n  let u = Sub() { d: { b, extend(a, 8) } }\n  \
                y = u.q\n}\n\n\
                module Sub {\n  in d: bits[12]\n  out q: bits[12]\n  q = d\n}\n";
    differential_clocked(src, Some("Fuzz"), &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_66_a1_repeat_unrolled_instance_port_connection_hoist_matches_icarus() {
    // "A1 also reproduces through a `repeat`-unrolled instance array (two
    // wires, both out of order)" — review Part 3.1.
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[12]\n  repeat i: 0..2 {\n    \
                let u[i] = Sub() { d: { b, extend(a, 8) } }\n  }\n  \
                y = u[0].q\n}\n\n\
                module Sub {\n  in d: bits[12]\n  out q: bits[12]\n  q = d\n}\n";
    differential_clocked(src, Some("Fuzz"), &[("a", 0b1111), ("b", 0b1010)]);
}

/// `reg`/`mem` reset-and-init values don't go through `differential_clocked`
/// (BUG-66 A2/A3 below): `elaborate_project` (mimz-sim) requires that
/// value to be a compile-time constant — a pre-existing, separate
/// limitation of OUR OWN kernel, unrelated to BUG-66 (the checker, and
/// the emitter, both accept a runtime port there; only mimz-sim's
/// elaborator doesn't). Compiles + runs against real Icarus directly
/// instead (mirrors the review's own verification method for these two
/// repros): the acceptance oracle is `support::run_vvp`'s own build-status
/// assert (BUG-66 made Icarus refuse to elaborate this at all) plus a
/// value check against the source's own math.
fn icarus_only_clocked(
    bin: &std::path::Path,
    src: &str,
    tag: &str,
    inputs: &[(&str, u32, u128)],
    output: (&str, u32),
    cycles: u64,
) -> std::collections::BTreeMap<String, u128> {
    let path = std::env::temp_dir().join(format!("mimz_{tag}.mimz"));
    fs::write(&path, src).unwrap();
    let design_v = support::compile_example(&path);
    let _ = fs::remove_file(&path);
    let inputs_meta: Vec<(String, u32, u128)> = inputs
        .iter()
        .map(|(n, w, v)| (n.to_string(), *w, *v))
        .collect();
    let outputs_meta = vec![(output.0.to_string(), output.1)];
    let tb = support::clocked_testbench(
        "Fuzz",
        &[],
        "clk",
        Some("rst"),
        &inputs_meta,
        &outputs_meta,
        cycles,
        1,
    );
    let stdout = support::run_vvp(bin, tag, &design_v, &tb);
    let icarus = support::parse_icarus(&stdout);
    let last = *icarus
        .keys()
        .max()
        .expect("no cycles recorded in Icarus output");
    icarus[&last].clone()
}

#[test]
fn bug_66_a2_reg_reset_hoist_matches_icarus() {
    let Some(bin) = support::require_iverilog() else {
        return;
    };
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[12]\n  reg r: bits[12] = { b, extend(a, 8) }\n  \
                on rise(clk) {\n    r <- { b, extend(a, 8) }\n  }\n  \
                y = r\n}\n";
    let row = icarus_only_clocked(
        &bin,
        src,
        "bug_66_a2",
        &[("a", 4, 0b1111), ("b", 4, 0b1010)],
        ("y", 12),
        4,
    );
    assert_eq!(row["y"], 2575, "expected y = {{b, extend(a,8)}} = 2575");
}

// ---------------------------------------------------------------------
// Round-8 plan Task 1 (BUG-70, review Part 2.2/GAP-18): BUG-66's own fix
// (above) captures `pre_decl_hoist_pos` once, before `emit_instances` runs
// at all — safe for BUG-66's own three sites (ports/parameters, already
// declared in the header) but NOT for an instance's OWN output wire, which
// `emit_instances` declares INLINE, interleaved with the next instance's
// connection rendering. A hoist raised by instance N's connection reading
// an EARLIER instance's output (`u1.q`) used to be spliced at
// `pre_decl_hoist_pos` — strictly BEFORE `u1`'s own wire, which had only
// just been written a few lines into that same region. `mimz check` OK,
// `mimz compile` exit 0, real Icarus refused ("declaration after use") on
// the instance wire; `assert_hoists_declared_before_use` stayed silent,
// since the out-of-order symbol was an ordinary wire (`u1_q`), not a
// `__mimz_*` hoisted name. Fixed by declaring every instance's output wire
// in its own pre-pass (`declare_instance_outputs`, `module/instances.rs`),
// entirely before `pre_decl_hoist_pos` is captured.
// ---------------------------------------------------------------------

#[test]
fn bug_70_instance_port_hoist_reading_an_earlier_instance_output_matches_icarus() {
    // Review Appendix A.10 / plan Task 1, Construction 1 — NOT one of
    // BUG-66's three repros: a second instance's port connection hoists an
    // expression that reads the FIRST instance's own output.
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[4]\n  \
                let u1 = Sub() { d: { b, extend(a, 8) } }\n  \
                let u2 = Sub() { d: { b, extend(u1.q, 8) } }\n  \
                y = u2.q\n}\n\n\
                module Sub {\n  in d: bits[12]\n  out q: bits[4]\n  q = d[3:0]\n}\n";
    differential_clocked(src, Some("Fuzz"), &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_70_mem_init_hoist_reading_an_instance_output_matches_icarus() {
    // Review Appendix A.11 / plan Task 1, Construction 2 — the same axis
    // through a DIFFERENT one of BUG-66's three render sites, to confirm
    // it is a property of the splice point and not of instances specifically.
    // Same `icarus_only_clocked` oracle as `bug_66_a3` (mimz-sim's own
    // elaborator requires a compile-time-constant `mem` init, unrelated to
    // this bug) and the identical math (`{b, extend(a,8))}` = 2575), since
    // `u1.q` here just forwards `a`'s own low 4 bits unchanged.
    let Some(bin) = support::require_iverilog() else {
        return;
    };
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[12]\n  \
                let u1 = Sub() { d: { b, extend(a, 8) } }\n  \
                mem m: bits[12][4] = { b, extend(u1.q, 8) }\n  \
                y = m[0]\n}\n\n\
                module Sub {\n  in d: bits[12]\n  out q: bits[4]\n  q = d[3:0]\n}\n";
    let row = icarus_only_clocked(
        &bin,
        src,
        "bug_70_mem_init",
        &[("a", 4, 0b1111), ("b", 4, 0b1010)],
        ("y", 12),
        4,
    );
    assert_eq!(
        row["y"], 2575,
        "expected y = {{b, extend(u1.q,8)}} = {{b, extend(a,8)}} = 2575"
    );
}

#[test]
fn bug_70_repeat_unrolled_instance_array_variant_matches_icarus() {
    // Plan Task 1's own "watch out for": a `repeat`-unrolled instance array
    // must have EVERY element's output wire declared by the pre-pass, not
    // just the first — a later, non-repeat instance here connects to
    // `u[1].q` (the array's SECOND element), which only exists under the
    // repeat-generated key `u__1_q` (BUG-53's own naming convention).
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[4]\n  \
                repeat i: 0..2 {\n    let u[i] = Sub() { d: { b, extend(a, 8) } }\n  }\n  \
                let u2 = Sub() { d: { b, extend(u[1].q, 8) } }\n  \
                y = u2.q\n}\n\n\
                module Sub {\n  in d: bits[12]\n  out q: bits[4]\n  q = d[3:0]\n}\n";
    differential_clocked(src, Some("Fuzz"), &[("a", 0b1111), ("b", 0b1010)]);
}

// ---------------------------------------------------------------------
// Round-7 plan Task 4 (BUG-67, review Part 3.2): `render_fn_decl`'s
// `fn_decls` never carried a `fn_ret_decl_key` for any project `fn` —
// `build_decls` (module scope) does, so a nested `fn` call resolved fine
// one level up but was unresolvable (`infer_kind` → `None`) inside
// ANOTHER `fn`'s own body. BUG-28's founding divergence (2575 vs the
// correct Icarus value), reached through a sixth context.
// ---------------------------------------------------------------------

#[test]
fn bug_67_nested_fn_call_in_a_concat_inside_a_fn_body_matches_icarus() {
    let src = "fn inner(x: bits[4]) -> bits[4] { x }\n\
               fn outer(x: bits[4], c: bits[4]) -> bits[12] { { c, extend(inner(x), 8) } }\n\n\
               module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bits[12]\n  \
               y = outer(a, b)\n}\n";
    differential(src, &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_67_extend_of_a_nested_fn_call_inside_a_fn_body_matches_icarus() {
    // A distinct self-determined position from the concat-member test above
    // (a reduction operand, BUG-62①'s own shape one level deeper: a nested
    // `fn` call instead of a bare parameter) — proves the fix isn't
    // coincidentally narrow to the concat-member call site.
    let src = "fn inner(x: bits[4]) -> bits[4] { x }\n\
               fn allset(x: bits[4]) -> bit { &extend(inner(x), 8) }\n\n\
               module Fuzz {\n  in a: bits[4]\n  out y: bit\n  y = allset(a)\n}\n";
    differential(src, &[("a", 0b1111)]);
}

#[test]
fn bug_67_module_scope_nested_fn_call_control_still_matches_icarus() {
    // The module-scope control the plan itself names: `inner(a)` called
    // directly from a MODULE body (not from inside another `fn`) already
    // resolved correctly before this task (`build_decls` — not
    // `render_fn_decl` — installs `fn_ret_decl_key` there) and must stay
    // that way; Task 4 only touches `render_fn_decl`'s OWN map.
    let src = "fn inner(x: bits[4]) -> bits[4] { x }\n\n\
               module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bits[12]\n  \
               y = { b, extend(inner(a), 8) }\n}\n";
    differential(src, &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn bug_66_a3_mem_init_hoist_matches_icarus() {
    let Some(bin) = support::require_iverilog() else {
        return;
    };
    let src = "module Fuzz {\n  clock clk\n  reset rst\n  in a: bits[4]\n  in b: bits[4]\n  \
                out y: bits[12]\n  mem m: bits[12][4] = { b, extend(a, 8) }\n  \
                y = m[0]\n}\n";
    let row = icarus_only_clocked(
        &bin,
        src,
        "bug_66_a3",
        &[("a", 4, 0b1111), ("b", 4, 0b1010)],
        ("y", 12),
        4,
    );
    assert_eq!(row["y"], 2575, "expected y = {{b, extend(a,8)}} = 2575");
}

// ---------------------------------------------------------------------
// Round-7 plan Task 5 (BUG-68, review Part 3.3): `emit_testbench` built
// `cur_decls` (hoisting) AND its own port-declaration/connection loop
// from `dut.items` UNFLATTENED — so any port a `const if`/`sync loop`
// generates was invisible to both, in two distinct symptoms sharing one
// cause: (a) a hoisting `expect` panicked (`hoist_unresolved`, the port's
// `Kind` wasn't in `cur_decls`), (b) a plain `expect`'s testbench had NO
// `wire`/connection for the port at all — `mimz test` reported PASS,
// real Icarus refused to elaborate ("Unable to bind"). Fixed by giving
// `emit_testbench` one flattened item list (`flat_dut_items`,
// `testbench.rs`) and reading it everywhere `dut.items` used to be read
// (decls, the port loop, the clock/reset zero-init loop, and cover
// collection — the review named only the first two, but every one of
// these was the SAME unflattened-view bug, so all were fixed together).
//
// The review's own repro uses a FILE-LEVEL `const if (BIG == 1)` —
// deliberately NOT reused here: `emit_testbench` never folds file- or
// module-level consts into its own `env` at all (a SEPARATE, pre-existing
// limitation this task doesn't touch — confirmed by hand: even a bare
// `bits[W]` port width with no `const if` involved fails to resolve under
// `--emit-testbench` today). A literal `const if (1 == 1)` condition
// isolates BUG-68's own defect (unflattened item visibility) without
// tripping that unrelated gap.
// ---------------------------------------------------------------------

/// Compiles `src` with `--emit-testbench`, builds + runs the FIRST
/// `module <name>;` found in the generated testbench against real Icarus,
/// and returns its stdout. Mirrors `tests/icarus.rs`'s own
/// `compile_example_tb` (private to that file, not reusable here) —
/// this crate's one-off counterpart, since none of the shared `support`
/// helpers drive `--emit-testbench` end to end.
fn compile_and_run_testbench(bin: &std::path::Path, src: &str, tag: &str) -> String {
    let path = std::env::temp_dir().join(format!("mimz_{tag}.mimz"));
    fs::write(&path, src).unwrap();
    let out_v = std::env::temp_dir().join(format!("mimz_{tag}.v"));
    let out_tb = std::env::temp_dir().join(format!("mimz_{tag}_tb.v"));
    let out = support::mimz()
        .arg("compile")
        .arg(&path)
        .arg("-o")
        .arg(&out_v)
        .arg("--emit-testbench")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "`mimz compile --emit-testbench` failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let tb_src = fs::read_to_string(&out_tb).unwrap();
    let name = tb_src
        .lines()
        .find_map(|l| l.strip_prefix("module ")?.strip_suffix(';'))
        .expect("no `module <name>;` line in the emitted testbench");
    let vvp_out = std::env::temp_dir().join(format!("mimz_{tag}.vvp"));
    let build = support::tool(bin, "iverilog")
        .arg("-o")
        .arg(&vvp_out)
        .args(["-s", name])
        .arg(&out_tb)
        .arg(&out_v)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "iverilog failed to build `{name}`:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let sim = support::tool(bin, "vvp")
        .current_dir(std::env::temp_dir())
        .arg(&vvp_out)
        .output()
        .unwrap();
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(&out_v);
    let _ = fs::remove_file(&out_tb);
    let _ = fs::remove_file(&vvp_out);
    // The testbench's own `$dumpfile("{name}.vcd")` writes into `vvp`'s
    // `current_dir` above (`std::env::temp_dir()`), never the repo root —
    // still clean it up so temp doesn't accumulate one per test run.
    let _ = fs::remove_file(std::env::temp_dir().join(format!("{name}.vcd")));
    let stdout = String::from_utf8_lossy(&sim.stdout).to_string();
    assert!(
        sim.status.success(),
        "vvp failed running `{name}`:\n{stdout}"
    );
    stdout
}

#[test]
fn bug_68_const_if_declared_port_is_declared_and_connected_in_the_emitted_testbench() {
    let Some(bin) = support::require_iverilog() else {
        return;
    };
    // The port-declaration half — needs no hoist at all, so it was
    // invisible to every hoist invariant: `mimz test` reported PASS
    // (its own elaborator has no such unflattened view), the emitted
    // testbench had no `wire y`/`.y(y)` at all, and real Icarus refused
    // to elaborate.
    let src = "module Fuzz {\n  in a: bits[4]\n  const if (1 == 1) {\n    out y: bits[4]\n  }\n  \
               y = a\n}\n\n\
               test \"t\" for Fuzz {\n  a = 0b1111\n  expect y == 0b1111\n}\n";
    let stdout = compile_and_run_testbench(&bin, src, "bug_68_plain");
    assert!(
        stdout.contains("PASS") && !stdout.contains("FAIL"),
        "expected PASS, got:\n{stdout}"
    );
}

#[test]
fn bug_68_const_if_declared_port_in_a_hoisting_expect_matches_icarus() {
    // The hoisting half — BUG-62 ⑨'s exact rendering (`(&(y))`, a fallback
    // `hoist_unresolved` used to panic on) reached through a `const if`-
    // generated port instead of a plain module port.
    let Some(bin) = support::require_iverilog() else {
        return;
    };
    let src = "module Fuzz {\n  in a: bits[4]\n  const if (1 == 1) {\n    out y: bits[4]\n  }\n  \
               y = a\n}\n\n\
               test \"t\" for Fuzz {\n  a = 0b1111\n  expect &extend(y, 8) == 0\n}\n";
    let stdout = compile_and_run_testbench(&bin, src, "bug_68_hoist");
    assert!(
        stdout.contains("PASS") && !stdout.contains("FAIL"),
        "expected PASS, got:\n{stdout}"
    );
}

#[test]
fn bug_68_sync_loop_declared_ports_are_declared_and_connected_in_the_emitted_testbench() {
    // `sync loop`'s 4 generated ports (`_start`/`_done`/`_result`/
    // `_running`) are the other half of what `flatten_items` adds —
    // `expand_sync_prims`, not `ConstIf` expansion.
    let Some(bin) = support::require_iverilog() else {
        return;
    };
    let src = "module Search {\n  clock clk\n  reset rst\n  mem m: bits[8][8] = 0\n  \
               in key: bits[8]\n  sync loop find_first on rise(clk) (i: 0..8) -> \
               result: signed[4] = 0 - 1 {\n    if m[i] == key { result <- 0 - 1 }\n  }\n}\n\n\
               test \"t\" for Search {\n  key = 0\n  tick(clk)\n  \
               expect find_first_running == 0\n}\n";
    let stdout = compile_and_run_testbench(&bin, src, "bug_68_sync_loop");
    assert!(
        stdout.contains("PASS") && !stdout.contains("FAIL"),
        "expected PASS, got:\n{stdout}"
    );
}

// ---------------------------------------------------------------------
// Round-7 plan Task 6 (review Part 5): round-6 Task 7 left `if-expr
// then-branch` (`expr.rs:414`) and `if-expr else-branch` (`expr.rs:416`)
// marked as HONEST open coverage gaps in `HOIST_CALL_SITES` — correctly,
// since no differential pinned them. Both sites are load-bearing: with
// `a = 0b1111` (15), `a +% a` wraps to 14 and `a *% a` wraps to 1 at 4
// bits; deleting either branch's hoist lets Verilog self-determine the
// operator at the OUTER `extend(...)`'s 8-bit context instead of the
// `if`'s own 4-bit width, giving the full UNWRAPPED value (30, 225) —
// exactly what these differentials would catch. With both hoists removed
// by hand, the review found the ENTIRE workspace produced exactly one
// failure: `every_hoist_call_site_in_expr_rs_has_a_coverage_entry`,
// because the call-site COUNT changed — no behavioral test noticed two
// load-bearing hoists vanish. These four close that.
// ---------------------------------------------------------------------

#[test]
fn task6_if_expr_then_branch_wrap_add_operand_inside_extend_matches_icarus() {
    let src = "module Fuzz {\n  in c: bit\n  in a: bits[4]\n  out y: bits[8]\n  \
               y = extend((if c { a +% a } else { a }), 8)\n}\n";
    differential(src, &[("c", 1), ("a", 0b1111)]);
}

#[test]
fn task6_if_expr_then_branch_wrap_mul_operand_inside_extend_matches_icarus() {
    let src = "module Fuzz {\n  in c: bit\n  in a: bits[4]\n  out y: bits[8]\n  \
               y = extend((if c { a *% a } else { a }), 8)\n}\n";
    differential(src, &[("c", 1), ("a", 0b1111)]);
}

#[test]
fn task6_if_expr_else_branch_wrap_add_operand_inside_extend_matches_icarus() {
    let src = "module Fuzz {\n  in c: bit\n  in a: bits[4]\n  out y: bits[8]\n  \
               y = extend((if c { a } else { a +% a }), 8)\n}\n";
    differential(src, &[("c", 0), ("a", 0b1111)]);
}

#[test]
fn task6_if_expr_else_branch_wrap_mul_operand_inside_extend_matches_icarus() {
    let src = "module Fuzz {\n  in c: bit\n  in a: bits[4]\n  out y: bits[8]\n  \
               y = extend((if c { a } else { a *% a }), 8)\n}\n";
    differential(src, &[("c", 0), ("a", 0b1111)]);
}

// ---------------------------------------------------------------------
// Round-7 plan Task 7 (review Part 9.2): three `HOIST_CALL_SITES` entries
// (`replicate member`, `nor operand`, `xnor operand`) cited a test that
// never reached the position — each hoisted somewhere ELSE instead
// (`replicate member`'s citation hoisted at `concat member`; `nor
// operand`'s citation was a `|` unary reduction, not the `nor` builtin;
// `xnor operand`'s citation was its own no-mismatch control, proving
// nothing). Confirmed each of the three below by reading the emission,
// not just running it — see the `HOIST_CALL_SITES` entries' own updated
// citations for the exact `.v` text each produces.
// ---------------------------------------------------------------------

#[test]
fn task7_replicate_member_hoists_a_composite_body_matches_icarus() {
    // `{2{extend(a, 8)}}` — the REPLICATION BODY itself (`extend(a, 8)`,
    // 4-bit `a` needing to render at 8) is the mismatch, isolated from
    // `extend`'s own argument position by nesting the replication INSIDE
    // a concat instead of inside `extend`'s argument (the wrong citation's
    // own shape, `extend({2{a}}, 12)`, which hoists at `concat member`
    // because the MISMATCH there is the whole replication vs `extend`'s
    // target width, not the replication's own body). Emission confirmed
    // by hand: exactly one hoisted wire, `{c, {2{__mimz_sub_1}}}` — the
    // replication's OWN total width (2×8=16) already matches `extend`'s
    // target once the body is hoisted, so no second, concat-member-level
    // hoist fires on top.
    let src = "module Fuzz {\n  in a: bits[4]\n  in c: bits[4]\n  out y: bits[20]\n  \
               y = { c, {2{extend(a, 8)}} }\n}\n";
    differential(src, &[("a", 0b1010), ("c", 0b0101)]);
}

#[test]
fn task7_nor_operand_of_an_extend_matches_icarus() {
    // The `nor(...)` BUILTIN's own argument — distinct from `bug_60_or_
    // reduction_of_a_negated_extend_matches_icarus`'s `|(~extend(a, 8))`,
    // which is a `|` unary reduction over a negated extend and hoists at
    // `expr.rs:589` (`Unary reduction operand`), never reaching `Builtin::
    // Nor`'s own call site at all.
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bits[5]\n  \
               y = { b, nor(extend(a, 8)) }\n}\n";
    differential(src, &[("a", 0b1111), ("b", 0b1010)]);
}

#[test]
fn task7_xnor_operand_of_an_extend_matches_icarus() {
    // `matrix_xnor_in_concat_matches_icarus`'s own source is `xnor(a)` —
    // `a` bare, no mismatch, nothing to hoist. This forces a composite
    // (`extend(a, 8)`) into the SAME position instead.
    let src = "module Fuzz {\n  in a: bits[4]\n  in b: bits[4]\n  out y: bits[5]\n  \
               y = { b, xnor(extend(a, 8)) }\n}\n";
    differential(src, &[("a", 0b1111), ("b", 0b1010)]);
}
