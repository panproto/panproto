//! Elimination whose result sort mentions the index being eliminated.
//!
//! A `Case` carries no motive, so with nothing expected the only sort
//! available is the one read off the first branch, and every later
//! branch has to agree with it. That is exactly what an eliminator over
//! an indexed family cannot do: in `eval : (t: Ty, Expr(t)) -> El(t)`
//! the `IntLit` branch has sort `El(int_code)` and the `BoolLit` branch
//! `El(bool_code)`, and neither is wrong.
//!
//! Pushing the declared output sort in as the motive turns the
//! comparison around: each branch is checked against the motive refined
//! by that branch's own substitution, so the branches are measured
//! against the result sort rather than against each other.
//!
//! The theory is a universe of two codes, `Expr(t)` closed over one
//! literal per code, and `El(t)` for the values.

use std::sync::Arc;

use panproto_gat::{
    CaseBranch, Equation, GatError, Implicit, Operation, Sort, SortExpr, SortParam, Term, Theory,
    typecheck_theory,
};

fn el_of(index: Term) -> SortExpr {
    SortExpr::App {
        name: Arc::from("El"),
        args: vec![index],
    }
}

fn expr_of(index: Term) -> SortExpr {
    SortExpr::App {
        name: Arc::from("Expr"),
        args: vec![index],
    }
}

fn int_code() -> Term {
    Term::app("int_code", vec![])
}

fn bool_code() -> Term {
    Term::app("bool_code", vec![])
}

/// `Ty` closed over two codes, `Expr(t)` closed over one literal each,
/// `El(t)` for values, and `R` for a result that ignores the index.
/// Only the equation under test varies.
fn expr_theory(eqs: Vec<Equation>) -> Theory {
    let ty = Sort::closed(
        "Ty",
        Vec::new(),
        [Arc::from("int_code") as Arc<str>, Arc::from("bool_code")],
    );
    let el = Sort::dependent("El", vec![SortParam::new("t", "Ty")]);
    let expr = Sort::closed(
        "Expr",
        vec![SortParam::new("t", "Ty")],
        [Arc::from("IntLit") as Arc<str>, Arc::from("BoolLit")],
    );
    let r = Sort::simple("R");

    let ops = vec![
        Operation::nullary("int_code", "Ty"),
        Operation::nullary("bool_code", "Ty"),
        Operation {
            name: Arc::from("IntLit"),
            inputs: vec![(Arc::from("v"), el_of(int_code()), Implicit::No)],
            output: expr_of(int_code()),
        },
        Operation {
            name: Arc::from("BoolLit"),
            inputs: vec![(Arc::from("v"), el_of(bool_code()), Implicit::No)],
            output: expr_of(bool_code()),
        },
        Operation::nullary("tag", "R"),
        // A value at a fixed code, for writing a branch that returns
        // the wrong thing on purpose.
        Operation::nullary("mkint", el_of(int_code())),
        // The result sort mentions the index.
        Operation {
            name: Arc::from("eval"),
            inputs: vec![
                (Arc::from("t"), SortExpr::from("Ty"), Implicit::No),
                (Arc::from("e"), expr_of(Term::var("t")), Implicit::No),
            ],
            output: el_of(Term::var("t")),
        },
        // The result sort ignores it.
        Operation {
            name: Arc::from("describe"),
            inputs: vec![
                (Arc::from("t"), SortExpr::from("Ty"), Implicit::No),
                (Arc::from("e"), expr_of(Term::var("t")), Implicit::No),
            ],
            output: SortExpr::from("R"),
        },
    ];

    Theory::new("ExprTh", vec![ty, el, expr, r], ops, eqs)
}

fn branches(int_body: Term, bool_body: Term) -> Vec<CaseBranch> {
    vec![
        CaseBranch {
            constructor: Arc::from("IntLit"),
            binders: vec![Arc::from("v")],
            body: int_body,
        },
        CaseBranch {
            constructor: Arc::from("BoolLit"),
            binders: vec![Arc::from("v")],
            body: bool_body,
        },
    ]
}

/// `<op>(t, e) = case e of ...`.
fn def_eq(op: &str, int_body: Term, bool_body: Term) -> Equation {
    Equation::new(
        format!("{op}_def"),
        Term::app(op, vec![Term::var("t"), Term::var("e")]),
        Term::Case {
            scrutinee: Box::new(Term::var("e")),
            branches: branches(int_body, bool_body),
        },
    )
}

/// The case the issue reports. Each branch returns its own bound value,
/// whose sort is the motive refined by that branch: `El(int_code)` in
/// the `IntLit` branch and `El(bool_code)` in the `BoolLit` branch.
#[test]
fn a_result_sort_that_mentions_the_index_typechecks() {
    let theory = expr_theory(vec![def_eq("eval", Term::var("v"), Term::var("v"))]);
    let result = typecheck_theory(&theory);
    assert!(
        result.is_ok(),
        "eval : (t, Expr(t)) -> El(t) should typecheck, got {result:?}",
    );
}

/// The branches have genuinely different sorts here, and both are
/// correct. Nothing in the checker may require them to agree.
#[test]
fn branches_are_measured_against_the_motive_not_against_each_other() {
    let theory = expr_theory(vec![def_eq("eval", Term::var("v"), Term::var("v"))]);
    assert!(typecheck_theory(&theory).is_ok());

    // Swapping the bodies makes each branch return the other's sort,
    // which no refinement licenses.
    let swapped = expr_theory(vec![def_eq(
        "eval",
        Term::app("mkint", vec![]),
        Term::app("mkint", vec![]),
    )]);
    let result = typecheck_theory(&swapped);
    let Err(GatError::CaseBranchSortMismatch {
        ref constructor, ..
    }) = result
    else {
        panic!(
            "a branch returning El(int_code) where El(bool_code) is required must fail, got {result:?}"
        );
    };
    assert_eq!(constructor.as_str(), "BoolLit");
}

/// The non-dependent path is unchanged: with a result sort that does
/// not mention the index, the refinement does not touch the motive and
/// every branch checks against the same sort.
#[test]
fn a_result_sort_that_ignores_the_index_still_typechecks() {
    let theory = expr_theory(vec![def_eq(
        "describe",
        Term::app("tag", vec![]),
        Term::app("tag", vec![]),
    )]);
    let result = typecheck_theory(&theory);
    assert!(
        result.is_ok(),
        "describe : (t, Expr(t)) -> R should typecheck, got {result:?}",
    );
}

/// A branch that does not produce the declared result sort is still
/// rejected, and the error names the branch and the sort the motive
/// requires there rather than two branch sorts.
#[test]
fn a_branch_that_misses_the_motive_is_rejected_by_name() {
    let theory = expr_theory(vec![def_eq(
        "describe",
        Term::app("tag", vec![]),
        Term::app("mkint", vec![]),
    )]);
    let result = typecheck_theory(&theory);
    let Err(GatError::CaseBranchSortMismatch {
        ref constructor,
        ref expected,
        ref got,
    }) = result
    else {
        panic!("a branch returning El(int_code) where R is required must fail, got {result:?}");
    };
    assert_eq!(constructor.as_str(), "BoolLit");
    assert_eq!(expected.as_str(), "R");
    assert_eq!(got.as_str(), "El(int_code())");
}
