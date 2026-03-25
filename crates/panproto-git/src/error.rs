//! Error types for git bridge operations.

use miette::Diagnostic;

/// Errors from git ↔ panproto-vcs translation.
#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum GitBridgeError {
    /// A git operation failed.
    #[error("git error: {0}")]
    Git(#[from] git2::Error),

    /// A panproto-vcs operation failed.
    #[error("vcs error: {0}")]
    Vcs(#[from] panproto_vcs::VcsError),

    /// A project assembly operation failed.
    #[error("project error: {0}")]
    Project(#[from] panproto_project::ProjectError),

    /// A parse operation failed.
    #[error("parse error: {0}")]
    Parse(#[from] panproto_parse::ParseError),

    /// The git repository has no commits.
    #[error("repository has no commits")]
    EmptyRepository,

    /// A git object could not be read.
    #[error("failed to read git object {oid}: {reason}")]
    ObjectRead {
        /// The git object ID.
        oid: String,
        /// The reason the read failed.
        reason: String,
    },

    /// A file in the git tree could not be decoded as UTF-8.
    #[error("file {path} is not valid UTF-8")]
    NotUtf8 {
        /// The file path.
        path: String,
    },
}
