use crate::ast::{Builtin, ExprKind};
use crate::elaborate::{Design, Signal};
use crate::ir::{Bits, Cell, CellKind, NetId, lower, parse_line, validate};
use crate::span::Span;

use super::{ident, w};

#[test]
fn accepts_the_adder_module() {
    let design = crate::ir::tests::adder_design();
    let module = lower(&design);
    assert_eq!(validate::validate(&module), Vec::new());
}

#[test]
fn rejects_a_net_driven_by_two_cells() {
    let design = crate::ir::tests::adder_design();
    let mut module = lower(&design);
    // Duplicate the existing Add cell so `sum`'s net is now driven twice.
    let dup = module.cells[0].clone();
    module.cells.push(dup);
    let errors = validate::validate(&module);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, validate::ValidationError::MultipleDrivers { .. }))
    );
}

#[test]
fn rejects_a_read_of_an_undriven_net() {
    let design = crate::ir::tests::adder_design();
    let mut module = lower(&design);
    // Allocate a stray net nothing drives, then reference it as a pin on
    // an existing cell to simulate a lowering bug.
    let stray = module.alloc_bits(1, None);
    module.cells[0].pins.insert("out", stray); // overwrite the real `out` pin with the undriven stray net's Bits — the ORIGINAL `sum` net this cell used to drive now has NO driver at all, which is exactly the under-driving case being tested
    let errors = validate::validate(&module);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, validate::ValidationError::UndrivenNet { .. }))
    );
}

#[test]
fn rejects_a_pin_width_mismatch() {
    let design = crate::ir::tests::adder_design();
    let mut module = lower(&design);
    // Add's `out` must equal `max(a,b)+1` (`lower_binop`'s own formula,
    // 8/8 -> 9 here) — corrupt it to a width `lower_binop` would never
    // produce. (Shrinking `a` instead, as an earlier draft of this test
    // did, is NOT a violation: Add's `a`/`b` legitimately differ in
    // width per `width_rules::lossless_result`, and with `b` unchanged
    // at 8 the max()+1 formula still lands on 9 either way — `out` is
    // the pin with a genuine fixed-formula contract here, not `a`.)
    let short = Bits(vec![NetId(0)]); // 1 bit, but Add's `out` needs 9
    module.cells[0].pins.insert("out", short);
    let errors = validate::validate(&module);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, validate::ValidationError::WidthMismatch { .. }))
    );
}

#[test]
fn rejects_mismatched_widths_on_a_bitwise_cell() {
    // And/Or/Xor/Eq/... require a/b to be the SAME width — unlike
    // Add/Sub/Mul, which legitimately allow them to differ.
    let mut module = crate::ir::Module {
        name: "bitwise".to_string(),
        ports: Vec::new(),
        cells: Vec::new(),
        nets: Vec::new(),
        extern_decls: Default::default(),
        signals: Default::default(),
        port_declared_widths: Default::default(),
    };
    let a = module.alloc_bits(8, None);
    let b = module.alloc_bits(4, None);
    let out = module.alloc_bits(8, None);
    module.cells.push(Cell {
        kind: CellKind::And,
        pins: [("a", a), ("b", b), ("out", out)].into_iter().collect(),
        span: crate::span::Span::default(),
    });
    let errors = validate::validate(&module);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, validate::ValidationError::WidthMismatch { pin: "b", .. }))
    );
}

#[test]
fn accepts_mismatched_widths_on_an_add_cell() {
    // Add legitimately allows a/b to differ — must NOT be flagged.
    let mut module = crate::ir::Module {
        name: "adder2".to_string(),
        ports: Vec::new(),
        cells: Vec::new(),
        nets: Vec::new(),
        extern_decls: Default::default(),
        signals: Default::default(),
        port_declared_widths: Default::default(),
    };
    let a = module.alloc_bits(8, None);
    let b = module.alloc_bits(4, None);
    let out = module.alloc_bits(9, None);
    module
        .ports
        .push(("a".to_string(), a.clone(), crate::ast::Dir::In));
    module
        .ports
        .push(("b".to_string(), b.clone(), crate::ast::Dir::In));
    module.cells.push(Cell {
        kind: CellKind::Add,
        pins: [("a", a), ("b", b), ("out", out)].into_iter().collect(),
        span: crate::span::Span::default(),
    });
    assert_eq!(validate::validate(&module), Vec::new());
}

#[test]
fn rejects_a_combinational_cycle() {
    // Build a 2-cell module by hand: an Add cell whose `out` feeds back
    // into its own `a` pin (no Dff in between).
    let mut module = crate::ir::Module {
        name: "cyclic".to_string(),
        ports: Vec::new(),
        cells: Vec::new(),
        nets: Vec::new(),
        extern_decls: Default::default(),
        signals: Default::default(),
        port_declared_widths: Default::default(),
    };
    let a = module.alloc_bits(1, None);
    module.cells.push(Cell {
        kind: CellKind::Not,
        pins: [("a", a.clone()), ("out", a)].into_iter().collect(),
        span: crate::span::Span::default(),
    });
    let errors = validate::validate(&module);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, validate::ValidationError::CombinationalCycle { .. }))
    );
}

#[test]
fn rejects_a_blackbox_port_shape_mismatch() {
    let mut module = crate::ir::Module {
        name: "bb".to_string(),
        ports: Vec::new(),
        cells: Vec::new(),
        nets: Vec::new(),
        extern_decls: Default::default(),
        signals: Default::default(),
        port_declared_widths: Default::default(),
    };
    let clk = module.alloc_bits(1, Some("clk_in"));
    module.cells.push(Cell {
        kind: CellKind::BlackBox {
            module_name: "Pll".to_string(),
        },
        pins: [("clk_in", clk), ("unexpected_pin", Bits(vec![]))]
            .into_iter()
            .collect(),
        span: crate::span::Span::default(),
    });
    // Declared shape: `Pll` has `clk_in` (1 bit) and `locked` (1 bit, an
    // output). The instance above is missing `locked` entirely and has an
    // extra `unexpected_pin` not in the declared list — both are shape
    // mismatches `validate` must catch via `Module::extern_decls`.
    module.extern_decls.insert(
        "Pll".to_string(),
        vec![("clk_in".to_string(), 1), ("locked".to_string(), 1)],
    );
    let errors = validate::validate(&module);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, validate::ValidationError::BlackBoxPortMismatch { .. }))
    );
}

#[test]
fn accepts_a_blackbox_cell_with_no_declared_shape_on_record() {
    // v1 text-format gap: no `extern_decls` entry for this module_name ->
    // skip the check gracefully, no error.
    let mut module = crate::ir::Module {
        name: "bb2".to_string(),
        ports: Vec::new(),
        cells: Vec::new(),
        nets: Vec::new(),
        extern_decls: Default::default(),
        signals: Default::default(),
        port_declared_widths: Default::default(),
    };
    let clk = module.alloc_bits(1, Some("clk_in"));
    module
        .ports
        .push(("clk_in".to_string(), clk.clone(), crate::ast::Dir::In));
    module.cells.push(Cell {
        kind: CellKind::BlackBox {
            module_name: "Pll".to_string(),
        },
        pins: [("clk_in", clk)].into_iter().collect(),
        span: crate::span::Span::default(),
    });
    assert_eq!(validate::validate(&module), Vec::new());
}

#[test]
fn rejects_a_shl_cell_whose_out_is_narrower_than_the_worst_case_growth() {
    let mut module = crate::ir::Module {
        name: "shl_bad".to_string(),
        ports: Vec::new(),
        cells: Vec::new(),
        nets: Vec::new(),
        extern_decls: Default::default(),
        signals: Default::default(),
        port_declared_widths: Default::default(),
    };
    let a = module.alloc_bits(2, None);
    let b = module.alloc_bits(2, None);
    let out = module.alloc_bits(2, None); // should be 5 (2 + (2^2 - 1))
    module.cells.push(Cell {
        kind: CellKind::Shl,
        pins: [("a", a), ("b", b), ("out", out)].into_iter().collect(),
        span: crate::span::Span::default(),
    });
    let errors = validate::validate(&module);
    assert!(errors.iter().any(|e| matches!(
        e,
        validate::ValidationError::WidthMismatch { pin: "out", .. }
    )));
}

#[test]
fn rejects_an_output_port_never_driven_by_any_cell() {
    let mut module = crate::ir::Module {
        name: "undriven_out".to_string(),
        ports: Vec::new(),
        cells: Vec::new(),
        nets: Vec::new(),
        extern_decls: Default::default(),
        signals: Default::default(),
        port_declared_widths: Default::default(),
    };
    let y = module.alloc_bits(4, None);
    module
        .ports
        .push(("y".to_string(), y, crate::ast::Dir::Out));
    // No cell drives `y`'s nets at all — the textbook UndrivenNet case.
    let errors = validate::validate(&module);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, validate::ValidationError::UndrivenNet { .. })),
        "an out port with zero driving cells must be caught, got: {errors:?}"
    );
}

/// `out y: bits[16] = extend(a, 16)` over an 8-bit input `a` — a real
/// widening `Zext` cell, sized exactly to the declared width. No false
/// positive: `Check 6` must accept this.
fn extend_widens_design() -> Design {
    let mut comb = std::collections::BTreeMap::new();
    comb.insert(
        "y".to_string(),
        crate::ast::Expr {
            kind: ExprKind::Call {
                func: Builtin::Extend,
                args: vec![
                    ident("a"),
                    crate::ast::Expr {
                        kind: ExprKind::Int {
                            value: 16u128.into(),
                            raw: "16".to_string(),
                        },
                        span: Span::default(),
                    },
                ],
            },
            span: Span::default(),
        },
    );
    Design {
        module: "ext_ok".to_string(),
        consts: std::collections::BTreeMap::new(),
        inputs: vec![Signal {
            name: "a".into(),
            width: w(8),
        }],
        outputs: vec![Signal {
            name: "y".into(),
            width: w(16),
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
fn accepts_a_legitimately_sized_output_port() {
    let design = extend_widens_design();
    let module = lower(&design);
    assert_eq!(module.port_declared_widths.get("y"), Some(&16));
    assert_eq!(validate::validate(&module), Vec::new());
}

/// `out y: bits[10] = extend(a << sh, 10)` where `sh` (the shift amount)
/// is a RUNTIME value, not a compile-time constant — `lower_binop`'s
/// `Shl` sizing has no `shl_const_amount` to use, so it falls back to
/// worst-case growth (`8 + (2^2-1) == 11` bits) for the inner `a << sh`.
/// `extend`'s own `target <= base.width()` no-op branch then passes
/// those 11 bits straight through to `y` unchanged, even though the
/// source declared `y` as only 10 bits wide — GAP-1's "silent, not a
/// loud `WidthMismatch`" residual (`docs/audit/gaps.md`). `Check 6` must
/// catch this via `Module::port_declared_widths`.
fn extend_of_dynamic_shl_design() -> Design {
    let mut comb = std::collections::BTreeMap::new();
    comb.insert(
        "y".to_string(),
        crate::ast::Expr {
            kind: ExprKind::Call {
                func: Builtin::Extend,
                args: vec![
                    crate::ast::Expr {
                        kind: ExprKind::Binary {
                            op: crate::ast::BinOp::Shl,
                            lhs: Box::new(ident("a")),
                            rhs: Box::new(ident("sh")),
                        },
                        span: Span::default(),
                    },
                    crate::ast::Expr {
                        kind: ExprKind::Int {
                            value: 10u128.into(),
                            raw: "10".to_string(),
                        },
                        span: Span::default(),
                    },
                ],
            },
            span: Span::default(),
        },
    );
    Design {
        module: "ext_shl".to_string(),
        consts: std::collections::BTreeMap::new(),
        inputs: vec![
            Signal {
                name: "a".into(),
                width: w(8),
            },
            Signal {
                name: "sh".into(),
                width: w(2),
            },
        ],
        outputs: vec![Signal {
            name: "y".into(),
            width: w(10),
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
fn rejects_an_extend_no_op_output_wider_than_its_declaration() {
    let design = extend_of_dynamic_shl_design();
    let module = lower(&design);
    assert_eq!(module.port_declared_widths.get("y"), Some(&10));
    let errors = validate::validate(&module);
    assert!(
        errors.iter().any(|e| matches!(
            e,
            validate::ValidationError::PortWidthMismatch {
                port,
                declared: 10,
                found: 11,
            } if port == "y"
        )),
        "expected a PortWidthMismatch on `y` (declared 10, found 11), got: {errors:?}"
    );
}

#[test]
fn hand_parsed_fixture_with_no_declared_width_skips_the_port_width_check() {
    // `parse_line` never populates `port_declared_widths` (v1 text-format
    // gap, see `Module::port_declared_widths` doc) — a hand-parsed
    // module's output port has no declared width on record, so Check 6
    // must skip it gracefully rather than error, mirroring
    // `accepts_a_blackbox_cell_with_no_declared_shape_on_record` above.
    let text = "module bad\nport in a[0:8]\nport out sum[0:9]\n\ncell $add :0 a=a[0:8] b=a[0:8] out=sum[0:9]\n";
    let module = parse_line::parse(text).expect("fixture should be syntactically valid IR text");
    assert!(module.port_declared_widths.is_empty());
    assert_eq!(validate::validate(&module), Vec::new());
}
