//! `mimz build <file>` - synthesize a design into an iCE40 bitstream:
//! check -> lower -> optimize -> IR Verilog -> pins -> Yosys -> nextpnr ->
//! icepack. Every user-fixable failure is an `E15xx` error with a `help:`
//! line (docs/code/06-diagnostics.md); a tool failure names the tool, its
//! log and the last log lines.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mimz::backend::verilog::emit;
use mimz::ir::CellKind;
use mimz::ir::failure::{self, Stage};

use super::ir_pipeline::{PipelineOpts, lower_project, report};
use mimz::build::boards::{self, Board};
use mimz::build::flow::{self, FlowError, FlowInput};
use mimz::build::pins::{self, PinError};
use mimz::build::toolchain::Toolchain;

/// Everything `mimz build` resolved from flags and `[build]` config
/// (flags already won).
pub(crate) struct BuildOpts<'a> {
    pub(crate) path: &'a Path,
    pub(crate) output: Option<PathBuf>,
    /// `--work`: replaces the default `<source dir>/build/<top>/`.
    pub(crate) work: Option<PathBuf>,
    pub(crate) module: Option<String>,
    pub(crate) param: &'a str,
    pub(crate) board: Option<String>,
    pub(crate) pcf: Option<PathBuf>,
    pub(crate) freq: Option<u32>,
    /// `[build] toolchain`.
    pub(crate) toolchain: Option<PathBuf>,
    /// Companion Verilog for extern modules (`--extern-src` + config).
    pub(crate) verilog_files: Vec<PathBuf>,
    pub(crate) panic: bool,
    pub(crate) lang: Option<&'a str>,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) quiet: bool,
    pub(crate) debug: bool,
}

/// `error[E15xx]: msg` then `  = help: help`; always exit 1.
fn build_error(code: &str, msg: &str, help: &str) -> ExitCode {
    eprintln!("error[{code}]: {msg}");
    eprintln!("  = help: {help}");
    ExitCode::FAILURE
}

fn quoted(names: &[String]) -> String {
    names
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn build_file(o: BuildOpts) -> ExitCode {
    let l = match lower_project(&PipelineOpts {
        path: o.path,
        module: o.module.clone(),
        param: o.param,
        lang: o.lang,
        config_path: o.config_path,
        no_opt: false,
        panic: o.panic,
        keep_lowered: false,
        debug: o.debug,
    }) {
        Ok(l) => l,
        Err(code) => return code,
    };

    let board: Option<&'static Board> = match o.board.as_deref() {
        None => None,
        Some(name) => match boards::board(name) {
            Some(b) => Some(b),
            None => {
                return build_error(
                    "E1503",
                    &format!(
                        "unknown board `{name}` (known: {})",
                        boards::names().join(", ")
                    ),
                    "pick a listed board, or leave --board out and pin every port with --pcf <file>",
                );
            }
        },
    };

    let emitted = match failure::catch(Stage::Emit, o.debug || o.panic, || emit(&l.module)) {
        Ok(e) => e,
        Err(f) => return report(f, &l.files, l.flavor, &l.top, o.panic, o.debug),
    };

    // Extern modules need their real Verilog.
    let externs: Vec<String> = l
        .module
        .cells
        .iter()
        .filter_map(|c| match &c.kind {
            CellKind::BlackBox { module_name, .. } => Some(module_name.clone()),
            _ => None,
        })
        .collect();
    if let Some(missing) = o.verilog_files.iter().find(|f| !f.is_file()) {
        return build_error(
            "E1505",
            &format!("Verilog file `{}` does not exist", missing.display()),
            "fix the --extern-src path, or the `[compile] verilog_files` entry in mimz.toml",
        );
    }
    if !externs.is_empty() && o.verilog_files.is_empty() {
        let mut names = externs;
        names.sort();
        names.dedup();
        return build_error(
            "E1505",
            &format!(
                "extern module {} has no Verilog source file",
                quoted(&names)
            ),
            "pass --extern-src <file.v> (repeatable) or list the file under `[compile] verilog_files` in mimz.toml",
        );
    }

    // Pins: board preset, overridden by the user's PCF.
    let user = match &o.pcf {
        None => Vec::new(),
        Some(p) => {
            let text = match std::fs::read_to_string(p) {
                Ok(t) => t,
                Err(e) => {
                    return build_error(
                        "E1506",
                        &format!("cannot read PCF file {}: {e}", p.display()),
                        "check the --pcf path (relative to where you run mimz); `[build] pcf` in mimz.toml is relative to mimz.toml",
                    );
                }
            };
            match pins::parse_pcf(&text) {
                Ok(u) => u,
                Err(PinError::BadLine { line, text }) => {
                    return build_error(
                        "E1506",
                        &format!(
                            "{}:{line}: malformed PCF line `{}`",
                            p.display(),
                            text.trim()
                        ),
                        "write each pin as `set_io <port> <pin>`, e.g. `set_io led 11` or `set_io leds[2] 25`",
                    );
                }
                Err(e) => {
                    eprintln!("error: internal: parse_pcf returned {e:?}");
                    return ExitCode::FAILURE;
                }
            }
        }
    };
    let ports: Vec<(String, String, u32)> = emitted
        .ports
        .iter()
        .map(|(ir, v, w, _)| (ir.clone(), v.clone(), *w))
        .collect();
    let pinned = match pins::resolve(board, &user, &ports) {
        Ok(p) => p,
        Err(PinError::Unpinned(names)) => {
            let preset = board
                .map(|b| {
                    let n: Vec<&str> = b.pins.iter().map(|(n, _)| *n).collect();
                    format!("`{}` pins {}", b.name, n.join(", "))
                })
                .unwrap_or_else(|| "no --board given".to_string());
            return build_error(
                "E1501",
                &format!("no pin for top-level port {}", quoted(&names)),
                &format!(
                    "give each a pin with --pcf <file> (`set_io <port> <pin>`) or rename it to a board preset name ({preset})"
                ),
            );
        }
        Err(PinError::NotAPort(names)) => {
            let real: Vec<String> = emitted.ports.iter().map(|p| p.0.clone()).collect();
            return build_error(
                "E1502",
                &format!("PCF names {} which is not a top-level port", quoted(&names)),
                &format!("the top module `{}` has ports {}", l.top, quoted(&real)),
            );
        }
        Err(e @ PinError::BadLine { .. }) => {
            eprintln!("error: internal: pins::resolve returned {e:?}");
            return ExitCode::FAILURE;
        }
    };

    // Tools.
    let tc = Toolchain::discover(o.toolchain.as_deref());
    for tool in ["yosys", "nextpnr-ice40", "icepack"] {
        if tc.find(tool).is_none() {
            return build_error(
                "E1504",
                &format!("`{tool}` not found"),
                "install the OSS CAD Suite and put its bin on PATH or set MIMZ_OSS_CAD (or `[build] toolchain`); see docs/BUILD.md. `mimz doctor` shows what was found",
            );
        }
    }

    let dir = match o.path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let work = o
        .work
        .clone()
        .unwrap_or_else(|| dir.join("build").join(&emitted.top));
    let out_bin = o
        .output
        .clone()
        .unwrap_or_else(|| dir.join(format!("{}.bin", emitted.top)));
    let (device, package) = board.map_or(("up5k", "sg48"), |b| (b.device, b.package));
    let freq_mhz = o.freq.or(board.map(|b| b.freq_mhz)).unwrap_or(12);
    let result = flow::run(
        &tc,
        &FlowInput {
            top: &emitted.top,
            verilog: &emitted.text,
            extern_files: &o.verilog_files,
            pcf: &pins::write_pcf(&pinned),
            device,
            package,
            freq_mhz,
            work: &work,
            out_bin: &out_bin,
        },
    );
    match result {
        Ok(r) => {
            if !o.quiet {
                println!("wrote {}", out_bin.display());
                if r.noabc {
                    println!("note: Windows Yosys build: synth_ice40 -noabc (docs/BUILD.md)");
                }
                for line in r.max_freq.iter().chain(&r.cells) {
                    println!("{line}");
                }
                println!("work folder: {}", work.display());
            }
            ExitCode::SUCCESS
        }
        Err(FlowError::ToolMissing(t)) => build_error(
            "E1504",
            &format!("`{t}` not found"),
            "install the OSS CAD Suite and put its bin on PATH or set MIMZ_OSS_CAD; see docs/BUILD.md",
        ),
        Err(FlowError::ToolFailed { tool, log, tail }) => {
            eprintln!("error: {tool} failed (log: {})", log.display());
            eprintln!("{tail}");
            ExitCode::FAILURE
        }
        Err(FlowError::Io(m)) => {
            eprintln!("error: {m}");
            ExitCode::FAILURE
        }
    }
}
