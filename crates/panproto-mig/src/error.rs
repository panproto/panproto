//! Error types for migration operations.
//!
//! Each error type corresponds to a distinct failure mode in the
//! migration pipeline: existence checking, compilation, lifting,
//! composition, and inversion.

use serde::{Deserialize, Serialize};

use crate::solve::build::BuildError;
use crate::solve::mcsplit::IsoError;

/// Top-level migration error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MigError {
    /// An existence condition was violated.
    #[error("existence check failed: {0}")]
    Existence(#[from] ExistenceError),

    /// Lifting a record failed.
    #[error("lift failed: {0}")]
    Lift(#[from] LiftError),

    /// Migration composition failed.
    #[error("compose failed: {0}")]
    Compose(#[from] ComposeError),

    /// Migration inversion failed.
    #[error("inversion failed: {0}")]
    Invert(#[from] InvertError),

    /// A span search could not produce a span.
    #[error("span search failed: {0}")]
    Span(#[from] SpanError),
}

/// Why a span search could not produce a span.
///
/// None of these variants means "no morphism exists". The span search is total:
/// leaving every source vertex out of the apex is always feasible, so the
/// absence of a match is reported as an empty apex rather than as an error.
/// What is reported here is a search that could not be posed or a result that
/// is not a schema, both of which are defects rather than answers.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SpanError {
    /// The cost function network could not be built from the schema pair.
    #[error("the search network could not be built: {source}")]
    Build {
        /// What the network builder refused.
        #[from]
        source: BuildError,
    },

    /// The apex is not a well-formed sub-schema of the source.
    ///
    /// This carries whatever validating the induced apex against the protocol
    /// reported. Dangling references are one cause and the one the network
    /// guards against, by forbidding the assignments whose apex would carry
    /// one, so a dangling reference here does mean a hard constraint is
    /// missing. It is not the only cause: validation also checks vertex kinds,
    /// edge rules and constraint sorts, none of which the network models, and a
    /// sub-schema of a source the protocol already rejects inherits the
    /// parent's findings. The common case is therefore an invalid input rather
    /// than a missing constraint, and the two are told apart by running
    /// [`validate`](fn@panproto_schema::validate) on the source: if it reports
    /// the same findings, the apex only surfaced them.
    #[error("the apex is not a well-formed sub-schema of the source: {source}")]
    Apex {
        /// What inducing the apex reported.
        #[from]
        source: panproto_schema::SchemaError,
    },

    /// The total-morphism search stopped before reaching any complete
    /// assignment, so whether one exists is unknown.
    ///
    /// Distinct from `Ok(vec![])`, which is the search finishing and finding
    /// nothing. Branch and bound reaches complete assignments as it dives, so a
    /// budget spent before the first leaf leaves it with no incumbent at all,
    /// and the empty answer that would report is a claim the search never
    /// established. A stop *after* a leaf is not reported here: that incumbent
    /// is a genuine total morphism, only not a proven-optimal one.
    #[error(
        "the total-morphism search stopped on {limit:?} before reaching any complete \
         assignment, so whether a total morphism exists is unknown"
    )]
    Stopped {
        /// Which budget ran out.
        limit: crate::solve::LimitKind,
    },

    /// The maximum common sub-schema search refused the network.
    ///
    /// Its reward frame has preconditions the network must meet, and it refuses
    /// rather than silently optimising a different objective when one is
    /// broken.
    #[error("the maximum common sub-schema search refused the network: {source}")]
    Iso {
        /// The precondition of the reward frame the network broke.
        #[from]
        source: IsoError,
    },

    /// Surjectivity was asked of a span.
    ///
    /// [`SearchOptions::epic`](crate::SearchOptions::epic) is a property of a
    /// *total* morphism and the span search cannot promise it: a span's right
    /// leg is deliberately partial, the empty apex is always feasible, and
    /// [`find_span`](crate::find_span) is documented never to refuse for want of
    /// a match. Enforcing surjectivity would make it refuse, and ignoring the
    /// flag would answer a different question than the one asked, so the
    /// combination is rejected instead.
    #[error(
        "`epic` asks for a surjective vertex map, which is a property of a total \
         morphism rather than of a span; use `find_morphisms` or \
         `find_best_morphism` for a surjective total morphism"
    )]
    EpicIsNotASpanProperty,

    /// The span's right leg identifies two apex vertices, so it has no pushout.
    ///
    /// A merge along the apex has to commute: an apex vertex must reach the
    /// same merged vertex through either leg. A right leg that sends two apex
    /// vertices to one target vertex makes that impossible, and the square that
    /// comes back is not a cocone over the span it was asked about. Set
    /// [`SearchOptions::iso`](crate::SearchOptions::iso), which is what
    /// [`discover_overlap`](crate::discover_overlap) does, to search for a span
    /// whose right leg is an embedding.
    #[error(
        "the span's right leg identifies two apex vertices, so merging along it \
         would not commute; search with `iso` for a span that embeds"
    )]
    ContractingRightLeg,
}

/// A structured existence error detected by `check_existence`.
///
/// These conditions are theory-derived: the set of applicable checks
/// depends on the sorts present in the protocol's schema and instance
/// theories.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[non_exhaustive]
pub enum ExistenceError {
    /// An edge required by the target schema has no preimage in the migration.
    #[error("edge missing: {src} -> {tgt} (kind: {kind})")]
    EdgeMissing {
        /// Source vertex ID.
        src: String,
        /// Target vertex ID.
        tgt: String,
        /// Edge kind.
        kind: String,
    },

    /// A vertex is mapped to targets with inconsistent kinds.
    #[error("kind inconsistency for {kind}: targets = {targets:?}")]
    KindInconsistency {
        /// The vertex kind that is inconsistent.
        kind: String,
        /// The set of target kinds observed.
        targets: Vec<String>,
    },

    /// A label is mapped to targets with inconsistent names.
    #[error("label inconsistency for {label}: targets = {targets:?}")]
    LabelInconsistency {
        /// The label that is inconsistent.
        label: String,
        /// The set of target labels observed.
        targets: Vec<String>,
    },

    /// A required field in the target has no source.
    #[error("required field missing: vertex {vertex}, field {field}")]
    RequiredFieldMissing {
        /// The target vertex ID.
        vertex: String,
        /// The missing field (edge name).
        field: String,
    },

    /// A constraint was tightened (target is more restrictive than source).
    #[error("constraint tightened on {vertex}: {sort} changed from {src_val} to {tgt_val}")]
    ConstraintTightened {
        /// The vertex ID.
        vertex: String,
        /// The constraint sort (e.g., `"maxLength"`).
        sort: String,
        /// Source constraint value.
        src_val: String,
        /// Target constraint value.
        tgt_val: String,
    },

    /// A resolver entry references an invalid vertex pair.
    #[error("resolver invalid for pair ({}, {})", pair.0, pair.1)]
    ResolverInvalid {
        /// The invalid `(src, tgt)` pair.
        pair: (String, String),
    },

    /// A general well-formedness violation.
    #[error("well-formedness: {message}")]
    WellFormedness {
        /// Description of the violation.
        message: String,
    },

    /// A hyper-edge signature is incoherent after mapping.
    #[error("signature incoherent for hyper-edge {hyper_edge}: label {label}")]
    SignatureCoherence {
        /// The hyper-edge ID.
        hyper_edge: String,
        /// The problematic label.
        label: String,
    },

    /// A hyper-edge requires simultaneous presence of labels that the
    /// migration drops.
    #[error("simultaneity violation for hyper-edge {hyper_edge}: missing label {missing_label}")]
    Simultaneity {
        /// The hyper-edge ID.
        hyper_edge: String,
        /// The label that would be missing.
        missing_label: String,
    },

    /// A vertex risks becoming unreachable after migration.
    #[error("reachability risk for vertex {vertex}: {reason}")]
    ReachabilityRisk {
        /// The vertex at risk.
        vertex: String,
        /// Why it risks becoming unreachable.
        reason: String,
    },

    /// The migration's mapped fragment is not a structure-preserving
    /// theory morphism: a mapped edge does not connect the images of its
    /// own endpoints, or a mapped vertex is absent from the target.
    #[error("migration is not a theory morphism on its mapped fragment: {detail}")]
    NotAMorphism {
        /// The underlying structural violation, as reported by
        /// `check_morphism`.
        detail: String,
    },
}

/// Errors from the lift (record migration) operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LiftError {
    /// The underlying restrict operation failed.
    #[error("restrict failed: {0}")]
    Restrict(#[from] panproto_inst::RestrictError),

    /// The target schema is missing.
    #[error("target schema is required for W-type lift")]
    MissingTargetSchema,

    /// The term-level chase failed while closing a `Sigma` result.
    ///
    /// The chase's own error is carried, not flattened to text, because
    /// the two failures it reports call for different responses: a
    /// budget that ran out can be retried with a larger one, while an
    /// equality conflict is a property of the data and the dependencies
    /// and will recur however much budget it is given.
    #[error("chase failed: {0}")]
    Chase(#[from] crate::chase::ChaseError),

    /// The term-level chase ran out of budget before reaching a
    /// fixpoint. Retrying with a larger budget may succeed.
    #[error(
        "term-level chase did not terminate within {max_iterations} iterations / {max_nulls} nulls"
    )]
    ChaseBudgetExhausted {
        /// The iteration ceiling the chase was given.
        max_iterations: usize,
        /// The labeled-null ceiling the chase was given.
        max_nulls: usize,
    },
}

impl LiftError {
    /// Whether retrying the lift with a larger chase budget could
    /// succeed.
    ///
    /// True exactly for the two budget-exhaustion failures. Every other
    /// variant reports something a larger budget cannot change.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        match self {
            Self::Chase(err) => err.is_retryable(),
            Self::ChaseBudgetExhausted { .. } => true,
            _ => false,
        }
    }
}

/// Errors from migration composition.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ComposeError {
    /// An edge in the intermediate schema is not in the second migration's domain.
    #[error("edge not found in second migration's domain: {src} -> {tgt} ({kind})")]
    EdgeNotInDomain {
        /// Source vertex.
        src: String,
        /// Target vertex.
        tgt: String,
        /// Edge kind.
        kind: String,
    },

    /// The first migration's codomain does not match the second
    /// migration's domain, so the two are not composable.
    #[error(
        "migrations are not composable: first codomain `{first_codomain}` != second domain `{second_domain}`"
    )]
    DomainMismatch {
        /// The codomain identifier of the first migration.
        first_codomain: String,
        /// The domain identifier of the second migration.
        second_domain: String,
    },
}

/// Errors from migration inversion.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvertError {
    /// The vertex map is not bijective (injective + surjective).
    #[error("vertex map is not bijective: {detail}")]
    NotBijective {
        /// Description of the bijectivity failure.
        detail: String,
    },

    /// The edge map is not bijective.
    #[error("edge map is not bijective: {detail}")]
    EdgeNotBijective {
        /// Description of the bijectivity failure.
        detail: String,
    },

    /// Vertices were dropped (the migration is not surjective on vertices).
    #[error("migration drops vertices: {dropped:?}")]
    DroppedVertices {
        /// The dropped vertex IDs.
        dropped: Vec<String>,
    },

    /// Edges were dropped.
    #[error("migration drops edges")]
    DroppedEdges,

    /// The hyper-edge map is not bijective.
    #[error("hyper-edge map is not bijective: {detail}")]
    HyperEdgeNotBijective {
        /// Description of the bijectivity failure.
        detail: String,
    },

    /// Hyper-edges were dropped (the migration is not surjective on hyper-edges).
    #[error("migration drops hyper-edges: {dropped:?}")]
    DroppedHyperEdges {
        /// The dropped hyper-edge IDs.
        dropped: Vec<String>,
    },

    /// A vertex's value-level coercion records no inverse term, so the
    /// inverted migration has no way to bring its values back.
    #[error(
        "the coercion at vertex `{vertex}` records no inverse term, so the values \
         it rewrites cannot be brought back; the inverse migration is undefined \
         there"
    )]
    CoercionNotInvertible {
        /// The source vertex whose coercion has no inverse.
        vertex: String,
    },
}
