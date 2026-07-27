use super::*;

#[test]
fn qualified_module_reference_parses() {
    let file = parse_ok("module M {\n  let x = a.b.Foo() { }\n}\n");
    let TopItem::Module(m) = &file.items[0] else {
        panic!("expected a module")
    };
    let ModuleItem::Inst(inst) = &m.items[0] else {
        panic!("expected an inst")
    };
    assert_eq!(inst.module.path.len(), 2);
    assert_eq!(inst.module.path[0].name, "a");
    assert_eq!(inst.module.path[1].name, "b");
    assert_eq!(inst.module.name.name, "Foo");
}

#[test]
fn bare_module_reference_still_parses_with_empty_path() {
    let file = parse_ok("module M {\n  let x = Foo() { }\n}\n");
    let TopItem::Module(m) = &file.items[0] else {
        panic!("expected a module")
    };
    let ModuleItem::Inst(inst) = &m.items[0] else {
        panic!("expected an inst")
    };
    assert!(inst.module.is_bare());
    assert_eq!(inst.module.name.name, "Foo");
}

#[test]
fn array_literal_parses() {
    let e = parse_expr_ok("[1, 2, 3, 4]");
    let ExprKind::ArrayLit(elems) = &e.kind else {
        panic!("expected an array literal, got {:?}", e.kind)
    };
    assert_eq!(elems.len(), 4);
}

#[test]
fn empty_array_literal_parses() {
    // The grammar allows it; the CHECKER rejects a zero-length array
    // (mirrors mem's own E0410 "at least one cell" rule) — this test only
    // proves the parser doesn't choke on `[]`, not that it's accepted
    // end-to-end.
    let e = parse_expr_ok("[]");
    let ExprKind::ArrayLit(elems) = &e.kind else {
        panic!("expected an array literal")
    };
    assert_eq!(elems.len(), 0);
}

#[test]
fn array_literal_as_fn_call_argument_parses() {
    let f = parse_ok(
        "fn f(vals: bits[8][4]) -> bits[8] {\n  vals[0]\n}\nmodule M {\n  out o: bits[8]\n  o = f([1, 2, 3, 4])\n}\n",
    );
    let TopItem::Module(m) = &f.items[1] else {
        panic!("not a module")
    };
    let drive = m
        .items
        .iter()
        .find_map(|it| match it {
            ModuleItem::Drive { rhs, .. } => Some(rhs),
            _ => None,
        })
        .expect("a drive");
    let ExprKind::FnCall { args, .. } = &drive.kind else {
        panic!("expected a call")
    };
    assert!(matches!(args[0].kind, ExprKind::ArrayLit(_)));
}
