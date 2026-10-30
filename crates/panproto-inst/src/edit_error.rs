//! Error types for edit operations on instances.

use panproto_schema::Edge;
use serde::Serialize;

use crate::error_detail::ErrorDetail;
use crate::fan_violation::FanViolation;
use crate::ginstance::EdgeId;
use crate::graph_edit::{AttrKey, ComposeError, ElemRef};
use crate::view_ref::RowId;

/// Errors from applying edits to instances.
///
/// [`EditError::detail`] is the error's structured projection, the value
/// every binding receives in place of the message.
#[derive(Clone, Debug, PartialEq, Serialize, thiserror::Error)]
#[non_exhaustive]
pub enum EditError {
    /// A node ID was referenced but not found in the instance.
    #[error("node not found: {0}")]
    NodeNotFound(u32),

    /// The node's anchor does not match what the edit expected.
    #[error("anchor mismatch at node {node_id}: expected {expected}, got {actual}")]
    AnchorMismatch {
        /// The node ID.
        node_id: u32,
        /// The expected anchor.
        expected: String,
        /// The actual anchor.
        actual: String,
    },

    /// A fan (hyper-edge instance) was not found.
    #[error("fan not found for hyper-edge: {0}")]
    FanNotFound(String),

    /// General edit application failure.
    #[error("edit apply failed: {0}")]
    ApplyFailed(String),

    /// A field transform failed during edit application.
    #[error("field transform failed: {0}")]
    FieldTransformFailed(String),

    /// No row of a table instance has this id.
    #[error("row not found: {0:?}")]
    RowNotFound(RowId),

    /// The edit would create a cycle in a tree-shaped instance.
    #[error("cycle detected: moving node {node_id} under {new_parent} would create a cycle")]
    CycleDetected {
        /// The node being moved.
        node_id: u32,
        /// The proposed new parent.
        new_parent: u32,
    },

    /// The parent node does not exist for an insert operation.
    #[error("parent node not found: {0}")]
    ParentNotFound(u32),

    /// A child ID already exists in the instance.
    #[error("duplicate node ID: {0}")]
    DuplicateNodeId(u32),

    /// An edge id was referenced but not found in the instance.
    #[error("edge not found: {0}")]
    EdgeNotFound(EdgeId),

    /// An added edge's id is already in use.
    #[error("duplicate edge id: {0}")]
    DuplicateEdge(EdgeId),

    /// An added edge names an endpoint the result does not have.
    #[error("edge {edge} names the missing endpoint {node}")]
    MissingEndpoint {
        /// The added edge.
        edge: EdgeId,
        /// The missing node.
        node: u32,
    },

    /// A deleted node still has edges the delta does not delete.
    #[error("deleting node {node} would leave the edges {edges:?} dangling")]
    DanglingEdges {
        /// The deleted node.
        node: u32,
        /// Its incident edges the delta keeps, ascending.
        edges: Vec<EdgeId>,
    },

    /// A deleted node differs from the payload the delta expects.
    #[error("node {node} is not the node the edit deletes")]
    StaleNode {
        /// The node.
        node: u32,
    },

    /// A node's value differs from the old value the edit expects.
    #[error("the value of node {node} is not the old value the edit expects")]
    StaleValue {
        /// The node.
        node: u32,
    },

    /// A node's field differs from the old value the edit expects.
    #[error("field {field} of node {node} is not the old value the edit expects")]
    StaleField {
        /// The node.
        node: u32,
        /// The field.
        field: String,
    },

    /// A node's anchor differs from the old anchor the edit expects.
    #[error("the anchor of node {node} is not the old anchor the edit expects")]
    StaleAnchor {
        /// The node.
        node: u32,
    },

    /// A node's metadata differs from the old metadata the edit expects.
    #[error("the metadata of node {node} is not the old metadata the edit expects")]
    StaleMeta {
        /// The node.
        node: u32,
    },

    /// A deleted edge differs from the payload the delta expects.
    #[error("edge {edge} is not the edge the edit deletes")]
    StaleEdge {
        /// The edge.
        edge: EdgeId,
    },

    /// An edge's label differs from the old label the edit expects.
    #[error("the label of edge {edge} is not the old label the edit expects")]
    StaleLabel {
        /// The edge.
        edge: EdgeId,
    },

    /// An edge's position differs from the old position the edit expects.
    #[error("the position of edge {edge} is not the old position the edit expects")]
    StalePosition {
        /// The edge.
        edge: EdgeId,
    },

    /// The edit's result has a malformed hyper-edge fan.
    #[error("node {node} of the result is not a well-formed fan: {reason}")]
    MalformedFan {
        /// The fan node.
        node: u32,
        /// The condition it breaks.
        reason: FanViolation,
    },

    /// The steps of an edit do not compose into one delta.
    #[error("edit steps do not compose: {0}")]
    Compose(ComposeError),

    /// A step of a batch failed.
    #[error("batch step {index} failed: {source}")]
    BatchStep {
        /// The step's position in the batch.
        index: usize,
        /// The step's error.
        source: Box<Self>,
    },

    /// A graph delta has no tree edit of the same effect.
    #[error("not a tree edit: {reason}")]
    NotTreeEdit {
        /// Why.
        reason: String,
    },

    /// An added row's id is already in use.
    #[error("duplicate row id: {0:?}")]
    DuplicateRow(RowId),

    /// A cell differs from the old value the edit expects.
    #[error("column {column} of row {row:?} is not the old value the edit expects")]
    StaleCell {
        /// The row.
        row: RowId,
        /// The column.
        column: String,
    },

    /// A row's table differs from the old table the edit expects.
    #[error("the table of row {row:?} is not the old table the edit expects")]
    StaleTable {
        /// The row.
        row: RowId,
    },

    /// A deleted row's foreign-key pairs differ from those the edit expects.
    #[error("the foreign-key pairs of row {row:?} are not those the edit expects")]
    StaleIncidence {
        /// The row.
        row: RowId,
    },

    /// A deleted foreign-key pair does not exist.
    #[error("foreign key {edge:?} from row {src:?} to row {tgt:?} not found")]
    ForeignKeyNotFound {
        /// The foreign-key edge.
        edge: Edge,
        /// The source row.
        src: RowId,
        /// The target row.
        tgt: RowId,
    },

    /// An added foreign-key pair already exists.
    #[error("foreign key {edge:?} from row {src:?} to row {tgt:?} already exists")]
    DuplicateForeignKey {
        /// The foreign-key edge.
        edge: Edge,
        /// The source row.
        src: RowId,
        /// The target row.
        tgt: RowId,
    },

    /// An added foreign-key pair names a row the result does not have.
    #[error("foreign key {edge:?} names the missing row {row:?}")]
    MissingRowEndpoint {
        /// The foreign-key edge.
        edge: Edge,
        /// The missing row.
        row: RowId,
    },

    /// A deleted row still has foreign-key pairs the delta does not delete.
    #[error("deleting row {row:?} would leave the pairs {pairs:?} dangling")]
    DanglingForeignKeys {
        /// The deleted row.
        row: RowId,
        /// Its pairs the delta keeps, ascending.
        pairs: Vec<(Edge, RowId, RowId)>,
    },

    /// A graph delta has no table edit of the same effect.
    #[error("not a table edit: {reason}")]
    NotTableEdit {
        /// Why.
        reason: String,
    },

    /// A tree edit names the root in an operation the root cannot undergo:
    /// deleting, contracting or moving it (the root has no parent arc).
    #[error("the root node {0} cannot undergo this operation")]
    RootOperation(u32),

    /// A leaf-only deletion names a node that has children.
    #[error("node {0} is not a leaf; use ContractNode to remove it and reattach its children")]
    NotLeaf(u32),
}

impl From<ComposeError> for EditError {
    fn from(err: ComposeError) -> Self {
        Self::Compose(err)
    }
}

impl EditError {
    /// The structured detail of this error: an
    /// [`ErrorDetail::EditConflict`] whose `path` lists the batch step
    /// indices leading to the failing step (outermost first), whose `kind`
    /// is the snake-case name of the innermost error's variant (for a
    /// composition error, `compose_` and then the composition error's
    /// variant), and whose `node`, `edge`, `row` and `pair` name the element
    /// the error is about. A pair is `(edge, src, tgt)` with the foreign-key
    /// edge written as by [`edge_text`].
    ///
    /// ```
    /// use panproto_inst::{EditError, EdgeId, ErrorDetail};
    ///
    /// let err = EditError::BatchStep {
    ///     index: 2,
    ///     source: Box::new(EditError::EdgeNotFound(EdgeId(7))),
    /// };
    /// assert_eq!(
    ///     err.detail(),
    ///     ErrorDetail::EditConflict {
    ///         path: vec![2],
    ///         kind: "edge_not_found".into(),
    ///         node: None,
    ///         edge: Some(7),
    ///         row: None,
    ///         pair: None,
    ///     },
    /// );
    /// ```
    #[must_use]
    pub fn detail(&self) -> ErrorDetail {
        let mut path = Vec::new();
        let mut err = self;
        while let Self::BatchStep { index, source } = err {
            path.push(u32::try_from(*index).unwrap_or(u32::MAX));
            err = source;
        }
        let subject = err.subject();
        ErrorDetail::EditConflict {
            path,
            kind: err.kind(),
            node: subject.node,
            edge: subject.edge,
            row: subject.row,
            pair: subject.pair,
        }
    }

    /// The snake-case name of this error's variant.
    fn kind(&self) -> String {
        let name = match self {
            Self::NodeNotFound(_) => "node_not_found",
            Self::AnchorMismatch { .. } => "anchor_mismatch",
            Self::FanNotFound(_) => "fan_not_found",
            Self::ApplyFailed(_) => "apply_failed",
            Self::FieldTransformFailed(_) => "field_transform_failed",
            Self::RowNotFound(_) => "row_not_found",
            Self::CycleDetected { .. } => "cycle_detected",
            Self::ParentNotFound(_) => "parent_not_found",
            Self::DuplicateNodeId(_) => "duplicate_node_id",
            Self::EdgeNotFound(_) => "edge_not_found",
            Self::DuplicateEdge(_) => "duplicate_edge",
            Self::MissingEndpoint { .. } => "missing_endpoint",
            Self::DanglingEdges { .. } => "dangling_edges",
            Self::StaleNode { .. } => "stale_node",
            Self::StaleValue { .. } => "stale_value",
            Self::StaleField { .. } => "stale_field",
            Self::StaleAnchor { .. } => "stale_anchor",
            Self::StaleMeta { .. } => "stale_meta",
            Self::StaleEdge { .. } => "stale_edge",
            Self::StaleLabel { .. } => "stale_label",
            Self::StalePosition { .. } => "stale_position",
            Self::MalformedFan { .. } => "malformed_fan",
            Self::Compose(ComposeError::Mismatch { .. }) => "compose_mismatch",
            Self::BatchStep { .. } => "batch_step",
            Self::NotTreeEdit { .. } => "not_tree_edit",
            Self::DuplicateRow(_) => "duplicate_row",
            Self::StaleCell { .. } => "stale_cell",
            Self::StaleTable { .. } => "stale_table",
            Self::StaleIncidence { .. } => "stale_incidence",
            Self::ForeignKeyNotFound { .. } => "foreign_key_not_found",
            Self::DuplicateForeignKey { .. } => "duplicate_foreign_key",
            Self::MissingRowEndpoint { .. } => "missing_row_endpoint",
            Self::DanglingForeignKeys { .. } => "dangling_foreign_keys",
            Self::NotTableEdit { .. } => "not_table_edit",
            Self::RootOperation(_) => "root_operation",
            Self::NotLeaf(_) => "not_leaf",
        };
        name.to_owned()
    }

    /// The element this error is about.
    fn subject(&self) -> Subject {
        match self {
            Self::NodeNotFound(node)
            | Self::ParentNotFound(node)
            | Self::DuplicateNodeId(node)
            | Self::RootOperation(node)
            | Self::NotLeaf(node)
            | Self::AnchorMismatch { node_id: node, .. }
            | Self::CycleDetected { node_id: node, .. }
            | Self::DanglingEdges { node, .. }
            | Self::StaleNode { node }
            | Self::StaleValue { node }
            | Self::StaleField { node, .. }
            | Self::StaleAnchor { node }
            | Self::StaleMeta { node }
            | Self::MalformedFan { node, .. } => Subject::node(*node),
            Self::EdgeNotFound(edge)
            | Self::DuplicateEdge(edge)
            | Self::StaleEdge { edge }
            | Self::StaleLabel { edge }
            | Self::StalePosition { edge } => Subject::edge(*edge),
            Self::MissingEndpoint { edge, node } => Subject {
                node: Some(*node),
                edge: Some(edge.0),
                ..Subject::default()
            },
            Self::Compose(ComposeError::Mismatch { elem }) => Subject::of(elem),
            Self::BatchStep { source, .. } => source.subject(),
            Self::RowNotFound(row)
            | Self::DuplicateRow(row)
            | Self::StaleCell { row, .. }
            | Self::StaleTable { row }
            | Self::StaleIncidence { row }
            | Self::DanglingForeignKeys { row, .. }
            | Self::MissingRowEndpoint { row, .. } => Subject::row(*row),
            Self::ForeignKeyNotFound { edge, src, tgt }
            | Self::DuplicateForeignKey { edge, src, tgt } => Subject {
                pair: Some((edge_text(edge), src.0, tgt.0)),
                ..Subject::default()
            },
            Self::FanNotFound(_)
            | Self::ApplyFailed(_)
            | Self::FieldTransformFailed(_)
            | Self::NotTreeEdit { .. }
            | Self::NotTableEdit { .. } => Subject::default(),
        }
    }
}

/// The element fields of [`ErrorDetail::EditConflict`].
#[derive(Default)]
struct Subject {
    node: Option<u32>,
    edge: Option<u32>,
    row: Option<u32>,
    pair: Option<(String, u32, u32)>,
}

impl Subject {
    fn node(node: u32) -> Self {
        Self {
            node: Some(node),
            ..Self::default()
        }
    }

    fn edge(edge: EdgeId) -> Self {
        Self {
            edge: Some(edge.0),
            ..Self::default()
        }
    }

    fn row(row: RowId) -> Self {
        Self {
            row: Some(row.0),
            ..Self::default()
        }
    }

    fn of(elem: &ElemRef) -> Self {
        match elem {
            ElemRef::Node(node) => Self::node(*node),
            ElemRef::Edge(edge) => Self::edge(*edge),
            ElemRef::Row(row) => Self::row(*row),
            ElemRef::Pair { edge, src, tgt } => Self {
                pair: Some((edge_text(edge), src.0, tgt.0)),
                ..Self::default()
            },
            ElemRef::Attr(key) => match key {
                AttrKey::Value { node }
                | AttrKey::Field { node, .. }
                | AttrKey::Meta { node }
                | AttrKey::Anchor { node } => Self::node(*node),
                AttrKey::EdgeLabel { edge } | AttrKey::EdgePosition { edge } => Self::edge(*edge),
                AttrKey::Cell { row, .. } | AttrKey::Table { row } => Self::row(*row),
            },
        }
    }
}

/// A schema edge as text, distinct for distinct edges.
///
/// The text is `src`, `-`, `kind`, then `:name` when it has a name, `->`,
/// `tgt`; for instance `person-prop:name->string`. Each part writes each
/// `\`, `-`, `:` and `>` after a `\`, so `a\-b-k->c` is the edge from
/// `a-b` of kind `k`.
#[must_use]
pub fn edge_text(edge: &Edge) -> String {
    let mut text = escaped(&edge.src);
    text.push('-');
    text.push_str(&escaped(&edge.kind));
    if let Some(name) = &edge.name {
        text.push(':');
        text.push_str(&escaped(name));
    }
    text.push_str("->");
    text.push_str(&escaped(&edge.tgt));
    text
}

/// `part` with each `\`, `-`, `:` and `>` written after a `\`.
fn escaped(part: &str) -> String {
    let mut out = String::with_capacity(part.len());
    for c in part.chars() {
        if matches!(c, '\\' | '-' | ':' | '>') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
