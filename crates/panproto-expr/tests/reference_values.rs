//! Production values agree exactly with `panproto-reference`'s.
//!
//! Each test runs a production function of `panproto-expr` and its
//! `panproto_reference` counterpart on every case of a stated finite scope,
//! or on generated cases, and requires identical results: the same value, or
//! the same typed refusal. Each exhaustive test prints its bound and counts.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cmp::Ordering;

use panproto_expr::exact::{
    CivilDate, CivilTime, DateTimeSeparator, DecimalClass, DecimalValue, Offset, Temporal,
    WideInt, ZuluCase,
};
use panproto_expr::{BuiltinOp, ExprError, Literal, apply_builtin};
use panproto_reference::exact as rx;
use panproto_reference::num::{Int, Nat};
use panproto_reference::temporal as rt;
use panproto_reference::value::{self as rv, EvalError, Value as RValue};

// ---------------------------------------------------------------------------
// Conversions into the reference model.
// ---------------------------------------------------------------------------

/// The reference value of a scalar production literal.
fn literal_to_ref(literal: &Literal) -> RValue {
    match literal {
        Literal::Bool(b) => RValue::Bool(*b),
        Literal::Int(n) => RValue::Int(*n),
        Literal::Float(x) => RValue::Float(x.to_bits()),
        Literal::Str(s) => RValue::Str(s.clone()),
        Literal::Bytes(b) => RValue::Bytes(b.clone()),
        Literal::Null => RValue::Null,
        Literal::Temporal(t) => RValue::Temporal(temporal_to_ref(t)),
        Literal::Decimal(d) => RValue::Decimal(decimal_to_ref(d)),
        other => panic!("no scalar reference value for {other:?}"),
    }
}

/// The reference refusal a production evaluation error maps to, for the
/// errors the builtins of these tests return.
fn error_to_ref(error: &ExprError) -> EvalError {
    match error {
        ExprError::FloatNotRepresentable(_) => EvalError::FloatNotRepresentable,
        ExprError::Overflow => EvalError::Overflow,
        ExprError::DivisionByZero => EvalError::DivisionByZero,
        other => panic!("no reference refusal for {other:?}"),
    }
}

/// A production builtin's outcome in the reference model.
fn outcome_to_ref(outcome: &Result<Literal, ExprError>) -> Result<RValue, EvalError> {
    outcome.as_ref().map(literal_to_ref).map_err(error_to_ref)
}

const fn op_to_ref(op: BuiltinOp) -> rv::BuiltinOp {
    match op {
        BuiltinOp::FloatToInt => rv::BuiltinOp::FloatToInt,
        BuiltinOp::Floor => rv::BuiltinOp::Floor,
        BuiltinOp::Ceil => rv::BuiltinOp::Ceil,
        BuiltinOp::Round => rv::BuiltinOp::Round,
        _ => panic!("no reference builtin mapped"),
    }
}

/// The reference natural of a canonical digit string.
fn nat(digits: &str) -> Nat {
    Nat::parse(digits).expect("canonical digits")
}

/// The reference integer of a `WideInt`.
fn int_of(w: &WideInt) -> Int {
    Int::new(w.is_negative(), nat(w.magnitude()))
}

/// The reference decimal of a production decimal: a finite value field
/// for field, an infinity or a NaN with its sign, flag and payload.
fn decimal_to_ref(d: &DecimalValue) -> rx::Decimal {
    match &d.class {
        DecimalClass::Finite => rx::Decimal::Finite(rx::DecimalValue {
            negative: d.negative,
            digits: nat(&d.digits),
            exponent: int_of(&d.exponent),
        }),
        DecimalClass::Infinite => rx::Decimal::Infinite {
            negative: d.negative,
        },
        DecimalClass::NaN {
            signalling,
            payload,
        } => rx::Decimal::NaN {
            negative: d.negative,
            signalling: *signalling,
            payload: nat(payload),
        },
    }
}

fn year_width(year: i64) -> usize {
    year.unsigned_abs().to_string().len().max(4)
}

const fn offset_to_ref(o: Offset) -> rt::Offset {
    match o {
        Offset::Local => rt::Offset::Local,
        Offset::Zulu(ZuluCase::Upper) => rt::Offset::Zulu(rt::ZuluCase::Upper),
        Offset::Zulu(ZuluCase::Lower) => rt::Offset::Zulu(rt::ZuluCase::Lower),
        Offset::Fixed(m) => rt::Offset::Fixed(m, rt::OffsetForm::Extended),
        Offset::NegZero => rt::Offset::NegZero(rt::OffsetForm::Extended),
    }
}

fn date_to_ref(d: CivilDate) -> rt::DateFields {
    rt::DateFields {
        year: Some(rt::Year {
            value: d.year,
            negative_zero: false,
            width: year_width(d.year),
        }),
        month: Some(d.month),
        day: Some(d.day),
    }
}

fn time_to_ref(t: &CivilTime) -> rt::TimeFields {
    rt::TimeFields {
        hour: Some(t.hour),
        minute: Some(t.minute),
        second: Some(t.second),
        fraction: t.fraction.as_str().to_owned(),
    }
}

const fn separator_to_ref(s: DateTimeSeparator) -> rt::Separator {
    match s {
        DateTimeSeparator::Upper => rt::Separator::Upper,
        DateTimeSeparator::Lower => rt::Separator::Lower,
        DateTimeSeparator::Space => rt::Separator::Space,
    }
}

/// The reference temporal of a production temporal.
fn temporal_to_ref(t: &Temporal) -> rt::Temporal {
    let (date, time, separator, offset) = match t {
        Temporal::Date(d) => (Some(date_to_ref(*d)), None, None, Offset::Local),
        Temporal::Time(t, o) => (None, Some(time_to_ref(t)), None, *o),
        Temporal::DateTime(d, t, o, s) => (
            Some(date_to_ref(*d)),
            Some(time_to_ref(t)),
            Some(separator_to_ref(*s)),
            *o,
        ),
    };
    rt::Temporal {
        date,
        time,
        separator,
        offset: offset_to_ref(offset),
        infinity: None,
    }
}

// ---------------------------------------------------------------------------
// Rounding builtins.
// ---------------------------------------------------------------------------

/// `x` moved by `steps` binary64 patterns.
const fn near(x: f64, steps: i64) -> f64 {
    let bits = x.to_bits();
    f64::from_bits(if steps >= 0 {
        bits + steps.unsigned_abs()
    } else {
        bits - steps.unsigned_abs()
    })
}

/// `round` breaks an exact tie to the even integer, as IEEE 754's
/// roundTiesToEven does: every half-integer in `[-64.5, 64.5]`, the largest
/// binary64 half-integers below 2^52 in magnitude, and their neighbours.
#[test]
fn round_breaks_an_exact_tie_to_even() {
    let mut scope: Vec<f64> = (-129..=129).map(|k| f64::from(k) / 2.0).collect();
    for tie in [4_503_599_627_370_495.5, -4_503_599_627_370_495.5] {
        for steps in -2..=2 {
            scope.push(near(tie, steps));
        }
    }
    let mut ties = 0;
    for &x in &scope {
        let production = apply_builtin(BuiltinOp::Round, &[Literal::Float(x)]);
        let reference = rv::apply_builtin(rv::BuiltinOp::Round, &[RValue::Float(x.to_bits())]);
        assert_eq!(outcome_to_ref(&production), reference, "round({x:e})");
        if x.fract().abs().to_bits() == 0.5_f64.to_bits() {
            ties += 1;
        }
    }
    println!(
        "round: {} floats, {ties} exact ties, all agree",
        scope.len()
    );
    // The odd k in [-129, 129] give 130 half-integers; each of the two
    // large ties and its neighbour two patterns below in magnitude
    // (binary64's unit there is 1/2) are ties too.
    assert_eq!(ties, 130 + 4);
}

/// Every float-to-integer builtin returns the exact integer when it lies in
/// `i64`'s range and refuses the float otherwise, as the reference does: the
/// scope holds 2^63 and -2^63 and twice each, with their binary64 neighbours
/// three patterns either way, values near them with fractions, the
/// infinities, a NaN, the extreme finite floats and small values.
#[test]
fn rounding_builtins_agree_with_the_reference_beyond_i64() {
    const TWO_63: f64 = 9_223_372_036_854_775_808.0;
    let mut scope = vec![
        0.0,
        -0.0,
        0.5,
        -0.5,
        1.5,
        -1.5,
        2.5,
        -2.7,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        f64::MAX,
        f64::MIN,
        4_503_599_627_370_495.5,
        -4_503_599_627_370_495.5,
    ];
    for base in [TWO_63, -TWO_63, 2.0 * TWO_63, -2.0 * TWO_63] {
        for steps in -3..=3 {
            scope.push(near(base, steps));
        }
    }
    let ops = [
        BuiltinOp::FloatToInt,
        BuiltinOp::Floor,
        BuiltinOp::Ceil,
        BuiltinOp::Round,
    ];
    for op in ops {
        for &x in &scope {
            let production = apply_builtin(op, &[Literal::Float(x)]);
            let reference = rv::apply_builtin(op_to_ref(op), &[RValue::Float(x.to_bits())]);
            assert_eq!(outcome_to_ref(&production), reference, "{op:?}({x:e})");
        }
    }
    println!(
        "rounding builtins: {} ops by {} floats, all agree",
        ops.len(),
        scope.len()
    );
}

// ---------------------------------------------------------------------------
// Chronological order.
// ---------------------------------------------------------------------------

/// A leap second is the instant between second 59 of its minute and the next
/// minute's second 0: `23:59:60Z` precedes the next day's `00:00:00Z`, with
/// or without fractions, on every zoned and local date-time and time of the
/// scope, as the reference orders them.
#[test]
fn a_leap_second_precedes_the_next_minute() {
    let mut lexemes = Vec::new();
    for (date, next) in [("2016-12-31", "2017-01-01"), ("2024-01-01", "2024-01-02")] {
        for offset in ["Z", "+00:00", "+00:01", "-00:01", ""] {
            for second in ["58", "59", "60"] {
                for fraction in ["", ".0", ".5", ".999"] {
                    lexemes.push(format!("{date}T23:59:{second}{fraction}{offset}"));
                    lexemes.push(format!("23:59:{second}{fraction}{offset}"));
                }
            }
            for fraction in ["", ".0", ".5"] {
                lexemes.push(format!("{next}T00:00:00{fraction}{offset}"));
                lexemes.push(format!("00:00:00{fraction}{offset}"));
                lexemes.push(format!("{date}T23:58:59{fraction}{offset}"));
            }
        }
    }
    lexemes.sort();
    lexemes.dedup();
    let values: Vec<Temporal> = lexemes
        .iter()
        .map(|t| Temporal::parse(t).unwrap_or_else(|e| panic!("{t}: {e}")))
        .collect();
    let mut compared = 0u64;
    for a in &values {
        for b in &values {
            let reference = rt::cmp_chronological(&temporal_to_ref(a), &temporal_to_ref(b));
            assert_eq!(a.cmp_chronological(b), reference, "{a} against {b}");
            compared += 1;
        }
    }
    let leap = Temporal::parse("2016-12-31T23:59:60Z").unwrap();
    let next = Temporal::parse("2017-01-01T00:00:00Z").unwrap();
    assert_eq!(leap.cmp_chronological(&next), Some(Ordering::Less));
    println!(
        "leap seconds: {} lexemes, {compared} ordered pairs, all agree",
        values.len()
    );
    assert_eq!(compared, (values.len() * values.len()) as u64);
}

// ---------------------------------------------------------------------------
// Decimals.
// ---------------------------------------------------------------------------

/// The decimal scope of the special-value laws: every decimal of at most
/// two significant digits (significands 1 to 99) with exponent -2 to 2 and
/// either sign, the zeros and negative zeros of those exponents, both
/// infinities, and the NaNs of each sign and flag with payloads 0 and 1.
fn decimal_scope() -> Vec<DecimalValue> {
    let mut out = Vec::new();
    for exponent in -2_i64..=2 {
        for negative in [false, true] {
            out.push(DecimalValue::new(negative, "0", exponent).unwrap());
            for significand in 1..=99 {
                out.push(DecimalValue::new(negative, significand.to_string(), exponent).unwrap());
            }
        }
    }
    for negative in [false, true] {
        out.push(DecimalValue::infinity(negative));
        for signalling in [false, true] {
            for payload in ["0", "1"] {
                out.push(DecimalValue::nan(negative, signalling, payload));
            }
        }
    }
    out
}

/// The scope's size, counted independently: 5 exponents by 2 signs by 100
/// significands (zero and 1 to 99), 2 infinities and 2 x 2 x 2 NaNs.
const DECIMAL_SCOPE: usize = 5 * 2 * 100 + 2 + 2 * 2 * 2;

/// The production order and compare-total agree with the reference's on
/// every pair of the scope; the order is a total preorder on every triple,
/// compare-total refines it on every pair, and the canonical bytes are the
/// reference's bytes and differ for every two distinct values.
#[test]
fn decimal_orders_agree_with_the_reference() {
    let scope = decimal_scope();
    assert_eq!(scope.len(), DECIMAL_SCOPE);
    let reference: Vec<rx::Decimal> = scope.iter().map(decimal_to_ref).collect();
    let n = scope.len();
    // The production numeric order as a matrix, checked pair by pair.
    let mut order = vec![Ordering::Equal; n * n];
    let mut bytes = std::collections::HashSet::new();
    for (i, a) in scope.iter().enumerate() {
        let a_bytes = a.canonical_bytes();
        assert_eq!(
            Some(a_bytes.clone()),
            panproto_reference::canonical::decimal_bytes(&reference[i]),
            "{a}"
        );
        assert!(bytes.insert(a_bytes), "two values with one encoding: {a}");
        for (j, b) in scope.iter().enumerate() {
            let numeric = a.cmp_numeric(b);
            assert_eq!(numeric, reference[i].cmp_numeric(&reference[j]), "{a} against {b}");
            let total = a.compare_total(b);
            assert_eq!(total, reference[i].compare_total(&reference[j]), "{a} total {b}");
            assert_eq!(total.is_eq(), a == b, "{a} total {b}");
            if numeric.is_lt() {
                assert!(total.is_lt(), "compare-total refines the order: {a} {b}");
            }
            assert_eq!(numeric, b.cmp_numeric(a).reverse(), "{a} against {b}");
            order[i * n + j] = numeric;
        }
    }
    // Transitivity on every triple.
    let mut triples = 0_u64;
    for i in 0..n {
        for j in 0..n {
            let ij = order[i * n + j];
            for k in 0..n {
                let jk = order[j * n + k];
                let ik = order[i * n + k];
                if ij.is_le() && jk.is_le() {
                    assert!(ik.is_le(), "{} {} {}", scope[i], scope[j], scope[k]);
                    if ij.is_lt() || jk.is_lt() {
                        assert!(ik.is_lt(), "{} {} {}", scope[i], scope[j], scope[k]);
                    }
                }
                triples += 1;
            }
        }
    }
    println!(
        "decimal orders: bound {n} decimals (significands 0 to 99, exponents -2 to 2, both signs, \
         2 infinities, 8 NaNs); {} pairs, {triples} triples",
        n * n
    );
    assert_eq!(triples, (n * n * n) as u64);
}

/// Every special spelling of the numeric string syntax, every finite
/// numeral of the values page's scope and a set of non-numerals read as
/// the reference reads them, and display back as the reference writes them.
#[test]
fn decimal_lexemes_read_as_the_reference_reads_them() {
    let mut lexemes: Vec<String> = panproto_reference::enumerate::values::numerals();
    for sign in ["", "+", "-"] {
        for body in [
            "NaN", "nan", "NAN", "sNaN", "snan", "SNAN", "NaN0", "NaN1", "NaN007", "sNaN12",
            "Inf", "inf", "INF", "Infinity", "infinity", "INFINITY", "0", "0.00", "0e5", "0E+3",
            "1.50", ".5", "5.", "Infinit", "NaN.5", "sNaN-1", "nanx", ".nan", "in", "",
        ] {
            lexemes.push(format!("{sign}{body}"));
        }
    }
    let mut read = 0;
    for text in &lexemes {
        let production = DecimalValue::parse(text).ok();
        let reference = rx::parse_decimal(text);
        assert_eq!(production.as_ref().map(decimal_to_ref), reference, "{text:?}");
        if let Some(value) = production {
            let shown = value.to_string();
            assert_eq!(DecimalValue::parse(&shown).ok().as_ref(), Some(&value), "{text:?}");
            assert_eq!(
                apply_builtin(BuiltinOp::ToString, &[Literal::Decimal(value.clone())])
                    .ok()
                    .map(|l| literal_to_ref(&l)),
                rv::apply_builtin(rv::BuiltinOp::ToString, &[RValue::Decimal(decimal_to_ref(&value))])
                    .ok(),
                "{text:?}"
            );
            read += 1;
        }
    }
    println!("decimal lexemes: {} lexemes, {read} read", lexemes.len());
}
