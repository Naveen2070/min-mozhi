//! Pure compiler pipeline: lexer, parser, AST, checker, Verilog emitter, and
//! supporting tooling (translate/pretty/explain/analysis/stdlib/morph). Zero
//! optional dependencies and no filesystem/OS access — the pure half of the
//! workspace split (`docs/plan/workspace-split.local.md`); `mimz-sim` and the
//! root shell crate build on top of this.
#![forbid(unsafe_code)]

/// Compile-time unroll cap for `repeat` — the ceiling on how many iterations
/// a single `repeat` may generate, guarding against runaway hardware
/// generation from a pathological or malicious bound.
pub const REPEAT_BUDGET: i128 = 4096;

/// NFC-normalize source text so combining-mark sequences (e.g. decomposed
/// Tamil vowel signs) compare equal to their precomposed form before lexing.
pub fn nfc_normalize(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfc().collect()
}

pub mod analysis;
pub mod ast;
pub mod checker;
pub mod diag;
pub mod emit_verilog;
pub mod explain;
pub mod lexer;
pub mod lint;
pub mod morph;
pub mod parser;
pub mod pretty;
pub mod project;
pub mod span;
pub mod stdlib;
pub mod translate;
pub mod version;
pub mod width_rules;

#[cfg(test)]
mod tests {
    use super::nfc_normalize;

    #[test]
    fn nfc_normalize_composes_decomposed_forms() {
        // "e" + combining acute accent (U+0301) decomposed vs precomposed "é" (U+00E9).
        let decomposed = "e\u{0301}";
        let precomposed = "\u{00E9}";
        assert_ne!(decomposed, precomposed);
        assert_eq!(nfc_normalize(decomposed), precomposed);
    }
}
