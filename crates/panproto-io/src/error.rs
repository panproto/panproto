//! Error types for instance parse and emit operations.

use panproto_inst::{RowId, ValueFidelityError, ValueSite};
use serde::{Deserialize, Serialize};

/// Why a value cannot be represented by an output format.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum EmitValueReason {
    /// The format has no representation for this value type.
    #[error("the value is not representable")]
    NotRepresentable,
    /// The format cannot represent a non-finite floating-point value.
    #[error("the format cannot represent a non-finite number")]
    NonFinite,
    /// The format has no null representation at this site.
    #[error("the format cannot represent null")]
    NullNotRepresentable,
    /// The format has no representation for an absent value at this site.
    #[error("the format cannot represent absence")]
    AbsentNotRepresentable,
}

/// A value that cannot be emitted without losing its meaning.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
#[error("{value_type} at {site} cannot be emitted as {format}: {reason}")]
pub struct EmitValueError {
    /// The schema vertex and source offset, when known.
    pub site: ValueSite,
    /// The stable [`panproto_inst::Value::type_name`] of the value.
    pub value_type: String,
    /// The destination format.
    pub format: String,
    /// Why that format cannot carry the value.
    pub reason: EmitValueReason,
}

impl EmitValueError {
    /// Describe why `value_type` cannot be written at `site` in `format`.
    #[must_use]
    pub fn new(
        site: ValueSite,
        value_type: impl Into<String>,
        format: impl Into<String>,
        reason: EmitValueReason,
    ) -> Self {
        Self {
            site,
            value_type: value_type.into(),
            format: format.into(),
            reason,
        }
    }
}

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

    /// One or more scalar lexemes cannot be held exactly by their declared kinds.
    #[error("value fidelity errors: {0:?}")]
    Fidelity(Vec<ValueFidelityError>),

    /// A YAML tag was attached to a mapping or sequence whose tag semantics
    /// the instance model cannot represent.
    #[error(
        "protocol '{protocol}' cannot represent YAML tag {tag:?} on {container_kind} at byte {byte_offset:?}"
    )]
    UnsupportedTaggedContainer {
        /// The protocol that was being parsed.
        protocol: String,
        /// The source tag spelling.
        tag: String,
        /// The tagged YAML container kind.
        container_kind: String,
        /// Zero-based byte offset of the tag, when recorded by the CST.
        byte_offset: Option<u64>,
    },
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

    /// A value has no faithful representation in the output format.
    #[error(transparent)]
    Value(#[from] EmitValueError),

    /// A format-preserving writer cannot express a structural edit using the
    /// source document's fixed CST or byte layout.
    #[error("{format} format-preserving emission cannot represent structural edit: {detail}")]
    UnsupportedStructuralEdit {
        /// The output format whose preserved layout cannot carry the edit.
        format: String,
        /// The first unsupported structural difference found.
        detail: String,
    },

    /// A table emitter encountered a chase placeholder, which has no literal form.
    #[error("labeled null at row {row:?}, column {column:?} cannot be emitted")]
    LabeledNull {
        /// The row containing the placeholder.
        row: RowId,
        /// The column containing the placeholder.
        column: String,
    },
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

#[cfg(all(test, feature = "tree-sitter"))]
mod unified_codec_error_tests {
    use super::UnifiedCodecError;

    #[test]
    fn missing_grammar_message_names_format_and_feature() {
        let err = UnifiedCodecError::MissingGrammar {
            format: "json".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("json"), "format name in message: {msg:?}");
        assert!(
            msg.contains("panproto-grammars/lang-json"),
            "feature hint in message: {msg:?}"
        );
    }

    #[test]
    fn parser_init_carries_source_chain() {
        use std::error::Error as _;
        let inner = panproto_parse::error::ParseError::TheoryExtraction {
            reason: "synthetic test".to_owned(),
        };
        let err = UnifiedCodecError::ParserInit {
            format: "yaml".into(),
            source: Box::new(inner),
        };
        let msg = err.to_string();
        assert!(msg.contains("yaml"), "format name in message: {msg:?}");
        assert!(err.source().is_some(), "source chain must be exposed");
    }
}
