//! Loads the trilingual keyword table from `keywords.toml` (embedded at
//! build time, parsed once at startup). The table is DATA, not code —
//! native-speaker review changes the TOML, never this file (spec/03 section 4).

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use super::token::{Flavor, Kw};

const KEYWORDS_TOML: &str = include_str!("../../lang/keywords.toml");

/// Shape of `keywords.toml`: one `[keywords.<key>]` table per keyword plus
/// a root-level `reserved` list (which must sit ABOVE the first table —
/// TOML root keys cannot follow a table header).
#[derive(Deserialize)]
struct TableFile {
    /// Keyword-set version (`version = N` at the TOML root). Bumped when
    /// canonical spellings change; cross-checked against
    /// [`crate::version::KEYWORD_SET_VERSION`].
    #[serde(default = "default_kw_version")]
    version: u8,
    keywords: HashMap<String, Spellings>,
    #[serde(default)]
    reserved: Vec<String>,
}

fn default_kw_version() -> u8 {
    1
}

/// The three canonical spellings of one keyword, plus optional per-column
/// alias lists (e.g. `include` as an English alias of `import`). All
/// spellings — canonical and alias — must be disjoint across the whole
/// table; enforced at startup. Aliases lex to the same token as their
/// column's canonical spelling, so nothing after the lexer can tell them
/// apart; future tooling (`mimz translate`, `mimz fmt`) normalizes aliases
/// to the canonical spelling.
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

/// The loaded trilingual keyword table. The lexer queries this for every
/// identifier-shaped lexeme.
pub struct KeywordTable {
    /// spelling -> (token, which column it came from)
    map: HashMap<String, (Kw, Flavor)>,
    /// token -> its canonical spelling in each column, `[en, tanglish, tamil]`.
    /// The reverse of `map`; drives `mimz translate`'s keyword reskin. Every
    /// `Kw` has all three (REQUIRED_KEYS guarantees the rows exist).
    by_kw: HashMap<Kw, [String; 3]>,
    reserved: Vec<String>,
    version: u8,
}

impl KeywordTable {
    /// Is this spelling a keyword? Returns the token and the flavor of the
    /// column it came from (drives error language + `mimz fmt`, P1.8).
    pub fn lookup(&self, ident: &str) -> Option<(Kw, Flavor)> {
        self.map.get(ident).copied()
    }

    /// The canonical spelling of `kw` in `flavor` — the reverse lookup that
    /// `mimz translate` uses to reskin keyword tokens into a target flavor.
    /// Always succeeds: every keyword has a spelling in every column.
    pub fn canonical(&self, kw: Kw, flavor: Flavor) -> &str {
        let cols = &self.by_kw[&kw];
        match flavor {
            Flavor::English => &cols[0],
            Flavor::Tanglish => &cols[1],
            Flavor::Tamil => &cols[2],
        }
    }

    /// Every keyword's canonical spelling in `flavor` — the completion list.
    /// One entry per `Kw` (order unspecified). Drives flavor-matched keyword
    /// completion in the LSP.
    pub fn canonical_spellings(&self, flavor: Flavor) -> Vec<&str> {
        let col = match flavor {
            Flavor::English => 0,
            Flavor::Tanglish => 1,
            Flavor::Tamil => 2,
        };
        self.by_kw.values().map(|cols| cols[col].as_str()).collect()
    }

    /// Reserved for a future feature (e.g. `sync`, `struct`, `inout`) —
    /// not a keyword yet, but not usable as an identifier either.
    pub fn is_reserved(&self, ident: &str) -> bool {
        self.reserved.iter().any(|r| r == ident)
    }

    /// The keyword-set version from `keywords.toml` (`version = N`). The
    /// language edition's `code` aligns with this;
    /// [`crate::version::KEYWORD_SET_VERSION`] mirrors it (a test cross-checks).
    pub fn version(&self) -> u8 {
        self.version
    }
}

/// The one global table, parsed from the embedded TOML on first use.
/// Panics at startup (not at some later lookup) if the TOML is malformed,
/// names an unknown key, is MISSING a required key, or has a spelling in
/// two columns — table bugs must be impossible to ship.
/// Every key the table must define — `kw_for_key` accepts exactly these.
/// Without this list, DELETING a `[keywords.*]` entry would silently
/// demote that keyword to a plain identifier (the unknown-key panic only
/// guards the other direction). Update together with [`Kw`] and the TOML.
const REQUIRED_KEYS: [&str; 35] = [
    "module", "in", "out", "wire", "reg", "mem", "clock", "reset", "async", "on", "rise", "fall",
    "if", "else", "match", "enum", "let", "const", "repeat", "import", "true", "false", "test",
    "for", "tick", "expect", "and", "or", "not", "syntax", "thamizh", "fn", "default", "bundle",
    "return",
];

pub static TABLE: LazyLock<KeywordTable> = LazyLock::new(|| {
    let file: TableFile =
        toml::from_str(KEYWORDS_TOML).expect("keywords.toml is malformed — fix the table");
    for key in REQUIRED_KEYS {
        assert!(
            file.keywords.contains_key(key),
            "keywords.toml is missing `[keywords.{key}]` — every keyword needs its three spellings"
        );
    }
    let mut map = HashMap::new();
    let mut by_kw = HashMap::new();
    for (key, s) in &file.keywords {
        let kw = kw_for_key(key)
            .unwrap_or_else(|| panic!("keywords.toml has unknown keyword key `{key}`"));
        by_kw.insert(kw, [s.en.clone(), s.tanglish.clone(), s.tamil.clone()]);
        let mut column = |spellings: Vec<&String>, flavor: Flavor| {
            for spelling in spellings {
                // A spelling may repeat ACROSS columns of the SAME keyword
                // (e.g. the profile name `thamizh` is identical in the English
                // and Tanglish columns) — that is unambiguous. What must never
                // happen is one spelling resolving to two DIFFERENT keywords.
                // First column wins the recorded flavor (English before
                // Tanglish before Tamil), which is irrelevant to lexing since
                // the keyword is the same either way.
                if let Some((prev_kw, _)) = map.get(spelling) {
                    assert!(
                        *prev_kw == kw,
                        "keywords.toml: spelling `{spelling}` maps to two different keywords — \
                         spellings across different keywords must be disjoint"
                    );
                    continue;
                }
                map.insert(spelling.clone(), (kw, flavor));
            }
        };
        column(
            std::iter::once(&s.en).chain(&s.en_aliases).collect(),
            Flavor::English,
        );
        column(
            std::iter::once(&s.tanglish)
                .chain(&s.tanglish_aliases)
                .collect(),
            Flavor::Tanglish,
        );
        column(
            std::iter::once(&s.tamil).chain(&s.tamil_aliases).collect(),
            Flavor::Tamil,
        );
    }
    KeywordTable {
        map,
        by_kw,
        reserved: file.reserved,
        version: file.version,
    }
});

/// `keywords.toml` key → token. The single point that ties the data file
/// to the [`Kw`] enum; a new keyword is added here and in the TOML.
fn kw_for_key(key: &str) -> Option<Kw> {
    Some(match key {
        "module" => Kw::Module,
        "in" => Kw::In,
        "out" => Kw::Out,
        "wire" => Kw::Wire,
        "reg" => Kw::Reg,
        "mem" => Kw::Mem,
        "clock" => Kw::Clock,
        "reset" => Kw::Reset,
        "async" => Kw::Async,
        "on" => Kw::On,
        "rise" => Kw::Rise,
        "fall" => Kw::Fall,
        "if" => Kw::If,
        "else" => Kw::Else,
        "match" => Kw::Match,
        "enum" => Kw::Enum,
        "let" => Kw::Let,
        "const" => Kw::Const,
        "repeat" => Kw::Repeat,
        "import" => Kw::Import,
        "true" => Kw::True,
        "false" => Kw::False,
        "test" => Kw::Test,
        "for" => Kw::For,
        "tick" => Kw::Tick,
        "expect" => Kw::Expect,
        "and" => Kw::And,
        "or" => Kw::Or,
        "not" => Kw::Not,
        "syntax" => Kw::Syntax,
        "thamizh" => Kw::Thamizh,
        "fn" => Kw::Fn,
        "default" => Kw::Default,
        "bundle" => Kw::Bundle,
        "return" => Kw::Return,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_three_flavors_resolve_to_same_keyword() {
        assert_eq!(TABLE.lookup("module").unwrap().0, Kw::Module);
        assert_eq!(TABLE.lookup("thoguthi").unwrap().0, Kw::Module);
        assert_eq!(TABLE.lookup("தொகுதி").unwrap().0, Kw::Module);
    }

    #[test]
    fn flavors_are_recorded() {
        assert_eq!(TABLE.lookup("reg").unwrap().1, Flavor::English);
        assert_eq!(TABLE.lookup("pathivedu").unwrap().1, Flavor::Tanglish);
        assert_eq!(TABLE.lookup("பதிவேடு").unwrap().1, Flavor::Tamil);
    }

    #[test]
    fn include_is_an_alias_for_import() {
        assert_eq!(
            TABLE.lookup("include"),
            Some((Kw::Import, Flavor::English)),
            "`include` must lex to the exact same token as `import`"
        );
        assert_eq!(TABLE.lookup("import"), Some((Kw::Import, Flavor::English)));
    }

    #[test]
    fn kw_default_is_recognized() {
        // Promoted from reserved to active for `default name <- expr` (2026-06-30).
        // Tanglish/Tamil spellings are PROVISIONAL pending native review (R9/R11).
        assert!(!TABLE.is_reserved("default"));
        assert_eq!(TABLE.lookup("default").unwrap().0, Kw::Default);
        assert_eq!(TABLE.lookup("iyalbu").unwrap().0, Kw::Default);
        assert_eq!(TABLE.lookup("இயல்பு").unwrap().0, Kw::Default);
    }

    #[test]
    fn kw_bundle_is_recognized() {
        assert!(!TABLE.is_reserved("bundle"));
        assert_eq!(TABLE.lookup("bundle").unwrap().0, Kw::Bundle);
        assert_eq!(TABLE.lookup("kattai").unwrap().0, Kw::Bundle);
        assert_eq!(TABLE.lookup("கட்டை").unwrap().0, Kw::Bundle);
    }

    #[test]
    fn kw_return_is_recognized() {
        // Added for statement-based fn bodies (2026-07-03). Tanglish/Tamil
        // spellings are PROVISIONAL pending native review (R9/R11), same
        // pattern as `default`/`fall`/`bundle`.
        assert!(!TABLE.is_reserved("return"));
        assert_eq!(TABLE.lookup("return").unwrap().0, Kw::Return);
        assert_eq!(TABLE.lookup("thirumbu").unwrap().0, Kw::Return);
        assert_eq!(TABLE.lookup("திரும்பு").unwrap().0, Kw::Return);
    }

    #[test]
    fn fall_is_an_active_keyword() {
        // Promoted from reserved to active for `on fall(clk)` (A3, 2026-06-17).
        // Tanglish/Tamil spellings are PROVISIONAL pending native review (R9/R11).
        assert!(!TABLE.is_reserved("fall"));
        assert_eq!(TABLE.lookup("fall").unwrap().0, Kw::Fall);
        assert_eq!(TABLE.lookup("irakkam").unwrap().0, Kw::Fall);
        assert_eq!(TABLE.lookup("இறக்கம்").unwrap().0, Kw::Fall);
    }

    #[test]
    fn future_keywords_are_reserved_not_usable() {
        // Every word in the `reserved` list (R11 + the growth-doctrine freeze) must be
        // rejected as an identifier AND must NOT also be an active keyword, so no v0.1
        // program can claim it before its feature lands. We iterate the table's own
        // `reserved` list (single source of truth = lang/keywords.toml) rather than a
        // hardcoded copy, so a newly reserved word is guarded automatically and this
        // test can never drift behind the table (it once missed sync/inout/struct/
        // suzhal). Reserved words stay English-only until native review supplies the
        // Tamil/Tanglish spellings, except sentinel pairs already in the table.
        assert!(
            !TABLE.reserved.is_empty(),
            "the reserved list (lang/keywords.toml) should not be empty"
        );
        for word in &TABLE.reserved {
            assert!(
                TABLE.is_reserved(word),
                "`{word}` must be reserved before v0.1.0 (R11) so no program can claim it"
            );
            assert!(
                TABLE.lookup(word).is_none(),
                "`{word}` is in the reserved list but is also an active keyword — \
                 promote it out of `reserved` when its feature lands"
            );
        }
    }

    #[test]
    fn canonical_spellings_lists_every_keyword_in_a_flavor() {
        let en = TABLE.canonical_spellings(Flavor::English);
        // One spelling per keyword (REQUIRED_KEYS has 35).
        assert_eq!(en.len(), 35);
        assert!(en.contains(&"module"));
        assert!(en.contains(&"reg"));
        // Tamil column gives the Tamil spellings, never the English ones.
        let ta = TABLE.canonical_spellings(Flavor::Tamil);
        assert!(ta.contains(&"தொகுதி"));
        assert!(!ta.contains(&"module"));
    }
}
