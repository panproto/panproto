#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Worked examples of signature checking and theory colimits.

use std::collections::BTreeMap;

use panproto_reference::theory::{
    self, ClashPolicy, ComposeError, Defect, Mode, Morphism, Op, Site, Sort, SortExpr, Term, Theory,
};

fn sort(n: &str) -> Sort {
    Sort {
        name: n.into(),
        params: vec![],
        kind: "structural".into(),
        closed: false,
    }
}

fn th(name: &str, sorts: &[&str]) -> Theory {
    Theory {
        name: name.into(),
        sorts: sorts.iter().map(|s| sort(s)).collect(),
        ..Theory::default()
    }
}

fn id_on(names: &[(&str, &str)]) -> Morphism {
    Morphism {
        sorts: names
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect(),
        ops: BTreeMap::default(),
    }
}

#[test]
fn a_dependent_sort_application_is_typed_against_its_parameters() {
    let mut t = th("C", &["Ob"]);
    t.sorts.push(Sort {
        name: "Hom".into(),
        params: vec![
            ("a".into(), SortExpr::Name("Ob".into())),
            ("b".into(), SortExpr::Name("Ob".into())),
        ],
        kind: "s".into(),
        closed: false,
    });
    t.ops.push(Op {
        name: "id".into(),
        inputs: vec![("x".into(), SortExpr::Name("Ob".into()))],
        output: SortExpr::App(
            "Hom".into(),
            vec![Term::Var("x".into()), Term::Var("x".into())],
        ),
    });
    assert_eq!(theory::typecheck_signature(&t), Ok(()));
    t.ops.push(Op {
        name: "bad".into(),
        inputs: vec![("x".into(), SortExpr::Name("Ob".into()))],
        output: SortExpr::App("Hom".into(), vec![Term::Var("x".into())]),
    });
    assert_eq!(
        theory::typecheck_signature(&t),
        Err(vec![Defect::SortArity(
            Site::OpOutput("bad".into()),
            "Hom".into(),
            2,
            1
        )])
    );
}

#[test]
fn majority_naming_is_order_free_and_explicit_identity_freshens_clashes() {
    let nodes = vec![
        ("a".to_owned(), th("A", &["Node"])),
        ("b".to_owned(), th("B", &["Vertex"])),
        ("c".to_owned(), th("C", &["Vertex"])),
    ];
    let edges = vec![
        ("a".to_owned(), "b".to_owned(), id_on(&[("Node", "Vertex")])),
        ("a".to_owned(), "c".to_owned(), id_on(&[("Node", "Vertex")])),
    ];
    let r = theory::compose(
        &nodes,
        &edges,
        Mode::ExplicitIdentity,
        ClashPolicy::Reject,
        "R",
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        r.theory
            .sorts
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>(),
        vec!["Vertex".to_owned()]
    );
    let apart = vec![
        ("l".to_owned(), th("L", &["X"])),
        ("r".to_owned(), th("R", &["X"])),
    ];
    assert!(matches!(
        theory::compose(
            &apart,
            &[],
            Mode::ExplicitIdentity,
            ClashPolicy::Reject,
            "R"
        ),
        Err(ComposeError::NameClash(..))
    ));
    let fresh = theory::compose(
        &apart,
        &[],
        Mode::ExplicitIdentity,
        ClashPolicy::Freshen,
        "R",
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        fresh
            .theory
            .sorts
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>(),
        vec!["l.X".to_owned(), "r.X".to_owned()]
    );
    let merged = theory::compose(&apart, &[], Mode::Amalgamate, ClashPolicy::Reject, "R")
        .unwrap()
        .unwrap();
    assert_eq!(merged.theory.sorts.len(), 1);
}

#[test]
fn a_leg_sending_a_sort_to_one_of_another_arity_is_refused() {
    let mut a = th("A", &["P"]);
    a.sorts.push(Sort {
        name: "D".into(),
        params: vec![("x".into(), SortExpr::Name("P".into()))],
        kind: "s".into(),
        closed: false,
    });
    let b = th("B", &["P", "E"]);
    let shared = th("S", &["P", "Q"]);
    let nodes = vec![
        ("s".to_owned(), shared),
        ("a".to_owned(), a),
        ("b".to_owned(), b),
    ];
    let edges = vec![
        (
            "s".to_owned(),
            "a".to_owned(),
            id_on(&[("P", "P"), ("Q", "D")]),
        ),
        (
            "s".to_owned(),
            "b".to_owned(),
            id_on(&[("P", "P"), ("Q", "E")]),
        ),
    ];
    // Q has no parameter in S but D has one in A: the first leg is no
    // morphism, so the composition is refused at that leg.
    assert!(matches!(
        theory::compose(
            &nodes,
            &edges,
            Mode::ExplicitIdentity,
            ClashPolicy::Reject,
            "R"
        ),
        Err(ComposeError::InvalidLeg(0))
    ));
}
