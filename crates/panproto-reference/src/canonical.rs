//! Canonical byte encoders and their left inverses.
//!
//! Each encoder writes the layout its definition states (integers
//! little-endian, every variable-length run behind a `u64` count, every sum
//! behind a one-byte discriminant). Each decoder reads that layout back and
//! refuses anything else (a wrong tag, an out-of-order record, a trailing
//! byte). A decoder that returns the encoded value for every value is the
//! injectivity witness of its encoder: `decode(encode(x)) == x` means two
//! values with equal bytes are equal.

use std::collections::{BTreeMap, BTreeSet};

use crate::exact::{BigIntValue, Decimal, DecimalValue};
use crate::model::{Fan, GEdge, Graph, Node, NodeShape, SEdge, Table, Tree};
use crate::num::{Int, Nat};
use crate::temporal::{
    DateFields, Offset, OffsetForm, Separator, Temporal, TimeFields, Year, ZuluCase,
};
use crate::value::{Presence, Value};

/// A byte sink with the canonical primitive encoders.
#[derive(Clone, Debug, Default)]
pub struct Writer {
    /// The bytes written.
    pub bytes: Vec<u8>,
}

impl Writer {
    /// A one-byte discriminant.
    pub fn tag(&mut self, t: u8) {
        self.bytes.push(t);
    }
    /// A count or length as a little-endian `u64`.
    pub fn len(&mut self, n: usize) {
        self.u64(u64::try_from(n).unwrap_or(u64::MAX));
    }
    /// A little-endian `u32`.
    pub fn u32(&mut self, v: u32) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }
    /// A little-endian `u64`.
    pub fn u64(&mut self, v: u64) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }
    /// A little-endian `i64`.
    pub fn i64(&mut self, v: i64) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }
    /// A length-prefixed byte string.
    pub fn raw(&mut self, b: &[u8]) {
        self.len(b.len());
        self.bytes.extend_from_slice(b);
    }
    /// A length-prefixed UTF-8 string.
    pub fn str(&mut self, s: &str) {
        self.raw(s.as_bytes());
    }
    /// An optional name: presence byte, then the name.
    pub fn opt_name(&mut self, n: Option<&str>) {
        match n {
            None => self.tag(0),
            Some(s) => {
                self.tag(1);
                self.str(s);
            }
        }
    }
    /// An optional `u32`: presence byte, then the value.
    pub fn opt_u32(&mut self, v: Option<u32>) {
        match v {
            None => self.tag(0),
            Some(x) => {
                self.tag(1);
                self.u32(x);
            }
        }
    }
    /// A schema edge: source, target, kind, optional name.
    pub fn edge(&mut self, e: &SEdge) {
        self.str(&e.src);
        self.str(&e.tgt);
        self.str(&e.kind);
        self.opt_name(e.name.as_deref());
    }
}

/// A byte source with the inverse primitive decoders; each returns `None`
/// on a short or malformed input.
#[derive(Clone, Debug)]
pub struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    /// A reader at the start of `bytes`.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    /// Whether every byte has been read.
    #[must_use]
    pub const fn done(&self) -> bool {
        self.at == self.bytes.len()
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let s = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(s)
    }
    /// One byte.
    pub fn tag(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    /// A `u32`.
    pub fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    /// A `u64`.
    pub fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    /// An `i64`.
    pub fn i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    /// A count.
    pub fn count(&mut self) -> Option<usize> {
        usize::try_from(self.u64()?).ok()
    }
    /// A length-prefixed byte string.
    pub fn raw(&mut self) -> Option<Vec<u8>> {
        let n = self.count()?;
        self.take(n).map(<[u8]>::to_vec)
    }
    /// A length-prefixed UTF-8 string.
    pub fn str(&mut self) -> Option<String> {
        String::from_utf8(self.raw()?).ok()
    }
    /// A presence-prefixed name.
    pub fn opt_name(&mut self) -> Option<Option<String>> {
        match self.tag()? {
            0 => Some(None),
            1 => Some(Some(self.str()?)),
            _ => None,
        }
    }
    /// A presence-prefixed `u32`.
    pub fn opt_u32(&mut self) -> Option<Option<u32>> {
        match self.tag()? {
            0 => Some(None),
            1 => Some(Some(self.u32()?)),
            _ => None,
        }
    }
    /// A schema edge.
    pub fn edge(&mut self) -> Option<SEdge> {
        Some(SEdge {
            src: self.str()?,
            tgt: self.str()?,
            kind: self.str()?,
            name: self.opt_name()?,
        })
    }
}

/// The digits of a natural behind a length, minimal (zero is `"0"`).
fn write_digits(w: &mut Writer, n: &Nat) {
    w.str(&n.to_string());
}

/// Read minimal decimal digits: no leading zero unless the number is zero.
fn read_digits(r: &mut Reader<'_>) -> Option<Nat> {
    let s = r.str()?;
    if s.len() > 1 && s.starts_with('0') {
        return None;
    }
    Nat::parse(&s)
}

/// The canonical bytes of a decimal.
///
/// A finite one as its sign byte (1 for a negative number or zero), its digits behind a length, and
/// its exponent as a sign byte and the magnitude's minimal digits behind a length; an infinity as
/// byte 2 and its sign byte; a NaN as byte 3, its sign byte, its signalling byte and its payload's
/// minimal digits behind a length.
pub fn write_decimal(w: &mut Writer, d: &Decimal) {
    match d {
        Decimal::Finite(v) => {
            w.tag(u8::from(v.negative));
            write_digits(w, &v.digits);
            w.tag(u8::from(v.exponent.is_negative()));
            write_digits(w, v.exponent.magnitude());
        }
        Decimal::Infinite { negative } => {
            w.tag(2);
            w.tag(u8::from(*negative));
        }
        Decimal::NaN {
            negative,
            signalling,
            payload,
        } => {
            w.tag(3);
            w.tag(u8::from(*negative));
            w.tag(u8::from(*signalling));
            write_digits(w, payload);
        }
    }
}

fn read_bool(r: &mut Reader<'_>) -> Option<bool> {
    match r.tag()? {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// The inverse of [`write_decimal`].
pub fn read_decimal(r: &mut Reader<'_>) -> Option<Decimal> {
    match r.tag()? {
        s @ (0 | 1) => {
            let digits = read_digits(r)?;
            let en = read_bool(r)?;
            let mag = read_digits(r)?;
            if en && mag.is_zero() {
                return None;
            }
            Some(Decimal::Finite(DecimalValue {
                negative: s == 1,
                digits,
                exponent: Int::new(en, mag),
            }))
        }
        2 => Some(Decimal::Infinite {
            negative: read_bool(r)?,
        }),
        3 => Some(Decimal::NaN {
            negative: read_bool(r)?,
            signalling: read_bool(r)?,
            payload: read_digits(r)?,
        }),
        _ => None,
    }
}

/// The canonical bytes of an integer outside `i64`: sign byte, significand
/// digits behind a length, count of trailing zeros as minimal digits.
pub fn write_bigint(w: &mut Writer, b: &BigIntValue) {
    w.tag(u8::from(b.negative));
    write_digits(w, &b.significand);
    write_digits(w, &b.zeros);
}

/// The inverse of [`write_bigint`]; refuses a significand ending in 0 and a
/// value within `i64` (which is never a `BigInt`).
pub fn read_bigint(r: &mut Reader<'_>) -> Option<BigIntValue> {
    let negative = read_bool(r)?;
    let significand = read_digits(r)?;
    let zeros = read_digits(r)?;
    if significand.is_zero() || significand.digit(0) == 0 {
        return None;
    }
    let b = BigIntValue {
        negative,
        significand,
        zeros,
    };
    match crate::exact::ExactInteger::from_parts(b.negative, &b.significand, &b.zeros) {
        crate::exact::ExactInteger::Big(_) => Some(b),
        crate::exact::ExactInteger::Int(_) => None,
    }
}

/// Whether a temporal is in the domain of the complete-form layout: a full
/// date (local, year written without a sign on zero), a full time, or both,
/// with an offset of extended written form (or `Z`/`z`).
fn complete_layout(t: &Temporal) -> bool {
    let offset_ok = match t.offset {
        Offset::Local | Offset::Zulu(ZuluCase::Upper | ZuluCase::Lower) => true,
        Offset::Fixed(_, form) | Offset::NegZero(form) => form == OffsetForm::Extended,
        Offset::Zulu(ZuluCase::Unwritten) => false,
    };
    let date_ok = t.date.is_none_or(|d| {
        d.month.is_some()
            && d.day.is_some()
            && d.year.is_some_and(|y| {
                !y.negative_zero && y.width == y.value.unsigned_abs().to_string().len().max(4)
            })
    });
    let date_only_local = !(t.date.is_some() && t.time.is_none()) || t.offset == Offset::Local;
    t.is_complete() && offset_ok && date_ok && date_only_local
}

/// The canonical bytes of a temporal in the complete-form layout.
///
/// A form byte (0 date, 1 time, 2 date-time); a date as a little-endian `i64` year and a month and
/// a day byte; a date-time's separator byte (`T`, `t` or a space, as ASCII) between date and time;
/// a time as hour, minute and second bytes and its fraction digits behind a length; the offset as 0
/// local, 1 `Z` followed by its case byte (`Z` or `z` as ASCII), 2 fixed followed by its minutes as
/// an `i64`, 3 negative zero. `None` for a form outside that layout (reduced, truncated, or another
/// written offset form).
#[must_use]
pub fn write_temporal(w: &mut Writer, t: &Temporal) -> Option<()> {
    if !complete_layout(t) {
        return None;
    }
    let form = match (&t.date, &t.time) {
        (Some(_), None) => 0,
        (None, Some(_)) => 1,
        _ => 2,
    };
    w.tag(form);
    if let Some(d) = &t.date {
        w.i64(d.year.map_or(0, |y| y.value));
        w.tag(d.month.unwrap_or(0));
        w.tag(d.day.unwrap_or(0));
    }
    if let Some(s) = t.separator {
        w.tag(match s {
            Separator::Upper => b'T',
            Separator::Lower => b't',
            Separator::Space => b' ',
        });
    }
    if let Some(tm) = &t.time {
        w.tag(tm.hour.unwrap_or(0));
        w.tag(tm.minute.unwrap_or(0));
        w.tag(tm.second.unwrap_or(0));
        w.str(&tm.fraction);
    }
    if t.time.is_some() {
        match t.offset {
            Offset::Local => w.tag(0),
            Offset::Zulu(case) => {
                w.tag(1);
                w.tag(if case == ZuluCase::Lower { b'z' } else { b'Z' });
            }
            Offset::Fixed(m, _) => {
                w.tag(2);
                w.i64(i64::from(m));
            }
            Offset::NegZero(_) => w.tag(3),
        }
    }
    Some(())
}

fn read_date(r: &mut Reader<'_>) -> Option<DateFields> {
    let year = r.i64()?;
    Some(DateFields {
        year: Some(Year {
            value: year,
            negative_zero: false,
            width: year.unsigned_abs().to_string().len().max(4),
        }),
        month: Some(r.tag()?),
        day: Some(r.tag()?),
    })
}

fn read_time(r: &mut Reader<'_>) -> Option<TimeFields> {
    let (hour, minute, second) = (r.tag()?, r.tag()?, r.tag()?);
    let fraction = r.str()?;
    if !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(TimeFields {
        hour: Some(hour),
        minute: Some(minute),
        second: Some(second),
        fraction,
    })
}

fn read_offset(r: &mut Reader<'_>) -> Option<Offset> {
    Some(match r.tag()? {
        0 => Offset::Local,
        1 => match r.tag()? {
            b'Z' => Offset::Zulu(ZuluCase::Upper),
            b'z' => Offset::Zulu(ZuluCase::Lower),
            _ => return None,
        },
        2 => {
            let m = i16::try_from(r.i64()?).ok()?;
            if m == 0 {
                Offset::Fixed(0, OffsetForm::Extended)
            } else {
                Offset::Fixed(m, OffsetForm::Extended)
            }
        }
        3 => Offset::NegZero(OffsetForm::Extended),
        _ => return None,
    })
}

/// The inverse of [`write_temporal`].
pub fn read_temporal(r: &mut Reader<'_>) -> Option<Temporal> {
    match r.tag()? {
        0 => Some(Temporal {
            date: Some(read_date(r)?),
            time: None,
            separator: None,
            offset: Offset::Local,
        }),
        1 => {
            let time = read_time(r)?;
            Some(Temporal {
                date: None,
                time: Some(time),
                separator: None,
                offset: read_offset(r)?,
            })
        }
        2 => {
            let date = read_date(r)?;
            let separator = match r.tag()? {
                b'T' => Separator::Upper,
                b't' => Separator::Lower,
                b' ' => Separator::Space,
                _ => return None,
            };
            let time = read_time(r)?;
            Some(Temporal {
                date: Some(date),
                time: Some(time),
                separator: Some(separator),
                offset: read_offset(r)?,
            })
        }
        _ => None,
    }
}

fn write_map(w: &mut Writer, m: &BTreeMap<String, Value>) -> Option<()> {
    w.len(m.len());
    for (k, v) in m {
        w.str(k);
        write_value(w, v)?;
    }
    Some(())
}

fn read_map(r: &mut Reader<'_>) -> Option<BTreeMap<String, Value>> {
    let n = r.count()?;
    let mut out = BTreeMap::new();
    let mut last: Option<String> = None;
    for _ in 0..n {
        let k = r.str()?;
        if last.as_ref().is_some_and(|l| *l >= k) {
            return None;
        }
        let v = read_value(r)?;
        last = Some(k.clone());
        out.insert(k, v);
    }
    Some(out)
}

/// The canonical bytes of a value.
///
/// A tag (0 `Bool` to 15 `Temporal`) and its payload; maps as their count and entries in ascending
/// key order. `None` when a temporal inside lies outside the complete-form layout.
pub fn write_value(w: &mut Writer, v: &Value) -> Option<()> {
    match v {
        Value::Bool(b) => {
            w.tag(0);
            w.tag(u8::from(*b));
        }
        Value::Int(i) => {
            w.tag(1);
            w.i64(*i);
        }
        Value::Float(bits) => {
            w.tag(2);
            w.u64(*bits);
        }
        Value::Str(s) => {
            w.tag(3);
            w.str(s);
        }
        Value::Bytes(b) => {
            w.tag(4);
            w.raw(b);
        }
        Value::CidLink(s) => {
            w.tag(5);
            w.str(s);
        }
        Value::Blob { ref_, mime, size } => {
            w.tag(6);
            w.str(ref_);
            w.str(mime);
            w.u64(*size);
        }
        Value::Token(s) => {
            w.tag(7);
            w.str(s);
        }
        Value::Null => w.tag(8),
        Value::Opaque { type_, fields } => {
            w.tag(9);
            w.str(type_);
            write_map(w, fields)?;
        }
        Value::Unknown(fields) => {
            w.tag(10);
            write_map(w, fields)?;
        }
        Value::List(items) => {
            w.tag(11);
            w.len(items.len());
            for i in items {
                write_value(w, i)?;
            }
        }
        Value::LabeledNull(id) => {
            w.tag(12);
            w.u64(*id);
        }
        Value::Decimal(d) => {
            w.tag(13);
            write_decimal(w, d);
        }
        Value::BigInt(b) => {
            w.tag(14);
            write_bigint(w, b);
        }
        Value::Temporal(t) => {
            w.tag(15);
            write_temporal(w, t)?;
        }
    }
    Some(())
}

/// The inverse of [`write_value`].
pub fn read_value(r: &mut Reader<'_>) -> Option<Value> {
    Some(match r.tag()? {
        0 => Value::Bool(read_bool(r)?),
        1 => Value::Int(r.i64()?),
        2 => Value::Float(r.u64()?),
        3 => Value::Str(r.str()?),
        4 => Value::Bytes(r.raw()?),
        5 => Value::CidLink(r.str()?),
        6 => Value::Blob {
            ref_: r.str()?,
            mime: r.str()?,
            size: r.u64()?,
        },
        7 => Value::Token(r.str()?),
        8 => Value::Null,
        9 => Value::Opaque {
            type_: r.str()?,
            fields: read_map(r)?,
        },
        10 => Value::Unknown(read_map(r)?),
        11 => {
            let n = r.count()?;
            let mut items = Vec::new();
            for _ in 0..n {
                items.push(read_value(r)?);
            }
            Value::List(items)
        }
        12 => Value::LabeledNull(r.u64()?),
        13 => Value::Decimal(read_decimal(r)?),
        14 => Value::BigInt(read_bigint(r)?),
        15 => Value::Temporal(read_temporal(r)?),
        _ => return None,
    })
}

/// The canonical bytes of one value, or `None` outside the layout.
#[must_use]
pub fn value_bytes(v: &Value) -> Option<Vec<u8>> {
    let mut w = Writer::default();
    write_value(&mut w, v)?;
    Some(w.bytes)
}

/// Decode one value, refusing trailing bytes.
#[must_use]
pub fn decode_value(bytes: &[u8]) -> Option<Value> {
    let mut r = Reader::new(bytes);
    let v = read_value(&mut r)?;
    r.done().then_some(v)
}

/// Canonical bytes of an exact decimal, including all special values.
#[must_use]
pub fn decimal_bytes(value: &Decimal) -> Option<Vec<u8>> {
    value_bytes(&Value::Decimal(value.clone()))
}

/// The instance shape whose complement an identity envelope accompanies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ComplementShape {
    /// A tree/W-type complement.
    WType,
    /// A graph complement.
    Graph,
    /// A table/functor complement.
    Functor,
    /// A reference-resolution complement.
    Resolution,
}

/// The inputs to the version-2 identity-envelope encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IdentityEnvelope {
    /// The source schema digest.
    pub source_schema: [u8; 32],
    /// The target schema digest.
    pub target_schema: [u8; 32],
    /// The compiled lens digest.
    pub lens: [u8; 32],
    /// The complement shape.
    pub shape: ComplementShape,
    /// The shape-specific complement-format version.
    pub format_version: u16,
}

/// Encode a version-2 identity envelope exactly as the segmented canonical
/// sink does.
///
/// The layout is little-endian version 2, three 32-byte digests, the shape
/// tag (`WType = 0`, `Graph = 1`, `Functor = 2`, `Resolution = 3`), and the
/// little-endian complement-format version.
#[must_use]
pub fn identity(envelope: &IdentityEnvelope) -> [u8; 101] {
    let mut bytes = [0; 101];
    bytes[..2].copy_from_slice(&2u16.to_le_bytes());
    bytes[2..34].copy_from_slice(&envelope.source_schema);
    bytes[34..66].copy_from_slice(&envelope.target_schema);
    bytes[66..98].copy_from_slice(&envelope.lens);
    bytes[98] = match envelope.shape {
        ComplementShape::WType => 0,
        ComplementShape::Graph => 1,
        ComplementShape::Functor => 2,
        ComplementShape::Resolution => 3,
    };
    bytes[99..].copy_from_slice(&envelope.format_version.to_le_bytes());
    bytes
}

/// The canonical order of values: the byte order of their canonical bytes
/// (a value outside the layout sorts before every value inside it).
#[must_use]
pub fn canonical_cmp(a: &Value, b: &Value) -> std::cmp::Ordering {
    value_bytes(a).cmp(&value_bytes(b))
}

/// A node record.
///
/// Id, anchor, value (0 none, 1 present and the value, 2 null, 3 absent), discriminator, fields,
/// position, shape (0 plain, 1 list, 2 XML element and its tag, 3 text segment, 4 hyper-edge) and
/// annotations.
pub fn write_node(w: &mut Writer, n: &Node) -> Option<()> {
    w.u32(n.id);
    w.str(&n.anchor);
    match &n.value {
        None => w.tag(0),
        Some(Presence::Present(v)) => {
            w.tag(1);
            write_value(w, v)?;
        }
        Some(Presence::Null) => w.tag(2),
        Some(Presence::Absent) => w.tag(3),
    }
    w.opt_name(n.discriminator.as_deref());
    write_map(w, &n.fields)?;
    w.opt_u32(n.position);
    match &n.shape {
        NodeShape::Plain => w.tag(0),
        NodeShape::List => w.tag(1),
        NodeShape::XmlElement(tag) => {
            w.tag(2);
            w.str(tag);
        }
        NodeShape::XmlTextSegment => w.tag(3),
        NodeShape::HyperEdge => w.tag(4),
    }
    write_map(w, &n.annotations)
}

/// The inverse of [`write_node`].
pub fn read_node(r: &mut Reader<'_>) -> Option<Node> {
    let id = r.u32()?;
    let anchor = r.str()?;
    let value = match r.tag()? {
        0 => None,
        1 => Some(Presence::Present(read_value(r)?)),
        2 => Some(Presence::Null),
        3 => Some(Presence::Absent),
        _ => return None,
    };
    let discriminator = r.opt_name()?;
    let fields = read_map(r)?;
    let position = r.opt_u32()?;
    let shape = match r.tag()? {
        0 => NodeShape::Plain,
        1 => NodeShape::List,
        2 => NodeShape::XmlElement(r.str()?),
        3 => NodeShape::XmlTextSegment,
        4 => NodeShape::HyperEdge,
        _ => return None,
    };
    let annotations = read_map(r)?;
    Some(Node {
        id,
        anchor,
        value,
        discriminator,
        fields,
        position,
        shape,
        annotations,
    })
}

/// The canonical bytes of a graph: node and edge counts, node records in
/// ascending id, edge records `(id, src, tgt, label, position)` in
/// ascending edge id.
#[must_use]
pub fn graph_bytes(g: &Graph) -> Option<Vec<u8>> {
    let mut w = Writer::default();
    w.len(g.nodes.len());
    w.len(g.edges.len());
    for n in g.nodes.values() {
        write_node(&mut w, n)?;
    }
    for (id, e) in &g.edges {
        w.u32(*id);
        w.u32(e.src);
        w.u32(e.tgt);
        w.edge(&e.label);
        w.opt_u32(e.position);
    }
    Some(w.bytes)
}

/// The inverse of [`graph_bytes`], refusing records out of ascending order,
/// dangling endpoints and trailing bytes.
#[must_use]
pub fn decode_graph(bytes: &[u8]) -> Option<Graph> {
    let mut r = Reader::new(bytes);
    let (nn, ne) = (r.count()?, r.count()?);
    let mut g = Graph::default();
    for _ in 0..nn {
        let n = read_node(&mut r)?;
        if g.nodes.keys().next_back().is_some_and(|last| *last >= n.id) {
            return None;
        }
        g.nodes.insert(n.id, n);
    }
    for _ in 0..ne {
        let id = r.u32()?;
        if g.edges.keys().next_back().is_some_and(|last| *last >= id) {
            return None;
        }
        let e = GEdge {
            src: r.u32()?,
            tgt: r.u32()?,
            label: r.edge()?,
            position: r.opt_u32()?,
        };
        g.edges.insert(id, e);
    }
    (r.done() && g.well_formed()).then_some(g)
}

/// The canonical bytes of a table: row and pair counts, row records
/// `(id, table, cells)` in ascending id, pair records `(edge, src, tgt)` in
/// ascending order.
#[must_use]
pub fn table_bytes(t: &Table) -> Option<Vec<u8>> {
    let mut w = Writer::default();
    w.len(t.rows.len());
    w.len(t.pairs.len());
    for (id, (table, cells)) in &t.rows {
        w.u32(*id);
        w.str(table);
        write_map(&mut w, cells)?;
    }
    for (e, s, d) in &t.pairs {
        w.edge(e);
        w.u32(*s);
        w.u32(*d);
    }
    Some(w.bytes)
}

/// The inverse of [`table_bytes`].
#[must_use]
pub fn decode_table(bytes: &[u8]) -> Option<Table> {
    let mut r = Reader::new(bytes);
    let (nr, np) = (r.count()?, r.count()?);
    let mut t = Table::default();
    for _ in 0..nr {
        let id = r.u32()?;
        if t.rows.keys().next_back().is_some_and(|last| *last >= id) {
            return None;
        }
        let table = r.str()?;
        t.rows.insert(id, (table, read_map(&mut r)?));
    }
    let mut pairs = Vec::new();
    for _ in 0..np {
        pairs.push((r.edge()?, r.u32()?, r.u32()?));
    }
    if pairs.windows(2).any(|w| w[0] >= w[1]) {
        return None;
    }
    t.pairs = pairs.into_iter().collect::<BTreeSet<_>>();
    (r.done() && t.well_formed()).then_some(t)
}

/// A fan's canonical record: parent, hyper-edge, children by label.
fn fan_record(f: &Fan) -> (u32, String, Vec<(String, u32)>) {
    (
        f.parent,
        f.hyper_edge.clone(),
        f.children.iter().map(|(l, n)| (l.clone(), *n)).collect(),
    )
}

/// The canonical bytes of a tree.
///
/// Node, arc and fan counts, node records in ascending id, arc records `(parent, child, label)` in
/// arc order, fan records in ascending record order. The root and the fans' listed order are not
/// written.
#[must_use]
pub fn tree_bytes(t: &Tree) -> Option<Vec<u8>> {
    let mut w = Writer::default();
    w.len(t.nodes.len());
    w.len(t.arcs.len());
    w.len(t.fans.len());
    for n in t.nodes.values() {
        write_node(&mut w, n)?;
    }
    for (p, c, e) in &t.arcs {
        w.u32(*p);
        w.u32(*c);
        w.edge(e);
    }
    let mut fans: Vec<_> = t.fans.iter().map(fan_record).collect();
    fans.sort();
    for (p, h, ch) in fans {
        w.u32(p);
        w.str(&h);
        w.len(ch.len());
        for (l, n) in ch {
            w.str(&l);
            w.u32(n);
        }
    }
    Some(w.bytes)
}

/// The inverse of [`tree_bytes`] up to what the bytes do not hold: the
/// fans come back in record order and the root is the one node no arc
/// enters (`None` when that node is not unique).
#[must_use]
pub fn decode_tree(bytes: &[u8]) -> Option<Tree> {
    let mut r = Reader::new(bytes);
    let (nn, na, nf) = (r.count()?, r.count()?, r.count()?);
    let mut nodes = BTreeMap::new();
    for _ in 0..nn {
        let n = read_node(&mut r)?;
        if nodes
            .keys()
            .next_back()
            .is_some_and(|last: &u32| *last >= n.id)
        {
            return None;
        }
        nodes.insert(n.id, n);
    }
    let mut arcs = Vec::new();
    for _ in 0..na {
        arcs.push((r.u32()?, r.u32()?, r.edge()?));
    }
    let mut fans = Vec::new();
    let mut last = None;
    for _ in 0..nf {
        let parent = r.u32()?;
        let hyper_edge = r.str()?;
        let k = r.count()?;
        let mut children = Vec::new();
        for _ in 0..k {
            children.push((r.str()?, r.u32()?));
        }
        if children.windows(2).any(|w| w[0].0 >= w[1].0) {
            return None;
        }
        let rec = (parent, hyper_edge.clone(), children.clone());
        if last.as_ref().is_some_and(|l| *l > rec) {
            return None;
        }
        last = Some(rec);
        fans.push(Fan {
            parent,
            hyper_edge,
            children: children.into_iter().collect(),
        });
    }
    let roots: Vec<u32> = nodes
        .keys()
        .copied()
        .filter(|n| !arcs.iter().any(|(_, c, _)| c == n))
        .collect();
    let [root] = roots.as_slice() else {
        return None;
    };
    r.done().then_some(Tree {
        nodes,
        arcs,
        fans,
        root: *root,
    })
}
