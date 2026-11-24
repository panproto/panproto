//! The binary form: decoding by the BSON 1.1 grammar and canonical
//! encoding.

use super::decimal128::{from_bid, to_bid};
use super::{
    BsonDecodeError, BsonDecodeErrorKind as Kind, BsonDocument, BsonValue, BsonWriteError,
    BsonWriteErrorKind, MAX_DEPTH, child_path, options_in_order, write_error,
};

/// The refusal at `offset` of `kind`.
const fn fault(offset: usize, kind: Kind) -> BsonDecodeError {
    BsonDecodeError { offset, kind }
}

/// Decode one binary BSON document, which must be the whole of `bytes`.
///
/// The bytes are read left to right by the BSON 1.1 grammar, and the
/// first rule broken is refused at its byte offset. A document's length
/// must be at least 5 and fit the bytes its container leaves it (the whole
/// input for the top-level document; for an embedded one, the bytes before
/// its container's last byte); its last byte must be `0x00` and no earlier
/// type byte may be `0x00`. Each element is a type byte of BSON 1.1
/// (`0x01` to `0x13`, `0x7F`, `0xFF`), a key (a `0x00`-terminated UTF-8
/// string) and a value that ends before its document's last byte: a
/// string's length is at least 1 and its last byte `0x00`, a boolean is
/// `0x00` or `0x01`, a binary's length is not negative and a subtype 2
/// binary repeats its length less 4, a code with scope's length is that of
/// its code and scope, an array's keys are `"0"`, `"1"`, … in order, a
/// regular expression's options are in alphabetical order, and a
/// decimal128 is a canonical encoding. Bytes after the document are
/// refused, and so is nesting deeper than [`MAX_DEPTH`].
///
/// # Errors
///
/// The [`BsonDecodeError`] of the first rule broken.
pub fn decode(bytes: &[u8]) -> Result<BsonDocument, BsonDecodeError> {
    let (elements, end) = Decoder { bytes }.document(0, bytes.len(), 1, false)?;
    if end != bytes.len() {
        return Err(fault(end, Kind::TrailingBytes));
    }
    Ok(BsonDocument(elements))
}

/// The decoder of one input.
struct Decoder<'a> {
    bytes: &'a [u8],
}

/// A document's elements and the offset after it.
type Elements = (Vec<(String, BsonValue)>, usize);

impl Decoder<'_> {
    /// The `N` bytes at `at`, which must end at or before `limit`.
    fn fixed<const N: usize>(&self, at: usize, limit: usize) -> Result<[u8; N], BsonDecodeError> {
        at.checked_add(N)
            .filter(|end| *end <= limit)
            .and_then(|end| self.bytes.get(at..end))
            .and_then(|slice| <[u8; N]>::try_from(slice).ok())
            .ok_or_else(|| fault(at, Kind::Truncated))
    }

    fn int32(&self, at: usize, limit: usize) -> Result<i32, BsonDecodeError> {
        self.fixed::<4>(at, limit).map(i32::from_le_bytes)
    }

    /// The offset `at + length` when `length` is at least `min` and the
    /// region ends at or before `limit`.
    fn region(at: usize, length: i32, min: i32, limit: usize) -> Option<usize> {
        if length < min {
            return None;
        }
        usize::try_from(length)
            .ok()
            .and_then(|length| at.checked_add(length))
            .filter(|end| *end <= limit)
    }

    /// A document at `start` within `limit`, at nesting `depth`; for an
    /// array, its keys are checked against the indices.
    fn document(
        &self,
        start: usize,
        limit: usize,
        depth: usize,
        array: bool,
    ) -> Result<Elements, BsonDecodeError> {
        if depth > MAX_DEPTH {
            return Err(fault(start, Kind::Depth));
        }
        let length = self.int32(start, limit)?;
        let end = Self::region(start, length, 5, limit).ok_or_else(|| {
            fault(
                start,
                Kind::DocumentLength {
                    declared: i64::from(length),
                },
            )
        })?;
        let last = end - 1;
        let mut at = start + 4;
        let mut elements = Vec::new();
        loop {
            let byte = self.bytes[at];
            if at == last {
                if byte != 0 {
                    return Err(fault(at, Kind::Terminator { byte }));
                }
                return Ok((elements, end));
            }
            if byte == 0 {
                return Err(fault(at, Kind::EarlyTerminator));
            }
            if !matches!(byte, 0x01..=0x13 | 0x7F | 0xFF) {
                return Err(fault(at, Kind::UnknownType { byte }));
            }
            let (key, after_key) = self.cstring(at + 1, last)?;
            if array {
                let expected = elements.len().to_string();
                if key != expected {
                    return Err(fault(at + 1, Kind::ArrayKey { expected }));
                }
            }
            let (value, next) = self.value(byte, after_key, last, depth)?;
            elements.push((key, value));
            at = next;
        }
    }

    /// A `0x00`-terminated UTF-8 string at `at`, whose `0x00` comes before
    /// `limit`, and the offset after it.
    fn cstring(&self, at: usize, limit: usize) -> Result<(String, usize), BsonDecodeError> {
        let region = self.bytes.get(at..limit).unwrap_or_default();
        let nul = region
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(|| fault(at, Kind::UnterminatedCString))?;
        let text = utf8(&region[..nul], at)?;
        Ok((text, at + nul + 1))
    }

    /// A length-prefixed string at `at` within `limit`, and the offset
    /// after it.
    fn string(&self, at: usize, limit: usize) -> Result<(String, usize), BsonDecodeError> {
        let length = self.int32(at, limit)?;
        let end = Self::region(at + 4, length, 1, limit).ok_or_else(|| {
            fault(
                at,
                Kind::StringLength {
                    declared: i64::from(length),
                },
            )
        })?;
        let byte = self.bytes[end - 1];
        if byte != 0 {
            return Err(fault(end - 1, Kind::StringTerminator { byte }));
        }
        Ok((utf8(&self.bytes[at + 4..end - 1], at + 4)?, end))
    }

    /// The value of type `byte` at `at` within `limit`, and the offset
    /// after it.
    fn value(
        &self,
        byte: u8,
        at: usize,
        limit: usize,
        depth: usize,
    ) -> Result<(BsonValue, usize), BsonDecodeError> {
        Ok(match byte {
            0x01 => (
                BsonValue::Double(f64::from_le_bytes(self.fixed(at, limit)?)),
                at + 8,
            ),
            0x02 => {
                let (text, end) = self.string(at, limit)?;
                (BsonValue::String(text), end)
            }
            0x03 => {
                let (elements, end) = self.document(at, limit, depth + 1, false)?;
                (BsonValue::Document(BsonDocument(elements)), end)
            }
            0x04 => {
                let (elements, end) = self.document(at, limit, depth + 1, true)?;
                (
                    BsonValue::Array(elements.into_iter().map(|(_, value)| value).collect()),
                    end,
                )
            }
            0x05 => self.binary(at, limit)?,
            0x06 => (BsonValue::Undefined, at),
            0x07 => (BsonValue::ObjectId(self.fixed(at, limit)?), at + 12),
            0x08 => match self.fixed::<1>(at, limit)?[0] {
                0 => (BsonValue::Boolean(false), at + 1),
                1 => (BsonValue::Boolean(true), at + 1),
                byte => return Err(fault(at, Kind::Boolean { byte })),
            },
            0x09 => (
                BsonValue::DateTime(i64::from_le_bytes(self.fixed(at, limit)?)),
                at + 8,
            ),
            0x0A => (BsonValue::Null, at),
            0x0B => {
                let (pattern, after) = self.cstring(at, limit)?;
                let (options, end) = self.cstring(after, limit)?;
                if !options_in_order(&options) {
                    return Err(fault(after, Kind::RegexOptions));
                }
                (BsonValue::Regex { pattern, options }, end)
            }
            0x0C => {
                let (namespace, after) = self.string(at, limit)?;
                let id = self.fixed(after, limit)?;
                (BsonValue::DbPointer { namespace, id }, after + 12)
            }
            0x0D => {
                let (text, end) = self.string(at, limit)?;
                (BsonValue::Code(text), end)
            }
            0x0E => {
                let (text, end) = self.string(at, limit)?;
                (BsonValue::Symbol(text), end)
            }
            0x0F => self.code_with_scope(at, limit, depth)?,
            0x10 => (
                BsonValue::Int32(i32::from_le_bytes(self.fixed(at, limit)?)),
                at + 4,
            ),
            0x11 => {
                let bytes: [u8; 8] = self.fixed(at, limit)?;
                let increment = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                let time = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                (BsonValue::Timestamp { time, increment }, at + 8)
            }
            0x12 => (
                BsonValue::Int64(i64::from_le_bytes(self.fixed(at, limit)?)),
                at + 8,
            ),
            0x13 => {
                let value = from_bid(self.fixed(at, limit)?)
                    .ok_or_else(|| fault(at, Kind::NonCanonicalDecimal128))?;
                (BsonValue::Decimal128(value), at + 16)
            }
            0x7F => (BsonValue::MaxKey, at),
            _ => (BsonValue::MinKey, at),
        })
    }

    fn binary(&self, at: usize, limit: usize) -> Result<(BsonValue, usize), BsonDecodeError> {
        let length = self.int32(at, limit)?;
        let end = Self::region(at + 5, length, 0, limit).ok_or_else(|| {
            fault(
                at,
                Kind::BinaryLength {
                    declared: i64::from(length),
                },
            )
        })?;
        let subtype = self.bytes[at + 4];
        let mut start = at + 5;
        if subtype == 2 {
            let inner = self.int32(start, end)?;
            if i64::from(inner) != i64::from(length) - 4 {
                return Err(fault(
                    start,
                    Kind::OldBinaryLength {
                        declared: i64::from(inner),
                    },
                ));
            }
            start += 4;
        }
        Ok((
            BsonValue::Binary {
                subtype,
                bytes: self.bytes[start..end].to_vec(),
            },
            end,
        ))
    }

    fn code_with_scope(
        &self,
        at: usize,
        limit: usize,
        depth: usize,
    ) -> Result<(BsonValue, usize), BsonDecodeError> {
        let length = self.int32(at, limit)?;
        let refused = || {
            fault(
                at,
                Kind::CodeWithScopeLength {
                    declared: i64::from(length),
                },
            )
        };
        let end = Self::region(at, length, 14, limit).ok_or_else(refused)?;
        let (code, after) = self.string(at + 4, end)?;
        let (elements, scope_end) = self.document(after, end, depth + 1, false)?;
        if scope_end != end {
            return Err(refused());
        }
        Ok((
            BsonValue::CodeWithScope {
                code,
                scope: BsonDocument(elements),
            },
            end,
        ))
    }
}

/// `bytes` as UTF-8, or the offset (from `at`) of its first invalid
/// sequence.
fn utf8(bytes: &[u8], at: usize) -> Result<String, BsonDecodeError> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|error| fault(at + error.valid_up_to(), Kind::InvalidUtf8))
}

/// Encode a document as its canonical binary form.
///
/// The form is the BSON 1.1 layout, little-endian, with arrays keyed `"0"`, `"1"`, … in order, a subtype 2
/// binary's inner length repeated, and each decimal128 in its canonical
/// BID encoding. [`decode`] reads the result back as the same document.
///
/// # Errors
///
/// A [`BsonWriteError`] at the first value, in document order, that no
/// binary document holds: a key or regular expression holding U+0000,
/// regular expression options out of alphabetical order, a decimal
/// decimal128 does not hold as written, a document or string too long for
/// an `int32` length, or nesting deeper than [`MAX_DEPTH`].
pub fn encode(document: &BsonDocument) -> Result<Vec<u8>, BsonWriteError> {
    let mut out = Vec::new();
    let entries: Vec<(&str, &BsonValue)> = document
        .0
        .iter()
        .map(|(key, value)| (key.as_str(), value))
        .collect();
    write_document(&mut out, &entries, "", 1)?;
    Ok(out)
}

fn write_document(
    out: &mut Vec<u8>,
    entries: &[(&str, &BsonValue)],
    path: &str,
    depth: usize,
) -> Result<(), BsonWriteError> {
    if depth > MAX_DEPTH {
        return Err(write_error(path.to_owned(), BsonWriteErrorKind::Depth));
    }
    let start = out.len();
    out.extend_from_slice(&[0; 4]);
    for (key, value) in entries {
        let path = child_path(path, key);
        if key.contains('\0') {
            return Err(write_error(path, BsonWriteErrorKind::NulInKey));
        }
        out.push(value.type_byte());
        out.extend_from_slice(key.as_bytes());
        out.push(0);
        write_value(out, value, &path, depth)?;
    }
    out.push(0);
    patch_length(out, start, path)
}

/// Write the length of the region from `start` to the end of `out` into
/// its first four bytes.
fn patch_length(out: &mut [u8], start: usize, path: &str) -> Result<(), BsonWriteError> {
    let length = i32::try_from(out.len() - start)
        .map_err(|_| write_error(path.to_owned(), BsonWriteErrorKind::TooLong))?;
    out[start..start + 4].copy_from_slice(&length.to_le_bytes());
    Ok(())
}

fn write_string(out: &mut Vec<u8>, text: &str, path: &str) -> Result<(), BsonWriteError> {
    let length = i32::try_from(text.len() + 1)
        .map_err(|_| write_error(path.to_owned(), BsonWriteErrorKind::TooLong))?;
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(text.as_bytes());
    out.push(0);
    Ok(())
}

fn write_value(
    out: &mut Vec<u8>,
    value: &BsonValue,
    path: &str,
    depth: usize,
) -> Result<(), BsonWriteError> {
    match value {
        BsonValue::Double(x) => out.extend_from_slice(&x.to_le_bytes()),
        BsonValue::String(text) | BsonValue::Code(text) | BsonValue::Symbol(text) => {
            write_string(out, text, path)?;
        }
        BsonValue::Document(document) => {
            let entries: Vec<(&str, &BsonValue)> =
                document.0.iter().map(|(k, v)| (k.as_str(), v)).collect();
            write_document(out, &entries, path, depth + 1)?;
        }
        BsonValue::Array(items) => {
            let keys: Vec<String> = (0..items.len()).map(|i| i.to_string()).collect();
            let entries: Vec<(&str, &BsonValue)> =
                keys.iter().map(String::as_str).zip(items).collect();
            write_document(out, &entries, path, depth + 1)?;
        }
        BsonValue::Binary { subtype, bytes } => {
            let inner = if *subtype == 2 { 4 } else { 0 };
            let length = i32::try_from(bytes.len() + inner)
                .map_err(|_| write_error(path.to_owned(), BsonWriteErrorKind::TooLong))?;
            out.extend_from_slice(&length.to_le_bytes());
            out.push(*subtype);
            if *subtype == 2 {
                out.extend_from_slice(&(length - 4).to_le_bytes());
            }
            out.extend_from_slice(bytes);
        }
        BsonValue::ObjectId(id) => out.extend_from_slice(id),
        BsonValue::Boolean(flag) => out.push(u8::from(*flag)),
        BsonValue::DateTime(ms) | BsonValue::Int64(ms) => out.extend_from_slice(&ms.to_le_bytes()),
        BsonValue::Regex { pattern, options } => {
            if pattern.contains('\0') || options.contains('\0') {
                return Err(write_error(path.to_owned(), BsonWriteErrorKind::NulInRegex));
            }
            if !options_in_order(options) {
                return Err(write_error(
                    path.to_owned(),
                    BsonWriteErrorKind::RegexOptions,
                ));
            }
            for part in [pattern, options] {
                out.extend_from_slice(part.as_bytes());
                out.push(0);
            }
        }
        BsonValue::DbPointer { namespace, id } => {
            write_string(out, namespace, path)?;
            out.extend_from_slice(id);
        }
        BsonValue::CodeWithScope { code, scope } => {
            let start = out.len();
            out.extend_from_slice(&[0; 4]);
            write_string(out, code, path)?;
            let entries: Vec<(&str, &BsonValue)> =
                scope.0.iter().map(|(k, v)| (k.as_str(), v)).collect();
            write_document(out, &entries, &child_path(path, "$scope"), depth + 1)?;
            patch_length(out, start, path)?;
        }
        BsonValue::Int32(n) => out.extend_from_slice(&n.to_le_bytes()),
        BsonValue::Timestamp { time, increment } => {
            out.extend_from_slice(&increment.to_le_bytes());
            out.extend_from_slice(&time.to_le_bytes());
        }
        BsonValue::Decimal128(decimal) => {
            let bytes = to_bid(decimal).ok_or_else(|| {
                write_error(
                    path.to_owned(),
                    BsonWriteErrorKind::Decimal128(decimal.to_string()),
                )
            })?;
            out.extend_from_slice(&bytes);
        }
        BsonValue::Undefined | BsonValue::Null | BsonValue::MinKey | BsonValue::MaxKey => {}
    }
    Ok(())
}
