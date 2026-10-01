//! Canonical bytes and digest of a tree instance.
//!
//! A tree's digest is a function of the tree alone. No edge id of a graph
//! embedding is written, so two equal trees have one digest whatever their
//! embeddings, and fans are written in their canonical record order rather
//! than in the tree's fan order, so the digest of a tree held as segments
//! can be computed by merging each segment's fans by that order.

use panproto_schema::{CanonicalWriter, Edge};

use crate::fan::Fan;
use crate::ginstance::write_node_record;
use crate::identity::{CanonicalSink, Digest, TREE_INSTANCE_CONTEXT};
use crate::wtype::WInstance;

/// A fan in its canonical record form: its parent, its hyper-edge, and its
/// children as `(label, node)` in label order. Fan records order by these
/// fields in turn.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FanRecord {
    /// The parent node.
    pub parent: u32,
    /// The schema hyper-edge.
    pub hyper_edge_id: String,
    /// The labelled children, in label order.
    pub children: Vec<(String, u32)>,
}

impl FanRecord {
    /// The record of a fan.
    #[must_use]
    pub fn of(fan: &Fan) -> Self {
        let mut children: Vec<(String, u32)> =
            fan.children.iter().map(|(l, n)| (l.clone(), *n)).collect();
        children.sort_unstable();
        Self {
            parent: fan.parent,
            hyper_edge_id: fan.hyper_edge_id.clone(),
            children,
        }
    }

    /// Write the record: parent, hyper-edge, then the child count and each
    /// `(label, node)`.
    pub fn write<S: CanonicalSink + ?Sized>(&self, sink: &mut S) {
        sink.u32(self.parent);
        sink.str(&self.hyper_edge_id);
        sink.len(self.children.len());
        for (label, node) in &self.children {
            sink.str(label);
            sink.u32(*node);
        }
    }
}

/// Write the record of one arc: parent, child, label. No edge id is
/// written.
pub fn write_arc_record<S: CanonicalSink + ?Sized>(sink: &mut S, parent: u32, child: u32, edge: &Edge) {
    sink.u32(parent);
    sink.u32(child);
    sink.edge(edge);
}

/// Write the header of a tree's canonical bytes: its node, arc and fan
/// counts.
pub fn write_winstance_header<S: CanonicalSink + ?Sized>(sink: &mut S, nodes: u64, arcs: u64, fans: u64) {
    sink.u64(nodes);
    sink.u64(arcs);
    sink.u64(fans);
}

/// Write a tree's canonical bytes into `sink`: the header, then every node
/// record in ascending id, every arc record in arc order, and every fan
/// record in ascending record order.
pub fn write_winstance<S: CanonicalSink + ?Sized>(sink: &mut S, tree: &WInstance) {
    let count = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
    write_winstance_header(
        sink,
        count(tree.nodes.len()),
        count(tree.arcs.len()),
        count(tree.fans.len()),
    );
    let mut ids: Vec<u32> = tree.nodes.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        write_node_record(sink, &tree.nodes[&id]);
    }
    for (parent, child, edge) in &tree.arcs {
        write_arc_record(sink, *parent, *child, edge);
    }
    let mut fans: Vec<FanRecord> = tree.fans.iter().map(FanRecord::of).collect();
    fans.sort_unstable();
    for fan in &fans {
        fan.write(sink);
    }
}

/// The canonical bytes of a tree instance, as [`write_winstance`] writes
/// them.
///
/// ```text
/// header = node_count: u64, arc_count: u64, fan_count: u64
/// node   = the graph node record                  (ascending id)
/// arc    = parent: u32, child: u32, label          (arc order)
/// fan    = parent: u32, hyper_edge_id, children: count,
///          (label, node: u32) in label order       (ascending record)
/// ```
///
/// A node record is the one [`write_node_record`] writes for a graph node;
/// a label is a schema edge (source, target, kind, optional name); strings
/// and counts are written as every canonical encoding writes them. The
/// arcs are in the tree's arc order, which is its child order. No edge id
/// appears, and the fans are written in the order of their records, not in
/// the tree's fan order: a tree's identity does not depend on how it is
/// embedded in a graph or on the order its fans are listed in. The root is
/// not written: it is the one node without a parent arc.
#[must_use]
pub fn canonical_winstance_bytes(tree: &WInstance) -> Vec<u8> {
    let mut writer = CanonicalWriter::new();
    write_winstance(&mut writer, tree);
    writer.into_bytes()
}

/// The digest of a tree instance: its canonical bytes
/// ([`canonical_winstance_bytes`]) under [`TREE_INSTANCE_CONTEXT`],
/// streamed into the hasher.
///
/// ```
/// use std::collections::HashMap;
///
/// use panproto_inst::identity::{Digest, TREE_INSTANCE_CONTEXT};
/// use panproto_inst::segment::{canonical_winstance_bytes, winstance_digest};
/// use panproto_inst::{Node, WInstance};
///
/// let tree = WInstance::new(HashMap::from([(0, Node::new(0, "root"))]), vec![], vec![], 0, "root".into());
/// assert_eq!(
///     winstance_digest(&tree),
///     Digest::derive(TREE_INSTANCE_CONTEXT, &canonical_winstance_bytes(&tree)),
/// );
/// ```
#[must_use]
pub fn winstance_digest(tree: &WInstance) -> Digest {
    let mut hasher = Digest::hasher(TREE_INSTANCE_CONTEXT);
    write_winstance(&mut hasher, tree);
    hasher.finalize()
}

impl WInstance {
    /// The digest of this tree: [`winstance_digest`].
    #[must_use]
    pub fn digest(&self) -> Digest {
        winstance_digest(self)
    }
}
