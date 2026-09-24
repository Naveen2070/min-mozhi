//! Constant folding/propagation: a pure-combinational cell whose every input
//! net is Const-driven becomes a `Const` cell with the same `out` pin.

use super::net_consts;
use crate::checker::consteval::ConstVal;
use crate::ir::exec::Executor;
use crate::ir::{Cell, CellKind, Module, NetId};
use std::collections::{BTreeMap, HashMap};

/// Folds every all-constant-input combinational cell. Returns whether
/// anything changed.
pub fn fold_constants(module: &mut Module) -> bool {
    fold_once(module)
}

fn fold_once(module: &mut Module) -> bool {
    let consts = net_consts(module);
    let foldable: Vec<usize> = (0..module.cells.len())
        .filter(|&i| is_candidate(module, &module.cells[i], &consts))
        .collect();
    for &i in &foldable {
        let value = evaluate(&module.cells[i], &consts);
        let cell = &mut module.cells[i];
        let out = cell.pins["out"].clone();
        cell.kind = CellKind::Const { value };
        cell.pins = BTreeMap::from([("out", out)]);
    }
    !foldable.is_empty()
}

fn is_candidate(_module: &Module, cell: &Cell, consts: &HashMap<NetId, bool>) -> bool {
    is_pure_comb(&cell.kind)
        && cell
            .pins
            .iter()
            .filter(|(name, _)| **name != "out")
            .all(|(_, bits)| bits.nets.iter().all(|n| consts.contains_key(n)))
}

fn is_pure_comb(kind: &CellKind) -> bool {
    matches!(
        kind,
        CellKind::Add
            | CellKind::Sub
            | CellKind::Mul
            | CellKind::AddWrap
            | CellKind::SubWrap
            | CellKind::MulWrap
            | CellKind::Shl
            | CellKind::Shr
            | CellKind::And
            | CellKind::Or
            | CellKind::Xor
            | CellKind::Not
            | CellKind::RedAnd
            | CellKind::RedOr
            | CellKind::RedXor
            | CellKind::Neg
            | CellKind::Eq
            | CellKind::Ne
            | CellKind::Lt { .. }
            | CellKind::Le { .. }
            | CellKind::Gt { .. }
            | CellKind::Ge { .. }
            | CellKind::LogicAnd
            | CellKind::LogicOr
            | CellKind::LogicNot
            | CellKind::Concat
            | CellKind::Slice { .. }
    )
}

/// Runs `cell` alone in a throwaway module, so the fold uses `exec`'s
/// semantics rather than a second evaluator that could drift from them.
fn evaluate(cell: &Cell, consts: &HashMap<NetId, bool>) -> ConstVal {
    let mut scratch = Module {
        name: String::new(),
        ports: Vec::new(),
        cells: Vec::new(),
        nets: Vec::new(),
        extern_decls: BTreeMap::new(),
        signals: BTreeMap::new(),
        port_declared_widths: BTreeMap::new(),
    };
    let mut probe = cell.clone();
    for (name, bits) in probe.pins.iter_mut() {
        let mut fresh = scratch.alloc_bits(bits.width(), None);
        fresh.signed = bits.signed;
        if *name != "out" {
            let raw = bits
                .nets
                .iter()
                .enumerate()
                .fold(0u128, |acc, (i, n)| acc | (u128::from(consts[n]) << i));
            scratch.cells.push(Cell {
                kind: CellKind::Const {
                    value: ConstVal {
                        bits: crate::bits::Bits::Small(raw),
                        width: bits.width(),
                        signed: false,
                    },
                },
                pins: BTreeMap::from([("out", fresh.clone())]),
                span: cell.span,
            });
        }
        *bits = fresh;
    }
    scratch
        .signals
        .insert("out".to_string(), probe.pins["out"].clone());
    scratch.cells.push(probe);
    let mut ex = Executor::new(&scratch);
    ex.tick();
    let v = ex.get_output("out");
    // Exactly `out`'s width rather than ConstVal's usual minimal width, and
    // unsigned: the raw pattern then needs no extension when `exec` drives
    // it, and the pin's own `Bits::signed` still carries the interpretation.
    ConstVal {
        bits: v.bits_masked(),
        width: v.width,
        signed: false,
    }
}
