#![allow(clippy::unwrap_used, clippy::expect_used)]
//! `coerce_value` reads a value back through its lexeme under a column's
//! declared kind: an `Ok` result denotes what the input's lexeme denotes,
//! and coercing the result again returns it unchanged.

use panproto_inst::Value;
use panproto_inst::value::coerce::coerce_value;
use panproto_inst::value::{DecimalValue, FidelityReason, Temporal, ValueSite};
use panproto_schema::{BytesText, ScalarKind, ValueSpec};
use proptest::prelude::*;

/// One spec of every scalar kind, with the parameters that change what a
/// kind holds.
fn specs() -> Vec<ValueSpec> {
    let spec = ValueSpec::new;
    let int = |bits, signed| spec(ScalarKind::Int { bits, signed });
    vec![
        spec(ScalarKind::Bool),
        int(8, true),
        int(16, false),
        int(32, true),
        int(64, true),
        int(64, false),
        spec(ScalarKind::BigInt),
        spec(ScalarKind::Float { bits: 32 }),
        spec(ScalarKind::Float { bits: 64 }),
        spec(ScalarKind::Decimal),
        ValueSpec {
            precision: Some(5),
            scale: Some(2),
            ..spec(ScalarKind::Decimal)
        },
        spec(ScalarKind::Str),
        spec(ScalarKind::Token),
        spec(ScalarKind::Bytes(BytesText::Hex)),
        spec(ScalarKind::Bytes(BytesText::Base64)),
        spec(ScalarKind::Date),
        spec(ScalarKind::Time),
        spec(ScalarKind::DateTime),
        ValueSpec {
            with_time_zone: Some(true),
            ..spec(ScalarKind::DateTime)
        },
        spec(ScalarKind::Any),
    ]
}

/// Texts that some kind reads: numerals, booleans, dates, encodings, words.
fn texts() -> impl Strategy<Value = String> {
    prop_oneof![
        any::<i64>().prop_map(|n| n.to_string()),
        "-?[0-9]{1,3}\\.[0-9]{1,4}",
        "[0-9]{19,25}",
        Just("true".to_owned()),
        Just("FALSE".to_owned()),
        Just("2024-05-06".to_owned()),
        Just("12:30:05".to_owned()),
        Just("2024-05-06T12:30:05Z".to_owned()),
        Just("2024-05-06T12:30:05".to_owned()),
        "([0-9a-f]{2}){0,4}",
        Just("aGk=".to_owned()),
        "[a-z]{0,5}",
    ]
}

fn values() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::Int),
        "[1-9][0-9]{19,24}".prop_map(|t| Value::parse_integer(&t).unwrap()),
        "-?[0-9]{1,4}\\.[0-9]{1,5}".prop_map(|t| Value::Decimal(DecimalValue::parse(&t).unwrap())),
        any::<f64>().prop_map(Value::Float),
        any::<i32>().prop_map(|n| Value::Float(f64::from(n))),
        any::<f32>().prop_map(|f| Value::Float(f64::from(f))),
        Just(Value::Float(-0.0)),
        Just(Value::Float(1e300)),
        Just(Value::Float(0.1)),
        texts().prop_map(Value::Str),
        texts().prop_map(Value::Token),
        Just(Value::Temporal(Temporal::parse("2024-05-06T12:30:05+02:00").unwrap())),
        Just(Value::Temporal(Temporal::parse("2024-05-06").unwrap())),
        Just(Value::Null),
        Just(Value::Bytes(vec![1, 2, 3])),
        Just(Value::List(vec![Value::Int(1)])),
        Just(Value::LabeledNull(4)),
    ]
}

/// The text a value's lexeme carries, read independently of the code under
/// test: under a 32-bit float kind a float a 32-bit float holds exactly is
/// written as that `f32`.
fn lexeme_text(value: &Value, kind: &ScalarKind) -> Option<String> {
    Some(match value {
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::BigInt(n) => n.to_string(),
        Value::Decimal(d) => d.to_string(),
        Value::Float(f) => {
            let narrow = f.to_string().parse::<f32>().ok();
            match (kind, narrow) {
                (ScalarKind::Float { bits: 32 }, Some(n)) if f64::from(n).to_bits() == f.to_bits() => {
                    n.to_string()
                }
                _ => f.to_string(),
            }
        }
        Value::Str(s) | Value::Token(s) => s.clone(),
        Value::Temporal(t) => t.to_string(),
        Value::Null => "null".to_owned(),
        _ => return None,
    })
}

fn hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

fn base64(text: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let body = text.trim_end_matches('=');
    let mut bits = 0_u32;
    let mut count = 0;
    let mut out = Vec::new();
    for c in body.bytes() {
        let v = u32::try_from(ALPHABET.iter().position(|&a| a == c)?).ok()?;
        bits = (bits << 6) | v;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push(u8::try_from((bits >> count) & 0xff).ok()?);
        }
    }
    Some(out)
}

/// Whether `result` denotes what `text` denotes when read as `result`'s
/// variant.
fn denotes_same(text: &str, result: &Value, kind: &ScalarKind) -> bool {
    match result {
        Value::Int(_) | Value::BigInt(_) | Value::Decimal(_) | Value::Float(_) => {
            let result_text = lexeme_text(result, kind).unwrap();
            match (DecimalValue::parse(text), DecimalValue::parse(&result_text)) {
                (Ok(a), Ok(b)) => a.cmp_numeric(&b).is_eq(),
                _ => text == result_text,
            }
        }
        Value::Bool(b) => text.eq_ignore_ascii_case(&b.to_string()),
        Value::Str(s) | Value::Token(s) => s == text,
        Value::Temporal(t) => Temporal::parse(text).is_ok_and(|p| p.to_string() == t.to_string()),
        Value::Bytes(bytes) => match kind {
            ScalarKind::Bytes(BytesText::Hex) => hex(text).as_ref() == Some(bytes),
            ScalarKind::Bytes(BytesText::Base64) => base64(text).as_ref() == Some(bytes),
            _ => false,
        },
        Value::Null => text == "null",
        _ => false,
    }
}

fn same(a: &Value, b: &Value) -> bool {
    a.canonical_cmp(b).is_eq()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    #[test]
    fn coerce_value_loses_nothing(value in values(), index in 0..20_usize) {
        let all = specs();
        let spec = &all[index % all.len()];
        let site = ValueSite::new("v", None);
        if let Ok(result) = coerce_value(&value, spec, site) {
            match (lexeme_text(&value, &spec.kind), &value) {
                _ if spec.kind == ScalarKind::Any => prop_assert!(same(&result, &value)),
                (_, Value::Float(f)) if !f.is_finite() => prop_assert!(same(&result, &value)),
                (Some(text), _) => prop_assert!(
                    denotes_same(&text, &result, &spec.kind),
                    "{value:?} as {:?} gave {result:?}", spec.kind
                ),
                (None, _) => prop_assert!(same(&result, &value)),
            }
        }
    }

    #[test]
    fn coerce_value_is_idempotent(value in values(), index in 0..20_usize) {
        let all = specs();
        let spec = &all[index % all.len()];
        let site = ValueSite::new("v", None);
        if let Ok(once) = coerce_value(&value, spec, site.clone()) {
            let twice = coerce_value(&once, spec, site);
            prop_assert!(
                twice.as_ref().is_ok_and(|t| same(t, &once)),
                "{value:?} as {:?}: {once:?} then {twice:?}", spec.kind
            );
        }
    }
}

#[test]
fn a_non_finite_float_is_kept_only_by_a_float_kind() {
    let site = ValueSite::new("v", None);
    let float = ValueSpec::new(ScalarKind::Float { bits: 64 });
    let decimal = ValueSpec::new(ScalarKind::Decimal);
    let nan = Value::Float(f64::NAN);
    assert!(same(&coerce_value(&nan, &float, site.clone()).unwrap(), &nan));
    let error = coerce_value(&nan, &decimal, site).unwrap_err();
    assert_eq!(error.reason, FidelityReason::NonFinite);
    assert_eq!(error.site.vertex.as_ref(), "v");
}

#[test]
fn a_32_bit_column_writes_its_float_as_a_32_bit_float() {
    let site = ValueSite::new("v", None);
    let float32 = ValueSpec::new(ScalarKind::Float { bits: 32 });
    let stored = coerce_value(&Value::Float(0.1), &float32, site.clone()).unwrap();
    assert!(same(&stored, &Value::Float(f64::from(0.1_f32))));
    assert!(same(
        &coerce_value(&stored, &float32, site).unwrap(),
        &stored
    ));
}
