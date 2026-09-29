//! Mux-tree simplification: bypasses a `Mux` whose select is constant (R1)
//! or whose two data bits agree (R2) by pointing its readers at the source,
//! and skips an inner mux on the same select (R3). The bypassed mux is left
//! for `eliminate_dead_cells`.

use super::{driving_cell, for_each_read_net_mut, net_consts, run_to_fixpoint};
use crate::ir::{CellKind, Module, NetId};
use std::collections::HashMap;

/// Bypasses every redundant mux. Returns whether any reference changed.
pub fn simplify_muxes(module: &mut Module) -> bool {
    run_to_fixpoint(module, simplify_once)
}

fn simplify_once(module: &mut Module) -> bool {
    let consts = net_consts(module);
    let drivers = driving_cell(module);
    // Each bypassed `out` net, mapped to the net its readers should read.
    let mut subst: HashMap<NetId, NetId> = HashMap::new();
    // R3 pin edits: (mux index, pin, bit, new net).
    let mut edits: Vec<(usize, &'static str, usize, NetId)> = Vec::new();
    for (m, cell) in module.cells.iter().enumerate() {
        if cell.kind != CellKind::Mux || cell.pins["sel"].width() != 1 {
            continue;
        }
        let s = cell.pins["sel"].nets[0];
        let (a, b, out) = (
            &cell.pins["a"].nets,
            &cell.pins["b"].nets,
            &cell.pins["out"].nets,
        );
        match consts.get(&s) {
            Some(&sel) => {
                let picked = if sel { a } else { b };
                subst.extend(out.iter().copied().zip(picked.iter().copied()));
            }
            None => {
                for i in 0..out.len() {
                    let same = a[i] == b[i]
                        || matches!(
                            (consts.get(&a[i]), consts.get(&b[i])),
                            (Some(x), Some(y)) if x == y
                        );
                    if same {
                        subst.insert(out[i], a[i]);
                    }
                }
            }
        }
        // R3: `mux(s, mux(s, x, y), z)` reads `x` on its `a` side whatever
        // `s` is; the mirror holds for `b`. Only this mux's own pin moves:
        // the inner mux may have other readers.
        for pin in ["a", "b"] {
            for (i, net) in cell.pins[pin].nets.iter().enumerate() {
                let Some(&k) = drivers.get(net) else { continue };
                let inner = &module.cells[k];
                if k == m || inner.kind != CellKind::Mux || inner.pins["sel"].nets != [s] {
                    continue;
                }
                let j = inner.pins["out"]
                    .nets
                    .iter()
                    .position(|n| n == net)
                    .expect("the driver's out holds the net");
                edits.push((m, pin, i, inner.pins[pin].nets[j]));
            }
        }
    }

    // `validate` sizes a `Shl` exactly once its `b` pin is one Const cell's
    // `out`, but `lower` sized it worst-case, so a bypass there would make a
    // valid module fail `validate` (the same guard as `const_fold`'s
    // `feeds_a_shift_amount`).
    // ponytail: this also keeps the mux for its other readers; bypass them
    // and leave only `Shl.b` if that ever matters.
    for cell in module.cells.iter().filter(|c| c.kind == CellKind::Shl) {
        for net in &cell.pins["b"].nets {
            subst.remove(net);
        }
    }

    // Edits go first: an edited pin may point at a net R1/R2 also bypassed
    // this round, and the rewrite below then follows it.
    let mut changed = !edits.is_empty();
    for (m, pin, i, net) in edits {
        module.cells[m].pins.get_mut(pin).expect("mux pin").nets[i] = net;
    }
    // One hop per round: when the replacement is itself a bypassed `out`,
    // `run_to_fixpoint`'s next round takes the next hop.
    for_each_read_net_mut(module, |net| {
        if let Some(&to) = subst.get(net) {
            *net = to;
            changed = true;
        }
    });
    changed
}
