//! Error types for edit operations on instances.

/// Errors from applying edits to instances.
#[derive(Debug, thiserror::Error)]
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

    /// A table was not found in a functor instance.
    #[error("table not found: {0}")]
    TableNotFound(String),

    /// A row was not found in a functor instance table.
    #[error("row not found in table {table}: key {key}")]
    RowNotFound {
        /// The table name.
        table: String,
        /// The row key that was not found.
        key: String,
    },

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
}
