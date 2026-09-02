use super::{ident, w};
use crate::elaborate::{Design, ExternInstance, Signal};
use crate::ir::{Cell, CellKind, lower};
use crate::span::Span;
use std::collections::BTreeMap;

/// The instance's own `let u = Pll() { .. }` span — arbitrary but non-
/// default, so a test relying on it can't pass by accident against
/// `Span::default()`.
const INST_SPAN: Span = Span { start: 10, end: 42 };

/// A design with one `extern module Pll` instance `u`, wired exactly as
/// `elaborate::flatten_extern_instance` would flatten
/// `let u = Pll() { clk_in: clk }` in `warn` `SimMode`: an input wire
/// `u_clk_in` driven by the connection expression, and a driverless output
/// wire `u_clk_out` recorded in `unknown_signals`.
pub(super) fn pll_design() -> Design {
    let mut comb = BTreeMap::new();
    comb.insert("u_clk_in".to_string(), ident("clk"));
    Design {
        module: "top".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "clk".into(),
            width: w(1),
        }],
        outputs: vec![],
        wires: vec![
            Signal {
                name: "u_clk_in".into(),
                width: w(1),
            },
            Signal {
                name: "u_clk_out".into(),
                width: w(1),
            },
        ],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: std::iter::once("u_clk_out".to_string()).collect(),
        extern_instances: vec![ExternInstance {
            module_name: "Pll".to_string(),
            ports: vec![
                (
                    "clk_in".to_string(),
                    Signal {
                        name: "u_clk_in".into(),
                        width: w(1),
                    },
                ),
                (
                    "clk_out".to_string(),
                    Signal {
                        name: "u_clk_out".into(),
                        width: w(1),
                    },
                ),
            ],
            span: INST_SPAN,
        }],
        asserts: vec![],
        covers: vec![],
    }
}

#[test]
fn lowers_extern_instance_to_one_blackbox_cell_with_matching_pins() {
    let design = pll_design();
    let module = lower(&design);

    let blackboxes: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| matches!(c.kind, CellKind::BlackBox { .. }))
        .collect();
    assert_eq!(blackboxes.len(), 1, "expected exactly one BlackBox cell");
    let bb = blackboxes[0];
    let CellKind::BlackBox { module_name } = &bb.kind else {
        unreachable!()
    };
    assert_eq!(module_name, "Pll");
    assert_eq!(
        bb.span, INST_SPAN,
        "the cell traces back to the instantiation's own span, not a dummy default"
    );

    assert_eq!(
        bb.pins.len(),
        2,
        "exactly the declared port list, no extra pins"
    );
    assert_eq!(bb.pins["clk_in"].width(), 1);
    assert_eq!(bb.pins["clk_out"].width(), 1);

    // `clk_in` traces to the module's own `clk` input net (its comb driver
    // is `Ident("clk")`, so lowering resolves it to the same Bits).
    let clk_bits = &module.ports.iter().find(|(n, ..)| n == "clk").unwrap().1;
    assert_eq!(&bb.pins["clk_in"], clk_bits);

    // `clk_out` is driverless (an extern output, in `unknown_signals`) —
    // still a real, distinct net, freshly allocated for this pin.
    assert_ne!(bb.pins["clk_out"], bb.pins["clk_in"]);
}
