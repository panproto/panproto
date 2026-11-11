//! Canonical bytes and digest of a graph instance.
//!
//! [`canonical_ginstance_bytes`] documents the record layout;
//! [`write_node_record`] and [`write_edge_record`] write one record each, so
//! that the same encoding serves whole graphs and streams of records.

use panproto_schema::CanonicalWriter;

use super::{EdgeId, GEdge, GInstance};
use crate::identity::{CanonicalSink, Digest, GRAPH_INSTANCE_CONTEXT, write_value_map};
use crate::metadata::{Node, NodeShape};
use crate::value::FieldPresence;

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

/// Write the record of one node: its id, then each of its fields in
/// declaration order. See [`canonical_ginstance_bytes`] for the layout.
pub fn write_node_record<S: CanonicalSink + ?Sized>(sink: &mut S, node: &Node) {
    sink.u32(node.id);
    sink.name(&node.anchor);
    match &node.value {
        None => sink.tag(0),
        Some(FieldPresence::Present(value)) => {
            sink.tag(1);
            sink.value(value);
        }
        Some(FieldPresence::Null) => sink.tag(2),
        Some(FieldPresence::Absent) => sink.tag(3),
    }
    sink.opt_name(node.discriminator.as_ref());
    write_value_map(sink, &node.extra_fields);
    opt_u32(sink, node.position);
    match &node.shape {
        NodeShape::Plain => sink.tag(0),
        NodeShape::List => sink.tag(1),
        NodeShape::XmlElement { tag } => {
            sink.tag(2);
            sink.name(tag);
        }
        NodeShape::XmlTextSegment => sink.tag(3),
        NodeShape::HyperEdge => sink.tag(4),
    }
    write_value_map(sink, &node.annotations);
}

/// Write the record of one edge: its id, its endpoints, its label and its
/// position. See [`canonical_ginstance_bytes`] for the layout.
pub fn write_edge_record<S: CanonicalSink + ?Sized>(sink: &mut S, id: EdgeId, edge: &GEdge) {
    sink.u32(id.0);
    sink.u32(edge.src);
    sink.u32(edge.tgt);
    sink.edge(&edge.edge);
    opt_u32(sink, edge.position);
}

/// Write a graph's canonical bytes into `sink`: the header, then every node
/// record in ascending id, then every edge record in ascending [`EdgeId`].
pub fn write_ginstance<S: CanonicalSink + ?Sized>(sink: &mut S, graph: &GInstance) {
    sink.len(graph.node_count());
    sink.len(graph.edge_count());
    let mut ids: Vec<u32> = graph.nodes().keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        write_node_record(sink, &graph.nodes()[&id]);
    }
    for (id, edge) in graph.edges_sorted() {
        write_edge_record(sink, id, edge);
    }
}

/// The canonical bytes of a graph instance, as [`write_ginstance`] writes
/// them.
///
/// Two graphs have the same bytes exactly when they have the same nodes and
/// the same edges under the same ids, a `NaN` float written by its bit
/// pattern.
///
/// The bytes are a header and then self-contained records:
///
/// ```text
/// header   = node_count: u64, edge_count: u64
/// node     = id: u32, anchor, value, discriminator, extra_fields, position,
///            shape, annotations                      (ascending id)
/// edge     = id: u32, src: u32, tgt: u32, label, position (ascending EdgeId)
/// ```
///
/// Every field is written through a [`CanonicalSink`] with the sink's own
/// encoders: integers little-endian, names and strings length-prefixed,
/// values by [`CanonicalSink::value`], string-keyed value maps as their entry
/// count and then each entry in ascending key order. An optional field is a
/// presence byte and then the field. A node's value is one byte (`0` no
/// value, `1` present, `2` null, `3` absent) followed, when present, by the
/// value; its shape is one byte (`0` plain, `1` list, `2` XML element
/// followed by its tag, `3` XML text segment, `4` hyper-edge). A label is a
/// schema edge: source, target, kind and optional name.
///
/// No record refers to another record or to a position in the byte string,
/// so the digest can be computed by streaming the records into a
/// [`DigestHasher`](crate::DigestHasher), and a partitioned graph can merge
/// record streams that are each already in order. The positions written are
/// the ones the instance carries as data ([`Node::position`],
/// [`GEdge::position`]); no derived positional code (an interval, a path or
/// a rank) is ever written, since such codes are recomputed after every edit
/// (Tatarinov et al. 2002, stated: worst-case renumbering).
#[must_use]
pub fn canonical_ginstance_bytes(graph: &GInstance) -> Vec<u8> {
    let mut writer = CanonicalWriter::new();
    write_ginstance(&mut writer, graph);
    writer.into_bytes()
}

impl GInstance {
    /// The digest of this graph: its canonical bytes
    /// ([`canonical_ginstance_bytes`]) under [`GRAPH_INSTANCE_CONTEXT`],
    /// computed by streaming the records into the hasher.
    ///
    /// ```
    /// use panproto_inst::identity::{Digest, GRAPH_INSTANCE_CONTEXT};
    /// use panproto_inst::{GInstance, Node, canonical_ginstance_bytes};
    ///
    /// let graph = GInstance::new().with_node(Node::new(0, "item"));
    /// assert_eq!(
    ///     graph.digest(),
    ///     Digest::derive(GRAPH_INSTANCE_CONTEXT, &canonical_ginstance_bytes(&graph)),
    /// );
    /// ```
    #[must_use]
    pub fn digest(&self) -> Digest {
        let mut hasher = Digest::hasher(GRAPH_INSTANCE_CONTEXT);
        write_ginstance(&mut hasher, self);
        hasher.finalize()
    }
}
