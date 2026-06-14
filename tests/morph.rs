//! Error-language plumbing (Phase 1.8, spec/04 §5): selection + inflection +
//! the additive English-fallback render path, end-to-end through the CLI.
//!
//! The localized catalog is a STUB (one shape, E0501, pending the C3 panel), so
//! these tests assert the MECHANISM — that the right flavor is chosen, that the
//! interpolated identifier is inflected, and crucially that any code the catalog
//! does NOT cover renders exactly as it did before (English, byte-for-byte) —
//! not the linguistic correctness of the stub strings.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use mimz::lexer::lex;
use mimz::lexer::token::Flavor;
use mimz::morph::{Case, effective_lang, inflect, majority_flavor};

fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

/// Write `src` to a unique temp `.mimz` and return its path.
fn temp_mimz(src: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let p = std::env::temp_dir().join(format!(
        "mimz_morph_{}.mimz",
        N.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&p, src).unwrap();
    p
}

/// Run `mimz check [--lang L] <file>` and return its stderr (where human
/// diagnostics go). The file is expected to FAIL the check.
fn check_stderr(src: &str, lang: Option<&str>) -> String {
    let path = temp_mimz(src);
    let mut cmd = mimz();
    cmd.arg("check");
    if let Some(l) = lang {
        cmd.arg("--lang").arg(l);
    }
    let out = cmd.arg(&path).output().unwrap();
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A program with a double-driven output — the one shape the stub catalog
/// covers (E0501). The offending span is the signal name `y`.
const DOUBLE_DRIVE: &str = "module M {\n  in a: bit\n  out y: bit\n  y = a\n  y = a\n}\n";

/// A program with a width mismatch (E0401) — a code the stub catalog does NOT
/// cover, so it must stay English under every `--lang`.
const WIDTH_MISMATCH: &str = "module M {\n  in a: bits[4]\n  out y: bits[8]\n  y = a\n}\n";

// ---- Selection ----------------------------------------------------------

#[test]
fn majority_and_effective_lang_track_the_keywords() {
    let en = lex("module M {\n  in a: bit\n  out y: bit\n  y = a\n}\n").unwrap();
    let ta = lex("தொகுதி M {\n  உள் a: bit\n  வெளி y: bit\n  y = a\n}\n").unwrap();
    assert_eq!(majority_flavor(&en), Flavor::English);
    assert_eq!(majority_flavor(&ta), Flavor::Tamil);
    // An explicit choice overrides the majority; absence falls back to it.
    assert_eq!(effective_lang(Some(Flavor::English), &ta), Flavor::English);
    assert_eq!(effective_lang(None, &ta), Flavor::Tamil);
}

#[test]
fn inflect_attaches_the_spec_case_suffixes() {
    assert_eq!(inflect("sum", Case::Accusative, Flavor::Tamil), "sum-ஐ");
    assert_eq!(inflect("y", Case::Dative, Flavor::Tamil), "y-க்கு");
    assert_eq!(inflect("y", Case::Dative, Flavor::Tanglish), "y-kku");
    assert_eq!(inflect("y", Case::Dative, Flavor::English), "y"); // no inflection
}

// ---- CLI: the localized path -------------------------------------------

#[test]
fn covered_code_renders_tamil_with_the_inflected_name() {
    let err = check_stderr(DOUBLE_DRIVE, Some("ta"));
    // The stub E0501 template, with the dative-inflected signal name.
    assert!(
        err.contains("error[E0501]: y-க்கு"),
        "expected the localized Tamil E0501 line, got:\n{err}"
    );
    // The English wording must be gone from the WHAT line.
    assert!(
        !err.contains("more than one driver"),
        "Tamil render still shows the English message:\n{err}"
    );
}

#[test]
fn covered_code_auto_selects_tamil_from_the_file() {
    // No `--lang`: a Tamil-keyword file must pick Tamil on its own.
    let tamil_src = "தொகுதி M {\n  உள் a: bit\n  வெளி y: bit\n  y = a\n  y = a\n}\n";
    let err = check_stderr(tamil_src, None);
    assert!(
        err.contains("error[E0501]: y-க்கு"),
        "majority detection should have rendered Tamil, got:\n{err}"
    );
}

// ---- CLI: the English-fallback invariant -------------------------------

#[test]
fn covered_code_stays_english_with_lang_en() {
    let err = check_stderr(DOUBLE_DRIVE, Some("en"));
    assert!(
        err.contains("error[E0501]: `y` has more than one driver"),
        "english must be the original wording, got:\n{err}"
    );
}

#[test]
fn uncovered_code_is_identical_across_languages() {
    // E0401 is not in the stub catalog, so the WHAT line must be byte-identical
    // under every flavor — proof the additive plumbing leaves untouched messages
    // exactly as they were.
    let en = check_stderr(WIDTH_MISMATCH, Some("en"));
    let ta = check_stderr(WIDTH_MISMATCH, Some("ta"));
    let tl = check_stderr(WIDTH_MISMATCH, Some("tanglish"));
    let line = |s: &str| {
        s.lines()
            .find(|l| l.starts_with("error[E0401]"))
            .unwrap_or("<none>")
            .to_string()
    };
    assert_eq!(
        line(&en),
        line(&ta),
        "uncovered code changed under --lang ta"
    );
    assert_eq!(
        line(&en),
        line(&tl),
        "uncovered code changed under --lang tanglish"
    );
    assert!(line(&en).starts_with("error[E0401]:"), "expected E0401");
}

#[test]
fn unknown_lang_is_a_clean_error() {
    let err = check_stderr(DOUBLE_DRIVE, Some("klingon"));
    assert!(
        err.contains("unknown language `klingon`"),
        "expected a clean unknown-language error, got:\n{err}"
    );
}
