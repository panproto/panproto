//! Lossless JSON pathway for schema-guided instance parsing.
//!
//! This module provides the core JSON → `WInstance` pipeline used by ~30
//! protocols whose instance data is JSON (or JSON-like: YAML, TOML, BSON).
//! Numeric tokens remain borrowed slices of the input until
//! [`coerce`](panproto_inst::value::coerce::coerce) classifies them under
//! the schema vertex's declared [`ValueSpec`](panproto_schema::ValueSpec).
//! No binary floating-point or bounded-integer carrier can therefore round
//! or reject an exact numeral before the value layer sees it.

use std::collections::{BTreeMap, HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::metadata::{Node, NodeShape};
use panproto_inst::value::coerce::{LexClass, Lexeme, coerce};
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{ValueSite, WInstance, parse_json, to_json};
use panproto_schema::{BytesText, Edge, Protocol, ScalarKind, Schema, ValueSpec};

use crate::error::{EmitInstanceError, EmitValueError, EmitValueReason, ParseInstanceError};

/// Maximum nesting accepted by the recursive lossless JSON reader.
///
/// The document's top value is level one and every value inside a container
/// is one level deeper than the container, so an empty innermost container
/// counts as a level exactly as a scalar does. One level is one node of the
/// parsed tree, which keeps every accepted document within the canonical
/// emitter's tree-depth bound of the same size.
const MAX_JSON_DEPTH: usize = 128;
pub(crate) const MAX_VALUE_EMIT_DEPTH: usize = 128;

/// Render a float for plain schema-guided text fields. These spellings are
/// accepted by the shared coercion layer in XML and delimited text. NaN
/// payloads have no text spelling, so only Rust's canonical positive and
/// negative NaNs can be written without silently changing their IEEE value.
pub(crate) fn plain_text_float_lexeme(value: f64) -> Option<String> {
    if value.is_finite() {
        Some(value.to_string())
    } else if value.to_bits() == f64::NAN.to_bits() {
        Some("NaN".to_owned())
    } else if value.to_bits() == (-f64::NAN).to_bits() {
        Some("-NaN".to_owned())
    } else if value == f64::INFINITY {
        Some("Infinity".to_owned())
    } else if value == f64::NEG_INFINITY {
        Some("-Infinity".to_owned())
    } else {
        None
    }
}

/// Whether a scalar written into an untyped text field reparses exactly under
/// `spec`.
pub(crate) fn plain_text_value_roundtrips(value: &Value, spec: &ValueSpec) -> bool {
    let text = match value {
        Value::Str(text) | Value::Token(text) => text.clone(),
        Value::Int(number) => number.to_string(),
        Value::Float(number) => {
            let Some(text) = plain_text_float_lexeme(*number) else {
                return false;
            };
            text
        }
        Value::Bool(value) => value.to_string(),
        Value::Bytes(bytes) => {
            let ScalarKind::Bytes(encoding) = &spec.kind else {
                return false;
            };
            let Some(text) = encode_bytes(bytes, encoding) else {
                return false;
            };
            text
        }
        Value::Decimal(number) => number.to_string(),
        Value::BigInt(number) => number.to_string(),
        Value::Temporal(value) => value.to_string(),
        Value::Null
        | Value::CidLink(_)
        | Value::Blob { .. }
        | Value::Opaque { .. }
        | Value::Unknown(_)
        | Value::List(_)
        | Value::LabeledNull(_) => return false,
    };
    coerce_exact_lexeme(&Lexeme::new(&text, LexClass::String), spec).is_ok_and(|actual| {
        match (value, actual) {
            (Value::Float(expected), Value::Float(actual)) => {
                expected.to_bits() == actual.to_bits()
            }
            (_, actual) => value == &actual,
        }
    })
}

/// Apply shared coercion, keeping the two facts its decimal carrier drops.
///
/// The shared decimal carrier has no negative zero and bounds its exponent,
/// so this wrapper restores what a numeral denotes in both cases. First, the
/// sign of a negative zero or of a negative textual NaN is kept when the
/// result is a float. Second, a numeral whose significand digits are all
/// zero denotes zero whatever its exponent, so an exponent outside the
/// carrier's range is read as the same zero rather than refused as an
/// overflow. Every other lexeme coerces exactly as the shared layer reads it.
pub(crate) fn coerce_exact_lexeme(
    lexeme: &Lexeme<'_>,
    spec: &ValueSpec,
) -> Result<Value, panproto_inst::ValueFidelityError> {
    let zero = zero_numeral_sign(lexeme.text);
    let value = match coerce(lexeme, spec) {
        Err(error) if zero.is_some() && error.reason == panproto_inst::FidelityReason::Overflow => {
            coerce(&Lexeme::new("0e0", lexeme.class.clone()), spec).map_err(|_| error)?
        }
        other => other?,
    };
    let negative = zero == Some(true) || is_negative_nan_text(lexeme.text);
    Ok(match value {
        Value::Float(number) if negative && number.is_nan() => Value::Float(-f64::NAN),
        Value::Float(number) if zero == Some(true) && number == 0.0 => Value::Float(-0.0),
        other => other,
    })
}

/// Whether `text` spells a NaN with a leading minus sign.
fn is_negative_nan_text(text: &str) -> bool {
    text.strip_prefix('-')
        .map(|text| text.strip_prefix('.').unwrap_or(text))
        .is_some_and(|text| text.eq_ignore_ascii_case("nan"))
}

/// The sign of a decimal numeral whose significand digits are all zero:
/// `Some(true)` for a leading minus, `Some(false)` otherwise, and `None` when
/// `text` is not such a numeral. The grammar is an optional sign, digits with
/// an optional fraction (at least one digit in all), and an optional exponent
/// with at least one digit.
fn zero_numeral_sign(text: &str) -> Option<bool> {
    let (negative, unsigned) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let (significand, exponent) = unsigned
        .split_once(['e', 'E'])
        .map_or((unsigned, None), |(significand, exponent)| {
            (significand, Some(exponent))
        });
    let (whole, fraction) = significand
        .split_once('.')
        .map_or((significand, ""), |(whole, fraction)| (whole, fraction));
    let zero_digits = |digits: &str| digits.bytes().all(|byte| byte == b'0');
    let significand_ok =
        !(whole.is_empty() && fraction.is_empty()) && zero_digits(whole) && zero_digits(fraction);
    let exponent_ok = exponent.is_none_or(|exponent| {
        let digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    });
    (significand_ok && exponent_ok).then_some(negative)
}

#[derive(Debug)]
enum RawJson<'a> {
    Null,
    Bool { value: bool, offset: usize },
    Number { lexeme: &'a str, offset: usize },
    String { value: String, offset: usize },
    Array(Vec<Self>),
    Object(Vec<(String, Self)>),
}

/// Where a refused JSON string token goes wrong, relative to the token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JsonStringFaultSite {
    /// The offset of the offending byte: the first byte at which the token
    /// can no longer be completed to a token `serde_json` accepts.
    Byte(usize),
    /// The token ends while it can still be completed, so no byte is at
    /// fault. Carries the offset of its last byte; an empty token has none.
    End(Option<usize>),
}

/// Why and where a JSON string token is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JsonStringFault {
    /// What is wrong, in `serde_json`'s vocabulary where it has a word.
    pub(crate) reason: &'static str,
    /// The token offset the reason applies to.
    pub(crate) site: JsonStringFaultSite,
}

impl JsonStringFault {
    /// The reason followed by its site, with the token placed at document
    /// byte `origin`: "at byte N" names the offending byte, "after byte N"
    /// the last byte of a token that ends too early.
    pub(crate) fn placed_at(self, origin: u64) -> String {
        let place =
            |offset: usize| origin.saturating_add(u64::try_from(offset).unwrap_or(u64::MAX));
        match self.site {
            JsonStringFaultSite::Byte(offset) => {
                format!("{} at byte {}", self.reason, place(offset))
            }
            JsonStringFaultSite::End(Some(last)) => {
                format!("{} after byte {}", self.reason, place(last))
            }
            JsonStringFaultSite::End(None) => self.reason.to_owned(),
        }
    }

    /// The reason followed by its site as an offset within the token, for a
    /// token whose document position is unrecorded.
    pub(crate) fn within_token(self) -> String {
        match self.site {
            JsonStringFaultSite::Byte(offset) => {
                format!("{} at byte {offset} of the token", self.reason)
            }
            JsonStringFaultSite::End(Some(last)) => {
                format!("{} after byte {last} of the token", self.reason)
            }
            JsonStringFaultSite::End(None) => format!("{} in an empty token", self.reason),
        }
    }
}

const JSON_EOF_IN_VALUE: &str = "EOF while parsing a value";
const JSON_EOF_IN_STRING: &str = "EOF while parsing a string";
const JSON_EXPECTED_STRING: &str = "expected a JSON string";
const JSON_CONTROL_CHARACTER: &str =
    "control character (\\u0000-\\u001F) found while parsing a string";
const JSON_INVALID_ESCAPE: &str = "invalid escape";
const JSON_INVALID_HEX_DIGIT: &str = "invalid hex digit in \\u escape";
const JSON_LONE_TRAILING_SURROGATE: &str = "lone trailing surrogate in \\u escape";
const JSON_UNPAIRED_LEADING_SURROGATE: &str =
    "leading surrogate in \\u escape not followed by a trailing surrogate escape";
const JSON_TRAILING_CHARACTERS: &str = "trailing characters";

/// The fault of the JSON string token `token`, or `None` when the token is
/// one `serde_json::from_str::<String>` accepts: optional JSON whitespace, a
/// string of RFC 8259 section 7 whose `\u` escapes spell Unicode scalar
/// values (a leading surrogate escape immediately followed by a trailing
/// surrogate escape, and no other surrogate), and optional whitespace.
///
/// `serde_json` places some of its string errors after the offending byte
/// (it reads all four digits of a `\u` escape before checking them, and
/// reports a truncated escape as the end of the input), so the decoder's
/// own position is not used. The fault is found by scanning the token left
/// to right: its site is the first byte at which no completion of the bytes
/// read so far is accepted, or the token's end when every byte can still be
/// completed.
pub(crate) fn json_string_fault(token: &str) -> Option<JsonStringFault> {
    let bytes = token.as_bytes();
    let fault = |reason, offset| {
        Some(JsonStringFault {
            reason,
            site: JsonStringFaultSite::Byte(offset),
        })
    };
    let end = |reason| {
        Some(JsonStringFault {
            reason,
            site: JsonStringFaultSite::End(bytes.len().checked_sub(1)),
        })
    };
    let is_whitespace = |byte: u8| matches!(byte, b' ' | b'\t' | b'\n' | b'\r');
    let mut index = 0;
    while bytes.get(index).copied().is_some_and(is_whitespace) {
        index += 1;
    }
    match bytes.get(index).copied() {
        None => return end(JSON_EOF_IN_VALUE),
        Some(b'"') => index += 1,
        Some(_) => return fault(JSON_EXPECTED_STRING, index),
    }
    loop {
        let Some(byte) = bytes.get(index).copied() else {
            return end(JSON_EOF_IN_STRING);
        };
        match byte {
            b'"' => {
                index += 1;
                break;
            }
            b'\\' => {
                index += 1;
                match bytes.get(index).copied() {
                    None => return end(JSON_EOF_IN_STRING),
                    Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => index += 1,
                    Some(b'u') => {
                        index += 1;
                        if let Err(found) = json_unicode_escape(bytes, &mut index) {
                            return match found {
                                Some((reason, offset)) => fault(reason, offset),
                                None => end(JSON_EOF_IN_STRING),
                            };
                        }
                    }
                    Some(_) => return fault(JSON_INVALID_ESCAPE, index),
                }
            }
            0x00..=0x1F => return fault(JSON_CONTROL_CHARACTER, index),
            _ => index += 1,
        }
    }
    while bytes.get(index).copied().is_some_and(is_whitespace) {
        index += 1;
    }
    if index < bytes.len() {
        return fault(JSON_TRAILING_CHARACTERS, index);
    }
    None
}

/// The reason `serde_json` gives for `error`, without the ` at line L column
/// C` its `Display` appends whenever the line is nonzero: decoding a single
/// token, that position is relative to the token, not to the document.
pub(crate) fn json_decoder_reason(error: &serde_json::Error) -> String {
    let display = error.to_string();
    if error.line() == 0 {
        return display;
    }
    let suffix = format!(" at line {} column {}", error.line(), error.column());
    display.strip_suffix(&suffix).unwrap_or(&display).to_owned()
}

/// The value of the hex digit at `bytes[index]`. `Err(None)` means the
/// token ends there; a byte that is not a hex digit is the offending byte.
fn json_hex_digit(bytes: &[u8], index: usize) -> Result<u16, Option<(&'static str, usize)>> {
    let byte = *bytes.get(index).ok_or(None)?;
    char::from(byte)
        .to_digit(16)
        .and_then(|digit| u16::try_from(digit).ok())
        .ok_or(Some((JSON_INVALID_HEX_DIGIT, index)))
}

/// Scan the four hex digits of a `\u` escape that start at `*index`, and the
/// trailing surrogate escape that a leading surrogate needs, advancing
/// `*index` past them. `Err(None)` means the token ends first;
/// `Err(Some((reason, offset)))` names the offending byte.
fn json_unicode_escape(
    bytes: &[u8],
    index: &mut usize,
) -> Result<(), Option<(&'static str, usize)>> {
    let start = *index;
    let mut unit = 0_u16;
    for offset in 0..4 {
        unit = (unit << 4) | json_hex_digit(bytes, start + offset)?;
        // `\uDC` to `\uDF` already spell a trailing surrogate, which no
        // further digit can pair.
        if offset == 1 && (0xDC..=0xDF).contains(&unit) {
            return Err(Some((JSON_LONE_TRAILING_SURROGATE, start + offset)));
        }
    }
    *index = start + 4;
    if !(0xD800..=0xDBFF).contains(&unit) {
        return Ok(());
    }
    for (offset, expected) in [(0, b'\\'), (1, b'u')] {
        match bytes.get(*index + offset).copied() {
            None => return Err(None),
            Some(byte) if byte == expected => {}
            Some(_) => return Err(Some((JSON_UNPAIRED_LEADING_SURROGATE, *index + offset))),
        }
    }
    let start = *index + 2;
    let mut unit = 0_u16;
    for offset in 0..4 {
        unit = (unit << 4) | json_hex_digit(bytes, start + offset)?;
        // Only `\uDC` to `\uDF` begin a trailing surrogate.
        let unpaired = match offset {
            0 => unit != 0xD,
            1 => !(0xDC..=0xDF).contains(&unit),
            _ => false,
        };
        if unpaired {
            return Err(Some((JSON_UNPAIRED_LEADING_SURROGATE, start + offset)));
        }
    }
    *index = start + 4;
    Ok(())
}

struct RawJsonParser<'a> {
    input: &'a [u8],
    text: &'a str,
    pos: usize,
}

impl<'a> RawJsonParser<'a> {
    fn parse(input: &'a [u8]) -> Result<RawJson<'a>, String> {
        let text =
            std::str::from_utf8(input).map_err(|e| format!("JSON input is not UTF-8: {e}"))?;
        let mut parser = Self {
            input,
            text,
            pos: 0,
        };
        let value = parser.value(0)?;
        parser.whitespace();
        if parser.pos != parser.input.len() {
            return Err(format!("trailing JSON input at byte {}", parser.pos));
        }
        Ok(value)
    }

    /// Read one value inside `depth` enclosing containers, which puts it at
    /// level `depth + 1`.
    fn value(&mut self, depth: usize) -> Result<RawJson<'a>, String> {
        if depth >= MAX_JSON_DEPTH {
            return Err(format!("JSON nesting exceeds {MAX_JSON_DEPTH} levels"));
        }
        self.whitespace();
        match self.input.get(self.pos).copied() {
            Some(b'n') => self.literal(b"null", RawJson::Null),
            Some(b't') => self.literal(
                b"true",
                RawJson::Bool {
                    value: true,
                    offset: self.pos,
                },
            ),
            Some(b'f') => self.literal(
                b"false",
                RawJson::Bool {
                    value: false,
                    offset: self.pos,
                },
            ),
            Some(b'"') => {
                let offset = self.pos;
                self.string().map(|value| RawJson::String { value, offset })
            }
            Some(b'[') => self.array(depth + 1),
            Some(b'{') => self.object(depth + 1),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(byte) => Err(format!(
                "unexpected JSON byte 0x{byte:02X} at byte {}",
                self.pos
            )),
            None => Err("unexpected end of JSON input".to_owned()),
        }
    }

    fn literal(&mut self, literal: &[u8], value: RawJson<'a>) -> Result<RawJson<'a>, String> {
        if self.input.get(self.pos..self.pos + literal.len()) != Some(literal) {
            return Err(format!("invalid JSON literal at byte {}", self.pos));
        }
        self.pos += literal.len();
        Ok(value)
    }

    fn string(&mut self) -> Result<String, String> {
        let start = self.pos;
        self.pos += 1;
        while let Some(byte) = self.input.get(self.pos).copied() {
            match byte {
                b'"' => {
                    self.pos += 1;
                    // Both ends are quote bytes, so they are character
                    // boundaries of the validated text.
                    let token = self.text.get(start..self.pos).ok_or_else(|| {
                        format!("JSON string at byte {start} does not end on a character boundary")
                    })?;
                    return serde_json::from_str(token).map_err(|error| {
                        let origin = u64::try_from(start).unwrap_or(u64::MAX);
                        let reason = json_string_fault(token).map_or_else(
                            || json_decoder_reason(&error),
                            |fault| fault.placed_at(origin),
                        );
                        format!("invalid JSON string at byte {start}: {reason}")
                    });
                }
                b'\\' => {
                    self.pos += 1;
                    if self.pos >= self.input.len() {
                        return Err(format!("unterminated JSON escape at byte {start}"));
                    }
                    self.pos += 1;
                }
                _ => self.pos += 1,
            }
        }
        Err(format!("unterminated JSON string at byte {start}"))
    }

    fn number(&mut self) -> Result<RawJson<'a>, String> {
        let start = self.pos;
        if self.input.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        match self.input.get(self.pos).copied() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                self.pos += 1;
                while self.input.get(self.pos).is_some_and(u8::is_ascii_digit) {
                    self.pos += 1;
                }
            }
            _ => return Err(format!("invalid JSON number at byte {start}")),
        }
        if self.input.get(self.pos) == Some(&b'.') {
            self.pos += 1;
            let fraction = self.pos;
            while self.input.get(self.pos).is_some_and(u8::is_ascii_digit) {
                self.pos += 1;
            }
            if self.pos == fraction {
                return Err(format!("JSON fraction has no digits at byte {start}"));
            }
        }
        if self
            .input
            .get(self.pos)
            .is_some_and(|b| matches!(b, b'e' | b'E'))
        {
            self.pos += 1;
            if self
                .input
                .get(self.pos)
                .is_some_and(|b| matches!(b, b'+' | b'-'))
            {
                self.pos += 1;
            }
            let exponent = self.pos;
            while self.input.get(self.pos).is_some_and(u8::is_ascii_digit) {
                self.pos += 1;
            }
            if self.pos == exponent {
                return Err(format!("JSON exponent has no digits at byte {start}"));
            }
        }
        if self
            .input
            .get(self.pos)
            .is_some_and(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b',' | b']' | b'}'))
        {
            return Err(format!(
                "invalid byte after JSON number at byte {}",
                self.pos
            ));
        }
        let lexeme = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|e| format!("invalid JSON number at byte {start}: {e}"))?;
        Ok(RawJson::Number {
            lexeme,
            offset: start,
        })
    }

    fn array(&mut self, depth: usize) -> Result<RawJson<'a>, String> {
        self.pos += 1;
        self.whitespace();
        let mut values = Vec::new();
        if self.input.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(RawJson::Array(values));
        }
        loop {
            values.push(self.value(depth)?);
            self.whitespace();
            match self.input.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(RawJson::Array(values));
                }
                _ => return Err(format!("expected ',' or ']' at byte {}", self.pos)),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<RawJson<'a>, String> {
        self.pos += 1;
        self.whitespace();
        let mut fields = Vec::new();
        let mut seen = HashSet::new();
        if self.input.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(RawJson::Object(fields));
        }
        loop {
            self.whitespace();
            if self.input.get(self.pos) != Some(&b'"') {
                return Err(format!("expected JSON object key at byte {}", self.pos));
            }
            let key_offset = self.pos;
            let key = self.string()?;
            if !seen.insert(key.clone()) {
                return Err(format!("duplicate JSON object key at byte {key_offset}"));
            }
            self.whitespace();
            if self.input.get(self.pos) != Some(&b':') {
                return Err(format!("expected ':' at byte {}", self.pos));
            }
            self.pos += 1;
            fields.push((key, self.value(depth)?));
            self.whitespace();
            match self.input.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(RawJson::Object(fields));
                }
                _ => return Err(format!("expected ',' or '}}' at byte {}", self.pos)),
            }
        }
    }

    fn whitespace(&mut self) {
        while self
            .input
            .get(self.pos)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        {
            self.pos += 1;
        }
    }
}

struct BuildState {
    nodes: HashMap<u32, Node>,
    arcs: Vec<(u32, u32, Edge)>,
    next_id: u32,
    fidelity: Vec<panproto_inst::ValueFidelityError>,
}

impl BuildState {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            arcs: Vec::new(),
            next_id: 0,
            fidelity: Vec::new(),
        }
    }

    const fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Parse JSON bytes into a `WInstance` without an inexact numeric carrier.
///
/// The `root_vertex` identifies which schema vertex anchors the root of
/// the instance tree. Every number retains its source slice and byte offset
/// until it is coerced under that vertex's declared scalar kind.
///
/// # Errors
///
/// Returns [`ParseInstanceError::Json`] if the bytes are not valid JSON,
/// or [`ParseInstanceError::SchemaMismatch`] if the JSON structure does
/// not match the schema.
pub fn parse_json_bytes(
    schema: &Schema,
    root_vertex: &str,
    input: &[u8],
    protocol: &str,
) -> Result<WInstance, ParseInstanceError> {
    if !schema.has_vertex(root_vertex) {
        return Err(ParseInstanceError::SchemaMismatch {
            protocol: protocol.to_owned(),
            message: format!("root vertex {root_vertex:?} does not exist"),
        });
    }
    let raw = RawJsonParser::parse(input).map_err(ParseInstanceError::Json)?;
    let protocol_definition = protocol_definition(protocol);
    let mut state = BuildState::new();
    let root_id = state.alloc_id();
    walk_raw_json(
        schema,
        &protocol_definition,
        root_vertex,
        &raw,
        root_id,
        &mut state,
    );
    if !state.fidelity.is_empty() {
        state
            .fidelity
            .sort_by_key(|error| error.site.byte_offset.unwrap_or(u64::MAX));
        return Err(ParseInstanceError::Fidelity(state.fidelity));
    }
    Ok(WInstance::new(
        state.nodes,
        state.arcs,
        Vec::new(),
        root_id,
        Name::from(root_vertex),
    ))
}

/// Parse an already-decoded JSON value through the legacy schema-guided
/// pathway.
///
/// Byte input should use [`parse_json_bytes`] so exact number lexemes reach
/// the value layer. This wrapper remains available for callers that already
/// hold a `serde_json::Value`, including YAML and TOML conversion pathways.
///
/// # Errors
///
/// Returns [`ParseInstanceError::Parse`] if the value does not conform to the
/// schema-guided instance construction.
pub fn parse_json_value(
    schema: &Schema,
    root_vertex: &str,
    json_value: &serde_json::Value,
    protocol: &str,
) -> Result<WInstance, ParseInstanceError> {
    parse_json(schema, root_vertex, json_value).map_err(|error| ParseInstanceError::Parse {
        protocol: protocol.to_owned(),
        message: error.to_string(),
    })
}

pub(crate) fn protocol_definition(name: &str) -> Protocol {
    panproto_protocols::registry::descriptor(name).map_or_else(
        || Protocol {
            name: name.to_owned(),
            ..Protocol::default()
        },
        |descriptor| (descriptor.protocol)(),
    )
}

fn resolve_transparent_indirection<'a>(schema: &'a Schema, vertex: &'a str) -> &'a str {
    let mut current = vertex;
    let mut seen = HashSet::new();
    while seen.insert(current) {
        match schema.outgoing_edges(current) {
            [edge]
                if edge.name.is_none() && matches!(edge.kind.as_ref(), "ref" | "record-schema") =>
            {
                current = &edge.tgt;
            }
            _ => break,
        }
    }
    current
}

fn schema_edge_carriers_cohere(schema: &Schema, edge: &Edge) -> bool {
    schema
        .edges
        .get(edge)
        .is_some_and(|kind| kind == &edge.kind)
        && schema.outgoing_edges(&edge.src).contains(edge)
        && schema.incoming_edges(&edge.tgt).contains(edge)
        && schema.edges_between(&edge.src, &edge.tgt).contains(edge)
}

fn resolve_transparent_indirection_checked<'a>(
    schema: &'a Schema,
    vertex: &'a str,
) -> Result<&'a str, String> {
    let mut current = vertex;
    let mut seen = HashSet::new();
    loop {
        if !schema.has_vertex(current) {
            return Err(format!(
                "transparent target '{current}' is not a schema vertex"
            ));
        }
        if !seen.insert(current) {
            return Err(format!(
                "transparent indirection from '{vertex}' contains a cycle"
            ));
        }
        match schema.outgoing_edges(current) {
            [edge]
                if edge.name.is_none() && matches!(edge.kind.as_ref(), "ref" | "record-schema") =>
            {
                if !schema_edge_carriers_cohere(schema, edge) {
                    return Err(format!(
                        "schema edge carriers disagree for transparent edge '{} -> {}'",
                        edge.src, edge.tgt
                    ));
                }
                current = &edge.tgt;
            }
            _ => return Ok(current),
        }
    }
}

fn canonical_anchor_for_route<'a>(
    schema: &'a Schema,
    route: &'a str,
    node: &Node,
    format: &str,
    route_must_exist: bool,
) -> Result<&'a str, String> {
    if route_must_exist && !schema.has_vertex(route) {
        return Err(format!("schema route '{route}' is not a declared vertex"));
    }
    if format == "JSON" && node.value.is_none() && !node.is_list() && schema.has_vertex(route) {
        resolve_transparent_indirection_checked(schema, route)
    } else {
        Ok(route)
    }
}

fn xml_parser_child_route(schema: &Schema, parent: &str, tag: &str) -> Option<Name> {
    schema
        .outgoing_edges(parent)
        .iter()
        .find(|edge| edge.name.as_deref() == Some(tag))
        .or_else(|| {
            schema.outgoing_edges(parent).iter().find(|edge| {
                schema
                    .vertices
                    .get(&edge.tgt)
                    .is_some_and(|vertex| vertex.kind.as_ref() == tag)
            })
        })
        .map(|edge| edge.tgt.clone())
}

fn is_exact_open_toml_property(schema: &Schema, parent: &Node, child: &Node, edge: &Edge) -> bool {
    parent.value.is_none()
        && !parent.is_list()
        && schema.outgoing_edges(&parent.anchor).is_empty()
        && edge.src == parent.anchor
        && edge.kind.as_ref() == "prop"
        && edge.name.as_deref().is_some_and(|key| {
            edge.tgt.as_ref() == format!("{}:{key}", parent.anchor) && child.anchor == edge.tgt
        })
}

fn walk_raw_json(
    schema: &Schema,
    protocol: &Protocol,
    vertex: &str,
    raw: &RawJson<'_>,
    node_id: u32,
    state: &mut BuildState,
) {
    match raw {
        RawJson::Object(fields) => {
            let anchor = resolve_transparent_indirection(schema, vertex);
            let mut node = Node::new(node_id, anchor);
            if let Some(RawJson::String { value, .. }) = fields
                .iter()
                .rev()
                .find(|(key, _)| key == "$type")
                .map(|(_, value)| value)
            {
                node.discriminator = Some(Name::from(value.as_str()));
            }
            let has_string_discriminator = node.discriminator.is_some();
            let outgoing = schema.outgoing_edges(anchor).to_vec();
            let mut handled = HashSet::new();
            for edge in outgoing {
                let field = edge.name.as_deref().unwrap_or(&edge.tgt).to_owned();
                handled.insert(field.clone());
                let Some((_, child)) = fields.iter().rev().find(|(key, _)| key == &field) else {
                    continue;
                };
                let child_id = state.alloc_id();
                walk_raw_json(schema, protocol, &edge.tgt, child, child_id, state);
                state.arcs.push((node_id, child_id, edge));
            }
            for (key, value) in fields {
                if (key == "$type" && has_string_discriminator) || handled.contains(key) {
                    continue;
                }
                node.extra_fields
                    .insert(key.clone(), raw_to_value(value, anchor, state));
            }
            state.nodes.insert(node_id, node);
        }
        RawJson::Array(items) => {
            let mut node = Node::new(node_id, vertex);
            node.shape = NodeShape::List;
            state.nodes.insert(node_id, node);
            let edge = schema
                .outgoing_edges(vertex)
                .iter()
                .find(|edge| edge.name.is_none())
                .cloned();
            if let Some(edge) = edge {
                for item in items {
                    let child_id = state.alloc_id();
                    walk_raw_json(schema, protocol, &edge.tgt, item, child_id, state);
                    state.arcs.push((node_id, child_id, edge.clone()));
                }
            } else {
                let child_anchor = format!("{vertex}:items");
                let edge = Edge {
                    src: Name::from(vertex),
                    tgt: Name::from(child_anchor.as_str()),
                    kind: "item".into(),
                    name: Some("item".into()),
                };
                for item in items {
                    let child_id = state.alloc_id();
                    walk_raw_json(schema, protocol, &child_anchor, item, child_id, state);
                    state.arcs.push((node_id, child_id, edge.clone()));
                }
            }
        }
        _ => {
            let spec = ValueSpec::for_vertex(schema, protocol, &Name::from(vertex));
            let presence = raw_scalar(raw, vertex, &spec, state);
            state
                .nodes
                .insert(node_id, Node::new(node_id, vertex).with_value(presence));
        }
    }
}

fn raw_scalar(
    raw: &RawJson<'_>,
    vertex: &str,
    spec: &ValueSpec,
    state: &mut BuildState,
) -> FieldPresence {
    match raw {
        RawJson::Null => FieldPresence::Null,
        RawJson::Bool { value, offset } => coerce_or_record(
            &Lexeme::new(if *value { "true" } else { "false" }, LexClass::Bool),
            *offset,
            vertex,
            spec,
            state,
        ),
        RawJson::Number { lexeme, offset } => coerce_or_record(
            &Lexeme::new(lexeme, LexClass::Number),
            *offset,
            vertex,
            spec,
            state,
        ),
        RawJson::String { value, offset } => coerce_or_record(
            &Lexeme::new(value, LexClass::String),
            *offset,
            vertex,
            spec,
            state,
        ),
        RawJson::Array(_) | RawJson::Object(_) => {
            FieldPresence::Present(raw_to_value(raw, vertex, state))
        }
    }
}

fn coerce_or_record(
    lexeme: &Lexeme<'_>,
    offset: usize,
    vertex: &str,
    spec: &ValueSpec,
    state: &mut BuildState,
) -> FieldPresence {
    match coerce_exact_lexeme(lexeme, spec) {
        Ok(Value::Null) => FieldPresence::Null,
        Ok(value) => FieldPresence::Present(value),
        Err(error) => {
            state
                .fidelity
                .push(error.at(ValueSite::new(vertex, u64::try_from(offset).ok())));
            FieldPresence::Present(Value::Null)
        }
    }
}

fn raw_to_value(raw: &RawJson<'_>, vertex: &str, state: &mut BuildState) -> Value {
    match raw {
        RawJson::Null => Value::Null,
        RawJson::Bool { value, .. } => Value::Bool(*value),
        RawJson::Number { lexeme, offset } => {
            let any = ValueSpec::new(ScalarKind::Any);
            match coerce_exact_lexeme(&Lexeme::new(lexeme, LexClass::Number), &any) {
                Ok(value) => value,
                Err(error) => {
                    state
                        .fidelity
                        .push(error.at(ValueSite::new(vertex, u64::try_from(*offset).ok())));
                    Value::Null
                }
            }
        }
        RawJson::String { value, .. } => Value::Str(value.clone()),
        RawJson::Array(items) => Value::List(
            items
                .iter()
                .map(|item| raw_to_value(item, vertex, state))
                .collect(),
        ),
        RawJson::Object(fields) => {
            let values = fields
                .iter()
                .map(|(key, value)| (key.clone(), raw_to_value(value, vertex, state)))
                .collect();
            Value::Unknown(values)
        }
    }
}

/// Emit a `WInstance` to JSON bytes.
///
/// Exact numbers are written as JSON number tokens. A value JSON cannot
/// represent returns [`EmitInstanceError::Value`]; it is never stringified
/// with Rust's debug representation and a non-finite float is never changed
/// to `null`.
///
/// # Errors
///
/// Returns [`EmitInstanceError::Value`] if an instance value has no faithful
/// JSON form.
pub fn emit_json_bytes(
    schema: &Schema,
    instance: &WInstance,
    protocol: &str,
) -> Result<Vec<u8>, EmitInstanceError> {
    validate_canonical_tree_structure(schema, instance, protocol, "JSON")?;
    let protocol = protocol_definition(protocol);
    let mut output = Vec::new();
    write_json_node(schema, &protocol, instance, instance.root, &mut output)?;
    Ok(output)
}

/// Refuse tree instances whose graph cannot be represented by a canonical
/// tree emitter without dropping structure or metadata.
pub(crate) fn validate_canonical_tree_structure(
    schema: &Schema,
    instance: &WInstance,
    protocol: &str,
    format: &str,
) -> Result<(), EmitInstanceError> {
    CanonicalTreeValidator {
        schema,
        instance,
        protocol,
        format,
    }
    .validate()
}

struct CanonicalTreeValidator<'a> {
    schema: &'a Schema,
    instance: &'a WInstance,
    protocol: &'a str,
    format: &'a str,
}

type TreeParents = HashMap<u32, u32>;
type TreeChildren = HashMap<u32, Vec<u32>>;
type TreeIncidence = (TreeParents, TreeChildren);

impl CanonicalTreeValidator<'_> {
    const MAX_DEPTH: usize = MAX_JSON_DEPTH;

    fn error(&self, message: impl std::fmt::Display) -> EmitInstanceError {
        EmitInstanceError::Emit {
            protocol: self.protocol.to_owned(),
            message: format!("{} canonical emission refused: {message}", self.format),
        }
    }

    fn validate(&self) -> Result<(), EmitInstanceError> {
        self.validate_root_invariant()?;
        let (expected_parents, expected_children) = self.derive_tree_incidence()?;
        self.validate_node_invariants(&expected_parents)?;
        self.validate_object_route_functionality()?;
        self.validate_xml_text_partition(&expected_children)?;
        self.validate_cache_equations(&expected_parents, &expected_children)?;
        self.validate_rooted_tree_reachability(&expected_children)
    }

    fn validate_root_invariant(&self) -> Result<(), EmitInstanceError> {
        let Some(root) = self.instance.nodes.get(&self.instance.root) else {
            return Err(self.error(format!("root node {} does not exist", self.instance.root)));
        };
        if !self.schema.has_vertex(&self.instance.schema_root) {
            return Err(self.error(format!(
                "schema root '{}' does not exist in the supplied schema",
                self.instance.schema_root
            )));
        }
        let expected = canonical_anchor_for_route(
            self.schema,
            &self.instance.schema_root,
            root,
            self.format,
            true,
        )
        .map_err(|message| self.error(message))?;
        if root.anchor.as_ref() != expected {
            return Err(self.error(format!(
                "root node {} has anchor '{}' but the emitted {} syntax reparses at expected anchor '{expected}'",
                self.instance.root, root.anchor, self.format
            )));
        }
        if self.format == "XML" && root.is_xml_text_segment() {
            return Err(self.error("the document root cannot be an XML text segment".to_owned()));
        }
        if !self.instance.fans.is_empty() {
            return Err(self.error(format!(
                "{} hyper-edge fan(s) have no {} representation",
                self.instance.fans.len(),
                self.format
            )));
        }
        Ok(())
    }

    fn derive_tree_incidence(&self) -> Result<TreeIncidence, EmitInstanceError> {
        let mut parents = HashMap::with_capacity(self.instance.arcs.len());
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for (parent, child, edge) in &self.instance.arcs {
            let Some(parent_node) = self.instance.nodes.get(parent) else {
                return Err(self.error(format!("arc references missing parent node {parent}")));
            };
            let Some(child_node) = self.instance.nodes.get(child) else {
                return Err(self.error(format!("arc references missing child node {child}")));
            };
            self.validate_arc_source(*parent, *child, parent_node, child_node, edge)?;
            let declared =
                self.validate_arc_carrier(*parent, *child, parent_node, child_node, edge)?;
            self.validate_child_anchor(*child, child_node, edge, declared)?;
            if *child == self.instance.root {
                return Err(self.error("the root node has an incoming arc".to_owned()));
            }
            if parents.insert(*child, *parent).is_some() {
                return Err(self.error(format!("node {child} has more than one incoming arc")));
            }
            children.entry(*parent).or_default().push(*child);
        }
        Ok((parents, children))
    }

    fn validate_arc_source(
        &self,
        parent: u32,
        child: u32,
        parent_node: &Node,
        child_node: &Node,
        edge: &Edge,
    ) -> Result<(), EmitInstanceError> {
        if edge.src != parent_node.anchor {
            return Err(self.error(format!(
                "arc {parent} -> {child} disagrees with its endpoint anchors"
            )));
        }
        if self.format == "XML"
            && child_node.is_xml_text_segment()
            && (edge.kind.as_ref() != "prop"
                || edge.name.as_deref() != Some("$xml_text")
                || edge.tgt.as_ref() != format!("{}:text", parent_node.anchor))
        {
            return Err(self.error(format!(
                "XML text segment {child} is not carried by the parent's exact $xml_text property arc"
            )));
        }
        Ok(())
    }

    fn validate_arc_carrier(
        &self,
        parent: u32,
        child: u32,
        parent_node: &Node,
        child_node: &Node,
        edge: &Edge,
    ) -> Result<bool, EmitInstanceError> {
        let selected_list_carrier =
            if matches!(self.format, "JSON" | "TOML") && parent_node.is_list() {
                self.schema
                    .outgoing_edges(&parent_node.anchor)
                    .iter()
                    .find(|candidate| candidate.name.is_none())
            } else {
                None
            };
        let synthetic_carrier = Edge {
            src: parent_node.anchor.clone(),
            tgt: format!("{}:items", parent_node.anchor).into(),
            kind: "item".into(),
            name: Some("item".into()),
        };
        let synthetic_list = if matches!(self.format, "JSON" | "TOML") && parent_node.is_list() {
            let expected = selected_list_carrier.unwrap_or(&synthetic_carrier);
            if edge != expected {
                return Err(self.error(format!(
                    "arc {parent} -> {child} does not use the parser-selected list carrier"
                )));
            }
            selected_list_carrier.is_none()
        } else {
            false
        };
        let carrier_flags = [
            self.schema
                .edges
                .get(edge)
                .is_some_and(|kind| kind == &edge.kind),
            self.schema.outgoing_edges(&edge.src).contains(edge),
            self.schema.incoming_edges(&edge.tgt).contains(edge),
            self.schema
                .edges_between(&edge.src, &edge.tgt)
                .contains(edge),
        ];
        let declared = carrier_flags.iter().all(|flag| *flag);
        if carrier_flags.iter().any(|flag| *flag) && !declared {
            return Err(self.error(format!(
                "schema edge carriers disagree for arc {parent} -> {child}"
            )));
        }
        if declared && (!self.schema.has_vertex(&edge.src) || !self.schema.has_vertex(&edge.tgt)) {
            return Err(self.error(format!(
                "declared arc {parent} -> {child} has a missing schema endpoint"
            )));
        }
        let open_xml = self.format == "XML"
            && (child_node.xml_tag().is_some_and(|tag| {
                xml_parser_child_route(self.schema, &parent_node.anchor, tag).is_none()
                    && edge.src == parent_node.anchor
                    && edge.tgt.as_ref() == format!("{}:{tag}", parent_node.anchor)
                    && child_node.anchor == edge.tgt
                    && edge.kind.as_ref() == "contains"
                    && edge
                        .name
                        .as_deref()
                        .is_some_and(|name| name == tag.as_ref())
            }) || (child_node.is_xml_text_segment()
                && edge.kind.as_ref() == "prop"
                && edge.name.as_deref() == Some("$xml_text")));
        let open_toml = self.format == "TOML"
            && is_exact_open_toml_property(self.schema, parent_node, child_node, edge);
        if !declared && !open_xml && !open_toml && !synthetic_list {
            return Err(self.error(format!(
                "arc {parent} -> {child} is not declared by the supplied schema"
            )));
        }
        Ok(declared)
    }

    fn validate_child_anchor(
        &self,
        child: u32,
        child_node: &Node,
        edge: &Edge,
        declared: bool,
    ) -> Result<(), EmitInstanceError> {
        let expected =
            canonical_anchor_for_route(self.schema, &edge.tgt, child_node, self.format, declared)
                .map_err(|message| self.error(message))?;
        if child_node.anchor.as_ref() != expected {
            return Err(self.error(format!(
                "child node {child} has anchor '{}' but the emitted {} syntax reparses at expected anchor '{expected}'",
                child_node.anchor, self.format
            )));
        }
        Ok(())
    }

    fn validate_node_invariants(
        &self,
        expected_parents: &HashMap<u32, u32>,
    ) -> Result<(), EmitInstanceError> {
        for (&node_id, node) in &self.instance.nodes {
            if node.id != node_id {
                return Err(self.error(format!(
                    "node map key {node_id} disagrees with stored id {}",
                    node.id
                )));
            }
            if node_id != self.instance.root && !expected_parents.contains_key(&node_id) {
                return Err(self.error(format!("node {node_id} is unreachable from the root")));
            }
            if node.position.is_some() {
                return Err(self.error(format!(
                    "node {node_id} carries position metadata with no {} representation",
                    self.format
                )));
            }
            if !node.annotations.is_empty() {
                return Err(self.error(format!(
                    "node {node_id} carries annotations with no {} representation",
                    self.format
                )));
            }
            if self.format == "XML"
                && !node.is_xml_text_segment()
                && !matches!(node.shape, NodeShape::XmlElement { .. })
            {
                return Err(self.error(format!(
                    "XML element node {node_id} does not use the XML element shape"
                )));
            }
        }
        Ok(())
    }

    fn validate_object_route_functionality(&self) -> Result<(), EmitInstanceError> {
        if !matches!(self.format, "JSON" | "TOML") {
            return Ok(());
        }
        for (&node_id, node) in &self.instance.nodes {
            if node.value.is_some() || node.is_list() {
                continue;
            }
            let routes = self.schema.outgoing_edges(&node.anchor);
            let matching_routes = |key: &str| {
                routes
                    .iter()
                    .filter(|edge| edge.name.as_deref().unwrap_or(&edge.tgt) == key)
                    .collect::<Vec<_>>()
            };
            if self.format == "JSON"
                && node.discriminator.is_some()
                && !matching_routes("$type").is_empty()
            {
                return Err(self.error(format!(
                    "JSON discriminator on node {node_id} is also claimed by a declared field route"
                )));
            }
            if self.format == "TOML" && routes.is_empty() && !node.extra_fields.is_empty() {
                return Err(self.error(format!(
                    "TOML open table node {node_id} carries extra fields that would reparse as child nodes"
                )));
            }
            self.validate_child_route_functionality(node_id, node, routes)?;
            for key in node.extra_fields.keys() {
                if !matching_routes(key).is_empty() {
                    return Err(self.error(format!(
                        "{} extra field '{key}' on node {node_id} is claimed by a declared field route",
                        self.format
                    )));
                }
            }
        }
        Ok(())
    }

    fn validate_child_route_functionality(
        &self,
        node_id: u32,
        node: &Node,
        routes: &[Edge],
    ) -> Result<(), EmitInstanceError> {
        for (parent, child, edge) in &self.instance.arcs {
            if *parent != node_id
                || self
                    .instance
                    .node(*child)
                    .is_some_and(|child| matches!(child.value, Some(FieldPresence::Absent)))
            {
                continue;
            }
            let key = edge.name.as_deref().unwrap_or(&edge.tgt);
            let matching = routes
                .iter()
                .filter(|route| route.name.as_deref().unwrap_or(&route.tgt) == key)
                .collect::<Vec<_>>();
            let open_toml = self.format == "TOML"
                && self.instance.node(*child).is_some_and(|child_node| {
                    is_exact_open_toml_property(self.schema, node, child_node, edge)
                });
            if !open_toml && (matching.len() != 1 || matching[0] != edge) {
                return Err(self.error(format!(
                    "{} field '{key}' on node {node_id} does not have one unambiguous parser route",
                    self.format
                )));
            }
        }
        Ok(())
    }

    fn validate_xml_text_partition(
        &self,
        expected_children: &HashMap<u32, Vec<u32>>,
    ) -> Result<(), EmitInstanceError> {
        if self.format != "XML" {
            return Ok(());
        }
        for (&parent, children) in expected_children {
            let emitted = children
                .iter()
                .filter_map(|child| self.instance.node(*child))
                .filter(|child| !matches!(child.value, Some(FieldPresence::Absent)))
                .collect::<Vec<_>>();
            if emitted.iter().any(|child| child.is_xml_text_segment())
                && !emitted.iter().any(|child| !child.is_xml_text_segment())
            {
                return Err(self.error(format!(
                    "XML text segments under node {parent} require an emitted element sibling"
                )));
            }
            if emitted
                .windows(2)
                .any(|pair| pair[0].is_xml_text_segment() && pair[1].is_xml_text_segment())
            {
                return Err(self.error(format!(
                    "adjacent XML text segments under node {parent} would coalesce on reparse"
                )));
            }
        }
        Ok(())
    }

    fn validate_cache_equations(
        &self,
        expected_parents: &HashMap<u32, u32>,
        expected_children: &HashMap<u32, Vec<u32>>,
    ) -> Result<(), EmitInstanceError> {
        if self.instance.parent_map != *expected_parents {
            return Err(self.error("the cached parent map disagrees with the arc set".to_owned()));
        }
        if self.instance.children_map.len() != expected_children.len()
            || expected_children.iter().any(|(parent, children)| {
                self.instance
                    .children_map
                    .get(parent)
                    .is_none_or(|actual| actual.as_slice() != children.as_slice())
            })
        {
            return Err(
                self.error("the cached children map disagrees with the ordered arc set".to_owned())
            );
        }
        Ok(())
    }

    fn validate_rooted_tree_reachability(
        &self,
        expected_children: &HashMap<u32, Vec<u32>>,
    ) -> Result<(), EmitInstanceError> {
        let mut pending = vec![(self.instance.root, 1_usize)];
        let mut visited = HashSet::with_capacity(self.instance.nodes.len());
        while let Some((node_id, depth)) = pending.pop() {
            if depth > Self::MAX_DEPTH {
                return Err(self.error(format!(
                    "tree depth exceeds the supported limit of {} nodes",
                    Self::MAX_DEPTH
                )));
            }
            if !visited.insert(node_id) {
                return Err(self.error(format!("node {node_id} is reached more than once")));
            }
            if let Some(children) = expected_children.get(&node_id) {
                pending.extend(
                    children
                        .iter()
                        .rev()
                        .copied()
                        .map(|child| (child, depth + 1)),
                );
            }
        }
        if visited.len() != self.instance.nodes.len() {
            return Err(self.error(format!(
                "root traversal reaches {} of {} nodes",
                visited.len(),
                self.instance.nodes.len()
            )));
        }
        Ok(())
    }
}

/// Convert an instance to an in-memory JSON value through the legacy
/// non-byte pathway.
///
/// Use [`emit_json_bytes`] when exact typed values and structured emission
/// errors matter. This wrapper preserves the public API used by callers that
/// need a `serde_json::Value` for another format conversion.
#[must_use]
pub fn emit_json_value(schema: &Schema, instance: &WInstance) -> serde_json::Value {
    to_json(schema, instance)
}

fn write_json_node(
    schema: &Schema,
    protocol: &Protocol,
    instance: &WInstance,
    node_id: u32,
    output: &mut Vec<u8>,
) -> Result<(), EmitInstanceError> {
    let node = instance
        .node(node_id)
        .ok_or_else(|| EmitInstanceError::Emit {
            protocol: protocol.name.clone(),
            message: format!("node {node_id} does not exist"),
        })?;
    validate_json_node_invariants(protocol, instance, node_id, node)?;
    if let Some(presence) = &node.value {
        return write_json_presence(
            presence,
            &ValueSpec::for_vertex(schema, protocol, &node.anchor),
            &ValueSite::new(node.anchor.clone(), None),
            output,
        );
    }
    if node.is_list() {
        return write_json_sequence(schema, protocol, instance, node_id, output);
    }
    write_json_mapping(schema, protocol, instance, node_id, node, output)
}

fn validate_json_node_invariants(
    protocol: &Protocol,
    instance: &WInstance,
    node_id: u32,
    node: &Node,
) -> Result<(), EmitInstanceError> {
    let has_children = instance
        .children_map
        .get(&node_id)
        .is_some_and(|children| !children.is_empty());
    if !matches!(&node.shape, NodeShape::Plain | NodeShape::List) {
        return Err(EmitInstanceError::Emit {
            protocol: protocol.name.clone(),
            message: format!("JSON node {node_id} carries an incompatible structural shape"),
        });
    }
    if node.value.is_some()
        && (node.is_list()
            || has_children
            || node.discriminator.is_some()
            || !node.extra_fields.is_empty())
    {
        return Err(EmitInstanceError::Emit {
            protocol: protocol.name.clone(),
            message: format!(
                "JSON scalar node {node_id} also carries object fields, a discriminator, or children"
            ),
        });
    }
    if node.is_list() && (node.discriminator.is_some() || !node.extra_fields.is_empty()) {
        return Err(EmitInstanceError::Emit {
            protocol: protocol.name.clone(),
            message: format!("JSON list node {node_id} also carries object metadata"),
        });
    }
    Ok(())
}

fn write_json_sequence(
    schema: &Schema,
    protocol: &Protocol,
    instance: &WInstance,
    node_id: u32,
    output: &mut Vec<u8>,
) -> Result<(), EmitInstanceError> {
    output.push(b'[');
    for (index, child) in instance.children(node_id).iter().enumerate() {
        if index > 0 {
            output.push(b',');
        }
        let child_node = instance
            .node(*child)
            .ok_or_else(|| EmitInstanceError::Emit {
                protocol: protocol.name.clone(),
                message: format!("child node {child} does not exist"),
            })?;
        if matches!(child_node.value, Some(FieldPresence::Absent)) {
            return Err(EmitValueError::new(
                ValueSite::new(child_node.anchor.clone(), None),
                "absent",
                "json",
                EmitValueReason::AbsentNotRepresentable,
            )
            .into());
        }
        write_json_node(schema, protocol, instance, *child, output)?;
    }
    output.push(b']');
    Ok(())
}

fn write_json_mapping(
    schema: &Schema,
    protocol: &Protocol,
    instance: &WInstance,
    node_id: u32,
    node: &Node,
    output: &mut Vec<u8>,
) -> Result<(), EmitInstanceError> {
    let site = ValueSite::new(node.anchor.clone(), None);

    let mut fields: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    if node.discriminator.is_none() && matches!(node.extra_fields.get("$type"), Some(Value::Str(_)))
    {
        return Err(EmitInstanceError::Emit {
            protocol: protocol.name.clone(),
            message: "string $type data would reparse as a JSON discriminator".to_owned(),
        });
    }
    if let Some(discriminator) = &node.discriminator {
        let mut value = Vec::new();
        write_json_string(discriminator, &mut value)?;
        fields.insert("$type".to_owned(), value);
    }
    for (parent, child, edge) in &instance.arcs {
        if *parent != node_id {
            continue;
        }
        if edge.name.as_deref() == Some("$type") {
            return Err(EmitInstanceError::Emit {
                protocol: protocol.name.clone(),
                message: format!(
                    "JSON node {node_id} has a declared $type child whose data carrier conflicts with the discriminator syntax"
                ),
            });
        }
        let child_node = instance
            .node(*child)
            .ok_or_else(|| EmitInstanceError::Emit {
                protocol: protocol.name.clone(),
                message: format!("child node {child} does not exist"),
            })?;
        if matches!(child_node.value, Some(FieldPresence::Absent)) {
            let has_grandchildren = instance
                .children_map
                .get(child)
                .is_some_and(|children| !children.is_empty());
            if has_grandchildren
                || child_node.discriminator.is_some()
                || !child_node.extra_fields.is_empty()
                || !matches!(&child_node.shape, NodeShape::Plain)
            {
                return Err(EmitInstanceError::Emit {
                    protocol: protocol.name.clone(),
                    message: format!(
                        "absent JSON child node {child} carries structure that omission would discard"
                    ),
                });
            }
            continue;
        }
        let key = edge.name.as_deref().unwrap_or(&edge.tgt).to_owned();
        ensure_unique_json_key(&fields, &key, protocol)?;
        let mut value = Vec::new();
        write_json_node(schema, protocol, instance, *child, &mut value)?;
        fields.insert(key, value);
    }
    let mut extra_fields: Vec<_> = node.extra_fields.iter().collect();
    extra_fields.sort_by_key(|(key, _)| *key);
    for (key, value) in extra_fields {
        ensure_unique_json_key(&fields, key, protocol)?;
        let mut rendered = Vec::new();
        write_json_value(
            value,
            &ValueSpec::new(ScalarKind::Any),
            &site,
            &mut rendered,
        )?;
        fields.insert(key.clone(), rendered);
    }
    output.push(b'{');
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            output.push(b',');
        }
        write_json_string(key, output)?;
        output.push(b':');
        output.extend_from_slice(value);
    }
    output.push(b'}');
    Ok(())
}

fn ensure_unique_json_key(
    fields: &BTreeMap<String, Vec<u8>>,
    key: &str,
    protocol: &Protocol,
) -> Result<(), EmitInstanceError> {
    if fields.contains_key(key) {
        return Err(EmitInstanceError::Emit {
            protocol: protocol.name.clone(),
            message: format!("duplicate emitted JSON object key {key:?}"),
        });
    }
    Ok(())
}

fn write_json_presence(
    presence: &FieldPresence,
    spec: &ValueSpec,
    site: &ValueSite,
    output: &mut Vec<u8>,
) -> Result<(), EmitInstanceError> {
    match presence {
        FieldPresence::Present(value @ (Value::Null | Value::Unknown(_) | Value::List(_))) => {
            Err(EmitValueError::new(
                site.clone(),
                value.type_name(),
                "json",
                EmitValueReason::NotRepresentable,
            )
            .into())
        }
        FieldPresence::Present(value) => write_json_value(value, spec, site, output),
        FieldPresence::Null => {
            output.extend_from_slice(b"null");
            Ok(())
        }
        FieldPresence::Absent => Err(EmitValueError::new(
            site.clone(),
            "absent",
            "json",
            EmitValueReason::AbsentNotRepresentable,
        )
        .into()),
    }
}

fn write_json_value(
    value: &Value,
    spec: &ValueSpec,
    site: &ValueSite,
    output: &mut Vec<u8>,
) -> Result<(), EmitInstanceError> {
    write_json_value_at_depth(value, spec, site, output, 0)
}

fn write_json_value_at_depth(
    value: &Value,
    spec: &ValueSpec,
    site: &ValueSite,
    output: &mut Vec<u8>,
    depth: usize,
) -> Result<(), EmitInstanceError> {
    if depth >= MAX_VALUE_EMIT_DEPTH
        && matches!(
            value,
            Value::Unknown(_) | Value::List(_) | Value::Opaque { .. }
        )
    {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "json",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    if matches!(value, Value::Float(number) if !number.is_finite()) {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "json",
            EmitValueReason::NonFinite,
        )
        .into());
    }
    if !json_value_roundtrips_at_depth(value, spec, depth) {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "json",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    match value {
        Value::Bool(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
        Value::Int(value) => output.extend_from_slice(value.to_string().as_bytes()),
        Value::Float(value) => {
            output.extend_from_slice(json_float_text(*value).as_bytes());
        }
        Value::Str(value) => write_json_string(value, output)?,
        Value::Token(value) => {
            write_json_string(value, output)?;
        }
        Value::Bytes(bytes) => {
            let ScalarKind::Bytes(encoding) = &spec.kind else {
                return Err(EmitValueError::new(
                    site.clone(),
                    value.type_name(),
                    "json",
                    EmitValueReason::NotRepresentable,
                )
                .into());
            };
            let encoded = encode_bytes(bytes, encoding).ok_or_else(|| {
                EmitValueError::new(
                    site.clone(),
                    value.type_name(),
                    "json",
                    EmitValueReason::NotRepresentable,
                )
            })?;
            write_json_string(&encoded, output)?;
        }
        Value::CidLink(link) => {
            output.extend_from_slice(b"{\"$link\":");
            write_json_string(link, output)?;
            output.push(b'}');
        }
        Value::Blob { ref_, mime, size } => {
            output.extend_from_slice(b"{\"$type\":\"blob\",\"mimeType\":");
            write_json_string(mime, output)?;
            output.extend_from_slice(b",\"ref\":");
            write_json_string(ref_, output)?;
            output.extend_from_slice(b",\"size\":");
            output.extend_from_slice(size.to_string().as_bytes());
            output.push(b'}');
        }
        Value::Null => output.extend_from_slice(b"null"),
        Value::Opaque { type_, fields } => {
            write_json_object(fields, Some(type_), site, output, depth + 1)?;
        }
        Value::Unknown(fields) => write_json_object(fields, None, site, output, depth + 1)?,
        Value::List(items) => write_json_list(items, site, output, depth + 1)?,
        Value::Decimal(value) => output.extend_from_slice(value.to_string().as_bytes()),
        Value::BigInt(value) => output.extend_from_slice(value.to_string().as_bytes()),
        Value::Temporal(temporal) => {
            write_json_string(&temporal.to_string(), output)?;
        }
        Value::LabeledNull(_) => {
            return Err(EmitValueError::new(
                site.clone(),
                value.type_name(),
                "json",
                EmitValueReason::NotRepresentable,
            )
            .into());
        }
    }
    Ok(())
}

/// Whether JSON emission followed by schema-guided parsing preserves `value`.
///
/// This checks the actual lexical class JSON writes, rather than only the
/// declared kind. That distinction matters for open schemas: `0.1` is a
/// `Float` under `Any`, while a longer numeral may remain a `Decimal`.
pub(crate) fn json_value_roundtrips(value: &Value, spec: &ValueSpec) -> bool {
    json_value_roundtrips_at_depth(value, spec, 0)
}

fn json_value_roundtrips_at_depth(value: &Value, spec: &ValueSpec, depth: usize) -> bool {
    if depth >= MAX_VALUE_EMIT_DEPTH
        && matches!(
            value,
            Value::Unknown(_) | Value::List(_) | Value::Opaque { .. }
        )
    {
        return false;
    }
    let reparsed = match value {
        Value::Bool(value) => coerce(
            &Lexeme::new(if *value { "true" } else { "false" }, LexClass::Bool),
            spec,
        ),
        Value::Int(value) => coerce(&Lexeme::new(&value.to_string(), LexClass::Number), spec),
        Value::Float(value) if value.is_finite() => coerce_exact_lexeme(
            &Lexeme::new(&json_float_text(*value), LexClass::Number),
            spec,
        ),
        Value::Str(value) | Value::Token(value) => {
            coerce(&Lexeme::new(value, LexClass::String), spec)
        }
        Value::Bytes(bytes) => {
            let ScalarKind::Bytes(encoding) = &spec.kind else {
                return false;
            };
            let Some(encoded) = encode_bytes(bytes, encoding) else {
                return false;
            };
            coerce(&Lexeme::new(&encoded, LexClass::String), spec)
        }
        Value::Null => coerce(&Lexeme::new("null", LexClass::Null), spec),
        Value::Decimal(value) => coerce(&Lexeme::new(&value.to_string(), LexClass::Number), spec),
        Value::BigInt(value) => coerce(&Lexeme::new(&value.to_string(), LexClass::Number), spec),
        Value::Temporal(value) => coerce(&Lexeme::new(&value.to_string(), LexClass::String), spec),
        Value::Unknown(fields) if spec.kind == ScalarKind::Any => {
            return fields.values().all(|field| {
                json_value_roundtrips_at_depth(field, &ValueSpec::new(ScalarKind::Any), depth + 1)
            });
        }
        Value::List(items) if spec.kind == ScalarKind::Any => {
            return items.iter().all(|item| {
                json_value_roundtrips_at_depth(item, &ValueSpec::new(ScalarKind::Any), depth + 1)
            });
        }
        Value::Float(_)
        | Value::CidLink(_)
        | Value::Blob { .. }
        | Value::Opaque { .. }
        | Value::Unknown(_)
        | Value::List(_)
        | Value::LabeledNull(_) => return false,
    };
    reparsed.is_ok_and(|actual| json_values_match(value, &actual))
}

fn json_values_match(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Float(expected), Value::Float(actual)) => expected.to_bits() == actual.to_bits(),
        _ => expected == actual,
    }
}

fn write_json_object(
    fields: &HashMap<String, Value>,
    type_name: Option<&str>,
    site: &ValueSite,
    output: &mut Vec<u8>,
    depth: usize,
) -> Result<(), EmitInstanceError> {
    let ordered: BTreeMap<&str, &Value> = fields
        .iter()
        .map(|(key, value)| (key.as_str(), value))
        .collect();
    output.push(b'{');
    if let Some(type_name) = type_name {
        write_json_string("$type", output)?;
        output.push(b':');
        write_json_string(type_name, output)?;
    }
    for (index, (key, value)) in ordered.iter().enumerate() {
        if type_name.is_some() || index > 0 {
            output.push(b',');
        }
        write_json_string(key, output)?;
        output.push(b':');
        write_json_value_at_depth(value, &ValueSpec::new(ScalarKind::Any), site, output, depth)?;
    }
    output.push(b'}');
    Ok(())
}

fn write_json_list(
    items: &[Value],
    site: &ValueSite,
    output: &mut Vec<u8>,
    depth: usize,
) -> Result<(), EmitInstanceError> {
    output.push(b'[');
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            output.push(b',');
        }
        write_json_value_at_depth(item, &ValueSpec::new(ScalarKind::Any), site, output, depth)?;
    }
    output.push(b']');
    Ok(())
}

fn write_json_string(value: &str, output: &mut Vec<u8>) -> Result<(), EmitInstanceError> {
    let encoded =
        serde_json::to_string(value).map_err(|error| EmitInstanceError::Json(error.to_string()))?;
    output.extend_from_slice(encoded.as_bytes());
    Ok(())
}

fn json_float_text(value: f64) -> String {
    let mut text = value.to_string();
    if !text.contains(['.', 'e', 'E']) {
        text.push_str(".0");
    }
    text
}

pub(crate) fn encode_bytes(bytes: &[u8], encoding: &BytesText) -> Option<String> {
    match encoding {
        BytesText::Base64 => Some(encode_base64(
            bytes,
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
        )),
        BytesText::Base64Url => Some(encode_base64(
            bytes,
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
        )),
        BytesText::Hex => {
            let mut result = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                use std::fmt::Write as _;
                let _ = write!(result, "{byte:02x}");
            }
            Some(result)
        }
        BytesText::Raw => std::str::from_utf8(bytes).ok().map(ToOwned::to_owned),
    }
}

fn encode_base64(bytes: &[u8], alphabet: &[u8; 64]) -> String {
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = u32::from(chunk[0]);
        let second = u32::from(chunk.get(1).copied().unwrap_or_default());
        let third = u32::from(chunk.get(2).copied().unwrap_or_default());
        let triple = (first << 16) | (second << 8) | third;
        result.push(char::from(alphabet[((triple >> 18) & 0x3f) as usize]));
        result.push(char::from(alphabet[((triple >> 12) & 0x3f) as usize]));
        if chunk.len() > 1 {
            result.push(char::from(alphabet[((triple >> 6) & 0x3f) as usize]));
        }
        if chunk.len() > 2 {
            result.push(char::from(alphabet[(triple & 0x3f) as usize]));
        }
        match chunk.len() {
            1 => result.push_str("=="),
            2 => result.push('='),
            _ => {}
        }
    }
    result
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use panproto_inst::value::DecimalValue;
    use panproto_schema::SchemaBuilder;

    #[test]
    fn plain_text_float_spellings_preserve_supported_nonfinite_values() {
        assert_eq!(plain_text_float_lexeme(f64::NAN).as_deref(), Some("NaN"));
        assert_eq!(
            plain_text_float_lexeme(f64::INFINITY).as_deref(),
            Some("Infinity")
        );
        assert_eq!(
            plain_text_float_lexeme(f64::NEG_INFINITY).as_deref(),
            Some("-Infinity")
        );
        assert_eq!(plain_text_float_lexeme(-f64::NAN).as_deref(), Some("-NaN"));
        let payload_nan = f64::from_bits(f64::NAN.to_bits() + 1);
        assert!(payload_nan.is_nan());
        assert_eq!(plain_text_float_lexeme(payload_nan), None);
    }

    #[test]
    fn plain_text_preflight_checks_the_complete_value_spec() {
        let int8 = ValueSpec::new(ScalarKind::Int {
            bits: 8,
            signed: true,
        });
        assert!(plain_text_value_roundtrips(&Value::Int(127), &int8));
        assert!(!plain_text_value_roundtrips(&Value::Int(256), &int8));

        let decimal_2 = ValueSpec {
            scale: Some(2),
            ..ValueSpec::new(ScalarKind::Decimal)
        };
        let Ok(decimal) = DecimalValue::parse("1.005") else {
            panic!("valid decimal fixture");
        };
        assert!(!plain_text_value_roundtrips(
            &Value::Decimal(decimal),
            &decimal_2
        ));
    }

    fn simple_schema() -> Schema {
        // A minimal schema with a root object and one string property.
        let proto = panproto_schema::Protocol {
            name: "test".into(),
            schema_theory: "ThTestSchema".into(),
            instance_theory: "ThTestInstance".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into()],
            constraint_sorts: vec![],
            ..Default::default()
        };
        SchemaBuilder::new(&proto)
            .vertex("root", "object", None)
            .expect("vertex")
            .vertex("root:name", "string", None)
            .expect("vertex")
            .edge("root", "root:name", "prop", Some("name"))
            .expect("edge")
            .build()
            .expect("build")
    }

    fn number_schema() -> Schema {
        let proto = panproto_schema::Protocol {
            name: "test".into(),
            schema_theory: "ThTestSchema".into(),
            instance_theory: "ThTestInstance".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "number".into()],
            constraint_sorts: vec![],
            ..Default::default()
        };
        SchemaBuilder::new(&proto)
            .vertex("root", "object", None)
            .expect("root")
            .vertex("root:integer", "number", None)
            .expect("integer")
            .vertex("root:decimal", "number", None)
            .expect("decimal")
            .edge("root", "root:integer", "prop", Some("integer"))
            .expect("integer edge")
            .edge("root", "root:decimal", "prop", Some("decimal"))
            .expect("decimal edge")
            .build()
            .expect("build")
    }

    #[test]
    fn roundtrip_json_bytes() {
        let schema = simple_schema();
        let input = br#"{"name": "Alice"}"#;

        let instance =
            parse_json_bytes(&schema, "root", input, "test").expect("parse should succeed");
        assert!(
            instance.node_count() >= 2,
            "should have at least root + name nodes"
        );

        let emitted = emit_json_bytes(&schema, &instance, "test").expect("emit should succeed");
        let instance2 =
            parse_json_bytes(&schema, "root", &emitted, "test").expect("re-parse should succeed");

        assert_eq!(instance.node_count(), instance2.node_count());
        assert_eq!(instance.arc_count(), instance2.arc_count());
    }

    #[test]
    fn invalid_json_returns_error() {
        let schema = simple_schema();
        let input = b"not valid json {{{";
        let result = parse_json_bytes(&schema, "root", input, "test");
        assert!(result.is_err());
    }

    #[test]
    fn duplicate_object_keys_are_rejected_at_every_depth() {
        let schema = simple_schema();
        for input in [
            br#"{"same":1,"same":2}"#.as_slice(),
            br#"{"outer":{"same":1,"same":2}}"#.as_slice(),
            br#"{"$type":"a","$type":"b"}"#.as_slice(),
        ] {
            assert!(matches!(
                parse_json_bytes(&schema, "root", input, "test"),
                Err(ParseInstanceError::Json(ref message))
                    if message.contains("duplicate JSON object key")
            ));
        }
    }

    #[test]
    fn a_non_string_type_member_remains_ordinary_data() {
        let schema = simple_schema();
        let input = br#"{"$type":7}"#;
        let instance = parse_json_bytes(&schema, "root", input, "test").expect("parse $type");
        let root = instance.node(instance.root).expect("root");
        assert!(root.discriminator.is_none());
        assert_eq!(root.extra_fields.get("$type"), Some(&Value::Int(7)));
        assert_eq!(
            emit_json_bytes(&schema, &instance, "test").expect("emit $type"),
            input
        );
    }

    #[test]
    fn json_parse_errors_name_input_bytes_and_document_offsets() {
        let message = |input: &[u8]| match parse_json_bytes(&simple_schema(), "root", input, "test")
        {
            Err(ParseInstanceError::Json(message)) => message,
            other => panic!("{input:?} must be a JSON error, got {other:?}"),
        };
        // A non-ASCII byte is named by its value, not by the Latin-1 character
        // of that value: a UTF-8 byte order mark starts with 0xEF.
        assert_eq!(
            message("\u{feff}{\"integer\":1}".as_bytes()),
            "unexpected JSON byte 0xEF at byte 0"
        );
        assert_eq!(
            message(b"{\"integer\":@}"),
            "unexpected JSON byte 0x40 at byte 11"
        );
        // A bad hex digit is named itself, not the escape's last digit.
        assert_eq!(
            message(br#"{"integer":"\uZZZZ"}"#),
            "invalid JSON string at byte 11: invalid hex digit in \\u escape at byte 14"
        );
        assert_eq!(
            message(br#"{"integer":"\u12Z4"}"#),
            "invalid JSON string at byte 11: invalid hex digit in \\u escape at byte 16"
        );
        // A closing quote cut into the escape is the offending byte.
        assert_eq!(
            message(br#"{"integer":"\u1"}"#),
            "invalid JSON string at byte 11: invalid hex digit in \\u escape at byte 15"
        );
        // `\udc` already spells a trailing surrogate, so its `c`, byte 15,
        // is the offending byte.
        assert_eq!(
            message(br#"{"integer":"\udc00"}"#),
            "invalid JSON string at byte 11: lone trailing surrogate in \\u escape at byte 15"
        );
        // The invalid escape character `q` is byte 13.
        assert_eq!(
            message(br#"{"integer":"\q"}"#),
            "invalid JSON string at byte 11: invalid escape at byte 13"
        );
        // A string error is placed in the document, not in the string slice,
        // at the offending byte: the first byte at which the token can no
        // longer be completed. The token spans bytes 11-18; the leading
        // surrogate needs a trailing surrogate escape, and the closing quote,
        // byte 18, is the first byte that rules it out.
        assert_eq!(
            message(br#"{"integer":"\ud800"}"#),
            "invalid JSON string at byte 11: leading surrogate in \\u escape not followed by \
             a trailing surrogate escape at byte 18"
        );
        // A raw line feed inside a string is a control character, and the
        // offending byte is the line feed itself, byte 13.
        assert_eq!(
            message(b"{\"integer\":\"a\nb\"}"),
            "invalid JSON string at byte 11: control character (\\u0000-\\u001F) found while \
             parsing a string at byte 13"
        );
        // The control character 0x01 is byte 14.
        assert_eq!(
            message(b"{\"integer\":\"ab\x01\"}"),
            "invalid JSON string at byte 11: control character (\\u0000-\\u001F) found while \
             parsing a string at byte 14"
        );
    }

    /// Completions that finish every prefix of an accepted string token. A
    /// prefix ends in one of these states, each finished by the listed
    /// completion: before or inside leading whitespace (`""`); inside the
    /// string outside any escape (`"`); just after `\` (`n"`); after `\u`
    /// and k < 4 hex digits (4 - k zeros, then `"`, or then `\udc00"` when
    /// the digits so far begin a leading surrogate); after a complete leading
    /// surrogate escape (`\udc00"`), its `\` (`udc00"`), its `\u` (`dc00"`),
    /// or one to three digits of the trailing escape (`c00"`, `00"`, `0"`);
    /// after the closing quote (nothing). A prefix ending inside a multibyte
    /// character is never formed below.
    const STRING_TOKEN_COMPLETIONS: [&str; 16] = [
        "",
        "\"",
        "\"\"",
        "n\"",
        "0\"",
        "00\"",
        "000\"",
        "0000\"",
        "0\\udc00\"",
        "00\\udc00\"",
        "000\\udc00\"",
        "0000\\udc00\"",
        "\\udc00\"",
        "udc00\"",
        "dc00\"",
        "c00\"",
    ];

    /// The units the exhaustive token enumeration below is built from.
    const STRING_TOKEN_UNITS: [&str; 14] = [
        "\"", "\\", "u", "d", "8", "c", "0", "Z", "n", " ", "\u{1}", "\u{e9}", "\\ud800", "\\udc00",
    ];

    fn string_prefix_completes(prefix: &[u8]) -> bool {
        STRING_TOKEN_COMPLETIONS.iter().any(|completion| {
            serde_json::from_slice::<String>(&[prefix, completion.as_bytes()].concat()).is_ok()
        })
    }

    #[test]
    fn json_string_faults_match_serde_json_and_name_the_first_uncompletable_byte() {
        let mut tokens = vec![String::new()];
        let mut frontier = vec![String::new()];
        for _ in 0..5 {
            frontier = frontier
                .iter()
                .flat_map(|token| {
                    STRING_TOKEN_UNITS
                        .iter()
                        .map(move |unit| format!("{token}{unit}"))
                })
                .collect();
            tokens.extend(frontier.iter().cloned());
        }
        let mut refused = 0_usize;
        for token in &tokens {
            let accepted = serde_json::from_str::<String>(token).is_ok();
            let fault = json_string_fault(token);
            assert_eq!(accepted, fault.is_none(), "{token:?}: {fault:?}");
            let Some(fault) = fault else { continue };
            refused += 1;
            let bytes = token.as_bytes();
            match fault.site {
                JsonStringFaultSite::Byte(offset) => {
                    let Some(width) = token.get(offset..).and_then(|rest| rest.chars().next())
                    else {
                        panic!("{token:?}: fault offset {offset} is not a character start");
                    };
                    assert!(
                        string_prefix_completes(&bytes[..offset]),
                        "{token:?}: the bytes before {offset} cannot be completed"
                    );
                    assert!(
                        !string_prefix_completes(&bytes[..offset + width.len_utf8()]),
                        "{token:?}: the bytes through {offset} can still be completed"
                    );
                }
                JsonStringFaultSite::End(last) => {
                    assert_eq!(last, bytes.len().checked_sub(1), "{token:?}");
                    assert!(
                        string_prefix_completes(bytes),
                        "{token:?}: an ended token must be completable"
                    );
                }
            }
        }
        assert_eq!(tokens.len(), 579_195);
        assert!(refused > 0);
    }

    #[test]
    fn json_codec_rejects_yaml_input() {
        let result = parse_json_bytes(&simple_schema(), "root", b"name: Alice\n", "test");
        assert!(
            matches!(result, Err(ParseInstanceError::Json(_))),
            "YAML must not enter the JSON instance pathway"
        );
    }

    #[test]
    fn large_numerals_parse_without_loss_or_rejection() {
        let schema = number_schema();
        let input = br#"{"integer":123456789012345678901234567890,"decimal":1e400}"#;
        let instance = parse_json_bytes(&schema, "root", input, "test").expect("exact parse");
        let integer = instance
            .nodes
            .values()
            .find(|node| node.anchor.as_ref() == "root:integer")
            .and_then(|node| node.value.as_ref());
        let decimal = instance
            .nodes
            .values()
            .find(|node| node.anchor.as_ref() == "root:decimal")
            .and_then(|node| node.value.as_ref());
        assert!(
            matches!(integer, Some(FieldPresence::Present(Value::BigInt(value))) if value.to_string() == "123456789012345678901234567890")
        );
        assert!(
            matches!(decimal, Some(FieldPresence::Present(Value::Decimal(value))) if value.to_string() == "1e400")
        );

        let emitted = emit_json_bytes(&schema, &instance, "test").expect("exact emit");
        let text = std::str::from_utf8(&emitted).expect("JSON is UTF-8");
        assert!(text.contains("123456789012345678901234567890"));
        assert!(text.contains("1e400"));
        assert!(!text.contains("\"123456789012345678901234567890\""));
        assert!(!text.contains("\"1e400\""));
    }

    #[test]
    fn json_numerals_beyond_the_decimal_exponent_range_are_typed_refusals() {
        let schema = number_schema();
        // The stored exponent is the written exponent less the fractional
        // digit count. A decimal holds an exponent of any size, so every
        // numeral is read exactly, at and beyond the bounds of `i32`; none is
        // refused.
        for (input, digits, exponent) in [
            (
                br#"{"integer":1,"decimal":1e2147483647}"#.as_slice(),
                "1",
                "2147483647",
            ),
            (
                br#"{"integer":1,"decimal":0.1e2147483648}"#.as_slice(),
                "1",
                "2147483647",
            ),
            (
                br#"{"integer":1,"decimal":1e-2147483648}"#.as_slice(),
                "1",
                "-2147483648",
            ),
            (
                br#"{"integer":1,"decimal":1e2147483648}"#.as_slice(),
                "1",
                "2147483648",
            ),
            (
                br#"{"integer":1,"decimal":1e99999999999999999999999}"#.as_slice(),
                "1",
                "99999999999999999999999",
            ),
            (
                br#"{"integer":1,"decimal":1.5e-2147483648}"#.as_slice(),
                "15",
                "-2147483649",
            ),
        ] {
            let instance = parse_json_bytes(&schema, "root", input, "test").expect("read exactly");
            let decimal = instance
                .nodes
                .values()
                .find(|node| node.anchor.as_ref() == "root:decimal")
                .and_then(|node| node.value.as_ref());
            assert!(
                matches!(
                    decimal,
                    Some(FieldPresence::Present(Value::Decimal(value)))
                        if value.digits == digits
                            && !value.negative
                            && value.exponent.to_string() == exponent
                ),
                "{decimal:?}"
            );
        }
    }

    #[test]
    fn json_pathway_reads_long_decimals_exactly() {
        let schema = number_schema();
        let input = br#"{"integer":7,"decimal":0.1000000000000000055511151231257827}"#;
        let instance = parse_json_bytes(&schema, "root", input, "test").expect("exact parse");
        let decimal = instance
            .nodes
            .values()
            .find(|node| node.anchor.as_ref() == "root:decimal")
            .and_then(|node| node.value.as_ref());
        assert!(
            matches!(decimal, Some(FieldPresence::Present(Value::Decimal(value))) if value.to_string() == "0.1000000000000000055511151231257827")
        );
    }

    #[test]
    fn non_finite_json_emit_is_a_typed_value_error() {
        let schema = number_schema();
        let nodes = HashMap::from([(
            0,
            Node::new(0, "root:decimal")
                .with_value(FieldPresence::Present(Value::Float(f64::INFINITY))),
        )]);
        let instance = WInstance::new(nodes, Vec::new(), Vec::new(), 0, "root:decimal".into());
        let error = emit_json_bytes(&schema, &instance, "test").expect_err("infinity is not JSON");
        assert!(matches!(
            error,
            EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NonFinite,
                ..
            })
        ));
    }

    #[test]
    fn json_refuses_values_that_change_type_when_reparsed() {
        let decimal = Value::Decimal(
            panproto_inst::value::DecimalValue::parse("0.1").expect("decimal fixture"),
        );
        let any = ValueSpec::new(ScalarKind::Any);
        assert!(!json_value_roundtrips(&decimal, &any));

        let instance = WInstance::new(
            HashMap::from([(
                0,
                Node::new(0, "root:name").with_value(FieldPresence::Present(decimal.clone())),
            )]),
            Vec::new(),
            Vec::new(),
            0,
            "root:name".into(),
        );
        let error = emit_json_bytes(&simple_schema(), &instance, "test")
            .expect_err("open JSON would reclassify 0.1 as a float");
        assert!(matches!(
            error,
            EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ..
            })
        ));

        let mut output = Vec::new();
        let error = write_json_value(
            &decimal,
            &ValueSpec::new(ScalarKind::Str),
            &ValueSite::new("string", None),
            &mut output,
        )
        .expect_err("a string vertex cannot carry a decimal JSON token");
        assert!(matches!(
            error,
            EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ..
            })
        ));
        assert!(output.is_empty());
    }

    #[test]
    fn json_refuses_presence_states_and_shapes_its_scalar_syntax_collapses() {
        for presence in [
            FieldPresence::Present(Value::Null),
            FieldPresence::Present(Value::Unknown(HashMap::new())),
            FieldPresence::Present(Value::List(Vec::new())),
        ] {
            let instance = WInstance::new(
                HashMap::from([(0, Node::new(0, "root:name").with_value(presence))]),
                Vec::new(),
                Vec::new(),
                0,
                "root:name".into(),
            );
            let error = emit_json_bytes(&simple_schema(), &instance, "test")
                .expect_err("the JSON scalar pathway cannot preserve this node state");
            assert!(matches!(
                error,
                EmitInstanceError::Value(EmitValueError {
                    reason: EmitValueReason::NotRepresentable,
                    ..
                })
            ));
        }
    }

    #[test]
    fn integral_float_keeps_its_json_number_class() {
        let schema = number_schema();
        let nodes = HashMap::from([(
            0,
            Node::new(0, "root:decimal").with_value(FieldPresence::Present(Value::Float(1.0))),
        )]);
        let instance = WInstance::new(nodes, Vec::new(), Vec::new(), 0, "root:decimal".into());
        let emitted = emit_json_bytes(&schema, &instance, "test").expect("emit");
        assert_eq!(emitted, b"1.0");

        let reparsed = parse_json_bytes(&schema, "root:decimal", &emitted, "test")
            .expect("reparse as an open numeric value");
        assert!(matches!(
            reparsed.node(0).and_then(|node| node.value.as_ref()),
            Some(FieldPresence::Present(Value::Float(value)))
                if value.to_bits() == 1.0_f64.to_bits()
        ));
    }

    #[test]
    fn anonymous_schema_edges_do_not_hide_record_fields() {
        let protocol = Protocol {
            name: "test".into(),
            obj_kinds: vec!["object".into(), "value".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .vertex("item", "value", None)
            .expect("item")
            .edge("root", "item", "item", None)
            .expect("anonymous edge")
            .build()
            .expect("schema");
        let mut root = Node::new(0, "root");
        root.extra_fields
            .insert("kept".to_owned(), Value::Str("yes".to_owned()));
        let instance = WInstance::new(
            HashMap::from([(0, root)]),
            Vec::new(),
            Vec::new(),
            0,
            "root".into(),
        );

        let emitted = emit_json_bytes(&schema, &instance, "test").expect("emit object");
        assert_eq!(emitted, br#"{"kept":"yes"}"#);

        let edge = schema.outgoing_edges("root")[0].clone();
        let instance = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "root")),
                (
                    1,
                    Node::new(1, "item")
                        .with_value(FieldPresence::Present(Value::Str("child".into()))),
                ),
            ]),
            vec![(0, 1, edge)],
            Vec::new(),
            0,
            "root".into(),
        );
        assert_eq!(
            emit_json_bytes(&schema, &instance, "test").expect("anonymous object property"),
            br#"{"item":"child"}"#
        );
    }

    #[test]
    fn canonical_json_anchor_checks_follow_the_emitted_syntax() {
        let protocol = Protocol {
            name: "test".into(),
            obj_kinds: vec!["object".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("wrap", "object", None)
            .expect("wrapper")
            .vertex("body", "object", None)
            .expect("body")
            .edge("wrap", "body", "record-schema", None)
            .expect("transparent edge")
            .build()
            .expect("schema");

        let wrapper_object = WInstance::new(
            HashMap::from([(0, Node::new(0, "wrap"))]),
            Vec::new(),
            Vec::new(),
            0,
            "wrap".into(),
        );
        assert!(matches!(
            emit_json_bytes(&schema, &wrapper_object, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("expected anchor 'body'")
        ));

        let target_object = WInstance::new(
            HashMap::from([(0, Node::new(0, "body"))]),
            Vec::new(),
            Vec::new(),
            0,
            "wrap".into(),
        );
        assert_eq!(
            emit_json_bytes(&schema, &target_object, "test").expect("resolved object root"),
            b"{}"
        );

        let target_scalar = WInstance::new(
            HashMap::from([(
                0,
                Node::new(0, "body")
                    .with_value(FieldPresence::Present(Value::Str("scalar".into()))),
            )]),
            Vec::new(),
            Vec::new(),
            0,
            "wrap".into(),
        );
        assert!(matches!(
            emit_json_bytes(&schema, &target_scalar, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("expected anchor 'wrap'")
        ));

        let mut ghost_schema = schema.clone();
        ghost_schema.vertices.remove("body");
        assert!(matches!(
            emit_json_bytes(&ghost_schema, &target_object, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("transparent target 'body'")
        ));
        assert!(matches!(
            crate::toml_pathway::emit_toml_bytes(&schema, &target_object, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("expected anchor 'wrap'")
        ));
    }

    #[test]
    fn canonical_json_uses_exactly_the_parser_selected_list_carrier() {
        let protocol = Protocol {
            name: "test".into(),
            obj_kinds: vec!["array".into(), "value".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "array", None)
            .expect("root")
            .vertex("first", "value", None)
            .expect("first")
            .vertex("second", "value", None)
            .expect("second")
            .edge("root", "first", "first-item", None)
            .expect("first edge")
            .edge("root", "second", "second-item", None)
            .expect("second edge")
            .build()
            .expect("schema");
        let second = schema.outgoing_edges("root")[1].clone();
        let mut root = Node::new(0, "root");
        root.shape = NodeShape::List;
        let instance = WInstance::new(
            HashMap::from([
                (0, root.clone()),
                (
                    1,
                    Node::new(1, "second")
                        .with_value(FieldPresence::Present(Value::Str("value".into()))),
                ),
            ]),
            vec![(0, 1, second)],
            Vec::new(),
            0,
            "root".into(),
        );
        assert!(matches!(
            emit_json_bytes(&schema, &instance, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("parser-selected list carrier")
        ));

        let synthetic = Edge {
            src: "root".into(),
            tgt: "root:items".into(),
            kind: "item".into(),
            name: Some("item".into()),
        };
        let instance = WInstance::new(
            HashMap::from([
                (0, root),
                (
                    1,
                    Node::new(1, "root:items")
                        .with_value(FieldPresence::Present(Value::Str("value".into()))),
                ),
            ]),
            vec![(0, 1, synthetic)],
            Vec::new(),
            0,
            "root".into(),
        );
        assert!(matches!(
            emit_json_bytes(&schema, &instance, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("parser-selected list carrier")
        ));
    }

    #[test]
    fn canonical_json_rejects_incoherent_schema_edge_indices() {
        let schema = simple_schema();
        let instance =
            parse_json_bytes(&schema, "root", br#"{"name":"value"}"#, "test").expect("parse");
        let edge = schema.outgoing_edges("root")[0].clone();

        let mut missing_primary = schema.clone();
        missing_primary.edges.remove(&edge);
        assert!(matches!(
            emit_json_bytes(&missing_primary, &instance, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("schema edge carriers disagree")
        ));

        let mut missing_route = schema;
        missing_route.outgoing.remove("root");
        assert!(matches!(
            emit_json_bytes(&missing_route, &instance, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("schema edge carriers disagree")
        ));
    }

    #[test]
    fn json_refuses_object_key_collisions() {
        let schema = simple_schema();
        let mut instance = parse_json_bytes(&schema, "root", br#"{"name":"child"}"#, "test")
            .expect("parse collision fixture");
        instance
            .nodes
            .get_mut(&instance.root)
            .expect("root")
            .extra_fields
            .insert("name".to_owned(), Value::Str("extra".to_owned()));
        assert!(matches!(
            emit_json_bytes(&schema, &instance, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("claimed by a declared field route")
        ));

        let root = instance.nodes.get_mut(&instance.root).expect("root");
        root.extra_fields.clear();
        root.extra_fields
            .insert("$type".to_owned(), Value::Str("data".to_owned()));
        assert!(matches!(
            emit_json_bytes(&schema, &instance, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("reparse as a JSON discriminator")
        ));

        let root = instance.nodes.get_mut(&instance.root).expect("root");
        root.extra_fields.clear();
        root.discriminator = Some("kind".into());
        root.extra_fields
            .insert("$type".to_owned(), Value::Str("extra".to_owned()));
        assert!(matches!(
            emit_json_bytes(&schema, &instance, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("duplicate emitted JSON object key")
        ));
    }

    #[test]
    fn canonical_json_refuses_schema_claimed_and_ambiguous_object_keys() {
        let protocol = Protocol {
            name: "test".into(),
            obj_kinds: vec!["object".into(), "string".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .vertex("first", "string", None)
            .expect("first")
            .vertex("second", "string", None)
            .expect("second")
            .edge("root", "first", "prop", Some("field"))
            .expect("first route")
            .edge("root", "second", "prop", Some("field"))
            .expect("second route")
            .build()
            .expect("schema");

        let mut extra_root = Node::new(0, "root");
        extra_root
            .extra_fields
            .insert("field".into(), Value::Str("extra".into()));
        let extra = WInstance::new(
            HashMap::from([(0, extra_root)]),
            Vec::new(),
            Vec::new(),
            0,
            "root".into(),
        );
        assert!(matches!(
            emit_json_bytes(&schema, &extra, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("claimed by a declared field route")
        ));

        let first_edge = schema.outgoing_edges("root")[0].clone();
        let ambiguous = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "root")),
                (
                    1,
                    Node::new(1, "first")
                        .with_value(FieldPresence::Present(Value::Str("value".into()))),
                ),
            ]),
            vec![(0, 1, first_edge)],
            Vec::new(),
            0,
            "root".into(),
        );
        assert!(matches!(
            emit_json_bytes(&schema, &ambiguous, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("one unambiguous parser route")
        ));
    }

    #[test]
    fn json_refuses_declared_type_children_that_cannot_round_trip_as_data() {
        let protocol = Protocol {
            name: "test".into(),
            obj_kinds: vec!["object".into(), "string".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .vertex("root:type", "string", None)
            .expect("type")
            .edge("root", "root:type", "prop", Some("$type"))
            .expect("type edge")
            .build()
            .expect("schema");

        let mut data_only =
            parse_json_bytes(&schema, "root", br#"{"$type":"data"}"#, "test").expect("parse");
        data_only
            .nodes
            .get_mut(&data_only.root)
            .expect("root")
            .discriminator = None;
        assert!(matches!(
            emit_json_bytes(&schema, &data_only, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("declared $type child")
        ));

        let mut discriminator_only =
            parse_json_bytes(&schema, "root", br#"{"$type":"kind"}"#, "test").expect("parse");
        let child = discriminator_only.children(discriminator_only.root)[0];
        discriminator_only
            .nodes
            .get_mut(&child)
            .expect("type child")
            .value = Some(FieldPresence::Absent);
        assert!(matches!(
            emit_json_bytes(&schema, &discriminator_only, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("also claimed by a declared field route")
        ));
    }

    #[test]
    fn repeated_property_arcs_are_not_reinterpreted_as_a_list() {
        let schema = simple_schema();
        let edge = schema
            .outgoing_edges("root")
            .first()
            .expect("name edge")
            .clone();
        let instance = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "root")),
                (
                    1,
                    Node::new(1, "root:name")
                        .with_value(FieldPresence::Present(Value::Str("first".into()))),
                ),
                (
                    2,
                    Node::new(2, "root:name")
                        .with_value(FieldPresence::Present(Value::Str("second".into()))),
                ),
            ]),
            vec![(0, 1, edge.clone()), (0, 2, edge)],
            Vec::new(),
            0,
            "root".into(),
        );
        assert!(matches!(
            emit_json_bytes(&schema, &instance, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("duplicate emitted JSON object key")
        ));
    }

    #[test]
    fn json_refuses_scalar_nodes_with_unrepresentable_structure() {
        let mut root = Node::new(0, "root:name")
            .with_value(FieldPresence::Present(Value::Str("scalar".into())));
        root.extra_fields
            .insert("lost".to_owned(), Value::Str("field".to_owned()));
        let mut instance = WInstance::new(
            HashMap::from([(0, root)]),
            Vec::new(),
            Vec::new(),
            0,
            "root:name".into(),
        );
        assert!(matches!(
            emit_json_bytes(&simple_schema(), &instance, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("scalar node")
        ));

        let root = instance.nodes.get_mut(&instance.root).expect("root");
        root.extra_fields.clear();
        root.shape = NodeShape::List;
        assert!(matches!(
            emit_json_bytes(&simple_schema(), &instance, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("scalar node")
        ));
    }

    #[test]
    fn json_refuses_absent_children_with_retained_structure() {
        let schema = simple_schema();
        let mut instance = parse_json_bytes(&schema, "root", br#"{"name":"child"}"#, "test")
            .expect("parse absent-child fixture");
        let child = instance.children(instance.root)[0];
        let node = instance.nodes.get_mut(&child).expect("child");
        node.value = Some(FieldPresence::Absent);
        node.extra_fields
            .insert("lost".to_owned(), Value::Str("field".to_owned()));

        assert!(matches!(
            emit_json_bytes(&schema, &instance, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("absent JSON child")
        ));
    }

    #[test]
    fn open_schema_arrays_keep_every_item() {
        let protocol = Protocol {
            name: "test".into(),
            obj_kinds: vec!["value".into()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "value", None)
            .expect("root")
            .build()
            .expect("schema");

        let instance = parse_json_bytes(&schema, "root", b"[1,2]", "test").expect("parse");
        assert_eq!(instance.children(instance.root).len(), 2);
        assert_eq!(
            emit_json_bytes(&schema, &instance, "test").expect("emit"),
            b"[1,2]"
        );
    }

    #[test]
    fn untyped_exact_string_variants_are_refused() {
        let mut root = Node::new(0, "root");
        root.extra_fields
            .insert("bytes".to_owned(), Value::Bytes(vec![0xff]));
        let instance = WInstance::new(
            HashMap::from([(0, root)]),
            Vec::new(),
            Vec::new(),
            0,
            "root".into(),
        );
        let error = emit_json_bytes(&simple_schema(), &instance, "test")
            .expect_err("an untyped byte string has no reversible JSON form");
        assert!(matches!(
            error,
            EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ..
            })
        ));
    }

    #[test]
    fn base64_encoders_emit_canonical_padding() {
        assert_eq!(encode_bytes(&[], &BytesText::Base64).as_deref(), Some(""));
        assert_eq!(
            encode_bytes(&[0xff], &BytesText::Base64).as_deref(),
            Some("/w==")
        );
        assert_eq!(
            encode_bytes(&[0xff, 0xee], &BytesText::Base64).as_deref(),
            Some("/+4=")
        );
        assert_eq!(
            encode_bytes(&[0xff, 0xee, 0xdd], &BytesText::Base64).as_deref(),
            Some("/+7d")
        );
        assert_eq!(
            encode_bytes(&[0xff, 0xee], &BytesText::Base64Url).as_deref(),
            Some("_-4=")
        );
    }

    #[test]
    fn extra_field_errors_follow_key_order() {
        let mut root = Node::new(0, "root");
        root.extra_fields
            .insert("z".to_owned(), Value::Float(f64::INFINITY));
        root.extra_fields
            .insert("a".to_owned(), Value::LabeledNull(1));
        let instance = WInstance::new(
            HashMap::from([(0, root)]),
            Vec::new(),
            Vec::new(),
            0,
            "root".into(),
        );

        let error = emit_json_bytes(&simple_schema(), &instance, "test").expect_err("must fail");
        assert!(matches!(
            error,
            EmitInstanceError::Value(EmitValueError {
                value_type,
                reason: EmitValueReason::NotRepresentable,
                ..
            }) if value_type == "labeled-null"
        ));
    }

    fn open_schema() -> Schema {
        let protocol = Protocol {
            name: "test".into(),
            obj_kinds: vec!["value".into()],
            ..Protocol::default()
        };
        SchemaBuilder::new(&protocol)
            .vertex("root", "value", None)
            .expect("root")
            .build()
            .expect("schema")
    }

    fn nested_arrays(arrays: usize, leaf: &str) -> String {
        format!("{}{leaf}{}", "[".repeat(arrays), "]".repeat(arrays))
    }

    #[test]
    fn json_nesting_bound_counts_every_value_including_empty_containers() {
        // The document's top value is level one and each nested value adds a
        // level, so 128 levels is the deepest a document may nest whatever
        // the innermost value is, and every accepted document re-emits.
        let open = open_schema();
        let named = simple_schema();
        let accepted = [
            (&open, nested_arrays(128, "")),
            (&open, nested_arrays(127, "1")),
            (&open, format!("{{\"x\":{}}}", nested_arrays(127, ""))),
            (&named, format!("{{\"name\":{}}}", nested_arrays(127, ""))),
        ];
        for (schema, input) in &accepted {
            let instance = parse_json_bytes(schema, "root", input.as_bytes(), "test")
                .unwrap_or_else(|error| panic!("128 levels parse: {error}"));
            let emitted = emit_json_bytes(schema, &instance, "test")
                .unwrap_or_else(|error| panic!("an accepted document re-emits: {error}"));
            assert_eq!(emitted, input.as_bytes());
        }
        let refused = [
            (&open, nested_arrays(129, "")),
            (&open, nested_arrays(128, "1")),
            (&open, format!("{{\"x\":{}}}", nested_arrays(128, ""))),
            (&named, format!("{{\"name\":{}}}", nested_arrays(128, ""))),
        ];
        for (schema, input) in &refused {
            let result = parse_json_bytes(schema, "root", input.as_bytes(), "test");
            assert!(
                matches!(
                    &result,
                    Err(ParseInstanceError::Json(message))
                        if message == "JSON nesting exceeds 128 levels"
                ),
                "129 levels must be refused: {result:?}"
            );
        }
    }

    fn decimal_field(instance: &WInstance) -> Option<&FieldPresence> {
        instance
            .nodes
            .values()
            .find(|node| node.anchor.as_ref() == "root:decimal")
            .and_then(|node| node.value.as_ref())
    }

    #[test]
    fn json_zero_numerals_keep_their_sign_whatever_their_exponent() {
        let schema = number_schema();
        for (numeral, expected) in [
            ("0e99999999999", 0.0_f64),
            ("0.0e-99999999999", 0.0),
            ("0E+99999999999999999999", 0.0),
            ("-0.0", -0.0),
            ("-0e99999999999", -0.0),
            ("-0.000e-99999999999", -0.0),
        ] {
            let input = format!("{{\"integer\":1,\"decimal\":{numeral}}}");
            let instance = parse_json_bytes(&schema, "root", input.as_bytes(), "test")
                .unwrap_or_else(|error| panic!("{numeral} has an exact carrier: {error}"));
            let Some(FieldPresence::Present(Value::Float(actual))) = decimal_field(&instance)
            else {
                panic!("{numeral} reads as a float: {:?}", decimal_field(&instance));
            };
            assert_eq!(actual.to_bits(), expected.to_bits(), "{numeral}");

            let emitted = emit_json_bytes(&schema, &instance, "test")
                .unwrap_or_else(|error| panic!("{numeral} re-emits: {error}"));
            let reparsed = parse_json_bytes(&schema, "root", &emitted, "test").expect("reparse");
            let Some(FieldPresence::Present(Value::Float(again))) = decimal_field(&reparsed) else {
                panic!("{numeral} reparses as a float");
            };
            assert_eq!(
                again.to_bits(),
                expected.to_bits(),
                "{numeral} after re-emission"
            );
        }

        let open = open_schema();
        let instance =
            parse_json_bytes(&open, "root", br#"{"x":-0.0,"y":[-0e99999999999]}"#, "test")
                .expect("open fields");
        let root = instance.node(instance.root).expect("root");
        assert!(
            matches!(root.extra_fields.get("x"), Some(Value::Float(x)) if x.to_bits() == (-0.0_f64).to_bits())
        );
        assert!(matches!(
            root.extra_fields.get("y"),
            Some(Value::List(items))
                if matches!(items.as_slice(), [Value::Float(y)] if y.to_bits() == (-0.0_f64).to_bits())
        ));
        assert_eq!(
            emit_json_bytes(&open, &instance, "test").expect("negative zero is JSON"),
            br#"{"x":-0.0,"y":[-0.0]}"#
        );

        let mut written = Node::new(0, "root:decimal");
        written.value = Some(FieldPresence::Present(Value::Float(-0.0)));
        let direct = WInstance::new(
            HashMap::from([(0, written)]),
            Vec::new(),
            Vec::new(),
            0,
            "root:decimal".into(),
        );
        assert_eq!(
            emit_json_bytes(&schema, &direct, "test").expect("negative zero emits"),
            b"-0.0"
        );
    }
}
