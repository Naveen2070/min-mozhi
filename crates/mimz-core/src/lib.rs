#![forbid(unsafe_code)]

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
