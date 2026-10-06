#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The coercion laws, checked against oracles written here without the
//! code under test.
//!
//! On its domain (`coerce` under a kind other than `Any`; `coerce_value`
//! on a value with a scalar lexeme, non-finite floats excluded):
//! (i) lexical fidelity, an `Ok` result is the kind's reading of the
//! lexeme, and a float kind accepts a numeral exactly when the value it
//! rounds to equals it or writes it numerically as its shortest text;
//! (ii) `coerce_value` is idempotent; (iii) every value `coerce` returns is
//! a fixed point of `coerce_value`. Outside the domain, values pass through
//! unchanged. Under a 32-bit float kind `coerce_value` keeps a float only
//! when it is exactly a binary32 value.

use std::cmp::Ordering;

use panproto_inst::Value;
use panproto_inst::value::coerce::{LexClass, Lexeme, coerce, coerce_value};
use panproto_inst::value::{DecimalValue, FidelityReason, Temporal, ValueSite};
use panproto_schema::{BytesText, ScalarKind, ValueSpec};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// An exact oracle: numerals as sign, integer digits and fraction digits,
// and a little-endian base-2^32 natural number for comparing a numeral
// with a binary float exactly.
// ---------------------------------------------------------------------------

/// A finite numeral `[sign] digits [. digits] [e [sign] digits]` with its
/// point moved to the end of the integer digits: sign, the integer digits
/// without leading zeros and the fraction digits without trailing zeros.
fn normalize(text: &str) -> (bool, String, String) {
    let (negative, rest) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let (mantissa, exponent) = match rest.find(['e', 'E']) {
        Some(i) => (&rest[..i], rest[i + 1..].parse::<i64>().unwrap()),
        None => (rest, 0),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut digits = format!("{int}{frac}");
    let mut point = i64::try_from(int.len()).unwrap() + exponent;
    if point < 0 {
        digits = format!("{}{digits}", "0".repeat(usize::try_from(-point).unwrap()));
        point = 0;
    }
    let point = usize::try_from(point).unwrap();
    if point > digits.len() {
        digits.push_str(&"0".repeat(point - digits.len()));
    }
    let int = digits[..point].trim_start_matches('0').to_owned();
    let frac = digits[point..].trim_end_matches('0').to_owned();
    let zero = int.is_empty() && frac.is_empty();
    (negative && !zero, int, frac)
}

/// A natural number, little-endian in base 2^32, without high zero limbs.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Nat(Vec<u32>);

impl Nat {
    fn from_u64(n: u64) -> Self {
        let mut limbs = vec![u32::try_from(n & 0xffff_ffff).unwrap(), u32::try_from(n >> 32).unwrap()];
        while limbs.last() == Some(&0) {
            limbs.pop();
        }
        Self(limbs)
    }

    fn mul_add(&mut self, factor: u32, addend: u32) {
        let mut carry = u64::from(addend);
        for limb in &mut self.0 {
            let product = u64::from(*limb) * u64::from(factor) + carry;
            *limb = u32::try_from(product & 0xffff_ffff).unwrap();
            carry = product >> 32;
        }
        if carry > 0 {
            self.0.push(u32::try_from(carry).unwrap());
        }
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
    }

    fn from_digits(digits: &str) -> Self {
        let mut n = Self(Vec::new());
        for d in digits.bytes() {
            n.mul_add(10, u32::from(d - b'0'));
        }
        n
    }

    fn mul_pow(&mut self, base: u32, power: u64) {
        for _ in 0..power {
            self.mul_add(base, 0);
        }
    }

    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .len()
            .cmp(&other.0.len())
            .then_with(|| self.0.iter().rev().cmp(other.0.iter().rev()))
    }
}

/// Whether the finite numeral `text` equals the finite float `x` exactly.
fn numeral_equals_float(text: &str, x: f64) -> bool {
    let (negative, int, frac) = normalize(text);
    if int.is_empty() && frac.is_empty() {
        return x == 0.0;
    }
    if x == 0.0 || negative != x.is_sign_negative() {
        return false;
    }
    // text = D / 10^f with D the digits int ++ frac and f = frac.len().
    let fraction_len = u64::try_from(frac.len()).unwrap();
    let mut numeral = Nat::from_digits(&format!("{int}{frac}"));
    // |x| = m * 2^e.
    let bits = x.abs().to_bits();
    let biased = i64::try_from(bits >> 52).unwrap();
    let fraction = bits & ((1 << 52) - 1);
    let (m, e) = if biased == 0 {
        (fraction, -1074_i64)
    } else {
        (fraction | (1 << 52), biased - 1075)
    };
    // D * 2^max(-e, 0) == m * 2^max(e, 0) * 10^f.
    let mut float = Nat::from_u64(m);
    float.mul_pow(10, fraction_len);
    if e >= 0 {
        float.mul_pow(2, u64::try_from(e).unwrap());
    } else {
        numeral.mul_pow(2, u64::try_from(-e).unwrap());
    }
    numeral.cmp(&float) == Ordering::Equal
}

/// Whether two finite numerals are numerically equal.
fn numerals_equal(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

/// The float a float kind of `bits` bits reads a finite numeral as, by
/// Rust's correctly rounded parsers, widened to `f64`; the shortest text of
/// that value at the kind's width; and the most significant digits that
/// shortest text has.
fn read_float(text: &str, bits: u8) -> (f64, String, u32) {
    if bits == 32 {
        let x = text.parse::<f32>().unwrap();
        (f64::from(x), format!("{x:e}"), 9)
    } else {
        let x = text.parse::<f64>().unwrap();
        (x, format!("{x:e}"), 17)
    }
}

/// What a float kind of `bits` bits must do with the finite numeral `text`
/// (law (i)): the float it reads, or the reason it is refused.
fn expected_float(text: &str, bits: u8) -> Result<f64, FidelityReason> {
    let (x, shortest, max) = read_float(text, bits);
    if !x.is_finite() {
        return Err(FidelityReason::Overflow);
    }
    if numeral_equals_float(text, x) || numerals_equal(&shortest, text) {
        Ok(x)
    } else {
        Err(FidelityReason::PrecisionExceeded { max })
    }
}

/// What a decimal kind with `precision` and `scale` must do with `text`.
fn expected_decimal(text: &str, precision: Option<u32>, scale: Option<u32>) -> Result<(), FidelityReason> {
    let (_, int, frac) = normalize(text);
    let frac_len = u32::try_from(frac.len()).unwrap();
    let int_len = u32::try_from(int.len()).unwrap();
    if let Some(s) = scale
        && frac_len > s
    {
        return Err(FidelityReason::ScaleExceeded { max: s });
    }
    if let Some(p) = precision {
        let fits = match scale {
            Some(s) => int_len <= p.saturating_sub(s),
            // The digits the number needs in positional notation.
            None => int_len + frac_len <= p,
        };
        if !fits {
            return Err(FidelityReason::PrecisionExceeded { max: p });
        }
    }
    Ok(())
}

/// The integer a numeral writes, as its sign and digits without leading
/// zeros (empty for zero), or `None` when it is not integral.
fn integer_of(text: &str) -> Option<(bool, String)> {
    let (negative, int, frac) = normalize(text);
    frac.is_empty().then_some((negative, int))
}

/// The integer `(negative, digits)` as an `i128`, when it fits.
fn as_i128((negative, digits): &(bool, String)) -> Option<i128> {
    if digits.len() > 38 {
        return None;
    }
    let magnitude = if digits.is_empty() { 0 } else { digits.parse::<i128>().ok()? };
    Some(if *negative { -magnitude } else { magnitude })
}

/// Whether an integer kind of `bits` bits holds `n`.
fn int_holds(n: i128, bits: u8, signed: bool) -> bool {
    let bits = u32::from(bits);
    if bits >= 128 {
        return signed || n >= 0;
    }
    if signed {
        let half = 1_i128 << (bits - 1);
        -half <= n && n < half
    } else {
        n >= 0 && n < (1_i128 << bits)
    }
}

/// The integer value of `(negative, digits)`: `Int` when it fits `i64`,
/// else `BigInt`.
fn integer_value((negative, digits): (bool, String)) -> Value {
    let digits = if digits.is_empty() { "0".to_owned() } else { digits };
    let sign = if negative { "-" } else { "" };
    Value::parse_integer(&format!("{sign}{digits}")).unwrap()
}

// ---------------------------------------------------------------------------
// Specs, generators and comparisons.
// ---------------------------------------------------------------------------

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
        int(128, true),
        spec(ScalarKind::BigInt),
        spec(ScalarKind::Float { bits: 32 }),
        spec(ScalarKind::Float { bits: 64 }),
        spec(ScalarKind::Decimal),
        ValueSpec {
            precision: Some(5),
            scale: Some(2),
            ..spec(ScalarKind::Decimal)
        },
        ValueSpec {
            precision: Some(4),
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

/// Finite numerals a float kind must decide: shortest texts and exact
/// expansions of floats of both widths, arbitrary numerals, and the
/// boundary cases.
fn float_numerals() -> impl Strategy<Value = String> {
    let finite64 = any::<f64>().prop_filter("finite", |x| x.is_finite());
    let finite32 = any::<f32>().prop_filter("finite", |x| x.is_finite());
    prop_oneof![
        finite64.clone().prop_map(|x| format!("{x:e}")),
        finite64.clone().prop_map(|x| format!("{:.1100e}", x)),
        finite32.clone().prop_map(|x| format!("{x:e}")),
        finite32.clone().prop_map(|x| format!("{:.200e}", f64::from(x))),
        finite32.prop_map(|x| format!("{}", f64::from(x))),
        "-?[0-9]{1,20}(\\.[0-9]{1,20})?([eE]-?[0-9]{1,2})?",
        "-?[0-9]{1,3}\\.[0-9]{1,12}",
        prop::sample::select(vec![
            "0.1", "-0.0", "-0", "0e5", "16777217.0", "0.10000000149011612",
            "9007199254740992", "1125899906842624", "3.4028235e38", "3.4028236e38",
            "1e39", "1e-45", "1e-46", "7e-46", "4e-324", "5e-324", "2.4703282292062328e-324",
            "1e-400", "1e400", "1.7976931348623157e308", "1.7976931348623159e308",
        ])
        .prop_map(str::to_owned),
    ]
}

/// Numerals for the decimal and integer kinds, integral and not.
fn exact_numerals() -> impl Strategy<Value = String> {
    prop_oneof![
        "-?[0-9]{1,22}",
        "-?[0-9]{1,8}\\.[0-9]{0,8}",
        "-?[0-9]{1,6}(\\.[0-9]{1,4})?[eE]-?[0-9]",
        prop::sample::select(vec!["0", "-0", "0.000", "1.0", "1e3", "-12.50e1", "0e5", "1.005"])
            .prop_map(str::to_owned),
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
        Just("23:59:60.123+05:30".to_owned()),
        Just("2024-05-06T12:30:05Z".to_owned()),
        Just("2024-05-06T12:30:05-00:00".to_owned()),
        Just("2024-05-06T12:30:05".to_owned()),
        "([0-9a-f]{2}){0,4}",
        Just("aGk=".to_owned()),
        "[a-z]{0,5}",
    ]
}

/// A non-finite `f64`, any sign and payload.
fn non_finite() -> impl Strategy<Value = f64> {
    (any::<bool>(), 0_u64..(1 << 52)).prop_map(|(negative, fraction)| {
        f64::from_bits((u64::from(negative) << 63) | (0x7ff << 52) | fraction)
    })
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
        non_finite().prop_map(Value::Float),
        prop::sample::select(vec![-0.0, 0.1, 1e300, 1e39, 16_777_217.0, 0.123_456_789])
            .prop_map(Value::Float),
        texts().prop_map(Value::Str),
        texts().prop_map(Value::Token),
        Just(Value::Temporal(
            Temporal::parse("2024-05-06T12:30:05+02:00").unwrap()
        )),
        Just(Value::Temporal(Temporal::parse("2024-05-06").unwrap())),
        Just(Value::Null),
        no_lexeme_values(),
    ]
}

/// Values with no scalar lexeme.
fn no_lexeme_values() -> impl Strategy<Value = Value> {
    prop_oneof![
        prop::collection::vec(any::<u8>(), 0..4).prop_map(Value::Bytes),
        Just(Value::List(vec![Value::Int(1)])),
        Just(Value::List(vec![Value::Float(f64::NAN)])),
        any::<u64>().prop_map(Value::LabeledNull),
        Just(Value::Opaque {
            type_: "point".into(),
            fields: std::iter::once(("x".to_owned(), Value::Int(1))).collect(),
        }),
    ]
}

fn same(a: &Value, b: &Value) -> bool {
    a.canonical_cmp(b).is_eq()
}

fn site() -> ValueSite {
    ValueSite::new("v", None)
}

/// The finite float `bits` reads `result` as, or a failed comparison.
fn assert_float_result(
    result: &Result<Value, panproto_inst::ValueFidelityError>,
    expected: &Result<f64, FidelityReason>,
) -> Result<(), TestCaseError> {
    match (result, expected) {
        (Ok(Value::Float(y)), Ok(x)) => prop_assert_eq!(y.to_bits(), x.to_bits()),
        (Err(e), Err(reason)) => prop_assert_eq!(&e.reason, reason),
        _ => prop_assert!(false, "got {result:?}, expected {expected:?}"),
    }
    Ok(())
}

/// What a numeric kind must do with the finite numeral `text`, read as
/// the kind's value: `None` when the kind is not numeric.
fn expected_numeral(text: &str, spec: &ValueSpec) -> Option<Result<Value, FidelityReason>> {
    let integral = |n: Option<(bool, String)>| n.ok_or(FidelityReason::ScaleExceeded { max: 0 });
    Some(match &spec.kind {
        ScalarKind::Float { bits } => expected_float(text, *bits).map(Value::Float),
        ScalarKind::Decimal => expected_decimal(text, spec.precision, spec.scale)
            .map(|()| Value::Decimal(DecimalValue::parse(text).unwrap())),
        ScalarKind::BigInt => integral(integer_of(text)).map(integer_value),
        ScalarKind::Int { bits, signed } => integral(integer_of(text)).and_then(|n| {
            if as_i128(&n).is_some_and(|m| int_holds(m, *bits, *signed)) {
                Ok(integer_value(n))
            } else {
                Err(FidelityReason::Overflow)
            }
        }),
        _ => return None,
    })
}

/// What `coerce_value` must do with a numeric `value` under a numeric
/// kind (law (i), with the narrowing rule of a 32-bit float kind): `None`
/// when the value or the kind is not numeric, or the value is a non-finite
/// float.
fn expected_value(value: &Value, spec: &ValueSpec) -> Option<Result<Value, FidelityReason>> {
    let text = match value {
        Value::Float(x) if !x.is_finite() => return None,
        Value::Float(x) => {
            if spec.kind == (ScalarKind::Float { bits: 32 }) {
                // The binary32 nearest the shortest text of x is x itself
                // exactly when x is a binary32 value: the text lies within
                // half a binary64 unit of x.
                let narrow = format!("{x:e}").parse::<f32>().unwrap();
                return Some(if f64::from(narrow).to_bits() == x.to_bits() {
                    Ok(value.clone())
                } else if x.abs() > f64::from(f32::MAX) {
                    Err(FidelityReason::Overflow)
                } else {
                    Err(FidelityReason::PrecisionExceeded { max: 9 })
                });
            }
            if spec.kind == (ScalarKind::Float { bits: 64 }) {
                return Some(Ok(value.clone()));
            }
            format!("{x}")
        }
        Value::Int(n) => n.to_string(),
        Value::BigInt(n) => n.to_string(),
        Value::Decimal(d) => d.to_string(),
        _ => return None,
    };
    expected_numeral(&text, spec)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// Law (i) for `coerce` under the float, decimal and integer kinds,
    /// with and without facets, and for `coerce_value` on numeric values.
    #[test]
    fn coercion_has_lexical_fidelity(
        float in float_numerals(),
        exact in exact_numerals(),
        precision in prop::option::of(1_u32..12),
        scale in prop::option::of(0_u32..6),
        int_bits in prop::sample::select(vec![8_u8, 16, 32, 64, 128]),
        signed in any::<bool>(),
        value in values(),
        index in 0..32_usize,
    ) {
        for bits in [32, 64] {
            let spec = ValueSpec::new(ScalarKind::Float { bits });
            let result = coerce(&Lexeme::new(&float, LexClass::Number), &spec);
            assert_float_result(&result, &expected_float(&float, bits))?;
        }
        let kinds = [
            ValueSpec { precision, scale, ..ValueSpec::new(ScalarKind::Decimal) },
            ValueSpec::new(ScalarKind::Decimal),
            ValueSpec::new(ScalarKind::BigInt),
            ValueSpec::new(ScalarKind::Int { bits: int_bits, signed }),
        ];
        for spec in &kinds {
            let result = coerce(&Lexeme::new(&exact, LexClass::Number), spec);
            let expected = expected_numeral(&exact, spec).unwrap();
            match (&result, &expected) {
                (Ok(got), Ok(want)) => prop_assert!(same(got, want), "{exact} as {spec:?}: {got:?}"),
                (Err(e), Err(reason)) => prop_assert_eq!(&e.reason, reason, "{} as {:?}", exact, spec),
                _ => prop_assert!(false, "{exact} as {spec:?}: got {result:?}, expected {expected:?}"),
            }
        }
        let all = specs();
        let spec = &all[index % all.len()];
        if let Some(expected) = expected_value(&value, spec) {
            let result = coerce_value(&value, spec, site());
            match (&result, &expected) {
                (Ok(Value::Float(got)), Ok(Value::Float(want))) => {
                    prop_assert_eq!(got.to_bits(), want.to_bits(), "{:?} as {:?}", value, spec.kind);
                }
                (Ok(got), Ok(want)) => prop_assert!(same(got, want), "{value:?} as {spec:?}: {got:?}"),
                (Err(e), Err(reason)) => prop_assert_eq!(&e.reason, reason, "{:?} as {:?}", value, spec),
                _ => prop_assert!(false, "{value:?} as {spec:?}: got {result:?}, expected {expected:?}"),
            }
        }
    }

    /// Law (ii): coercing a coerced value again returns it unchanged.
    #[test]
    fn coerce_value_is_idempotent(value in values(), index in 0..32_usize) {
        let all = specs();
        let spec = &all[index % all.len()];
        if let Ok(once) = coerce_value(&value, spec, site()) {
            let twice = coerce_value(&once, spec, site());
            prop_assert!(
                twice.as_ref().is_ok_and(|t| same(t, &once)),
                "{value:?} as {:?}: {once:?} then {twice:?}", spec.kind
            );
        }
    }

    /// Law (iii): every value `coerce` returns under a kind is a fixed
    /// point of `coerce_value` under that kind, by canonical bytes.
    #[test]
    fn coerced_values_are_fixed_points(
        text in prop_oneof![texts(), float_numerals(), exact_numerals()],
        class in prop::sample::select(vec![
            LexClass::Number, LexClass::String, LexClass::DateTime, LexClass::Bool,
        ]),
    ) {
        for spec in specs() {
            if let Ok(read) = coerce(&Lexeme::new(&text, class.clone()), &spec) {
                let again = coerce_value(&read, &spec, site());
                prop_assert!(
                    again.as_ref().is_ok_and(|v| same(v, &read)),
                    "{text:?} ({class:?}) as {:?}: {read:?} then {again:?}", spec.kind
                );
            }
        }
    }

    /// Outside the laws' domain values pass through: every value under
    /// `Any`; a non-finite float under a float kind that holds it, sign and
    /// payload included (a 32-bit kind refuses a NaN whose payload uses
    /// the low 29 fraction bits); a value with no scalar lexeme under
    /// every kind.
    #[test]
    fn coerce_value_passes_through_outside_its_domain(
        value in values(),
        nan_or_inf in non_finite(),
        opaque in no_lexeme_values(),
    ) {
        let any = ValueSpec::new(ScalarKind::Any);
        let kept = coerce_value(&value, &any, site()).unwrap();
        prop_assert!(same(&kept, &value));
        let float = Value::Float(nan_or_inf);
        let kept = coerce_value(&float, &ValueSpec::new(ScalarKind::Float { bits: 64 }), site());
        prop_assert!(kept.is_ok_and(|v| same(&v, &float)));
        let narrow = coerce_value(&float, &ValueSpec::new(ScalarKind::Float { bits: 32 }), site());
        if nan_or_inf.to_bits() & ((1 << 29) - 1) == 0 {
            prop_assert!(narrow.is_ok_and(|v| same(&v, &float)));
        } else {
            prop_assert_eq!(narrow.unwrap_err().reason, FidelityReason::NonFinite);
        }
        for spec in specs() {
            let kept = coerce_value(&opaque, &spec, site());
            prop_assert!(kept.is_ok_and(|v| same(&v, &opaque)));
        }
    }

    /// The two routes into a 32-bit float agree wherever both succeed: a
    /// numeral read directly, and the same numeral read as a 64-bit float
    /// and then narrowed; and an integer or decimal value coerced directly
    /// or through a 64-bit float.
    #[test]
    fn float32_routes_agree_on_ok_results(
        float in float_numerals(),
        exact in exact_numerals(),
    ) {
        let f32_ = ValueSpec::new(ScalarKind::Float { bits: 32 });
        let f64_ = ValueSpec::new(ScalarKind::Float { bits: 64 });
        let direct = coerce(&Lexeme::new(&float, LexClass::Number), &f32_);
        let path = coerce(&Lexeme::new(&float, LexClass::Number), &f64_)
            .and_then(|wide| coerce_value(&wide, &f32_, site()));
        if let (Ok(a), Ok(b)) = (&direct, &path) {
            prop_assert!(same(a, b), "{float}: {a:?} and {b:?}");
        }
        let inputs = [
            Value::Decimal(DecimalValue::parse(&exact).unwrap()),
            coerce(&Lexeme::new(&exact, LexClass::Number), &ValueSpec::new(ScalarKind::BigInt))
                .unwrap_or(Value::Null),
        ];
        for input in inputs.iter().filter(|v| **v != Value::Null) {
            for target in [&f32_, &f64_] {
                // An integer or decimal input is coerced as its lexeme.
                let lexeme = match input { Value::Decimal(d) => d.to_string(), other => integer_text(other) };
                prop_assert!(same_result(
                    &coerce_value(input, target, site()),
                    &coerce(&Lexeme::new(&lexeme, LexClass::Number), target),
                ), "{input:?} as {target:?}");
                let direct = coerce_value(input, target, site());
                let path = coerce_value(input, &f64_, site())
                    .and_then(|wide| coerce_value(&wide, target, site()));
                if let (Ok(a), Ok(b)) = (&direct, &path) {
                    prop_assert!(same(a, b), "{input:?}: {a:?} and {b:?}");
                }
            }
        }
    }
}

#[test]
fn coerce_value_refuses_a_float_that_is_not_a_binary32() {
    let f32_ = ValueSpec::new(ScalarKind::Float { bits: 32 });
    for x in [0.1, 1.1, 0.123_456_789, 16_777_217.0, 1e-46] {
        let error = coerce_value(&Value::Float(x), &f32_, site()).unwrap_err();
        assert_eq!(error.reason, FidelityReason::PrecisionExceeded { max: 9 }, "{x}");
        assert_eq!(error.site, site());
    }
    let error = coerce_value(&Value::Float(1e39), &f32_, site()).unwrap_err();
    assert_eq!(error.reason, FidelityReason::Overflow);
    for x in [
        0.100_000_001_490_116_12,
        -0.0,
        3.402_823_466_385_288_6e38,
        1.401_298_464_324_817e-45,
        16_777_216.0,
    ] {
        let kept = coerce_value(&Value::Float(x), &f32_, site()).unwrap();
        let Value::Float(y) = kept else {
            panic!("a float")
        };
        assert_eq!(y.to_bits(), x.to_bits(), "{x}");
    }
    // A decimal or integer keeps the lexeme rule: `0.1` reads as the
    // binary32 nearest it.
    let tenth = Value::Decimal(DecimalValue::parse("0.1").unwrap());
    assert!(same(
        &coerce_value(&tenth, &f32_, site()).unwrap(),
        &Value::Float(f64::from(0.1_f32))
    ));
}

#[test]
fn float32_route_refusals_differ_in_both_directions() {
    let f32_ = ValueSpec::new(ScalarKind::Float { bits: 32 });
    let f64_ = ValueSpec::new(ScalarKind::Float { bits: 64 });
    let direct = |t: &str| coerce(&Lexeme::new(t, LexClass::Number), &f32_);
    let path = |t: &str| {
        coerce(&Lexeme::new(t, LexClass::Number), &f64_)
            .and_then(|wide| coerce_value(&wide, &f32_, site()))
    };
    let tenth32 = Value::Float(f64::from(0.1_f32));
    assert!(direct("0.10000000149011612").is_err());
    assert!(same(&path("0.10000000149011612").unwrap(), &tenth32));
    assert!(same(&direct("0.1").unwrap(), &tenth32));
    assert!(path("0.1").is_err());
    let exact_tenth64 = "0.1000000000000000055511151231257827021181583404541015625";
    assert!(same(
        &coerce(&Lexeme::new(exact_tenth64, LexClass::Number), &f64_).unwrap(),
        &Value::Float(0.1)
    ));
    assert!(path(exact_tenth64).is_err());
    assert!(direct(exact_tenth64).is_err());
}

#[test]
fn a_precision_refusal_says_what_the_kind_cannot_hold() {
    let f32_ = ValueSpec::new(ScalarKind::Float { bits: 32 });
    // "16777217.0" has eight significant digits, so the message cannot
    // claim more than nine.
    let error = coerce(&Lexeme::new("16777217.0", LexClass::Number), &f32_).unwrap_err();
    assert_eq!(
        error.reason.to_string(),
        "not exactly a value of the kind, whose values are written in at most 9 significant digits"
    );
    let money = ValueSpec {
        precision: Some(3),
        ..ValueSpec::new(ScalarKind::Decimal)
    };
    let error = coerce(&Lexeme::new("1.234", LexClass::Number), &money).unwrap_err();
    assert_eq!(
        error.reason.to_string(),
        "not exactly a value of the kind, whose values are written in at most 3 significant digits"
    );
}

#[test]
fn a_non_finite_float_is_kept_only_by_a_float_kind() {
    let float = ValueSpec::new(ScalarKind::Float { bits: 64 });
    let decimal = ValueSpec::new(ScalarKind::Decimal);
    let nan = Value::Float(f64::NAN);
    assert!(same(&coerce_value(&nan, &float, site()).unwrap(), &nan));
    let error = coerce_value(&nan, &decimal, site()).unwrap_err();
    assert_eq!(error.reason, FidelityReason::NonFinite);
    assert_eq!(error.site.vertex.as_ref(), "v");
}

#[test]
fn a_nan_binary32_cannot_hold_is_refused_under_float32() {
    let f32_ = ValueSpec::new(ScalarKind::Float { bits: 32 });
    let f64_ = ValueSpec::new(ScalarKind::Float { bits: 64 });
    let payload = Value::Float(f64::from_bits(0x7ff8_0000_0000_1234));
    let error = coerce_value(&payload, &f32_, site()).unwrap_err();
    assert_eq!(error.reason, FidelityReason::NonFinite);
    assert!(same(&coerce_value(&payload, &f64_, site()).unwrap(), &payload));
    for bits in [
        0x7ff8_0000_0000_0000_u64,
        0xfff8_0000_0000_0000,
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
    ] {
        let kept = Value::Float(f64::from_bits(bits));
        for spec in [&f32_, &f64_] {
            assert!(same(&coerce_value(&kept, spec, site()).unwrap(), &kept), "{bits:x}");
        }
    }
}

/// The decimal digits of an integer value.
fn integer_text(value: &Value) -> String {
    match value {
        Value::Int(n) => n.to_string(),
        Value::BigInt(n) => n.to_string(),
        other => panic!("not an integer: {other:?}"),
    }
}

/// Whether two coercion results agree: equal values by canonical bytes,
/// or refusals for one reason.
fn same_result(
    a: &Result<Value, panproto_inst::ValueFidelityError>,
    b: &Result<Value, panproto_inst::ValueFidelityError>,
) -> bool {
    match (a, b) {
        (Ok(x), Ok(y)) => same(x, y),
        (Err(x), Err(y)) => x.reason == y.reason,
        _ => false,
    }
}
