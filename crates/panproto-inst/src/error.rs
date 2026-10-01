//! Error types for instance operations.

use panproto_gat::Name;
use panproto_schema::Edge;

use crate::fan_violation::FanViolation;
use crate::ginstance::{EdgeId, NotTableReason, NotTreeReason};
use crate::value::ValueFidelityError;
use crate::view_ref::RowId;

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

    /// A graph edge names a node the instance does not hold.
    #[error("edge {edge} names node {endpoint}, which is not in the instance")]
    DanglingEdge {
        /// The edge.
        edge: EdgeId,
        /// The missing endpoint.
        endpoint: u32,
    },

    /// A graph node is filed under an id other than its own.
    #[error("node {node_id} is filed under {key}")]
    NodeIdMismatch {
        /// The key the node is filed under.
        key: u32,
        /// The id the node carries.
        node_id: u32,
    },

    /// A node with this id is already in the instance.
    #[error("node {0} is already in the instance")]
    DuplicateNode(u32),

    /// An edge with this id is already in the instance.
    #[error("edge {0} is already in the instance")]
    DuplicateEdge(EdgeId),

    /// No edge has this id.
    #[error("edge not found: {0}")]
    EdgeNotFound(EdgeId),

    /// A node cannot be removed while edges touch it.
    #[error("node {node} still has incident edges {edges:?}")]
    IncidentEdges {
        /// The node.
        node: u32,
        /// The edges touching it, ascending.
        edges: Vec<EdgeId>,
    },

    /// Every edge id up to `u32::MAX` is in use, so no fresh one exists.
    #[error("no edge id is free above the largest in use")]
    EdgeIdsExhausted,

    /// Every node id up to `u32::MAX` is in use, so no fresh one exists.
    #[error("no node id is free above the largest in use")]
    NodeIdsExhausted,

    /// A graph instance is not a tree (rooted at the requested node, where
    /// one is requested), so it has no tree form.
    #[error("not a tree: {reason}")]
    NotATree {
        /// Why the graph is not a tree.
        reason: NotTreeReason,
    },

    /// A foreign-key pair names a row that is not in the endpoint table of
    /// its edge.
    #[error("foreign key {edge:?} names row {}, which its endpoint table does not hold", .row.0)]
    DanglingForeignKey {
        /// The foreign-key edge.
        edge: Edge,
        /// The row the edge's endpoint table does not hold.
        row: RowId,
    },

    /// A node-link document does not have the node-link shape.
    #[error("malformed node-link document: {0}")]
    NodeLink(String),

    /// A graph instance holds reserved fan vocabulary outside a well-formed
    /// fan: a node of the fan shape, or the source of an edge of a reserved
    /// fan kind, that is not a well-formed fan node.
    #[error("node {node} is not a well-formed fan: {reason}")]
    MalformedFan {
        /// The offending node.
        node: u32,
        /// The condition it breaks.
        reason: FanViolation,
    },

    /// A row id appears in more than one table of a table instance.
    #[error("row {} appears in tables {tables:?}", .row.0)]
    DuplicateRowId {
        /// The row id.
        row: RowId,
        /// The tables holding it, ascending.
        tables: Vec<Name>,
    },

    /// A row map sends a row to a row of another table.
    #[error("row {} is mapped into a table other than its own", .row.0)]
    HomTableMismatch {
        /// The domain row.
        row: RowId,
    },

    /// A foreign-key pair is listed twice (a foreign key is a set of pairs).
    #[error("foreign key {edge:?} lists the pair ({}, {}) twice", .src.0, .tgt.0)]
    DuplicateForeignKey {
        /// The foreign-key edge.
        edge: Edge,
        /// The source row.
        src: RowId,
        /// The target row.
        tgt: RowId,
    },

    /// A graph instance is not the graph form of a table instance, so it has
    /// no table form.
    #[error("not a table: {reason}")]
    NotATable {
        /// Why the graph is not a table.
        reason: NotTableReason,
    },

    /// A row cannot move to another table while it takes part in a
    /// foreign-key pair, since the pair would no longer join its edge's
    /// endpoint tables.
    #[error(
        "row {} cannot move from table \"{from}\" to table \"{to}\" while it takes part in the pair ({}, {}) of foreign key {edge:?}",
        .row.0, .src.0, .tgt.0
    )]
    RowHasForeignKeys {
        /// The row asked to move.
        row: RowId,
        /// The table holding the row.
        from: Name,
        /// The table the row was asked to move to.
        to: Name,
        /// The foreign-key edge of the first pair the row takes part in.
        edge: Edge,
        /// The source row of that pair.
        src: RowId,
        /// The target row of that pair.
        tgt: RowId,
    },

    /// A tree holds reserved fan vocabulary: a node of the fan shape, or an
    /// arc of a reserved fan kind. A tree carries its fans as
    /// [`Fan`](crate::Fan) records, never as nodes or arcs.
    #[error("reserved fan vocabulary at {at:?}")]
    ReservedVocabulary {
        /// The offending node (`ViewRef::Node`) or arc (`ViewRef::Arc`).
        at: crate::ViewRef,
    },
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

    /// An operation that creates rows found no row id free above the
    /// largest in use.
    #[error("no row id is free above the largest in use")]
    RowIdsExhausted,

    /// A table instance holds rows at a vertex outside the migration's
    /// domain that is the image of another vertex: the migration names it as
    /// a target only, so a total operation has nowhere to send its rows and
    /// refuses rather than dropping them or merging them unasked.
    #[error("vertex `{vertex}` holds rows but is a target of the migration, not a source")]
    VertexOutsideDomain {
        /// The vertex whose rows have no image.
        vertex: Name,
    },

    /// A table instance holds foreign-key pairs on an edge that has no image
    /// of its own under the migration: either an edge the migration names
    /// only as the image of another edge, or an edge it does not mention
    /// whose relabelling through the vertex map lands on an edge some other
    /// edge already reaches. A total operation has nowhere to send these
    /// pairs and refuses rather than dropping them or merging them unasked.
    #[error("edge {edge:?} holds pairs but has no image of its own under the migration")]
    EdgeOutsideDomain {
        /// The edge whose pairs have no image.
        edge: Edge,
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

    /// Values whose declared scalar kind cannot hold their lexeme exactly,
    /// every one in the document, in document order.
    #[error("{} value(s) cannot be read exactly; the first: {}", .0.len(), .0.first().map_or_else(String::new, ToString::to_string))]
    Fidelity(Vec<ValueFidelityError>),
}
