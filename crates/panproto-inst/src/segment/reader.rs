//! Reading canonical bytes back.
//!
//! [`Reader`] is the inverse of the [`CanonicalSink`](crate::CanonicalSink)
//! encoders for the records a segment holds. It checks every length against
//! the bytes left, so a corrupt count cannot make it allocate more than the
//! input could hold, and every value it builds through the value types' own
//! checked constructors.

use std::collections::HashMap;

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
                let exponent = i32::try_from(self.i64()?)
                    .map_err(|_| self.error("a decimal exponent leaves i32"))?;
                let decimal = DecimalValue::new(negative, digits, exponent)
                    .map_err(|e| self.error(e.to_string()))?;
                if decimal.negative != negative {
                    return Err(self.error("a decimal zero is negative"));
                }
                Value::Decimal(decimal)
            }
            14 => {
                let big = BigIntValue {
                    negative: self.bool()?,
                    digits: self.string()?,
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

    fn civil_date(&mut self) -> Result<CivilDate, SegmentError> {
        let year = i32::try_from(self.i64()?).map_err(|_| self.error("a year leaves i32"))?;
        let (month, day) = (self.u8()?, self.u8()?);
        CivilDate::new(year, month, day).map_err(|e| self.error(e.to_string()))
    }

    fn civil_time(&mut self) -> Result<CivilTime, SegmentError> {
        let (hour, minute, second) = (self.u8()?, self.u8()?, self.u8()?);
        let nanos = self.u32()?;
        let frac_digits = self.u8()?;
        CivilTime::new(hour, minute, second, nanos, frac_digits)
            .map_err(|e| self.error(e.to_string()))
    }

    fn offset(&mut self) -> Result<Offset, SegmentError> {
        match self.u8()? {
            0 => Ok(Offset::Local),
            1 => Ok(Offset::Utc),
            2 => {
                let minutes =
                    i16::try_from(self.i64()?).map_err(|_| self.error("an offset leaves i16"))?;
                Offset::fixed(minutes).map_err(|e| self.error(e.to_string()))
            }
            3 => Ok(Offset::Unknown),
            other => Err(self.error(format!("offset tag {other}"))),
        }
    }

    fn temporal(&mut self) -> Result<Temporal, SegmentError> {
        match self.u8()? {
            0 => Ok(Temporal::Date(self.civil_date()?)),
            1 => Ok(Temporal::Time(self.civil_time()?, self.offset()?)),
            2 => Ok(Temporal::DateTime(
                self.civil_date()?,
                self.civil_time()?,
                self.offset()?,
            )),
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
