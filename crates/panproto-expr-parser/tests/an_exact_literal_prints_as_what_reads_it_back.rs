//! A decimal literal has no surface syntax of its own, so the pretty
//! printer writes the `parse_decimal` call that reads it back, scale
//! included; the exact-value builtins are reachable by name.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_expr::exact::DecimalValue;
use panproto_expr::{Env, EvalConfig, Expr, Literal};
use panproto_expr_parser::{parse, pretty_print, tokenize};

fn eval(src: &str) -> Literal {
    let tokens = tokenize(src).unwrap_or_else(|e| panic!("{src} lexes: {e:?}"));
    let expr = parse(&tokens).unwrap_or_else(|e| panic!("{src} parses: {e:?}"));
    panproto_expr::eval(&expr, &Env::new(), &EvalConfig::default()).unwrap()
}

#[test]
fn a_printed_decimal_evaluates_back_to_the_same_decimal() {
    let decimal = Literal::Decimal(DecimalValue::parse("1.50").unwrap());
    let printed = pretty_print(&Expr::Lit(decimal.clone()));
    assert_eq!(printed, r#"(parse_decimal "1.50")"#);
    assert_eq!(eval(&printed), decimal);
    assert_eq!(
        eval(r#"to_string (parse_decimal "0.10")"#),
        Literal::Str("0.10".into())
    );
    assert_eq!(
        eval(r#"decimal_to_float (parse_decimal "0.5")"#),
        Literal::Float(0.5)
    );
}
