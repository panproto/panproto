//! Tier 1 class agreement compares signatures syntactically.
//!
//! [`compose`] checks that the members of an operation class agree once
//! every sort and operation they mention is sent to its class. The check
//! does not use the sort conversions that term equations induce: by the
//! substitution rule a term equation `a = b` makes `Y(a)` and `Y(b)`
//! derivably equal (garner2015combinatorial Table 1, stated), but the
//! comparison sees two different argument classes. An operation class
//! whose members differ only in arguments that a tier 2 equation equates
//! is therefore refused as not representable, although its colimit needs
//! no sort equation. The refusal is conservative: it rejects a diagram
//! whose colimit exists, and never accepts a class whose members
//! disagree.
//!
//! The diagram is a span over the apex `S`. Every node declares `X`,
//! `Y(x : X)`, a constant `a : X` and one constant of sort `Y(a)`: `h` in
//! `S`, `f` in `L`, `g` in `R`. The left leg renames `a` to `a` and `h` to
//! `f`, so `S.a` and `L.a` form one class. The right leg sends `a` to the
//! derived term `a` of `R` and renames `h` to `g`; a derived term joins no
//! class, so `R.a` is a class of its own (the `b` of the identification
//! `a = b`), and tier 2 adds the equation `S.a = R.a`. The class
//! `{S.h, L.f, R.g}` then asks `Y(class of L.a)` and `Y(class of R.a)` to
//! agree.
//!
//! `R`'s constant carries the apex's name because the leg check compares
//! the image of `Y(a)` with `g`'s declared sort without substituting the
//! derived-term image of `a` into it; with `R`'s constant named `b` and
//! `g : Y(b)` the right leg is refused as not a theory morphism before
//! any class is formed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{
    ClashPolicy, ComposeOptions, CompositionError, CompositionMode, DEFAULT_VERIFY_BUDGET,
    NotRepresentableReason, OpAssignment, Operation, Sort, SortExpr, SortParam, SymRef, Term,
    Theory, TheoryDiagram, TheoryMorphism, compose,
};

/// A theory with `X`, `Y(x : X)`, a constant `a : X` and a constant
/// `fibre : Y(a)`.
fn fibred(name: &str, fibre: &str) -> Theory {
    Theory::new(
        name,
        vec![
            Sort::simple("X"),
            Sort::dependent("Y", vec![SortParam::new("x", "X")]),
        ],
        vec![
            Operation::new("a", vec![], "X"),
            Operation::new(fibre, vec![], SortExpr::app("Y", vec![Term::constant("a")])),
        ],
        vec![],
    )
}

/// The identity on `X` and `Y`, with the given operation assignments.
fn leg(dom: &str, cod: &str, ops: Vec<(&str, OpAssignment)>) -> TheoryMorphism {
    TheoryMorphism::new(
        format!("{dom}_to_{cod}"),
        dom,
        cod,
        [("X", "X"), ("Y", "Y")]
            .into_iter()
            .map(|(s, t)| (Arc::from(s), Arc::from(t)))
            .collect(),
        ops.into_iter()
            .map(|(o, image)| (Arc::from(o), image))
            .collect::<HashMap<Arc<str>, OpAssignment>>(),
    )
}

/// The span, with the right leg's image of `a` given.
fn span(right_a: OpAssignment) -> TheoryDiagram {
    TheoryDiagram::span(
        ("S", fibred("S", "h")),
        ("L", fibred("L", "f")),
        ("R", fibred("R", "g")),
        leg(
            "S",
            "L",
            vec![("a", OpAssignment::op("a")), ("h", OpAssignment::op("f"))],
        ),
        leg("S", "R", vec![("a", right_a), ("h", OpAssignment::op("g"))]),
    )
}

fn options() -> ComposeOptions {
    ComposeOptions {
        mode: CompositionMode::ExplicitIdentity,
        result_name: Arc::from("Q"),
        clash: ClashPolicy::Freshen,
        rewrite_budget: DEFAULT_VERIFY_BUDGET,
    }
}

/// The diagram whose class agrees only modulo the tier 2 equation
/// `S.a = R.a`.
fn agreement_modulo_term_equation() -> TheoryDiagram {
    span(OpAssignment::Term(Term::constant("a")))
}

#[test]
fn tier1_refuses_agreement_that_holds_only_modulo_term_equations() {
    let outcome = compose(&agreement_modulo_term_equation(), &options());
    eprintln!("observed outcome: {outcome:?}");
    let error = outcome.expect_err("tier 1 compares signatures syntactically");
    let CompositionError::QuotientNotRepresentable {
        first,
        second,
        reason,
    } = error
    else {
        panic!("expected QuotientNotRepresentable, observed {error:?}");
    };
    assert_eq!(reason, NotRepresentableReason::NeedsSortEquation);
    let class = [
        SymRef::new("L", "f"),
        SymRef::new("R", "g"),
        SymRef::new("S", "h"),
    ];
    assert!(
        class.contains(&first) && class.contains(&second) && first != second,
        "the refused pair is two members of the class of h: {first}, {second}"
    );
}

#[test]
fn the_same_class_is_accepted_when_the_edges_identify_the_arguments() {
    // With `a` renamed on the right as well, `S.a`, `L.a` and `R.a` form
    // one class, so every member of the class of `h` has sort
    // `Y(class of a)` and tier 1 accepts it without any equation.
    let composition =
        compose(&span(OpAssignment::op("a")), &options()).expect("the class agrees syntactically");
    let mut ops: Vec<&str> = composition
        .theory
        .ops
        .iter()
        .map(|o| o.name.as_ref())
        .collect();
    ops.sort_unstable();
    assert_eq!(ops, ["a", "f"], "one class for a and one for h, f, g");
    assert!(composition.theory.eqs.is_empty(), "no tier 2 equation");
}

#[test]
fn the_refusal_reads_as_a_representability_limit_of_a_syntactic_check() {
    let error = compose(&agreement_modulo_term_equation(), &options())
        .expect_err("tier 1 compares signatures syntactically");
    let rendered = error.to_string();
    eprintln!("observed message: {rendered}");
    assert!(
        rendered.contains("sort equation") && rendered.contains("term equation"),
        "the message names both the representability limit and the syntactic \
         comparison: {rendered}"
    );
    assert!(
        !rendered.contains("needs a sort equation"),
        "the message does not claim that every refused identification needs a \
         sort equation: {rendered}"
    );
}

#[test]
fn quotient_reports_the_same_limit_for_dependent_sort_arguments() {
    // `quotient` runs the same class check; identifying `f : Y(a)` with
    // `g : Y(b)` while `a` and `b` stay apart differs only in a dependent
    // sort argument.
    let theory = Theory::new(
        "T",
        vec![
            Sort::simple("X"),
            Sort::dependent("Y", vec![SortParam::new("x", "X")]),
        ],
        vec![
            Operation::new("a", vec![], "X"),
            Operation::new("b", vec![], "X"),
            Operation::new("f", vec![], SortExpr::app("Y", vec![Term::constant("a")])),
            Operation::new("g", vec![], SortExpr::app("Y", vec![Term::constant("b")])),
        ],
        vec![],
    );
    let error = panproto_gat::quotient(&theory, &[(Arc::from("f"), Arc::from("g"))])
        .expect_err("the identified operations disagree syntactically");
    let panproto_gat::GatError::QuotientIncompatible { detail, .. } = &error else {
        panic!("expected QuotientIncompatible, observed {error:?}");
    };
    eprintln!("observed detail: {detail}");
    assert!(
        detail.contains("sort equation") && detail.contains("term equation"),
        "the detail names both the representability limit and the syntactic \
         comparison: {detail}"
    );
    assert!(
        !detail.contains("would need a sort equation"),
        "the detail does not claim that a sort equation is needed: {detail}"
    );
}
