//! `extern module` (Verilog FFI) — end-to-end CLI coverage for Task 9's
//! config/CLI plumbing: `[compile] verilog_files` + `--extern-src` union
//! (`mimz compile`), and `extern_sim` / `--extern-sim` mode selection
//! (`mimz sim`/`mimz test`). Task 10 extends this file with fixture-backed
//! sweep-exclusion coverage.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

/// A valid extern-module declaration + instantiation that checks clean
/// (mirrors `crates/mimz-core/src/checker/tests.rs`'s
/// `extern_module_valid_instantiation_checks_clean` fixture) — used for the
/// `mimz compile` union test, which must get past the checker to prove the
/// merged `verilog_files` list is what's actually wired through.
const VALID_EXTERN_MODULE: &str = "extern module Pll(MULT: int = 2) {\n  \
    clock clk_in\n  out clk_out: bit\n  out locked: bit\n}\n\
    module M {\n  clock sysclk\n  out fast: bit\n  out ok: bit\n  \
    let u = Pll(MULT: 4) { clk_in: sysclk }\n  fast = u.clk_out\n  ok = u.locked\n}\n";

/// Unique scratch dir per test (parallel `cargo test` runs share no state).
fn temp_dir(tag: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mimz_extern_{tag}_{}_{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// `mimz compile --extern-src <path>` against a fixture whose `mimz.toml`
/// already lists a different `[compile] verilog_files` entry: both must be
/// present in the merged list afterward — an additive union, CLI never
/// overriding config. Surfaced via `--debug`'s stderr echo (`src/commands/
/// compile.rs`), confirmed by actually running the built binary and reading
/// its real stderr rather than assuming a format.
#[test]
fn extern_src_cli_flag_unions_with_mimz_toml_verilog_files() {
    let dir = temp_dir("union");
    fs::write(
        dir.join("mimz.toml"),
        "[compile]\nverilog_files = [\"vendor/pll.v\"]\n",
    )
    .unwrap();
    let design = dir.join("top.mimz");
    fs::write(&design, VALID_EXTERN_MODULE).unwrap();

    let out = mimz()
        .arg("compile")
        .arg(&design)
        .args(["--extern-src", "vendor/ddr.v"])
        .arg("--debug")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "compile should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("vendor/pll.v") && err.contains("vendor/ddr.v"),
        "merged verilog_files should list both the config entry and the \
         --extern-src flag: {err}"
    );

    fs::remove_dir_all(&dir).ok();
}

/// `mimz test --extern-sim strict` against a fixture instantiating an extern
/// module inside the module under test must exit non-zero, naming the extern
/// instance, before running any cycles (no `tick`s are consumed — the error
/// comes from elaboration, not a failed `expect`). The default (`warn`) mode
/// must NOT fail the same fixture, proving `--extern-sim` actually changes
/// behavior rather than the fixture being broken outright.
#[test]
fn extern_sim_strict_flag_makes_mimz_test_fail_fast() {
    let dir = temp_dir("strict");
    let design = dir.join("pll_test.mimz");
    fs::write(
        &design,
        "extern module Pll {\n  in clk_in: bit\n  out locked: bit\n}\n\
         module M {\n  clock sysclk\n  out ok: bit\n  \
         let u = Pll() { clk_in: sysclk, locked: ok }\n  ok = u.locked\n}\n\
         test \"extern strict\" for M {\n  tick(sysclk)\n  expect ok == 0\n}\n",
    )
    .unwrap();

    // Default mode (warn): the extern instance is stubbed to X, not an error.
    let warn_out = mimz().arg("test").arg(&design).output().unwrap();
    assert!(
        warn_out.status.success(),
        "warn (default) mode should not fail on an extern instance: {}",
        String::from_utf8_lossy(&warn_out.stdout)
    );

    // `--extern-sim strict`: a hard error before any cycle runs.
    let strict_out = mimz()
        .arg("test")
        .arg(&design)
        .args(["--extern-sim", "strict"])
        .output()
        .unwrap();
    assert!(
        !strict_out.status.success(),
        "--extern-sim strict should fail on an extern instance"
    );
    let s = format!(
        "{}{}",
        String::from_utf8_lossy(&strict_out.stdout),
        String::from_utf8_lossy(&strict_out.stderr)
    );
    assert!(s.contains('u'), "error should name the instance `u`: {s}");
    assert!(
        s.contains("Pll"),
        "error should name the extern module `Pll`: {s}"
    );

    fs::remove_dir_all(&dir).ok();
}
