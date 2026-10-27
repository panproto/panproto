//! Small countermodels for the withdrawn validation-closure claim.
//!
//! Each pair is operationally independent and each member preserves a valid
//! baseline, while the merged delta violates one compiled check.

use std::collections::BTreeSet;
use std::sync::Arc;

use panproto_gat::CheckStatus;
use panproto_inst::graph_edit::{AttrKey, AttrVal, GraphDelta};
use panproto_inst::validation::{
    ExternalVerdicts, ValidatedGraph, ValidationBudget, ValidationOptions, ValidationPlan,
};
use panproto_inst::{EdgeId, FieldPresence, GEdge, GInstance, Node, Value};
use panproto_schema::{
    Constraint, ConstraintSemantics, Edge, InstanceShapeKind, Key, KeyPart, KeyRole, KeyScope,
    Protocol, Schema, Vertex,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Feature {
    MaxOut,
    RequiredName,
    Ordered,
    Key,
    Acyclic,
}

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn e_items() -> Edge {
    edge("doc", "item", "items")
}

fn e_name() -> Edge {
    edge("item", "name", "name")
}

fn e_next() -> Edge {
    edge("item", "item", "next")
}

fn plan(features: impl IntoIterator<Item = Feature>) -> ValidationPlan {
    let features: BTreeSet<Feature> = features.into_iter().collect();
    let mut schema = Schema::default();
    for id in ["doc", "item", "name"] {
        schema.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: "object".into(),
                nsid: None,
            },
        );
    }
    for edge in [e_items(), e_name(), e_next()] {
        schema.edges.insert(edge.clone(), edge.kind.clone());
        schema
            .outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        schema
            .incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        schema
            .between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge);
    }
    if features.contains(&Feature::MaxOut) {
        schema
            .constraints
            .entry("item".into())
            .or_default()
            .push(Constraint {
                sort: "maxOut".into(),
                value: "1".into(),
            });
    }
    if features.contains(&Feature::RequiredName) {
        schema.required.insert("item".into(), vec![e_name()]);
    }
    if features.contains(&Feature::Ordered) {
        schema.orderings.insert(e_items(), 0);
    }
    if features.contains(&Feature::Key) {
        schema.declarations.insert_key(
            "item".into(),
            Key {
                id: "pk".into(),
                parts: vec![KeyPart::Path(vec![e_name()])],
                role: KeyRole::Primary,
                scope: KeyScope::Global,
                equality: vec![],
            },
        );
    }
    if features.contains(&Feature::Acyclic) {
        schema
            .constraints
            .entry("item".into())
            .or_default()
            .push(Constraint {
                sort: "acyclic".into(),
                value: "next".into(),
            });
    }
    let protocol = Protocol {
        name: "merge-counterexamples".into(),
        constraint_sorts: vec!["maxOut".into(), "acyclic".into()],
        constraint_semantics: [
            ("maxOut".to_owned(), ConstraintSemantics::MaxOutDegree),
            ("acyclic".to_owned(), ConstraintSemantics::Acyclic),
        ]
        .into_iter()
        .collect(),
        syntactic_shape: InstanceShapeKind::Graph,
        semantic_shape: InstanceShapeKind::Graph,
        ..Protocol::default()
    };
    ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default())
}

fn assert_countermodel(
    plan: &ValidationPlan,
    baseline: &GInstance,
    left: &GraphDelta,
    right: &GraphDelta,
) -> TestResult {
    let validate = |graph: &GInstance| {
        plan.validate_graph(
            graph,
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
        )
    };
    let local = |delta: &GraphDelta| {
        plan.check_graph_delta(
            baseline,
            delta,
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
        )
    };
    assert_eq!(validate(baseline).status, CheckStatus::Passed);
    assert!(left.independence(right).is_empty());
    for delta in [left, right] {
        let mut result = baseline.clone();
        delta.apply(&mut result)?;
        assert_eq!(validate(&result).status, CheckStatus::Passed);
        assert_eq!(local(delta)?.status, CheckStatus::Passed);
    }
    let mut result = baseline.clone();
    let merged = left.merge(right)?;
    merged.apply(&mut result)?;
    assert_eq!(validate(&result).status, CheckStatus::Failed);
    // The authoritative answer for the merged delta is its baseline-relative
    // check, which reports the joint failure the two passing deltas hide.
    let authoritative = local(&merged)?;
    assert_eq!(authoritative.status, CheckStatus::Failed);
    assert_eq!(authoritative.diagnostics, validate(&result).diagnostics);
    assert_eq!(
        authoritative.canonical_bytes(),
        local(&right.merge(left)?)?.canonical_bytes()
    );
    let (validated, _) = ValidatedGraph::new(
        Arc::new(plan.clone()),
        baseline.clone(),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let incremental = validated.check_edit(
        &panproto_inst::GraphEdit::Delta(merged),
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )?;
    assert_eq!(incremental.report.status, CheckStatus::Failed);
    Ok(())
}

#[test]
fn independent_insertions_can_jointly_exceed_a_degree_bound() -> TestResult {
    let plan = plan([Feature::MaxOut]);
    let baseline = GInstance::new()
        .with_node(Node::new(0, "item"))
        .with_node(Node::new(1, "name"))
        .with_node(Node::new(2, "name"));
    let left = GraphDelta::identity().with_added_edge(EdgeId(0), GEdge::new(0, 1, e_name()));
    let right = GraphDelta::identity().with_added_edge(EdgeId(1), GEdge::new(0, 2, e_name()));
    assert_countermodel(&plan, &baseline, &left, &right)
}

#[test]
fn independent_deletions_can_jointly_remove_the_last_required_edge() -> TestResult {
    let plan = plan([Feature::RequiredName]);
    let first = GEdge::new(0, 1, e_name());
    let second = GEdge::new(0, 2, e_name());
    let baseline = GInstance::new()
        .with_node(Node::new(0, "item"))
        .with_node(Node::new(1, "name"))
        .with_node(Node::new(2, "name"))
        .with_edge_id(EdgeId(0), 0, 1, e_name())
        .with_edge_id(EdgeId(1), 0, 2, e_name());
    let left = GraphDelta::identity().with_deleted_edge(EdgeId(0), first);
    let right = GraphDelta::identity().with_deleted_edge(EdgeId(1), second);
    assert_countermodel(&plan, &baseline, &left, &right)
}

#[test]
fn independent_position_updates_can_jointly_duplicate_a_position() -> TestResult {
    let plan = plan([Feature::Ordered]);
    let baseline = GInstance::new()
        .with_node(Node::new(0, "doc"))
        .with_node(Node::new(1, "item"))
        .with_node(Node::new(2, "item"))
        .with_edge_id(EdgeId(0), 0, 1, e_items())
        .with_edge_id(EdgeId(1), 0, 2, e_items());
    let mut positioned = baseline;
    positioned.set_edge_position(EdgeId(0), Some(1))?;
    positioned.set_edge_position(EdgeId(1), Some(2))?;
    let left = GraphDelta::identity().with_change(
        AttrKey::EdgePosition { edge: EdgeId(0) },
        AttrVal::Position(Some(1)),
        AttrVal::Position(Some(0)),
    )?;
    let right = GraphDelta::identity().with_change(
        AttrKey::EdgePosition { edge: EdgeId(1) },
        AttrVal::Position(Some(2)),
        AttrVal::Position(Some(0)),
    )?;
    assert_countermodel(&plan, &positioned, &left, &right)
}

#[test]
fn independent_insertions_can_jointly_collide_on_a_unique_key() -> TestResult {
    let plan = plan([Feature::Key]);
    let baseline = GInstance::new();
    let name =
        || Node::new(1, "name").with_value(FieldPresence::Present(Value::Str("same".into())));
    let left = GraphDelta::identity()
        .with_added_node(Node::new(0, "item"))
        .with_added_node(name())
        .with_added_edge(EdgeId(0), GEdge::new(0, 1, e_name()));
    let right = GraphDelta::identity()
        .with_added_node(Node::new(2, "item"))
        .with_added_node(
            Node::new(3, "name").with_value(FieldPresence::Present(Value::Str("same".into()))),
        )
        .with_added_edge(EdgeId(1), GEdge::new(2, 3, e_name()));
    assert_countermodel(&plan, &baseline, &left, &right)
}

#[test]
fn independent_edge_insertions_can_jointly_close_a_cycle() -> TestResult {
    let plan = plan([Feature::Acyclic]);
    let baseline = GInstance::new()
        .with_node(Node::new(0, "item"))
        .with_node(Node::new(1, "item"));
    let left = GraphDelta::identity().with_added_edge(EdgeId(0), GEdge::new(0, 1, e_next()));
    let right = GraphDelta::identity().with_added_edge(EdgeId(1), GEdge::new(1, 0, e_next()));
    assert_countermodel(&plan, &baseline, &left, &right)
}

#[test]
fn independent_merge_has_only_the_operational_algebra_laws() -> TestResult {
    let edge = e_next();
    let a = GraphDelta::identity().with_added_edge(EdgeId(0), GEdge::new(0, 1, edge.clone()));
    let b = GraphDelta::identity().with_added_edge(EdgeId(1), GEdge::new(1, 2, edge.clone()));
    let c = GraphDelta::identity().with_added_edge(EdgeId(2), GEdge::new(2, 3, edge));
    assert_eq!(a.merge(&b)?, b.merge(&a)?);
    assert_eq!(a.merge(&b)?.merge(&c)?, a.merge(&b.merge(&c)?)?);
    assert_eq!(a.merge(&GraphDelta::identity())?, a);
    assert!(a.merge(&a).is_err());
    Ok(())
}
