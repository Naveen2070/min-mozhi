use crate::ir::{Bits, Cell, CellKind, NetId, lower, validate};

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
