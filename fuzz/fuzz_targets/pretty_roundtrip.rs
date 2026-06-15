#![no_main]
//! Fuzz the `translate --order` AST pretty-printer for ROUND-TRIP safety: any
//! parseable program, pretty-printed back to source, must (1) re-lex and
//! re-parse — the printer never emits syntax the parser rejects — and (2) when
//! the original is a complete, emittable program, lower to byte-identical
//! Verilog. The unit suite only checks this over the fixed example corpus;
//! libFuzzer drives it on arbitrary input. Only a crash or a Verilog mismatch
//! is a finding.
//!
//! Single in-memory file: a program using `import` simply fails project
//! assembly with a clean `Err`, so it is excluded from the Verilog oracle but
//! still required to re-parse. Pretty-prints in (English, code) order — the
//! canonical default; the round-trip property holds for every (flavor, order).
use libfuzzer_sys::fuzz_target;
use unicode_normalization::UnicodeNormalization;

use mimz::lexer::token::Flavor;
use mimz::pretty::{Order, pretty_print};

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let src: String = src.nfc().collect();

    let Ok(tokens) = mimz::lexer::lex(&src) else {
        return;
    };
    let Ok(file) = mimz::parser::parse(tokens) else {
        return;
    };

    // Lower a parsed AST to Verilog, or `None` if it does not check / assemble
    // (an incomplete or import-using program — not a round-trip concern).
    let emit_one = |f| -> Option<String> {
        let mut asts = vec![f];
        if mimz::checker::check(&asts).is_err() {
            return None;
        }
        mimz::emit_verilog::transliterate(&mut asts);
        let project = mimz::emit_verilog::Project::from_files(&asts).ok()?;
        mimz::emit_verilog::emit(&project, &asts).ok()
    };

    // Pretty-print, then the printed source MUST re-lex and re-parse.
    let printed = pretty_print(&file, Flavor::English, Order::Code);
    let toks2 = mimz::lexer::lex(&printed)
        .unwrap_or_else(|_| panic!("pretty output did not lex:\n{printed}"));
    let file2 = mimz::parser::parse(toks2)
        .unwrap_or_else(|_| panic!("pretty output did not parse:\n{printed}"));

    // For an emittable program, the round-trip must preserve the Verilog.
    if let Some(v1) = emit_one(file) {
        let v2 = emit_one(file2)
            .unwrap_or_else(|| panic!("re-parsed program no longer emits:\n{printed}"));
        assert_eq!(v1, v2, "pretty round-trip changed the Verilog:\n{printed}");
    }
});
