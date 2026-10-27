//! Finite carriers for the edit-algebra page.
//!
//! [`graphs`], [`graph_delta_cases`] and [`graph_delta_sequences`] are one
//! completely enumerated labelled **graph-only** witness carrier. They are not
//! advertised as the C8 carrier: that contract fixes several cardinality
//! bounds but does not yet fix the finite alphabets needed to make its
//! quantifiers executable. [`c8_scope_status`] records that distinction as
//! data.

use std::collections::{BTreeMap, BTreeSet};

use crate::edit::{AttrKey, GElem, GraphDelta, TElem, TKey, TPayload, TVal, TableDelta};
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

/// Every labelled table in the release carrier.
///
/// There are zero to three rows, with ids `0..n`, all in table `a`. Each
/// row independently has no `f` cell or `f = 0`. There is either no
/// foreign-key pair or exactly one pair over the `n^2` ordered endpoints,
/// under the single label `a -e-> a`.
#[must_use]
pub fn tables() -> Vec<crate::model::Table> {
    let edge = SEdge::new("a", "a", "e", None);
    let mut out = Vec::new();
    for n in 0..=3u32 {
        for mask in 0..(1u32 << n) {
            let rows: BTreeMap<_, _> = (0..n)
                .map(|id| {
                    let cells = if (mask & (1 << id)) != 0 {
                        BTreeMap::from([("f".to_owned(), Value::Int(0))])
                    } else {
                        BTreeMap::default()
                    };
                    (id, ("a".to_owned(), cells))
                })
                .collect();
            out.push(crate::model::Table {
                rows: rows.clone(),
                pairs: BTreeSet::default(),
            });
            for src in 0..n {
                for tgt in 0..n {
                    out.push(crate::model::Table {
                        rows: rows.clone(),
                        pairs: std::iter::once((edge.clone(), src, tgt)).collect(),
                    });
                }
            }
        }
    }
    out
}

/// Independent count of [`tables`]:
/// `sum_(n=0)^3 2^n (1 + n^2) = 105`.
#[must_use]
pub const fn tables_count_certificate() -> u64 {
    1 + 4 + 20 + 80
}

/// Every atomic table delta over one labelled row, pair and cell.
#[must_use]
pub fn table_deltas() -> Vec<TableDelta> {
    let mut out = vec![TableDelta::identity()];
    for cells in [
        BTreeMap::new(),
        BTreeMap::from([("f".to_owned(), Value::Int(0))]),
    ] {
        let payload = TPayload::Row("a".to_owned(), cells);
        let mut delete = TableDelta::identity();
        delete.del.insert(TElem::Row(0), payload.clone());
        out.push(delete);
        let mut add = TableDelta::identity();
        add.add.insert(TElem::Row(0), payload);
        out.push(add);
    }
    let pair = TElem::Pair(SEdge::new("a", "a", "e", None), 0, 0);
    let mut delete_pair = TableDelta::identity();
    delete_pair.del.insert(pair.clone(), TPayload::Pair);
    out.push(delete_pair);
    let mut add_pair = TableDelta::identity();
    add_pair.add.insert(pair, TPayload::Pair);
    out.push(add_pair);
    for (old, new) in [(None, Some(Value::Int(0))), (Some(Value::Int(0)), None)] {
        let mut cell = TableDelta::identity();
        cell.attrs.insert(
            TKey::Cell(0, "f".to_owned()),
            (TVal::Cell(old), TVal::Cell(new)),
        );
        out.push(cell);
    }
    for (old, new) in [("a", "b"), ("b", "a")] {
        let mut table = TableDelta::identity();
        table.attrs.insert(
            TKey::Table(0),
            (TVal::Table(old.to_owned()), TVal::Table(new.to_owned())),
        );
        out.push(table);
    }
    out
}

/// Independent count of [`table_deltas`]: identity, two row payloads in
/// both directions, one pair in both directions, one cell in both
/// directions and one table name in both directions.
#[must_use]
pub const fn table_deltas_count_certificate() -> u64 {
    1 + 2 * 2 + 2 + 2 + 2
}

/// Every word of at most three atomic table deltas.
#[must_use]
pub fn table_delta_sequences() -> Vec<Vec<TableDelta>> {
    super::sequences(&table_deltas(), 3)
}

/// Independent count of [`table_delta_sequences`]:
/// `1 + 11 + 11^2 + 11^3 = 1,464`.
#[must_use]
pub const fn table_delta_sequences_count_certificate() -> u64 {
    1 + 11 + 11 * 11 + 11 * 11 * 11
}

/// The direction in which a named edit-translation carrier case runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditTranslationDirection {
    /// Translate an edit on the source through `get`.
    Get,
    /// Translate an edit on the view through `put`.
    Put,
}

/// One named edit in the finite projection-lens translation carrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditTranslationCase {
    /// Stable diagnostic name for the case.
    pub name: &'static str,
    /// Which whole-state square the case exercises.
    pub direction: EditTranslationDirection,
    /// The tree edit lowered to the graph delta passed to the oracle.
    pub edit: crate::edit::TreeEdit,
}

/// The five edits in the named projection-lens translation carrier.
///
/// The source is a root with one retained leaf and one hidden leaf. The get
/// cases contain identity and deletion of each kind of leaf. The put cases
/// contain identity and deletion of the retained leaf. Together they
/// distinguish an observable edit from an edit absorbed into the complement
/// in the forward direction and check residue restoration in the backward
/// direction.
#[must_use]
pub fn edit_translation_cases() -> Vec<EditTranslationCase> {
    use crate::edit::TreeEdit;

    vec![
        EditTranslationCase {
            name: "get-identity",
            direction: EditTranslationDirection::Get,
            edit: TreeEdit::Identity,
        },
        EditTranslationCase {
            name: "get-delete-visible",
            direction: EditTranslationDirection::Get,
            edit: TreeEdit::DeleteNode { id: 1 },
        },
        EditTranslationCase {
            name: "get-delete-hidden",
            direction: EditTranslationDirection::Get,
            edit: TreeEdit::DeleteNode { id: 2 },
        },
        EditTranslationCase {
            name: "put-identity",
            direction: EditTranslationDirection::Put,
            edit: TreeEdit::Identity,
        },
        EditTranslationCase {
            name: "put-delete-visible",
            direction: EditTranslationDirection::Put,
            edit: TreeEdit::DeleteNode { id: 1 },
        },
    ]
}

/// Independent count of [`edit_translation_cases`]: three get cases and two
/// put cases.
#[must_use]
pub const fn edit_translation_cases_count_certificate() -> u64 {
    3 + 2
}
