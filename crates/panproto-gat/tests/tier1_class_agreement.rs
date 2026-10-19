//! Pin for the documented tier 1 representability limit.
//!
//! The pin `tier1_refuses_agreement_that_holds_only_modulo_term_equations`
//! uses distinct constants. The apex `S` declares `X`, `Y(x : X)`, `c : X`
//! and `h : Y(c)`; `L` declares `a : X` and `f : Y(a)`; `R` declares
//! `b : X` and `g : Y(b)`. The left leg renames `c` to `a` and `h` to `f`.
//! The right leg sends `c` to the derived term `b()` and renames `h` to
//! `g`, which is a theory morphism because it sends `h`'s sort `Y(c)` to
//! `Y(b)`. Tier 2 adds the equation `a = b` (as `S.c = R.b`), and the class
//! `{S.h, L.f, R.g}` asks `Y(class of a)` and `Y(class of b)` to agree.
//!
//! The complete fibre-span family now lives in
//! `panproto_reference::enumerate::theories::fibre_spans` and is compared
//! through the public production and reference `compose` functions by the
//! `reference_theories` adapter. This file retains the contract pin whose
//! body is fixed by P5-O909.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{
    ClashPolicy, ComposeOptions, CompositionError, CompositionMode, DEFAULT_VERIFY_BUDGET,
    NotRepresentableReason, OpAssignment, Operation, Sort, SortExpr, SortParam, SymRef, Term,
    Theory, TheoryDiagram, TheoryMorphism, compose,
};

/// A theory with `X`, `Y(x : X)`, a constant `constant : X` and a
/// constant `fibre : Y(constant)`.
fn fibred(name: &str, constant: &str, fibre: &str) -> Theory {
    Theory::new(
        name,
        vec![
            Sort::simple("X"),
            Sort::dependent("Y", vec![SortParam::new("x", "X")]),
        ],
        vec![
            Operation::new(constant, vec![], "X"),
            Operation::new(
                fibre,
                vec![],
                SortExpr::app("Y", vec![Term::constant(constant)]),
            ),
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

fn options() -> ComposeOptions {
    ComposeOptions {
        mode: CompositionMode::ExplicitIdentity,
        result_name: Arc::from("Q"),
        clash: ClashPolicy::Freshen,
        rewrite_budget: DEFAULT_VERIFY_BUDGET,
    }
}

#[test]
fn tier1_refuses_agreement_that_holds_only_modulo_term_equations() {
    let diagram = TheoryDiagram::span(
        ("S", fibred("S", "c", "h")),
        ("L", fibred("L", "a", "f")),
        ("R", fibred("R", "b", "g")),
        leg(
            "S",
            "L",
            vec![("c", OpAssignment::op("a")), ("h", OpAssignment::op("f"))],
        ),
        leg(
            "S",
            "R",
            vec![
                ("c", OpAssignment::Term(Term::constant("b"))),
                ("h", OpAssignment::op("g")),
            ],
        ),
    );
    let outcome = compose(&diagram, &options());
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
    // The members over the class of `a` are `S.h` and `L.f`; the member
    // over the class of `b` is `R.g`.
    let over_a = [SymRef::new("L", "f"), SymRef::new("S", "h")];
    let over_b = SymRef::new("R", "g");
    assert!(
        (over_a.contains(&first) && second == over_b)
            || (over_a.contains(&second) && first == over_b),
        "the refused pair is a member over a and the member over b: {first}, {second}"
    );
}
