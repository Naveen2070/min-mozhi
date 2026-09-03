//! Golden IR-text snapshot tests (Phase 2 IR plan, Task 17).
//!
//! For a handful of real `.mimz` examples, compile through the full
//! load -> check -> elaborate -> lower -> print_line pipeline and lock the
//! resulting IR text against `tests/golden/ir/<name>.ir` — so an
//! unintended change to lowering or printing gets caught the same way
//! `tests/examples.rs`'s Verilog goldens catch emitter drift.
//!
//! To regenerate after an INTENDED lowering/printer change:
//! `MIMZ_UPDATE_GOLDENS=1 cargo test --test ir_golden` — then review the
//! diff like any other code change.

use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn check_golden(example_path: &str, golden_path: &str) {
    let update = std::env::var("MIMZ_UPDATE_GOLDENS").is_ok();

    let full_path = repo().join(example_path);
    let files = match mimz::project::load_project(&full_path) {
        Ok(files) => files,
        // `LoadError` intentionally has no `Debug` (diagnostics render via
        // `render_diags`, not `{:?}`) — see tests/stdlib.rs's own note.
        Err(_) => panic!("failed to load {}", full_path.display()),
    };
    let asts: Vec<mimz_core::ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    if let Err(errors) = mimz_core::checker::check(&asts) {
        panic!("{} failed checking: {errors:?}", full_path.display());
    }
    let design = mimz_core::elaborate::elaborate_project(&asts, None, &Default::default())
        .unwrap_or_else(|e| panic!("{} failed to elaborate: {e:?}", full_path.display()));
    let module = mimz_core::ir::lower(&design);
    let text = mimz_core::ir::print_line::print(&module);

    let golden_full = repo().join(golden_path);
    if update {
        std::fs::create_dir_all(golden_full.parent().unwrap()).unwrap();
        std::fs::write(&golden_full, &text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&golden_full)
        .unwrap_or_else(|_| {
            panic!(
                "missing golden {} — run with MIMZ_UPDATE_GOLDENS=1 to create it",
                golden_full.display()
            )
        })
        .replace("\r\n", "\n");
    assert_eq!(
        text, want,
        "IR text for {example_path} drifted from {golden_path} — if this is an intentional \
         lowering/printer change, regenerate with `MIMZ_UPDATE_GOLDENS=1 cargo test --test ir_golden` \
         and review the diff"
    );
}

#[test]
fn adder_example_matches_golden_ir() {
    check_golden("examples/english/adder.mimz", "tests/golden/ir/adder.ir");
}

#[test]
fn mux4_example_matches_golden_ir() {
    check_golden("examples/english/mux4.mimz", "tests/golden/ir/mux4.ir");
}

#[test]
fn pll_extern_example_matches_golden_ir() {
    check_golden("tests/fixtures/extern/pll.mimz", "tests/golden/ir/pll.ir");
}

#[test]
fn async_reset_example_matches_golden_ir() {
    check_golden(
        "examples/english/async_reset.mimz",
        "tests/golden/ir/async_reset.ir",
    );
}

#[test]
fn regfile_example_matches_golden_ir() {
    check_golden(
        "examples/english/regfile.mimz",
        "tests/golden/ir/regfile.ir",
    );
}

// NOTE: a comparator.mimz-based control-flow example is blocked for an
// unrelated, already self-documented reason (`Gt`/`Ge`/logical and/or are
// not yet lowered — see the `unimplemented!` in `lower_expr`'s `BinOp`
// arm); `mux4.mimz` above covers the control-flow category instead.
