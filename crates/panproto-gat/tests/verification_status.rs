//! Verification reports what it established, not merely what it found.
//!
//! An empty violation list means two different things: every equation
//! was checked and holds, or no equation was checked at all. Reading
//! the second as the first reports a model verified that was never
//! examined, so [`verify_model`] keeps the two apart.
//!
//! The four cases here are the four ways a run can end: it passes, it
//! finds a violation, the theory does not typecheck, or the assignment
//! enumeration runs out of budget.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use panproto_gat::{
    CheckModelOptions, Equation, GatError, Incompleteness, Model, ModelValue, Operation, Sort,
    SortClosure, Term, Theory, VerificationStatus, verify_model,
};

/// A monoid: associative `mul` with a two-sided `unit`.
fn monoid_theory() -> Theory {
    Theory::new(
        "Monoid",
        vec![Sort::simple("Carrier")],
        vec![
            Operation::new(
                "mul",
                vec![
                    ("a".into(), "Carrier".into()),
                    ("b".into(), "Carrier".into()),
                ],
                "Carrier",
            ),
            Operation::nullary("unit", "Carrier"),
        ],
        vec![
            Equation::new(
                "assoc",
                Term::app(
                    "mul",
                    vec![
                        Term::var("a"),
                        Term::app("mul", vec![Term::var("b"), Term::var("c")]),
                    ],
                ),
                Term::app(
                    "mul",
                    vec![
                        Term::app("mul", vec![Term::var("a"), Term::var("b")]),
                        Term::var("c"),
                    ],
                ),
            ),
            Equation::new(
                "left_id",
                Term::app("mul", vec![Term::constant("unit"), Term::var("a")]),
                Term::var("a"),
            ),
        ],
    )
}

/// Addition modulo `n` on `{0, ..., n-1}`: a monoid for every `n`.
fn cyclic_model(n: i64) -> Model {
    let mut model = Model::new("Monoid");
    model.add_sort("Carrier", (0..n).map(ModelValue::Int).collect());
    model.add_op("mul", move |args: &[ModelValue]| {
        match (&args[0], &args[1]) {
            (ModelValue::Int(a), ModelValue::Int(b)) => Ok(ModelValue::Int((a + b) % n)),
            _ => Err(GatError::ModelError("expected Int".into())),
        }
    });
    model.add_op("unit", |_: &[ModelValue]| Ok(ModelValue::Int(0)));
    model
}

/// Subtraction modulo `n`: not associative, so `assoc` is refuted.
fn non_associative_model(n: i64) -> Model {
    let mut model = Model::new("Monoid");
    model.add_sort("Carrier", (0..n).map(ModelValue::Int).collect());
    model.add_op("mul", move |args: &[ModelValue]| {
        match (&args[0], &args[1]) {
            (ModelValue::Int(a), ModelValue::Int(b)) => Ok(ModelValue::Int((a - b).rem_euclid(n))),
            _ => Err(GatError::ModelError("expected Int".into())),
        }
    });
    model.add_op("unit", |_: &[ModelValue]| Ok(ModelValue::Int(0)));
    model
}

#[test]
fn a_completed_check_that_finds_nothing_passes() {
    let report = verify_model(
        &cyclic_model(5),
        &monoid_theory(),
        &CheckModelOptions::default(),
    );
    assert_eq!(report.status(), VerificationStatus::Passed);
    assert!(report.status().is_verified());
    assert!(report.incomplete.is_none());
}

#[test]
fn a_refuted_equation_fails() {
    let report = verify_model(
        &non_associative_model(5),
        &monoid_theory(),
        &CheckModelOptions::default(),
    );
    assert_eq!(report.status(), VerificationStatus::Failed);
    assert!(!report.status().is_verified());
    assert!(
        report.violations.iter().any(|v| &*v.equation == "assoc"),
        "expected assoc to be refuted, got {:?}",
        report.violations,
    );
}

/// A theory that does not typecheck has no equations that can be
/// checked against anything. Before this distinction existed the run
/// recorded no violations and was read as a pass.
#[test]
fn a_theory_that_does_not_typecheck_is_incomplete_not_passed() {
    let mut theory = monoid_theory();
    // Close `Carrier` over a constructor no operation produces, which
    // `typecheck_theory` rejects.
    theory.sorts[0].closure = SortClosure::Closed(vec![Arc::from("no_such_op")]);

    let report = verify_model(&cyclic_model(5), &theory, &CheckModelOptions::default());
    assert_eq!(report.status(), VerificationStatus::Incomplete);
    assert!(!report.status().is_verified());
    assert!(
        matches!(report.incomplete, Some(Incompleteness::TheoryTypeError(_))),
        "expected a theory type error, got {:?}",
        report.incomplete,
    );
    assert!(
        report.violations.is_empty(),
        "an unchecked theory reports no violations, which is exactly why \
         the empty list must not be read as a pass",
    );
}

/// An enumeration cut off at its budget checked some assignments and
/// not others, so it establishes nothing either way.
#[test]
fn an_exhausted_assignment_budget_is_incomplete_not_passed() {
    // `assoc` has three variables over a carrier of 5, so 125
    // assignments; a budget of 2 cannot cover it.
    let report = verify_model(
        &cyclic_model(5),
        &monoid_theory(),
        &CheckModelOptions { max_assignments: 2 },
    );
    assert_eq!(report.status(), VerificationStatus::Incomplete);
    assert!(
        matches!(report.incomplete, Some(Incompleteness::CheckError(_))),
        "expected a bounded-check error, got {:?}",
        report.incomplete,
    );
}

/// The same budget over a model small enough to fit completes, so the
/// budget alone is not what makes a run incomplete.
#[test]
fn a_budget_large_enough_for_the_carrier_completes() {
    let report = verify_model(
        &cyclic_model(2),
        &monoid_theory(),
        &CheckModelOptions { max_assignments: 8 },
    );
    assert_eq!(report.status(), VerificationStatus::Passed);
}
