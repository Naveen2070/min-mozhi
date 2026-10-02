//! Dead signal & dead cell elimination: drops every combinational cell that
//! no output port and no stateful cell reads, directly or transitively.

use super::driving_cell;
use crate::ast::Dir;
use crate::ir::{CellKind, Module, NetId};

/// Removes every dead combinational cell, then every net nothing mentions
/// any more. Returns whether anything was removed.
pub fn eliminate_dead_cells(module: &mut Module) -> bool {
    let live = live_cells(module);
    let before = module.cells.len();
    let cells = std::mem::take(&mut module.cells);
    module.cells = cells
        .into_iter()
        .enumerate()
        .filter(|(i, cell)| live[*i] || is_stateful(&cell.kind))
        .map(|(_, cell)| cell)
        .collect();
    let removed = module.cells.len() < before;
    if removed {
        compact_nets(module);
    }
    removed
}

fn is_stateful(kind: &CellKind) -> bool {
    matches!(
        kind,
        CellKind::Dff { .. }
            | CellKind::Adff { .. }
            | CellKind::Mem { .. }
            | CellKind::BlackBox { .. }
    )
}

fn live_cells(module: &Module) -> Vec<bool> {
    let drivers = driving_cell(module);
    let mut live = vec![false; module.cells.len()];
    let mut work = roots(module);
    while let Some(net) = work.pop() {
        let Some(&i) = drivers.get(&net) else {
            continue;
        };
        if live[i] {
            continue;
        }
        live[i] = true;
        work.extend(
            module.cells[i]
                .pins
                .iter()
                .filter(|(name, _)| **name != "out")
                .flat_map(|(_, bits)| bits.nets.iter().copied()),
        );
    }
    live
}

/// Output-port nets plus every net a stateful cell reads.
fn roots(module: &Module) -> Vec<NetId> {
    let mut nets: Vec<NetId> = module
        .ports
        .iter()
        .filter(|(_, _, dir)| *dir == Dir::Out)
        .flat_map(|(_, bits, _)| bits.nets.iter().copied())
        .collect();
    for cell in module.cells.iter().filter(|c| is_stateful(&c.kind)) {
        nets.extend(
            cell.pins
                .values()
                .flat_map(|bits| bits.nets.iter().copied()),
        );
        // Inputs carried in the kind itself rather than in `pins`.
        match &cell.kind {
            CellKind::Dff { clock, .. } | CellKind::Adff { clock, .. } => nets.push(*clock),
            CellKind::Mem { read_ports, .. } => nets.extend(
                read_ports
                    .iter()
                    .flat_map(|(raddr, _)| raddr.nets.iter().copied()),
            ),
            _ => {}
        }
    }
    nets
}

/// Drops every net no port or cell still mentions and renumbers the rest in
/// order. `validate` rejects any allocated net without a driver, so a removed
/// cell's `out` nets cannot stay behind in `nets`.
fn compact_nets(module: &mut Module) {
    let mut used = vec![false; module.nets.len()];
    for (_, bits, _) in &module.ports {
        mark(&mut used, &bits.nets);
    }
    for cell in &module.cells {
        for bits in cell.pins.values() {
            mark(&mut used, &bits.nets);
        }
        match &cell.kind {
            CellKind::Dff { clock, .. } | CellKind::Adff { clock, .. } => {
                mark(&mut used, std::slice::from_ref(clock))
            }
            CellKind::Mem { read_ports, .. } => {
                for (raddr, rdata) in read_ports {
                    mark(&mut used, &raddr.nets);
                    mark(&mut used, &rdata.nets);
                }
            }
            _ => {}
        }
    }

    let mut remap = vec![None; used.len()];
    for (i, info) in std::mem::take(&mut module.nets).into_iter().enumerate() {
        if used[i] {
            remap[i] = Some(NetId(module.nets.len() as u32));
            module.nets.push(info);
        }
    }

    for (_, bits, _) in &mut module.ports {
        renumber(&mut bits.nets, &remap);
    }
    for cell in &mut module.cells {
        for bits in cell.pins.values_mut() {
            renumber(&mut bits.nets, &remap);
        }
        match &mut cell.kind {
            CellKind::Dff { clock, .. } | CellKind::Adff { clock, .. } => {
                renumber(std::slice::from_mut(clock), &remap)
            }
            CellKind::Mem { read_ports, .. } => {
                for (raddr, rdata) in read_ports {
                    renumber(&mut raddr.nets, &remap);
                    renumber(&mut rdata.nets, &remap);
                }
            }
            _ => {}
        }
    }
    module
        .signals
        .retain(|_, bits| bits.nets.iter().all(|n| used[n.0 as usize]));
    for bits in module.signals.values_mut() {
        renumber(&mut bits.nets, &remap);
    }
}

fn mark(used: &mut [bool], nets: &[NetId]) {
    for n in nets {
        used[n.0 as usize] = true;
    }
}

fn renumber(nets: &mut [NetId], remap: &[Option<NetId>]) {
    for n in nets {
        *n = remap[n.0 as usize].expect("every net a survivor mentions was kept");
    }
}
