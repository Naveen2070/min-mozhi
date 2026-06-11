//! Icarus Verilog differential tests (Phase 1 plan item 5).
//!
//! Two layers, both against the REAL tool:
//! 1. Every example's emitted Verilog must be valid to `iverilog -t null`
//!    (syntax + elaboration — our own substring asserts only check OUR
//!    expectations).
//! 2. A hand-written, SELF-CHECKING testbench per base example encodes
//!    Min-Mozhi's documented semantics (`+%` wraps, lossless `+` grows,
//!    sync reset, non-blocking `<-`, …) and runs under `vvp`. Icarus
//!    agreeing with the testbench is the differential: two independent
//!    interpretations of the same program, compared.
//!
//! If Icarus is not installed the tests SKIP with a note (so machines
//! without it stay green); in CI `REQUIRE_IVERILOG=1` turns a missing
//! install into a failure, so CI can never skip silently. Local install:
//! the Windows installer (bleyer.org/icarus) or `apt-get install iverilog`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Testbench file (under tests/icarus/) -> the example it tests.
/// Testbench module name = file name minus `.v`.
const TESTBENCHES: [(&str, &str); 11] = [
    ("adder_tb.v", "english/adder.mimz"),
    ("alu_tb.v", "english/alu.mimz"),
    ("blinker_tb.v", "english/blinker.mimz"),
    ("chained_tb.v", "english/chained.mimz"),
    ("comparator_tb.v", "english/comparator.mimz"),
    ("counter_tb.v", "english/counter.mimz"),
    ("edge_detector_tb.v", "english/edge_detector.mimz"),
    ("full_adder_tb.v", "english/lib/full_adder.mimz"),
    ("mux4_tb.v", "english/mux4.mimz"),
    ("shift_register_tb.v", "english/shift_register.mimz"),
    ("traffic_light_tb.v", "english/traffic_light.mimz"),
];

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

/// Locate the Icarus `bin` directory: `MIMZ_IVERILOG` (a directory or the
/// iverilog executable itself) → PATH → the Windows installer default.
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

/// Compile one example with mimz; return the generated `.v` path.
fn compile_example(path: &Path) -> PathBuf {
    let name = path.display().to_string().replace(['\\', '/', ':'], "_");
    let out_v = std::env::temp_dir().join(format!("mimz_icarus_{name}.v"));
    let out = mimz()
        .arg("compile")
        .arg(path)
        .arg("-o")
        .arg(&out_v)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "`mimz compile {}` failed:\n{}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    out_v
}

/// Layer 1 — every emitted `.v` in the corpus is valid Verilog by the
/// judgment of a real tool, not just our substring asserts.
#[test]
fn every_emitted_verilog_passes_iverilog() {
    let Some(bin) = require_iverilog() else {
        return;
    };
    let mut checked = 0;
    let mut stack = vec![repo().join("examples")];
    let mut files = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "mimz") {
                files.push(path);
            }
        }
    }
    files.sort();
    for path in files {
        let v = compile_example(&path);
        let out = tool(&bin, "iverilog")
            .args(["-t", "null"])
            .arg(&v)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "iverilog rejected the Verilog emitted for {}:\n{}",
            path.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        checked += 1;
    }
    assert!(checked >= 44, "expected the whole corpus, found {checked}");
}

/// Layer 2 — the self-checking testbenches: Min-Mozhi semantics encoded
/// in Verilog asserts, simulated by Icarus. Each prints PASS exactly once
/// or FAIL with a reason.
#[test]
fn self_checking_testbenches_pass() {
    let Some(bin) = require_iverilog() else {
        return;
    };
    for (tb_file, example) in TESTBENCHES {
        let tb = repo().join("tests").join("icarus").join(tb_file);
        assert!(tb.exists(), "missing testbench {}", tb.display());
        let design = compile_example(&repo().join("examples").join(example));
        let tb_module = tb_file.trim_end_matches(".v");
        let vvp_out = std::env::temp_dir().join(format!("mimz_icarus_{tb_module}.vvp"));

        let out = tool(&bin, "iverilog")
            .arg("-o")
            .arg(&vvp_out)
            .args(["-s", tb_module])
            .arg(&tb)
            .arg(&design)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "iverilog failed on {tb_file} + {example}:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );

        let sim = tool(&bin, "vvp").arg(&vvp_out).output().unwrap();
        let stdout = String::from_utf8_lossy(&sim.stdout);
        assert!(
            sim.status.success(),
            "vvp failed on {tb_module}:\n{stdout}\n{}",
            String::from_utf8_lossy(&sim.stderr)
        );
        assert!(
            !stdout.contains("FAIL"),
            "{tb_module} reported a semantic mismatch:\n{stdout}"
        );
        assert!(
            stdout.contains("PASS"),
            "{tb_module} never reached PASS (testbench bug?):\n{stdout}"
        );
    }
}
