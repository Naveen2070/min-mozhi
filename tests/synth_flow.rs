//! Synthesis v1 design sections 6.3 and 6.4: Yosys's synthesized netlist,
//! simulated in Icarus with Yosys's iCE40 cell models, matches `ir::exec`;
//! and `mimz build` produces a bitstream. Needs the OSS CAD Suite
//! (`REQUIRE_YOSYS=1` makes a missing suite a failure; `MIMZ_OSS_CAD` points
//! at it).

mod support;

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
        let work = std::env::temp_dir().join(format!("mimz_synth_{top}_{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("top.v"), &e.text).unwrap();
        let noabc = if mimz::build::toolchain::needs_noabc() {
            " -noabc"
        } else {
            ""
        };
        let out = tc
            .command("yosys")
            .current_dir(&work)
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
            "{file}:{top}: yosys failed\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let mut stim = stimulus(&m, 8);
        reset_once(&mut stim, *reset);
        let want = exec_trace(&m, &stim);
        // Not vacuous: the oracle's outputs must actually move.
        match *top {
            "Blinker" => assert_eq!(distinct(&want, "led"), 2, "Blinker led never toggles"),
            "Counter" => assert!(distinct(&want, "count") > 2, "Counter barely counts"),
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
        let _ = std::fs::remove_dir_all(&work);
    }
}

/// A unique temp folder per test (process id + tag); never inside the repo.
fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mimz_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
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
    let _ = std::fs::remove_dir_all(&dir);
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
    let _ = std::fs::remove_dir_all(&dir);
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
    let _ = std::fs::remove_dir_all(&dir);
}
