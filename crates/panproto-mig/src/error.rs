//! Error types for migration operations.
//!
//! Each error type corresponds to a distinct failure mode in the
//! migration pipeline: existence checking, compilation, lifting,
//! composition, and inversion.

use serde::{Deserialize, Serialize};

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
}
