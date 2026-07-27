use super::*;

#[test]
fn async_reset_parses_with_the_async_flag() {
    let f = parse_ok("module M {\n  clock clk\n  async reset rst\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let is_async = m
        .items
        .iter()
        .find_map(|it| match it {
            ModuleItem::Reset { is_async, .. } => Some(*is_async),
            _ => None,
        })
        .expect("a reset declaration");
    assert!(is_async, "`async reset` should set is_async");
}

#[test]
fn a_plain_reset_is_synchronous() {
    let f = parse_ok("module M {\n  clock clk\n  reset rst\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let is_async = m
        .items
        .iter()
        .find_map(|it| match it {
            ModuleItem::Reset { is_async, .. } => Some(*is_async),
            _ => None,
        })
        .expect("a reset declaration");
    assert!(!is_async, "a plain `reset` stays synchronous");
}

#[test]
fn thamizh_order_on_fall_parses_to_the_fall_edge() {
    // `irakkam(clk) pothu { }` — the thamizh-order falling-edge block.
    let f = parse_ok(
        "ilakkanam thamizh\nthoguthi M {\n  thudippu clk\n  meettamai rst\n  pathivedu r: bits[8] = 0\n  irakkam(clk) pothu {\n    r <- r +% 1\n  }\n}\n",
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
}

#[test]
fn thamizh_order_on_block_parses_to_the_same_shape() {
    // `syntax thamizh` + the flipped clocked block `yetram(clk) pothu { }`
    // must build the SAME module as the code-order counter: 6 items, an
    // `on` block clocked by `clk`. The directive leaves no trace in the AST.
    let f = parse_ok(
        "ilakkanam thamizh\nthoguthi Counter(WIDTH: int = 8) {\n  thudippu clk\n  meettamai rst\n  veliyeedu count: bits[WIDTH]\n  pathivedu value: bits[WIDTH] = 0\n  yetram(clk) pothu {\n    value <- value +% 1\n  }\n  count = value\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    assert_eq!(m.items.len(), 6);
    let on = m
        .items
        .iter()
        .find_map(|it| match it {
            ModuleItem::On(o) => Some(o),
            _ => None,
        })
        .expect("the flipped block must still parse as an `on` block");
    assert_eq!(on.clock.name, "clk");
    assert_eq!(
        on.body.len(),
        1,
        "the body (`value <- value +% 1`) survives"
    );
}

#[test]
fn english_syntax_thamizh_directive_also_selects_the_profile() {
    // Keyword flavor and word-order profile are orthogonal: the English
    // spelling `syntax thamizh` selects the same profile as `ilakkanam thamizh`.
    let f = parse_ok(
        "syntax thamizh\nmodule M {\n  clock clk\n  reg r: bit = 0\n  rise(clk) on {\n    r <- r\n  }\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    assert!(m.items.iter().any(|it| matches!(it, ModuleItem::On(_))));
}

#[test]
fn unknown_syntax_profile_is_e1112() {
    let d = parse_err("syntax wibble\nmodule M {\n  in a: bit\n}\n");
    assert!(d.iter().any(|e| e.code == Some("E1112")));
}

#[test]
fn flipped_on_block_needs_the_directive() {
    // Without `syntax thamizh`, a leading `rise(...)` is not a valid item.
    parse_err("module M {\n  clock clk\n  reg r: bit = 0\n  rise(clk) on {\n    r <- r\n  }\n}\n");
}

#[test]
fn thamizh_order_test_header_parses_to_the_same_shape() {
    // `syntax thamizh` + the flipped test header `M(args) kaaga "…" sodhanai { }`
    // must build the SAME `TestDecl` as the code-order `test "…" for M(args) { }`:
    // same name, module, args, and body. The clause heads trail the module.
    let f = parse_ok(
        "syntax thamizh\nCounter(WIDTH: 4) kaaga \"counts up\" sodhanai {\n  \
         rst = 0\n  tick(clk)\n  expect count == 1\n}\n",
    );
    let TopItem::Test(t) = &f.items[0] else {
        panic!("expected a test decl")
    };
    assert_eq!(t.name, "counts up");
    assert_eq!(t.module.name.name, "Counter");
    assert_eq!(t.args.len(), 1);
    assert_eq!(t.args[0].name.name, "WIDTH");
    assert_eq!(t.body.len(), 3); // drive, tick, expect
}

#[test]
fn thamizh_test_header_with_no_params_parses() {
    let f = parse_ok(
        "syntax thamizh\nCounter kaaga \"runs\" sodhanai {\n  tick(clk)\n  expect count == 0\n}\n",
    );
    let TopItem::Test(t) = &f.items[0] else {
        panic!("expected a test decl")
    };
    assert_eq!(t.module.name.name, "Counter");
    assert!(t.args.is_empty());
}

#[test]
fn the_test_header_flip_needs_the_directive() {
    // Without `syntax thamizh`, a leading identifier at file level is not a
    // valid item (a code-order test must start with `test`).
    let d = parse_err("Counter kaaga \"runs\" sodhanai {\n  tick(clk)\n}\n");
    assert!(d.iter().any(|e| e.code == Some("E1102")));
}
