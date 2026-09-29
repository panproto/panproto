//! A graph instance has one canonical byte string and one digest.
//!
//! The canonical bytes are a header and self-contained node and edge
//! records in ascending id, written through a `CanonicalSink`; the digest is
//! those bytes under `GRAPH_INSTANCE_CONTEXT`, and can be computed by
//! streaming the records. The expected bytes below are written out by hand
//! from the documented layout.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_inst::identity::{CanonicalSink, Digest, GRAPH_INSTANCE_CONTEXT};
use panproto_inst::{
    EdgeId, FieldPresence, GEdge, GInstance, Node, NodeShape, Value, canonical_ginstance_bytes,
    write_edge_record, write_node_record,
};
use panproto_schema::{CanonicalWriter, Edge};

fn link() -> Edge {
    Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: "k".into(),
        name: Some("n".into()),
    }
}

/// Two nodes and one positioned edge, with a value, a discriminator, an
/// extra field, a node position, a shape and an annotation among them.
fn small_graph() -> GInstance {
    let mut first = Node::new(3, "a");
    first.value = Some(FieldPresence::Present(Value::Int(-2)));
    first.discriminator = Some("d".into());
    first.extra_fields.insert("x".into(), Value::Bool(true));
    let mut second = Node::new(1, "b").with_shape(NodeShape::List);
    second.position = Some(7);
    second.annotations.insert("y".into(), Value::Null);
    let mut graph = GInstance::new().with_node(first).with_node(second);
    graph
        .insert_edge(EdgeId(9), GEdge::new(3, 1, link()).with_position(Some(4)))
        .expect("both ends exist");
    graph
}

fn u32le(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn u64le(v: u64) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn string(s: &str) -> Vec<u8> {
    let mut out = u64le(s.len() as u64);
    out.extend_from_slice(s.as_bytes());
    out
}

fn expected_small_graph_bytes() -> Vec<u8> {
    let mut b = Vec::new();
    // Header: 2 nodes, 1 edge.
    b.extend(u64le(2));
    b.extend(u64le(1));
    // Node 1: anchor "b", no value, no discriminator, no extra fields,
    // position 7, shape list, one annotation y = Null.
    b.extend(u32le(1));
    b.extend(string("b"));
    b.push(0);
    b.push(0);
    b.extend(u64le(0));
    b.push(1);
    b.extend(u32le(7));
    b.push(1);
    b.extend(u64le(1));
    b.extend(string("y"));
    b.push(8);
    // Node 3: anchor "a", value Int(-2), discriminator "d", extra x = true,
    // no position, plain, no annotations.
    b.extend(u32le(3));
    b.extend(string("a"));
    b.push(1);
    b.push(1);
    b.extend((-2_i64).to_le_bytes());
    b.push(1);
    b.extend(string("d"));
    b.extend(u64le(1));
    b.extend(string("x"));
    b.push(0);
    b.push(1);
    b.push(0);
    b.push(0);
    b.extend(u64le(0));
    // Edge 9: 3 -> 1, label a -> b kind k name n, position 4.
    b.extend(u32le(9));
    b.extend(u32le(3));
    b.extend(u32le(1));
    b.extend(string("a"));
    b.extend(string("b"));
    b.extend(string("k"));
    b.push(1);
    b.extend(string("n"));
    b.push(1);
    b.extend(u32le(4));
    b
}

#[test]
fn canonical_graph_bytes_follow_the_record_layout() {
    assert_eq!(
        canonical_ginstance_bytes(&small_graph()),
        expected_small_graph_bytes()
    );
    let mut empty = u64le(0);
    empty.extend(u64le(0));
    assert_eq!(canonical_ginstance_bytes(&GInstance::new()), empty);
}

#[test]
fn graph_digest_uses_the_graph_instance_context() {
    let graph = small_graph();
    let digest = graph.digest();
    assert_eq!(
        digest,
        Digest::derive(GRAPH_INSTANCE_CONTEXT, &canonical_ginstance_bytes(&graph))
    );
    assert_ne!(
        digest,
        Digest::derive(
            "panproto 2026-09 table instance v1",
            &canonical_ginstance_bytes(&graph)
        ),
        "the context separates graph digests from other digests of the same bytes"
    );
    // Pinned values: a change here is a change of encoding, which bumps the
    // context version.
    assert_eq!(
        GInstance::new().digest().to_hex(),
        Digest::derive(GRAPH_INSTANCE_CONTEXT, &[0; 16]).to_hex()
    );
    assert_eq!(digest.to_hex(), PINNED_SMALL_GRAPH_DIGEST);
}

/// The digest of `small_graph()` under `GRAPH_INSTANCE_CONTEXT` v1.
const PINNED_SMALL_GRAPH_DIGEST: &str =
    "49d99ba6a6cbf024a509c08e398aa5037796e6cc677f6eb6ccdc08905ef799d7";

#[test]
fn a_streamed_graph_digest_equals_the_whole_digest() {
    let graph = small_graph();
    let mut hasher = Digest::hasher(GRAPH_INSTANCE_CONTEXT);
    hasher.len(graph.node_count());
    hasher.len(graph.edge_count());
    // Each record is encoded on its own, with no reference to what was
    // written before it, and fed to the hasher in pieces.
    let mut ids: Vec<u32> = graph.nodes().keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let mut record = CanonicalWriter::new();
        write_node_record(&mut record, graph.node(id).expect("a node"));
        for chunk in record.as_bytes().chunks(3) {
            hasher.update(chunk);
        }
    }
    for (id, edge) in graph.edges_sorted() {
        let mut record = CanonicalWriter::new();
        write_edge_record(&mut record, id, edge);
        hasher.update(record.as_bytes());
    }
    assert_eq!(hasher.finalize(), graph.digest());
}

#[test]
fn renumbering_positions_changes_no_digest_field_but_position() {
    // Shift every position while keeping their order: the records differ
    // only in the position fields, and no other field (no rank or path)
    // moves.
    let graph = small_graph();
    let mut shifted = graph.clone();
    shifted.node_mut(1).expect("a node").position = Some(70);
    shifted
        .set_edge_position(EdgeId(9), Some(40))
        .expect("the edge exists");
    let before = canonical_ginstance_bytes(&graph);
    let after = canonical_ginstance_bytes(&shifted);
    assert_eq!(before.len(), after.len());
    let differing: Vec<usize> = before
        .iter()
        .zip(&after)
        .enumerate()
        .filter(|(_, (x, y))| x != y)
        .map(|(i, _)| i)
        .collect();
    // Node 1's position is the four bytes after its header, id, anchor,
    // value tag, discriminator tag, extra-field count and presence byte.
    let node_position = 16 + 4 + 9 + 1 + 1 + 8 + 1;
    let edge_position = before.len() - 4;
    assert_eq!(
        differing,
        vec![node_position, edge_position],
        "only the low byte of each position changes"
    );
    // The rest of the graph is untouched, and the node records other than
    // node 1 are identical.
    let mut record = CanonicalWriter::new();
    write_node_record(&mut record, graph.node(3).expect("a node"));
    let mut again = CanonicalWriter::new();
    write_node_record(&mut again, shifted.node(3).expect("a node"));
    assert_eq!(record.as_bytes(), again.as_bytes());
    assert_ne!(graph.digest(), shifted.digest());
}

#[test]
fn a_digest_does_not_depend_on_insertion_order() {
    let forward = small_graph();
    let mut backward = GInstance::new();
    let mut ids: Vec<u32> = forward.nodes().keys().copied().collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    for id in ids {
        backward = backward.with_node(forward.node(id).expect("a node").clone());
    }
    for (id, edge) in forward.edges_sorted() {
        backward.insert_edge(id, edge.clone()).expect("ends exist");
    }
    assert_eq!(forward.digest(), backward.digest());
}
