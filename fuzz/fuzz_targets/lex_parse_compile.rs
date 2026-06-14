#![no_main]
//! Fuzz the Verilog BACKEND path: any byte string must lex → parse → check →
//! transliterate → build-project → emit to Verilog text or a clean `Diag`/`Err`,
//! never panic/abort/hang. Companion to `lex_parse_eval` (which fuzzes the
//! evaluator); this one drives the emitter, which `eval` never touches.
//!
//! Single in-memory file: cross-file `import`s do not resolve here, so an
//! import-using input simply fails name resolution or project assembly with a
//! clean `Err` — that is fine. Only a crash is a finding.
use libfuzzer_sys::fuzz_target;
use unicode_normalization::UnicodeNormalization;

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

    let mut asts = vec![file];
    // Only emit code that passed every checker pass — the emitter assumes a
    // checked AST (same contract as the CLI's `compile`).
    if mimz::checker::check(&asts).is_err() {
        return;
    }
    mimz::emit_verilog::transliterate(&mut asts);
    let Ok(project) = mimz::emit_verilog::Project::from_files(&asts) else {
        return;
    };
    let _ = mimz::emit_verilog::emit(&project, &asts);
});
