//! IEEE 754 binary16: membership of a binary64 value, the binary16 nearest
//! a decimal (round to nearest, ties to even), and a binary16 value's
//! closest-shortest decimal text.
//!
//! Binary16 has an 11-bit significand and exponents from -14 to 15, with
//! subnormals down to 2^-24; its largest finite value is 65504. Every
//! binary16 value, and every midpoint of two adjacent ones, is a binary64
//! value, so the functions here hold binary16 values widened to `f64` and
//! decide every rounding on exact quantities.

use std::cmp::Ordering;

use super::DecimalValue;

/// The least magnitude that rounds to an infinity at binary16: the largest
/// finite value, 65504, plus half its unit in the last place, 16. A tie,
/// which goes to the even neighbour 2^16 and so overflows.
pub const BINARY16_OVERFLOW: f64 = 65_520.0;

/// The binary16 bits of the binary64 value `x` when binary16 holds `x`,
/// else `None`.
///
/// A finite `x` is held when it is a zero of either sign or a value that
/// eleven significant bits and binary16's exponent range (normal and
/// subnormal) hold. An infinity is held, with its sign. A NaN is decided on
/// its bits: it is held when its low 42 fraction bits are zero, quiet or
/// signalling, and its binary16 is the NaN with `x`'s sign, an all-ones
/// exponent and `x`'s high 10 fraction bits. So the binary16 quiet NaN
/// `0x7e00` widens on its bits to `0x7ff8000000000000`.
///
/// ```
/// use panproto_expr::exact::binary16_bits;
///
/// assert_eq!(binary16_bits(65_504.0), Some(0x7bff));
/// assert_eq!(binary16_bits(f64::from_bits(0x7ff8_0000_0000_0000)), Some(0x7e00));
/// assert_eq!(binary16_bits(f64::from_bits(0x7ff4_0000_0000_0000)), Some(0x7d00));
/// assert_eq!(binary16_bits(f64::from_bits(0x7ff8_0000_0000_0001)), None);
/// assert_eq!(binary16_bits(0.1), None);
/// assert_eq!(binary16_bits(0.099_975_585_937_5), Some(0x2e66));
/// ```
#[must_use]
pub fn binary16_bits(x: f64) -> Option<u16> {
    let bits = x.to_bits();
    let sign = u16::from(bits >> 63 == 1) << 15;
    let biased = (bits >> 52) & 0x7ff;
    let fraction = bits & ((1 << 52) - 1);
    let low = fraction & ((1 << 42) - 1);
    let high = u16::try_from(fraction >> 42).ok()?;
    match biased {
        0x7ff => (low == 0).then_some(sign | 0x7c00 | high),
        // A zero; a non-zero binary64 subnormal is far below 2^-24.
        0 => (fraction == 0).then_some(sign),
        _ => {
            let exponent = i64::try_from(biased).ok()? - 1023;
            match exponent {
                -14..=15 => {
                    let field = u16::try_from(exponent + 15).ok()?;
                    (low == 0).then_some(sign | (field << 10) | high)
                }
                -24..=-15 => {
                    // A subnormal: the significand with its implicit bit,
                    // shifted so that its unit is 2^-24, which needs the
                    // bits shifted out to be zero.
                    let shift = u32::try_from(28 - exponent).ok()?;
                    let significand = fraction | (1 << 52);
                    let out = significand & ((1 << shift) - 1);
                    let field = u16::try_from(significand >> shift).ok()?;
                    (out == 0).then_some(sign | field)
                }
                _ => None,
            }
        }
    }
}

/// The binary16 value of the bits `h`, widened exactly to `f64`, with a
/// NaN widened on its bits (sign, then the 10 fraction bits as the high
/// fraction bits), so that [`binary16_bits`] gives `h` back.
///
/// ```
/// use panproto_expr::exact::{binary16_bits, binary16_widen};
///
/// for h in 0..=u16::MAX {
///     assert_eq!(binary16_bits(binary16_widen(h)), Some(h));
/// }
/// ```
#[must_use]
pub fn binary16_widen(h: u16) -> f64 {
    let sign = u64::from(h >> 15) << 63;
    let field = u64::from((h >> 10) & 0x1f);
    let fraction = u64::from(h & 0x3ff);
    match field {
        0x1f => f64::from_bits(sign | (0x7ff << 52) | (fraction << 42)),
        0 => {
            // Zero or a subnormal: fraction × 2^-24, exact in binary64.
            let magnitude =
                f64::from(u32::try_from(fraction).unwrap_or(0)) * f64::from_bits(0x3e70_0000_0000_0000);
            f64::from_bits(sign | magnitude.to_bits())
        }
        _ => f64::from_bits(sign | ((field + 1008) << 52) | (fraction << 42)),
    }
}

/// The two binary16 magnitudes around the finite non-negative binary64 `a`
/// (`lo <= a <= hi`; `lo == hi` when binary16 holds `a`), widened, with
/// 2^16 standing for an overflow, and how `a` lies between them: `Less`
/// when nearer `lo`, `Greater` when nearer `hi`, `Equal` at their exact
/// midpoint. The third component is whether `lo` has an even significand.
fn bracket(a: f64) -> (f64, f64, Ordering, bool) {
    // The unit in the last place of a binary16 value of a's magnitude:
    // 2^(e - 10) in the normal binade [2^e, 2^(e+1)), 2^-24 below 2^-14.
    let exponent = i64::try_from((a.to_bits() >> 52) & 0x7ff).unwrap_or(0) - 1023;
    let unit = f64::from_bits(u64::try_from(exponent.max(-14) - 10 + 1023).unwrap_or(0) << 52);
    // Scaling by a power of two and rounding to an integer are exact.
    let scaled = a / unit;
    let (floor, ceil) = (scaled.floor(), scaled.ceil());
    let side = (scaled - floor).total_cmp(&0.5);
    let even = floor.rem_euclid(2.0).total_cmp(&0.0).is_eq();
    let exact = floor.total_cmp(&ceil).is_eq();
    (
        floor * unit,
        ceil * unit,
        if exact { Ordering::Less } else { side },
        even,
    )
}

/// The exact decimal value of the finite binary64 `x`.
fn exact_decimal(x: f64) -> DecimalValue {
    // A binary64 value's decimal expansion has at most 767 significant
    // digits, and formatting at that precision writes it exactly.
    DecimalValue::parse(&format!("{:.767e}", x.abs())).unwrap_or_else(|_| DecimalValue::from_i64(0))
}

/// The binary16 value nearest the finite decimal `d`, widened to `f64`.
///
/// This is `L_16`: IEEE 754 round to nearest, ties to even, with the sign
/// of `d` (a zero keeps its sign), or the infinity of `d`'s sign when `d`
/// rounds beyond the largest binary16, that is when `|d| >= 65520`.
///
/// The rounding is exact: `d` is first rounded to binary64 by Rust's
/// correctly rounded parser, giving `x`. No binary16 midpoint lies strictly
/// between `d` and `x`, since every midpoint is a binary64 value and `x` is
/// the binary64 nearest `d`; so `d` rounds as `x` does, unless `x` is a
/// midpoint itself, where `d` is compared with it exactly.
///
/// ```
/// use panproto_expr::exact::{DecimalValue, binary16_nearest};
///
/// let l16 = |t: &str| binary16_nearest(&DecimalValue::parse(t).unwrap());
/// assert_eq!(l16("0.1"), 0.099_975_585_937_5);
/// assert_eq!(l16("65519"), 65_504.0);
/// assert_eq!(l16("65520"), f64::INFINITY);
/// assert_eq!(l16("65519.9999999999999999999999"), 65_504.0);
/// assert_eq!(l16("2049"), 2048.0);
/// assert_eq!(l16("6e-8"), 5.960_464_477_539_063e-8);
/// ```
#[must_use]
pub fn binary16_nearest(d: &DecimalValue) -> f64 {
    let signed = |m: f64| if d.negative { -m } else { m };
    if d.is_zero() {
        return signed(0.0);
    }
    let a = d.to_f64().abs();
    if a > BINARY16_OVERFLOW {
        return signed(f64::INFINITY);
    }
    let (lo, hi, side, even) = bracket(a);
    let up = match side {
        Ordering::Less => false,
        Ordering::Greater => true,
        Ordering::Equal => {
            let magnitude = DecimalValue {
                negative: false,
                ..d.clone()
            };
            match magnitude.cmp_numeric(&exact_decimal(a)) {
                Ordering::Less => false,
                Ordering::Greater => true,
                Ordering::Equal => !even,
            }
        }
    };
    let m = if up { hi } else { lo };
    signed(if m > BINARY16_OVERFLOW { f64::INFINITY } else { m })
}

impl DecimalValue {
    /// The closest-shortest decimal of the finite binary16 value `x`
    /// (given widened to `f64`), or `None` when `x` is not finite or not a
    /// binary16 value: the rule of
    /// [`from_f64_shortest`](Self::from_f64_shortest), ties to even, with
    /// reading back at binary16 by [`binary16_nearest`]. So the binary16
    /// nearest 0.1, `0.0999755859375`, gives `0.1`.
    ///
    /// ```
    /// use panproto_expr::exact::DecimalValue;
    ///
    /// let text = |x: f64| DecimalValue::from_f16_shortest(x).map(|d| d.to_string());
    /// assert_eq!(text(0.099_975_585_937_5).as_deref(), Some("0.1"));
    /// assert_eq!(text(65_504.0).as_deref(), Some("655e2"));
    /// assert_eq!(text(0.1), None);
    /// ```
    #[must_use]
    pub fn from_f16_shortest(x: f64) -> Option<Self> {
        if !x.is_finite() || binary16_bits(x).is_none() {
            return None;
        }
        let magnitude = x.abs();
        Some(super::decimal::closest_shortest(
            x.is_sign_negative(),
            &format!("{magnitude:.767e}"),
            1,
            |text| {
                Self::parse(text)
                    .is_ok_and(|d| binary16_nearest(&d).to_bits() == magnitude.to_bits())
            },
        ))
    }
}
