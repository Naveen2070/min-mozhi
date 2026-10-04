//! Runs the three IR optimizer passes (`fold_constants`, `simplify_muxes`,
//! `eliminate_dead_cells`, together as `ir::opt::optimize`) over every module
//! of every example and extern fixture that lowers `validate`-clean,
//! and checks the result is still `validate`-clean, computes the same
//! outputs, and is a fixpoint. Every module that does not lower is named in
//! `EXPECTED_SKIPS`. See
//! `docs/superpowers/specs/2026-09-27-ir-mux-simplify-design.local.md`.

mod support;

use mimz_core::ast::Dir;
use mimz_core::ir::Module;
use mimz_core::ir::exec::Executor;
use mimz_core::ir::opt::optimize;
use mimz_core::ir::validate::validate;
use mimz_core::value::Val;
use std::path::PathBuf;
use support::ir_sim::{lowered_modules, mimz_files};

/// Every file:module the corpus test skips, with why. A file that starts or
/// stops lowering changes this list, so the test names it. Empty since
/// 2026-10-02: lowering each module of a multi-module file as its own top
/// (`alu.mimz`'s `Alu` and `Top`) left nothing unchecked (230 modules).
const EXPECTED_SKIPS: &[&str] = &[];

/// Every output port's value after each of 4 ticks, with every input port
/// driven by a deterministic per-tick pattern.
fn trace(module: &Module) -> Vec<Val> {
    let mut ex = Executor::new(module);
    let mut seen = Vec::new();
    for tick in 0..4u128 {
        for (i, (name, bits, dir)) in module.ports.iter().enumerate() {
            if *dir == Dir::In {
                let raw = (tick + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15_F39C_C060_5CED_C835)
                    ^ ((i as u128) << 7);
                ex.set_input(name, Val::new(raw, bits.width(), false));
            }
        }
        ex.tick();
        for (name, _, dir) in &module.ports {
            if *dir == Dir::Out {
                seen.push(ex.get_output(name));
            }
        }
    }
    seen
}

#[test]
fn optimizer_passes_preserve_every_example() {
    let mut files = Vec::new();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    mimz_files(&root.join("examples"), &mut files);
    mimz_files(&root.join("tests/fixtures/extern"), &mut files);
    files.sort();
    let mut checked = 0;
    let mut skipped: Vec<String> = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        for (name, module) in lowered_modules(path) {
            let at = format!("{rel}:{name}");
            // `Val::new` takes a u128.
            let Some(module) =
                module.filter(|m| m.ports.iter().all(|(_, bits, _)| bits.width() <= 128))
            else {
                skipped.push(at);
                continue;
            };
            let mut opt = module.clone();
            let rounds = optimize(&mut opt);
            // Tighter than `MAX_ROUNDS`: today's corpus settles in a few rounds.
            assert!(rounds < 20, "{at}: {rounds} rounds");

            let errs = validate(&opt);
            assert!(
                errs.is_empty(),
                "{at}: optimized module fails validate: {errs:?}"
            );
            assert_eq!(trace(&opt), trace(&module), "{at}: outputs changed");
            assert_eq!(
                optimize(&mut opt),
                0,
                "{at}: a second run changed something"
            );
            checked += 1;
        }
    }
    eprintln!("checked {checked} modules");
    assert_eq!(skipped, EXPECTED_SKIPS, "corpus skip list changed");
    assert!(checked > 0, "no example module was checked");
}
