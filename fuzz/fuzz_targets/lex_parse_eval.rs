#![no_main]
//! Fuzz the full untrusted-input path: any byte string must lex/parse/eval to a
//! value or a clean `Diag`/`Err` — never panic, abort, or hang. libFuzzer turns
//! any panic/abort/timeout into a finding, so this asserts the audit's core
//! guarantee by construction. SEC-1 (stack overflow) is capped by the parser's
//! `MAX_DEPTH`/E1113; SEC-2 (const overflow) by the checker's checked arithmetic
//! that `sim::comb` now delegates to.
use libfuzzer_sys::fuzz_target;
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
    let inputs: BTreeMap<String, u128> = BTreeMap::new();
    let params: BTreeMap<String, i128> = BTreeMap::new();
    let _ = mimz::sim::comb::eval_outputs(&file, None, &inputs, &params);
});
