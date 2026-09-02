use crate::elaborate::Design;
use crate::ir::{lower, parse_line, print_line};

/// Asserts `print(parse(print(lower(design)))) == print(lower(design))` —
/// text-stability of the round trip, not `Module == Module` structural
/// equality (net-id renumbering during parse is fine as long as it
/// reprints identically; see Task 13 brief).
fn assert_round_trips(design: &Design) {
    let module = lower(design);
    let text = print_line::print(&module);
    let reparsed = parse_line::parse(&text).expect("should parse what we just printed");
    let reprinted = print_line::print(&reparsed);
    assert_eq!(
        text, reprinted,
        "print(parse(print(m))) must equal print(m)"
    );
}

#[test]
fn round_trips_the_adder_module() {
    // Exercises CellKind::Add.
    assert_round_trips(&crate::ir::tests::adder_design());
}

#[test]
fn round_trips_the_mux_module() {
    // Exercises CellKind::Mux (and the Const cells feeding its ports via
    // ordinary named nets, not literals here).
    assert_round_trips(&super::lower_mux::if_mux_design());
}

#[test]
fn round_trips_the_reg_module() {
    // Exercises CellKind::Dff.
    assert_round_trips(&super::lower_regs::reg_design());
}

#[test]
fn round_trips_the_mem_module() {
    // Exercises CellKind::Mem (plus Const, And-free Mux for the gated
    // write-enable in other lower_mem fixtures — this one is the
    // unconditional-write shape).
    assert_round_trips(&super::lower_mem::ram_design());
}

#[test]
fn round_trips_the_blackbox_module() {
    // Exercises CellKind::BlackBox, including its dynamically-named pins
    // (leak_pin_name's fallback path, not the fixed match arms).
    assert_round_trips(&super::lower_blackbox::pll_design());
}

#[test]
fn a_huge_bracket_net_id_is_rejected_not_allocated() {
    // A malformed/adversarial net id (`vec![NetInfo; u32::MAX as usize]`
    // would be a ~100GB OOM, not a parse error) must come back as `Err`,
    // not panic/hang/abort — see the cap in `parse`.
    let text = "module m\n\ncell $add a={4294967295} b={0} out={1}\n";
    let result = parse_line::parse(text);
    assert!(
        result.is_err(),
        "a huge bracket net id must be rejected, not allocated"
    );
}
