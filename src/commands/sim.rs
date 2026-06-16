// ------------------------------------------------------------------ sim

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mimz::sim::run::{SimOpts, run};
use mimz::sim::{elaborate, trace, vcd};
use mimz::{diag, lexer, morph, parser, project};

use super::helpers::{parse_bindings, parse_u128, resolve_lang, trace_scope};
use crate::Output;

/// `mimz sim <file>` — simulate a clocked module under a default stimulus
/// (reset asserted the first cycle, inputs held, the clock toggled for
/// `cycles`) and emit a VCD waveform (`-o`) and/or a console trace (`--trace`).
/// Single-file/single-module for now (like `mimz eval`); a clockless module is
/// rejected with a pointer to `mimz eval`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sim_file(
    path: &Path,
    output: Option<PathBuf>,
    cycles: u64,
    clock: Option<String>,
    inputs: &str,
    param: &str,
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
    let src = match project::read_source(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let path_str = path.display().to_string();
    let tokens = match lexer::lex(&src) {
        Ok(t) => t,
        Err(diags) => return out.one_file(&diags, &src, &path_str),
    };
    if let Some(w) = morph::flavor_mix_warning(&tokens) {
        eprint!("{}", diag::render_lang(&[w], &src, &path_str, flavor));
    }
    let file = match parser::parse(tokens) {
        Ok(f) => f,
        Err(diags) => return out.one_file(&diags, &src, &path_str),
    };

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

    let design = match elaborate::elaborate(&file, module.as_deref(), &params) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    // Capture the trace-scope groups before `run` consumes the design.
    let in_names: Vec<String> = design.inputs.iter().map(|s| s.name.clone()).collect();
    let out_names: Vec<String> = design.outputs.iter().map(|s| s.name.clone()).collect();
    let reg_names: Vec<String> = design.regs.iter().map(|r| r.name.clone()).collect();

    let opts = SimOpts {
        clock,
        inputs,
        cycles,
        reset_cycles: 1,
    };
    let timeline = match run(design, &opts) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // VCD waveform — only when `-o` is given.
    if let Some(dest) = &output {
        if let Err(e) = std::fs::write(dest, vcd::to_vcd(&timeline)) {
            eprintln!("error: writing VCD to {}: {e}", dest.display());
            return ExitCode::FAILURE;
        }
        println!(
            "wrote {} ({cycles} cycles) — open in GTKWave",
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
        println!(
            "simulated {cycles} cycle(s) of `{}` — pass -o <file.vcd> for a waveform \
             or --trace for a console trace",
            timeline.module
        );
    }
    ExitCode::SUCCESS
}
