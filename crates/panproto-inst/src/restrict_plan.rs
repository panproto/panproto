//! The compiled plan of a restrict, shared by the tree, table and graph
//! engines.

use panproto_gat::Name;
use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

/// An element of the source schema on which the survival rule of the
/// restrict and the table functor of a migration disagree.
///
/// Serialized externally tagged in `snake_case`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurvivalElement {
    /// A source table.
    Table(Name),
    /// A source relation or attribute arrow.
    Edge(Edge),
}

impl std::fmt::Display for SurvivalElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Table(table) => write!(f, "table `{table}`"),
            Self::Edge(edge) => write!(f, "edge {edge:?}"),
        }
    }
}

impl crate::error::RestrictError {
    /// The error's position in the graph engine's precedence order:
    /// `Some` exactly on [`RestrictError::At`], the form in which the graph
    /// engine returns every error it raises while evaluating an instance,
    /// and `None` on every other error (the tree and table engines, plan
    /// compilation and the boundary refusals locate nothing).
    ///
    /// [`RestrictError::At`]: crate::error::RestrictError::At
    #[must_use]
    pub const fn key(&self) -> Option<crate::segment::ErrorKey> {
        match self {
            Self::At { key, .. } => Some(*key),
            _ => None,
        }
    }

    /// The error without its location: the inner error of
    /// [`RestrictError::At`] and the error itself otherwise. Statements that
    /// two engines raise the same error compare this.
    ///
    /// [`RestrictError::At`]: crate::error::RestrictError::At
    #[must_use]
    pub fn unlocated(&self) -> &Self {
        match self {
            Self::At { error, .. } => error,
            _ => self,
        }
    }

    /// The index of the error's variant in declaration order (the inner
    /// error's, for a located one): the `kind` of its [`ErrorKey`].
    ///
    /// [`ErrorKey`]: crate::segment::ErrorKey
    #[must_use]
    pub fn kind_index(&self) -> u8 {
        match self {
            Self::NoEdgeFound { .. } => 0,
            Self::AmbiguousEdge { .. } => 1,
            Self::RootPruned => 2,
            Self::FanReconstructionFailed { .. } => 3,
            Self::ProductSizeExceeded { .. } => 4,
            Self::MultiElementFiber { .. } => 5,
            Self::UnmappedAnchor { .. } => 6,
            Self::AttributeCollision { .. } => 7,
            Self::NonInjectiveVertexMap { .. } => 8,
            Self::FieldTransformFailed { .. } => 9,
            Self::RowIdsExhausted => 10,
            Self::VertexOutsideDomain { .. } => 11,
            Self::EdgeOutsideDomain { .. } => 12,
            Self::IncoherentEdgeImage { .. } => 13,
            Self::VertexImageNotATable { .. } => 14,
            Self::IncoherentAttributeImage { .. } => 15,
            Self::AttributeOutsideImage { .. } => 16,
            Self::ValueStructureRemap { .. } => 17,
            Self::RelSchema(_) => 18,
            Self::Validation { .. } => 19,
            Self::AmbiguousChildField { .. } => 20,
            Self::NotTableRepresentable { .. } => 21,
            Self::ValueNotExpressible { .. } => 22,
            Self::At { error, .. } => error.kind_index(),
            Self::SurvivalRulesDisagree { .. } => 24,
            Self::TablePlanRequired => 25,
            Self::StructuralChildNotATree { .. } => 26,
            Self::IncoherentPairImage { .. } => 27,
            Self::ValueFidelity { .. } => 28,
            Self::IdsExhausted { .. } => 29,
            Self::SurvivalPredicateFailed { .. } => 30,
            Self::PlanSchemaMismatch { .. } => 31,
        }
    }
}
