//! IR optimizer passes over a lowered `ir::Module`, and [`optimize`], which
//! runs them together. See
//! `docs/superpowers/specs/2026-09-24-ir-const-fold-design.local.md`,
//! `docs/superpowers/specs/2026-09-27-ir-dead-cell-elim-design.local.md`,
//! `docs/superpowers/specs/2026-09-27-ir-mux-simplify-design.local.md` and
//! `docs/superpowers/specs/2026-10-01-ir-pipeline-cli-design.local.md`.

mod const_fold;
mod dead_cell_elim;
mod mux_simplify;

pub use const_fold::fold_constants;
pub use dead_cell_elim::eliminate_dead_cells;
pub use mux_simplify::simplify_muxes;

/// Rounds after which [`optimize`] gives up. The passes only shrink or
/// rewire the module, so needing this many means two passes keep undoing
/// each other.
pub const MAX_ROUNDS: usize = 32;

/// Runs `fold_constants`, `simplify_muxes` and `eliminate_dead_cells`, every
/// pass every round, until none changes anything. Returns the number of
/// rounds that changed something. Precondition: `module` is
/// `validate`-clean. Panics past [`MAX_ROUNDS`] (an optimizer bug).
pub fn optimize(module: &mut Module) -> usize {
    let mut rounds = 0;
    // `|`, not `||`: every pass runs every round.
    while fold_constants(module) | simplify_muxes(module) | eliminate_dead_cells(module) {
        rounds += 1;
        assert!(
            rounds <= MAX_ROUNDS,
            "optimizer did not converge in {MAX_ROUNDS} rounds"
        );
    }
    rounds
}

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
        // A black box's pins carry the extern's own port names, so a pin
        // called `out` is not necessarily an output.
        if matches!(cell.kind, CellKind::BlackBox { .. }) {
            continue;
        }
        if let Some(out) = cell.pins.get("out") {
            for &net in &out.nets {
                drivers.insert(net, i);
            }
        }
    }
    drivers
}

/// Calls `f` on every net reference that reads a value: every pin except
/// the driver pins `validate` counts (`out`/`q`/`rdata`), every port,
/// `Dff::clock`, every `Mem` read address, and every `Module::signals`
/// entry. `Mem` read ports' `rdata` are drivers and are skipped.
pub(crate) fn for_each_read_net_mut(module: &mut Module, mut f: impl FnMut(&mut NetId)) {
    for (_, bits, _) in &mut module.ports {
        bits.nets.iter_mut().for_each(&mut f);
    }
    for cell in &mut module.cells {
        for (name, bits) in cell.pins.iter_mut() {
            if !matches!(*name, "out" | "q" | "rdata") {
                bits.nets.iter_mut().for_each(&mut f);
            }
        }
        match &mut cell.kind {
            CellKind::Dff { clock, .. } => f(clock),
            CellKind::Mem { read_ports, .. } => {
                for (raddr, _) in read_ports {
                    raddr.nets.iter_mut().for_each(&mut f);
                }
            }
            _ => {}
        }
    }
    for bits in module.signals.values_mut() {
        bits.nets.iter_mut().for_each(&mut f);
    }
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
