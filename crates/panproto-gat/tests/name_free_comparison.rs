//! Signatures are compared by their name-free forms.
//!
//! The comparisons of signatures (`signatures_equivalent_modulo_param_rename`,
//! `sort_params_equivalent_modulo_rename`, the class comparisons of
//! `compose` and `quotient`, and `check_morphism`) do not rely on
//! `typecheck_signature` having refused repeated binder names. Each
//! variable resolves to the nearest enclosing binder of its name (the
//! earlier binders of the signature, then the `Let` and `Case` binders
//! around it); two bound occurrences agree iff they resolve to the same
//! binder position, and two free occurrences agree iff they have the same
//! name. Each comparison is therefore symmetric and exact on every input.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{
    GatError, Implicit, OpAssignment, Operation, Sort, SortExpr, SortParam, Term, Theory,
    TheoryMorphism, check_morphism, quotient, signatures_equivalent_modulo_param_rename,
    sort_params_equivalent_modulo_rename,
};

fn inputs(names: &[&str]) -> Vec<(Arc<str>, SortExpr, Implicit)> {
    names
        .iter()
        .map(|n| (Arc::from(*n), SortExpr::from("X"), Implicit::No))
        .collect()
}

fn sorts_xyh() -> Vec<Sort> {
    vec![
        Sort::simple("X"),
        Sort::dependent("Y", vec![SortParam::new("x", "X")]),
        Sort::dependent(
            "H",
            vec![SortParam::new("x", "X"), SortParam::new("y", "X")],
        ),
    ]
}

fn sort_app(head: &str, vars: &[&str]) -> SortExpr {
    SortExpr::app(head, vars.iter().map(|v| Term::var(*v)).collect())
}

fn binary(name: &str, a: &str, b: &str, out: SortExpr) -> Operation {
    Operation::new(
        name,
        vec![(a.into(), "X".into()), (b.into(), "X".into())],
        out,
    )
}

#[test]
fn signature_equivalence_is_exact_and_symmetric_under_repeated_binders() {
    let ab = inputs(&["a", "b"]);
    let zz = inputs(&["z", "z"]);
    let both = |l: &[(Arc<str>, SortExpr, Implicit)], lo: &SortExpr, r: &[_], ro: &SortExpr| {
        (
            signatures_equivalent_modulo_param_rename(l, lo, r, ro),
            signatures_equivalent_modulo_param_rename(r, ro, l, lo),
        )
    };
    // (a, b) : Y(a) names the first input; (z, z) : Y(z) names the second.
    let first = both(&ab, &sort_app("Y", &["a"]), &zz, &sort_app("Y", &["z"]));
    eprintln!("observed (a, b) : Y(a) against (z, z) : Y(z): {first:?}");
    assert_eq!(first, (false, false));
    // (a, b) : Y(b) names the second input, as (z, z) : Y(z) does.
    let second = both(&ab, &sort_app("Y", &["b"]), &zz, &sort_app("Y", &["z"]));
    eprintln!("observed (a, b) : Y(b) against (z, z) : Y(z): {second:?}");
    assert_eq!(second, (true, true));
}

#[test]
fn sort_parameter_equivalence_is_exact_and_symmetric_under_repeated_binders() {
    let block = |a: &str, b: &str, dep: &str| {
        vec![
            SortParam::new(a, "X"),
            SortParam::new(b, "X"),
            SortParam::new("w", sort_app("Y", &[dep])),
        ]
    };
    let both = |l: &[SortParam], r: &[SortParam]| {
        (
            sort_params_equivalent_modulo_rename(l, r),
            sort_params_equivalent_modulo_rename(r, l),
        )
    };
    let first = both(&block("a", "b", "a"), &block("z", "z", "z"));
    eprintln!("observed (a, b, w : Y(a)) against (z, z, w : Y(z)): {first:?}");
    assert_eq!(first, (false, false));
    let second = both(&block("a", "b", "b"), &block("z", "z", "z"));
    eprintln!("observed (a, b, w : Y(b)) against (z, z, w : Y(z)): {second:?}");
    assert_eq!(second, (true, true));
}

#[test]
fn check_morphism_compares_binders_by_position_not_by_name() {
    let s = Theory::new(
        "S",
        sorts_xyh(),
        vec![binary("h", "x", "y", sort_app("H", &["x", "y"]))],
        vec![],
    );
    let r = Theory::new(
        "R",
        sorts_xyh(),
        vec![binary("f", "z", "z", sort_app("H", &["z", "z"]))],
        vec![],
    );
    let m = TheoryMorphism::new(
        "m",
        "S",
        "R",
        [("X", "X"), ("Y", "Y"), ("H", "H")]
            .into_iter()
            .map(|(a, b)| (Arc::from(a), Arc::from(b)))
            .collect(),
        HashMap::from([(Arc::from("h"), OpAssignment::op("f"))]),
    );
    let outcome = check_morphism(&m, &s, &r);
    eprintln!("observed: {outcome:?}");
    assert!(
        matches!(&outcome, Err(GatError::OpTypeMismatch { op, .. }) if op == "h"),
        "H(x, y) and H(z, z) under (z, z) differ in their first argument: {outcome:?}"
    );
}

#[test]
fn quotient_compares_members_with_repeated_binders_by_position() {
    let theory = Theory::new(
        "T",
        sorts_xyh(),
        vec![
            binary("f", "x", "y", sort_app("H", &["x", "y"])),
            binary("g", "z", "z", sort_app("H", &["z", "z"])),
        ],
        vec![],
    );
    let outcome = quotient(&theory, &[(Arc::from("f"), Arc::from("g"))]);
    eprintln!("observed: {outcome:?}");
    assert!(
        matches!(&outcome, Err(GatError::QuotientIncompatible { .. })),
        "{outcome:?}"
    );
}

#[test]
fn members_that_differ_only_by_a_let_binder_name_are_equivalent() {
    let let_in = |sig: &str, local: &str| {
        SortExpr::app(
            "Y",
            vec![Term::Let {
                name: local.into(),
                bound: Box::new(Term::var(sig)),
                body: Box::new(Term::var(local)),
            }],
        )
    };
    let x = inputs(&["x"]);
    let y = inputs(&["y"]);
    for (l, lo, r, ro) in [
        (&x, let_in("x", "u"), &x, let_in("x", "v")),
        (&x, let_in("x", "u"), &y, let_in("y", "v")),
    ] {
        let forth = signatures_equivalent_modulo_param_rename(l, &lo, r, &ro);
        let back = signatures_equivalent_modulo_param_rename(r, &ro, l, &lo);
        eprintln!("observed {lo} against {ro}: {forth}, {back}");
        assert!(forth && back, "{lo} against {ro}");
    }
    // A local binder that shadows the signature binder is not the
    // signature binder.
    let shadow = SortExpr::app(
        "Y",
        vec![Term::Let {
            name: "x".into(),
            bound: Box::new(Term::constant("c")),
            body: Box::new(Term::var("x")),
        }],
    );
    let outer = SortExpr::app(
        "Y",
        vec![Term::Let {
            name: "u".into(),
            bound: Box::new(Term::constant("c")),
            body: Box::new(Term::var("x")),
        }],
    );
    assert!(!signatures_equivalent_modulo_param_rename(
        &x, &shadow, &x, &outer
    ));
}
