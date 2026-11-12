#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! Exhaustive release-carrier differentials for the edit algebra.
//!
//! The oracle is the dependency-free implementation in `panproto-reference`.
//! This file contains adapters only: every operation under comparison is
//! called independently on the same labelled input.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::edit_error::EditError;
use panproto_inst::fan_violation::FanViolation;
use panproto_inst::graph_edit::{
    AttrChange, AttrKey, AttrVal, ComposeError, EditConflict, ElemRef, GraphDelta, GraphDeltaParts,
    canonical_graph_delta_bytes,
};
use panproto_inst::table_delta::{CellChange, TableChange, TableDelta, TableDeltaParts};
use panproto_inst::{
    EdgeId, Fan, FieldPresence, GEdge, GInstance, Node, NodeShape, TableEmbedding, TreeEdit,
    TreeEmbedding, Value, WInstance, canonical_tree_edit_bytes,
};
use panproto_inst::{FInstance, RowId, canonical_table_delta_bytes};
use panproto_reference::edit as reference;
use panproto_reference::enumerate::edits as carrier;
use panproto_reference::model as reference_model;
use panproto_reference::value::{Presence as ReferencePresence, Value as ReferenceValue};
use panproto_schema::{CanonicalWriter, Edge};

fn edge_to_production(edge: &reference_model::SEdge) -> Edge {
    Edge {
        src: edge.src.as_str().into(),
        tgt: edge.tgt.as_str().into(),
        kind: edge.kind.as_str().into(),
        name: edge.name.as_deref().map(Into::into),
    }
}

fn edge_to_reference(edge: &Edge) -> reference_model::SEdge {
    reference_model::SEdge::new(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
}

fn value_to_production(value: &ReferenceValue) -> Value {
    match value {
        ReferenceValue::Bool(value) => Value::Bool(*value),
        ReferenceValue::Int(value) => Value::Int(*value),
        other => panic!("value outside the edits release carrier: {other:?}"),
    }
}

fn value_to_reference(value: &Value) -> ReferenceValue {
    match value {
        Value::Bool(value) => ReferenceValue::Bool(*value),
        Value::Int(value) => ReferenceValue::Int(*value),
        other => panic!("production returned a value outside the edits release carrier: {other:?}"),
    }
}

fn presence_to_production(value: &ReferencePresence) -> FieldPresence {
    match value {
        ReferencePresence::Present(value) => FieldPresence::Present(value_to_production(value)),
        ReferencePresence::Null => FieldPresence::Null,
        ReferencePresence::Absent => FieldPresence::Absent,
    }
}

fn presence_to_reference(value: &FieldPresence) -> ReferencePresence {
    match value {
        FieldPresence::Present(value) => ReferencePresence::Present(value_to_reference(value)),
        FieldPresence::Null => ReferencePresence::Null,
        FieldPresence::Absent => ReferencePresence::Absent,
    }
}

fn shape_to_production(shape: &reference_model::NodeShape) -> NodeShape {
    match shape {
        reference_model::NodeShape::Plain => NodeShape::Plain,
        reference_model::NodeShape::List => NodeShape::List,
        reference_model::NodeShape::XmlElement(tag) => NodeShape::XmlElement {
            tag: tag.as_str().into(),
        },
        reference_model::NodeShape::XmlTextSegment => NodeShape::XmlTextSegment,
        reference_model::NodeShape::HyperEdge => NodeShape::HyperEdge,
    }
}

fn shape_to_reference(shape: &NodeShape) -> reference_model::NodeShape {
    match shape {
        NodeShape::Plain => reference_model::NodeShape::Plain,
        NodeShape::List => reference_model::NodeShape::List,
        NodeShape::XmlElement { tag } => reference_model::NodeShape::XmlElement(tag.to_string()),
        NodeShape::XmlTextSegment => reference_model::NodeShape::XmlTextSegment,
        NodeShape::HyperEdge => reference_model::NodeShape::HyperEdge,
    }
}

fn node_to_production(node: &reference_model::Node) -> Node {
    Node {
        id: node.id,
        anchor: node.anchor.as_str().into(),
        value: node.value.as_ref().map(presence_to_production),
        discriminator: node.discriminator.as_deref().map(Into::into),
        extra_fields: node
            .fields
            .iter()
            .map(|(name, value)| (name.clone(), value_to_production(value)))
            .collect(),
        position: node.position,
        shape: shape_to_production(&node.shape),
        annotations: node
            .annotations
            .iter()
            .map(|(name, value)| (name.clone(), value_to_production(value)))
            .collect(),
    }
}

fn node_to_reference(node: &Node) -> reference_model::Node {
    reference_model::Node {
        id: node.id,
        anchor: node.anchor.to_string(),
        value: node.value.as_ref().map(presence_to_reference),
        discriminator: node.discriminator.as_deref().map(str::to_owned),
        fields: node
            .extra_fields
            .iter()
            .map(|(name, value)| (name.clone(), value_to_reference(value)))
            .collect(),
        position: node.position,
        shape: shape_to_reference(&node.shape),
        annotations: node
            .annotations
            .iter()
            .map(|(name, value)| (name.clone(), value_to_reference(value)))
            .collect(),
    }
}

fn graph_edge_to_production(edge: &reference_model::GEdge) -> GEdge {
    GEdge {
        src: edge.src,
        tgt: edge.tgt,
        edge: edge_to_production(&edge.label),
        position: edge.position,
    }
}

fn graph_edge_to_reference(edge: &GEdge) -> reference_model::GEdge {
    reference_model::GEdge {
        src: edge.src,
        tgt: edge.tgt,
        label: edge_to_reference(&edge.edge),
        position: edge.position,
    }
}

fn graph_to_production(graph: &reference_model::Graph) -> GInstance {
    let mut out = GInstance::new();
    for node in graph.nodes.values() {
        out = out.with_node(node_to_production(node));
    }
    for (id, edge) in &graph.edges {
        out.insert_edge(EdgeId(*id), graph_edge_to_production(edge))
            .expect("reference carrier graph is well formed");
    }
    out
}

fn graph_to_reference(graph: &GInstance) -> reference_model::Graph {
    reference_model::Graph {
        nodes: graph
            .nodes()
            .iter()
            .map(|(id, node)| (*id, node_to_reference(node)))
            .collect(),
        edges: graph
            .edges_sorted()
            .map(|(id, edge)| (id.0, graph_edge_to_reference(edge)))
            .collect(),
    }
}

fn meta_to_production(meta: &reference::NodeMeta) -> panproto_inst::graph_edit::NodeMeta {
    panproto_inst::graph_edit::NodeMeta {
        discriminator: meta.discriminator.as_deref().map(Into::into),
        position: meta.position,
        shape: shape_to_production(&meta.shape),
        annotations: meta
            .annotations
            .iter()
            .map(|(name, value)| (name.clone(), value_to_production(value)))
            .collect(),
    }
}

fn meta_to_reference(meta: &panproto_inst::graph_edit::NodeMeta) -> reference::NodeMeta {
    reference::NodeMeta {
        discriminator: meta.discriminator.as_deref().map(str::to_owned),
        position: meta.position,
        shape: shape_to_reference(&meta.shape),
        annotations: meta
            .annotations
            .iter()
            .map(|(name, value)| (name.clone(), value_to_reference(value)))
            .collect(),
    }
}

fn attr_key_to_production(key: &reference::AttrKey) -> AttrKey {
    match key {
        reference::AttrKey::Value(node) => AttrKey::Value { node: *node },
        reference::AttrKey::Field(node, field) => AttrKey::Field {
            node: *node,
            field: field.clone(),
        },
        reference::AttrKey::Meta(node) => AttrKey::Meta { node: *node },
        reference::AttrKey::Anchor(node) => AttrKey::Anchor { node: *node },
        reference::AttrKey::EdgeLabel(edge) => AttrKey::EdgeLabel {
            edge: EdgeId(*edge),
        },
        reference::AttrKey::EdgePosition(edge) => AttrKey::EdgePosition {
            edge: EdgeId(*edge),
        },
    }
}

fn attr_key_to_reference(key: &AttrKey) -> reference::AttrKey {
    match key {
        AttrKey::Value { node } => reference::AttrKey::Value(*node),
        AttrKey::Field { node, field } => reference::AttrKey::Field(*node, field.clone()),
        AttrKey::Meta { node } => reference::AttrKey::Meta(*node),
        AttrKey::Anchor { node } => reference::AttrKey::Anchor(*node),
        AttrKey::EdgeLabel { edge } => reference::AttrKey::EdgeLabel(edge.0),
        AttrKey::EdgePosition { edge } => reference::AttrKey::EdgePosition(edge.0),
        AttrKey::Cell { .. } | AttrKey::Table { .. } => {
            panic!("table key returned by graph delta: {key:?}")
        }
    }
}

fn attr_val_to_production(value: &reference::AttrVal) -> AttrVal {
    match value {
        reference::AttrVal::Presence(value) => {
            AttrVal::Presence(value.as_ref().map(presence_to_production))
        }
        reference::AttrVal::Field(value) => AttrVal::Field(value.as_ref().map(value_to_production)),
        reference::AttrVal::Meta(value) => AttrVal::Meta(meta_to_production(value)),
        reference::AttrVal::Anchor(value) => AttrVal::Anchor(value.as_str().into()),
        reference::AttrVal::Label(value) => AttrVal::Label(edge_to_production(value)),
        reference::AttrVal::Position(value) => AttrVal::Position(*value),
    }
}

fn attr_val_to_reference(value: &AttrVal) -> reference::AttrVal {
    match value {
        AttrVal::Presence(value) => {
            reference::AttrVal::Presence(value.as_ref().map(presence_to_reference))
        }
        AttrVal::Field(value) => reference::AttrVal::Field(value.as_ref().map(value_to_reference)),
        AttrVal::Meta(value) => reference::AttrVal::Meta(meta_to_reference(value)),
        AttrVal::Anchor(value) => reference::AttrVal::Anchor(value.to_string()),
        AttrVal::Label(value) => reference::AttrVal::Label(edge_to_reference(value)),
        AttrVal::Position(value) => reference::AttrVal::Position(*value),
    }
}

fn delta_to_production(delta: &reference::GraphDelta) -> GraphDelta {
    let mut parts = GraphDeltaParts::default();
    for (element, payload) in &delta.del {
        match (element, payload) {
            (reference::GElem::Node(id), reference::GPayload::Node(node)) => {
                parts.del_nodes.insert(*id, node_to_production(node));
            }
            (reference::GElem::Edge(id), reference::GPayload::Edge(edge)) => {
                parts
                    .del_edges
                    .insert(EdgeId(*id), graph_edge_to_production(edge));
            }
            _ => panic!("cross-sort reference deletion: {element:?} {payload:?}"),
        }
    }
    for (element, payload) in &delta.add {
        match (element, payload) {
            (reference::GElem::Node(id), reference::GPayload::Node(node)) => {
                parts.add_nodes.insert(*id, node_to_production(node));
            }
            (reference::GElem::Edge(id), reference::GPayload::Edge(edge)) => {
                parts
                    .add_edges
                    .insert(EdgeId(*id), graph_edge_to_production(edge));
            }
            _ => panic!("cross-sort reference addition: {element:?} {payload:?}"),
        }
    }
    parts.attrs = delta
        .attrs
        .iter()
        .map(|(key, (old, new))| {
            (
                attr_key_to_production(key),
                AttrChange {
                    old: attr_val_to_production(old),
                    new: attr_val_to_production(new),
                },
            )
        })
        .collect();
    GraphDelta::normalize(parts).expect("reference carrier delta is normal")
}

fn delta_to_reference(delta: &GraphDelta) -> reference::GraphDelta {
    let mut out = reference::GraphDelta::identity();
    for (id, node) in delta.del_nodes() {
        out.del.insert(
            reference::GElem::Node(*id),
            reference::GPayload::Node(node_to_reference(node)),
        );
    }
    for (id, edge) in delta.del_edges() {
        out.del.insert(
            reference::GElem::Edge(id.0),
            reference::GPayload::Edge(graph_edge_to_reference(edge)),
        );
    }
    for (id, node) in delta.add_nodes() {
        out.add.insert(
            reference::GElem::Node(*id),
            reference::GPayload::Node(node_to_reference(node)),
        );
    }
    for (id, edge) in delta.add_edges() {
        out.add.insert(
            reference::GElem::Edge(id.0),
            reference::GPayload::Edge(graph_edge_to_reference(edge)),
        );
    }
    out.attrs = delta
        .attrs()
        .iter()
        .map(|(key, change)| {
            (
                attr_key_to_reference(key),
                (
                    attr_val_to_reference(&change.old),
                    attr_val_to_reference(&change.new),
                ),
            )
        })
        .collect();
    out
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ElementObs {
    Node(u32),
    Edge(u32),
    Row(u32),
    Pair(String, u32, u32),
    Attr(String),
}

const fn reference_element(element: &reference::GElem) -> ElementObs {
    match element {
        reference::GElem::Node(id) => ElementObs::Node(*id),
        reference::GElem::Edge(id) => ElementObs::Edge(*id),
    }
}

fn reference_elem_ref(element: &reference::ElemRef) -> ElementObs {
    match element {
        reference::ElemRef::Elem(element) => reference_element(element),
        reference::ElemRef::Attr(key) => ElementObs::Attr(format!("{key:?}")),
    }
}

fn production_elem_ref(element: &ElemRef) -> ElementObs {
    match element {
        ElemRef::Node(id) => ElementObs::Node(*id),
        ElemRef::Edge(id) => ElementObs::Edge(id.0),
        ElemRef::Attr(key) => ElementObs::Attr(format!("{:?}", attr_key_to_reference(key))),
        ElemRef::Row(_) | ElemRef::Pair { .. } => {
            panic!("table element returned by graph operation: {element:?}")
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ConflictObs {
    DeleteDelete(ElementObs),
    DeleteUse(ElementObs, ElementObs),
    UpdateUpdate(String),
    AddAdd(ElementObs),
    AnchorUse(u32, ElementObs),
    FanUse(ElementObs, ElementObs),
}

fn reference_conflict(conflict: &reference::EditConflict) -> ConflictObs {
    match conflict {
        reference::EditConflict::DeleteDelete { elem } => {
            ConflictObs::DeleteDelete(reference_element(elem))
        }
        reference::EditConflict::DeleteUse { deleted, user } => {
            ConflictObs::DeleteUse(reference_element(deleted), reference_elem_ref(user))
        }
        reference::EditConflict::UpdateUpdate { key } => {
            ConflictObs::UpdateUpdate(format!("{key:?}"))
        }
        reference::EditConflict::AddAdd { elem } => ConflictObs::AddAdd(reference_element(elem)),
        reference::EditConflict::AnchorUse { node, user } => {
            ConflictObs::AnchorUse(*node, reference_elem_ref(user))
        }
        reference::EditConflict::FanUse { first, second } => {
            ConflictObs::FanUse(reference_elem_ref(first), reference_elem_ref(second))
        }
    }
}

fn production_conflict(conflict: &EditConflict) -> ConflictObs {
    match conflict {
        EditConflict::DeleteDelete { elem } => ConflictObs::DeleteDelete(production_elem_ref(elem)),
        EditConflict::DeleteUse { deleted, user } => {
            ConflictObs::DeleteUse(production_elem_ref(deleted), production_elem_ref(user))
        }
        EditConflict::UpdateUpdate { key } => {
            ConflictObs::UpdateUpdate(format!("{:?}", attr_key_to_reference(key)))
        }
        EditConflict::AddAdd { elem } => ConflictObs::AddAdd(production_elem_ref(elem)),
        EditConflict::AnchorUse { node, user } => {
            ConflictObs::AnchorUse(*node, production_elem_ref(user))
        }
        EditConflict::FanUse { first, second } => {
            ConflictObs::FanUse(production_elem_ref(first), production_elem_ref(second))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ApplyObs {
    NodeNotFound(u32),
    StaleNode(u32),
    EdgeNotFound(u32),
    StaleEdge(u32),
    StaleValue(u32),
    StaleField(u32, String),
    StaleMeta(u32),
    StaleAnchor(u32),
    StaleLabel(u32),
    StalePosition(u32),
    DanglingEdges(u32, Vec<u32>),
    DuplicateNodeId(u32),
    DuplicateEdge(u32),
    MissingEndpoint(u32, u32),
    MalformedFan(u32, String),
}

fn fan_reference(reason: &panproto_reference::fan::FanViolation) -> String {
    format!("{reason:?}")
}

fn fan_production(reason: &FanViolation) -> String {
    match reason {
        FanViolation::MissingParent => "MissingParent".to_owned(),
        FanViolation::MultipleParents { edges } => format!(
            "MultipleParents {{ edges: {:?} }}",
            edges.iter().map(|edge| edge.0).collect::<Vec<_>>()
        ),
        FanViolation::DuplicateLabel { label, edges } => format!(
            "DuplicateLabel {{ label: {label:?}, edges: {:?} }}",
            edges.iter().map(|edge| edge.0).collect::<Vec<_>>()
        ),
        FanViolation::UnknownLabel { label } => format!("UnknownLabel {{ label: {label:?} }}"),
        FanViolation::ReservedKindOutsideFan { edge } => {
            format!("ReservedKindOutsideFan {{ edge: {} }}", edge.0)
        }
        FanViolation::ForeignIncidentEdge { edge } => {
            format!("ForeignIncidentEdge {{ edge: {} }}", edge.0)
        }
        FanViolation::UnknownHyperEdge { hyper_edge } => {
            format!("UnknownHyperEdge {{ hyper_edge: {hyper_edge:?} }}")
        }
        FanViolation::MissingChild { label } => format!("MissingChild {{ label: {label:?} }}"),
    }
}

fn reference_apply_error(error: &reference::ApplyError) -> ApplyObs {
    match error {
        reference::ApplyError::NodeNotFound(id) => ApplyObs::NodeNotFound(*id),
        reference::ApplyError::StaleNode { node } => ApplyObs::StaleNode(*node),
        reference::ApplyError::EdgeNotFound(id) => ApplyObs::EdgeNotFound(*id),
        reference::ApplyError::StaleEdge { edge } => ApplyObs::StaleEdge(*edge),
        reference::ApplyError::StaleValue { node } => ApplyObs::StaleValue(*node),
        reference::ApplyError::StaleField { node, field } => {
            ApplyObs::StaleField(*node, field.clone())
        }
        reference::ApplyError::StaleMeta { node } => ApplyObs::StaleMeta(*node),
        reference::ApplyError::StaleAnchor { node } => ApplyObs::StaleAnchor(*node),
        reference::ApplyError::StaleLabel { edge } => ApplyObs::StaleLabel(*edge),
        reference::ApplyError::StalePosition { edge } => ApplyObs::StalePosition(*edge),
        reference::ApplyError::DanglingEdges { node, edges } => {
            ApplyObs::DanglingEdges(*node, edges.clone())
        }
        reference::ApplyError::DuplicateNodeId(id) => ApplyObs::DuplicateNodeId(*id),
        reference::ApplyError::DuplicateEdge(id) => ApplyObs::DuplicateEdge(*id),
        reference::ApplyError::MissingEndpoint { edge, node } => {
            ApplyObs::MissingEndpoint(*edge, *node)
        }
        reference::ApplyError::MalformedFan { node, reason } => {
            ApplyObs::MalformedFan(*node, fan_reference(reason))
        }
        reference::ApplyError::ElementTypeMismatch(_)
        | reference::ApplyError::AttributeTypeMismatch(_) => {
            panic!("normal release delta produced a representation error: {error:?}")
        }
    }
}

fn production_apply_error(error: &EditError) -> ApplyObs {
    match error {
        EditError::NodeNotFound(id) => ApplyObs::NodeNotFound(*id),
        EditError::StaleNode { node } => ApplyObs::StaleNode(*node),
        EditError::EdgeNotFound(id) => ApplyObs::EdgeNotFound(id.0),
        EditError::StaleEdge { edge } => ApplyObs::StaleEdge(edge.0),
        EditError::StaleValue { node } => ApplyObs::StaleValue(*node),
        EditError::StaleField { node, field } => ApplyObs::StaleField(*node, field.clone()),
        EditError::StaleMeta { node } => ApplyObs::StaleMeta(*node),
        EditError::StaleAnchor { node } => ApplyObs::StaleAnchor(*node),
        EditError::StaleLabel { edge } => ApplyObs::StaleLabel(edge.0),
        EditError::StalePosition { edge } => ApplyObs::StalePosition(edge.0),
        EditError::DanglingEdges { node, edges } => {
            ApplyObs::DanglingEdges(*node, edges.iter().map(|edge| edge.0).collect())
        }
        EditError::DuplicateNodeId(id) => ApplyObs::DuplicateNodeId(*id),
        EditError::DuplicateEdge(id) => ApplyObs::DuplicateEdge(id.0),
        EditError::MissingEndpoint { edge, node } => ApplyObs::MissingEndpoint(edge.0, *node),
        EditError::MalformedFan { node, reason } => {
            ApplyObs::MalformedFan(*node, fan_production(reason))
        }
        other => panic!("unexpected production graph-apply error: {other:?}"),
    }
}

fn reference_compose_error(
    error: &reference::Mismatch<reference::GElem, reference::AttrKey>,
) -> ElementObs {
    match error {
        reference::Mismatch::Elem(element) => reference_element(element),
        reference::Mismatch::Attr(key) => ElementObs::Attr(format!("{key:?}")),
    }
}

fn production_compose_error(error: &ComposeError) -> ElementObs {
    match error {
        ComposeError::Mismatch { elem } => production_elem_ref(elem),
    }
}

type ProductionRow = (RowId, HashMap<String, Value>);

fn table_to_production(table: &reference_model::Table) -> FInstance {
    let mut by_table: BTreeMap<Name, Vec<ProductionRow>> = BTreeMap::new();
    for (id, (name, cells)) in &table.rows {
        by_table.entry(name.as_str().into()).or_default().push((
            RowId(*id),
            cells
                .iter()
                .map(|(column, value)| (column.clone(), value_to_production(value)))
                .collect(),
        ));
    }
    let mut pairs: BTreeMap<Edge, Vec<(RowId, RowId)>> = BTreeMap::new();
    for (edge, src, tgt) in &table.pairs {
        pairs
            .entry(edge_to_production(edge))
            .or_default()
            .push((RowId(*src), RowId(*tgt)));
    }
    FInstance::from_parts(by_table, pairs).expect("reference table is well formed")
}

fn table_to_reference(table: &FInstance) -> reference_model::Table {
    let rows = table
        .row_ids()
        .into_iter()
        .map(|id| {
            let name = table.table_of(id).expect("row has a table").to_string();
            let cells = table
                .row(id)
                .expect("listed row exists")
                .iter()
                .map(|(column, value)| (column.clone(), value_to_reference(value)))
                .collect();
            (id.0, (name, cells))
        })
        .collect();
    let pairs = table
        .foreign_keys()
        .into_iter()
        .flat_map(|(edge, pairs)| {
            pairs
                .iter()
                .map(move |(src, tgt)| (edge_to_reference(edge), src.0, tgt.0))
        })
        .collect();
    reference_model::Table { rows, pairs }
}

fn table_graph_under(
    table: &reference_model::Table,
    embedding: &TableEmbedding,
) -> reference_model::Graph {
    let nodes = table
        .rows
        .iter()
        .map(|(id, (name, cells))| {
            let mut node = reference_model::Node::new(*id, name);
            node.fields.clone_from(cells);
            (*id, node)
        })
        .collect();
    let edges = table
        .pairs
        .iter()
        .map(|(edge, src, tgt)| {
            let id = embedding
                .pairs
                .get(&(edge_to_production(edge), RowId(*src), RowId(*tgt)))
                .expect("embedding records every table pair");
            (
                id.0,
                reference_model::GEdge {
                    src: *src,
                    tgt: *tgt,
                    label: edge.clone(),
                    position: None,
                },
            )
        })
        .collect();
    reference_model::Graph { nodes, edges }
}

fn table_delta_to_production(delta: &reference::TableDelta) -> TableDelta {
    let mut parts = TableDeltaParts::default();
    for (element, payload) in &delta.del {
        match (element, payload) {
            (reference::TElem::Row(id), reference::TPayload::Row(table, cells)) => {
                parts.del_rows.insert(
                    RowId(*id),
                    (
                        table.as_str().into(),
                        cells
                            .iter()
                            .map(|(column, value)| (column.clone(), value_to_production(value)))
                            .collect(),
                    ),
                );
            }
            (reference::TElem::Pair(edge, src, tgt), reference::TPayload::Pair) => {
                parts
                    .del_pairs
                    .insert((edge_to_production(edge), RowId(*src), RowId(*tgt)));
            }
            _ => panic!("cross-sort reference table deletion: {element:?} {payload:?}"),
        }
    }
    for (element, payload) in &delta.add {
        match (element, payload) {
            (reference::TElem::Row(id), reference::TPayload::Row(table, cells)) => {
                parts.add_rows.insert(
                    RowId(*id),
                    (
                        table.as_str().into(),
                        cells
                            .iter()
                            .map(|(column, value)| (column.clone(), value_to_production(value)))
                            .collect(),
                    ),
                );
            }
            (reference::TElem::Pair(edge, src, tgt), reference::TPayload::Pair) => {
                parts
                    .add_pairs
                    .insert((edge_to_production(edge), RowId(*src), RowId(*tgt)));
            }
            _ => panic!("cross-sort reference table addition: {element:?} {payload:?}"),
        }
    }
    for (key, (old, new)) in &delta.attrs {
        match (key, old, new) {
            (
                reference::TKey::Cell(row, column),
                reference::TVal::Cell(old),
                reference::TVal::Cell(new),
            ) => {
                parts.cells.insert(
                    (RowId(*row), column.clone()),
                    CellChange {
                        old: old.as_ref().map(value_to_production),
                        new: new.as_ref().map(value_to_production),
                    },
                );
            }
            (
                reference::TKey::Table(row),
                reference::TVal::Table(old),
                reference::TVal::Table(new),
            ) => {
                parts.tables.insert(
                    RowId(*row),
                    TableChange {
                        old: old.as_str().into(),
                        new: new.as_str().into(),
                    },
                );
            }
            _ => panic!("cross-sort reference table attribute: {key:?} {old:?} {new:?}"),
        }
    }
    TableDelta::normalize(parts)
}

fn table_delta_to_reference(delta: &TableDelta) -> reference::TableDelta {
    let mut out = reference::TableDelta::identity();
    for (id, (table, cells)) in delta.del_rows() {
        out.del.insert(
            reference::TElem::Row(id.0),
            reference::TPayload::Row(
                table.to_string(),
                cells
                    .iter()
                    .map(|(column, value)| (column.clone(), value_to_reference(value)))
                    .collect(),
            ),
        );
    }
    for (edge, src, tgt) in delta.del_pairs() {
        out.del.insert(
            reference::TElem::Pair(edge_to_reference(edge), src.0, tgt.0),
            reference::TPayload::Pair,
        );
    }
    for (id, (table, cells)) in delta.add_rows() {
        out.add.insert(
            reference::TElem::Row(id.0),
            reference::TPayload::Row(
                table.to_string(),
                cells
                    .iter()
                    .map(|(column, value)| (column.clone(), value_to_reference(value)))
                    .collect(),
            ),
        );
    }
    for (edge, src, tgt) in delta.add_pairs() {
        out.add.insert(
            reference::TElem::Pair(edge_to_reference(edge), src.0, tgt.0),
            reference::TPayload::Pair,
        );
    }
    for ((row, column), change) in delta.cells() {
        out.attrs.insert(
            reference::TKey::Cell(row.0, column.clone()),
            (
                reference::TVal::Cell(change.old.as_ref().map(value_to_reference)),
                reference::TVal::Cell(change.new.as_ref().map(value_to_reference)),
            ),
        );
    }
    for (row, change) in delta.tables() {
        out.attrs.insert(
            reference::TKey::Table(row.0),
            (
                reference::TVal::Table(change.old.to_string()),
                reference::TVal::Table(change.new.to_string()),
            ),
        );
    }
    out
}

fn reference_table_elem(element: &reference::TElem) -> ElementObs {
    match element {
        reference::TElem::Row(row) => ElementObs::Row(*row),
        reference::TElem::Pair(edge, src, tgt) => ElementObs::Pair(format!("{edge:?}"), *src, *tgt),
    }
}

fn reference_table_compose_error(
    error: &reference::Mismatch<reference::TElem, reference::TKey>,
) -> ElementObs {
    match error {
        reference::Mismatch::Elem(element) => reference_table_elem(element),
        reference::Mismatch::Attr(key) => ElementObs::Attr(format!("{key:?}")),
    }
}

fn production_table_compose_error(error: &ComposeError) -> ElementObs {
    match error {
        ComposeError::Mismatch {
            elem: ElemRef::Row(row),
        } => ElementObs::Row(row.0),
        ComposeError::Mismatch {
            elem: ElemRef::Pair { edge, src, tgt },
        } => ElementObs::Pair(format!("{:?}", edge_to_reference(edge)), src.0, tgt.0),
        ComposeError::Mismatch {
            elem: ElemRef::Attr(AttrKey::Cell { row, column }),
        } => ElementObs::Attr(format!("Cell({}, {column:?})", row.0)),
        ComposeError::Mismatch {
            elem: ElemRef::Attr(AttrKey::Table { row }),
        } => ElementObs::Attr(format!("Table({})", row.0)),
        other @ ComposeError::Mismatch {
            elem:
                ElemRef::Node(_)
                | ElemRef::Edge(_)
                | ElemRef::Attr(
                    AttrKey::Value { .. }
                    | AttrKey::Field { .. }
                    | AttrKey::Meta { .. }
                    | AttrKey::Anchor { .. }
                    | AttrKey::EdgeLabel { .. }
                    | AttrKey::EdgePosition { .. },
                ),
        } => panic!("graph element returned by table composition: {other:?}"),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TableApplyObs {
    RowNotFound(u32),
    ForeignKeyNotFound(String, u32, u32),
    StaleCell(u32, String),
    StaleTable(u32),
    StaleIncidence(u32),
    DanglingForeignKeys(u32, Vec<(String, u32, u32)>),
    DuplicateRow(u32),
    DuplicateForeignKey(String, u32, u32),
    MissingRowEndpoint(String, u32),
}

fn reference_table_apply_error(error: &reference::TableApplyError) -> TableApplyObs {
    match error {
        reference::TableApplyError::RowNotFound(row) => TableApplyObs::RowNotFound(*row),
        reference::TableApplyError::ForeignKeyNotFound { edge, src, tgt } => {
            TableApplyObs::ForeignKeyNotFound(format!("{edge:?}"), *src, *tgt)
        }
        reference::TableApplyError::StaleCell { row, column } => {
            TableApplyObs::StaleCell(*row, column.clone())
        }
        reference::TableApplyError::StaleTable { row } => TableApplyObs::StaleTable(*row),
        reference::TableApplyError::StaleIncidence { row } => TableApplyObs::StaleIncidence(*row),
        reference::TableApplyError::DanglingForeignKeys { row, pairs } => {
            TableApplyObs::DanglingForeignKeys(
                *row,
                pairs
                    .iter()
                    .map(|(edge, src, tgt)| (format!("{edge:?}"), *src, *tgt))
                    .collect(),
            )
        }
        reference::TableApplyError::DuplicateRow(row) => TableApplyObs::DuplicateRow(*row),
        reference::TableApplyError::DuplicateForeignKey { edge, src, tgt } => {
            TableApplyObs::DuplicateForeignKey(format!("{edge:?}"), *src, *tgt)
        }
        reference::TableApplyError::MissingRowEndpoint { edge, row } => {
            TableApplyObs::MissingRowEndpoint(format!("{edge:?}"), *row)
        }
        reference::TableApplyError::ElementTypeMismatch(_)
        | reference::TableApplyError::AttributeTypeMismatch(_) => {
            panic!("normal table delta produced a representation error: {error:?}")
        }
    }
}

fn production_table_apply_error(error: &EditError) -> TableApplyObs {
    match error {
        EditError::RowNotFound(row) => TableApplyObs::RowNotFound(row.0),
        EditError::ForeignKeyNotFound { edge, src, tgt } => TableApplyObs::ForeignKeyNotFound(
            format!("{:?}", edge_to_reference(edge)),
            src.0,
            tgt.0,
        ),
        EditError::StaleCell { row, column } => TableApplyObs::StaleCell(row.0, column.clone()),
        EditError::StaleTable { row } => TableApplyObs::StaleTable(row.0),
        EditError::StaleIncidence { row } => TableApplyObs::StaleIncidence(row.0),
        EditError::DanglingForeignKeys { row, pairs } => TableApplyObs::DanglingForeignKeys(
            row.0,
            pairs
                .iter()
                .map(|(edge, src, tgt)| (format!("{:?}", edge_to_reference(edge)), src.0, tgt.0))
                .collect(),
        ),
        EditError::DuplicateRow(row) => TableApplyObs::DuplicateRow(row.0),
        EditError::DuplicateForeignKey { edge, src, tgt } => TableApplyObs::DuplicateForeignKey(
            format!("{:?}", edge_to_reference(edge)),
            src.0,
            tgt.0,
        ),
        EditError::MissingRowEndpoint { edge, row } => {
            TableApplyObs::MissingRowEndpoint(format!("{:?}", edge_to_reference(edge)), row.0)
        }
        other => panic!("unexpected production table-apply error: {other:?}"),
    }
}

fn fan_to_production(fan: &reference_model::Fan) -> Fan {
    Fan {
        hyper_edge_id: fan.hyper_edge.clone(),
        parent: fan.parent,
        children: fan.children.clone().into_iter().collect(),
    }
}

fn fan_to_reference(fan: &Fan) -> reference_model::Fan {
    reference_model::Fan {
        parent: fan.parent,
        hyper_edge: fan.hyper_edge_id.clone(),
        children: fan.children.clone().into_iter().collect(),
    }
}

fn tree_to_production(tree: &reference_model::Tree) -> WInstance {
    WInstance::new(
        tree.nodes
            .iter()
            .map(|(id, node)| (*id, node_to_production(node)))
            .collect(),
        tree.arcs
            .iter()
            .map(|(parent, child, edge)| (*parent, *child, edge_to_production(edge)))
            .collect(),
        tree.fans.iter().map(fan_to_production).collect(),
        tree.root,
        tree.nodes[&tree.root].anchor.as_str().into(),
    )
}

fn tree_to_reference(tree: &WInstance) -> reference_model::Tree {
    reference_model::Tree {
        nodes: tree
            .nodes
            .iter()
            .map(|(id, node)| (*id, node_to_reference(node)))
            .collect(),
        arcs: tree
            .arcs
            .iter()
            .map(|(parent, child, edge)| (*parent, *child, edge_to_reference(edge)))
            .collect(),
        fans: tree.fans.iter().map(fan_to_reference).collect(),
        root: tree.root,
    }
}

fn tree_edit_to_production(edit: &reference::TreeEdit) -> TreeEdit {
    match edit {
        reference::TreeEdit::Identity => TreeEdit::Identity,
        reference::TreeEdit::InsertNode {
            parent,
            child_id,
            node,
            edge,
        } => TreeEdit::InsertNode {
            parent: *parent,
            child_id: *child_id,
            node: node_to_production(node),
            edge: edge_to_production(edge),
        },
        reference::TreeEdit::DeleteNode { id } => TreeEdit::DeleteNode { id: *id },
        reference::TreeEdit::ContractNode { id } => TreeEdit::ContractNode { id: *id },
        reference::TreeEdit::RelabelNode { id, new_anchor } => TreeEdit::RelabelNode {
            id: *id,
            new_anchor: new_anchor.as_str().into(),
        },
        reference::TreeEdit::SetField {
            node_id,
            field,
            value,
        } => TreeEdit::SetField {
            node_id: *node_id,
            field: field.as_str().into(),
            value: value_to_production(value),
        },
        reference::TreeEdit::RemoveField { node_id, field } => TreeEdit::RemoveField {
            node_id: *node_id,
            field: field.as_str().into(),
        },
        reference::TreeEdit::MoveSubtree {
            node_id,
            new_parent,
            edge,
        } => TreeEdit::MoveSubtree {
            node_id: *node_id,
            new_parent: *new_parent,
            edge: edge_to_production(edge),
        },
        reference::TreeEdit::InsertFan { fan } => TreeEdit::InsertFan {
            fan: fan_to_production(fan),
        },
        reference::TreeEdit::DeleteFan { hyper_edge } => TreeEdit::DeleteFan {
            hyper_edge_id: hyper_edge.as_str().into(),
        },
        reference::TreeEdit::JoinFeatures {
            primary,
            joined,
            produce,
        } => TreeEdit::JoinFeatures {
            primary: *primary,
            joined: joined.clone(),
            produce: node_to_production(produce),
        },
        reference::TreeEdit::Sequence(steps) => {
            TreeEdit::Sequence(steps.iter().map(tree_edit_to_production).collect())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EmbeddingObs {
    arcs: BTreeMap<u32, u32>,
    fans: Vec<(u32, u32, BTreeMap<String, u32>)>,
    next_node: u32,
    next_edge: u32,
}

fn reference_embedding(embedding: &panproto_reference::encode::TreeEmbedding) -> EmbeddingObs {
    EmbeddingObs {
        arcs: embedding.arcs.clone(),
        fans: embedding.fans.clone(),
        next_node: embedding.next_node,
        next_edge: embedding.next_edge,
    }
}

fn production_embedding(embedding: &TreeEmbedding) -> EmbeddingObs {
    EmbeddingObs {
        arcs: embedding
            .arcs
            .iter()
            .map(|(child, edge)| (*child, edge.0))
            .collect(),
        fans: embedding
            .fans
            .iter()
            .map(|fan| {
                (
                    fan.node,
                    fan.parent_edge.0,
                    fan.child_edges
                        .iter()
                        .map(|(label, edge)| (label.clone(), edge.0))
                        .collect(),
                )
            })
            .collect(),
        next_node: embedding.next_node,
        next_edge: embedding.next_edge,
    }
}

fn tree(nodes: &[(u32, &str)], arcs: &[(u32, u32, &str)]) -> reference_model::Tree {
    reference_model::Tree {
        nodes: nodes
            .iter()
            .map(|(id, anchor)| (*id, reference_model::Node::new(*id, anchor)))
            .collect(),
        arcs: arcs
            .iter()
            .map(|(parent, child, name)| {
                (
                    *parent,
                    *child,
                    reference_model::SEdge::new(
                        nodes
                            .iter()
                            .find(|(id, _)| id == parent)
                            .expect("parent exists")
                            .1,
                        nodes
                            .iter()
                            .find(|(id, _)| id == child)
                            .expect("child exists")
                            .1,
                        "child",
                        Some(name),
                    ),
                )
            })
            .collect(),
        fans: Vec::new(),
        root: 0,
    }
}

struct TreeReleaseFixtures {
    base: reference_model::Tree,
    with_field: reference_model::Tree,
    fan: reference_model::Fan,
    with_fan: reference_model::Tree,
    join: reference_model::Tree,
}

fn tree_release_fixtures() -> TreeReleaseFixtures {
    let base = tree(&[(0, "r"), (1, "a"), (2, "b")], &[(0, 1, "a"), (1, 2, "b")]);
    let mut with_field = base.clone();
    with_field
        .nodes
        .get_mut(&1)
        .expect("node exists")
        .fields
        .insert("f".to_owned(), ReferenceValue::Int(0));
    let fan = reference_model::Fan {
        parent: 0,
        hyper_edge: "h".to_owned(),
        children: BTreeMap::from([("arm".to_owned(), 2)]),
    };
    let mut with_fan = base.clone();
    with_fan.fans.push(fan.clone());
    let join = tree(
        &[(0, "r"), (1, "a"), (2, "a"), (3, "c")],
        &[(0, 1, "one"), (0, 2, "two"), (1, 3, "child")],
    );
    TreeReleaseFixtures {
        base,
        with_field,
        fan,
        with_fan,
        join,
    }
}

fn tree_release_sequence() -> reference::TreeEdit {
    reference::TreeEdit::Sequence(vec![
        reference::TreeEdit::RelabelNode {
            id: 1,
            new_anchor: "z".to_owned(),
        },
        reference::TreeEdit::SetField {
            node_id: 1,
            field: "f".to_owned(),
            value: ReferenceValue::Int(0),
        },
        reference::TreeEdit::MoveSubtree {
            node_id: 2,
            new_parent: 0,
            edge: reference_model::SEdge::new("r", "b", "child", Some("moved")),
        },
    ])
}

type TreeReleaseCase = (&'static str, reference_model::Tree, reference::TreeEdit);

fn tree_release_cases() -> Vec<TreeReleaseCase> {
    let fixtures = tree_release_fixtures();
    let mut cases = tree_release_primitive_cases(&fixtures);
    cases.extend(tree_release_composite_cases(fixtures));
    cases
}

/// The release cases of the primitives that change nodes, fields and arcs.
fn tree_release_primitive_cases(fixtures: &TreeReleaseFixtures) -> Vec<TreeReleaseCase> {
    let base = &fixtures.base;
    let with_field = fixtures.with_field.clone();
    vec![
        ("identity", base.clone(), reference::TreeEdit::Identity),
        (
            "insert",
            base.clone(),
            reference::TreeEdit::InsertNode {
                parent: 0,
                child_id: 3,
                node: reference_model::Node::new(3, "c"),
                edge: reference_model::SEdge::new("r", "c", "child", Some("c")),
            },
        ),
        (
            "delete",
            base.clone(),
            reference::TreeEdit::DeleteNode { id: 2 },
        ),
        (
            "contract",
            base.clone(),
            reference::TreeEdit::ContractNode { id: 1 },
        ),
        (
            "relabel",
            base.clone(),
            reference::TreeEdit::RelabelNode {
                id: 1,
                new_anchor: "z".to_owned(),
            },
        ),
        (
            "set-field",
            base.clone(),
            reference::TreeEdit::SetField {
                node_id: 1,
                field: "f".to_owned(),
                value: ReferenceValue::Int(0),
            },
        ),
        (
            "remove-field",
            with_field,
            reference::TreeEdit::RemoveField {
                node_id: 1,
                field: "f".to_owned(),
            },
        ),
        (
            "move",
            base.clone(),
            reference::TreeEdit::MoveSubtree {
                node_id: 2,
                new_parent: 0,
                edge: reference_model::SEdge::new("r", "b", "child", Some("moved")),
            },
        ),
    ]
}

/// The release cases of fans, joins and sequences.
fn tree_release_composite_cases(fixtures: TreeReleaseFixtures) -> Vec<TreeReleaseCase> {
    let TreeReleaseFixtures {
        base,
        fan,
        with_fan,
        join,
        ..
    } = fixtures;
    vec![
        (
            "insert-fan",
            base.clone(),
            reference::TreeEdit::InsertFan { fan },
        ),
        (
            "delete-fan",
            with_fan,
            reference::TreeEdit::DeleteFan {
                hyper_edge: "h".to_owned(),
            },
        ),
        (
            "join",
            join,
            reference::TreeEdit::JoinFeatures {
                primary: 2,
                joined: vec![1],
                produce: reference_model::Node::new(2, "a"),
            },
        ),
        ("sequence", base.clone(), tree_release_sequence()),
        (
            "delete-then-reinsert",
            base,
            reference::TreeEdit::Sequence(vec![
                reference::TreeEdit::DeleteNode { id: 2 },
                reference::TreeEdit::InsertNode {
                    parent: 1,
                    child_id: 2,
                    node: reference_model::Node::new(2, "b"),
                    edge: reference_model::SEdge::new("a", "b", "child", Some("b")),
                },
            ]),
        ),
        {
            let case = carrier::join_cases()
                .into_iter()
                .find(|case| case.name == "join-two-with-fans")
                .expect("the carrier has the two-node join");
            ("join-two-with-fans", case.tree, case.edit)
        },
    ]
}

#[test]
fn apply_matches_reference_on_every_graph_and_atomic_delta() {
    let graphs = carrier::graphs();
    let deltas = carrier::graph_delta_cases();
    assert_eq!(graphs.len(), 105);
    assert_eq!(deltas.len(), 25);

    let mut outcomes = BTreeMap::<Result<(), ApplyObs>, usize>::new();
    for graph in &graphs {
        for case in &deltas {
            let expected = panproto_reference::edit::apply(&case.delta, graph)
                .map_err(|error| reference_apply_error(&error));
            let delta = delta_to_production(&case.delta);
            let mut actual_graph = graph_to_production(graph);
            let actual = delta
                .apply(&mut actual_graph)
                .map(|()| graph_to_reference(&actual_graph))
                .map_err(|error| production_apply_error(&error));
            assert_eq!(actual, expected, "graph={graph:?}; delta={:?}", case.delta);
            *outcomes
                .entry(expected.as_ref().map(|_| ()).map_err(Clone::clone))
                .or_default() += 1;
        }
    }
    assert_eq!(outcomes.values().sum::<usize>(), 105 * 25);
    assert!(outcomes.keys().any(Result::is_ok));
    assert!(outcomes.keys().any(Result::is_err));
}

#[test]
fn algebra_matches_reference_on_the_exact_atomic_word_carrier() {
    let cases = carrier::graph_delta_cases();
    assert_eq!(cases.len(), 25);

    for case in &cases {
        let production = delta_to_production(&case.delta);
        assert_eq!(
            delta_to_reference(&production.invert()),
            panproto_reference::edit::Delta::invert(&case.delta),
            "invert: {:?}",
            case.delta
        );
        assert_eq!(
            delta_to_reference(&production.invert().invert()),
            case.delta,
            "involution: {:?}",
            case.delta
        );
    }

    let mut pair_count = 0usize;
    for first in &cases {
        for second in &cases {
            let reference = panproto_reference::edit::Delta::compose(&first.delta, &second.delta);
            let production =
                delta_to_production(&first.delta).compose(&delta_to_production(&second.delta));
            match (reference, production) {
                (Ok(expected), Ok(actual)) => assert_eq!(
                    delta_to_reference(&actual),
                    expected,
                    "compose {:?} then {:?}",
                    first.delta,
                    second.delta
                ),
                (Err(expected), Err(actual)) => assert_eq!(
                    production_compose_error(&actual),
                    reference_compose_error(&expected),
                    "compose refusal {:?} then {:?}",
                    first.delta,
                    second.delta
                ),
                (expected, actual) => panic!(
                    "compose domain differs for {:?} then {:?}: reference={expected:?}, production={actual:?}",
                    first.delta, second.delta
                ),
            }
            pair_count += 1;
        }
    }
    assert_eq!(pair_count, 25 * 25);

    let words = carrier::graph_delta_sequences();
    assert_eq!(words.len(), 1 + 25 + 25 * 25 + 25 * 25 * 25);
    for word in words {
        let mut expected = Ok(reference::GraphDelta::identity());
        let mut actual = Ok(GraphDelta::identity());
        for letter in &word {
            expected = expected.and_then(|prefix| {
                panproto_reference::edit::Delta::compose(&prefix, &letter.delta)
            });
            actual = actual.and_then(|prefix| prefix.compose(&delta_to_production(&letter.delta)));
        }
        match (expected, actual) {
            (Ok(expected), Ok(actual)) => assert_eq!(delta_to_reference(&actual), expected),
            (Err(expected), Err(actual)) => assert_eq!(
                production_compose_error(&actual),
                reference_compose_error(&expected)
            ),
            (expected, actual) => panic!(
                "word composition domain differs: reference={expected:?}, production={actual:?}"
            ),
        }
    }
}

#[test]
fn independence_and_merge_match_reference_on_every_atomic_pair() {
    let cases = carrier::graph_delta_cases();
    let mut count = 0usize;
    for first in &cases {
        for second in &cases {
            let expected_conflicts: Vec<_> =
                panproto_reference::edit::independence(&first.delta, &second.delta)
                    .iter()
                    .map(reference_conflict)
                    .collect();
            let first_production = delta_to_production(&first.delta);
            let second_production = delta_to_production(&second.delta);
            let actual_conflicts: Vec<_> = first_production
                .independence(&second_production)
                .iter()
                .map(production_conflict)
                .collect();
            assert_eq!(
                actual_conflicts, expected_conflicts,
                "independence {:?} vs {:?}",
                first.delta, second.delta
            );

            let expected_merge = panproto_reference::edit::merge(&first.delta, &second.delta);
            let actual_merge = first_production.merge(&second_production);
            match (expected_merge, actual_merge) {
                (Ok(expected), Ok(actual)) => assert_eq!(delta_to_reference(&actual), expected),
                (Err(expected), Err(actual)) => {
                    assert_eq!(production_conflict(&actual), reference_conflict(&expected));
                }
                (expected, actual) => {
                    panic!("merge domain differs: reference={expected:?}, production={actual:?}")
                }
            }
            count += 1;
        }
    }
    assert_eq!(count, 25 * 25);
}

#[test]
fn e3a_graph_delta_bytes_match_reference_on_every_atomic_delta() {
    let cases = carrier::graph_delta_cases();
    let mut distinct = BTreeSet::new();
    for case in &cases {
        let expected = reference::canonical_graph_delta_bytes(&case.delta)
            .expect("release-carrier delta has canonical bytes");
        let mut writer = CanonicalWriter::new();
        canonical_graph_delta_bytes(&delta_to_production(&case.delta), &mut writer);
        let actual = writer.into_bytes();
        assert_eq!(actual, expected, "delta={:?}", case.delta);
        distinct.insert(actual);
    }
    assert_eq!(distinct.len(), 25);
}

#[test]
fn decision_9_delete_then_add_is_replacement_even_for_equal_payloads() {
    let payloads: Vec<_> = carrier::graph_delta_cases()
        .into_iter()
        .filter_map(|case| {
            let (element, payload) = case.delta.del.into_iter().next()?;
            case.delta.add.is_empty().then_some((element, payload))
        })
        .collect();
    assert_eq!(payloads.len(), 6);

    for (element, payload) in payloads {
        let mut delete = reference::GraphDelta::identity();
        delete.del.insert(element.clone(), payload.clone());
        let mut add = reference::GraphDelta::identity();
        add.add.insert(element.clone(), payload.clone());
        let expected = panproto_reference::edit::Delta::compose(&delete, &add)
            .expect("delete then add composes");
        assert_eq!(expected.del.get(&element), Some(&payload));
        assert_eq!(expected.add.get(&element), Some(&payload));

        let actual = delta_to_production(&delete)
            .compose(&delta_to_production(&add))
            .expect("production delete then add composes");
        assert_eq!(delta_to_reference(&actual), expected);
        assert!(!actual.is_identity());
        assert_eq!(actual.invert(), actual);
    }
}

#[test]
fn table_apply_and_algebra_match_the_exact_release_carrier() {
    let tables = carrier::tables();
    let deltas = carrier::table_deltas();
    assert_eq!(
        u64::try_from(tables.len()).unwrap(),
        carrier::tables_count_certificate()
    );
    assert_eq!(
        u64::try_from(deltas.len()).unwrap(),
        carrier::table_deltas_count_certificate()
    );

    let mut apply_count = 0usize;
    for table in &tables {
        for delta in &deltas {
            let expected = panproto_reference::edit::apply_table(delta, table)
                .map_err(|error| reference_table_apply_error(&error));
            let production_delta = table_delta_to_production(delta);
            let mut actual_table = table_to_production(table);
            let actual = production_delta
                .apply(&mut actual_table)
                .map(|()| table_to_reference(&actual_table))
                .map_err(|error| production_table_apply_error(&error));
            assert_eq!(actual, expected, "table={table:?}; delta={delta:?}");
            apply_count += 1;
        }
    }
    assert_eq!(apply_count, 105 * 11);

    for delta in &deltas {
        let production = table_delta_to_production(delta);
        assert_eq!(
            table_delta_to_reference(&production.invert()),
            panproto_reference::edit::invert_table(delta)
        );
        assert_eq!(
            table_delta_to_reference(&production.invert().invert()),
            *delta
        );
    }

    let mut pair_count = 0usize;
    for first in &deltas {
        for second in &deltas {
            let expected = panproto_reference::edit::compose_table(first, second);
            let actual =
                table_delta_to_production(first).compose(&table_delta_to_production(second));
            match (expected, actual) {
                (Ok(expected), Ok(actual)) => {
                    assert_eq!(table_delta_to_reference(&actual), expected);
                }
                (Err(expected), Err(actual)) => assert_eq!(
                    production_table_compose_error(&actual),
                    reference_table_compose_error(&expected)
                ),
                (expected, actual) => panic!(
                    "table compose domain differs: reference={expected:?}, production={actual:?}"
                ),
            }
            pair_count += 1;
        }
    }
    assert_eq!(pair_count, 11 * 11);

    let mut diff_count = 0usize;
    for before in &tables {
        for after in &tables {
            let expected = panproto_reference::edit::diff_table(before, after);
            let actual =
                TableDelta::diff(&table_to_production(before), &table_to_production(after));
            assert_eq!(table_delta_to_reference(&actual), expected);
            diff_count += 1;
        }
    }
    assert_eq!(diff_count, 105 * 105);

    let words = carrier::table_delta_sequences();
    assert_eq!(
        u64::try_from(words.len()).unwrap(),
        carrier::table_delta_sequences_count_certificate()
    );
    for word in words {
        let mut expected = Ok(reference::TableDelta::identity());
        let mut actual = Ok(TableDelta::identity());
        for letter in &word {
            expected = expected.and_then(|prefix| reference::compose_table(&prefix, letter));
            actual = actual.and_then(|prefix| prefix.compose(&table_delta_to_production(letter)));
        }
        match (expected, actual) {
            (Ok(expected), Ok(actual)) => {
                assert_eq!(table_delta_to_reference(&actual), expected);
            }
            (Err(expected), Err(actual)) => assert_eq!(
                production_table_compose_error(&actual),
                reference_table_compose_error(&expected)
            ),
            (expected, actual) => panic!(
                "table word composition domain differs: reference={expected:?}, production={actual:?}"
            ),
        }
    }
}

#[test]
fn e3a_table_delta_bytes_match_reference_on_every_atomic_delta() {
    let deltas = carrier::table_deltas();
    let mut distinct = BTreeSet::new();
    for delta in &deltas {
        let expected = reference::canonical_table_delta_bytes(delta)
            .expect("release-carrier table delta has canonical bytes");
        let mut writer = CanonicalWriter::new();
        canonical_table_delta_bytes(&table_delta_to_production(delta), &mut writer);
        let actual = writer.into_bytes();
        assert_eq!(actual, expected, "delta={delta:?}");
        distinct.insert(actual);
    }
    assert_eq!(distinct.len(), 11);
}

#[test]
fn translation_square_commutes_for_every_applicable_table_and_atomic_delta() {
    let mut applicable = 0usize;
    for table in carrier::tables() {
        let production_table = table_to_production(&table);
        let (production_graph, initial_embedding) =
            GInstance::from_finstance(&production_table).expect("release table embeds");
        let (reference_graph, reference_embedding_state) =
            panproto_reference::encode::table_to_graph(&table).expect("release table embeds");
        assert_eq!(graph_to_reference(&production_graph), reference_graph);
        assert_eq!(
            initial_embedding
                .pairs
                .iter()
                .map(|((edge, src, tgt), id)| { ((edge_to_reference(edge), src.0, tgt.0), id.0) })
                .collect::<BTreeMap<_, _>>(),
            reference_embedding_state
        );

        for delta in carrier::table_deltas() {
            let Ok(after_table) = reference::apply_table(&delta, &table) else {
                continue;
            };
            let production_delta = table_delta_to_production(&delta);
            let mut result_embedding = initial_embedding.clone();
            let graph_delta = production_delta
                .to_graph_delta(&mut result_embedding)
                .unwrap_or_else(|error| panic!("table-to-graph translation refused: {error:?}"));
            let expected_graph = table_graph_under(&after_table, &result_embedding);
            let expected_delta = reference::diff(&reference_graph, &expected_graph);
            assert_eq!(delta_to_reference(&graph_delta), expected_delta);

            let mut applied_graph = production_graph.clone();
            graph_delta
                .apply(&mut applied_graph)
                .unwrap_or_else(|error| panic!("translated graph delta refused: {error:?}"));
            assert_eq!(graph_to_reference(&applied_graph), expected_graph);

            let mut round_trip_embedding = initial_embedding.clone();
            let round_trip = TableDelta::from_graph_delta(&graph_delta, &mut round_trip_embedding)
                .unwrap_or_else(|error| panic!("graph-to-table translation refused: {error:?}"));
            assert_eq!(round_trip, production_delta);
            assert_eq!(round_trip_embedding, result_embedding);
            applicable += 1;
        }
    }
    // Independently: 105 identities; 2 row adds; 50 row deletes without
    // incident pairs; 90 pair adds; 14 pair deletes; 52 changes in each
    // cell direction; and 50 `a -> b` table changes without incident pairs.
    // No `b -> a` change applies because the carrier's rows start in `a`.
    assert_eq!(applicable, 105 + 2 + 50 + 90 + 14 + 52 + 52 + 50);
}

#[test]
fn table_translation_refuses_malformed_embeddings_and_non_table_graph_content_atomically() {
    let table = reference_model::Table {
        rows: BTreeMap::from([
            (0, ("a".to_owned(), BTreeMap::new())),
            (1, ("a".to_owned(), BTreeMap::new())),
        ]),
        pairs: BTreeSet::from([(reference_model::SEdge::new("a", "a", "e", None), 0, 1)]),
    };
    let production_table = table_to_production(&table);
    let (_, mut embedding) = GInstance::from_finstance(&production_table).expect("table embeds");
    let pair = embedding
        .pairs
        .first_key_value()
        .expect("one pair")
        .0
        .clone();
    let mut delete = TableDeltaParts::default();
    delete.del_pairs.insert(pair.clone());
    let delete = TableDelta::normalize(delete);
    embedding.pairs.remove(&pair);
    let before = embedding.clone();
    assert!(matches!(
        delete.to_graph_delta(&mut embedding),
        Err(EditError::ForeignKeyNotFound { .. })
    ));
    assert_eq!(embedding, before);

    let mut parts = GraphDeltaParts::default();
    parts.attrs.insert(
        AttrKey::Value { node: 0 },
        AttrChange {
            old: AttrVal::Presence(None),
            new: AttrVal::Presence(Some(FieldPresence::Present(Value::Int(0)))),
        },
    );
    let graph_delta = GraphDelta::normalize(parts).expect("well-typed graph delta");
    let (_, mut embedding) = GInstance::from_finstance(&production_table).expect("table embeds");
    let before = embedding.clone();
    assert!(matches!(
        TableDelta::from_graph_delta(&graph_delta, &mut embedding),
        Err(EditError::NotTableEdit { .. })
    ));
    assert_eq!(embedding, before);
}

#[test]
fn tree_lowering_and_application_match_reference_for_every_primitive() {
    let cases = tree_release_cases();
    assert_eq!(cases.len(), 14);
    for (name, tree, edit) in cases {
        let (reference_graph, mut reference_embedding_state) =
            panproto_reference::encode::tree_to_graph(&tree, None)
                .expect("release tree has a graph form");
        let production_tree = tree_to_production(&tree);
        let (production_graph, mut production_embedding_state) =
            GInstance::from_wtype_embedded(&production_tree, None)
                .expect("release tree has a production graph form");
        assert_eq!(
            graph_to_reference(&production_graph),
            reference_graph,
            "{name}"
        );
        assert_eq!(
            production_embedding(&production_embedding_state),
            reference_embedding(&reference_embedding_state),
            "initial embedding: {name}"
        );

        let expected_delta = panproto_reference::edit::graph_delta_from_tree_edit(
            &tree,
            &mut reference_embedding_state,
            &edit,
            None,
        )
        .unwrap_or_else(|error| panic!("reference lowering {name}: {error:?}"));
        let production_edit = tree_edit_to_production(&edit);
        let actual_delta = GraphDelta::from_tree_edit(
            &production_graph,
            &mut production_embedding_state,
            &production_edit,
        )
        .unwrap_or_else(|error| panic!("production lowering {name}: {error:?}"));
        assert_eq!(
            delta_to_reference(&actual_delta),
            expected_delta,
            "lowering: {name}"
        );
        assert_eq!(
            production_embedding(&production_embedding_state),
            reference_embedding(&reference_embedding_state),
            "result embedding: {name}"
        );

        let expected_tree = panproto_reference::edit::apply_tree(
            &edit,
            &tree,
            &mut panproto_reference::encode::tree_to_graph(&tree, None)
                .expect("release tree embeds")
                .1,
        )
        .unwrap_or_else(|error| panic!("reference tree application {name}: {error:?}"));
        let mut actual_tree = tree_to_production(&tree);
        production_edit
            .apply(&mut actual_tree)
            .unwrap_or_else(|error| panic!("production tree application {name}: {error:?}"));
        assert_eq!(
            tree_to_reference(&actual_tree),
            expected_tree,
            "apply: {name}"
        );

        let mut expected_graph = reference_graph.clone();
        expected_graph = reference::apply(&expected_delta, &expected_graph)
            .unwrap_or_else(|error| panic!("reference graph application {name}: {error:?}"));
        let mut actual_graph = production_graph.clone();
        actual_delta
            .apply(&mut actual_graph)
            .unwrap_or_else(|error| panic!("production graph application {name}: {error:?}"));
        assert_eq!(
            graph_to_reference(&actual_graph),
            expected_graph,
            "graph apply: {name}"
        );
    }
}

#[test]
fn e3a_tree_edit_bytes_match_reference_for_every_primitive_and_sequence() {
    let cases = tree_release_cases();
    let mut distinct = BTreeSet::new();
    for (name, _, edit) in cases {
        let expected = reference::canonical_tree_edit_bytes(&edit)
            .unwrap_or_else(|| panic!("reference tree bytes missing: {name}"));
        let mut writer = CanonicalWriter::new();
        canonical_tree_edit_bytes(&tree_edit_to_production(&edit), &mut writer);
        let actual = writer.into_bytes();
        assert_eq!(actual, expected, "{name}");
        distinct.insert(actual);
    }
    assert_eq!(distinct.len(), 14);
}

#[test]
fn e3a_misfiled_tree_edit_is_outside_the_reference_byte_domain() {
    let edit = reference::TreeEdit::InsertNode {
        parent: 0,
        child_id: 1,
        node: reference_model::Node::new(2, "c"),
        edge: reference_model::SEdge::new("r", "c", "child", Some("c")),
    };
    assert_eq!(reference::canonical_tree_edit_bytes(&edit), None);

    let production_edit = tree_edit_to_production(&edit);
    let mut production_tree = tree_to_production(&tree(&[(0, "r")], &[]));
    assert!(matches!(
        production_edit.apply(&mut production_tree),
        Err(EditError::MisfiledNode {
            expected: 1,
            actual: 2
        })
    ));

    let mut writer = CanonicalWriter::new();
    canonical_tree_edit_bytes(&production_edit, &mut writer);
    assert_ne!(writer.into_bytes(), Vec::<u8>::new());
}

#[test]
fn malformed_tree_embeddings_refuse_without_mutating_the_embedding() {
    let tree = tree(&[(0, "r"), (1, "a")], &[(0, 1, "a")]);
    let (reference_graph, mut reference_embedding_state) =
        panproto_reference::encode::tree_to_graph(&tree, None).expect("tree embeds");
    let production_tree = tree_to_production(&tree);
    let (production_graph, mut production_embedding_state) =
        GInstance::from_wtype_embedded(&production_tree, None).expect("tree embeds");
    assert_eq!(graph_to_reference(&production_graph), reference_graph);

    reference_embedding_state.arcs.remove(&1);
    production_embedding_state.arcs.remove(&1);
    let reference_before = reference_embedding_state.clone();
    let production_before = production_embedding_state.clone();
    let edit = reference::TreeEdit::RelabelNode {
        id: 1,
        new_anchor: "z".to_owned(),
    };
    let reference_error =
        reference::graph_delta_from_tree_edit(&tree, &mut reference_embedding_state, &edit, None)
            .expect_err("malformed reference embedding must refuse");
    let production_error = GraphDelta::from_tree_edit(
        &production_graph,
        &mut production_embedding_state,
        &tree_edit_to_production(&edit),
    )
    .expect_err("malformed production embedding must refuse");
    assert!(matches!(
        reference_error,
        reference::TreeApplyError::InvalidEmbedding
    ));
    assert!(matches!(production_error, EditError::ApplyFailed(_)));
    assert_eq!(reference_embedding_state, reference_before);
    assert_eq!(production_embedding_state, production_before);
}

/// The distinct normalized deltas of the composable words of length at most
/// two over the atomic alphabet: the atomic deltas themselves and every
/// defined composite of two, each once.
fn two_letter_net_deltas() -> Vec<reference::GraphDelta> {
    let cases = carrier::graph_delta_cases();
    let mut out: Vec<reference::GraphDelta> = Vec::new();
    let mut push = |delta: reference::GraphDelta| {
        if !out.contains(&delta) {
            out.push(delta);
        }
    };
    for first in &cases {
        push(first.delta.clone());
        for second in &cases {
            if let Ok(net) = reference::Delta::compose(&first.delta, &second.delta) {
                push(net);
            }
        }
    }
    out
}

fn production_applied(delta: &reference::GraphDelta, graph: &GInstance) -> Option<GInstance> {
    let mut out = graph.clone();
    delta_to_production(delta)
        .apply(&mut out)
        .ok()
        .map(|()| out)
}

#[test]
fn independent_deltas_commute_and_merge_on_every_carrier_graph() {
    let graphs = carrier::graphs();
    let deltas = two_letter_net_deltas();
    let mut independent_pairs = 0usize;
    let mut squares = 0usize;
    for first in &deltas {
        for second in &deltas {
            if !reference::independence(first, second).is_empty() {
                continue;
            }
            independent_pairs += 1;
            let merged = reference::merge(first, second).expect("independent deltas merge");
            let production_merged = delta_to_production(first)
                .merge(&delta_to_production(second))
                .expect("production merges independent deltas");
            assert_eq!(delta_to_reference(&production_merged), merged);
            for graph in &graphs {
                let (Ok(after_first), Ok(after_second)) = (
                    reference::apply(first, graph),
                    reference::apply(second, graph),
                ) else {
                    continue;
                };
                let first_then_second = reference::apply(second, &after_first)
                    .unwrap_or_else(|error| panic!("second after first refused: {error:?}"));
                let second_then_first = reference::apply(first, &after_second)
                    .unwrap_or_else(|error| panic!("first after second refused: {error:?}"));
                let at_once = reference::apply(&merged, graph)
                    .unwrap_or_else(|error| panic!("merge refused: {error:?}"));
                assert_eq!(
                    first_then_second, second_then_first,
                    "{first:?} | {second:?}"
                );
                assert_eq!(first_then_second, at_once, "{first:?} | {second:?}");

                let production_graph = graph_to_production(graph);
                let production_first = production_applied(first, &production_graph)
                    .expect("production applies the first delta");
                let production_second = production_applied(second, &production_graph)
                    .expect("production applies the second delta");
                let production_one_two = production_applied(second, &production_first)
                    .expect("production applies the second after the first");
                let production_two_one = production_applied(first, &production_second)
                    .expect("production applies the first after the second");
                let mut production_at_once = production_graph.clone();
                production_merged
                    .apply(&mut production_at_once)
                    .expect("production applies the merge");
                for actual in [production_one_two, production_two_one, production_at_once] {
                    assert_eq!(graph_to_reference(&actual), at_once);
                }
                squares += 1;
            }
        }
    }
    assert!(independent_pairs > 0);
    assert!(squares > 0);
    println!(
        "net deltas {}, independent ordered pairs {independent_pairs}, applicable squares {squares}",
        deltas.len()
    );
}

#[test]
fn diff_table_applies_and_detects_equality_on_every_well_formed_carrier_pair() {
    let tables = carrier::tables();
    let mut count = 0usize;
    for before in &tables {
        for after in &tables {
            let delta = reference::diff_table(before, after);
            assert_eq!(
                reference::apply_table(&delta, before).as_ref(),
                Ok(after),
                "{before:?} -> {after:?}"
            );
            assert_eq!(delta == reference::TableDelta::identity(), before == after);

            let production =
                TableDelta::diff(&table_to_production(before), &table_to_production(after));
            let mut applied = table_to_production(before);
            production
                .apply(&mut applied)
                .expect("production diff applies");
            assert_eq!(table_to_reference(&applied), *after);
            assert_eq!(production.is_identity(), before == after);
            count += 1;
        }
    }
    assert_eq!(count, 105 * 105);
}

#[test]
fn diff_table_law_excludes_tables_whose_pairs_leave_their_edge_tables() {
    let malformed = reference_model::Table {
        rows: BTreeMap::from([(0, ("a".to_owned(), BTreeMap::new()))]),
        pairs: BTreeSet::from([(reference_model::SEdge::new("a", "b", "e", None), 0, 0)]),
    };
    let identity = reference::diff_table(&malformed, &malformed);
    assert_eq!(identity, reference::TableDelta::identity());
    assert!(matches!(
        reference::apply_table(&identity, &malformed),
        Err(reference::TableApplyError::MissingRowEndpoint { row: 0, .. })
    ));
}

#[test]
fn unit_associativity_and_anti_homomorphism_hold_on_every_atomic_triple() {
    let cases = carrier::graph_delta_cases();
    let identity = reference::GraphDelta::identity();
    for case in &cases {
        let delta = &case.delta;
        assert_eq!(
            reference::Delta::compose(&identity, delta).as_ref(),
            Ok(delta)
        );
        assert_eq!(
            reference::Delta::compose(delta, &identity).as_ref(),
            Ok(delta)
        );
        let production = delta_to_production(delta);
        assert_eq!(
            GraphDelta::identity()
                .compose(&production)
                .map(|net| delta_to_reference(&net))
                .as_ref(),
            Ok(delta)
        );
        assert_eq!(
            production
                .compose(&GraphDelta::identity())
                .map(|net| delta_to_reference(&net))
                .as_ref(),
            Ok(delta)
        );
        let regular = reference::Delta::compose(delta, &delta.invert())
            .and_then(|net| reference::Delta::compose(&net, delta))
            .expect("regularity composites are defined");
        assert_eq!(&regular, delta);
    }

    let mut defined_pairs = 0usize;
    for first in &cases {
        for second in &cases {
            let Ok(net) = reference::Delta::compose(&first.delta, &second.delta) else {
                continue;
            };
            defined_pairs += 1;
            let reversed = reference::Delta::compose(&second.delta.invert(), &first.delta.invert())
                .expect("the inverted pair composes in reverse order");
            assert_eq!(net.invert(), reversed);
            let production = delta_to_production(&first.delta)
                .compose(&delta_to_production(&second.delta))
                .expect("production composes the pair");
            let production_reversed = delta_to_production(&second.delta)
                .invert()
                .compose(&delta_to_production(&first.delta).invert())
                .expect("production composes the inverted pair");
            assert_eq!(production.invert(), production_reversed);
        }
    }
    assert!(defined_pairs > 0);

    let mut both_defined = 0usize;
    for a in &cases {
        for b in &cases {
            let ab = reference::Delta::compose(&a.delta, &b.delta);
            let production_ab =
                delta_to_production(&a.delta).compose(&delta_to_production(&b.delta));
            for c in &cases {
                let bc = reference::Delta::compose(&b.delta, &c.delta);
                let left = ab
                    .as_ref()
                    .ok()
                    .and_then(|ab| reference::Delta::compose(ab, &c.delta).ok());
                let right = bc
                    .as_ref()
                    .ok()
                    .and_then(|bc| reference::Delta::compose(&a.delta, bc).ok());
                let production_left = production_ab
                    .as_ref()
                    .ok()
                    .and_then(|ab| ab.compose(&delta_to_production(&c.delta)).ok());
                let production_right = delta_to_production(&b.delta)
                    .compose(&delta_to_production(&c.delta))
                    .ok()
                    .and_then(|bc| delta_to_production(&a.delta).compose(&bc).ok());
                if let (Some(left), Some(right)) = (&left, &right) {
                    assert_eq!(left, right, "{:?} {:?} {:?}", a.delta, b.delta, c.delta);
                    both_defined += 1;
                }
                if let (Some(left), Some(right)) = (&production_left, &production_right) {
                    assert_eq!(left, right);
                }
            }
        }
    }
    assert!(both_defined > 0);
    println!("defined pairs {defined_pairs}, triples with both bracketings defined {both_defined}");
}

/// A tree-edit refusal at the granularity both implementations share.
#[derive(Clone, Debug, PartialEq, Eq)]
enum JoinRefusalObs {
    NodeNotFound(u32),
    RootOperation(u32),
    InvalidJoin(u32),
    Misfiled { expected: u32, actual: u32 },
    Cycle { node: u32, parent: u32 },
}

fn reference_join_refusal(error: &reference::TreeApplyError) -> JoinRefusalObs {
    match error {
        reference::TreeApplyError::NodeNotFound(id) => JoinRefusalObs::NodeNotFound(*id),
        reference::TreeApplyError::RootOperation(id) => JoinRefusalObs::RootOperation(*id),
        reference::TreeApplyError::InvalidJoin(id) => JoinRefusalObs::InvalidJoin(*id),
        reference::TreeApplyError::MisfiledNode { expected, actual } => JoinRefusalObs::Misfiled {
            expected: *expected,
            actual: *actual,
        },
        reference::TreeApplyError::Cycle { node, parent } => JoinRefusalObs::Cycle {
            node: *node,
            parent: *parent,
        },
        other => panic!("unexpected reference join refusal: {other:?}"),
    }
}

fn production_join_refusal(error: &EditError) -> JoinRefusalObs {
    match error {
        EditError::NodeNotFound(id) => JoinRefusalObs::NodeNotFound(*id),
        EditError::RootOperation(id) => JoinRefusalObs::RootOperation(*id),
        EditError::InvalidJoin(id) => JoinRefusalObs::InvalidJoin(*id),
        EditError::MisfiledNode { expected, actual } => JoinRefusalObs::Misfiled {
            expected: *expected,
            actual: *actual,
        },
        EditError::CycleDetected {
            node_id,
            new_parent,
        } => JoinRefusalObs::Cycle {
            node: *node_id,
            parent: *new_parent,
        },
        other => panic!("unexpected production join refusal: {other:?}"),
    }
}

#[test]
fn tree_join_refusals_match_reference_and_leave_tree_and_embedding_unchanged() {
    let cases = carrier::join_cases();
    assert_eq!(
        u64::try_from(cases.len()).unwrap(),
        carrier::join_cases_count_certificate()
    );
    let expected_refusal = |name: &str| match name {
        "joined-ancestor" => JoinRefusalObs::Cycle { node: 1, parent: 3 },
        "missing-primary" | "missing-joined" => JoinRefusalObs::NodeNotFound(9),
        "joined-root" => JoinRefusalObs::RootOperation(0),
        "repeated-joined" => JoinRefusalObs::InvalidJoin(2),
        "joined-primary" => JoinRefusalObs::InvalidJoin(1),
        "misfiled-product" => JoinRefusalObs::Misfiled {
            expected: 1,
            actual: 2,
        },
        other => panic!("unexpected refusal case {other}"),
    };
    let mut refusals = 0usize;
    for case in cases.into_iter().filter(|case| !case.applies) {
        let (name, probe, edit) = (case.name, case.tree, case.edit);
        let expected = expected_refusal(name);
        refusals += 1;
        let (_, mut reference_embedding_state) =
            panproto_reference::encode::tree_to_graph(&probe, None).expect("probe tree embeds");
        let reference_before = reference_embedding_state.clone();
        let reference_error =
            reference::apply_tree(&edit, &probe, &mut reference_embedding_state).expect_err(name);
        assert_eq!(reference_join_refusal(&reference_error), expected, "{name}");
        assert_eq!(reference_embedding_state, reference_before, "{name}");
        let reference_lowering = reference::graph_delta_from_tree_edit(
            &probe,
            &mut reference_embedding_state,
            &edit,
            None,
        )
        .expect_err(name);
        assert_eq!(
            reference_join_refusal(&reference_lowering),
            expected,
            "{name}"
        );
        assert_eq!(reference_embedding_state, reference_before, "{name}");

        let production_edit = tree_edit_to_production(&edit);
        let production_tree = tree_to_production(&probe);
        let mut applied = production_tree.clone();
        let production_error = production_edit.apply(&mut applied).expect_err(name);
        assert_eq!(
            production_join_refusal(&production_error),
            expected,
            "{name}"
        );
        assert_eq!(tree_to_reference(&applied), probe, "{name}");
        let (production_graph, mut production_embedding_state) =
            GInstance::from_wtype_embedded(&production_tree, None).expect("probe tree embeds");
        let production_before = production_embedding_state.clone();
        let production_lowering = GraphDelta::from_tree_edit(
            &production_graph,
            &mut production_embedding_state,
            &production_edit,
        )
        .expect_err(name);
        assert_eq!(
            production_join_refusal(&production_lowering),
            expected,
            "{name}"
        );
        assert_eq!(production_embedding_state, production_before, "{name}");
    }
    assert_eq!(refusals, 7);
}

fn cells_to_production(cells: &BTreeMap<String, ReferenceValue>) -> HashMap<String, Value> {
    cells
        .iter()
        .map(|(column, value)| (column.clone(), value_to_production(value)))
        .collect()
}

fn table_edit_to_production(edit: &reference::TableEdit) -> panproto_inst::TableEdit {
    use panproto_inst::TableEdit as P;
    match edit {
        reference::TableEdit::Identity => P::Identity,
        reference::TableEdit::InsertRow { table, id, row } => P::InsertRow {
            table: table.as_str().into(),
            id: RowId(*id),
            row: cells_to_production(row),
        },
        reference::TableEdit::DeleteRow {
            table,
            id,
            row,
            incident,
        } => P::DeleteRow {
            table: table.as_str().into(),
            id: RowId(*id),
            row: cells_to_production(row),
            incident_fks: incident
                .iter()
                .map(|(edge, src, tgt)| (edge_to_production(edge), RowId(*src), RowId(*tgt)))
                .collect(),
        },
        reference::TableEdit::UpdateCell {
            table,
            id,
            column,
            old,
            new,
        } => P::UpdateCell {
            table: table.as_str().into(),
            id: RowId(*id),
            column: column.clone(),
            old: old.as_ref().map(value_to_production),
            new: new.as_ref().map(value_to_production),
        },
        reference::TableEdit::MoveRow {
            id,
            old_table,
            new_table,
        } => P::MoveRow {
            id: RowId(*id),
            old_table: old_table.as_str().into(),
            new_table: new_table.as_str().into(),
        },
        reference::TableEdit::AddForeignKey { edge, src, tgt } => P::AddForeignKey {
            edge: edge_to_production(edge),
            src: RowId(*src),
            tgt: RowId(*tgt),
        },
        reference::TableEdit::RemoveForeignKey { edge, src, tgt } => P::RemoveForeignKey {
            edge: edge_to_production(edge),
            src: RowId(*src),
            tgt: RowId(*tgt),
        },
        reference::TableEdit::Batch(steps) => {
            P::Batch(steps.iter().map(table_edit_to_production).collect())
        }
        reference::TableEdit::Delta(delta) => P::Delta(table_delta_to_production(delta)),
    }
}

/// Every single surface table step the carrier table admits or refuses:
/// insertions under a fresh and an occupied id, exact and stale deletions,
/// cell updates in both directions with a stale one, moves to another table
/// with a stale one, and every foreign-key addition and removal over the
/// rows and one fresh id.
fn surface_table_steps(table: &reference_model::Table) -> Vec<reference::TableEdit> {
    let edge = reference_model::SEdge::new("a", "a", "e", None);
    let fresh = u32::try_from(table.rows.len()).expect("small carrier");
    let zero = BTreeMap::from([("f".to_owned(), ReferenceValue::Int(0))]);
    let mut out = Vec::new();
    for id in [fresh, 0] {
        for row in [BTreeMap::new(), zero.clone()] {
            out.push(reference::TableEdit::InsertRow {
                table: "a".to_owned(),
                id,
                row,
            });
        }
    }
    for (id, (name, cells)) in &table.rows {
        let incident: Vec<_> = table
            .pairs
            .iter()
            .filter(|(_, src, tgt)| src == id || tgt == id)
            .cloned()
            .collect();
        let flipped = if cells.is_empty() {
            zero.clone()
        } else {
            BTreeMap::new()
        };
        let stale_incidence = if incident.is_empty() {
            vec![(edge.clone(), *id, *id)]
        } else {
            Vec::new()
        };
        for (table_name, row, listed) in [
            (name.clone(), cells.clone(), incident.clone()),
            ("b".to_owned(), cells.clone(), incident.clone()),
            (name.clone(), flipped, incident.clone()),
            (name.clone(), cells.clone(), stale_incidence),
        ] {
            out.push(reference::TableEdit::DeleteRow {
                table: table_name,
                id: *id,
                row,
                incident: listed,
            });
        }
        for (old, new) in [
            (None, Some(ReferenceValue::Int(0))),
            (Some(ReferenceValue::Int(0)), None),
        ] {
            out.push(reference::TableEdit::UpdateCell {
                table: name.clone(),
                id: *id,
                column: "f".to_owned(),
                old,
                new,
            });
        }
        out.push(reference::TableEdit::UpdateCell {
            table: "b".to_owned(),
            id: *id,
            column: "f".to_owned(),
            old: cells.get("f").cloned(),
            new: Some(ReferenceValue::Int(1)),
        });
        for old_table in [name.clone(), "b".to_owned()] {
            out.push(reference::TableEdit::MoveRow {
                id: *id,
                old_table,
                new_table: "b".to_owned(),
            });
        }
    }
    let ids: Vec<u32> = table.rows.keys().copied().chain([fresh]).collect();
    for src in &ids {
        for tgt in &ids {
            out.push(reference::TableEdit::AddForeignKey {
                edge: edge.clone(),
                src: *src,
                tgt: *tgt,
            });
            out.push(reference::TableEdit::RemoveForeignKey {
                edge: edge.clone(),
                src: *src,
                tgt: *tgt,
            });
        }
    }
    out
}

/// Compare the lowering of every two-step batch of `steps` and, when it
/// applies to `table`, its inversion in both implementations. Returns the
/// number of batches inverted.
fn check_surface_batches(
    table: &reference_model::Table,
    production_table: &FInstance,
    steps: &[reference::TableEdit],
) -> usize {
    let mut batches_inverted = 0usize;
    for first in steps {
        for second in steps {
            let batch = reference::TableEdit::Batch(vec![first.clone(), second.clone()]);
            let expected = reference::table_edit_delta(&batch);
            let production_batch = table_edit_to_production(&batch);
            match (expected, production_batch.to_delta()) {
                (Ok(expected), Ok(actual)) => {
                    assert_eq!(table_delta_to_reference(&actual), expected, "{batch:?}");
                    let Ok(after) = reference::apply_table(&expected, table) else {
                        continue;
                    };
                    assert_eq!(
                        reference::apply_table(&expected.invert(), &after).as_ref(),
                        Ok(table)
                    );
                    let mut production_after = production_table.clone();
                    actual
                        .apply(&mut production_after)
                        .expect("production applies the lowered batch");
                    production_batch
                        .invert()
                        .expect("a defined batch inverts")
                        .apply(&mut production_after)
                        .expect("production applies the inverted batch");
                    assert_eq!(table_to_reference(&production_after), *table);
                    batches_inverted += 1;
                }
                (Err(expected), Err(actual)) => assert_eq!(
                    production_table_compose_error(&actual),
                    reference_table_compose_error(&expected)
                ),
                (expected, actual) => panic!(
                    "batch lowering domain differs for {batch:?}: reference={expected:?}, production={actual:?}"
                ),
            }
        }
    }
    batches_inverted
}

#[test]
fn surface_table_edits_lower_check_and_invert_like_the_reference() {
    let mut steps_checked = 0usize;
    let mut applied = 0usize;
    let mut batches_inverted = 0usize;
    let mut outcomes = BTreeSet::new();
    for table in carrier::tables() {
        let production_table = table_to_production(&table);
        let steps = surface_table_steps(&table);
        for edit in &steps {
            let expected_delta = reference::table_edit_delta(edit).expect("a single step lowers");
            let production_edit = table_edit_to_production(edit);
            let actual_delta = production_edit.to_delta().expect("a single step lowers");
            assert_eq!(
                table_delta_to_reference(&actual_delta),
                expected_delta,
                "{edit:?}"
            );

            if let reference::TableEdit::DeleteRow {
                table: name,
                id,
                row,
                incident,
            } = edit
            {
                let checked = reference::delete_row_delta(
                    &table,
                    *id,
                    name,
                    row,
                    &incident.iter().cloned().collect(),
                );
                let stepped = reference::apply_table_step(edit, &table);
                match (&checked, &stepped) {
                    (Ok(delta), Ok(_)) => assert_eq!(delta, &expected_delta),
                    (Err(left), Err(right)) => assert_eq!(left, right),
                    _ => panic!("surface deletion checks differ: {checked:?} vs {stepped:?}"),
                }
            }
            let expected = reference::apply_table_step(edit, &table)
                .map_err(|error| reference_table_apply_error(&error));
            let mut actual_table = production_table.clone();
            let actual = production_edit
                .apply(&mut actual_table)
                .map(|()| table_to_reference(&actual_table))
                .map_err(|error| production_table_apply_error(&error));
            assert_eq!(actual, expected, "table={table:?}; edit={edit:?}");
            outcomes.insert(format!(
                "{:?}",
                expected
                    .as_ref()
                    .map(|_| ())
                    .map_err(|error| format!("{error:?}")
                        .split('(')
                        .next()
                        .unwrap_or_default()
                        .to_owned())
            ));
            steps_checked += 1;
            let Ok(after) = expected else {
                assert_eq!(table_to_reference(&actual_table), table, "{edit:?}");
                continue;
            };
            assert_eq!(
                reference::apply_table(&expected_delta.invert(), &after).as_ref(),
                Ok(&table),
                "reference inversion: {edit:?}"
            );
            production_edit
                .invert()
                .expect("a single step inverts")
                .apply(&mut actual_table)
                .unwrap_or_else(|error| panic!("production inversion {edit:?}: {error:?}"));
            assert_eq!(table_to_reference(&actual_table), table, "{edit:?}");
            applied += 1;
        }
        batches_inverted += check_surface_batches(&table, &production_table, &steps);
    }
    println!(
        "single steps {steps_checked}, applied {applied}, inverted batches {batches_inverted}, outcomes {outcomes:?}"
    );
    assert!(applied > 0 && batches_inverted > 0);
    for refusal in [
        "StaleIncidence",
        "StaleTable",
        "StaleCell",
        "DuplicateRow",
        "MissingRowEndpoint",
        "ForeignKeyNotFound",
        "DuplicateForeignKey",
    ] {
        assert!(
            outcomes.iter().any(|outcome| outcome.contains(refusal)),
            "{refusal} not reached: {outcomes:?}"
        );
    }
}

/// Words checked, by exact length, with the initial states they start from
/// and the outcome census of their last letters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CoverageTally {
    states: u64,
    words: [u64; 4],
    refusals: BTreeMap<String, u64>,
    replacements: u64,
    commuting: u64,
}

impl CoverageTally {
    fn absorb(&mut self, other: Self) {
        self.states += other.states;
        for (total, part) in self.words.iter_mut().zip(other.words) {
            *total += part;
        }
        for (name, count) in other.refusals {
            *self.refusals.entry(name).or_default() += count;
        }
        self.replacements += other.replacements;
        self.commuting += other.commuting;
    }

    fn refuse(&mut self, refusal: &impl std::fmt::Debug) {
        let name = format!("{refusal:?}");
        let variant = name
            .split(['(', ' ', '{'])
            .next()
            .unwrap_or_default()
            .to_owned();
        *self.refusals.entry(variant).or_default() += 1;
    }
}

/// A deterministic partition of the ordered source-state inventory.
#[derive(Clone, Copy, Debug)]
struct CoverageShard {
    index: usize,
    count: usize,
}

impl CoverageShard {
    fn from_env(total_states: usize) -> Self {
        let shard = match std::env::var("PANPROTO_EDIT_COVERAGE_SHARD") {
            Ok(value) => {
                let (index, count) = value.split_once('/').expect("edit shard is index/count");
                Self {
                    index: index
                        .parse()
                        .expect("edit shard index is an unsigned integer"),
                    count: count
                        .parse()
                        .expect("edit shard count is an unsigned integer"),
                }
            }
            Err(std::env::VarError::NotPresent) => Self { index: 0, count: 1 },
            Err(error) => panic!("invalid edit shard environment: {error}"),
        };
        assert!(shard.count > 0 && shard.count <= total_states);
        assert!(shard.index < shard.count);
        shard
    }

    const fn contains(self, state: usize) -> bool {
        state % self.count == self.index
    }
}

struct CoverageRun {
    tally: CoverageTally,
    shard: CoverageShard,
    total_states: usize,
}

/// Run every selected source state completely, partitioning by its global
/// enumeration index. No word or outcome assertion inside `work` is sampled.
fn coverage_parallel<S: Sync + std::fmt::Debug>(
    jobs: &[(usize, S)],
    work: impl Fn(&carrier::CoverageSchema, &S, &mut CoverageTally) + Sync,
) -> CoverageRun {
    let shard = CoverageShard::from_env(jobs.len());
    let schemas = carrier::coverage_schemas();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(1, |n| usize::from(n).min(4));
    let (tally, mut completed_state_indices) = std::thread::scope(|threads| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                threads.spawn(|| {
                    let mut tally = CoverageTally::default();
                    let mut completed = Vec::new();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((schema, state)) = jobs.get(index) else {
                            break;
                        };
                        if !shard.contains(index) {
                            continue;
                        }
                        tally.states += 1;
                        work(&schemas[*schema], state, &mut tally);
                        completed.push(index);
                    }
                    (tally, completed)
                })
            })
            .collect();
        let mut total = CoverageTally::default();
        let mut completed = Vec::new();
        for handle in handles {
            let (tally, indices) = handle.join().expect("worker completes");
            total.absorb(tally);
            completed.extend(indices);
        }
        (total, completed)
    });
    completed_state_indices.sort_unstable();
    let expected: Vec<_> = (0..jobs.len())
        .filter(|index| shard.contains(*index))
        .collect();
    assert_eq!(
        completed_state_indices, expected,
        "every selected state finishes exactly once"
    );
    assert_eq!(
        tally.states,
        u64::try_from(expected.len()).expect("state count fits")
    );
    CoverageRun {
        tally,
        shard,
        total_states: jobs.len(),
    }
}

fn coverage_jobs<S>(states: impl Fn(&carrier::CoverageSchema) -> Vec<S>) -> Vec<(usize, S)> {
    carrier::coverage_schemas()
        .iter()
        .enumerate()
        .flat_map(|(index, schema)| states(schema).into_iter().map(move |state| (index, state)))
        .collect()
}

fn assert_coverage_counts(tally: &CoverageTally, certificate: carrier::CoverageCounts) {
    assert_eq!(tally.states, certificate.states, "initial states");
    assert_eq!(tally.words, certificate.words, "words by exact length");
    assert_eq!(tally.words[0], tally.states);
}

/// The prefix states of the graph word under test, by length.
struct GraphPrefix {
    reference: reference_model::Graph,
    production: GInstance,
    composite: reference::GraphDelta,
    production_composite: GraphDelta,
}

/// Compare the composite of a graph word after its last letter applied:
/// composition, inversion and canonical bytes against the reference, and
/// the composite and its inverse applied from the initial state. Returns
/// both composites.
fn check_graph_composite(
    prefixes: &[GraphPrefix],
    letter: &reference::GraphDelta,
    production_letter: &GraphDelta,
    letters: &[reference::GraphDelta],
    production_state: &GInstance,
    tally: &mut CoverageTally,
) -> (reference::GraphDelta, GraphDelta) {
    let before = &prefixes[letters.len() - 1];
    let composite = reference::Delta::compose(&before.composite, letter)
        .expect("sequentially applicable letters compose");
    let production_composite = before
        .production_composite
        .compose(production_letter)
        .expect("production composes sequentially applicable letters");
    assert_eq!(
        delta_to_reference(&production_composite),
        composite,
        "compose {letters:?}"
    );
    assert_eq!(
        delta_to_reference(&production_composite.invert()),
        reference::Delta::invert(&composite),
        "invert {letters:?}"
    );
    let expected_bytes = reference::canonical_graph_delta_bytes(&composite)
        .expect("a scope composite has canonical bytes");
    let mut writer = CanonicalWriter::new();
    canonical_graph_delta_bytes(&production_composite, &mut writer);
    assert_eq!(writer.into_bytes(), expected_bytes, "bytes {letters:?}");

    let start = &prefixes[0];
    let mut net = start.production.clone();
    production_composite
        .apply(&mut net)
        .unwrap_or_else(|error| panic!("composite of {letters:?}: {error:?}"));
    assert!(
        GInstance::identical(&net, production_state),
        "composite {letters:?}"
    );
    production_composite
        .invert()
        .apply(&mut net)
        .unwrap_or_else(|error| panic!("inverse of {letters:?}: {error:?}"));
    assert!(
        GInstance::identical(&net, &start.production),
        "inverse {letters:?}"
    );
    if composite.del.keys().any(|x| composite.add.contains_key(x)) {
        assert!(!production_composite.is_identity());
        tally.replacements += 1;
    }
    (composite, production_composite)
}

#[test]
fn graph_edit_algebra_matches_reference_on_every_word_of_the_coverage_scope() {
    let jobs = coverage_jobs(carrier::coverage_graphs);
    let run = coverage_parallel(&jobs, |schema, graph, tally| {
        let mut prefixes: Vec<GraphPrefix> = Vec::with_capacity(4);
        carrier::coverage_graph_words(schema, graph, &mut |word| {
            let length = word.letters.len();
            tally.words[length] += 1;
            let Some(letter) = word.letters.last() else {
                prefixes.clear();
                prefixes.push(GraphPrefix {
                    reference: graph.clone(),
                    production: graph_to_production(graph),
                    composite: reference::GraphDelta::identity(),
                    production_composite: GraphDelta::identity(),
                });
                return;
            };
            prefixes.truncate(length);
            let before = &prefixes[length - 1];
            let production_letter = delta_to_production(letter);
            let mut production_state = before.production.clone();
            let actual = production_letter
                .apply(&mut production_state)
                .map(|()| graph_to_reference(&production_state))
                .map_err(|error| production_apply_error(&error));
            let expected = word
                .outcome
                .clone()
                .map_err(|error| reference_apply_error(&error));
            assert_eq!(
                actual, expected,
                "apply {letter:?} to {:?}",
                before.reference
            );
            let Ok(after) = word.outcome else {
                assert_eq!(
                    graph_to_reference(&production_state),
                    before.reference,
                    "a refusal leaves the graph unchanged"
                );
                if let Err(error) = &expected {
                    tally.refuse(error);
                }
                return;
            };

            let (composite, production_composite) = check_graph_composite(
                &prefixes,
                letter,
                &production_letter,
                word.letters,
                &production_state,
                tally,
            );
            prefixes.push(GraphPrefix {
                reference: after.clone(),
                production: production_state,
                composite,
                production_composite,
            });
        });
    });
    let tally = &run.tally;
    println!("graph scope: {tally:?}");
    assert_eq!(
        u64::try_from(run.total_states).unwrap(),
        carrier::coverage_graph_counts_certificate().states
    );
    assert_eq!(tally.words[0], tally.states);
    if run.shard.count == 1 {
        assert_coverage_counts(tally, carrier::coverage_graph_counts_certificate());
        assert_eq!(tally.refusals.keys().collect::<Vec<_>>(), ["DanglingEdges"]);
        assert!(tally.replacements > 0);
    }
}

#[test]
fn graph_independence_and_merge_match_reference_on_every_letter_pair_of_the_coverage_scope() {
    let jobs = coverage_jobs(carrier::coverage_graphs);
    let run = coverage_parallel(&jobs, |schema, graph, tally| {
        let letters = carrier::coverage_graph_letters(
            schema,
            graph,
            carrier::CoverageFloors::of_graph(graph),
        );
        let production_graph = graph_to_production(graph);
        let production: Vec<GraphDelta> = letters.iter().map(delta_to_production).collect();
        let applied: Vec<Option<GInstance>> = production
            .iter()
            .map(|delta| {
                let mut state = production_graph.clone();
                delta.apply(&mut state).ok().map(|()| state)
            })
            .collect();
        for (first, (first_production, first_applied)) in
            letters.iter().zip(production.iter().zip(&applied))
        {
            for (second, (second_production, second_applied)) in
                letters.iter().zip(production.iter().zip(&applied))
            {
                tally.words[2] += 1;
                let expected: Vec<_> = reference::independence(first, second)
                    .iter()
                    .map(reference_conflict)
                    .collect();
                let actual: Vec<_> = first_production
                    .independence(second_production)
                    .iter()
                    .map(production_conflict)
                    .collect();
                assert_eq!(actual, expected, "independence {first:?} vs {second:?}");
                let merged = match (
                    reference::merge(first, second),
                    first_production.merge(second_production),
                ) {
                    (Ok(expected), Ok(actual)) => {
                        assert_eq!(delta_to_reference(&actual), expected);
                        actual
                    }
                    (Err(expected), Err(actual)) => {
                        assert_eq!(production_conflict(&actual), reference_conflict(&expected));
                        tally.refuse(&reference_conflict(&expected));
                        continue;
                    }
                    (expected, actual) => panic!(
                        "merge domain differs for {first:?} and {second:?}: reference={expected:?}, production={actual:?}"
                    ),
                };
                let (Some(first_applied), Some(second_applied)) = (first_applied, second_applied)
                else {
                    continue;
                };
                let mut one_then_two = first_applied.clone();
                second_production
                    .apply(&mut one_then_two)
                    .expect("an independent second delta applies after the first");
                let mut two_then_one = second_applied.clone();
                first_production
                    .apply(&mut two_then_one)
                    .expect("an independent first delta applies after the second");
                let mut both = production_graph.clone();
                merged.apply(&mut both).expect("the merge applies");
                let expected =
                    reference::apply(&reference::merge(first, second).expect("merged"), graph)
                        .expect("the reference merge applies");
                assert_eq!(graph_to_reference(&one_then_two), expected);
                assert_eq!(graph_to_reference(&two_then_one), expected);
                assert_eq!(graph_to_reference(&both), expected);
                tally.commuting += 1;
            }
        }
    });
    let tally = &run.tally;
    println!("graph pairs: {tally:?}");
    assert_eq!(
        u64::try_from(run.total_states).unwrap(),
        carrier::coverage_graph_counts_certificate().states
    );
    if run.shard.count == 1 {
        assert_eq!(
            tally.states,
            carrier::coverage_graph_counts_certificate().states
        );
        assert!(tally.commuting > 0 && !tally.refusals.is_empty());
    }
}

/// The prefix states of the table word under test, by length.
struct TablePrefix {
    reference: reference_model::Table,
    production: FInstance,
    composite: reference::TableDelta,
    production_composite: TableDelta,
}

/// Compare the composite of a table word after its last letter applied:
/// composition and canonical bytes against the reference, and the
/// composite and its inverse applied from the initial state in both
/// implementations. Returns both composites.
fn check_table_composite(
    prefixes: &[TablePrefix],
    lowered: &reference::TableDelta,
    production_lowered: &TableDelta,
    letters: &[reference::TableEdit],
    after: &reference_model::Table,
    tally: &mut CoverageTally,
) -> (reference::TableDelta, TableDelta) {
    let before = &prefixes[letters.len() - 1];
    let composite =
        reference::compose_table(&before.composite, lowered).expect("applicable letters compose");
    let production_composite = before
        .production_composite
        .compose(production_lowered)
        .expect("production composes applicable letters");
    assert_eq!(
        table_delta_to_reference(&production_composite),
        composite,
        "compose {letters:?}"
    );
    let expected_bytes = reference::canonical_table_delta_bytes(&composite)
        .expect("a scope composite has canonical bytes");
    let mut writer = CanonicalWriter::new();
    canonical_table_delta_bytes(&production_composite, &mut writer);
    assert_eq!(writer.into_bytes(), expected_bytes, "bytes {letters:?}");
    assert_eq!(
        reference::apply_table(&composite.invert(), after).as_ref(),
        Ok(&prefixes[0].reference),
        "reference inverse {letters:?}"
    );

    let mut net = prefixes[0].production.clone();
    production_composite
        .apply(&mut net)
        .unwrap_or_else(|error| panic!("composite of {letters:?}: {error:?}"));
    assert_eq!(table_to_reference(&net), *after, "composite {letters:?}");
    production_composite
        .invert()
        .apply(&mut net)
        .unwrap_or_else(|error| panic!("inverse of {letters:?}: {error:?}"));
    assert_eq!(
        table_to_reference(&net),
        prefixes[0].reference,
        "inverse {letters:?}"
    );
    if composite.del.keys().any(|x| composite.add.contains_key(x)) {
        tally.replacements += 1;
    }
    (composite, production_composite)
}

#[test]
fn table_edits_match_reference_on_every_word_of_the_coverage_scope() {
    let jobs = coverage_jobs(carrier::coverage_tables);
    let run = coverage_parallel(&jobs, |schema, table, tally| {
        let mut prefixes: Vec<TablePrefix> = Vec::with_capacity(4);
        carrier::coverage_table_words(schema, table, &mut |word| {
            let length = word.letters.len();
            tally.words[length] += 1;
            let Some(letter) = word.letters.last() else {
                prefixes.clear();
                prefixes.push(TablePrefix {
                    reference: table.clone(),
                    production: table_to_production(table),
                    composite: reference::TableDelta::identity(),
                    production_composite: TableDelta::identity(),
                });
                return;
            };
            prefixes.truncate(length);
            let before = &prefixes[length - 1];
            let production_letter = table_edit_to_production(letter);
            let lowered = reference::table_edit_delta(letter).expect("a single step lowers");
            let production_lowered = production_letter.to_delta().expect("a single step lowers");
            assert_eq!(
                table_delta_to_reference(&production_lowered),
                lowered,
                "lower {letter:?}"
            );
            let mut production_state = before.production.clone();
            let actual = production_letter
                .apply(&mut production_state)
                .map(|()| table_to_reference(&production_state))
                .map_err(|error| production_table_apply_error(&error));
            let expected = word
                .outcome
                .clone()
                .map_err(|error| reference_table_apply_error(&error));
            assert_eq!(
                actual, expected,
                "apply {letter:?} to {:?}",
                before.reference
            );
            let Ok(after) = word.outcome else {
                assert_eq!(
                    table_to_reference(&production_state),
                    before.reference,
                    "a refusal leaves the table unchanged"
                );
                if let Err(error) = &expected {
                    tally.refuse(error);
                }
                return;
            };

            let (composite, production_composite) = check_table_composite(
                &prefixes,
                &lowered,
                &production_lowered,
                word.letters,
                after,
                tally,
            );
            prefixes.push(TablePrefix {
                reference: after.clone(),
                production: production_state,
                composite,
                production_composite,
            });
        });
    });
    let tally = &run.tally;
    println!("table scope: {tally:?}");
    assert_eq!(
        u64::try_from(run.total_states).unwrap(),
        carrier::coverage_table_counts_certificate().states
    );
    assert_eq!(tally.words[0], tally.states);
    if run.shard.count == 1 {
        assert_coverage_counts(tally, carrier::coverage_table_counts_certificate());
        assert_eq!(
            tally.refusals.keys().collect::<Vec<_>>(),
            ["DuplicateForeignKey"]
        );
        assert!(tally.replacements > 0);
    }
}

/// A tree-edit refusal, read exactly from either implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TreeRefusalObs {
    RootOperation(u32),
    NotLeaf(u32),
    Cycle { node: u32, parent: u32 },
}

fn reference_tree_refusal(error: &reference::TreeApplyError) -> TreeRefusalObs {
    match error {
        reference::TreeApplyError::RootOperation(id) => TreeRefusalObs::RootOperation(*id),
        reference::TreeApplyError::NotLeaf(id) => TreeRefusalObs::NotLeaf(*id),
        reference::TreeApplyError::Cycle { node, parent } => TreeRefusalObs::Cycle {
            node: *node,
            parent: *parent,
        },
        other => panic!("refusal outside the coverage scope's alphabet: {other:?}"),
    }
}

fn production_tree_refusal(error: &EditError) -> TreeRefusalObs {
    match error {
        EditError::RootOperation(id) => TreeRefusalObs::RootOperation(*id),
        EditError::NotLeaf(id) => TreeRefusalObs::NotLeaf(*id),
        EditError::CycleDetected {
            node_id,
            new_parent,
        } => TreeRefusalObs::Cycle {
            node: *node_id,
            parent: *new_parent,
        },
        other => panic!("production refusal outside the scope's alphabet: {other:?}"),
    }
}

/// The initial tree of a word and its graph forms in both implementations.
struct TreeStart {
    reference_embedding: panproto_reference::encode::TreeEmbedding,
    reference_graph: reference_model::Graph,
    production_embedding: TreeEmbedding,
    production_graph: GInstance,
}

/// The lowering of a whole tree word as one sequence, from its initial
/// tree, in both implementations, with the embeddings each reaches.
struct TreeWordLowering {
    sequence: reference::TreeEdit,
    production_sequence: TreeEdit,
    lowered: Result<reference::GraphDelta, reference::TreeApplyError>,
    lowered_embedding: panproto_reference::encode::TreeEmbedding,
    production_lowered: Result<GraphDelta, EditError>,
    production_lowered_embedding: TreeEmbedding,
}

fn lower_tree_word(
    start: &TreeStart,
    tree: &reference_model::Tree,
    letters: &[reference::TreeEdit],
) -> TreeWordLowering {
    let sequence = reference::TreeEdit::Sequence(letters.to_vec());
    let production_sequence = tree_edit_to_production(&sequence);
    let mut lowered_embedding = start.reference_embedding.clone();
    let lowered =
        reference::graph_delta_from_tree_edit(tree, &mut lowered_embedding, &sequence, None);
    let mut production_lowered_embedding = start.production_embedding.clone();
    let production_lowered = GraphDelta::from_tree_edit(
        &start.production_graph,
        &mut production_lowered_embedding,
        &production_sequence,
    );
    TreeWordLowering {
        sequence,
        production_sequence,
        lowered,
        lowered_embedding,
        production_lowered,
        production_lowered_embedding,
    }
}

impl TreeWordLowering {
    /// A refused word: both lowerings refuse with `expected` and leave
    /// their embeddings as they were.
    fn assert_refused(
        self,
        start: &TreeStart,
        expected: &TreeRefusalObs,
        letters: &[reference::TreeEdit],
    ) {
        assert_eq!(
            reference_tree_refusal(&self.lowered.expect_err("lowering refuses")),
            *expected
        );
        assert_eq!(self.lowered_embedding, start.reference_embedding);
        let actual = self
            .production_lowered
            .expect_err("production lowering refuses");
        assert_eq!(production_tree_refusal(&actual), *expected, "{letters:?}");
        assert_eq!(
            self.production_lowered_embedding,
            start.production_embedding
        );
    }
}

/// An applied word: equal lowerings, embeddings, applied graphs and
/// canonical bytes. Returns whether the lowering holds a replacement.
fn check_applied_tree_word(
    lowering: TreeWordLowering,
    start: &TreeStart,
    letters: &[reference::TreeEdit],
) -> bool {
    let TreeWordLowering {
        sequence,
        production_sequence,
        lowered,
        lowered_embedding,
        production_lowered,
        production_lowered_embedding,
    } = lowering;
    let lowered =
        lowered.unwrap_or_else(|error| panic!("reference lowering {letters:?}: {error:?}"));
    let production_lowered = production_lowered
        .unwrap_or_else(|error| panic!("production lowering {letters:?}: {error:?}"));
    assert_eq!(
        delta_to_reference(&production_lowered),
        lowered,
        "lower {letters:?}"
    );
    assert_eq!(
        production_embedding(&production_lowered_embedding),
        reference_embedding(&lowered_embedding),
        "embedding after {letters:?}"
    );
    let reference_graph = reference::apply(&lowered, &start.reference_graph)
        .unwrap_or_else(|error| panic!("reference graph apply {letters:?}: {error:?}"));
    let mut production_graph = start.production_graph.clone();
    production_lowered
        .apply(&mut production_graph)
        .unwrap_or_else(|error| panic!("production graph apply {letters:?}: {error:?}"));
    assert_eq!(graph_to_reference(&production_graph), reference_graph);
    let expected_bytes =
        reference::canonical_tree_edit_bytes(&sequence).expect("a scope word is well filed");
    let mut writer = CanonicalWriter::new();
    canonical_tree_edit_bytes(&production_sequence, &mut writer);
    assert_eq!(writer.into_bytes(), expected_bytes, "bytes {letters:?}");
    lowered.del.keys().any(|x| lowered.add.contains_key(x))
}

#[test]
fn tree_edits_and_lowering_match_reference_on_every_word_of_the_coverage_scope() {
    let jobs = coverage_jobs(carrier::coverage_trees);
    let run = coverage_parallel(&jobs, |schema, tree, tally| {
        let (start_graph, start_embedding) = panproto_reference::encode::tree_to_graph(tree, None)
            .expect("a scope tree has a graph form");
        let production_tree = tree_to_production(tree);
        let (production_start_graph, production_start_embedding) =
            GInstance::from_wtype_embedded(&production_tree, None)
                .expect("a scope tree has a production graph form");
        assert_eq!(graph_to_reference(&production_start_graph), start_graph);
        assert_eq!(
            production_embedding(&production_start_embedding),
            reference_embedding(&start_embedding)
        );
        let start = TreeStart {
            reference_embedding: start_embedding,
            reference_graph: start_graph,
            production_embedding: production_start_embedding,
            production_graph: production_start_graph,
        };
        let mut prefixes: Vec<WInstance> = Vec::with_capacity(4);
        carrier::coverage_tree_words(schema, tree, &mut |word| {
            let length = word.letters.len();
            tally.words[length] += 1;
            let Some(letter) = word.letters.last() else {
                prefixes.clear();
                prefixes.push(production_tree.clone());
                return;
            };
            prefixes.truncate(length);
            let mut production_state = prefixes[length - 1].clone();
            let applied = tree_edit_to_production(letter).apply(&mut production_state);

            let lowering = lower_tree_word(&start, tree, word.letters);
            let after = match word.outcome {
                Err(error) => {
                    let expected = reference_tree_refusal(error);
                    let actual = applied.expect_err("production refuses with the reference");
                    assert_eq!(production_tree_refusal(&actual), expected, "{letter:?}");
                    assert_eq!(
                        tree_to_reference(&production_state),
                        tree_to_reference(&prefixes[length - 1])
                    );
                    lowering.assert_refused(&start, &expected, word.letters);
                    tally.refuse(&expected);
                    return;
                }
                Ok(after) => after,
            };
            applied.unwrap_or_else(|error| panic!("production apply {letter:?}: {error:?}"));
            assert_eq!(
                tree_to_reference(&production_state),
                *after,
                "apply {letter:?}"
            );
            if check_applied_tree_word(lowering, &start, word.letters) {
                tally.replacements += 1;
            }
            prefixes.push(production_state);
        })
        .expect("a scope tree has a graph form");
    });
    let tally = &run.tally;
    println!("tree scope: {tally:?}");
    assert_eq!(
        u64::try_from(run.total_states).unwrap(),
        carrier::coverage_tree_counts_certificate().states
    );
    assert_eq!(tally.words[0], tally.states);
    if run.shard.count == 1 {
        assert_coverage_counts(tally, carrier::coverage_tree_counts_certificate());
        assert_eq!(
            tally.refusals.keys().collect::<Vec<_>>(),
            ["Cycle", "NotLeaf", "RootOperation"]
        );
        assert!(tally.replacements > 0);
    }
}

/// Every move of the root, under each node including itself, is a root
/// operation in both implementations, before any cycle check; in both, tree
/// apply and graph lowering refuse alike and leave the tree and embedding
/// unchanged.
#[test]
fn moving_the_root_is_a_root_operation_in_production_and_the_reference() {
    let edge = reference_model::SEdge::new("a", "a", "e", None);
    let mut tree = reference_model::Tree {
        nodes: (0..3)
            .map(|id| (id, reference_model::Node::new(id, "a")))
            .collect(),
        arcs: vec![(0, 1, edge.clone()), (1, 2, edge.clone())],
        fans: Vec::new(),
        root: 0,
    };
    tree.nodes
        .get_mut(&2)
        .expect("leaf")
        .fields
        .insert("f".to_owned(), ReferenceValue::Int(1));
    let mut checked = 0usize;
    for new_parent in 0..3 {
        let letter = reference::TreeEdit::MoveSubtree {
            node_id: 0,
            new_parent,
            edge: edge.clone(),
        };
        let (_, mut reference_embedding_state) =
            panproto_reference::encode::tree_to_graph(&tree, None).expect("tree embeds");
        let before = reference_embedding_state.clone();
        let expected = reference::apply_tree(&letter, &tree, &mut reference_embedding_state)
            .expect_err("a root move refuses");
        assert_eq!(
            reference_tree_refusal(&expected),
            TreeRefusalObs::RootOperation(0)
        );
        assert_eq!(reference_embedding_state, before);

        let production_letter = tree_edit_to_production(&letter);
        let production_tree = tree_to_production(&tree);
        let mut applied = production_tree.clone();
        let actual = production_letter
            .apply(&mut applied)
            .expect_err("production refuses a root move");
        assert_eq!(
            production_tree_refusal(&actual),
            TreeRefusalObs::RootOperation(0),
            "{letter:?}"
        );
        assert_eq!(tree_to_reference(&applied), tree);
        let (graph, mut embedding) =
            GInstance::from_wtype_embedded(&production_tree, None).expect("tree embeds");
        let production_before = embedding.clone();
        let lowered = GraphDelta::from_tree_edit(&graph, &mut embedding, &production_letter)
            .expect_err("production lowering refuses a root move");
        assert_eq!(
            production_tree_refusal(&lowered),
            TreeRefusalObs::RootOperation(0)
        );
        assert_eq!(embedding, production_before);
        checked += 1;
    }
    assert_eq!(checked, 3);

    // Contracting the root is the same root operation in both.
    let contract = reference::TreeEdit::ContractNode { id: 0 };
    let (_, mut reference_embedding_state) =
        panproto_reference::encode::tree_to_graph(&tree, None).expect("tree embeds");
    let expected = reference::apply_tree(&contract, &tree, &mut reference_embedding_state)
        .expect_err("contracting the root refuses");
    let mut applied = tree_to_production(&tree);
    let actual = tree_edit_to_production(&contract)
        .apply(&mut applied)
        .expect_err("production refuses contracting the root");
    assert_eq!(
        production_tree_refusal(&actual),
        reference_tree_refusal(&expected)
    );
    assert_eq!(tree_to_reference(&applied), tree);
}

/// The reader soundness case the reference pins: inserting a leaf over the
/// first fan's node id relocates both fans, and the same delta with the
/// first relocated fan's parent edge renamed is no lowering of that
/// insertion. Production's reader must read the lowering as an edit that
/// lowers to it and refuse the renamed delta, as the reference's does.
#[test]
fn production_reader_refuses_a_relocated_fan_with_a_renamed_parent_edge() {
    let fan = |parent: u32, member: u32| reference_model::Fan {
        parent,
        hyper_edge: "fan".to_owned(),
        children: BTreeMap::from([("arm".to_owned(), member)]),
    };
    let mut tree = tree(
        &[
            (0, "root"),
            (1, "feature"),
            (2, "feature"),
            (3, "leaf"),
            (4, "leaf"),
        ],
        &[(0, 1, "a"), (0, 2, "b"), (2, 3, "c"), (2, 4, "d")],
    );
    tree.fans = vec![fan(2, 3), fan(2, 3)];
    let (_, embedding) =
        panproto_reference::encode::tree_to_graph(&tree, None).expect("tree embeds");
    let first_fan = embedding.fans[0].0;
    let insert = reference::TreeEdit::InsertNode {
        parent: 0,
        child_id: first_fan,
        node: reference_model::Node::new(first_fan, "leaf"),
        edge: reference_model::SEdge::new("root", "leaf", "child", Some("inserted")),
    };
    let mut lowered_embedding = embedding.clone();
    let delta = reference::graph_delta_from_tree_edit(&tree, &mut lowered_embedding, &insert, None)
        .expect("the insertion lowers");
    let mut perturbed = delta.clone();
    let (parent_edge, mut renamed) = perturbed
        .add
        .iter()
        .find_map(|(element, payload)| match payload {
            reference::GPayload::Edge(edge) if edge.label.kind == "hyper_parent" => {
                Some((element.clone(), edge.clone()))
            }
            _ => None,
        })
        .expect("a relocated fan parent edge");
    renamed.label.name = Some("perturbed".to_owned());
    perturbed
        .add
        .insert(parent_edge, reference::GPayload::Edge(renamed));
    assert!(reference::tree_edit_from_graph_delta(&tree, &embedding, &perturbed, None).is_err());

    let production_tree = tree_to_production(&tree);
    let (_, production_embedding) =
        GInstance::from_wtype_embedded(&production_tree, None).expect("tree embeds");
    let read = TreeEdit::from_graph_delta(
        &production_tree,
        &production_embedding,
        &delta_to_production(&delta),
    )
    .expect("production reads the lowering");
    let (graph, mut relowered_embedding) =
        GInstance::from_wtype_embedded(&production_tree, None).expect("tree embeds");
    let relowered = GraphDelta::from_tree_edit(&graph, &mut relowered_embedding, &read)
        .expect("the reading lowers");
    assert_eq!(delta_to_reference(&relowered), delta, "{read:?}");
    let refused = TreeEdit::from_graph_delta(
        &production_tree,
        &production_embedding,
        &delta_to_production(&perturbed),
    );
    assert!(
        matches!(refused, Err(EditError::NotTreeEdit { .. })),
        "{refused:?}"
    );
}

/// The inverse of deleting a leaf re-adds its arc under its old id, below
/// the edge floor. A tree edit draws every fresh arc id from the floor
/// (L-edits-3), so no tree edit lowers to that delta: the reference reader
/// refuses it as outside the tree-edit image, and production's reader
/// refuses it too, although inserting the leaf has its effect.
#[test]
fn inverse_reusing_an_arc_id_below_the_floor_is_no_tree_edit() {
    let edge = |tgt: &str| reference_model::SEdge::new("root", tgt, "prop", Some(tgt));
    let leaf = |id: u32, anchor: &str| {
        reference_model::Node::new(id, anchor).with_value(ReferenceValue::Int(0))
    };
    let tree = reference_model::Tree {
        nodes: BTreeMap::from([
            (0, reference_model::Node::new(0, "root")),
            (1, leaf(1, "visible")),
        ]),
        arcs: vec![(0, 1, edge("visible"))],
        fans: Vec::new(),
        root: 0,
    };
    let (_, mut embedding) =
        panproto_reference::encode::tree_to_graph(&tree, None).expect("tree embeds");
    assert_eq!(embedding.arcs, BTreeMap::from([(1, 0)]));
    // Node 2 and its arc 1 were there and were deleted.
    embedding.next_node = 3;
    embedding.next_edge = 2;
    let mut delta = reference::GraphDelta::identity();
    delta.add.insert(
        reference::GElem::Node(2),
        reference::GPayload::Node(leaf(2, "hidden")),
    );
    delta.add.insert(
        reference::GElem::Edge(1),
        reference::GPayload::Edge(reference_model::GEdge {
            src: 0,
            tgt: 2,
            label: edge("hidden"),
            position: None,
        }),
    );
    assert!(matches!(
        reference::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        Err(reference::TreeApplyError::NotTreeEdit { .. })
    ));

    let production_tree = tree_to_production(&tree);
    let (_, mut production_embedding) =
        GInstance::from_wtype_embedded(&production_tree, None).expect("tree embeds");
    production_embedding.next_node = 3;
    production_embedding.next_edge = 2;
    let refused = TreeEdit::from_graph_delta(
        &production_tree,
        &production_embedding,
        &delta_to_production(&delta),
    );
    assert!(
        matches!(refused, Err(EditError::NotTreeEdit { .. })),
        "{refused:?}"
    );
}

fn generated_edit_schema() -> carrier::CoverageSchema {
    carrier::CoverageSchema {
        vertices: vec!["a".into()],
        edges: vec![reference_model::SEdge::new("a", "a", "e", None)],
    }
}

fn generated_edit_graph(values: &[i64], edges: &[(u8, u8)]) -> reference_model::Graph {
    let nodes = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let id = u32::try_from(index).unwrap();
            (
                id,
                reference_model::Node::new(id, "a").with_value(ReferenceValue::Int(*value)),
            )
        })
        .collect();
    let count = u32::try_from(values.len()).unwrap();
    let edges = edges
        .iter()
        .enumerate()
        .map(|(index, (a, b))| {
            (
                u32::try_from(index).unwrap(),
                reference_model::GEdge {
                    src: u32::from(*a) % count,
                    tgt: u32::from(*b) % count,
                    label: reference_model::SEdge::new("a", "a", "e", None),
                    position: None,
                },
            )
        })
        .collect();
    reference_model::Graph { nodes, edges }
}

fn generated_graph_word_agrees(graph: &reference_model::Graph, choices: &[usize]) {
    let schema = generated_edit_schema();
    assert_eq!(graph_to_reference(&graph_to_production(graph)), *graph);
    assert_eq!(
        delta_to_reference(&GraphDelta::diff(
            &graph_to_production(graph),
            &graph_to_production(graph)
        )),
        reference::diff(graph, graph)
    );
    let mut floors = carrier::CoverageFloors::of_graph(graph);
    let mut prefixes = vec![GraphPrefix {
        reference: graph.clone(),
        production: graph_to_production(graph),
        composite: reference::GraphDelta::identity(),
        production_composite: GraphDelta::identity(),
    }];
    let mut word = Vec::new();
    let mut tally = CoverageTally::default();
    for choice in choices {
        let before = prefixes.last().unwrap();
        let alphabet = carrier::coverage_graph_letters(&schema, &before.reference, floors);
        let letter = alphabet[*choice % alphabet.len()].clone();
        let production_letter = delta_to_production(&letter);
        let mut production = before.production.clone();
        let actual = production_letter
            .apply(&mut production)
            .map(|()| graph_to_reference(&production))
            .map_err(|e| production_apply_error(&e));
        let expected =
            reference::apply(&letter, &before.reference).map_err(|e| reference_apply_error(&e));
        assert_eq!(actual, expected);
        let Ok(after) = expected else {
            assert_eq!(graph_to_reference(&production), before.reference);
            break;
        };
        word.push(letter.clone());
        let (composite, production_composite) = check_graph_composite(
            &prefixes,
            &letter,
            &production_letter,
            &word,
            &production,
            &mut tally,
        );
        let expected_diff = reference::diff(graph, &after);
        let actual_diff = GraphDelta::diff(&prefixes[0].production, &production);
        assert_eq!(delta_to_reference(&actual_diff), expected_diff);
        assert_eq!(actual_diff.is_identity(), graph == &after);
        floors = floors.after_graph(&after);
        prefixes.push(GraphPrefix {
            reference: after,
            production,
            composite,
            production_composite,
        });
    }
}

fn generated_table_word_agrees(table: &reference_model::Table, choices: &[usize]) {
    let schema = generated_edit_schema();
    assert_eq!(table_to_reference(&table_to_production(table)), *table);
    assert_eq!(
        table_delta_to_reference(&TableDelta::diff(
            &table_to_production(table),
            &table_to_production(table)
        )),
        reference::diff_table(table, table)
    );
    let mut floors = carrier::CoverageFloors::of_table(table);
    let mut prefixes = vec![TablePrefix {
        reference: table.clone(),
        production: table_to_production(table),
        composite: reference::TableDelta::identity(),
        production_composite: TableDelta::identity(),
    }];
    let mut word = Vec::new();
    let mut tally = CoverageTally::default();
    for choice in choices {
        let before = prefixes.last().unwrap();
        let alphabet = carrier::coverage_table_letters(&schema, &before.reference, floors);
        let letter = alphabet[*choice % alphabet.len()].clone();
        let production_letter = table_edit_to_production(&letter);
        let lowered = reference::table_edit_delta(&letter).unwrap();
        let production_lowered = production_letter.to_delta().unwrap();
        assert_eq!(table_delta_to_reference(&production_lowered), lowered);
        let mut production = before.production.clone();
        let actual = production_letter
            .apply(&mut production)
            .map(|()| table_to_reference(&production))
            .map_err(|e| production_table_apply_error(&e));
        let expected = reference::apply_table_step(&letter, &before.reference)
            .map_err(|e| reference_table_apply_error(&e));
        assert_eq!(actual, expected);
        let Ok(after) = expected else {
            assert_eq!(table_to_reference(&production), before.reference);
            break;
        };
        word.push(letter);
        let (composite, production_composite) = check_table_composite(
            &prefixes,
            &lowered,
            &production_lowered,
            &word,
            &after,
            &mut tally,
        );
        let actual_diff = TableDelta::diff(&prefixes[0].production, &production);
        assert_eq!(
            table_delta_to_reference(&actual_diff),
            reference::diff_table(table, &after)
        );
        assert_eq!(actual_diff.is_identity(), table == &after);
        floors = floors.after_table(&after);
        prefixes.push(TablePrefix {
            reference: after,
            production,
            composite,
            production_composite,
        });
    }
}

fn generated_tree_word_agrees(tree: &reference_model::Tree, choices: &[usize]) {
    let schema = generated_edit_schema();
    let (reference_graph, reference_embedding) =
        panproto_reference::encode::tree_to_graph(tree, None).unwrap();
    let mut production = tree_to_production(tree);
    let (production_graph, production_embedding) =
        GInstance::from_wtype_embedded(&production, None).unwrap();
    let start = TreeStart {
        reference_embedding,
        reference_graph,
        production_embedding,
        production_graph,
    };
    assert_eq!(
        graph_to_reference(&start.production_graph),
        start.reference_graph
    );
    assert_eq!(tree_to_reference(&production), *tree);
    let mut expected_tree = tree.clone();
    let mut expected_embedding = start.reference_embedding.clone();
    let mut floors = carrier::CoverageFloors::of_tree(tree);
    let mut word = Vec::new();
    for choice in choices {
        let alphabet = carrier::coverage_tree_letters(&schema, &expected_tree, floors);
        let letter = alphabet[*choice % alphabet.len()].clone();
        let before = production.clone();
        let old_embedding = expected_embedding.clone();
        let expected = reference::apply_tree(&letter, &expected_tree, &mut expected_embedding);
        let actual = tree_edit_to_production(&letter).apply(&mut production);
        word.push(letter);
        let lowering = lower_tree_word(&start, tree, &word);
        match (expected, actual) {
            (Ok(after), Ok(())) => {
                assert_eq!(tree_to_reference(&production), after);
                check_applied_tree_word(lowering, &start, &word);
                floors = floors.after_tree(&after);
                expected_tree = after;
            }
            (Err(expected), Err(actual)) => {
                assert_eq!(
                    reference_tree_refusal(&expected),
                    production_tree_refusal(&actual)
                );
                assert_eq!(tree_to_reference(&production), tree_to_reference(&before));
                assert_eq!(expected_embedding, old_embedding);
                lowering.assert_refused(&start, &reference_tree_refusal(&expected), &word);
                break;
            }
            (expected, actual) => panic!("generated tree apply differs: {expected:?} / {actual:?}"),
        }
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::default())]

    #[test]
    fn randomized_graph_edit_algebra_differential(
        values in proptest::collection::vec(proptest::prelude::any::<i64>(),1..6),
        edges in proptest::collection::vec((proptest::prelude::any::<u8>(),proptest::prelude::any::<u8>()),0..7),
        choices in proptest::collection::vec(proptest::prelude::any::<usize>(),0..6),
        first in proptest::prelude::any::<usize>(),second in proptest::prelude::any::<usize>(),
    ) {
        let graph=generated_edit_graph(&values,&edges);
        generated_graph_word_agrees(&graph,&choices);
        let alphabet=carrier::coverage_graph_letters(&generated_edit_schema(),&graph,carrier::CoverageFloors::of_graph(&graph));
        let (a,b)=(&alphabet[first%alphabet.len()],&alphabet[second%alphabet.len()]);
        let (pa,pb)=(delta_to_production(a),delta_to_production(b));
        assert_eq!(pa.independence(&pb).iter().map(production_conflict).collect::<Vec<_>>(),reference::independence(a,b).iter().map(reference_conflict).collect::<Vec<_>>());
        match (pa.merge(&pb),reference::merge(a,b)) {
            (Ok(actual),Ok(expected)) => {
                assert_eq!(delta_to_reference(&actual),expected);
                let mut first_state=graph_to_production(&graph);
                let mut second_state=first_state.clone();
                if pa.apply(&mut first_state).is_ok() && pb.apply(&mut second_state).is_ok() {
                    pb.apply(&mut first_state).unwrap();
                    pa.apply(&mut second_state).unwrap();
                    let merged=reference::apply(&expected,&graph).unwrap();
                    assert_eq!(graph_to_reference(&first_state),merged);
                    assert_eq!(graph_to_reference(&second_state),merged);
                }
            },
            (Err(actual),Err(expected)) => assert_eq!(production_conflict(&actual),reference_conflict(&expected)),
            (actual,expected) => panic!("generated merge differs: {actual:?} / {expected:?}"),
        }
        let node=graph.nodes[&0].clone();
        let mut delete=reference::GraphDelta::identity();
        delete.del.insert(reference::GElem::Node(0),reference::GPayload::Node(node.clone()));
        let mut add=reference::GraphDelta::identity();
        add.add.insert(reference::GElem::Node(0),reference::GPayload::Node(node));
        let expected=reference::Delta::compose(&delete,&add).unwrap();
        let actual=delta_to_production(&delete).compose(&delta_to_production(&add)).unwrap();
        assert_eq!(delta_to_reference(&actual),expected);
        assert!(!actual.is_identity());
        assert_eq!(actual.invert(),actual);
    }

    #[test]
    fn randomized_table_edit_algebra_differential(
        values in proptest::collection::vec(proptest::prelude::any::<i64>(),1..6),
        choices in proptest::collection::vec(proptest::prelude::any::<usize>(),0..6),
    ) {
        let table=reference_model::Table { rows:values.iter().enumerate().map(|(index,value)|(u32::try_from(index).unwrap(),("a".into(),BTreeMap::from([("f".into(),ReferenceValue::Int(*value))])))).collect(),pairs:BTreeSet::new() };
        generated_table_word_agrees(&table,&choices);
    }

    #[test]
    fn randomized_tree_edit_algebra_differential(
        nodes in proptest::collection::vec((proptest::prelude::any::<u8>(),proptest::prelude::any::<i64>()),1..6),
        choices in proptest::collection::vec(proptest::prelude::any::<usize>(),0..6),
    ) {
        let tree=reference_model::Tree { nodes:nodes.iter().enumerate().map(|(index,(_,value))| { let id=u32::try_from(index).unwrap(); (id,reference_model::Node::new(id,"a").with_value(ReferenceValue::Int(*value))) }).collect(),arcs:nodes.iter().enumerate().skip(1).map(|(index,(parent,_))|(u32::try_from(usize::from(*parent)%index).unwrap(),u32::try_from(index).unwrap(),reference_model::SEdge::new("a","a","e",None))).collect(),fans:Vec::new(),root:0 };
        generated_tree_word_agrees(&tree,&choices);
    }
}

/// Graph deltas applied to segmented graphs, against the reference.
mod segmented_apply {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};

    use panproto_inst::identity::Digest;
    use panproto_inst::segment::{
        AnySegment, ApplyRun, ArtifactKey, Budget, EdgeIdRun, FamilyIndex, FreshIds, GraphSegment,
        IdKind, IdRange, ManifestHeader, ManifestRole, MemoryStore, MissingSegment, Partition,
        Phase, RecordSource, Residual, Resource, RunIncompleteReason, RunJournal, RunSpec,
        RunStatus, SegmentError, SegmentId, SegmentManifest, SegmentRecord, SegmentRunReport,
        SegmentSource, SegmentSpec, SegmentStore, SegmentedApplyError, TaskRecord, apply_run,
        apply_segmented, assemble_graph, partition_graph, read_emitted, seal_full, verify_manifest,
    };
    use panproto_inst::{EdgeId, GEdge, GraphDelta};
    use panproto_reference::enumerate::segments as carrier;
    use panproto_reference::segment as segments;
    use panproto_schema::InstanceShapeKind;

    use super::{
        ApplyObs, delta_to_production, graph_to_production, graph_to_reference,
        production_apply_error, reference, reference_apply_error, reference_model,
    };

    /// The schema digest of the carrier's segments.
    pub const SCHEMA: [u8; 32] = [41; 32];

    /// The production partition of a reference partition.
    pub fn prod_partition(partition: &segments::Partition) -> Partition {
        Partition::new(
            match partition.shape {
                segments::InstanceShapeKind::Graph => InstanceShapeKind::Graph,
                segments::InstanceShapeKind::Table => InstanceShapeKind::Table,
                segments::InstanceShapeKind::Tree => InstanceShapeKind::Tree,
            },
            partition
                .specs
                .iter()
                .map(|spec| SegmentSpec {
                    id: SegmentId(spec.id.0),
                    nodes: spec
                        .nodes
                        .iter()
                        .map(|range| IdRange {
                            lo: range.lo,
                            hi: range.hi,
                        })
                        .collect(),
                    alloc_nodes: spec.alloc_nodes.map(|range| IdRange {
                        lo: range.lo,
                        hi: range.hi,
                    }),
                    alloc_edges: spec.alloc_edges.map(|range| IdRange {
                        lo: range.lo,
                        hi: range.hi,
                    }),
                })
                .collect(),
        )
    }

    /// A refusal as both implementations project it.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum Refusal {
        /// A whole-state refusal.
        Edit(ApplyObs),
        /// An added node no range owns.
        Unowned(u32),
        /// An added edge id outside its holder's edge allocation range.
        Outside(u32, u32),
        /// An added edge id at or above the floor in a segment without an
        /// edge allocation range.
        NoRange(u32),
    }

    pub fn production_refusal(error: &SegmentedApplyError) -> Refusal {
        match error {
            SegmentedApplyError::Edit(error) => Refusal::Edit(production_apply_error(error)),
            SegmentedApplyError::Segment(SegmentError::Unowned {
                elem: panproto_inst::ViewRef::Node(node),
            }) => Refusal::Unowned(*node),
            SegmentedApplyError::Segment(SegmentError::EdgeIdOutsideAllocation {
                edge,
                segment,
            }) => Refusal::Outside(edge.0, segment.0),
            SegmentedApplyError::Segment(SegmentError::NoAllocationRange {
                segment,
                kind: IdKind::Edge,
            }) => Refusal::NoRange(segment.0),
            other => panic!("a refusal outside the carrier: {other:?}"),
        }
    }

    pub fn reference_refusal(error: &segments::SegmentedApplyRefusal) -> Refusal {
        match error {
            segments::SegmentedApplyRefusal::Edit(error) => {
                Refusal::Edit(reference_apply_error(error))
            }
            segments::SegmentedApplyRefusal::Segment(segments::SegmentError::Unowned {
                elem: segments::ViewRef::Node(node),
            }) => Refusal::Unowned(*node),
            segments::SegmentedApplyRefusal::Segment(
                segments::SegmentError::EdgeIdOutsideAllocation { edge, segment },
            ) => Refusal::Outside(*edge, segment.0),
            segments::SegmentedApplyRefusal::Segment(
                segments::SegmentError::NoAllocationRange {
                    segment,
                    kind: segments::IdKind::Edge,
                },
            ) => Refusal::NoRange(segment.0),
            segments::SegmentedApplyRefusal::Segment(other) => {
                panic!("a reference refusal outside the carrier: {other:?}")
            }
        }
    }

    /// The canonical bytes of a production segment.
    fn bytes(segment: &GraphSegment, partition: &Partition) -> Vec<u8> {
        AnySegment::Graph(segment.clone())
            .canonical_bytes(&Digest(SCHEMA), partition)
            .expect("production segment bytes")
    }

    /// The canonical bytes of a reference segment.
    fn reference_bytes(
        segment: &segments::GraphSegment,
        partition: &segments::Partition,
    ) -> Vec<u8> {
        segments::canonical_segment_bytes(
            &segments::AnySegment::Graph(segment.clone()),
            segments::Digest(SCHEMA),
            partition,
        )
        .expect("reference segment bytes")
    }

    /// What one case showed.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum Outcome {
        /// Both applied alike.
        Applied,
        /// Both refused alike with a whole-state refusal.
        Edit,
        /// Both refused alike laying the edited graph out.
        Layout,
    }

    /// Check the law on one case: the read set equals the reference's and
    /// no segment outside it changes; the output segments equal the
    /// reference's byte for byte and glue to whole-state application, which
    /// equals the reference's; or both refuse alike, whole-state
    /// application refusing alike when the refusal is an edit refusal.
    pub fn check_case(
        graph: &reference_model::Graph,
        partition: &segments::Partition,
        delta: &reference::GraphDelta,
    ) -> Outcome {
        let production_partition = prod_partition(partition);
        let whole = graph_to_production(graph);
        let held = partition_graph(&whole, &production_partition).expect("production split");
        let reference_held = segments::partition_graph(graph, partition).expect("reference split");
        let production_delta = delta_to_production(delta);
        let family = FamilyIndex::of_segments(
            &held
                .iter()
                .cloned()
                .map(AnySegment::Graph)
                .collect::<Vec<_>>(),
            &production_partition,
        )
        .expect("a family index");
        let read = family.read_set(&production_delta);
        let any: Vec<segments::AnySegment> = reference_held
            .iter()
            .cloned()
            .map(segments::AnySegment::Graph)
            .collect();
        let records = segments::manifest_records(&any, partition, segments::Digest(SCHEMA))
            .expect("reference records");
        let expected_read: BTreeSet<u32> = segments::read_set(delta, &records, partition)
            .into_iter()
            .map(|segment| segment.0)
            .collect();
        assert_eq!(
            read.iter()
                .map(|segment| segment.0)
                .collect::<BTreeSet<_>>(),
            expected_read,
            "read set of {delta:?} on {graph:?} under {partition:?}"
        );
        let production = apply_segmented(&production_delta, &held, &production_partition);
        let expected = segments::apply_graph_segmented(delta, &reference_held, partition);
        let mut whole_state = whole;
        let applied = production_delta.apply(&mut whole_state);
        match (production, expected) {
            (Ok(output), Ok(expected)) => {
                assert_eq!(output.len(), expected.len());
                for ((segment, expected), before) in
                    output.iter().zip(&expected).zip(&reference_held)
                {
                    assert_eq!(
                        bytes(segment, &production_partition),
                        reference_bytes(expected, partition),
                        "segment {} of {delta:?} on {graph:?} under {partition:?}",
                        segment.segment.0
                    );
                    if !read.contains(&segment.segment) {
                        assert_eq!(expected, before, "an unread segment changes");
                    }
                }
                applied.expect("whole-state application accepts");
                let glued = assemble_graph(output, &production_partition).expect("glues");
                assert_eq!(graph_to_reference(&glued), graph_to_reference(&whole_state));
                assert_eq!(
                    graph_to_reference(&whole_state),
                    reference::apply(delta, graph).expect("reference applies")
                );
                Outcome::Applied
            }
            (Err(error), Err(expected)) => {
                let refusal = production_refusal(&error);
                assert_eq!(
                    refusal,
                    reference_refusal(&expected),
                    "refusal of {delta:?} on {graph:?} under {partition:?}"
                );
                if let Refusal::Edit(obs) = &refusal {
                    assert_eq!(
                        applied.map_err(|error| production_apply_error(&error)),
                        Err(obs.clone())
                    );
                    Outcome::Edit
                } else {
                    applied.expect("whole-state application accepts");
                    Outcome::Layout
                }
            }
            (production, expected) => panic!(
                "segmented apply disagrees on {delta:?} on {graph:?} under {partition:?}: {production:?} / {expected:?}"
            ),
        }
    }

    /// The share `i/n` of `PANPROTO_TEST_SHARD`, every case when unset.
    pub fn shard() -> (usize, usize) {
        std::env::var("PANPROTO_TEST_SHARD").map_or((0, 1), |value| {
            let (index, count) = value.split_once('/').expect("a shard i/n");
            (
                index.parse().expect("a shard index"),
                count.parse().expect("a shard count"),
            )
        })
    }

    /// Walk the carrier, checking every case whose position `select`
    /// keeps; the number of cases walked and each outcome's count.
    fn sweep(select: impl Fn(usize) -> bool) -> (u64, BTreeMap<Outcome, u64>) {
        let mut walked = 0;
        let mut outcomes = BTreeMap::new();
        for (position, case) in carrier::segmented_apply_cases().enumerate() {
            walked += 1;
            if select(position) {
                *outcomes
                    .entry(check_case(&case.graph, &case.partition, &case.delta))
                    .or_insert(0) += 1;
            }
        }
        (walked, outcomes)
    }

    /// Every case of the segmented graph edit carrier, or the share of
    /// `PANPROTO_TEST_SHARD`.
    #[test]
    fn segmented_apply_agrees_with_the_reference_on_every_case_of_the_carrier() {
        let (graphs, partitions, deltas, total) = carrier::segmented_apply_case_counts();
        assert_eq!(carrier::segmented_apply_graphs().len() as u64, graphs);
        assert_eq!(
            carrier::segmented_apply_partitions().len() as u64,
            partitions
        );
        assert_eq!(carrier::segmented_apply_deltas().len() as u64, deltas);
        assert_eq!(total, 958_740);
        let (index, count) = shard();
        let (walked, outcomes) = sweep(|position| position % count == index);
        println!("shard {index}/{count}: {outcomes:?}");
        assert_eq!(walked, total);
        assert_eq!(outcomes.len(), 3, "every outcome occurs: {outcomes:?}");
    }

    /// Every 97th case of the segmented graph edit carrier.
    #[test]
    fn segmented_apply_agrees_with_the_reference_on_every_97th_case_of_the_carrier() {
        let (walked, outcomes) = sweep(|position| position % 97 == 0);
        println!("{outcomes:?}");
        assert_eq!(walked, 958_740);
        assert_eq!(outcomes.values().sum::<u64>(), 9_884);
        assert_eq!(outcomes.len(), 3, "every outcome occurs: {outcomes:?}");
    }

    /// A source that counts the reads of each segment.
    pub struct CountingSource {
        /// Each segment's canonical bytes.
        pub bytes: BTreeMap<SegmentId, Vec<u8>>,
        /// The reads of each segment.
        pub reads: RefCell<BTreeMap<SegmentId, usize>>,
    }

    impl SegmentSource for CountingSource {
        type Error = MissingSegment;

        fn read(&self, segment: SegmentId) -> Result<Vec<u8>, Self::Error> {
            *self.reads.borrow_mut().entry(segment).or_insert(0) += 1;
            self.bytes.read(segment)
        }

        fn byte_len(&self, segment: SegmentId) -> Result<u64, Self::Error> {
            *self.reads.borrow_mut().entry(segment).or_insert(0) += 1;
            self.bytes.byte_len(segment)
        }
    }

    /// The sealed source family of one graph under one partition.
    pub struct Input {
        /// The source manifest, sealed with its instance digest.
        pub manifest: SegmentManifest,
        /// Each segment's canonical bytes.
        pub bytes: BTreeMap<SegmentId, Vec<u8>>,
        /// The production partition.
        pub partition: Partition,
        /// The production segments.
        pub held: Vec<GraphSegment>,
        /// The reference segments.
        pub reference: Vec<segments::GraphSegment>,
    }

    pub fn input(graph: &reference_model::Graph, partition: &segments::Partition) -> Input {
        let production_partition = prod_partition(partition);
        let held = partition_graph(&graph_to_production(graph), &production_partition)
            .expect("production split");
        let any: Vec<AnySegment> = held.iter().cloned().map(AnySegment::Graph).collect();
        let (manifest, bytes) = seal_family(&any, &production_partition);
        Input {
            manifest,
            bytes,
            partition: production_partition,
            held,
            reference: segments::partition_graph(graph, partition).expect("reference split"),
        }
    }

    /// The source manifest of a family, sealed with its instance digest,
    /// and each segment's canonical bytes.
    pub fn seal_family(
        any: &[AnySegment],
        production_partition: &Partition,
    ) -> (SegmentManifest, BTreeMap<SegmentId, Vec<u8>>) {
        let schema = Digest(SCHEMA);
        let records: Vec<SegmentRecord> = any
            .iter()
            .zip(production_partition.specs())
            .map(|(segment, spec)| SegmentRecord::of(segment, spec, &schema))
            .collect();
        let runs: Vec<EdgeIdRun<'_>> = any
            .iter()
            .map(|segment| {
                (
                    segment.segment(),
                    Box::new(segment.edge_id_run().into_iter()) as Box<dyn Iterator<Item = EdgeId>>,
                )
            })
            .collect();
        let sources = any
            .iter()
            .map(|segment| Box::new(segment.records()) as Box<dyn RecordSource + '_>)
            .collect();
        let manifest = seal_full(
            records,
            ManifestHeader {
                shape: production_partition.shape(),
                schema,
                role: ManifestRole::Source,
                binding: None,
            },
            runs,
            sources,
        )
        .expect("the family seals");
        let bytes = any
            .iter()
            .map(|segment| {
                (
                    segment.segment(),
                    segment
                        .canonical_bytes(&schema, production_partition)
                        .expect("segment bytes"),
                )
            })
            .collect();
        (manifest, bytes)
    }

    /// A report as both implementations project it.
    #[derive(Debug, PartialEq, Eq)]
    pub struct Seen {
        status: String,
        budget: (u64, Option<u64>, u64, u32),
        progress: (String, u32, u32, u64, u64),
        peak: (u64, Option<u64>),
        complement: (u64, u64, usize),
        tasks: (u32, u32),
        orphans: Vec<(String, Option<u32>, String)>,
        phases: Vec<(String, u32, u32, u64, Option<u64>)>,
    }

    pub fn seen_production(report: &SegmentRunReport<SegmentedApplyError>) -> Seen {
        seen_production_by(report, &|error| format!("{:?}", production_refusal(error)))
    }

    /// A production report, its refusal projected by `refusal`.
    pub fn seen_production_by(
        report: &SegmentRunReport<SegmentedApplyError>,
        refusal: &dyn Fn(&SegmentedApplyError) -> String,
    ) -> Seen {
        Seen {
            status: match &report.status {
                RunStatus::Complete { manifest } => format!("complete {:?}", manifest.0),
                RunStatus::Incomplete { reason } => format!("incomplete {reason:?}"),
                RunStatus::Residual(residual) => format!("residual {residual:?}"),
                RunStatus::Failed(error) => format!("failed {}", refusal(error)),
            },
            budget: (
                report.budget.working_set,
                report.budget.complement,
                report.budget.summary,
                report.budget.affected_segments,
            ),
            progress: (
                format!("{:?}", report.progress.phase),
                report.progress.tasks_done,
                report.progress.tasks_total,
                report.progress.elements_done,
                report.progress.elements_total,
            ),
            peak: (report.peak_estimated_bytes, report.observed_peak_bytes),
            complement: (
                report.complement.elements,
                report.complement.bytes,
                report.complement.per_segment.len(),
            ),
            tasks: (report.tasks_run, report.tasks_reused),
            orphans: report
                .orphans
                .iter()
                .map(|key| {
                    (
                        format!("{:?}", key.task.phase),
                        key.task.segment.map(|segment| segment.0),
                        format!("{:?}", key.kind),
                    )
                })
                .collect(),
            phases: report
                .phases
                .iter()
                .map(|phase| {
                    (
                        format!("{:?}", phase.phase),
                        phase.tasks,
                        phase.reused,
                        phase.peak_estimated_bytes,
                        phase.wall_ms,
                    )
                })
                .collect(),
        }
    }

    pub fn seen_reference(
        report: &segments::SegmentRunReport<segments::SegmentedApplyRefusal>,
    ) -> Seen {
        seen_reference_by(report, &|error| format!("{:?}", reference_refusal(error)))
    }

    /// A reference report, its refusal projected by `refusal`.
    pub fn seen_reference_by<E>(
        report: &segments::SegmentRunReport<segments::SegmentedApplyRefusal<E>>,
        refusal: &dyn Fn(&segments::SegmentedApplyRefusal<E>) -> String,
    ) -> Seen {
        Seen {
            status: match &report.status {
                segments::RunStatus::Complete { manifest } => format!("complete {:?}", manifest.0),
                segments::RunStatus::Incomplete { reason } => format!("incomplete {reason:?}"),
                segments::RunStatus::Residual(residual) => format!("residual {residual:?}"),
                segments::RunStatus::Failed(error) => format!("failed {}", refusal(error)),
            },
            budget: (
                report.budget.working_set,
                report.budget.complement,
                report.budget.summary,
                report.budget.affected_segments,
            ),
            progress: (
                format!("{:?}", report.progress.phase),
                report.progress.tasks_done,
                report.progress.tasks_total,
                report.progress.elements_done,
                report.progress.elements_total,
            ),
            peak: (report.peak_estimated_bytes, report.observed_peak_bytes),
            complement: (
                report.complement.elements,
                report.complement.bytes,
                report.complement.per_segment.len(),
            ),
            tasks: (report.tasks_run, report.tasks_reused),
            orphans: report
                .orphans
                .iter()
                .map(|key| {
                    (
                        format!("{:?}", key.task.phase),
                        key.task.segment.map(|segment| segment.0),
                        format!("{:?}", key.kind),
                    )
                })
                .collect(),
            phases: report
                .phases
                .iter()
                .map(|phase| {
                    (
                        format!("{:?}", phase.phase),
                        phase.tasks,
                        phase.reused,
                        phase.peak_estimated_bytes,
                        phase.wall_ms,
                    )
                })
                .collect(),
        }
    }

    fn production_run<Src: SegmentSource, St: SegmentStore>(
        delta: &GraphDelta,
        input: &Input,
        source: &Src,
        budget: Budget,
        store: &mut St,
        cancel: Option<u32>,
    ) -> ApplyRun {
        apply_run(
            delta,
            &input.manifest,
            source,
            RunSpec {
                lens: Digest([0; 32]),
                input: input.manifest.digest(),
                output_schema: Digest([0; 32]),
                cancel_after_tasks: cancel,
            },
            budget,
            store,
            &mut |_| {},
        )
    }

    fn reference_run(
        delta: &reference::GraphDelta,
        input: &Input,
        partition: &segments::Partition,
        budget: Budget,
        store: &mut segments::MemoryStore,
        cancel: Option<u32>,
    ) -> segments::ApplyRun {
        segments::segmented_apply_with_progress(
            delta,
            &input.reference,
            partition,
            &segments::ApplyRunSpec {
                schema: segments::Digest(SCHEMA),
                binding: None,
                cancel_after_tasks: cancel,
            },
            segments::Budget {
                working_set: budget.working_set,
                complement: budget.complement,
                summary: budget.summary,
                affected_segments: budget.affected_segments,
            },
            store,
            &mut |_| {},
        )
    }

    /// The journal of a run holds no Emit or Seal task.
    pub fn sealed_nothing(store: &MemoryStore, run: Digest) -> bool {
        let journal: RunJournal = store.journal(&run).unwrap_or_else(|never| match never {});
        journal
            .tasks
            .keys()
            .all(|task| !matches!(task.phase, Phase::Emit | Phase::Seal))
    }

    /// How a checked run ended.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum RunOutcome {
        /// Complete.
        Complete,
        /// Refused.
        Refused,
        /// Declined within its budget.
        Declined,
    }

    /// Check the run law on one case under `budget`: the production report
    /// equals the reference runner's, uninterrupted and, when `every_point`,
    /// at every cancellation point and after resuming; a complete run reads
    /// each segment of the read set once and no other, emits the
    /// reference's edited segments for the read set, and seals the
    /// reference's manifest; a refused or declined run seals nothing. It
    /// returns how the run ended and the cancellation points checked.
    pub fn check_run(
        graph: &reference_model::Graph,
        partition: &segments::Partition,
        delta: &reference::GraphDelta,
        budget: Budget,
        every_point: bool,
    ) -> (RunOutcome, u64) {
        let input = input(graph, partition);
        let production_delta = delta_to_production(delta);
        let source = CountingSource {
            bytes: input.bytes.clone(),
            reads: RefCell::new(BTreeMap::new()),
        };
        let mut store = MemoryStore::default();
        let baseline = production_run(&production_delta, &input, &source, budget, &mut store, None);
        let mut reference_store = segments::MemoryStore::default();
        let expected = reference_run(delta, &input, partition, budget, &mut reference_store, None);
        assert_eq!(
            seen_production(&baseline.report),
            seen_reference(&expected.report),
            "run of {delta:?} on {graph:?} under {partition:?}"
        );
        let run = baseline.run.expect("a run identity");
        assert_eq!(run.0, expected.run.0, "run identity");
        let outcome = match &baseline.report.status {
            RunStatus::Complete { manifest } => {
                let read: BTreeSet<SegmentId> = expected
                    .read
                    .iter()
                    .map(|segment| SegmentId(segment.0))
                    .collect();
                let reads = source.reads.borrow().clone();
                assert_eq!(
                    reads,
                    read.iter()
                        .map(|segment| (*segment, 1))
                        .collect::<BTreeMap<_, _>>(),
                    "a complete run reads each segment of its read set once and no other"
                );
                let sealed = baseline.manifest.as_ref().expect("a manifest");
                assert_eq!(verify_manifest(sealed, *manifest), Ok(()));
                assert_eq!(
                    sealed.canonical_bytes(),
                    segments::canonical_manifest_bytes(
                        expected.manifest.as_ref().expect("a reference manifest")
                    ),
                    "the sealed manifest of {delta:?} on {graph:?}"
                );
                let output = expected.output.as_ref().expect("the edited family");
                for segment in &read {
                    let emitted = read_emitted(&store, run, &input.partition, *segment)
                        .expect("an emitted segment");
                    assert_eq!(emitted.complement, Vec::<u8>::new());
                    assert_eq!(
                        bytes(&emitted.view, &input.partition),
                        reference_bytes(&output[segment.0 as usize], partition)
                    );
                }
                RunOutcome::Complete
            }
            RunStatus::Failed(_) => {
                assert!(sealed_nothing(&store, run), "a refused run seals nothing");
                RunOutcome::Refused
            }
            RunStatus::Residual(_) => {
                assert!(sealed_nothing(&store, run), "a declined run seals nothing");
                RunOutcome::Declined
            }
            RunStatus::Incomplete { reason } => panic!("an uninterrupted run stopped: {reason:?}"),
        };
        let points = if every_point {
            check_cancellations(&input, partition, delta, budget, &baseline)
        } else {
            0
        };
        (outcome, points)
    }

    /// Stopping the run at every cancellation point and resuming it reports
    /// what the reference runner reports, and ends as `baseline`.
    fn check_cancellations(
        input: &Input,
        partition: &segments::Partition,
        delta: &reference::GraphDelta,
        budget: Budget,
        baseline: &ApplyRun,
    ) -> u64 {
        let production_delta = delta_to_production(delta);
        let run = baseline.run.expect("a run identity");
        let (total, done) = (
            baseline.report.progress.tasks_total,
            baseline.report.progress.tasks_done,
        );
        for cancel in 0..=total {
            let mut store = MemoryStore::default();
            let stopped = production_run(
                &production_delta,
                input,
                &input.bytes,
                budget,
                &mut store,
                Some(cancel),
            );
            let mut reference_store = segments::MemoryStore::default();
            let expected_stop = reference_run(
                delta,
                input,
                partition,
                budget,
                &mut reference_store,
                Some(cancel),
            );
            assert_eq!(
                seen_production(&stopped.report),
                seen_reference(&expected_stop.report),
                "run cancelled after {cancel} tasks"
            );
            let resumed = production_run(
                &production_delta,
                input,
                &input.bytes,
                budget,
                &mut store,
                None,
            );
            let again = reference_run(delta, input, partition, budget, &mut reference_store, None);
            assert_eq!(
                seen_production(&resumed.report),
                seen_reference(&again.report),
                "run resumed after {cancel} tasks"
            );
            assert_eq!(resumed.report.tasks_reused, cancel.min(done));
            assert_eq!(resumed.manifest, baseline.manifest);
            assert_eq!(
                store
                    .staged_uncommitted(&run)
                    .unwrap_or_else(|never| match never {}),
                Vec::<ArtifactKey>::new()
            );
        }
        u64::from(total) + 1
    }

    /// Every 331st case of the segmented graph edit carrier, run as a
    /// checkpointed run from every cancellation point against the
    /// reference runner.
    #[test]
    fn segmented_apply_run_matches_the_reference_runner_at_every_cancellation_point() {
        let mut outcomes: BTreeMap<RunOutcome, u64> = BTreeMap::new();
        let mut points = 0;
        for case in carrier::segmented_apply_cases().step_by(331) {
            let (outcome, checked) = check_run(
                &case.graph,
                &case.partition,
                &case.delta,
                Budget::unlimited(),
                true,
            );
            *outcomes.entry(outcome).or_insert(0) += 1;
            points += checked;
        }
        println!("{outcomes:?}, {points} cancellation points");
        assert_eq!(outcomes.values().sum::<u64>(), 2_897);
        assert!(
            outcomes.contains_key(&RunOutcome::Complete)
                && outcomes.contains_key(&RunOutcome::Refused),
            "{outcomes:?}"
        );
    }

    /// The witness family: nodes 0 to 3, one per segment, each with a
    /// self-loop, and the edges 0 to 1 and 1 to 2 crossing segments; edge
    /// allocation ranges `[10 + 10 i, 19 + 10 i]`.
    fn witness() -> (reference_model::Graph, segments::Partition) {
        let mut graph = reference_model::Graph::default();
        for id in 0..4 {
            graph.nodes.insert(id, reference_model::Node::new(id, "a"));
        }
        let label = reference_model::SEdge::new("a", "a", "e", None);
        for (id, (src, tgt)) in (0_u32..).zip([(0, 0), (1, 1), (2, 2), (3, 3), (0, 1), (1, 2)]) {
            graph.edges.insert(
                id,
                reference_model::GEdge {
                    src,
                    tgt,
                    label: label.clone(),
                    position: None,
                },
            );
        }
        let partition = segments::Partition {
            shape: segments::InstanceShapeKind::Graph,
            specs: (0..4)
                .map(|id| segments::SegmentSpec {
                    id: segments::SegmentId(id),
                    nodes: vec![segments::IdRange { lo: id, hi: id }],
                    alloc_nodes: None,
                    alloc_edges: Some(segments::IdRange {
                        lo: 10 + 10 * id,
                        hi: 19 + 10 * id,
                    }),
                })
                .collect(),
        };
        (graph, partition)
    }

    /// The delta changing node `node`'s anchor from `a` to `b`.
    fn anchor_change(node: u32) -> reference::GraphDelta {
        let mut delta = reference::GraphDelta::identity();
        delta.attrs.insert(
            reference::AttrKey::Anchor(node),
            (
                reference::AttrVal::Anchor("a".to_owned()),
                reference::AttrVal::Anchor("b".to_owned()),
            ),
        );
        delta
    }

    #[test]
    fn a_segmented_apply_reads_only_the_owners_and_boundary_peers_of_what_it_names() {
        let (graph, partition) = witness();
        let input = input(&graph, &partition);
        // A position change of edge 5, the family's largest edge id, reads
        // segment 1 alone, the one segment whose edge ids reach 5; an anchor
        // change of node 1 also reads the peers whose ghost and in-stub copy
        // it; adding the edge 3 to 0 under a fresh id of segment 3 reads its
        // two endpoints' segments.
        let mut position = reference::GraphDelta::identity();
        position.attrs.insert(
            reference::AttrKey::EdgePosition(5),
            (
                reference::AttrVal::Position(None),
                reference::AttrVal::Position(Some(0)),
            ),
        );
        let mut fresh = FreshIds::of(
            &AnySegment::Graph(input.held[3].clone()),
            &input.partition.specs()[3],
        )
        .expect("ids");
        let id = fresh.edge().expect("a fresh edge id");
        assert_eq!(id, EdgeId(40));
        let added = delta_to_production(&reference::GraphDelta::identity()).with_added_edge(
            id,
            GEdge::new(
                3,
                0,
                panproto_schema::Edge {
                    src: "a".into(),
                    tgt: "a".into(),
                    kind: "e".into(),
                    name: None,
                },
            ),
        );
        let added = super::delta_to_reference(&added);
        for (delta, read) in [
            (position, vec![1]),
            (anchor_change(1), vec![0, 1, 2]),
            (added, vec![0, 3]),
        ] {
            let production = delta_to_production(&delta);
            let source = CountingSource {
                bytes: input.bytes.clone(),
                reads: RefCell::new(BTreeMap::new()),
            };
            let mut store = MemoryStore::default();
            let run = production_run(
                &production,
                &input,
                &source,
                Budget::unlimited(),
                &mut store,
                None,
            );
            let sealed = run.manifest.expect("the run completes");
            let expected: BTreeMap<SegmentId, usize> = read
                .iter()
                .map(|segment| (SegmentId(*segment), 1))
                .collect();
            assert_eq!(*source.reads.borrow(), expected, "{delta:?}");
            let total = u32::try_from(4 * read.len() + 2).expect("a small count");
            assert_eq!(run.report.progress.tasks_total, total);
            for (before, after) in input.manifest.segments.iter().zip(&sealed.segments) {
                if !expected.contains_key(&before.id) {
                    assert_eq!(before, after, "an unread segment keeps its record");
                }
            }
            assert_eq!(sealed.instance_digest, None);
            assert_eq!(
                check_run(&graph, &partition, &delta, Budget::unlimited(), true).0,
                RunOutcome::Complete
            );
        }
    }

    #[test]
    fn a_refused_delta_fails_at_assemble_with_the_whole_state_refusal_and_seals_nothing() {
        let (graph, partition) = witness();
        let input = input(&graph, &partition);
        // Deleting node 1 leaves its loop and the edges 0 to 1 and 1 to 2.
        let mut delete = reference::GraphDelta::identity();
        delete.del.insert(
            reference::GElem::Node(1),
            reference::GPayload::Node(reference_model::Node::new(1, "a")),
        );
        let production = delta_to_production(&delete);
        let mut store = MemoryStore::default();
        let run = production_run(
            &production,
            &input,
            &input.bytes,
            Budget::unlimited(),
            &mut store,
            None,
        );
        match &run.report.status {
            RunStatus::Failed(error) => assert_eq!(
                production_refusal(error),
                Refusal::Edit(ApplyObs::DanglingEdges(1, vec![1, 4, 5]))
            ),
            other => panic!("the delta is refused: {other:?}"),
        }
        assert_eq!(run.report.progress.phase, Phase::Summarize);
        assert!(sealed_nothing(&store, run.run.expect("a run identity")));
        assert_eq!(
            check_run(&graph, &partition, &delete, Budget::unlimited(), true).0,
            RunOutcome::Refused
        );
    }

    #[test]
    fn a_segmented_apply_run_declines_each_exceeded_budget_like_the_reference() {
        let (graph, partition) = witness();
        let delta = anchor_change(1);
        for (budget, resource) in [
            (
                Budget {
                    affected_segments: 2,
                    ..Budget::unlimited()
                },
                Resource::AffectedSegments,
            ),
            (
                Budget {
                    working_set: 0,
                    ..Budget::unlimited()
                },
                Resource::WorkingSet,
            ),
            (
                Budget {
                    summary: 2,
                    ..Budget::unlimited()
                },
                Resource::Summary,
            ),
        ] {
            assert_eq!(
                check_run(&graph, &partition, &delta, budget, true).0,
                RunOutcome::Declined
            );
            let input = input(&graph, &partition);
            let mut store = MemoryStore::default();
            let run = production_run(
                &delta_to_production(&delta),
                &input,
                &input.bytes,
                budget,
                &mut store,
                None,
            );
            match &run.report.status {
                RunStatus::Residual(Residual::BudgetExceeded {
                    resource: found, ..
                }) => {
                    assert_eq!(*found, resource);
                }
                other => panic!("{resource:?} declines: {other:?}"),
            }
        }
    }

    /// A store whose `fail_at`th commit fails, leaving its artifact staged.
    pub struct FailingStore {
        /// The store the commits reach.
        pub inner: MemoryStore,
        /// The commits so far.
        pub commits: usize,
        /// The commit that fails.
        pub fail_at: Option<usize>,
    }

    #[derive(Debug)]
    pub struct CommitFailed;

    impl std::fmt::Display for CommitFailed {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("commit failed")
        }
    }

    impl std::error::Error for CommitFailed {}

    impl SegmentStore for FailingStore {
        type Error = CommitFailed;

        fn read(&self, key: &ArtifactKey) -> Result<Option<Vec<u8>>, Self::Error> {
            Ok(self.inner.read(key).unwrap_or_else(|never| match never {}))
        }

        fn read_at(
            &self,
            key: &ArtifactKey,
            offset: u64,
            len: u64,
        ) -> Result<Option<Vec<u8>>, Self::Error> {
            Ok(self
                .inner
                .read_at(key, offset, len)
                .unwrap_or_else(|never| match never {}))
        }

        fn stage(&mut self, key: &ArtifactKey, bytes: Vec<u8>) -> Result<(), Self::Error> {
            self.inner
                .stage(key, bytes)
                .unwrap_or_else(|never| match never {});
            Ok(())
        }

        fn commit(&mut self, key: &ArtifactKey, record: &TaskRecord) -> Result<(), Self::Error> {
            self.commits += 1;
            if Some(self.commits) == self.fail_at {
                return Err(CommitFailed);
            }
            self.inner
                .commit(key, record)
                .unwrap_or_else(|never| match never {});
            Ok(())
        }

        fn journal(&self, run: &Digest) -> Result<RunJournal, Self::Error> {
            Ok(self
                .inner
                .journal(run)
                .unwrap_or_else(|never| match never {}))
        }

        fn staged_uncommitted(&self, run: &Digest) -> Result<Vec<ArtifactKey>, Self::Error> {
            Ok(self
                .inner
                .staged_uncommitted(run)
                .unwrap_or_else(|never| match never {}))
        }
    }

    #[test]
    fn a_segmented_apply_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run()
     {
        let (graph, partition) = witness();
        let input = input(&graph, &partition);
        let delta = delta_to_production(&anchor_change(1));
        let mut baseline_store = MemoryStore::default();
        let baseline = production_run(
            &delta,
            &input,
            &input.bytes,
            Budget::unlimited(),
            &mut baseline_store,
            None,
        );
        assert!(matches!(baseline.report.status, RunStatus::Complete { .. }));
        let total = baseline.report.progress.tasks_total;
        for fail_at in 1..=total {
            let mut store = FailingStore {
                inner: MemoryStore::default(),
                commits: 0,
                fail_at: Some(usize::try_from(fail_at).expect("a small count")),
            };
            let failed = production_run(
                &delta,
                &input,
                &input.bytes,
                Budget::unlimited(),
                &mut store,
                None,
            );
            assert!(
                matches!(
                    &failed.report.status,
                    RunStatus::Incomplete {
                        reason: RunIncompleteReason::Store { message }
                    } if message == "commit failed"
                ),
                "{:?}",
                failed.report.status
            );
            assert_eq!(failed.report.progress.tasks_done, fail_at - 1);
            assert_eq!(failed.report.orphans.len(), 1, "the failed task's artifact");
            store.fail_at = None;
            let resumed = production_run(
                &delta,
                &input,
                &input.bytes,
                Budget::unlimited(),
                &mut store,
                None,
            );
            assert_eq!(resumed.report.tasks_reused, fail_at - 1);
            assert_eq!(resumed.report.orphans, Vec::<ArtifactKey>::new());
            assert_eq!(resumed.manifest, baseline.manifest);
        }
    }

    #[test]
    fn fresh_ids_come_from_the_holders_ranges_and_are_never_refused() {
        let (graph, mut partition) = witness();
        partition.specs[2].alloc_nodes = None;
        partition.specs[3].alloc_edges = None;
        let input = input(&graph, &partition);
        // Segment 2 owns node 2 alone and has no node allocation range;
        // segment 3 has no edge allocation range.
        let mut two = FreshIds::of(
            &AnySegment::Graph(input.held[2].clone()),
            &input.partition.specs()[2],
        )
        .expect("ids");
        assert_eq!(
            two.node(),
            Err(SegmentError::NoAllocationRange {
                segment: SegmentId(2),
                kind: IdKind::Node,
            })
        );
        assert_eq!(two.edge(), Ok(EdgeId(30)));
        assert_eq!(two.edge(), Ok(EdgeId(31)));
        let mut three = FreshIds::of(
            &AnySegment::Graph(input.held[3].clone()),
            &input.partition.specs()[3],
        )
        .expect("ids");
        assert_eq!(
            three.edge(),
            Err(SegmentError::NoAllocationRange {
                segment: SegmentId(3),
                kind: IdKind::Edge,
            })
        );
        // Two fresh edges from node 2, to node 3 and to itself, apply.
        let mut two = FreshIds::of(
            &AnySegment::Graph(input.held[2].clone()),
            &input.partition.specs()[2],
        )
        .expect("ids");
        let label = panproto_schema::Edge {
            src: "a".into(),
            tgt: "a".into(),
            kind: "e".into(),
            name: None,
        };
        let delta = GraphDelta::identity()
            .with_added_edge(two.edge().expect("an id"), GEdge::new(2, 3, label.clone()))
            .with_added_edge(two.edge().expect("an id"), GEdge::new(2, 2, label));
        let output = apply_segmented(&delta, &input.held, &input.partition).expect("applies");
        let glued = assemble_graph(output, &input.partition).expect("glues");
        assert_eq!(glued.edge(EdgeId(30)).map(|edge| edge.tgt), Some(3));
        assert_eq!(glued.edge(EdgeId(31)).map(|edge| edge.tgt), Some(2));
    }

    /// The graph whose nodes `0 .. anchors.len()` carry `anchors` (`0` for
    /// `a`, `1` for `b`, `2` for `a` with the hyper-edge shape, which
    /// leaves the node out of the input) and whose edges are `edges`
    /// (source, target, label, id; an edge naming a missing node or a used
    /// id is left out).
    fn generated(
        anchors: &[u8],
        edges: &[(u32, u32, u8, u32)],
        fans: bool,
    ) -> reference_model::Graph {
        let mut graph = reference_model::Graph::default();
        for (id, anchor) in (0_u32..).zip(anchors) {
            let mut node = reference_model::Node::new(id, if *anchor == 1 { "b" } else { "a" });
            match anchor {
                2 if fans => node.shape = reference_model::NodeShape::HyperEdge,
                2 | 3 => continue,
                _ => {}
            }
            graph.nodes.insert(id, node);
        }
        for (src, tgt, label, id) in edges {
            if graph.nodes.contains_key(src)
                && graph.nodes.contains_key(tgt)
                && !graph.edges.contains_key(id)
            {
                graph.edges.insert(
                    *id,
                    reference_model::GEdge {
                        src: *src,
                        tgt: *tgt,
                        label: reference_model::SEdge::new(
                            "a",
                            "a",
                            "e",
                            (*label == 1).then_some("x"),
                        ),
                        position: None,
                    },
                );
            }
        }
        graph
    }

    /// The partition assigning node id `i` to segment `owners[i]`
    /// (renumbered by first occurrence, `3` for no segment) with the edge
    /// allocation `allocation` of [`carrier::segmented_apply_partitions`].
    fn generated_partition(owners: &[u8], allocation: u8) -> segments::Partition {
        let mut renamed: BTreeMap<u8, u32> = BTreeMap::new();
        let mut owned = Vec::new();
        for owner in owners {
            if *owner == 3 {
                owned.push(None);
            } else {
                let next = u32::try_from(renamed.len()).expect("a small count");
                owned.push(Some(*renamed.entry(*owner).or_insert(next)));
            }
        }
        let segments = u32::try_from(renamed.len()).expect("a small count");
        segments::Partition {
            shape: segments::InstanceShapeKind::Graph,
            specs: (0..segments)
                .map(|id| segments::SegmentSpec {
                    id: segments::SegmentId(id),
                    nodes: (0_u32..)
                        .zip(&owned)
                        .filter(|(_, owner)| **owner == Some(id))
                        .map(|(node, _)| segments::IdRange { lo: node, hi: node })
                        .collect(),
                    alloc_nodes: None,
                    alloc_edges: match allocation {
                        1 => Some(segments::IdRange {
                            lo: 10 + 10 * id,
                            hi: 19 + 10 * id,
                        }),
                        2 if id == 0 => Some(segments::IdRange { lo: 10, hi: 19 }),
                        _ => None,
                    },
                })
                .collect(),
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

        /// The law on generated families and deltas: the delta is the
        /// reference difference between the input and a second generated
        /// graph, whose node ids may be owned by no segment and whose edge
        /// ids reach into the edge allocation ranges.
        #[test]
        fn segmented_apply_agrees_with_the_reference_on_generated_families(
            anchors in proptest::collection::vec(0_u8..2, 1..=6),
            edges in proptest::collection::vec((0_u32..6, 0_u32..6, 0_u8..2, 0_u32..8), 0..=8),
            after_anchors in proptest::collection::vec(0_u8..4, 8),
            after_edges in proptest::collection::vec(
                (0_u32..8, 0_u32..8, 0_u8..2, proptest::sample::select(vec![0_u32, 1, 2, 3, 4, 5, 9, 10, 11, 20, 21, 30])),
                0..=8,
            ),
            owners in proptest::collection::vec(0_u8..3, 6),
            spare in proptest::collection::vec(0_u8..4, 2),
            allocation in 0_u8..3,
        ) {
            let owners: Vec<u8> = owners.into_iter().chain(spare).collect();
            let graph = generated(&anchors, &edges, false);
            let partition = generated_partition(&owners, allocation);
            let after = generated(&after_anchors, &after_edges, true);
            let delta = reference::diff(&graph, &after);
            check_case(&graph, &partition, &delta);
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(32))]

        /// The run law on generated families and deltas, at every
        /// cancellation point.
        #[test]
        fn segmented_apply_run_matches_the_reference_runner_on_generated_families(
            anchors in proptest::collection::vec(0_u8..2, 1..=5),
            edges in proptest::collection::vec((0_u32..5, 0_u32..5, 0_u8..2, 0_u32..8), 0..=6),
            after_anchors in proptest::collection::vec(0_u8..4, 6),
            after_edges in proptest::collection::vec(
                (0_u32..6, 0_u32..6, 0_u8..2, proptest::sample::select(vec![0_u32, 1, 2, 3, 9, 10, 11, 20])),
                0..=6,
            ),
            owners in proptest::collection::vec(0_u8..3, 5),
            spare in 0_u8..4,
            allocation in 0_u8..3,
        ) {
            let owners: Vec<u8> = owners.into_iter().chain([spare]).collect();
            let graph = generated(&anchors, &edges, false);
            let partition = generated_partition(&owners, allocation);
            let after = generated(&after_anchors, &after_edges, true);
            let delta = reference::diff(&graph, &after);
            check_run(&graph, &partition, &delta, Budget::unlimited(), true);
        }
    }

    /// The output of in-memory segmented apply, as canonical bytes per
    /// segment.
    fn applied_bytes(
        delta: &GraphDelta,
        held: &[GraphSegment],
        partition: &Partition,
    ) -> Option<Vec<Vec<u8>>> {
        apply_segmented(delta, held, partition)
            .ok()
            .map(|output| output.iter().map(|s| bytes(s, partition)).collect())
    }

    /// What one ordered pair of the composition carrier showed.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum PairOutcome {
        /// Both deltas applied in turn, their composite is defined and
        /// applied to the same family.
        Composed,
        /// Both applied in turn and their composite is undefined.
        Uncomposable,
        /// The first delta is refused.
        FirstRefused,
        /// The first applies and the second is refused on its result.
        SecondRefused,
    }

    /// Composition over segments: for every ordered pair of deltas of the
    /// composition carrier on every family of it, when the first applies
    /// segment by segment, the second applies to its result, and their
    /// composite is defined, the composite applies to the family and gives
    /// the same segments byte for byte, as the reference composite does; and
    /// on this carrier the composite of two deltas that apply in turn is
    /// always defined.
    #[test]
    fn a_composite_delta_applies_to_a_segmented_family_as_its_factors_do_in_turn() {
        let (families, deltas, pairs) = carrier::segmented_compose_counts();
        let family_cases = carrier::segmented_compose_families();
        let delta_cases = carrier::segmented_compose_deltas();
        assert_eq!(family_cases.len() as u64, families);
        assert_eq!(delta_cases.len() as u64, deltas);
        let mut outcomes: BTreeMap<PairOutcome, u64> = BTreeMap::new();
        for (graph, partition) in &family_cases {
            let production_partition = prod_partition(partition);
            let held =
                partition_graph(&graph_to_production(graph), &production_partition).expect("split");
            let reference_held = segments::partition_graph(graph, partition).expect("split");
            for first in &delta_cases {
                let a = delta_to_production(first);
                let after_first = apply_segmented(&a, &held, &production_partition);
                for second in &delta_cases {
                    let b = delta_to_production(second);
                    let composite = a.compose(&b);
                    assert_eq!(
                        composite.is_ok(),
                        first.compose(second).is_ok(),
                        "composability of {first:?} and {second:?}"
                    );
                    let outcome = after_first.as_ref().map_or(
                        PairOutcome::FirstRefused,
                        |middle| match (
                            apply_segmented(&b, middle, &production_partition),
                            &composite,
                        ) {
                            (Err(_), _) => PairOutcome::SecondRefused,
                            (Ok(_), Err(_)) => PairOutcome::Uncomposable,
                            (Ok(last), Ok(composite)) => {
                                let expected: Vec<Vec<u8>> = last
                                    .iter()
                                    .map(|s| bytes(s, &production_partition))
                                    .collect();
                                assert_eq!(
                                    applied_bytes(composite, &held, &production_partition),
                                    Some(expected.clone()),
                                    "{first:?} then {second:?} on {graph:?} under {partition:?}"
                                );
                                let reference_composite =
                                    first.compose(second).expect("the reference composes");
                                let reference = segments::apply_graph_segmented(
                                    &reference_composite,
                                    &reference_held,
                                    partition,
                                )
                                .expect("the reference composite applies");
                                assert_eq!(
                                    reference
                                        .iter()
                                        .map(|s| reference_bytes(s, partition))
                                        .collect::<Vec<_>>(),
                                    expected
                                );
                                PairOutcome::Composed
                            }
                        },
                    );
                    *outcomes.entry(outcome).or_insert(0) += 1;
                }
            }
        }
        println!("{outcomes:?}");
        assert_eq!(outcomes.values().sum::<u64>(), pairs);
        assert_eq!(pairs, 43_808);
        // Two deltas that apply in turn always compose on this carrier.
        assert_eq!(
            outcomes.keys().copied().collect::<Vec<_>>(),
            vec![
                PairOutcome::Composed,
                PairOutcome::FirstRefused,
                PairOutcome::SecondRefused
            ],
            "{outcomes:?}"
        );
    }

    /// Inversion over segments, in memory: on every 97th case of the
    /// segmented graph edit carrier that applies, the inverse delta applied
    /// segment by segment to the output restores every input segment byte
    /// for byte, as it does in the reference.
    #[test]
    fn the_inverse_of_an_applied_segmented_edit_restores_every_segment_byte_for_byte() {
        let mut restored = 0_u64;
        for case in carrier::segmented_apply_cases().step_by(97) {
            let production_partition = prod_partition(&case.partition);
            let held = partition_graph(&graph_to_production(&case.graph), &production_partition)
                .expect("split");
            let delta = delta_to_production(&case.delta);
            let Ok(output) = apply_segmented(&delta, &held, &production_partition) else {
                continue;
            };
            let inverse = delta.invert();
            assert_eq!(
                applied_bytes(&inverse, &output, &production_partition),
                Some(
                    held.iter()
                        .map(|s| bytes(s, &production_partition))
                        .collect()
                ),
                "the inverse of {:?} on {:?}",
                case.delta,
                case.graph
            );
            let reference_output = segments::apply_graph_segmented(
                &case.delta,
                &segments::partition_graph(&case.graph, &case.partition).expect("split"),
                &case.partition,
            )
            .expect("the reference applies");
            let back = segments::apply_graph_segmented(
                &case.delta.invert(),
                &reference_output,
                &case.partition,
            )
            .expect("the reference inverse applies");
            assert_eq!(
                back,
                segments::partition_graph(&case.graph, &case.partition).expect("split")
            );
            restored += 1;
        }
        println!("{restored} restored");
        assert_eq!(restored, 1_363);
    }

    /// Inversion over segments, as checkpointed runs: on every 331st case of
    /// the carrier whose run completes, running the inverse delta over the
    /// output manifest, with the emitted segments in place of the read ones,
    /// seals the input manifest with its instance digest cleared, and so the
    /// two manifests differ in that field alone.
    #[test]
    fn the_inverse_run_of_a_completed_segmented_edit_restores_the_manifest_but_its_instance_digest()
    {
        let mut restored = 0_u64;
        for case in carrier::segmented_apply_cases().step_by(331) {
            let input = input(&case.graph, &case.partition);
            let delta = delta_to_production(&case.delta);
            let mut store = MemoryStore::default();
            let forward = production_run(
                &delta,
                &input,
                &input.bytes,
                Budget::unlimited(),
                &mut store,
                None,
            );
            let (Some(edited), Some(run)) = (forward.manifest, forward.run) else {
                continue;
            };
            let mut bytes_after = input.bytes.clone();
            for spec in input.partition.specs() {
                if let Ok(emitted) = read_emitted(&store, run, &input.partition, spec.id) {
                    bytes_after.insert(
                        spec.id,
                        AnySegment::Graph(emitted.view)
                            .canonical_bytes(&Digest(SCHEMA), &input.partition)
                            .expect("bytes"),
                    );
                }
            }
            let after = Input {
                manifest: edited,
                bytes: bytes_after,
                partition: input.partition.clone(),
                held: Vec::new(),
                reference: Vec::new(),
            };
            let back = production_run(
                &delta.invert(),
                &after,
                &after.bytes,
                Budget::unlimited(),
                &mut store,
                None,
            )
            .manifest
            .expect("the inverse run completes");
            let mut expected = input.manifest.clone();
            assert!(expected.instance_digest.is_some());
            expected.clear_instance_digest();
            assert_eq!(back.canonical_bytes(), expected.canonical_bytes());
            restored += 1;
        }
        println!("{restored} restored");
        assert_eq!(restored, 402);
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

        /// Composition and inversion over segments on generated families:
        /// the deltas are the reference differences from the input to a
        /// second graph and from it to a third, so both apply in turn to
        /// the glued graph; when both apply segment by segment, their
        /// composite applies to the family with the same segments, and the
        /// inverse of the first restores the family byte for byte.
        #[test]
        fn composition_and_inversion_hold_over_segments_on_generated_families(
            anchors in proptest::collection::vec(0_u8..2, 1..=5),
            edges in proptest::collection::vec((0_u32..5, 0_u32..5, 0_u8..2, 0_u32..8), 0..=6),
            middle in proptest::collection::vec(0_u8..4, 7),
            middle_edges in proptest::collection::vec(
                (0_u32..7, 0_u32..7, 0_u8..2, proptest::sample::select(vec![0_u32, 1, 2, 3, 9, 10, 11, 20])),
                0..=6,
            ),
            last in proptest::collection::vec(0_u8..4, 7),
            last_edges in proptest::collection::vec(
                (0_u32..7, 0_u32..7, 0_u8..2, proptest::sample::select(vec![0_u32, 1, 2, 3, 9, 10, 11, 20])),
                0..=6,
            ),
            owners in proptest::collection::vec(0_u8..3, 7),
            allocation in 0_u8..3,
        ) {
            let graph = generated(&anchors, &edges, false);
            let partition = generated_partition(&owners, allocation);
            let middle = generated(&middle, &middle_edges, false);
            let last = generated(&last, &last_edges, false);
            let first = delta_to_production(&reference::diff(&graph, &middle));
            let second = delta_to_production(&reference::diff(&middle, &last));
            let production_partition = prod_partition(&partition);
            let held = partition_graph(&graph_to_production(&graph), &production_partition)
                .expect("split");
            if let Ok(after_first) = apply_segmented(&first, &held, &production_partition) {
                proptest::prop_assert_eq!(
                    applied_bytes(&first.invert(), &after_first, &production_partition),
                    Some(held.iter().map(|s| bytes(s, &production_partition)).collect())
                );
                if let (Ok(after_second), Ok(composite)) = (
                    apply_segmented(&second, &after_first, &production_partition),
                    first.compose(&second),
                ) {
                    proptest::prop_assert_eq!(
                        applied_bytes(&composite, &held, &production_partition),
                        Some(
                            after_second
                                .iter()
                                .map(|s| bytes(s, &production_partition))
                                .collect()
                        )
                    );
                }
            }
        }
    }
}

/// Segmented table edits against the reference: the law of in-memory
/// segmented table application, its checkpointed run, and its composition
/// and inversion.
mod segmented_table_apply {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};

    use panproto_inst::identity::Digest;
    use panproto_inst::segment::{
        AnySegment, ApplyRun, ArtifactKey, Budget, FamilyIndex, FreshIds, IdKind, MemoryStore,
        Partition, Phase, Residual, Resource, RunIncompleteReason, RunSpec, RunStatus,
        SegmentError, SegmentId, SegmentManifest, SegmentSource, SegmentStore, SegmentedApplyError,
        TableSegment, apply_run, apply_segmented, assemble_table, partition_table,
        read_emitted_segment, verify_manifest,
    };
    use panproto_inst::table_delta::TableDelta;
    use panproto_inst::{RowId, ViewRef};
    use panproto_reference::enumerate::segments as carrier;
    use panproto_reference::segment as segments;

    use super::segmented_apply::{
        CountingSource, FailingStore, RunOutcome, SCHEMA, Seen, prod_partition, seal_family,
        sealed_nothing, seen_production_by, seen_reference_by, shard,
    };
    use super::{
        TableApplyObs, production_table_apply_error, reference, reference_model,
        reference_table_apply_error, table_delta_to_production, table_to_production,
        table_to_reference,
    };

    /// A refusal as both implementations project it.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Refusal {
        /// A whole-state refusal.
        Edit(TableApplyObs),
        /// An added row no range owns.
        Unowned(u32),
    }

    fn production_refusal(error: &SegmentedApplyError) -> Refusal {
        match error {
            SegmentedApplyError::Edit(error) => Refusal::Edit(production_table_apply_error(error)),
            SegmentedApplyError::Segment(SegmentError::Unowned {
                elem: ViewRef::Row(row),
            }) => Refusal::Unowned(row.0),
            other => panic!("a refusal outside the carrier: {other:?}"),
        }
    }

    fn reference_refusal(
        error: &segments::SegmentedApplyRefusal<reference::TableApplyError>,
    ) -> Refusal {
        match error {
            segments::SegmentedApplyRefusal::Edit(error) => {
                Refusal::Edit(reference_table_apply_error(error))
            }
            segments::SegmentedApplyRefusal::Segment(segments::SegmentError::Unowned {
                elem: segments::ViewRef::Row(row),
            }) => Refusal::Unowned(*row),
            segments::SegmentedApplyRefusal::Segment(other) => {
                panic!("a reference refusal outside the carrier: {other:?}")
            }
        }
    }

    /// The canonical bytes of a production segment.
    fn bytes(segment: &TableSegment, partition: &Partition) -> Vec<u8> {
        AnySegment::Table(segment.clone())
            .canonical_bytes(&Digest(SCHEMA), partition)
            .expect("production segment bytes")
    }

    /// The canonical bytes of a reference segment.
    fn reference_bytes(
        segment: &segments::TableSegment,
        partition: &segments::Partition,
    ) -> Vec<u8> {
        segments::canonical_segment_bytes(
            &segments::AnySegment::Table(segment.clone()),
            segments::Digest(SCHEMA),
            partition,
        )
        .expect("reference segment bytes")
    }

    /// The production split of a reference table.
    fn split(table: &reference_model::Table, partition: &Partition) -> Vec<TableSegment> {
        partition_table(&table_to_production(table), partition).expect("production split")
    }

    /// What one case showed.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Outcome {
        /// Both applied alike.
        Applied,
        /// Both refused alike with a whole-state refusal.
        Edit,
        /// Both refused alike laying the edited table out.
        Layout,
    }

    /// Check the law on one case: the read set equals the reference's and
    /// no segment outside it changes; the output segments equal the
    /// reference's byte for byte and glue to whole-state application, which
    /// equals the reference's; or both refuse alike, whole-state
    /// application refusing alike when the refusal is an edit refusal.
    fn check_case(
        table: &reference_model::Table,
        partition: &segments::Partition,
        delta: &reference::TableDelta,
    ) -> Outcome {
        let production_partition = prod_partition(partition);
        let whole = table_to_production(table);
        let held = partition_table(&whole, &production_partition).expect("production split");
        let reference_held = segments::partition_table(table, partition).expect("reference split");
        let production_delta = table_delta_to_production(delta);
        let any: Vec<AnySegment> = held.iter().cloned().map(AnySegment::Table).collect();
        let family = FamilyIndex::of_segments(&any, &production_partition).expect("an index");
        let read = family.read_set(&production_delta);
        let expected_read: BTreeSet<u32> = segments::table_read_set(delta, partition)
            .into_iter()
            .map(|segment| segment.0)
            .collect();
        assert_eq!(
            read.iter()
                .map(|segment| segment.0)
                .collect::<BTreeSet<_>>(),
            expected_read,
            "read set of {delta:?} on {table:?} under {partition:?}"
        );
        let production = apply_segmented(&production_delta, &held, &production_partition);
        let expected = segments::apply_table_segmented(delta, &reference_held, partition);
        let mut whole_state = whole;
        let applied = production_delta.apply(&mut whole_state);
        match (production, expected) {
            (Ok(output), Ok(expected)) => {
                assert_eq!(output.len(), expected.len());
                for ((segment, expected), before) in
                    output.iter().zip(&expected).zip(&reference_held)
                {
                    assert_eq!(
                        bytes(segment, &production_partition),
                        reference_bytes(expected, partition),
                        "segment {} of {delta:?} on {table:?} under {partition:?}",
                        segment.segment.0
                    );
                    if !read.contains(&segment.segment) {
                        assert_eq!(expected, before, "an unread segment changes");
                    }
                }
                applied.expect("whole-state application accepts");
                let glued = assemble_table(output, &production_partition).expect("glues");
                assert_eq!(table_to_reference(&glued), table_to_reference(&whole_state));
                assert_eq!(
                    table_to_reference(&whole_state),
                    reference::apply_table(delta, table).expect("reference applies")
                );
                Outcome::Applied
            }
            (Err(error), Err(expected)) => {
                let refusal = production_refusal(&error);
                assert_eq!(
                    refusal,
                    reference_refusal(&expected),
                    "refusal of {delta:?} on {table:?} under {partition:?}"
                );
                if let Refusal::Edit(obs) = &refusal {
                    assert_eq!(
                        applied.map_err(|error| production_table_apply_error(&error)),
                        Err(obs.clone())
                    );
                    Outcome::Edit
                } else {
                    applied.expect("whole-state application accepts");
                    Outcome::Layout
                }
            }
            (production, expected) => panic!(
                "segmented table apply disagrees on {delta:?} on {table:?} under {partition:?}: {production:?} / {expected:?}"
            ),
        }
    }

    /// Every case of the segmented table edit carrier, or the share of
    /// `PANPROTO_TEST_SHARD`.
    #[test]
    fn segmented_table_apply_agrees_with_the_reference_on_every_case_of_the_carrier() {
        let (tables, partitions, deltas, total) = carrier::segmented_table_apply_case_counts();
        assert_eq!(
            panproto_reference::enumerate::edits::tables().len() as u64,
            tables
        );
        assert_eq!(
            carrier::segmented_table_apply_partitions().len() as u64,
            partitions
        );
        assert_eq!(
            carrier::segmented_table_deltas(&[0, 2, 3]).len() as u64,
            deltas
        );
        assert_eq!(total, 85_785);
        let (index, count) = shard();
        let mut walked = 0;
        let mut outcomes: BTreeMap<Outcome, u64> = BTreeMap::new();
        for (position, case) in carrier::segmented_table_apply_cases().enumerate() {
            walked += 1;
            if position % count == index {
                *outcomes
                    .entry(check_case(&case.table, &case.partition, &case.delta))
                    .or_insert(0) += 1;
            }
        }
        println!("shard {index}/{count}: {outcomes:?}");
        assert_eq!(walked, total);
        assert_eq!(outcomes.len(), 3, "every outcome occurs: {outcomes:?}");
        if count == 1 {
            assert_eq!(
                outcomes,
                BTreeMap::from([
                    (Outcome::Applied, 19_375),
                    (Outcome::Edit, 65_360),
                    (Outcome::Layout, 1_050)
                ])
            );
        }
    }

    /// The sealed source family of one table under one partition.
    struct Input {
        manifest: SegmentManifest,
        bytes: BTreeMap<SegmentId, Vec<u8>>,
        partition: Partition,
        held: Vec<TableSegment>,
        reference: Vec<segments::TableSegment>,
    }

    fn input(table: &reference_model::Table, partition: &segments::Partition) -> Input {
        let production_partition = prod_partition(partition);
        let held = split(table, &production_partition);
        let any: Vec<AnySegment> = held.iter().cloned().map(AnySegment::Table).collect();
        let (manifest, bytes) = seal_family(&any, &production_partition);
        Input {
            manifest,
            bytes,
            partition: production_partition,
            held,
            reference: segments::partition_table(table, partition).expect("reference split"),
        }
    }

    fn seen_production(run: &ApplyRun) -> Seen {
        seen_production_by(&run.report, &|error| {
            format!("{:?}", production_refusal(error))
        })
    }

    fn seen_reference(
        run: &segments::ApplyRun<segments::TableSegment, reference::TableApplyError>,
    ) -> Seen {
        seen_reference_by(&run.report, &|error| {
            format!("{:?}", reference_refusal(error))
        })
    }

    fn production_run<Src: SegmentSource, St: SegmentStore>(
        delta: &TableDelta,
        input: &Input,
        source: &Src,
        budget: Budget,
        store: &mut St,
        cancel: Option<u32>,
    ) -> ApplyRun {
        apply_run(
            delta,
            &input.manifest,
            source,
            RunSpec {
                lens: Digest([0; 32]),
                input: input.manifest.digest(),
                output_schema: Digest([0; 32]),
                cancel_after_tasks: cancel,
            },
            budget,
            store,
            &mut |_| {},
        )
    }

    fn reference_run(
        delta: &reference::TableDelta,
        input: &Input,
        partition: &segments::Partition,
        budget: Budget,
        store: &mut segments::MemoryStore,
        cancel: Option<u32>,
    ) -> segments::ApplyRun<segments::TableSegment, reference::TableApplyError> {
        segments::segmented_table_apply_with_progress(
            delta,
            &input.reference,
            partition,
            &segments::ApplyRunSpec {
                schema: segments::Digest(SCHEMA),
                binding: None,
                cancel_after_tasks: cancel,
            },
            segments::Budget {
                working_set: budget.working_set,
                complement: budget.complement,
                summary: budget.summary,
                affected_segments: budget.affected_segments,
            },
            store,
            &mut |_| {},
        )
    }

    /// Check the run law on one case under `budget`, as the graph run law
    /// is checked: the production report equals the reference runner's,
    /// uninterrupted and, when `every_point`, at every cancellation point
    /// and after resuming; a complete run reads each segment of the read
    /// set once and no other, emits the reference's edited segments and
    /// seals the reference's manifest; a refused or declined run seals
    /// nothing. It returns how the run ended and the cancellation points
    /// checked.
    fn check_run(
        table: &reference_model::Table,
        partition: &segments::Partition,
        delta: &reference::TableDelta,
        budget: Budget,
        every_point: bool,
    ) -> (RunOutcome, u64) {
        let input = input(table, partition);
        let production_delta = table_delta_to_production(delta);
        let source = CountingSource {
            bytes: input.bytes.clone(),
            reads: RefCell::new(BTreeMap::new()),
        };
        let mut store = MemoryStore::default();
        let baseline = production_run(&production_delta, &input, &source, budget, &mut store, None);
        let mut reference_store = segments::MemoryStore::default();
        let expected = reference_run(delta, &input, partition, budget, &mut reference_store, None);
        assert_eq!(
            seen_production(&baseline),
            seen_reference(&expected),
            "run of {delta:?} on {table:?} under {partition:?}"
        );
        let run = baseline.run.expect("a run identity");
        assert_eq!(run.0, expected.run.0, "run identity");
        let outcome = match &baseline.report.status {
            RunStatus::Complete { manifest } => {
                let read: BTreeSet<SegmentId> = expected
                    .read
                    .iter()
                    .map(|segment| SegmentId(segment.0))
                    .collect();
                assert_eq!(
                    source.reads.borrow().clone(),
                    read.iter()
                        .map(|segment| (*segment, 1))
                        .collect::<BTreeMap<_, _>>(),
                    "a complete run reads each segment of its read set once and no other"
                );
                let sealed = baseline.manifest.as_ref().expect("a manifest");
                assert_eq!(verify_manifest(sealed, *manifest), Ok(()));
                assert_eq!(
                    sealed.canonical_bytes(),
                    segments::canonical_manifest_bytes(
                        expected.manifest.as_ref().expect("a reference manifest")
                    ),
                    "the sealed manifest of {delta:?} on {table:?}"
                );
                let output = expected.output.as_ref().expect("the edited family");
                for segment in &read {
                    let (emitted, complement) =
                        read_emitted_segment(&store, run, &input.partition, *segment)
                            .expect("an emitted segment");
                    assert_eq!(complement, Vec::<u8>::new());
                    assert_eq!(
                        emitted
                            .canonical_bytes(&Digest(SCHEMA), &input.partition)
                            .expect("bytes"),
                        reference_bytes(&output[segment.0 as usize], partition)
                    );
                }
                RunOutcome::Complete
            }
            RunStatus::Failed(_) => {
                assert!(sealed_nothing(&store, run), "a refused run seals nothing");
                RunOutcome::Refused
            }
            RunStatus::Residual(_) => {
                assert!(sealed_nothing(&store, run), "a declined run seals nothing");
                RunOutcome::Declined
            }
            RunStatus::Incomplete { reason } => panic!("an uninterrupted run stopped: {reason:?}"),
        };
        let points = if every_point {
            check_cancellations(&input, partition, delta, budget, &baseline)
        } else {
            0
        };
        (outcome, points)
    }

    /// Stopping the run at every cancellation point and resuming it reports
    /// what the reference runner reports, and ends as `baseline`.
    fn check_cancellations(
        input: &Input,
        partition: &segments::Partition,
        delta: &reference::TableDelta,
        budget: Budget,
        baseline: &ApplyRun,
    ) -> u64 {
        let production_delta = table_delta_to_production(delta);
        let run = baseline.run.expect("a run identity");
        let (total, done) = (
            baseline.report.progress.tasks_total,
            baseline.report.progress.tasks_done,
        );
        for cancel in 0..=total {
            let mut store = MemoryStore::default();
            let stopped = production_run(
                &production_delta,
                input,
                &input.bytes,
                budget,
                &mut store,
                Some(cancel),
            );
            let mut reference_store = segments::MemoryStore::default();
            let expected_stop = reference_run(
                delta,
                input,
                partition,
                budget,
                &mut reference_store,
                Some(cancel),
            );
            assert_eq!(
                seen_production(&stopped),
                seen_reference(&expected_stop),
                "run cancelled after {cancel} tasks"
            );
            let resumed = production_run(
                &production_delta,
                input,
                &input.bytes,
                budget,
                &mut store,
                None,
            );
            let again = reference_run(delta, input, partition, budget, &mut reference_store, None);
            assert_eq!(
                seen_production(&resumed),
                seen_reference(&again),
                "run resumed after {cancel} tasks"
            );
            assert_eq!(resumed.report.tasks_reused, cancel.min(done));
            assert_eq!(resumed.manifest, baseline.manifest);
            assert_eq!(
                store
                    .staged_uncommitted(&run)
                    .unwrap_or_else(|never| match never {}),
                Vec::<ArtifactKey>::new()
            );
        }
        u64::from(total) + 1
    }

    /// Every 31st case of the segmented table edit carrier, run as a
    /// checkpointed run from every cancellation point against the reference
    /// runner.
    #[test]
    fn segmented_table_apply_run_matches_the_reference_runner_at_every_cancellation_point() {
        let mut outcomes: BTreeMap<RunOutcome, u64> = BTreeMap::new();
        let mut points = 0;
        for case in carrier::segmented_table_apply_cases().step_by(31) {
            let (outcome, checked) = check_run(
                &case.table,
                &case.partition,
                &case.delta,
                Budget::unlimited(),
                true,
            );
            *outcomes.entry(outcome).or_insert(0) += 1;
            points += checked;
        }
        println!("{outcomes:?}, {points} cancellation points");
        assert_eq!(outcomes.values().sum::<u64>(), 2_768);
        assert_eq!(points, 20_080);
        assert!(
            outcomes.contains_key(&RunOutcome::Complete)
                && outcomes.contains_key(&RunOutcome::Refused),
            "{outcomes:?}"
        );
    }

    /// The `a -e-> a` edge of the witnesses.
    fn edge() -> reference_model::SEdge {
        reference_model::SEdge::new("a", "a", "e", None)
    }

    /// The witness family: rows 0 to 3 in table `a`, one per segment, and
    /// the pairs 0 to 1 and 1 to 2 crossing segments; each segment owns its
    /// row and allocates row ids from `[10 + 10 i, 19 + 10 i]`.
    fn witness() -> (reference_model::Table, segments::Partition) {
        let mut table = reference_model::Table::default();
        for id in 0..4 {
            table.rows.insert(id, ("a".to_owned(), BTreeMap::new()));
        }
        table.pairs.insert((edge(), 0, 1));
        table.pairs.insert((edge(), 1, 2));
        let partition = segments::Partition {
            shape: segments::InstanceShapeKind::Table,
            specs: (0..4)
                .map(|id| segments::SegmentSpec {
                    id: segments::SegmentId(id),
                    nodes: vec![
                        segments::IdRange { lo: id, hi: id },
                        segments::IdRange {
                            lo: 10 + 10 * id,
                            hi: 19 + 10 * id,
                        },
                    ],
                    alloc_nodes: Some(segments::IdRange {
                        lo: 10 + 10 * id,
                        hi: 19 + 10 * id,
                    }),
                    alloc_edges: None,
                })
                .collect(),
        };
        (table, partition)
    }

    /// The delta changing row `row`'s table from `old` to `new`.
    fn table_change(row: u32, old: &str, new: &str) -> reference::TableDelta {
        let mut delta = reference::TableDelta::identity();
        delta.attrs.insert(
            reference::TKey::Table(row),
            (
                reference::TVal::Table(old.to_owned()),
                reference::TVal::Table(new.to_owned()),
            ),
        );
        delta
    }

    #[test]
    fn a_segmented_table_apply_reads_only_the_owners_of_the_rows_it_names() {
        let (table, partition) = witness();
        let input = input(&table, &partition);
        // A cell added to row 3 reads segment 3 alone; moving row 3, which
        // no pair meets, to table `b` reads segment 3 alone, no boundary
        // peer; adding the pair 3 to 0 reads its two endpoints' segments;
        // adding a row under a fresh id of segment 2 with a pair from it to
        // row 1 reads segments 1 and 2.
        let mut cell = reference::TableDelta::identity();
        cell.attrs.insert(
            reference::TKey::Cell(3, "f".to_owned()),
            (
                reference::TVal::Cell(None),
                reference::TVal::Cell(Some(panproto_reference::value::Value::Int(0))),
            ),
        );
        let mut pair = reference::TableDelta::identity();
        pair.add.insert(
            reference::TElem::Pair(edge(), 3, 0),
            reference::TPayload::Pair,
        );
        let mut fresh = FreshIds::of(
            &AnySegment::Table(input.held[2].clone()),
            &input.partition.specs()[2],
        )
        .expect("ids");
        let row = fresh.node().expect("a fresh row id");
        assert_eq!(row, 30);
        let mut added = reference::TableDelta::identity();
        added.add.insert(
            reference::TElem::Row(row),
            reference::TPayload::Row("a".to_owned(), BTreeMap::new()),
        );
        added.add.insert(
            reference::TElem::Pair(edge(), row, 1),
            reference::TPayload::Pair,
        );
        for (delta, read) in [
            (cell, vec![3]),
            (table_change(3, "a", "b"), vec![3]),
            (pair, vec![0, 3]),
            (added, vec![1, 2]),
        ] {
            let production = table_delta_to_production(&delta);
            let source = CountingSource {
                bytes: input.bytes.clone(),
                reads: RefCell::new(BTreeMap::new()),
            };
            let mut store = MemoryStore::default();
            let run = production_run(
                &production,
                &input,
                &source,
                Budget::unlimited(),
                &mut store,
                None,
            );
            let sealed = run.manifest.expect("the run completes");
            let expected: BTreeMap<SegmentId, usize> = read
                .iter()
                .map(|segment| (SegmentId(*segment), 1))
                .collect();
            assert_eq!(*source.reads.borrow(), expected, "{delta:?}");
            let total = u32::try_from(4 * read.len() + 2).expect("a small count");
            assert_eq!(run.report.progress.tasks_total, total);
            for (before, after) in input.manifest.segments.iter().zip(&sealed.segments) {
                if !expected.contains_key(&before.id) {
                    assert_eq!(before, after, "an unread segment keeps its record");
                }
            }
            assert_eq!(sealed.instance_digest, None);
            assert_eq!(
                check_run(&table, &partition, &delta, Budget::unlimited(), true).0,
                RunOutcome::Complete
            );
        }
    }

    #[test]
    fn a_table_change_at_a_row_with_pairs_is_refused_from_its_owner_alone() {
        let (table, partition) = witness();
        let input = input(&table, &partition);
        // Moving row 1 to table `b` leaves the pairs 0 to 1 and 1 to 2 off
        // their edge's tables: the refusal names the first, read from row
        // 1's in-stub, and the run reads segment 1 alone.
        let delta = table_change(1, "a", "b");
        let source = CountingSource {
            bytes: input.bytes.clone(),
            reads: RefCell::new(BTreeMap::new()),
        };
        let mut store = MemoryStore::default();
        let run = production_run(
            &table_delta_to_production(&delta),
            &input,
            &source,
            Budget::unlimited(),
            &mut store,
            None,
        );
        match &run.report.status {
            RunStatus::Failed(error) => assert_eq!(
                production_refusal(error),
                Refusal::Edit(TableApplyObs::MissingRowEndpoint(
                    format!("{:?}", edge()),
                    1
                ))
            ),
            other => panic!("the delta is refused: {other:?}"),
        }
        assert_eq!(*source.reads.borrow(), BTreeMap::from([(SegmentId(1), 1)]));
        assert!(sealed_nothing(&store, run.run.expect("a run identity")));
        assert_eq!(
            check_run(&table, &partition, &delta, Budget::unlimited(), true).0,
            RunOutcome::Refused
        );
    }

    #[test]
    fn a_refused_table_delta_fails_at_assemble_with_the_whole_state_refusal_and_seals_nothing() {
        let (table, partition) = witness();
        let input = input(&table, &partition);
        // Deleting row 1 leaves the pairs 0 to 1 and 1 to 2.
        let mut delete = reference::TableDelta::identity();
        delete.del.insert(
            reference::TElem::Row(1),
            reference::TPayload::Row("a".to_owned(), BTreeMap::new()),
        );
        let mut store = MemoryStore::default();
        let run = production_run(
            &table_delta_to_production(&delete),
            &input,
            &input.bytes,
            Budget::unlimited(),
            &mut store,
            None,
        );
        let label = format!("{:?}", edge());
        match &run.report.status {
            RunStatus::Failed(error) => assert_eq!(
                production_refusal(error),
                Refusal::Edit(TableApplyObs::DanglingForeignKeys(
                    1,
                    vec![(label.clone(), 0, 1), (label, 1, 2)]
                ))
            ),
            other => panic!("the delta is refused: {other:?}"),
        }
        assert_eq!(run.report.progress.phase, Phase::Summarize);
        assert!(sealed_nothing(&store, run.run.expect("a run identity")));
        assert_eq!(
            check_run(&table, &partition, &delete, Budget::unlimited(), true).0,
            RunOutcome::Refused
        );
    }

    /// The delta adding the pair 3 to 0, which reads two segments.
    fn crossing_pair() -> reference::TableDelta {
        let mut delta = reference::TableDelta::identity();
        delta.add.insert(
            reference::TElem::Pair(edge(), 3, 0),
            reference::TPayload::Pair,
        );
        delta
    }

    #[test]
    fn a_segmented_table_apply_run_declines_each_exceeded_budget_like_the_reference() {
        let (table, partition) = witness();
        let delta = crossing_pair();
        for (budget, resource) in [
            (
                Budget {
                    affected_segments: 1,
                    ..Budget::unlimited()
                },
                Resource::AffectedSegments,
            ),
            (
                Budget {
                    working_set: 0,
                    ..Budget::unlimited()
                },
                Resource::WorkingSet,
            ),
            (
                Budget {
                    summary: 2,
                    ..Budget::unlimited()
                },
                Resource::Summary,
            ),
        ] {
            assert_eq!(
                check_run(&table, &partition, &delta, budget, true).0,
                RunOutcome::Declined
            );
            let input = input(&table, &partition);
            let mut store = MemoryStore::default();
            let run = production_run(
                &table_delta_to_production(&delta),
                &input,
                &input.bytes,
                budget,
                &mut store,
                None,
            );
            match &run.report.status {
                RunStatus::Residual(Residual::BudgetExceeded {
                    resource: found, ..
                }) => assert_eq!(*found, resource),
                other => panic!("{resource:?} declines: {other:?}"),
            }
        }
    }

    #[test]
    fn a_segmented_table_apply_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run()
     {
        let (table, partition) = witness();
        let input = input(&table, &partition);
        let delta = table_delta_to_production(&crossing_pair());
        let mut baseline_store = MemoryStore::default();
        let baseline = production_run(
            &delta,
            &input,
            &input.bytes,
            Budget::unlimited(),
            &mut baseline_store,
            None,
        );
        assert!(matches!(baseline.report.status, RunStatus::Complete { .. }));
        let total = baseline.report.progress.tasks_total;
        for fail_at in 1..=total {
            let mut store = FailingStore {
                inner: MemoryStore::default(),
                commits: 0,
                fail_at: Some(usize::try_from(fail_at).expect("a small count")),
            };
            let failed = production_run(
                &delta,
                &input,
                &input.bytes,
                Budget::unlimited(),
                &mut store,
                None,
            );
            assert!(
                matches!(
                    &failed.report.status,
                    RunStatus::Incomplete {
                        reason: RunIncompleteReason::Store { message }
                    } if message == "commit failed"
                ),
                "{:?}",
                failed.report.status
            );
            assert_eq!(failed.report.progress.tasks_done, fail_at - 1);
            assert_eq!(failed.report.orphans.len(), 1, "the failed task's artifact");
            store.fail_at = None;
            let resumed = production_run(
                &delta,
                &input,
                &input.bytes,
                Budget::unlimited(),
                &mut store,
                None,
            );
            assert_eq!(resumed.report.tasks_reused, fail_at - 1);
            assert_eq!(resumed.report.orphans, Vec::<ArtifactKey>::new());
            assert_eq!(resumed.manifest, baseline.manifest);
        }
    }

    #[test]
    fn fresh_row_ids_come_from_the_owners_range_and_are_never_refused() {
        let (table, mut partition) = witness();
        partition.specs[3].alloc_nodes = None;
        let input = input(&table, &partition);
        // Segment 3 has no row allocation range, and no segment of a table
        // has an edge allocation range.
        let mut three = FreshIds::of(
            &AnySegment::Table(input.held[3].clone()),
            &input.partition.specs()[3],
        )
        .expect("ids");
        assert_eq!(
            three.node(),
            Err(SegmentError::NoAllocationRange {
                segment: SegmentId(3),
                kind: IdKind::Node,
            })
        );
        let mut two = FreshIds::of(
            &AnySegment::Table(input.held[2].clone()),
            &input.partition.specs()[2],
        )
        .expect("ids");
        assert_eq!(
            two.edge(),
            Err(SegmentError::NoAllocationRange {
                segment: SegmentId(2),
                kind: IdKind::Edge,
            })
        );
        // Two fresh rows of segment 2, a pair between them and one to row
        // 0, apply.
        let (first, second) = (
            RowId(two.node().expect("an id")),
            RowId(two.node().expect("an id")),
        );
        assert_eq!((first, second), (RowId(30), RowId(31)));
        let label = super::edge_to_production(&edge());
        let delta = TableDelta::identity()
            .with_added_row(first, "a", std::collections::HashMap::new())
            .with_added_row(second, "a", std::collections::HashMap::new())
            .with_added_pair(label.clone(), first, second)
            .with_added_pair(label, second, RowId(0));
        let output = apply_segmented(&delta, &input.held, &input.partition).expect("applies");
        let glued = assemble_table(output, &input.partition).expect("glues");
        assert_eq!(glued.incident_pairs(second).len(), 2);
    }

    /// A row of a generated table: `0` in table `a`, `1` in `b`, `2` in `a`
    /// with the cell `f = 0`, `3` absent.
    fn generated_row(
        code: u8,
    ) -> Option<(String, BTreeMap<String, panproto_reference::value::Value>)> {
        match code {
            0 => Some(("a".to_owned(), BTreeMap::new())),
            1 => Some(("b".to_owned(), BTreeMap::new())),
            2 => Some((
                "a".to_owned(),
                BTreeMap::from([("f".to_owned(), panproto_reference::value::Value::Int(0))]),
            )),
            _ => None,
        }
    }

    /// The table whose rows `0 .. rows.len()` are [`generated_row`] of
    /// `rows` and whose pairs are `pairs` between present rows, each under
    /// the edge `e` between its endpoints' tables.
    fn generated(rows: &[u8], pairs: &[(u32, u32)]) -> reference_model::Table {
        let mut table = reference_model::Table::default();
        for (id, code) in (0_u32..).zip(rows) {
            if let Some(row) = generated_row(*code) {
                table.rows.insert(id, row);
            }
        }
        for (src, tgt) in pairs {
            if let (Some((a, _)), Some((b, _))) = (table.rows.get(src), table.rows.get(tgt)) {
                let edge = reference_model::SEdge::new(a, b, "e", None);
                table.pairs.insert((edge, *src, *tgt));
            }
        }
        table
    }

    /// The table partition assigning row id `i` to segment `owners[i]`
    /// (renumbered by first occurrence, `3` for no segment).
    fn generated_partition(owners: &[u8]) -> segments::Partition {
        let mut renamed: BTreeMap<u8, u32> = BTreeMap::new();
        let mut owned = Vec::new();
        for owner in owners {
            if *owner == 3 {
                owned.push(None);
            } else {
                let next = u32::try_from(renamed.len()).expect("a small count");
                owned.push(Some(*renamed.entry(*owner).or_insert(next)));
            }
        }
        let segments = u32::try_from(renamed.len()).expect("a small count");
        segments::Partition {
            shape: segments::InstanceShapeKind::Table,
            specs: (0..segments)
                .map(|id| segments::SegmentSpec {
                    id: segments::SegmentId(id),
                    nodes: (0_u32..)
                        .zip(&owned)
                        .filter(|(_, owner)| **owner == Some(id))
                        .map(|(row, _)| segments::IdRange { lo: row, hi: row })
                        .collect(),
                    alloc_nodes: None,
                    alloc_edges: None,
                })
                .collect(),
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

        /// The law on generated families and deltas: the delta is the
        /// reference difference between the input and a second generated
        /// table, whose rows may change table or be owned by no segment.
        #[test]
        fn segmented_table_apply_agrees_with_the_reference_on_generated_families(
            rows in proptest::collection::vec(0_u8..3, 1..=6),
            pairs in proptest::collection::vec((0_u32..6, 0_u32..6), 0..=8),
            after_rows in proptest::collection::vec(0_u8..4, 8),
            after_pairs in proptest::collection::vec((0_u32..8, 0_u32..8), 0..=8),
            owners in proptest::collection::vec(0_u8..3, 6),
            spare in proptest::collection::vec(0_u8..4, 2),
        ) {
            let owners: Vec<u8> = owners.into_iter().chain(spare).collect();
            let table = generated(&rows, &pairs);
            let partition = generated_partition(&owners);
            let after = generated(&after_rows, &after_pairs);
            check_case(&table, &partition, &reference::diff_table(&table, &after));
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(32))]

        /// The run law on generated families and deltas, at every
        /// cancellation point.
        #[test]
        fn segmented_table_apply_run_matches_the_reference_runner_on_generated_families(
            rows in proptest::collection::vec(0_u8..3, 1..=5),
            pairs in proptest::collection::vec((0_u32..5, 0_u32..5), 0..=6),
            after_rows in proptest::collection::vec(0_u8..4, 6),
            after_pairs in proptest::collection::vec((0_u32..6, 0_u32..6), 0..=6),
            owners in proptest::collection::vec(0_u8..3, 5),
            spare in 0_u8..4,
        ) {
            let owners: Vec<u8> = owners.into_iter().chain([spare]).collect();
            let table = generated(&rows, &pairs);
            let partition = generated_partition(&owners);
            let after = generated(&after_rows, &after_pairs);
            check_run(
                &table,
                &partition,
                &reference::diff_table(&table, &after),
                Budget::unlimited(),
                true,
            );
        }
    }

    /// The output of in-memory segmented table apply, as canonical bytes
    /// per segment.
    fn applied_bytes(
        delta: &TableDelta,
        held: &[TableSegment],
        partition: &Partition,
    ) -> Option<Vec<Vec<u8>>> {
        apply_segmented(delta, held, partition)
            .ok()
            .map(|output| output.iter().map(|s| bytes(s, partition)).collect())
    }

    /// What one ordered pair of the composition carrier showed.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum PairOutcome {
        /// Both deltas applied in turn and their composite applied to the
        /// same family.
        Composed,
        /// Both applied in turn and their composite is undefined.
        Uncomposable,
        /// The first delta is refused.
        FirstRefused,
        /// The first applies and the second is refused on its result.
        SecondRefused,
    }

    /// Composition over segments: for every ordered pair of deltas of the
    /// table composition carrier on every family of it, when the first
    /// applies segment by segment, the second applies to its result, and
    /// their composite is defined, the composite applies to the family and
    /// gives the same segments byte for byte, as the reference composite
    /// does; and on this carrier the composite of two deltas that apply in
    /// turn is always defined.
    #[test]
    fn a_composite_table_delta_applies_to_a_segmented_family_as_its_factors_do_in_turn() {
        let (families, deltas, pairs) = carrier::segmented_table_compose_counts();
        let family_cases = carrier::segmented_table_compose_families();
        let delta_cases = carrier::segmented_table_deltas(&[0, 3]);
        assert_eq!(family_cases.len() as u64, families);
        assert_eq!(delta_cases.len() as u64, deltas);
        let mut outcomes: BTreeMap<PairOutcome, u64> = BTreeMap::new();
        for (table, partition) in &family_cases {
            let production_partition = prod_partition(partition);
            let held = split(table, &production_partition);
            let reference_held = segments::partition_table(table, partition).expect("split");
            for first in &delta_cases {
                let a = table_delta_to_production(first);
                let after_first = apply_segmented(&a, &held, &production_partition);
                for second in &delta_cases {
                    let b = table_delta_to_production(second);
                    let composite = a.compose(&b);
                    assert_eq!(
                        composite.is_ok(),
                        reference::compose_table(first, second).is_ok(),
                        "composability of {first:?} and {second:?}"
                    );
                    let outcome = after_first.as_ref().map_or(
                        PairOutcome::FirstRefused,
                        |middle| match (
                            apply_segmented(&b, middle, &production_partition),
                            &composite,
                        ) {
                            (Err(_), _) => PairOutcome::SecondRefused,
                            (Ok(_), Err(_)) => PairOutcome::Uncomposable,
                            (Ok(last), Ok(composite)) => {
                                let expected: Vec<Vec<u8>> = last
                                    .iter()
                                    .map(|s| bytes(s, &production_partition))
                                    .collect();
                                assert_eq!(
                                    applied_bytes(composite, &held, &production_partition),
                                    Some(expected.clone()),
                                    "{first:?} then {second:?} on {table:?} under {partition:?}"
                                );
                                let reference_composite = reference::compose_table(first, second)
                                    .expect("the reference composes");
                                let reference = segments::apply_table_segmented(
                                    &reference_composite,
                                    &reference_held,
                                    partition,
                                )
                                .expect("the reference composite applies");
                                assert_eq!(
                                    reference
                                        .iter()
                                        .map(|s| reference_bytes(s, partition))
                                        .collect::<Vec<_>>(),
                                    expected
                                );
                                PairOutcome::Composed
                            }
                        },
                    );
                    *outcomes.entry(outcome).or_insert(0) += 1;
                }
            }
        }
        println!("{outcomes:?}");
        assert_eq!(outcomes.values().sum::<u64>(), pairs);
        assert_eq!(pairs, 62_500);
        assert_eq!(outcomes[&PairOutcome::Composed], 4_068);
        assert_eq!(
            outcomes.keys().copied().collect::<Vec<_>>(),
            vec![
                PairOutcome::Composed,
                PairOutcome::FirstRefused,
                PairOutcome::SecondRefused
            ],
            "{outcomes:?}"
        );
    }

    /// Inversion over segments, in memory: on every case of the segmented
    /// table edit carrier that applies, the inverse delta applied
    /// segment by segment to the output restores every input segment byte
    /// for byte, as it does in the reference.
    #[test]
    fn the_inverse_of_an_applied_segmented_table_edit_restores_every_segment_byte_for_byte() {
        let mut restored = 0_u64;
        for case in carrier::segmented_table_apply_cases() {
            let production_partition = prod_partition(&case.partition);
            let held = split(&case.table, &production_partition);
            let delta = table_delta_to_production(&case.delta);
            let Ok(output) = apply_segmented(&delta, &held, &production_partition) else {
                continue;
            };
            assert_eq!(
                applied_bytes(&delta.invert(), &output, &production_partition),
                Some(
                    held.iter()
                        .map(|s| bytes(s, &production_partition))
                        .collect()
                ),
                "the inverse of {:?} on {:?}",
                case.delta,
                case.table
            );
            let reference_held =
                segments::partition_table(&case.table, &case.partition).expect("split");
            let reference_output =
                segments::apply_table_segmented(&case.delta, &reference_held, &case.partition)
                    .expect("the reference applies");
            let back = segments::apply_table_segmented(
                &reference::invert_table(&case.delta),
                &reference_output,
                &case.partition,
            )
            .expect("the reference inverse applies");
            assert_eq!(back, reference_held);
            restored += 1;
        }
        println!("{restored} restored");
        assert_eq!(restored, 19_375);
    }

    /// Inversion over segments, as checkpointed runs: on every 31st case of
    /// the carrier whose run completes, running the inverse delta over the
    /// output manifest, with the emitted segments in place of the read ones,
    /// seals the input manifest with its instance digest cleared.
    #[test]
    fn the_inverse_run_of_a_completed_segmented_table_edit_restores_the_manifest_but_its_instance_digest()
     {
        let mut restored = 0_u64;
        for case in carrier::segmented_table_apply_cases().step_by(31) {
            let input = input(&case.table, &case.partition);
            let delta = table_delta_to_production(&case.delta);
            let mut store = MemoryStore::default();
            let forward = production_run(
                &delta,
                &input,
                &input.bytes,
                Budget::unlimited(),
                &mut store,
                None,
            );
            let (Some(edited), Some(run)) = (forward.manifest, forward.run) else {
                continue;
            };
            let mut bytes_after = input.bytes.clone();
            for spec in input.partition.specs() {
                if let Ok((emitted, _)) =
                    read_emitted_segment(&store, run, &input.partition, spec.id)
                {
                    bytes_after.insert(
                        spec.id,
                        emitted
                            .canonical_bytes(&Digest(SCHEMA), &input.partition)
                            .expect("bytes"),
                    );
                }
            }
            let after = Input {
                manifest: edited,
                bytes: bytes_after,
                partition: input.partition.clone(),
                held: Vec::new(),
                reference: Vec::new(),
            };
            let back = production_run(
                &delta.invert(),
                &after,
                &after.bytes,
                Budget::unlimited(),
                &mut store,
                None,
            )
            .manifest
            .expect("the inverse run completes");
            let mut expected = input.manifest.clone();
            assert!(expected.instance_digest.is_some());
            expected.clear_instance_digest();
            assert_eq!(back.canonical_bytes(), expected.canonical_bytes());
            restored += 1;
        }
        println!("{restored} restored");
        assert_eq!(restored, 617);
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

        /// Composition and inversion over segments on generated families:
        /// the deltas are the reference differences from the input to a
        /// second table and from it to a third; when both apply segment by
        /// segment, their composite applies to the family with the same
        /// segments, and the inverse of the first restores the family byte
        /// for byte.
        #[test]
        fn table_composition_and_inversion_hold_over_segments_on_generated_families(
            rows in proptest::collection::vec(0_u8..3, 1..=5),
            pairs in proptest::collection::vec((0_u32..5, 0_u32..5), 0..=6),
            middle in proptest::collection::vec(0_u8..4, 7),
            middle_pairs in proptest::collection::vec((0_u32..7, 0_u32..7), 0..=6),
            last in proptest::collection::vec(0_u8..4, 7),
            last_pairs in proptest::collection::vec((0_u32..7, 0_u32..7), 0..=6),
            owners in proptest::collection::vec(0_u8..3, 7),
        ) {
            let table = generated(&rows, &pairs);
            let partition = generated_partition(&owners);
            let middle = generated(&middle, &middle_pairs);
            let last = generated(&last, &last_pairs);
            let first = table_delta_to_production(&reference::diff_table(&table, &middle));
            let second = table_delta_to_production(&reference::diff_table(&middle, &last));
            let production_partition = prod_partition(&partition);
            let held = split(&table, &production_partition);
            if let Ok(after_first) = apply_segmented(&first, &held, &production_partition) {
                proptest::prop_assert_eq!(
                    applied_bytes(&first.invert(), &after_first, &production_partition),
                    Some(held.iter().map(|s| bytes(s, &production_partition)).collect())
                );
                if let (Ok(after_second), Ok(composite)) = (
                    apply_segmented(&second, &after_first, &production_partition),
                    first.compose(&second),
                ) {
                    proptest::prop_assert_eq!(
                        applied_bytes(&composite, &held, &production_partition),
                        Some(
                            after_second
                                .iter()
                                .map(|s| bytes(s, &production_partition))
                                .collect()
                        )
                    );
                }
            }
        }
    }
}

mod segmented_tree_apply {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};

    use panproto_inst::edit_error::EditError;
    use panproto_inst::identity::Digest;
    use panproto_inst::segment::{
        AnySegment, ApplyRun, ArtifactKey, Budget, FamilyIndex, MemoryStore, Partition, Phase,
        Residual, Resource, RunIncompleteReason, RunSpec, RunStatus, SegmentError, SegmentId,
        SegmentManifest, SegmentSource, SegmentStore, SegmentedApplyError, TreeSegment, apply_run,
        apply_segmented, assemble_tree, partition_tree, read_emitted_segment, verify_manifest,
    };
    use panproto_inst::{GInstance, TreeEdit, TreeEmbedding, ViewRef, WInstance};
    use panproto_reference::enumerate::segments as carrier;
    use panproto_reference::segment as segments;

    use super::segmented_apply::{
        CountingSource, FailingStore, RunOutcome, SCHEMA, Seen, prod_partition, seal_family,
        sealed_nothing, seen_production_by, seen_reference_by, shard,
    };
    use super::{
        EmbeddingObs, production_embedding, reference, reference_embedding, reference_model,
        tree_edit_to_production, tree_to_production, tree_to_reference,
    };

    /// A whole-state tree refusal, as both implementations report it.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum TreeObs {
        NodeNotFound(u32),
        ParentNotFound(u32),
        Duplicate(u32),
        Root(u32),
        NotLeaf(u32),
        Cycle(u32, u32),
        InvalidJoin(u32),
        FanNotFound(String),
    }

    fn production_obs(error: &EditError) -> TreeObs {
        match error {
            EditError::NodeNotFound(id) => TreeObs::NodeNotFound(*id),
            EditError::ParentNotFound(id) => TreeObs::ParentNotFound(*id),
            EditError::DuplicateNodeId(id) => TreeObs::Duplicate(*id),
            EditError::RootOperation(id) => TreeObs::Root(*id),
            EditError::NotLeaf(id) => TreeObs::NotLeaf(*id),
            EditError::CycleDetected {
                node_id,
                new_parent,
            } => TreeObs::Cycle(*node_id, *new_parent),
            EditError::InvalidJoin(id) => TreeObs::InvalidJoin(*id),
            EditError::FanNotFound(hyper) => TreeObs::FanNotFound(hyper.clone()),
            other => panic!("a production refusal outside the carrier: {other:?}"),
        }
    }

    fn reference_obs(error: &reference::TreeApplyError) -> TreeObs {
        match error {
            reference::TreeApplyError::NodeNotFound(id) => TreeObs::NodeNotFound(*id),
            reference::TreeApplyError::ParentNotFound(id) => TreeObs::ParentNotFound(*id),
            reference::TreeApplyError::DuplicateNodeId(id) => TreeObs::Duplicate(*id),
            reference::TreeApplyError::RootOperation(id) => TreeObs::Root(*id),
            reference::TreeApplyError::NotLeaf(id) => TreeObs::NotLeaf(*id),
            reference::TreeApplyError::Cycle { node, parent } => TreeObs::Cycle(*node, *parent),
            reference::TreeApplyError::InvalidJoin(id) => TreeObs::InvalidJoin(*id),
            reference::TreeApplyError::FanNotFound(hyper) => TreeObs::FanNotFound(hyper.clone()),
            other => panic!("a reference refusal outside the carrier: {other:?}"),
        }
    }

    /// A refusal as both implementations project it.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Refusal {
        /// A whole-state refusal.
        Edit(TreeObs),
        /// The split of the edited tree is refused: the element.
        NotEmbedded(String),
        /// A node no range owns.
        Unowned(u32),
    }

    fn production_refusal(error: &SegmentedApplyError) -> Refusal {
        match error {
            SegmentedApplyError::Edit(error) => Refusal::Edit(production_obs(error)),
            SegmentedApplyError::Segment(SegmentError::NotEmbedded { elem }) => {
                Refusal::NotEmbedded(format!("{elem:?}"))
            }
            SegmentedApplyError::Segment(SegmentError::Unowned {
                elem: ViewRef::Node(node),
            }) => Refusal::Unowned(*node),
            other => panic!("a refusal outside the carrier: {other:?}"),
        }
    }

    fn reference_refusal(
        error: &segments::SegmentedApplyRefusal<reference::TreeApplyError>,
    ) -> Refusal {
        match error {
            segments::SegmentedApplyRefusal::Edit(error) => Refusal::Edit(reference_obs(error)),
            segments::SegmentedApplyRefusal::Segment(segments::SegmentError::NotEmbedded {
                elem,
            }) => Refusal::NotEmbedded(format!("{elem:?}")),
            segments::SegmentedApplyRefusal::Segment(segments::SegmentError::Unowned {
                elem: segments::ViewRef::Node(node),
            }) => Refusal::Unowned(*node),
            segments::SegmentedApplyRefusal::Segment(other) => {
                panic!("a reference refusal outside the carrier: {other:?}")
            }
        }
    }

    /// The canonical bytes of a production segment.
    fn bytes(segment: &TreeSegment, partition: &Partition) -> Vec<u8> {
        AnySegment::Tree(segment.clone())
            .canonical_bytes(&Digest(SCHEMA), partition)
            .expect("production segment bytes")
    }

    /// The canonical bytes of a reference segment.
    fn reference_bytes(
        segment: &segments::TreeSegment,
        partition: &segments::Partition,
    ) -> Vec<u8> {
        segments::canonical_segment_bytes(
            &segments::AnySegment::Tree(segment.clone()),
            segments::Digest(SCHEMA),
            partition,
        )
        .expect("reference segment bytes")
    }

    /// The production tree and embedding of a reference tree, whose
    /// embedding is the graph encoding's in both implementations.
    fn production_tree(
        tree: &reference_model::Tree,
        embedding: &panproto_reference::encode::TreeEmbedding,
    ) -> (WInstance, TreeEmbedding) {
        let whole = tree_to_production(tree);
        let (_, production) = GInstance::from_wtype_embedded(&whole, None).expect("embeds");
        assert_eq!(
            production_embedding(&production),
            reference_embedding(embedding),
            "the encodings agree on {tree:?}"
        );
        (whole, production)
    }

    /// The tree and embedding a family glues to, as observed.
    fn glued(
        segments: Vec<TreeSegment>,
        partition: &Partition,
    ) -> (reference_model::Tree, EmbeddingObs) {
        let (tree, embedding) = assemble_tree(segments, partition).expect("glues");
        (tree_to_reference(&tree), production_embedding(&embedding))
    }

    /// What one case showed.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Outcome {
        /// Both applied alike.
        Applied,
        /// Both refused alike with a whole-state refusal.
        Edit,
        /// Both refused alike laying the edited tree out.
        Layout,
    }

    /// Check the law on one case: the read set (from the segments and from
    /// the sealed manifest) equals the reference's; the output segments
    /// equal the reference's byte for byte, no segment outside the read set
    /// changes, and the output glues to whole-state application
    /// ([`TreeEmbedding::follow_edit`]) of the glued input; or both refuse
    /// alike, whole-state application refusing alike when the refusal is an
    /// edit refusal and applying when it is a layout refusal.
    fn check_case(
        tree: &reference_model::Tree,
        embedding: &panproto_reference::encode::TreeEmbedding,
        partition: &segments::Partition,
        edit: &reference::TreeEdit,
    ) -> Outcome {
        let production_partition = prod_partition(partition);
        let (whole, production_embedding_state) = production_tree(tree, embedding);
        let held = partition_tree(&whole, &production_embedding_state, &production_partition)
            .expect("production split");
        let reference_held =
            segments::partition_tree(tree, embedding, partition).expect("reference split");
        let production_edit = tree_edit_to_production(edit);
        let any: Vec<AnySegment> = held.iter().cloned().map(AnySegment::Tree).collect();
        let family = FamilyIndex::of_segments(&any, &production_partition).expect("an index");
        let read = family.read_set(&production_edit);
        let (manifest, _) = seal_family(&any, &production_partition);
        assert_eq!(
            FamilyIndex::of_manifest(&manifest)
                .expect("an index")
                .read_set(&production_edit),
            read,
            "the manifest gives the read set the segments give"
        );
        let reference_any: Vec<segments::AnySegment> = reference_held
            .iter()
            .cloned()
            .map(segments::AnySegment::Tree)
            .collect();
        let records = segments::seal_manifest(
            &reference_any,
            partition,
            segments::Digest(SCHEMA),
            segments::ManifestRole::Source,
            None,
        )
        .expect("the reference seals")
        .segments;
        let expected_read: BTreeSet<u32> = segments::tree_read_set(edit, &records, partition)
            .into_iter()
            .map(|segment| segment.0)
            .collect();
        assert_eq!(
            read.iter()
                .map(|segment| segment.0)
                .collect::<BTreeSet<_>>(),
            expected_read,
            "read set of {edit:?} on {tree:?} under {partition:?}"
        );
        let production = apply_segmented(&production_edit, &held, &production_partition);
        let expected = segments::apply_tree_segmented(edit, &reference_held, partition);
        let (mut whole_state, mut followed) =
            assemble_tree(held, &production_partition).expect("the input glues");
        let applied = followed.follow_edit(&mut whole_state, &production_edit);
        match (production, expected) {
            (Ok(output), Ok(expected)) => {
                assert_eq!(output.len(), expected.len());
                for ((segment, expected), before) in
                    output.iter().zip(&expected).zip(&reference_held)
                {
                    assert_eq!(
                        bytes(segment, &production_partition),
                        reference_bytes(expected, partition),
                        "segment {} of {edit:?} on {tree:?} under {partition:?}",
                        segment.segment.0
                    );
                    if !read.contains(&segment.segment) {
                        assert_eq!(expected, before, "an unread segment changes");
                    }
                }
                applied.expect("whole-state application accepts");
                assert_eq!(
                    glued(output, &production_partition),
                    (
                        tree_to_reference(&whole_state),
                        production_embedding(&followed)
                    ),
                    "the output glues to whole-state application"
                );
                Outcome::Applied
            }
            (Err(error), Err(expected)) => {
                let refusal = production_refusal(&error);
                assert_eq!(
                    refusal,
                    reference_refusal(&expected),
                    "refusal of {edit:?} on {tree:?} under {partition:?}"
                );
                if let Refusal::Edit(obs) = &refusal {
                    assert_eq!(
                        applied.map_err(|error| production_obs(&error)),
                        Err(obs.clone())
                    );
                    Outcome::Edit
                } else {
                    applied.expect("whole-state application accepts");
                    Outcome::Layout
                }
            }
            (production, expected) => panic!(
                "segmented tree apply disagrees on {edit:?} on {tree:?} under {partition:?}: {production:?} / {expected:?}"
            ),
        }
    }

    /// Every case of the segmented tree edit carrier, or the share of
    /// `PANPROTO_TEST_SHARD`.
    #[test]
    fn segmented_tree_apply_agrees_with_the_reference_on_every_case_of_the_carrier() {
        let (trees, partitions, edits, total) = carrier::segmented_tree_apply_case_counts();
        let carrier_trees = carrier::segmented_tree_apply_trees();
        assert_eq!(carrier_trees.len() as u64, trees);
        assert_eq!(
            carrier::segmented_tree_apply_partitions().len() as u64,
            partitions
        );
        assert_eq!(
            carrier_trees
                .iter()
                .map(|(tree, embedding)| {
                    let n = u32::try_from(tree.nodes.len()).expect("a small tree");
                    carrier::segmented_tree_edits(n, embedding.next_node).len() as u64
                })
                .sum::<u64>(),
            edits
        );
        assert_eq!(total, 11_550);
        let (index, count) = shard();
        let mut walked = 0;
        let mut outcomes: BTreeMap<Outcome, u64> = BTreeMap::new();
        for (position, case) in carrier::segmented_tree_apply_cases().enumerate() {
            walked += 1;
            if position % count == index {
                *outcomes
                    .entry(check_case(
                        &case.tree,
                        &case.embedding,
                        &case.partition,
                        &case.edit,
                    ))
                    .or_insert(0) += 1;
            }
        }
        println!("shard {index}/{count}: {outcomes:?}");
        assert_eq!(walked, total);
        assert_eq!(outcomes.len(), 3, "every outcome occurs: {outcomes:?}");
        if count == 1 {
            assert_eq!(
                outcomes,
                BTreeMap::from([
                    (Outcome::Applied, 6_762),
                    (Outcome::Edit, 4_620),
                    (Outcome::Layout, 168)
                ])
            );
        }
    }

    /// The sealed source family of one tree under one partition.
    struct Input {
        manifest: SegmentManifest,
        bytes: BTreeMap<SegmentId, Vec<u8>>,
        partition: Partition,
        held: Vec<TreeSegment>,
        reference: Vec<segments::TreeSegment>,
    }

    fn input(
        tree: &reference_model::Tree,
        embedding: &panproto_reference::encode::TreeEmbedding,
        partition: &segments::Partition,
    ) -> Input {
        let production_partition = prod_partition(partition);
        let (whole, production_embedding_state) = production_tree(tree, embedding);
        let held = partition_tree(&whole, &production_embedding_state, &production_partition)
            .expect("production split");
        let any: Vec<AnySegment> = held.iter().cloned().map(AnySegment::Tree).collect();
        let (manifest, bytes) = seal_family(&any, &production_partition);
        Input {
            manifest,
            bytes,
            partition: production_partition,
            held,
            reference: segments::partition_tree(tree, embedding, partition)
                .expect("reference split"),
        }
    }

    fn seen_production(run: &ApplyRun) -> Seen {
        seen_production_by(&run.report, &|error| {
            format!("{:?}", production_refusal(error))
        })
    }

    fn seen_reference(
        run: &segments::ApplyRun<segments::TreeSegment, reference::TreeApplyError>,
    ) -> Seen {
        seen_reference_by(&run.report, &|error| {
            format!("{:?}", reference_refusal(error))
        })
    }

    fn production_run<Src: SegmentSource, St: SegmentStore>(
        edit: &TreeEdit,
        input: &Input,
        source: &Src,
        budget: Budget,
        store: &mut St,
        cancel: Option<u32>,
    ) -> ApplyRun {
        apply_run(
            edit,
            &input.manifest,
            source,
            RunSpec {
                lens: Digest([0; 32]),
                input: input.manifest.digest(),
                output_schema: Digest([0; 32]),
                cancel_after_tasks: cancel,
            },
            budget,
            store,
            &mut |_| {},
        )
    }

    fn reference_run(
        edit: &reference::TreeEdit,
        input: &Input,
        partition: &segments::Partition,
        budget: Budget,
        store: &mut segments::MemoryStore,
        cancel: Option<u32>,
    ) -> segments::ApplyRun<segments::TreeSegment, reference::TreeApplyError> {
        segments::segmented_tree_apply_with_progress(
            edit,
            &input.reference,
            partition,
            &segments::ApplyRunSpec {
                schema: segments::Digest(SCHEMA),
                binding: None,
                cancel_after_tasks: cancel,
            },
            segments::Budget {
                working_set: budget.working_set,
                complement: budget.complement,
                summary: budget.summary,
                affected_segments: budget.affected_segments,
            },
            store,
            &mut |_| {},
        )
    }

    /// Check the run law on one case under `budget`, as the graph and table
    /// run laws are checked: the production report equals the reference
    /// runner's, uninterrupted and, when `every_point`, at every
    /// cancellation point and after resuming; a complete run reads each
    /// segment of the read set once and no other, emits the reference's
    /// edited segments and seals the reference's manifest; a refused or
    /// declined run seals nothing. It returns how the run ended and the
    /// cancellation points checked.
    fn check_run(
        tree: &reference_model::Tree,
        embedding: &panproto_reference::encode::TreeEmbedding,
        partition: &segments::Partition,
        edit: &reference::TreeEdit,
        budget: Budget,
        every_point: bool,
    ) -> (RunOutcome, u64) {
        let input = input(tree, embedding, partition);
        let production_edit = tree_edit_to_production(edit);
        let source = CountingSource {
            bytes: input.bytes.clone(),
            reads: RefCell::new(BTreeMap::new()),
        };
        let mut store = MemoryStore::default();
        let baseline = production_run(&production_edit, &input, &source, budget, &mut store, None);
        let mut reference_store = segments::MemoryStore::default();
        let expected = reference_run(edit, &input, partition, budget, &mut reference_store, None);
        assert_eq!(
            seen_production(&baseline),
            seen_reference(&expected),
            "run of {edit:?} on {tree:?} under {partition:?}"
        );
        let run = baseline.run.expect("a run identity");
        assert_eq!(run.0, expected.run.0, "run identity");
        let outcome = match &baseline.report.status {
            RunStatus::Complete { manifest } => {
                let read: BTreeSet<SegmentId> = expected
                    .read
                    .iter()
                    .map(|segment| SegmentId(segment.0))
                    .collect();
                assert_eq!(
                    source.reads.borrow().clone(),
                    read.iter()
                        .map(|segment| (*segment, 1))
                        .collect::<BTreeMap<_, _>>(),
                    "a complete run reads each segment of its read set once and no other"
                );
                let sealed = baseline.manifest.as_ref().expect("a manifest");
                assert_eq!(verify_manifest(sealed, *manifest), Ok(()));
                assert_eq!(
                    sealed.canonical_bytes(),
                    segments::canonical_manifest_bytes(
                        expected.manifest.as_ref().expect("a reference manifest")
                    ),
                    "the sealed manifest of {edit:?} on {tree:?}"
                );
                let output = expected.output.as_ref().expect("the edited family");
                for segment in &read {
                    let (emitted, complement) =
                        read_emitted_segment(&store, run, &input.partition, *segment)
                            .expect("an emitted segment");
                    assert_eq!(complement, Vec::<u8>::new());
                    assert_eq!(
                        emitted
                            .canonical_bytes(&Digest(SCHEMA), &input.partition)
                            .expect("bytes"),
                        reference_bytes(&output[segment.0 as usize], partition)
                    );
                }
                RunOutcome::Complete
            }
            RunStatus::Failed(_) => {
                assert!(sealed_nothing(&store, run), "a refused run seals nothing");
                RunOutcome::Refused
            }
            RunStatus::Residual(_) => {
                assert!(sealed_nothing(&store, run), "a declined run seals nothing");
                RunOutcome::Declined
            }
            RunStatus::Incomplete { reason } => panic!("an uninterrupted run stopped: {reason:?}"),
        };
        let points = if every_point {
            check_cancellations(&input, partition, edit, budget, &baseline)
        } else {
            0
        };
        (outcome, points)
    }

    /// Stopping the run at every cancellation point and resuming it reports
    /// what the reference runner reports, and ends as `baseline`.
    fn check_cancellations(
        input: &Input,
        partition: &segments::Partition,
        edit: &reference::TreeEdit,
        budget: Budget,
        baseline: &ApplyRun,
    ) -> u64 {
        let production_edit = tree_edit_to_production(edit);
        let run = baseline.run.expect("a run identity");
        let (total, done) = (
            baseline.report.progress.tasks_total,
            baseline.report.progress.tasks_done,
        );
        for cancel in 0..=total {
            let mut store = MemoryStore::default();
            let stopped = production_run(
                &production_edit,
                input,
                &input.bytes,
                budget,
                &mut store,
                Some(cancel),
            );
            let mut reference_store = segments::MemoryStore::default();
            let expected_stop = reference_run(
                edit,
                input,
                partition,
                budget,
                &mut reference_store,
                Some(cancel),
            );
            assert_eq!(
                seen_production(&stopped),
                seen_reference(&expected_stop),
                "run cancelled after {cancel} tasks"
            );
            let resumed = production_run(
                &production_edit,
                input,
                &input.bytes,
                budget,
                &mut store,
                None,
            );
            let again = reference_run(edit, input, partition, budget, &mut reference_store, None);
            assert_eq!(
                seen_production(&resumed),
                seen_reference(&again),
                "run resumed after {cancel} tasks"
            );
            assert_eq!(resumed.report.tasks_reused, cancel.min(done));
            assert_eq!(resumed.manifest, baseline.manifest);
            assert_eq!(
                store
                    .staged_uncommitted(&run)
                    .unwrap_or_else(|never| match never {}),
                Vec::<ArtifactKey>::new()
            );
        }
        u64::from(total) + 1
    }

    /// Every 37th case of the segmented tree edit carrier, run as a
    /// checkpointed run from every cancellation point against the reference
    /// runner.
    #[test]
    fn segmented_tree_apply_run_matches_the_reference_runner_at_every_cancellation_point() {
        let mut outcomes: BTreeMap<RunOutcome, u64> = BTreeMap::new();
        let mut points = 0;
        for case in carrier::segmented_tree_apply_cases().step_by(37) {
            let (outcome, checked) = check_run(
                &case.tree,
                &case.embedding,
                &case.partition,
                &case.edit,
                Budget::unlimited(),
                true,
            );
            *outcomes.entry(outcome).or_insert(0) += 1;
            points += checked;
        }
        println!("{outcomes:?}, {points} cancellation points");
        assert_eq!(
            outcomes,
            BTreeMap::from([(RunOutcome::Complete, 188), (RunOutcome::Refused, 125)])
        );
        assert_eq!(points, 3_391);
    }

    /// The `a -e-> a` arc label of the witnesses.
    fn arc() -> reference_model::SEdge {
        reference_model::SEdge::new("a", "a", "e", None)
    }

    /// The witness family: the tree `0 -> 1, 0 -> 2, 1 -> 3, 2 -> 4` with the
    /// fan of `h` from 3 to 4 (fan node 5, node counter 6), under four
    /// segments: 0 owns the root and the fresh ids 6 to 9, 1 owns 1 and 3, 2
    /// owns 2 and 4, and 3 owns the fan node 5.
    fn witness() -> (
        reference_model::Tree,
        panproto_reference::encode::TreeEmbedding,
        segments::Partition,
    ) {
        let tree = reference_model::Tree {
            nodes: (0..5)
                .map(|id| (id, reference_model::Node::new(id, "a")))
                .collect(),
            arcs: vec![(0, 1, arc()), (0, 2, arc()), (1, 3, arc()), (2, 4, arc())],
            fans: vec![reference_model::Fan {
                parent: 3,
                hyper_edge: "h".to_owned(),
                children: BTreeMap::from([("x".to_owned(), 4)]),
            }],
            root: 0,
        };
        let (_, embedding) =
            panproto_reference::encode::tree_to_graph(&tree, None).expect("the witness embeds");
        assert_eq!((embedding.next_node, embedding.fans[0].0), (6, 5));
        let ranges = |pairs: &[(u32, u32)]| -> Vec<segments::IdRange> {
            pairs
                .iter()
                .map(|(lo, hi)| segments::IdRange { lo: *lo, hi: *hi })
                .collect()
        };
        let owned: [&[(u32, u32)]; 4] = [
            &[(0, 0), (6, 9)],
            &[(1, 1), (3, 3)],
            &[(2, 2), (4, 4)],
            &[(5, 5)],
        ];
        let partition = segments::Partition {
            shape: segments::InstanceShapeKind::Tree,
            specs: (0_u32..)
                .zip(owned)
                .map(|(id, owned)| segments::SegmentSpec {
                    id: segments::SegmentId(id),
                    nodes: ranges(owned),
                    alloc_nodes: None,
                    alloc_edges: None,
                })
                .collect(),
        };
        (tree, embedding, partition)
    }

    fn insert(parent: u32, child_id: u32) -> reference::TreeEdit {
        reference::TreeEdit::InsertNode {
            parent,
            child_id,
            node: reference_model::Node::new(child_id, "a"),
            edge: arc(),
        }
    }

    #[test]
    fn a_segmented_tree_apply_reads_only_the_owners_and_peers_of_what_it_names() {
        let (tree, embedding, partition) = witness();
        let input = input(&tree, &embedding, &partition);
        let any: Vec<AnySegment> = input.held.iter().cloned().map(AnySegment::Tree).collect();
        let family = FamilyIndex::of_segments(&any, &input.partition).expect("an index");
        // A field of node 3 reads its owner alone; relabelling node 4 reads
        // its owner and the holders of the arc and the fan edge into it;
        // deleting the fans of `h` reads the fan's holder and the owners of
        // its endpoints; inserting node 6 under node 3 reads the parent's
        // owner, the child's owner (the root's segment, which holds the
        // counters) and its target peers; deleting leaf 4 reads its owner,
        // the holders of the edges into it and their target peers; a move
        // reads every segment.
        let cases: [(reference::TreeEdit, &[u32]); 6] = [
            (
                reference::TreeEdit::SetField {
                    node_id: 3,
                    field: "f".to_owned(),
                    value: panproto_reference::value::Value::Int(0),
                },
                &[1],
            ),
            (
                reference::TreeEdit::RelabelNode {
                    id: 4,
                    new_anchor: "b".to_owned(),
                },
                &[0, 2, 3],
            ),
            (
                reference::TreeEdit::DeleteFan {
                    hyper_edge: "h".to_owned(),
                },
                &[1, 2, 3],
            ),
            (insert(3, 6), &[0, 1, 2]),
            (reference::TreeEdit::DeleteNode { id: 4 }, &[0, 1, 2, 3]),
            (
                reference::TreeEdit::MoveSubtree {
                    node_id: 4,
                    new_parent: 1,
                    edge: arc(),
                },
                &[0, 1, 2, 3],
            ),
        ];
        for (edit, expected) in cases {
            let production_edit = tree_edit_to_production(&edit);
            assert_eq!(
                family
                    .read_set(&production_edit)
                    .into_iter()
                    .map(|segment| segment.0)
                    .collect::<Vec<_>>(),
                expected,
                "{edit:?}"
            );
            assert_eq!(
                check_run(
                    &tree,
                    &embedding,
                    &partition,
                    &edit,
                    Budget::unlimited(),
                    true
                )
                .0,
                RunOutcome::Complete,
                "{edit:?}"
            );
            assert_eq!(
                check_case(&tree, &embedding, &partition, &edit),
                Outcome::Applied
            );
            let mut store = MemoryStore::default();
            let run = production_run(
                &production_edit,
                &input,
                &input.bytes,
                Budget::unlimited(),
                &mut store,
                None,
            );
            let read = u32::try_from(expected.len()).expect("a small count");
            assert_eq!(run.report.progress.tasks_total, 4 * read + 2);
            let sealed = run.manifest.expect("a manifest");
            for record in &input.manifest.segments {
                if !expected.contains(&record.id.0) {
                    assert_eq!(
                        sealed.segments[record.id.0 as usize], *record,
                        "an unread record is kept"
                    );
                }
            }
        }
    }

    #[test]
    fn a_refused_tree_edit_fails_at_assemble_with_the_whole_state_refusal_and_seals_nothing() {
        let (tree, embedding, partition) = witness();
        let input = input(&tree, &embedding, &partition);
        // Node 1 has the child 3, so deleting it is refused; the refusal is
        // decided from the facts of the segments the edit reads.
        let delete = reference::TreeEdit::DeleteNode { id: 1 };
        let mut store = MemoryStore::default();
        let run = production_run(
            &tree_edit_to_production(&delete),
            &input,
            &input.bytes,
            Budget::unlimited(),
            &mut store,
            None,
        );
        match &run.report.status {
            RunStatus::Failed(error) => {
                assert_eq!(
                    production_refusal(error),
                    Refusal::Edit(TreeObs::NotLeaf(1))
                );
            }
            other => panic!("the edit is refused: {other:?}"),
        }
        assert_eq!(run.report.progress.phase, Phase::Summarize);
        assert!(sealed_nothing(&store, run.run.expect("a run identity")));
        assert_eq!(
            check_run(
                &tree,
                &embedding,
                &partition,
                &delete,
                Budget::unlimited(),
                true
            )
            .0,
            RunOutcome::Refused
        );
    }

    #[test]
    fn a_segmented_tree_apply_run_declines_each_exceeded_budget_like_the_reference() {
        let (tree, embedding, partition) = witness();
        let edit = insert(3, 6);
        for (budget, resource) in [
            (
                Budget {
                    affected_segments: 1,
                    ..Budget::unlimited()
                },
                Resource::AffectedSegments,
            ),
            (
                Budget {
                    working_set: 0,
                    ..Budget::unlimited()
                },
                Resource::WorkingSet,
            ),
            (
                Budget {
                    summary: 2,
                    ..Budget::unlimited()
                },
                Resource::Summary,
            ),
        ] {
            assert_eq!(
                check_run(&tree, &embedding, &partition, &edit, budget, true).0,
                RunOutcome::Declined
            );
            let input = input(&tree, &embedding, &partition);
            let mut store = MemoryStore::default();
            let run = production_run(
                &tree_edit_to_production(&edit),
                &input,
                &input.bytes,
                budget,
                &mut store,
                None,
            );
            match &run.report.status {
                RunStatus::Residual(Residual::BudgetExceeded {
                    resource: found, ..
                }) => assert_eq!(*found, resource),
                other => panic!("{resource:?} declines: {other:?}"),
            }
        }
    }

    #[test]
    fn a_segmented_tree_apply_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run()
     {
        let (tree, embedding, partition) = witness();
        let input = input(&tree, &embedding, &partition);
        let edit = tree_edit_to_production(&insert(3, 6));
        let mut baseline_store = MemoryStore::default();
        let baseline = production_run(
            &edit,
            &input,
            &input.bytes,
            Budget::unlimited(),
            &mut baseline_store,
            None,
        );
        assert!(matches!(baseline.report.status, RunStatus::Complete { .. }));
        let total = baseline.report.progress.tasks_total;
        for fail_at in 1..=total {
            let mut store = FailingStore {
                inner: MemoryStore::default(),
                commits: 0,
                fail_at: Some(usize::try_from(fail_at).expect("a small count")),
            };
            let failed = production_run(
                &edit,
                &input,
                &input.bytes,
                Budget::unlimited(),
                &mut store,
                None,
            );
            assert!(
                matches!(
                    &failed.report.status,
                    RunStatus::Incomplete {
                        reason: RunIncompleteReason::Store { message }
                    } if message == "commit failed"
                ),
                "{:?}",
                failed.report.status
            );
            assert_eq!(failed.report.progress.tasks_done, fail_at - 1);
            assert_eq!(failed.report.orphans.len(), 1, "the failed task's artifact");
            store.fail_at = None;
            let resumed = production_run(
                &edit,
                &input,
                &input.bytes,
                Budget::unlimited(),
                &mut store,
                None,
            );
            assert_eq!(resumed.report.tasks_reused, fail_at - 1);
            assert_eq!(resumed.report.orphans, Vec::<ArtifactKey>::new());
            assert_eq!(resumed.manifest, baseline.manifest);
        }
    }

    /// The witness family in production, with the tree and embedding it
    /// glues to after `edit` in whole-state application.
    fn production_witness(
        edit: &TreeEdit,
    ) -> (
        Vec<TreeSegment>,
        Partition,
        Result<(WInstance, TreeEmbedding), EditError>,
    ) {
        let (tree, embedding, partition) = witness();
        let input = input(&tree, &embedding, &partition);
        let (mut whole, mut followed) =
            assemble_tree(input.held.clone(), &input.partition).expect("glues");
        let applied = followed
            .follow_edit(&mut whole, edit)
            .map(|()| (whole, followed));
        (input.held, input.partition, applied)
    }

    /// A node inserted under the id of a fan node relocates the fan node to
    /// the node counter, keeping its edges: the fan moves to the segment
    /// owning the counter's value, which the read set holds, and the output
    /// glues to whole-state application.
    #[test]
    fn a_node_taking_a_fan_nodes_id_relocates_the_fan_into_the_counters_owner() {
        let edit = TreeEdit::InsertNode {
            parent: 0,
            child_id: 5,
            node: panproto_inst::Node::new(5, "a"),
            edge: super::edge_to_production(&arc()),
        };
        let (held, partition, applied) = production_witness(&edit);
        let any: Vec<AnySegment> = held.iter().cloned().map(AnySegment::Tree).collect();
        let family = FamilyIndex::of_segments(&any, &partition).expect("an index");
        assert_eq!(
            family
                .read_set(&edit)
                .into_iter()
                .map(|s| s.0)
                .collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
        let output = apply_segmented(&edit, &held, &partition).expect("applies");
        let (whole, followed) = applied.expect("whole-state application accepts");
        assert_eq!(
            followed.fans[0].node, 6,
            "the fan node moves to the counter"
        );
        assert_eq!(output[0].fans.len(), 1, "segment 0 owns node 6");
        assert!(output[3].fans.is_empty() && output[3].nodes.contains_key(&5));
        assert_eq!(
            glued(output, &partition),
            (tree_to_reference(&whole), production_embedding(&followed))
        );
    }

    /// A fan inserted with an endpoint that is no tree node applies to the
    /// whole tree, whose split then refuses it; the segmented apply returns
    /// that refusal.
    #[test]
    fn a_fan_with_an_absent_endpoint_is_refused_as_the_split_refuses_it() {
        let mut fan = panproto_inst::Fan::new("g", 3);
        fan = fan.with_child("y", 8);
        let edit = TreeEdit::InsertFan { fan };
        let (held, partition, applied) = production_witness(&edit);
        let (whole, followed) = applied.expect("whole-state application accepts");
        let expected =
            partition_tree(&whole, &followed, &partition).expect_err("the split refuses");
        assert_eq!(
            expected,
            SegmentError::NotEmbedded {
                elem: ViewRef::Node(6)
            }
        );
        assert!(matches!(
            apply_segmented(&edit, &held, &partition),
            Err(SegmentedApplyError::Segment(error)) if error == expected
        ));
    }

    /// The three partitions of the composition carrier: every id in one
    /// segment, `0 | 1 2 | 3 4`, and `0 3 | 1 4 | 2`.
    fn compose_partitions() -> Vec<segments::Partition> {
        [[0_u32, 0, 0, 0, 0], [0, 1, 1, 2, 2], [0, 1, 2, 0, 1]]
            .iter()
            .map(|owners| {
                let segments = owners.iter().max().map_or(0, |max| max + 1);
                segments::Partition {
                    shape: segments::InstanceShapeKind::Tree,
                    specs: (0..segments)
                        .map(|id| segments::SegmentSpec {
                            id: segments::SegmentId(id),
                            nodes: (0_u32..)
                                .zip(owners)
                                .filter(|(_, owner)| **owner == id)
                                .map(|(node, _)| segments::IdRange { lo: node, hi: node })
                                .collect(),
                            alloc_nodes: None,
                            alloc_edges: None,
                        })
                        .collect(),
                }
            })
            .collect()
    }

    /// What one ordered pair of the composition carrier showed.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum PairOutcome {
        /// Both edits applied in turn and their composite applied alike.
        Composed,
        /// The first edit is refused, and so is the composite.
        FirstRefused,
        /// The first applies and the second is refused on its result; so is
        /// the composite.
        SecondRefused,
        /// Both applied in turn and their composite applied alike, and the
        /// composite lies outside the reference's domain ([`diverges`]), so
        /// the reference is not compared.
        Diverging,
    }

    /// Whether a step of `edit`, applied in turn from `tree`, inserts a node
    /// under the id of a fan node or inserts a fan with an endpoint that is
    /// no tree node. On the first, production relocates the fan node alone,
    /// keeping its edge ids, where the reference also gives fresh ids to its
    /// edges and to every later fan; on the second, production's tree edit
    /// applies and the split of the edited tree refuses it, where the
    /// reference's tree edit refuses it. The reference differentials of
    /// segmented tree edits leave these edits out, and two witnesses compare
    /// them with production whole-state application.
    fn diverges(
        tree: &reference_model::Tree,
        embedding: &panproto_reference::encode::TreeEmbedding,
        edit: &reference::TreeEdit,
    ) -> bool {
        let (mut state, mut followed) = (tree.clone(), embedding.clone());
        for step in edit.steps() {
            match step {
                reference::TreeEdit::InsertNode { child_id, .. }
                    if followed.fans.iter().any(|(node, _, _)| node == child_id) =>
                {
                    return true;
                }
                reference::TreeEdit::InsertFan { fan }
                    if std::iter::once(&fan.parent)
                        .chain(fan.children.values())
                        .any(|end| !state.nodes.contains_key(end)) =>
                {
                    return true;
                }
                _ => {}
            }
            match reference::apply_tree(step, &state, &mut followed) {
                Ok(next) => state = next,
                Err(_) => return false,
            }
        }
        false
    }

    /// Composition over segments: for every ordered pair of carrier edits on
    /// every carrier tree under three partitions, the composite applies to
    /// the family exactly when the edits apply in turn, giving the same
    /// segments byte for byte, which the reference composite gives too.
    #[test]
    fn a_composite_tree_edit_applies_to_a_segmented_family_as_its_factors_do_in_turn() {
        let mut outcomes: BTreeMap<PairOutcome, u64> = BTreeMap::new();
        for partition in compose_partitions() {
            for (tree, embedding) in carrier::segmented_tree_apply_trees() {
                let input = input(&tree, &embedding, &partition);
                let n = u32::try_from(tree.nodes.len()).expect("a small tree");
                let edits = carrier::segmented_tree_edits(n, embedding.next_node);
                for first in &edits {
                    let a = tree_edit_to_production(first);
                    let after_first = apply_segmented(&a, &input.held, &input.partition);
                    for second in &edits {
                        let b = tree_edit_to_production(second);
                        let composite = a.clone().compose(b.clone());
                        let together = apply_segmented(&composite, &input.held, &input.partition);
                        let reference_composite =
                            reference::TreeEdit::Sequence(vec![first.clone(), second.clone()]);
                        let reference_together = segments::apply_tree_segmented(
                            &reference_composite,
                            &input.reference,
                            &partition,
                        );
                        let outcome = match &after_first {
                            Err(_) => {
                                assert!(together.is_err() && reference_together.is_err());
                                PairOutcome::FirstRefused
                            }
                            Ok(middle) => match apply_segmented(&b, middle, &input.partition) {
                                Err(_) => {
                                    assert!(together.is_err() && reference_together.is_err());
                                    PairOutcome::SecondRefused
                                }
                                Ok(last) => {
                                    let expected: Vec<Vec<u8>> =
                                        last.iter().map(|s| bytes(s, &input.partition)).collect();
                                    let together = together.expect("the composite applies");
                                    assert_eq!(
                                        together
                                            .iter()
                                            .map(|s| bytes(s, &input.partition))
                                            .collect::<Vec<_>>(),
                                        expected,
                                        "{first:?} then {second:?} on {tree:?}"
                                    );
                                    if diverges(&tree, &embedding, &reference_composite) {
                                        *outcomes.entry(PairOutcome::Diverging).or_insert(0) += 1;
                                        continue;
                                    }
                                    assert_eq!(
                                        reference_together
                                            .expect("the reference composite applies")
                                            .iter()
                                            .map(|s| reference_bytes(s, &partition))
                                            .collect::<Vec<_>>(),
                                        expected
                                    );
                                    PairOutcome::Composed
                                }
                            },
                        };
                        *outcomes.entry(outcome).or_insert(0) += 1;
                    }
                }
            }
        }
        println!("{outcomes:?}");
        assert_eq!(outcomes.values().sum::<u64>(), 3 * 6_206);
        assert_eq!(
            outcomes,
            BTreeMap::from([
                (PairOutcome::Composed, 6_081),
                (PairOutcome::FirstRefused, 6_957),
                (PairOutcome::SecondRefused, 5_496),
                (PairOutcome::Diverging, 84)
            ])
        );
    }

    /// A generated tree over nodes `0 .. parents.len()`: node `i + 1` hangs
    /// under node `parents[i] % (i + 1)`, every node anchored at `a`, with
    /// the fan of `h` from its last node to the root when `fan`.
    fn generated(
        parents: &[u32],
        fan: bool,
    ) -> (
        reference_model::Tree,
        panproto_reference::encode::TreeEmbedding,
    ) {
        let n = u32::try_from(parents.len()).expect("a small tree") + 1;
        let tree = reference_model::Tree {
            nodes: (0..n)
                .map(|id| (id, reference_model::Node::new(id, "a")))
                .collect(),
            arcs: (1..n)
                .zip(parents)
                .map(|(child, parent)| (parent % child, child, arc()))
                .collect(),
            fans: if fan {
                vec![reference_model::Fan {
                    parent: n - 1,
                    hyper_edge: "h".to_owned(),
                    children: BTreeMap::from([("x".to_owned(), 0)]),
                }]
            } else {
                Vec::new()
            },
            root: 0,
        };
        let (_, embedding) = panproto_reference::encode::tree_to_graph(&tree, None)
            .expect("a generated tree embeds");
        (tree, embedding)
    }

    /// The tree partition assigning id `i` to segment `owners[i]`
    /// (renumbered by first occurrence, `3` for no segment).
    fn generated_partition(owners: &[u8]) -> segments::Partition {
        let mut renamed: BTreeMap<u8, u32> = BTreeMap::new();
        let mut owned = Vec::new();
        for owner in owners {
            if *owner == 3 {
                owned.push(None);
            } else {
                let next = u32::try_from(renamed.len()).expect("a small count");
                owned.push(Some(*renamed.entry(*owner).or_insert(next)));
            }
        }
        let segments = u32::try_from(renamed.len()).expect("a small count");
        segments::Partition {
            shape: segments::InstanceShapeKind::Tree,
            specs: (0..segments)
                .map(|id| segments::SegmentSpec {
                    id: segments::SegmentId(id),
                    nodes: (0_u32..)
                        .zip(&owned)
                        .filter(|(_, owner)| **owner == Some(id))
                        .map(|(node, _)| segments::IdRange { lo: node, hi: node })
                        .collect(),
                    alloc_nodes: None,
                    alloc_edges: None,
                })
                .collect(),
        }
    }

    /// The generated edit: the sequence of the carrier letters at `tree`
    /// that `choices` pick.
    fn generated_edit(
        tree: &reference_model::Tree,
        embedding: &panproto_reference::encode::TreeEmbedding,
        choices: &[usize],
    ) -> reference::TreeEdit {
        let n = u32::try_from(tree.nodes.len()).expect("a small tree");
        let letters = carrier::segmented_tree_edits(n, embedding.next_node);
        reference::TreeEdit::Sequence(
            choices
                .iter()
                .map(|choice| letters[choice % letters.len()].clone())
                .collect(),
        )
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

        /// The law on generated trees of up to six nodes and sequences of
        /// up to three carrier letters, under partitions of the ids 0 to 6
        /// in which every tree and fan node is owned and the fresh ids may
        /// be owned by no segment.
        #[test]
        fn segmented_tree_apply_agrees_with_the_reference_on_generated_families(
            parents in proptest::collection::vec(0_u32..6, 0..=5),
            fan in proptest::bool::ANY,
            owners in proptest::collection::vec(0_u8..3, 7),
            spare in proptest::collection::vec(0_u8..4, 2),
            choices in proptest::collection::vec(0_usize..200, 1..=3),
        ) {
            let (tree, embedding) = generated(&parents, fan);
            let owners: Vec<u8> = owners.into_iter().chain(spare).collect();
            let partition = generated_partition(&owners);
            let edit = generated_edit(&tree, &embedding, &choices);
            proptest::prop_assume!(!diverges(&tree, &embedding, &edit));
            check_case(&tree, &embedding, &partition, &edit);
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(32))]

        /// The run law on generated trees and sequences, at every
        /// cancellation point.
        #[test]
        fn segmented_tree_apply_run_matches_the_reference_runner_on_generated_families(
            parents in proptest::collection::vec(0_u32..5, 0..=4),
            fan in proptest::bool::ANY,
            owners in proptest::collection::vec(0_u8..3, 6),
            spare in 0_u8..4,
            choices in proptest::collection::vec(0_usize..200, 1..=2),
        ) {
            let (tree, embedding) = generated(&parents, fan);
            let owners: Vec<u8> = owners.into_iter().chain([spare]).collect();
            let partition = generated_partition(&owners);
            let edit = generated_edit(&tree, &embedding, &choices);
            proptest::prop_assume!(!diverges(&tree, &embedding, &edit));
            check_run(&tree, &embedding, &partition, &edit, Budget::unlimited(), true);
        }
    }
}
