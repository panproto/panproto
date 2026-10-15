//! Exact decimal numbers, and the closest-shortest decimal of a binary
//! float.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::{
    CANONICAL_NAN, ExactKind, ExactParseError, FidelityReason, NEGATIVE_CANONICAL_NAN, WideInt,
    cmp_digit_strings, is_canonical_digits, put_str, strip_leading_zeros,
};

/// An exact decimal number: `(-1)^negative × digits × 10^exponent`, or one
/// of the special values of the General Decimal Arithmetic: an infinity or
/// a NaN.
///
/// A finite value ([`DecimalClass::Finite`]) has `digits`, a non-empty
/// string of ASCII digits with no leading zero (except the single digit
/// `"0"`), and `exponent`, an integer of any size ([`WideInt`]), so every
/// decimal numeral is held, whatever its exponent: `0.0e-3000000000` is the
/// zero of scale 3000000001 and `1e3000000000` is 1 × 10^3000000000. The
/// representation keeps the scale the number was written with, so `1.50`
/// (`digits` `"150"`, `exponent` `-2`) and `1.5` (`"15"`, `-1`) are
/// different values under `==`; [`cmp_numeric`](Self::cmp_numeric)
/// compares them as numbers. A zero keeps a written minus sign: `-0` and
/// `-0E+3` are negative zeros, distinct from `0` and `0E+3` and from each
/// other, and every zero compares `Equal` to every other under
/// [`cmp_numeric`](Self::cmp_numeric).
///
/// An infinity ([`DecimalClass::Infinite`]) has its sign; a NaN
/// ([`DecimalClass::NaN`]) has its sign, whether it is signalling, and its
/// diagnostic payload, so `NaN`, `-NaN`, `sNaN` and `NaN12` are four values
/// and `NaN` and `NaN0` are one. A special value has `digits` `"0"` and
/// `exponent` zero.
///
/// This is the abstract representation of the General Decimal Arithmetic
/// specification (Cowlishaw 2009, "Abstract representation" and "Numeric
/// string syntax"), of XSD's precisionDecimal (Peterson and Sperberg-McQueen
/// 2011, Secs 3.1 to 3.2) and of Java's `BigDecimal` (Oracle 2023, Java SE
/// 21 API) for its finite values: a coefficient (`digits`) and an exponent,
/// so a number has one representation per scale. Representation equality
/// (`==`) is GDA's compare-total identity and `BigDecimal.equals` (`2.0`
/// differs from `2.00`); [`cmp_numeric`](Self::cmp_numeric) is GDA's
/// compare on finite values, precisionDecimal's equality and order, and
/// `BigDecimal.compareTo` (`2.0` equals `2.00`), and is also the equality
/// of XSD's `xs:decimal`, whose value space does not keep the scale (W3C
/// 2012, XML Schema 1.1 Part 2: Datatypes, Sec 3.3.3, stated);
/// [`compare_total`](Self::compare_total) is GDA's compare-total.
/// [`scale`](Self::scale) is the number of fractional digits written,
/// clamped at zero, while `exponent` keeps a positive power of ten (`1e3`
/// has exponent 3, where `BigDecimal` would record the scale -3). Unlike
/// GDA's and `BigDecimal`'s, whose exponents have bounds, the exponent here
/// has none.
///
/// ```
/// use std::cmp::Ordering;
///
/// use panproto_expr::exact::{DecimalClass, DecimalValue, WideInt};
///
/// let a = DecimalValue::parse("1.50")?;
/// let b = DecimalValue::parse("1.5")?;
/// assert_ne!(a, b);
/// assert_eq!(a.cmp_numeric(&b), Ordering::Equal);
/// assert_eq!((a.scale(), b.scale()), (WideInt::from(2_i64), WideInt::from(1_i64)));
/// assert_eq!(a.to_string(), "1.50");
/// let far = DecimalValue::parse("0.0e-3000000000")?;
/// assert_eq!(far.scale().to_string(), "3000000001");
/// let negative_zero = DecimalValue::parse("-0")?;
/// assert!(negative_zero.negative && negative_zero.is_zero());
/// assert_eq!(negative_zero.cmp_numeric(&DecimalValue::parse("0")?), Ordering::Equal);
/// let nan = DecimalValue::parse("sNaN12")?;
/// assert_eq!(nan.class, DecimalClass::NaN { signalling: true, payload: "12".into() });
/// assert_eq!(nan.to_string(), "sNaN12");
/// # Ok::<(), panproto_expr::ExactParseError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "DecimalParts")]
pub struct DecimalValue {
    /// Whether the value is negative: a negative number, a negative zero, a
    /// negative infinity or a NaN with its sign set.
    pub negative: bool,
    /// The significant digits, without leading zeros; `"0"` for a special
    /// value.
    pub digits: String,
    /// The power of ten the digits are scaled by; zero for a special value.
    pub exponent: WideInt,
    /// Whether the value is finite, an infinity or a NaN.
    #[serde(default, skip_serializing_if = "DecimalClass::is_finite")]
    pub class: DecimalClass,
}

/// Which of the General Decimal Arithmetic's classes a [`DecimalValue`]
/// is in.
///
/// Serialized as `"finite"`, `"infinite"` or `{"na_n": {"signalling":
/// bool, "payload": digits}}`; a finite value omits the field.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecimalClass {
    /// A finite number.
    #[default]
    Finite,
    /// An infinity, of the value's sign.
    Infinite,
    /// A NaN, of the value's sign.
    NaN {
        /// Whether the NaN is signalling (`sNaN`).
        signalling: bool,
        /// The diagnostic payload, as canonical digits (no leading zero;
        /// `"0"` when none was written, so `NaN` and `NaN0` are one value).
        payload: String,
    },
}

impl DecimalClass {
    /// Whether this is [`DecimalClass::Finite`].
    #[must_use]
    pub const fn is_finite(&self) -> bool {
        matches!(self, Self::Finite)
    }
}

/// The unchecked fields of a [`DecimalValue`], as they are deserialized.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecimalParts {
    negative: bool,
    digits: String,
    exponent: WideInt,
    #[serde(default)]
    class: DecimalClass,
}

impl TryFrom<DecimalParts> for DecimalValue {
    type Error = ExactParseError;

    fn try_from(parts: DecimalParts) -> Result<Self, Self::Error> {
        match parts.class {
            DecimalClass::Finite => Self::new(parts.negative, parts.digits, parts.exponent),
            DecimalClass::Infinite if parts.digits == "0" && parts.exponent.is_zero() => {
                Ok(Self::infinity(parts.negative))
            }
            DecimalClass::NaN {
                signalling,
                payload,
            } if parts.digits == "0"
                && parts.exponent.is_zero()
                && is_canonical_digits(&payload) =>
            {
                Ok(Self::nan(parts.negative, signalling, payload))
            }
            DecimalClass::Infinite | DecimalClass::NaN { .. } => Err(not_a_number()),
        }
    }
}

const fn not_a_number() -> ExactParseError {
    ExactParseError::new(ExactKind::Decimal, FidelityReason::NotANumber)
}

/// A number of fractional digits at which Rust's `{:.p$e}` formatting of
/// the finite binary float `significand × 2^exponent` (`significand` of at
/// most `significand_digits` decimal digits) writes the float's exact
/// decimal expansion, padded with zeros: Rust's formatting with an
/// explicit precision writes correctly rounded digits, so at any precision
/// at least the expansion's count of significant digits it writes the
/// expansion itself.
///
/// The count is bounded without computing it: for `exponent >= 0` the
/// expansion is the integer `significand × 2^exponent`, of at most
/// `significand_digits + floor(exponent × log10 2) + 1` digits; for
/// `exponent = -k < 0` it is `significand × 5^k × 10^-k`, whose significant
/// digits are those of `significand × 5^k`, at most `significand_digits +
/// floor(k × log10 5) + 1`. Since `0.302 > log10 2` and `0.699 > log10 5`,
/// `exponent × 302 / 1000` and `k × 699 / 1000` (rounded down) are at least
/// the two floors.
fn exact_precision(significand_digits: usize, exponent: i32) -> usize {
    let factor = if exponent >= 0 { 302 } else { 699 };
    let magnitude = u64::from(exponent.unsigned_abs());
    let scaled = usize::try_from(magnitude * factor / 1000).unwrap_or(usize::MAX);
    significand_digits.saturating_add(scaled).saturating_add(1)
}

/// A precision at which scientific formatting writes `x`'s exact binary64
/// expansion, including its last non-zero decimal digit.
fn exact_f64_precision(x: f64) -> usize {
    let biased = i32::try_from((x.to_bits() >> 52) & 0x7ff).unwrap_or(0);
    exact_precision(16, biased.max(1) - 1075)
}

/// A precision at which scientific formatting writes `x`'s exact binary32
/// expansion, including its last non-zero decimal digit.
fn exact_f32_precision(x: f32) -> usize {
    let biased = i32::try_from((x.to_bits() >> 23) & 0xff).unwrap_or(0);
    exact_precision(8, biased.max(1) - 150)
}

impl DecimalValue {
    /// The finite decimal `(-1)^negative × digits × 10^exponent`.
    ///
    /// A zero keeps its sign: `new(true, "0", 0)` is the negative zero.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::NotANumber`] when `digits` is empty, holds
    /// anything but ASCII digits, or has a leading zero.
    pub fn new(
        negative: bool,
        digits: impl Into<String>,
        exponent: impl Into<WideInt>,
    ) -> Result<Self, ExactParseError> {
        let digits = digits.into();
        if !is_canonical_digits(&digits) {
            return Err(not_a_number());
        }
        Ok(Self {
            negative,
            digits,
            exponent: exponent.into(),
            class: DecimalClass::Finite,
        })
    }

    /// The infinity of the sign `negative`.
    #[must_use]
    pub fn infinity(negative: bool) -> Self {
        Self {
            negative,
            digits: "0".to_owned(),
            exponent: WideInt::zero(),
            class: DecimalClass::Infinite,
        }
    }

    /// The NaN of the sign `negative`, signalling or quiet, with the
    /// diagnostic `payload` (canonical digits, `"0"` for none).
    #[must_use]
    pub fn nan(negative: bool, signalling: bool, payload: impl Into<String>) -> Self {
        Self {
            negative,
            digits: "0".to_owned(),
            exponent: WideInt::zero(),
            class: DecimalClass::NaN {
                signalling,
                payload: payload.into(),
            },
        }
    }

    /// Whether the value is finite (a number, possibly a zero of either
    /// sign).
    #[must_use]
    pub const fn is_finite(&self) -> bool {
        self.class.is_finite()
    }

    /// Read a decimal lexeme.
    ///
    /// Accepts the JSON number grammar and the decimal forms of SQL and XML
    /// Schema: an optional sign, digits with an optional fractional part
    /// (either side of the point may be empty, not both), and an optional
    /// exponent `e`/`E` with its own optional sign and any number of
    /// digits. Leading zeros are dropped; trailing fractional zeros are
    /// kept as scale, and a minus sign on a zero is kept (`-0.00` is a
    /// negative zero of scale 2). The stored exponent (the written exponent,
    /// 0 when absent, less the number of fractional digits) is computed
    /// exactly, whatever its size.
    ///
    /// Beside the finite forms it reads the special values of the General
    /// Decimal Arithmetic's numeric string syntax (Cowlishaw 2009, "Numeric
    /// string syntax", stated): `Infinity` or `Inf`, and `NaN` or `sNaN`
    /// followed by optional payload digits, in any case and with an optional
    /// sign. A NaN without digits has payload zero ("A plain 'NaN' is
    /// usually the same as `NaN0`"), so `NaN` and `NaN0` read as one value;
    /// leading zeros of a payload are dropped.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::NotANumber`] for anything that is not a decimal
    /// lexeme.
    pub fn parse(text: &str) -> Result<Self, ExactParseError> {
        let (negative, rest) = match text.as_bytes().first() {
            Some(b'-') => (true, &text[1..]),
            Some(b'+') => (false, &text[1..]),
            _ => (false, text),
        };
        if let Some(special) = Self::parse_special(negative, rest) {
            return Ok(special);
        }
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
        let written = match exponent_text {
            None => WideInt::zero(),
            Some(e) => {
                let (negative, magnitude) = match e.as_bytes().first() {
                    Some(b'-') => (true, &e[1..]),
                    Some(b'+') => (false, &e[1..]),
                    _ => (false, e),
                };
                WideInt::from_sign_and_digits(negative, magnitude).ok_or_else(not_a_number)?
            }
        };
        let exponent = written.minus(&WideInt::from(frac_part.len()));
        let mut digits = String::with_capacity(int_part.len() + frac_part.len());
        digits.push_str(int_part);
        digits.push_str(frac_part);
        let digits = strip_leading_zeros(&digits).to_owned();
        Self::new(negative, digits, exponent)
    }

    /// The special value `unsigned` spells (sign already removed), if it
    /// spells one: `inf`, `infinity`, or `nan` or `snan` followed by
    /// payload digits, in any case.
    fn parse_special(negative: bool, unsigned: &str) -> Option<Self> {
        if unsigned.eq_ignore_ascii_case("inf") || unsigned.eq_ignore_ascii_case("infinity") {
            return Some(Self::infinity(negative));
        }
        let lower = unsigned.to_ascii_lowercase();
        let (signalling, payload) = match lower.strip_prefix("snan") {
            Some(payload) => (true, payload),
            None => (false, lower.strip_prefix("nan")?),
        };
        if !payload.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(Self::nan(
            negative,
            signalling,
            strip_leading_zeros(payload).to_owned(),
        ))
    }

    /// The decimal equal to `value`.
    #[must_use]
    pub fn from_i64(value: i64) -> Self {
        Self {
            negative: value < 0,
            digits: value.unsigned_abs().to_string(),
            exponent: WideInt::zero(),
            class: DecimalClass::Finite,
        }
    }

    /// The closest-shortest decimal of `x` at binary64, or `None` when `x`
    /// is not finite.
    ///
    /// Among the decimals with the fewest significant digits (the minimal
    /// significand) that read back to exactly `x` under IEEE 754
    /// round-to-nearest-even at binary64, it is the one closest to `x`, and
    /// at an exact tie, of the two candidates equally close to `x`, the one
    /// whose significand (its last significant digit) is even: ties to even,
    /// as ECMA-262's `Number::toString` recommends (Ecma International TC39,
    /// ECMAScript Language Specification, Sec 6.1.6.1.20, Note 2, stated)
    /// and as this function follows. Rust's float formatting, which takes
    /// the candidate of larger magnitude at such a tie, is not used for the
    /// choice. So `2^50 +
    /// 0.25` gives `1125899906842624.2`, while `1125899906842624.3`, the
    /// other text of the tie, also reads back to it but is not returned;
    /// `0.1` gives `1 × 10^-1`, not the float's exact binary expansion; and
    /// a negative zero gives zero. Every route that writes a binary64 as
    /// decimal text writes this decimal.
    ///
    /// ```
    /// use panproto_expr::exact::DecimalValue;
    ///
    /// let tie = f64::from_bits(0x4310_0000_0000_0001); // 2^50 + 1/4
    /// let text = DecimalValue::from_f64_shortest(tie).map(|d| d.to_string());
    /// assert_eq!(text.as_deref(), Some("1125899906842624.2"));
    /// assert_eq!("1125899906842624.3".parse::<f64>(), Ok(tie));
    /// ```
    #[must_use]
    pub fn from_f64_shortest(x: f64) -> Option<Self> {
        if !x.is_finite() {
            return None;
        }
        let magnitude = x.abs();
        // `magnitude` is `fraction × 2^-1074` for a subnormal and
        // `(2^52 + fraction) × 2^(biased - 1075)` otherwise, a significand
        // below 2^53 < 10^16.
        let precision = exact_f64_precision(x);
        Some(closest_shortest(
            x.is_sign_negative(),
            &format!("{magnitude:.precision$e}"),
            shortest_len(&format!("{magnitude:e}")),
            |text| {
                text.parse::<f64>()
                    .is_ok_and(|y| y.to_bits() == magnitude.to_bits())
            },
        ))
    }

    /// The closest-shortest decimal of `x` at binary32, or `None` when `x`
    /// is not finite: the rule of
    /// [`from_f64_shortest`](Self::from_f64_shortest), ties to even, with
    /// reading back at binary32. So the binary32 value `52401.3125` gives
    /// `52401.312`, not `52401.313`.
    ///
    /// ```
    /// use panproto_expr::exact::DecimalValue;
    ///
    /// let tie = 838_421.0_f32 / 16.0; // 52401.3125
    /// let text = DecimalValue::from_f32_shortest(tie).map(|d| d.to_string());
    /// assert_eq!(text.as_deref(), Some("52401.312"));
    /// ```
    #[must_use]
    pub fn from_f32_shortest(x: f32) -> Option<Self> {
        if !x.is_finite() {
            return None;
        }
        let magnitude = x.abs();
        // `magnitude` is `fraction × 2^-149` for a subnormal and
        // `(2^23 + fraction) × 2^(biased - 150)` otherwise, a significand
        // below 2^24 < 10^8.
        let precision = exact_f32_precision(x);
        Some(closest_shortest(
            x.is_sign_negative(),
            &format!("{magnitude:.precision$e}"),
            shortest_len(&format!("{magnitude:e}")),
            |text| {
                text.parse::<f32>()
                    .is_ok_and(|y| y.to_bits() == magnitude.to_bits())
            },
        ))
    }

    /// The text `0.<digits>e<lead>`, with `lead` the decimal exponent of the
    /// place just above the leading digit, which Rust's float parsers read
    /// as this number whatever the size of the exponent: a written exponent
    /// beyond their range only saturates, and here it is beyond that range
    /// only when the number is.
    fn leading_form(&self) -> String {
        let sign = if self.negative { "-" } else { "" };
        let lead = self.exponent.plus(&WideInt::from(self.digits.len()));
        format!("{sign}0.{}e{lead}", self.digits)
    }

    /// The nearest `f64`, which may be infinite or zero when the decimal is
    /// outside the range of `f64`; lossy in general. A zero keeps its sign
    /// (`-0` gives `-0.0`), an infinity gives the infinity of its sign, and
    /// a NaN the quiet NaN of its sign ([`CANONICAL_NAN`] or
    /// [`NEGATIVE_CANONICAL_NAN`]), carrying neither its signalling flag
    /// nor its payload.
    #[must_use]
    pub fn to_f64(&self) -> f64 {
        match self.class {
            DecimalClass::Finite => self.leading_form().parse::<f64>().unwrap_or(CANONICAL_NAN),
            DecimalClass::Infinite if self.negative => f64::NEG_INFINITY,
            DecimalClass::Infinite => f64::INFINITY,
            DecimalClass::NaN { .. } if self.negative => NEGATIVE_CANONICAL_NAN,
            DecimalClass::NaN { .. } => CANONICAL_NAN,
        }
    }

    /// The nearest `f32`, which may be infinite or zero when the decimal is
    /// outside the range of `f32`; lossy in general. The special values map
    /// as under [`to_f64`](Self::to_f64), a NaN to the quiet binary32 NaN of
    /// its sign (bits `0x7fc00000` or `0xffc00000`).
    #[must_use]
    pub fn to_f32(&self) -> f32 {
        match self.class {
            DecimalClass::Finite => self
                .leading_form()
                .parse::<f32>()
                .unwrap_or(f32::from_bits(0x7fc0_0000)),
            DecimalClass::Infinite if self.negative => f32::NEG_INFINITY,
            DecimalClass::Infinite => f32::INFINITY,
            DecimalClass::NaN { .. } if self.negative => f32::from_bits(0xffc0_0000),
            DecimalClass::NaN { .. } => f32::from_bits(0x7fc0_0000),
        }
    }

    /// Whether the decimal is a zero, of either sign and at any scale; a
    /// special value is not.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.class.is_finite() && self.digits == "0"
    }

    /// The number of digits written after the decimal point:
    /// `max(0, -exponent)`; zero for a special value.
    #[must_use]
    pub fn scale(&self) -> WideInt {
        if self.exponent.is_negative() {
            self.exponent.abs()
        } else {
            WideInt::zero()
        }
    }

    /// The smallest scale that writes the same number: the scale with every
    /// trailing fractional zero dropped (zero for a zero and for a special
    /// value).
    #[must_use]
    pub fn minimal_scale(&self) -> WideInt {
        if self.digits == "0" {
            return WideInt::zero();
        }
        let trailing = self.digits.len() - self.digits.trim_end_matches('0').len();
        let minimal = self.scale().minus(&WideInt::from(trailing));
        if minimal.is_negative() {
            WideInt::zero()
        } else {
            minimal
        }
    }

    /// The number of digits before the decimal point once leading zeros are
    /// dropped (zero for a number whose magnitude is below one, and for a
    /// special value).
    #[must_use]
    pub fn integer_digits(&self) -> WideInt {
        if self.digits == "0" {
            return WideInt::zero();
        }
        let position = self.exponent.plus(&WideInt::from(self.digits.len()));
        if position.is_negative() {
            WideInt::zero()
        } else {
            position
        }
    }

    /// The number of digits the number needs in positional notation: its
    /// [`integer_digits`](Self::integer_digits) plus its
    /// [`minimal_scale`](Self::minimal_scale).
    #[must_use]
    pub fn significant_digits(&self) -> WideInt {
        self.minimal_scale().plus(&self.integer_digits())
    }

    /// The rank of the value's class in the numeric preorder: negative quiet
    /// NaNs, negative signalling NaNs, minus infinity, the finite values,
    /// plus infinity, positive signalling NaNs, positive quiet NaNs.
    const fn class_rank(&self) -> u8 {
        match (&self.class, self.negative) {
            (
                DecimalClass::NaN {
                    signalling: false, ..
                },
                true,
            ) => 0,
            (
                DecimalClass::NaN {
                    signalling: true, ..
                },
                true,
            ) => 1,
            (DecimalClass::Infinite, true) => 2,
            (DecimalClass::Finite, _) => 3,
            (DecimalClass::Infinite, false) => 4,
            (
                DecimalClass::NaN {
                    signalling: true, ..
                },
                false,
            ) => 5,
            (
                DecimalClass::NaN {
                    signalling: false, ..
                },
                false,
            ) => 6,
        }
    }

    /// Compare two decimals as numbers, ignoring scale: a total preorder.
    ///
    /// Finite values compare as the numbers they denote, every zero of
    /// either sign and any exponent `Equal` to every other (the General
    /// Decimal Arithmetic's compare treats "zero or negative zero" alike;
    /// Cowlishaw 2009, "compare", stated). Minus infinity is below every
    /// finite value and plus infinity above. The NaNs take compare-total's
    /// places: the negative quiet NaNs lowest, then the negative signalling
    /// NaNs, and above plus infinity the positive signalling NaNs, then the
    /// positive quiet NaNs; NaNs of one sign and flag compare `Equal`
    /// whatever their payloads. So [`compare_total`](Self::compare_total)
    /// refines this order: `Less` here implies `Less` there.
    #[must_use]
    pub fn cmp_numeric(&self, other: &Self) -> Ordering {
        if !(self.class.is_finite() && other.class.is_finite()) {
            return self.class_rank().cmp(&other.class_rank());
        }
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

    /// The General Decimal Arithmetic's compare-total, a total order whose
    /// equality is representation equality (Cowlishaw 2009, "compare-total",
    /// stated: `-NaN -sNaN -Infinity -127 -1.00 -1 -0.000 -0 0 1.2300 1.23
    /// 1E+9 Infinity sNaN NaN NaN456`, lowest to highest).
    ///
    /// Every value with its sign set is below every value without. Of two
    /// values of one sign: two finite values compare as numbers and, when
    /// equal as numbers, by exponent, the smaller exponent lower, for either
    /// sign; otherwise, for the positive sign, finite values are below the
    /// infinity, which is below the signalling NaNs, which are below the
    /// quiet NaNs, and two NaNs of one flag compare by payload; for the
    /// negative sign that order of the non-finite classes and payloads is
    /// reversed.
    #[must_use]
    pub fn compare_total(&self, other: &Self) -> Ordering {
        if self.negative != other.negative {
            return if self.negative {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
        if self.class.is_finite() && other.class.is_finite() {
            return self
                .cmp_numeric(other)
                .then_with(|| self.exponent.cmp(&other.exponent));
        }
        let rank = |d: &Self| match d.class {
            DecimalClass::Finite => 0_u8,
            DecimalClass::Infinite => 1,
            DecimalClass::NaN {
                signalling: true, ..
            } => 2,
            DecimalClass::NaN {
                signalling: false, ..
            } => 3,
        };
        let positive = match (&self.class, &other.class) {
            (
                DecimalClass::NaN {
                    signalling: left_flag,
                    payload: left,
                },
                DecimalClass::NaN {
                    signalling: right_flag,
                    payload: right,
                },
            ) if left_flag == right_flag => cmp_digit_strings(left, right),
            _ => rank(self).cmp(&rank(other)),
        };
        if self.negative {
            positive.reverse()
        } else {
            positive
        }
    }

    /// The canonical payload bytes.
    ///
    /// A finite value: its sign as one byte (`0`, or `1` for a negative
    /// number or a negative zero), the digits behind a little-endian `u64`
    /// length, then the exponent as its sign byte (`0` non-negative, `1`
    /// negative) and the decimal digits of its magnitude, with no leading
    /// zero, behind a little-endian `u64` length. An infinity: the byte `2`
    /// and its sign byte. A NaN: the byte `3`, its sign byte, its signalling
    /// byte (`0` quiet, `1` signalling) and its payload's digits behind a
    /// little-endian `u64` length. No finite layout starts with `2` or `3`,
    /// so the bytes of distinct values differ.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(18 + self.digits.len() + self.exponent.magnitude().len());
        match &self.class {
            DecimalClass::Finite => {
                out.push(u8::from(self.negative));
                put_str(&mut out, &self.digits);
                self.exponent.write_canonical(&mut out);
            }
            DecimalClass::Infinite => out.extend_from_slice(&[2, u8::from(self.negative)]),
            DecimalClass::NaN {
                signalling,
                payload,
            } => {
                out.extend_from_slice(&[3, u8::from(self.negative), u8::from(*signalling)]);
                put_str(&mut out, payload);
            }
        }
        out
    }
}

/// The closest-shortest decimal of a finite binary float whose magnitude
/// has the exact decimal expansion `expansion` (Rust's `{:e}` form with
/// enough digits to be exact, `d.ddd...e<exponent>`), given the test
/// `reads_back` of whether a decimal text reads back to that magnitude at
/// the float's width. `negative` is the float's sign; a zero gives zero.
///
/// For each count n of significant digits from one up, the only n-digit
/// decimals that can read back are the two that bracket the magnitude
/// (`lo`, its expansion cut to n digits, and `lo` plus one unit in the n-th
/// digit): the reals that round to a float form an interval around it, so
/// an n-digit decimal that reads back and lies beyond `lo` or `hi` has that
/// one between itself and the magnitude, which then reads back too. The
/// first n at which either reads back is the minimal significand; of two
/// that both read back the closer is taken, and at an exact tie (the
/// cut-off digits are exactly `5`) the even one. When n reaches the number
/// of digits of the expansion, `lo` is the magnitude itself.
///
/// `hint` (the length of Rust's shortest text) only skips counts that
/// cannot be the first: whether some n-digit decimal reads back is monotone
/// in n (an n-digit decimal is an (n + 1)-digit one with a zero appended),
/// so when no candidate of `hint - 1` digits reads back, none of fewer
/// digits does and the search starts at `hint`; otherwise it starts at one.
/// The result is the same either way.
pub(super) fn closest_shortest(
    negative: bool,
    expansion: &str,
    hint: usize,
    reads_back: impl Fn(&str) -> bool,
) -> DecimalValue {
    let (mantissa, power) = expansion.split_once('e').unwrap_or((expansion, "0"));
    let power = power.parse::<i64>().unwrap_or(0);
    let all: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let exact = all.trim_end_matches('0');
    if exact.is_empty() {
        return DecimalValue::from_i64(0);
    }
    // The candidate of n digits that the rule takes, if either reads back,
    // as its digits and the exponent of its last digit.
    let pick_at = |n: usize| -> Option<(String, i64)> {
        let lo = &exact[..n];
        let hi = increment(lo);
        // Both texts scale their digits so that the n-th digit of `lo`
        // stands at 10^exponent; `hi` has n + 1 digits only when it carried
        // into a new leading 1, which the same exponent places correctly.
        let exponent = power + 1 - i64::try_from(n).unwrap_or(i64::MAX);
        let pick = match (
            reads_back(&format!("{lo}e{exponent}")),
            reads_back(&format!("{hi}e{exponent}")),
        ) {
            (false, false) => return None,
            (true, false) => lo.to_owned(),
            (false, true) => hi,
            (true, true) => {
                // Both read back: the closer, by the cut-off digits `rest`
                // against one half of a unit in the n-th digit (`5`), and at
                // an exact tie the even one of two consecutive integers.
                let rest = &exact[n..];
                let lo_is_even = (lo.as_bytes()[n - 1] - b'0') % 2 == 0;
                let take_lo = match rest.as_bytes()[0].cmp(&b'5') {
                    Ordering::Less => true,
                    Ordering::Greater => false,
                    Ordering::Equal => rest.len() == 1 && lo_is_even,
                };
                if take_lo { lo.to_owned() } else { hi }
            }
        };
        Some((pick, exponent))
    };
    let first = if hint >= 2 && hint < exact.len() && pick_at(hint - 1).is_none() {
        hint
    } else {
        1
    };
    let (pick, exponent) = (first..exact.len()).find_map(pick_at).unwrap_or_else(|| {
        let n = i64::try_from(exact.len()).unwrap_or(i64::MAX);
        (exact.to_owned(), power + 1 - n)
    });
    let kept = pick.trim_end_matches('0');
    let exponent = exponent + i64::try_from(pick.len() - kept.len()).unwrap_or(0);
    DecimalValue {
        negative,
        digits: kept.to_owned(),
        exponent: WideInt::from(exponent),
        class: DecimalClass::Finite,
    }
}

/// The number of significant digits of Rust's shortest text of a float,
/// `d.ddd...e<exponent>` (`{:e}`).
fn shortest_len(text: &str) -> usize {
    let mantissa = text.split_once('e').map_or(text, |(m, _)| m);
    mantissa.bytes().filter(u8::is_ascii_digit).count()
}

/// The digit string `digits` plus one in its last place.
fn increment(digits: &str) -> String {
    let mut out: Vec<u8> = digits.bytes().collect();
    for at in (0..out.len()).rev() {
        if out[at] == b'9' {
            out[at] = b'0';
        } else {
            out[at] += 1;
            return out.into_iter().map(char::from).collect();
        }
    }
    let mut carried = vec![b'1'];
    carried.extend(out);
    carried.into_iter().map(char::from).collect()
}

/// Compare the magnitudes of two non-zero decimals.
fn cmp_magnitude(left: &DecimalValue, right: &DecimalValue) -> Ordering {
    // The position of the leading digit decides unless it ties.
    let lead = |d: &DecimalValue| d.exponent.plus(&WideInt::from(d.digits.len()));
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
    /// leading zeros, with a minus sign on a negative number or zero; and
    /// `Infinity`, `NaN` or `sNaN` (payload digits after a NaN unless the
    /// payload is zero), with a minus sign when the sign is set.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.negative {
            f.write_str("-")?;
        }
        match &self.class {
            DecimalClass::Finite => {}
            DecimalClass::Infinite => return f.write_str("Infinity"),
            DecimalClass::NaN {
                signalling,
                payload,
            } => {
                if *signalling {
                    f.write_str("s")?;
                }
                f.write_str("NaN")?;
                return if payload == "0" {
                    Ok(())
                } else {
                    f.write_str(payload)
                };
            }
        }
        let len = WideInt::from(self.digits.len());
        let floor = len.plus(&WideInt::from(6_i64)).negated();
        let positive = !self.exponent.is_negative() && !self.exponent.is_zero();
        if positive || self.exponent < floor {
            return write!(f, "{}e{}", self.digits, self.exponent);
        }
        // Here -(len + 6) <= exponent <= 0, so the scale fits `usize`.
        let scale = self
            .exponent
            .negated()
            .to_i64()
            .and_then(|s| usize::try_from(s).ok())
            .unwrap_or(0);
        if scale == 0 {
            return f.write_str(&self.digits);
        }
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
    fn float_expansion_precision_covers_the_exact_decimal() {
        assert_eq!(exact_precision(16, -1074), 767);
        assert_eq!(exact_precision(16, -52), 53);
        assert_eq!(exact_precision(16, 971), 310);
        assert_eq!(exact_precision(8, -149), 113);
        assert_eq!(exact_precision(8, -23), 25);
        assert_eq!(exact_precision(8, 104), 40);

        assert_eq!(exact_f64_precision(0.0), 767);
        assert_eq!(exact_f64_precision(f64::from_bits(1)), 767);
        assert_eq!(exact_f64_precision(1.0), 53);
        assert_eq!(exact_f64_precision(f64::MAX), 310);
        assert_eq!(exact_f32_precision(0.0), 113);
        assert_eq!(exact_f32_precision(f32::from_bits(1)), 113);
        assert_eq!(exact_f32_precision(1.0), 25);
        assert_eq!(exact_f32_precision(f32::MAX), 40);
    }

    /// The stored exponent of a lexeme whose written exponent is
    /// `(exponent_negative, exponent_digits)` and which has `frac_len`
    /// fractional digits, computed as [`DecimalValue::parse`] computes it.
    fn stored_exponent(exponent_negative: bool, exponent_digits: &str, frac_len: usize) -> String {
        WideInt::from_sign_and_digits(exponent_negative, exponent_digits)
            .map(|written| written.minus(&WideInt::from(frac_len)).to_string())
            .unwrap_or_default()
    }

    #[test]
    fn stored_exponent_is_exact_at_the_i32_bounds() {
        // A twelve-digit written exponent less a fractional digit count:
        // 10^11 - 97852516353 = i32::MAX, and one digit fewer is one more.
        assert_eq!(
            stored_exponent(false, "100000000000", 97_852_516_353),
            "2147483647"
        );
        assert_eq!(
            stored_exponent(false, "100000000000", 97_852_516_352),
            "2147483648"
        );
        // The bounds and one step beyond them, with and without fractional
        // digits: every one is read exactly, none refused.
        assert_eq!(stored_exponent(false, "2147483647", 0), "2147483647");
        assert_eq!(stored_exponent(true, "2147483648", 0), "-2147483648");
        assert_eq!(stored_exponent(false, "2147483648", 1), "2147483647");
        assert_eq!(stored_exponent(true, "2147483647", 1), "-2147483648");
        assert_eq!(stored_exponent(false, "0", 0), "0");
        assert_eq!(stored_exponent(true, "0", 3), "-3");
        assert_eq!(stored_exponent(false, "2147483648", 0), "2147483648");
        assert_eq!(stored_exponent(true, "2147483649", 0), "-2147483649");
        assert_eq!(stored_exponent(true, "2147483648", 1), "-2147483649");
        assert_eq!(stored_exponent(false, "0", 2_147_483_649), "-2147483649");
        // Long exponents are computed exactly whatever the fractional digit
        // count.
        assert_eq!(
            stored_exponent(false, "100000000000000000000", 0),
            "100000000000000000000"
        );
        assert_eq!(
            stored_exponent(false, "100000000000000000000", usize::MAX),
            (100_000_000_000_000_000_000_i128 - i128::try_from(usize::MAX).unwrap_or(0))
                .to_string()
        );
        assert_eq!(
            stored_exponent(true, "100000000000000000000", 0),
            "-100000000000000000000"
        );
        assert_eq!(stored_exponent(false, "999999999999", 0), "999999999999");
        assert_eq!(stored_exponent(true, "000100000000000", 0), "-100000000000");
        assert_eq!(stored_exponent(false, "99999999999", 0), "99999999999");
        assert_eq!(stored_exponent(true, "99999999999", 2), "-100000000001");
    }

    #[test]
    fn parse_reads_every_stored_exponent_exactly() {
        let exponent =
            |text: &str| DecimalValue::parse(text).map(|value| value.exponent.to_string());
        for (text, stored) in [
            ("1e2147483647", "2147483647"),
            ("0.1e2147483648", "2147483647"),
            ("1e-2147483648", "-2147483648"),
            ("1e000000000002147483647", "2147483647"),
            ("1e2147483648", "2147483648"),
            ("1.5e-2147483648", "-2147483649"),
            ("1e99999999999", "99999999999"),
            ("1e100000000000", "100000000000"),
            ("1e99999999999999999999999", "99999999999999999999999"),
            ("1e-99999999999999999999999", "-99999999999999999999999"),
            ("0.0e-3000000000", "-3000000001"),
            ("-0e3000000000", "3000000000"),
        ] {
            assert_eq!(exponent(text), Ok(stored.to_owned()), "{text}");
        }
        for text in ["1e", "1e+", "1e-", "1e1.5", "1ee1", "1e--1"] {
            assert_eq!(exponent(text), Err(not_a_number()), "{text}");
        }
    }
}
