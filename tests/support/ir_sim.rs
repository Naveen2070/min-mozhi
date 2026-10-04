//! IR-vs-Icarus plumbing: lower a corpus module, drive `ir::exec` and the
//! emitted Verilog with the same stimulus, and compare per-tick outputs.
//! Shared by `tests/ir_verilog_diff.rs` (Task 4) and the post-synthesis diff
//! (Task 7). Dead-code is allowed once, in `support/mod.rs`.

use mimz_core::ast::Dir;
use mimz_core::backend::verilog::Emitted;
use mimz_core::ir::exec::Executor;
use mimz_core::ir::validate::validate;
use mimz_core::ir::{CellKind, Module};
use mimz_core::value::Val;
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

/// Every `.mimz` file under `dir`, recursively (unsorted).
pub(crate) fn mimz_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            mimz_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "mimz") {
            out.push(path);
        }
    }
}

/// Every module of the file at `path`, lowered as the top: `(name, Some(module))`
/// when it lowers `validate`-clean, `(name, None)` otherwise. A file that does
/// not load, check or name any module yields one `("<file>", None)` entry.
pub(crate) fn lowered_modules(path: &Path) -> Vec<(String, Option<Module>)> {
    let fail = || vec![("<file>".to_string(), None)];
    let Ok(files) = mimz::project::load_project(path) else {
        return fail();
    };
    let asts: Vec<mimz_core::ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    if mimz_core::checker::check(&asts).is_err() {
        return fail();
    }
    // `load_project` always puts the entry file at `files[0]`.
    let names: Vec<String> = asts[0]
        .items
        .iter()
        .filter_map(|i| match i {
            mimz_core::ast::TopItem::Module(m) => Some(m.name.name.clone()),
            _ => None,
        })
        .collect();
    if names.is_empty() {
        return fail();
    }
    names
        .into_iter()
        .map(|name| {
            let module =
                mimz_core::elaborate::elaborate_project(&asts, Some(&name), &Default::default())
                    .ok()
                    .and_then(|design| {
                        catch_unwind(AssertUnwindSafe(|| mimz_core::ir::lower(&design))).ok()
                    })
                    .filter(|m| validate(m).is_empty());
            (name, module)
        })
        .collect()
}

/// IR input ports whose net is some `Dff`/`Adff` clock or a `Mem` `clock` pin.
pub(crate) fn clock_ports(m: &Module) -> HashSet<String> {
    let mut nets = HashSet::new();
    for c in &m.cells {
        match &c.kind {
            CellKind::Dff { clock, .. } | CellKind::Adff { clock, .. } => {
                nets.insert(*clock);
            }
            CellKind::Mem { .. } => {
                if let Some(b) = c.pins.get("clock") {
                    nets.extend(b.nets.iter().copied());
                }
            }
            _ => {}
        }
    }
    m.ports
        .iter()
        .filter(|(_, b, d)| *d == Dir::In && b.nets.iter().any(|n| nets.contains(n)))
        .map(|(n, _, _)| n.clone())
        .collect()
}

/// Same pattern as `ir_opt_corpus.rs`'s trace; clock inputs held at 0 (the
/// testbench toggles them, `ir::exec::tick` advances every register anyway).
pub(crate) fn stimulus(m: &Module, ticks: u32) -> Vec<Vec<(String, u128)>> {
    let clocks = clock_ports(m);
    (0..ticks as u128)
        .map(|t| {
            m.ports
                .iter()
                .enumerate()
                .filter(|(_, (_, _, d))| *d == Dir::In)
                .map(|(i, (n, b, _))| {
                    let raw = if clocks.contains(n) {
                        0
                    } else {
                        (t + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15_F39C_C060_5CED_C835)
                            ^ ((i as u128) << 7)
                    };
                    (n.clone(), raw & super::mask(b.width()))
                })
                .collect()
        })
        .collect()
}

pub(crate) fn exec_trace(m: &Module, stim: &[Vec<(String, u128)>]) -> Vec<Vec<(String, u128)>> {
    let mut ex = Executor::new(m);
    stim.iter()
        .map(|inputs| {
            for (n, v) in inputs {
                let w = m.ports.iter().find(|(p, ..)| p == n).unwrap().1.width();
                ex.set_input(n, Val::new(*v, w, false));
            }
            ex.tick();
            m.ports
                .iter()
                .filter(|(_, _, d)| *d == Dir::Out)
                .map(|(n, _, _)| (n.clone(), ex.get_output(n).bits_small_or_zero()))
                .collect()
        })
        .collect()
}

/// Per tick: drive the inputs, settle, one rising then one falling edge on
/// every clock input, settle, print every output as `DIFF <t> <name>=<bits>`.
pub(crate) fn testbench(
    e: &Emitted,
    clocks: &HashSet<String>,
    stim: &[Vec<(String, u128)>],
) -> String {
    let vname = |ir: &str| e.ports.iter().find(|(n, ..)| n == ir).unwrap().1.clone();
    let mut s = String::from("`timescale 1ns/1ps\nmodule diff_tb;\n");
    for (_, v, w, d) in &e.ports {
        let kw = if *d == Dir::In { "reg" } else { "wire" };
        let init = if *d == Dir::In { " = 0" } else { "" };
        s += &format!("  {kw} [{}:0] {v}{init};\n", w - 1);
    }
    let conns: Vec<String> = e
        .ports
        .iter()
        .map(|(_, v, _, _)| format!(".{v}({v})"))
        .collect();
    s += &format!("  {} uut ({});\n  initial begin\n", e.top, conns.join(", "));
    let outs: Vec<&String> = e
        .ports
        .iter()
        .filter(|p| p.3 == Dir::Out)
        .map(|p| &p.1)
        .collect();
    let fmt: Vec<String> = outs.iter().map(|v| format!("{v}=%b")).collect();
    let args: Vec<String> = outs.iter().map(|v| v.to_string()).collect();
    // Sorted so the generated testbench is deterministic.
    let mut clks: Vec<&String> = clocks.iter().collect();
    clks.sort();
    for (t, inputs) in stim.iter().enumerate() {
        for (n, val) in inputs {
            if !clocks.contains(n) {
                let w = e.ports.iter().find(|p| &p.0 == n).unwrap().2;
                s += &format!("    {} = {w}'d{val};\n", vname(n));
            }
        }
        s += "    #1;\n";
        for c in &clks {
            s += &format!("    {} = 1;\n", vname(c));
        }
        s += "    #1;\n";
        for c in &clks {
            s += &format!("    {} = 0;\n", vname(c));
        }
        s += "    #1;\n";
        if outs.is_empty() {
            s += &format!("    $display(\"DIFF {t}\");\n");
        } else {
            s += &format!(
                "    $display(\"DIFF {t} {}\", {});\n",
                fmt.join(" "),
                args.join(", ")
            );
        }
    }
    s += "    $finish;\n  end\nendmodule\n";
    s
}

/// `parse_icarus` of `stdout`, mapped back to IR output-port names, per tick,
/// in IR output-port order.
pub(crate) fn icarus_trace(stdout: &str, e: &Emitted) -> Vec<Vec<(String, u128)>> {
    super::parse_icarus(stdout)
        .values()
        .map(|row| {
            e.ports
                .iter()
                .filter(|p| p.3 == Dir::Out)
                .map(|(ir, v, _, _)| (ir.clone(), row[v]))
                .collect()
        })
        .collect()
}

/// `label` with everything outside ASCII alphanumerics replaced by `_`.
pub(crate) fn file_safe(label: &str) -> String {
    label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Like `support::run_vvp`, but with many design files and extra `iverilog`
/// arguments (placed before the files).
pub(crate) fn run_iverilog(
    bin: &Path,
    label: &str,
    files: &[PathBuf],
    tb: &str,
    extra_args: &[&str],
) -> String {
    // Module names may be Tamil; keep temp file names ASCII. The process id
    // keeps two test binaries sharing this helper from colliding.
    let safe = format!("{}_{}", std::process::id(), file_safe(label));
    let tb_path = std::env::temp_dir().join(format!("mimz_irsim_{safe}.v"));
    std::fs::write(&tb_path, tb).unwrap();
    let vvp_out = std::env::temp_dir().join(format!("mimz_irsim_{safe}.vvp"));
    let build = super::tool(bin, "iverilog")
        .arg("-o")
        .arg(&vvp_out)
        .args(["-s", "diff_tb"])
        .args(extra_args)
        .arg(&tb_path)
        .args(files)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "iverilog failed on the {label} differential testbench:\n{}\n--- tb ---\n{tb}",
        String::from_utf8_lossy(&build.stderr)
    );
    let sim = super::tool(bin, "vvp")
        .current_dir(std::env::temp_dir())
        .arg(&vvp_out)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&tb_path);
    let _ = std::fs::remove_file(&vvp_out);
    let stdout = String::from_utf8_lossy(&sim.stdout).to_string();
    assert!(sim.status.success(), "vvp failed on {label}:\n{stdout}");
    stdout
}
