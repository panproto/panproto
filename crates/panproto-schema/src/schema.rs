//! Core schema data structures.
//!
//! A [`Schema`] is a model of a protocol's schema theory GAT. It stores
//! vertices, binary edges, hyper-edges, constraints, required-edge
//! declarations, and NSID mappings. Precomputed adjacency indices
//! (`outgoing`, `incoming`, `between`) enable fast traversal.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// A schema vertex.
///
/// Each vertex has a unique `id`, a `kind` drawn from the protocol's
/// recognized vertex kinds, and an optional NSID (namespace identifier).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Vertex {
    /// Unique vertex identifier within the schema.
    pub id: String,
    /// The vertex kind (e.g., `"record"`, `"object"`, `"string"`).
    pub kind: String,
    /// Optional namespace identifier (e.g., `"app.bsky.feed.post"`).
    pub nsid: Option<String>,
}

/// A binary edge between two vertices.
///
/// Edges are directed: they go from `src` to `tgt`. The `kind` determines
/// the structural role (e.g., `"prop"`, `"record-schema"`), and `name`
/// provides an optional label (e.g., the property name).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Edge {
    /// Source vertex ID.
    pub src: String,
    /// Target vertex ID.
    pub tgt: String,
    /// Edge kind (e.g., `"prop"`, `"record-schema"`).
    pub kind: String,
    /// Optional edge label (e.g., a property name like `"text"`).
    pub name: Option<String>,
}

/// A hyper-edge (present only when the schema theory includes `ThHypergraph`).
///
/// Hyper-edges connect multiple vertices via a labeled signature.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HyperEdge {
    /// Unique hyper-edge identifier.
    pub id: String,
    /// Hyper-edge kind.
    pub kind: String,
    /// Maps label names to vertex IDs.
    pub signature: HashMap<String, String>,
    /// The label that identifies the parent vertex.
    pub parent_label: String,
}

/// A constraint on a vertex.
///
/// Constraints restrict the values a vertex can hold (e.g., maximum
/// string length, format pattern).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Constraint {
    /// The constraint sort (e.g., `"maxLength"`, `"format"`).
    pub sort: String,
    /// The constraint value (e.g., `"3000"`, `"at-uri"`).
    pub value: String,
}

/// A schema: a model of the protocol's schema theory.
///
/// Contains both the raw data (vertices, edges, constraints, etc.) and
/// precomputed adjacency indices for efficient graph traversal.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Schema {
    /// The protocol this schema belongs to.
    pub protocol: String,
    /// Vertices keyed by their ID.
    pub vertices: HashMap<String, Vertex>,
    /// Edges keyed by the edge itself, value is the edge kind.
    pub edges: HashMap<Edge, String>,
    /// Hyper-edges keyed by their ID.
    pub hyper_edges: HashMap<String, HyperEdge>,
    /// Constraints per vertex ID.
    pub constraints: HashMap<String, Vec<Constraint>>,
    /// Required edges per vertex ID.
    pub required: HashMap<String, Vec<Edge>>,
    /// NSID mapping: vertex ID to NSID string.
    pub nsids: HashMap<String, String>,

    // -- precomputed indices --
    /// Outgoing edges per vertex ID.
    pub outgoing: HashMap<String, SmallVec<Edge, 4>>,
    /// Incoming edges per vertex ID.
    pub incoming: HashMap<String, SmallVec<Edge, 4>>,
    /// Edges between a specific `(src, tgt)` pair.
    pub between: HashMap<(String, String), SmallVec<Edge, 2>>,
}

impl Schema {
    /// Look up a vertex by ID.
    #[must_use]
    pub fn vertex(&self, id: &str) -> Option<&Vertex> {
        self.vertices.get(id)
    }

    /// Return all outgoing edges from the given vertex.
    #[must_use]
    pub fn outgoing_edges(&self, vertex_id: &str) -> &[Edge] {
        self.outgoing.get(vertex_id).map_or(&[], SmallVec::as_slice)
    }

    /// Return all incoming edges to the given vertex.
    #[must_use]
    pub fn incoming_edges(&self, vertex_id: &str) -> &[Edge] {
        self.incoming.get(vertex_id).map_or(&[], SmallVec::as_slice)
    }

    /// Return edges between a specific `(src, tgt)` pair.
    #[must_use]
    pub fn edges_between(&self, src: &str, tgt: &str) -> &[Edge] {
        self.between
            .get(&(src.to_owned(), tgt.to_owned()))
            .map_or(&[], SmallVec::as_slice)
    }

    /// Returns `true` if the given vertex ID exists in this schema.
    #[must_use]
    pub fn has_vertex(&self, id: &str) -> bool {
        self.vertices.contains_key(id)
    }

    /// Returns the number of vertices in the schema.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Returns the number of edges in the schema.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}
