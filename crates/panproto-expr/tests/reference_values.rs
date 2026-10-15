//! Production values agree exactly with `panproto-reference`'s.
//!
//! Each test runs a production function of `panproto-expr` and its
//! `panproto_reference` counterpart on every case of a stated finite scope,
//! or on generated cases, and requires identical results: the same value, or
//! the same typed refusal. Each exhaustive test prints its bound and counts.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cmp::Ordering;
use std::sync::OnceLock;

use panproto_expr::exact::{
    CivilDate, CivilTime, DateFields, DateTimeSeparator, DecimalClass, DecimalValue, Offset,
    OffsetForm, Temporal, TemporalForm, TemporalInfinity, TimeFields, TimeUnit, WideInt, ZuluCase,
};
use panproto_expr::{BuiltinOp, ExprError, Literal, apply_builtin};
use panproto_reference::exact as rx;
use panproto_reference::num::{Int, Nat};
use panproto_reference::temporal as rt;
use panproto_reference::value::{self as rv, EvalError, Value as RValue};
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
        let mut tagged_bytes = vec![13];
        tagged_bytes.extend_from_slice(&a_bytes);
        assert_eq!(
            Some(tagged_bytes),
            panproto_reference::canonical::decimal_bytes(&reference[i]),
            "{a}"
        );
        assert!(bytes.insert(a_bytes), "two values with one encoding: {a}");
        for (j, b) in scope.iter().enumerate() {
            let numeric = a.cmp_numeric(b);
            assert_eq!(
                numeric,
                reference[i].cmp_numeric(&reference[j]),
                "{a} against {b}"
            );
            let total = a.compare_total(b);
            assert_eq!(
                total,
                reference[i].compare_total(&reference[j]),
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
        cases: 256,
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
                "generated {op:?} disagrees for float bits {float_bits:#018x}"
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
        prop_assert_eq!(
            production_epoch.as_ref().ok().map(temporal_to_ref),
            reference_epoch.clone(),
            "generated epoch conversion disagrees for {count} {production_unit:?}, instant={instant}"
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
            "generated epoch round trip disagrees for {count} {production_unit:?}, instant={instant}"
        );
        prop_assert_eq!(
            production_back,
            Some(count),
            "generated epoch count did not round trip for {count} {production_unit:?}, instant={instant}"
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
            "generated Z and +00:00 forms do not denote the same instant: {instant_text:?} and {fixed_text:?}"
        );
        prop_assert_eq!(
            production_fixed.cmp_chronological(&production_instant),
            Some(Ordering::Equal),
            "generated +00:00 and Z forms do not denote the same instant: {fixed_text:?} and {instant_text:?}"
        );
        prop_assert_eq!(
            rt::cmp_chronological(&reference_instant, &reference_fixed),
            Some(Ordering::Equal),
            "reference Z and +00:00 forms do not denote the same instant: {instant_text:?} and {fixed_text:?}"
        );
        prop_assert_eq!(
            rt::cmp_chronological(&reference_fixed, &reference_instant),
            Some(Ordering::Equal),
            "reference +00:00 and Z forms do not denote the same instant: {fixed_text:?} and {instant_text:?}"
        );
        prop_assert_ne!(
            &production_instant,
            &production_fixed,
            "generated Z and +00:00 representations collapsed: {instant_text:?} and {fixed_text:?}"
        );
        prop_assert_ne!(
            &reference_instant,
            &reference_fixed,
            "reference Z and +00:00 representations collapsed: {instant_text:?} and {fixed_text:?}"
        );
        prop_assert_ne!(
            production_instant.canonical_bytes(),
            production_fixed.canonical_bytes(),
            "generated Z and +00:00 canonical payloads collided: {instant_text:?} and {fixed_text:?}"
        );
        prop_assert_ne!(
            rt::canonical_bytes(&reference_instant),
            rt::canonical_bytes(&reference_fixed),
            "reference Z and +00:00 canonical bytes collided: {instant_text:?} and {fixed_text:?}"
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
            temporal_to_ref(&production_neighbor),
            reference_neighbor.clone(),
            "generated neighboring epoch conversion disagrees for {neighboring_count} {production_unit:?}, instant={instant}"
        );
        prop_assert_eq!(
            production_value.cmp_chronological(&production_neighbor),
            rt::cmp_chronological(reference_value, &reference_neighbor),
            "generated complete epoch forms disagree in chronological order for {count} and {neighboring_count} {production_unit:?}, instant={instant}"
        );
        prop_assert_eq!(
            production_neighbor.cmp_chronological(production_value),
            rt::cmp_chronological(&reference_neighbor, reference_value),
            "generated complete epoch forms disagree in reverse chronological order for {neighboring_count} and {count} {production_unit:?}, instant={instant}"
        );
        prop_assert_eq!(
            production_value.cmp_chronological(&production_neighbor),
            Some(Ordering::Less),
            "successive complete epoch forms are not ordered for {count} {production_unit:?}, instant={instant}"
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
            temporal_to_ref(&production_unwritten),
            reference_unwritten.clone(),
            "production and reference unwritten-Z values disagree for {local_text:?}"
        );
        prop_assert!(
            rt::unwritten_zulu(&reference_unwritten),
            "reference parsing lost the unwritten-Z marker for {local_text:?}"
        );
        prop_assert_eq!(
            production_unwritten.to_string(),
            local_text,
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
            "an unwritten-Z temporal does not equal its explicit-Z instant for {local_text:?}"
        );
        prop_assert_eq!(
            production_instant.cmp_chronological(&production_unwritten),
            Some(Ordering::Equal),
            "an explicit-Z instant does not equal its unwritten-Z temporal for {local_text:?}"
        );
        prop_assert_eq!(
            rt::cmp_chronological(&reference_unwritten, &reference_instant),
            Some(Ordering::Equal),
            "the reference unwritten-Z temporal does not equal its explicit-Z instant for {local_text:?}"
        );
        prop_assert_eq!(
            rt::cmp_chronological(&reference_instant, &reference_unwritten),
            Some(Ordering::Equal),
            "the reference explicit-Z instant does not equal its unwritten-Z temporal for {local_text:?}"
        );
        prop_assert_ne!(
            &production_unwritten,
            &production_local,
            "an unwritten-Z temporal collapsed with its local representation for {local_text:?}"
        );
        prop_assert_ne!(
            &reference_unwritten,
            &reference_local,
            "a reference unwritten-Z temporal collapsed with its local representation for {local_text:?}"
        );
        prop_assert_ne!(
            &production_unwritten,
            &production_instant,
            "an unwritten-Z temporal collapsed with its explicit-Z representation for {local_text:?}"
        );
        prop_assert_ne!(
            &reference_unwritten,
            &reference_instant,
            "a reference unwritten-Z temporal collapsed with its explicit-Z representation for {local_text:?}"
        );
        prop_assert_ne!(
            production_unwritten.canonical_bytes(),
            production_local.canonical_bytes(),
            "an unwritten-Z canonical payload collided with its local payload for {local_text:?}"
        );
        prop_assert_ne!(
            rt::canonical_bytes(&reference_unwritten),
            rt::canonical_bytes(&reference_local),
            "reference unwritten-Z canonical bytes collided with local bytes for {local_text:?}"
        );
        prop_assert_ne!(
            production_unwritten.canonical_bytes(),
            production_instant.canonical_bytes(),
            "an unwritten-Z canonical payload collided with explicit-Z payload for {local_text:?}"
        );
        prop_assert_ne!(
            rt::canonical_bytes(&reference_unwritten),
            rt::canonical_bytes(&reference_instant),
            "reference unwritten-Z canonical bytes collided with explicit-Z bytes for {local_text:?}"
        );
    }
}
