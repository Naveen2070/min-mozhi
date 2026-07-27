use super::*;

#[test]
fn parses_fn_with_local_let_and_body() {
    let f =
        parse_ok("fn mac(a: bits[8], b: bits[8]) -> bits[16] {\n let p = a *% b\n extend(p,16) }");
    let TopItem::Func(fd) = &f.items[0] else {
        panic!("not a func")
    };
    assert_eq!(fd.name.name, "mac");
    assert_eq!(fd.params.len(), 2);
    assert_eq!(
        fd.stmts
            .iter()
            .filter(|s| matches!(s, FnStmt::Let(_)))
            .count(),
        1
    );
    assert!(matches!(fd.ret, Type::Bits(_)));
}

#[test]
fn parses_fn_with_guard_clause_return() {
    let f = parse_ok(
        "fn find_first(a: bits[8]) -> int {\n  if a[0] == 1 { return 0 }\n  if a[1] == 1 { return 1 }\n  -1\n}",
    );
    let TopItem::Func(fd) = &f.items[0] else {
        panic!("not a func")
    };
    assert_eq!(fd.stmts.len(), 2);
    assert!(matches!(fd.stmts[0], FnStmt::If { .. }));
    let FnStmt::If { then, els, .. } = &fd.stmts[0] else {
        panic!("expected If")
    };
    assert_eq!(then.len(), 1);
    assert!(matches!(then[0], FnStmt::Return(_)));
    assert!(els.is_none());
}

#[test]
fn parses_fn_with_thamizh_order_guard_clause_return() {
    // thamizh word order: condition precedes `enil` (Kw::If); body is a
    // STATEMENT block (`{ thirumbu ... }`), not a value expression — this is
    // exactly the shape that broke before this fix (it used to be
    // misparsed as an expression-level if, which requires `{ expr }` and a
    // mandatory else). `return`/`thirumbu` is prefix-keyword-only in BOTH
    // word orders (spec/04 section 3 lists only 5 clause-level flips, and
    // `return` is not one of them), so the branch body stays `thirumbu 0`,
    // not `0 thirumbu` — only the `<cond> enil` conditional flips.
    let f = parse_ok(
        "syntax thamizh\nfn find_first(a: bits[8]) -> int {\n  a[0] == 1 enil { thirumbu 0 }\n  a[1] == 1 enil { thirumbu 1 }\n  -1\n}",
    );
    let TopItem::Func(fd) = &f.items[0] else {
        panic!("not a func")
    };
    assert_eq!(fd.stmts.len(), 2);
    assert!(matches!(fd.stmts[0], FnStmt::If { .. }));
    let FnStmt::If { then, els, .. } = &fd.stmts[0] else {
        panic!("expected If")
    };
    assert_eq!(then.len(), 1);
    assert!(matches!(then[0], FnStmt::Return(_)));
    assert!(els.is_none());
}

#[test]
fn parses_fn_with_if_else_stmt() {
    let f = parse_ok(
        "fn abs(a: signed[8]) -> signed[8] {\n  if a < 0 { return 0 -% a } else { return a }\n  0\n}",
    );
    let TopItem::Func(fd) = &f.items[0] else {
        panic!("not a func")
    };
    let FnStmt::If { els, .. } = &fd.stmts[0] else {
        panic!("expected If")
    };
    assert!(els.is_some());
}

#[test]
fn parses_fn_with_only_locals_and_tail_backward_compat() {
    // Every pre-existing `fn` (locals + tail expr, no `if`/`return`) must
    // still parse — this is the backward-compatibility contract from the
    // design spec.
    let f =
        parse_ok("fn mac(a: bits[8], b: bits[8]) -> bits[16] {\n let p = a *% b\n extend(p,16) }");
    let TopItem::Func(fd) = &f.items[0] else {
        panic!("not a func")
    };
    assert_eq!(fd.stmts.len(), 1);
    assert!(matches!(fd.stmts[0], FnStmt::Let(_)));
}
