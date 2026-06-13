//! End-to-end checks for `mimz eval` — the combinational evaluator's CLI.
//!
//! Runs the REAL binary on corpus examples and confirms the printed outputs
//! (so the lib evaluator AND the `--in`/`--module` argument plumbing are both
//! exercised). The deeper truth-table coverage lives in the unit tests in
//! `src/sim/comb.rs`; this proves a user gets the right answer on the CLI.

use std::path::PathBuf;
use std::process::Command;

fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("english")
        .join(name)
}

/// Run `mimz eval <example> <extra args>` and return (success, stdout, stderr).
fn eval(name: &str, args: &[&str]) -> (bool, String, String) {
    let out = mimz()
        .arg("eval")
        .arg(example(name))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn adder_carries() {
    let (ok, out, _) = eval("adder.mimz", &["--in", "a=200,b=100"]);
    assert!(ok);
    assert!(out.contains("sum = 300"), "got: {out}");
}

#[test]
fn mux4_selects_with_hex_and_binary_inputs() {
    let (ok, out, _) = eval("mux4.mimz", &["--in", "sel=0b10,a=10,b=20,c=30,d=40"]);
    assert!(ok);
    assert!(out.contains("y = 30"), "got: {out}");
}

#[test]
fn comparator_reports_all_three_outputs() {
    let (ok, out, _) = eval("comparator.mimz", &["--in", "a=7,b=3"]);
    assert!(ok);
    assert!(out.contains("eq = 0"), "got: {out}");
    assert!(out.contains("gt = 1"), "got: {out}");
    assert!(out.contains("max = 7"), "got: {out}");
}

#[test]
fn window_chained_comparison_boundaries() {
    let (_, inside, _) = eval("window.mimz", &["--in", "lo=10,value=100,hi=100"]);
    assert!(
        inside.contains("in_range = 1"),
        "boundary inclusive: {inside}"
    );
    let (_, below, _) = eval("window.mimz", &["--in", "lo=10,value=5,hi=100"]);
    assert!(below.contains("in_range = 0"), "below: {below}");
}

#[test]
fn multi_module_file_needs_module_flag() {
    // alu.mimz defines Alu and Top — eval must ask which, then accept --module.
    let (ok, _, err) = eval("alu.mimz", &["--in", "a=1,b=1,op=0"]);
    assert!(!ok);
    assert!(
        err.contains("2 modules"),
        "expected the disambiguation message: {err}"
    );

    let (ok, out, _) = eval(
        "alu.mimz",
        &["--module", "Alu", "--in", "a=12,b=10,op=0b11"],
    );
    assert!(ok);
    assert!(out.contains("y = 14"), "12 | 10 = 14, got: {out}");
}

#[test]
fn instances_are_rejected_clearly() {
    // chained.mimz wires up sub-modules — out of the combinational slice's scope.
    let (ok, _, err) = eval("chained.mimz", &["--in", "a0=1,a1=0,b0=1,b1=1,cin=0"]);
    assert!(!ok);
    assert!(
        err.contains("sub-module"),
        "expected an instance rejection: {err}"
    );
}
