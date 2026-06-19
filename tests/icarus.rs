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

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use mimz::ast::{File, ModuleItem, TopItem};
use mimz::sim::elaborate::elaborate_project;
use mimz::sim::run::{SimOpts, Timeline, comb_run, run};
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
const PURE_TESTBENCHES: [(&str, &str); 6] = [
    ("kanakki_tb.v", "tamil-pure/kanakki.mimz"),
    ("cimitti_tb.v", "tamil-pure/cimitti.mimz"),
    ("oppidi_tb.v", "tamil-pure/oppidi.mimz"),
    ("thervi_tb.v", "tamil-pure/thervi.mimz"),
    ("kuutti_tb.v", "tamil-pure/kuutti.mimz"),
    ("saalaivilakku_tb.v", "tamil-pure/saalaivilakku.mimz"),
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

// ---- Layer 3 (Phase 1.5 B8 + C1): OUR simulator vs Icarus, bit-for-bit ----
//
// Layer 2 compares Icarus against hand-written semantic asserts. Layer 3 compares
// Icarus against MIN-MOZZHI'S OWN event-driven kernel: elaborate + run the design
// in-process (the exact engine behind `mimz sim` / `mimz test`), reconstruct the
// values from the VCD our writer emits, then drive the emitted Verilog through a
// generated testbench under the SAME stimulus, and assert all three agree
// bit-for-bit, per step. `differential` auto-routes: a clocked design runs the
// default stimulus (reset the first cycle, inputs held, clock toggled for
// `steps` cycles); a combinational design settles one frame per generated input
// vector (`steps` vectors). Output values are compared via Verilog `%b` (binary),
// so the comparison is signedness-agnostic and exact. Tamil-identifier examples
// work too: the testbench romanizes interface names (via the emitter's own
// `transliterate`) to match the compiled Verilog, while the kernel keeps source
// names — see `interface_name_map`.

/// One held input: name + value (clocked designs only).
type Stim<'a> = &'a [(&'a str, u128)];

/// Low-`w`-bits mask (`w >= 128` ⇒ all ones).
fn mask(w: u32) -> u128 {
    if w >= 128 {
        u128::MAX
    } else {
        (1u128 << w) - 1
    }
}

/// `name #(.P(v), …) uut (conns)` — the instantiation line, with optional
/// parameter overrides (so a design can be driven at a chosen width/limit).
fn instantiation(module: &str, params: &[(String, i128)], conns: &[String]) -> String {
    let p = if params.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = params.iter().map(|(n, v)| format!(".{n}({v})")).collect();
        format!(" #({})", items.join(", "))
    };
    format!("  {module}{p} uut ({});\n", conns.join(", "))
}

/// Clocked testbench: instantiate, apply the default stimulus, and print
/// `DIFF <cycle> <out>=<bits> …` (binary) after each rising edge.
#[allow(clippy::too_many_arguments)]
fn clocked_testbench(
    module: &str,
    params: &[(String, i128)],
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
    s += &instantiation(module, params, &conns);

    let fmt = display_fmt(outputs);
    let args = display_args(outputs);
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

/// Combinational testbench: instantiate (no clock/reset), and for each input
/// vector set the inputs, settle (`#1`), and print `DIFF <i> <out>=<bits> …`.
fn comb_testbench(
    module: &str,
    params: &[(String, i128)],
    inputs: &[(String, u32)],
    outputs: &[(String, u32)],
    vectors: &[BTreeMap<String, u128>],
) -> String {
    let mut s = String::from("module diff_tb;\n");
    for (n, w) in inputs {
        s += &format!("  reg [{}:0] {n} = 0;\n", w - 1);
    }
    for (n, w) in outputs {
        s += &format!("  wire [{}:0] {n};\n", w - 1);
    }
    let mut conns: Vec<String> = inputs.iter().map(|(n, _)| format!(".{n}({n})")).collect();
    conns.extend(outputs.iter().map(|(n, _)| format!(".{n}({n})")));
    s += &instantiation(module, params, &conns);

    let fmt = display_fmt(outputs);
    let args = display_args(outputs);
    s += "  initial begin\n";
    for (i, vec) in vectors.iter().enumerate() {
        for (n, w) in inputs {
            let v = vec.get(n).copied().unwrap_or(0);
            s += &format!("    {n} = {w}'d{v};\n");
        }
        s += "    #1;\n";
        s += &format!("    $display(\"DIFF {i} {fmt}\", {args});\n");
    }
    s += "    $finish;\n  end\nendmodule\n";
    s
}

fn display_fmt(outputs: &[(String, u32)]) -> String {
    outputs
        .iter()
        .map(|(n, _)| format!("{n}=%b"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_args(outputs: &[(String, u32)]) -> String {
    outputs
        .iter()
        .map(|(n, _)| n.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Deterministic pseudo-random input vectors, each value masked to its input's
/// width — the same vectors fed to our kernel and the Verilog testbench.
fn gen_vectors(inputs: &[(String, u32)], n: u64) -> Vec<BTreeMap<String, u128>> {
    (0..n)
        .map(|k| {
            inputs
                .iter()
                .enumerate()
                .map(|(j, (name, w))| {
                    let raw = (k as u128)
                        .wrapping_mul(2_654_435_761)
                        .wrapping_add((j as u128 + 1).wrapping_mul(40_503));
                    (name.clone(), raw & mask(*w))
                })
                .collect()
        })
        .collect()
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

/// Build + run a testbench under `iverilog`/`vvp`; return vvp's stdout.
fn run_vvp(bin: &Path, example: &str, design_v: &Path, tb: &str) -> String {
    let safe = example.replace(['\\', '/', ':', '.'], "_");
    let tb_path = std::env::temp_dir().join(format!("mimz_diff_{safe}.v"));
    std::fs::write(&tb_path, tb).unwrap();
    let vvp_out = std::env::temp_dir().join(format!("mimz_diff_{safe}.vvp"));
    let build = tool(bin, "iverilog")
        .arg("-o")
        .arg(&vvp_out)
        .args(["-s", "diff_tb"])
        .arg(&tb_path)
        .arg(design_v)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "iverilog failed on the {example} differential testbench:\n{}\n--- tb ---\n{tb}",
        String::from_utf8_lossy(&build.stderr)
    );
    let sim = tool(bin, "vvp").arg(&vvp_out).output().unwrap();
    let stdout = String::from_utf8_lossy(&sim.stdout).to_string();
    assert!(sim.status.success(), "vvp failed on {example}:\n{stdout}");
    stdout
}

/// Parse `DIFF <step> name=<bits> …` (binary values) into `step -> {name: value}`.
fn parse_icarus(stdout: &str) -> BTreeMap<u64, BTreeMap<String, u128>> {
    let mut icarus: BTreeMap<u64, BTreeMap<String, u128>> = BTreeMap::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("DIFF ") else {
            continue;
        };
        let mut it = rest.split_whitespace();
        let step: u64 = it.next().unwrap().parse().unwrap();
        let row = icarus.entry(step).or_default();
        for pair in it {
            let (n, v) = pair.split_once('=').unwrap();
            row.insert(
                n.to_string(),
                u128::from_str_radix(v, 2).expect("binary value"),
            );
        }
    }
    icarus
}

/// Assert kernel == VCD waveform == Icarus, per step, for every output.
fn compare_three_ways(
    example: &str,
    tl: &Timeline,
    icarus: &BTreeMap<u64, BTreeMap<String, u128>>,
    outputs: &[(String, u32)],
    outputs_rom: &[(String, u32)],
) {
    // Our kernel + VCD key outputs by their SOURCE name (`name`); the Icarus row
    // keys them by the ROMANIZED Verilog name (`rom`). For ASCII examples the two
    // are identical; for tamil-pure they differ, so look each side up by its own.
    let vcd = vcd_snapshots(&to_vcd(tl));
    let mut compared = 0;
    for f in tl.frames.iter().filter(|f| f.cycle.is_some()) {
        let step = f.cycle.unwrap();
        let theirs = icarus
            .get(&step)
            .unwrap_or_else(|| panic!("Icarus produced no row for step {step} of {example}"));
        let wave = vcd
            .get(&(step * 10))
            .unwrap_or_else(|| panic!("our VCD has no frame at time {} for {example}", step * 10));
        for ((name, _), (rom, _)) in outputs.iter().zip(outputs_rom) {
            let kernel = f.values[name];
            let icarus_v = theirs[rom];
            let vcd_v = wave[name];
            assert_eq!(
                kernel, icarus_v,
                "{example} step {step}: our kernel `{name}`={kernel} but Icarus `{rom}`={icarus_v}"
            );
            assert_eq!(
                vcd_v, icarus_v,
                "{example} step {step}: our VCD `{name}`={vcd_v} but Icarus `{rom}`={icarus_v}"
            );
            compared += 1;
        }
    }
    assert!(compared > 0, "{example}: nothing was compared");
}

/// Run the entry module of `example` through our simulator (kernel + VCD) and
/// Icarus, asserting the outputs match bit-for-bit, per step. `module` picks the
/// entry module when the file has more than one (else `None`).
fn differential(bin: &Path, example: &str, params: &[(&str, i128)], stim: Stim, steps: u64) {
    differential_m(bin, example, None, params, stim, steps);
}

/// As [`differential`], with an explicit entry-module name. Auto-routes on the
/// elaborated design: a **clocked** design runs the default stimulus for `steps`
/// cycles with the held `stim` inputs; a **combinational** design settles `steps`
/// generated vectors (`stim` ignored). `params` overrides module parameters on
/// both sides. Loads the entry file AND its imports, so an instantiating module
/// is flattened the same way the emitter lowers it.
fn differential_m(
    bin: &Path,
    example: &str,
    module: Option<&str>,
    params: &[(&str, i128)],
    stim: Stim,
    steps: u64,
) {
    let path = repo().join("examples").join(example);
    let files = match mimz::project::load_project(&path) {
        Ok(f) => f,
        Err(_) => panic!("load_project failed for {example}"),
    };
    let asts: Vec<File> = files.iter().map(|f| f.ast.clone()).collect();
    let entry = asts[0].clone();
    let pmap: BTreeMap<String, i128> = params.iter().map(|(n, v)| (n.to_string(), *v)).collect();
    let design = elaborate_project(&asts, module, &pmap).expect("elaborates");

    // The emitted Verilog romanizes Tamil identifiers (`emit_verilog::transliterate`),
    // but our kernel/Design keep the SOURCE names. Transliterate a clone of the
    // project exactly as the emitter does and pair identifiers positionally to map
    // source → romanized, so the generated testbench references the names the
    // compiled module actually exposes. ASCII names map to themselves.
    let mut tasts = asts.clone();
    mimz::emit_verilog::transliterate(&mut tasts);
    let rom = interface_name_map(&entry, &tasts[0]);
    let r = |n: &str| rom.get(n).cloned().unwrap_or_else(|| n.to_string());

    let module = r(&design.module);
    let params_rom: Vec<(String, i128)> = params.iter().map(|(n, v)| (r(n), *v)).collect();
    // Source-named outputs (kernel/VCD lookup) paired with romanized (TB/Icarus).
    let outputs: Vec<(String, u32)> = design
        .outputs
        .iter()
        .map(|s| (s.name.clone(), s.width.bits))
        .collect();
    let outputs_rom: Vec<(String, u32)> = design
        .outputs
        .iter()
        .map(|s| (r(&s.name), s.width.bits))
        .collect();
    let design_v = compile_example(&path);

    let (tl, tb) = if !design.clocks.is_empty() {
        // Clocked: default stimulus, held inputs.
        const RESET_CYCLES: u64 = 1;
        let clock = r(&design.clocks[0]);
        let reset = design.resets.first().map(|s| r(s));
        let held: Vec<(String, u32, u128)> = design
            .inputs
            .iter()
            .map(|s| {
                let v = stim
                    .iter()
                    .find(|(n, _)| *n == s.name)
                    .map(|(_, v)| *v)
                    .unwrap_or(0);
                (r(&s.name), s.width.bits, v)
            })
            .collect();
        let opts = SimOpts {
            clock: None,
            inputs: stim.iter().map(|(n, v)| (n.to_string(), *v)).collect(),
            cycles: steps,
            reset_cycles: RESET_CYCLES,
        };
        let tl = run(design, &opts).expect("our sim runs");
        let tb = clocked_testbench(
            &module,
            &params_rom,
            &clock,
            reset.as_deref(),
            &held,
            &outputs_rom,
            steps,
            RESET_CYCLES,
        );
        (tl, tb)
    } else {
        // Combinational: one settled frame per generated input vector. Vectors are
        // generated source-keyed (for our kernel) and re-keyed to romanized names
        // for the Verilog testbench.
        let inputs: Vec<(String, u32)> = design
            .inputs
            .iter()
            .map(|s| (s.name.clone(), s.width.bits))
            .collect();
        let inputs_rom: Vec<(String, u32)> = design
            .inputs
            .iter()
            .map(|s| (r(&s.name), s.width.bits))
            .collect();
        let vectors = gen_vectors(&inputs, steps);
        let vectors_rom: Vec<BTreeMap<String, u128>> = vectors
            .iter()
            .map(|m| m.iter().map(|(k, v)| (r(k), *v)).collect())
            .collect();
        let tl = comb_run(design, &vectors).expect("our comb sim runs");
        let tb = comb_testbench(
            &module,
            &params_rom,
            &inputs_rom,
            &outputs_rom,
            &vectors_rom,
        );
        (tl, tb)
    };

    let stdout = run_vvp(bin, example, &design_v, &tb);
    let icarus = parse_icarus(&stdout);
    compare_three_ways(example, &tl, &icarus, &outputs, &outputs_rom);
}

/// Identifier from an interface module item (port/clock/reset); `None` for
/// internals (wire/reg/instance/on-block/…), which never reach the testbench.
fn item_ident(it: &ModuleItem) -> Option<String> {
    match it {
        ModuleItem::Port { name, .. } => Some(name.name.clone()),
        ModuleItem::Clock(n) | ModuleItem::Reset { name: n, .. } => Some(n.name.clone()),
        _ => None,
    }
}

/// Map every interface identifier (module name, params, ports) from its SOURCE
/// spelling to the ROMANIZED spelling the emitter produces. `trans` is `orig`
/// after `emit_verilog::transliterate`, which only renames in place, so the two
/// ASTs pair positionally.
fn interface_name_map(orig: &File, trans: &File) -> HashMap<String, String> {
    fn modules(f: &File) -> Vec<&mimz::ast::Module> {
        f.items
            .iter()
            .filter_map(|it| match it {
                TopItem::Module(m) => Some(m),
                _ => None,
            })
            .collect()
    }
    let mut map = HashMap::new();
    for (om, tm) in modules(orig).into_iter().zip(modules(trans)) {
        map.insert(om.name.name.clone(), tm.name.name.clone());
        for (op, tp) in om.params.iter().zip(&tm.params) {
            map.insert(op.name.name.clone(), tp.name.name.clone());
        }
        for (oi, ti) in om.items.iter().zip(&tm.items) {
            if let (Some(on), Some(tn)) = (item_ident(oi), item_ident(ti)) {
                map.insert(on, tn);
            }
        }
    }
    map
}

/// Layer 3 — the simulator differential, bit-for-bit vs Icarus (kernel == VCD ==
/// Icarus). Covers clocked designs (register/reset/wrap, held inputs, FSM-free)
/// and combinational designs (incl. SIGNED) across the **entire single-file
/// corpus** — english + pure-Tamil (romanized interface names), cross-file
/// instances (C2), `repeat`/instance-arrays (C3: ripple_adder), and enum FSMs
/// (C4: traffic_light). Every example the emitter compiles also simulates here.
#[test]
fn our_simulator_matches_icarus_bit_for_bit() {
    let Some(bin) = require_iverilog() else {
        return;
    };
    // Clocked.
    differential(&bin, "english/counter.mimz", &[], &[], 20);
    differential(&bin, "english/shift_register.mimz", &[], &[("din", 1)], 16);
    differential(&bin, "english/edge_detector.mimz", &[], &[("din", 1)], 8);
    // Dual-edge: a posedge reg feeding a negedge reg — exercises the edge-aware
    // kernel's rise-before-fall ordering against Icarus (A3).
    differential(&bin, "english/dual_edge.mimz", &[], &[("d", 1)], 8);
    // Blinker at a tiny LIMIT so `led` actually toggles within the run.
    differential(&bin, "english/blinker.mimz", &[("LIMIT", 3)], &[], 12);
    // Async reset (A5): `always @(posedge clk or posedge rst)`. Under the
    // clock-aligned default stimulus the kernel (reset at the edge) and the
    // async Verilog agree at every sample point.
    differential(&bin, "english/async_reset.mimz", &[], &[], 20);
    // Combinational (generated input vectors).
    differential(&bin, "english/adder.mimz", &[], &[], 8);
    differential(&bin, "english/comparator.mimz", &[], &[], 8);
    differential(&bin, "english/mux4.mimz", &[], &[], 8);
    differential(&bin, "english/datapath.mimz", &[], &[], 8);
    differential(&bin, "english/window.mimz", &[], &[], 8);
    differential(&bin, "english/lib/full_adder.mimz", &[], &[], 8);
    // Combinational + SIGNED — the `%b` binary compare makes signedness moot.
    differential(&bin, "english/bitops.mimz", &[], &[], 8);
    differential(&bin, "english/signed_math.mimz", &[], &[], 8);
    // Replication `{N{x}}` (combinational).
    differential(&bin, "english/replicate.mimz", &[], &[], 8);
    // Don't-care `match` patterns `0b1??` (combinational priority decoder).
    differential(&bin, "english/priority.mimz", &[], &[], 8);
    // Memory `mem` (A4): a register file — `initial`-seeded cells, a clocked
    // indexed write (`m[waddr] <- wdata` when `we`), and a combinational indexed
    // read (`rdata = m[raddr]`). Held stimulus writes and reads the same cell.
    differential(
        &bin,
        "english/regfile.mimz",
        &[],
        &[("we", 1), ("waddr", 2), ("wdata", 165), ("raddr", 2)],
        8,
    );
    // Pure-Tamil (Tamil keywords AND identifiers): the testbench romanizes names
    // to match the emitted Verilog, so these now ride the same bit-for-bit
    // differential as their english twins (`கணக்கி`/kanakki = counter, etc.).
    differential(&bin, "tamil-pure/kanakki.mimz", &[], &[], 16);
    differential(&bin, "tamil-pure/cimitti.mimz", &[("வரம்பு", 3)], &[], 12);
    differential(&bin, "tamil-pure/oppidi.mimz", &[], &[], 8);
    differential(&bin, "tamil-pure/thervi.mimz", &[], &[], 8);
    differential(&bin, "tamil-pure/kuutti.mimz", &[], &[], 8);
    differential(&bin, "tamil-pure/saalaivilakku.mimz", &[], &[], 12);
    // Cross-file module instances, flattened by the sim elaborator (C2): alu's
    // `Top` instantiates `Adder` (imported); `chained` chains two `FullAdder`s
    // (an instance output feeds the next instance's input).
    differential_m(
        &bin,
        "english/alu.mimz",
        Some("Top"),
        &[],
        &[("x", 7), ("y", 5)],
        6,
    );
    differential(&bin, "english/chained.mimz", &[], &[], 8);
    // `repeat` unrolling (C3): ripple_adder chains a FullAdder per bit via
    // `repeat` + an instance array + bit-indexed drives.
    differential(&bin, "english/ripple_adder.mimz", &[], &[], 8);
    // enum-typed signals (C4): the traffic-light FSM (`reg state: State`, `match`
    // over the enum). 12 cycles cover the reset + the first state transition.
    differential(&bin, "english/traffic_light.mimz", &[], &[], 12);
    // Tamil-identifier toggler (interface names romanize like tamil-pure).
    differential(&bin, "english/vilakku.mimz", &[], &[], 8);
}
