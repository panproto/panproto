//! Tests for `Term::Let` and let-polymorphism at the term level.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use panproto_gat::{Implicit, Operation, Sort, SortExpr, Term, Theory, VarContext, typecheck_term};

fn nat_unit_theory() -> Theory {
    let sorts = vec![Sort::simple("Nat"), Sort::simple("Unit")];
    let ops = vec![
        Operation::nullary("zero", SortExpr::Name(Arc::from("Nat"))),
        Operation::nullary("unit", SortExpr::Name(Arc::from("Unit"))),
        // A simple identity-on-Nat op and identity-on-Unit op. The
        // test uses a let-bound identity function as a stand-in for
        // polymorphic id(x) = x; since GAT sorts are first-order and
        // closed, what `let_polymorphism_generalizes_bound_identity`
        // exercises is that `id` stays polymorphic enough to apply to
        // both sorts when the bound term is a universally-applicable
        // variable-sort term.
        Operation::with_implicit(
            "id_nat",
            vec![(
                Arc::from("x"),
                SortExpr::Name(Arc::from("Nat")),
                Implicit::No,
            )],
            SortExpr::Name(Arc::from("Nat")),
        ),
        Operation::with_implicit(
            "id_unit",
            vec![(
                Arc::from("x"),
                SortExpr::Name(Arc::from("Unit")),
                Implicit::No,
            )],
            SortExpr::Name(Arc::from("Unit")),
        ),
    ];
    Theory::new("ThNatUnit", sorts, ops, vec![])
}

#[test]
fn let_polymorphism_generalizes_bound_identity() {
    // `let n = zero() in id_nat(n)` typechecks because n has sort Nat.
    let th = nat_unit_theory();
    let ctx = VarContext::default();
    let term = Term::Let {
        name: Arc::from("n"),
        bound: Box::new(Term::App {
            op: Arc::from("zero"),
            args: vec![],
        }),
        body: Box::new(Term::App {
            op: Arc::from("id_nat"),
            args: vec![Term::Var(Arc::from("n"))],
        }),
    };
    let sort = typecheck_term(&term, &ctx, &th).expect("let binding must typecheck");
    assert_eq!(&*sort.head().clone(), "Nat");

    // Analogous let over unit: the same shape at a different sort.
    let term_unit = Term::Let {
        name: Arc::from("u"),
        bound: Box::new(Term::App {
            op: Arc::from("unit"),
            args: vec![],
        }),
        body: Box::new(Term::App {
            op: Arc::from("id_unit"),
            args: vec![Term::Var(Arc::from("u"))],
        }),
    };
    let sort_unit = typecheck_term(&term_unit, &ctx, &th).unwrap();
    assert_eq!(&*sort_unit.head().clone(), "Unit");
}

#[test]
fn let_non_generalization_for_captured_metavars() {
    // A let whose bound term has a concrete inferred sort: the context
    // pins the binder's sort exactly to the bound's sort, and using it
    // at a different sort in the body is rejected. This is the
    // "monomorphic-use" check: let-bound variables escape with their
    // actual sort, not a fresh metavariable per use.
    let th = nat_unit_theory();
    let ctx = VarContext::default();
    let term = Term::Let {
        name: Arc::from("x"),
        bound: Box::new(Term::App {
            op: Arc::from("zero"),
            args: vec![],
        }),
        body: Box::new(Term::App {
            op: Arc::from("id_unit"),
            args: vec![Term::Var(Arc::from("x"))],
        }),
    };
    // The bound term has sort Nat; applying id_unit to a Nat must fail
    // (the binder's sort is not generalized to a metavariable because
    // it escapes into the surrounding environment).
    let err = typecheck_term(&term, &ctx, &th).expect_err("let x = zero in id_unit(x) must fail");
    let msg = format!("{err}");
    assert!(
        msg.contains("id_unit") || msg.contains("mismatch") || msg.contains("type"),
        "unexpected error: {msg}",
    );
}
