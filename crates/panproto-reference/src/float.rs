//! IEEE 754 binary interchange formats by exact rationals.
//!
//! Decoding a bit pattern, the round-to-nearest-even lexical mapping `L_b`, the closest-shortest
//! printer `C_b` with ties to even, and the bit-wise narrowing and widening of NaNs.
//!
//! No float arithmetic is used. A finite value is a [`Rat`]; rounding
//! compares the exact remainder with one half.

use std::cmp::Ordering;

use crate::num::{Int, Nat, Rat, floor_log2, floor_log10};

/// A binary interchange format of width 16, 32 or 64.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Format {
    /// binary16: 11-bit significand, 5-bit exponent.
    B16,
    /// binary32: 24-bit significand, 8-bit exponent.
    B32,
    /// binary64: 53-bit significand, 11-bit exponent.
    B64,
}

impl Format {
    /// The format of width `bits`, if it is 16, 32 or 64.
    #[must_use]
    pub const fn of_width(bits: u8) -> Option<Self> {
        match bits {
            16 => Some(Self::B16),
            32 => Some(Self::B32),
            64 => Some(Self::B64),
            _ => None,
        }
    }

    /// The width in bits.
    #[must_use]
    pub const fn width(self) -> u32 {
        match self {
            Self::B16 => 16,
            Self::B32 => 32,
            Self::B64 => 64,
        }
    }

    /// The precision p (significand bits, the implicit bit included).
    #[must_use]
    pub const fn precision(self) -> u32 {
        match self {
            Self::B16 => 11,
            Self::B32 => 24,
            Self::B64 => 53,
        }
    }

    /// The number of exponent bits.
    #[must_use]
    pub const fn exponent_bits(self) -> u32 {
        self.width() - self.precision()
    }

    /// The exponent bias, `2^(w-1) - 1`.
    #[must_use]
    pub const fn bias(self) -> i64 {
        (1i64 << (self.exponent_bits() - 1)) - 1
    }

    /// The least normal exponent, `1 - bias`.
    #[must_use]
    pub const fn emin(self) -> i64 {
        1 - self.bias()
    }

    /// The greatest exponent, `bias`.
    #[must_use]
    pub const fn emax(self) -> i64 {
        self.bias()
    }

    /// The `max` of `PrecisionExceeded` at this width: `1 + ceil(p log10 2)`,
    /// which is 5, 9 and 17 for p = 11, 24 and 53.
    #[must_use]
    pub const fn max_significant_digits(self) -> u32 {
        match self {
            Self::B16 => 5,
            Self::B32 => 9,
            Self::B64 => 17,
        }
    }

    const fn fraction_mask(self) -> u64 {
        (1u64 << (self.precision() - 1)) - 1
    }

    const fn exponent_mask(self) -> u64 {
        (1u64 << self.exponent_bits()) - 1
    }

    const fn sign_bit(self) -> u64 {
        1u64 << (self.width() - 1)
    }
}

/// What a bit pattern denotes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FloatValue {
    /// A finite value, with the sign of zero kept apart (`negative` is the
    /// sign bit; for a non-zero value it agrees with the sign of `value`).
    Finite {
        /// The sign bit.
        negative: bool,
        /// The exact value.
        value: Rat,
    },
    /// An infinity.
    Infinite {
        /// The sign bit.
        negative: bool,
    },
    /// A NaN.
    NaN {
        /// The sign bit.
        negative: bool,
        /// The fraction field (quiet bit included).
        fraction: u64,
    },
}

/// Decode `bits` (the low `format.width()` bits) under `format`.
///
/// Definition (IEEE 754-2019 Sec 3.4): with sign s, biased exponent E and
/// fraction F, all-ones E is an infinity (F = 0) or a NaN; E = 0 is the
/// subnormal `(-1)^s F 2^(emin - p + 1)`; otherwise the normal
/// `(-1)^s (2^(p-1) + F) 2^(E - bias - p + 1)`.
#[must_use]
pub fn decode(format: Format, bits: u64) -> FloatValue {
    let p = i64::from(format.precision());
    let negative = bits & format.sign_bit() != 0;
    let e_field = (bits >> (format.precision() - 1)) & format.exponent_mask();
    let fraction = bits & format.fraction_mask();
    if e_field == format.exponent_mask() {
        return if fraction == 0 {
            FloatValue::Infinite { negative }
        } else {
            FloatValue::NaN { negative, fraction }
        };
    }
    let (significand, exponent) = if e_field == 0 {
        (fraction, format.emin() - p + 1)
    } else {
        (
            fraction | (1u64 << (format.precision() - 1)),
            i64::try_from(e_field).unwrap_or(0) - format.bias() - p + 1,
        )
    };
    let magnitude =
        Rat::from_int(Int::new(false, Nat::from_u64(significand))).mul(&Rat::pow2(exponent));
    let value = if negative { magnitude.neg() } else { magnitude };
    FloatValue::Finite { negative, value }
}

/// The result of rounding a rational to a format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rounded {
    /// A finite bit pattern.
    Finite(u64),
    /// The rounding overflowed to an infinity of this sign.
    Overflow {
        /// The sign.
        negative: bool,
    },
}

impl Rounded {
    /// The bit pattern, an overflow giving the infinity of its sign.
    #[must_use]
    pub const fn bits(self, format: Format) -> u64 {
        match self {
            Self::Finite(b) => b,
            Self::Overflow { negative } => {
                let inf = format.exponent_mask() << (format.precision() - 1);
                if negative {
                    inf | format.sign_bit()
                } else {
                    inf
                }
            }
        }
    }
}

/// `L_b`.
///
/// Round the exact value `q` to the nearest value of `format`, ties to the even significand (IEEE
/// 754-2019 Sec 4.3.1, roundTiesToEven); `negative_zero` gives the sign of a zero `q` (a numeral
/// `-0` reads as negative zero).
///
/// Definition: with `e = floor(log2 |q|)` and the quantum exponent
/// `qe = max(e, emin) - p + 1`, the significand is `|q| / 2^qe` rounded to
/// an integer n by comparing the remainder with 1/2 (ties to even n); a
/// carry to `2^p` renormalizes; a value of exponent above `emax` overflows.
#[must_use]
pub fn round_to(format: Format, value: &Rat, negative_zero: bool) -> Rounded {
    let precision = i64::from(format.precision());
    let negative = if value.is_zero() {
        negative_zero
    } else {
        value.is_negative()
    };
    let sign = if negative { format.sign_bit() } else { 0 };
    if value.is_zero() {
        return Rounded::Finite(sign);
    }
    let magnitude = value.abs();
    let log2 = floor_log2(&magnitude).unwrap_or(0);
    let mut qe = log2.max(format.emin()) - precision + 1;
    let scaled = magnitude.div(&Rat::pow2(qe)).unwrap_or_else(Rat::zero);
    let floor = scaled.floor();
    let rem = scaled.sub(&Rat::from_int(floor.clone()));
    let half = Rat::pow2(-1);
    let mut significand = floor.magnitude().clone();
    let round_up = match rem.cmp(&half) {
        Ordering::Greater => true,
        Ordering::Equal => !significand.is_even(),
        Ordering::Less => false,
    };
    if round_up {
        significand = significand.add(&Nat::one());
    }
    if significand == Nat::pow2(format.precision()) {
        significand = Nat::pow2(format.precision() - 1);
        qe += 1;
    }
    if significand.is_zero() {
        return Rounded::Finite(sign);
    }
    if qe + precision - 1 > format.emax() {
        return Rounded::Overflow { negative };
    }
    let n64 = significand.to_u64().unwrap_or(0);
    let implicit = 1u64 << (format.precision() - 1);
    if n64 < implicit {
        return Rounded::Finite(sign | n64);
    }
    let biased = u64::try_from(qe + precision - 1 + format.bias()).unwrap_or(0);
    Rounded::Finite(sign | (biased << (format.precision() - 1)) | (n64 - implicit))
}

/// Whether the exact value `q` is a value of `format` (rounding returns a
/// finite pattern that decodes to `q`).
#[must_use]
pub fn holds_exactly(format: Format, q: &Rat) -> bool {
    match round_to(format, q, false) {
        Rounded::Finite(bits) => match decode(format, bits) {
            FloatValue::Finite { value, .. } => value == *q,
            _ => false,
        },
        Rounded::Overflow { .. } => false,
    }
}

/// A decimal number `(-1)^negative * coefficient * 10^exponent`, with the
/// coefficient's trailing zeros moved into the exponent (zero is `0e0`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Dec {
    /// The sign (kept for a negative zero).
    pub negative: bool,
    /// The coefficient, without trailing zero digits unless it is zero.
    pub coefficient: Nat,
    /// The power of ten.
    pub exponent: i64,
}

impl Dec {
    /// Normalize: strip trailing zeros of the coefficient into the exponent.
    #[must_use]
    pub fn normalized(negative: bool, coefficient: &Nat, exponent: i64) -> Self {
        if coefficient.is_zero() {
            return Self {
                negative,
                coefficient: Nat::zero(),
                exponent: 0,
            };
        }
        let text = coefficient.to_string();
        let trimmed = text.trim_end_matches('0');
        let zeros = i64::try_from(text.len() - trimmed.len()).unwrap_or(0);
        Self {
            negative,
            coefficient: Nat::parse(trimmed).unwrap_or_default(),
            exponent: exponent + zeros,
        }
    }

    /// The number of significant digits (zero has one).
    #[must_use]
    pub fn significant_digits(&self) -> usize {
        self.coefficient.digit_count().max(1)
    }

    /// The exact value.
    #[must_use]
    pub fn value(&self) -> Rat {
        let m = Rat::from_int(Int::new(self.negative, self.coefficient.clone()));
        m.mul(&Rat::pow10(self.exponent))
    }

    /// Plain scientific text `[-]d[.ddd]e<exp>`, the form tests read.
    #[must_use]
    pub fn to_scientific(&self) -> String {
        let digits = self.coefficient.to_string();
        let sign = if self.negative { "-" } else { "" };
        let point_exp = self.exponent + i64::try_from(digits.len()).unwrap_or(0) - 1;
        if digits.len() == 1 {
            format!("{sign}{digits}e{point_exp}")
        } else {
            format!("{sign}{}.{}e{point_exp}", &digits[..1], &digits[1..])
        }
    }
}

/// The rounding interval of a positive finite pattern `bits`: the exact
/// midpoints to its neighbours below and above. The neighbour below the
/// least subnormal is zero; the "neighbour" above the greatest finite value
/// is `2^(emax+1)`, the point at which rounding overflows.
fn midpoints(format: Format, bits: u64) -> Option<(Rat, Rat, Rat)> {
    let FloatValue::Finite { value, .. } = decode(format, bits) else {
        return None;
    };
    let below = if bits == 0 {
        Rat::zero()
    } else {
        match decode(format, bits - 1) {
            FloatValue::Finite { value, .. } => value,
            _ => return None,
        }
    };
    let above = match decode(format, bits + 1) {
        FloatValue::Finite { value, .. } => value,
        FloatValue::Infinite { .. } => Rat::pow2(format.emax() + 1),
        FloatValue::NaN { .. } => return None,
    };
    let half = Rat::pow2(-1);
    let lo = value.add(&below).mul(&half);
    let hi = value.add(&above).mul(&half);
    Some((value, lo, hi))
}

/// `C_b`: the closest-shortest decimal of the finite pattern `bits`, ties to
/// even.
///
/// Definition: among the decimals with the fewest significant digits that
/// read back to the value under [`round_to`], the one closest to the value,
/// and at an exact tie the one whose last significant digit is even. The
/// search is naive: for k = 1, 2, ... every multiple of `10^(e-k+1)` lying
/// between the two rounding midpoints (`e = floor(log10 |x|)`) is a
/// candidate of at most k significant digits; each candidate is read back
/// with [`round_to`], and the first k with a candidate that reads back is
/// the minimal length. Returns `None` for a NaN or an infinity.
#[must_use]
pub fn shortest(format: Format, bits: u64) -> Option<Dec> {
    let negative = bits & format.sign_bit() != 0;
    let magnitude_bits = bits & !format.sign_bit();
    let (x, lo, hi) = midpoints(format, magnitude_bits)?;
    if x.is_zero() {
        return Some(Dec::normalized(negative, &Nat::zero(), 0));
    }
    let e = floor_log10(&x)?;
    for k in 1i64.. {
        let unit_exp = e - k + 1;
        let unit = Rat::pow10(unit_exp);
        let first = ceil_div(&lo, &unit);
        let last = lo_floor_div(&hi, &unit);
        let mut best: Option<(Nat, Rat)> = None;
        let mut c = first;
        while c <= last {
            let candidate = Rat::from_int(Int::new(false, c.clone())).mul(&unit);
            if round_to(format, &candidate, false) == Rounded::Finite(magnitude_bits) {
                let distance = candidate.sub(&x).abs();
                best = Some(match best {
                    None => (c.clone(), distance),
                    Some((bc, bd)) => pick_closer(bc, bd, c.clone(), distance),
                });
            }
            c = c.add(&Nat::one());
        }
        if let Some((coefficient, _)) = best {
            return Some(Dec::normalized(negative, &coefficient, unit_exp));
        }
    }
    None
}

/// The closer of two candidate coefficients, ties to the one whose last
/// significant digit is even.
fn pick_closer(a: Nat, da: Rat, b: Nat, db: Rat) -> (Nat, Rat) {
    match da.cmp(&db) {
        Ordering::Less => (a, da),
        Ordering::Greater => (b, db),
        Ordering::Equal => {
            if last_significant_digit(&a) % 2 == 0 {
                (a, da)
            } else {
                (b, db)
            }
        }
    }
}

fn last_significant_digit(n: &Nat) -> u8 {
    (0..n.digit_count())
        .map(|i| n.digit(i))
        .find(|d| *d != 0)
        .unwrap_or(0)
}

/// `ceil(a / u)` for non-negative `a` and positive `u`, as a natural.
fn ceil_div(a: &Rat, u: &Rat) -> Nat {
    let q = a.div(u).unwrap_or_else(Rat::zero);
    let f = q.floor();
    if q.is_integer() {
        f.magnitude().clone()
    } else {
        f.magnitude().add(&Nat::one())
    }
}

/// `floor(a / u)` for non-negative `a` and positive `u`, as a natural.
fn lo_floor_div(a: &Rat, u: &Rat) -> Nat {
    a.div(u)
        .unwrap_or_else(Rat::zero)
        .floor()
        .magnitude()
        .clone()
}

/// The binary64 pattern that widens the `format` pattern `bits` on the bits.
///
/// A finite value exactly, an infinity to the infinity of its sign, and a NaN to the NaN with its
/// sign, an all-ones exponent and its fraction shifted to the high fraction bits of binary64 (quiet
/// or signalling as written; no quieting).
#[must_use]
pub fn widen_bits(format: Format, bits: u64) -> u64 {
    match decode(format, bits) {
        FloatValue::Finite { negative, value } => {
            round_to(Format::B64, &value, negative).bits(Format::B64)
        }
        FloatValue::Infinite { negative } => Rounded::Overflow { negative }.bits(Format::B64),
        FloatValue::NaN { negative, fraction } => {
            let shift = Format::B64.precision() - format.precision();
            let sign = if negative { Format::B64.sign_bit() } else { 0 };
            sign | (Format::B64.exponent_mask() << 52) | (fraction << shift)
        }
    }
}

/// The `format` pattern of the binary64 pattern `bits` when `format` holds it exactly, on the bits.
///
/// A finite value iff it is a value of `format`, an infinity always, and a NaN iff its fraction
/// bits below the narrow fraction are zero (quiet or signalling). `None` otherwise.
#[must_use]
pub fn narrow_bits(format: Format, bits: u64) -> Option<u64> {
    match decode(Format::B64, bits) {
        FloatValue::Finite { negative, value } => {
            if !holds_exactly(format, &value) {
                return None;
            }
            Some(round_to(format, &value, negative).bits(format))
        }
        FloatValue::Infinite { negative } => Some(Rounded::Overflow { negative }.bits(format)),
        FloatValue::NaN { negative, fraction } => {
            let shift = Format::B64.precision() - format.precision();
            if fraction & ((1u64 << shift) - 1) != 0 {
                return None;
            }
            let sign = if negative { format.sign_bit() } else { 0 };
            Some(sign | (format.exponent_mask() << (format.precision() - 1)) | (fraction >> shift))
        }
    }
}

/// The canonical quiet NaN of binary64, bits `0x7ff8000000000000`.
pub const CANONICAL_NAN_BITS: u64 = 0x7ff8_0000_0000_0000;

/// The negative canonical NaN of binary64, bits `0xfff8000000000000`.
pub const NEGATIVE_CANONICAL_NAN_BITS: u64 = 0xfff8_0000_0000_0000;

#[cfg(test)]
mod tests {
    use super::*;

    fn rat(text: &str) -> Rat {
        let (n, d) = text.split_once('/').unwrap_or((text, "1"));
        Rat::new(
            Int::parse(n).unwrap_or_default(),
            Nat::parse(d).unwrap_or_default(),
        )
        .unwrap_or_else(Rat::zero)
    }

    #[test]
    fn worked_examples_round_to_the_published_patterns() {
        // binary64 nearest 1/10 is 0x3fb999999999999a; binary32 nearest is
        // 0x3dcccccd; binary16 nearest is 0x2e66 (IEEE 754 worked values).
        assert_eq!(
            round_to(Format::B64, &rat("1/10"), false),
            Rounded::Finite(0x3fb9_9999_9999_999a)
        );
        assert_eq!(
            round_to(Format::B32, &rat("1/10"), false),
            Rounded::Finite(0x3dcc_cccd)
        );
        assert_eq!(
            round_to(Format::B16, &rat("1/10"), false),
            Rounded::Finite(0x2e66)
        );
        // 65504 is the largest binary16; 65520 rounds to infinity; 65519
        // rounds down to 65504.
        assert_eq!(
            round_to(Format::B16, &rat("65504"), false),
            Rounded::Finite(0x7bff)
        );
        assert_eq!(
            round_to(Format::B16, &rat("65520"), false),
            Rounded::Overflow { negative: false }
        );
        assert_eq!(
            round_to(Format::B16, &rat("65519"), false),
            Rounded::Finite(0x7bff)
        );
        // 2049 is a tie between 2048 and 2050 at binary16; even is 2048.
        assert_eq!(
            round_to(Format::B16, &rat("2049"), false),
            Rounded::Finite(0x6800)
        );
        // 2^-1075 is a tie between 0 and the least subnormal; even is 0.
        assert_eq!(
            round_to(Format::B64, &Rat::pow2(-1075), false),
            Rounded::Finite(0)
        );
        assert_eq!(
            round_to(Format::B64, &Rat::zero(), true),
            Rounded::Finite(0x8000_0000_0000_0000)
        );
    }

    #[test]
    fn every_binary16_pattern_decodes_and_rounds_back_to_itself() {
        for bits in 0u64..=0xffff {
            if let FloatValue::Finite { negative, value } = decode(Format::B16, bits) {
                assert_eq!(
                    round_to(Format::B16, &value, negative),
                    Rounded::Finite(bits),
                    "{bits:#x}"
                );
            }
        }
    }

    #[test]
    fn the_printer_breaks_exact_ties_to_even() {
        // 2^50 + 0.25 prints "1125899906842624.2" (both .2 and .3 read
        // back); 52401.3125 at binary32 prints "52401.312".
        let x = Rat::pow2(50).add(&Rat::pow2(-2));
        let Rounded::Finite(bits) = round_to(Format::B64, &x, false) else {
            return;
        };
        let d =
            shortest(Format::B64, bits).unwrap_or_else(|| Dec::normalized(false, &Nat::zero(), 0));
        assert_eq!(d.to_scientific(), "1.1258999068426242e15");
        let y = rat("838421/16");
        let Rounded::Finite(b32) = round_to(Format::B32, &y, false) else {
            return;
        };
        let d =
            shortest(Format::B32, b32).unwrap_or_else(|| Dec::normalized(false, &Nat::zero(), 0));
        assert_eq!(d.to_scientific(), "5.2401312e4");
        // The binary32 nearest 0.1 prints "0.1".
        let d = shortest(Format::B32, 0x3dcc_cccd)
            .unwrap_or_else(|| Dec::normalized(false, &Nat::zero(), 0));
        assert_eq!(d.to_scientific(), "1e-1");
    }

    #[test]
    fn nan_narrowing_decides_on_the_low_fraction_bits() {
        assert_eq!(
            narrow_bits(Format::B32, 0x7ff0_0000_2000_0000),
            Some(0x7f80_0001)
        );
        assert_eq!(narrow_bits(Format::B32, 0x7ff8_0000_0000_1234), None);
        assert_eq!(
            narrow_bits(Format::B32, NEGATIVE_CANONICAL_NAN_BITS),
            Some(0xffc0_0000)
        );
        assert_eq!(widen_bits(Format::B32, 0x7f80_0001), 0x7ff0_0000_2000_0000);
        assert_eq!(widen_bits(Format::B16, 0x7e00), CANONICAL_NAN_BITS);
        assert_eq!(narrow_bits(Format::B16, 0x3fb9_9999_9999_999a), None);
    }
}
