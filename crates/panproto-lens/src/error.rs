//! Error types for lens operations.
//!
//! [`LensError`] covers failures during lens construction and combinator
//! compilation. [`LawViolation`] reports failures of the round-trip laws
//! (`GetPut`, `PutGet` and `PutPut`).

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

    /// A value-level transform of a later lens reads a field the preceding
    /// lens drops, so those stages cannot be fused into one lens.
    #[error(
        "lens fusion refused: transform on `{anchor}` reads field `{field}`, dropped by the first lens"
    )]
    ComposeUnboundField {
        /// The anchor vertex carrying the transform.
        anchor: String,
        /// The field the transform's expression reads.
        field: String,
    },

    /// A lens path operation failed in one normal-form group.
    ///
    /// The inner error is never another `AtStage`: a caller that locates an
    /// already located error replaces its location with the outer path's
    /// group.
    #[error("lens path group {at:?} failed: {error}")]
    AtStage {
        /// The normal-form group and atomic stages at which the error arose.
        at: crate::lens_path::StageIndex,
        /// The unlocated error returned by that group.
        error: Box<Self>,
    },

    /// A path complement has a different number of entries from the
    /// normal-form groups of the path receiving it.
    #[error("path complement has {found} entries; expected {expected}")]
    PathComplementMismatch {
        /// Number of normal-form groups.
        expected: usize,
        /// Number of supplied complement entries.
        found: usize,
    },

    /// A table path received the wrong number of boundary protocols.
    #[error("table lens path needs {expected} boundary contexts; found {found}")]
    PathContextMismatch {
        /// Number of boundary contexts required.
        expected: usize,
        /// Number supplied by the caller.
        found: usize,
    },

    /// A graph complement carries state that no table complement can name.
    #[error("graph complement is not table-shaped: {reason}")]
    NotTableComplement {
        /// The first unrepresentable field or element.
        reason: String,
    },

    /// A graph complement carries state that no tree complement can name.
    #[error("graph complement is not tree-shaped: {reason}")]
    NotTreeComplement {
        /// The first unrepresentable field or element.
        reason: String,
    },

    /// A caller that can store only one compiled lens received a path whose
    /// tree normal form has more than one group.
    #[error("lens path has {stages} normal-form groups; one is required")]
    NotSingleStage {
        /// Number of normal-form groups in the path.
        stages: usize,
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

    /// A complement's identity was refused: it names a different lens,
    /// carries a legacy fingerprint that does not match or that the policy
    /// refuses, or carries data with no identity at all. See
    /// [`panproto_inst::identity`].
    #[error("complement identity: {0}")]
    Identity(#[from] panproto_inst::IdentityError),

    /// The view is outside the domain of `put`: the first violation, in
    /// report order, that
    /// [`check_put_domain`](crate::asymmetric::check_put_domain) finds.
    #[error("the view is outside the domain of put: {0}")]
    PutDomain(Box<panproto_inst::PutDomainError>),

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

    /// `PutPut` law violation: putting `v1` and then `v2` differs from
    /// putting `v2` alone.
    PutPut {
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
            Self::PutPut { detail } => write!(f, "PutPut law violated: {detail}"),
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

impl LensError {
    /// The structured detail exposed to language bindings, when this error
    /// has a shared cross-binding representation.
    #[must_use]
    pub fn detail(&self) -> Option<panproto_inst::ErrorDetail> {
        use panproto_inst::{ErrorDetail, IdentityError};

        match self {
            Self::Identity(IdentityError::Mismatch {
                field,
                expected,
                found,
            }) => Some(ErrorDetail::IdentityMismatch {
                field: *field,
                expected: expected.clone(),
                found: found.clone(),
            }),
            Self::Identity(IdentityError::LegacyComplementRejected { reason }) => {
                Some(ErrorDetail::LegacyRejected {
                    reason: *reason,
                    fingerprint: 0,
                })
            }
            Self::Identity(IdentityError::UnboundComplement) => {
                Some(ErrorDetail::UnboundComplement {})
            }
            Self::PutDomain(error) => Some(ErrorDetail::PutDomain {
                error: (**error).clone(),
            }),
            Self::Restrict(error) => Some(error.detail()),
            Self::AtStage { error, .. } => error.detail(),
            _ => None,
        }
    }
}
