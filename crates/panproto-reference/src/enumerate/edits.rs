//! Finite carriers for the edit-algebra page.
//!
//! [`graphs`], [`graph_delta_cases`] and [`graph_delta_sequences`] are one
//! completely enumerated labelled graph witness carrier over fixed
//! alphabets. The coverage scope of the edit algebra is enumerated
//! separately: [`c8_scope_status`] records its parameters as data, and
//! [`c8_schemas`], [`c8_graphs`], [`c8_tables`] and [`c8_trees`] with their
//! state-regenerated alphabets and word walkers enumerate it exactly.

use std::collections::{BTreeMap, BTreeSet};

use crate::edit::{
    AttrKey, AttrVal, GElem, GPayload, GraphDelta, TElem, TKey, TPayload, TVal, TableDelta,
};
use crate::model::{GEdge, Graph, Node, NodeShape, SEdge};
use crate::value::Presence;
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

/// The bounds and alphabets of the edit algebra's coverage scope.
///
/// Schemas take their vertices from `vertex_names`, at most
/// `schema_vertices` of them, and at most `schema_edges` edges of the one
/// kind `edge_kind`, without names, between those vertices. Node values,
/// table cells and tree fields range over `values` (absent, `0`, `1`); the
/// table cell and tree field is named `column`. Initial graphs, tables and
/// trees respect the node, row, edge and pair bounds; words have length at
/// most `sequence_len`. Additions use two ids: the fresh id, one above the
/// largest id present, and the floor, one above the largest id present at
/// any earlier point of the word. Every other edit names an id of the state
/// it is generated from. The alphabet is regenerated at each state a word
/// reaches, and a letter that refuses ends its word with that refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct C8Scope {
    /// The vertex names schemas are built from.
    pub vertex_names: [&'static str; 2],
    /// The one schema edge kind.
    pub edge_kind: &'static str,
    /// The table column and tree field that carries a value.
    pub column: &'static str,
    /// The value alphabet: absent, then the integers listed.
    pub values: [Option<i64>; 3],
    /// Most vertices in a schema.
    pub schema_vertices: u8,
    /// Most edges in a schema.
    pub schema_edges: u8,
    /// Most nodes in an initial graph.
    pub graph_nodes: u8,
    /// Most edges in an initial graph.
    pub graph_edges: u8,
    /// Most rows in an initial table.
    pub table_rows: u8,
    /// Most foreign-key pairs in an initial table.
    pub table_pairs: u8,
    /// Most nodes in an initial tree.
    pub tree_nodes: u8,
    /// Most edits in a word.
    pub sequence_len: u8,
}

/// The executable status of the coverage scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C8ScopeStatus {
    /// Every parameter is fixed and the carrier is finite and exact.
    Complete(C8Scope),
}

const C8: C8Scope = C8Scope {
    vertex_names: ["a", "b"],
    edge_kind: "e",
    column: "f",
    values: [None, Some(0), Some(1)],
    schema_vertices: 2,
    schema_edges: 2,
    graph_nodes: 3,
    graph_edges: 2,
    table_rows: 3,
    table_pairs: 2,
    tree_nodes: 3,
    sequence_len: 3,
};

/// The coverage scope's parameters.
#[must_use]
pub const fn c8_scope_status() -> C8ScopeStatus {
    C8ScopeStatus::Complete(C8)
}

/// A schema of the coverage scope: its vertices and its edges, each edge of
/// the one kind and without a name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct C8Schema {
    /// Vertices, ascending.
    pub vertices: Vec<String>,
    /// Edges, ascending.
    pub edges: Vec<SEdge>,
}

impl C8Schema {
    /// The edge from `src` to `tgt`, when the schema has it.
    #[must_use]
    pub fn edge(&self, src: &str, tgt: &str) -> Option<&SEdge> {
        self.edges
            .iter()
            .find(|edge| edge.src == src && edge.tgt == tgt)
    }
}

/// Every schema of the coverage scope: each set of at most two vertices of
/// `{a, b}`, with each set of at most two edges between them.
#[must_use]
pub fn c8_schemas() -> Vec<C8Schema> {
    let names: Vec<String> = C8.vertex_names.iter().map(|name| (*name).to_owned()).collect();
    let mut out = Vec::new();
    for vertices in super::subsets(&names, usize::from(C8.schema_vertices)) {
        let candidates: Vec<SEdge> = vertices
            .iter()
            .flat_map(|src| {
                vertices
                    .iter()
                    .map(move |tgt| SEdge::new(src, tgt, C8.edge_kind, None))
            })
            .collect();
        for mut edges in super::subsets(&candidates, usize::from(C8.schema_edges)) {
            edges.sort();
            out.push(C8Schema {
                vertices: vertices.clone(),
                edges,
            });
        }
    }
    out
}

/// The value alphabet as values: absent, `0` and `1`.
#[must_use]
pub fn c8_values() -> Vec<Option<Value>> {
    C8.values.iter().map(|value| value.map(Value::Int)).collect()
}

fn c8_fresh(ids: impl Iterator<Item = u32>) -> u32 {
    ids.max().map_or(0, |id| id.saturating_add(1))
}

/// The allocation floors a word carries: one above the largest node (or
/// row) id and edge id present at any point of the word so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct C8Floors {
    /// The node or row floor.
    pub node: u32,
    /// The edge floor (unused for tables and trees).
    pub edge: u32,
}

impl C8Floors {
    /// The floors of a graph at the start of a word.
    #[must_use]
    pub fn of_graph(graph: &Graph) -> Self {
        Self {
            node: c8_fresh(graph.nodes.keys().copied()),
            edge: c8_fresh(graph.edges.keys().copied()),
        }
    }

    /// The floors after a word reaches `graph`.
    #[must_use]
    pub fn after_graph(self, graph: &Graph) -> Self {
        let next = Self::of_graph(graph);
        Self {
            node: self.node.max(next.node),
            edge: self.edge.max(next.edge),
        }
    }

    /// The floor of a table at the start of a word.
    #[must_use]
    pub fn of_table(table: &crate::model::Table) -> Self {
        Self {
            node: c8_fresh(table.rows.keys().copied()),
            edge: 0,
        }
    }

    /// The floor after a word reaches `table`.
    #[must_use]
    pub fn after_table(self, table: &crate::model::Table) -> Self {
        Self {
            node: self.node.max(Self::of_table(table).node),
            edge: 0,
        }
    }

    /// The floor of a tree at the start of a word.
    #[must_use]
    pub fn of_tree(tree: &crate::model::Tree) -> Self {
        Self {
            node: c8_fresh(tree.nodes.keys().copied()),
            edge: 0,
        }
    }

    /// The floor after a word reaches `tree`.
    #[must_use]
    pub fn after_tree(self, tree: &crate::model::Tree) -> Self {
        Self {
            node: self.node.max(Self::of_tree(tree).node),
            edge: 0,
        }
    }
}

/// The ids an addition draws: the fresh id and the floor, ascending and
/// without repetition.
fn c8_addition_ids(fresh: u32, floor: u32) -> Vec<u32> {
    let mut ids = vec![fresh, floor];
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn c8_id(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

/// Every initial graph over `schema`.
///
/// Nodes `0 .. n-1` (n at most three), each anchored at a schema vertex
/// with a value of the alphabet, and edges `0 .. m-1` (m at most two), each
/// between any two nodes and labelled by the schema edge between their
/// anchors, which must exist. Parallel edges and self-loops are included.
#[must_use]
pub fn c8_graphs(schema: &C8Schema) -> Vec<Graph> {
    let values = c8_values();
    let payloads: Vec<(&str, &Option<Value>)> = schema
        .vertices
        .iter()
        .flat_map(|vertex| values.iter().map(move |value| (vertex.as_str(), value)))
        .collect();
    let mut out = Vec::new();
    for n in 0..=usize::from(C8.graph_nodes) {
        for choice in super::mixed(&vec![payloads.len(); n]) {
            let mut base = Graph::default();
            for (index, payload) in choice.iter().enumerate() {
                let (anchor, value) = payloads[*payload];
                let mut node = Node::new(c8_id(index), anchor);
                if let Some(value) = value {
                    node = node.with_value(value.clone());
                }
                base.nodes.insert(c8_id(index), node);
            }
            let ends: Vec<(u32, u32, SEdge)> = base
                .nodes
                .values()
                .flat_map(|src| {
                    base.nodes.values().filter_map(move |tgt| {
                        schema
                            .edge(&src.anchor, &tgt.anchor)
                            .map(|label| (src.id, tgt.id, label.clone()))
                    })
                })
                .collect();
            for m in 0..=usize::from(C8.graph_edges) {
                for choice in super::mixed(&vec![ends.len(); m]) {
                    let mut graph = base.clone();
                    for (index, end) in choice.iter().enumerate() {
                        let (src, tgt, label) = ends[*end].clone();
                        graph.edges.insert(
                            c8_id(index),
                            GEdge {
                                src,
                                tgt,
                                label,
                                position: None,
                            },
                        );
                    }
                    out.push(graph);
                }
            }
        }
    }
    out
}

/// The alphabet of graph edits at `graph` with the word's `floors`.
///
/// In order: each node addition under the fresh id and the floor, with
/// each schema vertex and each value; each node deletion with its payload;
/// each change of a node's value to another value of the alphabet; each
/// edge addition under the fresh edge id and the edge floor, between any
/// two nodes whose anchors the schema joins by an edge, labelled by it; and
/// each edge deletion with its payload. A node deletion refuses when the
/// node has an incident edge; every other letter applies.
#[must_use]
pub fn c8_graph_letters(schema: &C8Schema, graph: &Graph, floors: C8Floors) -> Vec<GraphDelta> {
    let values = c8_values();
    let presence = |value: &Option<Value>| value.clone().map(Presence::Present);
    let mut out = Vec::new();
    for id in c8_addition_ids(c8_fresh(graph.nodes.keys().copied()), floors.node) {
        for vertex in &schema.vertices {
            for value in &values {
                let mut node = Node::new(id, vertex);
                node.value = presence(value);
                let mut delta = GraphDelta::identity();
                delta.add.insert(GElem::Node(id), GPayload::Node(node));
                out.push(delta);
            }
        }
    }
    for (id, node) in &graph.nodes {
        let mut delta = GraphDelta::identity();
        delta
            .del
            .insert(GElem::Node(*id), GPayload::Node(node.clone()));
        out.push(delta);
    }
    for (id, node) in &graph.nodes {
        for value in &values {
            let new = presence(value);
            if new != node.value {
                let mut delta = GraphDelta::identity();
                delta.attrs.insert(
                    AttrKey::Value(*id),
                    (AttrVal::Presence(node.value.clone()), AttrVal::Presence(new)),
                );
                out.push(delta);
            }
        }
    }
    for id in c8_addition_ids(c8_fresh(graph.edges.keys().copied()), floors.edge) {
        for src in graph.nodes.values() {
            for tgt in graph.nodes.values() {
                if let Some(label) = schema.edge(&src.anchor, &tgt.anchor) {
                    let mut delta = GraphDelta::identity();
                    delta.add.insert(
                        GElem::Edge(id),
                        GPayload::Edge(GEdge {
                            src: src.id,
                            tgt: tgt.id,
                            label: label.clone(),
                            position: None,
                        }),
                    );
                    out.push(delta);
                }
            }
        }
    }
    for (id, edge) in &graph.edges {
        let mut delta = GraphDelta::identity();
        delta
            .del
            .insert(GElem::Edge(*id), GPayload::Edge(edge.clone()));
        out.push(delta);
    }
    out
}

/// One word of the coverage scope and its reference outcome.
#[derive(Clone, Copy, Debug)]
pub struct C8Word<'a, L, S, E> {
    /// The letters, in order.
    pub letters: &'a [L],
    /// The state the whole word reaches, or the refusal of its last
    /// letter.
    pub outcome: &'a Result<S, E>,
}

fn c8_walk_graph(
    schema: &C8Schema,
    state: &Graph,
    floors: C8Floors,
    prefix: &mut Vec<GraphDelta>,
    visit: &mut dyn FnMut(C8Word<'_, GraphDelta, Graph, crate::edit::ApplyError>),
) {
    if prefix.len() == usize::from(C8.sequence_len) {
        return;
    }
    for letter in c8_graph_letters(schema, state, floors) {
        let outcome = crate::edit::apply(&letter, state);
        prefix.push(letter);
        visit(C8Word {
            letters: prefix.as_slice(),
            outcome: &outcome,
        });
        if let Ok(next) = &outcome {
            c8_walk_graph(schema, next, floors.after_graph(next), prefix, visit);
        }
        prefix.pop();
    }
}

/// Visit every graph word of the coverage scope from `graph`, the empty
/// word first, with its reference outcome.
///
/// Each letter is drawn from [`c8_graph_letters`] at the state its prefix
/// reaches, with the floors that prefix carries; a refused letter ends its
/// word.
pub fn c8_graph_words(
    schema: &C8Schema,
    graph: &Graph,
    visit: &mut dyn FnMut(C8Word<'_, GraphDelta, Graph, crate::edit::ApplyError>),
) {
    let start = Ok(graph.clone());
    visit(C8Word {
        letters: &[],
        outcome: &start,
    });
    c8_walk_graph(schema, graph, C8Floors::of_graph(graph), &mut Vec::new(), visit);
}

/// Every initial table over `schema`.
///
/// Rows `0 .. n-1` (n at most three), each in a schema vertex's table with
/// the column absent or holding a value of the alphabet, and each set of at
/// most two foreign-key pairs, each under a schema edge between rows of its
/// source and target tables.
#[must_use]
pub fn c8_tables(schema: &C8Schema) -> Vec<crate::model::Table> {
    let values = c8_values();
    let payloads: Vec<(&str, &Option<Value>)> = schema
        .vertices
        .iter()
        .flat_map(|vertex| values.iter().map(move |value| (vertex.as_str(), value)))
        .collect();
    let mut out = Vec::new();
    for n in 0..=usize::from(C8.table_rows) {
        for choice in super::mixed(&vec![payloads.len(); n]) {
            let rows: BTreeMap<u32, (String, BTreeMap<String, Value>)> = choice
                .iter()
                .enumerate()
                .map(|(index, payload)| {
                    let (table, value) = payloads[*payload];
                    let cells = value
                        .iter()
                        .map(|value| (C8.column.to_owned(), value.clone()))
                        .collect();
                    (c8_id(index), (table.to_owned(), cells))
                })
                .collect();
            let possible = c8_table_pairs(schema, &rows);
            for pairs in super::subsets(&possible, usize::from(C8.table_pairs)) {
                out.push(crate::model::Table {
                    rows: rows.clone(),
                    pairs: pairs.into_iter().collect(),
                });
            }
        }
    }
    out
}

fn c8_table_pairs(
    schema: &C8Schema,
    rows: &BTreeMap<u32, (String, BTreeMap<String, Value>)>,
) -> Vec<(SEdge, u32, u32)> {
    let mut out = Vec::new();
    for edge in &schema.edges {
        for (src, (src_table, _)) in rows {
            for (tgt, (tgt_table, _)) in rows {
                if *src_table == edge.src && *tgt_table == edge.tgt {
                    out.push((edge.clone(), *src, *tgt));
                }
            }
        }
    }
    out
}

/// The alphabet of surface table edits at `table` with the word's
/// `floors`.
///
/// In order: each row insertion under the fresh id and the floor, into each
/// schema vertex's table, with the column absent or holding each value; each
/// row deletion stating the row's table, cells and incident pairs; each
/// change of a row's column to another value of the alphabet; each
/// foreign-key addition under each schema edge between rows of its tables;
/// and each removal of a present pair. Adding a pair that is present
/// refuses; every other letter applies.
#[must_use]
pub fn c8_table_letters(
    schema: &C8Schema,
    table: &crate::model::Table,
    floors: C8Floors,
) -> Vec<crate::edit::TableEdit> {
    use crate::edit::TableEdit;

    let values = c8_values();
    let cells = |value: &Option<Value>| -> BTreeMap<String, Value> {
        value
            .iter()
            .map(|value| (C8.column.to_owned(), value.clone()))
            .collect()
    };
    let mut out = Vec::new();
    for id in c8_addition_ids(c8_fresh(table.rows.keys().copied()), floors.node) {
        for vertex in &schema.vertices {
            for value in &values {
                out.push(TableEdit::InsertRow {
                    table: vertex.clone(),
                    id,
                    row: cells(value),
                });
            }
        }
    }
    for (id, (name, row)) in &table.rows {
        out.push(TableEdit::DeleteRow {
            table: name.clone(),
            id: *id,
            row: row.clone(),
            incident: table
                .pairs
                .iter()
                .filter(|(_, src, tgt)| src == id || tgt == id)
                .cloned()
                .collect(),
        });
    }
    for (id, (name, row)) in &table.rows {
        let old = row.get(C8.column).cloned();
        for value in &values {
            if *value != old {
                out.push(TableEdit::UpdateCell {
                    table: name.clone(),
                    id: *id,
                    column: C8.column.to_owned(),
                    old: old.clone(),
                    new: value.clone(),
                });
            }
        }
    }
    for (edge, src, tgt) in c8_table_pairs(schema, &table.rows) {
        out.push(TableEdit::AddForeignKey { edge, src, tgt });
    }
    for (edge, src, tgt) in &table.pairs {
        out.push(TableEdit::RemoveForeignKey {
            edge: edge.clone(),
            src: *src,
            tgt: *tgt,
        });
    }
    out
}

fn c8_walk_table(
    schema: &C8Schema,
    state: &crate::model::Table,
    floors: C8Floors,
    prefix: &mut Vec<crate::edit::TableEdit>,
    visit: &mut dyn FnMut(
        C8Word<'_, crate::edit::TableEdit, crate::model::Table, crate::edit::TableApplyError>,
    ),
) {
    if prefix.len() == usize::from(C8.sequence_len) {
        return;
    }
    for letter in c8_table_letters(schema, state, floors) {
        let outcome = crate::edit::apply_table_step(&letter, state);
        prefix.push(letter);
        visit(C8Word {
            letters: prefix.as_slice(),
            outcome: &outcome,
        });
        if let Ok(next) = &outcome {
            c8_walk_table(schema, next, floors.after_table(next), prefix, visit);
        }
        prefix.pop();
    }
}

/// Visit every table word of the coverage scope from `table`, the empty
/// word first, with its reference outcome under
/// [`crate::edit::apply_table_step`].
pub fn c8_table_words(
    schema: &C8Schema,
    table: &crate::model::Table,
    visit: &mut dyn FnMut(
        C8Word<'_, crate::edit::TableEdit, crate::model::Table, crate::edit::TableApplyError>,
    ),
) {
    let start = Ok(table.clone());
    visit(C8Word {
        letters: &[],
        outcome: &start,
    });
    c8_walk_table(schema, table, C8Floors::of_table(table), &mut Vec::new(), visit);
}

/// Every initial tree over `schema`.
///
/// Nodes `0 .. n-1` (n from one to three) rooted at node 0, under every
/// parent map that reaches the root and every order of its arcs, each node
/// anchored at a schema vertex with the field absent or holding a value of
/// the alphabet, and every arc labelled by the schema edge from its
/// parent's anchor to its child's, which must exist. Trees carry no fans:
/// the scope's schemas declare no hyper-edges.
#[must_use]
pub fn c8_trees(schema: &C8Schema) -> Vec<crate::model::Tree> {
    let values = c8_values();
    let mut out = Vec::new();
    for n in 1..=usize::from(C8.tree_nodes) {
        for parents in super::functions(n - 1, n) {
            let parent = |child: usize| parents[child - 1];
            let reaches = (1..n).all(|child| {
                let mut at = child;
                for _ in 0..n {
                    if at == 0 {
                        return true;
                    }
                    at = parent(at);
                }
                false
            });
            if !reaches {
                continue;
            }
            for anchors in super::mixed(&vec![schema.vertices.len(); n]) {
                let anchor = |index: usize| schema.vertices[anchors[index]].as_str();
                let labels: Option<Vec<SEdge>> = (1..n)
                    .map(|child| schema.edge(anchor(parent(child)), anchor(child)).cloned())
                    .collect();
                let Some(labels) = labels else {
                    continue;
                };
                for order in super::permutations(n - 1) {
                    let arcs: Vec<(u32, u32, SEdge)> = order
                        .iter()
                        .map(|index| {
                            let child = index + 1;
                            (c8_id(parent(child)), c8_id(child), labels[*index].clone())
                        })
                        .collect();
                    for fields in super::mixed(&vec![values.len(); n]) {
                        let nodes = (0..n)
                            .map(|index| {
                                let mut node = Node::new(c8_id(index), anchor(index));
                                if let Some(value) = &values[fields[index]] {
                                    node = node.with_field(C8.column, value.clone());
                                }
                                (c8_id(index), node)
                            })
                            .collect();
                        out.push(crate::model::Tree {
                            nodes,
                            arcs: arcs.clone(),
                            fans: Vec::new(),
                            root: 0,
                        });
                    }
                }
            }
        }
    }
    out
}

/// The alphabet of tree edits at `tree` with the word's `floors`.
///
/// In order: each insertion under each node, with the fresh id and the
/// floor, each schema vertex and each field value, whose arc is the schema
/// edge from the parent's anchor to the child's, which must exist; each leaf
/// deletion; each change of a node's field to another value of the alphabet
/// (`RemoveField` for absent, `SetField` otherwise); each change of a node's
/// anchor to another schema vertex; and each parent change of a non-root
/// node to another node than its parent, under the schema edge from the new
/// parent's anchor to the node's, which must exist. Deleting the root or a
/// node with children and moving a node under itself or a descendant
/// refuse; every other letter applies.
#[must_use]
pub fn c8_tree_letters(
    schema: &C8Schema,
    tree: &crate::model::Tree,
    floors: C8Floors,
) -> Vec<crate::edit::TreeEdit> {
    use crate::edit::TreeEdit;

    let values = c8_values();
    let mut out = Vec::new();
    let ids = c8_addition_ids(c8_fresh(tree.nodes.keys().copied()), floors.node);
    for parent in tree.nodes.values() {
        for id in &ids {
            for vertex in &schema.vertices {
                let Some(edge) = schema.edge(&parent.anchor, vertex) else {
                    continue;
                };
                for value in &values {
                    let mut node = Node::new(*id, vertex);
                    if let Some(value) = value {
                        node = node.with_field(C8.column, value.clone());
                    }
                    out.push(TreeEdit::InsertNode {
                        parent: parent.id,
                        child_id: *id,
                        node,
                        edge: edge.clone(),
                    });
                }
            }
        }
    }
    for id in tree.nodes.keys() {
        out.push(TreeEdit::DeleteNode { id: *id });
    }
    for (id, node) in &tree.nodes {
        let old = node.fields.get(C8.column).cloned();
        for value in &values {
            if *value == old {
                continue;
            }
            out.push(value.as_ref().map_or_else(
                || TreeEdit::RemoveField {
                    node_id: *id,
                    field: C8.column.to_owned(),
                },
                |value| TreeEdit::SetField {
                    node_id: *id,
                    field: C8.column.to_owned(),
                    value: value.clone(),
                },
            ));
        }
    }
    for (id, node) in &tree.nodes {
        for vertex in &schema.vertices {
            if *vertex != node.anchor {
                out.push(TreeEdit::RelabelNode {
                    id: *id,
                    new_anchor: vertex.clone(),
                });
            }
        }
    }
    for (id, node) in &tree.nodes {
        let Some(current) = tree
            .arcs
            .iter()
            .find_map(|(parent, child, _)| (child == id).then_some(*parent))
        else {
            continue;
        };
        for parent in tree.nodes.values() {
            if parent.id == current {
                continue;
            }
            if let Some(edge) = schema.edge(&parent.anchor, &node.anchor) {
                out.push(TreeEdit::MoveSubtree {
                    node_id: *id,
                    new_parent: parent.id,
                    edge: edge.clone(),
                });
            }
        }
    }
    out
}

fn c8_walk_tree(
    schema: &C8Schema,
    state: &crate::model::Tree,
    embedding: &crate::encode::TreeEmbedding,
    floors: C8Floors,
    prefix: &mut Vec<crate::edit::TreeEdit>,
    visit: &mut dyn FnMut(
        C8Word<'_, crate::edit::TreeEdit, crate::model::Tree, crate::edit::TreeApplyError>,
    ),
) {
    if prefix.len() == usize::from(C8.sequence_len) {
        return;
    }
    for letter in c8_tree_letters(schema, state, floors) {
        let mut next_embedding = embedding.clone();
        let outcome = crate::edit::apply_tree(&letter, state, &mut next_embedding);
        prefix.push(letter);
        visit(C8Word {
            letters: prefix.as_slice(),
            outcome: &outcome,
        });
        if let Ok(next) = &outcome {
            c8_walk_tree(
                schema,
                next,
                &next_embedding,
                floors.after_tree(next),
                prefix,
                visit,
            );
        }
        prefix.pop();
    }
}

/// Visit every tree word of the coverage scope from `tree`, the empty word
/// first, with its reference outcome under [`crate::edit::apply_tree`] from
/// the tree's graph embedding.
///
/// # Errors
///
/// The tree's encoding refusal when it has no graph form; no tree of
/// [`c8_trees`] has one.
pub fn c8_tree_words(
    schema: &C8Schema,
    tree: &crate::model::Tree,
    visit: &mut dyn FnMut(
        C8Word<'_, crate::edit::TreeEdit, crate::model::Tree, crate::edit::TreeApplyError>,
    ),
) -> Result<(), crate::encode::TreeEncodeError> {
    let (_, embedding) = crate::encode::tree_to_graph(tree, None)?;
    let start = Ok(tree.clone());
    visit(C8Word {
        letters: &[],
        outcome: &start,
    });
    c8_walk_tree(
        schema,
        tree,
        &embedding,
        C8Floors::of_tree(tree),
        &mut Vec::new(),
        visit,
    );
    Ok(())
}

/// Independent counts of the coverage scope, derived without the
/// enumerators above.
///
/// Node values, cells and fields never change which letters exist or apply,
/// so the counts run on value-free structures and multiply by three per
/// node, row or tree node; arc orders multiply tree counts the same way.
/// `states` counts initial states over all sixteen schemas, and
/// `words[l]` the words of length exactly `l` from them, the empty word
/// included at length zero and refused words included at the length of
/// their refused letter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct C8Counts {
    /// Initial states.
    pub states: u64,
    /// Words by exact length.
    pub words: [u64; 4],
}

impl C8Counts {
    /// Words of every length.
    #[must_use]
    pub const fn total_words(self) -> u64 {
        self.words[0] + self.words[1] + self.words[2] + self.words[3]
    }
}

/// Number of schemas in [`c8_schemas`]: the empty schema, one vertex with
/// no edge or its loop (two each for `a` and `b`), and both vertices with
/// at most two of their four edges (`1 + 4 + 6 = 11`).
#[must_use]
pub const fn c8_schemas_count_certificate() -> u64 {
    1 + 2 + 2 + 11
}

/// The labelled graph witness carrier: at most three nodes and one edge.
///
/// Three is the coverage scope's node bound; the scope itself, with its
/// two-edge bound, is enumerated by [`c8_graphs`].
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
/// This is the fixed-alphabet witness carrier; the coverage scope, whose
/// alphabets are regenerated at each state, is walked by [`c8_graph_words`].
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

/// The eight edits in the named projection-lens translation carrier.
///
/// The source is a root with one retained leaf and one hidden leaf. The get
/// cases contain identity, deletion of each kind of leaf, and replacement of
/// each kind of leaf (deleting it and inserting an equal leaf under its id).
/// The put cases contain identity, deletion of the retained leaf and its
/// replacement. Together they distinguish an observable edit from an edit
/// absorbed into the complement in the forward direction, check residue
/// restoration in the backward direction, and check that a replacement is
/// carried through the translation rather than read back as no change.
#[must_use]
pub fn edit_translation_cases() -> Vec<EditTranslationCase> {
    use crate::edit::TreeEdit;

    let replace = |id: u32, anchor: &str| {
        TreeEdit::Sequence(vec![
            TreeEdit::DeleteNode { id },
            TreeEdit::InsertNode {
                parent: 0,
                child_id: id,
                node: crate::model::Node::new(id, anchor),
                edge: SEdge::new("root", anchor, "prop", Some(anchor)),
            },
        ])
    };
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
            name: "get-replace-visible",
            direction: EditTranslationDirection::Get,
            edit: replace(1, "visible"),
        },
        EditTranslationCase {
            name: "get-replace-hidden",
            direction: EditTranslationDirection::Get,
            edit: replace(2, "hidden"),
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
        EditTranslationCase {
            name: "put-replace-visible",
            direction: EditTranslationDirection::Put,
            edit: replace(1, "visible"),
        },
    ]
}

/// Independent count of [`edit_translation_cases`]: five get cases
/// (identity, two deletions, two replacements) and three put cases
/// (identity, one deletion, one replacement).
#[must_use]
pub const fn edit_translation_cases_count_certificate() -> u64 {
    5 + 3
}

/// One named `JoinFeatures` case of the TR14 carrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinCase {
    /// Stable diagnostic name for the case.
    pub name: &'static str,
    /// The tree the join is applied to.
    pub tree: crate::model::Tree,
    /// The join.
    pub edit: crate::edit::TreeEdit,
    /// Whether the join applies.
    pub applies: bool,
}

fn join_tree(nodes: &[(u32, &str)], arcs: &[(u32, u32, &str)]) -> crate::model::Tree {
    let anchor = |id: u32| {
        nodes
            .iter()
            .find(|(node, _)| *node == id)
            .map_or("", |(_, anchor)| *anchor)
    };
    crate::model::Tree {
        nodes: nodes
            .iter()
            .map(|(id, anchor)| (*id, crate::model::Node::new(*id, anchor)))
            .collect(),
        arcs: arcs
            .iter()
            .map(|(parent, child, name)| {
                (
                    *parent,
                    *child,
                    SEdge::new(anchor(*parent), anchor(*child), "child", Some(name)),
                )
            })
            .collect(),
        fans: Vec::new(),
        root: 0,
    }
}

fn join(primary: u32, joined: Vec<u32>, anchor: &str, id: u32) -> crate::edit::TreeEdit {
    crate::edit::TreeEdit::JoinFeatures {
        primary,
        joined,
        produce: crate::model::Node::new(id, anchor),
    }
}

/// The TR14 join carrier.
///
/// Two joins apply. The first joins one node with one child on the probe
/// tree `r(0) -> a(1) -> c(3), r -> a(2)`. The second joins nodes 3 and 2,
/// in that order, into node 1 of a tree in which each joined node has two
/// children, one fan names joined node 2 and one fan names node 4, a child
/// that the join re-parents; the product changes the primary's anchor. The
/// seven refusals on the probe tree join the root, an ancestor of the
/// primary, a missing primary, a missing node, one node twice and the
/// primary itself, and give the product another id.
#[must_use]
pub fn join_cases() -> Vec<JoinCase> {
    let probe = join_tree(
        &[(0, "r"), (1, "a"), (2, "a"), (3, "c")],
        &[(0, 1, "one"), (0, 2, "two"), (1, 3, "child")],
    );
    let mut two = join_tree(
        &[
            (0, "r"),
            (1, "p"),
            (2, "j"),
            (3, "j"),
            (4, "x"),
            (5, "x"),
            (6, "x"),
            (7, "x"),
        ],
        &[
            (0, 1, "p"),
            (0, 2, "j1"),
            (0, 3, "j2"),
            (2, 4, "c1"),
            (2, 5, "c2"),
            (3, 6, "c3"),
            (3, 7, "c4"),
        ],
    );
    two.fans.push(crate::model::Fan {
        parent: 0,
        hyper_edge: "h".to_owned(),
        children: BTreeMap::from([("arm".to_owned(), 2)]),
    });
    two.fans.push(crate::model::Fan {
        parent: 1,
        hyper_edge: "g".to_owned(),
        children: BTreeMap::from([("arm".to_owned(), 4)]),
    });
    let case = |name, tree: &crate::model::Tree, edit, applies| JoinCase {
        name,
        tree: tree.clone(),
        edit,
        applies,
    };
    vec![
        case("join-one", &probe, join(2, vec![1], "a", 2), true),
        case(
            "join-two-with-fans",
            &two,
            join(1, vec![3, 2], "q", 1),
            true,
        ),
        case("joined-root", &probe, join(1, vec![0], "a", 1), false),
        case("joined-ancestor", &probe, join(3, vec![1], "c", 3), false),
        case("missing-primary", &probe, join(9, vec![2], "a", 9), false),
        case("missing-joined", &probe, join(1, vec![9], "a", 1), false),
        case(
            "repeated-joined",
            &probe,
            join(1, vec![2, 2], "a", 1),
            false,
        ),
        case("joined-primary", &probe, join(1, vec![1], "a", 1), false),
        case("misfiled-product", &probe, join(1, vec![2], "a", 2), false),
    ]
}

/// Independent count of [`join_cases`]: two joins that apply and seven
/// refusals.
#[must_use]
pub const fn join_cases_count_certificate() -> u64 {
    2 + 7
}

/// Independent count of [`c8_graphs`] over [`c8_schemas`] and of the words
/// [`c8_graph_words`] visits from them.
#[must_use]
pub const fn c8_graph_counts_certificate() -> C8Counts {
    C8Counts {
        states: 61_066,
        words: [61_066, 1_381_020, 31_675_362, 814_704_552],
    }
}

/// Independent count of [`c8_tables`] over [`c8_schemas`] and of the words
/// [`c8_table_words`] visits from them.
#[must_use]
pub const fn c8_table_counts_certificate() -> C8Counts {
    C8Counts {
        states: 32_038,
        words: [32_038, 711_888, 15_862_728, 386_747_526],
    }
}

/// Independent count of [`c8_trees`] over [`c8_schemas`] and of the words
/// [`c8_tree_words`] visits from them.
#[must_use]
pub const fn c8_tree_counts_certificate() -> C8Counts {
    C8Counts {
        states: 3_156,
        words: [3_156, 75_216, 1_898_478, 59_260_194],
    }
}
