//! Error types for lens operations.
//!
//! [`LensError`] covers failures during lens construction, combinator
//! compilation, and lens execution. [`LawViolation`] reports typed failures
//! from the strict gate sequence and the standalone law and classification
//! checks.

use std::fmt;

use panproto_inst::{EdgeId, RowId};
use panproto_schema::Edge;

/// Why graph complement state cannot be represented by a table complement.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum NotTableComplementReason {
    /// The source schema has no relational classification under the boundary protocol.
    #[error("source schema is not relational: {detail}")]
    RelSchema {
        /// The relational-classification failure.
        detail: String,
    },
    /// A dropped table pair has no durable graph edge id.
    #[error("source embedding has no dropped pair {edge:?} ({src:?}, {tgt:?})")]
    MissingDroppedPair {
        /// The relation edge.
        edge: Box<Edge>,
        /// Source row.
        src: RowId,
        /// Target row.
        tgt: RowId,
    },
    /// A view pair with an original relation has no durable graph edge id.
    #[error("view embedding has no original pair {edge:?} ({src:?}, {tgt:?})")]
    MissingOriginalPair {
        /// The view relation edge.
        edge: Box<Edge>,
        /// Source row.
        src: RowId,
        /// Target row.
        tgt: RowId,
    },
    /// One graph edge would have to remember several source relation edges.
    #[error("one graph edge cannot carry {count} original foreign keys")]
    MultipleOriginalForeignKeys {
        /// Number of source relation edges.
        count: usize,
    },
    /// A row has no unique source table under the vertex map.
    #[error("row {row:?} has no determined source table")]
    UndeterminedSourceTable {
        /// The row.
        row: RowId,
    },
    /// A graph-only synthesized node is present.
    #[error("synthesized node {node} has no table-complement coordinate")]
    SynthesizedNode {
        /// The synthesized node id.
        node: u32,
    },
    /// A dropped node carries a value attribute.
    #[error("dropped node {node} carries a value attribute")]
    NodeValue {
        /// The node id.
        node: u32,
    },
    /// A dropped node carries discriminator metadata.
    #[error("dropped node {node} carries discriminator metadata")]
    NodeDiscriminator {
        /// The node id.
        node: u32,
    },
    /// A dropped node carries an edge-position attribute.
    #[error("dropped node {node} carries a position attribute")]
    NodePosition {
        /// The node id.
        node: u32,
    },
    /// A dropped node reifies a fan.
    #[error("dropped node {node} reifies a fan")]
    FanNode {
        /// The fan node id.
        node: u32,
    },
    /// A dropped node carries graph metadata.
    #[error("dropped node {node} carries annotations")]
    NodeAnnotations {
        /// The node id.
        node: u32,
    },
    /// A dropped node carries a nested value in a cell slot.
    #[error("dropped node {node} carries nested cell `{column}`")]
    NestedCell {
        /// The node id.
        node: u32,
        /// The offending cell name.
        column: String,
    },
    /// A dropped or original graph edge carries an edge position.
    #[error("edge {edge} carries a position attribute")]
    EdgePosition {
        /// The graph edge id.
        edge: EdgeId,
    },
    /// A dropped or original graph edge reifies a fan leg.
    #[error("edge {edge} reifies a fan leg")]
    FanEdge {
        /// The graph edge id.
        edge: EdgeId,
    },
    /// An original graph edge is absent from the table embedding.
    #[error("view embedding has no edge {edge}")]
    MissingEmbeddedEdge {
        /// The graph edge id.
        edge: EdgeId,
    },
    /// A field snapshot names no view row.
    #[error("view has no row {row:?}")]
    MissingViewRow {
        /// The row.
        row: RowId,
    },
    /// A synthesized graph edge is absent from the table embedding.
    #[error("view embedding has no synthesized edge {edge}")]
    MissingSynthesizedEdge {
        /// The graph edge id.
        edge: EdgeId,
    },
    /// A synthesized edge key is not a contracted table pair.
    #[error("synthesized edge {edge} is not a contracted table pair")]
    InvalidSynthesizedEdge {
        /// The graph edge id.
        edge: EdgeId,
    },
}

/// Why graph complement state cannot be represented by a tree complement.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum NotTreeComplementReason {
    /// The view embedding has no graph edge for a tree arc.
    #[error("view embedding has no arc ({parent}, {child}) with an original edge")]
    MissingEmbeddedArc {
        /// Parent node.
        parent: u32,
        /// Child node.
        child: u32,
    },
    /// Two graph edges would both be the parent arc of one tree node.
    #[error("node {node} has graph edges {first} and {second} as parents")]
    MultipleParents {
        /// The child node.
        node: u32,
        /// First edge id.
        first: EdgeId,
        /// Second edge id.
        second: EdgeId,
    },
    /// A fan id has no node record.
    #[error("fan node {node} has no node record")]
    MissingFanNode {
        /// The fan node id.
        node: u32,
    },
    /// A fan edge id has no edge record.
    #[error("fan edge {edge} has no edge record")]
    MissingFanEdge {
        /// The fan edge id.
        edge: EdgeId,
    },
    /// A fan has more than one parent edge.
    #[error("fan node {node} has two parent edges")]
    MultipleFanParents {
        /// The fan node id.
        node: u32,
    },
    /// A fan child edge has no label.
    #[error("fan edge {edge} has no label")]
    MissingFanLabel {
        /// The fan edge id.
        edge: EdgeId,
    },
    /// A fan repeats a child label.
    #[error("fan node {node} repeats label `{label}`")]
    DuplicateFanLabel {
        /// The fan node id.
        node: u32,
        /// The repeated label.
        label: String,
    },
    /// A fan has no parent target.
    #[error("fan node {node} has no parent")]
    MissingFanParent {
        /// The fan node id.
        node: u32,
    },
    /// A fan has no durable parent edge id.
    #[error("fan node {node} has no parent edge")]
    MissingFanParentEdge {
        /// The fan node id.
        node: u32,
    },
    /// A synthesized tree arc is absent from the graph view.
    #[error("synthesized edge {edge} is absent from the view")]
    MissingSynthesizedEdge {
        /// The graph edge id.
        edge: EdgeId,
    },
    /// A resolution key has no tree-complement coordinate.
    #[error("synthesized edge {edge} carries a resolution key")]
    ResolutionSynthesizedEdge {
        /// The graph edge id.
        edge: EdgeId,
    },
    /// A tree expansion key names no arc in the durable source embedding.
    #[error("tree expansion key names absent source arc {edge}")]
    MissingSourceArc {
        /// The source arc id in the tree presentation.
        edge: EdgeId,
    },
    /// A synthesized tree node has no entering arc in the view embedding.
    #[error("synthesized tree node {node} has no entering arc")]
    MissingSynthesizedNodeArc {
        /// The synthesized node id.
        node: u32,
    },
}

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
    ///
    /// Adding a fusion condition can regroup an unchanged atomic path and
    /// therefore invalidate an older grouped complement. One complement
    /// entry per group is the product complement for the inferred Kleisli
    /// composition of partial puts in the error monad (`xie2025effectful`,
    /// Sections 2.1 and 2.4).
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

    /// A checked protolens instantiation failed its declared source
    /// precondition before either schema transform was evaluated.
    #[error("protolens is not applicable: {reasons:?}")]
    ProtolensInapplicable {
        /// The complete set of failed precondition clauses.
        reasons: Vec<String>,
    },

    /// A protolens operation other than source-precondition checking failed.
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

    /// A graph complement carries state that no table complement can name.
    #[error("graph complement is not table-shaped: {reason}")]
    NotTableComplement {
        /// The first unrepresentable field or element.
        reason: NotTableComplementReason,
    },

    /// A graph complement carries state that no tree complement can name.
    #[error("graph complement is not tree-shaped: {reason}")]
    NotTreeComplement {
        /// The first unrepresentable field or element.
        reason: NotTreeComplementReason,
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

    /// `WeakPutGet` law violation.
    WeakPutGet {
        /// Human-readable description of the difference.
        detail: String,
    },

    /// A generated candidate disagreed with its declared put-domain outcome.
    ///
    /// This covers both an admitted view whose put failed and a declared
    /// refusal whose complete ordered report or raw first violation differed.
    Totality {
        /// The mutation family that exposed the disagreement.
        mutation: crate::laws::Mutation,
        /// The put failure or exact-negative certificate mismatch.
        error: String,
    },

    /// Repeating a put with the complement returned by re-get drifted.
    PutTwice {
        /// Human-readable description of the difference.
        detail: String,
    },

    /// Two distinct in-domain views put to one source.
    PutInjectivity {
        /// Human-readable description of the collision.
        detail: String,
    },

    /// Put changed a source carrier outside the edit footprint and cascade.
    Locality {
        /// Human-readable description of the unexpected change.
        detail: String,
    },

    /// Observational equivalence was not a congruence of get or put.
    Congruence {
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
            Self::WeakPutGet { detail } => write!(f, "WeakPutGet law violated: {detail}"),
            Self::Totality { mutation, error } => {
                write!(f, "Totality law violated by {}: {error}", mutation.as_str())
            }
            Self::PutTwice { detail } => write!(f, "PutTwice law violated: {detail}"),
            Self::PutInjectivity { detail } => {
                write!(f, "PutInjectivity law violated: {detail}")
            }
            Self::Locality { detail } => write!(f, "Locality law violated: {detail}"),
            Self::Congruence { detail } => write!(f, "Congruence law violated: {detail}"),
            Self::Error(e) => write!(f, "error during law check: {e}"),
        }
    }
}

impl LawViolation {
    /// Structured detail exposed to language bindings.
    #[must_use]
    pub fn detail(&self) -> panproto_inst::ErrorDetail {
        let (law, detail) = match self {
            Self::GetPut { detail } => ("get_put", detail.clone()),
            Self::PutGet { detail } => ("put_get", detail.clone()),
            Self::PutPut { detail } => ("put_put", detail.clone()),
            Self::WeakPutGet { detail } => ("weak_put_get", detail.clone()),
            Self::Totality { mutation, error } => {
                ("totality", format!("{}: {error}", mutation.as_str()))
            }
            Self::PutTwice { detail } => ("put_twice", detail.clone()),
            Self::PutInjectivity { detail } => ("put_injectivity", detail.clone()),
            Self::Locality { detail } => ("locality", detail.clone()),
            Self::Congruence { detail } => ("congruence", detail.clone()),
            Self::Error(error) => ("operation", error.to_string()),
        };
        panproto_inst::ErrorDetail::LawViolation {
            law: law.to_owned(),
            detail,
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
            Self::Identity(IdentityError::UnboundComplement) => {
                Some(ErrorDetail::UnboundComplement {})
            }
            Self::PutDomain(error) => Some(ErrorDetail::PutDomain {
                error: (**error).clone(),
            }),
            Self::Restrict(error) => Some(error.detail()),
            Self::AtStage { error, .. } => error.detail(),
            // This includes `LegacyComplementRejected`: its typed source
            // error has no fingerprint, so the binding detail's required
            // fingerprint field cannot be filled without fabrication.
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LawViolation;
    use crate::laws::Mutation;
    use panproto_inst::ErrorDetail;

    #[test]
    fn law_violations_project_to_stable_binding_details() {
        let cases = [
            (
                LawViolation::GetPut {
                    detail: "get-put".to_owned(),
                },
                "get_put",
                "get-put",
            ),
            (
                LawViolation::PutGet {
                    detail: "put-get".to_owned(),
                },
                "put_get",
                "put-get",
            ),
            (
                LawViolation::PutPut {
                    detail: "put-put".to_owned(),
                },
                "put_put",
                "put-put",
            ),
            (
                LawViolation::WeakPutGet {
                    detail: "weak".to_owned(),
                },
                "weak_put_get",
                "weak",
            ),
            (
                LawViolation::Totality {
                    mutation: Mutation::Reconnect,
                    error: "partial put".to_owned(),
                },
                "totality",
                "reconnect: partial put",
            ),
            (
                LawViolation::PutTwice {
                    detail: "twice".to_owned(),
                },
                "put_twice",
                "twice",
            ),
            (
                LawViolation::PutInjectivity {
                    detail: "collision".to_owned(),
                },
                "put_injectivity",
                "collision",
            ),
            (
                LawViolation::Locality {
                    detail: "footprint".to_owned(),
                },
                "locality",
                "footprint",
            ),
            (
                LawViolation::Congruence {
                    detail: "reindexing".to_owned(),
                },
                "congruence",
                "reindexing",
            ),
        ];
        for (violation, law, detail) in cases {
            assert_eq!(
                violation.detail(),
                ErrorDetail::LawViolation {
                    law: law.to_owned(),
                    detail: detail.to_owned(),
                }
            );
        }
    }
}
