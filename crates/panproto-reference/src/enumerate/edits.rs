//! Finite carriers for the edit-algebra page.
//!
//! [`graphs`], [`graph_delta_cases`] and [`graph_delta_sequences`] are one
//! completely enumerated labelled **graph-only** witness carrier. They are not
//! advertised as the C8 carrier: that contract fixes several cardinality
//! bounds but does not yet fix the finite alphabets needed to make its
//! quantifiers executable. [`c8_scope_status`] records that distinction as
//! data.

use crate::edit::{AttrKey, GElem, GraphDelta};
use crate::model::{Graph, NodeShape, SEdge};
use crate::value::Value;

/// A constructor family in the atomic graph-delta alphabet.
///
/// The attribute families remain separate, in particular node shape and edge
/// position. The case retains the complete delta, so opposite directions such
/// as `Plain -> HyperEdge` and `HyperEdge -> Plain` are distinct labelled
/// cases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GraphDeltaFamily {
    /// The identity delta.
    Identity,
    /// Add one node.
    NodeAdd,
    /// Delete one node.
    NodeDelete,
    /// Add one edge.
    EdgeAdd,
    /// Delete one edge.
    EdgeDelete,
    /// Change one node anchor.
    AnchorSet,
    /// Change one node value presence.
    ValueSet,
    /// Change one named field.
    FieldSet,
    /// Change one node shape.
    ShapeSet,
    /// Change one edge label.
    EdgeLabelSet,
    /// Change one edge position.
    EdgePositionSet,
}

/// One tagged atomic graph delta.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphDeltaCase {
    /// The constructor family.
    pub family: GraphDeltaFamily,
    /// The complete labelled delta.
    pub delta: GraphDelta,
}

/// Independent per-family count certificate for [`graph_delta_cases`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphDeltaFamilyCounts {
    /// Identity cases.
    pub identity: u64,
    /// Node additions.
    pub node_add: u64,
    /// Node deletions.
    pub node_delete: u64,
    /// Edge additions.
    pub edge_add: u64,
    /// Edge deletions.
    pub edge_delete: u64,
    /// Anchor changes.
    pub anchor_set: u64,
    /// Value-presence changes.
    pub value_set: u64,
    /// Field changes.
    pub field_set: u64,
    /// Shape changes.
    pub shape_set: u64,
    /// Edge-label changes.
    pub edge_label_set: u64,
    /// Edge-position changes.
    pub edge_position_set: u64,
}

impl GraphDeltaFamilyCounts {
    /// Sum of every constructor family.
    #[must_use]
    pub const fn total(self) -> u64 {
        self.identity
            + self.node_add
            + self.node_delete
            + self.edge_add
            + self.edge_delete
            + self.anchor_set
            + self.value_set
            + self.field_set
            + self.shape_set
            + self.edge_label_set
            + self.edge_position_set
    }
}

/// Cardinality bounds that C8 does settle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct C8SettledBounds {
    /// Most vertices in a schema.
    pub schema_vertices: u8,
    /// Most edges in a schema.
    pub schema_edges: u8,
    /// Most nodes in a graph.
    pub graph_nodes: u8,
    /// Most rows in a table.
    pub table_rows: u8,
    /// Most nodes in a tree.
    pub tree_nodes: u8,
    /// Most edits in a sequence.
    pub sequence_len: u8,
}

/// A finite parameter still missing from C8.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum C8MissingParameter {
    /// Vertex names, edge kinds and edge names from which schemas are built.
    SchemaAlphabet,
    /// Values used by states and value-set edits.
    ValueAlphabet,
    /// Initial graph/table payload choices beyond the scalar-value alphabet:
    /// presence states, discriminator and metadata values, field and
    /// annotation names and values, node and graph-edge position alphabets,
    /// and bounds on every variable-size payload map.
    InitialStatePayloadAlphabetAndBounds,
    /// A bound on graph instance edges (C8 bounds graph nodes only).
    GraphEdgeBound,
    /// The finite labelled existing-id carrier and concrete representatives
    /// of the required fresh-id and id-floor classes.
    StateIdCarrier,
    /// Payload choices for node, edge and row additions.
    AdditionPayloadAlphabet,
    /// Table names, columns, foreign-key pairs and the exact row-edit
    /// constructors generated at each state.
    TableStructureAndEditAlphabet,
    /// Non-edge schema declarations and the finite hyper-edge signature
    /// alphabet from which schemas are built.
    SchemaDeclarationsAndHyperEdgeSignatures,
    /// Ordinary, ordered, XML and fan node-shape choices.
    NodeShapeAlphabet,
    /// Tree arc, fan, anchor-change and parent-change choices.
    TreeEditAlphabet,
    /// Initial tree states, including ordered children, complete fan records,
    /// and finite bounds on arc and fan sequences before any edit is applied.
    InitialTreeStateAndFanAlphabet,
    /// Which generated edits are applicable at each intermediate state,
    /// which are retained as typed refusals, and whether later alphabets are
    /// regenerated or fixed from the initial state.
    SequenceApplicabilityAndRegenerationPolicy,
    /// The finite canonical graph/table/tree golden corpus used by C8 is not
    /// fixed. E.3a's encoders are implemented, but without the preceding
    /// payload and edit alphabets C8 cannot enumerate their complete image.
    CanonicalDeltaEncoding,
}

/// The executable status of the C8 scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C8ScopeStatus {
    /// Some bounds are settled, but the listed parameters still prevent an
    /// exact finite enumeration.
    Incomplete {
        /// Bounds fixed by the contract.
        settled: C8SettledBounds,
        /// Parameters that must be fixed before the carrier is finite and
        /// exact.
        missing: &'static [C8MissingParameter],
    },
}

const C8_MISSING: &[C8MissingParameter] = &[
    C8MissingParameter::SchemaAlphabet,
    C8MissingParameter::ValueAlphabet,
    C8MissingParameter::InitialStatePayloadAlphabetAndBounds,
    C8MissingParameter::GraphEdgeBound,
    C8MissingParameter::StateIdCarrier,
    C8MissingParameter::AdditionPayloadAlphabet,
    C8MissingParameter::TableStructureAndEditAlphabet,
    C8MissingParameter::SchemaDeclarationsAndHyperEdgeSignatures,
    C8MissingParameter::NodeShapeAlphabet,
    C8MissingParameter::TreeEditAlphabet,
    C8MissingParameter::InitialTreeStateAndFanAlphabet,
    C8MissingParameter::SequenceApplicabilityAndRegenerationPolicy,
    C8MissingParameter::CanonicalDeltaEncoding,
];

/// C8's settled bounds and the parameters that still prevent a complete
/// finite enumeration.
#[must_use]
pub const fn c8_scope_status() -> C8ScopeStatus {
    C8ScopeStatus::Incomplete {
        settled: C8SettledBounds {
            schema_vertices: 2,
            schema_edges: 2,
            graph_nodes: 3,
            table_rows: 3,
            tree_nodes: 3,
            sequence_len: 3,
        },
        missing: C8_MISSING,
    }
}

/// The labelled graph witness carrier: at most three nodes and one edge.
///
/// Three is C8's settled node bound. One is the carrier's explicit legacy
/// edge bound; [`c8_scope_status`] records that C8 itself does not yet supply
/// an edge bound.
#[must_use]
pub fn graphs() -> Vec<Graph> {
    let labels = [SEdge::new("a", "a", "e", None)];
    super::graphs(&super::GraphScope {
        anchors: &["a"],
        values: &[Value::Int(0)],
        shapes: &[NodeShape::Plain],
        labels: &labels,
        positions: &[None],
        max_nodes: 3,
        max_edges: 1,
    })
}

/// Independent count of [`graphs`].
///
/// With `n` labelled nodes, each node independently has no value or value
/// zero, and the graph has no edge or one edge over the `n^2` ordered endpoint
/// pairs. Thus the count is
/// `sum_(n=0)^3 2^n (1 + n^2) = 1 + 4 + 20 + 80 = 105`.
#[must_use]
pub const fn graphs_count_certificate() -> u64 {
    1 + 4 + 20 + 80
}

/// Every atomic graph edit over one labelled node, edge and field.
#[must_use]
pub fn graph_deltas() -> Vec<GraphDelta> {
    let labels = [
        SEdge::new("a", "a", "e", None),
        SEdge::new("a", "a", "e", Some("x")),
    ];
    super::graph_deltas(&super::DeltaScope {
        nodes: &[0],
        anchors: &["a", "b"],
        values: &[Value::Int(0)],
        fields: &["f"],
        edges: &[0],
        labels: &labels,
        positions: &[None, Some(0)],
    })
}

fn graph_delta_family(delta: &GraphDelta) -> GraphDeltaFamily {
    if delta.del.is_empty() && delta.add.is_empty() && delta.attrs.is_empty() {
        return GraphDeltaFamily::Identity;
    }
    if let Some((element, _)) = delta.add.first_key_value() {
        return match element {
            GElem::Node(_) => GraphDeltaFamily::NodeAdd,
            GElem::Edge(_) => GraphDeltaFamily::EdgeAdd,
        };
    }
    if let Some((element, _)) = delta.del.first_key_value() {
        return match element {
            GElem::Node(_) => GraphDeltaFamily::NodeDelete,
            GElem::Edge(_) => GraphDeltaFamily::EdgeDelete,
        };
    }
    match delta.attrs.first_key_value().map(|(key, _)| key) {
        Some(AttrKey::Anchor(_)) => GraphDeltaFamily::AnchorSet,
        Some(AttrKey::Value(_)) => GraphDeltaFamily::ValueSet,
        Some(AttrKey::Field(..)) => GraphDeltaFamily::FieldSet,
        Some(AttrKey::Meta(_)) => GraphDeltaFamily::ShapeSet,
        Some(AttrKey::EdgeLabel(_)) => GraphDeltaFamily::EdgeLabelSet,
        Some(AttrKey::EdgePosition(_)) => GraphDeltaFamily::EdgePositionSet,
        None => GraphDeltaFamily::Identity,
    }
}

/// Every atomic delta in [`graph_deltas`], tagged by constructor family.
#[must_use]
pub fn graph_delta_cases() -> Vec<GraphDeltaCase> {
    graph_deltas()
        .into_iter()
        .map(|delta| GraphDeltaCase {
            family: graph_delta_family(&delta),
            delta,
        })
        .collect()
}

/// Independent per-family count of [`graph_delta_cases`].
///
/// With the fixed one-id alphabets, additions and deletions each have four node
/// payloads and two edge payloads. Anchor, value, field, shape, edge-label and
/// edge-position changes each have two ordered directions. Thus every tagged
/// atomic constructor family is nonempty.
#[must_use]
pub const fn graph_delta_count_certificate() -> GraphDeltaFamilyCounts {
    GraphDeltaFamilyCounts {
        identity: 1,
        node_add: 4,
        node_delete: 4,
        edge_add: 2,
        edge_delete: 2,
        anchor_set: 2,
        value_set: 2,
        field_set: 2,
        shape_set: 2,
        edge_label_set: 2,
        edge_position_set: 2,
    }
}

/// Every word of at most three deltas from [`graph_delta_cases`].
///
/// This discharges C8's settled sequence-length bound on the explicit witness
/// alphabet only. It does not fill any parameter listed by
/// [`c8_scope_status`].
#[must_use]
pub fn graph_delta_sequences() -> Vec<Vec<GraphDeltaCase>> {
    super::sequences(&graph_delta_cases(), 3)
}

/// Independent count of [`graph_delta_sequences`]:
/// `1 + 25 + 25^2 + 25^3 = 16,276`.
#[must_use]
pub const fn graph_delta_sequences_count_certificate() -> u64 {
    1 + 25 + 25 * 25 + 25 * 25 * 25
}
