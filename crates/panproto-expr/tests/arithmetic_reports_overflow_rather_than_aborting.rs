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
