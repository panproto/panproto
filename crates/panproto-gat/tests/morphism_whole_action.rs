//! `check_morphism` reads every sort expression through the morphism's
//! whole action.
//!
//! A theory morphism `m` acts on terms by `m^`
//! ([`TheoryMorphism::apply_to_term_substituting`]): an operation sent to
//! an operation is renamed, and an operation sent to a derived term is
//! replaced by that term with the mapped arguments substituted for the
//! operation's inputs. It acts on sort expressions by
//! `F(S(t_1, ..., t_n)) = sort_map(S)(m^(t_1), ..., m^(t_n))`, which is the
//! interpretation of lynch2024gatlab Sec 4.1 (stated): the map pushes
//! types forward along the whole assignment, derived terms included. The
//! tests below pin that every comparison of `check_morphism` (the
//! parameter sorts of a dependent sort, the inputs and output of a renamed
//! operation, the context and output of a derived-term assignment) uses
//! `F`, and that a constructor of a closed sort is read by its
//! constructor image: `c`'s image is `k` iff `m^` sends `c(x_1, ..., x_n)`
//! to `k(x_1, ..., x_n)`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{
    CaseBranch, Equation, GatError, OpAssignment, Operation, Sort, SortExpr, SortParam, Term,
    Theory, TheoryMorphism, check_morphism,
};

/// `X` and `Y(x : X)`.
fn base_sorts() -> Vec<Sort> {
    vec![
        Sort::simple("X"),
        Sort::dependent("Y", vec![SortParam::new("x", "X")]),
    ]
}

fn y_of(constant: &str) -> SortExpr {
    SortExpr::app("Y", vec![Term::constant(constant)])
}

fn constant(name: &str) -> Operation {
    Operation::new(name, vec![], "X")
}

/// A morphism with the identity on the given sorts and the given
/// operation assignments.
fn morphism(
    dom: &str,
    cod: &str,
    sorts: &[(&str, &str)],
    ops: Vec<(&str, OpAssignment)>,
) -> TheoryMorphism {
    TheoryMorphism::new(
        format!("{dom}_to_{cod}"),
        dom,
        cod,
        sorts
            .iter()
            .map(|(s, t)| (Arc::from(*s), Arc::from(*t)))
            .collect(),
        ops.into_iter()
            .map(|(o, image)| (Arc::from(o), image))
            .collect::<HashMap<Arc<str>, OpAssignment>>(),
    )
}

const XY: &[(&str, &str)] = &[("X", "X"), ("Y", "Y")];

#[test]
fn check_morphism_refuses_a_derived_term_image_that_changes_a_dependent_sort() {
    // c -> d() changes the image of h's sort to Y(d), but g : Y(c).
    let s = Theory::new(
        "S",
        base_sorts(),
        vec![
            constant("c"),
            constant("d"),
            Operation::new("h", vec![], y_of("c")),
        ],
        vec![],
    );
    let r = Theory::new(
        "R",
        base_sorts(),
        vec![
            constant("c"),
            constant("d"),
            Operation::new("g", vec![], y_of("c")),
        ],
        vec![],
    );
    let m = morphism(
        "S",
        "R",
        XY,
        vec![
            ("c", OpAssignment::Term(Term::constant("d"))),
            ("d", OpAssignment::op("d")),
            ("h", OpAssignment::op("g")),
        ],
    );
    let outcome = check_morphism(&m, &s, &r);
    eprintln!("observed: {outcome:?}");
    assert!(
        matches!(&outcome, Err(GatError::OpTypeMismatch { op, .. }) if op == "h"),
        "the image of h's sort is Y(d), not g's sort Y(c): {outcome:?}"
    );
}

#[test]
fn check_morphism_accepts_a_derived_term_image_that_matches_a_dependent_sort() {
    // The right leg of the literal pin fixture: c -> b() sends h : Y(c)
    // to Y(b), which is g's sort.
    let s = Theory::new(
        "S",
        base_sorts(),
        vec![constant("c"), Operation::new("h", vec![], y_of("c"))],
        vec![],
    );
    let r = Theory::new(
        "R",
        base_sorts(),
        vec![constant("b"), Operation::new("g", vec![], y_of("b"))],
        vec![],
    );
    let m = morphism(
        "S",
        "R",
        XY,
        vec![
            ("c", OpAssignment::Term(Term::constant("b"))),
            ("h", OpAssignment::op("g")),
        ],
    );
    let outcome = check_morphism(&m, &s, &r);
    eprintln!("observed: {outcome:?}");
    assert!(outcome.is_ok(), "{outcome:?}");
}

/// `X`, `Y(x : X)`, constants `c`, `d`, and `Z(z : Y(param))`.
fn with_z(name: &str, param: &str) -> Theory {
    let mut sorts = base_sorts();
    sorts.push(Sort::dependent("Z", vec![SortParam::new("z", y_of(param))]));
    Theory::new(name, sorts, vec![constant("c"), constant("d")], vec![])
}

#[test]
fn check_morphism_reads_a_parameter_sort_through_derived_term_images() {
    let m = morphism(
        "S",
        "R",
        &[("X", "X"), ("Y", "Y"), ("Z", "Z")],
        vec![
            ("c", OpAssignment::Term(Term::constant("d"))),
            ("d", OpAssignment::op("d")),
        ],
    );
    // F(Y(c)) = Y(d): the parameter of Z matches R's Z(z : Y(d)) ...
    let matching = check_morphism(&m, &with_z("S", "c"), &with_z("R", "d"));
    eprintln!("observed (target Z(z : Y(d))): {matching:?}");
    assert!(matching.is_ok(), "{matching:?}");
    // ... and does not match R's Z(z : Y(c)).
    let differing = check_morphism(&m, &with_z("S", "c"), &with_z("R", "c"));
    eprintln!("observed (target Z(z : Y(c))): {differing:?}");
    assert!(
        matches!(&differing, Err(GatError::SortParamMismatch { sort, param_index: 0, .. }) if sort == "Z"),
        "{differing:?}"
    );
}

#[test]
fn check_morphism_reads_a_term_assignment_context_through_derived_term_images() {
    // k(y : Y(c)) : X goes to j(y) with j(y : Y(d)) : X; the context binds
    // y to F(Y(c)) = Y(d), so j(y) is well typed.
    let s = Theory::new(
        "S",
        base_sorts(),
        vec![
            constant("c"),
            constant("d"),
            Operation::new("k", vec![("y".into(), y_of("c"))], "X"),
        ],
        vec![],
    );
    let r = Theory::new(
        "R",
        base_sorts(),
        vec![
            constant("c"),
            constant("d"),
            Operation::new("j", vec![("y".into(), y_of("d"))], "X"),
            Operation::new("e", vec![], y_of("d")),
        ],
        vec![],
    );
    let m = morphism(
        "S",
        "R",
        XY,
        vec![
            ("c", OpAssignment::Term(Term::constant("d"))),
            ("d", OpAssignment::op("d")),
            (
                "k",
                OpAssignment::Term(Term::app("j", vec![Term::var("y")])),
            ),
        ],
    );
    let context = check_morphism(&m, &s, &r);
    eprintln!("observed (context): {context:?}");
    assert!(context.is_ok(), "{context:?}");

    // The expected output of a derived-term assignment is F of the
    // domain output: u : Y(c) goes to e() : Y(d).
    let s_out = Theory::new(
        "S",
        base_sorts(),
        vec![
            constant("c"),
            constant("d"),
            Operation::new("u", vec![], y_of("c")),
        ],
        vec![],
    );
    let m_out = morphism(
        "S",
        "R",
        XY,
        vec![
            ("c", OpAssignment::Term(Term::constant("d"))),
            ("d", OpAssignment::op("d")),
            ("u", OpAssignment::Term(Term::constant("e"))),
        ],
    );
    let output = check_morphism(&m_out, &s_out, &r);
    eprintln!("observed (output): {output:?}");
    assert!(output.is_ok(), "{output:?}");
}

/// The closed sort `B = {tt, ff}` with its two constructors.
fn booleans(name: &str) -> Theory {
    Theory::new(
        name,
        vec![Sort::closed("B", vec![], ["tt", "ff"])],
        vec![
            Operation::new("tt", vec![], "B"),
            Operation::new("ff", vec![], "B"),
        ],
        vec![],
    )
}

#[test]
fn check_morphism_reads_a_constructor_sent_to_a_term_by_its_constructor_image() {
    let check = |tt: OpAssignment| {
        let m = morphism(
            "S",
            "R",
            &[("B", "B")],
            vec![("tt", tt), ("ff", OpAssignment::op("ff"))],
        );
        check_morphism(&m, &booleans("S"), &booleans("R"))
    };
    let renamed = check(OpAssignment::op("ff"));
    let derived = check(OpAssignment::Term(Term::constant("ff")));
    let kept = check(OpAssignment::Term(Term::constant("tt")));
    eprintln!("observed tt -> ff: {renamed:?}");
    eprintln!("observed tt -> ff(): {derived:?}");
    eprintln!("observed tt -> tt(): {kept:?}");
    assert!(
        matches!(&renamed, Err(GatError::MorphismClosureMismatch { sort, .. }) if sort == "B"),
        "{renamed:?}"
    );
    assert_eq!(
        format!("{derived:?}"),
        format!("{renamed:?}"),
        "tt -> ff() sends tt to the same element as the rename tt -> ff"
    );
    assert!(kept.is_ok(), "{kept:?}");
}

/// The closed sort `N = {zero, succ(n : N)}` under the given names.
fn naturals(name: &str, zero: &str, succ: &str) -> Theory {
    Theory::new(
        name,
        vec![Sort::closed("N", vec![], [zero, succ])],
        vec![
            Operation::new(zero, vec![], "N"),
            Operation::new(succ, vec![("n".into(), "N".into())], "N"),
        ],
        vec![],
    )
}

#[test]
fn check_morphism_refuses_a_constructor_whose_term_image_is_no_constructor_application() {
    let check = |succ: Term| {
        let m = morphism(
            "S",
            "R",
            &[("N", "N")],
            vec![
                ("z", OpAssignment::op("zero")),
                ("s", OpAssignment::Term(succ)),
            ],
        );
        check_morphism(&m, &naturals("S", "z", "s"), &naturals("R", "zero", "succ"))
    };
    // succ(n) is the eta-expanded application of succ: its image is succ.
    let eta = check(Term::app("succ", vec![Term::var("n")]));
    // succ(succ(n)) is well typed but applies no constructor to n alone.
    let twice = check(Term::app(
        "succ",
        vec![Term::app("succ", vec![Term::var("n")])],
    ));
    eprintln!("observed s -> succ(n): {eta:?}");
    eprintln!("observed s -> succ(succ(n)): {twice:?}");
    assert!(eta.is_ok(), "{eta:?}");
    let Err(GatError::MorphismClosureMismatch {
        sort,
        expected,
        got,
    }) = &twice
    else {
        panic!("expected MorphismClosureMismatch, observed {twice:?}");
    };
    assert_eq!(sort, "N");
    assert!(
        expected.iter().any(|e| e == "succ(succ(n))"),
        "the constructor with no image is rendered by its term: {expected:?}"
    );
    assert_eq!(got, &["succ".to_owned(), "zero".to_owned()]);

    // Under one set of names the constructor's own name must not stand in
    // for its missing image.
    let m = morphism(
        "S",
        "R",
        &[("N", "N")],
        vec![
            ("zero", OpAssignment::op("zero")),
            (
                "succ",
                OpAssignment::Term(Term::app(
                    "succ",
                    vec![Term::app("succ", vec![Term::var("n")])],
                )),
            ),
        ],
    );
    let same_names = check_morphism(
        &m,
        &naturals("S", "zero", "succ"),
        &naturals("R", "zero", "succ"),
    );
    eprintln!("observed succ -> succ(succ(n)) under one set of names: {same_names:?}");
    assert!(
        matches!(&same_names, Err(GatError::MorphismClosureMismatch { sort, .. }) if sort == "N"),
        "{same_names:?}"
    );
}

/// `B` closed over `t`, `f`, with `neg(b : B) : B` defined by case
/// analysis.
fn negation(name: &str, t: &str, f: &str, neg: &str) -> Theory {
    let case = Term::Case {
        scrutinee: Box::new(Term::var("b")),
        branches: vec![
            CaseBranch {
                constructor: t.into(),
                binders: vec![],
                body: Term::constant(f),
            },
            CaseBranch {
                constructor: f.into(),
                binders: vec![],
                body: Term::constant(t),
            },
        ],
    };
    Theory::new(
        name,
        vec![Sort::closed("B", vec![], [t, f])],
        vec![
            Operation::new(t, vec![], "B"),
            Operation::new(f, vec![], "B"),
            Operation::new(neg, vec![("b".into(), "B".into())], "B"),
        ],
        vec![Equation::new(
            "neg_def",
            Term::app(neg, vec![Term::var("b")]),
            case,
        )],
    )
}

#[test]
fn a_case_branch_follows_the_constructor_image_of_a_derived_term() {
    let s = negation("S", "tt", "ff", "not");
    let r = negation("R", "yes", "no", "flip");
    let m = morphism(
        "S",
        "R",
        &[("B", "B")],
        vec![
            ("tt", OpAssignment::Term(Term::constant("yes"))),
            ("ff", OpAssignment::Term(Term::constant("no"))),
            ("not", OpAssignment::op("flip")),
        ],
    );
    let mapped = m.apply_to_term_substituting(&s.eqs[0].rhs, &s);
    eprintln!("observed image of the case: {mapped}");
    assert_eq!(
        mapped, r.eqs[0].rhs,
        "each branch is translated to its image"
    );
    let outcome = check_morphism(&m, &s, &r);
    eprintln!("observed: {outcome:?}");
    assert!(outcome.is_ok(), "{outcome:?}");
}

/// `X`, a constant `a : X`, and `B` closed over the one constructor
/// `ctor(first : X, second : X) : B`, with `pick(b : B) : X` defined by a
/// case analysis that binds both inputs.
fn pairs(name: &str, ctor: &str, first: &str, second: &str) -> Theory {
    let case = Term::Case {
        scrutinee: Box::new(Term::var("b")),
        branches: vec![CaseBranch {
            constructor: ctor.into(),
            binders: vec!["p".into(), "q".into()],
            body: Term::var("p"),
        }],
    };
    Theory::new(
        name,
        vec![Sort::simple("X"), Sort::closed("B", vec![], [ctor])],
        vec![
            constant("a"),
            Operation::new(
                ctor,
                vec![(first.into(), "X".into()), (second.into(), "X".into())],
                "B",
            ),
            Operation::new("pick", vec![("b".into(), "B".into())], "X"),
        ],
        vec![Equation::new(
            "pick_def",
            Term::app("pick", vec![Term::var("b")]),
            case,
        )],
    )
}

#[test]
fn a_constructor_with_a_repeated_input_name_has_no_image_through_a_term() {
    // The domain repeats the input name z, so its constructor's generic
    // term is c(#0, #1) by position and every z reads the later binder.
    // k(z, z) is then k(#1, #1), not the eta-expanded k(#0, #1).
    let dom = pairs("S", "c", "z", "z");
    let cod = pairs("R", "k", "u", "v");
    let check = |image: OpAssignment| {
        let m = morphism(
            "S",
            "R",
            &[("X", "X"), ("B", "B")],
            vec![
                ("a", OpAssignment::op("a")),
                ("c", image),
                ("pick", OpAssignment::op("pick")),
            ],
        );
        (check_morphism(&m, &dom, &cod), m)
    };
    let (derived, m) = check(OpAssignment::Term(Term::app(
        "k",
        vec![Term::var("z"), Term::var("z")],
    )));
    let (renamed, _) = check(OpAssignment::op("k"));
    eprintln!("observed c -> k(z, z): {derived:?}");
    eprintln!("observed c -> k: {renamed:?}");
    let Err(GatError::MorphismClosureMismatch { sort, expected, .. }) = &derived else {
        panic!("expected MorphismClosureMismatch, observed {derived:?}");
    };
    assert_eq!(sort, "B");
    assert!(
        expected.iter().any(|e| e == "k(z, z)"),
        "the constructor with no image is rendered by its term: {expected:?}"
    );
    assert!(
        renamed.is_ok(),
        "an Op image is always a constructor image: {renamed:?}"
    );
    // With no image the branch keeps its constructor's name.
    let mapped = m.apply_to_term_substituting(&dom.eqs[0].rhs, &dom);
    eprintln!("observed image of the case under c -> k(z, z): {mapped}");
    assert_eq!(mapped, dom.eqs[0].rhs);
}
