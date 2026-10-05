//! Data carried by reference-resolution lenses.
//!
//! This module contains only serializable carriers.  Execution and provider
//! interfaces live in `panproto-io`, keeping the instance crate independent
//! of codecs while allowing complements to participate in durable identity.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_schema::{Edge, InstanceShapeKind, Reference, Schema};
use serde::{Deserialize, Serialize};

use crate::fan::Fan;
use crate::identity::{ComplementShape, Digest, IdentityCarrier, IdentityEnvelope};
use crate::metadata::Node;
use crate::value::Value;
use crate::view_ref::{SynthIndex, SynthKey, ViewRef};
use crate::{FInstance, GInstance, WInstance};

/// The syntactic-to-semantic shape change performed by a resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionKind {
    /// Resolve a nested document into a graph.
    TreeToGraph,
    /// Resolve a relational instance into a graph.
    TableToGraph,
    /// Assemble a nested document into tables.
    TreeToTable,
    /// Assemble tables into a nested document.
    TableToTree,
}

impl ResolutionKind {
    /// The kind's syntactic shape.
    #[must_use]
    pub const fn syntactic_shape(self) -> InstanceShapeKind {
        match self {
            Self::TreeToGraph | Self::TreeToTable => InstanceShapeKind::Tree,
            Self::TableToGraph | Self::TableToTree => InstanceShapeKind::Table,
        }
    }

    /// The kind's semantic shape.
    #[must_use]
    pub const fn semantic_shape(self) -> InstanceShapeKind {
        match self {
            Self::TreeToGraph | Self::TableToGraph => InstanceShapeKind::Graph,
            Self::TreeToTable => InstanceShapeKind::Table,
            Self::TableToTree => InstanceShapeKind::Tree,
        }
    }
}

/// A semantic, versioned resolution-provider identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResolutionProviderId {
    /// Provider name.
    pub name: Name,
    /// Version of the provider's semantic behavior.
    pub version: u16,
}

/// A semantic, versioned assembly grammar identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AssemblyPlanId {
    /// Grammar name.
    pub grammar: Name,
    /// Version of the grammar and its inverse.
    pub version: u16,
}

/// A document-varying service a resolution requires.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionContextKind {
    /// Bytes loaded from outside the parsed document.
    ExternalText,
    /// Token boundaries over a UTF-8 byte string.
    TokenBoundaries,
}

/// The immutable, identity-bearing plan produced by a resolution provider.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolutionSpec {
    /// Shape change performed by the plan.
    pub kind: ResolutionKind,
    /// Provider whose behavior the plan relies on.
    pub provider: ResolutionProviderId,
    /// Schema of the resolved view.
    pub semantic_schema: Schema,
    /// Provider-level references merged with schema declarations.
    pub protocol_references: BTreeMap<Name, Reference>,
    /// Semantic edges eligible for structural placement.
    pub structural_edges: BTreeSet<Edge>,
    /// Unique syntactic reference declaration used to write each edge.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub reference_writers: BTreeMap<Edge, Name>,
    /// Document-varying services required during execution.
    pub context_requirements: BTreeSet<ResolutionContextKind>,
    /// Versioned grammar for an assembly kind.
    pub assembly: Option<AssemblyPlanId>,
}

/// One of the three instance carriers accepted by resolution APIs.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnyInstance {
    /// A tree-shaped instance.
    Tree(WInstance),
    /// A table-shaped instance.
    Table(FInstance),
    /// A graph-shaped instance.
    Graph(GInstance),
}

impl AnyInstance {
    /// The instance's shape.
    #[must_use]
    pub const fn shape(&self) -> InstanceShapeKind {
        match self {
            Self::Tree(_) => InstanceShapeKind::Tree,
            Self::Table(_) => InstanceShapeKind::Table,
            Self::Graph(_) => InstanceShapeKind::Graph,
        }
    }

    /// Borrow the tree carrier, when this is a tree.
    #[must_use]
    pub const fn as_tree(&self) -> Option<&WInstance> {
        match self {
            Self::Tree(value) => Some(value),
            Self::Table(_) | Self::Graph(_) => None,
        }
    }

    /// Borrow the table carrier, when this is a table.
    #[must_use]
    pub const fn as_table(&self) -> Option<&FInstance> {
        match self {
            Self::Table(value) => Some(value),
            Self::Tree(_) | Self::Graph(_) => None,
        }
    }

    /// Borrow the graph carrier, when this is a graph.
    #[must_use]
    pub const fn as_graph(&self) -> Option<&GInstance> {
        match self {
            Self::Graph(value) => Some(value),
            Self::Tree(_) | Self::Table(_) => None,
        }
    }
}

impl From<WInstance> for AnyInstance {
    fn from(value: WInstance) -> Self {
        Self::Tree(value)
    }
}

impl From<FInstance> for AnyInstance {
    fn from(value: FInstance) -> Self {
        Self::Table(value)
    }
}

impl From<GInstance> for AnyInstance {
    fn from(value: GInstance) -> Self {
        Self::Graph(value)
    }
}

/// The namespace and target of one variable definition.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DefinitionKey {
    /// Reference declaration, and thus variable namespace.
    pub reference: Name,
    /// Semantic target node.
    pub target: u32,
}

/// The source record corresponding to one resolved occurrence.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefOccurrence {
    /// Syntactic element holding the occurrence.
    pub holder: ViewRef,
    /// Reference declaration used for it.
    pub reference: Name,
    /// Position among this declaration's occurrences at the holder.
    pub index: u32,
    /// Exact syntactic literal.
    pub literal: Value,
    /// Provider-specific historical spelling.
    pub form: Option<String>,
    /// Digest of document-varying external state used to resolve it.
    pub context: Option<Digest>,
}

/// Why one syntactic occurrence was not resolved.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedReason {
    /// No target matched.
    Dangling,
    /// More than one target matched.
    Ambiguous {
        /// Matching candidates, in ascending order.
        candidates: Vec<ViewRef>,
    },
}

/// One occurrence retained because it could not be resolved.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnresolvedRef {
    /// Syntactic element holding the occurrence.
    pub holder: ViewRef,
    /// Reference declaration used for it.
    pub reference: Name,
    /// Position among this declaration's occurrences at the holder.
    pub index: u32,
    /// Exact syntactic literal.
    pub literal: Value,
    /// Resolution failure.
    pub reason: UnresolvedReason,
    /// Digest of document-varying external state used to resolve it.
    pub context: Option<Digest>,
}

/// A materialized standoff span in a semantic graph.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResolutionSpan {
    /// Text carrier indexed by the span.
    pub text: ViewRef,
    /// Inclusive start byte offset.
    pub start: u64,
    /// Exclusive end byte offset.
    pub end: u64,
}

/// Where a redirected fragment was attached in its source tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedirectedAttachment {
    /// Parent node outside or at the boundary of the fragment.
    pub parent: u32,
    /// Attached child.
    pub child: u32,
    /// Exact schema edge of the attachment.
    pub edge: Edge,
    /// Position of the arc among the source's arcs.
    pub source_slot: u32,
}

/// One labelled arc retained inside a redirected fragment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedirectedArc {
    /// Parent node.
    pub parent: u32,
    /// Child node.
    pub child: u32,
    /// Exact schema edge.
    pub edge: Edge,
    /// Position of the arc among the source's arcs.
    pub source_slot: u32,
}

/// Exact syntactic structure removed by a redirecting reference.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RedirectedFragment {
    /// Root of the removed fragment.
    pub root: u32,
    /// Attachment to the retained tree, absent when the fragment held its root.
    pub attachment: Option<RedirectedAttachment>,
    /// Removed nodes, keyed by id.
    pub nodes: BTreeMap<u32, Node>,
    /// Removed arcs in source-slot order.
    pub arcs: Vec<RedirectedArc>,
    /// Removed fans in canonical source order.
    pub fans: Vec<Fan>,
}

/// One assembly production's source-to-view correspondence.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssemblyTrace {
    /// Grammar production.
    pub production: Name,
    /// Source carriers consumed by the production.
    pub source: Vec<ViewRef>,
    /// View carriers produced by the production.
    pub view: Vec<ViewRef>,
}

/// Conservative lossless residue of an assembly provider.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssemblyResidue {
    /// Exact syntactic source snapshot.
    pub source: Box<AnyInstance>,
    /// Sorted production correspondences.
    pub trace: Vec<AssemblyTrace>,
    /// Digest of document-varying state used by the assembly.
    pub context: Option<Digest>,
}

/// Everything discarded or chosen by a reference resolution.
///
/// Fields are deliberately serialized in declaration order. Maps whose keys
/// are structured values use pair arrays so their JSON representation is
/// lossless and deterministic.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResolutionComplement {
    /// Source occurrences, keyed by their semantic carrier.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub occurrences: BTreeMap<ViewRef, Vec<RefOccurrence>>,
    /// Variable definitions, separated by reference namespace.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub definitions: BTreeMap<DefinitionKey, ViewRef>,
    /// Exact fragments removed by redirecting references.
    pub redirected: BTreeMap<u32, RedirectedFragment>,
    /// Original child order for each retained source parent.
    pub order: BTreeMap<u32, Vec<ViewRef>>,
    /// Structural parent selected for each non-root semantic node.
    pub spanning: BTreeMap<u32, ViewRef>,
    /// Materialized span nodes.
    pub span_nodes: BTreeMap<u32, ResolutionSpan>,
    /// Occurrences retained after a lenient or complementing failure.
    pub unresolved: Vec<UnresolvedRef>,
    /// Assembly-provider residue, present exactly for assembly kinds.
    pub assembly: Option<AssemblyResidue>,
    /// Durable identity of the resolution that produced this complement.
    pub identity: Option<IdentityEnvelope>,
}

impl ResolutionComplement {
    /// Whether the complement contains no residue or resolution state.
    #[must_use]
    pub fn records_nothing(&self) -> bool {
        self.occurrences.is_empty()
            && self.definitions.is_empty()
            && self.redirected.is_empty()
            && self.order.is_empty()
            && self.spanning.is_empty()
            && self.span_nodes.is_empty()
            && self.unresolved.is_empty()
            && self.assembly.is_none()
    }

    /// Stable names for every node and edge synthesized by resolution.
    ///
    /// One resolved occurrence creates one semantic edge. Materialized
    /// standoff spans create semantic nodes. Numeric ids are only the current
    /// presentation of these keys.
    #[must_use]
    pub fn synth_index(&self) -> SynthIndex {
        let edges = self
            .occurrences
            .iter()
            .filter_map(|(carrier, occurrences)| {
                let ViewRef::Edge(id) = carrier else {
                    return None;
                };
                let occurrence = occurrences.first()?;
                Some((
                    *id,
                    SynthKey::ResolutionReference {
                        holder: occurrence.holder.clone(),
                        reference: occurrence.reference.clone(),
                        index: occurrence.index,
                    },
                ))
            })
            .collect();
        let nodes = self
            .span_nodes
            .iter()
            .map(|(&id, span)| {
                (
                    id,
                    SynthKey::ResolutionSpan {
                        text: span.text.clone(),
                        start: span.start,
                        end: span.end,
                    },
                )
            })
            .collect();
        SynthIndex { nodes, edges }
    }
}

impl IdentityCarrier for ResolutionComplement {
    const SHAPE: ComplementShape = ComplementShape::Resolution;

    fn identity(&self) -> Option<&IdentityEnvelope> {
        self.identity.as_ref()
    }

    fn set_identity(&mut self, envelope: Option<IdentityEnvelope>) {
        self.identity = envelope;
    }

    fn legacy_fingerprint(&self) -> u64 {
        0
    }

    fn set_legacy_fingerprint(&mut self, _fingerprint: u64) {}

    fn records_nothing(&self) -> bool {
        ResolutionComplement::records_nothing(self)
    }
}
