//! Production coercion agrees exactly with `panproto-reference`'s.
//!
//! Each test enumerates a finite scope built from the reference's own value
//! enumerators (`enumerate::values` and the boundary float patterns), runs
//! the production function and its `panproto_reference::coerce` counterpart
//! on every case, and requires identical results: the same value, or a
//! refusal for the same [`FidelityReason`] with the same payload. Each test
//! prints its bound and its counts.
//!
//! # The conversion between the two value models
//!
//! The reference and production carriers cover the same values in this
//! scope: decimal NaNs, infinities and negative zeros; reduced-precision,
//! front-truncated and unwritten-offset temporals; padded and Latin-1 byte
//! texts; numeral dialects; and both float readings. Production embeds in
//! the reference model by [`to_ref`]
//! (values), [`spec_to_ref`] (specs) and [`reason_to_ref`] (refusals):
//!
//! - `Bool`, `Int`, `Str`, `Bytes`, `CidLink`, `Blob`, `Token`, `Null` and
//!   `LabeledNull` go to the variant of the same name with the same payload.
//! - `Float(x)` goes to `Float(x.to_bits())`.
//! - `Decimal { negative, digits, exponent }` goes to
//!   `Decimal::Finite { negative, digits: Nat(digits), exponent: Int(exponent) }`,
//!   `BigInt { negative, significand, zeros }` to
//!   `BigInt { negative, significand: Nat(significand), zeros: Nat(zeros) }`.
//! - `Opaque`, `Unknown` and `List` go to the same variant with every
//!   field value or item converted, the hash map becoming a sorted map with
//!   the same keys.
//! - A date `y-m-d` goes to the reference temporal with date fields
//!   `(Year { value: y, negative_zero: false, width: max(4, digits(|y|)) }, m, d)`,
//!   no time, no separator, a local offset and no infinity; a time `h:m:s.f`
//!   with offset `o` to the one with time fields `(h, m, s, f)`, no date and
//!   offset `o'`; a date-time to the one with both, its separator mapped
//!   case for case. The offset map `o -> o'` sends `Local` to `Local`,
//!   `Zulu(case)` to `Zulu(case)`, `Fixed(m)` to `Fixed(m, Extended)` and
//!   `NegZero` to `NegZero(Extended)`. The extended temporal carrier maps
//!   every optional field, written offset form and infinity component
//!   directly.
//! - A spec goes to the spec of the corresponding kind (the four production
//!   byte texts to the reference's of the same name) with the same
//!   precision, scale, offset requirement, integer width and signedness,
//!   the exact float reading and no numeral dialect. The conversion is
//!   defined on specs with no `max_length`, the only specs the scope holds
//!   (neither coercion reads a length bound).
//! - Each of the ten refusal reasons goes to the reason of the same name
//!   with the same payload.
//!
//! **Proof that the conversion is total and injective.** Totality: every
//! clause above is defined on every production value of its variant. The
//! digit and exponent maps are defined because production holds `digits`,
//! `significand` and a `WideInt` magnitude as non-empty ASCII digit strings
//! (the invariants of `DecimalValue::new`, `BigIntValue` and `WideInt`),
//! which `Nat::parse` reads, and a `BigInt`'s zero count is never negative;
//! the temporal clauses cover the three historical temporal variants and
//! the extended carrier; the spec clause
//! covers every production kind. Injectivity, by induction on the value:
//! (1) distinct non-temporal production variants go to distinct reference
//! variants, while temporal constructors map injectively through their exact
//! field-presence and lexical descriptors; (2) within a variant every
//! component map is
//! injective: `f64::to_bits` is injective on representations (it is a
//! bijection with `u64`); `Nat::parse` is injective on digit strings without
//! a leading zero (two such strings read as one number are equal, since
//! positional notation without leading zeros is unique), and production
//! keeps exactly such strings (zero is `"0"`); a `WideInt` is its sign and
//! such a magnitude, and `Int::new` keeps both for a nonzero magnitude and
//! production never stores a negative zero; the year map keeps `value`, the
//! month, day, hour, minute, second and fraction maps are identities, and
//! the offset and separator maps send distinct constructors (and distinct
//! minute counts) to distinct ones; a map with distinct keys goes to a
//! sorted map with the same distinct keys and, by the induction hypothesis,
//! injectively converted values, and a list is converted item by item;
//! (3) the reason map is a bijection of the ten names, and the spec map is
//! injective on its domain because each of its components is copied or
//! mapped by an injective kind map. So equal images have equal preimages.
//! [`to_ref`] has the left inverse [`from_ref`] on the scope, which
//! [`the_value_conversion_is_injective_on_the_scope`] checks exhaustively,
//! with production values compared bit for bit ([`same`]).
//!
//! A reference input outside the image of the conversion (one production's
//! model cannot hold) is counted and reported, never compared; a reference
//! result of `None` (an input outside the reference's stated domain) is
//! counted the same way. Every other case is compared, and the tests fail
//! on any disagreement.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

use panproto_inst::Value;
use panproto_inst::value::coerce::{LexClass, Lexeme, coerce, coerce_discriminated, coerce_value};
use panproto_inst::value::{
    BigIntValue, CivilDate, CivilTime, DateFields, DateTimeSeparator, DecimalClass, DecimalValue,
    ExtendedTemporal, FidelityReason, Fraction, Offset, OffsetForm, Temporal, TemporalForm,
    TemporalInfinity, TemporalYear, TimeFields, ValueSite, WideInt, ZuluCase,
};
use panproto_reference::coerce as r;
use panproto_reference::enumerate::values as scope;
use panproto_reference::exact as rx;
use panproto_reference::float::{Format, narrow_bits, widen_bits};
use panproto_reference::num::{Int, Nat};
use panproto_reference::temporal as rt;
use panproto_reference::value as rv;
use panproto_reference::value::Value as RValue;
use panproto_schema::value_kinds::{FloatReading, NumeralDialect};
use panproto_schema::{BytesText, ScalarKind, ValueSpec};

// ---------------------------------------------------------------------------
// The conversion and its left inverse.
// ---------------------------------------------------------------------------

/// The reference natural of a canonical production digit string.
fn nat(digits: &str) -> Nat {
    Nat::parse(digits).expect("production digit strings are non-empty ASCII digits")
}

/// The reference integer of a production `WideInt`.
fn int_of(w: &WideInt) -> Int {
    Int::new(w.is_negative(), nat(w.magnitude()))
}

/// The production `WideInt` of a reference integer.
fn wide_of(i: &Int) -> WideInt {
    WideInt::from_sign_and_digits(i.is_negative(), &i.magnitude().to_string())
        .expect("a natural writes as ASCII digits")
}

/// The number of digits of `|year|`, at least four: the width production's
/// date display writes.
fn year_width(year: i64) -> usize {
    year.unsigned_abs().to_string().len().max(4)
}

const fn offset_to_ref(o: Offset) -> rt::Offset {
    match o {
        Offset::Local => rt::Offset::Local,
        Offset::Zulu(ZuluCase::Upper) => rt::Offset::Zulu(rt::ZuluCase::Upper),
        Offset::Zulu(ZuluCase::Lower) => rt::Offset::Zulu(rt::ZuluCase::Lower),
        Offset::Zulu(ZuluCase::Unwritten) => rt::Offset::Zulu(rt::ZuluCase::Unwritten),
        Offset::Fixed(m) => rt::Offset::Fixed(m, rt::OffsetForm::Extended),
        Offset::FixedForm(m, form) => rt::Offset::Fixed(m, offset_form_to_ref(form)),
        Offset::FixedSeconds(s) => rt::Offset::FixedSeconds(s),
        Offset::NegZero => rt::Offset::NegZero(rt::OffsetForm::Extended),
        Offset::NegZeroForm(form) => rt::Offset::NegZero(offset_form_to_ref(form)),
    }
}

const fn offset_form_to_ref(form: OffsetForm) -> rt::OffsetForm {
    match form {
        OffsetForm::Extended => rt::OffsetForm::Extended,
        OffsetForm::Basic => rt::OffsetForm::Basic,
        OffsetForm::HourOnly => rt::OffsetForm::HourOnly,
        OffsetForm::ExtendedSeconds => rt::OffsetForm::ExtendedSeconds,
        OffsetForm::Unwritten => rt::OffsetForm::Unwritten,
    }
}

const fn offset_form_from_ref(form: rt::OffsetForm) -> OffsetForm {
    match form {
        rt::OffsetForm::Extended => OffsetForm::Extended,
        rt::OffsetForm::Basic => OffsetForm::Basic,
        rt::OffsetForm::HourOnly => OffsetForm::HourOnly,
        rt::OffsetForm::ExtendedSeconds => OffsetForm::ExtendedSeconds,
        rt::OffsetForm::Unwritten => OffsetForm::Unwritten,
    }
}

fn offset_from_ref(o: rt::Offset) -> Option<Offset> {
    Some(match o {
        rt::Offset::Local => Offset::Local,
        rt::Offset::Zulu(rt::ZuluCase::Upper) => Offset::Zulu(ZuluCase::Upper),
        rt::Offset::Zulu(rt::ZuluCase::Lower) => Offset::Zulu(ZuluCase::Lower),
        rt::Offset::Zulu(rt::ZuluCase::Unwritten) => Offset::Zulu(ZuluCase::Unwritten),
        rt::Offset::Fixed(m, rt::OffsetForm::Extended) => Offset::fixed(m).ok()?,
        rt::Offset::Fixed(m, form) => Offset::fixed_form(m, offset_form_from_ref(form)).ok()?,
        rt::Offset::FixedSeconds(s) => Offset::fixed_seconds(s).ok()?,
        rt::Offset::NegZero(rt::OffsetForm::Extended) => Offset::NegZero,
        rt::Offset::NegZero(form) => {
            let form = offset_form_from_ref(form);
            if form == OffsetForm::Unwritten {
                return None;
            }
            Offset::NegZeroForm(form)
        }
    })
}

const fn separator_to_ref(s: DateTimeSeparator) -> rt::Separator {
    match s {
        DateTimeSeparator::Upper => rt::Separator::Upper,
        DateTimeSeparator::Lower => rt::Separator::Lower,
        DateTimeSeparator::Space => rt::Separator::Space,
    }
}

const fn separator_from_ref(s: rt::Separator) -> DateTimeSeparator {
    match s {
        rt::Separator::Upper => DateTimeSeparator::Upper,
        rt::Separator::Lower => DateTimeSeparator::Lower,
        rt::Separator::Space => DateTimeSeparator::Space,
    }
}

fn date_to_ref(d: CivilDate) -> rt::DateFields {
    rt::DateFields {
        year: Some(rt::Year {
            value: d.year,
            negative_zero: false,
            width: year_width(d.year),
        }),
        month: Some(d.month),
        day: Some(d.day),
    }
}

fn time_to_ref(t: &CivilTime) -> rt::TimeFields {
    rt::TimeFields {
        hour: Some(t.hour),
        minute: Some(t.minute),
        second: Some(t.second),
        fraction: t.fraction.as_str().to_owned(),
    }
}

fn date_from_ref(d: &rt::DateFields) -> Option<CivilDate> {
    let year = d.year?;
    if year.negative_zero {
        return None;
    }
    CivilDate::new(year.value, d.month?, d.day?).ok()
}

fn time_from_ref(t: &rt::TimeFields) -> Option<CivilTime> {
    CivilTime::new(
        t.hour?,
        t.minute?,
        t.second?,
        Fraction::new(&t.fraction).ok()?,
    )
    .ok()
}

fn date_fields_to_ref(date: DateFields) -> rt::DateFields {
    rt::DateFields {
        year: date.year.map(|year| rt::Year {
            value: year.value,
            negative_zero: year.negative_zero,
            width: year.width,
        }),
        month: date.month,
        day: date.day,
    }
}

fn time_fields_to_ref(time: &TimeFields) -> rt::TimeFields {
    rt::TimeFields {
        hour: time.hour,
        minute: time.minute,
        second: time.second,
        fraction: time.fraction.as_str().to_owned(),
    }
}

fn date_fields_from_ref(date: &rt::DateFields) -> DateFields {
    DateFields {
        year: date.year.map(|year| TemporalYear {
            value: year.value,
            negative_zero: year.negative_zero,
            width: year.width,
        }),
        month: date.month,
        day: date.day,
    }
}

fn time_fields_from_ref(time: &rt::TimeFields) -> Option<TimeFields> {
    Some(TimeFields {
        hour: time.hour,
        minute: time.minute,
        second: time.second,
        fraction: Fraction::new(&time.fraction).ok()?,
    })
}

fn temporal_to_ref(t: &Temporal) -> rt::Temporal {
    let (date, time, separator, offset) = match t {
        Temporal::Date(d) => (Some(date_to_ref(*d)), None, None, Offset::Local),
        Temporal::Time(t, o) => (None, Some(time_to_ref(t)), None, *o),
        Temporal::DateTime(d, t, o, s) => (
            Some(date_to_ref(*d)),
            Some(time_to_ref(t)),
            Some(separator_to_ref(*s)),
            *o,
        ),
        Temporal::Extended(value) => {
            return rt::Temporal {
                date: value.date.map(date_fields_to_ref),
                time: value.time.as_ref().map(time_fields_to_ref),
                separator: value.separator.map(separator_to_ref),
                offset: offset_to_ref(value.offset),
                infinity: value.infinity.map(|(sign, form)| {
                    (
                        match sign {
                            TemporalInfinity::Negative => rt::TemporalInfinity::Negative,
                            TemporalInfinity::Positive => rt::TemporalInfinity::Positive,
                        },
                        match form {
                            TemporalForm::Date => rt::TemporalForm::Date,
                            TemporalForm::Time => rt::TemporalForm::Time,
                            TemporalForm::DateTime => rt::TemporalForm::DateTime,
                        },
                    )
                }),
            };
        }
    };
    rt::Temporal {
        date,
        time,
        separator,
        offset: offset_to_ref(offset),
        infinity: None,
    }
}

fn temporal_from_ref(t: &rt::Temporal) -> Option<Temporal> {
    let offset = offset_from_ref(t.offset)?;
    if t.infinity.is_none() {
        match (&t.date, &t.time, t.separator) {
            (Some(d), None, None) if offset == Offset::Local => {
                if let Some(date) = date_from_ref(d) {
                    return Some(Temporal::Date(date));
                }
            }
            (None, Some(time), None)
                if matches!(
                    offset,
                    Offset::Local
                        | Offset::Zulu(ZuluCase::Upper | ZuluCase::Lower)
                        | Offset::Fixed(_)
                        | Offset::NegZero
                ) =>
            {
                if let Some(time) = time_from_ref(time) {
                    return Some(Temporal::Time(time, offset));
                }
            }
            (Some(d), Some(time), Some(separator))
                if matches!(
                    offset,
                    Offset::Local
                        | Offset::Zulu(ZuluCase::Upper | ZuluCase::Lower)
                        | Offset::Fixed(_)
                        | Offset::NegZero
                ) =>
            {
                if let (Some(date), Some(time)) = (date_from_ref(d), time_from_ref(time)) {
                    return Some(Temporal::DateTime(
                        date,
                        time,
                        offset,
                        separator_from_ref(separator),
                    ));
                }
            }
            _ => {}
        }
    }
    Some(Temporal::Extended(ExtendedTemporal {
        date: t.date.as_ref().map(date_fields_from_ref),
        time: match &t.time {
            Some(time) => Some(time_fields_from_ref(time)?),
            None => None,
        },
        separator: t.separator.map(separator_from_ref),
        offset,
        infinity: t.infinity.map(|(sign, form)| {
            (
                match sign {
                    rt::TemporalInfinity::Negative => TemporalInfinity::Negative,
                    rt::TemporalInfinity::Positive => TemporalInfinity::Positive,
                },
                match form {
                    rt::TemporalForm::Date => TemporalForm::Date,
                    rt::TemporalForm::Time => TemporalForm::Time,
                    rt::TemporalForm::DateTime => TemporalForm::DateTime,
                },
            )
        }),
    }))
}

/// The reference decimal of a production decimal: a finite value field for
/// field (a negative zero included), an infinity or a NaN with its sign,
/// flag and payload.
fn decimal_to_ref(d: &DecimalValue) -> rx::Decimal {
    match &d.class {
        DecimalClass::Finite => rx::Decimal::Finite(rx::DecimalValue {
            negative: d.negative,
            digits: nat(&d.digits),
            exponent: int_of(&d.exponent),
        }),
        DecimalClass::Infinite => rx::Decimal::Infinite {
            negative: d.negative,
        },
        DecimalClass::NaN {
            signalling,
            payload,
        } => rx::Decimal::NaN {
            negative: d.negative,
            signalling: *signalling,
            payload: nat(payload),
        },
    }
}

/// The conversion of a production value into the reference model.
fn to_ref(v: &Value) -> RValue {
    let fields = |m: &HashMap<String, Value>| -> BTreeMap<String, RValue> {
        m.iter().map(|(k, v)| (k.clone(), to_ref(v))).collect()
    };
    match v {
        Value::Bool(b) => RValue::Bool(*b),
        Value::Int(i) => RValue::Int(*i),
        Value::Float(x) => RValue::Float(x.to_bits()),
        Value::Str(s) => RValue::Str(s.clone()),
        Value::Bytes(b) => RValue::Bytes(b.clone()),
        Value::CidLink(c) => RValue::CidLink(c.clone()),
        Value::Blob { ref_, mime, size } => RValue::Blob {
            ref_: ref_.clone(),
            mime: mime.clone(),
            size: *size,
        },
        Value::Token(t) => RValue::Token(t.clone()),
        Value::Null => RValue::Null,
        Value::Opaque { type_, fields: f } => RValue::Opaque {
            type_: type_.clone(),
            fields: fields(f),
        },
        Value::Unknown(f) => RValue::Unknown(fields(f)),
        Value::List(items) => RValue::List(items.iter().map(to_ref).collect()),
        Value::LabeledNull(n) => RValue::LabeledNull(*n),
        Value::Decimal(d) => RValue::Decimal(decimal_to_ref(d)),
        Value::BigInt(b) => RValue::BigInt(rx::BigIntValue {
            negative: b.negative,
            significand: nat(&b.significand),
            zeros: nat(b.zeros.magnitude()),
        }),
        Value::Temporal(t) => RValue::Temporal(temporal_to_ref(t)),
    }
}

/// The left inverse of [`to_ref`]: a production value whose image may be
/// `x`, or `None` for a reference value of a form production has no
/// variant or constructor for. [`into_production`] keeps only exact
/// preimages.
fn from_ref(x: &RValue) -> Option<Value> {
    let fields = |m: &BTreeMap<String, RValue>| -> Option<HashMap<String, Value>> {
        m.iter()
            .map(|(k, v)| from_ref(v).map(|v| (k.clone(), v)))
            .collect()
    };
    Some(match x {
        RValue::Bool(b) => Value::Bool(*b),
        RValue::Int(i) => Value::Int(*i),
        RValue::Float(bits) => Value::Float(f64::from_bits(*bits)),
        RValue::Str(s) => Value::Str(s.clone()),
        RValue::Bytes(b) => Value::Bytes(b.clone()),
        RValue::CidLink(c) => Value::CidLink(c.clone()),
        RValue::Blob { ref_, mime, size } => Value::Blob {
            ref_: ref_.clone(),
            mime: mime.clone(),
            size: *size,
        },
        RValue::Token(t) => Value::Token(t.clone()),
        RValue::Null => Value::Null,
        RValue::Opaque { type_, fields: f } => Value::Opaque {
            type_: type_.clone(),
            fields: fields(f)?,
        },
        RValue::Unknown(f) => Value::Unknown(fields(f)?),
        RValue::List(items) => Value::List(items.iter().map(from_ref).collect::<Option<_>>()?),
        RValue::LabeledNull(n) => Value::LabeledNull(*n),
        RValue::Decimal(rx::Decimal::Finite(d)) => Value::Decimal(
            DecimalValue::new(d.negative, d.digits.to_string(), wide_of(&d.exponent)).ok()?,
        ),
        RValue::Decimal(rx::Decimal::Infinite { negative }) => {
            Value::Decimal(DecimalValue::infinity(*negative))
        }
        RValue::Decimal(rx::Decimal::NaN {
            negative,
            signalling,
            payload,
        }) => Value::Decimal(DecimalValue::nan(
            *negative,
            *signalling,
            payload.to_string(),
        )),
        RValue::BigInt(b) => Value::BigInt(BigIntValue {
            negative: b.negative,
            significand: b.significand.to_string(),
            zeros: WideInt::from_sign_and_digits(false, &b.zeros.to_string())?,
        }),
        RValue::Temporal(t) => Value::Temporal(temporal_from_ref(t)?),
    })
}

/// The production value whose image is exactly `x`, if there is one.
fn into_production(x: &RValue) -> Option<Value> {
    from_ref(x).filter(|p| to_ref(p) == *x)
}

/// Equality of production values with floats compared by their bits, so
/// that a NaN equals itself and the two zeros differ.
fn same(left: &Value, right: &Value) -> bool {
    let fields = |mine: &HashMap<String, Value>, theirs: &HashMap<String, Value>| {
        mine.len() == theirs.len()
            && mine
                .iter()
                .all(|(key, value)| theirs.get(key).is_some_and(|other| same(value, other)))
    };
    match (left, right) {
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(),
        (
            Value::Opaque {
                type_: left_type,
                fields: left_fields,
            },
            Value::Opaque {
                type_: right_type,
                fields: right_fields,
            },
        ) => left_type == right_type && fields(left_fields, right_fields),
        (Value::Unknown(left_fields), Value::Unknown(right_fields)) => {
            fields(left_fields, right_fields)
        }
        (Value::List(left_items), Value::List(right_items)) => {
            left_items.len() == right_items.len()
                && left_items
                    .iter()
                    .zip(right_items)
                    .all(|(item, other)| same(item, other))
        }
        _ => left == right,
    }
}

const fn bytes_text_to_ref(t: &BytesText) -> r::BytesText {
    match t {
        BytesText::Base64 => r::BytesText::Base64,
        BytesText::Base64Url => r::BytesText::Base64Url,
        BytesText::Base64Padded => r::BytesText::Base64Padded,
        BytesText::Base64UrlPadded => r::BytesText::Base64UrlPadded,
        BytesText::Hex => r::BytesText::Hex,
        BytesText::Raw => r::BytesText::Raw,
        BytesText::Latin1 => r::BytesText::Latin1,
    }
}

const fn kind_to_ref(k: &ScalarKind) -> r::ScalarKind {
    match k {
        ScalarKind::Bool => r::ScalarKind::Bool,
        ScalarKind::Int { bits, signed } => r::ScalarKind::Int {
            bits: *bits,
            signed: *signed,
        },
        ScalarKind::BigInt => r::ScalarKind::BigInt,
        ScalarKind::Float { bits } => r::ScalarKind::Float { bits: *bits },
        ScalarKind::Decimal => r::ScalarKind::Decimal,
        ScalarKind::Str => r::ScalarKind::Str,
        ScalarKind::Bytes(t) => r::ScalarKind::Bytes(bytes_text_to_ref(t)),
        ScalarKind::Date => r::ScalarKind::Date,
        ScalarKind::Time => r::ScalarKind::Time,
        ScalarKind::DateTime => r::ScalarKind::DateTime,
        ScalarKind::Token => r::ScalarKind::Token,
        ScalarKind::Any => r::ScalarKind::Any,
    }
}

const fn kind_from_ref(k: r::ScalarKind) -> ScalarKind {
    match k {
        r::ScalarKind::Bool => ScalarKind::Bool,
        r::ScalarKind::Int { bits, signed } => ScalarKind::Int { bits, signed },
        r::ScalarKind::BigInt => ScalarKind::BigInt,
        r::ScalarKind::Float { bits } => ScalarKind::Float { bits },
        r::ScalarKind::Decimal => ScalarKind::Decimal,
        r::ScalarKind::Str => ScalarKind::Str,
        r::ScalarKind::Bytes(r::BytesText::Base64) => ScalarKind::Bytes(BytesText::Base64),
        r::ScalarKind::Bytes(r::BytesText::Base64Url) => ScalarKind::Bytes(BytesText::Base64Url),
        r::ScalarKind::Bytes(r::BytesText::Base64Padded) => {
            ScalarKind::Bytes(BytesText::Base64Padded)
        }
        r::ScalarKind::Bytes(r::BytesText::Base64UrlPadded) => {
            ScalarKind::Bytes(BytesText::Base64UrlPadded)
        }
        r::ScalarKind::Bytes(r::BytesText::Hex) => ScalarKind::Bytes(BytesText::Hex),
        r::ScalarKind::Bytes(r::BytesText::Raw) => ScalarKind::Bytes(BytesText::Raw),
        r::ScalarKind::Bytes(r::BytesText::Latin1) => ScalarKind::Bytes(BytesText::Latin1),
        r::ScalarKind::Date => ScalarKind::Date,
        r::ScalarKind::Time => ScalarKind::Time,
        r::ScalarKind::DateTime => ScalarKind::DateTime,
        r::ScalarKind::Token => ScalarKind::Token,
        r::ScalarKind::Any => ScalarKind::Any,
    }
}

const fn reading_to_ref(reading: FloatReading) -> r::FloatReading {
    match reading {
        FloatReading::Exact => r::FloatReading::Exact,
        FloatReading::NearestKeepingLexeme => r::FloatReading::NearestKeepingLexeme,
    }
}

const fn dialect_to_ref(dialect: NumeralDialect) -> r::NumeralDialect {
    match dialect {
        NumeralDialect::Json => r::NumeralDialect::Json,
        NumeralDialect::Toml => r::NumeralDialect::Toml,
        NumeralDialect::Yaml12 => r::NumeralDialect::Yaml12,
        NumeralDialect::Yaml11 => r::NumeralDialect::Yaml11,
        NumeralDialect::Xsd => r::NumeralDialect::Xsd,
        NumeralDialect::Java => r::NumeralDialect::Java,
        NumeralDialect::ProtoJson => r::NumeralDialect::ProtoJson,
        NumeralDialect::Sql => r::NumeralDialect::Sql,
    }
}

const DIALECTS: [NumeralDialect; 8] = [
    NumeralDialect::Json,
    NumeralDialect::Toml,
    NumeralDialect::Yaml12,
    NumeralDialect::Yaml11,
    NumeralDialect::Xsd,
    NumeralDialect::Java,
    NumeralDialect::ProtoJson,
    NumeralDialect::Sql,
];

/// The conversion of a production spec. Its domain is the specs with no
/// `max_length`, discriminator table, time unit or time zone, the only
/// specs the scope holds: neither coercion reads a length bound, the
/// reference's spec has no table (the discriminated reading is compared
/// separately) and no coercion reads a time unit or zone.
fn spec_to_ref(s: &ValueSpec) -> r::ValueSpec {
    assert!(
        s.max_length.is_none()
            && s.discriminators.is_none()
            && s.time_unit.is_none()
            && s.time_zone.is_none(),
        "the conversion's domain"
    );
    r::ValueSpec {
        kind: kind_to_ref(&s.kind),
        precision: s.precision,
        scale: s.scale,
        with_time_zone: s.with_time_zone,
        int_bits: s.int_bits,
        unsigned: s.unsigned,
        float_reading: reading_to_ref(s.float_reading),
        numeral: s.numeral.map(dialect_to_ref),
    }
}

/// The production spec whose image is exactly `s`, if there is one.
fn spec_into_production(s: &r::ValueSpec) -> Option<ValueSpec> {
    let reading = match s.float_reading {
        r::FloatReading::Exact => FloatReading::Exact,
        r::FloatReading::NearestKeepingLexeme => FloatReading::NearestKeepingLexeme,
    };
    let numeral = match s.numeral {
        None => None,
        Some(d) => Some(*DIALECTS.iter().find(|p| dialect_to_ref(**p) == d)?),
    };
    let p = ValueSpec {
        kind: kind_from_ref(s.kind),
        precision: s.precision,
        scale: s.scale,
        with_time_zone: s.with_time_zone,
        int_bits: s.int_bits,
        unsigned: s.unsigned,
        float_reading: reading,
        numeral,
        ..ValueSpec::new(ScalarKind::Any)
    };
    (spec_to_ref(&p) == *s).then_some(p)
}

const fn reason_to_ref(reason: &FidelityReason) -> r::FidelityReason {
    match reason {
        FidelityReason::Overflow => r::FidelityReason::Overflow,
        FidelityReason::PrecisionExceeded { max } => {
            r::FidelityReason::PrecisionExceeded { max: *max }
        }
        FidelityReason::ScaleExceeded { max } => r::FidelityReason::ScaleExceeded { max: *max },
        FidelityReason::NotANumber => r::FidelityReason::NotANumber,
        FidelityReason::NonFinite => r::FidelityReason::NonFinite,
        FidelityReason::InvalidTemporal => r::FidelityReason::InvalidTemporal,
        FidelityReason::OffsetRequired => r::FidelityReason::OffsetRequired,
        FidelityReason::OffsetForbidden => r::FidelityReason::OffsetForbidden,
        FidelityReason::InvalidEncoding => r::FidelityReason::InvalidEncoding,
        FidelityReason::NotANumberForKind => r::FidelityReason::NotANumberForKind,
    }
}

fn class_to_ref(c: &LexClass) -> r::LexClass {
    match c {
        LexClass::Number => r::LexClass::Number,
        LexClass::String => r::LexClass::String,
        LexClass::Bool => r::LexClass::Bool,
        LexClass::Null => r::LexClass::Null,
        LexClass::DateTime => r::LexClass::DateTime,
        LexClass::Bytes => r::LexClass::Bytes,
        LexClass::Tagged(tag) => r::LexClass::Tagged(tag.as_ref().to_owned()),
    }
}

// ---------------------------------------------------------------------------
// The scope.
// ---------------------------------------------------------------------------

const TAGS: &[&str] = &[
    "int",
    "!!float",
    "tag:yaml.org,2002:str",
    "null",
    "!!null",
    "binary",
    "hex",
    "date",
    "time",
    "timestamp",
    "bool",
    "decimal",
    "numberLong",
    "numberDouble",
    "custom",
    "!!custom",
];

fn dedupe<T: Clone + Eq + std::hash::Hash>(items: Vec<T>) -> Vec<T> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|x| seen.insert(x.clone()))
        .collect()
}

/// The label of a result, for the counts.
fn label(result: &Result<RValue, r::FidelityReason>) -> String {
    match result {
        Ok(_) => "value".to_owned(),
        Err(reason) => format!("{reason:?}"),
    }
}

/// The variant of a reference value, for the disagreement classes.
fn value_class(x: &RValue) -> String {
    let debug = format!("{x:?}");
    debug
        .split(['(', ' ', '{'])
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// The label of a reference spec's kind, for the counts.
fn kind_label(s: &r::ValueSpec) -> String {
    format!("{:?}", s.kind)
}

/// The tallies one differential run prints.
#[derive(Default)]
struct Tally {
    compared: u64,
    outside_production: u64,
    outside_reference: u64,
    by_outcome: BTreeMap<String, u64>,
    by_kind: BTreeMap<String, u64>,
    disagreements: BTreeMap<String, (u64, Vec<String>)>,
    disagreement_count: u64,
}

impl Tally {
    fn record(
        &mut self,
        spec: &r::ValueSpec,
        input: &dyn std::fmt::Debug,
        input_class: impl FnOnce() -> String,
        production: &Result<RValue, r::FidelityReason>,
        reference: &Result<RValue, r::FidelityReason>,
    ) {
        self.compared += 1;
        *self.by_outcome.entry(label(reference)).or_default() += 1;
        *self.by_kind.entry(kind_label(spec)).or_default() += 1;
        if production != reference {
            self.disagreement_count += 1;
            let class = format!(
                "{} / {} / production {} / reference {}",
                kind_label(spec),
                input_class(),
                label(production),
                label(reference)
            );
            let entry = self.disagreements.entry(class).or_default();
            entry.0 += 1;
            if entry.1.len() < 2 {
                entry.1.push(format!(
                    "{input:?} under {spec:?}: production {production:?}, reference {reference:?}"
                ));
            }
        }
    }

    fn print(&self, name: &str, total: u64, started: Instant) {
        println!("{name}: total cases {total}");
        println!("{name}: compared {}", self.compared);
        println!(
            "{name}: outside the production model {}",
            self.outside_production
        );
        println!(
            "{name}: outside the reference domain {}",
            self.outside_reference
        );
        for (k, n) in &self.by_kind {
            println!("{name}: compared under {k}: {n}");
        }
        for (k, n) in &self.by_outcome {
            println!("{name}: outcome {k}: {n}");
        }
        println!("{name}: disagreements {}", self.disagreement_count);
        for (class, (n, examples)) in &self.disagreements {
            println!("{name}: disagreement class {class}: {n}");
            for e in examples {
                println!("{name}:   e.g. {e}");
            }
        }
        println!("{name}: elapsed {:.1?}", started.elapsed());
        assert_eq!(
            total,
            self.compared + self.outside_production + self.outside_reference,
            "{name}: every case is compared or counted outside"
        );
        assert_eq!(self.disagreement_count, 0, "{name}: disagreements");
    }
}

// ---------------------------------------------------------------------------
// The tests.
// ---------------------------------------------------------------------------

#[test]
fn the_value_conversion_is_injective_on_the_scope() {
    let values = scope::coercion_values();
    let mut images = HashSet::new();
    let mut held = 0u64;
    let mut represented = 0u64;
    let mut previous: Option<(&RValue, Vec<u8>)> = None;
    let mut neighbours = 0u64;
    for x in &values {
        let Some(p) = into_production(x) else {
            continue;
        };
        held += 1;
        let back = from_ref(&to_ref(&p)).expect("an image has a preimage");
        assert!(same(&back, &p), "from_ref(to_ref(p)) = p for {p:?}");
        assert!(
            images.insert(to_ref(&p)),
            "two values with one image: {p:?}"
        );
        // Representation equality is canonical-byte equality: the image of
        // the production value has the representation of its preimage, and
        // two successive values of the scope are equal in representation
        // exactly when their production canonical bytes are equal.
        let bytes = p.canonical_bytes();
        if let Some(equal) = rv::representation(x, &to_ref(&p)) {
            assert!(equal, "the image of {p:?} changed its representation");
            represented += 1;
            if let Some((before, before_bytes)) = &previous {
                if let Some(equal) = rv::representation(before, x) {
                    assert_eq!(equal, *before_bytes == bytes, "{before:?} against {x:?}");
                    neighbours += 1;
                }
            }
            previous = Some((x, bytes));
        }
    }
    let specs = scope::coercion_specs();
    let mut held_specs = 0u64;
    for s in &specs {
        if let Some(p) = spec_into_production(s) {
            held_specs += 1;
            assert_eq!(spec_into_production(&spec_to_ref(&p)), Some(p));
        }
    }
    let reasons = [
        FidelityReason::Overflow,
        FidelityReason::PrecisionExceeded { max: 5 },
        FidelityReason::PrecisionExceeded { max: 9 },
        FidelityReason::PrecisionExceeded { max: 17 },
        FidelityReason::ScaleExceeded { max: 0 },
        FidelityReason::ScaleExceeded { max: 1 },
        FidelityReason::NotANumber,
        FidelityReason::NonFinite,
        FidelityReason::InvalidTemporal,
        FidelityReason::OffsetRequired,
        FidelityReason::OffsetForbidden,
        FidelityReason::InvalidEncoding,
        FidelityReason::NotANumberForKind,
    ];
    let images: HashSet<r::FidelityReason> = reasons.iter().map(reason_to_ref).collect();
    assert_eq!(images.len(), reasons.len());
    println!(
        "conversion: {} reference values, {held} held by production, all round-tripped and pairwise distinct; \
         {represented} in the canonical encoder's domain keep their representation, \
         {neighbours} successive pairs compare in representation as by canonical bytes",
        values.len()
    );
    assert!(represented > 0 && neighbours + 1 == represented);
    println!(
        "conversion: {} reference specs, {held_specs} held by production, all round-tripped",
        specs.len()
    );
    println!(
        "conversion: {} refusal reasons, pairwise distinct",
        reasons.len()
    );
}

#[test]
fn a_complete_local_date_requires_an_offset_when_declared() {
    let spec = ValueSpec {
        with_time_zone: Some(true),
        ..ValueSpec::new(ScalarKind::Date)
    };
    let class = LexClass::DateTime;
    let production =
        coerce(&Lexeme::new("2000-01-02", class.clone()), &spec).map_err(|error| error.reason);
    assert_eq!(production, Err(FidelityReason::OffsetRequired));

    let reference = r::coerce("2000-01-02", &class_to_ref(&class), &spec_to_ref(&spec))
        .expect("a complete date is in the reference coercion domain");
    assert_eq!(reference, Err(r::FidelityReason::OffsetRequired));
}

/// The number of worker threads a differential run uses: the host's
/// available parallelism, at most eight.
fn workers() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get().min(8))
}

/// Run `case` on every spec and every chunk of `CHUNK` consecutive indices
/// below `items`, the work items shared among [`workers`] threads, so that a
/// spec whose cases are slow is spread over every thread.
fn over_spec_chunks(
    specs: &[r::ValueSpec],
    items: usize,
    case: impl Fn(&r::ValueSpec, std::ops::Range<usize>, &mut Tally) + Sync,
) -> Tally {
    const CHUNK: usize = 4096;
    let chunks = items.div_ceil(CHUNK);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let tallies: Vec<Tally> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..workers())
            .map(|_| {
                s.spawn(|| {
                    let mut tally = Tally::default();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(spec) = specs.get(i / chunks.max(1)) else {
                            return tally;
                        };
                        let start = (i % chunks.max(1)) * CHUNK;
                        case(spec, start..(start + CHUNK).min(items), &mut tally);
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().expect("worker"))
            .collect()
    });
    merge_tallies(tallies)
}

fn over_specs(specs: &[r::ValueSpec], case: impl Fn(&r::ValueSpec, &mut Tally) + Sync) -> Tally {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let tallies: Vec<Tally> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..workers())
            .map(|_| {
                s.spawn(|| {
                    let mut tally = Tally::default();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(spec) = specs.get(i) else {
                            return tally;
                        };
                        let started = Instant::now();
                        let before = tally.disagreement_count;
                        case(spec, &mut tally);
                        println!(
                            "spec {i} {spec:?}: {:.1?}, disagreements {}",
                            started.elapsed(),
                            tally.disagreement_count - before
                        );
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().expect("worker"))
            .collect()
    });
    merge_tallies(tallies)
}

/// The sum of the workers' tallies.
fn merge_tallies(tallies: Vec<Tally>) -> Tally {
    let mut all = Tally::default();
    for t in tallies {
        all.compared += t.compared;
        all.outside_production += t.outside_production;
        all.outside_reference += t.outside_reference;
        for (k, n) in t.by_outcome {
            *all.by_outcome.entry(k).or_default() += n;
        }
        for (k, n) in t.by_kind {
            *all.by_kind.entry(k).or_default() += n;
        }
        all.disagreement_count += t.disagreement_count;
        for (class, (n, examples)) in t.disagreements {
            let entry = all.disagreements.entry(class).or_default();
            entry.0 += n;
            entry.1.extend(examples);
            entry.1.truncate(2);
        }
    }
    all
}

/// The fixed points of value coercion under a declared kind that is neither
/// `Any` nor a float kind: a value is fixed exactly when the direct coercion
/// of its own lexeme under the dialect-free specification gives it back, or
/// when it has no lexeme and is not a non-finite float.
fn check_fixed_point(
    x: &RValue,
    spec: &r::ValueSpec,
    reference: &Result<RValue, r::FidelityReason>,
) {
    if matches!(spec.kind, r::ScalarKind::Any | r::ScalarKind::Float { .. }) {
        return;
    }
    let fixed = reference.as_ref() == Ok(x);
    let plain = r::ValueSpec {
        numeral: None,
        ..*spec
    };
    let expected = match r::lexeme_of(x, spec.kind) {
        Some((text, class)) => r::coerce(&text, &class, &plain).as_ref() == Some(&Ok(x.clone())),
        None => !matches!(x, RValue::Float(bits) if !f64::from_bits(*bits).is_finite()),
    };
    assert_eq!(fixed, expected, "fixed point of {x:?} under {spec:?}");
}

#[test]
fn coerce_value_agrees_with_the_reference_over_the_values_scope() {
    let started = Instant::now();
    let values = scope::coercion_values();
    let specs = scope::coercion_specs();
    // The stated bounds, written out so that a dropped or duplicated element
    // of either enumeration fails here rather than shrinking the sweep.
    assert_eq!(values.len(), 337_876);
    assert_eq!(specs.len(), 81);
    let held: Vec<(&RValue, Option<Value>)> =
        values.iter().map(|x| (x, into_production(x))).collect();
    let total = (values.len() * specs.len()) as u64;
    println!(
        "coerce_value: bound {} reference values by {} reference specs",
        values.len(),
        specs.len()
    );
    let tally = over_spec_chunks(&specs, held.len(), |spec, range, tally| {
        let Some(production_spec) = spec_into_production(spec) else {
            tally.outside_production += range.len() as u64;
            return;
        };
        for (x, p) in &held[range] {
            let Some(p) = p else {
                tally.outside_production += 1;
                continue;
            };
            let Some(reference) = r::coerce_value(x, spec) else {
                tally.outside_reference += 1;
                continue;
            };
            check_fixed_point(x, spec, &reference);
            let production = coerce_value(p, &production_spec, ValueSite::unknown())
                .map(|v| to_ref(&v))
                .map_err(|e| reason_to_ref(&e.reason));
            tally.record(spec, x, || value_class(x), &production, &reference);
        }
    });
    tally.print("coerce_value", total, started);
}

#[test]
fn coerce_agrees_with_the_reference_over_the_values_scope() {
    let started = Instant::now();
    let lexemes = scope::coercion_lexemes();
    let specs = scope::coercion_specs();
    assert_eq!(lexemes.len(), 20_986);
    assert_eq!(specs.len(), 81);
    let mut classes = vec![
        LexClass::Number,
        LexClass::String,
        LexClass::Bool,
        LexClass::Null,
        LexClass::DateTime,
        LexClass::Bytes,
    ];
    classes.extend(TAGS.iter().map(|t| LexClass::Tagged((*t).into())));
    let total = (lexemes.len() * classes.len() * specs.len()) as u64;
    println!(
        "coerce: bound {} lexemes by {} classes by {} reference specs",
        lexemes.len(),
        classes.len(),
        specs.len()
    );
    let tally = over_specs(&specs, |spec, tally| {
        let Some(production_spec) = spec_into_production(spec) else {
            tally.outside_production += (lexemes.len() * classes.len()) as u64;
            return;
        };
        for class in &classes {
            let rclass = class_to_ref(class);
            for t in &lexemes {
                let Some(reference) = r::coerce(t, &rclass, spec) else {
                    tally.outside_reference += 1;
                    continue;
                };
                let production = coerce(&Lexeme::new(t, class.clone()), &production_spec)
                    .map(|v| to_ref(&v))
                    .map_err(|e| reason_to_ref(&e.reason));
                tally.record(
                    spec,
                    &(t, class),
                    || format!("{class:?}"),
                    &production,
                    &reference,
                );
            }
        }
    });
    tally.print("coerce", total, started);
}

// ---------------------------------------------------------------------------
// Binary16.
// ---------------------------------------------------------------------------

/// The outcome of production `coerce` in the reference model.
fn coerce_to_ref(
    text: &str,
    class: &LexClass,
    spec: &ValueSpec,
) -> Result<RValue, r::FidelityReason> {
    coerce(&Lexeme::new(text, class.clone()), spec)
        .map(|v| to_ref(&v))
        .map_err(|e| reason_to_ref(&e.reason))
}

/// The outcome of production `coerce_value` in the reference model.
fn coerce_value_to_ref(value: &Value, spec: &ValueSpec) -> Result<RValue, r::FidelityReason> {
    coerce_value(value, spec, ValueSite::unknown())
        .map(|v| to_ref(&v))
        .map_err(|e| reason_to_ref(&e.reason))
}

/// Under `Float { bits: 16 }`, with each reading: every binary16 pattern,
/// widened, and its two binary64 neighbours are coerced as values; and the
/// closest-shortest text of every finite pattern, that text with its last
/// digit moved one either way, and every numeral of the values page are read
/// as lexemes. Every outcome agrees with the reference's.
#[test]
fn float16_coercion_agrees_with_the_reference() {
    let mut values = 0_u64;
    let mut texts = Vec::new();
    for h in scope::binary16_patterns() {
        let widened = widen_bits(Format::B16, h);
        for bits in [widened, widened.wrapping_add(1), widened.wrapping_sub(1)] {
            let x = RValue::Float(bits);
            let p = from_ref(&x).unwrap();
            for reading in [FloatReading::Exact, FloatReading::NearestKeepingLexeme] {
                let spec = ValueSpec {
                    float_reading: reading,
                    ..ValueSpec::new(ScalarKind::Float { bits: 16 })
                };
                let Some(reference) = r::coerce_value(&x, &spec_to_ref(&spec)) else {
                    continue;
                };
                assert_eq!(
                    coerce_value_to_ref(&p, &spec),
                    reference,
                    "{bits:#x} {reading:?}"
                );
                values += 1;
            }
        }
        if let Some(shortest) = panproto_reference::float::shortest(Format::B16, h) {
            let text = shortest.to_scientific();
            let (mantissa, exponent) = text.split_once('e').unwrap();
            let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
            let n: u64 = digits.parse().unwrap();
            let point = i64::try_from(digits.len()).unwrap() - 1;
            let exponent: i64 = exponent.parse::<i64>().unwrap() - point;
            let sign = if shortest.negative { "-" } else { "" };
            texts.push(text.clone());
            texts.push(format!("{sign}{}e{exponent}", n + 1));
            if n > 0 {
                texts.push(format!("{sign}{}e{exponent}", n - 1));
            }
        }
    }
    texts.extend(scope::numerals());
    texts.extend(scope::edge_numerals());
    let texts = dedupe(texts);
    let mut lexemes = 0_u64;
    for text in &texts {
        for reading in [FloatReading::Exact, FloatReading::NearestKeepingLexeme] {
            let spec = ValueSpec {
                float_reading: reading,
                ..ValueSpec::new(ScalarKind::Float { bits: 16 })
            };
            let reference = r::coerce(text, &r::LexClass::Number, &spec_to_ref(&spec)).unwrap();
            assert_eq!(
                coerce_to_ref(text, &LexClass::Number, &spec),
                reference,
                "{text} {reading:?}"
            );
            lexemes += 1;
        }
    }
    println!(
        "binary16: bound 65536 patterns with 2 neighbours each under 2 readings: {values} value cases; \
         {} lexemes under 2 readings: {lexemes} lexeme cases",
        texts.len()
    );
    assert_eq!(lexemes, 2 * texts.len() as u64);
}

/// Every pattern of the binary32 scope (the boundary patterns and the NaNs)
/// widens to a binary64 value that narrows back to it, in production
/// (`binary32_bits`) as in the reference (`narrow_bits`), and coercing that
/// binary64 value under `Float { bits: 32 }` keeps its bits; the same value
/// with its lowest binary64 fraction bit set is not a binary32 value, both
/// narrowings refuse it, and both coercions refuse it alike.
#[test]
fn binary32_nan_bits_agree_with_the_reference() {
    let spec = ValueSpec::new(ScalarKind::Float { bits: 32 });
    let patterns = scope::binary32_scope();
    let nans = scope::binary32_nans();
    let boundary = scope::binary32_boundary_patterns();
    let mut cases = 0_u64;
    for &narrow in &patterns {
        let wide = widen_bits(Format::B32, narrow);
        assert_eq!(
            narrow_bits(Format::B32, wide),
            Some(narrow),
            "{narrow:#010x}"
        );
        for bits in [wide, wide | 1] {
            assert_eq!(
                panproto_expr::exact::binary32_bits(f64::from_bits(bits)).map(u64::from),
                narrow_bits(Format::B32, bits),
                "{bits:#018x}"
            );
            let x = RValue::Float(bits);
            let p = from_ref(&x).unwrap();
            let reference = r::coerce_value(&x, &spec_to_ref(&spec))
                .unwrap_or_else(|| panic!("{bits:#018x} is outside the reference"));
            let production = coerce_value_to_ref(&p, &spec);
            assert_eq!(production, reference, "{bits:#018x}");
            if bits == wide {
                assert_eq!(production, Ok(RValue::Float(wide)), "{narrow:#010x}");
            } else {
                assert_eq!(narrow_bits(Format::B32, bits), None, "{bits:#018x}");
                assert!(production.is_err(), "{bits:#018x}");
            }
            cases += 1;
        }
    }
    println!(
        "bound: patterns={}; binary32: {} boundary patterns and {} NaNs ({} shared), each with a \
         low-bit neighbour: {cases} cases, all agree",
        patterns.len(),
        boundary.len(),
        nans.len(),
        boundary.len() + nans.len() - patterns.len()
    );
    assert_eq!(boundary.len(), 2 * 256 * 8);
    // A binary16 NaN carries its fraction to the top ten binary32 fraction
    // bits, so of the probes only the quiet NaN with payload 0 (one of each
    // sign) is among the carried binary16 NaNs.
    assert_eq!(nans.len(), 2 * 1023 + 2 * 9 - 2);
    // The boundary NaNs of each sign are those of fractions 1, 2, 2^22 - 1,
    // 2^22, 2^22 + 1, 2^23 - 2 and 2^23 - 1; all but 2^23 - 2 are NaNs of
    // the list.
    assert_eq!(patterns.len(), boundary.len() + nans.len() - 2 * 6);
    assert_eq!(cases, 2 * patterns.len() as u64);
}

// ---------------------------------------------------------------------------
// Numeral dialects and discriminators.
// ---------------------------------------------------------------------------

/// The numeral-dialect probes of the reference enumerator, as texts.
fn dialect_probes() -> &'static [&'static str] {
    static PROBES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    PROBES.get_or_init(|| {
        scope::dialect_probes()
            .into_iter()
            .map(|text| &*Box::leak(text.into_boxed_str()))
            .collect()
    })
}

/// Each dialect, and none, over every probe of [`dialect_probes`] as a
/// number and as a string, under the numeric kinds, `Any` and `Str`, agrees
/// with the reference.
#[test]
fn dialect_coercion_agrees_with_the_reference() {
    let kinds = [
        ScalarKind::Int {
            bits: 64,
            signed: true,
        },
        ScalarKind::Int {
            bits: 8,
            signed: false,
        },
        ScalarKind::BigInt,
        ScalarKind::Float { bits: 64 },
        ScalarKind::Float { bits: 32 },
        ScalarKind::Decimal,
        ScalarKind::Any,
        ScalarKind::Str,
    ];
    let mut cases = 0_u64;
    for dialect in std::iter::once(None).chain(DIALECTS.iter().copied().map(Some)) {
        for kind in &kinds {
            let spec = ValueSpec {
                numeral: dialect,
                ..ValueSpec::new(kind.clone())
            };
            for class in [LexClass::Number, LexClass::String] {
                for probe in dialect_probes() {
                    let reference =
                        r::coerce_dialect(probe, &class_to_ref(&class), &spec_to_ref(&spec))
                            .unwrap();
                    assert_eq!(
                        coerce_to_ref(probe, &class, &spec),
                        reference,
                        "{probe:?} {class:?} under {dialect:?} {kind:?}"
                    );
                    cases += 1;
                }
            }
        }
    }
    println!(
        "dialects: bound 9 dialects (8 and none) by {} kinds by 2 classes by {} probes: {cases} cases",
        kinds.len(),
        dialect_probes().len()
    );
    assert_eq!(
        cases,
        9 * kinds.len() as u64 * 2 * dialect_probes().len() as u64
    );
    // The dialect table's examples, the listed malformed spellings and the
    // grammars' edge forms, as the enumerator lists them.
    assert_eq!(dialect_probes().len(), 92);
    println!("bound: probes={}", dialect_probes().len());
}

/// Coercion is idempotent under every numeral dialect: a dialect reads
/// source text, so a value coerced from a string is a fixed point of the
/// same specification, in production and in the reference alike, on every
/// dialect probe and the special spellings under the integer and decimal
/// kinds.
#[test]
fn a_coerced_value_is_a_fixed_point_under_every_dialect() {
    let kinds = [
        ScalarKind::Int {
            bits: 64,
            signed: true,
        },
        ScalarKind::BigInt,
        ScalarKind::Decimal,
    ];
    let probes: Vec<&str> = dialect_probes().to_vec();
    let mut fixed = 0_u64;
    let mut cases = 0_u64;
    for dialect in std::iter::once(None).chain(DIALECTS.iter().copied().map(Some)) {
        for kind in &kinds {
            for unsigned in [false, true] {
                let spec = ValueSpec {
                    numeral: dialect,
                    unsigned,
                    ..ValueSpec::new(kind.clone())
                };
                for probe in &probes {
                    let source = Value::Str((*probe).to_owned());
                    let reference_spec = spec_to_ref(&spec);
                    let first = coerce_value_to_ref(&source, &spec);
                    // A string is source text: its coercion is the reading of
                    // its lexeme under the specification, dialect included.
                    assert_eq!(
                        r::coerce_value(&to_ref(&source), &reference_spec),
                        r::coerce(probe, &r::LexClass::String, &reference_spec),
                        "{probe:?} under {dialect:?} {kind:?}"
                    );
                    assert_eq!(
                        Some(first.clone()),
                        r::coerce_value(&to_ref(&source), &reference_spec),
                        "{probe:?} under {dialect:?} {kind:?}"
                    );
                    if let Ok(value) = &first {
                        let second = coerce_value_to_ref(&from_ref(value).unwrap(), &spec);
                        assert_eq!(second, first, "{probe:?} under {dialect:?} {kind:?}");
                        assert_eq!(
                            r::coerce_value(value, &reference_spec),
                            Some(first.clone()),
                            "{probe:?} under {dialect:?} {kind:?}"
                        );
                        fixed += 1;
                    }
                    cases += 1;
                }
            }
        }
    }
    println!(
        "idempotence: 9 dialect settings by {} kinds by 2 signedness flags by {} probes: \
         {cases} cases, {fixed} coerced values, each a fixed point",
        kinds.len(),
        probes.len()
    );
    assert_eq!(cases, 9 * kinds.len() as u64 * 2 * probes.len() as u64);
}

/// Under the discriminator table {integer, float, text}, every tag of the
/// table, two unknown tags and no tag, crossed with every class and a set
/// of lexemes, under specs of several own kinds, agrees with the
/// reference's discriminated coercion.
#[test]
fn discriminated_coercion_agrees_with_the_reference() {
    let table: BTreeMap<String, ScalarKind> = BTreeMap::from([
        (
            "integer".to_owned(),
            ScalarKind::Int {
                bits: 32,
                signed: true,
            },
        ),
        ("float".to_owned(), ScalarKind::Float { bits: 64 }),
        ("text".to_owned(), ScalarKind::Str),
    ]);
    let reference_table: BTreeMap<String, r::ScalarKind> = table
        .iter()
        .map(|(tag, kind)| (tag.clone(), kind_to_ref(kind)))
        .collect();
    let tags = [
        None,
        Some("integer"),
        Some("float"),
        Some("text"),
        Some("x-custom"),
        Some(""),
    ];
    let lexemes = scope::tagged_lexemes();
    // A number, an exponent numeral, a fraction, a word, a boolean, the
    // empty text, a date and a base64 text.
    assert_eq!(lexemes.len(), 8);
    let mut classes = vec![
        LexClass::Number,
        LexClass::String,
        LexClass::Bool,
        LexClass::Null,
        LexClass::DateTime,
        LexClass::Bytes,
    ];
    classes.extend(["int", "custom"].map(|t| LexClass::Tagged(t.into())));
    let mut cases = 0_u64;
    for own in [ScalarKind::Any, ScalarKind::Str, ScalarKind::Decimal] {
        let spec = ValueSpec {
            discriminators: Some(table.clone()),
            ..ValueSpec::new(own.clone())
        };
        let plain = ValueSpec::new(own.clone());
        for tag in tags {
            for class in &classes {
                for text in &lexemes {
                    let production =
                        coerce_discriminated(&Lexeme::new(text, class.clone()), tag, &spec)
                            .map(|v| to_ref(&v))
                            .map_err(|e| reason_to_ref(&e.reason));
                    let reference = r::coerce_tagged(
                        text,
                        &class_to_ref(class),
                        tag,
                        &reference_table,
                        &spec_to_ref(&plain),
                    )
                    .unwrap();
                    assert_eq!(
                        production, reference,
                        "{text:?} {class:?} tagged {tag:?} under {own:?}"
                    );
                    cases += 1;
                }
            }
        }
    }
    println!(
        "discriminators: 3 own kinds by {} tags by {} classes by {} lexemes: {cases} cases",
        tags.len(),
        classes.len(),
        lexemes.len()
    );
    assert_eq!(
        cases,
        3 * (tags.len() * classes.len() * lexemes.len()) as u64
    );
}

// ---------------------------------------------------------------------------
// Generated cases.
// ---------------------------------------------------------------------------

proptest::proptest! {
    // The default configuration persists every shrunk counterexample under
    // `proptest-regressions/` and replays it first on the next run.
    #![proptest_config(proptest::test_runner::Config::default())]

    /// Generated lexemes over the numeral, temporal and byte-text alphabets,
    /// of every class, under every spec of the scope that production holds,
    /// and generated values under the same specs: production and reference
    /// agree on every case they both hold.
    #[test]
    fn randomized_coercion_differential(
        text in "[-+0-9.eE_xobINFAanfiyst:TZz /=]{0,14}",
        class_index in 0_usize..8,
        spec_index in 0_usize..1000,
        value_index in 0_usize..100_000,
    ) {
        let specs: Vec<(r::ValueSpec, ValueSpec)> = scope::coercion_specs()
            .into_iter()
            .filter_map(|s| spec_into_production(&s).map(|p| (s, p)))
            .collect();
        let (rspec, spec) = &specs[spec_index % specs.len()];
        let mut classes = vec![
            LexClass::Number,
            LexClass::String,
            LexClass::Bool,
            LexClass::Null,
            LexClass::DateTime,
            LexClass::Bytes,
        ];
        classes.extend(["int", "custom"].map(|t| LexClass::Tagged(t.into())));
        let class = &classes[class_index % classes.len()];
        if let Some(reference) = r::coerce(&text, &class_to_ref(class), rspec) {
            proptest::prop_assert_eq!(coerce_to_ref(&text, class, spec), reference, "{:?} {:?} {:?}", text, class, spec);
        }
        let values = randomized_values();
        let x = &values[value_index % values.len()];
        if let Some(p) = into_production(x) {
            let bytes = panproto_reference::canonical::value_bytes(x);
            let production_bytes = p.canonical_bytes();
            proptest::prop_assert_eq!(Some(production_bytes.as_slice()), bytes.as_deref(), "{:?}", x);
            if let Some(bytes) = bytes {
                proptest::prop_assert_eq!(
                    panproto_reference::canonical::decode_value(&bytes),
                    Some(x.clone()),
                    "{:?}",
                    x
                );
            }
            if let Some(reference) = r::coerce_value(x, rspec) {
                proptest::prop_assert_eq!(coerce_value_to_ref(&p, spec), reference, "{:?} {:?}", x, spec);
            }
        }
    }
}

/// The values the generated cases draw from: the reference reading of every
/// lexeme of the values page under each numeric and temporal kind, the
/// decimal special values, and the boundary floats of binary16.
fn randomized_values() -> &'static [RValue] {
    static VALUES: std::sync::OnceLock<Vec<RValue>> = std::sync::OnceLock::new();
    VALUES.get_or_init(|| {
        use r::ScalarKind as K;
        let mut out = Vec::new();
        for t in scope::coercion_words() {
            for kind in [
                K::Any,
                K::BigInt,
                K::Decimal,
                K::Float { bits: 16 },
                K::Float { bits: 32 },
                K::Float { bits: 64 },
            ] {
                if let Some(Ok(v)) = r::coerce(&t, &r::LexClass::Number, &r::ValueSpec::of(kind)) {
                    out.push(v);
                }
            }
            for kind in [K::Date, K::Time, K::DateTime] {
                if let Some(Ok(v)) = r::coerce(&t, &r::LexClass::DateTime, &r::ValueSpec::of(kind))
                {
                    out.push(v);
                }
            }
            out.push(RValue::Str(t));
        }
        out.extend(
            scope::binary16_patterns()
                .into_iter()
                .map(|h| RValue::Float(widen_bits(Format::B16, h))),
        );
        dedupe(out)
    })
}

// ---------------------------------------------------------------------------
// Representation and canonical bytes.
// ---------------------------------------------------------------------------

/// The number of values of `scope::canonical_values`, summed from its parts:
/// the decimals (five exponents by two signs by a hundred significands, two
/// infinities and eight NaNs), the byte strings of length at most one, the
/// temporal forms (twelve date bodies, fourteen time bodies and four dates
/// by eight times, each under eleven offsets, and two infinities of two
/// forms under five offsets), seven integers, two booleans and null, four
/// strings and four tokens, two signs by five exponent fields by four
/// fractions of binary16, three big integers, a link, a blob and an opaque
/// value, two records, four lists and two labelled nulls.
const fn canonical_value_count() -> usize {
    let decimals = 5 * 2 * 100 + 2 + 2 * 2 * 2;
    let byte_strings = 1 + 256;
    let temporals = 12 * 11 + 14 * 11 + 4 * 8 * 11 + 2 * 5 * 2;
    let integers = 7;
    let booleans_and_null = 3;
    let strings_and_tokens = 2 * 4;
    let floats = 2 * 5 * 4;
    let big_integers = 3;
    let link_blob_and_opaque = 3;
    let records = 2;
    let lists = 4;
    let labelled_nulls = 2;
    decimals
        + byte_strings
        + temporals
        + integers
        + booleans_and_null
        + strings_and_tokens
        + floats
        + big_integers
        + link_blob_and_opaque
        + records
        + lists
        + labelled_nulls
}

/// Every value of `scope::canonical_values` with its production preimage,
/// which every one of them has.
fn canonical_scope() -> Vec<(RValue, Value)> {
    scope::canonical_values()
        .into_iter()
        .map(|x| {
            let p = into_production(&x)
                .unwrap_or_else(|| panic!("{x:?} is outside the production model"));
            (x, p)
        })
        .collect()
}

/// Run `row` on every index below `rows` on [`workers`] threads and sum the
/// counts it returns.
fn sum_over_rows<const N: usize>(rows: usize, row: impl Fn(usize) -> [u64; N] + Sync) -> [u64; N] {
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        let workers: Vec<_> = (0..workers())
            .map(|_| {
                s.spawn(|| {
                    let mut sum = [0_u64; N];
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if i >= rows {
                            return sum;
                        }
                        for (total, n) in sum.iter_mut().zip(row(i)) {
                            *total += n;
                        }
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().expect("worker"))
            .fold([0_u64; N], |mut sum, part| {
                for (total, n) in sum.iter_mut().zip(part) {
                    *total += n;
                }
                sum
            })
    })
}

/// On every ordered pair of the canonical values, the reference's
/// representation equality is the equality of the production canonical
/// bytes of the two production preimages.
#[test]
fn representation_is_canonical_byte_equality_on_every_pair() {
    let started = Instant::now();
    let scope = canonical_scope();
    let n = scope.len();
    let bytes: Vec<Vec<u8>> = scope.iter().map(|(_, p)| p.canonical_bytes()).collect();
    let [equal, distinct] = sum_over_rows(n, |i| {
        let mut counts = [0_u64; 2];
        for j in 0..n {
            let production_equal = bytes[i] == bytes[j];
            assert_eq!(
                rv::representation(&scope[i].0, &scope[j].0),
                Some(production_equal),
                "{:?} against {:?}",
                scope[i].0,
                scope[j].0
            );
            counts[usize::from(!production_equal)] += 1;
        }
        counts
    });
    println!(
        "representation: bound {n} canonical values, {} ordered pairs: {equal} equal, \
         {distinct} distinct; elapsed {:.1?}",
        n * n,
        started.elapsed()
    );
    assert_eq!(n, canonical_value_count());
    assert_eq!(equal + distinct, (n * n) as u64);
    // No two distinct values of the scope share a representation.
    assert_eq!(equal, n as u64);
}

/// On every canonical value, production's canonical bytes are the
/// reference's canonical bytes, and the reference decodes them back to the
/// value.
#[test]
fn canonical_bytes_agree_with_the_reference() {
    let started = Instant::now();
    let scope = canonical_scope();
    let mut by_variant: BTreeMap<String, u64> = BTreeMap::new();
    let mut differing = 0_u64;
    for (x, p) in &scope {
        let reference = panproto_reference::canonical::value_bytes(x)
            .unwrap_or_else(|| panic!("{x:?} is outside the canonical encoder"));
        let production = p.canonical_bytes();
        if production != reference && differing == 0 {
            println!("canonical bytes: first difference at {x:?}");
            println!("canonical bytes:   production {production:02x?}");
            println!("canonical bytes:   reference  {reference:02x?}");
        }
        differing += u64::from(production != reference);
        assert_eq!(
            panproto_reference::canonical::decode_value(&reference).as_ref(),
            Some(x),
            "the reference decodes its own bytes of {x:?}"
        );
        *by_variant.entry(value_class(x)).or_default() += 1;
    }
    for (variant, count) in &by_variant {
        println!("canonical bytes: {variant}: {count}");
    }
    println!(
        "canonical bytes: bound {} canonical values, {differing} differ; elapsed {:.1?}",
        scope.len(),
        started.elapsed()
    );
    assert_eq!(scope.len(), canonical_value_count());
    assert_eq!(differing, 0);
}

// ---------------------------------------------------------------------------
// The nearest reading at binary32 and binary64.
// ---------------------------------------------------------------------------

/// The closest-shortest text of a finite pattern of `format`, and that text
/// with its last digit moved one either way (down only from a nonzero
/// significand); nothing for a NaN or an infinity.
fn shortest_with_neighbours(format: Format, bits: u64) -> Vec<String> {
    let Some(shortest) = panproto_reference::float::shortest(format, bits) else {
        return Vec::new();
    };
    let text = shortest.to_scientific();
    let (mantissa, exponent) = text.split_once('e').unwrap();
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let n: u64 = digits.parse().unwrap();
    let point = i64::try_from(digits.len()).unwrap() - 1;
    let exponent: i64 = exponent.parse::<i64>().unwrap() - point;
    let sign = if shortest.negative { "-" } else { "" };
    let mut out = vec![text.clone(), format!("{sign}{}e{exponent}", n + 1)];
    if n > 0 {
        out.push(format!("{sign}{}e{exponent}", n - 1));
    }
    out
}

/// A finite numeral accepted under the nearest reading at width `bits`
/// reads as the reference's nearest reading of its exact value, widened to
/// binary64.
fn check_nearest_value(text: &str, bits: u8, production: &Result<RValue, r::FidelityReason>) {
    let Some(rx::Decimal::Finite(decimal)) = rx::parse_decimal(text) else {
        return;
    };
    let rx::Scaled::Value(value) = decimal.scaled(-400, 400) else {
        return;
    };
    let format = Format::of_width(bits).unwrap();
    let nearest = panproto_reference::float::nearest_reading(format, &value, decimal.negative);
    assert_eq!(
        production,
        &Ok(RValue::Float(widen_bits(format, nearest.bits(format)))),
        "{text} at {bits}"
    );
}

/// Under `Float { bits: 32 }` and `Float { bits: 64 }`, with the nearest
/// reading and, for contrast, the exact one: the closest-shortest text of
/// every binary32 and every binary64 boundary pattern with its two
/// last-digit neighbours, and every numeral of the values page, read as
/// numbers. Every outcome agrees with the reference's.
#[test]
fn nearest_reading_coercion_agrees_with_the_reference() {
    let started = Instant::now();
    let (binary32, binary64, numerals) = scope::nearest_reading_scope();
    assert_eq!(binary32.len(), 2 * 256 * 8);
    assert_eq!(binary64.len(), 2 * 2048 * 14);
    let mut texts = Vec::new();
    for &bits in &binary32 {
        texts.extend(shortest_with_neighbours(Format::B32, bits));
    }
    for &bits in &binary64 {
        texts.extend(shortest_with_neighbours(Format::B64, bits));
    }
    assert_eq!(
        numerals.len() as u64,
        panproto_reference::enumerate::numerals_count(2, -1, 1)
    );
    assert_eq!(numerals.len(), 600);
    println!(
        "bound: binary32={}, binary64={}, numerals={}",
        binary32.len(),
        binary64.len(),
        numerals.len()
    );
    texts.extend(numerals);
    let texts = dedupe(texts);
    println!(
        "nearest: bound {} binary32 and {} binary64 boundary patterns, their shortest texts and \
         last-digit neighbours, and the values-page numerals: {} distinct texts",
        binary32.len(),
        binary64.len(),
        texts.len()
    );
    let class = LexClass::Number;
    let rclass = class_to_ref(&class);
    for bits in [32, 64] {
        for reading in [FloatReading::NearestKeepingLexeme, FloatReading::Exact] {
            let spec = ValueSpec {
                float_reading: reading,
                ..ValueSpec::new(ScalarKind::Float { bits })
            };
            let rspec = spec_to_ref(&spec);
            let tally = over_spec_chunks(&[rspec], texts.len(), |rspec, range, tally| {
                for text in &texts[range] {
                    let Some(reference) = r::coerce(text, &rclass, rspec) else {
                        tally.outside_reference += 1;
                        continue;
                    };
                    let production = coerce_to_ref(text, &class, &spec);
                    if reading == FloatReading::NearestKeepingLexeme {
                        check_nearest_value(text, bits, &production);
                    }
                    tally.record(rspec, text, || "Number".to_owned(), &production, &reference);
                }
            });
            tally.print(
                &format!("nearest Float {{ bits: {bits} }} {reading:?}"),
                texts.len() as u64,
                started,
            );
            assert_eq!(tally.outside_reference, 0);
        }
    }
}

// ---------------------------------------------------------------------------
// Generated discriminator tables and dialects.
// ---------------------------------------------------------------------------

/// The tags a generated discriminator table draws from.
const TABLE_TAGS: [&str; 4] = ["integer", "float", "text", "number"];

/// The kinds a generated discriminator table maps its tags to.
const fn table_kinds() -> [ScalarKind; 4] {
    [
        ScalarKind::Int {
            bits: 32,
            signed: true,
        },
        ScalarKind::Float { bits: 64 },
        ScalarKind::Str,
        ScalarKind::Decimal,
    ]
}

/// The lexeme classes of the generated cases.
fn generated_classes() -> Vec<LexClass> {
    let mut classes = vec![
        LexClass::Number,
        LexClass::String,
        LexClass::Bool,
        LexClass::Null,
        LexClass::DateTime,
        LexClass::Bytes,
    ];
    classes.extend(["int", "custom"].map(|t| LexClass::Tagged(t.into())));
    classes
}

/// The numeric kinds, `Any` and `Str` of the generated dialect cases.
const fn dialect_kinds() -> [ScalarKind; 7] {
    [
        ScalarKind::Int {
            bits: 64,
            signed: true,
        },
        ScalarKind::BigInt,
        ScalarKind::Float { bits: 64 },
        ScalarKind::Float { bits: 32 },
        ScalarKind::Decimal,
        ScalarKind::Any,
        ScalarKind::Str,
    ]
}

/// A dialect probe, or a generated numeral: signed, prefixed, separated,
/// fractional and exponent forms, special spellings and sexagesimal forms.
fn dialect_lexeme() -> impl proptest::strategy::Strategy<Value = String> {
    use proptest::strategy::Strategy as _;
    proptest::prop_oneof![
        (0..dialect_probes().len()).prop_map(|i| dialect_probes()[i].to_owned()),
        "[-+]?(0[xXoObB])?[0-9a-fA-F_]{0,5}(\\.[0-9_]{0,3})?([eE][-+]?[0-9_]{0,3})?",
        "[-+]?\\.?(inf|Inf|INF|nan|NaN|NAN|Infinity|infinity)",
        "[-+]?[0-9]{1,3}(:[0-9]{1,2}){1,2}(\\.[0-9]{0,2})?",
    ]
}

proptest::proptest! {
    // The default configuration persists every shrunk counterexample under
    // `proptest-regressions/` and replays it first on the next run.
    #![proptest_config(proptest::test_runner::Config::default())]

    /// A generated discriminator table (each of four tags absent or mapped
    /// to one of four kinds), a tag of the table, outside it or none, an own
    /// kind, a class and a lexeme over a small alphabet: production's
    /// discriminated coercion agrees with the reference's.
    #[test]
    fn randomized_discriminated_differential(
        entries in proptest::collection::vec(proptest::option::of(0_usize..4), 4),
        tag_index in 0_usize..6,
        own_index in 0_usize..3,
        class_index in 0_usize..8,
        text in "[-+0-9.eEaAtrufls:T=]{0,8}",
    ) {
        let kinds = table_kinds();
        let table: BTreeMap<String, ScalarKind> = TABLE_TAGS
            .iter()
            .zip(&entries)
            .filter_map(|(tag, kind)| kind.map(|k| ((*tag).to_owned(), kinds[k].clone())))
            .collect();
        let reference_table: BTreeMap<String, r::ScalarKind> = table
            .iter()
            .map(|(tag, kind)| (tag.clone(), kind_to_ref(kind)))
            .collect();
        let tag = match tag_index {
            0..4 => Some(TABLE_TAGS[tag_index]),
            4 => Some("x-unknown"),
            _ => None,
        };
        let own = [ScalarKind::Any, ScalarKind::Str, ScalarKind::Decimal][own_index].clone();
        let spec = ValueSpec {
            discriminators: Some(table),
            ..ValueSpec::new(own.clone())
        };
        let plain = ValueSpec::new(own);
        let class = &generated_classes()[class_index];
        let reference = r::coerce_tagged(
            &text,
            &class_to_ref(class),
            tag,
            &reference_table,
            &spec_to_ref(&plain),
        );
        if let Some(reference) = reference {
            let production = coerce_discriminated(&Lexeme::new(&text, class.clone()), tag, &spec)
                .map(|v| to_ref(&v))
                .map_err(|e| reason_to_ref(&e.reason));
            proptest::prop_assert_eq!(
                production,
                reference,
                "{:?} {:?} tagged {:?} under {:?}",
                text,
                class,
                tag,
                spec
            );
        }
    }

    /// A generated dialect (each of the eight, or none), kind, class and
    /// lexeme (a dialect probe or a generated numeral): production's
    /// coercion agrees with the reference's dialect reading.
    #[test]
    fn randomized_dialect_differential(
        dialect_index in 0_usize..9,
        kind_index in 0_usize..7,
        class_index in 0_usize..3,
        text in dialect_lexeme(),
    ) {
        let dialect = DIALECTS.get(dialect_index).copied();
        let kind = dialect_kinds()[kind_index].clone();
        let spec = ValueSpec {
            numeral: dialect,
            ..ValueSpec::new(kind)
        };
        let class = [
            LexClass::Number,
            LexClass::String,
            LexClass::Tagged("x".into()),
        ][class_index]
            .clone();
        if let Some(reference) = r::coerce_dialect(&text, &class_to_ref(&class), &spec_to_ref(&spec)) {
            proptest::prop_assert_eq!(
                coerce_to_ref(&text, &class, &spec),
                reference,
                "{:?} {:?} under {:?}",
                text,
                class,
                spec
            );
        }
    }
}
