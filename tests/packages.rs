//! End-to-end proof that qualified references (`a.b.Name`) genuinely
//! disambiguate two different files' same-named module through the REAL
//! `mimz` binary and the real `project.rs` loader — not a hand-wired
//! `resolved_file` like the unit tests in `src/checker/tests.rs` and
//! `src/emit_verilog/mod.rs`. This is the fixture the original
//! packages/namespacing plan never had (added after Task 9 found the
//! resolution mechanism was originally missing): if qualified resolution
//! silently picked the wrong file (or the same file twice), this test
//! would catch it.

use std::path::PathBuf;
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("packages")
        .join(name)
}

fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

/// `qual_top.mimz` imports `qual_a.fifo` and `qual_b.fifo`, both of which
/// declare a module named `Fifo` with a different body, and instantiates
/// each via its own qualified path (`qual_a.fifo.Fifo` / `qual_b.fifo.Fifo`).
/// Must check clean — zero diagnostics, no E0110 (ambiguous) or E0111
/// (unmatched qualifier).
#[test]
fn qualified_references_check_clean_with_zero_diagnostics() {
    let out = mimz()
        .arg("check")
        .arg(fixture("qual_top.mimz"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "`mimz check qual_top.mimz` should succeed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "[]",
        "zero diagnostics expected — both qualified references must resolve \
         cleanly without E0110/E0111"
    );
}

/// Beyond "no diagnostics": prove the two qualified instantiations actually
/// picked their DISTINCT, correct target files, not e.g. both silently
/// resolving to the same one. `qual_a/fifo.mimz`'s `Fifo` drives `y = 1`;
/// `qual_b/fifo.mimz`'s `Fifo` drives `y = 0` — both bodies must appear in
/// the compiled Verilog, each wired to its own instance.
#[test]
fn qualified_instances_compile_with_their_own_distinct_bodies() {
    let out_v = std::env::temp_dir().join("mimz_test_qual_top.v");
    let out = mimz()
        .arg("compile")
        .arg(fixture("qual_top.mimz"))
        .arg("-o")
        .arg(&out_v)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "`mimz compile qual_top.mimz` should succeed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = std::fs::read_to_string(&out_v).unwrap();
    assert!(v.contains("Fifo a ("), "instance `a` must be emitted");
    assert!(v.contains("Fifo b ("), "instance `b` must be emitted");
    assert!(
        v.contains("assign y = 1;"),
        "qual_a's Fifo body (y = 1) must be emitted:\n{v}"
    );
    assert!(
        v.contains("assign y = 0;"),
        "qual_b's Fifo body (y = 0) must be emitted:\n{v}"
    );
}
