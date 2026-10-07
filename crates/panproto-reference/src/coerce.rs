//! Schema-kind-driven coercion of lexemes and of values.
//!
//! [`coerce`] reads a lexeme under a declared scalar kind; [`coerce_value`]
//! coerces an existing value by reading its lexeme the same way, with the
//! narrowing rules for float kinds. Both are written straight from their
//! definitions, with exact rationals for every float decision.
//!
//! Domain: the default numeral reading (no numeral dialect declared) and
//! untagged lexemes. A tagged lexeme (a discriminator table's input) and a
//! float width other than 16, 32 and 64 are outside the reference's domain
//! and give `None`.

use crate::exact::{
    Decimal, DecimalValue, ExactInteger, NonFinite, Scaled, integer_of_numeral, parse_decimal,
    parse_non_finite, parse_numeral,
};
use crate::float::{
    CANONICAL_NAN_BITS, Dec, FloatValue, Format, NEGATIVE_CANONICAL_NAN_BITS, Rounded, decode,
    holds_exactly, narrow_bits, round_to, shortest, widen_bits,
};
use crate::num::{Int, Nat, Rat};
use crate::temporal::{self, Offset, TemporalKind};
use crate::value::Value;

/// How bytes are written as text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BytesText {
    /// Standard base64, padding optional on read.
    Base64,
    /// URL-safe base64, padding optional on read.
    Base64Url,
    /// Lower- or upper-case hexadecimal, two digits per byte.
    Hex,
    /// The UTF-8 bytes of the text.
    Raw,
    /// Standard base64 with padding required.
    Base64Padded,
    /// URL-safe base64 with padding required.
    Base64UrlPadded,
    /// One code point U+0000 to U+00FF per byte.
    Latin1,
}

/// A declared scalar kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScalarKind {
    /// Booleans.
    Bool,
    /// Integers of a width and signedness.
    Int {
        /// Width in bits.
        bits: u8,
        /// Whether negative values are admitted.
        signed: bool,
    },
    /// Integers of any size.
    BigInt,
    /// Binary floats of a width.
    Float {
        /// Width in bits (16, 32 or 64).
        bits: u8,
    },
    /// Exact decimals.
    Decimal,
    /// Strings.
    Str,
    /// Bytes written as text.
    Bytes(BytesText),
    /// Dates.
    Date,
    /// Times.
    Time,
    /// Date-times.
    DateTime,
    /// Tokens.
    Token,
    /// No declared kind (open schema).
    Any,
}

/// How a float column reads a finite numeral.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum FloatReading {
    /// Law (i): accept iff exact or the canonical text of the rounded value.
    #[default]
    Exact,
    /// Read every finite numeral as its nearest value, keeping the lexeme.
    NearestKeepingLexeme,
}

/// A kind with its value parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ValueSpec {
    /// The kind.
    pub kind: ScalarKind,
    /// Decimal precision.
    pub precision: Option<u32>,
    /// Decimal scale.
    pub scale: Option<u32>,
    /// Whether an offset is required (`Some(true)`) or forbidden.
    pub with_time_zone: Option<bool>,
    /// Integer width overriding the kind's.
    pub int_bits: Option<u8>,
    /// Whether the integer kind is unsigned.
    pub unsigned: bool,
    /// The float reading.
    pub float_reading: FloatReading,
}

impl ValueSpec {
    /// The spec of `kind` with no parameters.
    #[must_use]
    pub const fn of(kind: ScalarKind) -> Self {
        Self {
            kind,
            precision: None,
            scale: None,
            with_time_zone: None,
            int_bits: None,
            unsigned: false,
            float_reading: FloatReading::Exact,
        }
    }
}

/// The lexical class a format gave a scalar.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LexClass {
    /// A numeral (with the NaN and infinity spellings).
    Number,
    /// A string.
    String,
    /// A boolean literal.
    Bool,
    /// A null literal.
    Null,
    /// A temporal literal.
    DateTime,
    /// Bytes written as text.
    Bytes,
    /// A scalar with a type tag.
    Tagged(String),
}

/// Why a lexeme or value is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FidelityReason {
    /// Out of the kind's range.
    Overflow,
    /// Not held exactly; `max` is the kind's significant-digit bound.
    PrecisionExceeded {
        /// The bound.
        max: u32,
    },
    /// Too many fractional digits.
    ScaleExceeded {
        /// The bound.
        max: u32,
    },
    /// Not a numeral.
    NotANumber,
    /// A NaN or infinity the kind cannot hold.
    NonFinite,
    /// Not a temporal of the kind.
    InvalidTemporal,
    /// An offset is required.
    OffsetRequired,
    /// An offset is forbidden.
    OffsetForbidden,
    /// Not in the kind's encoding.
    InvalidEncoding,
    /// A numeral under a non-numeric kind.
    NotANumberForKind,
}

/// The outcome of a reference coercion: `None` outside the reference's
/// domain, else the value or the typed refusal.
pub type Coerced = Option<Result<Value, FidelityReason>>;

/// The bound on decimal positions inside which values are computed exactly:
/// every format overflows at `10^400` and rounds every non-zero magnitude
/// below `10^-400` to zero.
const BOUND: i64 = 400;

const fn nan_bits(negative: bool) -> u64 {
    if negative {
        NEGATIVE_CANONICAL_NAN_BITS
    } else {
        CANONICAL_NAN_BITS
    }
}

const fn inf_bits(negative: bool) -> u64 {
    Rounded::Overflow { negative }.bits(Format::B64)
}

/// Read `lexeme` of class `class` under `spec`: the value-kind coercion, with exact numerals and canonical shortest numerals only.
#[must_use]
pub fn coerce(text: &str, class: &LexClass, spec: &ValueSpec) -> Coerced {
    if *class == LexClass::Null {
        return Some(Ok(Value::Null));
    }
    if matches!(class, LexClass::Tagged(_)) {
        return None;
    }
    let numeric_class = || match class {
        LexClass::Bool | LexClass::Bytes | LexClass::DateTime => Err(FidelityReason::NotANumber),
        _ => Ok(()),
    };
    Some(match spec.kind {
        ScalarKind::Any => coerce_any(text, class),
        ScalarKind::Bool => coerce_bool(text, class),
        ScalarKind::Int { bits, signed } => {
            let bits = spec.int_bits.unwrap_or(bits);
            if bits == 0 {
                return None;
            }
            numeric_class().and_then(|()| coerce_int(text, Some((bits, signed && !spec.unsigned))))
        }
        ScalarKind::BigInt => numeric_class().and_then(|()| coerce_int(text, None)),
        ScalarKind::Float { bits } => {
            let format = Format::of_width(bits)?;
            numeric_class().and_then(|()| coerce_float(text, format, spec.float_reading))
        }
        ScalarKind::Decimal => numeric_class().and_then(|()| coerce_decimal(text, spec)),
        ScalarKind::Str | ScalarKind::Token => match class {
            LexClass::Number => Err(FidelityReason::NotANumberForKind),
            LexClass::Bool => Err(FidelityReason::InvalidEncoding),
            _ if spec.kind == ScalarKind::Token => Ok(Value::Token(text.to_owned())),
            _ => Ok(Value::Str(text.to_owned())),
        },
        ScalarKind::Bytes(encoding) => match class {
            LexClass::Number => Err(FidelityReason::NotANumberForKind),
            LexClass::Bool => Err(FidelityReason::InvalidEncoding),
            _ => decode_bytes(text, encoding)
                .map(Value::Bytes)
                .ok_or(FidelityReason::InvalidEncoding),
        },
        ScalarKind::Date | ScalarKind::Time | ScalarKind::DateTime => match class {
            LexClass::Number => Err(FidelityReason::NotANumberForKind),
            LexClass::Bool => Err(FidelityReason::InvalidTemporal),
            _ => coerce_temporal(text, spec),
        },
    })
}

fn coerce_bool(text: &str, class: &LexClass) -> Result<Value, FidelityReason> {
    let read = match class {
        LexClass::Bool | LexClass::String => {
            if text.eq_ignore_ascii_case("true") {
                Some(true)
            } else if text.eq_ignore_ascii_case("false") {
                Some(false)
            } else {
                None
            }
        }
        _ => None,
    };
    read.map(Value::Bool).ok_or(if *class == LexClass::Number {
        FidelityReason::NotANumberForKind
    } else {
        FidelityReason::InvalidEncoding
    })
}

fn coerce_any(text: &str, class: &LexClass) -> Result<Value, FidelityReason> {
    match class {
        LexClass::Number => {
            if let Some(nf) = parse_non_finite(text) {
                return Ok(Value::Float(non_finite_bits(nf)));
            }
            let n = parse_numeral(text).ok_or(FidelityReason::NotANumber)?;
            if n.is_integer_form() {
                return integer_of_numeral(&n)
                    .map(Value::integer)
                    .ok_or(FidelityReason::NotANumber);
            }
            let d = DecimalValue::of_numeral(&n);
            match nearest(Format::B64, &d) {
                Rounded::Finite(bits) => {
                    let shortest_equal =
                        shortest(Format::B64, bits).is_some_and(|s| dec_equals(&s, &d));
                    Ok(if shortest_equal {
                        Value::Float(bits)
                    } else {
                        Value::Decimal(Decimal::Finite(d))
                    })
                }
                Rounded::Overflow { .. } => Ok(Value::Decimal(Decimal::Finite(d))),
            }
        }
        LexClass::Bool => coerce_bool(text, class).map_err(|_| FidelityReason::InvalidEncoding),
        LexClass::DateTime => temporal::parse_any(text)
            .map(Value::Temporal)
            .ok_or(FidelityReason::InvalidTemporal),
        LexClass::Bytes => decode_bytes(text, BytesText::Base64)
            .map(Value::Bytes)
            .ok_or(FidelityReason::InvalidEncoding),
        LexClass::String | LexClass::Null | LexClass::Tagged(_) => Ok(Value::Str(text.to_owned())),
    }
}

const fn non_finite_bits(nf: NonFinite) -> u64 {
    match nf {
        NonFinite::NaN { negative } => nan_bits(negative),
        NonFinite::Infinity { negative } => inf_bits(negative),
    }
}

/// `L_b` of a decimal (the sign of a zero kept), with magnitudes beyond the
/// exact bound classified without expansion.
fn nearest(format: Format, d: &DecimalValue) -> Rounded {
    match d.scaled(-BOUND, BOUND) {
        Scaled::Value(q) => round_to(format, &q, d.negative),
        Scaled::AtLeastHi => Rounded::Overflow {
            negative: d.negative,
        },
        Scaled::BelowLo => Rounded::Finite(if d.negative { sign_bit(format) } else { 0 }),
    }
}

const fn sign_bit(format: Format) -> u64 {
    1u64 << (format.width() - 1)
}

/// Whether a printed decimal equals a decimal numerically.
fn dec_equals(s: &Dec, d: &DecimalValue) -> bool {
    let as_decimal = DecimalValue {
        negative: s.negative,
        digits: s.coefficient.clone(),
        exponent: Int::from_i64(s.exponent),
    };
    as_decimal.cmp_numeric(d).is_eq()
}

/// Whether `d` denotes the finite `format` value `bits` exactly.
fn denotes_exactly(format: Format, d: &DecimalValue, bits: u64) -> bool {
    match (d.scaled(-BOUND, BOUND), decode(format, bits)) {
        (Scaled::Value(q), FloatValue::Finite { value, .. }) => q == value,
        _ => false,
    }
}

fn coerce_float(
    text: &str,
    format: Format,
    reading: FloatReading,
) -> Result<Value, FidelityReason> {
    if let Some(nf) = parse_non_finite(text) {
        return Ok(Value::Float(non_finite_bits(nf)));
    }
    let n = parse_numeral(text).ok_or(FidelityReason::NotANumber)?;
    let d = DecimalValue::of_numeral(&n);
    let rounded = nearest(format, &d);
    if reading == FloatReading::NearestKeepingLexeme {
        return Ok(Value::Float(widen_bits(format, rounded.bits(format))));
    }
    let Rounded::Finite(bits) = rounded else {
        return Err(FidelityReason::Overflow);
    };
    let canonical = shortest(format, bits).is_some_and(|s| dec_equals(&s, &d));
    if canonical || denotes_exactly(format, &d, bits) {
        Ok(Value::Float(widen_bits(format, bits)))
    } else {
        Err(FidelityReason::PrecisionExceeded {
            max: format.max_significant_digits(),
        })
    }
}

/// `-2^(bits-1)` and `2^(bits-1)` (signed) or `0` and `2^bits` (unsigned):
/// the half-open range of an integer kind, as decimals.
fn int_range(bits: u8, signed: bool) -> (DecimalValue, DecimalValue) {
    let as_dec = |negative: bool, n: Nat| DecimalValue {
        negative,
        digits: n,
        exponent: Int::zero(),
    };
    if signed {
        let half = Nat::pow2(u32::from(bits) - 1);
        (as_dec(true, half.clone()), as_dec(false, half))
    } else {
        (
            as_dec(false, Nat::zero()),
            as_dec(false, Nat::pow2(u32::from(bits))),
        )
    }
}

fn coerce_int(text: &str, width: Option<(u8, bool)>) -> Result<Value, FidelityReason> {
    if parse_non_finite(text).is_some() {
        return Err(FidelityReason::NonFinite);
    }
    let n = parse_numeral(text).ok_or(FidelityReason::NotANumber)?;
    let d = DecimalValue::of_numeral(&n);
    if !d.minimal_scale().is_zero() {
        return Err(FidelityReason::ScaleExceeded { max: 0 });
    }
    if let Some((bits, signed)) = width {
        let (lo, hi) = int_range(bits, signed);
        if d.cmp_numeric(&lo).is_lt() || d.cmp_numeric(&hi).is_ge() {
            return Err(FidelityReason::Overflow);
        }
    }
    integer_of_numeral(&n)
        .map(Value::integer)
        .ok_or(FidelityReason::NotANumber)
}

fn coerce_decimal(text: &str, spec: &ValueSpec) -> Result<Value, FidelityReason> {
    let dec = parse_decimal(text).ok_or(FidelityReason::NotANumber)?;
    if let Decimal::Finite(d) = &dec {
        if let Some(scale) = spec.scale
            && d.minimal_scale() > Nat::from_u64(u64::from(scale))
        {
            return Err(FidelityReason::ScaleExceeded { max: scale });
        }
        if let Some(precision) = spec.precision {
            let fits = spec.scale.map_or_else(
                || d.positional_digits() <= Nat::from_u64(u64::from(precision)),
                |scale| {
                    d.integer_digits() <= Nat::from_u64(u64::from(precision.saturating_sub(scale)))
                },
            );
            if !fits {
                return Err(FidelityReason::PrecisionExceeded { max: precision });
            }
        }
    }
    Ok(Value::Decimal(dec))
}

fn coerce_temporal(text: &str, spec: &ValueSpec) -> Result<Value, FidelityReason> {
    let kind = match spec.kind {
        ScalarKind::Date => TemporalKind::Date,
        ScalarKind::Time => TemporalKind::Time,
        _ => TemporalKind::DateTime,
    };
    let t = temporal::parse(kind, text).ok_or(FidelityReason::InvalidTemporal)?;
    match (t.offset, spec.with_time_zone) {
        (Offset::Local, Some(true)) => Err(FidelityReason::OffsetRequired),
        (o, Some(false)) if o.is_zoned() => Err(FidelityReason::OffsetForbidden),
        _ => Ok(Value::Temporal(t)),
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64 by its definition (RFC 4648 Sec 4, Sec 5): each character is six
/// bits, the bit string is cut into bytes, the leftover bits (fewer than
/// eight) must be zero, a body of length 1 mod 4 is no encoding, and
/// padding brings the length to a multiple of four.
fn decode_base64(text: &str, alphabet: &[u8; 64], padding_required: bool) -> Option<Vec<u8>> {
    let body = text.trim_end_matches('=');
    let pads = text.len() - body.len();
    let expected_pads = (4 - body.len() % 4) % 4;
    let padding_ok = if padding_required {
        pads == expected_pads
    } else {
        pads == 0 || pads == expected_pads
    };
    if !padding_ok || body.len() % 4 == 1 {
        return None;
    }
    let mut bits: Vec<bool> = Vec::new();
    for c in body.bytes() {
        let v = alphabet.iter().position(|a| *a == c)?;
        for k in (0..6).rev() {
            bits.push((v >> k) & 1 == 1);
        }
    }
    let whole = bits.len() / 8 * 8;
    if bits[whole..].iter().any(|b| *b) {
        return None;
    }
    Some(
        bits[..whole]
            .chunks(8)
            .map(|byte| byte.iter().fold(0u8, |acc, b| (acc << 1) | u8::from(*b)))
            .collect(),
    )
}

fn decode_bytes(text: &str, encoding: BytesText) -> Option<Vec<u8>> {
    match encoding {
        BytesText::Base64 => decode_base64(text, B64, false),
        BytesText::Base64Url => decode_base64(text, B64_URL, false),
        BytesText::Base64Padded => decode_base64(text, B64, true),
        BytesText::Base64UrlPadded => decode_base64(text, B64_URL, true),
        BytesText::Hex => {
            if text.len() % 2 != 0 {
                return None;
            }
            text.as_bytes()
                .chunks(2)
                .map(|p| {
                    let h = char::from(p[0]).to_digit(16)?;
                    let l = char::from(p[1]).to_digit(16)?;
                    u8::try_from(h * 16 + l).ok()
                })
                .collect()
        }
        BytesText::Raw => Some(text.as_bytes().to_vec()),
        BytesText::Latin1 => text
            .chars()
            .map(|c| u8::try_from(u32::from(c)).ok())
            .collect(),
    }
}

/// The positional text of a decimal (no exponent), the form Rust's
/// `Display` writes a float in: `0.001`, `1000`, `-2.5`.
#[must_use]
pub fn positional(d: &Dec) -> String {
    let digits = d.coefficient.to_string();
    let sign = if d.negative { "-" } else { "" };
    if d.exponent >= 0 {
        let zeros = usize::try_from(d.exponent).unwrap_or(0);
        return format!(
            "{sign}{digits}{}",
            "0".repeat(if d.coefficient.is_zero() { 0 } else { zeros })
        );
    }
    let frac = usize::try_from(-d.exponent).unwrap_or(0);
    if digits.len() > frac {
        let (i, f) = digits.split_at(digits.len() - frac);
        format!("{sign}{i}.{f}")
    } else {
        format!("{sign}0.{}{digits}", "0".repeat(frac - digits.len()))
    }
}

/// The lexeme of a value with a scalar lexeme, and its class: the value
/// read back through [`coerce`] by [`coerce_value`]. A finite float is its
/// shortest binary64 text (its shortest binary32 text under a 32-bit kind
/// when it is a binary32 value), written positionally; `None` for a value
/// with no scalar lexeme.
fn lexeme_of(v: &Value, kind: ScalarKind) -> Option<(String, LexClass)> {
    Some(match v {
        Value::Bool(b) => (b.to_string(), LexClass::Bool),
        Value::Int(i) => (i.to_string(), LexClass::Number),
        Value::BigInt(b) => {
            let sign = if b.negative { "-" } else { "" };
            (
                format!("{sign}{}e{}", b.significand, b.zeros),
                LexClass::Number,
            )
        }
        Value::Decimal(Decimal::Finite(d)) => {
            let sign = if d.negative { "-" } else { "" };
            (
                format!("{sign}{}e{}", d.digits, d.exponent),
                LexClass::Number,
            )
        }
        Value::Float(bits) => {
            let narrow = match kind {
                ScalarKind::Float { bits: 32 } => {
                    narrow_bits(Format::B32, *bits).map(|b| (Format::B32, b))
                }
                _ => None,
            };
            let (format, b) = narrow.unwrap_or((Format::B64, *bits));
            (positional(&shortest(format, b)?), LexClass::Number)
        }
        Value::Str(s) | Value::Token(s) => (s.clone(), LexClass::String),
        Value::Temporal(t) => (t.to_string(), LexClass::DateTime),
        Value::Null => ("null".to_owned(), LexClass::Null),
        _ => return None,
    })
}

/// The exact value of a numeric value, for the float-kind rules.
fn exact_numeric(v: &Value) -> Option<DecimalValue> {
    match v {
        Value::Int(i) => Some(ExactInteger::Int(*i).as_decimal()),
        Value::BigInt(b) => Some(ExactInteger::Big(b.clone()).as_decimal()),
        Value::Decimal(Decimal::Finite(d)) => Some(d.clone()),
        _ => None,
    }
}

/// Coerce an existing value under `spec`.
///
/// Rules: under `Any` the value is unchanged. Under a float kind of width b
/// (and the exact reading): a finite `Float` is kept unchanged iff b holds
/// it exactly, else refused with `Overflow` when it rounds to an infinity at
/// b and `PrecisionExceeded { max }` otherwise; an infinity is kept; a NaN
/// is kept iff its fraction bits below b's fraction are zero (quiet or
/// signalling), else refused with `NonFinite`; an `Int`, `BigInt` or finite
/// `Decimal` gives `Float(x)` iff b holds its value x exactly (a decimal
/// zero keeps its sign), else `Overflow` or `PrecisionExceeded`; a special
/// decimal is refused with `NonFinite`. Under any other non-`Any` kind a
/// non-finite `Float` is refused with `NonFinite`, a value with a scalar
/// lexeme is read through [`coerce`], and any other value is unchanged.
/// `None` under the nearest-keeping-lexeme reading, a tagged spec, or a
/// float width other than 16, 32 and 64.
#[must_use]
pub fn coerce_value(v: &Value, spec: &ValueSpec) -> Coerced {
    if spec.kind == ScalarKind::Any {
        return Some(Ok(v.clone()));
    }
    if let ScalarKind::Float { bits } = spec.kind {
        let format = Format::of_width(bits)?;
        if spec.float_reading == FloatReading::NearestKeepingLexeme {
            return None;
        }
        return Some(coerce_into_float(v, format));
    }
    if let Value::Float(bits) = v
        && !matches!(decode(Format::B64, *bits), FloatValue::Finite { .. })
    {
        return Some(Err(FidelityReason::NonFinite));
    }
    match lexeme_of(v, spec.kind) {
        Some((text, class)) => coerce(&text, &class, spec),
        None => Some(Ok(v.clone())),
    }
}

fn coerce_into_float(v: &Value, format: Format) -> Result<Value, FidelityReason> {
    let reject = |q: &Rat, negative: bool| match round_to(format, q, negative) {
        Rounded::Overflow { .. } => FidelityReason::Overflow,
        Rounded::Finite(_) => FidelityReason::PrecisionExceeded {
            max: format.max_significant_digits(),
        },
    };
    match v {
        Value::Float(bits) => match decode(Format::B64, *bits) {
            FloatValue::Finite { negative, value } => {
                if holds_exactly(format, &value) {
                    Ok(v.clone())
                } else {
                    Err(reject(&value, negative))
                }
            }
            FloatValue::Infinite { .. } => Ok(v.clone()),
            FloatValue::NaN { .. } => narrow_bits(format, *bits)
                .map(|_| v.clone())
                .ok_or(FidelityReason::NonFinite),
        },
        Value::Decimal(Decimal::Infinite { .. } | Decimal::NaN { .. }) => {
            Err(FidelityReason::NonFinite)
        }
        _ => match exact_numeric(v) {
            Some(d) => match d.scaled(-BOUND, BOUND) {
                Scaled::AtLeastHi => Err(FidelityReason::Overflow),
                Scaled::BelowLo => Err(FidelityReason::PrecisionExceeded {
                    max: format.max_significant_digits(),
                }),
                Scaled::Value(q) => {
                    if holds_exactly(format, &q) {
                        Ok(Value::Float(
                            round_to(Format::B64, &q, d.negative).bits(Format::B64),
                        ))
                    } else {
                        Err(reject(&q, d.negative))
                    }
                }
            },
            None => match lexeme_of(v, ScalarKind::Float { bits: 64 }) {
                Some((text, class)) => coerce(
                    &text,
                    &class,
                    &ValueSpec::of(ScalarKind::Float {
                        bits: u8::try_from(format.width()).unwrap_or(64),
                    }),
                )
                .unwrap_or(Err(FidelityReason::NotANumber)),
                None => Ok(v.clone()),
            },
        },
    }
}
