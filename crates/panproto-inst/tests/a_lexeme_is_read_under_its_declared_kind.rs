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
        assert_eq!(time.fraction.as_str(), "999999");
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
        // A fraction of any number of digits is kept as written.
        for text in [
            "2026-09-28T12:00:00.1234567891",
            "2026-09-28T12:00:00.123456789012345678901234567890Z",
        ] {
            let read = string(text, &open).unwrap();
            assert_eq!(read, Value::Temporal(Temporal::parse(text).unwrap()));
            let Value::Temporal(t) = read else {
                panic!("a temporal")
            };
            assert_eq!(t.to_string(), text);
        }
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

/// The bits of a float result, or a panic naming what came instead.
fn float_bits(result: Result<Value, ValueFidelityError>) -> u64 {
    match result {
        Ok(Value::Float(f)) => f.to_bits(),
        other => panic!("expected a float, got {other:?}"),
    }
}

#[test]
fn a_negative_zero_lexeme_keeps_its_sign_under_every_float_kind() {
    let negative_zero = (-0.0_f64).to_bits();
    for bits in [32, 64] {
        let kind = spec(ScalarKind::Float { bits });
        for text in ["-0.0", "-0", "-0e0", "-0.000e-12"] {
            assert_eq!(
                float_bits(number(text, &kind)),
                negative_zero,
                "{text} under Float{{bits:{bits}}}"
            );
        }
        for text in ["0.0", "0", "+0e0"] {
            assert_eq!(float_bits(number(text, &kind)), 0, "{text}");
        }
        let site = ValueSite::new("v", None);
        let kept = panproto_inst::value::coerce::coerce_value(&Value::Float(-0.0), &kind, site);
        assert_eq!(
            float_bits(kept),
            negative_zero,
            "Float(-0.0) as {bits} bits"
        );
    }
    // An open schema reads the fractional and exponent forms as a float,
    // and the integer form as the integer zero, which has no sign.
    let any = spec(ScalarKind::Any);
    assert_eq!(float_bits(number("-0.0", &any)), negative_zero);
    assert_eq!(float_bits(number("-0e0", &any)), negative_zero);
    assert_eq!(number("-0", &any).unwrap(), Value::Int(0));
    // A decimal zero is never negative.
    let Value::Decimal(zero) = number("-0.0", &spec(ScalarKind::Decimal)).unwrap() else {
        panic!("a decimal")
    };
    assert!(!zero.negative);
}

#[test]
fn an_exact_binary_lexeme_is_accepted_under_its_float_kind() {
    let f32_ = spec(ScalarKind::Float { bits: 32 });
    let f64_ = spec(ScalarKind::Float { bits: 64 });
    // 2^53 and 2^50 are binary32 values, written with more than nine
    // significant digits.
    assert_eq!(
        number("9007199254740992", &f32_).unwrap(),
        Value::Float(9_007_199_254_740_992.0)
    );
    assert_eq!(
        number("1125899906842624", &f32_).unwrap(),
        Value::Float(1_125_899_906_842_624.0)
    );
    // The exact value of the binary32 nearest 0.1.
    assert_eq!(
        float_bits(number("0.100000001490116119384765625", &f32_)),
        f64::from(0.1_f32).to_bits()
    );
    // The exact value of the binary64 nearest 0.1, 55 significant digits.
    assert_eq!(
        float_bits(number(
            "0.1000000000000000055511151231257827021181583404541015625",
            &f64_
        )),
        0.1_f64.to_bits()
    );
    // The same digits with an exponent, and with trailing zeros.
    assert_eq!(
        float_bits(number(
            "1000000000000000055511151231257827021181583404541015625e-55",
            &f64_
        )),
        0.1_f64.to_bits()
    );
    assert_eq!(
        float_bits(number("0.1000000014901161193847656250000", &f32_)),
        f64::from(0.1_f32).to_bits()
    );
    // The least positive subnormal of each width, exactly.
    assert_eq!(
        float_bits(number(&format!("{:.1100e}", f64::from_bits(1)), &f64_)),
        1
    );
    assert_eq!(
        float_bits(number(
            &format!("{:.200e}", f64::from(f32::from_bits(1))),
            &f32_
        )),
        f64::from(f32::from_bits(1)).to_bits()
    );
    // A numeral that is neither exact nor the shortest text of its
    // rounding is still refused, and one that is the shortest text of its
    // rounding is still accepted.
    for text in ["16777217.0", "0.1000000000000001", "0.10000000149011612"] {
        assert_eq!(
            reason(number(text, &f32_)),
            FidelityReason::PrecisionExceeded { max: 9 },
            "{text}"
        );
    }
    assert_eq!(
        reason(number(
            "0.100000000000000005551115123125782702118158340454101562",
            &f64_
        )),
        FidelityReason::PrecisionExceeded { max: 17 }
    );
    assert_eq!(
        float_bits(number("0.1", &f32_)),
        f64::from(0.1_f32).to_bits()
    );
}

#[test]
fn every_nan_spelling_reads_as_the_canonical_nan() {
    const CANONICAL: u64 = 0x7ff8_0000_0000_0000;
    let kinds = [
        spec(ScalarKind::Float { bits: 32 }),
        spec(ScalarKind::Float { bits: 64 }),
        spec(ScalarKind::Any),
    ];
    for kind in &kinds {
        for text in [
            "NaN", "nan", "+nan", "-nan", "-NaN", ".nan", "+NaN", "-.NaN",
        ] {
            assert_eq!(
                float_bits(number(text, kind)),
                CANONICAL,
                "{text} under {:?}",
                kind.kind
            );
        }
        assert_eq!(
            float_bits(number("-inf", kind)),
            f64::NEG_INFINITY.to_bits()
        );
        assert_eq!(float_bits(number("+.inf", kind)), f64::INFINITY.to_bits());
    }
    // A NaN that is already a value keeps its sign and payload under a
    // float kind that holds it.
    let site = ValueSite::new("v", None);
    let coerce_value = panproto_inst::value::coerce::coerce_value;
    for bits in [0xfff8_0000_0000_0000_u64, 0x7ff8_0000_0000_1234] {
        let kept = coerce_value(&Value::Float(f64::from_bits(bits)), &kinds[1], site.clone());
        assert_eq!(float_bits(kept), bits);
    }
    let kept = coerce_value(
        &Value::Float(f64::from_bits(0xfff8_0000_0000_0000)),
        &kinds[0],
        site,
    );
    assert_eq!(float_bits(kept), 0xfff8_0000_0000_0000);
}

/// Every way of writing `s` with each letter in either case.
fn case_variants(s: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    for c in s.chars() {
        out = out
            .iter()
            .flat_map(|prefix| {
                let (lower, upper) = (c.to_ascii_lowercase(), c.to_ascii_uppercase());
                [format!("{prefix}{lower}"), format!("{prefix}{upper}")]
            })
            .collect();
    }
    out
}

/// Every NaN and infinity spelling in a bounded scope (each letter of
/// `nan`, `inf` and `infinity` in either case, with no sign, `+` or `-`,
/// and with or without YAML's leading `.`): a NaN spelling reads as the
/// canonical NaN and an infinity spelling as the infinity of its sign,
/// under both float kinds and `Any`; a spelling with another word, or a
/// sign or dot alone, is no number.
#[test]
fn every_non_finite_spelling_reads_by_its_word_on_a_bounded_scope() {
    const CANONICAL: u64 = 0x7ff8_0000_0000_0000;
    let kinds = [
        spec(ScalarKind::Float { bits: 32 }),
        spec(ScalarKind::Float { bits: 64 }),
        spec(ScalarKind::Any),
    ];
    let mut checked = 0;
    for (word, nan) in [("nan", true), ("inf", false), ("infinity", false)] {
        for spelled in case_variants(word) {
            for sign in ["", "+", "-"] {
                for dot in ["", "."] {
                    let text = format!("{sign}{dot}{spelled}");
                    let want = if nan {
                        CANONICAL
                    } else if sign == "-" {
                        f64::NEG_INFINITY.to_bits()
                    } else {
                        f64::INFINITY.to_bits()
                    };
                    for kind in &kinds {
                        assert_eq!(float_bits(number(&text, kind)), want, "{text}");
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, (8 + 8 + 256) * 3 * 2 * 3);
    for text in [
        "nana", "-", ".", "+.", "na", "infinit", "in", "-.-nan", "..nan", "nan.",
    ] {
        for kind in &kinds[..2] {
            assert_eq!(
                reason(number(text, kind)),
                FidelityReason::NotANumber,
                "{text}"
            );
        }
    }
}

/// The shortest text of a float breaks an exact tie away from zero, and a
/// float kind accepts exactly that candidate: the binary32 value
/// 52401.3125 lies midway between `52401.312` and `52401.313`.
#[test]
fn a_closest_shortest_tie_breaks_away_from_zero() {
    let f32_ = spec(ScalarKind::Float { bits: 32 });
    assert_eq!(52_401.312_5_f32.to_string(), "52401.313");
    assert_eq!(format!("{:e}", -52_401.312_5_f32), "-5.2401313e4");
    for sign in ["", "-"] {
        let tie = f64::from(52_401.312_5_f32);
        let want = if sign.is_empty() { tie } else { -tie };
        assert_eq!(
            float_bits(number(&format!("{sign}52401.313"), &f32_)),
            want.to_bits()
        );
        assert_eq!(
            reason(number(&format!("{sign}52401.312"), &f32_)),
            FidelityReason::PrecisionExceeded { max: 9 }
        );
        // The exact value is accepted too.
        assert_eq!(
            float_bits(number(&format!("{sign}52401.3125"), &f32_)),
            want.to_bits()
        );
    }
    // A binary64 tie: 2229016984128891.25 lies midway between two 17-digit
    // texts that both read back to it, and is written with the larger.
    let x = f64::from_bits(0x431f_ad1d_d626_35ed);
    assert_eq!(format!("{x:.2}"), "2229016984128891.25");
    assert_eq!(x.to_string(), "2229016984128891.3");
    let f64_ = spec(ScalarKind::Float { bits: 64 });
    assert_eq!(float_bits(number("2229016984128891.3", &f64_)), x.to_bits());
    assert_eq!(
        reason(number("2229016984128891.2", &f64_)),
        FidelityReason::PrecisionExceeded { max: 17 }
    );
    // `Display` writes a float positionally.
    let big = 1e300_f64.to_string();
    assert_eq!(big, format!("1{}", "0".repeat(300)));
}

#[test]
fn an_exponent_numeral_expanding_past_4096_zeros_overflows_under_bigint() {
    let big_int = spec(ScalarKind::BigInt);
    let digits = |v: Value| match v {
        Value::BigInt(b) => b.digits,
        other => panic!("expected a big integer, got {other:?}"),
    };
    let expanded = digits(number("1e4096", &big_int).unwrap());
    assert_eq!(expanded, format!("1{}", "0".repeat(4096)));
    assert_eq!(expanded.len(), 4097);
    assert_eq!(
        digits(number("10e4096", &big_int).unwrap()),
        format!("1{}", "0".repeat(4097))
    );
    assert_eq!(
        digits(number("0.1e4097", &big_int).unwrap()),
        format!("1{}", "0".repeat(4096))
    );
    assert_eq!(
        digits(number("-1.5e4096", &big_int).unwrap()),
        format!("15{}", "0".repeat(4095))
    );
    // The stored exponent, not the written one, is bounded: "10.0e4097"
    // stores the digits 100 with exponent 4096.
    assert_eq!(
        digits(number("10.0e4097", &big_int).unwrap()),
        format!("1{}", "0".repeat(4098))
    );
    for text in ["1e4097", "1e5000", "10e4097", "1e2147483647"] {
        assert_eq!(
            reason(number(text, &big_int)),
            FidelityReason::Overflow,
            "{text}"
        );
    }
    let written = format!("1{}", "0".repeat(5000));
    assert_eq!(digits(number(&written, &big_int).unwrap()), written);
    let int = spec(ScalarKind::Int {
        bits: 64,
        signed: true,
    });
    assert_eq!(reason(number("1e4097", &int)), FidelityReason::Overflow);
    assert_eq!(reason(number("1e4096", &int)), FidelityReason::Overflow);
    // A zero with a large exponent is zero.
    assert_eq!(number("0e5000", &big_int).unwrap(), Value::Int(0));
}
