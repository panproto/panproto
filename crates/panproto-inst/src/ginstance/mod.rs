//! Graph-shaped instance representation.
//!
//! A [`GInstance`] is the most general instance form: a directed multigraph
//! of nodes and identified edges with no distinguished root, cycles, self-loops
//! and parallel edges allowed. Both [`WInstance`](crate::WInstance) (trees) and
//! [`FInstance`](crate::FInstance) (tables) embed into it; see
//! [`GInstance::from_wtype`] and [`GInstance::from_finstance`].
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

mod embed;
mod nodelink;
mod reify;

use std::collections::HashMap;
use std::fmt;

use panproto_schema::{Edge, Schema};
use serde::{Deserialize, Serialize};

use crate::error::{InstError, RestrictError};
use crate::metadata::Node;
use crate::value::{FieldPresence, Value};
use crate::wtype::CompiledMigration;

pub use nodelink::{from_node_link_json, to_node_link_json};
pub use reify::{HYPER_LABEL_KIND, HYPER_PARENT_KIND, reify_fans, unreify_fans};

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
/// process. Decoding rebuilds the adjacency indices and fails with
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(into = "GInstanceWire", try_from = "GInstanceWireEntries")]
pub struct GInstance {
    nodes: HashMap<u32, Node>,
    edges: HashMap<EdgeId, GEdge>,
    /// Out-edges of each node, ascending.
    out_index: HashMap<u32, Vec<EdgeId>>,
    /// In-edges of each node, ascending.
    in_index: HashMap<u32, Vec<EdgeId>>,
}

/// The serialized form of a [`GInstance`]: its node and edge tables.
#[derive(Serialize)]
struct GInstanceWire {
    #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
    nodes: HashMap<u32, Node>,
    #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
    edges: HashMap<EdgeId, GEdge>,
}

impl From<GInstance> for GInstanceWire {
    fn from(graph: GInstance) -> Self {
        debug_assert!(
            graph.nodes.iter().all(|(key, node)| *key == node.id),
            "a node's id was changed through node_mut"
        );
        Self {
            nodes: graph.nodes,
            edges: graph.edges,
        }
    }
}

/// [`GInstanceWire`] as decoded: each map's entries in document order, a key
/// listed twice kept twice so that decoding can refuse it.
#[derive(Deserialize)]
struct GInstanceWireEntries {
    #[serde(deserialize_with = "map_entries")]
    nodes: Vec<(u32, Node)>,
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

    /// Add an edge from `src` to `tgt` under the next free [`EdgeId`].
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
        self.edges.keys().copied().max()
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
        Ok(())
    }

    /// Add an edge under the next free id, one above the largest in use.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::DanglingEdge`] if an endpoint is not a node and
    /// [`InstError::EdgeIdsExhausted`] if `u32::MAX` is already in use.
    pub fn add_edge(&mut self, src: u32, tgt: u32, edge: Edge) -> Result<EdgeId, InstError> {
        let id = match self.max_edge_id() {
            None => EdgeId(0),
            Some(EdgeId(max)) => EdgeId(max.checked_add(1).ok_or(InstError::EdgeIdsExhausted)?),
        };
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
    #[must_use]
    pub fn identical(a: &Self, b: &Self) -> bool {
        a.edges == b.edges && nodes_identical(&a.nodes, &b.nodes)
    }

    /// Whether two instances have the same nodes (by id, `NaN`-tolerant) and
    /// the same multiset of edges `(src, tgt, edge, position)`, whatever ids
    /// the edges carry.
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

/// The target anchor a node's anchor maps to: its image under
/// `vertex_remap`, or the anchor itself when the vertex is not renamed.
fn target_anchor<'a>(
    migration: &'a CompiledMigration,
    anchor: &'a panproto_gat::Name,
) -> &'a panproto_gat::Name {
    migration.vertex_remap.get(anchor).unwrap_or(anchor)
}

/// Restrict a graph instance along a compiled migration.
///
/// A node survives when its target anchor, the image of its anchor under
/// `vertex_remap` or the anchor itself when the vertex is not renamed, is in
/// `surviving_verts`. A surviving node is re-anchored to its target anchor and
/// keeps its id, value and other data. An edge survives, under its own
/// [`EdgeId`] and position, when both endpoints survive and its schema edge is
/// either renamed by `edge_remap` or listed in `surviving_edges`.
///
/// # Errors
///
/// Returns an error if the restrict pipeline fails.
pub fn graph_restrict(
    instance: &GInstance,
    _src_schema: &Schema,
    _tgt_schema: &Schema,
    migration: &CompiledMigration,
) -> Result<GInstance, RestrictError> {
    let mut result = GInstance::new();
    for (&id, node) in &instance.nodes {
        let anchor = target_anchor(migration, &node.anchor);
        if migration.surviving_verts.contains(anchor) {
            let mut kept = node.clone();
            kept.anchor.clone_from(anchor);
            result.nodes.insert(id, kept);
        }
    }
    carry_edges(instance, migration, &mut result);
    Ok(result)
}

/// Extend (left Kan extension, `Sigma_F`) a graph instance along a migration.
///
/// Maps source nodes into the target schema by remapping anchors through
/// `vertex_remap`. A node whose anchor has no remap is kept under its own
/// anchor when that anchor is in `surviving_verts` and dropped otherwise. An
/// edge is carried, under its own [`EdgeId`] and position, when both endpoints
/// are kept and its schema edge is renamed by `edge_remap` or listed in
/// `surviving_edges`.
///
/// # Errors
///
/// Returns [`RestrictError`] if the extend pipeline fails.
pub fn graph_extend(
    instance: &GInstance,
    _tgt_schema: &Schema,
    migration: &CompiledMigration,
) -> Result<GInstance, RestrictError> {
    let mut result = GInstance::new();
    for (&id, node) in &instance.nodes {
        let mut kept = node.clone();
        if let Some(remapped) = migration.vertex_remap.get(&node.anchor) {
            kept.anchor.clone_from(remapped);
        } else if !migration.surviving_verts.contains(&node.anchor) {
            continue;
        }
        result.nodes.insert(id, kept);
    }
    carry_edges(instance, migration, &mut result);
    Ok(result)
}

/// Copy into `result` every edge of `instance` whose endpoints are both nodes
/// of `result` and whose schema edge the migration keeps, relabelled through
/// `edge_remap`.
fn carry_edges(instance: &GInstance, migration: &CompiledMigration, result: &mut GInstance) {
    for (id, edge) in instance.edges_sorted() {
        if !result.nodes.contains_key(&edge.src) || !result.nodes.contains_key(&edge.tgt) {
            continue;
        }
        let label = if let Some(renamed) = migration.edge_remap.get(&edge.edge) {
            renamed.clone()
        } else if migration.surviving_edges.contains(&edge.edge) {
            edge.edge.clone()
        } else {
            continue;
        };
        index_insert(&mut result.out_index, edge.src, id);
        index_insert(&mut result.in_index, edge.tgt, id);
        result.edges.insert(
            id,
            GEdge {
                src: edge.src,
                tgt: edge.tgt,
                edge: label,
                position: edge.position,
            },
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

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

    fn empty_schema() -> Schema {
        panproto_schema::SchemaBuilder::new(&panproto_schema::Protocol::default())
            .vertex("person", "record", None)
            .expect("a vertex")
            .build()
            .expect("a one-vertex schema builds")
    }

    #[test]
    fn graph_restrict_drops_unmapped_nodes() {
        let g = GInstance::new()
            .with_node(Node::new(0, "a"))
            .with_node(Node::new(1, "b"))
            .with_node(Node::new(2, "c"));
        let schema = empty_schema();
        let restricted = graph_restrict(
            &g,
            &schema,
            &schema,
            &migration(&["a_new"], &[("a", "a_new")]),
        )
        .expect("graph_restrict should succeed");
        assert_eq!(restricted.node_count(), 1);
        assert_eq!(restricted.node(0).unwrap().anchor, "a_new");
    }

    #[test]
    fn graph_restrict_keeps_edge_ids_and_positions() {
        let mut g = people(3)
            .with_edge_id(EdgeId(4), 0, 1, knows())
            .with_edge_id(EdgeId(9), 1, 2, knows());
        g.set_edge_position(EdgeId(9), Some(2)).unwrap();
        let schema = empty_schema();
        let restricted = graph_restrict(&g, &schema, &schema, &migration(&["person"], &[]))
            .expect("graph_restrict should succeed");
        assert!(GInstance::identical(&restricted, &g));
    }
}
