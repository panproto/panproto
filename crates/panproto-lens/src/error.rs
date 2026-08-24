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

    /// A symmetric lens was assembled from two legs whose source
    /// schemas are not the same schema, so the middle it claims to
    /// share does not exist.
    #[error("the two legs do not share a middle: {detail}")]
    NoSharedMiddle {
        /// How the two source schemas differ.
        detail: String,
    },

    /// A value-level transform of the second lens reads a field the first
    /// lens drops, so the composed expression has no binding for it.
    ///
    /// Caught when the lenses are composed rather than when the composite
    /// runs, where it would surface as an `UnboundVariable` far from its
    /// cause.
    #[error(
        "composition failed: transform on `{anchor}` reads field `{field}`, dropped by the first lens"
    )]
    ComposeUnboundField {
        /// The anchor vertex carrying the transform.
        anchor: String,
        /// The field the transform's expression reads.
        field: String,
    },

    /// A source was asked to be reconstructed from a view alone, but the
    /// lens is not an isomorphism, so its complement is not terminal and
    /// distinct sources share the given view.
    #[error("cannot reconstruct without a complement: {detail}")]
    NotAnIsomorphism {
        /// Which condition of invertibility fails.
        detail: String,
    },

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

    /// An edit could not be applied during law checking.
    #[error("edit apply error: {0}")]
    EditApply(#[from] panproto_inst::EditError),

    /// Partial-monoid `Complement::compose` rejected an attempted merge
    /// because both operands carried distinct entries on the same key.
    /// Composition is defined exactly when the two complements agree on
    /// every shared key (idempotent merge); disagreement is the
    /// boundary of the partial-monoid's domain of definition.
    #[error("complement conflict on {kind} key `{key}`")]
    ComplementConflict {
        /// Which keyed map produced the conflict.
        kind: &'static str,
        /// String representation of the conflicting key.
        key: String,
    },

    /// Partial-monoid `Complement::compose` rejected an attempted merge
    /// because the two complements were taken at distinct source
    /// schemas (their fingerprints differ).
    #[error(
        "complement fingerprint mismatch: {left:#x} vs {right:#x}; complements were captured against different source schemas"
    )]
    ComplementFingerprintMismatch {
        /// Left fingerprint.
        left: u64,
        /// Right fingerprint.
        right: u64,
    },

    /// No enrichment synthesis driver is registered for the named
    /// `(kind, enricher)` pair. The driver must be registered via
    /// [`enrichment_registry::register_enricher`](crate::enrichment_registry::register_enricher)
    /// before a protolens that adds this enrichment can be
    /// instantiated.
    #[error("no enrichment driver registered for ({kind:?}, {enricher:?})")]
    UnknownEnricher {
        /// The enrichment kind requested.
        kind: panproto_gat::EnrichmentKind,
        /// The enricher name (e.g. a grammar name for `Layout`).
        enricher: String,
    },

    /// The registered enrichment synthesis driver rejected its input.
    #[error("enrichment synthesis failed ({kind:?}, {enricher}): {detail}")]
    EnrichmentSynthesisFailed {
        /// The enrichment kind.
        kind: panproto_gat::EnrichmentKind,
        /// The enricher name.
        enricher: String,
        /// Human-readable failure.
        detail: String,
    },
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
