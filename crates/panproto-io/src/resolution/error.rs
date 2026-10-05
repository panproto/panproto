//! Closed, serializable failures for reference resolution.

use panproto_gat::Name;
use panproto_inst::{
    Digest, IdKind, IdentityError, InstanceShapeKind, ResolutionKind, ResolutionProviderId, Value,
    ValueFidelityError, ViewRef,
};
use panproto_schema::{KeyRefError, PathSyntax};
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
    /// A resolution complement on a native, non-resolution route.
    ResolutionState,
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
    /// A semantic reference carrier has no declared writer.
    #[error("reference {reference} cannot represent {carrier:?}")]
    UnrepresentableReference {
        /// Semantic carrier.
        carrier: ViewRef,
        /// Reference declaration.
        reference: Name,
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
#[derive(Clone, Debug, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum ResolveError {
    /// A reference matched no target.
    #[error("dangling reference {reference} at {at:?}")]
    DanglingReference {
        /// Reference declaration.
        reference: Name,
        /// Holder.
        at: ViewRef,
        /// Canonical key tuple.
        key: Vec<Value>,
    },
    /// A reference matched more than one target.
    #[error("ambiguous reference {reference} at {at:?}")]
    AmbiguousReference {
        /// Reference declaration.
        reference: Name,
        /// Holder.
        at: ViewRef,
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
        at: ViewRef,
        /// Start offset.
        start: u64,
        /// End offset.
        end: u64,
        /// Text length in the declared unit.
        len: u64,
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
        at: ViewRef,
        /// Length of each part.
        lengths: Vec<u64>,
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
        at: Option<ViewRef>,
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
        at: Option<ViewRef>,
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
