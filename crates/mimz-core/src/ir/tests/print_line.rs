use crate::ir::{lower, print_line};

#[test]
fn prints_a_single_add_cell_readably() {
    // Reuse the exact Design from lower_binops.rs's
    // lowers_wire_add_of_two_inputs_to_an_add_cell test (factor that
    // Design-construction into a shared `pub(super) fn adder_design()`
    // helper in tests/mod.rs so this task and Task 5 don't duplicate it).
    let design = crate::ir::tests::adder_design();
    let module = lower(&design);
    let text = print_line::print(&module);
    assert!(text.contains("cell $add"));
    assert!(text.contains("a=a[0:8]"));
    assert!(text.contains("out=sum[0:9]"));
}
