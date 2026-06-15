//! Keeps the VS Code TextMate grammar in lockstep with `keywords.toml`
//! (the single source of truth for keyword words). If a spelling is
//! added or changed in the table and the grammar is not updated, this
//! fails naming the missing word — same philosophy as `docs_sync.rs`:
//! fix the grammar, don't weaken the test.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Deserialize)]
struct TableFile {
    keywords: HashMap<String, Spellings>,
    #[serde(default)]
    reserved: Vec<String>,
}

/// Mirror of the loader's schema (`src/lexer/keywords.rs`) — canonical
/// spellings plus optional per-column alias lists.
#[derive(Deserialize)]
struct Spellings {
    en: String,
    tanglish: String,
    tamil: String,
    #[serde(default)]
    en_aliases: Vec<String>,
    #[serde(default)]
    tanglish_aliases: Vec<String>,
    #[serde(default)]
    tamil_aliases: Vec<String>,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn grammar() -> String {
    fs::read_to_string(root().join("editors/vscode/syntaxes/mimz.tmLanguage.json"))
        .expect("editors/vscode/syntaxes/mimz.tmLanguage.json exists")
}

/// A plain `contains` would pass vacuously for short spellings (`in` is
/// a substring of `include`) — require the word as a whole alternation
/// member: delimited by `|`, `(`, or `)` on both sides.
fn grammar_has_word(grammar: &str, word: &str) -> bool {
    ["|", "("].iter().any(|l| {
        ["|", ")"]
            .iter()
            .any(|r| grammar.contains(&format!("{l}{word}{r}")))
    })
}

fn table() -> TableFile {
    toml::from_str(&fs::read_to_string(root().join("keywords.toml")).unwrap())
        .expect("keywords.toml parses with the loader's schema")
}

#[test]
fn every_keyword_spelling_is_in_the_grammar() {
    let grammar = grammar();
    let keywords = table().keywords;
    assert!(
        keywords.len() >= 26,
        "keywords.toml parsed suspiciously small"
    );
    for (key, s) in keywords {
        let spellings = [&s.en, &s.tanglish, &s.tamil]
            .into_iter()
            .chain(&s.en_aliases)
            .chain(&s.tanglish_aliases)
            .chain(&s.tamil_aliases);
        for sp in spellings {
            assert!(
                grammar_has_word(&grammar, sp),
                "keyword `{key}` spelling `{sp}` is missing from the VS Code grammar — \
                 update editors/vscode/syntaxes/mimz.tmLanguage.json"
            );
        }
    }
}

#[test]
fn every_reserved_word_is_marked_invalid() {
    let grammar = grammar();
    let reserved = table().reserved;
    assert!(!reserved.is_empty(), "keywords.toml has no reserved list?");
    for word in reserved {
        assert!(
            grammar_has_word(&grammar, &word),
            "reserved word `{word}` is missing from the grammar's invalid.illegal rule — \
             update editors/vscode/syntaxes/mimz.tmLanguage.json"
        );
    }
}

/// spec/03's keyword table is the human-readable mirror of `keywords.toml`.
/// Every spelling (all three columns + aliases) must appear in spec/03 as a
/// backtick-delimited `word`, so the spec can never silently drift from the
/// table after the v1 lock. Same philosophy as the grammar sync above.
#[test]
fn spec_03_keyword_table_matches_keywords_toml() {
    let spec =
        fs::read_to_string(root().join("spec/03-keywords-trilingual.md")).expect("spec/03 exists");
    let t = table();
    for (key, s) in t.keywords {
        let spellings = [&s.en, &s.tanglish, &s.tamil]
            .into_iter()
            .chain(&s.en_aliases)
            .chain(&s.tanglish_aliases)
            .chain(&s.tamil_aliases);
        for sp in spellings {
            assert!(
                spec.contains(&format!("`{sp}`")),
                "keyword `{key}` spelling `{sp}` is missing from spec/03-keywords-trilingual.md — \
                 update the keyword table there to match keywords.toml"
            );
        }
    }
    for word in t.reserved {
        assert!(
            spec.contains(&format!("`{word}`")),
            "reserved word `{word}` is missing from spec/03's reserved-words section"
        );
    }
}

#[test]
fn grammar_and_extension_manifest_agree() {
    let manifest = fs::read_to_string(root().join("editors/vscode/package.json")).unwrap();
    assert!(
        manifest.contains("\".mimz\""),
        "package.json must register the .mimz extension"
    );
    assert!(
        manifest.contains("source.mimz") && grammar().contains("source.mimz"),
        "scopeName must match between package.json and the grammar"
    );
}
