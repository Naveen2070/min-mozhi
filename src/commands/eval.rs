//! `mimz eval <file> --in a=3,b=5` — interpret a combinational module and print
//! each output. Single-file/single-module only (no import resolution); the
//! interpreter handles the subset that maps to combinational logic. Reports a
//! clear error on anything out of scope (latches, clocks, multi-module designs).

use std::path::Path;
use std::process::ExitCode;

use mimz::{checker, diag, lexer, morph, parser, project, sim};

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
    _quiet: bool,
    debug: bool,
) -> ExitCode {
    if debug {
        eprintln!("debug: evaluating combinational file {}", path.display());
    }
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
    // A2 (docs/audit/review-2026-07-17.md §3.1): eval must not evaluate a
    // program the checker would reject — `1 << 2` on a bare literal type-checks
    // differently under the sim's width rules than the checker's (E0405), and
    // a program that never sees the checker is a different language. Warnings
    // alone don't block eval, matching `mimz check`'s own error/warning split.
    //
    // Only gated when the file has no `import`/`include`: eval resolves none
    // (single-file only, by design — see module doc comment), so the checker
    // would otherwise reject an unrelated module's unresolved cross-file
    // instance (e.g. `Top` needing `Adder` while `--module Alu` is what's
    // actually being evaluated) instead of letting `comb::eval_outputs`'s own
    // friendlier "sub-module" rejection fire. ponytail: import-bearing files
    // skip this gate entirely; a per-module check that ignores unrelated
    // modules' import errors would close the gap, add if eval ever gains
    // real import resolution.
    if file.imports.is_empty()
        && let Err(diags) = checker::check(std::slice::from_ref(&file))
    {
        let has_error = diags.iter().any(|d| d.is_error());
        let code = out.one_file(&diags, &src, &path_str);
        if has_error {
            return code;
        }
    }
    let inputs = match parse_bindings(inputs, parse_u128) {
        Ok(m) => m
            .into_iter()
            .map(|(k, v)| (k, sim::value::Bits::Small(v)))
            .collect(),
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
    match sim::comb::eval_outputs(
        std::slice::from_ref(&file),
        module.as_deref(),
        &inputs,
        &params,
    ) {
        Ok(outputs) => {
            for o in outputs {
                let kind = if o.signed { "signed" } else { "bits" };
                let value = sim::value::bits_to_decimal_string(&o.value, o.width, o.signed);
                println!("{} = {value}  ({kind}[{}])", o.name, o.width);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
