#![allow(clippy::unwrap_used, clippy::expect_used)]
//! `to_json` writes a named, unordered outgoing edge of a node's anchor as
//! one JSON member, so a node holding two arcs on it is refused rather than
//! written as an array of the field's values. Repeated arcs on an anonymous
//! or ordered edge, and the children of a node marked as a list, are still
//! written as arrays.

use std::collections::HashMap;

use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{Node, NodeShape, ParseError, WInstance, to_json};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};
use serde_json::json;

fn edge(src: &str, tgt: &str, kind: &str, name: Option<&str>) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: name.map(Into::into),
    }
}

/// A record `post` with the string field `text`, and a `tags` vertex whose
/// single outgoing edge is the anonymous item slot.
fn schema() -> Schema {
    let protocol = Protocol::default();
    SchemaBuilder::new(&protocol)
        .vertex("post", "object", None)
        .unwrap()
        .vertex("post.text", "string", None)
        .unwrap()
        .vertex("tags", "array", None)
        .unwrap()
        .vertex("tag", "string", None)
        .unwrap()
        .edge("post", "post.text", "prop", Some("text"))
        .unwrap()
        .edge("tags", "tag", "items", None)
        .unwrap()
        .build()
        .unwrap()
}

fn leaf(id: u32, anchor: &str, text: &str) -> (u32, Node) {
    (
        id,
        Node::new(id, anchor).with_value(FieldPresence::Present(Value::Str(text.into()))),
    )
}

/// The node `root` holding one leaf per value, each on the edge `on`.
fn instance(root: Node, on: &Edge, values: &[&str]) -> WInstance {
    let mut nodes = HashMap::from([(0, root)]);
    let mut arcs = Vec::new();
    for (child, value) in (1..).zip(values) {
        nodes.extend([leaf(child, &on.tgt, value)]);
        arcs.push((0, child, on.clone()));
    }
    WInstance::new(nodes, arcs, Vec::new(), 0, "post".into())
}

#[test]
fn two_arcs_on_a_named_prop_edge_are_refused() {
    let text = edge("post", "post.text", "prop", Some("text"));
    let inst = instance(Node::new(0, "post"), &text, &["hi", "alice.test"]);
    match to_json(&schema(), &inst) {
        Err(ParseError::MultipleValuesOnField { path, field, count }) => {
            assert_eq!((path.as_str(), field.as_str(), count), ("$", "text", 2));
        }
        other => panic!("expected MultipleValuesOnField, got {other:?}"),
    }
}

#[test]
fn one_arc_on_a_named_prop_edge_is_an_object_member() {
    let text = edge("post", "post.text", "prop", Some("text"));
    let inst = instance(Node::new(0, "post"), &text, &["hi"]);
    assert_eq!(to_json(&schema(), &inst).unwrap(), json!({"text": "hi"}));
}

#[test]
fn a_node_marked_as_a_list_is_an_array_even_on_a_named_edge() {
    let text = edge("post", "post.text", "prop", Some("text"));
    let root = Node::new(0, "post").with_shape(NodeShape::List);
    let inst = instance(root, &text, &["hi", "alice.test"]);
    assert_eq!(
        to_json(&schema(), &inst).unwrap(),
        json!(["hi", "alice.test"])
    );
}

#[test]
fn repeated_arcs_on_an_anonymous_edge_are_an_array() {
    let item = edge("tags", "tag", "items", None);
    let inst = instance(Node::new(0, "tags"), &item, &["a", "b"]);
    assert_eq!(to_json(&schema(), &inst).unwrap(), json!(["a", "b"]));
}

#[test]
fn repeated_arcs_on_an_ordered_named_edge_are_an_array() {
    let mut schema = schema();
    let ordered = edge("post", "post.text", "prop", Some("text"));
    schema.orderings.insert(ordered.clone(), 0);
    let inst = instance(Node::new(0, "post"), &ordered, &["a", "b"]);
    assert_eq!(to_json(&schema, &inst).unwrap(), json!(["a", "b"]));
}

#[test]
fn repeated_arcs_on_an_edge_the_anchor_does_not_declare_are_an_array() {
    let foreign = edge("elsewhere", "post.text", "prop", Some("note"));
    let inst = instance(Node::new(0, "post"), &foreign, &["a", "b"]);
    assert_eq!(to_json(&schema(), &inst).unwrap(), json!(["a", "b"]));
}

#[test]
fn a_repeated_field_beside_other_fields_is_refused_at_its_path() {
    let protocol = Protocol::default();
    let schema = SchemaBuilder::new(&protocol)
        .vertex("post", "object", None)
        .unwrap()
        .vertex("post.text", "string", None)
        .unwrap()
        .vertex("post.reply", "object", None)
        .unwrap()
        .edge("post", "post.reply", "prop", Some("reply"))
        .unwrap()
        .edge("post.reply", "post.text", "prop", Some("text"))
        .unwrap()
        .edge("post.reply", "post.text", "prop", Some("lang"))
        .unwrap()
        .build()
        .unwrap();
    let reply = edge("post", "post.reply", "prop", Some("reply"));
    let text = edge("post.reply", "post.text", "prop", Some("text"));
    let lang = edge("post.reply", "post.text", "prop", Some("lang"));
    let nodes = HashMap::from([
        (0, Node::new(0, "post")),
        (1, Node::new(1, "post.reply")),
        leaf(2, "post.text", "hi"),
        leaf(3, "post.text", "en"),
        leaf(4, "post.text", "again"),
    ]);
    let arcs = vec![
        (0, 1, reply),
        (1, 2, text.clone()),
        (1, 3, lang),
        (1, 4, text),
    ];
    let inst = WInstance::new(nodes, arcs, Vec::new(), 0, "post".into());
    match to_json(&schema, &inst) {
        Err(ParseError::MultipleValuesOnField { path, field, count }) => {
            assert_eq!(
                (path.as_str(), field.as_str(), count),
                ("$.reply", "text", 2)
            );
        }
        other => panic!("expected MultipleValuesOnField, got {other:?}"),
    }
}
