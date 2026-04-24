//! Tests for `SortExpr::alpha_eq_modulo_rewrites`.

#![allow(clippy::unwrap_used)]

use panproto_gat::{DirectedEquation, SortExpr, Term};

fn rule(name: &str, lhs: Term, rhs: Term) -> DirectedEquation {
    DirectedEquation::new(name, lhs, rhs, panproto_expr::Expr::Var("_".into()))
}

#[test]
fn definitional_equality_joins_via_rewrite() {
    // Rule: arrow(a, b) -> fun(a, b)
    let rewrite = rule(
        "arrow_to_fun",
        Term::app("arrow", vec![Term::var("a"), Term::var("b")]),
        Term::app("fun", vec![Term::var("a"), Term::var("b")]),
    );
    // Sorts `P(arrow(Int, Int))` vs `P(fun(Int, Int))`
    let lhs = SortExpr::app(
        "P",
        vec![Term::app(
            "arrow",
            vec![Term::constant("Int"), Term::constant("Int")],
        )],
    );
    let rhs = SortExpr::app(
        "P",
        vec![Term::app(
            "fun",
            vec![Term::constant("Int"), Term::constant("Int")],
        )],
    );

    assert!(!lhs.alpha_eq(&rhs), "strict alpha_eq must reject the pair");
    assert!(
        lhs.alpha_eq_modulo_rewrites(&rhs, &[rewrite], 100),
        "relaxed equality should join via the rewrite"
    );
}

#[test]
fn definitional_equality_respects_step_limit() {
    // Rule: f(x) -> f(f(x)) -- non-terminating
    let rewrite = rule(
        "expand",
        Term::app("f", vec![Term::var("x")]),
        Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
    );
    let lhs = SortExpr::app("S", vec![Term::app("f", vec![Term::constant("c")])]);
    let rhs = SortExpr::app("S", vec![Term::constant("d")]);
    // A tight step budget must not panic or hang; correctness of the
    // answer is secondary here, the invariant is termination.
    let _ = lhs.alpha_eq_modulo_rewrites(&rhs, &[rewrite], 5);
    // Same sort under tight budget remains equal.
    let same = SortExpr::app("S", vec![Term::constant("c")]);
    assert!(same.alpha_eq_modulo_rewrites(&same, &[], 1));
    // Both sides terminate at different normal forms under the
    // non-terminating rule; the relaxed equality must still return a
    // boolean (not loop) within the step budget.
    let left = SortExpr::app("S", vec![Term::app("f", vec![Term::var("x")])]);
    let right = SortExpr::app("S", vec![Term::app("f", vec![Term::var("y")])]);
    let _guarded: bool = left.alpha_eq_modulo_rewrites(&right, &[], 3);
    // The key observation: the query returned rather than looping.
    let non_term = rule(
        "expand_alias",
        Term::app("f", vec![Term::var("x")]),
        Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
    );
    let _ = left.alpha_eq_modulo_rewrites(&left, &[non_term], 3);
}

#[test]
fn strict_alpha_eq_unchanged_by_rewrites() {
    let sort = SortExpr::app("P", vec![Term::constant("c")]);
    let rewrite = rule("c_to_d", Term::constant("c"), Term::constant("d"));
    // Without the new API, strict alpha_eq is unaffected; both sides
    // remain equal since the terms are unchanged.
    assert!(sort.alpha_eq(&sort));
    // With the new API and a rewrite, two different sorts collapse to
    // the same normal form.
    let other = SortExpr::app("P", vec![Term::constant("d")]);
    assert!(!sort.alpha_eq(&other));
    assert!(sort.alpha_eq_modulo_rewrites(&other, &[rewrite], 10));
}
