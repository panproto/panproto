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

/// A variant in a coproduct (sum type / union).
///
/// Each variant is injected into a parent vertex (the union/coproduct)
/// with an optional discriminant tag.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Variant {
    /// Unique variant identifier.
    pub id: String,
    /// The parent coproduct vertex this variant belongs to.
    pub parent_vertex: String,
    /// Optional discriminant tag.
    pub tag: Option<String>,
}

/// An ordering annotation on an edge.
///
/// Records that the children reached via this edge are ordered,
/// with a specific position index.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Ordering {
    /// The edge being ordered.
    pub edge: Edge,
    /// Position in the ordered collection.
    pub position: u32,
}

/// A recursion point (fixpoint marker) in the schema.
///
/// Marks a vertex as a recursive reference to another vertex,
/// satisfying the fold-unfold law: `unfold(fold(v)) = v`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecursionPoint {
    /// The fixpoint marker vertex ID.
    pub mu_id: String,
    /// The target vertex this unfolds to.
    pub target_vertex: String,
}

/// A span connecting two vertices through a common source.
///
/// Spans model correspondences, diffs, and migrations:
/// `left ← span → right`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    /// Unique span identifier.
    pub id: String,
    /// Left vertex of the span.
    pub left: String,
    /// Right vertex of the span.
    pub right: String,
}

/// Use-counting mode for an edge.
///
/// Captures the substructural distinction between edges that can
/// be used freely (structural), exactly once (linear), or at most
/// once (affine).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum UsageMode {
    /// Can be used any number of times (default).
    #[default]
    Structural,
    /// Must be used exactly once (e.g., protobuf `oneof`).
    Linear,
    /// Can be used at most once.
    Affine,
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
    #[serde(with = "crate::serde_helpers::map_as_vec")]
    pub edges: HashMap<Edge, String>,
    /// Hyper-edges keyed by their ID.
    pub hyper_edges: HashMap<String, HyperEdge>,
    /// Constraints per vertex ID.
    pub constraints: HashMap<String, Vec<Constraint>>,
    /// Required edges per vertex ID.
    pub required: HashMap<String, Vec<Edge>>,
    /// NSID mapping: vertex ID to NSID string.
    pub nsids: HashMap<String, String>,

    /// Coproduct variants per union vertex ID.
    #[serde(default)]
    pub variants: HashMap<String, Vec<Variant>>,
    /// Edge ordering positions (edge → position index).
    #[serde(default, with = "crate::serde_helpers::map_as_vec_default")]
    pub orderings: HashMap<Edge, u32>,
    /// Recursion points (fixpoint markers).
    #[serde(default)]
    pub recursion_points: HashMap<String, RecursionPoint>,
    /// Spans connecting pairs of vertices.
    #[serde(default)]
    pub spans: HashMap<String, Span>,
    /// Edge usage modes (default: `Structural` for all).
    #[serde(default, with = "crate::serde_helpers::map_as_vec_default")]
    pub usage_modes: HashMap<Edge, UsageMode>,
    /// Whether each vertex uses nominal identity (`true`) or
    /// structural identity (`false`). Absent = structural.
    #[serde(default)]
    pub nominal: HashMap<String, bool>,

    // -- precomputed indices --
    /// Outgoing edges per vertex ID.
    pub outgoing: HashMap<String, SmallVec<Edge, 4>>,
    /// Incoming edges per vertex ID.
    pub incoming: HashMap<String, SmallVec<Edge, 4>>,
    /// Edges between a specific `(src, tgt)` pair.
    #[serde(with = "crate::serde_helpers::map_as_vec")]
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
    #[inline]
    pub fn edges_between(&self, src: &str, tgt: &str) -> &[Edge] {
        self.between
            .get(&(src.to_owned(), tgt.to_owned()))
            .map_or(&[], SmallVec::as_slice)
    }

    /// Returns `true` if the given vertex ID exists in this schema.
    #[must_use]
    #[inline]
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
