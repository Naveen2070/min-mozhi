//! IR optimizer passes over a lowered `ir::Module`. See
//! `docs/superpowers/specs/2026-09-24-ir-const-fold-design.local.md` and
//! `docs/superpowers/specs/2026-09-27-ir-dead-cell-elim-design.local.md`.

mod const_fold;
mod dead_cell_elim;

pub use const_fold::fold_constants;
pub use dead_cell_elim::eliminate_dead_cells;

use super::{CellKind, Module, NetId};
use std::collections::HashMap;

/// Every net driven by a `CellKind::Const` cell, mapped to its bit value.
pub(crate) fn net_consts(module: &Module) -> HashMap<NetId, bool> {
    let mut consts = HashMap::new();
    for cell in &module.cells {
        let CellKind::Const { value } = &cell.kind else {
            continue;
        };
        let out = &cell.pins["out"];
        // The same extension `exec` applies to a Const cell, so every bit here
        // matches what executing the module drives onto that net.
        let limbs = crate::value::from_const_at_width(value, out.width(), false).to_limbs();
        for (i, &net) in out.nets.iter().enumerate() {
            consts.insert(net, crate::wide::bit_at(&limbs, i as u32));
        }
    }
    consts
}

/// Every net some cell's `out` pin drives, mapped to that cell's index.
/// Unambiguous because `validate` rejects a net with two drivers.
pub(crate) fn driving_cell(module: &Module) -> HashMap<NetId, usize> {
    let mut drivers = HashMap::new();
    for (i, cell) in module.cells.iter().enumerate() {
        if let Some(out) = cell.pins.get("out") {
            for &net in &out.nets {
                drivers.insert(net, i);
            }
        }
    }
    drivers
}

/// Runs `pass` until it reports no change. Returns whether any run did.
pub(crate) fn run_to_fixpoint(
    module: &mut Module,
    mut pass: impl FnMut(&mut Module) -> bool,
) -> bool {
    let mut changed = false;
    while pass(module) {
        changed = true;
    }
    changed
}
