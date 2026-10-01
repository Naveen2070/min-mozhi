//! `ir::opt::optimize`, the joint fixpoint of all three optimizer passes.
//! See `docs/superpowers/specs/2026-10-01-ir-pipeline-cli-design.local.md`.

use super::lower_valid;
use crate::ir::exec::Executor;
use crate::ir::opt::optimize;
use crate::ir::validate::validate;
use crate::ir::{CellKind, Module};
use crate::value::Val;

/// `m` is driven from a `Const` cell (lower cannot fold `if m`, `m` is a
/// signal), so fold + mux bypass + DCE must together remove the mux and the
/// dead adder.
const FOLDABLE: &str = "module M {\n  in a: bits[8]\n  in b: bits[8]\n  out o: bits[8]\n  wire m: bit = 1 == 1\n  o = if m { a } else { a +% b }\n}\n";

fn count(module: &Module, pred: impl Fn(&CellKind) -> bool) -> usize {
    module.cells.iter().filter(|c| pred(&c.kind)).count()
}

fn out_o(module: &Module, a: u128, b: u128) -> Val {
    let mut ex = Executor::new(module);
    ex.set_input("a", Val::new(a, 8, false));
    ex.set_input("b", Val::new(b, 8, false));
    ex.tick();
    ex.get_output("o")
}

#[test]
fn optimize_runs_all_three_passes_to_a_fixpoint() {
    let before = lower_valid(FOLDABLE);
    assert_eq!(
        count(&before, |k| matches!(k, CellKind::Mux)),
        1,
        "precondition: one mux"
    );

    let mut after = before.clone();
    let rounds = optimize(&mut after);

    assert!(rounds >= 1, "something changed");
    assert!(validate(&after).is_empty(), "{:?}", validate(&after));
    assert_eq!(
        count(&after, |k| matches!(k, CellKind::Mux)),
        0,
        "mux bypassed and removed"
    );
    assert_eq!(
        count(&after, |k| matches!(k, CellKind::AddWrap)),
        0,
        "dead adder removed"
    );
    for (a, b) in [(0x5A, 0x3C), (0xFF, 0x01)] {
        assert_eq!(out_o(&after, a, b), out_o(&before, a, b));
    }
    assert_eq!(optimize(&mut after), 0, "a second call changes nothing");
}
