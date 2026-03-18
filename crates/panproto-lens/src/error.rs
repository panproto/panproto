//! Error types for lens operations.
//!
//! [`LensError`] covers failures during lens construction and combinator
//! compilation. [`LawViolation`] reports failures of the round-trip laws
//! (`GetPut` and `PutGet`).

use std::fmt;

/// Errors from lens construction, combinator compilation, or lens operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LensError {
    /// A schema vertex referenced by a combinator was not found.
    #[error("vertex not found: {0}")]
    VertexNotFound(String),

    /// An edge referenced by a combinator was not found.
    #[error("edge not found: {src} -> {tgt}")]
    EdgeNotFound {
        /// Source vertex.
        src: String,
        /// Target vertex.
        tgt: String,
    },

    /// A combinator references an invalid field name.
    #[error("field not found: {0}")]
    FieldNotFound(String),

    /// A combinator references an NSID on a vertex that has no NSID.
    #[error("no NSID on vertex: {0}")]
    NsidNotFound(String),

    /// A combinator references a constraint sort that does not exist.
    #[error("constraint sort not found: {0}")]
    ConstraintSortNotFound(String),

    /// A combinator references an edge kind that does not exist.
    #[error("edge kind not found: {0}")]
    EdgeKindNotFound(String),

    /// Type coercion between incompatible kinds.
    #[error("cannot coerce from {from} to {to}")]
    IncompatibleCoercion {
        /// Source kind.
        from: String,
        /// Target kind.
        to: String,
    },

    /// Lens composition failed because schemas don't align.
    #[error(
        "composition failed: target schema of first lens does not match source schema of second"
    )]
    CompositionMismatch,

    /// Delegation to the restrict pipeline failed.
    #[error("restrict error: {0}")]
    Restrict(#[from] panproto_inst::RestrictError),

    /// A complement was incompatible with the view during `put`.
    #[error("complement mismatch: {detail}")]
    ComplementMismatch {
        /// Details about the mismatch.
        detail: String,
    },

    /// A protolens operation failed.
    #[error("protolens error: {0}")]
    ProtolensError(String),
}

/// A violation of a round-trip lens law.
#[derive(Debug)]
#[non_exhaustive]
pub enum LawViolation {
    /// `GetPut` law violation: `put(s, get(s)) != s`.
    GetPut {
        /// Human-readable description of the difference.
        detail: String,
    },

    /// `PutGet` law violation: `get(put(s, v)) != v`.
    PutGet {
        /// Human-readable description of the difference.
        detail: String,
    },

    /// An error occurred while checking the laws.
    Error(LensError),
}

impl fmt::Display for LawViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GetPut { detail } => write!(f, "GetPut law violated: {detail}"),
            Self::PutGet { detail } => write!(f, "PutGet law violated: {detail}"),
            Self::Error(e) => write!(f, "error during law check: {e}"),
        }
    }
}

impl std::error::Error for LawViolation {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Error(e) => Some(e),
            _ => None,
        }
    }
}
