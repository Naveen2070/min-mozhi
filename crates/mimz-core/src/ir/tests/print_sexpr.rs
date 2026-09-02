use crate::ir::{lower, print_sexpr};

#[test]
fn prints_a_single_add_cell_as_an_sexpr() {
    let design = crate::ir::tests::adder_design();
    let module = lower(&design);
    let text = print_sexpr::print(&module);
    assert!(text.contains("(cell $add"));
    assert!(text.contains("(a a)"));
    assert!(text.contains("(out sum)"));
}
