//! `ir::opt::fold_constants`. See
//! `docs/superpowers/specs/2026-09-24-ir-const-fold-design.local.md`.

use super::lower_valid;
use crate::bits::Bits as CBits;
use crate::checker::consteval::ConstVal;
use crate::ir::exec::Executor;
use crate::ir::opt::{fold_constants, net_consts};
use crate::ir::validate::validate;
use crate::ir::{CellKind, Module};
use crate::value::Val;

fn only_index(module: &Module, kind: &CellKind) -> usize {
    let hits: Vec<usize> = module
        .cells
        .iter()
        .enumerate()
        .filter(|(_, c)| c.kind == *kind)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one {kind:?} cell, got {hits:?}"
    );
    hits[0]
}

fn const_at(module: &Module, i: usize) -> &ConstVal {
    match &module.cells[i].kind {
        CellKind::Const { value } => value,
        other => panic!("cell {i} should have folded to Const, is {other:?}"),
    }
}

fn read(module: &Module, name: &str) -> Val {
    let mut ex = Executor::new(module);
    ex.tick();
    ex.get_output(name)
}

fn assert_valid(module: &Module) {
    let errs = validate(module);
    assert!(errs.is_empty(), "module must validate clean, got {errs:?}");
}

fn cv(bits: u128, width: u32) -> ConstVal {
    ConstVal {
        bits: CBits::Small(bits),
        width,
        signed: false,
    }
}

#[test]
fn net_consts_maps_every_const_driven_net_to_its_bit() {
    let module =
        lower_valid("module M {\n  wire k: bits[4] = 5\n  out o: bits[5]\n  o = k + 2\n}\n");
    let consts = net_consts(&module);
    let k: Vec<bool> = module.signals["k"].nets.iter().map(|n| consts[n]).collect();
    assert_eq!(k, [true, false, true, false], "5 = 0b0101, LSB first");
    assert!(
        !consts.contains_key(&module.signals["o"].nets[0]),
        "an Add-driven net is not a constant"
    );
}

#[test]
fn folds_an_add_whose_inputs_are_both_constant() {
    let mut module =
        lower_valid("module M {\n  wire k: bits[8] = 1\n  out o: bits[9]\n  o = k + 2\n}\n");
    let add = only_index(&module, &CellKind::Add);
    let out_before = module.cells[add].pins["out"].clone();
    let before = read(&module, "o");

    assert!(fold_constants(&mut module));

    assert_eq!(*const_at(&module, add), cv(3, 9));
    assert_eq!(
        module.cells[add].pins.keys().copied().collect::<Vec<_>>(),
        ["out"]
    );
    assert_eq!(
        module.cells[add].pins["out"], out_before,
        "out nets unchanged, no rewiring"
    );
    assert_valid(&module);
    assert_eq!(read(&module, "o"), before);
}

#[test]
fn leaves_a_cell_with_one_non_constant_input_alone() {
    let mut module = lower_valid(
        "module M {\n  in x: bits[8]\n  wire k: bits[8] = 1\n  out o: bits[9]\n  o = k + x\n}\n",
    );
    let add = only_index(&module, &CellKind::Add);

    assert!(!fold_constants(&mut module));
    assert_eq!(module.cells[add].kind, CellKind::Add);
}
