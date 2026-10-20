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
//!
//! It also holds deadline guards: each runs a fixpoint computation
//! (class naming in `compose`, the free model's quotient, `quotient`'s
//! union-find, the signature check's sort ordering) on a worker thread and
//! requires its exact result within a few seconds, so a computation that
//! stops terminating fails here quickly rather than hanging the suite.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use panproto_gat::{
    ClashPolicy, ComposeOptions, CompositionError, CompositionMode, DEFAULT_VERIFY_BUDGET,
    Equation, FreeModelConfig, ModelValue, NotRepresentableReason, OpAssignment, Operation,
    SignatureDefect, SignatureSite, Sort, SortExpr, SortParam, SymRef, Term, Theory, TheoryDiagram,
    TheoryMorphism, compose, free_model, quotient, typecheck_signature,
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

/// Run `work` on a worker thread and return its result, failing if it
/// does not finish within the deadline.
fn within_deadline<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("the computation finishes within five seconds")
}

fn simple_theory(name: &str, sorts: &[&str], ops: Vec<Operation>, eqs: Vec<Equation>) -> Theory {
    Theory::new(
        name,
        sorts.iter().map(|s| Sort::simple(*s)).collect(),
        ops,
        eqs,
    )
}

fn renaming(dom: &str, cod: &str, sorts: &[(&str, &str)]) -> TheoryMorphism {
    TheoryMorphism::new(
        format!("{dom}_to_{cod}"),
        dom,
        cod,
        sorts
            .iter()
            .map(|(a, b)| (Arc::from(*a), Arc::from(*b)))
            .collect(),
        HashMap::<Arc<str>, Arc<str>>::new(),
    )
}

#[test]
fn compose_names_its_classes_within_the_deadline() {
    let (sorts, ops) = within_deadline(|| {
        let diagram = TheoryDiagram::span(
            ("Sh", simple_theory("Sh", &["S"], vec![], vec![])),
            (
                "T1",
                simple_theory(
                    "T1",
                    &["S", "X"],
                    vec![Operation::unary("f", "s", "S", "X")],
                    vec![],
                ),
            ),
            ("T2", simple_theory("T2", &["S", "Y"], vec![], vec![])),
            renaming("Sh", "T1", &[("S", "S")]),
            renaming("Sh", "T2", &[("S", "S")]),
        );
        let composition = compose(&diagram, &options()).expect("the span composes");
        let mut sorts: Vec<String> = composition
            .theory
            .sorts
            .iter()
            .map(|s| s.name.to_string())
            .collect();
        sorts.sort();
        let ops: Vec<String> = composition
            .theory
            .ops
            .iter()
            .map(|o| o.name.to_string())
            .collect();
        (sorts, ops)
    });
    assert_eq!(sorts, ["S", "X", "Y"]);
    assert_eq!(ops, ["f"]);
}

/// The carrier of `sort` in the free model of `theory` to `depth`, as
/// sorted strings.
fn free_carrier(theory: &Theory, depth: usize, sort: &str) -> Vec<String> {
    let result = free_model(
        theory,
        &FreeModelConfig {
            max_depth: depth,
            max_terms_per_sort: 1000,
        },
    )
    .expect("the free model is built");
    let mut values: Vec<String> = result.model.sort_interp[sort]
        .iter()
        .map(|v| match v {
            ModelValue::Str(s) => s.clone(),
            other => format!("{other:?}"),
        })
        .collect();
    values.sort();
    values
}

#[test]
fn the_free_model_quotient_reaches_its_fixpoint_within_the_deadline() {
    let (retract, alternating) = within_deadline(|| {
        let retract = simple_theory(
            "Retract",
            &["A"],
            vec![
                Operation::nullary("a", "A"),
                Operation::nullary("b", "A"),
                Operation::unary("f", "x", "A", "A"),
            ],
            vec![Equation::new(
                "retract",
                Term::app("f", vec![Term::var("x")]),
                Term::var("x"),
            )],
        );
        let alternating = simple_theory(
            "Alternating",
            &["A", "B"],
            vec![
                Operation::nullary("a", "A"),
                Operation::nullary("b", "A"),
                Operation::unary("f", "x", "A", "B"),
                Operation::unary("g", "y", "B", "A"),
            ],
            vec![Equation::new(
                "ab",
                Term::constant("a"),
                Term::constant("b"),
            )],
        );
        (
            free_carrier(&retract, 2, "A"),
            free_carrier(&alternating, 2, "A"),
        )
    });
    assert_eq!(retract, ["a()", "b()"]);
    assert_eq!(alternating, ["a()", "g(f(a()))"]);
}

#[test]
fn quotient_merges_classes_within_the_deadline() {
    let sorts = within_deadline(|| {
        let theory = simple_theory(
            "Three",
            &["C", "B", "A"],
            vec![Operation::unary("f", "x", "B", "C")],
            vec![],
        );
        let merged = quotient(
            &theory,
            &[
                (Arc::from("C"), Arc::from("B")),
                (Arc::from("B"), Arc::from("A")),
            ],
        )
        .expect("the quotient is defined");
        let ops: Vec<(String, String, String)> = merged
            .ops
            .iter()
            .map(|o| {
                (
                    o.name.to_string(),
                    o.inputs[0].1.to_string(),
                    o.output.to_string(),
                )
            })
            .collect();
        let sorts: Vec<String> = merged.sorts.iter().map(|s| s.name.to_string()).collect();
        (sorts, ops)
    });
    assert_eq!(
        sorts,
        (
            vec!["A".to_owned()],
            vec![("f".to_owned(), "A".to_owned(), "A".to_owned())]
        )
    );
}

#[test]
fn signature_checking_orders_sorts_within_the_deadline() {
    let (chain, cycle, empty) = within_deadline(|| {
        // Declared against dependency order: `C` over `B` over `A`.
        let chain = Theory::new(
            "Chain",
            vec![
                Sort::dependent("C", vec![SortParam::new("y", "B")]),
                Sort::dependent("B", vec![SortParam::new("x", "A")]),
                Sort::simple("A"),
            ],
            vec![],
            vec![],
        );
        let cycle = Theory::new(
            "Cycle",
            vec![
                Sort::dependent("P", vec![SortParam::new("x", "Q")]),
                Sort::dependent("Q", vec![SortParam::new("y", "P")]),
            ],
            vec![],
            vec![],
        );
        let empty = Theory::new("Empty", vec![], vec![], vec![]);
        (
            typecheck_signature(&chain),
            typecheck_signature(&cycle),
            typecheck_signature(&empty),
        )
    });
    assert_eq!(chain, Ok(()));
    assert_eq!(
        cycle,
        Err(vec![
            SignatureDefect::UndeclaredSort {
                site: SignatureSite::SortParam {
                    sort: Arc::from("P"),
                    index: 0,
                },
                sort: Arc::from("Q"),
            },
            SignatureDefect::UndeclaredSort {
                site: SignatureSite::SortParam {
                    sort: Arc::from("Q"),
                    index: 0,
                },
                sort: Arc::from("P"),
            },
        ])
    );
    assert_eq!(empty, Ok(()));
}
