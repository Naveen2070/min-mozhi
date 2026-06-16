// ------------------------------------------------------------------ sim

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mimz::sim::run::{SimOpts, comb_run, run};
use mimz::sim::{elaborate, trace, vcd};
use mimz::{ast, project};

use super::helpers::{
    parse_bindings, parse_sweep, parse_u128, project_warnings, resolve_lang, sweep_vectors,
    trace_scope,
};
use crate::Output;

/// `mimz sim <file>` — simulate a module and emit a VCD waveform (`-o`) and/or a
/// console trace (`--trace`). A **clocked** module runs under the default
/// stimulus (reset asserted the first cycle, inputs held, the clock toggled for
/// `cycles`); a **combinational** module settles once per input vector (held
/// `--in`, or one frame per `--sweep` combination). Single-file/single-module for
/// now (like `mimz eval`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn sim_file(
    path: &Path,
    output: Option<PathBuf>,
    cycles: u64,
    clock: Option<String>,
    inputs: &str,
    param: &str,
    sweep: &str,
    module: Option<String>,
    trace_style: Option<String>,
    verbose: bool,
    signals: Option<String>,
    lang: Option<&str>,
) -> ExitCode {
    let flavor = match resolve_lang(path, lang) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let out = Output::Human(flavor);
    // Load the entry file and all transitive imports, so a module that
    // instantiates a sub-module from another file can be flattened.
    let files = match project::load_project(path) {
        Ok(f) => f,
        Err(e) => return out.load_error(&e),
    };
    let asts: Vec<ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    let warnings = project_warnings(&files);
    if !warnings.is_empty() {
        eprint!("{}", project::render_diags_lang(&warnings, &files, flavor));
    }

    let inputs = match parse_bindings(inputs, parse_u128) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let params = match parse_bindings(param, |s| parse_u128(s).map(|v| v as i128)) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let sweep = match parse_sweep(sweep) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let design = match elaborate::elaborate_project(&asts, module.as_deref(), &params) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    // Capture the trace-scope groups + whether the design is clocked before the
    // run consumes it.
    let in_names: Vec<String> = design.inputs.iter().map(|s| s.name.clone()).collect();
    let out_names: Vec<String> = design.outputs.iter().map(|s| s.name.clone()).collect();
    let reg_names: Vec<String> = design.regs.iter().map(|r| r.name.clone()).collect();
    let clocked = !design.clocks.is_empty();

    // Clocked → default stimulus over `cycles`. Combinational → one settled frame
    // per input vector (held `--in`, fanned out by `--sweep`).
    let timeline = if clocked {
        let opts = SimOpts {
            clock,
            inputs,
            cycles,
            reset_cycles: 1,
        };
        match run(design, &opts) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        let vectors = match sweep_vectors(&inputs, &sweep) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        match comb_run(design, &vectors) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    };
    let steps = timeline.frames.iter().filter(|f| f.cycle.is_some()).count();

    // VCD waveform — only when `-o` is given.
    if let Some(dest) = &output {
        if let Err(e) = std::fs::write(dest, vcd::to_vcd(&timeline)) {
            eprintln!("error: writing VCD to {}: {e}", dest.display());
            return ExitCode::FAILURE;
        }
        let unit = if clocked { "cycles" } else { "input vectors" };
        println!(
            "wrote {} ({steps} {unit}) — open in GTKWave",
            dest.display()
        );
    }

    // Console trace — only when `--trace` is given.
    if let Some(style) = &trace_style {
        let all_names: Vec<String> = timeline.signals.iter().map(|s| s.name.clone()).collect();
        let default: Vec<String> = in_names
            .into_iter()
            .chain(out_names)
            .chain(reg_names)
            .collect();
        let scope = match trace_scope(&all_names, &default, verbose, &signals, &timeline.module) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        print!("{}", trace::render(&timeline, style, &scope));
    }

    if output.is_none() && trace_style.is_none() {
        let what = if clocked {
            format!("{steps} cycle(s)")
        } else {
            format!("{steps} input vector(s)")
        };
        println!(
            "simulated {what} of `{}` — pass -o <file.vcd> for a waveform \
             or --trace for a console trace",
            timeline.module
        );
    }
    ExitCode::SUCCESS
}
