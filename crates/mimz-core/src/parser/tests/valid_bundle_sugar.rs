use super::*;

// ---- `T?` valid-bundle sugar (bit?/bits[N]?/signed[N]?) ----

#[test]
fn bit_question_desugars_to_builtin_valid_bundle() {
    let f = parse_ok("module M {\n  in a: bit?\n  out o: bit\n  o = a.valid\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let ModuleItem::Port { ty, .. } = &m.items[0] else {
        panic!()
    };
    let Type::Bundle { name, args } = ty else {
        panic!("expected Type::Bundle, got {ty:?}")
    };
    assert_eq!(name.name.name, "__Valid");
    assert_eq!(args.len(), 1);
    assert_eq!(args[0].name.name, "N");
    assert!(matches!(&args[0].value.kind, ExprKind::Int { value, .. } if *value == Bits::Small(1)));
}

#[test]
fn bits_n_question_desugars_with_the_width_expr() {
    let f = parse_ok("module M {\n  in a: bits[8]?\n  out o: bit\n  o = a.valid\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let ModuleItem::Port { ty, .. } = &m.items[0] else {
        panic!()
    };
    let Type::Bundle { name, args } = ty else {
        panic!("expected Type::Bundle, got {ty:?}")
    };
    assert_eq!(name.name.name, "__Valid");
    assert!(matches!(&args[0].value.kind, ExprKind::Int { value, .. } if *value == Bits::Small(8)));
}

#[test]
fn signed_n_question_desugars_to_valid_signed() {
    let f = parse_ok("module M {\n  in a: signed[8]?\n  out o: bit\n  o = a.valid\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let ModuleItem::Port { ty, .. } = &m.items[0] else {
        panic!()
    };
    let Type::Bundle { name, args } = ty else {
        panic!("expected Type::Bundle, got {ty:?}")
    };
    assert_eq!(name.name.name, "__ValidSigned");
    assert!(matches!(&args[0].value.kind, ExprKind::Int { value, .. } if *value == Bits::Small(8)));
}

#[test]
fn double_question_on_a_type_is_rejected() {
    let d = parse_err("module M {\n  in a: bits[8]??\n  out o: bit\n}\n");
    assert!(
        d.iter().any(|x| x.code == Some("E1115")),
        "bits[8]?? must be a parse error, not silently accepted: {d:?}"
    );
}

#[test]
fn mem_declaration_still_parses_to_the_same_shape_after_array_type_grammar_lands() {
    // Regression: mem's OWN declaration grammar (`mem name: elem[DEPTH] =
    // init`) must parse to the EXACT SAME ModuleItem::Mem shape as before
    // this plan — `ty` a scalar Bits/Signed/Bit, `depth` a separate Expr.
    // This is the load-bearing backward-compat test for this task.
    let f = parse_ok("module M {\n  mem m: bits[8][4] = 0\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let (ty, depth) = m
        .items
        .iter()
        .find_map(|it| match it {
            ModuleItem::Mem { ty, depth, .. } => Some((ty, depth)),
            _ => None,
        })
        .expect("a `mem` declaration");
    assert!(
        matches!(ty, Type::Bits(_)),
        "mem's element type must stay a scalar Type::Bits, not become Type::Array — got {ty:?}"
    );
    // `depth` is still a plain Expr (the `4`), not folded into `ty`.
    assert!(matches!(&depth.kind, ExprKind::Int { value, .. } if *value == Bits::Small(4)));
}
