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
mod hoist_declaration_order;
mod module_scoped_enums;
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

/// Minimal `Emitter` for unit-level tests that need to call a `&mut self`
/// method directly without driving the whole `emit()` pipeline. Mirrors
/// `emit()`'s own `Emitter` struct literal field-for-field (`module/
/// tests.rs`'s own `test_emitter` does the same, one module level down,
/// per that file's own doc comment — no shared helper exists across the
/// two test trees).
fn test_emitter<'a>(project: &'a Project<'a>) -> Emitter<'a> {
    Emitter {
        project,
        out: String::new(),
        diags: Vec::new(),
        cur_file: 0,
        env: Env::new(),
        module_envs: HashMap::new(),
        repeat_budget: REPEAT_BUDGET,
        clog2_fn_used: false,
        emitting_port: false,
        funcs_used: Vec::new(),
        bundle_sigs: HashMap::new(),
        hoist_counter: 0,
        hoisted_decls: String::new(),
        pre_decl_hoisted_decls: String::new(),
        in_pre_decl_render: false,
        cur_decls: Default::default(),
        in_fn_body: false,
        fn_hoist_counter: 0,
        fn_hoisted_regs: String::new(),
        fn_hoisted_stmts: Vec::new(),
        cover_ordinals: HashMap::new(),
        declared_signal_names: std::collections::HashSet::new(),
        cur_module_enums: HashMap::new(),
    }
}
