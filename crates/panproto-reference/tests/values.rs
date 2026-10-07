#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Worked examples of the coercion and printing reference, each taken from
//! the value-layer definitions (law (i), the narrowing rules, the NaN rules).

use panproto_reference::coerce::{
    FidelityReason as R, FloatReading, LexClass, ScalarKind as K, ValueSpec, coerce, coerce_value,
};
use panproto_reference::exact::{Decimal, ExactInteger, parse_decimal, parse_numeral, integer_of_numeral};
use panproto_reference::value::Value;

fn num(text: &str, kind: K) -> Result<Value, R> {
    coerce(text, &LexClass::Number, &ValueSpec::of(kind)).unwrap()
}

fn f(x: f64) -> Value {
    Value::Float(x.to_bits())
}

const F16: K = K::Float { bits: 16 };
const F32: K = K::Float { bits: 32 };
const F64: K = K::Float { bits: 64 };

#[test]
fn the_open_reading_follows_decision_24_and_ties_to_even() {
    assert_eq!(num("0.1", K::Any), Ok(f(0.1)));
    assert_eq!(num("0.10", K::Any), Ok(f(0.1)));
    assert_eq!(num("-0.0", K::Any), Ok(f(-0.0)));
    assert_eq!(num("-0e0", K::Any), Ok(f(-0.0)));
    assert_eq!(num("-0", K::Any), Ok(Value::Int(0)));
    assert_eq!(num("5e-324", K::Any), Ok(Value::Float(1)));
    assert!(matches!(num("4e-324", K::Any), Ok(Value::Decimal(_))));
    assert!(matches!(num("1e400", K::Any), Ok(Value::Decimal(_))));
    assert!(matches!(num("9007199254740993.0", K::Any), Ok(Value::Decimal(_))));
    assert_eq!(num("1125899906842624.2", K::Any), Ok(f(1_125_899_906_842_624.25)));
    assert!(matches!(num("1125899906842624.3", K::Any), Ok(Value::Decimal(_))));
    assert!(matches!(num("18446744073709551615", K::Any), Ok(Value::BigInt(_))));
}

#[test]
fn law_i_accepts_exact_and_canonical_numerals_only() {
    assert_eq!(num("1125899906842624.2", F64), Ok(f(1_125_899_906_842_624.25)));
    assert_eq!(num("1125899906842624.3", F64), Err(R::PrecisionExceeded { max: 17 }));
    assert_eq!(num("0.1", F32), Ok(f(f64::from(0.1f32))));
    assert_eq!(num("0.100000001490116119384765625", F32), Ok(f(f64::from(0.1f32))));
    assert_eq!(num("0.10000000149011612", F32), Err(R::PrecisionExceeded { max: 9 }));
    assert_eq!(num("9007199254740992", F32), Ok(f(9_007_199_254_740_992.0)));
    assert_eq!(num("16777217.0", F32), Err(R::PrecisionExceeded { max: 9 }));
    assert_eq!(num("52401.312", F32), Ok(f(52401.3125)));
    assert_eq!(num("52401.313", F32), Err(R::PrecisionExceeded { max: 9 }));
    assert_eq!(num("12345679000", F32), Ok(f(12_345_678_848.0)));
    assert_eq!(num("1e39", F32), Err(R::Overflow));
    assert_eq!(num("-0", F64), Ok(f(-0.0)));
    // binary16: the ruling's run.
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
    assert_eq!(num("NaN", K::Int { bits: 64, signed: true }), Err(R::NonFinite));
}

#[test]
fn exact_kinds_admit_by_value_and_facets() {
    let spec = |scale, precision| ValueSpec { scale, precision, ..ValueSpec::of(K::Decimal) };
    assert_eq!(coerce("1.005", &LexClass::Number, &spec(Some(2), None)).unwrap(), Err(R::ScaleExceeded { max: 2 }));
    assert_eq!(coerce("1.234", &LexClass::Number, &spec(None, Some(3))).unwrap(), Err(R::PrecisionExceeded { max: 3 }));
    assert_eq!(num("1e2147483647", K::Int { bits: 64, signed: true }), Err(R::Overflow));
    assert_eq!(num("127", K::Int { bits: 8, signed: true }), Ok(Value::Int(127)));
    assert_eq!(num("128", K::Int { bits: 8, signed: true }), Err(R::Overflow));
    assert_eq!(num("-128", K::Int { bits: 8, signed: true }), Ok(Value::Int(-128)));
    assert_eq!(num("-1", K::Int { bits: 8, signed: false }), Err(R::Overflow));
    assert_eq!(num("1.5", K::BigInt), Err(R::ScaleExceeded { max: 0 }));
    let big = num("1e4097", K::BigInt).unwrap();
    let written = num(&format!("1{}", "0".repeat(4097)), K::BigInt).unwrap();
    assert_eq!(big, written);
    assert_eq!(num("NaN", K::Decimal), Ok(Value::Decimal(parse_decimal("NaN").unwrap())));
    assert!(matches!(num("-0", K::Decimal), Ok(Value::Decimal(Decimal::Finite(d))) if d.negative));
}

#[test]
fn coerce_value_narrows_only_exact_values() {
    let v = |val: Value, kind| coerce_value(&val, &ValueSpec::of(kind)).unwrap();
    assert_eq!(v(f(0.1), F32), Err(R::PrecisionExceeded { max: 9 }));
    assert_eq!(v(f(f64::from(0.1f32)), F32), Ok(f(f64::from(0.1f32))));
    assert_eq!(v(f(1e39), F32), Err(R::Overflow));
    assert_eq!(v(f(3.402_823_567_797_336_6e38), F32), Err(R::Overflow));
    assert_eq!(v(f(3.402_823_567_797_336e38), F32), Err(R::PrecisionExceeded { max: 9 }));
    assert_eq!(v(Value::Float(0x7ff0_0000_2000_0000), F32), Ok(Value::Float(0x7ff0_0000_2000_0000)));
    assert_eq!(v(Value::Float(0x7ff8_0000_0000_1234), F32), Err(R::NonFinite));
    assert_eq!(v(Value::Float(0x7ff8_0000_0000_1234), F64), Ok(Value::Float(0x7ff8_0000_0000_1234)));
    assert_eq!(v(Value::Int(16_777_217), F32), Err(R::PrecisionExceeded { max: 9 }));
    assert_eq!(v(Value::Int(16_777_216), F32), Ok(f(16_777_216.0)));
    assert_eq!(v(Value::Int(2049), F16), Err(R::PrecisionExceeded { max: 5 }));
    let pow = |text: &str| Value::integer(integer_of_numeral(&parse_numeral(text).unwrap()).unwrap());
    assert_eq!(v(pow("1180591620717411303424"), F32), Ok(f(2f64.powi(70))));
    assert_eq!(v(pow("1e39"), F32), Err(R::Overflow));
    assert_eq!(v(pow("1e39"), F64), Err(R::PrecisionExceeded { max: 17 }));
    assert_eq!(v(Value::Decimal(parse_decimal("0.1").unwrap()), F32), Err(R::PrecisionExceeded { max: 9 }));
    assert_eq!(v(f(f64::NAN), K::Decimal), Err(R::NonFinite));
    assert_eq!(v(Value::List(vec![Value::Int(1)]), F32), Ok(Value::List(vec![Value::Int(1)])));
    assert!(matches!(ExactInteger::Int(0), ExactInteger::Int(0)));
}

#[test]
fn the_nearest_keeping_lexeme_reading_refuses_no_numeral() {
    let spec = ValueSpec { float_reading: FloatReading::NearestKeepingLexeme, ..ValueSpec::of(F64) };
    let read = |t: &str| coerce(t, &LexClass::Number, &spec).unwrap();
    assert_eq!(read("0.1000000000000000055"), Ok(f(0.1)));
    assert_eq!(read("12345678901234567170.0"), Ok(f(12_345_678_901_234_567_168.0)));
    assert_eq!(read("9.0e+999"), Ok(f(f64::INFINITY)));
    assert_eq!(read("-9.0e+999"), Ok(f(f64::NEG_INFINITY)));
    assert_eq!(num("0.1000000000000000055", F64), Err(R::PrecisionExceeded { max: 17 }));
}

#[test]
fn coerce_value_is_idempotent_and_fixes_what_coerce_returns() {
    let lexemes = ["0", "-0", "0.1", "1.5", "2.5e3", "1e-7", "123456789", "65504", "3.4028235e38", "NaN", "-inf", "7"];
    let kinds = [F16, F32, F64, K::Decimal, K::BigInt, K::Int { bits: 32, signed: true }, K::Any];
    for kind in kinds {
        let spec = ValueSpec::of(kind);
        for t in lexemes {
            let Some(Ok(v)) = coerce(t, &LexClass::Number, &spec) else { continue };
            let once = coerce_value(&v, &spec).unwrap();
            assert_eq!(once, Ok(v.clone()), "fixed point: {t} under {kind:?}");
            let twice = coerce_value(&once.clone().unwrap(), &spec).unwrap();
            assert_eq!(twice, once, "idempotence: {t} under {kind:?}");
        }
    }
}
