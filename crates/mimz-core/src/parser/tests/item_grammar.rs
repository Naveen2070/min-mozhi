use super::*;

#[test]
fn on_fall_parses_with_the_fall_edge() {
    let f = parse_ok(
        "module M {\n  clock clk\n  reset rst\n  reg r: bits[8] = 0\n  on fall(clk) {\n    r <- r +% 1\n  }\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let on = m
        .items
        .iter()
        .find_map(|it| match it {
            ModuleItem::On(o) => Some(o),
            _ => None,
        })
        .expect("an `on` block");
    assert_eq!(on.edge, Edge::Fall);
    assert_eq!(on.clock.name, "clk");
}

#[test]
fn mem_declaration_parses_to_a_mem_item() {
    let f = parse_ok("module M {\n  mem m: bits[8][4] = 0\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let mem = m
        .items
        .iter()
        .find_map(|it| match it {
            ModuleItem::Mem {
                name, depth, init, ..
            } => Some((name, depth, init)),
            _ => None,
        })
        .expect("a `mem` declaration");
    assert_eq!(mem.0.name, "m");
}

#[test]
fn a_mem_without_an_init_value_is_e1104() {
    let d = parse_err("module M {\n  mem m: bits[8][4]\n}\n");
    assert_eq!(d[0].code, Some("E1104"));
}

#[test]
fn array_type_parses_in_a_fn_param() {
    let f = parse_ok("fn f(vals: bits[8][4]) -> bits[8] {\n  vals[0]\n}");
    let TopItem::Func(fd) = &f.items[0] else {
        panic!("not a func")
    };
    let Type::Array { elem, len: _ } = &fd.params[0].ty else {
        panic!("expected an array type, got {:?}", fd.params[0].ty)
    };
    assert!(matches!(**elem, Type::Bits(_)));
}

#[test]
fn nested_array_type_parses_two_brackets_deep() {
    // The grammar doesn't reject this (nested arrays are a NON-goal
    // rejected by the CHECKER, not the parser — matches this project's
    // existing house style of "parser is lenient, checker narrows" used
    // elsewhere, e.g. `repeat` bodies parse generally and the checker
    // restricts what's inside). This test only proves the grammar itself
    // is unambiguous for a doubly-bracketed type — it makes no claim
    // about whether the CHECKER accepts it (it won't, once Task 5 lands
    // the non-goal rejection — that's a separate checker test).
    let f = parse_ok("fn f(vals: bits[8][4][2]) -> bits[8] {\n  0\n}");
    let TopItem::Func(fd) = &f.items[0] else {
        panic!("not a func")
    };
    let Type::Array { elem, .. } = &fd.params[0].ty else {
        panic!("expected outer array type")
    };
    assert!(matches!(**elem, Type::Array { .. }));
}
