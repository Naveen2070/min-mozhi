//! Pass 5 — compile-time constant evaluation.
//!
//! Evaluates file-level `const` declarations (top to bottom, each may use
//! the ones above it) and provides [`eval`] for every other place a
//! constant is required (`repeat` bounds, parameter defaults). Values are
//! [`ConstVal`] — arbitrary-width two's-complement integers, bounded only
//! by `width_rules::MAX_WIDTH` (BUG-13 layer 2); overflow past that
//! ceiling is an error, never a silent wrap (E0202).
//!
//! What does NOT const-evaluate (E0201): signal names, wrapping operators
//! (`+%` needs a bit width), `match`, concat/index/slice, builtins. The
//! error says which and why — this list shrinks as the checker grows.

use std::collections::HashMap;

use crate::ast::{BinOp, Expr, ExprKind, TopItem, UnOp};
use crate::bits::{self, Bits};
use crate::diag::Diag;
use crate::width_rules::MAX_WIDTH;

use super::Checker;

/// A compile-time integer: an arbitrary-width two's-complement value.
/// Mirrors `mimz-sim`'s `Val` (minus `unknown` — a constant is always
/// fully known): `signed: false` means `bits` is an unsigned magnitude at
/// its own tight `width`; `signed: true` means `bits` is a two's-
/// complement negative value at its own tight `width`. Always at its
/// MINIMAL width — every constructor in this module runs the result
/// through `bits::shrink` before returning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstVal {
    pub bits: Bits,
    pub width: u32,
    pub signed: bool,
}

impl ConstVal {
    pub fn zero() -> ConstVal {
        ConstVal {
            bits: Bits::Small(0),
            width: 1,
            signed: false,
        }
    }
    pub fn from_bool(b: bool) -> ConstVal {
        ConstVal {
            bits: Bits::Small(b as u128),
            width: 1,
            signed: false,
        }
    }
    /// Build a `ConstVal` from a plain `i128` (the shape every call site
    /// still producing an `i128` — e.g. from a not-yet-migrated caller —
    /// needs). Mirrors `mimz-sim`'s `Val::from_int`'s own two-branch rule.
    pub fn from_i128(v: i128) -> ConstVal {
        if v >= 0 {
            let w = bits::natural_width(&Bits::Small(v as u128));
            ConstVal {
                bits: Bits::Small(v as u128),
                width: w,
                signed: false,
            }
        } else {
            // Two's complement of `v` at 128 bits, then shrink to the
            // true minimal width (mirrors `Val::from_int`'s negative
            // branch, generalized via `bits::shrink` instead of the
            // fixed `129 - leading_ones()` formula).
            let (shrunk, w, _) = bits::shrink(&Bits::Small(v as u128), 128, true);
            ConstVal {
                bits: shrunk,
                width: w,
                signed: true,
            }
        }
    }
    pub fn is_negative(&self) -> bool {
        self.signed
    }
    pub fn is_zero(&self) -> bool {
        matches!(&self.bits, Bits::Small(0))
    }
    /// True for the literal value `1` (width 1, unsigned) — the other half
    /// of the "is this a bare 0/1 used as a bit" check that condition/
    /// logical-operator positions need (`bits::shrink`'s convention keeps
    /// `1` at its tight width of 1, same as `0`).
    pub fn is_one(&self) -> bool {
        !self.signed && self.width == 1 && matches!(&self.bits, Bits::Small(1))
    }
    /// This value's limbs, at ITS OWN width (not a target width) —
    /// callers extend further themselves via `bits::to_limbs` when they
    /// need a wider container.
    fn limbs(&self) -> Vec<u64> {
        bits::to_limbs(&self.bits, self.width)
    }
    /// This value as `i128`, saturating to `i128::MAX`/`i128::MIN` if it
    /// doesn't fit — used only for internal bookkeeping that was never
    /// part of BUG-13's scope (`repeat`-loop bounds, `const if`
    /// conditions, module-parameter worklist bindings): all already
    /// bounded far below i128's range by their own existing sanity caps
    /// (`REPEAT_BUDGET`, `MAX_CONFIGS`), so a value this large is already
    /// headed for the same "reject/degrade" path a too-large i128 would
    /// have taken before this module existed.
    pub fn to_i128_saturating(&self) -> i128 {
        let limbs = self.limbs();
        let lo = limbs.first().copied().unwrap_or(0) as u128
            | ((limbs.get(1).copied().unwrap_or(0) as u128) << 64);
        if self.width > 128 {
            return if self.signed { i128::MIN } else { i128::MAX };
        }
        if self.signed {
            let m = bits::mask(self.width);
            let b = lo & m;
            if self.width >= 1 && (b >> (self.width - 1)) & 1 == 1 {
                (b | !m) as i128
            } else {
                b as i128
            }
        } else {
            i128::try_from(lo).unwrap_or(i128::MAX)
        }
    }
}

impl std::fmt::Display for ConstVal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            bits::bits_to_decimal_string(&self.bits, self.width, self.signed)
        )
    }
}

/// Environment for one evaluation: name -> value. Built from file consts
/// plus (in `names.rs`) module consts and enclosing `repeat` variables.
/// `pub` so the Verilog emitter and `mimz-sim`'s simulator can fold the
/// same constants when they unroll `repeat` (both share this evaluator
/// rather than reimplementing it).
pub type Env = HashMap<String, ConstVal>;

impl<'a> Checker<'a> {
    /// Evaluate every file-level `const`, in source order, into
    /// `self.file_consts`. Duplicates are E0004; a const referring to a
    /// LATER const fails naturally as E0201 (evaluation is top to bottom).
    pub(super) fn eval_consts(&mut self) {
        for file in 0..self.files.len() {
            for item in &self.files[file].items {
                let TopItem::Const(c) = item else { continue };
                if self.file_consts[file].contains_key(&c.name.name) {
                    self.err(
                        file,
                        c.name.span,
                        "E0004",
                        format!(
                            "const `{}` is defined more than once in this file",
                            c.name.name
                        ),
                        "rename one of them — consts are file-local, so the names only \
                         need to be unique within this file",
                    );
                    continue;
                }
                match eval(&c.value, &self.file_consts[file]) {
                    Ok(v) => {
                        self.file_consts[file].insert(c.name.name.clone(), v);
                    }
                    Err(d) => self.diags.push(d.with_file(file)),
                }
            }
        }
    }
}

/// A common width, wide enough to hold either operand's true signed
/// value with one bit of headroom to spare — needed whenever an operand
/// might be an unsigned value sitting at its own TIGHT natural width
/// (whose top bit is set precisely because it's the highest magnitude
/// bit, not a sign bit); comparing/combining it at exactly that width
/// under signed two's-complement rules would misread it as negative.
/// `shrink`'s later pass always re-tightens the result, so the one extra
/// bit of headroom here costs nothing.
fn common_width(l: &ConstVal, r: &ConstVal) -> u32 {
    l.width.max(r.width) + 1
}

fn extend_to(v: &ConstVal, w: u32) -> Vec<u64> {
    crate::wide::extend(&v.limbs(), v.width, w, v.signed)
}

/// Run `add`/`sub`/`mul`/`shl` at a width wide enough that the true
/// mathematical result can NEVER be truncated, then shrink to minimal
/// width and check that against `MAX_WIDTH`. This is the one place
/// const-eval's "overflow is a clean error, never a silent wrap" contract
/// (this module's own doc comment) gets enforced past 128 bits.
#[allow(clippy::result_large_err)]
fn grown_op(
    e: &Expr,
    la: &[u64],
    ra: &[u64],
    safe_width: u32,
    f: impl Fn(&[u64], &[u64], u32) -> Vec<u64>,
) -> Result<ConstVal, Diag> {
    let result_bits = bits::from_limbs(f(la, ra, safe_width), safe_width);
    let negative = bits::top_bit_set(&result_bits, safe_width);
    let (shrunk, width, signed) = bits::shrink(&result_bits, safe_width, negative);
    if width > MAX_WIDTH as u32 {
        return Err(overflow(e));
    }
    Ok(ConstVal {
        bits: shrunk,
        width,
        signed,
    })
}

/// Evaluate `e` to a compile-time value, or explain why it is not one.
/// The returned diagnostic carries its code but NOT a file index — the
/// caller stamps that (`.with_file(...)`), since only it knows the file.
#[allow(clippy::result_large_err)]
pub fn eval(e: &Expr, env: &Env) -> Result<ConstVal, Diag> {
    let not_const = |what: &str, why: &str| {
        Err(
            Diag::new(e.span, format!("{what} is not a compile-time constant"))
                .with_code("E0201")
                .with_help(why.to_string()),
        )
    };
    match &e.kind {
        ExprKind::Int { value, .. } => {
            let w = bits::natural_width(value);
            if w > MAX_WIDTH as u32 {
                return Err(overflow(e));
            }
            // `retag`, not a raw `.clone()` — the lexer's `Bits::Wide`
            // vector may hold more limbs than `w` needs (e.g. a hex
            // literal with leading zero digits); `retag` re-packs it to
            // EXACTLY `wide::limb_count(w)` elements, preserving the
            // Global Constraints invariant every `Bits::Wide` must hold.
            Ok(ConstVal {
                bits: bits::retag(value, w),
                width: w,
                signed: false,
            })
        }
        ExprKind::Bool(b) => Ok(ConstVal::from_bool(*b)),
        ExprKind::Ident(name) => match env.get(name) {
            Some(v) => Ok(v.clone()),
            None => not_const(
                &format!("`{name}`"),
                "only `const` values, literals, and `repeat` variables work here — \
                 consts are evaluated top to bottom, so a const can only use the \
                 ones declared above it",
            ),
        },
        ExprKind::Unary { op, expr } => {
            let v = eval(expr, env)?;
            match op {
                UnOp::Neg => {
                    if v.is_zero() {
                        return Ok(ConstVal::zero());
                    }
                    // One extra bit is always enough headroom to negate
                    // any value at its own tight width without truncating.
                    let safe_width = v.width + 1;
                    let limbs = extend_to(&v, safe_width);
                    let zero = vec![0u64; limbs.len()];
                    grown_op(e, &zero, &limbs, safe_width, |z, l, w| {
                        crate::wide::sub(z, l, w)
                    })
                }
                UnOp::LogicNot => Ok(ConstVal::from_bool(v.is_zero())),
                _ => not_const(
                    "this operator",
                    "bitwise operators need a known bit width, which constants \
                     do not have — use arithmetic and comparisons instead",
                ),
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let l = eval(lhs, env)?;
            let r = eval(rhs, env)?;
            match op {
                BinOp::Add => {
                    // +2, not +1: two n-bit UNSIGNED-at-their-own-tight-
                    // width magnitudes (top bit legitimately part of the
                    // magnitude, no sign headroom reserved) can sum to just
                    // under 2^(n+1) — e.g. (2^127-1)+(2^127-1) needs exactly
                    // 128 bits, whose own top bit is set without the true
                    // sum being negative. +1 headroom is only safe for
                    // already-signed n-bit inputs; +2 is safe unconditionally.
                    let sw = l.width.max(r.width) + 2;
                    let la = extend_to(&l, sw);
                    let ra = extend_to(&r, sw);
                    grown_op(e, &la, &ra, sw, crate::wide::add)
                }
                BinOp::Sub => {
                    // +2 — same headroom reasoning as Add.
                    let sw = l.width.max(r.width) + 2;
                    let la = extend_to(&l, sw);
                    let ra = extend_to(&r, sw);
                    grown_op(e, &la, &ra, sw, crate::wide::sub)
                }
                BinOp::Mul => {
                    // +1: the unsigned product of an n-bit and m-bit
                    // magnitude always fits in n+m bits, but its OWN top
                    // bit (bit n+m-1) can legitimately be set (e.g. 3*3=9
                    // needs 4 bits, top bit set) without the true result
                    // being negative — one extra bit of headroom keeps
                    // that a genuine sign indicator, same as Add/Sub.
                    let sw = (l.width + r.width + 1).max(1);
                    let la = extend_to(&l, sw);
                    let ra = extend_to(&r, sw);
                    grown_op(e, &la, &ra, sw, crate::wide::mul)
                }
                BinOp::Shl => {
                    let amount = shift_amount(&r, e)?;
                    // +1: same headroom reasoning as Mul — the shifted
                    // magnitude fits in `l.width + amount` bits, but its
                    // own top bit can legitimately be set without the
                    // true result being negative.
                    let sw_u64 = (l.width as u64) + (amount as u64) + 1;
                    if sw_u64 > MAX_WIDTH as u64 {
                        return Err(overflow(e));
                    }
                    let sw = sw_u64 as u32;
                    let la = extend_to(&l, sw);
                    grown_op(e, &la, &la, sw, |a, _, w| crate::wide::shl(a, amount, w))
                }
                BinOp::Shr => {
                    // Arithmetic shift (sign-preserving) — matches this
                    // module's PRE-EXISTING i128 `>>` behavior exactly
                    // (Rust's signed `checked_shr`), just generalized past
                    // 128 bits. Never grows past `l.width`.
                    let amount = shift_amount(&r, e)?;
                    if amount >= l.width {
                        // Shifted entirely away: result is 0 (unsigned) or
                        // -1 (negative), matching arithmetic-shift saturation.
                        return Ok(if l.is_negative() {
                            ConstVal {
                                bits: Bits::Small(1),
                                width: 1,
                                signed: true,
                            }
                        } else {
                            ConstVal::zero()
                        });
                    }
                    let extended_width = l.width + amount;
                    let extended = extend_to(&l, extended_width);
                    let shifted = crate::wide::shr(&extended, amount);
                    let result_bits = bits::from_limbs(shifted, l.width);
                    let negative = bits::top_bit_set(&result_bits, l.width);
                    let (shrunk, width, signed) = bits::shrink(&result_bits, l.width, negative);
                    Ok(ConstVal {
                        bits: shrunk,
                        width,
                        signed,
                    })
                }
                BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                    let w = common_width(&l, &r);
                    let la = extend_to(&l, w);
                    let ra = extend_to(&r, w);
                    let result = match op {
                        BinOp::BitAnd => crate::wide::bitand(&la, &ra),
                        BinOp::BitOr => crate::wide::bitor(&la, &ra),
                        _ => crate::wide::bitxor(&la, &ra),
                    };
                    let result_bits = bits::from_limbs(result, w);
                    let negative = bits::top_bit_set(&result_bits, w);
                    let (shrunk, width, signed) = bits::shrink(&result_bits, w, negative);
                    Ok(ConstVal {
                        bits: shrunk,
                        width,
                        signed,
                    })
                }
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    let w = common_width(&l, &r);
                    let la = extend_to(&l, w);
                    let ra = extend_to(&r, w);
                    let ord = crate::wide::cmp_signed(&la, &ra, w);
                    use std::cmp::Ordering::*;
                    let result = match op {
                        BinOp::Eq => ord == Equal,
                        BinOp::Ne => ord != Equal,
                        BinOp::Lt => ord == Less,
                        BinOp::Le => ord != Greater,
                        BinOp::Gt => ord == Greater,
                        BinOp::Ge => ord != Less,
                        _ => unreachable!("guarded by the outer match arm"),
                    };
                    Ok(ConstVal::from_bool(result))
                }
                BinOp::LogicAnd => Ok(ConstVal::from_bool(!l.is_zero() && !r.is_zero())),
                BinOp::LogicOr => Ok(ConstVal::from_bool(!l.is_zero() || !r.is_zero())),
                BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap => not_const(
                    "a wrapping operator",
                    "`+%`/`-%`/`*%` wrap at a bit width, and compile-time integers \
                     have no width — use plain `+`/`-`/`*` in constants",
                ),
                // ponytail: `??` operates on valid-bundles (`T?`), which have no
                // compile-time-constant form; typing/lowering lands in later tasks.
                BinOp::Coalesce => not_const(
                    "`??`",
                    "`??` unwraps or muxes a valid-bundle, which has no \
                     compile-time-constant form",
                ),
            }
        }
        ExprKind::IfExpr { cond, then, els } => {
            if !eval(cond, env)?.is_zero() {
                eval(then, env)
            } else {
                eval(els, env)
            }
        }
        // The one compile-time builtin: `clog2(n)` folds to a constant, so it is
        // valid anywhere a constant is. Other builtins stay runtime-only (they
        // fall through to the catch-all `not_const` below).
        ExprKind::Call {
            func: crate::ast::Builtin::Clog2,
            args,
        } => {
            let n = eval(&args[0], env)?;
            if n.is_negative() || n.is_zero() {
                return Err(
                    Diag::new(e.span, "clog2 needs a positive argument".to_string())
                        .with_code("E0202")
                        .with_help(
                            "clog2(n) is the number of bits to address n items, so n must be >= 1",
                        ),
                );
            }
            let magnitude = n.limbs();
            let n_u128 = magnitude.first().copied().unwrap_or(0) as u128
                | ((magnitude.get(1).copied().unwrap_or(0) as u128) << 64);
            let result = clog2_bits(n_u128) as u128;
            let w = bits::natural_width(&Bits::Small(result));
            Ok(ConstVal {
                bits: Bits::Small(result),
                width: w,
                signed: false,
            })
        }
        ExprKind::Field { .. } => not_const(
            "an enum variant or instance port",
            "constants are plain `int`/`bool` values (spec/02 section 1.6)",
        ),
        _ => not_const(
            "this expression",
            "compile-time constants support literals, named consts, arithmetic, \
             comparisons, logic, and `if`/`else` (spec/02 section 1.6)",
        ),
    }
}

/// Read a shift amount as a plain `u32` shift count — errors (E0202) if
/// negative or implausibly large (an amount this evaluator would never
/// finish computing a result for is treated the same as "overflowed").
#[allow(clippy::result_large_err)]
fn shift_amount(v: &ConstVal, e: &Expr) -> Result<u32, Diag> {
    if v.is_negative() {
        return Err(overflow(e));
    }
    let limbs = v.limbs();
    let lo = limbs.first().copied().unwrap_or(0) as u128
        | ((limbs.get(1).copied().unwrap_or(0) as u128) << 64);
    u32::try_from(lo).map_err(|_| overflow(e))
}

fn overflow(e: &Expr) -> Diag {
    Diag::new(e.span, "constant is too large")
        .with_code("E0202")
        .with_help(format!(
            "compile-time arithmetic works on values up to {MAX_WIDTH} bits — \
             the same ceiling every signal width is checked against"
        ))
}

/// Bits needed to address `n` items — `⌈log₂(n)⌉`, floored at 1 (Min-Mozhi has
/// no zero-width signal, so `bits[clog2(N)]` is always a legal width). This is
/// the single source for both the `clog2` const-builtin and the enum-signal
/// encoding width (`emit_verilog` / `sim::elaborate` delegate here), so the two
/// can never drift. `n` must be `>= 1` (callers guard with E0202).
pub fn clog2_bits(n: u128) -> u32 {
    if n <= 1 {
        1
    } else {
        u128::BITS - (n - 1).leading_zeros()
    }
}

#[cfg(test)]
mod clog2_tests {
    use super::clog2_bits;

    #[test]
    fn clog2_bits_matches_spec_table() {
        // spec/02 section 1.8: clog2(n) = bits to address n items, floored at 1.
        assert_eq!(clog2_bits(1), 1);
        assert_eq!(clog2_bits(2), 1);
        assert_eq!(clog2_bits(3), 2);
        assert_eq!(clog2_bits(4), 2);
        assert_eq!(clog2_bits(5), 3);
        assert_eq!(clog2_bits(8), 3);
        assert_eq!(clog2_bits(9), 4);
        assert_eq!(clog2_bits(1024), 10);
    }
}

#[cfg(test)]
mod eval_tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    #[allow(clippy::result_large_err)]
    fn eval_src(src: &str) -> Result<ConstVal, Diag> {
        let toks = lexer::lex(src).unwrap();
        let file = parser::parse(toks).unwrap();
        let TopItem::Const(c) = &file.items[0] else {
            panic!("expected a const item");
        };
        eval(&c.value, &Env::new())
    }

    #[test]
    fn a_literal_past_the_old_i128_ceiling_folds_cleanly() {
        // 2^127 — used to hard-error under the old i128 cap; now well
        // under MAX_WIDTH (1,000,000 bits).
        let src = "const HUGE: int = 170141183460469231731687303715884105728";
        let v = eval_src(src).expect("2^127 must fold cleanly now");
        assert!(v.bits != Bits::Small(0));
        assert!(!v.signed);
    }

    #[test]
    fn addition_past_128_bits_folds_to_a_wide_constval() {
        // 2^127 + 2^127 = 2^128, which needs 129 bits — genuinely past the
        // Small/Wide boundary (unlike (2^127-1)*2 = 2^128-2, which fits
        // exactly in 128 bits and stays `Small`).
        let src = "const HUGE: int = 170141183460469231731687303715884105728 + 170141183460469231731687303715884105728";
        let v = eval_src(src).expect("addition of two large values must fold");
        assert!(matches!(v.bits, Bits::Wide(_)) || v.width > 128);
    }

    #[test]
    fn a_constant_exceeding_max_width_is_a_clean_e0202_error() {
        let src = "const HUGE: int = 1 << 1000001";
        let err = eval_src(src).expect_err("must exceed MAX_WIDTH cleanly, not panic");
        assert_eq!(err.code, Some("E0202"));
    }

    #[test]
    fn negation_round_trips_through_shrink() {
        let src = "const N: int = -5";
        let v = eval_src(src).expect("negation must fold");
        assert!(v.signed);
        // -5 in minimal two's complement is 4 bits: 0b1011.
        assert_eq!(v.width, 4);
    }

    #[test]
    fn small_arithmetic_still_works_exactly_as_before() {
        assert_eq!(
            eval_src("const N: int = 2 + 3").unwrap().bits,
            Bits::Small(5)
        );
        assert_eq!(
            eval_src("const N: int = 10 - 3").unwrap().bits,
            Bits::Small(7)
        );
        assert_eq!(
            eval_src("const N: int = 4 * 5").unwrap().bits,
            Bits::Small(20)
        );
        assert!(eval_src("const N: int = 3 == 3").unwrap().bits == Bits::Small(1));
        assert!(eval_src("const N: int = 3 != 3").unwrap().bits == Bits::Small(0));
        assert!(eval_src("const N: int = 2 < 3").unwrap().bits == Bits::Small(1));
    }
}
