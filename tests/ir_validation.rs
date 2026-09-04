//! Standing regression corpus for `mimz_core::ir::validate`'s five checks,
//! mirroring `tests/fixtures/errors/`'s broken-corpus role for the AST
//! checker but at the IR-text level: one hand-written, deliberately-broken
//! `.ir` fixture per check under `tests/fixtures/ir_errors/`, parsed via
//! `parse_line::parse` (the hand-writable line-based text format) and
//! confirmed to fail `validate::validate` with the targeted error variant.
//!
//! A `BlackBoxPortMismatch` fixture is deliberately absent: `parse_line`
//! always constructs `Module::extern_decls` empty (a documented v1
//! text-format gap — see that field's doc comment in `ir/mod.rs` and the
//! comment in `parse_line::parse`), so a `$blackbox` cell parsed from
//! hand-written text never has a declared shape on record, and `validate`'s
//! black-box check skips gracefully rather than flagging anything. This
//! matches `ir::tests::validate`'s own
//! `accepts_a_blackbox_cell_with_no_declared_shape_on_record` unit test,
//! which asserts exactly this no-entry-means-no-error behavior. There is no
//! way to construct a rejected `$blackbox` fixture through the text-format
//! route in v1.

use mimz_core::ir::{parse_line, validate};

fn assert_fixture_rejected(path: &str, check: impl Fn(&validate::ValidationError) -> bool) {
    let text = std::fs::read_to_string(path).expect("fixture should exist");
    let module = parse_line::parse(&text).expect("fixture should be syntactically valid IR text");
    let errors = validate::validate(&module);
    assert!(
        errors.iter().any(check),
        "expected fixture {path} to fail validation with the targeted check, got: {errors:?}"
    );
}

#[test]
fn multiple_drivers_fixture_is_rejected() {
    assert_fixture_rejected("tests/fixtures/ir_errors/multiple_drivers.ir", |e| {
        matches!(e, validate::ValidationError::MultipleDrivers { .. })
    });
}

#[test]
fn undriven_net_fixture_is_rejected() {
    assert_fixture_rejected("tests/fixtures/ir_errors/undriven_net.ir", |e| {
        matches!(e, validate::ValidationError::UndrivenNet { .. })
    });
}

#[test]
fn width_mismatch_fixture_is_rejected() {
    assert_fixture_rejected("tests/fixtures/ir_errors/width_mismatch.ir", |e| {
        matches!(
            e,
            validate::ValidationError::WidthMismatch {
                pin: "sel",
                expected: 1,
                found: 2,
                ..
            }
        )
    });
}

#[test]
fn combinational_cycle_fixture_is_rejected() {
    assert_fixture_rejected("tests/fixtures/ir_errors/combinational_cycle.ir", |e| {
        matches!(e, validate::ValidationError::CombinationalCycle { .. })
    });
}
