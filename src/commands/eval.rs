// ----------------------------------------------------------------- eval

use std::path::Path;
use std::process::ExitCode;

use mimz::{diag, lexer, morph, parser, project, sim};

use super::helpers::{parse_bindings, parse_u128, resolve_lang};
use crate::Output;

/// `mimz eval <file> --in a=3,b=5` — interpret a combinational module and print
/// each output. Lexes/parses the file directly (no import resolution — the
/// evaluator is single-module, combinational only) and reports a clear message
/// on anything out of that scope.
pub(crate) fn eval_file(
    path: &Path,
    inputs: &str,
    param: &str,
    module: Option<String>,
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
    // Non-fatal mixed-flavor warning (W0001) — printed, never blocks eval.
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
    match sim::comb::eval_outputs(&file, module.as_deref(), &inputs, &params) {
        Ok(outputs) => {
            for o in outputs {
                let kind = if o.signed { "signed" } else { "bits" };
                println!("{} = {}  ({kind}[{}])", o.name, o.value, o.width);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
