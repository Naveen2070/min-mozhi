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

use crate::ast::{BinOp, Builtin, Expr, ExprKind, UnOp};
use crate::checker::consteval::Env;
use crate::width_rules::Kind;

use super::kinds::infer_kind;

/// What Verilog would compute as `expr`'s width in a self-determined
/// position. `None` means "no Verilog-specific rule differs from
/// mimz's own here" (a plain identifier, a `Bool` literal — always sized,
/// `1'b1`/`1'b0`) — nothing for the caller to compare against.
///
/// `Int` is grouped here too but the claim is narrower than it looks: it
/// only holds when the checker's own concat-typing rule
/// (`checker/widths/ops/mod.rs`, E0405) has already rejected the literal as
/// a DIRECT concat/replicate member. Nested one level under an
/// adapt-to-sibling operator (`a & 15` as the member), `verilog_literal`
/// (`emit_verilog/mod.rs`) still renders an UNSIZED token, and real Icarus
/// refuses to elaborate it inside `{...}` — BUG-56 (docs/audit/bugs.md,
/// OPEN, found auditing this exact claim per round-4 plan Task 4). Left
/// grouped with `Ident`/`Bool` rather than split out because the FIX
/// belongs in `verilog_literal` (always emit a sized literal), not here —
/// splitting this arm would not change what it returns.
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
        // BUG-52 (docs/audit/bugs.md): a ternary's own self-determined
        // width is max of its RENDERED branch widths — same rule and
        // same rendering as `Min`/`Max` above (BUG-42's own lesson:
        // "same-width by mimz's rule" says nothing about what each
        // branch RENDERS as). `{b, if s { extend(a,8) } else {
        // extend(a,8) }}` renders each branch as the bare `(a)`, self-
        // determined at `a`'s own width, not mimz's grown one.
        ExprKind::IfExpr { then, els, .. } => Some(Kind {
            width: self_determined_operand_width(then, decls, env)?
                .max(self_determined_operand_width(els, decls, env)?),
            signed: infer_kind(expr, decls, env)?.signed,
        }),
        // Same reasoning as `IfExpr`, over every arm instead of two
        // branches — Verilog renders a `match` as a chain of ternaries.
        ExprKind::Match { arms, .. } => {
            let mut w = 0u32;
            for a in arms {
                w = w.max(self_determined_operand_width(&a.value, decls, env)?);
            }
            Some(Kind {
                width: w,
                signed: infer_kind(expr, decls, env)?.signed,
            })
        }
        // A reduction (`&`/`|`/`^` prefix) is exactly 1 bit in BOTH
        // models — nothing to compare. Every other unary op renders its
        // operand unchanged, so recurse the same way `SignedCast` does.
        ExprKind::Unary { op, expr: inner } => match op {
            UnOp::RedAnd | UnOp::RedOr | UnOp::RedXor => None,
            _ => Some(Kind {
                width: self_determined_operand_width(inner, decls, env)?,
                signed: infer_kind(expr, decls, env)?.signed,
            }),
        },
        // Each member is hoisted at its OWN position first (this
        // module's own recursion into every operand), so by the time a
        // concat/replicate is itself examined, its rendered width
        // already equals mimz's — nothing left to mismatch.
        ExprKind::Concat(_) | ExprKind::Replicate { .. } => None,
        // A part-select is exactly `hi-lo+1` bits in Verilog, and BUG-20
        // already forces a non-identifier base to a named wire first —
        // the rendered width can never disagree with mimz's.
        ExprKind::Slice { .. } => None,
        // A bit-select is 1 bit in both models; a `mem` read renders as
        // the declared element width, which is mimz's own width too.
        ExprKind::Index { .. } => None,
        // An instance port or enum field renders as a real `wire` of
        // its declared width (round 3 Task 1's `decls` fix is what
        // makes mimz's own inference agree with it).
        ExprKind::Field { .. } => None,
        // The emitted Verilog function declares its own return width
        // (`function automatic [(N)-1:0]`) — that IS mimz's width.
        ExprKind::FnCall { .. } => None,
        // Not a `bits` value in a self-determined position — but the THREE
        // reasons differ (round-4 plan Task 4 checked each): `BundleLit`'s
        // `Type { .. }` literal syntax is PARSER-restricted to a `Wire`
        // init/`Drive` RHS, so it cannot reach here as a sub-expression at
        // all; `EnumConstruct` is checker-rejected here specifically
        // (E0403, BUG-31); `ArrayLit` is neither — the checker currently
        // accepts `[a,a,a][0]` and the EMITTER panics rendering it
        // (`unreachable!` a few match arms up, in `expr_subst` — BUG-56's
        // sibling, BUG-57, docs/audit/bugs.md), at every position, not
        // specifically a self-determined one, which is why `None` is still
        // correct here (nothing ever renders to compare against).
        ExprKind::BundleLit(_) | ExprKind::ArrayLit(_) | ExprKind::EnumConstruct { .. } => None,
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
