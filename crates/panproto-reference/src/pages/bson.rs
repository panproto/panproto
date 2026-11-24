//! Complete reference operations assigned to `bson`.
//!
//! A binary document is read byte by byte from the BSON 1.1 grammar with
//! the rules and the refusal order the `bson` page states, decimal128 from
//! the bit layout of IEEE 754-2008 BID, and Extended JSON from the
//! crate's own JSON reader with the version 2 conversion table. Doubles
//! are held by their bits, datetimes by their millisecond count, and a
//! decimal128 by its sign, coefficient and exponent. It shares no code with
//! production, which reads the bytes with its own decoder, holds values in
//! panproto's value types and reads Extended JSON through the shared
//! tree-sitter JSON route.

use std::fmt::Write as _;

use crate::json::{self, Value as Json};

/// A document: its elements in order.
pub type Document = Vec<(String, Bson)>;

/// A decimal128 value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decimal {
    /// `(-1)^negative × coefficient × 10^exponent`.
    Finite {
        /// The sign.
        negative: bool,
        /// The coefficient, below `10^34`.
        coefficient: u128,
        /// The exponent, from -6176 to 6111.
        exponent: i32,
    },
    /// An infinity of the sign.
    Infinity(bool),
    /// A NaN: sign, signalling, payload below `10^33`.
    NaN(bool, bool, u128),
}

/// A BSON value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bson {
    /// A double, by its bits.
    Double(u64),
    /// A string.
    String(String),
    /// An embedded document.
    Document(Document),
    /// An array.
    Array(Vec<Self>),
    /// Subtype and payload.
    Binary(u8, Vec<u8>),
    /// Undefined.
    Undefined,
    /// An object id.
    ObjectId([u8; 12]),
    /// A boolean.
    Boolean(bool),
    /// Milliseconds since the epoch.
    DateTime(i64),
    /// Null.
    Null,
    /// Pattern and options.
    Regex(String, String),
    /// Namespace and object id.
    DbPointer(String, [u8; 12]),
    /// Code.
    Code(String),
    /// A symbol.
    Symbol(String),
    /// Code and scope.
    CodeWithScope(String, Document),
    /// A 32-bit integer.
    Int32(i32),
    /// Time and increment.
    Timestamp(u32, u32),
    /// A 64-bit integer.
    Int64(i64),
    /// A decimal128.
    Decimal128(Decimal),
    /// The minimum key.
    MinKey,
    /// The maximum key.
    MaxKey,
}

/// Why bytes are refused, in the order the page states, each at the
/// offset the page gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeRefusal {
    /// A document length below 5 or past its container.
    DocumentLength(i64),
    /// A fixed-size field past its container.
    Truncated,
    /// A last byte other than 0.
    Terminator(u8),
    /// A 0 type byte before the last byte.
    EarlyTerminator,
    /// A type byte outside BSON 1.1.
    UnknownType(u8),
    /// A C string without its 0.
    UnterminatedCString,
    /// Text that is not UTF-8.
    InvalidUtf8,
    /// A string length below 1 or past its container.
    StringLength(i64),
    /// A string's last byte other than 0.
    StringTerminator(u8),
    /// A boolean other than 0 and 1.
    Boolean(u8),
    /// A negative binary length or one past the container.
    BinaryLength(i64),
    /// A subtype 2 inner length other than the length less 4.
    OldBinaryLength(i64),
    /// A code-with-scope length that is not its parts' length.
    CodeWithScopeLength(i64),
    /// An array key other than the index.
    ArrayKey(String),
    /// Options out of order.
    RegexOptions,
    /// Decimal128 bits that are not a canonical encoding.
    NonCanonicalDecimal128,
    /// Bytes after the document.
    TrailingBytes,
    /// Nesting deeper than 128.
    Depth,
}

type Refused = (usize, DecodeRefusal);

/// An offset or length as a signed count (saturating; offsets of real
/// inputs are far below the bound).
fn wide(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// A non-negative count as an offset (saturating; callers check the sign
/// first).
fn narrow(n: i64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

/// A length as the `int32` the binary form writes (saturating).
fn int32(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// The deepest nesting accepted, the top-level document at depth 1.
pub const MAX_DEPTH: usize = 128;

const fn le32(bytes: &[u8], at: usize, end: usize) -> Result<i32, Refused> {
    if at + 4 > end {
        return Err((at, DecodeRefusal::Truncated));
    }
    Ok(i32::from_le_bytes([
        bytes[at],
        bytes[at + 1],
        bytes[at + 2],
        bytes[at + 3],
    ]))
}

fn take<const N: usize>(bytes: &[u8], at: usize, end: usize) -> Result<[u8; N], Refused> {
    if at + N > end {
        return Err((at, DecodeRefusal::Truncated));
    }
    let mut out = [0; N];
    out.copy_from_slice(&bytes[at..at + N]);
    Ok(out)
}

fn text_at(bytes: &[u8], at: usize) -> Result<String, Refused> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok(text.to_owned()),
        Err(error) => Err((at + error.valid_up_to(), DecodeRefusal::InvalidUtf8)),
    }
}

/// A C string from `at`, its 0 before `end`; the text and the offset after.
fn cstring(bytes: &[u8], at: usize, end: usize) -> Result<(String, usize), Refused> {
    let mut i = at;
    while i < end {
        if bytes[i] == 0 {
            return Ok((text_at(&bytes[at..i], at)?, i + 1));
        }
        i += 1;
    }
    Err((at, DecodeRefusal::UnterminatedCString))
}

fn string(bytes: &[u8], at: usize, end: usize) -> Result<(String, usize), Refused> {
    let length = i64::from(le32(bytes, at, end)?);
    let body = at + 4;
    if length < 1 || wide(body) + length > wide(end) {
        return Err((at, DecodeRefusal::StringLength(length)));
    }
    let stop = body + narrow(length);
    if bytes[stop - 1] != 0 {
        return Err((stop - 1, DecodeRefusal::StringTerminator(bytes[stop - 1])));
    }
    Ok((text_at(&bytes[body..stop - 1], body)?, stop))
}

/// The document at `at`, which must end by `end`.
fn document(
    bytes: &[u8],
    at: usize,
    end: usize,
    depth: usize,
    array: bool,
) -> Result<(Document, usize), Refused> {
    if depth > MAX_DEPTH {
        return Err((at, DecodeRefusal::Depth));
    }
    let length = i64::from(le32(bytes, at, end)?);
    if length < 5 || wide(at) + length > wide(end) {
        return Err((at, DecodeRefusal::DocumentLength(length)));
    }
    let stop = at + narrow(length);
    let mut out = Vec::new();
    let mut i = at + 4;
    loop {
        let byte = bytes[i];
        if i + 1 == stop {
            return if byte == 0 {
                Ok((out, stop))
            } else {
                Err((i, DecodeRefusal::Terminator(byte)))
            };
        }
        if byte == 0 {
            return Err((i, DecodeRefusal::EarlyTerminator));
        }
        let known = (1..=0x13).contains(&byte) || byte == 0x7F || byte == 0xFF;
        if !known {
            return Err((i, DecodeRefusal::UnknownType(byte)));
        }
        let (key, after) = cstring(bytes, i + 1, stop - 1)?;
        if array && key != out.len().to_string() {
            return Err((i + 1, DecodeRefusal::ArrayKey(out.len().to_string())));
        }
        let (value, next) = element(bytes, byte, after, stop - 1, depth)?;
        out.push((key, value));
        i = next;
    }
}

fn element(
    bytes: &[u8],
    kind: u8,
    at: usize,
    end: usize,
    depth: usize,
) -> Result<(Bson, usize), Refused> {
    let fixed8 = |at| take::<8>(bytes, at, end);
    Ok(match kind {
        0x01 => (Bson::Double(u64::from_le_bytes(fixed8(at)?)), at + 8),
        0x02 | 0x0D | 0x0E => {
            let (text, next) = string(bytes, at, end)?;
            let value = match kind {
                0x02 => Bson::String(text),
                0x0D => Bson::Code(text),
                _ => Bson::Symbol(text),
            };
            (value, next)
        }
        0x03 | 0x04 => {
            let (inner, next) = document(bytes, at, end, depth + 1, kind == 0x04)?;
            let value = if kind == 0x03 {
                Bson::Document(inner)
            } else {
                Bson::Array(inner.into_iter().map(|(_, v)| v).collect())
            };
            (value, next)
        }
        0x05 => {
            let length = i64::from(le32(bytes, at, end)?);
            if length < 0 || wide(at + 5) + length > wide(end) {
                return Err((at, DecodeRefusal::BinaryLength(length)));
            }
            let subtype = bytes[at + 4];
            let stop = at + 5 + narrow(length);
            let mut start = at + 5;
            if subtype == 2 {
                let inner = i64::from(le32(bytes, start, stop)?);
                if inner != length - 4 {
                    return Err((start, DecodeRefusal::OldBinaryLength(inner)));
                }
                start += 4;
            }
            (Bson::Binary(subtype, bytes[start..stop].to_vec()), stop)
        }
        0x06 => (Bson::Undefined, at),
        0x07 => (Bson::ObjectId(take::<12>(bytes, at, end)?), at + 12),
        0x08 => match take::<1>(bytes, at, end)?[0] {
            0 => (Bson::Boolean(false), at + 1),
            1 => (Bson::Boolean(true), at + 1),
            other => return Err((at, DecodeRefusal::Boolean(other))),
        },
        0x09 => (Bson::DateTime(i64::from_le_bytes(fixed8(at)?)), at + 8),
        0x0A => (Bson::Null, at),
        0x0B => {
            let (pattern, mid) = cstring(bytes, at, end)?;
            let (options, next) = cstring(bytes, mid, end)?;
            if !sorted(&options) {
                return Err((mid, DecodeRefusal::RegexOptions));
            }
            (Bson::Regex(pattern, options), next)
        }
        0x0C => {
            let (namespace, mid) = string(bytes, at, end)?;
            (
                Bson::DbPointer(namespace, take::<12>(bytes, mid, end)?),
                mid + 12,
            )
        }
        0x0F => {
            let length = i64::from(le32(bytes, at, end)?);
            if length < 14 || wide(at) + length > wide(end) {
                return Err((at, DecodeRefusal::CodeWithScopeLength(length)));
            }
            let stop = at + narrow(length);
            let (code, mid) = string(bytes, at + 4, stop)?;
            let (scope, next) = document(bytes, mid, stop, depth + 1, false)?;
            if next != stop {
                return Err((at, DecodeRefusal::CodeWithScopeLength(length)));
            }
            (Bson::CodeWithScope(code, scope), stop)
        }
        0x10 => (
            Bson::Int32(i32::from_le_bytes(take::<4>(bytes, at, end)?)),
            at + 4,
        ),
        0x11 => {
            let raw = fixed8(at)?;
            let low = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
            let high = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
            (Bson::Timestamp(high, low), at + 8)
        }
        0x12 => (Bson::Int64(i64::from_le_bytes(fixed8(at)?)), at + 8),
        0x13 => {
            let raw = take::<16>(bytes, at, end)?;
            let value = bid_value(u128::from_le_bytes(raw))
                .ok_or((at, DecodeRefusal::NonCanonicalDecimal128))?;
            (Bson::Decimal128(value), at + 16)
        }
        0x7F => (Bson::MaxKey, at),
        _ => (Bson::MinKey, at),
    })
}

fn sorted(options: &str) -> bool {
    let chars: Vec<u8> = options.bytes().collect();
    (1..chars.len()).all(|i| chars[i - 1] <= chars[i])
}

const TEN_34: u128 = 10_000_000_000_000_000_000_000_000_000_000_000;
const TEN_33: u128 = TEN_34 / 10;

/// The value of BID bits, or `None` for bits that are not a canonical
/// encoding.
fn bid_value(bits: u128) -> Option<Decimal> {
    let negative = bits >> 127 == 1;
    let top5 = (bits >> 122) & 0b1_1111;
    if top5 == 0b1_1111 {
        let signalling = (bits >> 121) & 1 == 1;
        let filler = (bits >> 110) & 0x7FF;
        let payload = bits & ((1u128 << 110) - 1);
        return (filler == 0 && payload < TEN_33)
            .then_some(Decimal::NaN(negative, signalling, payload));
    }
    if top5 == 0b1_1110 {
        return (bits & ((1u128 << 122) - 1) == 0).then_some(Decimal::Infinity(negative));
    }
    if (bits >> 125) & 0b11 == 0b11 {
        return None;
    }
    let coefficient = bits & ((1u128 << 113) - 1);
    let biased = i32::try_from((bits >> 113) & 0x3FFF).ok()?;
    (coefficient < TEN_34).then_some(Decimal::Finite {
        negative,
        coefficient,
        exponent: biased - 6176,
    })
}

fn bid_bits(value: &Decimal) -> u128 {
    match *value {
        Decimal::Finite {
            negative,
            coefficient,
            exponent,
        } => {
            (u128::from(negative) << 127)
                | (u128::try_from(exponent + 6176).unwrap_or_default() << 113)
                | coefficient
        }
        Decimal::Infinity(negative) => (u128::from(negative) << 127) | (0b1_1110 << 122),
        Decimal::NaN(negative, signalling, payload) => {
            (u128::from(negative) << 127)
                | (0b1_1111 << 122)
                | (u128::from(signalling) << 121)
                | payload
        }
    }
}

/// Decode one binary document, the whole of `bytes`.
///
/// # Errors
///
/// The offset and refusal of the first rule broken.
pub fn decode(bytes: &[u8]) -> Result<Document, (usize, DecodeRefusal)> {
    let (out, stop) = document(bytes, 0, bytes.len(), 1, false)?;
    if stop == bytes.len() {
        Ok(out)
    } else {
        Err((stop, DecodeRefusal::TrailingBytes))
    }
}

fn put_document(out: &mut Vec<u8>, elements: &[(String, &Bson)]) {
    let start = out.len();
    out.extend_from_slice(&[0, 0, 0, 0]);
    for (key, value) in elements {
        out.push(type_byte(value));
        out.extend_from_slice(key.as_bytes());
        out.push(0);
        put_value(out, value);
    }
    out.push(0);
    let length = int32(out.len() - start);
    out[start..start + 4].copy_from_slice(&length.to_le_bytes());
}

fn put_string(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&int32(text.len() + 1).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    out.push(0);
}

const fn type_byte(value: &Bson) -> u8 {
    match value {
        Bson::Double(_) => 1,
        Bson::String(_) => 2,
        Bson::Document(_) => 3,
        Bson::Array(_) => 4,
        Bson::Binary(..) => 5,
        Bson::Undefined => 6,
        Bson::ObjectId(_) => 7,
        Bson::Boolean(_) => 8,
        Bson::DateTime(_) => 9,
        Bson::Null => 10,
        Bson::Regex(..) => 11,
        Bson::DbPointer(..) => 12,
        Bson::Code(_) => 13,
        Bson::Symbol(_) => 14,
        Bson::CodeWithScope(..) => 15,
        Bson::Int32(_) => 16,
        Bson::Timestamp(..) => 17,
        Bson::Int64(_) => 18,
        Bson::Decimal128(_) => 19,
        Bson::MinKey => 0xFF,
        Bson::MaxKey => 0x7F,
    }
}

fn keyed(document: &Document) -> Vec<(String, &Bson)> {
    document.iter().map(|(k, v)| (k.clone(), v)).collect()
}

fn put_value(out: &mut Vec<u8>, value: &Bson) {
    match value {
        Bson::Double(bits) => out.extend_from_slice(&bits.to_le_bytes()),
        Bson::String(text) | Bson::Code(text) | Bson::Symbol(text) => put_string(out, text),
        Bson::Document(inner) => put_document(out, &keyed(inner)),
        Bson::Array(items) => {
            let keyed: Vec<(String, &Bson)> = items
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v))
                .collect();
            put_document(out, &keyed);
        }
        Bson::Binary(subtype, payload) => {
            let extra = if *subtype == 2 { 4 } else { 0 };
            out.extend_from_slice(&int32(payload.len() + extra).to_le_bytes());
            out.push(*subtype);
            if *subtype == 2 {
                out.extend_from_slice(&int32(payload.len()).to_le_bytes());
            }
            out.extend_from_slice(payload);
        }
        Bson::ObjectId(id) => out.extend_from_slice(id),
        Bson::Boolean(flag) => out.push(u8::from(*flag)),
        Bson::DateTime(n) | Bson::Int64(n) => out.extend_from_slice(&n.to_le_bytes()),
        Bson::Regex(pattern, options) => {
            for part in [pattern, options] {
                out.extend_from_slice(part.as_bytes());
                out.push(0);
            }
        }
        Bson::DbPointer(namespace, id) => {
            put_string(out, namespace);
            out.extend_from_slice(id);
        }
        Bson::CodeWithScope(code, scope) => {
            let start = out.len();
            out.extend_from_slice(&[0, 0, 0, 0]);
            put_string(out, code);
            put_document(out, &keyed(scope));
            let length = int32(out.len() - start);
            out[start..start + 4].copy_from_slice(&length.to_le_bytes());
        }
        Bson::Int32(n) => out.extend_from_slice(&n.to_le_bytes()),
        Bson::Timestamp(time, increment) => {
            out.extend_from_slice(&increment.to_le_bytes());
            out.extend_from_slice(&time.to_le_bytes());
        }
        Bson::Decimal128(value) => out.extend_from_slice(&bid_bits(value).to_le_bytes()),
        Bson::Undefined | Bson::Null | Bson::MinKey | Bson::MaxKey => {}
    }
}

/// The canonical binary form of a document every value of which is
/// valid (as every decoded document is).
#[must_use]
pub fn encode(document: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    put_document(&mut out, &keyed(document));
    out
}

// ── The tree encoding ──────────────────────────────────────────────────

/// One node of the BSON tree encoding: its parent and the edge from it
/// (`None` for the root), its anchor, and what it holds as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeNode {
    /// The parent node's index and the edge's name.
    pub parent: Option<(usize, &'static str)>,
    /// The anchor.
    pub anchor: &'static str,
    /// The held value, written as the page's table writes it.
    pub held: Option<String>,
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::new();
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// The anchor and held text of a value's node.
fn node_of(value: &Bson) -> (&'static str, Option<String>) {
    match value {
        Bson::Double(bits) => ("double", Some(format!("float {bits:016x}"))),
        Bson::String(text) => ("string", Some(format!("str {text}"))),
        Bson::Document(_) => ("document", None),
        Bson::Array(_) => ("array", None),
        Bson::Binary(subtype, payload) => (
            "binary",
            Some(format!("binary subtype={subtype} bytes={}", hex(payload))),
        ),
        Bson::Undefined => ("undefined", None),
        Bson::ObjectId(id) => ("object-id", Some(format!("bytes {}", hex(id)))),
        Bson::Boolean(flag) => ("boolean", Some(format!("bool {flag}"))),
        Bson::DateTime(ms) => ("datetime", Some(format!("temporal {}", iso(*ms, true)))),
        Bson::Null => ("null", Some("null".to_owned())),
        Bson::Regex(pattern, options) => (
            "regex",
            Some(format!("regex options={options} pattern={pattern}")),
        ),
        Bson::DbPointer(namespace, id) => (
            "db-pointer",
            Some(format!("db-pointer id={} namespace={namespace}", hex(id))),
        ),
        Bson::Code(text) => ("code", Some(format!("str {text}"))),
        Bson::Symbol(text) => ("symbol", Some(format!("str {text}"))),
        Bson::CodeWithScope(code, _) => ("code-with-scope", Some(format!("str {code}"))),
        Bson::Int32(n) => ("int32", Some(format!("int {n}"))),
        Bson::Timestamp(time, increment) => (
            "timestamp",
            Some(format!("timestamp increment={increment} time={time}")),
        ),
        Bson::Int64(n) => ("int64", Some(format!("int {n}"))),
        Bson::Decimal128(value) => (
            "decimal128",
            Some(format!("decimal {}", decimal_key(value))),
        ),
        Bson::MinKey => ("min-key", None),
        Bson::MaxKey => ("max-key", None),
    }
}

/// A decimal written as its parts.
fn decimal_key(value: &Decimal) -> String {
    match value {
        Decimal::Finite {
            negative,
            coefficient,
            exponent,
        } => format!(
            "{}{coefficient}e{exponent}",
            if *negative { "-" } else { "" }
        ),
        Decimal::Infinity(negative) => format!("{}inf", if *negative { "-" } else { "" }),
        Decimal::NaN(negative, signalling, payload) => format!(
            "{}{}nan{payload}",
            if *negative { "-" } else { "" },
            if *signalling { "s" } else { "" }
        ),
    }
}

fn tree_document(
    out: &mut Vec<TreeNode>,
    document: &Document,
    parent: Option<(usize, &'static str)>,
) {
    let at = out.len();
    out.push(TreeNode {
        parent,
        anchor: "document",
        held: None,
    });
    for (key, value) in document {
        let element = out.len();
        out.push(TreeNode {
            parent: Some((at, "element")),
            anchor: "element",
            held: Some(format!("str {key}")),
        });
        tree_value(out, value, (element, "value"));
    }
}

fn tree_value(out: &mut Vec<TreeNode>, value: &Bson, parent: (usize, &'static str)) {
    match value {
        Bson::Document(inner) => tree_document(out, inner, Some(parent)),
        other => {
            let at = out.len();
            let (anchor, held) = node_of(other);
            out.push(TreeNode {
                parent: Some(parent),
                anchor,
                held,
            });
            match other {
                Bson::Array(items) => {
                    for item in items {
                        tree_value(out, item, (at, "item"));
                    }
                }
                Bson::CodeWithScope(_, scope) => tree_document(out, scope, Some((at, "scope"))),
                _ => {}
            }
        }
    }
}

/// The nodes of a document in the BSON tree encoding, in the order the
/// page numbers them.
#[must_use]
pub fn tree(document: &Document) -> Vec<TreeNode> {
    let mut out = Vec::new();
    tree_document(&mut out, document, None);
    out
}

// ── Numbers and dates as text ──────────────────────────────────────────

/// The days-since-epoch civil date (Hinnant's algorithm).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Days since the epoch of a civil date.
const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The ISO date-time of a millisecond count: always three fraction digits
/// when `full`, else none when they are zero; `Z`.
fn iso(ms: i64, full: bool) -> String {
    let (year, month, day) = civil(ms.div_euclid(86_400_000));
    let within = ms.rem_euclid(86_400_000);
    let mut out = String::new();
    if year < 0 {
        let _ = write!(out, "-{:04}", -year);
    } else {
        let _ = write!(out, "{year:04}");
    }
    let _ = write!(
        out,
        "-{month:02}-{day:02}T{:02}:{:02}:{:02}",
        within / 3_600_000,
        within / 60_000 % 60,
        within / 1000 % 60
    );
    if full || within % 1000 != 0 {
        let _ = write!(out, ".{:03}", within % 1000);
    }
    out.push('Z');
    out
}

/// The double text of the page: shortest digits, positional with a
/// fraction digit for decimal exponents -4 to 15, else `dE±n`.
#[must_use]
pub fn double_text(bits: u64) -> Option<String> {
    let x = f64::from_bits(bits);
    if !x.is_finite() {
        return None;
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    let lower = format!("{:e}", x.abs());
    let (mantissa, exponent) = lower.split_once('e')?;
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let digits = if digits.trim_end_matches('0').is_empty() {
        "0".to_owned()
    } else {
        digits.trim_end_matches('0').to_owned()
    };
    let point: i64 = exponent.parse().ok()?;
    if (-4..16).contains(&point) {
        let whole_len = point + 1;
        return Some(if whole_len <= 0 {
            format!("{sign}0.{}{digits}", "0".repeat(narrow(-whole_len)))
        } else if narrow(whole_len) >= digits.len() {
            format!(
                "{sign}{digits}{}.0",
                "0".repeat(narrow(whole_len) - digits.len())
            )
        } else {
            let (a, b) = digits.split_at(narrow(whole_len));
            format!("{sign}{a}.{b}")
        });
    }
    let (first, rest) = digits.split_at(1);
    Some(format!(
        "{sign}{first}{}{rest}E{}{}",
        if rest.is_empty() { "" } else { "." },
        if point < 0 { '-' } else { '+' },
        point.abs()
    ))
}

/// The decimal128 string of the BSON Decimal128 specification.
#[must_use]
pub fn decimal_text(value: &Decimal) -> String {
    match *value {
        Decimal::Infinity(negative) => if negative { "-Infinity" } else { "Infinity" }.to_owned(),
        Decimal::NaN(..) => "NaN".to_owned(),
        Decimal::Finite {
            negative,
            coefficient,
            exponent,
        } => {
            let sign = if negative { "-" } else { "" };
            let digits = coefficient.to_string();
            let adjusted = i64::from(exponent) + wide(digits.len()) - 1;
            if exponent <= 0 && adjusted >= -6 {
                let scale = narrow(i64::from(-exponent));
                if scale == 0 {
                    format!("{sign}{digits}")
                } else if digits.len() > scale {
                    let (a, b) = digits.split_at(digits.len() - scale);
                    format!("{sign}{a}.{b}")
                } else {
                    format!("{sign}0.{}{digits}", "0".repeat(scale - digits.len()))
                }
            } else {
                let (first, rest) = digits.split_at(1);
                format!(
                    "{sign}{first}{}{rest}E{}{}",
                    if rest.is_empty() { "" } else { "." },
                    if adjusted < 0 { '-' } else { '+' },
                    adjusted.abs()
                )
            }
        }
    }
}

// ── Writing Extended JSON ──────────────────────────────────────────────

/// Why a document has no Extended JSON text, with the JSON pointer of the
/// value at fault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteRefusal {
    /// A key holding U+0000.
    NulInKey,
    /// A name given twice in one document.
    DuplicateKey,
    /// A wrapper key in an embedded document.
    WrapperKey,
    /// A regular expression holding U+0000.
    NulInRegex,
    /// Options out of order.
    RegexOptions,
    /// A NaN other than the one `NaN` reads as.
    NaN,
    /// Nesting deeper than 128.
    Depth,
}

/// The keys of the Extended JSON type wrappers.
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

fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn pointer(path: &str, token: &str) -> String {
    format!("{path}/{}", token.replace('~', "~0").replace('/', "~1"))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(B64[narrow(i64::from((n >> (18 - 6 * i)) & 63))]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

struct Writer {
    relaxed: bool,
}

type Written = Result<String, (String, WriteRefusal)>;

impl Writer {
    fn document(&self, document: &Document, path: &str, depth: usize, top: bool) -> Written {
        if depth > MAX_DEPTH {
            return Err((path.to_owned(), WriteRefusal::Depth));
        }
        let mut members = Vec::new();
        for (i, (key, value)) in document.iter().enumerate() {
            let at = pointer(path, key);
            if key.contains('\0') {
                return Err((at, WriteRefusal::NulInKey));
            }
            if document[..i].iter().any(|(k, _)| k == key) {
                return Err((at, WriteRefusal::DuplicateKey));
            }
            if !top && WRAPPER_KEYS.contains(&key.as_str()) {
                return Err((at, WriteRefusal::WrapperKey));
            }
            members.push(format!("{}:{}", quote(key), self.value(value, &at, depth)?));
        }
        Ok(format!("{{{}}}", members.join(",")))
    }

    fn value(&self, value: &Bson, path: &str, depth: usize) -> Written {
        let wrap = |key: &str, inner: String| format!("{{\"{key}\":{inner}}}");
        Ok(match value {
            Bson::Double(bits) => {
                let x = f64::from_bits(*bits);
                if x.is_nan() && *bits != 0x7FF8_0000_0000_0000 {
                    return Err((path.to_owned(), WriteRefusal::NaN));
                }
                match double_text(*bits) {
                    Some(text) if self.relaxed => text,
                    Some(text) => wrap("$numberDouble", quote(&text)),
                    None if x.is_nan() => wrap("$numberDouble", quote("NaN")),
                    None if x > 0.0 => wrap("$numberDouble", quote("Infinity")),
                    None => wrap("$numberDouble", quote("-Infinity")),
                }
            }
            Bson::String(text) => quote(text),
            Bson::Document(inner) => self.document(inner, path, depth + 1, false)?,
            Bson::Array(items) => {
                if depth + 1 > MAX_DEPTH {
                    return Err((path.to_owned(), WriteRefusal::Depth));
                }
                let mut parts = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    parts.push(self.value(item, &pointer(path, &i.to_string()), depth + 1)?);
                }
                format!("[{}]", parts.join(","))
            }
            Bson::Binary(subtype, payload) => wrap(
                "$binary",
                format!(
                    "{{\"base64\":{},\"subType\":\"{subtype:02x}\"}}",
                    quote(&base64(payload))
                ),
            ),
            Bson::Undefined => wrap("$undefined", "true".to_owned()),
            Bson::ObjectId(id) => wrap("$oid", quote(&hex(id))),
            Bson::Boolean(flag) => flag.to_string(),
            Bson::DateTime(ms) => {
                if self.relaxed && (0..253_402_300_800_000).contains(ms) {
                    wrap("$date", quote(&iso(*ms, false)))
                } else {
                    wrap("$date", format!("{{\"$numberLong\":\"{ms}\"}}"))
                }
            }
            Bson::Null => "null".to_owned(),
            Bson::Regex(pattern, options) => {
                if pattern.contains('\0') || options.contains('\0') {
                    return Err((path.to_owned(), WriteRefusal::NulInRegex));
                }
                if !sorted(options) {
                    return Err((path.to_owned(), WriteRefusal::RegexOptions));
                }
                wrap(
                    "$regularExpression",
                    format!(
                        "{{\"pattern\":{},\"options\":{}}}",
                        quote(pattern),
                        quote(options)
                    ),
                )
            }
            Bson::DbPointer(namespace, id) => wrap(
                "$dbPointer",
                format!(
                    "{{\"$ref\":{},\"$id\":{{\"$oid\":{}}}}}",
                    quote(namespace),
                    quote(&hex(id))
                ),
            ),
            Bson::Code(code) => wrap("$code", quote(code)),
            Bson::Symbol(symbol) => wrap("$symbol", quote(symbol)),
            Bson::CodeWithScope(code, scope) => format!(
                "{{\"$code\":{},\"$scope\":{}}}",
                quote(code),
                self.document(scope, &pointer(path, "$scope"), depth + 1, true)?
            ),
            Bson::Int32(n) if self.relaxed => n.to_string(),
            Bson::Int64(n) if self.relaxed => n.to_string(),
            Bson::Int32(n) => wrap("$numberInt", quote(&n.to_string())),
            Bson::Int64(n) => wrap("$numberLong", quote(&n.to_string())),
            Bson::Timestamp(time, increment) => {
                wrap("$timestamp", format!("{{\"t\":{time},\"i\":{increment}}}"))
            }
            Bson::Decimal128(value) => {
                if matches!(value, Decimal::NaN(n, s, p) if *n || *s || *p != 0) {
                    return Err((path.to_owned(), WriteRefusal::NaN));
                }
                wrap("$numberDecimal", quote(&decimal_text(value)))
            }
            Bson::MinKey => wrap("$minKey", "1".to_owned()),
            Bson::MaxKey => wrap("$maxKey", "1".to_owned()),
        })
    }
}

/// The canonical Extended JSON of a document.
///
/// # Errors
///
/// The JSON pointer and refusal of the first value no text reads back as.
pub fn canonical_text(document: &Document) -> Result<String, (String, WriteRefusal)> {
    Writer { relaxed: false }.document(document, "", 1, true)
}

/// The relaxed Extended JSON of a document.
///
/// # Errors
///
/// As [`canonical_text`].
pub fn relaxed_text(document: &Document) -> Result<String, (String, WriteRefusal)> {
    Writer { relaxed: true }.document(document, "", 1, true)
}

// ── Reading Extended JSON ──────────────────────────────────────────────

/// Why an Extended JSON text is refused, with the JSON pointer the page
/// gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadRefusal {
    /// Not one JSON value, or an object naming a member twice.
    Json,
    /// The top-level value is not an object.
    NotADocument,
    /// A key holding U+0000.
    NulInKey,
    /// A wrapper key in an object whose keys are no wrapper's.
    WrapperKeys(&'static str),
    /// A wrapper value of the wrong kind or form.
    WrapperValue(&'static str),
    /// A regular expression holding U+0000.
    NulInRegex,
    /// Options out of order.
    RegexOptions,
    /// A decimal128 string that needs clamping or rounding.
    DecimalRepresentation,
    /// An integer outside int64.
    IntegerRange,
    /// A number no double holds as written.
    InexactNumber,
}

type Read<T> = Result<T, (String, ReadRefusal)>;

fn no_duplicates(value: &Json) -> bool {
    match value {
        Json::Array(items) => items.iter().all(no_duplicates),
        Json::Object(members) => members
            .iter()
            .enumerate()
            .all(|(i, (key, v))| members[..i].iter().all(|(k, _)| k != key) && no_duplicates(v)),
        _ => true,
    }
}

/// Read an Extended JSON text, canonical or relaxed.
///
/// # Errors
///
/// The JSON pointer and refusal of the first value at fault.
pub fn read_text(text: &str) -> Read<Document> {
    let tree = json::parse(text)
        .filter(no_duplicates)
        .ok_or((String::new(), ReadRefusal::Json))?;
    match &tree {
        Json::Object(members) => read_document(members, ""),
        _ => Err((String::new(), ReadRefusal::NotADocument)),
    }
}

fn read_document(members: &[(String, Json)], path: &str) -> Read<Document> {
    let mut out = Vec::new();
    for (key, value) in members {
        let at = pointer(path, key);
        if key.contains('\0') {
            return Err((at, ReadRefusal::NulInKey));
        }
        out.push((key.clone(), read_value(value, &at)?));
    }
    Ok(out)
}

/// Whether `text` is a JSON integer lexeme (no fraction, no exponent).
fn integer_lexeme(text: &str) -> bool {
    !text.contains(['.', 'e', 'E'])
}

/// The decimal value of a finite decimal string as (negative, digits
/// without leading zeros, exponent), when `text` is one: an optional sign,
/// digits with an optional point (digits on at least one side), an
/// optional `e`/`E` with an optional sign and digits.
fn decimal_parts(text: &str) -> Option<(bool, String, i128)> {
    let (negative, rest) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let (mantissa, exponent) = rest
        .find(['e', 'E'])
        .map_or((rest, None), |at| (&rest[..at], Some(&rest[at + 1..])));
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digit = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if (whole.is_empty() && fraction.is_empty()) || !digit(whole) || !digit(fraction) {
        return None;
    }
    let written: i128 = match exponent {
        None => 0,
        Some(e) => {
            let body = e.strip_prefix(['+', '-']).unwrap_or(e);
            if body.is_empty() || !digit(body) {
                return None;
            }
            let magnitude: i128 = body.parse().unwrap_or(i128::MAX / 4);
            if e.starts_with('-') {
                -magnitude
            } else {
                magnitude
            }
        }
    };
    let all = format!("{whole}{fraction}");
    let trimmed = all.trim_start_matches('0');
    let digits = if trimmed.is_empty() { "0" } else { trimmed };
    Some((
        negative,
        digits.to_owned(),
        written - fraction.len() as i128,
    ))
}

/// The double a finite decimal string names, when the shortest form of the
/// nearest double has the same value.
fn exact_double(text: &str) -> Option<u64> {
    let (negative, digits, exponent) = decimal_parts(text)?;
    let normal = format!("{}{digits}e{exponent}", if negative { "-" } else { "" });
    let x: f64 = normal.parse().ok()?;
    if !x.is_finite() {
        return None;
    }
    let shortest = format!("{x:e}");
    let (_, back_digits, back_exponent) = decimal_parts(&shortest)?;
    let canonical = |d: &str, e: i128| {
        let t = d.trim_end_matches('0');
        if t.is_empty() {
            ("0".to_owned(), 0)
        } else {
            (t.to_owned(), e + (d.len() - t.len()) as i128)
        }
    };
    (canonical(&digits, exponent) == canonical(&back_digits, back_exponent)).then_some(x.to_bits())
}

fn read_number(text: &str, path: &str) -> Read<Bson> {
    if integer_lexeme(text) {
        let n: i64 = text
            .parse()
            .map_err(|_| (path.to_owned(), ReadRefusal::IntegerRange))?;
        return Ok(i32::try_from(n).map_or(Bson::Int64(n), Bson::Int32));
    }
    exact_double(text)
        .map(Bson::Double)
        .ok_or_else(|| (path.to_owned(), ReadRefusal::InexactNumber))
}

fn read_value(value: &Json, path: &str) -> Read<Bson> {
    match value {
        Json::Null => Ok(Bson::Null),
        Json::Bool(flag) => Ok(Bson::Boolean(*flag)),
        Json::String(text) => Ok(Bson::String(text.clone())),
        Json::Number(text) => read_number(text, path),
        Json::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, item)| read_value(item, &pointer(path, &i.to_string())))
            .collect::<Read<Vec<_>>>()
            .map(Bson::Array),
        Json::Object(members) => {
            let first = members
                .iter()
                .find_map(|(k, _)| WRAPPER_KEYS.iter().find(|w| **w == k.as_str()).copied());
            first.map_or_else(
                || read_document(members, path).map(Bson::Document),
                |key| read_wrapper(key, members, path),
            )
        }
    }
}

fn get<'j>(members: &'j [(String, Json)], key: &str) -> Option<&'j Json> {
    members.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn exactly(members: &[(String, Json)], keys: &[&str]) -> bool {
    members.len() == keys.len() && keys.iter().all(|k| get(members, k).is_some())
}

fn hex_bytes(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

fn object_id(text: &str) -> Option<[u8; 12]> {
    (text.len() == 24)
        .then(|| hex_bytes(text))
        .flatten()
        .and_then(|bytes| bytes.try_into().ok())
}

/// Strict base64: the bytes whose canonical encoding is exactly `text`.
fn unbase64(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let mut bits = Vec::new();
    for c in text.bytes().filter(|b| *b != b'=') {
        bits.push(u32::try_from(B64.iter().position(|a| *a == c)?).ok()?);
    }
    let mut out = Vec::new();
    for chunk in bits.chunks(4) {
        let mut n = 0u32;
        for (i, v) in chunk.iter().enumerate() {
            n |= v << (18 - 6 * i);
        }
        let count = chunk.len() * 6 / 8;
        for i in 0..count {
            out.push(u8::try_from((n >> (16 - 8 * i)) & 0xFF).ok()?);
        }
    }
    (base64(&out) == text).then_some(out)
}

fn int_lexeme(value: &Json) -> Option<i64> {
    match value {
        Json::Number(text) if integer_lexeme(text) => text.parse().ok(),
        _ => None,
    }
}

/// The milliseconds of an RFC 3339 date-time with a four-digit year, at
/// most three fraction digits, no leap second and an offset `Z`, `z` or
/// `±HH:MM`.
fn rfc3339(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    let num = |from: usize, to: usize| -> Option<i64> {
        let part = text.get(from..to)?;
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't') {
        return None;
    }
    if b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    let mut i = 19;
    let mut millis = 0;
    if b.get(i) == Some(&b'.') {
        let start = i + 1;
        let mut end = start;
        while b.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end == start || end - start > 3 {
            return None;
        }
        millis = num(start, end)? * 10_i64.pow(u32::try_from(3 - (end - start)).ok()?);
        i = end;
    }
    let offset_minutes = match b.get(i) {
        Some(b'Z' | b'z') if i + 1 == b.len() => 0,
        Some(sign @ (b'+' | b'-')) if b.len() == i + 6 && b[i + 3] == b':' => {
            let (h, m) = (num(i + 1, i + 3)?, num(i + 4, i + 6)?);
            if h > 23 || m > 59 {
                return None;
            }
            if *sign == b'-' {
                -(h * 60 + m)
            } else {
                h * 60 + m
            }
        }
        _ => return None,
    };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=12).contains(&month)
        || day < 1
        || day > month_days[narrow(month - 1)]
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    Some(
        days_from_civil(year, month, day) * 86_400_000
            + hour * 3_600_000
            + minute * 60_000
            + second * 1000
            + millis
            - offset_minutes * 60_000,
    )
}

/// A decimal128 numeric string, or `Err(true)` for a syntax fault and
/// `Err(false)` for a value decimal128 does not hold as written.
fn decimal128(text: &str) -> Result<Decimal, bool> {
    let (negative, unsigned) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let lower = unsigned.to_ascii_lowercase();
    if lower == "inf" || lower == "infinity" {
        return Ok(Decimal::Infinity(negative));
    }
    if lower == "nan" {
        return Ok(Decimal::NaN(negative, false, 0));
    }
    let (negative, digits, exponent) = decimal_parts(text).ok_or(true)?;
    if digits.len() > 34 || !(-6176..=6111).contains(&exponent) {
        return Err(false);
    }
    Ok(Decimal::Finite {
        negative,
        coefficient: digits.parse().map_err(|_| true)?,
        exponent: i32::try_from(exponent).map_err(|_| false)?,
    })
}

fn read_wrapper(key: &'static str, members: &[(String, Json)], path: &str) -> Read<Bson> {
    let keys_fault = || (path.to_owned(), ReadRefusal::WrapperKeys(key));
    if exactly(members, &["$code", "$scope"]) {
        let code = match get(members, "$code") {
            Some(Json::String(code)) => code.clone(),
            _ => return Err((pointer(path, "$code"), ReadRefusal::WrapperValue("$code"))),
        };
        let scope_at = pointer(path, "$scope");
        return match get(members, "$scope") {
            Some(Json::Object(scope)) => {
                Ok(Bson::CodeWithScope(code, read_document(scope, &scope_at)?))
            }
            _ => Err((scope_at, ReadRefusal::WrapperValue("$scope"))),
        };
    }
    if members.len() != 1 || key == "$scope" {
        return Err(keys_fault());
    }
    let inner = &members[0].1;
    let at = pointer(path, key);
    let bad = || (at.clone(), ReadRefusal::WrapperValue(key));
    let text = match inner {
        Json::String(text) => Some(text.as_str()),
        _ => None,
    };
    Ok(match key {
        "$oid" => Bson::ObjectId(text.and_then(object_id).ok_or_else(bad)?),
        "$symbol" => Bson::Symbol(text.ok_or_else(bad)?.to_owned()),
        "$code" => Bson::Code(text.ok_or_else(bad)?.to_owned()),
        "$numberInt" => Bson::Int32(
            text.and_then(canonical_int)
                .and_then(|n| i32::try_from(n).ok())
                .ok_or_else(bad)?,
        ),
        "$numberLong" => Bson::Int64(text.and_then(canonical_int).ok_or_else(bad)?),
        "$numberDouble" => {
            let t = text.ok_or_else(bad)?;
            match t {
                "Infinity" => Bson::Double(f64::INFINITY.to_bits()),
                "-Infinity" => Bson::Double(f64::NEG_INFINITY.to_bits()),
                "NaN" => Bson::Double(0x7FF8_0000_0000_0000),
                _ if decimal_parts(t).is_none() => return Err(bad()),
                _ => Bson::Double(
                    exact_double(t).ok_or_else(|| (at.clone(), ReadRefusal::InexactNumber))?,
                ),
            }
        }
        "$numberDecimal" => match decimal128(text.ok_or_else(bad)?) {
            Ok(value) => Bson::Decimal128(value),
            Err(true) => return Err(bad()),
            Err(false) => return Err((at.clone(), ReadRefusal::DecimalRepresentation)),
        },
        "$binary" => read_binary(inner, &at)?,
        "$timestamp" => read_timestamp(inner, &at)?,
        "$regularExpression" => read_regex(inner, &at)?,
        "$dbPointer" => read_db_pointer(inner, &at)?,
        "$date" => Bson::DateTime(read_date(inner, &at)?),
        "$minKey" if int_lexeme(inner) == Some(1) => Bson::MinKey,
        "$maxKey" if int_lexeme(inner) == Some(1) => Bson::MaxKey,
        "$undefined" if *inner == Json::Bool(true) => Bson::Undefined,
        "$uuid" => {
            let t = text.ok_or_else(bad)?;
            let groups: Vec<&str> = t.split('-').collect();
            let shape: Vec<usize> = groups.iter().map(|g| g.len()).collect();
            if shape != [8, 4, 4, 4, 12] {
                return Err(bad());
            }
            Bson::Binary(4, hex_bytes(&groups.concat()).ok_or_else(bad)?)
        }
        _ => return Err(bad()),
    })
}

/// The integer a string `-?(0|[1-9][0-9]*)` writes, when it fits `i64`.
fn canonical_int(t: &str) -> Option<i64> {
    let body = t.strip_prefix('-').unwrap_or(t);
    let ok = body == "0"
        || (!body.is_empty() && !body.starts_with('0') && body.bytes().all(|b| b.is_ascii_digit()));
    if ok { t.parse::<i64>().ok() } else { None }
}

fn wrapper_value(at: &str, key: &'static str) -> (String, ReadRefusal) {
    (at.to_owned(), ReadRefusal::WrapperValue(key))
}

fn read_binary(inner: &Json, at: &str) -> Read<Bson> {
    let bad = || wrapper_value(at, "$binary");
    let Json::Object(parts) = inner else {
        return Err(bad());
    };
    if !exactly(parts, &["base64", "subType"]) {
        return Err(bad());
    }
    let payload = match get(parts, "base64") {
        Some(Json::String(t)) => unbase64(t).ok_or_else(bad)?,
        _ => return Err(bad()),
    };
    let subtype = match get(parts, "subType") {
        Some(Json::String(t)) if (1..=2).contains(&t.len()) => {
            hex_bytes(&format!("{t:0>2}")).ok_or_else(bad)?[0]
        }
        _ => return Err(bad()),
    };
    Ok(Bson::Binary(subtype, payload))
}

fn read_timestamp(inner: &Json, at: &str) -> Read<Bson> {
    let bad = || wrapper_value(at, "$timestamp");
    let Json::Object(parts) = inner else {
        return Err(bad());
    };
    if !exactly(parts, &["t", "i"]) {
        return Err(bad());
    }
    let part = |k| {
        get(parts, k)
            .and_then(int_lexeme)
            .and_then(|n| u32::try_from(n).ok())
    };
    Ok(Bson::Timestamp(
        part("t").ok_or_else(bad)?,
        part("i").ok_or_else(bad)?,
    ))
}

fn read_regex(inner: &Json, at: &str) -> Read<Bson> {
    let bad = || wrapper_value(at, "$regularExpression");
    let Json::Object(parts) = inner else {
        return Err(bad());
    };
    if !exactly(parts, &["pattern", "options"]) {
        return Err(bad());
    }
    let (Some(Json::String(pattern)), Some(Json::String(options))) =
        (get(parts, "pattern"), get(parts, "options"))
    else {
        return Err(bad());
    };
    if pattern.contains('\0') || options.contains('\0') {
        return Err((at.to_owned(), ReadRefusal::NulInRegex));
    }
    if !sorted(options) {
        return Err((at.to_owned(), ReadRefusal::RegexOptions));
    }
    Ok(Bson::Regex(pattern.clone(), options.clone()))
}

fn read_db_pointer(inner: &Json, at: &str) -> Read<Bson> {
    let bad = || wrapper_value(at, "$dbPointer");
    let Json::Object(parts) = inner else {
        return Err(bad());
    };
    if !exactly(parts, &["$ref", "$id"]) {
        return Err(bad());
    }
    let Some(Json::String(namespace)) = get(parts, "$ref") else {
        return Err(bad());
    };
    let id = match get(parts, "$id") {
        Some(Json::Object(id)) if exactly(id, &["$oid"]) => match &id[0].1 {
            Json::String(t) => object_id(t),
            _ => None,
        },
        _ => None,
    };
    Ok(Bson::DbPointer(namespace.clone(), id.ok_or_else(bad)?))
}

fn read_date(inner: &Json, at: &str) -> Read<i64> {
    let bad = || wrapper_value(at, "$date");
    match inner {
        Json::String(t) => rfc3339(t).ok_or_else(bad),
        Json::Object(parts) if exactly(parts, &["$numberLong"]) => match &parts[0].1 {
            Json::String(t) => canonical_int(t).ok_or_else(bad),
            _ => Err(bad()),
        },
        _ => Err(bad()),
    }
}
