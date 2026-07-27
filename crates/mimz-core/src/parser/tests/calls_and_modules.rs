use super::*;

#[test]
fn builtin_with_wrong_arity_is_e1110() {
    // `min` takes two arguments; calling it with one is a parse-time arity error.
    // Builtin arity is still checked at parse time (E1110 stays a parser code).
    let d = parse_err("module M {\n  in a: bits[4]\n  out y: bits[4]\n  y = min(a)\n}\n");
    assert_eq!(d[0].code, Some("E1110"));
}

#[test]
fn non_builtin_call_parses_as_fncall() {
    let e = parse_expr_ok("mac(x, y)");
    let ExprKind::FnCall { name, args } = e.kind else {
        panic!("not FnCall: {:?}", e.kind)
    };
    assert_eq!(name.name, "mac");
    assert_eq!(args.len(), 2);
}

#[test]
fn builtin_call_still_parses_as_builtin() {
    let e = parse_expr_ok("extend(x, 8)");
    assert!(matches!(
        e.kind,
        ExprKind::Call {
            func: Builtin::Extend,
            ..
        }
    ));
}

#[test]
fn zero_arg_call_parses_as_fncall() {
    let e = parse_expr_ok("foo()");
    let ExprKind::FnCall { name, args } = e.kind else {
        panic!("not FnCall: {:?}", e.kind)
    };
    assert_eq!(name.name, "foo");
    assert_eq!(args.len(), 0);
}

#[test]
fn parses_counter() {
    let f = parse_ok(
        "module Counter(WIDTH: int = 8) {\n  clock clk\n  reset rst\n  out count: bits[WIDTH]\n  reg value: bits[WIDTH] = 0\n  on rise(clk) {\n    value <- value +% 1\n  }\n  count = value\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    assert_eq!(m.name.name, "Counter");
    assert_eq!(m.items.len(), 6);
}

#[test]
fn parses_tanglish_counter_to_same_shape() {
    let f = parse_ok(
        "thoguthi Counter(WIDTH: int = 8) {\n  thudippu clk\n  meettamai rst\n  veliyeedu count: bits[WIDTH]\n  pathivedu value: bits[WIDTH] = 0\n  pothu yetram(clk) {\n    value <- value +% 1\n  }\n  count = value\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    assert_eq!(m.name.name, "Counter");
    assert_eq!(m.items.len(), 6);
}

// ---- grammar engine: thamizh-order profile (spec/04, Phase 1.8) ----
