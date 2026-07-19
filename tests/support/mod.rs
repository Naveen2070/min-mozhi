//! Shared Icarus-differential plumbing, reused by `tests/icarus.rs` and
//! `tests/differential_fuzz.rs`. Each `tests/*.rs` file compiles as its
//! own crate, so this is the standard `tests/<name>/mod.rs` sharing
//! pattern — pulled in via `mod support;` from each consumer.
//!
//! `#![allow(dead_code)]`: each consuming binary only uses a subset of
//! these functions, so per-binary `dead_code` would otherwise fire on
//! whichever half a given file doesn't call — the two files together use
//! all of them, just not each alone.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub(crate) fn mimz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mimz"))
}

/// Locate the Icarus `bin` directory: `MIMZ_IVERILOG` (a directory or the
/// iverilog executable itself) → PATH → the Windows installer default.
/// `None` means "not installed" — the caller decides skip vs fail.
pub(crate) fn iverilog_bin() -> Option<PathBuf> {
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
pub(crate) fn require_iverilog() -> Option<PathBuf> {
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

pub(crate) fn tool(bin: &Path, name: &str) -> Command {
    if bin.as_os_str().is_empty() {
        Command::new(name)
    } else {
        Command::new(bin.join(name))
    }
}

/// Compile one example with mimz; return the generated `.v` path.
pub(crate) fn compile_example(path: &Path) -> PathBuf {
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

/// Low-`w`-bits mask (`w >= 128` ⇒ all ones).
pub(crate) fn mask(w: u32) -> u128 {
    if w >= 128 {
        u128::MAX
    } else {
        (1u128 << w) - 1
    }
}

/// `name #(.P(v), …) uut (conns)` — the instantiation line, with optional
/// parameter overrides (so a design can be driven at a chosen width/limit).
pub(crate) fn instantiation(module: &str, params: &[(String, i128)], conns: &[String]) -> String {
    let p = if params.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = params.iter().map(|(n, v)| format!(".{n}({v})")).collect();
        format!(" #({})", items.join(", "))
    };
    format!("  {module}{p} uut ({});\n", conns.join(", "))
}

/// Combinational testbench: instantiate (no clock/reset), and for each input
/// vector set the inputs, settle (`#1`), and print `DIFF <i> <out>=<bits> …`.
pub(crate) fn comb_testbench(
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

/// Clocked testbench: instantiate, apply the default stimulus (reset
/// asserted for `reset_cycles`, inputs held constant), and print
/// `DIFF <cycle> <out>=<bits> …` (binary) after each rising edge.
#[allow(clippy::too_many_arguments)]
pub(crate) fn clocked_testbench(
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

pub(crate) fn display_fmt(outputs: &[(String, u32)]) -> String {
    outputs
        .iter()
        .map(|(n, _)| format!("{n}=%b"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn display_args(outputs: &[(String, u32)]) -> String {
    outputs
        .iter()
        .map(|(n, _)| n.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Deterministic pseudo-random input vectors, each value masked to its input's
/// width — the same vectors fed to our kernel and the Verilog testbench.
pub(crate) fn gen_vectors(inputs: &[(String, u32)], n: u64) -> Vec<BTreeMap<String, u128>> {
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

/// Build + run a testbench under `iverilog`/`vvp`; return vvp's stdout.
pub(crate) fn run_vvp(bin: &Path, example: &str, design_v: &Path, tb: &str) -> String {
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
pub(crate) fn parse_icarus(stdout: &str) -> BTreeMap<u64, BTreeMap<String, u128>> {
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
