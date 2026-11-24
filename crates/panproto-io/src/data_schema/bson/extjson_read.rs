//! Reading Extended JSON, canonical or relaxed, through the shared JSON
//! route.

use std::cmp::Ordering;

use panproto_expr::exact::{DateTimeSeparator, DecimalClass, DecimalValue, Offset, Temporal};
use panproto_inst::Value;
use panproto_inst::value::coerce::{LexClass, Lexeme, coerce};
use panproto_schema::{BytesText, ScalarKind, ValueSpec};

use super::decimal128::{Decimal128ParseError, parse_decimal128};
use super::extjson_write::WRAPPER_KEYS;
use super::{BsonDocument, BsonValue, MAX_DEPTH, child_path, options_in_order};
use crate::config::tree::{Content, Node, read_json};

/// A refused Extended JSON document: the place at fault and why.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{} (byte {offset}): {kind}", if path.is_empty() { "the root" } else { path.as_str() })]
pub struct ExtJsonError {
    /// The JSON pointer of the value at fault from the document root,
    /// empty for the root.
    pub path: String,
    /// The byte offset at which the value starts.
    pub offset: usize,
    /// Why the document is refused.
    pub kind: ExtJsonErrorKind,
}

/// Why [`read_extended_json`] refuses a document.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ExtJsonErrorKind {
    /// The shared JSON route refuses the text.
    #[error("the JSON route refuses the document: {0}")]
    Json(String),
    /// The top-level value is not an object.
    #[error("the top-level value is not an object")]
    NotADocument,
    /// A key holds U+0000, which no BSON key can.
    #[error("the key holds U+0000")]
    NulInKey,
    /// An object holding a type wrapper's key whose keys are not exactly
    /// those of a type wrapper.
    #[error("an object with the key {0} must have exactly the keys of its type wrapper")]
    WrapperKeys(&'static str),
    /// A type wrapper whose value is of another JSON kind or form than the
    /// wrapper takes.
    #[error("the value of {0} is not of the form its type wrapper takes")]
    WrapperValue(&'static str),
    /// A regular expression's pattern or options hold U+0000.
    #[error("the regular expression holds U+0000")]
    NulInRegex,
    /// Regular expression options not in alphabetical order.
    #[error("the regular expression options are not in alphabetical order")]
    RegexOptions,
    /// A `$numberDecimal` string that decimal128 holds only by clamping or
    /// rounding, or not at all.
    #[error("{0:?} is not a decimal128 value as written")]
    DecimalRepresentation(String),
    /// A JSON integer outside the int64 range.
    #[error("the integer is outside the int64 range")]
    IntegerRange,
    /// A JSON number or `$numberDouble` string with a fraction or an
    /// exponent whose decimal value is not that of the double nearest it,
    /// written in its shortest form.
    #[error("no double holds the number as written")]
    InexactNumber,
    /// Nesting deeper than [`MAX_DEPTH`].
    #[error("the nesting is deeper than {MAX_DEPTH}")]
    Depth,
}

impl ExtJsonErrorKind {
    /// The refusal's class, the name the conformance limits use.
    #[must_use]
    pub const fn class(&self) -> &'static str {
        match self {
            Self::Json(_) => "json",
            Self::NotADocument => "not-a-document",
            Self::NulInKey => "nul-in-key",
            Self::WrapperKeys(_) => "wrapper-keys",
            Self::WrapperValue(_) => "wrapper-value",
            Self::NulInRegex => "nul-in-regex",
            Self::RegexOptions => "regex-options",
            Self::DecimalRepresentation(_) => "decimal-representation",
            Self::IntegerRange => "integer-range",
            Self::InexactNumber => "inexact-number",
            Self::Depth => "depth",
        }
    }
}

/// The refusal of `node` at `path` with `kind`.
fn fault(path: &str, node: &Node, kind: ExtJsonErrorKind) -> ExtJsonError {
    ExtJsonError {
        path: path.to_owned(),
        offset: node.offset,
        kind,
    }
}

type Read<T> = Result<T, ExtJsonError>;

/// Read an Extended JSON document, canonical or relaxed, through the
/// shared JSON route.
///
/// # Errors
///
/// An [`ExtJsonError`]: the route's refusal (at the root, as
/// [`ExtJsonErrorKind::Json`]) or [`read_extended_json_tree`]'s.
pub fn read_extended_json(source: &[u8]) -> Read<BsonDocument> {
    let tree = read_json(source).map_err(|error| ExtJsonError {
        path: String::new(),
        offset: 0,
        kind: ExtJsonErrorKind::Json(error.to_string()),
    })?;
    read_extended_json_tree(&tree)
}

/// Read the tree of an Extended JSON document.
///
/// The top-level value must be an object, and is a document whatever its
/// keys. Any other object holding a key of [`WRAPPER_KEYS`] is a type
/// wrapper and must have exactly the keys of one, in any order: `$oid` (24
/// hexadecimal digits, either case), `$symbol`, `$numberInt` and
/// `$numberLong` (a string `-?(0|[1-9][0-9]*)` in range), `$numberDouble`
/// (`Infinity`, `-Infinity`, `NaN`, or a decimal string by the rule below),
/// `$numberDecimal` (by [`parse_decimal128`]), `$binary` (an object of
/// exactly `base64`, padded base64, and `subType`, one or two hexadecimal
/// digits), `$code`, `$code` with `$scope` (an object read as a document),
/// `$timestamp` (an object of exactly `t` and `i`, integers from 0 to
/// `2^32 - 1`), `$regularExpression` (an object of exactly `pattern` and
/// `options`, strings with no U+0000, the options in alphabetical order),
/// `$dbPointer` (an object of exactly `$ref`, a string, and `$id`, an
/// `$oid` wrapper), `$date` (a `$numberLong` wrapper, or an RFC 3339
/// date-time string with a four-digit year, at most three fractional
/// digits, no leap second and an offset `Z` or `±HH:MM`), `$minKey` and
/// `$maxKey` (the integer 1), `$undefined` (`true`) and `$uuid` (a UUID's
/// 8-4-4-4-12 hexadecimal form, read as a subtype 4 binary). Every other
/// object is a document, whatever `$` keys it has (a `DBRef`, `$regex`,
/// `$type`). A string is a string, a boolean, null and an array themselves;
/// a JSON integer is an int32 when it fits and an int64 otherwise, and one
/// outside the int64 range is refused rather than read as a double; a JSON
/// number with a fraction or an exponent is a double, and so is a
/// `$numberDouble` decimal string, when the shortest decimal form of the
/// double nearest it has the same value (`0.1` and `1.50` read; a number
/// with more digits than its double keeps, or out of the double range, is
/// refused rather than rounded).
///
/// # Errors
///
/// The [`ExtJsonError`] of the first value at fault, in document order.
pub fn read_extended_json_tree(tree: &Node) -> Read<BsonDocument> {
    match &tree.content {
        Content::Mapping(entries) => document(entries, "", 1),
        _ => Err(fault("", tree, ExtJsonErrorKind::NotADocument)),
    }
}

fn document(entries: &[(String, Node)], path: &str, depth: usize) -> Read<BsonDocument> {
    let mut elements = Vec::with_capacity(entries.len());
    for (key, node) in entries {
        let at = child_path(path, key);
        if depth > MAX_DEPTH {
            return Err(fault(&at, node, ExtJsonErrorKind::Depth));
        }
        if key.contains('\0') {
            return Err(fault(&at, node, ExtJsonErrorKind::NulInKey));
        }
        elements.push((key.clone(), value(node, &at, depth)?));
    }
    Ok(BsonDocument(elements))
}

fn value(node: &Node, path: &str, depth: usize) -> Read<BsonValue> {
    match &node.content {
        Content::Scalar(scalar) => scalar_value(scalar, node, path),
        Content::Sequence(items) => {
            if depth + 1 > MAX_DEPTH {
                return Err(fault(path, node, ExtJsonErrorKind::Depth));
            }
            items
                .iter()
                .enumerate()
                .map(|(i, item)| value(item, &child_path(path, &i.to_string()), depth + 1))
                .collect::<Read<Vec<_>>>()
                .map(BsonValue::Array)
        }
        Content::Mapping(entries) => {
            let key = entries
                .iter()
                .find_map(|(key, _)| WRAPPER_KEYS.iter().copied().find(|k| k == key));
            key.map_or_else(
                || document(entries, path, depth + 1).map(BsonValue::Document),
                |key| wrapper(key, entries, node, path, depth),
            )
        }
    }
}

fn scalar_value(scalar: &Value, node: &Node, path: &str) -> Read<BsonValue> {
    Ok(match scalar {
        Value::Null => BsonValue::Null,
        Value::Bool(flag) => BsonValue::Boolean(*flag),
        Value::Str(text) => BsonValue::String(text.clone()),
        Value::Int(n) => i32::try_from(*n).map_or(BsonValue::Int64(*n), BsonValue::Int32),
        Value::Float(x) => BsonValue::Double(*x),
        Value::Decimal(decimal) => BsonValue::Double(
            exact_double(decimal)
                .ok_or_else(|| fault(path, node, ExtJsonErrorKind::InexactNumber))?,
        ),
        Value::BigInt(_) => return Err(fault(path, node, ExtJsonErrorKind::IntegerRange)),
        other => {
            return Err(fault(
                path,
                node,
                ExtJsonErrorKind::Json(format!("the route read a {}", other.type_name())),
            ));
        }
    })
}

/// The double nearest a finite decimal, when the shortest decimal form of
/// that double has the decimal's value.
fn exact_double(decimal: &DecimalValue) -> Option<f64> {
    if !decimal.is_finite() {
        return None;
    }
    let nearest: f64 = decimal.to_string().parse().ok()?;
    let shortest = DecimalValue::from_f64_shortest(nearest)?;
    (shortest.cmp_numeric(decimal) == Ordering::Equal).then_some(nearest)
}

/// The members of an object, in source order.
type Members<'n> = &'n [(String, Node)];

/// The member `key` of `members`.
fn member<'n>(members: Members<'n>, key: &str) -> Option<&'n Node> {
    members.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Whether `members` has exactly the keys `keys`, in any order.
fn has_keys(members: Members<'_>, keys: &[&str]) -> bool {
    members.len() == keys.len() && keys.iter().all(|key| member(members, key).is_some())
}

/// The string of `node`, or the refusal of `wrapper`'s value.
fn string<'n>(node: &'n Node, path: &str, wrapper: &'static str) -> Read<&'n str> {
    node.string()
        .ok_or_else(|| fault(path, node, ExtJsonErrorKind::WrapperValue(wrapper)))
}

/// The members of the object `node`, or the refusal of `wrapper`'s value.
fn object<'n>(node: &'n Node, path: &str, wrapper: &'static str) -> Read<Members<'n>> {
    node.mapping()
        .ok_or_else(|| fault(path, node, ExtJsonErrorKind::WrapperValue(wrapper)))
}

/// The type wrapper `members`, of which `key` is the first wrapper key in
/// source order.
fn wrapper(
    key: &'static str,
    members: Members<'_>,
    node: &Node,
    path: &str,
    depth: usize,
) -> Read<BsonValue> {
    let keys_fault = || fault(path, node, ExtJsonErrorKind::WrapperKeys(key));
    if has_keys(members, &["$code", "$scope"]) {
        let code_at = child_path(path, "$code");
        let scope_at = child_path(path, "$scope");
        let code = string(required(members, "$code", keys_fault)?, &code_at, "$code")?;
        let scope_node = required(members, "$scope", keys_fault)?;
        let scope = object(scope_node, &scope_at, "$scope")?;
        if depth + 1 > MAX_DEPTH {
            return Err(fault(&scope_at, scope_node, ExtJsonErrorKind::Depth));
        }
        return Ok(BsonValue::CodeWithScope {
            code: code.to_owned(),
            scope: document(scope, &scope_at, depth + 1)?,
        });
    }
    if members.len() != 1 || key == "$scope" {
        return Err(keys_fault());
    }
    let inner = &members[0].1;
    let at = child_path(path, key);
    let value_fault = || fault(&at, inner, ExtJsonErrorKind::WrapperValue(key));
    Ok(match key {
        "$oid" => BsonValue::ObjectId(object_id(string(inner, &at, key)?).ok_or_else(value_fault)?),
        "$symbol" => BsonValue::Symbol(string(inner, &at, key)?.to_owned()),
        "$code" => BsonValue::Code(string(inner, &at, key)?.to_owned()),
        "$numberInt" => BsonValue::Int32(
            integer_string(string(inner, &at, key)?)
                .and_then(|n| i32::try_from(n).ok())
                .ok_or_else(value_fault)?,
        ),
        "$numberLong" => {
            BsonValue::Int64(integer_string(string(inner, &at, key)?).ok_or_else(value_fault)?)
        }
        "$numberDouble" => BsonValue::Double(double(inner, &at)?),
        "$numberDecimal" => {
            BsonValue::Decimal128(parse_decimal128(string(inner, &at, key)?).map_err(|error| {
                match error {
                    Decimal128ParseError::Representation(text) => {
                        fault(&at, inner, ExtJsonErrorKind::DecimalRepresentation(text))
                    }
                    _ => value_fault(),
                }
            })?)
        }
        "$binary" => binary(inner, &at)?,
        "$timestamp" => timestamp(inner, &at)?,
        "$regularExpression" => regex(inner, &at)?,
        "$dbPointer" => db_pointer(inner, &at)?,
        "$date" => BsonValue::DateTime(date(inner, &at)?),
        "$minKey" | "$maxKey" | "$undefined" => match (&inner.content, key) {
            (Content::Scalar(Value::Int(1)), "$minKey") => BsonValue::MinKey,
            (Content::Scalar(Value::Int(1)), "$maxKey") => BsonValue::MaxKey,
            (Content::Scalar(Value::Bool(true)), "$undefined") => BsonValue::Undefined,
            _ => return Err(value_fault()),
        },
        _ => BsonValue::Binary {
            subtype: 4,
            bytes: uuid(string(inner, &at, key)?).ok_or_else(value_fault)?,
        },
    })
}

/// The member `key` of `members`, or `refusal`.
fn required<'n>(
    members: Members<'n>,
    key: &str,
    refusal: impl FnOnce() -> ExtJsonError,
) -> Read<&'n Node> {
    member(members, key).ok_or_else(refusal)
}

/// The integer a string `-?(0|[1-9][0-9]*)` writes, when it fits `i64`.
fn integer_string(text: &str) -> Option<i64> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let canonical = digits == "0"
        || (!digits.is_empty()
            && !digits.starts_with('0')
            && digits.bytes().all(|b| b.is_ascii_digit()));
    canonical.then(|| text.parse().ok()).flatten()
}

/// The bytes `text` writes in `encoding`, through the shared value
/// coercion.
fn bytes_of(text: &str, encoding: BytesText) -> Option<Vec<u8>> {
    match coerce(
        &Lexeme::new(text, LexClass::String),
        &ValueSpec::new(ScalarKind::Bytes(encoding)),
    ) {
        Ok(Value::Bytes(bytes)) => Some(bytes),
        _ => None,
    }
}

/// The twelve bytes of an object id's 24 hexadecimal digits.
fn object_id(text: &str) -> Option<[u8; 12]> {
    if text.len() != 24 {
        return None;
    }
    bytes_of(text, BytesText::Hex).and_then(|bytes| <[u8; 12]>::try_from(bytes).ok())
}

/// The sixteen bytes of a UUID's `8-4-4-4-12` hexadecimal form.
fn uuid(text: &str) -> Option<Vec<u8>> {
    let groups: Vec<&str> = text.split('-').collect();
    let lengths: Vec<usize> = groups.iter().map(|g| g.len()).collect();
    if lengths != [8, 4, 4, 4, 12] {
        return None;
    }
    bytes_of(&groups.concat(), BytesText::Hex)
}

fn double(node: &Node, path: &str) -> Read<f64> {
    let text = string(node, path, "$numberDouble")?;
    match text {
        "Infinity" => return Ok(f64::INFINITY),
        "-Infinity" => return Ok(f64::NEG_INFINITY),
        "NaN" => return Ok(f64::NAN),
        _ => {}
    }
    let decimal = DecimalValue::parse(text)
        .ok()
        .filter(|decimal| decimal.class == DecimalClass::Finite)
        .ok_or_else(|| fault(path, node, ExtJsonErrorKind::WrapperValue("$numberDouble")))?;
    exact_double(&decimal).ok_or_else(|| fault(path, node, ExtJsonErrorKind::InexactNumber))
}

fn binary(node: &Node, path: &str) -> Read<BsonValue> {
    let members = object(node, path, "$binary")?;
    let value_fault = || fault(path, node, ExtJsonErrorKind::WrapperValue("$binary"));
    if !has_keys(members, &["base64", "subType"]) {
        return Err(value_fault());
    }
    let bytes = required(members, "base64", value_fault)?
        .string()
        .and_then(|text| bytes_of(text, BytesText::Base64Padded))
        .ok_or_else(value_fault)?;
    let subtype = required(members, "subType", value_fault)?
        .string()
        .filter(|text| matches!(text.len(), 1 | 2) && text.bytes().all(|b| b.is_ascii_hexdigit()))
        .and_then(|text| u8::from_str_radix(text, 16).ok())
        .ok_or_else(value_fault)?;
    Ok(BsonValue::Binary { subtype, bytes })
}

fn timestamp(node: &Node, path: &str) -> Read<BsonValue> {
    let members = object(node, path, "$timestamp")?;
    let value_fault = || fault(path, node, ExtJsonErrorKind::WrapperValue("$timestamp"));
    if !has_keys(members, &["t", "i"]) {
        return Err(value_fault());
    }
    let part = |key: &str| match member(members, key).map(|node| &node.content) {
        Some(Content::Scalar(Value::Int(n))) => u32::try_from(*n).ok(),
        _ => None,
    };
    match (part("t"), part("i")) {
        (Some(time), Some(increment)) => Ok(BsonValue::Timestamp { time, increment }),
        _ => Err(value_fault()),
    }
}

fn regex(node: &Node, path: &str) -> Read<BsonValue> {
    let members = object(node, path, "$regularExpression")?;
    let value_fault = || {
        fault(
            path,
            node,
            ExtJsonErrorKind::WrapperValue("$regularExpression"),
        )
    };
    if !has_keys(members, &["pattern", "options"]) {
        return Err(value_fault());
    }
    let (Some(pattern), Some(options)) = (
        member(members, "pattern").and_then(Node::string),
        member(members, "options").and_then(Node::string),
    ) else {
        return Err(value_fault());
    };
    if pattern.contains('\0') || options.contains('\0') {
        return Err(fault(path, node, ExtJsonErrorKind::NulInRegex));
    }
    if !options_in_order(options) {
        return Err(fault(path, node, ExtJsonErrorKind::RegexOptions));
    }
    Ok(BsonValue::Regex {
        pattern: pattern.to_owned(),
        options: options.to_owned(),
    })
}

fn db_pointer(node: &Node, path: &str) -> Read<BsonValue> {
    let members = object(node, path, "$dbPointer")?;
    let value_fault = || fault(path, node, ExtJsonErrorKind::WrapperValue("$dbPointer"));
    if !has_keys(members, &["$ref", "$id"]) {
        return Err(value_fault());
    }
    let namespace = member(members, "$ref")
        .and_then(Node::string)
        .ok_or_else(value_fault)?;
    let id = member(members, "$id")
        .and_then(Node::mapping)
        .filter(|members| has_keys(members, &["$oid"]))
        .and_then(|members| members[0].1.string())
        .and_then(object_id)
        .ok_or_else(value_fault)?;
    Ok(BsonValue::DbPointer {
        namespace: namespace.to_owned(),
        id,
    })
}

fn date(node: &Node, path: &str) -> Read<i64> {
    let value_fault = || fault(path, node, ExtJsonErrorKind::WrapperValue("$date"));
    match &node.content {
        Content::Scalar(Value::Str(text)) => rfc3339_millis(text).ok_or_else(value_fault),
        Content::Mapping(members) if has_keys(members, &["$numberLong"]) => members[0]
            .1
            .string()
            .and_then(integer_string)
            .ok_or_else(value_fault),
        _ => Err(value_fault()),
    }
}

/// The milliseconds since the epoch of an RFC 3339 date-time with a
/// four-digit year, at most three fractional digits, no leap second and an
/// offset `Z`, `z` or `±HH:MM`.
fn rfc3339_millis(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 20 || !bytes[..4].iter().all(u8::is_ascii_digit) || bytes[4] != b'-' {
        return None;
    }
    let Ok(Temporal::DateTime(date, time, offset, separator)) = Temporal::parse_date_time(text)
    else {
        return None;
    };
    let rfc_offset = matches!(offset, Offset::Zulu(_) | Offset::Fixed(_) | Offset::NegZero);
    let rfc_separator = matches!(
        separator,
        DateTimeSeparator::Upper | DateTimeSeparator::Lower
    );
    let fraction = time.fraction.as_str();
    if !rfc_offset || !rfc_separator || time.hour > 23 || time.second > 59 || fraction.len() > 3 {
        return None;
    }
    let millis: i64 = format!("{fraction:0<3}").parse().ok()?;
    let days = i64::try_from(date.days_since_epoch()).ok()?;
    days.checked_mul(86_400_000)?
        .checked_add(i64::from(time.hour) * 3_600_000)?
        .checked_add(i64::from(time.minute) * 60_000)?
        .checked_add(i64::from(time.second) * 1000)?
        .checked_add(millis)?
        .checked_sub(i64::from(offset.seconds()) * 1000)
}
