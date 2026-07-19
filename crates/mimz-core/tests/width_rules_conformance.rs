//! T3 (Stage 4, `docs/plan/phase-2-correctness-consolidation.local.md`):
//! a conformance table pinning spec semantics per operator, executed
//! against all three authorities — the shared `width_rules` module
//! directly, the checker's own `Ty`-level inference, and the
//! simulator's own `Val`-level evaluation — asserting all three agree.
//! Starts with shift (`<<`/`>>`), per Stage 4's own scoping decision
//! (the deepest bug history — BUG-6, BUG-11 — and the existing
//! `eval_ctx`/`binary_ctx` context-threading precedent to check against).

use mimz_core::width_rules::{Kind, shift_result};
use mimz_core::{checker, lexer, parser};
use mimz_sim::sim::value::{Val, eval};

/// One shift conformance case: an LHS kind, an amount kind, and the
/// expected result (or `None` if this combination is spec-invalid).
struct Case {
    lhs: Kind,
    amount: Kind,
    expect: Option<Kind>,
}

const CASES: &[Case] = &[
    // Unsigned LHS, unsigned amount — the common case.
    Case {
        lhs: Kind {
            width: 8,
            signed: false,
        },
        amount: Kind {
            width: 3,
            signed: false,
        },
        expect: Some(Kind {
            width: 8,
            signed: false,
        }),
    },
    // Signed LHS preserves its own kind.
    Case {
        lhs: Kind {
            width: 16,
            signed: true,
        },
        amount: Kind {
            width: 4,
            signed: false,
        },
        expect: Some(Kind {
            width: 16,
            signed: true,
        }),
    },
    // A single-bit LHS.
    Case {
        lhs: Kind {
            width: 1,
            signed: false,
        },
        amount: Kind {
            width: 1,
            signed: false,
        },
        expect: Some(Kind {
            width: 1,
            signed: false,
        }),
    },
    // Signed amount is always rejected, regardless of LHS.
    Case {
        lhs: Kind {
            width: 8,
            signed: false,
        },
        amount: Kind {
            width: 3,
            signed: true,
        },
        expect: None,
    },
];

#[test]
fn shift_result_matches_the_table() {
    for (i, case) in CASES.iter().enumerate() {
        let got = shift_result(case.lhs, case.amount).ok();
        assert_eq!(
            got, case.expect,
            "case {i}: width_rules::shift_result({:?}, {:?}) = {got:?}, expected {:?}",
            case.lhs, case.amount, case.expect
        );
    }
}

/// Build a minimal `.mimz` module declaring `lhs`/`amount` at the
/// given kinds and driving `out y = lhs << amount`, then run it through
/// the checker's own type inference (`checker::check`'s pass/fail IS
/// the checker's `Ty`-level answer for this table: `Some` when it
/// accepts the program, `None` when the case is meant to be rejected).
fn checker_agrees(lhs: Kind, amount: Kind) -> bool {
    let ty = |k: Kind| {
        if k.signed {
            format!("signed[{}]", k.width)
        } else {
            format!("bits[{}]", k.width)
        }
    };
    let src = format!(
        "module Fuzz {{\n  in lhs: {}\n  in amount: {}\n  out y: {}\n  y = lhs << amount\n}}\n",
        ty(lhs),
        ty(amount),
        ty(lhs), // shift preserves lhs's kind — the declared `y` matches lhs exactly when the case is valid
    );
    let tokens = lexer::lex(&src).expect("generated source always lexes");
    let file = parser::parse(tokens).expect("generated source always parses");
    checker::check(std::slice::from_ref(&file)).is_ok()
}

/// Same question, answered by the simulator's own `Val`-level
/// evaluator: build concrete `Val`s at the given kinds and run them
/// through the real expression evaluator (`sim::value::eval` via a
/// literal `ExprKind::Binary(Shl)` node is unnecessary — `binary_known`
/// itself is private to `mimz-sim`, so this drives it the same way
/// `width_rules_conformance`'s own direct `shift_result` case does,
/// through the one function `mimz-sim` re-exports for exactly this
/// purpose: `mimz_sim::sim::value::eval` on a hand-built `Binary` expr).
fn simulator_agrees(lhs: Kind, amount: Kind) -> bool {
    use mimz_core::ast::{BinOp, Expr, ExprKind};
    use mimz_core::span::Span;
    use mimz_sim::sim::value::Resolver;

    struct FixedResolver {
        lhs: Val,
        amount: Val,
    }
    impl Resolver for FixedResolver {
        fn signal(&mut self, name: &str) -> Result<Val, String> {
            match name {
                "lhs" => Ok(self.lhs),
                "amount" => Ok(self.amount),
                other => Err(format!("unknown signal `{other}` in conformance test")),
            }
        }
        fn ints(&self) -> &std::collections::BTreeMap<String, i128> {
            static EMPTY: std::sync::OnceLock<std::collections::BTreeMap<String, i128>> =
                std::sync::OnceLock::new();
            EMPTY.get_or_init(Default::default)
        }
        fn funcs(&self) -> Option<&std::collections::HashMap<String, mimz_core::ast::FuncDecl>> {
            None
        }
    }

    let span = Span::new(0, 0);
    let expr = Expr {
        kind: ExprKind::Binary {
            op: BinOp::Shl,
            lhs: Box::new(Expr {
                kind: ExprKind::Ident("lhs".to_string()),
                span,
            }),
            rhs: Box::new(Expr {
                kind: ExprKind::Ident("amount".to_string()),
                span,
            }),
        },
        span,
    };
    let mut resolver = FixedResolver {
        lhs: Val::new(0, lhs.width, lhs.signed),
        amount: Val::new(0, amount.width, amount.signed),
    };
    eval(&mut resolver, &expr).is_ok()
}

#[test]
fn checker_and_simulator_agree_with_the_table() {
    for (i, case) in CASES.iter().enumerate() {
        let should_succeed = case.expect.is_some();
        assert_eq!(
            checker_agrees(case.lhs, case.amount),
            should_succeed,
            "case {i}: checker disagreed with the table for lhs={:?} amount={:?}",
            case.lhs,
            case.amount
        );
        assert_eq!(
            simulator_agrees(case.lhs, case.amount),
            should_succeed,
            "case {i}: simulator disagreed with the table for lhs={:?} amount={:?}",
            case.lhs,
            case.amount
        );
    }
}
