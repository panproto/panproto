//! Error types for full-AST parsing and emission.

use miette::Diagnostic;

/// Errors from full-AST parse and emit operations.
#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum ParseError {
    /// Tree-sitter failed to parse the source file.
    #[error("tree-sitter parse failed for {path}")]
    TreeSitterParse {
        /// The file path that failed to parse.
        path: String,
    },

    /// The source file's language could not be detected.
    #[error("unknown language for file extension: {extension}")]
    UnknownLanguage {
        /// The unrecognized file extension.
        extension: String,
    },

    /// Schema construction failed during AST walking.
    #[error("schema construction failed: {reason}")]
    SchemaConstruction {
        /// Description of the construction failure.
        reason: String,
    },

    /// Emission failed when converting a schema back to source text.
    #[error("emit failed for protocol {protocol}: {reason}")]
    EmitFailed {
        /// The protocol being emitted.
        protocol: String,
        /// Description of the emit failure.
        reason: String,
    },

    /// Theory extraction from grammar metadata failed.
    #[error("theory extraction failed: {reason}")]
    TheoryExtraction {
        /// Description of the extraction failure.
        reason: String,
    },

    /// JSON deserialization of node-types.json failed.
    #[error("failed to parse node-types.json: {source}")]
    NodeTypesJson {
        /// The underlying JSON parse error.
        #[source]
        source: serde_json::Error,
    },

    /// A protocol error propagated from panproto-protocols.
    #[error(transparent)]
    Protocol(#[from] panproto_protocols::ProtocolError),
}
