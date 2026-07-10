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
