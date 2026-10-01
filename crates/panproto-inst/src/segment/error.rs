//! The typed failures and residuals of segmented operations.
//!
//! [`SegmentError`] is the one error type of every segmented operation. It
//! is `Clone`, `PartialEq` and serializable, so a phase output, a run report
//! or a binding can carry it as data. It never wraps a restrict or edit
//! error, which are neither: a restrict failure is carried as its
//! [`ErrorAt`] position and its [`ErrorDetail`] projection, and an edit
//! failure as its [`ErrorDetail`].
//!
//! [`Residual`] is not an error: it states that the whole-state path would
//! perform an operation which the segmented path declines within its
//! declared bounds.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::ids::{IdKind, IdRange, SegmentId};
use super::store::{ArtifactKind, Phase};
use crate::error_detail::{ErrorDetail, PutDomainError};
use crate::ginstance::EdgeId;
use crate::identity::{CanonicalSink, Digest, IdentityError};
use crate::locality::{GlobalReason, SegmentOp};
use crate::view_ref::ViewRef;
use panproto_schema::InstanceShapeKind;

/// The position of an error in the graph engine's precedence order.
///
/// Errors compare by `stage` (0 node survival and transforms, 1 edges, 2
/// contraction pairs, 3 fans, 4 a pruned root of segmented tree execution),
/// then by `kind` within the stage, then by `id` and `id2` (the node, the
/// edge id or the pair the error is about). The order is total, so the
/// first error of a whole instance is the least of the first errors of its
/// segments.
///
/// Its canonical bytes are the four integers in field order.
///
/// ```
/// use panproto_inst::segment::ErrorKey;
///
/// let node = ErrorKey { stage: 0, kind: 3, id: 90, id2: 0 };
/// let edge = ErrorKey { stage: 1, kind: 0, id: 2, id2: 0 };
/// assert!(node < edge);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ErrorKey {
    /// The engine stage the error arises in.
    pub stage: u8,
    /// The error's kind within its stage.
    pub kind: u8,
    /// The node, edge id or pair source the error is about.
    pub id: u32,
    /// The pair target, or 0.
    pub id2: u32,
}

impl ErrorKey {
    /// Write the key's canonical bytes: `stage`, `kind`, `id`, `id2`.
    pub fn write<S: CanonicalSink + ?Sized>(&self, sink: &mut S) {
        sink.tag(self.stage);
        sink.tag(self.kind);
        sink.u32(self.id);
        sink.u32(self.id2);
    }
}

/// The first error of one segment: the segment and the error's
/// [`ErrorKey`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ErrorAt {
    /// The segment the error arises in.
    pub segment: SegmentId,
    /// The error's position in the precedence order.
    pub key: ErrorKey,
}

/// A resource a segmented run is budgeted in.
///
/// Serialized in `snake_case`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    /// The bytes one task may hold.
    WorkingSet,
    /// The bytes of one complement segment.
    Complement,
    /// The bytes of the coordinator's summaries.
    Summary,
    /// The number of segments one edit may touch.
    AffectedSegments,
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::WorkingSet => "working set",
            Self::Complement => "complement",
            Self::Summary => "summary",
            Self::AffectedSegments => "affected segments",
        })
    }
}

/// An operation the whole-state path would perform and the segmented path
/// declines within its declared bounds.
///
/// A residual never stands in for an error on invalid input. Serialized
/// externally tagged in `snake_case`, for instance
/// `{"global": {"op": "get", "reason": "pi_product"}}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Residual {
    /// The operation is global: no segment-wise evaluation exists.
    Global {
        /// The operation.
        op: SegmentOp,
        /// Why it is global.
        reason: GlobalReason,
    },
    /// The operation needs more of a resource than the budget grants.
    BudgetExceeded {
        /// The resource.
        resource: Resource,
        /// The phase that needs it.
        phase: Phase,
        /// The segment that needs it, or `None` for the coordinator.
        segment: Option<SegmentId>,
        /// The amount needed.
        needed: u64,
        /// The amount granted.
        budget: u64,
    },
}

impl fmt::Display for Residual {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Global { op, reason } => {
                write!(f, "{op:?} is global on this lens: {reason:?}")
            }
            Self::BudgetExceeded {
                resource,
                phase,
                segment,
                needed,
                budget,
            } => {
                write!(f, "{phase:?}")?;
                if let Some(segment) = segment {
                    write!(f, " of {segment}")?;
                }
                write!(
                    f,
                    " needs {needed} of {resource}, the budget grants {budget}"
                )
            }
        }
    }
}

/// What is wrong with a partition, or with the set of segments laid out
/// under one, beyond overlapping or inverted ranges.
///
/// Serialized in `snake_case`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionDefect {
    /// The spec at this position carries another id: segments are numbered
    /// densely from 0 in the order the partition lists them.
    NotDense {
        /// The id the spec carries.
        found: SegmentId,
    },
    /// The segment's node ranges are not in ascending order.
    UnsortedRanges,
    /// The segment's node allocation range is not inside its node ranges.
    AllocationOutsideNodes,
    /// A table partition gives the segment an edge allocation range; table
    /// pairs have no edge ids.
    EdgeAllocationOnTable,
    /// No segment of the partition's spec was supplied.
    SegmentMissing,
    /// The segment was supplied more than once.
    SegmentRepeated,
    /// A segment was supplied whose id the partition does not list.
    UnknownSegment,
    /// A tree segment holds an embedding counter (`next_node` or
    /// `next_edge`) but not the tree's root; only the segment owning the
    /// root holds the counters.
    CountersWithoutRoot,
}

impl fmt::Display for PartitionDefect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotDense { found } => write!(f, "its spec carries the id {}", found.0),
            Self::UnsortedRanges => f.write_str("its node ranges are not ascending"),
            Self::AllocationOutsideNodes => {
                f.write_str("its node allocation range is not inside its node ranges")
            }
            Self::EdgeAllocationOnTable => {
                f.write_str("it has an edge allocation range in a table partition")
            }
            Self::SegmentMissing => f.write_str("it was not supplied"),
            Self::SegmentRepeated => f.write_str("it was supplied more than once"),
            Self::UnknownSegment => f.write_str("the partition does not list it"),
            Self::CountersWithoutRoot => {
                f.write_str("it holds an embedding counter but not the tree's root")
            }
        }
    }
}

/// The typed failure of a segmented operation.
///
/// Serialized externally tagged in `snake_case`, for instance
/// `{"allocation_exhausted": {"segment": 2, "kind": "edge"}}`. No variant
/// holds free text except [`SegmentError::Decode`]'s message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum SegmentError {
    /// A range of `segment` has `lo > hi`.
    #[error("{segment} has the inverted id range {range}")]
    InvertedRange {
        /// The segment the range belongs to.
        segment: SegmentId,
        /// The inverted range.
        range: IdRange,
    },

    /// Two segments (or one segment twice) claim a common id, in their node
    /// ranges or in their edge allocation ranges.
    #[error("the id ranges of {a} and {b} overlap")]
    OverlappingRanges {
        /// The lesser segment.
        a: SegmentId,
        /// The greater segment.
        b: SegmentId,
    },

    /// An element carries an id no range of the partition contains.
    #[error("{elem:?} carries an id no segment owns")]
    Unowned {
        /// The element.
        elem: ViewRef,
    },

    /// An element is held by a segment other than its owner.
    #[error("{elem:?} is owned by {expected} but held by {found}")]
    WrongOwner {
        /// The element.
        elem: ViewRef,
        /// The segment that owns it.
        expected: SegmentId,
        /// The segment that holds it.
        found: SegmentId,
    },

    /// A ghost does not match the remote node it stands for: it names the
    /// wrong owner or anchor, no owned edge reaches it, or an owned edge
    /// reaches a remote node without one.
    #[error("the ghost of node {node} does not match its owner")]
    GhostMismatch {
        /// The remote node.
        node: u32,
    },

    /// An in-stub does not match the edge it stands for: its target is not
    /// owned here, it names the wrong source owner, or its owner holds no
    /// such edge.
    #[error("the in-stub of {elem:?} does not match the edge its owner holds")]
    InStubMismatch {
        /// The edge.
        elem: ViewRef,
    },

    /// One edge id is held by more than one owned edge.
    #[error("edge id {edge} is held in {segments:?}")]
    DuplicateEdgeId {
        /// The repeated edge id.
        edge: EdgeId,
        /// The segments holding it, ascending; one segment is listed once
        /// for each time it holds the id.
        segments: Vec<SegmentId>,
    },

    /// The cross digest that segment `from` records for its edges into
    /// `to` differs from the one `to` records for them, or one side has
    /// none.
    #[error("the cross references from {from} into {to} disagree")]
    CrossMismatch {
        /// The segment owning the edges.
        from: SegmentId,
        /// The segment owning their targets.
        to: SegmentId,
    },

    /// A digest differs from the one recorded: a segment's content, a
    /// manifest record, or the manifest root (`segment` is `None`).
    #[error("digest mismatch: expected {expected}, found {found}")]
    DigestMismatch {
        /// The segment, or `None` for the manifest as a whole.
        segment: Option<SegmentId>,
        /// The recorded digest.
        expected: Digest,
        /// The digest computed.
        found: Digest,
    },

    /// A segment was written under another schema than the one expected.
    #[error("schema mismatch: expected {expected}, found {found}")]
    SchemaMismatch {
        /// The schema digest expected.
        expected: Digest,
        /// The schema digest the segment carries.
        found: Digest,
    },

    /// An instance, segment or manifest of another shape than the one
    /// expected.
    #[error("shape mismatch: expected {expected:?}, found {found:?}")]
    ShapeMismatch {
        /// The shape expected.
        expected: InstanceShapeKind,
        /// The shape found.
        found: InstanceShapeKind,
    },

    /// `segment` must allocate ids of `kind` but its partition assigns it no
    /// allocation range for them.
    #[error("{segment} must allocate {kind} ids but has no {kind} allocation range")]
    NoAllocationRange {
        /// The segment.
        segment: SegmentId,
        /// The kind of id it must allocate.
        kind: IdKind,
    },

    /// Every id of `segment`'s allocation range for `kind` is at or below an
    /// id in use.
    #[error("{segment} has no free {kind} id left in its allocation range")]
    AllocationExhausted {
        /// The segment.
        segment: SegmentId,
        /// The kind of id it failed to allocate.
        kind: IdKind,
    },

    /// `segment`'s allocation range for `kind` does not lie above every id of
    /// that kind in the input.
    #[error("the {kind} allocation range of {segment} does not lie above every {kind} id in use")]
    AllocationBelowFloor {
        /// The segment.
        segment: SegmentId,
        /// The kind of id.
        kind: IdKind,
    },

    /// A fresh edge of `segment` carries an id outside its edge allocation
    /// range.
    #[error("edge id {edge} lies outside the edge allocation range of {segment}")]
    EdgeIdOutsideAllocation {
        /// The edge id.
        edge: EdgeId,
        /// The segment.
        segment: SegmentId,
    },

    /// A segment breaks its local contract; every violation, sorted by the
    /// element it is about.
    #[error("the segment breaks its contract in {} place(s)", violations.len())]
    Invalid {
        /// The violations, sorted.
        violations: Vec<Self>,
    },

    /// A restrict failed. `at` is its position in the precedence order and
    /// `detail` its structured projection.
    #[error("restrict failed in {} at {:?}", at.segment, at.key)]
    Restrict {
        /// Where the first error arises.
        at: ErrorAt,
        /// The error's structured detail.
        detail: Box<ErrorDetail>,
    },

    /// A view lies outside the domain of `put`, for every reason found.
    #[error("the view lies outside the domain of put ({} reason(s))", .0.len())]
    PutDomain(Vec<PutDomainError>),

    /// An edit failed in `segment`.
    #[error("an edit failed in {segment}")]
    Edit {
        /// The segment the edit failed in.
        segment: SegmentId,
        /// The error's structured detail.
        detail: Box<ErrorDetail>,
    },

    /// A complement or manifest identity was refused.
    #[error(transparent)]
    Identity(IdentityError),

    /// Stored bytes could not be read as the artifact they were stored as.
    #[error("cannot decode the {artifact:?} artifact: {message}")]
    Decode {
        /// The kind of artifact.
        artifact: ArtifactKind,
        /// What is wrong with the bytes.
        message: String,
    },

    /// A partition, or the segments laid out under one, is malformed in a
    /// way no other variant names.
    #[error("{segment} is malformed: {defect}")]
    MalformedPartition {
        /// The segment the defect is about.
        segment: SegmentId,
        /// The defect.
        defect: PartitionDefect,
    },

    /// An owned edge or an in-stub names an endpoint that this segment owns
    /// but does not hold.
    #[error("{elem:?} names node {node}, which its owner does not hold")]
    DanglingEndpoint {
        /// The edge.
        elem: ViewRef,
        /// The missing endpoint.
        node: u32,
    },

    /// A tree and its graph embedding, or a tree segment, do not describe
    /// one tree at `elem`: an arc or fan without embedding ids, arcs or fans
    /// out of embedding order, a node with two parents, a node the root does
    /// not reach, or a fan whose reified edges disagree with its record.
    #[error("{elem:?} is not embedded as one tree")]
    NotEmbedded {
        /// The element.
        elem: ViewRef,
    },
}

impl SegmentError {
    /// A short name of the error's variant, as its serialized tag spells it.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::InvertedRange { .. } => "inverted_range",
            Self::OverlappingRanges { .. } => "overlapping_ranges",
            Self::Unowned { .. } => "unowned",
            Self::WrongOwner { .. } => "wrong_owner",
            Self::GhostMismatch { .. } => "ghost_mismatch",
            Self::InStubMismatch { .. } => "in_stub_mismatch",
            Self::DuplicateEdgeId { .. } => "duplicate_edge_id",
            Self::CrossMismatch { .. } => "cross_mismatch",
            Self::DigestMismatch { .. } => "digest_mismatch",
            Self::SchemaMismatch { .. } => "schema_mismatch",
            Self::ShapeMismatch { .. } => "shape_mismatch",
            Self::NoAllocationRange { .. } => "no_allocation_range",
            Self::AllocationExhausted { .. } => "allocation_exhausted",
            Self::AllocationBelowFloor { .. } => "allocation_below_floor",
            Self::EdgeIdOutsideAllocation { .. } => "edge_id_outside_allocation",
            Self::Invalid { .. } => "invalid",
            Self::Restrict { .. } => "restrict",
            Self::PutDomain(_) => "put_domain",
            Self::Edit { .. } => "edit",
            Self::Identity(_) => "identity",
            Self::Decode { .. } => "decode",
            Self::MalformedPartition { .. } => "malformed_partition",
            Self::DanglingEndpoint { .. } => "dangling_endpoint",
            Self::NotEmbedded { .. } => "not_embedded",
        }
    }
}
