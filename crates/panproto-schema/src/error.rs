//! Error types for schema operations.

use std::fmt;

/// Errors that can occur during schema construction and validation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaError {
    /// A vertex ID was referenced but not found in the schema.
    #[error("vertex not found: {0}")]
    VertexNotFound(String),

    /// A duplicate vertex ID was added to the schema.
    #[error("duplicate vertex id: {0}")]
    DuplicateVertex(String),

    /// A duplicate edge was added to the schema.
    #[error("duplicate edge from {src} to {tgt} of kind {kind}")]
    DuplicateEdge {
        /// Source vertex ID.
        src: String,
        /// Target vertex ID.
        tgt: String,
        /// Edge kind.
        kind: String,
    },

    /// A duplicate hyper-edge ID was added to the schema.
    #[error("duplicate hyper-edge id: {0}")]
    DuplicateHyperEdge(String),

    /// An edge kind violates the protocol's edge rules.
    #[error(
        "invalid edge kind {kind}: source kind {src_kind} not allowed (permitted: {permitted})"
    )]
    InvalidEdgeSource {
        /// The edge kind.
        kind: String,
        /// The actual source vertex kind.
        src_kind: String,
        /// Comma-separated list of permitted source kinds.
        permitted: String,
    },

    /// An edge kind violates the protocol's edge rules (target).
    #[error(
        "invalid edge kind {kind}: target kind {tgt_kind} not allowed (permitted: {permitted})"
    )]
    InvalidEdgeTarget {
        /// The edge kind.
        kind: String,
        /// The actual target vertex kind.
        tgt_kind: String,
        /// Comma-separated list of permitted target kinds.
        permitted: String,
    },

    /// An edge kind is not recognized by the protocol.
    #[error("unknown edge kind: {0}")]
    UnknownEdgeKind(String),

    /// A vertex kind is not recognized by the protocol's schema theory.
    #[error("unknown vertex kind: {0}")]
    UnknownVertexKind(String),

    /// The schema has no vertices.
    #[error("schema has no vertices")]
    EmptySchema,

    /// A pushout overlap identified an edge that the schema it is drawn from
    /// does not contain, so the identification has no endpoints to close over.
    #[error(
        "overlap names a {side} edge from {src} to {tgt} of kind {kind} that is not in that schema"
    )]
    OverlapEdgeNotFound {
        /// Which schema the edge was drawn from: `left` or `right`.
        side: &'static str,
        /// Source vertex ID.
        src: String,
        /// Target vertex ID.
        tgt: String,
        /// Edge kind.
        kind: String,
    },

    /// A declared entry vertex does not exist in the schema.
    #[error("entry vertex not found: {0}")]
    UnknownEntryVertex(String),

    /// `SchemaBuilder::build_abstract` was called on a builder that
    /// has accumulated constraints in the layout enrichment fibre.
    /// Abstract schemas must carry no layout witnesses; call
    /// `build_decorated` if a decorated schema was intended.
    #[error(
        "build_abstract called on a builder with layout-fibre constraints; \
         use build_decorated for a decorated schema"
    )]
    LayoutConstraintsOnAbstractBuild,

    /// [`induce`](crate::induce()) produced a sub-schema that fails
    /// [`validate`](crate::validate) against the protocol it was cut from.
    ///
    /// Because induction never invents a vertex, an edge, a kind or a
    /// constraint, every finding here is inherited from the parent schema:
    /// either the parent was already invalid, or the cut exposed a
    /// requirement whose endpoints did not survive.
    #[error(
        "induced sub-schema is invalid: {}",
        .findings.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
    )]
    InducedSchemaInvalid {
        /// Every violation reported for the induced sub-schema.
        findings: Vec<ValidationError>,
    },
}

/// An error found during schema validation against a protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValidationError {
    /// An edge violates the protocol's edge rules.
    InvalidEdge {
        /// The offending edge's source vertex ID.
        src: String,
        /// The offending edge's target vertex ID.
        tgt: String,
        /// The edge kind.
        kind: String,
        /// Human-readable reason for the violation.
        reason: String,
    },

    /// A constraint uses a sort not recognized by the protocol.
    InvalidConstraintSort {
        /// The vertex with the invalid constraint.
        vertex: String,
        /// The unrecognized sort.
        sort: String,
    },

    /// A vertex kind is not recognized by the protocol.
    InvalidVertexKind {
        /// The vertex ID.
        vertex: String,
        /// The unrecognized kind.
        kind: String,
    },

    /// A required edge references a missing vertex.
    DanglingRequiredEdge {
        /// The vertex ID.
        vertex: String,
        /// The dangling edge description.
        edge: String,
    },

    /// A recursion point names a vertex the schema does not have.
    ///
    /// Either end can dangle: the marker itself, which is the key the point is
    /// filed under, or the vertex it unfolds to. Neither is caught anywhere
    /// else, and inducing a sub-schema silently drops a marker whose ends it
    /// cannot find, so without this check a schema carrying one validates
    /// clean and then loses the marker with no diagnostic.
    DanglingRecursionPoint {
        /// The marker vertex, which is the key in `recursion_points`.
        mu: String,
        /// Which end is missing, for the message.
        missing: String,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEdge {
                src,
                tgt,
                kind,
                reason,
            } => write!(f, "invalid edge {src} -> {tgt} ({kind}): {reason}"),
            Self::InvalidConstraintSort { vertex, sort } => {
                write!(f, "vertex {vertex} has invalid constraint sort: {sort}")
            }
            Self::InvalidVertexKind { vertex, kind } => {
                write!(f, "vertex {vertex} has invalid kind: {kind}")
            }
            Self::DanglingRequiredEdge { vertex, edge } => {
                write!(f, "vertex {vertex} has dangling required edge: {edge}")
            }
            Self::DanglingRecursionPoint { mu, missing } => {
                write!(
                    f,
                    "recursion point {mu} names a vertex the schema does not have: {missing}"
                )
            }
        }
    }
}
