//! `ir::opt::fold_constants`. See
//! `docs/superpowers/specs/2026-09-24-ir-const-fold-design.local.md`.

use super::lower_valid;
use crate::ast::{Expr, ExprKind, Ident, LValue, SeqStmt};
use crate::bits::Bits as CBits;
use crate::checker::consteval::ConstVal;
use crate::ir::exec::Executor;
use crate::ir::opt::{fold_constants, net_consts};
use crate::ir::validate::validate;
use crate::ir::{Bits, Cell, CellKind, Module};
use crate::span::Span;
use crate::value::Val;
use std::collections::BTreeMap;

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

const CHAIN: &str = "module M {\n  wire a: bits[8] = 1\n  wire b: bits[9] = a + 2\n  out o: bits[18]\n  o = b * 3\n}\n";

#[test]
fn folds_a_multi_hop_chain_regardless_of_cell_order() {
    let mut module = lower_valid(CHAIN);
    // Consumer before producer: a single scan cannot see the Add's fold in
    // time for the Mul, so only the fixpoint loop folds both.
    module.cells.reverse();
    let add = only_index(&module, &CellKind::Add);
    let mul = only_index(&module, &CellKind::Mul);
    assert!(mul < add, "precondition: Mul scans before the Add it reads");
    let before = read(&module, "o");

    assert!(fold_constants(&mut module));

    assert_eq!(*const_at(&module, add), cv(3, 9));
    assert_eq!(*const_at(&module, mul), cv(9, 18));
    assert_valid(&module);
    assert_eq!(read(&module, "o"), before);
}

#[test]
fn a_second_call_on_a_folded_module_reports_no_change() {
    let mut module = lower_valid(CHAIN);
    assert!(fold_constants(&mut module));
    let snapshot = format!("{:?}", module.cells);

    assert!(!fold_constants(&mut module));
    assert_eq!(format!("{:?}", module.cells), snapshot);
}

#[test]
fn folds_a_signed_add_with_its_operands_signedness() {
    let mut module =
        lower_valid("module M {\n  wire k: signed[8] = -3\n  out o: signed[9]\n  o = k + 1\n}\n");
    let add = only_index(&module, &CellKind::Add);
    let pins = &module.cells[add].pins;
    assert!(
        pins["a"].signed || pins["b"].signed,
        "precondition: lower marks the operands signed"
    );
    let before = read(&module, "o");
    assert_eq!(
        before.bits,
        CBits::Small(0x1FE),
        "precondition: -3 + 1 = -2 at 9 bits"
    );

    assert!(fold_constants(&mut module));

    assert_eq!(
        *const_at(&module, add),
        cv(0x1FE, 9),
        "an unsigned re-read gives 0x0FE"
    );
    assert_valid(&module);
    assert_eq!(read(&module, "o"), before);
}

fn const_cell(module: &mut Module, value: u128, width: u32) -> Bits {
    let out = module.alloc_bits(width, None);
    module.cells.push(Cell {
        kind: CellKind::Const {
            value: cv(value, width),
        },
        pins: BTreeMap::from([("out", out.clone())]),
        span: Span::default(),
    });
    out
}

#[test]
fn folds_concat_and_slice_in_execs_bit_order() {
    // Hand-built: `lower` never emits Concat/Slice cells (both are pure
    // `Bits` re-pointing at lowering time), so no source program reaches them.
    let mut module = Module {
        name: "M".to_string(),
        ports: vec![],
        cells: vec![],
        nets: vec![],
        extern_decls: BTreeMap::new(),
        signals: BTreeMap::new(),
        port_declared_widths: BTreeMap::new(),
    };
    let lo = const_cell(&mut module, 0x5, 4);
    let hi = const_cell(&mut module, 0x3, 4);
    let cat = module.alloc_bits(8, None);
    module.cells.push(Cell {
        kind: CellKind::Concat,
        pins: BTreeMap::from([("a", lo), ("b", hi), ("out", cat.clone())]),
        span: Span::default(),
    });
    let mid = module.alloc_bits(4, None);
    module.cells.push(Cell {
        kind: CellKind::Slice { lo: 2, hi: 5 },
        pins: BTreeMap::from([("a", cat), ("out", mid)]),
        span: Span::default(),
    });
    assert_valid(&module);

    assert!(fold_constants(&mut module));

    assert_eq!(
        *const_at(&module, 2),
        cv(0x35, 8),
        "Concat joins pins in key order, LSB pin first"
    );
    assert_eq!(
        *const_at(&module, 3),
        cv(0xD, 4),
        "bits 2..=5 of 0b0011_0101 are 0b1101"
    );
    assert_valid(&module);
}

#[test]
fn never_folds_a_dff_even_with_a_constant_d() {
    let mut design = super::lower_regs::reg_design();
    design.procs[0].body = vec![SeqStmt::Assign {
        lhs: LValue {
            base: Ident {
                name: "q".to_string(),
                span: Span::default(),
            },
            index: None,
            span: Span::default(),
        },
        rhs: Expr {
            kind: ExprKind::Int {
                value: CBits::Small(5),
                raw: "5".to_string(),
            },
            span: Span::default(),
        },
    }];
    let mut module = crate::ir::lower(&design);
    assert_valid(&module);
    let dff = module
        .cells
        .iter()
        .position(|c| matches!(c.kind, CellKind::Dff { .. }))
        .expect("one Dff for q");
    let consts = net_consts(&module);
    assert!(
        module.cells[dff].pins["d"]
            .nets
            .iter()
            .all(|n| consts.contains_key(n)),
        "precondition: d is entirely Const-driven"
    );
    let snapshot = format!("{:?}", module.cells[dff]);

    fold_constants(&mut module);

    assert_eq!(format!("{:?}", module.cells[dff]), snapshot);
}

#[test]
fn never_folds_a_mem_even_with_constant_write_pins() {
    let mut module = lower_valid(
        "module M {\n  clock clk\n  out o: bits[8]\n  mem m: bits[8][4] = 0\n  on rise(clk) { m[1] <- 5 }\n  o = m[1]\n}\n",
    );
    let mem = module
        .cells
        .iter()
        .position(|c| matches!(c.kind, CellKind::Mem { .. }))
        .expect("one Mem for m");
    let consts = net_consts(&module);
    for pin in ["waddr", "wdata"] {
        assert!(
            module.cells[mem].pins[pin]
                .nets
                .iter()
                .all(|n| consts.contains_key(n)),
            "precondition: {pin} is entirely Const-driven"
        );
    }
    let snapshot = format!("{:?}", module.cells[mem]);

    fold_constants(&mut module);

    assert_eq!(format!("{:?}", module.cells[mem]), snapshot);
}

#[test]
fn skips_a_candidate_too_wide_for_the_executor() {
    let mut module =
        lower_valid("module M {\n  wire k: bits[200] = 1\n  out o: bits[201]\n  o = k + 1\n}\n");
    let add = only_index(&module, &CellKind::Add);

    assert!(!fold_constants(&mut module));
    assert_eq!(module.cells[add].kind, CellKind::Add);
}

#[test]
fn does_not_fold_a_shift_amount_that_lower_sized_as_runtime() {
    let mut module = lower_valid(
        "module M {\n  in x: bits[4]\n  wire k: bits[2] = 1\n  out o: bits[7]\n  o = x << (k +% 1)\n}\n",
    );
    let amount = only_index(&module, &CellKind::AddWrap);
    let shl = only_index(&module, &CellKind::Shl);
    assert_eq!(
        module.cells[shl].pins["b"], module.cells[amount].pins["out"],
        "precondition: the AddWrap drives the Shl's amount pin exactly"
    );

    fold_constants(&mut module);

    assert_valid(&module);
    assert_eq!(module.cells[amount].kind, CellKind::AddWrap);
}
