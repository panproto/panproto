//! Complete reference operations assigned to `values`.

pub mod canonical {
    //! Canonical byte encoders and their left inverses.
    //!
    //! Each encoder writes the layout its definition states (integers
    //! little-endian, every variable-length run behind a `u64` count, every sum
    //! behind a one-byte discriminant). Each decoder reads that layout back and
    //! refuses anything else (a wrong tag, an out-of-order record, a trailing
    //! byte). A decoder that returns the encoded value for every value is the
    //! injectivity witness of its encoder: `decode(encode(x)) == x` means two
    //! values with equal bytes are equal.

    use std::collections::BTreeMap;

    use crate::exact::{BigIntValue, Decimal, DecimalValue};
    use crate::model::SEdge;
    use crate::num::{Int, Nat};
    use crate::temporal::{
        DateFields, Offset, OffsetForm, Separator, Temporal, TemporalForm, TemporalInfinity,
        TimeFields, Year, ZuluCase,
    };
    use crate::value::Value;

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
        /// Current byte offset from the start of the input.
        #[must_use]
        pub const fn position(&self) -> usize {
            self.at
        }
        /// Bytes not yet consumed by this reader.
        #[must_use]
        pub const fn remaining(&self) -> &'a [u8] {
            self.bytes.split_at(self.at).1
        }
        /// Consume exactly `n` bytes.
        ///
        /// Returns `None` without advancing past the input when fewer than
        /// `n` bytes remain or the offset computation overflows.
        pub fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
            self.take(n)
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
    pub(crate) fn write_digits(w: &mut Writer, n: &Nat) {
        w.str(&n.to_string());
    }

    /// Read minimal decimal digits: no leading zero unless the number is zero.
    pub(crate) fn read_digits(r: &mut Reader<'_>) -> Option<Nat> {
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

    pub(crate) fn read_bool(r: &mut Reader<'_>) -> Option<bool> {
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
    pub(crate) fn complete_layout(t: &Temporal) -> bool {
        let offset_ok = match t.offset {
            Offset::Local | Offset::Zulu(ZuluCase::Upper | ZuluCase::Lower) => true,
            Offset::Fixed(_, form) | Offset::NegZero(form) => form == OffsetForm::Extended,
            Offset::Zulu(ZuluCase::Unwritten) | Offset::FixedSeconds(_) => false,
        };
        let date_ok = t.date.is_none_or(|d| {
            d.month.is_some()
                && d.day.is_some()
                && d.year.is_some_and(|y| {
                    !y.negative_zero && y.width == y.value.unsigned_abs().to_string().len().max(4)
                })
        });
        let date_only_local = !(t.date.is_some() && t.time.is_none()) || t.offset == Offset::Local;
        t.infinity.is_none() && t.is_complete() && offset_ok && date_ok && date_only_local
    }

    /// The canonical bytes of every temporal form.
    ///
    /// A form byte (0 date, 1 time, 2 date-time); a date as a little-endian `i64` year and a month and
    /// a day byte; a date-time's separator byte (`T`, `t` or a space, as ASCII) between date and time;
    /// a time as hour, minute and second bytes and its fraction digits behind a length; the offset as 0
    /// local, 1 `Z` followed by its case byte (`Z` or `z` as ASCII), 2 fixed followed by its minutes as
    /// an `i64`, 3 negative zero. Those historical layouts remain byte-for-byte
    /// unchanged. New form tags encode reduced/truncated fields, numeric-offset
    /// spelling, second offsets, unwritten offsets and temporal infinities.
    #[must_use]
    pub fn write_temporal(w: &mut Writer, t: &Temporal) -> Option<()> {
        if !t.well_formed() {
            return None;
        }
        if complete_layout(t) {
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
                write_separator(w, s);
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
                    Offset::FixedSeconds(_) => return None,
                }
            }
            return Some(());
        }

        if let Some((infinity, form)) = t.infinity {
            w.tag(match form {
                TemporalForm::Date => 6,
                TemporalForm::DateTime => 7,
                TemporalForm::Time => return None,
            });
            w.tag(match infinity {
                TemporalInfinity::Negative => 0,
                TemporalInfinity::Positive => 1,
            });
            write_offset_form(w, t.offset);
            return Some(());
        }
        w.tag(match t.form()? {
            TemporalForm::Date => 3,
            TemporalForm::Time => 4,
            TemporalForm::DateTime => 5,
        });
        if let Some(date) = t.date {
            write_date_form(w, date);
        }
        if let Some(separator) = t.separator {
            write_separator(w, separator);
        }
        if let Some(time) = &t.time {
            write_time_form(w, time);
        }
        write_offset_form(w, t.offset);
        Some(())
    }

    fn write_separator(w: &mut Writer, separator: Separator) {
        w.tag(match separator {
            Separator::Upper => b'T',
            Separator::Lower => b't',
            Separator::Space => b' ',
        });
    }

    fn write_option_u8(w: &mut Writer, value: Option<u8>) {
        match value {
            None => w.tag(0),
            Some(value) => {
                w.tag(1);
                w.tag(value);
            }
        }
    }

    fn write_date_form(w: &mut Writer, date: DateFields) {
        match date.year {
            None => w.tag(0),
            Some(year) => {
                w.tag(1);
                w.i64(year.value);
                w.tag(u8::from(year.negative_zero));
                w.u64(u64::try_from(year.width).unwrap_or(u64::MAX));
            }
        }
        write_option_u8(w, date.month);
        write_option_u8(w, date.day);
    }

    fn write_time_form(w: &mut Writer, time: &TimeFields) {
        write_option_u8(w, time.hour);
        write_option_u8(w, time.minute);
        write_option_u8(w, time.second);
        w.str(&time.fraction);
    }

    const fn offset_form_tag(form: OffsetForm) -> u8 {
        match form {
            OffsetForm::Extended => 0,
            OffsetForm::Basic => 1,
            OffsetForm::HourOnly => 2,
            OffsetForm::ExtendedSeconds => 3,
            OffsetForm::Unwritten => 4,
        }
    }

    fn write_offset_form(w: &mut Writer, offset: Offset) {
        match offset {
            Offset::Local => w.tag(0),
            Offset::Zulu(case) => {
                w.tag(1);
                w.tag(match case {
                    ZuluCase::Upper => 0,
                    ZuluCase::Lower => 1,
                    ZuluCase::Unwritten => 2,
                });
            }
            Offset::Fixed(minutes, form) => {
                w.tag(2);
                w.i64(i64::from(minutes));
                w.tag(offset_form_tag(form));
            }
            Offset::NegZero(form) => {
                w.tag(3);
                w.tag(offset_form_tag(form));
            }
            Offset::FixedSeconds(seconds) => {
                w.tag(4);
                w.i64(i64::from(seconds));
            }
        }
    }

    pub(crate) fn read_date(r: &mut Reader<'_>) -> Option<DateFields> {
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

    pub(crate) fn read_time(r: &mut Reader<'_>) -> Option<TimeFields> {
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

    pub(crate) fn read_offset(r: &mut Reader<'_>) -> Option<Offset> {
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

    #[derive(Clone, Copy)]
    enum OptionalU8 {
        Absent,
        Present(u8),
    }

    fn read_option_u8(r: &mut Reader<'_>) -> Option<OptionalU8> {
        match r.tag()? {
            0 => Some(OptionalU8::Absent),
            1 => Some(OptionalU8::Present(r.tag()?)),
            _ => None,
        }
    }

    const fn optional_u8(value: OptionalU8) -> Option<u8> {
        match value {
            OptionalU8::Absent => None,
            OptionalU8::Present(value) => Some(value),
        }
    }

    fn read_date_form(r: &mut Reader<'_>) -> Option<DateFields> {
        let year = match r.tag()? {
            0 => None,
            1 => Some(Year {
                value: r.i64()?,
                negative_zero: read_bool(r)?,
                width: usize::try_from(r.u64()?).ok()?,
            }),
            _ => return None,
        };
        Some(DateFields {
            year,
            month: optional_u8(read_option_u8(r)?),
            day: optional_u8(read_option_u8(r)?),
        })
    }

    fn read_time_form(r: &mut Reader<'_>) -> Option<TimeFields> {
        Some(TimeFields {
            hour: optional_u8(read_option_u8(r)?),
            minute: optional_u8(read_option_u8(r)?),
            second: optional_u8(read_option_u8(r)?),
            fraction: r.str()?,
        })
    }

    fn read_offset_form_tag(r: &mut Reader<'_>) -> Option<OffsetForm> {
        match r.tag()? {
            0 => Some(OffsetForm::Extended),
            1 => Some(OffsetForm::Basic),
            2 => Some(OffsetForm::HourOnly),
            3 => Some(OffsetForm::ExtendedSeconds),
            4 => Some(OffsetForm::Unwritten),
            _ => None,
        }
    }

    fn read_offset_form(r: &mut Reader<'_>) -> Option<Offset> {
        match r.tag()? {
            0 => Some(Offset::Local),
            1 => Some(Offset::Zulu(match r.tag()? {
                0 => ZuluCase::Upper,
                1 => ZuluCase::Lower,
                2 => ZuluCase::Unwritten,
                _ => return None,
            })),
            2 => Some(Offset::Fixed(
                i16::try_from(r.i64()?).ok()?,
                read_offset_form_tag(r)?,
            )),
            3 => Some(Offset::NegZero(read_offset_form_tag(r)?)),
            4 => Some(Offset::FixedSeconds(i32::try_from(r.i64()?).ok()?)),
            _ => None,
        }
    }

    fn read_separator(r: &mut Reader<'_>) -> Option<Separator> {
        match r.tag()? {
            b'T' => Some(Separator::Upper),
            b't' => Some(Separator::Lower),
            b' ' => Some(Separator::Space),
            _ => None,
        }
    }

    /// The inverse of [`write_temporal`].
    pub fn read_temporal(r: &mut Reader<'_>) -> Option<Temporal> {
        match r.tag()? {
            0 => {
                let value = Temporal {
                    date: Some(read_date(r)?),
                    time: None,
                    separator: None,
                    offset: Offset::Local,
                    infinity: None,
                };
                value.well_formed().then_some(value)
            }
            1 => {
                let time = read_time(r)?;
                let value = Temporal {
                    date: None,
                    time: Some(time),
                    separator: None,
                    offset: read_offset(r)?,
                    infinity: None,
                };
                value.well_formed().then_some(value)
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
                let value = Temporal {
                    date: Some(date),
                    time: Some(time),
                    separator: Some(separator),
                    offset: read_offset(r)?,
                    infinity: None,
                };
                value.well_formed().then_some(value)
            }
            tag @ (3..=5) => {
                let form = match tag {
                    3 => TemporalForm::Date,
                    4 => TemporalForm::Time,
                    _ => TemporalForm::DateTime,
                };
                let date = if matches!(form, TemporalForm::Date | TemporalForm::DateTime) {
                    Some(read_date_form(r)?)
                } else {
                    None
                };
                let separator = if form == TemporalForm::DateTime {
                    Some(read_separator(r)?)
                } else {
                    None
                };
                let time = if matches!(form, TemporalForm::Time | TemporalForm::DateTime) {
                    Some(read_time_form(r)?)
                } else {
                    None
                };
                let value = Temporal {
                    date,
                    time,
                    separator,
                    offset: read_offset_form(r)?,
                    infinity: None,
                };
                value.well_formed().then_some(value)
            }
            tag @ (6 | 7) => {
                let infinity = match r.tag()? {
                    0 => TemporalInfinity::Negative,
                    1 => TemporalInfinity::Positive,
                    _ => return None,
                };
                let form = if tag == 6 {
                    TemporalForm::Date
                } else {
                    TemporalForm::DateTime
                };
                let value = Temporal::infinite(infinity, form, read_offset_form(r)?)?;
                value.well_formed().then_some(value)
            }
            _ => None,
        }
    }

    pub(crate) fn write_map(w: &mut Writer, m: &BTreeMap<String, Value>) -> Option<()> {
        w.len(m.len());
        for (k, v) in m {
            w.str(k);
            write_value(w, v)?;
        }
        Some(())
    }

    pub(crate) fn read_map(r: &mut Reader<'_>) -> Option<BTreeMap<String, Value>> {
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
    /// key order. `None` only when a public temporal carrier is ill formed.
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

    /// Definition: Decode one value, refusing trailing bytes.
    ///
    /// Book anchors: `L-values-13`.
    #[must_use]
    pub fn decode_value(bytes: &[u8]) -> Option<Value> {
        let mut r = Reader::new(bytes);
        let v = read_value(&mut r)?;
        r.done().then_some(v)
    }

    /// Definition: Canonical bytes of an exact decimal, including all special values.
    ///
    /// Book anchors: `L-values-38`.
    #[must_use]
    pub fn decimal_bytes(value: &Decimal) -> Option<Vec<u8>> {
        value_bytes(&Value::Decimal(value.clone()))
    }
}

pub mod bytes {
    //! Exact byte/text reference mappings.
    //!
    //! See the byte-text laws on `correctness/values.md`.

    /// Why text cannot denote bytes in the selected form.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ByteTextError {
        /// A character lies above U+00FF.
        NonLatin1,
        /// Required base64 padding is absent or malformed.
        InvalidPadding,
        /// A character is outside the selected base64 alphabet or has
        /// non-zero unused bits.
        InvalidBase64,
    }

    /// Definition: Map each byte `b` to U+0000 + `b`.
    ///
    /// Book anchors: `L-values-24`, `L-values-41`.
    #[must_use]
    pub fn latin1_decode(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| char::from(*byte)).collect()
    }

    /// Definition: Map each code point U+0000 through U+00FF to its byte.
    ///
    /// # Errors
    ///
    /// [`ByteTextError::NonLatin1`] when any code point is above U+00FF.
    ///
    /// Book anchors: `L-values-24`, `L-values-41`.
    pub fn latin1_encode(text: &str) -> Result<Vec<u8>, ByteTextError> {
        text.chars()
            .map(|character| {
                u8::try_from(u32::from(character)).map_err(|_| ByteTextError::NonLatin1)
            })
            .collect()
    }

    /// Definition: The Latin-1 decode-then-encode law.
    ///
    /// Book anchors: `L-values-24`.
    #[must_use]
    pub fn latin1_decode_encode(bytes: &[u8]) -> bool {
        latin1_encode(&latin1_decode(bytes)).as_deref() == Ok(bytes)
    }

    /// Definition: The Latin-1 encode-then-decode law on its exact domain.
    ///
    /// Book anchors: `L-values-41`.
    #[must_use]
    pub fn latin1_encode_decode(text: &str) -> bool {
        latin1_encode(text).is_ok_and(|bytes| latin1_decode(&bytes) == text)
    }

    /// Definition: Normalize optional base64 padding to the unique padded length.
    ///
    /// This operation checks only the padding shape. Alphabet and quantum
    /// decoding remain the caller's format-specific operation.
    ///
    /// # Errors
    ///
    /// [`ByteTextError::InvalidPadding`] when padding is misplaced, has the
    /// wrong length, or cannot complete one four-character quantum.
    ///
    /// Book anchors: `L-values-26`, `L-values-27`.
    pub fn optional(text: &str) -> Result<String, ByteTextError> {
        let (body, padding) = text
            .find('=')
            .map_or((text, ""), |at| (&text[..at], &text[at..]));
        if !padding.bytes().all(|byte| byte == b'=') || padding.len() > 2 {
            return Err(ByteTextError::InvalidPadding);
        }
        if !body
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
        {
            return Err(ByteTextError::InvalidBase64);
        }
        let needed = (4 - body.len() % 4) % 4;
        if needed > 2 || (!padding.is_empty() && padding.len() != needed) {
            return Err(ByteTextError::InvalidPadding);
        }
        Ok(format!("{body}{}", "=".repeat(needed)))
    }

    const fn sextet(byte: u8) -> Option<u8> {
        Some(match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    }

    /// Definition: Encode bytes with the standard base64 alphabet and required padding.
    ///
    /// Book anchors: `L-values-25`, `L-values-42`.
    #[must_use]
    pub fn encode_base64(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut output = String::new();
        for chunk in bytes.chunks(3) {
            let word = u32::from(chunk[0]) << 16
                | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
                | u32::from(*chunk.get(2).unwrap_or(&0));
            output.push(char::from(ALPHABET[((word >> 18) & 63) as usize]));
            output.push(char::from(ALPHABET[((word >> 12) & 63) as usize]));
            if chunk.len() > 1 {
                output.push(char::from(ALPHABET[((word >> 6) & 63) as usize]));
            } else {
                output.push('=');
            }
            if chunk.len() > 2 {
                output.push(char::from(ALPHABET[(word & 63) as usize]));
            } else {
                output.push('=');
            }
        }
        output
    }

    /// Definition: Decode standard base64, optionally requiring its canonical padding.
    ///
    /// # Errors
    ///
    /// [`ByteTextError::InvalidPadding`] or
    /// [`ByteTextError::InvalidBase64`] at the first malformed quantum.
    ///
    /// Book anchors: `L-values-25`, `L-values-42`.
    pub fn decode_base64(text: &str, require_padding: bool) -> Result<Vec<u8>, ByteTextError> {
        let normalized = optional(text)?;
        if require_padding && normalized != text {
            return Err(ByteTextError::InvalidPadding);
        }
        let mut output = Vec::new();
        for quantum in normalized.as_bytes().chunks_exact(4) {
            let padding = quantum
                .iter()
                .rev()
                .take_while(|byte| **byte == b'=')
                .count();
            let mut values = [0u8; 4];
            for (index, byte) in quantum.iter().enumerate() {
                if *byte != b'=' {
                    values[index] = sextet(*byte).ok_or(ByteTextError::InvalidBase64)?;
                }
            }
            if (padding == 2 && values[1] & 0x0f != 0) || (padding == 1 && values[2] & 0x03 != 0) {
                return Err(ByteTextError::InvalidBase64);
            }
            let word = u32::from(values[0]) << 18
                | u32::from(values[1]) << 12
                | u32::from(values[2]) << 6
                | u32::from(values[3]);
            output.push(((word >> 16) & 0xff) as u8);
            if padding < 2 {
                output.push(((word >> 8) & 0xff) as u8);
            }
            if padding == 0 {
                output.push((word & 0xff) as u8);
            }
        }
        Ok(output)
    }

    /// Definition: The padded accepted-text encode/decode law: an accepted padded spelling
    /// is already the optional-padding normal form.
    ///
    /// Book anchors: `L-values-25`.
    #[must_use]
    pub fn padded_text_encode_decode(text: &str) -> bool {
        decode_base64(text, true).is_ok_and(|bytes| encode_base64(&bytes) == text)
    }

    /// Definition: The padded byte decode/encode law.
    ///
    /// Book anchors: `L-values-42`.
    #[must_use]
    pub fn padded_bytes_decode_encode(bytes: &[u8]) -> bool {
        decode_base64(&encode_base64(bytes), true).as_deref() == Ok(bytes)
    }
}

pub mod coerce {
    //! Schema-kind-driven coercion of lexemes and of values.
    //!
    //! [`coerce`] reads a lexeme under a declared scalar kind; [`coerce_value`]
    //! coerces an existing value by reading its lexeme the same way, with the
    //! narrowing rules for float kinds. Both are written straight from their
    //! definitions, with exact rationals for every float decision.
    //!
    //! Domain: the default numeral reading (no numeral dialect declared).
    //! Tagged lexemes are read with or without a discriminator table. A float
    //! width other than 16, 32 and 64, for which no rule is stated, gives
    //! `None`, as does the nearest-keeping-lexeme reading of an existing value.

    use crate::exact::{
        Decimal, DecimalValue, ExactInteger, NonFinite, Scaled, integer_of_numeral, parse_decimal,
        parse_non_finite, parse_numeral,
    };
    use crate::float::{
        CANONICAL_NAN_BITS, Dec, FloatValue, Format, NEGATIVE_CANONICAL_NAN_BITS, Rounded, decode,
        holds_exactly, narrow_bits, round_to, shortest, widen_bits,
    };
    use crate::num::{Int, Nat, Rat};
    use crate::temporal::{self, Offset, TemporalKind};
    use crate::value::Value;

    /// How bytes are written as text.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum BytesText {
        /// Standard base64, padding optional on read.
        Base64,
        /// URL-safe base64, padding optional on read.
        Base64Url,
        /// Lower- or upper-case hexadecimal, two digits per byte.
        Hex,
        /// The UTF-8 bytes of the text.
        Raw,
        /// Standard base64 with padding required.
        Base64Padded,
        /// URL-safe base64 with padding required.
        Base64UrlPadded,
        /// One code point U+0000 to U+00FF per byte.
        Latin1,
    }

    /// A declared scalar kind.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum ScalarKind {
        /// Booleans.
        Bool,
        /// Integers of a width and signedness.
        Int {
            /// Width in bits.
            bits: u8,
            /// Whether negative values are admitted.
            signed: bool,
        },
        /// Integers of any size.
        BigInt,
        /// Binary floats of a width.
        Float {
            /// Width in bits (16, 32 or 64).
            bits: u8,
        },
        /// Exact decimals.
        Decimal,
        /// Strings.
        Str,
        /// Bytes written as text.
        Bytes(BytesText),
        /// Dates.
        Date,
        /// Times.
        Time,
        /// Date-times.
        DateTime,
        /// Tokens.
        Token,
        /// No declared kind (open schema).
        Any,
    }

    /// How a float column reads a finite numeral.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
    pub enum FloatReading {
        /// Law (i): accept iff exact or the canonical text of the rounded value.
        #[default]
        Exact,
        /// Read every finite numeral as its nearest value, keeping the lexeme.
        NearestKeepingLexeme,
    }

    /// The numeral grammar declared by the route that supplied a lexeme.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum NumeralDialect {
        /// RFC 8259 JSON numbers.
        Json,
        /// TOML integers, floats and special floats.
        Toml,
        /// YAML 1.2 core-schema numerals.
        Yaml12,
        /// YAML 1.1 integer and float numerals, including sexagesimal integers.
        Yaml11,
        /// XML Schema decimal and double lexical forms.
        Xsd,
        /// Java decimal floats and its case-sensitive special values.
        Java,
        /// `ProtoJSON` numbers and quoted special floats.
        ProtoJson,
        /// PostgreSQL-style SQL numerals and special floats.
        Sql,
    }

    /// A kind with its value parameters.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct ValueSpec {
        /// The kind.
        pub kind: ScalarKind,
        /// Decimal precision.
        pub precision: Option<u32>,
        /// Decimal scale.
        pub scale: Option<u32>,
        /// Whether an offset is required (`Some(true)`) or forbidden.
        pub with_time_zone: Option<bool>,
        /// Integer width overriding the kind's.
        pub int_bits: Option<u8>,
        /// Whether the integer kind is unsigned.
        pub unsigned: bool,
        /// The float reading.
        pub float_reading: FloatReading,
        /// The route's numeral grammar, or the legacy grammar when absent.
        pub numeral: Option<NumeralDialect>,
    }

    impl ValueSpec {
        /// The spec of `kind` with no parameters.
        #[must_use]
        pub const fn of(kind: ScalarKind) -> Self {
            Self {
                kind,
                precision: None,
                scale: None,
                with_time_zone: None,
                int_bits: None,
                unsigned: false,
                float_reading: FloatReading::Exact,
                numeral: None,
            }
        }
    }

    /// The lexical class a format gave a scalar.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub enum LexClass {
        /// A numeral (with the NaN and infinity spellings).
        Number,
        /// A string.
        String,
        /// A boolean literal.
        Bool,
        /// A null literal.
        Null,
        /// A temporal literal.
        DateTime,
        /// Bytes written as text.
        Bytes,
        /// A scalar with a type tag.
        Tagged(String),
    }

    /// Why a lexeme or value is refused.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum FidelityReason {
        /// Out of the kind's range.
        Overflow,
        /// Not held exactly; `max` is the kind's significant-digit bound.
        PrecisionExceeded {
            /// The bound.
            max: u32,
        },
        /// Too many fractional digits.
        ScaleExceeded {
            /// The bound.
            max: u32,
        },
        /// Not a numeral.
        NotANumber,
        /// A NaN or infinity the kind cannot hold.
        NonFinite,
        /// Not a temporal of the kind.
        InvalidTemporal,
        /// An offset is required.
        OffsetRequired,
        /// An offset is forbidden.
        OffsetForbidden,
        /// Not in the kind's encoding.
        InvalidEncoding,
        /// A numeral under a non-numeric kind.
        NotANumberForKind,
    }

    /// The outcome of a reference coercion: `None` outside the reference's
    /// domain, else the value or the typed refusal.
    pub type Coerced = Option<Result<Value, FidelityReason>>;

    /// The bound on decimal positions inside which values are computed exactly:
    /// every format overflows at `10^400` and rounds every non-zero magnitude
    /// below `10^-400` to zero.
    const BOUND: i64 = 400;

    const fn nan_bits(negative: bool) -> u64 {
        if negative {
            NEGATIVE_CANONICAL_NAN_BITS
        } else {
            CANONICAL_NAN_BITS
        }
    }

    const fn inf_bits(negative: bool) -> u64 {
        Rounded::Overflow { negative }.bits(Format::B64)
    }

    /// A type tag without a YAML `tag:yaml.org,2002:` or `!!` prefix.
    fn tag_name(tag: &str) -> &str {
        tag.strip_prefix("tag:yaml.org,2002:")
            .or_else(|| tag.strip_prefix("!!"))
            .unwrap_or(tag)
    }

    /// The scalar kind a tag names, and the lexical class its lexeme is read
    /// in: the YAML core and extended-JSON names of the scalar kinds.
    fn tagged_kind(name: &str) -> Option<(ScalarKind, LexClass)> {
        Some(match name {
            "bool" | "boolean" => (ScalarKind::Bool, LexClass::Bool),
            "int" | "integer" | "numberLong" | "numberInt" => {
                (ScalarKind::BigInt, LexClass::Number)
            }
            "float" | "double" | "numberDouble" => {
                (ScalarKind::Float { bits: 64 }, LexClass::Number)
            }
            "decimal" | "numberDecimal" => (ScalarKind::Decimal, LexClass::Number),
            "str" | "string" => (ScalarKind::Str, LexClass::String),
            "binary" | "base64" => (ScalarKind::Bytes(BytesText::Base64), LexClass::Bytes),
            "hex" => (ScalarKind::Bytes(BytesText::Hex), LexClass::Bytes),
            "timestamp" | "datetime" | "date-time" => (ScalarKind::DateTime, LexClass::DateTime),
            "date" => (ScalarKind::Date, LexClass::DateTime),
            "time" => (ScalarKind::Time, LexClass::DateTime),
            _ => return None,
        })
    }

    /// The opaque value that keeps a scalar of an unknown tag: its type is the
    /// tag as written and its one field `value` the lexeme.
    fn opaque_of(tag: &str, text: &str) -> Value {
        Value::Opaque {
            type_: tag.to_owned(),
            fields: std::iter::once(("value".to_owned(), Value::Str(text.to_owned()))).collect(),
        }
    }

    /// A tagged lexeme with no discriminator table.
    ///
    /// Definition: "A tagged scalar: the declared kind governs when there is
    /// one; under `Any` the tag's own kind does, and a tag naming no scalar
    /// kind keeps the scalar as an opaque value of that type, so the tag is
    /// never dropped" (the reading every call without a discriminator table
    /// keeps). Under a declared kind the lexeme is read as a `String`; under
    /// `Any` the tag, with its YAML prefix removed, is `null` (the null value),
    /// a scalar kind's name (the lexeme read under that kind in its class), or
    /// any other name (the opaque value of the tag as written).
    fn coerce_tagged_open(text: &str, tag: &str, spec: &ValueSpec) -> Coerced {
        if spec.kind != ScalarKind::Any {
            return coerce(text, &LexClass::String, spec);
        }
        let name = tag_name(tag);
        if name == "null" {
            return Some(Ok(Value::Null));
        }
        match tagged_kind(name) {
            Some((kind, class)) => coerce(text, &class, &ValueSpec::of(kind)),
            None => Some(Ok(opaque_of(tag, text))),
        }
    }

    /// A lexeme with a tag under a spec with a discriminator table.
    ///
    /// Definition: "Under a spec with a table, a lexeme whose tag the table
    /// holds is read by `coerce` under the spec with that kind, so a JSON
    /// number tagged "text" is refused under `Str` as an untagged one is; a
    /// lexeme whose tag the table lacks is kept unchanged as `Value::Opaque {
    /// type_: tag, fields: { "value": Str(lexeme) } }` ... a lexeme with no tag
    /// is read under the spec's own kind." The lexeme keeps its own class.
    #[must_use]
    pub fn coerce_discriminated(
        text: &str,
        class: &LexClass,
        tag: Option<&str>,
        table: &std::collections::BTreeMap<String, ScalarKind>,
        spec: &ValueSpec,
    ) -> Coerced {
        tag.map_or_else(
            || coerce(text, class, spec),
            |tag| {
                table.get(tag).map_or_else(
                    || Some(Ok(opaque_of(tag, text))),
                    |kind| {
                        coerce(
                            text,
                            class,
                            &ValueSpec {
                                kind: *kind,
                                ..*spec
                            },
                        )
                    },
                )
            },
        )
    }

    /// Definition: Read `lexeme` of class `class` under `spec`: the value-kind coercion, with exact numerals and canonical shortest numerals only.
    /// Under `Any`, a temporal lexeme with a calendar-date reading takes that
    /// reading; reduced-form offset readings are considered only when no date
    /// reading exists. A declared temporal kind selects its own reading.
    ///
    /// A lexeme of class `Tagged(tag)` with no discriminator table is read by
    /// the declared kind as a `String` when there is one, and under `Any` by
    /// its tag: `null` (with the YAML prefix removed) is the null value, a
    /// scalar kind's name reads the lexeme under that kind, and any other tag
    /// keeps it as the opaque value of the tag. A lexeme with a tag under a
    /// discriminator table goes through [`coerce_discriminated`].
    ///
    /// Book anchors: `L-values-2`, `L-values-4`, `L-values-6`.
    #[must_use]
    pub fn coerce(text: &str, class: &LexClass, spec: &ValueSpec) -> Coerced {
        if *class == LexClass::Null {
            return Some(Ok(Value::Null));
        }
        if let LexClass::Tagged(tag) = class {
            return coerce_tagged_open(text, tag, spec);
        }
        let numeric_kind = matches!(
            spec.kind,
            ScalarKind::Int { .. }
                | ScalarKind::BigInt
                | ScalarKind::Float { .. }
                | ScalarKind::Decimal
        );
        let dialect_applies =
            *class == LexClass::Number || (*class == LexClass::String && numeric_kind);
        if let Some(dialect) = spec.numeral
            && dialect_applies
        {
            let Some(normalized) = normalize_numeral(text, class, spec.kind, dialect) else {
                return Some(Err(FidelityReason::NotANumber));
            };
            return coerce(
                &normalized,
                class,
                &ValueSpec {
                    numeral: None,
                    ..*spec
                },
            );
        }
        let numeric_class = || match class {
            LexClass::Bool | LexClass::Bytes | LexClass::DateTime => {
                Err(FidelityReason::NotANumber)
            }
            _ => Ok(()),
        };
        Some(match spec.kind {
            ScalarKind::Any => coerce_any(text, class),
            ScalarKind::Bool => coerce_bool(text, class),
            ScalarKind::Int { bits, signed } => {
                let bits = spec.int_bits.unwrap_or(bits);
                if bits == 0 {
                    return None;
                }
                numeric_class()
                    .and_then(|()| coerce_int(text, Some((bits, signed && !spec.unsigned))))
            }
            ScalarKind::BigInt => numeric_class().and_then(|()| coerce_int(text, None)),
            ScalarKind::Float { bits } => {
                let format = Format::of_width(bits)?;
                numeric_class().and_then(|()| coerce_float(text, format, spec.float_reading))
            }
            ScalarKind::Decimal => numeric_class().and_then(|()| coerce_decimal(text, spec)),
            ScalarKind::Str | ScalarKind::Token => match class {
                LexClass::Number => Err(FidelityReason::NotANumberForKind),
                LexClass::Bool => Err(FidelityReason::InvalidEncoding),
                _ if spec.kind == ScalarKind::Token => Ok(Value::Token(text.to_owned())),
                _ => Ok(Value::Str(text.to_owned())),
            },
            ScalarKind::Bytes(encoding) => match class {
                LexClass::Number => Err(FidelityReason::NotANumberForKind),
                LexClass::Bool => Err(FidelityReason::InvalidEncoding),
                _ => decode_bytes(text, encoding)
                    .map(Value::Bytes)
                    .ok_or(FidelityReason::InvalidEncoding),
            },
            ScalarKind::Date | ScalarKind::Time | ScalarKind::DateTime => match class {
                LexClass::Number => Err(FidelityReason::NotANumberForKind),
                LexClass::Bool => Err(FidelityReason::InvalidTemporal),
                _ => coerce_temporal(text, spec),
            },
        })
    }

    /// Definition: Read a lexeme under the numeral dialect carried by `spec`.
    ///
    /// This named entry point is used by the schema-v2 manifest; [`coerce`]
    /// itself performs the dispatch so adapters cannot accidentally bypass it.
    ///
    /// Book anchors: `L-values-23`.
    #[must_use]
    pub fn coerce_dialect(text: &str, class: &LexClass, spec: &ValueSpec) -> Coerced {
        coerce(text, class, spec)
    }

    fn normalize_numeral(
        text: &str,
        class: &LexClass,
        kind: ScalarKind,
        dialect: NumeralDialect,
    ) -> Option<String> {
        let float_kind = matches!(kind, ScalarKind::Float { .. });
        match dialect {
            NumeralDialect::Json => json_number(text).then(|| text.to_owned()),
            NumeralDialect::Toml => normalize_toml(text),
            NumeralDialect::Yaml12 => normalize_yaml12(text),
            NumeralDialect::Yaml11 => normalize_yaml11(text),
            NumeralDialect::Xsd => normalize_xsd(text, float_kind),
            NumeralDialect::Java => normalize_java(text, float_kind),
            NumeralDialect::ProtoJson => {
                if *class == LexClass::String && float_kind {
                    match text {
                        "NaN" | "Infinity" | "-Infinity" => Some(text.to_owned()),
                        _ => json_number(text).then(|| text.to_owned()),
                    }
                } else {
                    json_number(text).then(|| text.to_owned())
                }
            }
            NumeralDialect::Sql => {
                normalize_sql(text, float_kind || matches!(kind, ScalarKind::Decimal))
            }
        }
    }

    fn unsigned_decimal(text: &str, leading_zero: bool) -> bool {
        !text.is_empty()
            && text.bytes().all(|byte| byte.is_ascii_digit())
            && (leading_zero || text.len() == 1 || !text.starts_with('0'))
    }

    #[derive(Clone, Copy)]
    struct DecimalGrammar(u8);

    impl DecimalGrammar {
        const PLUS: u8 = 1;
        const LEADING_ZERO: u8 = 2;
        const DOT: u8 = 4;
        const EXPONENT: u8 = 8;

        const JSON: Self = Self(Self::DOT | Self::EXPONENT);
        const TOML: Self = Self(Self::PLUS | Self::DOT | Self::EXPONENT);
        const PERMISSIVE: Self = Self(Self::PLUS | Self::LEADING_ZERO | Self::DOT | Self::EXPONENT);
        const XSD_DECIMAL: Self = Self(Self::PLUS | Self::LEADING_ZERO | Self::DOT);

        const fn permits(self, feature: u8) -> bool {
            self.0 & feature != 0
        }
    }

    fn decimal_number(text: &str, grammar: DecimalGrammar) -> bool {
        let bytes = text.as_bytes();
        let mut at = if grammar.permits(DecimalGrammar::PLUS) && matches!(bytes.first(), Some(b'+'))
        {
            1
        } else {
            usize::from(matches!(bytes.first(), Some(b'-')))
        };
        if at == bytes.len()
            || (!grammar.permits(DecimalGrammar::PLUS) && matches!(bytes.first(), Some(b'+')))
        {
            return false;
        }
        let int_start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        let int = &text[int_start..at];
        let fraction_len = if grammar.permits(DecimalGrammar::DOT) && bytes.get(at) == Some(&b'.') {
            at += 1;
            let start = at;
            while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                at += 1;
            }
            at - start
        } else {
            0
        };
        if int.is_empty() && fraction_len == 0 {
            return false;
        }
        if !int.is_empty() && !unsigned_decimal(int, grammar.permits(DecimalGrammar::LEADING_ZERO))
        {
            return false;
        }
        if grammar.permits(DecimalGrammar::EXPONENT) && matches!(bytes.get(at), Some(b'e' | b'E')) {
            at += 1;
            if matches!(bytes.get(at), Some(b'+' | b'-')) {
                at += 1;
            }
            let start = at;
            while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                at += 1;
            }
            if start == at {
                return false;
            }
        }
        at == bytes.len()
    }

    fn json_number(text: &str) -> bool {
        let mantissa = text.split(['e', 'E']).next().unwrap_or(text);
        decimal_number(text, DecimalGrammar::JSON)
            && !mantissa.ends_with('.')
            && !mantissa.starts_with('.')
            && !mantissa.starts_with("-.")
    }

    fn strip_separators(text: &str, digit: impl Fn(u8) -> bool) -> Option<String> {
        let bytes = text.as_bytes();
        if bytes.first() == Some(&b'_') || bytes.last() == Some(&b'_') {
            return None;
        }
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'_'
                && (!index
                    .checked_sub(1)
                    .and_then(|i| bytes.get(i))
                    .is_some_and(|b| digit(*b))
                    || !bytes.get(index + 1).is_some_and(|b| digit(*b)))
            {
                return None;
            }
        }
        Some(text.replace('_', ""))
    }

    fn radix_decimal(digits: &str, radix: u8) -> Option<String> {
        let mut decimal = vec![0u8];
        for byte in digits.bytes() {
            let value = match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                b'A'..=b'F' => byte - b'A' + 10,
                _ => return None,
            };
            if value >= radix {
                return None;
            }
            let mut carry = u16::from(value);
            for digit in decimal.iter_mut().rev() {
                let next = u16::from(*digit) * u16::from(radix) + carry;
                *digit = u8::try_from(next % 10).ok()?;
                carry = next / 10;
            }
            while carry != 0 {
                decimal.insert(0, u8::try_from(carry % 10).ok()?);
                carry /= 10;
            }
        }
        if digits.is_empty() {
            return None;
        }
        Some(decimal.into_iter().map(|d| char::from(b'0' + d)).collect())
    }

    fn prefixed_integer(text: &str, separators: bool) -> Option<String> {
        let (prefix, radix) = if text.starts_with("0x") {
            ("0x", 16)
        } else if text.starts_with("0o") {
            ("0o", 8)
        } else if text.starts_with("0b") {
            ("0b", 2)
        } else {
            return None;
        };
        let digits = &text[prefix.len()..];
        let digits = if separators {
            strip_separators(digits, |b| b.is_ascii_hexdigit())?
        } else {
            digits.to_owned()
        };
        radix_decimal(&digits, radix)
    }

    fn normalize_toml(text: &str) -> Option<String> {
        if matches!(text, "inf" | "+inf" | "-inf" | "nan" | "+nan" | "-nan") {
            return Some(
                match text {
                    "inf" | "+inf" => "Infinity",
                    "-inf" => "-Infinity",
                    "nan" | "+nan" => "NaN",
                    "-nan" => "-NaN",
                    _ => unreachable!(),
                }
                .to_owned(),
            );
        }
        if !text.starts_with(['+', '-'])
            && let Some(value) = prefixed_integer(text, true)
        {
            return Some(value);
        }
        let clean = strip_separators(text, |byte| byte.is_ascii_digit())?;
        let unsigned = clean.strip_prefix(['+', '-']).unwrap_or(&clean);
        let mantissa = unsigned.split(['e', 'E']).next().unwrap_or(unsigned);
        (decimal_number(&clean, DecimalGrammar::TOML)
            && !mantissa.starts_with('.')
            && !mantissa.ends_with('.'))
        .then_some(clean)
    }

    /// The YAML special floats: `.inf`, `.Inf`, `.INF` after an optional
    /// sign; `.nan`, `.NaN`, `.NAN` unsigned or, by the recorded departure,
    /// after a minus sign.
    fn yaml_special(text: &str) -> Option<&'static str> {
        let (sign, body) = match text.as_bytes().first() {
            Some(b'-') => ('-', &text[1..]),
            Some(b'+') => ('+', &text[1..]),
            _ => (' ', text),
        };
        let infinity = matches!(body, ".inf" | ".Inf" | ".INF");
        let nan = matches!(body, ".nan" | ".NaN" | ".NAN");
        match sign {
            '-' if infinity => Some("-Infinity"),
            '-' if nan => Some("-NaN"),
            '+' | ' ' if infinity => Some("Infinity"),
            ' ' if nan => Some("NaN"),
            _ => None,
        }
    }

    /// YAML 1.2.2 Sec 10.3.2 core schema: `[-+]? [0-9]+`, `0o [0-7]+`,
    /// `0x [0-9a-fA-F]+`, the decimal float form and the special floats.
    fn normalize_yaml12(text: &str) -> Option<String> {
        if let Some(special) = yaml_special(text) {
            return Some(special.to_owned());
        }
        if let Some(digits) = text.strip_prefix("0o") {
            return radix_decimal(digits, 8);
        }
        if let Some(digits) = text.strip_prefix("0x") {
            return radix_decimal(digits, 16);
        }
        decimal_number(text, DecimalGrammar::PERMISSIVE).then(|| text.to_owned())
    }

    /// Whether `s` is `first(_|digit)*` with `first` accepted by `lead` and
    /// every later character a digit or `_`.
    fn underscored(s: &str, lead: impl Fn(u8) -> bool) -> bool {
        s.as_bytes().first().is_some_and(|b| lead(*b))
            && s.bytes().all(|b| b.is_ascii_digit() || b == b'_')
    }

    /// `[1-9][0-9_]*(:[0-5]?[0-9])+` (or `[0-9]` first when `zero_lead`):
    /// the base 60 value of its fields as decimal digits.
    fn base60(s: &str, zero_lead: bool) -> Option<String> {
        let mut parts = s.split(':');
        let first = parts.next()?;
        let lead = |b: u8| {
            if zero_lead {
                b.is_ascii_digit()
            } else {
                (b'1'..=b'9').contains(&b)
            }
        };
        if !underscored(first, lead) {
            return None;
        }
        // Most significant decimal digit first.
        let mut value: Vec<u32> = first
            .bytes()
            .filter(u8::is_ascii_digit)
            .map(|b| u32::from(b - b'0'))
            .collect();
        let mut fields = 0;
        for part in parts {
            let short = matches!(part.len(), 1 | 2) && part.bytes().all(|b| b.is_ascii_digit());
            let field: u32 = part.parse().ok().filter(|f| short && *f < 60)?;
            let mut carry = field;
            for digit in value.iter_mut().rev() {
                let next = *digit * 60 + carry;
                *digit = next % 10;
                carry = next / 10;
            }
            while carry > 0 {
                value.insert(0, carry % 10);
                carry /= 10;
            }
            fields += 1;
        }
        let text: String = value
            .iter()
            .map(|d| char::from(b'0' + u8::try_from(*d).unwrap_or(0)))
            .collect();
        let trimmed = text.trim_start_matches('0');
        (fields > 0).then(|| if trimmed.is_empty() { "0" } else { trimmed }.to_owned())
    }

    /// YAML 1.1 `int` and `float` type pages, with every `_` ignored: the
    /// integers `[-+]?0b[0-1_]+`, `[-+]?0[0-7_]+`, `[-+]?(0|[1-9][0-9_]*)`,
    /// `[-+]?0x[0-9a-fA-F_]+`, `[-+]?[1-9][0-9_]*(:[0-5]?[0-9])+`; the floats
    /// `[-+]?([0-9][0-9_]*)?\.[0-9_]*([eE][-+][0-9]+)?` (with a digit) and
    /// `[-+]?[0-9][0-9_]*(:[0-5]?[0-9])+\.[0-9_]*`; and the special floats.
    fn normalize_yaml11(text: &str) -> Option<String> {
        if let Some(special) = yaml_special(text) {
            return Some(special.to_owned());
        }
        let (sign, body) = match text.as_bytes().first() {
            Some(b'-') => ("-", &text[1..]),
            Some(b'+') => ("", &text[1..]),
            _ => ("", text),
        };
        let with_sign = |v: String| format!("{sign}{v}");
        let prefixed = |rest: &str, radix: u8| {
            let digit = |b: u8| char::from(b).is_digit(u32::from(radix));
            (rest.bytes().all(|b| digit(b) || b == b'_') && rest.bytes().any(digit))
                .then(|| radix_decimal(&rest.replace('_', ""), radix))
                .flatten()
        };
        if let Some(rest) = body.strip_prefix("0b") {
            return prefixed(rest, 2).map(with_sign);
        }
        if let Some(rest) = body.strip_prefix("0x") {
            return prefixed(rest, 16).map(with_sign);
        }
        if body.contains(':') {
            return match body.split_once('.') {
                None => base60(body, false).map(with_sign),
                Some((whole, fraction)) => {
                    let whole = base60(whole, true)?;
                    fraction
                        .bytes()
                        .all(|b| b.is_ascii_digit() || b == b'_')
                        .then(|| with_sign(format!("{whole}.{}", fraction.replace('_', ""))))
                }
            };
        }
        if let Some(rest) = body.strip_prefix('0')
            && !rest.is_empty()
            && rest
                .bytes()
                .all(|b| (b'0'..=b'7').contains(&b) || b == b'_')
        {
            let digits = rest.replace('_', "");
            return if digits.is_empty() {
                Some(with_sign("0".to_owned()))
            } else {
                radix_decimal(&digits, 8).map(with_sign)
            };
        }
        if body == "0" || underscored(body, |b| (b'1'..=b'9').contains(&b)) {
            return Some(with_sign(body.replace('_', "")));
        }
        let (mantissa, exponent) = body
            .find(['e', 'E'])
            .map_or((body, None), |at| (&body[..at], Some(&body[at + 1..])));
        let (whole, fraction) = mantissa.split_once('.')?;
        let exponent_ok = exponent.is_none_or(|e| {
            matches!(e.as_bytes().first(), Some(b'+' | b'-'))
                && e.len() > 1
                && e[1..].bytes().all(|b| b.is_ascii_digit())
        });
        let ok = (whole.is_empty() || underscored(whole, |b| b.is_ascii_digit()))
            && fraction.bytes().all(|b| b.is_ascii_digit() || b == b'_')
            && mantissa.bytes().any(|b| b.is_ascii_digit())
            && exponent_ok;
        if !ok {
            return None;
        }
        let whole = whole.replace('_', "");
        let fraction = fraction.replace('_', "");
        Some(with_sign(format!(
            "{}.{}{}",
            if whole.is_empty() { "0" } else { &whole },
            if fraction.is_empty() { "0" } else { &fraction },
            exponent.map(|e| format!("e{e}")).unwrap_or_default()
        )))
    }

    fn normalize_xsd(text: &str, float_kind: bool) -> Option<String> {
        if float_kind {
            return match text {
                "INF" | "+INF" => Some("Infinity".to_owned()),
                "-INF" => Some("-Infinity".to_owned()),
                "NaN" => Some("NaN".to_owned()),
                "-NaN" => Some("-NaN".to_owned()),
                _ => decimal_number(text, DecimalGrammar::PERMISSIVE).then(|| text.to_owned()),
            };
        }
        (decimal_number(text, DecimalGrammar::XSD_DECIMAL) && !text.contains(['e', 'E']))
            .then(|| text.to_owned())
    }

    fn normalize_java(text: &str, float_kind: bool) -> Option<String> {
        if float_kind {
            match text {
                "NaN" | "Infinity" | "-Infinity" => return Some(text.to_owned()),
                _ => {}
            }
        }
        decimal_number(text, DecimalGrammar::PERMISSIVE).then(|| text.to_owned())
    }

    fn normalize_sql(text: &str, admits_specials: bool) -> Option<String> {
        if admits_specials {
            match text.to_ascii_lowercase().as_str() {
                "nan" => return Some("NaN".to_owned()),
                "-nan" => return Some("-NaN".to_owned()),
                "infinity" | "+infinity" | "inf" | "+inf" => return Some("Infinity".to_owned()),
                "-infinity" | "-inf" => return Some("-Infinity".to_owned()),
                _ => {}
            }
        }
        decimal_number(text, DecimalGrammar::PERMISSIVE).then(|| text.to_owned())
    }

    fn coerce_bool(text: &str, class: &LexClass) -> Result<Value, FidelityReason> {
        let read = match class {
            LexClass::Bool | LexClass::String => {
                if text.eq_ignore_ascii_case("true") {
                    Some(true)
                } else if text.eq_ignore_ascii_case("false") {
                    Some(false)
                } else {
                    None
                }
            }
            _ => None,
        };
        read.map(Value::Bool).ok_or(if *class == LexClass::Number {
            FidelityReason::NotANumberForKind
        } else {
            FidelityReason::InvalidEncoding
        })
    }

    fn coerce_any(text: &str, class: &LexClass) -> Result<Value, FidelityReason> {
        match class {
            LexClass::Number => {
                if let Some(nf) = parse_non_finite(text) {
                    return Ok(Value::Float(non_finite_bits(nf)));
                }
                let n = parse_numeral(text).ok_or(FidelityReason::NotANumber)?;
                if n.is_integer_form() {
                    return integer_of_numeral(&n)
                        .map(Value::integer)
                        .ok_or(FidelityReason::NotANumber);
                }
                let d = DecimalValue::of_numeral(&n);
                match nearest(Format::B64, &d) {
                    Rounded::Finite(bits) => {
                        let shortest_equal =
                            shortest(Format::B64, bits).is_some_and(|s| dec_equals(&s, &d));
                        Ok(if shortest_equal {
                            Value::Float(bits)
                        } else {
                            Value::Decimal(Decimal::Finite(d))
                        })
                    }
                    Rounded::Overflow { .. } => Ok(Value::Decimal(Decimal::Finite(d))),
                }
            }
            LexClass::Bool => coerce_bool(text, class).map_err(|_| FidelityReason::InvalidEncoding),
            LexClass::DateTime => temporal::parse(TemporalKind::Date, text)
                .or_else(|| temporal::parse_any(text))
                .map(Value::Temporal)
                .ok_or(FidelityReason::InvalidTemporal),
            LexClass::Bytes => decode_bytes(text, BytesText::Base64)
                .map(Value::Bytes)
                .ok_or(FidelityReason::InvalidEncoding),
            LexClass::String | LexClass::Null | LexClass::Tagged(_) => {
                Ok(Value::Str(text.to_owned()))
            }
        }
    }

    const fn non_finite_bits(nf: NonFinite) -> u64 {
        match nf {
            NonFinite::NaN { negative } => nan_bits(negative),
            NonFinite::Infinity { negative } => inf_bits(negative),
        }
    }

    /// `L_b` of a decimal (the sign of a zero kept), with magnitudes beyond the
    /// exact bound classified without expansion.
    fn nearest(format: Format, d: &DecimalValue) -> Rounded {
        match d.scaled(-BOUND, BOUND) {
            Scaled::Value(q) => round_to(format, &q, d.negative),
            Scaled::AtLeastHi => Rounded::Overflow {
                negative: d.negative,
            },
            Scaled::BelowLo => Rounded::Finite(if d.negative { sign_bit(format) } else { 0 }),
        }
    }

    const fn sign_bit(format: Format) -> u64 {
        1u64 << (format.width() - 1)
    }

    /// Whether a printed decimal equals a decimal numerically.
    fn dec_equals(s: &Dec, d: &DecimalValue) -> bool {
        let as_decimal = DecimalValue {
            negative: s.negative,
            digits: s.coefficient.clone(),
            exponent: Int::from_i64(s.exponent),
        };
        as_decimal.cmp_numeric(d).is_eq()
    }

    /// Whether `d` denotes the finite `format` value `bits` exactly.
    fn denotes_exactly(format: Format, d: &DecimalValue, bits: u64) -> bool {
        match (d.scaled(-BOUND, BOUND), decode(format, bits)) {
            (Scaled::Value(q), FloatValue::Finite { value, .. }) => q == value,
            _ => false,
        }
    }

    fn coerce_float(
        text: &str,
        format: Format,
        reading: FloatReading,
    ) -> Result<Value, FidelityReason> {
        if let Some(nf) = parse_non_finite(text) {
            return Ok(Value::Float(non_finite_bits(nf)));
        }
        let n = parse_numeral(text).ok_or(FidelityReason::NotANumber)?;
        let d = DecimalValue::of_numeral(&n);
        let rounded = nearest(format, &d);
        if reading == FloatReading::NearestKeepingLexeme {
            return Ok(Value::Float(widen_bits(format, rounded.bits(format))));
        }
        let Rounded::Finite(bits) = rounded else {
            return Err(FidelityReason::Overflow);
        };
        let canonical = shortest(format, bits).is_some_and(|s| dec_equals(&s, &d));
        if canonical || denotes_exactly(format, &d, bits) {
            Ok(Value::Float(widen_bits(format, bits)))
        } else {
            Err(FidelityReason::PrecisionExceeded {
                max: format.max_significant_digits(),
            })
        }
    }

    /// `-2^(bits-1)` and `2^(bits-1)` (signed) or `0` and `2^bits` (unsigned):
    /// the half-open range of an integer kind, as decimals.
    fn int_range(bits: u8, signed: bool) -> (DecimalValue, DecimalValue) {
        let as_dec = |negative: bool, n: Nat| DecimalValue {
            negative,
            digits: n,
            exponent: Int::zero(),
        };
        if signed {
            let half = Nat::pow2(u32::from(bits) - 1);
            (as_dec(true, half.clone()), as_dec(false, half))
        } else {
            (
                as_dec(false, Nat::zero()),
                as_dec(false, Nat::pow2(u32::from(bits))),
            )
        }
    }

    fn coerce_int(text: &str, width: Option<(u8, bool)>) -> Result<Value, FidelityReason> {
        if parse_non_finite(text).is_some() {
            return Err(FidelityReason::NonFinite);
        }
        let n = parse_numeral(text).ok_or(FidelityReason::NotANumber)?;
        let d = DecimalValue::of_numeral(&n);
        if !d.minimal_scale().is_zero() {
            return Err(FidelityReason::ScaleExceeded { max: 0 });
        }
        if let Some((bits, signed)) = width {
            let (lo, hi) = int_range(bits, signed);
            if d.cmp_numeric(&lo).is_lt() || d.cmp_numeric(&hi).is_ge() {
                return Err(FidelityReason::Overflow);
            }
        }
        integer_of_numeral(&n)
            .map(Value::integer)
            .ok_or(FidelityReason::NotANumber)
    }

    fn coerce_decimal(text: &str, spec: &ValueSpec) -> Result<Value, FidelityReason> {
        let dec = parse_decimal(text).ok_or(FidelityReason::NotANumber)?;
        if let Decimal::Finite(d) = &dec {
            if let Some(scale) = spec.scale
                && d.minimal_scale() > Nat::from_u64(u64::from(scale))
            {
                return Err(FidelityReason::ScaleExceeded { max: scale });
            }
            if let Some(precision) = spec.precision {
                let fits = spec.scale.map_or_else(
                    || d.positional_digits() <= Nat::from_u64(u64::from(precision)),
                    |scale| {
                        d.integer_digits()
                            <= Nat::from_u64(u64::from(precision.saturating_sub(scale)))
                    },
                );
                if !fits {
                    return Err(FidelityReason::PrecisionExceeded { max: precision });
                }
            }
        }
        Ok(Value::Decimal(dec))
    }

    fn coerce_temporal(text: &str, spec: &ValueSpec) -> Result<Value, FidelityReason> {
        let kind = match spec.kind {
            ScalarKind::Date => TemporalKind::Date,
            ScalarKind::Time => TemporalKind::Time,
            _ => TemporalKind::DateTime,
        };
        let t = temporal::parse(kind, text).ok_or(FidelityReason::InvalidTemporal)?;
        match (t.offset, spec.with_time_zone) {
            (Offset::Local, Some(true)) => Err(FidelityReason::OffsetRequired),
            (o, Some(false)) if o.is_zoned() => Err(FidelityReason::OffsetForbidden),
            _ => Ok(Value::Temporal(t)),
        }
    }

    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const B64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    /// Base64 by its definition (RFC 4648 Sec 4, Sec 5): each character is six
    /// bits, the bit string is cut into bytes, the leftover bits (fewer than
    /// eight) must be zero, a body of length 1 mod 4 is no encoding, and
    /// padding brings the length to a multiple of four.
    fn decode_base64(text: &str, alphabet: &[u8; 64], padding_required: bool) -> Option<Vec<u8>> {
        let body = text.trim_end_matches('=');
        let pads = text.len() - body.len();
        let expected_pads = (4 - body.len() % 4) % 4;
        let padding_ok = if padding_required {
            pads == expected_pads
        } else {
            pads == 0 || pads == expected_pads
        };
        if !padding_ok || body.len() % 4 == 1 {
            return None;
        }
        let mut bits: Vec<bool> = Vec::new();
        for c in body.bytes() {
            let v = alphabet.iter().position(|a| *a == c)?;
            for k in (0..6).rev() {
                bits.push((v >> k) & 1 == 1);
            }
        }
        let whole = bits.len() / 8 * 8;
        if bits[whole..].iter().any(|b| *b) {
            return None;
        }
        Some(
            bits[..whole]
                .chunks(8)
                .map(|byte| byte.iter().fold(0u8, |acc, b| (acc << 1) | u8::from(*b)))
                .collect(),
        )
    }

    fn decode_bytes(text: &str, encoding: BytesText) -> Option<Vec<u8>> {
        match encoding {
            BytesText::Base64 => decode_base64(text, B64, false),
            BytesText::Base64Url => decode_base64(text, B64_URL, false),
            BytesText::Base64Padded => decode_base64(text, B64, true),
            BytesText::Base64UrlPadded => decode_base64(text, B64_URL, true),
            BytesText::Hex => {
                if text.len() % 2 != 0 {
                    return None;
                }
                text.as_bytes()
                    .chunks(2)
                    .map(|p| {
                        let h = char::from(p[0]).to_digit(16)?;
                        let l = char::from(p[1]).to_digit(16)?;
                        u8::try_from(h * 16 + l).ok()
                    })
                    .collect()
            }
            BytesText::Raw => Some(text.as_bytes().to_vec()),
            BytesText::Latin1 => text
                .chars()
                .map(|c| u8::try_from(u32::from(c)).ok())
                .collect(),
        }
    }

    /// The positional text of a decimal (no exponent), the form Rust's
    /// `Display` writes a float in: `0.001`, `1000`, `-2.5`.
    #[must_use]
    pub fn positional(d: &Dec) -> String {
        let digits = d.coefficient.to_string();
        let sign = if d.negative { "-" } else { "" };
        if d.exponent >= 0 {
            let zeros = usize::try_from(d.exponent).unwrap_or(0);
            return format!(
                "{sign}{digits}{}",
                "0".repeat(if d.coefficient.is_zero() { 0 } else { zeros })
            );
        }
        let frac = usize::try_from(-d.exponent).unwrap_or(0);
        if digits.len() > frac {
            let (i, f) = digits.split_at(digits.len() - frac);
            format!("{sign}{i}.{f}")
        } else {
            format!("{sign}0.{}{digits}", "0".repeat(frac - digits.len()))
        }
    }

    /// The lexeme of a value with a scalar lexeme, and its class: the value
    /// read back through [`coerce`] by [`coerce_value`]. A finite float is its
    /// shortest binary64 text (its shortest binary32 text under a 32-bit kind
    /// when it is a binary32 value), written positionally; `None` for a value
    /// with no scalar lexeme.
    fn lexeme_of(v: &Value, kind: ScalarKind) -> Option<(String, LexClass)> {
        Some(match v {
            Value::Bool(b) => (b.to_string(), LexClass::Bool),
            Value::Int(i) => (i.to_string(), LexClass::Number),
            Value::BigInt(b) => {
                let sign = if b.negative { "-" } else { "" };
                (
                    format!("{sign}{}e{}", b.significand, b.zeros),
                    LexClass::Number,
                )
            }
            Value::Decimal(Decimal::Finite(d)) => {
                let sign = if d.negative { "-" } else { "" };
                (
                    format!("{sign}{}e{}", d.digits, d.exponent),
                    LexClass::Number,
                )
            }
            Value::Decimal(Decimal::Infinite { negative }) => (
                if *negative { "-Infinity" } else { "Infinity" }.to_owned(),
                LexClass::Number,
            ),
            Value::Decimal(Decimal::NaN {
                negative,
                signalling,
                payload,
            }) => {
                let sign = if *negative { "-" } else { "" };
                let signal = if *signalling { "s" } else { "" };
                let tail = if payload.is_zero() {
                    String::new()
                } else {
                    payload.to_string()
                };
                (format!("{sign}{signal}NaN{tail}"), LexClass::Number)
            }
            Value::Float(bits) => {
                let narrow = match kind {
                    ScalarKind::Float { bits: 32 } => {
                        narrow_bits(Format::B32, *bits).map(|b| (Format::B32, b))
                    }
                    _ => None,
                };
                let (format, b) = narrow.unwrap_or((Format::B64, *bits));
                (positional(&shortest(format, b)?), LexClass::Number)
            }
            Value::Str(s) | Value::Token(s) => (s.clone(), LexClass::String),
            Value::Temporal(t) => (t.to_string(), LexClass::DateTime),
            Value::Null => ("null".to_owned(), LexClass::Null),
            _ => return None,
        })
    }

    /// The exact value of a numeric value, for the float-kind rules.
    fn exact_numeric(v: &Value) -> Option<DecimalValue> {
        match v {
            Value::Int(i) => Some(ExactInteger::Int(*i).as_decimal()),
            Value::BigInt(b) => Some(ExactInteger::Big(b.clone()).as_decimal()),
            Value::Decimal(Decimal::Finite(d)) => Some(d.clone()),
            _ => None,
        }
    }

    /// Definition: Coerce an existing value under `spec`.
    ///
    /// Rules: under `Any` the value is unchanged. Under a float kind of width b
    /// (and the exact reading): a finite `Float` is kept unchanged iff b holds
    /// it exactly, else refused with `Overflow` when it rounds to an infinity at
    /// b and `PrecisionExceeded { max }` otherwise; an infinity is kept; a NaN
    /// is kept iff its fraction bits below b's fraction are zero (quiet or
    /// signalling), else refused with `NonFinite`; an `Int`, `BigInt` or finite
    /// `Decimal` gives `Float(x)` iff b holds its value x exactly (a decimal
    /// zero keeps its sign), else `Overflow` or `PrecisionExceeded`; a special
    /// decimal is refused with `NonFinite`. Under any other non-`Any` kind a
    /// non-finite `Float` is refused with `NonFinite`, a value with a scalar
    /// lexeme is read through [`coerce`], and any other value is unchanged.
    /// `None` under the nearest-keeping-lexeme reading (no rule states how an
    /// existing value is read under it) or a float width other than 16, 32
    /// and 64.
    ///
    /// Book anchors: `L-values-3`, `L-values-5`, `L-values-6`.
    #[must_use]
    pub fn coerce_value(v: &Value, spec: &ValueSpec) -> Coerced {
        if spec.kind == ScalarKind::Any {
            return Some(Ok(v.clone()));
        }
        if let ScalarKind::Float { bits } = spec.kind {
            let format = Format::of_width(bits)?;
            if spec.float_reading == FloatReading::NearestKeepingLexeme {
                return None;
            }
            return Some(coerce_into_float(v, format));
        }
        if let Value::Float(bits) = v
            && !matches!(decode(Format::B64, *bits), FloatValue::Finite { .. })
        {
            return Some(Err(FidelityReason::NonFinite));
        }
        if matches!(
            v,
            Value::Decimal(Decimal::Infinite { .. } | Decimal::NaN { .. })
        ) && spec.kind == ScalarKind::BigInt
        {
            return Some(Err(FidelityReason::NonFinite));
        }
        match lexeme_of(v, spec.kind) {
            Some((text, class)) => coerce(&text, &class, spec),
            None => Some(Ok(v.clone())),
        }
    }

    /// The discriminator-driven coercion operation.
    ///
    /// This is the manifest name for [`coerce_discriminated`]. The latter is
    /// kept as the descriptive implementation name used by the unit tests.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-values-22`.
    pub use coerce_discriminated as coerce_tagged;

    fn coerce_into_float(v: &Value, format: Format) -> Result<Value, FidelityReason> {
        let reject = |q: &Rat, negative: bool| match round_to(format, q, negative) {
            Rounded::Overflow { .. } => FidelityReason::Overflow,
            Rounded::Finite(_) => FidelityReason::PrecisionExceeded {
                max: format.max_significant_digits(),
            },
        };
        match v {
            Value::Float(bits) => match decode(Format::B64, *bits) {
                FloatValue::Finite { negative, value } => {
                    if holds_exactly(format, &value) {
                        Ok(v.clone())
                    } else {
                        Err(reject(&value, negative))
                    }
                }
                FloatValue::Infinite { .. } => Ok(v.clone()),
                FloatValue::NaN { .. } => narrow_bits(format, *bits)
                    .map(|_| v.clone())
                    .ok_or(FidelityReason::NonFinite),
            },
            Value::Decimal(Decimal::Infinite { .. } | Decimal::NaN { .. }) => {
                Err(FidelityReason::NonFinite)
            }
            _ => match exact_numeric(v) {
                Some(d) => match d.scaled(-BOUND, BOUND) {
                    Scaled::AtLeastHi => Err(FidelityReason::Overflow),
                    Scaled::BelowLo => Err(FidelityReason::PrecisionExceeded {
                        max: format.max_significant_digits(),
                    }),
                    Scaled::Value(q) => {
                        if holds_exactly(format, &q) {
                            Ok(Value::Float(
                                round_to(Format::B64, &q, d.negative).bits(Format::B64),
                            ))
                        } else {
                            Err(reject(&q, d.negative))
                        }
                    }
                },
                None => match lexeme_of(v, ScalarKind::Float { bits: 64 }) {
                    Some((text, class)) => coerce(
                        &text,
                        &class,
                        &ValueSpec::of(ScalarKind::Float {
                            bits: u8::try_from(format.width()).unwrap_or(64),
                        }),
                    )
                    .unwrap_or(Err(FidelityReason::NotANumber)),
                    None => Ok(v.clone()),
                },
            },
        }
    }
}

pub mod exact {
    //! Exact numbers: numeral lexemes, decimals with an unbounded exponent and
    //! integers outside `i64` held as a significand and a count of trailing
    //! zeros.

    use std::cmp::Ordering;

    use crate::num::{Int, Nat, Rat};

    /// A decimal numeral as written: sign, the digits before and after the
    /// point, and the written exponent.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Numeral {
        /// Whether a minus sign was written.
        pub negative: bool,
        /// The digits before the point (possibly empty).
        pub integer: String,
        /// The digits after the point (possibly empty).
        pub fraction: String,
        /// The written exponent (zero when none was written).
        pub exponent: Int,
        /// Whether a point was written.
        pub has_point: bool,
        /// Whether an exponent was written.
        pub has_exponent: bool,
    }

    /// A NaN or infinity spelling: `nan`, `inf` or `infinity` in any case,
    /// after an optional sign and an optional `.` (YAML's `.nan`, `.inf`).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum NonFinite {
        /// A NaN spelling, with whether a minus sign was written.
        NaN {
            /// A minus sign was written.
            negative: bool,
        },
        /// An infinity spelling with its sign.
        Infinity {
            /// A minus sign was written.
            negative: bool,
        },
    }

    /// Read a NaN or infinity spelling.
    #[must_use]
    pub fn parse_non_finite(text: &str) -> Option<NonFinite> {
        let (negative, rest) = split_sign(text);
        let rest = rest.strip_prefix('.').unwrap_or(rest);
        if rest.eq_ignore_ascii_case("nan") {
            Some(NonFinite::NaN { negative })
        } else if rest.eq_ignore_ascii_case("inf") || rest.eq_ignore_ascii_case("infinity") {
            Some(NonFinite::Infinity { negative })
        } else {
            None
        }
    }

    fn split_sign(text: &str) -> (bool, &str) {
        match text.as_bytes().first() {
            Some(b'-') => (true, &text[1..]),
            Some(b'+') => (false, &text[1..]),
            _ => (false, text),
        }
    }

    fn all_digits(s: &str) -> bool {
        s.bytes().all(|b| b.is_ascii_digit())
    }

    /// Read a decimal numeral.
    ///
    /// An optional sign, digits with an optional point (either side of the point may be empty, not
    /// both), and an optional exponent `e`/`E` with an optional sign and at least one digit. This is
    /// the JSON number grammar widened by the decimal forms of SQL and XML Schema (a leading `+`, a
    /// point with an empty side). `None` otherwise.
    #[must_use]
    pub fn parse_numeral(text: &str) -> Option<Numeral> {
        let (negative, rest) = split_sign(text);
        let (mantissa, exponent_text) = rest
            .find(['e', 'E'])
            .map_or((rest, None), |at| (&rest[..at], Some(&rest[at + 1..])));
        let (integer, fraction, has_point) =
            mantissa.find('.').map_or((mantissa, "", false), |at| {
                (&mantissa[..at], &mantissa[at + 1..], true)
            });
        if (integer.is_empty() && fraction.is_empty())
            || !all_digits(integer)
            || !all_digits(fraction)
        {
            return None;
        }
        let exponent = match exponent_text {
            None => Int::zero(),
            Some(e) => {
                let (en, digits) = split_sign(e);
                if digits.is_empty() || !all_digits(digits) {
                    return None;
                }
                Int::new(en, Nat::parse(digits)?)
            }
        };
        Some(Numeral {
            negative,
            integer: integer.to_owned(),
            fraction: fraction.to_owned(),
            exponent,
            has_point,
            has_exponent: exponent_text.is_some(),
        })
    }

    impl Numeral {
        /// The significand digits `integer ++ fraction` as a natural.
        #[must_use]
        pub fn significand(&self) -> Nat {
            Nat::parse(&format!("{}{}", self.integer, self.fraction)).unwrap_or_default()
        }

        /// The stored exponent: the written exponent less the number of
        /// fractional digits.
        #[must_use]
        pub fn stored_exponent(&self) -> Int {
            let f = Int::new(
                false,
                Nat::from_u64(u64::try_from(self.fraction.len()).unwrap_or(u64::MAX)),
            );
            self.exponent.sub(&f)
        }

        /// Whether every significand digit is zero.
        #[must_use]
        pub fn is_zero(&self) -> bool {
            self.significand().is_zero()
        }

        /// Whether this is an integer numeral (no point, no exponent).
        #[must_use]
        pub const fn is_integer_form(&self) -> bool {
            !self.has_point && !self.has_exponent
        }
    }

    /// A decimal value placed against two powers of ten, so that a number with
    /// an exponent too large to expand is still classified exactly.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Scaled {
        /// The exact value (its magnitude lies in `[10^lo, 10^hi)` or is zero).
        Value(Rat),
        /// The magnitude is at least `10^hi`.
        AtLeastHi,
        /// The magnitude is non-zero and below `10^lo`.
        BelowLo,
    }

    /// `10^k` for an exponent `k` that fits `i64`.
    fn pow10_small(k: &Int) -> Option<Rat> {
        k.to_i64().map(Rat::pow10)
    }
    /// A finite exact decimal `(-1)^negative x digits x 10^exponent` with an unbounded integer
    /// exponent.
    ///
    /// `digits` has no leading zero (zero is `"0"`), a zero keeps a written minus sign (`-0` and
    /// `-0E+3` are negative zeros), and the representation keeps the written scale (`1.50` and `1.5`
    /// differ; [`DecimalValue::cmp_numeric`] compares numbers).
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct DecimalValue {
        /// The sign (a zero may be negative).
        pub negative: bool,
        /// The significant digits, without leading zeros.
        pub digits: Nat,
        /// The power of ten.
        pub exponent: Int,
    }

    impl DecimalValue {
        /// The decimal of a numeral: digits = integer and fraction digits with
        /// leading zeros dropped, exponent = the stored exponent.
        #[must_use]
        pub fn of_numeral(n: &Numeral) -> Self {
            let digits = n.significand();
            Self {
                negative: n.negative,
                digits,
                exponent: n.stored_exponent(),
            }
        }

        /// Read a decimal lexeme ([`parse_numeral`]); `None` otherwise.
        #[must_use]
        pub fn parse(text: &str) -> Option<Self> {
            parse_numeral(text).map(|n| Self::of_numeral(&n))
        }

        /// Whether this is zero.
        #[must_use]
        pub fn is_zero(&self) -> bool {
            self.digits.is_zero()
        }

        /// `max(0, -exponent)`, the number of fractional digits written.
        #[must_use]
        pub fn scale(&self) -> Nat {
            if self.exponent.is_negative() {
                self.exponent.magnitude().clone()
            } else {
                Nat::zero()
            }
        }

        /// The number of trailing zero digits of `digits` (none for zero).
        fn trailing_zeros(&self) -> Nat {
            if self.is_zero() {
                return Nat::zero();
            }
            let mut k = 0u64;
            while self.digits.digit(usize::try_from(k).unwrap_or(0)) == 0 {
                k += 1;
            }
            Nat::from_u64(k)
        }

        /// The scale with trailing fractional zeros dropped; zero has scale 0.
        #[must_use]
        pub fn minimal_scale(&self) -> Nat {
            if self.is_zero() {
                return Nat::zero();
            }
            self.scale()
                .checked_sub(&self.trailing_zeros())
                .unwrap_or_default()
        }

        /// The number of digits before the point once leading zeros are dropped:
        /// `max(0, len(digits) + exponent)`; zero has none.
        #[must_use]
        pub fn integer_digits(&self) -> Nat {
            if self.is_zero() {
                return Nat::zero();
            }
            let len = Int::new(
                false,
                Nat::from_u64(u64::try_from(self.digits.digit_count()).unwrap_or(0)),
            );
            let pos = len.add(&self.exponent);
            if pos.is_negative() {
                Nat::zero()
            } else {
                pos.magnitude().clone()
            }
        }

        /// The digits in positional notation: integer digits plus minimal scale.
        #[must_use]
        pub fn positional_digits(&self) -> Nat {
            self.integer_digits().add(&self.minimal_scale())
        }

        /// The position of the leading digit: the magnitude lies in
        /// `[10^(p-1), 10^p)` for `p = len(digits) + exponent` (zero has none).
        #[must_use]
        pub fn position(&self) -> Int {
            Int::new(
                false,
                Nat::from_u64(u64::try_from(self.digits.digit_count()).unwrap_or(0)),
            )
            .add(&self.exponent)
        }

        /// The value placed against `10^lo` and `10^hi` (`lo < hi`): exact when
        /// the magnitude lies in `[10^lo, 10^hi)` or is zero, else which side
        /// it lies on. Decided on the leading-digit position, so no power of
        /// ten beyond the two bounds is ever expanded.
        #[must_use]
        pub fn scaled(&self, lo: i64, hi: i64) -> Scaled {
            if self.is_zero() {
                return Scaled::Value(Rat::zero());
            }
            let p = self.position();
            if p > Int::from_i64(hi) {
                return Scaled::AtLeastHi;
            }
            if p <= Int::from_i64(lo) {
                return Scaled::BelowLo;
            }
            let e = pow10_small(&self.exponent).unwrap_or_else(Rat::zero);
            Scaled::Value(Rat::from_int(Int::new(self.negative, self.digits.clone())).mul(&e))
        }

        /// Definition: Compare as numbers, ignoring scale. Exact for every exponent: the
        /// position of the leading digit (`len(digits) + exponent`) decides
        /// unless equal, then the digit strings aligned at the leading digit.
        ///
        /// Book anchors: `L-values-10`, `L-values-35`, `L-values-36`.
        #[must_use]
        pub fn cmp_numeric(&self, other: &Self) -> Ordering {
            let sign = |d: &Self| {
                if d.is_zero() {
                    0i8
                } else if d.negative {
                    -1
                } else {
                    1
                }
            };
            let (a, b) = (sign(self), sign(other));
            if a != b || a == 0 {
                return a.cmp(&b);
            }
            let m = cmp_magnitude(self, other);
            if a < 0 { m.reverse() } else { m }
        }
    }

    fn cmp_magnitude(left: &DecimalValue, right: &DecimalValue) -> Ordering {
        left.position().cmp(&right.position()).then_with(|| {
            let (lhs, rhs) = (left.digits.to_string(), right.digits.to_string());
            let width = lhs.len().max(rhs.len());
            let pad = |text: &str| format!("{text:0<width$}");
            pad(&lhs).cmp(&pad(&rhs))
        })
    }

    /// An integer outside `i64`: `(-1)^negative x significand x 10^zeros`, the
    /// significand without leading zeros and with a last digit other than 0,
    /// `zeros` an unbounded count of trailing zeros.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct BigIntValue {
        /// Whether the integer is negative.
        pub negative: bool,
        /// The significand, its last digit non-zero.
        pub significand: Nat,
        /// The count of trailing zeros.
        pub zeros: Nat,
    }

    /// An exact integer: `Int` when it fits `i64`, else `BigInt`, so each
    /// integer has one representation.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub enum ExactInteger {
        /// An integer within `i64`.
        Int(i64),
        /// An integer outside `i64`.
        Big(BigIntValue),
    }

    impl ExactInteger {
        /// The representation of `(-1)^negative x significand x 10^zeros`:
        /// trailing zeros of the significand move into the count, and a value
        /// within `i64` is `Int`. The count may be any natural, so no expansion
        /// past a bound ever happens.
        #[must_use]
        pub fn from_parts(negative: bool, significand: &Nat, zeros: &Nat) -> Self {
            if significand.is_zero() {
                return Self::Int(0);
            }
            let mut s = significand.clone();
            let mut z = zeros.clone();
            while s.digit(0) == 0 {
                s = s
                    .div_rem(&Nat::from_u64(10))
                    .map(|(q, _)| q)
                    .unwrap_or_default();
                z = z.add(&Nat::one());
            }
            // Small enough to expand: 19 digits bound i64.
            if z.to_u64().is_some_and(|k| k <= 19) && s.digit_count() <= 19 {
                let full = s.shift10(usize::try_from(z.to_u64().unwrap_or(0)).unwrap_or(0));
                if let Some(v) = Int::new(negative, full).to_i64() {
                    return Self::Int(v);
                }
            }
            Self::Big(BigIntValue {
                negative,
                significand: s,
                zeros: z,
            })
        }

        /// The exact value; `None` when the count of zeros exceeds `i64`.
        #[must_use]
        pub fn value(&self) -> Option<Rat> {
            match self {
                Self::Int(v) => Some(Rat::from_int(Int::from_i64(*v))),
                Self::Big(b) => {
                    let k = i64::try_from(b.zeros.to_u64()?).ok()?;
                    Some(
                        Rat::from_int(Int::new(b.negative, b.significand.clone()))
                            .mul(&Rat::pow10(k)),
                    )
                }
            }
        }

        /// Compare two integers as numbers, exactly, for any count of zeros.
        #[must_use]
        pub fn cmp_numeric(&self, other: &Self) -> Ordering {
            self.as_decimal().cmp_numeric(&other.as_decimal())
        }

        /// The integer as a decimal of exponent `zeros` (or 0).
        #[must_use]
        pub fn as_decimal(&self) -> DecimalValue {
            match self {
                Self::Int(v) => DecimalValue {
                    negative: *v < 0,
                    digits: Nat::from_u64(v.unsigned_abs()),
                    exponent: Int::zero(),
                },
                Self::Big(b) => DecimalValue {
                    negative: b.negative,
                    digits: b.significand.clone(),
                    exponent: Int::new(false, b.zeros.clone()),
                },
            }
        }
    }

    /// The integer an integral numeral denotes, or `None` when the numeral has
    /// a fractional part that is not zero (its minimal scale is positive).
    /// Every integral numeral is read, whatever its exponent.
    #[must_use]
    pub fn integer_of_numeral(n: &Numeral) -> Option<ExactInteger> {
        let d = DecimalValue::of_numeral(n);
        if d.is_zero() {
            return Some(ExactInteger::Int(0));
        }
        if !d.minimal_scale().is_zero() {
            return None;
        }
        // Integral: strip trailing zeros into the count, then add the exponent.
        let mut s = d.digits.clone();
        let mut z = Int::zero();
        while s.digit(0) == 0 {
            s = s
                .div_rem(&Nat::from_u64(10))
                .map(|(q, _)| q)
                .unwrap_or_default();
            z = z.add(&Int::from_i64(1));
        }
        let count = z.add(&d.exponent);
        Some(ExactInteger::from_parts(d.negative, &s, count.magnitude()))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn numerals_read_their_stored_exponent_and_scale() {
            let d = DecimalValue::parse("1.50");
            let shown = d.as_ref().map(|d| {
                (
                    d.digits.to_string(),
                    d.exponent.to_string(),
                    d.scale().to_string(),
                )
            });
            assert_eq!(shown, Some(("150".into(), "-2".into(), "2".into())));
            assert_eq!(d.map(|d| d.minimal_scale().to_string()), Some("1".into()));
            assert!(parse_numeral(".5").is_some() && parse_numeral("5.").is_some());
            assert!(
                parse_numeral(".").is_none()
                    && parse_numeral("1e").is_none()
                    && parse_numeral("--1").is_none()
            );
            let big = parse_numeral("0.0e-3000000000").map(|n| DecimalValue::of_numeral(&n));
            assert_eq!(
                big.map(|d| d.scale().to_string()),
                Some("3000000001".into())
            );
        }

        #[test]
        fn integers_keep_one_representation_and_any_count() {
            let n = parse_numeral("1e4097").and_then(|n| integer_of_numeral(&n));
            let written = parse_numeral(&format!("1{}", "0".repeat(4097)))
                .and_then(|n| integer_of_numeral(&n));
            assert_eq!(n, written);
            assert_eq!(
                parse_numeral("9223372036854775807").and_then(|n| integer_of_numeral(&n)),
                Some(ExactInteger::Int(i64::MAX))
            );
            assert!(matches!(
                parse_numeral("9223372036854775808").and_then(|n| integer_of_numeral(&n)),
                Some(ExactInteger::Big(_))
            ));
            assert_eq!(
                parse_numeral("-9223372036854775808").and_then(|n| integer_of_numeral(&n)),
                Some(ExactInteger::Int(i64::MIN))
            );
            assert_eq!(
                parse_numeral("1.5").and_then(|n| integer_of_numeral(&n)),
                None
            );
            assert_eq!(
                parse_numeral("150e-1").and_then(|n| integer_of_numeral(&n)),
                Some(ExactInteger::Int(15))
            );
            let huge = parse_numeral("1e9223372036854775807").and_then(|n| integer_of_numeral(&n));
            let bigger =
                parse_numeral("1e9223372036854775808").and_then(|n| integer_of_numeral(&n));
            assert_eq!(
                huge.zip(bigger).map(|(a, b)| a.cmp_numeric(&b)),
                Some(Ordering::Less)
            );
        }
    }

    /// A decimal value: a finite decimal, an infinity, or a NaN with its sign,
    /// signalling flag and payload (the General Decimal Arithmetic values).
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub enum Decimal {
        /// A finite decimal.
        Finite(DecimalValue),
        /// An infinity.
        Infinite {
            /// The sign.
            negative: bool,
        },
        /// A NaN; a plain `NaN` has payload zero (`NaN` and `NaN0` are one value).
        NaN {
            /// The sign.
            negative: bool,
            /// Whether it is signalling (`sNaN`).
            signalling: bool,
            /// The diagnostic payload.
            payload: Nat,
        },
    }

    /// Read a decimal lexeme: a finite numeral ([`parse_numeral`]) or a special
    /// form of the General Decimal Arithmetic numeric string syntax:
    ///
    /// `Infinity` or `Inf`, `NaN` or `sNaN` followed by optional payload
    /// digits, in any case, with an optional sign. `None` otherwise.
    #[must_use]
    pub fn parse_decimal(text: &str) -> Option<Decimal> {
        if let Some(n) = parse_numeral(text) {
            return Some(Decimal::Finite(DecimalValue::of_numeral(&n)));
        }
        let (negative, rest) = split_sign(text);
        let lower = rest.to_ascii_lowercase();
        if lower == "inf" || lower == "infinity" {
            return Some(Decimal::Infinite { negative });
        }
        let (signalling, tail) = if let Some(t) = lower.strip_prefix("snan") {
            (true, t)
        } else {
            (false, lower.strip_prefix("nan")?)
        };
        let payload = if tail.is_empty() {
            Nat::zero()
        } else {
            Nat::parse(tail)?
        };
        Some(Decimal::NaN {
            negative,
            signalling,
            payload,
        })
    }

    impl Decimal {
        /// The rank of a value's class in the numeric preorder: negative quiet
        /// NaNs, negative signalling NaNs, minus infinity, finite, plus
        /// infinity, positive signalling NaNs, positive quiet NaNs.
        const fn class_rank(&self) -> u8 {
            match self {
                Self::NaN {
                    negative: true,
                    signalling: false,
                    ..
                } => 0,
                Self::NaN {
                    negative: true,
                    signalling: true,
                    ..
                } => 1,
                Self::Infinite { negative: true } => 2,
                Self::Finite(_) => 3,
                Self::Infinite { negative: false } => 4,
                Self::NaN {
                    negative: false,
                    signalling: true,
                    ..
                } => 5,
                Self::NaN {
                    negative: false,
                    signalling: false,
                    ..
                } => 6,
            }
        }

        /// The total numeric preorder: finite values by number (every zero
        /// `Equal`), the infinities at the ends of the finite values, and NaNs
        /// in compare-total's places, payloads ignored.
        #[must_use]
        pub fn cmp_numeric(&self, other: &Self) -> Ordering {
            match (self, other) {
                (Self::Finite(a), Self::Finite(b)) => a.cmp_numeric(b),
                _ => self.class_rank().cmp(&other.class_rank()),
            }
        }

        /// Definition: The General Decimal Arithmetic compare-total order, from its rules:
        /// for representations of sign 0, numbers by value and, at equal value,
        /// by exponent (larger higher); every signalling NaN below every quiet
        /// NaN, NaNs by payload; numeric magnitude and the non-finite classes
        /// are reversed for sign 1, while the equal-number representation tie
        /// remains ordered by exponent, and every representation of sign 1 is
        /// below every one of sign 0.
        ///
        /// Book anchors: `L-values-11`, `L-values-37`.
        #[must_use]
        pub fn compare_total(&self, other: &Self) -> Ordering {
            let (sa, sb) = (self.sign(), other.sign());
            if sa != sb {
                return if sa {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
            }
            if let (Self::Finite(a), Self::Finite(b)) = (self, other) {
                let numeric = a.cmp_numeric(b);
                return if numeric.is_eq() {
                    a.exponent.cmp(&b.exponent)
                } else {
                    numeric
                };
            }
            let positive = cmp_total_positive(self, other);
            if sa { positive.reverse() } else { positive }
        }

        const fn sign(&self) -> bool {
            match self {
                Self::Finite(d) => d.negative,
                Self::Infinite { negative } | Self::NaN { negative, .. } => *negative,
            }
        }
    }

    /// Compare-total of two representations of one sign, as if both were of
    /// sign 0.
    fn cmp_total_positive(a: &Decimal, b: &Decimal) -> Ordering {
        let rank = |d: &Decimal| match d {
            Decimal::Finite(_) => 0u8,
            Decimal::Infinite { .. } => 1,
            Decimal::NaN {
                signalling: true, ..
            } => 2,
            Decimal::NaN {
                signalling: false, ..
            } => 3,
        };
        match (a, b) {
            (Decimal::Finite(x), Decimal::Finite(y)) => {
                let (mx, my) = (
                    DecimalValue {
                        negative: false,
                        ..x.clone()
                    },
                    DecimalValue {
                        negative: false,
                        ..y.clone()
                    },
                );
                mx.cmp_numeric(&my)
                    .then_with(|| x.exponent.cmp(&y.exponent))
            }
            (
                Decimal::NaN {
                    signalling: s1,
                    payload: p1,
                    ..
                },
                Decimal::NaN {
                    signalling: s2,
                    payload: p2,
                    ..
                },
            ) if s1 == s2 => p1.cmp(p2),
            _ => rank(a).cmp(&rank(b)),
        }
    }

    #[cfg(test)]
    mod decimal_tests {
        use super::*;

        #[test]
        fn compare_total_follows_its_rules() {
            // Equal finite numbers use their exponent as the representation
            // tie for either sign, so the finer representation sorts first.
            let order = [
                "-NaN",
                "-sNaN",
                "-Infinity",
                "-127",
                "-1.00",
                "-1",
                "-0.000",
                "-0",
                "0",
                "1.2300",
                "1.23",
                "1E+9",
                "Infinity",
                "sNaN",
                "NaN",
                "NaN456",
            ];
            let parsed: Vec<Decimal> = order.iter().filter_map(|t| parse_decimal(t)).collect();
            assert_eq!(parsed.len(), order.len());
            for w in parsed.windows(2) {
                assert_eq!(
                    w[0].compare_total(&w[1]),
                    Ordering::Less,
                    "{:?} {:?}",
                    w[0],
                    w[1]
                );
            }
            for a in &parsed {
                for b in &parsed {
                    if a.cmp_numeric(b) == Ordering::Less {
                        assert_eq!(a.compare_total(b), Ordering::Less);
                    }
                }
            }
            assert_eq!(parse_decimal("NaN"), parse_decimal("nan0"));
        }
    }
}

pub mod float {
    //! IEEE 754 binary interchange formats by exact rationals.
    //!
    //! Decoding a bit pattern, the round-to-nearest-even lexical mapping `L_b`, the closest-shortest
    //! printer `C_b` with ties to even, and the bit-wise narrowing and widening of NaNs.
    //!
    //! No float arithmetic is used. A finite value is a [`Rat`]; rounding
    //! compares the exact remainder with one half.

    use std::cmp::Ordering;

    use crate::num::{Int, Nat, Rat, floor_log2, floor_log10};

    /// A binary interchange format of width 16, 32 or 64.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum Format {
        /// binary16: 11-bit significand, 5-bit exponent.
        B16,
        /// binary32: 24-bit significand, 8-bit exponent.
        B32,
        /// binary64: 53-bit significand, 11-bit exponent.
        B64,
    }

    impl Format {
        /// The format of width `bits`, if it is 16, 32 or 64.
        #[must_use]
        pub const fn of_width(bits: u8) -> Option<Self> {
            match bits {
                16 => Some(Self::B16),
                32 => Some(Self::B32),
                64 => Some(Self::B64),
                _ => None,
            }
        }

        /// The width in bits.
        #[must_use]
        pub const fn width(self) -> u32 {
            match self {
                Self::B16 => 16,
                Self::B32 => 32,
                Self::B64 => 64,
            }
        }

        /// The precision p (significand bits, the implicit bit included).
        #[must_use]
        pub const fn precision(self) -> u32 {
            match self {
                Self::B16 => 11,
                Self::B32 => 24,
                Self::B64 => 53,
            }
        }

        /// The number of exponent bits.
        #[must_use]
        pub const fn exponent_bits(self) -> u32 {
            self.width() - self.precision()
        }

        /// The exponent bias, `2^(w-1) - 1`.
        #[must_use]
        pub const fn bias(self) -> i64 {
            (1i64 << (self.exponent_bits() - 1)) - 1
        }

        /// The least normal exponent, `1 - bias`.
        #[must_use]
        pub const fn emin(self) -> i64 {
            1 - self.bias()
        }

        /// The greatest exponent, `bias`.
        #[must_use]
        pub const fn emax(self) -> i64 {
            self.bias()
        }

        /// The `max` of `PrecisionExceeded` at this width: `1 + ceil(p log10 2)`,
        /// which is 5, 9 and 17 for p = 11, 24 and 53.
        #[must_use]
        pub const fn max_significant_digits(self) -> u32 {
            match self {
                Self::B16 => 5,
                Self::B32 => 9,
                Self::B64 => 17,
            }
        }

        const fn fraction_mask(self) -> u64 {
            (1u64 << (self.precision() - 1)) - 1
        }

        const fn exponent_mask(self) -> u64 {
            (1u64 << self.exponent_bits()) - 1
        }

        const fn sign_bit(self) -> u64 {
            1u64 << (self.width() - 1)
        }
    }

    /// What a bit pattern denotes.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum FloatValue {
        /// A finite value, with the sign of zero kept apart (`negative` is the
        /// sign bit; for a non-zero value it agrees with the sign of `value`).
        Finite {
            /// The sign bit.
            negative: bool,
            /// The exact value.
            value: Rat,
        },
        /// An infinity.
        Infinite {
            /// The sign bit.
            negative: bool,
        },
        /// A NaN.
        NaN {
            /// The sign bit.
            negative: bool,
            /// The fraction field (quiet bit included).
            fraction: u64,
        },
    }

    /// Decode `bits` (the low `format.width()` bits) under `format`.
    ///
    /// Definition (IEEE 754-2019 Sec 3.4): with sign s, biased exponent E and
    /// fraction F, all-ones E is an infinity (F = 0) or a NaN; E = 0 is the
    /// subnormal `(-1)^s F 2^(emin - p + 1)`; otherwise the normal
    /// `(-1)^s (2^(p-1) + F) 2^(E - bias - p + 1)`.
    #[must_use]
    pub fn decode(format: Format, bits: u64) -> FloatValue {
        let p = i64::from(format.precision());
        let negative = bits & format.sign_bit() != 0;
        let e_field = (bits >> (format.precision() - 1)) & format.exponent_mask();
        let fraction = bits & format.fraction_mask();
        if e_field == format.exponent_mask() {
            return if fraction == 0 {
                FloatValue::Infinite { negative }
            } else {
                FloatValue::NaN { negative, fraction }
            };
        }
        let (significand, exponent) = if e_field == 0 {
            (fraction, format.emin() - p + 1)
        } else {
            (
                fraction | (1u64 << (format.precision() - 1)),
                i64::try_from(e_field).unwrap_or(0) - format.bias() - p + 1,
            )
        };
        let magnitude =
            Rat::from_int(Int::new(false, Nat::from_u64(significand))).mul(&Rat::pow2(exponent));
        let value = if negative { magnitude.neg() } else { magnitude };
        FloatValue::Finite { negative, value }
    }

    /// The result of rounding a rational to a format.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Rounded {
        /// A finite bit pattern.
        Finite(u64),
        /// The rounding overflowed to an infinity of this sign.
        Overflow {
            /// The sign.
            negative: bool,
        },
    }

    impl Rounded {
        /// The bit pattern, an overflow giving the infinity of its sign.
        #[must_use]
        pub const fn bits(self, format: Format) -> u64 {
            match self {
                Self::Finite(b) => b,
                Self::Overflow { negative } => {
                    let inf = format.exponent_mask() << (format.precision() - 1);
                    if negative {
                        inf | format.sign_bit()
                    } else {
                        inf
                    }
                }
            }
        }
    }

    /// `L_b`.
    ///
    /// Round the exact value `q` to the nearest value of `format`, ties to the even significand (IEEE
    /// 754-2019 Sec 4.3.1, roundTiesToEven); `negative_zero` gives the sign of a zero `q` (a numeral
    /// `-0` reads as negative zero).
    ///
    /// Definition: with `e = floor(log2 |q|)` and the quantum exponent
    /// `qe = max(e, emin) - p + 1`, the significand is `|q| / 2^qe` rounded to
    /// an integer n by comparing the remainder with 1/2 (ties to even n); a
    /// carry to `2^p` renormalizes; a value of exponent above `emax` overflows.
    ///
    /// Book anchors: `L-values-43`.
    #[must_use]
    pub fn round_to(format: Format, value: &Rat, negative_zero: bool) -> Rounded {
        let precision = i64::from(format.precision());
        let negative = if value.is_zero() {
            negative_zero
        } else {
            value.is_negative()
        };
        let sign = if negative { format.sign_bit() } else { 0 };
        if value.is_zero() {
            return Rounded::Finite(sign);
        }
        let magnitude = value.abs();
        let log2 = floor_log2(&magnitude).unwrap_or(0);
        let mut qe = log2.max(format.emin()) - precision + 1;
        let scaled = magnitude.div(&Rat::pow2(qe)).unwrap_or_else(Rat::zero);
        let floor = scaled.floor();
        let rem = scaled.sub(&Rat::from_int(floor.clone()));
        let half = Rat::pow2(-1);
        let mut significand = floor.magnitude().clone();
        let round_up = match rem.cmp(&half) {
            Ordering::Greater => true,
            Ordering::Equal => !significand.is_even(),
            Ordering::Less => false,
        };
        if round_up {
            significand = significand.add(&Nat::one());
        }
        if significand == Nat::pow2(format.precision()) {
            significand = Nat::pow2(format.precision() - 1);
            qe += 1;
        }
        if significand.is_zero() {
            return Rounded::Finite(sign);
        }
        if qe + precision - 1 > format.emax() {
            return Rounded::Overflow { negative };
        }
        let n64 = significand.to_u64().unwrap_or(0);
        let implicit = 1u64 << (format.precision() - 1);
        if n64 < implicit {
            return Rounded::Finite(sign | n64);
        }
        let biased = u64::try_from(qe + precision - 1 + format.bias()).unwrap_or(0);
        Rounded::Finite(sign | (biased << (format.precision() - 1)) | (n64 - implicit))
    }

    /// Whether the exact value `q` is a value of `format` (rounding returns a
    /// finite pattern that decodes to `q`).
    #[must_use]
    pub fn holds_exactly(format: Format, q: &Rat) -> bool {
        match round_to(format, q, false) {
            Rounded::Finite(bits) => match decode(format, bits) {
                FloatValue::Finite { value, .. } => value == *q,
                _ => false,
            },
            Rounded::Overflow { .. } => false,
        }
    }

    /// A decimal number `(-1)^negative * coefficient * 10^exponent`, with the
    /// coefficient's trailing zeros moved into the exponent (zero is `0e0`).
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct Dec {
        /// The sign (kept for a negative zero).
        pub negative: bool,
        /// The coefficient, without trailing zero digits unless it is zero.
        pub coefficient: Nat,
        /// The power of ten.
        pub exponent: i64,
    }

    impl Dec {
        /// Normalize: strip trailing zeros of the coefficient into the exponent.
        #[must_use]
        pub fn normalized(negative: bool, coefficient: &Nat, exponent: i64) -> Self {
            if coefficient.is_zero() {
                return Self {
                    negative,
                    coefficient: Nat::zero(),
                    exponent: 0,
                };
            }
            let text = coefficient.to_string();
            let trimmed = text.trim_end_matches('0');
            let zeros = i64::try_from(text.len() - trimmed.len()).unwrap_or(0);
            Self {
                negative,
                coefficient: Nat::parse(trimmed).unwrap_or_default(),
                exponent: exponent + zeros,
            }
        }

        /// The number of significant digits (zero has one).
        #[must_use]
        pub fn significant_digits(&self) -> usize {
            self.coefficient.digit_count().max(1)
        }

        /// The exact value.
        #[must_use]
        pub fn value(&self) -> Rat {
            let m = Rat::from_int(Int::new(self.negative, self.coefficient.clone()));
            m.mul(&Rat::pow10(self.exponent))
        }

        /// Plain scientific text `[-]d[.ddd]e<exp>`, the form tests read.
        #[must_use]
        pub fn to_scientific(&self) -> String {
            let digits = self.coefficient.to_string();
            let sign = if self.negative { "-" } else { "" };
            let point_exp = self.exponent + i64::try_from(digits.len()).unwrap_or(0) - 1;
            if digits.len() == 1 {
                format!("{sign}{digits}e{point_exp}")
            } else {
                format!("{sign}{}.{}e{point_exp}", &digits[..1], &digits[1..])
            }
        }
    }

    /// The rounding interval of a positive finite pattern `bits`: the exact
    /// midpoints to its neighbours below and above. The neighbour below the
    /// least subnormal is zero; the "neighbour" above the greatest finite value
    /// is `2^(emax+1)`, the point at which rounding overflows.
    fn midpoints(format: Format, bits: u64) -> Option<(Rat, Rat, Rat)> {
        let FloatValue::Finite { value, .. } = decode(format, bits) else {
            return None;
        };
        let below = if bits == 0 {
            Rat::zero()
        } else {
            match decode(format, bits - 1) {
                FloatValue::Finite { value, .. } => value,
                _ => return None,
            }
        };
        let above = match decode(format, bits + 1) {
            FloatValue::Finite { value, .. } => value,
            FloatValue::Infinite { .. } => Rat::pow2(format.emax() + 1),
            FloatValue::NaN { .. } => return None,
        };
        let half = Rat::pow2(-1);
        let lo = value.add(&below).mul(&half);
        let hi = value.add(&above).mul(&half);
        Some((value, lo, hi))
    }

    /// Definition: `C_b`: the closest-shortest decimal of the finite pattern `bits`, ties to
    /// even.
    ///
    /// Definition: among the decimals with the fewest significant digits that
    /// read back to the value under [`round_to`], the one closest to the value,
    /// and at an exact tie the one whose last significant digit is even. The
    /// search is naive: for k = 1, 2, ... every multiple of `10^(e-k+1)` lying
    /// between the two rounding midpoints (`e = floor(log10 |x|)`) is a
    /// candidate of at most k significant digits; each candidate is read back
    /// with [`round_to`], and the first k with a candidate that reads back is
    /// the minimal length. Returns `None` for a NaN or an infinity.
    ///
    /// Book anchors: `L-values-9`, `L-values-33`.
    #[must_use]
    pub fn shortest(format: Format, bits: u64) -> Option<Dec> {
        let negative = bits & format.sign_bit() != 0;
        let magnitude_bits = bits & !format.sign_bit();
        let (x, lo, hi) = midpoints(format, magnitude_bits)?;
        if x.is_zero() {
            return Some(Dec::normalized(negative, &Nat::zero(), 0));
        }
        let e = floor_log10(&x)?;
        for k in 1i64.. {
            let unit_exp = e - k + 1;
            let unit = Rat::pow10(unit_exp);
            let first = ceil_div(&lo, &unit);
            let last = lo_floor_div(&hi, &unit);
            let mut best: Option<(Nat, Rat)> = None;
            let mut c = first;
            while c <= last {
                let candidate = Rat::from_int(Int::new(false, c.clone())).mul(&unit);
                if round_to(format, &candidate, false) == Rounded::Finite(magnitude_bits) {
                    let distance = candidate.sub(&x).abs();
                    best = Some(match best {
                        None => (c.clone(), distance),
                        Some((bc, bd)) => pick_closer(bc, bd, c.clone(), distance),
                    });
                }
                c = c.add(&Nat::one());
            }
            if let Some((coefficient, _)) = best {
                return Some(Dec::normalized(negative, &coefficient, unit_exp));
            }
        }
        None
    }

    /// The closer of two candidate coefficients, ties to the one whose last
    /// significant digit is even.
    fn pick_closer(a: Nat, da: Rat, b: Nat, db: Rat) -> (Nat, Rat) {
        match da.cmp(&db) {
            Ordering::Less => (a, da),
            Ordering::Greater => (b, db),
            Ordering::Equal => {
                if last_significant_digit(&a) % 2 == 0 {
                    (a, da)
                } else {
                    (b, db)
                }
            }
        }
    }

    fn last_significant_digit(n: &Nat) -> u8 {
        (0..n.digit_count())
            .map(|i| n.digit(i))
            .find(|d| *d != 0)
            .unwrap_or(0)
    }

    /// `ceil(a / u)` for non-negative `a` and positive `u`, as a natural.
    fn ceil_div(a: &Rat, u: &Rat) -> Nat {
        let q = a.div(u).unwrap_or_else(Rat::zero);
        let f = q.floor();
        if q.is_integer() {
            f.magnitude().clone()
        } else {
            f.magnitude().add(&Nat::one())
        }
    }

    /// `floor(a / u)` for non-negative `a` and positive `u`, as a natural.
    fn lo_floor_div(a: &Rat, u: &Rat) -> Nat {
        a.div(u)
            .unwrap_or_else(Rat::zero)
            .floor()
            .magnitude()
            .clone()
    }

    /// Definition: The binary64 pattern that widens the `format` pattern `bits` on the bits.
    ///
    /// A finite value exactly, an infinity to the infinity of its sign, and a NaN to the NaN with its
    /// sign, an all-ones exponent and its fraction shifted to the high fraction bits of binary64 (quiet
    /// or signalling as written; no quieting).
    ///
    /// Book anchors: `L-values-8`.
    #[must_use]
    pub fn widen_bits(format: Format, bits: u64) -> u64 {
        match decode(format, bits) {
            FloatValue::Finite { negative, value } => {
                round_to(Format::B64, &value, negative).bits(Format::B64)
            }
            FloatValue::Infinite { negative } => Rounded::Overflow { negative }.bits(Format::B64),
            FloatValue::NaN { negative, fraction } => {
                let shift = Format::B64.precision() - format.precision();
                let sign = if negative { Format::B64.sign_bit() } else { 0 };
                sign | (Format::B64.exponent_mask() << 52) | (fraction << shift)
            }
        }
    }

    /// Definition: The `format` pattern of the binary64 pattern `bits` when `format` holds it exactly, on the bits.
    ///
    /// A finite value iff it is a value of `format`, an infinity always, and a NaN iff its fraction
    /// bits below the narrow fraction are zero (quiet or signalling). `None` otherwise.
    ///
    /// Book anchors: `L-values-8`.
    #[must_use]
    pub fn narrow_bits(format: Format, bits: u64) -> Option<u64> {
        match decode(Format::B64, bits) {
            FloatValue::Finite { negative, value } => {
                if !holds_exactly(format, &value) {
                    return None;
                }
                Some(round_to(format, &value, negative).bits(format))
            }
            FloatValue::Infinite { negative } => Some(Rounded::Overflow { negative }.bits(format)),
            FloatValue::NaN { negative, fraction } => {
                let shift = Format::B64.precision() - format.precision();
                if fraction & ((1u64 << shift) - 1) != 0 {
                    return None;
                }
                let sign = if negative { format.sign_bit() } else { 0 };
                Some(
                    sign | (format.exponent_mask() << (format.precision() - 1))
                        | (fraction >> shift),
                )
            }
        }
    }

    /// Definition: The canonical quiet NaN produced by arithmetic in `format`.
    ///
    /// Book anchors: `L-values-7`.
    #[must_use]
    pub const fn computed_nan(format: Format) -> u64 {
        format.exponent_mask() << (format.precision() - 1) | 1u64 << (format.precision() - 2)
    }

    /// Definition: The binary16 lexical mapping `L_16`: exact round-to-nearest, ties to
    /// even, with the supplied sign retained for zero.
    ///
    /// Book anchors: `L-values-30`.
    #[must_use]
    pub fn binary16(value: &Rat, negative: bool) -> Rounded {
        round_to(Format::B16, value, negative)
    }

    /// The exact-reading operation used by the default float route.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-values-34`.
    pub use holds_exactly as exact_reading;

    /// The nearest-reading operation used by routes that retain their input
    /// lexeme in a complement.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-values-31`, `L-values-32`.
    pub use round_to as nearest_reading;

    /// The canonical quiet NaN of binary64, bits `0x7ff8000000000000`.
    pub const CANONICAL_NAN_BITS: u64 = 0x7ff8_0000_0000_0000;

    /// The negative canonical NaN of binary64, bits `0xfff8000000000000`.
    pub const NEGATIVE_CANONICAL_NAN_BITS: u64 = 0xfff8_0000_0000_0000;

    #[cfg(test)]
    mod tests {
        use super::*;

        fn rat(text: &str) -> Rat {
            let (n, d) = text.split_once('/').unwrap_or((text, "1"));
            Rat::new(
                Int::parse(n).unwrap_or_default(),
                Nat::parse(d).unwrap_or_default(),
            )
            .unwrap_or_else(Rat::zero)
        }

        #[test]
        fn worked_examples_round_to_the_published_patterns() {
            // binary64 nearest 1/10 is 0x3fb999999999999a; binary32 nearest is
            // 0x3dcccccd; binary16 nearest is 0x2e66 (IEEE 754 worked values).
            assert_eq!(
                round_to(Format::B64, &rat("1/10"), false),
                Rounded::Finite(0x3fb9_9999_9999_999a)
            );
            assert_eq!(
                round_to(Format::B32, &rat("1/10"), false),
                Rounded::Finite(0x3dcc_cccd)
            );
            assert_eq!(
                round_to(Format::B16, &rat("1/10"), false),
                Rounded::Finite(0x2e66)
            );
            // 65504 is the largest binary16; 65520 rounds to infinity; 65519
            // rounds down to 65504.
            assert_eq!(
                round_to(Format::B16, &rat("65504"), false),
                Rounded::Finite(0x7bff)
            );
            assert_eq!(
                round_to(Format::B16, &rat("65520"), false),
                Rounded::Overflow { negative: false }
            );
            assert_eq!(
                round_to(Format::B16, &rat("65519"), false),
                Rounded::Finite(0x7bff)
            );
            // 2049 is a tie between 2048 and 2050 at binary16; even is 2048.
            assert_eq!(
                round_to(Format::B16, &rat("2049"), false),
                Rounded::Finite(0x6800)
            );
            // 2^-1075 is a tie between 0 and the least subnormal; even is 0.
            assert_eq!(
                round_to(Format::B64, &Rat::pow2(-1075), false),
                Rounded::Finite(0)
            );
            assert_eq!(
                round_to(Format::B64, &Rat::zero(), true),
                Rounded::Finite(0x8000_0000_0000_0000)
            );
        }

        #[test]
        fn every_binary16_pattern_decodes_and_rounds_back_to_itself() {
            for bits in 0u64..=0xffff {
                if let FloatValue::Finite { negative, value } = decode(Format::B16, bits) {
                    assert_eq!(
                        round_to(Format::B16, &value, negative),
                        Rounded::Finite(bits),
                        "{bits:#x}"
                    );
                }
            }
        }

        #[test]
        fn the_printer_breaks_exact_ties_to_even() {
            // 2^50 + 0.25 prints "1125899906842624.2" (both .2 and .3 read
            // back); 52401.3125 at binary32 prints "52401.312".
            let x = Rat::pow2(50).add(&Rat::pow2(-2));
            let Rounded::Finite(bits) = round_to(Format::B64, &x, false) else {
                return;
            };
            let d = shortest(Format::B64, bits)
                .unwrap_or_else(|| Dec::normalized(false, &Nat::zero(), 0));
            assert_eq!(d.to_scientific(), "1.1258999068426242e15");
            let y = rat("838421/16");
            let Rounded::Finite(b32) = round_to(Format::B32, &y, false) else {
                return;
            };
            let d = shortest(Format::B32, b32)
                .unwrap_or_else(|| Dec::normalized(false, &Nat::zero(), 0));
            assert_eq!(d.to_scientific(), "5.2401312e4");
            // The binary32 nearest 0.1 prints "0.1".
            let d = shortest(Format::B32, 0x3dcc_cccd)
                .unwrap_or_else(|| Dec::normalized(false, &Nat::zero(), 0));
            assert_eq!(d.to_scientific(), "1e-1");
        }

        #[test]
        fn nan_narrowing_decides_on_the_low_fraction_bits() {
            assert_eq!(
                narrow_bits(Format::B32, 0x7ff0_0000_2000_0000),
                Some(0x7f80_0001)
            );
            assert_eq!(narrow_bits(Format::B32, 0x7ff8_0000_0000_1234), None);
            assert_eq!(
                narrow_bits(Format::B32, NEGATIVE_CANONICAL_NAN_BITS),
                Some(0xffc0_0000)
            );
            assert_eq!(widen_bits(Format::B32, 0x7f80_0001), 0x7ff0_0000_2000_0000);
            assert_eq!(widen_bits(Format::B16, 0x7e00), CANONICAL_NAN_BITS);
            assert_eq!(narrow_bits(Format::B16, 0x3fb9_9999_9999_999a), None);
        }
    }
}

pub mod temporal {
    //! Dates, times and date-times with every written form kept, and their
    //! partial chronological order.
    //!
    //! A value records exactly what was written: which fields are present (a
    //! year, month and day; an hour, minute and second; a fractional second of
    //! any length), the separator between a date and a time, the written case
    //! of `Z`, the written form of a numeric offset and a written `-` on year
    //! zero. Values that differ only in written forms are distinct values that
    //! denote one moment, and [`cmp_chronological`] compares them `Equal`.

    use std::cmp::Ordering;
    use std::fmt;

    /// The case of a written `Z` offset, or none written (a YAML timestamp with
    /// no zone reads as UTC with its zone unwritten).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum ZuluCase {
        /// `Z`.
        Upper,
        /// `z`.
        Lower,
        /// No zone was written; the value is read as UTC.
        Unwritten,
    }

    /// The written form of a numeric offset.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum OffsetForm {
        /// `+hh:mm`.
        Extended,
        /// `+hhmm`.
        Basic,
        /// `+hh`.
        HourOnly,
        /// `+hh:mm:00`; a written seconds field whose value is zero.
        ExtendedSeconds,
        /// No offset text was written, though the value carries this offset.
        Unwritten,
    }

    /// The offset a value was written with.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum Offset {
        /// No offset: a local value, incomparable with a zoned one.
        Local,
        /// `Z` or `z` (or an unwritten UTC zone).
        Zulu(ZuluCase),
        /// A numeric offset in minutes east of UTC, with its written form; zero
        /// minutes is `+00:00` (or `+0000`, `+00`).
        Fixed(i16, OffsetForm),
        /// A numeric offset in seconds east of UTC whose seconds component is
        /// nonzero, written as `+hh:mm:ss` or `-hh:mm:ss`.
        FixedSeconds(i32),
        /// `-00:00` (or `-0000`, `-00`): UTC known, local offset unknown.
        NegZero(OffsetForm),
    }

    impl Offset {
        /// Minutes east of UTC; `None` for a local value.
        #[must_use]
        pub const fn minutes(self) -> Option<i64> {
            match self {
                Self::Local => None,
                Self::Zulu(_) | Self::NegZero(_) => Some(0),
                Self::Fixed(m, _) => Some(m as i64),
                Self::FixedSeconds(seconds) => Some(seconds as i64 / 60),
            }
        }

        /// Seconds east of UTC; `None` for a local value.
        #[must_use]
        pub const fn seconds(self) -> Option<i64> {
            match self {
                Self::Local => None,
                Self::Zulu(_) | Self::NegZero(_) => Some(0),
                Self::Fixed(m, _) => Some(m as i64 * 60),
                Self::FixedSeconds(seconds) => Some(seconds as i64),
            }
        }

        /// Whether the value is zoned (carries an offset).
        #[must_use]
        pub const fn is_zoned(self) -> bool {
            !matches!(self, Self::Local)
        }
    }

    /// The year as written: its value and whether `-` was written on zero.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct Year {
        /// The year (year 0 is 1 BCE, proleptic Gregorian).
        pub value: i64,
        /// Whether the lexeme was `-0000` (a sign its value does not fix).
        pub negative_zero: bool,
        /// The number of digits written (at least four).
        pub width: usize,
    }

    /// The date fields present.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct DateFields {
        /// The year, absent in a front-truncated date.
        pub year: Option<Year>,
        /// The month, absent in a year or a day-only date.
        pub month: Option<u8>,
        /// The day, absent in a year or year-month.
        pub day: Option<u8>,
    }

    /// The time fields present.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct TimeFields {
        /// The hour, absent in a front-truncated time.
        pub hour: Option<u8>,
        /// The minute, absent in an hour-only or second-only time.
        pub minute: Option<u8>,
        /// The second (60 for a leap second), absent at reduced precision.
        pub second: Option<u8>,
        /// The fractional-second digits as written (empty when none).
        pub fraction: String,
    }

    /// The separator written between a date and a time.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum Separator {
        /// `T`.
        Upper,
        /// `t`.
        Lower,
        /// A space.
        Space,
    }

    /// The declared form used to parse or classify a temporal value.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum TemporalForm {
        /// A date form.
        Date,
        /// A time form.
        Time,
        /// A date-time form.
        DateTime,
    }

    /// The exact lexical descriptor used by the display/parse laws.
    ///
    /// A carrier kind alone is insufficient: `1973-06` can be a local
    /// year-month or the year 1973 with an hour-only `-06` offset. This
    /// descriptor records the field shape and every written-form choice that
    /// selects one of those values. The exact offset value is retained so an
    /// unwritten offset can round-trip even though its magnitude is absent
    /// from the lexeme.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct TemporalLexicalForm {
        /// Date, time, or date-time carrier.
        pub kind: TemporalForm,
        /// Presence of `(year, month, day)`, or no date part.
        pub date_shape: Option<(bool, bool, bool)>,
        /// Presence of `(hour, minute, second)`, or no time part.
        pub time_shape: Option<(bool, bool, bool)>,
        /// The date/time separator, if any.
        pub separator: Option<Separator>,
        /// The exact written or unwritten offset representation.
        pub offset: Offset,
        /// The infinity sign, if this descriptor is for an infinity.
        pub infinity: Option<TemporalInfinity>,
    }

    /// A temporal infinity, ordered outside every finite value of its form.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum TemporalInfinity {
        /// Negative infinity.
        Negative,
        /// Positive infinity.
        Positive,
    }

    /// A temporal value: a date, a time, or both, and an offset.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct Temporal {
        /// The date part.
        pub date: Option<DateFields>,
        /// The time part.
        pub time: Option<TimeFields>,
        /// The separator, present exactly when both parts are.
        pub separator: Option<Separator>,
        /// The offset.
        pub offset: Offset,
        /// A date or date-time infinity; finite values carry `None`.
        pub infinity: Option<(TemporalInfinity, TemporalForm)>,
    }

    /// Which fields a value has: the comparability classes of the order.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct Shape {
        /// Date fields present (year, month, day).
        pub date: Option<(bool, bool, bool)>,
        /// Time fields present (hour, minute, second).
        pub time: Option<(bool, bool, bool)>,
        /// The form of an infinity, absent for a finite value.
        pub infinity: Option<TemporalForm>,
    }

    impl Temporal {
        /// The shape: which fields are present.
        #[must_use]
        pub fn shape(&self) -> Shape {
            Shape {
                date: self
                    .date
                    .map(|d| (d.year.is_some(), d.month.is_some(), d.day.is_some())),
                time: self
                    .time
                    .as_ref()
                    .map(|t| (t.hour.is_some(), t.minute.is_some(), t.second.is_some())),
                infinity: self.infinity.map(|(_, form)| form),
            }
        }

        /// The declared form of this value.
        #[must_use]
        pub const fn form(&self) -> Option<TemporalForm> {
            if let Some((_, form)) = self.infinity {
                return Some(form);
            }
            match (self.date.is_some(), self.time.is_some()) {
                (true, false) => Some(TemporalForm::Date),
                (false, true) => Some(TemporalForm::Time),
                (true, true) => Some(TemporalForm::DateTime),
                (false, false) => None,
            }
        }

        /// The exact descriptor needed to parse this value's displayed
        /// lexeme without choosing between colliding standard forms.
        #[must_use]
        pub fn lexical_form(&self) -> Option<TemporalLexicalForm> {
            if !self.well_formed() {
                return None;
            }
            Some(TemporalLexicalForm {
                kind: self.form()?,
                date_shape: self.date.map(|date| {
                    (
                        date.year.is_some(),
                        date.month.is_some(),
                        date.day.is_some(),
                    )
                }),
                time_shape: self.time.as_ref().map(|time| {
                    (
                        time.hour.is_some(),
                        time.minute.is_some(),
                        time.second.is_some(),
                    )
                }),
                separator: self.separator,
                offset: self.offset,
                infinity: self.infinity.map(|(sign, _)| sign),
            })
        }

        /// Construct a date or date-time infinity with its local or carried
        /// offset. Time-only infinities are outside the carrier.
        #[must_use]
        pub fn infinite(
            sign: TemporalInfinity,
            form: TemporalForm,
            offset: Offset,
        ) -> Option<Self> {
            (form != TemporalForm::Time).then_some(Self {
                date: None,
                time: None,
                separator: None,
                offset,
                infinity: Some((sign, form)),
            })
        }

        /// Whether the public fields denote one admitted finite form or one
        /// admitted date/date-time infinity.
        #[must_use]
        pub fn well_formed(&self) -> bool {
            let offset_ok = match self.offset {
                Offset::Local | Offset::Zulu(_) => true,
                Offset::Fixed(minutes, form) => {
                    minutes.unsigned_abs() < 24 * 60
                        && (form != OffsetForm::HourOnly || minutes % 60 == 0)
                }
                Offset::FixedSeconds(seconds) => {
                    seconds.unsigned_abs() < 24 * 60 * 60 && seconds % 60 != 0
                }
                Offset::NegZero(form) => form != OffsetForm::Unwritten,
            };
            if !offset_ok {
                return false;
            }
            if let Some((_, form)) = self.infinity {
                return form != TemporalForm::Time
                    && self.date.is_none()
                    && self.time.is_none()
                    && self.separator.is_none();
            }
            let Some(form) = self.form() else {
                return false;
            };
            if self.separator.is_some() != (form == TemporalForm::DateTime) {
                return false;
            }
            if let Some(date) = self.date {
                let shape = (
                    date.year.is_some(),
                    date.month.is_some(),
                    date.day.is_some(),
                );
                if !matches!(
                    shape,
                    (true, false | true, false)
                        | (true | false, true, true)
                        | (false, true, false)
                        | (false, false, true)
                ) || !valid_date(&date)
                {
                    return false;
                }
                if let Some(year) = date.year {
                    let width = year.value.unsigned_abs().to_string().len().max(4);
                    if year.width != width || (year.negative_zero && year.value != 0) {
                        return false;
                    }
                }
            }
            if let Some(time) = &self.time {
                let shape = (
                    time.hour.is_some(),
                    time.minute.is_some(),
                    time.second.is_some(),
                );
                if !matches!(
                    shape,
                    (true, false | true, false)
                        | (true | false, true, true)
                        | (false, true, false)
                        | (false, false, true)
                ) || time.hour.is_some_and(|hour| hour > 23)
                    || time.minute.is_some_and(|minute| minute > 59)
                    || time.second.is_some_and(|second| second > 60)
                    || (!time.fraction.is_empty()
                        && (time.second.is_none()
                            || !time.fraction.bytes().all(|byte| byte.is_ascii_digit())))
                {
                    return false;
                }
            }
            form != TemporalForm::DateTime
                || (self.date.is_some_and(|date| {
                    let full = date.year.is_some() && date.month.is_some() && date.day.is_some();
                    let day_only =
                        date.year.is_none() && date.month.is_none() && date.day.is_some();
                    full || day_only
                }) && self.time.as_ref().is_some_and(|time| time.hour.is_some()))
        }

        /// Whether the shape is front-truncated (a date without its year, or a
        /// time-only value without its hour).
        #[must_use]
        pub fn is_truncated(&self) -> bool {
            self.infinity.is_none()
                && (self.date.is_some_and(|d| d.year.is_none())
                    || (self.date.is_none()
                        && self.time.as_ref().is_some_and(|t| t.hour.is_none())))
        }

        /// Whether this is one of the complete forms: a full date, a full time,
        /// or a full date and full time.
        #[must_use]
        pub fn is_complete(&self) -> bool {
            let full_date = self
                .date
                .is_none_or(|d| d.year.is_some() && d.month.is_some() && d.day.is_some());
            let full_time = self
                .time
                .as_ref()
                .is_none_or(|t| t.hour.is_some() && t.minute.is_some() && t.second.is_some());
            self.infinity.is_none()
                && full_date
                && full_time
                && (self.date.is_some() || self.time.is_some())
        }
    }

    /// Ceiling division for a positive divisor, exact for negative dividends.
    const fn div_ceil(value: i128, divisor: i128) -> i128 {
        let quotient = value.div_euclid(divisor);
        if value.rem_euclid(divisor) == 0 {
            quotient
        } else {
            quotient + 1
        }
    }

    /// Days from 0000-03-01 style counting avoided: the number of days before
    /// January 1 of year `y` since January 1 of year 0, proleptic Gregorian,
    /// from the leap rule (divisible by 4, except by 100 unless by 400).
    fn days_before_year(y: i64) -> i128 {
        // Leap years in [0, y): ceil(y/4) - ceil(y/100) + ceil(y/400), which for
        // negative y counts backwards; div_euclid keeps it exact both ways.
        let y = i128::from(y);
        let leaps = |k| div_ceil(y, k);
        365 * y + leaps(4) - leaps(100) + leaps(400)
    }

    /// Whether `y` is a leap year.
    #[must_use]
    pub const fn is_leap(y: i64) -> bool {
        (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
    }

    /// The number of days of month `m` (1 to 12) in year `y`.
    #[must_use]
    pub const fn month_length(y: i64, m: u8) -> u8 {
        match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if is_leap(y) {
                    29
                } else {
                    28
                }
            }
            _ => 0,
        }
    }

    /// The day number of a full date since January 1 of year 0.
    #[must_use]
    pub fn day_number(y: i64, m: u8, d: u8) -> i128 {
        let months: i128 = (1..m).map(|k| i128::from(month_length(y, k))).sum();
        days_before_year(y) + months + i128::from(d) - 1
    }

    /// The key a value is ordered by within its comparability class.
    ///
    /// For a value with a year: twice the second index of its first moment,
    /// with a leap second placed on the odd point between second 59 and the
    /// next minute, less twice its offset in seconds. Missing fields take
    /// their least value. For a
    /// value without a year that is not truncated (a time): the same with no
    /// days. For a truncated value: its present fields, most significant first.
    fn order_key(v: &Temporal) -> Option<(Vec<i128>, String)> {
        let fraction = v
            .time
            .as_ref()
            .map(|t| t.fraction.clone())
            .unwrap_or_default();
        let second = v.time.as_ref().and_then(|t| t.second).map_or(0, i128::from);
        if v.is_truncated() {
            let mut fields = Vec::new();
            if let Some(d) = v.date {
                fields.extend(d.month.map(i128::from));
                fields.extend(d.day.map(i128::from));
            }
            if let Some(t) = &v.time {
                fields.extend(t.hour.map(i128::from));
                fields.extend(t.minute.map(i128::from));
                fields.extend(t.second.map(i128::from));
            }
            return Some((fields, fraction));
        }
        let days = v.date.and_then(|d| {
            d.year
                .map(|y| day_number(y.value, d.month.unwrap_or(1), d.day.unwrap_or(1)))
        });
        let hour = v.time.as_ref().and_then(|t| t.hour).map_or(0, i128::from);
        let minute = v.time.as_ref().and_then(|t| t.minute).map_or(0, i128::from);
        let shift = i128::from(v.offset.seconds().unwrap_or(0));
        let minute_index = days
            .unwrap_or(0)
            .checked_mul(1440)?
            .checked_add(hour.checked_mul(60)?)?
            .checked_add(minute)?;
        let ordinary_second = second.min(59);
        let seconds_twice = minute_index
            .checked_mul(120)?
            .checked_add(ordinary_second.checked_mul(2)?)?
            .checked_add(i128::from(second == 60))?
            .checked_sub(shift.checked_mul(2)?)?;
        Some((vec![seconds_twice], fraction))
    }

    /// Compare two fractional-second digit strings as decimal fractions: the
    /// shorter is padded with zeros, then the digit strings compare as
    /// integers over the common power of ten.
    #[must_use]
    pub fn cmp_fraction(a: &str, b: &str) -> Ordering {
        let n = a.len().max(b.len());
        format!("{a:0<n$}").cmp(&format!("{b:0<n$}"))
    }

    /// Whether two values are comparable: one shape, both local or both zoned,
    /// and, for a truncated shape, offsets of equal minutes.
    #[must_use]
    pub fn comparable(a: &Temporal, b: &Temporal) -> bool {
        let same_shape = if a.infinity.is_some() || b.infinity.is_some() {
            a.form() == b.form()
        } else {
            a.shape() == b.shape()
        };
        same_shape
            && a.offset.is_zoned() == b.offset.is_zoned()
            && (!a.is_truncated() || a.offset.minutes() == b.offset.minutes())
    }

    /// Definition: The partial chronological order: `None` for incomparable values, else
    /// the order of `order_key` with the fraction compared as a decimal
    /// fraction.
    ///
    /// Book anchors: `L-values-12`, `L-values-17`, `L-values-18`, `L-values-19`, `L-values-20`.
    #[must_use]
    pub fn cmp_chronological(a: &Temporal, b: &Temporal) -> Option<Ordering> {
        if !comparable(a, b) {
            return None;
        }
        match (a.infinity, b.infinity) {
            (Some((left, _)), Some((right, _))) => return Some(left.cmp(&right)),
            (Some((TemporalInfinity::Negative, _)), None)
            | (None, Some((TemporalInfinity::Positive, _))) => return Some(Ordering::Less),
            (Some((TemporalInfinity::Positive, _)), None)
            | (None, Some((TemporalInfinity::Negative, _))) => return Some(Ordering::Greater),
            (None, None) => {}
        }
        let (ka, fa) = order_key(a)?;
        let (kb, fb) = order_key(b)?;
        Some(ka.cmp(&kb).then_with(|| cmp_fraction(&fa, &fb)))
    }

    /// Backward-compatible name for the form-indexed parser discriminant.
    pub type TemporalKind = TemporalForm;

    /// Why the kind-union parser refused a temporal lexeme.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TemporalParseError {
        /// No form accepts the lexeme.
        InvalidTemporal,
        /// More than one form accepts the lexeme, so guessing would discard
        /// a standard value.
        AmbiguousTemporal,
    }

    fn digits_n(s: &str, n: usize) -> Option<u8> {
        if s.len() == n && s.bytes().all(|b| b.is_ascii_digit()) {
            s.parse().ok()
        } else {
            None
        }
    }

    /// A year: `'-'? ([1-9] d d d+ | '0' d d d)` (XML Schema `yearFrag`).
    fn parse_year(s: &str) -> Option<Year> {
        let (neg, body) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
        let bytes = body.as_bytes();
        let ok = bytes.iter().all(u8::is_ascii_digit)
            && ((bytes.len() == 4) || (bytes.len() > 4 && bytes[0] != b'0'));
        if !ok {
            return None;
        }
        let magnitude: u64 = body.parse().ok()?;
        let value = if neg {
            i64::try_from(-i128::from(magnitude)).ok()?
        } else {
            i64::try_from(magnitude).ok()?
        };
        Some(Year {
            value,
            negative_zero: neg && magnitude == 0,
            width: body.len(),
        })
    }

    fn valid_date(d: &DateFields) -> bool {
        let month_ok = d.month.is_none_or(|m| (1..=12).contains(&m));
        let day_ok = d.day.is_none_or(|day| {
            let limit = match (d.year, d.month) {
                (Some(y), Some(m)) => month_length(y.value, m),
                (None, Some(m)) => month_length(0, m),
                _ => 31,
            };
            day >= 1 && day <= limit
        });
        month_ok && day_ok
    }

    fn parse_date_body(s: &str) -> Option<DateFields> {
        let d = if let Some(rest) = s.strip_prefix("---") {
            DateFields {
                year: None,
                month: None,
                day: Some(digits_n(rest, 2)?),
            }
        } else if let Some(rest) = s.strip_prefix("--") {
            match rest.split_once('-') {
                Some((m, d)) => DateFields {
                    year: None,
                    month: Some(digits_n(m, 2)?),
                    day: Some(digits_n(d, 2)?),
                },
                None => DateFields {
                    year: None,
                    month: Some(digits_n(rest, 2)?),
                    day: None,
                },
            }
        } else {
            let (sign, body) = s.strip_prefix('-').map_or(("", s), |r| ("-", r));
            let mut parts = body.splitn(3, '-');
            let year = parse_year(&format!("{sign}{}", parts.next()?))?;
            let month = parts
                .next()
                .map(|m| digits_n(m, 2))
                .map_or(Some(None), |m| m.map(Some))?;
            let day = parts
                .next()
                .map(|d| digits_n(d, 2))
                .map_or(Some(None), |d| d.map(Some))?;
            DateFields {
                year: Some(year),
                month,
                day,
            }
        };
        valid_date(&d).then_some(d)
    }

    fn parse_seconds(s: &str) -> Option<(u8, String)> {
        let (sec, frac) = s.split_once('.').map_or((s, None), |(a, b)| (a, Some(b)));
        let second = digits_n(sec, 2)?;
        let fraction = match frac {
            None => String::new(),
            Some(f) if !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()) => f.to_owned(),
            Some(_) => return None,
        };
        (second <= 60).then_some((second, fraction))
    }

    fn parse_time_body(s: &str) -> Option<TimeFields> {
        let t = if let Some(rest) = s.strip_prefix("--") {
            let (second, fraction) = parse_seconds(rest)?;
            TimeFields {
                hour: None,
                minute: None,
                second: Some(second),
                fraction,
            }
        } else if let Some(rest) = s.strip_prefix('-') {
            match rest.split_once(':') {
                Some((m, sec)) => {
                    let (second, fraction) = parse_seconds(sec)?;
                    TimeFields {
                        hour: None,
                        minute: Some(digits_n(m, 2)?),
                        second: Some(second),
                        fraction,
                    }
                }
                None => TimeFields {
                    hour: None,
                    minute: Some(digits_n(rest, 2)?),
                    second: None,
                    fraction: String::new(),
                },
            }
        } else {
            let mut parts = s.splitn(3, ':');
            let hour = digits_n(parts.next()?, 2)?;
            let minute = parts
                .next()
                .map(|m| digits_n(m, 2))
                .map_or(Some(None), |m| m.map(Some))?;
            let (second, fraction) = match parts.next() {
                None => (None, String::new()),
                Some(sec) => {
                    let (second, fraction) = parse_seconds(sec)?;
                    (Some(second), fraction)
                }
            };
            TimeFields {
                hour: Some(hour),
                minute,
                second,
                fraction,
            }
        };
        let ok = t.hour.is_none_or(|h| h <= 23) && t.minute.is_none_or(|m| m <= 59);
        ok.then_some(t)
    }

    fn parse_offset(s: &str) -> Option<Offset> {
        match s {
            "" => return Some(Offset::Local),
            "Z" => return Some(Offset::Zulu(ZuluCase::Upper)),
            "z" => return Some(Offset::Zulu(ZuluCase::Lower)),
            _ => {}
        }
        let (negative, body) = match s.as_bytes().first() {
            Some(b'+') => (false, &s[1..]),
            Some(b'-') => (true, &s[1..]),
            _ => return None,
        };
        let pieces: Vec<&str> = body.split(':').collect();
        if pieces.len() == 3 {
            let (h, m, second) = (
                digits_n(pieces[0], 2)?,
                digits_n(pieces[1], 2)?,
                digits_n(pieces[2], 2)?,
            );
            if h > 23 || m > 59 || second > 59 {
                return None;
            }
            let minutes = i16::from(h) * 60 + i16::from(m);
            if second == 0 {
                return Some(if minutes == 0 && negative {
                    Offset::NegZero(OffsetForm::ExtendedSeconds)
                } else {
                    Offset::Fixed(
                        if negative { -minutes } else { minutes },
                        OffsetForm::ExtendedSeconds,
                    )
                });
            }
            let seconds = i32::from(minutes) * 60 + i32::from(second);
            return Some(Offset::FixedSeconds(if negative {
                -seconds
            } else {
                seconds
            }));
        }
        let (h, m, form) = if pieces.len() == 2 {
            (
                digits_n(pieces[0], 2)?,
                digits_n(pieces[1], 2)?,
                OffsetForm::Extended,
            )
        } else if body.len() == 4 {
            (
                digits_n(&body[..2], 2)?,
                digits_n(&body[2..], 2)?,
                OffsetForm::Basic,
            )
        } else {
            (digits_n(body, 2)?, 0, OffsetForm::HourOnly)
        };
        if h > 23 || m > 59 {
            return None;
        }
        let minutes = i16::from(h) * 60 + i16::from(m);
        Some(if minutes == 0 && negative {
            Offset::NegZero(form)
        } else {
            Offset::Fixed(if negative { -minutes } else { minutes }, form)
        })
    }

    /// Every split of `s` into a body and a trailing offset lexeme (possibly
    /// empty) that both parse.
    fn splits<T>(s: &str, body: impl Fn(&str) -> Option<T>) -> Vec<(T, Offset)> {
        let mut out = Vec::new();
        for cut in (0..=s.len()).rev() {
            if !s.is_char_boundary(cut) {
                continue;
            }
            if let (Some(b), Some(o)) = (body(&s[..cut]), parse_offset(&s[cut..])) {
                out.push((b, o));
            }
        }
        out
    }

    fn parse_candidates(form: TemporalForm, s: &str) -> Vec<Temporal> {
        let infinity = match s {
            "-infinity" => Some(TemporalInfinity::Negative),
            "infinity" => Some(TemporalInfinity::Positive),
            _ => None,
        };
        if let Some(sign) = infinity {
            return Temporal::infinite(sign, form, Offset::Local)
                .into_iter()
                .collect();
        }
        match form {
            TemporalForm::Date => splits(s, parse_date_body)
                .into_iter()
                .map(|(date, offset)| Temporal {
                    date: Some(date),
                    time: None,
                    separator: None,
                    offset,
                    infinity: None,
                })
                .collect(),
            TemporalForm::Time => splits(s, parse_time_body)
                .into_iter()
                .map(|(time, offset)| Temporal {
                    date: None,
                    time: Some(time),
                    separator: None,
                    offset,
                    infinity: None,
                })
                .collect(),
            TemporalForm::DateTime => splits(s, |body| {
                let at = body.find(['T', 't', ' '])?;
                let separator = match &body[at..=at] {
                    "T" => Separator::Upper,
                    "t" => Separator::Lower,
                    _ => Separator::Space,
                };
                let date = parse_date_body(&body[..at])?;
                let full = date.year.is_some() && date.month.is_some() && date.day.is_some();
                let day_only = date.year.is_none() && date.month.is_none() && date.day.is_some();
                let time = parse_time_body(&body[at + 1..])?;
                (time.hour.is_some() && (full || day_only)).then_some((date, separator, time))
            })
            .into_iter()
            .map(|((date, separator, time), offset)| Temporal {
                date: Some(date),
                time: Some(time),
                separator: Some(separator),
                offset,
                infinity: None,
            })
            .collect(),
        }
    }

    /// Read a lexeme using its exact lexical descriptor.
    ///
    /// Definition: among all standard parses of `text`, return the unique
    /// value with descriptor `form`. An unwritten offset is supplied by the
    /// descriptor because its magnitude has no lexical representation.
    ///
    /// Book anchors: `L-values-15`, `L-values-16`.
    #[must_use]
    pub fn parse_as(form: TemporalLexicalForm, text: &str) -> Option<Temporal> {
        if let Some(sign) = form.infinity {
            let spelling = match sign {
                TemporalInfinity::Negative => "-infinity",
                TemporalInfinity::Positive => "infinity",
            };
            return (text == spelling)
                .then(|| Temporal::infinite(sign, form.kind, form.offset))
                .flatten()
                .filter(|value| value.lexical_form() == Some(form));
        }
        let unwritten = matches!(
            form.offset,
            Offset::Zulu(ZuluCase::Unwritten) | Offset::Fixed(_, OffsetForm::Unwritten)
        );
        let mut matches = parse_candidates(form.kind, text)
            .into_iter()
            .filter_map(|mut value| {
                if unwritten && value.offset == Offset::Local {
                    value.offset = form.offset;
                }
                (value.lexical_form() == Some(form)).then_some(value)
            });
        let value = matches.next()?;
        matches.next().is_none().then_some(value)
    }

    /// Compatibility parser for a declared carrier kind.
    ///
    /// If the body/offset split is ambiguous within the kind, this preserves
    /// the historical longest-body policy. Exact round trips use [`parse_as`]
    /// with [`Temporal::lexical_form`] instead.
    #[must_use]
    pub fn parse(kind: TemporalKind, s: &str) -> Option<Temporal> {
        parse_candidates(kind, s).into_iter().next()
    }

    impl Temporal {
        /// Read `text` under an exact lexical descriptor.
        #[must_use]
        pub fn parse_as(form: TemporalLexicalForm, text: &str) -> Option<Self> {
            parse_as(form, text)
        }
    }

    /// Read a lexeme with no descriptor, refusing ambiguity both across
    /// carrier kinds and between body/offset splits within one kind.
    ///
    /// # Errors
    ///
    /// Returns [`TemporalParseError::InvalidTemporal`] when no form accepts
    /// the lexeme and [`TemporalParseError::AmbiguousTemporal`] when more than
    /// one distinct value accepts it.
    pub fn parse_union(s: &str) -> Result<Temporal, TemporalParseError> {
        let mut all = Vec::new();
        for kind in [
            TemporalForm::Date,
            TemporalForm::Time,
            TemporalForm::DateTime,
        ] {
            for value in parse_candidates(kind, s) {
                if !all.contains(&value) {
                    all.push(value);
                }
            }
        }
        match all.as_slice() {
            [one] => Ok(one.clone()),
            [] => Err(TemporalParseError::InvalidTemporal),
            _ => Err(TemporalParseError::AmbiguousTemporal),
        }
    }

    /// Read a lexeme with no declared kind: exactly one kind must read it.
    #[must_use]
    pub fn parse_any(s: &str) -> Option<Temporal> {
        parse_union(s).ok()
    }

    /// Definition: The parse-then-display law for an exact-descriptor accepted lexeme.
    ///
    /// Book anchors: `L-values-15`.
    #[must_use]
    pub fn parse_display(form: TemporalLexicalForm, text: &str) -> bool {
        parse_as(form, text).is_some_and(|value| value.to_string() == text)
    }

    /// Definition: The display-then-parse law under the value's exact lexical
    /// descriptor.
    ///
    /// Book anchors: `L-values-16`.
    #[must_use]
    pub fn display_parse(value: &Temporal) -> bool {
        value
            .lexical_form()
            .is_some_and(|form| parse_as(form, &value.to_string()).as_ref() == Some(value))
    }

    /// Definition: Canonical bytes of a temporal value.
    ///
    /// Book anchors: `L-values-21`.
    #[must_use]
    pub fn canonical_bytes(value: &Temporal) -> Option<Vec<u8>> {
        crate::canonical::value_bytes(&crate::value::Value::Temporal(value.clone()))
    }

    /// Definition: Whether a value records YAML's unwritten-UTC representation.
    ///
    /// Book anchors: `L-values-39`.
    #[must_use]
    pub const fn unwritten_zulu(value: &Temporal) -> bool {
        matches!(value.offset, Offset::Zulu(ZuluCase::Unwritten))
    }

    /// Unit of an integral epoch count.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum TimeUnit {
        /// Seconds.
        Seconds,
        /// Milliseconds.
        Millis,
        /// Microseconds.
        Micros,
        /// Nanoseconds.
        Nanos,
    }

    impl TimeUnit {
        const fn digits(self) -> u32 {
            match self {
                Self::Seconds => 0,
                Self::Millis => 3,
                Self::Micros => 6,
                Self::Nanos => 9,
            }
        }

        const fn ticks(self) -> i128 {
            10i128.pow(self.digits())
        }
    }

    fn days_before_year_wide(year: i128) -> Option<i128> {
        let leaps = |divisor| div_ceil(year, divisor);
        year.checked_mul(365)?
            .checked_add(leaps(4))?
            .checked_sub(leaps(100))?
            .checked_add(leaps(400))
    }

    fn date_from_day(day: i128) -> Option<(i64, u8, u8)> {
        let cycle = day.div_euclid(146_097);
        let mut year = cycle.checked_mul(400)?;
        while days_before_year_wide(year.checked_add(1)?)? <= day {
            year = year.checked_add(1)?;
        }
        while days_before_year_wide(year)? > day {
            year = year.checked_sub(1)?;
        }
        let year_i64 = i64::try_from(year).ok()?;
        let mut within = day.checked_sub(days_before_year_wide(year)?)?;
        let mut month = 1u8;
        while month <= 12 {
            let length = i128::from(month_length(year_i64, month));
            if within < length {
                return Some((year_i64, month, u8::try_from(within + 1).ok()?));
            }
            within -= length;
            month += 1;
        }
        None
    }

    /// Definition: Convert an epoch count to the exact date-time in its unit.
    ///
    /// The epoch is 1970-01-01T00:00:00. An instant column writes `Z`; a
    /// wall-clock column writes no offset.
    ///
    /// Book anchors: `L-values-28`.
    #[must_use]
    pub fn from_epoch_count(count: i128, unit: TimeUnit, instant: bool) -> Option<Temporal> {
        let ticks = unit.ticks();
        let seconds = count.div_euclid(ticks);
        let fraction = count.rem_euclid(ticks);
        let days = seconds.div_euclid(86_400);
        let in_day = seconds.rem_euclid(86_400);
        let epoch = day_number(1970, 1, 1);
        let (year, month, day) = date_from_day(epoch.checked_add(days)?)?;
        let width = usize::try_from(unit.digits()).ok()?;
        let year_width = year.unsigned_abs().to_string().len().max(4);
        Some(Temporal {
            date: Some(DateFields {
                year: Some(Year {
                    value: year,
                    negative_zero: false,
                    width: year_width,
                }),
                month: Some(month),
                day: Some(day),
            }),
            time: Some(TimeFields {
                hour: Some(u8::try_from(in_day / 3_600).ok()?),
                minute: Some(u8::try_from(in_day % 3_600 / 60).ok()?),
                second: Some(u8::try_from(in_day % 60).ok()?),
                fraction: if width == 0 {
                    String::new()
                } else {
                    format!("{fraction:0width$}")
                },
            }),
            separator: Some(Separator::Upper),
            offset: if instant {
                Offset::Zulu(ZuluCase::Upper)
            } else {
                Offset::Local
            },
            infinity: None,
        })
    }

    /// Definition: Convert a date-time in the image of [`from_epoch_count`] back to its
    /// count.
    ///
    /// Book anchors: `L-values-29`.
    #[must_use]
    pub fn to_epoch_count(value: &Temporal, unit: TimeUnit, instant: bool) -> Option<i128> {
        let date = value.date?;
        let time = value.time.as_ref()?;
        let year = date.year?;
        if year.negative_zero
            || value.separator != Some(Separator::Upper)
            || value.offset
                != if instant {
                    Offset::Zulu(ZuluCase::Upper)
                } else {
                    Offset::Local
                }
        {
            return None;
        }
        let (month, day, hour, minute, second) = (
            date.month?,
            date.day?,
            time.hour?,
            time.minute?,
            time.second?,
        );
        let canonical_width = year.value.unsigned_abs().to_string().len().max(4);
        if year.width != canonical_width
            || !(1..=12).contains(&month)
            || day == 0
            || day > month_length(year.value, month)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return None;
        }
        let digits = usize::try_from(unit.digits()).ok()?;
        if time.fraction.len() != digits || !time.fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        let fraction = if digits == 0 {
            0
        } else {
            time.fraction.parse::<i128>().ok()?
        };
        let epoch = day_number(1970, 1, 1);
        let days = day_number(year.value, month, day).checked_sub(epoch)?;
        let seconds = days
            .checked_mul(86_400)?
            .checked_add(i128::from(hour) * 3_600)?
            .checked_add(i128::from(minute) * 60)?
            .checked_add(i128::from(second))?;
        seconds.checked_mul(unit.ticks())?.checked_add(fraction)
    }

    impl fmt::Display for Offset {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let numeric = |f: &mut fmt::Formatter<'_>, sign: char, m: i64, form: OffsetForm| {
                let (h, mm) = (m / 60, m % 60);
                match form {
                    OffsetForm::Extended => write!(f, "{sign}{h:02}:{mm:02}"),
                    OffsetForm::Basic => write!(f, "{sign}{h:02}{mm:02}"),
                    OffsetForm::HourOnly => write!(f, "{sign}{h:02}"),
                    OffsetForm::ExtendedSeconds => write!(f, "{sign}{h:02}:{mm:02}:00"),
                    OffsetForm::Unwritten => Ok(()),
                }
            };
            match *self {
                Self::Local | Self::Zulu(ZuluCase::Unwritten) => Ok(()),
                Self::Zulu(ZuluCase::Upper) => f.write_str("Z"),
                Self::Zulu(ZuluCase::Lower) => f.write_str("z"),
                Self::NegZero(form) => numeric(f, '-', 0, form),
                Self::Fixed(m, form) => {
                    let sign = if m < 0 { '-' } else { '+' };
                    numeric(f, sign, i64::from(m).abs(), form)
                }
                Self::FixedSeconds(seconds) => {
                    let sign = if seconds < 0 { '-' } else { '+' };
                    let magnitude = i64::from(seconds).abs();
                    write!(
                        f,
                        "{sign}{:02}:{:02}:{:02}",
                        magnitude / 3_600,
                        magnitude % 3_600 / 60,
                        magnitude % 60
                    )
                }
            }
        }
    }

    fn write_date(f: &mut fmt::Formatter<'_>, d: &DateFields) -> fmt::Result {
        match d.year {
            Some(y) => {
                let sign = if y.value < 0 || y.negative_zero {
                    "-"
                } else {
                    ""
                };
                write!(f, "{sign}{:0w$}", y.value.unsigned_abs(), w = y.width)?;
                if let Some(m) = d.month {
                    write!(f, "-{m:02}")?;
                }
                if let Some(day) = d.day {
                    write!(f, "-{day:02}")?;
                }
                Ok(())
            }
            None => match (d.month, d.day) {
                (Some(m), Some(day)) => write!(f, "--{m:02}-{day:02}"),
                (Some(m), None) => write!(f, "--{m:02}"),
                (None, Some(day)) => write!(f, "---{day:02}"),
                (None, None) => Ok(()),
            },
        }
    }

    fn write_time(out: &mut fmt::Formatter<'_>, time: &TimeFields) -> fmt::Result {
        let frac = |out: &mut fmt::Formatter<'_>| {
            if time.fraction.is_empty() {
                Ok(())
            } else {
                write!(out, ".{}", time.fraction)
            }
        };
        match (time.hour, time.minute, time.second) {
            (Some(hour), None, _) => write!(out, "{hour:02}"),
            (Some(hour), Some(minute), None) => write!(out, "{hour:02}:{minute:02}"),
            (Some(hour), Some(minute), Some(second)) => {
                write!(out, "{hour:02}:{minute:02}:{second:02}")?;
                frac(out)
            }
            (None, Some(minute), Some(second)) => {
                write!(out, "-{minute:02}:{second:02}")?;
                frac(out)
            }
            (None, Some(minute), None) => write!(out, "-{minute:02}"),
            (None, None, Some(second)) => {
                write!(out, "--{second:02}")?;
                frac(out)
            }
            (None, None, None) => Ok(()),
        }
    }

    impl fmt::Display for Temporal {
        /// The lexeme back, in every written form the value records.
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if let Some((infinity, _)) = self.infinity {
                return f.write_str(match infinity {
                    TemporalInfinity::Negative => "-infinity",
                    TemporalInfinity::Positive => "infinity",
                });
            }
            if let Some(d) = &self.date {
                write_date(f, d)?;
            }
            if let Some(s) = self.separator {
                f.write_str(match s {
                    Separator::Upper => "T",
                    Separator::Lower => "t",
                    Separator::Space => " ",
                })?;
            }
            if let Some(t) = &self.time {
                write_time(f, t)?;
            }
            write!(f, "{}", self.offset)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn dt(s: &str) -> Option<Temporal> {
            parse(TemporalKind::DateTime, s)
        }

        #[test]
        fn every_written_form_displays_back() {
            for s in [
                "2024-01-02T03:04:05z",
                "2024-01-02t03:04:05Z",
                "2024-01-02 03:04:05Z",
                "2024-01-02T03:04:05.120+05:30",
                "1985-04-12T23",
                "1979-05-27 07:32Z",
                "---15T09:45:00+01:00",
                "2024-01-02T03:04:05-0000",
            ] {
                assert_eq!(dt(s).map(|v| v.to_string()), Some(s.to_owned()), "{s}");
            }
            for s in [
                "2018",
                "-0044",
                "1973-06",
                "--04-12",
                "---12",
                "-0000",
                "2018Z",
                "2002-10-10+13:00",
            ] {
                assert_eq!(
                    parse(TemporalKind::Date, s).map(|v| v.to_string()),
                    Some(s.to_owned()),
                    "{s}"
                );
            }
            for s in [
                "03:04:05z",
                "23",
                "23:20",
                "-20:50",
                "-20",
                "--50",
                "12:00:00.1234567891",
                "23:20+04",
            ] {
                assert_eq!(
                    parse(TemporalKind::Time, s).map(|v| v.to_string()),
                    Some(s.to_owned()),
                    "{s}"
                );
            }
            assert_eq!(parse_any("--04"), None);
            assert!(
                parse(TemporalKind::Date, "--04").is_some()
                    && parse(TemporalKind::Time, "--04").is_some()
            );
            assert_eq!(parse(TemporalKind::Date, "+2018"), None);
            assert_eq!(parse(TemporalKind::Date, "2023-02-29"), None);
        }

        #[test]
        fn written_forms_of_one_moment_compare_equal() {
            let zulu = dt("2024-01-02T03:04:05Z");
            let lower = dt("2024-01-02t03:04:05z");
            let plus = dt("2024-01-02T03:04:05+00:00");
            let negzero = dt("2024-01-02T03:04:05-00:00");
            let shifted = dt("2024-01-02T08:34:05+05:30");
            let Some(base) = zulu else { return };
            for other in [lower, plus, negzero, shifted].into_iter().flatten() {
                assert_eq!(cmp_chronological(&base, &other), Some(Ordering::Equal));
            }
            let local = dt("2024-01-02T03:04:05");
            assert_eq!(
                local.map(|local| cmp_chronological(&base, &local)),
                Some(None)
            );
        }

        #[test]
        fn fractions_compare_as_decimal_fractions_and_a_leap_second_precedes_the_next_minute() {
            let p = |s: &str| parse(TemporalKind::Time, s);
            assert_eq!(
                p("12:00:00.1")
                    .zip(p("12:00:00.10"))
                    .map(|(a, b)| cmp_chronological(&a, &b)),
                Some(Some(Ordering::Equal))
            );
            assert_eq!(
                p("12:00:00.09")
                    .zip(p("12:00:00.1"))
                    .map(|(a, b)| cmp_chronological(&a, &b)),
                Some(Some(Ordering::Less))
            );
            let leap = dt("2016-12-31T23:59:60Z");
            let next = dt("2017-01-01T00:00:00Z");
            let before = dt("2016-12-31T23:59:59.999Z");
            assert_eq!(
                leap.clone()
                    .zip(next)
                    .map(|(a, b)| cmp_chronological(&a, &b)),
                Some(Some(Ordering::Less))
            );
            assert_eq!(
                before.zip(leap).map(|(a, b)| cmp_chronological(&a, &b)),
                Some(Some(Ordering::Less))
            );
            // A year is placed by the UTC instant of its first moment.
            let y = parse(TemporalKind::Date, "2018+05:00");
            let z = parse(TemporalKind::Date, "2017Z");
            assert_eq!(
                y.zip(z).map(|(a, b)| cmp_chronological(&a, &b)),
                Some(Some(Ordering::Greater))
            );
        }
    }
}

pub mod value {
    //! The instance value model: every variant a cell or a node value can hold,
    //! with floats held by their bit patterns so that equality is bit-wise.

    use std::cmp::Ordering;
    use std::collections::BTreeMap;

    use crate::exact::{BigIntValue, Decimal, DecimalValue, ExactInteger, parse_decimal};
    use crate::float::{CANONICAL_NAN_BITS, Format, shortest};
    use crate::num::Int;
    use crate::temporal::Temporal;

    /// An instance value.
    ///
    /// A float is its binary64 bit pattern, so two floats are equal iff their patterns are (a NaN
    /// equals itself and differs from a NaN of another sign or payload; `0.0` differs from `-0.0`),
    /// which is canonical-byte equality.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub enum Value {
        /// A boolean.
        Bool(bool),
        /// An integer within `i64`.
        Int(i64),
        /// A binary64 float, by its bits.
        Float(u64),
        /// A string.
        Str(String),
        /// Bytes.
        Bytes(Vec<u8>),
        /// A content-identifier link.
        CidLink(String),
        /// A blob reference.
        Blob {
            /// Reference.
            ref_: String,
            /// MIME type.
            mime: String,
            /// Size.
            size: u64,
        },
        /// A token.
        Token(String),
        /// An explicit null.
        Null,
        /// An opaque typed value.
        Opaque {
            /// Type identifier.
            type_: String,
            /// Fields.
            fields: BTreeMap<String, Self>,
        },
        /// A record of named values.
        Unknown(BTreeMap<String, Self>),
        /// A list.
        List(Vec<Self>),
        /// A labelled null.
        LabeledNull(u64),
        /// An exact decimal.
        Decimal(Decimal),
        /// An integer outside `i64`.
        BigInt(BigIntValue),
        /// A date, time or date-time.
        Temporal(Temporal),
    }

    impl Value {
        /// The value of an exact integer: `Int` when it fits, else `BigInt`.
        #[must_use]
        pub fn integer(n: ExactInteger) -> Self {
            match n {
                ExactInteger::Int(v) => Self::Int(v),
                ExactInteger::Big(b) => Self::BigInt(b),
            }
        }

        /// Whether a labelled null occurs anywhere inside this value (the value
        /// itself excluded).
        #[must_use]
        pub fn has_nested_labeled_null(&self) -> bool {
            let inner = |v: &Self| matches!(v, Self::LabeledNull(_)) || v.has_nested_labeled_null();
            match self {
                Self::Opaque { fields, .. } | Self::Unknown(fields) => fields.values().any(inner),
                Self::List(items) => items.iter().any(inner),
                _ => false,
            }
        }
    }

    /// Whether a field is present with a value, explicitly null, or absent.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub enum Presence {
        /// Present with a value.
        Present(Value),
        /// Explicitly null.
        Null,
        /// Absent.
        Absent,
    }

    /// Definition: Representation equality: equality of canonical bytes on the canonical
    /// encoder's domain.
    ///
    /// Book anchors: `L-values-1`.
    #[must_use]
    pub fn representation(left: &Value, right: &Value) -> Option<bool> {
        Some(crate::canonical::value_bytes(left)? == crate::canonical::value_bytes(right)?)
    }

    /// Exact-value and numeric operations of the reference expression layer.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum BuiltinOp {
        /// Numeric addition.
        Add,
        /// Numeric subtraction.
        Sub,
        /// Numeric multiplication.
        Mul,
        /// Numeric division.
        Div,
        /// Integer remainder.
        Mod,
        /// Numeric negation.
        Neg,
        /// Numeric absolute value.
        Abs,
        /// Float floor to integer.
        Floor,
        /// Float ceiling to integer.
        Ceil,
        /// Float round-to-nearest, ties to even, to integer.
        Round,
        /// Representation equality.
        Eq,
        /// Representation inequality.
        Ne,
        /// Strict ordering.
        Lt,
        /// Nonstrict ordering.
        Lte,
        /// Strict reverse ordering.
        Gt,
        /// Nonstrict reverse ordering.
        Gte,
        /// Boolean conjunction.
        And,
        /// Boolean disjunction.
        Or,
        /// Boolean negation.
        Not,
        /// String concatenation.
        Concat,
        /// UTF-8 byte length.
        Len,
        /// Byte-indexed, character-boundary-safe string slice.
        Slice,
        /// Unicode uppercase mapping.
        Upper,
        /// Unicode lowercase mapping.
        Lower,
        /// Trim Unicode whitespace.
        Trim,
        /// Split a string.
        Split,
        /// Join strings.
        Join,
        /// Replace string occurrences.
        Replace,
        /// String containment.
        Contains,
        /// Map a function over a list.
        Map,
        /// Keep list items accepted by a predicate.
        Filter,
        /// Fold a list from the left.
        Fold,
        /// Append an item to a list.
        Append,
        /// First list item.
        Head,
        /// All but the first list item.
        Tail,
        /// Reverse a list.
        Reverse,
        /// Map and concatenate list results.
        FlatMap,
        /// List length.
        Length,
        /// Inclusive integer range.
        Range,
        /// Merge records, with right fields winning.
        MergeRecords,
        /// Record keys.
        Keys,
        /// Record values.
        Values,
        /// Record field-membership test.
        HasField,
        /// Null-defaulting.
        DefaultVal,
        /// Numeric clamp.
        Clamp,
        /// Byte-bound, character-safe string truncation.
        TruncateStr,
        /// Integer to float.
        IntToFloat,
        /// Float to integer by truncation.
        FloatToInt,
        /// Integer to string.
        IntToStr,
        /// Float to shortest string.
        FloatToStr,
        /// String to integer.
        StrToInt,
        /// String to float.
        StrToFloat,
        /// Runtime value type name.
        TypeOf,
        /// Null predicate.
        IsNull,
        /// List predicate.
        IsList,
        /// Instance traversal, undefined without an instance context.
        Edge,
        /// Instance traversal, undefined without an instance context.
        Children,
        /// Instance traversal, undefined without an instance context.
        HasEdge,
        /// Instance traversal, undefined without an instance context.
        EdgeCount,
        /// Instance traversal, undefined without an instance context.
        Anchor,
        /// Instance traversal, undefined without an instance context.
        Edges,
        /// Lossless lexical conversion.
        ToString,
        /// Exact decimal parsing/conversion.
        ParseDecimal,
        /// Explicit lossy decimal/integer-to-float conversion.
        DecimalToFloat,
    }

    /// A typed expression refusal.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum EvalError {
        /// A variable was not bound.
        UnboundVariable(String),
        /// An operation received an inadmissible type.
        TypeError {
            /// Expected family.
            expected: &'static str,
            /// Actual type.
            got: &'static str,
        },
        /// An arithmetic operation was applied to an exact value.
        ExactArithmetic {
            /// Operation.
            op: BuiltinOp,
            /// Exact operand type.
            operand: &'static str,
        },
        /// Wrong number of arguments.
        ArityMismatch {
            /// Operation.
            op: BuiltinOp,
            /// Expected arity.
            expected: usize,
            /// Actual arity.
            got: usize,
        },
        /// Integer arithmetic overflowed.
        Overflow,
        /// Division or remainder by zero.
        DivisionByZero,
        /// A string is not a decimal.
        InvalidDecimal(String),
        /// Canonical representation was undefined for an ill-formed carrier.
        UndefinedRepresentation,
        /// List index out of bounds.
        IndexOutOfBounds {
            /// Requested zero-based index.
            index: i64,
            /// List length.
            len: usize,
        },
        /// A record field was absent.
        FieldNotFound(String),
        /// No match arm accepted the scrutinee.
        NonExhaustiveMatch,
        /// Application reached a non-function.
        NotAFunction,
        /// A float is not a finite in-range integer.
        FloatNotRepresentable,
        /// A traversal builtin was evaluated without an instance.
        NoInstanceContext {
            /// Traversal operation requiring an instance.
            op: BuiltinOp,
        },
    }

    /// A destructuring pattern.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Pattern {
        /// Match anything without binding it.
        Wildcard,
        /// Bind the matched value.
        Variable(String),
        /// Match by representation equality.
        Literal(Value),
        /// Match the named fields of a record.
        Record(Vec<(String, Self)>),
        /// Match a list of exactly this length.
        List(Vec<Self>),
        /// Match an opaque constructor and its fields in key order.
        Constructor(String, Vec<Self>),
    }

    /// The small expression language used by reference predicates.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Expr {
        /// A literal.
        Literal(Value),
        /// A variable.
        Variable(String),
        /// Lambda abstraction.
        Lambda(String, Box<Self>),
        /// Function application.
        Apply(Box<Self>, Box<Self>),
        /// Record construction.
        Record(Vec<(String, Self)>),
        /// A list whose items are evaluated left to right.
        List(Vec<Self>),
        /// Record field access.
        Field(Box<Self>, String),
        /// List indexing.
        Index(Box<Self>, Box<Self>),
        /// Pattern matching.
        Match {
            /// Scrutinee.
            scrutinee: Box<Self>,
            /// Arms in priority order.
            arms: Vec<(Pattern, Self)>,
        },
        /// Lexical let binding.
        Let {
            /// Bound name.
            name: String,
            /// Bound expression.
            value: Box<Self>,
            /// Body.
            body: Box<Self>,
        },
        /// Representation equality.
        Equal(Box<Self>, Box<Self>),
        /// A conditional; only a boolean condition is defined.
        If(Box<Self>, Box<Self>, Box<Self>),
        /// A named exact/numeric builtin.
        Builtin(BuiltinOp, Vec<Self>),
    }

    /// Definition: Evaluate an expression in a finite environment.
    ///
    /// Evaluation is exact. Arithmetic accepts only `Int` and `Float`; exact
    /// decimals, big integers and temporals require an explicit conversion and
    /// otherwise return [`EvalError::ExactArithmetic`]. A NaN produced by a
    /// builtin is canonicalized, while a literal, variable, list member or
    /// selected branch passes its stored NaN bits through unchanged.
    ///
    /// Book anchors: `L-values-14`.
    ///
    /// # Errors
    ///
    /// Returns a typed [`EvalError`] for an unbound variable, an ill-typed or
    /// inexact built-in operation, or an operation outside its exact domain.
    #[derive(Clone)]
    enum Runtime {
        Value(Value),
        Closure {
            parameter: String,
            body: Box<Expr>,
            environment: BTreeMap<String, Value>,
        },
    }

    impl Runtime {
        fn value(self) -> Result<Value, EvalError> {
            match self {
                Self::Value(value) => Ok(value),
                Self::Closure { .. } => Err(EvalError::NotAFunction),
            }
        }
    }

    fn apply_runtime(function: Runtime, argument: Value) -> Result<Runtime, EvalError> {
        let Runtime::Closure {
            parameter,
            body,
            mut environment,
        } = function
        else {
            return Err(EvalError::NotAFunction);
        };
        environment.insert(parameter, argument);
        eval_runtime(&body, &environment)
    }

    fn match_pattern(
        pattern: &Pattern,
        value: &Value,
        bindings: &mut BTreeMap<String, Value>,
    ) -> bool {
        match pattern {
            Pattern::Wildcard => true,
            Pattern::Variable(name) => {
                if bindings.contains_key(name) {
                    representation(&bindings[name], value) == Some(true)
                } else {
                    bindings.insert(name.clone(), value.clone());
                    true
                }
            }
            Pattern::Literal(expected) => representation(expected, value) == Some(true),
            Pattern::Record(fields) => {
                let (Value::Unknown(record) | Value::Opaque { fields: record, .. }) = value else {
                    return false;
                };
                fields.iter().all(|(name, pattern)| {
                    record
                        .get(name)
                        .is_some_and(|value| match_pattern(pattern, value, bindings))
                })
            }
            Pattern::List(patterns) => match value {
                Value::List(values) if patterns.len() == values.len() => patterns
                    .iter()
                    .zip(values)
                    .all(|(pattern, value)| match_pattern(pattern, value, bindings)),
                _ => false,
            },
            Pattern::Constructor(type_, patterns) => match value {
                Value::Opaque {
                    type_: actual,
                    fields,
                } if type_ == actual && patterns.len() == fields.len() => patterns
                    .iter()
                    .zip(fields.values())
                    .all(|(pattern, value)| match_pattern(pattern, value, bindings)),
                _ => false,
            },
        }
    }

    fn eval_runtime(
        expr: &Expr,
        environment: &BTreeMap<String, Value>,
    ) -> Result<Runtime, EvalError> {
        match expr {
            Expr::Literal(value) => Ok(Runtime::Value(value.clone())),
            Expr::Variable(name) => environment
                .get(name)
                .cloned()
                .map(Runtime::Value)
                .ok_or_else(|| EvalError::UnboundVariable(name.clone())),
            Expr::Lambda(parameter, body) => Ok(Runtime::Closure {
                parameter: parameter.clone(),
                body: body.clone(),
                environment: environment.clone(),
            }),
            Expr::Apply(function, argument) => {
                let function = eval_runtime(function, environment)?;
                let argument = eval_runtime(argument, environment)?.value()?;
                apply_runtime(function, argument)
            }
            Expr::Record(fields) => fields
                .iter()
                .map(|(name, value)| {
                    eval_runtime(value, environment)
                        .and_then(Runtime::value)
                        .map(|value| (name.clone(), value))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()
                .map(Value::Unknown)
                .map(Runtime::Value),
            Expr::List(items) => items
                .iter()
                .map(|item| eval_runtime(item, environment).and_then(Runtime::value))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List)
                .map(Runtime::Value),
            Expr::Field(record, field) => eval_field(record, field, environment),
            Expr::Index(list, index) => eval_index(list, index, environment),
            Expr::Match { scrutinee, arms } => eval_match(scrutinee, arms, environment),
            Expr::Let { name, value, body } => {
                let value = eval_runtime(value, environment)?.value()?;
                let mut nested = environment.clone();
                nested.insert(name.clone(), value);
                eval_runtime(body, &nested)
            }
            Expr::Equal(left, right) => {
                let left = eval_runtime(left, environment)?.value()?;
                let right = eval_runtime(right, environment)?.value()?;
                representation(&left, &right)
                    .map(Value::Bool)
                    .map(Runtime::Value)
                    .ok_or(EvalError::UndefinedRepresentation)
            }
            Expr::If(condition, then_expr, else_expr) => {
                match eval_runtime(condition, environment)?.value()? {
                    Value::Bool(true) => eval_runtime(then_expr, environment),
                    Value::Bool(false) => eval_runtime(else_expr, environment),
                    other => Err(type_error("bool", &other)),
                }
            }
            Expr::Builtin(op, arguments) => eval_builtin_expr(*op, arguments, environment),
        }
    }

    fn eval_field(
        record: &Expr,
        field: &str,
        environment: &BTreeMap<String, Value>,
    ) -> Result<Runtime, EvalError> {
        match eval_runtime(record, environment)?.value()? {
            Value::Unknown(fields) | Value::Opaque { fields, .. } => fields
                .get(field)
                .cloned()
                .map(Runtime::Value)
                .ok_or_else(|| EvalError::FieldNotFound(field.to_owned())),
            other => Err(type_error("record", &other)),
        }
    }

    fn eval_index(
        list: &Expr,
        index: &Expr,
        environment: &BTreeMap<String, Value>,
    ) -> Result<Runtime, EvalError> {
        let list = eval_runtime(list, environment)?.value()?;
        let index = eval_runtime(index, environment)?.value()?;
        match (list, index) {
            (Value::List(items), Value::Int(index)) => {
                let selected = usize::try_from(index)
                    .ok()
                    .and_then(|index| items.get(index).cloned());
                selected
                    .map(Runtime::Value)
                    .ok_or(EvalError::IndexOutOfBounds {
                        index,
                        len: items.len(),
                    })
            }
            (other, _) => Err(type_error("list and int", &other)),
        }
    }

    fn eval_match(
        scrutinee: &Expr,
        arms: &[(Pattern, Expr)],
        environment: &BTreeMap<String, Value>,
    ) -> Result<Runtime, EvalError> {
        let scrutinee = eval_runtime(scrutinee, environment)?.value()?;
        for (pattern, body) in arms {
            let mut bindings = BTreeMap::new();
            if match_pattern(pattern, &scrutinee, &mut bindings) {
                let mut nested = environment.clone();
                nested.extend(bindings);
                return eval_runtime(body, &nested);
            }
        }
        Err(EvalError::NonExhaustiveMatch)
    }

    fn eval_builtin_expr(
        op: BuiltinOp,
        arguments: &[Expr],
        environment: &BTreeMap<String, Value>,
    ) -> Result<Runtime, EvalError> {
        if matches!(
            op,
            BuiltinOp::Map | BuiltinOp::Filter | BuiltinOp::Fold | BuiltinOp::FlatMap
        ) {
            return eval_higher_order(op, arguments, environment);
        }
        arguments
            .iter()
            .map(|argument| eval_runtime(argument, environment).and_then(Runtime::value))
            .collect::<Result<Vec<_>, _>>()
            .and_then(|values| apply_builtin(op, &values))
            .map(Runtime::Value)
    }

    fn eval_higher_order(
        op: BuiltinOp,
        arguments: &[Expr],
        environment: &BTreeMap<String, Value>,
    ) -> Result<Runtime, EvalError> {
        let expected = if op == BuiltinOp::Fold { 3 } else { 2 };
        if arguments.len() != expected {
            return Err(EvalError::ArityMismatch {
                op,
                expected,
                got: arguments.len(),
            });
        }
        let list = eval_runtime(&arguments[0], environment)?.value()?;
        let Value::List(items) = list else {
            return Err(type_error("list", &list));
        };
        let function = eval_runtime(arguments.last().unwrap_or(&arguments[0]), environment)?;
        match op {
            BuiltinOp::Map => items
                .into_iter()
                .map(|item| apply_runtime(function.clone(), item).and_then(Runtime::value))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List)
                .map(Runtime::Value),
            BuiltinOp::Filter => {
                let mut output = Vec::new();
                for item in items {
                    match apply_runtime(function.clone(), item.clone())?.value()? {
                        Value::Bool(true) => output.push(item),
                        Value::Bool(false) => {}
                        other => return Err(type_error("bool", &other)),
                    }
                }
                Ok(Runtime::Value(Value::List(output)))
            }
            BuiltinOp::FlatMap => {
                let mut output = Vec::new();
                for item in items {
                    match apply_runtime(function.clone(), item)?.value()? {
                        Value::List(values) => output.extend(values),
                        other => return Err(type_error("list", &other)),
                    }
                }
                Ok(Runtime::Value(Value::List(output)))
            }
            BuiltinOp::Fold => {
                let mut accumulator = eval_runtime(&arguments[1], environment)?.value()?;
                for item in items {
                    let partial = apply_runtime(function.clone(), accumulator)?;
                    accumulator = apply_runtime(partial, item)?.value()?;
                }
                Ok(Runtime::Value(accumulator))
            }
            _ => unreachable!(),
        }
    }

    /// Definition: Evaluate one exact-value expression in the supplied
    /// lexical environment.
    ///
    /// Lambdas are applied with lexical capture; every builtin returns a
    /// typed error rather than silently changing a value's carrier.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError`] for an unbound name, ill-typed operation,
    /// overflow, failed conversion, or non-exhaustive pattern.
    ///
    /// Book anchors: `L-values-14`.
    pub fn eval(expr: &Expr, environment: &BTreeMap<String, Value>) -> Result<Value, EvalError> {
        eval_runtime(expr, environment)?.value()
    }

    const fn type_name(value: &Value) -> &'static str {
        match value {
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "string",
            Value::Bytes(_) => "bytes",
            Value::CidLink(_) => "cid_link",
            Value::Blob { .. } => "blob",
            Value::Token(_) => "token",
            Value::Null => "null",
            Value::Opaque { .. } => "opaque",
            Value::Unknown(_) => "unknown",
            Value::List(_) => "list",
            Value::LabeledNull(_) => "labeled_null",
            Value::Decimal(_) => "decimal",
            Value::BigInt(_) => "bigint",
            Value::Temporal(_) => "temporal",
        }
    }

    const fn type_error(expected: &'static str, got: &Value) -> EvalError {
        EvalError::TypeError {
            expected,
            got: type_name(got),
        }
    }

    const fn is_exact(value: &Value) -> bool {
        matches!(
            value,
            Value::Decimal(_) | Value::BigInt(_) | Value::Temporal(_)
        )
    }

    const fn expected_arity(op: BuiltinOp) -> usize {
        match op {
            BuiltinOp::Neg
            | BuiltinOp::Abs
            | BuiltinOp::Floor
            | BuiltinOp::Ceil
            | BuiltinOp::Round
            | BuiltinOp::Not
            | BuiltinOp::Len
            | BuiltinOp::Upper
            | BuiltinOp::Lower
            | BuiltinOp::Trim
            | BuiltinOp::Head
            | BuiltinOp::Tail
            | BuiltinOp::Reverse
            | BuiltinOp::Length
            | BuiltinOp::Keys
            | BuiltinOp::Values
            | BuiltinOp::IntToFloat
            | BuiltinOp::FloatToInt
            | BuiltinOp::IntToStr
            | BuiltinOp::FloatToStr
            | BuiltinOp::StrToInt
            | BuiltinOp::StrToFloat
            | BuiltinOp::TypeOf
            | BuiltinOp::IsNull
            | BuiltinOp::IsList
            | BuiltinOp::Children
            | BuiltinOp::EdgeCount
            | BuiltinOp::Anchor
            | BuiltinOp::ToString
            | BuiltinOp::ParseDecimal
            | BuiltinOp::DecimalToFloat => 1,
            BuiltinOp::Slice | BuiltinOp::Replace | BuiltinOp::Clamp | BuiltinOp::Fold => 3,
            _ => 2,
        }
    }

    /// Apply one exact/numeric builtin to already evaluated arguments.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError::ArityMismatch`] for the wrong number of arguments,
    /// or the operation's typed domain, arithmetic, parsing, or representation
    /// error.
    pub fn apply_builtin(op: BuiltinOp, arguments: &[Value]) -> Result<Value, EvalError> {
        let expected = expected_arity(op);
        if arguments.len() != expected {
            return Err(EvalError::ArityMismatch {
                op,
                expected,
                got: arguments.len(),
            });
        }
        match op {
            BuiltinOp::Add | BuiltinOp::Sub | BuiltinOp::Mul | BuiltinOp::Div => {
                numeric_binop(op, &arguments[0], &arguments[1])
            }
            BuiltinOp::Mod => integer_remainder(&arguments[0], &arguments[1]),
            BuiltinOp::Neg | BuiltinOp::Abs => numeric_unary(op, &arguments[0]),
            BuiltinOp::Floor | BuiltinOp::Ceil | BuiltinOp::Round => {
                rounded_integer(op, &arguments[0])
            }
            BuiltinOp::Eq | BuiltinOp::Ne => {
                let equal = representation(&arguments[0], &arguments[1])
                    .ok_or(EvalError::UndefinedRepresentation)?;
                Ok(Value::Bool(if op == BuiltinOp::Eq {
                    equal
                } else {
                    !equal
                }))
            }
            BuiltinOp::Lt | BuiltinOp::Lte | BuiltinOp::Gt | BuiltinOp::Gte => {
                let order = compare(&arguments[0], &arguments[1])?;
                Ok(Value::Bool(match op {
                    BuiltinOp::Lt => order.is_lt(),
                    BuiltinOp::Lte => !order.is_gt(),
                    BuiltinOp::Gt => order.is_gt(),
                    BuiltinOp::Gte => !order.is_lt(),
                    _ => false,
                }))
            }
            BuiltinOp::And | BuiltinOp::Or => match (&arguments[0], &arguments[1]) {
                (Value::Bool(left), Value::Bool(right)) => {
                    Ok(Value::Bool(if op == BuiltinOp::And {
                        *left && *right
                    } else {
                        *left || *right
                    }))
                }
                (other, _) => Err(type_error("bool", other)),
            },
            BuiltinOp::Not => match &arguments[0] {
                Value::Bool(value) => Ok(Value::Bool(!value)),
                other => Err(type_error("bool", other)),
            },
            BuiltinOp::Concat
            | BuiltinOp::Len
            | BuiltinOp::Slice
            | BuiltinOp::Upper
            | BuiltinOp::Lower
            | BuiltinOp::Trim
            | BuiltinOp::Split
            | BuiltinOp::Join
            | BuiltinOp::Replace
            | BuiltinOp::Contains => string_builtin(op, arguments),
            BuiltinOp::Append
            | BuiltinOp::Head
            | BuiltinOp::Tail
            | BuiltinOp::Reverse
            | BuiltinOp::Length
            | BuiltinOp::Range
            | BuiltinOp::MergeRecords
            | BuiltinOp::Keys
            | BuiltinOp::Values
            | BuiltinOp::HasField
            | BuiltinOp::DefaultVal
            | BuiltinOp::Clamp
            | BuiltinOp::TruncateStr => collection_builtin(op, arguments),
            BuiltinOp::IntToFloat
            | BuiltinOp::FloatToInt
            | BuiltinOp::IntToStr
            | BuiltinOp::FloatToStr
            | BuiltinOp::StrToInt
            | BuiltinOp::StrToFloat
            | BuiltinOp::TypeOf
            | BuiltinOp::IsNull
            | BuiltinOp::IsList
            | BuiltinOp::ToString
            | BuiltinOp::ParseDecimal
            | BuiltinOp::DecimalToFloat => conversion_builtin(op, arguments),
            BuiltinOp::Edge
            | BuiltinOp::Children
            | BuiltinOp::HasEdge
            | BuiltinOp::EdgeCount
            | BuiltinOp::Anchor
            | BuiltinOp::Edges => Err(EvalError::NoInstanceContext { op }),
            BuiltinOp::Map | BuiltinOp::Filter | BuiltinOp::Fold | BuiltinOp::FlatMap => {
                Err(EvalError::NotAFunction)
            }
        }
    }

    fn string_builtin(op: BuiltinOp, arguments: &[Value]) -> Result<Value, EvalError> {
        match op {
            BuiltinOp::Concat => strings2(&arguments[0], &arguments[1], |left, right| {
                Value::Str(format!("{left}{right}"))
            }),
            BuiltinOp::Len => match &arguments[0] {
                Value::Str(value) => i64::try_from(value.len())
                    .map(Value::Int)
                    .map_err(|_| EvalError::Overflow),
                other => Err(type_error("string", other)),
            },
            BuiltinOp::Slice => string_slice(&arguments[0], &arguments[1], &arguments[2]),
            BuiltinOp::Upper | BuiltinOp::Lower | BuiltinOp::Trim => match &arguments[0] {
                Value::Str(value) => Ok(Value::Str(match op {
                    BuiltinOp::Upper => value.to_uppercase(),
                    BuiltinOp::Lower => value.to_lowercase(),
                    BuiltinOp::Trim => value.trim().to_owned(),
                    _ => unreachable!(),
                })),
                other => Err(type_error("string", other)),
            },
            BuiltinOp::Split => strings2(&arguments[0], &arguments[1], |text, delimiter| {
                Value::List(
                    text.split(delimiter)
                        .map(|part| Value::Str(part.to_owned()))
                        .collect(),
                )
            }),
            BuiltinOp::Join => join_strings(&arguments[0], &arguments[1]),
            BuiltinOp::Replace => match (&arguments[0], &arguments[1], &arguments[2]) {
                (Value::Str(text), Value::Str(from), Value::Str(to)) => {
                    Ok(Value::Str(text.replace(from, to)))
                }
                (other, _, _) => Err(type_error("string", other)),
            },
            BuiltinOp::Contains => match (&arguments[0], &arguments[1]) {
                (Value::Str(text), Value::Str(part)) => Ok(Value::Bool(text.contains(part))),
                (Value::List(items), needle) => Ok(Value::Bool(items.contains(needle))),
                (other, _) => Err(type_error("string|string or list|any", other)),
            },
            _ => unreachable!("caller selected a string builtin"),
        }
    }

    fn collection_builtin(op: BuiltinOp, arguments: &[Value]) -> Result<Value, EvalError> {
        match op {
            BuiltinOp::Append => match &arguments[0] {
                Value::List(items) => {
                    let mut output = items.clone();
                    output.push(arguments[1].clone());
                    Ok(Value::List(output))
                }
                other => Err(type_error("list", other)),
            },
            BuiltinOp::Head => match &arguments[0] {
                Value::List(items) => items
                    .first()
                    .cloned()
                    .ok_or(EvalError::IndexOutOfBounds { index: 0, len: 0 }),
                other => Err(type_error("list", other)),
            },
            BuiltinOp::Tail => match &arguments[0] {
                Value::List(items) if !items.is_empty() => Ok(Value::List(items[1..].to_vec())),
                Value::List(_) => Err(EvalError::IndexOutOfBounds { index: 0, len: 0 }),
                other => Err(type_error("list", other)),
            },
            BuiltinOp::Reverse => match &arguments[0] {
                Value::List(items) => Ok(Value::List(items.iter().rev().cloned().collect())),
                other => Err(type_error("list", other)),
            },
            BuiltinOp::Length => match &arguments[0] {
                Value::List(items) => i64::try_from(items.len())
                    .map(Value::Int)
                    .map_err(|_| EvalError::Overflow),
                other => Err(type_error("list", other)),
            },
            BuiltinOp::Range => integer_range(&arguments[0], &arguments[1]),
            BuiltinOp::MergeRecords => merge_records(&arguments[0], &arguments[1]),
            BuiltinOp::Keys | BuiltinOp::Values => record_projection(op, &arguments[0]),
            BuiltinOp::HasField => match (&arguments[0], &arguments[1]) {
                (
                    Value::Unknown(record) | Value::Opaque { fields: record, .. },
                    Value::Str(field),
                ) => Ok(Value::Bool(record.contains_key(field))),
                (other, _) => Err(type_error("record and string", other)),
            },
            BuiltinOp::DefaultVal => Ok(if arguments[0] == Value::Null {
                arguments[1].clone()
            } else {
                arguments[0].clone()
            }),
            BuiltinOp::Clamp => clamp(&arguments[0], &arguments[1], &arguments[2]),
            BuiltinOp::TruncateStr => truncate_string(&arguments[0], &arguments[1]),
            _ => unreachable!("caller selected a collection builtin"),
        }
    }

    fn conversion_builtin(op: BuiltinOp, arguments: &[Value]) -> Result<Value, EvalError> {
        match op {
            BuiltinOp::IntToFloat => match arguments[0] {
                Value::Int(value) => Ok(Value::Float(canonical_float(int_as_f64(value)))),
                ref other => Err(type_error("int", other)),
            },
            BuiltinOp::FloatToInt => float_to_integer(&arguments[0], false),
            BuiltinOp::IntToStr => match arguments[0] {
                Value::Int(value) => Ok(Value::Str(value.to_string())),
                ref other => Err(type_error("int", other)),
            },
            BuiltinOp::FloatToStr => match arguments[0] {
                Value::Float(bits) => shortest(Format::B64, bits)
                    .map(|text| Value::Str(text.to_scientific()))
                    .ok_or(EvalError::FloatNotRepresentable),
                ref other => Err(type_error("float", other)),
            },
            BuiltinOp::StrToInt => match &arguments[0] {
                Value::Str(value) => value
                    .parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| EvalError::InvalidDecimal(value.clone())),
                other => Err(type_error("string", other)),
            },
            BuiltinOp::StrToFloat => match &arguments[0] {
                Value::Str(value) => value
                    .parse::<f64>()
                    .map(|value| Value::Float(canonical_float(value)))
                    .map_err(|_| EvalError::InvalidDecimal(value.clone())),
                other => Err(type_error("string", other)),
            },
            BuiltinOp::TypeOf => Ok(Value::Str(type_name(&arguments[0]).to_owned())),
            BuiltinOp::IsNull => Ok(Value::Bool(arguments[0] == Value::Null)),
            BuiltinOp::IsList => Ok(Value::Bool(matches!(arguments[0], Value::List(_)))),
            BuiltinOp::ToString => to_string_value(&arguments[0]).map(Value::Str),
            BuiltinOp::ParseDecimal => parse_decimal_value(&arguments[0]),
            BuiltinOp::DecimalToFloat => decimal_to_float(&arguments[0]),
            _ => unreachable!("caller selected a conversion builtin"),
        }
    }

    fn rounded_integer(op: BuiltinOp, value: &Value) -> Result<Value, EvalError> {
        let Value::Float(bits) = value else {
            return Err(type_error("float", value));
        };
        let value = f64::from_bits(*bits);
        if !value.is_finite() {
            return Err(EvalError::FloatNotRepresentable);
        }
        let rounded = match op {
            BuiltinOp::Floor => value.floor(),
            BuiltinOp::Ceil => value.ceil(),
            BuiltinOp::Round => value.round_ties_even(),
            _ => unreachable!(),
        };
        format!("{rounded:.0}")
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| EvalError::FloatNotRepresentable)
    }

    fn strings2(
        left: &Value,
        right: &Value,
        function: impl FnOnce(&str, &str) -> Value,
    ) -> Result<Value, EvalError> {
        match (left, right) {
            (Value::Str(left), Value::Str(right)) => Ok(function(left, right)),
            (other, _) => Err(type_error("string", other)),
        }
    }

    fn string_slice(text: &Value, start: &Value, end: &Value) -> Result<Value, EvalError> {
        let (Value::Str(text), Value::Int(start), Value::Int(end)) = (text, start, end) else {
            return Err(type_error("string, int, int", text));
        };
        let characters: Vec<char> = text.chars().collect();
        let start = usize::try_from((*start).max(0)).unwrap_or(usize::MAX);
        let end = usize::try_from((*end).max(0)).unwrap_or(usize::MAX);
        let end = end.min(characters.len());
        let start = start.min(end);
        Ok(Value::Str(characters[start..end].iter().collect()))
    }

    fn clamp(value: &Value, minimum: &Value, maximum: &Value) -> Result<Value, EvalError> {
        match (value, minimum, maximum) {
            (Value::Int(value), Value::Int(minimum), Value::Int(maximum)) if minimum <= maximum => {
                Ok(Value::Int((*value).clamp(*minimum, *maximum)))
            }
            (Value::Float(value), Value::Float(minimum), Value::Float(maximum)) => {
                let (value, minimum, maximum) = (
                    f64::from_bits(*value),
                    f64::from_bits(*minimum),
                    f64::from_bits(*maximum),
                );
                if minimum.is_nan() || maximum.is_nan() || minimum > maximum {
                    return Err(EvalError::TypeError {
                        expected: "clamp requires min <= max",
                        got: "unordered or reversed bounds",
                    });
                }
                Ok(Value::Float(canonical_float(value.clamp(minimum, maximum))))
            }
            (Value::Int(_), Value::Int(_), Value::Int(_)) => Err(EvalError::TypeError {
                expected: "clamp requires min <= max",
                got: "min > max",
            }),
            (other, _, _) => Err(type_error("three homogeneous ints or floats", other)),
        }
    }

    fn join_strings(items: &Value, delimiter: &Value) -> Result<Value, EvalError> {
        let (Value::List(items), Value::Str(delimiter)) = (items, delimiter) else {
            return Err(type_error("list and string", items));
        };
        let strings: Option<Vec<&str>> = items
            .iter()
            .map(|item| match item {
                Value::Str(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        strings
            .map(|strings| Value::Str(strings.join(delimiter)))
            .ok_or_else(|| type_error("list of strings", items.first().unwrap_or(&Value::Null)))
    }

    fn integer_range(start: &Value, stop: &Value) -> Result<Value, EvalError> {
        let (Value::Int(start), Value::Int(stop)) = (start, stop) else {
            return Err(type_error("int", start));
        };
        if stop < start {
            return Ok(Value::List(Vec::new()));
        }
        let mut values = Vec::new();
        let mut current = *start;
        loop {
            values.push(Value::Int(current));
            if current == *stop {
                break;
            }
            current = current.checked_add(1).ok_or(EvalError::Overflow)?;
        }
        Ok(Value::List(values))
    }

    fn merge_records(left: &Value, right: &Value) -> Result<Value, EvalError> {
        let left_fields = match left {
            Value::Unknown(fields) | Value::Opaque { fields, .. } => fields,
            other => return Err(type_error("record", other)),
        };
        let right_fields = match right {
            Value::Unknown(fields) | Value::Opaque { fields, .. } => fields,
            other => return Err(type_error("record", other)),
        };
        let mut result = left_fields.clone();
        result.extend(right_fields.clone());
        Ok(Value::Unknown(result))
    }

    fn record_projection(op: BuiltinOp, record: &Value) -> Result<Value, EvalError> {
        let record = match record {
            Value::Unknown(fields) | Value::Opaque { fields, .. } => fields,
            other => return Err(type_error("record", other)),
        };
        Ok(Value::List(match op {
            BuiltinOp::Keys => record.keys().cloned().map(Value::Str).collect(),
            BuiltinOp::Values => record.values().cloned().collect(),
            _ => unreachable!(),
        }))
    }

    fn truncate_string(text: &Value, maximum: &Value) -> Result<Value, EvalError> {
        let (Value::Str(text), Value::Int(maximum)) = (text, maximum) else {
            return Err(type_error("string and int", text));
        };
        let maximum = usize::try_from(*maximum).map_err(|_| EvalError::Overflow)?;
        if text.len() <= maximum {
            return Ok(Value::Str(text.clone()));
        }
        let boundary = (0..=maximum)
            .rev()
            .find(|index| text.is_char_boundary(*index))
            .unwrap_or(0);
        Ok(Value::Str(text[..boundary].to_owned()))
    }

    fn float_to_integer(value: &Value, rounded: bool) -> Result<Value, EvalError> {
        let Value::Float(bits) = value else {
            return Err(type_error("float", value));
        };
        let value = f64::from_bits(*bits);
        if !value.is_finite() {
            return Err(EvalError::FloatNotRepresentable);
        }
        let value = if rounded {
            value.round_ties_even()
        } else {
            value.trunc()
        };
        format!("{value:.0}")
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| EvalError::FloatNotRepresentable)
    }

    const fn canonical_float(value: f64) -> u64 {
        if value.is_nan() {
            CANONICAL_NAN_BITS
        } else {
            value.to_bits()
        }
    }

    fn exact_arithmetic(op: BuiltinOp, values: &[&Value]) -> Option<EvalError> {
        values
            .iter()
            .find(|value| is_exact(value))
            .map(|value| EvalError::ExactArithmetic {
                op,
                operand: type_name(value),
            })
    }

    fn numeric_binop(op: BuiltinOp, left: &Value, right: &Value) -> Result<Value, EvalError> {
        if let Some(error) = exact_arithmetic(op, &[left, right]) {
            return Err(error);
        }
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                if op == BuiltinOp::Div && *b == 0 {
                    return Err(EvalError::DivisionByZero);
                }
                let result = match op {
                    BuiltinOp::Add => a.checked_add(*b),
                    BuiltinOp::Sub => a.checked_sub(*b),
                    BuiltinOp::Mul => a.checked_mul(*b),
                    BuiltinOp::Div => a.checked_div(*b),
                    _ => None,
                };
                result.map(Value::Int).ok_or(EvalError::Overflow)
            }
            (Value::Int(a), Value::Float(b)) => float_binop(op, int_as_f64(*a), f64::from_bits(*b)),
            (Value::Float(a), Value::Int(b)) => float_binop(op, f64::from_bits(*a), int_as_f64(*b)),
            (Value::Float(a), Value::Float(b)) => {
                float_binop(op, f64::from_bits(*a), f64::from_bits(*b))
            }
            (other, _) => Err(type_error("int|float", other)),
        }
    }

    fn int_as_f64(value: i64) -> f64 {
        value.to_string().parse().unwrap_or(if value < 0 {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        })
    }

    fn float_binop(op: BuiltinOp, left: f64, right: f64) -> Result<Value, EvalError> {
        if op == BuiltinOp::Div && right == 0.0 {
            return Err(EvalError::DivisionByZero);
        }
        let value = match op {
            BuiltinOp::Add => left + right,
            BuiltinOp::Sub => left - right,
            BuiltinOp::Mul => left * right,
            BuiltinOp::Div => left / right,
            _ => {
                return Err(EvalError::TypeError {
                    expected: "arithmetic",
                    got: "builtin",
                });
            }
        };
        Ok(Value::Float(canonical_float(value)))
    }

    fn integer_remainder(left: &Value, right: &Value) -> Result<Value, EvalError> {
        if let Some(error) = exact_arithmetic(BuiltinOp::Mod, &[left, right]) {
            return Err(error);
        }
        match (left, right) {
            (Value::Int(_), Value::Int(0)) => Err(EvalError::DivisionByZero),
            (Value::Int(a), Value::Int(b)) => {
                a.checked_rem(*b).map(Value::Int).ok_or(EvalError::Overflow)
            }
            (other, _) => Err(type_error("int", other)),
        }
    }

    fn numeric_unary(op: BuiltinOp, value: &Value) -> Result<Value, EvalError> {
        if let Some(error) = exact_arithmetic(op, &[value]) {
            return Err(error);
        }
        match value {
            Value::Int(value) => match op {
                BuiltinOp::Neg => value.checked_neg(),
                BuiltinOp::Abs => value.checked_abs(),
                _ => None,
            }
            .map(Value::Int)
            .ok_or(EvalError::Overflow),
            Value::Float(bits) => {
                let value = f64::from_bits(*bits);
                let computed = if op == BuiltinOp::Neg {
                    -value
                } else {
                    value.abs()
                };
                Ok(Value::Float(canonical_float(computed)))
            }
            other => Err(type_error("int|float", other)),
        }
    }

    fn exact_decimal(value: &Value) -> Option<DecimalValue> {
        match value {
            Value::Int(value) => match parse_decimal(&value.to_string())? {
                Decimal::Finite(decimal) => Some(decimal),
                _ => None,
            },
            Value::BigInt(value) => Some(DecimalValue {
                negative: value.negative,
                digits: value.significand.clone(),
                exponent: Int::new(false, value.zeros.clone()),
            }),
            Value::Decimal(Decimal::Finite(value)) => Some(value.clone()),
            _ => None,
        }
    }

    fn compare(left: &Value, right: &Value) -> Result<Ordering, EvalError> {
        if let (Value::Temporal(left), Value::Temporal(right)) = (left, right) {
            return crate::temporal::cmp_chronological(left, right).ok_or({
                EvalError::TypeError {
                    expected: "temporals of one comparable form",
                    got: "incomparable temporals",
                }
            });
        }
        if is_exact(left) || is_exact(right) {
            return match (exact_decimal(left), exact_decimal(right)) {
                (Some(left), Some(right)) => Ok(left.cmp_numeric(&right)),
                _ => Err(EvalError::TypeError {
                    expected: "int|bigint|finite decimal, or comparable temporals",
                    got: "incomparable exact values",
                }),
            };
        }
        match (left, right) {
            (Value::Int(left), Value::Int(right)) => Ok(left.cmp(right)),
            (Value::Float(left), Value::Float(right)) => {
                Ok(f64::from_bits(*left).total_cmp(&f64::from_bits(*right)))
            }
            (Value::Int(left), Value::Float(right)) => {
                Ok(int_as_f64(*left).total_cmp(&f64::from_bits(*right)))
            }
            (Value::Float(left), Value::Int(right)) => {
                Ok(f64::from_bits(*left).total_cmp(&int_as_f64(*right)))
            }
            (Value::Str(left), Value::Str(right)) => Ok(left.cmp(right)),
            (other, _) => Err(type_error("comparable values", other)),
        }
    }

    fn decimal_text(value: &Decimal) -> String {
        match value {
            Decimal::Finite(value) => {
                let sign = if value.negative { "-" } else { "" };
                let exponent = value.exponent.to_i64();
                if value.exponent.is_negative()
                    && exponent.is_some_and(|exponent| {
                        exponent
                            >= -(i64::try_from(value.digits.to_string().len()).unwrap_or(i64::MAX)
                                + 6)
                    })
                {
                    let digits = value.digits.to_string();
                    let scale = usize::try_from(exponent.unwrap_or(0).unsigned_abs()).unwrap_or(0);
                    if scale == 0 {
                        format!("{sign}{digits}")
                    } else if scale >= digits.len() {
                        format!("{sign}0.{}{}", "0".repeat(scale - digits.len()), digits)
                    } else {
                        let (integer, fraction) = digits.split_at(digits.len() - scale);
                        format!("{sign}{integer}.{fraction}")
                    }
                } else if value.exponent.is_zero() {
                    format!("{sign}{}", value.digits)
                } else {
                    format!("{sign}{}e{}", value.digits, value.exponent)
                }
            }
            Decimal::Infinite { negative } => {
                if *negative { "-Infinity" } else { "Infinity" }.to_owned()
            }
            Decimal::NaN {
                negative,
                signalling,
                payload,
            } => {
                let sign = if *negative { "-" } else { "" };
                let signal = if *signalling { "s" } else { "" };
                let payload = if payload.is_zero() {
                    String::new()
                } else {
                    payload.to_string()
                };
                format!("{sign}{signal}NaN{payload}")
            }
        }
    }

    fn bigint_text(value: &BigIntValue) -> String {
        let sign = if value.negative { "-" } else { "" };
        let zeros = value.zeros.to_u64().and_then(|n| usize::try_from(n).ok());
        zeros.map_or_else(
            || format!("{sign}{}e{}", value.significand, value.zeros),
            |zeros| format!("{sign}{}{}", value.significand, "0".repeat(zeros)),
        )
    }

    fn to_string_value(value: &Value) -> Result<String, EvalError> {
        match value {
            Value::Bool(value) => Ok(value.to_string()),
            Value::Int(value) => Ok(value.to_string()),
            Value::Float(bits) => Ok(f64::from_bits(*bits).to_string()),
            Value::Str(value) => Ok(value.clone()),
            Value::Decimal(value) => Ok(decimal_text(value)),
            Value::BigInt(value) => Ok(bigint_text(value)),
            Value::Temporal(value) => Ok(value.to_string()),
            other => Err(type_error(
                "bool|int|float|string|decimal|bigint|temporal",
                other,
            )),
        }
    }

    fn parse_decimal_value(value: &Value) -> Result<Value, EvalError> {
        match value {
            Value::Str(text) => parse_decimal(text)
                .map(Value::Decimal)
                .ok_or_else(|| EvalError::InvalidDecimal(text.clone())),
            Value::Int(_) | Value::BigInt(_) | Value::Decimal(Decimal::Finite(_)) => {
                exact_decimal(value)
                    .map(|decimal| Value::Decimal(Decimal::Finite(decimal)))
                    .ok_or_else(|| type_error("string|int|bigint|decimal", value))
            }
            Value::Decimal(value) => Ok(Value::Decimal(value.clone())),
            other => Err(type_error("string|int|bigint|decimal", other)),
        }
    }

    fn decimal_to_float(value: &Value) -> Result<Value, EvalError> {
        if let Value::Decimal(Decimal::Infinite { negative }) = value {
            return Ok(Value::Float(if *negative {
                f64::NEG_INFINITY.to_bits()
            } else {
                f64::INFINITY.to_bits()
            }));
        }
        if matches!(value, Value::Decimal(Decimal::NaN { .. })) {
            return Ok(Value::Float(CANONICAL_NAN_BITS));
        }
        let text = match value {
            Value::Decimal(value) => decimal_text(value),
            Value::BigInt(value) => format!(
                "{}{}e{}",
                if value.negative { "-" } else { "" },
                value.significand,
                value.zeros
            ),
            other => return Err(type_error("decimal|bigint", other)),
        };
        let parsed = text
            .parse::<f64>()
            .map_err(|_| EvalError::InvalidDecimal(text))?;
        Ok(Value::Float(canonical_float(parsed)))
    }
}

pub use self::bytes::{
    latin1_decode_encode, latin1_encode_decode, optional, padded_bytes_decode_encode,
    padded_text_encode_decode,
};
pub use self::canonical::{decimal_bytes, decode_value};
pub use self::coerce::{coerce, coerce_dialect, coerce_discriminated, coerce_tagged, coerce_value};
pub use self::exact::{Decimal, DecimalValue};
pub use self::float::{
    binary16, computed_nan, exact_reading, narrow_bits, nearest_reading, round_to, shortest,
    widen_bits,
};
pub use self::temporal::{
    canonical_bytes, cmp_chronological, display_parse, from_epoch_count, parse_display,
    to_epoch_count, unwritten_zulu,
};
pub use self::value::{BuiltinOp, EvalError, Value, apply_builtin, eval, representation};
