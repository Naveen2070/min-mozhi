//! End-to-end proof that qualified references (`a.b.Name`) genuinely
//! disambiguate two different files' same-named module through the REAL
//! `mimz` binary and the real `project.rs` loader — not a hand-wired
//! `resolved_file` like the unit tests in `src/checker/tests.rs` and
//! `src/emit_verilog/mod.rs`. This is the fixture the original
//! packages/namespacing plan never had (added after Task 9 found the
//! resolution mechanism was originally missing): if qualified resolution
//! silently picked the wrong file (or the same file twice), this test
//! would catch it.

use std::path::{Path, PathBuf};
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

// ---- Icarus invocation — mirrors tests/icarus.rs's helpers (a separate
// integration-test binary, so the logic is copied rather than shared; keep
// this trimmed to just what this file needs). ----

/// Locate the Icarus `bin` directory: `MIMZ_IVERILOG` (a directory or the
/// iverilog executable itself) -> PATH -> the Windows installer default.
/// `None` means "not installed" — the caller decides skip vs fail.
fn iverilog_bin() -> Option<PathBuf> {
    let exe = |dir: &Path| dir.join(format!("iverilog{}", std::env::consts::EXE_SUFFIX));
    if let Ok(p) = std::env::var("MIMZ_IVERILOG") {
        let p = PathBuf::from(p);
        let dir = if p.is_file() {
            p.parent().map(Path::to_path_buf).unwrap_or_default()
        } else {
            p
        };
        return exe(&dir).exists().then_some(dir);
    }
    if Command::new("iverilog")
        .arg("-V")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return Some(PathBuf::new()); // empty = resolve via PATH
    }
    let default = PathBuf::from(r"C:\iverilog\bin");
    if cfg!(windows) && exe(&default).exists() {
        return Some(default);
    }
    None
}

/// `Some(bin dir)` to run, `None` to skip (already logged). Panics when
/// CI demands Icarus (`REQUIRE_IVERILOG`) but it is missing.
fn require_iverilog() -> Option<PathBuf> {
    match iverilog_bin() {
        Some(d) => Some(d),
        None => {
            assert!(
                std::env::var("REQUIRE_IVERILOG").is_err(),
                "REQUIRE_IVERILOG is set but iverilog was not found — \
                 install it (CI: apt-get install -y iverilog)"
            );
            eprintln!("skipping: Icarus Verilog not installed (docs/code/10-test-map.md)");
            None
        }
    }
}

fn tool(bin: &Path, name: &str) -> Command {
    if bin.as_os_str().is_empty() {
        Command::new(name)
    } else {
        Command::new(bin.join(name))
    }
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
///
/// This is also the acceptance test for Task 13 (emitter disambiguation):
/// the two same-named `Fifo` declarations must emit under DIFFERENT Verilog
/// identifiers (checked structurally below, without hardcoding the exact
/// suffix scheme), and the resulting `.v` text must be valid, correctly
/// bound Verilog to a REAL toolchain (Icarus) — not just "no diagnostics
/// from our own checker". Before Task 13 this fails: both declarations
/// emit as literally `module Fifo`, which Icarus rejects outright with a
/// duplicate-declaration error.
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
    assert!(
        v.contains("assign y = 1;"),
        "qual_a's Fifo body (y = 1) must be emitted:\n{v}"
    );
    assert!(
        v.contains("assign y = 0;"),
        "qual_b's Fifo body (y = 0) must be emitted:\n{v}"
    );

    // The two `Fifo` declarations must be emitted under two DIFFERENT
    // Verilog identifiers — a bare `module Fifo (` declared twice is a
    // duplicate-declaration error in real Verilog.
    let decl_lines: Vec<&str> = v.lines().filter(|l| l.starts_with("module Fifo")).collect();
    assert_eq!(
        decl_lines.len(),
        2,
        "expected two `Fifo` module declarations:\n{v}"
    );
    assert_ne!(
        decl_lines[0], decl_lines[1],
        "the two same-named `Fifo` declarations must use distinct Verilog \
         identifiers, not collide as literally `module Fifo`:\n{v}"
    );

    // The load-bearing check: feed the emitted Verilog to a REAL toolchain.
    // Our own checker/emitter substring asserts above only check OUR
    // expectations — Icarus is the independent judge of "is this valid,
    // correctly-bound Verilog" (mirrors tests/icarus.rs's own rationale).
    let Some(bin) = require_iverilog() else {
        return;
    };
    let icarus_out = tool(&bin, "iverilog")
        .args(["-t", "null"])
        .arg(&out_v)
        .output()
        .unwrap();
    assert!(
        icarus_out.status.success(),
        "iverilog rejected the emitted Verilog for the same-named cross-file \
         `Fifo` fixture:\n{}",
        String::from_utf8_lossy(&icarus_out.stderr)
    );
}
