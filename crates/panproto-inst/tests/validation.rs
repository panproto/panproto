//! Whole-instance validation of trees, tables and graphs: the report
//! vocabulary, plan compilation, and the checks of each shape.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Debug;

use panproto_gat::{CheckStatus, Name};
use panproto_inst::validation::aggregate_status;
use panproto_inst::{
    CheckClass, CheckCoverage, CheckInstance, CheckKey, CheckKind, DeltaElementKind, Diagnostic,
    Digest, EdgeId, EndpointSide, ExternalCheckDecl, ExternalVerdict, ExternalVerdicts, FInstance,
    Fan, FanRecord, FanViolation, FieldPresence, GEdge, GInstance, IncompleteCheck,
    IncompleteReason, InstanceShapeKind, Node, NodeShape, ReportScope, Requirement, RowId,
    SchemaRef, SchemaSourceMap, SegmentId, SourceLocation, TriggerTable, ValidationBudget,
    ValidationOptions, ValidationPlan, ValidationReport, Value, ViewRef, Violation, WInstance,
    validate_graph, validate_table, validate_wtype,
};
use panproto_schema::{
    Constraint, ConstraintSemantics, Edge, HyperEdge, Key, KeyPart, KeyRole, KeyScope, LengthUnit,
    Protocol, Schema, Vertex,
};
use proptest::prelude::*;
use serde::Serialize;
use serde::de::DeserializeOwned;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Builders.
// ---------------------------------------------------------------------------

fn edge(src: &str, tgt: &str, kind: &str, name: Option<&str>) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: name.map(Name::from),
    }
}

fn prop(src: &str, tgt: &str, name: &str) -> Edge {
    edge(src, tgt, "prop", Some(name))
}

/// A schema built vertex by vertex and edge by edge, with its adjacency
/// indices kept up to date.
#[derive(Default)]
struct S(Schema);

impl S {
    fn vertex(mut self, id: &str, kind: &str) -> Self {
        self.0.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: kind.into(),
                nsid: None,
            },
        );
        self
    }

    fn edge(mut self, e: &Edge) -> Self {
        self.0.edges.insert(e.clone(), e.kind.clone());
        self.0
            .outgoing
            .entry(e.src.clone())
            .or_default()
            .push(e.clone());
        self.0
            .incoming
            .entry(e.tgt.clone())
            .or_default()
            .push(e.clone());
        self.0
            .between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
        self
    }

    fn constraint(mut self, vertex: &str, sort: &str, value: &str) -> Self {
        self.0
            .constraints
            .entry(vertex.into())
            .or_default()
            .push(Constraint {
                sort: sort.into(),
                value: value.into(),
            });
        self
    }

    fn required(mut self, vertex: &str, e: &Edge) -> Self {
        self.0
            .required
            .entry(vertex.into())
            .or_default()
            .push(e.clone());
        self
    }

    fn ordered(mut self, e: &Edge) -> Self {
        self.0.orderings.insert(e.clone(), 0);
        self
    }

    fn hyper_edge(mut self, id: &str, signature: &[(&str, &str)], parent: &str) -> Self {
        self.0.hyper_edges.insert(
            id.into(),
            HyperEdge {
                id: id.into(),
                kind: "rel".into(),
                signature: signature
                    .iter()
                    .map(|(l, v)| (Name::from(*l), Name::from(*v)))
                    .collect(),
                parent_label: parent.into(),
            },
        );
        self
    }

    fn key(mut self, vertex: &str, key: Key) -> Self {
        self.0.declarations.insert_key(vertex.into(), key);
        self
    }

    fn build(self) -> Schema {
        self.0
    }
}

/// A protocol giving each listed sort its meaning.
fn protocol(semantics: &[(&str, ConstraintSemantics)]) -> Protocol {
    Protocol {
        name: "test".into(),
        constraint_sorts: semantics.iter().map(|(s, _)| (*s).to_owned()).collect(),
        constraint_semantics: semantics
            .iter()
            .map(|(s, m)| ((*s).to_owned(), m.clone()))
            .collect(),
        ..Protocol::default()
    }
}

fn plan(schema: &Schema, protocol: &Protocol) -> ValidationPlan {
    ValidationPlan::compile(schema, protocol, &ValidationOptions::default())
}

fn unlimited() -> ValidationBudget {
    ValidationBudget::default()
}

fn none() -> ExternalVerdicts {
    ExternalVerdicts::default()
}

fn str_node(id: u32, anchor: &str, value: &str) -> Node {
    Node::new(id, anchor).with_value(FieldPresence::Present(Value::Str(value.into())))
}

/// The violations of the report's diagnostics of `kind`, with their
/// subjects.
fn of_kind(report: &ValidationReport, kind: CheckKind) -> Vec<(Option<ViewRef>, Violation)> {
    report
        .diagnostics
        .iter()
        .filter(|d| d.check.kind == kind)
        .map(|d| (d.subject.clone(), d.violation.clone()))
        .collect()
}

fn kinds(report: &ValidationReport) -> BTreeSet<CheckKind> {
    report.diagnostics.iter().map(|d| d.check.kind).collect()
}

/// Round-trip a value through JSON, the JSON object model (the shape
/// `pythonize` produces), named and positional `MessagePack`, and CBOR.
fn round_trip<T>(value: &T) -> TestResult
where
    T: Serialize + DeserializeOwned + PartialEq + Debug,
{
    let json = serde_json::to_string(value)?;
    assert_eq!(&serde_json::from_str::<T>(&json)?, value, "json");
    let dynamic = serde_json::to_value(value)?;
    assert_eq!(
        &serde_json::from_value::<T>(dynamic)?,
        value,
        "object model"
    );
    let named = rmp_serde::to_vec_named(value)?;
    assert_eq!(&rmp_serde::from_slice::<T>(&named)?, value, "named msgpack");
    let compact = rmp_serde::to_vec(value)?;
    assert_eq!(&rmp_serde::from_slice::<T>(&compact)?, value, "msgpack");
    let mut cbor = Vec::new();
    ciborium::into_writer(value, &mut cbor)?;
    assert_eq!(
        &ciborium::from_reader::<T, _>(cbor.as_slice())?,
        value,
        "cbor"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Vocabulary.
// ---------------------------------------------------------------------------

#[test]
fn check_class_serde_names_are_the_contract_names() -> TestResult {
    let names: Vec<String> = [
        CheckClass::Static,
        CheckClass::GraphLocal,
        CheckClass::MaintainedGlobal,
        CheckClass::BoundedExternal,
    ]
    .iter()
    .map(serde_json::to_string)
    .collect::<Result<_, _>>()?;
    assert_eq!(
        names,
        [
            r#""static""#,
            r#""graph_local""#,
            r#""maintained_global""#,
            r#""bounded_external""#
        ]
    );
    Ok(())
}

#[test]
fn requirement_defaults_to_required() -> TestResult {
    assert_eq!(Requirement::default(), Requirement::Required);
    assert_eq!(
        serde_json::to_string(&Requirement::Advisory)?,
        r#""advisory""#
    );
    Ok(())
}

const CONTRACT_KINDS: [(CheckKind, &str, CheckClass); 20] = [
    (
        CheckKind::SchemaWellFormed,
        "schema_well_formed",
        CheckClass::Static,
    ),
    (
        CheckKind::DeclarationInterpretable,
        "declaration_interpretable",
        CheckClass::Static,
    ),
    (CheckKind::NodeAnchor, "node_anchor", CheckClass::GraphLocal),
    (
        CheckKind::ReservedVocabulary,
        "reserved_vocabulary",
        CheckClass::GraphLocal,
    ),
    (
        CheckKind::EdgeDeclared,
        "edge_declared",
        CheckClass::GraphLocal,
    ),
    (
        CheckKind::EndpointCoherence,
        "endpoint_coherence",
        CheckClass::GraphLocal,
    ),
    (
        CheckKind::FanWellFormed,
        "fan_well_formed",
        CheckClass::GraphLocal,
    ),
    (
        CheckKind::RequiredEdge,
        "required_edge",
        CheckClass::GraphLocal,
    ),
    (CheckKind::OutDegree, "out_degree", CheckClass::GraphLocal),
    (
        CheckKind::PositionDistinct,
        "position_distinct",
        CheckClass::GraphLocal,
    ),
    (
        CheckKind::ValueConstraint,
        "value_constraint",
        CheckClass::GraphLocal,
    ),
    (
        CheckKind::RequiredValue,
        "required_value",
        CheckClass::GraphLocal,
    ),
    (CheckKind::Attribute, "attribute", CheckClass::GraphLocal),
    (CheckKind::TreeRoot, "tree_root", CheckClass::GraphLocal),
    (
        CheckKind::TreeParentMap,
        "tree_parent_map",
        CheckClass::GraphLocal,
    ),
    (
        CheckKind::KeyUnique,
        "key_unique",
        CheckClass::MaintainedGlobal,
    ),
    (
        CheckKind::ReferenceResolves,
        "reference_resolves",
        CheckClass::MaintainedGlobal,
    ),
    (CheckKind::Acyclic, "acyclic", CheckClass::MaintainedGlobal),
    (
        CheckKind::TreeReachable,
        "tree_reachable",
        CheckClass::MaintainedGlobal,
    ),
    (CheckKind::External, "external", CheckClass::BoundedExternal),
];

#[test]
fn check_kind_order_is_the_contract_order() -> TestResult {
    assert_eq!(CheckKind::ALL.len(), 20);
    for (i, (kind, name, _)) in CONTRACT_KINDS.iter().enumerate() {
        assert_eq!(CheckKind::ALL[i], *kind);
        assert_eq!(serde_json::to_string(kind)?, format!("{name:?}"));
        assert_eq!(kind.name(), *name);
        if i > 0 {
            assert!(
                CONTRACT_KINDS[i - 1].0 < *kind,
                "{kind:?} sorts after its predecessor"
            );
        }
    }
    Ok(())
}

#[test]
fn check_kind_class_is_the_contract_table() {
    for (kind, _, class) in CONTRACT_KINDS {
        assert_eq!(kind.class(), class, "{kind:?}");
    }
}

fn all_schema_refs() -> Vec<SchemaRef> {
    let e = prop("a", "b", "x");
    vec![
        SchemaRef::Schema,
        SchemaRef::Vertex("a".into()),
        SchemaRef::Edge(e.clone()),
        SchemaRef::HyperEdge("h".into()),
        SchemaRef::HyperLabel {
            hyper_edge: "h".into(),
            label: "l".into(),
        },
        SchemaRef::Required {
            vertex: "a".into(),
            edge: e.clone(),
        },
        SchemaRef::Ordering(e),
        SchemaRef::Constraint {
            vertex: "a".into(),
            constraint: Constraint {
                sort: "maxLength".into(),
                value: "3".into(),
            },
        },
        SchemaRef::Attribute {
            vertex: "a".into(),
            field: "f".into(),
        },
        SchemaRef::Key {
            vertex: "a".into(),
            key: "pk".into(),
        },
        SchemaRef::Reference("r".into()),
        SchemaRef::External("ext".into()),
    ]
}

#[test]
fn schema_ref_round_trips_every_variant_in_every_encoding() -> TestResult {
    let refs = all_schema_refs();
    assert_eq!(refs.len(), 12);
    for pair in refs.windows(2) {
        assert!(pair[0] < pair[1], "{:?} < {:?}", pair[0], pair[1]);
    }
    let tags: Vec<String> = refs
        .iter()
        .map(|r| {
            let v = serde_json::to_value(r)?;
            Ok(match v {
                serde_json::Value::String(s) => s,
                serde_json::Value::Object(m) => m.keys().next().cloned().unwrap_or_default(),
                _ => String::new(),
            })
        })
        .collect::<Result<_, serde_json::Error>>()?;
    assert_eq!(
        tags,
        [
            "schema",
            "vertex",
            "edge",
            "hyper_edge",
            "hyper_label",
            "required",
            "ordering",
            "constraint",
            "attribute",
            "key",
            "reference",
            "external"
        ]
    );
    for r in &refs {
        round_trip(r)?;
        let key = CheckKey::new(CheckKind::NodeAnchor, r.clone());
        round_trip(&key)?;
        round_trip(&CheckInstance {
            check: key,
            subject: Some(ViewRef::Node(1)),
        })?;
    }
    Ok(())
}

fn d(kind: CheckKind, requirement: Requirement, violation: Violation) -> Diagnostic {
    Diagnostic {
        check: CheckKey::new(kind, SchemaRef::Schema),
        class: kind.class(),
        requirement,
        subject: Some(ViewRef::Node(0)),
        elements: vec![ViewRef::Node(0)],
        violation,
        source: None,
    }
}

const fn i(kind: CheckKind, requirement: Requirement, reason: IncompleteReason) -> IncompleteCheck {
    IncompleteCheck {
        check: CheckKey::new(kind, SchemaRef::Schema),
        class: kind.class(),
        requirement,
        subject: None,
        reason,
    }
}

fn all_violations() -> Vec<Violation> {
    let e = prop("a", "b", "x");
    vec![
        Violation::UnknownAnchor { anchor: "g".into() },
        Violation::UndeclaredEdge { label: e.clone() },
        Violation::IncoherentEndpoint {
            side: EndpointSide::Tgt,
            expected: "b".into(),
            found: "c".into(),
        },
        Violation::MalformedFan {
            reason: FanViolation::UnknownLabel { label: "l".into() },
            fan: Some(FanRecord::of(&Fan::new("h", 0).with_child("l", 1))),
        },
        Violation::ReservedVocabulary,
        Violation::MissingRequiredEdge { edge: e.clone() },
        Violation::OutDegree {
            edge: Some(e),
            min: Some(1),
            max: None,
            found: 0,
            edges: vec![],
        },
        Violation::DuplicatePosition {
            position: 2,
            edges: vec![EdgeId(1), EdgeId(3)],
        },
        Violation::ValueConstraint {
            sort: "maxLength".into(),
            bound: "3".into(),
            value: Some(Value::Str("abcd".into())),
        },
        Violation::MissingValue {
            field: Some("id".into()),
        },
        Violation::UndeclaredAttribute { key: "k".into() },
        Violation::MistypedAttribute {
            key: "k".into(),
            expected: "string".into(),
            found: "int".into(),
        },
        Violation::DuplicateKey {
            key: vec![("id".into(), Value::Int(1))],
            elems: vec![ViewRef::Row(RowId(1)), ViewRef::Row(RowId(2))],
        },
        Violation::DanglingReference {
            reference: "r".into(),
            key: vec![Value::Int(9)],
        },
        Violation::AmbiguousReference {
            reference: "r".into(),
            key: vec![Value::Int(1)],
            candidates: vec![ViewRef::Node(1), ViewRef::Node(2)],
        },
        Violation::Cycle {
            scc: vec![ViewRef::Node(0), ViewRef::Node(1)],
            witness: vec![EdgeId(0), EdgeId(1)],
        },
        Violation::MissingRoot,
        Violation::Unreachable,
        Violation::ParentMismatch {
            recorded: Some(1),
            actual: None,
        },
        Violation::ExternalRejected {
            witness: Digest([7; 32]),
        },
    ]
}

#[test]
fn violation_serde_tags_are_the_contract_tags_in_order() -> TestResult {
    let tags: Vec<String> = all_violations()
        .iter()
        .map(|v| {
            Ok(match serde_json::to_value(v)? {
                serde_json::Value::String(s) => s,
                serde_json::Value::Object(m) => m.keys().next().cloned().unwrap_or_default(),
                _ => String::new(),
            })
        })
        .collect::<Result<_, serde_json::Error>>()?;
    assert_eq!(
        tags,
        [
            "unknown_anchor",
            "undeclared_edge",
            "incoherent_endpoint",
            "malformed_fan",
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
            "external_rejected"
        ]
    );
    assert_eq!(serde_json::to_string(&EndpointSide::Src)?, r#""src""#);
    for v in all_violations() {
        round_trip(&v)?;
    }
    Ok(())
}

#[test]
fn every_violation_renders_a_nonempty_message() {
    for v in all_violations() {
        let diagnostic = d(CheckKind::NodeAnchor, Requirement::Required, v.clone());
        assert!(!v.to_string().is_empty(), "{v:?}");
        let message = diagnostic.to_string();
        assert!(message.starts_with("node_anchor at node 0: "), "{message}");
        assert!(message.len() > "node_anchor at node 0: ".len(), "{message}");
    }
}

fn all_reasons() -> Vec<IncompleteReason> {
    vec![
        IncompleteReason::Budget {
            limit: 4,
            spent: 3,
            from: Some(ViewRef::Node(2)),
        },
        IncompleteReason::UnsupportedDeclaration { sort: "s".into() },
        IncompleteReason::IllFormedDeclaration { detail: "d".into() },
        IncompleteReason::MissingExternalWitness { name: "e".into() },
        IncompleteReason::NoValidatedBaseline,
        IncompleteReason::OutsideSegment {
            locality: panproto_inst::locality::Locality::Summary(
                panproto_inst::locality::SummaryKind::Reach,
            ),
        },
        IncompleteReason::Unsupported { detail: "u".into() },
    ]
}

#[test]
fn incomplete_reason_round_trips_every_variant() -> TestResult {
    let reasons = all_reasons();
    assert_eq!(reasons.len(), 7);
    for r in &reasons {
        round_trip(r)?;
        assert!(!r.to_string().is_empty());
        round_trip(&i(CheckKind::Acyclic, Requirement::Required, r.clone()))?;
    }
    Ok(())
}

#[test]
fn report_scope_round_trips_every_variant() -> TestResult {
    for scope in [
        ReportScope::Instance,
        ReportScope::Edit {
            baseline_seq: 4,
            baseline_status: CheckStatus::Incomplete,
        },
        ReportScope::DeltaLocal,
        ReportScope::Segment {
            segment: SegmentId(3),
        },
    ] {
        round_trip(&scope)?;
    }
    assert_eq!(
        serde_json::to_string(&ReportScope::Instance)?,
        r#""instance""#
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Reports and aggregation.
// ---------------------------------------------------------------------------

#[test]
fn status_is_passed_iff_no_required_failure_or_incomplete() {
    let fail = || {
        d(
            CheckKind::NodeAnchor,
            Requirement::Required,
            Violation::Unreachable,
        )
    };
    let fail_adv = || {
        d(
            CheckKind::NodeAnchor,
            Requirement::Advisory,
            Violation::Unreachable,
        )
    };
    let inc = || {
        i(
            CheckKind::External,
            Requirement::Required,
            IncompleteReason::NoValidatedBaseline,
        )
    };
    let inc_adv = || {
        i(
            CheckKind::External,
            Requirement::Advisory,
            IncompleteReason::NoValidatedBaseline,
        )
    };
    assert_eq!(aggregate_status(&[], &[]), CheckStatus::Passed);
    assert_eq!(aggregate_status(&[fail()], &[]), CheckStatus::Failed);
    assert_eq!(aggregate_status(&[], &[inc()]), CheckStatus::Incomplete);
    assert_eq!(aggregate_status(&[fail()], &[inc()]), CheckStatus::Failed);
    assert_eq!(
        aggregate_status(&[fail_adv()], &[inc_adv()]),
        CheckStatus::Passed
    );
    assert_eq!(
        aggregate_status(&[fail_adv()], &[inc()]),
        CheckStatus::Incomplete
    );
}

#[test]
fn advisory_entries_are_listed_and_never_change_the_status() {
    let report = ValidationReport::new(
        InstanceShapeKind::Graph,
        ReportScope::Instance,
        Digest([0; 32]),
        vec![d(
            CheckKind::NodeAnchor,
            Requirement::Advisory,
            Violation::Unreachable,
        )],
        vec![i(
            CheckKind::External,
            Requirement::Advisory,
            IncompleteReason::NoValidatedBaseline,
        )],
        BTreeMap::new(),
    );
    assert_eq!(report.status, CheckStatus::Passed);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.incomplete.len(), 1);
}

proptest! {
    #[test]
    fn v1_status_soundness_over_random_verdict_sets(
        diags in prop::collection::vec(any::<bool>(), 0..6),
        incs in prop::collection::vec(any::<bool>(), 0..6),
    ) {
        let req = |b: bool| if b { Requirement::Required } else { Requirement::Advisory };
        let diagnostics: Vec<Diagnostic> = diags
            .iter()
            .map(|b| d(CheckKind::OutDegree, req(*b), Violation::MissingRoot))
            .collect();
        let incomplete: Vec<IncompleteCheck> = incs
            .iter()
            .map(|b| i(CheckKind::Acyclic, req(*b), IncompleteReason::NoValidatedBaseline))
            .collect();
        let status = aggregate_status(&diagnostics, &incomplete);
        let any_failed = diags.iter().any(|b| *b);
        let any_incomplete = incs.iter().any(|b| *b);
        prop_assert_eq!(status == CheckStatus::Passed, !any_failed && !any_incomplete);
        prop_assert_eq!(status == CheckStatus::Failed, any_failed);
        prop_assert_eq!(status == CheckStatus::Incomplete, !any_failed && any_incomplete);
    }
}

#[test]
fn decoding_a_report_with_a_wrong_status_is_rejected() -> TestResult {
    let report = ValidationReport::new(
        InstanceShapeKind::Tree,
        ReportScope::Instance,
        Digest([1; 32]),
        vec![d(
            CheckKind::TreeRoot,
            Requirement::Required,
            Violation::MissingRoot,
        )],
        vec![],
        BTreeMap::new(),
    );
    assert_eq!(report.status, CheckStatus::Failed);
    round_trip(&report)?;
    let mut json = serde_json::to_value(&report)?;
    json["status"] = serde_json::Value::String("passed".into());
    let err = serde_json::from_value::<ValidationReport>(json).expect_err("a forged status");
    assert!(err.to_string().contains("does not match"), "{err}");
    Ok(())
}

#[test]
fn error_detail_validation_round_trips() -> TestResult {
    let report = validate_graph(
        &GInstance::new().with_node(Node::new(0, "ghost")),
        &S::default().vertex("a", "object").build(),
        &Protocol::default(),
    );
    let detail = panproto_inst::ErrorDetail::Validation { report };
    round_trip(&detail)?;
    let json = serde_json::to_value(&detail)?;
    assert!(json.get("validation").is_some(), "{json}");
    assert_eq!(json["validation"]["report"]["status"], "failed");
    Ok(())
}

#[derive(Debug, PartialEq, Serialize, serde::Deserialize)]
struct Maps {
    options: ValidationOptions,
    verdicts: ExternalVerdicts,
    sources: SchemaSourceMap,
}

#[test]
fn non_string_keyed_maps_round_trip_in_four_encodings() -> TestResult {
    let decl = SchemaRef::Constraint {
        vertex: "a".into(),
        constraint: Constraint {
            sort: "maxLength".into(),
            value: "3".into(),
        },
    };
    let location = SourceLocation {
        source: "schema.ncl".into(),
        start_byte: 3,
        end_byte: 9,
    };
    let sources = SchemaSourceMap(BTreeMap::from([(decl.clone(), location)]));
    let options = ValidationOptions {
        requirements: BTreeMap::from([(decl, Requirement::Advisory)]),
        external: BTreeMap::from([(
            Name::from("audit"),
            ExternalCheckDecl {
                name: "audit".into(),
                anchors: BTreeSet::from([Name::from("a")]),
                requirement: Requirement::Required,
            },
        )]),
        sources: sources.clone(),
    };
    let mut verdicts = ExternalVerdicts::default();
    verdicts.insert(
        "audit".into(),
        ViewRef::Pair {
            edge: prop("a", "b", "x"),
            src: RowId(1),
            tgt: RowId(2),
        },
        ExternalVerdict {
            passed: false,
            witness: Digest([3; 32]),
        },
    );
    // A plain map keyed by these types is not JSON.
    assert!(serde_json::to_string(&options.requirements).is_err());
    let maps = Maps {
        options,
        verdicts,
        sources,
    };
    round_trip(&maps)?;
    let schema = S::default()
        .vertex("a", "string")
        .constraint("a", "maxLength", "3")
        .build();
    let p = protocol(&[(
        "maxLength",
        ConstraintSemantics::MaxLength(LengthUnit::Chars),
    )]);
    let report = validate_graph(
        &GInstance::new().with_node(str_node(0, "a", "abcd")),
        &schema,
        &p,
    );
    assert!(!report.coverage.is_empty());
    round_trip(&report)?;
    Ok(())
}

#[test]
fn default_validation_options_are_empty() -> TestResult {
    let options = ValidationOptions::default();
    assert!(options.requirements.is_empty());
    assert!(options.external.is_empty());
    assert!(options.sources.0.is_empty());
    assert_eq!(
        serde_json::to_string(&options)?,
        r#"{"requirements":[],"external":{},"sources":[]}"#
    );
    Ok(())
}

#[test]
fn default_validation_budget_is_unlimited() {
    assert_eq!(ValidationBudget::default().max_reads, None);
}

/// A report's canonical bytes and digest, pinned: a change to the encoding
/// must bump `VALIDATION_REPORT_CONTEXT`.
#[test]
fn report_canonical_bytes_are_pinned() {
    let report = ValidationReport::new(
        InstanceShapeKind::Graph,
        ReportScope::Instance,
        Digest([0; 32]),
        vec![d(
            CheckKind::NodeAnchor,
            Requirement::Required,
            Violation::UnknownAnchor { anchor: "g".into() },
        )],
        vec![],
        BTreeMap::from([(
            CheckKey::new(CheckKind::NodeAnchor, SchemaRef::Schema),
            CheckCoverage {
                class: CheckClass::GraphLocal,
                requirement: Requirement::Required,
                instances: 1,
                passed: 0,
                failed: 1,
                incomplete: 0,
            },
        )]),
    );
    let bytes = report.canonical_bytes();
    let mut expected = vec![2u8, 0];
    expected.extend([0u8; 32]);
    expected.push(1); // status failed
    expected.extend(1u64.to_le_bytes()); // one diagnostic
    expected.extend([2, 0, 1, 0]); // node_anchor, schema, graph_local, required
    expected.extend([1, 0]); // subject Some(Node(..))
    expected.extend(0u32.to_le_bytes());
    expected.extend(1u64.to_le_bytes()); // one element
    expected.extend([0]);
    expected.extend(0u32.to_le_bytes());
    expected.push(0); // unknown_anchor
    expected.extend(1u64.to_le_bytes());
    expected.extend(b"g");
    expected.push(0); // no source
    expected.extend(0u64.to_le_bytes()); // no incomplete
    expected.extend(1u64.to_le_bytes()); // one coverage entry
    expected.extend([2, 0, 1, 0]);
    for n in [1u64, 0, 1, 0] {
        expected.extend(n.to_le_bytes());
    }
    assert_eq!(bytes, expected);
    assert_eq!(
        report.digest(),
        Digest::derive(panproto_inst::identity::VALIDATION_REPORT_CONTEXT, &bytes)
    );
    assert_eq!(
        report.digest().to_hex(),
        "503de6e0c67bcd80ebf98b1246fb5df500296a0f253baeb19be30a800d8e6639"
    );
}

// ---------------------------------------------------------------------------
// Plans.
// ---------------------------------------------------------------------------

/// A schema exercising most kinds: an object with a string, a self-edge, a
/// required edge, an ordered edge, a key, a hyper-edge and constraints.
fn rich_schema() -> Schema {
    let name = prop("person", "str", "name");
    let knows = edge("person", "person", "ref", Some("knows"));
    S::default()
        .vertex("person", "object")
        .vertex("str", "string")
        .edge(&name)
        .edge(&knows)
        .required("person", &name)
        .ordered(&knows)
        .constraint("str", "maxLength", "3")
        .constraint("person", "acyclic", "knows")
        .constraint("person", "maxOut", "4")
        .hyper_edge("rel", &[("arg", "person")], "head")
        .key(
            "person",
            Key {
                id: "pk".into(),
                parts: vec![KeyPart::Path(vec![name])],
                role: KeyRole::Primary,
                scope: KeyScope::Document,
            },
        )
        .build()
}

fn rich_protocol() -> Protocol {
    protocol(&[
        (
            "maxLength",
            ConstraintSemantics::MaxLength(LengthUnit::Chars),
        ),
        ("acyclic", ConstraintSemantics::Acyclic),
        ("maxOut", ConstraintSemantics::MaxOutDegree),
    ])
}

#[test]
fn a_plan_has_one_check_key_per_compiled_check() {
    let plan = plan(&rich_schema(), &rich_protocol());
    for (key, spec) in plan.checks() {
        assert_eq!(&spec.key, key);
        assert_eq!(spec.class, key.kind.class());
    }
    let keys: Vec<&CheckKey> = plan.checks().keys().collect();
    let unique: BTreeSet<&CheckKey> = keys.iter().copied().collect();
    assert_eq!(keys.len(), unique.len());
    let kinds: BTreeSet<CheckKind> = plan.checks().keys().map(|k| k.kind).collect();
    for kind in [
        CheckKind::NodeAnchor,
        CheckKind::RequiredEdge,
        CheckKind::OutDegree,
        CheckKind::PositionDistinct,
        CheckKind::ValueConstraint,
        CheckKind::RequiredValue,
        CheckKind::Attribute,
        CheckKind::KeyUnique,
        CheckKind::Acyclic,
    ] {
        assert!(kinds.contains(&kind), "{kind:?} compiled");
    }
    assert!(
        plan.static_findings().is_empty(),
        "{:?}",
        plan.static_findings()
    );
}

#[test]
fn check_kinds_compile_only_for_their_shapes() {
    use InstanceShapeKind::{Graph, Table, Tree};
    let mut p = rich_protocol();
    p.syntactic_shape = Tree;
    p.semantic_shape = Graph;
    let mut schema = rich_schema();
    let refers = prop("person", "str", "friend");
    schema.edges.insert(refers.clone(), "prop".into());
    schema
        .declarations
        .insert_reference(panproto_schema::Reference {
            id: "friend".into(),
            holder: "person".into(),
            source: panproto_schema::RefSource::KeyValue {
                parts: vec![vec![refers]],
            },
            target_vertex: "person".into(),
            target_key: Some("pk".into()),
            edge: Some(edge("person", "person", "ref", Some("knows"))),
            cardinality: panproto_schema::Cardinality::One,
            nullable: true,
            on_unresolved: panproto_schema::OnUnresolved::Error,
            mode: panproto_schema::ResolveMode::AddEdge,
        });
    let plan = plan(&schema, &p);
    for (key, spec) in plan.checks() {
        let shapes: BTreeSet<InstanceShapeKind> = spec.locality.keys().copied().collect();
        let expected: BTreeSet<InstanceShapeKind> = match (key.kind, &key.decl) {
            // Tree-only kinds, and references on this protocol's syntactic
            // shape.
            (
                CheckKind::TreeRoot
                | CheckKind::TreeParentMap
                | CheckKind::TreeReachable
                | CheckKind::ReservedVocabulary
                | CheckKind::ReferenceResolves,
                _,
            ) => BTreeSet::from([Tree]),
            // Graph-only kinds, and references on its semantic shape.
            (CheckKind::PositionDistinct | CheckKind::Acyclic, _)
            | (_, SchemaRef::Reference(_)) => BTreeSet::from([Graph]),
            _ => BTreeSet::from([Tree, Table, Graph]),
        };
        assert_eq!(shapes, expected, "{key:?}");
    }
    assert!(
        plan.checks()
            .keys()
            .any(|k| k.kind == CheckKind::ReferenceResolves)
    );
    // When the syntax already has the semantic shape, every reference check
    // compiles on that one shape, ReferenceResolves included.
    let mut same = p;
    same.syntactic_shape = Graph;
    let plan = ValidationPlan::compile(&schema, &same, &ValidationOptions::default());
    let reference_checks: Vec<(CheckKind, BTreeSet<InstanceShapeKind>)> = plan
        .checks()
        .values()
        .filter(|s| matches!(s.key.decl, SchemaRef::Reference(_)))
        .map(|s| (s.key.kind, s.locality.keys().copied().collect()))
        .collect();
    assert_eq!(
        reference_checks,
        vec![
            (CheckKind::OutDegree, BTreeSet::from([Graph])),
            (CheckKind::ReferenceResolves, BTreeSet::from([Graph])),
        ]
    );
}

#[test]
fn check_spec_locality_is_the_contract_table() {
    use panproto_inst::locality::{Locality, SummaryKind};
    let plan = plan(&rich_schema(), &rich_protocol());
    for (key, spec) in plan.checks() {
        let expected = match key.kind {
            CheckKind::TreeRoot => Locality::Summary(SummaryKind::Fold),
            CheckKind::KeyUnique | CheckKind::ReferenceResolves => {
                Locality::Summary(SummaryKind::SortedMerge)
            }
            CheckKind::Acyclic | CheckKind::TreeReachable => Locality::Summary(SummaryKind::Reach),
            _ => Locality::Local,
        };
        for locality in spec.locality.values() {
            assert_eq!(locality, &expected, "{key:?}");
        }
        assert_eq!(key.kind.locality(), expected);
    }
}

#[test]
fn plan_digest_changes_with_schema_semantics_or_options() {
    let schema = rich_schema();
    let p = rich_protocol();
    let base = plan(&schema, &p);
    assert_eq!(base.digest(), plan(&schema, &p).digest(), "deterministic");
    assert_eq!(
        base.schema_digest(),
        &panproto_inst::identity::schema_digest(&schema)
    );

    let other_schema = S::default().vertex("person", "object").build();
    assert_ne!(base.digest(), plan(&other_schema, &p).digest());

    let mut other_p = p.clone();
    other_p.constraint_semantics.insert(
        "maxLength".into(),
        ConstraintSemantics::MaxLength(LengthUnit::Utf8Bytes),
    );
    assert_ne!(base.digest(), plan(&schema, &other_p).digest());

    let options = ValidationOptions {
        requirements: BTreeMap::from([(SchemaRef::Schema, Requirement::Advisory)]),
        ..ValidationOptions::default()
    };
    assert_ne!(
        base.digest(),
        ValidationPlan::compile(&schema, &p, &options).digest()
    );
}

#[test]
fn plans_for_different_schemas_have_different_digests() {
    let p = Protocol::default();
    let a = S::default().vertex("a", "object").build();
    let b = S::default().vertex("b", "object").build();
    let (pa, pb) = (plan(&a, &p), plan(&b, &p));
    assert_ne!(pa.schema_digest(), pb.schema_digest());
    assert_ne!(pa.digest(), pb.digest());
    let report = pa.validate_graph(&GInstance::new(), &unlimited(), &none());
    assert_eq!(&report.plan, pa.digest());
}

/// A plan compiled once and shared (the cache a plan-taking caller keeps)
/// validates every instance exactly as the schema-taking call, which
/// compiles a fresh plan per call.
#[test]
fn a_cached_plan_validates_like_the_schema_taking_call() {
    let schema = S::default()
        .vertex("a", "string")
        .edge(&prop("a", "a", "next"))
        .constraint("a", "maxLength", "3")
        .build();
    let p = protocol(&[(
        "maxLength",
        ConstraintSemantics::MaxLength(LengthUnit::Chars),
    )]);
    let cached = std::sync::Arc::new(plan(&schema, &p));
    let shared = std::sync::Arc::clone(&cached);
    let instances = [
        GInstance::new()
            .with_node(str_node(0, "a", "ok"))
            .with_node(str_node(1, "a", "abc"))
            .with_edge(0, 1, prop("a", "a", "next")),
        GInstance::new()
            .with_node(str_node(0, "a", "much too long"))
            .with_node(str_node(1, "ghost", "x"))
            .with_edge(0, 1, prop("a", "a", "nope")),
    ];
    let mut statuses = Vec::new();
    for graph in &instances {
        let fresh = validate_graph(graph, &schema, &p);
        assert_eq!(cached.validate_graph(graph, &unlimited(), &none()), fresh);
        assert_eq!(shared.validate_graph(graph, &unlimited(), &none()), fresh);
        assert_eq!(&fresh.plan, cached.digest());
        statuses.push(fresh.status);
    }
    assert_eq!(statuses, [CheckStatus::Passed, CheckStatus::Failed]);
}

#[test]
fn a_source_map_does_not_change_the_schema_digest() -> TestResult {
    let schema = S::default()
        .vertex("a", "string")
        .constraint("a", "maxLength", "1")
        .build();
    let p = protocol(&[(
        "maxLength",
        ConstraintSemantics::MaxLength(LengthUnit::Chars),
    )]);
    let decl = SchemaRef::Constraint {
        vertex: "a".into(),
        constraint: Constraint {
            sort: "maxLength".into(),
            value: "1".into(),
        },
    };
    let location = SourceLocation {
        source: "a.ncl".into(),
        start_byte: 10,
        end_byte: 20,
    };
    let options = ValidationOptions {
        sources: SchemaSourceMap(BTreeMap::from([(decl, location.clone())])),
        ..ValidationOptions::default()
    };
    let with = ValidationPlan::compile(&schema, &p, &options);
    let without = plan(&schema, &p);
    assert_eq!(with.schema_digest(), without.schema_digest());
    assert_eq!(
        with.schema_digest(),
        &panproto_inst::identity::schema_digest(&schema)
    );
    let report = with.validate_graph(
        &GInstance::new().with_node(str_node(0, "a", "ab")),
        &unlimited(),
        &none(),
    );
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|d| d.check.kind == CheckKind::ValueConstraint)
        .expect("the length bound fails");
    assert_eq!(diagnostic.source.as_ref(), Some(&location));
    assert!(diagnostic.to_string().contains("a.ncl:10..20"));
    round_trip(&report)?;
    Ok(())
}

#[test]
fn trigger_table_is_the_rustdoc_table_restricted_to_compiled_kinds() {
    let plan = plan(&rich_schema(), &rich_protocol());
    let compiled: BTreeSet<CheckKind> = plan.checks().keys().map(|k| k.kind).collect();
    assert_eq!(plan.triggers(), &TriggerTable::restricted_to(&compiled));
    for change in DeltaElementKind::ALL {
        let expected: BTreeSet<CheckKind> = change
            .triggers()
            .iter()
            .copied()
            .filter(|k| compiled.contains(k))
            .collect();
        assert_eq!(
            plan.triggers().kinds(change).cloned().unwrap_or_default(),
            expected,
            "{change:?}"
        );
    }
    // External is not compiled for this schema, so no row lists it.
    assert!(!compiled.contains(&CheckKind::External));
    assert!(
        plan.triggers()
            .0
            .values()
            .all(|kinds| !kinds.contains(&CheckKind::External))
    );
    // The rows the rustdoc lists for two changes.
    assert_eq!(
        DeltaElementKind::AttrEdgePosition.triggers(),
        &[CheckKind::PositionDistinct, CheckKind::KeyUnique]
    );
    assert_eq!(
        DeltaElementKind::InsertFan.triggers(),
        &[CheckKind::FanWellFormed]
    );
}

// ---------------------------------------------------------------------------
// Static findings.
// ---------------------------------------------------------------------------

fn finding_reasons(plan: &ValidationPlan, kind: CheckKind) -> Vec<IncompleteReason> {
    plan.static_findings()
        .iter()
        .filter(|f| f.check.kind == kind)
        .map(|f| f.reason.clone())
        .collect()
}

#[test]
fn an_unregistered_sort_is_unsupported() {
    let schema = S::default()
        .vertex("a", "string")
        .constraint("a", "mystery", "1")
        .build();
    let plan = plan(&schema, &Protocol::default());
    let key = CheckKey::new(
        CheckKind::ValueConstraint,
        SchemaRef::Constraint {
            vertex: "a".into(),
            constraint: Constraint {
                sort: "mystery".into(),
                value: "1".into(),
            },
        },
    );
    let report = plan.validate_graph(
        &GInstance::new()
            .with_node(str_node(0, "a", "x"))
            .with_node(str_node(1, "a", "y")),
        &unlimited(),
        &none(),
    );
    assert_eq!(report.status, CheckStatus::Incomplete);
    let reason = IncompleteReason::UnsupportedDeclaration {
        sort: "mystery".into(),
    };
    assert!(
        report
            .incomplete
            .iter()
            .any(|i| i.check == key && i.subject.is_none() && i.reason == reason),
        "{:?}",
        report.incomplete
    );
    assert!(
        report
            .incomplete
            .iter()
            .any(|i| { i.check.kind == CheckKind::DeclarationInterpretable && i.reason == reason })
    );
    let cov = report.coverage[&key];
    assert_eq!((cov.instances, cov.incomplete, cov.passed), (2, 2, 0));
}

#[test]
fn max_length_abc_is_ill_formed() {
    let schema = S::default()
        .vertex("a", "string")
        .constraint("a", "maxLength", "abc")
        .build();
    let p = protocol(&[(
        "maxLength",
        ConstraintSemantics::MaxLength(LengthUnit::Chars),
    )]);
    let plan = plan(&schema, &p);
    let reasons = finding_reasons(&plan, CheckKind::ValueConstraint);
    assert!(
        matches!(reasons.as_slice(), [IncompleteReason::IllFormedDeclaration { detail }] if detail.contains("abc")),
        "{reasons:?}"
    );
    let report = plan.validate_graph(&GInstance::new(), &unlimited(), &none());
    assert_eq!(report.status, CheckStatus::Incomplete);
}

#[test]
fn a_pattern_with_lookahead_is_ill_formed() {
    let schema = S::default()
        .vertex("a", "string")
        .constraint("a", "pattern", "^(?=x)x$")
        .build();
    let p = protocol(&[("pattern", ConstraintSemantics::Pattern)]);
    let plan = plan(&schema, &p);
    let reasons = finding_reasons(&plan, CheckKind::ValueConstraint);
    assert!(
        matches!(reasons.as_slice(), [IncompleteReason::IllFormedDeclaration { detail }] if detail.contains("does not compile")),
        "{reasons:?}"
    );
    let report = plan.validate_graph(
        &GInstance::new().with_node(str_node(0, "a", "x")),
        &unlimited(),
        &none(),
    );
    assert_eq!(report.status, CheckStatus::Incomplete);
    assert!(!report.incomplete.is_empty());
}

#[test]
fn an_unlisted_format_is_unsupported() {
    let schema = S::default()
        .vertex("a", "string")
        .vertex("b", "string")
        .constraint("a", "format", "carrier-pigeon")
        .constraint("b", "format", "uuid")
        .build();
    let p = protocol(&[("format", ConstraintSemantics::Format("json-schema".into()))]);
    let plan = plan(&schema, &p);
    assert_eq!(
        finding_reasons(&plan, CheckKind::ValueConstraint),
        vec![IncompleteReason::UnsupportedDeclaration {
            sort: "format".into()
        }]
    );
    let report = plan.validate_graph(
        &GInstance::new()
            .with_node(str_node(0, "b", "123e4567-e89b-12d3-a456-426614174000"))
            .with_node(str_node(1, "b", "not-a-uuid")),
        &unlimited(),
        &none(),
    );
    assert_eq!(
        of_kind(&report, CheckKind::ValueConstraint)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<Vec<_>>(),
        vec![Some(ViewRef::Node(1))]
    );
}

/// The fixed set of formats is public, and each format it lists is
/// evaluated: a constraint naming one compiles with no finding.
#[test]
fn the_listed_formats_are_public_and_each_is_evaluated() {
    use panproto_inst::validation::ValueFormat;
    let names: Vec<&str> = ValueFormat::ALL.iter().map(|f| f.name()).collect();
    assert_eq!(
        names,
        [
            "date-time",
            "date",
            "time",
            "uri",
            "uuid",
            "ipv4",
            "ipv6",
            "did"
        ]
    );
    for format in ValueFormat::ALL {
        assert_eq!(ValueFormat::named(format.name()), Some(format));
        let schema = S::default()
            .vertex("a", "string")
            .constraint("a", "format", format.name())
            .build();
        let p = protocol(&[("format", ConstraintSemantics::Format("json-schema".into()))]);
        let plan = plan(&schema, &p);
        assert_eq!(plan.static_findings(), &[], "{format:?}");
        assert!(
            plan.checks()
                .keys()
                .any(|k| k.kind == CheckKind::ValueConstraint),
            "{format:?}"
        );
    }
    assert_eq!(ValueFormat::named("datetime"), Some(ValueFormat::DateTime));
    assert_eq!(ValueFormat::named("carrier-pigeon"), None);
}

#[test]
fn annotation_and_value_parameter_compile_to_no_check() {
    let schema = S::default()
        .vertex("a", "string")
        .constraint("a", "description", "anything")
        .constraint("a", "precision", "10")
        .build();
    let p = protocol(&[
        ("description", ConstraintSemantics::Annotation),
        ("precision", ConstraintSemantics::ValueParameter),
    ]);
    let plan = plan(&schema, &p);
    assert!(
        plan.checks()
            .keys()
            .all(|k| !matches!(k.decl, SchemaRef::Constraint { .. })),
        "{:?}",
        plan.checks().keys().collect::<Vec<_>>()
    );
    assert!(plan.static_findings().is_empty());
}

#[test]
fn schema_well_formed_reports_p4_key_defects() {
    // A key whose path does not start at the keyed vertex.
    let schema = S::default()
        .vertex("a", "object")
        .vertex("s", "string")
        .key(
            "a",
            Key {
                id: "pk".into(),
                parts: vec![KeyPart::Path(vec![prop("s", "a", "nope")])],
                role: KeyRole::Unique,
                scope: KeyScope::Document,
            },
        )
        .build();
    let plan = plan(&schema, &Protocol::default());
    assert!(
        plan.static_findings()
            .iter()
            .any(|f| f.check.kind == CheckKind::SchemaWellFormed)
    );
    let dependent = CheckKey::new(
        CheckKind::KeyUnique,
        SchemaRef::Key {
            vertex: "a".into(),
            key: "pk".into(),
        },
    );
    assert!(
        plan.static_findings().iter().any(|f| f.check == dependent),
        "{:?}",
        plan.static_findings()
    );
    let report = plan.validate_graph(&GInstance::new(), &unlimited(), &none());
    assert_eq!(report.status, CheckStatus::Incomplete);
}

#[test]
fn declaration_interpretable_reports_each_listed_defect() {
    let ghost = prop("a", "ghost", "g");
    let schema = S::default()
        .vertex("a", "object")
        // A constraint sort with no meaning.
        .constraint("a", "mystery", "1")
        // A value that does not read under its meaning.
        .constraint("a", "maxOut", "many")
        // An acyclic constraint naming no self-edge.
        .constraint("a", "acyclic", "parent")
        // A required edge and an ordered edge the schema does not declare.
        .required("a", &ghost)
        .ordered(&ghost)
        // A hyper-edge labelling a vertex that does not exist.
        .hyper_edge("h", &[("x", "nowhere")], "a")
        .build();
    let p = protocol(&[
        ("maxOut", ConstraintSemantics::MaxOutDegree),
        ("acyclic", ConstraintSemantics::Acyclic),
    ]);
    let plan = plan(&schema, &p);
    let dependents: BTreeSet<CheckKind> = plan
        .static_findings()
        .iter()
        .map(|f| f.check.kind)
        .collect();
    for kind in [
        CheckKind::DeclarationInterpretable,
        CheckKind::ValueConstraint,
        CheckKind::OutDegree,
        CheckKind::Acyclic,
        CheckKind::RequiredEdge,
        CheckKind::PositionDistinct,
        CheckKind::FanWellFormed,
    ] {
        assert!(dependents.contains(&kind), "{kind:?} in {dependents:?}");
    }
    let interpretable = plan
        .static_findings()
        .iter()
        .filter(|f| f.check.kind == CheckKind::DeclarationInterpretable)
        .count();
    assert_eq!(interpretable, 6, "{:?}", plan.static_findings());
}

#[test]
fn a_sql_table_signature_must_equal_its_columns() {
    let id = prop("t", "t.id", "id");
    let mut schema = S::default()
        .vertex("t", "table")
        .vertex("t.id", "integer")
        .vertex("t.name", "string")
        .edge(&id)
        .hyper_edge("he_0", &[("id", "t.id"), ("name", "t.name")], "t")
        .build();
    if let Some(h) = schema.hyper_edges.get_mut("he_0") {
        h.kind = "table".into();
    }
    let mut p = Protocol {
        name: "sql".into(),
        ..Protocol::default()
    };
    let findings =
        |p: &Protocol, s: &Schema| finding_reasons(&plan(s, p), CheckKind::FanWellFormed).len();
    assert_eq!(findings(&p, &schema), 1, "a declared label with no column");
    let name = prop("t", "t.name", "name");
    let fixed = S(schema.clone()).edge(&name).build();
    assert_eq!(findings(&p, &fixed), 0);
    p.name = "other".into();
    assert_eq!(findings(&p, &schema), 0, "the rule is SQL's");
}

#[test]
fn static_findings_appear_in_every_report_of_the_plan() {
    let schema = S::default()
        .vertex("a", "string")
        .constraint("a", "mystery", "1")
        .build();
    let plan = plan(&schema, &Protocol::default());
    assert!(!plan.static_findings().is_empty());
    let tree = WInstance::new(
        HashMap::from([(0, str_node(0, "a", "x"))]),
        vec![],
        vec![],
        0,
        "a".into(),
    );
    let reports = [
        plan.validate_graph(&GInstance::new(), &unlimited(), &none()),
        plan.validate_tree(&tree, &unlimited(), &none()),
        plan.validate_table(&FInstance::new(), &unlimited(), &none()),
    ];
    for report in reports {
        for finding in plan.static_findings() {
            assert!(report.incomplete.contains(finding), "{finding:?}");
        }
        assert_ne!(report.status, CheckStatus::Passed);
    }
}

#[test]
fn a_schema_problem_is_incomplete_never_failed() {
    // Every node is fine; only the declaration is broken.
    let schema = S::default()
        .vertex("a", "string")
        .constraint("a", "maxLength", "abc")
        .build();
    let p = protocol(&[(
        "maxLength",
        ConstraintSemantics::MaxLength(LengthUnit::Chars),
    )]);
    let report = validate_graph(
        &GInstance::new().with_node(str_node(0, "a", "toolong")),
        &schema,
        &p,
    );
    assert_eq!(report.status, CheckStatus::Incomplete);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    // Advisory requirements do not hide a required static finding.
    assert!(
        report
            .incomplete
            .iter()
            .any(|i| i.requirement == Requirement::Required)
    );
}

// ---------------------------------------------------------------------------
// Constraint semantics.
// ---------------------------------------------------------------------------

#[test]
fn value_constraints_apply_to_node_values_and_table_cells() {
    let name = prop("user", "user.name", "name");
    let schema = S::default()
        .vertex("user", "table")
        .vertex("user.name", "string")
        .edge(&name)
        .constraint("user.name", "maxLength", "3")
        .build();
    let p = protocol(&[(
        "maxLength",
        ConstraintSemantics::MaxLength(LengthUnit::Chars),
    )]);
    // Graph: the value of a node anchored at the constrained vertex.
    let graph = GInstance::new()
        .with_node(Node::new(0, "user"))
        .with_node(str_node(1, "user.name", "ada"))
        .with_node(str_node(2, "user.name", "grace"))
        .with_edge(0, 1, name);
    let report = validate_graph(&graph, &schema, &p);
    assert_eq!(
        of_kind(&report, CheckKind::ValueConstraint),
        vec![(
            Some(ViewRef::Node(2)),
            Violation::ValueConstraint {
                sort: "maxLength".into(),
                bound: "3".into(),
                value: Some(Value::Str("grace".into())),
            }
        )]
    );
    // Table: the cell keyed by the prop edge's name.
    let mut table = FInstance::new();
    table
        .insert_row(
            "user",
            RowId(0),
            HashMap::from([("name".to_owned(), Value::Str("ada".into()))]),
        )
        .unwrap();
    table
        .insert_row(
            "user",
            RowId(1),
            HashMap::from([("name".to_owned(), Value::Str("grace".into()))]),
        )
        .unwrap();
    let report = validate_table(&table, &schema, &p);
    assert_eq!(
        of_kind(&report, CheckKind::ValueConstraint)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<Vec<_>>(),
        vec![Some(ViewRef::Row(RowId(1)))]
    );
}

#[test]
fn length_units_count_chars_bytes_and_graphemes() {
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    for (unit, limit, passes) in [
        (LengthUnit::Graphemes, "1", true),
        (LengthUnit::Chars, "4", false),
        (LengthUnit::Chars, "5", true),
        (LengthUnit::Utf8Bytes, "17", false),
        (LengthUnit::Utf8Bytes, "18", true),
    ] {
        let schema = S::default()
            .vertex("a", "string")
            .constraint("a", "maxLength", limit)
            .build();
        let p = protocol(&[("maxLength", ConstraintSemantics::MaxLength(unit.clone()))]);
        let report = validate_graph(
            &GInstance::new().with_node(str_node(0, "a", family)),
            &schema,
            &p,
        );
        assert_eq!(report.passed(), passes, "{unit:?} {limit}: {report:?}");
    }
}

#[test]
fn degree_bounds_count_non_fan_out_edges_and_fk_pairs() {
    let item = edge("list", "list", "item", Some("next"));
    let fk = edge("order", "customer", "fk", Some("customer"));
    let schema = S::default()
        .vertex("list", "object")
        .vertex("order", "table")
        .vertex("customer", "table")
        .edge(&item)
        .edge(&fk)
        .constraint("list", "maxItems", "1")
        .constraint("order", "maxItems", "1")
        .hyper_edge("rel", &[("arg", "list")], "head")
        .build();
    let p = protocol(&[("maxItems", ConstraintSemantics::MaxOutDegree)]);
    // Node 0 has two out-edges labelled `next` and a fan edge into it from
    // a fan node; fan edges leave the fan node, so they never count.
    let graph = GInstance::new()
        .with_node(Node::new(0, "list"))
        .with_node(Node::new(1, "list"))
        .with_node(Node::new(2, "list"))
        .with_edge(0, 1, item.clone())
        .with_edge(0, 2, item);
    let graph = panproto_inst::reify_fans(&graph, &[Fan::new("rel", 1).with_child("arg", 0)], None)
        .unwrap()
        // An edge of a reserved kind out of node 0 is not a fan edge of node 0's
        // own; it is reported by FanWellFormed and never counted.
        .with_edge(0, 1, edge("list", "list", "hyper_label", Some("arg")));
    let report = validate_graph(&graph, &schema, &p);
    assert!(kinds(&report).contains(&CheckKind::FanWellFormed));
    let degree = of_kind(&report, CheckKind::OutDegree);
    assert_eq!(degree.len(), 1, "{report:?}");
    assert!(matches!(
        &degree[0],
        (
            Some(ViewRef::Node(0)),
            Violation::OutDegree {
                found: 2,
                max: Some(1),
                min: None,
                edge: None,
                ..
            }
        )
    ));
    // Table rows: the foreign-key pairs out of the row.
    let mut table = FInstance::new();
    table
        .insert_row("customer", RowId(0), HashMap::new())
        .unwrap();
    table
        .insert_row("customer", RowId(1), HashMap::new())
        .unwrap();
    table.insert_row("order", RowId(2), HashMap::new()).unwrap();
    table
        .insert_foreign_key(fk.clone(), RowId(2), RowId(0))
        .unwrap();
    table.insert_foreign_key(fk, RowId(2), RowId(1)).unwrap();
    let report = validate_table(&table, &schema, &p);
    assert!(matches!(
        of_kind(&report, CheckKind::OutDegree).as_slice(),
        [(
            Some(ViewRef::Row(RowId(2))),
            Violation::OutDegree { found: 2, .. }
        )]
    ));
}

#[test]
fn acyclic_rejects_two_cycles_long_cycles_and_self_loops() {
    let parent = edge("n", "n", "ref", Some("parent"));
    let other = edge("n", "n", "ref", Some("other"));
    let schema = S::default()
        .vertex("n", "object")
        .edge(&parent)
        .edge(&other)
        .constraint("n", "acyclic", "parent")
        .build();
    let p = protocol(&[("acyclic", ConstraintSemantics::Acyclic)]);
    let nodes = |g: GInstance, n: u32| (0..n).fold(g, |g, i| g.with_node(Node::new(i, "n")));
    // Legal: a chain, and a cycle of the undeclared label.
    let chain = nodes(GInstance::new(), 4)
        .with_edge(0, 1, parent.clone())
        .with_edge(1, 2, parent.clone())
        .with_edge(2, 0, other);
    assert!(validate_graph(&chain, &schema, &p).passed());
    // A 2-cycle, a 3-cycle and a self-loop.
    let cyclic = nodes(GInstance::new(), 6)
        .with_edge(0, 1, parent.clone())
        .with_edge(1, 0, parent.clone())
        .with_edge(2, 3, parent.clone())
        .with_edge(3, 4, parent.clone())
        .with_edge(4, 2, parent.clone())
        .with_edge(5, 5, parent.clone());
    let report = validate_graph(&cyclic, &schema, &p);
    let cycles: Vec<Violation> = of_kind(&report, CheckKind::Acyclic)
        .into_iter()
        .map(|(subject, v)| {
            assert_eq!(subject, None);
            v
        })
        .collect();
    assert_eq!(cycles.len(), 3, "{cycles:?}");
    for v in &cycles {
        let Violation::Cycle { scc, witness } = v else {
            panic!("{v:?}");
        };
        // The witness is a closed walk through the component.
        let edges: Vec<&GEdge> = witness.iter().map(|e| cyclic.edge(*e).unwrap()).collect();
        for pair in edges.windows(2) {
            assert_eq!(pair[0].tgt, pair[1].src);
        }
        assert_eq!(edges[0].src, edges[edges.len() - 1].tgt);
        assert!(edges.iter().all(|e| e.edge == parent));
        assert!(edges.iter().all(|e| scc.contains(&ViewRef::Node(e.src))));
    }
    let sizes: Vec<usize> = cycles
        .iter()
        .map(|v| match v {
            Violation::Cycle { scc, .. } => scc.len(),
            _ => 0,
        })
        .collect();
    assert_eq!(sizes, vec![2, 3, 1]);
}

#[test]
fn bounds_compare_exact_values_exactly() {
    use panproto_inst::value::{DecimalValue, Temporal};
    let schema = |sort: &str, bound: &str| {
        S::default()
            .vertex("a", "number")
            .constraint("a", sort, bound)
            .build()
    };
    let p = protocol(&[
        ("maximum", ConstraintSemantics::Maximum),
        ("minimum", ConstraintSemantics::Minimum),
    ]);
    let check = |sort: &str, bound: &str, value: Value| {
        validate_graph(
            &GInstance::new()
                .with_node(Node::new(0, "a").with_value(FieldPresence::Present(value))),
            &schema(sort, bound),
            &p,
        )
    };
    let dec = |s: &str| Value::Decimal(DecimalValue::parse(s).unwrap());
    // Decimals compare numerically, past the precision of a float.
    assert!(check("maximum", "0.30000000000000000001", dec("0.3")).passed());
    assert!(!check("maximum", "0.3", dec("0.30000000000000000001")).passed());
    assert!(check("maximum", "1.50", dec("1.5")).passed());
    // Big integers compare numerically.
    let big = Value::parse_integer("123456789012345678901234567890").unwrap();
    assert!(!check("maximum", "123456789012345678901234567889", big.clone()).passed());
    assert!(check("minimum", "123456789012345678901234567889", big).passed());
    assert!(check("minimum", "-1", Value::Int(-1)).passed());
    // Temporal values compare on the instant when both carry offsets.
    let t = |s: &str| Value::Temporal(Temporal::parse(s).unwrap());
    assert!(
        check(
            "maximum",
            "2024-01-01T00:00:00Z",
            t("2024-01-01T01:00:00+02:00")
        )
        .passed()
    );
    assert!(
        !check(
            "maximum",
            "2024-01-01T00:00:00Z",
            t("2024-01-01T01:00:00+00:00")
        )
        .passed()
    );
    // A bound whose kind cannot be compared with the value is ill formed,
    // and the instance is not decided.
    let local = check("maximum", "2024-01-01T00:00:00Z", t("2024-01-01T00:00:00"));
    assert_eq!(local.status, CheckStatus::Incomplete, "{local:?}");
    assert!(matches!(
        local.incomplete.as_slice(),
        [IncompleteCheck {
            reason: IncompleteReason::IllFormedDeclaration { .. },
            ..
        }]
    ));
    let text = check("maximum", "3", Value::Str("2".into()));
    assert_eq!(text.status, CheckStatus::Incomplete);
}

#[test]
fn enum_and_const_members_compare_by_kind() {
    let p = protocol(&[
        ("enum", ConstraintSemantics::Enum),
        ("const", ConstraintSemantics::Const),
    ]);
    let check = |sort: &str, bound: &str, value: Value| {
        let schema = S::default()
            .vertex("a", "string")
            .constraint("a", sort, bound)
            .build();
        validate_graph(
            &GInstance::new()
                .with_node(Node::new(0, "a").with_value(FieldPresence::Present(value))),
            &schema,
            &p,
        )
        .passed()
    };
    assert!(check("enum", "red,green", Value::Str("green".into())));
    assert!(!check("enum", "red,green", Value::Str("blue".into())));
    assert!(check("enum", r#"["a,b","c"]"#, Value::Str("a,b".into())));
    assert!(check("enum", "1,2.50", Value::Int(1)));
    assert!(check("enum", "1,2.50", Value::Float(2.5)));
    assert!(check("const", "true", Value::Bool(true)));
    assert!(!check("const", "x", Value::Str("y".into())));
}

// ---------------------------------------------------------------------------
// Keys and references.
// ---------------------------------------------------------------------------

fn keyed_table_schema() -> Schema {
    let id = prop("film", "film.id", "id");
    let lang = prop("film", "film.lang", "lang");
    S::default()
        .vertex("film", "table")
        .vertex("film.id", "integer")
        .vertex("film.lang", "string")
        .edge(&id)
        .edge(&lang)
        .key(
            "film",
            Key {
                id: "pk".into(),
                parts: vec![KeyPart::Path(vec![id]), KeyPart::Path(vec![lang])],
                role: KeyRole::Primary,
                scope: KeyScope::Document,
            },
        )
        .build()
}

fn film(table: &mut FInstance, row: u32, id: Option<i64>, lang: &str) {
    let mut cells = HashMap::from([("lang".to_owned(), Value::Str(lang.into()))]);
    if let Some(id) = id {
        cells.insert("id".to_owned(), Value::Int(id));
    }
    table.insert_row("film", RowId(row), cells).unwrap();
}

#[test]
fn keys_compile_to_key_unique_and_primary_parts_to_required_value() {
    let schema = keyed_table_schema();
    let plan = plan(&schema, &Protocol::default());
    let decl = SchemaRef::Key {
        vertex: "film".into(),
        key: "pk".into(),
    };
    assert!(
        plan.checks()
            .contains_key(&CheckKey::new(CheckKind::KeyUnique, decl.clone()))
    );
    assert!(
        plan.checks()
            .contains_key(&CheckKey::new(CheckKind::RequiredValue, decl.clone()))
    );
    let mut table = FInstance::new();
    film(&mut table, 0, Some(1), "en");
    film(&mut table, 1, Some(1), "en");
    film(&mut table, 2, Some(1), "fr");
    // A NULL key part takes no part in uniqueness, but the primary key
    // requires it.
    film(&mut table, 3, None, "en");
    film(&mut table, 4, None, "en");
    let report = plan.validate_table(&table, &unlimited(), &none());
    assert_eq!(
        of_kind(&report, CheckKind::KeyUnique),
        vec![(
            None,
            Violation::DuplicateKey {
                key: vec![
                    ("id".into(), Value::Int(1)),
                    ("lang".into(), Value::Str("en".into()))
                ],
                elems: vec![ViewRef::Row(RowId(0)), ViewRef::Row(RowId(1))],
            }
        )]
    );
    assert_eq!(
        of_kind(&report, CheckKind::RequiredValue),
        vec![
            (
                Some(ViewRef::Row(RowId(3))),
                Violation::MissingValue {
                    field: Some("id".into())
                }
            ),
            (
                Some(ViewRef::Row(RowId(4))),
                Violation::MissingValue {
                    field: Some("id".into())
                }
            ),
        ]
    );
    // A unique key does not require its parts.
    let mut unique = schema;
    if let Some(keys) = unique.declarations.keys.get_mut("film") {
        keys[0].role = KeyRole::Unique;
    }
    let plan =
        ValidationPlan::compile(&unique, &Protocol::default(), &ValidationOptions::default());
    assert!(
        !plan
            .checks()
            .contains_key(&CheckKey::new(CheckKind::RequiredValue, decl))
    );
}

#[test]
fn a_key_scoped_within_an_ancestor_is_unique_per_ancestor() {
    let item = prop("list", "item", "item");
    let name = prop("item", "name", "name");
    let schema = S::default()
        .vertex("list", "object")
        .vertex("item", "object")
        .vertex("name", "string")
        .edge(&item)
        .edge(&name)
        .key(
            "item",
            Key {
                id: "k".into(),
                parts: vec![KeyPart::Path(vec![name.clone()])],
                role: KeyRole::Unique,
                scope: KeyScope::Within("list".into()),
            },
        )
        .build();
    let tree = |second_list_name: &str| {
        let nodes = HashMap::from([
            (0, Node::new(0, "list")),
            (1, Node::new(1, "list")),
            (2, Node::new(2, "item")),
            (3, Node::new(3, "item")),
            (4, str_node(4, "name", "a")),
            (5, str_node(5, "name", second_list_name)),
        ]);
        let mut graph = GInstance::new();
        for (_, n) in nodes {
            graph = graph.with_node(n);
        }
        graph
            .with_edge(0, 2, item.clone())
            .with_edge(1, 3, item.clone())
            .with_edge(2, 4, name.clone())
            .with_edge(3, 5, name.clone())
    };
    let p = Protocol::default();
    assert!(
        validate_graph(&tree("a"), &schema, &p).passed(),
        "one per list"
    );
    let same_list = tree("a").with_edge(0, 3, item.clone());
    // Item 3 now has two list ancestors at one distance; the least is list 0.
    let report = validate_graph(&same_list, &schema, &p);
    assert_eq!(
        of_kind(&report, CheckKind::KeyUnique).len(),
        1,
        "{report:?}"
    );
}

fn referencing_schema(on_unresolved: panproto_schema::OnUnresolved) -> Schema {
    let id = prop("person", "person.id", "id");
    let boss = prop("person", "person.boss", "boss");
    let manages = edge("person", "person", "ref", Some("boss"));
    let mut schema = S::default()
        .vertex("person", "object")
        .vertex("person.id", "integer")
        .vertex("person.boss", "integer")
        .edge(&id)
        .edge(&boss)
        .edge(&manages)
        .key(
            "person",
            Key {
                id: "pk".into(),
                parts: vec![KeyPart::Path(vec![id])],
                role: KeyRole::Identifier,
                scope: KeyScope::Document,
            },
        )
        .build();
    schema
        .declarations
        .insert_reference(panproto_schema::Reference {
            id: "boss".into(),
            holder: "person".into(),
            source: panproto_schema::RefSource::KeyValue {
                parts: vec![vec![boss]],
            },
            target_vertex: "person".into(),
            target_key: Some("pk".into()),
            edge: Some(manages),
            cardinality: panproto_schema::Cardinality::One,
            nullable: false,
            on_unresolved,
            mode: panproto_schema::ResolveMode::AddEdge,
        });
    schema
}

fn syntactic_protocol() -> Protocol {
    Protocol {
        syntactic_shape: InstanceShapeKind::Tree,
        semantic_shape: InstanceShapeKind::Graph,
        ..Protocol::default()
    }
}

/// A tree of people under a root: each person holds an `id` and a `boss`
/// child.
fn people(bosses: &[(i64, Option<i64>)]) -> WInstance {
    let id = prop("person", "person.id", "id");
    let boss = prop("person", "person.boss", "boss");
    let root = edge("person", "person", "item", Some("member"));
    let mut nodes = HashMap::from([(0, Node::new(0, "person"))]);
    let mut arcs = Vec::new();
    let mut next = 1;
    for (pid, b) in bosses {
        let person = next;
        nodes.insert(person, Node::new(person, "person"));
        nodes.insert(
            person + 1,
            Node::new(person + 1, "person.id").with_value(FieldPresence::Present(Value::Int(*pid))),
        );
        arcs.push((0, person, root.clone()));
        arcs.push((person, person + 1, id.clone()));
        if let Some(b) = b {
            nodes.insert(
                person + 2,
                Node::new(person + 2, "person.boss")
                    .with_value(FieldPresence::Present(Value::Int(*b))),
            );
            arcs.push((person, person + 2, boss.clone()));
        }
        next += 3;
    }
    WInstance::new(nodes, arcs, vec![], 0, "person".into())
}

#[test]
fn references_compile_to_reference_resolves_with_the_declared_requirement() {
    use panproto_schema::OnUnresolved;
    let p = syntactic_protocol();
    let key = CheckKey::new(
        CheckKind::ReferenceResolves,
        SchemaRef::Reference("boss".into()),
    );
    let strict = plan(&referencing_schema(OnUnresolved::Error), &p);
    let lenient = plan(&referencing_schema(OnUnresolved::Complement), &p);
    assert_eq!(strict.checks()[&key].requirement, Requirement::Required);
    assert_eq!(lenient.checks()[&key].requirement, Requirement::Advisory);
    // Person 10 names boss 10 (itself), person 20 names 99, which nobody is.
    let root = people(&[(10, Some(10)), (20, Some(99))]);
    let strict_report = strict.validate_tree(&root, &unlimited(), &none());
    let dangling: Vec<(Option<ViewRef>, Violation)> =
        of_kind(&strict_report, CheckKind::ReferenceResolves)
            .into_iter()
            .filter(|(_, v)| matches!(v, Violation::DanglingReference { .. }))
            .collect();
    assert_eq!(
        dangling,
        vec![(
            Some(ViewRef::Node(4)),
            Violation::DanglingReference {
                reference: "boss".into(),
                key: vec![Value::Int(99)],
            }
        )]
    );
    assert_eq!(strict_report.status, CheckStatus::Failed);
    let lenient_report = lenient.validate_tree(&root, &unlimited(), &none());
    assert!(
        lenient_report
            .diagnostics
            .iter()
            .filter(|d| d.check.kind == CheckKind::ReferenceResolves)
            .all(|d| d.requirement == Requirement::Advisory)
    );
    // Two people share id 10: the reference is ambiguous.
    let ambiguous = people(&[(10, Some(10)), (10, None)]);
    let report = strict.validate_tree(&ambiguous, &unlimited(), &none());
    assert!(report.diagnostics.iter().any(|d| matches!(
        &d.violation,
        Violation::AmbiguousReference { candidates, .. } if candidates.len() == 2
    )));
}

#[test]
fn semantic_references_compile_to_degree_and_required_edge_checks() {
    use panproto_schema::OnUnresolved;
    let p = syntactic_protocol();
    let schema = referencing_schema(OnUnresolved::Error);
    let plan = plan(&schema, &p);
    let decl = SchemaRef::Reference("boss".into());
    let degree = &plan.checks()[&CheckKey::new(CheckKind::OutDegree, decl.clone())];
    let required = &plan.checks()[&CheckKey::new(CheckKind::RequiredEdge, decl)];
    for spec in [degree, required] {
        assert_eq!(
            spec.locality.keys().copied().collect::<Vec<_>>(),
            vec![InstanceShapeKind::Graph]
        );
    }
    let manages = edge("person", "person", "ref", Some("boss"));
    let graph = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(Node::new(1, "person"))
        .with_node(Node::new(2, "person"))
        .with_edge(0, 1, manages.clone())
        .with_edge(0, 2, manages.clone())
        .with_edge(1, 1, manages);
    let report = plan.validate_graph(&graph, &unlimited(), &none());
    assert!(matches!(
        of_kind(&report, CheckKind::OutDegree).as_slice(),
        [(
            Some(ViewRef::Node(0)),
            Violation::OutDegree {
                found: 2,
                max: Some(1),
                ..
            }
        )]
    ));
    assert_eq!(
        of_kind(&report, CheckKind::RequiredEdge)
            .into_iter()
            .map(|(s, _)| s)
            .collect::<Vec<_>>(),
        vec![Some(ViewRef::Node(2))]
    );
}

// ---------------------------------------------------------------------------
// Graphs.
// ---------------------------------------------------------------------------

/// The probe: a person and an organization joined by `worksAt`, a name
/// bounded to three characters.
fn probe_schema() -> Schema {
    let works_at = edge("person", "org", "ref", Some("worksAt"));
    let name = prop("person", "name", "name");
    S::default()
        .vertex("person", "object")
        .vertex("org", "object")
        .vertex("name", "string")
        .edge(&works_at)
        .edge(&name)
        .required("person", &works_at)
        .constraint("name", "maxLength", "3")
        .build()
}

fn probe_protocol() -> Protocol {
    protocol(&[(
        "maxLength",
        ConstraintSemantics::MaxLength(LengthUnit::Chars),
    )])
}

#[test]
fn the_probe_instance_fails_with_five_diagnostics() {
    let works_at = edge("person", "org", "ref", Some("worksAt"));
    let graph = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(Node::new(1, "person"))
        .with_node(Node::new(2, "ghost"))
        .with_node(str_node(3, "name", "Alexandra"))
        .with_node(Node::new(4, "org"))
        // A label the schema does not declare.
        .with_edge(0, 4, edge("person", "org", "ref", Some("owns")))
        // person to person over `worksAt`, whose target is `org`.
        .with_edge_id(EdgeId(1), 0, 1, works_at)
        .with_edge_id(EdgeId(2), 0, 3, prop("person", "name", "name"));
    let report = validate_graph(&graph, &probe_schema(), &probe_protocol());
    assert_eq!(report.status, CheckStatus::Failed);
    let found: Vec<(CheckKind, Option<ViewRef>)> = report
        .diagnostics
        .iter()
        .map(|d| (d.check.kind, d.subject.clone()))
        .collect();
    assert_eq!(
        found,
        vec![
            (CheckKind::NodeAnchor, Some(ViewRef::Node(2))),
            (CheckKind::EdgeDeclared, Some(ViewRef::Edge(EdgeId(0)))),
            (CheckKind::EndpointCoherence, Some(ViewRef::Edge(EdgeId(1)))),
            (CheckKind::RequiredEdge, Some(ViewRef::Node(1))),
            (CheckKind::ValueConstraint, Some(ViewRef::Node(3))),
        ]
    );
    assert!(matches!(
        &report.diagnostics[2].violation,
        Violation::IncoherentEndpoint { side: EndpointSide::Tgt, expected, found }
            if expected == "org" && found == "person"
    ));
    assert_eq!(
        report.diagnostics[3].check.decl,
        SchemaRef::Required {
            vertex: "person".into(),
            edge: edge("person", "org", "ref", Some("worksAt")),
        }
    );
}

#[test]
fn an_undeclared_edge_label_is_reported_by_validate_graph() {
    let graph = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(Node::new(1, "org"))
        .with_edge(0, 1, edge("person", "org", "ref", Some("owns")));
    let report = validate_graph(&graph, &probe_schema(), &probe_protocol());
    assert!(of_kind(&report, CheckKind::EdgeDeclared).contains(&(
        Some(ViewRef::Edge(EdgeId(0))),
        Violation::UndeclaredEdge {
            label: edge("person", "org", "ref", Some("owns"))
        }
    )));
}

#[test]
fn an_unknown_anchor_suppresses_value_checks_but_not_edge_checks() {
    let schema = S::default()
        .vertex("a", "string")
        .edge(&prop("a", "a", "self"))
        .constraint("a", "maxLength", "1")
        .build();
    let p = protocol(&[(
        "maxLength",
        ConstraintSemantics::MaxLength(LengthUnit::Chars),
    )]);
    let graph = GInstance::new()
        .with_node(str_node(0, "ghost", "much too long"))
        .with_node(str_node(1, "a", "x"))
        .with_edge(0, 1, prop("a", "a", "self"));
    let report = validate_graph(&graph, &schema, &p);
    assert_eq!(
        kinds(&report),
        BTreeSet::from([CheckKind::NodeAnchor, CheckKind::EndpointCoherence])
    );
    assert!(of_kind(&report, CheckKind::EndpointCoherence).contains(&(
        Some(ViewRef::Edge(EdgeId(0))),
        Violation::IncoherentEndpoint {
            side: EndpointSide::Src,
            expected: "a".into(),
            found: "ghost".into(),
        }
    )));
}

#[test]
fn an_untyped_element_is_an_unknown_anchor() {
    let schema = S::default().vertex("a", "object").build();
    let report = validate_graph(
        &GInstance::new().with_node(Node::new(0, "")),
        &schema,
        &Protocol::default(),
    );
    assert_eq!(
        of_kind(&report, CheckKind::NodeAnchor),
        vec![(
            Some(ViewRef::Node(0)),
            Violation::UnknownAnchor { anchor: "".into() }
        )]
    );
}

fn fan_schema() -> Schema {
    S::default()
        .vertex("sent", "object")
        .vertex("word", "object")
        .hyper_edge("rel", &[("arg", "word")], "head")
        .build()
}

#[test]
fn an_unknown_fan_label_needs_a_schema() {
    let graph = GInstance::new()
        .with_node(Node::new(0, "sent"))
        .with_node(Node::new(1, "word"));
    let graph =
        panproto_inst::reify_fans(&graph, &[Fan::new("rel", 0).with_child("theme", 1)], None)
            .unwrap();
    // Without a schema the label cannot be checked.
    assert!(panproto_inst::unreify_fans(&graph).is_ok());
    let refused = panproto_inst::unreify_fans_with_schema(&graph, &fan_schema());
    assert!(matches!(
        refused,
        Err(panproto_inst::InstError::MalformedFan {
            node: 2,
            reason: FanViolation::UnknownLabel { .. }
        })
    ));
    let report = validate_graph(&graph, &fan_schema(), &Protocol::default());
    assert_eq!(
        of_kind(&report, CheckKind::FanWellFormed),
        vec![(
            Some(ViewRef::Node(2)),
            Violation::MalformedFan {
                reason: FanViolation::UnknownLabel {
                    label: "theme".into()
                },
                fan: None,
            }
        )]
    );
}

#[test]
fn fan_edges_are_checked_against_hyper_edges_not_schema_edges() {
    let graph = GInstance::new()
        .with_node(Node::new(0, "sent"))
        .with_node(Node::new(1, "word"));
    let graph = panproto_inst::reify_fans(
        &graph,
        &[Fan::new("rel", 0).with_child("arg", 1)],
        Some(&fan_schema()),
    )
    .unwrap();
    // The fan's edges are labelled with no schema edge, and its node is
    // anchored at a hyper-edge, not a vertex: none of that is reported.
    let report = validate_graph(&graph, &fan_schema(), &Protocol::default());
    assert!(report.passed(), "{report:?}");
    let edge_checks = report.coverage[&CheckKey::new(CheckKind::EdgeDeclared, SchemaRef::Schema)];
    assert_eq!(edge_checks.instances, 0);
    let fan_checks = report.coverage[&CheckKey::new(CheckKind::FanWellFormed, SchemaRef::Schema)];
    assert_eq!((fan_checks.instances, fan_checks.passed), (1, 1));
    // A fan node anchored at an undeclared hyper-edge is a malformed fan,
    // never an unknown anchor: `NodeAnchor` does not apply to fan nodes.
    let mut other = fan_schema();
    other.hyper_edges.clear();
    let report = validate_graph(&graph, &other, &Protocol::default());
    assert_eq!(
        of_kind(&report, CheckKind::FanWellFormed),
        vec![(
            Some(ViewRef::Node(2)),
            Violation::MalformedFan {
                reason: FanViolation::UnknownHyperEdge {
                    hyper_edge: "rel".into()
                },
                fan: None,
            }
        )]
    );
    assert!(of_kind(&report, CheckKind::NodeAnchor).is_empty());
}

#[test]
fn graph_fans_are_checked_by_fan_well_formed_only() {
    let schema = fan_schema();
    let p = Protocol::default();
    let fan_node = Node::new(2, "rel").with_shape(NodeShape::HyperEdge);
    let parent = edge("rel", "sent", "hyper_parent", None);
    let arg = edge("rel", "word", "hyper_label", Some("arg"));
    let base = || {
        GInstance::new()
            .with_node(Node::new(0, "sent"))
            .with_node(Node::new(1, "word"))
            .with_node(fan_node.clone())
    };
    let cases: Vec<(GInstance, FanViolation)> = vec![
        (
            base().with_edge(2, 1, arg.clone()),
            FanViolation::MissingParent,
        ),
        (
            base()
                .with_edge(2, 0, parent.clone())
                .with_edge(2, 0, parent.clone()),
            FanViolation::MultipleParents {
                edges: vec![EdgeId(0), EdgeId(1)],
            },
        ),
        (
            base()
                .with_edge(2, 0, parent.clone())
                .with_edge(2, 1, arg.clone())
                .with_edge(2, 1, arg.clone()),
            FanViolation::DuplicateLabel {
                label: "arg".into(),
                edges: vec![EdgeId(1), EdgeId(2)],
            },
        ),
        (
            base().with_edge(2, 0, parent.clone()).with_edge(
                2,
                1,
                edge("rel", "word", "hyper_label", Some("nope")),
            ),
            FanViolation::UnknownLabel {
                label: "nope".into(),
            },
        ),
        (
            base().with_edge(2, 0, parent.clone()).with_edge(0, 1, arg),
            FanViolation::ReservedKindOutsideFan { edge: EdgeId(1) },
        ),
        (
            base()
                .with_edge(2, 0, parent)
                .with_edge(1, 2, prop("word", "rel", "x")),
            FanViolation::ForeignIncidentEdge { edge: EdgeId(1) },
        ),
    ];
    for (graph, expected) in cases {
        let report = validate_graph(&graph, &schema, &p);
        let fans: Vec<Violation> = of_kind(&report, CheckKind::FanWellFormed)
            .into_iter()
            .map(|(_, v)| v)
            .collect();
        assert!(
            fans.contains(&Violation::MalformedFan {
                reason: expected.clone(),
                fan: None,
            }),
            "{expected:?} in {fans:?}"
        );
        // The diagnostic lists the edges the violation names.
        let d = report
            .diagnostics
            .iter()
            .find(|d| d.check.kind == CheckKind::FanWellFormed)
            .unwrap();
        let mut sorted = d.elements.clone();
        sorted.sort();
        assert_eq!(sorted, d.elements);
    }
}

/// A fan edge is an edge of a reserved kind and nothing else (F): an edge of
/// a foreign kind incident to a fan node is a non-fan edge, so it is checked
/// for being declared and for its endpoints, whichever way it points, besides
/// breaking its fan.
#[test]
fn a_foreign_edge_at_a_fan_node_is_a_non_fan_edge_in_either_direction() {
    let schema = fan_schema();
    let p = Protocol::default();
    let fan_node = Node::new(2, "rel").with_shape(NodeShape::HyperEdge);
    let parent = edge("rel", "sent", "hyper_parent", None);
    let base = || {
        GInstance::new()
            .with_node(Node::new(0, "sent"))
            .with_node(Node::new(1, "word"))
            .with_node(fan_node.clone())
            .with_edge(2, 0, parent.clone())
    };
    let foreign = prop("sent", "word", "undeclared");
    let out_edge = base().with_edge(2, 1, foreign.clone());
    let in_edge = base().with_edge(1, 2, foreign.clone());
    let cases = [
        (out_edge, EndpointSide::Src, "sent", "rel"),
        (in_edge, EndpointSide::Src, "sent", "word"),
    ];
    for (graph, side, expected, found) in cases {
        let report = validate_graph(&graph, &schema, &p);
        let subject = Some(ViewRef::Edge(EdgeId(1)));
        assert_eq!(
            of_kind(&report, CheckKind::EdgeDeclared),
            vec![(
                subject.clone(),
                Violation::UndeclaredEdge {
                    label: foreign.clone()
                }
            )],
            "{report:?}"
        );
        let coherence = of_kind(&report, CheckKind::EndpointCoherence);
        assert_eq!(
            coherence.first(),
            Some(&(
                subject,
                Violation::IncoherentEndpoint {
                    side,
                    expected: expected.into(),
                    found: found.into(),
                }
            )),
            "{report:?}"
        );
        assert_eq!(
            of_kind(&report, CheckKind::FanWellFormed)
                .into_iter()
                .map(|(_, v)| v)
                .collect::<Vec<_>>(),
            vec![Violation::MalformedFan {
                reason: FanViolation::ForeignIncidentEdge { edge: EdgeId(1) },
                fan: None,
            }]
        );
    }
}

/// A graph fan node breaking the fan form several ways reports each of them,
/// and every malformed fan node is reported, not only the first.
#[test]
fn validate_graph_reports_every_fan_violation() {
    let arg = edge("rel", "word", "hyper_label", Some("arg"));
    let graph = GInstance::new()
        .with_node(Node::new(0, "sent"))
        .with_node(Node::new(1, "word"))
        .with_node(Node::new(2, "rel").with_shape(NodeShape::HyperEdge))
        .with_node(Node::new(3, "rel").with_shape(NodeShape::HyperEdge))
        // Node 2: no parent, `arg` twice, an undeclared label.
        .with_edge(2, 1, arg.clone())
        .with_edge(2, 1, arg)
        .with_edge(2, 1, edge("rel", "word", "hyper_label", Some("nope")))
        // Node 3: a parent, and an edge into it.
        .with_edge(3, 0, edge("rel", "sent", "hyper_parent", None))
        .with_edge(1, 3, prop("word", "rel", "x"));
    let report = validate_graph(&graph, &fan_schema(), &Protocol::default());
    assert_eq!(report.status, CheckStatus::Failed);
    let fan = |node: u32, reason: FanViolation| {
        (
            Some(ViewRef::Node(node)),
            Violation::MalformedFan { reason, fan: None },
        )
    };
    assert_eq!(
        of_kind(&report, CheckKind::FanWellFormed),
        vec![
            // Sorted by elements: the two naming the node alone (in tag
            // order), then the one naming its edges too.
            fan(2, FanViolation::MissingParent),
            fan(
                2,
                FanViolation::UnknownLabel {
                    label: "nope".into()
                }
            ),
            fan(
                2,
                FanViolation::DuplicateLabel {
                    label: "arg".into(),
                    edges: vec![EdgeId(0), EdgeId(1)],
                }
            ),
            fan(3, FanViolation::ForeignIncidentEdge { edge: EdgeId(4) }),
        ]
    );
    // The first violation of each node is the one unreifying reports.
    assert!(matches!(
        panproto_inst::unreify_fans_with_schema(&graph, &fan_schema()),
        Err(panproto_inst::InstError::MalformedFan {
            node: 2,
            reason: FanViolation::MissingParent
        })
    ));
}

#[test]
fn parallel_edges_and_self_loops_count_toward_degree_and_keys() {
    let next = edge("n", "n", "ref", Some("next"));
    let schema = S::default()
        .vertex("n", "object")
        .edge(&next)
        .constraint("n", "maxOut", "2")
        .build();
    let p = protocol(&[("maxOut", ConstraintSemantics::MaxOutDegree)]);
    let two = GInstance::new()
        .with_node(Node::new(0, "n"))
        .with_node(Node::new(1, "n"))
        .with_edge(0, 1, next.clone())
        .with_edge(0, 1, next.clone());
    let report = validate_graph(&two, &schema, &p);
    assert!(
        report.passed(),
        "parallel edges and self-loops are legal: {report:?}"
    );
    let three = two.with_edge(0, 0, next);
    let report = validate_graph(&three, &schema, &p);
    assert!(matches!(
        of_kind(&report, CheckKind::OutDegree).as_slice(),
        [(Some(ViewRef::Node(0)), Violation::OutDegree { found: 3, edges, .. })] if edges.len() == 3
    ));
}

#[test]
fn duplicate_positions_are_reported_and_none_is_allowed() {
    let item = edge("list", "x", "item", Some("items"));
    let schema = S::default()
        .vertex("list", "array")
        .vertex("x", "string")
        .edge(&item)
        .ordered(&item)
        .build();
    let p = Protocol::default();
    let graph = |positions: &[Option<u32>]| {
        let mut g = GInstance::new().with_node(Node::new(0, "list"));
        for (i, pos) in positions.iter().enumerate() {
            let n = u32::try_from(i).unwrap() + 1;
            g = g.with_node(Node::new(n, "x"));
            g.insert_edge(
                EdgeId(n),
                GEdge::new(0, n, item.clone()).with_position(*pos),
            )
            .unwrap();
        }
        g
    };
    assert!(validate_graph(&graph(&[None, None, Some(0), Some(1)]), &schema, &p).passed());
    let report = validate_graph(&graph(&[Some(0), Some(1), Some(1), None]), &schema, &p);
    assert_eq!(
        of_kind(&report, CheckKind::PositionDistinct),
        vec![(
            Some(ViewRef::Node(0)),
            Violation::DuplicatePosition {
                position: 1,
                edges: vec![EdgeId(2), EdgeId(3)],
            }
        )]
    );
}

#[test]
fn dangling_endpoints_are_rejected_by_construction_and_decode() {
    let mut graph = GInstance::new().with_node(Node::new(0, "a"));
    assert!(matches!(
        graph.insert_edge(EdgeId(0), GEdge::new(0, 9, prop("a", "a", "x"))),
        Err(panproto_inst::InstError::DanglingEdge { .. })
    ));
    let valid = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "a"))
        .with_edge(0, 1, prop("a", "a", "x"));
    let mut json = serde_json::to_value(&valid).unwrap();
    json["edges"]["0"]["tgt"] = serde_json::json!(9);
    let decoded = serde_json::from_value::<GInstance>(json);
    let err = decoded.expect_err("a dangling edge does not decode");
    assert!(
        err.to_string()
            .contains("names node 9, which is not in the instance"),
        "{err}"
    );
    let mut table = FInstance::new();
    table.insert_row("a", RowId(0), HashMap::new()).unwrap();
    assert!(matches!(
        table.insert_foreign_key(prop("a", "a", "x"), RowId(0), RowId(9)),
        Err(panproto_inst::InstError::DanglingForeignKey { .. })
    ));
}

#[test]
fn whole_instance_checks_have_no_subject() {
    let schema = rich_schema();
    let p = rich_protocol();
    let knows = edge("person", "person", "ref", Some("knows"));
    let graph = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_edge(0, 0, knows);
    let report = validate_graph(&graph, &schema, &p);
    let acyclic = report
        .diagnostics
        .iter()
        .find(|d| d.check.kind == CheckKind::Acyclic)
        .expect("a self-loop is a cycle");
    assert_eq!(acyclic.subject, None);
    let tree = WInstance::new(HashMap::new(), vec![], vec![], 0, "person".into());
    let report = validate_wtype(&tree, &schema, &p);
    let root = report
        .diagnostics
        .iter()
        .find(|d| d.check.kind == CheckKind::TreeRoot)
        .expect("an empty tree has no root");
    assert_eq!(root.subject, None);
    assert_eq!(root.violation, Violation::MissingRoot);
}

// ---------------------------------------------------------------------------
// Tables.
// ---------------------------------------------------------------------------

#[test]
fn tables_report_unknown_tables_and_undeclared_fk_edges() {
    let fk = edge("order", "customer", "fk", Some("customer"));
    let schema = S::default()
        .vertex("order", "table")
        .vertex("customer", "table")
        .edge(&fk)
        .build();
    let ghost_fk = edge("order", "customer", "fk", Some("payer"));
    let mut table = FInstance::new();
    table
        .insert_row("customer", RowId(0), HashMap::new())
        .unwrap();
    table.insert_row("order", RowId(1), HashMap::new()).unwrap();
    table
        .insert_row("invoice", RowId(2), HashMap::new())
        .unwrap();
    table.insert_foreign_key(fk, RowId(1), RowId(0)).unwrap();
    table
        .insert_foreign_key(ghost_fk.clone(), RowId(1), RowId(0))
        .unwrap();
    let report = validate_table(&table, &schema, &Protocol::default());
    assert_eq!(report.shape, InstanceShapeKind::Table);
    assert_eq!(
        of_kind(&report, CheckKind::NodeAnchor),
        vec![(
            Some(ViewRef::Row(RowId(2))),
            Violation::UnknownAnchor {
                anchor: "invoice".into()
            }
        )]
    );
    assert_eq!(
        of_kind(&report, CheckKind::EdgeDeclared),
        vec![(
            Some(ViewRef::Pair {
                edge: ghost_fk.clone(),
                src: RowId(1),
                tgt: RowId(0)
            }),
            Violation::UndeclaredEdge { label: ghost_fk }
        )]
    );
    assert_eq!(report.status, CheckStatus::Failed);
}

/// A pair whose rows lie outside its edge's endpoint tables is rejected when
/// it is built, so `EndpointCoherence` on a table instance compares rows'
/// tables with the label's endpoints through the anchor families only.
#[test]
fn a_pair_outside_its_edges_tables_is_rejected_by_construction() {
    let fk = edge("order", "customer", "fk", Some("buyer"));
    let schema = S::default()
        .vertex("order", "table")
        .vertex("customer", "table")
        .vertex("vendor", "table")
        .edge(&fk)
        .build();
    let mut table = FInstance::new();
    table.insert_row("order", RowId(0), HashMap::new()).unwrap();
    table
        .insert_row("vendor", RowId(1), HashMap::new())
        .unwrap();
    table
        .insert_row("customer", RowId(2), HashMap::new())
        .unwrap();
    assert!(matches!(
        table.insert_foreign_key(fk.clone(), RowId(0), RowId(1)),
        Err(panproto_inst::InstError::DanglingForeignKey { .. })
    ));
    table.insert_foreign_key(fk, RowId(0), RowId(2)).unwrap();
    let report = validate_table(&table, &schema, &Protocol::default());
    assert!(report.passed(), "{report:?}");
    let coherence =
        report.coverage[&CheckKey::new(CheckKind::EndpointCoherence, SchemaRef::Schema)];
    assert_eq!((coherence.instances, coherence.passed), (1, 1));
}

// ---------------------------------------------------------------------------
// Trees.
// ---------------------------------------------------------------------------

fn tree_schema() -> Schema {
    S::default()
        .vertex("obj", "object")
        .vertex("str1", "string")
        .vertex("str2", "string")
        .edge(&prop("obj", "str1", "name"))
        .edge(&prop("obj", "str2", "desc"))
        .build()
}

fn three_node_tree() -> WInstance {
    WInstance::new(
        HashMap::from([
            (0, Node::new(0, "obj")),
            (1, str_node(1, "str1", "Alice")),
            (2, str_node(2, "str2", "A person")),
        ]),
        vec![
            (0, 1, prop("obj", "str1", "name")),
            (0, 2, prop("obj", "str2", "desc")),
        ],
        vec![],
        0,
        "obj".into(),
    )
}

#[test]
fn a_valid_tree_passes() {
    let report = validate_wtype(&three_node_tree(), &tree_schema(), &Protocol::default());
    assert_eq!(report.status, CheckStatus::Passed, "{report:?}");
    assert_eq!(report.shape, InstanceShapeKind::Tree);
    assert_eq!(report.scope, ReportScope::Instance);
}

#[test]
fn tree_validation_reports_each_tree_kind() {
    let schema = tree_schema();
    let p = Protocol::default();
    // TreeRoot.
    let mut no_root = three_node_tree();
    no_root.nodes.remove(&0);
    let report = validate_wtype(&no_root, &schema, &p);
    assert!(kinds(&report).contains(&CheckKind::TreeRoot));
    // TreeReachable.
    let mut orphan = three_node_tree();
    orphan.nodes.insert(9, str_node(9, "str2", "orphan"));
    let report = validate_wtype(&orphan, &schema, &p);
    assert_eq!(
        of_kind(&report, CheckKind::TreeReachable),
        vec![(Some(ViewRef::Node(9)), Violation::Unreachable)]
    );
    // TreeParentMap.
    let mut skewed = three_node_tree();
    skewed.parent_map.insert(2, 1);
    let report = validate_wtype(&skewed, &schema, &p);
    assert_eq!(
        of_kind(&report, CheckKind::TreeParentMap),
        vec![(
            Some(ViewRef::Node(2)),
            Violation::ParentMismatch {
                recorded: Some(1),
                actual: Some(0)
            }
        )]
    );
    // ReservedVocabulary.
    let mut reserved = three_node_tree();
    reserved.nodes.insert(
        1,
        str_node(1, "str1", "Alice").with_shape(NodeShape::HyperEdge),
    );
    let report = validate_wtype(&reserved, &schema, &p);
    assert_eq!(
        of_kind(&report, CheckKind::ReservedVocabulary),
        vec![(Some(ViewRef::Node(1)), Violation::ReservedVocabulary)]
    );
}

#[test]
fn validate_wtype_reports_reserved_vocabulary() {
    let schema = tree_schema();
    let mut tree = three_node_tree();
    tree.arcs[0].2 = edge("obj", "str1", "hyper_parent", None);
    let report = validate_wtype(&tree, &schema, &Protocol::default());
    let reserved = of_kind(&report, CheckKind::ReservedVocabulary);
    assert_eq!(
        reserved,
        vec![(
            Some(ViewRef::Arc {
                parent: 0,
                child: 1
            }),
            Violation::ReservedVocabulary
        )]
    );
    assert_eq!(
        reserved[0].0,
        Some(panproto_inst::wtype_check::reserved_vocabulary(&tree)[0].clone()),
        "the validator reports what the constructor refuses"
    );
}

/// Each structural invariant the removed `ValidationError` reported maps to
/// a typed violation: an unknown anchor `UnknownAnchor`, an undeclared or
/// incoherent arc `UndeclaredEdge` and `IncoherentEndpoint`, a missing root
/// `MissingRoot`, an unreachable node `Unreachable`, a missing required edge
/// `MissingRequiredEdge`, a parent map disagreeing with the arcs
/// `ParentMismatch`, and a malformed fan `MalformedFan`: a fan label outside
/// the signature `UnknownLabel`, a fan of an undeclared hyper-edge
/// `UnknownHyperEdge`, and a fan naming an absent parent or child
/// `MissingParent` and `MissingChild`.
#[test]
fn legacy_invariants_map_to_typed_violations() {
    let p = Protocol::default();
    let mut schema = tree_schema();
    let name = prop("obj", "str1", "name");
    schema.required.insert("obj".into(), vec![name.clone()]);
    schema = S(schema)
        .hyper_edge("rel", &[("arg", "str1")], "head")
        .build();
    let check = |tree: &WInstance| validate_wtype(tree, &schema, &p);
    let has = |tree: &WInstance, want: &dyn Fn(&Violation) -> bool| {
        let report = check(tree);
        assert!(
            report.diagnostics.iter().any(|d| want(&d.violation)),
            "{report:?}"
        );
    };
    // An unknown anchor.
    let mut t = three_node_tree();
    t.nodes.insert(1, str_node(1, "nonexistent", "Alice"));
    has(
        &t,
        &|v| matches!(v, Violation::UnknownAnchor { anchor } if anchor == "nonexistent"),
    );
    // An undeclared arc, and an arc whose child is anchored elsewhere.
    let mut t = three_node_tree();
    t.arcs[0].2 = prop("obj", "str1", "nickname");
    has(&t, &|v| matches!(v, Violation::UndeclaredEdge { .. }));
    let mut t = three_node_tree();
    t.arcs[0].1 = 2;
    t.parent_map.insert(2, 0);
    has(
        &t,
        &|v| matches!(v, Violation::IncoherentEndpoint { side: EndpointSide::Tgt, found, .. } if found == "str2"),
    );
    // A missing root.
    let mut t = three_node_tree();
    t.nodes.remove(&0);
    has(&t, &|v| matches!(v, Violation::MissingRoot));
    // An unreachable node.
    let mut t = three_node_tree();
    t.nodes.insert(99, str_node(99, "str2", "orphan"));
    has(&t, &|v| matches!(v, Violation::Unreachable));
    // A missing required edge.
    let mut t = three_node_tree();
    t.arcs.remove(0);
    t.parent_map.remove(&1);
    has(
        &t,
        &|v| matches!(v, Violation::MissingRequiredEdge { edge } if *edge == name),
    );
    // A parent map that disagrees with the arcs.
    let mut t = three_node_tree();
    t.parent_map.remove(&1);
    has(&t, &|v| {
        matches!(
            v,
            Violation::ParentMismatch {
                recorded: None,
                actual: Some(0)
            }
        )
    });
    // A malformed fan, a fan of an undeclared hyper-edge, and fans naming an
    // absent parent or child.
    let mut t = three_node_tree();
    t.fans.push(Fan::new("rel", 0).with_child("theme", 1));
    has(
        &t,
        &|v| matches!(v, Violation::MalformedFan { reason: FanViolation::UnknownLabel { label }, .. } if label == "theme"),
    );
    let mut t = three_node_tree();
    t.fans.push(Fan::new("nope", 0));
    has(
        &t,
        &|v| matches!(v, Violation::MalformedFan { reason: FanViolation::UnknownHyperEdge { hyper_edge }, .. } if hyper_edge == "nope"),
    );
    let mut t = three_node_tree();
    t.fans.push(Fan::new("rel", 9).with_child("arg", 1));
    has(&t, &|v| {
        matches!(
            v,
            Violation::MalformedFan {
                reason: FanViolation::MissingParent,
                ..
            }
        )
    });
    let mut t = three_node_tree();
    t.fans.push(Fan::new("rel", 0).with_child("arg", 9));
    has(
        &t,
        &|v| matches!(v, Violation::MalformedFan { reason: FanViolation::MissingChild { label }, .. } if label == "arg"),
    );
    let mut t = three_node_tree();
    t.fans.push(Fan::new("rel", 0).with_child("arg", 1));
    assert!(check(&t).passed(), "a well-formed tree fan passes");
}

#[test]
fn anchor_families_satisfy_edge_endpoints_in_trees() {
    // A union vertex stands for its variants.
    let mut schema = tree_schema();
    schema = S(schema).vertex("str1_variant", "string").build();
    schema.variants.insert(
        "str1".into(),
        vec![panproto_schema::Variant {
            id: "str1_variant".into(),
            parent_vertex: "str1".into(),
            tag: None,
        }],
    );
    let mut tree = three_node_tree();
    tree.nodes.insert(1, str_node(1, "str1_variant", "Alice"));
    assert!(validate_wtype(&tree, &schema, &Protocol::default()).passed());
    // A recursion point stands for the vertex it unfolds to.
    let mut schema = S(tree_schema()).vertex("str1_rec", "string").build();
    schema.recursion_points.insert(
        "str1_rec".into(),
        panproto_schema::RecursionPoint {
            target_vertex: "str1".into(),
        },
    );
    let mut tree = three_node_tree();
    tree.nodes.insert(1, str_node(1, "str1_rec", "Alice"));
    assert!(validate_wtype(&tree, &schema, &Protocol::default()).passed());
    // A reference vertex stands for the target of its reference edge.
    let mut schema = S(tree_schema())
        .vertex("actual", "string")
        .edge(&edge("str1", "actual", "ref", None))
        .build();
    if let Some(v) = schema.vertices.get_mut("str1") {
        v.kind = "ref".into();
    }
    let mut tree = three_node_tree();
    tree.nodes.insert(1, str_node(1, "actual", "Alice"));
    assert!(validate_wtype(&tree, &schema, &Protocol::default()).passed());
}

#[test]
fn attribute_validation_agrees_on_a_tree_and_its_graph() {
    let schema = tree_schema();
    let mut tree = three_node_tree();
    let mut root = Node::new(0, "obj");
    root.extra_fields.insert("name".into(), Value::Int(3));
    root.extra_fields
        .insert("color".into(), Value::Str("red".into()));
    tree.nodes.insert(0, root);
    let graph = GInstance::from_wtype(&tree).unwrap();
    let attrs = panproto_inst::AttributeSchema::from_schema(&schema);
    // The tree projection visits fields in map order; the graph projection
    // in ascending key. The two find the same violations.
    let key_of = |v: &panproto_inst::AttributeViolation| format!("{v:?}");
    let mut on_tree = attrs.validate_wtype(&tree);
    on_tree.sort_by_key(key_of);
    let mut on_graph = attrs.validate_ginstance(&graph);
    on_graph.sort_by_key(key_of);
    assert_eq!(on_tree, on_graph);
    assert_eq!(on_tree.len(), 2, "{on_tree:?}");
    let p = Protocol::default();
    let tree_report = validate_wtype(&tree, &schema, &p);
    let graph_report = validate_graph(&graph, &schema, &p);
    assert_eq!(
        of_kind(&tree_report, CheckKind::Attribute),
        of_kind(&graph_report, CheckKind::Attribute)
    );
    assert_eq!(
        of_kind(&tree_report, CheckKind::Attribute),
        vec![
            (
                Some(ViewRef::Node(0)),
                Violation::UndeclaredAttribute {
                    key: "color".into()
                }
            ),
            (
                Some(ViewRef::Node(0)),
                Violation::MistypedAttribute {
                    key: "name".into(),
                    expected: "string".into(),
                    found: "int".into()
                }
            ),
        ]
    );
}

// ---------------------------------------------------------------------------
// Budgets, external verdicts and evaluation order.
// ---------------------------------------------------------------------------

/// A chain of `n` nodes over a declared acyclic self-edge.
fn chain(n: u32) -> (Schema, Protocol, GInstance) {
    let next = edge("n", "n", "ref", Some("next"));
    let schema = S::default()
        .vertex("n", "object")
        .edge(&next)
        .constraint("n", "acyclic", "next")
        .build();
    let p = protocol(&[("acyclic", ConstraintSemantics::Acyclic)]);
    let mut g = GInstance::new();
    for i in 0..n {
        g = g.with_node(Node::new(i, "n"));
    }
    for i in 1..n {
        g = g.with_edge(i - 1, i, next.clone());
    }
    (schema, p, g)
}

#[test]
fn a_budget_cuts_a_suffix_in_check_instance_order() {
    let (schema, p, g) = chain(50);
    let plan = plan(&schema, &p);
    let full = plan.validate_graph(&g, &unlimited(), &none());
    assert!(full.passed());
    // NodeAnchor reads one node per instance (50), EdgeDeclared one edge
    // (49), EndpointCoherence an edge and its two ends (49 * 3); Acyclic
    // reads every labelled edge and its endpoints (99).
    let small = ValidationBudget {
        max_reads: Some(60),
    };
    let report = plan.validate_graph(&g, &small, &none());
    assert_eq!(report.status, CheckStatus::Incomplete);
    let budget: Vec<&IncompleteCheck> = report
        .incomplete
        .iter()
        .filter(|i| matches!(i.reason, IncompleteReason::Budget { .. }))
        .collect();
    // NodeAnchor (50 reads) fits; EdgeDeclared is cut after 10 instances,
    // and every later check is cut from its first instance.
    let IncompleteReason::Budget { limit, spent, from } = &budget[0].reason else {
        panic!("{budget:?}");
    };
    assert_eq!(budget[0].check.kind, CheckKind::EdgeDeclared);
    assert_eq!((*limit, *spent), (60, 60));
    assert_eq!(from, &Some(ViewRef::Edge(EdgeId(10))));
    let cut_kinds: Vec<CheckKind> = budget.iter().map(|i| i.check.kind).collect();
    assert_eq!(
        cut_kinds,
        vec![
            CheckKind::EdgeDeclared,
            CheckKind::EndpointCoherence,
            CheckKind::Acyclic
        ]
    );
    assert!(budget.iter().all(|i| i.subject.is_none()));
    let cov = report.coverage[&CheckKey::new(CheckKind::EdgeDeclared, SchemaRef::Schema)];
    assert_eq!((cov.passed, cov.incomplete), (10, 39));
    // A limit at least what the unlimited run spent gives the same report.
    let enough = ValidationBudget {
        max_reads: Some(50 + 49 + 49 * 3 + 99),
    };
    assert_eq!(plan.validate_graph(&g, &enough, &none()), full);
    let one_short = ValidationBudget {
        max_reads: Some(50 + 49 + 49 * 3 + 98),
    };
    let report = plan.validate_graph(&g, &one_short, &none());
    assert_eq!(report.status, CheckStatus::Incomplete);
    assert!(matches!(
        report.incomplete.as_slice(),
        [IncompleteCheck { check, reason: IncompleteReason::Budget { from: None, .. }, .. }]
            if check.kind == CheckKind::Acyclic
    ));
}

#[test]
fn evaluation_and_budget_follow_check_instance_order() {
    let report = validate_graph(
        &GInstance::new()
            .with_node(Node::new(3, "ghost"))
            .with_node(Node::new(1, "ghost"))
            .with_node(str_node(2, "name", "much too long"))
            .with_edge(3, 1, prop("person", "org", "x")),
        &probe_schema(),
        &probe_protocol(),
    );
    let order: Vec<(CheckKey, Option<ViewRef>)> = report
        .diagnostics
        .iter()
        .map(|d| (d.check.clone(), d.subject.clone()))
        .collect();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(order, sorted);
    assert_eq!(
        order.iter().map(|(k, _)| k.kind).collect::<Vec<_>>(),
        vec![
            CheckKind::NodeAnchor,
            CheckKind::NodeAnchor,
            CheckKind::EdgeDeclared,
            CheckKind::EndpointCoherence,
            CheckKind::EndpointCoherence,
            CheckKind::ValueConstraint
        ]
    );
    // The budget is consumed in the same order: with room for exactly the
    // two node anchors, every later check is cut.
    let plan = plan(&probe_schema(), &probe_protocol());
    let graph = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(Node::new(1, "org"));
    let report = plan.validate_graph(&graph, &ValidationBudget { max_reads: Some(2) }, &none());
    let first_cut = report
        .incomplete
        .iter()
        .find(|i| matches!(i.reason, IncompleteReason::Budget { .. }))
        .unwrap();
    assert_eq!(first_cut.check.kind, CheckKind::RequiredEdge);
}

#[test]
fn external_verdicts_missing_and_rejected() {
    let schema = S::default().vertex("doc", "object").build();
    let options = ValidationOptions {
        external: BTreeMap::from([(
            Name::from("licensed"),
            ExternalCheckDecl {
                name: "licensed".into(),
                anchors: BTreeSet::from([Name::from("doc")]),
                requirement: Requirement::Required,
            },
        )]),
        ..ValidationOptions::default()
    };
    let plan = ValidationPlan::compile(&schema, &Protocol::default(), &options);
    let graph = GInstance::new()
        .with_node(Node::new(0, "doc"))
        .with_node(Node::new(1, "doc"))
        .with_node(Node::new(2, "doc"));
    let mut verdicts = ExternalVerdicts::default();
    verdicts.insert(
        "licensed".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: true,
            witness: Digest([1; 32]),
        },
    );
    verdicts.insert(
        "licensed".into(),
        ViewRef::Node(1),
        ExternalVerdict {
            passed: false,
            witness: Digest([2; 32]),
        },
    );
    let report = plan.validate_graph(&graph, &unlimited(), &verdicts);
    assert_eq!(report.status, CheckStatus::Failed);
    assert_eq!(
        of_kind(&report, CheckKind::External),
        vec![(
            Some(ViewRef::Node(1)),
            Violation::ExternalRejected {
                witness: Digest([2; 32])
            }
        )]
    );
    assert_eq!(
        report.incomplete,
        vec![IncompleteCheck {
            check: CheckKey::new(CheckKind::External, SchemaRef::External("licensed".into())),
            class: CheckClass::BoundedExternal,
            requirement: Requirement::Required,
            subject: Some(ViewRef::Node(2)),
            reason: IncompleteReason::MissingExternalWitness {
                name: "licensed".into()
            },
        }]
    );
    // Without the rejection, the missing witness leaves it incomplete.
    verdicts.insert(
        "licensed".into(),
        ViewRef::Node(1),
        ExternalVerdict {
            passed: true,
            witness: Digest([2; 32]),
        },
    );
    let report = plan.validate_graph(&graph, &unlimited(), &verdicts);
    assert_eq!(report.status, CheckStatus::Incomplete);
    assert!(!report.incomplete.is_empty());
}

#[test]
fn every_public_validator_returns_a_validation_report() {
    let graph: fn(&GInstance, &Schema, &Protocol) -> ValidationReport = validate_graph;
    let tree: fn(&WInstance, &Schema, &Protocol) -> ValidationReport = validate_wtype;
    let table: fn(&FInstance, &Schema, &Protocol) -> ValidationReport = validate_table;
    let plan_graph: fn(
        &ValidationPlan,
        &GInstance,
        &ValidationBudget,
        &ExternalVerdicts,
    ) -> ValidationReport = ValidationPlan::validate_graph;
    let plan_tree: fn(
        &ValidationPlan,
        &WInstance,
        &ValidationBudget,
        &ExternalVerdicts,
    ) -> ValidationReport = ValidationPlan::validate_tree;
    let plan_table: fn(
        &ValidationPlan,
        &FInstance,
        &ValidationBudget,
        &ExternalVerdicts,
    ) -> ValidationReport = ValidationPlan::validate_table;
    let schema = Schema::default();
    let p = Protocol::default();
    let empty_tree = WInstance::new(HashMap::new(), vec![], vec![], 0, "x".into());
    let reports = [
        graph(&GInstance::new(), &schema, &p),
        tree(&empty_tree, &schema, &p),
        table(&FInstance::new(), &schema, &p),
    ];
    // An empty graph and table pass; an empty tree has no root.
    assert_eq!(reports[0].status, CheckStatus::Passed);
    assert_eq!(reports[1].status, CheckStatus::Failed);
    assert_eq!(reports[2].status, CheckStatus::Passed);
    let compiled = plan(&schema, &p);
    let _ = (
        plan_graph(&compiled, &GInstance::new(), &unlimited(), &none()),
        plan_tree(&compiled, &empty_tree, &unlimited(), &none()),
        plan_table(&compiled, &FInstance::new(), &unlimited(), &none()),
    );
}

// ---------------------------------------------------------------------------
// Properties.
// ---------------------------------------------------------------------------

/// Random graphs over the probe schema: anchors drawn from the schema's
/// vertices and one unknown name, labels from its edges and one undeclared
/// label, values of varying length.
fn arb_probe_graph() -> impl Strategy<Value = GInstance> {
    let anchors = prop::sample::select(vec!["person", "org", "name", "ghost"]);
    let labels = prop::sample::select(vec![
        edge("person", "org", "ref", Some("worksAt")),
        prop("person", "name", "name"),
        edge("person", "org", "ref", Some("owns")),
    ]);
    let nodes = prop::collection::vec((anchors, "[a-z]{0,6}"), 1..7);
    nodes
        .prop_flat_map(move |nodes| {
            let n = u32::try_from(nodes.len()).unwrap_or(1);
            let edges = prop::collection::vec((0..n, 0..n, labels.clone()), 0..9);
            (Just(nodes), edges)
        })
        .prop_map(|(nodes, edges)| {
            let mut g = GInstance::new();
            for (i, (anchor, text)) in nodes.into_iter().enumerate() {
                let id = u32::try_from(i).unwrap_or(0);
                g = g.with_node(str_node(id, anchor, &text));
            }
            for (s, t, label) in edges {
                g = g.with_edge(s, t, label);
            }
            g
        })
}

fn violation_elements(v: &Violation) -> Vec<ViewRef> {
    match v {
        Violation::OutDegree { edges, .. } => edges.clone(),
        Violation::DuplicatePosition { edges, .. } | Violation::Cycle { witness: edges, .. } => {
            edges.iter().map(|e| ViewRef::Edge(*e)).collect()
        }
        Violation::DuplicateKey { elems, .. } => elems.clone(),
        Violation::AmbiguousReference { candidates, .. } => candidates.clone(),
        _ => Vec::new(),
    }
}

proptest! {
    #[test]
    fn diagnostic_elements_are_ascending_and_cover_the_violation(g in arb_probe_graph()) {
        let report = validate_graph(&g, &probe_schema(), &probe_protocol());
        for d in &report.diagnostics {
            let mut sorted = d.elements.clone();
            sorted.sort();
            sorted.dedup();
            prop_assert_eq!(&sorted, &d.elements);
            if let Some(subject) = &d.subject {
                prop_assert!(d.elements.contains(subject), "{:?}", d);
            }
            for e in violation_elements(&d.violation) {
                prop_assert!(d.elements.contains(&e), "{:?}", d);
            }
        }
    }

    #[test]
    fn coverage_counts_sum_to_instances(
        g in arb_probe_graph(),
        limit in prop::option::of(0u64..40),
    ) {
        let plan = plan(&probe_schema(), &probe_protocol());
        let report = plan.validate_graph(&g, &ValidationBudget { max_reads: limit }, &none());
        for (key, cov) in &report.coverage {
            prop_assert_eq!(cov.passed + cov.failed + cov.incomplete, cov.instances, "{:?}", key);
            let failed: BTreeSet<&Option<ViewRef>> = report
                .diagnostics
                .iter()
                .filter(|d| &d.check == key)
                .map(|d| &d.subject)
                .collect();
            if key.kind != CheckKind::KeyUnique && key.kind != CheckKind::Acyclic {
                prop_assert_eq!(failed.len() as u64, cov.failed, "{:?}", key);
            }
        }
    }

    #[test]
    fn report_entries_are_sorted(g in arb_probe_graph(), limit in prop::option::of(0u64..40)) {
        let plan = plan(&probe_schema(), &probe_protocol());
        let report = plan.validate_graph(&g, &ValidationBudget { max_reads: limit }, &none());
        let keys: Vec<_> = report
            .diagnostics
            .iter()
            .map(|d| (d.check.clone(), d.subject.clone(), d.elements.clone()))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        prop_assert_eq!(keys, sorted);
        let keys: Vec<_> = report
            .incomplete
            .iter()
            .map(|i| (i.check.clone(), i.subject.clone()))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        prop_assert_eq!(keys, sorted);
        // The canonical bytes, and so the digest, are a function of the
        // instance alone.
        let again = plan.validate_graph(&g, &ValidationBudget { max_reads: limit }, &none());
        prop_assert_eq!(report.canonical_bytes(), again.canonical_bytes());
    }

    #[test]
    fn v7_finite_budget_never_passes_where_unlimited_does_not(
        g in arb_probe_graph(),
        limit in 0u64..60,
    ) {
        let plan = plan(&probe_schema(), &probe_protocol());
        let full = plan.validate_graph(&g, &unlimited(), &none());
        let cut = plan.validate_graph(&g, &ValidationBudget { max_reads: Some(limit) }, &none());
        if full.status != CheckStatus::Passed {
            prop_assert_ne!(cut.status, CheckStatus::Passed);
        }
        let budget: Vec<&IncompleteCheck> = cut
            .incomplete
            .iter()
            .filter(|i| matches!(i.reason, IncompleteReason::Budget { .. }))
            .collect();
        if budget.is_empty() {
            prop_assert_eq!(&cut, &full);
        } else {
            // The cut keys are a suffix of the evaluated keys: every key
            // after the first cut one is cut from its first instance.
            let first = &budget[0].check;
            for (key, cov) in &cut.coverage {
                if key > first && cov.instances > 0 {
                    prop_assert_eq!(cov.incomplete, cov.instances, "{:?}", key);
                }
                if key < first {
                    prop_assert_eq!(cov, &full.coverage[key]);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reference compilation, fan anchors, endpoint families, required targets,
// report decoding.
// ---------------------------------------------------------------------------

/// The reference checks a plan compiled, by kind.
fn reference_kinds(plan: &ValidationPlan) -> Vec<(CheckKind, Requirement)> {
    plan.checks()
        .values()
        .filter(|s| matches!(s.key.decl, SchemaRef::Reference(_)))
        .map(|s| (s.key.kind, s.requirement))
        .collect()
}

#[test]
fn a_reference_compiles_to_reference_resolves_when_the_shapes_agree() {
    use panproto_schema::OnUnresolved;
    // Tree and Tree: the syntactic and the semantic shape are one.
    let p = Protocol::default();
    assert_eq!(p.syntactic_shape, p.semantic_shape);
    // Without an edge, the reference is only ReferenceResolves.
    let mut schema = referencing_schema(OnUnresolved::Error);
    if let Some(r) = schema.declarations.references.values_mut().next() {
        r.edge = None;
    }
    let strict = plan(&schema, &p);
    assert_eq!(
        reference_kinds(&strict),
        vec![(CheckKind::ReferenceResolves, Requirement::Required)]
    );
    let report = strict.validate_tree(&people(&[(10, Some(99))]), &unlimited(), &none());
    assert_eq!(
        of_kind(&report, CheckKind::ReferenceResolves)
            .into_iter()
            .filter(|(_, v)| matches!(v, Violation::DanglingReference { .. }))
            .collect::<Vec<_>>(),
        vec![(
            Some(ViewRef::Node(1)),
            Violation::DanglingReference {
                reference: "boss".into(),
                key: vec![Value::Int(99)],
            }
        )]
    );
    assert_eq!(report.status, CheckStatus::Failed);
    // With an edge and a lenient policy, ReferenceResolves is advisory
    // beside the semantic degree and required-edge checks.
    let lenient = plan(&referencing_schema(OnUnresolved::Complement), &p);
    assert_eq!(
        reference_kinds(&lenient),
        vec![
            (CheckKind::RequiredEdge, Requirement::Required),
            (CheckKind::OutDegree, Requirement::Required),
            (CheckKind::ReferenceResolves, Requirement::Advisory),
        ]
    );
}

/// Every `FanWellFormed` diagnostic of `report` as (subject, elements,
/// violation).
fn fan_entries(report: &ValidationReport) -> Vec<(Option<ViewRef>, Vec<ViewRef>, Violation)> {
    report
        .diagnostics
        .iter()
        .filter(|d| d.check.kind == CheckKind::FanWellFormed)
        .map(|d| (d.subject.clone(), d.elements.clone(), d.violation.clone()))
        .collect()
}

/// A root `doc` with one child `1` of the same vertex, and no hyper-edge.
fn doc_tree_with_child() -> (Schema, WInstance) {
    let schema = S::default()
        .vertex("doc", "object")
        .edge(&prop("doc", "doc", "c"))
        .build();
    let tree = WInstance::new(
        HashMap::from([(0, Node::new(0, "doc")), (1, Node::new(1, "doc"))]),
        vec![(0, 1, prop("doc", "doc", "c"))],
        vec![],
        0,
        "doc".into(),
    );
    (schema, tree)
}

#[test]
fn a_tree_fan_of_an_undeclared_hyper_edge_is_malformed() {
    let (schema, mut tree) = doc_tree_with_child();
    let fan = Fan::new("nope", 0).with_child("lab", 1);
    tree.fans.push(fan.clone());
    let report = validate_wtype(&tree, &schema, &Protocol::default());
    // I7 maps to `MalformedFan`: the hyper-edge is undeclared, so its labels
    // are not read against any signature.
    assert_eq!(
        fan_entries(&report),
        vec![(
            Some(ViewRef::Node(0)),
            vec![ViewRef::Node(0), ViewRef::Node(1)],
            Violation::MalformedFan {
                reason: FanViolation::UnknownHyperEdge {
                    hyper_edge: "nope".into()
                },
                fan: Some(FanRecord::of(&fan)),
            }
        )]
    );
    assert_eq!(report.status, CheckStatus::Failed);
}

/// The `FanWellFormed` diagnostics of a graph report about the graph form
/// of a tree, read back as tree fan diagnostics: the diagnostic at the fan
/// node of the tree's `k`th fan becomes `(FanRecord::of(&tree.fans[k]),
/// reason)`.
fn graph_fan_pairs(
    tree: &WInstance,
    graph_report: &ValidationReport,
    fan_nodes: &[u32],
) -> BTreeSet<(FanRecord, FanViolation)> {
    fan_entries(graph_report)
        .into_iter()
        .map(|(subject, _, violation)| {
            let Some(ViewRef::Node(node)) = subject else {
                panic!("a graph fan diagnostic names its fan node");
            };
            let k = fan_nodes.iter().position(|n| *n == node).unwrap();
            let Violation::MalformedFan { reason, fan: None } = violation else {
                panic!("a graph fan diagnostic carries no fan record");
            };
            (FanRecord::of(&tree.fans[k]), reason)
        })
        .collect()
}

/// The `FanWellFormed` diagnostics of a tree report as `(fan, reason)`.
fn tree_fan_pairs(report: &ValidationReport) -> BTreeSet<(FanRecord, FanViolation)> {
    fan_entries(report)
        .into_iter()
        .map(|(_, _, violation)| {
            let Violation::MalformedFan {
                reason,
                fan: Some(fan),
            } = violation
            else {
                panic!("a tree fan diagnostic names its fan");
            };
            (fan, reason)
        })
        .collect()
}

#[test]
fn a_fan_of_an_undeclared_hyper_edge_is_one_malformed_fan_in_every_shape() {
    for fan in [
        Fan::new("nope", 0).with_child("lab", 1),
        Fan::new("nope", 0),
    ] {
        let (schema, mut tree) = doc_tree_with_child();
        tree.fans.push(fan.clone());
        let tree_report = validate_wtype(&tree, &schema, &Protocol::default());
        let (graph, emb) = GInstance::from_wtype_embedded(&tree, None).unwrap();
        let fan_node = emb.fans[0].node;
        let graph_report = validate_graph(&graph, &schema, &Protocol::default());
        // One fault, one diagnostic, in each shape: the graph's fan node is
        // not an element `NodeAnchor` checks.
        let expected = BTreeSet::from([(
            FanRecord::of(&fan),
            FanViolation::UnknownHyperEdge {
                hyper_edge: "nope".into(),
            },
        )]);
        assert_eq!(tree_fan_pairs(&tree_report), expected);
        assert_eq!(graph_fan_pairs(&tree, &graph_report, &[fan_node]), expected);
        assert_eq!(fan_entries(&tree_report).len(), 1);
        assert_eq!(fan_entries(&graph_report).len(), 1);
        assert!(
            of_kind(&graph_report, CheckKind::NodeAnchor)
                .iter()
                .all(|(subject, _)| subject != &Some(ViewRef::Node(fan_node))),
            "{graph_report:?}"
        );
        assert!(of_kind(&graph_report, CheckKind::NodeAnchor).is_empty());
        let anchors = report_key(&graph_report, CheckKind::NodeAnchor);
        assert_eq!(graph_report.coverage[&anchors].instances, 2);
        assert_eq!(tree_report.status, CheckStatus::Failed);
        assert_eq!(graph_report.status, CheckStatus::Failed);
    }
}

/// The key of the one check of `kind` that a report covers.
fn report_key(report: &ValidationReport, kind: CheckKind) -> CheckKey {
    let keys: Vec<&CheckKey> = report.coverage.keys().filter(|k| k.kind == kind).collect();
    assert_eq!(keys.len(), 1, "{keys:?}");
    keys[0].clone()
}

#[test]
fn a_tree_fan_of_an_undeclared_hyper_edge_with_no_labels_is_a_malformed_fan() {
    let (schema, mut tree) = doc_tree_with_child();
    tree.fans.push(Fan::new("nope", 0));
    let report = validate_wtype(&tree, &schema, &Protocol::default());
    assert_eq!(
        fan_entries(&report),
        vec![(
            Some(ViewRef::Node(0)),
            vec![ViewRef::Node(0)],
            Violation::MalformedFan {
                reason: FanViolation::UnknownHyperEdge {
                    hyper_edge: "nope".into()
                },
                fan: Some(FanRecord::of(&Fan::new("nope", 0))),
            }
        )]
    );
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| !matches!(d.violation, Violation::UnknownAnchor { .. })),
        "{report:?}"
    );
    assert_eq!(report.status, CheckStatus::Failed);
}

#[test]
fn a_tree_fan_with_an_absent_child_is_missing_child() {
    let (schema, mut tree) = three_doc_tree(vec![]);
    // `x` is declared and its child 9 is absent: `MissingChild`, never
    // `UnknownLabel`.
    let fan = Fan::new("a", 0).with_child("x", 9);
    tree.fans.push(fan.clone());
    let report = validate_wtype(&tree, &schema, &Protocol::default());
    assert_eq!(
        fan_entries(&report),
        vec![(
            Some(ViewRef::Node(0)),
            vec![ViewRef::Node(0), ViewRef::Node(9)],
            Violation::MalformedFan {
                reason: FanViolation::MissingChild { label: "x".into() },
                fan: Some(FanRecord::of(&fan)),
            }
        )]
    );
    assert_eq!(report.status, CheckStatus::Failed);
}

/// Every reason a tree fan breaks reaches the report (the order in which
/// they are produced is the unit test `tree_fan_reasons_are_reported_in_order`
/// of the checks; the report orders diagnostics with one subject and one
/// element list by their canonical bytes, V.2).
#[test]
fn every_tree_fan_reason_reaches_the_report() {
    // Parent 9 is absent, children 7 and 8 are absent, and `w` and `z` are
    // not in the signature `{x, y}` of `h`.
    let fan = Fan::new("h", 9)
        .with_child("z", 8)
        .with_child("x", 7)
        .with_child("w", 1)
        .with_child("y", 1);
    let (schema, mut tree) = three_doc_tree(vec![fan.clone()]);
    let report = validate_wtype(&tree, &schema, &Protocol::default());
    let record = FanRecord::of(&fan);
    let elements = vec![
        ViewRef::Node(1),
        ViewRef::Node(7),
        ViewRef::Node(8),
        ViewRef::Node(9),
    ];
    let expected = |reason: FanViolation| {
        (
            Some(ViewRef::Node(9)),
            elements.clone(),
            Violation::MalformedFan {
                reason,
                fan: Some(record.clone()),
            },
        )
    };
    // In canonical byte order: fan reason tags 0 (missing parent), 3
    // (unknown label) and 7 (missing child).
    assert_eq!(
        fan_entries(&report),
        vec![
            expected(FanViolation::MissingParent),
            expected(FanViolation::UnknownLabel { label: "w".into() }),
            expected(FanViolation::UnknownLabel { label: "z".into() }),
            expected(FanViolation::MissingChild { label: "x".into() }),
            expected(FanViolation::MissingChild { label: "z".into() }),
        ]
    );
    // With an undeclared hyper-edge, `UnknownHyperEdge` replaces every
    // `UnknownLabel`.
    let ghost = Fan {
        hyper_edge_id: "ghost".into(),
        ..fan
    };
    tree.fans = vec![ghost.clone()];
    let report = validate_wtype(&tree, &schema, &Protocol::default());
    assert_eq!(
        tree_fan_pairs(&report),
        BTreeSet::from([
            (FanRecord::of(&ghost), FanViolation::MissingParent),
            (
                FanRecord::of(&ghost),
                FanViolation::UnknownHyperEdge {
                    hyper_edge: "ghost".into()
                }
            ),
            (
                FanRecord::of(&ghost),
                FanViolation::MissingChild { label: "x".into() }
            ),
            (
                FanRecord::of(&ghost),
                FanViolation::MissingChild { label: "z".into() }
            ),
        ])
    );
}

/// A tree fan diagnostic with subject the fan's parent `parent`, elements
/// `parent` and `children`, and the fan's record.
fn tree_fan_entry(
    fan: &Fan,
    elements: &[u32],
    reason: FanViolation,
) -> (Option<ViewRef>, Vec<ViewRef>, Violation) {
    (
        Some(ViewRef::Node(fan.parent)),
        elements.iter().map(|n| ViewRef::Node(*n)).collect(),
        Violation::MalformedFan {
            reason,
            fan: Some(FanRecord::of(fan)),
        },
    )
}

#[test]
fn tree_fan_diagnostics_name_the_fan_they_concern() {
    let unknown = || FanViolation::UnknownLabel {
        label: "lab".into(),
    };
    // Two fans at one parent: each diagnostic names its own fan.
    let (first, second) = (
        Fan::new("a", 0).with_child("lab", 1),
        Fan::new("a", 0).with_child("lab", 2),
    );
    let (schema, tree) = three_doc_tree(vec![first.clone(), second.clone()]);
    let report = validate_wtype(&tree, &schema, &Protocol::default());
    assert_eq!(
        fan_entries(&report),
        vec![
            tree_fan_entry(&first, &[0, 1], unknown()),
            tree_fan_entry(&second, &[0, 2], unknown()),
        ]
    );
    assert_eq!(report.coverage[&report.diagnostics[0].check].failed, 1);
}

#[test]
fn tree_fan_diagnostics_name_children_the_tree_does_not_hold() {
    let schema = S::default()
        .vertex("doc", "object")
        .edge(&prop("doc", "doc", "c"))
        .hyper_edge("he", &[("lab", "doc")], "head")
        .build();
    let mut t = WInstance::new(
        HashMap::from([(0, Node::new(0, "doc"))]),
        vec![],
        vec![],
        0,
        "doc".into(),
    );
    // Two unequal fans at one parent, each naming a child the tree lacks.
    let (five, six) = (
        Fan::new("he", 0).with_child("lab", 5),
        Fan::new("he", 0).with_child("lab", 6),
    );
    t.fans = vec![five.clone(), six.clone()];
    let missing = || FanViolation::MissingChild {
        label: "lab".into(),
    };
    let report = validate_wtype(&t, &schema, &Protocol::default());
    assert_eq!(
        fan_entries(&report),
        vec![
            tree_fan_entry(&five, &[0, 5], missing()),
            tree_fan_entry(&six, &[0, 6], missing()),
        ]
    );
    assert_eq!(report.coverage[&report.diagnostics[0].check].failed, 1);
}

#[test]
fn tree_fan_diagnostics_at_an_absent_parent_name_the_fan() {
    let schema = S::default()
        .vertex("doc", "object")
        .edge(&prop("doc", "doc", "c"))
        .hyper_edge("he", &[("lab", "doc")], "head")
        .hyper_edge("other", &[("lab", "doc")], "head")
        .build();
    let (_, mut t) = three_doc_tree(vec![]);
    // Unequal fans whose parent 9 the tree lacks: two that differ in their
    // child, and two that differ only in their hyper-edge.
    let fans = [
        Fan::new("he", 9).with_child("lab", 1),
        Fan::new("he", 9).with_child("lab", 2),
        Fan::new("other", 9).with_child("lab", 1),
    ];
    t.fans = fans.to_vec();
    let report = validate_wtype(&t, &schema, &Protocol::default());
    assert_eq!(
        fan_entries(&report),
        vec![
            tree_fan_entry(&fans[0], &[1, 9], FanViolation::MissingParent),
            tree_fan_entry(&fans[2], &[1, 9], FanViolation::MissingParent),
            tree_fan_entry(&fans[1], &[2, 9], FanViolation::MissingParent),
        ]
    );
}

#[test]
fn equal_tree_fans_give_one_diagnostic_per_reason() {
    // Two equal fans of an undeclared hyper-edge, each with a child the tree
    // lacks: one fan of the tree's fan multiset, two reasons.
    let fan = Fan::new("ghost", 0).with_child("lab", 9);
    let (schema, tree) = three_doc_tree(vec![fan.clone(), fan.clone()]);
    let report = validate_wtype(&tree, &schema, &Protocol::default());
    assert_eq!(
        fan_entries(&report),
        vec![
            tree_fan_entry(
                &fan,
                &[0, 9],
                FanViolation::UnknownHyperEdge {
                    hyper_edge: "ghost".into()
                }
            ),
            tree_fan_entry(
                &fan,
                &[0, 9],
                FanViolation::MissingChild {
                    label: "lab".into()
                }
            ),
        ]
    );
    assert_eq!(report.coverage[&report.diagnostics[0].check].failed, 1);
    // Equal label-less fans: one diagnostic.
    let bare = Fan::new("ghost", 0);
    let (schema, tree) = three_doc_tree(vec![bare.clone(), bare.clone()]);
    let report = validate_wtype(&tree, &schema, &Protocol::default());
    assert_eq!(
        fan_entries(&report),
        vec![tree_fan_entry(
            &bare,
            &[0],
            FanViolation::UnknownHyperEdge {
                hyper_edge: "ghost".into()
            }
        )]
    );
}

#[test]
fn fan_record_order_is_the_canonical_fan_record_order() -> TestResult {
    // I orders a tree's fan records by parent, then hyper-edge, then
    // children ascending by label.
    let fans = [
        Fan::new("b", 0).with_child("x", 1),
        Fan::new("a", 1),
        Fan::new("a", 0).with_child("y", 1),
        Fan::new("a", 0).with_child("x", 2),
        Fan::new("a", 0).with_child("x", 1).with_child("y", 3),
        Fan::new("a", 0).with_child("x", 1),
        Fan::new("a", 0),
    ];
    let mut records: Vec<FanRecord> = fans.iter().map(FanRecord::of).collect();
    records.sort();
    let key = |r: &FanRecord| (r.parent, r.hyper_edge.clone(), r.children.clone());
    let mut by_key = records.clone();
    by_key.sort_by_key(key);
    assert_eq!(records, by_key);
    assert_eq!(
        records,
        vec![
            FanRecord::of(&Fan::new("a", 0)),
            FanRecord::of(&Fan::new("a", 0).with_child("x", 1)),
            FanRecord::of(&Fan::new("a", 0).with_child("x", 1).with_child("y", 3)),
            FanRecord::of(&Fan::new("a", 0).with_child("x", 2)),
            FanRecord::of(&Fan::new("a", 0).with_child("y", 1)),
            FanRecord::of(&Fan::new("b", 0).with_child("x", 1)),
            FanRecord::of(&Fan::new("a", 1)),
        ]
    );
    // Children are ascending by label whatever order the fan lists them in,
    // and the record serializes with named fields.
    let record = FanRecord::of(&Fan::new("h", 4).with_child("y", 2).with_child("x", 3));
    assert_eq!(
        serde_json::to_string(&record)?,
        r#"{"parent":4,"hyper_edge":"h","children":[["x",3],["y",2]]}"#
    );
    round_trip(&record)?;
    Ok(())
}

#[test]
fn malformed_fan_bytes_and_messages_name_the_fan() {
    let fan = FanRecord::of(&Fan::new("h", 4).with_child("y", 2).with_child("x", 3));
    let tree_fan = Violation::MalformedFan {
        reason: FanViolation::MissingChild { label: "z".into() },
        fan: Some(fan),
    };
    let graph_fan = Violation::MalformedFan {
        reason: FanViolation::UnknownHyperEdge {
            hyper_edge: "h".into(),
        },
        fan: None,
    };
    assert_eq!(
        tree_fan.to_string(),
        "malformed fan h at 4 {x: 3, y: 2}: fan child at label z is not a node of the tree"
    );
    assert_eq!(
        graph_fan.to_string(),
        "malformed fan: fan hyper-edge h is not declared by the schema"
    );
    let bytes = |v: Violation| {
        ValidationReport::new(
            InstanceShapeKind::Tree,
            ReportScope::Instance,
            Digest([0; 32]),
            vec![d(CheckKind::FanWellFormed, Requirement::Required, v)],
            vec![],
            BTreeMap::new(),
        )
        .canonical_bytes()
    };
    let name = |s: &str| {
        let mut out = u64::try_from(s.len()).unwrap().to_le_bytes().to_vec();
        out.extend(s.as_bytes());
        out
    };
    // malformed_fan (3), missing_child (7) and its label, then Some, the
    // parent, the hyper-edge, two children and each (label, child).
    let mut tree_tail = vec![3u8, 7];
    tree_tail.extend(name("z"));
    tree_tail.push(1);
    tree_tail.extend(4u32.to_le_bytes());
    tree_tail.extend(name("h"));
    tree_tail.extend(2u64.to_le_bytes());
    for (label, child) in [("x", 3u32), ("y", 2)] {
        tree_tail.extend(name(label));
        tree_tail.extend(child.to_le_bytes());
    }
    let tree_bytes = bytes(tree_fan);
    assert!(
        tree_bytes.windows(tree_tail.len()).any(|w| w == tree_tail),
        "{tree_bytes:?}"
    );
    // unknown_hyper_edge (6) and its hyper-edge, then None.
    let mut graph_tail = vec![3u8, 6];
    graph_tail.extend(name("h"));
    graph_tail.push(0);
    let graph_bytes = bytes(graph_fan);
    assert!(
        graph_bytes
            .windows(graph_tail.len())
            .any(|w| w == graph_tail),
        "{graph_bytes:?}"
    );
}

/// A root with a union `u` of variants `a` and `b`, and an edge from the
/// root to `a`.
fn sibling_variant_schema() -> Schema {
    let mut schema = S::default()
        .vertex("root", "object")
        .vertex("u", "object")
        .vertex("a", "object")
        .vertex("b", "object")
        .edge(&prop("root", "a", "x"))
        .build();
    schema.variants.insert(
        "u".into(),
        ["a", "b"]
            .into_iter()
            .map(|v| panproto_schema::Variant {
                id: v.into(),
                parent_vertex: "u".into(),
                tag: None,
            })
            .collect(),
    );
    schema
}

#[test]
fn a_sibling_variant_is_not_an_endpoint_of_its_siblings_edge() {
    let schema = sibling_variant_schema();
    let graph = GInstance::new()
        .with_node(Node::new(0, "root"))
        .with_node(Node::new(1, "b"))
        .with_edge(0, 1, prop("root", "a", "x"));
    let report = validate_graph(&graph, &schema, &Protocol::default());
    assert_eq!(
        of_kind(&report, CheckKind::EndpointCoherence),
        vec![(
            Some(ViewRef::Edge(EdgeId(0))),
            Violation::IncoherentEndpoint {
                side: EndpointSide::Tgt,
                expected: "a".into(),
                found: "b".into(),
            }
        )]
    );
    // The union stands for each variant, and a variant for its union.
    let mut schema = sibling_variant_schema();
    schema = S(schema).edge(&prop("root", "u", "y")).build();
    let graph = GInstance::new()
        .with_node(Node::new(0, "root"))
        .with_node(Node::new(1, "b"))
        .with_node(Node::new(2, "u"))
        .with_edge(0, 1, prop("root", "u", "y"))
        .with_edge(0, 2, prop("root", "a", "x"));
    let report = validate_graph(&graph, &schema, &Protocol::default());
    assert!(of_kind(&report, CheckKind::EndpointCoherence).is_empty());
}

#[test]
fn a_required_edge_is_met_only_by_an_edge_to_its_target() {
    let to_a = prop("post", "a", "x");
    let to_b = prop("post", "b", "x");
    let schema = S::default()
        .vertex("post", "object")
        .vertex("a", "object")
        .vertex("b", "object")
        .edge(&to_a)
        .edge(&to_b)
        .required("post", &to_a)
        .build();
    let graph = GInstance::new()
        .with_node(Node::new(0, "post"))
        .with_node(Node::new(1, "b"))
        .with_edge(0, 1, to_b);
    let report = validate_graph(&graph, &schema, &Protocol::default());
    assert_eq!(
        of_kind(&report, CheckKind::RequiredEdge),
        vec![(
            Some(ViewRef::Node(0)),
            Violation::MissingRequiredEdge { edge: to_a.clone() }
        )]
    );
    let graph = GInstance::new()
        .with_node(Node::new(0, "post"))
        .with_node(Node::new(1, "a"))
        .with_edge(0, 1, to_a);
    assert!(validate_graph(&graph, &schema, &Protocol::default()).passed());
}

#[test]
fn decoding_a_report_with_unordered_entries_is_rejected() -> TestResult {
    let report = validate_graph(
        &GInstance::new()
            .with_node(Node::new(0, "ghost"))
            .with_node(Node::new(1, "ghost")),
        &Schema::default(),
        &Protocol::default(),
    );
    assert!(report.diagnostics.len() >= 2);
    round_trip(&report)?;
    let mut json = serde_json::to_value(&report)?;
    if let Some(diagnostics) = json["diagnostics"].as_array_mut() {
        diagnostics.reverse();
    }
    let err = serde_json::from_value::<ValidationReport>(json).expect_err("unordered entries");
    assert!(err.to_string().contains("order"), "{err}");
    Ok(())
}

#[test]
fn decoding_a_report_with_unordered_incomplete_checks_is_rejected() -> TestResult {
    let schema = S::default()
        .vertex("a", "string")
        .constraint("a", "mystery", "1")
        .constraint("a", "mystery", "2")
        .constraint("a", "mystery", "3")
        .build();
    let report = validate_graph(
        &GInstance::new().with_node(str_node(0, "a", "x")),
        &schema,
        &Protocol::default(),
    );
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert!(report.incomplete.len() >= 2, "{:?}", report.incomplete);
    round_trip(&report)?;
    let mut json = serde_json::to_value(&report)?;
    if let Some(incomplete) = json["incomplete"].as_array_mut() {
        incomplete.reverse();
    }
    let err = serde_json::from_value::<ValidationReport>(json).expect_err("unordered incompletes");
    assert!(
        err.to_string()
            .contains("report incomplete checks are not in canonical order"),
        "{err}"
    );
    Ok(())
}

#[test]
fn an_incomplete_check_without_subject_covers_every_instance() {
    let schema = S::default()
        .vertex("a", "string")
        .constraint("a", "mystery", "1")
        .build();
    let plan = plan(&schema, &Protocol::default());
    let key = CheckKey::new(
        CheckKind::ValueConstraint,
        SchemaRef::Constraint {
            vertex: "a".into(),
            constraint: Constraint {
                sort: "mystery".into(),
                value: "1".into(),
            },
        },
    );
    for count in [0u32, 1, 3] {
        let mut graph = GInstance::new();
        for id in 0..count {
            graph = graph.with_node(str_node(id, "a", "x"));
        }
        let report = plan.validate_graph(&graph, &unlimited(), &none());
        let entries: Vec<&IncompleteCheck> = report
            .incomplete
            .iter()
            .filter(|i| i.check == key)
            .collect();
        assert_eq!(entries.len(), 1, "one entry for the whole check");
        assert_eq!(entries[0].subject, None);
        let cov = report.coverage[&key];
        assert_eq!(cov.instances, u64::from(count));
        assert_eq!(cov.incomplete, cov.instances, "every instance undecided");
        assert_eq!((cov.passed, cov.failed), (0, 0));
        assert!(report.diagnostics.iter().all(|d| d.check != key));
    }
}

#[test]
fn validate_attributes_is_the_tree_attribute_projection() {
    let schema = S::default()
        .vertex("post", "object")
        .vertex("post.title", "string")
        .edge(&prop("post", "post.title", "title"))
        .build();
    let mut node = Node::new(0, "post");
    node.extra_fields.insert("title".into(), Value::Int(3));
    node.extra_fields.insert("stray".into(), Value::Int(4));
    let tree = WInstance::new(HashMap::from([(0, node)]), vec![], vec![], 0, "post".into());
    let found = panproto_inst::validate_attributes(&schema, &tree);
    assert_eq!(found.len(), 2, "{found:?}");
}

/// The subjects of the reserved-vocabulary diagnostics of a tree.
fn reserved_subjects(tree: &WInstance, schema: &Schema) -> Vec<Option<ViewRef>> {
    of_kind(
        &validate_wtype(tree, schema, &Protocol::default()),
        CheckKind::ReservedVocabulary,
    )
    .into_iter()
    .map(|(subject, _)| subject)
    .collect()
}

#[test]
fn validation_reports_the_reserved_vocabulary_the_tree_constructor_refuses() {
    use panproto_inst::ginstance::{HYPER_LABEL_KIND, HYPER_PARENT_KIND};
    use panproto_inst::{InstError, wtype_check};
    let schema = S::default()
        .vertex("doc", "object")
        .vertex("p", "object")
        .edge(&edge("doc", "p", "prop", None))
        .edge(&edge("doc", "p", HYPER_PARENT_KIND, None))
        .build();
    let two_nodes = || HashMap::from([(0, Node::new(0, "doc")), (1, Node::new(1, "p"))]);
    // A non-fan node of the fan shape.
    let mut nodes = two_nodes();
    nodes.insert(1, Node::new(1, "p").with_shape(NodeShape::HyperEdge));
    let arcs = vec![(0, 1, edge("doc", "p", "prop", None))];
    let tree = WInstance::new(nodes.clone(), arcs.clone(), vec![], 0, "doc".into());
    assert_eq!(
        reserved_subjects(&tree, &schema),
        vec![Some(ViewRef::Node(1))]
    );
    assert!(WInstance::try_new(nodes, arcs, vec![], 0, "doc".into()).is_err());
    // An arc of the reserved parent kind.
    let arcs = vec![(0, 1, edge("doc", "p", HYPER_PARENT_KIND, None))];
    let tree = WInstance::new(two_nodes(), arcs.clone(), vec![], 0, "doc".into());
    assert_eq!(
        reserved_subjects(&tree, &schema),
        vec![Some(ViewRef::Arc {
            parent: 0,
            child: 1
        })]
    );
    assert!(WInstance::try_new(two_nodes(), arcs, vec![], 0, "doc".into()).is_err());
    // The trees of the two refusals of `from_wtype`. A root of the fan shape
    // with a reserved-kind arc: both are reported, under any schema.
    let root = Node::new(0, "doc").with_shape(NodeShape::HyperEdge);
    let tree = WInstance::new(
        HashMap::from([(0, root), (1, Node::new(1, "para"))]),
        vec![(0, 1, edge("doc", "para", HYPER_PARENT_KIND, None))],
        vec![],
        0,
        "doc".into(),
    );
    assert_eq!(
        reserved_subjects(&tree, &Schema::default()),
        vec![
            Some(ViewRef::Node(0)),
            Some(ViewRef::Arc {
                parent: 0,
                child: 1
            })
        ]
    );
    // Node 1 has the form of a fan node, with no incoming arc: the fan
    // shape, a `hyper_parent` arc and a named `hyper_label` arc to node 0.
    let orphan = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "sent")),
            (1, Node::new(1, "arg").with_shape(NodeShape::HyperEdge)),
        ]),
        vec![
            (1, 0, edge("arg", "sent", HYPER_PARENT_KIND, None)),
            (1, 0, edge("arg", "sent", HYPER_LABEL_KIND, Some("theme"))),
        ],
        vec![],
        0,
        "sent".into(),
    );
    assert_eq!(
        reserved_subjects(&orphan, &Schema::default()),
        vec![
            Some(ViewRef::Node(1)),
            Some(ViewRef::Arc {
                parent: 1,
                child: 0
            })
        ]
    );
    // For each, the subjects are the reserved vocabulary the tree check
    // finds, and the validating constructor refuses the tree.
    for tree in [&tree, &orphan] {
        let found: Vec<Option<ViewRef>> = wtype_check::reserved_vocabulary(tree)
            .into_iter()
            .map(Some)
            .collect();
        assert_eq!(reserved_subjects(tree, &Schema::default()), found);
        let refused = WInstance::try_new(
            tree.nodes.clone(),
            tree.arcs.clone(),
            tree.fans.clone(),
            tree.root,
            tree.schema_root.clone(),
        );
        assert!(
            matches!(refused, Err(InstError::ReservedVocabulary { .. })),
            "{refused:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Fans of an undeclared hyper-edge, absent children, and naming tree fans.
// ---------------------------------------------------------------------------

/// A root `doc` with children 1 and 2, and the hyper-edges `a`, `b` (each
/// with the signature `{x}`) and `h` (with the signature `{x, y}`).
fn three_doc_tree(fans: Vec<Fan>) -> (Schema, WInstance) {
    let schema = S::default()
        .vertex("doc", "object")
        .edge(&prop("doc", "doc", "c"))
        .hyper_edge("a", &[("x", "doc")], "head")
        .hyper_edge("b", &[("x", "doc")], "head")
        .hyper_edge("h", &[("x", "doc"), ("y", "doc")], "head")
        .build();
    let mut tree = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "doc")),
            (1, Node::new(1, "doc")),
            (2, Node::new(2, "doc")),
        ]),
        vec![
            (0, 1, prop("doc", "doc", "c")),
            (0, 2, prop("doc", "doc", "c")),
        ],
        vec![],
        0,
        "doc".into(),
    );
    tree.fans = fans;
    (schema, tree)
}

#[test]
fn tree_fan_diagnostics_tell_apart_fans_that_differ_only_in_hyper_edge() {
    let (schema, tree) = three_doc_tree(vec![
        Fan::new("a", 0).with_child("lab", 1),
        Fan::new("b", 0).with_child("lab", 1),
    ]);
    let report = validate_wtype(&tree, &schema, &Protocol::default());
    let entries = fan_entries(&report);
    assert_eq!(entries.len(), 2, "{entries:?}");
    let unknown = || FanViolation::UnknownLabel {
        label: "lab".into(),
    };
    assert_eq!(
        entries,
        vec![
            tree_fan_entry(&tree.fans[0], &[0, 1], unknown()),
            tree_fan_entry(&tree.fans[1], &[0, 1], unknown()),
        ]
    );
}

#[test]
fn tree_fan_diagnostics_tell_apart_fans_that_differ_only_in_label_assignment() {
    let (schema, tree) = three_doc_tree(vec![
        Fan::new("h", 0)
            .with_child("x", 1)
            .with_child("y", 2)
            .with_child("z", 1),
        Fan::new("h", 0)
            .with_child("x", 2)
            .with_child("y", 1)
            .with_child("z", 1),
    ]);
    let report = validate_wtype(&tree, &schema, &Protocol::default());
    let entries = fan_entries(&report);
    assert_eq!(entries.len(), 2, "{entries:?}");
    let unknown = || FanViolation::UnknownLabel { label: "z".into() };
    assert_eq!(
        entries,
        vec![
            tree_fan_entry(&tree.fans[0], &[0, 1, 2], unknown()),
            tree_fan_entry(&tree.fans[1], &[0, 1, 2], unknown()),
        ]
    );
}

#[test]
fn a_label_less_fan_of_an_undeclared_hyper_edge_leaves_the_fan_domain() {
    use panproto_inst::ginstance::{HYPER_PARENT_KIND, unreify_fans, unreify_fans_with_schema};
    use panproto_inst::{FanDomain, InstError};
    let g = GInstance::new()
        .with_node(Node::new(0, "item"))
        .with_node(Node::new(5, "ghost").with_shape(NodeShape::HyperEdge))
        .with_edge(5, 0, edge("ghost", "item", HYPER_PARENT_KIND, None));
    assert!(
        FanDomain::check(&g).is_ok(),
        "no schema, no hyper-edge check"
    );
    let ghost = FanViolation::UnknownHyperEdge {
        hyper_edge: "ghost".into(),
    };
    let checked = FanDomain::check_with_schema(&g, &Schema::default());
    assert!(
        matches!(
            &checked,
            Err(InstError::MalformedFan { node: 5, reason }) if reason == &ghost
        ),
        "{checked:?}"
    );
    assert_eq!(
        FanDomain::violations_at(&g, 5, Some(&Schema::default())),
        vec![ghost.clone()]
    );
    assert!(unreify_fans(&g).is_ok(), "no schema, no hyper-edge check");
    let unreified = unreify_fans_with_schema(&g, &Schema::default());
    assert!(
        matches!(
            &unreified,
            Err(InstError::MalformedFan { node: 5, reason }) if reason == &ghost
        ),
        "{unreified:?}"
    );
}

/// A doc tree of three nodes, the root and two children under `c`, with a
/// schema that declares the vertex `doc` and the hyper-edge `h`; node 2 is
/// anchored at `anchor` and has the fan shape.
fn doc_tree_with_fan_shaped_leaf(anchor: &str, required: bool) -> (Schema, WInstance) {
    let mut schema = S::default()
        .vertex("doc", "object")
        .edge(&prop("doc", "doc", "c"))
        .hyper_edge("h", &[], "p");
    if required {
        schema = schema.required("doc", &prop("doc", "doc", "c"));
    }
    let tree = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "doc")),
            (1, Node::new(1, "doc")),
            (2, Node::new(2, anchor).with_shape(NodeShape::HyperEdge)),
        ]),
        vec![
            (0, 1, prop("doc", "doc", "c")),
            (0, 2, prop("doc", "doc", "c")),
        ],
        vec![],
        0,
        "doc".into(),
    );
    (schema.build(), tree)
}

/// The `ReservedVocabulary` subjects and the `NodeAnchor` findings of the
/// tree of [`doc_tree_with_fan_shaped_leaf`] with node 2 anchored at
/// `anchor`.
fn fan_shaped_leaf_anchor_findings(anchor: &str) -> Vec<(Option<ViewRef>, Violation)> {
    let (schema, tree) = doc_tree_with_fan_shaped_leaf(anchor, false);
    let report = validate_wtype(&tree, &schema, &Protocol::default());
    assert_eq!(
        of_kind(&report, CheckKind::ReservedVocabulary)
            .into_iter()
            .map(|(subject, _)| subject)
            .collect::<Vec<_>>(),
        vec![Some(ViewRef::Node(2))]
    );
    of_kind(&report, CheckKind::NodeAnchor)
}

#[test]
fn a_tree_node_of_the_fan_shape_anchored_at_a_declared_vertex_has_a_known_anchor() {
    // One fault, the reserved vocabulary: the anchor is a declared vertex.
    assert_eq!(fan_shaped_leaf_anchor_findings("doc"), vec![]);
}

#[test]
fn a_tree_node_of_the_fan_shape_anchored_at_a_hyper_edge_has_an_unknown_anchor() {
    // A hyper-edge is not a vertex, and every tree node is anchored at one.
    assert_eq!(
        fan_shaped_leaf_anchor_findings("h"),
        vec![(
            Some(ViewRef::Node(2)),
            Violation::UnknownAnchor { anchor: "h".into() }
        )]
    );
}

#[test]
fn a_tree_node_of_the_fan_shape_is_checked_for_its_required_edges() {
    let (schema, tree) = doc_tree_with_fan_shaped_leaf("doc", true);
    let report = validate_wtype(&tree, &schema, &Protocol::default());
    assert_eq!(
        of_kind(&report, CheckKind::RequiredEdge)
            .into_iter()
            .map(|(subject, _)| subject)
            .collect::<Vec<_>>(),
        vec![Some(ViewRef::Node(1)), Some(ViewRef::Node(2))]
    );
}

/// A tree whose root, node 0 anchored at `doc`, holds `child` (node 1) with
/// the fan shape under `child_edge`, and node 1 holds each `(id, anchor)` of
/// `grandchildren` under `grandchild_edge`.
fn tree_with_fan_shaped_child(
    child: Node,
    child_edge: &Edge,
    grandchildren: &[(u32, &str)],
    grandchild_edge: &Edge,
) -> WInstance {
    let mut nodes = HashMap::from([
        (0, Node::new(0, "doc")),
        (1, child.with_shape(NodeShape::HyperEdge)),
    ]);
    let mut arcs = vec![(0, 1, child_edge.clone())];
    for &(id, anchor) in grandchildren {
        nodes.insert(id, Node::new(id, anchor));
        arcs.push((1, id, grandchild_edge.clone()));
    }
    WInstance::new(nodes, arcs, vec![], 0, "doc".into())
}

#[test]
fn a_tree_node_of_the_fan_shape_is_checked_for_its_value() {
    let a = prop("doc", "a", "a");
    let schema = S::default()
        .vertex("doc", "object")
        .vertex("a", "string")
        .edge(&a)
        .constraint("a", "maxLength", "3")
        .build();
    let p = protocol(&[(
        "maxLength",
        ConstraintSemantics::MaxLength(LengthUnit::Chars),
    )]);
    let tree = tree_with_fan_shaped_child(str_node(1, "a", "abcd"), &a, &[], &a);
    let report = validate_wtype(&tree, &schema, &p);
    assert_eq!(
        of_kind(&report, CheckKind::ValueConstraint),
        vec![(
            Some(ViewRef::Node(1)),
            Violation::ValueConstraint {
                sort: "maxLength".into(),
                bound: "3".into(),
                value: Some(Value::Str("abcd".into())),
            }
        )]
    );
}

#[test]
fn a_tree_node_of_the_fan_shape_is_checked_for_its_attributes() {
    // `doc` declares the scalar attribute `title`, so the attributes of a
    // node anchored there are checked: `title` is mistyped, `stray` is
    // undeclared.
    let c = prop("doc", "doc", "c");
    let schema = S::default()
        .vertex("doc", "object")
        .vertex("doc.title", "string")
        .edge(&c)
        .edge(&prop("doc", "doc.title", "title"))
        .build();
    let mut child = Node::new(1, "doc");
    child.extra_fields.insert("title".into(), Value::Int(3));
    child.extra_fields.insert("stray".into(), Value::Int(4));
    let tree = tree_with_fan_shaped_child(child, &c, &[], &c);
    let report = validate_wtype(&tree, &schema, &Protocol::default());
    let found = of_kind(&report, CheckKind::Attribute);
    assert_eq!(
        found
            .iter()
            .map(|(subject, violation)| (
                subject.clone(),
                match violation {
                    Violation::UndeclaredAttribute { key }
                    | Violation::MistypedAttribute { key, .. } => {
                        key.clone()
                    }
                    other => panic!("not an attribute violation: {other:?}"),
                }
            ))
            .collect::<Vec<_>>(),
        vec![
            (Some(ViewRef::Node(1)), "stray".to_owned()),
            (Some(ViewRef::Node(1)), "title".to_owned()),
        ],
        "{found:?}"
    );
}

#[test]
fn a_tree_node_of_the_fan_shape_is_checked_for_its_out_degree() {
    let c = prop("doc", "doc", "c");
    let schema = S::default()
        .vertex("doc", "object")
        .edge(&c)
        .constraint("doc", "maxOut", "1")
        .build();
    let p = protocol(&[("maxOut", ConstraintSemantics::MaxOutDegree)]);
    let tree = tree_with_fan_shaped_child(Node::new(1, "doc"), &c, &[(2, "doc"), (3, "doc")], &c);
    let report = validate_wtype(&tree, &schema, &p);
    assert_eq!(
        of_kind(&report, CheckKind::OutDegree)
            .into_iter()
            .map(|(subject, violation)| (
                subject,
                matches!(violation, Violation::OutDegree { found: 2, .. })
            ))
            .collect::<Vec<_>>(),
        vec![(Some(ViewRef::Node(1)), true)]
    );
}

/// A graph that may hold reserved vocabulary, as the adversarial generator
/// of the fan reification tests builds it (nodes of the fan shape, edges of
/// the reserved kinds between any nodes, so fan-shaped nodes may nest, and
/// ordinary edges), with each fan-shaped node anchored at the declared
/// hyper-edge `rel` (signature `{a, b}`) or the undeclared `ghost`, and
/// labels drawn from `a` to `c`, so every condition of the fan domain and
/// both schema conditions occur.
fn arb_adversarial_fan_graph() -> impl Strategy<Value = GInstance> {
    (1u32..7)
        .prop_flat_map(|n| {
            let shapes = prop::collection::vec(
                prop::sample::select(vec![None, None, None, Some("rel"), Some("ghost")]),
                n as usize,
            );
            let edges = prop::collection::vec(
                (
                    0..n,
                    0..n,
                    prop::sample::select(vec![
                        "link",
                        "link",
                        panproto_inst::ginstance::HYPER_PARENT_KIND,
                        panproto_inst::ginstance::HYPER_LABEL_KIND,
                        panproto_inst::ginstance::HYPER_LABEL_KIND,
                    ]),
                    prop::option::weighted(0.8, "[a-c]"),
                ),
                0..10,
            );
            (Just(n), shapes, edges)
        })
        .prop_map(|(n, shapes, edges)| {
            let mut graph = GInstance::new();
            for (id, fan) in (0..n).zip(shapes) {
                graph = graph.with_node(fan.map_or_else(
                    || Node::new(id, "item"),
                    |he| Node::new(id, he).with_shape(NodeShape::HyperEdge),
                ));
            }
            for (src, tgt, kind, name) in edges {
                graph = graph.with_edge(src, tgt, edge("item", "item", kind, name.as_deref()));
            }
            graph
        })
}

/// The schema of [`arb_adversarial_fan_graph`]: the vertex `item` and the
/// hyper-edge `rel` with the signature `{a, b}`.
fn adversarial_fan_schema() -> Schema {
    S::default()
        .vertex("item", "object")
        .hyper_edge("rel", &[("a", "item"), ("b", "item")], "head")
        .build()
}

/// The reasons of the `FanWellFormed` diagnostics of a graph report, by
/// subject node.
fn fan_reasons_by_node(report: &ValidationReport) -> BTreeMap<u32, Vec<FanViolation>> {
    let mut by_node: BTreeMap<u32, Vec<FanViolation>> = BTreeMap::new();
    for (subject, _, violation) in fan_entries(report) {
        let (Some(ViewRef::Node(node)), Violation::MalformedFan { reason, .. }) =
            (subject, violation)
        else {
            panic!("a graph fan diagnostic names its fan node");
        };
        by_node.entry(node).or_default().push(reason);
    }
    by_node
}

proptest! {
    #[test]
    fn fan_domain_reports_the_first_fan_violation_the_validator_reports(
        g in arb_adversarial_fan_graph(),
    ) {
        use panproto_inst::{FanDomain, InstError};
        let schema = adversarial_fan_schema();
        let report = validate_graph(&g, &schema, &Protocol::default());
        let by_node = fan_reasons_by_node(&report);
        match FanDomain::check_with_schema(&g, &schema) {
            Ok(()) => prop_assert!(by_node.is_empty(), "{by_node:?}"),
            Err(InstError::MalformedFan { node, reason }) => {
                let first = by_node.iter().next();
                prop_assert_eq!(first.map(|(n, _)| *n), Some(node));
                prop_assert!(by_node[&node].contains(&reason), "{reason:?} not in {by_node:?}");
                // It is the first violation of the least offending node.
                let at = FanDomain::violations_at(&g, node, Some(&schema));
                prop_assert_eq!(at.first(), Some(&reason));
            }
            Err(other) => prop_assert!(false, "unexpected error {other:?}"),
        }
        // The validator reports every violation `violations_at` lists, at
        // every node, and nothing else.
        let ids: BTreeSet<u32> = g.nodes().keys().copied().collect();
        let mut listed: BTreeMap<u32, Vec<FanViolation>> = BTreeMap::new();
        for &id in &ids {
            let mut at = FanDomain::violations_at(&g, id, Some(&schema));
            if !at.is_empty() {
                at.sort();
                listed.insert(id, at);
            }
        }
        let mut reported = by_node;
        for reasons in reported.values_mut() {
            reasons.sort();
        }
        prop_assert_eq!(&reported, &listed);
        // Without a schema, `check` reports the first violation of the least
        // node `violations_at` faults with no schema.
        let first_unschema = ids.iter().find_map(|&id| {
            FanDomain::violations_at(&g, id, None).into_iter().next().map(|r| (id, r))
        });
        match FanDomain::check(&g) {
            Ok(()) => prop_assert_eq!(first_unschema, None),
            Err(InstError::MalformedFan { node, reason }) => {
                prop_assert_eq!(first_unschema, Some((node, reason)));
            }
            Err(other) => prop_assert!(false, "unexpected error {other:?}"),
        }
    }
}
