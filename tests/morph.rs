//! Error-language plumbing (Phase 1.8, spec/04 section 5): selection + inflection +
//! structured-arg interpolation + the additive English-fallback render path,
//! end-to-end through the CLI.
//!
//! The localized catalog is now the native-authored one (`messages.toml`, 33 of
//! 36 codes, decision C3 ratified 2026-06-15), so these tests assert the
//! MECHANISM — that the right flavor is chosen, that the interpolated identifier
//! is inflected and the structured args (`{expected}`/`{op}`/`{type}`/…) are
//! filled, and crucially that any code the catalog does NOT cover (E0403/E0404/
//! E0405) renders exactly as before (English, byte-for-byte) — not the linguistic
//! correctness of the catalog strings.

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

/// A program with a literal that doesn't fit (E0405) — a multi-shape code the
/// catalog deliberately does NOT localize, so it must stay English under every
/// `--lang` (the English-fallback invariant).
const UNCOVERED_E0405: &str = "module M {\n  out y: bits[2]\n  y = 9\n}\n";

// ---- Selection ----------------------------------------------------------

#[test]
fn majority_and_effective_lang_track_the_keywords() {
    let en = lex("module M {\n  in a: bit\n  out y: bit\n  y = a\n}\n").unwrap();
    let ta = lex("தொகுதி M {\n  உள்ளீடு a: bit\n  வெளியீடு y: bit\n  y = a\n}\n").unwrap();
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
        err.contains("error[E0501]: `y-க்கு`"),
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
    let tamil_src = "தொகுதி M {\n  உள்ளீடு a: bit\n  வெளியீடு y: bit\n  y = a\n  y = a\n}\n";
    let err = check_stderr(tamil_src, None);
    assert!(
        err.contains("error[E0501]: `y-க்கு`"),
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
    // E0405 is not localized, so the WHAT line must be byte-identical under
    // every flavor — proof the additive plumbing leaves untouched messages
    // exactly as they were.
    let en = check_stderr(UNCOVERED_E0405, Some("en"));
    let ta = check_stderr(UNCOVERED_E0405, Some("ta"));
    let tl = check_stderr(UNCOVERED_E0405, Some("tanglish"));
    let line = |s: &str| {
        s.lines()
            .find(|l| l.starts_with("error[E0405]"))
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
    assert!(line(&en).starts_with("error[E0405]:"), "expected E0405");
}

#[test]
fn compile_also_localizes_diagnostics() {
    // The localization path is shared by `check` AND `compile` (both render via
    // morph). A failing compile under `--lang ta` shows the Tamil E0501 line.
    let path = temp_mimz(DOUBLE_DRIVE);
    let out = mimz()
        .arg("compile")
        .arg("--lang")
        .arg("ta")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!out.status.success(), "double-drive must fail to compile");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("error[E0501]: `y-க்கு`"),
        "compile localizes too, got:\n{err}"
    );
}

#[test]
fn unknown_lang_is_a_clean_error() {
    let err = check_stderr(DOUBLE_DRIVE, Some("klingon"));
    assert!(
        err.contains("unknown language `klingon`"),
        "expected a clean unknown-language error, got:\n{err}"
    );
}

// ---- Newly-wired catalog codes (v1 native-authored) --------------------

/// An undriven output (E0502) — a `{name}`-only template, now localized.
#[test]
fn e0502_renders_tamil() {
    let src = "தொகுதி M {\n  உள்ளீடு a: bit\n  வெளியீடு y: bit\n}\n";
    let err = check_stderr(src, None);
    assert!(
        err.contains("error[E0502]: `y`") && err.contains("இயக்கப்படவில்லை"),
        "expected localized Tamil E0502, got:\n{err}"
    );
}

/// `=` on a reg (E0505) — Tamil under an explicit `--lang ta`.
#[test]
fn e0505_renders_tamil() {
    let src = "module M {\n  clock clk\n  reset rst\n  reg r: bit = 0\n  on rise(clk) {\n    r <- 1\n  }\n  r = 1\n}\n";
    let err = check_stderr(src, Some("ta"));
    assert!(
        err.contains("error[E0505]: `r`") && err.contains("பதிவேட்டை"),
        "expected localized Tamil E0505, got:\n{err}"
    );
}

/// A name-less template (E0202 const overflow) localizes with no `{name}` slot.
#[test]
fn e0202_renders_tanglish_nameless() {
    let src = "thoguthi M {\n  maarili HUGE: int = 170141183460469231731687303715884105727 + 1\n  veliyeedu y: bit\n  y = 0\n}\n";
    let err = check_stderr(src, None);
    assert!(
        err.contains("error[E0202]: Maariliyin kanakkeedu"),
        "expected localized Tanglish E0202, got:\n{err}"
    );
}

// ---- Step 2: structured-arg interpolation ------------------------------

/// E0401 carries `{expected}`/`{found}` args, so its localized template
/// interpolates the two type strings (the renderer extension).
#[test]
fn e0401_interpolates_expected_and_found() {
    let src = "தொகுதி M {\n  உள்ளீடு a: bits[4]\n  வெளியீடு y: bits[8]\n  y = a\n}\n";
    let err = check_stderr(src, None);
    assert!(
        err.contains("error[E0401]:")
            && err.contains("bits[8]")
            && err.contains("bits[4]")
            && err.contains("எதிர்பார்க்கப்பட்டது"),
        "expected localized Tamil E0401 with interpolated widths, got:\n{err}"
    );
    // No leftover template token leaked into the message.
    assert!(
        !err.contains("{expected}") && !err.contains("{found}"),
        "got:\n{err}"
    );
}

/// E0402 carries `{op}`/`{lhs}`/`{rhs}` — an unequal-width binary op localizes
/// with the operator token and both operand widths interpolated.
#[test]
fn e0402_interpolates_op_lhs_rhs() {
    let src = "module M {\n  in a: bits[4]\n  in b: bits[8]\n  out y: bits[8]\n  y = a +% b\n}\n";
    let err = check_stderr(src, Some("ta"));
    assert!(
        err.contains("error[E0402]:")
            && err.contains("`+%`")
            && err.contains("bits[4]")
            && err.contains("bits[8]")
            && err.contains("அகலங்கள்"),
        "expected localized Tamil E0402 with op/lhs/rhs interpolated, got:\n{err}"
    );
    assert!(
        !err.contains("{op}") && !err.contains("{lhs}") && !err.contains("{rhs}"),
        "no template token may leak:\n{err}"
    );
}

/// E0408 carries `{first}`/`{second}` — disagreeing `if`-arms in a width-inferred
/// (operand) position localize with both arm types interpolated.
#[test]
fn e0408_interpolates_first_and_second() {
    let src = "module M {\n  in c: bit\n  in a: bits[4]\n  in b: bits[8]\n  out y: bits[8]\n  y = (if c { a } else { b }) | b\n}\n";
    let err = check_stderr(src, Some("ta"));
    assert!(
        err.contains("error[E0408]:")
            && err.contains("bits[4]")
            && err.contains("bits[8]")
            && err.contains("மாறுபடுகின்றன"),
        "expected localized Tamil E0408 with first/second interpolated, got:\n{err}"
    );
    assert!(
        !err.contains("{first}") && !err.contains("{second}"),
        "no template token may leak:\n{err}"
    );
}

/// E0601 carries `{type}` — a non-exhaustive `match` localizes with the
/// scrutinee type interpolated.
#[test]
fn e0601_interpolates_type() {
    let src = "module M {\n  in sel: bits[2]\n  out y: bits[8]\n  y = match sel {\n    0b00 => 1\n    0b01 => 2\n  }\n}\n";
    let err = check_stderr(src, Some("ta"));
    assert!(
        err.contains("error[E0601]:") && err.contains("bits[2]") && err.contains("உள்ளடக்கவில்லை"),
        "expected localized Tamil E0601 with type interpolated, got:\n{err}"
    );
    assert!(
        !err.contains("{type}"),
        "no template token may leak:\n{err}"
    );
}

// ---- Catalog completeness guard ----------------------------------------

/// Every `[message.Exxxx]` key in `messages.toml` must be a real checker code.
/// A typo'd key would be a dead localization that silently never fires; this
/// fails naming it. (Both flavors are required by the loader's serde schema, so
/// a half-localized entry fails to parse and panics at startup — this guards the
/// key namespace, the part serde cannot.)
#[test]
fn message_catalog_keys_are_real_checker_codes() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lang/messages.toml");
    let toml = fs::read_to_string(&path).expect("messages.toml exists");
    let keys: Vec<&str> = toml
        .lines()
        .filter_map(|l| l.trim().strip_prefix("[message."))
        .filter_map(|r| r.strip_suffix(']'))
        .collect();
    assert!(!keys.is_empty(), "messages.toml has no [message.*] entries");
    for code in keys {
        assert!(
            mimz::diag::ALL_CHECKER_CODES.contains(&code),
            "messages.toml localizes `{code}`, which is not a checker code in \
             diag::ALL_CHECKER_CODES — fix the typo or remove the entry"
        );
    }
}

/// Every `{token}` placeholder in `messages.toml` must be one `morph::fill`
/// actually fills — the `{name}`/`{name.*}` identifier tokens or a structured
/// arg key wired in the checker. A typo (`{expcted}`) or an arg no diagnostic
/// supplies would silently never fill, leaving the leftover `{` to force English
/// fallback FOREVER — a dead localization no other test would notice. This fails
/// naming the bad token. (Comment/doc lines in the file use only known tokens, so
/// scanning the whole file is fine and also guards the doc examples.)
#[test]
fn message_catalog_placeholders_are_known_tokens() {
    const KNOWN: &[&str] = &[
        "name",
        "name.acc",
        "name.dat",
        "name.loc",
        "name.inst", // identifier
        "expected",
        "found",
        "op",
        "lhs",
        "rhs",
        "first",
        "second",
        "type", // structured args
    ];
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lang/messages.toml");
    let toml = fs::read_to_string(&path).expect("messages.toml exists");
    // Only ACTIVE template lines — skip `#` comments (doc header + the
    // commented-out DEFERRED examples legitimately mention `{reason}`/`{token}`).
    for line in toml.lines().filter(|l| !l.trim_start().starts_with('#')) {
        let mut rest = line;
        while let Some(open) = rest.find('{') {
            rest = &rest[open + 1..];
            let Some(close) = rest.find('}') else { break };
            let token = &rest[..close];
            assert!(
                KNOWN.contains(&token),
                "messages.toml has placeholder `{{{token}}}` that morph::fill does not \
                 fill — fix the typo, or wire the arg via Diag::with_arg and add it to KNOWN"
            );
            rest = &rest[close + 1..];
        }
    }
}

// ---- Mixed-flavor lint (W0001) -----------------------------------------

/// Tamil `module` keyword + English `in`/`out` — a VALID program, so the only
/// diagnostic is the non-fatal mixed-flavor warning.
const TAMIL_ENGLISH_MIX: &str = "தொகுதி M {\n  in a: bit\n  out y: bit\n  y = a\n}\n";

#[test]
fn mixing_tamil_with_english_warns_but_check_succeeds() {
    let path = temp_mimz(TAMIL_ENGLISH_MIX);
    let out = mimz().arg("check").arg(&path).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the mixed-flavor lint is non-fatal — check must still succeed:\n{stderr}"
    );
    assert!(
        stderr.contains("warning[W0001]"),
        "expected the W0001 warning on stderr, got:\n{stderr}"
    );
}

#[test]
fn a_single_flavor_file_has_no_mix_warning() {
    let path = temp_mimz("module M {\n  in a: bit\n  out y: bit\n  y = a\n}\n");
    let out = mimz().arg("check").arg(&path).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success());
    assert!(
        !stderr.contains("W0001"),
        "a clean single-flavor file must not warn, got:\n{stderr}"
    );
}

#[test]
fn json_check_carries_the_warning_and_still_succeeds() {
    let path = temp_mimz(TAMIL_ENGLISH_MIX);
    let out = mimz()
        .arg("check")
        .arg("--json")
        .arg(&path)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "warnings are non-fatal under --json too"
    );
    assert!(
        stdout.contains("\"W0001\""),
        "the JSON array must include the warning, got:\n{stdout}"
    );
    assert!(
        stdout.contains("\"severity\":\"warning\""),
        "the JSON diagnostic must mark its severity, got:\n{stdout}"
    );
}
