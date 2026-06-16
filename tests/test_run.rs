//! Integration: `mimz test` — the real binary runs `test` blocks, reports
//! pass/fail with teaching messages, and sets the exit code (Phase 1.5, B6).

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

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
