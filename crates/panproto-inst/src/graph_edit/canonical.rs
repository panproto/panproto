//! Canonical bytes and digest of a graph delta.

use std::collections::BTreeMap;

use panproto_schema::CanonicalWriter;

use super::delta::{AttrChange, AttrKey, AttrVal, GraphDelta, NodeMeta};
use crate::ginstance::{GEdge, write_edge_record, write_node_record};
use crate::identity::{CanonicalSink, Digest, GRAPH_DELTA_CONTEXT};
use crate::metadata::{Node, NodeShape};
use crate::value::{FieldPresence, Value};

/// Write an optional `u32` as a presence byte and then the value.
fn opt_u32<S: CanonicalSink + ?Sized>(sink: &mut S, value: Option<u32>) {
    match value {
        None => sink.bool(false),
        Some(v) => {
            sink.bool(true);
            sink.u32(v);
        }
    }
}

/// Write an attribution key: a one-byte tag in declaration order (`0`
/// `Value` to `7` `Table`), then its payload fields in declaration order.
pub fn write_attr_key<S: CanonicalSink + ?Sized>(sink: &mut S, key: &AttrKey) {
    match key {
        AttrKey::Value { node } => {
            sink.tag(0);
            sink.u32(*node);
        }
        AttrKey::Field { node, field } => {
            sink.tag(1);
            sink.u32(*node);
            sink.str(field);
        }
        AttrKey::Meta { node } => {
            sink.tag(2);
            sink.u32(*node);
        }
        AttrKey::Anchor { node } => {
            sink.tag(3);
            sink.u32(*node);
        }
        AttrKey::EdgeLabel { edge } => {
            sink.tag(4);
            sink.u32(edge.0);
        }
        AttrKey::EdgePosition { edge } => {
            sink.tag(5);
            sink.u32(edge.0);
        }
        AttrKey::Cell { row, column } => {
            sink.tag(6);
            sink.u32(row.0);
            sink.str(column);
        }
        AttrKey::Table { row } => {
            sink.tag(7);
            sink.u32(row.0);
        }
    }
}

/// Write an attribution value: a one-byte tag in declaration order (`0`
/// `Presence` to `5` `Position`), then its payload.
///
/// A presence is one byte (`0` none, `1` present, `2` null, `3` absent)
/// followed, when present, by the value, as in a node record; a field is a
/// presence byte and then the value; metadata is the discriminator, the
/// position, the shape byte (with an XML element's tag) and the annotation
/// map, as in a node record; an anchor is a name; a label is a schema edge;
/// a position is an optional `u32`.
pub fn write_attr_val<S: CanonicalSink + ?Sized>(sink: &mut S, value: &AttrVal) {
    match value {
        AttrVal::Presence(presence) => {
            sink.tag(0);
            match presence {
                None => sink.tag(0),
                Some(FieldPresence::Present(v)) => {
                    sink.tag(1);
                    sink.value(v);
                }
                Some(FieldPresence::Null) => sink.tag(2),
                Some(FieldPresence::Absent) => sink.tag(3),
            }
        }
        AttrVal::Field(field) => {
            sink.tag(1);
            match field {
                None => sink.bool(false),
                Some(v) => {
                    sink.bool(true);
                    sink.value(v);
                }
            }
        }
        AttrVal::Meta(meta) => {
            sink.tag(2);
            write_meta(sink, meta);
        }
        AttrVal::Anchor(name) => {
            sink.tag(3);
            sink.name(name);
        }
        AttrVal::Label(edge) => {
            sink.tag(4);
            sink.edge(edge);
        }
        AttrVal::Position(position) => {
            sink.tag(5);
            opt_u32(sink, *position);
        }
    }
}

/// Write node metadata in the field order of a node record.
fn write_meta<S: CanonicalSink + ?Sized>(sink: &mut S, meta: &NodeMeta) {
    sink.opt_name(meta.discriminator.as_ref());
    opt_u32(sink, meta.position);
    match &meta.shape {
        NodeShape::Plain => sink.tag(0),
        NodeShape::List => sink.tag(1),
        NodeShape::XmlElement { tag } => {
            sink.tag(2);
            sink.name(tag);
        }
        NodeShape::XmlTextSegment => sink.tag(3),
        NodeShape::HyperEdge => sink.tag(4),
    }
    write_btree_value_map(sink, &meta.annotations);
}

/// Write a string-keyed value map: its entry count, then each key and value
/// in ascending key order.
fn write_btree_value_map<S: CanonicalSink + ?Sized>(sink: &mut S, map: &BTreeMap<String, Value>) {
    sink.len(map.len());
    for (key, value) in map {
        sink.str(key);
        sink.value(value);
    }
}

/// Write an attribution change: its `old` value, then its `new` value.
pub fn write_attr_change<S: CanonicalSink + ?Sized>(sink: &mut S, change: &AttrChange) {
    write_attr_val(sink, &change.old);
    write_attr_val(sink, &change.new);
}

/// Write a graph delta's canonical bytes into `sink`; see
/// [`canonical_graph_delta_bytes`] for the layout.
pub fn write_graph_delta<S: CanonicalSink + ?Sized>(sink: &mut S, delta: &GraphDelta) {
    sink.len(delta.del_nodes().len());
    sink.len(delta.del_edges().len());
    sink.len(delta.add_nodes().len());
    sink.len(delta.add_edges().len());
    sink.len(delta.attrs().len());
    for node in delta.del_nodes().values() {
        write_node_record(sink, node);
    }
    for (id, edge) in delta.del_edges() {
        write_edge_record(sink, *id, edge);
    }
    for node in delta.add_nodes().values() {
        write_node_record(sink, node);
    }
    for (id, edge) in delta.add_edges() {
        write_edge_record(sink, *id, edge);
    }
    for (key, change) in delta.attrs() {
        write_attr_key(sink, key);
        write_attr_change(sink, change);
    }
}

/// The canonical bytes of a graph delta, which is always in normal form.
///
/// ```text
/// header    = del_nodes, del_edges, add_nodes, add_edges, attrs   (counts, u64 each)
/// del_nodes = node record                  (ascending id)
/// del_edges = edge record                  (ascending EdgeId)
/// add_nodes = node record                  (ascending id)
/// add_edges = edge record                  (ascending EdgeId)
/// attrs     = key, old value, new value    (ascending key)
/// ```
///
/// Node and edge records are those of the canonical bytes of a graph
/// instance ([`canonical_ginstance_bytes`](crate::canonical_ginstance_bytes)),
/// keys are written by [`write_attr_key`] and values by [`write_attr_val`],
/// every value through [`CanonicalSink::value`]. No record refers to a
/// position in the byte string, so the digest can be streamed.
///
/// ```
/// use panproto_inst::graph_edit::{GraphDelta, canonical_graph_delta_bytes};
///
/// // Five zero counts.
/// assert_eq!(canonical_graph_delta_bytes(&GraphDelta::identity()), vec![0u8; 40]);
/// ```
#[must_use]
pub fn canonical_graph_delta_bytes(delta: &GraphDelta) -> Vec<u8> {
    let mut writer = CanonicalWriter::new();
    write_graph_delta(&mut writer, delta);
    writer.into_bytes()
}

impl GraphDelta {
    /// The digest of this delta: its canonical bytes
    /// ([`canonical_graph_delta_bytes`]) under [`GRAPH_DELTA_CONTEXT`],
    /// computed by streaming the records into the hasher.
    ///
    /// ```
    /// use panproto_inst::graph_edit::{GraphDelta, canonical_graph_delta_bytes};
    /// use panproto_inst::identity::{Digest, GRAPH_DELTA_CONTEXT};
    ///
    /// let delta = GraphDelta::identity();
    /// assert_eq!(
    ///     delta.digest(),
    ///     Digest::derive(GRAPH_DELTA_CONTEXT, &canonical_graph_delta_bytes(&delta)),
    /// );
    /// ```
    #[must_use]
    pub fn digest(&self) -> Digest {
        let mut hasher = Digest::hasher(GRAPH_DELTA_CONTEXT);
        write_graph_delta(&mut hasher, self);
        hasher.finalize()
    }
}

/// Whether two nodes have the same canonical record.
pub(super) fn same_node(a: &Node, b: &Node) -> bool {
    let mut x = CanonicalWriter::new();
    write_node_record(&mut x, a);
    let mut y = CanonicalWriter::new();
    write_node_record(&mut y, b);
    x.as_bytes() == y.as_bytes()
}

/// Whether two edges have the same endpoints, label and position.
pub(super) fn same_edge(a: &GEdge, b: &GEdge) -> bool {
    a == b
}

/// Whether two attribution values have the same canonical bytes.
pub(super) fn same_val(a: &AttrVal, b: &AttrVal) -> bool {
    let mut x = CanonicalWriter::new();
    write_attr_val(&mut x, a);
    let mut y = CanonicalWriter::new();
    write_attr_val(&mut y, b);
    x.as_bytes() == y.as_bytes()
}
