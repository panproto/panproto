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
//! The reference holds a richer model than production: decimal NaNs,
//! infinities and negative zeros, reduced-precision and unwritten-offset
//! temporals, the padded and Latin-1 byte texts, numeral dialects and a
//! second float reading. Production's model embeds in it by [`to_ref`]
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
//!   `NegZero` to `NegZero(Extended)`.
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
//! the temporal clauses cover the three temporal variants; the spec clause
//! covers every production kind. Injectivity, by induction on the value:
//! (1) distinct production variants go to distinct reference variants, the
//! three temporal variants to temporals told apart by which of `date` and
//! `time` are present; (2) within a variant every component map is
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
use std::fmt::Write as _;
use std::time::Instant;

use panproto_inst::Value;
use panproto_inst::value::coerce::{LexClass, Lexeme, coerce, coerce_value};
use panproto_inst::value::{
    BigIntValue, CivilDate, CivilTime, DateTimeSeparator, DecimalValue, FidelityReason, Fraction,
    Offset, Temporal, ValueSite, WideInt, ZuluCase,
};
use panproto_reference::bytes::{encode_base64, latin1_decode};
use panproto_reference::coerce as r;
use panproto_reference::enumerate::{self as en, values as scope};
use panproto_reference::exact as rx;
use panproto_reference::float::{Format, widen_bits};
use panproto_reference::num::{Int, Nat};
use panproto_reference::temporal as rt;
use panproto_reference::value::Value as RValue;
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
fn year_width(year: i32) -> usize {
    year.unsigned_abs().to_string().len().max(4)
}

const fn offset_to_ref(o: Offset) -> rt::Offset {
    match o {
        Offset::Local => rt::Offset::Local,
        Offset::Zulu(ZuluCase::Upper) => rt::Offset::Zulu(rt::ZuluCase::Upper),
        Offset::Zulu(ZuluCase::Lower) => rt::Offset::Zulu(rt::ZuluCase::Lower),
        Offset::Fixed(m) => rt::Offset::Fixed(m, rt::OffsetForm::Extended),
        Offset::NegZero => rt::Offset::NegZero(rt::OffsetForm::Extended),
    }
}

fn offset_from_ref(o: rt::Offset) -> Option<Offset> {
    Some(match o {
        rt::Offset::Local => Offset::Local,
        rt::Offset::Zulu(rt::ZuluCase::Upper) => Offset::Zulu(ZuluCase::Upper),
        rt::Offset::Zulu(rt::ZuluCase::Lower) => Offset::Zulu(ZuluCase::Lower),
        rt::Offset::Fixed(m, rt::OffsetForm::Extended) => Offset::fixed(m).ok()?,
        rt::Offset::NegZero(rt::OffsetForm::Extended) => Offset::NegZero,
        _ => return None,
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
            value: i64::from(d.year),
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
    CivilDate::new(i32::try_from(year.value).ok()?, d.month?, d.day?).ok()
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
    if t.infinity.is_some() {
        return None;
    }
    let offset = offset_from_ref(t.offset)?;
    Some(match (&t.date, &t.time, t.separator) {
        (Some(d), None, None) if offset == Offset::Local => Temporal::Date(date_from_ref(d)?),
        (None, Some(time), None) => Temporal::Time(time_from_ref(time)?, offset),
        (Some(d), Some(time), Some(s)) => Temporal::DateTime(
            date_from_ref(d)?,
            time_from_ref(time)?,
            offset,
            separator_from_ref(s),
        ),
        _ => return None,
    })
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
        Value::Decimal(d) => RValue::Decimal(rx::Decimal::Finite(rx::DecimalValue {
            negative: d.negative,
            digits: nat(&d.digits),
            exponent: int_of(&d.exponent),
        })),
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
        RValue::Decimal(_) => return None,
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
        BytesText::Hex => r::BytesText::Hex,
        BytesText::Raw => r::BytesText::Raw,
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

const fn kind_from_ref(k: r::ScalarKind) -> Option<ScalarKind> {
    Some(match k {
        r::ScalarKind::Bool => ScalarKind::Bool,
        r::ScalarKind::Int { bits, signed } => ScalarKind::Int { bits, signed },
        r::ScalarKind::BigInt => ScalarKind::BigInt,
        r::ScalarKind::Float { bits } => ScalarKind::Float { bits },
        r::ScalarKind::Decimal => ScalarKind::Decimal,
        r::ScalarKind::Str => ScalarKind::Str,
        r::ScalarKind::Bytes(r::BytesText::Base64) => ScalarKind::Bytes(BytesText::Base64),
        r::ScalarKind::Bytes(r::BytesText::Base64Url) => ScalarKind::Bytes(BytesText::Base64Url),
        r::ScalarKind::Bytes(r::BytesText::Hex) => ScalarKind::Bytes(BytesText::Hex),
        r::ScalarKind::Bytes(r::BytesText::Raw) => ScalarKind::Bytes(BytesText::Raw),
        r::ScalarKind::Bytes(_) => return None,
        r::ScalarKind::Date => ScalarKind::Date,
        r::ScalarKind::Time => ScalarKind::Time,
        r::ScalarKind::DateTime => ScalarKind::DateTime,
        r::ScalarKind::Token => ScalarKind::Token,
        r::ScalarKind::Any => ScalarKind::Any,
    })
}

/// The conversion of a production spec (with no `max_length`).
fn spec_to_ref(s: &ValueSpec) -> r::ValueSpec {
    assert_eq!(s.max_length, None, "the conversion's domain");
    r::ValueSpec {
        kind: kind_to_ref(&s.kind),
        precision: s.precision,
        scale: s.scale,
        with_time_zone: s.with_time_zone,
        int_bits: s.int_bits,
        unsigned: s.unsigned,
        float_reading: r::FloatReading::Exact,
        numeral: None,
    }
}

/// The production spec whose image is exactly `s`, if there is one.
fn spec_into_production(s: &r::ValueSpec) -> Option<ValueSpec> {
    let p = ValueSpec {
        kind: kind_from_ref(s.kind)?,
        precision: s.precision,
        scale: s.scale,
        max_length: None,
        with_time_zone: s.with_time_zone,
        int_bits: s.int_bits,
        unsigned: s.unsigned,
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

/// Every reference spec of the scope: each scalar kind, with the integer
/// widths 8 to 64 either signed or not, the float widths 16, 32 and 64
/// under both readings, the decimal facets, the offset requirements, the
/// integer-width overrides, and each numeral dialect over the numeric
/// kinds. Production holds only some of them.
fn specs() -> Vec<r::ValueSpec> {
    use r::ScalarKind as K;
    let of = r::ValueSpec::of;
    let mut out = vec![
        of(K::Any),
        of(K::Bool),
        of(K::BigInt),
        of(K::Str),
        of(K::Token),
    ];
    for bits in [8, 16, 32, 64] {
        for signed in [true, false] {
            out.push(of(K::Int { bits, signed }));
        }
    }
    for int_bits in [Some(8), Some(16)] {
        for unsigned in [false, true] {
            out.push(r::ValueSpec {
                int_bits,
                unsigned,
                ..of(K::Int {
                    bits: 64,
                    signed: true,
                })
            });
        }
    }
    out.push(r::ValueSpec {
        unsigned: true,
        ..of(K::Int {
            bits: 64,
            signed: true,
        })
    });
    for bits in [16, 32, 64] {
        out.push(of(K::Float { bits }));
        out.push(r::ValueSpec {
            float_reading: r::FloatReading::NearestKeepingLexeme,
            ..of(K::Float { bits })
        });
    }
    for precision in [None, Some(1), Some(2)] {
        for scale in [None, Some(0), Some(1)] {
            out.push(r::ValueSpec {
                precision,
                scale,
                ..of(K::Decimal)
            });
        }
    }
    for text in [
        r::BytesText::Base64,
        r::BytesText::Base64Url,
        r::BytesText::Hex,
        r::BytesText::Raw,
        r::BytesText::Base64Padded,
        r::BytesText::Base64UrlPadded,
        r::BytesText::Latin1,
    ] {
        out.push(of(K::Bytes(text)));
    }
    for kind in [K::Date, K::Time, K::DateTime] {
        for with_time_zone in [None, Some(true), Some(false)] {
            out.push(r::ValueSpec {
                with_time_zone,
                ..of(kind)
            });
        }
    }
    for numeral in [
        r::NumeralDialect::Json,
        r::NumeralDialect::Toml,
        r::NumeralDialect::Yaml12,
        r::NumeralDialect::Yaml11,
        r::NumeralDialect::Xsd,
        r::NumeralDialect::Java,
        r::NumeralDialect::ProtoJson,
        r::NumeralDialect::Sql,
    ] {
        for kind in [
            K::Int {
                bits: 64,
                signed: true,
            },
            K::BigInt,
            K::Float { bits: 64 },
            K::Decimal,
        ] {
            out.push(r::ValueSpec {
                numeral: Some(numeral),
                ..of(kind)
            });
        }
    }
    out
}

/// The NaN, infinity and signalling spellings, and decimal edge numerals
/// at the float boundaries.
const SPELLINGS: &[&str] = &[
    "NaN",
    "-NaN",
    "nan",
    "+nan",
    ".nan",
    "-.nan",
    "NAN",
    "sNaN",
    "-sNaN",
    "NaN5",
    "sNaN7",
    "inf",
    "-inf",
    "+inf",
    "Infinity",
    "-Infinity",
    "INF",
    ".inf",
    "-.inf",
    "+.inf",
    "infinity",
];

const EDGE_NUMERALS: &[&str] = &[
    "0",
    "-0",
    "0.0",
    "-0.0",
    "0e5",
    "-0e-5",
    "0.00",
    "0.10",
    "0.1",
    "1.50",
    "-1.5",
    "65504",
    "65505",
    "65519",
    "65519.99",
    "65520",
    "-65520",
    "6.103515625e-5",
    "5.960464477539063e-8",
    "2.98023223876953125e-8",
    "2.9802322387695312e-8",
    "16777216",
    "16777217",
    "16777217.0",
    "3.4028235e38",
    "3.4028235677973366e38",
    "3.4028236e38",
    "1.401298464324817e-45",
    "7.006492321624085e-46",
    "1e308",
    "1.7976931348623157e308",
    "1.7976931348623158e308",
    "1.7976931348623159e308",
    "1e309",
    "4.9e-324",
    "5e-324",
    "2.4703282292062328e-324",
    "2.4703282292062327e-324",
    "1e-400",
    "1e400",
    "1e401",
    "-1e401",
    "1e-401",
    "9007199254740992",
    "9007199254740993",
    "1125899906842624.2",
    "1125899906842624.3",
    "52401.312",
    "52401.313",
    "127",
    "128",
    "-128",
    "-129",
    "255",
    "256",
    "32767",
    "32768",
    "-32768",
    "-32769",
    "65535",
    "65536",
    "2147483647",
    "2147483648",
    "-2147483648",
    "-2147483649",
    "4294967295",
    "4294967296",
    "9223372036854775807",
    "9223372036854775808",
    "-9223372036854775808",
    "-9223372036854775809",
    "18446744073709551615",
    "18446744073709551616",
    "1e20",
    "1e4097",
    "-1e20",
    "1.5e1",
    "1.25e1",
    "12.5",
    "0.5e1",
];

const WORDS: &[&str] = &[
    "true", "false", "TRUE", "False", "yes", "null", "", " ", "x", "abc", "\u{e9}", "\u{100}", "T",
    "Z", "00:00", "+04:00",
];

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

/// The byte texts of `b`: padded and unpadded base64, URL-safe base64, both
/// hex cases and the Latin-1 text.
fn byte_texts(b: &[u8]) -> Vec<String> {
    let padded = encode_base64(b);
    let unpadded = padded.trim_end_matches('=').to_owned();
    let url = padded.replace('+', "-").replace('/', "_");
    let lower: String = b.iter().fold(String::new(), |mut out, x| {
        let _ = write!(out, "{x:02x}");
        out
    });
    vec![
        unpadded,
        url.trim_end_matches('=').to_owned(),
        url,
        lower.to_uppercase(),
        lower,
        padded,
        latin1_decode(b),
    ]
}

/// Every lexeme of the scope: the values-page numerals, the spellings, the
/// edge numerals, the words, every temporal lexeme, and every byte text of
/// the byte strings up to the given length.
fn lexemes(byte_text_len: usize) -> Vec<String> {
    let mut out: Vec<String> = scope::numerals();
    out.extend(SPELLINGS.iter().map(|s| (*s).to_owned()));
    out.extend(EDGE_NUMERALS.iter().map(|s| (*s).to_owned()));
    out.extend(WORDS.iter().map(|s| (*s).to_owned()));
    out.extend(scope::temporal_lexemes());
    for b in scope::byte_strings().expect("bounded") {
        if b.len() <= byte_text_len {
            out.extend(byte_texts(&b));
        }
    }
    dedupe(out)
}

fn dedupe<T: Clone + Eq + std::hash::Hash>(items: Vec<T>) -> Vec<T> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|x| seen.insert(x.clone()))
        .collect()
}

/// Every reference value of the scope.
///
/// Floats: every binary16 pattern widened and its two binary64
/// neighbours; the binary64 boundary patterns of the binary32 narrowing
/// (every exponent field from 871 to 1153 and 0, 1, 2046, 2047, by the
/// boundary fractions); every binary32 boundary pattern widened and its
/// neighbours. Numbers: each lexeme read under `Any`, `BigInt`, `Decimal`,
/// the three float widths and `Int`, wherever the reference accepts it.
/// Temporals: each temporal lexeme read under the three temporal kinds.
/// Strings and tokens of every lexeme; bytes of every byte string up to
/// length two; booleans, null and one value of each structured variant.
fn values() -> Vec<RValue> {
    use r::ScalarKind as K;
    let mut out = Vec::new();
    let around = |w: u64| [w, w.wrapping_sub(1), w.wrapping_add(1)];
    for b in scope::binary16_patterns() {
        out.extend(around(widen_bits(Format::B16, b)).map(RValue::Float));
    }
    let exps: Vec<u64> = (871..=1153).chain([0, 1, 2046, 2047]).collect();
    out.extend(
        en::boundary_bits(Format::B64, Some(&exps), &en::binary64_fractions())
            .into_iter()
            .map(RValue::Float),
    );
    for b in en::boundary_bits(Format::B32, None, &en::binary32_fractions()) {
        out.extend(around(widen_bits(Format::B32, b)).map(RValue::Float));
    }
    let numeric = [
        K::Any,
        K::BigInt,
        K::Decimal,
        K::Float { bits: 16 },
        K::Float { bits: 32 },
        K::Float { bits: 64 },
        K::Int {
            bits: 64,
            signed: true,
        },
    ];
    let lexemes = lexemes(1);
    for t in &lexemes {
        for kind in numeric {
            if let Some(Ok(v)) = r::coerce(t, &r::LexClass::Number, &r::ValueSpec::of(kind)) {
                out.push(v);
            }
        }
        for kind in [K::Date, K::Time, K::DateTime] {
            if let Some(Ok(v)) = r::coerce(t, &r::LexClass::DateTime, &r::ValueSpec::of(kind)) {
                out.push(v);
            }
        }
        out.push(RValue::Str(t.clone()));
        out.push(RValue::Token(t.clone()));
    }
    for i in [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX] {
        out.push(RValue::Int(i));
    }
    out.extend(scope::byte_strings().expect("bounded").map(RValue::Bytes));
    out.extend([
        RValue::Bool(true),
        RValue::Bool(false),
        RValue::Null,
        RValue::CidLink("bafy".to_owned()),
        RValue::Blob {
            ref_: "bafy".to_owned(),
            mime: "text/plain".to_owned(),
            size: 3,
        },
        RValue::Opaque {
            type_: "custom".to_owned(),
            fields: std::iter::once(("value".to_owned(), RValue::Str("1".to_owned()))).collect(),
        },
        RValue::Unknown(std::iter::once(("a".to_owned(), RValue::Int(1))).collect()),
        RValue::List(vec![]),
        RValue::List(vec![RValue::Int(1), RValue::Float(f64::NAN.to_bits())]),
        RValue::LabeledNull(0),
        RValue::LabeledNull(u64::MAX),
    ]);
    dedupe(out)
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
        input_class: &str,
        production: &Result<RValue, r::FidelityReason>,
        reference: &Result<RValue, r::FidelityReason>,
    ) {
        self.compared += 1;
        *self.by_outcome.entry(label(reference)).or_default() += 1;
        *self.by_kind.entry(kind_label(spec)).or_default() += 1;
        if production != reference {
            self.disagreement_count += 1;
            let class = format!(
                "{} / {input_class} / production {} / reference {}",
                kind_label(spec),
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
    let values = values();
    let mut images = HashSet::new();
    let mut held = 0u64;
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
    }
    let specs = specs();
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
        "conversion: {} reference values, {held} held by production, all round-tripped and pairwise distinct",
        values.len()
    );
    println!(
        "conversion: {} reference specs, {held_specs} held by production, all round-tripped",
        specs.len()
    );
    println!(
        "conversion: {} refusal reasons, pairwise distinct",
        reasons.len()
    );
}

/// The number of worker threads a differential run uses.
const WORKERS: usize = 4;

/// Run `case` for every spec on [`WORKERS`] threads, each spec on one
/// thread, and merge the tallies.
fn over_specs(specs: &[r::ValueSpec], case: impl Fn(&r::ValueSpec, &mut Tally) + Sync) -> Tally {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let tallies: Vec<Tally> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..WORKERS)
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

#[test]
fn coerce_value_agrees_with_the_reference_over_the_values_scope() {
    let started = Instant::now();
    let values = values();
    let specs = specs();
    let held: Vec<(&RValue, Option<Value>)> =
        values.iter().map(|x| (x, into_production(x))).collect();
    let total = (values.len() * specs.len()) as u64;
    println!(
        "coerce_value: bound {} reference values by {} reference specs",
        values.len(),
        specs.len()
    );
    let tally = over_specs(&specs, |spec, tally| {
        let Some(production_spec) = spec_into_production(spec) else {
            tally.outside_production += values.len() as u64;
            return;
        };
        for (x, p) in &held {
            let Some(p) = p else {
                tally.outside_production += 1;
                continue;
            };
            let Some(reference) = r::coerce_value(x, spec) else {
                tally.outside_reference += 1;
                continue;
            };
            let production = coerce_value(p, &production_spec, ValueSite::unknown())
                .map(|v| to_ref(&v))
                .map_err(|e| reason_to_ref(&e.reason));
            tally.record(spec, x, &value_class(x), &production, &reference);
        }
    });
    tally.print("coerce_value", total, started);
}

#[test]
fn coerce_agrees_with_the_reference_over_the_values_scope() {
    let started = Instant::now();
    let lexemes = lexemes(1);
    let specs = specs();
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
                    &format!("{class:?}"),
                    &production,
                    &reference,
                );
            }
        }
    });
    tally.print("coerce", total, started);
}
