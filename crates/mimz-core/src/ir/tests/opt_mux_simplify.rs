//! `ir::opt::simplify_muxes` and its `ir/opt/mod.rs` helpers. See
//! `docs/superpowers/specs/2026-09-27-ir-mux-simplify-design.local.md`.

use super::lower_valid;
use crate::bits::Bits as CBits;
use crate::ir::exec::Executor;
use crate::ir::opt::{
    driving_cell, eliminate_dead_cells, fold_constants, for_each_read_net_mut, simplify_muxes,
};
use crate::ir::validate::validate;
use crate::ir::{Bits, Cell, CellKind, Module, NetId, parse_line};
use crate::span::Span;
use crate::value::Val;
use std::collections::BTreeMap;

const REG_MEM: &str = "module M {\n  clock clk\n  reset rst\n  in i: bits[2]\n  in d: bits[8]\n  out o: bits[8]\n  mem m: bits[8][4] = 0\n  reg q: bits[8] = 0\n  on rise(clk) {\n    m[0] <- d\n    q <- d\n  }\n  o = m[i +% 1] ^ q\n}\n";

#[test]
fn for_each_read_net_mut_rewrites_readers_and_skips_drivers() {
    let mut module = lower_valid(REG_MEM);
    assert!(
        module
            .cells
            .iter()
            .any(|c| matches!(c.kind, CellKind::Dff { .. })),
        "precondition: a Dff"
    );
    assert!(
        module
            .cells
            .iter()
            .any(|c| matches!(&c.kind, CellKind::Mem { read_ports, .. } if !read_ports.is_empty())),
        "precondition: a Mem with a read port"
    );
    let before = module.clone();
    let sentinel = NetId(u32::MAX);

    for_each_read_net_mut(&mut module, |n| *n = sentinel);

    for (old, new) in before.cells.iter().zip(&module.cells) {
        for (name, bits) in &new.pins {
            let is_driver = matches!(*name, "out" | "q" | "rdata");
            assert_eq!(
                bits == &old.pins[*name],
                is_driver,
                "pin `{name}` of {:?}: drivers untouched, readers rewritten",
                new.kind
            );
        }
        match (&old.kind, &new.kind) {
            (CellKind::Dff { .. }, CellKind::Dff { clock, .. }) => {
                assert_eq!(*clock, sentinel, "Dff clock is a reader")
            }
            (
                CellKind::Mem {
                    read_ports: old_ports,
                    ..
                },
                CellKind::Mem { read_ports, .. },
            ) => {
                for ((_, old_rdata), (raddr, rdata)) in old_ports.iter().zip(read_ports) {
                    assert!(
                        raddr.nets.iter().all(|&n| n == sentinel),
                        "raddr is a reader"
                    );
                    assert_eq!(rdata, old_rdata, "rdata is a driver");
                }
            }
            _ => {}
        }
    }
    assert!(
        module
            .ports
            .iter()
            .all(|(_, bits, _)| bits.nets.iter().all(|&n| n == sentinel)),
        "every port is rewritten"
    );
    assert!(
        module
            .signals
            .values()
            .all(|bits| bits.nets.iter().all(|&n| n == sentinel)),
        "every signals entry is rewritten"
    );
}

#[test]
fn driving_cell_skips_a_blackbox_pin_named_out() {
    let mut module = Module {
        name: "M".to_string(),
        ports: vec![],
        cells: vec![],
        nets: vec![],
        extern_decls: BTreeMap::new(),
        signals: BTreeMap::new(),
        port_declared_widths: BTreeMap::new(),
    };
    let pin = module.alloc_bits(1, None);
    module.cells.push(Cell {
        kind: CellKind::BlackBox {
            module_name: "Sink".to_string(),
        },
        pins: BTreeMap::from([("out", pin)]),
        span: Span::default(),
    });

    assert!(
        driving_cell(&module).is_empty(),
        "a black box's `out` is an extern port name, not necessarily an output"
    );
}

type Inputs<'a> = &'a [(&'a str, u128, u32)];

fn run(module: &Module, inputs: Inputs, name: &str) -> Val {
    let mut ex = Executor::new(module);
    for &(port, value, width) in inputs {
        ex.set_input(port, Val::new(value, width, false));
    }
    ex.tick();
    ex.get_output(name)
}

/// One `Executor` across several ticks, reading `name` after each.
fn trace(module: &Module, steps: &[Inputs], name: &str) -> Vec<Val> {
    let mut ex = Executor::new(module);
    steps
        .iter()
        .map(|step| {
            for &(port, value, width) in *step {
                ex.set_input(port, Val::new(value, width, false));
            }
            ex.tick();
            ex.get_output(name)
        })
        .collect()
}

fn assert_valid(module: &Module) {
    let errs = validate(module);
    assert!(errs.is_empty(), "module must validate clean, got {errs:?}");
}

fn parse(text: &str) -> Module {
    let module = parse_line::parse(text).expect("parses");
    assert_valid(&module);
    module
}

fn port_bits(module: &Module, name: &str) -> Bits {
    module
        .ports
        .iter()
        .find(|(n, ..)| n == name)
        .unwrap_or_else(|| panic!("no port {name}"))
        .1
        .clone()
}

fn mux_count(module: &Module) -> usize {
    module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Mux)
        .count()
}

/// How many reader references `net` has (see `for_each_read_net_mut`).
fn read_count(module: &Module, net: NetId) -> usize {
    let mut copy = module.clone();
    let mut n = 0;
    for_each_read_net_mut(&mut copy, |x| {
        if *x == net {
            n += 1;
        }
    });
    n
}

const SEL_ONE: &str = "module M\nport in x[0:4]\nport in y[0:4]\nport out o[0:4]\n\ncell $const[1'd1] :0 out={0}\ncell $mux :1 a=x[0:4] b=y[0:4] out=o[0:4] sel={0}\n";

const SEL_ZERO: &str = "module M\nport in x[0:4]\nport in y[0:4]\nport out o[0:4]\n\ncell $const[1'd0] :0 out={0}\ncell $mux :1 a=x[0:4] b=y[0:4] out=o[0:4] sel={0}\n";

const XY: Inputs<'static> = &[("x", 0xA, 4), ("y", 0x5, 4)];

#[test]
fn a_constant_one_select_bypasses_to_a() {
    let mut module = parse(SEL_ONE);
    let before = run(&module, XY, "o");
    assert_eq!(before.bits, CBits::Small(0xA), "precondition: o = x");
    let x = port_bits(&module, "x");

    assert!(simplify_muxes(&mut module));

    assert_eq!(
        port_bits(&module, "o").nets,
        x.nets,
        "o now reads x directly"
    );
    assert_valid(&module);
    eliminate_dead_cells(&mut module);
    assert_eq!(mux_count(&module), 0);
    assert_valid(&module);
    assert_eq!(run(&module, XY, "o"), before);
}

#[test]
fn a_constant_zero_select_bypasses_to_b() {
    let mut module = parse(SEL_ZERO);
    let before = run(&module, XY, "o");
    assert_eq!(before.bits, CBits::Small(0x5), "precondition: o = y");
    let y = port_bits(&module, "y");

    assert!(simplify_muxes(&mut module));

    assert_eq!(port_bits(&module, "o").nets, y.nets);
    eliminate_dead_cells(&mut module);
    assert_eq!(mux_count(&module), 0);
    assert_valid(&module);
    assert_eq!(run(&module, XY, "o"), before);
}

#[test]
fn a_chain_of_bypassed_muxes_resolves_to_the_source() {
    let mut module = parse(
        "module M\nport in x[0:4]\nport in y[0:4]\nport out o[0:4]\n\ncell $const[1'd1] :0 out={0}\ncell $mux :1 a=x[0:4] b=y[0:4] out={1,2,3,4} sel={0}\ncell $mux :2 a={1,2,3,4} b=y[0:4] out=o[0:4] sel={0}\n",
    );
    let before = run(&module, XY, "o");
    let x = port_bits(&module, "x");

    assert!(simplify_muxes(&mut module));

    assert_eq!(
        port_bits(&module, "o").nets,
        x.nets,
        "o skips both muxes, not just the outer one"
    );
    eliminate_dead_cells(&mut module);
    assert_eq!(mux_count(&module), 0);
    assert_valid(&module);
    assert_eq!(run(&module, XY, "o"), before);
}

const AB: Inputs<'static> = &[("a", 0x5A, 8), ("b", 0x3C, 8)];

#[test]
fn a_parameter_select_folds_then_bypasses() {
    let mut module = lower_valid(
        "module M(MODE: int = 0) {\n  in a: bits[8]\n  in b: bits[8]\n  out o: bits[8]\n  o = if MODE == 0 { a } else { b }\n}\n",
    );
    assert_eq!(mux_count(&module), 1, "precondition");
    let before = run(&module, AB, "o");

    fold_constants(&mut module);
    assert!(simplify_muxes(&mut module));
    eliminate_dead_cells(&mut module);

    assert!(module.cells.is_empty(), "left: {:?}", module.cells);
    assert_eq!(port_bits(&module, "o").nets, port_bits(&module, "a").nets);
    assert_valid(&module);
    assert_eq!(run(&module, AB, "o"), before);
}

#[test]
fn identical_data_nets_bypass_the_mux() {
    let mut module = lower_valid(
        "module M {\n  in c: bit\n  in a: bits[8]\n  out o: bits[8]\n  o = if c { a } else { a }\n}\n",
    );
    assert_eq!(mux_count(&module), 1, "precondition");

    assert!(simplify_muxes(&mut module));

    assert_eq!(port_bits(&module, "o").nets, port_bits(&module, "a").nets);
    eliminate_dead_cells(&mut module);
    assert_eq!(mux_count(&module), 0);
    assert_valid(&module);
    for c in 0..2 {
        assert_eq!(
            run(&module, &[("c", c, 1), ("a", 0x5A, 8)], "o").bits,
            CBits::Small(0x5A)
        );
    }
}

#[test]
fn equal_constant_data_bits_bypass_the_mux() {
    // `bits[3]`, 5's own width: `lower` does not yet size an all-constant
    // `if` to a wider declared port (docs/audit/gaps.md, GAP-1).
    let mut module =
        lower_valid("module M {\n  in c: bit\n  out o: bits[3]\n  o = if c { 5 } else { 5 }\n}\n");
    let mux = module
        .cells
        .iter()
        .find(|c| c.kind == CellKind::Mux)
        .expect("precondition: one mux");
    assert_ne!(
        mux.pins["a"].nets, mux.pins["b"].nets,
        "precondition: two separate Const cells, so R2 must compare values, not nets"
    );

    assert!(simplify_muxes(&mut module));

    eliminate_dead_cells(&mut module);
    assert_eq!(mux_count(&module), 0);
    assert_valid(&module);
    for c in 0..2 {
        assert_eq!(run(&module, &[("c", c, 1)], "o").bits, CBits::Small(5));
    }
}

const SLICE_HOLD: &str = "module M {\n  clock clk\n  reset rst\n  in c: bit\n  in v: bits[4]\n  out o: bits[8]\n  reg q: bits[8] = 0\n  on rise(clk) {\n    if c {\n      q[7:4] <- v\n    }\n  }\n  o = q\n}\n";

const SLICE_STEPS: &[Inputs<'static>] = &[
    &[("rst", 1, 1), ("c", 0, 1), ("v", 0x0, 4)],
    &[("rst", 0, 1), ("c", 1, 1), ("v", 0x9, 4)],
    &[("rst", 0, 1), ("c", 0, 1), ("v", 0x3, 4)],
    &[("rst", 0, 1), ("c", 1, 1), ("v", 0x6, 4)],
];

#[test]
fn only_the_identical_bits_of_a_hold_mux_are_bypassed() {
    let mut module = lower_valid(SLICE_HOLD);
    let q = module.signals["q"].clone();
    let is_hold = |c: &Cell| c.kind == CellKind::Mux && c.pins["b"] == q;
    let hold = module
        .cells
        .iter()
        .position(is_hold)
        .expect("precondition: the if-merge mux holds q on its b side");
    let out = module.cells[hold].pins["out"].clone();
    let before = trace(&module, SLICE_STEPS, "o");

    assert!(simplify_muxes(&mut module));

    for i in 0..4 {
        assert_eq!(
            read_count(&module, out.nets[i]),
            0,
            "bit {i} is q's own bit on both sides"
        );
    }
    for i in 4..8 {
        assert!(
            read_count(&module, out.nets[i]) > 0,
            "bit {i} still selects v or q"
        );
    }
    eliminate_dead_cells(&mut module);
    assert!(
        module.cells.iter().any(is_hold),
        "the mux still drives bits 4..8"
    );
    assert_valid(&module);
    assert_eq!(trace(&module, SLICE_STEPS, "o"), before);
}

#[test]
fn a_named_wire_over_a_bypassed_mux_stays_readable() {
    let mut module = lower_valid(
        "module M {\n  in c: bit\n  in a: bits[8]\n  out o: bits[8]\n  wire w: bits[8] = if c { a } else { a }\n  o = w ^ a\n}\n",
    );

    assert!(simplify_muxes(&mut module));
    eliminate_dead_cells(&mut module);

    assert_eq!(mux_count(&module), 0);
    assert_valid(&module);
    assert_eq!(
        run(&module, &[("c", 1, 1), ("a", 0x5A, 8)], "w").bits,
        CBits::Small(0x5A),
        "signals[\"w\"] follows the bypass instead of being dropped with the mux"
    );
}

#[test]
fn leaves_a_mux_with_a_live_select_and_different_data_alone() {
    let mut module = lower_valid(
        "module M {\n  in c: bit\n  in a: bits[8]\n  in b: bits[8]\n  out o: bits[8]\n  o = if c { a } else { b }\n}\n",
    );
    let snapshot = format!("{module:?}");

    assert!(!simplify_muxes(&mut module));
    assert_eq!(format!("{module:?}"), snapshot);
}

#[test]
fn a_second_call_reports_no_change() {
    let mut module = parse(SEL_ONE);
    assert!(simplify_muxes(&mut module));
    assert_eq!(
        mux_count(&module),
        1,
        "removing the bypassed mux is DCE's job"
    );
    let snapshot = format!("{module:?}");

    assert!(!simplify_muxes(&mut module));
    assert_eq!(format!("{module:?}"), snapshot);
}

const SXYZ: [Inputs<'static>; 2] = [
    &[("s", 0, 1), ("x", 0x3, 4), ("y", 0x5, 4), ("z", 0xC, 4)],
    &[("s", 1, 1), ("x", 0x3, 4), ("y", 0x5, 4), ("z", 0xC, 4)],
];

fn outputs_over_s(module: &Module, name: &str) -> Vec<Val> {
    SXYZ.iter()
        .map(|inputs| run(module, inputs, name))
        .collect()
}

const PORTS_SXYZ: &str =
    "module M\nport in s[0:1]\nport in x[0:4]\nport in y[0:4]\nport in z[0:4]\nport out o[0:4]\n";

#[test]
fn r3_skips_an_inner_mux_on_the_a_side() {
    let mut module = parse(&format!(
        "{PORTS_SXYZ}\ncell $mux :0 a=x[0:4] b=y[0:4] out={{0,1,2,3}} sel=s[0:1]\ncell $mux :1 a={{0,1,2,3}} b=z[0:4] out=o[0:4] sel=s[0:1]\n"
    ));
    let before = outputs_over_s(&module, "o");

    assert!(simplify_muxes(&mut module));

    assert_eq!(
        module.cells[1].pins["a"],
        port_bits(&module, "x"),
        "mux(s, mux(s, x, y), z) -> mux(s, x, z)"
    );
    eliminate_dead_cells(&mut module);
    assert_eq!(mux_count(&module), 1, "the inner mux is dead");
    assert_valid(&module);
    assert_eq!(outputs_over_s(&module, "o"), before);
}

#[test]
fn r3_skips_an_inner_mux_on_the_b_side() {
    let mut module = parse(&format!(
        "{PORTS_SXYZ}\ncell $mux :0 a=x[0:4] b=y[0:4] out={{0,1,2,3}} sel=s[0:1]\ncell $mux :1 a=z[0:4] b={{0,1,2,3}} out=o[0:4] sel=s[0:1]\n"
    ));
    let before = outputs_over_s(&module, "o");

    assert!(simplify_muxes(&mut module));

    assert_eq!(
        module.cells[1].pins["b"],
        port_bits(&module, "y"),
        "mux(s, z, mux(s, x, y)) -> mux(s, z, y)"
    );
    eliminate_dead_cells(&mut module);
    assert_eq!(mux_count(&module), 1);
    assert_valid(&module);
    assert_eq!(outputs_over_s(&module, "o"), before);
}

#[test]
fn r3_keeps_an_inner_mux_that_has_another_reader() {
    let mut module = parse(&format!(
        "{PORTS_SXYZ}port out p[0:4]\n\ncell $mux :0 a=x[0:4] b=y[0:4] out={{0,1,2,3}} sel=s[0:1]\ncell $mux :1 a={{0,1,2,3}} b=z[0:4] out=o[0:4] sel=s[0:1]\ncell $not :2 a={{0,1,2,3}} out=p[0:4]\n"
    ));
    let before = (outputs_over_s(&module, "o"), outputs_over_s(&module, "p"));

    assert!(simplify_muxes(&mut module));
    eliminate_dead_cells(&mut module);

    assert_eq!(mux_count(&module), 2, "the inner mux still feeds p");
    assert_valid(&module);
    assert_eq!(
        (outputs_over_s(&module, "o"), outputs_over_s(&module, "p")),
        before
    );
}

#[test]
fn r3_leaves_nested_muxes_on_different_selects_alone() {
    let mut module = parse(
        "module M\nport in s[0:1]\nport in t[0:1]\nport in x[0:4]\nport in y[0:4]\nport in z[0:4]\nport out o[0:4]\n\ncell $mux :0 a=x[0:4] b=y[0:4] out={0,1,2,3} sel=t[0:1]\ncell $mux :1 a={0,1,2,3} b=z[0:4] out=o[0:4] sel=s[0:1]\n",
    );
    let snapshot = format!("{module:?}");

    assert!(!simplify_muxes(&mut module));
    assert_eq!(format!("{module:?}"), snapshot);
}
