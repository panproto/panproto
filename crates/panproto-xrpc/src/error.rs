//! Error types for XRPC client operations.

use miette::Diagnostic;

/// Errors from XRPC client operations against a cospan node.
#[non_exhaustive]
#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum XrpcError {
    /// HTTP request failed.
    #[error("XRPC request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// The node returned a non-success status code.
    #[error("XRPC {endpoint} returned {status}: {body}")]
    NodeError {
        /// The XRPC endpoint that was called.
        endpoint: String,
        /// The HTTP status code.
        status: u16,
        /// The response body.
        body: String,
    },

    /// `MessagePack` deserialization failed.
    #[error("msgpack decode failed: {0}")]
    MsgpackDecode(#[from] rmp_serde::decode::Error),

    /// `MessagePack` serialization failed.
    #[error("msgpack encode failed: {0}")]
    MsgpackEncode(#[from] rmp_serde::encode::Error),

    /// JSON deserialization failed.
    #[error("JSON decode failed: {0}")]
    JsonDecode(#[from] serde_json::Error),

    /// A VCS operation failed.
    #[error("VCS error: {0}")]
    Vcs(#[from] panproto_vcs::VcsError),

    /// The node URL is malformed.
    #[error("invalid node URL: {0}")]
    InvalidUrl(String),

    /// Authentication failed or token missing.
    #[error("authentication required: {0}")]
    AuthRequired(String),
}
