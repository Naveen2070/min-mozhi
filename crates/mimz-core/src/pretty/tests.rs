#[test]
fn assert_stmt_round_trips_in_a_module_body() {
    use crate::ast::{AssertStmt, Dir, Ident, Module, ModuleItem, Type};
    use crate::span::Span;

    let f = crate::ast::File {
        imports: vec![],
        items: vec![crate::ast::TopItem::Module(Module {
            name: Ident {
                name: "M".to_string(),
                span: Span::default(),
            },
            params: vec![],
            items: vec![
                ModuleItem::Port {
                    dir: Dir::In,
                    name: Ident {
                        name: "a".to_string(),
                        span: Span::default(),
                    },
                    ty: Type::Bit,
                },
                ModuleItem::Assert(AssertStmt {
                    cond: crate::ast::Expr {
                        kind: crate::ast::ExprKind::Ident("a".to_string()),
                        span: Span::default(),
                    },
                    msg: None,
                    span: Span::default(),
                }),
            ],
            span: Span::default(),
        })],
    };
    let out = crate::pretty::pretty_print(
        &f,
        crate::lexer::token::Flavor::English,
        crate::pretty::Order::Code,
    );
    assert!(out.contains("assert(a)"), "got:\n{out}");
}

/// Regression for a real `pretty_roundtrip` fuzz crash (2026-08-05,
/// artifact `crash-584b58e21628765f4798fe285c32ffd5c0775dac`): an
/// `assert`'s message containing a raw control byte (`\u{4}` here, a
/// literal Rust escape — NOT a min-mozhi one) silently changed on a
/// print → re-lex round trip. Root cause: the pretty-printer quoted
/// string LITERAL VALUES (already-lexed `TokKind::Str` content — assert/
/// cover messages, `test` names, `sim { bind }` string args, `extern
/// module`'s alias/`doc:`) with Rust's `{:?}` Debug formatting, which
/// escapes control bytes as `\u{XXXX}` — but the lexer's own string
/// grammar (`lexer/mod.rs`'s `string()`) has NO escape-sequence support
/// at all, so re-lexing `\u{4}` reads back 6 literal characters instead
/// of decoding them, changing the string's actual content. Fixed by a
/// shared `Pretty::quote` helper that wraps in plain double quotes with
/// no escaping — provably safe because a successfully-lexed `Str` token
/// can never contain `"` or `\n` in the first place (the lexer stops at
/// the first unescaped `"` or a raw newline), so nothing to escape.
#[test]
fn assert_message_with_a_control_byte_round_trips_byte_identical_verilog() {
    let src = format!(
        "module M {{\n  clock clk\n  reset rst\n  out y: bit\n  reg r: bit = 0\n  \
         on rise(clk) {{\n    assert(r == 0, \"has a {} control byte\")\n    r <- 1\n  }}\n  y = r\n}}\n",
        '\u{4}'
    );

    let toks = crate::lexer::lex(&src).expect("lexes");
    let file = crate::parser::parse(toks).expect("parses");
    let printed = crate::pretty::pretty_print(
        &file,
        crate::lexer::token::Flavor::English,
        crate::pretty::Order::Code,
    );

    // The printed source must re-lex/re-parse...
    let toks2 = crate::lexer::lex(&printed).expect("pretty output must lex");
    let file2 = crate::parser::parse(toks2).expect("pretty output must parse");

    // ...and the control byte must have survived, unchanged, in the
    // re-parsed AST — not turned into 6 literal `\`/`u`/`{`/`4`/`}` chars.
    let crate::ast::TopItem::Module(m) = &file2.items[0] else {
        panic!("expected a module")
    };
    let crate::ast::ModuleItem::On(on) = &m.items[4] else {
        panic!("expected the on-block")
    };
    let crate::ast::SeqStmt::Assert(a) = &on.body[0] else {
        panic!("expected the assert")
    };
    assert_eq!(
        a.msg.as_deref(),
        Some("has a \u{4} control byte"),
        "the control byte must round-trip unchanged, got:\n{printed}"
    );
}

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
