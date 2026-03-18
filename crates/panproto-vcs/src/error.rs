//! Error types for the VCS engine.

use std::fmt;

/// All errors produced by the VCS engine.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VcsError {
    /// An object was not found in the store.
    #[error("object not found: {id}")]
    ObjectNotFound {
        /// The missing object's ID.
        id: crate::ObjectId,
    },

    /// A ref was not found.
    #[error("ref not found: {name}")]
    RefNotFound {
        /// The missing ref name.
        name: String,
    },

    /// HEAD is detached when a branch was expected.
    #[error("HEAD is detached")]
    DetachedHead,

    /// Nothing is staged for commit.
    #[error("nothing staged")]
    NothingStaged,

    /// Staging validation failed.
    #[error("validation failed: {reasons:?}")]
    ValidationFailed {
        /// The validation errors.
        reasons: Vec<String>,
    },

    /// Merge produced conflicts.
    #[error("merge conflict: {count} conflict(s)")]
    MergeConflicts {
        /// The number of conflicts.
        count: usize,
    },

    /// A branch already exists.
    #[error("branch already exists: {name}")]
    BranchExists {
        /// The branch name.
        name: String,
    },

    /// Not inside a panproto repository.
    #[error("not a panproto repository")]
    NotARepository,

    /// An expected object had the wrong type.
    #[error("expected {expected} object, found {found}")]
    WrongObjectType {
        /// The expected object type.
        expected: &'static str,
        /// The actual object type.
        found: &'static str,
    },

    /// I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization / deserialization error.
    #[error("serialization error: {0}")]
    Serialization(SerializationError),

    /// Migration composition error.
    #[error("compose error: {0}")]
    Compose(#[from] panproto_mig::ComposeError),

    /// No common ancestor found for merge.
    #[error("no common ancestor found")]
    NoCommonAncestor,

    /// No path found between two commits.
    #[error("no path found between commits")]
    NoPath,

    /// Branch is not fully merged into HEAD.
    #[error("branch '{name}' is not fully merged")]
    BranchNotMerged {
        /// The branch name.
        name: String,
    },

    /// A merge or cherry-pick is already in progress.
    #[error("a {operation} is already in progress")]
    OperationInProgress {
        /// The operation type (e.g. "merge", "cherry-pick").
        operation: String,
    },

    /// Feature is not yet implemented.
    #[error("{feature} is not yet implemented")]
    NotImplemented {
        /// Description of the unimplemented feature.
        feature: String,
    },

    /// Merge cannot fast-forward but --ff-only was requested.
    #[error("cannot fast-forward; refusing to merge")]
    FastForwardOnly,

    /// Amend requested but no commits exist.
    #[error("nothing to amend")]
    NothingToAmend,

    /// A tag already exists.
    #[error("tag already exists: {name}")]
    TagExists {
        /// The tag name.
        name: String,
    },

    /// Data migration failed.
    #[error("data migration failed: {reason}")]
    DataMigrationFailed {
        /// Description of the failure.
        reason: String,
    },

    /// An object had the wrong type (owned variant for runtime strings).
    #[error("type mismatch: expected {expected}, got {got}")]
    TypeMismatch {
        /// The expected type name.
        expected: String,
        /// The actual type name.
        got: String,
    },

    /// I/O error from a string description (for cases where
    /// `std::io::Error` is not directly available).
    #[error("io: {0}")]
    IoError(String),
}

/// Wrapper for serialization errors from rmp-serde.
#[derive(Debug)]
pub struct SerializationError(pub String);

impl fmt::Display for SerializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<rmp_serde::encode::Error> for VcsError {
    fn from(e: rmp_serde::encode::Error) -> Self {
        Self::Serialization(SerializationError(e.to_string()))
    }
}

impl From<rmp_serde::decode::Error> for VcsError {
    fn from(e: rmp_serde::decode::Error) -> Self {
        Self::Serialization(SerializationError(e.to_string()))
    }
}
