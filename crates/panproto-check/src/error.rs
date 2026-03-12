//! Error types for the check crate.
//!
//! [`CheckError`] covers failures during schema diffing, classification,
//! and report generation.

/// Errors from breaking-change detection operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CheckError {
    /// The two schemas belong to different protocols.
    #[error("protocol mismatch: old={old}, new={new}")]
    ProtocolMismatch {
        /// The old schema's protocol.
        old: String,
        /// The new schema's protocol.
        new: String,
    },

    /// JSON serialization failed during report generation.
    #[error("report serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}
