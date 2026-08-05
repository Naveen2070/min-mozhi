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
fn assert_parses_in_a_module_body() {
    let f = parse_ok("module M {\n  in a: bit\n  out y: bit\n  assert(a)\n  y = a\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    // items: [0] in a, [1] out y, [2] assert(a), [3] y = a
    assert!(matches!(&m.items[2], ModuleItem::Assert(_)));
}

#[test]
fn assert_with_a_message_parses_in_a_module_body() {
    let f = parse_ok(
        "module M {\n  in a: bit\n  out y: bit\n  assert(a, \"a must be set\")\n  y = a\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let ModuleItem::Assert(a) = &m.items[2] else {
        panic!("expected ModuleItem::Assert")
    };
    assert_eq!(a.msg.as_deref(), Some("a must be set"));
}

#[test]
fn assert_message_must_be_a_string_literal() {
    let d = parse_err("module M {\n  in a: bit\n  out y: bit\n  assert(a, a)\n}\n");
    assert_eq!(d[0].code, Some("E1101"));
    assert!(d[0].help.is_some());
}

#[test]
fn assert_parses_inside_an_on_block() {
    let f = parse_ok(
        "module M {\n  clock clk\n  in a: bit\n  out y: bit\n  reg r: bit = 0\n  \
         on rise(clk) {\n    assert(a)\n    r <- a\n  }\n  y = r\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    // items: [0] clock, [1] in a, [2] out y, [3] reg r, [4] on-block, [5] y = r
    let ModuleItem::On(on) = &m.items[4] else {
        panic!("expected the on-block")
    };
    assert!(matches!(&on.body[0], SeqStmt::Assert(_)));
}

#[test]
fn assert_parses_inside_an_on_block_thamizh_order() {
    // `assert` is keyword-first in BOTH word orders (like `loop`/`default`/
    // `foreach`) — not a clause head, so no SOV flip is needed.
    let f = parse_ok(
        "ilakkanam thamizh\nthoguthi M {\n  thudippu clk\n  ulleedu a: bit\n  veliyeedu y: bit\n  pathivedu r: bit = 0\n  \
         yetram(clk) pothu {\n    assert(a)\n    r <- a\n  }\n  y = r\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let ModuleItem::On(on) = &m.items[4] else {
        panic!("expected the on-block")
    };
    assert!(matches!(&on.body[0], SeqStmt::Assert(_)));
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
