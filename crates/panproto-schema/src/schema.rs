//! Core schema data structures.
//!
//! A [`Schema`] is a model of a protocol's schema theory GAT. It stores
//! vertices, binary edges, hyper-edges, constraints, required-edge
//! declarations, and NSID mappings. Precomputed adjacency indices
//! (`outgoing`, `incoming`, `between`) enable fast traversal.

use std::collections::HashMap;

use panproto_gat::Name;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// A schema vertex.
///
/// Each vertex has a unique `id`, a `kind` drawn from the protocol's
/// recognized vertex kinds, and an optional NSID (namespace identifier).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Vertex {
    /// Unique vertex identifier within the schema.
    pub id: Name,
    /// The vertex kind (e.g., `"record"`, `"object"`, `"string"`).
    pub kind: Name,
    /// Optional namespace identifier (e.g., `"app.bsky.feed.post"`).
    pub nsid: Option<Name>,
}

/// A binary edge between two vertices.
///
/// Edges are directed: they go from `src` to `tgt`. The `kind` determines
/// the structural role (e.g., `"prop"`, `"record-schema"`), and `name`
/// provides an optional label (e.g., the property name).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Edge {
    /// Source vertex ID.
    pub src: Name,
    /// Target vertex ID.
    pub tgt: Name,
    /// Edge kind (e.g., `"prop"`, `"record-schema"`).
    pub kind: Name,
    /// Optional edge label (e.g., a property name like `"text"`).
    pub name: Option<Name>,
}

/// A hyper-edge (present only when the schema theory includes `ThHypergraph`).
///
/// Hyper-edges connect multiple vertices via a labeled signature.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HyperEdge {
    /// Unique hyper-edge identifier.
    pub id: Name,
    /// Hyper-edge kind.
    pub kind: Name,
    /// Maps label names to vertex IDs.
    pub signature: HashMap<Name, Name>,
    /// The label that identifies the parent vertex.
    pub parent_label: Name,
}

/// A constraint on a vertex.
///
/// Constraints restrict the values a vertex can hold (e.g., maximum
/// string length, format pattern).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Constraint {
    /// The constraint sort (e.g., `"maxLength"`, `"format"`).
    pub sort: Name,
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
    pub id: Name,
    /// The parent coproduct vertex this variant belongs to.
    pub parent_vertex: Name,
    /// Optional discriminant tag.
    pub tag: Option<Name>,
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
///
/// The marker vertex is the key this is filed under in
/// [`Schema::recursion_points`], and is deliberately not repeated here. Naming
/// it twice would make the two copies independently settable, and
/// deserialisation accepts whatever a file says: a schema filing a marker under
/// one name while the marker claimed another would be a contradiction no
/// constructor could rule out and every reader would have to pick a side.
/// Inducing, validation, diffing, hashing and the protocol emitters key on the
/// map; the search network and the span's right leg read the marker. With one
/// copy they cannot disagree, which is a stronger guarantee than any check
/// could give, since a check has to be run and this cannot be skipped.
/// [`ValidationError::DanglingRecursionPoint`](crate::ValidationError) covers
/// what remains, a marker naming a vertex the schema does not have.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecursionPoint {
    /// The target vertex this unfolds to.
    pub target_vertex: Name,
}

/// A span connecting two vertices through a common source.
///
/// Spans model correspondences, diffs, and migrations:
/// `left ← span → right`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    /// Unique span identifier.
    pub id: Name,
    /// Left vertex of the span.
    pub left: Name,
    /// Right vertex of the span.
    pub right: Name,
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

/// Specification of a coercion between two value kinds.
///
/// Contains the forward coercion expression, an optional inverse for
/// round-tripping, and the coercion class classifying the round-trip behavior.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoercionSpec {
    /// Forward coercion expression (source to target).
    pub forward: panproto_expr::Expr,
    /// Inverse coercion expression (target to source) for the `put` direction.
    pub inverse: Option<panproto_expr::Expr>,
    /// Round-trip classification.
    pub class: panproto_gat::CoercionClass,
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
    pub vertices: HashMap<Name, Vertex>,
    /// Edges keyed by the edge itself, value is the edge kind.
    #[serde(with = "crate::serde_helpers::map_as_vec")]
    pub edges: HashMap<Edge, Name>,
    /// Hyper-edges keyed by their ID.
    pub hyper_edges: HashMap<Name, HyperEdge>,
    /// Constraints per vertex ID.
    pub constraints: HashMap<Name, Vec<Constraint>>,
    /// Required edges per vertex ID.
    pub required: HashMap<Name, Vec<Edge>>,
    /// NSID mapping: vertex ID to NSID string.
    pub nsids: HashMap<Name, Name>,
    /// Declared entry vertices.
    ///
    /// Semantically, this is the finite family of basepoints that makes
    /// the schema a *pointed* schema: `E → Ob(C_S)` selecting the sorts
    /// at which the W-algebra of instances may be rooted. Parsers set
    /// this explicitly per their protocol's notion of a top-level
    /// definition (a record, a top-level type, a path root, etc.);
    /// consumers that need to choose an instance root consult it via
    /// [`primary_entry`].
    ///
    /// Empty means the parser declined to supply a pointing; consumers
    /// should then either fall back to a deterministic (but non-
    /// canonical) selection or report an error. Order is preserved for
    /// reproducibility; the set carries no duplicates.
    #[serde(default)]
    pub entries: Vec<Name>,

    /// Coproduct variants per union vertex ID.
    #[serde(default)]
    pub variants: HashMap<Name, Vec<Variant>>,
    /// Edge ordering positions (edge → position index).
    #[serde(default, with = "crate::serde_helpers::map_as_vec_default")]
    pub orderings: HashMap<Edge, u32>,
    /// Recursion points (fixpoint markers).
    #[serde(default)]
    pub recursion_points: HashMap<Name, RecursionPoint>,
    /// Spans connecting pairs of vertices.
    #[serde(default)]
    pub spans: HashMap<Name, Span>,
    /// Edge usage modes (default: `Structural` for all).
    #[serde(default, with = "crate::serde_helpers::map_as_vec_default")]
    pub usage_modes: HashMap<Edge, UsageMode>,
    /// Whether each vertex uses nominal identity (`true`) or
    /// structural identity (`false`). Absent = structural.
    #[serde(default)]
    pub nominal: HashMap<Name, bool>,

    // -- enrichment fields --
    /// Coercion specifications: `(source_kind, target_kind)` to coercion spec.
    #[serde(default, with = "crate::serde_helpers::map_as_vec_default")]
    pub coercions: HashMap<(Name, Name), CoercionSpec>,
    /// Merger expressions: `vertex_id` to merger expression.
    #[serde(default)]
    pub mergers: HashMap<Name, panproto_expr::Expr>,
    /// Default value expressions: `vertex_id` to default expression.
    #[serde(default)]
    pub defaults: HashMap<Name, panproto_expr::Expr>,
    /// Conflict resolution policy expressions: `sort_name` to policy expression.
    #[serde(default)]
    pub policies: HashMap<Name, panproto_expr::Expr>,

    // -- precomputed indices --
    /// Outgoing edges per vertex ID.
    pub outgoing: HashMap<Name, SmallVec<Edge, 4>>,
    /// Incoming edges per vertex ID.
    pub incoming: HashMap<Name, SmallVec<Edge, 4>>,
    /// Edges between a specific `(src, tgt)` pair.
    #[serde(with = "crate::serde_helpers::map_as_vec")]
    pub between: HashMap<(Name, Name), SmallVec<Edge, 2>>,
}

impl Schema {
    /// Strip every constraint whose sort belongs to the layout
    /// enrichment fibre (per [`panproto_gat::is_layout_sort`]).
    ///
    /// This is the schema-level forgetful U sending a decorated schema
    /// to its abstract base in the parse/decorate/emit lens. Idempotent.
    #[must_use]
    pub fn forget_layout(&self) -> Self {
        let mut clone = self.clone();
        clone.forget_layout_in_place();
        clone
    }

    /// In-place variant of [`Self::forget_layout`].
    pub fn forget_layout_in_place(&mut self) {
        for constraints in self.constraints.values_mut() {
            constraints.retain(|c| !panproto_gat::is_layout_sort(c.sort.as_ref()));
        }
        // Drop now-empty vertex entries so equality is structural.
        self.constraints.retain(|_, cs| !cs.is_empty());
    }

    /// Returns `true` when no constraint sort belongs to the layout
    /// enrichment fibre. This is the well-formedness predicate for an
    /// [`AbstractSchema`](crate::AbstractSchema).
    #[must_use]
    pub fn is_layout_free(&self) -> bool {
        self.constraints.values().all(|cs| {
            cs.iter()
                .all(|c| !panproto_gat::is_layout_sort(c.sort.as_ref()))
        })
    }

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
            .get(&(Name::from(src), Name::from(tgt)))
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

    /// Return the declared entry vertices.
    ///
    /// See [`Schema::entries`] for semantics. Use [`primary_entry`] for
    /// callers that need a single root and want a deterministic
    /// fallback when no entries are declared.
    #[must_use]
    pub fn entry_vertices(&self) -> &[Name] {
        &self.entries
    }

    /// Return every constraint attached to the given vertex.
    ///
    /// Tree-sitter-derived schemas attach byte ranges, interstitials,
    /// formatting, and `field:<name>` entries here.
    #[must_use]
    pub fn constraints_for(&self, vertex_id: &str) -> &[Constraint] {
        self.constraints
            .get(&Name::from(vertex_id))
            .map_or(&[], Vec::as_slice)
    }

    /// Return the text value of a tree-sitter `field('<name>', ...)`
    /// anonymous-token child on the given vertex, if any.
    ///
    /// Tree-sitter rules of the form
    /// `field('op', choice('+', '-', '*', '/'))` attach a field name to
    /// an unnamed token alternative. The walker emits the token's text
    /// as a `field:<name>` constraint on the parent vertex; this is the
    /// supported accessor for that text.
    ///
    /// Returns `None` if no `field:<name>` constraint exists on
    /// `vertex_id`. Named-node field children continue to surface as
    /// edges (use [`outgoing_edges`](Self::outgoing_edges) and filter
    /// by [`Edge::kind`](crate::Edge::kind) for those).
    #[must_use]
    pub fn field_text(&self, vertex_id: &str, field_name: &str) -> Option<&str> {
        let sort = format!("field:{field_name}");
        self.constraints
            .get(&Name::from(vertex_id))?
            .iter()
            .find(|c| c.sort.as_ref() == sort.as_str())
            .map(|c| c.value.as_str())
    }
}

/// Choose a single entry vertex for a schema.
///
/// Returns the first declared entry if any. Otherwise falls back to a
/// deterministic, protocol-agnostic choice: among vertex ids sorted
/// lexicographically, the first vertex that is a source of at least
/// one edge and a target of none (an edgeless "root" in the signature
/// graph); failing that, the first vertex that has outgoing edges at
/// all; failing that, the lexicographically first vertex; `None` only
/// for an empty schema.
///
/// The fallback is explicitly *non-canonical*: it exists so that legacy
/// schemas without declared entries remain usable, but new parsers
/// should always supply at least one entry via
/// [`SchemaBuilder::entry`](crate::SchemaBuilder::entry) so that the
/// pointing is part of the schema's semantics rather than recovered by
/// heuristic.
#[must_use]
pub fn primary_entry(schema: &Schema) -> Option<&Name> {
    if let Some(first) = schema.entries.first() {
        return Some(first);
    }

    let mut ids: Vec<&Name> = schema.vertices.keys().collect();
    ids.sort();

    let has_outgoing = |id: &Name| -> bool {
        schema
            .outgoing
            .get(id)
            .is_some_and(|edges| !edges.is_empty())
    };
    let has_incoming = |id: &Name| -> bool {
        schema
            .incoming
            .get(id)
            .is_some_and(|edges| !edges.is_empty())
    };

    if let Some(id) = ids
        .iter()
        .copied()
        .find(|id| has_outgoing(id) && !has_incoming(id))
    {
        return Some(id);
    }
    if let Some(id) = ids.iter().copied().find(|id| has_outgoing(id)) {
        return Some(id);
    }
    ids.into_iter().next()
}
