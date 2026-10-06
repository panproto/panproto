//! Reading a lexeme into a [`Value`] under a declared scalar kind.
//!
//! [`coerce`] is the one coercion every codec calls: a codec that reads a
//! scalar hands over its [`Lexeme`] (the source text and the lexical class
//! the format gave it) and the [`ValueSpec`] of the vertex it belongs to,
//! and receives the exact [`Value`] or a [`ValueFidelityError`].
//!
//! # Declared kinds
//!
//! Under a declared kind a lexeme is read by the kind's lexical mapping
//! or refused with a typed [`FidelityReason`]; it is never silently
//! rounded to a value no reading gives the lexeme back from. Under
//! `Float { bits }` a finite numeral denotes its IEEE 754
//! round-to-nearest-even value at the kind's width (32 or 64 bits), and
//! the numeral is accepted exactly when that value equals the numeral, or
//! when the value's shortest round-trip text at that width is numerically
//! the numeral (so `0.1` is accepted under a 32-bit kind and read as the
//! binary32 nearest it, while `16777217.0`, which rounds to `16777216`, is
//! refused with [`FidelityReason::PrecisionExceeded`]). A negative-zero
//! numeral is the negative zero of the kind. Under `Decimal`, `Int` and
//! `BigInt` a numeral is accepted when its value lies in the kind's value
//! space and the spec's `precision`, `scale`, `int_bits` and `unsigned`
//! facets admit it.
//!
//! # Open schemas
//!
//! With [`ScalarKind::Any`] the value is the most exact one the lexeme
//! allows: an integer that fits `i64` is [`Value::Int`] and a larger one
//! [`Value::BigInt`]; a number with a fraction or exponent is
//! [`Value::Float`] exactly when the float is finite and its shortest
//! decimal is numerically the lexeme, and [`Value::Decimal`] otherwise
//! (`-0.0` is `Float(-0.0)`, `0.10` is `Float(0.1)`, `4e-324` is a
//! decimal); strings stay strings.
//!
//! # Non-finite floats
//!
//! The NaN spellings (`NaN`, `nan`, `+nan`, `-nan`, `.nan`, in any case)
//! all read as the canonical quiet NaN, bits `0x7ff8000000000000`, under
//! both float kinds and under `Any`: the float value space has one
//! notANumber, written without a sign (W3C 2012, XML Schema 1.1 Datatypes,
//! Secs 3.3.4.1 and 3.3.4.2, stated), and TOML 1.0 leaves the encoding of
//! `-nan` to the implementation (TOML 1.0.0, Float, stated). So no sign or
//! payload is read from a lexeme. The infinity spellings read as the
//! signed infinities.
//!
//! [`FidelityLog`] collects the errors of a whole document, in document
//! order, into one [`ParseError::Fidelity`].

use panproto_gat::Name;
use panproto_schema::{BytesText, ScalarKind, ValueSpec};

use super::{
    BigIntValue, DecimalValue, ExactInteger, FidelityReason, Offset, Temporal, Value,
    ValueFidelityError, ValueSite,
};
use crate::error::ParseError;

/// A scalar as a source wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lexeme<'a> {
    /// The source text of the scalar, without quotes or escapes.
    pub text: &'a str,
    /// The lexical class the format gave the scalar.
    pub class: LexClass,
}

/// The lexical class a format gives a scalar.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LexClass {
    /// A numeral (JSON number, unquoted YAML or TOML number, SQL numeric
    /// literal), including the NaN and infinity spellings of the formats
    /// that have them.
    Number,
    /// A string.
    String,
    /// A boolean literal.
    Bool,
    /// A null literal.
    Null,
    /// A date, time or date-time literal (TOML, YAML timestamps, SQL
    /// temporal literals).
    DateTime,
    /// Bytes written as text (base64, hexadecimal, or raw).
    Bytes,
    /// A scalar carrying an explicit type tag (a YAML tag, an extended-JSON
    /// type key), named by the tag.
    Tagged(Name),
}

impl<'a> Lexeme<'a> {
    /// The lexeme `text` of class `class`.
    #[must_use]
    pub const fn new(text: &'a str, class: LexClass) -> Self {
        Self { text, class }
    }
}

/// An error for `lexeme` read as `kind`, at an unknown site.
fn fail(lexeme: &Lexeme<'_>, kind: &ScalarKind, reason: FidelityReason) -> ValueFidelityError {
    ValueFidelityError {
        site: ValueSite::unknown(),
        lexeme: lexeme.text.to_owned(),
        target: kind.clone(),
        reason,
    }
}

/// Read `lexeme` as a value of `spec`.
///
/// A null lexeme is [`Value::Null`] under every kind: whether the vertex
/// may be null is a validation question, not a fidelity one. The error's
/// site is unknown; the codec sets it with [`ValueFidelityError::at`] (or
/// reads through [`FidelityLog::coerce`], which does).
///
/// ```
/// use panproto_inst::Value;
/// use panproto_inst::value::coerce::{LexClass, Lexeme, coerce};
/// use panproto_inst::value::{DecimalValue, FidelityReason};
/// use panproto_schema::{ScalarKind, ValueSpec};
///
/// let number = |text| Lexeme::new(text, LexClass::Number);
/// let any = ValueSpec::new(ScalarKind::Any);
/// assert_eq!(coerce(&number("0.1"), &any)?, Value::Float(0.1));
/// assert_eq!(
///     coerce(&number("1e400"), &any)?,
///     Value::Decimal(DecimalValue::parse("1e400")?)
/// );
///
/// let money = ValueSpec { scale: Some(2), ..ValueSpec::new(ScalarKind::Decimal) };
/// let err = coerce(&number("1.005"), &money).unwrap_err();
/// assert_eq!(err.reason, FidelityReason::ScaleExceeded { max: 2 });
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
///
/// A [`ValueFidelityError`] naming the lexeme, the kind and the
/// [`FidelityReason`] the kind cannot hold it for.
pub fn coerce(lexeme: &Lexeme<'_>, spec: &ValueSpec) -> Result<Value, ValueFidelityError> {
    if lexeme.class == LexClass::Null {
        return Ok(Value::Null);
    }
    if let LexClass::Tagged(tag) = &lexeme.class {
        return coerce_tagged(lexeme, tag, spec);
    }
    let kind = &spec.kind;
    let fail = |reason| fail(lexeme, kind, reason);
    match kind {
        ScalarKind::Any => coerce_any(lexeme),
        ScalarKind::Bool => coerce_bool(lexeme).ok_or_else(|| {
            fail(if lexeme.class == LexClass::Number {
                FidelityReason::NotANumberForKind
            } else {
                FidelityReason::InvalidEncoding
            })
        }),
        ScalarKind::Int { bits, signed } => {
            let bits = spec.int_bits.unwrap_or(*bits);
            let signed = *signed && !spec.unsigned;
            numeric_class(lexeme, kind)?;
            coerce_int(lexeme.text, bits, signed).map_err(fail)
        }
        ScalarKind::BigInt => {
            numeric_class(lexeme, kind)?;
            integral(lexeme.text).map(Value::integer).map_err(fail)
        }
        ScalarKind::Float { bits } => {
            numeric_class(lexeme, kind)?;
            coerce_float(lexeme.text, *bits).map_err(fail)
        }
        ScalarKind::Decimal => {
            numeric_class(lexeme, kind)?;
            coerce_decimal(lexeme.text, spec).map_err(fail)
        }
        ScalarKind::Str | ScalarKind::Token => match &lexeme.class {
            LexClass::Number => Err(fail(FidelityReason::NotANumberForKind)),
            LexClass::Bool => Err(fail(FidelityReason::InvalidEncoding)),
            _ if *kind == ScalarKind::Token => Ok(Value::Token(lexeme.text.to_owned())),
            _ => Ok(Value::Str(lexeme.text.to_owned())),
        },
        ScalarKind::Bytes(encoding) => match &lexeme.class {
            LexClass::Number => Err(fail(FidelityReason::NotANumberForKind)),
            LexClass::Bool => Err(fail(FidelityReason::InvalidEncoding)),
            _ => decode_bytes(lexeme.text, encoding)
                .map(Value::Bytes)
                .ok_or_else(|| fail(FidelityReason::InvalidEncoding)),
        },
        ScalarKind::Date | ScalarKind::Time | ScalarKind::DateTime => match &lexeme.class {
            LexClass::Number => Err(fail(FidelityReason::NotANumberForKind)),
            LexClass::Bool => Err(fail(FidelityReason::InvalidTemporal)),
            _ => coerce_temporal(lexeme.text, kind, spec.with_time_zone).map_err(fail),
        },
    }
}

/// Reject a lexeme whose class cannot write a number: booleans, and bytes.
/// Strings pass, since text formats (CSV, XML) write every number as one.
fn numeric_class(lexeme: &Lexeme<'_>, kind: &ScalarKind) -> Result<(), ValueFidelityError> {
    match &lexeme.class {
        LexClass::Bool | LexClass::Bytes | LexClass::DateTime => {
            Err(fail(lexeme, kind, FidelityReason::NotANumber))
        }
        _ => Ok(()),
    }
}

/// `true` or `false`, in any case.
fn coerce_bool(lexeme: &Lexeme<'_>) -> Option<Value> {
    match &lexeme.class {
        LexClass::Bool | LexClass::String => {
            if lexeme.text.eq_ignore_ascii_case("true") {
                Some(Value::Bool(true))
            } else if lexeme.text.eq_ignore_ascii_case("false") {
                Some(Value::Bool(false))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The non-finite float a lexeme spells, if it spells one: `NaN`, `inf`,
/// `Infinity` and YAML's `.nan` and `.inf`, in any case, with an optional
/// sign. Every NaN spelling gives the canonical quiet NaN, `f64::NAN`
/// (bits `0x7ff8000000000000`), whatever its sign.
fn non_finite(text: &str) -> Option<f64> {
    let (negative, unsigned) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let unsigned = unsigned.strip_prefix('.').unwrap_or(unsigned);
    if unsigned.eq_ignore_ascii_case("nan") {
        Some(f64::NAN)
    } else if unsigned.eq_ignore_ascii_case("inf") || unsigned.eq_ignore_ascii_case("infinity") {
        Some(if negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        })
    } else {
        None
    }
}

/// The most zeros a positive exponent may expand into when an integral
/// decimal numeral is read as an integer.
const MAX_EXPANDED_ZEROS: usize = 4096;

/// Whether a numeral has neither a fraction nor an exponent.
fn is_integer_numeral(text: &str) -> bool {
    !text.contains(['.', 'e', 'E'])
}

/// The value of an open-schema lexeme (see the module documentation).
fn coerce_any(lexeme: &Lexeme<'_>) -> Result<Value, ValueFidelityError> {
    let fail = |reason| fail(lexeme, &ScalarKind::Any, reason);
    match &lexeme.class {
        LexClass::Number => {
            if let Some(f) = non_finite(lexeme.text) {
                return Ok(Value::Float(f));
            }
            if is_integer_numeral(lexeme.text) {
                return Value::parse_integer(lexeme.text).map_err(|e| fail(e.reason));
            }
            let decimal = DecimalValue::parse(lexeme.text).map_err(|e| fail(e.reason))?;
            let x = nearest_float(&decimal, lexeme.text, 64).map_err(fail)?;
            Ok(float_if_shortest(&decimal, x).map_or(Value::Decimal(decimal), Value::Float))
        }
        LexClass::Bool => coerce_bool(lexeme).ok_or_else(|| fail(FidelityReason::InvalidEncoding)),
        LexClass::DateTime => Temporal::parse(lexeme.text)
            .map(Value::Temporal)
            .map_err(|e| fail(e.reason)),
        LexClass::Bytes => decode_bytes(lexeme.text, &BytesText::Base64)
            .map(Value::Bytes)
            .ok_or_else(|| fail(FidelityReason::InvalidEncoding)),
        LexClass::String | LexClass::Null | LexClass::Tagged(_) => {
            Ok(Value::Str(lexeme.text.to_owned()))
        }
    }
}

/// `x` when it is finite and its shortest decimal is numerically
/// `decimal`.
fn float_if_shortest(decimal: &DecimalValue, x: f64) -> Option<f64> {
    let back = DecimalValue::from_f64_shortest(x)?;
    back.cmp_numeric(decimal).is_eq().then_some(x)
}

/// The float of `bits` bits nearest the numeral `text`, which reads as
/// `decimal`, widened to `f64`: IEEE 754 round-to-nearest-even at the
/// width (32, or 64 for any other width), by Rust's correctly rounded
/// parsers, which may give an infinity or a zero outside the width's
/// range. A zero numeral written with a `-` is the negative zero, which
/// `decimal` cannot carry.
fn nearest_float(decimal: &DecimalValue, text: &str, bits: u8) -> Result<f64, FidelityReason> {
    let negative = decimal.negative || (decimal.is_zero() && text.starts_with('-'));
    let sign = if negative { "-" } else { "" };
    let numeral = format!("{sign}{}e{}", decimal.digits, decimal.exponent);
    if bits == 32 {
        numeral.parse::<f32>().map(f64::from)
    } else {
        numeral.parse::<f64>()
    }
    .map_err(|_| FidelityReason::NotANumber)
}

/// The shortest text that reads back as `x` at `bits` bits (32, or 64 for
/// any other width), for a finite `x` that the width holds.
fn shortest_text(x: f64, bits: u8) -> Option<DecimalValue> {
    if bits == 32 {
        let narrow = format!("{x:e}").parse::<f32>().ok()?;
        DecimalValue::parse(&format!("{narrow:e}")).ok()
    } else {
        DecimalValue::from_f64_shortest(x)
    }
}

/// The most significant digits a finite binary64 value's exact decimal
/// expansion has (that of the largest subnormal); a binary32 value, widened,
/// needs fewer.
const MAX_EXACT_DIGITS: usize = 767;

/// Whether the finite float `x` equals `decimal` exactly. A binary float's
/// decimal expansion terminates, within [`MAX_EXACT_DIGITS`] significant
/// digits, and Rust's formatting with an explicit precision writes the
/// correctly rounded digits, so at that precision it writes the expansion
/// itself.
fn denotes_exactly(decimal: &DecimalValue, x: f64) -> bool {
    if decimal.digits.trim_end_matches('0').len() > MAX_EXACT_DIGITS {
        return false;
    }
    DecimalValue::parse(&format!("{x:.MAX_EXACT_DIGITS$e}"))
        .is_ok_and(|exact| exact.cmp_numeric(decimal).is_eq())
}

/// The exact integer a numeral writes: an integer numeral, or a decimal
/// numeral whose value is integral (`1.0`, `1e3`).
fn integral(text: &str) -> Result<ExactInteger, FidelityReason> {
    if non_finite(text).is_some() {
        return Err(FidelityReason::NonFinite);
    }
    if is_integer_numeral(text) {
        return BigIntValue::parse(text).map_err(|e| e.reason);
    }
    let decimal = DecimalValue::parse(text).map_err(|e| e.reason)?;
    if decimal.minimal_scale() > 0 {
        return Err(FidelityReason::ScaleExceeded { max: 0 });
    }
    if decimal.is_zero() {
        return Ok(ExactInteger::Int(0));
    }
    // Integral with a non-negative effective exponent: the digits without
    // their trailing fractional zeros, then zeros for a positive exponent.
    let exponent = i64::from(decimal.exponent);
    let digits = if exponent >= 0 {
        let zeros = usize::try_from(exponent).map_err(|_| FidelityReason::Overflow)?;
        // An exponent that writes more than 4096 zeros is refused rather
        // than expanded: no fixed-width kind holds such an integer (128
        // bits is 39 digits), and `1e999999999` would otherwise allocate a
        // gigabyte of zeros.
        if zeros > MAX_EXPANDED_ZEROS {
            return Err(FidelityReason::Overflow);
        }
        format!("{}{}", decimal.digits, "0".repeat(zeros))
    } else {
        let cut = usize::try_from(-exponent).map_err(|_| FidelityReason::Overflow)?;
        decimal.digits[..decimal.digits.len() - cut].to_owned()
    };
    BigIntValue::from_parts(decimal.negative, &digits).map_err(|e| e.reason)
}

/// An integer of `bits` bits, signed or not.
fn coerce_int(text: &str, bits: u8, signed: bool) -> Result<Value, FidelityReason> {
    let value = integral(text)?;
    let bits = u32::from(bits.clamp(1, 128));
    let in_range = match &value {
        ExactInteger::Int(n) => {
            let n = i128::from(*n);
            if signed {
                let half = 1_i128 << (bits - 1).min(126);
                bits > 127 || (-half <= n && n < half)
            } else {
                n >= 0 && (bits >= 64 || n < (1_i128 << bits))
            }
        }
        ExactInteger::Big(big) => {
            // Outside i64: only a kind wider than 64 bits holds it.
            let magnitude = big.digits.parse::<u128>().ok();
            match (signed, big.negative, magnitude) {
                (_, _, None) | (false, true, _) => false,
                (false, false, Some(m)) => bits >= 128 || m < (1_u128 << bits),
                (true, negative, Some(m)) => {
                    let half = 1_u128 << (bits - 1);
                    if negative { m <= half } else { m < half }
                }
            }
        }
    };
    if in_range {
        Ok(Value::integer(value))
    } else {
        Err(FidelityReason::Overflow)
    }
}

/// A float of `bits` bits (32 or 64; any other width reads as 64): the
/// value nearest the numeral, accepted when it equals the numeral or its
/// shortest text at the width is numerically the numeral (see the module
/// documentation).
fn coerce_float(text: &str, bits: u8) -> Result<Value, FidelityReason> {
    if let Some(f) = non_finite(text) {
        return Ok(Value::Float(f));
    }
    let decimal = DecimalValue::parse(text).map_err(|e| e.reason)?;
    let x = nearest_float(&decimal, text, bits)?;
    if !x.is_finite() {
        return Err(FidelityReason::Overflow);
    }
    let shortest = shortest_text(x, bits).is_some_and(|back| back.cmp_numeric(&decimal).is_eq());
    if shortest || denotes_exactly(&decimal, x) {
        Ok(Value::Float(x))
    } else {
        Err(FidelityReason::PrecisionExceeded {
            max: if bits == 32 { 9 } else { 17 },
        })
    }
}

/// A decimal within the spec's precision and scale.
///
/// With a scale `s`, the number may need at most `s` digits after the
/// point (trailing zeros beyond it are no loss); with a precision `p` as
/// well, at most `p - s` before it; with a precision alone, at most `p`
/// digits in all.
fn coerce_decimal(text: &str, spec: &ValueSpec) -> Result<Value, FidelityReason> {
    let decimal = DecimalValue::parse(text).map_err(|e| e.reason)?;
    if let Some(scale) = spec.scale
        && decimal.minimal_scale() > scale
    {
        return Err(FidelityReason::ScaleExceeded { max: scale });
    }
    if let Some(precision) = spec.precision {
        let fits = spec.scale.map_or_else(
            || decimal.significant_digits() <= u64::from(precision),
            |scale| decimal.integer_digits() <= u64::from(precision.saturating_sub(scale)),
        );
        if !fits {
            return Err(FidelityReason::PrecisionExceeded { max: precision });
        }
    }
    Ok(Value::Decimal(decimal))
}

/// A date, time or date-time of `kind`, with the offset `with_time_zone`
/// requires (`Some(true)`: an offset; `Some(false)`: none).
fn coerce_temporal(
    text: &str,
    kind: &ScalarKind,
    with_time_zone: Option<bool>,
) -> Result<Value, FidelityReason> {
    let parsed = match kind {
        ScalarKind::Date => Temporal::parse_date(text),
        ScalarKind::Time => Temporal::parse_time(text),
        _ => Temporal::parse_date_time(text),
    }
    .map_err(|e| e.reason)?;
    match (parsed.offset(), with_time_zone) {
        (Some(Offset::Local), Some(true)) => Err(FidelityReason::OffsetRequired),
        (Some(offset), Some(false)) if offset.is_zoned() => Err(FidelityReason::OffsetForbidden),
        _ => Ok(Value::Temporal(parsed)),
    }
}

/// The value of a digit in `alphabet`, if it is one.
fn digit_value(alphabet: &[u8; 64], byte: u8) -> Option<u32> {
    alphabet
        .iter()
        .position(|&b| b == byte)
        .and_then(|i| u32::try_from(i).ok())
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const BASE64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Decode base64 over `alphabet`, padded or not; `None` for a character
/// outside the alphabet, a misplaced or wrong amount of padding, a length
/// no encoding produces, or nonzero bits after the last byte (which a
/// canonical encoder never writes, so two texts never decode to one
/// value).
fn decode_base64(text: &str, alphabet: &[u8; 64]) -> Option<Vec<u8>> {
    let body = text.trim_end_matches('=');
    let padding = text.len() - body.len();
    if padding > 2 || (padding > 0 && text.len() % 4 != 0) || body.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(body.len() * 3 / 4);
    let mut acc = 0_u32;
    let mut bits = 0_u32;
    for byte in body.bytes() {
        acc = (acc << 6) | digit_value(alphabet, byte)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xff).ok()?);
            acc &= (1 << bits) - 1;
        }
    }
    (acc == 0).then_some(out)
}

/// Decode hexadecimal, either case, an even number of digits.
fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    text.as_bytes()
        .chunks(2)
        .map(|pair| {
            let high = char::from(pair[0]).to_digit(16)?;
            let low = char::from(pair[1]).to_digit(16)?;
            u8::try_from(high * 16 + low).ok()
        })
        .collect()
}

/// The bytes `text` encodes in `encoding`.
fn decode_bytes(text: &str, encoding: &BytesText) -> Option<Vec<u8>> {
    match encoding {
        BytesText::Base64 => decode_base64(text, BASE64),
        BytesText::Base64Url => decode_base64(text, BASE64_URL),
        BytesText::Hex => decode_hex(text),
        BytesText::Raw => Some(text.as_bytes().to_vec()),
    }
}

/// A type tag without any YAML `tag:yaml.org,2002:` or `!!` prefix.
fn tag_name(tag: &str) -> &str {
    tag.strip_prefix("tag:yaml.org,2002:")
        .or_else(|| tag.strip_prefix("!!"))
        .unwrap_or(tag)
}

/// The scalar kind a type tag names (see [`tag_name`]): the YAML core and
/// extended-JSON names of the scalar kinds.
fn tagged_kind(name: &str) -> Option<ScalarKind> {
    Some(match name {
        "bool" | "boolean" => ScalarKind::Bool,
        "int" | "integer" | "numberLong" | "numberInt" => ScalarKind::BigInt,
        "float" | "double" | "numberDouble" => ScalarKind::Float { bits: 64 },
        "decimal" | "numberDecimal" => ScalarKind::Decimal,
        "str" | "string" => ScalarKind::Str,
        "binary" | "base64" => ScalarKind::Bytes(BytesText::Base64),
        "hex" => ScalarKind::Bytes(BytesText::Hex),
        "timestamp" | "datetime" | "date-time" => ScalarKind::DateTime,
        "date" => ScalarKind::Date,
        "time" => ScalarKind::Time,
        _ => return None,
    })
}

/// A tagged scalar: the declared kind governs when there is one; under
/// [`ScalarKind::Any`] the tag's own kind does, and a tag naming no scalar
/// kind keeps the scalar as an opaque value of that type, so the tag is
/// never dropped.
fn coerce_tagged(
    lexeme: &Lexeme<'_>,
    tag: &Name,
    spec: &ValueSpec,
) -> Result<Value, ValueFidelityError> {
    if spec.kind != ScalarKind::Any {
        return coerce(&Lexeme::new(lexeme.text, LexClass::String), spec);
    }
    let name = tag_name(tag.as_ref());
    if name == "null" {
        return Ok(Value::Null);
    }
    tagged_kind(name).map_or_else(
        || {
            Ok(Value::Opaque {
                type_: tag.as_ref().to_owned(),
                fields: std::iter::once(("value".to_owned(), Value::Str(lexeme.text.to_owned())))
                    .collect(),
            })
        },
        |kind| {
            let class = match kind {
                ScalarKind::Bool => LexClass::Bool,
                ScalarKind::Str => LexClass::String,
                ScalarKind::Bytes(_) => LexClass::Bytes,
                ScalarKind::Date | ScalarKind::Time | ScalarKind::DateTime => LexClass::DateTime,
                _ => LexClass::Number,
            };
            coerce(&Lexeme::new(lexeme.text, class), &ValueSpec::new(kind))
        },
    )
}

/// The fidelity errors of one document, collected in the order its values
/// are read.
///
/// A codec reads every scalar through [`coerce`](Self::coerce), which
/// records an error instead of stopping, so one parse reports every value
/// the document cannot hold; [`finish`](Self::finish) turns them into one
/// [`ParseError::Fidelity`]. There is no lenient mode: a value whose error
/// is recorded is not produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FidelityLog {
    errors: Vec<ValueFidelityError>,
}

impl FidelityLog {
    /// An empty log.
    #[must_use]
    pub const fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Read `lexeme` as a value of `spec` at `site`, recording the error
    /// (with its site) and returning `None` when the kind cannot hold it.
    pub fn coerce(
        &mut self,
        site: ValueSite,
        lexeme: &Lexeme<'_>,
        spec: &ValueSpec,
    ) -> Option<Value> {
        match coerce(lexeme, spec) {
            Ok(value) => Some(value),
            Err(error) => {
                self.errors.push(error.at(site));
                None
            }
        }
    }

    /// Record an error the codec found itself.
    pub fn record(&mut self, error: ValueFidelityError) {
        self.errors.push(error);
    }

    /// The errors recorded so far, in order.
    #[must_use]
    pub fn errors(&self) -> &[ValueFidelityError] {
        &self.errors
    }

    /// `Ok` when nothing was recorded, else every recorded error, in order.
    ///
    /// # Errors
    ///
    /// [`ParseError::Fidelity`] listing every recorded error.
    pub fn finish(self) -> Result<(), ParseError> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(ParseError::Fidelity(self.errors))
        }
    }
}

/// Coerce an already-built `value` to `spec`, at `site`: the value a column
/// of kind `spec` holds for it.
///
/// The value is read back through its lexeme: under [`ScalarKind::Any`] it
/// is returned unchanged; a non-finite [`Value::Float`] is returned
/// unchanged under a float kind and refused with
/// [`FidelityReason::NonFinite`] under any other; every other value with a
/// scalar lexeme is [`coerce`] of that lexeme, which is `true` or `false`
/// of class [`LexClass::Bool`] for a boolean, the decimal digits of class
/// [`LexClass::Number`] for an integer, the canonical text of class
/// `Number` for a decimal, the shortest text that reads back as the same
/// float (Rust's `Display`) of class `Number` for a finite float (under a
/// 32-bit float kind, a float that a 32-bit float holds exactly takes
/// instead the shortest text that reads back as that 32-bit float, the
/// lexeme its column writes), the text of class [`LexClass::String`] for a
/// string or a token, the RFC 3339 text of class [`LexClass::DateTime`] for
/// a date or time, and `null` of class [`LexClass::Null`] for null. Any
/// other value (bytes, links, blobs,
/// records, lists, labelled nulls) has no scalar lexeme and is returned
/// unchanged; validation types it.
///
/// So an `Ok` value denotes exactly what the input's lexeme denotes, since
/// [`coerce`] holds a lexeme exactly or refuses it, and coercing the result
/// again returns it unchanged.
///
/// ```
/// use panproto_inst::Value;
/// use panproto_inst::value::ValueSite;
/// use panproto_inst::value::coerce::coerce_value;
/// use panproto_schema::{ScalarKind, ValueSpec};
///
/// let int = ValueSpec::new(ScalarKind::Int { bits: 64, signed: true });
/// let site = ValueSite::new("person.age", None);
/// assert_eq!(coerce_value(&Value::Float(2.0), &int, site.clone())?, Value::Int(2));
/// assert!(coerce_value(&Value::Float(2.5), &int, site.clone()).is_err());
/// let any = ValueSpec::new(ScalarKind::Any);
/// assert_eq!(coerce_value(&Value::Float(2.0), &any, site)?, Value::Float(2.0));
/// # Ok::<(), panproto_inst::ValueFidelityError>(())
/// ```
///
/// # Errors
///
/// A [`ValueFidelityError`] at `site` when the kind cannot hold the value's
/// lexeme exactly.
pub fn coerce_value(
    value: &Value,
    spec: &ValueSpec,
    site: ValueSite,
) -> Result<Value, ValueFidelityError> {
    if spec.kind == ScalarKind::Any {
        return Ok(value.clone());
    }
    let (text, class) = match value {
        Value::Float(f) if !f.is_finite() => {
            return if matches!(spec.kind, ScalarKind::Float { .. }) {
                Ok(value.clone())
            } else {
                Err(ValueFidelityError {
                    site,
                    lexeme: f.to_string(),
                    target: spec.kind.clone(),
                    reason: FidelityReason::NonFinite,
                })
            };
        }
        Value::Bool(b) => (b.to_string(), LexClass::Bool),
        Value::Int(n) => (n.to_string(), LexClass::Number),
        Value::BigInt(n) => (n.to_string(), LexClass::Number),
        Value::Decimal(d) => (d.to_string(), LexClass::Number),
        Value::Float(f) => (float_text(*f, &spec.kind), LexClass::Number),
        Value::Str(s) | Value::Token(s) => (s.clone(), LexClass::String),
        Value::Temporal(t) => (t.to_string(), LexClass::DateTime),
        Value::Null => ("null".to_owned(), LexClass::Null),
        Value::Bytes(_)
        | Value::CidLink(_)
        | Value::Blob { .. }
        | Value::Opaque { .. }
        | Value::Unknown(_)
        | Value::List(_)
        | Value::LabeledNull(_) => return Ok(value.clone()),
    };
    coerce(&Lexeme::new(&text, class), spec).map_err(|error| error.at(site))
}

/// The lexeme of a finite float coerced to `kind`: the shortest text that
/// reads back as the float, except under a 32-bit float kind, where a float
/// that a 32-bit float holds exactly reads back through `f32`.
///
/// A 32-bit column stores the `f32` nearest its lexeme, widened to `f64`
/// (`0.1` is stored as `0.100000001490116...`); the `f64` text of that value
/// would name a number the column cannot hold, so the value is written as
/// the column writes it.
fn float_text(f: f64, kind: &ScalarKind) -> String {
    let text = f.to_string();
    if matches!(kind, ScalarKind::Float { bits: 32 })
        && let Ok(narrow) = text.parse::<f32>()
        && f64::from(narrow).to_bits() == f.to_bits()
    {
        // The `f32` nearest the text is the float itself exactly when a
        // 32-bit float holds it: the text is within half an `f64` unit of
        // the float, far inside half an `f32` unit.
        return narrow.to_string();
    }
    text
}
