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
    #[error(
        "protocol '{protocol}' does not support {requested:?} representation (native: {native:?})"
    )]
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
    #[error(
        "protocol '{protocol}' does not support {requested:?} representation (native: {native:?})"
    )]
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

/// Errors that can occur when constructing a [`UnifiedCodec`](super::unified_codec::UnifiedCodec).
///
/// Returned by `UnifiedCodec::new` and the format-specific convenience
/// constructors (`json`, `xml`, `yaml`, `toml`, `csv`, `tsv`) when the
/// requested tree-sitter grammar is not available at runtime or when
/// the grammar's parser-init machinery rejects it.
#[cfg(feature = "tree-sitter")]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UnifiedCodecError {
    /// The tree-sitter grammar for the requested format is not compiled in.
    ///
    /// Enable the corresponding `panproto-grammars/lang-<format>` feature
    /// (transitively via `panproto-io/tree-sitter`) to make the codec
    /// available.
    #[error(
        "tree-sitter grammar '{format}' not available; \
         enable panproto-grammars/lang-{format}"
    )]
    MissingGrammar {
        /// Grammar / format name (`json`, `xml`, `yaml`, `toml`, `csv`, `tsv`).
        format: String,
    },

    /// The grammar is compiled in but its parser failed to initialize.
    ///
    /// Typical causes: malformed bundled `node-types.json`, a `tags.scm`
    /// query that fails to compile against the grammar, or a tree-sitter
    /// version mismatch between the compiled grammar and the runtime.
    #[error("failed to initialize grammar '{format}': {source}")]
    ParserInit {
        /// Grammar / format name.
        format: String,
        /// Underlying parse-error from `LanguageParser::from_language`.
        #[source]
        source: Box<panproto_parse::error::ParseError>,
    },
}
