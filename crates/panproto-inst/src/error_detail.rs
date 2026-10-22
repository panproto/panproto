//! Structured error details shared by every shape and every binding.
//!
//! An error crosses the language bindings as a message and an
//! [`ErrorDetail`]: a serde value a host decodes into its own types, so no
//! binding ever parses a message. [`PutDomainError`] is the one type of the
//! reasons a view lies outside the domain of `put`, for trees, tables and
//! graphs alike, so that a dispatched call reports the error its graph form
//! would report.
//!
//! Both types live here rather than in `panproto-lens` because errors this
//! crate defines project onto them, and this crate cannot depend on
//! `panproto-lens`, which re-exports both.

use panproto_gat::CompositionError;
use panproto_gat::Name;
use serde::{Deserialize, Serialize};

use crate::error::RestrictError;
use crate::fan_violation::FanViolation;
use crate::graph_edit::EditConflict;
use crate::identity::{IdentityField, LegacyRejectReason};
use crate::segment::{ErrorKey, Residual, SegmentError};
use crate::validation::ValidationReport;
use crate::value::Value;
use crate::view_ref::ViewRef;

/// Why a view is outside the domain of `put`.
///
/// One enum serves all three shapes: an element is named by a [`ViewRef`]
/// of the view's own shape. Serialized externally tagged in `snake_case`,
/// for instance `{"contracted_edge_inserted": {"elem": {"node": 3}}}`.
/// The variants are declared in the primary order in which a report of
/// several errors lists them ([`variant_index`](Self::variant_index)).
///
/// ```
/// use panproto_inst::{PutDomainError, ViewRef};
///
/// let e = PutDomainError::SynthesizedEdited { elem: ViewRef::Node(4) };
/// assert_eq!(
///     serde_json::to_string(&e).unwrap(),
///     r#"{"synthesized_edited":{"elem":{"node":4}}}"#,
/// );
/// assert_eq!(e.variant_index(), 1);
///
/// let e = PutDomainError::ContractedEdgeInserted { elem: ViewRef::Node(3) };
/// assert_eq!(
///     serde_json::to_string(&e).unwrap(),
///     r#"{"contracted_edge_inserted":{"elem":{"node":3}}}"#,
/// );
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum PutDomainError {
    /// A view element reuses the id of a source element it does not denote.
    #[error("view element {elem:?} collides with the id of another source element")]
    IdCollision {
        /// The colliding view element.
        elem: ViewRef,
    },
    /// A view element that `get` synthesized was edited.
    #[error("view element {elem:?} was synthesized by get and cannot be edited")]
    SynthesizedEdited {
        /// The edited synthesized element.
        elem: ViewRef,
    },
    /// A view edge was inserted where the lens contracts a path of the
    /// source, so no source path can carry it.
    #[error("view element {elem:?} would be a new contracted pair")]
    ContractedEdgeInserted {
        /// The inserted edge.
        elem: ViewRef,
    },
    /// A view edge has no source edge or path it can be written back to.
    #[error("view element {elem:?} has no representation in the source")]
    UnrepresentableViewEdge {
        /// The unrepresentable edge.
        elem: ViewRef,
    },
    /// A view node could be written back under more than one source anchor.
    #[error("view element {elem:?} has several possible source anchors: {candidates:?}")]
    AmbiguousReverseAnchor {
        /// The ambiguous element.
        elem: ViewRef,
        /// The candidate source anchors, ascending.
        candidates: Vec<Name>,
    },
    /// Writing the view element back would give a source element whose
    /// survival predicate is false, so `get` would drop it again.
    #[error("view element {elem:?} would not survive get")]
    SurvivalPredicateFails {
        /// The element.
        elem: ViewRef,
    },
    /// A field the lens computes from other fields was edited in the view.
    #[error("derived field {field} of view element {elem:?} was edited")]
    DerivedCellEdited {
        /// The element holding the field.
        elem: ViewRef,
        /// The derived field.
        field: String,
    },
    /// A field was edited whose transform has no inverse to write it back.
    #[error("field {field} of view element {elem:?} was edited but its transform has no inverse")]
    TransformNotInvertible {
        /// The element holding the field.
        elem: ViewRef,
        /// The field.
        field: String,
    },
    /// Writing the view back would give two rows or nodes of one table the
    /// same key.
    #[error("rows {elems:?} of {table} share the key {key:?}")]
    DuplicateKey {
        /// The table (or vertex) whose key repeats.
        table: Name,
        /// The repeated key, one `(column, value)` pair per key part.
        key: Vec<(String, Value)>,
        /// The restored rows or nodes sharing the key, ascending.
        elems: Vec<ViewRef>,
    },
    /// Writing the view back would leave a required field without a value.
    #[error("view element {elem:?} lacks the required field {field}")]
    MissingRequiredCell {
        /// The element.
        elem: ViewRef,
        /// The required field.
        field: String,
    },
    /// A view node meant as a hyper-edge fan is malformed.
    #[error("fan node {node} is malformed: {reason}")]
    MalformedFan {
        /// The fan node.
        node: u32,
        /// What is wrong with it.
        reason: FanViolation,
    },
    /// Writing the view element back would give a source element whose
    /// survival predicate gives no verdict: `get` would refuse the restored
    /// source rather than keep or drop the element.
    ///
    /// ```
    /// use panproto_inst::{PredicateRefusal, PutDomainError, ViewRef};
    ///
    /// let e = PutDomainError::SurvivalPredicateRefused {
    ///     elem: ViewRef::Node(1),
    ///     refusal: PredicateRefusal::NotBoolean { got: "int".into() },
    /// };
    /// let json = serde_json::to_string(&e).unwrap();
    /// assert_eq!(
    ///     json,
    ///     r#"{"survival_predicate_refused":{"elem":{"node":1},"refusal":{"not_boolean":{"got":"int"}}}}"#,
    /// );
    /// assert_eq!(serde_json::from_str::<PutDomainError>(&json).unwrap(), e);
    /// assert_eq!(e.variant_index(), 11);
    /// ```
    #[error("view element {elem:?} would be refused by its survival predicate: {refusal}")]
    SurvivalPredicateRefused {
        /// The element.
        elem: ViewRef,
        /// Why the predicate gives no verdict.
        refusal: PredicateRefusal,
    },
    /// The migration cannot be compiled into an executable restrict plan.
    ///
    /// A plan failure is a property of the lens rather than of one view,
    /// but it still makes every view lie outside that lens's put domain.
    /// Keeping it in the ordered domain report prevents a failed compile
    /// from being mistaken for an admitted view.
    #[error("the migration cannot be compiled into a restrict plan: {detail}")]
    PlanCompilation {
        /// Stable display form of the restrict-plan compilation error.
        detail: String,
    },
}

/// Why a survival predicate gives neither `true` nor `false` at an element.
///
/// The variants follow the order `get` checks: a read child name that two or
/// more children carry, a read structural child that is no tree, a read value
/// with no expression literal, then an evaluation that fails or yields no
/// boolean.
///
/// Serialized externally tagged in `snake_case`, with the derives of
/// [`PutDomainError`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum PredicateRefusal {
    /// The predicate reads a child name two or more children carry.
    #[error("child field {field} is carried by several children")]
    AmbiguousChildField {
        /// The child name.
        field: String,
    },
    /// The predicate reads a structural child whose subgraph is no tree.
    #[error("structural child {field} is not a tree")]
    StructuralChildNotATree {
        /// The child name.
        field: String,
    },
    /// The predicate reads a value with no expression literal.
    #[error("field {field} holds a {value_type} value, which has no expression literal")]
    ValueNotExpressible {
        /// The field.
        field: String,
        /// The value's type name.
        value_type: String,
    },
    /// The predicate fails to evaluate.
    #[error("evaluation failed: {message}")]
    EvaluationFailed {
        /// The evaluator's message.
        message: String,
    },
    /// The predicate evaluates to a literal that is not a boolean.
    #[error("evaluates to a {got}, not a bool")]
    NotBoolean {
        /// The literal's type name.
        got: String,
    },
}

impl PutDomainError {
    /// The position of this error's variant in declaration order, the
    /// primary key of the order in which a report lists several errors.
    #[must_use]
    pub const fn variant_index(&self) -> u8 {
        match self {
            Self::IdCollision { .. } => 0,
            Self::SynthesizedEdited { .. } => 1,
            Self::ContractedEdgeInserted { .. } => 2,
            Self::UnrepresentableViewEdge { .. } => 3,
            Self::AmbiguousReverseAnchor { .. } => 4,
            Self::SurvivalPredicateFails { .. } => 5,
            Self::DerivedCellEdited { .. } => 6,
            Self::TransformNotInvertible { .. } => 7,
            Self::DuplicateKey { .. } => 8,
            Self::MissingRequiredCell { .. } => 9,
            Self::MalformedFan { .. } => 10,
            Self::SurvivalPredicateRefused { .. } => 11,
            Self::PlanCompilation { .. } => 12,
        }
    }
}

/// The structured detail of an error, as every binding receives it.
///
/// Serialized externally tagged in `snake_case`, for instance
/// `{"unbound_complement": {}}`. A host decodes this value and never the
/// error's message.
///
/// ```
/// use panproto_inst::ErrorDetail;
///
/// assert_eq!(
///     serde_json::to_string(&ErrorDetail::UnboundComplement {}).unwrap(),
///     r#"{"unbound_complement":{}}"#,
/// );
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorDetail {
    /// A complement's identity envelope differs from the lens's.
    IdentityMismatch {
        /// The first differing envelope field.
        field: IdentityField,
        /// The lens's value: a digest as hex, a format as `shape/version`.
        expected: String,
        /// The complement's value, rendered the same way.
        found: String,
    },
    /// A legacy complement, which carries only a schema fingerprint, was
    /// refused.
    LegacyRejected {
        /// Why it was refused.
        reason: LegacyRejectReason,
        /// The complement's legacy fingerprint.
        fingerprint: u64,
    },
    /// A complement carries data but nothing that binds it to a lens.
    UnboundComplement {},
    /// A view is outside the domain of `put`.
    PutDomain {
        /// Why.
        error: PutDomainError,
    },
    /// A composition of theories was refused.
    Composition {
        /// Why.
        error: CompositionError,
    },
    /// An edit could not be applied or composed. `path` lists the batch
    /// step indices leading to the failing step, outermost first; `kind` is
    /// the snake-case name of the error; `node`, `edge`, `row` and `pair`
    /// name the element it is about, a pair as `(edge, src, tgt)`.
    EditConflict {
        /// The batch step indices, outermost first.
        path: Vec<u32>,
        /// The snake-case name of the error.
        kind: String,
        /// The node the error is about.
        node: Option<u32>,
        /// The edge the error is about.
        edge: Option<u32>,
        /// The row the error is about.
        row: Option<u32>,
        /// The foreign-key pair the error is about.
        pair: Option<(String, u32, u32)>,
    },
    /// Two edits of one state are not independent.
    Independence {
        /// Every conflict between them, ascending.
        conflicts: Vec<EditConflict>,
    },
    /// A lens law checker found a counterexample.
    LawViolation {
        /// Stable snake-case law name.
        law: String,
        /// Human-readable counterexample detail.
        detail: String,
    },
    /// Validation at a boundary did not pass.
    Validation {
        /// The report.
        report: ValidationReport,
    },
    /// A segmented operation failed.
    Segment {
        /// Why.
        error: SegmentError,
    },
    /// A segmented operation was declined within its declared bounds: the
    /// whole-state path would perform it.
    Residual {
        /// What was declined, and why.
        residual: Residual,
    },
    /// A restrict failed. `kind` is the snake-case name of the
    /// [`RestrictError`] variant and `message` its display text.
    Restrict {
        /// The error's position in the graph engine's precedence order:
        /// [`RestrictError::key`], which is `Some` exactly for an error the
        /// graph engine located.
        key: Option<ErrorKey>,
        /// The snake-case name of the error's variant.
        kind: String,
        /// The error's display text.
        message: String,
    },
}

/// The snake-case name of a restrict error's variant.
const fn restrict_kind(error: &RestrictError) -> &'static str {
    match error {
        RestrictError::NoEdgeFound { .. } => "no_edge_found",
        RestrictError::AmbiguousEdge { .. } => "ambiguous_edge",
        RestrictError::RootPruned => "root_pruned",
        RestrictError::FanReconstructionFailed { .. } => "fan_reconstruction_failed",
        RestrictError::ProductSizeExceeded { .. } => "product_size_exceeded",
        RestrictError::MultiElementFiber { .. } => "multi_element_fiber",
        RestrictError::UnmappedAnchor { .. } => "unmapped_anchor",
        RestrictError::AttributeCollision { .. } => "attribute_collision",
        RestrictError::NonInjectiveVertexMap { .. } => "non_injective_vertex_map",
        RestrictError::FieldTransformFailed { .. } => "field_transform_failed",
        RestrictError::RowIdsExhausted => "row_ids_exhausted",
        RestrictError::VertexOutsideDomain { .. } => "vertex_outside_domain",
        RestrictError::EdgeOutsideDomain { .. } => "edge_outside_domain",
        RestrictError::IncoherentEdgeImage { .. } => "incoherent_edge_image",
        RestrictError::VertexImageNotATable { .. } => "vertex_image_not_a_table",
        RestrictError::IncoherentAttributeImage { .. } => "incoherent_attribute_image",
        RestrictError::AttributeOutsideImage { .. } => "attribute_outside_image",
        RestrictError::ValueStructureRemap { .. } => "value_structure_remap",
        RestrictError::RelSchema(_) => "rel_schema",
        RestrictError::Validation { .. } => "validation",
        RestrictError::AmbiguousChildField { .. } => "ambiguous_child_field",
        RestrictError::NotTableRepresentable { .. } => "not_table_representable",
        RestrictError::ValueNotExpressible { .. } => "value_not_expressible",
        RestrictError::At { error, .. } => restrict_kind(error),
        RestrictError::SurvivalRulesDisagree { .. } => "survival_rules_disagree",
        RestrictError::TablePlanRequired => "table_plan_required",
        RestrictError::StructuralChildNotATree { .. } => "structural_child_not_a_tree",
        RestrictError::IncoherentPairImage { .. } => "incoherent_pair_image",
        RestrictError::ValueFidelity { .. } => "value_fidelity",
        RestrictError::IdsExhausted { .. } => "ids_exhausted",
        RestrictError::SurvivalPredicateFailed { .. } => "survival_predicate_failed",
        RestrictError::PlanSchemaMismatch { .. } => "plan_schema_mismatch",
        RestrictError::ExpansionTooLong { .. } => "expansion_too_long",
        RestrictError::NonConforming { .. } => "non_conforming",
        RestrictError::UnmappedEdge { .. } => "unmapped_edge",
        RestrictError::SigmaConstantClash { .. } => "sigma_constant_clash",
        RestrictError::NullIdsExhausted => "null_ids_exhausted",
        RestrictError::MalformedFan { .. } => "malformed_fan",
    }
}

impl RestrictError {
    /// The error's structured detail.
    ///
    /// A boundary failure ([`RestrictError::Validation`]) projects onto
    /// [`ErrorDetail::Validation`] with its report. Every other error
    /// projects onto [`ErrorDetail::Restrict`] with its variant name and
    /// display text, and with its [`key`](Self::key): the error's position
    /// in the graph engine's precedence order when the graph engine located
    /// it ([`RestrictError::At`], whose kind and message are its inner
    /// error's), and `None` otherwise.
    ///
    /// ```
    /// use panproto_inst::segment::ErrorKey;
    /// use panproto_inst::{ErrorDetail, RestrictError};
    ///
    /// assert_eq!(
    ///     RestrictError::RootPruned.detail(),
    ///     ErrorDetail::Restrict {
    ///         key: None,
    ///         kind: "root_pruned".to_owned(),
    ///         message: "root node was pruned during restriction".to_owned(),
    ///     },
    /// );
    ///
    /// let key = ErrorKey { stage: 1, id: 4, id2: 0, kind: 0 };
    /// let located = RestrictError::At {
    ///     key,
    ///     error: Box::new(RestrictError::NoEdgeFound { src: "a".into(), tgt: "b".into() }),
    /// };
    /// assert_eq!(
    ///     located.detail(),
    ///     ErrorDetail::Restrict {
    ///         key: Some(key),
    ///         kind: "no_edge_found".to_owned(),
    ///         message: "no edge found between a and b in target schema".to_owned(),
    ///     },
    /// );
    /// ```
    #[must_use]
    pub fn detail(&self) -> ErrorDetail {
        match self {
            Self::Validation { report, .. } => ErrorDetail::Validation {
                report: (**report).clone(),
            },
            _ => ErrorDetail::Restrict {
                key: self.key(),
                kind: restrict_kind(self).to_owned(),
                message: self.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorDetail, PutDomainError};

    #[test]
    fn plan_compilation_domain_error_has_a_stable_wire_shape_and_order()
    -> Result<(), serde_json::Error> {
        let error = PutDomainError::PlanCompilation {
            detail: "incoherent edge image".to_owned(),
        };

        assert_eq!(error.variant_index(), 12);
        let json = serde_json::to_string(&error)?;
        assert_eq!(
            json,
            r#"{"plan_compilation":{"detail":"incoherent edge image"}}"#
        );
        assert_eq!(serde_json::from_str::<PutDomainError>(&json)?, error);
        Ok(())
    }

    #[test]
    fn error_detail_law_violation_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        let detail = ErrorDetail::LawViolation {
            law: "put_twice".to_owned(),
            detail: "the complement drifted".to_owned(),
        };

        let json = serde_json::to_string(&detail)?;
        assert_eq!(
            json,
            r#"{"law_violation":{"law":"put_twice","detail":"the complement drifted"}}"#
        );
        assert_eq!(serde_json::from_str::<ErrorDetail>(&json)?, detail);

        let message_pack = rmp_serde::to_vec_named(&detail)?;
        assert_eq!(rmp_serde::from_slice::<ErrorDetail>(&message_pack)?, detail);

        let mut cbor = Vec::new();
        ciborium::into_writer(&detail, &mut cbor)?;
        assert_eq!(ciborium::from_reader::<ErrorDetail, _>(&cbor[..])?, detail);
        Ok(())
    }
}
