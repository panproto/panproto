//! Error types for project assembly operations.

use miette::Diagnostic;

/// Errors from multi-file project assembly.
#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum ProjectError {
    /// A file could not be parsed.
    #[error("failed to parse {path}: {reason}")]
    ParseFailed {
        /// The file path that failed.
        path: String,
        /// The reason parsing failed.
        reason: String,
    },

    /// Language detection failed for a file.
    #[error("unknown language for {path}")]
    UnknownLanguage {
        /// The file path with unrecognized language.
        path: String,
    },

    /// Schema coproduct construction failed.
    #[error("coproduct construction failed: {reason}")]
    CoproductFailed {
        /// Description of the construction failure.
        reason: String,
    },

    /// A cross-file import could not be resolved.
    #[error("unresolved import in {source_file}: {import_target}")]
    UnresolvedImport {
        /// The file containing the unresolved import.
        source_file: String,
        /// The target that could not be resolved.
        import_target: String,
    },

    /// An I/O error occurred reading files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A parse error propagated from panproto-parse.
    #[error(transparent)]
    Parse(#[from] panproto_parse::ParseError),
}
