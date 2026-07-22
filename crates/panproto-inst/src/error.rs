//! Error types for instance operations.

use std::fmt;

use panproto_gat::Name;

/// Errors from instance construction or manipulation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InstError {
    /// A node ID was referenced but not found.
    #[error("node not found: {0}")]
    NodeNotFound(u32),

    /// A vertex ID was referenced but not found in the schema.
    #[error("vertex not found in schema: {0}")]
    VertexNotFound(String),

    /// The root node is missing.
    #[error("root node missing from instance")]
    MissingRoot,

    /// An arc references a nonexistent node.
    #[error("dangling arc: ({src}, {tgt})")]
    DanglingArc {
        /// Source node ID.
        src: u32,
        /// Target node ID.
        tgt: u32,
    },

    /// Internal hom evaluation failed.
    #[error("eval_hom failed: {0}")]
    EvalHom(String),
}

/// Errors from the restrict operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RestrictError {
    /// No edge found between two vertices after ancestor contraction.
    #[error("no edge found between {src} and {tgt} in target schema")]
    NoEdgeFound {
        /// Source vertex anchor.
        src: String,
        /// Target vertex anchor.
        tgt: String,
    },

    /// Multiple edges found without a resolver entry.
    #[error("ambiguous edge between {src} and {tgt}: {count} candidates")]
    AmbiguousEdge {
        /// Source vertex anchor.
        src: String,
        /// Target vertex anchor.
        tgt: String,
        /// Number of candidate edges.
        count: usize,
    },

    /// The root was pruned during restriction.
    #[error("root node was pruned during restriction")]
    RootPruned,

    /// Fan reconstruction failed.
    #[error("fan reconstruction failed for hyper-edge {hyper_edge_id}: {detail}")]
    FanReconstructionFailed {
        /// The hyper-edge ID.
        hyper_edge_id: String,
        /// Details about the failure.
        detail: String,
    },

    /// Cartesian product exceeded the configured size limit.
    #[error("product size {actual} exceeds limit {limit} for vertex {vertex}")]
    ProductSizeExceeded {
        /// The target vertex whose fiber product is too large.
        vertex: String,
        /// The actual product size.
        actual: usize,
        /// The configured limit.
        limit: usize,
    },

    /// Multi-element fiber encountered where only single-element fibers
    /// are supported (e.g., W-type right Kan extension).
    #[error("multi-element fiber for vertex {vertex}: {count} source vertices")]
    MultiElementFiber {
        /// The target vertex with multiple preimages.
        vertex: String,
        /// Number of source vertices in the fiber.
        count: usize,
    },

    /// A source node's anchor is neither remapped by `vertex_remap` nor
    /// present in `surviving_verts`, so a *total* operation (left Kan
    /// extension `Sigma_F`, right Kan extension `Pi_F`) has no target
    /// vertex to map it to. These operations never drop data silently;
    /// they report the unmapped anchor instead.
    #[error("node {node_id} has anchor `{anchor}` that is neither remapped nor surviving")]
    UnmappedAnchor {
        /// The anchor that has no image in the target schema.
        anchor: Name,
        /// The offending source node ID.
        node_id: u32,
    },

    /// Two source vertices in a right Kan extension (`Pi_F`) fiber contribute
    /// the same column with conflicting values, so the product-row attribute
    /// merge would have to silently overwrite one. The merge rejects instead.
    #[error(
        "attribute collision on column `{column}` for vertex `{vertex}`: \
         fiber sources disagree on its value"
    )]
    AttributeCollision {
        /// The target vertex whose fiber product collides.
        vertex: String,
        /// The colliding column name.
        column: String,
    },

    /// A vertex map sends two or more distinct source vertices to a single
    /// target vertex. The W-type right Kan extension ([`wtype_pi`]) is only
    /// defined here for vertex-injective migrations, since a non-injective
    /// map would require constructing a product of subtrees.
    ///
    /// [`wtype_pi`]: crate::wtype_pi
    #[error(
        "non-injective vertex map: {} source vertices map to target `{target}`",
        sources.len()
    )]
    NonInjectiveVertexMap {
        /// The target vertex receiving multiple source vertices.
        target: Name,
        /// The source vertices in the fiber (sorted for determinism).
        sources: Vec<Name>,
    },

    /// A field transform's expression failed to evaluate.
    ///
    /// The transform is reported rather than skipped. A user-authored
    /// `apply_expr` or `compute_field` whose expression does not evaluate
    /// is a defect in the lens, and leaving the field untouched makes it
    /// indistinguishable from a transform that ran and changed nothing.
    #[error("field transform on `{key}` failed to evaluate: {source}")]
    FieldTransformFailed {
        /// The field the transform targets: the edited key for
        /// `ApplyExpr`, the target key for `ComputeField`, or a
        /// `<case branch N>` marker for a `Case` predicate, which has no
        /// single field of its own.
        key: String,
        /// The underlying evaluation failure.
        #[source]
        source: panproto_expr::ExprError,
    },
}

/// Errors from JSON parsing.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError {
    /// The root vertex was not found in the schema.
    #[error("root vertex not found in schema: {0}")]
    RootVertexNotFound(String),

    /// Expected a JSON object.
    #[error("expected JSON object at path {path}")]
    ExpectedObject {
        /// JSON path where the error occurred.
        path: String,
    },

    /// Expected a JSON array.
    #[error("expected JSON array at path {path}")]
    ExpectedArray {
        /// JSON path where the error occurred.
        path: String,
    },

    /// An edge references an unknown vertex kind.
    #[error("unknown edge target at path {path}: {detail}")]
    UnknownEdgeTarget {
        /// JSON path where the error occurred.
        path: String,
        /// Details.
        detail: String,
    },

    /// A value could not be parsed.
    #[error("invalid value at path {path}: {detail}")]
    InvalidValue {
        /// JSON path where the error occurred.
        path: String,
        /// Details.
        detail: String,
    },

    /// JSON structure error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// A validation error found when checking a W-type instance against a schema.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValidationError {
    /// A node's anchor vertex is not in the schema.
    InvalidAnchor {
        /// The offending node ID.
        node_id: u32,
        /// The anchor that was not found.
        anchor: String,
    },

    /// An arc's edge is not in the schema.
    InvalidEdge {
        /// Parent node ID.
        parent: u32,
        /// Child node ID.
        child: u32,
        /// Details.
        detail: String,
    },

    /// The root node is not present in the node set.
    MissingRoot,

    /// A child node is unreachable from the root.
    UnreachableNode {
        /// The unreachable node ID.
        node_id: u32,
    },

    /// A required edge is missing from a node.
    MissingRequiredEdge {
        /// The node ID.
        node_id: u32,
        /// Description of the missing edge.
        edge: String,
    },

    /// Parent map inconsistency.
    ParentMapInconsistent {
        /// The node with the inconsistency.
        node_id: u32,
        /// Details.
        detail: String,
    },

    /// Fan references a nonexistent node.
    InvalidFan {
        /// The hyper-edge ID.
        hyper_edge_id: String,
        /// Details.
        detail: String,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAnchor { node_id, anchor } => {
                write!(f, "node {node_id} has invalid anchor: {anchor}")
            }
            Self::InvalidEdge {
                parent,
                child,
                detail,
            } => write!(f, "invalid edge ({parent}, {child}): {detail}"),
            Self::MissingRoot => write!(f, "root node is missing"),
            Self::UnreachableNode { node_id } => {
                write!(f, "node {node_id} is unreachable from root")
            }
            Self::MissingRequiredEdge { node_id, edge } => {
                write!(f, "node {node_id} missing required edge: {edge}")
            }
            Self::ParentMapInconsistent { node_id, detail } => {
                write!(f, "parent map inconsistency at node {node_id}: {detail}")
            }
            Self::InvalidFan {
                hyper_edge_id,
                detail,
            } => write!(f, "invalid fan for hyper-edge {hyper_edge_id}: {detail}"),
        }
    }
}
