use super::*;

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
            if narrow {
                let bits = v.as_i128().wrapping_neg() as u128;
                Val::new(bits, v.width, true)
            } else {
                Val::new_wide(wide::neg(&v.to_limbs(), v.width), v.width, true)
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
/// `expected_width` is the enclosing context's width (an assignment target,
/// `extend`'s target) — only `Shl`/`Shr` use it; pass `None` for a
/// self-determined position (see [`eval_ctx`]'s doc comment).
pub(super) fn binary_ctx(
    op: BinOp,
    l: Val,
    r: Val,
    expected_width: Option<u32>,
) -> Result<Val, String> {
    let unknown = l.unknown || r.unknown;
    binary_known(op, l, r, expected_width).map(|mut v| {
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
fn add(l: Val, r: Val) -> Val {
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
    .expect("checker already rejected mixed signed/unsigned operands");
    if !l.is_wide() && !r.is_wide() && k.width <= 128 {
        Val::new(
            l.as_i128().wrapping_add(r.as_i128()) as u128,
            k.width,
            k.signed,
        )
    } else {
        let (lw, rw) = wide_operands(l, r, k.width);
        Val::new_wide(wide::add(&lw, &rw, k.width), k.width, k.signed)
    }
}

fn sub(l: Val, r: Val) -> Val {
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
    .expect("checker already rejected mixed signed/unsigned operands");
    if !l.is_wide() && !r.is_wide() && k.width <= 128 {
        Val::new(
            l.as_i128().wrapping_sub(r.as_i128()) as u128,
            k.width,
            k.signed,
        )
    } else {
        let (lw, rw) = wide_operands(l, r, k.width);
        Val::new_wide(wide::sub(&lw, &rw, k.width), k.width, k.signed)
    }
}

fn mul(l: Val, r: Val) -> Val {
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
    .expect("checker already rejected mixed signed/unsigned operands");
    if !l.is_wide() && !r.is_wide() && k.width <= 128 {
        Val::new(
            l.as_i128().wrapping_mul(r.as_i128()) as u128,
            k.width,
            k.signed,
        )
    } else {
        let (lw, rw) = wide_operands(l, r, k.width);
        Val::new_wide(wide::mul(&lw, &rw, k.width), k.width, k.signed)
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

// `<<`/`>>` are context-determined on their left operand in real
// Verilog (ground-truthed against `iverilog`, BUG-11): the operand
// widens to the ENCLOSING width before the shift, not after —
// growing by the shift amount (the old fix here) or truncating to
// `l.width` unconditionally (the naive "spec says width preserved"
// fix) are both wrong in general; only "widen to the real
// context, then shift, keeping that width" matches Icarus for
// every case tried (same-width chain, narrower-operand-into-wider-
// context, standalone). `ctx_w` is `l`'s own width when no context
// is known (self-determined fallback — e.g. a bare test/eval
// expression with no assignment target).
fn shl(l: Val, r: Val, expected_width: Option<u32>) -> Result<Val, String> {
    let base = mimz_core::width_rules::shift_result(
        mimz_core::width_rules::Kind {
            width: l.width,
            signed: l.signed,
        },
        mimz_core::width_rules::Kind {
            width: r.width,
            signed: r.signed,
        },
    )
    .map_err(|_| "a shift amount cannot be `signed`".to_string())?;
    let ctx_w = expected_width
        .map(|w| w.max(base.width))
        .unwrap_or(base.width);
    Ok(if !l.is_wide() && ctx_w <= 128 {
        let widened = extend_bits(l, ctx_w);
        let shift = r.bits_small_or_zero();
        let bits = if shift >= 128 {
            0
        } else {
            widened.checked_shl(shift as u32).unwrap_or(0)
        };
        Val::new(bits, ctx_w, base.signed)
    } else {
        let widened = wide::extend(&l.to_limbs(), l.width, ctx_w, l.signed);
        let shift = r.bits_small_or_zero().min(ctx_w as u128) as u32;
        Val::new_wide(wide::shl(&widened, shift, ctx_w), ctx_w, base.signed)
    })
}

fn shr(l: Val, r: Val, expected_width: Option<u32>) -> Result<Val, String> {
    let base = mimz_core::width_rules::shift_result(
        mimz_core::width_rules::Kind {
            width: l.width,
            signed: l.signed,
        },
        mimz_core::width_rules::Kind {
            width: r.width,
            signed: r.signed,
        },
    )
    .map_err(|_| "a shift amount cannot be `signed`".to_string())?;
    let ctx_w = expected_width
        .map(|w| w.max(base.width))
        .unwrap_or(base.width);
    Ok(if !l.is_wide() && ctx_w <= 128 {
        let widened = extend_bits(l, ctx_w);
        let bits = if r.bits_small_or_zero() >= 128 {
            0
        } else {
            widened >> (r.bits_small_or_zero() as u32)
        };
        Val::new(bits, ctx_w, base.signed)
    } else {
        let widened = wide::extend(&l.to_limbs(), l.width, ctx_w, l.signed);
        let shift = r.bits_small_or_zero().min(ctx_w as u128) as u32;
        Val::new_wide(wide::shr(&widened, shift), ctx_w, base.signed)
    })
}

pub(super) fn binary_known(
    op: BinOp,
    l: Val,
    r: Val,
    expected_width: Option<u32>,
) -> Result<Val, String> {
    Ok(match op {
        BinOp::Add => add(l, r),
        BinOp::Sub => sub(l, r),
        BinOp::Mul => mul(l, r),
        BinOp::AddWrap => add_wrap(l, r),
        BinOp::SubWrap => sub_wrap(l, r),
        BinOp::MulWrap => mul_wrap(l, r),
        BinOp::BitAnd => bitand(l, r),
        BinOp::BitOr => bitor(l, r),
        BinOp::BitXor => bitxor(l, r),
        BinOp::Shl => return shl(l, r, expected_width),
        BinOp::Shr => return shr(l, r, expected_width),
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
            return Err("?? should have been lowered during elaboration".to_string());
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
