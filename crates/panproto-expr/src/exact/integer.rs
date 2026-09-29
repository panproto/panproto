//! Integers outside the range of `i64`.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::{
    ExactKind, ExactParseError, FidelityReason, cmp_digit_strings, is_canonical_digits, put_str,
    strip_leading_zeros,
};

/// An integer outside the range of `i64`: `(-1)^negative × digits`.
///
/// `digits` is a non-empty string of ASCII digits with no leading zero.
/// The type holds only integers that do not fit `i64`: every constructor
/// returns an [`ExactInteger`], which is [`ExactInteger::Int`] for an
/// integer that fits, so each integer has exactly one representation.
/// Deserializing a `BigIntValue` that fits `i64` fails for the same
/// reason.
///
/// ```
/// use panproto_expr::exact::{BigIntValue, ExactInteger};
///
/// assert_eq!(BigIntValue::parse("-42")?, ExactInteger::Int(-42));
/// let ExactInteger::Big(big) = BigIntValue::parse("18446744073709551615")? else {
///     unreachable!("u64::MAX does not fit i64")
/// };
/// assert_eq!(big.to_string(), "18446744073709551615");
/// # Ok::<(), panproto_expr::ExactParseError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "BigIntParts")]
pub struct BigIntValue {
    /// Whether the integer is negative.
    pub negative: bool,
    /// The digits of the magnitude, without leading zeros.
    pub digits: String,
}

/// An exact integer in its one representation: an `i64` when it fits,
/// else a [`BigIntValue`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExactInteger {
    /// An integer in the range of `i64`.
    Int(i64),
    /// An integer outside the range of `i64`.
    Big(BigIntValue),
}

/// The unchecked fields of a [`BigIntValue`], as they are deserialized.
#[derive(Deserialize)]
struct BigIntParts {
    negative: bool,
    digits: String,
}

impl TryFrom<BigIntParts> for BigIntValue {
    type Error = ExactParseError;

    fn try_from(parts: BigIntParts) -> Result<Self, Self::Error> {
        match Self::from_parts(parts.negative, &parts.digits)? {
            ExactInteger::Big(big) if big.digits == parts.digits => Ok(big),
            // A leading zero, or a value an `i64` holds: neither is the one
            // representation of its integer.
            _ => Err(not_an_integer()),
        }
    }
}

const fn not_an_integer() -> ExactParseError {
    ExactParseError::new(ExactKind::BigInt, FidelityReason::NotANumber)
}

/// The `i64` that `(-1)^negative × digits` is, if it is one; `digits` is a
/// canonical digit string.
fn fit_i64(negative: bool, digits: &str) -> Option<i64> {
    // `i64::MIN` has nineteen digits, so anything longer does not fit.
    if digits.len() > 19 {
        return None;
    }
    let magnitude = digits.parse::<i128>().ok()?;
    let value = if negative { -magnitude } else { magnitude };
    i64::try_from(value).ok()
}

impl BigIntValue {
    /// The integer `(-1)^negative × digits` in its one representation.
    ///
    /// Leading zeros of `digits` are dropped, and a negative zero is zero.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::NotANumber`] when `digits` is empty or holds
    /// anything but ASCII digits.
    pub fn from_parts(negative: bool, digits: &str) -> Result<ExactInteger, ExactParseError> {
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(not_an_integer());
        }
        let digits = strip_leading_zeros(digits);
        Ok(fit_i64(negative, digits).map_or_else(
            || {
                ExactInteger::Big(Self {
                    negative,
                    digits: digits.to_owned(),
                })
            },
            ExactInteger::Int,
        ))
    }

    /// Read an integer lexeme: an optional sign and one or more ASCII
    /// digits.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::NotANumber`] for anything else.
    pub fn parse(text: &str) -> Result<ExactInteger, ExactParseError> {
        let (negative, digits) = match text.as_bytes().first() {
            Some(b'-') => (true, &text[1..]),
            Some(b'+') => (false, &text[1..]),
            _ => (false, text),
        };
        Self::from_parts(negative, digits)
    }

    /// The integer `value` in its one representation.
    #[must_use]
    pub fn from_i128(value: i128) -> ExactInteger {
        i64::try_from(value).map_or_else(
            |_| {
                ExactInteger::Big(Self {
                    negative: value < 0,
                    digits: value.unsigned_abs().to_string(),
                })
            },
            ExactInteger::Int,
        )
    }

    /// The nearest `f64`. Lossy in general.
    #[must_use]
    pub fn to_f64(&self) -> f64 {
        let sign = if self.negative { "-" } else { "" };
        format!("{sign}{}", self.digits)
            .parse::<f64>()
            .unwrap_or(f64::NAN)
    }

    /// Compare two integers as numbers.
    #[must_use]
    pub fn cmp_numeric(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => cmp_digit_strings(&self.digits, &other.digits),
            (true, true) => cmp_digit_strings(&other.digits, &self.digits),
        }
    }

    /// Compare this integer with an `i64`, which it never equals.
    #[must_use]
    pub const fn cmp_i64(&self, _other: i64) -> Ordering {
        // Outside the range of `i64`, so below every `i64` when negative
        // and above every one otherwise.
        if self.negative {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }

    /// Whether the digit string and sign keep the type's invariants.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        is_canonical_digits(&self.digits) && fit_i64(self.negative, &self.digits).is_none()
    }

    /// The canonical payload bytes: the sign as one byte (`0` or `1`), then
    /// the digits behind a `u64` length.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(9 + self.digits.len());
        out.push(u8::from(self.negative));
        put_str(&mut out, &self.digits);
        out
    }
}

impl ExactInteger {
    /// Compare two integers as numbers.
    #[must_use]
    pub fn cmp_numeric(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a.cmp(b),
            (Self::Big(a), Self::Big(b)) => a.cmp_numeric(b),
            (Self::Big(a), Self::Int(b)) => a.cmp_i64(*b),
            (Self::Int(a), Self::Big(b)) => b.cmp_i64(*a).reverse(),
        }
    }
}

impl std::fmt::Display for BigIntValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.negative {
            f.write_str("-")?;
        }
        f.write_str(&self.digits)
    }
}

impl std::fmt::Display for ExactInteger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(n) => write!(f, "{n}"),
            Self::Big(b) => write!(f, "{b}"),
        }
    }
}
