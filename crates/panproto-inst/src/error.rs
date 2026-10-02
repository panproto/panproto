//! Error types for instance operations.

use panproto_gat::Name;
use panproto_schema::Edge;

use crate::fan_violation::FanViolation;
use crate::ginstance::{EdgeId, NotTableReason, NotTreeReason};
use crate::restrict_boundary::BoundarySide;
use crate::restrict_plan::SurvivalElement;
use crate::segment::{ErrorKey, IdKind};
use crate::validation::ValidationReport;
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

    /// The migration keeps a foreign key but names an image that is not a
    /// foreign key of the target running between the images of its
    /// endpoints, so it states no functor between the relation-ified
    /// schemas.
    #[error(
        "the migration carries foreign key {source_edge:?} to {image:?}, \
         which is not a foreign key between the images of its endpoints"
    )]
    IncoherentEdgeImage {
        /// The source foreign key (boxed, as the image is, so that the
        /// error stays within the size `clippy::result_large_err` allows).
        source_edge: Box<Edge>,
        /// The image the migration names for it.
        image: Box<Edge>,
    },

    /// The migration keeps a source table but sends it to a vertex that is
    /// not a table of the target.
    #[error(
        "the migration sends table `{vertex}` to `{image}`, which is not a table of the target"
    )]
    VertexImageNotATable {
        /// The source table.
        vertex: Name,
        /// The image the migration names for it.
        image: Name,
    },

    /// The migration keeps a column (an attribute arrow of a kept table) but
    /// names no attribute arrow of the target as its image: the image it
    /// names is no attribute arrow, leaves another table than the image of
    /// the column's table, or ends elsewhere than the image of the column's
    /// vertex, or a surviving column has no attribute arrow of its kind and
    /// name in the target.
    #[error(
        "the migration carries column {edge:?} to {image:?}, \
         which is no column of the image of its table"
    )]
    IncoherentAttributeImage {
        /// The source attribute arrow (boxed, as the image is, so that the
        /// error stays within the size `clippy::result_large_err` allows).
        edge: Box<Edge>,
        /// The image the migration names for it, or `None` when a
        /// surviving column has none in the target.
        image: Option<Box<Edge>>,
    },

    /// A column of a target table is the image of no kept column of its
    /// fibre, so the right Kan extension would have to range over every
    /// value there and has no result.
    #[error("column `{column}` of target table `{table}` is the image of no source column")]
    AttributeOutsideImage {
        /// The least such table.
        table: Name,
        /// The least such column of it.
        column: String,
    },

    /// The migration sends an edge inside a column's value to an edge that
    /// is no edge inside a target column's value, or renames it, changes its
    /// kind, or changes the kind of an endpoint, so it would change the
    /// structure of the values cells hold.
    #[error("the migration changes the structure of the values inside cells at edge {edge:?}")]
    ValueStructureRemap {
        /// The source edge inside a value.
        edge: Edge,
    },

    /// A schema the migration runs between has no attributed relation-ified
    /// form under its protocol.
    #[error("the schema has no relation-ified form: {0}")]
    RelSchema(#[from] crate::rel_schema::RelSchemaError),

    /// Validation at the boundary of a schema-taking restrict or extend did
    /// not pass: the input under the source schema, or the output under the
    /// target schema.
    #[error("the {side} of the restrict did not validate: status {:?}", .report.status)]
    Validation {
        /// The side whose report did not pass.
        side: BoundarySide,
        /// The report.
        report: Box<ValidationReport>,
    },

    /// An expression at a node reads a child name that two or more of the
    /// node's out-edges carry, so the name binds a list and the expression
    /// cannot be read as asking for one value.
    #[error("node {node} has several children named `{field}`, and an expression reads the name")]
    AmbiguousChildField {
        /// The node the expression is evaluated at.
        node: u32,
        /// The repeated child name.
        field: String,
    },

    /// The migration needs a construction a table instance cannot hold: an
    /// expansion of an edge or of a contracted pair into a path, or a
    /// hyper-edge resolution.
    #[error("the migration is not representable on tables: {reason}")]
    NotTableRepresentable {
        /// What the migration needs that tables lack.
        reason: String,
    },

    /// An expression at a node reads a field whose value has no expression
    /// literal, so it cannot be evaluated without inventing a value.
    #[error("field `{field}` of node {node} holds a {value_type}, which no expression can read")]
    ValueNotExpressible {
        /// The node the expression is evaluated at.
        node: u32,
        /// The field read.
        field: String,
        /// The type name of the value that has no literal.
        value_type: String,
    },

    /// An error of the graph engine, with the element it arose at and its
    /// position in the engine's precedence order. The message is the inner
    /// error's; the inner error is never itself located.
    #[error("{error}")]
    At {
        /// The error's position in the precedence order.
        key: ErrorKey,
        /// The error.
        error: Box<Self>,
    },

    /// The migration keeps or drops a table, relation or column of a table
    /// instance differently under the survival rule of the restrict and
    /// under the table functor of the migration, so the table restrict is
    /// not defined on it.
    #[error("the restrict and the table functor of the migration disagree on {element}")]
    SurvivalRulesDisagree {
        /// The first element on which they disagree.
        element: Box<SurvivalElement>,
    },

    /// The table engine was handed a plan compiled without the table
    /// functor of the migration.
    #[error("the table restrict needs a plan compiled for tables")]
    TablePlanRequired,

    /// An expression at a node reads a child whose target holds no value of
    /// its own and whose subgraph is not a tree, so the child has no
    /// structured value.
    #[error(
        "child `{field}` of node {node} is read as a structure, but its subgraph is not a tree"
    )]
    StructuralChildNotATree {
        /// The node the expression is evaluated at.
        node: u32,
        /// The child name read.
        field: String,
    },

    /// The migration resolves a contracted pair of source vertices to an
    /// edge that does not join the images of the pair, or that the target
    /// schema does not declare.
    #[error(
        "the contracted pair {pair:?} is resolved to {image:?}, \
         which does not join it in the target"
    )]
    IncoherentPairImage {
        /// The images `(a', b')` of the contracted pair.
        pair: Box<(Name, Name)>,
        /// The incoherent resolution (a label of an expansion, or the
        /// resolved edge).
        image: Box<Edge>,
    },

    /// A value a transform wrote to a column of the node's target table
    /// cannot be held exactly by that column's declared kind.
    #[error("node {node}: {error}")]
    ValueFidelity {
        /// The node whose transform wrote the value.
        node: u32,
        /// The fidelity failure.
        error: Box<ValueFidelityError>,
    },

    /// No id below `u32::MAX` at or above the floor is free for a
    /// synthesized node or edge.
    #[error("no {kind} id is free for a synthesized element")]
    IdsExhausted {
        /// Which ids ran out.
        kind: IdKind,
    },

    /// A survival predicate at a node failed to evaluate, or evaluated to
    /// something other than a boolean (reported as a type error expecting
    /// `bool`).
    #[error("the survival predicate of node {node} did not evaluate to a boolean: {source}")]
    SurvivalPredicateFailed {
        /// The node.
        node: u32,
        /// The evaluation failure.
        #[source]
        source: panproto_expr::ExprError,
    },

    /// A plan handed to a validated restrict or extend was compiled for a
    /// schema other than the one a validation plan was compiled for.
    #[error("the {side} validation plan was compiled for another schema than the restrict plan")]
    PlanSchemaMismatch {
        /// The side whose validation plan does not match.
        side: BoundarySide,
    },

    /// An expansion has more hops than a synthesized key can number: the
    /// key numbers them with 16 bits, so a path takes at most 65536 edges.
    #[error(
        "the expansion from `{src}` to `{tgt}` has {hops} hops, \
         more than the 65536 a synthesized key can number"
    )]
    ExpansionTooLong {
        /// The anchor the path leaves.
        src: String,
        /// The anchor the path enters.
        tgt: String,
        /// The number of edges on the path.
        hops: usize,
    },

    /// A row of a table instance does not conform to the source schema of
    /// a table restrict: it lies at no table, or its cells are not exactly
    /// its table's columns. The table restrict reads total rows only, so
    /// that the complement can name every column on which a view row and
    /// its source row disagree.
    #[error(
        "row {} does not conform to the source schema{}: {reason}",
        .row.0,
        at_column(.column.as_deref())
    )]
    NonConforming {
        /// The least offending row.
        row: RowId,
        /// The offending column, when the violation is at a cell.
        column: Option<String>,
        /// What is wrong: [`NonConformance::TableOutside`],
        /// [`NonConformance::MissingCell`] or
        /// [`NonConformance::StrayCell`].
        ///
        /// [`NonConformance::TableOutside`]: crate::NonConformance::TableOutside
        /// [`NonConformance::MissingCell`]: crate::NonConformance::MissingCell
        /// [`NonConformance::StrayCell`]: crate::NonConformance::StrayCell
        reason: crate::NonConformance,
    },
}

/// The place of a non-conforming row's violation in its message: ` at
/// column` and the column's name, or nothing for a violation at the row.
fn at_column(column: Option<&str>) -> String {
    column.map_or_else(String::new, |column| format!(" at column `{column}`"))
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
