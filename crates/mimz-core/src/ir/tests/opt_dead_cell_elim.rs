//! `ir::opt::eliminate_dead_cells`. See
//! `docs/superpowers/specs/2026-09-27-ir-dead-cell-elim-design.local.md`.

use super::lower_valid;
use crate::ast::Dir;
use crate::bits::Bits as CBits;
use crate::ir::exec::Executor;
use crate::ir::opt::{driving_cell, eliminate_dead_cells, fold_constants};
use crate::ir::validate::validate;
use crate::ir::{Bits, Cell, CellKind, Edge, Module};
use crate::span::Span;
use crate::value::Val;
use std::collections::BTreeMap;

fn kinds(module: &Module) -> Vec<CellKind> {
    module.cells.iter().map(|c| c.kind.clone()).collect()
}

fn index_of(module: &Module, kind: &CellKind) -> usize {
    module
        .cells
        .iter()
        .position(|c| c.kind == *kind)
        .unwrap_or_else(|| panic!("no {kind:?} cell"))
}

fn run(module: &Module, inputs: &[(&str, u128, u32)], name: &str) -> Val {
    let mut ex = Executor::new(module);
    for &(port, value, width) in inputs {
        ex.set_input(port, Val::new(value, width, false));
    }
    ex.tick();
    ex.get_output(name)
}

fn assert_valid(module: &Module) {
    let errs = validate(module);
    assert!(errs.is_empty(), "module must validate clean, got {errs:?}");
}

const AB: &[(&str, u128, u32)] = &[("a", 0x5A, 8), ("b", 0x3C, 8)];

const UNUSED: &str = "module M {\n  in a: bits[8]\n  in b: bits[8]\n  out o: bits[8]\n  wire k: bits[9] = a + b\n  o = a ^ b\n}\n";

const CHAIN: &str = "module M {\n  in a: bits[8]\n  in b: bits[8]\n  out o: bits[8]\n  wire j: bits[9] = a + b\n  wire k: bits[18] = j * 2\n  o = a ^ b\n}\n";

const REG: &str = "module M {\n  clock clk\n  reset rst\n  in a: bits[8]\n  in b: bits[8]\n  out o: bits[8]\n  wire k: bits[8] = a +% b\n  reg q: bits[8] = 0\n  on rise(clk) { q <- k }\n  o = a ^ b\n}\n";

#[test]
fn driving_cell_maps_out_nets_only() {
    let module = lower_valid(REG);
    let drivers = driving_cell(&module);
    let add = index_of(&module, &CellKind::AddWrap);
    let dff = module
        .cells
        .iter()
        .position(|c| matches!(c.kind, CellKind::Dff { .. }))
        .expect("one Dff for q");
    for net in &module.signals["k"].nets {
        assert_eq!(drivers.get(net), Some(&add));
    }
    for net in &module.cells[dff].pins["q"].nets {
        assert_eq!(drivers.get(net), None, "a Dff drives q, not out");
    }
    for net in &module.signals["a"].nets {
        assert_eq!(drivers.get(net), None, "an input port has no driving cell");
    }
}

#[test]
fn removes_the_cell_of_an_unread_wire() {
    let mut module = lower_valid(UNUSED);
    assert_eq!(
        kinds(&module),
        [CellKind::Xor, CellKind::Add],
        "precondition"
    );
    let before = run(&module, AB, "o");

    assert!(eliminate_dead_cells(&mut module));

    assert_eq!(kinds(&module), [CellKind::Xor]);
    assert_valid(&module);
    assert_eq!(run(&module, AB, "o"), before);
}

#[test]
fn removes_a_whole_unread_chain_in_one_call() {
    let mut module = lower_valid(CHAIN);
    // Consumer first: the sweep must not depend on scan order.
    module.cells.reverse();
    assert_eq!(
        module.cells.len(),
        4,
        "precondition: Xor, Add, Const 2, Mul"
    );
    let before = run(&module, AB, "o");

    assert!(eliminate_dead_cells(&mut module));

    assert_eq!(kinds(&module), [CellKind::Xor]);
    assert_valid(&module);
    assert_eq!(run(&module, AB, "o"), before);
}

#[test]
fn keeps_a_wire_that_feeds_an_output_port() {
    let mut module = lower_valid(
        "module M {\n  in a: bits[8]\n  in b: bits[8]\n  out o: bits[9]\n  wire k: bits[9] = a + b\n  o = k\n}\n",
    );
    let snapshot = format!("{module:?}");

    assert!(!eliminate_dead_cells(&mut module));
    assert_eq!(format!("{module:?}"), snapshot);
}

#[test]
fn keeps_every_input_of_a_register_nobody_reads() {
    let mut module = lower_valid(REG);
    let q = &module.signals["q"];
    assert!(
        module
            .cells
            .iter()
            .flat_map(|c| c.pins.iter())
            .filter(|(name, _)| **name != "q")
            .all(|(_, bits)| bits.nets.iter().all(|n| !q.nets.contains(n))),
        "precondition: nothing reads q"
    );
    let snapshot = format!("{module:?}");

    assert!(!eliminate_dead_cells(&mut module));
    assert_eq!(format!("{module:?}"), snapshot);
}

#[test]
fn a_removal_leaves_no_undriven_net() {
    let mut module = lower_valid(UNUSED);
    let nets_before = module.nets.len();

    assert!(eliminate_dead_cells(&mut module));

    assert_eq!(
        module.nets.len(),
        nets_before - 9,
        "the Add's 9 out nets are gone"
    );
    assert_valid(&module);
}

#[test]
fn drops_the_signals_entry_of_an_unread_named_wire() {
    let mut module = lower_valid(UNUSED);
    assert!(module.signals.contains_key("k"), "precondition");

    eliminate_dead_cells(&mut module);

    assert_eq!(
        module.signals.keys().collect::<Vec<_>>(),
        ["a", "b", "o"],
        "k is dropped, not left pointing at removed nets"
    );
    assert_eq!(module.signals["o"], module.ports[2].1);
}

#[test]
fn keeps_every_out_net_of_a_partly_read_cell() {
    let mut module = lower_valid(
        "module M {\n  in a: bits[8]\n  in b: bits[8]\n  out o: bits[4]\n  wire s: bits[9] = a + b\n  wire k: bits[8] = a ^ b\n  o = s[3:0]\n}\n",
    );
    let before = run(&module, AB, "o");

    assert!(eliminate_dead_cells(&mut module));

    let add = index_of(&module, &CellKind::Add);
    assert_eq!(
        module.cells[add].pins["out"].width(),
        9,
        "unread high bits stay"
    );
    assert_valid(&module);
    assert_eq!(run(&module, AB, "o"), before);
}

#[test]
fn removes_the_inputs_fold_constants_orphans() {
    let mut module =
        lower_valid("module M {\n  wire k: bits[8] = 1\n  out o: bits[9]\n  o = k + 2\n}\n");
    let before = run(&module, &[], "o");
    assert!(fold_constants(&mut module));
    assert_eq!(
        module.cells.len(),
        3,
        "precondition: two orphaned Consts plus the folded Add"
    );

    assert!(eliminate_dead_cells(&mut module));

    assert_eq!(module.cells.len(), 1);
    assert!(matches!(module.cells[0].kind, CellKind::Const { .. }));
    assert_valid(&module);
    assert_eq!(run(&module, &[], "o"), before);
}

#[test]
fn keeps_a_cell_that_only_feeds_a_mem_read_address() {
    let mut module = lower_valid(
        "module M {\n  clock clk\n  in i: bits[2]\n  in d: bits[8]\n  out o: bits[8]\n  mem m: bits[8][4] = 0\n  on rise(clk) { m[0] <- d }\n  o = m[i +% 1]\n}\n",
    );
    let inputs: &[(&str, u128, u32)] = &[("i", 3, 2), ("d", 0x42, 8)];
    let before = run(&module, inputs, "o");
    assert_eq!(
        before.bits,
        CBits::Small(0x42),
        "precondition: reads m[0] after the write"
    );

    assert!(
        eliminate_dead_cells(&mut module),
        "lower leaves unread Consts behind"
    );

    index_of(&module, &CellKind::AddWrap);
    assert_valid(&module);
    assert_eq!(run(&module, inputs, "o"), before);
}

fn empty_module() -> Module {
    Module {
        name: "M".to_string(),
        ports: vec![],
        cells: vec![],
        nets: vec![],
        extern_decls: BTreeMap::new(),
        signals: BTreeMap::new(),
        port_declared_widths: BTreeMap::new(),
    }
}

fn port(module: &mut Module, name: &str, dir: Dir) -> Bits {
    let bits = module.alloc_bits(1, Some(name));
    module.ports.push((name.to_string(), bits.clone(), dir));
    bits
}

fn not_cell(module: &mut Module, a: Bits) -> Bits {
    let out = module.alloc_bits(1, None);
    module.cells.push(Cell {
        kind: CellKind::Not,
        pins: BTreeMap::from([("a", a), ("out", out.clone())]),
        span: Span::default(),
    });
    out
}

#[test]
fn keeps_the_driver_of_a_register_clock() {
    // Hand-built: the checker only accepts a declared `clock` input as a
    // register clock, so no source program derives one from a cell.
    let mut module = empty_module();
    let c = port(&mut module, "c", Dir::In);
    let x = port(&mut module, "x", Dir::In);
    let clock = not_cell(&mut module, c);
    let q = module.alloc_bits(1, None);
    module.ports.push(("o".to_string(), q.clone(), Dir::Out));
    module.cells.push(Cell {
        kind: CellKind::Dff {
            clock: clock.nets[0],
            edge: Edge::Rise,
        },
        pins: BTreeMap::from([("d", x), ("q", q)]),
        span: Span::default(),
    });
    assert_valid(&module);

    assert!(!eliminate_dead_cells(&mut module));
    index_of(&module, &CellKind::Not);
}

#[test]
fn keeps_the_driver_of_a_blackbox_pin() {
    let mut module = empty_module();
    let a = port(&mut module, "a", Dir::In);
    let x = not_cell(&mut module, a);
    module.cells.push(Cell {
        kind: CellKind::BlackBox {
            module_name: "Sink".to_string(),
            verilog_name: "Sink".to_string(),
            aliased: false,
            params: vec![],
        },
        pins: BTreeMap::from([("x", x)]),
        span: Span::default(),
    });
    assert_valid(&module);

    assert!(!eliminate_dead_cells(&mut module));
    index_of(&module, &CellKind::Not);
}

#[test]
fn a_second_call_reports_no_change() {
    let mut module = lower_valid(CHAIN);
    assert!(eliminate_dead_cells(&mut module));
    let snapshot = format!("{module:?}");

    assert!(!eliminate_dead_cells(&mut module));
    assert_eq!(format!("{module:?}"), snapshot);
}
