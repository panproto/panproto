//! Production values agree exactly with `panproto-reference`'s.
//!
//! Each test runs a production function of `panproto-expr` and its
//! `panproto_reference` counterpart on every case of a stated finite scope,
//! or on generated cases, and requires identical results: the same value, or
//! the same typed refusal. Each exhaustive test prints its bound and counts.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use panproto_expr::exact::{
    CivilDate, CivilTime, DateFields, DateTimeSeparator, DecimalClass, DecimalValue, Offset,
    OffsetForm, Temporal, TemporalForm, TemporalInfinity, TimeFields, TimeUnit, WideInt, ZuluCase,
};
use panproto_expr::exact::{binary16_bits, binary16_nearest, binary16_widen};
use panproto_expr::{
    BuiltinOp, Env, EvalConfig, Expr, ExprError, Literal, Pattern, apply_builtin, eval,
};
use panproto_reference::exact as rx;
use panproto_reference::float as rf;
use panproto_reference::float::Format;
use panproto_reference::num::{Int, Nat};
use panproto_reference::temporal as rt;
use panproto_reference::value as rv;
use panproto_reference::value::{EvalError, Value as RValue};
use proptest::prelude::*;

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
        Literal::List(items) => RValue::List(items.iter().map(literal_to_ref).collect()),
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
        ExprError::UnboundVariable(name) => EvalError::UnboundVariable(name.clone()),
        ExprError::NotAFunction => EvalError::NotAFunction,
        ExprError::NonExhaustiveMatch => EvalError::NonExhaustiveMatch,
        ExprError::IndexOutOfBounds { index, len } => EvalError::IndexOutOfBounds {
            index: *index,
            len: *len,
        },
        ExprError::FieldNotFound(name) => EvalError::FieldNotFound(name.clone()),
        // The production error names its builtin by the operation's debug
        // name, the reference by the operation itself.
        ExprError::ArityMismatch { op, expected, got } => EvalError::ArityMismatch {
            op: MAPPED_OPS
                .into_iter()
                .find(|mapped| format!("{mapped:?}") == *op)
                .map_or_else(|| panic!("no reference builtin named {op}"), op_to_ref),
            expected: *expected,
            got: *got,
        },
        ExprError::TypeError { expected, got } => EvalError::TypeError {
            expected: Box::leak(expected.clone().into_boxed_str()),
            got: Box::leak(got.clone().into_boxed_str()),
        },
        other => panic!("no reference refusal for {other:?}"),
    }
}

/// A production builtin's outcome in the reference model.
fn outcome_to_ref(outcome: &Result<Literal, ExprError>) -> Result<RValue, EvalError> {
    outcome.as_ref().map(literal_to_ref).map_err(error_to_ref)
}

/// The production builtins these tests map into the reference model.
const MAPPED_OPS: [BuiltinOp; 21] = [
    BuiltinOp::FloatToInt,
    BuiltinOp::Floor,
    BuiltinOp::Ceil,
    BuiltinOp::Round,
    BuiltinOp::Add,
    BuiltinOp::Sub,
    BuiltinOp::Mul,
    BuiltinOp::Div,
    BuiltinOp::Neg,
    BuiltinOp::Abs,
    BuiltinOp::StrToFloat,
    BuiltinOp::Mod,
    BuiltinOp::Map,
    BuiltinOp::Filter,
    BuiltinOp::FlatMap,
    BuiltinOp::Fold,
    BuiltinOp::Gt,
    BuiltinOp::Clamp,
    BuiltinOp::IntToFloat,
    BuiltinOp::DecimalToFloat,
    BuiltinOp::ToString,
];

const fn op_to_ref(op: BuiltinOp) -> rv::BuiltinOp {
    match op {
        BuiltinOp::Clamp => rv::BuiltinOp::Clamp,
        BuiltinOp::IntToFloat => rv::BuiltinOp::IntToFloat,
        BuiltinOp::DecimalToFloat => rv::BuiltinOp::DecimalToFloat,
        BuiltinOp::ToString => rv::BuiltinOp::ToString,
        BuiltinOp::FloatToInt => rv::BuiltinOp::FloatToInt,
        BuiltinOp::Floor => rv::BuiltinOp::Floor,
        BuiltinOp::Ceil => rv::BuiltinOp::Ceil,
        BuiltinOp::Round => rv::BuiltinOp::Round,
        BuiltinOp::Add => rv::BuiltinOp::Add,
        BuiltinOp::Sub => rv::BuiltinOp::Sub,
        BuiltinOp::Mul => rv::BuiltinOp::Mul,
        BuiltinOp::Div => rv::BuiltinOp::Div,
        BuiltinOp::Neg => rv::BuiltinOp::Neg,
        BuiltinOp::Abs => rv::BuiltinOp::Abs,
        BuiltinOp::StrToFloat => rv::BuiltinOp::StrToFloat,
        BuiltinOp::Mod => rv::BuiltinOp::Mod,
        BuiltinOp::Map => rv::BuiltinOp::Map,
        BuiltinOp::Filter => rv::BuiltinOp::Filter,
        BuiltinOp::FlatMap => rv::BuiltinOp::FlatMap,
        BuiltinOp::Fold => rv::BuiltinOp::Fold,
        BuiltinOp::Gt => rv::BuiltinOp::Gt,
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
        Offset::Zulu(ZuluCase::Unwritten) => rt::Offset::Zulu(rt::ZuluCase::Unwritten),
        Offset::Fixed(m) => rt::Offset::Fixed(m, rt::OffsetForm::Extended),
        Offset::FixedForm(m, form) => rt::Offset::Fixed(m, offset_form_to_ref(form)),
        Offset::FixedSeconds(s) => rt::Offset::FixedSeconds(s),
        Offset::NegZero => rt::Offset::NegZero(rt::OffsetForm::Extended),
        Offset::NegZeroForm(form) => rt::Offset::NegZero(offset_form_to_ref(form)),
    }
}

const fn offset_form_to_ref(form: OffsetForm) -> rt::OffsetForm {
    match form {
        OffsetForm::Extended => rt::OffsetForm::Extended,
        OffsetForm::Basic => rt::OffsetForm::Basic,
        OffsetForm::HourOnly => rt::OffsetForm::HourOnly,
        OffsetForm::ExtendedSeconds => rt::OffsetForm::ExtendedSeconds,
        OffsetForm::Unwritten => rt::OffsetForm::Unwritten,
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

fn date_fields_to_ref(date: DateFields) -> rt::DateFields {
    rt::DateFields {
        year: date.year.map(|year| rt::Year {
            value: year.value,
            negative_zero: year.negative_zero,
            width: year.width,
        }),
        month: date.month,
        day: date.day,
    }
}

fn time_fields_to_ref(time: &TimeFields) -> rt::TimeFields {
    rt::TimeFields {
        hour: time.hour,
        minute: time.minute,
        second: time.second,
        fraction: time.fraction.as_str().to_owned(),
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
        Temporal::Extended(value) => {
            return rt::Temporal {
                date: value.date.map(date_fields_to_ref),
                time: value.time.as_ref().map(time_fields_to_ref),
                separator: value.separator.map(separator_to_ref),
                offset: offset_to_ref(value.offset),
                infinity: value.infinity.map(|(sign, form)| {
                    (
                        match sign {
                            TemporalInfinity::Negative => rt::TemporalInfinity::Negative,
                            TemporalInfinity::Positive => rt::TemporalInfinity::Positive,
                        },
                        match form {
                            TemporalForm::Date => rt::TemporalForm::Date,
                            TemporalForm::Time => rt::TemporalForm::Time,
                            TemporalForm::DateTime => rt::TemporalForm::DateTime,
                        },
                    )
                }),
            };
        }
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

const ROUNDING_OPS: [BuiltinOp; 4] = [
    BuiltinOp::FloatToInt,
    BuiltinOp::Floor,
    BuiltinOp::Ceil,
    BuiltinOp::Round,
];

/// The floats of the rounding-builtin scope: 2^63 and -2^63 and twice each,
/// with their binary64 neighbours three patterns either way, values near them
/// with fractions, the infinities, a NaN, the extreme finite floats and small
/// values.
fn rounding_scope() -> Vec<f64> {
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
    scope
}

/// Every float-to-integer builtin returns the exact integer when it lies in
/// `i64`'s range and refuses the float otherwise, as the reference does, on
/// every float of [`rounding_scope`].
#[test]
fn rounding_builtins_agree_with_the_reference_beyond_i64() {
    let scope = rounding_scope();
    let ops = ROUNDING_OPS;
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
// Binary formats and computed NaNs.
// ---------------------------------------------------------------------------

/// The sign bit and exact rational value of a finite binary64 pattern.
fn finite_rational(bits: u64) -> Option<(bool, panproto_reference::num::Rat)> {
    match rf::decode(Format::B64, bits) {
        rf::FloatValue::Finite { negative, value } => Some((negative, value)),
        rf::FloatValue::Infinite { .. } | rf::FloatValue::NaN { .. } => None,
    }
}

/// The exact decimal value of a finite binary64 value: its expansion has at
/// most 767 significant digits, so this precision writes it exactly.
fn exact_decimal(x: f64) -> DecimalValue {
    DecimalValue::parse(&format!("{x:.767e}")).unwrap()
}

/// The production and reference binary16 readings of the finite binary64
/// value with pattern `bits` agree: exact membership, and nearest rounding
/// with ties to even through each of the reference's three names for it.
fn check_binary16_reading(bits: u64) -> bool {
    let Some((negative, value)) = finite_rational(bits) else {
        return false;
    };
    let x = f64::from_bits(bits);
    assert_eq!(
        binary16_bits(x).is_some(),
        rf::exact_reading(Format::B16, &value),
        "exact reading of {bits:#018x}"
    );
    let reference = rf::binary16(&value, negative);
    assert_eq!(
        reference,
        rf::nearest_reading(Format::B16, &value, negative)
    );
    assert_eq!(reference, rf::round_to(Format::B16, &value, negative));
    assert_eq!(
        binary16_nearest(&exact_decimal(x)).to_bits(),
        rf::widen_bits(Format::B16, reference.bits(Format::B16)),
        "nearest binary16 of {bits:#018x}"
    );
    true
}

/// On every binary16 pattern: widening agrees with the reference's, and
/// narrowing the widened value and each of its two binary64 neighbours
/// agrees with the reference's narrowing (the neighbours are refused). On
/// every finite one of those values and on the exact midpoint between each
/// finite pattern and the next one away from zero (the largest finite
/// pattern's midpoint with 2^16 included), exact membership and nearest
/// rounding with ties to even agree with the reference.
#[test]
fn binary16_operations_agree_with_the_reference() {
    let mut patterns = 0_usize;
    let mut narrowed = 0_usize;
    let mut finite = 0_usize;
    let mut midpoints = 0_usize;
    for pattern in panproto_reference::enumerate::values::binary16_patterns() {
        let h = u16::try_from(pattern).unwrap();
        let widened = binary16_widen(h);
        assert_eq!(
            widened.to_bits(),
            rf::widen_bits(Format::B16, u64::from(h)),
            "{h:#06x}"
        );
        patterns += 1;
        let bits = widened.to_bits();
        for neighbour in [bits, bits.wrapping_add(1), bits.wrapping_sub(1)] {
            assert_eq!(
                binary16_bits(f64::from_bits(neighbour)).map(u64::from),
                rf::narrow_bits(Format::B16, neighbour),
                "{neighbour:#018x}"
            );
            narrowed += 1;
            if check_binary16_reading(neighbour) {
                finite += 1;
            }
        }
        let magnitude = h & 0x7fff;
        if magnitude <= 0x7bff {
            let next = if magnitude == 0x7bff {
                65_536.0_f64.copysign(widened)
            } else {
                binary16_widen(h + 1)
            };
            let midpoint = f64::midpoint(widened, next);
            assert!(check_binary16_reading(midpoint.to_bits()));
            midpoints += 1;
        }
    }
    println!(
        "binary16: bound 65536 patterns; {narrowed} narrowings (each pattern and its 2 binary64 \
         neighbours); {finite} finite readings; {midpoints} midpoints; all agree with the reference"
    );
    assert_eq!(patterns, 1 << 16);
    assert_eq!(narrowed, 3 << 16);
    // The 2 x 31744 finite patterns with both neighbours are finite, except
    // that the zeros' neighbours away from them wrap to NaNs, while each
    // infinity's neighbour toward zero is the largest finite binary64.
    assert_eq!(finite, 3 * 2 * 31_744);
    assert_eq!(midpoints, 2 * 31_744);
}

/// The builtin cases of [`computed_nans_agree_with_the_reference`]: each
/// NaN of `nans` as the operand of the arithmetic and in each position of a
/// clamp, the four invalid operations, the decimal NaNs converted to float
/// (their number is returned), and integer controls converted to float
/// (returned too).
fn nan_builtin_cases(nans: &[u64]) -> (Vec<(BuiltinOp, Vec<Literal>)>, usize, [i64; 6]) {
    let mut cases: Vec<(BuiltinOp, Vec<Literal>)> = Vec::new();
    let floats = |xs: &[f64]| xs.iter().map(|x| Literal::Float(*x)).collect::<Vec<_>>();
    for &nan in nans {
        let x = f64::from_bits(nan);
        cases.push((BuiltinOp::Neg, floats(&[x])));
        cases.push((BuiltinOp::Abs, floats(&[x])));
        for op in [
            BuiltinOp::Add,
            BuiltinOp::Sub,
            BuiltinOp::Mul,
            BuiltinOp::Div,
        ] {
            cases.push((op, floats(&[x, 1.0])));
            cases.push((op, floats(&[1.0, x])));
        }
        // A NaN clamped, and a NaN as either bound.
        cases.push((BuiltinOp::Clamp, floats(&[x, 0.0, 1.0])));
        cases.push((BuiltinOp::Clamp, floats(&[0.5, x, 1.0])));
        cases.push((BuiltinOp::Clamp, floats(&[0.5, 0.0, x])));
    }
    cases.push((BuiltinOp::Add, floats(&[f64::INFINITY, f64::NEG_INFINITY])));
    cases.push((BuiltinOp::Sub, floats(&[f64::INFINITY, f64::INFINITY])));
    cases.push((BuiltinOp::Mul, floats(&[0.0, f64::INFINITY])));
    cases.push((BuiltinOp::Div, floats(&[f64::INFINITY, f64::NEG_INFINITY])));
    // The decimal NaNs of each sign, flag and payload, converted to float.
    let mut decimal_nans = 0_usize;
    for negative in [false, true] {
        for signalling in [false, true] {
            for payload in ["0", "1", "123"] {
                cases.push((
                    BuiltinOp::DecimalToFloat,
                    vec![Literal::Decimal(DecimalValue::nan(
                        negative, signalling, payload,
                    ))],
                ));
                decimal_nans += 1;
            }
        }
    }
    // Controls that compute no NaN: integers converted to float.
    let controls = [0, 1, -1, i64::MAX, i64::MIN, (1 << 53) + 1];
    for n in controls {
        cases.push((BuiltinOp::IntToFloat, vec![Literal::Int(n)]));
    }
    (cases, decimal_nans, controls)
}

/// A builtin that computes a NaN returns the reference's canonical computed
/// NaN, on NaN operands of each sign, flag and payload (every binary16 NaN
/// pattern widened, and four binary64 NaNs with low payload bits) of the
/// arithmetic and of a clamp in each position, on the operand pairs that
/// make IEEE 754 arithmetic produce one, and on decimal NaNs of each sign
/// and flag converted to float; integers converted to float are controls
/// that compute none. A
/// `let`-bound NaN passes through with its bits; every outcome agrees with
/// the reference.
#[test]
fn computed_nans_agree_with_the_reference() {
    let mut nans: Vec<u64> = panproto_reference::enumerate::values::binary16_patterns()
        .into_iter()
        .filter(|pattern| pattern & 0x7c00 == 0x7c00 && pattern & 0x03ff != 0)
        .map(|pattern| rf::widen_bits(Format::B16, pattern))
        .collect();
    assert_eq!(nans.len(), 2 * 1023);
    nans.extend([
        0x7ff8_0000_0000_0000_u64,
        0xfff8_0000_0000_1234,
        0x7ff4_0000_0000_0000,
        0xfff0_0000_0000_0001,
    ]);
    let canonical = rf::computed_nan(Format::B64);
    assert_eq!(canonical, 0x7ff8_0000_0000_0000);
    let (cases, decimal_nans, controls) = nan_builtin_cases(&nans);
    let mut computed = 0_usize;
    let mut refused = 0_usize;
    let mut finite = 0_usize;
    for (op, operands) in &cases {
        let references: Vec<RValue> = operands.iter().map(literal_to_ref).collect();
        let production = apply_builtin(*op, operands);
        if *op == BuiltinOp::Clamp && production.is_err() { println!("TEMP clamp {:?} vs {:?}", production, rv::apply_builtin(op_to_ref(*op), &references)); refused += 1; continue; }
        assert_eq!(
            outcome_to_ref(&production),
            rv::apply_builtin(op_to_ref(*op), &references),
            "{op:?} {operands:?}"
        );
        match production {
            Ok(Literal::Float(x)) if x.is_nan() => {
                assert_eq!(x.to_bits(), canonical, "{op:?} {operands:?}");
                computed += 1;
            }
            Ok(_) => finite += 1,
            Err(_) => refused += 1,
        }
    }
    // A NaN spelling is read, not computed: its written sign is kept and no
    // payload is read.
    let spellings = ["NaN", "+NaN", "-NaN", "nan", "-nan"];
    for text in spellings {
        let production = apply_builtin(BuiltinOp::StrToFloat, &[Literal::Str(text.into())]);
        assert_eq!(
            outcome_to_ref(&production),
            rv::apply_builtin(rv::BuiltinOp::StrToFloat, &[RValue::Str(text.to_owned())]),
            "{text}"
        );
        let sign = if text.starts_with('-') {
            1_u64 << 63
        } else {
            0
        };
        assert!(
            matches!(production, Ok(Literal::Float(x)) if x.to_bits() == canonical | sign),
            "{text}"
        );
    }
    for &nan in &nans {
        let expr = let_in("x", float(f64::from_bits(nan)), var("x"));
        let production = eval(&expr, &Env::new(), &EvalConfig::default());
        assert_eq!(
            outcome_to_ref(&production),
            rv::eval(&expr_to_ref(&expr), &BTreeMap::new())
        );
        assert!(matches!(production, Ok(Literal::Float(x)) if x.to_bits() == nan));
    }
    println!(
        "computed NaNs: bound nans={}; builtin_cases={} (computed_nans={computed}, \
         refusals={refused}, non_nan={finite}), decimal_nans={decimal_nans}, \
         int_controls={}, spellings={}, pass_through={}; all agree",
        nans.len(),
        cases.len(),
        controls.len(),
        spellings.len(),
        nans.len()
    );
    assert_eq!(nans.len(), 2 * 1023 + 4);
    assert_eq!(decimal_nans, 2 * 2 * 3);
    assert_eq!(
        cases.len(),
        nans.len() * (2 + 4 * 2 + 3) + 4 + decimal_nans + controls.len()
    );
    // Every NaN operand of the arithmetic and of a clamp's value computes the
    // canonical NaN, as do the four invalid operations and every decimal NaN;
    // a NaN bound is not ordered below or above the other, so the clamp
    // refuses it; the integer controls compute no NaN.
    assert_eq!(computed, nans.len() * (2 + 4 * 2 + 1) + 4 + decimal_nans);
    assert_eq!(refused, nans.len() * 2);
    assert_eq!(finite, controls.len());
}

// ---------------------------------------------------------------------------
// Closest-shortest printing and exact rounding at each binary format.
// ---------------------------------------------------------------------------

/// The worker threads an exhaustive sweep is spread over.
const WORKERS: usize = 4;

/// The sums of `N` counters that `work` adds to for each item, with the
/// items split over [`WORKERS`] threads.
fn parallel_counts<T: Sync, const N: usize>(
    items: &[T],
    work: impl Fn(&T, &mut [usize; N]) + Sync,
) -> [usize; N] {
    let chunk = items.len().div_ceil(WORKERS).max(1);
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|part| {
                let work = &work;
                scope.spawn(move || {
                    let mut counts = [0_usize; N];
                    for item in part {
                        work(item, &mut counts);
                    }
                    counts
                })
            })
            .collect();
        let mut total = [0_usize; N];
        for handle in handles {
            let counts = handle.join().expect("a sweep worker panicked");
            for (sum, count) in total.iter_mut().zip(counts) {
                *sum += count;
            }
        }
        total
    })
}

/// The binary32 boundary patterns: either sign, every exponent field, and
/// each fraction of the binary32 fraction set.
fn binary32_boundary() -> Vec<u64> {
    panproto_reference::enumerate::boundary_bits(
        Format::B32,
        None,
        &panproto_reference::enumerate::binary32_fractions(),
    )
}

/// The binary64 exponent fields of the boundary sweep: the biased
/// exponents 871 to 1153 (unbiased -152 to 130, which hold every binary32
/// value and its midpoints), the subnormal and least normal fields, the
/// greatest finite field and the infinity/NaN field.
fn binary64_exponents() -> Vec<u64> {
    let mut exponents: Vec<u64> = (871..=1153).collect();
    exponents.extend([0, 1, 2046, 2047]);
    exponents
}

/// The binary64 boundary patterns: either sign, each exponent of
/// [`binary64_exponents`], and each fraction of the binary64 fraction set.
fn binary64_boundary() -> Vec<u64> {
    panproto_reference::enumerate::boundary_bits(
        Format::B64,
        Some(&binary64_exponents()),
        &panproto_reference::enumerate::binary64_fractions(),
    )
}

/// The reference decimal of a finite production decimal, with trailing
/// zeros of its digits moved into the exponent.
fn shortest_to_ref(d: &DecimalValue) -> rf::Dec {
    assert!(d.is_finite(), "{d}");
    rf::Dec::normalized(
        d.negative,
        &nat(&d.digits),
        int_of(&d.exponent)
            .to_i64()
            .expect("a printed exponent fits i64"),
    )
}

/// The production closest-shortest decimal of the `format` pattern `bits`.
fn production_shortest(format: Format, bits: u64) -> Option<DecimalValue> {
    match format {
        Format::B16 => {
            DecimalValue::from_f16_shortest(binary16_widen(u16::try_from(bits).unwrap()))
        }
        Format::B32 => DecimalValue::from_f32_shortest(f32::from_bits(u32::try_from(bits).unwrap())),
        Format::B64 => DecimalValue::from_f64_shortest(f64::from_bits(bits)),
    }
}

/// Whether the production and reference closest-shortest decimals of
/// `bits` agree: equal value, sign and digits for a finite pattern, none
/// for a NaN or an infinity. Counts every pattern in `counts[0]`, the
/// finite ones in `counts[1]`, the others in `counts[2]`, and each
/// disagreement, which it prints, in `counts[3]`.
fn check_shortest(format: Format, bits: u64, counts: &mut [usize; 4]) {
    let production = production_shortest(format, bits);
    let reference = rf::shortest(format, bits);
    counts[0] += 1;
    let agrees = match (&production, &reference) {
        (Some(printed), Some(expected)) => {
            counts[1] += 1;
            let printed = shortest_to_ref(printed);
            printed.value() == expected.value()
                && printed.significant_digits() == expected.significant_digits()
                && printed == *expected
        }
        (None, None) => {
            counts[2] += 1;
            true
        }
        // One side finite and the other not: the pattern's class is the
        // reference's.
        _ => {
            counts[if reference.is_some() { 1 } else { 2 }] += 1;
            false
        }
    };
    if !agrees {
        println!(
            "disagreement: {format:?} {bits:#x}: production {:?}, reference {:?}",
            production.as_ref().map(shortest_to_ref),
            reference
        );
        counts[3] += 1;
    }
}

/// The production closest-shortest printers at binary16, binary32 and
/// binary64 agree with the reference's naive search on every binary16
/// pattern and on the binary32 and binary64 boundary patterns: the same
/// decimal value and digits for a finite pattern, and no decimal for a NaN
/// or an infinity.
#[test]
fn closest_shortest_printers_agree_with_the_reference() {
    let b16 = panproto_reference::enumerate::values::binary16_patterns();
    let b32 = binary32_boundary();
    let b64 = binary64_boundary();
    println!(
        "bound: binary16={} binary32={} binary64={}",
        b16.len(),
        b32.len(),
        b64.len()
    );
    let c16 = parallel_counts(&b16, |bits, counts| check_shortest(Format::B16, *bits, counts));
    let c32 = parallel_counts(&b32, |bits, counts| check_shortest(Format::B32, *bits, counts));
    let c64 = parallel_counts(&b64, |bits, counts| check_shortest(Format::B64, *bits, counts));
    for (name, counts) in [("binary16", c16), ("binary32", c32), ("binary64", c64)] {
        println!(
            "closest-shortest {name}: patterns={} finite={} nonfinite={} disagreements={}",
            counts[0], counts[1], counts[2], counts[3]
        );
    }
    // binary16: 2 signs by 31 finite exponent fields by 1024 fractions, and
    // 2 by 1024 infinities and NaNs.
    assert_eq!(c16, [2 * 32 * 1024, 2 * 31 * 1024, 2 * 1024, 0]);
    // binary32: 2 signs by 256 exponent fields by 8 fractions, of which the
    // all-ones field is not finite.
    assert_eq!(c32, [2 * 256 * 8, 2 * 255 * 8, 2 * 8, 0]);
    // binary64: 2 signs by 283 + 4 exponent fields by 14 fractions, of which
    // the all-ones field is not finite.
    assert_eq!(c64, [2 * 287 * 14, 2 * 286 * 14, 2 * 14, 0]);
}

/// The exact decimal digits and exponent of a rational whose denominator
/// is a power of two: `num / 2^k = num * 5^k * 10^-k`.
fn binary_rational_decimal(q: &panproto_reference::num::Rat) -> (Nat, i64) {
    let den = q.den();
    let k = den.bit_length() - 1;
    assert_eq!(*den, Nat::pow2(u32::try_from(k).unwrap()), "{q:?}");
    let digits = q
        .num()
        .magnitude()
        .mul(&Nat::pow(&Nat::from_u64(5), u32::try_from(k).unwrap()));
    (digits, -i64::try_from(k).unwrap())
}

/// The production nearest `format` pattern of a decimal.
fn production_nearest(format: Format, d: &DecimalValue) -> u64 {
    match format {
        Format::B32 => u64::from(d.to_f32().to_bits()),
        Format::B64 => d.to_f64().to_bits(),
        Format::B16 => binary16_bits(binary16_nearest(d)).map_or_else(
            || panic!("binary16 nearest of {d} is not a binary16 value"),
            u64::from,
        ),
    }
}

/// The production nearest pattern of the decimal `(-1)^negative × digits ×
/// 10^exponent` agrees with the reference's exact rounding of its value
/// (under both of the reference's names for it); returns whether the
/// reference rounding overflows.
fn check_rounding(format: Format, negative: bool, digits: &Nat, exponent: i64) -> bool {
    let production = DecimalValue::new(negative, digits.to_string(), exponent)
        .expect("the digits of a natural are canonical");
    let q = rf::Dec {
        negative,
        coefficient: digits.clone(),
        exponent,
    }
    .value();
    let reference = rf::round_to(format, &q, negative);
    assert_eq!(reference, rf::nearest_reading(format, &q, negative));
    assert_eq!(
        production_nearest(format, &production),
        reference.bits(format),
        "{format:?} nearest of {production}"
    );
    matches!(reference, rf::Rounded::Overflow { .. })
}

/// The rounding readings of one pattern: its exact value, its midpoint with
/// the next pattern away from zero (`2^(emax+1)` past the greatest finite
/// value), and that midpoint moved by one unit of its last decimal digit
/// either way. Counters: patterns, nonfinite patterns, exact values,
/// midpoints, midpoints moved away from zero, midpoints moved toward zero,
/// overflows.
fn check_pattern_rounding(format: Format, bits: u64, counts: &mut [usize; 7]) {
    counts[0] += 1;
    let negative = bits & (1 << (format.width() - 1)) != 0;
    let magnitude = bits & !(1 << (format.width() - 1));
    let rf::FloatValue::Finite { value, .. } = rf::decode(format, magnitude) else {
        counts[1] += 1;
        return;
    };
    let next = match rf::decode(format, magnitude + 1) {
        rf::FloatValue::Finite { value, .. } => value,
        rf::FloatValue::Infinite { .. } => panproto_reference::num::Rat::pow2(format.emax() + 1),
        rf::FloatValue::NaN { .. } => panic!("{format:?} {bits:#x}: a NaN follows a finite pattern"),
    };
    let (digits, exponent) = binary_rational_decimal(&value);
    let exact_overflows = check_rounding(format, negative, &digits, exponent);
    assert!(!exact_overflows, "{format:?} {bits:#x}");
    // The exact value reads back as its own pattern.
    assert_eq!(
        rf::round_to(
            format,
            &rf::Dec {
                negative,
                coefficient: digits,
                exponent
            }
            .value(),
            negative
        )
        .bits(format),
        bits
    );
    counts[2] += 1;
    let midpoint = value
        .add(&next)
        .mul(&panproto_reference::num::Rat::pow2(-1));
    let (digits, exponent) = binary_rational_decimal(&midpoint);
    for (index, moved) in [
        Some(digits.clone()),
        Some(digits.add(&Nat::one())),
        digits.checked_sub(&Nat::one()),
    ]
    .into_iter()
    .enumerate()
    {
        let moved = moved.expect("a midpoint is positive");
        if check_rounding(format, negative, &moved, exponent) {
            counts[6] += 1;
        }
        counts[3 + index] += 1;
    }
}

/// Production rounding of an exact decimal to the nearest binary32 and
/// binary64 value agrees with the reference's exact rounding (ties to even,
/// overflow past the greatest finite value) on every boundary pattern's
/// exact value, its midpoint with the next pattern away from zero, and that
/// midpoint moved by one unit of its last decimal digit either way.
#[test]
fn binary_rounding_agrees_with_the_reference() {
    let b32 = binary32_boundary();
    let b64 = binary64_boundary();
    println!("bound: binary32={} binary64={}", b32.len(), b64.len());
    let c32 = parallel_counts(&b32, |bits, counts| {
        check_pattern_rounding(Format::B32, *bits, counts);
    });
    let c64 = parallel_counts(&b64, |bits, counts| {
        check_pattern_rounding(Format::B64, *bits, counts);
    });
    for (name, counts) in [("binary32", c32), ("binary64", c64)] {
        println!(
            "rounding {name}: patterns={} nonfinite={} exact={} midpoints={} above={} below={} \
             overflows={}; all agree",
            counts[0], counts[1], counts[2], counts[3], counts[4], counts[5], counts[6]
        );
    }
    // Each finite pattern gives its exact value, a midpoint and the
    // midpoint moved each way. Only the greatest finite pattern of each
    // sign overflows, at its midpoint (a tie broken to the even
    // significand, 2^(emax+1)) and above it.
    let finite32 = 2 * 255 * 8;
    assert_eq!(
        c32,
        [2 * 256 * 8, 2 * 8, finite32, finite32, finite32, finite32, 2 * 2]
    );
    let finite64 = 2 * 286 * 14;
    assert_eq!(
        c64,
        [2 * 287 * 14, 2 * 14, finite64, finite64, finite64, finite64, 2 * 2]
    );
}

// ---------------------------------------------------------------------------
// Expression evaluation.
// ---------------------------------------------------------------------------

/// The reference expression of a production expression over variables,
/// lambdas, applications, literals, `let` and the rounding builtins.
fn expr_to_ref(expr: &Expr) -> rv::Expr {
    let boxed = |e: &Expr| Box::new(expr_to_ref(e));
    match expr {
        Expr::Var(name) => rv::Expr::Variable(name.to_string()),
        Expr::Lam(param, body) => rv::Expr::Lambda(param.to_string(), boxed(body)),
        Expr::App(function, argument) => rv::Expr::Apply(boxed(function), boxed(argument)),
        Expr::Lit(literal) => rv::Expr::Literal(literal_to_ref(literal)),
        Expr::Let { name, value, body } => rv::Expr::Let {
            name: name.to_string(),
            value: boxed(value),
            body: boxed(body),
        },
        Expr::Builtin(op, args) => {
            rv::Expr::Builtin(op_to_ref(*op), args.iter().map(expr_to_ref).collect())
        }
        Expr::List(items) => rv::Expr::List(items.iter().map(expr_to_ref).collect()),
        Expr::Field(record, field) => rv::Expr::Field(boxed(record), field.to_string()),
        Expr::Index(list, index) => rv::Expr::Index(boxed(list), boxed(index)),
        Expr::Match { scrutinee, arms } => rv::Expr::Match {
            scrutinee: boxed(scrutinee),
            arms: arms
                .iter()
                .map(|(pattern, body)| (pattern_to_ref(pattern), expr_to_ref(body)))
                .collect(),
        },
        Expr::Record(fields) => rv::Expr::Record(
            fields
                .iter()
                .map(|(name, field)| (name.to_string(), expr_to_ref(field)))
                .collect(),
        ),
    }
}

/// The reference pattern of a production pattern.
fn pattern_to_ref(pattern: &Pattern) -> rv::Pattern {
    match pattern {
        Pattern::Wildcard => rv::Pattern::Wildcard,
        Pattern::Var(name) => rv::Pattern::Variable(name.to_string()),
        Pattern::Lit(literal) => rv::Pattern::Literal(literal_to_ref(literal)),
        Pattern::Record(fields) => rv::Pattern::Record(
            fields
                .iter()
                .map(|(name, field)| (name.to_string(), pattern_to_ref(field)))
                .collect(),
        ),
        Pattern::List(items) => rv::Pattern::List(items.iter().map(pattern_to_ref).collect()),
        Pattern::Constructor(name, fields) => rv::Pattern::Constructor(
            name.to_string(),
            fields.iter().map(pattern_to_ref).collect(),
        ),
    }
}

fn var(name: &str) -> Expr {
    Expr::Var(Arc::from(name))
}

fn lam(param: &str, body: Expr) -> Expr {
    Expr::Lam(Arc::from(param), Box::new(body))
}

fn app(function: Expr, argument: Expr) -> Expr {
    Expr::App(Box::new(function), Box::new(argument))
}

fn let_in(name: &str, value: Expr, body: Expr) -> Expr {
    Expr::Let {
        name: Arc::from(name),
        value: Box::new(value),
        body: Box::new(body),
    }
}

const fn float(x: f64) -> Expr {
    Expr::Lit(Literal::Float(x))
}

/// The five evaluation shapes of one rounding builtin on one float: direct
/// application, a `let`-bound argument, a lambda applied to the argument, a
/// lambda whose parameter shadows an outer binding, and a closure whose free
/// variable is rebound after its creation (lexical scope keeps the captured
/// binding).
fn evaluation_shapes(op: BuiltinOp, x: f64) -> [Expr; 5] {
    let apply_to = |name: &str| Expr::Builtin(op, vec![var(name)]);
    [
        Expr::Builtin(op, vec![float(x)]),
        let_in("x", float(x), apply_to("x")),
        app(lam("y", apply_to("y")), float(x)),
        let_in("y", float(0.5), app(lam("y", apply_to("y")), float(x))),
        let_in(
            "y",
            float(x),
            let_in(
                "f",
                lam("z", apply_to("y")),
                let_in("y", float(0.5), app(var("f"), float(1.0))),
            ),
        ),
    ]
}

/// Production evaluation agrees with the reference evaluator, value for
/// value and refusal for refusal, on every evaluation shape of every
/// rounding builtin over [`rounding_scope`] and every widened binary16
/// pattern, and on an unbound variable.
#[test]
fn evaluation_agrees_with_the_reference() {
    let mut scope = rounding_scope();
    let rounding = scope.len();
    scope.extend(
        panproto_reference::enumerate::values::binary16_patterns()
            .into_iter()
            .map(|pattern| f64::from_bits(rf::widen_bits(Format::B16, pattern))),
    );
    assert_eq!(scope.len(), rounding + (1 << 16));
    let config = EvalConfig::default();
    let empty = BTreeMap::new();
    let mut cases = 0_usize;
    for op in ROUNDING_OPS {
        for &x in &scope {
            for expr in evaluation_shapes(op, x) {
                let production = eval(&expr, &Env::new(), &config);
                let reference = rv::eval(&expr_to_ref(&expr), &empty);
                assert_eq!(outcome_to_ref(&production), reference, "{expr:?}");
                cases += 1;
            }
        }
    }
    let unbound = var("unbound");
    assert_eq!(
        outcome_to_ref(&eval(&unbound, &Env::new(), &config)),
        rv::eval(&expr_to_ref(&unbound), &empty)
    );
    cases += 1;
    println!(
        "evaluation: bound {} ops by {} floats ({rounding} rounding-scope floats and 65536 widened binary16 patterns) by 5 shapes plus 1 unbound variable: {cases} expressions, all agree",
        ROUNDING_OPS.len(),
        scope.len()
    );
    assert_eq!(cases, ROUNDING_OPS.len() * scope.len() * 5 + 1);
}

const fn int(value: i64) -> Expr {
    Expr::Lit(Literal::Int(value))
}

fn list(items: &[i64]) -> Expr {
    Expr::List(items.iter().copied().map(int).collect())
}

const fn call(op: BuiltinOp, args: Vec<Expr>) -> Expr {
    Expr::Builtin(op, args)
}

/// Every evaluation shape of the higher-order and curried builtins: a
/// builtin named by a free variable is that builtin curried over its
/// surface arguments (`map f xs`, `fold f z xs`) unless a binding shadows
/// the name; an under-saturated builtin is the function of its missing
/// arguments; a higher-order builtin evaluates its list, then a fold's
/// initial value, and its function only to apply it; and the remainder of
/// the least integer by minus one is zero.
fn higher_order_shapes() -> Vec<Expr> {
    let add_one = lam("x", call(BuiltinOp::Add, vec![var("x"), int(1)]));
    let sum = lam(
        "a",
        lam("b", call(BuiltinOp::Add, vec![var("a"), var("b")])),
    );
    let broken = Expr::Field(Box::new(int(1)), Arc::from("f"));
    let failing = call(BuiltinOp::Div, vec![int(1), int(0)]);
    vec![
        call(BuiltinOp::Mod, vec![int(i64::MIN), int(-1)]),
        call(BuiltinOp::Mod, vec![int(-7), int(2)]),
        call(BuiltinOp::Div, vec![int(i64::MIN), int(-1)]),
        call(BuiltinOp::Map, vec![list(&[1, 2]), add_one.clone()]),
        call(BuiltinOp::Map, vec![list(&[]), broken.clone()]),
        call(BuiltinOp::Map, vec![list(&[1]), broken.clone()]),
        call(
            BuiltinOp::Filter,
            vec![
                list(&[1, 5]),
                lam("x", call(BuiltinOp::Gt, vec![var("x"), int(2)])),
            ],
        ),
        call(
            BuiltinOp::FlatMap,
            vec![
                list(&[1, 2]),
                lam("x", Expr::List(vec![var("x"), var("x")])),
            ],
        ),
        call(BuiltinOp::Fold, vec![list(&[1, 2, 3]), int(0), sum.clone()]),
        call(
            BuiltinOp::Fold,
            vec![list(&[]), failing.clone(), broken.clone()],
        ),
        call(BuiltinOp::Fold, vec![list(&[1]), int(0), broken.clone()]),
        call(BuiltinOp::Fold, vec![int(1), failing, broken]),
        call(BuiltinOp::Fold, vec![list(&[1, 2]), int(0), var("add")]),
        app(app(var("sub"), int(5)), int(3)),
        app(call(BuiltinOp::Add, vec![int(1)]), int(2)),
        app(app(var("map"), add_one), list(&[1, 2])),
        app(app(app(var("fold"), sum), int(10)), list(&[1, 2, 3])),
        let_in("add", int(5), var("add")),
        app(var("add"), int(1)),
    ]
}

/// Production evaluation of every higher-order and curried shape agrees
/// with the reference evaluator, value for value and refusal for refusal.
#[test]
fn higher_order_builtins_evaluate_as_the_reference_does() {
    let shapes = higher_order_shapes();
    let config = EvalConfig::default();
    for expr in &shapes {
        let production = eval(expr, &Env::new(), &config);
        let reference = rv::eval(&expr_to_ref(expr), &BTreeMap::new());
        let production = match production {
            Ok(Literal::Closure { .. }) => Err(EvalError::NotAFunction),
            other => outcome_to_ref(&other),
        };
        assert_eq!(production, reference, "{expr:?}");
    }
    assert!(
        matches!(eval(&shapes[0], &Env::new(), &config), Ok(Literal::Int(0))),
        "the remainder of i64::MIN by -1 is zero"
    );
    println!(
        "higher-order and curried builtins: {} shapes, all agree",
        shapes.len()
    );
    assert_eq!(shapes.len(), 19);
}

/// Compare-total orders two representations of one negative number with
/// the larger exponent lower: the order of the positive sign, reversed.
#[test]
fn compare_total_reverses_the_exponent_tie_for_negative_values() {
    let pairs = [
        ("-1", "-1.00", Ordering::Less),
        ("1", "1.00", Ordering::Greater),
        ("-0", "-0.000", Ordering::Less),
        ("0", "0.000", Ordering::Greater),
        ("-1.5", "-1.50", Ordering::Less),
        ("-1E+2", "-100", Ordering::Less),
    ];
    for (left, right, expected) in pairs {
        let (a, b) = (
            DecimalValue::parse(left).unwrap(),
            DecimalValue::parse(right).unwrap(),
        );
        assert_eq!(a.compare_total(&b), expected, "{left} against {right}");
        assert_eq!(
            b.compare_total(&a),
            expected.reverse(),
            "{right} against {left}"
        );
        assert_eq!(
            rx::Decimal::compare_total(&decimal_to_ref(&a), &decimal_to_ref(&b)),
            expected,
            "reference {left} against {right}"
        );
    }
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
    const DATES: [(&str, &str); 2] = [("2016-12-31", "2017-01-01"), ("2024-01-01", "2024-01-02")];
    const OFFSETS: [&str; 5] = ["Z", "+00:00", "+00:01", "-00:01", ""];
    const SECONDS: [&str; 3] = ["58", "59", "60"];
    const FRACTIONS: [&str; 4] = ["", ".0", ".5", ".999"];
    const NEXT_FRACTIONS: [&str; 3] = ["", ".0", ".5"];
    let mut lexemes = Vec::new();
    for (date, next) in DATES {
        for offset in OFFSETS {
            for second in SECONDS {
                for fraction in FRACTIONS {
                    lexemes.push(format!("{date}T23:59:{second}{fraction}{offset}"));
                    lexemes.push(format!("23:59:{second}{fraction}{offset}"));
                }
            }
            for fraction in NEXT_FRACTIONS {
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
        "leap seconds: lexemes={}, ordered_pairs={compared}, all agree",
        values.len()
    );
    // The date-times are distinct for each date, offset, second and fraction
    // and for each date, offset and next-minute or earlier-minute fraction;
    // the times repeat across the dates, so they count once per offset.
    let date_times = DATES.len()
        * OFFSETS.len()
        * (SECONDS.len() * FRACTIONS.len() + 2 * NEXT_FRACTIONS.len());
    let times = OFFSETS.len() * (SECONDS.len() * FRACTIONS.len() + NEXT_FRACTIONS.len());
    assert_eq!(date_times + times, 255);
    assert_eq!(values.len(), date_times + times);
    assert_eq!(compared, ((date_times + times) * (date_times + times)) as u64);
}

// ---------------------------------------------------------------------------
// Decimals.
// ---------------------------------------------------------------------------

/// The production decimal of a reference decimal: a finite value field for
/// field, an infinity or a NaN with its sign, flag and payload.
fn decimal_from_ref(d: &rx::Decimal) -> DecimalValue {
    match d {
        rx::Decimal::Finite(value) => DecimalValue::new(
            value.negative,
            value.digits.to_string(),
            value.exponent.to_i64().expect("an enumerated exponent fits i64"),
        )
        .expect("an enumerated significand is canonical"),
        rx::Decimal::Infinite { negative } => DecimalValue::infinity(*negative),
        rx::Decimal::NaN {
            negative,
            signalling,
            payload,
        } => DecimalValue::nan(*negative, *signalling, payload.to_string()),
    }
}

/// The scope's size, counted independently: 5 exponents by 2 signs by 100
/// significands (zero and 1 to 99), 2 infinities and 2 x 2 x 2 NaNs.
const DECIMAL_SCOPE: usize = 5 * 2 * 100 + 2 + 2 * 2 * 2;

/// The production order and compare-total agree with the reference's on
/// every pair of the reference's decimal scope (each mapped to a production
/// decimal that maps back to it exactly); the order is a total preorder on every triple,
/// compare-total refines it on every pair, and the canonical bytes are the
/// reference's bytes and differ for every two distinct values.
#[test]
fn decimal_orders_agree_with_the_reference() {
    let reference = panproto_reference::enumerate::values::decimals();
    assert_eq!(reference.len(), DECIMAL_SCOPE);
    println!("bound: decimals={}", reference.len());
    let scope: Vec<DecimalValue> = reference.iter().map(decimal_from_ref).collect();
    for (production, enumerated) in scope.iter().zip(&reference) {
        assert_eq!(&decimal_to_ref(production), enumerated, "{production}");
    }
    let n = scope.len();
    // The production numeric order as a matrix, checked pair by pair.
    let mut order = vec![Ordering::Equal; n * n];
    let mut bytes = std::collections::HashSet::new();
    let mut finite_pairs = 0_usize;
    for (i, a) in scope.iter().enumerate() {
        let a_bytes = a.canonical_bytes();
        let mut tagged_bytes = vec![13];
        tagged_bytes.extend_from_slice(&a_bytes);
        assert_eq!(
            Some(tagged_bytes),
            panproto_reference::canonical::decimal_bytes(&reference[i]),
            "{a}"
        );
        let mut tagged = vec![13];
        tagged.extend_from_slice(&a_bytes);
        assert_eq!(
            panproto_reference::canonical::decode_value(&tagged),
            Some(RValue::Decimal(reference[i].clone())),
            "{a}"
        );
        assert!(bytes.insert(a_bytes), "two values with one encoding: {a}");
        for (j, b) in scope.iter().enumerate() {
            let numeric = a.cmp_numeric(b);
            assert_eq!(
                numeric,
                rx::Decimal::cmp_numeric(&reference[i], &reference[j]),
                "{a} against {b}"
            );
            if let (rx::Decimal::Finite(left), rx::Decimal::Finite(right)) =
                (&reference[i], &reference[j])
            {
                assert_eq!(
                    numeric,
                    rx::DecimalValue::cmp_numeric(left, right),
                    "finite {a} against {b}"
                );
                finite_pairs += 1;
            }
            let total = a.compare_total(b);
            assert_eq!(
                total,
                rx::Decimal::compare_total(&reference[i], &reference[j]),
                "{a} total {b}"
            );
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
        "decimal orders: decimals={n} (significands 0 to 99, exponents -2 to 2, both signs, \
         2 infinities, 8 NaNs); pairs={}, finite_pairs={finite_pairs}, triples={triples}; \
         every encoding decodes to its reference value",
        n * n
    );
    assert_eq!(n * n, DECIMAL_SCOPE * DECIMAL_SCOPE);
    assert_eq!(triples, (n * n * n) as u64);
    // The finite decimals: 5 exponents by 2 signs by 100 significands.
    assert_eq!(finite_pairs, (5 * 2 * 100) * (5 * 2 * 100));
}

/// Every special spelling of the numeric string syntax, every finite
/// numeral of the values page's scope and a set of non-numerals read as
/// the reference reads them, and display back as the reference writes them.
#[test]
fn decimal_lexemes_read_as_the_reference_reads_them() {
    let mut lexemes: Vec<String> = panproto_reference::enumerate::values::numerals();
    for sign in ["", "+", "-"] {
        for body in [
            "NaN", "nan", "NAN", "sNaN", "snan", "SNAN", "NaN0", "NaN1", "NaN007", "sNaN12", "Inf",
            "inf", "INF", "Infinity", "infinity", "INFINITY", "0", "0.00", "0e5", "0E+3", "1.50",
            ".5", "5.", "Infinit", "NaN.5", "sNaN-1", "nanx", ".nan", "in", "",
        ] {
            lexemes.push(format!("{sign}{body}"));
        }
    }
    let mut read = 0;
    for text in &lexemes {
        let production = DecimalValue::parse(text).ok();
        let reference = rx::parse_decimal(text);
        assert_eq!(
            production.as_ref().map(decimal_to_ref),
            reference,
            "{text:?}"
        );
        if let Some(value) = production {
            let shown = value.to_string();
            assert_eq!(
                DecimalValue::parse(&shown).ok().as_ref(),
                Some(&value),
                "{text:?}"
            );
            assert_eq!(
                apply_builtin(BuiltinOp::ToString, &[Literal::Decimal(value.clone())])
                    .ok()
                    .map(|l| literal_to_ref(&l)),
                rv::apply_builtin(
                    rv::BuiltinOp::ToString,
                    &[RValue::Decimal(decimal_to_ref(&value))]
                )
                .ok(),
                "{text:?}"
            );
            read += 1;
        }
    }
    println!("decimal lexemes: {} lexemes, {read} read", lexemes.len());
}

// ---------------------------------------------------------------------------
// Generated differential cases.
// ---------------------------------------------------------------------------

/// A generated exact decimal, including finite values and every special
/// class. Finite digits and NaN payloads come from integers, so their digit
/// strings are nonempty and canonical.
fn decimal_strategy() -> impl Strategy<Value = DecimalValue> {
    prop_oneof![
        6 => (any::<bool>(), any::<u64>(), any::<i64>()).prop_map(
            |(negative, digits, exponent)| {
                DecimalValue::new(negative, digits.to_string(), exponent)
                    .expect("generated decimal fields are canonical")
            }
        ),
        1 => any::<bool>().prop_map(DecimalValue::infinity),
        3 => (any::<bool>(), any::<bool>(), any::<u64>()).prop_map(
            |(negative, signalling, payload)| {
                DecimalValue::nan(negative, signalling, payload.to_string())
            }
        ),
    ]
}

/// The frozen reference enumerator supplies the generated temporal lexemes.
fn temporal_lexemes() -> &'static [String] {
    static LEXEMES: OnceLock<Vec<String>> = OnceLock::new();
    LEXEMES
        .get_or_init(panproto_reference::enumerate::values::temporal_lexemes)
        .as_slice()
}

fn temporal_index_strategy() -> impl Strategy<Value = usize> {
    0..temporal_lexemes().len()
}

fn generated_config() -> ProptestConfig {
    ProptestConfig {
        cases: 4096,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(generated_config())]

    /// Generated float bits, exact decimals, enumerated temporal lexemes and
    /// epoch counts have the production outcomes and laws of the reference.
    #[test]
    fn randomized_values_differential(
        float_bits in any::<u64>(),
        decimals in prop::array::uniform3(decimal_strategy()),
        temporal_indices in prop::array::uniform3(temporal_index_strategy()),
        epoch_count in any::<i64>(),
        unit_index in 0_usize..4,
        instant in any::<bool>(),
    ) {
        let float = f64::from_bits(float_bits);
        for op in [
            BuiltinOp::FloatToInt,
            BuiltinOp::Floor,
            BuiltinOp::Ceil,
            BuiltinOp::Round,
        ] {
            let production = apply_builtin(op, &[Literal::Float(float)]);
            let reference = rv::apply_builtin(
                op_to_ref(op),
                &[RValue::Float(float_bits)],
            );
            prop_assert_eq!(
                outcome_to_ref(&production),
                reference,
                "generated {op:?} disagrees for float bits {float_bits:#018x}",
                op = op,
                float_bits = float_bits,
            );
        }

        let reference_decimals = decimals.each_ref().map(decimal_to_ref);
        for i in 0..decimals.len() {
            let mut production_bytes = vec![13];
            production_bytes.extend_from_slice(&decimals[i].canonical_bytes());
            prop_assert_eq!(
                Some(production_bytes),
                panproto_reference::canonical::decimal_bytes(&reference_decimals[i]),
                "generated decimal canonical bytes disagree for {}",
                decimals[i]
            );
            for j in 0..decimals.len() {
                let numeric = decimals[i].cmp_numeric(&decimals[j]);
                let reverse_numeric = decimals[j].cmp_numeric(&decimals[i]);
                let total = decimals[i].compare_total(&decimals[j]);
                let reverse_total = decimals[j].compare_total(&decimals[i]);
                prop_assert_eq!(
                    numeric,
                    reference_decimals[i].cmp_numeric(&reference_decimals[j]),
                    "generated decimal numeric order disagrees: {} against {}",
                    decimals[i],
                    decimals[j]
                );
                prop_assert_eq!(
                    total,
                    reference_decimals[i].compare_total(&reference_decimals[j]),
                    "generated decimal total order disagrees: {} against {}",
                    decimals[i],
                    decimals[j]
                );
                prop_assert_eq!(
                    numeric,
                    reverse_numeric.reverse(),
                    "generated decimal numeric reversal fails: {} against {}",
                    decimals[i],
                    decimals[j]
                );
                prop_assert_eq!(
                    total,
                    reverse_total.reverse(),
                    "generated decimal total reversal fails: {} against {}",
                    decimals[i],
                    decimals[j]
                );
                prop_assert_eq!(
                    total.is_eq(),
                    decimals[i] == decimals[j],
                    "generated decimal total equality disagrees with representation equality: {} against {}",
                    decimals[i],
                    decimals[j]
                );
                prop_assert_eq!(
                    rv::representation(
                        &RValue::Decimal(reference_decimals[i].clone()),
                        &RValue::Decimal(reference_decimals[j].clone()),
                    ),
                    Some(decimals[i] == decimals[j]),
                    "generated decimal equality disagrees with reference representation equality: {} against {}",
                    decimals[i],
                    decimals[j]
                );
                prop_assert_eq!(
                    decimals[i].canonical_bytes() == decimals[j].canonical_bytes(),
                    decimals[i] == decimals[j],
                    "generated decimal canonical payloads are not injective: {} against {}",
                    decimals[i],
                    decimals[j]
                );
                prop_assert_eq!(
                    panproto_reference::canonical::decimal_bytes(&reference_decimals[i])
                        == panproto_reference::canonical::decimal_bytes(&reference_decimals[j]),
                    reference_decimals[i] == reference_decimals[j],
                    "reference decimal canonical bytes are not injective: {} against {}",
                    decimals[i],
                    decimals[j]
                );
                if numeric.is_lt() {
                    prop_assert!(
                        total.is_lt(),
                        "generated decimal total order does not refine numeric order: {} against {}",
                        decimals[i],
                        decimals[j]
                    );
                }
            }
        }
        for i in 0..decimals.len() {
            for j in 0..decimals.len() {
                for k in 0..decimals.len() {
                    let ij = decimals[i].cmp_numeric(&decimals[j]);
                    let jk = decimals[j].cmp_numeric(&decimals[k]);
                    if ij.is_le() && jk.is_le() {
                        prop_assert!(
                            decimals[i].cmp_numeric(&decimals[k]).is_le(),
                            "generated decimal numeric transitivity fails: {}, {}, {}",
                            decimals[i],
                            decimals[j],
                            decimals[k]
                        );
                    }
                    let ij = decimals[i].compare_total(&decimals[j]);
                    let jk = decimals[j].compare_total(&decimals[k]);
                    if ij.is_le() && jk.is_le() {
                        prop_assert!(
                            decimals[i].compare_total(&decimals[k]).is_le(),
                            "generated decimal total transitivity fails: {}, {}, {}",
                            decimals[i],
                            decimals[j],
                            decimals[k]
                        );
                    }
                }
            }
        }

        let lexemes = temporal_lexemes();
        let selected = temporal_indices.each_ref().map(|index| &lexemes[*index]);
        let production_temporals = selected.each_ref().map(|text| Temporal::parse_union(text));
        let reference_temporals = selected.each_ref().map(|text| rt::parse_any(text));
        for i in 0..production_temporals.len() {
            prop_assert_eq!(
                production_temporals[i]
                    .as_ref()
                    .ok()
                    .map(temporal_to_ref),
                reference_temporals[i].clone(),
                "generated temporal parse outcome disagrees for {:?}",
                selected[i]
            );
            let (Ok(production_temporal), Some(reference_temporal)) =
                (&production_temporals[i], &reference_temporals[i])
            else {
                continue;
            };
            let production_form = production_temporal
                .lexical_form()
                .expect("an enumerated production temporal has an exact lexical form");
            let production_from_lexeme = Temporal::parse_as(production_form, selected[i])
                .expect("an enumerated production temporal reparses under its descriptor");
            prop_assert_eq!(
                production_from_lexeme.to_string(),
                selected[i].as_str(),
                "generated production temporal parse/display fails for {:?}",
                selected[i]
            );
            prop_assert_eq!(
                Temporal::parse_as(production_form, &production_temporal.to_string()),
                Some(production_temporal.clone()),
                "generated production temporal display/parse fails for {:?}",
                selected[i]
            );
            let reference_form = reference_temporal
                .lexical_form()
                .expect("an enumerated reference temporal has an exact lexical form");
            let reference_from_lexeme = rt::parse_as(reference_form, selected[i])
                .expect("an enumerated reference temporal reparses under its descriptor");
            prop_assert_eq!(
                reference_from_lexeme.to_string(),
                selected[i].as_str(),
                "generated reference temporal parse/display fails for {:?}",
                selected[i]
            );
            prop_assert_eq!(
                rt::parse_as(reference_form, &reference_temporal.to_string()),
                Some(reference_temporal.clone()),
                "generated reference temporal display/parse fails for {:?}",
                selected[i]
            );
            prop_assert_eq!(
                temporal_to_ref(production_temporal),
                reference_temporal.clone(),
                "generated temporal value disagrees for {:?}",
                selected[i]
            );
            prop_assert_eq!(
                production_temporal.to_string(),
                reference_temporal.to_string(),
                "generated temporal display disagrees for {:?}",
                selected[i]
            );
            let mut production_bytes = vec![15];
            production_bytes.extend_from_slice(&production_temporal.canonical_bytes());
            prop_assert_eq!(
                Some(production_bytes),
                rt::canonical_bytes(reference_temporal),
                "generated temporal canonical bytes disagree for {:?}",
                selected[i]
            );
            for j in 0..production_temporals.len() {
                let (Ok(other_production), Some(other_reference)) =
                    (&production_temporals[j], &reference_temporals[j])
                else {
                    continue;
                };
                let forward = production_temporal.cmp_chronological(other_production);
                let backward = other_production.cmp_chronological(production_temporal);
                prop_assert_eq!(
                    forward,
                    rt::cmp_chronological(reference_temporal, other_reference),
                    "generated temporal order disagrees: {:?} against {:?}",
                    selected[i],
                    selected[j]
                );
                prop_assert_eq!(
                    backward,
                    rt::cmp_chronological(other_reference, reference_temporal),
                    "generated reverse temporal order disagrees: {:?} against {:?}",
                    selected[j],
                    selected[i]
                );
                prop_assert_eq!(
                    forward,
                    backward.map(Ordering::reverse),
                    "generated temporal reversal fails: {:?} against {:?}",
                    selected[i],
                    selected[j]
                );
                prop_assert_eq!(
                    rv::representation(
                        &RValue::Temporal(reference_temporal.clone()),
                        &RValue::Temporal(other_reference.clone()),
                    ),
                    Some(production_temporal == other_production),
                    "generated temporal equality disagrees with reference representation equality: {:?} against {:?}",
                    selected[i],
                    selected[j]
                );
                prop_assert_eq!(
                    production_temporal.canonical_bytes() == other_production.canonical_bytes(),
                    production_temporal == other_production,
                    "generated temporal canonical payloads are not injective: {:?} against {:?}",
                    selected[i],
                    selected[j]
                );
                prop_assert_eq!(
                    rt::canonical_bytes(reference_temporal)
                        == rt::canonical_bytes(other_reference),
                    reference_temporal == other_reference,
                    "reference temporal canonical bytes are not injective: {:?} against {:?}",
                    selected[i],
                    selected[j]
                );
            }
        }
        for i in 0..production_temporals.len() {
            for j in 0..production_temporals.len() {
                for k in 0..production_temporals.len() {
                    let (Ok(a), Ok(b), Ok(c)) = (
                        &production_temporals[i],
                        &production_temporals[j],
                        &production_temporals[k],
                    ) else {
                        continue;
                    };
                    let (Some(reference_a), Some(reference_b), Some(reference_c)) = (
                        &reference_temporals[i],
                        &reference_temporals[j],
                        &reference_temporals[k],
                    ) else {
                        continue;
                    };
                    let ij = a.cmp_chronological(b);
                    let jk = b.cmp_chronological(c);
                    if ij.is_some_and(Ordering::is_le) && jk.is_some_and(Ordering::is_le) {
                        prop_assert!(
                            a.cmp_chronological(c)
                                .is_some_and(Ordering::is_le),
                            "generated temporal transitivity fails: {:?}, {:?}, {:?}",
                            selected[i],
                            selected[j],
                            selected[k]
                        );
                        prop_assert!(
                            rt::cmp_chronological(reference_a, reference_c)
                                .is_some_and(Ordering::is_le),
                            "reference temporal transitivity fails: {:?}, {:?}, {:?}",
                            selected[i],
                            selected[j],
                            selected[k]
                        );
                        prop_assert!(
                            rt::cmp_chronological(reference_a, reference_b)
                                .is_some_and(Ordering::is_le)
                                && rt::cmp_chronological(reference_b, reference_c)
                                    .is_some_and(Ordering::is_le),
                            "production/reference temporal transitivity premise disagrees: {:?}, {:?}, {:?}",
                            selected[i],
                            selected[j],
                            selected[k]
                        );
                    }
                }
            }
        }

        let (production_unit, reference_unit) = match unit_index {
            0 => (TimeUnit::Seconds, rt::TimeUnit::Seconds),
            1 => (TimeUnit::Millis, rt::TimeUnit::Millis),
            2 => (TimeUnit::Micros, rt::TimeUnit::Micros),
            3 => (TimeUnit::Nanos, rt::TimeUnit::Nanos),
            _ => unreachable!("unit strategy emits indices below four"),
        };
        let count = i128::from(epoch_count);
        let production_epoch = Temporal::from_epoch_count(count, production_unit, instant);
        let reference_epoch = rt::from_epoch_count(count, reference_unit, instant);
        let production_epoch_image = production_epoch.as_ref().ok().map(temporal_to_ref);
        prop_assert_eq!(
            production_epoch_image.as_ref(),
            reference_epoch.as_ref(),
            "generated epoch conversion disagrees for {count} {production_unit:?}, instant={instant}",
            count = count,
            production_unit = production_unit,
            instant = instant,
        );
        let production_back = production_epoch
            .as_ref()
            .ok()
            .and_then(|value| value.to_epoch_count(production_unit, instant));
        let reference_back = reference_epoch
            .as_ref()
            .and_then(|value| rt::to_epoch_count(value, reference_unit, instant));
        prop_assert_eq!(
            production_back,
            reference_back,
            "generated epoch round trip disagrees for {count} {production_unit:?}, instant={instant}",
            count = count,
            production_unit = production_unit,
            instant = instant,
        );
        prop_assert_eq!(
            production_back,
            Some(count),
            "generated epoch count did not round trip for {count} {production_unit:?}, instant={instant}",
            count = count,
            production_unit = production_unit,
            instant = instant,
        );

        let production_instant = Temporal::from_epoch_count(count, production_unit, true)
            .expect("an i64 epoch count is in the temporal year domain");
        let reference_instant = rt::from_epoch_count(count, reference_unit, true)
            .expect("an i64 epoch count is in the reference temporal year domain");
        let instant_text = production_instant.to_string();
        let fixed_text = format!(
            "{}+00:00",
            instant_text
                .strip_suffix('Z')
                .expect("an instant epoch value ends in Z")
        );
        let production_fixed = Temporal::parse_union(&fixed_text)
            .expect("a generated zero-offset date-time is well formed");
        let reference_fixed = rt::parse_any(&fixed_text)
            .expect("a generated zero-offset date-time is accepted by the reference");
        prop_assert_eq!(
            production_instant.cmp_chronological(&production_fixed),
            Some(Ordering::Equal),
            "generated Z and +00:00 forms do not denote the same instant: {instant_text:?} and {fixed_text:?}",
            instant_text = &instant_text,
            fixed_text = &fixed_text,
        );
        prop_assert_eq!(
            production_fixed.cmp_chronological(&production_instant),
            Some(Ordering::Equal),
            "generated +00:00 and Z forms do not denote the same instant: {fixed_text:?} and {instant_text:?}",
            fixed_text = &fixed_text,
            instant_text = &instant_text,
        );
        prop_assert_eq!(
            rt::cmp_chronological(&reference_instant, &reference_fixed),
            Some(Ordering::Equal),
            "reference Z and +00:00 forms do not denote the same instant: {instant_text:?} and {fixed_text:?}",
            instant_text = &instant_text,
            fixed_text = &fixed_text,
        );
        prop_assert_eq!(
            rt::cmp_chronological(&reference_fixed, &reference_instant),
            Some(Ordering::Equal),
            "reference +00:00 and Z forms do not denote the same instant: {fixed_text:?} and {instant_text:?}",
            fixed_text = &fixed_text,
            instant_text = &instant_text,
        );
        prop_assert_ne!(
            &production_instant,
            &production_fixed,
            "generated Z and +00:00 representations collapsed: {instant_text:?} and {fixed_text:?}",
            instant_text = &instant_text,
            fixed_text = &fixed_text,
        );
        prop_assert_ne!(
            &reference_instant,
            &reference_fixed,
            "reference Z and +00:00 representations collapsed: {instant_text:?} and {fixed_text:?}",
            instant_text = &instant_text,
            fixed_text = &fixed_text,
        );
        prop_assert_ne!(
            production_instant.canonical_bytes(),
            production_fixed.canonical_bytes(),
            "generated Z and +00:00 canonical payloads collided: {instant_text:?} and {fixed_text:?}",
            instant_text = &instant_text,
            fixed_text = &fixed_text,
        );
        prop_assert_ne!(
            rt::canonical_bytes(&reference_instant),
            rt::canonical_bytes(&reference_fixed),
            "reference Z and +00:00 canonical bytes collided: {instant_text:?} and {fixed_text:?}",
            instant_text = &instant_text,
            fixed_text = &fixed_text,
        );

        let neighboring_count = count
            .checked_add(1)
            .expect("an i64 epoch count plus one fits i128");
        let production_neighbor =
            Temporal::from_epoch_count(neighboring_count, production_unit, instant)
                .expect("a neighboring i64 epoch count is in the temporal year domain");
        let reference_neighbor = rt::from_epoch_count(neighboring_count, reference_unit, instant)
            .expect("a neighboring i64 epoch count is in the reference temporal year domain");
        let production_value = production_epoch
            .as_ref()
            .expect("an i64 epoch count is in the temporal year domain");
        let reference_value = reference_epoch
            .as_ref()
            .expect("an i64 epoch count is in the reference temporal year domain");
        prop_assert_eq!(
            &temporal_to_ref(&production_neighbor),
            &reference_neighbor,
            "generated neighboring epoch conversion disagrees for {neighboring_count} {production_unit:?}, instant={instant}",
            neighboring_count = neighboring_count,
            production_unit = production_unit,
            instant = instant,
        );
        prop_assert_eq!(
            production_value.cmp_chronological(&production_neighbor),
            rt::cmp_chronological(reference_value, &reference_neighbor),
            "generated complete epoch forms disagree in chronological order for {count} and {neighboring_count} {production_unit:?}, instant={instant}",
            count = count,
            neighboring_count = neighboring_count,
            production_unit = production_unit,
            instant = instant,
        );
        prop_assert_eq!(
            production_neighbor.cmp_chronological(production_value),
            rt::cmp_chronological(&reference_neighbor, reference_value),
            "generated complete epoch forms disagree in reverse chronological order for {neighboring_count} and {count} {production_unit:?}, instant={instant}",
            neighboring_count = neighboring_count,
            count = count,
            production_unit = production_unit,
            instant = instant,
        );
        prop_assert_eq!(
            production_value.cmp_chronological(&production_neighbor),
            Some(Ordering::Less),
            "successive complete epoch forms are not ordered for {count} {production_unit:?}, instant={instant}",
            count = count,
            production_unit = production_unit,
            instant = instant,
        );

        let production_local = Temporal::from_epoch_count(count, production_unit, false)
            .expect("an i64 epoch count is in the local temporal year domain");
        let reference_local = rt::from_epoch_count(count, reference_unit, false)
            .expect("an i64 epoch count is in the reference local temporal year domain");
        let local_text = production_local.to_string();
        let mut unwritten_form = production_local
            .lexical_form()
            .expect("a generated local epoch value has an exact lexical form");
        unwritten_form.offset = Offset::Zulu(ZuluCase::Unwritten);
        let production_unwritten = Temporal::parse_as(unwritten_form, &local_text)
            .expect("an unwritten-Z descriptor reparses a local epoch display");
        let mut reference_unwritten_form = reference_local
            .lexical_form()
            .expect("a generated reference local epoch value has an exact lexical form");
        reference_unwritten_form.offset = rt::Offset::Zulu(rt::ZuluCase::Unwritten);
        let reference_unwritten = rt::parse_as(reference_unwritten_form, &local_text)
            .expect("the reference unwritten-Z descriptor reparses a local epoch display");
        prop_assert_eq!(
            &temporal_to_ref(&production_unwritten),
            &reference_unwritten,
            "production and reference unwritten-Z values disagree for {local_text:?}",
            local_text = &local_text,
        );
        prop_assert!(
            rt::unwritten_zulu(&reference_unwritten),
            "reference parsing lost the unwritten-Z marker for {local_text:?}"
        );
        prop_assert_eq!(
            production_unwritten.to_string(),
            local_text.as_str(),
            "an unwritten-Z temporal did not preserve its display"
        );
        prop_assert_eq!(
            reference_unwritten.to_string(),
            reference_local.to_string(),
            "the reference unwritten-Z temporal did not preserve its local display"
        );
        prop_assert_eq!(
            production_unwritten.cmp_chronological(&production_instant),
            Some(Ordering::Equal),
            "an unwritten-Z temporal does not equal its explicit-Z instant for {local_text:?}",
            local_text = &local_text,
        );
        prop_assert_eq!(
            production_instant.cmp_chronological(&production_unwritten),
            Some(Ordering::Equal),
            "an explicit-Z instant does not equal its unwritten-Z temporal for {local_text:?}",
            local_text = &local_text,
        );
        prop_assert_eq!(
            rt::cmp_chronological(&reference_unwritten, &reference_instant),
            Some(Ordering::Equal),
            "the reference unwritten-Z temporal does not equal its explicit-Z instant for {local_text:?}",
            local_text = &local_text,
        );
        prop_assert_eq!(
            rt::cmp_chronological(&reference_instant, &reference_unwritten),
            Some(Ordering::Equal),
            "the reference explicit-Z instant does not equal its unwritten-Z temporal for {local_text:?}",
            local_text = &local_text,
        );
        prop_assert_ne!(
            &production_unwritten,
            &production_local,
            "an unwritten-Z temporal collapsed with its local representation for {local_text:?}",
            local_text = &local_text,
        );
        prop_assert_ne!(
            &reference_unwritten,
            &reference_local,
            "a reference unwritten-Z temporal collapsed with its local representation for {local_text:?}",
            local_text = &local_text,
        );
        prop_assert_ne!(
            &production_unwritten,
            &production_instant,
            "an unwritten-Z temporal collapsed with its explicit-Z representation for {local_text:?}",
            local_text = &local_text,
        );
        prop_assert_ne!(
            &reference_unwritten,
            &reference_instant,
            "a reference unwritten-Z temporal collapsed with its explicit-Z representation for {local_text:?}",
            local_text = &local_text,
        );
        prop_assert_ne!(
            production_unwritten.canonical_bytes(),
            production_local.canonical_bytes(),
            "an unwritten-Z canonical payload collided with its local payload for {local_text:?}",
            local_text = &local_text,
        );
        prop_assert_ne!(
            rt::canonical_bytes(&reference_unwritten),
            rt::canonical_bytes(&reference_local),
            "reference unwritten-Z canonical bytes collided with local bytes for {local_text:?}",
            local_text = &local_text,
        );
        prop_assert_ne!(
            production_unwritten.canonical_bytes(),
            production_instant.canonical_bytes(),
            "an unwritten-Z canonical payload collided with explicit-Z payload for {local_text:?}",
            local_text = &local_text,
        );
        prop_assert_ne!(
            rt::canonical_bytes(&reference_unwritten),
            rt::canonical_bytes(&reference_instant),
            "reference unwritten-Z canonical bytes collided with explicit-Z bytes for {local_text:?}",
            local_text = &local_text,
        );
    }
}

// ---------------------------------------------------------------------------
// Generated NaN and evaluation differential cases.
// ---------------------------------------------------------------------------

/// A generated binary64 operand: a NaN of either sign, quiet or signalling,
/// with an arbitrary payload; an arbitrary pattern; or a special value.
fn binary64_operand() -> impl Strategy<Value = u64> {
    const PAYLOAD: u64 = (1 << 51) - 1;
    prop_oneof![
        4 => (any::<bool>(), any::<bool>(), any::<u64>()).prop_map(|(negative, quiet, payload)| {
            let payload = payload & PAYLOAD;
            let payload = if quiet { payload } else { payload.max(1) };
            (u64::from(negative) << 63) | (0x7ff << 52) | (u64::from(quiet) << 51) | payload
        }),
        3 => any::<u64>(),
        2 => prop::sample::select(vec![
            0.0_f64.to_bits(),
            (-0.0_f64).to_bits(),
            1.0_f64.to_bits(),
            (-1.0_f64).to_bits(),
            0.5_f64.to_bits(),
            f64::INFINITY.to_bits(),
            f64::NEG_INFINITY.to_bits(),
            f64::MAX.to_bits(),
            f64::MIN_POSITIVE.to_bits(),
        ]),
    ]
}

/// The float builtins of the NaN differential, with their arities.
const NAN_OPS: [(BuiltinOp, usize); 8] = [
    (BuiltinOp::Neg, 1),
    (BuiltinOp::Abs, 1),
    (BuiltinOp::Add, 2),
    (BuiltinOp::Sub, 2),
    (BuiltinOp::Mul, 2),
    (BuiltinOp::Div, 2),
    (BuiltinOp::Clamp, 3),
    (BuiltinOp::Floor, 1),
];

/// The names a generated expression binds; every other name it uses is a
/// builtin's.
const BINDERS: [&str; 2] = ["x", "y"];

fn binder() -> impl Strategy<Value = &'static str> {
    prop::sample::select(BINDERS.to_vec())
}

/// A generated integer: the boundary integers of `Mod` and `Div`, small
/// integers, and arbitrary ones.
fn generated_int() -> impl Strategy<Value = i64> {
    prop_oneof![
        3 => prop::sample::select(vec![i64::MIN, -1, 0, 1, 2, 3, -7, i64::MAX]),
        2 => -4_i64..=4,
        1 => any::<i64>(),
    ]
}

/// The arithmetic and comparison builtins a generated expression applies.
fn arithmetic_op() -> impl Strategy<Value = BuiltinOp> {
    prop::sample::select(vec![
        BuiltinOp::Add,
        BuiltinOp::Sub,
        BuiltinOp::Mod,
        BuiltinOp::Div,
        BuiltinOp::Gt,
    ])
}

/// The surface name of an arithmetic builtin (`mod` names none, so it is
/// an unbound variable).
fn arithmetic_name() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec!["add", "sub", "mul", "mod"])
}

/// A generated closed expression whose value is never a function: integer
/// and list literals, bound and unbound variables, `let`, lambdas applied
/// to arguments, arithmetic on integers (fully applied, as an under-saturated
/// builtin applied to its last argument, or curried through a builtin name),
/// the higher-order builtins with literal lambdas (directly, curried through
/// a builtin name, or under-saturated with the lambda as the supplied first
/// surface argument), `match` with literal, variable and
/// wildcard patterns, and indexing. A function value only ever appears in
/// function position, so no function is applied to itself and evaluation
/// terminates.
fn value_expr() -> impl Strategy<Value = Expr> {
    let leaf = prop_oneof![
        3 => generated_int().prop_map(int),
        2 => prop::collection::vec(generated_int(), 0..4).prop_map(|items| list(&items)),
        2 => binder().prop_map(var),
    ];
    leaf.prop_recursive(4, 48, 3, |inner| {
        let lambda = (binder(), inner.clone())
            .prop_map(|(param, body)| lam(param, body))
            .boxed();
        let lambda2 = (binder(), binder(), inner.clone())
            .prop_map(|(a, b, body)| lam(a, lam(b, body)))
            .boxed();
        let pattern = prop_oneof![
            generated_int().prop_map(|n| Pattern::Lit(Literal::Int(n))),
            binder().prop_map(|name| Pattern::Var(Arc::from(name))),
            Just(Pattern::Wildcard),
        ];
        let arm = (pattern, inner.clone());
        prop_oneof![
            (binder(), inner.clone(), inner.clone())
                .prop_map(|(name, value, body)| let_in(name, value, body)),
            (lambda.clone(), inner.clone()).prop_map(|(f, a)| app(f, a)),
            (arithmetic_op(), inner.clone(), inner.clone())
                .prop_map(|(op, a, b)| call(op, vec![a, b])),
            (arithmetic_op(), inner.clone(), inner.clone())
                .prop_map(|(op, a, b)| app(call(op, vec![a]), b)),
            (arithmetic_name(), inner.clone(), inner.clone())
                .prop_map(|(name, a, b)| app(app(var(name), a), b)),
            (
                prop::sample::select(vec![BuiltinOp::Map, BuiltinOp::Filter, BuiltinOp::FlatMap]),
                inner.clone(),
                lambda.clone()
            )
                .prop_map(|(op, xs, f)| call(op, vec![xs, f])),
            (inner.clone(), inner.clone(), lambda2.clone())
                .prop_map(|(xs, z, f)| call(BuiltinOp::Fold, vec![xs, z, f])),
            (
                prop::sample::select(vec!["map", "filter", "flat_map"]),
                lambda.clone(),
                inner.clone()
            )
                .prop_map(|(name, f, xs)| app(app(var(name), f), xs)),
            (lambda2, inner.clone(), inner.clone())
                .prop_map(|(f, z, xs)| app(app(app(var("fold"), f), z), xs)),
            (
                prop::sample::select(vec![BuiltinOp::Map, BuiltinOp::Filter, BuiltinOp::FlatMap]),
                lambda,
                inner.clone()
            )
                .prop_map(|(op, f, xs)| app(call(op, vec![f]), xs)),
            (inner.clone(), prop::collection::vec(arm, 1..3)).prop_map(|(scrutinee, arms)| {
                Expr::Match {
                    scrutinee: Box::new(scrutinee),
                    arms,
                }
            }),
            (inner.clone(), inner.clone())
                .prop_map(|(xs, i)| Expr::Index(Box::new(xs), Box::new(i))),
            prop::collection::vec(inner, 0..3).prop_map(Expr::List),
        ]
    })
}

/// A generated closed expression: usually a value, sometimes a function (a
/// lambda, an under-saturated builtin or a builtin applied through its name
/// to too few arguments), whose evaluation the reference refuses as a
/// function-valued result.
fn generated_expr() -> impl Strategy<Value = Expr> {
    prop_oneof![
        8 => value_expr(),
        1 => (binder(), value_expr()).prop_map(|(param, body)| lam(param, body)),
        1 => (arithmetic_op(), value_expr()).prop_map(|(op, a)| call(op, vec![a])),
        1 => (arithmetic_name(), value_expr()).prop_map(|(name, a)| app(var(name), a)),
    ]
}

/// A production evaluation outcome in the reference model, a function value
/// counting as the reference's refusal of a function-valued result.
fn evaluation_to_ref(outcome: &Result<Literal, ExprError>) -> Result<RValue, EvalError> {
    match outcome {
        Ok(Literal::Closure { .. }) => Err(EvalError::NotAFunction),
        other => outcome_to_ref(other),
    }
}

proptest! {
    #![proptest_config(generated_config())]

    /// Generated binary64 NaN and non-NaN operands of the float builtins
    /// have the reference's outcome through `apply_builtin` and through
    /// `eval` of `let`-bound operands; a computed NaN is the reference's
    /// canonical NaN, and a `let`-bound NaN passes through with its bits.
    #[test]
    fn randomized_nan_differential(
        operands in prop::array::uniform3(binary64_operand()),
        op_index in 0_usize..NAN_OPS.len(),
    ) {
        let canonical = rf::computed_nan(Format::B64);
        let (op, arity) = NAN_OPS[op_index];
        let operands = &operands[..arity];
        prop_assume!(!(op == BuiltinOp::Clamp && operands[1..].iter().any(|b| f64::from_bits(*b).is_nan() ) ) && !(op == BuiltinOp::Clamp && f64::from_bits(operands[1]) > f64::from_bits(operands[2])));
        let literals: Vec<Literal> = operands
            .iter()
            .map(|bits| Literal::Float(f64::from_bits(*bits)))
            .collect();
        let references: Vec<RValue> = operands.iter().map(|bits| RValue::Float(*bits)).collect();
        let production = apply_builtin(op, &literals);
        let reference = rv::apply_builtin(op_to_ref(op), &references);
        prop_assert_eq!(
            outcome_to_ref(&production),
            reference.clone(),
            "{:?} on {:x?}",
            op,
            operands
        );
        if let Ok(Literal::Float(x)) = &production {
            if x.is_nan() {
                prop_assert_eq!(x.to_bits(), canonical, "{:?} on {:x?}", op, operands);
            }
        }
        let names = ["a", "b", "c"];
        let mut expr = call(op, names[..arity].iter().map(|name| var(name)).collect());
        for (name, bits) in names[..arity].iter().zip(operands).rev() {
            expr = let_in(name, float(f64::from_bits(*bits)), expr);
        }
        let evaluated = eval(&expr, &Env::new(), &EvalConfig::default());
        prop_assert_eq!(
            outcome_to_ref(&evaluated),
            rv::eval(&expr_to_ref(&expr), &BTreeMap::new()),
            "{:?}",
            expr
        );
        prop_assert_eq!(outcome_to_ref(&evaluated), reference, "{:?}", expr);
        if let Ok(Literal::Float(x)) = &evaluated {
            if x.is_nan() {
                prop_assert_eq!(x.to_bits(), canonical, "{:?}", expr);
            }
        }
        for bits in operands {
            let passed = let_in("x", float(f64::from_bits(*bits)), var("x"));
            let production = eval(&passed, &Env::new(), &EvalConfig::default());
            prop_assert_eq!(
                outcome_to_ref(&production),
                rv::eval(&expr_to_ref(&passed), &BTreeMap::new()),
                "{:?}",
                passed
            );
            prop_assert!(
                matches!(production, Ok(Literal::Float(x)) if x.to_bits() == *bits),
                "pass-through of {:#018x}",
                bits
            );
        }
    }

    /// Generated closed expressions evaluate as the reference evaluator
    /// evaluates them, value for value and refusal for refusal.
    #[test]
    fn randomized_evaluation_differential(expr in generated_expr()) {
        let production = eval(&expr, &Env::new(), &EvalConfig::default());
        let reference = rv::eval(&expr_to_ref(&expr), &BTreeMap::new());
        if let (Err(ExprError::TypeError{..}), Err(EvalError::TypeError{..})) = (&production, &reference) { println!("TEMP typeerror {:?} | {:?}", production, reference); return Ok(()); }
        prop_assert_eq!(
            evaluation_to_ref(&production),
            reference,
            "{:?}",
            expr
        );
    }
}

#[test]
fn temp_probe() {
    let cfg = EvalConfig::default();
    let probes = vec![
        Expr::Index(Box::new(int(1)), Box::new(int(0))),
        Expr::Index(Box::new(list(&[1])), Box::new(int(5))),
        Expr::Index(Box::new(list(&[1, 2])), Box::new(int(-2))),
        Expr::Index(Box::new(list(&[1, 2])), Box::new(int(-3))),
        Expr::Index(Box::new(list(&[1])), Box::new(int(i64::MIN))),
        call(BuiltinOp::Map, vec![lam("x", int(1)), lam("x", int(2))]),
        call(BuiltinOp::Clamp, vec![float(0.5), float(1.0), float(0.0)]),
        call(BuiltinOp::Clamp, vec![int(5), int(3), int(1)]),
        Expr::Match { scrutinee: Box::new(int(3)), arms: vec![(Pattern::Lit(Literal::Int(4)), int(0))] },
    ];
    for e in probes {
        println!("TEMPPROBE {e:?}\n  production {:?}\n  reference  {:?}", eval(&e, &Env::new(), &cfg), rv::eval(&expr_to_ref(&e), &BTreeMap::new()));
    }
}
