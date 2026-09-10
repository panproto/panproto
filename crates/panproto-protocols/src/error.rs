//! Error types for protocol operations.

/// Errors from protocol parsing or definition.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProtocolError {
    /// A theory colimit failed during protocol construction.
    #[error("theory colimit failed: {0}")]
    ColimitFailed(#[from] panproto_gat::GatError),

    /// A bounded resource ran out while reading or building a schema.
    ///
    /// Names which allowance was exhausted and what it was set to, so
    /// raising it is a matter of reading the error rather than guessing
    /// which of several unrelated bounds was reached.
    #[error(transparent)]
    LimitExceeded(#[from] panproto_expr::limits::LimitExceeded),

    /// Composing a protocol's theories failed, so the registry entry it
    /// would have produced could not be built.
    ///
    /// Distinct from [`ProtocolError::ColimitFailed`] in naming the
    /// composition stage: a protocol's theory set is built by several
    /// pushouts in sequence, and which one failed is what identifies
    /// the theory that is missing.
    #[error("theory registration failed while composing {stage}: {source}")]
    TheoryRegistration {
        /// The composition stage that failed.
        stage: String,
        /// The failure that stage reported.
        ///
        /// Boxed so this variant does not enlarge `ProtocolError`.
        /// `ColimitFailed` already holds a `GatError` inline, so
        /// carrying one plus a `String` here would make this the
        /// largest variant and push every `Result` wrapping a
        /// `ProtocolError` over clippy's `result_large_err` threshold,
        /// several crates downstream.
        #[source]
        source: Box<panproto_gat::GatError>,
    },

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

    /// Emit/serialization failed.
    #[error("emit error: {0}")]
    Emit(String),

    /// The schema does not match the expected protocol structure.
    #[error("protocol mismatch: expected {expected}, got vertex kinds {actual}")]
    ProtocolMismatch {
        /// Expected protocol name.
        expected: String,
        /// Actual vertex kinds found.
        actual: String,
    },
}
