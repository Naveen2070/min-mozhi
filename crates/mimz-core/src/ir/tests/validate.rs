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
    let short = Bits::unsigned(vec![NetId(0)]); // 1 bit, but Add's `out` needs 9
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
        pins: [("clk_in", clk), ("unexpected_pin", Bits::unsigned(vec![]))]
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
        vec![
            ("clk_in".to_string(), 1, crate::ast::Dir::In),
            ("locked".to_string(), 1, crate::ast::Dir::Out),
        ],
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

/// One `Pll` black box whose `clk_out` pin is a fresh 1-bit net, plus
/// whether `extern_decls` records `clk_out` as an output.
fn extern_out_module(declared: bool) -> (crate::ir::Module, Bits) {
    let mut module = crate::ir::Module {
        name: "bb3".to_string(),
        ports: Vec::new(),
        cells: Vec::new(),
        nets: Vec::new(),
        extern_decls: Default::default(),
        signals: Default::default(),
        port_declared_widths: Default::default(),
    };
    let clk_out = module.alloc_bits(1, None);
    module.cells.push(Cell {
        kind: CellKind::BlackBox {
            module_name: "Pll".to_string(),
        },
        pins: [("clk_out", clk_out.clone())].into_iter().collect(),
        span: Span::default(),
    });
    if declared {
        module.extern_decls.insert(
            "Pll".to_string(),
            vec![("clk_out".to_string(), 1, crate::ast::Dir::Out)],
        );
    }
    (module, clk_out)
}

#[test]
fn an_extern_output_also_driven_by_a_cell_is_a_multiple_driver() {
    let (mut module, clk_out) = extern_out_module(true);
    module.cells.push(Cell {
        kind: CellKind::Const {
            value: crate::checker::consteval::ConstVal {
                bits: crate::bits::Bits::Small(0),
                width: 1,
                signed: false,
            },
        },
        pins: [("out", clk_out)].into_iter().collect(),
        span: Span::default(),
    });

    let errors = validate::validate(&module);

    assert!(
        errors
            .iter()
            .any(|e| matches!(e, validate::ValidationError::MultipleDrivers { .. })),
        "got {errors:?}"
    );
}

#[test]
fn an_extern_output_with_no_declared_shape_stays_undriven() {
    let (module, _) = extern_out_module(false);

    let errors = validate::validate(&module);

    assert!(
        errors
            .iter()
            .any(|e| matches!(e, validate::ValidationError::UndrivenNet { .. })),
        "got {errors:?}"
    );
}

/// One `Reg` black box with a single 1-bit pin called `pin`, declared with
/// direction `dir`.
fn named_pin_module(pin: &'static str, dir: crate::ast::Dir) -> (crate::ir::Module, Bits) {
    let mut module = crate::ir::Module {
        name: "bb4".to_string(),
        ports: Vec::new(),
        cells: Vec::new(),
        nets: Vec::new(),
        extern_decls: Default::default(),
        signals: Default::default(),
        port_declared_widths: Default::default(),
    };
    let net = module.alloc_bits(1, None);
    module.cells.push(Cell {
        kind: CellKind::BlackBox {
            module_name: "Reg".to_string(),
        },
        pins: [(pin, net.clone())].into_iter().collect(),
        span: Span::default(),
    });
    module
        .extern_decls
        .insert("Reg".to_string(), vec![(pin.to_string(), 1, dir)]);
    (module, net)
}

#[test]
fn a_declared_extern_output_named_q_is_driven_once() {
    // Register-like externs (`out q`) used to be counted twice: once by the
    // `out`/`q`/`rdata` name rule and once by the declared direction.
    let (module, _) = named_pin_module("q", crate::ast::Dir::Out);

    assert_eq!(validate::validate(&module), Vec::new());
}

#[test]
fn a_declared_extern_input_named_out_is_not_a_driver() {
    let (mut module, net) = named_pin_module("out", crate::ast::Dir::In);
    module.cells.push(Cell {
        kind: CellKind::Const {
            value: crate::checker::consteval::ConstVal {
                bits: crate::bits::Bits::Small(1),
                width: 1,
                signed: false,
            },
        },
        pins: [("out", net)].into_iter().collect(),
        span: Span::default(),
    });

    assert_eq!(validate::validate(&module), Vec::new());
}

#[test]
fn every_validation_error_has_a_one_line_message() {
    use crate::ir::NetId;
    use crate::ir::validate::ValidationError as E;
    let cases = [
        (
            E::MultipleDrivers {
                net: NetId(4),
                cell_indices: vec![1, 3],
            },
            "net 4 has 2 drivers (cells 1, 3)",
        ),
        (
            E::UndrivenNet { net: NetId(7) },
            "net 7 is read but nothing drives it",
        ),
        (
            E::WidthMismatch {
                cell_index: 2,
                pin: "a",
                expected: 8,
                found: 4,
            },
            "cell 2: pin `a` is 4 bits, expected 8",
        ),
        (
            E::CombinationalCycle {
                nets: vec![NetId(1), NetId(2)],
            },
            "combinational cycle through nets 1, 2",
        ),
        (
            E::BlackBoxPortMismatch {
                cell_index: 5,
                reason: "missing port `clk`".to_string(),
            },
            "cell 5: black box ports do not match the extern declaration: missing port `clk`",
        ),
        (
            E::PortWidthMismatch {
                port: "o".to_string(),
                declared: 8,
                found: 9,
            },
            "output port `o` is 9 bits, declared 8",
        ),
        (
            E::ShiftGrowthTooWide {
                cell_index: 6,
                lhs_width: 64,
                amount_width: 32,
            },
            "cell 6: left shift of a 64-bit value by a 32-bit amount can grow past the width limit",
        ),
    ];
    for (err, want) in cases {
        assert_eq!(err.to_string(), want);
    }
}
