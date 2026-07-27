use super::*;

#[test]
fn tagged_enum_parses() {
    // note: parser-only test; checker rejects payload types (T3)
    let f = parse_ok(
        "enum Packet {\n  Read(addr: bits[32]),\n  Write(addr: bits[32], data: bits[32])\n}\n",
    );
    let TopItem::Enum(e) = &f.items[0] else {
        panic!("expected enum")
    };
    assert_eq!(e.name.name, "Packet");
    assert_eq!(e.variants.len(), 2);
    assert_eq!(e.variants[0].name.name, "Read");
    assert_eq!(e.variants[0].fields.len(), 1);
    assert_eq!(e.variants[0].fields[0].name.name, "addr");
    assert_eq!(e.variants[1].name.name, "Write");
    assert_eq!(e.variants[1].fields.len(), 2);
}

#[test]
fn mixed_tag_only_and_tagged_parses() {
    let f = parse_ok("enum X {\n  Empty,\n  Full(v: bits[8])\n}\n");
    let TopItem::Enum(e) = &f.items[0] else {
        panic!("expected enum")
    };
    assert_eq!(e.variants[0].name.name, "Empty");
    assert_eq!(e.variants[0].fields.len(), 0);
    assert_eq!(e.variants[1].name.name, "Full");
    assert_eq!(e.variants[1].fields.len(), 1);
    assert_eq!(e.variants[1].fields[0].name.name, "v");
}

#[test]
fn match_with_payload_bindings_parses() {
    let f = parse_ok(
        "enum Packet { Read(addr: bits[32]) }\nmodule M {\n  in x: bits[32]\n  out y: bits[32]\n  y = match x {\n    Packet.Read(a) => a\n    _ => 0\n  }\n}\n",
    );
    let TopItem::Module(m) = f
        .items
        .iter()
        .find(|i| matches!(i, TopItem::Module(_)))
        .unwrap()
    else {
        panic!()
    };
    let drive = m
        .items
        .iter()
        .find_map(|i| match i {
            ModuleItem::Drive { rhs, .. } => Some(rhs),
            _ => None,
        })
        .expect("Drive item");
    let ExprKind::Match { arms, .. } = &drive.kind else {
        panic!("expected match")
    };
    let Pattern::Variant { bindings, .. } = &arms[0].patterns[0] else {
        panic!("expected variant pattern")
    };
    assert_eq!(bindings.len(), 1, "expected 1 binding");
    assert_eq!(bindings[0].name, "a");
}

#[test]
fn enum_construct_parses_with_payload_args() {
    let f = parse_ok(
        "enum Packet {\n  Ctrl(k: bits[4]),\n  Data(v: bits[8])\n}\n\
         module M {\n  in k: bits[4]\n  out y: Packet\n  y = Packet.Ctrl(k)\n}\n",
    );
    let TopItem::Module(m) = &f.items[1] else {
        panic!("expected module")
    };
    let ModuleItem::Drive { rhs, .. } = m
        .items
        .iter()
        .find(|i| matches!(i, ModuleItem::Drive { .. }))
        .expect("expected a drive")
    else {
        unreachable!()
    };
    let ExprKind::EnumConstruct {
        enum_name,
        variant,
        args,
    } = &rhs.kind
    else {
        panic!("expected EnumConstruct, got {:?}", rhs.kind)
    };
    assert_eq!(enum_name.name, "Packet");
    assert_eq!(variant.name, "Ctrl");
    assert_eq!(args.len(), 1);
}

#[test]
fn enum_construct_parses_with_zero_args_for_tag_only_variant() {
    let f = parse_ok(
        "enum State {\n  Idle,\n  Running\n}\n\
         module M {\n  out y: State\n  y = State.Idle()\n}\n",
    );
    let TopItem::Module(m) = &f.items[1] else {
        panic!("expected module")
    };
    let ModuleItem::Drive { rhs, .. } = m
        .items
        .iter()
        .find(|i| matches!(i, ModuleItem::Drive { .. }))
        .expect("expected a drive")
    else {
        unreachable!()
    };
    let ExprKind::EnumConstruct { args, .. } = &rhs.kind else {
        panic!("expected EnumConstruct, got {:?}", rhs.kind)
    };
    assert_eq!(args.len(), 0);
}

#[test]
fn bare_enum_variant_reference_still_parses_as_field() {
    // Regression: `State.Idle` with NO trailing `(...)` must keep parsing
    // as the existing `ExprKind::Field`, not be swept into EnumConstruct.
    let f = parse_ok(
        "enum State {\n  Idle,\n  Running\n}\n\
         module M {\n  out y: State\n  y = State.Idle\n}\n",
    );
    let TopItem::Module(m) = &f.items[1] else {
        panic!("expected module")
    };
    let ModuleItem::Drive { rhs, .. } = m
        .items
        .iter()
        .find(|i| matches!(i, ModuleItem::Drive { .. }))
        .expect("expected a drive")
    else {
        unreachable!()
    };
    assert!(matches!(rhs.kind, ExprKind::Field { .. }));
}
