//! Closed, serializable failures for reference resolution.

use panproto_gat::Name;
use panproto_inst::segment::IdKind;
use panproto_inst::{
    Digest, IdentityError, InstanceShapeKind, ResolutionKind, ResolutionProviderId, Value,
    ValueFidelityError, ViewRef,
};
use panproto_schema::{KeyRefError, OffsetUnit, PathSyntax};
use serde::{Deserialize, Serialize};

/// A provider capability needed by an operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionFeature {
    /// Parsing or rendering a path syntax.
    Path(PathSyntax),
    /// Loading externally stored text.
    ExternalText,
    /// Mapping token offsets to UTF-8 byte offsets.
    TokenBoundaries,
    /// A reversible assembly grammar.
    Assembly(ResolutionKind),
    /// A declared writer for a semantic reference edge.
    ReferenceWriter,
    /// A resolution complement on a route with no registered resolution.
    ResolutionState,
    /// Offsets counted over a normalized text (folded line endings, a
    /// normalization form), which the exact byte form cannot represent.
    NormalizedOffsets,
    /// An identified inline occurrence (`RefSource::Inline`), a
    /// declaration form this engine does not resolve.
    InlineReference,
    /// A reference whose target vertex is a declared target set.
    TargetSet,
    /// A reference that declares a null vertex for its sequence.
    NullVertex,
    /// A reference whose scope is reached through other references.
    ScopeVia,
    /// A standoff reference whose spans come from ordered fragments.
    StandoffFragments,
    /// Standoff text relative to another standoff span (`RefTextSource::Span`).
    SpanText,
    /// Standoff text reached through another reference
    /// (`RefTextSource::Referenced`).
    ReferencedText,
    /// External standoff text named by a URI under a base
    /// (`RefTextSource::ExternalUri`).
    ExternalUriText,
}

/// Why an offset is not at a unit boundary of its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffsetUnitReason {
    /// The offset falls inside a unit: inside a UTF-8 scalar, inside a
    /// UTF-16 surrogate pair, or between two token boundaries.
    NotOnBoundary,
    /// The unit counts characters and the text is not UTF-8.
    TextNotUtf8,
    /// An inclusive declaration cannot denote an empty span.
    EmptyInclusive,
}

/// Why a declared key equality cannot be evaluated during resolution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyEqualityReason {
    /// The declaration names a collation whose kernel is not available.
    UnsupportedCollation(Name),
    /// The equality vector or value tuple has the wrong arity.
    EqualityArity {
        /// Required number of parts.
        expected: usize,
        /// Supplied number of parts.
        found: usize,
    },
    /// A numeric value violates the invariant of its representation.
    InvalidNumericRepresentation {
        /// Name of the invalid representation.
        kind: String,
    },
    /// A keyed value has no canonical bytes, such as a temporal value
    /// outside the admitted carrier.
    NoCanonicalBytes,
}

/// A serializable instance-shape failure at the resolution boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionInstanceError {
    /// A complement names a carrier absent from the supplied instance.
    #[error("missing carrier {carrier:?}")]
    MissingCarrier {
        /// Missing carrier.
        carrier: ViewRef,
    },
    /// A carrier exists but one of its endpoints is absent.
    #[error("dangling carrier {carrier:?}")]
    DanglingCarrier {
        /// Dangling carrier.
        carrier: ViewRef,
    },
    /// A graph cannot be read as a tree.
    #[error("the graph is not tree-shaped")]
    NotTree,
    /// A graph cannot be read as a table.
    #[error("the graph is not table-shaped")]
    NotTable,
    /// A value cannot be read as a graph.
    #[error("the instance is not graph-shaped")]
    NotGraph,
}

/// Why an assembly edit is outside its provider's put domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyDomainReason {
    /// No grammar production can place the carrier.
    MissingProduction,
    /// More than one grammar production can place the carrier.
    AmbiguousProduction,
    /// The residue and edited view no longer describe one assembly state.
    ResidueMismatch,
}

/// One complete, typed reason an edited view cannot be put.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum ResolveDomainError {
    /// A carrier has no legal syntactic placement.
    #[error("no syntactic placement for {carrier:?}")]
    NoPlacement {
        /// Unplaceable carrier.
        carrier: ViewRef,
    },
    /// More than one reference declaration could place a carrier.
    #[error("ambiguous syntactic placement for {carrier:?}")]
    AmbiguousPlacement {
        /// Ambiguous carrier.
        carrier: ViewRef,
        /// Candidate declarations, in ascending order.
        references: Vec<Name>,
    },
    /// A reference declaration cannot write the carrier so that the
    /// written source resolves back to it.
    #[error("reference {reference} cannot represent {carrier:?}")]
    UnrepresentableReference {
        /// Semantic carrier.
        carrier: ViewRef,
        /// Reference declaration.
        reference: Name,
    },
    /// A span no longer lies within its text after a text edit.
    #[error("span {carrier:?} of {reference} is out of range of its text")]
    OffsetOutOfRange {
        /// The span node or the reference edge.
        carrier: ViewRef,
        /// Reference declaration.
        reference: Name,
    },
    /// A span is no longer on unit boundaries of its text after a text
    /// edit, or cannot be written in its declared unit.
    #[error("span {carrier:?} of {reference} is not on a unit boundary: {reason:?}")]
    OffsetUnitViolation {
        /// The span node or the reference edge.
        carrier: ViewRef,
        /// Reference declaration.
        reference: Name,
        /// Why the offset is not on a boundary.
        reason: OffsetUnitReason,
    },
    /// A synthesized carrier was edited independently of its stable key.
    #[error("synthesized carrier {carrier:?} was edited incoherently")]
    SynthesizedEdited {
        /// Edited synthesized carrier.
        carrier: ViewRef,
    },
    /// External state differs from the state used by get.
    #[error("resolution context changed at {at:?}")]
    ContextMismatch {
        /// Affected carrier, when local.
        at: Option<ViewRef>,
    },
    /// An assembly provider refused an edit.
    #[error("assembly domain failure at {at:?} for {production:?}: {reason:?}")]
    Assembly {
        /// Affected carrier, when local.
        at: Option<ViewRef>,
        /// Affected production, when known.
        production: Option<Name>,
        /// Refusal reason.
        reason: AssemblyDomainReason,
    },
}

/// Every runtime failure exposed by resolution APIs.
///
/// Carriers are boxed so that the error stays small; a box serializes as
/// the value it holds, so the wire form is that of the unboxed fields.
#[derive(Clone, Debug, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum ResolveError {
    /// A reference matched no target.
    #[error("dangling reference {reference} at {at:?}")]
    DanglingReference {
        /// Reference declaration.
        reference: Name,
        /// Holder.
        at: Box<ViewRef>,
        /// Canonical key tuple.
        key: Vec<Value>,
    },
    /// A reference matched more than one target.
    #[error("ambiguous reference {reference} at {at:?}")]
    AmbiguousReference {
        /// Reference declaration.
        reference: Name,
        /// Holder.
        at: Box<ViewRef>,
        /// Canonical key tuple.
        key: Vec<Value>,
        /// Matching candidates, in ascending order.
        candidates: Vec<ViewRef>,
    },
    /// A standoff interval is not a valid interval of its text.
    #[error("offset interval {start}..{end} for {reference} at {at:?} exceeds {len}")]
    OffsetOutOfRange {
        /// Reference declaration.
        reference: Name,
        /// Holder.
        at: Box<ViewRef>,
        /// Start offset.
        start: u64,
        /// End offset.
        end: u64,
        /// Text length in the declared unit.
        len: u64,
    },
    /// A standoff offset is not on a unit boundary of its text.
    #[error("offset {offset} of {reference} at {at:?} in unit {unit:?}: {reason:?}")]
    OffsetUnitViolation {
        /// Reference declaration.
        reference: Name,
        /// Holder.
        at: Box<ViewRef>,
        /// The declared unit.
        unit: OffsetUnit,
        /// The offset, in the declared unit.
        offset: u64,
        /// Why the offset is refused.
        reason: OffsetUnitReason,
    },
    /// An operation received the wrong instance shape.
    #[error("expected {expected:?}, found {found:?}")]
    ShapeMismatch {
        /// Required shape.
        expected: InstanceShapeKind,
        /// Supplied shape.
        found: InstanceShapeKind,
    },
    /// Complement identity did not match this compiled resolution.
    #[error(transparent)]
    Identity(#[from] IdentityError),
    /// No resolution kind implements this shape change.
    #[error("unsupported resolution from {syntactic:?} to {semantic:?}")]
    UnsupportedKind {
        /// Syntactic shape.
        syntactic: InstanceShapeKind,
        /// Semantic shape.
        semantic: InstanceShapeKind,
    },
    /// Schema and provider declarations disagree under one id.
    #[error("conflicting reference declaration {reference}")]
    ReferenceConflict {
        /// Conflicting id.
        reference: Name,
    },
    /// Composite many-valued parts have different lengths.
    #[error("reference {reference} at {at:?} has part lengths {lengths:?}")]
    ReferenceCardinalityMismatch {
        /// Reference declaration.
        reference: Name,
        /// Holder.
        at: Box<ViewRef>,
        /// Length of each part.
        lengths: Vec<u64>,
    },
    /// A target key's declared equality cannot be evaluated exactly.
    #[error("key equality for {key} cannot be evaluated: {reason:?}")]
    KeyEquality {
        /// Target key declaration.
        key: Name,
        /// Exact refusal reason.
        reason: KeyEqualityReason,
    },
    /// No provider is registered for a non-native route.
    #[error("no resolution provider for protocol {protocol}")]
    ProviderUnavailable {
        /// Canonical protocol name.
        protocol: String,
    },
    /// The provider does not implement a required capability.
    #[error("unsupported resolution feature {feature:?}")]
    UnsupportedFeature {
        /// Provider, absent for a native registry route.
        provider: Option<ResolutionProviderId>,
        /// Missing capability.
        feature: ResolutionFeature,
    },
    /// Document-varying state changed since get.
    #[error("resolution context mismatch at {at:?}")]
    ContextMismatch {
        /// Affected carrier, when local.
        at: Option<Box<ViewRef>>,
        /// Digest recorded by get.
        expected: Digest,
        /// Digest recomputed by put.
        found: Option<Digest>,
    },
    /// A checked synthesized-id allocator is exhausted.
    #[error("{kind} ids exhausted")]
    IdExhausted {
        /// Exhausted id family.
        kind: IdKind,
    },
    /// A key or reference declaration is malformed.
    #[error("schema reference error: {0}")]
    Schema(KeyRefError),
    /// A value cannot be represented without loss.
    #[error(transparent)]
    Fidelity(ValueFidelityError),
    /// An instance carrier is malformed for the requested conversion.
    #[error(transparent)]
    Instance(ResolutionInstanceError),
    /// A provider returned a stable typed failure code.
    #[error("provider {provider:?} failed with {code} at {at:?}")]
    ProviderFailure {
        /// Provider identity.
        provider: ResolutionProviderId,
        /// Provider-defined stable code.
        code: Name,
        /// Affected carrier, when local.
        at: Option<Box<ViewRef>>,
    },
    /// An edit lies outside the put domain.
    #[error(transparent)]
    PutDomain(ResolveDomainError),
}

impl From<KeyRefError> for ResolveError {
    fn from(value: KeyRefError) -> Self {
        Self::Schema(value)
    }
}

impl From<ValueFidelityError> for ResolveError {
    fn from(value: ValueFidelityError) -> Self {
        Self::Fidelity(value)
    }
}

impl ResolveDomainError {
    /// The carrier the violation names, if any.
    #[must_use]
    pub const fn carrier(&self) -> Option<&ViewRef> {
        match self {
            Self::NoPlacement { carrier }
            | Self::AmbiguousPlacement { carrier, .. }
            | Self::UnrepresentableReference { carrier, .. }
            | Self::OffsetOutOfRange { carrier, .. }
            | Self::OffsetUnitViolation { carrier, .. }
            | Self::SynthesizedEdited { carrier } => Some(carrier),
            Self::ContextMismatch { at } | Self::Assembly { at, .. } => at.as_ref(),
        }
    }

    /// The variant's position in declaration order.
    #[must_use]
    pub const fn variant_index(&self) -> u8 {
        match self {
            Self::NoPlacement { .. } => 0,
            Self::AmbiguousPlacement { .. } => 1,
            Self::UnrepresentableReference { .. } => 2,
            Self::OffsetOutOfRange { .. } => 3,
            Self::OffsetUnitViolation { .. } => 4,
            Self::SynthesizedEdited { .. } => 5,
            Self::ContextMismatch { .. } => 6,
            Self::Assembly { .. } => 7,
        }
    }

    /// The order of a violation vector: carrier (a violation without one
    /// first), then the reference or production it names, then variant.
    #[must_use]
    pub fn sort_key(&self) -> (Option<&ViewRef>, Option<&Name>, u8) {
        let name = match self {
            Self::AmbiguousPlacement { references, .. } => references.first(),
            Self::UnrepresentableReference { reference, .. }
            | Self::OffsetOutOfRange { reference, .. }
            | Self::OffsetUnitViolation { reference, .. } => Some(reference),
            Self::Assembly { production, .. } => production.as_ref(),
            Self::NoPlacement { .. }
            | Self::SynthesizedEdited { .. }
            | Self::ContextMismatch { .. } => None,
        };
        (self.carrier(), name, self.variant_index())
    }
}
