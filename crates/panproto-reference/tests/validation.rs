#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The probe fixture of validation: a ghost anchor, an undeclared label, an
//! incoherent endpoint, a missing required edge and a length bound.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::keys::{KeyEquality, SqlCollation, SqlPad};
use panproto_reference::lens::ViewRef;
use panproto_reference::model::{Fan, GEdge, Graph, Node, SEdge, Table, Tree};
use panproto_reference::schema::{
    Cardinality, DeclarationError, HyperEdge, Key, KeyPart, KeyRole, KeyScope, OnUnresolved,
    PathSyntax, RefGuard, RefSource, Reference, ResolveMode, Schema, TargetSet,
};
use panproto_reference::validation::{
    self, BaselineBudgetEvidence, CheckClass, CheckInstance, CheckKey, CheckKind, CheckRun,
    CheckStatus, Diagnostic, IncompleteReason, InstanceShape, NativeRef, PriorAdmissionPolicy,
    Requirement, RunBundleError, SchemaRef, Semantics, ShapeRefusal, TrustedSchedule,
    ValidationBudget, ValidationContext, ValidationInput, ValidationOutcome, ValidationTrust,
    Violation, WitnessError,
};
use panproto_reference::value::{Presence, Value};

#[test]
fn required_value_treats_present_null_as_missing_at_an_own_site() {
    let mut schema = Schema::default()
        .vertex("record", "record")
        .vertex("scalar", "str")
        .edge("record", "scalar", "prop", Some("value"));
    schema.constraints.insert(
        "scalar".to_owned(),
        vec![("required".to_owned(), "true".to_owned())],
    );
    let semantics = BTreeMap::from([("required".to_owned(), Semantics::RequiredValue)]);
    for presence in [
        None,
        Some(Presence::Absent),
        Some(Presence::Null),
        Some(Presence::Present(Value::Null)),
    ] {
        let mut node = Node::new(0, "scalar");
        node.value = presence;
        let graph = Graph {
            nodes: [(0, node)].into(),
            ..Graph::default()
        };
        let report = validation::validate(&schema, &semantics, &graph);
        assert_eq!(report.diagnostics.len(), 1);
        let diagnostic = &report.diagnostics[0];
        assert_eq!(diagnostic.check.kind, CheckKind::RequiredValue);
        assert_eq!(diagnostic.subject, Some(ViewRef::Node(0)));
        assert_eq!(diagnostic.elements, vec![ViewRef::Node(0)]);
        assert_eq!(diagnostic.violation, Violation::MissingValue(None));
        assert_eq!(report.status, CheckStatus::Failed);
    }

    let present = Graph {
        nodes: [(
            0,
            Node::new(0, "scalar").with_value(Value::Str("present".to_owned())),
        )]
        .into(),
        ..Graph::default()
    };
    let report = validation::validate(&schema, &semantics, &present);
    assert!(report.diagnostics.is_empty());
    assert_eq!(report.status, CheckStatus::Passed);

    for value in [
        None,
        Some(Value::Null),
        Some(Value::Str("present".to_owned())),
    ] {
        let mut node = Node::new(1, "record");
        if let Some(value) = value.clone() {
            node.fields.insert("value".to_owned(), value);
        }
        let graph = Graph {
            nodes: [(1, node)].into(),
            ..Graph::default()
        };
        let report = validation::validate(&schema, &semantics, &graph);
        if matches!(value, Some(Value::Str(_))) {
            assert!(report.diagnostics.is_empty());
        } else {
            assert_eq!(report.diagnostics.len(), 1);
            assert_eq!(
                report.diagnostics[0].violation,
                Violation::MissingValue(Some("value".to_owned()))
            );
        }
    }
}

#[test]
fn the_probe_instance_fails_five_ways() {
    let works = SEdge::new("person", "org", "prop", Some("worksAt"));
    let mut s = Schema::default()
        .vertex("person", "obj")
        .vertex("org", "obj")
        .vertex("name", "str")
        .edge("person", "org", "prop", Some("worksAt"))
        .edge("person", "name", "prop", Some("name"));
    s.required.insert("person".into(), vec![works.clone()]);
    s.constraints
        .insert("name".into(), vec![("maxLength".into(), "3".into())]);
    let sem = BTreeMap::from([("maxLength".to_owned(), Semantics::MaxLength(true))]);
    let mut g = Graph::default();
    g.nodes.insert(
        0,
        Node::new(0, "person").with_field("name", Value::Str("Alice".into())),
    );
    g.nodes.insert(1, Node::new(1, "ghost"));
    g.nodes.insert(
        2,
        Node::new(2, "person").with_field("name", Value::Str("Bob".into())),
    );
    g.edges.insert(
        0,
        GEdge {
            src: 0,
            tgt: 2,
            label: works,
            position: None,
        },
    );
    g.edges.insert(
        1,
        GEdge {
            src: 2,
            tgt: 0,
            label: SEdge::new("person", "person", "prop", Some("knows")),
            position: None,
        },
    );
    let r = validation::validate(&s, &sem, &g);
    let kinds: Vec<CheckKind> = r.diagnostics.iter().map(|d| d.check.kind).collect();
    assert_eq!(
        kinds,
        vec![
            CheckKind::NodeAnchor,
            CheckKind::EdgeDeclared,
            CheckKind::EndpointCoherence,
            CheckKind::RequiredEdge,
            CheckKind::ValueConstraint
        ]
    );
    assert_eq!(r.status, CheckStatus::Failed);
    let unsupported = validation::validate(&s, &BTreeMap::new(), &g);
    assert!(!unsupported.incomplete.is_empty());
    assert_eq!(unsupported.status, CheckStatus::Failed);
}

#[test]
fn the_status_is_kleene_conjunction() {
    use CheckStatus::{Failed, Incomplete, Passed};
    for a in [Passed, Failed, Incomplete] {
        for b in [Passed, Failed, Incomplete] {
            let expected = if a == Failed || b == Failed {
                Failed
            } else if a == Incomplete || b == Incomplete {
                Incomplete
            } else {
                Passed
            };
            assert_eq!(a.and(b), expected);
        }
    }
    assert_eq!(CheckStatus::all([]), Passed);
}

#[test]
fn admission_policies_compare_exact_nonempty_bundles_and_virtual_cuts() {
    let empty: BTreeMap<&str, Vec<u8>> = BTreeMap::new();
    let baseline = BTreeMap::from([("value", vec![2, 7, 7])]);
    let cut = BTreeMap::from([("skipped", vec![1, 9])]);

    assert!(validation::admits(true, &empty, &empty, &empty));
    assert!(!validation::admits(true, &baseline, &cut, &baseline));
    assert!(validation::admits(false, &baseline, &cut, &baseline));
    assert!(validation::admits(
        false,
        &baseline,
        &cut,
        &BTreeMap::from([("skipped", vec![1, 9])]),
    ));
    assert!(!validation::admits(
        false,
        &baseline,
        &cut,
        &BTreeMap::from([("value", vec![1, 7])]),
    ));
    assert!(!validation::admits(
        false,
        &baseline,
        &cut,
        &BTreeMap::from([("new", vec![2, 7, 7])]),
    ));

    let conflicting_cut = BTreeMap::from([("value", vec![8])]);
    assert!(validation::admits(
        false,
        &baseline,
        &conflicting_cut,
        &baseline,
    ));
    assert!(!validation::admits(
        false,
        &baseline,
        &conflicting_cut,
        &conflicting_cut,
    ));
}

fn graph_input(anchor: &str) -> ValidationInput {
    ValidationInput::Graph {
        schema: Schema::default().vertex("a", "object"),
        semantics: BTreeMap::new(),
        instance: Graph {
            nodes: [(0, Node::new(0, anchor))].into(),
            ..Graph::default()
        },
    }
}

#[test]
fn no_new_witness_accepts_only_byte_equal_invalid_baseline_bundles() {
    let prior = graph_input("ghost");
    let baseline = validation::witness(prior.clone()).outcome;
    let retained = validation::witness_with_trust(
        prior.clone(),
        ValidationTrust {
            baseline_context: Some(ValidationContext::default()),
            baseline: Some(baseline.clone()),
            prior: Some(baseline.clone()),
            prior_input: Some(Box::new(prior.clone())),
            prior_policy: Some(PriorAdmissionPolicy::NoNewFailures),
            require_passed: false,
            ..ValidationTrust::default()
        },
    )
    .unwrap();
    assert!(retained.admitted);
    assert_eq!(retained.verify(), Ok(()));

    let changed = validation::witness_with_trust(
        graph_input("other"),
        ValidationTrust {
            baseline_context: Some(ValidationContext::default()),
            baseline: Some(baseline.clone()),
            prior: Some(baseline),
            prior_input: Some(Box::new(prior)),
            prior_policy: Some(PriorAdmissionPolicy::NoNewFailures),
            delta: vec![ViewRef::Node(0)],
            require_passed: false,
            ..ValidationTrust::default()
        },
    )
    .unwrap();
    assert!(!changed.admitted);
    assert_eq!(changed.verify(), Ok(()));
}

fn budget_evidence(
    budget: ValidationBudget,
    evaluation: &validation::BudgetEvaluation,
) -> BaselineBudgetEvidence {
    BaselineBudgetEvidence {
        budget,
        scheduled: evaluation.scheduled.clone(),
        skipped: evaluation.skipped.clone(),
        spent: evaluation.spent,
    }
}

fn assert_budget_evidence_mutations_do_not_verify(witness: &validation::ValidationWitness) {
    let mut mutated_budget = witness.clone();
    mutated_budget
        .trust
        .baseline_budget
        .as_mut()
        .unwrap()
        .budget
        .max_reads = Some(u64::MAX);
    assert_eq!(mutated_budget.verify(), Err(WitnessError::WitnessMismatch));

    let mut mutated_schedule = witness.clone();
    mutated_schedule
        .trust
        .baseline_budget
        .as_mut()
        .unwrap()
        .scheduled
        .pop();
    assert_eq!(
        mutated_schedule.verify(),
        Err(WitnessError::WitnessMismatch)
    );

    let mut mutated_skipped = witness.clone();
    mutated_skipped
        .trust
        .baseline_budget
        .as_mut()
        .unwrap()
        .skipped
        .pop();
    assert_eq!(mutated_skipped.verify(), Err(WitnessError::WitnessMismatch));

    let mut mutated_spent = witness.clone();
    mutated_spent.trust.baseline_budget.as_mut().unwrap().spent += 1;
    assert_eq!(mutated_spent.verify(), Err(WitnessError::WitnessMismatch));
}

fn assert_invalid_budget_evidence_is_rejected(
    input: &ValidationInput,
    exact_trust: &ValidationTrust,
) {
    let mut bad_budget = exact_trust.clone();
    bad_budget
        .baseline_budget
        .as_mut()
        .unwrap()
        .budget
        .max_reads = Some(u64::MAX);
    assert!(matches!(
        validation::witness_with_trust(input.clone(), bad_budget),
        Err(WitnessError::IncrementalMismatch)
    ));

    let mut bad_schedule = exact_trust.clone();
    bad_schedule
        .baseline_budget
        .as_mut()
        .unwrap()
        .scheduled
        .pop();
    assert!(matches!(
        validation::witness_with_trust(input.clone(), bad_schedule),
        Err(WitnessError::IncrementalMismatch)
    ));

    let mut bad_skipped = exact_trust.clone();
    bad_skipped.baseline_budget.as_mut().unwrap().skipped.pop();
    assert!(matches!(
        validation::witness_with_trust(input.clone(), bad_skipped),
        Err(WitnessError::IncrementalMismatch)
    ));

    let mut bad_spent = exact_trust.clone();
    bad_spent.baseline_budget.as_mut().unwrap().spent += 1;
    assert!(matches!(
        validation::witness_with_trust(input.clone(), bad_spent),
        Err(WitnessError::IncrementalMismatch)
    ));
}

#[test]
fn no_new_witness_replays_budget_cuts_and_rejects_newly_discovered_failures() {
    let prior = graph_input("ghost");
    let baseline = validation::witness(prior.clone()).outcome;
    let cut_budget = ValidationBudget { max_reads: Some(0) };
    let budgeted = validation::witness_with_trust(
        prior.clone(),
        ValidationTrust {
            budget: cut_budget,
            ..ValidationTrust::default()
        },
    )
    .unwrap();
    let evidence = budget_evidence(cut_budget, &budgeted.evaluation);
    assert!(evidence.skipped.iter().any(|instance| {
        instance.check.kind == CheckKind::NodeAnchor && instance.subject == Some(ViewRef::Node(0))
    }));

    let trust = |target_budget| ValidationTrust {
        baseline_context: Some(ValidationContext::default()),
        baseline: Some(baseline.clone()),
        baseline_budget: Some(evidence.clone()),
        prior: Some(baseline.clone()),
        prior_input: Some(Box::new(prior.clone())),
        prior_policy: Some(PriorAdmissionPolicy::NoNewFailures),
        require_passed: false,
        budget: target_budget,
        ..ValidationTrust::default()
    };
    let exact_trust = trust(cut_budget);
    let admitted = validation::witness_with_trust(prior.clone(), exact_trust.clone()).unwrap();
    assert!(admitted.admitted);
    assert!(admitted.evaluation.report.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::NodeAnchor
            && entry.class == CheckClass::GraphLocal
            && entry.requirement == Requirement::Required
            && entry.subject == Some(ViewRef::Node(0))
            && matches!(
                entry.reason,
                IncompleteReason::Budget {
                    limit: 0,
                    spent: 0,
                    from: Some(ViewRef::Node(0))
                }
            )
    }));
    assert_eq!(admitted.verify(), Ok(()));
    assert_budget_evidence_mutations_do_not_verify(&admitted);

    let revealed =
        validation::witness_with_trust(prior.clone(), trust(ValidationBudget::default())).unwrap();
    assert!(!revealed.admitted);
    assert!(revealed.evaluation.report.diagnostics.iter().any(|entry| {
        entry.check.kind == CheckKind::NodeAnchor
            && entry.subject == Some(ViewRef::Node(0))
            && entry.violation == Violation::UnknownAnchor("ghost".to_owned())
    }));

    assert_invalid_budget_evidence_is_rejected(&prior, &exact_trust);
}

#[test]
fn validation_key_binding_and_form_observables_execute_their_exact_paths() {
    assert_eq!(
        validation::key_equality(
            &KeyEquality::Representation,
            &Value::Str("same".to_owned()),
            &Value::Str("same".to_owned()),
        ),
        Some(true),
    );
    assert_eq!(
        validation::key_equality(
            &KeyEquality::Representation,
            &Value::Str("left".to_owned()),
            &Value::Str("right".to_owned()),
        ),
        Some(false),
    );
    assert_eq!(
        validation::key_equality(
            &KeyEquality::Sql {
                collation: SqlCollation::Named("vendor".to_owned()),
                pad: SqlPad::NoPad,
            },
            &Value::Str("a".to_owned()),
            &Value::Str("a".to_owned()),
        ),
        None,
    );

    let definitions = vec![
        (7, "x".to_owned(), 70),
        (2, "x".to_owned(), 20),
        (9, "x".to_owned(), 90),
        (1, "y".to_owned(), 10),
    ];
    assert_eq!(
        validation::variable_binding(&definitions, 8, "x", true),
        Some(70),
    );
    assert_eq!(
        validation::variable_binding(&definitions, 8, "x", false),
        Some(20),
    );
    assert_eq!(
        validation::variable_binding(&definitions, 2, "x", true),
        None,
    );

    let duplicate = Key {
        id: "duplicate".to_owned(),
        parts: vec![KeyPart::Path(Vec::new())],
        role: KeyRole::Index,
        scope: KeyScope::Document,
        equality: Vec::new(),
    };
    let mut malformed = Schema::default().vertex("v", "object");
    malformed
        .keys
        .insert("v".to_owned(), vec![duplicate.clone(), duplicate]);
    assert_eq!(
        validation::key_forms(&malformed),
        Err(vec![DeclarationError::DuplicateKeyId {
            vertex: "v".to_owned(),
            key: "duplicate".to_owned(),
        }]),
    );
    assert_eq!(
        validation::key_forms(&Schema::default().vertex("v", "object")),
        Ok(()),
    );
}

#[test]
fn validation_index_and_lexical_observables_are_owned_and_nonvacuous() {
    assert!(validation::index_and_null_match(Some(4), Some(4), false));
    assert!(!validation::index_and_null_match(Some(4), Some(5), true));
    assert!(validation::index_and_null_match(None, None, true));
    assert!(!validation::index_and_null_match(None, None, false));
    assert!(!validation::index_and_null_match(Some(4), None, true));
    assert!(!validation::index_and_null_match(None, Some(4), true));

    let mut seen = None;
    assert!(validation::lexical_space("urn:literal", |text| {
        seen = Some(text.to_owned());
        text == "urn:literal"
    }));
    assert_eq!(seen, Some("urn:literal".to_owned()));
    assert!(!validation::lexical_space("urn:literal", |_| false));
}

#[test]
fn validation_format_observables_distinguish_advisory_and_asserting_use() {
    assert!(validation::annotated_format_parse(&Some(7), &Some(7)));
    assert!(!validation::annotated_format_parse(&Some(7), &Some(8)));
    assert!(validation::annotated_format_parse::<u8>(&None, &None));

    assert!(validation::annotated_format_advisory(
        "accepted",
        |text| text == "accepted",
        false,
    ));
    assert!(validation::annotated_format_advisory(
        "rejected",
        |_| false,
        true,
    ));
    assert!(!validation::annotated_format_advisory(
        "accepted",
        |_| true,
        true,
    ));

    assert!(validation::format_assertion_acceptance(
        "accepted",
        |text| text == "accepted",
        true,
    ));
    assert!(validation::format_assertion_acceptance(
        "rejected",
        |_| false,
        false,
    ));
    assert!(!validation::format_assertion_acceptance(
        "rejected",
        |_| false,
        true,
    ));
}

#[test]
fn an_edge_to_a_missing_node_is_an_endpoint_finding_not_a_panic() {
    let works = SEdge::new("person", "org", "prop", Some("worksAt"));
    let s = Schema::default()
        .vertex("person", "obj")
        .vertex("org", "obj")
        .edge("person", "org", "prop", Some("worksAt"));
    let mut g = Graph::default();
    g.nodes.insert(0, Node::new(0, "person"));
    g.edges.insert(
        0,
        GEdge {
            src: 0,
            tgt: 9,
            label: works,
            position: None,
        },
    );
    let r = validation::validate(&s, &BTreeMap::new(), &g);
    assert_eq!(r.diagnostics.len(), 1);
    assert_eq!(r.diagnostics[0].check.kind, CheckKind::EndpointCoherence);
    assert_eq!(
        format!("{:?}", r.diagnostics[0].violation),
        "DanglingEndpoint(false, 9)"
    );
    assert_eq!(r.status, CheckStatus::Failed);
}

#[test]
fn path_and_many_references_are_executable_or_explicitly_incomplete() {
    let (schema, graph) = path_and_many_fixture();
    let report = validation::validate(&schema, &BTreeMap::new(), &graph);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        matches!(
            &diagnostic.violation,
            panproto_reference::validation::Violation::DanglingReference(reference, values)
                if reference == "path" && values == &[Value::Str("/missing".into())]
        )
    }));
    assert!(report.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::ReferenceResolves
            && entry.subject == Some(panproto_reference::lens::ViewRef::Node(0))
            && matches!(
                &entry.reason,
                validation::IncompleteReason::Unsupported { detail }
                    if detail.contains("ReferenceCardinalityMismatch")
            )
    }));
    assert_eq!(report.status, CheckStatus::Failed);

    let mut invalid = schema;
    invalid.references.get_mut("path").unwrap().target_key = Some("absent".into());
    let invalid_report = validation::validate(&invalid, &BTreeMap::new(), &graph);
    assert_eq!(invalid_report.status, CheckStatus::Failed);
    assert!(invalid_report.incomplete.iter().any(|entry| {
        matches!(
            entry.reason,
            validation::IncompleteReason::IllFormedDeclaration { .. }
        )
    }));
}

fn path_and_many_fixture() -> (Schema, Graph) {
    let path = SEdge::new("holder", "scalar", "prop", Some("path"));
    let left = SEdge::new("holder", "scalar", "prop", Some("left"));
    let right = SEdge::new("holder", "scalar", "prop", Some("right"));
    let id = SEdge::new("target", "scalar", "prop", Some("id"));
    let x = SEdge::new("target", "scalar", "prop", Some("x"));
    let y = SEdge::new("target", "scalar", "prop", Some("y"));
    let path_link = SEdge::new("holder", "target", "ref", Some("path_target"));
    let many_link = SEdge::new("holder", "target", "ref", Some("many_target"));
    let mut schema = Schema::default()
        .vertex("holder", "object")
        .vertex("target", "object")
        .vertex("scalar", "value");
    schema.edges.extend([
        path.clone(),
        left.clone(),
        right.clone(),
        id.clone(),
        x.clone(),
        y.clone(),
        path_link.clone(),
        many_link.clone(),
    ]);
    schema.keys.insert(
        "target".into(),
        vec![
            Key {
                id: "id".into(),
                parts: vec![KeyPart::Path(vec![id])],
                role: KeyRole::Identifier,
                scope: KeyScope::Document,
                equality: Vec::new(),
            },
            Key {
                id: "pair".into(),
                parts: vec![KeyPart::Path(vec![x]), KeyPart::Path(vec![y])],
                role: KeyRole::Unique,
                scope: KeyScope::Document,
                equality: Vec::new(),
            },
        ],
    );
    let reference = |id: &str, source: RefSource, key: &str, edge: SEdge| Reference {
        id: id.into(),
        holder: "holder".into(),
        source,
        target_vertex: "target".into(),
        target_key: Some(key.into()),
        edge: Some(edge),
        cardinality: if id == "many" {
            Cardinality::Many
        } else {
            Cardinality::One
        },
        nullable: false,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
        guard: None,
        null_match: panproto_reference::schema::NullMatch::Simple,
        null_vertex: None,
        scope_via: None,
    };
    schema.references.insert(
        "path".into(),
        reference(
            "path",
            RefSource::Path {
                field: vec![path],
                syntax: PathSyntax::JsonPointer,
            },
            "id",
            path_link,
        ),
    );
    schema.references.insert(
        "many".into(),
        reference(
            "many",
            RefSource::KeyValue {
                parts: vec![vec![left], vec![right]],
            },
            "pair",
            many_link,
        ),
    );
    (schema, path_and_many_graph())
}

fn path_and_many_graph() -> Graph {
    Graph {
        nodes: BTreeMap::from([
            (
                0,
                Node::new(0, "holder")
                    .with_field("path", Value::Str("/missing".into()))
                    .with_field("left", Value::List(vec![Value::Int(1), Value::Int(2)]))
                    .with_field("right", Value::List(vec![Value::Int(1)])),
            ),
            (
                1,
                Node::new(1, "target")
                    .with_field("id", Value::Str("/present".into()))
                    .with_field("x", Value::Int(1))
                    .with_field("y", Value::Int(1)),
            ),
        ]),
        edges: BTreeMap::new(),
    }
}

#[test]
fn unsupported_and_ill_formed_declarations_are_typed_incomplete() {
    let mut schema = Schema::default().vertex("v", "object");
    schema.constraints.insert(
        "v".to_owned(),
        vec![
            ("unknown".to_owned(), "x".to_owned()),
            ("length".to_owned(), "not-a-count".to_owned()),
        ],
    );
    let semantics = BTreeMap::from([
        ("unknown".to_owned(), Semantics::Other),
        ("length".to_owned(), Semantics::MaxLength(true)),
    ]);
    let report = validation::validate(&schema, &semantics, &Graph::default());
    assert_eq!(report.shape, InstanceShape::Graph);
    assert_eq!(report.status, CheckStatus::Incomplete);
    assert!(report.incomplete.iter().any(|entry| {
        entry.class == CheckClass::Static && entry.check.kind == CheckKind::DeclarationInterpretable
    }));
    let unknown_kinds: std::collections::BTreeSet<_> = report
        .incomplete
        .iter()
        .filter(|entry| {
            matches!(
                entry.check.decl,
                SchemaRef::Constraint(_, ref sort, _) if sort == "unknown"
            )
        })
        .map(|entry| entry.check.kind)
        .collect();
    assert_eq!(unknown_kinds, [CheckKind::DeclarationInterpretable].into());
    assert!(report.incomplete.iter().any(|entry| matches!(
        entry.reason,
        IncompleteReason::UnsupportedDeclaration { ref sort } if sort == "unknown"
    )));
    let length_kinds: BTreeSet<_> = report
        .incomplete
        .iter()
        .filter(|entry| {
            matches!(
                entry.check.decl,
                SchemaRef::Constraint(_, ref sort, _) if sort == "length"
            )
        })
        .map(|entry| entry.check.kind)
        .collect();
    assert_eq!(
        length_kinds,
        [
            CheckKind::DeclarationInterpretable,
            CheckKind::ValueConstraint
        ]
        .into()
    );
    assert!(
        report
            .incomplete
            .iter()
            .any(|entry| matches!(entry.reason, IncompleteReason::IllFormedDeclaration { .. }))
    );
}

#[test]
fn segment_scope_preserves_global_checks_as_typed_incomplete() {
    let mut schema = Schema::default().vertex("v", "object");
    schema.keys.insert(
        "v".to_owned(),
        vec![Key {
            id: "id".to_owned(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Primary,
            scope: KeyScope::Document,
            equality: Vec::new(),
        }],
    );
    let context = ValidationContext {
        external: BTreeMap::new(),
        complete_segment: false,
    };
    let report = validation::validate_graph_with_context(
        &schema,
        &BTreeMap::new(),
        &Graph::default(),
        &context,
    );
    assert!(report.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::KeyUnique
            && matches!(
                entry.reason,
                IncompleteReason::OutsideSegment {
                    class: CheckClass::MaintainedGlobal
                }
            )
    }));
}

fn advisory_reference_fixture() -> (Schema, Graph) {
    let link = SEdge::new("holder", "target", "rel", Some("link"));
    let mut schema = Schema::default()
        .vertex("holder", "scalar")
        .vertex("target", "scalar");
    schema.edges.insert(link.clone());
    schema.keys.insert(
        "target".to_owned(),
        vec![Key {
            id: "id".to_owned(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Identifier,
            scope: KeyScope::Document,
            equality: Vec::new(),
        }],
    );
    schema.references.insert(
        "lookup".to_owned(),
        Reference {
            id: "lookup".to_owned(),
            holder: "holder".to_owned(),
            source: RefSource::Path {
                field: Vec::new(),
                syntax: PathSyntax::JsonPointer,
            },
            target_vertex: "target".to_owned(),
            target_key: Some("id".to_owned()),
            edge: Some(link),
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Complement,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: panproto_reference::schema::NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    let graph = Graph {
        nodes: [
            (0, Node::new(0, "holder").with_value(Value::Int(1))),
            (1, Node::new(1, "target").with_value(Value::Int(1))),
        ]
        .into(),
        ..Graph::default()
    };
    (schema, graph)
}

#[test]
fn advisory_references_remain_advisory_in_segments_schedules_and_budget_cuts() {
    let (schema, graph) = advisory_reference_fixture();
    let input = ValidationInput::Graph {
        schema: schema.clone(),
        semantics: BTreeMap::new(),
        instance: graph.clone(),
    };
    let schedule = validation::trusted_schedule(&input, &ValidationTrust::default());
    let reference_runs: Vec<_> = schedule
        .runs()
        .iter()
        .filter(|run| run.instance.check.kind == CheckKind::ReferenceResolves)
        .collect();
    assert!(!reference_runs.is_empty());
    assert!(
        reference_runs
            .iter()
            .all(|run| run.requirement == Requirement::Advisory)
    );

    let partial_context = ValidationContext {
        external: BTreeMap::new(),
        complete_segment: false,
    };
    let partial = validation::validate_graph_with_context(
        &schema,
        &BTreeMap::new(),
        &graph,
        &partial_context,
    );
    assert!(partial.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::ReferenceResolves
            && entry.requirement == Requirement::Advisory
            && matches!(entry.reason, IncompleteReason::OutsideSegment { .. })
    }));

    let budgeted = validation::witness_with_trust(
        input,
        ValidationTrust {
            budget: ValidationBudget { max_reads: Some(0) },
            ..ValidationTrust::default()
        },
    )
    .unwrap();
    assert!(budgeted.evaluation.report.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::ReferenceResolves
            && entry.requirement == Requirement::Advisory
    }));
}

#[test]
fn schema_errors_mark_every_compiled_dependent_incomplete() {
    let mut schema = Schema::default().vertex("v", "object");
    schema.keys.insert(
        "v".to_owned(),
        vec![Key {
            id: "bad".to_owned(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Primary,
            scope: KeyScope::Document,
            equality: vec![KeyEquality::Representation, KeyEquality::Representation],
        }],
    );
    let report = validation::validate(&schema, &BTreeMap::new(), &Graph::default());
    let kinds: BTreeSet<_> = report
        .incomplete
        .iter()
        .map(|entry| entry.check.kind)
        .collect();
    assert!(kinds.contains(&CheckKind::SchemaWellFormed));
    assert!(kinds.contains(&CheckKind::KeyUnique));
    assert!(kinds.contains(&CheckKind::RequiredValue));
    assert!(!kinds.contains(&CheckKind::KeyOrder));
}

#[test]
fn duplicate_key_ids_union_role_dependencies_without_order_bias() {
    let primary = Key {
        id: "same".to_owned(),
        parts: vec![KeyPart::Path(Vec::new())],
        role: KeyRole::Primary,
        scope: KeyScope::Document,
        equality: Vec::new(),
    };
    let sort = Key {
        role: KeyRole::SortKey,
        scope: KeyScope::Within("v".to_owned()),
        ..primary.clone()
    };
    for declarations in [vec![primary.clone(), sort.clone()], vec![sort, primary]] {
        let mut schema = Schema::default().vertex("v", "object");
        schema.keys.insert("v".to_owned(), declarations);
        let report = validation::validate(&schema, &BTreeMap::new(), &Graph::default());
        let kinds: BTreeSet<_> = report
            .incomplete
            .iter()
            .filter(|entry| entry.check.decl == SchemaRef::Key("v".to_owned(), "same".to_owned()))
            .map(|entry| entry.check.kind)
            .collect();
        assert_eq!(
            kinds,
            [
                CheckKind::KeyUnique,
                CheckKind::RequiredValue,
                CheckKind::KeyOrder,
            ]
            .into()
        );
    }
}

fn target_set_reference(id: &str, target: &str) -> Reference {
    Reference {
        id: id.to_owned(),
        holder: "holder".to_owned(),
        source: RefSource::Path {
            field: Vec::new(),
            syntax: PathSyntax::JsonPointer,
        },
        target_vertex: target.to_owned(),
        target_key: None,
        edge: None,
        cardinality: Cardinality::One,
        nullable: false,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
        guard: None,
        null_match: panproto_reference::schema::NullMatch::Simple,
        null_vertex: None,
        scope_via: None,
    }
}

#[test]
fn target_set_errors_invalidate_targeting_references_but_not_member_keys() {
    let mut schema = Schema::default()
        .vertex("holder", "object")
        .vertex("a", "scalar")
        .vertex("b", "scalar");
    schema.keys.insert(
        "a".to_owned(),
        vec![Key {
            id: "id".to_owned(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Identifier,
            scope: KeyScope::Document,
            equality: vec![KeyEquality::Representation],
        }],
    );
    schema.keys.insert(
        "b".to_owned(),
        vec![Key {
            id: "id".to_owned(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Primary,
            scope: KeyScope::Document,
            equality: vec![KeyEquality::NumericValue],
        }],
    );
    schema.target_sets.insert(
        "ids".to_owned(),
        TargetSet {
            members: vec![
                ("a".to_owned(), "id".to_owned()),
                ("b".to_owned(), "id".to_owned()),
            ],
            scope: KeyScope::Document,
        },
    );
    schema
        .references
        .insert("to-ids".to_owned(), target_set_reference("to-ids", "ids"));
    let report = validation::validate(&schema, &BTreeMap::new(), &Graph::default());
    assert!(report.incomplete.iter().any(|entry| {
        entry.check
            == CheckKey {
                kind: CheckKind::ReferenceResolves,
                decl: SchemaRef::Reference("to-ids".to_owned()),
            }
    }));
    assert!(
        report
            .incomplete
            .iter()
            .any(|entry| { entry.check.decl == SchemaRef::TargetSet("ids".to_owned()) })
    );
    assert!(!report.incomplete.iter().any(|entry| {
        entry.check.decl == SchemaRef::Key("b".to_owned(), "id".to_owned())
            && matches!(entry.reason, IncompleteReason::IllFormedDeclaration { .. })
    }));

    let mut clash = schema;
    clash.vertices.insert("ids".to_owned(), "object".to_owned());
    let report = validation::validate(&clash, &BTreeMap::new(), &Graph::default());
    assert!(report.incomplete.iter().any(|entry| {
        entry.check.decl == SchemaRef::Reference("to-ids".to_owned())
            && matches!(entry.reason, IncompleteReason::IllFormedDeclaration { .. })
    }));
}

#[test]
fn table_validation_preserves_native_row_and_pair_identity() {
    let mut schema = Schema::default()
        .vertex("t", "object")
        .vertex("v", "scalar")
        .edge("t", "v", "prop", Some("x"));
    schema.constraints.insert(
        "v".to_owned(),
        vec![("required".to_owned(), "true".to_owned())],
    );
    let table = Table {
        rows: [(7, ("t".to_owned(), BTreeMap::new()))].into(),
        ..Table::default()
    };
    let report = validation::validate_table(
        &schema,
        &BTreeMap::from([("required".to_owned(), Semantics::RequiredValue)]),
        &table,
    )
    .unwrap();
    assert_eq!(report.shape, InstanceShape::Table);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].subject, Some(ViewRef::Row(7)));
    assert_eq!(report.diagnostics[0].elements, vec![ViewRef::Row(7)]);
    assert_eq!(report.identity[&ViewRef::Row(7)], NativeRef::TableRow(7));

    let relation = SEdge::new("t", "t", "rel", Some("r"));
    let malformed = Table {
        rows: [(7, ("t".to_owned(), BTreeMap::new()))].into(),
        pairs: [(relation, 7, 9)].into(),
    };
    assert!(matches!(
        validation::validate_table(&schema, &BTreeMap::new(), &malformed),
        Err(ShapeRefusal::Table(_))
    ));
}

#[test]
fn tree_validation_distinguishes_findings_from_shape_refusals() {
    let schema = Schema::default().vertex("v", "object");
    let missing_root = Tree {
        nodes: [(0, Node::new(0, "v"))].into(),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 9,
    };
    let report = validation::validate_tree(&schema, &BTreeMap::new(), &missing_root).unwrap();
    assert_eq!(report.shape, InstanceShape::Tree);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.check.kind == CheckKind::TreeRoot
            && diagnostic.violation == Violation::MissingRoot
    }));

    let unreachable = Tree {
        nodes: [(0, Node::new(0, "v")), (1, Node::new(1, "v"))].into(),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    let report = validation::validate_tree(&schema, &BTreeMap::new(), &unreachable).unwrap();
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.check.kind == CheckKind::TreeReachable
            && diagnostic.subject == Some(ViewRef::Node(1))
    }));

    let edge = SEdge::new("v", "v", "prop", Some("e"));
    let second_parent = Tree {
        nodes: [
            (0, Node::new(0, "v")),
            (1, Node::new(1, "v")),
            (2, Node::new(2, "v")),
        ]
        .into(),
        arcs: vec![(0, 2, edge.clone()), (1, 2, edge)],
        fans: Vec::new(),
        root: 0,
    };
    let report = validation::validate_tree(&schema, &BTreeMap::new(), &second_parent).unwrap();
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.check.kind == CheckKind::TreeParentMap
            && diagnostic.subject == Some(ViewRef::Node(2))
            && diagnostic.violation == Violation::ParentMismatch(Some(0), Some(1))
    }));
    assert_eq!(report.identity[&ViewRef::Edge(0)], NativeRef::TreeArc(0));
    assert_eq!(report.identity[&ViewRef::Edge(1)], NativeRef::TreeArc(1));
}

fn check(kind: CheckKind, vertex: &str) -> CheckKey {
    CheckKey {
        kind,
        decl: SchemaRef::Vertex(vertex.to_owned()),
    }
}

fn failed_run(instance: CheckInstance, reads: Vec<ViewRef>) -> CheckRun {
    CheckRun {
        class: instance.check.kind.class(),
        diagnostics: vec![Diagnostic {
            check: instance.check.clone(),
            class: instance.check.kind.class(),
            requirement: Requirement::Required,
            subject: instance.subject.clone(),
            elements: reads.clone(),
            violation: Violation::UnknownAnchor("ghost".to_owned()),
        }],
        instance,
        requirement: Requirement::Required,
        reads,
        incomplete: Vec::new(),
    }
}

const fn passing_run(instance: CheckInstance, reads: Vec<ViewRef>) -> CheckRun {
    CheckRun {
        class: instance.check.kind.class(),
        instance,
        requirement: Requirement::Required,
        reads,
        diagnostics: Vec::new(),
        incomplete: Vec::new(),
    }
}

#[test]
fn run_order_uses_canonical_entry_bytes_and_strict_element_lists() {
    let instance = CheckInstance {
        check: check(CheckKind::External, "v"),
        subject: Some(ViewRef::Node(0)),
    };
    let diagnostic = |name: &str| Diagnostic {
        check: instance.check.clone(),
        class: CheckClass::BoundedExternal,
        requirement: Requirement::Required,
        subject: instance.subject.clone(),
        elements: vec![ViewRef::Node(0)],
        violation: Violation::ExternalRejected(name.to_owned()),
    };
    let canonical = CheckRun {
        class: CheckClass::BoundedExternal,
        instance: instance.clone(),
        requirement: Requirement::Required,
        reads: vec![ViewRef::Node(0)],
        diagnostics: vec![diagnostic("z"), diagnostic("aa")],
        incomplete: Vec::new(),
    };
    assert!(
        validation::evaluate_runs(
            InstanceShape::Graph,
            std::slice::from_ref(&canonical),
            &[],
            ValidationBudget::default(),
        )
        .is_ok()
    );
    let mut reversed = canonical.clone();
    reversed.diagnostics.reverse();
    assert!(matches!(
        validation::evaluate_runs(
            InstanceShape::Graph,
            &[reversed],
            &[],
            ValidationBudget::default(),
        ),
        Err(RunBundleError::NonCanonicalOutcomes(_))
    ));
    let mut duplicate_elements = canonical;
    duplicate_elements.diagnostics.truncate(1);
    duplicate_elements.diagnostics[0]
        .elements
        .push(ViewRef::Node(0));
    assert!(matches!(
        validation::evaluate_runs(
            InstanceShape::Graph,
            &[duplicate_elements],
            &[],
            ValidationBudget::default(),
        ),
        Err(RunBundleError::NonCanonicalElements(_))
    ));
}

#[test]
fn budget_is_an_ordered_cut_with_exact_coverage_and_failed_dominance() {
    let first = CheckInstance {
        check: check(CheckKind::NodeAnchor, "a"),
        subject: Some(ViewRef::Node(0)),
    };
    let second = CheckInstance {
        check: check(CheckKind::EdgeDeclared, "b"),
        subject: Some(ViewRef::Edge(0)),
    };
    let third = CheckInstance {
        check: check(CheckKind::EdgeDeclared, "b"),
        subject: Some(ViewRef::Edge(1)),
    };
    let runs = vec![
        failed_run(first.clone(), vec![ViewRef::Node(0)]),
        passing_run(second.clone(), vec![ViewRef::Node(1), ViewRef::Edge(0)]),
        passing_run(third.clone(), vec![ViewRef::Node(2), ViewRef::Edge(1)]),
    ];
    let evaluation = validation::evaluate_runs(
        InstanceShape::Graph,
        &runs,
        &[],
        ValidationBudget { max_reads: Some(1) },
    )
    .unwrap();
    assert_eq!(
        evaluation.scheduled,
        vec![first.clone(), second.clone(), third.clone()]
    );
    assert_eq!(evaluation.skipped, vec![second.clone(), third.clone()]);
    assert_eq!(evaluation.spent, 1);
    assert_eq!(evaluation.report.status, CheckStatus::Failed);
    assert_eq!(evaluation.coverage[&first.check].failed, 1);
    assert_eq!(
        evaluation.coverage[&first.check].class,
        CheckClass::GraphLocal
    );
    assert_eq!(
        evaluation.coverage[&first.check].requirement,
        Requirement::Required
    );
    assert_eq!(
        evaluation.coverage[&check(CheckKind::EdgeDeclared, "b")].incomplete,
        2
    );
    let exact_skipped: Vec<_> = evaluation
        .report
        .incomplete
        .iter()
        .map(|entry| {
            let IncompleteReason::Budget { limit, spent, from } = &entry.reason else {
                panic!("only budget incompleteness expected")
            };
            (
                entry.check.clone(),
                entry.class,
                entry.requirement,
                entry.subject.clone(),
                *limit,
                *spent,
                from.clone(),
            )
        })
        .collect();
    assert_eq!(
        exact_skipped,
        vec![
            (
                second.check,
                CheckClass::GraphLocal,
                Requirement::Required,
                Some(ViewRef::Edge(0)),
                1,
                1,
                Some(ViewRef::Edge(0)),
            ),
            (
                third.check,
                CheckClass::GraphLocal,
                Requirement::Required,
                Some(ViewRef::Edge(1)),
                1,
                1,
                Some(ViewRef::Edge(0)),
            ),
        ]
    );
}

#[test]
fn a_skipped_advisory_run_does_not_make_status_incomplete() {
    let instance = CheckInstance {
        check: check(CheckKind::KeyOrder, "v"),
        subject: Some(ViewRef::Node(0)),
    };
    let mut run = passing_run(instance, vec![ViewRef::Node(0)]);
    run.requirement = Requirement::Advisory;
    let evaluation = validation::evaluate_runs(
        InstanceShape::Graph,
        &[run],
        &[],
        ValidationBudget { max_reads: Some(0) },
    )
    .unwrap();
    assert_eq!(evaluation.report.status, CheckStatus::Passed);
    assert_eq!(evaluation.report.incomplete.len(), 1);
    assert_eq!(
        evaluation.report.incomplete[0].requirement,
        Requirement::Advisory
    );
}

fn assert_witness_mutation_is_rejected() {
    let witness = validation::witness(ValidationInput::Graph {
        schema: Schema::default().vertex("a", "object"),
        semantics: BTreeMap::new(),
        instance: Graph {
            nodes: [(0, Node::new(0, "ghost"))].into(),
            ..Graph::default()
        },
    });
    assert_eq!(witness.verify(), Ok(()));
    let mut mutated = witness;
    let ValidationOutcome::Report(report) = &mut mutated.outcome else {
        panic!("graph validation returns a report");
    };
    report.diagnostics.clear();
    report.status = CheckStatus::Passed;
    assert_eq!(mutated.verify(), Err(WitnessError::WitnessMismatch));

    let replacement = validation::witness(ValidationInput::Graph {
        schema: Schema::default().vertex("b", "object"),
        semantics: BTreeMap::new(),
        instance: Graph {
            nodes: [(4, Node::new(4, "b"))].into(),
            ..Graph::default()
        },
    });
    mutated.input = replacement.input;
    mutated.trust = replacement.trust;
    mutated.schedule = replacement.schedule;
    mutated.outcome = replacement.outcome;
    mutated.evaluation = replacement.evaluation;
    mutated.admitted = replacement.admitted;
    assert_eq!(mutated.verify(), Err(WitnessError::WitnessMismatch));
}

fn assert_trusted_schedule_rejects_omission_and_reordering() {
    let trusted = validation::trusted_schedule(
        &ValidationInput::Graph {
            schema: Schema::default().vertex("a", "object"),
            semantics: BTreeMap::new(),
            instance: Graph {
                nodes: [(0, Node::new(0, "a"))].into(),
                ..Graph::default()
            },
        },
        &ValidationTrust::default(),
    );
    let complete = trusted.instances();
    assert!(complete.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(!complete.iter().any(|instance| {
        instance.check.kind == CheckKind::NodeAnchor && instance.check.decl == SchemaRef::Schema
    }));
    assert!(trusted.runs().iter().any(|run| {
        run.instance.check.kind == CheckKind::NodeAnchor
            && run.diagnostics.is_empty()
            && run.incomplete.is_empty()
            && run.reads == vec![ViewRef::Node(0)]
    }));
    assert_eq!(trusted.verify_instances(&complete), Ok(()));
    assert_eq!(
        trusted.verify_instances(&complete[1..]),
        Err(RunBundleError::ScheduleMismatch)
    );
    let mut reordered = complete;
    reordered.swap(0, 1);
    assert_eq!(
        trusted.verify_instances(&reordered),
        Err(RunBundleError::ScheduleMismatch)
    );
}

fn assert_run_bundle_rejects_noncanonical_inputs() {
    let instance = CheckInstance {
        check: check(CheckKind::NodeAnchor, "a"),
        subject: Some(ViewRef::Node(0)),
    };
    let run = passing_run(instance.clone(), vec![ViewRef::Node(0)]);
    let noncanonical = passing_run(instance, vec![ViewRef::Node(1), ViewRef::Node(0)]);
    assert!(matches!(
        validation::evaluate_runs(
            InstanceShape::Graph,
            &[noncanonical],
            &[],
            ValidationBudget::default()
        ),
        Err(RunBundleError::NonCanonicalReads(_))
    ));
    let later = CheckInstance {
        check: check(CheckKind::EdgeDeclared, "b"),
        subject: Some(ViewRef::Edge(0)),
    };
    assert_eq!(
        validation::evaluate_runs(
            InstanceShape::Graph,
            &[
                passing_run(later, Vec::new()),
                passing_run(run.instance.clone(), Vec::new()),
            ],
            &[],
            ValidationBudget::default()
        ),
        Err(RunBundleError::NonCanonicalSchedule)
    );
    assert!(matches!(
        validation::evaluate_runs(
            InstanceShape::Graph,
            &[run.clone(), run],
            &[],
            ValidationBudget::default()
        ),
        Err(RunBundleError::DuplicateInstance(_))
    ));
}

#[test]
fn validation_witness_replays_and_rejects_mutation_and_bad_schedules() {
    assert_witness_mutation_is_rejected();
    assert_trusted_schedule_rejects_omission_and_reordering();
    assert_run_bundle_rejects_noncanonical_inputs();
}

fn rich_schedule_input() -> ValidationInput {
    let field = SEdge::new("record", "scalar", "prop", Some("field"));
    let child = SEdge::new("record", "child", "prop", Some("child"));
    let next = SEdge::new("record", "record", "rel", Some("next"));
    let mut schema = Schema::default()
        .vertex("record", "object")
        .vertex("scalar", "string")
        .vertex("child", "int");
    schema.edges.extend([field, child.clone(), next]);
    schema.orderings.insert(child.clone(), 0);
    schema.constraints.insert(
        "scalar".to_owned(),
        vec![("max".to_owned(), "8".to_owned())],
    );
    schema.constraints.insert(
        "record".to_owned(),
        vec![("acyclic".to_owned(), "next".to_owned())],
    );
    schema.keys.insert(
        "child".to_owned(),
        vec![
            Key {
                id: "primary".to_owned(),
                parts: vec![KeyPart::Path(Vec::new())],
                role: KeyRole::Primary,
                scope: KeyScope::Document,
                equality: Vec::new(),
            },
            Key {
                id: "sort".to_owned(),
                parts: vec![KeyPart::Path(Vec::new())],
                role: KeyRole::SortKey,
                scope: KeyScope::Within("record".to_owned()),
                equality: Vec::new(),
            },
        ],
    );
    let semantics = BTreeMap::from([
        ("max".to_owned(), Semantics::MaxLength(true)),
        ("acyclic".to_owned(), Semantics::Acyclic),
    ]);
    ValidationInput::Graph {
        schema,
        semantics,
        instance: Graph {
            nodes: [
                (
                    0,
                    Node::new(0, "record").with_field("field", Value::Str("ok".to_owned())),
                ),
                (1, Node::new(1, "child").with_value(Value::Int(1))),
            ]
            .into(),
            edges: [(
                0,
                GEdge {
                    src: 0,
                    tgt: 1,
                    label: child,
                    position: Some(0),
                },
            )]
            .into(),
        },
    }
}

fn assert_tree_schedule_dependencies() {
    let arc = SEdge::new("v", "v", "child", Some("child"));
    let tree_input = ValidationInput::Tree {
        schema: Schema::default()
            .vertex("v", "object")
            .edge("v", "v", "child", Some("child")),
        semantics: BTreeMap::new(),
        instance: Tree {
            nodes: [
                (0, Node::new(0, "v")),
                (1, Node::new(1, "v")),
                (2, Node::new(2, "v")),
                (3, Node::new(3, "v")),
            ]
            .into(),
            arcs: vec![
                (0, 1, arc.clone()),
                (1, 2, arc.clone()),
                (0, 3, arc.clone()),
                (3, 2, arc.clone()),
            ],
            fans: Vec::new(),
            root: 0,
        },
    };
    let tree_schedule = validation::trusted_schedule(&tree_input, &ValidationTrust::default());
    assert_eq!(
        scheduled_run(
            &tree_schedule,
            CheckKind::TreeParentMap,
            Some(&ViewRef::Node(2))
        )
        .reads,
        vec![ViewRef::Node(2), ViewRef::Edge(1), ViewRef::Edge(3)]
    );
    assert_eq!(
        scheduled_run(
            &tree_schedule,
            CheckKind::TreeReachable,
            Some(&ViewRef::Node(2))
        )
        .reads,
        vec![
            ViewRef::Node(0),
            ViewRef::Node(1),
            ViewRef::Node(2),
            ViewRef::Node(3),
            ViewRef::Edge(0),
            ViewRef::Edge(1),
            ViewRef::Edge(2),
            ViewRef::Edge(3),
        ]
    );
    assert_eq!(
        scheduled_run(
            &tree_schedule,
            CheckKind::TreeReachable,
            Some(&ViewRef::Node(0))
        )
        .reads,
        vec![ViewRef::Node(0)]
    );
    assert!(tree_schedule.runs().iter().any(|run| {
        run.instance.check.kind == CheckKind::ReservedVocabulary
            && run.instance.check.decl == SchemaRef::Edge(arc.clone())
            && run.instance.subject == Some(ViewRef::Node(0))
            && run.diagnostics.is_empty()
    }));
}

#[test]
fn trusted_schedule_contains_every_passing_concrete_dependency() {
    let input = rich_schedule_input();
    let schedule = validation::trusted_schedule(&input, &ValidationTrust::default());
    assert!(schedule.runs().iter().any(|run| {
        run.instance.check.kind == CheckKind::ValueConstraint
            && run.instance.subject == Some(ViewRef::Node(0))
    }));
    assert!(schedule.runs().iter().any(|run| {
        run.instance.check.kind == CheckKind::RequiredValue
            && run.instance.subject == Some(ViewRef::Node(1))
    }));
    let own_primary = scheduled_run(&schedule, CheckKind::RequiredValue, Some(&ViewRef::Node(1)));
    assert_eq!(own_primary.class, CheckClass::GraphLocal);
    assert_eq!(own_primary.reads, vec![ViewRef::Node(1)]);
    assert!(schedule.runs().iter().any(|run| {
        run.instance.check.kind == CheckKind::KeyOrder
            && run.instance.subject == Some(ViewRef::Node(0))
            && run.requirement == Requirement::Advisory
    }));
    assert!(schedule.runs().iter().any(|run| {
        run.instance.check.kind == CheckKind::PositionDistinct
            && run.instance.subject == Some(ViewRef::Node(0))
    }));
    assert!(
        schedule
            .runs()
            .iter()
            .any(|run| run.instance.check.kind == CheckKind::Acyclic)
    );

    let ValidationInput::Graph {
        schema, semantics, ..
    } = input
    else {
        panic!("rich schedule fixture is a graph");
    };
    let table_schedule = validation::trusted_schedule(
        &ValidationInput::Table {
            schema,
            semantics,
            instance: Table {
                rows: [(0, ("record".to_owned(), BTreeMap::new()))].into(),
                ..Table::default()
            },
        },
        &ValidationTrust::default(),
    );
    assert!(!table_schedule.runs().iter().any(|run| {
        matches!(
            run.instance.check.kind,
            CheckKind::PositionDistinct | CheckKind::Acyclic
        )
    }));
    assert_tree_schedule_dependencies();
}

fn scheduled_run<'a>(
    schedule: &'a TrustedSchedule,
    kind: CheckKind,
    subject: Option<&ViewRef>,
) -> &'a CheckRun {
    schedule
        .runs()
        .iter()
        .find(|run| run.instance.check.kind == kind && run.instance.subject.as_ref() == subject)
        .unwrap()
}

#[test]
fn structural_schedule_has_exact_local_reads_and_plain_reserved_fan_candidates() {
    let relation = SEdge::new("v", "v", "rel", Some("r"));
    let reserved = SEdge::new("v", "v", "hyper_parent", Some("parent"));
    let input = ValidationInput::Graph {
        schema: Schema::default()
            .vertex("v", "object")
            .edge("v", "v", "rel", Some("r")),
        semantics: BTreeMap::new(),
        instance: Graph {
            nodes: [(0, Node::new(0, "v")), (1, Node::new(1, "v"))].into(),
            edges: [
                (
                    0,
                    GEdge {
                        src: 0,
                        tgt: 1,
                        label: reserved,
                        position: None,
                    },
                ),
                (
                    1,
                    GEdge {
                        src: 0,
                        tgt: 1,
                        label: relation,
                        position: None,
                    },
                ),
            ]
            .into(),
        },
    };
    let schedule = validation::trusted_schedule(&input, &ValidationTrust::default());
    assert_eq!(
        scheduled_run(&schedule, CheckKind::NodeAnchor, Some(&ViewRef::Node(0))).reads,
        vec![ViewRef::Node(0)]
    );
    assert_eq!(
        scheduled_run(&schedule, CheckKind::Attribute, Some(&ViewRef::Node(0))).reads,
        vec![ViewRef::Node(0)]
    );
    assert_eq!(
        scheduled_run(&schedule, CheckKind::EdgeDeclared, Some(&ViewRef::Edge(1))).reads,
        vec![ViewRef::Edge(1)]
    );
    assert_eq!(
        scheduled_run(
            &schedule,
            CheckKind::EndpointCoherence,
            Some(&ViewRef::Edge(1))
        )
        .reads,
        vec![ViewRef::Node(0), ViewRef::Node(1), ViewRef::Edge(1)]
    );
    let fan = scheduled_run(&schedule, CheckKind::FanWellFormed, Some(&ViewRef::Node(0)));
    assert_eq!(
        fan.reads,
        vec![ViewRef::Node(0), ViewRef::Edge(0), ViewRef::Edge(1)]
    );
    assert!(
        fan.diagnostics
            .iter()
            .any(|diagnostic| matches!(&diagnostic.violation, Violation::MalformedFan(_)))
    );
    assert_eq!(schedule.verify_instances(&schedule.instances()), Ok(()));

    let budgeted = validation::witness_with_trust(
        input,
        ValidationTrust {
            budget: ValidationBudget { max_reads: Some(1) },
            ..ValidationTrust::default()
        },
    )
    .unwrap();
    assert_eq!(budgeted.evaluation.spent, 1);
    assert_eq!(
        budgeted.evaluation.skipped.first().unwrap(),
        &CheckInstance {
            check: CheckKey {
                kind: CheckKind::NodeAnchor,
                decl: SchemaRef::Vertex("v".to_owned()),
            },
            subject: Some(ViewRef::Node(1)),
        }
    );
}

#[test]
fn target_set_named_collation_incompleteness_has_a_concrete_scheduled_site() {
    let mut schema = Schema::default().vertex("v", "string");
    schema.keys.insert(
        "v".to_owned(),
        vec![Key {
            id: "id".to_owned(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Identifier,
            scope: KeyScope::Document,
            equality: vec![KeyEquality::Sql {
                collation: SqlCollation::Named("locale".to_owned()),
                pad: SqlPad::NoPad,
            }],
        }],
    );
    schema.target_sets.insert(
        "ids".to_owned(),
        TargetSet {
            members: vec![("v".to_owned(), "id".to_owned())],
            scope: KeyScope::Document,
        },
    );
    let schedule = validation::trusted_schedule(
        &ValidationInput::Graph {
            schema,
            semantics: BTreeMap::new(),
            instance: Graph {
                nodes: [(4, Node::new(4, "v").with_value(Value::Str("x".to_owned())))].into(),
                ..Graph::default()
            },
        },
        &ValidationTrust::default(),
    );
    let run = schedule
        .runs()
        .iter()
        .find(|run| {
            run.instance.check
                == CheckKey {
                    kind: CheckKind::KeyUnique,
                    decl: SchemaRef::TargetSet("ids".to_owned()),
                }
                && run.instance.subject == Some(ViewRef::Node(4))
        })
        .unwrap();
    assert_eq!(run.reads, vec![ViewRef::Node(4)]);
    assert!(
        run.incomplete
            .iter()
            .any(|entry| matches!(entry.reason, IncompleteReason::Unsupported { .. }))
    );
    assert_eq!(schedule.verify_instances(&schedule.instances()), Ok(()));
}

fn guarded_error_input() -> ValidationInput {
    let source = SEdge::new("holder", "scalar", "prop", Some("source"));
    let guard = SEdge::new("holder", "scalar", "prop", Some("kind"));
    let link = SEdge::new("holder", "target", "ref", Some("target"));
    let mut schema = Schema::default()
        .vertex("holder", "object")
        .vertex("target", "object")
        .vertex("scalar", "string");
    schema
        .edges
        .extend([source.clone(), guard.clone(), link.clone()]);
    schema.keys.insert(
        "target".to_owned(),
        vec![Key {
            id: "id".to_owned(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Identifier,
            scope: KeyScope::Document,
            equality: Vec::new(),
        }],
    );
    schema.references.insert(
        "guarded".to_owned(),
        Reference {
            id: "guarded".to_owned(),
            holder: "holder".to_owned(),
            source: RefSource::KeyValue {
                parts: vec![vec![source]],
            },
            target_vertex: "target".to_owned(),
            target_key: Some("id".to_owned()),
            edge: Some(link),
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: Some(RefGuard {
                field: vec![guard],
                equals: Value::Str("selected".to_owned()),
            }),
            null_match: panproto_reference::schema::NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    ValidationInput::Graph {
        schema,
        semantics: BTreeMap::new(),
        instance: Graph {
            nodes: [(
                0,
                Node::new(0, "holder")
                    .with_field("source", Value::Str("id".to_owned()))
                    .with_field("kind", Value::Str("other".to_owned())),
            )]
            .into(),
            ..Graph::default()
        },
    }
}

#[test]
fn unmatched_error_guard_is_a_separate_advisory_run() {
    let schedule =
        validation::trusted_schedule(&guarded_error_input(), &ValidationTrust::default());
    let run = scheduled_run(
        &schedule,
        CheckKind::ReferenceResolves,
        Some(&ViewRef::Node(0)),
    );
    assert_eq!(run.requirement, Requirement::Advisory);
    assert_eq!(run.diagnostics.len(), 1);
    assert!(matches!(
        &run.diagnostics[0].violation,
        Violation::UnselectedReference(id, Value::Str(value))
            if id == "guarded" && value == "other"
    ));
    assert_eq!(schedule.verify_instances(&schedule.instances()), Ok(()));
}

#[test]
fn native_missing_fan_incidence_reads_are_compiled_before_evaluation() {
    let fan_case = panproto_reference::enumerate::validation::valid_tree_fan_case();
    let schedule = validation::trusted_schedule(
        &ValidationInput::Tree {
            schema: fan_case.schema,
            semantics: BTreeMap::new(),
            instance: Tree {
                nodes: [(0, Node::new(0, "parent"))].into(),
                arcs: Vec::new(),
                fans: vec![Fan {
                    parent: 9,
                    hyper_edge: "h".to_owned(),
                    children: BTreeMap::from([("child".to_owned(), 8)]),
                }],
                root: 0,
            },
        },
        &ValidationTrust::default(),
    );
    let run = scheduled_run(&schedule, CheckKind::FanWellFormed, Some(&ViewRef::Node(9)));
    assert_eq!(
        run.reads,
        vec![
            ViewRef::Node(1),
            ViewRef::Node(8),
            ViewRef::Node(9),
            ViewRef::Edge(0),
            ViewRef::Edge(1),
        ]
    );
    assert!(
        run.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.violation == Violation::MissingFanParent)
    );
    assert!(run.diagnostics.iter().any(|diagnostic| {
        diagnostic.violation == Violation::MissingFanChild("child".to_owned())
    }));
    assert_eq!(schedule.verify_instances(&schedule.instances()), Ok(()));
}

#[test]
fn native_refusals_are_required_incomplete_and_never_admitted() {
    let relation = SEdge::new("t", "t", "rel", Some("r"));
    let table_witness = validation::witness(ValidationInput::Table {
        schema: Schema::default().vertex("t", "object"),
        semantics: BTreeMap::new(),
        instance: Table {
            rows: [(0, ("t".to_owned(), BTreeMap::new()))].into(),
            pairs: [(relation, 0, 9)].into(),
        },
    });
    assert!(matches!(
        table_witness.outcome,
        ValidationOutcome::Refused(_)
    ));
    assert_eq!(
        table_witness.evaluation.report.status,
        CheckStatus::Incomplete
    );
    assert!(!table_witness.admitted);
    assert!(
        table_witness
            .evaluation
            .report
            .incomplete
            .iter()
            .any(|entry| {
                entry.requirement == Requirement::Required
                    && matches!(entry.reason, IncompleteReason::NativeShapeRefusal { .. })
            })
    );
    assert_eq!(table_witness.verify(), Ok(()));

    let tree_witness = validation::witness(ValidationInput::Tree {
        schema: Schema::default().vertex("v", "object"),
        semantics: BTreeMap::new(),
        instance: Tree {
            nodes: [(0, Node::new(0, "v"))].into(),
            arcs: vec![(0, 9, SEdge::new("v", "v", "child", Some("x")))],
            fans: Vec::new(),
            root: 0,
        },
    });
    assert!(matches!(
        tree_witness.outcome,
        ValidationOutcome::Refused(_)
    ));
    assert_eq!(
        tree_witness.evaluation.report.status,
        CheckStatus::Incomplete
    );
    assert!(!tree_witness.admitted);
    assert_eq!(tree_witness.verify(), Ok(()));
}

fn assert_incremental_trust_rejects_mutations(current: &ValidationInput, trust: &ValidationTrust) {
    let mut wrong_delta = trust.clone();
    wrong_delta.delta = vec![ViewRef::Node(9)];
    assert_eq!(
        validation::witness_with_trust(current.clone(), wrong_delta),
        Err(WitnessError::IncrementalMismatch)
    );
    let mut wrong_baseline = trust.clone();
    let Some(ValidationOutcome::Report(wrong_baseline_report)) = &mut wrong_baseline.baseline
    else {
        panic!("trusted baseline is a report");
    };
    wrong_baseline_report.status = CheckStatus::Failed;
    assert_eq!(
        validation::witness_with_trust(current.clone(), wrong_baseline),
        Err(WitnessError::IncrementalMismatch)
    );
    let mut missing_policy = trust.clone();
    missing_policy.prior_policy = None;
    assert_eq!(
        validation::witness_with_trust(current.clone(), missing_policy),
        Err(WitnessError::IncrementalMismatch)
    );
    let mut wrong_prior = trust.clone();
    let mut mismatched_outcome = validation::witness(current.clone()).outcome;
    let ValidationOutcome::Report(mismatched_report) = &mut mismatched_outcome else {
        panic!("graph validation returns a report");
    };
    mismatched_report.status = CheckStatus::Failed;
    wrong_prior.prior = Some(mismatched_outcome);
    assert_eq!(
        validation::witness_with_trust(current.clone(), wrong_prior),
        Err(WitnessError::IncrementalMismatch)
    );
}

fn assert_failed_prior_is_not_admitted() {
    let failed_prior = ValidationInput::Graph {
        schema: Schema::default().vertex("a", "object"),
        semantics: BTreeMap::new(),
        instance: Graph {
            nodes: [(0, Node::new(0, "ghost"))].into(),
            ..Graph::default()
        },
    };
    let failed_outcome = validation::witness(failed_prior.clone()).outcome;
    assert_eq!(
        validation::witness_with_trust(
            failed_prior.clone(),
            ValidationTrust {
                baseline_context: Some(ValidationContext::default()),
                baseline: Some(failed_outcome.clone()),
                prior: Some(failed_outcome),
                prior_input: Some(Box::new(failed_prior)),
                prior_policy: Some(PriorAdmissionPolicy::RequirePassed),
                require_passed: false,
                ..ValidationTrust::default()
            },
        ),
        Err(WitnessError::IncrementalMismatch)
    );
}

#[test]
fn trusted_witness_uses_budget_prior_and_exact_delta_semantically() {
    let mut declared_schema = Schema::default().vertex("text", "string");
    declared_schema
        .constraints
        .insert("text".to_owned(), vec![("max".to_owned(), "4".to_owned())]);
    let declared = ValidationInput::Graph {
        schema: declared_schema,
        semantics: BTreeMap::from([("max".to_owned(), Semantics::MaxLength(true))]),
        instance: Graph {
            nodes: [(
                0,
                Node::new(0, "text").with_value(Value::Str("ok".to_owned())),
            )]
            .into(),
            ..Graph::default()
        },
    };
    let schedule = validation::trusted_schedule(&declared, &ValidationTrust::default());
    assert!(schedule.runs().iter().any(|run| {
        run.instance.check.kind == CheckKind::DeclarationInterpretable
            && run.diagnostics.is_empty()
            && run.incomplete.is_empty()
    }));
    assert!(schedule.runs().iter().any(|run| {
        run.instance.check.kind == CheckKind::ValueConstraint
            && run.reads == vec![ViewRef::Node(0)]
            && run.diagnostics.is_empty()
            && run.incomplete.is_empty()
    }));

    let input = ValidationInput::Graph {
        schema: Schema::default().vertex("a", "object"),
        semantics: BTreeMap::new(),
        instance: Graph {
            nodes: [(0, Node::new(0, "a"))].into(),
            ..Graph::default()
        },
    };
    let budgeted = validation::witness_with_trust(
        input.clone(),
        ValidationTrust {
            budget: ValidationBudget { max_reads: Some(0) },
            ..ValidationTrust::default()
        },
    )
    .unwrap();
    assert_eq!(budgeted.evaluation.report.status, CheckStatus::Incomplete);
    assert!(!budgeted.admitted);
    assert_eq!(budgeted.verify(), Ok(()));

    let passing_outcome = validation::witness(input.clone()).outcome;
    assert!(matches!(
        validation::witness_with_trust(
            input.clone(),
            ValidationTrust {
                baseline_context: Some(ValidationContext::default()),
                baseline: Some(passing_outcome.clone()),
                baseline_budget: Some(budget_evidence(
                    ValidationBudget { max_reads: Some(0) },
                    &budgeted.evaluation,
                )),
                prior: Some(passing_outcome),
                prior_input: Some(Box::new(input.clone())),
                prior_policy: Some(PriorAdmissionPolicy::RequirePassed),
                require_passed: false,
                ..ValidationTrust::default()
            },
        ),
        Err(WitnessError::IncrementalMismatch)
    ));

    let prior = input;
    let current = ValidationInput::Graph {
        schema: Schema::default().vertex("a", "object"),
        semantics: BTreeMap::new(),
        instance: Graph {
            nodes: [(0, Node::new(0, "a").with_value(Value::Int(1)))].into(),
            ..Graph::default()
        },
    };
    let prior_outcome = validation::witness(prior.clone()).outcome;
    let trust = ValidationTrust {
        baseline_context: Some(ValidationContext::default()),
        baseline: Some(prior_outcome.clone()),
        prior: Some(prior_outcome),
        prior_input: Some(Box::new(prior)),
        prior_policy: Some(PriorAdmissionPolicy::RequirePassed),
        delta: vec![ViewRef::Node(0)],
        require_passed: false,
        ..ValidationTrust::default()
    };
    let incremental = validation::witness_with_trust(current.clone(), trust.clone()).unwrap();
    assert!(incremental.admitted);
    assert_eq!(incremental.verify(), Ok(()));
    assert_incremental_trust_rejects_mutations(&current, &trust);
    assert_failed_prior_is_not_admitted();
}

#[test]
fn no_new_admission_ignores_advisory_sort_findings() {
    let children = SEdge::new("parent", "child", "prop", Some("children"));
    let mut schema = Schema::default()
        .vertex("parent", "object")
        .vertex("child", "int");
    schema.edges.insert(children.clone());
    schema.orderings.insert(children.clone(), 0);
    schema.keys.insert(
        "child".to_owned(),
        vec![Key {
            id: "order".to_owned(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::SortKey,
            scope: KeyScope::Within("parent".to_owned()),
            equality: Vec::new(),
        }],
    );
    let graph = |left: i64, right: i64| Graph {
        nodes: [
            (0, Node::new(0, "parent")),
            (1, Node::new(1, "child").with_value(Value::Int(left))),
            (2, Node::new(2, "child").with_value(Value::Int(right))),
        ]
        .into(),
        edges: [
            (
                0,
                GEdge {
                    src: 0,
                    tgt: 1,
                    label: children.clone(),
                    position: Some(0),
                },
            ),
            (
                1,
                GEdge {
                    src: 0,
                    tgt: 2,
                    label: children.clone(),
                    position: Some(1),
                },
            ),
        ]
        .into(),
    };
    let prior = ValidationInput::Graph {
        schema: schema.clone(),
        semantics: BTreeMap::new(),
        instance: graph(1, 2),
    };
    let current = ValidationInput::Graph {
        schema,
        semantics: BTreeMap::new(),
        instance: graph(2, 1),
    };
    let baseline = validation::witness(prior.clone()).outcome;
    let witness = validation::witness_with_trust(
        current,
        ValidationTrust {
            baseline_context: Some(ValidationContext::default()),
            baseline: Some(baseline.clone()),
            prior: Some(baseline),
            prior_input: Some(Box::new(prior)),
            prior_policy: Some(PriorAdmissionPolicy::RequirePassed),
            delta: vec![ViewRef::Node(1), ViewRef::Node(2)],
            require_passed: false,
            ..ValidationTrust::default()
        },
    )
    .unwrap();
    assert!(
        witness
            .evaluation
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| {
                diagnostic.check.kind == CheckKind::KeyOrder
                    && diagnostic.requirement == Requirement::Advisory
            })
    );
    assert_eq!(witness.evaluation.report.status, CheckStatus::Passed);
    assert!(witness.admitted);
    assert_eq!(witness.verify(), Ok(()));
}

#[test]
fn sort_and_index_keys_do_not_create_uniqueness_failures() {
    for role in [KeyRole::SortKey, KeyRole::Index] {
        let mut schema = Schema::default().vertex("v", "object");
        schema.keys.insert(
            "v".to_owned(),
            vec![Key {
                id: "k".to_owned(),
                parts: vec![KeyPart::Path(Vec::new())],
                role,
                scope: KeyScope::Document,
                equality: Vec::new(),
            }],
        );
        let graph = Graph {
            nodes: [
                (0, Node::new(0, "v").with_value(Value::Int(1))),
                (1, Node::new(1, "v").with_value(Value::Int(1))),
            ]
            .into(),
            ..Graph::default()
        };
        let report = validation::validate(&schema, &BTreeMap::new(), &graph);
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.check.kind == CheckKind::KeyUnique)
        );
    }
}

#[test]
fn a_primary_position_part_is_required() {
    let children = SEdge::new("parent", "child", "prop", Some("children"));
    let mut schema = Schema::default()
        .vertex("parent", "object")
        .vertex("child", "object")
        .edge("parent", "child", "prop", Some("children"));
    schema.orderings.insert(children.clone(), 0);
    schema.keys.insert(
        "child".to_owned(),
        vec![Key {
            id: "position".to_owned(),
            parts: vec![KeyPart::Position { within: children }],
            role: KeyRole::Primary,
            scope: KeyScope::Document,
            equality: Vec::new(),
        }],
    );
    let graph = Graph {
        nodes: [(0, Node::new(0, "child"))].into(),
        ..Graph::default()
    };
    let report = validation::validate(&schema, &BTreeMap::new(), &graph);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.check.kind == CheckKind::RequiredValue
            && diagnostic.subject == Some(ViewRef::Node(0))
            && diagnostic.violation == Violation::MissingValue(Some("children".to_owned()))
    }));
}

fn count_graph_scope() -> u64 {
    let mut actual = 0;
    let mut schemas = BTreeSet::new();
    let mut current_schema = None;
    let mut instances = BTreeSet::new();
    panproto_reference::enumerate::validation::for_each_graph_case(|case| {
        let schema_bytes = panproto_reference::schema::canonical_bytes(&case.schema);
        if current_schema.as_ref() != Some(&schema_bytes) {
            assert!(schemas.insert(schema_bytes.clone()));
            current_schema = Some(schema_bytes);
            instances.clear();
        }
        assert!(
            instances.insert(panproto_reference::canonical::graph_bytes(&case.instance).unwrap())
        );
        let report = validation::validate(&case.schema, &BTreeMap::new(), &case.instance);
        assert_eq!(report.shape, InstanceShape::Graph);
        actual += 1;
    });
    actual
}

fn count_table_scope() -> u64 {
    let mut pairs = BTreeSet::new();
    for case in panproto_reference::enumerate::validation::table_cases() {
        let key = (
            panproto_reference::schema::canonical_bytes(&case.schema),
            panproto_reference::canonical::table_bytes(&case.instance).unwrap(),
        );
        assert!(pairs.insert(key));
        let report =
            validation::validate_table(&case.schema, &BTreeMap::new(), &case.instance).unwrap();
        assert_eq!(report.shape, InstanceShape::Table);
    }
    u64::try_from(pairs.len()).unwrap()
}

fn count_tree_scope() -> u64 {
    let mut actual = 0;
    let mut schemas = BTreeSet::new();
    let mut current_schema = None;
    let mut instances = BTreeSet::new();
    panproto_reference::enumerate::validation::for_each_tree_case(|case| {
        let schema_bytes = panproto_reference::schema::canonical_bytes(&case.schema);
        if current_schema.as_ref() != Some(&schema_bytes) {
            assert!(schemas.insert(schema_bytes.clone()));
            current_schema = Some(schema_bytes);
            instances.clear();
        }
        assert!(
            instances.insert(panproto_reference::canonical::tree_bytes(&case.instance).unwrap())
        );
        let report =
            validation::validate_tree(&case.schema, &BTreeMap::new(), &case.instance).unwrap();
        assert_eq!(report.shape, InstanceShape::Tree);
        actual += 1;
    });
    actual
}

fn closed_graph_count() -> u64 {
    let schema_strata = [
        (0u64, 0u64, 1u64),
        (1, 0, 1),
        (1, 1, 1),
        (2, 0, 1),
        (2, 1, 4),
        (2, 2, 6),
        (3, 0, 1),
        (3, 1, 9),
        (3, 2, 36),
    ];
    schema_strata
        .into_iter()
        .map(|(vertices, edges, schemas)| {
            let per_schema = (0..=3u32)
                .map(|nodes| {
                    (0..=(3 - nodes))
                        .map(|edge_count| {
                            (3 * vertices).pow(nodes)
                                * (u64::from(nodes).pow(2) * edges).pow(edge_count)
                        })
                        .sum::<u64>()
                })
                .sum::<u64>();
            schemas * per_schema
        })
        .sum()
}

fn closed_table_strata() -> (u64, u64) {
    let attributes = [1u64, 3, 9, 27].into_iter().sum();
    let relations = [1u64, 2, 16, 512].into_iter().sum();
    (attributes, relations)
}

fn closed_tree_count() -> u64 {
    [(0u64, 1u64), (1, 2), (2, 11), (3, 46)]
        .into_iter()
        .map(|(vertices, schemas)| {
            schemas * (3 * vertices + 18 * vertices.pow(2) + 648 * vertices.pow(3))
        })
        .sum()
}

#[test]
fn structural_schema_ordinals_and_bytes_are_unique() {
    use panproto_reference::enumerate::validation as scope;

    let schemas = scope::schemas();
    let mut bytes = BTreeSet::new();
    let mut histogram = BTreeMap::new();
    for (ordinal, schema) in schemas.iter().enumerate() {
        assert_eq!(
            scope::schema_ordinal(schema),
            Some(u64::try_from(ordinal).unwrap())
        );
        assert!(bytes.insert(panproto_reference::schema::canonical_bytes(schema)));
        *histogram
            .entry((schema.vertices.len(), schema.edges.len()))
            .or_insert(0u64) += 1;
    }
    assert_eq!(
        histogram,
        BTreeMap::from([
            ((0, 0), 1),
            ((1, 0), 1),
            ((1, 1), 1),
            ((2, 0), 1),
            ((2, 1), 4),
            ((2, 2), 6),
            ((3, 0), 1),
            ((3, 1), 9),
            ((3, 2), 36),
        ])
    );
    assert_eq!(
        panproto_reference::schema::canonical_bytes(&Schema::default()),
        vec![0; 72],
        "the empty-schema record has nine independently specified zero counts"
    );
    assert_eq!(
        panproto_reference::canonical::graph_bytes(&Graph::default()),
        Some(vec![0; 16])
    );
    assert_eq!(
        panproto_reference::canonical::table_bytes(&Table::default()),
        Some(vec![0; 16])
    );
    assert_eq!(
        panproto_reference::canonical::tree_bytes(&Tree {
            nodes: BTreeMap::new(),
            arcs: Vec::new(),
            fans: Vec::new(),
            root: 0,
        }),
        Some(vec![0; 24])
    );
}

#[test]
fn declaration_and_native_boundary_subcarriers_are_exact() {
    use panproto_reference::enumerate::validation::{
        self as scope, BoundaryExpectation, BoundaryInstance,
    };

    let declarations = scope::declaration_cases();
    assert_eq!(declarations.len(), 29);
    let identities: BTreeSet<_> = declarations.iter().map(|case| case.declaration).collect();
    assert_eq!(
        identities,
        BTreeSet::from([
            scope::SettledDeclaration::Annotation,
            scope::SettledDeclaration::MaxLengthChars,
            scope::SettledDeclaration::MaxLengthBytes,
            scope::SettledDeclaration::MinLengthChars,
            scope::SettledDeclaration::MinLengthBytes,
            scope::SettledDeclaration::MaxLengthGraphemes,
            scope::SettledDeclaration::MinLengthGraphemes,
            scope::SettledDeclaration::Minimum,
            scope::SettledDeclaration::Maximum,
            scope::SettledDeclaration::Enum,
            scope::SettledDeclaration::Const,
            scope::SettledDeclaration::Pattern,
            scope::SettledDeclaration::Format,
            scope::SettledDeclaration::External,
            scope::SettledDeclaration::ValueParameter,
            scope::SettledDeclaration::MinOutDegree,
            scope::SettledDeclaration::MaxOutDegree,
            scope::SettledDeclaration::RequiredValue,
            scope::SettledDeclaration::Acyclic,
            scope::SettledDeclaration::Unsupported,
            scope::SettledDeclaration::MalformedLength,
            scope::SettledDeclaration::RequiredEdge,
            scope::SettledDeclaration::Ordering,
            scope::SettledDeclaration::PrimaryPath,
            scope::SettledDeclaration::PrimaryPosition,
            scope::SettledDeclaration::Unique,
            scope::SettledDeclaration::Identifier,
            scope::SettledDeclaration::SortKey,
            scope::SettledDeclaration::Index,
        ])
    );
    for case in declarations {
        let _ = validation::validate(&case.schema, &case.semantics, &Graph::default());
    }

    let boundaries = scope::boundary_cases();
    assert_eq!(boundaries.len(), 12);
    for case in boundaries {
        match (case.expectation, case.instance) {
            (expectation, BoundaryInstance::Graph(graph)) => {
                let report = validation::validate(&case.schema, &BTreeMap::new(), &graph);
                assert!(boundary_graph_finding(expectation, &report));
            }
            (BoundaryExpectation::MalformedTable, BoundaryInstance::Table(table)) => {
                assert!(matches!(
                    validation::validate_table(&case.schema, &BTreeMap::new(), &table),
                    Err(ShapeRefusal::Table(_))
                ));
            }
            (expectation, BoundaryInstance::Tree(tree)) => {
                assert!(boundary_tree_outcome(expectation, &case.schema, &tree));
            }
            _ => panic!("boundary expectation and native shape disagree"),
        }
    }
}

#[test]
fn native_pair_and_fan_identities_are_preserved() {
    use panproto_reference::enumerate::validation as scope;

    let pair = scope::valid_table_pair_case();
    let relation = SEdge::new("a", "b", "rel", Some("r"));
    let report =
        validation::validate_table(&pair.schema, &BTreeMap::new(), &pair.instance).unwrap();
    assert_eq!(
        report
            .identity
            .get(&ViewRef::Pair(Box::new(relation.clone()), 0, 1)),
        Some(&NativeRef::TablePair(Box::new(relation), 0, 1))
    );

    let fan = scope::valid_tree_fan_case();
    let report = validation::validate_tree(&fan.schema, &BTreeMap::new(), &fan.instance).unwrap();
    assert_eq!(
        report.identity.get(&ViewRef::Node(2)),
        Some(&NativeRef::TreeFan(0))
    );
    assert_eq!(
        report.identity.get(&ViewRef::Edge(0)),
        Some(&NativeRef::TreeFanParent(0))
    );
    assert_eq!(
        report.identity.get(&ViewRef::Edge(1)),
        Some(&NativeRef::TreeFanChild(0, "child".to_owned()))
    );
}

#[test]
fn every_settled_constraint_facet_is_executable_or_typed_incomplete() {
    let cases = [
        (
            Semantics::MaxGraphemes,
            "1",
            Value::Str("e\u{301}".to_owned()),
            true,
        ),
        (Semantics::Enum, "a|b", Value::Str("a".to_owned()), true),
        (Semantics::Const, "a", Value::Str("b".to_owned()), false),
        (Semantics::Pattern, "^a$", Value::Str("a".to_owned()), true),
        (Semantics::Format, "uri", Value::Str("x:y".to_owned()), true),
    ];
    for (meaning, bound, value, passes) in cases {
        let mut schema = Schema::default().vertex("v", "scalar");
        schema
            .constraints
            .insert("v".to_owned(), vec![("c".to_owned(), bound.to_owned())]);
        let graph = Graph {
            nodes: [(0, Node::new(0, "v").with_value(value))].into(),
            ..Graph::default()
        };
        let report = validation::validate(
            &schema,
            &BTreeMap::from([("c".to_owned(), meaning)]),
            &graph,
        );
        assert_eq!(
            !report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.check.kind == CheckKind::ValueConstraint),
            passes
        );
    }

    let mut schema = Schema::default().vertex("v", "object");
    schema.constraints.insert(
        "v".to_owned(),
        vec![
            ("external".to_owned(), "audit".to_owned()),
            ("parameter".to_owned(), "12".to_owned()),
        ],
    );
    let graph = Graph {
        nodes: [(0, Node::new(0, "v"))].into(),
        ..Graph::default()
    };
    let meanings = BTreeMap::from([
        ("external".to_owned(), Semantics::External),
        ("parameter".to_owned(), Semantics::ValueParameter),
    ]);
    let missing = validation::validate(&schema, &meanings, &graph);
    assert!(missing.incomplete.iter().any(|entry| matches!(
        &entry.reason,
        IncompleteReason::MissingExternalWitness { name } if name == "external"
    )));
    let context = ValidationContext {
        external: BTreeMap::from([(("external".to_owned(), ViewRef::Node(0)), false)]),
        complete_segment: true,
    };
    let rejected = validation::validate_graph_with_context(&schema, &meanings, &graph, &context);
    assert!(rejected.diagnostics.iter().any(|diagnostic| {
        diagnostic.violation == Violation::ExternalRejected("external".to_owned())
    }));
    let passing_trust = ValidationTrust {
        context: ValidationContext {
            external: BTreeMap::from([(("external".to_owned(), ViewRef::Node(0)), true)]),
            complete_segment: true,
        },
        ..ValidationTrust::default()
    };
    let schedule = validation::trusted_schedule(
        &ValidationInput::Graph {
            schema,
            semantics: meanings,
            instance: graph,
        },
        &passing_trust,
    );
    assert!(schedule.runs().iter().any(|run| {
        run.instance.check.kind == CheckKind::External
            && run.instance.subject == Some(ViewRef::Node(0))
            && run.diagnostics.is_empty()
            && run.incomplete.is_empty()
    }));
}

#[test]
fn attributes_check_declared_value_kinds_without_guessing_extension_kinds() {
    let schema = Schema::default()
        .vertex("record", "object")
        .vertex("count", "int")
        .edge("record", "count", "prop", Some("count"));
    let wrong = Graph {
        nodes: [(
            0,
            Node::new(0, "record").with_field("count", Value::Str("three".to_owned())),
        )]
        .into(),
        ..Graph::default()
    };
    let report = validation::validate(&schema, &BTreeMap::new(), &wrong);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.violation
            == Violation::MistypedAttribute(
                "count".to_owned(),
                "int".to_owned(),
                "string".to_owned(),
            )
    }));

    let shared = Schema::default()
        .vertex("record", "object")
        .vertex("text", "string")
        .vertex("link", "cid")
        .vertex("body", "blob")
        .vertex("when", "date")
        .vertex("clock", "time")
        .vertex("stamp", "date-time")
        .edge("record", "text", "prop", Some("text"))
        .edge("record", "link", "prop", Some("link"))
        .edge("record", "body", "prop", Some("body"))
        .edge("record", "when", "prop", Some("when"))
        .edge("record", "clock", "prop", Some("clock"))
        .edge("record", "stamp", "prop", Some("stamp"));
    let shared_graph = Graph {
        nodes: [(
            0,
            Node::new(0, "record")
                .with_field("text", Value::CidLink("bafy".to_owned()))
                .with_field("link", Value::Str("bafy".to_owned()))
                .with_field("body", Value::Bytes(vec![1, 2]))
                .with_field("when", Value::Int(0))
                .with_field("clock", Value::Str("12:30:45Z".to_owned()))
                .with_field("stamp", Value::Token("epoch".to_owned())),
        )]
        .into(),
        ..Graph::default()
    };
    let shared_report = validation::validate(&shared, &BTreeMap::new(), &shared_graph);
    assert!(
        !shared_report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.check.kind == CheckKind::Attribute)
    );
    assert!(
        !shared_report
            .incomplete
            .iter()
            .any(|entry| entry.check.kind == CheckKind::Attribute)
    );

    let extension = Schema::default()
        .vertex("record", "object")
        .vertex("payload", "vendor-value")
        .edge("record", "payload", "prop", Some("payload"));
    let graph = Graph {
        nodes: [(
            0,
            Node::new(0, "record").with_field("payload", Value::Str("x".to_owned())),
        )]
        .into(),
        ..Graph::default()
    };
    let report = validation::validate(&extension, &BTreeMap::new(), &graph);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.check.kind == CheckKind::Attribute)
    );
    assert!(report.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::Attribute
            && matches!(entry.reason, IncompleteReason::Unsupported { .. })
    }));
}

fn one_value_constraint(semantics: Semantics, bound: &str, value: Value) -> validation::Report {
    let mut schema = Schema::default().vertex("v", "scalar");
    schema.constraints.insert(
        "v".to_owned(),
        vec![("constraint".to_owned(), bound.to_owned())],
    );
    validation::validate(
        &schema,
        &BTreeMap::from([("constraint".to_owned(), semantics)]),
        &Graph {
            nodes: [(0, Node::new(0, "v").with_value(value))].into(),
            ..Graph::default()
        },
    )
}

#[test]
fn scalar_grapheme_and_format_subsets_refuse_unsupported_values_explicitly() {
    for semantics in [Semantics::Enum, Semantics::Const] {
        let report = one_value_constraint(semantics, "1", Value::Float(1.0f64.to_bits()));
        assert!(report.diagnostics.is_empty());
        assert!(report.incomplete.iter().any(|entry| {
            entry.check.kind == CheckKind::ValueConstraint
                && matches!(entry.reason, IncompleteReason::Unsupported { .. })
        }));
    }
    for text in ["a\u{1ab0}", "\u{1100}\u{1161}", "x\u{1f3fb}"] {
        let report =
            one_value_constraint(Semantics::MaxGraphemes, "1", Value::Str(text.to_owned()));
        assert!(report.incomplete.iter().any(|entry| {
            entry.check.kind == CheckKind::ValueConstraint
                && matches!(entry.reason, IncompleteReason::Unsupported { .. })
        }));
    }

    let invalid_date = one_value_constraint(
        Semantics::Format,
        "date",
        Value::Str("2023-02-31".to_owned()),
    );
    assert!(
        invalid_date
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.check.kind == CheckKind::ValueConstraint })
    );
    let valid_time = one_value_constraint(
        Semantics::Format,
        "time",
        Value::Str("12:30:45+04:00".to_owned()),
    );
    assert!(valid_time.diagnostics.is_empty());
    assert!(valid_time.incomplete.is_empty());
    let unsupported_time =
        one_value_constraint(Semantics::Format, "time", Value::Str("12:30:45".to_owned()));
    assert!(unsupported_time.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::ValueConstraint
            && matches!(entry.reason, IncompleteReason::Unsupported { .. })
    }));
}

#[test]
fn fan_diagnostics_name_edges_and_reserved_edges_do_not_hide_shared_checks() {
    use panproto_reference::model::NodeShape;
    use panproto_reference::restrict::HYPER_PARENT;

    let mut fan = Node::new(0, "h");
    fan.shape = NodeShape::HyperEdge;
    let label = SEdge::new("h", "v", HYPER_PARENT, Some("parent"));
    let graph = Graph {
        nodes: [(0, fan), (1, Node::new(1, "v")), (2, Node::new(2, "ghost"))].into(),
        edges: [
            (
                0,
                GEdge {
                    src: 0,
                    tgt: 1,
                    label: label.clone(),
                    position: None,
                },
            ),
            (
                1,
                GEdge {
                    src: 0,
                    tgt: 2,
                    label,
                    position: None,
                },
            ),
        ]
        .into(),
    };
    let report = validation::validate(
        &Schema::default().vertex("v", "object"),
        &BTreeMap::new(),
        &graph,
    );
    let multiple = report.diagnostics.iter().find(|diagnostic| {
        matches!(
            diagnostic.violation,
            Violation::MalformedFan(panproto_reference::fan::FanViolation::MultipleParents { .. })
        )
    });
    assert_eq!(
        multiple.map(|diagnostic| diagnostic.elements.clone()),
        Some(vec![ViewRef::Node(0), ViewRef::Edge(0), ViewRef::Edge(1)])
    );
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.check.kind == CheckKind::NodeAnchor
            && diagnostic.subject == Some(ViewRef::Node(2))
    }));
}

#[test]
fn native_reserved_tree_nodes_still_run_shared_checks() {
    use panproto_reference::model::NodeShape;

    let mut reserved = Node::new(0, "ghost").with_field("unknown", Value::Int(1));
    reserved.shape = NodeShape::HyperEdge;
    let tree = Tree {
        nodes: [(0, reserved), (1, Node::new(1, "v"))].into(),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    let schema = Schema::default().vertex("v", "object");
    let report = validation::validate_tree(&schema, &BTreeMap::new(), &tree).unwrap();
    let on_reserved: BTreeSet<_> = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.subject == Some(ViewRef::Node(0)))
        .map(|diagnostic| diagnostic.check.kind)
        .collect();
    assert!(on_reserved.contains(&CheckKind::ReservedVocabulary));
    assert!(on_reserved.contains(&CheckKind::NodeAnchor));
    assert!(!on_reserved.contains(&CheckKind::Attribute));
}

#[test]
fn unknown_anchors_stop_before_attribute_value_degree_and_external_checks() {
    let mut schema = Schema::default().vertex("known", "object");
    schema.constraints.insert(
        "ghost".to_owned(),
        vec![
            ("degree".to_owned(), "0".to_owned()),
            ("required".to_owned(), "true".to_owned()),
            ("external".to_owned(), "audit".to_owned()),
        ],
    );
    let semantics = BTreeMap::from([
        ("degree".to_owned(), Semantics::MaxOutDegree),
        ("required".to_owned(), Semantics::RequiredValue),
        ("external".to_owned(), Semantics::External),
    ]);
    let input = ValidationInput::Graph {
        schema,
        semantics,
        instance: Graph {
            nodes: [(
                7,
                Node::new(7, "ghost").with_field("undeclared", Value::Int(1)),
            )]
            .into(),
            ..Graph::default()
        },
    };
    let ValidationInput::Graph {
        schema,
        semantics,
        instance,
    } = &input
    else {
        unreachable!()
    };
    let report = validation::validate(schema, semantics, instance);
    let subject_kinds: BTreeSet<_> = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.subject == Some(ViewRef::Node(7)))
        .map(|diagnostic| diagnostic.check.kind)
        .collect();
    assert_eq!(subject_kinds, BTreeSet::from([CheckKind::NodeAnchor]));
    assert!(!report.incomplete.iter().any(|entry| {
        entry.subject == Some(ViewRef::Node(7))
            && matches!(
                entry.check.kind,
                CheckKind::Attribute
                    | CheckKind::OutDegree
                    | CheckKind::RequiredValue
                    | CheckKind::ValueConstraint
                    | CheckKind::External
            )
    }));
    let schedule = validation::trusted_schedule(&input, &ValidationTrust::default());
    assert!(schedule.runs().iter().any(|run| {
        run.instance.check.kind == CheckKind::NodeAnchor
            && run.instance.subject == Some(ViewRef::Node(7))
    }));
    assert!(!schedule.runs().iter().any(|run| {
        run.instance.subject == Some(ViewRef::Node(7))
            && matches!(
                run.instance.check.kind,
                CheckKind::Attribute
                    | CheckKind::OutDegree
                    | CheckKind::RequiredValue
                    | CheckKind::ValueConstraint
                    | CheckKind::External
            )
    }));
}

fn two_site_external_input() -> ValidationInput {
    let value = SEdge::new("record", "scalar", "prop", Some("value"));
    let alternate = SEdge::new("record", "scalar", "prop", Some("alternate"));
    let outgoing = SEdge::new("record", "other", "rel", Some("outgoing"));
    let incoming = SEdge::new("other", "record", "rel", Some("incoming"));
    let mut schema = Schema::default()
        .vertex("record", "object")
        .vertex("scalar", "string")
        .vertex("other", "object");
    schema
        .edges
        .extend([value, alternate, outgoing.clone(), incoming.clone()]);
    schema.constraints.insert(
        "scalar".to_owned(),
        vec![("audit_sort".to_owned(), "irrelevant-bound".to_owned())],
    );
    ValidationInput::Graph {
        schema,
        semantics: BTreeMap::from([("audit_sort".to_owned(), Semantics::External)]),
        instance: Graph {
            nodes: [
                (
                    0,
                    Node::new(0, "record")
                        .with_field("value", Value::Str("x".to_owned()))
                        .with_field("alternate", Value::Str("y".to_owned())),
                ),
                (1, Node::new(1, "other")),
                (2, Node::new(2, "other")),
            ]
            .into(),
            edges: [
                (
                    5,
                    GEdge {
                        src: 0,
                        tgt: 1,
                        label: outgoing,
                        position: None,
                    },
                ),
                (
                    6,
                    GEdge {
                        src: 2,
                        tgt: 0,
                        label: incoming,
                        position: None,
                    },
                ),
            ]
            .into(),
        },
    }
}

fn external_report(input: &ValidationInput, context: &ValidationContext) -> validation::Report {
    let ValidationInput::Graph {
        schema,
        semantics,
        instance,
    } = input
    else {
        unreachable!("fixture is a graph")
    };
    validation::validate_graph_with_context(schema, semantics, instance, context)
}

fn assert_external_sort_identity(input: &ValidationInput, subject: &ViewRef) {
    let missing = external_report(input, &ValidationContext::default());
    assert_eq!(
        missing
            .incomplete
            .iter()
            .filter(|entry| entry.check.kind == CheckKind::External)
            .count(),
        1,
    );

    let wrong_identity = ValidationContext {
        external: BTreeMap::from([(("irrelevant-bound".to_owned(), subject.clone()), true)]),
        complete_segment: true,
    };
    let still_missing = external_report(input, &wrong_identity);
    assert_eq!(
        still_missing
            .incomplete
            .iter()
            .filter(|entry| entry.check.kind == CheckKind::External)
            .count(),
        1,
    );
    assert!(still_missing.diagnostics.is_empty());

    let passing_context = ValidationContext {
        external: BTreeMap::from([(("audit_sort".to_owned(), subject.clone()), true)]),
        complete_segment: true,
    };
    let passed = external_report(input, &passing_context);
    assert!(
        !passed
            .incomplete
            .iter()
            .any(|entry| entry.check.kind == CheckKind::External)
    );
    assert!(
        !passed
            .diagnostics
            .iter()
            .any(|entry| entry.check.kind == CheckKind::External)
    );
}

fn assert_external_rejection_is_exact(input: &ValidationInput, subject: &ViewRef) {
    let rejecting_context = ValidationContext {
        external: BTreeMap::from([(("audit_sort".to_owned(), subject.clone()), false)]),
        complete_segment: true,
    };
    let rejected = external_report(input, &rejecting_context);
    assert_eq!(
        rejected
            .diagnostics
            .iter()
            .filter(|entry| entry.check.kind == CheckKind::External)
            .count(),
        1,
    );
    assert_eq!(
        rejected
            .diagnostics
            .iter()
            .find(|entry| entry.check.kind == CheckKind::External)
            .map(|entry| &entry.violation),
        Some(&Violation::ExternalRejected("audit_sort".to_owned())),
    );
    let rejected_schedule = validation::trusted_schedule(
        input,
        &ValidationTrust {
            context: rejecting_context,
            ..ValidationTrust::default()
        },
    );
    let rejected_run = scheduled_run(&rejected_schedule, CheckKind::External, Some(subject));
    assert_eq!(rejected_run.diagnostics.len(), 1);
    assert!(rejected_run.incomplete.is_empty());
}

#[test]
fn external_constraints_cover_property_backed_sites_and_read_incident_edges() {
    let input = two_site_external_input();
    let schedule = validation::trusted_schedule(&input, &ValidationTrust::default());
    let subject = ViewRef::Node(0);
    let run = scheduled_run(&schedule, CheckKind::External, Some(&subject));
    assert_eq!(
        run.reads,
        vec![ViewRef::Node(0), ViewRef::Edge(5), ViewRef::Edge(6)]
    );
    assert_eq!(run.class, CheckClass::BoundedExternal);
    assert_eq!(run.incomplete.len(), 1);
    assert!(matches!(
        &run.incomplete[0].reason,
        IncompleteReason::MissingExternalWitness { name } if name == "audit_sort"
    ));
    assert_external_sort_identity(&input, &subject);
    assert_external_rejection_is_exact(&input, &subject);
}

#[test]
fn incremental_external_projection_change_replays_distinct_baseline_context() {
    let input = two_site_external_input();
    let subject = ViewRef::Node(0);
    let baseline_context = ValidationContext {
        external: BTreeMap::from([(("audit_sort".to_owned(), subject.clone()), false)]),
        complete_segment: true,
    };
    let target_context = ValidationContext {
        external: BTreeMap::from([(("audit_sort".to_owned(), subject), true)]),
        complete_segment: true,
    };
    let baseline = validation::witness_with_trust(
        input.clone(),
        ValidationTrust {
            context: baseline_context.clone(),
            ..ValidationTrust::default()
        },
    )
    .unwrap()
    .outcome;
    let witness = validation::witness_with_trust(
        input.clone(),
        ValidationTrust {
            context: target_context,
            baseline_context: Some(baseline_context),
            baseline: Some(baseline.clone()),
            prior: Some(baseline),
            prior_input: Some(Box::new(input)),
            prior_policy: Some(PriorAdmissionPolicy::NoNewFailures),
            require_passed: false,
            ..ValidationTrust::default()
        },
    )
    .unwrap();
    assert!(witness.admitted);
    assert!(
        !witness
            .evaluation
            .report
            .diagnostics
            .iter()
            .any(|entry| entry.check.kind == CheckKind::External)
    );
    assert!(
        !witness
            .evaluation
            .report
            .incomplete
            .iter()
            .any(|entry| entry.check.kind == CheckKind::External)
    );
    assert_eq!(witness.verify(), Ok(()));
}

#[test]
fn primary_key_required_traces_follow_paths_and_position_carriers() {
    let children = SEdge::new("parent", "child", "prop", Some("children"));
    let code = SEdge::new("child", "scalar", "prop", Some("code"));
    let mut schema = Schema::default()
        .vertex("parent", "object")
        .vertex("child", "object")
        .vertex("scalar", "string");
    schema.edges.extend([children.clone(), code.clone()]);
    schema.orderings.insert(children.clone(), 0);
    schema.keys.insert(
        "child".to_owned(),
        vec![Key {
            id: "primary".to_owned(),
            parts: vec![
                KeyPart::Path(vec![code.clone()]),
                KeyPart::Position {
                    within: children.clone(),
                },
            ],
            role: KeyRole::Primary,
            scope: KeyScope::Document,
            equality: Vec::new(),
        }],
    );
    let input = ValidationInput::Graph {
        schema,
        semantics: BTreeMap::new(),
        instance: Graph {
            nodes: [
                (0, Node::new(0, "parent")),
                (1, Node::new(1, "child")),
                (2, Node::new(2, "child")),
                (
                    3,
                    Node::new(3, "scalar").with_value(Value::Str("x".to_owned())),
                ),
            ]
            .into(),
            edges: [
                (
                    10,
                    GEdge {
                        src: 0,
                        tgt: 1,
                        label: children.clone(),
                        position: Some(0),
                    },
                ),
                (
                    11,
                    GEdge {
                        src: 0,
                        tgt: 2,
                        label: children,
                        position: Some(1),
                    },
                ),
                (
                    12,
                    GEdge {
                        src: 1,
                        tgt: 3,
                        label: code,
                        position: None,
                    },
                ),
            ]
            .into(),
        },
    };
    let schedule = validation::trusted_schedule(&input, &ValidationTrust::default());
    let run = scheduled_run(&schedule, CheckKind::RequiredValue, Some(&ViewRef::Node(1)));
    assert_eq!(run.class, CheckClass::MaintainedGlobal);
    assert_eq!(
        run.reads,
        vec![
            ViewRef::Node(0),
            ViewRef::Node(1),
            ViewRef::Node(2),
            ViewRef::Node(3),
            ViewRef::Edge(10),
            ViewRef::Edge(11),
            ViewRef::Edge(12),
        ]
    );
}

#[test]
fn malformed_structural_declarations_are_static_without_instance_subjects() {
    let missing_required = SEdge::new("a", "missing", "prop", Some("required"));
    let missing_order = SEdge::new("a", "a", "prop", Some("ordered"));
    let mut schema = Schema::default().vertex("a", "object");
    schema
        .required
        .insert("a".to_owned(), vec![missing_required.clone()]);
    schema.orderings.insert(missing_order.clone(), 0);
    schema.hyper_edges.insert(
        "h".to_owned(),
        HyperEdge {
            kind: "fan".to_owned(),
            signature: BTreeMap::from([("child".to_owned(), "ghost".to_owned())]),
            parent_label: "parent".to_owned(),
        },
    );
    let input = ValidationInput::Graph {
        schema,
        semantics: BTreeMap::new(),
        instance: Graph::default(),
    };
    let schedule = validation::trusted_schedule(&input, &ValidationTrust::default());
    let declaration_refs: BTreeSet<_> = schedule
        .runs()
        .iter()
        .filter(|run| run.instance.check.kind == CheckKind::DeclarationInterpretable)
        .flat_map(|run| {
            assert_eq!(run.class, CheckClass::Static);
            assert_eq!(run.instance.subject, None);
            assert!(run.incomplete.iter().any(|entry| matches!(
                entry.reason,
                IncompleteReason::IllFormedDeclaration { .. }
            )));
            [run.instance.check.decl.clone()]
        })
        .collect();
    assert_eq!(
        declaration_refs,
        BTreeSet::from([
            SchemaRef::Required("a".to_owned(), missing_required.clone()),
            SchemaRef::Ordering(missing_order.clone()),
            SchemaRef::Vertex("h".to_owned()),
        ])
    );
    for (kind, declaration) in [
        (
            CheckKind::RequiredEdge,
            SchemaRef::Required("a".to_owned(), missing_required),
        ),
        (
            CheckKind::PositionDistinct,
            SchemaRef::Ordering(missing_order),
        ),
        (CheckKind::FanWellFormed, SchemaRef::Vertex("h".to_owned())),
    ] {
        let run = schedule
            .runs()
            .iter()
            .find(|run| {
                run.instance.check.kind == kind
                    && run.instance.check.decl == declaration
                    && run.instance.subject.is_none()
            })
            .unwrap();
        assert_eq!(run.class, CheckClass::GraphLocal);
        assert!(
            run.incomplete
                .iter()
                .any(|entry| matches!(entry.reason, IncompleteReason::IllFormedDeclaration { .. }))
        );
    }
}

#[test]
fn incomplete_tree_segments_defer_reachability() {
    let tree = Tree {
        nodes: [(0, Node::new(0, "v")), (1, Node::new(1, "v"))].into(),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    let schema = Schema::default().vertex("v", "object");
    let partial = validation::validate_tree_with_context(
        &schema,
        &BTreeMap::new(),
        &tree,
        &ValidationContext {
            external: BTreeMap::new(),
            complete_segment: false,
        },
    )
    .unwrap();
    assert!(
        !partial
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.check.kind == CheckKind::TreeReachable)
    );
    assert!(partial.incomplete.iter().any(|entry| {
        entry.check.kind == CheckKind::TreeReachable
            && entry.subject == Some(ViewRef::Node(1))
            && matches!(entry.reason, IncompleteReason::OutsideSegment { .. })
    }));
}

fn boundary_graph_finding(
    expectation: panproto_reference::enumerate::validation::BoundaryExpectation,
    report: &validation::Report,
) -> bool {
    use panproto_reference::enumerate::validation::BoundaryExpectation;

    report.diagnostics.iter().any(|diagnostic| {
        matches!(
            (expectation, &diagnostic.violation),
            (
                BoundaryExpectation::UnknownAnchor,
                Violation::UnknownAnchor(_)
            ) | (
                BoundaryExpectation::UndeclaredEdge,
                Violation::UndeclaredEdge(_)
            ) | (
                BoundaryExpectation::DanglingEndpoint,
                Violation::DanglingEndpoint(_, _)
            ) | (
                BoundaryExpectation::DuplicatePosition,
                Violation::DuplicatePosition(_, _)
            ) | (
                BoundaryExpectation::MalformedFan,
                Violation::MalformedFan(_)
            )
        )
    })
}

fn boundary_tree_outcome(
    expectation: panproto_reference::enumerate::validation::BoundaryExpectation,
    schema: &Schema,
    tree: &Tree,
) -> bool {
    use panproto_reference::enumerate::validation::BoundaryExpectation;

    match validation::validate_tree(schema, &BTreeMap::new(), tree) {
        Err(ShapeRefusal::Tree(error)) => matches!(
            (expectation, error),
            (
                BoundaryExpectation::DanglingArc,
                panproto_reference::pages::representation::encode::TreeEncodeError::DanglingArc
            )
        ),
        Ok(report) => report.diagnostics.iter().any(|diagnostic| {
            matches!(
                (expectation, &diagnostic.violation),
                (
                    BoundaryExpectation::SecondParent,
                    Violation::ParentMismatch(_, _)
                ) | (
                    BoundaryExpectation::ReservedVocabulary,
                    Violation::ReservedVocabulary
                ) | (BoundaryExpectation::MissingRoot, Violation::MissingRoot)
                    | (
                        BoundaryExpectation::MissingFanParent,
                        Violation::MissingFanParent
                    )
                    | (
                        BoundaryExpectation::MissingFanChild,
                        Violation::MissingFanChild(_)
                    )
            )
        }),
        Err(ShapeRefusal::Table(_)) => false,
    }
}

#[test]
fn settled_validation_scope_has_independently_pinned_counts() {
    use panproto_reference::enumerate::validation as scope;

    assert_eq!(scope::schemas().len(), 60);
    assert_eq!(scope::schemas_count(), 60);
    assert_eq!(closed_graph_count(), 71_610);
    assert_eq!(closed_table_strata(), (40, 531));
    assert_eq!(closed_tree_count(), 871_902);
    assert_eq!(scope::graph_cases_count(), closed_graph_count());
    assert_eq!(scope::table_cases_count(), 40 + 531);
    assert_eq!(scope::tree_cases_count(), closed_tree_count());
    assert_eq!(count_graph_scope(), 71_610);
    assert_eq!(count_table_scope(), 571);
    assert_eq!(count_tree_scope(), 871_902);

    let certificate = scope::scope_certificate();
    assert_eq!(certificate.structural_schemas, 60);
    assert_eq!(certificate.graph_cases, 71_610);
    assert_eq!(certificate.table_cases, 571);
    assert_eq!(certificate.tree_cases, 871_902);
    assert_eq!(certificate.declaration_cases, 29);
    assert_eq!(certificate.boundary_cases, 12);
    assert_eq!(certificate.declaration_families.len(), 19);
    assert_eq!(
        certificate.missing,
        vec![
            scope::MissingFiniteParameter::VertexKinds,
            scope::MissingFiniteParameter::EdgeVocabulary,
            scope::MissingFiniteParameter::NodePresenceStates,
            scope::MissingFiniteParameter::NodeValueAlphabet,
            scope::MissingFiniteParameter::NodeDiscriminatorAlphabet,
            scope::MissingFiniteParameter::FieldNameAlphabet,
            scope::MissingFiniteParameter::FieldValueAlphabet,
            scope::MissingFiniteParameter::NodePositionAlphabet,
            scope::MissingFiniteParameter::AnnotationAlphabet,
            scope::MissingFiniteParameter::MapBounds,
            scope::MissingFiniteParameter::NameBounds,
            scope::MissingFiniteParameter::DeclarationArguments,
            scope::MissingFiniteParameter::ExternalEvidence,
            scope::MissingFiniteParameter::RegisteredConstraints,
            scope::MissingFiniteParameter::NodeShapes,
            scope::MissingFiniteParameter::EdgePositions,
            scope::MissingFiniteParameter::FanSignatures,
            scope::MissingFiniteParameter::TableSchemaAlphabet,
            scope::MissingFiniteParameter::TableCellAlphabet,
            scope::MissingFiniteParameter::TableRelationAlphabet,
            scope::MissingFiniteParameter::MalformedCarrierBounds,
            scope::MissingFiniteParameter::RootedTreeBounds,
            scope::MissingFiniteParameter::FanListBounds,
            scope::MissingFiniteParameter::AdmissionBundleBytes,
            scope::MissingFiniteParameter::NativeIds,
        ]
    );
    println!(
        "validation narrow carrier: vertices=v0..v2 kinds=[object] edges=[prop(e)] node-values=[absent,Int(0),Int(1)] graph-total<=3 table-rows<=3 table-strata=[attribute:40,relation:531] tree-nodes=1..3 arc-names=[e] fans=0 schemas=60 graphs=71610 tables=571 trees=871902 declaration-exemplars=29 boundary-exemplars=12 missing-finite-parameters={:?}",
        certificate.missing
    );
}
