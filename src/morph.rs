//! Error-language plumbing (Phase 1.8, `spec/04` §5): pick WHICH flavor an
//! error is written in, and inflect interpolated identifiers with Tamil case
//! suffixes so the sentence reads as Tamil, not transliterated English.
//!
//! Two concerns, one module — both answer "what language is this error in?":
//!
//! 1. **Selection** — [`majority_flavor`] counts the keyword flavors a file
//!    actually uses; [`effective_lang`] lets a `--lang` flag override that.
//!    The rule is spec/03's: "errors are emitted in the flavor the file
//!    predominantly uses (`--lang` overrides)."
//! 2. **Inflection** — [`inflect`] attaches one of the four case suffixes
//!    (வேற்றுமை உருபுகள் -ஐ/-க்கு/-இல்/-ஆல், data in `case_suffixes.toml`) to an
//!    identifier. This is "a suffix lookup table plus sandhi-joining rules …
//!    not NLP" (spec/04 §5): error TEMPLATES are authored once per language by
//!    humans; the helper only inflects the names dropped into them.
//!
//! **Additive, English-fallback.** Today every diagnostic is a hardcoded
//! English string at its `self.err()` call site. This module does NOT touch
//! those — [`localized_msg`] looks up a localized template for a diagnostic's
//! E-code and, only if one exists for the chosen flavor, returns it (running
//! interpolated names through [`inflect`]); otherwise the renderer keeps the
//! English `msg` verbatim. So with the (currently stub) catalog, output is
//! byte-identical to before — the plumbing is inert until real content lands.
//!
//! **Panel-gated content (decision C3).** The full Tamil + Tanglish error
//! catalog and the real sandhi rules need the native-speaker panel. What is
//! committed here is the *mechanism*; the `MESSAGES` catalog holds ONE worked
//! stub so the whole path (select → catalog → inflect → render) runs end-to-end.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::diag::Diag;
use crate::lexer::token::{Flavor, TokKind, Token};

// ---- Selection ----------------------------------------------------------

/// The flavor a file predominantly uses, by counting its keyword tokens
/// (only keywords carry a [`Flavor`]). Ties and keyword-free files fall back to
/// [`Flavor::English`] — the always-present catalog (spec/03: "the flavor the
/// file predominantly uses"). Order of preference on a tie is English →
/// Tanglish → Tamil, matching the keyword-table column order.
pub fn majority_flavor(tokens: &[Token]) -> Flavor {
    let mut en = 0usize;
    let mut tl = 0usize;
    let mut ta = 0usize;
    for t in tokens {
        if matches!(t.kind, TokKind::Kw(_)) {
            match t.flavor {
                Some(Flavor::English) => en += 1,
                Some(Flavor::Tanglish) => tl += 1,
                Some(Flavor::Tamil) => ta += 1,
                None => {}
            }
        }
    }
    // `>` (not `>=`) keeps the earlier column winning a tie.
    if ta > en && ta > tl {
        Flavor::Tamil
    } else if tl > en && tl >= ta {
        Flavor::Tanglish
    } else {
        Flavor::English
    }
}

/// Parse a `--lang` argument (the three flavor names + short aliases), reusing
/// the same parser `--to` uses so the spellings never drift apart.
pub fn parse_lang(s: &str) -> Option<Flavor> {
    crate::translate::parse_flavor(s)
}

/// The effective error language: an explicit `--lang` choice if present, else
/// the file's [`majority_flavor`]. The single source of truth for "which
/// flavor do errors render in".
pub fn effective_lang(cli: Option<Flavor>, tokens: &[Token]) -> Flavor {
    cli.unwrap_or_else(|| majority_flavor(tokens))
}

/// The distinct keyword flavors that appear in a token stream, in column order
/// (English → Tanglish → Tamil). `mimz fmt --strict` uses this to flag a file
/// that mixes flavors (mixing stays legal — spec/03 — it is the learning path).
pub fn flavors_used(tokens: &[Token]) -> Vec<Flavor> {
    [Flavor::English, Flavor::Tanglish, Flavor::Tamil]
        .into_iter()
        .filter(|&f| {
            tokens
                .iter()
                .any(|t| matches!(t.kind, TokKind::Kw(_)) && t.flavor == Some(f))
        })
        .collect()
}

// ---- Morphology (case-suffix inflection) --------------------------------

/// The four Tamil grammatical cases the error catalog inflects names into
/// (வேற்றுமை, spec/04 §5). Suffix forms live in `case_suffixes.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    /// -ஐ — the object of the action (`widen 'sum'` → `'sum'-ஐ அகலமாக்கவும்`).
    Accusative,
    /// -க்கு — "to / for" the name.
    Dative,
    /// -இல் — "in / at" the name.
    Locative,
    /// -ஆல் — "by / with" the name.
    Instrumental,
}

const SUFFIXES_TOML: &str = include_str!("../case_suffixes.toml");

#[derive(Deserialize)]
struct SuffixFile {
    suffix: HashMap<String, SuffixForms>,
}

#[derive(Deserialize)]
struct SuffixForms {
    tamil: String,
    tanglish: String,
}

/// Suffix table, parsed once from the embedded TOML. Panics at startup (not at
/// some later lookup) if a case row is missing — a stub table that cannot
/// inflect must be impossible to ship, exactly like the keyword table.
struct SuffixTable {
    by_case: HashMap<&'static str, SuffixForms>,
}

static SUFFIXES: LazyLock<SuffixTable> = LazyLock::new(|| {
    let file: SuffixFile =
        toml::from_str(SUFFIXES_TOML).expect("case_suffixes.toml is malformed — fix the table");
    let mut by_case = HashMap::new();
    for key in ["accusative", "dative", "locative", "instrumental"] {
        let forms = file
            .suffix
            .get(key)
            .unwrap_or_else(|| panic!("case_suffixes.toml is missing `[suffix.{key}]`"));
        by_case.insert(
            key,
            SuffixForms {
                tamil: forms.tamil.clone(),
                tanglish: forms.tanglish.clone(),
            },
        );
    }
    SuffixTable { by_case }
});

impl Case {
    fn key(self) -> &'static str {
        match self {
            Case::Accusative => "accusative",
            Case::Dative => "dative",
            Case::Locative => "locative",
            Case::Instrumental => "instrumental",
        }
    }

    /// This case's suffix in `flavor` (Tamil script or Tanglish romanization).
    /// English has no case suffix, so it returns "" — English never goes
    /// through the localized catalog anyway.
    fn suffix(self, flavor: Flavor) -> &'static str {
        let forms = &SUFFIXES.by_case[self.key()];
        match flavor {
            Flavor::Tamil => &forms.tamil,
            Flavor::Tanglish => &forms.tanglish,
            Flavor::English => "",
        }
    }
}

/// Attach `case`'s suffix to `name` in `flavor`.
///
/// PROVISIONAL sandhi — full Tamil euphonic joining waits on the native-speaker
/// panel (decision C3). The committed rule is deliberately minimal and matches
/// the spec/04 §5 example (`'sum'-ஐ`): a Latin-script identifier takes a hyphen
/// before the suffix; a Tamil-script stem joins directly. Tanglish (already
/// romanized) always hyphenates. English returns the bare name.
pub fn inflect(name: &str, case: Case, flavor: Flavor) -> String {
    let suffix = case.suffix(flavor);
    if suffix.is_empty() {
        return name.to_string();
    }
    let latin_stem = name
        .chars()
        .last()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
    if flavor == Flavor::Tanglish || latin_stem {
        format!("{name}-{suffix}")
    } else {
        format!("{name}{suffix}")
    }
}

// ---- Localized catalog (stub) + render hook -----------------------------

/// One localized message template, keyed by E-code + flavor. The template is
/// plain text with interpolation tokens filled by [`fill`]:
/// `{name}` (bare identifier) and `{name.acc|dat|loc|inst}` (inflected).
struct Localized {
    code: &'static str,
    flavor: Flavor,
    template: &'static str,
}

/// The localized error catalog.
///
/// STUB — ONE worked shape (E0501, "more than one driver") in Tamil and
/// Tanglish, so the select → catalog → inflect → render path is real and
/// tested. The full ~10-shape catalog is authored by the native-speaker panel
/// (decision C3); every other E-code falls back to its English `msg`.
const MESSAGES: &[Localized] = &[
    Localized {
        code: "E0501",
        flavor: Flavor::Tamil,
        template: "{name.dat} ஒன்றுக்கு மேற்பட்ட இயக்கிகள் உள்ளன",
    },
    Localized {
        code: "E0501",
        flavor: Flavor::Tanglish,
        template: "{name.dat} oru iyakkikku mael ulladhu",
    },
];

/// Look up the localized template for a code in a flavor, if the catalog has
/// one. English always returns `None` (it is the verbatim fallback).
fn localized(code: &str, flavor: Flavor) -> Option<&'static str> {
    if flavor == Flavor::English {
        return None;
    }
    MESSAGES
        .iter()
        .find(|m| m.code == code && m.flavor == flavor)
        .map(|m| m.template)
}

/// Fill a template's interpolation tokens with `name`, inflected per token.
fn fill(template: &str, name: &str, flavor: Flavor) -> String {
    template
        .replace("{name.acc}", &inflect(name, Case::Accusative, flavor))
        .replace("{name.dat}", &inflect(name, Case::Dative, flavor))
        .replace("{name.loc}", &inflect(name, Case::Locative, flavor))
        .replace("{name.inst}", &inflect(name, Case::Instrumental, flavor))
        .replace("{name}", name)
}

/// The localized rendering of a diagnostic's message in `flavor`, or `None` to
/// fall back to the English `Diag.msg`. The interpolated identifier is the text
/// the span underlines — the same slice the caret points at, so the catalog
/// needs no extra structured data from the call site.
///
/// This is the one entry point the renderer calls; everything above is its
/// machinery.
pub fn localized_msg(d: &Diag, src: &str, flavor: Flavor) -> Option<String> {
    let code = d.code?;
    let template = localized(code, flavor)?;
    let name = src.get(d.span.start..d.span.end).unwrap_or("");
    Some(fill(template, name, flavor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn lang_of(src: &str) -> Flavor {
        majority_flavor(&lex(src).expect("lexes"))
    }

    #[test]
    fn majority_picks_the_dominant_keyword_flavor() {
        // All-English keywords.
        assert_eq!(
            lang_of("module M {\n  in a: bit\n  out y: bit\n  y = a\n}\n"),
            Flavor::English
        );
        // All-Tamil keywords (தொகுதி = module, etc.).
        assert_eq!(
            lang_of("தொகுதி M {\n  உள் a: bit\n  வெளி y: bit\n  y = a\n}\n"),
            Flavor::Tamil
        );
    }

    #[test]
    fn majority_falls_back_to_english_with_no_keywords() {
        assert_eq!(majority_flavor(&[]), Flavor::English);
    }

    #[test]
    fn effective_lang_override_beats_majority() {
        let toks = lex("தொகுதி M {\n  உள் a: bit\n  வெளி y: bit\n  y = a\n}\n").unwrap();
        // Majority is Tamil, but an explicit choice wins.
        assert_eq!(
            effective_lang(Some(Flavor::English), &toks),
            Flavor::English
        );
        // No override → majority.
        assert_eq!(effective_lang(None, &toks), Flavor::Tamil);
    }

    #[test]
    fn parse_lang_matches_translate_flavor() {
        assert_eq!(parse_lang("ta"), Some(Flavor::Tamil));
        assert_eq!(parse_lang("english"), Some(Flavor::English));
        assert_eq!(parse_lang("klingon"), None);
    }

    #[test]
    fn inflect_attaches_each_case_suffix() {
        // Latin stem → hyphen + Tamil suffix (the spec/04 §5 `'sum'-ஐ` shape).
        assert_eq!(inflect("sum", Case::Accusative, Flavor::Tamil), "sum-ஐ");
        assert_eq!(inflect("y", Case::Dative, Flavor::Tamil), "y-க்கு");
        // Tamil-script stem → direct join (no hyphen).
        assert_eq!(inflect("நிலை", Case::Locative, Flavor::Tamil), "நிலைஇல்");
        // Tanglish romanizes the suffix and always hyphenates.
        assert_eq!(
            inflect("sum", Case::Instrumental, Flavor::Tanglish),
            "sum-aal"
        );
        // English does not inflect.
        assert_eq!(inflect("sum", Case::Accusative, Flavor::English), "sum");
    }

    #[test]
    fn suffix_table_has_every_case() {
        // Touching the table forces the startup parse/validation to run.
        for case in [
            Case::Accusative,
            Case::Dative,
            Case::Locative,
            Case::Instrumental,
        ] {
            assert!(!case.suffix(Flavor::Tamil).is_empty());
            assert!(!case.suffix(Flavor::Tanglish).is_empty());
        }
    }

    #[test]
    fn localized_is_none_for_uncovered_codes_and_for_english() {
        assert!(localized("E0501", Flavor::Tamil).is_some());
        assert!(localized("E0501", Flavor::English).is_none()); // English = fallback
        assert!(localized("E0001", Flavor::Tamil).is_none()); // not in the stub catalog
    }

    #[test]
    fn fill_inflects_the_stub_template() {
        let out = fill("{name.dat} ஒன்றுக்கு மேற்பட்ட இயக்கிகள் உள்ளன", "y", Flavor::Tamil);
        assert!(out.starts_with("y-க்கு"), "got {out:?}");
    }
}
