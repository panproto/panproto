//! Error types for instance parse and emit operations.

/// Errors that can occur when parsing raw format bytes into an instance.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseInstanceError {
    /// The input bytes could not be parsed as the expected format.
    #[error("parse error for protocol '{protocol}': {message}")]
    Parse {
        /// The protocol that was being parsed.
        protocol: String,
        /// A human-readable description of the parse failure.
        message: String,
    },

    /// The parsed data does not conform to the provided schema.
    #[error("schema mismatch for protocol '{protocol}': {message}")]
    SchemaMismatch {
        /// The protocol that was being parsed.
        protocol: String,
        /// A human-readable description of the mismatch.
        message: String,
    },

    /// The requested representation (`WType` or `Functor`) is not supported
    /// by this protocol's instance theory.
    #[error("protocol '{protocol}' does not support {requested:?} representation (native: {native:?})")]
    UnsupportedRepresentation {
        /// The protocol name.
        protocol: String,
        /// The representation that was requested.
        requested: super::NativeRepr,
        /// The protocol's native representation.
        native: super::NativeRepr,
    },

    /// The protocol is not registered in the registry.
    #[error("unknown protocol: '{0}'")]
    UnknownProtocol(String),

    /// An I/O error occurred while reading the input.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parsing failed.
    #[error("JSON parse error: {0}")]
    Json(String),
}

/// Errors that can occur when emitting an instance to raw format bytes.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EmitInstanceError {
    /// The instance could not be serialized to the target format.
    #[error("emit error for protocol '{protocol}': {message}")]
    Emit {
        /// The protocol that was being emitted.
        protocol: String,
        /// A human-readable description of the emit failure.
        message: String,
    },

    /// The requested representation (`WType` or `Functor`) is not supported
    /// by this protocol's instance theory.
    #[error("protocol '{protocol}' does not support {requested:?} representation (native: {native:?})")]
    UnsupportedRepresentation {
        /// The protocol name.
        protocol: String,
        /// The representation that was requested.
        requested: super::NativeRepr,
        /// The protocol's native representation.
        native: super::NativeRepr,
    },

    /// The protocol is not registered in the registry.
    #[error("unknown protocol: '{0}'")]
    UnknownProtocol(String),

    /// An I/O error occurred while writing the output.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization failed.
    #[error("JSON emit error: {0}")]
    Json(String),
}
