//! Writing Extended JSON, canonical and relaxed.

use std::fmt::Write as _;

use panproto_expr::exact::{CivilDate, DecimalClass, DecimalValue};
use panproto_schema::BytesText;

use super::decimal128::{decimal128_string, representable};
use super::{
    BsonDocument, BsonValue, BsonWriteError, BsonWriteErrorKind, MAX_DEPTH, child_path,
    options_in_order, write_error,
};
use crate::json_pathway::encode_bytes;

/// Which Extended JSON format a writer writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Canonical Extended JSON: every value whose type JSON cannot tell
    /// apart is in its type wrapper, so the text reads back as the same
    /// document.
    Canonical,
    /// Relaxed Extended JSON: an int32 or int64 is a JSON integer, a
    /// finite double a JSON number with a fraction or an exponent, and a
    /// datetime from 1970 to 9999 an RFC 3339 string; everything else as in
    /// [`Mode::Canonical`]. An int64 within the int32 range reads back as an
    /// int32, so a relaxed text keeps the values and not every type.
    Relaxed,
}

/// The keys that make a JSON object other than a top-level document a type
/// wrapper.
pub const WRAPPER_KEYS: &[&str] = &[
    "$oid",
    "$symbol",
    "$numberInt",
    "$numberLong",
    "$numberDouble",
    "$numberDecimal",
    "$binary",
    "$code",
    "$scope",
    "$timestamp",
    "$regularExpression",
    "$dbPointer",
    "$date",
    "$minKey",
    "$maxKey",
    "$undefined",
    "$uuid",
];

/// The bits of the double `"NaN"` reads as.
pub const CANONICAL_NAN_BITS: u64 = 0x7FF8_0000_0000_0000;

/// The decimal string of a finite double in the form the writers use.
///
/// It is the shortest digits that read back as the double, in positional
/// notation with at least one fractional digit (`1.0`, `-0.0`, `0.001`)
/// when the decimal exponent of the first digit is from -4 to 15, and in
/// exponential notation otherwise (`1.2345678921232E+18`, `1E-5`), with
/// `E`, a sign and no leading zeros in the exponent. `None` for an
/// infinity or a NaN.
#[must_use]
pub fn double_string(x: f64) -> Option<String> {
    let decimal = DecimalValue::from_f64_shortest(x)?;
    let sign = if decimal.negative { "-" } else { "" };
    let mut digits = decimal.digits.as_str();
    let mut exponent = decimal.exponent.to_i64()?;
    if digits != "0" {
        let trimmed = digits.trim_end_matches('0');
        exponent += i64::try_from(digits.len() - trimmed.len()).ok()?;
        digits = trimmed;
    }
    let length = i64::try_from(digits.len()).ok()?;
    let adjusted = exponent + length - 1;
    if (-4..16).contains(&adjusted) {
        if exponent >= 0 {
            let zeros = "0".repeat(usize::try_from(exponent).ok()?);
            return Some(format!("{sign}{digits}{zeros}.0"));
        }
        let scale = usize::try_from(-exponent).ok()?;
        return Some(if scale >= digits.len() {
            format!("{sign}0.{}{digits}", "0".repeat(scale - digits.len()))
        } else {
            let (whole, fraction) = digits.split_at(digits.len() - scale);
            format!("{sign}{whole}.{fraction}")
        });
    }
    let (first, rest) = digits.split_at(1);
    let point = if rest.is_empty() { "" } else { "." };
    let exponent_sign = if adjusted < 0 { '-' } else { '+' };
    Some(format!(
        "{sign}{first}{point}{rest}E{exponent_sign}{}",
        adjusted.unsigned_abs()
    ))
}

/// The lowest datetime, in milliseconds, that relaxed Extended JSON
/// writes as a string (1970-01-01T00:00:00Z).
const RELAXED_DATE_MIN: i64 = 0;
/// The first datetime, in milliseconds, after those relaxed Extended JSON
/// writes as a string (10000-01-01T00:00:00Z).
const RELAXED_DATE_END: i64 = 253_402_300_800_000;

/// The RFC 3339 string of a datetime from 1970 to 9999: the date, `T`, the
/// time, three digits of milliseconds after a point unless they are zero,
/// and `Z`.
fn relaxed_date(ms: i64) -> Option<String> {
    let days = ms.div_euclid(86_400_000);
    let within = ms.rem_euclid(86_400_000);
    let date = CivilDate::from_days_since_epoch(i128::from(days))?;
    let (hour, minute, second, milli) = (
        within / 3_600_000,
        within / 60_000 % 60,
        within / 1000 % 60,
        within % 1000,
    );
    let mut text = format!(
        "{:04}-{:02}-{:02}T{hour:02}:{minute:02}:{second:02}",
        date.year, date.month, date.day
    );
    if milli != 0 {
        let _ = write!(text, ".{milli:03}");
    }
    text.push('Z');
    Some(text)
}

/// Write a document as Extended JSON in `mode`.
///
/// The text is one JSON value with no whitespace between tokens and no line ending, each document's members in
/// element order, each string escaped as `serde_json` escapes it, an object
/// id's hexadecimal and a binary subtype's two hexadecimal digits in lower
/// case, base64 with padding, a double by [`double_string`], a decimal128 by
/// [`decimal128_string`], and the type wrappers' members in the order the
/// specification's conversion table gives them.
///
/// # Errors
///
/// A [`BsonWriteError`] at the first value, in document order, that no
/// Extended JSON text reads back as: those [`encode`](super::encode)
/// refuses; a document with two elements of one name, which the shared
/// JSON route refuses to read; an embedded document holding a key of a type
/// wrapper ([`WRAPPER_KEYS`]), which would read back as that wrapper or be
/// refused; and a NaN other than the one `NaN` reads as (a double NaN with
/// other bits than [`CANONICAL_NAN_BITS`], a decimal128 NaN with a sign, a
/// signal or a payload).
pub fn write_extended_json(document: &BsonDocument, mode: Mode) -> Result<String, BsonWriteError> {
    let mut out = String::new();
    Writer { mode }.document(&mut out, document, "", 1, true)?;
    Ok(out)
}

/// A writer in one mode.
#[derive(Clone, Copy)]
struct Writer {
    mode: Mode,
}

impl Writer {
    fn document(
        self,
        out: &mut String,
        document: &BsonDocument,
        path: &str,
        depth: usize,
        top: bool,
    ) -> Result<(), BsonWriteError> {
        if depth > MAX_DEPTH {
            return Err(write_error(path.to_owned(), BsonWriteErrorKind::Depth));
        }
        out.push('{');
        for (i, (key, value)) in document.0.iter().enumerate() {
            let at = child_path(path, key);
            if key.contains('\0') {
                return Err(write_error(at, BsonWriteErrorKind::NulInKey));
            }
            if document.0[..i].iter().any(|(earlier, _)| earlier == key) {
                return Err(write_error(
                    at,
                    BsonWriteErrorKind::DuplicateKey(key.clone()),
                ));
            }
            if !top && WRAPPER_KEYS.contains(&key.as_str()) {
                return Err(write_error(at, BsonWriteErrorKind::WrapperKey(key.clone())));
            }
            if i > 0 {
                out.push(',');
            }
            out.push_str(&json_string(key));
            out.push(':');
            self.value(out, value, &at, depth)?;
        }
        out.push('}');
        Ok(())
    }

    fn value(
        self,
        out: &mut String,
        value: &BsonValue,
        path: &str,
        depth: usize,
    ) -> Result<(), BsonWriteError> {
        let fail = |kind| Err(write_error(path.to_owned(), kind));
        let relaxed = self.mode == Mode::Relaxed;
        match value {
            BsonValue::Double(x) => {
                if x.is_nan() && x.to_bits() != CANONICAL_NAN_BITS {
                    return fail(BsonWriteErrorKind::NaN);
                }
                self.double(out, *x);
            }
            BsonValue::String(text) => out.push_str(&json_string(text)),
            BsonValue::Document(document) => {
                self.document(out, document, path, depth + 1, false)?;
            }
            BsonValue::Array(items) => {
                if depth + 1 > MAX_DEPTH {
                    return fail(BsonWriteErrorKind::Depth);
                }
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    self.value(out, item, &child_path(path, &i.to_string()), depth + 1)?;
                }
                out.push(']');
            }
            BsonValue::Binary { subtype, bytes } => {
                let base64 = encode_bytes(bytes, &BytesText::Base64Padded).unwrap_or_default();
                let inner = format!(
                    "{{\"base64\":{},\"subType\":\"{subtype:02x}\"}}",
                    json_string(&base64)
                );
                wrap(out, "$binary", &inner);
            }
            BsonValue::Undefined => wrap(out, "$undefined", "true"),
            BsonValue::ObjectId(id) => wrap(out, "$oid", &hex_string(id)),
            BsonValue::Boolean(flag) => out.push_str(if *flag { "true" } else { "false" }),
            BsonValue::DateTime(ms) => match relaxed_date(*ms) {
                Some(text) if relaxed && (RELAXED_DATE_MIN..RELAXED_DATE_END).contains(ms) => {
                    wrap(out, "$date", &json_string(&text));
                }
                _ => wrap(out, "$date", &format!("{{\"$numberLong\":\"{ms}\"}}")),
            },
            BsonValue::Null => out.push_str("null"),
            BsonValue::Regex { pattern, options } => {
                regex(out, pattern, options).map_err(|kind| write_error(path.to_owned(), kind))?;
            }
            BsonValue::DbPointer { namespace, id } => {
                let inner = format!(
                    "{{\"$ref\":{},\"$id\":{{\"$oid\":{}}}}}",
                    json_string(namespace),
                    hex_string(id)
                );
                wrap(out, "$dbPointer", &inner);
            }
            BsonValue::Code(code) => wrap(out, "$code", &json_string(code)),
            BsonValue::Symbol(symbol) => wrap(out, "$symbol", &json_string(symbol)),
            BsonValue::CodeWithScope { code, scope } => {
                out.push_str("{\"$code\":");
                out.push_str(&json_string(code));
                out.push_str(",\"$scope\":");
                self.document(out, scope, &child_path(path, "$scope"), depth + 1, true)?;
                out.push('}');
            }
            BsonValue::Int32(n) if relaxed => {
                let _ = write!(out, "{n}");
            }
            BsonValue::Int64(n) if relaxed => {
                let _ = write!(out, "{n}");
            }
            BsonValue::Int32(n) => wrap(out, "$numberInt", &format!("\"{n}\"")),
            BsonValue::Int64(n) => wrap(out, "$numberLong", &format!("\"{n}\"")),
            BsonValue::Timestamp { time, increment } => {
                wrap(
                    out,
                    "$timestamp",
                    &format!("{{\"t\":{time},\"i\":{increment}}}"),
                );
            }
            BsonValue::Decimal128(value) => {
                decimal(out, value).map_err(|kind| write_error(path.to_owned(), kind))?;
            }
            BsonValue::MinKey => wrap(out, "$minKey", "1"),
            BsonValue::MaxKey => wrap(out, "$maxKey", "1"),
        }
        Ok(())
    }
}

impl Writer {
    /// Append a double: a JSON number in relaxed mode when it is finite, and
    /// its `$numberDouble` wrapper otherwise.
    fn double(self, out: &mut String, x: f64) {
        match double_string(x) {
            Some(text) if self.mode == Mode::Relaxed => out.push_str(&text),
            Some(text) => wrap(out, "$numberDouble", &json_string(&text)),
            None if x.is_nan() => wrap(out, "$numberDouble", "\"NaN\""),
            None if x > 0.0 => wrap(out, "$numberDouble", "\"Infinity\""),
            None => wrap(out, "$numberDouble", "\"-Infinity\""),
        }
    }
}

/// Append a regular expression's wrapper, or refuse it.
fn regex(out: &mut String, pattern: &str, options: &str) -> Result<(), BsonWriteErrorKind> {
    if pattern.contains('\0') || options.contains('\0') {
        return Err(BsonWriteErrorKind::NulInRegex);
    }
    if !options_in_order(options) {
        return Err(BsonWriteErrorKind::RegexOptions);
    }
    let inner = format!(
        "{{\"pattern\":{},\"options\":{}}}",
        json_string(pattern),
        json_string(options)
    );
    wrap(out, "$regularExpression", &inner);
    Ok(())
}

/// Append a decimal128's wrapper, or refuse it.
fn decimal(out: &mut String, value: &DecimalValue) -> Result<(), BsonWriteErrorKind> {
    if !representable(value) {
        return Err(BsonWriteErrorKind::Decimal128(value.to_string()));
    }
    if let DecimalClass::NaN {
        signalling,
        payload,
    } = &value.class
        && (value.negative || *signalling || payload != "0")
    {
        return Err(BsonWriteErrorKind::NaN);
    }
    wrap(
        out,
        "$numberDecimal",
        &json_string(&decimal128_string(value)),
    );
    Ok(())
}

/// `text` as a JSON string literal, escaped as `serde_json` escapes it.
fn json_string(text: &str) -> String {
    serde_json::Value::String(text.to_owned()).to_string()
}

/// Append the type wrapper `{"<key>":<inner>}`.
fn wrap(out: &mut String, key: &str, inner: &str) {
    out.push_str("{\"");
    out.push_str(key);
    out.push_str("\":");
    out.push_str(inner);
    out.push('}');
}

/// The JSON string of `bytes` in lower-case hexadecimal.
fn hex_string(bytes: &[u8]) -> String {
    format!(
        "\"{}\"",
        encode_bytes(bytes, &BytesText::Hex).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_are_written_in_the_canonical_decimal_form() {
        for (x, text) in [
            (1.0, "1.0"),
            (-0.0, "-0.0"),
            (0.0, "0.0"),
            (1.000_122_070_312_5, "1.0001220703125"),
            (1.234_567_892_123_2e18, "1.2345678921232E+18"),
            (1e16, "1E+16"),
            (123_456.0, "123456.0"),
            (0.0001, "0.0001"),
            (0.00001, "1E-5"),
            (5e-324, "5E-324"),
        ] {
            assert_eq!(double_string(x).as_deref(), Some(text), "{x}");
        }
    }

    #[test]
    fn relaxed_dates_omit_zero_milliseconds() {
        assert_eq!(relaxed_date(0).as_deref(), Some("1970-01-01T00:00:00Z"));
        assert_eq!(
            relaxed_date(1_356_351_330_501).as_deref(),
            Some("2012-12-24T12:15:30.501Z")
        );
    }
}
