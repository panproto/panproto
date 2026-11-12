//! `PostgreSQL` 17 values: the input text of each column type read into a
//! [`Value`], and the value written back as its output text.
//!
//! The input text is what a type's input function reads: the body of a
//! string constant, a CSV cell or a `COPY` text field. Each rule below is
//! the input function's, restricted to the forms whose value the instance
//! model holds exactly; any other form is refused with a [`ValueRefusal`]
//! naming why, never rounded, truncated or reinterpreted.
//!
//! - `smallint`, `integer`, `bigint`: optional surrounding white space, an
//!   optional sign, decimal digits or a `0x`, `0o` or `0b` prefix and its
//!   digits, with `_` between two digits (or after the prefix), in the
//!   type's range (Sec 4.1.2.6 and `pg_strtoint`).
//! - `numeric`: the same white space and sign, decimal digits with an
//!   optional point and exponent, or `NaN`, `Infinity`, `inf` with an
//!   optional sign, in any case. The value has the display scale
//!   `PostgreSQL` gives it: the digits after the point less the exponent,
//!   never below zero, so `1e2` is `100` and `1.50` keeps its scale; a zero
//!   has no sign. Under `numeric(p, s)` a value with more than `s` digits
//!   after the point is refused (`PostgreSQL` would round it), the scale is
//!   padded to `s`, and more than `p - s` integer digits or an infinity is
//!   refused.
//! - `real`, `double precision`: a decimal numeral read by the shared
//!   coercion, which refuses a numeral the type would round, or `NaN`,
//!   `Infinity`, `+Infinity`, `-Infinity`, `inf`, `+inf`, `-inf` in any
//!   case.
//! - `text`, `character varying(n)`, `character(n)`: the text, with no NUL
//!   character; a value longer than `n` characters is refused unless the
//!   excess is spaces, which are cut (Sec 8.3); `character(n)` pads to `n`
//!   with spaces.
//! - `boolean`: `boolin`'s spellings, white space trimmed, any case: a
//!   prefix of `true`, `false`, `yes` or `no`, at least two letters of `on`
//!   or `off`, or `1` or `0`.
//! - `bytea`: the hex format `\x` followed by pairs of hex digits, with
//!   white space between pairs, or the escape format, where `\\` is a
//!   backslash and `\` with three octal digits (the first at most 3) is a
//!   byte (Sec 8.4).
//! - `date`, `time`, `timestamp`, `timestamp with time zone`: the ISO 8601
//!   forms the shared temporal reader reads, kept as written (separator,
//!   fraction digits, offset form), with at most six fraction digits (more
//!   would be rounded to microseconds); `timestamp with time zone` needs an
//!   offset and the others refuse one, which `PostgreSQL` would ignore; a
//!   date or timestamp may be `infinity` or `-infinity`.
//! - `uuid`: 32 hex digits, optionally in braces, with a hyphen allowed
//!   after any group of four; the value is the canonical lowercase form.
//! - `json`, `jsonb`: a JSON text, kept as written. `jsonb`'s normal form
//!   (sorted keys, the last of duplicate keys) is not computed.
//! - an enum: one of its labels, exactly.
//! - an array: `array_in`'s form, `{` elements `}` nested once per
//!   dimension, an element quoted with `"` (and `\` escaping any character)
//!   or unquoted with its surrounding white space trimmed, unquoted `NULL`
//!   in any case a null element; sub-arrays of one level have one length,
//!   and the nesting is the column's declared number of dimensions or the
//!   empty array `{}`. An array with explicit bounds (`[1:2]={...}`) is
//!   refused.

use std::fmt::Write as _;

use panproto_gat::Name;
use panproto_inst::Value;
use panproto_inst::value::coerce::{LexClass, Lexeme, coerce};
use panproto_inst::value::{
    DecimalClass, DecimalValue, ExtendedTemporal, FidelityReason, Offset, Temporal, TemporalForm,
    TemporalInfinity, binary32_bits,
};
use panproto_schema::{ScalarKind, Schema, ValueSpec};

/// Why a value is refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ValueRefusal {
    /// The shared coercion refuses the value for the type, for this reason.
    #[error("{0}")]
    Fidelity(FidelityReason),
    /// The text is not a form the type's input reads.
    #[error("not {expected}")]
    Syntax {
        /// The form the type reads.
        expected: &'static str,
    },
    /// `PostgreSQL` would round the value to fit the type.
    #[error("a value the type would round")]
    Rounded,
    /// A text longer than the type's length, other than by spaces.
    #[error("longer than {max} characters")]
    TooLong {
        /// The type's length.
        max: u32,
    },
    /// A text that is not a label of the column's enum type.
    #[error("not a label of the enum type")]
    NotALabel,
    /// An array whose nesting is not the column's declared dimensions.
    #[error("an array of {found} dimensions in a column of {declared}")]
    Dimensions {
        /// The column's declared dimensions.
        declared: usize,
        /// The array's dimensions.
        found: usize,
    },
    /// A value the codec does not carry, named.
    #[error("{0} is not read")]
    Unsupported(&'static str),
    /// A value of another kind than the column's type holds.
    #[error("a {found} value in a column of type {expected}")]
    NotOfType {
        /// The column's type.
        expected: &'static str,
        /// The value's kind.
        found: &'static str,
    },
}

/// The element type of a column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Element {
    /// An integer of this many bits.
    Int(u8),
    /// `numeric`, with its precision and scale when declared.
    Numeric(Option<(u32, u32)>),
    /// `real`.
    Real,
    /// `double precision`.
    Double,
    /// `text` or `character varying`, with its length when declared.
    Text(Option<u32>),
    /// `character(n)`.
    Char(u32),
    /// `boolean`.
    Bool,
    /// `bytea`.
    Bytea,
    /// `date`.
    Date,
    /// `time without time zone`.
    Time,
    /// `timestamp`, with or without time zone.
    Timestamp(bool),
    /// `uuid`.
    Uuid,
    /// `json` (`false`) or `jsonb` (`true`).
    Json(bool),
    /// An enum type's labels.
    Enum(Vec<String>),
}

impl Element {
    /// The type's name, as refusals spell it.
    pub(crate) const fn name(&self) -> &'static str {
        match self {
            Self::Int(16) => "smallint",
            Self::Int(32) => "integer",
            Self::Int(_) => "bigint",
            Self::Numeric(_) => "numeric",
            Self::Real => "real",
            Self::Double => "double precision",
            Self::Text(_) => "text",
            Self::Char(_) => "character",
            Self::Bool => "boolean",
            Self::Bytea => "bytea",
            Self::Date => "date",
            Self::Time => "time",
            Self::Timestamp(false) => "timestamp",
            Self::Timestamp(true) => "timestamp with time zone",
            Self::Uuid => "uuid",
            Self::Json(false) => "json",
            Self::Json(true) => "jsonb",
            Self::Enum(_) => "enum",
        }
    }

    const fn is_numeric(&self) -> bool {
        matches!(
            self,
            Self::Int(_) | Self::Numeric(_) | Self::Real | Self::Double
        )
    }
}

/// The type of a column: its element type and array dimensions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColumnType {
    /// The element type.
    pub element: Element,
    /// The array dimensions.
    pub dims: usize,
}

impl ColumnType {
    /// The type of the column vertex `vertex` of `schema`, as the DDL
    /// reader builds it: an `array` vertex with an `items` edge per
    /// dimension, and the element's vertex kind and value parameters.
    pub(crate) fn of(schema: &Schema, vertex: &Name) -> Option<Self> {
        let mut element = vertex.clone();
        let mut dims = 0;
        loop {
            let v = schema.vertices.get(&element)?;
            if v.kind != "array" {
                break;
            }
            element = schema
                .outgoing_edges(&element)
                .iter()
                .find(|e| e.kind == "items")?
                .tgt
                .clone();
            dims += 1;
        }
        let kind = schema.vertices.get(&element)?.kind.as_str();
        let parameter = |sort: &str| {
            schema
                .constraints
                .get(&element)
                .and_then(|cs| cs.iter().find(|c| c.sort == sort).map(|c| c.value.clone()))
        };
        let number = |sort: &str| parameter(sort).and_then(|v| v.parse::<u32>().ok());
        let element = match kind {
            "integer" => Element::Int(match parameter("int_bits").as_deref() {
                Some("16") => 16,
                Some("32") => 32,
                _ => 64,
            }),
            "number" => {
                Element::Numeric(number("precision").map(|p| (p, number("scale").unwrap_or(0))))
            }
            "real" => Element::Real,
            "double" => Element::Double,
            "string" => match parameter("ENUM") {
                Some(json) => Element::Enum(serde_json::from_str(&json).ok()?),
                None => Element::Text(number("max_length")),
            },
            "character" => Element::Char(number("max_length").unwrap_or(1)),
            "boolean" => Element::Bool,
            "bytes" => Element::Bytea,
            "date" => Element::Date,
            "time" => Element::Time,
            "timestamp" => {
                Element::Timestamp(parameter("with_time_zone").as_deref() == Some("true"))
            }
            "uuid" => Element::Uuid,
            "json" => Element::Json(false),
            "jsonb" => Element::Json(true),
            _ => return None,
        };
        Some(Self { element, dims })
    }
}

/// White space as `PostgreSQL`'s input functions skip it (`isspace`).
const fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{0B}' | '\u{0C}')
}

fn trim(text: &str) -> &str {
    text.trim_matches(is_space)
}

fn syntax(expected: &'static str) -> ValueRefusal {
    ValueRefusal::Syntax { expected }
}

/// Read the input text `text` of a column of type `ty`.
pub(crate) fn decode(text: &str, ty: &ColumnType) -> Result<Value, ValueRefusal> {
    if ty.dims == 0 {
        return decode_element(text, &ty.element);
    }
    let array = parse_array(text)?;
    let found = array.depth();
    if found != 0 && found != ty.dims {
        return Err(ValueRefusal::Dimensions {
            declared: ty.dims,
            found,
        });
    }
    array.into_value(&ty.element)
}

/// Read an unquoted numeric constant `token` (an integer or float literal
/// of the grammar, `_` allowed between digits), negated when `negative`,
/// into a column of type `ty`.
///
/// The constant is an `integer` or `numeric` value cast to the column's
/// type: a numeric type reads it as its input text does, except that a
/// float reads a zero as positive (a `numeric` zero has no sign); any
/// other type refuses it.
pub(crate) fn decode_numeral(
    token: &str,
    negative: bool,
    ty: &ColumnType,
) -> Result<Value, ValueRefusal> {
    if ty.dims > 0 || !ty.element.is_numeric() {
        return Err(ValueRefusal::Unsupported(
            "a numeric constant for a column of a non-numeric type",
        ));
    }
    let text = format!("{}{}", if negative { "-" } else { "" }, token);
    match &ty.element {
        Element::Int(bits) => {
            if let Ok(value) = integer(&text, *bits) {
                return Ok(Value::Int(value));
            }
            // A float constant is a `numeric`, cast to the integer type.
            let (decimal, _) = numeric_parts(&text)?;
            let integral = integral_text(&decimal).ok_or(ValueRefusal::Rounded)?;
            integer(&integral, *bits).map(Value::Int)
        }
        Element::Real | Element::Double => {
            let (decimal, _) = numeric_parts(&text)?;
            if decimal.is_zero() {
                return Ok(Value::Float(0.0));
            }
            float(&decimal.to_string(), &ty.element)
        }
        element => decode_element(&text, element),
    }
}

/// The integer text of a finite decimal, or `None` when its fraction is
/// not all zeros (a cast to an integer type would round it).
fn integral_text(decimal: &DecimalValue) -> Option<String> {
    let scale = decimal
        .exponent
        .negated()
        .to_i64()
        .and_then(|s| usize::try_from(s).ok())
        .unwrap_or(0);
    let digits = &decimal.digits;
    if digits == "0" {
        return Some("0".to_owned());
    }
    if scale >= digits.len() {
        return None;
    }
    let (integer, fraction) = digits.split_at(digits.len() - scale);
    if fraction.bytes().any(|b| b != b'0') {
        return None;
    }
    Some(format!(
        "{}{integer}",
        if decimal.negative { "-" } else { "" }
    ))
}

fn decode_element(text: &str, element: &Element) -> Result<Value, ValueRefusal> {
    match element {
        Element::Int(bits) => integer(text, *bits).map(Value::Int),
        Element::Numeric(typmod) => numeric(text, *typmod).map(Value::Decimal),
        Element::Real | Element::Double => float_input(text, element),
        Element::Text(max) => sized_text(text, *max, false).map(Value::Str),
        Element::Char(n) => sized_text(text, Some(*n), true).map(Value::Str),
        Element::Bool => boolean(text).map(Value::Bool),
        Element::Bytea => bytea(text).map(Value::Bytes),
        Element::Date | Element::Time | Element::Timestamp(_) => {
            temporal(text, element).map(Value::Temporal)
        }
        Element::Uuid => uuid(text).map(Value::Str),
        Element::Json(_) => {
            if text.contains('\0') {
                return Err(ValueRefusal::Unsupported("a NUL character"));
            }
            serde_json::from_str::<serde::de::IgnoredAny>(text)
                .map_err(|_| syntax("a JSON text"))?;
            Ok(Value::Str(text.to_owned()))
        }
        Element::Enum(labels) => {
            if labels.iter().any(|l| l == text) {
                Ok(Value::Str(text.to_owned()))
            } else {
                Err(ValueRefusal::NotALabel)
            }
        }
    }
}

/// The digits of `text` after an optional radix prefix, `_` removed: the
/// radix and the digits, or `None` when an underscore is not between two
/// digits (after a prefix, one may follow it directly).
fn radix_digits(text: &str) -> Option<(u32, String)> {
    let (radix, body, after_prefix) = match text.get(..2) {
        Some("0x" | "0X") => (16, &text[2..], true),
        Some("0o" | "0O") => (8, &text[2..], true),
        Some("0b" | "0B") => (2, &text[2..], true),
        _ => (10, text, false),
    };
    let mut digits = String::with_capacity(body.len());
    let mut previous_digit = after_prefix;
    let chars: Vec<char> = body.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c == '_' {
            let next_digit = chars.get(i + 1).is_some_and(|n| n.is_digit(radix));
            if !previous_digit || !next_digit {
                return None;
            }
            previous_digit = false;
        } else if c.is_digit(radix) {
            digits.push(*c);
            previous_digit = true;
        } else {
            return None;
        }
    }
    (!digits.is_empty()).then_some((radix, digits))
}

/// An integer of `bits` bits.
fn integer(text: &str, bits: u8) -> Result<i64, ValueRefusal> {
    let t = trim(text);
    let (negative, body) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let (radix, digits) = radix_digits(body).ok_or_else(|| syntax("an integer"))?;
    let overflow = ValueRefusal::Fidelity(FidelityReason::Overflow);
    let magnitude = u128::from_str_radix(&digits, radix).map_err(|_| overflow.clone())?;
    let value = if negative {
        0_i128.checked_sub_unsigned(magnitude)
    } else {
        i128::try_from(magnitude).ok()
    }
    .ok_or_else(|| overflow.clone())?;
    let (min, max) = match bits {
        16 => (i128::from(i16::MIN), i128::from(i16::MAX)),
        32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
        _ => (i128::from(i64::MIN), i128::from(i64::MAX)),
    };
    if value < min || value > max {
        return Err(overflow);
    }
    i64::try_from(value).map_err(|_| overflow)
}

/// The largest exponent `numeric_in` reads.
const MAX_EXPONENT: i64 = 1000;
/// The most digits after the point a `numeric` holds.
const MAX_SCALE: usize = 16_383;
/// The most digits before the point a `numeric` holds.
const MAX_INTEGER_DIGITS: usize = 131_072;

/// A `numeric` value of `numeric_in`'s text, with its display scale, and
/// whether it was written as a special value.
fn numeric_parts(text: &str) -> Result<(DecimalValue, bool), ValueRefusal> {
    let t = trim(text);
    let lower = t.to_ascii_lowercase();
    match lower.as_str() {
        "nan" => return Ok((DecimalValue::nan(false, false, "0"), true)),
        "infinity" | "+infinity" | "inf" | "+inf" => {
            return Ok((DecimalValue::infinity(false), true));
        }
        "-infinity" | "-inf" => return Ok((DecimalValue::infinity(true), true)),
        _ => {}
    }
    let (negative, body) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let not_numeric = || syntax("a numeric");
    // A radix prefix reads an integer.
    if matches!(body.get(..2), Some("0x" | "0X" | "0o" | "0O" | "0b" | "0B")) {
        let (radix, digits) = radix_digits(body).ok_or_else(not_numeric)?;
        let magnitude = u128::from_str_radix(&digits, radix)
            .map_err(|_| ValueRefusal::Unsupported("a non-decimal numeric beyond 128 bits"))?;
        let decimal = DecimalValue::new(negative && magnitude != 0, magnitude.to_string(), 0)
            .map_err(|_| not_numeric())?;
        return Ok((decimal, false));
    }
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(at) => {
            let exp = &body[at + 1..];
            let digits = exp.strip_prefix(['+', '-']).unwrap_or(exp);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(not_numeric());
            }
            let value = exp.parse::<i64>().unwrap_or(i64::MAX);
            if !(-MAX_EXPONENT..=MAX_EXPONENT).contains(&value) {
                return Err(ValueRefusal::Fidelity(FidelityReason::Overflow));
            }
            (&body[..at], value)
        }
        None => (body, 0),
    };
    let (int_part, frac_part) = mantissa
        .split_once('.')
        .map_or((mantissa, ""), |(i, f)| (i, f));
    let clean = |part: &str| -> Option<String> {
        if part.is_empty() {
            return Some(String::new());
        }
        radix_digits(part)
            .filter(|(radix, _)| *radix == 10)
            .map(|(_, digits)| digits)
    };
    let int_digits = clean(int_part).ok_or_else(not_numeric)?;
    let frac_digits = clean(frac_part).ok_or_else(not_numeric)?;
    if int_digits.is_empty() && frac_digits.is_empty() {
        return Err(not_numeric());
    }
    // The value is digits × 10^(exponent - fraction digits); its display
    // scale is never below zero.
    let frac_len = i64::try_from(frac_digits.len()).map_err(|_| not_numeric())?;
    let power = exponent - frac_len;
    let mut digits = format!("{int_digits}{frac_digits}");
    let scale = if power >= 0 {
        digits.push_str(&"0".repeat(usize::try_from(power).unwrap_or(0)));
        0
    } else {
        usize::try_from(-power).unwrap_or(usize::MAX)
    };
    let digits = digits.trim_start_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    if scale > MAX_SCALE
        || (digits != "0" && digits.len().saturating_sub(scale) > MAX_INTEGER_DIGITS)
    {
        return Err(ValueRefusal::Fidelity(FidelityReason::Overflow));
    }
    let negative = negative && digits != "0";
    let exponent = -i64::try_from(scale).map_err(|_| not_numeric())?;
    let decimal = DecimalValue::new(negative, digits, exponent).map_err(|_| not_numeric())?;
    Ok((decimal, false))
}

/// A `numeric` under its declared precision and scale.
fn numeric(text: &str, typmod: Option<(u32, u32)>) -> Result<DecimalValue, ValueRefusal> {
    let (decimal, _) = numeric_parts(text)?;
    let Some((precision, scale)) = typmod else {
        return Ok(decimal);
    };
    match decimal.class {
        DecimalClass::Finite => {}
        DecimalClass::NaN { .. } => return Ok(decimal),
        DecimalClass::Infinite => return Err(ValueRefusal::Fidelity(FidelityReason::NonFinite)),
    }
    let written = decimal
        .exponent
        .negated()
        .to_i64()
        .and_then(|s| u32::try_from(s).ok())
        .unwrap_or(u32::MAX);
    if written > scale {
        return Err(ValueRefusal::Fidelity(FidelityReason::ScaleExceeded {
            max: scale,
        }));
    }
    let pad = usize::try_from(scale - written).unwrap_or(0);
    let digits = if decimal.digits == "0" {
        "0".to_owned()
    } else {
        format!("{}{}", decimal.digits, "0".repeat(pad))
    };
    let integer_digits = if digits == "0" {
        0
    } else {
        digits
            .len()
            .saturating_sub(usize::try_from(scale).unwrap_or(0))
    };
    if integer_digits > usize::try_from(precision - scale).unwrap_or(0) {
        return Err(ValueRefusal::Fidelity(FidelityReason::PrecisionExceeded {
            max: precision,
        }));
    }
    DecimalValue::new(decimal.negative, digits, -i64::from(scale)).map_err(|_| syntax("a numeric"))
}

/// A float of the width `element` names, from a numeral or special
/// spelling in the shared coercion's default grammar.
fn float(numeral: &str, element: &Element) -> Result<Value, ValueRefusal> {
    let bits = if *element == Element::Real { 32 } else { 64 };
    coerce(
        &Lexeme::new(numeral, LexClass::Number),
        &ValueSpec::new(ScalarKind::Float { bits }),
    )
    .map_err(|e| ValueRefusal::Fidelity(e.reason))
}

/// `float4in` and `float8in`.
fn float_input(text: &str, element: &Element) -> Result<Value, ValueRefusal> {
    let t = trim(text);
    let special = match t.to_ascii_lowercase().as_str() {
        "nan" => Some("NaN"),
        "infinity" | "+infinity" | "inf" | "+inf" => Some("Infinity"),
        "-infinity" | "-inf" => Some("-Infinity"),
        _ => None,
    };
    if let Some(special) = special {
        return float(special, element);
    }
    let body = t.strip_prefix(['+', '-']).unwrap_or(t);
    let (mantissa, exponent) = body
        .split_once(['e', 'E'])
        .map_or((body, None), |(m, e)| (m, Some(e)));
    let (int_part, frac_part) = mantissa
        .split_once('.')
        .map_or((mantissa, ""), |(i, f)| (i, f));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && digits(e)
    });
    if (int_part.is_empty() && frac_part.is_empty())
        || !digits(int_part)
        || !digits(frac_part)
        || !exponent_ok
    {
        return Err(syntax("a floating-point number"));
    }
    let numeral = t.strip_prefix('+').unwrap_or(t);
    float(numeral, element)
}

/// A string of at most `max` characters, the excess cut when it is all
/// spaces, padded to `max` when `pad`.
fn sized_text(text: &str, max: Option<u32>, pad: bool) -> Result<String, ValueRefusal> {
    if text.contains('\0') {
        return Err(ValueRefusal::Unsupported("a NUL character"));
    }
    let Some(max) = max else {
        return Ok(text.to_owned());
    };
    let limit = usize::try_from(max).unwrap_or(usize::MAX);
    let count = text.chars().count();
    let mut out: String = if count > limit {
        let cut = text
            .char_indices()
            .nth(limit)
            .map_or(text.len(), |(at, _)| at);
        if text[cut..].chars().any(|c| c != ' ') {
            return Err(ValueRefusal::TooLong { max });
        }
        text[..cut].to_owned()
    } else {
        text.to_owned()
    };
    if pad {
        let have = out.chars().count();
        out.push_str(&" ".repeat(limit.saturating_sub(have)));
    }
    Ok(out)
}

/// `boolin`.
fn boolean(text: &str) -> Result<bool, ValueRefusal> {
    let t = trim(text).to_ascii_lowercase();
    let prefix_of = |word: &str, min: usize| t.len() >= min && word.starts_with(t.as_str());
    match t.as_bytes().first() {
        Some(b't') if prefix_of("true", 1) => Ok(true),
        Some(b'f') if prefix_of("false", 1) => Ok(false),
        Some(b'y') if prefix_of("yes", 1) => Ok(true),
        Some(b'n') if prefix_of("no", 1) => Ok(false),
        Some(b'o') if prefix_of("on", 2) => Ok(true),
        Some(b'o') if prefix_of("off", 2) => Ok(false),
        Some(b'1') if t.len() == 1 => Ok(true),
        Some(b'0') if t.len() == 1 => Ok(false),
        _ => Err(syntax("a boolean")),
    }
}

/// `byteain`.
fn bytea(text: &str) -> Result<Vec<u8>, ValueRefusal> {
    let bytes = text.as_bytes();
    if let Some(hex) = text.strip_prefix("\\x") {
        let mut out = Vec::with_capacity(hex.len() / 2);
        let mut chars = hex.bytes().peekable();
        while let Some(c) = chars.next() {
            if matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
                continue;
            }
            let high = char::from(c).to_digit(16);
            let low = chars.next().and_then(|l| char::from(l).to_digit(16));
            match (high, low) {
                (Some(h), Some(l)) => out.push(u8::try_from(h * 16 + l).unwrap_or(0)),
                _ => return Err(syntax("bytea hex digits in pairs")),
            }
        }
        return Ok(out);
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        if bytes.get(i + 1) == Some(&b'\\') {
            out.push(b'\\');
            i += 2;
            continue;
        }
        let octal = bytes.get(i + 1..i + 4).filter(|d| {
            (b'0'..=b'3').contains(&d[0])
                && (b'0'..=b'7').contains(&d[1])
                && (b'0'..=b'7').contains(&d[2])
        });
        let Some(d) = octal else {
            return Err(syntax("a bytea escape: \\\\ or \\ and three octal digits"));
        };
        out.push((d[0] - b'0') * 64 + (d[1] - b'0') * 8 + (d[2] - b'0'));
        i += 4;
    }
    Ok(out)
}

/// The offset a temporal value carries.
fn offset_of(value: &Temporal) -> Offset {
    match value {
        Temporal::Date(_) => Offset::Local,
        Temporal::Time(_, offset) | Temporal::DateTime(_, _, offset, _) => *offset,
        Temporal::Extended(extended) => extended.offset,
    }
}

/// `date_in`, `time_in`, `timestamp_in` and `timestamptz_in`, on their
/// ISO 8601 forms.
fn temporal(text: &str, element: &Element) -> Result<Temporal, ValueRefusal> {
    let t = trim(text);
    let invalid = || ValueRefusal::Fidelity(FidelityReason::InvalidTemporal);
    let infinity = match t {
        "infinity" | "+infinity" => Some(TemporalInfinity::Positive),
        "-infinity" => Some(TemporalInfinity::Negative),
        _ => None,
    };
    if let Some(sign) = infinity {
        let form = match element {
            Element::Date => TemporalForm::Date,
            Element::Timestamp(_) => TemporalForm::DateTime,
            _ => return Err(invalid()),
        };
        return ExtendedTemporal::infinite(sign, form, Offset::Local)
            .map(Temporal::Extended)
            .ok_or_else(invalid);
    }
    let value = match element {
        Element::Date => Temporal::parse_date(t),
        Element::Time => Temporal::parse_time(t),
        _ => Temporal::parse_date_time(t),
    }
    .map_err(|_| invalid())?;
    if !value.is_complete() {
        return Err(invalid());
    }
    // Microseconds are the finest unit; a finer fraction would be rounded.
    let fraction = t.split_once('.').map_or(0, |(_, rest)| {
        rest.bytes().take_while(u8::is_ascii_digit).count()
    });
    if fraction > 6 {
        return Err(ValueRefusal::Rounded);
    }
    let leap = match &value {
        Temporal::Time(time, _) | Temporal::DateTime(_, time, _, _) => time.second == 60,
        Temporal::Extended(extended) => {
            extended.time.as_ref().is_some_and(|f| f.second == Some(60))
        }
        Temporal::Date(_) => false,
    };
    if leap {
        return Err(ValueRefusal::Rounded);
    }
    let has_offset = offset_of(&value) != Offset::Local;
    match element {
        Element::Timestamp(true) if !has_offset => {
            Err(ValueRefusal::Fidelity(FidelityReason::OffsetRequired))
        }
        Element::Date | Element::Time | Element::Timestamp(false) if has_offset => {
            Err(ValueRefusal::Fidelity(FidelityReason::OffsetForbidden))
        }
        _ => Ok(value),
    }
}

/// `uuid_in`: the canonical lowercase form.
fn uuid(text: &str) -> Result<String, ValueRefusal> {
    let not_uuid = || syntax("a UUID");
    let body = match text.strip_prefix('{') {
        Some(rest) => rest.strip_suffix('}').ok_or_else(not_uuid)?,
        None => text,
    };
    let mut hex = String::with_capacity(32);
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if !c.is_ascii_hexdigit() {
            return Err(not_uuid());
        }
        hex.push(c.to_ascii_lowercase());
        if hex.len() % 4 == 0 && hex.len() < 32 && chars.peek() == Some(&'-') {
            chars.next();
        }
    }
    if hex.len() != 32 {
        return Err(not_uuid());
    }
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

/// An array literal: a list of elements or of sub-arrays.
enum ArrayText {
    Items(Vec<Option<String>>),
    Arrays(Vec<ArrayText>),
}

impl ArrayText {
    fn depth(&self) -> usize {
        match self {
            Self::Items(items) if items.is_empty() => 0,
            Self::Items(_) => 1,
            Self::Arrays(arrays) => 1 + arrays.first().map_or(0, Self::depth),
        }
    }

    fn into_value(self, element: &Element) -> Result<Value, ValueRefusal> {
        match self {
            Self::Items(items) => items
                .into_iter()
                .map(|item| item.map_or(Ok(Value::Null), |t| decode_element(&t, element)))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List),
            Self::Arrays(arrays) => arrays
                .into_iter()
                .map(|a| a.into_value(element))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List),
        }
    }

    /// The shape: the length at each level, when every sub-array of a
    /// level has one length.
    fn shape(&self) -> Option<Vec<usize>> {
        match self {
            Self::Items(items) => Some(vec![items.len()]),
            Self::Arrays(arrays) => {
                let first = arrays.first()?.shape()?;
                for a in arrays {
                    if a.shape()? != first {
                        return None;
                    }
                }
                let mut shape = vec![arrays.len()];
                shape.extend(first);
                Some(shape)
            }
        }
    }
}

/// `array_in`'s text, before its elements are read.
fn parse_array(text: &str) -> Result<ArrayText, ValueRefusal> {
    let not_array = || syntax("an array literal");
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    while chars.get(at).copied().is_some_and(is_space) {
        at += 1;
    }
    if chars.get(at) == Some(&'[') {
        return Err(ValueRefusal::Unsupported("an array with explicit bounds"));
    }
    let array = array_level(&chars, &mut at).ok_or_else(not_array)?;
    while chars.get(at).copied().is_some_and(is_space) {
        at += 1;
    }
    if at != chars.len() {
        return Err(not_array());
    }
    if let ArrayText::Arrays(arrays) = &array
        && arrays
            .iter()
            .any(|a| matches!(a, ArrayText::Items(i) if i.is_empty()))
    {
        return Err(not_array());
    }
    array.shape().ok_or(ValueRefusal::Syntax {
        expected: "sub-arrays of one length",
    })?;
    Ok(array)
}

/// One `{...}` level at `at`, which is advanced past it.
fn array_level(chars: &[char], at: &mut usize) -> Option<ArrayText> {
    if chars.get(*at) != Some(&'{') {
        return None;
    }
    *at += 1;
    let skip = |at: &mut usize| {
        while chars.get(*at).copied().is_some_and(is_space) {
            *at += 1;
        }
    };
    skip(at);
    if chars.get(*at) == Some(&'}') {
        *at += 1;
        return Some(ArrayText::Items(Vec::new()));
    }
    if chars.get(*at) == Some(&'{') {
        let mut arrays = Vec::new();
        loop {
            skip(at);
            arrays.push(array_level(chars, at)?);
            skip(at);
            match chars.get(*at) {
                Some(',') => *at += 1,
                Some('}') => {
                    *at += 1;
                    return Some(ArrayText::Arrays(arrays));
                }
                _ => return None,
            }
        }
    }
    let mut items = Vec::new();
    loop {
        skip(at);
        let item = if chars.get(*at) == Some(&'"') {
            *at += 1;
            let mut value = String::new();
            loop {
                match chars.get(*at)? {
                    '"' => {
                        *at += 1;
                        break;
                    }
                    '\\' => {
                        value.push(*chars.get(*at + 1)?);
                        *at += 2;
                    }
                    c => {
                        value.push(*c);
                        *at += 1;
                    }
                }
            }
            skip(at);
            Some(value)
        } else {
            let mut value = String::new();
            let mut escaped = false;
            // The length of `value` up to its last escaped character, which
            // trimming keeps.
            let mut kept = 0;
            loop {
                match chars.get(*at)? {
                    ',' | '}' => break,
                    '{' | '"' => return None,
                    '\\' => {
                        value.push(*chars.get(*at + 1)?);
                        *at += 2;
                        escaped = true;
                        kept = value.len();
                    }
                    c => {
                        value.push(*c);
                        *at += 1;
                    }
                }
            }
            let trimmed_len = value.trim_end_matches(is_space).len().max(kept);
            value.truncate(trimmed_len);
            if value.is_empty() {
                return None;
            }
            if !escaped && value.eq_ignore_ascii_case("null") {
                None
            } else {
                Some(value)
            }
        };
        items.push(item);
        match chars.get(*at) {
            Some(',') => *at += 1,
            Some('}') => {
                *at += 1;
                return Some(ArrayText::Items(items));
            }
            _ => return None,
        }
    }
}

/// The output text of a value of a column of type `ty`, which [`decode`]
/// reads back as the same value.
pub(crate) fn encode(value: &Value, ty: &ColumnType) -> Result<String, ValueRefusal> {
    let text = if ty.dims == 0 {
        encode_element(value, &ty.element)?
    } else {
        encode_array(value, &ty.element, ty.dims)?
    };
    let back = decode(&text, ty)?;
    if same(&back, value) {
        Ok(text)
    } else {
        Err(ValueRefusal::NotOfType {
            expected: ty.element.name(),
            found: value.type_name(),
        })
    }
}

/// Whether two values are the same, floats compared by their bits.
pub(crate) fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
        (Value::List(xs), Value::List(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| same(x, y))
        }
        _ => a == b,
    }
}

fn not_of_type(value: &Value, element: &Element) -> ValueRefusal {
    ValueRefusal::NotOfType {
        expected: element.name(),
        found: value.type_name(),
    }
}

/// A finite decimal in positional notation.
fn plain_decimal(decimal: &DecimalValue) -> String {
    match decimal.class {
        DecimalClass::NaN { .. } => return "NaN".to_owned(),
        DecimalClass::Infinite => {
            return if decimal.negative {
                "-Infinity".to_owned()
            } else {
                "Infinity".to_owned()
            };
        }
        DecimalClass::Finite => {}
    }
    let sign = if decimal.negative { "-" } else { "" };
    let scale = decimal
        .exponent
        .negated()
        .to_i64()
        .and_then(|s| usize::try_from(s).ok())
        .unwrap_or(0);
    let digits = &decimal.digits;
    if scale == 0 {
        return format!("{sign}{digits}");
    }
    let padded = if digits.len() <= scale {
        format!("{}{digits}", "0".repeat(scale + 1 - digits.len()))
    } else {
        digits.clone()
    };
    let point = padded.len() - scale;
    format!("{sign}{}.{}", &padded[..point], &padded[point..])
}

fn encode_element(value: &Value, element: &Element) -> Result<String, ValueRefusal> {
    let wrong = || not_of_type(value, element);
    match (element, value) {
        (Element::Int(_), Value::Int(n)) => Ok(n.to_string()),
        (Element::Numeric(_), Value::Decimal(d)) => Ok(plain_decimal(d)),
        (Element::Real | Element::Double, Value::Float(x)) => {
            if x.is_nan() {
                return Ok("NaN".to_owned());
            }
            if x.is_infinite() {
                return Ok(if *x > 0.0 { "Infinity" } else { "-Infinity" }.to_owned());
            }
            let shortest = if *element == Element::Real {
                binary32_bits(*x)
                    .and_then(|bits| DecimalValue::from_f32_shortest(f32::from_bits(bits)))
            } else {
                DecimalValue::from_f64_shortest(*x)
            };
            shortest.map(|d| d.to_string()).ok_or_else(wrong)
        }
        (
            Element::Text(_)
            | Element::Char(_)
            | Element::Uuid
            | Element::Json(_)
            | Element::Enum(_),
            Value::Str(s),
        ) => Ok(s.clone()),
        (Element::Bool, Value::Bool(b)) => Ok(if *b { "t" } else { "f" }.to_owned()),
        (Element::Bytea, Value::Bytes(bytes)) => {
            let mut out = String::with_capacity(2 + 2 * bytes.len());
            out.push_str("\\x");
            for b in bytes {
                let _ = write!(out, "{b:02x}");
            }
            Ok(out)
        }
        (Element::Date | Element::Time | Element::Timestamp(_), Value::Temporal(t)) => {
            Ok(t.to_string())
        }
        _ => Err(wrong()),
    }
}

fn encode_array(value: &Value, element: &Element, dims: usize) -> Result<String, ValueRefusal> {
    let Value::List(items) = value else {
        return Err(not_of_type(value, element));
    };
    let mut out = String::from("{");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        if dims > 1 {
            out.push_str(&encode_array(item, element, dims - 1)?);
            continue;
        }
        if *item == Value::Null {
            out.push_str("NULL");
            continue;
        }
        let text = encode_element(item, element)?;
        let quote = text.is_empty()
            || text.eq_ignore_ascii_case("null")
            || text
                .chars()
                .any(|c| matches!(c, '{' | '}' | ',' | '"' | '\\') || is_space(c));
        if quote {
            out.push('"');
            for c in text.chars() {
                if matches!(c, '"' | '\\') {
                    out.push('\\');
                }
                out.push(c);
            }
            out.push('"');
        } else {
            out.push_str(&text);
        }
    }
    out.push('}');
    Ok(out)
}
