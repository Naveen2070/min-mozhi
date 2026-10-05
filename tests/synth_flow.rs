//! Synthesis v1 design sections 6.3 and 6.4: Yosys's synthesized netlist,
//! simulated in Icarus with Yosys's iCE40 cell models, matches `ir::exec`;
//! and `mimz build` produces a bitstream. Needs the OSS CAD Suite
//! (`REQUIRE_YOSYS=1` makes a missing suite a failure; `MIMZ_OSS_CAD` points
//! at it).

mod support;

use mimz_core::backend::verilog::Emitted;
use std::collections::BTreeMap;
use std::path::PathBuf;
use support::ir_sim::*;

/// (file, top module, parameter overrides, reset input port) through
/// synthesis. The reset port is named here because the IR keeps no record of a
/// synchronous reset (it is a Mux before a Dff).
type Case = (
    &'static str,
    &'static str,
    &'static [(&'static str, i128)],
    Option<&'static str>,
);
const SUBSET: &[Case] = &[
    (
        "examples/english/blinker.mimz",
        "Blinker",
        &[("LIMIT", 3)],
        Some("rst"),
    ),
    ("examples/english/counter.mimz", "Counter", &[], Some("rst")),
    ("examples/english/alu.mimz", "Alu", &[], None),
    ("examples/english/alu.mimz", "Top", &[], Some("rst")),
    (
        "examples/english/async_reset.mimz",
        "ACounter",
        &[],
        Some("rst"),
    ),
    ("examples/english/regfile.mimz", "RegFile", &[], None),
    (
        "tests/fixtures/build/odd_mem.mimz",
        "OddMem",
        &[],
        Some("rst"),
    ),
];

/// The shared `stimulus` drives a 1-bit reset 1,0,1,0..., so a counter never
/// runs. Hold `reset` high at tick 0 only (reset, then run free).
fn reset_once(stim: &mut [Vec<(String, u128)>], reset: Option<&str>) {
    let Some(r) = reset else { return };
    let mut found = false;
    for (t, tick) in stim.iter_mut().enumerate() {
        for (n, v) in tick.iter_mut() {
            if n == r {
                *v = u128::from(t == 0);
                found = true;
            }
        }
    }
    // A misspelled reset name would silently leave the entry free-running on
    // the 1,0,1,0 reset again.
    assert!(found, "reset port `{r}` is not an input of this module");
}

/// Distinct values `port` takes over a trace.
fn distinct(trace: &[Vec<(String, u128)>], port: &str) -> usize {
    trace
        .iter()
        .flatten()
        .filter(|(n, _)| n == port)
        .map(|(_, v)| *v)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

fn lower(file: &str, top: &str, params: &[(&str, i128)]) -> mimz_core::ir::Module {
    let files = mimz::project::load_project(&support::repo().join(file))
        .ok()
        .expect("loads");
    let asts: Vec<_> = files.iter().map(|f| f.ast.clone()).collect();
    mimz_core::checker::check(&asts).expect("checks");
    let p: BTreeMap<String, i128> = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    let design = mimz_core::elaborate::elaborate_project_with_mode(
        &asts,
        Some(top),
        &p,
        mimz_core::elaborate::SimMode::Lower,
    )
    .expect("elaborates");
    let mut m = mimz_core::ir::lower(&design);
    mimz_core::ir::opt::optimize(&mut m);
    m
}

#[test]
fn synthesized_netlists_match_ir_exec() {
    let Some(tc) = support::require_yosys() else {
        return;
    };
    let Some(iv) = support::require_iverilog() else {
        return;
    };
    let cells = tc.yosys_datdir().unwrap().join("ice40").join("cells_sim.v");
    for (file, top, params, reset) in SUBSET {
        let m = lower(file, top, params);
        let e = mimz_core::backend::verilog::emit(&m);
        let work = synthesize(&tc, &e, &format!("{file}:{top}"));
        let mut stim = stimulus(&m, 8);
        reset_once(&mut stim, *reset);
        let want = exec_trace(&m, &stim);
        // Not vacuous: the oracle's outputs must actually move.
        match *top {
            "Blinker" => assert_eq!(distinct(&want, "led"), 2, "Blinker led never toggles"),
            "Counter" => assert!(distinct(&want, "count") > 2, "Counter barely counts"),
            "ACounter" => assert!(distinct(&want, "count") > 2, "ACounter barely counts"),
            "Top" => assert!(distinct(&want, "total") > 2, "Top total barely moves"),
            _ => {}
        }
        let tb = testbench(&e, &clock_ports(&m), &stim);
        let got = icarus_trace(
            &run_iverilog(
                &iv,
                &format!("{top}_synth"),
                &[work.join("synth.v"), cells.clone()],
                &tb,
                &["-g2012", "-DNO_ICE40_DEFAULT_ASSIGNMENTS"],
            ),
            &e,
        );
        assert_eq!(
            got, want,
            "{file}:{top}: synthesized netlist differs from ir::exec"
        );
    }
}

/// A scratch folder removed on drop, so a failed assertion leaves nothing behind.
struct Scratch(PathBuf);

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A unique temp folder per test (process id + tag); never inside the repo.
fn scratch(tag: &str) -> Scratch {
    let d = std::env::temp_dir().join(format!("mimz_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Scratch(d)
}

/// Yosys `synth_ice40` over the emitted Verilog in a fresh scratch folder;
/// `synth.v` lands there.
fn synthesize(tc: &mimz::build::toolchain::Toolchain, e: &Emitted, label: &str) -> Scratch {
    let work = scratch(&format!("synth_{}", file_safe(label)));
    std::fs::write(work.join("top.v"), &e.text).unwrap();
    let noabc = if mimz::build::toolchain::needs_noabc() {
        " -noabc"
    } else {
        ""
    };
    let out = tc
        .command("yosys")
        .current_dir(&*work)
        .args([
            "-q",
            "-p",
            &format!(
                "read_verilog top.v; synth_ice40{noabc} -top {}; write_verilog -noattr synth.v",
                e.top
            ),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{label}: yosys failed\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    work
}

/// `ir::exec` has no notion of time between edges, so an asynchronous reset's
/// timing is pinned on the synthesized netlist itself, by the Verilog
/// semantics: `q` equals the reset value right after `arst` rises, with no
/// clock edge in between.
#[test]
fn an_async_reset_clears_the_synthesized_register_between_edges() {
    let Some(tc) = support::require_yosys() else {
        return;
    };
    let Some(iv) = support::require_iverilog() else {
        return;
    };
    let cells = tc.yosys_datdir().unwrap().join("ice40").join("cells_sim.v");
    let m = lower("examples/english/async_reset.mimz", "ACounter", &[]);
    let e = mimz_core::backend::verilog::emit(&m);
    let work = synthesize(&tc, &e, "async_reset_midtick");
    let v = |ir: &str| e.ports.iter().find(|p| p.0 == ir).unwrap().1.clone();
    let (clk, rst, count) = (v("clk"), v("rst"), v("count"));
    let tb = format!(
        "`timescale 1ns/1ps\nmodule diff_tb;\n  reg {clk} = 0;\n  reg {rst} = 0;\n  wire [7:0] {count};\n  {top} uut (.{clk}({clk}), .{rst}({rst}), .{count}({count}));\n  initial begin\n    #1 {rst} = 1; #1 {rst} = 0; #1;\n    repeat (3) begin {clk} = 1; #1 {clk} = 0; #1; end\n    $display(\"DIFF 0 {count}=%b\", {count});\n    {rst} = 1; #1;\n    $display(\"DIFF 1 {count}=%b\", {count});\n    $finish;\n  end\nendmodule\n",
        top = e.top
    );
    let out = run_iverilog(
        &iv,
        "async_reset_midtick",
        &[work.join("synth.v"), cells],
        &tb,
        &["-g2012", "-DNO_ICE40_DEFAULT_ASSIGNMENTS"],
    );
    let rows = icarus_trace(&out, &e);
    assert_eq!(rows[0][0].1, 3, "counted three edges first: {out}");
    assert_eq!(rows[1][0].1, 0, "no reset between edges: {out}");
}

fn build(args: &[&str]) -> std::process::Output {
    support::mimz().arg("build").args(args).output().unwrap()
}

/// Blinker + iCEBreaker + the fixture PCF, into `dir` (work folder and bin).
fn build_blinker(dir: &std::path::Path) -> std::process::Output {
    let src = support::repo().join("examples/english/blinker.mimz");
    let pcf = support::repo().join("tests/fixtures/build/blinker.pcf");
    build(&[
        src.to_str().unwrap(),
        "--board",
        "icebreaker",
        "--pcf",
        pcf.to_str().unwrap(),
        "--work",
        dir.join("work").to_str().unwrap(),
        "-o",
        dir.join("out.bin").to_str().unwrap(),
    ])
}

#[test]
fn mimz_build_produces_a_bitstream() {
    if support::require_yosys().is_none() {
        return;
    }
    let dir = scratch("blinker");
    let out = build_blinker(&dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(std::fs::metadata(dir.join("out.bin")).unwrap().len() > 1000);
    // `--work` is honoured: logs and Verilog land there, not next to the source.
    assert!(dir.join("work").join("yosys.log").is_file());
    assert!(dir.join("work").join("top.v").is_file());
    assert!(
        !support::repo().join("examples/english/build").exists(),
        "default work folder was created"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("MHz")
            || String::from_utf8_lossy(&out.stdout).contains("MHz")
    );
}

#[test]
fn building_twice_overwrites_the_work_folder() {
    if support::require_yosys().is_none() {
        return;
    }
    let dir = scratch("twice");
    assert!(build_blinker(&dir).status.success());
    assert!(build_blinker(&dir).status.success());
    assert!(std::fs::metadata(dir.join("out.bin")).unwrap().len() > 1000);
}

#[test]
fn a_tool_failure_points_at_its_log() {
    if support::require_yosys().is_none() {
        return;
    }
    let src = support::repo().join("tests/fixtures/extern/pll.mimz");
    let bad = support::repo().join("tests/fixtures/build/bad_extern.v");
    let dir = scratch("badext");
    std::fs::write(
        dir.join("p.pcf"),
        "set_io sysclk 35\nset_io fast_clk 11\nset_io pll_ok 37\n",
    )
    .unwrap();
    let out = build(&[
        src.to_str().unwrap(),
        "--pcf",
        dir.join("p.pcf").to_str().unwrap(),
        "--extern-src",
        bad.to_str().unwrap(),
        "--work",
        dir.join("work").to_str().unwrap(),
        "-o",
        dir.join("x.bin").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("yosys failed") && err.contains("yosys.log"),
        "{err}"
    );
    let work = dir.join("work");
    assert!(
        err.contains(work.to_str().unwrap()),
        "log path is not under --work: {err}"
    );
}
