//! Verilog's own self-determined-width rule — Stage 4, Phase A1b
//! (`docs/superpowers/specs/2026-07-19-emitter-self-determined-position-design.local.md`).
//!
//! What real Verilog computes as an expression's width when it lands in
//! a self-determined position (concat member, replication's repeated
//! part/count, comparison operand, `$signed`/`$unsigned` argument) —
//! NOT mimz's own semantics (that's `kinds::infer_kind`). Confirmed
//! empirically against real `iverilog`, matching this codebase's
//! existing convention for BUG-18/19/20/21's own investigations.
//!
//! Classifying a new `Builtin` arm here: ask "is this argument's
//! RENDERED width necessarily its mimz width?", never "is this
//! operator's RESULT width necessarily its mimz width?" — the second
//! question is about mimz's own semantics (already guaranteed by the
//! checker) and says nothing about what Verilog does with the literal
//! text this file's emitter produces. BUG-42 shipped from answering the
//! wrong question: `min`/`max`'s two operands ARE same-width under
//! mimz's own rule, which was mistaken for "so no mismatch is possible"
//! — but an operand can still each independently render as a narrower
//! mismatched sub-expression (`extend(p, N)` renders as the bare `(p)`),
//! which only the first question catches.

use std::collections::HashMap;

use crate::ast::{BinOp, Builtin, Expr, ExprKind};
use crate::checker::consteval::Env;
use crate::width_rules::Kind;

use super::kinds::infer_kind;

/// What Verilog would compute as `expr`'s width in a self-determined
/// position. `None` means "no Verilog-specific rule differs from
/// mimz's own here" (a plain identifier, an explicitly-sized literal) —
/// nothing for the caller to compare against.
pub(crate) fn verilog_self_determined_kind(
    expr: &Expr,
    decls: &HashMap<String, Kind>,
    env: &Env,
) -> Option<Kind> {
    match &expr.kind {
        ExprKind::Ident(_) | ExprKind::Int { .. } | ExprKind::Bool(_) => None,
        ExprKind::Binary { op, lhs, rhs } => match op {
            // Comparisons are always 1-bit self-determined regardless of
            // operand kind — same as mimz's own rule, so no mismatch is
            // possible; `None` (nothing to check).
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => None,
            // Every other binary operator: Verilog self-determines each
            // operand at its OWN width (no growth, no context), then
            // takes the max — the exact "matched" rule, applied
            // uniformly, not just to the width-matching family.
            _ => {
                let l = self_determined_operand_width(lhs, decls, env)?;
                let r = self_determined_operand_width(rhs, decls, env)?;
                Some(Kind {
                    width: l.max(r),
                    signed: infer_kind(expr, decls, env)?.signed,
                })
            }
        },
        // BUG-28/BUG-29 (docs/audit/bugs.md): this match must stay
        // exhaustive over `Builtin` — a new builtin silently inheriting
        // a wildcard `None` here is exactly how `extend`/`abs` shipped a
        // silent miscompile (sim passes, real hardware wrong, no
        // diagnostic). A new variant now fails the build until it is
        // classified here.
        ExprKind::Call { func, args } => match func {
            // `extend(x, N)` renders as bare `(x)` — Verilog gives it the
            // ARGUMENT's width in a self-determined position, never N.
            // Report that so the caller sees the mismatch against mimz's
            // `Kind{N}` and hoists to `wire [N-1:0] __mimz_sub_k`.
            Builtin::Extend => Some(Kind {
                width: self_determined_operand_width(&args[0], decls, env)?,
                signed: infer_kind(expr, decls, env)?.signed,
            }),
            // Renders to a ternary: Verilog sizes it at
            // `max(operand widths)`, not mimz's grown `N+1` result.
            Builtin::Abs => Some(Kind {
                width: self_determined_operand_width(&args[0], decls, env)?,
                signed: infer_kind(expr, decls, env)?.signed,
            }),
            // `min`/`max` render to a ternary — `(a < b) ? a : b` — whose
            // OWN self-determined width is `max` of the two RENDERED
            // operand widths, same as any other binary-shaped construct
            // (BUG-42, `docs/audit/bugs.md`). Recurse into each operand,
            // same as `SignedCast`/`UnsignedCast` below: same-width by the
            // checker's own rule is a fact about mimz's widths, not about
            // what each operand renders as. `extend(p, 11)` renders as the
            // bare `(p)` — self-determined at `p`'s own 6 bits, not 11 —
            // so `min(extend(p, 11), extend(p, 11))` self-determines to 6
            // bits, not mimz's 11; the mismatch this now exposes is what
            // makes the caller hoist.
            Builtin::Min | Builtin::Max => Some(Kind {
                width: self_determined_operand_width(&args[0], decls, env)?
                    .max(self_determined_operand_width(&args[1], decls, env)?),
                signed: infer_kind(expr, decls, env)?.signed,
            }),
            // `trunc` renders as an explicit part-select `x[N-1:0]` —
            // already exactly N bits in Verilog regardless of the base
            // (BUG-36 already hoists a composite base to a named wire, so
            // the base's own rendered width can never leak through).
            // Reductions are 1-bit on both sides regardless of operand
            // width. No mismatch possible for either.
            Builtin::Trunc | Builtin::Nand | Builtin::Nor | Builtin::Xnor => None,
            // `$signed`/`$unsigned`'s argument is self-determined at its
            // own width (confirmed empirically during BUG-18/19/20/21's
            // investigations) — same width mimz's own model gives, UNLESS
            // the argument is itself a mismatched sub-expression, which
            // is caught by recursing into it, not by this call site.
            Builtin::SignedCast | Builtin::UnsignedCast => {
                verilog_self_determined_kind(&args[0], decls, env)
            }
            // Renders as `$unsigned(...)`, exactly like `UnsignedCast` —
            // same reasoning: the cast doesn't change the argument's own
            // self-determined width, so recurse into it.
            Builtin::Encoding => verilog_self_determined_kind(&args[0], decls, env),
            // Const-folded before emit — never reaches a rendered
            // self-determined position as a runtime expression.
            Builtin::Clog2 => None,
            // Lowered to items (registers/always blocks) before emit —
            // never appears as an inline self-determined operand.
            Builtin::SyncDoubleFlop | Builtin::SyncPulse => None,
        },
        _ => None,
    }
}

/// A single operand's OWN self-determined width, ignoring any
/// surrounding context — recurses through the same binary-operator rule
/// so a NESTED mismatch is visible to the caller's `l.max(r)` too. `None`
/// when neither Verilog's rule nor mimz's own can resolve `expr` (BUG-41,
/// `docs/audit/bugs.md`) — propagated by every caller, same convention
/// as `infer_kind` itself.
fn self_determined_operand_width(
    expr: &Expr,
    decls: &HashMap<String, Kind>,
    env: &Env,
) -> Option<u32> {
    let k = match verilog_self_determined_kind(expr, decls, env) {
        Some(k) => k,
        None => infer_kind(expr, decls, env)?,
    };
    Some(k.width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::ExprKind;
    use crate::span::Span;

    fn ident(name: &str) -> Expr {
        Expr {
            kind: ExprKind::Ident(name.to_string()),
            span: Span::new(0, 0),
        }
    }

    #[test]
    fn plain_identifier_has_no_verilog_specific_rule() {
        let decls = HashMap::new();
        assert_eq!(
            verilog_self_determined_kind(&ident("p0"), &decls, &Env::new()),
            None
        );
    }

    #[test]
    fn lossless_sub_self_determines_to_max_operand_width_not_growth() {
        let mut decls = HashMap::new();
        decls.insert(
            "p0".to_string(),
            Kind {
                width: 15,
                signed: false,
            },
        );
        decls.insert(
            "p1".to_string(),
            Kind {
                width: 15,
                signed: false,
            },
        );
        let e = Expr {
            kind: ExprKind::Binary {
                op: BinOp::Sub,
                lhs: Box::new(ident("p0")),
                rhs: Box::new(ident("p1")),
            },
            span: Span::new(0, 0),
        };
        // Verilog: max(15,15) = 15, no growth (unlike mimz's own
        // lossless_result, which would say 16) — this is BUG-19's exact
        // mismatch, now representable and detectable.
        assert_eq!(
            verilog_self_determined_kind(&e, &decls, &Env::new()),
            Some(Kind {
                width: 15,
                signed: false
            })
        );
    }

    #[test]
    fn comparison_has_no_verilog_specific_rule() {
        let mut decls = HashMap::new();
        decls.insert(
            "p0".to_string(),
            Kind {
                width: 8,
                signed: false,
            },
        );
        decls.insert(
            "p1".to_string(),
            Kind {
                width: 8,
                signed: false,
            },
        );
        let e = Expr {
            kind: ExprKind::Binary {
                op: BinOp::Eq,
                lhs: Box::new(ident("p0")),
                rhs: Box::new(ident("p1")),
            },
            span: Span::new(0, 0),
        };
        assert_eq!(verilog_self_determined_kind(&e, &decls, &Env::new()), None);
    }
}
