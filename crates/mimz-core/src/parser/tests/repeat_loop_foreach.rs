use super::*;

#[test]
fn parses_repeat_and_const() {
    parse_ok(
        "const N: int = 8\nmodule M {\n  in e: bits[8]\n  out led: bits[8]\n  repeat i: 0..8 {\n    led[i] = e[i]\n  }\n}\n",
    );
}

#[test]
fn parses_loop_inside_on_block() {
    let f = parse_ok(
        "module M {\n  in clk: bit\n  reg acc: bits[8] = 0\n  on rise(clk) {\n    loop i: 0..4 {\n      acc <- acc\n    }\n  }\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!("expected module")
    };
    let on = m
        .items
        .iter()
        .find_map(|i| match i {
            ModuleItem::On(o) => Some(o),
            _ => None,
        })
        .expect("an `on` block");
    assert_eq!(on.body.len(), 1);
    let SeqStmt::Loop {
        var, lo, hi, body, ..
    } = &on.body[0]
    else {
        panic!("expected Loop")
    };
    assert_eq!(var.name, "i");
    assert!(matches!(&lo.kind, ExprKind::Int { value, .. } if *value == Bits::Small(0)));
    assert!(matches!(&hi.kind, ExprKind::Int { value, .. } if *value == Bits::Small(4)));
    assert_eq!(body.len(), 1);
}

#[test]
fn sync_loop_parses() {
    let f = parse_ok(
        "module M {\n  clock clk\n  mem m: bits[8][8] = 0\n  in key: bits[8]\n  sync loop find_first on rise(clk) (i: 0..8) -> result: signed[4] = 0 - 1 {\n    if m[i] == key { result <- i }\n  }\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!("expected module")
    };
    let sl = m
        .items
        .iter()
        .find_map(|it| match it {
            ModuleItem::SyncLoop(sl) => Some(sl),
            _ => None,
        })
        .expect("sync loop item parsed");
    assert_eq!(sl.name.name, "find_first");
    assert_eq!(sl.clock.name, "clk");
    assert!(matches!(sl.edge, Edge::Rise));
    assert_eq!(sl.var.name, "i");
    assert!(matches!(&sl.lo.kind, ExprKind::Int { value, .. } if *value == Bits::Small(0)));
    assert!(matches!(&sl.hi.kind, ExprKind::Int { value, .. } if *value == Bits::Small(8)));
    assert_eq!(sl.result_name.name, "result");
    assert!(matches!(sl.result_ty, Type::Signed(_)));
    assert_eq!(sl.body.len(), 1);
}

#[test]
fn parses_loop_inside_fn_body() {
    let f = parse_ok(
        "fn find(vals: bits[8][4]) -> signed[4] {\n  loop i: 0..4 {\n    if vals[i] == 0xFF { return i }\n  }\n  0 - 1\n}\nmodule M {\n  in a: bits[8][4]\n  out o: signed[4]\n  o = find(a)\n}\n",
    );
    let TopItem::Func(fd) = &f.items[0] else {
        panic!("not a func")
    };
    assert_eq!(fd.stmts.len(), 1);
    let FnStmt::Loop {
        var, lo, hi, body, ..
    } = &fd.stmts[0]
    else {
        panic!("expected Loop")
    };
    assert_eq!(var.name, "i");
    assert!(matches!(&lo.kind, ExprKind::Int { value, .. } if *value == Bits::Small(0)));
    assert!(matches!(&hi.kind, ExprKind::Int { value, .. } if *value == Bits::Small(4)));
    assert_eq!(body.len(), 1);
    assert!(matches!(body[0], FnStmt::If { .. }));
}

#[test]
fn foreach_range_form_parses_as_module_item() {
    let f = parse_ok(
        "module M {\n  in src: bits[8][4]\n  out lamps: bits[8][4]\n  foreach i in 0..4 {\n    lamps[i] = src[i]\n  }\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!("expected module")
    };
    let fe = m
        .items
        .iter()
        .find_map(|it| match it {
            ModuleItem::ForEach(fe) => Some(fe),
            _ => None,
        })
        .expect("foreach item parsed");
    assert_eq!(fe.var.name, "i");
    assert!(matches!(fe.source, ForEachSource::Range { .. }));
}

#[test]
fn foreach_elements_form_parses_as_module_item() {
    let f = parse_ok(
        "module M {\n  in values: bits[8][8]\n  reg acc: bits[11] = 0\n  foreach v in values {\n  }\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!("expected module")
    };
    let fe = m
        .items
        .iter()
        .find_map(|it| match it {
            ModuleItem::ForEach(fe) => Some(fe),
            _ => None,
        })
        .expect("foreach item parsed");
    assert_eq!(fe.var.name, "v");
    assert!(matches!(&fe.source, ForEachSource::Elements(id) if id.name == "values"));
}

#[test]
fn foreach_parses_inside_on_block() {
    let f = parse_ok(
        "module M {\n  in clk: bit\n  in values: bits[8][8]\n  reg acc: bits[11] = 0\n  on rise(clk) {\n    foreach v in values {\n      acc <- acc\n    }\n  }\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!("expected module")
    };
    let on = m
        .items
        .iter()
        .find_map(|i| match i {
            ModuleItem::On(o) => Some(o),
            _ => None,
        })
        .expect("an `on` block");
    assert!(matches!(&on.body[0], SeqStmt::ForEach { var, .. } if var.name == "v"));
}

#[test]
fn foreach_parses_inside_fn_body() {
    let f = parse_ok(
        "fn f(values: bits[8][8]) -> bits[8] {\n  foreach v in values {\n    return v\n  }\n  0\n}\nmodule M {\n  in a: bits[8][8]\n  out o: bits[8]\n  o = f(a)\n}\n",
    );
    let has_foreach = f.items.iter().any(|it| {
        matches!(it, TopItem::Func(fd) if fd.stmts.iter().any(|s| matches!(s, FnStmt::ForEach { .. })))
    });
    assert!(has_foreach, "expected FnStmt::ForEach in the fn body");
}

#[test]
fn foreach_elements_form_rejects_non_identifier_source() {
    let diags =
        parse_err("module M {\n  in a: bits[8]\n  in b: bits[8]\n  foreach x in a + b {\n  }\n}\n");
    assert!(
        !diags.is_empty(),
        "an expression source must be rejected at parse time"
    );
}
