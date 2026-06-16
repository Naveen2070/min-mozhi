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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use mimz::sim::elaborate::elaborate;
use mimz::sim::run::{SimOpts, run};
use mimz::sim::vcd::to_vcd;

/// Testbench file (under tests/icarus/) -> the example it tests.
/// Testbench module name = file name minus `.v`.
const TESTBENCHES: [(&str, &str); 16] = [
    ("adder_tb.v", "english/adder.mimz"),
    ("alu_tb.v", "english/alu.mimz"),
    ("bitops_tb.v", "english/bitops.mimz"),
    ("blinker_tb.v", "english/blinker.mimz"),
    ("chained_tb.v", "english/chained.mimz"),
    ("comparator_tb.v", "english/comparator.mimz"),
    ("counter_tb.v", "english/counter.mimz"),
    ("datapath_tb.v", "english/datapath.mimz"),
    ("edge_detector_tb.v", "english/edge_detector.mimz"),
    ("full_adder_tb.v", "english/lib/full_adder.mimz"),
    ("mux4_tb.v", "english/mux4.mimz"),
    ("ripple_adder_tb.v", "english/ripple_adder.mimz"),
    ("shift_register_tb.v", "english/shift_register.mimz"),
    ("signed_math_tb.v", "english/signed_math.mimz"),
    ("traffic_light_tb.v", "english/traffic_light.mimz"),
    ("window_tb.v", "english/window.mimz"),
];

/// Pure-Tamil showcase testbenches (examples/tamil-pure/) — the same circuits as
/// their English counterparts, instantiated through the romanized Tamil port
/// names (clk=katikai, rst=miill, …). Proves the transliterated Verilog
/// simulates correctly, not just that it elaborates.
const PURE_TESTBENCHES: [(&str, &str); 4] = [
    ("kanakki_tb.v", "tamil-pure/kanakki.mimz"),
    ("cimitti_tb.v", "tamil-pure/cimitti.mimz"),
    ("oppidi_tb.v", "tamil-pure/oppidi.mimz"),
    ("thervi_tb.v", "tamil-pure/thervi.mimz"),
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
    assert!(checked >= 48, "expected the whole corpus, found {checked}");
}

/// Run a testbench table through iverilog + vvp, asserting each prints PASS
/// exactly once and never FAIL. Shared by the English and pure-Tamil layers.
fn run_self_checking(bin: &Path, table: &[(&str, &str)]) {
    for (tb_file, example) in table {
        let tb = repo().join("tests").join("icarus").join(tb_file);
        assert!(tb.exists(), "missing testbench {}", tb.display());
        let design = compile_example(&repo().join("examples").join(example));
        let tb_module = tb_file.trim_end_matches(".v");
        let vvp_out = std::env::temp_dir().join(format!("mimz_icarus_{tb_module}.vvp"));

        let out = tool(bin, "iverilog")
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

        let sim = tool(bin, "vvp").arg(&vvp_out).output().unwrap();
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

/// Layer 2 — the self-checking testbenches: Min-Mozhi semantics encoded
/// in Verilog asserts, simulated by Icarus. Each prints PASS exactly once
/// or FAIL with a reason.
#[test]
fn self_checking_testbenches_pass() {
    let Some(bin) = require_iverilog() else {
        return;
    };
    run_self_checking(&bin, &TESTBENCHES);
}

/// Layer 2 for the pure-Tamil showcase: the same semantics, driven through the
/// romanized Tamil port names — the transliterated Verilog must simulate, not
/// just elaborate.
#[test]
fn self_checking_pure_tamil_testbenches_pass() {
    let Some(bin) = require_iverilog() else {
        return;
    };
    run_self_checking(&bin, &PURE_TESTBENCHES);
}

// ---- Layer 3 (Phase 1.5 B8): OUR simulator vs Icarus, bit-for-bit ----
//
// Layer 2 compares Icarus against hand-written semantic asserts. Layer 3 compares
// Icarus against MIN-MOZZHI'S OWN event-driven kernel: elaborate + run the design
// in-process (the exact engine behind `mimz sim` / `mimz test`), then drive the
// emitted Verilog through a generated testbench applying the SAME stimulus (reset
// the first cycle, inputs held, clock toggled) and assert the per-cycle output
// values match exactly. Two independent simulators, same program, same stimulus.

/// One drivable input: name + value, held for the whole run.
type Stim<'a> = &'a [(&'a str, u128)];

/// Generate a Verilog testbench that instantiates `module` and applies the
/// default stimulus, printing `DIFF <cycle> <out>=<val> …` after each rising edge.
fn diff_testbench(
    module: &str,
    clock: &str,
    reset: Option<&str>,
    inputs: &[(String, u32, u128)],
    outputs: &[(String, u32)],
    cycles: u64,
    reset_cycles: u64,
) -> String {
    let mut s = String::from("module diff_tb;\n");
    s += &format!("  reg {clock} = 0;\n");
    if let Some(r) = reset {
        s += &format!("  reg {r} = 0;\n");
    }
    for (n, w, v) in inputs {
        s += &format!("  reg [{}:0] {n} = {v};\n", w - 1);
    }
    for (n, w) in outputs {
        s += &format!("  wire [{}:0] {n};\n", w - 1);
    }
    s += "  integer cyc;\n";

    let mut conns = vec![format!(".{clock}({clock})")];
    if let Some(r) = reset {
        conns.push(format!(".{r}({r})"));
    }
    conns.extend(inputs.iter().map(|(n, _, _)| format!(".{n}({n})")));
    conns.extend(outputs.iter().map(|(n, _)| format!(".{n}({n})")));
    s += &format!("  {module} uut ({});\n", conns.join(", "));

    let fmt: String = outputs
        .iter()
        .map(|(n, _)| format!("{n}=%0d"))
        .collect::<Vec<_>>()
        .join(" ");
    let args: String = outputs
        .iter()
        .map(|(n, _)| n.clone())
        .collect::<Vec<_>>()
        .join(", ");

    s += "  initial begin\n";
    s += &format!("    for (cyc = 0; cyc < {cycles}; cyc = cyc + 1) begin\n");
    if let Some(r) = reset {
        s += &format!("      {r} = (cyc < {reset_cycles}) ? 1'b1 : 1'b0;\n");
    }
    s += &format!("      #5 {clock} = 1;\n");
    s += "      #1;\n";
    s += &format!("      $display(\"DIFF %0d {fmt}\", cyc, {args});\n");
    s += &format!("      #4 {clock} = 0;\n");
    s += "    end\n    $finish;\n  end\nendmodule\n";
    s
}

/// Replay a VCD document into `time -> {signal: value}` snapshots. VCD lists
/// only CHANGES, so we carry the running state forward and snapshot it whenever
/// the timestamp advances (and at EOF). Handles our writer's format: `#<t>`
/// timestamps, scalar `1!` / `0!`, vector `b1010 !`, and the `$dumpvars` block.
fn vcd_snapshots(text: &str) -> BTreeMap<u64, BTreeMap<String, u128>> {
    let mut id2name: BTreeMap<String, String> = BTreeMap::new();
    let mut cur: BTreeMap<String, u128> = BTreeMap::new();
    let mut snaps: BTreeMap<u64, BTreeMap<String, u128>> = BTreeMap::new();
    let mut now: Option<u64> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("$var") {
            // `$var wire <bits> <id> <name> $end`
            let p: Vec<&str> = rest.split_whitespace().collect();
            if p.len() >= 4 {
                id2name.insert(p[2].to_string(), p[3].to_string());
            }
            continue;
        }
        if let Some(ts) = line.strip_prefix('#') {
            if let Some(t) = now {
                snaps.insert(t, cur.clone());
            }
            now = ts.parse().ok();
            continue;
        }
        if line.starts_with('$') {
            continue; // $timescale / $scope / $dumpvars / $end / $enddefinitions
        }
        // A value-change line.
        if let Some(rest) = line.strip_prefix('b') {
            let mut it = rest.split_whitespace();
            let bits = it.next().unwrap_or("");
            if let Some(id) = it.next() {
                if let Some(name) = id2name.get(id) {
                    cur.insert(name.clone(), u128::from_str_radix(bits, 2).unwrap_or(0));
                }
            }
        } else {
            let (b, id) = line.split_at(1);
            if let Some(name) = id2name.get(id) {
                cur.insert(name.clone(), (b == "1") as u128);
            }
        }
    }
    if let Some(t) = now {
        snaps.insert(t, cur.clone());
    }
    snaps
}

/// Run one example through both simulators and assert every output agrees at
/// every cycle. Three independent views are compared per cycle: our event-driven
/// kernel (in-process), the **VCD waveform** our writer emits from that run, and
/// Icarus running the emitted Verilog under the same stimulus.
fn differential(bin: &Path, example: &str, stim: Stim, cycles: u64) {
    const RESET_CYCLES: u64 = 1;
    let path = repo().join("examples").join(example);
    let src = std::fs::read_to_string(&path).expect("read example");
    let file = mimz::parser::parse(mimz::lexer::lex(&src).expect("lexes")).expect("parses");
    let design = elaborate(&file, None, &BTreeMap::new()).expect("elaborates");

    // Capture the port shape before `run` consumes the design.
    let module = design.module.clone();
    let clock = design.clocks.first().expect("a clock").clone();
    let reset = design.resets.first().cloned();
    let outputs: Vec<(String, u32)> = design
        .outputs
        .iter()
        .map(|s| (s.name.clone(), s.width.bits))
        .collect();
    let inputs: Vec<(String, u32, u128)> = design
        .inputs
        .iter()
        .map(|s| {
            let v = stim
                .iter()
                .find(|(n, _)| *n == s.name)
                .map(|(_, v)| *v)
                .unwrap_or(0);
            (s.name.clone(), s.width.bits, v)
        })
        .collect();

    // OUR side: the kernel that `mimz sim` / `mimz test` use.
    let opts = SimOpts {
        clock: None,
        inputs: stim.iter().map(|(n, v)| (n.to_string(), *v)).collect(),
        cycles,
        reset_cycles: RESET_CYCLES,
    };
    let tl = run(design, &opts).expect("our sim runs");

    // OUR VCD: the waveform our writer emits from this same run. Reconstruct the
    // value of each signal at every rising-edge time (`cycle * 10`, where the
    // frame carries the post-edge state) so we can check the file is correct too.
    let vcd = vcd_snapshots(&to_vcd(&tl));

    // ICARUS side: emit Verilog + a stimulus-matched testbench, run under vvp.
    let design_v = compile_example(&path);
    let tb = diff_testbench(
        &module,
        &clock,
        reset.as_deref(),
        &inputs,
        &outputs,
        cycles,
        RESET_CYCLES,
    );
    let safe = example.replace(['\\', '/', ':', '.'], "_");
    let tb_path = std::env::temp_dir().join(format!("mimz_diff_{safe}.v"));
    std::fs::write(&tb_path, &tb).unwrap();
    let vvp_out = std::env::temp_dir().join(format!("mimz_diff_{safe}.vvp"));

    let build = tool(bin, "iverilog")
        .arg("-o")
        .arg(&vvp_out)
        .args(["-s", "diff_tb"])
        .arg(&tb_path)
        .arg(&design_v)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "iverilog failed on the {example} differential testbench:\n{}\n--- tb ---\n{tb}",
        String::from_utf8_lossy(&build.stderr)
    );
    let sim = tool(bin, "vvp").arg(&vvp_out).output().unwrap();
    let stdout = String::from_utf8_lossy(&sim.stdout);
    assert!(sim.status.success(), "vvp failed on {example}:\n{stdout}");

    // Parse `DIFF <cyc> name=val …` into cycle -> {name: value}.
    let mut icarus: BTreeMap<u64, BTreeMap<String, u128>> = BTreeMap::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("DIFF ") else {
            continue;
        };
        let mut it = rest.split_whitespace();
        let cyc: u64 = it.next().unwrap().parse().unwrap();
        let row = icarus.entry(cyc).or_default();
        for pair in it {
            let (n, v) = pair.split_once('=').unwrap();
            row.insert(n.to_string(), v.parse().unwrap());
        }
    }

    // Compare every rising-edge frame, output by output: kernel == VCD == Icarus.
    let mut compared = 0;
    for f in tl.frames.iter().filter(|f| f.cycle.is_some()) {
        let cyc = f.cycle.unwrap();
        let theirs = icarus
            .get(&cyc)
            .unwrap_or_else(|| panic!("Icarus produced no row for cycle {cyc} of {example}"));
        let wave = vcd
            .get(&(cyc * 10))
            .unwrap_or_else(|| panic!("our VCD has no rising-edge frame at time {}", cyc * 10));
        for (name, _) in &outputs {
            let kernel = f.values[name];
            let icarus_v = theirs[name];
            let vcd_v = wave[name];
            assert_eq!(
                kernel, icarus_v,
                "{example} cycle {cyc}: our kernel `{name}`={kernel} but Icarus={icarus_v}"
            );
            assert_eq!(
                vcd_v, icarus_v,
                "{example} cycle {cyc}: our VCD waveform `{name}`={vcd_v} but Icarus={icarus_v}"
            );
            compared += 1;
        }
    }
    assert!(compared > 0, "{example}: nothing was compared");
}

/// Layer 3 — the simulator differential. Per cycle, three independent views must
/// agree bit-for-bit: our event-driven kernel, the **VCD waveform** our writer
/// emits from that run, and Icarus running the emitted Verilog under the same
/// stimulus. The counter exercises a register + sync reset + wrapping arithmetic;
/// the shift register adds a held input and a shift/extend combinational path.
#[test]
fn our_simulator_matches_icarus_bit_for_bit() {
    let Some(bin) = require_iverilog() else {
        return;
    };
    differential(&bin, "english/counter.mimz", &[], 20);
    differential(&bin, "english/shift_register.mimz", &[("din", 1)], 16);
    // A held input feeding a combinational output that also reads a register
    // (`pulse = din && !prev`) — the rising-edge detector pulses once then holds.
    differential(&bin, "english/edge_detector.mimz", &[("din", 1)], 8);
}
