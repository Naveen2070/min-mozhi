//! Checker unit tests — one per rule/error code, plus clean-pass cases.
//! Error-path tests assert on the CODE (stable contract) and message
//! substrings (loose, so wording can be polished).

use crate::diag::Diag;
use crate::{lexer, parser};

use super::check;

mod arrays;
mod bundles;
mod clocks;
mod drivers;
mod enums;
mod funcs_and_loops;
mod insts;
mod names_and_consts;
mod patterns;
mod regressions;
mod widths;

fn parse(src: &str) -> crate::ast::File {
    let toks = lexer::lex(src).expect("lexes");
    parser::parse(toks).expect("parses")
}

fn check_one(src: &str) -> Result<(), Vec<Diag>> {
    check(&[parse(src)])
}

/// Lex, parse, check and emit a single (import-free) source string to
/// Verilog. A local, single-file stand-in for `crate::compile_string`
/// (which lives in the root crate's command runner, out of reach for
/// mimz-core's own tests) — same pipeline, minus `import` resolution.
fn compile_one(src: &str) -> Result<String, Vec<Diag>> {
    let mut asts = vec![parse(src)];
    check(&asts)?;
    crate::emit_verilog::transliterate(&mut asts);
    let project = crate::emit_verilog::Project::from_files(&asts)?;
    crate::emit_verilog::emit(&project, &asts)
}

fn errs(src: &str) -> Vec<Diag> {
    check_one(src).expect_err("expected checker errors")
}

/// True if any diagnostic carries `code` (some codes may not be the FIRST
/// error, since a forbidden construct also trips later passes). Shared
/// helper — used across several split-out test modules, not just the one
/// (`funcs_and_loops.rs`) that originally physically contained it.
fn any_code(src: &str, code: &str) -> bool {
    errs(src).iter().any(|d| d.code == Some(code))
}

/// First error must carry the expected code; returns it for further asserts.
fn first_err(src: &str, code: &str) -> Diag {
    let diags = errs(src);
    assert_eq!(
        diags[0].code,
        Some(code),
        "expected {code}, got {:?}: {}",
        diags[0].code,
        diags[0].msg
    );
    diags.into_iter().next().unwrap()
}

/// Like [`first_err`], but takes a pre-built file slice instead of parsing
/// one string — needed for scenarios that are inherently multi-file (e.g.
/// cross-file ambiguity).
fn first_err_multi(files: &[crate::ast::File], code: &str) -> Diag {
    let diags = check(files).expect_err("expected checker errors");
    assert_eq!(
        diags[0].code,
        Some(code),
        "expected {code}, got {:?}: {}",
        diags[0].code,
        diags[0].msg
    );
    diags.into_iter().next().unwrap()
}

/// Like [`errs`], but takes a pre-built file slice instead of parsing one
/// string — needed for scenarios that are inherently multi-file.
fn errs_multi(files: &[crate::ast::File]) -> Vec<Diag> {
    check(files).expect_err("expected checker errors")
}

const COUNTER: &str = "module Counter(WIDTH: int = 8) {
  clock clk
  reset rst
  out count: bits[WIDTH]
  reg value: bits[WIDTH] = 0
  on rise(clk) {
    value <- value +% 1
  }
  count = value
}
";
