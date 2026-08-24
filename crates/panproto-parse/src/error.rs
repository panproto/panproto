//! Error types for full-AST parsing and emission.

use miette::Diagnostic;

/// Errors from full-AST parse and emit operations.
#[non_exhaustive]
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

    /// The parse tree nests deeper than the walk is willing to descend.
    ///
    /// The walk is recursive, so unbounded nesting would exhaust the thread's
    /// stack and abort the process rather than fail. Raising
    /// [`WalkerConfig::max_depth`] admits deeper input.
    ///
    /// [`WalkerConfig::max_depth`]: crate::walker::WalkerConfig::max_depth
    #[error(
        "nesting exceeds the {limit}-level limit at byte {byte} (node kind `{kind}`); \
         raise the walker's max_depth to admit it"
    )]
    NestingTooDeep {
        /// The depth limit that was exceeded.
        limit: usize,
        /// The kind of the node that sat one level past the limit.
        kind: String,
        /// The byte offset at which that node begins.
        byte: usize,
    },

    /// A protocol error propagated from panproto-protocols.
    #[error(transparent)]
    Protocol(#[from] panproto_protocols::ProtocolError),

    /// A tags-query failed to compile against a grammar.
    ///
    /// Raised when `tree-sitter-tags` rejects the grammar's `queries/tags.scm`
    /// (or a project-level override) for structural reasons: malformed
    /// S-expression, unknown capture name outside the tags-query vocabulary,
    /// or regex syntax error in a `#strip!` predicate.
    #[error("scope-detection query failed to compile: {reason}")]
    ScopeQueryCompile {
        /// Underlying compiler error message.
        reason: String,
    },
}
