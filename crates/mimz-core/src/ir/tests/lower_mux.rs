use super::{ident, lower_valid, w};
use crate::ast::{Arm, Expr, ExprKind, Pattern};
use crate::elaborate::{Design, Signal};
use crate::ir::{Bits, Cell, CellKind, Module, lower};
use crate::span::Span;
use std::collections::BTreeMap;

fn int_lit(value: u128) -> Expr {
    Expr {
        kind: ExprKind::Int {
            value: crate::bits::Bits::Small(value),
            raw: value.to_string(),
        },
        span: Span::default(),
    }
}

fn int_pattern(value: u128) -> Pattern {
    Pattern::Int {
        value: crate::bits::Bits::Small(value),
        raw: value.to_string(),
    }
}

fn find_port<'a>(module: &'a Module, name: &str) -> &'a Bits {
    &module
        .ports
        .iter()
        .find(|(n, ..)| n == name)
        .expect("port")
        .1
}

/// `wire out = if sel { a } else { b }` — the simplest design that lowers
/// to a single `Mux` cell. Reused by Task 13's round-trip test as well as
/// this file's own lowering test.
pub(super) fn if_mux_design() -> Design {
    let mut comb = BTreeMap::new();
    comb.insert(
        "out".to_string(),
        Expr {
            kind: ExprKind::IfExpr {
                cond: Box::new(ident("sel")),
                then: Box::new(ident("a")),
                els: Box::new(ident("b")),
            },
            span: Span::default(),
        },
    );
    Design {
        module: "muxer".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "sel".into(),
                width: w(1),
            },
            Signal {
                name: "a".into(),
                width: w(8),
            },
            Signal {
                name: "b".into(),
                width: w(8),
            },
        ],
        outputs: vec![Signal {
            name: "out".into(),
            width: w(8),
        }],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    }
}

#[test]
fn lowers_if_expr_to_a_mux_cell() {
    let design = if_mux_design();
    let module = lower(&design);

    let mux_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Mux)
        .collect();
    assert_eq!(mux_cells.len(), 1);
    let mux = mux_cells[0];
    assert_eq!(mux.pins["sel"].width(), 1);
    assert_eq!(mux.pins["a"].width(), 8);
    assert_eq!(mux.pins["b"].width(), 8);
    assert_eq!(mux.pins["out"].width(), 8);
    assert_eq!(*find_port(&module, "out"), mux.pins["out"]);
}

#[test]
fn lowers_match_with_int_arms_and_wildcard_to_chained_mux_eq() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "out".to_string(),
        Expr {
            kind: ExprKind::Match {
                scrutinee: Box::new(ident("x")),
                arms: vec![
                    Arm {
                        patterns: vec![int_pattern(0)],
                        value: int_lit(10),
                    },
                    Arm {
                        patterns: vec![int_pattern(1)],
                        value: int_lit(20),
                    },
                    Arm {
                        patterns: vec![Pattern::Wildcard],
                        value: int_lit(30),
                    },
                ],
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "matcher".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "x".into(),
            width: w(8),
        }],
        outputs: vec![Signal {
            name: "out".into(),
            width: w(8),
        }],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    };
    let module = lower(&design);

    let eq_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Eq)
        .collect();
    assert_eq!(
        eq_cells.len(),
        2,
        "one Eq per literal arm, none for the wildcard"
    );

    let mux_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Mux)
        .collect();
    assert_eq!(mux_cells.len(), 2, "one Mux per non-last arm");

    // The design's `out` output is driven by the outermost (first-arm) mux.
    let out_bits = find_port(&module, "out");
    let outer_mux = mux_cells
        .iter()
        .find(|c| c.pins["out"] == *out_bits)
        .expect("outermost mux drives `out`");
    let inner_mux = mux_cells
        .iter()
        .find(|c| c.pins["out"] != *out_bits)
        .expect("the other mux is the inner one");

    // Outer mux's `b` (no-match fallthrough) traces to the inner mux's `out`.
    assert_eq!(outer_mux.pins["b"], inner_mux.pins["out"]);

    // Inner mux's `b` traces to the wildcard arm's constant value: a Const
    // cell's `out`, not another Mux/Eq.
    let const_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| matches!(c.kind, CellKind::Const { .. }))
        .collect();
    let wildcard_const = const_cells
        .iter()
        .find(|c| c.pins["out"] == inner_mux.pins["b"])
        .expect("inner mux's `b` pin traces to a Const cell (the wildcard arm's value)");
    let CellKind::Const { value } = &wildcard_const.kind else {
        unreachable!()
    };
    assert_eq!(value.bits, crate::bits::Bits::Small(30));

    // Pin down WHICH arm landed on the outer mux — this is the part that
    // distinguishes correct fold direction (arm 0 outermost/checked-first)
    // from an inverted one (arm 1 outermost): the outer mux's `a` must be
    // arm 0's value (Const 10), and its `sel` must be the Eq comparing
    // against arm 0's pattern (Const 0), not arm 1's (Const 1).
    let outer_a_const = const_cells
        .iter()
        .find(|c| c.pins["out"] == outer_mux.pins["a"])
        .expect("outer mux's `a` pin traces to a Const cell (arm 0's value)");
    let CellKind::Const {
        value: outer_a_value,
    } = &outer_a_const.kind
    else {
        unreachable!()
    };
    assert_eq!(
        outer_a_value.bits,
        crate::bits::Bits::Small(10),
        "outer mux's `a` should be arm 0's value (10), not arm 1's (20) — \
         a reversed fold would put arm 1 outermost instead"
    );

    let sel_const = const_cells
        .iter()
        .find(|c| {
            eq_cells
                .iter()
                .any(|eq| eq.pins["b"] == c.pins["out"] && eq.pins["out"] == outer_mux.pins["sel"])
        })
        .expect("outer mux's `sel` traces through an Eq cell to a Const cell (arm 0's pattern)");
    let CellKind::Const { value: sel_value } = &sel_const.kind else {
        unreachable!()
    };
    assert_eq!(
        sel_value.bits,
        crate::bits::Bits::Small(0),
        "outer mux's `sel` should compare against arm 0's pattern (0), not arm 1's (1) — \
         a reversed fold would check arm 1's pattern first"
    );
}

#[test]
fn lowers_match_with_int_mask_pattern_to_and_then_eq() {
    let mut comb = BTreeMap::new();
    comb.insert(
        "out".to_string(),
        Expr {
            kind: ExprKind::Match {
                scrutinee: Box::new(ident("x")),
                arms: vec![
                    Arm {
                        patterns: vec![Pattern::IntMask {
                            value: 0b1000_0000,
                            mask: 0b1000_0000,
                            width: 8,
                            raw: "1???????".to_string(),
                        }],
                        value: int_lit(1),
                    },
                    Arm {
                        patterns: vec![Pattern::Wildcard],
                        value: int_lit(0),
                    },
                ],
            },
            span: Span::default(),
        },
    );
    let design = Design {
        module: "masker".to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "x".into(),
            width: w(8),
        }],
        outputs: vec![Signal {
            name: "out".into(),
            width: w(1),
        }],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb,
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    };
    let module = lower(&design);

    let and_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::And)
        .collect();
    assert_eq!(and_cells.len(), 1, "one And cell for the mask");
    let eq_cells: Vec<&Cell> = module
        .cells
        .iter()
        .filter(|c| c.kind == CellKind::Eq)
        .collect();
    assert_eq!(eq_cells.len(), 1, "one Eq cell comparing the masked result");

    // The And cell's output feeds the Eq cell's `a` input.
    assert_eq!(and_cells[0].pins["out"], eq_cells[0].pins["a"]);
    assert_eq!(and_cells[0].pins["a"].width(), 8);
    assert_eq!(and_cells[0].pins["b"].width(), 8);
}

/// GAP-1: `lower_match`'s "every arm constant, no sibling" fallback used to
/// size each arm at its own natural width instead of the others' — `Light.Red`
/// (rewritten to the literal `0`, natural width 1) and `Light.Blue`
/// (rewritten to `2`, natural width 2) fed the same `push_mux_cell` chain at
/// different widths, and `push_mux_cell` wired them straight to `a`/`b`
/// unchanged. `validate`'s `Mux` `a`/`b`-vs-`out` check (added GAP-1 Task 6
/// round 4) caught this for real on `enum_encoding.mimz` and four sibling
/// examples (`priority.mimz`, `seg7.mimz`, `sync_loop_search.mimz`,
/// `traffic_light.mimz`) — this is `enum_encoding.mimz`'s own shape, pinned
/// directly rather than only through the full example-file coverage probe.
#[test]
fn mux_chain_widens_a_narrower_arm_to_match_a_wider_sibling() {
    let src = r#"
        module M {
          clock clk
          reset rst
          out state_bits: bits[2]
          enum Light { Red, Green, Blue }
          reg state: Light = Light.Red
          on rise(clk) {
            state <- match state {
              Light.Red   => Light.Green
              Light.Green => Light.Blue
              Light.Blue  => Light.Red
            }
          }
          state_bits = encoding(state)
        }
    "#;
    let _module = lower_valid(src);
}

/// Companion to the test above, over a SIGNED value — `widen_to` must
/// replicate the narrower operand's own sign bit, not pad with a fresh zero
/// constant, or a negative narrower arm corrupts into a positive one once
/// widened. Also depends on Task 3 (`lower_match`'s `target_width`
/// threading) to go fully green: this design's widest arm (`-3` / `2`, both
/// needing only 2 bits) is narrower than the declared `signed[4]` output, so
/// `push_mux_cell`'s widening alone still leaves a `PortWidthMismatch` —
/// confirm after Task 1 alone that the SIGN part is fixed (no more `Mux`
/// `a`/`b` `WidthMismatch`, only `PortWidthMismatch` remains).
#[test]
fn mux_chain_sign_extends_a_narrower_signed_arm() {
    let src = r#"
        module M {
          in sel: bits[2]
          out o: signed[4]
          o = match sel {
            0 => 1
            1 => -3
            _ => 2
          }
        }
    "#;
    let module = lower_valid(src);
    let mut exec = crate::ir::exec::Executor::new(&module);
    exec.set_input("sel", crate::value::Val::new(1, 2, false));
    exec.tick();
    let o = exec.get_output("o");
    assert_eq!(
        o.bits,
        crate::bits::Bits::Small(0b1101),
        "sel=1 selects the -3 arm — must stay -3 (4-bit two's complement 0b1101) after sign-extending from its own narrower natural width, not zero-pad into a positive value"
    );
}

/// GAP-1: closing the sibling bug above via `push_mux_cell` alone is not
/// sufficient — it only equalizes a `match`'s own arms against EACH OTHER,
/// not against the DECLARED width of whatever the match feeds. Here every
/// arm's own natural width (4 or 5 bits, from `10`/`20`/`30`) is narrower
/// than `o`'s declared `bits[8]`; closed by Task 3 of this plan
/// (`lower_expr_sized`'s dedicated `Match` arm threading `target_width` into
/// `lower_match`'s no-sibling fallback). Stays RED after Task 1 alone.
#[test]
fn match_with_all_constant_arms_narrower_than_out_widens_to_the_declared_port_width() {
    let src = r#"
        module M {
          in x: bits[8]
          out o: bits[8]
          o = match x {
            0 => 10
            1 => 20
            _ => 30
          }
        }
    "#;
    let module = lower_valid(src);
    let out = module.ports.iter().find(|(n, _, _)| n == "o").unwrap();
    assert_eq!(
        out.1.width(),
        8,
        "match result must size to the declared bits[8] output, not its widest arm's own natural width (5 bits, from 30)"
    );
    let mut exec = crate::ir::exec::Executor::new(&module);
    exec.set_input("x", crate::value::Val::new(1, 8, false));
    exec.tick();
    let o = exec.get_output("o");
    assert_eq!(
        o.bits,
        crate::bits::Bits::Small(20),
        "x=1 must select arm 1's value (20) at the correct width, not a truncated/corrupted one"
    );
}
