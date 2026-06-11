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

/// Top-level modules declared in src/main.rs (`mod name;` lines).
fn crate_modules() -> Vec<String> {
    read("src/main.rs")
        .lines()
        .filter_map(|l| {
            l.strip_prefix("mod ")
                .and_then(|r| r.strip_suffix(';'))
                .map(str::to_string)
        })
        .collect()
}

/// The crate map lives in TWO places — the `//!` table in src/main.rs and
/// docs/code/README.md. This keeps both honest: add a module and forget
/// either copy, and this fails naming the place to fix.
#[test]
fn crate_map_lists_every_module() {
    let modules = crate_modules();
    assert!(
        modules.len() >= 7,
        "expected the known modules, found {modules:?}"
    );
    let main = read("src/main.rs");
    let readme_lower = read("docs/code/README.md").to_lowercase();
    for m in &modules {
        assert!(
            main.contains(&format!("[`{m}`]")),
            "src/main.rs crate-map table has no row for module `{m}` — update the //! table"
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
        for entry in fs::read_dir(root().join("src").join(dir)).unwrap() {
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
