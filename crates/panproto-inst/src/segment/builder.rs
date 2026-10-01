//! Streaming construction of one segment.
//!
//! A caller whose instance does not fit in memory writes each segment
//! element by element: its owned nodes (or rows), its owned edges (those
//! whose source it owns), and an in-stub for each edge owned elsewhere whose
//! target it owns. [`SegmentBuilder::finish`] derives the ghosts from the
//! owned edges, asking the caller for each remote target's anchor, and
//! checks the segment's contract.

use std::collections::BTreeMap;

use panproto_gat::Name;
use panproto_schema::{Edge, InstanceShapeKind};

use super::body::{AnySegment, Boundary, Ghost, GraphSegment, InStub, TableSegment, TreeSegment};
use super::check::check_segment_with;
use super::error::{PartitionDefect, SegmentError};
use super::ids::SegmentId;
use super::partition::{OwnerIndex, Partition};
use crate::functor::Row;
use crate::ginstance::{EdgeId, FanEmbedding, GEdge};
use crate::metadata::Node;
use crate::view_ref::{RowId, ViewRef};

/// A node pushed into a [`SegmentBuilder`]: a graph or tree node, or a
/// table row with its table.
#[derive(Clone, Debug)]
pub enum BuilderNode {
    /// A graph node, or a tree node (a fan node included).
    Node(Node),
    /// A table row.
    Row {
        /// The row's id.
        id: RowId,
        /// Its table.
        table: Name,
        /// Its cells.
        row: Row,
    },
}

/// An owned edge pushed into a [`SegmentBuilder`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuilderEdge {
    /// A graph edge.
    Graph {
        /// Its edge id.
        id: EdgeId,
        /// The edge.
        edge: GEdge,
    },
    /// A tree arc or a fan edge, under its embedding edge id.
    Arc {
        /// Its embedding edge id.
        id: EdgeId,
        /// The source: the parent, or the fan node.
        src: u32,
        /// The target: the child, or the fan's parent or child.
        tgt: u32,
        /// The label.
        label: Edge,
    },
    /// A table foreign-key pair.
    Pair {
        /// The schema edge of the foreign key.
        edge: Edge,
        /// The referencing row.
        src: RowId,
        /// The referenced row.
        tgt: RowId,
    },
}

/// The contents of the segment being built, in the shape of its partition.
#[derive(Debug)]
enum Contents {
    Graph(GraphSegment),
    Table(TableSegment),
    Tree(TreeSegment),
}

/// Builds one segment element by element.
///
/// ```
/// use panproto_inst::segment::{BuilderEdge, BuilderNode, Partition, SegmentBuilder, SegmentId};
/// use panproto_inst::{EdgeId, GEdge, InstanceShapeKind, Node};
/// use panproto_schema::Edge;
///
/// let partition = Partition::by_node_ranges(InstanceShapeKind::Graph, &[10], 0, 20)?;
/// let label = Edge { src: "a".into(), tgt: "b".into(), kind: "prop".into(), name: None };
/// let mut builder = SegmentBuilder::new(&partition, SegmentId(0))?;
/// builder.push_node(BuilderNode::Node(Node::new(1, "a")))?;
/// builder.push_edge(BuilderEdge::Graph { id: EdgeId(0), edge: GEdge::new(1, 12, label) })?;
/// // Node 12 is owned by segment 1; its anchor becomes the ghost's.
/// let segment = builder.finish(&|node| (node == 12).then(|| "b".into()))?;
/// assert_eq!(segment.boundary().ghosts[&12].anchor.as_ref(), "b");
/// # Ok::<(), panproto_inst::segment::SegmentError>(())
/// ```
#[derive(Debug)]
pub struct SegmentBuilder {
    index: OwnerIndex,
    contents: Contents,
}

impl SegmentBuilder {
    /// A builder for segment `segment` of `partition`.
    ///
    /// # Errors
    ///
    /// The partition's own error when it is invalid, and
    /// [`SegmentError::MalformedPartition`] when it lists no such segment.
    pub fn new(partition: &Partition, segment: SegmentId) -> Result<Self, SegmentError> {
        let index = partition.owner_index()?;
        if partition.spec(segment).is_none() {
            return Err(SegmentError::MalformedPartition {
                segment,
                defect: PartitionDefect::UnknownSegment,
            });
        }
        let contents = match partition.shape {
            InstanceShapeKind::Graph => Contents::Graph(GraphSegment {
                segment,
                nodes: BTreeMap::new(),
                edges: BTreeMap::new(),
                boundary: Boundary::default(),
            }),
            InstanceShapeKind::Table => Contents::Table(TableSegment {
                segment,
                rows: BTreeMap::new(),
                pairs: std::collections::BTreeSet::new(),
                boundary: Boundary::default(),
            }),
            InstanceShapeKind::Tree => Contents::Tree(TreeSegment {
                segment,
                nodes: BTreeMap::new(),
                arcs: BTreeMap::new(),
                fans: Vec::new(),
                root: None,
                boundary: Boundary::default(),
            }),
        };
        Ok(Self { index, contents })
    }

    /// The segment being built.
    const fn segment(&self) -> SegmentId {
        match &self.contents {
            Contents::Graph(s) => s.segment,
            Contents::Table(s) => s.segment,
            Contents::Tree(s) => s.segment,
        }
    }

    /// The shape of the segment being built.
    const fn shape(&self) -> InstanceShapeKind {
        match &self.contents {
            Contents::Graph(_) => InstanceShapeKind::Graph,
            Contents::Table(_) => InstanceShapeKind::Table,
            Contents::Tree(_) => InstanceShapeKind::Tree,
        }
    }

    /// The error for an element of `found`'s shape pushed into this
    /// builder.
    const fn wrong_shape(&self, found: InstanceShapeKind) -> SegmentError {
        SegmentError::ShapeMismatch {
            expected: self.shape(),
            found,
        }
    }

    /// Check that node `id`, carried by `elem`, is owned by this segment.
    fn check_owned(&self, id: u32, elem: ViewRef) -> Result<(), SegmentError> {
        let me = self.segment();
        match self.index.owner_of_node(id) {
            None => Err(SegmentError::Unowned { elem }),
            Some(owner) if owner != me => Err(SegmentError::WrongOwner {
                elem,
                expected: owner,
                found: me,
            }),
            Some(_) => Ok(()),
        }
    }

    /// Add an owned node (or row). A node pushed again under the same id
    /// replaces the earlier one, as a map insert does.
    ///
    /// # Errors
    ///
    /// [`SegmentError::ShapeMismatch`] for a node of another shape, and
    /// [`SegmentError::Unowned`] or [`SegmentError::WrongOwner`] when the
    /// segment does not own its id.
    pub fn push_node(&mut self, node: BuilderNode) -> Result<(), SegmentError> {
        let (id, elem) = match &node {
            BuilderNode::Node(n) => (n.id, ViewRef::Node(n.id)),
            BuilderNode::Row { id, .. } => (id.0, ViewRef::Row(*id)),
        };
        match (&self.contents, &node) {
            (Contents::Graph(_) | Contents::Tree(_), BuilderNode::Node(_))
            | (Contents::Table(_), BuilderNode::Row { .. }) => {}
            (_, BuilderNode::Node(_)) => return Err(self.wrong_shape(InstanceShapeKind::Graph)),
            (_, BuilderNode::Row { .. }) => return Err(self.wrong_shape(InstanceShapeKind::Table)),
        }
        self.check_owned(id, elem)?;
        match (&mut self.contents, node) {
            (Contents::Graph(s), BuilderNode::Node(n)) => {
                s.nodes.insert(id, n);
            }
            (Contents::Tree(s), BuilderNode::Node(n)) => {
                s.nodes.insert(id, n);
            }
            (Contents::Table(s), BuilderNode::Row { id, table, row }) => {
                s.rows.insert(id, (table, row));
            }
            _ => {}
        }
        Ok(())
    }

    /// Add an owned edge: one whose source this segment owns. An edge
    /// pushed again under the same edge id (or the same pair) replaces the
    /// earlier one, as a map insert does.
    ///
    /// # Errors
    ///
    /// [`SegmentError::ShapeMismatch`] for an edge of another shape, and
    /// [`SegmentError::Unowned`] or [`SegmentError::WrongOwner`] when the
    /// segment does not own its source.
    pub fn push_edge(&mut self, edge: BuilderEdge) -> Result<(), SegmentError> {
        let (src, elem) = match &edge {
            BuilderEdge::Graph { id, edge } => (edge.src, ViewRef::Edge(*id)),
            BuilderEdge::Arc { id, src, .. } => (*src, ViewRef::Edge(*id)),
            BuilderEdge::Pair { edge, src, tgt } => (
                src.0,
                ViewRef::Pair {
                    edge: edge.clone(),
                    src: *src,
                    tgt: *tgt,
                },
            ),
        };
        match (&self.contents, &edge) {
            (Contents::Graph(_), BuilderEdge::Graph { .. })
            | (Contents::Tree(_), BuilderEdge::Arc { .. })
            | (Contents::Table(_), BuilderEdge::Pair { .. }) => {}
            (_, BuilderEdge::Graph { .. }) => return Err(self.wrong_shape(InstanceShapeKind::Graph)),
            (_, BuilderEdge::Arc { .. }) => return Err(self.wrong_shape(InstanceShapeKind::Tree)),
            (_, BuilderEdge::Pair { .. }) => return Err(self.wrong_shape(InstanceShapeKind::Table)),
        }
        self.check_owned(src, elem)?;
        match (&mut self.contents, edge) {
            (Contents::Graph(s), BuilderEdge::Graph { id, edge }) => {
                s.edges.insert(id, edge);
            }
            (Contents::Tree(s), BuilderEdge::Arc { id, src, tgt, label }) => {
                s.arcs.insert(id, (src, tgt, label));
            }
            (Contents::Table(s), BuilderEdge::Pair { edge, src, tgt }) => {
                s.pairs.insert((edge, src, tgt));
            }
            _ => {}
        }
        Ok(())
    }

    /// Add the embedding of an owned fan of a tree segment, replacing an
    /// earlier one of the same fan node. Its fan node and fan edges are
    /// pushed with [`SegmentBuilder::push_node`] and
    /// [`SegmentBuilder::push_edge`].
    ///
    /// # Errors
    ///
    /// [`SegmentError::ShapeMismatch`] unless the segment is a tree segment,
    /// and [`SegmentError::Unowned`] or [`SegmentError::WrongOwner`] when the
    /// segment does not own the fan node.
    pub fn push_fan(&mut self, fan: FanEmbedding) -> Result<(), SegmentError> {
        if !matches!(self.contents, Contents::Tree(_)) {
            return Err(self.wrong_shape(InstanceShapeKind::Tree));
        }
        self.check_owned(fan.node, ViewRef::Node(fan.node))?;
        if let Contents::Tree(s) = &mut self.contents {
            let at = s.fans.partition_point(|f| f.node < fan.node);
            if s.fans.get(at).is_some_and(|f| f.node == fan.node) {
                s.fans[at] = fan;
            } else {
                s.fans.insert(at, fan);
            }
        }
        Ok(())
    }

    /// Record that this tree segment owns the tree's root.
    ///
    /// # Errors
    ///
    /// [`SegmentError::ShapeMismatch`] unless the segment is a tree segment,
    /// and [`SegmentError::Unowned`] or [`SegmentError::WrongOwner`] when the
    /// segment does not own the root.
    pub fn push_root(&mut self, root: u32) -> Result<(), SegmentError> {
        if !matches!(self.contents, Contents::Tree(_)) {
            return Err(self.wrong_shape(InstanceShapeKind::Tree));
        }
        self.check_owned(root, ViewRef::Node(root))?;
        if let Contents::Tree(s) = &mut self.contents {
            s.root = Some(root);
        }
        Ok(())
    }

    /// Add the in-stub of an edge owned elsewhere whose target this segment
    /// owns, keyed by the edge's reference; a stub pushed again under the
    /// same key replaces the earlier one.
    ///
    /// # Errors
    ///
    /// [`SegmentError::InStubMismatch`] when the segment does not own the
    /// target, the source is owned here or by no segment, or the stub names
    /// a source owner other than the source's.
    pub fn push_in_stub(&mut self, elem: ViewRef, stub: InStub) -> Result<(), SegmentError> {
        let me = self.segment();
        let source_owner = self.index.owner_of_node(stub.src);
        if self.index.owner_of_node(stub.tgt) != Some(me)
            || source_owner != Some(stub.src_owner)
            || stub.src_owner == me
        {
            return Err(SegmentError::InStubMismatch { elem });
        }
        let boundary = match &mut self.contents {
            Contents::Graph(s) => &mut s.boundary,
            Contents::Table(s) => &mut s.boundary,
            Contents::Tree(s) => &mut s.boundary,
        };
        boundary.in_stubs.insert(elem, stub);
        Ok(())
    }

    /// Finish the segment: add a ghost for every remote target of an owned
    /// edge, anchored where `ghost_anchors` says its owner holds it, and
    /// check the segment's contract.
    ///
    /// # Errors
    ///
    /// [`SegmentError::GhostMismatch`] for the least remote target whose
    /// anchor `ghost_anchors` does not know, and [`SegmentError::Invalid`]
    /// with every violation when the segment breaks its contract (an owned
    /// edge whose target no segment owns, a fan whose edges were not
    /// pushed, an endpoint owned here but never pushed).
    pub fn finish(self, ghost_anchors: &dyn Fn(u32) -> Option<Name>) -> Result<AnySegment, SegmentError> {
        let me = self.segment();
        let mut segment = match self.contents {
            Contents::Graph(s) => AnySegment::Graph(s),
            Contents::Table(s) => AnySegment::Table(s),
            Contents::Tree(s) => AnySegment::Tree(s),
        };
        let mut ghosts: BTreeMap<u32, Ghost> = BTreeMap::new();
        for edge in segment.owned_edges() {
            let Some(owner) = self.index.owner_of_node(edge.tgt).filter(|o| *o != me) else {
                continue;
            };
            if ghosts.contains_key(&edge.tgt) {
                continue;
            }
            let anchor = ghost_anchors(edge.tgt).ok_or(SegmentError::GhostMismatch { node: edge.tgt })?;
            ghosts.insert(
                edge.tgt,
                Ghost {
                    node: edge.tgt,
                    owner,
                    anchor,
                },
            );
        }
        match &mut segment {
            AnySegment::Graph(s) => s.boundary.ghosts = ghosts,
            AnySegment::Table(s) => s.boundary.ghosts = ghosts,
            AnySegment::Tree(s) => s.boundary.ghosts = ghosts,
        }
        check_segment_with(&segment, &self.index)?;
        Ok(segment)
    }
}
