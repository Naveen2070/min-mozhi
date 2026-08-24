//! `Val`'s (and, from BUG-13 layer 2, `ConstVal`'s) raw bit-pattern
//! representation — moved down from `mimz-sim` into this crate so the
//! lexer/AST/checker can share the same type instead of growing a
//! second, independent bignum representation (which would repeat the
//! "two copies of the same rule" mistake BUG-21 already forced a fix
//! for).

use crate::wide;

/// Low-`w`-bits mask (`w >= 128` ⇒ all ones).
pub fn mask(w: u32) -> u128 {
    if w >= 128 {
        u128::MAX
    } else {
        (1u128 << w) - 1
    }
}

/// A value's raw bit pattern: `Small` for the fast path (width <= 128,
/// stored as a plain `u128`), `Wide` for anything larger (little-endian
/// `u64` limbs, `wide::limb_count` of them). Whoever constructs a `Wide`
/// value must guarantee `width > 128` — a value that fits in 128 bits is
/// ALWAYS `Small`; no constructor in this module produces a `Wide` value
/// that fits in 128 bits.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Bits {
    Small(u128),
    Wide(Vec<u64>),
}

/// Render `bits` (masked to `width`, interpreted per `signed`) as a
/// decimal string.
pub fn bits_to_decimal_string(bits: &Bits, width: u32, signed: bool) -> String {
    match bits {
        Bits::Small(b) => {
            let m = b & mask(width);
            if signed && width >= 1 && (m >> (width - 1)) & 1 == 1 {
                ((m | !mask(width)) as i128).to_string()
            } else {
                m.to_string()
            }
        }
        Bits::Wide(limbs) => wide::to_decimal_string(limbs, width, signed),
    }
}

/// `bits`'s limbs at `width`, promoting a `Small` value to a
/// `wide::limb_count(width)`-length vector on the fly. `width` is
/// external context — `Bits` alone never carries its own width (that
/// lives one level up, on `Val`/`ConstVal`).
pub fn to_limbs(bits: &Bits, width: u32) -> Vec<u64> {
    match bits {
        Bits::Wide(v) => v.clone(),
        Bits::Small(b) => {
            let mut out = wide::zeros(width);
            out[0] = *b as u64;
            if out.len() > 1 {
                out[1] = (*b >> 64) as u64;
            }
            out
        }
    }
}

/// Build a `Bits` from a limb vector, resizing it to EXACTLY
/// `wide::limb_count(width)` elements (truncating or zero-padding as
/// needed — `limbs` arriving longer than that is expected, e.g. from
/// `retag`), masking to `width`, and auto-narrowing to `Small` when
/// `width <= 128` — the free-function counterpart of `Val::new_wide`'s
/// construction half, for callers (like const-eval) that don't have a
/// full `Val` to build. `resize`, not just `mask_to_width`, is the part
/// that actually enforces the Global-Constraints invariant — masking
/// alone zeroes high BITS but leaves an over-long vector over-long.
pub fn from_limbs(mut limbs: Vec<u64>, width: u32) -> Bits {
    limbs.resize(wide::limb_count(width), 0);
    wide::mask_to_width(&mut limbs, width);
    if width <= 128 {
        let lo = limbs.first().copied().unwrap_or(0) as u128;
        let hi = limbs.get(1).copied().unwrap_or(0) as u128;
        Bits::Small(lo | (hi << 64))
    } else {
        Bits::Wide(limbs)
    }
}

/// Minimal width (>=1) that holds `bits` as an UNSIGNED magnitude — the
/// position of the highest set bit, plus one (or 1 for zero). Only
/// correct for a value known to be non-negative; a negative
/// two's-complement value needs `shrink` instead.
pub fn natural_width(bits: &Bits) -> u32 {
    match bits {
        Bits::Small(b) => (u128::BITS - b.leading_zeros()).max(1),
        Bits::Wide(limbs) => limbs
            .iter()
            .copied()
            .enumerate()
            .rev()
            .find(|(_, l)| *l != 0)
            .map(|(i, l)| i as u32 * 64 + (64 - l.leading_zeros()))
            .unwrap_or(1),
    }
}

/// Whether bit `width - 1` (the top bit at this width) is set.
pub fn top_bit_set(bits: &Bits, width: u32) -> bool {
    if width == 0 {
        return false;
    }
    match bits {
        Bits::Small(b) => (b >> (width - 1)) & 1 == 1,
        Bits::Wide(limbs) => wide::bit_at(limbs, width - 1),
    }
}

impl From<u128> for Bits {
    fn from(v: u128) -> Bits {
        Bits::Small(v)
    }
}

/// Re-pack `bits` so its `Wide` variant (if any) holds EXACTLY
/// `wide::limb_count(width)` elements — the Global-Constraints invariant
/// every `Bits::Wide` must satisfy. Use this whenever pairing an
/// already-built `Bits` (e.g. straight from the lexer, which may have
/// left padding limbs) with a NEW, possibly narrower `width` — a raw
/// `.clone()` would silently violate the invariant if the source vector
/// is longer than the new width needs.
pub fn retag(bits: &Bits, width: u32) -> Bits {
    from_limbs(to_limbs(bits, width), width)
}

/// Number of consecutive `1` bits starting at the top (bit `width - 1`)
/// and moving downward — the two's-complement analogue of
/// `u128::leading_zeros`, used to find the minimal width that still
/// sign-extends back to the same negative value.
pub fn leading_ones(bits: &Bits, width: u32) -> u32 {
    let mut n = 0;
    for i in (0..width).rev() {
        let set = match bits {
            Bits::Small(b) => (b >> i) & 1 == 1,
            Bits::Wide(limbs) => wide::bit_at(limbs, i),
        };
        if !set {
            break;
        }
        n += 1;
    }
    n
}

/// Trim a two's-complement value at `width` bits to its minimal width,
/// preserving the numeric value exactly. `signed` says how to interpret
/// `bits` at `width` on the way in (same convention as `Val`: a
/// non-negative value is unsigned at its own tight width; a negative
/// value is two's-complement at its own tight width). Returns
/// `(trimmed_bits, minimal_width, is_negative)` — `is_negative` mirrors
/// the input's sign for a nonzero value; `0` always comes back
/// `(Small(0), 1, false)` regardless of the input `signed` flag (mirrors
/// `Val::from_int`'s existing two-branch rule, generalized past 128 bits).
pub fn shrink(bits: &Bits, width: u32, signed: bool) -> (Bits, u32, bool) {
    if signed && top_bit_set(bits, width) {
        // Negative: trim leading 1s, but always keep at least the sign
        // bit plus one bit below it (a lone `1` bit alone, at width 1, IS
        // a valid representation of -1 — the minimum is 1, not 2).
        let ones = leading_ones(bits, width);
        let w = (width - ones + 1).max(1).min(width);
        let limbs = to_limbs(bits, width);
        (from_limbs(limbs, w), w, true)
    } else {
        if matches!(bits, Bits::Small(0)) {
            return (Bits::Small(0), 1, false);
        }
        let w = natural_width(bits);
        let limbs = to_limbs(bits, width.max(w));
        (from_limbs(limbs, w), w, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_of_128_or_more_is_all_ones() {
        assert_eq!(mask(128), u128::MAX);
        assert_eq!(mask(200), u128::MAX);
    }

    #[test]
    fn bits_to_decimal_string_renders_a_small_negative_value() {
        // -1 at 8 bits, signed: 0xFF.
        assert_eq!(bits_to_decimal_string(&Bits::Small(0xFF), 8, true), "-1");
    }

    #[test]
    fn to_limbs_promotes_a_small_value() {
        let b = Bits::Small(0xFF);
        let limbs = to_limbs(&b, 200);
        assert_eq!(limbs.len(), wide::limb_count(200));
        assert_eq!(limbs[0], 0xFF);
    }

    #[test]
    fn from_limbs_auto_narrows_at_128_bits_or_less() {
        let limbs = wide::from_u128(42, 96);
        let b = from_limbs(limbs, 96);
        assert_eq!(b, Bits::Small(42));
    }

    #[test]
    fn from_limbs_stays_wide_past_128_bits() {
        let limbs = wide::from_u128(42, 200);
        let b = from_limbs(limbs, 200);
        assert!(matches!(b, Bits::Wide(_)));
    }

    #[test]
    fn natural_width_of_zero_is_one() {
        assert_eq!(natural_width(&Bits::Small(0)), 1);
    }

    #[test]
    fn natural_width_of_a_small_value_is_tight() {
        // 5 = 0b101, needs 3 bits.
        assert_eq!(natural_width(&Bits::Small(5)), 3);
    }

    #[test]
    fn natural_width_of_a_wide_value_scans_limbs() {
        // Bit 130 set: needs 131 bits.
        let mut limbs = wide::zeros(200);
        limbs[2] |= 1u64 << (130 - 128);
        assert_eq!(natural_width(&Bits::Wide(limbs)), 131);
    }

    #[test]
    fn top_bit_set_reads_the_correct_position() {
        assert!(top_bit_set(&Bits::Small(0b100), 3));
        assert!(!top_bit_set(&Bits::Small(0b100), 4));
    }

    #[test]
    fn from_u128_impl_matches_bits_small() {
        let b: Bits = 42u128.into();
        assert_eq!(b, Bits::Small(42));
    }

    #[test]
    fn retag_trims_a_padded_wide_vector_to_the_new_widths_limb_count() {
        // A `Wide` value built at 512 bits (8 limbs) but only using its
        // low bits — retagging down to width 200 (4 limbs) must shrink
        // the vector to exactly `limb_count(200)`, not leave it at 8.
        let padded = wide::from_u128(0xFF, 512);
        let retagged = retag(&Bits::Wide(padded), 200);
        match retagged {
            Bits::Wide(limbs) => assert_eq!(limbs.len(), wide::limb_count(200)),
            Bits::Small(_) => panic!("200 bits must stay Wide, not auto-narrow"),
        }
    }

    #[test]
    fn leading_ones_counts_from_the_top_bit_down() {
        // 0b1110 at width 4: top 3 bits are 1, then a 0.
        assert_eq!(leading_ones(&Bits::Small(0b1110), 4), 3);
    }

    #[test]
    fn leading_ones_of_all_ones_is_the_full_width() {
        assert_eq!(leading_ones(&Bits::Small(u128::MAX), 8), 8);
    }

    #[test]
    fn shrink_of_a_nonnegative_value_finds_the_tight_unsigned_width() {
        let (b, w, signed) = shrink(&Bits::Small(5), 8, false);
        assert_eq!(w, 3);
        assert!(!signed);
        assert_eq!(b, Bits::Small(5));
    }

    #[test]
    fn shrink_of_negative_one_round_trips() {
        let (b, w, signed) = shrink(&Bits::Small(0xFF), 8, true);
        assert!(signed);
        let extended = wide::extend(&to_limbs(&b, w), w, 8, true);
        assert_eq!(from_limbs(extended, 8), Bits::Small(0xFF));
    }

    #[test]
    fn shrink_of_negative_four_reproduces_the_same_value_at_a_smaller_width() {
        // -4 at 8 bits is 0b11111100. Minimal two's-complement width for
        // -4 is 3 bits (range [-4, 3]).
        let (b, w, signed) = shrink(&Bits::Small(0b11111100), 8, true);
        assert!(signed);
        assert_eq!(w, 3);
        let extended = wide::extend(&to_limbs(&b, w), w, 8, true);
        assert_eq!(from_limbs(extended, 8), Bits::Small(0b11111100));
    }

    #[test]
    fn shrink_of_zero_is_never_reported_negative() {
        let (b, w, signed) = shrink(&Bits::Small(0), 8, true);
        assert!(!signed);
        assert_eq!(w, 1);
        assert_eq!(b, Bits::Small(0));
    }
}
