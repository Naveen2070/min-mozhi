//! The three toolchain steps of `mimz build`, in a work folder:
//! yosys (synth_ice40) -> nextpnr-ice40 -> icepack.

use super::toolchain::{Toolchain, needs_noabc};
use std::path::{Path, PathBuf};

pub struct FlowInput<'a> {
    pub top: &'a str,
    pub verilog: &'a str,
    pub extern_files: &'a [PathBuf],
    pub pcf: &'a str,
    pub device: &'a str,
    pub package: &'a str,
    pub freq_mhz: u32,
    pub work: &'a Path,
    pub out_bin: &'a Path,
}

pub struct FlowReport {
    /// `SB_*` cell lines of Yosys's `stat`.
    pub cells: Vec<String>,
    /// One `clk: 80.62 MHz (PASS at 12.00 MHz)` per clock: the final
    /// (post-route) result only.
    pub max_freq: Vec<String>,
    pub noabc: bool,
}

pub enum FlowError {
    ToolMissing(String),
    ToolFailed {
        tool: String,
        log: PathBuf,
        tail: String,
    },
    Io(String),
    /// A Verilog file path holding `"`: Yosys's `-p` script has no escape for
    /// it inside a quoted argument.
    QuoteInPath(PathBuf),
}

pub fn yosys_script(top: &str, verilog_files: &[PathBuf], noabc: bool) -> String {
    // Quoted so a path with a space stays one argument.
    let files: Vec<String> = verilog_files
        .iter()
        .map(|p| format!("\"{}\"", p.display()))
        .collect();
    let noabc = if noabc { " -noabc" } else { "" };
    format!(
        "read_verilog {}; synth_ice40{noabc} -top {top} -json synth.json; write_verilog -noattr synth.v; tee -q -o stat.txt stat",
        files.join(" ")
    )
}

/// `clk: 80.62 MHz (PASS at 12.00 MHz)` per clock, from nextpnr's log.
/// nextpnr prints a pre-route estimate then the post-route result, so only
/// the last line per clock is kept (clocks in first-seen order); nextpnr's
/// `$...` suffix on a clock name is dropped unless that would make two clocks
/// share a name.
pub fn max_freq_lines(log: &str) -> Vec<String> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (_, rest) in log
        .lines()
        .filter_map(|l| l.split_once("Max frequency for clock '"))
    {
        let Some((clk, f)) = rest.split_once("': ") else {
            continue;
        };
        // Keyed by the full name so clocks differing only after a `$` stay apart.
        let f = f.trim().to_string();
        match out.iter_mut().find(|(c, _)| c == clk) {
            Some(e) => e.1 = f,
            None => out.push((clk.to_string(), f)),
        }
    }
    let short = |c: &str| c.split('$').next().unwrap_or(c).to_string();
    // Shortened names that collide fall back to the full nextpnr name.
    out.iter()
        .map(|(c, f)| {
            let shared = out.iter().filter(|(o, _)| short(o) == short(c)).count() > 1;
            format!("{}: {f}", if shared { c.clone() } else { short(c) })
        })
        .collect()
}

fn tail(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(15)..].join("\n")
}

fn step(
    tc: &Toolchain,
    tool: &str,
    args: &[String],
    work: &Path,
    log: &str,
) -> Result<(), FlowError> {
    if tc.find(tool).is_none() {
        return Err(FlowError::ToolMissing(tool.to_string()));
    }
    let log_path = work.join(log);
    let out = tc
        .command(tool)
        .args(args)
        .current_dir(work)
        .output()
        .map_err(|e| FlowError::Io(format!("cannot run {tool}: {e}")))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text += &String::from_utf8_lossy(&out.stderr);
    // yosys (-l) and nextpnr (--log) write their own log; icepack has none.
    if !log_path.exists() {
        std::fs::write(&log_path, &text).map_err(|e| FlowError::Io(e.to_string()))?;
    }
    if !out.status.success() {
        return Err(FlowError::ToolFailed {
            tool: tool.to_string(),
            tail: tail(&log_path),
            log: log_path,
        });
    }
    Ok(())
}

pub fn run(tc: &Toolchain, input: &FlowInput) -> Result<FlowReport, FlowError> {
    let io = |e: std::io::Error| FlowError::Io(e.to_string());
    // Checked on the absolute path, exactly as the script will spell it.
    for f in input.extern_files {
        let abs = std::path::absolute(f).map_err(io)?;
        if abs.to_string_lossy().contains('"') {
            return Err(FlowError::QuoteInPath(abs));
        }
    }
    std::fs::create_dir_all(input.work).map_err(io)?;
    for f in [
        "yosys.log",
        "nextpnr.log",
        "icepack.log",
        "synth.json",
        "synth.v",
        "top.asc",
        "stat.txt",
    ] {
        let _ = std::fs::remove_file(input.work.join(f)); // a re-run never reads stale output
    }
    // A failed re-run never leaves a stale bitstream behind.
    let _ = std::fs::remove_file(input.out_bin);
    std::fs::write(input.work.join("top.v"), input.verilog).map_err(io)?;
    std::fs::write(input.work.join("pins.pcf"), input.pcf).map_err(io)?;
    let mut files = vec![PathBuf::from("top.v")];
    for f in input.extern_files {
        files.push(std::path::absolute(f).map_err(io)?);
    }
    let noabc = needs_noabc();
    let script = yosys_script(input.top, &files, noabc);
    step(
        tc,
        "yosys",
        &[
            "-q".into(),
            "-l".into(),
            "yosys.log".into(),
            "-p".into(),
            script,
        ],
        input.work,
        "yosys.log",
    )?;
    let freq = input.freq_mhz.to_string();
    step(
        tc,
        "nextpnr-ice40",
        &[
            format!("--{}", input.device),
            "--package".into(),
            input.package.into(),
            "--json".into(),
            "synth.json".into(),
            "--pcf".into(),
            "pins.pcf".into(),
            "--asc".into(),
            "top.asc".into(),
            "--freq".into(),
            freq,
            "--log".into(),
            "nextpnr.log".into(),
        ],
        input.work,
        "nextpnr.log",
    )?;
    let bin = std::path::absolute(input.out_bin).map_err(io)?;
    step(
        tc,
        "icepack",
        &["top.asc".into(), bin.display().to_string()],
        input.work,
        "icepack.log",
    )?;
    let stat = std::fs::read_to_string(input.work.join("stat.txt")).unwrap_or_default();
    let cells = stat
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("SB_"))
        .map(str::to_string)
        .collect();
    let max_freq = max_freq_lines(
        &std::fs::read_to_string(input.work.join("nextpnr.log")).unwrap_or_default(),
    );
    Ok(FlowReport {
        cells,
        max_freq,
        noabc,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_yosys_script_reads_every_file_and_synthesizes_the_top() {
        let s = yosys_script(
            "Blinker",
            &[PathBuf::from("top.v"), PathBuf::from("pll.v")],
            false,
        );
        assert_eq!(
            s,
            "read_verilog \"top.v\" \"pll.v\"; synth_ice40 -top Blinker -json synth.json; write_verilog -noattr synth.v; tee -q -o stat.txt stat"
        );
    }

    #[test]
    fn clocks_that_differ_only_after_a_dollar_are_kept_apart() {
        let log = "\
Info: Max frequency for clock 'clk$a': 10.00 MHz (PASS at 12.00 MHz)
Info: Max frequency for clock 'clk$b': 20.00 MHz (PASS at 12.00 MHz)
Info: Max frequency for clock 'clk$a': 11.00 MHz (FAIL at 12.00 MHz)
";
        assert_eq!(
            max_freq_lines(log),
            vec![
                "clk$a: 11.00 MHz (FAIL at 12.00 MHz)".to_string(),
                "clk$b: 20.00 MHz (PASS at 12.00 MHz)".to_string()
            ]
        );
    }

    #[test]
    fn a_quote_in_an_extern_path_is_rejected_before_anything_runs() {
        let work = std::env::temp_dir().join(format!("mimz_flow_quote_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        let externs = [PathBuf::from("dir").join("a\"b.v")];
        let r = run(
            &Toolchain { root: None },
            &FlowInput {
                top: "T",
                verilog: "",
                extern_files: &externs,
                pcf: "",
                device: "up5k",
                package: "sg48",
                freq_mhz: 12,
                work: &work,
                out_bin: &work.join("o.bin"),
            },
        );
        assert!(matches!(r, Err(FlowError::QuoteInPath(p)) if p.to_string_lossy().contains('"')));
        assert!(!work.exists(), "nothing is created before the check");
    }

    #[test]
    fn noabc_is_passed_to_synth_ice40() {
        assert!(
            yosys_script("T", &[PathBuf::from("top.v")], true)
                .contains("synth_ice40 -noabc -top T")
        );
    }

    #[test]
    fn max_frequency_lines_are_picked_from_the_nextpnr_log() {
        let log = "Info: x\nInfo: Max frequency for clock 'clk': 80.62 MHz (PASS at 12.00 MHz)\n";
        assert_eq!(
            max_freq_lines(log),
            vec!["clk: 80.62 MHz (PASS at 12.00 MHz)".to_string()]
        );
    }

    #[test]
    fn only_the_last_max_frequency_line_per_clock_survives_with_its_suffix_stripped() {
        let log = "\
Info: Max frequency for clock 'clk$SB_IO_IN_$glb_clk': 85.78 MHz (PASS at 12.00 MHz)
Info: Max frequency for clock 'other': 50.00 MHz (PASS at 12.00 MHz)
Info: Max frequency for clock 'clk$SB_IO_IN_$glb_clk': 9.10 MHz (FAIL at 12.00 MHz)
";
        assert_eq!(
            max_freq_lines(log),
            vec![
                "clk: 9.10 MHz (FAIL at 12.00 MHz)".to_string(),
                "other: 50.00 MHz (PASS at 12.00 MHz)".to_string()
            ]
        );
    }
}
