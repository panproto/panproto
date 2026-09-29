//! A lexeme is read under the scalar kind its vertex declares, exactly or
//! not at all: a declared kind that cannot hold a lexeme exactly is a typed
//! error, never a float, and an open schema reads each lexeme into the most
//! exact value its form allows.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_gat::Name;
use panproto_inst::error::ParseError;
use panproto_inst::value::coerce::{FidelityLog, LexClass, Lexeme, coerce};
use panproto_inst::value::{BigIntValue, DecimalValue, ExactInteger, Offset, Temporal};
use panproto_inst::{FidelityReason, Value, ValueFidelityError, ValueSite};
use panproto_schema::{BytesText, ScalarKind, ValueSpec};

fn read(text: &str, class: LexClass, spec: &ValueSpec) -> Result<Value, ValueFidelityError> {
    coerce(&Lexeme::new(text, class), spec)
}

fn number(text: &str, spec: &ValueSpec) -> Result<Value, ValueFidelityError> {
    read(text, LexClass::Number, spec)
}

fn string(text: &str, spec: &ValueSpec) -> Result<Value, ValueFidelityError> {
    read(text, LexClass::String, spec)
}

fn reason(result: Result<Value, ValueFidelityError>) -> FidelityReason {
    match result {
        Err(e) => e.reason,
        Ok(v) => panic!("expected an error, got {v:?}"),
    }
}

const fn spec(kind: ScalarKind) -> ValueSpec {
    ValueSpec::new(kind)
}

fn dec(text: &str) -> Value {
    Value::Decimal(DecimalValue::parse(text).unwrap())
}

fn big(text: &str) -> Value {
    match BigIntValue::parse(text).unwrap() {
        ExactInteger::Big(b) => Value::BigInt(b),
        ExactInteger::Int(n) => panic!("{n} fits"),
    }
}

#[test]
fn any_coercion_follows_the_exactness_rule() {
    let any = spec(ScalarKind::Any);
    // Integers: Int when they fit i64, else BigInt.
    assert_eq!(number("42", &any).unwrap(), Value::Int(42));
    assert_eq!(
        number("-9223372036854775808", &any).unwrap(),
        Value::Int(i64::MIN)
    );
    assert_eq!(
        number("18446744073709551615", &any).unwrap(),
        big("18446744073709551615")
    );
    assert_eq!(
        number("100000000000000000000000000000", &any).unwrap(),
        big("100000000000000000000000000000")
    );
    // Fractions and exponents: Float exactly when the float reads back as
    // the lexeme's number.
    assert_eq!(number("0.1", &any).unwrap(), Value::Float(0.1));
    assert_eq!(number("1.5e3", &any).unwrap(), Value::Float(1500.0));
    assert_eq!(number("2.50", &any).unwrap(), Value::Float(2.5));
    assert_eq!(number("1e400", &any).unwrap(), dec("1e400"));
    assert_eq!(number("1e-400", &any).unwrap(), dec("1e-400"));
    assert_eq!(
        number("12345678901234567890.123456789", &any).unwrap(),
        dec("12345678901234567890.123456789")
    );
    assert_eq!(
        number("0.1000000000000000055511151231257827", &any).unwrap(),
        dec("0.1000000000000000055511151231257827")
    );
    // Non-finite spellings are floats.
    assert!(matches!(number("NaN", &any).unwrap(), Value::Float(f) if f.is_nan()));
    assert_eq!(
        number("-.inf", &any).unwrap(),
        Value::Float(f64::NEG_INFINITY)
    );
    // Strings stay strings: no guessing of dates, numbers or base64.
    for text in ["2026-09-28", "12", "AAECAw==", "true"] {
        assert_eq!(string(text, &any).unwrap(), Value::Str(text.into()));
    }
    // The other classes.
    assert_eq!(
        read("true", LexClass::Bool, &any).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(read("null", LexClass::Null, &any).unwrap(), Value::Null);
    assert_eq!(
        read("2026-09-28T12:34:56.789+05:30", LexClass::DateTime, &any).unwrap(),
        Value::Temporal(Temporal::parse("2026-09-28T12:34:56.789+05:30").unwrap())
    );
    assert_eq!(
        read("AAEC/w==", LexClass::Bytes, &any).unwrap(),
        Value::Bytes(vec![0, 1, 2, 0xff])
    );
    assert_eq!(reason(number("1.2.3", &any)), FidelityReason::NotANumber);
}

#[test]
fn tagged_lexemes_read_under_their_tag() {
    let any = spec(ScalarKind::Any);
    let tagged = |text: &str, tag: &str| read(text, LexClass::Tagged(Name::from(tag)), &any);
    assert_eq!(
        tagged("AAEC/w==", "!!binary").unwrap(),
        Value::Bytes(vec![0, 1, 2, 0xff])
    );
    assert_eq!(
        tagged(
            "2001-12-14T21:59:43.10-05:00",
            "tag:yaml.org,2002:timestamp"
        )
        .unwrap(),
        Value::Temporal(Temporal::parse("2001-12-14T21:59:43.10-05:00").unwrap())
    );
    assert_eq!(tagged("1.50", "numberDecimal").unwrap(), dec("1.50"));
    assert_eq!(tagged("~", "!!null").unwrap(), Value::Null);
    assert_eq!(tagged("12", "!!str").unwrap(), Value::Str("12".into()));
    // A tag naming no scalar kind is kept, never dropped.
    let Value::Opaque { type_, fields } = tagged("x", "!custom").unwrap() else {
        panic!("an opaque value")
    };
    assert_eq!(type_, "!custom");
    assert_eq!(fields.get("value"), Some(&Value::Str("x".into())));
    // A declared kind governs a tagged lexeme.
    assert_eq!(
        read(
            "7",
            LexClass::Tagged(Name::from("!!str")),
            &spec(ScalarKind::BigInt)
        )
        .unwrap(),
        Value::Int(7)
    );
}

/// One test per declared scalar kind: what it holds exactly, and the
/// typed error for what it cannot.
mod coerce_rejects_a_lexeme_its_kind_cannot_hold {
    use super::*;

    #[test]
    fn bool() {
        let s = spec(ScalarKind::Bool);
        assert_eq!(read("true", LexClass::Bool, &s).unwrap(), Value::Bool(true));
        assert_eq!(string("FALSE", &s).unwrap(), Value::Bool(false));
        assert_eq!(reason(string("yes", &s)), FidelityReason::InvalidEncoding);
        assert_eq!(reason(number("1", &s)), FidelityReason::NotANumberForKind);
    }

    #[test]
    fn int() {
        let i32_ = spec(ScalarKind::Int {
            bits: 32,
            signed: true,
        });
        assert_eq!(
            number("2147483647", &i32_).unwrap(),
            Value::Int(2_147_483_647)
        );
        assert_eq!(
            number("-2147483648", &i32_).unwrap(),
            Value::Int(-2_147_483_648)
        );
        assert_eq!(string("12", &i32_).unwrap(), Value::Int(12));
        assert_eq!(number("1.0e2", &i32_).unwrap(), Value::Int(100));
        assert_eq!(
            reason(number("2147483648", &i32_)),
            FidelityReason::Overflow
        );
        assert_eq!(
            reason(number("1.5", &i32_)),
            FidelityReason::ScaleExceeded { max: 0 }
        );
        assert_eq!(reason(number("NaN", &i32_)), FidelityReason::NonFinite);
        assert_eq!(reason(number("12abc", &i32_)), FidelityReason::NotANumber);
        assert_eq!(
            reason(read("true", LexClass::Bool, &i32_)),
            FidelityReason::NotANumber
        );
        // An unsigned 64-bit integer holds u64::MAX, as a BigInt.
        let u64_ = spec(ScalarKind::Int {
            bits: 64,
            signed: false,
        });
        assert_eq!(
            number("18446744073709551615", &u64_).unwrap(),
            big("18446744073709551615")
        );
        assert_eq!(
            reason(number("18446744073709551616", &u64_)),
            FidelityReason::Overflow
        );
        assert_eq!(reason(number("-1", &u64_)), FidelityReason::Overflow);
        // The vertex's parameters narrow the kind.
        let narrowed = ValueSpec {
            int_bits: Some(8),
            unsigned: true,
            ..spec(ScalarKind::Int {
                bits: 64,
                signed: true,
            })
        };
        assert_eq!(number("255", &narrowed).unwrap(), Value::Int(255));
        assert_eq!(reason(number("256", &narrowed)), FidelityReason::Overflow);
        assert_eq!(reason(number("-1", &narrowed)), FidelityReason::Overflow);
        let i128_ = spec(ScalarKind::Int {
            bits: 128,
            signed: true,
        });
        assert_eq!(
            number("-170141183460469231731687303715884105728", &i128_).unwrap(),
            big("-170141183460469231731687303715884105728")
        );
        assert_eq!(
            reason(number("170141183460469231731687303715884105728", &i128_)),
            FidelityReason::Overflow
        );
    }

    #[test]
    fn big_int() {
        let s = spec(ScalarKind::BigInt);
        assert_eq!(number("5", &s).unwrap(), Value::Int(5));
        assert_eq!(
            number("100000000000000000000000000000", &s).unwrap(),
            big("100000000000000000000000000000")
        );
        assert_eq!(
            number("1e30", &s).unwrap(),
            big(&format!("1{}", "0".repeat(30)))
        );
        assert_eq!(
            reason(number("0.5", &s)),
            FidelityReason::ScaleExceeded { max: 0 }
        );
        assert_eq!(reason(number("1e999999999", &s)), FidelityReason::Overflow);
    }

    #[test]
    fn float() {
        let f64_ = spec(ScalarKind::Float { bits: 64 });
        assert_eq!(number("0.1", &f64_).unwrap(), Value::Float(0.1));
        assert!(matches!(number("nan", &f64_).unwrap(), Value::Float(f) if f.is_nan()));
        assert_eq!(
            number("-inf", &f64_).unwrap(),
            Value::Float(f64::NEG_INFINITY)
        );
        assert_eq!(reason(number("1e400", &f64_)), FidelityReason::Overflow);
        assert_eq!(
            reason(number("0.1000000000000000055511151231257827", &f64_)),
            FidelityReason::PrecisionExceeded { max: 17 }
        );
        assert_eq!(
            reason(number("12345678901234567890.123456789", &f64_)),
            FidelityReason::PrecisionExceeded { max: 17 }
        );
        let f32_ = spec(ScalarKind::Float { bits: 32 });
        assert_eq!(number("0.5", &f32_).unwrap(), Value::Float(0.5));
        assert_eq!(
            number("0.1", &f32_).unwrap(),
            Value::Float(f64::from(0.1_f32))
        );
        assert_eq!(
            reason(number("0.123456789012", &f32_)),
            FidelityReason::PrecisionExceeded { max: 9 }
        );
        assert_eq!(reason(number("1e39", &f32_)), FidelityReason::Overflow);
    }

    #[test]
    fn decimal() {
        let numeric = ValueSpec {
            precision: Some(38),
            scale: Some(10),
            ..spec(ScalarKind::Decimal)
        };
        assert_eq!(
            number("1234567890123456789012345678.0123456789", &numeric).unwrap(),
            dec("1234567890123456789012345678.0123456789")
        );
        // Trailing zeros past the scale lose nothing; the written form is kept.
        assert_eq!(
            number("1.50000000000", &numeric).unwrap(),
            dec("1.50000000000")
        );
        assert_eq!(
            reason(number("0.01234567891", &numeric)),
            FidelityReason::ScaleExceeded { max: 10 }
        );
        assert_eq!(
            reason(number("12345678901234567890123456789", &numeric)),
            FidelityReason::PrecisionExceeded { max: 38 }
        );
        let total = ValueSpec {
            precision: Some(3),
            ..spec(ScalarKind::Decimal)
        };
        assert_eq!(number("1.23", &total).unwrap(), dec("1.23"));
        assert_eq!(
            reason(number("1.234", &total)),
            FidelityReason::PrecisionExceeded { max: 3 }
        );
        let any_decimal = spec(ScalarKind::Decimal);
        assert_eq!(number("7", &any_decimal).unwrap(), dec("7"));
        assert_eq!(string("0.10", &any_decimal).unwrap(), dec("0.10"));
        assert_eq!(
            reason(number("Infinity", &any_decimal)),
            FidelityReason::NonFinite
        );
        assert_eq!(
            reason(number("x", &any_decimal)),
            FidelityReason::NotANumber
        );
    }

    #[test]
    fn str() {
        let s = spec(ScalarKind::Str);
        assert_eq!(string("12", &s).unwrap(), Value::Str("12".into()));
        assert_eq!(
            read("2026-09-28", LexClass::DateTime, &s).unwrap(),
            Value::Str("2026-09-28".into())
        );
        assert_eq!(reason(number("12", &s)), FidelityReason::NotANumberForKind);
        assert_eq!(
            reason(read("true", LexClass::Bool, &s)),
            FidelityReason::InvalidEncoding
        );
    }

    #[test]
    fn bytes() {
        let b64 = spec(ScalarKind::Bytes(BytesText::Base64));
        let expected = Value::Bytes(vec![0x00, 0x01, 0x02, 0xff]);
        assert_eq!(string("AAEC/w==", &b64).unwrap(), expected);
        assert_eq!(string("AAEC/w", &b64).unwrap(), expected);
        assert_eq!(string("", &b64).unwrap(), Value::Bytes(Vec::new()));
        for bad in ["AAEC_w==", "AAEC/x==", "A", "AAEC/w=", "AA=C"] {
            assert_eq!(
                reason(string(bad, &b64)),
                FidelityReason::InvalidEncoding,
                "{bad}"
            );
        }
        let url = spec(ScalarKind::Bytes(BytesText::Base64Url));
        assert_eq!(string("AAEC_w", &url).unwrap(), expected);
        let hex = spec(ScalarKind::Bytes(BytesText::Hex));
        assert_eq!(string("000102fF", &hex).unwrap(), expected);
        assert_eq!(reason(string("0g", &hex)), FidelityReason::InvalidEncoding);
        assert_eq!(reason(string("012", &hex)), FidelityReason::InvalidEncoding);
        let raw = spec(ScalarKind::Bytes(BytesText::Raw));
        assert_eq!(string("hi", &raw).unwrap(), Value::Bytes(b"hi".to_vec()));
        assert_eq!(reason(number("1", &hex)), FidelityReason::NotANumberForKind);
    }

    #[test]
    fn date() {
        let s = spec(ScalarKind::Date);
        assert_eq!(
            string("2026-09-28", &s).unwrap(),
            Value::Temporal(Temporal::parse("2026-09-28").unwrap())
        );
        assert_eq!(
            reason(string("2026-09-28T00:00:00", &s)),
            FidelityReason::InvalidTemporal
        );
        assert_eq!(
            reason(string("2026-02-30", &s)),
            FidelityReason::InvalidTemporal
        );
        assert_eq!(
            reason(number("20260928", &s)),
            FidelityReason::NotANumberForKind
        );
    }

    #[test]
    fn time() {
        let zoned = ValueSpec {
            with_time_zone: Some(true),
            ..spec(ScalarKind::Time)
        };
        assert!(string("12:00:00+01:00", &zoned).is_ok());
        assert_eq!(
            reason(string("12:00:00", &zoned)),
            FidelityReason::OffsetRequired
        );
        assert_eq!(
            reason(string("25:00:00Z", &zoned)),
            FidelityReason::InvalidTemporal
        );
    }

    #[test]
    fn date_time() {
        let local = ValueSpec {
            with_time_zone: Some(false),
            ..spec(ScalarKind::DateTime)
        };
        let Value::Temporal(Temporal::DateTime(_, time, Offset::Local)) =
            read("1979-05-27T07:32:00.999999", LexClass::DateTime, &local).unwrap()
        else {
            panic!("a local date-time")
        };
        assert_eq!((time.nanos, time.frac_digits), (999_999_000, 6));
        assert_eq!(
            reason(string("1979-05-27T07:32:00Z", &local)),
            FidelityReason::OffsetForbidden
        );
        assert_eq!(
            reason(string("1979-05-27T07:32:00-00:00", &local)),
            FidelityReason::OffsetForbidden
        );
        let open = spec(ScalarKind::DateTime);
        assert_eq!(
            string("2026-09-28T12:34:56.789+05:30", &open).unwrap(),
            Value::Temporal(Temporal::parse("2026-09-28T12:34:56.789+05:30").unwrap())
        );
        assert_eq!(
            reason(string("2026-09-28", &open)),
            FidelityReason::InvalidTemporal
        );
        assert_eq!(
            reason(string("2026-09-28T12:00:00.1234567891", &open)),
            FidelityReason::PrecisionExceeded { max: 9 }
        );
    }

    #[test]
    fn token() {
        let s = spec(ScalarKind::Token);
        assert_eq!(string("RED", &s).unwrap(), Value::Token("RED".into()));
        assert_eq!(reason(number("1", &s)), FidelityReason::NotANumberForKind);
    }

    #[test]
    fn any() {
        let s = spec(ScalarKind::Any);
        assert_eq!(reason(number("--1", &s)), FidelityReason::NotANumber);
        assert_eq!(
            reason(read("2026-13-01", LexClass::DateTime, &s)),
            FidelityReason::InvalidTemporal
        );
        assert_eq!(
            reason(read("@@", LexClass::Bytes, &s)),
            FidelityReason::InvalidEncoding
        );
    }

    #[test]
    fn a_null_is_null_under_every_kind() {
        for kind in [
            ScalarKind::Bool,
            ScalarKind::Int {
                bits: 8,
                signed: false,
            },
            ScalarKind::Decimal,
            ScalarKind::Date,
            ScalarKind::Bytes(BytesText::Hex),
        ] {
            assert_eq!(
                read("NULL", LexClass::Null, &spec(kind)).unwrap(),
                Value::Null
            );
        }
    }

    #[test]
    fn the_error_names_the_lexeme_and_the_kind() {
        let kind = ScalarKind::Int {
            bits: 8,
            signed: true,
        };
        let error = number("300", &spec(kind.clone())).unwrap_err();
        assert_eq!(error.lexeme, "300");
        assert_eq!(error.target, kind);
        assert_eq!(error.site, ValueSite::unknown());
    }
}

#[test]
fn parse_reports_every_fidelity_error_in_document_order() {
    let int8 = spec(ScalarKind::Int {
        bits: 8,
        signed: true,
    });
    let money = ValueSpec {
        scale: Some(2),
        ..spec(ScalarKind::Decimal)
    };
    let when = spec(ScalarKind::Date);
    // A document of five scalars, three of which cannot be held.
    let document = [
        ("a", 0, "12", LexClass::Number, &int8),
        ("b", 10, "300", LexClass::Number, &int8),
        ("c", 20, "1.005", LexClass::Number, &money),
        ("d", 30, "1.10", LexClass::Number, &money),
        ("e", 40, "2026-02-30", LexClass::String, &when),
    ];
    let mut log = FidelityLog::new();
    let mut read = Vec::new();
    for (vertex, offset, text, class, spec) in document {
        let site = ValueSite::new(vertex, Some(offset));
        read.push(log.coerce(site, &Lexeme::new(text, class), spec));
    }
    assert_eq!(
        read,
        vec![Some(Value::Int(12)), None, None, Some(dec("1.10")), None]
    );
    let Err(ParseError::Fidelity(errors)) = log.finish() else {
        panic!("a fidelity error")
    };
    let summary: Vec<(&str, Option<u64>, &str, FidelityReason)> = errors
        .iter()
        .map(|e| {
            (
                e.site.vertex.as_ref(),
                e.site.byte_offset,
                e.lexeme.as_str(),
                e.reason.clone(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            ("b", Some(10), "300", FidelityReason::Overflow),
            (
                "c",
                Some(20),
                "1.005",
                FidelityReason::ScaleExceeded { max: 2 }
            ),
            ("e", Some(40), "2026-02-30", FidelityReason::InvalidTemporal),
        ]
    );
    let message = ParseError::Fidelity(errors).to_string();
    assert!(
        message.starts_with("3 value(s) cannot be read exactly; the first: \"300\""),
        "{message}"
    );
    // A document with nothing to report finishes cleanly.
    let mut clean = FidelityLog::new();
    assert_eq!(
        clean.coerce(
            ValueSite::new("a", None),
            &Lexeme::new("1", LexClass::Number),
            &int8
        ),
        Some(Value::Int(1))
    );
    assert!(clean.errors().is_empty());
    assert!(clean.finish().is_ok());
}
