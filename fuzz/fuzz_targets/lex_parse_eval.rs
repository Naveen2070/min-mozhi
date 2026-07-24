#![no_main]
//! Fuzz the full untrusted-input path: any byte string must lex/parse/eval to a
//! value or a clean `Diag`/`Err` — never panic, abort, or hang. libFuzzer turns
//! any panic/abort/timeout into a finding, so this asserts the audit's core
//! guarantee by construction. SEC-1 (stack overflow) is capped by the parser's
//! `MAX_DEPTH`/E1113; SEC-2 (const overflow) by the checker's checked arithmetic
//! that `sim::comb` now delegates to.
//!
//! Two eval passes: empty inputs (the constant-folding / width / slice-bound
//! path) and AST-derived inputs (a value per real input port, so the evaluator
//! walks the actual datapath, not just constant folding).
use libfuzzer_sys::fuzz_target;
use mimz::sim::value::Bits;
use std::collections::BTreeMap;
use unicode_normalization::UnicodeNormalization;

fuzz_target!(|data: &[u8]| {
    // The compiler ingests UTF-8 source; reject non-UTF-8 the way the CLI would.
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    // Mirror `project::read_source`: the lexer expects NFC-normalized input.
    let src: String = src.nfc().collect();

    let Ok(tokens) = mimz::lexer::lex(&src) else {
        return;
    };
    let Ok(file) = mimz::parser::parse(tokens) else {
        return;
    };

    // Empty inputs/params still drive constant evaluation of widths, slice
    // bounds, and indices — the SEC-2 path. A clean `Err` is fine; a panic is
    // not, which is exactly what the fuzzer is here to catch.
    let params: BTreeMap<String, i128> = BTreeMap::new();
    let _ = mimz::sim::comb::eval_outputs(std::slice::from_ref(&file), None, &BTreeMap::new(), &params);

    // Runtime path: feed each input port a value derived from the bytes so the
    // evaluator exercises the real datapath. Use the first module with inputs.
    use mimz::ast::{Dir, ModuleItem, TopItem};
    for item in &file.items {
        let TopItem::Module(m) = item else {
            continue;
        };
        let mut inputs: BTreeMap<String, Bits> = BTreeMap::new();
        for (i, mi) in m.items.iter().enumerate() {
            if let ModuleItem::Port {
                dir: Dir::In, name, ..
            } = mi
            {
                // Deterministic pseudo-value from the input bytes + port index.
                let seed = data
                    .iter()
                    .fold(i as u128, |a, &b| a.wrapping_mul(31).wrapping_add(b as u128));
                inputs.insert(name.name.clone(), Bits::Small(seed));
            }
        }
        if !inputs.is_empty() {
            let _ = mimz::sim::comb::eval_outputs(std::slice::from_ref(&file), Some(&m.name.name), &inputs, &params);

            // Edge-case passes: re-evaluate the same module with dangerous input
            // values that triggered Finding A (Shl `as u32` truncation) and other
            // truncation-prone boundaries. Safe values mask to width; a clean
            // Err is fine (e.g. the module may not accept all-ones on every port),
            // but a panic is not — exactly what the fuzzer catches.
            let edge_cases: [u128; 8] = [
                0,
                1,
                u128::MAX,
                1u128 << 32,   // bit 32 — the `as u32` truncation threshold
                1u128 << 63,   // bit 63 — upper half of u128
                1u128 << 127,  // bit 127 — max valid bit
                (1u128 << 126) - 1,  // just below the 127-bit boundary
                (1u128 << 64) - 1,   // 64-bit all-ones
            ];
            for &edge in &edge_cases {
                let edge_inputs: BTreeMap<String, Bits> = inputs
                    .keys()
                    .map(|k| (k.clone(), Bits::Small(edge)))
                    .collect();
                let _ = mimz::sim::comb::eval_outputs(
                    std::slice::from_ref(&file),
                    Some(&m.name.name),
                    &edge_inputs,
                    &params,
                );
            }
            break;
        }
    }
});
