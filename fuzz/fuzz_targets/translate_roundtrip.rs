#![no_main]
//! Fuzz the `translate` surface — keyword reskin, `--romanize-names`, and the
//! sidecar name-map restore — for crash- AND round-trip-safety on arbitrary
//! input. libFuzzer turns any panic into a finding; the asserts below add two
//! correctness invariants the unit/integration suites only check over the fixed
//! corpus:
//!
//!   1. **Reskin never produces unlexable output.** Translating to any flavor,
//!      and romanizing identifiers, must yield source that re-lexes — the
//!      boundary guard (`src/translate.rs::push_guarded`) keeps a script-changing
//!      re-emit (Tamil keyword/name -> ASCII) from gluing onto an adjacent
//!      number/identifier (the 2026-06-15 audit's `42தொகுதி` -> `42module` bug).
//!   2. **romanize -> restore is token-equivalent.** Restoring the name-map must
//!      reproduce the plain (keyword-only) translation up to the separator spaces
//!      the guard may insert, so compare whitespace-insensitively.
//!
//! This is the coverage gap the audit flagged: the other targets drive
//! lex/parse/eval and the pretty-printer, none the `translate` byte-walk.
use libfuzzer_sys::fuzz_target;
use unicode_normalization::UnicodeNormalization;

use mimz::lexer::token::Flavor;
use mimz::translate::{restore_with_map, romanize_with_map, translate};

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    // Mirror `project::read_source`: the lexer expects NFC-normalized input.
    let src: String = src.nfc().collect();

    let norm = |s: &str| s.split_whitespace().collect::<String>();

    for to in [Flavor::English, Flavor::Tanglish, Flavor::Tamil] {
        // Keyword-only reskin: a clean `Err` (does not lex) is fine; a panic is
        // not. When it succeeds, the output must itself re-lex.
        let Ok(plain) = translate(&src, to) else {
            continue;
        };
        if mimz::lexer::lex(&plain).is_err() {
            panic!("keyword reskin produced unlexable output (to={to:?}):\n{plain}");
        }

        // Romanize identifiers + capture the map; the output must re-lex, and
        // restoring via the map must be token-equivalent to the plain reskin.
        let Ok((romanized, map)) = romanize_with_map(&src, to) else {
            continue;
        };
        if mimz::lexer::lex(&romanized).is_err() {
            panic!("romanized output does not lex (to={to:?}):\n{romanized}");
        }
        let restored = restore_with_map(&romanized, to, &map)
            .unwrap_or_else(|_| panic!("restore failed (to={to:?}) for:\n{src}"));
        assert_eq!(
            norm(&restored),
            norm(&plain),
            "romanize -> restore not token-equivalent (to={to:?}) for:\n{src}"
        );
    }
});
