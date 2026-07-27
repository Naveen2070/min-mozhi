use super::*;

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
fn qq_parses_as_lowest_precedence_left_associative() {
    // `a || b ?? c` must parse as `(a || b) ?? c`, not `a || (b ?? c)`.
    let e = parse_expr_ok("a || b ?? c");
    match e.kind {
        ExprKind::Binary {
            op: BinOp::Coalesce,
            lhs,
            rhs,
        } => {
            assert!(matches!(rhs.kind, ExprKind::Ident(ref n) if n == "c"));
            assert!(matches!(
                lhs.kind,
                ExprKind::Binary {
                    op: BinOp::LogicOr,
                    ..
                }
            ));
        }
        other => panic!("expected top-level Coalesce, got {other:?}"),
    }
}

#[test]
fn qq_chain_is_left_associative() {
    // `a ?? b ?? c` reads `(a ?? b) ?? c`.
    let e = parse_expr_ok("a ?? b ?? c");
    match e.kind {
        ExprKind::Binary {
            op: BinOp::Coalesce,
            lhs,
            ..
        } => {
            assert!(matches!(
                lhs.kind,
                ExprKind::Binary {
                    op: BinOp::Coalesce,
                    ..
                }
            ));
        }
        other => panic!("expected nested left-associative Coalesce, got {other:?}"),
    }
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
    assert!(matches!(&count.kind, ExprKind::Int { value, .. } if *value == Bits::Small(2)));
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
