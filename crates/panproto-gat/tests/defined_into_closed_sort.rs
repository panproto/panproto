//! Functions that produce an inhabitant of a closed family.
//!
//! A closed sort's closure lists its canonical forms, which is what makes
//! a `Case` over it exhaustive. That is a statement about constructors,
//! not about every operation whose output is the sort: `replicate : (n :
//! Nat, x : A) -> Vec(n)` is a function into `Vec`, defined by an equation
//! whose right side is built from `nil` and `cons`, and it introduces no
//! canonical form of its own. Refusing it left `Vec` with no functions
//! into it at all.
//!
//! An operation into a closed sort that is neither listed nor defined is
//! still refused, since it would be a stuck term no `Case` covers.

use std::sync::Arc;

use panproto_gat::{
    CaseBranch, Equation, GatError, Implicit, Operation, Sort, SortExpr, SortParam, Term, Theory,
    typecheck_theory,
};

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

/// `Nat` and `Vec(n)` both closed, `A` open, plus whatever `extra_ops`
/// and `eqs` the case under test needs.
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
    let mut ops = vec![
        Operation::nullary("zero", "Nat"),
        Operation::unary("succ", "n", "Nat", "Nat"),
        Operation::nullary("nil", vec_of(zero())),
        Operation {
            name: Arc::from("cons"),
            inputs: vec![
                (Arc::from("n"), SortExpr::from("Nat"), Implicit::Yes),
                (Arc::from("x"), SortExpr::from("A"), Implicit::No),
                (Arc::from("rest"), vec_of(Term::var("n")), Implicit::No),
            ],
            output: vec_of(succ(Term::var("n"))),
        },
    ];
    ops.extend(extra_ops);
    Theory::new("VecFns", vec![nat, vec, elem], ops, eqs)
}

/// `replicate : (n : Nat, x : A) -> Vec(n)`.
fn replicate_op() -> Operation {
    Operation {
        name: Arc::from("replicate"),
        inputs: vec![
            (Arc::from("n"), SortExpr::from("Nat"), Implicit::No),
            (Arc::from("x"), SortExpr::from("A"), Implicit::No),
        ],
        output: vec_of(Term::var("n")),
    }
}

/// `replicate(n, x) = case n of zero -> nil | succ m -> cons(x, replicate(m, x))`.
fn replicate_def() -> Equation {
    Equation::new(
        "replicate_def",
        Term::app("replicate", vec![Term::var("n"), Term::var("x")]),
        Term::Case {
            scrutinee: Box::new(Term::var("n")),
            branches: vec![
                CaseBranch {
                    constructor: Arc::from("zero"),
                    binders: Vec::new(),
                    body: Term::app("nil", vec![]),
                },
                CaseBranch {
                    constructor: Arc::from("succ"),
                    binders: vec![Arc::from("m")],
                    body: Term::app(
                        "cons",
                        vec![
                            Term::var("x"),
                            Term::app("replicate", vec![Term::var("m"), Term::var("x")]),
                        ],
                    ),
                },
            ],
        },
    )
}

/// The reported case: a defined function into a closed family typechecks,
/// with each branch checked against `Vec(n)` under its own refinement.
#[test]
fn a_defined_operation_may_produce_a_closed_sort() {
    let theory = vec_theory(vec![replicate_op()], vec![replicate_def()]);
    let result = typecheck_theory(&theory);
    assert!(
        result.is_ok(),
        "replicate : (n, x) -> Vec(n) with a defining equation should typecheck, got {result:?}",
    );
}

/// The closure rule is not simply dropped: an operation into a closed sort
/// that nothing defines is still an error, and the message says why.
#[test]
fn an_undefined_operation_into_a_closed_sort_is_still_refused() {
    let theory = vec_theory(vec![replicate_op()], Vec::new());
    let result = typecheck_theory(&theory);
    let Err(GatError::InvalidClosedSortConstructor {
        ref constructor,
        ref detail,
        ..
    }) = result
    else {
        panic!("an undefined operation into Vec must be refused, got {result:?}");
    };
    assert_eq!(constructor.as_str(), "replicate");
    assert!(detail.contains("nor defined by an equation"), "{detail}");
}

/// A definition counts only when its left side is the operation applied to
/// distinct variables. `replicate(zero, x) = nil` alone is a fact about one
/// argument, not a definition, so it does not license the operation.
#[test]
fn a_partial_equation_does_not_count_as_a_definition() {
    let partial = Equation::new(
        "replicate_at_zero",
        Term::app("replicate", vec![zero(), Term::var("x")]),
        Term::app("nil", vec![]),
    );
    let theory = vec_theory(vec![replicate_op()], vec![partial]);
    let result = typecheck_theory(&theory);
    assert!(
        matches!(result, Err(GatError::InvalidClosedSortConstructor { .. })),
        "an equation at a concrete argument is not a definition, got {result:?}",
    );
}

/// A defined function into the sort is not a canonical form, so a `Case`
/// over the sort still needs exactly the listed constructors and no branch
/// for the function.
#[test]
fn a_defined_operation_is_not_a_required_case_branch() {
    // `len : (n : Nat, v : Vec(n)) -> Nat`, matching on `v` with only
    // `nil` and `cons` branches, in a theory that also defines `replicate`.
    let len = Operation {
        name: Arc::from("len"),
        inputs: vec![
            (Arc::from("n"), SortExpr::from("Nat"), Implicit::No),
            (Arc::from("v"), vec_of(Term::var("n")), Implicit::No),
        ],
        output: SortExpr::from("Nat"),
    };
    let len_def = Equation::new(
        "len_def",
        Term::app("len", vec![Term::var("n"), Term::var("v")]),
        Term::Case {
            scrutinee: Box::new(Term::var("v")),
            branches: vec![
                CaseBranch {
                    constructor: Arc::from("nil"),
                    binders: Vec::new(),
                    body: zero(),
                },
                CaseBranch {
                    constructor: Arc::from("cons"),
                    binders: vec![Arc::from("m"), Arc::from("x"), Arc::from("rest")],
                    body: succ(Term::app("len", vec![Term::var("m"), Term::var("rest")])),
                },
            ],
        },
    );
    let theory = vec_theory(vec![replicate_op(), len], vec![replicate_def(), len_def]);
    let result = typecheck_theory(&theory);
    assert!(
        result.is_ok(),
        "a case over Vec is exhaustive with nil and cons even though replicate is defined, got {result:?}",
    );
}

/// The other shape the issue names: a function that both eliminates and
/// produces the closed family, `vmap : (n : Nat, v : Vec(n)) -> Vec(n)`.
#[test]
fn a_function_from_the_closed_sort_to_itself_typechecks() {
    let f = Operation::unary("f", "x", "A", "A");
    let vmap = Operation {
        name: Arc::from("vmap"),
        inputs: vec![
            (Arc::from("n"), SortExpr::from("Nat"), Implicit::No),
            (Arc::from("v"), vec_of(Term::var("n")), Implicit::No),
        ],
        output: vec_of(Term::var("n")),
    };
    let vmap_def = Equation::new(
        "vmap_def",
        Term::app("vmap", vec![Term::var("n"), Term::var("v")]),
        Term::Case {
            scrutinee: Box::new(Term::var("v")),
            branches: vec![
                CaseBranch {
                    constructor: Arc::from("nil"),
                    binders: Vec::new(),
                    body: Term::app("nil", vec![]),
                },
                CaseBranch {
                    constructor: Arc::from("cons"),
                    binders: vec![Arc::from("m"), Arc::from("x"), Arc::from("rest")],
                    body: Term::app(
                        "cons",
                        vec![
                            Term::app("f", vec![Term::var("x")]),
                            Term::app("vmap", vec![Term::var("m"), Term::var("rest")]),
                        ],
                    ),
                },
            ],
        },
    );
    let theory = vec_theory(vec![f, vmap], vec![vmap_def]);
    let result = typecheck_theory(&theory);
    assert!(
        result.is_ok(),
        "vmap : (n, Vec(n)) -> Vec(n) should typecheck, got {result:?}",
    );
}

/// A binder may shadow a name free in the scrutinee's sort. `len (m, v :
/// Vec(m))` with a `cons m x rest` branch is ordinary, and the body uses
/// `rest` where a `Vec(m)` is required, so the binder's sort must be in
/// the binder's name and the outer `m` must have been refined rather
/// than unified against its own successor.
#[test]
fn a_binder_may_shadow_a_name_free_in_the_scrutinee_sort() {
    let len = Operation {
        name: Arc::from("len"),
        inputs: vec![
            (Arc::from("m"), SortExpr::from("Nat"), Implicit::No),
            (Arc::from("v"), vec_of(Term::var("m")), Implicit::No),
        ],
        output: SortExpr::from("Nat"),
    };
    let len_def = Equation::new(
        "len_def",
        Term::app("len", vec![Term::var("m"), Term::var("v")]),
        Term::Case {
            scrutinee: Box::new(Term::var("v")),
            branches: vec![
                CaseBranch {
                    constructor: Arc::from("nil"),
                    binders: Vec::new(),
                    body: zero(),
                },
                CaseBranch {
                    constructor: Arc::from("cons"),
                    binders: vec![Arc::from("m"), Arc::from("x"), Arc::from("rest")],
                    body: succ(Term::app("len", vec![Term::var("m"), Term::var("rest")])),
                },
            ],
        },
    );
    let theory = vec_theory(vec![len], vec![len_def]);
    let result = typecheck_theory(&theory);
    assert!(
        result.is_ok(),
        "a cons branch binding m under an eliminator whose own m is free in Vec(m) must typecheck, got {result:?}",
    );
}

/// The constructor's own parameter names never reach a binder's sort:
/// after `cons m x rest`, the tail is a `Vec(m)`, not a `Vec(n)` in the
/// constructor's spelling. Checked through inference on a standalone
/// case, where a wrong name would surface as an unbound variable.
#[test]
fn binder_sorts_are_in_the_branch_s_own_names() {
    use panproto_gat::{VarContext, typecheck_term};

    let theory = vec_theory(Vec::new(), Vec::new());
    let mut ctx = VarContext::default();
    ctx.insert(Arc::from("k"), SortExpr::from("Nat"));
    ctx.insert(Arc::from("v"), vec_of(succ(Term::var("k"))));
    // `case v of cons m x rest -> rest` has sort `Vec(m)` where `m` is the
    // binder, which the refinement identifies with `k`.
    let case = Term::Case {
        scrutinee: Box::new(Term::var("v")),
        branches: vec![CaseBranch {
            constructor: Arc::from("cons"),
            binders: vec![Arc::from("m"), Arc::from("x"), Arc::from("rest")],
            body: Term::var("rest"),
        }],
    };
    let result = typecheck_term(&case, &ctx, &theory);
    let Ok(sort) = result else {
        panic!("the tail of a non-empty vector must have a sort, got {result:?}");
    };
    assert_eq!(
        sort.to_string(),
        "Vec(k)",
        "the tail's length is the outer k, via the binder"
    );
}
