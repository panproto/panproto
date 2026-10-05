//! Relational Kan extensions for graph-shaped instances.
//!
//! A graph in [`EncodingSide`] is encoded as a conforming [`FInstance`]
//! together with a [`GraphLayout`]. The table instance carries the relational
//! data. The layout carries graph-only identity, positions, value-subtree
//! shape, and decorations. Graph Sigma, Delta, and Pi run the corresponding
//! table Kan extension and then decode its result with a deterministically
//! transported layout.
//!
//! The encoding is fully faithful over [`GraphKanHom`] and surjective on
//! objects onto the conforming table instances, so it is an equivalence of
//! categories (Leinster 2014, Prop 1.3.18, stated), and not an isomorphism:
//! graphs that differ only in their layout encode to one table. For a fixed
//! layout, decoding is a quasi-inverse (Leinster 2014, Ex 1.3.32(b), stated),
//! and the adjunctions `Sigma -| Delta -| Pi` of the table functors transfer
//! along the equivalence to the graph functors (Leinster 2014, Rem 2.1.11,
//! stated). Since graphs that differ only in layout are isomorphic, the layout
//! a graph Kan function selects is a choice of representative.
//!
//! A relation is a set of pairs, so graph Sigma identifies two relation edges
//! that the table Sigma carries to one pair. This is relationalization
//! (Spivak and Wisnesky 2015, Def 9.1.6 and Prop 8.3.3, proven) restricted to
//! relation pairs: rows are never identified. That this image is a reflection
//! onto the relational instances is inferred.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_schema::{Edge, Protocol, Schema};

use crate::adjunction::{
    AdjunctionError, DeltaProvenance, NonConformance, SigmaProvenance, f_counit,
    f_delta_with_provenance, f_pi_counit, f_pi_transpose_left, f_pi_transpose_right, f_pi_unit,
    f_sigma_restricted, f_sigma_with_provenance, f_transpose_left, f_transpose_right, f_unit,
};
use crate::error::{InstError, RestrictError};
use crate::functor::{FInstance, Row, same_value};
use crate::ginstance::{EdgeId, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND};
use crate::instance_hom::{FInstanceHom, HomError};
use crate::metadata::{Node, NodeShape};
use crate::migration_functor::MigrationFunctor;
use crate::pi::{PiProvenance, functor_pi_along_with_provenance, functor_pi_restricted};
use crate::rel_schema::{RelSchema, RelSchemaError, column_name};
use crate::segment::{IdAllocator, IdRange};
use crate::value::{FieldPresence, Value};
use crate::view_ref::RowId;
use crate::wtype::CompiledMigration;

/// One of the relational schema sides on which graph Kan is defined.
#[derive(Clone, Copy, Debug)]
pub enum EncodingSide<'a> {
    /// A whole relation-ified source or target schema.
    Whole(&'a RelSchema),
    /// The kept part `S'` of a migration functor's source.
    Domain(&'a MigrationFunctor),
}

impl<'a> EncodingSide<'a> {
    /// A whole relation-ified schema.
    #[must_use]
    pub const fn whole(schema: &'a RelSchema) -> Self {
        Self::Whole(schema)
    }

    /// The kept part of `functor`'s source.
    #[must_use]
    pub const fn domain(functor: &'a MigrationFunctor) -> Self {
        Self::Domain(functor)
    }

    fn data(self) -> SideData {
        match self {
            Self::Whole(schema) => SideData::whole(schema),
            Self::Domain(functor) => SideData::domain(functor),
        }
    }
}

/// The concrete objects and arrows of an [`EncodingSide`].
#[derive(Clone, Debug)]
struct SideData {
    tables: BTreeSet<Name>,
    relations: BTreeSet<Edge>,
    columns: BTreeMap<Name, BTreeMap<String, Edge>>,
    value_vertices: BTreeSet<Name>,
    inner_edges: BTreeSet<Edge>,
}

impl SideData {
    fn whole(schema: &RelSchema) -> Self {
        Self {
            tables: schema.tables().clone(),
            relations: schema.relations().clone(),
            columns: schema.all_columns().clone(),
            value_vertices: schema
                .all_columns()
                .values()
                .flat_map(BTreeMap::values)
                .map(|edge| edge.tgt.clone())
                .chain(
                    schema
                        .inner_edges()
                        .iter()
                        .flat_map(|edge| [edge.src.clone(), edge.tgt.clone()]),
                )
                .collect(),
            inner_edges: schema.inner_edges().clone(),
        }
    }

    fn domain(functor: &MigrationFunctor) -> Self {
        let source = functor.source();
        let tables: BTreeSet<Name> = functor.table_map().keys().cloned().collect();
        let relations: BTreeSet<Edge> = functor.relation_map().keys().cloned().collect();
        let mut columns: BTreeMap<Name, BTreeMap<String, Edge>> = tables
            .iter()
            .map(|table| (table.clone(), BTreeMap::new()))
            .collect();
        let mut value_vertices = BTreeSet::new();
        for (table, column) in functor.column_map().keys() {
            if let Some(edge) = source.columns(table).get(column) {
                columns
                    .entry(table.clone())
                    .or_default()
                    .insert(column.clone(), edge.clone());
                value_vertices.insert(edge.tgt.clone());
            }
        }

        let mut inner_edges = BTreeSet::new();
        loop {
            let before = value_vertices.len();
            for edge in source.inner_edges() {
                if value_vertices.contains(&edge.src) {
                    inner_edges.insert(edge.clone());
                    value_vertices.insert(edge.tgt.clone());
                }
            }
            if value_vertices.len() == before {
                break;
            }
        }
        Self {
            tables,
            relations,
            columns,
            value_vertices,
            inner_edges,
        }
    }

    fn contains_anchor(&self, anchor: &Name) -> bool {
        self.tables.contains(anchor) || self.value_vertices.contains(anchor)
    }

    fn contains_edge(&self, edge: &Edge) -> bool {
        self.relations.contains(edge)
            || self
                .columns
                .get(&edge.src)
                .is_some_and(|columns| columns.values().any(|column| column == edge))
            || self.inner_edges.contains(edge)
    }

    fn is_value(&self, anchor: &Name) -> bool {
        self.value_vertices.contains(anchor)
    }

    fn is_relation(&self, edge: &Edge) -> bool {
        self.relations.contains(edge)
    }
}

/// The first reason a graph is outside the relational graph-Kan domain.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, thiserror::Error)]
#[non_exhaustive]
pub enum NotEncodableReason {
    /// A node is a reified fan, or is the source of a reserved fan edge.
    #[error("node {node} is a fan node")]
    FanNode {
        /// The least offending node.
        node: u32,
    },
    /// A node's anchor is not an object of the encoding side.
    #[error("node {node} is anchored outside the encoding side")]
    AnchorOutsideSide {
        /// The least offending node.
        node: u32,
    },
    /// An edge's label is not an arrow of the encoding side.
    #[error("edge {edge} is labelled outside the encoding side")]
    EdgeOutsideSide {
        /// The least offending edge.
        edge: EdgeId,
    },
    /// An edge's endpoints are not anchored at its label's endpoints.
    #[error("edge {edge} does not join nodes at its label's endpoints")]
    IncoherentEndpoints {
        /// The least offending edge.
        edge: EdgeId,
    },
    /// Several graph edges represent one relational pair.
    #[error("parallel edges {edges:?} represent one relational pair")]
    ParallelEdges {
        /// The ascending edge ids of the least offending group.
        edges: Vec<EdgeId>,
    },
    /// A table node has no cell edge for a required column.
    #[error("node {node} has no cell for column `{column}`")]
    MissingCell {
        /// The table node.
        node: u32,
        /// The missing column.
        column: String,
    },
    /// A table node carries a would-be cell as an extra field.
    #[error("node {node} carries column `{column}` as an extra field, not a value node")]
    CellAsExtraField {
        /// The table node.
        node: u32,
        /// The column.
        column: String,
    },
    /// A table node has several cell edges for one column.
    #[error("node {node} has repeated cell edges {edges:?} for column `{column}`")]
    RepeatedCell {
        /// The table node.
        node: u32,
        /// The repeated column.
        column: String,
        /// The ascending cell-edge ids.
        edges: Vec<EdgeId>,
    },
    /// A value node is the target of several cell or inner-value edges.
    #[error("value node {node} is shared by edges {edges:?}")]
    SharedValue {
        /// The shared value node.
        node: u32,
        /// Its ascending in-edge ids.
        edges: Vec<EdgeId>,
    },
    /// A value node is not rooted at one table cell.
    #[error("value node {node} is not rooted at a table cell")]
    UnrootedValue {
        /// The least offending value node.
        node: u32,
    },
    /// A leaf value node has no present or null value.
    #[error("value node {node} has no cell value")]
    MissingValue {
        /// The least offending value node.
        node: u32,
    },
    /// The outgoing inner edges of a structured value do not form a record
    /// or a list.
    #[error("value node {node} has an incoherent inner-value shape")]
    InnerValueShape {
        /// The least offending value node.
        node: u32,
    },
    /// A labelled null occurs below the top level of a cell.
    #[error("value node {node} holds a nested labelled null")]
    NestedLabeledNull {
        /// The least offending value node.
        node: u32,
    },
}

/// Error from graph encoding, a graph Kan extension, or graph decoding.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GraphKanError {
    /// The graph is outside the relational graph-Kan domain.
    #[error("graph is not encodable: {reason}")]
    NotEncodable {
        /// The first reason, under the five ordered encoding passes.
        reason: NotEncodableReason,
    },
    /// A schema has no attributed relational form.
    #[error("cannot classify the schema: {0}")]
    RelSchema(#[from] RelSchemaError),
    /// The migration does not define the required relational functor.
    #[error("cannot construct the migration functor: {0}")]
    Functor(RestrictError),
    /// A table Kan extension or conformance check failed.
    #[error("the table adjunction failed: {0}")]
    Adjunction(AdjunctionError),
    /// A graph Kan homomorphism failed.
    #[error("the graph Kan homomorphism failed: {0}")]
    Hom(#[from] HomError),
    /// No fresh graph node id remains.
    #[error("no graph node id remains")]
    NodeIdsExhausted,
    /// No fresh graph edge id remains.
    #[error("no graph edge id remains")]
    EdgeIdsExhausted,
    /// Created edges cannot be positioned after the carried edges.
    #[error("created edges out of node {node} do not fit after its carried positions")]
    EdgePositionsExhausted {
        /// The least source node at which positioning overflows.
        node: u32,
    },
}

impl From<AdjunctionError> for GraphKanError {
    fn from(error: AdjunctionError) -> Self {
        Self::Adjunction(error)
    }
}

/// The graph-only data of one cell subtree.
#[derive(Clone, Debug, Default)]
pub struct CellLayout {
    /// Root value node.
    pub root: u32,
    /// The attribute edge from the row to `root`.
    pub attribute: EdgeLayout,
    /// Every node of the value subtree, by original id.
    pub nodes: BTreeMap<u32, Node>,
    /// Every inner-value edge of the subtree, by original id.
    pub edges: BTreeMap<EdgeId, GEdge>,
}

/// Identity and position available to one decoded edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EdgeLayout {
    /// The source edge id to retain when it is collision-free.
    pub id: Option<EdgeId>,
    /// The carried or selected position.
    pub position: Option<u32>,
    /// Whether the edge has at least one input contributor. Edges without one
    /// are placed after positioned carried edges.
    pub has_source: bool,
}

/// The graph-only record paired with an encoded table instance.
#[derive(Clone, Debug, Default)]
pub struct GraphLayout {
    /// Decorations of each table node, keyed by its row id.
    pub table_nodes: BTreeMap<RowId, Node>,
    /// Layout of each cell subtree, keyed by row and column.
    pub cells: BTreeMap<(RowId, String), CellLayout>,
    /// Layout of each relation edge, keyed by relation and pair.
    pub relations: BTreeMap<(Edge, RowId, RowId), EdgeLayout>,
    /// Every input/layout node id. Fresh ids lie above all of them.
    pub reserved_node_ids: BTreeSet<u32>,
    /// Every input/layout edge id. Fresh ids lie above all of them.
    pub reserved_edge_ids: BTreeSet<EdgeId>,
}

/// Encode an encodable graph as relational tables plus its graph layout.
///
/// The five encoding passes are run in their documented order and return the
/// least offender of the first failing pass.
///
/// # Errors
///
/// [`GraphKanError::NotEncodable`] when one of those passes fails.
pub fn encode_graph(
    graph: &GInstance,
    side: EncodingSide<'_>,
) -> Result<(FInstance, GraphLayout), GraphKanError> {
    let side = side.data();
    check_encodable(graph, &side)?;

    let mut tables = FInstance::new();
    let mut layout = GraphLayout {
        reserved_node_ids: graph.nodes().keys().copied().collect(),
        reserved_edge_ids: graph.edges().keys().copied().collect(),
        ..GraphLayout::default()
    };

    let mut node_ids: Vec<u32> = graph.nodes().keys().copied().collect();
    node_ids.sort_unstable();
    for id in node_ids {
        let node = &graph.nodes()[&id];
        if !side.tables.contains(&node.anchor) {
            continue;
        }
        let mut row = Row::new();
        for (column, arrow) in side.columns.get(&node.anchor).into_iter().flatten() {
            let edge_id = graph
                .out_edges(id)
                .iter()
                .copied()
                .find(|edge_id| graph.edges()[edge_id].edge == *arrow)
                .ok_or_else(|| {
                    not_encodable(NotEncodableReason::MissingCell {
                        node: id,
                        column: column.clone(),
                    })
                })?;
            let edge = &graph.edges()[&edge_id];
            let value = fold_cell(graph, edge.tgt, &side)?;
            row.insert(column.clone(), value);
            layout.cells.insert(
                (RowId(id), column.clone()),
                collect_cell_layout(graph, edge_id, edge.tgt, &side),
            );
        }
        tables
            .insert_row(node.anchor.clone(), RowId(id), row)
            .map_err(graph_construction_error)?;
        layout.table_nodes.insert(RowId(id), node.clone());
    }

    for (edge_id, edge) in graph.edges_sorted() {
        if !side.is_relation(&edge.edge) {
            continue;
        }
        tables
            .insert_foreign_key(edge.edge.clone(), RowId(edge.src), RowId(edge.tgt))
            .map_err(graph_construction_error)?;
        layout.relations.insert(
            (edge.edge.clone(), RowId(edge.src), RowId(edge.tgt)),
            EdgeLayout {
                id: Some(edge_id),
                position: edge.position,
                has_source: true,
            },
        );
    }
    Ok((tables, layout))
}

const fn not_encodable(reason: NotEncodableReason) -> GraphKanError {
    GraphKanError::NotEncodable { reason }
}

fn graph_construction_error(error: InstError) -> GraphKanError {
    match error {
        InstError::NodeIdsExhausted => GraphKanError::NodeIdsExhausted,
        InstError::EdgeIdsExhausted => GraphKanError::EdgeIdsExhausted,
        other => GraphKanError::Adjunction(AdjunctionError::Graph(other)),
    }
}

fn reserved_fan_kind(kind: &Name) -> bool {
    kind.as_ref() == HYPER_PARENT_KIND || kind.as_ref() == HYPER_LABEL_KIND
}

fn check_encodable(graph: &GInstance, side: &SideData) -> Result<(), GraphKanError> {
    let mut nodes: Vec<(u32, &Node)> = graph.nodes().iter().map(|(id, n)| (*id, n)).collect();
    nodes.sort_unstable_by_key(|(id, _)| *id);

    check_nodes(graph, side, &nodes)?;
    check_edges(graph, side)?;
    check_parallel_edges(graph)?;
    check_cells(graph, side, &nodes)?;
    check_value_trees(graph, side, nodes)
}

fn check_nodes(
    graph: &GInstance,
    side: &SideData,
    nodes: &[(u32, &Node)],
) -> Result<(), GraphKanError> {
    for (id, node) in nodes {
        let is_fan = node.shape == NodeShape::HyperEdge
            || graph.out_edges(*id).iter().any(|edge_id| {
                graph
                    .edge(*edge_id)
                    .is_some_and(|edge| reserved_fan_kind(&edge.edge.kind))
            });
        if is_fan {
            return Err(not_encodable(NotEncodableReason::FanNode { node: *id }));
        }
        if !side.contains_anchor(&node.anchor) {
            return Err(not_encodable(NotEncodableReason::AnchorOutsideSide {
                node: *id,
            }));
        }
    }
    Ok(())
}

fn check_edges(graph: &GInstance, side: &SideData) -> Result<(), GraphKanError> {
    for (id, edge) in graph.edges_sorted() {
        if !side.contains_edge(&edge.edge) {
            return Err(not_encodable(NotEncodableReason::EdgeOutsideSide {
                edge: id,
            }));
        }
        let coherent = graph
            .node(edge.src)
            .is_some_and(|node| node.anchor == edge.edge.src)
            && graph
                .node(edge.tgt)
                .is_some_and(|node| node.anchor == edge.edge.tgt);
        if !coherent {
            return Err(not_encodable(NotEncodableReason::IncoherentEndpoints {
                edge: id,
            }));
        }
    }
    Ok(())
}

fn check_parallel_edges(graph: &GInstance) -> Result<(), GraphKanError> {
    let mut parallel: BTreeMap<(Edge, u32, u32), Vec<EdgeId>> = BTreeMap::new();
    for (id, edge) in graph.edges_sorted() {
        parallel
            .entry((edge.edge.clone(), edge.src, edge.tgt))
            .or_default()
            .push(id);
    }
    if let Some(edges) = parallel
        .into_values()
        .filter(|edges| edges.len() > 1)
        .min_by_key(|edges| edges[0])
    {
        return Err(not_encodable(NotEncodableReason::ParallelEdges { edges }));
    }
    Ok(())
}

fn check_cells(
    graph: &GInstance,
    side: &SideData,
    nodes: &[(u32, &Node)],
) -> Result<(), GraphKanError> {
    for (id, node) in nodes {
        let Some(columns) = side.columns.get(&node.anchor) else {
            continue;
        };
        for (column, arrow) in columns {
            let edges: Vec<EdgeId> = graph
                .out_edges(*id)
                .iter()
                .copied()
                .filter(|edge_id| graph.edges()[edge_id].edge == *arrow)
                .collect();
            match edges.as_slice() {
                [] if node.extra_fields.contains_key(column) => {
                    return Err(not_encodable(NotEncodableReason::CellAsExtraField {
                        node: *id,
                        column: column.clone(),
                    }));
                }
                [] => {
                    return Err(not_encodable(NotEncodableReason::MissingCell {
                        node: *id,
                        column: column.clone(),
                    }));
                }
                [_] => {}
                _ => {
                    return Err(not_encodable(NotEncodableReason::RepeatedCell {
                        node: *id,
                        column: column.clone(),
                        edges,
                    }));
                }
            }
        }
    }
    Ok(())
}

fn check_value_trees(
    graph: &GInstance,
    side: &SideData,
    nodes: Vec<(u32, &Node)>,
) -> Result<(), GraphKanError> {
    for (id, node) in nodes {
        if !side.is_value(&node.anchor) {
            continue;
        }
        let incoming = graph.in_edges(id);
        if incoming.is_empty() {
            return Err(not_encodable(NotEncodableReason::UnrootedValue {
                node: id,
            }));
        }
        if incoming.len() > 1 {
            return Err(not_encodable(NotEncodableReason::SharedValue {
                node: id,
                edges: incoming.to_vec(),
            }));
        }
        if !reaches_table(graph, id, side) {
            return Err(not_encodable(NotEncodableReason::UnrootedValue {
                node: id,
            }));
        }

        let outgoing = graph.out_edges(id);
        if outgoing.is_empty() {
            match node.value.as_ref() {
                Some(FieldPresence::Present(value)) => {
                    let below_top = side.inner_edges.contains(&graph.edges()[&incoming[0]].edge);
                    if (below_top && holds_any_null(value)) || holds_nested_null(value) {
                        return Err(not_encodable(NotEncodableReason::NestedLabeledNull {
                            node: id,
                        }));
                    }
                }
                Some(FieldPresence::Null) => {}
                None | Some(FieldPresence::Absent) => {
                    return Err(not_encodable(NotEncodableReason::MissingValue { node: id }));
                }
            }
        } else {
            if node.value.is_some() {
                return Err(not_encodable(NotEncodableReason::InnerValueShape {
                    node: id,
                }));
            }
            let positions: Vec<Option<u32>> = outgoing
                .iter()
                .map(|edge| graph.edges()[edge].position)
                .collect();
            let record = positions.iter().all(Option::is_none) && {
                let names: BTreeSet<String> = outgoing
                    .iter()
                    .map(|edge| column_name(&graph.edges()[edge].edge))
                    .collect();
                names.len() == outgoing.len()
            };
            let list = positions.iter().all(Option::is_some)
                && positions.iter().copied().collect::<BTreeSet<_>>().len() == outgoing.len();
            if !record && !list {
                return Err(not_encodable(NotEncodableReason::InnerValueShape {
                    node: id,
                }));
            }
        }
    }
    Ok(())
}

fn reaches_table(graph: &GInstance, start: u32, side: &SideData) -> bool {
    let mut seen = BTreeSet::new();
    let mut node = start;
    while seen.insert(node) {
        let Some(edge_id) = graph.in_edges(node).first() else {
            return false;
        };
        let Some(edge) = graph.edge(*edge_id) else {
            return false;
        };
        let Some(parent) = graph.node(edge.src) else {
            return false;
        };
        if side.tables.contains(&parent.anchor) {
            return true;
        }
        if !side.is_value(&parent.anchor) {
            return false;
        }
        node = edge.src;
    }
    false
}

fn holds_any_null(value: &Value) -> bool {
    match value {
        Value::LabeledNull(_) => true,
        Value::List(values) => values.iter().any(holds_any_null),
        Value::Unknown(fields) | Value::Opaque { fields, .. } => {
            fields.values().any(holds_any_null)
        }
        _ => false,
    }
}

fn holds_nested_null(value: &Value) -> bool {
    match value {
        Value::List(values) => values.iter().any(holds_any_null),
        Value::Unknown(fields) | Value::Opaque { fields, .. } => {
            fields.values().any(holds_any_null)
        }
        _ => false,
    }
}

fn fold_cell(graph: &GInstance, root: u32, side: &SideData) -> Result<Value, GraphKanError> {
    let node = &graph.nodes()[&root];
    let outgoing = graph.out_edges(root);
    if outgoing.is_empty() {
        return match node.value.as_ref() {
            Some(FieldPresence::Present(value)) => Ok(value.clone()),
            Some(FieldPresence::Null) => Ok(Value::Null),
            None | Some(FieldPresence::Absent) => {
                Err(not_encodable(NotEncodableReason::MissingValue {
                    node: root,
                }))
            }
        };
    }
    let positioned = outgoing
        .iter()
        .all(|edge| graph.edges()[edge].position.is_some());
    if positioned {
        let mut edges: Vec<EdgeId> = outgoing.to_vec();
        edges.sort_unstable_by_key(|edge| (graph.edges()[edge].position, *edge));
        return edges
            .into_iter()
            .map(|edge| fold_cell(graph, graph.edges()[&edge].tgt, side))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::List);
    }
    let mut fields = HashMap::new();
    for edge_id in outgoing {
        let edge = &graph.edges()[edge_id];
        if !side.inner_edges.contains(&edge.edge) {
            return Err(not_encodable(NotEncodableReason::EdgeOutsideSide {
                edge: *edge_id,
            }));
        }
        fields.insert(column_name(&edge.edge), fold_cell(graph, edge.tgt, side)?);
    }
    Ok(Value::Unknown(fields))
}

fn collect_cell_layout(
    graph: &GInstance,
    attribute: EdgeId,
    root: u32,
    side: &SideData,
) -> CellLayout {
    let mut layout = CellLayout {
        root,
        attribute: EdgeLayout {
            id: Some(attribute),
            position: graph.edges()[&attribute].position,
            has_source: true,
        },
        ..CellLayout::default()
    };
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        if layout.nodes.contains_key(&id) {
            continue;
        }
        layout.nodes.insert(id, graph.nodes()[&id].clone());
        let mut edges: Vec<EdgeId> = graph
            .out_edges(id)
            .iter()
            .copied()
            .filter(|edge_id| side.inner_edges.contains(&graph.edges()[edge_id].edge))
            .collect();
        edges.sort_unstable_by(|left, right| {
            let a = &graph.edges()[left];
            let b = &graph.edges()[right];
            (a.position, column_name(&a.edge), *left).cmp(&(
                b.position,
                column_name(&b.edge),
                *right,
            ))
        });
        for edge_id in edges.into_iter().rev() {
            let edge = graph.edges()[&edge_id].clone();
            pending.push(edge.tgt);
            layout.edges.insert(edge_id, edge);
        }
    }
    layout
}

/// Decode a conforming table instance through `layout` into an encodable
/// graph. A default layout produces one leaf value node per cell.
///
/// # Errors
///
/// Returns the first conformance error, then node-id, edge-id, or edge-position
/// exhaustion. No partial graph escapes.
pub fn decode_graph(
    tables: &FInstance,
    side: EncodingSide<'_>,
    layout: &GraphLayout,
) -> Result<GInstance, GraphKanError> {
    decode_with_trace(tables, side, layout).map(|decoded| decoded.graph)
}

#[derive(Clone, Debug, Default)]
struct CellTrace {
    nodes: BTreeSet<u32>,
    edges: BTreeSet<EdgeId>,
}

#[derive(Clone, Debug)]
struct DecodeResult {
    graph: GInstance,
    cells: BTreeMap<(RowId, String), CellTrace>,
    relations: BTreeMap<(Edge, RowId, RowId), EdgeId>,
}

#[derive(Clone, Debug)]
struct PendingEdge {
    id: EdgeId,
    edge: GEdge,
    created: bool,
}

struct DecodeState {
    graph: GInstance,
    pending_edges: Vec<PendingEdge>,
    claimed_nodes: BTreeSet<u32>,
    claimed_edges: BTreeSet<EdgeId>,
    node_allocator: IdAllocator,
    edge_allocator: IdAllocator,
}

fn decode_with_trace(
    tables: &FInstance,
    side: EncodingSide<'_>,
    layout: &GraphLayout,
) -> Result<DecodeResult, GraphKanError> {
    let side = side.data();
    conform(tables, &side)?;

    let row_ids: BTreeSet<u32> = tables.row_ids().into_iter().map(|row| row.0).collect();
    let node_floor = row_ids
        .iter()
        .copied()
        .chain(layout.reserved_node_ids.iter().copied());
    let edge_floor = layout.reserved_edge_ids.iter().map(|edge| edge.0);
    let node_allocator = IdAllocator::from_source(IdRange::FULL, node_floor);
    let edge_allocator = IdAllocator::from_source(IdRange::FULL, edge_floor);
    let mut state = DecodeState {
        graph: GInstance::new(),
        pending_edges: Vec::new(),
        claimed_nodes: row_ids.clone(),
        claimed_edges: BTreeSet::new(),
        node_allocator,
        edge_allocator,
    };
    let mut cells = BTreeMap::new();
    let mut relations = BTreeMap::new();

    for (table, rows) in tables.tables() {
        for row in rows.ids() {
            let mut node = layout
                .table_nodes
                .get(&row)
                .cloned()
                .unwrap_or_else(|| Node::new(row.0, table.clone()));
            node.id = row.0;
            node.anchor = table.clone();
            state
                .graph
                .insert_node(node)
                .map_err(graph_construction_error)?;
        }
    }

    for (table, rows) in tables.tables() {
        let columns = side.columns.get(table).cloned().unwrap_or_default();
        for row in rows.ids() {
            let values = tables.row(row).cloned().unwrap_or_default();
            for (column, arrow) in &columns {
                let value = values.get(column).ok_or_else(|| {
                    GraphKanError::Adjunction(AdjunctionError::NonConforming {
                        row,
                        column: Some(column.clone()),
                        reason: NonConformance::MissingCell,
                    })
                })?;
                let candidate = layout
                    .cells
                    .get(&(row, column.clone()))
                    .and_then(|cell| relabel_cell_layout(cell, value, arrow, &side));
                let trace = if let Some(cell) = candidate.as_ref() {
                    decode_cell_layout(cell, row, arrow, &mut state)?
                } else {
                    decode_record_free_cell(value, row, arrow, &mut state)?
                };
                cells.insert((row, column.clone()), trace);
            }
        }
    }

    for (edge, pairs) in tables.foreign_keys() {
        for &(src, tgt) in pairs {
            let candidate = layout.relations.get(&(edge.clone(), src, tgt)).copied();
            let id = choose_edge_id(
                candidate.and_then(|entry| entry.id),
                &mut state.claimed_edges,
                &mut state.edge_allocator,
            )?;
            let entry = candidate.unwrap_or_default();
            state.pending_edges.push(PendingEdge {
                id,
                edge: GEdge::new(src.0, tgt.0, edge.clone()).with_position(entry.position),
                created: !entry.has_source,
            });
            relations.insert((edge.clone(), src, tgt), id);
        }
    }

    assign_created_positions(&mut state.pending_edges)?;
    state.pending_edges.sort_unstable_by_key(|edge| edge.id);
    for pending in state.pending_edges {
        state
            .graph
            .insert_edge(pending.id, pending.edge)
            .map_err(graph_construction_error)?;
    }
    Ok(DecodeResult {
        graph: state.graph,
        cells,
        relations,
    })
}

fn allocate_fresh(allocator: &mut IdAllocator) -> Result<u32, ()> {
    if allocator.floor().is_none_or(|floor| floor == u32::MAX) {
        return Err(());
    }
    allocator.allocate().map_err(|_| ())
}

fn choose_node_id(
    candidate: Option<u32>,
    claimed: &mut BTreeSet<u32>,
    allocator: &mut IdAllocator,
) -> Result<u32, GraphKanError> {
    if let Some(id) = candidate.filter(|id| !claimed.contains(id)) {
        claimed.insert(id);
        return Ok(id);
    }
    let id = allocate_fresh(allocator).map_err(|()| GraphKanError::NodeIdsExhausted)?;
    claimed.insert(id);
    Ok(id)
}

fn choose_edge_id(
    candidate: Option<EdgeId>,
    claimed: &mut BTreeSet<EdgeId>,
    allocator: &mut IdAllocator,
) -> Result<EdgeId, GraphKanError> {
    if let Some(id) = candidate.filter(|id| !claimed.contains(id)) {
        claimed.insert(id);
        return Ok(id);
    }
    let id = EdgeId(allocate_fresh(allocator).map_err(|()| GraphKanError::EdgeIdsExhausted)?);
    claimed.insert(id);
    Ok(id)
}

/// The layout `cell` moved onto the column `arrow`: each inner edge of the
/// subtree is relabelled to the inner edge of the side out of its source's
/// new anchor with the same kind and the same field name ([`column_name`],
/// which for an unnamed edge is its target vertex). Returns the moved layout
/// when every edge has such an image, the anchors are coherent, and the
/// moved subtree folds to `value`; otherwise `None`, and the cell is decoded
/// without the layout.
fn relabel_cell_layout(
    cell: &CellLayout,
    value: &Value,
    arrow: &Edge,
    side: &SideData,
) -> Option<CellLayout> {
    let (_, edge_order) = cell_order(cell).ok()?;
    let mut anchors = BTreeMap::from([(cell.root, arrow.tgt.clone())]);
    let mut result = cell.clone();
    for id in edge_order {
        let original = cell.edges.get(&id)?;
        let source = anchors.get(&original.src)?;
        let field = column_name(&original.edge);
        let mapped = side.inner_edges.iter().find(|candidate| {
            candidate.src == *source
                && candidate.kind == original.edge.kind
                && column_name(candidate) == field
        })?;
        match anchors.entry(original.tgt) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(mapped.tgt.clone());
            }
            std::collections::btree_map::Entry::Occupied(entry) if entry.get() != &mapped.tgt => {
                return None;
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
        }
        result.edges.get_mut(&id)?.edge.clone_from(mapped);
    }
    for (id, node) in &mut result.nodes {
        node.anchor.clone_from(anchors.get(id)?);
    }
    fold_layout(&result)
        .is_ok_and(|folded| same_value(&folded, value))
        .then_some(result)
}

fn fold_layout(cell: &CellLayout) -> Result<Value, ()> {
    fn fold(cell: &CellLayout, node: u32, visiting: &mut BTreeSet<u32>) -> Result<Value, ()> {
        if !visiting.insert(node) {
            return Err(());
        }
        let current = cell.nodes.get(&node).ok_or(())?;
        let mut outgoing: Vec<(EdgeId, &GEdge)> = cell
            .edges
            .iter()
            .filter(|(_, edge)| edge.src == node)
            .map(|(id, edge)| (*id, edge))
            .collect();
        let value = if outgoing.is_empty() {
            match current.value.as_ref() {
                Some(FieldPresence::Present(value)) => value.clone(),
                Some(FieldPresence::Null) => Value::Null,
                None | Some(FieldPresence::Absent) => return Err(()),
            }
        } else if outgoing.iter().all(|(_, edge)| edge.position.is_some()) {
            outgoing.sort_unstable_by_key(|(id, edge)| (edge.position, *id));
            Value::List(
                outgoing
                    .into_iter()
                    .map(|(_, edge)| fold(cell, edge.tgt, visiting))
                    .collect::<Result<Vec<_>, _>>()?,
            )
        } else if outgoing.iter().all(|(_, edge)| edge.position.is_none()) {
            let mut fields = HashMap::new();
            for (_, edge) in outgoing {
                fields.insert(column_name(&edge.edge), fold(cell, edge.tgt, visiting)?);
            }
            Value::Unknown(fields)
        } else {
            return Err(());
        };
        visiting.remove(&node);
        Ok(value)
    }
    fold(cell, cell.root, &mut BTreeSet::new())
}

fn decode_record_free_cell(
    value: &Value,
    row: RowId,
    arrow: &Edge,
    state: &mut DecodeState,
) -> Result<CellTrace, GraphKanError> {
    let id = choose_node_id(None, &mut state.claimed_nodes, &mut state.node_allocator)?;
    let presence = if same_value(value, &Value::Null) {
        FieldPresence::Null
    } else {
        FieldPresence::Present(value.clone())
    };
    state
        .graph
        .insert_node(Node::new(id, arrow.tgt.clone()).with_value(presence))
        .map_err(graph_construction_error)?;
    let edge_id = choose_edge_id(None, &mut state.claimed_edges, &mut state.edge_allocator)?;
    state.pending_edges.push(PendingEdge {
        id: edge_id,
        edge: GEdge::new(row.0, id, arrow.clone()),
        created: true,
    });
    Ok(CellTrace {
        nodes: BTreeSet::from([id]),
        edges: BTreeSet::from([edge_id]),
    })
}

fn decode_cell_layout(
    cell: &CellLayout,
    row: RowId,
    arrow: &Edge,
    state: &mut DecodeState,
) -> Result<CellTrace, GraphKanError> {
    let (node_order, edge_order) = cell_order(cell)?;
    let mut remap = BTreeMap::new();
    let mut trace = CellTrace::default();
    for old in node_order {
        let id = choose_node_id(
            Some(old),
            &mut state.claimed_nodes,
            &mut state.node_allocator,
        )?;
        remap.insert(old, id);
        let mut node = cell.nodes[&old].clone();
        node.id = id;
        state
            .graph
            .insert_node(node)
            .map_err(graph_construction_error)?;
        trace.nodes.insert(id);
    }

    let attribute_id = choose_edge_id(
        cell.attribute.id,
        &mut state.claimed_edges,
        &mut state.edge_allocator,
    )?;
    state.pending_edges.push(PendingEdge {
        id: attribute_id,
        edge: GEdge::new(row.0, remap[&cell.root], arrow.clone())
            .with_position(cell.attribute.position),
        created: !cell.attribute.has_source,
    });
    trace.edges.insert(attribute_id);

    for old_edge in edge_order {
        let original = &cell.edges[&old_edge];
        let id = choose_edge_id(
            Some(old_edge),
            &mut state.claimed_edges,
            &mut state.edge_allocator,
        )?;
        state.pending_edges.push(PendingEdge {
            id,
            edge: GEdge::new(
                remap[&original.src],
                remap[&original.tgt],
                original.edge.clone(),
            )
            .with_position(original.position),
            created: false,
        });
        trace.edges.insert(id);
    }
    Ok(trace)
}

fn cell_order(cell: &CellLayout) -> Result<(Vec<u32>, Vec<EdgeId>), GraphKanError> {
    fn visit(
        cell: &CellLayout,
        node: u32,
        seen: &mut BTreeSet<u32>,
        nodes: &mut Vec<u32>,
        edges: &mut Vec<EdgeId>,
    ) -> Result<(), GraphKanError> {
        if !seen.insert(node) {
            return Ok(());
        }
        if !cell.nodes.contains_key(&node) {
            return Err(GraphKanError::NodeIdsExhausted);
        }
        nodes.push(node);
        let mut outgoing: Vec<(EdgeId, &GEdge)> = cell
            .edges
            .iter()
            .filter(|(_, edge)| edge.src == node)
            .map(|(id, edge)| (*id, edge))
            .collect();
        outgoing.sort_unstable_by(|(left_id, left), (right_id, right)| {
            (left.position, column_name(&left.edge), *left_id).cmp(&(
                right.position,
                column_name(&right.edge),
                *right_id,
            ))
        });
        for (id, edge) in outgoing {
            edges.push(id);
            visit(cell, edge.tgt, seen, nodes, edges)?;
        }
        Ok(())
    }

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    visit(
        cell,
        cell.root,
        &mut BTreeSet::new(),
        &mut nodes,
        &mut edges,
    )?;
    Ok((nodes, edges))
}

fn assign_created_positions(edges: &mut [PendingEdge]) -> Result<(), GraphKanError> {
    let mut by_source: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (index, edge) in edges.iter().enumerate() {
        by_source.entry(edge.edge.src).or_default().push(index);
    }
    for (node, indices) in by_source {
        let max_carried = indices
            .iter()
            .filter_map(|index| {
                (!edges[*index].created)
                    .then_some(edges[*index].edge.position)
                    .flatten()
            })
            .max();
        let Some(mut position) = max_carried else {
            continue;
        };
        let mut created: Vec<usize> = indices
            .into_iter()
            .filter(|index| edges[*index].created)
            .collect();
        created.sort_unstable_by_key(|index| edges[*index].id);
        for index in created {
            position = position
                .checked_add(1)
                .ok_or(GraphKanError::EdgePositionsExhausted { node })?;
            edges[index].edge.position = Some(position);
        }
    }
    Ok(())
}

fn conform(instance: &FInstance, side: &SideData) -> Result<(), GraphKanError> {
    let row_violation = instance.row_ids().into_iter().find_map(|id| {
        let table = instance.table_of(id)?;
        let Some(columns) = side.columns.get(table) else {
            return Some((
                id,
                None,
                NonConformance::TableOutside {
                    table: table.clone(),
                },
            ));
        };
        let cells = instance.row(id)?;
        let missing = columns
            .keys()
            .find(|column| !cells.contains_key(*column))
            .map(|column| (column.clone(), NonConformance::MissingCell));
        let stray = cells
            .keys()
            .filter(|column| !columns.contains_key(*column))
            .min()
            .map(|column| (column.clone(), NonConformance::StrayCell));
        let first = match (missing, stray) {
            (Some(missing), Some(stray)) => Some(if stray.0 < missing.0 { stray } else { missing }),
            (missing, stray) => missing.or(stray),
        };
        if let Some((column, reason)) = first {
            return Some((id, Some(column), reason));
        }
        columns.keys().find_map(|column| {
            cells
                .get(column)
                .is_some_and(holds_nested_null)
                .then(|| (id, Some(column.clone()), NonConformance::NestedLabeledNull))
        })
    });

    let pair_violation = instance
        .foreign_keys()
        .into_iter()
        .filter(|(edge, pairs)| !pairs.is_empty() && !side.relations.contains(*edge))
        .filter_map(|(edge, pairs)| pairs.first().map(|(src, _)| (*src, (*edge).clone())))
        .min_by_key(|(src, _)| *src);
    let violation = match (row_violation, pair_violation) {
        (Some(row), Some((src, _))) if row.0 <= src => Some(row),
        (_, Some((src, edge))) => Some((
            src,
            None,
            NonConformance::RelationOutside {
                edge: Box::new(edge),
            },
        )),
        (row, None) => row,
    };
    if let Some((row, column, reason)) = violation {
        return Err(GraphKanError::Adjunction(AdjunctionError::NonConforming {
            row,
            column,
            reason,
        }));
    }
    Ok(())
}

type CellKey = (RowId, String);
type RelationKey = (Edge, RowId, RowId);

/// Input objects contributing to each relational object of a table result.
#[derive(Clone, Debug, Default)]
struct OriginPlan {
    rows: BTreeMap<RowId, Vec<RowId>>,
    cells: BTreeMap<CellKey, Vec<CellKey>>,
    relations: BTreeMap<RelationKey, Vec<RelationKey>>,
    relation_layouts: BTreeMap<RelationKey, EdgeLayout>,
}

fn derive_layout(input: &GraphLayout, output: &FInstance, origins: &OriginPlan) -> GraphLayout {
    let mut layout = GraphLayout {
        reserved_node_ids: input.reserved_node_ids.clone(),
        reserved_edge_ids: input.reserved_edge_ids.clone(),
        ..GraphLayout::default()
    };
    for row in output.row_ids() {
        if let Some([source]) = origins.rows.get(&row).map(Vec::as_slice)
            && let Some(node) = input.table_nodes.get(source)
        {
            layout.table_nodes.insert(row, node.clone());
        }
        let Some(_) = output.table_of(row) else {
            continue;
        };
        let Some(cells) = output.row(row) else {
            continue;
        };
        for (column, value) in cells {
            let key = (row, column.clone());
            let candidate = origins
                .cells
                .get(&key)
                .into_iter()
                .flatten()
                .find_map(|source| {
                    let cell = input.cells.get(source)?;
                    fold_layout(cell)
                        .is_ok_and(|folded| same_value(&folded, value))
                        .then(|| cell.clone())
                });
            if let Some(cell) = candidate {
                layout.cells.insert(key, cell);
            }
        }
    }
    layout.relations.extend(origins.relation_layouts.clone());
    layout
}

fn relation_is_injective_at(functor: &MigrationFunctor, source: &Edge) -> bool {
    let Some(image) = functor.relation_image(source) else {
        return false;
    };
    functor.relation_fibre(image).len() == 1
        && functor.table_fibre(&image.src).len() == 1
        && functor.table_fibre(&image.tgt).len() == 1
}

fn selected_relation_layout(
    input: &GraphLayout,
    contributors: &[RelationKey],
    retain_id: bool,
) -> EdgeLayout {
    let entries: Vec<EdgeLayout> = contributors
        .iter()
        .filter_map(|key| input.relations.get(key).copied())
        .collect();
    EdgeLayout {
        id: retain_id
            .then(|| entries.first().and_then(|entry| entry.id))
            .flatten(),
        position: entries.iter().filter_map(|entry| entry.position).min(),
        has_source: !entries.is_empty(),
    }
}

fn sigma_origins(
    input: &FInstance,
    input_layout: &GraphLayout,
    output: &FInstance,
    functor: &MigrationFunctor,
) -> OriginPlan {
    let mut plan = OriginPlan::default();
    for row in output.row_ids() {
        plan.rows.insert(row, vec![row]);
        let Some(source_table) = input.table_of(row) else {
            continue;
        };
        let Some(output_cells) = output.row(row) else {
            continue;
        };
        for column in output_cells.keys() {
            let sources = functor
                .column_map()
                .iter()
                .filter(|((table, _), image)| table == source_table && *image == column)
                .map(|((_, source), _)| (row, source.clone()))
                .collect();
            plan.cells.insert((row, column.clone()), sources);
        }
    }
    for (target, pairs) in output.foreign_keys() {
        for &(src, tgt) in pairs {
            let key = (target.clone(), src, tgt);
            let contributors: Vec<RelationKey> = functor
                .relation_fibre(target)
                .into_iter()
                .filter(|source| input.pairs(source).contains(&(src, tgt)))
                .map(|source| (source.clone(), src, tgt))
                .collect();
            let retain =
                contributors.len() == 1 && relation_is_injective_at(functor, &contributors[0].0);
            plan.relation_layouts.insert(
                key.clone(),
                selected_relation_layout(input_layout, &contributors, retain),
            );
            plan.relations.insert(key, contributors);
        }
    }
    plan
}

fn delta_origins(
    input: &FInstance,
    input_layout: &GraphLayout,
    output: &FInstance,
    table: &DeltaProvenance,
    functor: &MigrationFunctor,
) -> OriginPlan {
    let mut plan = OriginPlan::default();
    for (&copy, (source_table, original)) in &table.copies {
        plan.rows.insert(copy, vec![*original]);
        if let Some(cells) = output.row(copy) {
            for source_column in cells.keys() {
                if let Some(target_column) = functor.column_image(source_table, source_column) {
                    plan.cells.insert(
                        (copy, source_column.clone()),
                        vec![(*original, target_column.to_owned())],
                    );
                }
            }
        }
    }
    for (source, pairs) in output.foreign_keys() {
        let Some(target) = functor.relation_image(source) else {
            continue;
        };
        for &(src, tgt) in pairs {
            let (Some((_, original_src)), Some((_, original_tgt))) =
                (table.copies.get(&src), table.copies.get(&tgt))
            else {
                continue;
            };
            let key = (source.clone(), src, tgt);
            let contributors = if input
                .pairs(target)
                .contains(&(*original_src, *original_tgt))
            {
                vec![(target.clone(), *original_src, *original_tgt)]
            } else {
                Vec::new()
            };
            plan.relation_layouts.insert(
                key.clone(),
                selected_relation_layout(
                    input_layout,
                    &contributors,
                    relation_is_injective_at(functor, source),
                ),
            );
            plan.relations.insert(key, contributors);
        }
    }
    plan
}

fn tuple_component(
    provenance: &PiProvenance,
    functor: &MigrationFunctor,
    output_row: RowId,
    source_table: &Name,
) -> Option<RowId> {
    let (target_table, tuple) = provenance.tuples.get(&output_row)?;
    let position = functor
        .table_fibre(target_table)
        .iter()
        .position(|table| *table == source_table)?;
    tuple.get(position).copied()
}

fn pi_origins(
    input: &FInstance,
    input_layout: &GraphLayout,
    output: &FInstance,
    table: &PiProvenance,
    functor: &MigrationFunctor,
) -> OriginPlan {
    let mut plan = OriginPlan::default();
    for (&row, (target_table, tuple)) in &table.tuples {
        plan.rows.insert(row, tuple.clone());
        let fibre = functor.table_fibre(target_table);
        let Some(output_cells) = output.row(row) else {
            continue;
        };
        for target_column in output_cells.keys() {
            let mut contributors = Vec::new();
            for (source_table, source_row) in fibre.iter().zip(tuple) {
                contributors.extend(
                    functor
                        .column_map()
                        .iter()
                        .filter(|((table, _), image)| {
                            table == *source_table && *image == target_column
                        })
                        .map(|((_, source_column), _)| (*source_row, source_column.clone())),
                );
            }
            plan.cells
                .insert((row, target_column.clone()), contributors);
        }
    }
    for (target, pairs) in output.foreign_keys() {
        for &(src, tgt) in pairs {
            let key = (target.clone(), src, tgt);
            let mut contributors = Vec::new();
            for source in functor.relation_fibre(target) {
                let Some(source_row) = tuple_component(table, functor, src, &source.src) else {
                    continue;
                };
                let Some(target_row) = tuple_component(table, functor, tgt, &source.tgt) else {
                    continue;
                };
                if input.pairs(source).contains(&(source_row, target_row)) {
                    contributors.push((source.clone(), source_row, target_row));
                }
            }
            let retain =
                contributors.len() == 1 && relation_is_injective_at(functor, &contributors[0].0);
            plan.relation_layouts.insert(
                key.clone(),
                selected_relation_layout(input_layout, &contributors, retain),
            );
            plan.relations.insert(key, contributors);
        }
    }
    plan
}

fn contributor_maps(
    decoded: &DecodeResult,
    input: &GraphLayout,
    origins: &OriginPlan,
) -> (
    BTreeMap<u32, BTreeSet<u32>>,
    BTreeMap<EdgeId, BTreeSet<EdgeId>>,
) {
    let mut nodes: BTreeMap<u32, BTreeSet<u32>> = decoded
        .graph
        .nodes()
        .keys()
        .map(|id| (*id, BTreeSet::new()))
        .collect();
    let mut edges: BTreeMap<EdgeId, BTreeSet<EdgeId>> = decoded
        .graph
        .edges()
        .keys()
        .map(|id| (*id, BTreeSet::new()))
        .collect();

    for (&row, sources) in &origins.rows {
        if let Some(entry) = nodes.get_mut(&row.0) {
            entry.extend(
                sources
                    .iter()
                    .filter(|source| input.table_nodes.contains_key(source))
                    .map(|source| source.0),
            );
        }
    }
    for (key, trace) in &decoded.cells {
        let source_cells: Vec<&CellLayout> = origins
            .cells
            .get(key)
            .into_iter()
            .flatten()
            .filter_map(|source| input.cells.get(source))
            .collect();
        let source_nodes: BTreeSet<u32> = source_cells
            .iter()
            .flat_map(|cell| cell.nodes.keys().copied())
            .collect();
        let source_edges: BTreeSet<EdgeId> = source_cells
            .iter()
            .flat_map(|cell| {
                cell.attribute
                    .id
                    .into_iter()
                    .chain(cell.edges.keys().copied())
            })
            .collect();
        for node in &trace.nodes {
            nodes.entry(*node).or_default().extend(&source_nodes);
        }
        for edge in &trace.edges {
            edges.entry(*edge).or_default().extend(&source_edges);
        }
    }
    for (key, output_edge) in &decoded.relations {
        let contributors = origins
            .relations
            .get(key)
            .into_iter()
            .flatten()
            .filter_map(|source| input.relations.get(source).and_then(|layout| layout.id));
        edges.entry(*output_edge).or_default().extend(contributors);
    }
    (nodes, edges)
}

/// Graph provenance for `Sigma_F`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphSigmaProvenance {
    /// The table Sigma provenance.
    pub table: SigmaProvenance,
    /// Every output node with all input nodes contributing to it.
    pub nodes: BTreeMap<u32, BTreeSet<u32>>,
    /// Every output edge with all input edges contributing to it.
    pub edges: BTreeMap<EdgeId, BTreeSet<EdgeId>>,
}

/// Graph provenance for `Delta_F`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphDeltaProvenance {
    /// The table Delta provenance.
    pub table: DeltaProvenance,
    /// Every output node with all input nodes contributing to it.
    pub nodes: BTreeMap<u32, BTreeSet<u32>>,
    /// Every output edge with all input edges contributing to it.
    pub edges: BTreeMap<EdgeId, BTreeSet<EdgeId>>,
}

/// Graph provenance for `Pi_F`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphPiProvenance {
    /// The table Pi provenance.
    pub table: PiProvenance,
    /// Every output node with all input nodes contributing to it.
    pub nodes: BTreeMap<u32, BTreeSet<u32>>,
    /// Every output edge with all input edges contributing to it.
    pub edges: BTreeMap<EdgeId, BTreeSet<EdgeId>>,
}

fn checked_functor(
    src_schema: &Schema,
    src_protocol: &Protocol,
    tgt_schema: &Schema,
    tgt_protocol: &Protocol,
    migration: &CompiledMigration,
) -> Result<MigrationFunctor, GraphKanError> {
    let source = RelSchema::new(src_schema, src_protocol)?;
    let target = RelSchema::new(tgt_schema, tgt_protocol)?;
    MigrationFunctor::between(source, target, migration).map_err(GraphKanError::Functor)
}

/// Graph `Sigma_F` for a migration between classified schemas.
///
/// # Errors
///
/// Returns the first classification, functor, encoding, table, or decoding error.
pub fn g_sigma(
    graph: &GInstance,
    src_schema: &Schema,
    src_protocol: &Protocol,
    tgt_schema: &Schema,
    tgt_protocol: &Protocol,
    migration: &CompiledMigration,
) -> Result<GInstance, GraphKanError> {
    g_sigma_with_provenance(
        graph,
        src_schema,
        src_protocol,
        tgt_schema,
        tgt_protocol,
        migration,
    )
    .map(|(result, _)| result)
}

/// Graph `Sigma_F` with graph and table provenance.
///
/// # Errors
///
/// Returns the first classification, functor, encoding, table, or decoding error.
pub fn g_sigma_with_provenance(
    graph: &GInstance,
    src_schema: &Schema,
    src_protocol: &Protocol,
    tgt_schema: &Schema,
    tgt_protocol: &Protocol,
    migration: &CompiledMigration,
) -> Result<(GInstance, GraphSigmaProvenance), GraphKanError> {
    let functor = checked_functor(
        src_schema,
        src_protocol,
        tgt_schema,
        tgt_protocol,
        migration,
    )?;
    g_sigma_along_with_provenance(graph, &functor)
}

/// Graph `Sigma_F` along a checked migration functor.
///
/// # Errors
///
/// Returns the first encoding, table Sigma, or decoding error.
pub fn g_sigma_along(
    graph: &GInstance,
    functor: &MigrationFunctor,
) -> Result<GInstance, GraphKanError> {
    g_sigma_along_with_provenance(graph, functor).map(|(result, _)| result)
}

/// Graph `Sigma_F` along a checked functor, with provenance.
///
/// # Errors
///
/// Returns the first encoding, table Sigma, or decoding error.
pub fn g_sigma_along_with_provenance(
    graph: &GInstance,
    functor: &MigrationFunctor,
) -> Result<(GInstance, GraphSigmaProvenance), GraphKanError> {
    let (encoded, input_layout) = encode_graph(graph, EncodingSide::Whole(functor.source()))?;
    let (table_result, table) =
        f_sigma_with_provenance(&encoded, functor).map_err(GraphKanError::Adjunction)?;
    let origins = sigma_origins(&encoded, &input_layout, &table_result, functor);
    let output_layout = derive_layout(&input_layout, &table_result, &origins);
    let decoded = decode_with_trace(
        &table_result,
        EncodingSide::Whole(functor.target()),
        &output_layout,
    )?;
    let (nodes, edges) = contributor_maps(&decoded, &input_layout, &origins);
    Ok((
        decoded.graph,
        GraphSigmaProvenance {
            table,
            nodes,
            edges,
        },
    ))
}

/// Graph `Sigma_F` of an instance already on the kept part `S'`.
///
/// # Errors
///
/// Returns the first encoding, restricted table Sigma, or decoding error.
pub fn g_sigma_restricted(
    graph: &GInstance,
    functor: &MigrationFunctor,
) -> Result<GInstance, GraphKanError> {
    g_sigma_restricted_with_provenance(graph, functor).map(|(result, _)| result)
}

/// Graph `Sigma_F` of an instance already on `S'`, with provenance.
///
/// # Errors
///
/// Returns the first encoding, restricted table Sigma, or decoding error.
pub fn g_sigma_restricted_with_provenance(
    graph: &GInstance,
    functor: &MigrationFunctor,
) -> Result<(GInstance, GraphSigmaProvenance), GraphKanError> {
    let (encoded, input_layout) = encode_graph(graph, EncodingSide::Domain(functor))?;
    let (table_result, table) =
        f_sigma_restricted(&encoded, functor).map_err(GraphKanError::Adjunction)?;
    let origins = sigma_origins(&encoded, &input_layout, &table_result, functor);
    let output_layout = derive_layout(&input_layout, &table_result, &origins);
    let decoded = decode_with_trace(
        &table_result,
        EncodingSide::Whole(functor.target()),
        &output_layout,
    )?;
    let (nodes, edges) = contributor_maps(&decoded, &input_layout, &origins);
    Ok((
        decoded.graph,
        GraphSigmaProvenance {
            table,
            nodes,
            edges,
        },
    ))
}

/// Graph `Delta_F`, total in the migration map on the encodable domain.
///
/// # Errors
///
/// Returns the first classification, functor, encoding, table, or decoding error.
pub fn g_delta(
    graph: &GInstance,
    src_schema: &Schema,
    src_protocol: &Protocol,
    tgt_schema: &Schema,
    tgt_protocol: &Protocol,
    migration: &CompiledMigration,
) -> Result<GInstance, GraphKanError> {
    g_delta_with_provenance(
        graph,
        src_schema,
        src_protocol,
        tgt_schema,
        tgt_protocol,
        migration,
    )
    .map(|(result, _)| result)
}

/// Graph `Delta_F` with graph and table provenance.
///
/// # Errors
///
/// Returns the first classification, functor, encoding, table, or decoding error.
pub fn g_delta_with_provenance(
    graph: &GInstance,
    src_schema: &Schema,
    src_protocol: &Protocol,
    tgt_schema: &Schema,
    tgt_protocol: &Protocol,
    migration: &CompiledMigration,
) -> Result<(GInstance, GraphDeltaProvenance), GraphKanError> {
    let functor = checked_functor(
        src_schema,
        src_protocol,
        tgt_schema,
        tgt_protocol,
        migration,
    )?;
    g_delta_along_with_provenance(graph, &functor)
}

/// Graph `Delta_F` along a checked migration functor.
///
/// # Errors
///
/// Returns the first encoding, table Delta, or decoding error.
pub fn g_delta_along(
    graph: &GInstance,
    functor: &MigrationFunctor,
) -> Result<GInstance, GraphKanError> {
    g_delta_along_with_provenance(graph, functor).map(|(result, _)| result)
}

/// Graph `Delta_F` along a checked functor, with provenance.
///
/// # Errors
///
/// Returns the first encoding, table Delta, or decoding error.
pub fn g_delta_along_with_provenance(
    graph: &GInstance,
    functor: &MigrationFunctor,
) -> Result<(GInstance, GraphDeltaProvenance), GraphKanError> {
    let (encoded, input_layout) = encode_graph(graph, EncodingSide::Whole(functor.target()))?;
    let (table_result, table) =
        f_delta_with_provenance(&encoded, functor).map_err(GraphKanError::Adjunction)?;
    let origins = delta_origins(&encoded, &input_layout, &table_result, &table, functor);
    let output_layout = derive_layout(&input_layout, &table_result, &origins);
    let decoded = decode_with_trace(&table_result, EncodingSide::Domain(functor), &output_layout)?;
    let (nodes, edges) = contributor_maps(&decoded, &input_layout, &origins);
    Ok((
        decoded.graph,
        GraphDeltaProvenance {
            table,
            nodes,
            edges,
        },
    ))
}

/// Graph `Pi_F` for a migration between classified schemas.
///
/// # Errors
///
/// Returns the first classification, functor, encoding, table, or decoding error.
pub fn g_pi(
    graph: &GInstance,
    src_schema: &Schema,
    src_protocol: &Protocol,
    tgt_schema: &Schema,
    tgt_protocol: &Protocol,
    migration: &CompiledMigration,
    max_product_size: usize,
) -> Result<GInstance, GraphKanError> {
    g_pi_with_provenance(
        graph,
        src_schema,
        src_protocol,
        tgt_schema,
        tgt_protocol,
        migration,
        max_product_size,
    )
    .map(|(result, _)| result)
}

/// Graph `Pi_F` with graph and table provenance.
///
/// # Errors
///
/// Returns the first classification, functor, encoding, table, or decoding error.
pub fn g_pi_with_provenance(
    graph: &GInstance,
    src_schema: &Schema,
    src_protocol: &Protocol,
    tgt_schema: &Schema,
    tgt_protocol: &Protocol,
    migration: &CompiledMigration,
    max_product_size: usize,
) -> Result<(GInstance, GraphPiProvenance), GraphKanError> {
    let functor = checked_functor(
        src_schema,
        src_protocol,
        tgt_schema,
        tgt_protocol,
        migration,
    )?;
    g_pi_along_with_provenance(graph, &functor, max_product_size)
}

/// Graph `Pi_F` along a checked migration functor.
///
/// # Errors
///
/// Returns the first encoding, table Pi, or decoding error.
pub fn g_pi_along(
    graph: &GInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<GInstance, GraphKanError> {
    g_pi_along_with_provenance(graph, functor, max_product_size).map(|(result, _)| result)
}

/// Graph `Pi_F` along a checked functor, with provenance.
///
/// # Errors
///
/// Returns the first encoding, table Pi, or decoding error.
pub fn g_pi_along_with_provenance(
    graph: &GInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<(GInstance, GraphPiProvenance), GraphKanError> {
    let (encoded, input_layout) = encode_graph(graph, EncodingSide::Whole(functor.source()))?;
    let (table_result, table) =
        functor_pi_along_with_provenance(&encoded, functor, max_product_size)
            .map_err(GraphKanError::Adjunction)?;
    let origins = pi_origins(&encoded, &input_layout, &table_result, &table, functor);
    let output_layout = derive_layout(&input_layout, &table_result, &origins);
    let decoded = decode_with_trace(
        &table_result,
        EncodingSide::Whole(functor.target()),
        &output_layout,
    )?;
    let (nodes, edges) = contributor_maps(&decoded, &input_layout, &origins);
    Ok((
        decoded.graph,
        GraphPiProvenance {
            table,
            nodes,
            edges,
        },
    ))
}

/// Graph `Pi_F` of an instance already on the kept part `S'`.
///
/// # Errors
///
/// Returns the first encoding, restricted table Pi, or decoding error.
pub fn g_pi_restricted(
    graph: &GInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<GInstance, GraphKanError> {
    g_pi_restricted_with_provenance(graph, functor, max_product_size).map(|(result, _)| result)
}

/// Graph `Pi_F` of an instance already on `S'`, with provenance.
///
/// # Errors
///
/// Returns the first encoding, restricted table Pi, or decoding error.
pub fn g_pi_restricted_with_provenance(
    graph: &GInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<(GInstance, GraphPiProvenance), GraphKanError> {
    let (encoded, input_layout) = encode_graph(graph, EncodingSide::Domain(functor))?;
    let (table_result, table) = functor_pi_restricted(&encoded, functor, max_product_size)
        .map_err(GraphKanError::Adjunction)?;
    let origins = pi_origins(&encoded, &input_layout, &table_result, &table, functor);
    let output_layout = derive_layout(&input_layout, &table_result, &origins);
    let decoded = decode_with_trace(
        &table_result,
        EncodingSide::Whole(functor.target()),
        &output_layout,
    )?;
    let (nodes, edges) = contributor_maps(&decoded, &input_layout, &origins);
    Ok((
        decoded.graph,
        GraphPiProvenance {
            table,
            nodes,
            edges,
        },
    ))
}

/// A homomorphism between encodable graphs over one encoding side.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphKanHom {
    /// Map on table nodes. Cell subtrees and relation edges have their unique
    /// induced images.
    pub node_map: BTreeMap<u32, u32>,
    /// Image of each labelled null of the domain.
    pub null_map: BTreeMap<u64, Value>,
}

impl GraphKanHom {
    /// Identity on an encodable graph.
    ///
    /// # Errors
    ///
    /// Returns the graph's first encoding error.
    pub fn identity(graph: &GInstance, side: &EncodingSide<'_>) -> Result<Self, GraphKanError> {
        let (encoded, _) = encode_graph(graph, *side)?;
        Ok(Self::from_table(FInstanceHom::identity(&encoded)))
    }

    /// Check this homomorphism in the exact order of the table hom check.
    ///
    /// # Errors
    ///
    /// Returns the domain's first encoding error, then the codomain's, then
    /// the first table homomorphism error.
    pub fn check(
        &self,
        domain: &GInstance,
        codomain: &GInstance,
        side: EncodingSide<'_>,
    ) -> Result<(), GraphKanError> {
        let (domain, _) = encode_graph(domain, side)?;
        let (codomain, _) = encode_graph(codomain, side)?;
        self.as_table()
            .check(&domain, &codomain)
            .map_err(GraphKanError::Hom)
    }

    /// Compose `self: A -> B` with `other: B -> C`.
    ///
    /// # Errors
    ///
    /// Returns the table homomorphism composition error.
    pub fn compose(&self, other: &Self) -> Result<Self, GraphKanError> {
        self.as_table()
            .compose(&other.as_table())
            .map(Self::from_table)
            .map_err(GraphKanError::Hom)
    }

    /// Equality after normalising identity null-map entries.
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        self.as_table().same_as(&other.as_table())
    }

    fn as_table(&self) -> FInstanceHom {
        FInstanceHom::new(
            self.node_map
                .iter()
                .map(|(source, target)| (RowId(*source), RowId(*target)))
                .collect(),
        )
        .with_null_map(self.null_map.clone())
    }

    fn from_table(hom: FInstanceHom) -> Self {
        Self {
            node_map: hom
                .row_map
                .into_iter()
                .map(|(source, target)| (source.0, target.0))
                .collect(),
            null_map: hom.null_map,
        }
    }
}

/// Unit of graph `Sigma_F -| Delta_F` on an instance of `S'`.
///
/// # Errors
///
/// Returns the first encoding or table-adjunction error.
pub fn g_unit(graph: &GInstance, functor: &MigrationFunctor) -> Result<GraphKanHom, GraphKanError> {
    let (encoded, _) = encode_graph(graph, EncodingSide::Domain(functor))?;
    f_unit(&encoded, functor)
        .map(GraphKanHom::from_table)
        .map_err(GraphKanError::Adjunction)
}

/// Counit of graph `Sigma_F -| Delta_F` on an instance of `T`.
///
/// # Errors
///
/// Returns the first encoding or table-adjunction error.
pub fn g_counit(
    graph: &GInstance,
    functor: &MigrationFunctor,
) -> Result<GraphKanHom, GraphKanError> {
    let (encoded, _) = encode_graph(graph, EncodingSide::Whole(functor.target()))?;
    f_counit(&encoded, functor)
        .map(GraphKanHom::from_table)
        .map_err(GraphKanError::Adjunction)
}

/// Transpose `Hom(g_Sigma I, J) -> Hom(I, g_Delta J)`.
///
/// # Errors
///
/// Returns the first source or target encoding error, then the table transpose error.
pub fn g_transpose_left(
    hom: &GraphKanHom,
    source: &GInstance,
    target: &GInstance,
    functor: &MigrationFunctor,
) -> Result<GraphKanHom, GraphKanError> {
    let (source, _) = encode_graph(source, EncodingSide::Domain(functor))?;
    let (target, _) = encode_graph(target, EncodingSide::Whole(functor.target()))?;
    f_transpose_left(&hom.as_table(), &source, &target, functor)
        .map(GraphKanHom::from_table)
        .map_err(GraphKanError::Adjunction)
}

/// Inverse transpose `Hom(I, g_Delta J) -> Hom(g_Sigma I, J)`.
///
/// # Errors
///
/// Returns the first source or target encoding error, then the table transpose error.
pub fn g_transpose_right(
    hom: &GraphKanHom,
    source: &GInstance,
    target: &GInstance,
    functor: &MigrationFunctor,
) -> Result<GraphKanHom, GraphKanError> {
    let (source, _) = encode_graph(source, EncodingSide::Domain(functor))?;
    let (target, _) = encode_graph(target, EncodingSide::Whole(functor.target()))?;
    f_transpose_right(&hom.as_table(), &source, &target, functor)
        .map(GraphKanHom::from_table)
        .map_err(GraphKanError::Adjunction)
}

/// Unit of graph `Delta_F -| Pi_F` on an instance of `T`.
///
/// # Errors
///
/// Returns the first encoding or table-adjunction error.
pub fn g_pi_unit(
    graph: &GInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<GraphKanHom, GraphKanError> {
    let (encoded, _) = encode_graph(graph, EncodingSide::Whole(functor.target()))?;
    f_pi_unit(&encoded, functor, max_product_size)
        .map(GraphKanHom::from_table)
        .map_err(GraphKanError::Adjunction)
}

/// Counit of graph `Delta_F -| Pi_F` on an instance of `S'`.
///
/// # Errors
///
/// Returns the first encoding or table-adjunction error.
pub fn g_pi_counit(
    graph: &GInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<GraphKanHom, GraphKanError> {
    let (encoded, _) = encode_graph(graph, EncodingSide::Domain(functor))?;
    f_pi_counit(&encoded, functor, max_product_size)
        .map(GraphKanHom::from_table)
        .map_err(GraphKanError::Adjunction)
}

/// Transpose `Hom(g_Delta J, I) -> Hom(J, g_Pi I)`.
///
/// # Errors
///
/// Returns the first target or source encoding error, then the table transpose error.
pub fn g_pi_transpose_left(
    hom: &GraphKanHom,
    target: &GInstance,
    source: &GInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<GraphKanHom, GraphKanError> {
    let (target, _) = encode_graph(target, EncodingSide::Whole(functor.target()))?;
    let (source, _) = encode_graph(source, EncodingSide::Domain(functor))?;
    f_pi_transpose_left(&hom.as_table(), &target, &source, functor, max_product_size)
        .map(GraphKanHom::from_table)
        .map_err(GraphKanError::Adjunction)
}

/// Inverse transpose `Hom(J, g_Pi I) -> Hom(g_Delta J, I)`.
///
/// # Errors
///
/// Returns the first target or source encoding error, then the table transpose error.
pub fn g_pi_transpose_right(
    hom: &GraphKanHom,
    target: &GInstance,
    source: &GInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<GraphKanHom, GraphKanError> {
    let (target, _) = encode_graph(target, EncodingSide::Whole(functor.target()))?;
    let (source, _) = encode_graph(source, EncodingSide::Domain(functor))?;
    f_pi_transpose_right(&hom.as_table(), &target, &source, functor, max_product_size)
        .map(GraphKanHom::from_table)
        .map_err(GraphKanError::Adjunction)
}
