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

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::diag::Diag;
use crate::emit_verilog::romanize;
use crate::lexer::keywords::TABLE;
use crate::lexer::lex;
use crate::lexer::token::{Flavor, TokKind, Token};

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

/// Options for [`translate_opts`].
#[derive(Clone, Copy, Default)]
pub struct TranslateOpts {
    /// Romanize non-ASCII (Tamil) identifiers to readable Latin, using the same
    /// scheme the Verilog emitter uses (`கணக்கி` -> `kannakki`). This is
    /// **one-way** — romanization cannot be inverted by rule — so the lossless
    /// round-trip contract only holds with it OFF (the default). See spec/04.
    pub romanize_names: bool,
}

/// Reskin `src`'s keywords into `target`, preserving every other byte verbatim
/// (lossless). Convenience wrapper over [`translate_opts`] with default options.
pub fn translate(src: &str, target: Flavor) -> Result<String, Vec<Diag>> {
    translate_opts(src, target, TranslateOpts::default())
}

/// Reskin `src`'s keywords into `target`. With `opts.romanize_names`, also
/// rewrite Tamil identifiers to Latin (one-way). Fails only if the source does
/// not lex, returning the lexer's diagnostics (translation runs before any
/// semantic check — it is pure surface rewriting).
pub fn translate_opts(src: &str, target: Flavor, opts: TranslateOpts) -> Result<String, Vec<Diag>> {
    if opts.romanize_names {
        return romanize_with_map(src, target).map(|(out, _map)| out);
    }
    let tokens = lex(src)?;
    Ok(reskin(src, &tokens, target, &|_| None))
}

/// A per-file identifier name-map — the sidecar that makes `--romanize-names`
/// reversible. `names` maps the **romanized** spelling (as it appears in the
/// translated source) back to the **original Tamil** name, capturing the
/// `_2`/`_3` uniquing. Written next to a `--romanize-names` output, read back by
/// `restore_with_map` (CLI `--names-map`). The `BTreeMap` keeps the JSON sorted
/// and deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameMap {
    /// Format version (currently 1) — lets the reader reject a future format.
    pub version: u32,
    /// romanized (Latin) -> original Tamil.
    pub names: BTreeMap<String, String>,
}

impl NameMap {
    /// The current format version.
    pub const VERSION: u32 = 1;
}

/// Reskin keywords to `target` AND romanize Tamil identifiers to Latin, returning
/// the translated source together with the [`NameMap`] needed to reverse it.
/// One-way on its own — pair the map with [`restore_with_map`] for a lossless
/// round-trip.
pub fn romanize_with_map(src: &str, target: Flavor) -> Result<(String, NameMap), Vec<Diag>> {
    let tokens = lex(src)?;
    let forward = build_rename_map(&tokens); // Tamil -> Latin
    let out = reskin(src, &tokens, target, &|name| {
        if name.is_ascii() {
            None
        } else {
            forward.get(name).cloned()
        }
    });
    // Invert for the sidecar: the reverse pass looks up by the Latin spelling.
    let names = forward
        .into_iter()
        .map(|(tamil, latin)| (latin, tamil))
        .collect();
    let map = NameMap {
        version: NameMap::VERSION,
        names,
    };
    Ok((out, map))
}

/// Reskin keywords to `target` AND restore original Tamil identifiers from a
/// [`NameMap`] (the inverse of [`romanize_with_map`]). An identifier is replaced
/// iff it appears as a key in `map.names` — and the forward pass's uniquing
/// guarantees those keys never collide with a genuine ASCII identifier, so the
/// substitution is unambiguous.
pub fn restore_with_map(src: &str, target: Flavor, map: &NameMap) -> Result<String, Vec<Diag>> {
    let tokens = lex(src)?;
    Ok(reskin(src, &tokens, target, &|name| {
        map.names.get(name).cloned()
    }))
}

/// The shared span-walk: copy `src` verbatim, swap keyword lexemes to `target`'s
/// canonical spelling, and replace an identifier when `sub` returns `Some`.
fn reskin(
    src: &str,
    tokens: &[Token],
    target: Flavor,
    sub: &dyn Fn(&str) -> Option<String>,
) -> String {
    let mut out = String::with_capacity(src.len() + src.len() / 8);
    let mut pos = 0usize;
    for t in tokens {
        // Defensive clamp: token spans are ordered and non-overlapping, but a
        // synthetic newline/EOF token may be zero-width at the current cursor.
        // Clamping keeps the slice indices valid (never panics) and emits each
        // source byte exactly once.
        let start = t.span.start.max(pos);
        let end = t.span.end.max(start);
        out.push_str(&src[pos..start]); // verbatim gap: whitespace + comments
        match &t.kind {
            TokKind::Kw(kw) => out.push_str(TABLE.canonical(*kw, target)),
            TokKind::Ident(name) => match sub(name) {
                Some(replacement) => out.push_str(&replacement),
                None => out.push_str(&src[start..end]),
            },
            _ => out.push_str(&src[start..end]),
        }
        pos = end;
    }
    out.push_str(&src[pos..]); // trailing bytes after the last token
    out
}

/// Build the `Tamil name -> romanized Latin` map for `--romanize-names`. Mirrors
/// the emitter's uniquing (`src/emit_verilog/translit.rs`): a romanization that
/// would land on an existing ASCII identifier, a keyword spelling, or a reserved
/// word gets `_2`, `_3`, … in first-seen (source) order — so the rewrite never
/// shadows a real name or re-lexes AS a keyword.
fn build_rename_map(tokens: &[Token]) -> HashMap<String, String> {
    let mut used: HashSet<String> = HashSet::new();
    for t in tokens {
        if let TokKind::Ident(n) = &t.kind {
            if n.is_ascii() {
                used.insert(n.clone());
            }
        }
    }
    let mut map: HashMap<String, String> = HashMap::new();
    for t in tokens {
        if let TokKind::Ident(n) = &t.kind {
            if n.is_ascii() || map.contains_key(n) {
                continue;
            }
            let base = romanize(n);
            let mut candidate = base.clone();
            let mut k = 2;
            while used.contains(&candidate)
                || TABLE.lookup(&candidate).is_some()
                || TABLE.is_reserved(&candidate)
            {
                candidate = format!("{base}_{k}");
                k += 1;
            }
            used.insert(candidate.clone());
            map.insert(n.clone(), candidate);
        }
    }
    map
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

    #[test]
    fn romanize_names_rewrites_tamil_identifiers_only_when_asked() {
        let src = "module M {\n  reg கணக்கு: bit = 0\n}\n";
        let on = translate_opts(
            src,
            Flavor::English,
            TranslateOpts {
                romanize_names: true,
            },
        )
        .unwrap();
        assert!(on.contains("reg kannakku: bit = 0"), "got: {on}");
        assert!(!on.contains("கணக்கு"), "Tamil name should be gone: {on}");
        // Default leaves the Tamil name untouched (lossless).
        assert!(translate(src, Flavor::English).unwrap().contains("கணக்கு"));
    }

    #[test]
    fn romanize_names_uniques_against_an_existing_ascii_name() {
        // `கணக்கு` romanizes to `kannakku`; an ASCII `kannakku` already present
        // forces the rewrite to `kannakku_2` so the two never merge.
        let src = "module M {\n  reg kannakku: bit = 0\n  reg கணக்கு: bit = 0\n}\n";
        let out = translate_opts(
            src,
            Flavor::English,
            TranslateOpts {
                romanize_names: true,
            },
        )
        .unwrap();
        assert!(out.contains("reg kannakku: bit = 0"));
        assert!(out.contains("reg kannakku_2: bit = 0"), "got: {out}");
    }

    #[test]
    fn romanize_with_map_returns_the_inverse_map() {
        let src = "module M {\n  reg கணக்கு: bit = 0\n}\n";
        let (out, map) = romanize_with_map(src, Flavor::English).unwrap();
        assert!(out.contains("reg kannakku: bit = 0"));
        assert_eq!(map.version, NameMap::VERSION);
        // Keyed by the Latin spelling (what the reverse pass sees), value Tamil.
        assert_eq!(map.names.get("kannakku").map(String::as_str), Some("கணக்கு"));
    }

    #[test]
    fn restore_with_map_inverts_romanize() {
        // Tamil -> (romanize) Latin -> (restore) Tamil reproduces the canonical
        // Tamil source byte-for-byte. Anchor on the canonical form so alias
        // normalization is not mistaken for a round-trip failure. (Identifiers
        // here — count/value — are NOT keyword spellings, so they lex as names.)
        let src = "module M {\n  reg கணக்கு: bit = 0\n  out மதிப்பு: bit\n  மதிப்பு = கணக்கு\n}\n";
        let canonical = translate(src, Flavor::Tamil).unwrap();
        let (romanized, map) = romanize_with_map(&canonical, Flavor::Tanglish).unwrap();
        let restored = restore_with_map(&romanized, Flavor::Tamil, &map).unwrap();
        assert_eq!(restored, canonical);
    }

    #[test]
    fn name_map_json_round_trips() {
        let mut names = BTreeMap::new();
        names.insert("kannakku".to_string(), "கணக்கு".to_string());
        let map = NameMap {
            version: NameMap::VERSION,
            names,
        };
        let json = serde_json::to_string(&map).unwrap();
        let back: NameMap = serde_json::from_str(&json).unwrap();
        assert_eq!(map, back);
    }
}
