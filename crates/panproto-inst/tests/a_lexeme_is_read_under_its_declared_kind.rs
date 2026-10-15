//! A lexeme is read under the scalar kind its vertex declares, exactly or
//! not at all: a declared kind that cannot hold a lexeme exactly is a typed
//! error, never a float, and an open schema reads each lexeme into the most
//! exact value its form allows.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_gat::Name;
use panproto_inst::error::ParseError;
use panproto_inst::value::coerce::{FidelityLog, LexClass, Lexeme, coerce};
use panproto_inst::value::{
    BigIntValue, DecimalClass, DecimalValue, ExactInteger, Offset, Temporal, WideInt,
};
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
        // An exponent of any size is read, without writing its zeros.
        let Value::BigInt(power) = number("1e999999999", &s).unwrap() else {
            panic!("a big integer")
        };
        assert_eq!(power.exponent_form(), "1e999999999");
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
        // The special values of the General Decimal Arithmetic are decimals.
        assert_eq!(number("Infinity", &any_decimal).unwrap(), dec("Infinity"));
        assert_eq!(number("sNaN4", &any_decimal).unwrap(), dec("sNaN4"));
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
        let Value::Temporal(Temporal::DateTime(_, time, Offset::Local, _)) =
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
    // and the integer form as the integer zero, which has no sign: the
    // written sign of `-0` is dropped there, a recorded loss kept until it
    // is settled how an open schema carries it.
    let any = spec(ScalarKind::Any);
    assert_eq!(float_bits(number("-0.0", &any)), negative_zero);
    assert_eq!(float_bits(number("-0e0", &any)), negative_zero);
    assert_eq!(number("-0", &any).unwrap(), Value::Int(0));
    // A decimal zero keeps its written sign.
    let Value::Decimal(zero) = number("-0.0", &spec(ScalarKind::Decimal)).unwrap() else {
        panic!("a decimal")
    };
    assert!(zero.negative && zero.is_zero());
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
fn every_nan_spelling_reads_as_its_signed_canonical_nan() {
    const CANONICAL: u64 = 0x7ff8_0000_0000_0000;
    const NEGATIVE: u64 = 0xfff8_0000_0000_0000;
    let kinds = [
        spec(ScalarKind::Float { bits: 32 }),
        spec(ScalarKind::Float { bits: 64 }),
        spec(ScalarKind::Any),
    ];
    for kind in &kinds {
        for (text, bits) in [
            ("NaN", CANONICAL),
            ("nan", CANONICAL),
            ("+nan", CANONICAL),
            (".nan", CANONICAL),
            ("+NaN", CANONICAL),
            ("-nan", NEGATIVE),
            ("-NaN", NEGATIVE),
            ("-.NaN", NEGATIVE),
        ] {
            assert_eq!(
                float_bits(number(text, kind)),
                bits,
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
                    let want = if nan && sign == "-" {
                        0xfff8_0000_0000_0000
                    } else if nan {
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

/// The shortest text of a float breaks an exact tie to even, and a float
/// kind accepts exactly that candidate: the binary32 value 52401.3125 lies
/// midway between `52401.312` and `52401.313`.
#[test]
fn a_closest_shortest_tie_breaks_to_even() {
    let f32_ = spec(ScalarKind::Float { bits: 32 });
    // 52401.3125 = 838421 / 16, a binary32 value.
    let tie32 = 838_421.0_f32 / 16.0;
    assert_eq!(format!("{tie32:.4}"), "52401.3125");
    for (sign, negative) in [("", false), ("-", true)] {
        let tie = f64::from(tie32);
        let want = if negative { -tie } else { tie };
        assert_eq!(
            float_bits(number(&format!("{sign}52401.312"), &f32_)),
            want.to_bits()
        );
        assert_eq!(
            reason(number(&format!("{sign}52401.313"), &f32_)),
            FidelityReason::PrecisionExceeded { max: 9 }
        );
        // The exact value is accepted too.
        assert_eq!(
            float_bits(number(&format!("{sign}52401.3125"), &f32_)),
            want.to_bits()
        );
    }
    // A binary64 tie: 2229016984128891.25 lies midway between two 17-digit
    // texts that both read back to it, and is written with the even one.
    let x = f64::from_bits(0x431f_ad1d_d626_35ed);
    assert_eq!(format!("{x:.2}"), "2229016984128891.25");
    let f64_ = spec(ScalarKind::Float { bits: 64 });
    assert_eq!(float_bits(number("2229016984128891.2", &f64_)), x.to_bits());
    assert_eq!(
        reason(number("2229016984128891.3", &f64_)),
        FidelityReason::PrecisionExceeded { max: 17 }
    );
}

/// Under `BigInt` every numeral whose value is an integer is read, whatever
/// its exponent, and a lexeme reads as the same value as its integer
/// written out; under a fixed-width `Int` the range is checked before any
/// digit is expanded.
#[test]
fn a_bigint_reads_the_same_whatever_its_spelling() {
    let big_int = spec(ScalarKind::BigInt);
    let read = |text: &str| match number(text, &big_int) {
        Ok(Value::BigInt(b)) => b,
        other => panic!("{text}: expected a big integer, got {other:?}"),
    };
    for text in [
        "1e4097",
        "1e5000",
        "1e9223372036854775807",
        "1e100000000000000000000",
    ] {
        let value = read(text);
        assert_eq!(value.exponent_form(), text);
        assert!(value.is_well_formed());
    }
    let written = format!("1{}", "0".repeat(4097));
    assert_eq!(written.len(), 4098);
    assert_eq!(read("1e4097"), read(&written));
    assert_eq!(
        read("1e4097").canonical_bytes(),
        read(&written).canonical_bytes()
    );
    // The 22 bytes of 10^4097: the sign byte, the significand "1" behind its
    // little-endian u64 length, the count "4097" behind its length.
    assert_eq!(
        read("1e4097").canonical_bytes(),
        [
            0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0x31, 0x04, 0, 0, 0, 0, 0, 0, 0, 0x34, 0x30, 0x39,
            0x37
        ]
    );
    // Other spellings of one integer read alike.
    for (a, b) in [
        ("1e4096", "10e4095"),
        ("0.1e4097", "1e4096"),
        ("-1.5e4096", "-15e4095"),
        ("10.0e4097", "1e4098"),
        ("1e5000", &format!("1{}", "0".repeat(5000))),
    ] {
        assert_eq!(read(a), read(b), "{a} {b}");
    }
    let int = spec(ScalarKind::Int {
        bits: 64,
        signed: true,
    });
    for text in ["1e4097", "1e2147483647", "1e100000000000000000000"] {
        assert_eq!(
            reason(number(text, &int)),
            FidelityReason::Overflow,
            "{text}"
        );
    }
    assert_eq!(
        number("1e18", &int).unwrap(),
        Value::Int(1_000_000_000_000_000_000)
    );
    // A zero with a large exponent is zero.
    assert_eq!(number("0e5000", &big_int).unwrap(), Value::Int(0));
}

/// A zero numeral whose stored exponent leaves `i32` is the zero that
/// "0e0" (or "-0e0") is under every kind whose values carry no scale, and
/// under `Decimal` the zero of its written scale.
#[test]
fn a_zero_numeral_beyond_i32_is_zero_where_its_kind_keeps_no_scale() {
    let unscaled = [
        spec(ScalarKind::BigInt),
        spec(ScalarKind::Int {
            bits: 64,
            signed: true,
        }),
        spec(ScalarKind::Float { bits: 32 }),
        spec(ScalarKind::Float { bits: 64 }),
        spec(ScalarKind::Any),
    ];
    let decimals = [
        spec(ScalarKind::Decimal),
        ValueSpec {
            precision: Some(3),
            scale: Some(2),
            ..spec(ScalarKind::Decimal)
        },
    ];
    for (text, zero, exponent) in [
        ("0e3000000000", "0e0", "3000000000"),
        ("-0e3000000000", "-0e0", "3000000000"),
        ("0.0e-3000000000", "0e0", "-3000000001"),
    ] {
        for kind in &unscaled {
            let got = number(text, kind).unwrap();
            let want = number(zero, kind).unwrap();
            match (&got, &want) {
                (Value::Float(a), Value::Float(b)) => {
                    assert_eq!(a.to_bits(), b.to_bits(), "{text}");
                }
                _ => assert_eq!(got, want, "{text} under {:?}", kind.kind),
            }
        }
        for kind in &decimals {
            let Value::Decimal(d) = number(text, kind).unwrap() else {
                panic!("{text}: a decimal")
            };
            assert!(d.is_zero() && d.negative == text.starts_with('-'), "{text}");
            assert_eq!(d.exponent.to_string(), exponent, "{text}");
        }
    }
    // A non-zero numeral beyond i32 is read by its kind: an integer kind
    // takes its integer or refuses its range, a float kind rounds it to an
    // infinity (refused) or to a zero that is not the numeral (refused), and
    // a malformed one is no number.
    let f32_ = spec(ScalarKind::Float { bits: 32 });
    let f64_ = spec(ScalarKind::Float { bits: 64 });
    for kind in [&f32_, &f64_] {
        assert_eq!(
            reason(number("1e3000000000", kind)),
            FidelityReason::Overflow
        );
        assert!(matches!(
            reason(number("0.1e-3000000000", kind)),
            FidelityReason::PrecisionExceeded { .. }
        ));
    }
    assert_eq!(
        reason(number("1e3000000000", &unscaled[1])),
        FidelityReason::Overflow
    );
    assert_eq!(
        reason(number("0.1e-3000000000", &unscaled[0])),
        FidelityReason::ScaleExceeded { max: 0 }
    );
    for kind in &unscaled[..4] {
        assert_eq!(reason(number("1e", kind)), FidelityReason::NotANumber);
        assert_eq!(reason(number("0.0.0e1", kind)), FidelityReason::NotANumber);
    }
}

/// Under `Decimal` a numeral whose stored exponent leaves `i32` keeps its
/// written scale, and under `Any` such a numeral that no finite float
/// writes is a `Decimal`.
#[test]
fn a_decimal_beyond_i32_keeps_its_scale() {
    let decimal = spec(ScalarKind::Decimal);
    let any = spec(ScalarKind::Any);
    let read = |text: &str, kind: &ValueSpec| match number(text, kind) {
        Ok(Value::Decimal(d)) => d,
        other => panic!("{text}: expected a decimal, got {other:?}"),
    };
    for (text, negative, digits, exponent) in [
        ("0.0e-3000000000", false, "0", "-3000000001"),
        ("1e3000000000", false, "1", "3000000000"),
        ("-1.5e-2147483648", true, "15", "-2147483649"),
        ("12e2147483647", false, "12", "2147483647"),
        (
            "1e100000000000000000000",
            false,
            "1",
            "100000000000000000000",
        ),
    ] {
        let d = read(text, &decimal);
        assert_eq!(
            (d.negative, d.digits.as_str(), d.exponent.to_string()),
            (negative, digits, exponent.to_owned()),
            "{text}"
        );
        assert_eq!(
            d.to_string(),
            DecimalValue::parse(text).unwrap().to_string()
        );
    }
    assert_eq!(
        read("0.0e-3000000000", &decimal).scale().to_string(),
        "3000000001"
    );
    for text in ["1e3000000000", "-1e3000000000", "1e-3000000000"] {
        let d = read(text, &any);
        assert_eq!(d, DecimalValue::parse(text).unwrap(), "{text}");
    }
    // The facets still judge the value: a precision of 3 holds 1.5e-2147483648
    // as a fraction of 2147483649 digits only if the scale allows it.
    let money = ValueSpec {
        precision: Some(3),
        scale: Some(2),
        ..spec(ScalarKind::Decimal)
    };
    assert_eq!(
        reason(number("1.5e-2147483648", &money)),
        FidelityReason::ScaleExceeded { max: 2 }
    );
    assert_eq!(
        reason(number("1e3000000000", &money)),
        FidelityReason::PrecisionExceeded { max: 3 }
    );
    assert_eq!(
        number("150e-2", &money).unwrap(),
        Value::Decimal(DecimalValue::parse("1.50").unwrap())
    );
}

/// The closest-shortest text of a binary32 value, from its definition, for
/// x = k / 256 with k in 2^23..2^24 (the binade [2^15, 2^16)): among the
/// decimals with the fewest significant digits that read back to x at
/// binary32 under round-to-nearest-even, the closest to x, at a tie the one
/// whose significand is even. Returns its significand digits, its number of
/// significant digits and whether there was a tie. Exact integer arithmetic
/// in units of 1 / (1024 * 10^9).
fn closest_shortest_reference(k: u64) -> (u64, u32, bool) {
    // x in units u = 1 / (1024 * 10^9).
    let x = u128::from(k) * 4 * 1_000_000_000;
    // Half the gap above is 1/512 = 2 * 10^9 u; below, the same, except at
    // x = 2^15, where the gap below is half as wide.
    let half_up = 2_000_000_000_u128;
    let half_down = if k == 1 << 23 { half_up / 2 } else { half_up };
    let even = k % 2 == 0;
    let reads_back = |v: u128| {
        if v >= x {
            v - x < half_up || (v - x == half_up && even)
        } else {
            x - v < half_down || (x - v == half_down && even)
        }
    };
    for n in 1..=9_u32 {
        // Candidates with n significant digits: multiples of 10^(5 - n),
        // that is of `step` units (x has five integer digits).
        let step = 1024 * 10_u128.pow(9 + 5 - n);
        let below = x / step * step;
        let above = below + step;
        let candidates: Vec<u128> = [below, above]
            .into_iter()
            .filter(|&v| reads_back(v))
            .collect();
        match candidates.as_slice() {
            [] => {}
            [v] => return (u64::try_from(v / step).unwrap(), n, false),
            [a, b] => {
                let (da, db) = (x - a, b - x);
                // The closer, and at a tie the even significand.
                let pick = if da < db || (da == db && (a / step) % 2 == 0) {
                    a
                } else {
                    b
                };
                return (u64::try_from(pick / step).unwrap(), n, da == db);
            }
            _ => unreachable!(),
        }
    }
    unreachable!("nine digits always suffice")
}

/// The digits of `d` without trailing zeros, and the decimal exponent of
/// its first digit.
fn leading(d: &DecimalValue) -> (String, i64) {
    let digits = d.digits.trim_end_matches('0').to_owned();
    let exponent =
        d.exponent.to_string().parse::<i64>().unwrap() + i64::try_from(d.digits.len()).unwrap() - 1;
    (digits, exponent)
}

/// The production printer `DecimalValue::from_f32_shortest` of every
/// binary32 in the binade [2^15, 2^16) is the closest-shortest text of the
/// definition (ties to even), and a 32-bit float kind accepts that text with
/// the value; at every tie it refuses the other candidate. Prints the bound
/// and the ties found.
#[test]
fn the_closest_shortest_text_agrees_with_its_definition_on_a_binade() {
    let f32_ = spec(ScalarKind::Float { bits: 32 });
    let mut ties = Vec::new();
    for k in (1_u32 << 23)..(1 << 24) {
        let x = f32::from_bits(0x4700_0000 | (k - (1 << 23)));
        assert_eq!((f64::from(x) * 256.0).to_bits(), f64::from(k).to_bits());
        let (digits, n, tie) = closest_shortest_reference(u64::from(k));
        let printed = DecimalValue::from_f32_shortest(x).unwrap();
        let (got, exponent) = leading(&printed);
        assert_eq!(exponent, 4, "{x}: {printed}");
        let want = digits.to_string();
        assert_eq!(got, want.trim_end_matches('0'), "{x}: {printed}");
        assert_eq!(u32::try_from(want.len()).unwrap(), n);
        if tie {
            ties.push(x);
            let mantissa = |d: u64| {
                let s = d.to_string();
                format!("{}.{}e4", &s[..1], &s[1..])
            };
            let accepted = number(&mantissa(digits), &f32_).unwrap();
            assert!(matches!(accepted, Value::Float(y) if y.to_bits() == f64::from(x).to_bits()));
            assert_eq!(digits % 2, 0, "the tie goes to the even candidate");
            // The other candidate lies on the other side of x.
            let step = 1024 * 10_u128.pow(14 - n);
            let x_units = u128::from(k) * 4 * 1_000_000_000;
            let other = if u128::from(digits) * step < x_units {
                digits + 1
            } else {
                digits - 1
            };
            assert_eq!(
                reason(number(&mantissa(other), &f32_)),
                FidelityReason::PrecisionExceeded { max: 9 },
                "{}",
                mantissa(other)
            );
        }
    }
    println!(
        "bound: {} binary32 values in [2^15, 2^16); {} ties, the first {:?}",
        1_u32 << 23,
        ties.len(),
        &ties[..ties.len().min(4)]
    );
    assert!(ties.contains(&(838_421.0_f32 / 16.0)));
}

/// The decimal a test expects under `Any` for a numeral it does not read as
/// a float: `negative`, the significand digits and the exponent, built
/// field by field.
fn decimal_of(negative: bool, digits: &str, exponent: i64) -> Value {
    Value::Decimal(DecimalValue {
        negative,
        digits: digits.to_owned(),
        exponent: WideInt::from(exponent),
        class: DecimalClass::Finite,
    })
}

/// At a binary64 tie the even text reads as the float and the other tie
/// text does not: x = 2^50 + 0.25 lies midway between `1125899906842624.2`
/// and `1125899906842624.3`, both of which round to x, and its shortest
/// text is the former. Under `Float { bits: 64 }` the latter is refused for
/// precision, since it is neither x exactly nor x's shortest text, and under
/// `Any` it is read as the decimal it writes. For 2^50 + 0.75 the even text
/// is `.8`.
#[test]
fn a_binary64_tie_reads_by_the_even_text() {
    let f64_ = spec(ScalarKind::Float { bits: 64 });
    let any = spec(ScalarKind::Any);
    // 2^50 + 1/4 and 2^50 + 3/4, from their bits (ulp 1/4 in [2^50, 2^51)).
    let quarter = f64::from_bits(0x4310_0000_0000_0001);
    let three_quarters = f64::from_bits(0x4310_0000_0000_0003);
    assert_eq!(format!("{quarter:.2}"), "1125899906842624.25");
    assert_eq!(format!("{three_quarters:.2}"), "1125899906842624.75");
    assert_eq!(
        DecimalValue::from_f64_shortest(quarter)
            .unwrap()
            .to_string(),
        "1125899906842624.2"
    );
    for (sign, negative) in [("", false), ("-", true)] {
        let signed = |x: f64| if negative { -x } else { x };
        for spec in [&f64_, &any] {
            assert_eq!(
                float_bits(number(&format!("{sign}1125899906842624.2"), spec)),
                signed(quarter).to_bits(),
                "{sign}1125899906842624.2 under {:?}",
                spec.kind
            );
            assert_eq!(
                float_bits(number(&format!("{sign}1125899906842624.8"), spec)),
                signed(three_quarters).to_bits(),
                "{sign}1125899906842624.8 under {:?}",
                spec.kind
            );
        }
        assert_eq!(
            reason(number(&format!("{sign}1125899906842624.3"), &f64_)),
            FidelityReason::PrecisionExceeded { max: 17 }
        );
        assert_eq!(
            reason(number(&format!("{sign}1125899906842624.7"), &f64_)),
            FidelityReason::PrecisionExceeded { max: 17 }
        );
        assert_eq!(
            number(&format!("{sign}1125899906842624.3"), &any).unwrap(),
            decimal_of(negative, "11258999068426243", -1)
        );
        assert_eq!(
            number(&format!("{sign}1125899906842624.7"), &any).unwrap(),
            decimal_of(negative, "11258999068426247", -1)
        );
    }
}

/// The closest-shortest text of the binary64 value x = 2^50 + i / 4, from
/// its definition: among the decimals with the fewest significant digits
/// that read back to x at binary64 under round-to-nearest-even, the
/// closest to x, at a tie the one whose significand is even. Returns its
/// significand digits, its number of significant digits and whether there
/// was a tie. Exact integer arithmetic in units of 1 / 80, in which x is 80 * 2^50 + 20 i, half the
/// gap above x is 10 (the ulp is 1 / 4) and a decimal of n significant
/// digits (x has sixteen integer digits) is a multiple of 8 * 10^(17 - n).
fn closest_shortest_reference_64(i: u64) -> (u128, u32, bool) {
    let x = 80 * (1_u128 << 50) + 20 * u128::from(i);
    let half_up = 10_u128;
    // At 2^50 the gap below is half as wide.
    let half_down = if i == 0 { 5 } else { half_up };
    // The significand of x is 2^52 + i.
    let even = i % 2 == 0;
    let reads_back = |v: u128| {
        if v >= x {
            v - x < half_up || (v - x == half_up && even)
        } else {
            x - v < half_down || (x - v == half_down && even)
        }
    };
    for n in 1..=17_u32 {
        let step = 8 * 10_u128.pow(17 - n);
        let below = x / step * step;
        let above = below + step;
        let candidates: Vec<u128> = [below, above]
            .into_iter()
            .filter(|&v| reads_back(v))
            .collect();
        match candidates.as_slice() {
            [] => {}
            [v] => return (v / step, n, false),
            [a, b] => {
                let (da, db) = (x - a, b - x);
                // The closer, and at a tie the even significand.
                let pick = if da < db || (da == db && (a / step) % 2 == 0) {
                    a
                } else {
                    b
                };
                return (pick / step, n, da == db);
            }
            _ => unreachable!(),
        }
    }
    unreachable!("seventeen digits always suffice")
}

/// The positional text of a seventeen-digit significand of a value with
/// sixteen integer digits: `dddddddddddddddd.d`.
fn positional_17(sign: &str, digits: u128) -> String {
    let s = digits.to_string();
    assert_eq!(s.len(), 17, "{s}");
    format!("{sign}{}.{}", &s[..16], &s[16..])
}

/// On the first 2^20 binary64 values of the binade [2^50, 2^51), that is
/// x = 2^50 + i / 4 for 0 <= i < 2^20, of either sign, the production
/// printer `DecimalValue::from_f64_shortest` is the closest-shortest text of
/// the definition, and at every tie (each n + 0.25 and n + 0.75) the even
/// text reads as the float under `Float { bits: 64 }` and `Any`, while the
/// other tie text is refused with `PrecisionExceeded { max: 17 }` under
/// `Float { bits: 64 }` and read under `Any` as the decimal it writes.
/// Prints the bound and the ties.
#[test]
fn binary64_ties_read_by_the_even_text_on_a_binade_prefix() {
    let f64_ = spec(ScalarKind::Float { bits: 64 });
    let any = spec(ScalarKind::Any);
    let count = 1_u64 << 20;
    let mut ties = 0_u64;
    let mut first_ties = Vec::new();
    for i in 0..count {
        let magnitude = f64::from_bits(0x4310_0000_0000_0000 | i);
        let (digits, n, tie) = closest_shortest_reference_64(i);
        assert_eq!(tie, i % 2 == 1, "the ties are the odd i: {i}");
        for (sign, negative) in [("", false), ("-", true)] {
            let x = if negative { -magnitude } else { magnitude };
            let printed = DecimalValue::from_f64_shortest(x).unwrap();
            assert_eq!(printed.negative, negative);
            let (got, exponent) = leading(&printed);
            assert_eq!(exponent, 15, "{i}: {printed}");
            let want = digits.to_string();
            assert_eq!(got, want.trim_end_matches('0'), "{i}: {printed}");
            if !tie {
                continue;
            }
            assert_eq!(n, 17);
            assert_eq!(digits % 2, 0, "the tie goes to the even candidate");
            // x is 80 * 2^50 + 20 i in units of 1/80; a 17-digit candidate
            // is a multiple of 8.
            let x_units = 80 * (1_u128 << 50) + 20 * u128::from(i);
            let other = if digits * 8 < x_units {
                digits + 1
            } else {
                digits - 1
            };
            let winner = positional_17(sign, digits);
            let loser = positional_17(sign, other);
            for spec in [&f64_, &any] {
                assert_eq!(
                    float_bits(number(&winner, spec)),
                    x.to_bits(),
                    "{winner} under {:?}",
                    spec.kind
                );
            }
            assert_eq!(
                reason(number(&loser, &f64_)),
                FidelityReason::PrecisionExceeded { max: 17 },
                "{loser}"
            );
            assert_eq!(
                number(&loser, &any).unwrap(),
                decimal_of(negative, &other.to_string(), -1),
                "{loser}"
            );
        }
        if tie {
            ties += 1;
            if first_ties.len() < 4 {
                first_ties.push(format!("{magnitude:.2}"));
            }
        }
    }
    println!(
        "bound: {count} binary64 values 2^50 + i/4, 0 <= i < 2^20, of either sign; {ties} ties, the first {first_ties:?}"
    );
    assert_eq!(ties, count / 2);
}

/// What a kind must read a numeral as, described field by field so that no
/// constructor of the code under test builds it.
#[derive(Debug, PartialEq, Eq)]
enum Far {
    /// `Value::Int`.
    Int(i64),
    /// `Value::BigInt` with this sign, significand and count of zeros.
    Big(bool, String, String),
    /// `Value::Decimal` with this sign, digits and exponent.
    Decimal(bool, String, String),
    /// `Value::Float` with these bits.
    Float(u64),
    /// A refusal for this reason.
    Refused(FidelityReason),
}

/// The [`Far`] description of what `coerce` returned.
fn far_of(result: Result<Value, ValueFidelityError>) -> Far {
    match result {
        Ok(Value::Int(n)) => Far::Int(n),
        Ok(Value::BigInt(b)) => Far::Big(b.negative, b.significand, b.zeros.to_string()),
        Ok(Value::Decimal(d)) => Far::Decimal(d.negative, d.digits, d.exponent.to_string()),
        Ok(Value::Float(x)) => Far::Float(x.to_bits()),
        Ok(other) => panic!("not a numeric value: {other:?}"),
        Err(e) => Far::Refused(e.reason),
    }
}

/// What a kind must read a numeral `<sign><significand>e<written>` as,
/// computed here from the numeral's parts without the code under test, for
/// a stored exponent so far from zero (beyond `i32`, |E| > 2^31 - 2) that
/// no float of either width holds a non-zero value of it.
///
/// `digits` are the significand's digits without leading zeros (`""` for a
/// zero) and `stored` the stored exponent E, so the value is
/// `(-1)^negative × digits × 10^E`.
fn expected_far(negative: bool, digits: &str, stored: i128, kind: &ValueSpec) -> Far {
    let zero = digits.is_empty();
    let sig = digits.trim_end_matches('0');
    let trailing = i128::try_from(digits.len() - sig.len()).unwrap();
    let decimal = || {
        Far::Decimal(
            negative,
            if zero {
                "0".to_owned()
            } else {
                digits.to_owned()
            },
            stored.to_string(),
        )
    };
    let signed_zero = if negative { (-0.0_f64).to_bits() } else { 0 };
    match &kind.kind {
        ScalarKind::Decimal => {
            if zero || (kind.precision.is_none() && kind.scale.is_none()) {
                return decimal();
            }
            // The facets of the scope: precision 3 and scale 2.
            let minimal_scale = (-stored - trailing).max(0);
            if minimal_scale > 2 {
                return Far::Refused(FidelityReason::ScaleExceeded { max: 2 });
            }
            let integer_digits = (i128::try_from(digits.len()).unwrap() + stored).max(0);
            if integer_digits > 1 {
                return Far::Refused(FidelityReason::PrecisionExceeded { max: 3 });
            }
            decimal()
        }
        ScalarKind::BigInt | ScalarKind::Int { .. } => {
            if zero {
                return Far::Int(0);
            }
            let zeros = stored + trailing;
            if zeros < 0 {
                return Far::Refused(FidelityReason::ScaleExceeded { max: 0 });
            }
            if matches!(kind.kind, ScalarKind::Int { .. }) {
                // More than 39 digits: no fixed-width integer holds it.
                return Far::Refused(FidelityReason::Overflow);
            }
            Far::Big(negative, sig.to_owned(), zeros.to_string())
        }
        ScalarKind::Float { bits } => {
            let max = if *bits == 32 { 9 } else { 17 };
            if zero {
                Far::Float(signed_zero)
            } else if stored > 0 {
                Far::Refused(FidelityReason::Overflow)
            } else {
                Far::Refused(FidelityReason::PrecisionExceeded { max })
            }
        }
        ScalarKind::Any => {
            if zero {
                Far::Float(signed_zero)
            } else {
                decimal()
            }
        }
        other => panic!("not a numeric kind: {other:?}"),
    }
}

/// Every numeral of at most two significant digits, zero included (with
/// zero, one or two fractional zeros), of either sign, with a stored
/// exponent among `i32::MIN - 1`, `i32::MIN`, `i32::MAX`, `i32::MAX + 1`,
/// 3000000000, -3000000000 and plus or minus 10^20, under every numeric
/// kind (`Decimal` with and without precision 3 and scale 2) and `Any`,
/// against [`expected_far`]: a zero is the zero of its kind (with its
/// written scale under `Decimal`), a decimal keeps its stored exponent, a
/// big integer is held without expanding it, and the other readings refuse
/// with the kind's reason. Prints the bound.
#[test]
fn zero_numerals_beyond_i32_read_as_zero_where_the_kind_keeps_no_scale() {
    let kinds = [
        spec(ScalarKind::BigInt),
        spec(ScalarKind::Int {
            bits: 8,
            signed: true,
        }),
        spec(ScalarKind::Int {
            bits: 64,
            signed: false,
        }),
        spec(ScalarKind::Int {
            bits: 128,
            signed: true,
        }),
        spec(ScalarKind::Float { bits: 32 }),
        spec(ScalarKind::Float { bits: 64 }),
        spec(ScalarKind::Any),
        spec(ScalarKind::Decimal),
        ValueSpec {
            precision: Some(3),
            scale: Some(2),
            ..spec(ScalarKind::Decimal)
        },
    ];
    let stored = [
        i128::from(i32::MIN) - 1,
        i128::from(i32::MIN),
        i128::from(i32::MAX),
        i128::from(i32::MAX) + 1,
        3_000_000_000,
        -3_000_000_000,
        100_000_000_000_000_000_000,
        -100_000_000_000_000_000_000,
    ];
    // (written significand, its digits without leading zeros, fraction length)
    let mut significands = vec![
        ("0".to_owned(), String::new(), 0_i128),
        ("0.0".to_owned(), String::new(), 1),
        ("0.00".to_owned(), String::new(), 2),
    ];
    for n in 1..=99_u32 {
        significands.push((n.to_string(), n.to_string(), 0));
        if n >= 10 {
            significands.push((format!("{}.{}", n / 10, n % 10), n.to_string(), 1));
        }
    }
    let mut checked = 0;
    for sign in ["", "-"] {
        for (written_significand, digits, frac_len) in &significands {
            for &stored_exponent in &stored {
                let written = stored_exponent + frac_len;
                let text = format!("{sign}{written_significand}e{written}");
                for kind in &kinds {
                    let got = far_of(number(&text, kind));
                    let want = expected_far(!sign.is_empty(), digits, stored_exponent, kind);
                    assert_eq!(got, want, "{text} {:?}", kind.kind);
                    checked += 1;
                }
            }
        }
    }
    println!(
        "bound: {checked} (numeral, kind) pairs: 2 signs x {} significands x 8 stored exponents x 9 kinds",
        significands.len()
    );
    assert_eq!(checked, 2 * significands.len() * 8 * 9);
}

/// Every numeral `<s>e<n>` with s among 1, 7 and 12, of either sign, and n
/// among 0 to 30, 4096, 4097, 5000, 2^31 - 1, 2^31, 2^63 - 1, 2^63 and
/// 10^20, read under `BigInt`: an `i64` when it fits, else the big integer
/// with that significand and n (plus the trailing zeros of s) zeros, equal
/// as a value and in canonical bytes to its integer written out where that
/// has at most 5000 digits, and refused under `Int { bits: 64 }` exactly
/// when it leaves `i64`. Prints the bound.
#[test]
fn bigint_numerals_read_alike_whatever_their_exponent_on_the_stated_scope() {
    let big_int = spec(ScalarKind::BigInt);
    let int = spec(ScalarKind::Int {
        bits: 64,
        signed: true,
    });
    let mut exponents: Vec<u128> = (0..=30).collect();
    exponents.extend([
        4096,
        4097,
        5000,
        (1 << 31) - 1,
        1 << 31,
        (1 << 63) - 1,
        1 << 63,
        100_000_000_000_000_000_000,
    ]);
    let mut checked = 0;
    let mut written_out = 0;
    for sign in ["", "-"] {
        for s in ["1", "7", "12"] {
            for &n in &exponents {
                let text = format!("{sign}{s}e{n}");
                let got = number(&text, &big_int).unwrap();
                // The reference: the integer as an i128 when it has at most
                // 38 digits, else its significand and count.
                let digit_count = u128::try_from(s.len()).unwrap() + n;
                let fits_i64 = if digit_count <= 19 {
                    let value = format!("{sign}{s}{}", "0".repeat(usize::try_from(n).unwrap()))
                        .parse::<i128>()
                        .unwrap();
                    i64::try_from(value).ok()
                } else {
                    None
                };
                if let Some(v) = fits_i64 {
                    assert_eq!(got, Value::Int(v), "{text}");
                    assert_eq!(number(&text, &int).unwrap(), Value::Int(v), "{text}");
                } else {
                    let Value::BigInt(b) = &got else {
                        panic!("{text}: {got:?}")
                    };
                    assert_eq!(
                        (b.negative, b.significand.as_str(), b.zeros.to_string()),
                        (!sign.is_empty(), s, n.to_string()),
                        "{text}"
                    );
                    assert_eq!(reason(number(&text, &int)), FidelityReason::Overflow);
                }
                if n <= 5000 {
                    let written = format!("{sign}{s}{}", "0".repeat(usize::try_from(n).unwrap()));
                    let out = number(&written, &big_int).unwrap();
                    assert_eq!(out, got, "{text}");
                    assert_eq!(out.canonical_bytes(), got.canonical_bytes(), "{text}");
                    written_out += 1;
                }
                checked += 1;
            }
        }
    }
    println!(
        "bound: {checked} numerals (2 signs x 3 significands x {} exponents), {written_out} compared with their integer written out",
        exponents.len()
    );
    assert_eq!(checked, 2 * 3 * exponents.len());
}
