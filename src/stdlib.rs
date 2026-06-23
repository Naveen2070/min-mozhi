//! The embedded standard library. `import std.fifo` (and trilingual
//! equivalents) resolves here instead of the filesystem; the source is the
//! already-tested example files, `include_str!`'d at compile time so there is
//! no install path and it works in WASM (no FS). A `mimz.toml [lib] std = <dir>`
//! setting overrides this with a local copy (see `project::load_project_with_lib`).

use std::path::{Path, PathBuf};

/// Namespace spellings that name the standard library, one per flavor.
const NS_ALIASES: [&str; 3] = ["std", "nuulagam", "நூலகம்"];

/// Which embedded source a resolved import selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StdVariant {
    /// English-identifier canonical source (`examples/english/std/<stem>.mimz`).
    Canonical,
    /// Pure-Tamil twin (`examples/tamil-pure/<twin>.mimz`).
    Twin,
}

/// One standard-library module: both source variants and the names that select
/// each. `*_src` are the verbatim example files (single source of truth).
pub struct StdModule {
    pub stem: &'static str,
    pub canonical_src: &'static str,
    pub canonical_name: &'static str,
    pub twin_src: &'static str,
    pub twin_name: &'static str,
    pub twin_roman: &'static str,
}

impl StdModule {
    pub fn source(&self, v: StdVariant) -> &'static str {
        match v {
            StdVariant::Canonical => self.canonical_src,
            StdVariant::Twin => self.twin_src,
        }
    }
    pub fn module_name(&self, v: StdVariant) -> &'static str {
        match v {
            StdVariant::Canonical => self.canonical_name,
            StdVariant::Twin => self.twin_name,
        }
    }
}

/// The catalog. Paths are relative to `src/stdlib.rs`.
static MODULES: &[StdModule] = &[
    StdModule {
        stem: "debouncer",
        canonical_src: include_str!("../examples/english/std/debouncer.mimz"),
        canonical_name: "Debouncer",
        twin_src: include_str!("../examples/tamil-pure/nilaippaduthi.mimz"),
        twin_name: "நிலைப்படுத்தி",
        twin_roman: "nilaippaduthi",
    },
    StdModule {
        stem: "fifo",
        canonical_src: include_str!("../examples/english/std/fifo.mimz"),
        canonical_name: "Fifo",
        twin_src: include_str!("../examples/tamil-pure/varisai.mimz"),
        twin_name: "வரிசை",
        twin_roman: "varisai",
    },
    StdModule {
        stem: "pwm",
        canonical_src: include_str!("../examples/english/std/pwm.mimz"),
        canonical_name: "Pwm",
        twin_src: include_str!("../examples/tamil-pure/minukki.mimz"),
        twin_name: "மினுக்கி",
        twin_roman: "minukki",
    },
    StdModule {
        stem: "seg7",
        canonical_src: include_str!("../examples/english/std/seg7.mimz"),
        canonical_name: "Seg7",
        twin_src: include_str!("../examples/tamil-pure/ennkaatti.mimz"),
        twin_name: "எண்காட்டி",
        twin_roman: "ennkaatti",
    },
    StdModule {
        stem: "uart_tx",
        canonical_src: include_str!("../examples/english/std/uart_tx.mimz"),
        canonical_name: "UartTx",
        twin_src: include_str!("../examples/tamil-pure/anuppi.mimz"),
        twin_name: "அனுப்பி",
        twin_roman: "anuppi",
    },
];

/// True if `seg` is one of the standard-library namespace spellings.
pub fn is_std_namespace(seg: &str) -> bool {
    NS_ALIASES.contains(&seg)
}

/// Resolve `<ns>.<module>` to a catalog row + the variant the module spelling
/// selected. `None` if `ns` is not a std namespace or `module` is unknown.
pub fn resolve(ns: &str, module: &str) -> Option<(&'static StdModule, StdVariant)> {
    if !is_std_namespace(ns) {
        return None;
    }
    for m in MODULES {
        if module == m.stem {
            return Some((m, StdVariant::Canonical));
        }
        if module == m.twin_name || module == m.twin_roman {
            return Some((m, StdVariant::Twin));
        }
    }
    None
}

/// All catalog rows (eject + error listings).
pub fn modules() -> &'static [StdModule] {
    MODULES
}

/// Comma-joined English stems for error messages.
pub fn available() -> String {
    MODULES
        .iter()
        .map(|m| m.stem)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Write the embedded standard library to `dir` as one `.mimz` per module.
/// `tamil` selects the pure-Tamil twins (file named after the twin), else the
/// English canonical (named after the stem). Without `force`, refuses to
/// overwrite an existing file. Returns the paths written.
pub fn eject_to(dir: &Path, tamil: bool, force: bool) -> std::io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let mut written = Vec::new();
    for m in MODULES {
        let (name, src) = if tamil {
            (m.twin_roman, m.twin_src)
        } else {
            (m.stem, m.canonical_src)
        };
        let path = dir.join(format!("{name}.mimz"));
        if path.exists() && !force {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("{} exists (use --force to overwrite)", path.display()),
            ));
        }
        std::fs::write(&path, src)?;
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_aliases_match_all_three_flavors() {
        assert!(is_std_namespace("std"));
        assert!(is_std_namespace("nuulagam"));
        assert!(is_std_namespace("நூலகம்"));
        assert!(!is_std_namespace("lib"));
    }

    #[test]
    fn english_stem_selects_canonical() {
        let (m, v) = resolve("std", "fifo").expect("fifo resolves");
        assert_eq!(m.stem, "fifo");
        assert!(matches!(v, StdVariant::Canonical));
        assert_eq!(m.module_name(v), "Fifo");
        assert!(m.source(v).contains("module Fifo"));
    }

    #[test]
    fn twin_name_and_roman_select_twin() {
        let (_, v1) = resolve("நூலகம்", "வரிசை").expect("tamil twin resolves");
        assert!(matches!(v1, StdVariant::Twin));
        let (m, v2) = resolve("nuulagam", "varisai").expect("roman twin resolves");
        assert!(matches!(v2, StdVariant::Twin));
        assert_eq!(m.module_name(v2), "வரிசை");
        assert!(m.source(v2).contains("தொகுதி வரிசை"));
    }

    #[test]
    fn unknown_module_is_none_and_available_lists_stems() {
        assert!(resolve("std", "nope").is_none());
        let avail = available();
        for stem in ["debouncer", "fifo", "pwm", "seg7", "uart_tx"] {
            assert!(avail.contains(stem), "available() missing {stem}");
        }
    }

    #[test]
    fn every_embedded_module_has_no_imports() {
        // Invariant: std modules are self-contained (the embedded branch does
        // not walk transitive imports). Guard it.
        for m in modules() {
            for src in [m.canonical_src, m.twin_src] {
                let toks = crate::lexer::lex(src).expect("std module lexes");
                let ast = crate::parser::parse(toks).expect("std module parses");
                assert!(ast.imports.is_empty(), "{} must have no imports", m.stem);
            }
        }
    }
}
