//! Graph-shaped instance representation.
//!
//! A [`GInstance`] is the most general instance form: a directed multigraph
//! of nodes and identified edges with no distinguished root, cycles, self-loops
//! and parallel edges allowed. Both [`WInstance`](crate::WInstance) (trees) and
//! [`FInstance`](crate::FInstance) (tables) embed into it; see
//! [`GInstance::from_wtype_embedded`] and [`GInstance::from_finstance`], and
//! [`GInstance::tree_shape`] for the way back to a tree.
//!
//! This is the natural instance theory for knowledge graphs (RDF, OWL,
//! JSON-LD), property graphs (Neo4j), and dependency graphs.
//!
//! # Representation
//!
//! Nodes are keyed by their `u32` id and edges by an [`EdgeId`]. Each edge is a
//! [`GEdge`]: its endpoints, the schema edge it instantiates, and an optional
//! position. Two parallel edges differ only by their [`EdgeId`].
//!
//! A node's leaf value lives in [`Node::value`], exactly as in a tree.
//!
//! Order is carried by edges, not nodes. The ordered successors of a node are
//! its out-edges sorted by `(position, EdgeId)`, with an absent position first;
//! see [`GInstance::ordered_out_edges`]. [`Node::position`] is opaque node data:
//! it is carried and restored verbatim, and no graph operation reads it.
//!
//! The fields are private, so every mutation goes through a method that keeps
//! the adjacency indices consistent with the edge table, and every edge's
//! endpoints exist.

mod canonical;
mod embed;
mod embedding;
mod nodelink;
mod reify;
mod table_embed;
mod tree_shape;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

use crate::error::InstError;
use crate::metadata::Node;
use crate::segment::{IdAllocator, IdKind, IdRange};
use crate::value::{FieldPresence, Value};

pub use canonical::{
    canonical_ginstance_bytes, write_edge_record, write_ginstance, write_node_record,
};
pub use embedding::{FanEmbedding, TreeEmbedding};
pub use nodelink::{from_node_link_json, to_node_link_json};
pub use reify::{
    FanDomain, HYPER_LABEL_KIND, HYPER_PARENT_KIND, reify_fans, reify_fans_owned, unreify_fans,
    unreify_fans_owned, unreify_fans_with_schema,
};
pub use table_embed::{NotTableReason, TableEmbedding, TableShapeReport};
pub use tree_shape::{NotTreeReason, TreeShapeReport};

/// Identifier of an edge in a [`GInstance`].
///
/// Edge identity is what distinguishes parallel edges: two edges with the
/// same endpoints and the same schema edge are different edges exactly when
/// their ids differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EdgeId(pub u32);

impl fmt::Display for EdgeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An edge of a [`GInstance`]: its endpoints, the schema edge it
/// instantiates, and its position among its source's out-edges.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GEdge {
    /// Source node id.
    pub src: u32,
    /// Target node id.
    pub tgt: u32,
    /// The schema edge this edge instantiates.
    pub edge: Edge,
    /// Position among the source's out-edges, if the edge is ordered.
    ///
    /// [`GInstance::ordered_out_edges`] sorts by `(position, EdgeId)` with an
    /// absent position before every present one.
    pub position: Option<u32>,
}

impl GEdge {
    /// An unordered edge from `src` to `tgt` instantiating `edge`.
    #[must_use]
    pub const fn new(src: u32, tgt: u32, edge: Edge) -> Self {
        Self {
            src,
            tgt,
            edge,
            position: None,
        }
    }

    /// Set the edge's position.
    #[must_use]
    pub const fn with_position(mut self, position: Option<u32>) -> Self {
        self.position = position;
        self
    }
}

/// A graph-shaped instance: the most general instance form.
///
/// Unlike [`WInstance`](crate::WInstance), `GInstance` has no distinguished
/// root and cycles are allowed. Unlike [`FInstance`](crate::FInstance), it is
/// not bipartite.
///
/// # Encoding
///
/// A `GInstance` serializes as `{ nodes, edges }`, both maps written in
/// ascending key order, so one instance encodes to the same bytes in every
/// process. In a human-readable format (JSON among them), each node is
/// written in the form [`to_node_link_json`] gives it: a float that is `NaN`
/// or infinite, which JSON has no number for, is written as
/// `{"Float": "NaN"}`, `{"Float": "Infinity"}` or `{"Float": "-Infinity"}`
/// and read back as that float, so every instance round-trips through JSON.
/// A binary format writes such floats as floats. Decoding rebuilds the adjacency indices and fails with
/// [`InstError::DanglingEdge`] when an edge names a missing node, with
/// [`InstError::NodeIdMismatch`] when a node is filed under an id other than
/// its own, and with [`InstError::DuplicateNode`] or
/// [`InstError::DuplicateEdge`] when a map lists one key twice, rather than
/// keeping either entry.
///
/// # Equality
///
/// `==` compares the node and edge tables. [`GInstance::identical`] does the
/// same but treats two `NaN` floats as equal, and
/// [`GInstance::equivalent_modulo_edge_ids`] ignores which ids the edges carry.
#[derive(Clone, Debug)]
pub struct GInstance {
    nodes: HashMap<u32, Node>,
    edges: HashMap<EdgeId, GEdge>,
    /// Out-edges of each node, ascending.
    out_index: HashMap<u32, Vec<EdgeId>>,
    /// In-edges of each node, ascending.
    in_index: HashMap<u32, Vec<EdgeId>>,
    /// The edge id allocator. Its floor is one above the largest edge id in
    /// use, 0 for none.
    edge_ids: IdAllocator,
    /// The edge ids in use, ascending, so that removing the edge with the
    /// largest id finds the next largest without a scan.
    edge_order: BTreeSet<u32>,
}

/// Writes the struct `GInstanceWire { nodes, edges }`, both maps in ascending
/// key order; see the type's encoding documentation.
impl Serialize for GInstance {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct as _;

        debug_assert!(
            self.nodes.iter().all(|(key, node)| *key == node.id),
            "a node's id was changed through node_mut"
        );
        let readable = serializer.is_human_readable();
        let mut wire = serializer.serialize_struct("GInstanceWire", 2)?;
        if readable {
            let nodes: BTreeMap<u32, nodelink::MarkedNode<'_>> = self
                .nodes
                .iter()
                .map(|(id, node)| (*id, nodelink::MarkedNode::new(node)))
                .collect();
            wire.serialize_field("nodes", &nodes)?;
        } else {
            let nodes: BTreeMap<u32, &Node> = self.nodes.iter().map(|(id, n)| (*id, n)).collect();
            wire.serialize_field("nodes", &nodes)?;
        }
        let edges: BTreeMap<EdgeId, &GEdge> = self.edges.iter().map(|(id, e)| (*id, e)).collect();
        wire.serialize_field("edges", &edges)?;
        wire.end()
    }
}

/// Reads what [`Serialize`] writes; see the type's encoding documentation.
impl<'de> Deserialize<'de> for GInstance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;

        let wire = if deserializer.is_human_readable() {
            let readable = GInstanceReadableEntries::deserialize(deserializer)?;
            let nodes = readable
                .nodes
                .into_iter()
                .map(|(key, json)| nodelink::decode_node(&json).map(|node| (key, node)))
                .collect::<Result<Vec<_>, _>>()
                .map_err(D::Error::custom)?;
            GInstanceWireEntries {
                nodes,
                edges: readable.edges,
            }
        } else {
            GInstanceWireEntries::deserialize(deserializer)?
        };
        Self::try_from(wire).map_err(D::Error::custom)
    }
}

/// The encoded form of a [`GInstance`] as decoded: each map's entries in
/// document order, a key listed twice kept twice so that decoding can refuse
/// it.
#[derive(Deserialize)]
struct GInstanceWireEntries {
    #[serde(deserialize_with = "map_entries")]
    nodes: Vec<(u32, Node)>,
    #[serde(deserialize_with = "map_entries")]
    edges: Vec<(EdgeId, GEdge)>,
}

/// [`GInstanceWireEntries`] in a human-readable format, each node still in
/// the JSON form that may write a non-finite float as a string.
#[derive(Deserialize)]
struct GInstanceReadableEntries {
    #[serde(deserialize_with = "map_entries")]
    nodes: Vec<(u32, serde_json::Value)>,
    #[serde(deserialize_with = "map_entries")]
    edges: Vec<(EdgeId, GEdge)>,
}

/// Read a map as its entries in document order, duplicate keys included.
fn map_entries<'de, D, K, V>(deserializer: D) -> Result<Vec<(K, V)>, D::Error>
where
    D: serde::Deserializer<'de>,
    K: Deserialize<'de>,
    V: Deserialize<'de>,
{
    struct Entries<K, V>(std::marker::PhantomData<(K, V)>);

    impl<'de, K: Deserialize<'de>, V: Deserialize<'de>> serde::de::Visitor<'de> for Entries<K, V> {
        type Value = Vec<(K, V)>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a map")
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0).min(4096));
            while let Some(entry) = map.next_entry()? {
                entries.push(entry);
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_map(Entries(std::marker::PhantomData))
}

impl TryFrom<GInstanceWireEntries> for GInstance {
    type Error = InstError;

    fn try_from(wire: GInstanceWireEntries) -> Result<Self, InstError> {
        let mut entries = wire.nodes;
        entries.sort_unstable_by_key(|(key, _)| *key);
        let mut nodes = HashMap::with_capacity(entries.len());
        for (key, node) in entries {
            if node.id != key {
                return Err(InstError::NodeIdMismatch {
                    key,
                    node_id: node.id,
                });
            }
            if nodes.insert(key, node).is_some() {
                return Err(InstError::DuplicateNode(key));
            }
        }
        let mut graph = Self {
            nodes,
            edges: HashMap::with_capacity(wire.edges.len()),
            out_index: HashMap::new(),
            in_index: HashMap::new(),
            edge_ids: IdAllocator::new(IdRange::FULL),
            edge_order: BTreeSet::new(),
        };
        let mut edges = wire.edges;
        edges.sort_unstable_by_key(|(id, _)| *id);
        for (id, edge) in edges {
            graph.insert_edge(id, edge)?;
        }
        Ok(graph)
    }
}

impl PartialEq for GInstance {
    fn eq(&self, other: &Self) -> bool {
        self.edges == other.edges
            && self.nodes.len() == other.nodes.len()
            && self.nodes.iter().all(|(id, node)| {
                other
                    .nodes
                    .get(id)
                    .is_some_and(|theirs| nodes_equal(node, theirs, values_equal))
            })
    }
}

impl Default for GInstance {
    fn default() -> Self {
        Self::new()
    }
}

/// Insert `id` into a sorted list, keeping it sorted.
fn index_insert(index: &mut HashMap<u32, Vec<EdgeId>>, node: u32, id: EdgeId) {
    let list = index.entry(node).or_default();
    if let Err(at) = list.binary_search(&id) {
        list.insert(at, id);
    }
}

/// Remove `id` from a sorted list, dropping the list once it is empty.
fn index_remove(index: &mut HashMap<u32, Vec<EdgeId>>, node: u32, id: EdgeId) {
    if let Some(list) = index.get_mut(&node) {
        if let Ok(at) = list.binary_search(&id) {
            list.remove(at);
        }
        if list.is_empty() {
            index.remove(&node);
        }
    }
}

impl GInstance {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Create a new empty graph instance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            out_index: HashMap::new(),
            in_index: HashMap::new(),
            edge_ids: IdAllocator::new(IdRange::FULL),
            edge_order: BTreeSet::new(),
        }
    }

    /// Add a node, replacing any node already filed under its id.
    ///
    /// Replacing a node keeps its edges, which name it by id.
    #[must_use]
    pub fn with_node(mut self, node: Node) -> Self {
        self.nodes.insert(node.id, node);
        self
    }

    /// Add an edge from `src` to `tgt` under the next free [`EdgeId`], as
    /// [`Self::add_edge`] allocates it.
    ///
    /// # Panics
    ///
    /// Panics if either endpoint is not a node of the instance, or if every
    /// edge id is already in use.
    #[must_use]
    pub fn with_edge(mut self, src: u32, tgt: u32, edge: Edge) -> Self {
        if let Err(err) = self.add_edge(src, tgt, edge) {
            panic!("GInstance::with_edge({src}, {tgt}): {err}");
        }
        self
    }

    /// Add an edge from `src` to `tgt` under the given [`EdgeId`].
    ///
    /// # Panics
    ///
    /// Panics if the id is already in use or either endpoint is not a node of
    /// the instance.
    #[must_use]
    pub fn with_edge_id(mut self, id: EdgeId, src: u32, tgt: u32, edge: Edge) -> Self {
        if let Err(err) = self.insert_edge(id, GEdge::new(src, tgt, edge)) {
            panic!("GInstance::with_edge_id({id}, {src}, {tgt}): {err}");
        }
        self
    }

    /// Set a node's value to `Present(value)`.
    ///
    /// # Panics
    ///
    /// In debug builds, panics if no node has the id. Release builds leave
    /// the instance unchanged in that case.
    #[must_use]
    pub fn with_value(mut self, node_id: u32, value: Value) -> Self {
        let node = self.nodes.get_mut(&node_id);
        debug_assert!(node.is_some(), "with_value: no node {node_id}");
        if let Some(node) = node {
            node.value = Some(FieldPresence::Present(value));
        }
        self
    }

    // -----------------------------------------------------------------------
    // Reads
    // -----------------------------------------------------------------------

    /// All nodes, keyed by id.
    #[must_use]
    pub const fn nodes(&self) -> &HashMap<u32, Node> {
        &self.nodes
    }

    /// The node with the given id.
    #[must_use]
    pub fn node(&self, id: u32) -> Option<&Node> {
        let node = self.nodes.get(&id);
        debug_assert!(
            node.is_none_or(|n| n.id == id),
            "node {id} had its id changed through node_mut"
        );
        node
    }

    /// All edges, keyed by id.
    #[must_use]
    pub const fn edges(&self) -> &HashMap<EdgeId, GEdge> {
        &self.edges
    }

    /// All edges in ascending [`EdgeId`] order.
    pub fn edges_sorted(&self) -> impl Iterator<Item = (EdgeId, &GEdge)> + '_ {
        let mut ids: Vec<EdgeId> = self.edges.keys().copied().collect();
        ids.sort_unstable();
        ids.into_iter().map(|id| (id, &self.edges[&id]))
    }

    /// The edge with the given id.
    #[must_use]
    pub fn edge(&self, id: EdgeId) -> Option<&GEdge> {
        self.edges.get(&id)
    }

    /// The ids of the edges leaving `node`, ascending.
    #[must_use]
    pub fn out_edges(&self, node: u32) -> &[EdgeId] {
        self.out_index.get(&node).map_or(&[], Vec::as_slice)
    }

    /// The ids of the edges entering `node`, ascending.
    #[must_use]
    pub fn in_edges(&self, node: u32) -> &[EdgeId] {
        self.in_index.get(&node).map_or(&[], Vec::as_slice)
    }

    /// The targets of the edges leaving `node`, one per edge, in ascending
    /// [`EdgeId`] order.
    pub fn successors(&self, node: u32) -> impl Iterator<Item = u32> + '_ {
        self.out_edges(node).iter().map(|id| self.edges[id].tgt)
    }

    /// The sources of the edges entering `node`, one per edge, in ascending
    /// [`EdgeId`] order.
    pub fn predecessors(&self, node: u32) -> impl Iterator<Item = u32> + '_ {
        self.in_edges(node).iter().map(|id| self.edges[id].src)
    }

    /// The ids of the edges from `src` to `tgt`, ascending.
    #[must_use]
    pub fn edges_between(&self, src: u32, tgt: u32) -> Vec<EdgeId> {
        self.out_edges(src)
            .iter()
            .copied()
            .filter(|id| self.edges[id].tgt == tgt)
            .collect()
    }

    /// The ids of the edges leaving `node`, in order: sorted by
    /// `(position, EdgeId)`, with an absent position first.
    #[must_use]
    pub fn ordered_out_edges(&self, node: u32) -> Vec<EdgeId> {
        let mut ids = self.out_edges(node).to_vec();
        ids.sort_by_key(|id| (self.edges[id].position, *id));
        ids
    }

    /// Returns the number of nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of edges.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// The largest node id in use.
    #[must_use]
    pub fn max_node_id(&self) -> Option<u32> {
        self.nodes.keys().copied().max()
    }

    /// The largest edge id in use.
    #[must_use]
    pub fn max_edge_id(&self) -> Option<EdgeId> {
        self.edge_order.last().copied().map(EdgeId)
    }

    // -----------------------------------------------------------------------
    // Mutation
    // -----------------------------------------------------------------------

    /// Insert a node.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::DuplicateNode`] if a node already has the id.
    pub fn insert_node(&mut self, node: Node) -> Result<(), InstError> {
        if self.nodes.contains_key(&node.id) {
            return Err(InstError::DuplicateNode(node.id));
        }
        self.nodes.insert(node.id, node);
        Ok(())
    }

    /// Remove a node that no edge touches, returning it.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::NodeNotFound`] if there is no such node and
    /// [`InstError::IncidentEdges`] if edges still touch it.
    pub fn remove_node(&mut self, id: u32) -> Result<Node, InstError> {
        if !self.nodes.contains_key(&id) {
            return Err(InstError::NodeNotFound(id));
        }
        let mut incident: Vec<EdgeId> = self
            .out_edges(id)
            .iter()
            .chain(self.in_edges(id))
            .copied()
            .collect();
        if !incident.is_empty() {
            incident.sort_unstable();
            incident.dedup();
            return Err(InstError::IncidentEdges {
                node: id,
                edges: incident,
            });
        }
        self.nodes.remove(&id).ok_or(InstError::NodeNotFound(id))
    }

    /// Insert an edge under the given id.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::DuplicateEdge`] if the id is in use and
    /// [`InstError::DanglingEdge`] if an endpoint is not a node.
    pub fn insert_edge(&mut self, id: EdgeId, edge: GEdge) -> Result<(), InstError> {
        if self.edges.contains_key(&id) {
            return Err(InstError::DuplicateEdge(id));
        }
        for endpoint in [edge.src, edge.tgt] {
            if !self.nodes.contains_key(&endpoint) {
                return Err(InstError::DanglingEdge { edge: id, endpoint });
            }
        }
        index_insert(&mut self.out_index, edge.src, id);
        index_insert(&mut self.in_index, edge.tgt, id);
        self.edges.insert(id, edge);
        self.edge_order.insert(id.0);
        self.edge_ids.observe(id.0);
        Ok(())
    }

    /// Add an edge under a fresh id and return the id.
    ///
    /// The id is one above the largest edge id in use, and 0 when there is
    /// none, so it depends on the edges present alone: two instances that
    /// compare equal allocate the same id, whatever edges each once held.
    /// The id comes from an [`IdAllocator`] over the whole id space, so a
    /// loop of calls takes constant time per call beyond recording the id;
    /// removing the edge with the largest id brings the floor down to the
    /// next largest in logarithmic time, without a scan.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::DanglingEdge`] if an endpoint is not a node and
    /// [`InstError::EdgeIdsExhausted`] if the id `u32::MAX` has been used.
    pub fn add_edge(&mut self, src: u32, tgt: u32, edge: Edge) -> Result<EdgeId, InstError> {
        for endpoint in [src, tgt] {
            if !self.nodes.contains_key(&endpoint) {
                let id = EdgeId(self.edge_ids.floor().unwrap_or(u32::MAX));
                return Err(InstError::DanglingEdge { edge: id, endpoint });
            }
        }
        let id = EdgeId(
            self.edge_ids
                .allocate()
                .map_err(|e| e.whole_state(IdKind::Edge))?,
        );
        self.insert_edge(id, GEdge::new(src, tgt, edge))?;
        Ok(id)
    }

    /// Remove an edge, returning it.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::EdgeNotFound`] if there is no such edge.
    pub fn remove_edge(&mut self, id: EdgeId) -> Result<GEdge, InstError> {
        let edge = self.edges.remove(&id).ok_or(InstError::EdgeNotFound(id))?;
        self.edge_order.remove(&id.0);
        if self.edge_ids.floor() == id.0.checked_add(1) {
            self.edge_ids =
                IdAllocator::from_source(IdRange::FULL, self.edge_order.last().copied());
        }
        index_remove(&mut self.out_index, edge.src, id);
        index_remove(&mut self.in_index, edge.tgt, id);
        Ok(edge)
    }

    /// Move an edge to new endpoints, returning the old `(src, tgt)`.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::EdgeNotFound`] if there is no such edge and
    /// [`InstError::DanglingEdge`] if a new endpoint is not a node; the edge
    /// is unchanged on error.
    pub fn set_endpoints(
        &mut self,
        id: EdgeId,
        src: u32,
        tgt: u32,
    ) -> Result<(u32, u32), InstError> {
        let Some(edge) = self.edges.get(&id) else {
            return Err(InstError::EdgeNotFound(id));
        };
        for endpoint in [src, tgt] {
            if !self.nodes.contains_key(&endpoint) {
                return Err(InstError::DanglingEdge { edge: id, endpoint });
            }
        }
        let old = (edge.src, edge.tgt);
        index_remove(&mut self.out_index, old.0, id);
        index_remove(&mut self.in_index, old.1, id);
        index_insert(&mut self.out_index, src, id);
        index_insert(&mut self.in_index, tgt, id);
        if let Some(edge) = self.edges.get_mut(&id) {
            edge.src = src;
            edge.tgt = tgt;
        }
        Ok(old)
    }

    /// Set an edge's position, returning the old one.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::EdgeNotFound`] if there is no such edge.
    pub fn set_edge_position(
        &mut self,
        id: EdgeId,
        position: Option<u32>,
    ) -> Result<Option<u32>, InstError> {
        let edge = self.edges.get_mut(&id).ok_or(InstError::EdgeNotFound(id))?;
        Ok(std::mem::replace(&mut edge.position, position))
    }

    /// Change the schema edge an edge instantiates, returning the old one.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::EdgeNotFound`] if there is no such edge.
    pub fn relabel_edge(&mut self, id: EdgeId, label: Edge) -> Result<Edge, InstError> {
        let edge = self.edges.get_mut(&id).ok_or(InstError::EdgeNotFound(id))?;
        Ok(std::mem::replace(&mut edge.edge, label))
    }

    /// Mutable access to a node's data.
    ///
    /// The node's `id` must not be changed: it is the key the node is filed
    /// under and the name every edge uses for it. Debug builds check this on
    /// the next read of the node and on serialization.
    pub fn node_mut(&mut self, id: u32) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    // -----------------------------------------------------------------------
    // Equality
    // -----------------------------------------------------------------------

    /// Whether two instances have the same nodes and the same edges under the
    /// same ids, treating two `NaN` floats as equal.
    ///
    /// This is equality of presentations, not the equivalence the lens laws
    /// use. Law equivalence is the identity on ids that come from the source
    /// and matching by synthesized key on synthesized objects; no graph law
    /// is stated up to isomorphism or bisimulation. Isomorphism would forget
    /// which view element corresponds to which source element, which is
    /// exactly what `put` needs (inferred); deciding it for graphs with
    /// unlabelled nodes is also GI-complete in the worst case (Hogan 2017,
    /// Thm 3.3, stated, for RDF graphs with blank nodes). Bisimulation would
    /// identify parallel edges, which a graph instance keeps apart (Hidaka et
    /// al. 2013, stated; Asada et al. 2013, stated).
    #[must_use]
    pub fn identical(a: &Self, b: &Self) -> bool {
        a.edges == b.edges && nodes_identical(&a.nodes, &b.nodes)
    }

    /// Whether two instances have the same nodes (by id, `NaN`-tolerant) and
    /// the same multiset of edges `(src, tgt, edge, position)`, whatever ids
    /// the edges carry.
    ///
    /// Like [`Self::identical`], this is not the law equivalence: it forgets
    /// edge identity, which is the source correspondence of an edge, while
    /// still counting parallel edges (which bisimulation would merge; Hidaka
    /// et al. 2013, stated; Asada et al. 2013, stated) and without searching
    /// for a node isomorphism (GI-complete in the worst case; Hogan 2017,
    /// Thm 3.3, stated).
    #[must_use]
    pub fn equivalent_modulo_edge_ids(a: &Self, b: &Self) -> bool {
        if a.edges.len() != b.edges.len() || !nodes_identical(&a.nodes, &b.nodes) {
            return false;
        }
        edge_multiset(a) == edge_multiset(b)
    }
}

/// A graph's edges as a sorted list of `(src, tgt, edge, position)`.
fn edge_multiset(graph: &GInstance) -> Vec<(u32, u32, &Edge, Option<u32>)> {
    let mut edges: Vec<(u32, u32, &Edge, Option<u32>)> = graph
        .edges
        .values()
        .map(|e| (e.src, e.tgt, &e.edge, e.position))
        .collect();
    edges.sort_unstable();
    edges
}

/// Node tables equal by id under the `NaN`-tolerant value comparison.
fn nodes_identical(a: &HashMap<u32, Node>, b: &HashMap<u32, Node>) -> bool {
    a.len() == b.len()
        && a.iter().all(|(id, node)| {
            b.get(id)
                .is_some_and(|theirs| nodes_equal(node, theirs, values_identical))
        })
}

/// Field-by-field node equality, comparing values with `value_eq`.
fn nodes_equal(a: &Node, b: &Node, value_eq: fn(&Value, &Value) -> bool) -> bool {
    let presence_eq = |x: &Option<FieldPresence>, y: &Option<FieldPresence>| match (x, y) {
        (Some(FieldPresence::Present(x)), Some(FieldPresence::Present(y))) => value_eq(x, y),
        (None, None)
        | (Some(FieldPresence::Null), Some(FieldPresence::Null))
        | (Some(FieldPresence::Absent), Some(FieldPresence::Absent)) => true,
        _ => false,
    };
    a.id == b.id
        && a.anchor == b.anchor
        && presence_eq(&a.value, &b.value)
        && a.discriminator == b.discriminator
        && maps_equal(&a.extra_fields, &b.extra_fields, value_eq)
        && a.position == b.position
        && a.shape == b.shape
        && maps_equal(&a.annotations, &b.annotations, value_eq)
}

fn maps_equal(
    a: &HashMap<String, Value>,
    b: &HashMap<String, Value>,
    value_eq: fn(&Value, &Value) -> bool,
) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(key, value)| b.get(key).is_some_and(|theirs| value_eq(value, theirs)))
}

/// Strict value equality, as `Value`'s own `==`.
fn values_equal(a: &Value, b: &Value) -> bool {
    a == b
}

/// Value equality under which two `NaN` floats are equal.
fn values_identical(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => {
            x.partial_cmp(y) == Some(std::cmp::Ordering::Equal) || (x.is_nan() && y.is_nan())
        }
        (Value::List(xs), Value::List(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| values_identical(x, y))
        }
        (Value::Unknown(xs), Value::Unknown(ys)) => maps_equal(xs, ys, values_identical),
        (
            Value::Opaque {
                type_: tx,
                fields: fx,
            },
            Value::Opaque {
                type_: ty,
                fields: fy,
            },
        ) => tx == ty && maps_equal(fx, fy, values_identical),
        _ => a == b,
    }
}

impl GInstance {
    /// Insert an edge under `id` without the checks of
    /// [`Self::insert_edge`], keeping every index current.
    ///
    /// For an engine that builds a graph from parts it has already checked:
    /// the caller guarantees that `id` is unused and that both endpoints
    /// are nodes of the graph (debug builds assert both).
    pub(crate) fn place_edge(&mut self, id: EdgeId, edge: GEdge) {
        debug_assert!(!self.edges.contains_key(&id), "edge {id} placed twice");
        debug_assert!(
            self.nodes.contains_key(&edge.src) && self.nodes.contains_key(&edge.tgt),
            "edge {id} placed between missing nodes"
        );
        index_insert(&mut self.out_index, edge.src, id);
        index_insert(&mut self.in_index, edge.tgt, id);
        self.edge_ids.observe(id.0);
        self.edge_order.insert(id.0);
        self.edges.insert(id, edge);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::wtype::CompiledMigration;
    use panproto_schema::Schema;

    fn knows() -> Edge {
        Edge {
            src: "person".into(),
            tgt: "person".into(),
            kind: "knows".into(),
            name: None,
        }
    }

    fn people(n: u32) -> GInstance {
        (0..n).fold(GInstance::new(), |g, id| {
            g.with_node(Node::new(id, "person"))
        })
    }

    #[test]
    fn empty_graph_instance() {
        let g = GInstance::new();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
        assert_eq!(g.max_node_id(), None);
        assert_eq!(g.max_edge_id(), None);
    }

    #[test]
    fn build_graph_instance_puts_values_on_nodes() {
        let g = people(2)
            .with_edge(0, 1, knows())
            .with_value(0, Value::Str("Alice".into()))
            .with_value(1, Value::Str("Bob".into()));

        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);
        assert_eq!(
            g.node(0).unwrap().value,
            Some(FieldPresence::Present(Value::Str("Alice".into())))
        );
    }

    #[test]
    fn parallel_edges_and_self_loops_are_distinct_edges() {
        let g = people(2)
            .with_edge(0, 1, knows())
            .with_edge(0, 1, knows())
            .with_edge(0, 0, knows());
        assert_eq!(g.edge_count(), 3);
        assert_eq!(g.edges_between(0, 1), vec![EdgeId(0), EdgeId(1)]);
        assert_eq!(g.edges_between(0, 0), vec![EdgeId(2)]);
        assert_eq!(g.successors(0).collect::<Vec<_>>(), vec![1, 1, 0]);
        assert_eq!(g.predecessors(0).collect::<Vec<_>>(), vec![0]);
        assert_eq!(g.in_edges(1), &[EdgeId(0), EdgeId(1)]);
    }

    #[test]
    fn add_edge_allocates_one_above_the_largest_id() {
        let mut g = people(2).with_edge_id(EdgeId(7), 0, 1, knows());
        assert_eq!(g.add_edge(1, 0, knows()).unwrap(), EdgeId(8));
        assert_eq!(g.max_edge_id(), Some(EdgeId(8)));
    }

    #[test]
    fn add_edge_allocates_from_the_edges_present_alone() {
        // Removing the largest edge lowers the next id to one above the
        // largest left; removing a smaller one does not.
        let mut g = people(2)
            .with_edge(0, 1, knows())
            .with_edge(1, 0, knows())
            .with_edge(0, 0, knows());
        g.remove_edge(EdgeId(1)).unwrap();
        assert_eq!(g.add_edge(1, 1, knows()).unwrap(), EdgeId(3));
        g.remove_edge(EdgeId(3)).unwrap();
        g.remove_edge(EdgeId(2)).unwrap();
        assert_eq!(g.add_edge(1, 1, knows()).unwrap(), EdgeId(1));

        // Two graphs equal in every observable sense allocate the same id,
        // whatever their histories: here one of them is a decoded copy.
        let mut a = people(2).with_edge(0, 1, knows()).with_edge(1, 0, knows());
        a.remove_edge(EdgeId(1)).unwrap();
        let mut b: GInstance = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert_eq!(a, b);
        assert!(GInstance::identical(&a, &b));
        assert_eq!(a.digest(), b.digest());
        assert_eq!(a.add_edge(0, 0, knows()).unwrap(), EdgeId(1));
        assert_eq!(b.add_edge(0, 0, knows()).unwrap(), EdgeId(1));

        // A graph whose every edge is removed allocates from 0 again.
        let mut c = people(2).with_edge(0, 1, knows());
        c.remove_edge(EdgeId(0)).unwrap();
        assert_eq!(c.add_edge(0, 1, knows()).unwrap(), EdgeId(0));

        // A failed call allocates nothing.
        assert!(matches!(
            c.add_edge(0, 9, knows()),
            Err(InstError::DanglingEdge {
                edge: EdgeId(1),
                endpoint: 9
            })
        ));
        assert_eq!(c.add_edge(0, 1, knows()).unwrap(), EdgeId(1));
    }

    #[test]
    fn removing_the_largest_edges_one_after_another_lowers_the_floor_each_time() {
        // The floor follows the largest id left down across gaps, and a
        // restricted copy keeps the ids it carried in order.
        let mut g = people(2)
            .with_edge_id(EdgeId(0), 0, 1, knows())
            .with_edge_id(EdgeId(5), 1, 0, knows())
            .with_edge_id(EdgeId(9), 0, 0, knows());
        g.remove_edge(EdgeId(9)).unwrap();
        assert_eq!(g.max_edge_id(), Some(EdgeId(5)));
        assert_eq!(g.add_edge(1, 1, knows()).unwrap(), EdgeId(6));
        g.remove_edge(EdgeId(6)).unwrap();
        g.remove_edge(EdgeId(5)).unwrap();
        assert_eq!(g.max_edge_id(), Some(EdgeId(0)));
        assert_eq!(g.add_edge(1, 1, knows()).unwrap(), EdgeId(1));
        g.remove_edge(EdgeId(1)).unwrap();
        g.remove_edge(EdgeId(0)).unwrap();
        assert_eq!(g.max_edge_id(), None);
        assert_eq!(g.add_edge(1, 1, knows()).unwrap(), EdgeId(0));

        let h = people(2)
            .with_edge_id(EdgeId(3), 0, 1, knows())
            .with_edge_id(EdgeId(8), 1, 0, knows());
        let schema = knows_schema();
        let plan =
            crate::RestrictPlan::compile(&schema, &schema, &migration(&["person"], &[])).unwrap();
        let (mut restricted, _) =
            crate::graph_restrict_with_plan(&h, &plan).expect("the restrict should succeed");
        restricted.remove_edge(EdgeId(8)).unwrap();
        assert_eq!(restricted.max_edge_id(), Some(EdgeId(3)));
        assert_eq!(restricted.add_edge(0, 0, knows()).unwrap(), EdgeId(4));
    }

    #[test]
    fn removing_the_edge_u32_max_frees_the_id_space() {
        let mut g = people(2).with_edge_id(EdgeId(u32::MAX), 0, 1, knows());
        assert!(matches!(
            g.add_edge(0, 1, knows()),
            Err(InstError::EdgeIdsExhausted)
        ));
        g.remove_edge(EdgeId(u32::MAX)).unwrap();
        assert_eq!(g.add_edge(0, 1, knows()).unwrap(), EdgeId(0));
    }

    #[test]
    fn a_restricted_graph_allocates_above_the_edges_it_kept() {
        let g = people(2).with_edge_id(EdgeId(6), 0, 1, knows());
        let schema = knows_schema();
        let plan =
            crate::RestrictPlan::compile(&schema, &schema, &migration(&["person"], &[])).unwrap();
        let (mut restricted, _) =
            crate::graph_restrict_with_plan(&g, &plan).expect("the restrict should succeed");
        assert_eq!(restricted.add_edge(1, 0, knows()).unwrap(), EdgeId(7));
    }

    #[test]
    fn add_edge_reports_an_exhausted_id_space() {
        let mut g = people(1).with_edge_id(EdgeId(u32::MAX), 0, 0, knows());
        assert!(matches!(
            g.add_edge(0, 0, knows()),
            Err(InstError::EdgeIdsExhausted)
        ));
    }

    #[test]
    fn insert_edge_rejects_duplicates_and_dangling_endpoints() {
        let mut g = people(2).with_edge(0, 1, knows());
        assert!(matches!(
            g.insert_edge(EdgeId(0), GEdge::new(1, 0, knows())),
            Err(InstError::DuplicateEdge(EdgeId(0)))
        ));
        assert!(matches!(
            g.insert_edge(EdgeId(1), GEdge::new(0, 9, knows())),
            Err(InstError::DanglingEdge {
                edge: EdgeId(1),
                endpoint: 9
            })
        ));
        assert_eq!(g.edge_count(), 1);
        assert_eq!(g.out_edges(0), &[EdgeId(0)]);
    }

    #[test]
    fn insert_node_rejects_a_duplicate_id() {
        let mut g = people(1);
        assert!(matches!(
            g.insert_node(Node::new(0, "other")),
            Err(InstError::DuplicateNode(0))
        ));
        assert_eq!(g.node(0).unwrap().anchor, "person");
    }

    #[test]
    fn remove_node_refuses_while_edges_touch_it() {
        let mut g = people(3).with_edge(0, 1, knows()).with_edge(2, 1, knows());
        match g.remove_node(1) {
            Err(InstError::IncidentEdges { node: 1, edges }) => {
                assert_eq!(edges, vec![EdgeId(0), EdgeId(1)]);
            }
            other => panic!("expected incident edges, got {other:?}"),
        }
        g.remove_edge(EdgeId(0)).unwrap();
        g.remove_edge(EdgeId(1)).unwrap();
        assert_eq!(g.remove_node(1).unwrap().id, 1);
        assert!(g.out_edges(0).is_empty());
        assert!(g.in_edges(1).is_empty());
        assert!(matches!(g.remove_node(1), Err(InstError::NodeNotFound(1))));
    }

    #[test]
    fn set_endpoints_moves_the_edge_between_indices() {
        let mut g = people(3).with_edge(0, 1, knows());
        assert_eq!(g.set_endpoints(EdgeId(0), 2, 0).unwrap(), (0, 1));
        assert!(g.out_edges(0).is_empty());
        assert!(g.in_edges(1).is_empty());
        assert_eq!(g.out_edges(2), &[EdgeId(0)]);
        assert_eq!(g.in_edges(0), &[EdgeId(0)]);
        assert!(matches!(
            g.set_endpoints(EdgeId(0), 2, 9),
            Err(InstError::DanglingEdge { .. })
        ));
        assert_eq!(g.edge(EdgeId(0)).unwrap().src, 2);
    }

    #[test]
    fn relabel_and_reposition_return_the_old_values() {
        let mut g = people(2).with_edge(0, 1, knows());
        let mut other = knows();
        other.kind = "likes".into();
        assert_eq!(g.relabel_edge(EdgeId(0), other.clone()).unwrap(), knows());
        assert_eq!(g.edge(EdgeId(0)).unwrap().edge, other);
        assert_eq!(g.set_edge_position(EdgeId(0), Some(3)).unwrap(), None);
        assert_eq!(g.edge(EdgeId(0)).unwrap().position, Some(3));
        assert!(matches!(
            g.relabel_edge(EdgeId(5), knows()),
            Err(InstError::EdgeNotFound(EdgeId(5)))
        ));
    }

    #[test]
    fn ordered_out_edges_sort_by_position_then_id() {
        let mut g = people(4)
            .with_edge(0, 1, knows())
            .with_edge(0, 2, knows())
            .with_edge(0, 3, knows())
            .with_edge(0, 1, knows());
        g.set_edge_position(EdgeId(0), Some(2)).unwrap();
        g.set_edge_position(EdgeId(1), Some(1)).unwrap();
        assert_eq!(
            g.ordered_out_edges(0),
            vec![EdgeId(2), EdgeId(3), EdgeId(1), EdgeId(0)],
            "absent positions first, by id; then by position"
        );
    }

    #[test]
    fn equality_notions_differ_on_edge_ids_and_nan() {
        let a = people(2)
            .with_edge(0, 1, knows())
            .with_value(0, Value::Float(f64::NAN));
        let b = people(2)
            .with_edge_id(EdgeId(5), 0, 1, knows())
            .with_value(0, Value::Float(f64::NAN));
        assert_ne!(a, a.clone(), "NaN is not equal to itself under ==");
        assert!(GInstance::identical(&a, &a.clone()));
        assert!(!GInstance::identical(&a, &b));
        assert!(GInstance::equivalent_modulo_edge_ids(&a, &b));
        let c = b.with_edge(1, 0, knows());
        assert!(!GInstance::equivalent_modulo_edge_ids(&a, &c));
    }

    #[test]
    fn the_encoding_round_trips_and_rebuilds_the_indices() {
        let g = people(3)
            .with_edge(0, 1, knows())
            .with_edge(1, 2, knows())
            .with_edge(2, 0, knows())
            .with_value(1, Value::Int(4));
        let json = serde_json::to_string(&g).unwrap();
        let back: GInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(back, g);
        assert_eq!(back.out_edges(1), &[EdgeId(1)]);
        assert_eq!(back.in_edges(0), &[EdgeId(2)]);
    }

    #[test]
    fn decoding_rejects_a_dangling_edge() {
        let json = r#"{"nodes":{"0":{"id":0,"anchor":"a","value":null,"discriminator":null,"extra_fields":{}}},
            "edges":{"3":{"src":0,"tgt":4,"edge":{"src":"a","tgt":"a","kind":"k","name":null},"position":null}}}"#;
        let err = serde_json::from_str::<GInstance>(json).unwrap_err();
        assert!(
            err.to_string().contains("edge 3") && err.to_string().contains("node 4"),
            "{err}"
        );
    }

    #[test]
    fn decoding_rejects_a_node_filed_under_another_id() {
        let json = r#"{"nodes":{"1":{"id":2,"anchor":"a","value":null,"discriminator":null,"extra_fields":{}}},"edges":{}}"#;
        let err = serde_json::from_str::<GInstance>(json).unwrap_err();
        assert!(err.to_string().contains("filed under 1"), "{err}");
    }

    fn migration(surviving: &[&str], remap: &[(&str, &str)]) -> CompiledMigration {
        CompiledMigration {
            surviving_verts: surviving.iter().map(|v| (*v).into()).collect(),
            surviving_edges: std::iter::once(knows()).collect(),
            vertex_remap: remap
                .iter()
                .map(|(s, t)| ((*s).into(), (*t).into()))
                .collect(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        }
    }

    fn knows_schema() -> Schema {
        panproto_schema::SchemaBuilder::new(&panproto_schema::Protocol::default())
            .vertex("person", "record", None)
            .expect("a vertex")
            .edge("person", "person", "knows", None)
            .expect("an edge")
            .build()
            .expect("a one-vertex schema builds")
    }
}
