//! A quoted numeral is read as a numeral under a numeric kind, and a kind
//! named by a sibling discriminator is read through the spec's table.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, HashMap};

use panproto_inst::value::DecimalValue;
use panproto_inst::value::coerce::{LexClass, Lexeme, coerce, coerce_discriminated};
use panproto_inst::{FidelityReason, Value};
use panproto_schema::{BytesText, ScalarKind, ValueSpec};

fn read(text: &str, class: LexClass, spec: &ValueSpec) -> Result<Value, FidelityReason> {
    coerce(&Lexeme::new(text, class), spec).map_err(|e| e.reason)
}

/// The table of the item's example: integer, float and text.
fn table() -> BTreeMap<String, ScalarKind> {
    BTreeMap::from([
        (
            "integer".to_owned(),
            ScalarKind::Int {
                bits: 32,
                signed: true,
            },
        ),
        ("float".to_owned(), ScalarKind::Float { bits: 64 }),
        ("text".to_owned(), ScalarKind::Str),
    ])
}

fn opaque(tag: &str, text: &str) -> Value {
    Value::Opaque {
        type_: tag.to_owned(),
        fields: HashMap::from([("value".to_owned(), Value::Str(text.to_owned()))]),
    }
}

/// Quoted numerals read as the unquoted ones do under a numeric kind and
/// stay strings under `Any`; a discriminator table reads a tagged lexeme
/// under its tag's kind and keeps an unknown tag's lexeme as an opaque
/// value, byte for byte; an untagged lexeme, and every lexeme under a spec
/// without a table, reads under the spec's own kind. The law is checked on
/// every tag of the table and one unknown tag, crossed with one lexeme of
/// each class.
#[test]
fn a_discriminator_picks_the_kind() {
    let int64 = ValueSpec::new(ScalarKind::Int {
        bits: 64,
        signed: true,
    });
    for (text, kind, expected) in [
        ("123", int64.clone(), Value::Int(123)),
        ("1e2", int64, Value::Int(100)),
        (
            "0.5",
            ValueSpec::new(ScalarKind::Float { bits: 64 }),
            Value::Float(0.5),
        ),
        (
            "1.50",
            ValueSpec::new(ScalarKind::Decimal),
            Value::Decimal(DecimalValue::parse("1.50").unwrap()),
        ),
    ] {
        assert_eq!(read(text, LexClass::String, &kind), Ok(expected.clone()));
        assert_eq!(read(text, LexClass::Number, &kind), Ok(expected));
    }
    assert_eq!(
        read("123", LexClass::String, &ValueSpec::new(ScalarKind::Any)),
        Ok(Value::Str("123".to_owned()))
    );

    let spec = ValueSpec {
        discriminators: Some(table()),
        ..ValueSpec::new(ScalarKind::Any)
    };
    let tagged = |text: &str, class: LexClass, tag: Option<&str>| {
        coerce_discriminated(&Lexeme::new(text, class), tag, &spec).map_err(|e| e.reason)
    };
    assert_eq!(
        tagged("42", LexClass::String, Some("integer")),
        Ok(Value::Int(42))
    );
    assert_eq!(
        tagged("42", LexClass::Number, Some("text")),
        Err(FidelityReason::NotANumberForKind)
    );
    assert_eq!(
        tagged("abc", LexClass::String, Some("x-custom")),
        Ok(opaque("x-custom", "abc"))
    );
    assert_eq!(
        tagged("abc", LexClass::String, None),
        Ok(Value::Str("abc".to_owned()))
    );

    // The law on every tag of the table and one unknown tag, crossed with a
    // lexeme of each class, against `coerce` under the tag's kind.
    let lexemes = [
        ("42", LexClass::Number),
        ("42", LexClass::String),
        ("true", LexClass::Bool),
        ("null", LexClass::Null),
        ("2024-01-02", LexClass::DateTime),
        ("AQI=", LexClass::Bytes),
        ("7", LexClass::Tagged("int".into())),
    ];
    let mut checked = 0;
    for (text, class) in &lexemes {
        for (tag, kind) in table() {
            let under_kind = read(text, class.clone(), &ValueSpec::new(kind));
            assert_eq!(
                tagged(text, class.clone(), Some(&tag)),
                under_kind,
                "{tag} {text}"
            );
            checked += 1;
        }
        assert_eq!(
            tagged(text, class.clone(), Some("x-unknown")),
            Ok(opaque("x-unknown", text)),
            "{text}"
        );
        checked += 1;
        assert_eq!(
            tagged(text, class.clone(), None),
            read(text, class.clone(), &spec),
            "{text}"
        );
    }
    println!("discriminated kinds: {checked} tagged cases over 4 tags and 7 classes");
    assert_eq!(checked, lexemes.len() * (table().len() + 1));

    // Without a table the tag is not consulted.
    let plain = ValueSpec::new(ScalarKind::Bytes(BytesText::Base64));
    for (text, class) in &lexemes {
        assert_eq!(
            coerce_discriminated(&Lexeme::new(text, class.clone()), Some("integer"), &plain)
                .map_err(|e| e.reason),
            read(text, class.clone(), &plain),
            "{text}"
        );
    }
}
