//! The laws of validation, checked over generated schemas and instances of
//! every shape.
//!
//! - Failure witnesses: every diagnostic verifies against the instance it
//!   refutes, and a diagnostic altered in its payload, subject or elements
//!   does not.
//! - Status soundness: a report is `Passed` exactly when it has no
//!   required diagnostic and no required incomplete check.
//! - Determinism: reports and their failure witnesses have the same
//!   canonical bytes in every process.
//! - Budget: a finite budget never passes what an unlimited one does not,
//!   a budget at least the unlimited run's reads changes nothing, and the
//!   instances a budget leaves undecided are a suffix in check-instance
//!   order.
//! - Shape squares: a tree validates as its graph form does, element for
//!   element through its embedding, and a table instance does too, every
//!   shared check compared, constraints on column and table vertices
//!   among them.
//! - Keys and references: `KeyUnique` and `ReferenceResolves` agree with
//!   a brute-force oracle.
//!
//! The length units of the protocols are pinned by unit tests: atproto's
//! `maxLength` counts UTF-8 bytes and its `maxGraphemes` extended grapheme
//! clusters, and JSON Schema's `maxLength` counts code points.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod validation_support;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::{CheckStatus, Name};
use panproto_inst::validation::{
    CheckInstance, CheckKey, CheckKind, CheckedInstance, Diagnostic, ExternalCheckDecl,
    ExternalVerdict, ExternalVerdicts, IncompleteReason, Requirement, SchemaRef, ValidationBudget,
    ValidationOptions, ValidationPlan, ValidationReport, Violation,
};
use panproto_inst::{
    Digest, EdgeId, FInstance, FanRecord, FanViolation, GInstance, RowId, Value, ViewRef, WInstance,
};
use panproto_schema::{Edge, InstanceShapeKind};
use proptest::prelude::*;

use validation_support::{
    Builder, Case, FanSpec, Features, Flag, RecordSpec, TreeSpec, ValueSpec, arb_graph_spec,
    arb_table_spec, arb_tree_spec, arb_validation_schema, e_cast, e_items, e_tag, graph_of,
    protocol, schema_of, table_of, tree_of,
};

fn assert_reference_validation_repeatable() {
    use panproto_reference::model::{Graph, Node as ReferenceNode};
    use panproto_reference::schema::Schema as ReferenceSchema;

    let schema = ReferenceSchema::default().vertex("item", "object");
    let graph = Graph {
        nodes: [(0, ReferenceNode::new(0, "item"))].into(),
        ..Graph::default()
    };
    let first = panproto_reference::validation::validate(&schema, &BTreeMap::new(), &graph);
    let second = panproto_reference::validation::validate(&schema, &BTreeMap::new(), &graph);
    assert_eq!(first, second);
}

/// A compact single-element report presented as the graph report: the row
/// subject becomes node 7 and the identity map the graph node's.
fn normalize(
    mut report: panproto_reference::validation::Report,
) -> panproto_reference::validation::Report {
    use panproto_reference::lens::ViewRef as ReferenceViewRef;
    use panproto_reference::validation::{
        InstanceShape as ReferenceShape, NativeRef as ReferenceNativeRef,
    };

    let normalize_ref = |reference: &mut ReferenceViewRef| match reference {
        ReferenceViewRef::Node(7) => {}
        ReferenceViewRef::Row(7) => *reference = ReferenceViewRef::Node(7),
        other => {
            panic!("the compact shape carrier introduced an unexpected identity: {other:?}")
        }
    };
    report.shape = ReferenceShape::Graph;
    for diagnostic in &mut report.diagnostics {
        if let Some(subject) = &mut diagnostic.subject {
            normalize_ref(subject);
        }
        for element in &mut diagnostic.elements {
            normalize_ref(element);
        }
    }
    for incomplete in &mut report.incomplete {
        if let Some(subject) = &mut incomplete.subject {
            normalize_ref(subject);
        }
    }
    report.identity =
        BTreeMap::from([(ReferenceViewRef::Node(7), ReferenceNativeRef::GraphNode(7))]);
    report
}

#[test]
fn reference_required_value_shape_square() {
    use panproto_reference::lens::ViewRef as ReferenceViewRef;
    use panproto_reference::model::{
        Graph, Node as ReferenceNode, Table as ReferenceTable, Tree as ReferenceTree,
    };
    use panproto_reference::schema::Schema as ReferenceSchema;
    use panproto_reference::validation::{
        CheckClass as ReferenceClass, CheckKey as ReferenceCheckKey,
        CheckKind as ReferenceCheckKind, CheckStatus as ReferenceStatus,
        Diagnostic as ReferenceDiagnostic, InstanceShape as ReferenceShape,
        NativeRef as ReferenceNativeRef, Report as ReferenceReport,
        Requirement as ReferenceRequirement, SchemaRef as ReferenceSchemaRef,
        Semantics as ReferenceSemantics, Violation as ReferenceViolation,
    };

    let mut schema = ReferenceSchema::default()
        .vertex("record", "object")
        .vertex("scalar", "string")
        .edge("record", "scalar", "prop", Some("value"));
    schema.constraints.insert(
        "scalar".to_owned(),
        vec![("required".to_owned(), String::new())],
    );
    let semantics = BTreeMap::from([("required".to_owned(), ReferenceSemantics::RequiredValue)]);
    let graph = Graph {
        nodes: [(7, ReferenceNode::new(7, "record"))].into(),
        ..Graph::default()
    };
    let graph_report = panproto_reference::validation::validate(&schema, &semantics, &graph);
    let table = ReferenceTable {
        rows: [(7, ("record".to_owned(), BTreeMap::new()))].into(),
        ..ReferenceTable::default()
    };
    let table_report = panproto_reference::validation::validate_table(&schema, &semantics, &table)
        .expect("the one-row table has a graph presentation");
    let tree = ReferenceTree {
        nodes: [(7, ReferenceNode::new(7, "record"))].into(),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 7,
    };
    let tree_report = panproto_reference::validation::validate_tree(&schema, &semantics, &tree)
        .expect("the one-node tree has a graph presentation");

    assert_eq!(
        graph_report.identity,
        BTreeMap::from([(ReferenceViewRef::Node(7), ReferenceNativeRef::GraphNode(7),)])
    );
    assert_eq!(
        table_report.identity,
        BTreeMap::from([(ReferenceViewRef::Row(7), ReferenceNativeRef::TableRow(7),)])
    );
    assert_eq!(
        tree_report.identity,
        BTreeMap::from([(ReferenceViewRef::Node(7), ReferenceNativeRef::TreeNode(7),)])
    );

    let expected = ReferenceReport {
        shape: ReferenceShape::Graph,
        diagnostics: vec![ReferenceDiagnostic {
            check: ReferenceCheckKey {
                kind: ReferenceCheckKind::RequiredValue,
                decl: ReferenceSchemaRef::Constraint(
                    "scalar".to_owned(),
                    "required".to_owned(),
                    String::new(),
                ),
            },
            class: ReferenceClass::GraphLocal,
            requirement: ReferenceRequirement::Required,
            subject: Some(ReferenceViewRef::Node(7)),
            elements: vec![ReferenceViewRef::Node(7)],
            violation: ReferenceViolation::MissingValue(Some("value".to_owned())),
        }],
        incomplete: Vec::new(),
        identity: BTreeMap::from([(ReferenceViewRef::Node(7), ReferenceNativeRef::GraphNode(7))]),
        status: ReferenceStatus::Failed,
    };

    assert_eq!(normalize(graph_report), expected);
    assert_eq!(normalize(table_report), expected);
    assert_eq!(normalize(tree_report), expected);
}

/// The reference person carrier: ids 10, 20, 10, 30 under a document key
/// and bosses 20, 20, 99, 10 referencing it.
fn reference_person_carrier() -> (
    panproto_reference::schema::Schema,
    panproto_reference::model::Graph,
) {
    use panproto_reference::model::{GEdge as ReferenceEdge, Graph, Node as ReferenceNode, SEdge};
    use panproto_reference::schema::{
        Cardinality, Key, KeyPart, KeyRole, KeyScope, NullMatch, OnUnresolved, RefSource,
        Reference, ResolveMode, Schema as ReferenceSchema,
    };
    use panproto_reference::value::Value as ReferenceValue;

    let id = SEdge::new("person", "person.id", "prop", Some("id"));
    let boss = SEdge::new("person", "person.boss", "prop", Some("boss"));
    let mut schema = ReferenceSchema::default()
        .vertex("person", "object")
        .vertex("person.id", "integer")
        .vertex("person.boss", "integer")
        .edge("person", "person.id", "prop", Some("id"))
        .edge("person", "person.boss", "prop", Some("boss"));
    schema.keys.insert(
        "person".to_owned(),
        vec![Key {
            id: "pk".to_owned(),
            parts: vec![KeyPart::Path(vec![id.clone()])],
            role: KeyRole::Identifier,
            scope: KeyScope::Document,
            equality: Vec::new(),
        }],
    );
    schema.references.insert(
        "boss".to_owned(),
        Reference {
            id: "boss".to_owned(),
            holder: "person".to_owned(),
            source: RefSource::KeyValue {
                parts: vec![vec![boss.clone()]],
            },
            target_vertex: "person".to_owned(),
            target_key: Some("pk".to_owned()),
            edge: None,
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );

    let ids = [10_i64, 20, 10, 30];
    let bosses = [20_i64, 20, 99, 10];
    let mut nodes = BTreeMap::new();
    let mut edges = BTreeMap::new();
    for index in 0_u32..4 {
        nodes.insert(index, ReferenceNode::new(index, "person"));
        nodes.insert(
            10 + index,
            ReferenceNode::new(10 + index, "person.id")
                .with_value(ReferenceValue::Int(ids[index as usize])),
        );
        nodes.insert(
            20 + index,
            ReferenceNode::new(20 + index, "person.boss")
                .with_value(ReferenceValue::Int(bosses[index as usize])),
        );
        edges.insert(
            index,
            ReferenceEdge {
                src: index,
                tgt: 10 + index,
                label: id.clone(),
                position: None,
            },
        );
        edges.insert(
            4 + index,
            ReferenceEdge {
                src: index,
                tgt: 20 + index,
                label: boss.clone(),
                position: None,
            },
        );
    }
    (schema, Graph { nodes, edges })
}

fn assert_reference_key_and_reference_carrier() {
    use panproto_reference::lens::ViewRef as ReferenceViewRef;
    use panproto_reference::validation::{
        CheckClass as ReferenceClass, CheckKey as ReferenceCheckKey,
        CheckKind as ReferenceCheckKind, CheckStatus as ReferenceStatus,
        Diagnostic as ReferenceDiagnostic, Requirement as ReferenceRequirement,
        SchemaRef as ReferenceSchemaRef, Violation as ReferenceViolation,
    };
    use panproto_reference::value::Value as ReferenceValue;

    let (schema, graph) = reference_person_carrier();
    let report = panproto_reference::validation::validate(&schema, &BTreeMap::new(), &graph);
    let duplicate = ReferenceDiagnostic {
        check: ReferenceCheckKey {
            kind: ReferenceCheckKind::KeyUnique,
            decl: ReferenceSchemaRef::Key("person".to_owned(), "pk".to_owned()),
        },
        class: ReferenceClass::MaintainedGlobal,
        requirement: ReferenceRequirement::Required,
        subject: None,
        elements: vec![ReferenceViewRef::Node(0), ReferenceViewRef::Node(2)],
        violation: ReferenceViolation::DuplicateKey(vec![(
            "id".to_owned(),
            ReferenceValue::Int(10),
        )]),
    };
    let reference_check = ReferenceCheckKey {
        kind: ReferenceCheckKind::ReferenceResolves,
        decl: ReferenceSchemaRef::Reference("boss".to_owned()),
    };
    let reference_diagnostic = |subject, elements, violation| ReferenceDiagnostic {
        check: reference_check.clone(),
        class: ReferenceClass::MaintainedGlobal,
        requirement: ReferenceRequirement::Required,
        subject: Some(ReferenceViewRef::Node(subject)),
        elements,
        violation,
    };
    assert_eq!(
        report.diagnostics,
        vec![
            duplicate,
            reference_diagnostic(
                2,
                vec![ReferenceViewRef::Node(2)],
                ReferenceViolation::DanglingReference(
                    "boss".to_owned(),
                    vec![ReferenceValue::Int(99)],
                ),
            ),
            reference_diagnostic(
                3,
                vec![
                    ReferenceViewRef::Node(0),
                    ReferenceViewRef::Node(2),
                    ReferenceViewRef::Node(3),
                ],
                ReferenceViolation::AmbiguousReference(
                    "boss".to_owned(),
                    vec![ReferenceValue::Int(10)],
                    vec![ReferenceViewRef::Node(0), ReferenceViewRef::Node(2)],
                ),
            ),
        ]
    );
    assert!(report.incomplete.is_empty());
    assert_eq!(report.status, ReferenceStatus::Failed);
}

#[test]
fn reference_validation_anchors_are_stable_on_compact_native_carriers() {
    assert_reference_validation_repeatable();
    assert_reference_key_and_reference_carrier();
}

/// A plan for a generated schema under the protocol of `syntactic`.
fn plan(case: &Case, syntactic: InstanceShapeKind) -> ValidationPlan {
    ValidationPlan::compile(
        &case.schema,
        &protocol(syntactic),
        &ValidationOptions::default(),
    )
}

/// The name of the application check of [`audited`] plans.
fn audit() -> Name {
    Name::from("audit")
}

/// A plan for a generated schema that also declares the application check
/// `audit` on every item and film.
fn audited(case: &Case, syntactic: InstanceShapeKind) -> ValidationPlan {
    let mut options = ValidationOptions::default();
    options.external.insert(
        audit(),
        ExternalCheckDecl {
            name: audit(),
            anchors: BTreeSet::from([Name::from("item"), Name::from("film")]),
            requirement: Requirement::Required,
        },
    );
    ValidationPlan::compile(&case.schema, &protocol(syntactic), &options)
}

/// The verdicts of `audit` and of the external constraint sort `external`
/// on an instance's elements: an element whose id is divisible by three
/// has none, an odd one is rejected, and the rest pass.
fn audit_verdicts(elements: impl IntoIterator<Item = ViewRef>) -> ExternalVerdicts {
    let mut verdicts = ExternalVerdicts::default();
    for element in elements {
        let (ViewRef::Node(id) | ViewRef::Row(panproto_inst::RowId(id))) = element else {
            continue;
        };
        if id % 3 != 0 {
            let byte = u8::try_from(id % 256).unwrap();
            for name in [audit(), Name::from("external")] {
                verdicts.insert(
                    name,
                    element.clone(),
                    ExternalVerdict {
                        passed: id % 2 == 0,
                        witness: Digest([byte; 32]),
                    },
                );
            }
        }
    }
    verdicts
}

fn unlimited() -> ValidationBudget {
    ValidationBudget::default()
}

fn no_verdicts() -> ExternalVerdicts {
    ExternalVerdicts::default()
}

/// Status soundness of one report: `Passed` exactly when nothing required failed or
/// was left undecided.
fn assert_status_sound(report: &ValidationReport) -> Result<(), TestCaseError> {
    let required_failure = report
        .diagnostics
        .iter()
        .any(|d| d.requirement == Requirement::Required);
    let required_incomplete = report
        .incomplete
        .iter()
        .any(|i| i.requirement == Requirement::Required);
    prop_assert_eq!(
        report.status == CheckStatus::Passed,
        !required_failure && !required_incomplete
    );
    prop_assert_eq!(report.status == CheckStatus::Failed, required_failure);
    Ok(())
}

/// One instance of any shape with the plan and report it was validated
/// with.
#[derive(Debug)]
enum Validated {
    Graph(GInstance),
    Tree(WInstance),
    Table(FInstance),
}

impl Validated {
    fn validate(
        &self,
        plan: &ValidationPlan,
        budget: &ValidationBudget,
        verdicts: &ExternalVerdicts,
    ) -> ValidationReport {
        match self {
            Self::Graph(g) => plan.validate_graph(g, budget, verdicts),
            Self::Tree(t) => plan.validate_tree(t, budget, verdicts),
            Self::Table(f) => plan.validate_table(f, budget, verdicts),
        }
    }

    /// Every node or row of the instance.
    fn elements(&self) -> Vec<ViewRef> {
        match self {
            Self::Graph(g) => g.nodes().keys().map(|id| ViewRef::Node(*id)).collect(),
            Self::Tree(t) => t.nodes.keys().map(|id| ViewRef::Node(*id)).collect(),
            Self::Table(f) => f.row_ids().into_iter().map(ViewRef::Row).collect(),
        }
    }

    /// [`Diagnostic::verify`], and with `verdicts`
    /// [`Diagnostic::verify_with_verdicts`].
    fn verify(
        &self,
        d: &Diagnostic,
        plan: &ValidationPlan,
        verdicts: Option<&ExternalVerdicts>,
    ) -> bool {
        fn go<I: CheckedInstance>(
            d: &Diagnostic,
            plan: &ValidationPlan,
            i: &I,
            verdicts: Option<&ExternalVerdicts>,
        ) -> bool {
            verdicts.map_or_else(|| d.verify(plan, i), |v| d.verify_with_verdicts(plan, i, v))
        }
        match self {
            Self::Graph(g) => go(d, plan, g, verdicts),
            Self::Tree(t) => go(d, plan, t, verdicts),
            Self::Table(f) => go(d, plan, f, verdicts),
        }
    }

    const fn syntactic(&self) -> InstanceShapeKind {
        match self {
            Self::Graph(_) => InstanceShapeKind::Graph,
            Self::Tree(_) => InstanceShapeKind::Tree,
            Self::Table(_) => InstanceShapeKind::Table,
        }
    }
}

/// An instance of any shape.
fn arb_validated() -> impl Strategy<Value = Validated> {
    prop_oneof![
        arb_graph_spec(10).prop_map(|s| Validated::Graph(graph_of(&s))),
        arb_tree_spec(12).prop_map(|s| Validated::Tree(tree_of(&s))),
        arb_table_spec().prop_map(|s| Validated::Table(table_of(&s))),
    ]
}

/// A name no generated schema or instance uses.
fn phantom() -> Name {
    Name::from("phantom")
}

/// Alterations of a true diagnostic, each of which makes it false: a subject
/// the instance does not hold, an element too few or too many, the other
/// requirement, and a payload the instance contradicts.
fn mutants(d: &Diagnostic) -> Vec<(&'static str, Diagnostic)> {
    let mut out = Vec::new();
    let mut m = d.clone();
    m.subject = Some(match d.subject {
        Some(ViewRef::Row(_)) => ViewRef::Row(panproto_inst::RowId(9999)),
        _ => ViewRef::Node(9999),
    });
    out.push(("subject", m));
    let mut m = d.clone();
    if m.elements.pop().is_none() {
        m.elements.push(ViewRef::Node(9999));
    }
    out.push(("elements", m));
    let mut m = d.clone();
    m.requirement = match d.requirement {
        Requirement::Required => Requirement::Advisory,
        Requirement::Advisory => Requirement::Required,
    };
    out.push(("requirement", m));
    if let Some(violation) = mutated_violation(&d.violation) {
        let mut m = d.clone();
        m.violation = violation;
        out.push(("violation", m));
    }
    if let Some(m) = shrunk(d) {
        out.push(("list of elements, shrunk consistently", m));
    }
    out
}

/// The diagnostic with one element left out of the list its violation names
/// and of its elements alike, so that only a checker that confirms the list
/// is complete (every element sharing the key, every candidate, the whole
/// component, every counted edge) refutes it.
fn shrunk(d: &Diagnostic) -> Option<Diagnostic> {
    let mut m = d.clone();
    let dropped = match &mut m.violation {
        Violation::DuplicateKey { elems: list, .. }
        | Violation::AmbiguousReference {
            candidates: list, ..
        } if list.len() > 2 => list.pop()?,
        Violation::Cycle { scc, .. } if scc.len() > 1 => scc.pop()?,
        Violation::OutDegree { edges, found, .. } if !edges.is_empty() => {
            *found -= 1;
            edges.pop()?
        }
        _ => return None,
    };
    m.elements.retain(|e| *e != dropped);
    Some(m)
}

/// The violation with one payload field changed to a value the instance
/// contradicts, for each variant with a payload.
fn mutated_violation(v: &Violation) -> Option<Violation> {
    let mut v = v.clone();
    match &mut v {
        Violation::UnknownAnchor { anchor } => *anchor = phantom(),
        Violation::UndeclaredEdge { label } => label.name = Some(phantom()),
        Violation::IncoherentEndpoint { found, .. } => *found = phantom(),
        Violation::MalformedFan { reason, fan } => match fan {
            Some(record) => record.hyper_edge = phantom(),
            None => {
                *reason = FanViolation::ForeignIncidentEdge {
                    edge: EdgeId(u32::MAX),
                };
            }
        },
        Violation::MissingRequiredEdge { edge } => edge.name = Some(phantom()),
        Violation::OutDegree { found, .. } => *found += 1,
        Violation::DuplicatePosition { position, .. } => *position += 100,
        Violation::ValueConstraint { value, .. } => *value = Some(Value::Str("phantom".into())),
        Violation::MissingValue { field } => *field = Some("phantom".into()),
        Violation::UndeclaredAttribute { key } => *key = "phantom".into(),
        Violation::MistypedAttribute { found, .. } => *found = "phantom".into(),
        Violation::DuplicateKey { key, .. } => {
            let (_, value) = key.first_mut()?;
            *value = Value::Str("phantom".into());
        }
        Violation::DanglingReference { key, .. } | Violation::AmbiguousReference { key, .. } => {
            *key = vec![Value::Str("phantom".into())];
        }
        Violation::Cycle { witness, .. } => {
            witness.pop();
        }
        Violation::ParentMismatch { recorded, .. } => *recorded = Some(9999),
        Violation::ExternalRejected { witness } => witness.0[0] ^= 0xff,
        Violation::OutsideKeyScope { scope, .. } => *scope = phantom(),
        Violation::ReservedVocabulary | Violation::MissingRoot | Violation::Unreachable => {
            return None;
        }
    }
    Some(v)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Every generated schema compiles without a static finding, so the
    /// laws below range over evaluated checks.
    #[test]
    fn generated_schemas_compile_without_findings(case in arb_validation_schema()) {
        for shape in [InstanceShapeKind::Tree, InstanceShapeKind::Table, InstanceShapeKind::Graph] {
            let plan = plan(&case, shape);
            prop_assert!(plan.static_findings().is_empty(), "{:#?}", plan.static_findings());
        }
    }

    /// Every diagnostic verifies against the instance it refutes, and each
    /// alteration of it (its subject, its elements, its requirement, a
    /// payload field) does not. An external rejection verifies against the
    /// verdicts it was read from, and with its witness altered does not.
    /// Each report is also sound in its status.
    #[test]
    fn every_diagnostic_verifies_and_a_mutated_one_does_not(
        case in arb_validation_schema(),
        instance in arb_validated(),
    ) {
        let plan = audited(&case, instance.syntactic());
        let verdicts = audit_verdicts(instance.elements());
        let report = instance.validate(&plan, &unlimited(), &verdicts);
        assert_status_sound(&report)?;
        for d in &report.diagnostics {
            prop_assert!(instance.verify(d, &plan, None), "does not verify: {d:#?}");
            prop_assert!(instance.verify(d, &plan, Some(&verdicts)), "does not verify: {d:#?}");
            for (what, m) in mutants(d) {
                let external_witness = what == "violation"
                    && matches!(m.violation, Violation::ExternalRejected { .. });
                if !external_witness {
                    prop_assert!(
                        !instance.verify(&m, &plan, None),
                        "verifies with its {what} altered: {m:#?}"
                    );
                }
                prop_assert!(
                    !instance.verify(&m, &plan, Some(&verdicts)),
                    "verifies against its verdicts with its {what} altered: {m:#?}"
                );
            }
        }
    }
}

/// The name of a violation's variant.
const fn variant(v: &Violation) -> &'static str {
    match v {
        Violation::UnknownAnchor { .. } => "unknown_anchor",
        Violation::UndeclaredEdge { .. } => "undeclared_edge",
        Violation::IncoherentEndpoint { .. } => "incoherent_endpoint",
        Violation::MalformedFan { fan: None, .. } => "malformed_fan (graph)",
        Violation::MalformedFan { fan: Some(_), .. } => "malformed_fan (tree)",
        Violation::ReservedVocabulary => "reserved_vocabulary",
        Violation::MissingRequiredEdge { .. } => "missing_required_edge",
        Violation::OutDegree { .. } => "out_degree",
        Violation::DuplicatePosition { .. } => "duplicate_position",
        Violation::ValueConstraint { .. } => "value_constraint",
        Violation::MissingValue { .. } => "missing_value",
        Violation::UndeclaredAttribute { .. } => "undeclared_attribute",
        Violation::MistypedAttribute { .. } => "mistyped_attribute",
        Violation::DuplicateKey { .. } => "duplicate_key",
        Violation::DanglingReference { .. } => "dangling_reference",
        Violation::AmbiguousReference { .. } => "ambiguous_reference",
        Violation::Cycle { .. } => "cycle",
        Violation::MissingRoot => "missing_root",
        Violation::Unreachable => "unreachable",
        Violation::ParentMismatch { .. } => "parent_mismatch",
        Violation::ExternalRejected { .. } => "external_rejected",
        Violation::OutsideKeyScope { .. } => "outside_key_scope",
    }
}

/// The generators reach every violation, in some shape, within a fixed
/// number of draws, so the witness law above is exercised on each.
#[test]
fn the_generators_reach_every_violation() {
    use proptest::strategy::ValueTree as _;
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
    let mut runner = TestRunner::new_with_rng(
        Config::default(),
        TestRng::from_seed(RngAlgorithm::ChaCha, &[7; 32]),
    );
    let strategy = (arb_validation_schema(), arb_validated());
    let mut seen: BTreeMap<&'static str, usize> = BTreeMap::new();
    for _ in 0..3000 {
        let (case, instance) = strategy.new_tree(&mut runner).unwrap().current();
        let plan = audited(&case, instance.syntactic());
        let verdicts = audit_verdicts(instance.elements());
        for d in instance
            .validate(&plan, &unlimited(), &verdicts)
            .diagnostics
        {
            *seen.entry(variant(&d.violation)).or_default() += 1;
        }
    }
    let expected = [
        "unknown_anchor",
        "undeclared_edge",
        "incoherent_endpoint",
        "malformed_fan (graph)",
        "malformed_fan (tree)",
        "reserved_vocabulary",
        "missing_required_edge",
        "out_degree",
        "duplicate_position",
        "value_constraint",
        "missing_value",
        "undeclared_attribute",
        "mistyped_attribute",
        "duplicate_key",
        "dangling_reference",
        "ambiguous_reference",
        "cycle",
        "missing_root",
        "unreachable",
        "parent_mismatch",
        "external_rejected",
        "outside_key_scope",
    ];
    let missing: Vec<&&str> = expected
        .iter()
        .filter(|v| !seen.contains_key(**v))
        .collect();
    assert!(
        missing.is_empty(),
        "never generated: {missing:?}; seen: {seen:#?}"
    );
}

/// The check instance a diagnostic or an incomplete check is about.
fn instance_of(check: &CheckKey, subject: Option<&ViewRef>) -> CheckInstance {
    CheckInstance {
        check: check.clone(),
        subject: subject.cloned(),
    }
}

/// The least read limit under which the instance validates with no budget
/// cut: the reads of the unlimited run.
fn unlimited_reads(
    instance: &Validated,
    plan: &ValidationPlan,
    verdicts: &ExternalVerdicts,
) -> u64 {
    let cut = |limit: u64| {
        instance
            .validate(
                plan,
                &ValidationBudget {
                    max_reads: Some(limit),
                },
                verdicts,
            )
            .incomplete
            .iter()
            .any(|i| matches!(i.reason, IncompleteReason::Budget { .. }))
    };
    let mut hi = 1_u64;
    while cut(hi) {
        hi *= 2;
    }
    let mut lo = 0_u64;
    // The least limit with no cut lies in (lo, hi] once lo is cut.
    if !cut(lo) {
        return 0;
    }
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if cut(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// No finite-budget report is `Passed` where the unlimited one
    /// is `Failed` or `Incomplete`; a limit at least the unlimited run's
    /// reads gives the unlimited report; and under a smaller limit the
    /// budget leaves undecided exactly a suffix of the check instances in
    /// ascending order, one `Budget` entry per check key it cuts, each
    /// naming the limit, the reads spent and the first instance of its key
    /// left undecided, while every instance before the cut is decided as
    /// the unlimited run decides it.
    #[test]
    fn v7_budget(
        case in arb_validation_schema(),
        instance in arb_validated(),
        fraction in 0_u64..=100,
    ) {
        let plan = audited(&case, instance.syntactic());
        let verdicts = audit_verdicts(instance.elements());
        let full = instance.validate(&plan, &unlimited(), &verdicts);
        let reads = unlimited_reads(&instance, &plan, &verdicts);
        for limit in [reads, reads + 3, u64::MAX] {
            let budget = ValidationBudget { max_reads: Some(limit) };
            let report = instance.validate(&plan, &budget, &verdicts);
            prop_assert_eq!(report.canonical_bytes(), full.canonical_bytes());
        }
        if reads == 0 {
            return Ok(());
        }
        let limit = reads.saturating_sub(1) * fraction / 100;
        let budget = ValidationBudget { max_reads: Some(limit) };
        let report = instance.validate(&plan, &budget, &verdicts);
        assert_status_sound(&report)?;
        prop_assert!(report.status != CheckStatus::Passed || full.status == CheckStatus::Passed);
        let budget: Vec<(&CheckKey, u64, u64, Option<&ViewRef>)> = report
            .incomplete
            .iter()
            .filter_map(|i| match &i.reason {
                IncompleteReason::Budget { limit, spent, from } if i.subject.is_none() => {
                    Some((&i.check, *limit, *spent, from.as_ref()))
                }
                _ => None,
            })
            .collect();
        prop_assert_eq!(
            budget.len(),
            report
                .incomplete
                .iter()
                .filter(|i| matches!(i.reason, IncompleteReason::Budget { .. }))
                .count(),
            "a budget entry is about every instance of its key"
        );
        prop_assert!(!budget.is_empty(), "a limit below the reads cuts something");
        for (_, l, spent, _) in &budget {
            prop_assert_eq!(*l, limit);
            prop_assert!(*spent <= limit);
            prop_assert_eq!(*spent, budget[0].2);
        }
        let cut = budget
            .iter()
            .map(|(check, _, _, from)| instance_of(check, *from))
            .min()
            .unwrap();
        // Before the cut, the limited run decides as the unlimited run.
        let before = |i: &CheckInstance| *i < cut;
        let decided: Vec<&Diagnostic> = full
            .diagnostics
            .iter()
            .filter(|d| before(&instance_of(&d.check, d.subject.as_ref())))
            .collect();
        let limited: Vec<&Diagnostic> = report.diagnostics.iter().collect();
        prop_assert_eq!(limited, decided);
        let unbudgeted = report
            .incomplete
            .iter()
            .filter(|i| !matches!(i.reason, IncompleteReason::Budget { .. }));
        for i in unbudgeted {
            prop_assert!(
                before(&instance_of(&i.check, i.subject.as_ref())),
                "{i:?} lies past the cut"
            );
        }
        // Every key after the cut's key that has an instance is cut, from its
        // first instance; no key before it is.
        for (key, coverage) in &full.coverage {
            let entry = budget.iter().find(|(check, ..)| *check == key);
            if *key > cut.check && coverage.instances > 0 {
                prop_assert!(entry.is_some(), "{key:?} lies past the cut but is not cut");
            }
            if *key < cut.check {
                prop_assert!(entry.is_none(), "{key:?} lies before the cut but is cut");
            }
            if let Some((_, _, _, from)) = entry {
                prop_assert_eq!(report.coverage[key].instances, coverage.instances);
                if *key > cut.check {
                    prop_assert_eq!(report.coverage[key].incomplete, coverage.instances);
                    prop_assert!(from.is_none() || coverage.instances > 0);
                }
            }
        }
    }
}

/// A violation with every element it names sent through `map`, and its
/// lists of elements put back in ascending order.
fn map_violation(v: &Violation, map: &impl Fn(&ViewRef) -> ViewRef) -> Violation {
    let mut v = v.clone();
    let sort = |refs: &mut Vec<ViewRef>| {
        *refs = refs.iter().map(map).collect();
        refs.sort();
    };
    match &mut v {
        Violation::OutDegree { edges, .. } => sort(edges),
        Violation::DuplicateKey { elems, .. } => sort(elems),
        Violation::AmbiguousReference { candidates, .. } => sort(candidates),
        Violation::Cycle { scc, .. } => sort(scc),
        _ => {}
    }
    v
}

/// A diagnostic as compared across shapes: its check, subject, elements
/// and violation, written out.
fn compared(d: &Diagnostic, map: &impl Fn(&ViewRef) -> ViewRef) -> String {
    let mut elements: Vec<ViewRef> = d.elements.iter().map(map).collect();
    elements.sort();
    format!(
        "{:?} {:?} {:?} {:?} {:?} {:?}",
        d.check,
        d.requirement,
        d.subject.as_ref().map(map),
        elements,
        map_violation(&d.violation, map),
        d.source
    )
}

/// The vertices of the generated schema of kind `kind`.
fn vertices_of_kind(case: &Case, kind: &str) -> BTreeSet<Name> {
    case.schema
        .vertices
        .values()
        .filter(|v| v.kind == kind)
        .map(|v| v.id.clone())
        .collect()
}

/// The column vertices of the generated schema: the targets of the `prop`
/// edges of its tables that are not tables themselves.
fn column_vertices(case: &Case) -> BTreeSet<Name> {
    let tables = vertices_of_kind(case, "table");
    case.schema
        .edges
        .keys()
        .filter(|e| e.kind == "prop" && tables.contains(&e.src) && !tables.contains(&e.tgt))
        .map(|e| e.tgt.clone())
        .collect()
}

/// Whether `d` is a diagnostic of a constraint on one of `vertices`.
fn constrains(d: &Diagnostic, vertices: &BTreeSet<Name>) -> bool {
    matches!(&d.check.decl, SchemaRef::Constraint { vertex, .. } if vertices.contains(vertex))
}

/// The check keys a plan applies to both `a` and `b`.
fn shared_keys(
    plan: &ValidationPlan,
    a: InstanceShapeKind,
    b: InstanceShapeKind,
) -> BTreeSet<CheckKey> {
    plan.checks()
        .iter()
        .filter(|(_, spec)| spec.applies_to(a) && spec.applies_to(b))
        .map(|(key, _)| key.clone())
        .collect()
}

/// The tree square: the diagnostics of every kind a tree shares with
/// graphs are those of its graph form, element for element through the
/// tree's embedding. Fan diagnostics, whose subject is the fan's parent
/// in a tree and the fan node in a graph, are compared as the set of
/// each fan's record with the reason it breaks.
fn tree_square(case: &Case, spec: &TreeSpec) -> Result<(), TestCaseError> {
    let tree = tree_of(spec);
    // A tree with reserved vocabulary or a fan naming an absent node has no
    // graph form. The case is rejected rather than passed, so proptest draws
    // a replacement and every counted case is compared.
    let Ok((graph, emb)) = GInstance::from_wtype_embedded(&tree, None) else {
        return Err(TestCaseError::reject("the tree has no graph form"));
    };
    let plan = audited(case, InstanceShapeKind::Tree);
    let verdicts = audit_verdicts(tree.nodes.keys().map(|id| ViewRef::Node(*id)));
    let shared = shared_keys(&plan, InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let w = plan.validate_tree(&tree, &unlimited(), &verdicts);
    let g = plan.validate_graph(&graph, &unlimited(), &verdicts);
    let map = |r: &ViewRef| emb.map_ref(r).unwrap_or_else(|| r.clone());
    let id = |r: &ViewRef| r.clone();
    let fan_kind = |d: &Diagnostic| d.check.kind == CheckKind::FanWellFormed;
    // Non-fan diagnostics are compared as sorted vectors, so multiplicity
    // counts as well as membership.
    let mut mapped: Vec<String> = w
        .diagnostics
        .iter()
        .filter(|d| shared.contains(&d.check) && !fan_kind(d))
        .map(|d| compared(d, &map))
        .collect();
    let mut direct: Vec<String> = g
        .diagnostics
        .iter()
        .filter(|d| shared.contains(&d.check) && !fan_kind(d))
        .map(|d| compared(d, &id))
        .collect();
    mapped.sort();
    direct.sort();
    prop_assert_eq!(&mapped, &direct);
    let tree_pairs: Vec<(FanRecord, FanViolation)> = w
        .diagnostics
        .iter()
        .filter_map(|d| match &d.violation {
            Violation::MalformedFan {
                reason,
                fan: Some(record),
            } => Some((record.clone(), reason.clone())),
            _ => None,
        })
        .collect();
    prop_assert_eq!(
        tree_pairs.len(),
        w.diagnostics.iter().filter(|d| fan_kind(d)).count(),
        "every tree fan diagnostic names its fan"
    );
    let tree_fans: BTreeSet<(FanRecord, FanViolation)> = tree_pairs.into_iter().collect();
    // Equal tree fans are one element of the tree's fan multiset and give
    // one diagnostic per reason, while the graph form has a fan node, and a
    // diagnostic, for each; so every graph diagnostic is mapped before the
    // pairs are compared as a set.
    let graph_pairs: Vec<(FanRecord, FanViolation)> = g
        .diagnostics
        .iter()
        .filter_map(|d| match (&d.violation, &d.subject) {
            (Violation::MalformedFan { reason, fan: None }, Some(ViewRef::Node(node))) => {
                let k = emb.fans.iter().position(|f| f.node == *node)?;
                Some((FanRecord::of(&tree.fans[k]), reason.clone()))
            }
            _ => None,
        })
        .collect();
    prop_assert_eq!(
        graph_pairs.len(),
        g.diagnostics.iter().filter(|d| fan_kind(d)).count(),
        "every graph fan diagnostic is about an embedded fan"
    );
    let graph_fans: BTreeSet<(FanRecord, FanViolation)> = graph_pairs.into_iter().collect();
    prop_assert_eq!(tree_fans, graph_fans);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// The tree square over drawn schemas and trees (see [`tree_square`]).
    #[test]
    fn v8_tree_square(case in arb_validation_schema(), spec in arb_tree_spec(8)) {
        tree_square(&case, &spec)?;
    }

    /// The table square: the diagnostics of every kind a table instance
    /// shares with graphs are those of its graph form, row for node and pair
    /// for edge through the table's embedding. Every shared check is
    /// compared, the constraints on column and table vertices among them.
    #[test]
    fn v8_table_square(case in arb_validation_schema(), spec in arb_table_spec()) {
        let table = table_of(&spec);
        let (graph, emb) =
            GInstance::from_finstance(&table).expect("a table instance has a graph form");
        let plan = audited(&case, InstanceShapeKind::Table);
        let verdicts = audit_verdicts(table.row_ids().into_iter().map(ViewRef::Row));
        let graph_verdicts =
            audit_verdicts(table.row_ids().into_iter().map(|r| ViewRef::Node(r.0)));
        let shared = shared_keys(&plan, InstanceShapeKind::Table, InstanceShapeKind::Graph);
        let t = plan.validate_table(&table, &unlimited(), &verdicts);
        let g = plan.validate_graph(&graph, &unlimited(), &graph_verdicts);
        let map = |r: &ViewRef| emb.map_ref(r).unwrap_or_else(|| r.clone());
        let id = |r: &ViewRef| r.clone();
        let mut mapped: Vec<String> = t
            .diagnostics
            .iter()
            .filter(|d| shared.contains(&d.check))
            .map(|d| compared(d, &map))
            .collect();
        let mut direct: Vec<String> = g
            .diagnostics
            .iter()
            .filter(|d| shared.contains(&d.check))
            .map(|d| compared(d, &id))
            .collect();
        mapped.sort();
        direct.sort();
        prop_assert_eq!(mapped, direct);
    }
}

/// Two equal fans of a tree are one element of its fan multiset and give
/// one diagnostic per reason, while the tree's graph form holds a fan node
/// for each and gives one diagnostic at each node; the square compares them
/// as the same set of fan records with reasons, every graph diagnostic
/// mapped to the fan its node embeds.
#[test]
fn the_tree_square_holds_for_equal_fans() {
    let features = Features {
        max_len: None,
        num_bounds: None,
        max_out: None,
        min_out: None,
        flags: BTreeSet::new(),
        item_key: None,
        item_ref: None,
        film_ref: None,
        title_max: None,
    };
    let case = Case {
        schema: schema_of(&features),
        features,
    };
    let fan = FanSpec {
        declared: true,
        parent: 1,
        a: 0,
        b: None,
        c: Some(1),
    };
    let spec = TreeSpec {
        root: (0, ValueSpec::Absent),
        nodes: Vec::new(),
        fans: vec![fan.clone(), fan],
        missing_root: false,
        skew: None,
        reserved: None,
        records: vec![RecordSpec {
            under: 0,
            name: None,
            num: None,
            tag: None,
            fresh_doc: false,
        }],
    };
    let tree = tree_of(&spec);
    assert_eq!(tree.fans.len(), 2);
    assert_eq!(tree.fans[0], tree.fans[1]);
    let (graph, _) = GInstance::from_wtype_embedded(&tree, None).unwrap();
    let plan = audited(&case, InstanceShapeKind::Tree);
    let fan_diagnostics = |r: &ValidationReport| {
        r.diagnostics
            .iter()
            .filter(|d| d.check.kind == CheckKind::FanWellFormed)
            .count()
    };
    let w = plan.validate_tree(&tree, &unlimited(), &no_verdicts());
    let g = plan.validate_graph(&graph, &unlimited(), &no_verdicts());
    assert_eq!((fan_diagnostics(&w), fan_diagnostics(&g)), (1, 2));
    tree_square(&case, &spec).unwrap();
}

/// A case whose schema carries only the declarations `flags` and the title
/// bound `title_max`.
fn case_of(flags: &[Flag], title_max: Option<u64>) -> Case {
    let features = Features {
        max_len: None,
        num_bounds: None,
        max_out: None,
        min_out: None,
        flags: flags.iter().copied().collect(),
        item_key: None,
        item_ref: None,
        film_ref: None,
        title_max,
    };
    Case {
        schema: schema_of(&features),
        features,
    }
}

/// The diagnostics of a report whose check is a constraint on `vertex`, as
/// subject and violation.
fn constraint_diagnostics(
    report: &ValidationReport,
    vertex: &str,
) -> Vec<(Option<ViewRef>, Violation)> {
    let vertices = BTreeSet::from([Name::from(vertex)]);
    report
        .diagnostics
        .iter()
        .filter(|d| constrains(d, &vertices))
        .map(|d| (d.subject.clone(), d.violation.clone()))
        .collect()
}

/// A missing value of `field` (`None` for an element's own value).
fn missing(field: Option<&str>) -> Violation {
    Violation::MissingValue {
        field: field.map(str::to_owned),
    }
}

/// A constraint on a column vertex is read from each row's cell in a table
/// instance and from the same field of the row's node in its graph form,
/// which keeps the cells as fields, so both give one diagnostic: at the
/// row, and at its node.
#[test]
fn column_constraints_are_read_from_cells_in_every_shape() {
    let case = case_of(&[], Some(1));
    let table = FInstance::new()
        .with_rows(
            "film",
            [(
                RowId(0),
                HashMap::from([
                    ("title".to_owned(), Value::Str("bb".into())),
                    ("year".to_owned(), Value::Int(2000)),
                ]),
            )],
        )
        .unwrap();
    let (graph, _) = GInstance::from_finstance(&table).unwrap();
    assert_eq!(
        graph.node(0).unwrap().extra_fields.get("title"),
        Some(&Value::Str("bb".into()))
    );
    let plan = plan(&case, InstanceShapeKind::Table);
    let t = plan.validate_table(&table, &unlimited(), &no_verdicts());
    let g = plan.validate_graph(&graph, &unlimited(), &no_verdicts());
    let at = |r: &ValidationReport| -> Vec<(CheckKind, Option<ViewRef>)> {
        r.diagnostics
            .iter()
            .map(|d| (d.check.kind, d.subject.clone()))
            .collect()
    };
    assert_eq!(
        at(&t),
        vec![(CheckKind::ValueConstraint, Some(ViewRef::Row(RowId(0))))]
    );
    assert_eq!(
        at(&g),
        vec![(CheckKind::ValueConstraint, Some(ViewRef::Node(0)))]
    );
    let bound = Violation::ValueConstraint {
        sort: "maxChars".into(),
        bound: "1".into(),
        value: Some(Value::Str("bb".into())),
    };
    assert_eq!(t.diagnostics[0].violation, bound);
    assert_eq!(g.diagnostics[0].violation, bound);
}

/// A value of the column vertex `tag` held by a child node at the edge
/// `tag` is read at the child, so the parent holds no field site for that
/// edge: under `RequiredValue` on `tag`, an item with a valued `tag` child
/// has no diagnostic (whatever its own `tag` field), an item with a `tag`
/// field and no child has none either, and an item with neither is missing
/// the field `tag`, in a tree, in its graph form, and in a table of items.
#[test]
fn a_column_value_held_by_a_child_node_is_not_a_field_site() {
    use panproto_inst::{FieldPresence, Node};
    let case = case_of(&[Flag::RequiredTag], None);
    let item = |id: u32| Node::new(id, "item");
    let tag = |id: u32, v: &str| {
        Node::new(id, "tag").with_value(FieldPresence::Present(Value::Str(v.into())))
    };
    let nodes = HashMap::from([
        (0, Node::new(0, "doc")),
        (1, item(1)),
        (2, tag(2, "a")),
        (3, item(3)),
        (4, item(4).with_extra_field("tag", Value::Str("b".into()))),
        (5, item(5).with_extra_field("tag", Value::Null)),
        (6, tag(6, "c")),
    ]);
    let arcs = vec![
        (0, 1, e_items()),
        (1, 2, e_tag()),
        (0, 3, e_items()),
        (0, 4, e_items()),
        (0, 5, e_items()),
        (5, 6, e_tag()),
    ];
    let tree = WInstance::new(nodes, arcs, Vec::new(), 0, "doc".into());
    let (graph, emb) = GInstance::from_wtype_embedded(&tree, None).unwrap();
    let plan = plan(&case, InstanceShapeKind::Tree);
    let w = plan.validate_tree(&tree, &unlimited(), &no_verdicts());
    let g = plan.validate_graph(&graph, &unlimited(), &no_verdicts());
    let expected = vec![(Some(ViewRef::Node(3)), missing(Some("tag")))];
    assert_eq!(constraint_diagnostics(&w, "tag"), expected);
    assert_eq!(
        emb.map_ref(&ViewRef::Node(3)),
        Some(ViewRef::Node(3)),
        "the embedding keeps node ids"
    );
    assert_eq!(constraint_diagnostics(&g, "tag"), expected);
    // The matching table: rows of the vertex `item`, one with a `tag` cell
    // and one without.
    let table = FInstance::new()
        .with_rows(
            "item",
            [
                (
                    RowId(0),
                    HashMap::from([("tag".to_owned(), Value::Str("a".into()))]),
                ),
                (RowId(1), HashMap::new()),
            ],
        )
        .unwrap();
    let t = plan.validate_table(&table, &unlimited(), &no_verdicts());
    assert_eq!(
        constraint_diagnostics(&t, "tag"),
        vec![(Some(ViewRef::Row(RowId(1))), missing(Some("tag")))]
    );
}

/// A row has no value of its own, so `RequiredValue` on a table vertex
/// finds each row's own value missing, and the row's node in the graph form
/// (whose value is absent) likewise. A `prop` edge into the table vertex
/// adds the field sites of the rows it leaves: a film whose `cast` pair
/// leaves it holds none, a film with a `cast` cell holds a value, and a
/// film with neither is missing the field `cast`.
#[test]
fn a_required_value_on_a_table_vertex_reads_the_absent_row_value() {
    let case = case_of(&[Flag::RequiredActor], None);
    let film = |cast: Option<&str>| {
        let mut row = HashMap::from([
            ("title".to_owned(), Value::Str("t".into())),
            ("year".to_owned(), Value::Int(2000)),
        ]);
        if let Some(c) = cast {
            row.insert("cast".to_owned(), Value::Str(c.into()));
        }
        row
    };
    let actor = |name: &str| HashMap::from([("aname".to_owned(), Value::Str(name.into()))]);
    let mut table = FInstance::new()
        .with_rows(
            "film",
            [
                (RowId(0), film(None)),
                (RowId(1), film(Some("x"))),
                (RowId(2), film(None)),
            ],
        )
        .unwrap()
        .with_rows("actor", [(RowId(10), actor("a")), (RowId(11), actor("b"))])
        .unwrap();
    table
        .insert_foreign_key(e_cast(), RowId(0), RowId(10))
        .unwrap();
    let (graph, _) = GInstance::from_finstance(&table).unwrap();
    let plan = plan(&case, InstanceShapeKind::Table);
    let t = plan.validate_table(&table, &unlimited(), &no_verdicts());
    let g = plan.validate_graph(&graph, &unlimited(), &no_verdicts());
    assert_eq!(
        constraint_diagnostics(&t, "actor"),
        vec![
            (Some(ViewRef::Row(RowId(2))), missing(Some("cast"))),
            (Some(ViewRef::Row(RowId(10))), missing(None)),
            (Some(ViewRef::Row(RowId(11))), missing(None)),
        ]
    );
    assert_eq!(
        constraint_diagnostics(&g, "actor"),
        vec![
            (Some(ViewRef::Node(2)), missing(Some("cast"))),
            (Some(ViewRef::Node(10)), missing(None)),
            (Some(ViewRef::Node(11)), missing(None)),
        ]
    );
}

/// A schema and protocol of two tables whose columns carry `External`
/// constraints, as the sql `ENUM` and dataframe `gt` rows declare them: the
/// column `t.kind` of table `t` under `ENUM`, and the column `df.x` of table
/// `df` under `gt`.
fn external_columns() -> (panproto_schema::Schema, panproto_schema::Protocol) {
    use panproto_schema::{ConstraintSemantics, Protocol};
    let kind = validation_support::edge("t", "t.kind", "prop", "kind");
    let x = validation_support::edge("df", "df.x", "prop", "x");
    let schema = Builder::default()
        .vertex("t", "table")
        .vertex("t.kind", "string")
        .vertex("df", "table")
        .vertex("df.x", "integer")
        .edge(&kind)
        .edge(&x)
        .constraint("t.kind", "ENUM", "'a','b'")
        .constraint("df.x", "gt", "0")
        .build();
    let sorts = ["ENUM", "gt"];
    let protocol = Protocol {
        name: "external-columns".into(),
        constraint_sorts: sorts.iter().map(|s| (*s).to_owned()).collect(),
        constraint_semantics: sorts
            .iter()
            .map(|s| ((*s).to_owned(), ConstraintSemantics::External))
            .collect(),
        syntactic_shape: InstanceShapeKind::Table,
        semantic_shape: InstanceShapeKind::Graph,
        ..Protocol::default()
    };
    (schema, protocol)
}

/// The rows of [`external_columns`]: rows 0 and 1 of `t`, of kinds `a` and
/// `z`, and rows 2 and 3 of `df`, with `x` 1 and -1.
fn external_rows() -> FInstance {
    let cell = |c: &str, v: Value| HashMap::from([(c.to_owned(), v)]);
    FInstance::new()
        .with_rows(
            "t",
            [
                (RowId(0), cell("kind", Value::Str("a".into()))),
                (RowId(1), cell("kind", Value::Str("z".into()))),
            ],
        )
        .unwrap()
        .with_rows(
            "df",
            [
                (RowId(2), cell("x", Value::Int(1))),
                (RowId(3), cell("x", Value::Int(-1))),
            ],
        )
        .unwrap()
}

/// The key of the `External` check of `sort` on `vertex`.
fn external_key(plan: &ValidationPlan, vertex: &str, sort: &str) -> CheckKey {
    plan.checks()
        .keys()
        .find(|k| {
            k.kind == CheckKind::External
                && matches!(
                    &k.decl,
                    SchemaRef::Constraint { vertex: v, constraint }
                        if v.as_ref() == vertex && constraint.sort.as_ref() == sort
                )
        })
        .cloned()
        .unwrap_or_else(|| panic!("{sort} on {vertex} compiles to an External check"))
}

/// The verdicts on [`external_rows`], each row read as `at(row)`: rows 1
/// and 3 rejected, rows 0 and 2 passed, each with the witness of its row.
fn external_row_verdicts(at: &dyn Fn(u32) -> ViewRef) -> ExternalVerdicts {
    let mut v = ExternalVerdicts::default();
    for (name, row, passed) in [
        ("ENUM", 0, true),
        ("ENUM", 1, false),
        ("gt", 2, true),
        ("gt", 3, false),
    ] {
        v.insert(
            name.into(),
            at(row),
            ExternalVerdict {
                passed,
                witness: Digest([u8::try_from(row).unwrap(); 32]),
            },
        );
    }
    v
}

/// The check, subject (through `m`) and payload of each `External` entry of
/// a report: its incomplete entries when `undecided`, else its diagnostics.
fn external_entries(
    r: &ValidationReport,
    m: &dyn Fn(&ViewRef) -> ViewRef,
    undecided: bool,
) -> Vec<(CheckKey, Option<ViewRef>, String)> {
    if undecided {
        r.incomplete
            .iter()
            .filter(|i| i.check.kind == CheckKind::External)
            .map(|i| {
                (
                    i.check.clone(),
                    i.subject.as_ref().map(m),
                    format!("{:?}", i.reason),
                )
            })
            .collect()
    } else {
        r.diagnostics
            .iter()
            .map(|d| {
                (
                    d.check.clone(),
                    d.subject.as_ref().map(m),
                    format!("{:?}", d.violation),
                )
            })
            .collect()
    }
}

/// An `External` constraint on a column vertex has one instance per row
/// that holds a cell of the column, though no element is anchored at the
/// column vertex: with no verdict each row is undecided for want of a
/// witness and the status is not `Passed`, a rejecting verdict fails the
/// row, and the graph form of the table agrees node for row.
#[test]
fn an_external_column_constraint_has_an_instance_per_row() {
    let (schema, protocol) = external_columns();
    let table = external_rows();
    let (graph, emb) = GInstance::from_finstance(&table).unwrap();
    let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
    let kind_key = external_key(&plan, "t.kind", "ENUM");
    let x_key = external_key(&plan, "df.x", "gt");
    let map = |r: &ViewRef| emb.map_ref(r).unwrap_or_else(|| r.clone());
    let id = |r: &ViewRef| r.clone();
    let row = |r: u32| Some(ViewRef::Row(RowId(r)));
    // No verdict: one undecided instance per row.
    let t = plan.validate_table(&table, &unlimited(), &no_verdicts());
    let g = plan.validate_graph(&graph, &unlimited(), &no_verdicts());
    for key in [&kind_key, &x_key] {
        assert_eq!(t.coverage[key].instances, 2, "{key:?}");
        assert_eq!(g.coverage[key].instances, 2, "{key:?}");
    }
    let witness = |name: &str| {
        format!(
            "{:?}",
            IncompleteReason::MissingExternalWitness { name: name.into() }
        )
    };
    // In check-key order: `df.x` before `t.kind`.
    let expected = vec![
        (x_key.clone(), row(2), witness("gt")),
        (x_key.clone(), row(3), witness("gt")),
        (kind_key.clone(), row(0), witness("ENUM")),
        (kind_key.clone(), row(1), witness("ENUM")),
    ];
    assert_eq!(external_entries(&t, &id, true), expected);
    assert_ne!(t.status, CheckStatus::Passed);
    assert_eq!(
        external_entries(&g, &id, true),
        external_entries(&t, &map, true)
    );
    assert_ne!(g.status, CheckStatus::Passed);
    // Rows 1 and 3 rejected, rows 0 and 2 passed.
    let row_verdicts = external_row_verdicts(&|r| ViewRef::Row(RowId(r)));
    let node_verdicts = external_row_verdicts(&ViewRef::Node);
    let t = plan.validate_table(&table, &unlimited(), &row_verdicts);
    let g = plan.validate_graph(&graph, &unlimited(), &node_verdicts);
    assert_eq!(t.status, CheckStatus::Failed);
    assert_eq!(g.status, CheckStatus::Failed);
    let rejection = |r: u8| {
        format!(
            "{:?}",
            Violation::ExternalRejected {
                witness: Digest([r; 32]),
            }
        )
    };
    assert_eq!(
        external_entries(&t, &id, false),
        vec![
            (x_key, row(3), rejection(3)),
            (kind_key, row(1), rejection(1)),
        ]
    );
    assert_eq!(
        external_entries(&g, &id, false),
        external_entries(&t, &map, false)
    );
    let verified = |r: &ValidationReport, v: &ExternalVerdicts, graph_form: bool| {
        r.diagnostics.iter().all(|d| {
            if graph_form {
                d.verify_with_verdicts(&plan, &graph, v)
            } else {
                d.verify_with_verdicts(&plan, &table, v)
            }
        })
    };
    assert!(verified(&t, &row_verdicts, false));
    assert!(verified(&g, &node_verdicts, true));
}

/// The squares above compare something: over fixed draws, many trees have a
/// graph form and a shared diagnostic, some of them a fan diagnostic, many
/// table instances have a shared diagnostic, and the shared diagnostics of
/// table instances include many of constraints on column vertices and on
/// table vertices.
#[test]
fn the_squares_compare_diagnostics() {
    use proptest::strategy::ValueTree as _;
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
    let mut runner = TestRunner::new_with_rng(
        Config::default(),
        TestRng::from_seed(RngAlgorithm::ChaCha, &[11; 32]),
    );
    let strategy = (arb_validation_schema(), arb_tree_spec(8), arb_table_spec());
    let (mut trees, mut fans, mut tables) = (0, 0, 0);
    let (mut column_diagnostics, mut table_vertex_diagnostics) = (0, 0);
    for _ in 0..1000 {
        let (case, tree_spec, table_spec) = strategy.new_tree(&mut runner).unwrap().current();
        let tree = tree_of(&tree_spec);
        if GInstance::from_wtype_embedded(&tree, None).is_ok() {
            let plan = plan(&case, InstanceShapeKind::Tree);
            let shared = shared_keys(&plan, InstanceShapeKind::Tree, InstanceShapeKind::Graph);
            let report = plan.validate_tree(&tree, &unlimited(), &no_verdicts());
            if report.diagnostics.iter().any(|d| shared.contains(&d.check)) {
                trees += 1;
            }
            if report
                .diagnostics
                .iter()
                .any(|d| d.check.kind == CheckKind::FanWellFormed)
            {
                fans += 1;
            }
        }
        let plan = plan(&case, InstanceShapeKind::Table);
        let shared = shared_keys(&plan, InstanceShapeKind::Table, InstanceShapeKind::Graph);
        let report = plan.validate_table(&table_of(&table_spec), &unlimited(), &no_verdicts());
        if report.diagnostics.iter().any(|d| shared.contains(&d.check)) {
            tables += 1;
        }
        let (columns, table_vertices) = (column_vertices(&case), vertices_of_kind(&case, "table"));
        let compared = || {
            report
                .diagnostics
                .iter()
                .filter(|d| shared.contains(&d.check))
        };
        column_diagnostics += compared().filter(|d| constrains(d, &columns)).count();
        table_vertex_diagnostics += compared()
            .filter(|d| constrains(d, &table_vertices))
            .count();
    }
    assert!(trees > 300, "{trees} trees with a shared diagnostic");
    assert!(fans > 30, "{fans} trees with a fan diagnostic");
    assert!(tables > 300, "{tables} tables with a shared diagnostic");
    assert!(
        column_diagnostics > 300,
        "{column_diagnostics} compared diagnostics of column-vertex constraints"
    );
    assert!(
        table_vertex_diagnostics > 300,
        "{table_vertex_diagnostics} compared diagnostics of table-vertex constraints"
    );
}

/// An instance read the way the key oracle reads it, written apart from the
/// validator: elements by anchor, the value of a one-edge key path, and the
/// nearest enclosing document.
#[derive(Clone, Copy)]
enum Oracle<'a> {
    Graph(&'a GInstance),
    Tree(&'a WInstance),
    Table(&'a FInstance),
}

impl Oracle<'_> {
    /// The typed elements anchored at `vertex`, ascending.
    fn anchored(self, vertex: &str) -> Vec<ViewRef> {
        let mut out: Vec<ViewRef> = match self {
            Self::Graph(g) => g
                .nodes()
                .values()
                .filter(|n| n.anchor == vertex && n.shape != panproto_inst::NodeShape::HyperEdge)
                .map(|n| ViewRef::Node(n.id))
                .collect(),
            Self::Tree(t) => t
                .nodes
                .values()
                .filter(|n| n.anchor == vertex)
                .map(|n| ViewRef::Node(n.id))
                .collect(),
            Self::Table(f) => f
                .rows(vertex)
                .into_iter()
                .map(|(id, _)| ViewRef::Row(id))
                .collect(),
        };
        out.sort();
        out
    }

    /// A node's own value, unless absent or null.
    fn own(node: &panproto_inst::Node) -> Option<Value> {
        match node.value.as_ref()? {
            panproto_inst::FieldPresence::Present(v) if *v != Value::Null => Some(v.clone()),
            _ => None,
        }
    }

    /// The value of the key path `[edge]` at `element`: the first value
    /// among the targets of its `edge` out-edges, in edge order, or, when it
    /// has none, its field named like the edge. `None` is SQL's NULL.
    fn part(self, element: &ViewRef, edge: &Edge) -> Option<Value> {
        let field = edge
            .name
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        let non_null = |v: Option<&Value>| v.filter(|v| **v != Value::Null).cloned();
        match (self, element) {
            (Self::Graph(g), ViewRef::Node(id)) => {
                let mut out: Vec<(EdgeId, u32)> = g
                    .edges_sorted()
                    .filter(|(_, e)| e.src == *id && e.edge == *edge)
                    .map(|(eid, e)| (eid, e.tgt))
                    .collect();
                out.sort();
                if out.is_empty() {
                    return non_null(g.node(*id)?.extra_fields.get(&field));
                }
                out.iter().find_map(|(_, t)| Self::own(g.node(*t)?))
            }
            (Self::Tree(t), ViewRef::Node(id)) => {
                let children: Vec<u32> = t
                    .arcs
                    .iter()
                    .filter(|(p, _, e)| p == id && e == edge)
                    .map(|(_, c, _)| *c)
                    .collect();
                if children.is_empty() {
                    return non_null(t.nodes.get(id)?.extra_fields.get(&field));
                }
                children.iter().find_map(|c| Self::own(t.nodes.get(c)?))
            }
            (Self::Table(f), ViewRef::Row(id)) => non_null(f.row(*id)?.get(&field)),
            _ => None,
        }
    }

    /// The nearest element anchored at `doc` that reaches `element` along
    /// edges (the least of the nearest), walking back one edge at a time.
    fn document(self, element: &ViewRef) -> Option<ViewRef> {
        let parents = |n: u32| -> Vec<u32> {
            match self {
                Self::Graph(g) => g
                    .edges_sorted()
                    .filter(|(_, e)| e.tgt == n)
                    .map(|(_, e)| e.src)
                    .collect(),
                Self::Tree(t) => t
                    .arcs
                    .iter()
                    .filter(|(_, c, _)| *c == n)
                    .map(|(p, _, _)| *p)
                    .collect(),
                Self::Table(_) => Vec::new(),
            }
        };
        let is_doc = |n: u32| match self {
            Self::Graph(g) => g.node(n).is_some_and(|x| {
                x.anchor == "doc" && x.shape != panproto_inst::NodeShape::HyperEdge
            }),
            Self::Tree(t) => t.nodes.get(&n).is_some_and(|x| x.anchor == "doc"),
            Self::Table(_) => false,
        };
        let ViewRef::Node(start) = element else {
            return None;
        };
        let mut seen = BTreeSet::from([*start]);
        let mut level = vec![*start];
        while !level.is_empty() {
            let mut next: Vec<u32> = level
                .iter()
                .flat_map(|n| parents(*n))
                .filter(|p| seen.insert(*p))
                .collect();
            next.sort_unstable();
            if let Some(doc) = next.iter().find(|n| is_doc(**n)) {
                return Some(ViewRef::Node(*doc));
            }
            level = next;
        }
        None
    }
}

/// One key the oracle checks: its declaration, keyed vertex, one-edge
/// parts and whether it is unique within the enclosing document.
struct OracleKey {
    vertex: &'static str,
    id: &'static str,
    parts: Vec<Edge>,
    within_doc: bool,
}

/// One reference the oracle checks.
struct OracleRef {
    id: &'static str,
    holder: &'static str,
    part: Edge,
    target: &'static str,
    target_key: Vec<Edge>,
    nullable: bool,
    requirement: Requirement,
    /// Whether the target key is unique within the enclosing document, so
    /// that a holder's targets are those in its own document.
    within_doc: bool,
}

/// The keys and references a generated schema declares, as the oracle
/// reads them.
fn oracle_declarations(case: &Case) -> (Vec<OracleKey>, Vec<OracleRef>) {
    use panproto_schema::{KeyScope, OnUnresolved};
    use validation_support::{e_aname, e_lead, e_name, e_num, e_tag, e_title, e_year};
    let f = &case.features;
    let mut keys = vec![
        OracleKey {
            vertex: "film",
            id: "film_pk",
            parts: vec![e_title(), e_year()],
            within_doc: false,
        },
        OracleKey {
            vertex: "actor",
            id: "actor_pk",
            parts: vec![e_aname()],
            within_doc: false,
        },
    ];
    let requirement = |policy: &OnUnresolved| match policy {
        OnUnresolved::Error => Requirement::Required,
        OnUnresolved::Complement => Requirement::Advisory,
    };
    let mut refs = Vec::new();
    if let Some(scope) = &f.item_key {
        keys.push(OracleKey {
            vertex: "item",
            id: "pk",
            parts: vec![e_name()],
            within_doc: *scope != KeyScope::Global,
        });
        if f.has(validation_support::Flag::Composite) {
            keys.push(OracleKey {
                vertex: "item",
                id: "nk",
                parts: vec![e_name(), e_num()],
                within_doc: true,
            });
        }
        if let Some((nullable, policy)) = &f.item_ref {
            refs.push(OracleRef {
                id: "item_ref",
                holder: "item",
                part: e_tag(),
                target: "item",
                target_key: vec![e_name()],
                nullable: *nullable,
                requirement: requirement(policy),
                within_doc: *scope != KeyScope::Global,
            });
        }
    }
    if let Some((nullable, policy)) = &f.film_ref {
        refs.push(OracleRef {
            id: "film_lead",
            holder: "film",
            part: e_lead(),
            target: "actor",
            target_key: vec![e_aname()],
            nullable: *nullable,
            requirement: requirement(policy),
            within_doc: false,
        });
    }
    (keys, refs)
}

/// The key value of `element` under `parts`, or `None` when a part is NULL.
fn key_value(oracle: Oracle<'_>, element: &ViewRef, parts: &[Edge]) -> Option<Vec<Value>> {
    parts.iter().map(|p| oracle.part(element, p)).collect()
}

/// The `KeyUnique` and `ReferenceResolves` diagnostics the oracle expects,
/// written as [`compared`] writes a diagnostic (with no source).
fn oracle_diagnostics(case: &Case, oracle: Oracle<'_>, references: bool) -> Vec<String> {
    let (keys, refs) = oracle_declarations(case);
    let mut out = Vec::new();
    let id = |r: &ViewRef| r.clone();
    let name = |e: &Edge| e.name.as_ref().map(ToString::to_string).unwrap_or_default();
    for key in &keys {
        let mut buckets: BTreeMap<String, (Vec<Value>, Vec<ViewRef>)> = BTreeMap::new();
        for element in oracle.anchored(key.vertex) {
            let Some(values) = key_value(oracle, &element, &key.parts) else {
                continue;
            };
            let scope = key.within_doc.then(|| oracle.document(&element)).flatten();
            if key.within_doc && scope.is_none() {
                // Outside the key's domain: no bucket, one advisory
                // observation naming the element.
                let d = Diagnostic {
                    check: CheckKey::new(
                        CheckKind::KeyUnique,
                        SchemaRef::Key {
                            vertex: key.vertex.into(),
                            key: key.id.into(),
                        },
                    ),
                    class: CheckKind::KeyUnique.class(),
                    requirement: Requirement::Advisory,
                    subject: Some(element.clone()),
                    elements: vec![element.clone()],
                    violation: Violation::OutsideKeyScope {
                        key: key.id.into(),
                        scope: "doc".into(),
                    },
                    source: None,
                };
                out.push(compared(&d, &id));
                continue;
            }
            buckets
                .entry(format!("{scope:?} {values:?}"))
                .or_insert_with(|| (values, Vec::new()))
                .1
                .push(element);
        }
        for (values, elems) in buckets.into_values().filter(|(_, e)| e.len() > 1) {
            let d = Diagnostic {
                check: CheckKey::new(
                    CheckKind::KeyUnique,
                    SchemaRef::Key {
                        vertex: key.vertex.into(),
                        key: key.id.into(),
                    },
                ),
                class: CheckKind::KeyUnique.class(),
                requirement: Requirement::Required,
                subject: None,
                elements: elems.clone(),
                violation: Violation::DuplicateKey {
                    key: key.parts.iter().map(name).zip(values).collect(),
                    elems,
                },
                source: None,
            };
            out.push(compared(&d, &id));
        }
    }
    if references {
        out.extend(oracle_references(&refs, oracle));
    }
    out.sort();
    out
}

/// The `ReferenceResolves` diagnostics the oracle expects of `refs`.
fn oracle_references(refs: &[OracleRef], oracle: Oracle<'_>) -> Vec<String> {
    let mut out = Vec::new();
    let id = |r: &ViewRef| r.clone();
    let name = |e: &Edge| e.name.as_ref().map(ToString::to_string).unwrap_or_default();
    for r in refs {
        let check = CheckKey::new(
            CheckKind::ReferenceResolves,
            SchemaRef::Reference(r.id.into()),
        );
        let diagnostic = |subject: &ViewRef, elements: Vec<ViewRef>, violation| Diagnostic {
            check: check.clone(),
            class: CheckKind::ReferenceResolves.class(),
            requirement: r.requirement,
            subject: Some(subject.clone()),
            elements,
            violation,
            source: None,
        };
        for holder in oracle.anchored(r.holder) {
            let Some(value) = oracle.part(&holder, &r.part) else {
                if !r.nullable {
                    let d = diagnostic(
                        &holder,
                        vec![holder.clone()],
                        Violation::MissingValue {
                            field: Some(name(&r.part)),
                        },
                    );
                    out.push(compared(&d, &id));
                }
                continue;
            };
            let key = vec![value];
            // Under a key unique within the document, a holder's targets are
            // those in its own document, and a holder in none has none.
            let document = r.within_doc.then(|| oracle.document(&holder));
            let candidates: Vec<ViewRef> = oracle
                .anchored(r.target)
                .into_iter()
                .filter(|t| key_value(oracle, t, &r.target_key).as_ref() == Some(&key))
                .filter(|t| match &document {
                    None => true,
                    Some(None) => false,
                    Some(Some(doc)) => oracle.document(t).as_ref() == Some(doc),
                })
                .collect();
            let d = match candidates.len() {
                1 => continue,
                0 => diagnostic(
                    &holder,
                    vec![holder.clone()],
                    Violation::DanglingReference {
                        reference: r.id.into(),
                        key,
                    },
                ),
                _ => {
                    let mut elements = candidates.clone();
                    elements.push(holder.clone());
                    elements.sort();
                    elements.dedup();
                    diagnostic(
                        &holder,
                        elements,
                        Violation::AmbiguousReference {
                            reference: r.id.into(),
                            key,
                            candidates,
                        },
                    )
                }
            };
            out.push(compared(&d, &id));
        }
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// The `KeyUnique` and `ReferenceResolves` diagnostics of every
    /// shape are exactly those of a brute-force oracle, over composite keys,
    /// keys unique within a document, NULL key parts (left out, as SQL
    /// does), and dangling, ambiguous, strict and lenient references.
    /// References are checked on the shape the protocol writes instances
    /// in, which here is the instance's own.
    #[test]
    fn v11_keys_and_references_match_the_oracle(
        case in arb_validation_schema(),
        instance in arb_validated(),
    ) {
        let plan = plan(&case, instance.syntactic());
        let report = instance.validate(&plan, &unlimited(), &no_verdicts());
        let oracle = match &instance {
            Validated::Graph(g) => Oracle::Graph(g),
            Validated::Tree(t) => Oracle::Tree(t),
            Validated::Table(f) => Oracle::Table(f),
        };
        let id = |r: &ViewRef| r.clone();
        // Sorted vectors, so a duplicated diagnostic on either side fails.
        let mut found: Vec<String> = report
            .diagnostics
            .iter()
            .filter(|d| matches!(d.check.kind, CheckKind::KeyUnique | CheckKind::ReferenceResolves))
            .map(|d| compared(d, &id))
            .collect();
        found.sort();
        prop_assert_eq!(found, oracle_diagnostics(&case, oracle, true));
    }
}

/// The oracle above expects failures of every kind it models, in every
/// shape, over fixed draws, so the agreement is not vacuous.
#[test]
fn the_oracle_expects_every_key_and_reference_failure() {
    use proptest::strategy::ValueTree as _;
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
    let mut runner = TestRunner::new_with_rng(
        Config::default(),
        TestRng::from_seed(RngAlgorithm::ChaCha, &[13; 32]),
    );
    let strategy = (arb_validation_schema(), arb_validated());
    let mut seen: BTreeMap<(InstanceShapeKind, &'static str), usize> = BTreeMap::new();
    for _ in 0..3000 {
        let (case, instance) = strategy.new_tree(&mut runner).unwrap().current();
        let oracle = match &instance {
            Validated::Graph(g) => Oracle::Graph(g),
            Validated::Tree(t) => Oracle::Tree(t),
            Validated::Table(f) => Oracle::Table(f),
        };
        for expected in oracle_diagnostics(&case, oracle, true) {
            for kind in [
                "DuplicateKey",
                "DanglingReference",
                "AmbiguousReference",
                "MissingValue",
                "OutsideKeyScope",
            ] {
                if expected.contains(kind) {
                    *seen.entry((instance.syntactic(), kind)).or_default() += 1;
                }
            }
        }
    }
    for shape in [
        InstanceShapeKind::Graph,
        InstanceShapeKind::Tree,
        InstanceShapeKind::Table,
    ] {
        for kind in [
            "DuplicateKey",
            "DanglingReference",
            "AmbiguousReference",
            "MissingValue",
        ] {
            assert!(
                seen.get(&(shape, kind)).copied().unwrap_or(0) > 0,
                "no {kind} expected on a {shape:?}: {seen:?}"
            );
        }
    }
    // The scope boundary of a `Within` key: an element with no ancestor of
    // the scope vertex is outside the key and named by an advisory
    // observation. Tables have no ancestors, so the boundary is reached on
    // the hierarchical shapes.
    for shape in [InstanceShapeKind::Graph, InstanceShapeKind::Tree] {
        assert!(
            seen.get(&(shape, "OutsideKeyScope")).copied().unwrap_or(0) > 0,
            "no OutsideKeyScope expected on a {shape:?}: {seen:?}"
        );
    }
}

/// A graph of one `text` node per value, under a protocol named `name`
/// whose sort `sort` bounds the length of `text` in `unit`, validated with
/// the limit `limit`: the values that fail, in order.
fn too_long(
    name: &str,
    sort: &str,
    unit: panproto_schema::LengthUnit,
    limit: u64,
    values: &[&str],
) -> Vec<String> {
    let mut schema = panproto_schema::Schema::default();
    schema.vertices.insert(
        "text".into(),
        panproto_schema::Vertex {
            id: "text".into(),
            kind: "string".into(),
            nsid: None,
        },
    );
    schema.constraints.insert(
        "text".into(),
        vec![panproto_schema::Constraint {
            sort: sort.into(),
            value: limit.to_string(),
        }],
    );
    let protocol = panproto_schema::Protocol {
        name: name.into(),
        constraint_sorts: vec![sort.to_owned()],
        constraint_semantics: BTreeMap::from([(
            sort.to_owned(),
            panproto_schema::ConstraintSemantics::MaxLength(unit),
        )]),
        ..panproto_schema::Protocol::default()
    };
    let mut graph = GInstance::new();
    for (id, value) in (0_u32..).zip(values) {
        graph = graph
            .with_node(panproto_inst::Node::new(id, "text"))
            .with_value(id, Value::Str((*value).into()));
    }
    let report = panproto_inst::validation::validate_graph(&graph, &schema, &protocol);
    assert!(report.incomplete.is_empty(), "{report:#?}");
    report
        .diagnostics
        .iter()
        .map(|d| match &d.violation {
            Violation::ValueConstraint {
                value: Some(Value::Str(s)),
                ..
            } => s.clone(),
            other => panic!("not a length failure: {other:?}"),
        })
        .collect()
}

/// `\u{e9}`: one code point, two UTF-8 bytes, one grapheme. A flag: two
/// code points, eight bytes, one grapheme. `e` with a combining acute: two
/// code points, three bytes, one grapheme.
const SAMPLES: [&str; 4] = ["ab", "\u{e9}", "\u{1f1eb}\u{1f1f7}", "e\u{301}"];

/// atproto's `maxLength` counts UTF-8 bytes and its `maxGraphemes`
/// extended grapheme clusters, the units its constraint-semantics rows
/// give them.
#[test]
fn atproto_max_length_counts_utf8_bytes_and_max_graphemes_counts_clusters() {
    use panproto_schema::LengthUnit;
    assert_eq!(
        too_long("atproto", "maxLength", LengthUnit::Utf8Bytes, 2, &SAMPLES),
        vec!["\u{1f1eb}\u{1f1f7}".to_owned(), "e\u{301}".to_owned()]
    );
    assert_eq!(
        too_long("atproto", "maxLength", LengthUnit::Utf8Bytes, 8, &SAMPLES),
        Vec::<String>::new()
    );
    assert_eq!(
        too_long(
            "atproto",
            "maxGraphemes",
            LengthUnit::Graphemes,
            1,
            &SAMPLES
        ),
        vec!["ab".to_owned()]
    );
}

/// JSON Schema's `maxLength` counts code points, the unit its
/// constraint-semantics row gives it.
#[test]
fn json_schema_max_length_counts_code_points() {
    use panproto_schema::LengthUnit;
    assert_eq!(
        too_long("json-schema", "maxLength", LengthUnit::Chars, 1, &SAMPLES),
        vec![
            "ab".to_owned(),
            "\u{1f1eb}\u{1f1f7}".to_owned(),
            "e\u{301}".to_owned()
        ]
    );
    assert_eq!(
        too_long("json-schema", "maxLength", LengthUnit::Chars, 2, &SAMPLES),
        Vec::<String>::new()
    );
}

/// How many separate processes the reports are compared across.
const PROCESSES: usize = 8;

/// The plan digest, report canonical bytes and report digest of a fixed
/// corpus of generated cases of every shape, under a budget and with
/// external verdicts, in hex, one case per line.
fn report_dump() -> String {
    use proptest::strategy::ValueTree as _;
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
    use std::fmt::Write as _;
    let mut runner = TestRunner::new_with_rng(
        Config::default(),
        TestRng::from_seed(RngAlgorithm::ChaCha, &[17; 32]),
    );
    let strategy = (arb_validation_schema(), arb_validated());
    let hex = |bytes: &[u8]| {
        bytes.iter().fold(String::new(), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
    };
    let mut out = String::new();
    for i in 0..48_u64 {
        let (case, instance) = strategy.new_tree(&mut runner).unwrap().current();
        let plan = audited(&case, instance.syntactic());
        let verdicts = audit_verdicts(instance.elements());
        let budget = if i % 3 == 0 {
            ValidationBudget { max_reads: Some(i) }
        } else {
            unlimited()
        };
        let report = instance.validate(&plan, &budget, &verdicts);
        let _ = writeln!(
            out,
            "{} {} {}",
            plan.digest().to_hex(),
            hex(&report.canonical_bytes()),
            report.digest().to_hex()
        );
    }
    out
}

/// The child half of the determinism law: one process's dump, printed.
#[test]
fn one_process_report_dump() {
    if std::env::var_os("PP_VALIDATION_REPORT_DUMP").is_none() {
        return;
    }
    print!("<<<{}>>>", report_dump());
}

/// Validation reports, failure witnesses included, have the same
/// canonical bytes and digests in every process, whatever the process's
/// hash seed.
#[test]
fn validation_reports_are_identical_across_processes() {
    if std::env::var_os("PP_VALIDATION_REPORT_DUMP").is_some() {
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let here = report_dump();
    assert_eq!(here.lines().count(), 48);
    for _ in 0..PROCESSES {
        let output = std::process::Command::new(&exe)
            .args(["one_process_report_dump", "--exact", "--nocapture"])
            .env("PP_VALIDATION_REPORT_DUMP", "1")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let start = text.find("<<<").expect("the child printed its dump") + 3;
        let end = text.rfind(">>>").expect("the child closed its dump");
        assert_eq!(
            &text[start..end],
            here,
            "a child process wrote other reports"
        );
    }
}

/// A fault with several witnesses verifies with each of them, not only with
/// the one the validator reports: a node entered by two arcs, neither from
/// the parent the parent map records, is reported with the first arc's
/// parent, and the diagnostic naming the second arc's parent verifies too.
/// A diagnostic naming a parent no arc comes from does not.
#[test]
fn a_parent_mismatch_verifies_with_any_arc_into_the_node() {
    use panproto_inst::Node;
    use panproto_schema::{Protocol, Schema, Vertex};
    let next = validation_support::e_next();
    let mut schema = Schema::default();
    schema.vertices.insert(
        "item".into(),
        Vertex {
            id: "item".into(),
            kind: "object".into(),
            nsid: None,
        },
    );
    schema.edges.insert(next.clone(), next.kind.clone());
    let plan =
        ValidationPlan::compile(&schema, &Protocol::default(), &ValidationOptions::default());
    let nodes = (0..4).map(|i| (i, Node::new(i, "item"))).collect();
    let arcs = vec![
        (0, 1, next.clone()),
        (0, 2, next.clone()),
        (1, 3, next.clone()),
        (2, 3, next),
    ];
    let mut tree = WInstance::new(nodes, arcs, Vec::new(), 0, "item".into());
    tree.parent_map.insert(3, 0);
    let report = plan.validate_tree(&tree, &unlimited(), &no_verdicts());
    let reported: Vec<&Diagnostic> = report
        .diagnostics
        .iter()
        .filter(|d| d.check.kind == CheckKind::TreeParentMap)
        .collect();
    assert_eq!(reported.len(), 1, "{report:#?}");
    let mismatch = reported[0];
    assert_eq!(
        mismatch.violation,
        Violation::ParentMismatch {
            recorded: Some(0),
            actual: Some(1)
        }
    );
    assert!(mismatch.verify(&plan, &tree));
    let with_parent = |actual: u32| {
        let mut other = mismatch.clone();
        other.violation = Violation::ParentMismatch {
            recorded: Some(0),
            actual: Some(actual),
        };
        other.elements = vec![
            ViewRef::Node(3),
            ViewRef::Arc {
                parent: actual,
                child: 3,
            },
        ];
        other.elements.sort();
        other
    };
    assert!(with_parent(2).verify(&plan, &tree));
    assert!(!with_parent(0).verify(&plan, &tree));
}
