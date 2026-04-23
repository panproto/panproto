//! Tests for typed holes (`Term::Hole`) and [`typecheck_term_with_holes`].

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use panproto_gat::{
    Implicit, Operation, Sort, SortExpr, Term, Theory, VarContext, typecheck_term_with_holes,
};

fn nat_theory() -> Theory {
    let sorts = vec![Sort::simple("Nat")];
    let ops = vec![
        Operation::nullary("zero", SortExpr::Name(Arc::from("Nat"))),
        Operation::with_implicit(
            "add",
            vec![
                (
                    Arc::from("x"),
                    SortExpr::Name(Arc::from("Nat")),
                    Implicit::No,
                ),
                (
                    Arc::from("y"),
                    SortExpr::Name(Arc::from("Nat")),
                    Implicit::No,
                ),
            ],
            SortExpr::Name(Arc::from("Nat")),
        ),
    ];
    Theory::new("ThNat", sorts, ops, vec![])
}

#[test]
fn hole_report_at_simple_site() {
    let th = nat_theory();
    let ctx = VarContext::default();
    // add(?, zero()) reports one hole whose expected sort is Nat.
    let term = Term::App {
        op: Arc::from("add"),
        args: vec![
            Term::Hole { name: None },
            Term::App {
                op: Arc::from("zero"),
                args: vec![],
            },
        ],
    };
    let (sort, reports) = typecheck_term_with_holes(&term, &ctx, &th).unwrap();
    assert_eq!(&*sort.head().clone(), "Nat");
    assert_eq!(reports.len(), 1);
    assert_eq!(&*reports[0].expected.head().clone(), "Nat");
    assert!(reports[0].name.is_none());
}

#[test]
fn hole_reports_include_context() {
    // Set up a theory with a closed sort so we can exercise a case
    // binder and verify the hole inside the branch body sees the binder
    // in its context.
    let sorts = vec![
        Sort::closed("Nat", vec![], vec![Arc::from("zero"), Arc::from("succ")]),
        Sort::simple("Bool"),
    ];
    let ops = vec![
        Operation::nullary("zero", SortExpr::Name(Arc::from("Nat"))),
        Operation::with_implicit(
            "succ",
            vec![(
                Arc::from("n"),
                SortExpr::Name(Arc::from("Nat")),
                Implicit::No,
            )],
            SortExpr::Name(Arc::from("Nat")),
        ),
        Operation::with_implicit(
            "is_zero",
            vec![(
                Arc::from("m"),
                SortExpr::Name(Arc::from("Nat")),
                Implicit::No,
            )],
            SortExpr::Name(Arc::from("Bool")),
        ),
    ];
    let th = Theory::new("ThNatCase", sorts, ops, vec![]);

    let term = Term::Case {
        scrutinee: Box::new(Term::App {
            op: Arc::from("zero"),
            args: vec![],
        }),
        branches: vec![
            panproto_gat::CaseBranch {
                constructor: Arc::from("zero"),
                binders: vec![],
                body: Term::App {
                    op: Arc::from("succ"),
                    args: vec![Term::Hole {
                        name: Some(Arc::from("bZero")),
                    }],
                },
            },
            panproto_gat::CaseBranch {
                constructor: Arc::from("succ"),
                binders: vec![Arc::from("pred")],
                body: Term::App {
                    op: Arc::from("succ"),
                    args: vec![Term::Hole {
                        name: Some(Arc::from("bSucc")),
                    }],
                },
            },
        ],
    };

    let ctx = VarContext::default();
    let (_sort, reports) = typecheck_term_with_holes(&term, &ctx, &th).unwrap();
    assert_eq!(reports.len(), 2);
    let succ_report = reports
        .iter()
        .find(|r| r.name.as_deref() == Some("bSucc"))
        .unwrap();
    assert!(succ_report.context.contains_key(&Arc::from("pred")));
    let zero_report = reports
        .iter()
        .find(|r| r.name.as_deref() == Some("bZero"))
        .unwrap();
    assert!(!zero_report.context.contains_key(&Arc::from("pred")));
}

#[test]
fn hole_name_is_preserved_in_report() {
    let th = nat_theory();
    let ctx = VarContext::default();
    let term = Term::App {
        op: Arc::from("add"),
        args: vec![
            Term::Hole {
                name: Some(Arc::from("foo")),
            },
            Term::App {
                op: Arc::from("zero"),
                args: vec![],
            },
        ],
    };
    let (_sort, reports) = typecheck_term_with_holes(&term, &ctx, &th).unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].name.as_deref(), Some("foo"));
}
