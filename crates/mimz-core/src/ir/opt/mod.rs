//! IR optimizer passes over a lowered `ir::Module`. See
//! `docs/superpowers/specs/2026-09-24-ir-const-fold-design.local.md`.

mod const_fold;

pub use const_fold::fold_constants;

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
