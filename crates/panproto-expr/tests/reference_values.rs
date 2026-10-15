//! Production values agree exactly with `panproto-reference`'s.
//!
//! Each test runs a production function of `panproto-expr` and its
//! `panproto_reference` counterpart on every case of a stated finite scope,
//! or on generated cases, and requires identical results: the same value, or
//! the same typed refusal. Each exhaustive test prints its bound and counts.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cmp::Ordering;

use panproto_expr::exact::{CivilDate, CivilTime, DateTimeSeparator, Offset, Temporal, ZuluCase};
use panproto_expr::{BuiltinOp, ExprError, Literal, apply_builtin};
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

fn year_width(year: i32) -> usize {
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
            value: i64::from(d.year),
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
fn near(x: f64, steps: i64) -> f64 {
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
        if x.fract().abs() == 0.5 {
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
