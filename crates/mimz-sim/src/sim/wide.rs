//! Arbitrary-width bit-vector arithmetic for values wider than 128 bits —
//! `Val`'s "slow path"
//! (`docs/superpowers/specs/2026-07-22-sim-wide-values-design.local.md`).
//! Little-endian `u64` limbs; a `Vec<u64>` for width `w` always holds
//! exactly `limb_count(w)` elements, index 0 least significant. Not a
//! general-purpose bignum library — only the operations `value.rs`'s
//! evaluator actually needs (no division; the language has none).

/// Number of 64-bit limbs needed to hold `width` bits.
pub(super) fn limb_count(width: u32) -> usize {
    ((width as u64 + 63) / 64) as usize
}

/// A `width`-bit zero vector, ready to mask/fill.
pub(super) fn zeros(width: u32) -> Vec<u64> {
    vec![0u64; limb_count(width)]
}

/// Mask `limbs` down to exactly `width` meaningful bits (extra high bits,
/// if any, zeroed) — the `Vec<u64>` analogue of `value::mask`.
pub(super) fn mask_to_width(limbs: &mut [u64], width: u32) {
    let full = (width / 64) as usize;
    let rem = width % 64;
    if rem != 0 {
        if let Some(l) = limbs.get_mut(full) {
            *l &= (1u64 << rem) - 1;
        }
    }
    let clear_from = if rem == 0 { full } else { full + 1 };
    for l in limbs.iter_mut().skip(clear_from) {
        *l = 0;
    }
}

/// Build a wide limb vector from a `u128`, zero-extended to `width`
/// limbs.
pub(super) fn from_u128(v: u128, width: u32) -> Vec<u64> {
    let mut out = zeros(width);
    out[0] = v as u64;
    if out.len() > 1 {
        out[1] = (v >> 64) as u64;
    }
    mask_to_width(&mut out, width);
    out
}

/// The value of bit `i` (0 = LSB). Out-of-range `i` reads as 0.
pub(super) fn bit_at(limbs: &[u64], i: u32) -> bool {
    let (limb, off) = ((i / 64) as usize, i % 64);
    limbs.get(limb).is_some_and(|l| (l >> off) & 1 == 1)
}

/// Set every bit in `[from, to)` to 1 — the sign-fill step `extend` uses.
fn set_bits_from(limbs: &mut [u64], from: u32, to: u32) {
    for bit in from..to {
        let (limb, off) = ((bit / 64) as usize, bit % 64);
        if let Some(l) = limbs.get_mut(limb) {
            *l |= 1u64 << off;
        }
    }
}

/// Resize `limbs` (currently `from_width` bits) to `to_width` bits,
/// sign-extending the new high bits when `signed` and the value is
/// negative — zero-extending otherwise. The `Vec<u64>` analogue of
/// `value::extend_bits`. A no-op-sized request (`to_width <= from_width`)
/// still masks down correctly. Assumes input is canonical (masked to `from_width`).
pub(super) fn extend(limbs: &[u64], from_width: u32, to_width: u32, signed: bool) -> Vec<u64> {
    let mut out = zeros(to_width);
    let n = limbs.len().min(out.len());
    out[..n].copy_from_slice(&limbs[..n]);
    if signed && to_width > from_width && from_width >= 1 && bit_at(limbs, from_width - 1) {
        set_bits_from(&mut out, from_width, to_width);
    }
    mask_to_width(&mut out, to_width);
    out
}

/// Unsigned magnitude compare, most-significant limb first.
/// Assumes inputs are canonical (masked to their declared width).
pub(super) fn cmp_unsigned(a: &[u64], b: &[u64]) -> std::cmp::Ordering {
    let len = a.len().max(b.len());
    for i in (0..len).rev() {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        match av.cmp(&bv) {
            std::cmp::Ordering::Equal => continue,
            ord => return ord,
        }
    }
    std::cmp::Ordering::Equal
}

/// Signed compare at `width` bits: flip the sign bit on both sides (the
/// standard two's-complement-to-unsigned-order trick), then compare
/// unsigned. Assumes inputs are canonical (masked to `width`).
pub(super) fn cmp_signed(a: &[u64], b: &[u64], width: u32) -> std::cmp::Ordering {
    fn flip(limbs: &[u64], width: u32) -> Vec<u64> {
        let mut out = limbs.to_vec();
        let bit = width - 1;
        let (limb, off) = ((bit / 64) as usize, bit % 64);
        if let Some(l) = out.get_mut(limb) {
            *l ^= 1u64 << off;
        }
        out
    }
    cmp_unsigned(&flip(a, width), &flip(b, width))
}

/// Whether every limb is zero.
/// Assumes input is canonical (masked to its declared width).
pub(super) fn is_zero(a: &[u64]) -> bool {
    a.iter().all(|&l| l == 0)
}

/// Total population count across all limbs.
/// Assumes input is canonical (masked to its declared width).
pub(super) fn count_ones(a: &[u64]) -> u32 {
    a.iter().map(|l| l.count_ones()).sum()
}

/// `a + b`, masked to `result_width`. Inputs may be shorter than the
/// result (treated as zero-extended); a carry that overflows
/// `result_width` is simply masked away (two's-complement wraparound,
/// matching every existing wrapping-arithmetic arm in `value.rs`).
/// Assumes inputs are canonical (masked to their declared width) — the
/// output is re-masked here, but garbage bits within `result_width` would
/// still corrupt the sum.
pub(super) fn add(a: &[u64], b: &[u64], result_width: u32) -> Vec<u64> {
    let mut out = zeros(result_width);
    let mut carry: u128 = 0;
    for i in 0..out.len() {
        let sum =
            carry + a.get(i).copied().unwrap_or(0) as u128 + b.get(i).copied().unwrap_or(0) as u128;
        out[i] = sum as u64;
        carry = sum >> 64;
    }
    mask_to_width(&mut out, result_width);
    out
}

/// `a - b`, masked to `result_width`, via direct borrow propagation
/// (two's complement makes this exact modulo `result_width`, regardless
/// of the true sign of the mathematical difference).
/// Assumes inputs are canonical (masked to their declared width) — same
/// caveat as `add`.
pub(super) fn sub(a: &[u64], b: &[u64], result_width: u32) -> Vec<u64> {
    let mut out = zeros(result_width);
    let mut borrow: i128 = 0;
    for i in 0..out.len() {
        let av = a.get(i).copied().unwrap_or(0) as i128;
        let bv = b.get(i).copied().unwrap_or(0) as i128;
        let diff = av - bv - borrow;
        if diff < 0 {
            out[i] = (diff + (1i128 << 64)) as u64;
            borrow = 1;
        } else {
            out[i] = diff as u64;
            borrow = 0;
        }
    }
    mask_to_width(&mut out, result_width);
    out
}

/// `-a` (two's complement negate) at `width` bits.
/// Assumes input is canonical (masked to `width`) — same caveat as `sub`.
pub(super) fn neg(a: &[u64], width: u32) -> Vec<u64> {
    sub(&zeros(width), a, width)
}

/// Bitwise NOT.
/// Assumes input is canonical (masked to its declared width); since this
/// has no width parameter to re-mask against, the output is canonical only
/// if the input was.
pub(super) fn not(a: &[u64]) -> Vec<u64> {
    a.iter().map(|l| !l).collect()
}

/// Schoolbook multiply — `O(len(a) * len(b))`, acceptable at the sizes
/// this simulator deals with (`MAX_WIDTH` = 1,000,000 bits = ~15,625
/// limbs at the absolute extreme; real designs are far smaller).
/// Assumes inputs are canonical (masked to their declared width) — same
/// caveat as `add`.
pub(super) fn mul(a: &[u64], b: &[u64], result_width: u32) -> Vec<u64> {
    let mut acc = vec![0u128; a.len() + b.len() + 1];
    for (i, &ai) in a.iter().enumerate() {
        let mut carry: u128 = 0;
        for (j, &bj) in b.iter().enumerate() {
            let prod = ai as u128 * bj as u128 + acc[i + j] + carry;
            acc[i + j] = prod & u64::MAX as u128;
            carry = prod >> 64;
        }
        let mut k = i + b.len();
        while carry > 0 {
            let sum = acc[k] + carry;
            acc[k] = sum & u64::MAX as u128;
            carry = sum >> 64;
            k += 1;
        }
    }
    let mut out = zeros(result_width);
    for i in 0..out.len().min(acc.len()) {
        out[i] = acc[i] as u64;
    }
    mask_to_width(&mut out, result_width);
    out
}

/// Render `limbs` (masked to `width`, interpreted per `signed`) as a
/// decimal string — the one place this module needs division-like
/// reduction (display only; the language has no `/` operator).
/// Repeatedly divides by `10^19` (the largest power of ten that fits
/// comfortably below `u64::MAX`, keeping the remainder cheap to compute
/// per limb) for speed, formatting the top chunk unpadded and the rest
/// zero-padded to 19 digits.
/// Assumes `limbs` is canonical (masked to `width`) — the non-negative
/// path does not re-mask before rendering.
pub(super) fn to_decimal_string(limbs: &[u64], width: u32, signed: bool) -> String {
    let negative = signed && width >= 1 && bit_at(limbs, width - 1);
    let mut cur = if negative {
        neg(limbs, width)
    } else {
        limbs.to_vec()
    };
    const DIV: u128 = 10_000_000_000_000_000_000; // 10^19
    let mut chunks: Vec<u64> = Vec::new();
    loop {
        let mut rem: u128 = 0;
        let mut any_nonzero = false;
        for l in cur.iter_mut().rev() {
            let acc = (rem << 64) | (*l as u128);
            *l = (acc / DIV) as u64;
            rem = acc % DIV;
            if *l != 0 {
                any_nonzero = true;
            }
        }
        chunks.push(rem as u64);
        if !any_nonzero {
            break;
        }
    }
    let mut s = String::new();
    if negative {
        s.push('-');
    }
    for (i, chunk) in chunks.iter().rev().enumerate() {
        if i == 0 {
            s.push_str(&chunk.to_string());
        } else {
            s.push_str(&format!("{chunk:019}"));
        }
    }
    s
}

/// `a << amount`, masked to `result_width`. `amount` is the caller's
/// already-decided shift count (the caller — Task 6's `Shl` arm —
/// decides what an amount `>= result_width` means; this function just
/// shifts and masks). Assumes `a` is canonical (masked to its declared
/// width) — the output is re-masked to `result_width` here, but garbage
/// bits within `result_width` would still corrupt the shifted value.
pub(super) fn shl(a: &[u64], amount: u32, result_width: u32) -> Vec<u64> {
    let mut out = zeros(result_width);
    let limb_shift = (amount / 64) as usize;
    let bit_shift = amount % 64;
    for i in 0..a.len() {
        let dst = i + limb_shift;
        if dst >= out.len() {
            break;
        }
        out[dst] |= a[i] << bit_shift;
        if bit_shift > 0 && dst + 1 < out.len() {
            out[dst + 1] |= a[i] >> (64 - bit_shift);
        }
    }
    mask_to_width(&mut out, result_width);
    out
}

/// Plain logical `a >> amount` — a raw shift of the bit pattern, never
/// sign-filling (mirrors `value.rs`'s own `Shr` arm exactly: any needed
/// sign-extension happens on `a` itself, via `extend`, BEFORE this call).
/// Output is the same length as `a`. Assumes `a` is canonical (masked to
/// its declared width); since this has no width parameter to re-mask
/// against, the output is canonical only if the input was.
pub(super) fn shr(a: &[u64], amount: u32) -> Vec<u64> {
    let mut out = vec![0u64; a.len()];
    let limb_shift = (amount / 64) as usize;
    let bit_shift = amount % 64;
    for i in 0..a.len() {
        let src = i + limb_shift;
        if src >= a.len() {
            continue;
        }
        out[i] = a[src] >> bit_shift;
        if bit_shift > 0 && src + 1 < a.len() {
            out[i] |= a[src + 1] << (64 - bit_shift);
        }
    }
    out
}

/// Elementwise bitwise AND/OR/XOR over two same-length limb vectors
/// (callers widen both operands to matching length first, mirroring
/// every existing narrow-path bitwise arm in `value.rs`). Assumes both
/// inputs are canonical (masked to their declared width); since these
/// have no width parameter to re-mask against, the output is canonical
/// only if the inputs were.
pub(super) fn bitand(a: &[u64], b: &[u64]) -> Vec<u64> {
    a.iter().zip(b).map(|(x, y)| x & y).collect()
}
pub(super) fn bitor(a: &[u64], b: &[u64]) -> Vec<u64> {
    a.iter().zip(b).map(|(x, y)| x | y).collect()
}
pub(super) fn bitxor(a: &[u64], b: &[u64]) -> Vec<u64> {
    a.iter().zip(b).map(|(x, y)| x ^ y).collect()
}

/// Render `limbs` (masked to `width`) as a binary string with no `0b`
/// prefix and no leading zeros (except the single digit for zero) — the
/// VCD writer's vector-value format (Task 9).
pub(super) fn to_binary_string(limbs: &[u64], width: u32) -> String {
    let s: String = (0..width)
        .rev()
        .map(|bit| if bit_at(limbs, bit) { '1' } else { '0' })
        .collect();
    let trimmed = s.trim_start_matches('0');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn from_u128_round_trips_through_bit_at() {
        let limbs = from_u128(0b1011, 200);
        assert!(bit_at(&limbs, 0));
        assert!(bit_at(&limbs, 1));
        assert!(!bit_at(&limbs, 2));
        assert!(bit_at(&limbs, 3));
        assert!(!bit_at(&limbs, 199));
    }

    #[test]
    fn extend_zero_fills_an_unsigned_value() {
        let a = from_u128(0xFF, 130);
        let wide = extend(&a, 130, 200, false);
        assert_eq!(limb_count(200), wide.len());
        assert!(!bit_at(&wide, 199));
    }

    #[test]
    fn extend_sign_fills_a_negative_signed_value() {
        // All-ones 130-bit value (== -1 signed) extended to 200 bits must
        // stay all-ones in the new high bits.
        let a = vec![u64::MAX; limb_count(130)];
        let wide = extend(&a, 130, 200, true);
        assert!(bit_at(&wide, 199));
        assert!(bit_at(&wide, 135));
    }

    #[test]
    fn cmp_unsigned_orders_by_magnitude() {
        let a = from_u128(5, 200);
        let b = from_u128(9, 200);
        assert_eq!(cmp_unsigned(&a, &b), Ordering::Less);
    }

    #[test]
    fn cmp_signed_a_negative_value_is_less_than_a_positive_one() {
        // All-ones (== -1 signed) vs. a small positive value. Masked to
        // width: limb_count(200) allocates 256 raw bits, and cmp_signed
        // (like cmp_unsigned) trusts its inputs to already be canonical
        // (bits beyond width are 0) — same contract from_u128 upholds.
        let mut neg_one = vec![u64::MAX; limb_count(200)];
        mask_to_width(&mut neg_one, 200);
        let one = from_u128(1, 200);
        assert_eq!(cmp_signed(&neg_one, &one, 200), Ordering::Less);
    }

    #[test]
    fn is_zero_and_count_ones() {
        let zero = zeros(200);
        assert!(is_zero(&zero));
        let a = from_u128(0b1011, 200);
        assert!(!is_zero(&a));
        assert_eq!(count_ones(&a), 3);
    }

    #[test]
    fn add_carries_across_a_limb_boundary() {
        // u64::MAX + 1 must carry into the second limb.
        let a = from_u128(u64::MAX as u128, 200);
        let b = from_u128(1, 200);
        let sum = add(&a, &b, 200);
        assert!(!bit_at(&sum, 63)); // low limb wrapped to 0
        assert!(bit_at(&sum, 64)); // carried into bit 64
    }

    #[test]
    fn sub_borrows_across_a_limb_boundary() {
        let a = from_u128(1u128 << 64, 200); // bit 64 set, rest 0
        let b = from_u128(1, 200);
        let diff = sub(&a, &b, 200);
        // (1 << 64) - 1 == 0xFFFF_FFFF_FFFF_FFFF in the low limb, 0 above.
        assert!(!bit_at(&diff, 64));
        for i in 0..64 {
            assert!(bit_at(&diff, i), "bit {i} should be set");
        }
    }

    #[test]
    fn neg_of_one_is_all_ones() {
        let one = from_u128(1, 200);
        let neg_one = neg(&one, 200);
        for i in 0..200 {
            assert!(bit_at(&neg_one, i), "bit {i} should be set in -1");
        }
    }

    #[test]
    fn mul_of_two_wide_values_carries_correctly() {
        // (2^64) * (2^64) == 2^128 — bit 128 set, nothing below it.
        let a = from_u128(1u128 << 64, 200);
        let b = from_u128(1u128 << 64, 200);
        let product = mul(&a, &b, 200);
        assert!(bit_at(&product, 128));
        for i in 0..128 {
            assert!(!bit_at(&product, i), "bit {i} should be clear");
        }
    }

    #[test]
    fn to_decimal_string_matches_a_known_large_unsigned_value() {
        // 2^130 = 1361129467683753853853498429727072845824
        let limbs = from_u128(0, 200);
        let mut limbs = limbs;
        // Set bit 130 directly.
        limbs[2] |= 1u64 << (130 - 128);
        assert_eq!(
            to_decimal_string(&limbs, 200, false),
            "1361129467683753853853498429727072845824"
        );
    }

    #[test]
    fn to_decimal_string_renders_a_negative_signed_value() {
        let neg_one = neg(&from_u128(1, 200), 200);
        assert_eq!(to_decimal_string(&neg_one, 200, true), "-1");
    }

    #[test]
    fn to_decimal_string_renders_zero() {
        assert_eq!(to_decimal_string(&zeros(200), 200, false), "0");
    }

    #[test]
    fn shl_crosses_a_limb_boundary() {
        let a = from_u128(1, 200);
        let shifted = shl(&a, 64, 200);
        assert!(bit_at(&shifted, 64));
        assert!(!bit_at(&shifted, 0));
    }

    #[test]
    fn shl_masks_bits_that_overflow_result_width() {
        let a = from_u128(1, 200);
        let shifted = shl(&a, 199, 200);
        assert!(bit_at(&shifted, 199));
        let overflowed = shl(&a, 200, 200);
        assert!(is_zero(&overflowed), "a shift past the width must vanish");
    }

    #[test]
    fn shr_crosses_a_limb_boundary() {
        let mut a = zeros(200);
        a[1] = 1; // bit 64 set
        let shifted = shr(&a, 64);
        assert!(bit_at(&shifted, 0));
        assert!(!bit_at(&shifted, 64));
    }

    #[test]
    fn bitwise_ops_are_elementwise() {
        let a = from_u128(0b1100, 200);
        let b = from_u128(0b1010, 200);
        assert!(bit_at(&bitand(&a, &b), 3));
        assert!(!bit_at(&bitand(&a, &b), 1));
        assert!(bit_at(&bitor(&a, &b), 1));
        assert!(bit_at(&bitxor(&a, &b), 2));
        assert!(!bit_at(&bitxor(&a, &b), 3));
    }

    #[test]
    fn to_binary_string_has_no_leading_zeros_except_for_the_value_zero() {
        let a = from_u128(0b101, 200);
        assert_eq!(to_binary_string(&a, 200), "101");
        assert_eq!(to_binary_string(&zeros(200), 200), "0");
    }
}
