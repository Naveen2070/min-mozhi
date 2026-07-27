use super::*;

#[test]
fn parse_bundle_decl() {
    let src = r#"
bundle MemBus(WIDTH: int = 32) {
  valid: bit
  data: bits[WIDTH]
}
"#;
    let file = parse_ok(src);
    let TopItem::Bundle(b) = &file.items[0] else {
        panic!("expected Bundle")
    };
    assert_eq!(b.name.name, "MemBus");
    assert_eq!(b.params.len(), 1);
    assert_eq!(b.params[0].name.name, "WIDTH");
    assert_eq!(b.fields.len(), 2);
    assert_eq!(b.fields[0].name.name, "valid");
    assert!(matches!(b.fields[0].ty, Type::Bit));
    assert_eq!(b.fields[1].name.name, "data");
    assert!(matches!(b.fields[1].ty, Type::Bits(_)));
}

#[test]
fn parse_bundle_as_port_type() {
    let src = r#"
bundle Hs { valid: bit, ready: bit }
module Top {
  in req: Hs
  out rsp: Hs(X: 1)
}
"#;
    let file = parse_ok(src);
    let TopItem::Module(m) = &file.items[1] else {
        panic!()
    };
    let ModuleItem::Port { ty, .. } = &m.items[0] else {
        panic!()
    };
    assert!(matches!(ty, Type::Named(_) | Type::Bundle { .. }));
}

#[test]
fn parse_bundle_literal() {
    let src = r#"
bundle Hs { valid: bit }
module Top {
  in src: Hs
  out dst: Hs
  dst = { valid: 1 }
}
"#;
    let file = parse_ok(src);
    let TopItem::Module(m) = &file.items[1] else {
        panic!()
    };
    let ModuleItem::Drive { rhs, .. } = &m.items[2] else {
        panic!()
    };
    assert!(matches!(rhs.kind, ExprKind::BundleLit(_)));
}

#[test]
fn parse_bundle_destructure() {
    let src = r#"
bundle Hs { valid: bit }
module Top {
  in bus: Hs
  let { valid } = bus
}
"#;
    let file = parse_ok(src);
    let TopItem::Module(m) = &file.items[1] else {
        panic!()
    };
    let ModuleItem::BundleDestructure { bindings, .. } = &m.items[1] else {
        panic!("expected BundleDestructure, got {:?}", m.items[1])
    };
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].name, "valid");
}

#[test]
fn parse_bundle_field_rename_is_error() {
    // `let { valid: v } = bus` must give E0904, not silently parse
    let src = r#"
bundle Hs { valid: bit }
module Top {
  in bus: Hs
  let { valid: v } = bus
}
"#;
    let errs = parse_err(src);
    assert!(
        errs.iter().any(|e| e.code == Some("E0904")),
        "expected E0904, got: {:?}",
        errs
    );
}
