//! Decimal128: the BID encoding, and the strings of the BSON Decimal128
//! specification.
//!
//! A decimal128 is held as a [`DecimalValue`] that decimal128 holds as
//! written: a finite value of at most 34 coefficient digits and an exponent
//! from -6176 to 6111, an infinity, or a NaN, quiet or signalling, with a
//! payload of at most 33 digits. [`to_bid`] and [`from_bid`] are inverse
//! bijections between those values and the canonical BID encodings.

use panproto_expr::exact::{DecimalClass, DecimalValue, WideInt};

/// The least exponent of a finite decimal128.
const MIN_EXPONENT: i64 = -6176;
/// The greatest exponent of a finite decimal128.
const MAX_EXPONENT: i64 = 6111;
/// The most coefficient digits of a finite decimal128.
const MAX_DIGITS: usize = 34;
/// The most payload digits of a decimal128 NaN.
const MAX_PAYLOAD_DIGITS: usize = 33;
/// The exponent bias of the BID encoding.
const BIAS: i64 = 6176;

/// The sign bit.
const SIGN: u128 = 1 << 127;
/// The combination bits of an infinity (`11110` in bits 126 to 122).
const INFINITY: u128 = 0x1E << 122;
/// The combination bits of a quiet NaN (`11111` in bits 126 to 122).
const NAN: u128 = 0x1F << 122;
/// The signalling bit of a NaN (bit 121).
const SIGNALLING: u128 = 1 << 121;
/// The trailing significand of a NaN (bits 109 to 0).
const PAYLOAD: u128 = (1 << 110) - 1;
/// The coefficient bits of a finite value in the first form (bits 112 to
/// 0).
const COEFFICIENT: u128 = (1 << 113) - 1;

/// `10^n` for `n` at most 38.
const fn power_of_ten(n: u32) -> u128 {
    let mut value: u128 = 1;
    let mut i = 0;
    while i < n {
        value *= 10;
        i += 1;
    }
    value
}

/// Whether decimal128 holds `value` as written.
#[must_use]
pub fn representable(value: &DecimalValue) -> bool {
    match &value.class {
        DecimalClass::Finite => {
            value.digits.len() <= MAX_DIGITS
                && value
                    .exponent
                    .to_i64()
                    .is_some_and(|e| (MIN_EXPONENT..=MAX_EXPONENT).contains(&e))
        }
        DecimalClass::Infinite => true,
        DecimalClass::NaN { payload, .. } => payload.len() <= MAX_PAYLOAD_DIGITS,
    }
}

/// The canonical BID encoding of `value`, little-endian, or `None` when
/// decimal128 does not hold it as written.
#[must_use]
pub fn to_bid(value: &DecimalValue) -> Option<[u8; 16]> {
    if !representable(value) {
        return None;
    }
    let sign = if value.negative { SIGN } else { 0 };
    let bits = match &value.class {
        DecimalClass::Finite => {
            let coefficient: u128 = value.digits.parse().ok()?;
            let biased = u128::try_from(value.exponent.to_i64()? + BIAS).ok()?;
            sign | (biased << 113) | coefficient
        }
        DecimalClass::Infinite => sign | INFINITY,
        DecimalClass::NaN {
            signalling,
            payload,
        } => {
            let payload: u128 = payload.parse().ok()?;
            sign | NAN | if *signalling { SIGNALLING } else { 0 } | payload
        }
    };
    Some(bits.to_le_bytes())
}

/// The value of a canonical BID encoding, little-endian, or `None` when
/// the bits are not the canonical encoding of a value: a finite value in
/// the second form (combination bits `11` then not `11110` or `11111`),
/// whose coefficient is at least `2^113` and which IEEE 754 reads as zero; a
/// first-form coefficient above `10^34 - 1`, which IEEE 754 also reads as
/// zero; an infinity with any of bits 121 to 0 set; and a NaN with any of
/// bits 120 to 110 set or a payload above `10^33 - 1`.
#[must_use]
pub fn from_bid(bytes: [u8; 16]) -> Option<DecimalValue> {
    let bits = u128::from_le_bytes(bytes);
    let negative = bits & SIGN != 0;
    let combination = (bits >> 122) & 0x1F;
    if combination == 0x1F {
        let signalling = bits & SIGNALLING != 0;
        let ignored = bits & !(SIGN | NAN | SIGNALLING | PAYLOAD);
        let payload = bits & PAYLOAD;
        if ignored != 0 || payload >= power_of_ten(33) {
            return None;
        }
        return Some(DecimalValue::nan(negative, signalling, payload.to_string()));
    }
    if combination == 0x1E {
        return (bits & !(SIGN | INFINITY) == 0).then(|| DecimalValue::infinity(negative));
    }
    if (bits >> 125) & 0b11 == 0b11 {
        return None;
    }
    let coefficient = bits & COEFFICIENT;
    if coefficient >= power_of_ten(34) {
        return None;
    }
    let biased = i64::try_from((bits >> 113) & 0x3FFF).ok()?;
    DecimalValue::new(
        negative,
        coefficient.to_string(),
        WideInt::from(biased - BIAS),
    )
    .ok()
}

/// Why [`parse_decimal128`] refuses a string.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Decimal128ParseError {
    /// The string is not a numeric string of the BSON Decimal128
    /// specification.
    #[error("{0:?} is not a decimal128 numeric string")]
    Syntax(String),
    /// The string names a decimal that decimal128 holds only by clamping
    /// its exponent, by rounding (exactly or not), or not at all.
    #[error("{0:?} is not a decimal128 value as written")]
    Representation(String),
}

/// Read a numeric string of the BSON Decimal128 specification ("From
/// String Representation") into the decimal it writes.
///
/// The syntax is the specification's: an optional sign, then digits with
/// an optional decimal point anywhere among them (`017.`, `.5`), then an
/// optional exponent (`e` or `E`, an optional sign, digits); or, in any
/// case, `Infinity`, `Inf` or `NaN` after an optional sign. A NaN with
/// payload digits and `sNaN` are not in that syntax and are refused, and so
/// is any blank.
///
/// The value is the one written, exponent and trailing zeros as written
/// (`1.50` is 150 × 10^-2). The specification clamps an exponent outside
/// decimal128's range and rounds away digits beyond 34 when that is exact;
/// both change the coefficient and exponent written, so a string that needs
/// either is refused, as are those whose value decimal128 cannot hold at
/// all. The sign of a NaN is dropped by the specification's string
/// conversion and is kept here, so `-NaN` reads as the negative NaN.
///
/// # Errors
///
/// [`Decimal128ParseError::Syntax`] or
/// [`Decimal128ParseError::Representation`].
pub fn parse_decimal128(text: &str) -> Result<DecimalValue, Decimal128ParseError> {
    let syntax = || Decimal128ParseError::Syntax(text.to_owned());
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let value = DecimalValue::parse(text).map_err(|_| syntax())?;
    if let DecimalClass::NaN { .. } = value.class
        && !unsigned.eq_ignore_ascii_case("nan")
    {
        return Err(syntax());
    }
    if representable(&value) {
        Ok(value)
    } else {
        Err(Decimal128ParseError::Representation(text.to_owned()))
    }
}

/// The string of a decimal128 by the BSON Decimal128 specification ("To
/// String Representation").
///
/// It is `Infinity` or `-Infinity`, `NaN` for every NaN
/// (whatever its sign, signal or payload), and for a finite value its
/// coefficient's digits, with a decimal point and leading zeros when the
/// exponent is at most 0 and the adjusted exponent (the exponent plus the
/// number of digits less one) at least -6, and in exponential notation
/// (`d.dddE+n`) otherwise, with a minus sign on a negative value or zero.
#[must_use]
pub fn decimal128_string(value: &DecimalValue) -> String {
    let sign = if value.negative { "-" } else { "" };
    match &value.class {
        DecimalClass::Infinite => return format!("{sign}Infinity"),
        DecimalClass::NaN { .. } => return "NaN".to_owned(),
        DecimalClass::Finite => {}
    }
    let digits = value.digits.as_str();
    let exponent = value.exponent.to_i64().unwrap_or_default();
    let length = i64::try_from(digits.len()).unwrap_or(i64::MAX);
    let adjusted = exponent + length - 1;
    if exponent <= 0 && adjusted >= -6 {
        if exponent == 0 {
            return format!("{sign}{digits}");
        }
        let scale = usize::try_from(-exponent).unwrap_or_default();
        return if scale >= digits.len() {
            format!("{sign}0.{}{digits}", "0".repeat(scale - digits.len()))
        } else {
            let (whole, fraction) = digits.split_at(digits.len() - scale);
            format!("{sign}{whole}.{fraction}")
        };
    }
    let (first, rest) = digits.split_at(1);
    let point = if rest.is_empty() { "" } else { "." };
    let exponent_sign = if adjusted < 0 { '-' } else { '+' };
    format!(
        "{sign}{first}{point}{rest}E{exponent_sign}{}",
        adjusted.unsigned_abs()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decimal(text: &str) -> DecimalValue {
        parse_decimal128(text).unwrap_or_else(|error| panic!("{error}"))
    }

    #[test]
    fn decimal128_strings_follow_the_specification_examples() {
        for (text, string) in [
            ("1.50", "1.50"),
            ("0.001234", "0.001234"),
            ("1E+3", "1E+3"),
            ("-0", "-0"),
            ("0E-6176", "0E-6176"),
            (
                "12345678901234567890123456789012.34",
                "12345678901234567890123456789012.34",
            ),
            ("0.0000001", "1E-7"),
            ("-Inf", "-Infinity"),
            ("nan", "NaN"),
        ] {
            assert_eq!(decimal128_string(&decimal(text)), string, "{text}");
        }
    }

    #[test]
    fn bid_encoding_round_trips_every_class() {
        for text in ["0", "-0", "1.50", "-1E+6111", "1E-6176", "Infinity", "-NaN"] {
            let value = decimal(text);
            let bytes = to_bid(&value).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(from_bid(bytes), Some(value), "{text}");
        }
        let signalling = DecimalValue::nan(true, true, "12");
        let bytes = to_bid(&signalling).unwrap_or_else(|| panic!("sNaN12"));
        assert_eq!(from_bid(bytes), Some(signalling));
    }

    #[test]
    fn strings_decimal128_does_not_hold_as_written_are_refused() {
        for text in [
            "1E+6112",
            "1E-6177",
            "1.000000000000000000000000000000000000",
            "sNaN",
            "NaN1",
            "1e",
            ".",
            " 1",
        ] {
            assert!(parse_decimal128(text).is_err(), "{text}");
        }
    }
}
