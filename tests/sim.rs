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
fn cycles_over_the_limit_is_rejected_by_the_cli() {
    // SEC: --cycles past MAX_SIM_CYCLES (1_000_000) is rejected at parse time, so
    // a huge value can't drive an unbounded frame-allocation loop.
    let p = temp_mimz(COUNTER);
    let out = mimz()
        .args(["sim"])
        .arg(&p)
        .args(["--cycles", "2000000"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "over-limit --cycles must be rejected"
    );
    let s = String::from_utf8_lossy(&out.stderr);
    assert!(s.contains("not in"), "no clap range error:\n{s}");
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

const ADDER: &str =
    "module Adder {\n  in a: bits[8]\n  in b: bits[8]\n  out sum: bits[9]\n  sum = a + b\n}\n";

#[test]
fn a_combinational_module_settles_one_frame() {
    let p = temp_mimz(ADDER);
    let out = mimz()
        .args(["sim"])
        .arg(&p)
        .args(["--in", "a=200,b=100", "--trace"])
        .output()
        .unwrap();
    assert!(out.status.success(), "combinational sim failed: {:?}", out);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("sum"), "no sum column:\n{s}");
    assert!(s.contains("300"), "expected sum=300 (lossless add):\n{s}");
    // One settled frame: header + separator + a single row.
    assert_eq!(s.lines().count(), 3, "expected one frame:\n{s}");
}

#[test]
fn sweep_emits_a_frame_per_combination() {
    let p = temp_mimz(ADDER);
    let out = mimz()
        .args(["sim"])
        .arg(&p)
        .args(["--in", "b=10", "--sweep", "a=1|2|3", "--trace"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let s = String::from_utf8_lossy(&out.stdout);
    // header + separator + 3 swept rows; sums are 11/12/13.
    assert_eq!(s.lines().count(), 5, "expected 3 swept frames:\n{s}");
    assert!(
        s.contains("11") && s.contains("12") && s.contains("13"),
        "{s}"
    );
}

#[test]
fn a_combinational_module_writes_a_vcd() {
    let p = temp_mimz(ADDER);
    let vcd = std::env::temp_dir().join(format!("mimz_comb_{}.vcd", std::process::id()));
    let out = mimz()
        .args(["sim"])
        .arg(&p)
        .args(["--in", "a=5,b=7", "-o"])
        .arg(&vcd)
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let v = fs::read_to_string(&vcd).unwrap();
    assert!(v.contains("$timescale") && v.contains(" sum $end"), "{v}");
    assert!(v.contains("b1100 "), "expected sum=12 vector line:\n{v}");
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

/// Tag-only enum match works end-to-end in the kernel: a 3-state FSM cycles
/// Idle → Run → Done → Idle, and the combinational `tag` output encodes
/// the current state as 0/1/2.
#[test]
fn sim_enum_tag_only_match_works() {
    use std::collections::BTreeMap;

    use mimz::sim::elaborate::elaborate;
    use mimz::sim::kernel::Sim;

    let src = "
module FSM {
  clock clk
  reset rst
  out tag: bits[2]
  enum S { Idle, Run, Done }
  reg st: S = S.Idle
  on rise(clk) {
    st <- match st {
      S.Idle => S.Run
      S.Run  => S.Done
      S.Done => S.Idle
    }
  }
  tag = match st {
    S.Idle => 0
    S.Run  => 1
    S.Done => 2
  }
}
";

    let file = mimz::parser::parse(mimz::lexer::lex(src).expect("lexes")).expect("parses");
    let design = elaborate(&file, None, &BTreeMap::new()).expect("elaborates");
    let mut sim = Sim::new(design);
    sim.set("rst", 0).unwrap();

    // Initial: Idle → tag = 0.
    assert_eq!(sim.peek("tag").unwrap(), 0, "Idle → tag=0");

    // After tick 1: Idle → Run, tag = 1.
    sim.tick("clk").unwrap();
    assert_eq!(sim.peek("tag").unwrap(), 1, "Run → tag=1");

    // After tick 2: Run → Done, tag = 2.
    sim.tick("clk").unwrap();
    assert_eq!(sim.peek("tag").unwrap(), 2, "Done → tag=2");

    // After tick 3: Done → Idle, tag = 0.
    sim.tick("clk").unwrap();
    assert_eq!(sim.peek("tag").unwrap(), 0, "Idle again → tag=0");
}

/// Tagged enum match works end-to-end: a `Packet { Read(addr: bits[4]), Nop }`
/// enum port is decoded; the correct arm fires and the payload `addr` field
/// is extracted from the packed value.
///
/// Layout (D3): total_w = tag_w + max_payload_w = 1 + 4 = 5 bits.
///   Packet.Read(addr) packed = (0 << 4) | addr  [tag=0]
///   Packet.Nop        packed = (1 << 4)           [tag=1, payload=0]
#[test]
fn sim_tagged_enum_payload_extracted() {
    use std::collections::BTreeMap;

    use mimz::sim::elaborate::elaborate;
    use mimz::sim::kernel::Sim;

    let src = "
module Decoder {
  enum Packet { Read(addr: bits[4]), Nop }
  in pkt: Packet
  out got_read: bit
  out addr_out: bits[4]
  got_read = match pkt {
    Packet.Read(a) => 1
    Packet.Nop     => 0
  }
  addr_out = match pkt {
    Packet.Read(a) => a
    Packet.Nop     => 0
  }
}
";

    let file = mimz::parser::parse(mimz::lexer::lex(src).expect("lexes")).expect("parses");
    // Checker must run to set inferred_total_width on the tagged enum.
    mimz::checker::check(std::slice::from_ref(&file)).expect("checks clean");
    let design = elaborate(&file, None, &BTreeMap::new()).expect("elaborates");
    let mut sim = Sim::new(design);

    // Packet.Read(addr=10): packed = (0 << 4) | 10 = 10.
    sim.set("pkt", 10).unwrap();
    assert_eq!(
        sim.peek("got_read").unwrap(),
        1,
        "Read tag must fire got_read"
    );
    assert_eq!(
        sim.peek("addr_out").unwrap(),
        10,
        "addr payload extracted = 10"
    );

    // Packet.Nop: packed = (1 << 4) | 0 = 16.
    sim.set("pkt", 16).unwrap();
    assert_eq!(
        sim.peek("got_read").unwrap(),
        0,
        "Nop tag must not fire got_read"
    );
    assert_eq!(sim.peek("addr_out").unwrap(), 0, "Nop has no addr payload");
}

/// Write-arm payload extraction: a two-field tagged variant `Write(addr: bits[32], data: bits[32])`
/// must bind *both* fields independently. Output is `addr XOR data` to prove the two slices
/// are distinct (if addr and data aliased, XOR would always be 0).
///
/// Layout (D3): total_w = tag_w + max_payload_w = 1 + 64 = 65 bits.
/// Fields are packed MSB-first inside [max_payload_w-1:0].
///   Packet.Write: addr→[63:32], data→[31:0]  packed = (1<<64)|(addr<<32)|data
///   Packet.Read:  addr→[63:32]               packed = (0<<64)|(addr<<32)
///
/// For Write(addr=0xAA, data=0x55): packed = (1<<64)|(0xAA<<32)|0x55
/// Expected xor_out = 0xAA ^ 0x55 = 0xFF
#[test]
fn sim_tagged_enum_write_arm_payload_extracted() {
    use std::collections::BTreeMap;

    use mimz::sim::elaborate::elaborate;
    use mimz::sim::kernel::Sim;

    let src = "
module BusDecoder {
  enum Packet {
    Read(addr: bits[32]),
    Write(addr: bits[32], data: bits[32])
  }
  in pkt: Packet
  out xor_out: bits[32]
  xor_out = match pkt {
    Packet.Read(a)     => a
    Packet.Write(a, d) => a ^ d
  }
}
";

    let file = mimz::parser::parse(mimz::lexer::lex(src).expect("lexes")).expect("parses");
    // Checker must run to set inferred_total_width on the tagged enum.
    mimz::checker::check(std::slice::from_ref(&file)).expect("checks clean");
    let design = elaborate(&file, None, &BTreeMap::new()).expect("elaborates");
    let mut sim = Sim::new(design);

    // Packet.Write(addr=0xAA, data=0x55):
    // packed = (1u128 << 64) | (0xAAu128 << 32) | 0x55u128
    let packed: u128 = (1u128 << 64) | (0xAAu128 << 32) | 0x55u128;
    // Sim::set takes u128 via Into<u128>; the value fits in 65 bits.
    sim.set("pkt", packed).unwrap();
    assert_eq!(
        sim.peek("xor_out").unwrap(),
        0xAA ^ 0x55,
        "Write arm: xor_out must equal addr XOR data = 0xFF"
    );

    // Packet.Read(addr=0xBEEF): fields are packed MSB-first inside [max_payload_w-1:0].
    // max_payload_w=64; Read.addr is the only field → hi=63, lo=32.
    // packed = (0 << 64) | (0xBEEF << 32)
    let packed_read: u128 = 0xBEEFu128 << 32;
    sim.set("pkt", packed_read).unwrap();
    assert_eq!(
        sim.peek("xor_out").unwrap(),
        0xBEEF,
        "Read arm: xor_out must equal addr = 0xBEEF"
    );
}

/// Helper: parse `src`, run all `test` blocks in it, assert every one passes.
fn run_test_ok(src: &str) {
    use mimz::ast::TopItem;
    use mimz::sim::harness::{TestResult, run_test};

    let file = mimz::parser::parse(mimz::lexer::lex(src).expect("lexes")).expect("parses");
    // Run checker so bundle widths are resolved.
    mimz::checker::check(std::slice::from_ref(&file)).expect("checks clean");
    let tests: Vec<_> = file
        .items
        .iter()
        .filter_map(|i| match i {
            TopItem::Test(t) => Some(t),
            _ => None,
        })
        .collect();
    assert!(!tests.is_empty(), "no test blocks found in src");
    for decl in tests {
        let outcome = run_test(std::slice::from_ref(&file), src, decl)
            .unwrap_or_else(|e| panic!("test `{}` errored: {e}", decl.name));
        match &outcome.result {
            TestResult::Pass => {}
            TestResult::Fail(msg) => panic!("test `{}` failed:\n{msg}", decl.name),
        }
    }
}

#[test]
fn sim_bundle_wire() {
    let src = r#"
bundle Hs { valid: bit, data: bits[8] }
module Top {
  in  req: Hs
  out rsp: Hs
  rsp = req
}
test "passthrough" for Top {
  req_valid = 1
  req_data = 42
  expect rsp_valid == 1
  expect rsp_data == 42
}
"#;
    run_test_ok(src);
}
