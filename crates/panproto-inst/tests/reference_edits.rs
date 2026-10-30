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

fn tree_release_cases() -> Vec<(&'static str, reference_model::Tree, reference::TreeEdit)> {
    let TreeReleaseFixtures {
        base,
        with_field,
        fan,
        with_fan,
        join,
    } = tree_release_fixtures();
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
            base.clone(),
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
fn e1_apply_matches_reference_on_every_graph_and_atomic_delta() {
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
fn e2_e3_algebra_matches_reference_on_the_exact_atomic_word_carrier() {
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
fn e1_independence_and_merge_match_reference_on_every_atomic_pair() {
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
fn e2_t5_table_apply_and_algebra_match_the_exact_release_carrier() {
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
fn e3_tr14_tree_lowering_and_application_match_reference_for_every_primitive() {
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
        Err(EditError::ApplyFailed(_))
    ));

    let mut writer = CanonicalWriter::new();
    canonical_tree_edit_bytes(&production_edit, &mut writer);
    assert!(!writer.into_bytes().is_empty());
}

#[test]
fn e3_malformed_tree_embeddings_refuse_without_mutating_the_embedding() {
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
fn e1_independent_deltas_commute_and_merge_on_every_carrier_graph() {
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
fn e2_diff_table_applies_and_detects_equality_on_every_well_formed_carrier_pair() {
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
fn e2_diff_table_law_excludes_tables_whose_pairs_leave_their_edge_tables() {
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
fn e3_unit_associativity_and_anti_homomorphism_hold_on_every_atomic_triple() {
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
    Rejected,
    Cycle { node: u32, parent: u32 },
}

fn reference_join_refusal(error: &reference::TreeApplyError) -> JoinRefusalObs {
    match error {
        reference::TreeApplyError::NodeNotFound(id) => JoinRefusalObs::NodeNotFound(*id),
        reference::TreeApplyError::MisfiledNode { .. }
        | reference::TreeApplyError::InvalidJoin(_)
        | reference::TreeApplyError::RootOperation(_) => JoinRefusalObs::Rejected,
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
        EditError::ApplyFailed(_) => JoinRefusalObs::Rejected,
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
fn tr14_join_refusals_match_reference_and_leave_tree_and_embedding_unchanged() {
    let cases = carrier::join_cases();
    assert_eq!(
        u64::try_from(cases.len()).unwrap(),
        carrier::join_cases_count_certificate()
    );
    let expected_refusal = |name: &str| match name {
        "joined-ancestor" => JoinRefusalObs::Cycle { node: 1, parent: 3 },
        "missing-primary" | "missing-joined" => JoinRefusalObs::NodeNotFound(9),
        "joined-root" | "repeated-joined" | "joined-primary" | "misfiled-product" => {
            JoinRefusalObs::Rejected
        }
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

#[test]
fn t5_surface_table_edits_lower_check_and_invert_like_the_reference() {
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
        for first in &steps {
            for second in &steps {
                let batch = reference::TableEdit::Batch(vec![first.clone(), second.clone()]);
                let expected = reference::table_edit_delta(&batch);
                let production_batch = table_edit_to_production(&batch);
                match (expected, production_batch.to_delta()) {
                    (Ok(expected), Ok(actual)) => {
                        assert_eq!(table_delta_to_reference(&actual), expected, "{batch:?}");
                        let Ok(after) = reference::apply_table(&expected, &table) else {
                            continue;
                        };
                        assert_eq!(
                            reference::apply_table(&expected.invert(), &after).as_ref(),
                            Ok(&table)
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
                        assert_eq!(table_to_reference(&production_after), table);
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
struct C8Tally {
    states: u64,
    words: [u64; 4],
    refusals: BTreeMap<String, u64>,
    replacements: u64,
    commuting: u64,
}

impl C8Tally {
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

/// Run `work` over every `(schema, state)` job on at most four workers and
/// sum their tallies.
fn c8_parallel<S: Sync>(
    jobs: &[(usize, S)],
    work: impl Fn(&carrier::C8Schema, &S, &mut C8Tally) + Sync,
) -> C8Tally {
    let schemas = carrier::c8_schemas();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(1, |n| usize::from(n).min(4));
    std::thread::scope(|threads| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                threads.spawn(|| {
                    let mut tally = C8Tally::default();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((schema, state)) = jobs.get(index) else {
                            break;
                        };
                        tally.states += 1;
                        work(&schemas[*schema], state, &mut tally);
                    }
                    tally
                })
            })
            .collect();
        let mut total = C8Tally::default();
        for handle in handles {
            total.absorb(handle.join().expect("worker completes"));
        }
        total
    })
}

fn c8_jobs<S>(states: impl Fn(&carrier::C8Schema) -> Vec<S>) -> Vec<(usize, S)> {
    carrier::c8_schemas()
        .iter()
        .enumerate()
        .flat_map(|(index, schema)| states(schema).into_iter().map(move |state| (index, state)))
        .collect()
}

fn assert_c8_counts(tally: &C8Tally, certificate: carrier::C8Counts) {
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

#[test]
fn c8_graph_edit_algebra_matches_reference_on_every_word_of_the_coverage_scope() {
    let jobs = c8_jobs(carrier::c8_graphs);
    let tally = c8_parallel(&jobs, |schema, graph, tally| {
        let mut prefixes: Vec<GraphPrefix> = Vec::with_capacity(4);
        carrier::c8_graph_words(schema, graph, &mut |word| {
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
            assert_eq!(actual, expected, "apply {letter:?} to {:?}", before.reference);
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

            let composite = reference::Delta::compose(&before.composite, letter)
                .expect("sequentially applicable letters compose");
            let production_composite = before
                .production_composite
                .compose(&production_letter)
                .expect("production composes sequentially applicable letters");
            assert_eq!(
                delta_to_reference(&production_composite),
                composite,
                "compose {:?}",
                word.letters
            );
            assert_eq!(
                delta_to_reference(&production_composite.invert()),
                composite.invert(),
                "invert {:?}",
                word.letters
            );
            let expected_bytes = reference::canonical_graph_delta_bytes(&composite)
                .expect("a scope composite has canonical bytes");
            let mut writer = CanonicalWriter::new();
            canonical_graph_delta_bytes(&production_composite, &mut writer);
            assert_eq!(writer.into_bytes(), expected_bytes, "bytes {:?}", word.letters);

            let start = &prefixes[0];
            let mut net = start.production.clone();
            production_composite
                .apply(&mut net)
                .unwrap_or_else(|error| panic!("composite of {:?}: {error:?}", word.letters));
            assert!(
                GInstance::identical(&net, &production_state),
                "composite {:?}",
                word.letters
            );
            production_composite
                .invert()
                .apply(&mut net)
                .unwrap_or_else(|error| panic!("inverse of {:?}: {error:?}", word.letters));
            assert!(
                GInstance::identical(&net, &start.production),
                "inverse {:?}",
                word.letters
            );
            if composite.del.keys().any(|x| composite.add.contains_key(x)) {
                assert!(!production_composite.is_identity());
                tally.replacements += 1;
            }
            prefixes.push(GraphPrefix {
                reference: after.clone(),
                production: production_state,
                composite,
                production_composite,
            });
        });
    });
    println!("graph scope: {tally:?}");
    assert_c8_counts(&tally, carrier::c8_graph_counts_certificate());
    assert_eq!(tally.refusals.keys().collect::<Vec<_>>(), ["DanglingEdges"]);
    assert!(tally.replacements > 0);
}

#[test]
fn c8_graph_independence_and_merge_match_reference_on_every_letter_pair_of_the_coverage_scope() {
    let jobs = c8_jobs(carrier::c8_graphs);
    let tally = c8_parallel(&jobs, |schema, graph, tally| {
        let letters =
            carrier::c8_graph_letters(schema, graph, carrier::C8Floors::of_graph(graph));
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
                let expected = reference::apply(&reference::merge(first, second).expect("merged"), graph)
                    .expect("the reference merge applies");
                assert_eq!(graph_to_reference(&one_then_two), expected);
                assert_eq!(graph_to_reference(&two_then_one), expected);
                assert_eq!(graph_to_reference(&both), expected);
                tally.commuting += 1;
            }
        }
    });
    println!("graph pairs: {tally:?}");
    assert_eq!(tally.states, carrier::c8_graph_counts_certificate().states);
    assert!(tally.commuting > 0 && !tally.refusals.is_empty());
}

/// The prefix states of the table word under test, by length.
struct TablePrefix {
    reference: reference_model::Table,
    production: FInstance,
    composite: reference::TableDelta,
    production_composite: TableDelta,
}

#[test]
fn c8_table_edits_match_reference_on_every_word_of_the_coverage_scope() {
    let jobs = c8_jobs(carrier::c8_tables);
    let tally = c8_parallel(&jobs, |schema, table, tally| {
        let mut prefixes: Vec<TablePrefix> = Vec::with_capacity(4);
        carrier::c8_table_words(schema, table, &mut |word| {
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
            assert_eq!(actual, expected, "apply {letter:?} to {:?}", before.reference);
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

            let composite = reference::compose_table(&before.composite, &lowered)
                .expect("applicable letters compose");
            let production_composite = before
                .production_composite
                .compose(&production_lowered)
                .expect("production composes applicable letters");
            assert_eq!(
                table_delta_to_reference(&production_composite),
                composite,
                "compose {:?}",
                word.letters
            );
            let expected_bytes = reference::canonical_table_delta_bytes(&composite)
                .expect("a scope composite has canonical bytes");
            let mut writer = CanonicalWriter::new();
            canonical_table_delta_bytes(&production_composite, &mut writer);
            assert_eq!(writer.into_bytes(), expected_bytes, "bytes {:?}", word.letters);
            assert_eq!(
                reference::apply_table(&composite.invert(), after).as_ref(),
                Ok(&prefixes[0].reference),
                "reference inverse {:?}",
                word.letters
            );

            let mut net = prefixes[0].production.clone();
            production_composite
                .apply(&mut net)
                .unwrap_or_else(|error| panic!("composite of {:?}: {error:?}", word.letters));
            assert_eq!(table_to_reference(&net), *after, "composite {:?}", word.letters);
            production_composite
                .invert()
                .apply(&mut net)
                .unwrap_or_else(|error| panic!("inverse of {:?}: {error:?}", word.letters));
            assert_eq!(
                table_to_reference(&net),
                prefixes[0].reference,
                "inverse {:?}",
                word.letters
            );
            if composite.del.keys().any(|x| composite.add.contains_key(x)) {
                tally.replacements += 1;
            }
            prefixes.push(TablePrefix {
                reference: after.clone(),
                production: production_state,
                composite,
                production_composite,
            });
        });
    });
    println!("table scope: {tally:?}");
    assert_c8_counts(&tally, carrier::c8_table_counts_certificate());
    assert_eq!(
        tally.refusals.keys().collect::<Vec<_>>(),
        ["DuplicateForeignKey"]
    );
    assert!(tally.replacements > 0);
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

#[test]
fn c8_tree_edits_and_lowering_match_reference_on_every_word_of_the_coverage_scope() {
    let jobs = c8_jobs(carrier::c8_trees);
    let tally = c8_parallel(&jobs, |schema, tree, tally| {
        let (start_graph, start_embedding) =
            panproto_reference::encode::tree_to_graph(tree, None)
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
        carrier::c8_tree_words(schema, tree, &mut |word| {
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

            let sequence = reference::TreeEdit::Sequence(word.letters.to_vec());
            let production_sequence = tree_edit_to_production(&sequence);
            let mut lowered_embedding = start.reference_embedding.clone();
            let lowered = reference::graph_delta_from_tree_edit(
                tree,
                &mut lowered_embedding,
                &sequence,
                None,
            );
            let mut production_lowered_embedding = start.production_embedding.clone();
            let production_lowered = GraphDelta::from_tree_edit(
                &start.production_graph,
                &mut production_lowered_embedding,
                &production_sequence,
            );
            let after = match word.outcome {
                Err(error) => {
                    let expected = reference_tree_refusal(error);
                    let actual = applied.expect_err("production refuses with the reference");
                    assert_eq!(production_tree_refusal(&actual), expected, "{letter:?}");
                    assert_eq!(
                        tree_to_reference(&production_state),
                        tree_to_reference(&prefixes[length - 1])
                    );
                    assert_eq!(
                        reference_tree_refusal(&lowered.expect_err("lowering refuses")),
                        expected
                    );
                    assert_eq!(lowered_embedding, start.reference_embedding);
                    let actual = production_lowered.expect_err("production lowering refuses");
                    assert_eq!(
                        production_tree_refusal(&actual),
                        expected,
                        "{:?}",
                        word.letters
                    );
                    assert_eq!(production_lowered_embedding, start.production_embedding);
                    tally.refuse(&expected);
                    return;
                }
                Ok(after) => after,
            };
            applied.unwrap_or_else(|error| panic!("production apply {letter:?}: {error:?}"));
            assert_eq!(tree_to_reference(&production_state), *after, "apply {letter:?}");
            let lowered = lowered
                .unwrap_or_else(|error| panic!("reference lowering {:?}: {error:?}", word.letters));
            let production_lowered = production_lowered.unwrap_or_else(|error| {
                panic!("production lowering {:?}: {error:?}", word.letters)
            });
            assert_eq!(
                delta_to_reference(&production_lowered),
                lowered,
                "lower {:?}",
                word.letters
            );
            assert_eq!(
                production_embedding(&production_lowered_embedding),
                reference_embedding(&lowered_embedding),
                "embedding after {:?}",
                word.letters
            );
            let reference_graph = reference::apply(&lowered, &start.reference_graph)
                .unwrap_or_else(|error| panic!("reference graph apply {:?}: {error:?}", word.letters));
            let mut production_graph = start.production_graph.clone();
            production_lowered.apply(&mut production_graph).unwrap_or_else(|error| {
                panic!("production graph apply {:?}: {error:?}", word.letters)
            });
            assert_eq!(graph_to_reference(&production_graph), reference_graph);
            let expected_bytes = reference::canonical_tree_edit_bytes(&sequence)
                .expect("a scope word is well filed");
            let mut writer = CanonicalWriter::new();
            canonical_tree_edit_bytes(&production_sequence, &mut writer);
            assert_eq!(writer.into_bytes(), expected_bytes, "bytes {:?}", word.letters);
            if lowered.del.keys().any(|x| lowered.add.contains_key(x)) {
                tally.replacements += 1;
            }
            prefixes.push(production_state);
        })
        .expect("a scope tree has a graph form");
    });
    println!("tree scope: {tally:?}");
    assert_c8_counts(&tally, carrier::c8_tree_counts_certificate());
    assert_eq!(
        tally.refusals.keys().collect::<Vec<_>>(),
        ["Cycle", "NotLeaf", "RootOperation"]
    );
    assert!(tally.replacements > 0);
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
    tree.nodes.get_mut(&2).expect("leaf").fields.insert(
        "f".to_owned(),
        ReferenceValue::Int(1),
    );
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
