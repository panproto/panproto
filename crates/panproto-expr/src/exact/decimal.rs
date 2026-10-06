//! Exact decimal numbers.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::{
    ExactKind, ExactParseError, FidelityReason, is_canonical_digits, put_str, strip_leading_zeros,
};

/// An exact decimal number: `(-1)^negative × digits × 10^exponent`.
///
/// `digits` is a non-empty string of ASCII digits with no leading zero
/// (except the single digit `"0"`), and zero is never negative. The
/// representation keeps the scale the number was written with, so `1.50`
/// (`digits` `"150"`, `exponent` `-2`) and `1.5` (`"15"`, `-1`) are
/// different values under `==`; [`cmp_numeric`](Self::cmp_numeric)
/// compares them as numbers.
///
/// This is the abstract representation of the General Decimal Arithmetic
/// specification (Cowlishaw 2009), of XSD's precisionDecimal (Peterson and
/// Sperberg-McQueen 2011, Secs 3.1 to 3.2) and of Java's `BigDecimal`
/// (Oracle 2023, Java SE 21 API), restricted to finite values without a
/// negative zero: a coefficient (`digits`) and an exponent, so a number has
/// one representation per scale. Representation equality (`==`) is GDA's
/// compare-total identity and `BigDecimal.equals` (`2.0` differs from
/// `2.00`); [`cmp_numeric`](Self::cmp_numeric) is GDA's compare,
/// precisionDecimal's equality and order, and `BigDecimal.compareTo`
/// (`2.0` equals `2.00`), and is also the equality of XSD's `xs:decimal`,
/// whose value space does not keep the scale (W3C 2012, XML Schema 1.1
/// Part 2: Datatypes, Sec 3.3.3, stated). [`scale`](Self::scale) is the
/// number of fractional digits written, clamped at zero, while `exponent`
/// keeps a positive power of ten (`1e3` has exponent 3, where `BigDecimal`
/// would record the scale -3).
///
/// ```
/// use std::cmp::Ordering;
///
/// use panproto_expr::exact::DecimalValue;
///
/// let a = DecimalValue::parse("1.50")?;
/// let b = DecimalValue::parse("1.5")?;
/// assert_ne!(a, b);
/// assert_eq!(a.cmp_numeric(&b), Ordering::Equal);
/// assert_eq!((a.scale(), b.scale()), (2, 1));
/// assert_eq!(a.to_string(), "1.50");
/// # Ok::<(), panproto_expr::ExactParseError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "DecimalParts")]
pub struct DecimalValue {
    /// Whether the number is negative; never set for zero.
    pub negative: bool,
    /// The significant digits, without leading zeros.
    pub digits: String,
    /// The power of ten the digits are scaled by.
    pub exponent: i32,
}

/// The unchecked fields of a [`DecimalValue`], as they are deserialized.
#[derive(Deserialize)]
struct DecimalParts {
    negative: bool,
    digits: String,
    exponent: i32,
}

impl TryFrom<DecimalParts> for DecimalValue {
    type Error = ExactParseError;

    fn try_from(parts: DecimalParts) -> Result<Self, Self::Error> {
        Self::new(parts.negative, parts.digits, parts.exponent)
    }
}

const fn not_a_number() -> ExactParseError {
    ExactParseError::new(ExactKind::Decimal, FidelityReason::NotANumber)
}

const fn overflow() -> ExactParseError {
    ExactParseError::new(ExactKind::Decimal, FidelityReason::Overflow)
}

/// The stored exponent of a decimal lexeme: its written exponent (negative
/// when `exponent_negative`, with significant digits `exponent_digits`) less
/// its fractional digit count `frac_len`, or `None` when that leaves `i32`.
///
/// The difference is exact. A written exponent of more than twenty
/// significant digits has magnitude at least 10^20: negative, the difference
/// is below `i32::MIN`; positive, it exceeds `i32::MAX`, since `frac_len` is
/// at most `u64::MAX` < 10^20 - `i32::MAX`. Twenty digits or fewer fit `i128`
/// together with any `frac_len`.
fn stored_exponent(exponent_negative: bool, exponent_digits: &str, frac_len: usize) -> Option<i32> {
    if exponent_digits.len() > 20 {
        return None;
    }
    let magnitude = i128::try_from(exponent_digits.parse::<u128>().ok()?).ok()?;
    let written = if exponent_negative {
        -magnitude
    } else {
        magnitude
    };
    let frac_len = i128::try_from(frac_len).ok()?;
    i32::try_from(written - frac_len).ok()
}

/// Whether `text` (sign and case ignored) names a NaN or an infinity.
fn names_non_finite(text: &str) -> bool {
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    ["nan", "inf", "infinity"]
        .iter()
        .any(|name| unsigned.eq_ignore_ascii_case(name))
}

impl DecimalValue {
    /// The decimal `(-1)^negative × digits × 10^exponent`.
    ///
    /// A negative zero is read as zero.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::NotANumber`] when `digits` is empty, holds
    /// anything but ASCII digits, or has a leading zero.
    pub fn new(
        negative: bool,
        digits: impl Into<String>,
        exponent: i32,
    ) -> Result<Self, ExactParseError> {
        let digits = digits.into();
        if !is_canonical_digits(&digits) {
            return Err(not_a_number());
        }
        let negative = negative && digits != "0";
        Ok(Self {
            negative,
            digits,
            exponent,
        })
    }

    /// Read a decimal lexeme.
    ///
    /// Accepts the JSON number grammar and the decimal forms of SQL and XML
    /// Schema: an optional sign, digits with an optional fractional part
    /// (either side of the point may be empty, not both), and an optional
    /// exponent `e`/`E` with its own optional sign. Leading zeros are
    /// dropped; trailing fractional zeros are kept as scale.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::NonFinite`] for a NaN or infinity spelling,
    /// [`FidelityReason::Overflow`] exactly when the stored exponent (the
    /// written exponent, 0 when absent, less the number of fractional digits)
    /// leaves `i32`, and
    /// [`FidelityReason::NotANumber`] for anything else that is not a
    /// decimal lexeme.
    pub fn parse(text: &str) -> Result<Self, ExactParseError> {
        if names_non_finite(text) {
            return Err(ExactParseError::new(
                ExactKind::Decimal,
                FidelityReason::NonFinite,
            ));
        }
        let (negative, rest) = match text.as_bytes().first() {
            Some(b'-') => (true, &text[1..]),
            Some(b'+') => (false, &text[1..]),
            _ => (false, text),
        };
        let (mantissa, exponent_text) = rest
            .find(['e', 'E'])
            .map_or((rest, None), |at| (&rest[..at], Some(&rest[at + 1..])));
        let (int_part, frac_part) = mantissa
            .find('.')
            .map_or((mantissa, ""), |at| (&mantissa[..at], &mantissa[at + 1..]));
        let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
        if (int_part.is_empty() && frac_part.is_empty())
            || !all_digits(int_part)
            || !all_digits(frac_part)
        {
            return Err(not_a_number());
        }
        let (exponent_negative, exponent_digits) = match exponent_text {
            None => (false, "0"),
            Some(e) => {
                let (negative, magnitude) = match e.as_bytes().first() {
                    Some(b'-') => (true, &e[1..]),
                    Some(b'+') => (false, &e[1..]),
                    _ => (false, e),
                };
                if magnitude.is_empty() || !all_digits(magnitude) {
                    return Err(not_a_number());
                }
                (negative, strip_leading_zeros(magnitude))
            }
        };
        let exponent = stored_exponent(exponent_negative, exponent_digits, frac_part.len())
            .ok_or_else(overflow)?;
        let mut digits = String::with_capacity(int_part.len() + frac_part.len());
        digits.push_str(int_part);
        digits.push_str(frac_part);
        let digits = strip_leading_zeros(&digits).to_owned();
        Self::new(negative, digits, exponent)
    }

    /// The decimal equal to `value`.
    #[must_use]
    pub fn from_i64(value: i64) -> Self {
        Self {
            negative: value < 0,
            digits: value.unsigned_abs().to_string(),
            exponent: 0,
        }
    }

    /// The shortest decimal that reads back as `x`, or `None` when `x` is
    /// not finite.
    ///
    /// This is the decimal Rust's float formatting writes (`{:e}`): among
    /// the decimals with the fewest significant digits (the minimal
    /// significand) that read back to exactly `x` under
    /// round-to-nearest-even, the one closest to `x`, with ties broken
    /// away from zero, so of two such decimals equally close to `x` the one of
    /// larger magnitude (`2229016984128891.25` gives `2229016984128891.3`;
    /// at 32 bits the same rule writes the binary32 value `52401.3125` as
    /// `52401.313`). ECMA-262's `Number::toString` recommends ties to even
    /// instead (Ecma International TC39, ECMAScript Language Specification,
    /// Sec 6.1.6.1.20, Note 2, stated); that recommendation is not
    /// followed. So `0.1` gives `1 × 10^-1`, not the float's exact binary
    /// expansion. A negative zero gives zero.
    #[must_use]
    pub fn from_f64_shortest(x: f64) -> Option<Self> {
        if !x.is_finite() {
            return None;
        }
        Self::parse(&format!("{x:e}")).ok()
    }

    /// The nearest `f64`, which may be infinite or zero when the decimal is
    /// outside the range of `f64`. Lossy in general.
    #[must_use]
    pub fn to_f64(&self) -> f64 {
        let sign = if self.negative { "-" } else { "" };
        format!("{sign}{}e{}", self.digits, self.exponent)
            .parse::<f64>()
            .unwrap_or(f64::NAN)
    }

    /// Whether the decimal is zero, at any scale.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.digits == "0"
    }

    /// The number of digits written after the decimal point:
    /// `max(0, -exponent)`.
    #[must_use]
    pub const fn scale(&self) -> u32 {
        if self.exponent < 0 {
            self.exponent.unsigned_abs()
        } else {
            0
        }
    }

    /// The smallest scale that writes the same number: the scale with every
    /// trailing fractional zero dropped.
    #[must_use]
    pub fn minimal_scale(&self) -> u32 {
        let trailing = self.digits.len() - self.digits.trim_end_matches('0').len();
        let trailing = if self.is_zero() {
            u32::MAX
        } else {
            u32::try_from(trailing).unwrap_or(u32::MAX)
        };
        self.scale().saturating_sub(trailing)
    }

    /// The number of digits before the decimal point once leading zeros are
    /// dropped (`0` for a number whose magnitude is below one).
    #[must_use]
    pub fn integer_digits(&self) -> u64 {
        if self.is_zero() {
            return 0;
        }
        let position =
            i64::try_from(self.digits.len()).unwrap_or(i64::MAX) + i64::from(self.exponent);
        u64::try_from(position).unwrap_or(0)
    }

    /// The number of digits the number needs in positional notation: its
    /// [`integer_digits`](Self::integer_digits) plus its
    /// [`minimal_scale`](Self::minimal_scale).
    #[must_use]
    pub fn significant_digits(&self) -> u64 {
        u64::from(self.minimal_scale()).saturating_add(self.integer_digits())
    }

    /// Compare two decimals as numbers, ignoring scale.
    #[must_use]
    pub fn cmp_numeric(&self, other: &Self) -> Ordering {
        let sign = |d: &Self| match (d.is_zero(), d.negative) {
            (true, _) => 0_i8,
            (false, true) => -1,
            (false, false) => 1,
        };
        let (a, b) = (sign(self), sign(other));
        if a != b || a == 0 {
            return a.cmp(&b);
        }
        let magnitude = cmp_magnitude(self, other);
        if a < 0 {
            magnitude.reverse()
        } else {
            magnitude
        }
    }

    /// The canonical payload bytes: the sign as one byte (`0` or `1`), the
    /// digits behind a `u64` length, and the exponent as a little-endian
    /// `i64`.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(17 + self.digits.len());
        out.push(u8::from(self.negative));
        put_str(&mut out, &self.digits);
        out.extend_from_slice(&i64::from(self.exponent).to_le_bytes());
        out
    }
}

/// Compare the magnitudes of two non-zero decimals.
fn cmp_magnitude(left: &DecimalValue, right: &DecimalValue) -> Ordering {
    // The position of the leading digit decides unless it ties.
    let lead = |d: &DecimalValue| {
        i64::try_from(d.digits.len()).unwrap_or(i64::MAX) + i64::from(d.exponent)
    };
    lead(left).cmp(&lead(right)).then_with(|| {
        let (lhs, rhs) = (left.digits.as_bytes(), right.digits.as_bytes());
        (0..lhs.len().max(rhs.len()))
            .map(|i| {
                let dl = lhs.get(i).copied().unwrap_or(b'0');
                let dr = rhs.get(i).copied().unwrap_or(b'0');
                dl.cmp(&dr)
            })
            .find(|o| o.is_ne())
            .unwrap_or(Ordering::Equal)
    })
}

impl std::fmt::Display for DecimalValue {
    /// The lexeme [`DecimalValue::parse`] reads back into the same
    /// representation: plain positional notation, or `digits e exponent`
    /// when the exponent is positive or the number would need more than six
    /// leading zeros.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.negative {
            f.write_str("-")?;
        }
        let len = i64::try_from(self.digits.len()).unwrap_or(i64::MAX);
        let exponent = i64::from(self.exponent);
        if exponent > 0 || exponent < -(len + 6) {
            return write!(f, "{}e{}", self.digits, self.exponent);
        }
        if exponent == 0 {
            return f.write_str(&self.digits);
        }
        let scale = usize::try_from(-exponent).unwrap_or(usize::MAX);
        if scale >= self.digits.len() {
            f.write_str("0.")?;
            for _ in 0..scale - self.digits.len() {
                f.write_str("0")?;
            }
            f.write_str(&self.digits)
        } else {
            let (int, frac) = self.digits.split_at(self.digits.len() - scale);
            write!(f, "{int}.{frac}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_exponent_is_exact_at_the_i32_bounds() {
        // A twelve-digit written exponent comes back into range once enough
        // fractional digits are taken off: 10^11 - 97852516353 = i32::MAX.
        assert_eq!(
            stored_exponent(false, "100000000000", 97_852_516_353),
            Some(i32::MAX)
        );
        assert_eq!(stored_exponent(false, "100000000000", 97_852_516_352), None);
        // The bounds are reached exactly, with and without fractional digits.
        assert_eq!(stored_exponent(false, "2147483647", 0), Some(i32::MAX));
        assert_eq!(stored_exponent(true, "2147483648", 0), Some(i32::MIN));
        assert_eq!(stored_exponent(false, "2147483648", 1), Some(i32::MAX));
        assert_eq!(stored_exponent(true, "2147483647", 1), Some(i32::MIN));
        assert_eq!(stored_exponent(false, "0", 0), Some(0));
        assert_eq!(stored_exponent(true, "0", 3), Some(-3));
        // Just outside the bounds.
        assert_eq!(stored_exponent(false, "2147483648", 0), None);
        assert_eq!(stored_exponent(true, "2147483649", 0), None);
        assert_eq!(stored_exponent(true, "2147483648", 1), None);
        assert_eq!(stored_exponent(false, "0", 2_147_483_649), None);
        // Long exponents: twenty digits are computed, more are refused
        // whatever the fractional digit count.
        assert_eq!(stored_exponent(false, "100000000000000000000", 0), None);
        assert_eq!(
            stored_exponent(false, "100000000000000000000", usize::MAX),
            None
        );
        assert_eq!(stored_exponent(true, "100000000000000000000", 0), None);
        assert_eq!(
            stored_exponent(false, "99999999999999999999", usize::MAX),
            None
        );
        assert_eq!(stored_exponent(true, "99999999999999999999", 0), None);
        // Every exponent refused by the eleven-digit length rule with a
        // fractional digit count that leaves it outside `i32` stays refused.
        assert_eq!(stored_exponent(false, "999999999999", 0), None);
        assert_eq!(stored_exponent(true, "100000000000", 0), None);
        assert_eq!(stored_exponent(false, "99999999999", 0), None);
        assert_eq!(stored_exponent(true, "99999999999", 0), None);
    }

    #[test]
    fn parse_refuses_exactly_outside_the_stored_exponent_range() {
        let exponent = |text: &str| DecimalValue::parse(text).map(|value| value.exponent);
        assert_eq!(exponent("1e2147483647"), Ok(i32::MAX));
        assert_eq!(exponent("0.1e2147483648"), Ok(i32::MAX));
        assert_eq!(exponent("1e-2147483648"), Ok(i32::MIN));
        assert_eq!(exponent("1e000000000002147483647"), Ok(i32::MAX));
        for text in [
            "1e2147483648",
            "1.5e-2147483648",
            "1e99999999999",
            "1e100000000000",
            "1e99999999999999999999999",
            "1e-99999999999999999999999",
        ] {
            assert_eq!(exponent(text), Err(overflow()), "{text}");
        }
    }
}
