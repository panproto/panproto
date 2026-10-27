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
    /// A present guard value selected no declaration in its group.
    Unselected {
        /// Exact unmatched guard value.
        value: Value,
    },
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

/// One terminal value site of a syntactic instance.
///
/// A site is the exact place a path ends: the own value of a tree node
/// (`Own`), or a named field of a tree node or a cell of a table row
/// (`Field`). Two fields of one holder are distinct sites even when their
/// values are equal, so a site, unlike a value, never aliases another.
///
/// Serialized externally tagged in `snake_case`; the derived order compares
/// the variant first, in declaration order, and then the payload.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionValueSite {
    /// The own value of a node.
    Own(ViewRef),
    /// A named field of a node, or a cell of a row.
    Field {
        /// The node or row holding the field.
        holder: ViewRef,
        /// The field or column name.
        field: String,
    },
}

impl ResolutionValueSite {
    /// The node or row the site belongs to.
    #[must_use]
    pub const fn holder(&self) -> &ViewRef {
        match self {
            Self::Own(holder) | Self::Field { holder, .. } => holder,
        }
    }

    /// The same site on the holder `holder`.
    #[must_use]
    pub fn with_holder(&self, holder: ViewRef) -> Self {
        match self {
            Self::Own(_) => Self::Own(holder),
            Self::Field { field, .. } => Self::Field {
                holder,
                field: field.clone(),
            },
        }
    }
}

/// The identity of the text a standoff span indexes.
///
/// `Local` names the site whose value is the text itself; `ExternalKey`
/// names the site whose value is the key under which the text is loaded
/// from outside the document. The identity is the site, never the text or
/// the loaded bytes, so equal texts at distinct sites keep distinct spans.
///
/// Serialized externally tagged in `snake_case`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionTextId {
    /// Text held at a site of the document.
    Local(ResolutionValueSite),
    /// Text loaded under the key held at a site of the document.
    ExternalKey(ResolutionValueSite),
}

impl ResolutionTextId {
    /// The site the identity names.
    #[must_use]
    pub const fn site(&self) -> &ResolutionValueSite {
        match self {
            Self::Local(site) | Self::ExternalKey(site) => site,
        }
    }

    /// Whether the text is held locally.
    #[must_use]
    pub const fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// The same kind of identity at the site `site`.
    #[must_use]
    pub const fn with_site(&self, site: ResolutionValueSite) -> Self {
        match self {
            Self::Local(_) => Self::Local(site),
            Self::ExternalKey(_) => Self::ExternalKey(site),
        }
    }
}

/// A materialized standoff span in a semantic graph.
///
/// `start` and `end` are half-open byte offsets into the exact text: the
/// UTF-8 bytes of a local string value, the bytes of a local byte-string
/// value, or the bytes loaded for an external key. Every declared offset
/// unit converts to this form before a span is keyed, so declarations at
/// one site share a span exactly when they denote the same byte interval.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResolutionSpan {
    /// The text the span indexes.
    pub text: ResolutionTextId,
    /// First byte of the span.
    pub start: u64,
    /// One past the last byte of the span.
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
        Self::records_nothing(self)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::error::Error;

    use panproto_schema::{
        Cardinality, Edge, OffsetUnit, OnUnresolved, PathSyntax, RefSource, RefTextSource,
        Reference, ResolveMode, Schema, UriBase,
    };
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use super::{
        AnyInstance, AssemblyPlanId, AssemblyResidue, AssemblyTrace, DefinitionKey, RedirectedArc,
        RedirectedAttachment, RedirectedFragment, RefOccurrence, ResolutionComplement,
        ResolutionContextKind, ResolutionKind, ResolutionProviderId, ResolutionSpan,
        ResolutionSpec, ResolutionTextId, ResolutionValueSite, UnresolvedReason, UnresolvedRef,
    };
    use crate::identity::{
        AnyComplement, ComplementShape, Digest, Encoding, IdentityError, IdentityField,
        RESOLUTION_CONTEXT, RESOLUTION_EXTERNAL_TEXT_CONTEXT, RESOLUTION_OCCURRENCE_CONTEXT,
        RESOLUTION_SPEC_CONTEXT, RESOLUTION_TOKEN_BOUNDARIES_CONTEXT, decode_complement,
        resolution_context_digest, resolution_digest, resolution_external_text_digest,
        resolution_identity, resolution_spec_digest, resolution_token_boundaries_digest,
        schema_digest,
    };
    use crate::metadata::Node;
    use crate::value::Value;
    use crate::view_ref::{RowId, SynthKey, ViewRef};
    use crate::{EdgeId, Fan, WInstance};

    type TestResult = Result<(), Box<dyn Error>>;

    fn label() -> Edge {
        Edge {
            src: "holder".into(),
            tgt: "target".into(),
            kind: "reference".into(),
            name: Some("to".into()),
        }
    }

    fn reference() -> Reference {
        Reference {
            id: "r".into(),
            holder: "holder".into(),
            source: RefSource::Path {
                field: Vec::new(),
                syntax: PathSyntax::IdRef,
            },
            target_vertex: "target".into(),
            target_key: None,
            edge: Some(label()),
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: Default::default(),
            null_vertex: None,
            scope_via: None,
        }
    }

    fn spec() -> ResolutionSpec {
        ResolutionSpec {
            kind: ResolutionKind::TreeToGraph,
            provider: ResolutionProviderId {
                name: "test".into(),
                version: 1,
            },
            semantic_schema: Schema::default(),
            protocol_references: BTreeMap::new(),
            structural_edges: BTreeSet::new(),
            reference_writers: BTreeMap::new(),
            context_requirements: BTreeSet::new(),
            assembly: None,
        }
    }

    fn local(holder: ViewRef, field: &str) -> ResolutionTextId {
        ResolutionTextId::Local(ResolutionValueSite::Field {
            holder,
            field: field.to_owned(),
        })
    }

    /// A complement with every field nonempty: duplicate occurrences at
    /// one holder, two definitions of one target in two namespaces, a
    /// redirected fragment with an attachment, an inner arc and a fan, two
    /// span texts of each kind, an unresolved occurrence and assembly
    /// residue.
    fn full_complement() -> ResolutionComplement {
        let occurrence = |index: u32| RefOccurrence {
            holder: ViewRef::Node(2),
            reference: "r".into(),
            index,
            literal: Value::Str("x".to_owned()),
            form: Some("X".to_owned()),
            context: Some(Digest([u8::from(index > 0); 32])),
        };
        let mut complement = ResolutionComplement::default();
        complement
            .occurrences
            .insert(ViewRef::Edge(EdgeId(9)), vec![occurrence(0)]);
        complement
            .occurrences
            .insert(ViewRef::Edge(EdgeId(10)), vec![occurrence(1)]);
        for namespace in ["r", "s"] {
            complement.definitions.insert(
                DefinitionKey {
                    reference: namespace.into(),
                    target: 7,
                },
                ViewRef::Node(7),
            );
        }
        let mut node = Node::new(4, "holder");
        node.value = Some(crate::value::FieldPresence::Present(Value::Str("a".into())));
        complement.redirected.insert(
            4,
            RedirectedFragment {
                root: 4,
                attachment: Some(RedirectedAttachment {
                    parent: 1,
                    child: 4,
                    edge: label(),
                    source_slot: 3,
                }),
                nodes: BTreeMap::from([(4, node), (5, Node::new(5, "leaf"))]),
                arcs: vec![RedirectedArc {
                    parent: 4,
                    child: 5,
                    edge: label(),
                    source_slot: 4,
                }],
                fans: vec![Fan::new("rel", 4).with_child("arg", 5)],
            },
        );
        complement
            .order
            .insert(1, vec![ViewRef::Node(2), ViewRef::Node(4)]);
        complement.spanning.insert(2, ViewRef::Edge(EdgeId(0)));
        complement.span_nodes.insert(
            11,
            ResolutionSpan {
                text: local(ViewRef::Row(RowId(3)), "text"),
                start: 1,
                end: 4,
            },
        );
        complement.span_nodes.insert(
            12,
            ResolutionSpan {
                text: ResolutionTextId::ExternalKey(ResolutionValueSite::Own(ViewRef::Node(6))),
                start: 0,
                end: 2,
            },
        );
        complement.unresolved.push(UnresolvedRef {
            holder: ViewRef::Node(2),
            reference: "r".into(),
            index: 2,
            literal: Value::Int(3),
            reason: UnresolvedReason::Ambiguous {
                candidates: vec![ViewRef::Node(1), ViewRef::Node(7)],
            },
            context: None,
        });
        complement.assembly = Some(AssemblyResidue {
            source: Box::new(AnyInstance::Tree(WInstance::new(
                std::collections::HashMap::from([(0, Node::new(0, "root"))]),
                Vec::new(),
                Vec::new(),
                0,
                "root".into(),
            ))),
            trace: vec![AssemblyTrace {
                production: "p".into(),
                source: vec![ViewRef::Node(0)],
                view: vec![ViewRef::Row(RowId(0))],
            }],
            context: Some(Digest([7; 32])),
        });
        complement.identity = Some(resolution_identity("test", &Schema::default(), &spec()));
        complement
    }

    /// Round-trip `value` through JSON text, the JSON object model, named
    /// and positional msgpack and CBOR, comparing each result's JSON form
    /// with the original's.
    fn four_encodings<T: Serialize + DeserializeOwned>(value: &T) -> TestResult {
        let expected = serde_json::to_value(value)?;
        let text: T = serde_json::from_str(&serde_json::to_string(value)?)?;
        assert_eq!(serde_json::to_value(&text)?, expected, "json");
        let model: T = serde_json::from_value(expected.clone())?;
        assert_eq!(serde_json::to_value(&model)?, expected, "object model");
        let named: T = rmp_serde::from_slice(&rmp_serde::to_vec_named(value)?)?;
        assert_eq!(serde_json::to_value(&named)?, expected, "named msgpack");
        let compact: T = rmp_serde::from_slice(&rmp_serde::to_vec(value)?)?;
        assert_eq!(serde_json::to_value(&compact)?, expected, "msgpack");
        let mut cbor = Vec::new();
        ciborium::into_writer(value, &mut cbor)?;
        let decoded: T = ciborium::from_reader(cbor.as_slice())?;
        assert_eq!(serde_json::to_value(&decoded)?, expected, "cbor");
        Ok(())
    }

    #[test]
    fn resolution_kinds_serialize_snake_case() -> TestResult {
        assert_eq!(
            serde_json::to_string(&[
                ResolutionKind::TreeToGraph,
                ResolutionKind::TableToGraph,
                ResolutionKind::TreeToTable,
                ResolutionKind::TableToTree,
            ])?,
            r#"["tree_to_graph","table_to_graph","tree_to_table","table_to_tree"]"#
        );
        assert_eq!(
            serde_json::to_string(&[
                ResolutionContextKind::ExternalText,
                ResolutionContextKind::TokenBoundaries,
            ])?,
            r#"["external_text","token_boundaries"]"#
        );
        Ok(())
    }

    #[test]
    fn resolution_complement_serializes_in_field_order() -> TestResult {
        assert_eq!(
            serde_json::to_string(&ResolutionComplement::default())?,
            r#"{"occurrences":[],"definitions":[],"redirected":{},"order":{},"spanning":{},"span_nodes":{},"unresolved":[],"assembly":null,"identity":null}"#
        );
        let text = serde_json::to_string(&spec())?;
        let positions: Vec<usize> = [
            "\"kind\"",
            "\"provider\"",
            "\"semantic_schema\"",
            "\"protocol_references\"",
            "\"structural_edges\"",
            "\"reference_writers\"",
            "\"context_requirements\"",
            "\"assembly\"",
        ]
        .iter()
        .filter_map(|field| text.find(field))
        .collect();
        assert_eq!(positions.len(), 8);
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        Ok(())
    }

    #[test]
    fn non_string_keyed_maps_round_trip_in_four_encodings() -> TestResult {
        let complement = full_complement();
        assert!(
            serde_json::to_string(
                &complement
                    .occurrences
                    .clone()
                    .into_iter()
                    .collect::<std::collections::HashMap<_, _>>()
            )
            .is_err()
        );
        four_encodings(&complement)?;
        let mut spec = spec();
        spec.reference_writers.insert(label(), "r".into());
        spec.protocol_references.insert("r".into(), reference());
        four_encodings(&spec)?;
        let json = serde_json::to_value(&complement)?;
        assert_eq!(json["occurrences"].as_array().map(Vec::len), Some(2));
        assert_eq!(json["definitions"].as_array().map(Vec::len), Some(2));
        Ok(())
    }

    #[test]
    fn a_duplicate_map_key_is_refused_on_decode() -> TestResult {
        let mut json = serde_json::to_value(full_complement())?;
        let first = json["definitions"][0].clone();
        json["definitions"][1] = first;
        assert!(serde_json::from_value::<ResolutionComplement>(json).is_err());
        Ok(())
    }

    #[test]
    fn resolution_text_ids_round_trip_in_four_encodings() -> TestResult {
        let ids = vec![
            ResolutionTextId::Local(ResolutionValueSite::Own(ViewRef::Node(1))),
            ResolutionTextId::Local(ResolutionValueSite::Field {
                holder: ViewRef::Row(RowId(2)),
                field: "text".to_owned(),
            }),
            ResolutionTextId::ExternalKey(ResolutionValueSite::Own(ViewRef::Node(1))),
            ResolutionTextId::ExternalKey(ResolutionValueSite::Field {
                holder: ViewRef::Node(1),
                field: "doc".to_owned(),
            }),
        ];
        four_encodings(&ids)?;
        let spans: Vec<ResolutionSpan> = ids
            .iter()
            .map(|text| ResolutionSpan {
                text: text.clone(),
                start: 2,
                end: 5,
            })
            .collect();
        four_encodings(&spans)?;
        let distinct: BTreeSet<&ResolutionTextId> = ids.iter().collect();
        assert_eq!(distinct.len(), 4);
        Ok(())
    }

    #[test]
    fn resolution_wire_shape_is_stable() -> TestResult {
        let span = ResolutionSpan {
            text: local(ViewRef::Row(RowId(3)), "text"),
            start: 1,
            end: 4,
        };
        assert_eq!(
            serde_json::to_string(&span)?,
            r#"{"text":{"local":{"field":{"holder":{"row":3},"field":"text"}}},"start":1,"end":4}"#
        );
        let external = ResolutionTextId::ExternalKey(ResolutionValueSite::Own(ViewRef::Node(6)));
        assert_eq!(
            serde_json::to_string(&external)?,
            r#"{"external_key":{"own":{"node":6}}}"#
        );
        let unresolved = UnresolvedReason::Ambiguous {
            candidates: vec![ViewRef::Node(1)],
        };
        assert_eq!(
            serde_json::to_string(&[
                UnresolvedReason::Dangling,
                UnresolvedReason::Unselected {
                    value: Value::Int(7),
                },
                unresolved,
            ])?,
            r#"["dangling",{"unselected":{"value":{"Int":7}}},{"ambiguous":{"candidates":[{"node":1}]}}]"#
        );
        let key = DefinitionKey {
            reference: "r".into(),
            target: 7,
        };
        assert_eq!(
            serde_json::to_string(&key)?,
            r#"{"reference":"r","target":7}"#
        );
        Ok(())
    }

    #[test]
    fn resolution_synth_index_names_occurrence_edges_and_span_nodes() {
        let complement = full_complement();
        let index = complement.synth_index();
        assert_eq!(
            index.edges.get(&EdgeId(10)),
            Some(&SynthKey::ResolutionReference {
                holder: ViewRef::Node(2),
                reference: "r".into(),
                index: 1,
            })
        );
        assert_eq!(
            index.nodes.get(&11),
            Some(&SynthKey::ResolutionSpan {
                text: local(ViewRef::Row(RowId(3)), "text"),
                start: 1,
                end: 4,
            })
        );
        assert_eq!(index.nodes.len(), 2);
        assert_eq!(index.edges.len(), 2);
    }

    /// Every way to change one field of a spec, each changing one input.
    fn spec_variants() -> Vec<ResolutionSpec> {
        let base = spec();
        let mut variants = Vec::new();
        let mut changed = base.clone();
        changed.kind = ResolutionKind::TableToGraph;
        variants.push(changed);
        let mut changed = base.clone();
        changed.provider.name = "other".into();
        variants.push(changed);
        let mut changed = base.clone();
        changed.provider.version = 2;
        variants.push(changed);
        let mut changed = base.clone();
        changed.semantic_schema.protocol = "other".to_owned();
        variants.push(changed);
        let mut changed = base.clone();
        changed.protocol_references.insert("r".into(), reference());
        variants.push(changed);
        let mut changed = base.clone();
        let mut altered = reference();
        altered.nullable = true;
        changed.protocol_references.insert("r".into(), altered);
        variants.push(changed);
        let mut changed = base.clone();
        changed.structural_edges.insert(label());
        variants.push(changed);
        let mut changed = base.clone();
        changed.reference_writers.insert(label(), "r".into());
        variants.push(changed);
        let mut changed = base.clone();
        changed.reference_writers.insert(label(), "s".into());
        variants.push(changed);
        let mut changed = base.clone();
        changed
            .context_requirements
            .insert(ResolutionContextKind::ExternalText);
        variants.push(changed);
        let mut changed = base.clone();
        changed
            .context_requirements
            .insert(ResolutionContextKind::TokenBoundaries);
        variants.push(changed);
        let mut changed = base.clone();
        changed.assembly = Some(AssemblyPlanId {
            grammar: "g".into(),
            version: 1,
        });
        variants.push(changed);
        let mut changed = base;
        changed.assembly = Some(AssemblyPlanId {
            grammar: "g".into(),
            version: 2,
        });
        variants.push(changed);
        variants
    }

    #[test]
    fn resolution_spec_digest_depends_on_each_input() {
        let mut digests = BTreeSet::from([resolution_spec_digest(&spec()).0]);
        let variants = spec_variants();
        for changed in &variants {
            assert!(
                digests.insert(resolution_spec_digest(changed).0),
                "a spec variant shares a digest: {changed:?}"
            );
        }
        assert_eq!(digests.len(), variants.len() + 1);
        let mut standoff = reference();
        standoff.source = RefSource::Standoff {
            text: RefTextSource::Local(vec![label()]),
            start: Vec::new(),
            end: Vec::new(),
            unit: OffsetUnit::Byte,
            end_inclusive: false,
            end_form: Default::default(),
            origin: 0,
            fragments: None,
        };
        let mut with = spec();
        with.protocol_references
            .insert("r".into(), standoff.clone());
        let mut units = BTreeSet::new();
        for unit in [
            OffsetUnit::Byte,
            OffsetUnit::Utf8Char,
            OffsetUnit::Utf16,
            OffsetUnit::Token,
        ] {
            for inclusive in [false, true] {
                let mut variant = standoff.clone();
                if let RefSource::Standoff {
                    unit: slot,
                    end_inclusive,
                    ..
                } = &mut variant.source
                {
                    *slot = unit.clone();
                    *end_inclusive = inclusive;
                }
                with.protocol_references.insert("r".into(), variant);
                units.insert(resolution_spec_digest(&with).0);
            }
        }
        assert_eq!(units.len(), 8);
    }

    #[test]
    fn the_token_boundary_digest_depends_on_the_exact_text_identity() {
        let holders = [
            ViewRef::Node(1),
            ViewRef::Node(2),
            ViewRef::Edge(EdgeId(1)),
            ViewRef::Arc {
                parent: 1,
                child: 2,
            },
            ViewRef::Arc {
                parent: 2,
                child: 1,
            },
            ViewRef::Row(RowId(1)),
            ViewRef::Pair {
                edge: label(),
                src: RowId(1),
                tgt: RowId(2),
            },
            ViewRef::Pair {
                edge: label(),
                src: RowId(2),
                tgt: RowId(1),
            },
        ];
        let mut ids = Vec::new();
        for holder in &holders {
            let sites = [
                ResolutionValueSite::Own(holder.clone()),
                ResolutionValueSite::Field {
                    holder: holder.clone(),
                    field: "a".to_owned(),
                },
                ResolutionValueSite::Field {
                    holder: holder.clone(),
                    field: "b".to_owned(),
                },
            ];
            for site in sites {
                ids.push(ResolutionTextId::Local(site.clone()));
                ids.push(ResolutionTextId::ExternalKey(site));
            }
        }
        let digests: BTreeSet<_> = ids
            .iter()
            .map(|id| resolution_token_boundaries_digest(id, &[0, 3]).0)
            .collect();
        assert_eq!(
            digests.len(),
            ids.len(),
            "every text identity has its digest"
        );
        let id = &ids[0];
        assert_ne!(
            resolution_token_boundaries_digest(id, &[0, 3]),
            resolution_token_boundaries_digest(id, &[0, 2, 3])
        );
    }

    #[test]
    fn the_spec_digest_depends_on_every_path_and_path_syntax() {
        let other = Edge {
            name: Some("other".into()),
            ..label()
        };
        let paths = [
            Vec::new(),
            vec![label()],
            vec![other.clone()],
            vec![label(), other],
        ];
        let syntaxes = [
            PathSyntax::JsonPointer,
            PathSyntax::XPath,
            PathSyntax::Uri {
                base: UriBase::Absolute,
            },
            PathSyntax::Uri {
                base: UriBase::Field(Vec::new()),
            },
            PathSyntax::Uri {
                base: UriBase::Field(vec![label()]),
            },
            PathSyntax::IdRef,
        ];
        let mut sources = Vec::new();
        for field in &paths {
            for syntax in &syntaxes {
                sources.push(RefSource::Path {
                    field: field.clone(),
                    syntax: syntax.clone(),
                });
            }
            sources.push(RefSource::KeyValue {
                parts: vec![field.clone()],
            });
            sources.push(RefSource::Variable {
                field: field.clone(),
                binder_key: "k".into(),
                binding: Default::default(),
            });
            for text in [
                RefTextSource::Local(field.clone()),
                RefTextSource::External(field.clone()),
            ] {
                sources.push(RefSource::Standoff {
                    text,
                    start: Vec::new(),
                    end: Vec::new(),
                    unit: OffsetUnit::Byte,
                    end_inclusive: false,
                    end_form: Default::default(),
                    origin: 0,
                    fragments: None,
                });
            }
            sources.push(RefSource::Standoff {
                text: RefTextSource::Local(Vec::new()),
                start: field.clone(),
                end: Vec::new(),
                unit: OffsetUnit::Byte,
                end_inclusive: false,
                end_form: Default::default(),
                origin: 0,
                fragments: None,
            });
            sources.push(RefSource::Standoff {
                text: RefTextSource::Local(Vec::new()),
                start: Vec::new(),
                end: field.clone(),
                unit: OffsetUnit::Byte,
                end_inclusive: false,
                end_form: Default::default(),
                origin: 0,
                fragments: None,
            });
        }
        assert_eq!(sources.len(), 48);
        let digests: BTreeSet<_> = sources
            .into_iter()
            .map(|source| {
                let mut variant = reference();
                variant.source = source;
                let mut with = spec();
                with.protocol_references.insert("r".into(), variant);
                resolution_spec_digest(&with).0
            })
            .collect();
        // Twelve sources per field path; at the empty path the local
        // standoff, its start variant and its end variant are one source,
        // so 48 sources are 46 distinct references.
        assert_eq!(digests.len(), 46);
    }

    #[test]
    fn an_any_instance_borrows_only_its_own_shape() {
        let tree = AnyInstance::Tree(WInstance::new(
            BTreeMap::from([(0, Node::new(0, "a"))])
                .into_iter()
                .collect(),
            Vec::new(),
            Vec::new(),
            0,
            "a".into(),
        ));
        let table = AnyInstance::Table(crate::FInstance::new());
        let graph = AnyInstance::Graph(crate::GInstance::new());
        assert!(tree.as_tree().is_some() && tree.as_table().is_none() && tree.as_graph().is_none());
        assert!(
            table.as_table().is_some() && table.as_tree().is_none() && table.as_graph().is_none()
        );
        assert!(
            graph.as_graph().is_some() && graph.as_tree().is_none() && graph.as_table().is_none()
        );
    }

    #[test]
    fn a_text_identity_says_whether_its_text_is_local() {
        let site = ResolutionValueSite::Own(ViewRef::Node(1));
        assert!(ResolutionTextId::Local(site.clone()).is_local());
        assert!(!ResolutionTextId::ExternalKey(site).is_local());
    }

    #[test]
    fn a_resolution_complement_carries_its_identity_and_no_fingerprint() {
        use crate::identity::IdentityCarrier;

        let mut complement = ResolutionComplement::default();
        assert!(IdentityCarrier::records_nothing(&complement));
        assert_eq!(IdentityCarrier::identity(&complement), None);
        let envelope = resolution_identity("p", &Schema::default(), &spec());
        IdentityCarrier::set_identity(&mut complement, Some(envelope.clone()));
        assert_eq!(IdentityCarrier::identity(&complement), Some(&envelope));
        IdentityCarrier::set_legacy_fingerprint(&mut complement, 7);
        assert_eq!(IdentityCarrier::legacy_fingerprint(&complement), 0);
        assert!(
            IdentityCarrier::records_nothing(&complement),
            "the identity is not a record"
        );
        complement.order.insert(0, Vec::new());
        assert!(!IdentityCarrier::records_nothing(&complement));
    }

    #[test]
    fn resolution_digest_depends_on_each_input() {
        let a = schema_digest(&Schema::default());
        let other = Schema {
            protocol: "other".to_owned(),
            ..Schema::default()
        };
        let b = schema_digest(&other);
        let spec_digest = resolution_spec_digest(&spec());
        let base = resolution_digest("p", ResolutionKind::TreeToGraph, &a, &a, &spec_digest);
        let variants = [
            resolution_digest("q", ResolutionKind::TreeToGraph, &a, &a, &spec_digest),
            resolution_digest("p", ResolutionKind::TableToGraph, &a, &a, &spec_digest),
            resolution_digest("p", ResolutionKind::TreeToTable, &a, &a, &spec_digest),
            resolution_digest("p", ResolutionKind::TableToTree, &a, &a, &spec_digest),
            resolution_digest("p", ResolutionKind::TreeToGraph, &b, &a, &spec_digest),
            resolution_digest("p", ResolutionKind::TreeToGraph, &a, &b, &spec_digest),
            resolution_digest("p", ResolutionKind::TreeToGraph, &a, &a, &Digest([1; 32])),
        ];
        let mut seen = BTreeSet::from([base.0]);
        for digest in variants {
            assert!(seen.insert(digest.0));
        }
        let envelope = resolution_identity("p", &Schema::default(), &spec());
        assert_eq!(envelope.lens, base);
        assert_eq!(envelope.source_schema, a);
        assert_eq!(envelope.target_schema, a);
        assert_eq!(
            envelope.complement_format,
            ComplementShape::Resolution.current_format()
        );
    }

    #[test]
    fn the_resolution_contexts_are_pinned_and_distinct() {
        let contexts = [
            (
                RESOLUTION_CONTEXT,
                "panproto 2026-09 resolution identity v1",
            ),
            (
                RESOLUTION_SPEC_CONTEXT,
                "panproto 2026-09 resolution spec v1",
            ),
            (
                RESOLUTION_EXTERNAL_TEXT_CONTEXT,
                "panproto 2026-10 resolution external text v1",
            ),
            (
                RESOLUTION_TOKEN_BOUNDARIES_CONTEXT,
                "panproto 2026-10 resolution token boundaries v1",
            ),
            (
                RESOLUTION_OCCURRENCE_CONTEXT,
                "panproto 2026-10 resolution occurrence context v1",
            ),
        ];
        for (context, expected) in contexts {
            assert_eq!(context, expected);
        }
        let distinct: BTreeSet<&str> = contexts.iter().map(|(context, _)| *context).collect();
        assert_eq!(distinct.len(), contexts.len());
    }

    #[test]
    fn context_digests_bind_their_inputs() {
        let key = Value::Str("doc".into());
        let external = resolution_external_text_digest(&key, b"abc");
        assert_ne!(external, resolution_external_text_digest(&key, b"abd"));
        assert_ne!(
            external,
            resolution_external_text_digest(&Value::Str("other".into()), b"abc")
        );
        let a = local(ViewRef::Node(1), "a");
        let b = local(ViewRef::Node(1), "b");
        let tokens = resolution_token_boundaries_digest(&a, &[0, 3]);
        assert_ne!(tokens, resolution_token_boundaries_digest(&b, &[0, 3]));
        assert_ne!(tokens, resolution_token_boundaries_digest(&a, &[0, 2, 3]));
        assert_eq!(resolution_context_digest(None, None), None);
        let only_external = resolution_context_digest(Some(&external), None);
        let only_tokens = resolution_context_digest(None, Some(&external));
        assert!(only_external.is_some());
        assert_ne!(only_external, only_tokens);
        assert_ne!(
            only_external,
            resolution_context_digest(Some(&external), Some(&tokens))
        );
    }

    #[test]
    fn decoding_reports_a_resolution_complement_where_a_tree_one_was_expected() -> TestResult {
        let complement = full_complement();
        let bytes = serde_json::to_vec(&complement)?;
        let decoded = decode_complement(&bytes, Encoding::Json, ComplementShape::Resolution)?;
        let AnyComplement::Resolution(decoded) = decoded else {
            return Err("decoded as another shape".into());
        };
        assert_eq!(
            serde_json::to_value(&*decoded)?,
            serde_json::to_value(&complement)?
        );
        match decode_complement(&bytes, Encoding::Json, ComplementShape::WType) {
            Err(IdentityError::Mismatch { field, .. }) => {
                assert_eq!(field, IdentityField::Format);
            }
            other => return Err(format!("expected a format mismatch, got {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn records_nothing_reads_every_residue_field() {
        assert!(ResolutionComplement::default().records_nothing());
        let full = full_complement();
        let only: [fn(&ResolutionComplement) -> ResolutionComplement; 8] = [
            |c| ResolutionComplement {
                occurrences: c.occurrences.clone(),
                ..ResolutionComplement::default()
            },
            |c| ResolutionComplement {
                definitions: c.definitions.clone(),
                ..ResolutionComplement::default()
            },
            |c| ResolutionComplement {
                redirected: c.redirected.clone(),
                ..ResolutionComplement::default()
            },
            |c| ResolutionComplement {
                order: c.order.clone(),
                ..ResolutionComplement::default()
            },
            |c| ResolutionComplement {
                spanning: c.spanning.clone(),
                ..ResolutionComplement::default()
            },
            |c| ResolutionComplement {
                span_nodes: c.span_nodes.clone(),
                ..ResolutionComplement::default()
            },
            |c| ResolutionComplement {
                unresolved: c.unresolved.clone(),
                ..ResolutionComplement::default()
            },
            |c| ResolutionComplement {
                assembly: c.assembly.clone(),
                ..ResolutionComplement::default()
            },
        ];
        for build in only {
            assert!(!build(&full).records_nothing());
        }
        let identity_only = ResolutionComplement {
            identity: full.identity,
            ..ResolutionComplement::default()
        };
        assert!(identity_only.records_nothing());
    }
}
