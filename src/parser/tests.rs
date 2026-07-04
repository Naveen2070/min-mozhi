//! Parser unit tests — including the locked-in safety behaviors
//! (precedence trap, latch teaching, `=` vs `<-`).

use super::*;
use crate::ast::{Builtin, ExprKind, FnStmt, ModuleItem, TopItem, Type};
use crate::lexer::lex;

fn parse_ok(src: &str) -> File {
    parse(lex(src).expect("lex error")).expect("parse error")
}

fn parse_err(src: &str) -> Vec<Diag> {
    match parse(lex(src).expect("lex error")) {
        Ok(_) => panic!("expected a parse error"),
        Err(d) => d,
    }
}

/// Parse `expr_src` as a combinational drive RHS inside a minimal module.
/// Wraps in `module M { in a: bits[4]; in x: bits[4]; in y: bits[4]; out z: bits[4]; z = <expr> }`.
fn parse_expr_ok(expr_src: &str) -> crate::ast::Expr {
    let src = format!(
        "module M {{\n  in a: bits[4]\n  in x: bits[4]\n  in y: bits[4]\n  out z: bits[4]\n  z = {expr_src}\n}}\n"
    );
    let f = parse(lex(&src).expect("lex error")).expect("parse error");
    let TopItem::Module(m) = &f.items[0] else {
        panic!("expected module")
    };
    for item in &m.items {
        if let ModuleItem::Drive { rhs, .. } = item {
            return rhs.clone();
        }
    }
    panic!("no Drive item found")
}

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

#[test]
fn on_fall_parses_with_the_fall_edge() {
    let f = parse_ok(
        "module M {\n  clock clk\n  reset rst\n  reg r: bits[8] = 0\n  on fall(clk) {\n    r <- r +% 1\n  }\n}\n",
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
    assert_eq!(on.clock.name, "clk");
}

#[test]
fn mem_declaration_parses_to_a_mem_item() {
    let f = parse_ok("module M {\n  mem m: bits[8][4] = 0\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let mem = m
        .items
        .iter()
        .find_map(|it| match it {
            ModuleItem::Mem {
                name, depth, init, ..
            } => Some((name, depth, init)),
            _ => None,
        })
        .expect("a `mem` declaration");
    assert_eq!(mem.0.name, "m");
}

#[test]
fn a_mem_without_an_init_value_is_e1104() {
    let d = parse_err("module M {\n  mem m: bits[8][4]\n}\n");
    assert_eq!(d[0].code, Some("E1104"));
}

#[test]
fn array_type_parses_in_a_fn_param() {
    let f = parse_ok("fn f(vals: bits[8][4]) -> bits[8] {\n  vals[0]\n}");
    let TopItem::Func(fd) = &f.items[0] else {
        panic!("not a func")
    };
    let Type::Array { elem, len: _ } = &fd.params[0].ty else {
        panic!("expected an array type, got {:?}", fd.params[0].ty)
    };
    assert!(matches!(**elem, Type::Bits(_)));
}

#[test]
fn nested_array_type_parses_two_brackets_deep() {
    // The grammar doesn't reject this (nested arrays are a NON-goal
    // rejected by the CHECKER, not the parser — matches this project's
    // existing house style of "parser is lenient, checker narrows" used
    // elsewhere, e.g. `repeat` bodies parse generally and the checker
    // restricts what's inside). This test only proves the grammar itself
    // is unambiguous for a doubly-bracketed type — it makes no claim
    // about whether the CHECKER accepts it (it won't, once Task 5 lands
    // the non-goal rejection — that's a separate checker test).
    let f = parse_ok("fn f(vals: bits[8][4][2]) -> bits[8] {\n  0\n}");
    let TopItem::Func(fd) = &f.items[0] else {
        panic!("not a func")
    };
    let Type::Array { elem, .. } = &fd.params[0].ty else {
        panic!("expected outer array type")
    };
    assert!(matches!(**elem, Type::Array { .. }));
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
    assert!(matches!(depth.kind, ExprKind::Int { value: 4, .. }));
}

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

#[test]
fn deeply_nested_expression_errors_not_overflows() {
    // Security: a recursive-descent parser with no depth limit stack-overflows
    // (aborts the process) on `(((…)))`. The MAX_DEPTH guard must turn that into
    // a clean E1113. 2000 parens is far past the cap (64) and cheap to parse.
    let src = format!(
        "module M {{\n  out y: bit\n  y = {}1{}\n}}\n",
        "(".repeat(2000),
        ")".repeat(2000)
    );
    let d = parse_err(&src);
    assert!(d.iter().any(|e| e.code == Some("E1113")));
}

#[test]
fn deeply_nested_unary_errors_not_overflows() {
    // The prefix-operator chain `!!!!…x` recurses through `unary`, not `expr` —
    // its own guard must catch it too.
    let src = format!(
        "module M {{\n  out y: bit\n  y = {}1\n}}\n",
        "!".repeat(2000)
    );
    let d = parse_err(&src);
    assert!(d.iter().any(|e| e.code == Some("E1113")));
}

#[test]
fn a_long_flat_binary_chain_parses_without_tripping_the_depth_guard() {
    // `a + a + … + a` is left-associative, parsed ITERATIVELY by the precedence
    // climb (which only recurses by precedence level — a constant). A chain far
    // longer than MAX_DEPTH (64) is flat in nesting depth, so it must parse
    // cleanly: neither a stack overflow nor a spurious E1113. This locks in that
    // chain LENGTH is unbounded and distinct from nesting DEPTH.
    let chain = vec!["a"; 5000].join(" + ");
    let src = format!("module M {{\n  in a: bits[8]\n  out y: bits[8]\n  y = {chain}\n}}\n");
    parse_ok(&src); // succeeds — no panic, no depth error
}

#[test]
fn stray_top_level_brace_does_not_hang() {
    // Regression: a stray `}` at file level (e.g. unbalanced braces from error
    // recovery) once spun `file()` forever — `sync_to_newline` stops at `}`
    // without consuming it. The loop must terminate with an error, not OOM.
    let d = parse_err("module M {\n  out y: bit\n  y = 0\n}\n}\n");
    assert!(d.iter().any(|e| e.code == Some("E1102")));
}

#[test]
fn rust_precedence_defuses_the_c_trap() {
    // x & 1 == 0 must parse as (x & 1) == 0
    let f = parse_ok("module M {\n  in x: bits[8]\n  out y: bit\n  y = x & 1 == 0\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let ModuleItem::Drive { rhs, .. } = &m.items[2] else {
        panic!()
    };
    let ExprKind::Binary { op, .. } = &rhs.kind else {
        panic!()
    };
    assert_eq!(*op, BinOp::Eq, "top of the tree must be `==`, not `&`");
}

#[test]
fn monotonic_chained_comparison_desugars_to_and() {
    // 0 <= x <= 7  →  (0 <= x) && (x <= 7); the shared `x` is read twice
    // (identical combinational value). The safe Python-style form (8.9).
    let f = parse_ok("module M {\n  in x: bits[8]\n  out y: bit\n  y = 0 <= x <= 7\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let ModuleItem::Drive { rhs, .. } = &m.items[2] else {
        panic!()
    };
    let ExprKind::Binary { op, lhs, rhs } = &rhs.kind else {
        panic!()
    };
    assert_eq!(*op, BinOp::LogicAnd, "a chain desugars to &&");
    let ExprKind::Binary { op: lop, .. } = &lhs.kind else {
        panic!("left of && is the first comparison")
    };
    let ExprKind::Binary { op: rop, .. } = &rhs.kind else {
        panic!("right of && is the second comparison")
    };
    assert_eq!(*lop, BinOp::Le);
    assert_eq!(*rop, BinOp::Le);
}

#[test]
fn replication_parses_to_replicate() {
    // `{2{a}}` is replication (count 2, one inner part), NOT concatenation.
    let f = parse_ok("module M {\n  in a: bits[4]\n  out y: bits[8]\n  y = {2{a}}\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let ModuleItem::Drive { rhs, .. } = &m.items[2] else {
        panic!()
    };
    let ExprKind::Replicate { count, parts } = &rhs.kind else {
        panic!("`{{2{{a}}}}` must parse as replication")
    };
    assert!(matches!(&count.kind, ExprKind::Int { value: 2, .. }));
    assert_eq!(parts.len(), 1, "one inner part");
}

#[test]
fn braces_without_an_inner_group_stay_concat() {
    // `{a, a}` is still concatenation — the replication path must not regress it.
    let f = parse_ok("module M {\n  in a: bits[4]\n  out y: bits[8]\n  y = {a, a}\n}\n");
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let ModuleItem::Drive { rhs, .. } = &m.items[2] else {
        panic!()
    };
    assert!(matches!(&rhs.kind, ExprKind::Concat(p) if p.len() == 2));
}

#[test]
fn dont_care_pattern_parses_to_intmask() {
    // `0b1??` in a `match` arm parses as a masked pattern.
    let f = parse_ok(
        "module M {\n  in s: bits[3]\n  out y: bit\n  y = match s {\n    0b1?? => true\n    _ => false\n  }\n}\n",
    );
    let TopItem::Module(m) = &f.items[0] else {
        panic!()
    };
    let ModuleItem::Drive { rhs, .. } = &m.items[2] else {
        panic!()
    };
    let ExprKind::Match { arms, .. } = &rhs.kind else {
        panic!("a match expression")
    };
    assert!(matches!(
        &arms[0].patterns[0],
        Pattern::IntMask {
            value: 0b100,
            mask: 0b100,
            width: 3,
            ..
        }
    ));
}

#[test]
fn mixed_direction_chain_is_an_error() {
    // `a < b > c` is the genuinely confusing form — still rejected.
    let d = parse_err("module M {\n  in a: bit\n  out y: bit\n  y = a < a > a\n}\n");
    assert_eq!(d[0].code, Some("E1109"));
    assert!(d[0].msg.contains("one direction"));
}

#[test]
fn equality_cannot_be_chained() {
    let d = parse_err("module M {\n  in a: bit\n  out y: bit\n  y = a == a == a\n}\n");
    assert_eq!(d[0].code, Some("E1109"));
}

#[test]
fn wire_if_without_else_teaches_about_latches() {
    let d = parse_err("module M {\n  in s: bit\n  out y: bit\n  y = if s { 1 }\n}\n");
    assert_eq!(d[0].code, Some("E1108"));
    assert!(d[0].msg.contains("else"));
    assert!(d[0].help.as_ref().unwrap().contains("latch"));
}

#[test]
fn reg_without_reset_value_is_an_error() {
    let d = parse_err("module M {\n  clock clk\n  reset rst\n  reg v: bits[8]\n}\n");
    assert_eq!(d[0].code, Some("E1104"));
    assert!(d[0].msg.contains("reset value"));
}

#[test]
fn assign_arrow_confusion_teaches() {
    let d = parse_err(
        "module M {\n  clock clk\n  reset rst\n  reg v: bits[8] = 0\n  on rise(clk) {\n    v = 1\n  }\n}\n",
    );
    assert_eq!(d[0].code, Some("E1106"));
    assert!(d[0].help.as_ref().unwrap().contains("<-"));
}

#[test]
fn every_parse_error_carries_a_code() {
    // The structural promise behind the E11xx retrofit: no parser
    // diagnostic ships codeless (the `error()` helper makes it
    // impossible; this locks the contract from the outside).
    // Note: `nope(1)` is no longer a parse error — non-builtin calls parse as
    // FnCall; name resolution is deferred to the checker (Task 6 / E1110).
    let broken = [
        "module M {\n  out y: bit\n  y = if y { 1 }\n}\n",
        "garbage here\n",
        "module M {\n  out y: bit\n  enum E {\n  }\n  y = 0\n}\n",
    ];
    for src in broken {
        for d in parse_err(src) {
            assert!(
                d.code.is_some_and(|c| c.starts_with("E11")),
                "codeless or mis-blocked parse error: {}",
                d.msg
            );
        }
    }
}

#[test]
fn parses_repeat_and_const() {
    parse_ok(
        "const N: int = 8\nmodule M {\n  in e: bits[8]\n  out led: bits[8]\n  repeat i: 0..8 {\n    led[i] = e[i]\n  }\n}\n",
    );
}

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
fn strict_parse_still_errs_on_bad_input() {
    // The strict `parse` contract is unchanged: any error discards the tree.
    assert!(parse(lex("module M {\n  1 2 3\n}\n").expect("lex error")).is_err());
}

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

#[test]
fn qualified_module_reference_parses() {
    let file = parse_ok("module M {\n  let x = a.b.Foo() { }\n}\n");
    let TopItem::Module(m) = &file.items[0] else {
        panic!("expected a module")
    };
    let ModuleItem::Inst(inst) = &m.items[0] else {
        panic!("expected an inst")
    };
    assert_eq!(inst.module.path.len(), 2);
    assert_eq!(inst.module.path[0].name, "a");
    assert_eq!(inst.module.path[1].name, "b");
    assert_eq!(inst.module.name.name, "Foo");
}

#[test]
fn bare_module_reference_still_parses_with_empty_path() {
    let file = parse_ok("module M {\n  let x = Foo() { }\n}\n");
    let TopItem::Module(m) = &file.items[0] else {
        panic!("expected a module")
    };
    let ModuleItem::Inst(inst) = &m.items[0] else {
        panic!("expected an inst")
    };
    assert!(inst.module.is_bare());
    assert_eq!(inst.module.name.name, "Foo");
}
