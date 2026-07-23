//! Operator, concat, and builtin typing: the lossless `+`/`-`/`*`
//! growth rules, the width-matching family (`+%`, bitwise,
//! comparisons), shifts, `{...}` concatenation, and the four builtins
//! (`extend`/`trunc`/`signed`/`unsigned`).

use crate::ast::{BinOp, Builtin, Expr, UnOp};
use crate::span::Span;

use super::super::Checker;
use super::super::consteval;
use super::{MAX_WIDTH, Ty, Wcx, bits, min_bits, min_signed_bits, op_text, same, show};

/// Narrow a checker-side width (`u128`, since `Ty::Bits`/`Ty::Signed`
/// store one) to the `u32` `width_rules::Kind` uses. Safe for any width
/// this checker would ever actually accept — `MAX_WIDTH` (1,000,000,
/// `crate::width_rules`) is far below `u32::MAX` (~4.29 billion) —
/// written as a checked conversion rather than a raw cast so a future
/// change to `MAX_WIDTH` fails loudly instead of silently wrapping.
pub(super) fn width_u32(n: u128) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The inverse of the checker's own `Ty` -> `Kind` narrowing: turn a
/// `width_rules::Kind` (the shared rule's result) back into this pass's
/// `Ty`.
fn kind_to_ty(k: crate::width_rules::Kind) -> Ty<'static> {
    if k.signed {
        Ty::Signed(k.width as u128)
    } else {
        bits(k.width as u128)
    }
}

/// Narrow a `Ty` to a `width_rules::Kind`, for the three scalar
/// variants `lossless_ty`/`matched_ty` accept (`Bit`/`Bits`/`Signed`).
/// `None` for anything else (`Enum`, `Memory`, `Array`, `Bundle`,
/// `CtInt`, `Unknown`, `Clock`, `Reset`) — those are rejected by their
/// callers with the existing "cannot mix" diagnostic before this
/// function is even relevant; it is not this helper's job to diagnose,
/// only to convert what IS a scalar kind.
fn ty_to_kind_opt(t: &Ty) -> Option<crate::width_rules::Kind> {
    match t {
        Ty::Bit => Some(crate::width_rules::Kind {
            width: 1,
            signed: false,
        }),
        Ty::Bits(n) => Some(crate::width_rules::Kind {
            width: width_u32(*n),
            signed: false,
        }),
        Ty::Signed(n) => Some(crate::width_rules::Kind {
            width: width_u32(*n),
            signed: true,
        }),
        _ => None,
    }
}

/// Returns `data`'s type iff `fields` is EXACTLY `{ valid: bit, data: T }`
/// — no missing `valid`, no wrong-typed `valid`, no extra fields. Shared by
/// `coalesce_ty`'s LHS check and its OR-mux RHS check — `??`'s rule is
/// narrower than `bundle_shape_match` (feature 2.9's "cover a required
/// subset, extras OK" rule) on purpose; do not reuse that function here.
fn valid_bundle_shape<'a>(fields: &[(String, Ty<'a>)]) -> Option<Ty<'a>> {
    if fields.len() != 2 {
        return None;
    }
    let valid = fields.iter().find(|(n, _)| n == "valid")?;
    if !same(&valid.1, &Ty::Bit) {
        return None;
    }
    let data = fields.iter().find(|(n, _)| n == "data")?;
    Some(data.1)
}

impl<'a> Checker<'a> {
    pub(super) fn unary_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        e: &'a Expr,
        op: UnOp,
        inner: &'a Expr,
    ) -> Ty<'a> {
        let t = self.infer_ty(cx, inner);
        if matches!(t, Ty::Unknown) {
            return Ty::Unknown;
        }
        if matches!(t, Ty::Clock | Ty::Reset) {
            return self.not_data(cx, inner.span, &t);
        }
        if let Ty::CtInt(_) = t {
            // Pure compile-time: fold (consteval explains what it rejects,
            // e.g. `~` has no width on an unsized value).
            return match consteval::eval(e, &cx.env) {
                Ok(v) => Ty::CtInt(v),
                Err(d) => {
                    self.diags.push(d.with_file(cx.file));
                    Ty::Unknown
                }
            };
        }
        match op {
            UnOp::Neg => match t {
                Ty::Signed(n) => Ty::Signed(n + 1), // lossless: gains the carry bit
                other => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        format!("`-` needs a `signed` value, found {}", show(&other)),
                        "negation is signed-only (spec/02 section 1.7) — use \
                         `0 -% x` for two's-complement wrap, or cast with \
                         `signed(x)` first",
                    );
                    Ty::Unknown
                }
            },
            UnOp::BitNot => match t {
                Ty::Bit | Ty::Bits(_) | Ty::Signed(_) => t,
                other => self.not_numeric(cx, e.span, &other, "`~`"),
            },
            UnOp::LogicNot => match t {
                Ty::Bit => Ty::Bit,
                other => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0404",
                        format!("`!` works on a single `bit`, found {}", show(&other)),
                        "make a bit first: compare (`x == 0`) or reduce (`|x`)",
                    );
                    Ty::Unknown
                }
            },
            UnOp::RedAnd | UnOp::RedOr | UnOp::RedXor => match t {
                Ty::Bit | Ty::Bits(_) => Ty::Bit,
                Ty::Signed(_) => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0403",
                        "reductions work on `bits`, not `signed`",
                        "cast first: `|unsigned(x)` (spec/02 section 3)",
                    );
                    Ty::Unknown
                }
                other => self.not_numeric(cx, e.span, &other, "a reduction"),
            },
        }
    }

    pub(super) fn binary_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        e: &'a Expr,
        op: BinOp,
        lhs: &'a Expr,
        rhs: &'a Expr,
    ) -> Ty<'a> {
        let lt = self.infer_ty(cx, lhs);
        let rt = self.infer_ty(cx, rhs);
        if matches!(lt, Ty::Unknown) || matches!(rt, Ty::Unknown) {
            return Ty::Unknown;
        }
        for (t, side) in [(&lt, lhs), (&rt, rhs)] {
            if matches!(t, Ty::Clock | Ty::Reset) {
                return self.not_data(cx, side.span, t);
            }
        }
        if let (Ty::CtInt(_), Ty::CtInt(_)) = (lt, rt) {
            // Pure compile-time: fold the whole node (consteval rejects
            // what genuinely has no compile-time meaning, e.g. `+%`).
            return match consteval::eval(e, &cx.env) {
                Ok(v) => Ty::CtInt(v),
                Err(d) => {
                    self.diags.push(d.with_file(cx.file));
                    Ty::Unknown
                }
            };
        }
        use BinOp::*;
        match op {
            Add | Sub | Mul => self.lossless_ty(cx, e, op, (lhs, lt), (rhs, rt)),
            AddWrap | SubWrap | MulWrap => self.matched_ty(cx, op_text(op), (lhs, lt), (rhs, rt)),
            BitAnd | BitOr | BitXor => self.matched_ty(cx, op_text(op), (lhs, lt), (rhs, rt)),
            Shl | Shr => self.shift_ty(cx, (lhs, lt), (rhs, rt)),
            Eq | Ne => {
                if let (Ty::Enum(a), Ty::Enum(b)) = (&lt, &rt) {
                    if a.name.name != b.name.name {
                        self.err(
                            cx.file,
                            e.span,
                            "E0403",
                            format!(
                                "cannot compare enum `{}` with enum `{}`",
                                a.name.name, b.name.name
                            ),
                            "only values of the SAME enum compare",
                        );
                    }
                    return Ty::Bit;
                }
                let _ = self.matched_ty(cx, op_text(op), (lhs, lt), (rhs, rt));
                Ty::Bit
            }
            Lt | Le | Gt | Ge => {
                if matches!(lt, Ty::Enum(_)) || matches!(rt, Ty::Enum(_)) {
                    self.err(
                        cx.file,
                        e.span,
                        "E0403",
                        "enums have no order",
                        "an enum's binary encoding is a compiler detail — compare \
                         with `==`/`!=`, or model an ordered quantity as `bits[N]`",
                    );
                    return Ty::Bit;
                }
                let _ = self.matched_ty(cx, op_text(op), (lhs, lt), (rhs, rt));
                Ty::Bit
            }
            LogicAnd | LogicOr => {
                for (t, side) in [(&lt, lhs), (&rt, rhs)] {
                    match t {
                        Ty::Bit => {}
                        Ty::CtInt(v) if *v == 0 || *v == 1 => {}
                        other => self.err(
                            cx.file,
                            side.span,
                            "E0404",
                            format!(
                                "`{}` works on single bits, found {}",
                                op_text(op),
                                show(other)
                            ),
                            "logical operators have no C-style truthiness — compare \
                             (`x != 0`) or reduce (`|x`) to make a bit first",
                        ),
                    }
                }
                Ty::Bit
            }
            // `??` in an inference-only position (a sub-expression with no
            // context type) — no `expected` to disambiguate a literal
            // fallback's width, so pass `Ty::Unknown` (check_expr's own
            // `Coalesce` arm threads the real context type in when there is
            // one). See `coalesce_ty`.
            Coalesce => self.coalesce_ty(cx, (lhs, lt), (rhs, rt), Ty::Unknown),
        }
    }

    /// Types a `??` expression (design:
    /// `docs/superpowers/specs/2026-07-16-valid-bundle-sugar-design.local.md`,
    /// "The `??` operator"). LHS must be a valid-bundle (`T?`, i.e.
    /// structurally `{ valid: bit, data: T }` — feature 2.9's structural
    /// rule decides this, same as any other bundle-typed slot); otherwise
    /// E0911. RHS is polymorphic:
    /// - a sized value whose type matches `data` exactly → UNWRAP form,
    ///   result `T`;
    /// - a bundle whose own `data` field matches `T` exactly → OR-MUX form,
    ///   result stays the LHS's own `Ty::Bundle` (`T?`), unchanged;
    /// - a bare literal → adapts to `data`'s width (UNWRAP form). `expected`
    ///   (the assignment/context type, or `Unknown` when inferring a
    ///   sub-expression) decides: a literal in a context that itself wants
    ///   `data`'s type unwraps cleanly; a context that wants a *different*
    ///   concrete type can't be satisfied by the unwrap result, so that is
    ///   the ??-specific mismatch E0912 rather than a generic width error.
    /// - anything else → E0912.
    ///
    /// Deliberately NOT routed through `bundle_shape_match`: that rule covers
    /// a required *subset* of fields (extras ignored); `??`'s rule is the
    /// narrower "`data` types match exactly on both sides", written here as a
    /// direct field comparison via `resolve_bundle_fields` + `same`.
    pub(super) fn coalesce_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        (lhs, lt): (&'a Expr, Ty<'a>),
        (rhs, rt): (&'a Expr, Ty<'a>),
        expected: Ty<'a>,
    ) -> Ty<'a> {
        if matches!(lt, Ty::Unknown) || matches!(rt, Ty::Unknown) {
            return Ty::Unknown; // an inner error was already reported
        }
        let Ty::Bundle {
            name: lname,
            bfile_hint: lhint,
            args: largs,
        } = lt
        else {
            self.err(
                cx.file,
                lhs.span,
                "E0911",
                "`??`'s left operand must be a valid-bundle (`T?`)",
                "`??` reads validity off its left operand — the left side must be a \
                 `bits[N]?`/`bit?`/`signed[N]?`-shaped value (or a user bundle with \
                 an identical `{ valid: bit, data: T }` shape)",
            );
            return Ty::Unknown;
        };
        let Some(lfields) = self.resolve_bundle_fields(cx, lname, lhint, largs, lhs.span) else {
            return Ty::Unknown; // E0906 already reported
        };
        let Some(data_ty) = valid_bundle_shape(&lfields) else {
            // Structurally a bundle, but not exactly `{ valid: bit, data: T }`
            // — missing `valid`, wrong-typed `valid`, or extra fields — not a
            // valid-bundle shape, same diagnostic as "not optional".
            self.err(
                cx.file,
                lhs.span,
                "E0911",
                "`??`'s left operand must be a valid-bundle (`T?`)",
                "expected a bundle shaped exactly `{ valid: bit, data: T }`",
            );
            return Ty::Unknown;
        };
        // Bare-literal fallback: it adapts to `data`'s width (unwrap form),
        // but only in a context that actually wants `data`'s type.
        if let Ty::CtInt(v) = rt {
            if matches!(expected, Ty::Unknown) || same(&expected, &data_ty) {
                self.fit(cx, rhs.span, v, data_ty);
                return data_ty;
            }
            return self.qq_rhs_mismatch(cx, rhs.span, &data_ty);
        }
        // Unwrap form: a sized RHS whose type matches `data` exactly.
        if same(&rt, &data_ty) {
            return data_ty;
        }
        // OR-mux form: RHS is itself a valid-bundle (exactly `{ valid, data }`
        // shaped, same rule as the LHS) whose own `data` matches `T` exactly
        // (direct field comparison, NOT `bundle_shape_match`).
        if let Ty::Bundle {
            name: rname,
            bfile_hint: rhint,
            args: rargs,
        } = rt
            && let Some(rfields) = self.resolve_bundle_fields(cx, rname, rhint, rargs, rhs.span)
            && let Some(rdata) = valid_bundle_shape(&rfields)
            && same(&rdata, &data_ty)
        {
            return lt;
        }
        self.qq_rhs_mismatch(cx, rhs.span, &data_ty)
    }

    /// The shared E0912 diagnostic for a `??` right operand that matches
    /// neither the unwrap nor the OR-mux form. Returns `Unknown`.
    fn qq_rhs_mismatch(&mut self, cx: &mut Wcx<'a>, span: Span, data_ty: &Ty<'a>) -> Ty<'a> {
        self.err(
            cx.file,
            span,
            "E0912",
            format!(
                "`??`'s right operand doesn't match the left operand's `data` type ({})",
                show(data_ty)
            ),
            "the right operand must be either the same type as `data`, or another \
             valid-bundle whose `data` matches exactly — no width coercion",
        );
        Ty::Unknown
    }

    /// `+`/`-`/`*` — lossless growth. Operand widths may differ (the
    /// result can never drop information); signedness must match. A
    /// compile-time operand takes the other side's width if it fits,
    /// otherwise its own minimal width (growing is always safe here).
    fn lossless_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        e: &'a Expr,
        op: BinOp,
        (lhs, lt): (&'a Expr, Ty<'a>),
        (rhs, rt): (&'a Expr, Ty<'a>),
    ) -> Ty<'a> {
        let _ = e;
        let (a, b) = match (lt, rt) {
            (Ty::CtInt(v), t) => {
                let Some(adapted) = self.adapt_lossless(cx, lhs.span, v, &t) else {
                    return Ty::Unknown;
                };
                (adapted, t)
            }
            (t, Ty::CtInt(v)) => {
                let Some(adapted) = self.adapt_lossless(cx, rhs.span, v, &t) else {
                    return Ty::Unknown;
                };
                (t, adapted)
            }
            (a, b) => (a, b),
        };
        let (Some(ka), Some(kb)) = (ty_to_kind_opt(&a), ty_to_kind_opt(&b)) else {
            self.err(
                cx.file,
                lhs.span.join(rhs.span),
                "E0403",
                format!("`{}` cannot mix {} and {}", op_text(op), show(&a), show(&b)),
                "`signed` and `bits` never mix in an operator — convert \
                 visibly with `signed(x)` / `unsigned(x)` (spec/02 section 1.7)",
            );
            return Ty::Unknown;
        };
        match crate::width_rules::lossless_result(ka, kb, matches!(op, BinOp::Mul)) {
            Ok(k) => kind_to_ty(k),
            Err(_) => {
                self.err(
                    cx.file,
                    lhs.span.join(rhs.span),
                    "E0403",
                    format!("`{}` cannot mix {} and {}", op_text(op), show(&a), show(&b)),
                    "`signed` and `bits` never mix in an operator — convert \
                     visibly with `signed(x)` / `unsigned(x)` (spec/02 section 1.7)",
                );
                Ty::Unknown
            }
        }
    }

    /// A compile-time operand of a lossless op: prefer the other side's
    /// width; if the value doesn't fit there, take its own minimal width
    /// (lossless growth makes that safe). Negative values need `signed`.
    fn adapt_lossless(
        &mut self,
        cx: &mut Wcx<'a>,
        span: Span,
        v: i128,
        other: &Ty<'a>,
    ) -> Option<Ty<'a>> {
        match other {
            Ty::Bit | Ty::Bits(_) => {
                if v < 0 {
                    self.fit(cx, span, v, *other); // reports the negative case
                    return None;
                }
                let n = if let Ty::Bits(n) = other { *n } else { 1 };
                Some(bits(n.max(min_bits(v))))
            }
            Ty::Signed(n) => Some(Ty::Signed((*n).max(min_signed_bits(v)))),
            _ => {
                self.fit(cx, span, v, *other);
                None
            }
        }
    }

    /// The width-matching operators (`+%` family, bitwise, comparisons):
    /// both sides the same kind and width; a compile-time operand adapts
    /// to the sized side (and must fit). Returns the common type.
    fn matched_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        op: &str,
        (lhs, lt): (&'a Expr, Ty<'a>),
        (rhs, rt): (&'a Expr, Ty<'a>),
    ) -> Ty<'a> {
        let (a, b) = match (lt, rt) {
            (Ty::CtInt(v), t) => {
                self.fit(cx, lhs.span, v, t);
                return t;
            }
            (t, Ty::CtInt(v)) => {
                self.fit(cx, rhs.span, v, t);
                return t;
            }
            (a, b) => (a, b),
        };
        if same(&a, &b) {
            if matches!(a, Ty::Enum(_)) {
                self.err(
                    cx.file,
                    lhs.span.join(rhs.span),
                    "E0403",
                    format!("`{op}` does not work on enum values"),
                    "enums only compare with `==`/`!=` and drive `match`",
                );
                return Ty::Unknown;
            }
            return a;
        }
        if matches!(a, Ty::Enum(_)) || matches!(b, Ty::Enum(_)) {
            self.err(
                cx.file,
                lhs.span.join(rhs.span),
                "E0403",
                format!("`{op}` cannot mix {} and {}", show(&a), show(&b)),
                "`signed` and `bits` never mix in an operator — convert \
                 visibly with `signed(x)` / `unsigned(x)` (spec/02 section 1.7)",
            );
            return Ty::Unknown;
        }
        match (ty_to_kind_opt(&a), ty_to_kind_opt(&b)) {
            (Some(ka), Some(kb)) => {
                // Both scalar, and `same(&a, &b)` above already ruled out
                // an exact match, so `matched_result` is guaranteed to
                // return `Err` here — called anyway so the shared rule
                // stays the single source of truth for what counts as a
                // scalar match, rather than re-deriving that boundary.
                match crate::width_rules::matched_result(ka, kb) {
                    Ok(k) => kind_to_ty(k),
                    Err(_) if ka.signed != kb.signed => {
                        self.err(
                            cx.file,
                            lhs.span.join(rhs.span),
                            "E0403",
                            format!("`{op}` cannot mix {} and {}", show(&a), show(&b)),
                            "`signed` and `bits` never mix in an operator — convert \
                             visibly with `signed(x)` / `unsigned(x)` (spec/02 section 1.7)",
                        );
                        Ty::Unknown
                    }
                    Err(_) => {
                        self.err_args(
                            cx.file,
                            lhs.span.join(rhs.span),
                            "E0402",
                            format!(
                                "`{op}` needs equal widths, found {} and {}",
                                show(&a),
                                show(&b)
                            ),
                            "this operator works bit-for-bit, so both sides must be the \
                             same width — `extend(x, N)` the narrow side, or slice the \
                             wide one (spec/02 section 3)",
                            vec![("op", op.to_string()), ("lhs", show(&a)), ("rhs", show(&b))],
                        );
                        Ty::Unknown
                    }
                }
            }
            _ => {
                // At least one side isn't a scalar Kind (e.g. two
                // different-shaped bundles/arrays, or a scalar vs. a
                // bundle) — the original code's fallback for this shape
                // is E0402 "needs equal widths" (its `kinds_differ` was
                // false here, since enum involvement was already handled
                // above).
                self.err_args(
                    cx.file,
                    lhs.span.join(rhs.span),
                    "E0402",
                    format!(
                        "`{op}` needs equal widths, found {} and {}",
                        show(&a),
                        show(&b)
                    ),
                    "this operator works bit-for-bit, so both sides must be the \
                     same width — `extend(x, N)` the narrow side, or slice the \
                     wide one (spec/02 section 3)",
                    vec![("op", op.to_string()), ("lhs", show(&a)), ("rhs", show(&b))],
                );
                Ty::Unknown
            }
        }
    }

    /// `<<`/`>>`: the result keeps the LEFT operand's type; the amount is
    /// a compile-time value or an unsigned signal. The actual width/kind
    /// rule is shared with the simulator via `width_rules::shift_result`
    /// (Stage 4, A1a) — see that function's doc comment for why the
    /// checker's static rule and the simulator's runtime rule (BUG-11)
    /// are deliberately different beyond this shared floor.
    fn shift_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        (lhs, lt): (&'a Expr, Ty<'a>),
        (rhs, rt): (&'a Expr, Ty<'a>),
    ) -> Ty<'a> {
        match rt {
            Ty::CtInt(v) if v < 0 => {
                self.err(
                    cx.file,
                    rhs.span,
                    "E0405",
                    format!("shift amount `{v}` is negative"),
                    "shift amounts count bits, so they are 0 or more",
                );
                return Ty::Unknown;
            }
            Ty::CtInt(_) | Ty::Bit | Ty::Bits(_) => {}
            Ty::Signed(_) => {
                self.err(
                    cx.file,
                    rhs.span,
                    "E0403",
                    "a shift amount cannot be `signed`",
                    "shift amounts are non-negative — cast with `unsigned(x)`",
                );
                return Ty::Unknown;
            }
            other => {
                self.err(
                    cx.file,
                    rhs.span,
                    "E0403",
                    format!("{} cannot be a shift amount", show(&other)),
                    "shift by a number or an unsigned signal",
                );
                return Ty::Unknown;
            }
        }
        let lhs_kind = match lt {
            Ty::Bit => crate::width_rules::Kind {
                width: 1,
                signed: false,
            },
            Ty::Bits(n) => crate::width_rules::Kind {
                width: width_u32(n),
                signed: false,
            },
            Ty::Signed(n) => crate::width_rules::Kind {
                width: width_u32(n),
                signed: true,
            },
            Ty::CtInt(_) => {
                self.err(
                    cx.file,
                    lhs.span,
                    "E0405",
                    "shifting a bare literal has no width to preserve",
                    "give it one first: `extend(1, N) << k`, or shift a sized \
                     signal",
                );
                return Ty::Unknown;
            }
            other => return self.not_numeric(cx, lhs.span, &other, "a shift"),
        };
        // `rt` is already known non-signed (every branch above that could
        // make it signed already returned) — the amount side of
        // `shift_result` can never fire here.
        match crate::width_rules::shift_result(
            lhs_kind,
            crate::width_rules::Kind {
                width: 0,
                signed: false,
            },
        ) {
            Ok(k) => kind_to_ty(k),
            Err(_) => unreachable!("amount signedness already checked above"),
        }
    }

    /// `{a, b, c}` — every part sized `bits` (or `bit`); result is the sum.
    pub(super) fn concat_ty(&mut self, cx: &mut Wcx<'a>, parts: &'a [Expr]) -> Ty<'a> {
        let mut sum: u128 = 0;
        let mut ok = true;
        for p in parts {
            let t = self.infer_ty(cx, p);
            match t {
                Ty::Bit => sum += 1,
                Ty::Bits(n) => sum += n,
                Ty::Unknown => ok = false,
                Ty::Signed(_) => {
                    self.err(
                        cx.file,
                        p.span,
                        "E0403",
                        "`signed` values do not concatenate directly",
                        "concatenation is bit-jugglery — make the intent visible \
                         with `unsigned(x)` first",
                    );
                    ok = false;
                }
                Ty::CtInt(_) => {
                    self.err(
                        cx.file,
                        p.span,
                        "E0405",
                        "a bare literal has no width inside `{...}`",
                        "every concat part needs a known width — `extend(1, N)` \
                         gives a literal one",
                    );
                    ok = false;
                }
                other => {
                    let _ = self.not_data(cx, p.span, &other);
                    ok = false;
                }
            }
        }
        if ok { bits(sum) } else { Ty::Unknown }
    }

    /// `{N{a, b}}` — replication: the inner concat repeated `count` times.
    /// Result width is `count * inner` bits; `count` must be a compile-time
    /// constant and the product a valid width (1..=`MAX_WIDTH`, E0410).
    pub(super) fn replicate_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        count: &'a Expr,
        parts: &'a [Expr],
    ) -> Ty<'a> {
        // The inner concat width (also reports signed/literal/non-data parts).
        let inner = match self.concat_ty(cx, parts) {
            Ty::Bit => 1u128,
            Ty::Bits(n) => n,
            _ => return Ty::Unknown,
        };
        // The count must be a compile-time constant.
        let c = match consteval::eval(count, &cx.env) {
            Ok(v) => v,
            Err(d) => {
                self.diags.push(d.with_file(cx.file));
                return Ty::Unknown;
            }
        };
        let total = i128::try_from(inner).ok().and_then(|n| c.checked_mul(n));
        match total {
            Some(t) if (1..=MAX_WIDTH).contains(&t) => bits(t as u128),
            _ => {
                self.err(
                    cx.file,
                    count.span,
                    "E0410",
                    match total {
                        Some(t) => format!("`{t}` is not a valid replicated width"),
                        None => "the replication width overflowed".to_string(),
                    },
                    format!(
                        "`{{N{{...}}}}` repeats its {inner}-bit group N times — N must be a \
                         constant giving a width between 1 and {MAX_WIDTH}"
                    ),
                );
                Ty::Unknown
            }
        }
    }

    /// The four builtins (spec/02 sections 1.7–1.8).
    pub(super) fn call_ty(
        &mut self,
        cx: &mut Wcx<'a>,
        e: &'a Expr,
        func: Builtin,
        args: &'a [Expr],
    ) -> Ty<'a> {
        let Some(x) = args.first() else {
            return Ty::Unknown; // parser enforces arity
        };
        let xt = self.infer_ty(cx, x);
        if matches!(xt, Ty::Unknown) {
            return Ty::Unknown;
        }
        match func {
            Builtin::Extend | Builtin::Trunc => {
                let Some(narg) = args.get(1) else {
                    return Ty::Unknown;
                };
                let Some(n) = self.eval_width(cx, narg) else {
                    return Ty::Unknown;
                };
                let name = if func == Builtin::Extend {
                    "extend"
                } else {
                    "trunc"
                };
                let m = match xt {
                    Ty::Bit => 1,
                    Ty::Bits(w) | Ty::Signed(w) => w,
                    Ty::CtInt(v) => {
                        // `extend(1, N)` is the idiom for giving a literal an
                        // explicit width; trunc of a literal is confusion.
                        if func == Builtin::Extend {
                            self.fit(cx, x.span, v, bits(n));
                            return bits(n);
                        }
                        self.err(
                            cx.file,
                            e.span,
                            "E0407",
                            "`trunc` of a bare literal does nothing useful",
                            "literals adapt to their context automatically — just \
                             write the smaller value",
                        );
                        return Ty::Unknown;
                    }
                    other => return self.not_numeric(cx, x.span, &other, name),
                };
                if func == Builtin::Extend && n < m {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        format!("`extend` to {n} bits would NARROW a {m}-bit value"),
                        "`extend(x, N)` only widens (N >= the current width) — \
                         to drop bits, say so with `trunc(x, N)` or a slice",
                    );
                    return Ty::Unknown;
                }
                if func == Builtin::Trunc && n > m {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        format!("`trunc` to {n} bits would WIDEN a {m}-bit value"),
                        "`trunc(x, N)` only narrows (it keeps the low N bits) — \
                         to add bits, say so with `extend(x, N)`",
                    );
                    return Ty::Unknown;
                }
                match xt {
                    Ty::Signed(_) => Ty::Signed(n),
                    _ => bits(n),
                }
            }
            Builtin::SignedCast => match xt {
                Ty::Bit => Ty::Signed(1),
                Ty::Bits(n) => Ty::Signed(n),
                Ty::Signed(_) => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        "this value is already `signed`",
                        "`signed(x)` reinterprets `bits` as `signed` — applying \
                         it twice means one of them is a mistake",
                    );
                    Ty::Unknown
                }
                Ty::CtInt(_) => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        "literals do not need a `signed(...)` cast",
                        "a literal already adapts to signed contexts — write it \
                         where the `signed[N]` is",
                    );
                    Ty::Unknown
                }
                other => self.not_numeric(cx, x.span, &other, "`signed`"),
            },
            Builtin::UnsignedCast => match xt {
                Ty::Signed(n) => bits(n),
                Ty::Bit | Ty::Bits(_) => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        "this value is already unsigned",
                        "`unsigned(x)` reinterprets `signed` as `bits` — this one \
                         was never signed",
                    );
                    Ty::Unknown
                }
                Ty::CtInt(_) => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        "literals do not need an `unsigned(...)` cast",
                        "a literal already adapts to its context",
                    );
                    Ty::Unknown
                }
                other => self.not_numeric(cx, x.span, &other, "`unsigned`"),
            },
            // `clog2` is compile-time only — reaching the value-typer means it
            // was used where a runtime value is expected. In a width/const/param
            // position it folds through `consteval` and never lands here.
            Builtin::Clog2 => {
                self.err(
                    cx.file,
                    e.span,
                    "E0407",
                    "`clog2` is a compile-time built-in and has no value here",
                    "use it where a constant is expected — a width `bits[clog2(N)]`, \
                     a `const`, or a parameter default",
                );
                Ty::Unknown
            }
            Builtin::SyncDoubleFlop | Builtin::SyncPulse => {
                let name = if func == Builtin::SyncDoubleFlop {
                    "sync.double_flop"
                } else {
                    "sync.pulse"
                };
                let Some(src_arg) = args.get(1) else {
                    return Ty::Unknown;
                };
                let Some(dst_arg) = args.get(2) else {
                    return Ty::Unknown;
                };
                let src_ty = self.infer_ty(cx, src_arg);
                let dst_ty = self.infer_ty(cx, dst_arg);
                if !matches!(src_ty, Ty::Clock) {
                    self.err(
                        cx.file,
                        src_arg.span,
                        "E0702",
                        format!("`{name}`'s second argument must be a declared `clock` name"),
                        "pass the clock the SIGNAL is already synchronous to — a \
                         `clock` declaration's own name, not a data expression",
                    );
                    return Ty::Unknown;
                }
                if !matches!(dst_ty, Ty::Clock) {
                    self.err(
                        cx.file,
                        dst_arg.span,
                        "E0702",
                        format!("`{name}`'s third argument must be a declared `clock` name"),
                        "pass the clock the RESULT should be synchronous to",
                    );
                    return Ty::Unknown;
                }
                let (crate::ast::ExprKind::Ident(src_name), crate::ast::ExprKind::Ident(dst_name)) =
                    (&src_arg.kind, &dst_arg.kind)
                else {
                    self.err(
                        cx.file,
                        e.span,
                        "E0702",
                        format!("`{name}`'s clock arguments must be bare clock names"),
                        "write the clock's own name directly, e.g. `clk_uart` — \
                         not a computed expression",
                    );
                    return Ty::Unknown;
                };
                if src_name == dst_name {
                    self.err(
                        cx.file,
                        e.span,
                        "E0702",
                        format!(
                            "`{name}`'s source and destination clocks are the same (`{src_name}`)"
                        ),
                        "synchronizing a signal to the clock it already belongs to \
                         is a no-op — check for a typo in one of the two clock names",
                    );
                    return Ty::Unknown;
                }
                let width_ok = matches!(xt, Ty::Bit) || matches!(xt, Ty::Bits(1));
                if !width_ok {
                    self.err(
                        cx.file,
                        x.span,
                        "E0703",
                        format!("`{name}`'s signal argument must be exactly 1 bit"),
                        "a 2-flop/toggle synchronizer is only sound for a single \
                         control bit — bit-independently synchronizing a wider bus \
                         is a real hardware hazard (bits can resolve on different \
                         destination cycles); a multi-bit-safe crossing (handshake \
                         or gray-coded FIFO) is not yet provided by this compiler",
                    );
                    return Ty::Unknown;
                }
                Ty::Bit
            }
            Builtin::Min | Builtin::Max => {
                let name = if func == Builtin::Min { "min" } else { "max" };
                let Some(b) = args.get(1) else {
                    return Ty::Unknown;
                };
                let bt = self.infer_ty(cx, b);
                if matches!(bt, Ty::Unknown) {
                    return Ty::Unknown;
                }
                if let (Ty::CtInt(_), Ty::CtInt(_)) = (xt, bt) {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        format!("`{name}` of two literals has no width"),
                        "give at least one operand a fixed width — a signal, or \
                         `extend(x, N)`",
                    );
                    return Ty::Unknown;
                }
                // Same operand rule as a comparison: equal kind + width (a
                // literal adapts to the sized side). The result is that type.
                self.matched_ty(cx, name, (x, xt), (b, bt))
            }
            Builtin::Abs => match xt {
                // Lossless like unary `-`: `abs(MIN)` needs the extra bit.
                Ty::Signed(n) => Ty::Signed(n + 1),
                Ty::CtInt(_) => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        "`abs` of a bare literal does nothing",
                        "write the non-negative value directly",
                    );
                    Ty::Unknown
                }
                other => {
                    self.err(
                        cx.file,
                        e.span,
                        "E0407",
                        format!("`abs` needs a `signed` value, found {}", show(&other)),
                        "absolute value is signed-only — `bits` are already \
                         non-negative; cast with `signed(x)` if needed",
                    );
                    Ty::Unknown
                }
            },
            Builtin::Nand | Builtin::Nor | Builtin::Xnor => {
                let name = match func {
                    Builtin::Nand => "`nand`",
                    Builtin::Nor => "`nor`",
                    _ => "`xnor`",
                };
                match xt {
                    // Negated reductions: a vector (or bit) collapses to one bit.
                    Ty::Bit | Ty::Bits(_) => Ty::Bit,
                    Ty::Signed(_) => {
                        self.err(
                            cx.file,
                            e.span,
                            "E0403",
                            "reductions work on `bits`, not `signed`",
                            "cast first: `nand(unsigned(x))` (spec/02 section 3)",
                        );
                        Ty::Unknown
                    }
                    other => self.not_numeric(cx, x.span, &other, name),
                }
            }
        }
    }
}
