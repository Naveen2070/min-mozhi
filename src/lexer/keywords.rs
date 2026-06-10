//! Loads the trilingual keyword table from `keywords.toml` (embedded at
//! build time, parsed once at startup). The table is DATA, not code —
//! native-speaker review changes the TOML, never this file (spec/03 §4).

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use super::token::{Flavor, Kw};

const KEYWORDS_TOML: &str = include_str!("../../keywords.toml");

#[derive(Deserialize)]
struct TableFile {
    keywords: HashMap<String, Spellings>,
    #[serde(default)]
    reserved: Vec<String>,
}

#[derive(Deserialize)]
struct Spellings {
    en: String,
    tanglish: String,
    tamil: String,
}

pub struct KeywordTable {
    /// spelling -> (token, which column it came from)
    map: HashMap<String, (Kw, Flavor)>,
    reserved: Vec<String>,
}

impl KeywordTable {
    pub fn lookup(&self, ident: &str) -> Option<(Kw, Flavor)> {
        self.map.get(ident).copied()
    }

    pub fn is_reserved(&self, ident: &str) -> bool {
        self.reserved.iter().any(|r| r == ident)
    }
}

pub static TABLE: LazyLock<KeywordTable> = LazyLock::new(|| {
    let file: TableFile =
        toml::from_str(KEYWORDS_TOML).expect("keywords.toml is malformed — fix the table");
    let mut map = HashMap::new();
    for (key, s) in &file.keywords {
        let kw = kw_for_key(key)
            .unwrap_or_else(|| panic!("keywords.toml has unknown keyword key `{key}`"));
        for (spelling, flavor) in [
            (&s.en, Flavor::English),
            (&s.tanglish, Flavor::Tanglish),
            (&s.tamil, Flavor::Tamil),
        ] {
            let prev = map.insert(spelling.clone(), (kw, flavor));
            assert!(
                prev.is_none(),
                "keywords.toml: spelling `{spelling}` appears in two columns — columns must be disjoint"
            );
        }
    }
    KeywordTable {
        map,
        reserved: file.reserved,
    }
});

fn kw_for_key(key: &str) -> Option<Kw> {
    Some(match key {
        "module" => Kw::Module,
        "in" => Kw::In,
        "out" => Kw::Out,
        "wire" => Kw::Wire,
        "reg" => Kw::Reg,
        "clock" => Kw::Clock,
        "reset" => Kw::Reset,
        "on" => Kw::On,
        "rise" => Kw::Rise,
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
        assert_eq!(TABLE.lookup("nilai").unwrap().1, Flavor::Tanglish);
        assert_eq!(TABLE.lookup("நிலை").unwrap().1, Flavor::Tamil);
    }

    #[test]
    fn fall_is_reserved() {
        assert!(TABLE.is_reserved("fall"));
        assert!(TABLE.lookup("fall").is_none());
    }
}
