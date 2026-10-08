#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Worked examples of the coercion and printing reference, each taken from
//! the value-layer definitions (law (i), the narrowing rules, the NaN rules).

use std::collections::BTreeMap;

use panproto_reference::coerce::{
    FidelityReason as R, FloatReading, LexClass, ScalarKind as K, ValueSpec, coerce, coerce_value,
};
use panproto_reference::exact::{
    Decimal, ExactInteger, integer_of_numeral, parse_decimal, parse_numeral,
};
use panproto_reference::value::{BuiltinOp, EvalError, Expr, Value, apply_builtin, eval};

fn num(text: &str, kind: K) -> Result<Value, R> {
    coerce(text, &LexClass::Number, &ValueSpec::of(kind)).unwrap()
}

/// The bits of 2^50 + 1/4, the binary64 value whose closest shortest
/// numeral is `1125899906842624.2`.
const TWO_POW_50_PLUS_QUARTER: u64 = 0x4310_0000_0000_0001;

const fn f(x: f64) -> Value {
    Value::Float(x.to_bits())
}

const F16: K = K::Float { bits: 16 };
const F32: K = K::Float { bits: 32 };
const F64: K = K::Float { bits: 64 };

#[test]
fn the_open_reading_admits_every_kind_and_ties_to_even() {
    assert_eq!(num("0.1", K::Any), Ok(f(0.1)));
    assert_eq!(num("0.10", K::Any), Ok(f(0.1)));
    assert_eq!(num("-0.0", K::Any), Ok(f(-0.0)));
    assert_eq!(num("-0e0", K::Any), Ok(f(-0.0)));
    assert_eq!(num("-0", K::Any), Ok(Value::Int(0)));
    assert_eq!(num("5e-324", K::Any), Ok(Value::Float(1)));
    assert!(matches!(num("4e-324", K::Any), Ok(Value::Decimal(_))));
    assert!(matches!(num("1e400", K::Any), Ok(Value::Decimal(_))));
    assert!(matches!(
        num("9007199254740993.0", K::Any),
        Ok(Value::Decimal(_))
    ));
    assert_eq!(
        num("1125899906842624.2", K::Any),
        Ok(Value::Float(TWO_POW_50_PLUS_QUARTER))
    );
    assert!(matches!(
        num("1125899906842624.3", K::Any),
        Ok(Value::Decimal(_))
    ));
    assert!(matches!(
        num("18446744073709551615", K::Any),
        Ok(Value::BigInt(_))
    ));
}

#[test]
fn law_i_accepts_exact_and_canonical_numerals_only() {
    assert_eq!(
        num("1125899906842624.2", F64),
        Ok(Value::Float(TWO_POW_50_PLUS_QUARTER))
    );
    assert_eq!(
        num("1125899906842624.3", F64),
        Err(R::PrecisionExceeded { max: 17 })
    );
    assert_eq!(num("0.1", F32), Ok(f(f64::from(0.1f32))));
    assert_eq!(
        num("0.100000001490116119384765625", F32),
        Ok(f(f64::from(0.1f32)))
    );
    assert_eq!(
        num("0.10000000149011612", F32),
        Err(R::PrecisionExceeded { max: 9 })
    );
    assert_eq!(num("9007199254740992", F32), Ok(f(9_007_199_254_740_992.0)));
    assert_eq!(num("16777217.0", F32), Err(R::PrecisionExceeded { max: 9 }));
    assert_eq!(num("52401.312", F32), Ok(f(52401.3125)));
    assert_eq!(num("52401.313", F32), Err(R::PrecisionExceeded { max: 9 }));
    assert_eq!(num("12345679000", F32), Ok(f(12_345_678_848.0)));
    assert_eq!(num("1e39", F32), Err(R::Overflow));
    assert_eq!(num("-0", F64), Ok(f(-0.0)));
    // binary16: the narrowest float format, with its subnormals and overflow.
    assert_eq!(num("0.1", F16), Ok(f(0.099_975_585_937_5)));
    assert_eq!(num("65500", F16), Ok(f(65504.0)));
    assert_eq!(num("65504", F16), Ok(f(65504.0)));
    assert_eq!(num("6e-8", F16), Ok(f(2f64.powi(-24))));
    assert_eq!(num("2050", F16), Ok(f(2050.0)));
    assert_eq!(num("2049", F16), Err(R::PrecisionExceeded { max: 5 }));
    assert_eq!(num("65519", F16), Err(R::PrecisionExceeded { max: 5 }));
    assert_eq!(num("65520", F16), Err(R::Overflow));
}

#[test]
fn nan_spellings_carry_their_sign_and_no_payload() {
    for kind in [F32, F64, K::Any] {
        assert_eq!(num("NaN", kind), Ok(Value::Float(0x7ff8_0000_0000_0000)));
        assert_eq!(num("-nan", kind), Ok(Value::Float(0xfff8_0000_0000_0000)));
        assert_eq!(num("-.nan", kind), Ok(Value::Float(0xfff8_0000_0000_0000)));
        assert_eq!(num("-Infinity", kind), Ok(f(f64::NEG_INFINITY)));
    }
    assert_eq!(
        num(
            "NaN",
            K::Int {
                bits: 64,
                signed: true
            }
        ),
        Err(R::NonFinite)
    );
}

#[test]
fn exact_kinds_admit_by_value_and_facets() {
    let spec = |scale, precision| ValueSpec {
        scale,
        precision,
        ..ValueSpec::of(K::Decimal)
    };
    assert_eq!(
        coerce("1.005", &LexClass::Number, &spec(Some(2), None)).unwrap(),
        Err(R::ScaleExceeded { max: 2 })
    );
    assert_eq!(
        coerce("1.234", &LexClass::Number, &spec(None, Some(3))).unwrap(),
        Err(R::PrecisionExceeded { max: 3 })
    );
    assert_eq!(
        num(
            "1e2147483647",
            K::Int {
                bits: 64,
                signed: true
            }
        ),
        Err(R::Overflow)
    );
    assert_eq!(
        num(
            "127",
            K::Int {
                bits: 8,
                signed: true
            }
        ),
        Ok(Value::Int(127))
    );
    assert_eq!(
        num(
            "128",
            K::Int {
                bits: 8,
                signed: true
            }
        ),
        Err(R::Overflow)
    );
    assert_eq!(
        num(
            "-128",
            K::Int {
                bits: 8,
                signed: true
            }
        ),
        Ok(Value::Int(-128))
    );
    assert_eq!(
        num(
            "-1",
            K::Int {
                bits: 8,
                signed: false
            }
        ),
        Err(R::Overflow)
    );
    assert_eq!(num("1.5", K::BigInt), Err(R::ScaleExceeded { max: 0 }));
    let big = num("1e4097", K::BigInt).unwrap();
    let written = num(&format!("1{}", "0".repeat(4097)), K::BigInt).unwrap();
    assert_eq!(big, written);
    assert_eq!(
        num("NaN", K::Decimal),
        Ok(Value::Decimal(parse_decimal("NaN").unwrap()))
    );
    assert!(matches!(num("-0", K::Decimal), Ok(Value::Decimal(Decimal::Finite(d))) if d.negative));
}

#[test]
fn coerce_value_narrows_only_exact_values() {
    let v = |val: Value, kind| coerce_value(&val, &ValueSpec::of(kind)).unwrap();
    assert_eq!(v(f(0.1), F32), Err(R::PrecisionExceeded { max: 9 }));
    assert_eq!(v(f(f64::from(0.1f32)), F32), Ok(f(f64::from(0.1f32))));
    assert_eq!(v(f(1e39), F32), Err(R::Overflow));
    assert_eq!(v(f(3.402_823_567_797_336_6e38), F32), Err(R::Overflow));
    assert_eq!(
        v(f(3.402_823_567_797_336e38), F32),
        Err(R::PrecisionExceeded { max: 9 })
    );
    assert_eq!(
        v(Value::Float(0x7ff0_0000_2000_0000), F32),
        Ok(Value::Float(0x7ff0_0000_2000_0000))
    );
    assert_eq!(
        v(Value::Float(0x7ff8_0000_0000_1234), F32),
        Err(R::NonFinite)
    );
    assert_eq!(
        v(Value::Float(0x7ff8_0000_0000_1234), F64),
        Ok(Value::Float(0x7ff8_0000_0000_1234))
    );
    assert_eq!(
        v(Value::Int(16_777_217), F32),
        Err(R::PrecisionExceeded { max: 9 })
    );
    assert_eq!(v(Value::Int(16_777_216), F32), Ok(f(16_777_216.0)));
    assert_eq!(
        v(Value::Int(2049), F16),
        Err(R::PrecisionExceeded { max: 5 })
    );
    let pow =
        |text: &str| Value::integer(integer_of_numeral(&parse_numeral(text).unwrap()).unwrap());
    assert_eq!(v(pow("1180591620717411303424"), F32), Ok(f(2f64.powi(70))));
    assert_eq!(v(pow("1e39"), F32), Err(R::Overflow));
    assert_eq!(v(pow("1e39"), F64), Err(R::PrecisionExceeded { max: 17 }));
    assert_eq!(
        v(Value::Decimal(parse_decimal("0.1").unwrap()), F32),
        Err(R::PrecisionExceeded { max: 9 })
    );
    assert_eq!(v(f(f64::NAN), K::Decimal), Err(R::NonFinite));
    for text in ["NaN", "-sNaN7", "Infinity", "-Infinity"] {
        let special = Value::Decimal(parse_decimal(text).unwrap());
        assert_eq!(v(special.clone(), K::Decimal), Ok(special.clone()));
        assert_eq!(v(special.clone(), K::BigInt), Err(R::NonFinite));
        assert_eq!(v(special, K::Str), Err(R::NotANumberForKind), "{text}");
    }
    assert_eq!(
        v(Value::List(vec![Value::Int(1)]), F32),
        Ok(Value::List(vec![Value::Int(1)]))
    );
    assert!(matches!(ExactInteger::Int(0), ExactInteger::Int(0)));
}

#[test]
fn sql_special_decimals_reach_the_decimal_parser() {
    use panproto_reference::coerce::NumeralDialect;

    let spec = ValueSpec {
        numeral: Some(NumeralDialect::Sql),
        ..ValueSpec::of(K::Decimal)
    };
    for (text, canonical) in [
        ("nan", "NaN"),
        ("-nan", "-NaN"),
        ("inf", "Infinity"),
        ("+infinity", "Infinity"),
        ("-inf", "-Infinity"),
    ] {
        assert_eq!(
            coerce(text, &LexClass::Number, &spec),
            Some(Ok(Value::Decimal(parse_decimal(canonical).unwrap()))),
            "{text}"
        );
    }
}

#[test]
fn the_nearest_keeping_lexeme_reading_refuses_no_numeral() {
    let spec = ValueSpec {
        float_reading: FloatReading::NearestKeepingLexeme,
        ..ValueSpec::of(F64)
    };
    let read = |t: &str| coerce(t, &LexClass::Number, &spec).unwrap();
    assert_eq!(read("0.1000000000000000055"), Ok(f(0.1)));
    assert_eq!(
        read("12345678901234567170.0"),
        Ok(f(12_345_678_901_234_567_168.0))
    );
    assert_eq!(read("9.0e+999"), Ok(f(f64::INFINITY)));
    assert_eq!(read("-9.0e+999"), Ok(f(f64::NEG_INFINITY)));
    assert_eq!(
        num("0.1000000000000000055", F64),
        Err(R::PrecisionExceeded { max: 17 })
    );
}

#[test]
fn coerce_value_is_idempotent_and_fixes_what_coerce_returns() {
    let lexemes = [
        "0",
        "-0",
        "0.1",
        "1.5",
        "2.5e3",
        "1e-7",
        "123456789",
        "65504",
        "3.4028235e38",
        "NaN",
        "-inf",
        "7",
    ];
    let kinds = [
        F16,
        F32,
        F64,
        K::Decimal,
        K::BigInt,
        K::Int {
            bits: 32,
            signed: true,
        },
        K::Any,
    ];
    for kind in kinds {
        let spec = ValueSpec::of(kind);
        for t in lexemes {
            let Some(Ok(v)) = coerce(t, &LexClass::Number, &spec) else {
                continue;
            };
            let once = coerce_value(&v, &spec).unwrap();
            assert_eq!(once, Ok(v.clone()), "fixed point: {t} under {kind:?}");
            let twice = coerce_value(&once.clone().unwrap(), &spec).unwrap();
            assert_eq!(twice, once, "idempotence: {t} under {kind:?}");
        }
    }
}

#[test]
fn exact_expression_operations_compare_convert_and_refuse_arithmetic() {
    let decimal = Value::Decimal(parse_decimal("1.50").unwrap());
    let big = num("100000000000000000000", K::BigInt).unwrap();
    assert_eq!(
        apply_builtin(
            BuiltinOp::Eq,
            &[
                decimal.clone(),
                Value::Decimal(parse_decimal("1.5").unwrap())
            ]
        ),
        Ok(Value::Bool(false))
    );
    assert_eq!(
        apply_builtin(
            BuiltinOp::Gt,
            &[
                big.clone(),
                Value::Decimal(parse_decimal("99999999999999999999.5").unwrap())
            ]
        ),
        Ok(Value::Bool(true))
    );
    assert_eq!(
        apply_builtin(BuiltinOp::ToString, std::slice::from_ref(&decimal)),
        Ok(Value::Str("1.50".into()))
    );
    assert_eq!(
        apply_builtin(BuiltinOp::ParseDecimal, &[Value::Str("1.50".into())]),
        Ok(decimal.clone())
    );
    assert_eq!(
        apply_builtin(BuiltinOp::DecimalToFloat, std::slice::from_ref(&decimal)),
        Ok(f(1.5))
    );
    for op in [
        BuiltinOp::Add,
        BuiltinOp::Sub,
        BuiltinOp::Mul,
        BuiltinOp::Div,
        BuiltinOp::Mod,
    ] {
        assert!(matches!(
            apply_builtin(op, &[decimal.clone(), Value::Int(1)]),
            Err(EvalError::ExactArithmetic {
                operand: "decimal",
                ..
            })
        ));
        assert!(matches!(
            apply_builtin(op, &[big.clone(), Value::Int(1)]),
            Err(EvalError::ExactArithmetic {
                operand: "bigint",
                ..
            })
        ));
    }
    assert!(matches!(
        apply_builtin(
            BuiltinOp::Lt,
            &[Value::Decimal(parse_decimal("NaN").unwrap()), Value::Int(1)]
        ),
        Err(EvalError::TypeError { .. })
    ));
}

#[test]
fn expression_evaluation_canonicalizes_only_computed_nans() {
    const PAYLOAD_NAN: u64 = 0x7ff8_0000_0000_0042;
    const CANONICAL_NAN: u64 = 0x7ff8_0000_0000_0000;
    let environment = BTreeMap::from([("x".into(), Value::Float(PAYLOAD_NAN))]);
    assert_eq!(
        eval(&Expr::Variable("x".into()), &environment),
        Ok(Value::Float(PAYLOAD_NAN))
    );
    assert_eq!(
        eval(
            &Expr::If(
                Box::new(Expr::Literal(Value::Bool(true))),
                Box::new(Expr::Variable("x".into())),
                Box::new(Expr::Literal(Value::Null)),
            ),
            &environment,
        ),
        Ok(Value::Float(PAYLOAD_NAN))
    );
    assert_eq!(
        eval(
            &Expr::Builtin(
                BuiltinOp::Add,
                vec![
                    Expr::Literal(f(f64::INFINITY)),
                    Expr::Literal(f(f64::NEG_INFINITY)),
                ],
            ),
            &environment,
        ),
        Ok(Value::Float(CANONICAL_NAN))
    );
    assert_eq!(
        eval(
            &Expr::Builtin(BuiltinOp::Neg, vec![Expr::Variable("x".into())]),
            &environment,
        ),
        Ok(Value::Float(CANONICAL_NAN))
    );
    assert_eq!(
        apply_builtin(
            BuiltinOp::DecimalToFloat,
            &[Value::Decimal(parse_decimal("-sNaN7").unwrap())],
        ),
        Ok(Value::Float(CANONICAL_NAN))
    );
}

#[test]
fn a_tagged_lexeme_is_read_by_its_tag_under_the_open_kind_and_by_the_declared_kind_otherwise() {
    let tagged = |text: &str, tag: &str, kind: K| {
        coerce(
            text,
            &LexClass::Tagged(tag.to_owned()),
            &ValueSpec::of(kind),
        )
    };
    // Under `Any` the tag's own kind reads the lexeme, the YAML prefixes
    // stripped; `null` is the null value.
    assert_eq!(
        tagged("12", "tag:yaml.org,2002:int", K::Any),
        Some(Ok(Value::Int(12)))
    );
    assert_eq!(tagged("3.5", "!!float", K::Any), Some(Ok(f(3.5))));
    assert_eq!(
        tagged("true", "!!bool", K::Any),
        Some(Ok(Value::Bool(true)))
    );
    assert_eq!(
        tagged("aGk=", "!!binary", K::Any),
        Some(Ok(Value::Bytes(b"hi".to_vec())))
    );
    assert_eq!(tagged("~", "!!null", K::Any), Some(Ok(Value::Null)));
    assert_eq!(tagged("x", "!!int", K::Any), Some(Err(R::NotANumber)));
    // An unknown tag keeps the lexeme as an opaque value of that type.
    assert_eq!(
        tagged("abc", "!custom", K::Any),
        Some(Ok(Value::Opaque {
            type_: "!custom".to_owned(),
            fields: [("value".to_owned(), Value::Str("abc".to_owned()))].into(),
        }))
    );
    // A declared kind governs: the lexeme is read as a string under it.
    let i64k = K::Int {
        bits: 64,
        signed: true,
    };
    assert_eq!(tagged("12", "!custom", i64k), Some(Ok(Value::Int(12))));
    assert_eq!(
        tagged("12", "!!str", K::Str),
        Some(Ok(Value::Str("12".to_owned())))
    );
}
