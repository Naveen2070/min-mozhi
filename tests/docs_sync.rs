//! Mechanical staleness guard for the maintainer docs (docs/code/).
//!
//! CI cannot verify that prose is TRUE, but it can verify the structural
//! facts the docs state: which modules exist, which files each module
//! page lists. When this test fails, the docs drifted from the code —
//! update the named page (RULES R1), don't weaken the test.

use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("cannot read {rel}: {e}"))
}

/// `pub mod name;` lines in a crate's lib.rs (one per line, no attributes).
fn pub_mod_names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            l.strip_prefix("pub mod ")
                .and_then(|r| r.strip_suffix(';'))
                .map(str::to_string)
        })
        .collect()
}

/// Names inside a `pub use CRATE::{a, b, c};` brace list in `text` — direct,
/// single-level paths only. A nested path like `mimz_sim::runner::{...}`
/// also starts with `pub use mimz_sim::`, but what follows isn't `{`, so it's
/// skipped here (those are function re-exports, not module re-exports).
/// Handles the brace list spanning multiple lines (src/lib.rs wraps its
/// mimz-core re-export across several).
fn brace_use_names(text: &str, krate: &str) -> Vec<String> {
    let prefix = format!("pub use {krate}::");
    let mut names = Vec::new();
    let mut from = 0;
    while let Some(rel) = text[from..].find(&prefix) {
        let pos = from + rel;
        let after = &text[pos + prefix.len()..];
        if let Some(body) = after.trim_start().strip_prefix('{') {
            // Assumes no nested braces inside the pub use list (e.g., no `pub use mimz_core::{ast::{Node}, ...}`).
            // If a re-export ever nests braces here, this finds the first `}` and silently truncates names instead of erroring.
            if let Some(end) = body.find('}') {
                names.extend(
                    body[..end]
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string),
                );
            }
        }
        from = pos + prefix.len();
    }
    names
}

/// Top-level modules exposed as `mimz::*`: shell-native `pub mod` lines in
/// src/lib.rs (the lib/bin split of 2026-06-12 moved the module tree there),
/// plus mimz-core/mimz-sim modules re-exported via `pub use CRATE::{...};`
/// (the 3-crate facade, Task 9). A single-item re-export like
/// `pub use mimz_core::REPEAT_BUDGET;` isn't a brace list, so it's never
/// picked up by `brace_use_names`; brace-list names that aren't modules
/// (e.g. the `compile_string`/`run_command` functions re-exported alongside
/// `sim` from mimz-sim) are dropped by cross-checking against each source
/// crate's own `pub mod` list.
fn crate_modules() -> Vec<String> {
    let lib = read("src/lib.rs");
    let mut modules = pub_mod_names(&lib);

    let core_mods = pub_mod_names(&read("crates/mimz-core/src/lib.rs"));
    modules.extend(
        brace_use_names(&lib, "mimz_core")
            .into_iter()
            .filter(|n| core_mods.contains(n)),
    );

    let sim_mods = pub_mod_names(&read("crates/mimz-sim/src/lib.rs"));
    modules.extend(
        brace_use_names(&lib, "mimz_sim")
            .into_iter()
            .filter(|n| sim_mods.contains(n)),
    );

    modules
}

/// The crate map lives in TWO places — the `//!` table in src/lib.rs and
/// docs/code/README.md. This keeps both honest: add a module and forget
/// either copy, and this fails naming the place to fix.
#[test]
fn crate_map_lists_every_module() {
    let modules = crate_modules();
    assert!(
        modules.len() >= 7,
        "expected the known modules, found {modules:?}"
    );
    let lib = read("src/lib.rs");
    let readme_lower = read("docs/code/README.md").to_lowercase();
    for m in &modules {
        assert!(
            lib.contains(&format!("[`{m}`]")),
            "src/lib.rs crate-map table has no row for module `{m}` — update the //! table"
        );
        assert!(
            readme_lower.contains(&m.to_lowercase()),
            "docs/code/README.md never mentions module `{m}` — update the 60-second overview"
        );
    }
}

/// Each per-module page has a file-layout table; every .rs file in the
/// corresponding src/ directory must appear in it.
#[test]
fn module_pages_list_every_source_file() {
    let pages = [
        ("lexer", "02-lexer.md"),
        ("parser", "03-parser.md"),
        ("ast", "04-ast.md"),
        ("checker", "11-checker.md"),
        ("emit_verilog", "05-emit-verilog.md"),
    ];
    for (dir, page) in pages {
        let text = read(&format!("docs/code/{page}"));
        for entry in fs::read_dir(root().join("crates/mimz-core/src").join(dir)).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if name.ends_with(".rs") {
                assert!(
                    text.contains(&format!("`{name}`")),
                    "docs/code/{page} does not mention `{name}` — its file-layout table is stale"
                );
            }
        }
    }
}

/// New top-level src/ modules need a docs/code/ page (or a deliberate
/// mention in an existing one). Fires when e.g. src/checker/ appears.
#[test]
fn every_module_is_documented_somewhere_in_docs_code() {
    let mut all_docs = String::new();
    for entry in fs::read_dir(root().join("docs/code")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "md") {
            all_docs.push_str(&fs::read_to_string(&path).unwrap().to_lowercase());
        }
    }
    for m in crate_modules() {
        assert!(
            all_docs.contains(&m.to_lowercase()),
            "no page in docs/code/ mentions module `{m}` — document it (new pipeline stage ⇒ new page)"
        );
    }
}

/// The index carries a "last synced" stamp — the human tripwire for
/// prose staleness that this file can't check mechanically.
#[test]
fn code_docs_have_a_sync_stamp() {
    assert!(
        read("docs/code/README.md").contains("Last synced"),
        "docs/code/README.md lost its 'Last synced' stamp"
    );
}

/// Count `#[test]` functions across the workspace the way cargo counts
/// them: every `.rs` file under these roots, lines whose trimmed form
/// starts with the attribute (mid-line mentions inside docs/strings don't
/// count — verified line-for-line against a full `cargo test --workspace`
/// run: per-root sums matched every suite's own passed-count exactly).
fn count_unit_tests() -> usize {
    fn walk(dir: &std::path::Path, total: &mut usize) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, total);
            } else if path.extension().is_some_and(|e| e == "rs") {
                for line in fs::read_to_string(&path).unwrap().lines() {
                    if line.trim_start().starts_with("#[test]") {
                        *total += 1;
                    }
                }
            }
        }
    }
    let roots = [
        "crates/mimz-core/src",
        "crates/mimz-core/tests",
        "crates/mimz-sim/src",
        "crates/mimz-sim/tests",
        "crates/mimz-wasm/src",
        "src",
        "tests",
    ];
    let mut total = 0;
    for r in roots {
        walk(&root().join(r), &mut total);
    }
    total
}

/// The test count in docs/code/10-test-map.md and the README badge must
/// match the live workspace total (counted from source, not pinned — a new
/// `#[test]` anywhere updates the number this enforces). This prevents the
/// badge/docs from drifting from reality.
#[test]
fn test_count_matches_docs_and_badge() {
    let expected_total = count_unit_tests();

    // 1. Check docs/code/10-test-map.md has the expected total
    let test_map = read("docs/code/10-test-map.md");
    let expected_line = format!("**{} tests**", expected_total);
    assert!(
        test_map.contains(&expected_line),
        "docs/code/10-test-map.md master count should say **{} tests** — update the page",
        expected_total
    );

    // 2. Check README.md badge matches
    let readme = read("README.md");
    let badge_pattern = format!("tests-{}%", expected_total);
    assert!(
        readme.contains(&badge_pattern) || readme.contains(&format!("tests-{} ", expected_total)),
        "README.md badge does not match expected test count {} — update the badge URL",
        expected_total
    );

    // 3. Check ROADMAP.md phase table references the right test counts (indirectly via status)
    // This is a soft check - just verify the file mentions the current scale
    let roadmap = read("ROADMAP.md");
    assert!(
        roadmap.contains("v0.2.0"),
        "ROADMAP.md should reference v0.2.0 release"
    );
}

/// Localized-code coverage must be stated identically everywhere from ONE
/// computation: count `[message.*]` keys in `lang/messages.toml`, read the
/// declared length of `ALL_CHECKER_CODES` out of `diag.rs`, then require both
/// present-tense doc claim sites to carry exactly those numbers. Before the
/// 2026-08-22 doc audit these counts existed in four conflicting forms
/// ("33 of 44", "33 of 36", "34 of 75", "35 of 76"); this kills that drift.
#[test]
fn localized_code_count_matches_docs() {
    // localized = number of [message.Exxxx] section keys in the catalog
    let catalog = read("lang/messages.toml");
    let localized = catalog
        .lines()
        .filter(|l| l.trim_start().starts_with("[message.E"))
        .count();
    assert!(
        localized > 0,
        "lang/messages.toml lost its [message.*] entries"
    );

    // total = the declared array length: `pub const ALL_CHECKER_CODES: [&str; N]`
    let diag = read("crates/mimz-core/src/diag.rs");
    let marker = "pub const ALL_CHECKER_CODES: [&str; ";
    let start = match diag.find(marker) {
        Some(i) => i + marker.len(),
        None => panic!("ALL_CHECKER_CODES moved or renamed in crates/mimz-core/src/diag.rs"),
    };
    let digits: String = diag[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let total: usize = digits
        .parse()
        .unwrap_or_else(|e| panic!("ALL_CHECKER_CODES length `{digits}` not a number: {e}"));

    // membership (every key is a real checker code) is asserted by tests/morph.rs;
    // here we only sanity-check magnitude before trusting it against prose.
    assert!(
        localized <= total,
        "{localized} localized entries exceed the {total}-code checker registry"
    );

    // Claim site 1: ROADMAP error-catalog row.
    let roadmap_claim = format!("{localized} of the {total} checker codes localized");
    assert!(
        read("ROADMAP.md").contains(&roadmap_claim),
        "ROADMAP.md error-catalog row should read \"{roadmap_claim}\" — update the row to the computed counts"
    );

    // Claim site 2: spec/04 grammar-engine bullet.
    let spec_claim = format!("**{localized} of the (now {total}) checker E-codes**");
    assert!(
        read("spec/04-grammar-engine.md").contains(&spec_claim),
        "spec/04-grammar-engine.md error-catalog bullet should read \"{spec_claim}\" — update the bullet to the computed counts"
    );
}
