#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Deterministic schema-v2 path and amendment-law audits.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::bytes;
use panproto_reference::canonical::{ComplementShape, IdentityEnvelope, decode_value, identity};
use panproto_reference::coerce::{
    FidelityReason, LexClass, NumeralDialect, ScalarKind, ValueSpec, coerce_dialect,
};
use panproto_reference::exact::{BigIntValue, Decimal, DecimalValue};
use panproto_reference::keys::{KeyEquality, NumericPrimitive, SqlCollation, SqlPad};
use panproto_reference::model::{GEdge, Graph, Node, NodeShape, SEdge};
use panproto_reference::num::Nat;
use panproto_reference::resolution::{rfc3986, standoff_form, standoff_inverse};
use panproto_reference::schema::{
    Change, Key, KeyPart, KeyRole, KeyScope, Registration, Schema, TargetSet, canonical_bytes,
    classify_change, register, validate_declarations,
};
use panproto_reference::temporal::{
    Offset, OffsetForm, Temporal, TemporalForm, TemporalInfinity, TemporalKind, TemporalParseError,
    TimeUnit, cmp_chronological, day_number, display_parse, from_epoch_count, parse, parse_display,
    parse_union, to_epoch_count,
};
use panproto_reference::theory::{CompositionSpec, CompositionStep, Sort, Theory};
use panproto_reference::value::Value;

fn read_dialect(
    text: &str,
    class: impl std::borrow::Borrow<LexClass>,
    kind: ScalarKind,
    numeral: NumeralDialect,
) -> Result<Value, FidelityReason> {
    coerce_dialect(
        text,
        class.borrow(),
        &ValueSpec {
            numeral: Some(numeral),
            ..ValueSpec::of(kind)
        },
    )
    .unwrap()
}

#[test]
fn manifest_reference_paths_resolve_at_compile_time() {
    assert_eq!(
        panproto_reference::theory::quotient(3, &[(0, 1), (1, 2)]),
        Some(vec![0, 0, 0])
    );
}

#[test]
fn identity_envelope_is_the_exact_version_two_layout() {
    let envelope = IdentityEnvelope {
        source_schema: [0x11; 32],
        target_schema: [0x22; 32],
        lens: [0x33; 32],
        shape: ComplementShape::Resolution,
        format_version: 0x5544,
    };
    let bytes = identity(&envelope);
    assert_eq!(bytes.len(), 101);
    assert_eq!(&bytes[..2], &[2, 0]);
    assert_eq!(&bytes[2..34], &[0x11; 32]);
    assert_eq!(&bytes[34..66], &[0x22; 32]);
    assert_eq!(&bytes[66..98], &[0x33; 32]);
    assert_eq!(bytes[98], 3);
    assert_eq!(&bytes[99..], &[0x44, 0x55]);

    for (shape, tag) in [
        (ComplementShape::WType, 0),
        (ComplementShape::Graph, 1),
        (ComplementShape::Functor, 2),
        (ComplementShape::Resolution, 3),
    ] {
        assert_eq!(identity(&IdentityEnvelope { shape, ..envelope })[98], tag);
    }
}

#[test]
fn numeral_dialect_probe_table_is_exact() {
    let int = ScalarKind::Int {
        bits: 64,
        signed: true,
    };
    let float = ScalarKind::Float { bits: 64 };

    assert_eq!(
        read_dialect("1_000", LexClass::Number, int, NumeralDialect::Toml),
        Ok(Value::Int(1_000))
    );
    assert_eq!(
        read_dialect("0x1F", LexClass::Number, int, NumeralDialect::Toml),
        Ok(Value::Int(31))
    );
    assert!(matches!(
        read_dialect("+inf", LexClass::Number, float, NumeralDialect::Toml),
        Ok(Value::Float(bits)) if bits == f64::INFINITY.to_bits()
    ));
    for invalid in ["1__0", "_1", "1_", ".5", "1."] {
        assert_eq!(
            read_dialect(invalid, LexClass::Number, int, NumeralDialect::Toml),
            Err(FidelityReason::NotANumber)
        );
    }
    assert_eq!(
        read_dialect("1.e2", LexClass::Number, float, NumeralDialect::Json),
        Err(FidelityReason::NotANumber)
    );

    assert_eq!(
        read_dialect("017", LexClass::Number, int, NumeralDialect::Yaml12),
        Ok(Value::Int(17))
    );
    assert_eq!(
        read_dialect("0o17", LexClass::Number, int, NumeralDialect::Yaml12),
        Ok(Value::Int(15))
    );
    assert_eq!(
        read_dialect("1_000", LexClass::Number, int, NumeralDialect::Yaml12),
        Err(FidelityReason::NotANumber)
    );
    assert_eq!(
        read_dialect("017", LexClass::Number, int, NumeralDialect::Yaml11),
        Ok(Value::Int(15))
    );
    assert_eq!(
        read_dialect("22:30", LexClass::Number, int, NumeralDialect::Yaml11),
        Ok(Value::Int(1_350))
    );
    assert_eq!(
        read_dialect("1234:30", LexClass::Number, int, NumeralDialect::Yaml11),
        Ok(Value::Int(74_070))
    );
    assert_eq!(
        read_dialect("1_000", LexClass::Number, int, NumeralDialect::Yaml11),
        Ok(Value::Int(1_000))
    );
    assert_eq!(
        read_dialect("1e3", LexClass::Number, float, NumeralDialect::Yaml11),
        Err(FidelityReason::NotANumber)
    );
}

#[test]
fn numeral_dialect_special_value_table_is_exact() {
    let int = ScalarKind::Int {
        bits: 64,
        signed: true,
    };
    let float = ScalarKind::Float { bits: 64 };

    assert!(matches!(
        read_dialect("INF", LexClass::String, float, NumeralDialect::Xsd),
        Ok(Value::Float(bits)) if bits == f64::INFINITY.to_bits()
    ));
    assert!(matches!(
        read_dialect("-NaN", LexClass::String, float, NumeralDialect::Xsd),
        Ok(Value::Float(bits)) if bits == 0xfff8_0000_0000_0000
    ));
    assert_eq!(
        read_dialect("inf", LexClass::String, float, NumeralDialect::Xsd),
        Err(FidelityReason::NotANumber)
    );
    assert_eq!(
        read_dialect("+1", LexClass::Number, int, NumeralDialect::Json),
        Err(FidelityReason::NotANumber)
    );
    assert_eq!(
        read_dialect("NaN", LexClass::String, float, NumeralDialect::Json),
        Err(FidelityReason::NotANumber)
    );
    assert!(matches!(
        read_dialect("Infinity", LexClass::String, float, NumeralDialect::Java),
        Ok(Value::Float(bits)) if bits == f64::INFINITY.to_bits()
    ));
    assert_eq!(
        read_dialect("inf", LexClass::String, float, NumeralDialect::Java),
        Err(FidelityReason::NotANumber)
    );
    assert!(matches!(
        read_dialect("NaN", LexClass::String, float, NumeralDialect::ProtoJson),
        Ok(Value::Float(bits)) if bits == f64::NAN.to_bits()
    ));
    assert_eq!(
        read_dialect("1e2", LexClass::String, int, NumeralDialect::ProtoJson),
        Ok(Value::Int(100))
    );
    assert!(matches!(
        read_dialect("Infinity", LexClass::String, float, NumeralDialect::Sql),
        Ok(Value::Float(bits)) if bits == f64::INFINITY.to_bits()
    ));
}

#[test]
fn schema_bytes_are_injective_on_the_labelled_scope() {
    let schemas = panproto_reference::enumerate::schemas(2, &["a", "b"], &["x"], 2);
    let encodings: BTreeSet<_> = schemas.iter().map(canonical_bytes).collect();
    assert_eq!(encodings.len(), schemas.len());
}

fn schema_with_key(role: KeyRole, scope: KeyScope) -> Schema {
    let mut schema = Schema::default()
        .vertex("record", "object")
        .vertex("a", "object")
        .vertex("b", "object")
        .vertex("scalar", "string")
        .edge("record", "scalar", "property", Some("id"));
    schema.keys.insert(
        "record".to_owned(),
        vec![Key {
            id: "key".to_owned(),
            parts: vec![KeyPart::Path(vec![panproto_reference::model::SEdge::new(
                "record",
                "scalar",
                "property",
                Some("id"),
            )])],
            role,
            scope,
            equality: Vec::new(),
        }],
    );
    schema
}

#[test]
fn schema_change_classification_has_the_key_boundaries() {
    let unique = schema_with_key(KeyRole::Unique, KeyScope::Document);
    let primary = schema_with_key(KeyRole::Primary, KeyScope::Document);
    assert_eq!(classify_change(&unique, &primary), Change::Breaking);
    assert_eq!(
        classify_change(&primary, &unique),
        Change::BackwardCompatible
    );

    let within_a = schema_with_key(KeyRole::Unique, KeyScope::Within("a".to_owned()));
    let within_b = schema_with_key(KeyRole::Unique, KeyScope::Within("b".to_owned()));
    assert_eq!(classify_change(&within_a, &within_b), Change::Breaking);
    assert_eq!(classify_change(&within_b, &within_a), Change::Breaking);

    let mut no_key = unique.clone();
    no_key.keys.clear();
    assert_eq!(classify_change(&no_key, &unique), Change::Breaking);
    assert_eq!(
        classify_change(&unique, &no_key),
        Change::BackwardCompatible
    );
}

#[test]
fn registration_requires_closed_replayable_theories_and_is_byte_idempotent() {
    let base = |name: &str| Theory {
        name: name.into(),
        sorts: vec![Sort {
            name: "Vertex".into(),
            params: vec![],
            kind: "structural".into(),
            constructors: None,
        }],
        ..Theory::default()
    };
    let schema_base = base("SchemaBase");
    let instance_base = base("InstanceBase");
    let theories = BTreeMap::from([
        (schema_base.name.clone(), schema_base.clone()),
        (instance_base.name.clone(), instance_base.clone()),
    ]);
    let mut schema_theory = schema_base;
    schema_theory.name = "SchemaResult".into();
    let mut instance_theory = instance_base;
    instance_theory.name = "InstanceResult".into();
    let registration = Registration {
        schema: Schema::default(),
        schema_theory,
        instance_theory,
        schema_composition: CompositionSpec {
            result_name: "SchemaResult".into(),
            steps: vec![CompositionStep::Base("SchemaBase".into())],
            version: 0,
        },
        instance_composition: CompositionSpec {
            result_name: "InstanceResult".into(),
            steps: vec![CompositionStep::Base("InstanceBase".into())],
            version: 0,
        },
    };
    let mut registry = BTreeMap::new();
    assert_eq!(
        register(&mut registry, &theories, "protocol", registration.clone()),
        Ok(())
    );
    assert_eq!(
        register(&mut registry, &theories, "protocol", registration),
        Ok(())
    );
}

#[test]
fn declared_numeric_equality_is_exact_at_unbounded_exponents_and_in_buckets() {
    let numeric = KeyEquality::NumericValue;
    let one = Value::Int(1);
    let one_point_zero = Value::Decimal(Decimal::Finite(DecimalValue::parse("1.0").unwrap()));
    assert_eq!(
        panproto_reference::keys::key_equivalence(&numeric, &one, &one_point_zero),
        Some(true)
    );
    let huge = Value::Decimal(Decimal::Finite(
        DecimalValue::parse("1e9223372036854775808").unwrap(),
    ));
    let same_huge = Value::Decimal(Decimal::Finite(
        DecimalValue::parse("10e9223372036854775807").unwrap(),
    ));
    assert_eq!(
        panproto_reference::keys::key_equivalence(&numeric, &huge, &same_huge),
        Some(true)
    );
    let huge_integer = Value::BigInt(BigIntValue {
        negative: false,
        significand: Nat::from_u64(1),
        zeros: Nat::parse("9223372036854775808").unwrap(),
    });
    assert_eq!(
        panproto_reference::keys::key_equivalence(&numeric, &huge_integer, &huge),
        Some(true)
    );

    let id = SEdge::new("record", "scalar", "prop", Some("id"));
    let key = Key {
        id: "numeric".into(),
        parts: vec![KeyPart::Path(vec![id])],
        role: KeyRole::Unique,
        scope: KeyScope::Document,
        equality: vec![numeric],
    };
    let graph = Graph {
        nodes: BTreeMap::from([
            (0, Node::new(0, "record").with_field("id", one)),
            (1, Node::new(1, "record").with_field("id", one_point_zero)),
        ]),
        edges: BTreeMap::new(),
    };
    let (buckets, outside) = panproto_reference::keys::buckets(&graph, "record", &key).unwrap();
    assert!(outside.is_empty());
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets.into_values().next().unwrap().1, vec![0, 1]);
}

#[test]
fn target_sets_require_distinct_compatible_declared_keys() {
    let mut schema = Schema::default().vertex("a", "obj").vertex("b", "obj");
    for vertex in ["a", "b"] {
        schema.keys.insert(
            vertex.into(),
            vec![Key {
                id: "id".into(),
                parts: vec![KeyPart::Path(vec![SEdge::new(
                    vertex,
                    "scalar",
                    "prop",
                    Some("id"),
                )])],
                role: KeyRole::Identifier,
                scope: KeyScope::Document,
                equality: vec![KeyEquality::NumericValue],
            }],
        );
    }
    schema.target_sets.insert(
        "ids".into(),
        TargetSet {
            members: vec![("a".into(), "id".into()), ("b".into(), "id".into())],
            scope: KeyScope::Document,
        },
    );
    assert_eq!(validate_declarations(&schema), Ok(()));
    schema.target_sets.get_mut("ids").unwrap().members[1].1 = "missing".into();
    assert!(validate_declarations(&schema).is_err());
}

#[test]
fn latin1_and_base64_laws_hold_on_the_fast_exhaustive_scope() {
    let strings = panproto_reference::enumerate::byte_strings(2).unwrap();
    assert_eq!(strings.len(), 65_793);
    for bytes in strings {
        assert!(bytes::latin1_decode_encode(&bytes));
        assert!(bytes::padded_bytes_decode_encode(&bytes));
        let padded = bytes::encode_base64(&bytes);
        assert!(bytes::padded_text_encode_decode(&padded));
        let unpadded = padded.trim_end_matches('=');
        assert_eq!(bytes::decode_base64(unpadded, false), Ok(bytes));
    }
    assert!(!bytes::latin1_encode_decode("Ā"));
    assert!(bytes::latin1_encode_decode("\u{0}\u{ff}"));
}

#[test]
#[ignore = "full O12 A6 audit: all 16,843,009 byte strings through length three"]
fn padded_base64_laws_hold_on_the_contract_scope() {
    for bytes in panproto_reference::enumerate::byte_strings(3).unwrap() {
        assert!(bytes::padded_bytes_decode_encode(&bytes));
        let padded = bytes::encode_base64(&bytes);
        assert!(bytes::padded_text_encode_decode(&padded));
        assert_eq!(
            bytes::decode_base64(padded.trim_end_matches('='), false),
            Ok(bytes)
        );
    }
}

#[test]
fn epoch_count_conversions_are_two_sided_on_the_contract_centre() {
    for unit in [
        TimeUnit::Seconds,
        TimeUnit::Millis,
        TimeUnit::Micros,
        TimeUnit::Nanos,
    ] {
        for instant in [false, true] {
            for count in -10_000..=10_000 {
                let temporal = from_epoch_count(count, unit, instant).unwrap();
                assert_eq!(to_epoch_count(&temporal, unit, instant), Some(count));
            }
        }
    }
}

#[test]
fn epoch_inverse_accepts_exactly_the_canonical_calendar_image() {
    let base = from_epoch_count(0, TimeUnit::Seconds, true).unwrap();
    let mut witnesses = Vec::new();

    let mut wide_year = base.clone();
    wide_year
        .date
        .as_mut()
        .unwrap()
        .year
        .as_mut()
        .unwrap()
        .width = 5;
    witnesses.push(wide_year);

    let mut bad_month = base.clone();
    bad_month.date.as_mut().unwrap().month = Some(13);
    witnesses.push(bad_month);

    let mut bad_day = base.clone();
    bad_day.date.as_mut().unwrap().day = Some(32);
    witnesses.push(bad_day);

    let mut bad_february = base.clone();
    let date = bad_february.date.as_mut().unwrap();
    date.month = Some(2);
    date.day = Some(29);
    witnesses.push(bad_february);

    for (field, value) in [(0, 24), (1, 60), (2, 60)] {
        let mut invalid = base.clone();
        let time = invalid.time.as_mut().unwrap();
        match field {
            0 => time.hour = Some(value),
            1 => time.minute = Some(value),
            _ => time.second = Some(value),
        }
        witnesses.push(invalid);
    }

    for witness in witnesses {
        assert_eq!(
            to_epoch_count(&witness, TimeUnit::Seconds, true),
            None,
            "{witness:?}"
        );
    }
    assert_eq!(to_epoch_count(&base, TimeUnit::Seconds, true), Some(0));
}

#[test]
fn epoch_count_conversions_are_two_sided_at_each_exact_domain_bound() {
    let epoch = day_number(1970, 1, 1);
    let first_second = (day_number(i64::MIN, 1, 1) - epoch)
        .checked_mul(86_400)
        .unwrap();
    let after_last_second = (day_number(i64::MAX, 12, 31) + 1 - epoch)
        .checked_mul(86_400)
        .unwrap();
    for unit in [
        TimeUnit::Seconds,
        TimeUnit::Millis,
        TimeUnit::Micros,
        TimeUnit::Nanos,
    ] {
        let ticks = match unit {
            TimeUnit::Seconds => 1,
            TimeUnit::Millis => 1_000,
            TimeUnit::Micros => 1_000_000,
            TimeUnit::Nanos => 1_000_000_000,
        };
        let lower = first_second.checked_mul(ticks).unwrap();
        let upper_exclusive = after_last_second.checked_mul(ticks).unwrap();
        for instant in [false, true] {
            assert_eq!(from_epoch_count(lower - 1, unit, instant), None);
            assert_eq!(from_epoch_count(upper_exclusive, unit, instant), None);
            for distance in 0..=10_000 {
                for count in [lower + distance, upper_exclusive - 1 - distance] {
                    let value = from_epoch_count(count, unit, instant).unwrap();
                    assert_eq!(to_epoch_count(&value, unit, instant), Some(count));
                }
            }
            assert_eq!(from_epoch_count(i128::MIN, unit, instant), None);
            assert_eq!(from_epoch_count(i128::MAX, unit, instant), None);
        }
    }
}

#[test]
fn extreme_years_parse_display_and_compare_without_narrow_arithmetic() {
    let minimum = "-9223372036854775808-01-01";
    let maximum = "9223372036854775807-12-31";
    let min_value = parse(TemporalKind::Date, minimum).unwrap();
    let max_value = parse(TemporalKind::Date, maximum).unwrap();
    assert!(parse_display(min_value.lexical_form().unwrap(), minimum));
    assert!(parse_display(max_value.lexical_form().unwrap(), maximum));
    assert_eq!(min_value.to_string(), minimum);
    assert_eq!(max_value.to_string(), maximum);
    assert_eq!(
        cmp_chronological(&min_value, &max_value),
        Some(Ordering::Less)
    );
    assert_eq!(
        cmp_chronological(&max_value, &min_value),
        Some(Ordering::Greater)
    );
}

#[test]
fn form_indexing_retains_both_meanings_of_an_ambiguous_temporal() {
    let month = parse(TemporalForm::Date, "--04").unwrap();
    let second = parse(TemporalForm::Time, "--04").unwrap();
    assert_ne!(month, second);
    assert_eq!(month.to_string(), "--04");
    assert_eq!(second.to_string(), "--04");
    assert_eq!(month.form(), Some(TemporalForm::Date));
    assert_eq!(second.form(), Some(TemporalForm::Time));
    assert_eq!(
        parse_union("--04"),
        Err(TemporalParseError::AmbiguousTemporal)
    );
    assert_eq!(
        parse_union("not-a-temporal"),
        Err(TemporalParseError::InvalidTemporal)
    );

    let year_month = parse(TemporalForm::Date, "1973-06").unwrap();
    let year_with_offset = Temporal {
        date: Some(panproto_reference::temporal::DateFields {
            year: Some(panproto_reference::temporal::Year {
                value: 1973,
                negative_zero: false,
                width: 4,
            }),
            month: None,
            day: None,
        }),
        time: None,
        separator: None,
        offset: Offset::Fixed(-360, OffsetForm::HourOnly),
        infinity: None,
    };
    assert_ne!(year_month, year_with_offset);
    assert_eq!(year_month.to_string(), "1973-06");
    assert_eq!(year_with_offset.to_string(), "1973-06");
    assert_eq!(
        year_month.to_string().as_bytes(),
        year_with_offset.to_string().as_bytes()
    );
    assert_eq!(
        parse(TemporalKind::Date, "1973-06"),
        Some(year_month.clone())
    );
    assert_ne!(
        parse(TemporalKind::Date, "1973-06"),
        Some(year_with_offset.clone())
    );
    assert_ne!(year_month.lexical_form(), year_with_offset.lexical_form());
    for value in [&year_month, &year_with_offset] {
        let descriptor = value.lexical_form().unwrap();
        assert_eq!(
            Temporal::parse_as(descriptor, "1973-06"),
            Some(value.clone())
        );
        assert!(display_parse(value));
    }
    assert_eq!(
        parse_union("1973-06"),
        Err(TemporalParseError::AmbiguousTemporal)
    );
}

#[test]
fn every_extended_temporal_carrier_has_injective_canonical_bytes() {
    let finite = [
        (TemporalForm::Date, "2018Z"),
        (TemporalForm::Date, "1973-06+0400"),
        (TemporalForm::Date, "--04-12-04"),
        (TemporalForm::Time, "23:20+04"),
        (TemporalForm::Time, "-20:50-04:56:02"),
        (TemporalForm::DateTime, "---15T09:45:00+05:30:00"),
    ];
    let mut values: Vec<Temporal> = finite
        .into_iter()
        .map(|(form, text)| parse(form, text).unwrap())
        .collect();
    values.push(
        Temporal::infinite(
            TemporalInfinity::Negative,
            TemporalForm::Date,
            Offset::Local,
        )
        .unwrap(),
    );
    values.push(
        Temporal::infinite(
            TemporalInfinity::Positive,
            TemporalForm::DateTime,
            Offset::Zulu(panproto_reference::temporal::ZuluCase::Upper),
        )
        .unwrap(),
    );
    let mut unwritten = parse(TemporalForm::DateTime, "2026-01-01T00:00:00+05:30").unwrap();
    unwritten.offset = Offset::Fixed(330, OffsetForm::Unwritten);
    assert_eq!(unwritten.to_string(), "2026-01-01T00:00:00");
    values.push(unwritten);

    let mut bytes = BTreeSet::new();
    for value in values {
        let wrapped = Value::Temporal(value.clone());
        let encoded = panproto_reference::canonical::value_bytes(&wrapped).unwrap();
        assert_eq!(decode_value(&encoded), Some(wrapped));
        assert!(bytes.insert(encoded), "duplicate bytes for {value:?}");
    }
}

#[test]
fn truncated_temporals_compare_offsets_by_minutes_not_written_seconds() {
    let first = parse(TemporalForm::Time, "-20:50+04:56:01").unwrap();
    let second = parse(TemporalForm::Time, "-20:50+04:56:02").unwrap();
    assert_eq!(first.offset.minutes(), second.offset.minutes());
    assert_eq!(cmp_chronological(&first, &second), Some(Ordering::Equal));
}

#[test]
fn value_io_oracles_are_independent_and_toml_rejects_reduced_years() {
    use panproto_reference::value_io::{
        Route, emit, parse as parse_scalar, parse_cst, round_trip, round_trip_cst, temporal,
    };

    assert_eq!(parse_scalar(Route::Json, b"1"), Ok(Value::Int(1)));
    assert_eq!(emit(Route::Json, &Value::Int(1)), Ok(b"1".to_vec()));
    assert_ne!(parse_scalar(Route::Json, b"2"), Ok(Value::Int(1)));
    assert_ne!(emit(Route::Json, &Value::Int(1)), Ok(Vec::new()));
    assert_eq!(parse_cst(Route::Json, b"1").unwrap().source, b"1");
    assert!(round_trip_cst(Route::Json, b"\"kept\""));
    assert_eq!(
        parse_scalar(Route::Json, br#""\ud83d\ude00\u0001""#),
        Ok(Value::Str("😀\u{1}".into()))
    );
    assert_eq!(
        emit(Route::Json, &Value::Str("😀\u{1}".into())),
        Ok("\"😀\\u0001\"".as_bytes().to_vec())
    );
    assert_eq!(
        parse_scalar(Route::Json, br#""\ud83d""#),
        Err(panproto_reference::value_io::ValueIoError::Malformed)
    );
    assert_eq!(
        parse_scalar(Route::Xml, b"&lt;x&gt;&amp;&#x1F600;"),
        Ok(Value::Str("<x>&😀".into()))
    );
    assert_eq!(
        emit(Route::Xml, &Value::Str("<x>&\"".into())),
        Ok(b"&lt;x&gt;&amp;&quot;".to_vec())
    );
    assert_eq!(
        parse_scalar(Route::Csv, br#""a,""b""#),
        Ok(Value::Str("a,\"b".into()))
    );
    assert_eq!(
        emit(Route::Csv, &Value::Str("a,\"b".into())),
        Ok(br#""a,""b""#.to_vec())
    );
    assert_eq!(
        emit(Route::Tsv, &Value::Str("a\tb".into())),
        Err(panproto_reference::value_io::ValueIoError::NotRepresentable)
    );
    assert_eq!(
        parse_scalar(Route::Yaml, b"plain"),
        Ok(Value::Str("plain".into()))
    );

    let exact_decimal = Value::Decimal(panproto_reference::exact::parse_decimal("100e-2").unwrap());
    for route in [
        Route::Json,
        Route::Toml,
        Route::Yaml,
        Route::Xml,
        Route::Csv,
        Route::Tsv,
    ] {
        assert!(round_trip(route, &exact_decimal), "{route:?}");
    }
    let positive_exponent =
        Value::Decimal(panproto_reference::exact::parse_decimal("1e1").unwrap());
    assert_eq!(
        emit(Route::Xml, &positive_exponent),
        Err(panproto_reference::value_io::ValueIoError::NotRepresentable)
    );
    assert!(round_trip(
        Route::Yaml,
        &Value::Float(panproto_reference::float::CANONICAL_NAN_BITS)
    ));
    assert_eq!(
        emit(Route::Yaml, &Value::Float(0x7ff8_0000_0000_0001)),
        Err(panproto_reference::value_io::ValueIoError::NotRepresentable)
    );

    let reduced_year = parse(TemporalForm::Date, "2018").unwrap();
    let full_date = parse(TemporalForm::Date, "2018-01-01").unwrap();
    assert!(!temporal(Route::Toml, &reduced_year));
    assert!(temporal(Route::Toml, &full_date));
    assert!(round_trip(Route::Toml, &Value::Temporal(full_date.clone())));
    assert!(round_trip(Route::Json, &Value::Temporal(full_date)));
    assert_eq!(
        emit(Route::Toml, &Value::Temporal(reduced_year)),
        Err(panproto_reference::value_io::ValueIoError::NotRepresentable)
    );
}

#[test]
fn second_offsets_unwritten_offsets_and_infinities_obey_the_order() {
    let west = parse(TemporalForm::DateTime, "1849-12-31 19:03:58-04:56:02").unwrap();
    let utc = parse(TemporalForm::DateTime, "1850-01-01T00:00:00Z").unwrap();
    assert_eq!(cmp_chronological(&west, &utc), Some(Ordering::Equal));

    let mut unwritten = utc.clone();
    unwritten.offset = Offset::Fixed(0, OffsetForm::Unwritten);
    assert_eq!(cmp_chronological(&unwritten, &utc), Some(Ordering::Equal));

    let negative = Temporal::infinite(
        TemporalInfinity::Negative,
        TemporalForm::DateTime,
        Offset::Local,
    )
    .unwrap();
    let positive = Temporal::infinite(
        TemporalInfinity::Positive,
        TemporalForm::DateTime,
        Offset::Local,
    )
    .unwrap();
    let finite = parse(TemporalForm::DateTime, "2026-01-01T00:00:00").unwrap();
    assert_eq!(cmp_chronological(&negative, &finite), Some(Ordering::Less));
    assert_eq!(cmp_chronological(&finite, &positive), Some(Ordering::Less));
    assert_eq!(
        cmp_chronological(&negative, &positive),
        Some(Ordering::Less)
    );
}

#[test]
fn rfc3986_matches_the_normal_and_abnormal_examples() {
    let base = "http://a/b/c/d;p?q";
    let examples = [
        ("g:h", "g:h"),
        ("g", "http://a/b/c/g"),
        ("./g", "http://a/b/c/g"),
        ("g/", "http://a/b/c/g/"),
        ("/g", "http://a/g"),
        ("//g", "http://g"),
        ("?y", "http://a/b/c/d;p?y"),
        ("g?y", "http://a/b/c/g?y"),
        ("#s", "http://a/b/c/d;p?q#s"),
        ("g#s", "http://a/b/c/g#s"),
        ("g?y#s", "http://a/b/c/g?y#s"),
        (";x", "http://a/b/c/;x"),
        ("g;x", "http://a/b/c/g;x"),
        ("g;x?y#s", "http://a/b/c/g;x?y#s"),
        ("", "http://a/b/c/d;p?q"),
        (".", "http://a/b/c/"),
        ("./", "http://a/b/c/"),
        ("..", "http://a/b/"),
        ("../", "http://a/b/"),
        ("../g", "http://a/b/g"),
        ("../..", "http://a/"),
        ("../../", "http://a/"),
        ("../../g", "http://a/g"),
        ("../../../g", "http://a/g"),
        ("../../../../g", "http://a/g"),
        ("/./g", "http://a/g"),
        ("/../g", "http://a/g"),
        ("g.", "http://a/b/c/g."),
        (".g", "http://a/b/c/.g"),
        ("g..", "http://a/b/c/g.."),
        ("..g", "http://a/b/c/..g"),
        ("./../g", "http://a/b/g"),
        ("./g/.", "http://a/b/c/g/"),
        ("g/./h", "http://a/b/c/g/h"),
        ("g/../h", "http://a/b/c/h"),
        ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
        ("g;x=1/../y", "http://a/b/c/y"),
        ("g?y/./x", "http://a/b/c/g?y/./x"),
        ("g?y/../x", "http://a/b/c/g?y/../x"),
        ("g#s/./x", "http://a/b/c/g#s/./x"),
        ("g#s/../x", "http://a/b/c/g#s/../x"),
        ("http:g", "http:g"),
        ("g//h", "http://a/b/c/g//h"),
        ("//g//h", "http://g//h"),
    ];
    for (reference, expected) in examples {
        assert_eq!(
            rfc3986(base, reference).as_deref(),
            Some(expected),
            "{reference}"
        );
    }
}

#[test]
fn declared_key_relations_and_sort_order_have_the_stated_boundaries() {
    let one = Value::Int(1);
    let one_float = Value::Float(1.0f64.to_bits());
    assert_eq!(
        panproto_reference::keys::key_equivalence(&KeyEquality::NumericValue, &one, &one_float),
        Some(true)
    );
    assert_eq!(
        panproto_reference::keys::key_equivalence(
            &KeyEquality::NumericValueWithinKind(NumericPrimitive::Any),
            &one,
            &one_float,
        ),
        Some(false)
    );
    let nocase = KeyEquality::Sql {
        collation: SqlCollation::AsciiCaseInsensitive,
        pad: SqlPad::PadSpace,
    };
    assert_eq!(
        panproto_reference::keys::key_equivalence(
            &nocase,
            &Value::Str("A  ".into()),
            &Value::Str("a".into()),
        ),
        Some(true)
    );
    assert_eq!(
        panproto_reference::keys::sort_order(&Value::Int(1), &Value::Int(2)),
        Some(Ordering::Less)
    );
    assert_eq!(
        panproto_reference::keys::key_order(&[Value::Int(1), Value::Int(1), Value::Int(2)]),
        Some(true)
    );
    assert_eq!(
        panproto_reference::keys::key_order(&[Value::Int(2), Value::Int(1)]),
        Some(false)
    );

    let numeric_classes = [
        vec![
            Value::Float(f64::NEG_INFINITY.to_bits()),
            Value::Decimal(Decimal::Infinite { negative: true }),
        ],
        vec![Value::Int(0), Value::Float(0.0f64.to_bits())],
        vec![
            Value::Float(f64::INFINITY.to_bits()),
            Value::Decimal(Decimal::Infinite { negative: false }),
        ],
    ];
    for (left_class, left_values) in numeric_classes.iter().enumerate() {
        for (right_class, right_values) in numeric_classes.iter().enumerate() {
            let expected = left_class.cmp(&right_class);
            for left in left_values {
                for right in right_values {
                    assert_eq!(
                        panproto_reference::keys::sort_order(left, right),
                        Some(expected),
                        "{left:?} versus {right:?}"
                    );
                }
            }
        }
    }
    let nan = Value::Float(f64::NAN.to_bits());
    for numeric in numeric_classes.iter().flatten() {
        assert_eq!(panproto_reference::keys::sort_order(&nan, numeric), None);
        assert_eq!(panproto_reference::keys::sort_order(numeric, &nan), None);
    }
    assert_eq!(panproto_reference::keys::sort_order(&nan, &nan), None);
    assert_eq!(
        panproto_reference::keys::sort_order(&Value::Int(0), &Value::Str("0".into())),
        None
    );
}

#[test]
fn fan_incidence_is_neither_ancestry_nor_a_position_parent() {
    let within = SEdge::new("parent", "item", "ordered", Some("items"));
    let mut graph = Graph {
        nodes: [
            (0, Node::new(0, "scope")),
            (1, Node::new(1, "fan")),
            (2, Node::new(2, "item")),
            (3, Node::new(3, "parent")),
            (4, Node::new(4, "item")),
        ]
        .into(),
        edges: BTreeMap::new(),
    };
    graph.nodes.get_mut(&1).unwrap().shape = NodeShape::HyperEdge;
    graph.edges.insert(
        0,
        GEdge {
            src: 0,
            tgt: 1,
            label: SEdge::new("scope", "fan", "foreign", None),
            position: None,
        },
    );
    graph.edges.insert(
        1,
        GEdge {
            src: 1,
            tgt: 2,
            label: within.clone(),
            position: Some(0),
        },
    );
    graph.edges.insert(
        2,
        GEdge {
            src: 3,
            tgt: 4,
            label: within.clone(),
            position: Some(0),
        },
    );
    assert_eq!(
        panproto_reference::keys::nearest_ancestor(&graph, 2, "scope"),
        None
    );
    let position_key = Key {
        id: "position".to_owned(),
        parts: vec![KeyPart::Position { within }],
        role: KeyRole::Unique,
        scope: KeyScope::Global,
        equality: Vec::new(),
    };
    assert_eq!(
        panproto_reference::keys::key_value(&graph, 2, &position_key),
        None
    );
    assert_eq!(
        panproto_reference::keys::key_value(&graph, 4, &position_key),
        Some(vec![Value::Int(0)])
    );
}

#[test]
fn an_index_key_never_derives_a_resolution_pair() {
    for index in [None, Some(0), Some(1)] {
        for target in [None, Some(0), Some(1)] {
            assert!(!panproto_reference::resolution::index_target(index, target));
        }
    }
}

#[test]
fn standoff_forms_are_left_inverses_and_entities_are_bounded() {
    for origin in [0, 1] {
        for length_form in [false, true] {
            for start in 0..=4 {
                for end in start..=4 {
                    let written = standoff_inverse(start, end, origin, length_form).unwrap();
                    assert_eq!(
                        standoff_form(written.0, written.1, origin, length_form),
                        Some((start, end))
                    );
                }
            }
        }
    }
    let declarations = BTreeMap::from([
        ("a".to_owned(), "A".to_owned()),
        ("b".to_owned(), "&a;B".to_owned()),
    ]);
    assert_eq!(
        panproto_reference::xml::expand_entities("x&b;y", &declarations).as_deref(),
        Ok("xABy")
    );
    assert_eq!(
        panproto_reference::xml::expand_entities(
            "&amp;&lt;&gt;&apos;&quot; &b; &amp;b;",
            &declarations,
        )
        .as_deref(),
        Ok("&<>'\" AB &b;")
    );
    assert_eq!(
        panproto_reference::xml::expand_entities(
            "&#65;&#x41;&#9;&#xA;&#13;&#x10FFFF;",
            &declarations,
        )
        .as_deref(),
        Ok("AA\t\n\r\u{10ffff}")
    );
    for malformed in [
        "&#;",
        "&#x;",
        "&#xg;",
        "&#0;",
        "&#1;",
        "&#xD800;",
        "&#xFFFE;",
        "&#xFFFF;",
        "&#999999999999;",
        "&;",
        "&1x;",
        "&x y;",
    ] {
        assert_eq!(
            panproto_reference::xml::expand_entities(malformed, &declarations),
            Err(panproto_reference::xml::EntityError::Malformed),
            "{malformed}",
        );
    }
    let recursive = BTreeMap::from([("r".to_owned(), "&r;".to_owned())]);
    assert_eq!(
        panproto_reference::xml::expand_entities("&r;", &recursive),
        Err(panproto_reference::xml::EntityError::Recursive(
            "r".to_owned()
        ))
    );

    let mut sixteen = BTreeMap::new();
    for depth in 0..16 {
        let replacement = if depth == 15 {
            "x".to_owned()
        } else {
            format!("&e{};", depth + 1)
        };
        sixteen.insert(format!("e{depth}"), replacement);
    }
    assert_eq!(
        panproto_reference::xml::expand_entities("&e0;", &sixteen).as_deref(),
        Ok("x")
    );
    sixteen.insert("e15".to_owned(), "&e16;".to_owned());
    sixteen.insert("e16".to_owned(), "x".to_owned());
    assert_eq!(
        panproto_reference::xml::expand_entities("&e0;", &sixteen),
        Err(panproto_reference::xml::EntityError::Depth)
    );
}

#[test]
fn tabular_edge_cases_are_literal() {
    assert!(panproto_reference::value_io::tsv_comment("#data"));
    assert!(panproto_reference::value_io::tabular_missing("_"));
    assert!(panproto_reference::value_io::tabular_empty_row(1, ""));
    assert!(panproto_reference::value_io::tsv_quote("x\"y"));
    assert!(!panproto_reference::value_io::tsv_quote("x\ty"));
}
