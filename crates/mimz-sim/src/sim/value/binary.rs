use super::*;
use crate::sim::Diag;

/// Promote `l`/`r` to matching-length limb vectors at `result_width`,
/// running the SAME sign-extension `extend_bits` already applies on the
/// narrow path. Shared by every wide-path binary operator arm below.
fn wide_operands(l: Val, r: Val, result_width: u32) -> (Vec<u64>, Vec<u64>) {
    (
        wide::extend(&l.to_limbs(), l.width, result_width, l.signed),
        wide::extend(&r.to_limbs(), r.width, result_width, r.signed),
    )
}

/// Widen `v`'s raw bits to (at least) `width`, sign-extending the new high
/// bits when `v` is signed and negative — zero-extending otherwise. If
/// `width <= v.width` this is just `v`'s own bits (truncation, if any,
/// happens later via `Val::new`'s masking). Shared by `Builtin::Extend`
/// (explicit, user-requested widening) and `eval_fn_call` (implicit
/// widening when a narrower argument binds to a wider parameter) — BUG-7:
/// binding used to mask the caller's raw bits to the param's width without
/// this extension, so a negative value went positive when the param was
/// wider (e.g. `signed[8]` `-128` into a `signed[16]` param read back as
/// `+128`, since the new high bits came from a zero-masked `Val::new` alone).
pub(super) fn extend_bits(v: Val, width: u32) -> u128 {
    // `masked()` panics on a `Wide` value — every caller of `extend_bits`
    // only invokes it on an operand already known to be `Small` (either
    // inside a dispatch's narrow-path `if` branch, or on a fn-call
    // argument, which stays narrow-only for now — see `docs/superpowers/
    // specs/2026-07-22-sim-wide-values-design.local.md`).
    let bits = v.masked();
    if width > v.width && v.signed && (bits >> (v.width - 1)) & 1 == 1 {
        bits | (mask(width) & !mask(v.width))
    } else {
        bits & mask(v.width)
    }
}

pub(super) fn unary(op: UnOp, v: Val) -> Val {
    let was_unknown = v.unknown;
    let mut r = unary_known(op, v);
    if was_unknown {
        r.unknown = true;
    }
    r
}

fn unary_known(op: UnOp, v: Val) -> Val {
    // Gate on `width <= 128`, not `is_wide()` alone: a `Bits::Small` value
    // can still be DECLARED wider than 128 bits (e.g. a `bits[200]` register
    // that currently holds a small magnitude) — the narrow arithmetic below
    // is only correct up to the value's actual bit width, so a width check
    // is required even when the representation happens to be `Small`.
    let narrow = !v.is_wide() && v.width <= 128;
    match op {
        UnOp::Neg => {
            // BUG-58 (docs/audit/bugs.md): `-x` for `x: signed[N]` is
            // lossless — `checker/widths/ops/mod.rs`'s own `unary_ty` types
            // it `Signed(N+1)`, the same "room for the MIN-value carry bit"
            // rule `Builtin::Abs` already gets (`fn_eval.rs`'s own
            // `width + 1`, BUG-35's fix). This arm used to keep `v.width`
            // unchanged, so `-(-128)` for `a: signed[8]` wrapped right back
            // to `-128` instead of the mathematically correct `+128` — real
            // Icarus gets this right for free (a context-determined `-a`
            // sign-extends `a` to the wider destination BEFORE negating),
            // so only the kernel disagreed with its own type system.
            let out_width = v.width + 1;
            if narrow {
                let bits = v.as_i128().wrapping_neg() as u128;
                Val::new(bits, out_width, true)
            } else {
                let extended = wide::extend(&v.to_limbs(), v.width, out_width, v.signed);
                Val::new_wide(wide::neg(&extended, out_width), out_width, true)
            }
        }
        UnOp::BitNot => {
            if narrow {
                Val::new(!v.masked(), v.width, v.signed)
            } else {
                Val::new_wide(wide::not(&v.to_limbs()), v.width, v.signed)
            }
        }
        UnOp::LogicNot => Val::new((!(v.lsb())) & 1, 1, false),
        UnOp::RedAnd => {
            let ones = if narrow {
                v.masked() == mask(v.width)
            } else {
                wide::count_ones(&v.to_limbs()) == v.width
            };
            Val::new(ones as u128, 1, false)
        }
        UnOp::RedOr => {
            let any = if narrow {
                v.masked() != 0
            } else {
                !wide::is_zero(&v.to_limbs())
            };
            Val::new(any as u128, 1, false)
        }
        UnOp::RedXor => {
            let ones = if narrow {
                v.masked().count_ones()
            } else {
                wide::count_ones(&v.to_limbs())
            };
            Val::new((ones & 1) as u128, 1, false)
        }
    }
}

/// Evaluate a binary operator over two already-evaluated operands.
/// `const_amount` is `Shl`/`Shr`'s compile-time shift amount, if the
/// source expression had one (see [`eval`]'s `Binary` arm) — every other
/// operator ignores it; pass `None` when there is none.
pub(super) fn binary_ctx(
    op: BinOp,
    l: Val,
    r: Val,
    const_amount: Option<u128>,
    span: mimz_core::span::Span,
) -> Result<Val, Box<Diag>> {
    let unknown = l.unknown || r.unknown;
    binary_known(op, l, r, const_amount, span).map(|mut v| {
        if unknown {
            v.unknown = true;
        }
        v
    })
}

// Lossless growth (spec/02 section 3). Operate on the SIGN-EXTENDED
// values (`as_i128`) so a negative signed operand is widened correctly
// before the result grows — matching Verilog's signed arithmetic. For
// unsigned operands `as_i128` is the plain magnitude, so this is
// identical to a raw-bit add/mul. (The wrapping family below keeps the
// operand width, where the raw-bit op is already correct mod 2^width.)
fn math_rule_err(err: mimz_core::width_rules::RuleError, span: mimz_core::span::Span) -> Box<Diag> {
    match err {
        mimz_core::width_rules::RuleError::KindMismatch { .. } => {
            Box::new(Diag::new(span, "cannot mix signed and unsigned operands").with_code("S0240"))
        }
        _ => unreachable!("lossless_result never returns other RuleError variants"),
    }
}

fn add(l: Val, r: Val, span: mimz_core::span::Span) -> Result<Val, Box<Diag>> {
    let k = mimz_core::width_rules::lossless_result(
        mimz_core::width_rules::Kind {
            width: l.width,
            signed: l.signed,
        },
        mimz_core::width_rules::Kind {
            width: r.width,
            signed: r.signed,
        },
        false,
    )
    .map_err(|e| math_rule_err(e, span))?;
    if !l.is_wide() && !r.is_wide() && k.width <= 128 {
        Ok(Val::new(
            l.as_i128().wrapping_add(r.as_i128()) as u128,
            k.width,
            k.signed,
        ))
    } else {
        let (lw, rw) = wide_operands(l, r, k.width);
        Ok(Val::new_wide(
            wide::add(&lw, &rw, k.width),
            k.width,
            k.signed,
        ))
    }
}

fn sub(l: Val, r: Val, span: mimz_core::span::Span) -> Result<Val, Box<Diag>> {
    let k = mimz_core::width_rules::lossless_result(
        mimz_core::width_rules::Kind {
            width: l.width,
            signed: l.signed,
        },
        mimz_core::width_rules::Kind {
            width: r.width,
            signed: r.signed,
        },
        false,
    )
    .map_err(|e| math_rule_err(e, span))?;
    if !l.is_wide() && !r.is_wide() && k.width <= 128 {
        Ok(Val::new(
            l.as_i128().wrapping_sub(r.as_i128()) as u128,
            k.width,
            k.signed,
        ))
    } else {
        let (lw, rw) = wide_operands(l, r, k.width);
        Ok(Val::new_wide(
            wide::sub(&lw, &rw, k.width),
            k.width,
            k.signed,
        ))
    }
}

fn mul(l: Val, r: Val, span: mimz_core::span::Span) -> Result<Val, Box<Diag>> {
    let k = mimz_core::width_rules::lossless_result(
        mimz_core::width_rules::Kind {
            width: l.width,
            signed: l.signed,
        },
        mimz_core::width_rules::Kind {
            width: r.width,
            signed: r.signed,
        },
        true,
    )
    .map_err(|e| math_rule_err(e, span))?;
    if !l.is_wide() && !r.is_wide() && k.width <= 128 {
        Ok(Val::new(
            l.as_i128().wrapping_mul(r.as_i128()) as u128,
            k.width,
            k.signed,
        ))
    } else {
        let (lw, rw) = wide_operands(l, r, k.width);
        Ok(Val::new_wide(
            wide::mul(&lw, &rw, k.width),
            k.width,
            k.signed,
        ))
    }
}

// Wrapping family: keep operand width. A bare integer literal's `Val`
// keeps its own minimal natural width (never pre-widened to match the
// other operand, unlike the checker's compile-time-only "adapting"
// fiction for `CtInt` — see `matched_ty`), so both operands must be
// widened to `wmax` here before `matched_result` can find their
// `Kind`s equal. The `.unwrap_or` reproduces the original
// `l.signed || r.signed` bookkeeping for the one case
// `matched_result` can still reject after widening (mismatched
// signedness) — real fallback code, not a placeholder. `k` is
// computed from `l.signed`/`r.signed` (field reads, not moves)
// BEFORE the dispatch below moves `l`/`r` into `extend_bits`/
// `wide_operands` — `Val` losing `Copy` (Task 2) means the old
// ordering (widen first, compute `k` after) would no longer
// compile.
fn add_wrap(l: Val, r: Val) -> Val {
    let wmax = l.width.max(r.width);
    let k = mimz_core::width_rules::matched_result(
        mimz_core::width_rules::Kind {
            width: wmax,
            signed: l.signed,
        },
        mimz_core::width_rules::Kind {
            width: wmax,
            signed: r.signed,
        },
    )
    .unwrap_or(mimz_core::width_rules::Kind {
        width: wmax,
        signed: l.signed || r.signed,
    });
    if !l.is_wide() && !r.is_wide() && wmax <= 128 {
        let lw = extend_bits(l, wmax);
        let rw = extend_bits(r, wmax);
        Val::new(lw.wrapping_add(rw), k.width, k.signed)
    } else {
        let (lw, rw) = wide_operands(l, r, wmax);
        Val::new_wide(wide::add(&lw, &rw, k.width), k.width, k.signed)
    }
}

fn sub_wrap(l: Val, r: Val) -> Val {
    let wmax = l.width.max(r.width);
    let k = mimz_core::width_rules::matched_result(
        mimz_core::width_rules::Kind {
            width: wmax,
            signed: l.signed,
        },
        mimz_core::width_rules::Kind {
            width: wmax,
            signed: r.signed,
        },
    )
    .unwrap_or(mimz_core::width_rules::Kind {
        width: wmax,
        signed: l.signed || r.signed,
    });
    if !l.is_wide() && !r.is_wide() && wmax <= 128 {
        let lw = extend_bits(l, wmax);
        let rw = extend_bits(r, wmax);
        Val::new(lw.wrapping_sub(rw), k.width, k.signed)
    } else {
        let (lw, rw) = wide_operands(l, r, wmax);
        Val::new_wide(wide::sub(&lw, &rw, k.width), k.width, k.signed)
    }
}

fn mul_wrap(l: Val, r: Val) -> Val {
    let wmax = l.width.max(r.width);
    let k = mimz_core::width_rules::matched_result(
        mimz_core::width_rules::Kind {
            width: wmax,
            signed: l.signed,
        },
        mimz_core::width_rules::Kind {
            width: wmax,
            signed: r.signed,
        },
    )
    .unwrap_or(mimz_core::width_rules::Kind {
        width: wmax,
        signed: l.signed || r.signed,
    });
    if !l.is_wide() && !r.is_wide() && wmax <= 128 {
        let lw = extend_bits(l, wmax);
        let rw = extend_bits(r, wmax);
        Val::new(lw.wrapping_mul(rw), k.width, k.signed)
    } else {
        let (lw, rw) = wide_operands(l, r, wmax);
        Val::new_wide(wide::mul(&lw, &rw, k.width), k.width, k.signed)
    }
}

fn bitand(l: Val, r: Val) -> Val {
    let wmax = l.width.max(r.width);
    let k = mimz_core::width_rules::matched_result(
        mimz_core::width_rules::Kind {
            width: wmax,
            signed: l.signed,
        },
        mimz_core::width_rules::Kind {
            width: wmax,
            signed: r.signed,
        },
    )
    .unwrap_or(mimz_core::width_rules::Kind {
        width: wmax,
        signed: l.signed || r.signed,
    });
    if !l.is_wide() && !r.is_wide() && wmax <= 128 {
        let lw = extend_bits(l, wmax);
        let rw = extend_bits(r, wmax);
        Val::new(lw & rw, k.width, k.signed)
    } else {
        let (lw, rw) = wide_operands(l, r, wmax);
        Val::new_wide(wide::bitand(&lw, &rw), k.width, k.signed)
    }
}

fn bitor(l: Val, r: Val) -> Val {
    let wmax = l.width.max(r.width);
    let k = mimz_core::width_rules::matched_result(
        mimz_core::width_rules::Kind {
            width: wmax,
            signed: l.signed,
        },
        mimz_core::width_rules::Kind {
            width: wmax,
            signed: r.signed,
        },
    )
    .unwrap_or(mimz_core::width_rules::Kind {
        width: wmax,
        signed: l.signed || r.signed,
    });
    if !l.is_wide() && !r.is_wide() && wmax <= 128 {
        let lw = extend_bits(l, wmax);
        let rw = extend_bits(r, wmax);
        Val::new(lw | rw, k.width, k.signed)
    } else {
        let (lw, rw) = wide_operands(l, r, wmax);
        Val::new_wide(wide::bitor(&lw, &rw), k.width, k.signed)
    }
}

fn bitxor(l: Val, r: Val) -> Val {
    let wmax = l.width.max(r.width);
    let k = mimz_core::width_rules::matched_result(
        mimz_core::width_rules::Kind {
            width: wmax,
            signed: l.signed,
        },
        mimz_core::width_rules::Kind {
            width: wmax,
            signed: r.signed,
        },
    )
    .unwrap_or(mimz_core::width_rules::Kind {
        width: wmax,
        signed: l.signed || r.signed,
    });
    if !l.is_wide() && !r.is_wide() && wmax <= 128 {
        let lw = extend_bits(l, wmax);
        let rw = extend_bits(r, wmax);
        Val::new(lw ^ rw, k.width, k.signed)
    } else {
        let (lw, rw) = wide_operands(l, r, wmax);
        Val::new_wide(wide::bitxor(&lw, &rw), k.width, k.signed)
    }
}

// `<<`/`>>` are context-determined on their left operand in real Verilog
// (ground-truthed against `iverilog`, BUG-11): the operand widens to the
// ENCLOSING width before the shift, not after. BUG-30 replaced full
// context-threading with per-node growth (`<<` grows, `>>` doesn't) — right
// for a LONE shift (its declared type already bounds the value, no ambient
// context needed), but BUG-34 (`docs/audit/bugs.md`) found this loses the
// original context-widening for a FUSED chain (`(a >> b) << c`, no named
// intermediate): `>>` alone doesn't grow, so its self-determined result is
// too narrow, and the outer `<<`'s later re-extension of that
// already-computed value is too late to recover the right fill bits.
// `eval_shift_chain` below is the scoped revival — walks a whole fused
// shift chain, widens the BASE operand ONCE to the chain's final width,
// then folds each shift at that fixed width, matching Icarus exactly.
/// Maps `width_rules::shift_result`'s `Err` to a `Diag` — shared by `shl`/
/// `shr` since both call the same shared rule.
fn shift_rule_err(
    err: mimz_core::width_rules::RuleError,
    span: mimz_core::span::Span,
) -> Box<Diag> {
    match err {
        mimz_core::width_rules::RuleError::ShiftAmountSigned => {
            Box::new(Diag::new(span, "a shift amount cannot be `signed`").with_code("S0221"))
        }
        mimz_core::width_rules::RuleError::ShiftGrowthTooWide { lhs, growth } => Box::new(
            Diag::new(
                span,
                format!(
                    "shifting bits[{}] wider by {growth} bits exceeds the \
                     {}-bit width limit",
                    lhs.width,
                    mimz_core::width_rules::MAX_WIDTH
                ),
            )
            .with_code("S0222"),
        ),
        _ => unreachable!("shift_result never returns any other RuleError variant"),
    }
}

/// `<<`: BUG-30 (`docs/audit/bugs.md`) — grows so the result always holds
/// every value it could possibly produce, matching `width_rules::shift_result`
/// exactly (Chisel's rule: exact growth for a compile-time `const_amount`,
/// worst-case `2^width(r) - 1` growth for a genuine runtime `r`). No
/// enclosing context is needed anymore — the grown width IS the correct
/// width in every position, self-determined or not.
fn shl(
    l: Val,
    r: Val,
    const_amount: Option<u128>,
    span: mimz_core::span::Span,
) -> Result<Val, Box<Diag>> {
    let result = mimz_core::width_rules::shift_result(
        mimz_core::width_rules::Kind {
            width: l.width,
            signed: l.signed,
        },
        mimz_core::width_rules::Kind {
            width: r.width,
            signed: r.signed,
        },
        const_amount,
        true,
    )
    .map_err(|e| shift_rule_err(e, span))?;
    let w = result.width;
    Ok(if !l.is_wide() && w <= 128 {
        let widened = extend_bits(l, w);
        let shift = r.bits_small_or_zero();
        let bits = if shift >= 128 {
            0
        } else {
            widened.checked_shl(shift as u32).unwrap_or(0)
        };
        Val::new(bits, w, result.signed)
    } else {
        let widened = wide::extend(&l.to_limbs(), l.width, w, l.signed);
        let shift = r.bits_small_or_zero().min(w as u128) as u32;
        Val::new_wide(wide::shl(&widened, shift, w), w, result.signed)
    })
}

/// `>>`: right-shifting only ever reduces a value's magnitude, so the left
/// operand's own width already bounds the result — unchanged by BUG-30
/// (`grows: false`), no `const_amount` needed.
fn shr(l: Val, r: Val, span: mimz_core::span::Span) -> Result<Val, Box<Diag>> {
    let result = mimz_core::width_rules::shift_result(
        mimz_core::width_rules::Kind {
            width: l.width,
            signed: l.signed,
        },
        mimz_core::width_rules::Kind {
            width: r.width,
            signed: r.signed,
        },
        None,
        false,
    )
    .map_err(|e| shift_rule_err(e, span))?;
    let w = result.width;
    Ok(if !l.is_wide() && w <= 128 {
        let widened = extend_bits(l, w);
        let bits = if r.bits_small_or_zero() >= 128 {
            0
        } else {
            widened >> (r.bits_small_or_zero() as u32)
        };
        Val::new(bits, w, result.signed)
    } else {
        let widened = wide::extend(&l.to_limbs(), l.width, w, l.signed);
        let shift = r.bits_small_or_zero().min(w as u128) as u32;
        Val::new_wide(wide::shr(&widened, shift), w, result.signed)
    })
}

/// Walks down `e`'s left spine through consecutive `Shl`/`Shr` nodes,
/// returning the first non-shift BASE expression and the ordered chain of
/// `(op, amount-expr)` from innermost to outermost. A lone shift (nothing
/// nested on its own left operand) returns a one-entry chain — not a
/// special case, just the chain-of-one case `eval_shift_chain` below
/// still has to handle correctly (and does, identically to plain `shl`/
/// `shr`, since extending a value to its own unchanged width is a no-op).
fn collect_shift_chain(e: &Expr) -> (&Expr, Vec<(BinOp, &Expr)>) {
    if let ExprKind::Binary { op, lhs, rhs } = &e.kind
        && matches!(op, BinOp::Shl | BinOp::Shr)
    {
        let (base, mut chain) = collect_shift_chain(lhs);
        chain.push((*op, rhs.as_ref()));
        return (base, chain);
    }
    (e, Vec::new())
}

/// BUG-34: evaluate a whole fused `Shl`/`Shr` chain as one unit instead of
/// per-node. Pass 1 resolves each step's `Kind` bottom-up (the exact rule
/// `shl`/`shr` already apply per-node) purely to learn the chain's FINAL
/// width/signedness up front. Pass 2 extends the base operand to that
/// final width ONCE — sign-extending iff the base is signed — then folds
/// every step as a plain logical shift at that fixed width, matching how
/// real Verilog sizes a fused shift expression (ground-truthed against
/// `iverilog` on BUG-34's own repro: `(p2 >> 4) << 7` for `p2:
/// signed[16] = -9563` gives `-76544`, not the per-node result `447744`).
pub(super) fn eval_shift_chain<R: super::Resolver>(r: &mut R, e: &Expr) -> Result<Val, Box<Diag>> {
    let (base_expr, chain) = collect_shift_chain(e);
    let base = super::eval(r, base_expr)?;

    let mut width = base.width;
    let mut signed = base.signed;
    let mut unknown = base.unknown;
    let mut steps: Vec<(BinOp, Val)> = Vec::with_capacity(chain.len());
    for &(op, rhs_expr) in &chain {
        let rv = super::eval(r, rhs_expr)?;
        unknown |= rv.unknown;
        let const_amount = super::const_eval(rhs_expr, r.ints())
            .ok()
            .and_then(|v| u128::try_from(v).ok());
        let k = mimz_core::width_rules::shift_result(
            mimz_core::width_rules::Kind { width, signed },
            mimz_core::width_rules::Kind {
                width: rv.width,
                signed: rv.signed,
            },
            const_amount,
            matches!(op, BinOp::Shl),
        )
        .map_err(|err| shift_rule_err(err, e.span))?;
        width = k.width;
        signed = k.signed;
        steps.push((op, rv));
    }

    let mut limbs = wide::extend(&base.to_limbs(), base.width, width, base.signed);
    for (op, rv) in &steps {
        let shift = rv.bits_small_or_zero().min(width as u128) as u32;
        limbs = match op {
            BinOp::Shl => wide::shl(&limbs, shift, width),
            BinOp::Shr => wide::shr(&limbs, shift),
            _ => unreachable!("collect_shift_chain only ever collects Shl/Shr"),
        };
    }
    let mut result = Val::new_wide(limbs, width, signed);
    result.unknown = unknown;
    Ok(result)
}

pub(super) fn binary_known(
    op: BinOp,
    l: Val,
    r: Val,
    const_amount: Option<u128>,
    span: mimz_core::span::Span,
) -> Result<Val, Box<Diag>> {
    Ok(match op {
        BinOp::Add => add(l, r, span)?,
        BinOp::Sub => sub(l, r, span)?,
        BinOp::Mul => mul(l, r, span)?,
        BinOp::AddWrap => add_wrap(l, r),
        BinOp::SubWrap => sub_wrap(l, r),
        BinOp::MulWrap => mul_wrap(l, r),
        BinOp::BitAnd => bitand(l, r),
        BinOp::BitOr => bitor(l, r),
        BinOp::BitXor => bitxor(l, r),
        BinOp::Shl => return shl(l, r, const_amount, span),
        BinOp::Shr => return shr(l, r, span),
        BinOp::Eq => Val::new(cmp_eq(l, r) as u128, 1, false),
        BinOp::Ne => Val::new(!cmp_eq(l, r) as u128, 1, false),
        BinOp::Lt => Val::new(cmp_lt(l, r) as u128, 1, false),
        BinOp::Le => Val::new(
            (cmp_lt(l.clone(), r.clone()) || cmp_eq(l, r)) as u128,
            1,
            false,
        ),
        BinOp::Gt => Val::new(
            (!cmp_lt(l.clone(), r.clone()) && !cmp_eq(l, r)) as u128,
            1,
            false,
        ),
        BinOp::Ge => Val::new((!cmp_lt(l, r)) as u128, 1, false),
        BinOp::LogicAnd => Val::new(l.lsb() & r.lsb(), 1, false),
        BinOp::LogicOr => Val::new(l.lsb() | r.lsb(), 1, false),
        // `??` is always rewritten to `IfExpr` by `Rw::expr` during
        // elaboration (crates/mimz-sim/src/sim/elaborate.rs) before the
        // kernel ever calls `binary_known` — so this arm is unreachable in
        // practice. Still a typed error rather than a panic: this function
        // returns `Result`, and a future caller of `binary_known` that skips
        // elaboration must get a diagnosable error, not a crashed process.
        BinOp::Coalesce => {
            return Err(Box::new(
                Diag::new(span, "?? should have been lowered during elaboration")
                    .with_code("S0222"),
            ));
        }
    })
}

pub(super) fn cmp_lt(l: Val, r: Val) -> bool {
    let wmax = l.width.max(r.width);
    // Gate on width, not `is_wide()` alone — same reasoning as
    // `unary_known`: a `Bits::Small` value can still be declared wider than
    // 128 bits, and `as_i128()`/`masked()` are narrow-path-only.
    if !l.is_wide() && !r.is_wide() && l.width <= 128 && r.width <= 128 {
        if l.signed || r.signed {
            l.as_i128() < r.as_i128()
        } else {
            l.masked() < r.masked()
        }
    } else {
        // Capture `signed` BEFORE `wide_operands` moves `l`/`r` — `Val`
        // losing `Copy` (Task 2) means reading `l.signed`/`r.signed` after
        // the move (as the brief's own draft code did) does not compile.
        let signed = l.signed || r.signed;
        let (lw, rw) = wide_operands(l, r, wmax);
        let ord = if signed {
            wide::cmp_signed(&lw, &rw, wmax)
        } else {
            wide::cmp_unsigned(&lw, &rw)
        };
        ord == std::cmp::Ordering::Less
    }
}
pub(super) fn cmp_eq(l: Val, r: Val) -> bool {
    if !l.is_wide() && !r.is_wide() && l.width <= 128 && r.width <= 128 {
        if l.signed || r.signed {
            l.as_i128() == r.as_i128()
        } else {
            l.masked() == r.masked()
        }
    } else {
        let wmax = l.width.max(r.width);
        let (lw, rw) = wide_operands(l, r, wmax);
        lw == rw
    }
}
