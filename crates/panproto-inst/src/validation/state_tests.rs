//! Unit tests of the incremental validation state that observe how many
//! check instances are evaluated, which only the crate's own counter can.

use std::sync::Arc;

use panproto_gat::CheckStatus;
use panproto_schema::{
    Cardinality, Edge, InstanceShapeKind, Key, KeyPart, KeyRole, KeyScope, OnUnresolved, Protocol,
    RefSource, Reference, ResolveMode, Schema, Vertex,
};

use super::checks::{reset_semantic_evaluations, semantic_evaluations};
use super::{
    AdmissionPolicy, CheckKind, EditRejection, ExternalVerdicts, IncompleteReason, ReportScope,
    ValidatedGraph, ValidatedShape, ValidationBudget, ValidationOptions, ValidationPlan,
};
use crate::ginstance::GInstance;
use crate::graph_edit::{GraphDelta, GraphEdit};
use crate::metadata::Node;
use crate::value::{FieldPresence, Value};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn declare(schema: &mut Schema, e: &Edge) {
    schema.edges.insert(e.clone(), e.kind.clone());
    schema
        .outgoing
        .entry(e.src.clone())
        .or_default()
        .push(e.clone());
    schema
        .incoming
        .entry(e.tgt.clone())
        .or_default()
        .push(e.clone());
    schema
        .between
        .entry((e.src.clone(), e.tgt.clone()))
        .or_default()
        .push(e.clone());
}

/// Items named through `name`, keyed by that name, and holders whose `tag`
/// names an item by its key.
fn reference_plan() -> Arc<ValidationPlan> {
    let mut schema = Schema::default();
    for (id, kind) in [
        ("item", "object"),
        ("holder", "object"),
        ("name", "string"),
        ("tag", "string"),
    ] {
        schema.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: kind.into(),
                nsid: None,
            },
        );
    }
    let name = edge("item", "name", "name");
    let tag = edge("holder", "tag", "tag");
    let link = Edge {
        kind: "ref".into(),
        ..edge("holder", "item", "link")
    };
    for e in [&name, &tag, &link] {
        declare(&mut schema, e);
    }
    schema.declarations.insert_key(
        "item".into(),
        Key {
            id: "pk".into(),
            parts: vec![KeyPart::Path(vec![name])],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
        },
    );
    schema.declarations.insert_reference(Reference {
        id: "holder_ref".into(),
        holder: "holder".into(),
        source: RefSource::KeyValue {
            parts: vec![vec![tag]],
        },
        target_vertex: "item".into(),
        target_key: Some("pk".into()),
        edge: Some(link),
        cardinality: Cardinality::One,
        nullable: true,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
    });
    Arc::new(ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    ))
}

/// `items` items named `0..items` and `holders` holders naming item 0.
fn reference_graph(items: u32, holders: u32) -> GInstance {
    let mut graph = GInstance::new();
    let mut next = 0_u32;
    let mut fresh = || {
        next += 1;
        next - 1
    };
    for i in 0..items {
        let (item, value) = (fresh(), fresh());
        graph = graph
            .with_node(Node::new(item, "item"))
            .with_node(
                Node::new(value, "name")
                    .with_value(FieldPresence::Present(Value::Str(i.to_string()))),
            )
            .with_edge(item, value, edge("item", "name", "name"));
    }
    for _ in 0..holders {
        let (holder, value) = (fresh(), fresh());
        graph = graph
            .with_node(Node::new(holder, "holder"))
            .with_node(
                Node::new(value, "tag").with_value(FieldPresence::Present(Value::Str("0".into()))),
            )
            .with_edge(holder, value, edge("holder", "tag", "tag"));
    }
    graph
}

fn zero() -> ValidationBudget {
    ValidationBudget { max_reads: Some(0) }
}

/// Initialization evaluates exactly the instances whole-instance validation
/// evaluates under the same budget: under a zero budget, only instances
/// with an empty trace, and never a budget-skipped one.
#[test]
fn a_zero_budget_initialization_evaluates_no_check_instance() {
    let plan = reference_plan();
    let graph = reference_graph(4, 4);
    reset_semantic_evaluations();
    let whole = plan.validate_graph(&graph, &zero(), &ExternalVerdicts::default());
    let by_validation = semantic_evaluations();
    reset_semantic_evaluations();
    let (validated, report) =
        ValidatedGraph::new(plan, graph, &zero(), &ExternalVerdicts::default());
    assert_eq!(semantic_evaluations(), by_validation);
    assert_eq!(report.canonical_bytes(), whole.canonical_bytes());
    assert_eq!(report.status, CheckStatus::Incomplete);
    let cut = validated
        .state()
        .budget_cuts
        .values()
        .next()
        .cloned()
        .unwrap_or_else(|| panic!("a zero budget cuts the schedule"));
    assert_eq!(by_validation + cut.skipped.len(), cut.scheduled.len());
    assert!(cut.skipped.len() > by_validation);
}

#[test]
fn a_zero_budget_edit_evaluates_no_check_instance() -> TestResult {
    let plan = reference_plan();
    let graph = reference_graph(4, 4);
    let (mut validated, _) = ValidatedGraph::new(
        plan,
        graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let edit = GraphEdit::AddNode {
        node: Node::new(1000, "item"),
    };
    reset_semantic_evaluations();
    let checked = validated.check_edit(&edit, &zero(), &ExternalVerdicts::default())?;
    assert_eq!(semantic_evaluations(), 0, "check_edit evaluated");
    assert_eq!(
        checked.report.status,
        CheckStatus::Incomplete,
        "{:#?}",
        checked.report.diagnostics
    );
    reset_semantic_evaluations();
    let result = validated.apply_edit(
        &edit,
        AdmissionPolicy::NoNewFailures,
        &zero(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(semantic_evaluations(), 0, "apply_edit evaluated");
    assert!(matches!(result, Err(EditRejection::NotAdmitted { .. })));
    Ok(())
}

#[test]
fn reattachment_replays_a_budget_cut_without_evaluating_skipped_instances() -> TestResult {
    let plan = reference_plan();
    let graph = reference_graph(4, 4);
    let (validated, _) = ValidatedGraph::new(
        plan.clone(),
        graph,
        &ValidationBudget { max_reads: Some(5) },
        &ExternalVerdicts::default(),
    );
    let skipped = validated
        .state()
        .budget_cuts
        .values()
        .next()
        .map_or(0, |cut| cut.skipped.len());
    let scheduled = validated
        .state()
        .budget_cuts
        .values()
        .next()
        .map_or(0, |cut| cut.scheduled.len());
    assert!(skipped > 0 && skipped < scheduled);
    let (graph, state) = validated.into_parts();
    reset_semantic_evaluations();
    ValidatedGraph::from_parts(plan, graph, state)?;
    // Only the evaluated prefix is evaluated again, to check the stored
    // outcome bundles; the replayed schedule and its skipped suffix are not.
    assert_eq!(semantic_evaluations(), scheduled - skipped);
    Ok(())
}

#[test]
fn the_admitted_sequence_number_refuses_to_wrap() -> TestResult {
    let plan = reference_plan();
    let (validated, _) = ValidatedGraph::new(
        plan.clone(),
        reference_graph(1, 0),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let (graph, mut state) = validated.into_parts();
    state.seq = u64::MAX - 1;
    let mut validated = ValidatedGraph::from_parts(plan, graph, state)?;
    let add = |id: u32| GraphEdit::AddNode {
        node: Node::new(id, "item"),
    };
    let first = validated.apply_edit(
        &add(100),
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(first.baseline_seq, u64::MAX - 1);
    assert_eq!(validated.state().seq, u64::MAX);
    let before = validated.state().canonical_bytes();
    let second = validated.apply_edit(
        &add(101),
        AdmissionPolicy::RequirePassed,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert!(
        matches!(
            second,
            Err(EditRejection::SequenceExhausted { seq: u64::MAX })
        ),
        "{second:?}"
    );
    assert_eq!(validated.state().canonical_bytes(), before);
    assert!(validated.instance().node(101).is_none());
    Ok(())
}

#[test]
fn delta_local_validation_evaluates_global_kinds_in_full() -> TestResult {
    let plan = reference_plan();
    let graph = reference_graph(2, 2);
    let full = plan.validate_graph(
        &graph,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(full.status, CheckStatus::Passed);
    let identity = plan.check_graph_delta(
        &graph,
        &GraphDelta::identity(),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(identity.scope, ReportScope::DeltaLocal);
    assert_eq!(
        identity.status,
        CheckStatus::Passed,
        "{:#?}",
        identity.diagnostics
    );
    for kind in [CheckKind::KeyUnique, CheckKind::ReferenceResolves] {
        let (key, coverage) = identity
            .coverage
            .iter()
            .find(|(key, _)| key.kind == kind)
            .ok_or("the plan compiles the global kind")?;
        assert_eq!(coverage, &full.coverage[key], "{kind:?}");
    }
    assert!(
        identity
            .incomplete
            .iter()
            .all(|entry| entry.reason != IncompleteReason::NoValidatedBaseline)
    );
    // Under a budget too small for the global instances, each global key
    // that was not evaluated in full says so.
    let cut = plan.check_graph_delta(
        &graph,
        &GraphDelta::identity(),
        &zero(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(cut.status, CheckStatus::Incomplete);
    for kind in [CheckKind::KeyUnique, CheckKind::ReferenceResolves] {
        assert!(cut.incomplete.iter().any(|entry| entry.check.kind == kind
            && entry.reason == IncompleteReason::NoValidatedBaseline));
    }
    Ok(())
}

#[test]
fn initialization_materializes_both_maintained_index_families() {
    let plan = reference_plan();
    let (validated, report) = ValidatedGraph::new(
        plan,
        reference_graph(2, 2),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert_eq!(report.status, CheckStatus::Passed);

    let key_kinds: Vec<_> = validated
        .state()
        .key_indexes
        .keys()
        .map(|key| key.kind)
        .collect();
    let reference_kinds: Vec<_> = validated
        .state()
        .reference_indexes
        .keys()
        .map(|key| key.kind)
        .collect();
    assert_eq!(key_kinds, [CheckKind::KeyUnique]);
    assert_eq!(reference_kinds, [CheckKind::ReferenceResolves]);
    assert!(
        validated
            .state()
            .key_indexes
            .values()
            .all(|index| !index.values.is_empty())
    );
    assert!(
        validated
            .state()
            .reference_indexes
            .values()
            .all(|index| !index.values.is_empty())
    );
}

#[test]
fn an_inapplicable_delta_is_refused_by_the_delta_local_checkers() {
    let mut schema = Schema::default();
    schema.vertices.insert(
        "item".into(),
        Vertex {
            id: "item".into(),
            kind: "object".into(),
            nsid: None,
        },
    );
    let plan = ValidationPlan::compile(
        &schema,
        &Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        },
        &ValidationOptions::default(),
    );
    let graph = GInstance::new().with_node(Node::new(0, "item"));
    // A delta that adds a node at an unknown anchor and deletes a node the
    // graph does not have: it does not apply.
    let ghost = GraphEdit::Batch(vec![
        GraphEdit::AddNode {
            node: Node::new(1, "ghost"),
        },
        GraphEdit::RemoveNode {
            node: Node::new(99, "item"),
            incident: Vec::new(),
        },
    ])
    .to_delta();
    assert!(ghost.is_ok(), "{ghost:?}");
    let Ok(ghost) = ghost else { return };
    assert!(ghost.apply(&mut graph.clone()).is_err());
    let result = plan.check_graph_delta(
        &graph,
        &ghost,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert!(
        matches!(result, Err(crate::edit_error::EditError::NodeNotFound(99))),
        "{result:?}"
    );
}

#[test]
fn validation_state_canonical_bytes_distinguish_sequence_numbers() {
    let plan = reference_plan();
    let (validated, _) = ValidatedGraph::new(
        plan,
        reference_graph(1, 1),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let initial = validated.state().canonical_bytes();
    let mut successor = validated.state().clone();
    successor.seq += 1;

    assert!(initial.len() > 1);
    assert_ne!(initial, successor.canonical_bytes());
}

#[test]
fn shape_evaluation_returns_the_reads_of_the_compiled_check() {
    let plan = reference_plan();
    let graph = reference_graph(1, 1);
    let external = ExternalVerdicts::default();
    let instance = graph
        .check_instances(&plan)
        .into_iter()
        .find(|instance| instance.check.kind == CheckKind::ReferenceResolves)
        .unwrap_or_else(|| panic!("the reference plan compiles a reference instance"));
    let separately_traced = graph.reads(&plan, &external, &instance);
    let (_, evaluated_reads) = graph.evaluate(&plan, &external, &instance);

    assert!(!separately_traced.is_empty());
    assert_eq!(evaluated_reads, separately_traced);
}
