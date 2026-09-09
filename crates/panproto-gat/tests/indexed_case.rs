//! Elimination over an indexed inductive family.
//!
//! A `Case` on an indexed sort must require exactly the constructors
//! the scrutinee's index admits. `nil : Vec(zero)` cannot have built a
//! `Vec(succ n)`, so a match on one needs no `nil` branch and admits
//! none; requiring the branch and then rejecting it made every
//! eliminator over an indexed family unwritable.
//!
//! The theory is `Vec(n : Nat)` closed over `nil` and `cons`, with
//! `head` total only on a non-empty vector.

use std::sync::Arc;

use panproto_gat::{
    CaseBranch, Equation, GatError, Implicit, Operation, Sort, SortExpr, SortParam, Term, Theory,
    typecheck_theory,
};

/// `Vec(index)`.
fn vec_of(index: Term) -> SortExpr {
    SortExpr::App {
        name: Arc::from("Vec"),
        args: vec![index],
    }
}

fn zero() -> Term {
    Term::app("zero", vec![])
}

fn succ(n: Term) -> Term {
    Term::app("succ", vec![n])
}

/// The shared sorts and operations. Only the `head` equation varies
/// between the cases under test.
fn vec_theory(extra_ops: Vec<Operation>, eqs: Vec<Equation>) -> Theory {
    let nat = Sort::closed(
        "Nat",
        Vec::new(),
        [Arc::from("zero") as Arc<str>, Arc::from("succ")],
    );
    let vec = Sort::closed(
        "Vec",
        vec![SortParam::new("n", "Nat")],
        [Arc::from("nil") as Arc<str>, Arc::from("cons")],
    );
    let elem = Sort::simple("A");

    let ops = vec![
        Operation::nullary("zero", "Nat"),
        Operation::unary("succ", "n", "Nat", "Nat"),
        Operation::nullary("nil", vec_of(zero())),
        Operation {
            name: Arc::from("cons"),
            inputs: vec![
                (Arc::from("n"), SortExpr::from("Nat"), Implicit::No),
                (Arc::from("x"), SortExpr::from("A"), Implicit::No),
                (Arc::from("v"), vec_of(Term::var("n")), Implicit::No),
            ],
            output: vec_of(succ(Term::var("n"))),
        },
        Operation::nullary("fallback", "A"),
        Operation {
            name: Arc::from("head"),
            inputs: vec![
                (Arc::from("n"), SortExpr::from("Nat"), Implicit::No),
                (Arc::from("v"), vec_of(succ(Term::var("n"))), Implicit::No),
            ],
            output: SortExpr::from("A"),
        },
    ];

    let mut ops = ops;
    ops.extend(extra_ops);
    Theory::new("VecTh", vec![nat, vec, elem], ops, eqs)
}

fn cons_branch() -> CaseBranch {
    CaseBranch {
        constructor: Arc::from("cons"),
        binders: vec![Arc::from("m"), Arc::from("x"), Arc::from("rest")],
        body: Term::var("x"),
    }
}

fn nil_branch() -> CaseBranch {
    CaseBranch {
        constructor: Arc::from("nil"),
        binders: Vec::new(),
        body: Term::app("fallback", vec![]),
    }
}

/// `head(n, v) = case v of ...`.
fn head_eq(branches: Vec<CaseBranch>) -> Equation {
    Equation::new(
        "head_def",
        Term::app("head", vec![Term::var("n"), Term::var("v")]),
        Term::Case {
            scrutinee: Box::new(Term::var("v")),
            branches,
        },
    )
}

/// `len(m, v : Vec(m))`: an operation whose scrutinee index is a bare
/// variable, so it refines nothing.
fn len_op() -> Operation {
    Operation {
        name: Arc::from("len"),
        inputs: vec![
            (Arc::from("m"), SortExpr::from("Nat"), Implicit::No),
            (Arc::from("v"), vec_of(Term::var("m")), Implicit::No),
        ],
        output: SortExpr::from("A"),
    }
}

/// `len(m, v) = case v of ...`.
fn len_eq(branches: Vec<CaseBranch>) -> Equation {
    Equation::new(
        "len_def",
        Term::app("len", vec![Term::var("m"), Term::var("v")]),
        Term::Case {
            scrutinee: Box::new(Term::var("v")),
            branches,
        },
    )
}

/// The case the issue reports: the scrutinee is `Vec(succ n)`, so
/// `cons` is the only reachable constructor and a `cons` branch alone
/// is exhaustive.
#[test]
fn index_refines_which_constructors_a_case_must_cover() {
    let theory = vec_theory(Vec::new(), vec![head_eq(vec![cons_branch()])]);
    let result = typecheck_theory(&theory);
    assert!(
        result.is_ok(),
        "a case on Vec(succ n) is exhaustive with only a cons branch, got {result:?}",
    );
}

/// The converse, and what makes the exclusion a real refinement rather
/// than mere leniency: a `nil` branch on a `Vec(succ n)` is not
/// optional-but-tolerated, it is unreachable and rejected as such.
#[test]
fn a_branch_the_index_excludes_is_rejected_as_unreachable() {
    let theory = vec_theory(Vec::new(), vec![head_eq(vec![nil_branch(), cons_branch()])]);
    let result = typecheck_theory(&theory);
    let Err(GatError::UnreachableCaseBranch {
        ref constructor, ..
    }) = result
    else {
        panic!("a nil branch on Vec(succ n) must be unreachable, got {result:?}");
    };
    assert_eq!(constructor.as_str(), "nil");
}

/// A scrutinee whose index is a bare variable refines nothing, so every
/// constructor stays required. This is the case that keeps a
/// non-dependent sort's behaviour unchanged.
#[test]
fn an_unrefined_index_still_requires_every_constructor() {
    // `len(m, v : Vec(m))` matches at an index that could be either.
    let theory = vec_theory(vec![len_op()], vec![len_eq(vec![cons_branch()])]);
    let result = typecheck_theory(&theory);
    let Err(GatError::NonExhaustiveCase { ref missing, .. }) = result else {
        panic!("a case on Vec(m) must still require nil, got {result:?}");
    };
    assert_eq!(missing, &["nil".to_string()]);
}

/// The exhaustiveness check and the branch checker now agree by
/// construction: every constructor the first requires, the second can
/// typecheck. Covering both admitted constructors of an unrefined
/// scrutinee therefore succeeds.
#[test]
fn covering_every_admitted_constructor_typechecks() {
    let theory = vec_theory(
        vec![len_op()],
        vec![len_eq(vec![nil_branch(), cons_branch()])],
    );
    let result = typecheck_theory(&theory);
    assert!(
        result.is_ok(),
        "a case covering both nil and cons at an unrefined index must typecheck, got {result:?}",
    );
}
