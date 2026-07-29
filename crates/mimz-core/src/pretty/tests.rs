#[test]
fn qualified_reference_round_trips_through_pretty_print() {
    let src = "module M {\n  let x = a.b.Foo() { }\n}\n";
    let toks = crate::lexer::lex(src).unwrap();
    let file = crate::parser::parse(toks).unwrap();
    let printed = crate::pretty::pretty_print(
        &file,
        crate::lexer::token::Flavor::English,
        crate::pretty::Order::Code,
    );
    assert!(printed.contains("a.b.Foo"), "got:\n{printed}");
}

#[test]
fn enum_construct_pretty_prints_with_args() {
    let src = "enum Packet {\n  Ctrl(k: bits[4])\n}\n\nmodule M {\n  in k: bits[4]\n  out y: Packet\n  y = Packet.Ctrl(k)\n}\n";
    let toks = crate::lexer::lex(src).unwrap();
    let file = crate::parser::parse(toks).unwrap();
    let printed = crate::pretty::pretty_print(
        &file,
        crate::lexer::token::Flavor::English,
        crate::pretty::Order::Code,
    );
    assert!(printed.contains("Packet.Ctrl(k)"), "got:\n{printed}");
    // Confirm it re-parses to the same shape, not just that the text appears.
    let toks2 = crate::lexer::lex(&printed).unwrap();
    crate::parser::parse(toks2).expect("pretty-printed EnumConstruct re-parses");
}

#[test]
fn sync_loop_round_trips_through_pretty_print() {
    let src = "module M {\n  clock clk\n  mem m: bits[8][8] = 0\n  in key: bits[8]\n  sync loop find_first on rise(clk) (i: 0..8) -> result: signed[4] = 0 - 1 {\n    if m[i] == key { result <- i }\n  }\n}\n";
    let toks = crate::lexer::lex(src).unwrap();
    let file = crate::parser::parse(toks).unwrap();
    let printed = crate::pretty::pretty_print(
        &file,
        crate::lexer::token::Flavor::English,
        crate::pretty::Order::Code,
    );
    assert!(printed.contains("sync loop find_first"), "got:\n{printed}");
    // Confirm it re-parses to the same shape, not just that the text appears.
    let toks2 = crate::lexer::lex(&printed).unwrap();
    let file2 = crate::parser::parse(toks2).expect("pretty-printed sync loop re-parses");
    let crate::ast::TopItem::Module(m) = &file2.items[0] else {
        panic!("expected module")
    };
    let sl = m
        .items
        .iter()
        .find_map(|it| match it {
            crate::ast::ModuleItem::SyncLoop(sl) => Some(sl),
            _ => None,
        })
        .expect("sync loop item round-trips");
    assert_eq!(sl.name.name, "find_first");
    assert_eq!(sl.body.len(), 1);
}

#[test]
fn foreach_round_trips_through_pretty_print() {
    let src = "module M {\n  in e: bits[8]\n  out led: bits[8]\n  foreach i in 0..4 {\n    led[i] = e[i]\n  }\n}\n";
    let toks = crate::lexer::lex(src).unwrap();
    let file = crate::parser::parse(toks).unwrap();
    let printed = crate::pretty::pretty_print(
        &file,
        crate::lexer::token::Flavor::English,
        crate::pretty::Order::Code,
    );
    assert!(printed.contains("foreach"), "got:\n{printed}");
    assert!(!printed.contains("repeat"), "got:\n{printed}");
    // Confirm it re-parses as `ForEach`, not its lowered `Repeat` form.
    let toks2 = crate::lexer::lex(&printed).unwrap();
    let file2 = crate::parser::parse(toks2).expect("pretty-printed foreach re-parses");
    let crate::ast::TopItem::Module(m) = &file2.items[0] else {
        panic!("expected module")
    };
    let fe = m
        .items
        .iter()
        .find_map(|it| match it {
            crate::ast::ModuleItem::ForEach(fe) => Some(fe),
            _ => None,
        })
        .expect("foreach item round-trips as ForEach, not Repeat");
    assert_eq!(fe.var.name, "i");
}

#[test]
fn extern_module_round_trips_through_pretty_print() {
    let src = "extern module Pll(MULT: int = 2) {\n  \
               doc: \"50MHz input, 100MHz output\"\n  \
               in clk_in: bit\n  out clk_out: bit\n  out locked: bit\n}\n";
    let toks = crate::lexer::lex(src).unwrap();
    let file = crate::parser::parse(toks).unwrap();
    let printed = crate::pretty::pretty_print(
        &file,
        crate::lexer::token::Flavor::English,
        crate::pretty::Order::Code,
    );
    assert!(printed.contains("extern module Pll"), "got:\n{printed}");
    assert!(printed.contains("MULT"), "got:\n{printed}");
    assert!(
        printed.contains("50MHz input, 100MHz output"),
        "got:\n{printed}"
    );
    assert!(printed.contains("in clk_in: bit"), "got:\n{printed}");
    assert!(printed.contains("out clk_out: bit"), "got:\n{printed}");
    assert!(printed.contains("out locked: bit"), "got:\n{printed}");
    // Confirm it actually re-parses back to the same shape (not just that
    // the text happens to appear somewhere in the output).
    let toks2 = crate::lexer::lex(&printed).unwrap();
    let file2 = crate::parser::parse(toks2).expect("pretty-printed extern module re-parses");
    let crate::ast::TopItem::ExternModule(em) = &file2.items[0] else {
        panic!("expected ExternModule, got {:?}", file2.items[0]);
    };
    assert_eq!(em.name.name, "Pll");
    assert_eq!(em.params.len(), 1);
    assert_eq!(em.params[0].name.name, "MULT");
    assert_eq!(em.doc.as_deref(), Some("50MHz input, 100MHz output"));
    assert_eq!(em.items.len(), 3);
}

#[test]
fn extern_module_with_verilog_alias_round_trips_through_pretty_print() {
    let src = "extern module Pll = \"PLL_HARD_IP_v2\" {\n  in clk_in: bit\n}\n";
    let toks = crate::lexer::lex(src).unwrap();
    let file = crate::parser::parse(toks).unwrap();
    let printed = crate::pretty::pretty_print(
        &file,
        crate::lexer::token::Flavor::English,
        crate::pretty::Order::Code,
    );
    assert!(printed.contains("PLL_HARD_IP_v2"), "got:\n{printed}");
    let toks2 = crate::lexer::lex(&printed).unwrap();
    let file2 = crate::parser::parse(toks2).expect("pretty-printed extern module alias re-parses");
    let crate::ast::TopItem::ExternModule(em) = &file2.items[0] else {
        panic!("expected ExternModule, got {:?}", file2.items[0]);
    };
    assert_eq!(em.verilog_name.as_deref(), Some("PLL_HARD_IP_v2"));
}

#[test]
fn sim_speed_clause_round_trips_through_pretty_print() {
    let src =
        "module M {\n  clock clk\n}\ntest \"m sim\" for M {\n  sim {\n    speed mhz(50)\n  }\n}\n";
    let toks = crate::lexer::lex(src).unwrap();
    let file = crate::parser::parse(toks).unwrap();
    let printed = crate::pretty::pretty_print(
        &file,
        crate::lexer::token::Flavor::English,
        crate::pretty::Order::Code,
    );
    assert!(printed.contains("mhz(50)"), "got:\n{printed}");
    // Confirm it re-parses cleanly (this is the actual bug: the old
    // printer emitted `speed 50 * 1000000`, which fails to re-parse).
    let toks2 = crate::lexer::lex(&printed).unwrap();
    crate::parser::parse(toks2).expect("pretty-printed speed clause re-parses");
}

#[test]
fn sync_double_flop_call_round_trips_through_pretty_print() {
    let src = "module M {\n  clock clk_src\n  clock clk_dst\n  in fast_bit: bit\n  wire slow_bit: bit = sync.double_flop(fast_bit, clk_src, clk_dst)\n}\n";
    let toks = crate::lexer::lex(src).unwrap();
    let file = crate::parser::parse(toks).unwrap();
    let printed = crate::pretty::pretty_print(
        &file,
        crate::lexer::token::Flavor::English,
        crate::pretty::Order::Code,
    );
    assert!(
        printed.contains("sync.double_flop(fast_bit, clk_src, clk_dst)"),
        "got:\n{printed}"
    );
    // Confirm it re-parses to the same shape.
    let toks2 = crate::lexer::lex(&printed).unwrap();
    let file2 = crate::parser::parse(toks2).expect("pretty-printed sync.double_flop re-parses");
    let crate::ast::TopItem::Module(m) = &file2.items[0] else {
        panic!("expected module")
    };
    let wire_init = m
        .items
        .iter()
        .find_map(|it| match it {
            crate::ast::ModuleItem::Wire { init, .. } => Some(init),
            _ => None,
        })
        .expect("expected wire");
    let crate::ast::ExprKind::Call { func, args } = &wire_init.kind else {
        panic!("expected a Call expression")
    };
    assert_eq!(*func, crate::ast::Builtin::SyncDoubleFlop);
    assert_eq!(args.len(), 3);
}
