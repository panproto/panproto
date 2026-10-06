//! Tier 1 class agreement compares signatures syntactically.
//!
//! [`compose`] checks that the members of an operation class agree once
//! every sort and operation they mention is sent to its class. The check
//! does not use the sort conversions that term equations induce: by the
//! substitution rule a term equation `a = b` makes `Y(a)` and `Y(b)`
//! derivably equal (garner2015combinatorial Table 1, stated), but the
//! comparison sees two different argument classes. An operation class
//! whose members differ only in arguments that a tier 2 equation equates
//! is therefore refused as not representable, although its colimit needs
//! no sort equation. The refusal is conservative: it rejects a diagram
//! whose colimit exists.
//!
//! Besides the pinned instance, the family of spans below, over every
//! choice of each leg's image of the constant (renamed or a derived
//! term), both modes and both clash policies, is checked against an
//! oracle that forms the classes of the constant itself: tier 1 accepts
//! the fibre class exactly when its members mention one class of the
//! constant, and otherwise refuses two members that mention different
//! classes. A property test repeats the check under renamings of the
//! constant and the fibres.
//!
//! The diagram is a span over the apex `S`. Every node declares `X`,
//! `Y(x : X)`, a constant `a : X` and one constant of sort `Y(a)`: `h` in
//! `S`, `f` in `L`, `g` in `R`. The left leg renames `a` to `a` and `h` to
//! `f`, so `S.a` and `L.a` form one class. The right leg sends `a` to the
//! derived term `a` of `R` and renames `h` to `g`; a derived term joins no
//! class, so `R.a` is a class of its own (the `b` of the identification
//! `a = b`), and tier 2 adds the equation `S.a = R.a`. The class
//! `{S.h, L.f, R.g}` then asks `Y(class of L.a)` and `Y(class of R.a)` to
//! agree.
//!
//! `R`'s constant carries the apex's name because the leg check compares
//! the image of `Y(a)` with `g`'s declared sort without substituting the
//! derived-term image of `a` into it; with `R`'s constant named `b` and
//! `g : Y(b)` the right leg is refused as not a theory morphism before
//! any class is formed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;

use proptest::prelude::*;

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

/// The image of the apex's constant under one leg: renamed to the
/// target's constant, or sent to the derived term that is the target's
/// constant.
fn image(constant: &str, renames: bool) -> OpAssignment {
    if renames {
        OpAssignment::op(constant)
    } else {
        OpAssignment::Term(Term::constant(constant))
    }
}

/// The span over the constant `constant` with fibres `fibres` (in `S`,
/// `L`, `R`), each leg renaming the constant or sending it to a derived
/// term as `renames` says, and sending the apex's fibre to the target's.
fn span_over(constant: &str, fibres: [&str; 3], renames: [bool; 2]) -> TheoryDiagram {
    let [h, f, g] = fibres;
    TheoryDiagram::span(
        ("S", fibred("S", constant, h)),
        ("L", fibred("L", constant, f)),
        ("R", fibred("R", constant, g)),
        leg(
            "S",
            "L",
            vec![
                (constant, image(constant, renames[0])),
                (h, OpAssignment::op(f)),
            ],
        ),
        leg(
            "S",
            "R",
            vec![
                (constant, image(constant, renames[1])),
                (h, OpAssignment::op(g)),
            ],
        ),
    )
}

/// The span, with the right leg's image of `a` given.
fn span(right_a: &OpAssignment) -> TheoryDiagram {
    let renames = matches!(right_a, OpAssignment::Op(_));
    let diagram = span_over("a", ["h", "f", "g"], [true, renames]);
    assert_eq!(
        diagram.edges[1].morphism.op_map.get("a"),
        Some(right_a),
        "the right leg carries the given image"
    );
    diagram
}

fn options() -> ComposeOptions {
    ComposeOptions {
        mode: CompositionMode::ExplicitIdentity,
        result_name: Arc::from("Q"),
        clash: ClashPolicy::Freshen,
        rewrite_budget: DEFAULT_VERIFY_BUDGET,
    }
}

/// The diagram whose class agrees only modulo the tier 2 equation
/// `S.a = R.a`.
fn agreement_modulo_term_equation() -> TheoryDiagram {
    span(&OpAssignment::Term(Term::constant("a")))
}

#[test]
fn tier1_refuses_agreement_that_holds_only_modulo_term_equations() {
    let outcome = compose(&agreement_modulo_term_equation(), &options());
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
    let class = [
        SymRef::new("L", "f"),
        SymRef::new("R", "g"),
        SymRef::new("S", "h"),
    ];
    assert!(
        class.contains(&first) && class.contains(&second) && first != second,
        "the refused pair is two members of the class of h: {first}, {second}"
    );
}

#[test]
fn the_same_class_is_accepted_when_the_edges_identify_the_arguments() {
    // With `a` renamed on the right as well, `S.a`, `L.a` and `R.a` form
    // one class, so every member of the class of `h` has sort
    // `Y(class of a)` and tier 1 accepts it without any equation.
    let composition =
        compose(&span(&OpAssignment::op("a")), &options()).expect("the class agrees syntactically");
    let mut ops: Vec<&str> = composition
        .theory
        .ops
        .iter()
        .map(|o| o.name.as_ref())
        .collect();
    ops.sort_unstable();
    assert_eq!(ops, ["a", "f"], "one class for a and one for h, f, g");
    assert!(composition.theory.eqs.is_empty(), "no tier 2 equation");
}

#[test]
fn the_refusal_reads_as_a_representability_limit_of_a_syntactic_check() {
    let error = compose(&agreement_modulo_term_equation(), &options())
        .expect_err("tier 1 compares signatures syntactically");
    let rendered = error.to_string();
    eprintln!("observed message: {rendered}");
    assert!(
        rendered.contains("sort equation") && rendered.contains("term equation"),
        "the message names both the representability limit and the syntactic \
         comparison: {rendered}"
    );
    assert!(
        !rendered.contains("needs a sort equation"),
        "the message does not claim that every refused identification needs a \
         sort equation: {rendered}"
    );
}

#[test]
fn quotient_reports_the_same_limit_for_dependent_sort_arguments() {
    // `quotient` runs the same class check; identifying `f : Y(a)` with
    // `g : Y(b)` while `a` and `b` stay apart differs only in a dependent
    // sort argument.
    let theory = Theory::new(
        "T",
        vec![
            Sort::simple("X"),
            Sort::dependent("Y", vec![SortParam::new("x", "X")]),
        ],
        vec![
            Operation::new("a", vec![], "X"),
            Operation::new("b", vec![], "X"),
            Operation::new("f", vec![], SortExpr::app("Y", vec![Term::constant("a")])),
            Operation::new("g", vec![], SortExpr::app("Y", vec![Term::constant("b")])),
        ],
        vec![],
    );
    let error = panproto_gat::quotient(&theory, &[(Arc::from("f"), Arc::from("g"))])
        .expect_err("the identified operations disagree syntactically");
    let panproto_gat::GatError::QuotientIncompatible { detail, .. } = &error else {
        panic!("expected QuotientIncompatible, observed {error:?}");
    };
    eprintln!("observed detail: {detail}");
    assert!(
        detail.contains("sort equation") && detail.contains("term equation"),
        "the detail names both the representability limit and the syntactic \
         comparison: {detail}"
    );
    assert!(
        !detail.contains("would need a sort equation"),
        "the detail does not claim that a sort equation is needed: {detail}"
    );
}

/// The node whose constant each member of the fibre class mentions, in
/// the order `S.h`, `L.f`, `R.g`.
const FIBRE_NODES: [&str; 3] = ["S", "L", "R"];

/// The class of each node's constant that the class map forms, by the
/// oracle's own reckoning: the apex's constant is class 0; a leg that
/// renames the constant, or the same-name rule of `Amalgamate`, puts the
/// target's constant in it; otherwise the target's constant is a class
/// of its own.
fn constant_classes(renames: [bool; 2], mode: CompositionMode) -> [usize; 3] {
    let apart = |renamed: bool| usize::from(!renamed && mode != CompositionMode::Amalgamate);
    [0, apart(renames[0]), 2 * apart(renames[1])]
}

/// Check one instance of the family against the oracle: tier 1 accepts
/// the fibre class exactly when its members mention one class of the
/// constant, and otherwise refuses two members that mention different
/// classes as not representable.
fn check_instance(
    constant: &str,
    fibres: [&str; 3],
    renames: [bool; 2],
    opts: &ComposeOptions,
) -> Result<bool, String> {
    let classes = constant_classes(renames, opts.mode);
    let outcome = compose(&span_over(constant, fibres, renames), opts);
    let accepts = classes.iter().all(|&c| c == classes[0]);
    let context = format!("{constant} {fibres:?} {renames:?} {opts:?}: {outcome:?}");
    match outcome {
        Ok(composition) if accepts => {
            let least = fibres.iter().min().copied().unwrap_or_default();
            let mut result_ops: Vec<&str> = composition
                .theory
                .ops
                .iter()
                .map(|o| o.name.as_ref())
                .collect();
            result_ops.sort_unstable();
            let mut expected = vec![constant, least];
            expected.sort_unstable();
            if result_ops != expected {
                return Err(format!(
                    "result operations {result_ops:?}, expected {expected:?}: {context}"
                ));
            }
            let fibre_sort = SortExpr::app("Y", vec![Term::constant(constant)]);
            if composition.theory.find_op(least).map(|o| &o.output) != Some(&fibre_sort) {
                return Err(format!(
                    "the fibre class is not of sort Y({constant}): {context}"
                ));
            }
            // Each derived-term leg adds its own equation, which reads
            // `constant = constant` once every symbol is sent to its class.
            let derived = renames.iter().filter(|r| !**r).count();
            let trivial = Term::constant(constant);
            let eqs_ok = composition.theory.eqs.len() == derived
                && composition
                    .theory
                    .eqs
                    .iter()
                    .all(|e| e.lhs == trivial && e.rhs == trivial);
            if !eqs_ok {
                return Err(format!("equations {:?}: {context}", composition.theory.eqs));
            }
            Ok(true)
        }
        Err(CompositionError::QuotientNotRepresentable {
            first,
            second,
            reason: NotRepresentableReason::NeedsSortEquation,
        }) if !accepts => {
            let class_of = |s: &SymRef| -> Option<usize> {
                let at = FIBRE_NODES.iter().position(|n| *n == s.node.as_ref())?;
                (fibres[at] == s.name.as_ref()).then_some(classes[at])
            };
            match (class_of(&first), class_of(&second)) {
                (Some(a), Some(b)) if a != b => Ok(false),
                _ => Err(format!(
                    "the refused pair {first}, {second} is not two fibre members over different classes: {context}"
                )),
            }
        }
        _ => Err(format!("expected acceptance {accepts}: {context}")),
    }
}

/// Every option set: both modes under both clash policies.
fn every_option_set() -> Vec<ComposeOptions> {
    let mut out = Vec::new();
    for mode in [
        CompositionMode::ExplicitIdentity,
        CompositionMode::Amalgamate,
    ] {
        for clash in [ClashPolicy::Freshen, ClashPolicy::Reject] {
            out.push(ComposeOptions {
                mode,
                result_name: Arc::from("Q"),
                clash,
                rewrite_budget: DEFAULT_VERIFY_BUDGET,
            });
        }
    }
    out
}

#[test]
fn tier1_agrees_with_the_class_map_oracle_on_every_instance_of_the_family() {
    let mut accepted = 0;
    let mut refused = 0;
    for renames in [[true, true], [true, false], [false, true], [false, false]] {
        for opts in every_option_set() {
            if check_instance("a", ["h", "f", "g"], renames, &opts).unwrap() {
                accepted += 1;
            } else {
                refused += 1;
            }
        }
    }
    eprintln!("family: 16 instances; accepted {accepted}, refused {refused}");
    // Explicit identity accepts only when both legs rename (2 of 8);
    // amalgamation joins the constants by name and accepts all 8.
    assert_eq!((accepted, refused), (10, 6));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn tier1_agrees_with_the_class_map_oracle_under_renaming(
        constant in proptest::sample::select(vec!["a", "b", "k", "zz"]),
        fibres in proptest::sample::subsequence(vec!["f", "g", "h", "p", "q"], 3).prop_shuffle(),
        renames in any::<[bool; 2]>(),
        opts in proptest::sample::select(every_option_set()),
    ) {
        let fibres: [&str; 3] = [fibres[0], fibres[1], fibres[2]];
        let verdict = check_instance(constant, fibres, renames, &opts);
        prop_assert!(verdict.is_ok(), "{}", verdict.unwrap_err());
    }
}
