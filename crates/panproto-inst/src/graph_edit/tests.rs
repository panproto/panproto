//! Unit tests of graph deltas and graph edits.

use std::collections::BTreeMap;
use std::error::Error;

use panproto_schema::Edge;

use super::*;
use crate::edit_error::EditError;
use crate::error_detail::ErrorDetail;
use crate::fan_violation::FanViolation;
use crate::ginstance::{EdgeId, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND};
use crate::metadata::{Node, NodeShape};
use crate::value::{FieldPresence, Value};
use crate::view_ref::RowId;

type TestResult = Result<(), Box<dyn Error>>;

fn label(kind: &str) -> Edge {
    Edge {
        src: "person".into(),
        tgt: "person".into(),
        kind: kind.into(),
        name: None,
    }
}

fn person(id: u32) -> Node {
    Node::new(id, "person")
}

/// Nodes 0, 1, 2 and edges 0: 0 -> 1, 1: 1 -> 2, 2: 2 -> 0, all `knows`;
/// node 0 has the field `age = 30` and the value `"ann"`.
fn triangle() -> GInstance {
    let mut first = person(0);
    first.extra_fields.insert("age".into(), Value::Int(30));
    first.value = Some(FieldPresence::Present(Value::Str("ann".into())));
    GInstance::new()
        .with_node(first)
        .with_node(person(1))
        .with_node(person(2))
        .with_edge_id(EdgeId(0), 0, 1, label("knows"))
        .with_edge_id(EdgeId(1), 1, 2, label("knows"))
        .with_edge_id(EdgeId(2), 2, 0, label("knows"))
}

fn node_of(graph: &GInstance, id: u32) -> Result<Node, Box<dyn Error>> {
    graph
        .node(id)
        .cloned()
        .ok_or_else(|| format!("no node {id}").into())
}

fn edge_of(graph: &GInstance, id: u32) -> Result<GEdge, Box<dyn Error>> {
    graph
        .edge(EdgeId(id))
        .cloned()
        .ok_or_else(|| format!("no edge {id}").into())
}

/// Apply `delta` to a copy of `graph`, expecting `err`, and check the copy
/// is unchanged and `check` reports the same error.
fn expect_apply_error(graph: &GInstance, delta: &GraphDelta, err: &EditError) {
    let mut copy = graph.clone();
    assert_eq!(delta.apply(&mut copy).as_ref(), Err(err));
    assert!(
        GInstance::identical(&copy, graph),
        "instance changed on error"
    );
    assert_eq!(delta.check(graph).as_ref(), Err(err));
}

fn anchor_change(node: u32, old: &str, new: &str) -> Result<GraphDelta, MalformedDelta> {
    GraphDelta::identity().with_change(
        AttrKey::Anchor { node },
        AttrVal::Anchor(old.into()),
        AttrVal::Anchor(new.into()),
    )
}

// ---------------------------------------------------------------------------
// Types and serialization
// ---------------------------------------------------------------------------

#[test]
fn graph_delta_fields_serialize_in_contract_order() -> TestResult {
    let g = triangle();
    let delta = GraphDelta::identity()
        .with_deleted_node(node_of(&g, 1)?)
        .with_deleted_edge(EdgeId(0), edge_of(&g, 0)?)
        .with_deleted_edge(EdgeId(1), edge_of(&g, 1)?)
        .with_added_node(person(7))
        .with_added_edge(EdgeId(9), GEdge::new(2, 7, label("knows")))
        .with_change(
            AttrKey::Field {
                node: 0,
                field: "age".into(),
            },
            AttrVal::Field(Some(Value::Int(30))),
            AttrVal::Field(None),
        )?;
    let json = serde_json::to_string(&delta)?;
    let positions: Vec<usize> = [
        "\"del_nodes\"",
        "\"del_edges\"",
        "\"add_nodes\"",
        "\"add_edges\"",
        "\"attrs\"",
    ]
    .iter()
    .map(|k| json.find(k).ok_or("missing field"))
    .collect::<Result<_, _>>()?;
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{json}");
    let back: GraphDelta = serde_json::from_str(&json)?;
    assert_eq!(back, delta);
    let packed = rmp_serde::to_vec_named(&delta)?;
    assert_eq!(rmp_serde::from_slice::<GraphDelta>(&packed)?, delta);
    Ok(())
}

#[test]
fn attr_key_orders_graph_variants_before_table_variants() {
    let declared = vec![
        AttrKey::Value { node: 9 },
        AttrKey::Field {
            node: 0,
            field: "a".into(),
        },
        AttrKey::Meta { node: 0 },
        AttrKey::Anchor { node: 0 },
        AttrKey::EdgeLabel { edge: EdgeId(9) },
        AttrKey::EdgePosition { edge: EdgeId(0) },
        AttrKey::Cell {
            row: RowId(0),
            column: "a".into(),
        },
        AttrKey::Table { row: RowId(0) },
    ];
    let mut sorted = declared.clone();
    sorted.reverse();
    sorted.sort();
    assert_eq!(sorted, declared);
    assert!(declared[..6].iter().all(|k| !k.is_table()));
    assert!(declared[6..].iter().all(AttrKey::is_table));
}

#[test]
fn attr_val_field_none_is_absence() -> TestResult {
    let mut g = triangle();
    let delta = GraphEdit::remove_field(&g, 0, "age")?.to_delta()?;
    let change = delta
        .attrs()
        .get(&AttrKey::Field {
            node: 0,
            field: "age".into(),
        })
        .ok_or("no change")?;
    assert_eq!(change.new, AttrVal::Field(None));
    delta.apply(&mut g)?;
    assert!(!node_of(&g, 0)?.extra_fields.contains_key("age"));
    delta.invert().apply(&mut g)?;
    assert_eq!(
        node_of(&g, 0)?.extra_fields.get("age"),
        Some(&Value::Int(30))
    );
    Ok(())
}

#[test]
fn an_attr_value_of_the_wrong_variant_for_its_key_is_rejected() {
    let wrong = GraphDelta::identity().with_change(
        AttrKey::Value { node: 0 },
        AttrVal::Anchor("person".into()),
        AttrVal::Presence(None),
    );
    assert_eq!(
        wrong.err(),
        Some(MalformedDelta::ValueVariant {
            key: AttrKey::Value { node: 0 }
        })
    );
    let table = GraphDelta::identity().with_change(
        AttrKey::Table { row: RowId(1) },
        AttrVal::Anchor("a".into()),
        AttrVal::Anchor("b".into()),
    );
    assert_eq!(
        table.err(),
        Some(MalformedDelta::TableKey {
            key: AttrKey::Table { row: RowId(1) }
        })
    );
    let decoded = serde_json::from_str::<GraphDelta>(
        r#"{"del_nodes":{},"del_edges":{},"add_nodes":{},"add_edges":{},
            "attrs":[[{"Meta":{"node":0}},{"old":{"Anchor":"a"},"new":{"Anchor":"b"}}]]}"#,
    );
    assert!(decoded.is_err());
    let mut parts = GraphDeltaParts::default();
    parts.add_nodes.insert(3, person(4));
    assert_eq!(
        GraphDelta::normalize(parts).err(),
        Some(MalformedDelta::NodeIdMismatch { key: 3, node: 4 })
    );
}

// ---------------------------------------------------------------------------
// Normal form
// ---------------------------------------------------------------------------

#[test]
fn normalize_drops_unchanged_attributes() -> TestResult {
    let delta = anchor_change(0, "person", "person")?;
    assert!(delta.is_identity());
    let mut parts = GraphDeltaParts::default();
    parts.attrs.insert(
        AttrKey::Value { node: 0 },
        AttrChange {
            old: AttrVal::Presence(Some(FieldPresence::Present(Value::Float(f64::NAN)))),
            new: AttrVal::Presence(Some(FieldPresence::Present(Value::Float(f64::NAN)))),
        },
    );
    assert!(GraphDelta::normalize(parts)?.is_identity());
    Ok(())
}

#[test]
fn normalize_drops_attributes_of_deleted_or_added_elements() -> TestResult {
    let g = triangle();
    let mut parts = GraphDeltaParts::default();
    parts.del_nodes.insert(1, node_of(&g, 1)?);
    parts.add_nodes.insert(5, person(5));
    parts
        .add_edges
        .insert(EdgeId(7), GEdge::new(0, 5, label("knows")));
    parts.attrs.insert(
        AttrKey::Anchor { node: 1 },
        AttrChange {
            old: AttrVal::Anchor("person".into()),
            new: AttrVal::Anchor("agent".into()),
        },
    );
    parts.attrs.insert(
        AttrKey::Anchor { node: 5 },
        AttrChange {
            old: AttrVal::Anchor("person".into()),
            new: AttrVal::Anchor("agent".into()),
        },
    );
    parts.attrs.insert(
        AttrKey::EdgePosition { edge: EdgeId(7) },
        AttrChange {
            old: AttrVal::Position(None),
            new: AttrVal::Position(Some(1)),
        },
    );
    parts.attrs.insert(
        AttrKey::Anchor { node: 2 },
        AttrChange {
            old: AttrVal::Anchor("person".into()),
            new: AttrVal::Anchor("agent".into()),
        },
    );
    let delta = GraphDelta::normalize(parts)?;
    assert_eq!(
        delta.attrs().keys().cloned().collect::<Vec<_>>(),
        vec![AttrKey::Anchor { node: 2 }]
    );
    Ok(())
}

#[test]
fn an_equal_payload_replacement_survives_normalize() -> TestResult {
    let g = triangle();
    let node = node_of(&g, 1)?;
    let delta = GraphDelta::identity()
        .with_deleted_node(node.clone())
        .with_added_node(node);
    assert_eq!(delta.del_nodes().len(), 1);
    assert_eq!(delta.add_nodes().len(), 1);
    let round: GraphDelta = serde_json::from_str(&serde_json::to_string(&delta)?)?;
    assert_eq!(round.del_nodes().len(), 1);
    assert_eq!(round.add_nodes().len(), 1);
    Ok(())
}

// ---------------------------------------------------------------------------
// Identity policy
// ---------------------------------------------------------------------------

#[test]
fn a_delete_and_add_of_one_id_is_a_replacement() -> TestResult {
    let g = triangle();
    let old = node_of(&g, 1)?;
    let replaced = GraphDelta::identity()
        .with_deleted_node(old)
        .with_added_node(Node::new(1, "agent"));
    // The old node's incident edges do not transfer: the delta must delete them.
    expect_apply_error(
        &g,
        &replaced,
        &EditError::DanglingEdges {
            node: 1,
            edges: vec![EdgeId(0), EdgeId(1)],
        },
    );
    let with_residue = replaced
        .with_deleted_edge(EdgeId(0), edge_of(&g, 0)?)
        .with_deleted_edge(EdgeId(1), edge_of(&g, 1)?);
    let mut h = g;
    with_residue.apply(&mut h)?;
    assert_eq!(node_of(&h, 1)?.anchor.as_ref(), "agent");
    assert!(h.in_edges(1).is_empty() && h.out_edges(1).is_empty());
    Ok(())
}

#[test]
fn composing_delete_then_add_is_not_the_identity() -> TestResult {
    let node = node_of(&triangle(), 2)?;
    let delete = GraphDelta::identity().with_deleted_node(node.clone());
    let add = GraphDelta::identity().with_added_node(node);
    let net = delete.compose(&add)?;
    assert!(!net.is_identity());
    assert_eq!(net.del_nodes().len(), 1);
    assert_eq!(net.add_nodes().len(), 1);
    Ok(())
}

#[test]
fn invert_recreates_the_deleted_element() -> TestResult {
    let g = triangle();
    let delta = GraphEdit::remove_node_cascade(&g, 0)?.to_delta()?;
    let mut h = g.clone();
    delta.apply(&mut h)?;
    assert!(h.node(0).is_none());
    delta.invert().apply(&mut h)?;
    assert!(GInstance::identical(&h, &g));
    assert_eq!(
        node_of(&h, 0)?.extra_fields.get("age"),
        Some(&Value::Int(30))
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Apply
// ---------------------------------------------------------------------------

#[test]
fn apply_reports_node_not_found() {
    let delta = GraphDelta::identity().with_deleted_node(person(9));
    expect_apply_error(&triangle(), &delta, &EditError::NodeNotFound(9));
    let attr = anchor_change(9, "person", "agent");
    assert!(attr.is_ok());
    if let Ok(attr) = attr {
        expect_apply_error(&triangle(), &attr, &EditError::NodeNotFound(9));
    }
}

#[test]
fn apply_reports_stale_node() {
    let delta = GraphDelta::identity().with_deleted_node(Node::new(1, "agent"));
    expect_apply_error(&triangle(), &delta, &EditError::StaleNode { node: 1 });
}

#[test]
fn apply_reports_edge_not_found() -> TestResult {
    let delta =
        GraphDelta::identity().with_deleted_edge(EdgeId(5), GEdge::new(0, 1, label("knows")));
    expect_apply_error(&triangle(), &delta, &EditError::EdgeNotFound(EdgeId(5)));
    let attr = GraphDelta::identity().with_change(
        AttrKey::EdgePosition { edge: EdgeId(5) },
        AttrVal::Position(None),
        AttrVal::Position(Some(0)),
    )?;
    expect_apply_error(&triangle(), &attr, &EditError::EdgeNotFound(EdgeId(5)));
    Ok(())
}

#[test]
fn apply_reports_stale_edge() {
    let delta =
        GraphDelta::identity().with_deleted_edge(EdgeId(0), GEdge::new(0, 2, label("knows")));
    expect_apply_error(
        &triangle(),
        &delta,
        &EditError::StaleEdge { edge: EdgeId(0) },
    );
}

#[test]
fn apply_reports_stale_value() -> TestResult {
    let delta = GraphDelta::identity().with_change(
        AttrKey::Value { node: 0 },
        AttrVal::Presence(None),
        AttrVal::Presence(Some(FieldPresence::Null)),
    )?;
    expect_apply_error(&triangle(), &delta, &EditError::StaleValue { node: 0 });
    Ok(())
}

#[test]
fn apply_reports_stale_field() -> TestResult {
    let delta = GraphDelta::identity().with_change(
        AttrKey::Field {
            node: 0,
            field: "age".into(),
        },
        AttrVal::Field(Some(Value::Int(31))),
        AttrVal::Field(Some(Value::Int(32))),
    )?;
    expect_apply_error(
        &triangle(),
        &delta,
        &EditError::StaleField {
            node: 0,
            field: "age".into(),
        },
    );
    Ok(())
}

#[test]
fn apply_reports_stale_meta() -> TestResult {
    let old = NodeMeta {
        position: Some(4),
        ..NodeMeta::default()
    };
    let delta = GraphDelta::identity().with_change(
        AttrKey::Meta { node: 1 },
        AttrVal::Meta(old),
        AttrVal::Meta(NodeMeta::default()),
    )?;
    expect_apply_error(&triangle(), &delta, &EditError::StaleMeta { node: 1 });
    Ok(())
}

#[test]
fn apply_reports_stale_anchor() -> TestResult {
    let delta = anchor_change(1, "agent", "thing")?;
    expect_apply_error(&triangle(), &delta, &EditError::StaleAnchor { node: 1 });
    Ok(())
}

#[test]
fn apply_reports_stale_label() -> TestResult {
    let delta = GraphDelta::identity().with_change(
        AttrKey::EdgeLabel { edge: EdgeId(1) },
        AttrVal::Label(label("likes")),
        AttrVal::Label(label("hates")),
    )?;
    expect_apply_error(
        &triangle(),
        &delta,
        &EditError::StaleLabel { edge: EdgeId(1) },
    );
    Ok(())
}

#[test]
fn apply_reports_stale_position() -> TestResult {
    let delta = GraphDelta::identity().with_change(
        AttrKey::EdgePosition { edge: EdgeId(2) },
        AttrVal::Position(Some(3)),
        AttrVal::Position(None),
    )?;
    expect_apply_error(
        &triangle(),
        &delta,
        &EditError::StalePosition { edge: EdgeId(2) },
    );
    Ok(())
}

#[test]
fn apply_reports_dangling_edges() -> TestResult {
    let g = triangle();
    let delta = GraphDelta::identity()
        .with_deleted_node(node_of(&g, 0)?)
        .with_deleted_edge(EdgeId(0), edge_of(&g, 0)?);
    expect_apply_error(
        &g,
        &delta,
        &EditError::DanglingEdges {
            node: 0,
            edges: vec![EdgeId(2)],
        },
    );
    Ok(())
}

#[test]
fn apply_reports_duplicate_node_id() -> TestResult {
    let g = triangle();
    expect_apply_error(
        &g,
        &GraphDelta::identity().with_added_node(person(2)),
        &EditError::DuplicateNodeId(2),
    );
    // After deletion the id is free: a replacement applies.
    let mut h = g.clone();
    GraphEdit::remove_node_cascade(&g, 2)?
        .to_delta()?
        .with_added_node(Node::new(2, "agent"))
        .apply(&mut h)?;
    assert_eq!(node_of(&h, 2)?.anchor.as_ref(), "agent");
    Ok(())
}

#[test]
fn apply_reports_duplicate_edge() {
    let delta = GraphDelta::identity().with_added_edge(EdgeId(1), GEdge::new(0, 0, label("knows")));
    expect_apply_error(&triangle(), &delta, &EditError::DuplicateEdge(EdgeId(1)));
}

#[test]
fn apply_reports_missing_endpoint() -> TestResult {
    let g = triangle();
    let delta = GraphDelta::identity().with_added_edge(EdgeId(3), GEdge::new(0, 8, label("knows")));
    expect_apply_error(
        &g,
        &delta,
        &EditError::MissingEndpoint {
            edge: EdgeId(3),
            node: 8,
        },
    );
    // An endpoint the same delta deletes is missing too.
    let deleted = GraphEdit::remove_node_cascade(&g, 2)?
        .to_delta()?
        .with_added_edge(EdgeId(3), GEdge::new(0, 2, label("knows")));
    expect_apply_error(
        &g,
        &deleted,
        &EditError::MissingEndpoint {
            edge: EdgeId(3),
            node: 2,
        },
    );
    Ok(())
}

#[test]
fn apply_reports_malformed_fan() -> TestResult {
    let g = triangle();
    let delta = GraphDelta::identity()
        .with_added_node(Node::new(5, "arg").with_shape(NodeShape::HyperEdge));
    expect_apply_error(
        &g,
        &delta,
        &EditError::MalformedFan {
            node: 5,
            reason: FanViolation::MissingParent,
        },
    );
    let parent = Edge {
        src: "arg".into(),
        tgt: "person".into(),
        kind: HYPER_PARENT_KIND.into(),
        name: None,
    };
    let fixed = delta.with_added_edge(EdgeId(3), GEdge::new(5, 0, parent));
    let mut h = g;
    fixed.apply(&mut h)?;
    assert_eq!(h.node_count(), 4);
    Ok(())
}

#[test]
fn a_delta_has_no_schema_check_method() {
    let source = include_str!("delta.rs").to_owned() + include_str!("apply.rs");
    assert!(!source.contains(concat!("fn check", "_schema")));
}

// ---------------------------------------------------------------------------
// Compose
// ---------------------------------------------------------------------------

fn n(anchor: &str) -> Node {
    Node::new(4, anchor)
}

fn del(p: &Node) -> GraphDelta {
    GraphDelta::identity().with_deleted_node(p.clone())
}

fn add(q: &Node) -> GraphDelta {
    GraphDelta::identity().with_added_node(q.clone())
}

fn repl(p: &Node, q: &Node) -> GraphDelta {
    del(p).with_added_node(q.clone())
}

fn attr(old: &str, new: &str) -> Result<GraphDelta, MalformedDelta> {
    anchor_change(4, old, new)
}

fn mismatch(elem: ElemRef) -> Result<GraphDelta, ComposeError> {
    Err(ComposeError::Mismatch { elem })
}

#[test]
fn elem_ref_orders_graph_variants_before_table_variants() {
    let declared = vec![
        ElemRef::Node(9),
        ElemRef::Edge(EdgeId(9)),
        ElemRef::Attr(AttrKey::Value { node: 0 }),
        ElemRef::Row(RowId(0)),
        ElemRef::Pair {
            edge: Box::new(label("knows")),
            src: RowId(0),
            tgt: RowId(0),
        },
    ];
    let mut sorted = declared.clone();
    sorted.reverse();
    sorted.sort();
    assert_eq!(sorted, declared);
}

#[test]
fn compose_case_table() -> TestResult {
    let (p, q, r, t) = (n("p"), n("q"), n("r"), n("t"));
    let node = ElemRef::Node(4);
    let key = ElemRef::Attr(AttrKey::Anchor { node: 4 });
    let id = GraphDelta::identity();

    // Row "none": the next delta passes through.
    assert_eq!(id.compose(&del(&r)), Ok(del(&r)));
    assert_eq!(id.compose(&add(&r)), Ok(add(&r)));
    assert_eq!(id.compose(&repl(&r, &t)), Ok(repl(&r, &t)));
    assert_eq!(id.compose(&attr("a", "b")?), Ok(attr("a", "b")?));

    // Row Del(p).
    assert_eq!(del(&p).compose(&del(&p)), mismatch(node.clone()));
    assert_eq!(del(&p).compose(&add(&r)), Ok(repl(&p, &r)));
    assert_eq!(del(&p).compose(&repl(&p, &t)), mismatch(node.clone()));
    assert_eq!(del(&p).compose(&attr("p", "b")?), mismatch(node.clone()));

    // Row Add(q).
    assert_eq!(add(&q).compose(&del(&q)), Ok(id.clone()));
    assert_eq!(add(&q).compose(&del(&r)), mismatch(node.clone()));
    assert_eq!(add(&q).compose(&add(&r)), mismatch(node.clone()));
    assert_eq!(add(&q).compose(&repl(&q, &t)), Ok(add(&t)));
    assert_eq!(add(&q).compose(&repl(&r, &t)), mismatch(node.clone()));
    assert_eq!(add(&q).compose(&attr("q", "z")?), Ok(add(&n("z"))));
    assert_eq!(add(&q).compose(&attr("y", "z")?), mismatch(key.clone()));

    // Row Repl(p -> q).
    assert_eq!(repl(&p, &q).compose(&del(&q)), Ok(del(&p)));
    assert_eq!(repl(&p, &q).compose(&del(&r)), mismatch(node.clone()));
    assert_eq!(repl(&p, &q).compose(&add(&r)), mismatch(node.clone()));
    assert_eq!(repl(&p, &q).compose(&repl(&q, &t)), Ok(repl(&p, &t)));
    assert_eq!(repl(&p, &q).compose(&repl(&r, &t)), mismatch(node.clone()));
    assert_eq!(
        repl(&p, &q).compose(&attr("q", "z")?),
        Ok(repl(&p, &n("z")))
    );
    assert_eq!(
        repl(&p, &q).compose(&attr("y", "z")?),
        mismatch(key.clone())
    );

    // Row Attr(k: o -> n).
    assert_eq!(attr("o", "r")?.compose(&del(&r)), Ok(del(&n("o"))));
    assert_eq!(attr("o", "x")?.compose(&del(&r)), mismatch(key.clone()));
    assert_eq!(attr("o", "r")?.compose(&add(&r)), mismatch(node));
    assert_eq!(
        attr("o", "r")?.compose(&repl(&r, &t)),
        Ok(repl(&n("o"), &t))
    );
    assert_eq!(
        attr("o", "x")?.compose(&repl(&r, &t)),
        mismatch(key.clone())
    );
    assert_eq!(
        attr("o", "m")?.compose(&attr("m", "z")?),
        Ok(attr("o", "z")?)
    );
    assert_eq!(attr("o", "m")?.compose(&attr("m", "o")?), Ok(id));
    assert_eq!(attr("o", "m")?.compose(&attr("x", "z")?), mismatch(key));
    let value = GraphDelta::identity().with_change(
        AttrKey::Value { node: 4 },
        AttrVal::Presence(None),
        AttrVal::Presence(Some(FieldPresence::Null)),
    )?;
    let both = attr("o", "m")?.compose(&value)?;
    assert_eq!(both.attrs().len(), 2);
    Ok(())
}

#[test]
fn delete_then_add_is_a_replacement_not_the_identity() -> TestResult {
    let p = n("p");
    let net = del(&p).compose(&add(&p))?;
    assert_eq!(net, repl(&p, &p));
    assert!(!net.is_identity());
    Ok(())
}

#[test]
fn add_then_delete_of_the_same_payload_cancels() -> TestResult {
    let q = n("q");
    assert!(add(&q).compose(&del(&q))?.is_identity());
    assert_eq!(add(&q).compose(&del(&n("r"))), mismatch(ElemRef::Node(4)));
    Ok(())
}

#[test]
fn attribute_changes_fold_into_a_later_delete_in_key_order() -> TestResult {
    let mut deleted = n("new-anchor");
    deleted.value = Some(FieldPresence::Null);
    deleted.extra_fields.insert("f".into(), Value::Int(2));
    let changes = GraphDelta::identity()
        .with_change(
            AttrKey::Value { node: 4 },
            AttrVal::Presence(None),
            AttrVal::Presence(Some(FieldPresence::Null)),
        )?
        .with_change(
            AttrKey::Field {
                node: 4,
                field: "f".into(),
            },
            AttrVal::Field(Some(Value::Int(1))),
            AttrVal::Field(Some(Value::Int(2))),
        )?
        .with_change(
            AttrKey::Anchor { node: 4 },
            AttrVal::Anchor("old-anchor".into()),
            AttrVal::Anchor("new-anchor".into()),
        )?;
    let net = changes.compose(&del(&deleted))?;
    let mut expected = n("old-anchor");
    expected.extra_fields.insert("f".into(), Value::Int(1));
    assert_eq!(net, del(&expected));
    Ok(())
}

#[test]
fn a_mismatched_batch_reports_a_compose_error() -> TestResult {
    let batch = GraphEdit::Batch(vec![
        GraphEdit::AddNode { node: person(5) },
        GraphEdit::AddNode { node: person(5) },
    ]);
    assert_eq!(
        batch.to_delta().err(),
        Some(ComposeError::Mismatch {
            elem: ElemRef::Node(5)
        })
    );
    let g = triangle();
    let conflicting = GraphEdit::RemoveNode {
        node: node_of(&g, 0)?,
        incident: vec![
            (EdgeId(0), edge_of(&g, 0)?),
            (EdgeId(0), GEdge::new(0, 2, label("knows"))),
        ],
    };
    let mut h = g.clone();
    assert_eq!(
        conflicting.apply(&mut h).err(),
        Some(EditError::Compose(ComposeError::Mismatch {
            elem: ElemRef::Edge(EdgeId(0))
        }))
    );
    assert!(GInstance::identical(&h, &g));
    Ok(())
}

#[test]
fn a_failing_batch_reports_its_step() {
    let g = triangle();
    let batch = GraphEdit::Batch(vec![
        GraphEdit::AddNode { node: person(5) },
        GraphEdit::RelabelNode {
            node: 5,
            old_anchor: "person".into(),
            new_anchor: "agent".into(),
        },
        GraphEdit::AddEdge {
            id: EdgeId(4),
            edge: GEdge::new(5, 9, label("knows")),
        },
    ]);
    let mut h = g.clone();
    assert_eq!(
        batch.apply(&mut h).err(),
        Some(EditError::BatchStep {
            index: 2,
            source: Box::new(EditError::MissingEndpoint {
                edge: EdgeId(4),
                node: 9
            }),
        })
    );
    assert!(GInstance::identical(&h, &g));
    let mismatched = GraphEdit::Batch(vec![
        GraphEdit::AddNode { node: person(5) },
        GraphEdit::AddNode { node: person(5) },
    ]);
    assert_eq!(
        mismatched.apply(&mut h).err(),
        Some(EditError::BatchStep {
            index: 1,
            source: Box::new(EditError::DuplicateNodeId(5)),
        })
    );
}

// ---------------------------------------------------------------------------
// Independence and merge
// ---------------------------------------------------------------------------

#[test]
fn independence_reports_delete_delete() -> TestResult {
    let g = triangle();
    let a = GraphEdit::remove_edge(&g, EdgeId(1))?.to_delta()?;
    assert_eq!(
        a.independence(&a),
        vec![EditConflict::DeleteDelete {
            elem: ElemRef::Edge(EdgeId(1))
        }]
    );
    Ok(())
}

#[test]
fn independence_reports_delete_use() -> TestResult {
    let g = triangle();
    let delete = GraphEdit::remove_node_cascade(&g, 1)?.to_delta()?;
    let used = |user: ElemRef| EditConflict::DeleteUse {
        deleted: ElemRef::Node(1),
        user,
    };
    // Adding an edge with the deleted node as an endpoint.
    let add_edge =
        GraphDelta::identity().with_added_edge(EdgeId(8), GEdge::new(2, 1, label("likes")));
    assert_eq!(
        delete.independence(&add_edge),
        vec![used(ElemRef::Edge(EdgeId(8)))]
    );
    // Replacing an edge so that it ends at the deleted node.
    let reconnect = GraphEdit::reconnect(&g, EdgeId(2), 2, 1)?.to_delta_in(&g)?;
    assert_eq!(
        delete.independence(&reconnect),
        vec![used(ElemRef::Edge(EdgeId(2)))]
    );
    // Changing an attribution entry of the deleted node.
    let relabel = anchor_change(1, "person", "agent")?;
    assert_eq!(
        relabel.independence(&delete),
        vec![used(ElemRef::Attr(AttrKey::Anchor { node: 1 }))]
    );
    // Replacing the deleted node.
    let replace = repl(&node_of(&g, 1)?, &Node::new(1, "agent"));
    assert!(
        delete
            .independence(&replace)
            .contains(&used(ElemRef::Node(1)))
    );
    // A deleted edge whose label the other delta changes.
    let edge_delete = GraphEdit::remove_edge(&g, EdgeId(0))?.to_delta()?;
    let edge_label = GraphEdit::relabel_edge(&g, EdgeId(0), label("likes"))?.to_delta()?;
    assert_eq!(
        edge_delete.independence(&edge_label),
        vec![EditConflict::DeleteUse {
            deleted: ElemRef::Edge(EdgeId(0)),
            user: ElemRef::Attr(AttrKey::EdgeLabel { edge: EdgeId(0) }),
        }]
    );
    Ok(())
}

#[test]
fn independence_reports_update_update() -> TestResult {
    let a = anchor_change(2, "person", "agent")?;
    let b = anchor_change(2, "person", "thing")?;
    assert_eq!(
        a.independence(&b),
        vec![EditConflict::UpdateUpdate {
            key: AttrKey::Anchor { node: 2 }
        }]
    );
    Ok(())
}

#[test]
fn independence_reports_add_add() {
    let a = add(&n("a"));
    let b = add(&n("b"));
    assert_eq!(
        a.independence(&b),
        vec![EditConflict::AddAdd {
            elem: ElemRef::Node(4)
        }]
    );
}

#[test]
fn independence_reports_anchor_use() -> TestResult {
    let g = triangle();
    let anchor = anchor_change(2, "person", "agent")?;
    let edge = GraphDelta::identity().with_added_edge(EdgeId(8), GEdge::new(0, 2, label("likes")));
    assert_eq!(
        anchor.independence(&edge),
        vec![EditConflict::AnchorUse {
            node: 2,
            user: ElemRef::Edge(EdgeId(8)),
        }]
    );
    let relabel = GraphEdit::relabel_edge(&g, EdgeId(1), label("likes"))?.to_delta()?;
    assert_eq!(
        relabel.independence(&anchor),
        vec![EditConflict::AnchorUse {
            node: 2,
            user: ElemRef::Attr(AttrKey::EdgeLabel { edge: EdgeId(1) }),
        }]
    );
    // A label change meets the anchor change even between vertex types the
    // anchors never name: edge 1 (1 -> 2) is incident to node 2, which only
    // the state shows.
    let other_types = Edge {
        src: "city".into(),
        tgt: "city".into(),
        kind: "road".into(),
        name: None,
    };
    let unrelated = GraphDelta::identity().with_change(
        AttrKey::EdgeLabel { edge: EdgeId(1) },
        AttrVal::Label(other_types.clone()),
        AttrVal::Label(Edge {
            kind: "rail".into(),
            ..other_types
        }),
    )?;
    assert_eq!(
        anchor.independence(&unrelated),
        vec![EditConflict::AnchorUse {
            node: 2,
            user: ElemRef::Attr(AttrKey::EdgeLabel { edge: EdgeId(1) }),
        }]
    );
    Ok(())
}

/// Nodes 0 and 1 anchored `person` and edge 3 from 0 to 1 whose label's
/// vertex types (`thing`) are neither anchor a re-anchoring of node 0 names:
/// the label is not coherent with the anchors, which a `GInstance` allows.
fn incoherent_edge() -> GInstance {
    let thing = Edge {
        src: "thing".into(),
        tgt: "thing".into(),
        kind: "rel".into(),
        name: None,
    };
    GInstance::new()
        .with_node(person(0))
        .with_node(person(1))
        .with_edge_id(EdgeId(3), 0, 1, thing)
}

#[test]
fn anchor_use_meets_a_label_change_of_an_incident_edge_whatever_its_types() -> TestResult {
    let g = incoherent_edge();
    let a = GraphEdit::relabel_node(&g, 0, "agent")?.to_delta()?;
    let q = Edge {
        src: "q".into(),
        tgt: "q".into(),
        kind: "rel".into(),
        name: None,
    };
    let b = GraphEdit::relabel_edge(&g, EdgeId(3), q)?.to_delta()?;
    let want = EditConflict::AnchorUse {
        node: 0,
        user: ElemRef::Attr(AttrKey::EdgeLabel { edge: EdgeId(3) }),
    };
    assert!(
        a.independence(&b).contains(&want),
        "{:?}",
        a.independence(&b)
    );
    assert!(b.independence(&a).contains(&want));
    assert!(a.merge(&b).is_err());
    Ok(())
}

/// Nodes 0 and 1 anchored `person` and fan node 5 (`arg`) whose parent edge
/// 0 goes to node 0: a graph in the fan domain.
fn one_fan() -> GInstance {
    let parent = Edge {
        src: "arg".into(),
        tgt: "person".into(),
        kind: HYPER_PARENT_KIND.into(),
        name: None,
    };
    GInstance::new()
        .with_node(person(0))
        .with_node(person(1))
        .with_node(Node::new(5, "arg").with_shape(NodeShape::HyperEdge))
        .with_edge_id(EdgeId(0), 5, 0, parent)
}

fn fan_label(name: &str) -> Edge {
    Edge {
        src: "arg".into(),
        tgt: "person".into(),
        kind: HYPER_LABEL_KIND.into(),
        name: Some(name.into()),
    }
}

#[test]
fn two_deltas_that_together_break_a_fan_are_not_independent() -> TestResult {
    let fan = one_fan();
    let a = GraphDelta::identity().with_added_edge(EdgeId(10), GEdge::new(5, 1, fan_label("x")));
    let b = GraphDelta::identity().with_added_edge(EdgeId(11), GEdge::new(5, 0, fan_label("x")));
    a.check(&fan)?;
    b.check(&fan)?;
    // One after the other they break the fan domain.
    let mut ab = fan.clone();
    a.apply(&mut ab)?;
    assert!(matches!(
        b.apply(&mut ab),
        Err(EditError::MalformedFan {
            node: 5,
            reason: FanViolation::DuplicateLabel { .. }
        })
    ));
    assert_eq!(
        a.independence(&b),
        vec![EditConflict::FanUse {
            first: ElemRef::Edge(EdgeId(10)),
            second: ElemRef::Edge(EdgeId(11)),
        }]
    );
    assert_eq!(a.independence(&b), b.independence(&a));
    assert!(a.merge(&b).is_err());
    // Two different labels are still reported: the check is on the
    // representation, which does not see the labels already present.
    let c = GraphDelta::identity().with_added_edge(EdgeId(11), GEdge::new(5, 0, fan_label("y")));
    assert!(!a.independence(&c).is_empty());
    // An edge change away from any fan node is not a fan conflict, even at
    // the target of a reserved edge.
    let d = GraphDelta::identity().with_added_edge(EdgeId(12), GEdge::new(0, 1, label("knows")));
    assert!(a.independence(&d).is_empty());
    // A shape change to the fan shape meets any edge change at the node.
    let mut meta = NodeMeta::of(&person(1));
    meta.shape = NodeShape::HyperEdge;
    let shape = GraphEdit::set_node_meta(&fan, 1, meta)?.to_delta()?;
    assert_eq!(
        a.independence(&shape),
        vec![EditConflict::FanUse {
            first: ElemRef::Edge(EdgeId(10)),
            second: ElemRef::Attr(AttrKey::Meta { node: 1 }),
        }]
    );
    // A label change to or from a reserved kind meets every fan change.
    let rename = GraphEdit::relabel_edge(&fan, EdgeId(0), fan_label("x"))?.to_delta()?;
    assert_eq!(
        rename.independence(&a),
        vec![EditConflict::FanUse {
            first: ElemRef::Edge(EdgeId(10)),
            second: ElemRef::Attr(AttrKey::EdgeLabel { edge: EdgeId(0) }),
        }]
    );
    assert!(
        !EditConflict::FanUse {
            first: ElemRef::Edge(EdgeId(10)),
            second: ElemRef::Edge(EdgeId(11)),
        }
        .is_benign(&a, &b)
    );
    Ok(())
}

#[test]
fn graph_edit_invert_of_a_reconnect_restores_the_state() -> TestResult {
    let g = triangle();
    let reconnect = GraphEdit::reconnect(&g, EdgeId(1), 0, 0)?;
    // Without the instance a reconnect has no inverse.
    assert!(matches!(
        reconnect.invert(),
        Err(ComposeError::NeedsState {
            elem: ElemRef::Edge(EdgeId(1))
        })
    ));
    let inverse = GraphEdit::Delta(reconnect.to_delta_in(&g)?.invert());
    let mut h = g.clone();
    reconnect.apply(&mut h)?;
    assert_eq!(edge_of(&h, 1)?.src, 0);
    inverse.apply(&mut h)?;
    assert!(GInstance::identical(&h, &g));
    // A batch mixing a delta step and reconnects inverts through its
    // lowering against the instance.
    let batch = GraphEdit::Batch(vec![
        GraphEdit::AddEdge {
            id: EdgeId(7),
            edge: GEdge::new(1, 2, label("likes")),
        },
        GraphEdit::Reconnect {
            id: EdgeId(7),
            old: (1, 2),
            new: (0, 2),
        },
        GraphEdit::Reconnect {
            id: EdgeId(7),
            old: (0, 2),
            new: (0, 1),
        },
    ]);
    let inverse = GraphEdit::Delta(batch.to_delta_in(&g)?.invert());
    let mut h = g.clone();
    batch.apply(&mut h)?;
    let seven = edge_of(&h, 7)?;
    assert_eq!((seven.src, seven.tgt), (0, 1));
    inverse.apply(&mut h)?;
    assert!(GInstance::identical(&h, &g));
    // An edit that lowers without the instance inverts to a delta.
    let relabel = GraphEdit::relabel_node(&g, 1, "agent")?;
    assert!(matches!(relabel.invert()?, GraphEdit::Delta(_)));
    Ok(())
}

#[test]
fn graph_edit_invert_is_the_inverse_of_to_delta_in_any_step_order() {
    let add = GraphEdit::AddEdge {
        id: EdgeId(7),
        edge: GEdge::new(1, 2, label("likes")),
    };
    let reconnect = GraphEdit::Reconnect {
        id: EdgeId(1),
        old: (1, 2),
        new: (0, 2),
    };
    // Two adds of one id do not compose; the reconnect does not lower
    // without the instance. Either order must invert as `to_delta` lowers.
    let orders = [
        vec![reconnect.clone(), add.clone(), add.clone()],
        vec![add.clone(), add, reconnect.clone()],
        vec![reconnect],
    ];
    for steps in orders {
        let batch = GraphEdit::Batch(steps);
        let expected = batch.to_delta().map(|d| GraphEdit::Delta(d.invert()));
        let agrees = match (batch.invert(), expected) {
            (Ok(GraphEdit::Delta(got)), Ok(GraphEdit::Delta(want))) => got == want,
            (Err(got), Err(want)) => got == want,
            _ => false,
        };
        assert!(agrees, "{batch:?}");
        assert!(batch.invert().is_err(), "{batch:?}");
    }
}

#[test]
fn benign_conflicts_are_exactly_the_three_equal_payload_cases() -> TestResult {
    let g = triangle();
    let delete = GraphEdit::remove_edge(&g, EdgeId(1))?.to_delta()?;
    let dd = EditConflict::DeleteDelete {
        elem: ElemRef::Edge(EdgeId(1)),
    };
    assert!(dd.is_benign(&delete, &delete));
    let other_payload =
        GraphDelta::identity().with_deleted_edge(EdgeId(1), GEdge::new(1, 1, label("knows")));
    assert!(!dd.is_benign(&delete, &other_payload));

    let a = anchor_change(2, "person", "agent")?;
    let b = anchor_change(2, "other", "agent")?;
    let c = anchor_change(2, "person", "thing")?;
    let uu = EditConflict::UpdateUpdate {
        key: AttrKey::Anchor { node: 2 },
    };
    assert!(uu.is_benign(&a, &b));
    assert!(!uu.is_benign(&a, &c));

    let aa = EditConflict::AddAdd {
        elem: ElemRef::Node(4),
    };
    assert!(aa.is_benign(&add(&n("a")), &add(&n("a"))));
    assert!(!aa.is_benign(&add(&n("a")), &add(&n("b"))));

    let delete_node = GraphEdit::remove_node_cascade(&g, 1)?.to_delta()?;
    let du = EditConflict::DeleteUse {
        deleted: ElemRef::Node(1),
        user: ElemRef::Node(1),
    };
    assert!(!du.is_benign(&delete_node, &delete_node));
    let au = EditConflict::AnchorUse {
        node: 2,
        user: ElemRef::Edge(EdgeId(8)),
    };
    assert!(!au.is_benign(&a, &a));
    Ok(())
}

#[test]
fn merge_of_dependent_deltas_reports_the_conflict() -> TestResult {
    let a = anchor_change(2, "person", "agent")?;
    let b = anchor_change(2, "person", "thing")?;
    assert_eq!(
        a.merge(&b),
        Err(EditConflict::UpdateUpdate {
            key: AttrKey::Anchor { node: 2 }
        })
    );
    let c = anchor_change(1, "person", "thing")?;
    let merged = a.merge(&c)?;
    assert_eq!(merged.attrs().len(), 2);
    Ok(())
}

#[test]
fn merge_reports_a_benign_conflict_too() -> TestResult {
    // A delta merged with itself is not independent of itself: the benign
    // duplicate is still a conflict, and merge is defined on independent
    // deltas only.
    let a = anchor_change(2, "person", "agent")?;
    let conflict = EditConflict::UpdateUpdate {
        key: AttrKey::Anchor { node: 2 },
    };
    assert_eq!(a.independence(&a), vec![conflict.clone()]);
    assert!(conflict.is_benign(&a, &a));
    assert_eq!(a.merge(&a), Err(conflict));
    let node = GraphDelta::normalize(GraphDeltaParts {
        add_nodes: std::iter::once((9, Node::new(9, "person"))).collect(),
        ..GraphDeltaParts::default()
    })?;
    assert_eq!(
        node.merge(&node),
        Err(EditConflict::AddAdd {
            elem: ElemRef::Node(9)
        })
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Diff
// ---------------------------------------------------------------------------

#[test]
fn diff_turns_an_endpoint_change_into_a_replacement() -> TestResult {
    let a = triangle();
    let mut b = a.clone();
    b.set_endpoints(EdgeId(0), 0, 2)?;
    b.relabel_edge(EdgeId(1), label("likes"))?;
    b.set_edge_position(EdgeId(2), Some(3))?;
    if let Some(node) = b.node_mut(1) {
        node.anchor = "agent".into();
        node.value = Some(FieldPresence::Absent);
        node.extra_fields.insert("k".into(), Value::Bool(true));
        node.annotations.insert("note".into(), Value::Int(1));
    }
    let delta = GraphDelta::diff(&a, &b);
    assert_eq!(
        delta.del_edges().keys().copied().collect::<Vec<_>>(),
        vec![EdgeId(0)]
    );
    assert_eq!(
        delta.add_edges().get(&EdgeId(0)),
        Some(&GEdge::new(0, 2, label("knows")))
    );
    let keys: Vec<AttrKey> = delta.attrs().keys().cloned().collect();
    assert_eq!(
        keys,
        vec![
            AttrKey::Value { node: 1 },
            AttrKey::Field {
                node: 1,
                field: "k".into()
            },
            AttrKey::Meta { node: 1 },
            AttrKey::Anchor { node: 1 },
            AttrKey::EdgeLabel { edge: EdgeId(1) },
            AttrKey::EdgePosition { edge: EdgeId(2) },
        ]
    );
    let mut c = a;
    delta.apply(&mut c)?;
    assert!(GInstance::identical(&c, &b));
    Ok(())
}

// ---------------------------------------------------------------------------
// The edit surface
// ---------------------------------------------------------------------------

/// The wire prefixes of the node variants.
fn node_wire_cases(e: &GEdge) -> Vec<(GraphEdit, &'static str)> {
    vec![
        (GraphEdit::Identity, r#""Identity""#),
        (
            GraphEdit::AddNode { node: person(3) },
            r#"{"AddNode":{"node":{"#,
        ),
        (
            GraphEdit::RemoveNode {
                node: person(3),
                incident: vec![(EdgeId(1), e.clone())],
            },
            r#"{"RemoveNode":{"node":{"#,
        ),
        (
            GraphEdit::SetValue {
                node: 1,
                old: None,
                new: Some(FieldPresence::Null),
            },
            r#"{"SetValue":{"node":1,"old":null,"new":"Null"}}"#,
        ),
        (
            GraphEdit::SetField {
                node: 1,
                field: "f".into(),
                old: None,
                new: Value::Int(2),
            },
            r#"{"SetField":{"node":1,"field":"f","old":null,"new":{"Int":2}}}"#,
        ),
        (
            GraphEdit::RemoveField {
                node: 1,
                field: "f".into(),
                old: Value::Int(2),
            },
            r#"{"RemoveField":{"node":1,"field":"f","old":{"Int":2}}}"#,
        ),
        (
            GraphEdit::SetNodeMeta {
                node: 1,
                old: NodeMeta::default(),
                new: NodeMeta::default(),
            },
            r#"{"SetNodeMeta":{"node":1,"old":{"discriminator":null,"position":null,"shape":{"kind":"plain"},"annotations":{}},"new":"#,
        ),
        (
            GraphEdit::RelabelNode {
                node: 1,
                old_anchor: "a".into(),
                new_anchor: "b".into(),
            },
            r#"{"RelabelNode":{"node":1,"old_anchor":"a","new_anchor":"b"}}"#,
        ),
    ]
}

/// The wire prefixes of the edge, batch and delta variants.
fn edge_wire_cases(e: &GEdge) -> Vec<(GraphEdit, &'static str)> {
    vec![
        (
            GraphEdit::AddEdge {
                id: EdgeId(2),
                edge: e.clone(),
            },
            r#"{"AddEdge":{"id":2,"edge":{"src":0,"tgt":1,"edge":{"#,
        ),
        (
            GraphEdit::RemoveEdge {
                id: EdgeId(2),
                edge: e.clone(),
            },
            r#"{"RemoveEdge":{"id":2,"edge":{"#,
        ),
        (
            GraphEdit::Reconnect {
                id: EdgeId(2),
                old: (0, 1),
                new: (1, 0),
            },
            r#"{"Reconnect":{"id":2,"old":[0,1],"new":[1,0]}}"#,
        ),
        (
            GraphEdit::RelabelEdge {
                id: EdgeId(2),
                old: label("a"),
                new: label("b"),
            },
            r#"{"RelabelEdge":{"id":2,"old":{"#,
        ),
        (
            GraphEdit::SetEdgePosition {
                id: EdgeId(2),
                old: None,
                new: Some(1),
            },
            r#"{"SetEdgePosition":{"id":2,"old":null,"new":1}}"#,
        ),
        (
            GraphEdit::Batch(vec![GraphEdit::Identity]),
            r#"{"Batch":["Identity"]}"#,
        ),
        (
            GraphEdit::Delta(GraphDelta::identity()),
            r#"{"Delta":{"del_nodes":{},"del_edges":{},"add_nodes":{},"add_edges":{},"attrs":[]}}"#,
        ),
    ]
}

#[test]
fn graph_edit_wire_names_are_exact() -> TestResult {
    let e = GEdge::new(0, 1, label("knows"));
    let mut cases = node_wire_cases(&e);
    cases.extend(edge_wire_cases(&e));
    assert_eq!(cases.len(), 15);
    for (edit, prefix) in cases {
        let json = serde_json::to_string(&edit)?;
        assert!(
            json.starts_with(prefix),
            "{json} does not start with {prefix}"
        );
        let back: GraphEdit = serde_json::from_str(&json)?;
        assert_eq!(serde_json::to_string(&back)?, json);
    }
    Ok(())
}

#[test]
fn graph_edit_to_delta_cases() -> TestResult {
    let g = triangle();
    let remove = GraphEdit::remove_node_cascade(&g, 0)?.to_delta()?;
    assert_eq!(
        remove.del_nodes().keys().copied().collect::<Vec<_>>(),
        vec![0]
    );
    assert_eq!(
        remove.del_edges().keys().copied().collect::<Vec<_>>(),
        vec![EdgeId(0), EdgeId(2)]
    );

    let reconnect = GraphEdit::reconnect(&g, EdgeId(1), 1, 0)?;
    assert_eq!(
        reconnect.to_delta().err(),
        Some(ComposeError::NeedsState {
            elem: ElemRef::Edge(EdgeId(1))
        })
    );
    let lowered = reconnect.to_delta_in(&g)?;
    assert_eq!(lowered.del_edges().get(&EdgeId(1)), Some(&edge_of(&g, 1)?));
    assert_eq!(
        lowered.add_edges().get(&EdgeId(1)),
        Some(&GEdge::new(1, 0, label("knows")))
    );

    let single_key = |edit: GraphEdit, key: AttrKey| -> TestResult {
        let delta = edit.to_delta()?;
        assert_eq!(delta.attrs().keys().cloned().collect::<Vec<_>>(), vec![key]);
        Ok(())
    };
    single_key(
        GraphEdit::relabel_node(&g, 1, "agent")?,
        AttrKey::Anchor { node: 1 },
    )?;
    single_key(
        GraphEdit::relabel_edge(&g, EdgeId(1), label("likes"))?,
        AttrKey::EdgeLabel { edge: EdgeId(1) },
    )?;
    single_key(
        GraphEdit::set_edge_position(&g, EdgeId(1), Some(2))?,
        AttrKey::EdgePosition { edge: EdgeId(1) },
    )?;
    single_key(
        GraphEdit::set_value(&g, 1, Some(FieldPresence::Null))?,
        AttrKey::Value { node: 1 },
    )?;
    single_key(
        GraphEdit::set_field(&g, 1, "f", Value::Int(1))?,
        AttrKey::Field {
            node: 1,
            field: "f".into(),
        },
    )?;
    single_key(
        GraphEdit::remove_field(&g, 0, "age")?,
        AttrKey::Field {
            node: 0,
            field: "age".into(),
        },
    )?;
    let meta = NodeMeta {
        position: Some(1),
        ..NodeMeta::default()
    };
    single_key(
        GraphEdit::set_node_meta(&g, 1, meta)?,
        AttrKey::Meta { node: 1 },
    )?;

    Ok(())
}

#[test]
fn graph_edit_to_delta_cases_for_structure_and_batches() -> TestResult {
    let g = triangle();
    let remove = GraphEdit::remove_node_cascade(&g, 0)?.to_delta()?;
    assert!(GraphEdit::Identity.to_delta()?.is_identity());
    assert_eq!(
        GraphEdit::AddNode { node: person(5) }.to_delta()?,
        add_node(5)
    );
    let e = GEdge::new(0, 1, label("likes"));
    assert_eq!(
        GraphEdit::AddEdge {
            id: EdgeId(5),
            edge: e.clone()
        }
        .to_delta()?,
        GraphDelta::identity().with_added_edge(EdgeId(5), e.clone())
    );
    assert_eq!(
        GraphEdit::RemoveEdge {
            id: EdgeId(5),
            edge: e.clone()
        }
        .to_delta()?,
        GraphDelta::identity().with_deleted_edge(EdgeId(5), e)
    );
    assert_eq!(GraphEdit::Delta(remove.clone()).to_delta()?, remove);
    let batch = GraphEdit::Batch(vec![
        GraphEdit::AddNode { node: person(5) },
        GraphEdit::RelabelNode {
            node: 5,
            old_anchor: "person".into(),
            new_anchor: "agent".into(),
        },
    ]);
    assert_eq!(
        batch.to_delta()?,
        add_node(5).compose(&anchor_change(5, "person", "agent")?)?
    );
    assert_eq!(
        batch.to_delta()?,
        GraphDelta::identity().with_added_node(Node::new(5, "agent"))
    );
    // A reconnect inside a batch reads the edge the earlier steps produced.
    let moved = GraphEdit::Batch(vec![
        GraphEdit::AddEdge {
            id: EdgeId(6),
            edge: GEdge::new(0, 1, label("likes")),
        },
        GraphEdit::Reconnect {
            id: EdgeId(6),
            old: (0, 1),
            new: (2, 2),
        },
    ]);
    assert_eq!(
        moved.to_delta_in(&g)?,
        GraphDelta::identity().with_added_edge(EdgeId(6), GEdge::new(2, 2, label("likes")))
    );
    Ok(())
}

fn add_node(id: u32) -> GraphDelta {
    GraphDelta::identity().with_added_node(person(id))
}

#[test]
fn graph_edit_invert_is_a_delta() -> TestResult {
    let g = triangle();
    let edit = GraphEdit::relabel_node(&g, 1, "agent")?;
    let inverse = edit.invert()?;
    assert!(
        matches!(&inverse, GraphEdit::Delta(d) if *d == edit.to_delta().map(|d| d.invert()).unwrap_or_default())
    );
    let mut h = g.clone();
    edit.apply(&mut h)?;
    inverse.apply(&mut h)?;
    assert!(GInstance::identical(&h, &g));
    Ok(())
}

#[test]
fn remove_node_cascade_lists_every_incident_edge() -> TestResult {
    let g = triangle().with_edge_id(EdgeId(7), 1, 1, label("self"));
    let GraphEdit::RemoveNode { node, incident } = GraphEdit::remove_node_cascade(&g, 1)? else {
        return Err("not a RemoveNode".into());
    };
    assert_eq!(node.id, 1);
    assert_eq!(
        incident.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![EdgeId(0), EdgeId(1), EdgeId(7)]
    );
    let mut h = g.clone();
    GraphEdit::remove_node_cascade(&g, 1)?.apply(&mut h)?;
    assert!(h.node(1).is_none());
    assert_eq!(h.edge_count(), 1);
    assert_eq!(
        GraphEdit::remove_node_cascade(&g, 9).err(),
        Some(EditError::NodeNotFound(9))
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Errors and their details
// ---------------------------------------------------------------------------

fn round_trips(detail: &ErrorDetail) -> TestResult {
    let json = serde_json::to_string(detail)?;
    assert_eq!(&serde_json::from_str::<ErrorDetail>(&json)?, detail);
    let packed = rmp_serde::to_vec_named(detail)?;
    assert_eq!(&rmp_serde::from_slice::<ErrorDetail>(&packed)?, detail);
    let mut cbor = Vec::new();
    ciborium::into_writer(detail, &mut cbor)?;
    assert_eq!(
        &ciborium::from_reader::<ErrorDetail, _>(cbor.as_slice())?,
        detail
    );
    Ok(())
}

#[test]
fn error_detail_edit_conflict_round_trips() -> TestResult {
    let detail = ErrorDetail::EditConflict {
        path: vec![1, 0],
        kind: "foreign_key_not_found".into(),
        node: Some(1),
        edge: Some(2),
        row: Some(3),
        pair: Some(("a-fk->b".into(), 4, 5)),
    };
    assert!(
        serde_json::to_string(&detail)?.starts_with(r#"{"edit_conflict":{"path":[1,0],"kind":"#)
    );
    round_trips(&detail)
}

#[test]
fn error_detail_independence_round_trips() -> TestResult {
    let detail = ErrorDetail::Independence {
        conflicts: vec![
            EditConflict::DeleteUse {
                deleted: ElemRef::Node(1),
                user: ElemRef::Attr(AttrKey::Field {
                    node: 1,
                    field: "f".into(),
                }),
            },
            EditConflict::AnchorUse {
                node: 2,
                user: ElemRef::Pair {
                    edge: Box::new(label("knows")),
                    src: RowId(0),
                    tgt: RowId(1),
                },
            },
        ],
    };
    assert!(serde_json::to_string(&detail)?.starts_with(r#"{"independence":{"conflicts":["#));
    round_trips(&detail)
}

/// One value of every `EditError` variant, with the `kind` its detail
/// carries.
fn every_edit_error() -> Vec<(EditError, &'static str)> {
    vec![
        (EditError::NodeNotFound(1), "node_not_found"),
        (
            EditError::AnchorMismatch {
                node_id: 1,
                expected: "a".into(),
                actual: "b".into(),
            },
            "anchor_mismatch",
        ),
        (EditError::FanNotFound("h".into()), "fan_not_found"),
        (EditError::ApplyFailed("x".into()), "apply_failed"),
        (
            EditError::FieldTransformFailed("x".into()),
            "field_transform_failed",
        ),
        (EditError::TableNotFound("t".into()), "table_not_found"),
        (
            EditError::RowNotFound {
                table: "t".into(),
                key: "k".into(),
            },
            "row_not_found",
        ),
        (
            EditError::CycleDetected {
                node_id: 1,
                new_parent: 2,
            },
            "cycle_detected",
        ),
        (EditError::ParentNotFound(1), "parent_not_found"),
        (EditError::DuplicateNodeId(1), "duplicate_node_id"),
        (EditError::EdgeNotFound(EdgeId(1)), "edge_not_found"),
        (EditError::DuplicateEdge(EdgeId(1)), "duplicate_edge"),
        (
            EditError::MissingEndpoint {
                edge: EdgeId(1),
                node: 2,
            },
            "missing_endpoint",
        ),
        (
            EditError::DanglingEdges {
                node: 1,
                edges: vec![EdgeId(2)],
            },
            "dangling_edges",
        ),
        (EditError::StaleNode { node: 1 }, "stale_node"),
        (EditError::StaleValue { node: 1 }, "stale_value"),
        (
            EditError::StaleField {
                node: 1,
                field: "f".into(),
            },
            "stale_field",
        ),
        (EditError::StaleAnchor { node: 1 }, "stale_anchor"),
        (EditError::StaleMeta { node: 1 }, "stale_meta"),
        (EditError::StaleEdge { edge: EdgeId(1) }, "stale_edge"),
        (EditError::StaleLabel { edge: EdgeId(1) }, "stale_label"),
        (
            EditError::StalePosition { edge: EdgeId(1) },
            "stale_position",
        ),
        (
            EditError::MalformedFan {
                node: 1,
                reason: FanViolation::MissingParent,
            },
            "malformed_fan",
        ),
        (
            EditError::Compose(ComposeError::Mismatch {
                elem: ElemRef::Node(1),
            }),
            "compose_mismatch",
        ),
        (
            EditError::Compose(ComposeError::NeedsState {
                elem: ElemRef::Edge(EdgeId(1)),
            }),
            "compose_needs_state",
        ),
        (
            EditError::NotTreeEdit {
                reason: "label change".into(),
            },
            "not_tree_edit",
        ),
    ]
}

/// One value of every table `EditError` variant, with its detail `kind`.
fn every_table_edit_error() -> Vec<(EditError, &'static str)> {
    let fk = label("fk");
    vec![
        (EditError::DuplicateRow(RowId(1)), "duplicate_row"),
        (
            EditError::StaleCell {
                row: RowId(1),
                column: "c".into(),
            },
            "stale_cell",
        ),
        (EditError::StaleTable { row: RowId(1) }, "stale_table"),
        (
            EditError::StaleIncidence { row: RowId(1) },
            "stale_incidence",
        ),
        (
            EditError::ForeignKeyNotFound {
                edge: fk.clone(),
                src: RowId(1),
                tgt: RowId(2),
            },
            "foreign_key_not_found",
        ),
        (
            EditError::DuplicateForeignKey {
                edge: fk.clone(),
                src: RowId(1),
                tgt: RowId(2),
            },
            "duplicate_foreign_key",
        ),
        (
            EditError::MissingRowEndpoint {
                edge: fk.clone(),
                row: RowId(2),
            },
            "missing_row_endpoint",
        ),
        (
            EditError::DanglingForeignKeys {
                row: RowId(1),
                pairs: vec![(fk, RowId(1), RowId(2))],
            },
            "dangling_foreign_keys",
        ),
        (
            EditError::NotTableEdit {
                reason: "fan node".into(),
            },
            "not_table_edit",
        ),
    ]
}

#[test]
fn every_edit_error_has_a_detail() -> TestResult {
    for (err, kind) in every_edit_error()
        .into_iter()
        .chain(every_table_edit_error())
    {
        let wrapped = EditError::BatchStep {
            index: 3,
            source: Box::new(EditError::BatchStep {
                index: 1,
                source: Box::new(err.clone()),
            }),
        };
        let ErrorDetail::EditConflict {
            path, kind: found, ..
        } = wrapped.detail()
        else {
            return Err(format!("{err:?} has no edit_conflict detail").into());
        };
        assert_eq!(path, vec![3, 1]);
        assert_eq!(found, kind);
        assert_eq!(err.clone(), err);
        assert!(serde_json::to_string(&err).is_ok());
        assert!(!err.to_string().is_empty());
        round_trips(&err.detail())?;
    }
    Ok(())
}

#[test]
fn edit_error_detail_is_structured() {
    let detail = EditError::ForeignKeyNotFound {
        edge: Edge {
            src: "order".into(),
            tgt: "customer".into(),
            kind: "fk".into(),
            name: Some("buyer".into()),
        },
        src: RowId(3),
        tgt: RowId(9),
    }
    .detail();
    assert_eq!(
        detail,
        ErrorDetail::EditConflict {
            path: vec![],
            kind: "foreign_key_not_found".into(),
            node: None,
            edge: None,
            row: None,
            pair: Some(("order-fk:buyer->customer".into(), 3, 9)),
        }
    );
    let attr = EditError::Compose(ComposeError::Mismatch {
        elem: ElemRef::Attr(AttrKey::Cell {
            row: RowId(4),
            column: "c".into(),
        }),
    })
    .detail();
    assert!(matches!(
        attr,
        ErrorDetail::EditConflict {
            row: Some(4),
            node: None,
            ..
        }
    ));
    let endpoint = EditError::MissingEndpoint {
        edge: EdgeId(2),
        node: 7,
    }
    .detail();
    assert!(matches!(
        endpoint,
        ErrorDetail::EditConflict {
            node: Some(7),
            edge: Some(2),
            ..
        }
    ));
}

#[test]
fn table_elements_map_to_graph_elements() {
    let pair_edge = |_: &Edge, src: RowId, tgt: RowId| Some(EdgeId(src.0 * 10 + tgt.0));
    assert_eq!(
        ElemRef::Row(RowId(3)).to_graph(pair_edge),
        Some(ElemRef::Node(3))
    );
    assert_eq!(
        ElemRef::Pair {
            edge: Box::new(label("fk")),
            src: RowId(1),
            tgt: RowId(2)
        }
        .to_graph(pair_edge),
        Some(ElemRef::Edge(EdgeId(12)))
    );
    assert_eq!(
        ElemRef::Attr(AttrKey::Cell {
            row: RowId(1),
            column: "c".into()
        })
        .to_graph(pair_edge),
        Some(ElemRef::Attr(AttrKey::Field {
            node: 1,
            field: "c".into()
        }))
    );
    assert_eq!(
        ElemRef::Attr(AttrKey::Table { row: RowId(1) }).to_graph(pair_edge),
        Some(ElemRef::Attr(AttrKey::Anchor { node: 1 }))
    );
    assert_eq!(
        ElemRef::Edge(EdgeId(5)).to_graph(pair_edge),
        Some(ElemRef::Edge(EdgeId(5)))
    );
    assert_eq!(
        ElemRef::Pair {
            edge: Box::new(label("fk")),
            src: RowId(1),
            tgt: RowId(2)
        }
        .to_graph(|_, _, _| None),
        None
    );
}

// ---------------------------------------------------------------------------
// Canonical bytes
// ---------------------------------------------------------------------------

/// Hand-written little-endian encoders, independent of the sink, for the
/// pinned layout.
struct Bytes(Vec<u8>);

impl Bytes {
    fn count(&mut self, n: u64) -> &mut Self {
        self.0.extend_from_slice(&n.to_le_bytes());
        self
    }
    fn u32(&mut self, n: u32) -> &mut Self {
        self.0.extend_from_slice(&n.to_le_bytes());
        self
    }
    fn byte(&mut self, b: u8) -> &mut Self {
        self.0.push(b);
        self
    }
    fn text(&mut self, s: &str) -> &mut Self {
        self.count(s.len() as u64);
        self.0.extend_from_slice(s.as_bytes());
        self
    }
    /// A plain node with no discriminator, fields, position or annotations;
    /// `value` is the value's own bytes after the presence byte.
    fn node(&mut self, id: u32, anchor: &str, value: Option<&[u8]>) -> &mut Self {
        self.u32(id).text(anchor);
        match value {
            None => {
                self.byte(0);
            }
            Some(bytes) => {
                self.byte(1);
                self.0.extend_from_slice(bytes);
            }
        }
        self.byte(0).count(0).byte(0).byte(0).count(0)
    }
    /// A schema edge `person -kind-> person` without a name.
    fn label(&mut self, kind: &str) -> &mut Self {
        self.text("person").text("person").text(kind).byte(0)
    }
}

/// The delta of the pinned byte layout: every part non-empty.
fn pinned_delta() -> Result<GraphDelta, MalformedDelta> {
    let mut added = Node::new(5, "x");
    added.value = Some(FieldPresence::Present(Value::Int(7)));
    GraphDelta::identity()
        .with_deleted_node(person(1))
        .with_deleted_edge(EdgeId(0), GEdge::new(0, 1, label("knows")))
        .with_added_node(added)
        .with_added_edge(
            EdgeId(3),
            GEdge::new(5, 5, label("loop")).with_position(Some(2)),
        )
        .with_change(
            AttrKey::Anchor { node: 0 },
            AttrVal::Anchor("a".into()),
            AttrVal::Anchor("b".into()),
        )
}

#[test]
fn graph_delta_golden_bytes_follow_the_layout() -> TestResult {
    let delta = pinned_delta()?;
    let mut want = Bytes(Vec::new());
    want.count(1).count(1).count(1).count(1).count(1);
    want.node(1, "person", None);
    want.u32(0).u32(0).u32(1).label("knows").byte(0);
    let mut seven = vec![1u8];
    seven.extend_from_slice(&7i64.to_le_bytes());
    want.node(5, "x", Some(&seven));
    want.u32(3).u32(5).u32(5).label("loop").byte(1).u32(2);
    want.byte(3).u32(0);
    want.byte(3).text("a").byte(3).text("b");
    let mut writer = panproto_schema::CanonicalWriter::new();
    canonical_graph_delta_bytes(&delta, &mut writer);
    assert_eq!(writer.into_bytes(), want.0);
    Ok(())
}

#[test]
fn canonical_graph_delta_bytes_writes_through_the_callers_sink() -> TestResult {
    let delta = pinned_delta()?;
    let mut writer = panproto_schema::CanonicalWriter::new();
    canonical_graph_delta_bytes(&delta, &mut writer);
    let mut hasher = crate::identity::Digest::hasher(crate::identity::GRAPH_DELTA_CONTEXT);
    canonical_graph_delta_bytes(&delta, &mut hasher);
    assert_eq!(hasher.finalize(), delta.digest());
    assert_eq!(
        crate::identity::Digest::derive(crate::identity::GRAPH_DELTA_CONTEXT, writer.as_bytes()),
        delta.digest()
    );
    Ok(())
}

#[test]
fn graph_delta_golden_digest_is_pinned() -> TestResult {
    let delta = pinned_delta()?;
    assert_eq!(delta.digest().to_hex(), PINNED_DIGEST);
    assert_eq!(
        GraphDelta::identity().digest().to_hex(),
        crate::identity::Digest::derive(crate::identity::GRAPH_DELTA_CONTEXT, &[0u8; 40]).to_hex()
    );
    Ok(())
}

/// The digest of [`pinned_delta`] under the graph delta context.
const PINNED_DIGEST: &str = "dbe5cc0b13835c65c731a0e61b0b0d1ab84ba4e6dcc1939b81d007190a1cdaeb";

#[test]
fn attr_encoders_cover_every_variant() {
    let keys = [
        AttrKey::Value { node: 1 },
        AttrKey::Field {
            node: 1,
            field: "f".into(),
        },
        AttrKey::Meta { node: 1 },
        AttrKey::Anchor { node: 1 },
        AttrKey::EdgeLabel { edge: EdgeId(1) },
        AttrKey::EdgePosition { edge: EdgeId(1) },
        AttrKey::Cell {
            row: RowId(1),
            column: "f".into(),
        },
        AttrKey::Table { row: RowId(1) },
    ];
    let tags: Vec<u8> = keys
        .iter()
        .map(|k| {
            let mut w = panproto_schema::CanonicalWriter::new();
            write_attr_key(&mut w, k);
            w.as_bytes()[0]
        })
        .collect();
    assert_eq!(tags, (0..8).collect::<Vec<u8>>());
    let meta = NodeMeta {
        discriminator: Some("d".into()),
        position: Some(1),
        shape: NodeShape::XmlElement { tag: "t".into() },
        annotations: BTreeMap::from([("k".to_owned(), Value::Bool(true))]),
    };
    let vals = [
        AttrVal::Presence(Some(FieldPresence::Absent)),
        AttrVal::Field(Some(Value::Null)),
        AttrVal::Meta(meta),
        AttrVal::Anchor("a".into()),
        AttrVal::Label(label("knows")),
        AttrVal::Position(Some(3)),
    ];
    let tags: Vec<u8> = vals
        .iter()
        .map(|v| {
            let mut w = panproto_schema::CanonicalWriter::new();
            write_attr_val(&mut w, v);
            w.as_bytes()[0]
        })
        .collect();
    assert_eq!(tags, (0..6).collect::<Vec<u8>>());
}
