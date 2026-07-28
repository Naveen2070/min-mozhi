use super::*;
use crate::{lexer, parser};

mod builtin_and_loops;
mod bundle_flatten;
mod clocking;
mod clog2;
mod consts_and_translit;
mod consts_scoping;
mod extern_and_arrays;
mod fn_loop;
mod structural_match;
mod valid_bundle_sugar;

fn parse(src: &str) -> File {
    parser::parse(lexer::lex(src).unwrap()).unwrap()
}

/// Emit one self-contained source (no imports) to Verilog text.
fn emit_src(src: &str) -> String {
    let files = [parse(src)];
    let project = Project::from_files(&files).unwrap();
    emit(&project, &files).expect("emit should succeed")
}

/// Emit one source expecting failure; return the diagnostics.
fn emit_src_err(src: &str) -> Vec<Diag> {
    let files = [parse(src)];
    let project = Project::from_files(&files).unwrap();
    emit(&project, &files).expect_err("emit should fail")
}

/// Like [`emit_src`], but with the transliteration pre-pass — the
/// path the CLI takes.
fn emit_src_translit(src: &str) -> String {
    let mut files = [parse(src)];
    transliterate(&mut files);
    let project = Project::from_files(&files).unwrap();
    emit(&project, &files).expect("emit should succeed")
}
