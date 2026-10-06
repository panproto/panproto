//! Integer arithmetic reports the one input pair it cannot answer for.
//!
//! Every arithmetic builtin is checked, so an expression that overflows comes
//! back as `ExprError::Overflow` and the caller decides what to do. `mod` was
//! the exception: it used the bare `%` operator, and `i64::MIN % -1` overflows
//! unconditionally in Rust — in a release build as much as a debug one — so a
//! hostile or merely unlucky expression aborted the process evaluating it.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use panproto_expr::{BuiltinOp, EvalConfig, Expr, ExprError, Literal, eval};

fn eval_binop(op: BuiltinOp, left: i64, right: i64) -> Result<Literal, ExprError> {
    let expr = Expr::Builtin(
        op,
        vec![
            Expr::Lit(Literal::Int(left)),
            Expr::Lit(Literal::Int(right)),
        ],
    );
    eval(&expr, &panproto_expr::Env::new(), &EvalConfig::default())
}

#[test]
fn the_one_overflowing_remainder_is_an_error() {
    assert!(
        matches!(
            eval_binop(BuiltinOp::Mod, i64::MIN, -1),
            Err(ExprError::Overflow)
        ),
        "mod(i64::MIN, -1) must report overflow"
    );
}

#[test]
fn the_matching_division_is_an_error_too() {
    assert!(
        matches!(
            eval_binop(BuiltinOp::Div, i64::MIN, -1),
            Err(ExprError::Overflow)
        ),
        "div(i64::MIN, -1) must report overflow"
    );
}

#[test]
fn a_zero_divisor_still_reports_division_by_zero() {
    assert!(matches!(
        eval_binop(BuiltinOp::Mod, 7, 0),
        Err(ExprError::DivisionByZero)
    ));
    assert!(matches!(
        eval_binop(BuiltinOp::Div, 7, 0),
        Err(ExprError::DivisionByZero)
    ));
}

#[test]
fn ordinary_remainders_are_unchanged() {
    assert_eq!(
        eval_binop(BuiltinOp::Mod, 17, 5).expect("17 mod 5 evaluates"),
        Literal::Int(2)
    );
    assert_eq!(
        eval_binop(BuiltinOp::Mod, -17, 5).expect("-17 mod 5 evaluates"),
        Literal::Int(-2)
    );
    assert_eq!(
        eval_binop(BuiltinOp::Mod, i64::MIN, 2).expect("i64::MIN mod 2 evaluates"),
        Literal::Int(0)
    );
}

/// The value an exact reference gives a float-to-integer builtin: `None`
/// when the rounded value is not an integer in `i64`'s range. The rounded
/// value is computed on the float (`trunc`, `floor`, `ceil` and `round`
/// are exact on binary64) and compared with the range as an exact integer
/// through its binary decomposition, so the reference uses no float-to-int
/// cast.
fn exact_integer(rounded: f64) -> Option<i64> {
    if !rounded.is_finite() {
        return None;
    }
    let bits = rounded.to_bits();
    let negative = bits >> 63 == 1;
    let biased = i32::try_from((bits >> 52) & 0x7ff).expect("eleven bits fit");
    let fraction = bits & ((1 << 52) - 1);
    if biased == 0 {
        // Zero, or a subnormal, which `rounded` (an integer) cannot be.
        assert_eq!(fraction, 0, "{rounded} is an integer");
        return Some(0);
    }
    let significand = i128::from((1_u64 << 52) | fraction);
    let shift = biased - 1075;
    let magnitude = if shift >= 0 {
        if shift > 64 {
            return None;
        }
        significand << shift
    } else {
        assert_eq!(significand % (1 << -shift), 0, "{rounded} is an integer");
        significand >> -shift
    };
    i64::try_from(if negative { -magnitude } else { magnitude }).ok()
}

/// A rounding of a float to an integral float.
type Rounding = fn(f64) -> f64;

/// Every float-to-integer builtin returns the exact integer when it lies
/// in `i64`'s range and refuses the float otherwise; it never saturates.
/// The scope holds 2^63 and -2^63, their binary64 neighbours on both sides,
/// values near them with fractions, the infinities, a NaN and small values.
#[test]
fn a_float_beyond_i64_is_refused_not_saturated() {
    const TWO_63: f64 = 9_223_372_036_854_775_808.0;
    let near = |x: f64, steps: i64| {
        let bits = x.to_bits();
        f64::from_bits(if steps >= 0 {
            bits + steps.unsigned_abs()
        } else {
            bits - steps.unsigned_abs()
        })
    };
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
    let ops: [(BuiltinOp, Rounding); 4] = [
        (BuiltinOp::FloatToInt, f64::trunc),
        (BuiltinOp::Floor, f64::floor),
        (BuiltinOp::Ceil, f64::ceil),
        (BuiltinOp::Round, f64::round),
    ];
    for (op, round) in ops {
        for &x in &scope {
            let got = panproto_expr::apply_builtin(op, &[Literal::Float(x)]);
            match exact_integer(round(x)) {
                Some(n) => assert!(
                    matches!(got, Ok(Literal::Int(m)) if m == n),
                    "{op:?}({x:e}) gave {got:?}, expected {n}"
                ),
                None => assert!(
                    matches!(got, Err(ExprError::FloatNotRepresentable(_))),
                    "{op:?}({x:e}) gave {got:?}"
                ),
            }
        }
    }
    assert!(
        panproto_expr::apply_builtin(BuiltinOp::FloatToInt, &[Literal::Float(TWO_63)]).is_err()
    );
    assert!(matches!(
        panproto_expr::apply_builtin(BuiltinOp::FloatToInt, &[Literal::Float(-TWO_63)]),
        Ok(Literal::Int(i64::MIN))
    ));
}
