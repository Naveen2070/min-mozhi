use super::*;

#[test]
fn parses_test_block() {
    parse_ok(
        "test \"counts\" for Counter(WIDTH: 4) {\n  tick(clk)\n  expect count == 1\n  tick(clk, 3)\n  expect count == 4\n}\n",
    );
}

#[test]
fn empty_parens_variant_is_a_parse_error() {
    // "A()" should be rejected — tag-only variants have no parens (D1)
    let d = parse_err("enum Foo { A() }\nmodule M { out y: bit\n  y = 0\n}\n");
    assert_eq!(d[0].code, Some("E1113"));
    assert!(
        d[0].msg.contains("tag-only"),
        "expected tag-only hint in message"
    );
}

// ---- error recovery: `parse_recover` yields a best-effort tree with `Error`
// placeholder nodes so the LSP can offer semantics on half-typed source.

fn parse_recover_str(src: &str) -> (File, Vec<Diag>) {
    parse_recover(lex(src).expect("lex error"))
}

#[test]
fn parse_recover_keeps_good_items_around_a_bad_one() {
    // A broken line between two valid ports must not swallow either port:
    // recovery records a single `ModuleItem::Error` and parsing continues.
    let (f, diags) =
        parse_recover_str("module M {\n  in a: bits[4]\n  1 2 3\n  out y: bits[4]\n}\n");
    assert!(
        !diags.is_empty(),
        "the bad line must still produce a diagnostic"
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!("expected the module to parse")
    };
    let ports = m
        .items
        .iter()
        .filter(|i| matches!(i, ModuleItem::Port { .. }))
        .count();
    assert_eq!(ports, 2, "both ports survive the broken line in between");
    let errors = m
        .items
        .iter()
        .filter(|i| matches!(i, ModuleItem::Error(_)))
        .count();
    assert_eq!(errors, 1, "exactly one Error placeholder for the bad line");
}

#[test]
fn parse_recover_top_level_error_keeps_following_module() {
    // Garbage at file level is recorded as a `TopItem::Error`; the valid
    // module after it still parses.
    let (f, diags) = parse_recover_str("garbage here\nmodule Good {\n  out y: bit\n  y = 0\n}\n");
    assert!(!diags.is_empty());
    assert!(
        f.items
            .iter()
            .any(|i| matches!(i, TopItem::Module(m) if m.name.name == "Good")),
        "the module following the garbage must parse"
    );
    assert!(
        f.items.iter().any(|i| matches!(i, TopItem::Error(_))),
        "the garbage line is an Error placeholder"
    );
}

#[test]
fn parse_recover_seq_and_test_blocks_emit_error_nodes() {
    // `on` block: a bad statement becomes `SeqStmt::Error`; the assign survives.
    let (f, _) = parse_recover_str(
        "module M {\n  clock clk\n  reg r: bits[8] = 0\n  on rise(clk) {\n    1 2 3\n    r <- r +% 1\n  }\n}\n",
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
    assert!(on.body.iter().any(|s| matches!(s, SeqStmt::Error(_))));
    assert!(on.body.iter().any(|s| matches!(s, SeqStmt::Assign { .. })));

    // `test` block: a bad statement becomes `TestStmt::Error`; tick survives.
    let (f, _) = parse_recover_str("test \"t\" for M {\n  1 2 3\n  tick(clk)\n}\n");
    let TopItem::Test(t) = &f.items[0] else {
        panic!()
    };
    assert!(t.body.iter().any(|s| matches!(s, TestStmt::Error(_))));
    assert!(t.body.iter().any(|s| matches!(s, TestStmt::Tick { .. })));
}

#[test]
fn sim_block_parses() {
    let src = "test \"play\" for M {\n  start = 1\n  sim {\n    speed mhz(50)\n    bind playing -> led(color: green)\n  }\n  tick(clk)\n}\n";
    let f = parse_ok(src);
    let TopItem::Test(t) = &f.items[0] else {
        panic!("expected a TestDecl")
    };
    let sim = t
        .body
        .iter()
        .find_map(|s| match s {
            TestStmt::Sim(s) => Some(s),
            _ => None,
        })
        .expect("a sim block");
    assert!(sim.speed.is_some(), "speed clause must parse");
    assert_eq!(sim.binds.len(), 1);
    assert_eq!(sim.binds[0].port.name, "playing");
    assert_eq!(sim.binds[0].peripheral.name, "led");
    assert_eq!(sim.binds[0].args.len(), 1);
    assert_eq!(sim.binds[0].args[0].name.name, "color");
    assert!(matches!(
        &sim.binds[0].args[0].value,
        BindArgValue::Ident(s) if s == "green"
    ));
}

#[test]
fn sim_block_bind_arg_accepts_a_bare_integer() {
    let src = "test \"tx\" for M {\n  start = 1\n  sim {\n    speed mhz(1)\n    bind tx -> uart_tx(baud: 9600)\n  }\n  tick(clk)\n}\n";
    let f = parse_ok(src);
    let TopItem::Test(t) = &f.items[0] else {
        panic!("expected a TestDecl")
    };
    let sim = t
        .body
        .iter()
        .find_map(|s| match s {
            TestStmt::Sim(s) => Some(s),
            _ => None,
        })
        .expect("a sim block");
    assert_eq!(sim.binds[0].args[0].name.name, "baud");
    assert!(matches!(
        &sim.binds[0].args[0].value,
        BindArgValue::Int(9600)
    ));
}

#[test]
fn sim_block_bad_syntax_recovers() {
    let (f, _) =
        parse_recover_str("test \"t\" for M {\n  sim {\n    nonsense\n  }\n  tick(clk)\n}\n");
    let TopItem::Test(t) = &f.items[0] else {
        panic!()
    };
    assert!(t.body.iter().any(|s| matches!(s, TestStmt::Error(_))));
    assert!(t.body.iter().any(|s| matches!(s, TestStmt::Tick { .. })));
}

#[test]
fn strict_parse_still_errs_on_bad_input() {
    // The strict `parse` contract is unchanged: any error discards the tree.
    assert!(parse(lex("module M {\n  1 2 3\n}\n").expect("lex error")).is_err());
}
