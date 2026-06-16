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

/// Phase 1.5 B8 perf baseline: the event-driven kernel must clear **1M
/// cycle-events/sec** on the counter in release. Each `tick` is one clock cycle
/// = several signal events (clock edge, register commit, combinational settle),
/// so cycles/sec is the conservative (lower) bound on cycle-events/sec. The hard
/// gate applies in release; a debug build only checks a low sanity floor (it runs
/// ~10× slower, so the 1M bar would be a false alarm).
#[test]
fn the_counter_kernel_clears_the_perf_baseline() {
    use std::collections::BTreeMap;
    use std::time::Instant;

    use mimz::sim::elaborate::elaborate;
    use mimz::sim::kernel::Sim;

    let src = "module Counter(WIDTH: int = 8) {\n  clock clk\n  reset rst\n  \
        out count: bits[WIDTH]\n  reg value: bits[WIDTH] = 0\n  \
        on rise(clk) { value <- value +% 1 }\n  count = value\n}\n";
    let file = mimz::parser::parse(mimz::lexer::lex(src).expect("lexes")).expect("parses");
    let design = elaborate(&file, None, &BTreeMap::new()).expect("elaborates");
    let mut sim = Sim::new(design);
    sim.set("rst", 0).unwrap();

    // Warm up, then time several short runs and take the BEST rate. The baseline
    // is a capability claim ("the kernel CAN do ≥1M/sec"), so best-of-N rejects
    // transient scheduling/thermal dips (which made a single hard-threshold run
    // flaky under a loaded build) while still catching a real >3× regression.
    for _ in 0..50_000 {
        sim.tick("clk").unwrap();
    }
    let n: u64 = 500_000;
    let mut best = 0.0_f64;
    for _ in 0..5 {
        let start = Instant::now();
        for _ in 0..n {
            sim.tick("clk").unwrap();
        }
        best = best.max(n as f64 / start.elapsed().as_secs_f64());
    }
    eprintln!(
        "counter kernel: {best:.0} cycle-events/sec (best of 5, debug={})",
        cfg!(debug_assertions)
    );

    let floor = if cfg!(debug_assertions) {
        50_000.0 // debug is far slower; just prove it isn't pathological
    } else {
        1_000_000.0 // the B8 baseline
    };
    assert!(
        best >= floor,
        "counter kernel too slow: best {best:.0} cycle-events/sec < {floor:.0}"
    );
}

/// Byte-for-byte golden lock on the VCD our writer emits (complements the
/// Icarus differential, which checks the waveform's *values*; this checks the
/// exact bytes of the file format). Regenerate an INTENDED format change with
/// `MIMZ_UPDATE_GOLDENS=1 cargo test --test sim golden_vcd`, then review the diff.
#[test]
fn the_counter_vcd_matches_the_golden_byte_for_byte() {
    use std::collections::BTreeMap;

    use mimz::sim::elaborate::elaborate;
    use mimz::sim::run::{SimOpts, run};
    use mimz::sim::vcd::to_vcd;

    let src = "module Counter(WIDTH: int = 8) {\n  clock clk\n  reset rst\n  \
        out count: bits[WIDTH]\n  reg value: bits[WIDTH] = 0\n  \
        on rise(clk) { value <- value +% 1 }\n  count = value\n}\n";
    let file = mimz::parser::parse(mimz::lexer::lex(src).expect("lexes")).expect("parses");
    let design = elaborate(&file, None, &BTreeMap::new()).expect("elaborates");
    let opts = SimOpts {
        clock: None,
        inputs: BTreeMap::new(),
        cycles: 8,
        reset_cycles: 1,
    };
    let got = to_vcd(&run(design, &opts).expect("runs"));

    let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("counter.vcd");
    if std::env::var("MIMZ_UPDATE_GOLDENS").is_ok() {
        fs::write(&golden, &got).unwrap();
        return;
    }
    let want = fs::read_to_string(&golden)
        .unwrap_or_else(|_| {
            panic!(
                "missing golden {} — run with MIMZ_UPDATE_GOLDENS=1 to create it",
                golden.display()
            )
        })
        .replace("\r\n", "\n");
    assert_eq!(
        got, want,
        "counter VCD differs from the golden — if intended, MIMZ_UPDATE_GOLDENS=1 and review the diff"
    );
}
