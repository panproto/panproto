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
fn nested_sort_arguments_are_recursively_typechecked() {
    let mut t = th("T", &["A", "B", "C", "D"]);
    t.sorts.push(Sort {
        name: "Indexed".into(),
        params: vec![("d".into(), SortExpr::Name("D".into()))],
        kind: "structural".into(),
        constructors: None,
    });
    t.ops.extend([
        op("f", &[("x", "A")], "B"),
        op("g", &[("x", "C")], "D"),
        Op {
            name: "bad".into(),
            inputs: vec![("x".into(), SortExpr::Name("A".into()))],
            output: SortExpr::App(
                "Indexed".into(),
                vec![Term::App(
                    "g".into(),
                    vec![Term::App("f".into(), vec![Term::Var("x".into())])],
                )],
            ),
        },
    ]);
    assert!(theory::typecheck_signature(&t).is_err());
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
    let r = theory::compose_legacy(
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
    assert_eq!(r.inclusions.len(), 3);
    assert_eq!(r.report.status, theory::CheckStatus::Passed);
    let apart = vec![
        ("l".to_owned(), th("L", &["X"])),
        ("r".to_owned(), th("R", &["X"])),
    ];
    assert!(matches!(
        theory::compose_legacy(
            &apart,
            &[],
            Mode::ExplicitIdentity,
            ClashPolicy::Reject,
            "R"
        ),
        Err(ComposeError::NameClash(..))
    ));
    let fresh = theory::compose_legacy(
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
    assert_eq!(fresh.report.freshened.len(), 2);
    assert!(!fresh.report.name_clash_free);
    assert_eq!(fresh.report.status, theory::CheckStatus::Failed);
    let merged =
        theory::compose_legacy(&apart, &[], Mode::Amalgamate, ClashPolicy::Reject, "R").unwrap();
    assert_eq!(merged.theory.sorts.len(), 1);
}

#[test]
fn a_composition_records_total_provenance_and_verifies_its_universal_arrow() {
    let node = th("A", &["S"]);
    let nodes = vec![("a".to_owned(), node.clone())];
    let composition = theory::compose_legacy(
        &nodes,
        &[],
        Mode::ExplicitIdentity,
        ClashPolicy::Reject,
        "R",
    )
    .unwrap();
    assert_eq!(
        composition.provenance.sorts,
        BTreeMap::from([(sym("a", "S"), "S".to_owned())])
    );
    assert!(composition.provenance.ops.is_empty());
    assert!(composition.provenance.eqs.is_empty());
    assert!(composition.provenance.directed_eqs.is_empty());
    assert!(composition.provenance.policies.is_empty());
    assert!(composition.provenance.added_eqs.is_empty());

    let leg = id_on(&[("S", "S")]);
    let diagram = theory::TheoryDiagram::discrete(nodes);
    let (mediator, report) = composition
        .verify_universal(&diagram, &node, &[leg])
        .unwrap();
    assert_eq!(mediator.sorts, BTreeMap::from([("S".into(), "S".into())]));
    assert!(report.checks.iter().any(|record| {
        record.check == theory::CompositionCheckKind::UniversalAgainst
            && record.outcome == theory::CheckOutcome::Passed
    }));
    assert!(report.checks.iter().any(|record| {
        record.check == theory::CompositionCheckKind::EquationEntailed
            && matches!(
                record.outcome,
                theory::CheckOutcome::Inapplicable {
                    reason: theory::InapplicableReason::NoEquations
                }
            )
    }));
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
        theory::compose_legacy(
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
) -> Result<theory::Composition, Box<ComposeError>> {
    let nodes: Vec<(String, Theory)> = nodes
        .iter()
        .map(|(l, t)| ((*l).to_owned(), t.clone()))
        .collect();
    let edges: Vec<(String, String, Morphism)> = edges
        .iter()
        .map(|(s, t, m)| ((*s).to_owned(), (*t).to_owned(), m.clone()))
        .collect();
    theory::compose_legacy(&nodes, &edges, mode, ClashPolicy::Reject, "R").map_err(Box::new)
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
        r.theory
            .sorts
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>(),
        vec!["Vertex".to_owned()]
    );
    assert_eq!(r.theory.ops.len(), 1);
    assert_eq!(
        r.provenance
            .get(&(theory::SymKind::Sort, sym("a", "Vertex"))),
        Some(&"Vertex".to_owned())
    );
    assert_eq!(r.report.status, theory::CheckStatus::Passed);
    // No node declares X: X stays a free name of the result.
    let mut c = th("C", &[]);
    c.ops.push(op("g", &[], "X"));
    let free = compose(&[("c", c)], &[], Mode::ExplicitIdentity).unwrap();
    assert!(free.theory.sorts.is_empty());
    assert_eq!(free.theory.ops[0].output, SortExpr::Name("X".into()));
    assert_eq!(free.report.status, theory::CheckStatus::Incomplete);
    assert!(free.report.checks.iter().any(|record| {
        matches!(
            &record.outcome,
            theory::CheckOutcome::Inapplicable {
                reason: theory::InapplicableReason::OpenTheory { missing_sorts }
            } if missing_sorts == &["X".to_owned()]
        )
    }));
    // Two unidentified declarations of Vertex make the name ambiguous
    // under explicit identity, and are one sort under amalgamation.
    let nodes = [
        ("a", a),
        ("b", th("B", &["Vertex"])),
        ("d", th("D", &["Vertex"])),
    ];
    assert_eq!(
        compose(&nodes, &[], Mode::ExplicitIdentity).map(|_| ()),
        Err(Box::new(ComposeError::NameClash(
            theory::SymKind::Sort,
            "Vertex".into(),
            Box::new(vec![vec![sym("b", "Vertex")], vec![sym("d", "Vertex")]])
        )))
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
    let r = compose(
        &[("a", a.clone()), ("b", b.clone())],
        &legs,
        Mode::ExplicitIdentity,
    )
    .unwrap();
    assert_eq!(
        r.theory
            .directed_eqs
            .iter()
            .map(|d| d.name.clone())
            .collect::<Vec<_>>(),
        vec!["da".to_owned()]
    );
    assert_eq!(
        r.theory
            .policies
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>(),
        vec!["pa".to_owned()]
    );
    assert_eq!(
        r.provenance
            .get(&(theory::SymKind::DirectedEq, sym("b", "db"))),
        Some(&"da".to_owned())
    );
    // Amalgamation: one name with two contents is a name conflict.
    let mut a2 = a.clone();
    a2.directed_eqs = vec![directed("d", "one")];
    let mut b2 = b.clone();
    b2.directed_eqs = vec![directed("d", "two")];
    assert!(matches!(
        compose(&[("a", a2), ("b", b2)], &legs, Mode::Amalgamate),
        Err(error)
            if matches!(*error, ComposeError::NameConflict(theory::SymKind::DirectedEq, ..))
    ));
    let mut a3 = a;
    a3.policies = vec![policy("p", "keep_left")];
    let mut b3 = b;
    b3.policies = vec![policy("p", "keep_right")];
    assert!(matches!(
        compose(&[("a", a3), ("b", b3)], &legs, Mode::Amalgamate),
        Err(error) if matches!(*error, ComposeError::NameConflict(theory::SymKind::Policy, ..))
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
    let case = |binder: &str, t: &str, f: &str| {
        Term::Case(
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
        )
    };
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
    let r = compose(
        &[("a", a.clone()), ("b", b)],
        &[("a", "b", leg.clone())],
        Mode::ExplicitIdentity,
    )
    .unwrap();
    assert_eq!(r.theory.eqs.len(), 1, "{:?}", r.theory.eqs);
    // A leg that sends the constructors out of order is no morphism.
    let mut swapped = leg;
    swapped
        .ops
        .insert("t".into(), theory::OpImage::Op("no".into()));
    swapped
        .ops
        .insert("f".into(), theory::OpImage::Op("yes".into()));
    let b2 = closed("B", &["yes", "no"]);
    assert_eq!(
        compose(
            &[("a", a), ("b", b2)],
            &[("a", "b", swapped)],
            Mode::ExplicitIdentity
        )
        .map(|_| ()),
        Err(Box::new(ComposeError::InvalidLeg(0)))
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
    assert!(
        compose(
            &[("s", s.clone()), ("t", t.clone())],
            &[("s", "t", good)],
            Mode::ExplicitIdentity
        )
        .is_ok()
    );
    let mut bad = id_on(&[("A", "A"), ("C", "C")]);
    bad.ops.insert(
        "k".into(),
        theory::OpImage::Term(Term::App("g".into(), vec![Term::Var("x".into())])),
    );
    assert_eq!(
        compose(
            &[("s", s), ("t", t)],
            &[("s", "t", bad)],
            Mode::ExplicitIdentity
        )
        .map(|_| ()),
        Err(Box::new(ComposeError::InvalidLeg(0)))
    );
}

#[test]
fn morphisms_preserve_kinds_closure_axioms_and_full_term_typing() {
    morphism_kinds_and_closure();
    morphism_axiom_preservation();
    morphism_full_term_typing();
    morphism_equation_rewriting();
}

fn morphism_kinds_and_closure() {
    let mut open = th("Open", &["S"]);
    open.sorts[0].kind = "source-kind".into();
    let mut other_kind = th("Other", &["T"]);
    other_kind.sorts[0].kind = "target-kind".into();
    assert!(!theory::check_morphism(
        &open,
        &other_kind,
        &id_on(&[("S", "T")])
    ));

    // A16 constrains only a closed domain sort. An open source sort therefore
    // imposes no closure condition on its image.
    let mut closed_target = th("ClosedTarget", &["T"]);
    closed_target.sorts[0].constructors = Some(vec!["k".into()]);
    closed_target.ops.push(op("k", &[], "T"));
    assert!(theory::check_morphism(
        &th("Open", &["S"]),
        &closed_target,
        &id_on(&[("S", "T")])
    ));

    // Closed constructor images are a set, and an eta-expanded derived term
    // is a constructor image.
    let mut source = th("Source", &["S"]);
    source.sorts[0].constructors = Some(vec!["c1".into(), "c2".into()]);
    source.ops.extend([op("c1", &[], "S"), op("c2", &[], "S")]);
    let mut target = th("Target", &["T"]);
    target.sorts[0].constructors = Some(vec!["k2".into(), "k1".into()]);
    target.ops.extend([op("k1", &[], "T"), op("k2", &[], "T")]);
    let mut constructors = id_on(&[("S", "T")]);
    constructors.ops.insert(
        "c1".into(),
        theory::OpImage::Term(Term::App("k1".into(), vec![])),
    );
    constructors
        .ops
        .insert("c2".into(), theory::OpImage::Op("k2".into()));
    assert!(theory::check_morphism(&source, &target, &constructors));
}

fn morphism_axiom_preservation() {
    let mut axiom_source = th("AxiomSource", &["S"]);
    axiom_source.ops.push(op("f", &[("x", "S")], "S"));
    axiom_source.eqs.push(theory::Equation {
        name: "unit".into(),
        lhs: Term::App("f".into(), vec![Term::Var("x".into())]),
        rhs: Term::Var("x".into()),
    });
    axiom_source.directed_eqs.push(directed("reduce", "impl"));
    let mut axiom_target = th("AxiomTarget", &["S"]);
    axiom_target.ops.push(op("f", &[("x", "S")], "S"));
    let mut identity = id_on(&[("S", "S")]);
    identity
        .ops
        .insert("f".into(), theory::OpImage::Op("f".into()));
    assert!(!theory::check_morphism(
        &axiom_source,
        &axiom_target,
        &identity
    ));
    axiom_target.eqs.clone_from(&axiom_source.eqs);
    assert!(!theory::check_morphism(
        &axiom_source,
        &axiom_target,
        &identity
    ));
    axiom_target
        .directed_eqs
        .clone_from(&axiom_source.directed_eqs);
    assert!(theory::check_morphism(
        &axiom_source,
        &axiom_target,
        &identity
    ));
}

fn morphism_full_term_typing() {
    let mut nested_source = th("NestedSource", &["A", "D"]);
    nested_source.ops.push(op("out", &[("x", "A")], "D"));
    let mut nested_target = th("NestedTarget", &["A", "B", "C", "D"]);
    nested_target
        .ops
        .extend([op("f", &[("x", "A")], "B"), op("g", &[("x", "C")], "D")]);
    let mut nested = id_on(&[("A", "A"), ("D", "D")]);
    nested.ops.insert(
        "out".into(),
        theory::OpImage::Term(Term::App(
            "g".into(),
            vec![Term::App("f".into(), vec![Term::Var("x".into())])],
        )),
    );
    assert!(!theory::check_morphism(
        &nested_source,
        &nested_target,
        &nested
    ));
}

fn morphism_equation_rewriting() {
    // Ordinary equations may be preserved by the target's directed theory,
    // not only by a same-shaped declaration.
    let mut equational_source = th("EquationalSource", &["A"]);
    equational_source.ops.push(op("f", &[("x", "A")], "A"));
    equational_source.eqs.push(theory::Equation {
        name: "involution".into(),
        lhs: Term::App(
            "f".into(),
            vec![Term::App("f".into(), vec![Term::Var("x".into())])],
        ),
        rhs: Term::Var("x".into()),
    });
    let mut equational_target = th("EquationalTarget", &["A"]);
    equational_target.ops.push(op("f", &[("x", "A")], "A"));
    equational_target
        .directed_eqs
        .push(theory::DirectedEquation {
            name: "idempotent".into(),
            lhs: Term::App("f".into(), vec![Term::Var("x".into())]),
            rhs: Term::Var("x".into()),
            implementation: String::new(),
            inverse: None,
            source_kind: None,
            target_kind: None,
            coercion_class: String::new(),
        });
    let mut rewrite_morphism = id_on(&[("A", "A")]);
    rewrite_morphism
        .ops
        .insert("f".into(), theory::OpImage::Op("f".into()));
    assert!(theory::check_morphism(
        &equational_source,
        &equational_target,
        &rewrite_morphism
    ));
}

#[test]
fn bidirectional_equation_search_rejects_budget_exhaustion() {
    // Undirected target equations are installed in declaration order in both
    // directions. This chain cycles under that concrete rule order, so the
    // 512-step search is inconclusive and must reject rather than accept the
    // common form produced by a one-way orientation.
    let mut cycling_source = th("CyclingSource", &["A"]);
    cycling_source
        .ops
        .extend([op("a", &[], "A"), op("b", &[], "A"), op("c", &[], "A")]);
    cycling_source.eqs.push(theory::Equation {
        name: "a_is_c".into(),
        lhs: Term::App("a".into(), vec![]),
        rhs: Term::App("c".into(), vec![]),
    });
    let mut cycling_target = th("CyclingTarget", &["A"]);
    cycling_target.ops = cycling_source.ops.clone();
    cycling_target.eqs.extend([
        theory::Equation {
            name: "a_is_b".into(),
            lhs: Term::App("a".into(), vec![]),
            rhs: Term::App("b".into(), vec![]),
        },
        theory::Equation {
            name: "b_is_c".into(),
            lhs: Term::App("b".into(), vec![]),
            rhs: Term::App("c".into(), vec![]),
        },
    ]);
    let mut cycling = id_on(&[("A", "A")]);
    for operation in ["a", "b", "c"] {
        cycling
            .ops
            .insert(operation.into(), theory::OpImage::Op(operation.into()));
    }
    assert!(!theory::check_morphism(
        &cycling_source,
        &cycling_target,
        &cycling
    ));
}

#[test]
fn a_discrete_diagram_refuses_an_ill_typed_transported_equation() {
    let mut node = th("Bad", &["A", "B"]);
    node.ops.extend([op("a", &[], "A"), op("b", &[], "B")]);
    node.eqs.push(theory::Equation {
        name: "cross_sort".into(),
        lhs: Term::App("a".into(), vec![]),
        rhs: Term::App("b".into(), vec![]),
    });

    assert!(matches!(
        compose(&[("bad", node)], &[], Mode::ExplicitIdentity),
        Err(error) if matches!(*error, ComposeError::IllTypedTransport { .. })
    ));
}

#[test]
fn explicit_diagrams_replay_and_budget_exhaustion_is_incomplete() {
    let mut node = th("A", &["S"]);
    node.ops
        .extend([op("a", &[], "S"), op("b", &[], "S"), op("c", &[], "S")]);
    node.eqs.push(theory::Equation {
        name: "a_is_c".into(),
        lhs: Term::App("a".into(), vec![]),
        rhs: Term::App("c".into(), vec![]),
    });
    let diagram = theory::TheoryDiagram::discrete(vec![("a".into(), node.clone())]);
    let options = theory::ComposeOptions {
        mode: theory::CompositionMode::ExplicitIdentity,
        result_name: "R".into(),
        clash: ClashPolicy::Reject,
        rewrite_budget: 1,
    };
    let composition = theory::compose(&diagram, &options).unwrap();

    let recipe = theory::CompositionSpec {
        result_name: "R".into(),
        steps: vec![theory::CompositionStep::Diagram {
            nodes: vec!["A".into()],
            edges: vec![],
            mode: theory::CompositionMode::ExplicitIdentity,
            clash: ClashPolicy::Reject,
            rewrite_budget: 1,
            inline: vec![node.clone()],
        }],
        version: 1,
    };
    assert_eq!(
        theory::recompose(&recipe, &BTreeMap::new()).unwrap(),
        composition.theory
    );
    let mut reordered = composition.theory.clone();
    reordered.ops.reverse();
    assert_eq!(
        theory::canonical_theory_bytes(&composition.theory),
        theory::canonical_theory_bytes(&reordered)
    );

    let mut target = node.clone();
    target.eqs = vec![
        theory::Equation {
            name: "a_is_b".into(),
            lhs: Term::App("a".into(), vec![]),
            rhs: Term::App("b".into(), vec![]),
        },
        theory::Equation {
            name: "b_is_c".into(),
            lhs: Term::App("b".into(), vec![]),
            rhs: Term::App("c".into(), vec![]),
        },
    ];
    let mut leg = id_on(&[("S", "S")]);
    for operation in ["a", "b", "c"] {
        leg.ops
            .insert(operation.into(), theory::OpImage::Op(operation.into()));
    }
    let (_, report) = composition
        .verify_universal_within(&diagram, &target, &[leg], 1)
        .unwrap();
    assert_eq!(report.status, theory::CheckStatus::Incomplete);
    assert!(report.checks.iter().any(|record| {
        matches!(
            record.outcome,
            theory::CheckOutcome::Incomplete {
                reason: theory::CompositionIncomplete::RewriteBudgetExhausted { .. }
            }
        )
    }));
}

#[test]
fn universal_cocones_compare_operation_images_after_eta_expansion() {
    let mut left = th("Left", &["S"]);
    left.ops.push(op("f", &[("x", "S")], "S"));
    let right = left.clone();
    let mut identity = id_on(&[("S", "S")]);
    identity
        .ops
        .insert("f".into(), theory::OpImage::Op("f".into()));
    let diagram = theory::TheoryDiagram {
        nodes: vec![
            theory::DiagramNode {
                label: "left".into(),
                theory: left,
            },
            theory::DiagramNode {
                label: "right".into(),
                theory: right,
            },
        ],
        edges: vec![theory::DiagramEdge {
            src: "left".into(),
            tgt: "right".into(),
            morphism: identity,
        }],
    };
    let composition = theory::compose(
        &diagram,
        &theory::ComposeOptions {
            mode: Mode::ExplicitIdentity,
            result_name: "R".into(),
            clash: ClashPolicy::Reject,
            rewrite_budget: 512,
        },
    )
    .unwrap();
    let mut target = th("Target", &["S"]);
    target.ops.push(op("g", &[("x", "S")], "S"));
    let mut direct = id_on(&[("S", "S")]);
    direct
        .ops
        .insert("f".into(), theory::OpImage::Op("g".into()));
    let mut expanded = id_on(&[("S", "S")]);
    expanded.ops.insert(
        "f".into(),
        theory::OpImage::Term(Term::App("g".into(), vec![Term::Var("x".into())])),
    );
    assert!(
        composition
            .verify_universal(&diagram, &target, &[direct, expanded])
            .is_ok()
    );
}
