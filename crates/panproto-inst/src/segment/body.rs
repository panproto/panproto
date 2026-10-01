//! Segment bodies and their boundaries.
//!
//! A segment holds the nodes it owns, the edges it owns (those whose source
//! it owns), and a [`Boundary`]: a [`Ghost`] for every remote target of an
//! owned edge and an [`InStub`] for every edge owned elsewhere whose target
//! is owned here. Every map is a `BTreeMap`, so a segment enumerates its
//! contents in one order in every process.
//!
//! A tree segment carries its fans in their graph form: the fan node is held
//! in `nodes` with [`NodeShape::HyperEdge`](crate::NodeShape::HyperEdge) and
//! its hyper-edge as anchor, its parent edge and labelled child edges in
//! `arcs` under the ids its [`FanEmbedding`] records, so a fan whose children
//! are owned elsewhere crosses the boundary like any other edge.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_schema::{Edge, InstanceShapeKind};
use serde::{Deserialize, Serialize};

use super::ids::SegmentId;
use crate::functor::Row;
use crate::ginstance::{EdgeId, FanEmbedding, GEdge};
use crate::metadata::{Node, NodeShape};
use crate::view_ref::{RowId, ViewRef};

/// A remote target of an owned edge: the node, its owner and its anchor (a
/// table name for a row).
///
/// A segment holds one ghost per remote node, whatever the number of owned
/// edges into it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ghost {
    /// The remote node id.
    pub node: u32,
    /// The segment owning it.
    pub owner: SegmentId,
    /// Its anchor, as its owner holds it.
    pub anchor: Name,
}

/// An edge owned elsewhere whose target is owned here: its source, the
/// source's owner and anchor, its target and its label.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InStub {
    /// The source node id.
    pub src: u32,
    /// The segment owning the source, and so the edge.
    pub src_owner: SegmentId,
    /// The source's anchor (a table name for a row).
    pub src_anchor: Name,
    /// The target node id, owned here.
    pub tgt: u32,
    /// The schema edge the edge instantiates.
    pub label: Edge,
}

/// The boundary of a segment: its ghosts by node id, and its in-stubs by
/// the [`ViewRef`] of their edge.
///
/// An in-stub is keyed by `Edge(EdgeId)` for a graph edge and a tree's fan
/// edge, `Arc { parent, child }` for a tree arc, and `Pair { edge, src, tgt }`
/// for a table pair.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Boundary {
    /// One ghost per remote target of an owned edge.
    pub ghosts: BTreeMap<u32, Ghost>,
    /// One in-stub per edge owned elsewhere whose target is owned here.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub in_stubs: BTreeMap<ViewRef, InStub>,
}

/// One segment of a graph instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphSegment {
    /// The segment's id.
    pub segment: SegmentId,
    /// The nodes it owns.
    pub nodes: BTreeMap<u32, Node>,
    /// The edges it owns: those whose source it owns.
    pub edges: BTreeMap<EdgeId, GEdge>,
    /// Its boundary.
    pub boundary: Boundary,
}

/// One segment of a table instance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableSegment {
    /// The segment's id.
    pub segment: SegmentId,
    /// The rows it owns, each with its table.
    pub rows: BTreeMap<RowId, (Name, Row)>,
    /// The foreign-key pairs it owns: those whose referencing row it owns.
    pub pairs: BTreeSet<(Edge, RowId, RowId)>,
    /// Its boundary.
    pub boundary: Boundary,
}

/// One segment of a tree instance.
///
/// `arcs` holds the owned tree arcs and fan edges under their embedding
/// edge ids; ascending edge id is arc order, so merging the segments' arcs by
/// edge id gives the whole tree's arc order. `fans` holds the embedding of
/// each owned fan, ascending by fan node id. `root` is `Some` in exactly the
/// segment owning the tree's root. A tree segment is a forest: each owned
/// node whose parent arc is an in-stub roots a local subtree.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TreeSegment {
    /// The segment's id.
    pub segment: SegmentId,
    /// The nodes it owns, fan nodes included.
    pub nodes: BTreeMap<u32, Node>,
    /// The arcs and fan edges it owns, as `(source, target, label)`.
    pub arcs: BTreeMap<EdgeId, (u32, u32, Edge)>,
    /// The embedding of each fan it owns.
    pub fans: Vec<FanEmbedding>,
    /// The tree's root, when this segment owns it.
    pub root: Option<u32>,
    /// Its boundary.
    pub boundary: Boundary,
}

/// A segment of any shape.
///
/// Serialized externally tagged in `snake_case`: `{"tree": ...}`,
/// `{"table": ...}` or `{"graph": ...}`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnySegment {
    /// A tree segment.
    Tree(TreeSegment),
    /// A table segment.
    Table(TableSegment),
    /// A graph segment.
    Graph(GraphSegment),
}

/// The counts of one segment, as its manifest record states them.
///
/// For trees, `nodes` counts tree nodes (not fan nodes), `edges` tree arcs
/// only and `fans` the owned fans; `fans` is 0 for graphs and tables, and a
/// table's `nodes` and `edges` count rows and pairs. `payload_bytes` is the
/// length of the canonical encoding of every value the segment holds: node
/// values, extra fields and annotations, and row cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SegmentStats {
    /// The segment.
    pub segment: SegmentId,
    /// The owned nodes (or rows).
    pub nodes: u64,
    /// The owned edges (tree arcs, or pairs).
    pub edges: u64,
    /// The owned fans.
    pub fans: u64,
    /// The ghosts.
    pub ghosts: u64,
    /// The in-stubs.
    pub in_stubs: u64,
    /// The canonical bytes of the held values.
    pub payload_bytes: u64,
}

/// An owned edge of a segment, in shape-independent form.
#[derive(Clone, Debug)]
pub struct OwnedEdge<'a> {
    /// The edge's reference.
    pub elem: ViewRef,
    /// Its edge id, when it has one (not for table pairs).
    pub id: Option<EdgeId>,
    /// Its source.
    pub src: u32,
    /// Its target.
    pub tgt: u32,
    /// Its label.
    pub label: &'a Edge,
}

impl TreeSegment {
    /// The ids of every fan edge the segment owns.
    #[must_use]
    pub fn fan_edge_ids(&self) -> BTreeSet<EdgeId> {
        self.fans
            .iter()
            .flat_map(|f| std::iter::once(f.parent_edge).chain(f.child_edges.values().copied()))
            .collect()
    }

    /// The ids of every fan node the segment owns.
    #[must_use]
    pub fn fan_node_ids(&self) -> BTreeSet<u32> {
        self.fans.iter().map(|f| f.node).collect()
    }
}

impl AnySegment {
    /// The segment's id.
    #[must_use]
    pub const fn segment(&self) -> SegmentId {
        match self {
            Self::Tree(s) => s.segment,
            Self::Table(s) => s.segment,
            Self::Graph(s) => s.segment,
        }
    }

    /// The segment's shape.
    #[must_use]
    pub const fn shape(&self) -> InstanceShapeKind {
        match self {
            Self::Tree(_) => InstanceShapeKind::Tree,
            Self::Table(_) => InstanceShapeKind::Table,
            Self::Graph(_) => InstanceShapeKind::Graph,
        }
    }

    /// The segment's boundary.
    #[must_use]
    pub const fn boundary(&self) -> &Boundary {
        match self {
            Self::Tree(s) => &s.boundary,
            Self::Table(s) => &s.boundary,
            Self::Graph(s) => &s.boundary,
        }
    }

    /// The reference of an owned node of this shape.
    pub const fn node_ref(&self, id: u32) -> ViewRef {
        match self {
            Self::Table(_) => ViewRef::Row(RowId(id)),
            Self::Tree(_) | Self::Graph(_) => ViewRef::Node(id),
        }
    }

    /// Every owned node (row) with its anchor (table), ascending by id.
    pub fn node_anchors(&self) -> Vec<(u32, &Name)> {
        match self {
            Self::Tree(s) => s.nodes.iter().map(|(id, n)| (*id, &n.anchor)).collect(),
            Self::Graph(s) => s.nodes.iter().map(|(id, n)| (*id, &n.anchor)).collect(),
            Self::Table(s) => s.rows.iter().map(|(id, (t, _))| (id.0, t)).collect(),
        }
    }

    /// The anchor of owned node `id`, if the segment holds it.
    pub fn anchor_of(&self, id: u32) -> Option<&Name> {
        match self {
            Self::Tree(s) => s.nodes.get(&id).map(|n| &n.anchor),
            Self::Graph(s) => s.nodes.get(&id).map(|n| &n.anchor),
            Self::Table(s) => s.rows.get(&RowId(id)).map(|(t, _)| t),
        }
    }

    /// Every owned edge, in the segment's own order.
    pub fn owned_edges(&self) -> Vec<OwnedEdge<'_>> {
        match self {
            Self::Graph(s) => s
                .edges
                .iter()
                .map(|(id, e)| OwnedEdge {
                    elem: ViewRef::Edge(*id),
                    id: Some(*id),
                    src: e.src,
                    tgt: e.tgt,
                    label: &e.edge,
                })
                .collect(),
            Self::Tree(s) => {
                let fan_edges = s.fan_edge_ids();
                s.arcs
                    .iter()
                    .map(|(id, (src, tgt, label))| OwnedEdge {
                        elem: if fan_edges.contains(id) {
                            ViewRef::Edge(*id)
                        } else {
                            ViewRef::Arc {
                                parent: *src,
                                child: *tgt,
                            }
                        },
                        id: Some(*id),
                        src: *src,
                        tgt: *tgt,
                        label,
                    })
                    .collect()
            }
            Self::Table(s) => s
                .pairs
                .iter()
                .map(|(edge, src, tgt)| OwnedEdge {
                    elem: ViewRef::Pair {
                        edge: edge.clone(),
                        src: *src,
                        tgt: *tgt,
                    },
                    id: None,
                    src: src.0,
                    tgt: tgt.0,
                    label: edge,
                })
                .collect(),
        }
    }

    /// The segment's counts.
    #[must_use]
    pub fn stats(&self) -> SegmentStats {
        let boundary = self.boundary();
        let count = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
        let payload = super::bytes::payload_bytes(self);
        let (nodes, edges, fans) = match self {
            Self::Graph(s) => (count(s.nodes.len()), count(s.edges.len()), 0),
            Self::Table(s) => (count(s.rows.len()), count(s.pairs.len()), 0),
            Self::Tree(s) => {
                let fan_nodes = s.nodes.values().filter(|n| is_fan_node(n)).count();
                (
                    count(s.nodes.len() - fan_nodes.min(s.nodes.len())),
                    count(s.arcs.len().saturating_sub(s.fan_edge_ids().len())),
                    count(s.fans.len()),
                )
            }
        };
        SegmentStats {
            segment: self.segment(),
            nodes,
            edges,
            fans,
            ghosts: count(boundary.ghosts.len()),
            in_stubs: count(boundary.in_stubs.len()),
            payload_bytes: payload,
        }
    }

    /// The largest owned node id (row id), or `None` when it owns none.
    #[must_use]
    pub fn max_node(&self) -> Option<u32> {
        match self {
            Self::Tree(s) => s.nodes.keys().next_back().copied(),
            Self::Graph(s) => s.nodes.keys().next_back().copied(),
            Self::Table(s) => s.rows.keys().next_back().map(|r| r.0),
        }
    }

    /// The largest owned edge id, or `None` when it owns no edge with an id
    /// (always `None` for tables).
    #[must_use]
    pub fn max_edge(&self) -> Option<u32> {
        match self {
            Self::Tree(s) => s.arcs.keys().next_back().map(|e| e.0),
            Self::Graph(s) => s.edges.keys().next_back().map(|e| e.0),
            Self::Table(_) => None,
        }
    }

    /// The owned edge ids, ascending: the sorted run a manifest's edge id
    /// uniqueness check merges.
    #[must_use]
    pub fn edge_id_run(&self) -> Vec<EdgeId> {
        match self {
            Self::Tree(s) => s.arcs.keys().copied().collect(),
            Self::Graph(s) => s.edges.keys().copied().collect(),
            Self::Table(_) => Vec::new(),
        }
    }
}

/// Whether a node is a reified fan node.
pub const fn is_fan_node(node: &Node) -> bool {
    matches!(node.shape, NodeShape::HyperEdge)
}
