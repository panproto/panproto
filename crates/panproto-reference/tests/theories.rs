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
        constructors: None,
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
        constructors: None,
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
        constructors: None,
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

fn op(name: &str, inputs: &[(&str, &str)], output: &str) -> Op {
    Op {
        name: name.into(),
        inputs: inputs
            .iter()
            .map(|(x, s)| ((*x).to_owned(), SortExpr::Name((*s).to_owned())))
            .collect(),
        output: SortExpr::Name(output.into()),
    }
}

fn compose(
    nodes: &[(&str, Theory)],
    edges: &[(&str, &str, Morphism)],
    mode: Mode,
) -> Result<theory::Composition, ComposeError> {
    let nodes: Vec<(String, Theory)> = nodes
        .iter()
        .map(|(l, t)| ((*l).to_owned(), t.clone()))
        .collect();
    let edges: Vec<(String, String, Morphism)> = edges
        .iter()
        .map(|(s, t, m)| ((*s).to_owned(), (*t).to_owned(), m.clone()))
        .collect();
    theory::compose(&nodes, &edges, mode, ClashPolicy::Reject, "R")
}

fn sym(node: &str, name: &str) -> theory::SymRef {
    theory::SymRef {
        node: node.into(),
        name: name.into(),
    }
}

#[test]
fn an_undeclared_sort_is_an_implicit_symbol_joined_by_name_or_left_free() {
    // Node a uses Vertex without declaring it; node b declares it.
    let mut a = th("A", &[]);
    a.ops.push(op("f", &[("x", "Vertex")], "Vertex"));
    let b = th("B", &["Vertex"]);
    let r = compose(&[("a", a.clone()), ("b", b)], &[], Mode::ExplicitIdentity).unwrap();
    assert_eq!(
        r.theory.sorts.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        vec!["Vertex".to_owned()]
    );
    assert_eq!(r.theory.ops.len(), 1);
    assert_eq!(
        r.provenance.get(&(theory::SymKind::Sort, sym("a", "Vertex"))),
        Some(&"Vertex".to_owned())
    );
    assert!(r.free_sorts.is_empty());
    // No node declares X: X stays a free name of the result.
    let mut c = th("C", &[]);
    c.ops.push(op("g", &[], "X"));
    let free = compose(&[("c", c)], &[], Mode::ExplicitIdentity).unwrap();
    assert!(free.theory.sorts.is_empty());
    assert_eq!(free.free_sorts, ["X".to_owned()].into());
    assert_eq!(free.theory.ops[0].output, SortExpr::Name("X".into()));
    // Two unidentified declarations of Vertex make the name ambiguous
    // under explicit identity, and are one sort under amalgamation.
    let nodes = [("a", a), ("b", th("B", &["Vertex"])), ("d", th("D", &["Vertex"]))];
    assert_eq!(
        compose(&nodes, &[], Mode::ExplicitIdentity).map(|_| ()),
        Err(ComposeError::NameClash(
            theory::SymKind::Sort,
            "Vertex".into(),
            vec![vec![sym("b", "Vertex")], vec![sym("d", "Vertex")]]
        ))
    );
    let merged = compose(&nodes, &[], Mode::Amalgamate).unwrap();
    assert_eq!(merged.theory.sorts.len(), 1);
}

fn directed(name: &str, implementation: &str) -> theory::DirectedEquation {
    theory::DirectedEquation {
        name: name.into(),
        lhs: Term::App("f".into(), vec![Term::Var("x".into())]),
        rhs: Term::Var("x".into()),
        implementation: implementation.into(),
        inverse: None,
        source_kind: None,
        target_kind: None,
        coercion_class: "iso".into(),
    }
}

fn policy(name: &str, strategy: &str) -> theory::Policy {
    theory::Policy {
        name: name.into(),
        value_kind: "str".into(),
        strategy: strategy.into(),
    }
}

#[test]
fn directed_equations_and_policies_are_sets_by_content_and_conflict_by_name() {
    let mut a = th("A", &["S"]);
    a.ops.push(op("f", &[("x", "S")], "S"));
    a.directed_eqs.push(directed("da", "impl"));
    a.policies.push(policy("pa", "keep_left"));
    let mut b = th("B", &["S"]);
    b.ops.push(op("f", &[("y", "S")], "S"));
    let mut db = directed("db", "impl");
    db.lhs = Term::App("f".into(), vec![Term::Var("z".into())]);
    db.rhs = Term::Var("z".into());
    b.directed_eqs.push(db);
    b.policies.push(policy("pb", "keep_left"));
    let legs = [("a", "b", {
        let mut m = id_on(&[("S", "S")]);
        m.ops.insert("f".into(), theory::OpImage::Op("f".into()));
        m
    })];
    // Explicit identity: alpha-equivalent directed equations with equal
    // opaque parts are one; equal policies are one; each named by the least
    // member name on a tie.
    let r = compose(&[("a", a.clone()), ("b", b.clone())], &legs, Mode::ExplicitIdentity).unwrap();
    assert_eq!(
        r.theory.directed_eqs.iter().map(|d| d.name.clone()).collect::<Vec<_>>(),
        vec!["da".to_owned()]
    );
    assert_eq!(
        r.theory.policies.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        vec!["pa".to_owned()]
    );
    assert_eq!(
        r.provenance.get(&(theory::SymKind::DirectedEq, sym("b", "db"))),
        Some(&"da".to_owned())
    );
    // Amalgamation: one name with two contents is a name conflict.
    let mut a2 = a.clone();
    a2.directed_eqs = vec![directed("d", "one")];
    let mut b2 = b.clone();
    b2.directed_eqs = vec![directed("d", "two")];
    assert!(matches!(
        compose(&[("a", a2), ("b", b2)], &legs, Mode::Amalgamate),
        Err(ComposeError::NameConflict(theory::SymKind::DirectedEq, ..))
    ));
    let mut a3 = a;
    a3.policies = vec![policy("p", "keep_left")];
    let mut b3 = b;
    b3.policies = vec![policy("p", "keep_right")];
    assert!(matches!(
        compose(&[("a", a3), ("b", b3)], &legs, Mode::Amalgamate),
        Err(ComposeError::NameConflict(theory::SymKind::Policy, ..))
    ));
}

#[test]
fn case_terms_translate_their_constructors_and_compare_up_to_binder_names() {
    // A closed sort B with constructors t, f; the equation not(not(x)) = x
    // written by cases, with different binder names in the two nodes.
    let closed = |n: &str, cs: &[&str]| {
        let mut t = th(n, &[]);
        t.sorts.push(Sort {
            name: "B".into(),
            params: vec![],
            kind: "structural".into(),
            constructors: Some(cs.iter().map(|c| (*c).to_owned()).collect()),
        });
        for c in cs {
            t.ops.push(op(c, &[], "B"));
        }
        t.ops.push(op("not", &[("x", "B")], "B"));
        t
    };
    let case = |binder: &str, t: &str, f: &str| Term::Case(
        Box::new(Term::Var(binder.into())),
        vec![
            theory::CaseBranch {
                constructor: t.into(),
                binders: vec![],
                body: Term::App(f.into(), vec![]),
            },
            theory::CaseBranch {
                constructor: f.into(),
                binders: vec![],
                body: Term::App(t.into(), vec![]),
            },
        ],
    );
    let mut a = closed("A", &["t", "f"]);
    a.eqs.push(theory::Equation {
        name: "neg".into(),
        lhs: Term::App("not".into(), vec![Term::Var("x".into())]),
        rhs: case("x", "t", "f"),
    });
    let mut b = closed("B", &["yes", "no"]);
    b.eqs.push(theory::Equation {
        name: "negation".into(),
        lhs: Term::App("not".into(), vec![Term::Var("y".into())]),
        rhs: case("y", "yes", "no"),
    });
    let mut leg = id_on(&[("B", "B")]);
    for (x, y) in [("t", "yes"), ("f", "no"), ("not", "not")] {
        leg.ops.insert(x.into(), theory::OpImage::Op(y.into()));
    }
    let r = compose(&[("a", a.clone()), ("b", b)], &[("a", "b", leg.clone())], Mode::ExplicitIdentity)
        .unwrap();
    assert_eq!(r.theory.eqs.len(), 1, "{:?}", r.theory.eqs);
    // A leg that sends the constructors out of order is no morphism.
    let mut swapped = leg;
    swapped.ops.insert("t".into(), theory::OpImage::Op("no".into()));
    swapped.ops.insert("f".into(), theory::OpImage::Op("yes".into()));
    let b2 = closed("B", &["yes", "no"]);
    assert_eq!(
        compose(&[("a", a), ("b", b2)], &[("a", "b", swapped)], Mode::ExplicitIdentity).map(|_| ()),
        Err(ComposeError::InvalidLeg(0))
    );
}

#[test]
fn a_term_image_of_the_wrong_sort_is_no_morphism() {
    let mut s = th("S", &["A", "C"]);
    s.ops.push(op("k", &[("x", "A")], "C"));
    let mut t = th("T", &["A", "C"]);
    t.ops.push(op("g", &[("x", "A")], "A"));
    t.ops.push(op("h", &[("x", "A")], "C"));
    let mut good = id_on(&[("A", "A"), ("C", "C")]);
    good.ops.insert(
        "k".into(),
        theory::OpImage::Term(Term::App(
            "h".into(),
            vec![Term::App("g".into(), vec![Term::Var("x".into())])],
        )),
    );
    assert!(compose(&[("s", s.clone()), ("t", t.clone())], &[("s", "t", good)], Mode::ExplicitIdentity).is_ok());
    let mut bad = id_on(&[("A", "A"), ("C", "C")]);
    bad.ops.insert(
        "k".into(),
        theory::OpImage::Term(Term::App("g".into(), vec![Term::Var("x".into())])),
    );
    assert_eq!(
        compose(&[("s", s), ("t", t)], &[("s", "t", bad)], Mode::ExplicitIdentity).map(|_| ()),
        Err(ComposeError::InvalidLeg(0))
    );
}
