//! Integration: `mimz sim` — the real binary drives a clocked module under the
//! default stimulus and emits a console trace and/or a VCD (Phase 1.5, B5).

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
        "mimz_sim_{}.mimz",
        N.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&p, src).unwrap();
    p
}

const COUNTER: &str = "module Counter(WIDTH: int = 4) {\n  clock clk\n  reset rst\n  \
    out count: bits[WIDTH]\n  reg value: bits[WIDTH] = 0\n  \
    on rise(clk) { value <- value +% 1 }\n  count = value\n}\n";

#[test]
fn trace_table_shows_a_row_per_cycle() {
    let p = temp_mimz(COUNTER);
    let out = mimz()
        .args(["sim"])
        .arg(&p)
        .args(["--cycles", "4", "--trace"])
        .output()
        .unwrap();
    assert!(out.status.success(), "sim failed: {:?}", out);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("cycle"), "no table header:\n{s}");
    assert!(s.contains("count"), "no count column:\n{s}");
    // header + separator + 4 cycle rows
    assert_eq!(s.lines().count(), 6, "expected 6 lines:\n{s}");
}

#[test]
fn changes_trace_is_monitor_style() {
    let p = temp_mimz(COUNTER);
    let out = mimz()
        .args(["sim"])
        .arg(&p)
        .args(["--cycles", "4", "--trace=changes"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    // counts 0,1,2,3 across the run — the last non-reset edge shows count=3.
    assert!(s.contains("count=3"), "expected $monitor output:\n{s}");
}

#[test]
fn writes_a_gtkwave_vcd() {
    let p = temp_mimz(COUNTER);
    let vcd = std::env::temp_dir().join(format!(
        "mimz_sim_{}.vcd",
        std::process::id() // unique-ish per run
    ));
    let out = mimz()
        .args(["sim"])
        .arg(&p)
        .args(["--cycles", "3", "-o"])
        .arg(&vcd)
        .output()
        .unwrap();
    assert!(out.status.success(), "sim failed: {:?}", out);
    let v = fs::read_to_string(&vcd).unwrap();
    assert!(v.contains("$timescale"), "no timescale:\n{v}");
    assert!(v.contains("$enddefinitions"), "no enddefinitions:\n{v}");
    assert!(v.contains(" count $end"), "no count var:\n{v}");
    assert!(v.contains("$dumpvars"), "no initial dump:\n{v}");
}

#[test]
fn signals_flag_limits_the_trace() {
    let p = temp_mimz(COUNTER);
    let out = mimz()
        .args(["sim"])
        .arg(&p)
        .args(["--cycles", "3", "--trace", "--signals", "count"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("count"), "{s}");
    assert!(!s.contains("value"), "scope should exclude `value`:\n{s}");
}

#[test]
fn a_clockless_module_is_rejected_with_a_pointer_to_eval() {
    let p = temp_mimz("module C {\n  in a: bits[8]\n  out y: bits[8]\n  y = a\n}\n");
    let out = mimz().args(["sim"]).arg(&p).output().unwrap();
    assert!(!out.status.success(), "a clockless module should fail sim");
    let s = String::from_utf8_lossy(&out.stderr);
    assert!(s.contains("no clock") && s.contains("eval"), "got: {s}");
}
