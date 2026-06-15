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

/// A non-fatal warning (W0001) when a file mixes **Tamil** keywords with English
/// or Tanglish ones. English and Tanglish share code word order (SVO) and may
/// mix freely; Tamil reads differently, so mixing it with the others is flagged
/// for readability. Returns `None` for a single-flavor file or an
/// English+Tanglish mix. The span points at the first keyword whose flavor is
/// not the file's majority — the odd one out. (`mimz fmt` normalizes any mix;
/// this only nudges, and never fails the build.)
pub fn flavor_mix_warning(tokens: &[Token]) -> Option<Diag> {
    let used = flavors_used(tokens);
    if !(used.contains(&Flavor::Tamil) && used.len() > 1) {
        return None;
    }
    let majority = majority_flavor(tokens);
    let span = tokens
        .iter()
        .find(|t| matches!(t.kind, TokKind::Kw(_)) && t.flavor.is_some_and(|f| f != majority))
        .or_else(|| tokens.iter().find(|t| t.flavor == Some(Flavor::Tamil)))
        .map(|t| t.span)?;
    Some(
        Diag::new(span, "this file mixes Tamil keywords with English/Tanglish")
            .with_code("W0001")
            .with_help(
                "Tamil reads differently from English/Tanglish — keep one language per file, \
                 or run `mimz fmt` to normalize",
            )
            .as_warning(),
    )
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
/// Sandhi rule — RATIFIED by the v1 native-speaker review (2026-06-15,
/// decision C3 closed). Identifiers in Min-Mozhi are Latin (R9: Tamil-script
/// names are transliterated), so the join is: a Latin-script stem takes a hyphen
/// before the suffix (`'sum'-ஐ`, matching spec/04 §5); a Tamil-script stem (only
/// reachable via a hand-written template, not an identifier) joins directly;
/// Tanglish always hyphenates. English returns the bare name.
pub fn inflect(name: &str, case: Case, flavor: Flavor) -> String {
    // A missing/empty stem (e.g. a diagnostic whose span did not resolve to
    // source text) has nothing to inflect — return it bare rather than emitting
    // a suffix-only fragment like `ஐ`.
    if name.is_empty() {
        return String::new();
    }
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

/// The localized error catalog, parsed once from the embedded `messages.toml`.
///
/// DATA, not code (the keywords.toml / case_suffixes.toml doctrine): the
/// native-speaker panel edits `messages.toml`, never this file. English is
/// absent — it is the verbatim fallback — so an uncovered code renders in
/// English unchanged. A covered code defines BOTH localized flavors (enforced
/// by a sync guard in `tests/morph.rs`). Templates interpolate `{name}` and
/// `{name.acc|dat|loc|inst}` via [`fill`].
const MESSAGES_TOML: &str = include_str!("../messages.toml");

#[derive(Deserialize)]
struct MessageFile {
    #[serde(default)]
    message: HashMap<String, MessageForms>,
}

#[derive(Deserialize)]
struct MessageForms {
    tamil: String,
    tanglish: String,
}

static MESSAGES: LazyLock<HashMap<String, MessageForms>> = LazyLock::new(|| {
    let file: MessageFile =
        toml::from_str(MESSAGES_TOML).expect("messages.toml is malformed — fix the catalog");
    file.message
});

/// Look up the localized template for a code in a flavor, if the catalog has
/// one. English always returns `None` (it is the verbatim fallback). The
/// `&'static` borrow is sound: `MESSAGES` is a `static`, so its entries live
/// for the whole program (same as `SUFFIXES`).
fn localized(code: &str, flavor: Flavor) -> Option<&'static str> {
    let forms = MESSAGES.get(code)?;
    match flavor {
        Flavor::Tamil => Some(forms.tamil.as_str()),
        Flavor::Tanglish => Some(forms.tanglish.as_str()),
        Flavor::English => None,
    }
}

/// Fill a template's interpolation tokens: the inflected identifier
/// (`{name}` / `{name.acc|dat|loc|inst}`) plus any structured `{key}` args the
/// diagnostic carried (e.g. `{expected}`/`{found}`). A token with no value is
/// left intact — [`localized_msg`] treats a leftover `{…}` as "this template
/// doesn't fit this diagnostic" and falls back to English.
fn fill(template: &str, name: &str, args: &[(&'static str, String)], flavor: Flavor) -> String {
    let mut out = template
        .replace("{name.acc}", &inflect(name, Case::Accusative, flavor))
        .replace("{name.dat}", &inflect(name, Case::Dative, flavor))
        .replace("{name.loc}", &inflect(name, Case::Locative, flavor))
        .replace("{name.inst}", &inflect(name, Case::Instrumental, flavor))
        .replace("{name}", name);
    for (key, value) in args {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    out
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
    let rendered = fill(template, name, &d.args, flavor);
    // A leftover `{token}` means this template needs a structured arg the
    // diagnostic did not supply (a different message shape under the same code).
    // Fall back to the English `msg` rather than print a literal `{token}`.
    if rendered.contains('{') {
        return None;
    }
    Some(rendered)
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
            lang_of("தொகுதி M {\n  உள்ளீடு a: bit\n  வெளியீடு y: bit\n  y = a\n}\n"),
            Flavor::Tamil
        );
    }

    #[test]
    fn majority_falls_back_to_english_with_no_keywords() {
        assert_eq!(majority_flavor(&[]), Flavor::English);
    }

    #[test]
    fn majority_breaks_ties_toward_the_earliest_keyword_column() {
        // Equal keyword counts → the earliest column wins (en < tl < ta). Each
        // word below is the `module` keyword in a different flavor (keywords.toml),
        // so the counts are exact with no type tokens to confound them.
        assert_eq!(lang_of("module thoguthi\n"), Flavor::English); // en == tl > ta
        assert_eq!(lang_of("thoguthi தொகுதி\n"), Flavor::Tanglish); // tl == ta > en
        assert_eq!(lang_of("module thoguthi தொகுதி\n"), Flavor::English); // three-way
    }

    #[test]
    fn inflect_of_an_empty_stem_is_empty_not_a_bare_suffix() {
        // A diagnostic whose span did not resolve must not render as `ஐ …`.
        assert_eq!(inflect("", Case::Accusative, Flavor::Tamil), "");
        assert_eq!(inflect("", Case::Dative, Flavor::Tanglish), "");
    }

    #[test]
    fn flavor_mix_warns_only_when_tamil_meets_the_others() {
        let warns = |src: &str| flavor_mix_warning(&lex(src).expect("lexes")).is_some();
        // Tamil keyword + a non-Tamil keyword → warn (the SOV/SVO clash).
        assert!(warns("தொகுதி in\n")); // module(tamil) + in(english)
        assert!(warns("தொகுதி veliyeedu\n")); // module(tamil) + out(tanglish)
        assert!(warns("module veliyeedu தொகுதி\n")); // all three
        // English + Tanglish share code order — mixing them stays clean.
        assert!(!warns("module veliyeedu\n"));
        // Single flavor → clean.
        assert!(!warns("தொகுதி வெளியீடு\n")); // both Tamil
        assert!(!warns("module in\n")); // both English
        assert!(!warns("")); // no keywords
    }

    #[test]
    fn flavor_mix_warning_is_a_nonfatal_w0001() {
        let d = flavor_mix_warning(&lex("தொகுதி in\n").expect("lexes")).expect("mix warns");
        assert_eq!(d.code, Some("W0001"));
        assert!(
            !d.is_error(),
            "the mixed-flavor lint must not fail the build"
        );
    }

    #[test]
    fn effective_lang_override_beats_majority() {
        let toks = lex("தொகுதி M {\n  உள்ளீடு a: bit\n  வெளியீடு y: bit\n  y = a\n}\n").unwrap();
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
        // E0403 has many message shapes, so it is intentionally not localized
        // (it falls back to English) — a stable "uncovered" example.
        assert!(localized("E0403", Flavor::Tamil).is_none());
    }

    #[test]
    fn fill_inflects_the_stub_template() {
        let out = fill(
            "{name.dat} ஒன்றுக்கு மேற்பட்ட இயக்கிகள் உள்ளன",
            "y",
            &[],
            Flavor::Tamil,
        );
        assert!(out.starts_with("y-க்கு"), "got {out:?}");
    }
}
