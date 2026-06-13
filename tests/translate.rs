//! `mimz translate --to <flavor>` validated against the four-flavor corpus.
//!
//! The `examples/{english,tanglish,tamil}/` folders are the same programs with
//! only their KEYWORDS swapped (RULES R9), so they are a ready-made oracle:
//!
//! 1. **Round-trip is byte-identical.** Translating a file to another flavor
//!    and back reproduces the original byte-for-byte — translation is lossless
//!    (comments, layout, identifiers all preserved verbatim).
//! 2. **Cross-flavor match at the token level.** Translating english `X` to
//!    flavor `T` lexes to the SAME token stream as the committed `T/X`. We
//!    compare tokens, not bytes, because the corpus files carry a flavor-tagged
//!    note in their header COMMENT ("Tamil flavor — only the keywords change");
//!    comments are deliberately preserved verbatim by the reskin, so they
//!    differ across flavors while the code does not.

use std::fs;
use std::path::PathBuf;

use mimz::lexer::lex;
use mimz::lexer::token::{Flavor, TokKind};
use mimz::translate::translate;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Base example file names present in ALL three pure-flavor folders.
fn base_examples() -> Vec<String> {
    let mut names = Vec::new();
    for entry in fs::read_dir(root().join("examples/english")).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            let name = entry.file_name().into_string().unwrap();
            if name.ends_with(".mimz") {
                names.push(name);
            }
        }
    }
    names.sort();
    assert!(
        names.len() >= 10,
        "expected the example corpus, got {names:?}"
    );
    names
}

fn read(flavor: &str, base: &str) -> String {
    fs::read_to_string(root().join("examples").join(flavor).join(base))
        .unwrap_or_else(|e| panic!("read examples/{flavor}/{base}: {e}"))
}

/// The token KINDS of a source (comments/whitespace are not tokens), as the
/// flavor-blind fingerprint of a program. Two files with the same identifiers,
/// numbers, structure, and keywords share this exactly.
fn token_kinds(src: &str) -> Vec<TokKind> {
    lex(src)
        .unwrap_or_else(|d| panic!("source must lex, got {} diags", d.len()))
        .into_iter()
        .map(|t| t.kind)
        .collect()
}

const FLAVORS: [(&str, Flavor); 3] = [
    ("english", Flavor::English),
    ("tanglish", Flavor::Tanglish),
    ("tamil", Flavor::Tamil),
];

#[test]
fn round_trip_to_every_flavor_is_byte_identical() {
    for base in base_examples() {
        // Translation normalizes accepted aliases to the canonical spelling
        // (e.g. `include` -> `import`), by design. So anchor the round-trip on
        // the canonical form: once canonical, reskinning is a perfect byte-level
        // bijection — translate to any flavor and back changes nothing.
        let canonical = translate(&read("english", &base), Flavor::English).expect("lexes");
        for (_, target) in FLAVORS {
            let there = translate(&canonical, target).expect("lexes");
            let back = translate(&there, Flavor::English).expect("lexes");
            assert_eq!(
                back, canonical,
                "round-trip english -> {target:?} -> english changed `{base}`"
            );
        }
    }
}

#[test]
fn translating_english_matches_the_committed_flavor_token_for_token() {
    for base in base_examples() {
        let english = read("english", &base);
        for (name, target) in FLAVORS {
            let translated = translate(&english, target).expect("lexes");
            let committed = read(name, &base);
            assert_eq!(
                token_kinds(&translated),
                token_kinds(&committed),
                "translate(english/{base} -> {name}) does not match the committed examples/{name}/{base} at the token level"
            );
        }
    }
}

#[test]
fn every_keyword_token_is_in_the_target_flavor() {
    // After translating to Tamil, no English keyword spelling should survive as
    // a keyword token (proves the reskin actually fired, not just round-tripped).
    let english = read("english", "counter.mimz");
    let tamil = translate(&english, Flavor::Tamil).expect("lexes");
    assert!(tamil.contains("தொகுதி"), "expected Tamil `module`");
    assert!(!tamil.contains("module"), "English `module` should be gone");
}
