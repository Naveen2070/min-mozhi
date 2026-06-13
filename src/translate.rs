//! `mimz translate --to <flavor>` — reskin a file's KEYWORDS into another
//! language flavor (english / tanglish / tamil), losslessly.
//!
//! This is the flavor-only half of spec/04's `translate`; the natural Tamil
//! WORD-ORDER half (`--order thamizh`, which reorders the AST) is Phase 1.8.
//! Here only keyword TOKENS change. The lexer already maps every spelling to a
//! flavor-blind [`Kw`](crate::lexer::token::Kw), so we re-lex, copy the source
//! verbatim, and swap only the keyword lexemes for the target column's
//! canonical spelling. Comments, layout, identifiers, and numbers are left
//! untouched — lossless by construction — and any accepted alias normalizes to
//! its canonical spelling along the way.
//!
//! Because the four `examples/<flavor>/` folders are byte-identical
//! keyword-swaps (RULES R9), they double as the test oracle: translating one
//! flavor of a base example must reproduce another flavor's file exactly, and
//! A→B→A round-trips to identity (`tests/translate.rs`).
//!
//! Down-payment on Phase 1.8 translate and `mimz fmt`. NOTE: tanglish/tamil
//! targets ride the DRAFT keyword columns until native-speaker review closes
//! (keywords.toml header).

use crate::diag::Diag;
use crate::lexer::keywords::TABLE;
use crate::lexer::lex;
use crate::lexer::token::{Flavor, TokKind};

/// Parse a `--to` flavor argument (the three column names, plus short aliases).
/// Returns `None` for anything else, so the CLI can list the valid choices.
pub fn parse_flavor(s: &str) -> Option<Flavor> {
    match s.trim().to_ascii_lowercase().as_str() {
        "english" | "en" => Some(Flavor::English),
        "tanglish" | "tl" => Some(Flavor::Tanglish),
        "tamil" | "ta" => Some(Flavor::Tamil),
        _ => None,
    }
}

/// The flavor's column name, for messages.
pub fn flavor_name(f: Flavor) -> &'static str {
    match f {
        Flavor::English => "english",
        Flavor::Tanglish => "tanglish",
        Flavor::Tamil => "tamil",
    }
}

/// Reskin `src`'s keywords into `target`, preserving every other byte verbatim.
/// Fails only if the source does not lex, returning the lexer's diagnostics
/// (translation runs before any semantic check — it is pure surface rewriting).
pub fn translate(src: &str, target: Flavor) -> Result<String, Vec<Diag>> {
    let tokens = lex(src)?;
    let mut out = String::with_capacity(src.len() + src.len() / 8);
    let mut pos = 0usize;
    for t in &tokens {
        // Defensive clamp: token spans are ordered and non-overlapping, but a
        // synthetic newline/EOF token may be zero-width at the current cursor.
        // Clamping keeps the slice indices valid (never panics) and emits each
        // source byte exactly once.
        let start = t.span.start.max(pos);
        let end = t.span.end.max(start);
        out.push_str(&src[pos..start]); // verbatim gap: whitespace + comments
        match &t.kind {
            TokKind::Kw(kw) => out.push_str(TABLE.canonical(*kw, target)),
            _ => out.push_str(&src[start..end]),
        }
        pos = end;
    }
    out.push_str(&src[pos..]); // trailing bytes after the last token
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_flavor_accepts_the_three_columns() {
        assert_eq!(parse_flavor("english"), Some(Flavor::English));
        assert_eq!(parse_flavor("TAMIL"), Some(Flavor::Tamil));
        assert_eq!(parse_flavor(" tanglish "), Some(Flavor::Tanglish));
        assert_eq!(parse_flavor("klingon"), None);
    }

    #[test]
    fn reskins_keywords_keeps_everything_else() {
        // `module`/`wire` are keywords; identifiers, `:`, layout, the comment,
        // and the number all survive byte-for-byte.
        let src = "module M {  // a note\n  wire w: bits[8]\n  w = 42\n}\n";
        let out = translate(src, Flavor::Tanglish).unwrap();
        assert!(out.contains("thoguthi M {  // a note"));
        assert!(out.contains("kambi w: bits[8]"));
        assert!(out.contains("w = 42"));
        // Non-keyword text is identical: same comment, same number, same braces.
        assert_eq!(out.matches("// a note").count(), 1);
    }

    #[test]
    fn translating_to_the_same_flavor_is_identity_for_canonical_input() {
        let src = "module M {\n  in a: bit\n  out y: bit\n  y = a\n}\n";
        assert_eq!(translate(src, Flavor::English).unwrap(), src);
    }
}
