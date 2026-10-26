//! Integers of any size: [`WideInt`], and [`BigIntValue`] for the integers
//! outside the range of `i64`.

use std::cmp::Ordering;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{
    ExactKind, ExactParseError, FidelityReason, cmp_digit_strings, put_str, strip_leading_zeros,
};

/// An integer of any magnitude, held as a sign and the decimal digits of
/// its magnitude, with no fixed width.
///
/// It is the type of a [`DecimalValue`](super::DecimalValue)'s exponent and
/// of a [`BigIntValue`]'s count of trailing zeros, so a numeral such as
/// `1e100000000000000000000` is held in space proportional to its text.
/// Each integer has one representation: the magnitude has no leading zero
/// and zero is never negative. `Ord` is the numeric order.
///
/// Serialized as its decimal text (`"-3000000001"`), which deserializing
/// checks for that one representation.
///
/// ```
/// use panproto_expr::exact::WideInt;
///
/// let big = WideInt::from(i64::MAX).plus(&WideInt::from(1_i64));
/// assert_eq!(big.to_string(), "9223372036854775808");
/// assert_eq!(big.to_i64(), None);
/// assert_eq!(big.minus(&WideInt::from(1_i64)).to_i64(), Some(i64::MAX));
/// assert!(WideInt::from(-5_i64) < WideInt::from(3_i64));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WideInt {
    negative: bool,
    magnitude: String,
}

impl Default for WideInt {
    fn default() -> Self {
        Self::zero()
    }
}

/// `a + b` for two canonical digit strings.
fn add_magnitudes(a: &str, b: &str) -> String {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0_u8;
    for i in 0..a.len().max(b.len()) {
        let digit = |s: &[u8]| s.len().checked_sub(i + 1).map_or(0, |at| s[at] - b'0');
        let sum = digit(a) + digit(b) + carry;
        out.push(b'0' + sum % 10);
        carry = sum / 10;
    }
    if carry > 0 {
        out.push(b'0' + carry);
    }
    out.reverse();
    let text: String = out.into_iter().map(char::from).collect();
    strip_leading_zeros(&text).to_owned()
}

/// `a - b` for two canonical digit strings with `a >= b`.
fn sub_magnitudes(a: &str, b: &str) -> String {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0_u8;
    for i in 0..a.len() {
        let top = a[a.len() - 1 - i] - b'0';
        let bottom = b.len().checked_sub(i + 1).map_or(0, |at| b[at] - b'0') + borrow;
        let (digit, next) = if top >= bottom {
            (top - bottom, 0)
        } else {
            (top + 10 - bottom, 1)
        };
        out.push(b'0' + digit);
        borrow = next;
    }
    out.reverse();
    let text: String = out.into_iter().map(char::from).collect();
    strip_leading_zeros(&text).to_owned()
}

impl WideInt {
    /// Zero.
    #[must_use]
    pub fn zero() -> Self {
        Self {
            negative: false,
            magnitude: "0".to_owned(),
        }
    }

    /// The integer `(-1)^negative × digits`, leading zeros dropped and a
    /// negative zero read as zero, or `None` when `digits` is empty or
    /// holds anything but ASCII digits.
    #[must_use]
    pub fn from_sign_and_digits(negative: bool, digits: &str) -> Option<Self> {
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let magnitude = strip_leading_zeros(digits).to_owned();
        Some(Self {
            negative: negative && magnitude != "0",
            magnitude,
        })
    }

    /// The integer `text` writes in its one representation (`-` and the
    /// digits of the magnitude, no leading zero, no `-0`), or `None` for
    /// any other text.
    #[must_use]
    pub fn parse_canonical(text: &str) -> Option<Self> {
        let (negative, digits) = text
            .strip_prefix('-')
            .map_or((false, text), |rest| (true, rest));
        let value = Self::from_sign_and_digits(negative, digits)?;
        (value.to_string() == text).then_some(value)
    }

    /// Whether the integer is negative.
    #[must_use]
    pub const fn is_negative(&self) -> bool {
        self.negative
    }

    /// Whether the integer is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.magnitude == "0"
    }

    /// The decimal digits of the magnitude, with no leading zero.
    #[must_use]
    pub fn magnitude(&self) -> &str {
        &self.magnitude
    }

    /// The integer as an `i64`, when it fits.
    #[must_use]
    pub fn to_i64(&self) -> Option<i64> {
        if self.magnitude.len() > 19 {
            return None;
        }
        let magnitude = self.magnitude.parse::<i128>().ok()?;
        i64::try_from(if self.negative { -magnitude } else { magnitude }).ok()
    }

    /// The negated integer.
    #[must_use]
    pub fn negated(&self) -> Self {
        Self {
            negative: !self.negative && !self.is_zero(),
            magnitude: self.magnitude.clone(),
        }
    }

    /// The magnitude, as a non-negative integer.
    #[must_use]
    pub fn abs(&self) -> Self {
        Self {
            negative: false,
            magnitude: self.magnitude.clone(),
        }
    }

    /// The sum `self + other`, exact.
    #[must_use]
    pub fn plus(&self, other: &Self) -> Self {
        if self.negative == other.negative {
            let magnitude = add_magnitudes(&self.magnitude, &other.magnitude);
            return Self {
                negative: self.negative && magnitude != "0",
                magnitude,
            };
        }
        // Opposite signs: the larger magnitude keeps its sign.
        match cmp_digit_strings(&self.magnitude, &other.magnitude) {
            Ordering::Equal => Self::zero(),
            Ordering::Greater => Self {
                negative: self.negative,
                magnitude: sub_magnitudes(&self.magnitude, &other.magnitude),
            },
            Ordering::Less => Self {
                negative: other.negative,
                magnitude: sub_magnitudes(&other.magnitude, &self.magnitude),
            },
        }
    }

    /// The difference `self - other`, exact.
    #[must_use]
    pub fn minus(&self, other: &Self) -> Self {
        self.plus(&other.negated())
    }

    /// Append the canonical bytes: the sign as one byte (`0` non-negative,
    /// `1` negative), then the magnitude's digits behind a little-endian
    /// `u64` length.
    pub fn write_canonical(&self, out: &mut Vec<u8>) {
        out.push(u8::from(self.negative));
        put_str(out, &self.magnitude);
    }
}

impl Ord for WideInt {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => cmp_digit_strings(&self.magnitude, &other.magnitude),
            (true, true) => cmp_digit_strings(&other.magnitude, &self.magnitude),
        }
    }
}

impl PartialOrd for WideInt {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl From<i64> for WideInt {
    fn from(value: i64) -> Self {
        Self {
            negative: value < 0,
            magnitude: value.unsigned_abs().to_string(),
        }
    }
}

impl From<i32> for WideInt {
    fn from(value: i32) -> Self {
        Self::from(i64::from(value))
    }
}

impl From<u32> for WideInt {
    fn from(value: u32) -> Self {
        Self::from(u64::from(value))
    }
}

impl From<u64> for WideInt {
    fn from(value: u64) -> Self {
        Self {
            negative: false,
            magnitude: value.to_string(),
        }
    }
}

impl From<usize> for WideInt {
    fn from(value: usize) -> Self {
        Self {
            negative: false,
            magnitude: value.to_string(),
        }
    }
}

impl From<i128> for WideInt {
    fn from(value: i128) -> Self {
        Self {
            negative: value < 0,
            magnitude: value.unsigned_abs().to_string(),
        }
    }
}

impl std::fmt::Display for WideInt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.negative {
            f.write_str("-")?;
        }
        f.write_str(&self.magnitude)
    }
}

impl Serialize for WideInt {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for WideInt {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse_canonical(&text).ok_or_else(|| {
            serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(&text),
                &"an integer in decimal digits with an optional `-`, no leading zero and no `-0`",
            )
        })
    }
}

/// An integer outside the range of `i64`:
/// `(-1)^negative × significand × 10^zeros`.
///
/// `significand` is a non-empty string of ASCII digits with no leading zero
/// whose last digit is not `0`, and `zeros` is the count of trailing zeros,
/// a non-negative [`WideInt`] with no fixed width. So `1e4097` and the
/// integer 1 followed by 4097 zeros written out are one value (significand
/// `"1"`, `zeros` 4097), and `1e100000000000000000000` is held in space
/// proportional to its lexeme. The type holds only integers that do not fit
/// `i64`: every constructor returns an [`ExactInteger`], which is
/// [`ExactInteger::Int`] for an integer that fits, so each integer has
/// exactly one representation. Deserializing a `BigIntValue` that breaks
/// these rules fails for the same reason.
///
/// [`Display`](std::fmt::Display) writes every digit of the integer, so
/// its text grows with `zeros`; [`digit_count`](Self::digit_count) gives
/// that length beforehand and [`exponent_form`](Self::exponent_form) a text
/// whose length grows only with the digits of `zeros`.
///
/// ```
/// use panproto_expr::exact::{BigIntValue, ExactInteger, WideInt};
///
/// assert_eq!(BigIntValue::parse("-42")?, ExactInteger::Int(-42));
/// let ExactInteger::Big(big) = BigIntValue::parse("18446744073709551615")? else {
///     unreachable!("u64::MAX does not fit i64")
/// };
/// assert_eq!(big.to_string(), "18446744073709551615");
/// let ExactInteger::Big(power) = BigIntValue::parse("100000000000000000000")? else {
///     unreachable!("10^20 does not fit i64")
/// };
/// assert_eq!((power.significand.as_str(), &power.zeros), ("1", &WideInt::from(20_i64)));
/// assert_eq!(power.exponent_form(), "1e20");
/// # Ok::<(), panproto_expr::ExactParseError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "BigIntParts")]
pub struct BigIntValue {
    /// Whether the integer is negative.
    pub negative: bool,
    /// The digits of the magnitude without its leading and trailing zeros.
    pub significand: String,
    /// The number of trailing zeros of the magnitude, never negative.
    pub zeros: WideInt,
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
#[serde(deny_unknown_fields)]
struct BigIntParts {
    negative: bool,
    significand: String,
    zeros: WideInt,
}

impl TryFrom<BigIntParts> for BigIntValue {
    type Error = ExactParseError;

    fn try_from(parts: BigIntParts) -> Result<Self, Self::Error> {
        let value = Self {
            negative: parts.negative,
            significand: parts.significand,
            zeros: parts.zeros,
        };
        if value.is_well_formed() {
            Ok(value)
        } else {
            Err(not_an_integer())
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
    /// The number `(-1)^negative × digits × 10^exponent` in its one
    /// representation when it is an integer; `None` when it is not, or when
    /// `digits` is empty or holds anything but ASCII digits. Leading and
    /// trailing zeros of `digits` are allowed; no digit beyond `digits` is
    /// written, whatever `exponent` is.
    fn normalize(negative: bool, digits: &str, exponent: &WideInt) -> Option<ExactInteger> {
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let digits = strip_leading_zeros(digits);
        if digits == "0" {
            return Some(ExactInteger::Int(0));
        }
        let significand = digits.trim_end_matches('0');
        let zeros = exponent.plus(&WideInt::from(digits.len() - significand.len()));
        if zeros.is_negative() {
            return None;
        }
        // An integer of at most nineteen digits may fit `i64`; one with
        // more never does.
        let small = zeros
            .to_i64()
            .and_then(|z| usize::try_from(z).ok())
            .filter(|&z| significand.len() + z <= 19);
        if let Some(z) = small {
            let written = format!("{significand}{}", "0".repeat(z));
            if let Some(n) = fit_i64(negative, &written) {
                return Some(ExactInteger::Int(n));
            }
        }
        Some(ExactInteger::Big(Self {
            negative,
            significand: significand.to_owned(),
            zeros,
        }))
    }

    /// The integer `(-1)^negative × digits` in its one representation.
    ///
    /// Leading zeros of `digits` are dropped, its trailing zeros are
    /// counted into [`zeros`](Self::zeros), and a negative zero is zero.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::NotANumber`] when `digits` is empty or holds
    /// anything but ASCII digits.
    pub fn from_parts(negative: bool, digits: &str) -> Result<ExactInteger, ExactParseError> {
        Self::normalize(negative, digits, &WideInt::zero()).ok_or_else(not_an_integer)
    }

    /// The integer `(-1)^negative × digits × 10^exponent` in its one
    /// representation, when that number is an integer: the shape of a
    /// [`DecimalValue`](super::DecimalValue). `None` when it is not an
    /// integer, or when `digits` is empty or holds anything but ASCII
    /// digits. No digit beyond `digits` is written, whatever `exponent` is.
    #[must_use]
    pub fn from_scaled(negative: bool, digits: &str, exponent: &WideInt) -> Option<ExactInteger> {
        Self::normalize(negative, digits, exponent)
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
        Self::normalize(
            value < 0,
            &value.unsigned_abs().to_string(),
            &WideInt::zero(),
        )
        .unwrap_or(ExactInteger::Int(0))
    }

    /// The number of digits of the magnitude written out: the
    /// significand's length plus [`zeros`](Self::zeros).
    #[must_use]
    pub fn digit_count(&self) -> WideInt {
        self.zeros.plus(&WideInt::from(self.significand.len()))
    }

    /// The nearest `f64`, an infinity of the integer's sign when it is
    /// beyond binary64's range. Lossy in general.
    #[must_use]
    pub fn to_f64(&self) -> f64 {
        let sign = if self.negative { "-" } else { "" };
        // `0.<significand>e<digit count>` keeps the written exponent at the
        // position of the leading digit, which Rust's parser reads exactly
        // whatever its size (an exponent beyond the range only saturates).
        format!("{sign}0.{}e{}", self.significand, self.digit_count())
            .parse::<f64>()
            .unwrap_or(if self.negative {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            })
    }

    /// Compare two integers as numbers.
    #[must_use]
    pub fn cmp_numeric(&self, other: &Self) -> Ordering {
        let magnitude = || {
            self.digit_count()
                .cmp(&other.digit_count())
                .then_with(|| cmp_significands(&self.significand, &other.significand))
        };
        match (self.negative, other.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => magnitude(),
            (true, true) => magnitude().reverse(),
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

    /// Whether the fields keep the type's invariants: a significand with no
    /// leading or trailing zero, a non-negative count, and a value outside
    /// `i64`.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        matches!(
            Self::normalize(self.negative, &self.significand, &self.zeros),
            Some(ExactInteger::Big(ref big)) if big == self
        )
    }

    /// The canonical payload bytes: the sign as one byte (`0` or `1`), the
    /// significand behind a little-endian `u64` length, then the decimal
    /// digits of [`zeros`](Self::zeros), with no leading zero (`"0"` when
    /// there is no trailing zero), behind a little-endian `u64` length.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out =
            Vec::with_capacity(18 + self.significand.len() + self.zeros.magnitude().len());
        out.push(u8::from(self.negative));
        put_str(&mut out, &self.significand);
        put_str(&mut out, self.zeros.magnitude());
        out
    }

    /// The integer in exponent form, `<sign><significand>e<zeros>`
    /// (`-12e30`), which [`DecimalValue::parse`](super::DecimalValue::parse)
    /// and every exact integer reading of a numeral read back as this
    /// integer; no digit beyond the significand is written.
    #[must_use]
    pub fn exponent_form(&self) -> String {
        let sign = if self.negative { "-" } else { "" };
        format!("{sign}{}e{}", self.significand, self.zeros)
    }
}

/// Sixty-four zeros, the piece in which `Display` writes trailing zeros.
const CHUNK: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Compare two significands of numbers with equal digit counts, digit by
/// digit, the shorter padded with zeros.
fn cmp_significands(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    (0..a.len().max(b.len()))
        .map(|i| {
            let digit = |s: &[u8]| s.get(i).copied().unwrap_or(b'0');
            digit(a).cmp(&digit(b))
        })
        .find(|o| o.is_ne())
        .unwrap_or(Ordering::Equal)
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
    /// Every digit of the integer, written out: the significand followed by
    /// [`zeros`](BigIntValue::zeros) zeros. The text is as long as
    /// [`digit_count`](BigIntValue::digit_count) (plus a sign).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.negative {
            f.write_str("-")?;
        }
        f.write_str(&self.significand)?;
        let mut left = self.zeros.clone();
        let chunk = WideInt::from(CHUNK.len());
        while left >= chunk {
            f.write_str(CHUNK)?;
            left = left.minus(&chunk);
        }
        let rest = left
            .to_i64()
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(0);
        f.write_str(&CHUNK[..rest])
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
