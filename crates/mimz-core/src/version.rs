//! The two version axes, kept deliberately distinct — like `rustc 1.x` (the
//! compiler) vs Rust edition `2021` (the language). Conflating them is the
//! confusion this module removes.
//!
//! - **Compiler version** — the crate version in `Cargo.toml`, surfaced as
//!   [`COMPILER_VERSION`] (`env!("CARGO_PKG_VERSION")`). Advances every release.
//! - **Language edition** — a freeform variant codename + year + code, whose
//!   single source of truth is [`EDITION_HISTORY`] below. Advances only when the
//!   language itself changes (a new keyword set / breaking change → a new
//!   edition + `mimz translate` migration, per R13).
//!
//! Both are surfaced together, uname-style, by `mimz --version` (variant on
//! top), in the emitted Verilog header, and mirrored for humans in
//! `CHANGELOG.md`. The edition history table is intentionally surfaced in source
//! so the language's lineage lives next to the compiler that implements it.

/// The compiler (crate) version. Single source: `Cargo.toml`.
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The keyword-set version this compiler implements. Aligned with the current
/// edition's `code` and cross-checked against `keywords.toml`'s `version` by a
/// unit test (so the three can never silently drift).
pub const KEYWORD_SET_VERSION: u8 = 1;

/// One language edition: a freeform variant codename (an animal/object label,
/// **not** R9-bound — like Ubuntu's "Jammy Jellyfish"), the year it was set, and
/// a per-year code. `slug` is the kebab-case form used in the edition string.
#[derive(Clone, Copy, Debug)]
pub struct Edition {
    /// Display codename, e.g. `"Wingless Butterfly"`.
    pub variant: &'static str,
    /// Kebab-case slug for the edition string, e.g. `"wingless-butterfly"`.
    pub slug: &'static str,
    /// The calendar year this edition was set.
    pub year: u16,
    /// Per-year edition code (monotonic within a year; tracks `KEYWORD_SET_VERSION`).
    pub code: u8,
    /// The date this edition was set (`YYYY-MM-DD`).
    pub date: &'static str,
    /// One-line summary of what this edition introduced (mirrored in CHANGELOG).
    pub summary: &'static str,
}

impl Edition {
    /// The edition string `<slug>-<year>-<code>`, e.g. `wingless-butterfly-2026-1`.
    pub fn tag(&self) -> String {
        format!("{}-{}-{}", self.slug, self.year, self.code)
    }
}

/// The edition history — the language axis's source of truth, surfaced in
/// source. One row per edition, **oldest first**; the LAST row is the current
/// edition (asserted by the unit tests). `CHANGELOG.md` mirrors this for humans.
pub const EDITION_HISTORY: &[Edition] = &[Edition {
    variant: "Wingless Butterfly",
    slug: "wingless-butterfly",
    year: 2026,
    code: 1,
    date: "2026-06-17",
    summary: "First edition — keyword set v1 plus the pre-freeze RTL-parity batch \
              (replication, don't-care match patterns, `on fall`, memories `mem`, \
              `async reset`).",
}];

/// The current (newest) language edition.
pub fn current() -> &'static Edition {
    EDITION_HISTORY
        .last()
        .expect("EDITION_HISTORY always has at least the first edition")
}

/// The uname-style block printed by `mimz --version` — the variant codename on
/// top, then the two axes labelled `(compiler)` and `(language)`.
pub fn version_block() -> String {
    let e = current();
    format!(
        "{variant}\n\
         {l1:<8}{cv:<28}(compiler)\n\
         {l2:<8}{tag:<28}(language)",
        variant = e.variant,
        l1 = "mimz",
        cv = COMPILER_VERSION,
        l2 = "edition",
        tag = e.tag(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_is_the_last_history_row() {
        // The newest edition is the tail; a new edition is APPENDED, never
        // inserted, so the lineage stays ordered.
        let last = EDITION_HISTORY.last().unwrap();
        assert_eq!(current().tag(), last.tag());
        // History codes are monotonic (no gaps/reorders).
        for pair in EDITION_HISTORY.windows(2) {
            assert!(
                pair[1].year > pair[0].year
                    || (pair[1].year == pair[0].year && pair[1].code > pair[0].code),
                "EDITION_HISTORY must be ordered oldest-first by (year, code)"
            );
        }
    }

    #[test]
    fn keyword_set_version_matches_keywords_toml() {
        // The compiler's declared keyword-set version must equal the data file's
        // `version = N`, and the current edition's `code` aligns with it.
        assert_eq!(
            KEYWORD_SET_VERSION,
            crate::lexer::keywords::TABLE.version(),
            "KEYWORD_SET_VERSION disagrees with keywords.toml `version` — bump both together"
        );
        assert_eq!(
            current().code,
            KEYWORD_SET_VERSION,
            "the current edition's code should track the keyword-set version"
        );
    }

    #[test]
    fn version_block_shows_both_axes() {
        let b = version_block();
        assert!(b.contains(current().variant), "variant codename on top");
        assert!(b.contains(COMPILER_VERSION), "compiler version line");
        assert!(b.contains(&current().tag()), "edition string");
        assert!(b.contains("(compiler)") && b.contains("(language)"));
    }
}
