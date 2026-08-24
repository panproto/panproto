//! Everything a committed instance carries survives being read back.
//!
//! The version-control store holds a data set as `MessagePack`, and
//! `MessagePack` writes a struct as an array of its fields in declaration
//! order rather than as a named map. Leaving a field out of that array
//! therefore does not mark it absent: it shifts every later field one slot
//! left, and the decoder reads each of them as the wrong thing. A node
//! carrying an annotation, or a complement recording its arc edges, could not
//! be read back at all.
//!
//! These fixtures populate one such field at a time and read the value back.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::{Complement, Fan, FieldPresence, Node, NodeShape, Value, WInstance};
use panproto_schema::Edge;

fn prop_edge(name: &str) -> Edge {
    Edge {
        src: "rec".into(),
        tgt: Name::from(format!("rec.{name}").as_str()),
        kind: "prop".into(),
        name: Some(Name::from(name)),
    }
}

/// A one-node instance holding `node`, encoded and read back.
fn round_trip_node(node: Node) -> Node {
    let mut nodes = HashMap::new();
    nodes.insert(node.id, node);
    let instance = WInstance::new(nodes, Vec::new(), Vec::new(), 0, "rec".into());
    let bytes = rmp_serde::to_vec(&instance).expect("the fixture serializes");
    let back: WInstance = rmp_serde::from_slice(&bytes).expect("the encoding parses");
    back.nodes.get(&0).expect("the node is still there").clone()
}

fn round_trip_complement(complement: &Complement) -> Complement {
    let bytes = rmp_serde::to_vec(complement).expect("the fixture serializes");
    rmp_serde::from_slice(&bytes).expect("the encoding parses")
}

#[test]
fn an_annotated_node_reads_back() {
    let mut node = Node::new(0, "rec");
    node.value = Some(FieldPresence::Present(Value::Int(7)));
    node.annotations
        .insert("origin".to_owned(), Value::Str("upstream".to_owned()));

    let back = round_trip_node(node);
    assert_eq!(
        back.annotations.get("origin"),
        Some(&Value::Str("upstream".to_owned()))
    );
    assert_eq!(back.value, Some(FieldPresence::Present(Value::Int(7))));
}

#[test]
fn every_optional_field_of_a_node_reads_back_together() {
    let mut node = Node::new(0, "rec");
    node.value = Some(FieldPresence::Present(Value::Str("v".to_owned())));
    node.discriminator = Some("app.bsky.feed.post".into());
    node.extra_fields.insert("kept".to_owned(), Value::Int(1));
    node.position = Some(3);
    node.shape = NodeShape::XmlElement { tag: "NAF".into() };
    node.annotations
        .insert("note".to_owned(), Value::Bool(true));

    let back = round_trip_node(node);
    assert_eq!(back.id, 0);
    assert_eq!(back.anchor, Name::from("rec"));
    assert_eq!(
        back.value,
        Some(FieldPresence::Present(Value::Str("v".to_owned())))
    );
    assert_eq!(back.discriminator, Some("app.bsky.feed.post".into()));
    assert_eq!(back.extra_fields.get("kept"), Some(&Value::Int(1)));
    assert_eq!(back.position, Some(3));
    assert_eq!(back.shape, NodeShape::XmlElement { tag: "NAF".into() });
    assert_eq!(back.annotations.get("note"), Some(&Value::Bool(true)));
}

#[test]
fn a_plain_node_still_reads_back() {
    let back = round_trip_node(Node::new(0, "rec"));
    assert_eq!(back.position, None);
    assert_eq!(back.shape, NodeShape::Plain);
    assert!(back.annotations.is_empty());
}

#[test]
fn a_complement_recording_its_arc_edges_reads_back() {
    let mut complement = Complement::empty();
    complement.arc_edges.insert((0, 1), prop_edge("f"));

    let back = round_trip_complement(&complement);
    assert_eq!(back.arc_edges.get(&(0, 1)), Some(&prop_edge("f")));
}

#[test]
fn every_table_of_a_complement_reads_back_together() {
    let mut complement = Complement::empty();
    complement.dropped_nodes.insert(1, Node::new(1, "rec.d"));
    complement.dropped_arcs.push((0, 1, prop_edge("d")));
    complement
        .dropped_fans
        .push(Fan::new("rec.all", 0).with_child("l", 1));
    complement
        .contraction_choices
        .insert((0, 1), prop_edge("c"));
    complement.original_parent.insert(1, 0);
    complement.source_fingerprint = 0x0123_4567_89ab_cdef;
    complement
        .original_extra_fields
        .insert(1, HashMap::from([("was".to_owned(), Value::Int(9))]));
    complement.arc_edges.insert((0, 1), prop_edge("f"));
    complement.arc_order.push((0, 1));
    complement
        .original_values
        .insert(1, Some(FieldPresence::Present(Value::Int(4))));
    complement.synthesized_nodes.insert(2);
    complement.contracted_into.insert(1, 0);

    let back = round_trip_complement(&complement);
    assert_eq!(back.dropped_nodes.len(), 1);
    assert_eq!(back.dropped_arcs, complement.dropped_arcs);
    assert_eq!(back.dropped_fans, complement.dropped_fans);
    assert_eq!(back.contraction_choices, complement.contraction_choices);
    assert_eq!(back.original_parent, complement.original_parent);
    assert_eq!(back.source_fingerprint, complement.source_fingerprint);
    assert_eq!(back.original_extra_fields, complement.original_extra_fields);
    assert_eq!(back.arc_edges, complement.arc_edges);
    assert_eq!(back.arc_order, complement.arc_order);
    assert_eq!(back.original_values, complement.original_values);
    assert_eq!(back.synthesized_nodes, complement.synthesized_nodes);
    assert_eq!(back.contracted_into, complement.contracted_into);
}

#[test]
fn an_empty_complement_still_reads_back() {
    let back = round_trip_complement(&Complement::empty());
    assert!(back.dropped_nodes.is_empty());
    assert!(back.arc_edges.is_empty());
    assert_eq!(back.source_fingerprint, 0);
}
