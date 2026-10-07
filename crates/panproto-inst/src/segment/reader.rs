//! Reading canonical bytes back.
//!
//! [`Reader`] is the inverse of the [`CanonicalSink`](crate::CanonicalSink)
//! encoders for the records a segment holds. It checks every length against
//! the bytes left, so a corrupt count cannot make it allocate more than the
//! input could hold, and every value it builds through the value types' own
//! checked constructors.

use std::collections::HashMap;

use panproto_expr::exact::{DateTimeSeparator, Fraction, WideInt, ZuluCase};
use panproto_gat::Name;
use panproto_schema::Edge;

use super::error::SegmentError;
use super::ids::{IdRange, SegmentId};
use super::store::ArtifactKind;
use crate::identity::Digest;
use crate::metadata::{Node, NodeShape};
use crate::value::{
    BigIntValue, CivilDate, CivilTime, DecimalValue, FieldPresence, Offset, Temporal, Value,
};
use crate::view_ref::{RowId, ViewRef};

/// How deeply nested a value may be before reading it is refused.
const MAX_VALUE_DEPTH: usize = 128;

/// A cursor over canonical bytes of one kind of artifact.
pub struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    artifact: ArtifactKind,
}

impl<'a> Reader<'a> {
    /// A reader at the start of `bytes`, which hold an artifact of
    /// `artifact`'s kind.
    pub const fn new(bytes: &'a [u8], artifact: ArtifactKind) -> Self {
        Self {
            bytes,
            at: 0,
            artifact,
        }
    }

    /// The decode error for this artifact.
    pub fn error(&self, message: impl Into<String>) -> SegmentError {
        SegmentError::Decode {
            artifact: self.artifact,
            message: format!("{} at byte {}", message.into(), self.at),
        }
    }

    /// Whether every byte has been read.
    pub const fn finished(&self) -> bool {
        self.at == self.bytes.len()
    }

    /// Fail unless every byte has been read.
    pub fn finish(&self) -> Result<(), SegmentError> {
        if self.finished() {
            Ok(())
        } else {
            Err(self.error("trailing bytes"))
        }
    }

    /// The next `n` bytes.
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], SegmentError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| self.error("unexpected end of input"))?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], SegmentError> {
        let slice = self.take(N)?;
        let mut out = [0_u8; N];
        out.copy_from_slice(slice);
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8, SegmentError> {
        Ok(self.array::<1>()?[0])
    }

    pub fn bool(&mut self) -> Result<bool, SegmentError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(self.error(format!("boolean byte {other}"))),
        }
    }

    pub fn u16(&mut self) -> Result<u16, SegmentError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32, SegmentError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn u64(&mut self) -> Result<u64, SegmentError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    pub fn i64(&mut self) -> Result<i64, SegmentError> {
        Ok(i64::from_le_bytes(self.array()?))
    }

    /// A count of items each at least `min_item` bytes long: refused when
    /// the bytes left cannot hold that many.
    pub fn count(&mut self, min_item: usize) -> Result<usize, SegmentError> {
        let raw = self.u64()?;
        let left = self.bytes.len() - self.at;
        usize::try_from(raw)
            .ok()
            .filter(|n| n.saturating_mul(min_item.max(1)) <= left)
            .ok_or_else(|| self.error(format!("count {raw} exceeds the input")))
    }

    pub fn byte_string(&mut self) -> Result<&'a [u8], SegmentError> {
        let n = self.count(1)?;
        self.take(n)
    }

    pub fn string(&mut self) -> Result<String, SegmentError> {
        let raw = self.byte_string()?;
        std::str::from_utf8(raw)
            .map(str::to_owned)
            .map_err(|_| self.error("a string is not UTF-8"))
    }

    pub fn name(&mut self) -> Result<Name, SegmentError> {
        self.string().map(Name::from)
    }

    pub fn opt_name(&mut self) -> Result<Option<Name>, SegmentError> {
        if self.bool()? {
            self.name().map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn opt_u32(&mut self) -> Result<Option<u32>, SegmentError> {
        if self.bool()? {
            self.u32().map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn digest(&mut self) -> Result<Digest, SegmentError> {
        self.array().map(Digest)
    }

    pub fn edge(&mut self) -> Result<Edge, SegmentError> {
        Ok(Edge {
            src: self.name()?,
            tgt: self.name()?,
            kind: self.name()?,
            name: self.opt_name()?,
        })
    }

    /// A range, refused when it is inverted (`segment` names its owner).
    pub fn range(&mut self, segment: SegmentId) -> Result<IdRange, SegmentError> {
        IdRange {
            lo: self.u32()?,
            hi: self.u32()?,
        }
        .check(segment)
    }

    pub fn ranges(&mut self, segment: SegmentId) -> Result<Vec<IdRange>, SegmentError> {
        let n = self.count(8)?;
        (0..n).map(|_| self.range(segment)).collect()
    }

    pub fn opt_range(&mut self, segment: SegmentId) -> Result<Option<IdRange>, SegmentError> {
        if self.bool()? {
            self.range(segment).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn view_ref(&mut self) -> Result<ViewRef, SegmentError> {
        match self.u8()? {
            0 => Ok(ViewRef::Node(self.u32()?)),
            1 => Ok(ViewRef::Edge(crate::EdgeId(self.u32()?))),
            2 => Ok(ViewRef::Arc {
                parent: self.u32()?,
                child: self.u32()?,
            }),
            3 => Ok(ViewRef::Row(RowId(self.u32()?))),
            4 => Ok(ViewRef::Pair {
                edge: self.edge()?,
                src: RowId(self.u32()?),
                tgt: RowId(self.u32()?),
            }),
            other => Err(self.error(format!("element tag {other}"))),
        }
    }

    pub fn value_map(&mut self, depth: usize) -> Result<HashMap<String, Value>, SegmentError> {
        let n = self.count(9)?;
        let mut map = HashMap::with_capacity(n);
        let mut last: Option<String> = None;
        for _ in 0..n {
            let key = self.string()?;
            if last.as_ref().is_some_and(|l| *l >= key) {
                return Err(self.error("map keys are not strictly ascending"));
            }
            let value = self.value(depth + 1)?;
            last = Some(key.clone());
            map.insert(key, value);
        }
        Ok(map)
    }

    /// An instance value, as [`CanonicalSink::value`](crate::CanonicalSink::value)
    /// writes it.
    pub fn value(&mut self, depth: usize) -> Result<Value, SegmentError> {
        if depth > MAX_VALUE_DEPTH {
            return Err(self.error("a value is nested too deeply"));
        }
        let tag = self.u8()?;
        Ok(match tag {
            0 => Value::Bool(self.bool()?),
            1 => Value::Int(self.i64()?),
            2 => Value::Float(f64::from_bits(self.u64()?)),
            3 => Value::Str(self.string()?),
            4 => Value::Bytes(self.byte_string()?.to_vec()),
            5 => Value::CidLink(self.string()?),
            6 => Value::Blob {
                ref_: self.string()?,
                mime: self.string()?,
                size: self.u64()?,
            },
            7 => Value::Token(self.string()?),
            8 => Value::Null,
            9 => Value::Opaque {
                type_: self.string()?,
                fields: self.value_map(depth)?,
            },
            10 => Value::Unknown(self.value_map(depth)?),
            11 => {
                let n = self.count(1)?;
                let items = (0..n)
                    .map(|_| self.value(depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                Value::List(items)
            }
            12 => Value::LabeledNull(self.u64()?),
            13 => {
                let negative = self.bool()?;
                let digits = self.string()?;
                let exponent = self.wide_int("a decimal exponent")?;
                let decimal = DecimalValue::new(negative, digits, exponent)
                    .map_err(|e| self.error(e.to_string()))?;
                if decimal.negative != negative {
                    return Err(self.error("a decimal zero is negative"));
                }
                Value::Decimal(decimal)
            }
            14 => {
                let negative = self.bool()?;
                let significand = self.string()?;
                let zeros = self.decimal_digits("a big integer's count of zeros")?;
                let big = BigIntValue {
                    negative,
                    significand,
                    zeros: WideInt::from_sign_and_digits(false, &zeros)
                        .unwrap_or_default(),
                };
                if !big.is_well_formed() {
                    return Err(self.error("a big integer is not in its canonical form"));
                }
                Value::BigInt(big)
            }
            15 => Value::Temporal(self.temporal()?),
            other => return Err(self.error(format!("value tag {other}"))),
        })
    }

    /// A string of ASCII digits with no leading zero (`"0"` alone for
    /// zero), behind its length; `what` names it in the error.
    fn decimal_digits(&mut self, what: &str) -> Result<String, SegmentError> {
        let digits = self.string()?;
        let canonical = !digits.is_empty()
            && digits.bytes().all(|b| b.is_ascii_digit())
            && (digits == "0" || !digits.starts_with('0'));
        if canonical {
            Ok(digits)
        } else {
            Err(self.error(format!(
                "{what} is not decimal digits without a leading zero"
            )))
        }
    }

    /// An integer of any size: a sign byte (`0` non-negative, `1`
    /// negative) and the digits of its magnitude, as
    /// [`WideInt::write_canonical`] writes them. A negative zero is refused,
    /// so each integer has one byte form.
    fn wide_int(&mut self, what: &str) -> Result<WideInt, SegmentError> {
        let negative = match self.u8()? {
            0 => false,
            1 => true,
            other => return Err(self.error(format!("{what} has sign byte {other}"))),
        };
        let magnitude = self.decimal_digits(what)?;
        if negative && magnitude == "0" {
            return Err(self.error(format!("{what} is a negative zero")));
        }
        WideInt::from_sign_and_digits(negative, &magnitude)
            .ok_or_else(|| self.error(format!("{what} is not an integer")))
    }

    fn civil_date(&mut self) -> Result<CivilDate, SegmentError> {
        let year = i32::try_from(self.i64()?).map_err(|_| self.error("a year leaves i32"))?;
        let (month, day) = (self.u8()?, self.u8()?);
        CivilDate::new(year, month, day).map_err(|e| self.error(e.to_string()))
    }

    /// A time of day: hour, minute and second bytes, then the fraction's
    /// digit count as a little-endian `u64` and its ASCII digits. A count
    /// beyond the bytes left and a byte that is not a digit are refused.
    fn civil_time(&mut self) -> Result<CivilTime, SegmentError> {
        let (hour, minute, second) = (self.u8()?, self.u8()?, self.u8()?);
        let digits = self.byte_string()?;
        let fraction = std::str::from_utf8(digits)
            .ok()
            .and_then(|d| Fraction::new(d).ok())
            .ok_or_else(|| self.error("a fraction of a second holds a byte that is not a digit"))?;
        CivilTime::new(hour, minute, second, fraction).map_err(|e| self.error(e.to_string()))
    }

    /// An offset: tag `0` local, `1` `Z` followed by the byte of the letter
    /// written (`Z` or `z`), `2` a fixed offset followed by its minutes as a
    /// little-endian `i64`, `3` `-00:00`.
    fn offset(&mut self) -> Result<Offset, SegmentError> {
        match self.u8()? {
            0 => Ok(Offset::Local),
            1 => {
                let byte = self.u8()?;
                ZuluCase::from_byte(byte)
                    .map(Offset::Zulu)
                    .ok_or_else(|| self.error(format!("Z offset case byte {byte}")))
            }
            2 => {
                let minutes =
                    i16::try_from(self.i64()?).map_err(|_| self.error("an offset leaves i16"))?;
                Offset::fixed(minutes).map_err(|e| self.error(e.to_string()))
            }
            3 => Ok(Offset::NegZero),
            other => Err(self.error(format!("offset tag {other}"))),
        }
    }

    fn temporal(&mut self) -> Result<Temporal, SegmentError> {
        match self.u8()? {
            0 => Ok(Temporal::Date(self.civil_date()?)),
            1 => Ok(Temporal::Time(self.civil_time()?, self.offset()?)),
            2 => {
                let date = self.civil_date()?;
                let byte = self.u8()?;
                let separator = DateTimeSeparator::from_byte(byte)
                    .ok_or_else(|| self.error(format!("date-time separator byte {byte}")))?;
                let time = self.civil_time()?;
                Ok(Temporal::DateTime(date, time, self.offset()?, separator))
            }
            other => Err(self.error(format!("temporal tag {other}"))),
        }
    }

    /// A node record, as [`write_node_record`](crate::write_node_record)
    /// writes it.
    pub fn node(&mut self) -> Result<Node, SegmentError> {
        let id = self.u32()?;
        let anchor = self.name()?;
        let value = match self.u8()? {
            0 => None,
            1 => Some(FieldPresence::Present(self.value(0)?)),
            2 => Some(FieldPresence::Null),
            3 => Some(FieldPresence::Absent),
            other => return Err(self.error(format!("node value tag {other}"))),
        };
        let discriminator = self.opt_name()?;
        let extra_fields = self.value_map(0)?;
        let position = self.opt_u32()?;
        let shape = match self.u8()? {
            0 => NodeShape::Plain,
            1 => NodeShape::List,
            2 => NodeShape::XmlElement { tag: self.name()? },
            3 => NodeShape::XmlTextSegment,
            4 => NodeShape::HyperEdge,
            other => return Err(self.error(format!("node shape tag {other}"))),
        };
        let annotations = self.value_map(0)?;
        let mut node = Node::new(id, anchor);
        node.value = value;
        node.discriminator = discriminator;
        node.extra_fields = extra_fields;
        node.position = position;
        node.shape = shape;
        node.annotations = annotations;
        Ok(node)
    }
}

#[cfg(test)]
mod tests {
    use panproto_schema::CanonicalWriter;

    use super::*;
    use crate::identity::CanonicalSink;
    use crate::value::ExactInteger;

    /// The canonical bytes of `value`, as a segment writes them.
    fn written(value: &Value) -> Vec<u8> {
        let mut out = CanonicalWriter::new();
        out.value(value);
        out.into_bytes()
    }

    /// The value `bytes` decode to, every byte read.
    fn read(bytes: &[u8]) -> Result<Value, SegmentError> {
        let mut reader = Reader::new(bytes, ArtifactKind::Segment);
        let value = reader.value(0)?;
        reader.finish()?;
        Ok(value)
    }

    /// The value of `result`, or a panic naming its error.
    fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("unexpected error: {error:?}"),
        }
    }

    /// The message of the decode error `result` must be.
    fn message(result: Result<Value, SegmentError>) -> String {
        match result {
            Err(SegmentError::Decode { message, .. }) => message,
            other => panic!("expected a decode error, got {other:?}"),
        }
    }

    #[test]
    fn segment_reader_decodes_every_temporal_layout() {
        let offsets = [
            ("", Offset::Local),
            ("Z", Offset::Zulu(ZuluCase::Upper)),
            ("z", Offset::Zulu(ZuluCase::Lower)),
            ("+00:00", Offset::Fixed(0)),
            ("-08:00", Offset::Fixed(-480)),
            ("+05:30", Offset::Fixed(330)),
            ("-00:00", Offset::NegZero),
        ];
        let digits = "123456789012345678901234567890";
        for (written_offset, offset) in offsets {
            for n in [0_usize, 3, 9, 10, 30] {
                let fraction = if n == 0 {
                    String::new()
                } else {
                    format!(".{}", &digits[..n])
                };
                for text in [
                    format!("12:34:56{fraction}{written_offset}"),
                    format!("2026-09-28T23:59:60{fraction}{written_offset}"),
                    format!("2026-09-28t23:59:60{fraction}{written_offset}"),
                    format!("2026-09-28 23:59:60{fraction}{written_offset}"),
                ] {
                    let temporal = ok(Temporal::parse(&text));
                    assert_eq!(temporal.offset(), Some(offset), "{text}");
                    let value = Value::Temporal(temporal);
                    let bytes = written(&value);
                    assert_eq!(ok(read(&bytes)), value, "{text}");
                    assert_eq!(ok(read(&bytes)).canonical_bytes(), value.canonical_bytes());
                    let Value::Temporal(back) = ok(read(&bytes)) else {
                        panic!("a temporal")
                    };
                    assert_eq!(back.to_string(), text, "{text}");
                }
            }
        }
        let date = Value::Temporal(ok(Temporal::parse("-0044-03-15")));
        assert_eq!(ok(read(&written(&date))), date);
    }

    #[test]
    fn segment_reader_refuses_a_malformed_fraction() {
        let value = Value::Temporal(ok(Temporal::parse("12:00:00.25Z")));
        let bytes = written(&value);
        // tag 15, form 1, hour, minute, second, then the u64 digit count.
        let count_at = 5;
        assert_eq!(bytes[count_at..count_at + 8], 2_u64.to_le_bytes());
        // A count beyond the bytes left.
        let mut long = bytes.clone();
        long[count_at..count_at + 8].copy_from_slice(&5_u64.to_le_bytes());
        let error = message(read(&long));
        assert!(error.starts_with("count 5 exceeds the input"), "{error}");
        let mut huge = bytes.clone();
        huge[count_at..count_at + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(message(read(&huge)).contains("exceeds the input"));
        // A byte that is not a digit, ASCII or not.
        for byte in [b'a', b'.', b'/', b':', 0xff, 0x00] {
            let mut bad = bytes.clone();
            bad[count_at + 8] = byte;
            let error = message(read(&bad));
            assert!(error.contains("not a digit"), "{byte}: {error}");
        }
        // The digits themselves decode.
        assert_eq!(ok(read(&bytes)), value);
    }

    #[test]
    fn segment_reader_refuses_an_unknown_offset_tag() {
        let value = Value::Temporal(ok(Temporal::parse("12:00:00-00:00")));
        let mut bytes = written(&value);
        let last = bytes.len() - 1;
        assert_eq!(bytes[last], 3);
        for (tag, offset) in [(0, Offset::Local), (3, Offset::NegZero)] {
            bytes[last] = tag;
            let Value::Temporal(t) = ok(read(&bytes)) else {
                panic!("a temporal")
            };
            assert_eq!(t.offset(), Some(offset));
        }
        bytes[last] = 4;
        assert!(message(read(&bytes)).starts_with("offset tag 4"));
        // Tag 1 is followed by the letter's byte, `Z` or `z`, and nothing
        // else.
        let zulu = Value::Temporal(ok(Temporal::parse("12:00:00Z")));
        let mut bytes = written(&zulu);
        let last = bytes.len() - 1;
        assert_eq!(bytes[last - 1..], [1, b'Z']);
        for (byte, case) in [(b'Z', ZuluCase::Upper), (b'z', ZuluCase::Lower)] {
            bytes[last] = byte;
            let Value::Temporal(t) = ok(read(&bytes)) else {
                panic!("a temporal")
            };
            assert_eq!(t.offset(), Some(Offset::Zulu(case)));
        }
        for byte in [0, b'U', b'T', 0xff] {
            bytes[last] = byte;
            assert!(message(read(&bytes)).starts_with(&format!("Z offset case byte {byte}")));
        }
        // A date-time's separator byte sits between its date and its time.
        let date_time = Value::Temporal(ok(Temporal::parse("2024-01-02 03:04:05Z")));
        let mut bytes = written(&date_time);
        // tag 15, form 2, the year (8 bytes), month and day.
        let at = 1 + 1 + 8 + 2;
        assert_eq!(bytes[at], b' ');
        for byte in [b'T', b't'] {
            bytes[at] = byte;
            assert!(read(&bytes).is_ok());
        }
        for byte in [b'x', 0, b'Z'] {
            bytes[at] = byte;
            assert!(message(read(&bytes)).starts_with(&format!("date-time separator byte {byte}")));
        }
    }

    /// The bytes of a `WideInt`: its sign byte and its magnitude's digits
    /// behind their length.
    fn wide(negative: bool, digits: &str) -> Vec<u8> {
        let mut out = vec![u8::from(negative)];
        out.extend_from_slice(&u64::try_from(digits.len()).unwrap_or(0).to_le_bytes());
        out.extend_from_slice(digits.as_bytes());
        out
    }

    /// The canonical bytes of the decimal `(negative, digits, exponent)`
    /// under tag 13, written field by field.
    fn decimal_bytes(negative: bool, digits: &str, exponent: &[u8]) -> Vec<u8> {
        let mut out = vec![13, u8::from(negative)];
        out.extend_from_slice(&u64::try_from(digits.len()).unwrap_or(0).to_le_bytes());
        out.extend_from_slice(digits.as_bytes());
        out.extend_from_slice(exponent);
        out
    }

    #[test]
    fn segment_reader_decodes_every_decimal_layout() {
        for exponent in [
            "-2147483649",
            "2147483648",
            "3000000000",
            "-3000000001",
            "100000000000000000000",
            "-100000000000000000000",
            "0",
            "-2",
        ] {
            for (negative, digits) in [(false, "1"), (true, "150"), (false, "0")] {
                let value = Value::Decimal(ok(DecimalValue::new(
                    negative,
                    digits,
                    WideInt::parse_canonical(exponent).unwrap_or_default(),
                )));
                let bytes = written(&value);
                let magnitude = exponent.trim_start_matches('-');
                assert_eq!(
                    bytes,
                    decimal_bytes(negative, digits, &wide(exponent.starts_with('-'), magnitude))
                );
                assert_eq!(ok(read(&bytes)), value, "{exponent}");
            }
        }
        // An exponent magnitude with a leading zero, a negative zero
        // exponent, a sign byte other than 0 or 1, and digits that are not
        // decimal digits are refused.
        for (exponent, what) in [
            (wide(false, "012"), "not decimal digits without a leading zero"),
            (wide(true, "0"), "is a negative zero"),
            (wide(false, ""), "not decimal digits without a leading zero"),
            (wide(false, "1a"), "not decimal digits without a leading zero"),
            ([vec![2], wide(false, "1")[1..].to_vec()].concat(), "has sign byte 2"),
        ] {
            let error = message(read(&decimal_bytes(false, "1", &exponent)));
            assert!(error.contains(what), "{error}");
        }
    }

    /// The canonical bytes of the big integer `(negative, significand,
    /// zeros)` under tag 14, written field by field.
    fn bigint_bytes(negative: bool, significand: &str, zeros: &str) -> Vec<u8> {
        let mut out = vec![14, u8::from(negative)];
        for part in [significand, zeros] {
            out.extend_from_slice(&u64::try_from(part.len()).unwrap_or(0).to_le_bytes());
            out.extend_from_slice(part.as_bytes());
        }
        out
    }

    #[test]
    fn segment_reader_decodes_every_bigint_layout() {
        for zeros in ["0", "1", "4097", "100000000000000000000"] {
            for (negative, significand) in [(false, "9223372036854775808"), (true, "9223372036854775809")] {
                let Some(ExactInteger::Big(big)) = BigIntValue::from_scaled(
                    negative,
                    significand,
                    &WideInt::parse_canonical(zeros).unwrap_or_default(),
                ) else {
                    panic!("{significand}e{zeros} is a big integer")
                };
                let value = Value::BigInt(big);
                let bytes = written(&value);
                assert_eq!(bytes, bigint_bytes(negative, significand, zeros));
                assert_eq!(ok(read(&bytes)), value, "{significand}e{zeros}");
            }
        }
        // A significand ending in 0, a count with a leading zero, a value
        // that fits i64 and a significand with a leading zero are refused.
        for (significand, zeros, what) in [
            ("10", "20", "not in its canonical form"),
            ("1", "020", "not decimal digits without a leading zero"),
            ("12", "0", "not in its canonical form"),
            ("1", "18", "not in its canonical form"),
            ("01", "20", "not in its canonical form"),
            ("1", "", "not decimal digits without a leading zero"),
        ] {
            let error = message(read(&bigint_bytes(false, significand, zeros)));
            assert!(error.contains(what), "{significand} {zeros}: {error}");
        }
    }
}
