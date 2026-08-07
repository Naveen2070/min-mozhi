//! Integration: `mimz test` — the real binary runs `test` blocks, reports
//! pass/fail with teaching messages, and sets the exit code (Phase 1.5, B6).

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use mimz::sim::run::MAX_SIM_CYCLES;

fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

/// Write `src` to a unique temp `.mimz` and return its path.
fn temp_mimz(src: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let p = std::env::temp_dir().join(format!(
        "mimz_test_{}.mimz",
        N.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&p, src).unwrap();
    p
}

const COUNTER: &str = "module Counter(WIDTH: int = 4) {\n  clock clk\n  reset rst\n  \
    out count: bits[WIDTH]\n  reg value: bits[WIDTH] = 0\n  \
    on rise(clk) { value <- value +% 1 }\n  count = value\n}\n";

#[test]
fn a_passing_test_exits_zero() {
    let src = format!(
        "{COUNTER}\ntest \"counts up\" for Counter(WIDTH: 4) {{\n  \
         rst = 1\n  tick(clk)\n  expect count == 0\n  \
         rst = 0\n  tick(clk, 4)\n  expect count == 4\n}}\n"
    );
    let p = temp_mimz(&src);
    let out = mimz().args(["test"]).arg(&p).output().unwrap();
    assert!(out.status.success(), "test should pass: {:?}", out);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("ok"), "no ok line:\n{s}");
    assert!(s.contains("1 passed, 0 failed"), "no summary:\n{s}");
}

#[test]
fn mimz_test_prints_a_coverage_summary() {
    let src = "module M {\n  clock clk\n  reset rst\n  out y: bit\n  reg r: bit = 0\n  \
               cover(r == 0)\n  \
               on rise(clk) {\n    r <- 1\n  }\n  y = r\n}\n\
               test \"t\" for M {\n  tick(clk, 2)\n}\n";
    let p = temp_mimz(src);
    let out = mimz().args(["test"]).arg(&p).output().unwrap();
    assert!(out.status.success(), "test should pass: {:?}", out);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("cover"), "no cover line:\n{s}");
}

#[test]
fn a_tick_count_over_the_cycle_limit_errors_fast_not_hangs() {
    // SEC: `tick(clk, n)` with n past MAX_SIM_CYCLES must fail fast with a
    // clean error, NEVER loop n times pushing frames (untrusted-input DoS).
    let over = MAX_SIM_CYCLES + 1;
    let src = format!(
        "{COUNTER}\ntest \"huge\" for Counter(WIDTH: 4) {{\n  \
         rst = 0\n  tick(clk, {over})\n  expect count == 0\n}}\n"
    );
    let p = temp_mimz(&src);
    let out = mimz().args(["test"]).arg(&p).output().unwrap();
    assert!(!out.status.success(), "over-limit tick must exit nonzero");
    let s = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        s.contains("simulation limit"),
        "no cycle-limit message:\n{s}"
    );
}

#[test]
fn a_failing_expect_exits_nonzero_with_a_teaching_message() {
    let src = format!(
        "{COUNTER}\ntest \"wrong\" for Counter(WIDTH: 4) {{\n  \
         rst = 0\n  tick(clk)\n  expect count == 9\n}}\n"
    );
    let p = temp_mimz(&src);
    let out = mimz().args(["test"]).arg(&p).output().unwrap();
    assert!(!out.status.success(), "a failing expect must exit nonzero");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("FAIL"), "no FAIL line:\n{s}");
    assert!(s.contains("count == 9"), "no expression:\n{s}");
    assert!(s.contains("left"), "no operand values:\n{s}");
    assert!(s.contains("0 passed, 1 failed"), "no summary:\n{s}");
}

#[test]
fn the_filter_selects_tests_by_name() {
    let src = format!(
        "{COUNTER}\n\
         test \"alpha\" for Counter(WIDTH: 4) {{\n  rst = 0\n  tick(clk)\n  expect count == 1\n}}\n\
         test \"beta\" for Counter(WIDTH: 4) {{\n  rst = 0\n  tick(clk)\n  expect count == 9\n}}\n"
    );
    let p = temp_mimz(&src);
    // Only `alpha` runs (and passes), so the failing `beta` is skipped.
    let out = mimz()
        .args(["test"])
        .arg(&p)
        .args(["--filter", "alpha"])
        .output()
        .unwrap();
    assert!(out.status.success(), "filtered run should pass: {:?}", out);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("alpha"), "{s}");
    assert!(!s.contains("beta"), "beta should be filtered out:\n{s}");
}

#[test]
fn trace_shows_a_per_cycle_table() {
    let src = format!(
        "{COUNTER}\ntest \"counts\" for Counter(WIDTH: 4) {{\n  \
         rst = 0\n  tick(clk, 3)\n  expect count == 3\n}}\n"
    );
    let p = temp_mimz(&src);
    let out = mimz()
        .args(["test"])
        .arg(&p)
        .args(["--trace"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("cycle"), "no trace header:\n{s}");
    assert!(s.contains("count"), "no count column:\n{s}");
}

#[test]
fn a_file_with_no_tests_is_reported() {
    let p = temp_mimz(COUNTER);
    let out = mimz().args(["test"]).arg(&p).output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("no `test` blocks"), "{s}");
}

// A fully thamizh-order, all-tanglish program: the clocked-block flip
// (`yetram(clk) pothu`) AND the test-header flip (`M(args) kaaga "…" sodhanai`)
// in one file. Execution is the oracle (B7) — it must run and pass exactly like
// its code-order twin, proving the flipped header builds the same `TestDecl`.
const THAMIZH_COUNTER: &str = "ilakkanam thamizh\n\
    thoguthi Counter(WIDTH: int = 4) {\n  thudippu clk\n  meettamai rst\n  \
    veliyeedu count: bits[WIDTH]\n  pathivedu value: bits[WIDTH] = 0\n  \
    yetram(clk) pothu { value <- value +% 1 }\n  count = value\n}\n\n\
    Counter(WIDTH: 4) kaaga \"counts up\" sodhanai {\n  rst = 1\n  kanam(clk)\n  \
    uruthisei count == 0\n  rst = 0\n  kanam(clk, 4)\n  uruthisei count == 4\n}\n";

#[test]
fn a_thamizh_order_test_header_runs_like_its_code_order_twin() {
    let p = temp_mimz(THAMIZH_COUNTER);
    let out = mimz().args(["test"]).arg(&p).output().unwrap();
    assert!(out.status.success(), "thamizh test should pass: {:?}", out);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("ok"), "no ok line:\n{s}");
    assert!(s.contains("1 passed, 0 failed"), "no summary:\n{s}");
}

// BUG-43 (docs/audit/bugs.md): a `test` block's `p = -9` was evaluated to
// a narrow SIGNED `Val` and then handed to `Sim::set` as a raw
// `bits_masked()` pattern — losing the sign, so the port was driven with
// 23 instead of 55 and every negative-stimulus test silently checked the
// wrong input. Drives the whole `signed[6]` table from the filing through
// the real binary, since the defect was in the harness's drive path
// specifically, not in the value model (which `crates/mimz-sim/src/sim/
// value/tests.rs` covers) nor in the emitter (which the Icarus
// differentials in `tests/self_determined_regression.rs` cover).
#[test]
fn a_negative_test_input_drives_its_twos_complement_pattern() {
    for (written, expected) in [
        (-1i32, 63u32),
        (-3, 61),
        (-5, 59),
        (-9, 55),
        (-16, 48),
        (-17, 47),
        (-32, 32),
    ] {
        let src = format!(
            "module NG {{\n  in p: signed[6]\n  out u: bits[6]\n  u = unsigned(p)\n}}\n\n\
             test \"drives {written}\" for NG {{\n  p = {written}\n  \
             expect u == {expected}\n}}\n"
        );
        let p = temp_mimz(&src);
        let out = mimz().args(["test"]).arg(&p).output().unwrap();
        assert!(
            out.status.success(),
            "`p = {written}` must drive {expected} on a signed[6] port:\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }
}
