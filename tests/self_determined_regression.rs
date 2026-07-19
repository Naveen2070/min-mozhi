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
use mimz::{checker, diag, lexer, parser};

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
    let outputs = comb::eval_outputs(
        std::slice::from_ref(&file),
        None,
        &input_map,
        &BTreeMap::new(),
    )
    .unwrap_or_else(|e| panic!("our kernel rejected this program:\n{src}\n{e}"));
    let kernel_row: BTreeMap<String, u128> =
        outputs.into_iter().map(|o| (o.name, o.value)).collect();

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
