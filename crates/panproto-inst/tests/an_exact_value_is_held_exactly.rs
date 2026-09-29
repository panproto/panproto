//! Exact values in instances: the three exact variants are appended to
//! `Value` without moving any existing variant, take the canonical tags
//! after every existing tag, survive every serde format, order totally by
//! their canonical bytes, and are what a lexeme becomes when its declared
//! kind (or, for an open schema, its own form) calls for exactness.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;

use panproto_inst::identity::{CanonicalSink, Digest, GRAPH_INSTANCE_CONTEXT, write_value};
use panproto_inst::value::{
    BigIntValue, DecimalValue, ExactInteger, ExactKind, ExactParseError, Temporal,
};
use panproto_inst::{FidelityReason, Value, ValueFidelityError, ValueSite};
use panproto_schema::{CanonicalWriter, ScalarKind};
use serde::Serialize;
use serde::ser::{self, Impossible};

// ---------------------------------------------------------------------------
// A serializer that reports the serde variant index of an enum value.
// ---------------------------------------------------------------------------

/// Stops serialization once the variant index has been recorded.
#[derive(Debug)]
struct Stop;

impl fmt::Display for Stop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("stop")
    }
}

impl std::error::Error for Stop {}

impl ser::Error for Stop {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self
    }
}

/// Records `(variant index, variant name)` of the outermost enum value.
struct VariantIndex<'a>(&'a Cell<Option<(u32, &'static str)>>);

macro_rules! refuse {
    ($($name:ident($($ty:ty),*)),* $(,)?) => {
        $(fn $name(self, $(_: $ty),*) -> Result<(), Stop> { Err(Stop) })*
    };
}

impl ser::Serializer for VariantIndex<'_> {
    type Ok = ();
    type Error = Stop;
    type SerializeSeq = Impossible<(), Stop>;
    type SerializeTuple = Impossible<(), Stop>;
    type SerializeTupleStruct = Impossible<(), Stop>;
    type SerializeTupleVariant = Impossible<(), Stop>;
    type SerializeMap = Impossible<(), Stop>;
    type SerializeStruct = Impossible<(), Stop>;
    type SerializeStructVariant = Impossible<(), Stop>;

    refuse!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_char(char),
        serialize_str(&str),
        serialize_bytes(&[u8]),
        serialize_none(),
        serialize_unit(),
        serialize_unit_struct(&'static str),
    );

    fn serialize_some<T: ?Sized + Serialize>(self, _: &T) -> Result<(), Stop> {
        Err(Stop)
    }

    fn serialize_unit_variant(self, _: &'static str, i: u32, v: &'static str) -> Result<(), Stop> {
        self.0.set(Some((i, v)));
        Err(Stop)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: &T,
    ) -> Result<(), Stop> {
        Err(Stop)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        i: u32,
        v: &'static str,
        _: &T,
    ) -> Result<(), Stop> {
        self.0.set(Some((i, v)));
        Err(Stop)
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Stop> {
        Err(Stop)
    }

    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Stop> {
        Err(Stop)
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Stop> {
        Err(Stop)
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        i: u32,
        v: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Stop> {
        self.0.set(Some((i, v)));
        Err(Stop)
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Stop> {
        Err(Stop)
    }

    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, Stop> {
        Err(Stop)
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        i: u32,
        v: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Stop> {
        self.0.set(Some((i, v)));
        Err(Stop)
    }
}

/// The serde variant index and name of an enum value.
fn variant_of<T: Serialize>(value: &T) -> (u32, &'static str) {
    let cell = Cell::new(None);
    let _stopped = value.serialize(VariantIndex(&cell));
    cell.get().expect("an enum value")
}

// ---------------------------------------------------------------------------
// One value of every variant.
// ---------------------------------------------------------------------------

fn decimal(text: &str) -> DecimalValue {
    DecimalValue::parse(text).unwrap()
}

fn big(text: &str) -> BigIntValue {
    match BigIntValue::parse(text).unwrap() {
        ExactInteger::Big(b) => b,
        ExactInteger::Int(n) => panic!("{n} fits i64"),
    }
}

fn every_variant() -> Vec<Value> {
    vec![
        Value::Bool(true),
        Value::Int(-7),
        Value::Float(2.5),
        Value::Str("s".into()),
        Value::Bytes(vec![0, 1, 2, 0xff]),
        Value::CidLink("bafy".into()),
        Value::Blob {
            ref_: "r".into(),
            mime: "text/plain".into(),
            size: 3,
        },
        Value::Token("t".into()),
        Value::Null,
        Value::Opaque {
            type_: "ty".into(),
            fields: HashMap::from([("a".to_owned(), Value::Int(1))]),
        },
        Value::Unknown(HashMap::from([("k".to_owned(), Value::Null)])),
        Value::List(vec![Value::Int(1), Value::Str("x".into())]),
        Value::LabeledNull(9),
        Value::Decimal(decimal("12345678901234567890.123456789")),
        Value::BigInt(big("18446744073709551615")),
        Value::Temporal(Temporal::parse("2026-09-28T12:34:56.789+05:30").unwrap()),
    ]
}

#[test]
fn existing_value_variants_keep_their_serde_index() {
    let names = [
        "Bool",
        "Int",
        "Float",
        "Str",
        "Bytes",
        "CidLink",
        "Blob",
        "Token",
        "Null",
        "Opaque",
        "Unknown",
        "List",
        "LabeledNull",
        "Decimal",
        "BigInt",
        "Temporal",
    ];
    for (value, (expected_index, expected_name)) in
        every_variant().iter().zip(names.iter().enumerate())
    {
        let (index, name) = variant_of(value);
        assert_eq!(
            (index, name),
            (u32::try_from(expected_index).unwrap(), *expected_name)
        );
    }
}

#[test]
fn new_value_variants_report_their_type_names() {
    let values = every_variant();
    let names: Vec<&str> = values[13..].iter().map(Value::type_name).collect();
    assert_eq!(names, ["decimal", "bigint", "temporal"]);
}

// ---------------------------------------------------------------------------
// Canonical bytes.
// ---------------------------------------------------------------------------

fn canonical(value: &Value) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    write_value(&mut w, value);
    w.into_bytes()
}

const fn len(n: u64) -> [u8; 8] {
    n.to_le_bytes()
}

#[test]
fn new_value_variants_take_tags_thirteen_to_fifteen() {
    // Decimal 1.50: tag 13, sign, digits behind their length, exponent.
    let mut expected = vec![13, 0];
    expected.extend_from_slice(&len(3));
    expected.extend_from_slice(b"150");
    expected.extend_from_slice(&(-2_i64).to_le_bytes());
    assert_eq!(canonical(&Value::Decimal(decimal("1.50"))), expected);

    // BigInt -10^29: tag 14, sign 1, thirty digits.
    let mut expected = vec![14, 1];
    expected.extend_from_slice(&len(30));
    expected.extend_from_slice(format!("1{}", "0".repeat(29)).as_bytes());
    assert_eq!(
        canonical(&Value::BigInt(big(&format!("-1{}", "0".repeat(29))))),
        expected
    );

    // A local date-time: tag 15, form 2, date, time, offset 0 (local).
    let mut expected = vec![15, 2];
    expected.extend_from_slice(&2026_i64.to_le_bytes());
    expected.extend_from_slice(&[1, 2, 3, 4, 5]);
    expected.extend_from_slice(&0_u32.to_le_bytes());
    expected.extend_from_slice(&[0, 0]);
    assert_eq!(
        canonical(&Value::Temporal(
            Temporal::parse("2026-01-02T03:04:05").unwrap()
        )),
        expected
    );

    // The sink streams the same bytes it writes, for every variant.
    for value in every_variant() {
        let bytes = canonical(&value);
        let mut hasher = Digest::hasher(GRAPH_INSTANCE_CONTEXT);
        hasher.value(&value);
        assert_eq!(
            hasher.finalize(),
            Digest::derive(GRAPH_INSTANCE_CONTEXT, &bytes),
            "{value:?}"
        );
        assert_eq!(value.canonical_bytes(), bytes);
    }
    // Tags of the existing variants are unchanged: the first byte of each
    // is its position.
    let tags: Vec<u8> = every_variant().iter().map(|v| canonical(v)[0]).collect();
    assert_eq!(tags, (0..16).collect::<Vec<u8>>());
}

mod ordering {
    use std::cmp::Ordering;

    use proptest::prelude::*;

    use super::{DecimalValue, Temporal, Value, big, every_variant};

    fn scalar() -> impl Strategy<Value = Value> {
        prop_oneof![
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(Value::Int),
            any::<f64>().prop_map(Value::Float),
            "[a-z]{0,6}".prop_map(Value::Str),
            proptest::collection::vec(any::<u8>(), 0..6).prop_map(Value::Bytes),
            Just(Value::Null),
            any::<u64>().prop_map(Value::LabeledNull),
            ("-?[0-9]{1,30}", "[0-9]{0,8}").prop_map(|(i, f)| {
                let text = if f.is_empty() { i } else { format!("{i}.{f}") };
                Value::Decimal(DecimalValue::parse(&text).unwrap())
            }),
            "[1-9][0-9]{19,40}".prop_map(|d| Value::BigInt(big(&d))),
            (0_u8..24, 0_u8..60, 0_u8..61, 1_u8..29).prop_map(|(h, m, s, d)| {
                Value::Temporal(
                    Temporal::parse(&format!("2026-02-{d:02}T{h:02}:{m:02}:{s:02}Z")).unwrap(),
                )
            }),
        ]
    }

    fn value() -> impl Strategy<Value = Value> {
        scalar().prop_recursive(2, 8, 3, |inner| {
            proptest::collection::vec(inner, 0..3).prop_map(Value::List)
        })
    }

    proptest! {
        #[test]
        fn canonical_cmp_agrees_with_canonical_bytes(a in value(), b in value(), c in value()) {
            let bytes = |v: &Value| v.canonical_bytes();
            prop_assert_eq!(a.canonical_cmp(&b), bytes(&a).cmp(&bytes(&b)));
            prop_assert_eq!(a.canonical_cmp(&b), b.canonical_cmp(&a).reverse());
            prop_assert_eq!(a.canonical_cmp(&a), Ordering::Equal);
            // Equal bytes only for equal values (the encoding is injective;
            // floats compare by bits there, so NaNs are skipped here).
            let has_nan = |v: &Value| format!("{v:?}").contains("NaN");
            if !has_nan(&a) && !has_nan(&b) {
                prop_assert_eq!(a.canonical_cmp(&b).is_eq(), a == b);
            }
            // Transitivity, so a sort by it is well defined.
            if a.canonical_cmp(&b).is_le() && b.canonical_cmp(&c).is_le() {
                prop_assert!(a.canonical_cmp(&c).is_le());
            }
        }
    }

    #[test]
    fn a_sort_by_canonical_cmp_is_the_same_from_any_starting_order() {
        let mut forward = every_variant();
        let mut backward = every_variant();
        backward.reverse();
        forward.sort_by(Value::canonical_cmp);
        backward.sort_by(Value::canonical_cmp);
        assert_eq!(forward, backward);
    }
}

// ---------------------------------------------------------------------------
// Serde.
// ---------------------------------------------------------------------------

#[test]
fn every_value_variant_round_trips_in_every_serde_format() {
    for value in every_variant() {
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&json).unwrap(),
            value,
            "{json}"
        );
        let packed = rmp_serde::to_vec(&value).unwrap();
        assert_eq!(rmp_serde::from_slice::<Value>(&packed).unwrap(), value);
        let named = rmp_serde::to_vec_named(&value).unwrap();
        assert_eq!(rmp_serde::from_slice::<Value>(&named).unwrap(), value);
        let mut cbor = Vec::new();
        ciborium::into_writer(&value, &mut cbor).unwrap();
        assert_eq!(
            ciborium::from_reader::<Value, _>(cbor.as_slice()).unwrap(),
            value
        );
    }
    // The exact variants' JSON form holds their fields.
    assert_eq!(
        serde_json::to_string(&Value::Decimal(decimal("1.50"))).unwrap(),
        r#"{"Decimal":{"negative":false,"digits":"150","exponent":-2}}"#
    );
    assert_eq!(
        serde_json::to_string(&Value::Temporal(Temporal::parse("2026-09-28").unwrap())).unwrap(),
        r#"{"Temporal":{"date":{"year":2026,"month":9,"day":28}}}"#
    );
    // A BigInt that fits i64 is not a representation of anything.
    assert!(
        serde_json::from_str::<Value>(r#"{"BigInt":{"negative":false,"digits":"5"}}"#).is_err()
    );
}

// ---------------------------------------------------------------------------
// Fidelity errors.
// ---------------------------------------------------------------------------

#[test]
fn fidelity_errors_serialize_in_contract_form() {
    let error = ValueFidelityError {
        site: ValueSite::new("price", Some(12)),
        lexeme: "1.005".into(),
        target: ScalarKind::Decimal,
        reason: FidelityReason::ScaleExceeded { max: 2 },
    };
    let json = serde_json::to_string(&error).unwrap();
    assert_eq!(
        json,
        r#"{"site":{"vertex":"price","byte_offset":12},"lexeme":"1.005","target":"decimal","reason":{"scale_exceeded":{"max":2}}}"#
    );
    assert_eq!(
        serde_json::from_str::<ValueFidelityError>(&json).unwrap(),
        error
    );

    let reasons = [
        (FidelityReason::Overflow, r#""overflow""#),
        (
            FidelityReason::PrecisionExceeded { max: 38 },
            r#"{"precision_exceeded":{"max":38}}"#,
        ),
        (
            FidelityReason::ScaleExceeded { max: 10 },
            r#"{"scale_exceeded":{"max":10}}"#,
        ),
        (FidelityReason::NotANumber, r#""not_a_number""#),
        (FidelityReason::NonFinite, r#""non_finite""#),
        (FidelityReason::InvalidTemporal, r#""invalid_temporal""#),
        (FidelityReason::OffsetRequired, r#""offset_required""#),
        (FidelityReason::OffsetForbidden, r#""offset_forbidden""#),
        (FidelityReason::InvalidEncoding, r#""invalid_encoding""#),
        (
            FidelityReason::NotANumberForKind,
            r#""not_a_number_for_kind""#,
        ),
    ];
    for (index, (reason, form)) in reasons.iter().enumerate() {
        assert_eq!(serde_json::to_string(reason).unwrap(), *form);
        assert_eq!(
            serde_json::from_str::<FidelityReason>(form).unwrap(),
            *reason
        );
        // The variants are in contract order.
        assert_eq!(reason_index(reason), u32::try_from(index).unwrap());
    }
    // Unknown sites and the Display form.
    assert_eq!(
        error.to_string(),
        "\"1.005\" at vertex \"price\" (byte 12) cannot be held exactly as Decimal: more than 2 digits after the point"
    );
}

fn reason_index(reason: &FidelityReason) -> u32 {
    variant_of(reason).0
}

#[test]
fn exact_parse_errors_convert_into_value_fidelity_errors() {
    let parse_error: ExactParseError = DecimalValue::parse("1.2.3").unwrap_err();
    assert_eq!(
        parse_error,
        ExactParseError {
            kind: ExactKind::Decimal,
            reason: FidelityReason::NotANumber
        }
    );
    let error = ValueFidelityError::from(parse_error)
        .at(ValueSite::new("amount", Some(40)))
        .with_lexeme("1.2.3");
    assert_eq!(
        error,
        ValueFidelityError {
            site: ValueSite::new("amount", Some(40)),
            lexeme: "1.2.3".into(),
            target: ScalarKind::Decimal,
            reason: FidelityReason::NotANumber,
        }
    );
    let temporal: ValueFidelityError = Temporal::parse("2023-02-29").unwrap_err().into();
    assert_eq!(temporal.target, ScalarKind::DateTime);
    assert_eq!(temporal.reason, FidelityReason::InvalidTemporal);
    assert_eq!(temporal.site, ValueSite::unknown());
    let integer: ValueFidelityError = BigIntValue::parse("x").unwrap_err().into();
    assert_eq!(
        integer
            .with_target(ScalarKind::Int {
                bits: 32,
                signed: true
            })
            .target,
        ScalarKind::Int {
            bits: 32,
            signed: true
        }
    );
}

#[test]
fn integers_that_fit_i64_are_always_int() {
    assert_eq!(Value::parse_integer("42").unwrap(), Value::Int(42));
    assert_eq!(
        Value::parse_integer("-9223372036854775808").unwrap(),
        Value::Int(i64::MIN)
    );
    assert_eq!(
        Value::parse_integer("18446744073709551615").unwrap(),
        Value::BigInt(big("18446744073709551615"))
    );
    assert_eq!(
        Value::from(BigIntValue::from_i128(i128::from(i64::MAX) + 1)),
        Value::BigInt(big("9223372036854775808"))
    );
    assert_eq!(Value::from(BigIntValue::from_i128(-3)), Value::Int(-3));
}
