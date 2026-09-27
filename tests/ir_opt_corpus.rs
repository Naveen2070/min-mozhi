//! Runs the three IR optimizer passes (`fold_constants`, `simplify_muxes`,
//! `eliminate_dead_cells`) over every example that lowers `validate`-clean,
//! and checks the result is still `validate`-clean, computes the same
//! outputs, and is a fixpoint. See
//! `docs/superpowers/specs/2026-09-27-ir-mux-simplify-design.local.md`.

use mimz_core::ast::Dir;
use mimz_core::ir::Module;
use mimz_core::ir::exec::Executor;
use mimz_core::ir::opt::{eliminate_dead_cells, fold_constants, simplify_muxes};
use mimz_core::ir::validate::validate;
use mimz_core::value::Val;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

/// Floor on the examples actually checked, so the test cannot pass by
/// skipping everything. Set to the count observed when this was added.
const MIN_CHECKED: usize = 216;

fn mimz_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            mimz_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "mimz") {
            out.push(path);
        }
    }
}

/// The lowered module, or `None` when any pipeline step fails or the result
/// is not `validate`-clean (the optimizer's precondition).
fn lowered(path: &Path) -> Option<Module> {
    let files = mimz::project::load_project(path).ok()?;
    let asts: Vec<mimz_core::ast::File> = files.iter().map(|f| f.ast.clone()).collect();
    mimz_core::checker::check(&asts).ok()?;
    let design = mimz_core::elaborate::elaborate_project(&asts, None, &Default::default()).ok()?;
    let module = catch_unwind(AssertUnwindSafe(|| mimz_core::ir::lower(&design))).ok()?;
    validate(&module).is_empty().then_some(module)
}

/// All three passes, repeated until none changes anything. Returns the
/// number of rounds that changed something.
fn optimize(module: &mut Module) -> usize {
    let mut rounds = 0;
    // `|`, not `||`: every pass runs every round.
    while fold_constants(module) | simplify_muxes(module) | eliminate_dead_cells(module) {
        rounds += 1;
        assert!(rounds < 20, "optimizer did not converge");
    }
    rounds
}

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
    mimz_files(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples"),
        &mut files,
    );
    files.sort();
    let mut checked = 0;
    for path in &files {
        let Some(module) = lowered(path) else {
            continue;
        };
        // `Val::new` takes a u128.
        if module.ports.iter().any(|(_, bits, _)| bits.width() > 128) {
            continue;
        }
        let mut opt = module.clone();
        optimize(&mut opt);

        let errs = validate(&opt);
        assert!(
            errs.is_empty(),
            "{}: optimized module fails validate: {errs:?}",
            path.display()
        );
        assert_eq!(
            trace(&opt),
            trace(&module),
            "{}: outputs changed",
            path.display()
        );
        assert_eq!(
            optimize(&mut opt),
            0,
            "{}: a second run changed something",
            path.display()
        );
        checked += 1;
    }
    eprintln!("checked {checked} of {} examples", files.len());
    assert!(
        checked >= MIN_CHECKED,
        "only {checked} examples checked, expected at least {MIN_CHECKED}"
    );
}
