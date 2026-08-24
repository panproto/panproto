//! A builtin's name denotes a function wherever it is written.
//!
//! Builtin names were recognised only in the head position of an application,
//! so `upper "a"` worked while `map xs upper`, `xs & upper`, and
//! `let f = upper in f "a"` all failed with `UnboundVariable("upper")` — the
//! higher-order builtins could not be passed the very functions the language
//! ships. A lexical binding still wins: builtins sit in a scope beneath the
//! environment, so a parameter or `let` named `upper` shadows the builtin as
//! it would shadow anything else.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use panproto_expr::{Env, EvalConfig, ExprError, Literal};
use panproto_expr_parser::{parse, tokenize};

fn eval(src: &str) -> Result<Literal, ExprError> {
    let tokens = tokenize(src).expect("the fixture tokenizes");
    let expr = parse(&tokens).unwrap_or_else(|e| panic!("the fixture parses: {e:?}"));
    panproto_expr::eval(&expr, &Env::new(), &EvalConfig::default())
}

fn strings(values: &[&str]) -> Literal {
    Literal::List(values.iter().map(|s| Literal::Str((*s).into())).collect())
}

#[test]
fn a_builtin_passed_to_map_is_applied_to_every_element() {
    assert_eq!(
        eval(r#"map upper ["a", "b"]"#).expect("the map evaluates"),
        strings(&["A", "B"])
    );
}

#[test]
fn a_builtin_passed_to_fold_accumulates() {
    assert_eq!(
        eval("fold add 0 [1, 2, 3]").expect("the fold evaluates"),
        Literal::Int(6)
    );
}

#[test]
fn a_builtin_on_the_right_of_a_pipe_is_applied() {
    assert_eq!(
        eval(r#""hello" & upper"#).expect("the pipe evaluates"),
        Literal::Str("HELLO".into())
    );
}

#[test]
fn a_builtin_bound_by_let_is_applied_through_its_name() {
    assert_eq!(
        eval(r#"let f = upper in f "a""#).expect("the binding evaluates"),
        Literal::Str("A".into())
    );
}

#[test]
fn a_partially_applied_builtin_is_a_function() {
    assert_eq!(
        eval("let inc = add 1 in inc 41").expect("the partial application evaluates"),
        Literal::Int(42)
    );
}

#[test]
fn a_binding_shadows_the_builtin_of_the_same_name() {
    assert_eq!(
        eval("let upper = 7 in upper").expect("the binding evaluates"),
        Literal::Int(7)
    );
}

#[test]
fn a_lambda_parameter_shadows_the_builtin_of_the_same_name() {
    assert_eq!(
        eval(r"(\upper -> upper) 7").expect("the application evaluates"),
        Literal::Int(7)
    );
}

#[test]
fn a_name_that_is_no_builtin_is_still_unbound() {
    assert!(
        matches!(
            eval("no_such_function"),
            Err(ExprError::UnboundVariable(ref name)) if name == "no_such_function"
        ),
        "a free variable naming nothing is still an error"
    );
}
