use super::*;

#[test]
fn fn_decl_parses_in_thamizh_order() {
    // `fn` declarations are code-order-only (no SOV flip needed — they are
    // prefix-keyword constructs, not clause-inverted). A file with
    // `syntax thamizh` must still accept a leading `fn` declaration.
    let f = parse_ok(
        "syntax thamizh\nfn square(x: bits[8]) -> bits[16] {\n  x * x\n}\nmodule M {\n  ulleedu x: bits[8]\n  veliyeedu y: bits[16]\n  y = square(x)\n}\n",
    );
    let func = f
        .items
        .iter()
        .find_map(|i| match i {
            TopItem::Func(f) => Some(f),
            _ => None,
        })
        .expect("fn declaration must parse under syntax thamizh");
    assert_eq!(func.name.name, "square");
    assert_eq!(func.params.len(), 1);
}

#[test]
fn parse_default_stmt() {
    let f = parse_ok(
        "module M {\n  clock clk\n  reg done: bit = 0\n  on rise(clk) { default done <- 0 }\n}",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!("expected module")
    };
    let on = m
        .items
        .iter()
        .find_map(|i| match i {
            ModuleItem::On(b) => Some(b),
            _ => None,
        })
        .expect("no on block");
    assert!(matches!(on.body[0], SeqStmt::Default { .. }));
}

#[test]
fn parse_const_if_block() {
    // `const if (N > 4) { wire w: bit = 0 }` must parse to `ModuleItem::ConstIf`
    // with the then-branch containing one item, and no else branch.
    let f = parse_ok("module M(N: int = 8) {\n  const if (N > 4) { wire w: bit = 0 }\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!("expected module")
    };
    let ci = m
        .items
        .iter()
        .find_map(|i| match i {
            ModuleItem::ConstIf {
                cond, then, els, ..
            } => Some((cond, then, els)),
            _ => None,
        })
        .expect("no ConstIf item found");
    assert_eq!(ci.1.len(), 1, "then-branch must have one item");
    assert!(ci.2.is_none(), "no else branch");

    // With an else branch:
    let f2 = parse_ok(
        "module M(N: int = 8) {\n  const if (N > 4) { wire w: bit = 0 } else { wire v: bit = 1 }\n}\n",
    );
    let TopItem::Module(m2) = &f2.items[0] else {
        panic!("expected module")
    };
    let ci2 = m2
        .items
        .iter()
        .find_map(|i| match i {
            ModuleItem::ConstIf { then, els, .. } => Some((then, els)),
            _ => None,
        })
        .expect("no ConstIf item found");
    assert_eq!(ci2.0.len(), 1, "then-branch has one item");
    assert!(ci2.1.is_some(), "else branch present");
    assert_eq!(ci2.1.as_ref().unwrap().len(), 1, "else-branch has one item");
}
