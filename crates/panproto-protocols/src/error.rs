//! Error types for protocol operations.

/// Errors from protocol parsing or definition.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProtocolError {
    /// A theory colimit failed during protocol construction.
    #[error("theory colimit failed: {0}")]
    ColimitFailed(#[from] panproto_gat::GatError),

    /// A schema building step failed.
    #[error("schema build failed: {0}")]
    SchemaBuild(#[from] panproto_schema::SchemaError),

    /// JSON parsing failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The input format was invalid or unsupported.
    #[error("parse error: {0}")]
    Parse(String),

    /// A required field is missing in the input.
    #[error("missing field: {0}")]
    MissingField(String),

    /// The input references an unknown type or definition.
    #[error("unknown reference: {0}")]
    UnknownRef(String),
}
