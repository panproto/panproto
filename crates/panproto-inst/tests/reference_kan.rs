#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The Kan extensions against the reference semantics.
//!
//! Every test here runs the production table, graph and tree Kan extensions
//! and `panproto_reference`'s on the same inputs and requires exact
//! agreement: equal outputs, equal provenance, and refusals that map to the
//! same typed reason with the same payload. The adapters below convert
//! between the production types and the reference model; they hold no
//! semantics of their own. Each conversion into the reference model is
//! injective on the values the scopes use (every production value of these
//! scopes has one reference counterpart, and the reverse conversion is a
//! left inverse, checked on every converted input).
//!
//! Exhaustive scopes come from `panproto_reference::enumerate::kan_extensions`
//! and print their bounds and labelled counts, each checked against the
//! enumerator's independently written count.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use panproto_gat::Name;
use panproto_inst::{
    AdjunctionError, BoundaryProtocols, CellLayout, CompiledMigration, DeltaProvenance, EdgeId,
    EdgeLayout, EncodingSide, EntityFunctor, FInstance, FInstanceHom, FieldPresence, GEdge,
    GInstance, GraphKanError, GraphKanHom, GraphLayout, HomError, MigrationFunctor, Node,
    NodeShape, NonConformance, NotEncodableReason, PiProvenance, RelSchema, RelSchemaError,
    RestrictError, Row, RowId, SigmaProvenance, TreeShapeReport, Value, WInstance,
};
use panproto_reference::enumerate::kan_extensions as scope;
use panproto_reference::{encode, model, rel, value as rvalue};
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, Vertex};
use proptest::prelude::*;
use smallvec::SmallVec;

// ---------------------------------------------------------------------------
// Values, edges, schemas and migrations.
// ---------------------------------------------------------------------------

/// The production value of a reference value. The Kan scopes hold booleans,
/// integers, float bit patterns, strings, byte strings, tokens, links,
/// explicit nulls, records, lists and labelled nulls; any other reference
/// value is outside every scope here and fails the test.
fn prod_value(value: &rvalue::Value) -> Value {
    match value {
        rvalue::Value::Bool(b) => Value::Bool(*b),
        rvalue::Value::Int(i) => Value::Int(*i),
        rvalue::Value::Float(bits) => Value::Float(f64::from_bits(*bits)),
        rvalue::Value::Str(s) => Value::Str(s.clone()),
        rvalue::Value::Bytes(b) => Value::Bytes(b.clone()),
        rvalue::Value::CidLink(s) => Value::CidLink(s.clone()),
        rvalue::Value::Token(s) => Value::Token(s.clone()),
        rvalue::Value::Null => Value::Null,
        rvalue::Value::Unknown(fields) => Value::Unknown(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), prod_value(v)))
                .collect(),
        ),
        rvalue::Value::List(items) => Value::List(items.iter().map(prod_value).collect()),
        rvalue::Value::LabeledNull(id) => Value::LabeledNull(*id),
        other => panic!("the Kan scopes hold no value {other:?}"),
    }
}

/// The reference value of a production value: the left inverse of
/// [`prod_value`] on its image.
fn ref_value(value: &Value) -> rvalue::Value {
    match value {
        Value::Bool(b) => rvalue::Value::Bool(*b),
        Value::Int(i) => rvalue::Value::Int(*i),
        Value::Float(f) => rvalue::Value::Float(f.to_bits()),
        Value::Str(s) => rvalue::Value::Str(s.clone()),
        Value::Bytes(b) => rvalue::Value::Bytes(b.clone()),
        Value::CidLink(s) => rvalue::Value::CidLink(s.clone()),
        Value::Token(s) => rvalue::Value::Token(s.clone()),
        Value::Null => rvalue::Value::Null,
        Value::Unknown(fields) => rvalue::Value::Unknown(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), ref_value(v)))
                .collect(),
        ),
        Value::List(items) => rvalue::Value::List(items.iter().map(ref_value).collect()),
        Value::LabeledNull(id) => rvalue::Value::LabeledNull(*id),
        other => panic!("the Kan scopes hold no value {other:?}"),
    }
}

fn prod_presence(presence: &rvalue::Presence) -> FieldPresence {
    match presence {
        rvalue::Presence::Present(value) => FieldPresence::Present(prod_value(value)),
        rvalue::Presence::Null => FieldPresence::Null,
        rvalue::Presence::Absent => FieldPresence::Absent,
    }
}

fn ref_presence(presence: &FieldPresence) -> rvalue::Presence {
    match presence {
        FieldPresence::Present(value) => rvalue::Presence::Present(ref_value(value)),
        FieldPresence::Null => rvalue::Presence::Null,
        FieldPresence::Absent => rvalue::Presence::Absent,
    }
}

fn prod_edge(edge: &model::SEdge) -> Edge {
    Edge {
        src: edge.src.as_str().into(),
        tgt: edge.tgt.as_str().into(),
        kind: edge.kind.as_str().into(),
        name: edge.name.as_deref().map(Into::into),
    }
}

fn ref_edge(edge: &Edge) -> model::SEdge {
    model::SEdge::new(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
}

/// The production schema of a reference schema: its vertices with their
/// kinds and its edges, each edge's kind as the edge kind; with
/// `declared`, every vertex holding a key in the reference schema holds one
/// key in the production schema too.
fn prod_schema(schema: &panproto_reference::schema::Schema) -> Schema {
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut edges = HashMap::new();
    for edge in &schema.edges {
        let edge = prod_edge(edge);
        between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge.clone());
        outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        edges.insert(edge.clone(), edge.kind.clone());
    }
    let mut out = Schema {
        protocol: "test".into(),
        vertices: schema
            .vertices
            .iter()
            .map(|(id, kind)| {
                (
                    Name::from(id.as_str()),
                    Vertex {
                        id: id.as_str().into(),
                        kind: kind.as_str().into(),
                        nsid: None,
                    },
                )
            })
            .collect(),
        edges,
        outgoing,
        incoming,
        between,
        ..Schema::default()
    };
    for (vertex, keys) in &schema.keys {
        if keys.is_empty() {
            continue;
        }
        out.declarations.keys.insert(
            vertex.as_str().into(),
            keys.iter()
                .map(|key| panproto_schema::Key {
                    id: key.id.as_str().into(),
                    parts: Vec::new(),
                    role: panproto_schema::KeyRole::Primary,
                    scope: panproto_schema::KeyScope::Document,
                })
                .collect(),
        );
    }
    out
}

/// The scalar kind a value kind name stands for in the test protocols.
fn scalar_of(kind: &str) -> ScalarKind {
    match kind {
        "int" => ScalarKind::Int {
            bits: 64,
            signed: true,
        },
        "str" => ScalarKind::Str,
        _ => ScalarKind::Any,
    }
}

/// The production protocol whose value kinds are `value` and whose
/// non-value kinds are `non_value`.
fn protocol_of(value: &BTreeSet<String>, non_value: &BTreeSet<String>) -> Protocol {
    Protocol {
        value_kinds: value
            .iter()
            .map(|kind| (kind.clone(), scalar_of(kind)))
            .collect(),
        non_value_kinds: non_value.clone(),
        ..Protocol::default()
    }
}

fn kinds(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// The kind partition of the O14 family: `value` values, `record` tables.
fn family_kinds() -> (BTreeSet<String>, BTreeSet<String>) {
    (kinds(&["value"]), kinds(&["record"]))
}

/// The kind partition of the graph specs.
fn graph_spec_kinds() -> (BTreeSet<String>, BTreeSet<String>) {
    (
        kinds(&scope::GRAPH_SPEC_VALUE_KINDS),
        kinds(&[scope::GRAPH_SPEC_TABLE_KIND]),
    )
}

/// The kind partition of the generated table specs.
fn table_spec_kinds() -> (BTreeSet<String>, BTreeSet<String>) {
    (
        kinds(&[scope::TABLE_SPEC_VALUE_KIND]),
        kinds(&[scope::TABLE_SPEC_TABLE_KIND]),
    )
}

fn prod_migration(migration: &panproto_reference::migration::Migration) -> CompiledMigration {
    CompiledMigration {
        surviving_verts: migration
            .surviving_verts
            .iter()
            .map(|v| Name::from(v.as_str()))
            .collect(),
        surviving_edges: migration.surviving_edges.iter().map(prod_edge).collect(),
        vertex_remap: migration
            .vertex_remap
            .iter()
            .map(|(a, b)| (Name::from(a.as_str()), Name::from(b.as_str())))
            .collect(),
        edge_remap: migration
            .edge_remap
            .iter()
            .map(|(a, b)| (prod_edge(a), prod_edge(b)))
            .collect(),
        ..CompiledMigration::default()
    }
}

/// A functor under test with its reference counterpart, both built from one
/// pair of reference schemas and one reference migration.
struct Functors {
    production: MigrationFunctor,
    reference: rel::RelFunctor,
    source_schema: Schema,
    target_schema: Schema,
    protocol: Protocol,
    migration: CompiledMigration,
    reference_source: panproto_reference::schema::Schema,
    reference_target: panproto_reference::schema::Schema,
    reference_migration: panproto_reference::migration::Migration,
    partition: encode::KindPartition,
}

impl Functors {
    fn new(
        source: &panproto_reference::schema::Schema,
        target: &panproto_reference::schema::Schema,
        migration: &panproto_reference::migration::Migration,
        (value, non_value): &(BTreeSet<String>, BTreeSet<String>),
    ) -> Self {
        let protocol = protocol_of(value, non_value);
        let source_schema = prod_schema(source);
        let target_schema = prod_schema(target);
        let compiled = prod_migration(migration);
        let production = MigrationFunctor::new(
            &source_schema,
            &protocol,
            &target_schema,
            &protocol,
            &compiled,
        )
        .unwrap_or_else(|e| panic!("the production functor: {e}"));
        let src = rel::classify(source, value, non_value).unwrap();
        let tgt = rel::classify(target, value, non_value).unwrap();
        let reference = rel::entity_functor(&src, &tgt, migration)
            .unwrap()
            .into_kan()
            .unwrap();
        Self {
            production,
            reference,
            source_schema,
            target_schema,
            protocol,
            migration: compiled,
            reference_source: source.clone(),
            reference_target: target.clone(),
            reference_migration: migration.clone(),
            partition: encode::KindPartition {
                value: value.clone(),
                non_value: non_value.clone(),
            },
        }
    }

    const fn protocols(&self) -> BoundaryProtocols<'_> {
        BoundaryProtocols {
            src: &self.protocol,
            tgt: &self.protocol,
        }
    }
}

// ---------------------------------------------------------------------------
// Tables and homomorphisms.
// ---------------------------------------------------------------------------

fn prod_row(cells: &BTreeMap<String, rvalue::Value>) -> Row {
    cells
        .iter()
        .map(|(column, value)| (column.clone(), prod_value(value)))
        .collect()
}

/// The production instance of a reference table: its rows by id and table,
/// and its pairs.
fn prod_table(table: &model::Table) -> FInstance {
    let mut tables: BTreeMap<String, Vec<(RowId, Row)>> = BTreeMap::new();
    for (id, (name, cells)) in &table.rows {
        tables
            .entry(name.clone())
            .or_default()
            .push((RowId(*id), prod_row(cells)));
    }
    let mut pairs: BTreeMap<model::SEdge, Vec<(RowId, RowId)>> = BTreeMap::new();
    for (edge, source, target) in &table.pairs {
        pairs
            .entry(edge.clone())
            .or_default()
            .push((RowId(*source), RowId(*target)));
    }
    let instance = FInstance::from_parts(
        tables
            .into_iter()
            .map(|(name, rows)| (Name::from(name.as_str()), rows)),
        pairs
            .into_iter()
            .map(|(edge, pairs)| (prod_edge(&edge), pairs)),
    )
    .unwrap_or_else(|e| panic!("the production instance of {table:?}: {e}"));
    assert_eq!(&ref_table(&instance), table, "the table adapter round trip");
    instance
}

/// The reference table of a production instance: its rows and its pairs.
fn ref_table(instance: &FInstance) -> model::Table {
    let mut out = model::Table::default();
    for id in instance.row_ids() {
        let table = instance.table_of(id).unwrap();
        let cells = instance
            .row(id)
            .unwrap()
            .iter()
            .map(|(column, value)| (column.clone(), ref_value(value)))
            .collect();
        out.rows.insert(id.0, (table.to_string(), cells));
    }
    for (edge, pairs) in instance.foreign_keys() {
        for (source, target) in pairs {
            out.pairs.insert((ref_edge(edge), source.0, target.0));
        }
    }
    out
}

fn prod_hom(hom: &rel::Hom) -> FInstanceHom {
    FInstanceHom::new(
        hom.rows
            .iter()
            .map(|(a, b)| (RowId(*a), RowId(*b)))
            .collect(),
    )
    .with_null_map(
        hom.nulls
            .iter()
            .map(|(null, value)| (*null, prod_value(value)))
            .collect(),
    )
}

/// The reference homomorphism of a production one. A homomorphism of the
/// Kan extensions renames no table and no edge.
fn ref_hom(hom: &FInstanceHom) -> rel::Hom {
    assert!(hom.table_map.is_empty(), "{hom:?} renames tables");
    assert!(hom.edge_map.is_empty(), "{hom:?} renames edges");
    rel::Hom {
        rows: hom.row_map.iter().map(|(a, b)| (a.0, b.0)).collect(),
        nulls: hom
            .null_map
            .iter()
            .map(|(null, value)| (*null, ref_value(value)))
            .collect(),
    }
}

fn ref_graph_hom(hom: &GraphKanHom) -> rel::Hom {
    rel::Hom {
        rows: hom.node_map.clone(),
        nulls: hom
            .null_map
            .iter()
            .map(|(null, value)| (*null, ref_value(value)))
            .collect(),
    }
}

fn prod_graph_hom(hom: &rel::Hom) -> GraphKanHom {
    GraphKanHom {
        node_map: hom.rows.clone(),
        null_map: hom
            .nulls
            .iter()
            .map(|(null, value)| (*null, prod_value(value)))
            .collect(),
    }
}

fn ref_sigma_provenance(provenance: &SigmaProvenance) -> rel::SigmaProvenance {
    rel::SigmaProvenance {
        fresh: provenance
            .fresh
            .iter()
            .map(|(null, (row, column))| (*null, (row.0, column.clone())))
            .collect(),
        merged: provenance
            .merged
            .iter()
            .map(|(null, value)| (*null, ref_value(value)))
            .collect(),
    }
}

fn ref_delta_provenance(provenance: &DeltaProvenance) -> rel::DeltaProvenance {
    provenance
        .copies
        .iter()
        .map(|(copy, (table, row))| (copy.0, (table.to_string(), row.0)))
        .collect()
}

fn ref_pi_provenance(provenance: &PiProvenance) -> rel::PiProvenance {
    provenance
        .tuples
        .iter()
        .map(|(row, (table, tuple))| {
            (
                row.0,
                (table.to_string(), tuple.iter().map(|r| r.0).collect()),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Graphs, layouts and trees.
// ---------------------------------------------------------------------------

fn prod_shape(shape: &model::NodeShape) -> NodeShape {
    match shape {
        model::NodeShape::Plain => NodeShape::Plain,
        model::NodeShape::List => NodeShape::List,
        model::NodeShape::XmlElement(tag) => NodeShape::XmlElement {
            tag: tag.as_str().into(),
        },
        model::NodeShape::XmlTextSegment => NodeShape::XmlTextSegment,
        model::NodeShape::HyperEdge => NodeShape::HyperEdge,
    }
}

fn ref_shape(shape: &NodeShape) -> model::NodeShape {
    match shape {
        NodeShape::Plain => model::NodeShape::Plain,
        NodeShape::List => model::NodeShape::List,
        NodeShape::XmlElement { tag } => model::NodeShape::XmlElement(tag.to_string()),
        NodeShape::XmlTextSegment => model::NodeShape::XmlTextSegment,
        NodeShape::HyperEdge => model::NodeShape::HyperEdge,
    }
}

fn prod_node(node: &model::Node) -> Node {
    let mut out = Node::new(node.id, node.anchor.as_str());
    out.value = node.value.as_ref().map(prod_presence);
    out.discriminator = node.discriminator.as_deref().map(Into::into);
    out.extra_fields = node
        .fields
        .iter()
        .map(|(k, v)| (k.clone(), prod_value(v)))
        .collect();
    out.position = node.position;
    out.shape = prod_shape(&node.shape);
    out.annotations = node
        .annotations
        .iter()
        .map(|(k, v)| (k.clone(), prod_value(v)))
        .collect();
    out
}

fn ref_node(node: &Node) -> model::Node {
    model::Node {
        id: node.id,
        anchor: node.anchor.to_string(),
        value: node.value.as_ref().map(ref_presence),
        discriminator: node.discriminator.as_ref().map(ToString::to_string),
        fields: node
            .extra_fields
            .iter()
            .map(|(k, v)| (k.clone(), ref_value(v)))
            .collect(),
        position: node.position,
        shape: ref_shape(&node.shape),
        annotations: node
            .annotations
            .iter()
            .map(|(k, v)| (k.clone(), ref_value(v)))
            .collect(),
    }
}

fn prod_gedge(edge: &model::GEdge) -> GEdge {
    GEdge::new(edge.src, edge.tgt, prod_edge(&edge.label)).with_position(edge.position)
}

fn ref_gedge(edge: &GEdge) -> model::GEdge {
    model::GEdge {
        src: edge.src,
        tgt: edge.tgt,
        label: ref_edge(&edge.edge),
        position: edge.position,
    }
}

/// The production graph of a reference graph: its nodes and edges by id.
fn prod_graph(graph: &model::Graph) -> GInstance {
    let mut out = GInstance::new();
    for node in graph.nodes.values() {
        out.insert_node(prod_node(node))
            .unwrap_or_else(|e| panic!("{e}"));
    }
    for (id, edge) in &graph.edges {
        out.insert_edge(EdgeId(*id), prod_gedge(edge))
            .unwrap_or_else(|e| panic!("{e}"));
    }
    assert_eq!(&ref_graph(&out), graph, "the graph adapter round trip");
    out
}

/// The reference graph of a production graph.
fn ref_graph(graph: &GInstance) -> model::Graph {
    model::Graph {
        nodes: graph
            .nodes()
            .iter()
            .map(|(id, node)| (*id, ref_node(node)))
            .collect(),
        edges: graph
            .edges()
            .iter()
            .map(|(id, edge)| (id.0, ref_gedge(edge)))
            .collect(),
    }
}

fn ref_edge_layout(layout: &EdgeLayout) -> encode::EdgeLayout {
    encode::EdgeLayout {
        id: layout.id.map(|id| id.0),
        position: layout.position,
        has_source: layout.has_source,
    }
}

fn prod_edge_layout(layout: &encode::EdgeLayout) -> EdgeLayout {
    EdgeLayout {
        id: layout.id.map(EdgeId),
        position: layout.position,
        has_source: layout.has_source,
    }
}

fn ref_layout(layout: &GraphLayout) -> encode::GraphLayout {
    encode::GraphLayout {
        table_nodes: layout
            .table_nodes
            .iter()
            .map(|(row, node)| (row.0, ref_node(node)))
            .collect(),
        cells: layout
            .cells
            .iter()
            .map(|((row, column), cell)| {
                (
                    (row.0, column.clone()),
                    encode::CellLayout {
                        root: cell.root,
                        attribute: ref_edge_layout(&cell.attribute),
                        nodes: cell
                            .nodes
                            .iter()
                            .map(|(id, node)| (*id, ref_node(node)))
                            .collect(),
                        edges: cell
                            .edges
                            .iter()
                            .map(|(id, edge)| (id.0, ref_gedge(edge)))
                            .collect(),
                    },
                )
            })
            .collect(),
        relations: layout
            .relations
            .iter()
            .map(|((edge, a, b), relation)| ((ref_edge(edge), a.0, b.0), ref_edge_layout(relation)))
            .collect(),
        reserved_node_ids: layout.reserved_node_ids.clone(),
        reserved_edge_ids: layout.reserved_edge_ids.iter().map(|id| id.0).collect(),
    }
}

fn prod_layout(layout: &encode::GraphLayout) -> GraphLayout {
    GraphLayout {
        table_nodes: layout
            .table_nodes
            .iter()
            .map(|(row, node)| (RowId(*row), prod_node(node)))
            .collect(),
        cells: layout
            .cells
            .iter()
            .map(|((row, column), cell)| {
                (
                    (RowId(*row), column.clone()),
                    CellLayout {
                        root: cell.root,
                        attribute: prod_edge_layout(&cell.attribute),
                        nodes: cell
                            .nodes
                            .iter()
                            .map(|(id, node)| (*id, prod_node(node)))
                            .collect(),
                        edges: cell
                            .edges
                            .iter()
                            .map(|(id, edge)| (EdgeId(*id), prod_gedge(edge)))
                            .collect(),
                    },
                )
            })
            .collect(),
        relations: layout
            .relations
            .iter()
            .map(|((edge, a, b), relation)| {
                (
                    (prod_edge(edge), RowId(*a), RowId(*b)),
                    prod_edge_layout(relation),
                )
            })
            .collect(),
        reserved_node_ids: layout.reserved_node_ids.clone(),
        reserved_edge_ids: layout
            .reserved_edge_ids
            .iter()
            .map(|id| EdgeId(*id))
            .collect(),
    }
}

fn ref_contributors(
    nodes: &BTreeMap<u32, BTreeSet<u32>>,
    edges: &BTreeMap<EdgeId, BTreeSet<EdgeId>>,
) -> (BTreeMap<u32, BTreeSet<u32>>, BTreeMap<u32, BTreeSet<u32>>) {
    (
        nodes.clone(),
        edges
            .iter()
            .map(|(id, sources)| (id.0, sources.iter().map(|e| e.0).collect()))
            .collect(),
    )
}

/// The production tree of a fan-free reference tree, its schema root the
/// root's anchor.
fn prod_tree(tree: &model::Tree) -> WInstance {
    assert!(tree.fans.is_empty(), "the tree scopes are fan-free");
    let root_anchor = tree.nodes[&tree.root].anchor.clone();
    WInstance::new(
        tree.nodes
            .iter()
            .map(|(id, node)| (*id, prod_node(node)))
            .collect(),
        tree.arcs
            .iter()
            .map(|(a, b, edge)| (*a, *b, prod_edge(edge)))
            .collect(),
        Vec::new(),
        tree.root,
        root_anchor.as_str().into(),
    )
}

/// The reference tree of a fan-free production tree.
fn ref_tree(tree: &WInstance) -> model::Tree {
    assert!(tree.fans.is_empty(), "the tree scopes are fan-free");
    model::Tree {
        nodes: tree
            .nodes
            .iter()
            .map(|(id, node)| (*id, ref_node(node)))
            .collect(),
        arcs: tree
            .arcs
            .iter()
            .map(|(a, b, edge)| (*a, *b, ref_edge(edge)))
            .collect(),
        fans: Vec::new(),
        root: tree.root,
    }
}

// ---------------------------------------------------------------------------
// Refusals. Each production refusal maps to at most one reference refusal;
// `None` marks a production refusal the reference has no counterpart for,
// which is always a disagreement.
// ---------------------------------------------------------------------------

fn ref_conformance(reason: &NonConformance) -> rel::Conformance {
    match reason {
        NonConformance::TableOutside { .. } => rel::Conformance::TableOutside,
        NonConformance::MissingCell => rel::Conformance::MissingCell,
        NonConformance::StrayCell => rel::Conformance::StrayCell,
        NonConformance::NestedLabeledNull => rel::Conformance::NestedLabeledNull,
        NonConformance::RelationOutside { edge } => {
            rel::Conformance::RelationOutside(Box::new(ref_edge(edge)))
        }
        other => panic!("no reference conformance reason for {other:?}"),
    }
}

/// The reference Kan refusal of a production table-adjunction refusal.
fn ref_kan_error(error: &AdjunctionError) -> Option<rel::KanError> {
    Some(match error {
        AdjunctionError::NonConforming {
            row,
            column,
            reason,
        } => rel::KanError::NonConforming(Box::new(rel::NonConforming {
            row: row.0,
            column: column.clone(),
            reason: ref_conformance(reason),
        })),
        AdjunctionError::SigmaConstantClash {
            row,
            column,
            left,
            right,
        } => rel::KanError::SigmaConstantClash {
            row: row.0,
            column: column.clone(),
            left: Box::new(ref_value(left)),
            right: Box::new(ref_value(right)),
        },
        AdjunctionError::Pi(RestrictError::AttributeOutsideImage { table, column }) => {
            rel::KanError::AttributeOutsideImage {
                table: table.to_string(),
                column: column.clone(),
            }
        }
        AdjunctionError::Pi(RestrictError::ProductSizeExceeded {
            vertex,
            actual,
            limit,
        }) => rel::KanError::ProductSizeExceeded {
            vertex: vertex.clone(),
            actual: *actual,
            limit: *limit,
        },
        AdjunctionError::RowIdsExhausted => rel::KanError::RowIdsExhausted,
        AdjunctionError::NullIdsExhausted => rel::KanError::NullIdsExhausted,
        _ => return None,
    })
}

fn ref_rel_schema_error(error: &RelSchemaError) -> Option<rel::RelSchemaError> {
    Some(match error {
        RelSchemaError::AmbiguousKind { kind } => {
            rel::RelSchemaError::AmbiguousKind { kind: kind.clone() }
        }
        RelSchemaError::UnclassifiedKind { vertex, kind } => {
            rel::RelSchemaError::UnclassifiedKind {
                vertex: vertex.to_string(),
                kind: kind.to_string(),
            }
        }
        RelSchemaError::ValueReachesTable { edge } => rel::RelSchemaError::ValueReachesTable {
            edge: ref_edge(edge),
        },
        RelSchemaError::DuplicateColumn { table, column } => rel::RelSchemaError::DuplicateColumn {
            table: table.to_string(),
            column: column.clone(),
        },
        _ => return None,
    })
}

fn ref_functor_error(error: &RestrictError) -> Option<rel::FunctorError> {
    Some(match error {
        RestrictError::VertexImageNotATable { vertex, image } => {
            rel::FunctorError::VertexImageNotATable {
                vertex: vertex.to_string(),
                image: image.to_string(),
            }
        }
        RestrictError::IncoherentEdgeImage { source_edge, image } => {
            rel::FunctorError::IncoherentEdgeImage {
                edge: Box::new(ref_edge(source_edge)),
                image: Box::new(ref_edge(image)),
            }
        }
        RestrictError::IncoherentAttributeImage { edge, image } => {
            rel::FunctorError::IncoherentAttributeImage {
                edge: Box::new(ref_edge(edge)),
                image: image.as_ref().map(|image| Box::new(ref_edge(image))),
            }
        }
        RestrictError::ValueStructureRemap { edge } => rel::FunctorError::ValueStructureRemap {
            edge: Box::new(ref_edge(edge)),
        },
        _ => return None,
    })
}

/// The production rendering of a reference homomorphism refusal: the
/// production payload names a relation by its endpoints alone.
fn prod_hom_error(error: &rel::HomError) -> HomError {
    match error {
        rel::HomError::NodeNotMapped(row) => HomError::NodeNotMapped(*row),
        rel::HomError::ImageNodeMissing { src, tgt } => HomError::ImageNodeMissing {
            src: *src,
            tgt: *tgt,
        },
        rel::HomError::AnchorMismatch {
            src,
            tgt,
            src_anchor,
            tgt_anchor,
        } => HomError::AnchorMismatch {
            src: *src,
            tgt: *tgt,
            src_anchor: src_anchor.as_str().into(),
            tgt_anchor: tgt_anchor.as_str().into(),
        },
        rel::HomError::AttributeMismatch { src, tgt } => HomError::AttributeMismatch {
            src: *src,
            tgt: *tgt,
        },
        rel::HomError::ForeignKeyNotPreserved {
            edge,
            mapped_src,
            mapped_tgt,
        } => HomError::ForeignKeyNotPreserved {
            edge: format!("{}->{}", edge.src, edge.tgt),
            mapped_src: *mapped_src,
            mapped_tgt: *mapped_tgt,
        },
        rel::HomError::ComposeNodeMismatch(row) => HomError::ComposeNodeMismatch(*row),
    }
}

fn ref_not_encodable(reason: &NotEncodableReason) -> Option<encode::NotEncodable> {
    let ids = |edges: &[EdgeId]| edges.iter().map(|e| e.0).collect::<Vec<_>>();
    Some(match reason {
        NotEncodableReason::FanNode { node } => encode::NotEncodable::FanNode { node: *node },
        NotEncodableReason::AnchorOutsideSide { node } => {
            encode::NotEncodable::AnchorOutsideSide { node: *node }
        }
        NotEncodableReason::EdgeOutsideSide { edge } => {
            encode::NotEncodable::EdgeOutsideSide { edge: edge.0 }
        }
        NotEncodableReason::IncoherentEndpoints { edge } => {
            encode::NotEncodable::IncoherentEndpoints { edge: edge.0 }
        }
        NotEncodableReason::ParallelEdges { edges } => {
            encode::NotEncodable::ParallelEdges { edges: ids(edges) }
        }
        NotEncodableReason::MissingCell { node, column } => encode::NotEncodable::MissingCell {
            node: *node,
            column: column.clone(),
        },
        NotEncodableReason::CellAsExtraField { node, column } => {
            encode::NotEncodable::CellAsExtraField {
                node: *node,
                column: column.clone(),
            }
        }
        NotEncodableReason::RepeatedCell {
            node,
            column,
            edges,
        } => encode::NotEncodable::RepeatedCell {
            node: *node,
            column: column.clone(),
            edges: ids(edges),
        },
        NotEncodableReason::SharedValue { node, edges } => encode::NotEncodable::SharedValue {
            node: *node,
            edges: ids(edges),
        },
        NotEncodableReason::UnrootedValue { node } => {
            encode::NotEncodable::UnrootedValue { node: *node }
        }
        NotEncodableReason::MissingValue { node } => {
            encode::NotEncodable::MissingValue { node: *node }
        }
        NotEncodableReason::InnerValueShape { node } => {
            encode::NotEncodable::InnerValueShape { node: *node }
        }
        NotEncodableReason::NestedLabeledNull { node } => {
            encode::NotEncodable::NestedLabeledNull { node: *node }
        }
        _ => return None,
    })
}

/// The reference graph-adjunction refusal of a production one.
fn ref_graph_adjunction_error(error: &AdjunctionError) -> Option<rel::AdjunctionError> {
    ref_kan_error(error).map(rel::AdjunctionError::from)
}

/// The reference graph Kan refusal of a production one.
fn ref_graph_kan_error(error: &GraphKanError) -> Option<encode::GraphKanError> {
    Some(match error {
        GraphKanError::NotEncodable { reason } => {
            encode::GraphKanError::NotEncodable(Box::new(ref_not_encodable(reason)?))
        }
        GraphKanError::RelSchema(error) => {
            encode::GraphKanError::RelSchema(Box::new(ref_rel_schema_error(error)?))
        }
        GraphKanError::Functor(error) => match error {
            RestrictError::RelSchema(error) => {
                encode::GraphKanError::RelSchema(Box::new(ref_rel_schema_error(error)?))
            }
            other => encode::GraphKanError::Functor(Box::new(ref_functor_error(other)?)),
        },
        GraphKanError::Adjunction(error) => {
            encode::GraphKanError::Adjunction(Box::new(ref_graph_adjunction_error(error)?))
        }
        GraphKanError::NodeIdsExhausted => encode::GraphKanError::NodeIdsExhausted,
        GraphKanError::EdgeIdsExhausted => encode::GraphKanError::EdgeIdsExhausted,
        GraphKanError::EdgePositionsExhausted { node } => {
            encode::GraphKanError::EdgePositionsExhausted { node: *node }
        }
        _ => return None,
    })
}

/// Whether a production graph-Kan result agrees with the reference one:
/// equal graphs, or refusals that map to the same reference refusal.
fn graph_results_agree<T: std::fmt::Debug>(
    production: &Result<GInstance, GraphKanError>,
    reference: &Result<model::Graph, encode::GraphKanError>,
    context: &T,
) {
    match (production, reference) {
        (Ok(ours), Ok(theirs)) => assert_eq!(&ref_graph(ours), theirs, "{context:?}"),
        (Err(ours), Err(theirs)) => graph_errors_agree(ours, theirs, context),
        (ours, theirs) => panic!("{context:?}: production {ours:?}, reference {theirs:?}"),
    }
}

/// Whether a production graph-Kan refusal is the reference one.
fn graph_errors_agree<T: std::fmt::Debug>(
    production: &GraphKanError,
    reference: &encode::GraphKanError,
    context: &T,
) {
    match (production, reference) {
        (GraphKanError::Hom(ours), encode::GraphKanError::Hom(theirs)) => {
            assert_eq!(*ours, prod_hom_error(theirs), "{context:?}");
        }
        (ours, theirs) => assert_eq!(
            ref_graph_kan_error(ours).as_ref(),
            Some(theirs),
            "{context:?}: production {ours:?}"
        ),
    }
}

/// Whether a production table-Kan refusal is the reference one.
fn kan_refusals_agree<T: std::fmt::Debug>(
    production: &AdjunctionError,
    reference: &rel::KanError,
    context: &T,
) {
    assert_eq!(
        ref_kan_error(production).as_ref(),
        Some(reference),
        "{context:?}: production {production:?}"
    );
}

/// Whether a production homomorphism result agrees with a reference one:
/// equal homomorphisms on `domain` (row maps exact, null images by canonical
/// bytes), or both refused.
fn hom_results_agree<T: std::fmt::Debug>(
    production: &Result<FInstanceHom, AdjunctionError>,
    reference: &Result<rel::Hom, rel::TransposeError>,
    domain: &model::Table,
    context: &T,
) {
    match (production, reference) {
        (Ok(ours), Ok(theirs)) => assert!(
            ref_hom(ours).same_as(theirs, domain),
            "{context:?}: production {ours:?}, reference {theirs:?}"
        ),
        (Err(ours), Err(rel::TransposeError::Adjunction(theirs))) => {
            assert_eq!(
                ref_graph_adjunction_error(ours).as_ref(),
                Some(theirs.as_ref()),
                "{context:?}: production {ours:?}"
            );
        }
        (ours, theirs) => panic!("{context:?}: production {ours:?}, reference {theirs:?}"),
    }
}

// ---------------------------------------------------------------------------
// Execution helpers.
// ---------------------------------------------------------------------------

/// Worker threads: the positive integer in `PANPROTO_KAN_THREADS`, else
/// four.
fn threads() -> usize {
    std::env::var("PANPROTO_KAN_THREADS").map_or(4, |text| {
        text.parse::<usize>()
            .ok()
            .filter(|threads| *threads > 0)
            .expect("PANPROTO_KAN_THREADS must be a positive integer")
    })
}

/// Run `check` on every element of `items` over [`threads`] workers, each
/// taking a contiguous chunk, and sum what they return. The first panic is
/// raised again once every worker has stopped.
fn par_sum<T: Sync, S: Default + Send + std::ops::AddAssign>(
    items: &[T],
    check: impl Fn(&T) -> S + Sync,
) -> S {
    let workers = threads().clamp(1, items.len().max(1));
    let chunk = items.len().div_ceil(workers).max(1);
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|part| {
                let (check, stop) = (&check, &stop);
                scope.spawn(move || {
                    let mut total = S::default();
                    for item in part {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(item)))
                        {
                            Ok(part) => total += part,
                            Err(panic) => {
                                stop.store(true, Ordering::Relaxed);
                                std::panic::resume_unwind(panic);
                            }
                        }
                    }
                    total
                })
            })
            .collect();
        let mut total = S::default();
        let mut first_panic = None;
        for handle in handles {
            match handle.join() {
                Ok(part) => total += part,
                Err(panic) => {
                    first_panic.get_or_insert(panic);
                }
            }
        }
        if let Some(panic) = first_panic {
            std::panic::resume_unwind(panic);
        }
        total
    })
}

/// A counter of named outcomes, summed across workers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Census(BTreeMap<String, u64>);

impl Census {
    fn one(name: impl Into<String>) -> Self {
        Self(BTreeMap::from([(name.into(), 1)]))
    }

    fn add(&mut self, name: impl Into<String>, count: u64) {
        *self.0.entry(name.into()).or_default() += count;
    }

    fn get(&self, name: &str) -> u64 {
        self.0.get(name).copied().unwrap_or_default()
    }
}

impl std::ops::AddAssign for Census {
    fn add_assign(&mut self, other: Self) {
        for (name, count) in other.0 {
            self.add(name, count);
        }
    }
}

// ---------------------------------------------------------------------------
// The tree form of a decoded graph.
// ---------------------------------------------------------------------------

/// The roots of a fan-free graph: the nodes no edge reaches.
fn graph_roots(graph: &model::Graph) -> Vec<u32> {
    let reached: BTreeSet<u32> = graph.edges.values().map(|edge| edge.tgt).collect();
    graph
        .nodes
        .keys()
        .filter(|node| !reached.contains(node))
        .copied()
        .collect()
}

/// The tree a decoded input graph is, when it is one: its unique root and the
/// reference tree form from there. This is the input conversion of the tree
/// routes, shared by both sides; the routes compare production and reference
/// on the tree it gives.
fn input_tree(graph: &model::Graph) -> Option<model::Tree> {
    let [root] = graph_roots(graph)[..] else {
        return None;
    };
    encode::graph_to_tree(graph, root, &encode::schema_parent_name(None)).ok()
}

/// The production read-back of a graph-route output as a tree, in the
/// reference's terms: the tree from the unique root, the number of roots
/// when there is not exactly one, or the production reason it is no tree.
#[derive(Debug, PartialEq, Eq)]
enum ReadBack {
    Tree(model::Tree),
    RootCount(usize),
    NotATree(String),
}

fn production_read_back(graph: &GInstance, schema: &Schema) -> ReadBack {
    match graph.tree_shape(schema) {
        TreeShapeReport::Tree { root } => match graph.to_wtype(root) {
            Ok(tree) => ReadBack::Tree(ref_tree(&tree)),
            Err(error) => ReadBack::NotATree(format!("{error:?}")),
        },
        TreeShapeReport::Forest { roots } => ReadBack::RootCount(roots.len()),
        TreeShapeReport::NotTree { reason } if graph.nodes().is_empty() => {
            assert!(matches!(reason, panproto_inst::NotTreeReason::NoRoot));
            ReadBack::RootCount(0)
        }
        TreeShapeReport::NotTree { reason } => ReadBack::NotATree(format!("{reason:?}")),
    }
}

/// The variant name of a Debug rendering: its text up to the first space,
/// brace or parenthesis.
fn variant_name(debug: &str) -> &str {
    debug.split([' ', '{', '(']).next().unwrap_or(debug)
}

/// The input conversion of a decoded graph that is no tree, on both sides:
/// a root count other than one must be production's root count exactly;
/// otherwise the label pairs the reference `NotATree` reason with the
/// production reason, so that the census records the correspondence of the
/// two vocabularies.
fn input_refusal_agrees(
    graph: &model::Graph,
    production_graph: &GInstance,
    schema: &Schema,
) -> String {
    let roots = graph_roots(graph);
    let production = production_read_back(production_graph, schema);
    if let [root] = roots[..] {
        let reason = encode::graph_to_tree(graph, root, &encode::schema_parent_name(None))
            .expect_err("a graph the input conversion refuses");
        let ReadBack::NotATree(ours) = &production else {
            panic!("reference NotATree({reason:?}), production {production:?}");
        };
        format!(
            "input NotATree({} / {})",
            variant_name(&format!("{reason:?}")),
            variant_name(ours)
        )
    } else {
        assert_eq!(production, ReadBack::RootCount(roots.len()), "{graph:?}");
        format!("input RootCount({})", roots.len())
    }
}

/// One tree route of the bounded family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TreeRoute {
    Sigma,
    Pi(usize),
    Delta,
}

/// A reference tree refusal for a merging map, checked against the
/// production tree function's own refusal; `None` for any other outcome.
fn merging_refusal_agrees(
    functors: &Functors,
    route: TreeRoute,
    tree: &model::Tree,
    production_tree: &WInstance,
    expected: &Result<model::Tree, encode::TreeKanError>,
) -> Option<String> {
    if let Err(encode::TreeKanError::NonInjectiveVertexMap {
        first,
        second,
        target,
    }) = expected
    {
        match route {
            TreeRoute::Delta => {
                match panproto_inst::w_delta(production_tree, &functors.migration) {
                    Err(AdjunctionError::NonInjectiveVertexMap {
                        first: f1,
                        second: s1,
                        target: t1,
                    }) => assert_eq!(
                        (f1.as_ref(), s1.as_ref(), t1.as_ref()),
                        (first.as_str(), second.as_str(), target.as_str())
                    ),
                    other => panic!("tree Delta of {tree:?}: {other:?}"),
                }
            }
            TreeRoute::Pi(bound) => match panproto_inst::wtype_pi(
                production_tree,
                &functors.target_schema,
                &functors.migration,
                bound,
            ) {
                Err(RestrictError::NonInjectiveVertexMap {
                    target: t1,
                    sources,
                }) => {
                    assert_eq!(t1.as_ref(), target.as_str());
                    assert_eq!(
                        sources
                            .iter()
                            .take(2)
                            .map(AsRef::as_ref)
                            .collect::<Vec<&str>>(),
                        vec![first.as_str(), second.as_str()]
                    );
                }
                other => panic!("tree Pi of {tree:?}: {other:?}"),
            },
            TreeRoute::Sigma => panic!("tree Sigma refuses no merging map"),
        }
        return Some("NonInjectiveVertexMap".to_owned());
    }
    if let Err(encode::TreeKanError::NonInjectiveEdgeMap { target }) = expected {
        assert_eq!(route, TreeRoute::Delta);
        match panproto_inst::w_delta(production_tree, &functors.migration) {
            Err(AdjunctionError::NonInjectiveEdgeMap { src, tgt }) => {
                assert_eq!(
                    (src.as_ref(), tgt.as_ref()),
                    (target.src.as_str(), target.tgt.as_str())
                );
            }
            other => panic!("tree Delta of {tree:?}: {other:?}"),
        }
        return Some("NonInjectiveEdgeMap".to_owned());
    }
    None
}

/// On the tree-Kan domain, the production tree function's graph form is
/// the production graph route's output: the production tree square.
fn check_tree_square(
    functors: &Functors,
    route: TreeRoute,
    tree: &model::Tree,
    production_tree: &WInstance,
    out: &GInstance,
) {
    let tree_function = match route {
        TreeRoute::Sigma => panproto_inst::w_sigma(
            production_tree,
            &functors.source_schema,
            &functors.target_schema,
            &functors.migration,
            functors.protocols(),
        )
        .map_err(|e| format!("{e:?}")),
        TreeRoute::Pi(bound) => panproto_inst::wtype_pi(
            production_tree,
            &functors.target_schema,
            &functors.migration,
            bound,
        )
        .map_err(|e| format!("{e:?}")),
        TreeRoute::Delta => panproto_inst::w_delta(production_tree, &functors.migration)
            .map_err(|e| format!("{e:?}")),
    };
    let tree_form = GInstance::from_wtype(&tree_function.unwrap()).unwrap();
    assert!(
        GInstance::identical(&tree_form, out),
        "the production tree square at {route:?} of {tree:?}"
    );
}

/// Compare one tree route: the reference tree Kan extension `expected` of
/// the input tree `tree` against production. A reference refusal for a
/// merging map is the production tree function's own refusal; otherwise the
/// production route is the graph route on the tree's graph form read back as
/// a tree, which must give the reference tree, the reference root count, or
/// the reference graph refusal. On the tree-Kan domain (the map injective
/// and every target table and column in its image) the production tree
/// function's graph form is also the graph route's output. Returns the
/// outcome's census name.
fn check_tree_route(
    functors: &Functors,
    route: TreeRoute,
    tree: &model::Tree,
    expected: &Result<model::Tree, encode::TreeKanError>,
    tree_domain: bool,
) -> String {
    let production_tree = prod_tree(tree);
    let (graph, _) = encode::tree_to_graph(tree, None).unwrap();
    let production_graph = GInstance::from_wtype(&production_tree).unwrap();
    assert_eq!(ref_graph(&production_graph), graph, "the tree's graph form");
    let f = &functors.production;
    if let Some(outcome) = merging_refusal_agrees(functors, route, tree, &production_tree, expected)
    {
        return outcome;
    }
    let (output, schema) = match route {
        TreeRoute::Sigma => (
            panproto_inst::g_sigma_restricted(&production_graph, f),
            &functors.target_schema,
        ),
        TreeRoute::Pi(bound) => (
            panproto_inst::g_pi_restricted(&production_graph, f, bound),
            &functors.target_schema,
        ),
        TreeRoute::Delta => (
            panproto_inst::g_delta_along(&production_graph, f),
            &functors.source_schema,
        ),
    };
    let outcome = match (&output, expected) {
        (Ok(out), Ok(tree)) => {
            let ReadBack::Tree(read) = production_read_back(out, schema) else {
                panic!("{route:?} of {tree:?}: no tree");
            };
            assert!(
                panproto_reference::migration::tree_square(&read, tree),
                "{route:?} of {tree:?}"
            );
            "Ok".to_owned()
        }
        (Ok(out), Err(encode::TreeKanError::RootCount { count })) => {
            assert_eq!(
                production_read_back(out, schema),
                ReadBack::RootCount(*count),
                "{route:?} of {tree:?}"
            );
            format!("RootCount({count})")
        }
        (Ok(out), Err(encode::TreeKanError::NotATree(reason))) => {
            let ReadBack::NotATree(ours) = production_read_back(out, schema) else {
                panic!("{route:?} of {tree:?}: production reads no NotATree reason");
            };
            assert_eq!(
                graph_roots(&ref_graph(out)).len(),
                1,
                "{route:?} of {tree:?}"
            );
            format!(
                "NotATree({} / {})",
                variant_name(&format!("{reason:?}")),
                variant_name(&ours)
            )
        }
        (Err(ours), Err(encode::TreeKanError::Graph(theirs))) => {
            graph_errors_agree(ours, theirs, &(route, tree));
            "Graph".to_owned()
        }
        (ours, theirs) => {
            panic!("{route:?} of {tree:?}: production {ours:?}, reference {theirs:?}")
        }
    };
    if tree_domain {
        if let Ok(out) = &output {
            check_tree_square(functors, route, tree, &production_tree, out);
        }
    }
    outcome
}

// ---------------------------------------------------------------------------
// The bounded family, object by object.
// ---------------------------------------------------------------------------

/// A functor under test with the sides its graphs are encoded over.
struct FamilyCase {
    name: String,
    functors: Functors,
    source_side: encode::GraphSide,
    target_side: encode::GraphSide,
    /// Whether the functor is in the tree-Kan domain: injective on tables,
    /// relations and columns, every target table, relation and column in
    /// the image.
    tree_domain: bool,
}

impl FamilyCase {
    fn new(case: &scope::Case) -> Self {
        let functors = Functors::new(
            &case.source_schema(),
            &case.target_schema(),
            &case.migration(),
            &family_kinds(),
        );
        Self::from_functors(case.name.to_owned(), functors)
    }

    fn from_functors(name: String, functors: Functors) -> Self {
        let f = &functors.reference;
        let injective =
            |images: Vec<String>| images.len() == images.iter().collect::<BTreeSet<_>>().len();
        let table_images: Vec<String> = f.tables.values().cloned().collect();
        let column_images: Vec<String> = f
            .columns
            .iter()
            .map(|((table, _), column)| format!("{}.{column}", f.tables[table]))
            .collect();
        let relation_images: Vec<String> = f.relations.values().map(|e| format!("{e:?}")).collect();
        let onto = f
            .tgt
            .tables
            .iter()
            .all(|t| f.tables.values().any(|i| i == t))
            && f.tgt
                .relations
                .iter()
                .all(|r| f.relations.values().any(|i| i == r))
            && f.tgt.columns.iter().all(|(t, columns)| {
                columns.keys().all(|c| {
                    f.columns
                        .iter()
                        .any(|((s, _), image)| f.tables[s] == *t && image == c)
                })
            });
        let tree_domain = injective(table_images)
            && injective(column_images)
            && injective(relation_images)
            && onto;
        let source_side = encode::GraphSide::whole(&f.src);
        let target_side = encode::GraphSide::whole(&f.tgt);
        Self {
            name,
            functors,
            source_side,
            target_side,
            tree_domain,
        }
    }
}

/// The schema-taking graph Sigma at one graph, production against the
/// reference.
fn schema_taking_graph_sigma_agree(
    functors: &Functors,
    graph: &model::Graph,
    production_graph: &GInstance,
) {
    let ours = panproto_inst::g_sigma(
        production_graph,
        &functors.source_schema,
        &functors.protocol,
        &functors.target_schema,
        &functors.protocol,
        &functors.migration,
    );
    let theirs = encode::g_sigma(
        graph,
        &functors.reference_source,
        &functors.partition,
        &functors.reference_target,
        &functors.partition,
        &functors.reference_migration,
    );
    graph_results_agree(&ours, &theirs, &("schema-taking graph Sigma", graph));
}

/// The schema-taking graph Pi at one graph and product bound, production
/// against the reference.
fn schema_taking_graph_pi_agree(
    functors: &Functors,
    graph: &model::Graph,
    production_graph: &GInstance,
    bound: usize,
) {
    let ours = panproto_inst::g_pi(
        production_graph,
        &functors.source_schema,
        &functors.protocol,
        &functors.target_schema,
        &functors.protocol,
        &functors.migration,
        bound,
    );
    let theirs = encode::g_pi(
        graph,
        &functors.reference_source,
        &functors.partition,
        &functors.reference_target,
        &functors.partition,
        &functors.reference_migration,
        bound,
    );
    graph_results_agree(&ours, &theirs, &("schema-taking graph Pi", bound, graph));
}

/// The table, graph and tree Sigma and Pi of one source object.
fn check_source_object(family: &FamilyCase, input: &model::Table) -> Census {
    let fp = &family.functors.production;
    let production_input = prod_table(input);
    let mut census = Census::default();

    // The graph form of the input: decoding with the record-free layout.
    let graph =
        encode::decode_table(input, &family.source_side, &encode::GraphLayout::default()).unwrap();
    let production_graph = prod_graph(&graph);
    let decoded = panproto_inst::decode_graph(
        &production_input,
        EncodingSide::Whole(fp.source()),
        &GraphLayout::default(),
    )
    .unwrap();
    assert_eq!(ref_graph(&decoded), graph, "the decoded input");

    census += check_source_sigma(family, input, &production_input, &graph, &production_graph);
    for bound in scope::PRODUCT_BOUNDS {
        census += check_source_pi(
            family,
            input,
            &production_input,
            (&graph, &production_graph),
            bound,
        );
    }
    census += check_source_trees(family, input, &graph, &production_graph);
    census.add("source objects", 1);
    census
}

/// Table, graph and schema-taking Sigma of one source object and their
/// squares, production against the reference.
fn check_source_sigma(
    family: &FamilyCase,
    input: &model::Table,
    production_input: &FInstance,
    graph: &model::Graph,
    production_graph: &GInstance,
) -> Census {
    let f = &family.functors.reference;
    let fp = &family.functors.production;
    let mut census = Census::default();
    let table_sigma = rel::sigma(f, input);
    let production_sigma = panproto_inst::f_sigma_with_provenance(production_input, fp);
    let graph_sigma = encode::g_sigma_along_with_provenance(f, graph);
    let production_graph_sigma = panproto_inst::g_sigma_along_with_provenance(production_graph, fp);
    match (&table_sigma, &production_sigma) {
        (Ok((out, provenance)), Ok((ours, our_provenance))) => {
            assert_eq!(&ref_table(ours), out, "Sigma of {input:?}");
            assert_eq!(&ref_sigma_provenance(our_provenance), provenance);
            let (graph_out, graph_provenance) = graph_sigma.as_ref().unwrap();
            let (production_out, production_provenance) = production_graph_sigma.as_ref().unwrap();
            assert!(
                panproto_reference::migration::graph_square(&ref_graph(production_out), graph_out),
                "graph Sigma of {input:?}: production {:?}, reference {graph_out:?}",
                ref_graph(production_out)
            );
            schema_taking_graph_sigma_agree(&family.functors, graph, production_graph);
            assert_eq!(
                &ref_sigma_provenance(&production_provenance.table),
                &graph_provenance.table
            );
            assert_eq!(
                ref_contributors(&production_provenance.nodes, &production_provenance.edges),
                (
                    graph_provenance.nodes.clone(),
                    graph_provenance.edges.clone()
                )
            );
            let (square, _) =
                panproto_inst::encode_graph(production_out, EncodingSide::Whole(fp.target()))
                    .unwrap();
            assert!(panproto_reference::migration::table_square(
                out,
                &ref_table(&square)
            ));
            assert_eq!(
                encode::encode_graph(graph_out, &family.target_side)
                    .unwrap()
                    .0,
                ref_table(&square)
            );
            census.add("Sigma ok", 1);
        }
        (Err(error), Err(ours)) => {
            kan_refusals_agree(ours, error, &("Sigma", input));
            match (&graph_sigma, &production_graph_sigma) {
                (Err(theirs), Err(ours)) => graph_errors_agree(ours, theirs, &input),
                other => panic!("graph Sigma of {input:?}: {other:?}"),
            }
            census.add("Sigma refused", 1);
        }
        other => panic!("Sigma of {input:?}: {other:?}"),
    }
    census
}

/// Table, graph and schema-taking Pi of one source object at one product
/// bound and their squares, production against the reference.
fn check_source_pi(
    family: &FamilyCase,
    input: &model::Table,
    production_input: &FInstance,
    (graph, production_graph): (&model::Graph, &GInstance),
    bound: usize,
) -> Census {
    let f = &family.functors.reference;
    let fp = &family.functors.production;
    let mut census = Census::default();
    let table_pi = rel::pi(f, input, bound);
    let production_pi =
        panproto_inst::functor_pi_along_with_provenance(production_input, fp, bound);
    let graph_pi = encode::g_pi_along_with_provenance(f, graph, bound);
    let production_graph_pi =
        panproto_inst::g_pi_along_with_provenance(production_graph, fp, bound);
    match (&table_pi, &production_pi) {
        (Ok((out, provenance)), Ok((ours, our_provenance))) => {
            assert_eq!(&ref_table(ours), out, "Pi({bound}) of {input:?}");
            assert_eq!(&ref_pi_provenance(our_provenance), provenance);
            let (graph_out, graph_provenance) = graph_pi.as_ref().unwrap();
            let (production_out, production_provenance) = production_graph_pi.as_ref().unwrap();
            assert!(
                panproto_reference::migration::graph_square(&ref_graph(production_out), graph_out),
                "graph Pi of {input:?}: production {:?}, reference {graph_out:?}",
                ref_graph(production_out)
            );
            schema_taking_graph_pi_agree(&family.functors, graph, production_graph, bound);
            assert_eq!(
                &ref_pi_provenance(&production_provenance.table),
                &graph_provenance.table
            );
            assert_eq!(
                ref_contributors(&production_provenance.nodes, &production_provenance.edges),
                (
                    graph_provenance.nodes.clone(),
                    graph_provenance.edges.clone()
                )
            );
            let (square, _) =
                panproto_inst::encode_graph(production_out, EncodingSide::Whole(fp.target()))
                    .unwrap();
            assert!(panproto_reference::migration::table_square(
                out,
                &ref_table(&square)
            ));
            assert_eq!(
                encode::encode_graph(graph_out, &family.target_side)
                    .unwrap()
                    .0,
                ref_table(&square)
            );
            census.add(format!("Pi({bound}) ok"), 1);
        }
        (Err(error), Err(ours)) => {
            kan_refusals_agree(ours, error, &("Pi", bound, input));
            match (&graph_pi, &production_graph_pi) {
                (Err(theirs), Err(ours)) => graph_errors_agree(ours, theirs, &input),
                other => panic!("graph Pi of {input:?}: {other:?}"),
            }
            let kind = match error {
                rel::KanError::ProductSizeExceeded { .. } => "over-bound",
                rel::KanError::AttributeOutsideImage { .. } => "outside-image",
                other => panic!("Pi of {input:?}: {other:?}"),
            };
            census.add(format!("Pi({bound}) {kind}"), 1);
        }
        other => panic!("Pi({bound}) of {input:?}: {other:?}"),
    }
    census
}

/// The tree Sigma and Pi routes of one source object whose graph form is a
/// tree; nothing for one whose graph form is no tree on either side.
fn check_source_trees(
    family: &FamilyCase,
    input: &model::Table,
    graph: &model::Graph,
    production_graph: &GInstance,
) -> Census {
    let f = &family.functors.reference;
    let mut census = Census::default();
    let tree = input_tree(graph);
    let production_tree_shape = production_graph.tree_shape(&family.functors.source_schema);
    match (&tree, &production_tree_shape) {
        (Some(tree), TreeShapeReport::Tree { root }) => {
            assert_eq!(*root, tree.root);
            assert_eq!(ref_tree(&production_graph.to_wtype(*root).unwrap()), *tree);
            let routes = [
                (TreeRoute::Sigma, encode::t_sigma(f, tree)),
                (
                    TreeRoute::Pi(scope::PRODUCT_BOUNDS[0]),
                    encode::t_pi(f, tree, scope::PRODUCT_BOUNDS[0]),
                ),
                (
                    TreeRoute::Pi(scope::PRODUCT_BOUNDS[1]),
                    encode::t_pi(f, tree, scope::PRODUCT_BOUNDS[1]),
                ),
            ];
            for (route, expected) in routes {
                let outcome =
                    check_tree_route(&family.functors, route, tree, &expected, family.tree_domain);
                census.add(format!("tree {route:?} {outcome}"), 1);
            }
            census.add("source trees", 1);
        }
        (None, shape) => {
            assert!(
                !matches!(shape, TreeShapeReport::Tree { .. }),
                "production reads a tree the reference does not: {input:?}"
            );
            census.add(
                input_refusal_agrees(graph, production_graph, &family.functors.source_schema),
                1,
            );
        }
        (Some(_), shape) => panic!("the reference reads a tree, production {shape:?}: {input:?}"),
    }
    census
}

/// The table, graph and tree Delta of one target object.
fn check_target_object(family: &FamilyCase, input: &model::Table) -> Census {
    let f = &family.functors.reference;
    let fp = &family.functors.production;
    let production_input = prod_table(input);
    let mut census = Census::default();
    let graph =
        encode::decode_table(input, &family.target_side, &encode::GraphLayout::default()).unwrap();
    let production_graph = prod_graph(&graph);
    let decoded = panproto_inst::decode_graph(
        &production_input,
        EncodingSide::Whole(fp.target()),
        &GraphLayout::default(),
    )
    .unwrap();
    assert_eq!(ref_graph(&decoded), graph, "the decoded input");

    let (out, provenance) = rel::delta(f, input).unwrap();
    let (ours, our_provenance) =
        panproto_inst::f_delta_with_provenance(&production_input, fp).unwrap();
    assert_eq!(ref_table(&ours), out, "Delta of {input:?}");
    assert_eq!(ref_delta_provenance(&our_provenance), provenance);
    let (graph_out, graph_provenance) = encode::g_delta_along_with_provenance(f, &graph).unwrap();
    let (production_out, production_provenance) =
        panproto_inst::g_delta_along_with_provenance(&production_graph, fp).unwrap();
    assert!(
        panproto_reference::migration::graph_square(&ref_graph(&production_out), &graph_out),
        "graph Delta of {input:?}"
    );
    graph_results_agree(
        &panproto_inst::g_delta(
            &production_graph,
            &family.functors.source_schema,
            &family.functors.protocol,
            &family.functors.target_schema,
            &family.functors.protocol,
            &family.functors.migration,
        ),
        &encode::g_delta(
            &graph,
            &family.functors.reference_source,
            &family.functors.partition,
            &family.functors.reference_target,
            &family.functors.partition,
            &family.functors.reference_migration,
        ),
        &("schema-taking Delta", input),
    );
    assert_eq!(
        ref_delta_provenance(&production_provenance.table),
        graph_provenance.table
    );
    assert_eq!(
        ref_contributors(&production_provenance.nodes, &production_provenance.edges),
        (graph_provenance.nodes, graph_provenance.edges)
    );
    let (square, _) =
        panproto_inst::encode_graph(&production_out, EncodingSide::Domain(fp)).unwrap();
    assert!(panproto_reference::migration::table_square(
        &out,
        &ref_table(&square)
    ));
    assert_eq!(
        encode::encode_graph(&graph_out, &encode::GraphSide::kept(f))
            .unwrap()
            .0,
        ref_table(&square)
    );
    census.add("Delta ok", 1);

    let tree = input_tree(&graph);
    let production_tree_shape = production_graph.tree_shape(&family.functors.target_schema);
    match (&tree, &production_tree_shape) {
        (Some(tree), TreeShapeReport::Tree { root }) => {
            assert_eq!(*root, tree.root);
            assert_eq!(ref_tree(&production_graph.to_wtype(*root).unwrap()), *tree);
            let expected = encode::t_delta(f, tree);
            let outcome = check_tree_route(
                &family.functors,
                TreeRoute::Delta,
                tree,
                &expected,
                family.tree_domain,
            );
            census.add(format!("tree Delta {outcome}"), 1);
            census.add("target trees", 1);
        }
        (None, shape) => {
            assert!(
                !matches!(shape, TreeShapeReport::Tree { .. }),
                "production reads a tree the reference does not: {input:?}"
            );
            census.add(
                input_refusal_agrees(&graph, &production_graph, &family.functors.target_schema),
                1,
            );
        }
        (Some(_), shape) => panic!("the reference reads a tree, production {shape:?}: {input:?}"),
    }
    census.add("target objects", 1);
    census
}

/// Sigma, Delta and Pi on every labelled object of the bounded family (at
/// most three rows per table and six in all, both product bounds), as
/// tables, as graphs (encode, operation, decode) and as trees where the
/// graph form is a tree, against the reference: outputs, provenance, typed
/// refusals with their payloads, and every graph, table and tree
/// conjugation square on its domain.
#[test]
fn bounded_family_kan_objects_agree_with_the_reference() {
    let independent = scope::case_object_counts(3, 6);
    let mut total = Census::default();
    for (index, case) in scope::family().into_iter().enumerate() {
        let family = FamilyCase::new(&case);
        let sources = scope::instances(&case.source, 3, 6, &case.cells);
        let targets = scope::instances(&case.target, 3, 6, &case.cells);
        assert_eq!(
            (sources.len() as u64, targets.len() as u64),
            independent[index],
            "{}",
            case.name
        );
        let mut census = par_sum(&sources, |input| check_source_object(&family, input));
        census += par_sum(&targets, |input| check_target_object(&family, input));
        println!(
            "case {index} {}: {} source and {} target objects; {:?}",
            family.name,
            sources.len(),
            targets.len(),
            census.0
        );
        total += census;
    }
    let objects = total.get("source objects") + total.get("target objects");
    println!(
        "bounded family: at most 3 rows per table and 6 in all, product bounds {:?}; \
         {objects} labelled objects (independent count {}); {:?}",
        scope::PRODUCT_BOUNDS,
        scope::objects_count(3, 6),
        total.0
    );
    assert_eq!(objects, 248_552);
    assert_eq!(objects, scope::objects_count(3, 6));
    assert_eq!(total.get("Sigma ok"), 127_818);
    assert_eq!(total.get("Sigma refused"), 6_386);
    assert_eq!(total.get("Delta ok"), 114_348);
    assert_eq!(total.get("Pi(4) ok"), 83_943);
    assert_eq!(total.get("Pi(81) ok"), 134_119);
    assert_eq!(total.get("Pi(4) over-bound"), 50_176);
    assert_eq!(total.get("Pi(81) over-bound"), 0);
    assert_eq!(total.get("Pi(4) outside-image"), 85);
    assert_eq!(total.get("Pi(81) outside-image"), 85);
    assert_eq!(
        (total.get("source trees"), total.get("target trees")),
        (706, 1_294)
    );
}

// ---------------------------------------------------------------------------
// The hom-profile pairs: both hom-set bijections on every representative
// pair of the bounded family.
// ---------------------------------------------------------------------------

/// A homomorphism as the key `same_as` compares on `domain`: its row map and
/// the canonical bytes of the image of every labelled null of the domain.
type HomKey = (Vec<(u32, u32)>, Vec<(u64, Vec<u8>)>);

fn hom_key(hom: &rel::Hom, domain: &model::Table) -> HomKey {
    let used: BTreeSet<u64> = domain
        .rows
        .values()
        .flat_map(|(_, cells)| cells.values())
        .filter_map(|value| match value {
            rvalue::Value::LabeledNull(id) => Some(*id),
            _ => None,
        })
        .collect();
    let nulls = used
        .into_iter()
        .map(|id| {
            let image = hom
                .nulls
                .get(&id)
                .cloned()
                .unwrap_or(rvalue::Value::LabeledNull(id));
            (
                id,
                panproto_reference::canonical::value_bytes(&image).unwrap(),
            )
        })
        .collect();
    (hom.rows.iter().map(|(a, b)| (*a, *b)).collect(), nulls)
}

/// `first` then `second`, as a reference homomorphism.
fn then(first: &FInstanceHom, second: &FInstanceHom) -> rel::Hom {
    ref_hom(&first.compose(second).unwrap())
}

/// A source representative with its production instance and every image
/// of it the pair checks read, production and reference.
struct PairSource {
    input: model::Table,
    production: FInstance,
    weight: u64,
    sigma: Result<(model::Table, rel::SigmaProvenance), rel::KanError>,
    production_sigma: Option<FInstance>,
    production_delta_sigma: Option<FInstance>,
    production_unit: Option<FInstanceHom>,
    pi: [Result<(model::Table, rel::PiProvenance), rel::KanError>; 2],
    production_pi: [Option<FInstance>; 2],
    production_pi_counit: [Option<FInstanceHom>; 2],
}

/// A target representative with its production instance and images.
struct PairTarget {
    target: model::Table,
    production: FInstance,
    weight: u64,
    delta: model::Table,
    production_delta: FInstance,
    production_counit: Option<FInstanceHom>,
    production_pi_delta: [Option<FInstance>; 2],
    production_pi_unit: [Option<FInstanceHom>; 2],
}

/// Build a source representative, checking the production unit, Pi counits
/// and Kan images against the reference ones.
fn pair_source(functors: &Functors, input: model::Table, weight: u64) -> PairSource {
    let (f, fp) = (&functors.reference, &functors.production);
    let production = prod_table(&input);
    let sigma = rel::sigma(f, &input);
    let production_sigma = panproto_inst::f_sigma(&production, fp).ok();
    assert_eq!(sigma.is_ok(), production_sigma.is_some());
    let production_delta_sigma = production_sigma
        .as_ref()
        .map(|s| panproto_inst::f_delta(s, fp).unwrap());
    let unit = rel::sigma_unit(f, &input);
    let production_unit_result = panproto_inst::f_unit(&production, fp);
    hom_results_agree(
        &production_unit_result,
        &unit,
        &input,
        &("Sigma unit", &input),
    );
    let pi = scope::PRODUCT_BOUNDS.map(|bound| rel::pi(f, &input, bound));
    let production_pi = scope::PRODUCT_BOUNDS
        .map(|bound| panproto_inst::functor_pi_along(&production, fp, bound).ok());
    let production_pi_counit = scope::PRODUCT_BOUNDS.map(|bound| {
        let ours = panproto_inst::f_pi_counit(&production, fp, bound);
        let theirs = rel::pi_counit(f, &input, bound);
        if let (Ok(_), Ok(_)) | (Err(_), Err(_)) = (&ours, &theirs) {
            let domain = rel::pi(f, &input, bound)
                .ok()
                .and_then(|(pi, _)| rel::delta(f, &pi).ok())
                .map(|(delta, _)| delta)
                .unwrap_or_default();
            hom_results_agree(&ours, &theirs, &domain, &("Pi counit", bound, &input));
        } else {
            panic!("Pi counit at {bound} of {input:?}: {ours:?} / {theirs:?}");
        }
        ours.ok()
    });
    PairSource {
        input,
        production,
        weight,
        sigma,
        production_sigma,
        production_delta_sigma,
        production_unit: production_unit_result.ok(),
        pi,
        production_pi,
        production_pi_counit,
    }
}

/// Build a target representative, checking the production counit, Pi units
/// and Kan images against the reference ones.
fn pair_target(functors: &Functors, target: model::Table, weight: u64) -> PairTarget {
    let (f, fp) = (&functors.reference, &functors.production);
    let production = prod_table(&target);
    let (delta, _) = rel::delta(f, &target).unwrap();
    let production_delta = panproto_inst::f_delta(&production, fp).unwrap();
    assert_eq!(ref_table(&production_delta), delta);
    let counit = rel::sigma_counit(f, &target);
    let production_counit_result = panproto_inst::f_counit(&production, fp);
    let counit_domain = rel::sigma_restricted(f, &delta)
        .map(|(s, _)| s)
        .unwrap_or_default();
    hom_results_agree(
        &production_counit_result,
        &counit,
        &counit_domain,
        &("Sigma counit", &target),
    );
    let production_pi_delta = scope::PRODUCT_BOUNDS.map(|bound| {
        panproto_inst::functor_pi_restricted(&production_delta, fp, bound)
            .ok()
            .map(|(p, _)| p)
    });
    let production_pi_unit = scope::PRODUCT_BOUNDS.map(|bound| {
        let ours = panproto_inst::f_pi_unit(&production, fp, bound);
        let theirs = rel::pi_unit(f, &target, bound);
        hom_results_agree(&ours, &theirs, &target, &("Pi unit", bound, &target));
        ours.ok()
    });
    PairTarget {
        target,
        production,
        weight,
        delta,
        production_delta,
        production_counit: production_counit_result.ok(),
        production_pi_delta,
        production_pi_unit,
    }
}

/// Production's units and counits of both adjunctions, at both product
/// bounds for Pi, on every labelled object of the bounded family, equal to
/// the reference's exactly (refusals by their typed reason): the object-level
/// half of the profile quotient, executed literally on all 248,552 objects
/// rather than on profile representatives only.
#[test]
fn every_labelled_object_has_the_reference_units_and_counits() {
    let independent = scope::case_object_counts(3, 6);
    let mut objects = 0u64;
    for (index, case) in scope::family().into_iter().enumerate() {
        let functors = Functors::new(
            &case.source_schema(),
            &case.target_schema(),
            &case.migration(),
            &family_kinds(),
        );
        let sources = scope::instances(&case.source, 3, 6, &case.cells);
        let targets = scope::instances(&case.target, 3, 6, &case.cells);
        assert_eq!(
            (sources.len() as u64, targets.len() as u64),
            independent[index],
            "{}",
            case.name
        );
        objects += par_sum(&sources, |input| {
            pair_source(&functors, input.clone(), 1);
            1u64
        });
        objects += par_sum(&targets, |target| {
            pair_target(&functors, target.clone(), 1);
            1u64
        });
    }
    println!(
        "units and counits: {objects} labelled objects (independent count {})",
        scope::objects_count(3, 6)
    );
    assert_eq!(objects, 248_552);
    assert_eq!(objects, scope::objects_count(3, 6));
}

/// One case of the bounded family with its representatives.
struct PairCase {
    name: &'static str,
    functors: Functors,
    sources: Vec<PairSource>,
    targets: Vec<PairTarget>,
}

/// The counters of the representative-pair run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PairStats {
    pairs: u64,
    weighted: u64,
    sigma_homs: u64,
    sigma_refusals: u64,
    pi_homs: [u64; 2],
    pi_naturality_refusals: u64,
    pi_outside_image: u64,
    pi_over_bound: u64,
}

impl std::ops::AddAssign for PairStats {
    fn add_assign(&mut self, other: Self) {
        let Self {
            pairs,
            weighted,
            sigma_homs,
            sigma_refusals,
            pi_homs,
            pi_naturality_refusals,
            pi_outside_image,
            pi_over_bound,
        } = other;
        self.pairs += pairs;
        self.weighted += weighted;
        self.sigma_homs += sigma_homs;
        self.sigma_refusals += sigma_refusals;
        self.pi_homs[0] += pi_homs[0];
        self.pi_homs[1] += pi_homs[1];
        self.pi_naturality_refusals += pi_naturality_refusals;
        self.pi_outside_image += pi_outside_image;
        self.pi_over_bound += pi_over_bound;
    }
}

/// The Sigma -| Delta bijection at one pair: every homomorphism of
/// `Hom(Sigma I, J)` and of `Hom(I, Delta J)` transposed by production both
/// ways, each image equal to the reference transpose and a member of the
/// other hom-set, each round trip the identity, and each image equal to its
/// component formula (`eta_I ; Delta(h)` and `Sigma(k) ; eps_J`) taken by
/// production, which is two-variable naturality.
fn check_sigma_pair(
    functors: &Functors,
    source: &PairSource,
    sigma: &model::Table,
    target: &PairTarget,
) -> u64 {
    let (f, fp) = (&functors.reference, &functors.production);
    let input = &source.input;
    let left = rel::homs(sigma, &target.target);
    let right = rel::homs(input, &target.delta);
    assert_eq!(
        left.len(),
        right.len(),
        "|Hom(Sigma I, J)| = |Hom(I, Delta J)|"
    );
    let right_keys: BTreeSet<HomKey> = right.iter().map(|h| hom_key(h, input)).collect();
    let left_keys: BTreeSet<HomKey> = left.iter().map(|h| hom_key(h, sigma)).collect();
    let production_sigma = source.production_sigma.as_ref().unwrap();
    let unit = source.production_unit.as_ref().unwrap();
    let counit = target.production_counit.as_ref().unwrap();
    for hom in &left {
        let production_hom = prod_hom(hom);
        let image = panproto_inst::f_transpose_left(
            &production_hom,
            &source.production,
            &target.production,
            fp,
        )
        .unwrap();
        let reference = rel::sigma_transpose_forward(f, input, &target.target, hom).unwrap();
        assert!(
            ref_hom(&image).same_as(&reference, input),
            "the forward Sigma transpose"
        );
        assert!(right_keys.contains(&hom_key(&ref_hom(&image), input)));
        let back =
            panproto_inst::f_transpose_right(&image, &source.production, &target.production, fp)
                .unwrap();
        assert!(ref_hom(&back).same_as(hom, sigma), "the Sigma round trip");
        let delta_h = panproto_inst::f_delta_on_hom(
            &production_hom,
            production_sigma,
            &target.production,
            fp,
        )
        .unwrap();
        let reference_delta_h = rel::delta_on_hom(f, sigma, &target.target, hom).unwrap();
        let delta_domain = source
            .production_delta_sigma
            .as_ref()
            .map(ref_table)
            .unwrap();
        assert!(ref_hom(&delta_h).same_as(&reference_delta_h, &delta_domain));
        assert!(
            then(unit, &delta_h).same_as(&ref_hom(&image), input),
            "eta ; Delta(h)"
        );
    }
    for hom in &right {
        let production_hom = prod_hom(hom);
        let image = panproto_inst::f_transpose_right(
            &production_hom,
            &source.production,
            &target.production,
            fp,
        )
        .unwrap();
        let reference = rel::sigma_transpose_inverse(f, input, &target.target, hom).unwrap();
        assert!(
            ref_hom(&image).same_as(&reference, sigma),
            "the inverse Sigma transpose"
        );
        assert!(left_keys.contains(&hom_key(&ref_hom(&image), sigma)));
        let back =
            panproto_inst::f_transpose_left(&image, &source.production, &target.production, fp)
                .unwrap();
        assert!(
            ref_hom(&back).same_as(hom, input),
            "the inverse Sigma round trip"
        );
        let sigma_k = panproto_inst::f_sigma_on_hom(
            &production_hom,
            &source.production,
            &target.production_delta,
            fp,
        )
        .unwrap();
        let reference_sigma_k = rel::sigma_on_hom(f, input, &target.delta, hom).unwrap();
        assert!(ref_hom(&sigma_k).same_as(&reference_sigma_k, sigma));
        assert!(
            then(&sigma_k, counit).same_as(&ref_hom(&image), sigma),
            "Sigma(k) ; eps"
        );
    }
    u64::try_from(left.len()).unwrap()
}

/// The Delta -| Pi bijection at one pair and one bound, as
/// [`check_sigma_pair`] states it for `Hom(Delta J, I)` and `Hom(J, Pi I)`,
/// the component formulas `unit_J ; Pi(h)` and `Delta(k) ; counit_I`. Where
/// the unit's codomain `Pi(Delta J)` is over the operational bound, the
/// transposes and their round trips remain defined from `Pi(I)`; only the
/// explicit unit naturality formula is unavailable at that bound. Returns
/// the number of homomorphisms and of unavailable unit formulas.
fn check_pi_pair(
    functors: &Functors,
    target: &PairTarget,
    source: &PairSource,
    pi: &model::Table,
    slot: usize,
) -> (u64, u64) {
    let (f, fp) = (&functors.reference, &functors.production);
    let bound = scope::PRODUCT_BOUNDS[slot];
    let input = &source.input;
    let left = rel::homs(&target.delta, input);
    let right = rel::homs(&target.target, pi);
    assert_eq!(
        left.len(),
        right.len(),
        "|Hom(Delta J, I)| = |Hom(J, Pi I)|"
    );
    let right_keys: BTreeSet<HomKey> = right.iter().map(|h| hom_key(h, &target.target)).collect();
    let left_keys: BTreeSet<HomKey> = left.iter().map(|h| hom_key(h, &target.delta)).collect();
    let unit = target.production_pi_unit[slot].as_ref();
    let mut refusals = 0u64;
    for hom in &left {
        let production_hom = prod_hom(hom);
        let image = panproto_inst::f_pi_transpose_left(
            &production_hom,
            &target.production,
            &source.production,
            fp,
            bound,
        );
        let reference = rel::pi_transpose_forward(f, &target.target, input, hom, bound);
        let (image, reference) = (image.unwrap(), reference.unwrap());
        assert!(
            ref_hom(&image).same_as(&reference, &target.target),
            "the forward Pi transpose"
        );
        assert!(right_keys.contains(&hom_key(&ref_hom(&image), &target.target)));
        let back = panproto_inst::f_pi_transpose_right(
            &image,
            &target.production,
            &source.production,
            fp,
            bound,
        )
        .unwrap();
        assert!(
            ref_hom(&back).same_as(hom, &target.delta),
            "the Pi round trip"
        );
        if let (Some(unit), Some(production_pi_delta)) =
            (unit, target.production_pi_delta[slot].as_ref())
        {
            let pi_h = panproto_inst::f_pi_on_hom(
                &production_hom,
                &target.production_delta,
                &source.production,
                fp,
                bound,
            )
            .unwrap();
            let reference_pi_h = rel::pi_on_hom(f, &target.delta, input, hom, bound).unwrap();
            assert!(ref_hom(&pi_h).same_as(&reference_pi_h, &ref_table(production_pi_delta)));
            assert!(
                then(unit, &pi_h).same_as(&ref_hom(&image), &target.target),
                "unit ; Pi(h)"
            );
        } else {
            assert!(unit.is_none() && target.production_pi_delta[slot].is_none());
            refusals += 1;
        }
    }
    check_pi_inverse_transposes(functors, (target, source), pi, slot, (&left_keys, &right));
    (u64::try_from(left.len()).unwrap(), refusals)
}

/// The inverse half of [`check_pi_pair`]: every `k` in `Hom(J, Pi I)`
/// transposes to a homomorphism of `Hom(Delta J, I)` as the reference's,
/// back to `k`, and as `Delta(k) ; counit_I`.
fn check_pi_inverse_transposes(
    functors: &Functors,
    (target, source): (&PairTarget, &PairSource),
    pi: &model::Table,
    slot: usize,
    (left_keys, right): (&BTreeSet<HomKey>, &[rel::Hom]),
) {
    let (f, fp) = (&functors.reference, &functors.production);
    let bound = scope::PRODUCT_BOUNDS[slot];
    let input = &source.input;
    let counit = source.production_pi_counit[slot].as_ref().unwrap();
    let production_pi = source.production_pi[slot].as_ref().unwrap();
    for hom in right {
        let production_hom = prod_hom(hom);
        let image = panproto_inst::f_pi_transpose_right(
            &production_hom,
            &target.production,
            &source.production,
            fp,
            bound,
        )
        .unwrap();
        let reference = rel::pi_transpose_inverse(f, &target.target, input, hom, bound).unwrap();
        assert!(
            ref_hom(&image).same_as(&reference, &target.delta),
            "the inverse Pi transpose"
        );
        assert!(left_keys.contains(&hom_key(&ref_hom(&image), &target.delta)));
        let back = panproto_inst::f_pi_transpose_left(
            &image,
            &target.production,
            &source.production,
            fp,
            bound,
        )
        .unwrap();
        assert!(
            ref_hom(&back).same_as(hom, &target.target),
            "the inverse Pi round trip"
        );
        let delta_k =
            panproto_inst::f_delta_on_hom(&production_hom, &target.production, production_pi, fp)
                .unwrap();
        let reference_delta_k = rel::delta_on_hom(f, &target.target, pi, hom).unwrap();
        assert!(ref_hom(&delta_k).same_as(&reference_delta_k, &target.delta));
        assert!(
            then(&delta_k, counit).same_as(&ref_hom(&image), &target.delta),
            "Delta(k) ; counit"
        );
    }
}

/// Both adjunctions at one representative pair.
fn check_pair(case: &PairCase, source: &PairSource, target: &PairTarget) -> PairStats {
    let mut stats = PairStats {
        pairs: 1,
        weighted: source.weight * target.weight,
        ..PairStats::default()
    };
    match &source.sigma {
        Ok((sigma, _)) => {
            stats.sigma_homs += check_sigma_pair(&case.functors, source, sigma, target);
        }
        Err(rel::KanError::SigmaConstantClash { .. }) => {
            assert!(rel::homs(&source.input, &target.delta).is_empty());
            stats.sigma_refusals += 1;
        }
        Err(other) => panic!("{}: Sigma refused with {other:?}", case.name),
    }
    for slot in 0..2 {
        match &source.pi[slot] {
            Ok((pi, _)) => {
                let (homs, refusals) = check_pi_pair(&case.functors, target, source, pi, slot);
                stats.pi_homs[slot] += homs;
                stats.pi_naturality_refusals += refusals;
            }
            Err(rel::KanError::AttributeOutsideImage { .. }) => stats.pi_outside_image += 1,
            Err(rel::KanError::ProductSizeExceeded { .. }) => stats.pi_over_bound += 1,
            Err(other) => panic!("{}: Pi refused with {other:?}", case.name),
        }
    }
    stats
}

/// Every case of the bounded family with its hom-profile representatives,
/// each weighted by its class's multiplicity as the independent
/// orbit-stabilizer certificate computes it.
fn pair_cases() -> Vec<PairCase> {
    let classes = scope::hom_profile_classes(3, 6).unwrap();
    let mut cases = Vec::new();
    for (index, case) in scope::family().into_iter().enumerate() {
        let functors = Functors::new(
            &case.source_schema(),
            &case.target_schema(),
            &case.migration(),
            &family_kinds(),
        );
        let mut sources = Vec::new();
        let mut targets = Vec::new();
        for (profile, class) in &classes {
            if profile.case != index {
                continue;
            }
            let certificate = scope::profile_multiplicity_certificate(
                profile.case,
                profile.side,
                &class.representative,
            )
            .unwrap();
            assert_eq!(certificate.multiplicity, class.multiplicity);
            match profile.side {
                scope::CaseSide::Source => sources.push(pair_source(
                    &functors,
                    class.representative.clone(),
                    certificate.multiplicity,
                )),
                scope::CaseSide::Target => targets.push(pair_target(
                    &functors,
                    class.representative.clone(),
                    certificate.multiplicity,
                )),
            }
        }
        cases.push(PairCase {
            name: case.name,
            functors,
            sources,
            targets,
        });
    }
    cases
}

/// The first `limit` representative pairs in case, source, target order,
/// one unit per source representative.
fn pair_units(cases: &[PairCase], limit: u64) -> Vec<(usize, usize, usize)> {
    let mut units = Vec::new();
    let mut remaining = limit;
    for (index, case) in cases.iter().enumerate() {
        for source in 0..case.sources.len() {
            let targets = usize::try_from(remaining)
                .unwrap_or(usize::MAX)
                .min(case.targets.len());
            if targets == 0 {
                continue;
            }
            units.push((index, source, targets));
            remaining -= u64::try_from(targets).unwrap();
        }
    }
    units
}

/// Execute the first `limit` representative pairs on [`threads`] workers,
/// worker `w` taking units `w, w + n, ...`, printing progress every 250,000
/// pairs.
fn run_pairs(cases: &[PairCase], limit: u64) -> PairStats {
    let units = pair_units(cases, limit);
    let workers = threads().clamp(1, units.len().max(1));
    let started = std::time::Instant::now();
    let done = AtomicU64::new(0);
    let stop = AtomicBool::new(false);
    let run_share = |share: usize| {
        let mut stats = PairStats::default();
        for &(case, source, targets) in units.iter().skip(share).step_by(workers) {
            let case = &cases[case];
            let source = &case.sources[source];
            for target in &case.targets[..targets] {
                if stop.load(Ordering::Relaxed) {
                    return stats;
                }
                let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    check_pair(case, source, target)
                }));
                match checked {
                    Ok(pair) => stats += pair,
                    Err(panic) => {
                        stop.store(true, Ordering::Relaxed);
                        eprintln!(
                            "{}: the pair {:?} / {:?} disagrees",
                            case.name, source.input, target.target
                        );
                        std::panic::resume_unwind(panic);
                    }
                }
                let pairs = done.fetch_add(1, Ordering::Relaxed) + 1;
                if pairs % 250_000 == 0 {
                    eprintln!(
                        "representative pairs: {pairs} after {:.1}s",
                        started.elapsed().as_secs_f64()
                    );
                }
            }
        }
        stats
    };
    let stats = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|share| {
                let run_share = &run_share;
                scope.spawn(move || run_share(share))
            })
            .collect();
        let mut stats = PairStats::default();
        let mut first_panic = None;
        for handle in handles {
            match handle.join() {
                Ok(share) => stats += share,
                Err(panic) => {
                    first_panic.get_or_insert(panic);
                }
            }
        }
        if let Some(panic) = first_panic {
            std::panic::resume_unwind(panic);
        }
        stats
    });
    println!(
        "representative pairs: workers={workers}, {stats:?}, elapsed {:.1}s",
        started.elapsed().as_secs_f64()
    );
    stats
}

/// The representative profile counts of the bounded family, case by case,
/// as `(source, target)`.
const PROFILE_COUNTS: [(u64, u64); 10] = [
    (697, 517),
    (697, 24),
    (24, 24),
    (24, 517),
    (24, 96),
    (1_507, 3_367),
    (3_367, 3_367),
    (517, 24),
    (969, 35),
    (1_507, 1_507),
];

/// The labelled object count of the bounded family.
fn labelled_objects() -> u64 {
    scope::objects_count(3, 6)
}

/// Every labelled object of the bounded family is isomorphic to its
/// profile representative: the reference finds a structure-preserving
/// bijection, production accepts it as an isomorphism, and its inverse
/// both ways. Returns the number of objects.
fn profile_members_are_isomorphic() -> u64 {
    let objects = scope::objects(3, 6);
    par_sum(&objects, |object| {
        let (_, relabeling) = scope::hom_profile(object).unwrap();
        let representative = relabeling.apply(&object.table).unwrap();
        let forth = rel::row_bijection(&object.table, &representative).unwrap();
        let back = rel::row_bijection(&representative, &object.table).unwrap();
        let (production, production_representative) =
            (prod_table(&object.table), prod_table(&representative));
        assert!(prod_hom(&forth).is_isomorphism(&production, &production_representative));
        assert!(prod_hom(&back).is_isomorphism(&production_representative, &production));
        1u64
    })
}

/// Both hom-set bijections of both adjunctions (forward and inverse
/// transposes, both identity composites and naturality through the
/// component formulas) on every ordered pair of hom-profile
/// representatives of the bounded family, at both product bounds, against
/// the reference. Each pair stands for its two classes; the class sizes come
/// from the orbit-stabilizer certificate and their weighted sum must be the
/// labelled pair count.
#[test]
fn o14_hom_profile_pairs_agree_with_the_reference() {
    let limit = std::env::var("PANPROTO_KAN_PAIR_LIMIT")
        .map_or(u64::MAX, |text| text.parse().expect("a pair limit"));
    let cases = pair_cases();
    let mut profiles = 0u64;
    let mut representative_pairs = 0u64;
    for (case, expected) in cases.iter().zip(PROFILE_COUNTS) {
        let counts = (case.sources.len() as u64, case.targets.len() as u64);
        let sizes = |weights: Vec<u64>| -> String {
            let mut census: BTreeMap<u64, u64> = BTreeMap::new();
            for weight in weights {
                *census.entry(weight).or_default() += 1;
            }
            format!("{census:?}")
        };
        println!(
            "{}: {} source and {} target profiles; source class sizes {}; target class sizes {}",
            case.name,
            counts.0,
            counts.1,
            sizes(case.sources.iter().map(|s| s.weight).collect()),
            sizes(case.targets.iter().map(|t| t.weight).collect()),
        );
        assert_eq!(counts, expected, "{}", case.name);
        profiles += counts.0 + counts.1;
        representative_pairs += counts.0 * counts.1;
    }
    let labelled_pairs = scope::object_pairs_count(3, 6);
    println!(
        "hom profiles: {profiles} profile occurrences, {representative_pairs} representative \
         pairs, {labelled_pairs} labelled ordered pairs (independent count)"
    );
    assert_eq!(profiles, 18_811);
    assert_eq!(representative_pairs, 19_120_495);
    assert_eq!(labelled_pairs, 3_918_714_467);
    let members = profile_members_are_isomorphic();
    println!("profile members: {members} labelled objects, each isomorphic to its representative");
    assert_eq!(members, labelled_objects());
    let stats = run_pairs(&cases, limit);
    if limit >= representative_pairs {
        assert_eq!(stats.pairs, representative_pairs);
        assert_eq!(stats.weighted, labelled_pairs);
        println!(
            "weighted total {} = labelled ordered pairs {labelled_pairs}",
            stats.weighted
        );
    }
    assert!(stats.sigma_homs > 0 && stats.pi_homs[0] > 0 && stats.pi_homs[1] > 0);
}

// ---------------------------------------------------------------------------
// O7's graph scopes: the encoding, its refusals and homomorphisms.
// ---------------------------------------------------------------------------

/// A graph spec's reference and production relation-ified schemas.
fn spec_rel(spec: &scope::GraphSpec) -> (rel::RelSchema, RelSchema) {
    let (value, non_value) = graph_spec_kinds();
    let reference = rel::classify(&spec.schema(), &value, &non_value).unwrap();
    let production = RelSchema::new(
        &prod_schema(&spec.schema()),
        &protocol_of(&value, &non_value),
    )
    .unwrap();
    (reference, production)
}

/// Encode one graph on both sides and require equal tables, layouts and
/// refusals; an accepted graph also decodes back identically on both sides.
/// Returns the reference result.
fn encodings_agree(
    graph: &model::Graph,
    side: &encode::GraphSide,
    production_side: &RelSchema,
) -> Result<model::Table, encode::NotEncodable> {
    let production_graph = prod_graph(graph);
    let ours = panproto_inst::encode_graph(&production_graph, EncodingSide::Whole(production_side));
    let theirs = encode::encode_graph(graph, side);
    match (&ours, &theirs) {
        (Ok((table, layout)), Ok((expected, expected_layout))) => {
            assert_eq!(&ref_table(table), expected, "the encoding of {graph:?}");
            assert_eq!(
                &ref_layout(layout),
                expected_layout,
                "the layout of {graph:?}"
            );
            let decoded =
                panproto_inst::decode_graph(table, EncodingSide::Whole(production_side), layout)
                    .unwrap();
            assert!(
                GInstance::identical(&decoded, &production_graph),
                "{graph:?}"
            );
            let reference_decoded = encode::decode_table(expected, side, expected_layout).unwrap();
            assert_eq!(&reference_decoded, graph);
            Ok(expected.clone())
        }
        (Err(GraphKanError::NotEncodable { reason }), Err(expected)) => {
            assert_eq!(
                ref_not_encodable(reason).as_ref(),
                Some(expected),
                "{graph:?}"
            );
            Err(expected.clone())
        }
        (ours, theirs) => panic!("encoding {graph:?}: production {ours:?}, reference {theirs:?}"),
    }
}

/// The census name of a refusal of the encoding.
const fn not_encodable_name(reason: &encode::NotEncodable) -> &'static str {
    match reason {
        encode::NotEncodable::FanNode { .. } => "FanNode",
        encode::NotEncodable::AnchorOutsideSide { .. } => "AnchorOutsideSide",
        encode::NotEncodable::DanglingEdge { .. } => "DanglingEdge",
        encode::NotEncodable::EdgeOutsideSide { .. } => "EdgeOutsideSide",
        encode::NotEncodable::IncoherentEndpoints { .. } => "IncoherentEndpoints",
        encode::NotEncodable::ParallelEdges { .. } => "ParallelEdges",
        encode::NotEncodable::MissingCell { .. } => "MissingCell",
        encode::NotEncodable::CellAsExtraField { .. } => "CellAsExtraField",
        encode::NotEncodable::RepeatedCell { .. } => "RepeatedCell",
        encode::NotEncodable::SharedValue { .. } => "SharedValue",
        encode::NotEncodable::UnrootedValue { .. } => "UnrootedValue",
        encode::NotEncodable::MissingValue { .. } => "MissingValue",
        encode::NotEncodable::InnerValueShape { .. } => "InnerValueShape",
        encode::NotEncodable::NestedLabeledNull { .. } => "NestedLabeledNull",
    }
}

/// The labelled counts of O7's enumerators against their closed formulas
/// and a sum written out by hand.
#[test]
fn o7_enumerators_give_their_labelled_counts() {
    let spec = scope::o7_encoding_spec();
    let count = scope::spec_graphs(&spec, &|_, _| scope::o7_encoding_forms(), 4).len() as u64;
    let by_hand: u64 = (0..=4u32)
        .flat_map(|k| (0..=4 - k).map(move |m| (k, m)))
        .map(|(k, m)| 7u64.pow(k) << (k * m))
        .sum();
    assert_eq!(count, scope::spec_graphs_count(&spec, &|_, _| 7, 4));
    assert_eq!((count, by_hand), (6_627, 6_627));
    assert_eq!(
        scope::o7_encoding_presentations(4).len() as u64,
        scope::o7_encoding_presentations_count(4)
    );
    for (spec, cells, expected) in [
        (scope::relation_spec(), 1usize, 51u64),
        (scope::shared_column_spec(), 2, 129),
    ] {
        let forms = scope::leaf_forms(&scope::small_cells(cells));
        let enumerated = scope::spec_graphs(&spec, &|_, _| forms.clone(), 4).len() as u64;
        assert_eq!(
            enumerated,
            scope::spec_graphs_count(&spec, &|_, _| cells as u64, 4)
        );
        assert_eq!(enumerated, expected);
    }
    for case in scope::reflection_family() {
        for spec in [&case.source, &case.kept, &case.target] {
            assert_eq!(
                case.graphs(spec).len() as u64,
                case.graphs_count(spec),
                "{}",
                case.name
            );
        }
    }
    println!(
        "O7 enumerators: 6627 encoding graphs (by hand {by_hand}), {} presentations, 51 \
         relation graphs, 129 shared-column graphs",
        scope::o7_encoding_presentations_count(4)
    );
}

/// The encoding and decoding of every presentation of O7's encoding spec
/// with at most four table nodes (plain and decorated, with plain and
/// spread ids) against the reference: equal tables, equal layouts, and the
/// decoding of each encoding the input itself on both sides.
#[test]
fn o7_encoding_agrees_with_the_reference() {
    let spec = scope::o7_encoding_spec();
    let (reference, production) = spec_rel(&spec);
    let side = encode::GraphSide::whole(&reference);
    let graphs = scope::o7_encoding_presentations(4);
    assert_eq!(
        graphs.len() as u64,
        scope::o7_encoding_presentations_count(4)
    );
    let accepted = par_sum(&graphs, |graph| {
        usize::from(encodings_agree(graph, &side, &production).is_ok())
    });
    println!(
        "O7 encoding: at most 4 table nodes, {} presentations (independent count {}), {accepted} \
         encoded",
        graphs.len(),
        scope::o7_encoding_presentations_count(4)
    );
    assert_eq!(accepted, graphs.len());
}

/// The layouts the decoding law is run with: the record-free layout and the
/// reference layout of every presentation with at most two table nodes that
/// is decorated with spread ids or plain with plain ids.
fn o7_layouts(side: &encode::GraphSide) -> Vec<encode::GraphLayout> {
    let mut layouts = vec![encode::GraphLayout::default()];
    for (index, graph) in scope::o7_encoding_presentations(2).iter().enumerate() {
        if index % 4 == 1 || index % 4 == 2 {
            layouts.push(encode::encode_graph(graph, side).unwrap().1);
        }
    }
    layouts
}

/// The tables of the decoding law: every conforming table of the encoding
/// spec with at most four rows, its cell a constant, `Null`, a record, a
/// list or a labelled null.
fn o7_tables(side: &encode::GraphSide) -> Vec<model::Table> {
    let spec = scope::o7_encoding_spec();
    scope::spec_graphs(&spec, &|_, _| scope::o7_table_cells(), 4)
        .iter()
        .map(|graph| encode::encode_graph(graph, side).unwrap().0)
        .collect()
}

/// Decoding every conforming table of the encoding spec with at most four
/// rows through every layout against the reference: equal graphs, each
/// re-encoding to the table; and between any two decodings of one table the
/// identity on table nodes checks as a graph Kan homomorphism on both sides,
/// so graphs that differ only in layout are isomorphic.
#[test]
fn o7_decoding_agrees_with_the_reference_on_every_table_and_layout() {
    let spec = scope::o7_encoding_spec();
    let (reference, production) = spec_rel(&spec);
    let side = encode::GraphSide::whole(&reference);
    let tables = o7_tables(&side);
    let layouts = o7_layouts(&side);
    let table_count = scope::spec_graphs_count(&spec, &|_, _| 5, 4);
    assert_eq!(tables.len() as u64, table_count);
    assert_eq!(layouts.len(), 1 + 2 * 73);
    let production_layouts: Vec<GraphLayout> = layouts.iter().map(prod_layout).collect();
    let census = par_sum(&tables, |table| {
        let production_table = prod_table(table);
        let mut decodings: Vec<model::Graph> = Vec::new();
        for (layout, production_layout) in layouts.iter().zip(&production_layouts) {
            let ours = panproto_inst::decode_graph(
                &production_table,
                EncodingSide::Whole(&production),
                production_layout,
            )
            .unwrap();
            let theirs = encode::decode_table(table, &side, layout).unwrap();
            assert_eq!(ref_graph(&ours), theirs, "decoding {table:?}");
            let (again, _) =
                panproto_inst::encode_graph(&ours, EncodingSide::Whole(&production)).unwrap();
            assert_eq!(&ref_table(&again), table);
            if !decodings.contains(&theirs) {
                decodings.push(theirs);
            }
        }
        let identity = rel::Hom {
            rows: table.rows.keys().map(|row| (*row, *row)).collect(),
            nulls: BTreeMap::new(),
        };
        for first in &decodings {
            for second in &decodings {
                let ours = prod_graph_hom(&identity).check(
                    &prod_graph(first),
                    &prod_graph(second),
                    EncodingSide::Whole(&production),
                );
                let theirs = encode::GraphKanHom {
                    node_map: identity.rows.clone(),
                    null_map: BTreeMap::new(),
                }
                .check(first, second, &side);
                assert!(ours.is_ok() && theirs.is_ok());
            }
        }
        Census::one(if decodings.len() > 1 {
            "several decodings"
        } else {
            "one decoding"
        })
    });
    println!(
        "O7 decoding: {} tables (independent count {table_count}) x {} layouts; {:?}",
        tables.len(),
        layouts.len(),
        census.0
    );
    assert!(census.get("several decodings") > 0);
}

/// The refusal half of the encoding domain: every one of the 18 mutation
/// kinds at each of the first eight picks of every presentation with at most
/// two table nodes is encoded by production exactly as by the reference,
/// refusals with their payloads; every production refusal reason is reached.
#[test]
fn o7_encoding_refusals_agree_with_the_reference() {
    let spec = scope::o7_encoding_spec();
    let (reference, production) = spec_rel(&spec);
    let side = encode::GraphSide::whole(&reference);
    let bases = scope::o7_encoding_presentations(2);
    assert_eq!(
        bases.len() as u64,
        scope::o7_encoding_presentations_count(2)
    );
    let census = par_sum(&bases, |graph| {
        let mut census = Census::default();
        for kind in 0..scope::ENCODING_MUTATIONS {
            for pick in 0..8 {
                let mutated = scope::encoding_mutation(graph, kind, pick);
                match encodings_agree(&mutated, &side, &production) {
                    Ok(_) => census.add("accepted", 1),
                    Err(reason) => census.add(not_encodable_name(&reason), 1),
                }
            }
        }
        census
    });
    let mutated = bases.len() * usize::from(scope::ENCODING_MUTATIONS) * 8;
    println!(
        "O7 encoding refusals: {} presentations x 18 kinds x 8 picks = {mutated}; {:?}",
        bases.len(),
        census.0
    );
    assert_eq!(census.0.values().sum::<u64>(), mutated as u64);
    for reason in [
        "FanNode",
        "AnchorOutsideSide",
        "EdgeOutsideSide",
        "IncoherentEndpoints",
        "ParallelEdges",
        "MissingCell",
        "CellAsExtraField",
        "RepeatedCell",
        "SharedValue",
        "UnrootedValue",
        "MissingValue",
        "InnerValueShape",
        "NestedLabeledNull",
    ] {
        assert!(census.get(reason) > 0, "{reason} is not reached");
    }
    assert!(census.get("accepted") > 0);
}

/// The unnamed-edge spec with at most two table nodes: a record's inner
/// edges are named by their target vertex, on both sides.
#[test]
fn o7_unnamed_inner_edges_encode_as_the_reference_does() {
    let spec = scope::unnamed_inner_spec();
    let (reference, production) = spec_rel(&spec);
    let side = encode::GraphSide::whole(&reference);
    let graphs = scope::spec_graphs(&spec, &|_, _| scope::unnamed_inner_forms(), 2);
    assert_eq!(
        graphs.len() as u64,
        scope::spec_graphs_count(&spec, &|_, _| 4, 2)
    );
    assert_eq!(graphs.len(), 1 + 4 + 16);
    let mut census = Census::default();
    for graph in &graphs {
        match encodings_agree(graph, &side, &production) {
            Ok(_) => census.add("accepted", 1),
            Err(reason) => census.add(not_encodable_name(&reason), 1),
        }
    }
    let (record, _) = encode::encode_graph(&graphs[1], &side).unwrap();
    assert_eq!(
        record.rows.values().next().unwrap().1["x"],
        rvalue::Value::Unknown(BTreeMap::from([
            ("a.x.k".to_owned(), rvalue::Value::Str("s".to_owned())),
            ("a.x.item".to_owned(), rvalue::Value::Int(1)),
        ]))
    );
    println!(
        "unnamed inner edges: {} graphs; {:?}",
        graphs.len(),
        census.0
    );
    assert!(census.get("accepted") > 0 && census.get("accepted") < graphs.len() as u64);
}

/// Every map of the rows of `domain` to rows of `codomain` keeping each
/// row's table.
fn table_respecting_maps(
    domain: &model::Table,
    codomain: &model::Table,
) -> Vec<BTreeMap<u32, u32>> {
    let mut out = vec![BTreeMap::new()];
    for (row, (table, _)) in &domain.rows {
        let images: Vec<u32> = codomain
            .rows
            .iter()
            .filter(|(_, (image, _))| image == table)
            .map(|(id, _)| *id)
            .collect();
        out = out
            .into_iter()
            .flat_map(|partial| {
                images.iter().map(move |image| {
                    let mut next = partial.clone();
                    next.insert(*row, *image);
                    next
                })
            })
            .collect();
    }
    out
}

/// The null map a row map fixes: each null of `domain` goes to the cell it
/// first meets in `(row, column)` order.
fn first_occurrence_nulls(
    domain: &model::Table,
    codomain: &model::Table,
    rows: &BTreeMap<u32, u32>,
) -> BTreeMap<u64, rvalue::Value> {
    let mut nulls = BTreeMap::new();
    for (row, (_, cells)) in &domain.rows {
        let Some((_, image)) = rows.get(row).and_then(|target| codomain.rows.get(target)) else {
            continue;
        };
        for (column, value) in cells {
            if let (rvalue::Value::LabeledNull(null), Some(target)) = (value, image.get(column)) {
                nulls.entry(*null).or_insert_with(|| target.clone());
            }
        }
    }
    nulls
}

/// Every composite of two reference homomorphisms between graphs of a hom
/// family with at most three table nodes, composed by production and by the
/// reference; returns how many.
fn graph_kan_composites_agree(
    spec: &scope::GraphSpec,
    forms: &[scope::CellForm],
    side: &encode::GraphSide,
) -> u64 {
    let small: Vec<(model::Graph, model::Table)> =
        scope::spec_graphs(spec, &|_, _| forms.to_vec(), 3)
            .into_iter()
            .map(|graph| {
                let table = encode::encode_graph(&graph, side).unwrap().0;
                (graph, table)
            })
            .collect();
    par_sum(&small, |(_, g_table)| {
        let mut composites = 0u64;
        for (_, h_table) in &small {
            for first in rel::homs(g_table, h_table) {
                for (_, k_table) in &small {
                    for second in rel::homs(h_table, k_table) {
                        let ours = prod_graph_hom(&first)
                            .compose(&prod_graph_hom(&second))
                            .unwrap();
                        let theirs = encode::GraphKanHom {
                            node_map: first.rows.clone(),
                            null_map: first.nulls.clone(),
                        }
                        .compose(&encode::GraphKanHom {
                            node_map: second.rows.clone(),
                            null_map: second.nulls.clone(),
                        })
                        .unwrap();
                        assert!(ref_graph_hom(&ours).same_as(
                            &rel::Hom {
                                rows: theirs.node_map.clone(),
                                nulls: theirs.null_map.clone(),
                            },
                            g_table
                        ));
                        composites += 1;
                    }
                }
            }
        }
        composites
    })
}

/// The graph Kan homomorphisms of every hom family with at most four table
/// nodes against the reference: for every ordered pair of graphs and every
/// table-respecting node map with the null map its first occurrences fix,
/// `GraphKanHom::check` gives the reference's result, refusal payload
/// included, and accepts exactly the reference homomorphisms of the
/// encodings (so `E` is full and faithful); identities agree; and on graphs
/// with at most three table nodes every composite of two homomorphisms is
/// the reference composite.
#[test]
fn o7_graph_kan_homs_agree_with_the_reference() {
    let mut total = Census::default();
    for (name, spec, forms) in scope::hom_families() {
        let (reference, production) = spec_rel(&spec);
        let side = encode::GraphSide::whole(&reference);
        let graphs = scope::spec_graphs(&spec, &|_, _| forms.clone(), 4);
        let count = forms.len().max(1) as u64;
        assert_eq!(
            graphs.len() as u64,
            scope::spec_graphs_count(&spec, &|_, _| count, 4)
        );
        let prepared: Vec<(model::Graph, GInstance, model::Table)> = graphs
            .iter()
            .map(|graph| {
                let table = encode::encode_graph(graph, &side).unwrap().0;
                (graph.clone(), prod_graph(graph), table)
            })
            .collect();
        let census = par_sum(&prepared, |(g, production_g, g_table)| {
            let mut census = Census::default();
            let identity =
                GraphKanHom::identity(production_g, &EncodingSide::Whole(&production)).unwrap();
            let reference_identity = encode::GraphKanHom::identity(g, &side).unwrap();
            assert!(ref_graph_hom(&identity).normalized_same_as(&rel::Hom {
                rows: reference_identity.node_map,
                nulls: reference_identity.null_map,
            }));
            for (h, production_h, h_table) in &prepared {
                let homs: BTreeSet<HomKey> = rel::homs(g_table, h_table)
                    .iter()
                    .map(|hom| hom_key(hom, g_table))
                    .collect();
                census.add("homs", homs.len() as u64);
                for rows in table_respecting_maps(g_table, h_table) {
                    let nulls = first_occurrence_nulls(g_table, h_table, &rows);
                    let candidate = rel::Hom { rows, nulls };
                    let ours = prod_graph_hom(&candidate).check(
                        production_g,
                        production_h,
                        EncodingSide::Whole(&production),
                    );
                    let theirs = encode::GraphKanHom {
                        node_map: candidate.rows.clone(),
                        null_map: candidate.nulls.clone(),
                    }
                    .check(g, h, &side);
                    match (&ours, &theirs) {
                        (Ok(()), Ok(())) => {
                            assert!(homs.contains(&hom_key(&candidate, g_table)));
                            census.add("accepted", 1);
                        }
                        (Err(ours), Err(theirs)) => {
                            graph_errors_agree(ours, theirs, &(name, g, h));
                            assert!(!homs.contains(&hom_key(&candidate, g_table)));
                            census.add("refused", 1);
                        }
                        other => panic!("{name}: {g:?} -> {h:?}: {other:?}"),
                    }
                }
            }
            census
        });
        println!("hom family {name}: {} graphs; {:?}", graphs.len(), census.0);
        assert!(census.get("homs") > 0);
        total += census;

        let composites = graph_kan_composites_agree(&spec, &forms, &side);
        println!("hom family {name}: {composites} composites on graphs of at most 3 table nodes");
        assert!(composites > 0);
    }
    println!("graph Kan homs: {:?}", total.0);
}

// ---------------------------------------------------------------------------
// O7's adjunction family: the graph Kan functors, their units, counits and
// transposes, the kept-part restriction and the separated reflection.
// ---------------------------------------------------------------------------

/// The functors of a graph case and its three sides.
struct GraphCase {
    case: scope::GraphKanCase,
    functors: Functors,
    whole: encode::GraphSide,
    kept: encode::GraphSide,
    target: encode::GraphSide,
}

impl GraphCase {
    fn new(case: scope::GraphKanCase) -> Self {
        let functors = Functors::new(
            &case.source.schema(),
            &case.target.schema(),
            &case.migration(),
            &graph_spec_kinds(),
        );
        let f = &functors.reference;
        Self {
            whole: encode::GraphSide::whole(&f.src),
            kept: encode::GraphSide::kept(f),
            target: encode::GraphSide::whole(&f.tgt),
            case,
            functors,
        }
    }
}

fn graph_homs_agree<T: std::fmt::Debug>(
    production: &Result<GraphKanHom, GraphKanError>,
    reference: &Result<encode::GraphKanHom, encode::GraphKanError>,
    domain: &model::Table,
    context: &T,
) {
    match (production, reference) {
        (Ok(ours), Ok(theirs)) => assert!(
            ref_graph_hom(ours).same_as(
                &rel::Hom {
                    rows: theirs.node_map.clone(),
                    nulls: theirs.null_map.clone(),
                },
                domain
            ),
            "{context:?}: production {ours:?}, reference {theirs:?}"
        ),
        (Err(ours), Err(theirs)) => graph_errors_agree(ours, theirs, context),
        (ours, theirs) => panic!("{context:?}: production {ours:?}, reference {theirs:?}"),
    }
}

/// The table of a graph encoded over a side, by the reference.
fn encoded(graph: &model::Graph, side: &encode::GraphSide) -> model::Table {
    encode::encode_graph(graph, side).unwrap().0
}

/// Both graph adjunctions of one family case at one source graph `g` (of
/// the kept part) against every target graph: Sigma and Pi of `g`, the unit
/// and the Pi counit at `g`, and for every target the transposes of every
/// homomorphism of both hom-sets of both adjunctions, each equal to the
/// reference transpose, each round trip the identity.
fn graph_adjunctions_at_source(
    case: &GraphCase,
    g: &model::Graph,
    targets: &[(model::Graph, GInstance, model::Table)],
) -> Census {
    let production_g = prod_graph(g);
    let g_table = encoded(g, &case.kept);
    let mut census = Census::default();
    census += graph_sigma_adjunction_at_source(case, (g, &production_g, &g_table), targets);
    census += graph_pi_adjunction_at_source(case, (g, &production_g, &g_table), targets);
    census
}

/// The graph Sigma -| Delta half of [`graph_adjunctions_at_source`].
fn graph_sigma_adjunction_at_source(
    case: &GraphCase,
    (g, production_g, g_table): (&model::Graph, &GInstance, &model::Table),
    targets: &[(model::Graph, GInstance, model::Table)],
) -> Census {
    let (f, fp) = (&case.functors.reference, &case.functors.production);
    let mut census = Census::default();
    let sigma = encode::g_sigma_restricted_with_provenance(f, g);
    let production_sigma = panproto_inst::g_sigma_restricted_with_provenance(production_g, fp);
    match (&production_sigma, &sigma) {
        (Ok((ours, our_provenance)), Ok((theirs, provenance))) => {
            assert_eq!(&ref_graph(ours), theirs, "Sigma of {g:?}");
            assert_eq!(
                ref_sigma_provenance(&our_provenance.table),
                provenance.table
            );
            assert_eq!(
                ref_contributors(&our_provenance.nodes, &our_provenance.edges),
                (provenance.nodes.clone(), provenance.edges.clone())
            );
        }
        (Err(ours), Err(theirs)) => graph_errors_agree(ours, theirs, &("Sigma", g)),
        (ours, theirs) => panic!("Sigma of {g:?}: production {ours:?}, reference {theirs:?}"),
    }
    graph_homs_agree(
        &panproto_inst::g_unit(production_g, fp),
        &encode::g_unit(g, f),
        g_table,
        &("unit", g),
    );
    if let Ok((sigma, _)) = &sigma {
        let sigma_table = encoded(sigma, &case.target);
        for (h, production_h, h_table) in targets {
            let delta_table = encoded(&encode::g_delta_along(f, h).unwrap(), &case.kept);
            let left = rel::homs(&sigma_table, h_table);
            let right = rel::homs(g_table, &delta_table);
            assert_eq!(left.len(), right.len(), "{}", case.case.name);
            for hom in &left {
                let ours = panproto_inst::g_transpose_left(
                    &prod_graph_hom(hom),
                    production_g,
                    production_h,
                    fp,
                );
                let theirs = encode::g_transpose_left(
                    &encode::GraphKanHom {
                        node_map: hom.rows.clone(),
                        null_map: hom.nulls.clone(),
                    },
                    g,
                    h,
                    f,
                );
                graph_homs_agree(&ours, &theirs, g_table, &("Sigma transpose", g, h));
                let back = panproto_inst::g_transpose_right(
                    &ours.unwrap(),
                    production_g,
                    production_h,
                    fp,
                )
                .unwrap();
                assert!(ref_graph_hom(&back).same_as(hom, &sigma_table));
            }
            for hom in &right {
                let ours = panproto_inst::g_transpose_right(
                    &prod_graph_hom(hom),
                    production_g,
                    production_h,
                    fp,
                );
                let theirs = encode::g_transpose_right(
                    &encode::GraphKanHom {
                        node_map: hom.rows.clone(),
                        null_map: hom.nulls.clone(),
                    },
                    g,
                    h,
                    f,
                );
                graph_homs_agree(
                    &ours,
                    &theirs,
                    &sigma_table,
                    &("Sigma inverse transpose", g, h),
                );
                let back =
                    panproto_inst::g_transpose_left(&ours.unwrap(), production_g, production_h, fp)
                        .unwrap();
                assert!(ref_graph_hom(&back).same_as(hom, g_table));
            }
            census.add("Sigma homs", left.len() as u64);
        }
        census.add("Sigma ok", 1);
    } else {
        for (h, _, _) in targets {
            let delta_table = encoded(&encode::g_delta_along(f, h).unwrap(), &case.kept);
            assert!(
                rel::homs(g_table, &delta_table).is_empty(),
                "a clash has no homs into Delta"
            );
        }
        census.add("Sigma refused", 1);
    }
    census
}

/// The graph Delta -| Pi half of [`graph_adjunctions_at_source`].
fn graph_pi_adjunction_at_source(
    case: &GraphCase,
    (g, production_g, g_table): (&model::Graph, &GInstance, &model::Table),
    targets: &[(model::Graph, GInstance, model::Table)],
) -> Census {
    let (f, fp) = (&case.functors.reference, &case.functors.production);
    let mut census = Census::default();
    let pi = encode::g_pi_restricted(f, g, 81);
    let production_pi = panproto_inst::g_pi_restricted(production_g, fp, 81);
    graph_results_agree(&production_pi, &pi, &("Pi", g));
    graph_homs_agree(
        &panproto_inst::g_pi_counit(production_g, fp, 81),
        &encode::g_pi_counit(g, f, 81),
        &pi.as_ref()
            .map(|p| encoded(&encode::g_delta_along(f, p).unwrap(), &case.kept))
            .unwrap_or_default(),
        &("Pi counit", g),
    );
    if let Ok(pi) = &pi {
        let pi_table = encoded(pi, &case.target);
        for (h, production_h, h_table) in targets {
            let delta_table = encoded(&encode::g_delta_along(f, h).unwrap(), &case.kept);
            let left = rel::homs(&delta_table, g_table);
            let right = rel::homs(h_table, &pi_table);
            assert_eq!(left.len(), right.len(), "{}", case.case.name);
            for hom in &left {
                let ours = panproto_inst::g_pi_transpose_left(
                    &prod_graph_hom(hom),
                    production_h,
                    production_g,
                    fp,
                    81,
                );
                let theirs = encode::g_pi_transpose_left(
                    &encode::GraphKanHom {
                        node_map: hom.rows.clone(),
                        null_map: hom.nulls.clone(),
                    },
                    h,
                    g,
                    f,
                    81,
                );
                graph_homs_agree(&ours, &theirs, h_table, &("Pi transpose", g, h));
                let back = panproto_inst::g_pi_transpose_right(
                    &ours.unwrap(),
                    production_h,
                    production_g,
                    fp,
                    81,
                )
                .unwrap();
                assert!(ref_graph_hom(&back).same_as(hom, &delta_table));
            }
            for hom in &right {
                let ours = panproto_inst::g_pi_transpose_right(
                    &prod_graph_hom(hom),
                    production_h,
                    production_g,
                    fp,
                    81,
                );
                let theirs = encode::g_pi_transpose_right(
                    &encode::GraphKanHom {
                        node_map: hom.rows.clone(),
                        null_map: hom.nulls.clone(),
                    },
                    h,
                    g,
                    f,
                    81,
                );
                graph_homs_agree(
                    &ours,
                    &theirs,
                    &delta_table,
                    &("Pi inverse transpose", g, h),
                );
                let back = panproto_inst::g_pi_transpose_left(
                    &ours.unwrap(),
                    production_h,
                    production_g,
                    fp,
                    81,
                )
                .unwrap();
                assert!(ref_graph_hom(&back).same_as(hom, h_table));
            }
            census.add("Pi homs", left.len() as u64);
        }
        census.add("Pi ok", 1);
    } else {
        census.add("Pi refused", 1);
    }
    census
}

/// Delta, the counit and the Pi unit at one target graph against the
/// reference.
fn graph_adjunctions_at_target(case: &GraphCase, h: &model::Graph) -> Census {
    let (f, fp) = (&case.functors.reference, &case.functors.production);
    let production_h = prod_graph(h);
    let h_table = encoded(h, &case.target);
    let (delta, provenance) = encode::g_delta_along_with_provenance(f, h).unwrap();
    let (ours, our_provenance) =
        panproto_inst::g_delta_along_with_provenance(&production_h, fp).unwrap();
    assert_eq!(ref_graph(&ours), delta, "Delta of {h:?}");
    assert_eq!(
        ref_delta_provenance(&our_provenance.table),
        provenance.table
    );
    assert_eq!(
        ref_contributors(&our_provenance.nodes, &our_provenance.edges),
        (provenance.nodes, provenance.edges)
    );
    let counit_domain = encode::g_sigma_restricted(f, &delta)
        .map(|s| encoded(&s, &case.target))
        .unwrap_or_default();
    graph_homs_agree(
        &panproto_inst::g_counit(&production_h, fp),
        &encode::g_counit(h, f),
        &counit_domain,
        &("counit", h),
    );
    graph_homs_agree(
        &panproto_inst::g_pi_unit(&production_h, fp, 81),
        &encode::g_pi_unit(h, f, 81),
        &h_table,
        &("Pi unit", h),
    );
    Census::one("targets")
}

/// Both graph adjunctions on every graph of O7's adjunction family (the
/// four merges and the three column cases, at most four table nodes, kept
/// sources against every target) against the reference.
#[test]
fn o7_graph_adjunctions_agree_with_the_reference() {
    for case in scope::o7_adjunction_family() {
        let case = GraphCase::new(case);
        let sources = case.case.graphs(&case.case.kept);
        let targets = case.case.graphs(&case.case.target);
        assert_eq!(
            sources.len() as u64,
            case.case.graphs_count(&case.case.kept)
        );
        assert_eq!(
            targets.len() as u64,
            case.case.graphs_count(&case.case.target)
        );
        let mut census = par_sum(&targets, |h| graph_adjunctions_at_target(&case, h));
        let prepared: Vec<(model::Graph, GInstance, model::Table)> = targets
            .iter()
            .map(|h| (h.clone(), prod_graph(h), encoded(h, &case.target)))
            .collect();
        census += par_sum(&sources, |g| {
            graph_adjunctions_at_source(&case, g, &prepared)
        });
        println!(
            "{}: at most {} table nodes, {} sources, {} targets; {:?}",
            case.case.name,
            case.case.bound,
            sources.len(),
            targets.len(),
            census.0
        );
        assert_eq!(
            census.get("Sigma refused") > 0,
            case.case.name == "two columns sent to one",
            "{}",
            case.case.name
        );
        assert_eq!(
            census.get("Pi refused") > 0,
            !case.case.pi,
            "{}",
            case.case.name
        );
    }
}

/// Every ordering of `items`.
fn permutations(items: &[u32]) -> Vec<Vec<u32>> {
    if items.is_empty() {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for (i, first) in items.iter().enumerate() {
        let mut rest = items.to_vec();
        rest.remove(i);
        for mut tail in permutations(&rest) {
            tail.insert(0, *first);
            out.push(tail);
        }
    }
    out
}

/// Whether the rows and pairs of `graph` (over `target`) are those of the
/// separated presheaf `pairs` with rows `rows`, up to a bijection of rows
/// within each table that commutes with the legs: the comparison for a
/// construction defined up to isomorphism.
fn rows_and_pairs_are_isomorphic(
    rows: &BTreeMap<String, Vec<usize>>,
    pairs: &rel::EndpointPairs,
    graph: &model::Graph,
    target: &encode::GraphSide,
) -> bool {
    let mut out_rows: BTreeMap<String, Vec<u32>> = target
        .tables
        .iter()
        .map(|t| (t.clone(), Vec::new()))
        .collect();
    for node in graph.nodes.values() {
        if let Some(list) = out_rows.get_mut(&node.anchor) {
            list.push(node.id);
        }
    }
    let mut out_pairs: BTreeMap<model::SEdge, BTreeMap<(u32, u32), usize>> = target
        .relations
        .iter()
        .map(|r| (r.clone(), BTreeMap::new()))
        .collect();
    for edge in graph.edges.values() {
        if let Some(multiset) = out_pairs.get_mut(&edge.label) {
            *multiset.entry((edge.src, edge.tgt)).or_default() += 1;
        }
    }
    if rows.keys().ne(out_rows.keys()) || rows.iter().any(|(t, l)| l.len() != out_rows[t].len()) {
        return false;
    }
    let tables: Vec<&String> = rows.keys().collect();
    let mut choices: Vec<BTreeMap<(&String, usize), u32>> = vec![BTreeMap::new()];
    for table in &tables {
        let next: Vec<_> = choices
            .iter()
            .flat_map(|partial| {
                permutations(&out_rows[*table])
                    .into_iter()
                    .map(move |image| {
                        let mut phi = partial.clone();
                        for (row, id) in rows[*table].iter().zip(image) {
                            phi.insert((*table, *row), id);
                        }
                        phi
                    })
            })
            .collect();
        choices = next;
    }
    choices.into_iter().any(|phi| {
        pairs.iter().all(|(relation, multiset)| {
            let mut renamed: BTreeMap<(u32, u32), usize> = BTreeMap::new();
            for ((src, tgt), count) in multiset {
                let ends = (phi[&(&relation.src, *src)], phi[&(&relation.tgt, *tgt)]);
                *renamed.entry(ends).or_default() += count;
            }
            renamed == out_pairs[relation]
        })
    })
}

/// The reference presheaf left Kan extension of a whole-source graph's
/// entity instance on the kept part: its rows per target table, `L_sep`,
/// the unseparated pairs and whether `L_sep` identified any pair.
fn presheaf_sigma(
    case: &GraphCase,
    graph: &model::Graph,
) -> (
    BTreeMap<String, Vec<usize>>,
    rel::EndpointPairs,
    rel::EndpointPairs,
    usize,
) {
    let f = &case.functors.reference;
    let kept_tables: BTreeSet<String> = f.tables.keys().cloned().collect();
    let kept_relations: BTreeSet<model::SEdge> = f.relations.keys().cloned().collect();
    let instance = rel::entity_instance(graph, &kept_tables, &kept_relations);
    let kan =
        rel::presheaf_left_kan(&instance, f, &case.target.tables, &case.target.relations).unwrap();
    let rows: BTreeMap<String, Vec<usize>> = case
        .target
        .tables
        .iter()
        .map(|t| {
            let set = &kan.sets[&rel::EntityObject::Table(t.clone())];
            (t.clone(), set.iter().copied().collect())
        })
        .collect();
    for (t, list) in &rows {
        let fibre: usize = f
            .tables
            .iter()
            .filter(|(_, image)| *image == t)
            .map(|(s, _)| instance.sets[&rel::EntityObject::Table(s.clone())].len())
            .sum();
        assert_eq!(list.len(), fibre, "{}: rows at {t}", case.case.name);
    }
    let (separated, identified) = rel::separated_pairs(&kan, &case.target.relations).unwrap();
    let unseparated = rel::unseparated_pairs(&kan, &case.target.relations).unwrap();
    (rows, separated, unseparated, identified)
}

/// Graph Sigma at one whole-source graph through its six entry points
/// against the reference: each output equal to the reference graph Sigma
/// and, rows and pairs, isomorphic to `L_sep` of the reference presheaf
/// Sigma; each refusal the reference's, exactly where the reference value
/// equations clash.
fn sigma_reflection_at(case: &GraphCase, graph: &model::Graph) -> Census {
    let (f, fp) = (&case.functors.reference, &case.functors.production);
    let production_graph = prod_graph(graph);
    let kept = encode::restrict_graph(graph, &case.kept);
    let production_kept = prod_graph(&kept);
    let (whole_table, _) =
        panproto_inst::encode_graph(&production_graph, EncodingSide::Whole(fp.source())).unwrap();
    let (kept_table, _) =
        panproto_inst::encode_graph(&production_kept, EncodingSide::Domain(fp)).unwrap();
    assert_eq!(
        ref_table(&panproto_inst::f_restrict_to_domain(&whole_table, fp).unwrap()),
        ref_table(&kept_table),
        "E(rho G) = restrict(E G)"
    );
    assert_eq!(
        ref_table(&kept_table),
        rel::restrict_to_kept(&encoded(graph, &case.whole), f)
    );
    let (rows, separated, _, identified) = presheaf_sigma(case, graph);
    let clash = rel::sigma_values_clash(&ref_table(&kept_table), f).unwrap();
    let reference = encode::g_sigma_along(f, graph);
    assert_eq!(reference.is_err(), clash, "{}: {graph:?}", case.case.name);
    let reference_restricted = encode::g_sigma_restricted(f, &kept);
    let results = [
        (
            "g_sigma",
            panproto_inst::g_sigma(
                &production_graph,
                &case.functors.source_schema,
                &case.functors.protocol,
                &case.functors.target_schema,
                &case.functors.protocol,
                &case.functors.migration,
            ),
            &reference,
        ),
        (
            "g_sigma_with_provenance",
            panproto_inst::g_sigma_with_provenance(
                &production_graph,
                &case.functors.source_schema,
                &case.functors.protocol,
                &case.functors.target_schema,
                &case.functors.protocol,
                &case.functors.migration,
            )
            .map(|(o, _)| o),
            &reference,
        ),
        (
            "g_sigma_along",
            panproto_inst::g_sigma_along(&production_graph, fp),
            &reference,
        ),
        (
            "g_sigma_along_with_provenance",
            panproto_inst::g_sigma_along_with_provenance(&production_graph, fp).map(|(o, _)| o),
            &reference,
        ),
        (
            "g_sigma_restricted",
            panproto_inst::g_sigma_restricted(&production_kept, fp),
            &reference_restricted,
        ),
        (
            "g_sigma_restricted_with_provenance",
            panproto_inst::g_sigma_restricted_with_provenance(&production_kept, fp).map(|(o, _)| o),
            &reference_restricted,
        ),
    ];
    let mut census = Census::default();
    for (name, ours, theirs) in &results {
        graph_results_agree(ours, theirs, &(case.case.name, name, graph));
        match ours {
            Ok(out) => {
                assert!(
                    rows_and_pairs_are_isomorphic(&rows, &separated, &ref_graph(out), &case.target),
                    "{}: {name} is not L_sep of the presheaf Sigma on {graph:?}",
                    case.case.name
                );
                census.add("compared", 1);
            }
            Err(_) => census.add("refused", 1),
        }
    }
    census.add("graphs", 1);
    if identified > 0 {
        census.add("separated", 1);
    }
    census
}

/// Graph Sigma is the separated reflection of the presheaf Sigma: on every
/// whole-source graph of every case of the reflection family (the
/// adjunction family and the three relation-merge cases), each of the six
/// graph Sigma entry points equals the reference graph Sigma and is, on rows
/// and pairs, `L_sep` of the reference pointwise left Kan extension; the
/// restriction to the kept part is the reference's.
#[test]
fn g_sigma_is_the_separated_reflection_of_the_reference_presheaf_sigma() {
    let mut total = Census::default();
    for case in scope::reflection_family() {
        let case = GraphCase::new(case);
        let graphs = case.case.graphs(&case.case.source);
        assert_eq!(
            graphs.len() as u64,
            case.case.graphs_count(&case.case.source)
        );
        let census = par_sum(&graphs, |graph| sigma_reflection_at(&case, graph));
        println!(
            "{}: at most {} table nodes; {} whole-source graphs (independent count {}); {:?}",
            case.case.name,
            case.case.bound,
            graphs.len(),
            case.case.graphs_count(&case.case.source),
            census.0
        );
        assert_eq!(
            census.get("compared") + census.get("refused"),
            6 * graphs.len() as u64
        );
        total += census;
    }
    println!("separated reflection: {:?}", total.0);
    assert!(total.get("separated") > 0 && total.get("refused") > 0);
}

/// Whether a cell value is one the declared kind of its column admits at
/// the extension's validation boundary: anything under `value`, integers
/// under `int`, strings under `str`.
fn admitted_by_kind(kind: &str, value: &rvalue::Value) -> bool {
    match kind {
        "value" => true,
        "int" => matches!(value, rvalue::Value::Int(_)),
        "str" => matches!(value, rvalue::Value::Str(_)),
        _ => false,
    }
}

/// The total extensions at one whole-source graph against the reference:
/// the table extension is the reference Sigma of the encoding, exactly and
/// as `L_sep` of the presheaf Sigma; the graph extension is the unreflected
/// presheaf Sigma. Outside the extension's domain each refuses: a cell its
/// declared kind does not admit, or a realized target table with a column
/// outside the image, fails validation; an element outside the kept part
/// has no image; and the table extension refuses a clash of constants.
fn extend_reflection_at(case: &GraphCase, graph: &model::Graph) -> Census {
    let f = &case.functors.reference;
    let functors = &case.functors;
    let production_graph = prod_graph(graph);
    let whole = encoded(graph, &case.whole);
    let kept = rel::restrict_to_kept(&whole, f);
    let outside_domain = encode::restrict_graph(graph, &case.kept) != *graph;
    let (rows, separated, unseparated, _) = presheaf_sigma(case, graph);
    let clash = rel::sigma_values_clash(&kept, f).unwrap();
    let invalid_boundary = whole.rows.values().any(|(table, cells)| {
        cells.iter().any(|(column, value)| {
            let kind = &f.src.kinds[&format!("{table}.{column}")];
            !admitted_by_kind(kind, value)
        })
    });
    let missing_target_column = case.target.tables.iter().any(|table| {
        rows.get(table).is_some_and(|rows| !rows.is_empty())
            && case.target.columns[table].keys().any(|column| {
                !f.columns
                    .iter()
                    .any(|((source, _), image)| f.tables[source] == *table && image == column)
            })
    });
    let production_table = prod_table(&whole);
    let mut census = Census::default();
    match panproto_inst::functor_extend(
        &production_table,
        &functors.source_schema,
        &functors.target_schema,
        &functors.migration,
        functors.protocols(),
    ) {
        Err(RestrictError::Validation { .. }) if invalid_boundary || missing_target_column => {
            census.add("table refused", 1);
        }
        Err(RestrictError::UnmappedAnchor { .. } | RestrictError::UnmappedEdge { .. })
            if !invalid_boundary && outside_domain =>
        {
            census.add("table refused", 1);
        }
        Err(RestrictError::SigmaConstantClash { .. })
            if !invalid_boundary && !outside_domain && clash =>
        {
            census.add("table refused", 1);
        }
        Ok(output) if !invalid_boundary && !missing_target_column && !outside_domain && !clash => {
            let (sigma, _) = rel::sigma(f, &whole).unwrap();
            assert_eq!(
                ref_table(&output),
                sigma,
                "{}: the table extension",
                case.case.name
            );
            let (output_graph, _) = GInstance::from_finstance(&output).unwrap();
            assert!(rows_and_pairs_are_isomorphic(
                &rows,
                &separated,
                &ref_graph(&output_graph),
                &case.target
            ));
            census.add("table compared", 1);
        }
        other => panic!(
            "{}: functor_extend on {graph:?}, invalid {invalid_boundary}, missing column \
             {missing_target_column}, outside {outside_domain}, clash {clash}: {other:?}",
            case.case.name
        ),
    }
    match panproto_inst::graph_extend(
        &production_graph,
        &functors.source_schema,
        &functors.target_schema,
        &functors.migration,
        functors.protocols(),
    ) {
        Err(RestrictError::UnmappedAnchor { .. } | RestrictError::UnmappedEdge { .. })
            if outside_domain =>
        {
            census.add("graph refused", 1);
        }
        Ok(output) if !outside_domain => {
            assert!(rows_and_pairs_are_isomorphic(
                &rows,
                &unseparated,
                &ref_graph(&output),
                &case.target
            ));
            census.add("graph compared", 1);
        }
        other => panic!(
            "{}: graph_extend on {graph:?}, outside {outside_domain}: {other:?}",
            case.case.name
        ),
    }
    census
}

/// The total extensions on every whole-source graph of the reflection
/// family against the reference Sigma and presheaf Sigma.
#[test]
fn an_extend_agrees_with_the_reference_sigma_and_its_presheaf() {
    let mut total = Census::default();
    let mut graphs_seen = 0u64;
    for case in scope::reflection_family() {
        let case = GraphCase::new(case);
        let graphs = case.case.graphs(&case.case.source);
        assert_eq!(
            graphs.len() as u64,
            case.case.graphs_count(&case.case.source)
        );
        let census = par_sum(&graphs, |graph| extend_reflection_at(&case, graph));
        println!(
            "{}: bound {}, {} whole-source graphs; {:?}",
            case.case.name,
            case.case.bound,
            graphs.len(),
            census.0
        );
        graphs_seen += graphs.len() as u64;
        total += census;
    }
    println!("total extensions: {graphs_seen} graphs; {:?}", total.0);
    assert_eq!(
        total.get("table compared") + total.get("table refused"),
        graphs_seen
    );
    assert_eq!(
        total.get("graph compared") + total.get("graph refused"),
        graphs_seen
    );
    assert!(total.get("table refused") > 0 && total.get("graph refused") > 0);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Graph Sigma is the separated reflection of the reference presheaf
    /// Sigma on sampled graphs of every case of the reflection family with
    /// up to six table nodes, above the exhaustive bound of four: `counts`
    /// gives each table's node count, `cells` each cell's leaf, and `masks`
    /// each relation's subset of the possible pairs.
    #[test]
    fn g_sigma_is_the_separated_reflection_on_sampled_graphs(
        index in 0usize..10,
        counts in proptest::collection::vec(0usize..=3, 3),
        cells in proptest::collection::vec(0usize..3, 12),
        masks in proptest::collection::vec(any::<u64>(), 3),
    ) {
        static CASES: std::sync::OnceLock<Vec<GraphCase>> = std::sync::OnceLock::new();
        let cases = CASES.get_or_init(|| {
            scope::reflection_family().into_iter().map(GraphCase::new).collect()
        });
        prop_assert_eq!(cases.len(), 10);
        let case = &cases[index];
        let counts = &counts[..case.case.source.tables.len()];
        prop_assume!(counts.iter().sum::<usize>() <= 6);
        let forms = scope::leaf_forms(&case.case.cells);
        let graph = scope::spec_graph(&case.case.source, counts, &forms, &cells, &masks);
        let census = sigma_reflection_at(case, &graph);
        prop_assert_eq!(census.get("compared") + census.get("refused"), 6);
    }

    /// The five-pass encoding domain, both halves: a presentation of O7's
    /// encoding spec with at most three table nodes, changed by up to three
    /// mutations (most of which leave the domain), is encoded by production
    /// exactly as by the reference: the same table and layout, or the same
    /// refusal with its payload.
    #[test]
    fn encoding_accepts_exactly_the_five_pass_domain(
        base in any::<proptest::sample::Index>(),
        mutations in proptest::collection::vec((0..scope::ENCODING_MUTATIONS, 0usize..16), 0..=3),
    ) {
        static BASES: std::sync::OnceLock<Vec<model::Graph>> = std::sync::OnceLock::new();
        let bases = BASES.get_or_init(|| scope::o7_encoding_presentations(3));
        let mut graph = bases[base.index(bases.len())].clone();
        for (kind, pick) in mutations {
            graph = scope::encoding_mutation(&graph, kind, pick);
        }
        let spec = scope::o7_encoding_spec();
        let (reference, production) = spec_rel(&spec);
        let side = encode::GraphSide::whole(&reference);
        let _ = encodings_agree(&graph, &side, &production);
    }
}

/// A hand-computed Pi fibre product: along the merge of `a{x}` and `b{x}`
/// into `t{x}`, Pi of `a#0{x=0}, a#1{x=1}, b#2{x=0}, b#3{x=0}` holds one `t`
/// row per pair `(a row, b row)` that agrees on `x`, that is `(0, 2)` and
/// `(0, 3)`, each with `x = 0`, on both sides.
#[test]
fn graph_pi_keeps_the_agreeing_pairs_of_a_merge() {
    let case = scope::o7_adjunction_family()
        .into_iter()
        .find(|case| case.name == "a merge with a shared column")
        .unwrap();
    let case = GraphCase::new(case);
    let table = model::Table {
        rows: BTreeMap::from([
            (
                0,
                (
                    "a".to_owned(),
                    BTreeMap::from([("x".to_owned(), rvalue::Value::Int(0))]),
                ),
            ),
            (
                1,
                (
                    "a".to_owned(),
                    BTreeMap::from([("x".to_owned(), rvalue::Value::Int(1))]),
                ),
            ),
            (
                2,
                (
                    "b".to_owned(),
                    BTreeMap::from([("x".to_owned(), rvalue::Value::Int(0))]),
                ),
            ),
            (
                3,
                (
                    "b".to_owned(),
                    BTreeMap::from([("x".to_owned(), rvalue::Value::Int(0))]),
                ),
            ),
        ]),
        pairs: BTreeSet::new(),
    };
    let graph = encode::decode_table(&table, &case.whole, &encode::GraphLayout::default()).unwrap();
    let (ours, provenance) = panproto_inst::g_pi_along_with_provenance(
        &prod_graph(&graph),
        &case.functors.production,
        81,
    )
    .unwrap();
    let (theirs, reference) =
        encode::g_pi_along_with_provenance(&case.functors.reference, &graph, 81).unwrap();
    assert_eq!(ref_graph(&ours), theirs);
    assert_eq!(ref_pi_provenance(&provenance.table), reference.table);
    let tuples: BTreeSet<Vec<u32>> = reference.table.values().map(|(_, t)| t.clone()).collect();
    assert_eq!(tuples, BTreeSet::from([vec![0, 2], vec![0, 3]]));
}

// ---------------------------------------------------------------------------
// Table functors by their maps: hand-built cases, generated cases and Pi's
// joins.
// ---------------------------------------------------------------------------

/// The functors a table spec states, over its whole source.
fn spec_functors(spec: &scope::TableSpec) -> Functors {
    Functors::new(
        &spec.src.schema(),
        &spec.tgt.schema(),
        &spec.migration(),
        &table_spec_kinds(),
    )
}

/// The functors a table spec states, over its kept part: the source
/// replaced by the tables, columns and relations the spec keeps.
fn kept_functors(spec: &scope::TableSpec) -> Functors {
    Functors::new(
        &spec.kept().schema(),
        &spec.tgt.schema(),
        &spec.migration(),
        &table_spec_kinds(),
    )
}

fn table_side(tables: &[(&str, &[&str])], relations: &[model::SEdge]) -> scope::TableSide {
    scope::TableSide {
        tables: tables
            .iter()
            .map(|(t, columns)| {
                (
                    (*t).to_owned(),
                    columns.iter().map(|c| (*c).to_owned()).collect(),
                )
            })
            .collect(),
        relations: relations.iter().cloned().collect(),
    }
}

fn table_spec(
    src: scope::TableSide,
    tgt: scope::TableSide,
    tables: &[(&str, &str)],
    columns: &[((&str, &str), &str)],
) -> scope::TableSpec {
    scope::TableSpec {
        src,
        tgt,
        tables: tables
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect(),
        relations: BTreeMap::new(),
        columns: columns
            .iter()
            .map(|((t, c), b)| (((*t).to_owned(), (*c).to_owned()), (*b).to_owned()))
            .collect(),
    }
}

/// `post(title) -> post(name)`.
fn rename_spec() -> scope::TableSpec {
    table_spec(
        table_side(&[("post", &["title"])], &[]),
        table_side(&[("post", &["name"])], &[]),
        &[("post", "post")],
        &[(("post", "title"), "name")],
    )
}

/// `t(a1, a2, c) -> t(b, c)` with `a1` and `a2` sent to `b`.
fn equating_spec() -> scope::TableSpec {
    table_spec(
        table_side(&[("t", &["a1", "a2", "c"])], &[]),
        table_side(&[("t", &["b", "c"])], &[]),
        &[("t", "t")],
        &[(("t", "a1"), "b"), (("t", "a2"), "b"), (("t", "c"), "c")],
    )
}

/// `a(x, gone) -> a(x, y)`: `gone` is dropped and `y` is outside the image.
fn widening_spec() -> scope::TableSpec {
    table_spec(
        table_side(&[("a", &["x", "gone"])], &[]),
        table_side(&[("a", &["x", "y"])], &[]),
        &[("a", "a")],
        &[(("a", "x"), "x")],
    )
}

/// `a(x) -> a(x)`.
fn identity_on_a() -> scope::TableSpec {
    table_spec(
        table_side(&[("a", &["x"])], &[]),
        table_side(&[("a", &["x"])], &[]),
        &[("a", "a")],
        &[(("a", "x"), "x")],
    )
}

const fn int(v: i64) -> rvalue::Value {
    rvalue::Value::Int(v)
}

const fn null(n: u64) -> rvalue::Value {
    rvalue::Value::LabeledNull(n)
}

fn one_table(table: &str, rows: &[(u32, &[(&str, rvalue::Value)])]) -> model::Table {
    model::Table {
        rows: rows
            .iter()
            .map(|(id, cells)| {
                (
                    *id,
                    (
                        table.to_owned(),
                        cells
                            .iter()
                            .map(|(c, v)| ((*c).to_owned(), v.clone()))
                            .collect(),
                    ),
                )
            })
            .collect(),
        pairs: BTreeSet::new(),
    }
}

/// The reference Sigma, Pi and Delta results of [`table_kan_agree`].
type KanResults = (
    Option<Result<(model::Table, rel::SigmaProvenance), rel::KanError>>,
    Option<Result<(model::Table, rel::PiProvenance), rel::KanError>>,
    Option<Result<(model::Table, rel::DeltaProvenance), rel::KanError>>,
);

/// Sigma, Delta and Pi of one input on both sides, outputs, provenance and
/// refusals compared exactly; returns the reference results.
fn table_kan_agree(
    functors: &Functors,
    input: &model::Table,
    as_source: bool,
    bound: usize,
) -> KanResults {
    let (f, fp) = (&functors.reference, &functors.production);
    let production = prod_table(input);
    if as_source {
        let sigma = rel::sigma(f, input);
        match (
            &sigma,
            panproto_inst::f_sigma_with_provenance(&production, fp),
        ) {
            (Ok((out, provenance)), Ok((ours, our_provenance))) => {
                assert_eq!(&ref_table(&ours), out, "Sigma of {input:?}");
                assert_eq!(&ref_sigma_provenance(&our_provenance), provenance);
            }
            (Err(theirs), Err(ours)) => kan_refusals_agree(&ours, theirs, &("Sigma", input)),
            (theirs, ours) => panic!("Sigma of {input:?}: {ours:?} / {theirs:?}"),
        }
        let pi = rel::pi(f, input, bound);
        match (
            &pi,
            panproto_inst::functor_pi_along_with_provenance(&production, fp, bound),
        ) {
            (Ok((out, provenance)), Ok((ours, our_provenance))) => {
                assert_eq!(&ref_table(&ours), out, "Pi of {input:?}");
                assert_eq!(&ref_pi_provenance(&our_provenance), provenance);
            }
            (Err(theirs), Err(ours)) => kan_refusals_agree(&ours, theirs, &("Pi", input)),
            (theirs, ours) => panic!("Pi of {input:?}: {ours:?} / {theirs:?}"),
        }
        (Some(sigma), Some(pi), None)
    } else {
        let delta = rel::delta(f, input);
        match (
            &delta,
            panproto_inst::f_delta_with_provenance(&production, fp),
        ) {
            (Ok((out, provenance)), Ok((ours, our_provenance))) => {
                assert_eq!(&ref_table(&ours), out, "Delta of {input:?}");
                assert_eq!(&ref_delta_provenance(&our_provenance), provenance);
            }
            (Err(theirs), Err(ours)) => kan_refusals_agree(&ours, theirs, &("Delta", input)),
            (theirs, ours) => panic!("Delta of {input:?}: {ours:?} / {theirs:?}"),
        }
        (None, None, Some(delta))
    }
}

/// Hand-built cases with their outputs pinned, each against the reference:
/// a column rename both ways, Delta dropping cells outside the image, a
/// fresh null for a column outside the image, columns equated by
/// union-find, and a clash of constants.
#[test]
fn hand_built_table_kan_cases_agree_with_the_reference() {
    hand_built_renames_and_drops_agree();
    hand_built_fresh_nulls_and_equations_agree();
}

/// The column rename both ways and Delta dropping cells outside the image.
fn hand_built_renames_and_drops_agree() {
    let rename = spec_functors(&rename_spec());
    let (_, _, delta) = table_kan_agree(
        &rename,
        &one_table("post", &[(0, &[("name", int(7))])]),
        false,
        81,
    );
    assert_eq!(
        delta.unwrap().unwrap().0,
        one_table("post", &[(0, &[("title", int(7))])])
    );
    let (sigma, _, _) = table_kan_agree(
        &rename,
        &one_table("post", &[(0, &[("title", int(7))])]),
        true,
        81,
    );
    assert_eq!(
        sigma.unwrap().unwrap().0,
        one_table("post", &[(0, &[("name", int(7))])])
    );

    let dropping = spec_functors(&table_spec(
        table_side(&[("post", &["title"])], &[]),
        table_side(&[("post", &["name", "extra"])], &[]),
        &[("post", "post")],
        &[(("post", "title"), "name")],
    ));
    let (_, _, delta) = table_kan_agree(
        &dropping,
        &one_table("post", &[(0, &[("name", int(1)), ("extra", null(4))])]),
        false,
        81,
    );
    assert_eq!(
        delta.unwrap().unwrap().0,
        one_table("post", &[(0, &[("title", int(1))])])
    );
}

/// A fresh null for a column outside the image, columns equated by
/// union-find, and a clash of constants.
fn hand_built_fresh_nulls_and_equations_agree() {
    let widening = spec_functors(&widening_spec());
    let (sigma, _, _) = table_kan_agree(
        &widening,
        &one_table(
            "a",
            &[
                (0, &[("x", int(1)), ("gone", int(0))]),
                (1, &[("x", null(3)), ("gone", int(0))]),
            ],
        ),
        true,
        81,
    );
    let (out, provenance) = sigma.unwrap().unwrap();
    assert_eq!(
        out,
        one_table(
            "a",
            &[
                (0, &[("x", int(1)), ("y", null(4))]),
                (1, &[("x", null(3)), ("y", null(5))])
            ]
        )
    );
    assert_eq!(
        provenance.fresh,
        BTreeMap::from([(4, (0, "y".to_owned())), (5, (1, "y".to_owned()))])
    );

    let equating = spec_functors(&equating_spec());
    let (sigma, _, _) = table_kan_agree(
        &equating,
        &one_table(
            "t",
            &[
                (0, &[("a1", null(5)), ("a2", int(0)), ("c", null(6))]),
                (1, &[("a1", null(6)), ("a2", null(7)), ("c", null(5))]),
                (2, &[("a1", null(8)), ("a2", null(8)), ("c", int(1))]),
            ],
        ),
        true,
        81,
    );
    let (out, provenance) = sigma.unwrap().unwrap();
    assert_eq!(
        out,
        one_table(
            "t",
            &[
                (0, &[("b", int(0)), ("c", null(6))]),
                (1, &[("b", null(6)), ("c", int(0))]),
                (2, &[("b", null(8)), ("c", int(1))])
            ]
        )
    );
    assert_eq!(
        provenance.merged,
        BTreeMap::from([(5, int(0)), (7, null(6))])
    );
    let (sigma, _, _) = table_kan_agree(
        &equating,
        &one_table(
            "t",
            &[
                (0, &[("a1", null(0)), ("a2", int(1)), ("c", int(9))]),
                (1, &[("a1", null(0)), ("a2", int(2)), ("c", int(9))]),
            ],
        ),
        true,
        81,
    );
    assert!(matches!(
        sigma.unwrap(),
        Err(rel::KanError::SigmaConstantClash { row: 1, ref column, .. }) if column == "b"
    ));
}

/// Fresh null ids against the reference: numbered above every input null,
/// even one only a dropped column holds, and refused at the top of `u64`,
/// exhaustively over the largest input null in `u64::MAX - 3..=u64::MAX` and
/// one to five rows.
#[test]
fn fresh_null_ids_agree_with_the_reference() {
    let widening = spec_functors(&widening_spec());
    let (sigma, _, _) = table_kan_agree(
        &widening,
        &one_table("a", &[(0, &[("x", null(2)), ("gone", null(9))])]),
        true,
        81,
    );
    assert_eq!(
        sigma
            .unwrap()
            .unwrap()
            .1
            .fresh
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [10]
    );
    let (sigma, _, _) = table_kan_agree(
        &widening,
        &one_table("a", &[(0, &[("x", int(2)), ("gone", null(u64::MAX))])]),
        true,
        81,
    );
    assert_eq!(sigma.unwrap(), Err(rel::KanError::NullIdsExhausted));
    let identity = spec_functors(&identity_on_a());
    for max in u64::MAX - 3..=u64::MAX {
        for rows in 1..=5u32 {
            let input = model::Table {
                rows: (0..rows)
                    .map(|r| {
                        let gone = if r == 0 { null(max) } else { int(r.into()) };
                        (
                            r,
                            (
                                "a".to_owned(),
                                BTreeMap::from([
                                    ("x".to_owned(), int(r.into())),
                                    ("gone".to_owned(), gone),
                                ]),
                            ),
                        )
                    })
                    .collect(),
                pairs: BTreeSet::new(),
            };
            let (sigma, _, _) = table_kan_agree(&widening, &input, true, 81);
            assert_eq!(
                sigma.unwrap().is_err(),
                max.checked_add(u64::from(rows)).is_none()
            );
        }
        let (sigma, _, _) = table_kan_agree(
            &identity,
            &one_table("a", &[(0, &[("x", null(max))])]),
            true,
            81,
        );
        assert_eq!(
            sigma.unwrap().unwrap().0,
            one_table("a", &[(0, &[("x", null(max))])])
        );
    }
}

/// The conformance refusals, provenance, float cells by their bits and NaN
/// payloads as distinct values, against the reference.
#[test]
fn table_kan_refusals_and_float_cells_agree_with_the_reference() {
    conformance_refusals_agree();
    merge_provenance_agrees();
    float_cells_agree();
}

/// Every conformance refusal of Sigma, Pi and Delta, with its row, column
/// and reason, and the cells conformance admits.
fn conformance_refusals_agree() {
    let identity = spec_functors(&identity_on_a());
    for (input, row, column, reason) in [
        (
            one_table(
                "a",
                &[
                    (0, &[("x", int(1))]),
                    (1, &[("x", int(1)), ("zz", int(2)), ("aa", int(3))]),
                ],
            ),
            1,
            Some("aa"),
            rel::Conformance::StrayCell,
        ),
        (
            one_table("a", &[(3, &[("x", int(1))]), (4, &[])]),
            4,
            Some("x"),
            rel::Conformance::MissingCell,
        ),
        (
            one_table("b", &[(0, &[])]),
            0,
            None,
            rel::Conformance::TableOutside,
        ),
        (
            one_table(
                "a",
                &[
                    (0, &[("x", null(2))]),
                    (
                        1,
                        &[(
                            "x",
                            rvalue::Value::List(vec![
                                int(1),
                                rvalue::Value::Unknown(BTreeMap::from([("k".to_owned(), null(3))])),
                            ]),
                        )],
                    ),
                ],
            ),
            1,
            Some("x"),
            rel::Conformance::NestedLabeledNull,
        ),
    ] {
        let (sigma, pi, _) = table_kan_agree(&identity, &input, true, 100);
        let (_, _, delta) = table_kan_agree(&identity, &input, false, 100);
        let expected = rel::KanError::NonConforming(Box::new(rel::NonConforming {
            row,
            column: column.map(str::to_owned),
            reason,
        }));
        assert_eq!(sigma.unwrap().unwrap_err(), expected);
        assert_eq!(pi.unwrap().unwrap_err(), expected);
        assert_eq!(delta.unwrap().unwrap_err(), expected);
    }
    let looped = model::Table {
        pairs: BTreeSet::from([(scope::fk_edge("a", "a", "loop"), 0, 0)]),
        ..one_table("a", &[(0, &[("x", int(1))])])
    };
    let (sigma, _, _) = table_kan_agree(&identity, &looped, true, 100);
    assert!(matches!(
        sigma.unwrap(),
        Err(rel::KanError::NonConforming(ref why)) if matches!(why.reason, rel::Conformance::RelationOutside(_))
    ));
    table_kan_agree(
        &identity,
        &one_table("a", &[(4, &[("x", rvalue::Value::Null)])]),
        true,
        100,
    );
    table_kan_agree(
        &identity,
        &one_table("a", &[(0, &[("x", rvalue::Value::List(vec![int(1)]))])]),
        true,
        100,
    );
}

/// Delta's copies along a merge of two tables and Pi of the copies.
fn merge_provenance_agrees() {
    let merge = spec_functors(&table_spec(
        table_side(&[("a", &["x"]), ("b", &["y"])], &[]),
        table_side(&[("m", &["x", "y"])], &[]),
        &[("a", "m"), ("b", "m")],
        &[(("a", "x"), "x"), (("b", "y"), "y")],
    ));
    let (_, _, delta) = table_kan_agree(
        &merge,
        &one_table(
            "m",
            &[
                (0, &[("x", int(1)), ("y", int(2))]),
                (1, &[("x", int(3)), ("y", int(4))]),
            ],
        ),
        false,
        100,
    );
    let (delta, copies) = delta.unwrap().unwrap();
    assert_eq!(
        copies,
        BTreeMap::from([
            (0, ("a".to_owned(), 0)),
            (1, ("a".to_owned(), 1)),
            (2, ("b".to_owned(), 0)),
            (3, ("b".to_owned(), 1))
        ])
    );
    let (_, pi, _) = table_kan_agree(&merge, &delta, true, 100);
    assert_eq!(pi.unwrap().unwrap().0.rows.len(), 4);
}

/// Float cells equal by their bits: zeros of both signs and NaNs of both
/// signs and payloads clash, and a NaN maps only to the same NaN.
fn float_cells_agree() {
    let equating = spec_functors(&equating_spec());
    let float = |bits: u64| rvalue::Value::Float(bits);
    let nan = 0x7ff8_0000_0000_0000;
    for (a, b) in [(0, 1u64 << 63), (nan, nan | (1 << 63)), (nan, nan | 1)] {
        let (sigma, _, _) = table_kan_agree(
            &equating,
            &one_table(
                "t",
                &[(0, &[("a1", float(a)), ("a2", float(b)), ("c", int(0))])],
            ),
            true,
            81,
        );
        assert!(matches!(
            sigma.unwrap(),
            Err(rel::KanError::SigmaConstantClash { .. })
        ));
    }
    let x = one_table(
        "t",
        &[(0, &[("a1", float(nan)), ("a2", float(nan)), ("c", int(0))])],
    );
    table_kan_agree(&equating, &x, true, 81);
    for (b, homs) in [(nan, 1), (nan | (1 << 63), 0)] {
        let j = one_table("t", &[(7, &[("b", float(b)), ("c", int(0))])]);
        let (_, _, delta) = table_kan_agree(&equating, &j, false, 81);
        assert_eq!(rel::homs(&x, &delta.unwrap().unwrap().0).len(), homs);
    }
}

/// Production and reference on one generated case: Sigma, Delta and Pi of
/// its instances as tables, graphs and trees; the restriction to the kept
/// part; the units, counits and both hom-set bijections at `(I, J)` and at
/// `(I, Sigma I)` and `(Delta J, J)`, where the identity makes the hom-sets
/// non-empty; and the refusal of a corrupted instance.
fn generated_case_agrees(seed: u64) {
    let mut case = scope::generated_case(seed);
    let whole = spec_functors(&case.spec);
    let kept = kept_functors(&case.spec);
    let i = rel::restrict_to_kept(&case.x, &whole.reference);
    assert_eq!(
        ref_table(
            &panproto_inst::f_restrict_to_domain(&prod_table(&case.x), &whole.production).unwrap()
        ),
        i,
        "the restriction at seed {seed}"
    );
    table_kan_agree(&whole, &case.x, true, case.max);
    table_kan_agree(&whole, &case.j, false, case.max);
    let family = FamilyCase::from_functors(format!("seed {seed}"), kept);
    check_source_object(&family, &i);
    check_target_object(&family, &case.j);
    let source = pair_source(&family.functors, i, 1);
    let target = pair_target(&family.functors, case.j.clone(), 1);
    let pair_case = PairCase {
        name: "generated",
        functors: family.functors,
        sources: Vec::new(),
        targets: Vec::new(),
    };
    check_pair(&pair_case, &source, &target);
    if let Ok((sigma, _)) = &source.sigma {
        if sigma.rows.len() <= 6 {
            check_pair(
                &pair_case,
                &source,
                &pair_target(&pair_case.functors, sigma.clone(), 1),
            );
        }
    }
    let delta = target.delta.clone();
    if delta.rows.len() <= 6 {
        check_pair(
            &pair_case,
            &pair_source(&pair_case.functors, delta, 1),
            &target,
        );
    }
    let bad = scope::generated_corruption(&mut case.rng, &case.x);
    table_kan_agree(&whole, &bad, true, case.max);
    let bad_j = scope::generated_corruption(&mut case.rng, &case.j);
    table_kan_agree(&whole, &bad_j, false, case.max);
}

/// The seeds whose cases shrank a failure of the generated table laws in
/// the past, run before any random seed.
const REGRESSION_SEEDS: [u64; 5] = [
    251_554_501_479_628_105,
    886_779_333_262_700_799,
    5_467_035_014_510_298_700,
    7_302_257_756_398_127_275,
    7_616_280_956_025_220_712,
];

#[test]
fn generated_regression_seeds_agree_with_the_reference() {
    for seed in REGRESSION_SEEDS {
        generated_case_agrees(seed);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// The randomized differential of the Kan extensions and both hom-set
    /// bijections: a seed names a generated functor (tables merged or
    /// dropped, relations carried or dropped, columns merged, dropped or
    /// missed) with an instance on its source (at most 3 rows per table and
    /// 6 in all, integer, float-bit and labelled-null cells, partial and
    /// many-valued relations), one on its target and a product bound from
    /// {4, 81}; [`generated_case_agrees`] compares everything production
    /// and the reference compute on it.
    #[test]
    fn randomized_kan_hom_bijection_differential(seed in any::<u64>()) {
        generated_case_agrees(seed);
    }

    /// Monotonicity and functoriality on generated cases, production
    /// against the reference: Sigma, Delta and Pi of an instance and of a
    /// larger one it lies within, and Sigma, Delta and Pi of two composable
    /// homomorphisms that fill nulls and add rows, with their composite.
    #[test]
    fn randomized_kan_functoriality_differential(seed in any::<u64>()) {
        let mut case = scope::generated_case(seed);
        let kept = kept_functors(&case.spec);
        let (f, fp) = (&kept.reference, &kept.production);
        let kept_side = case.spec.kept();
        let i = rel::restrict_to_kept(&case.x, &spec_functors(&case.spec).reference);
        let big = scope::generated_growth(&mut case.rng, &kept_side, &i);
        table_kan_agree(&kept, &big, true, case.max);
        let big_j = scope::generated_growth(&mut case.rng, &case.spec.tgt, &case.j);
        table_kan_agree(&kept, &big_j, false, case.max);
        let (b, h1) = scope::generated_hom_out(&mut case.rng, &kept_side, &i);
        let (c, h2) = scope::generated_hom_out(&mut case.rng, &kept_side, &b);
        let h12 = h1.compose(&h2).unwrap();
        let (pi, pb, pc) = (prod_table(&i), prod_table(&b), prod_table(&c));
        for (domain, codomain, hom, pd, pcod) in [(&i, &b, &h1, &pi, &pb), (&b, &c, &h2, &pb, &pc), (&i, &c, &h12, &pi, &pc)] {
            let ours = panproto_inst::f_sigma_on_hom(&prod_hom(hom), pd, pcod, fp);
            let theirs = rel::sigma_on_hom(f, domain, codomain, hom);
            prop_assert_eq!(ours.is_ok(), theirs.is_some());
            if let (Ok(ours), Some(theirs)) = (&ours, &theirs) {
                let (sigma, _) = rel::sigma_restricted(f, domain).unwrap();
                prop_assert!(ref_hom(ours).same_as(theirs, &sigma));
            }
            let ours = panproto_inst::f_pi_on_hom(&prod_hom(hom), pd, pcod, fp, case.max);
            let theirs = rel::pi_on_hom(f, domain, codomain, hom, case.max);
            prop_assert_eq!(ours.is_ok(), theirs.is_some());
            if let (Ok(ours), Some(theirs)) = (&ours, &theirs) {
                let (pi_out, _) = rel::pi_restricted(f, domain, case.max).unwrap();
                prop_assert!(ref_hom(ours).same_as(theirs, &pi_out));
            }
        }
        let (tb, t1) = scope::generated_hom_out(&mut case.rng, &case.spec.tgt, &case.j);
        let ours = panproto_inst::f_delta_on_hom(&prod_hom(&t1), &prod_table(&case.j), &prod_table(&tb), fp);
        let theirs = rel::delta_on_hom(f, &case.j, &tb, &t1);
        prop_assert_eq!(ours.is_ok(), theirs.is_some());
        if let (Ok(ours), Some(theirs)) = (&ours, &theirs) {
            let (delta, _) = rel::delta(f, &case.j).unwrap();
            prop_assert!(ref_hom(ours).same_as(theirs, &delta));
        }
    }
}

/// Pi at one target table on the 3000 generated join cases (fibres of one
/// to four tables over the columns `p, q, r, s`, connected and
/// disconnected, acyclic and cyclic, bounds 1, 4 and 81) against the
/// reference: tuples, cells and provenance within the bound, the exact
/// product over it.
#[test]
fn pi_joins_agree_with_the_reference() {
    let cases = scope::pi_join_cases();
    assert_eq!(cases.len(), 3000);
    let mut census = Census::default();
    for case in &cases {
        let functors = spec_functors(&case.spec);
        let (_, pi, _) = table_kan_agree(&functors, &case.instance, true, case.max);
        match pi.unwrap() {
            Ok(_) => census.add("within", 1),
            Err(rel::KanError::ProductSizeExceeded { .. }) => census.add("over", 1),
            Err(other) => panic!("{other:?}"),
        }
        if case.cyclic {
            census.add("cyclic", 1);
        }
    }
    println!("Pi joins: 3000 cases; {:?}", census.0);
    assert!(census.get("within") > 0 && census.get("over") > 0 && census.get("cyclic") == 750);
}

// ---------------------------------------------------------------------------
// Classification and the entity functor.
// ---------------------------------------------------------------------------

/// One classification on both sides: equal tables, relations, columns,
/// value vertices with their kinds, and inner edges, or the same refusal.
fn classifications_agree(
    schema: &panproto_reference::schema::Schema,
    (value, non_value): &(BTreeSet<String>, BTreeSet<String>),
) -> &'static str {
    let ours = RelSchema::new(&prod_schema(schema), &protocol_of(value, non_value));
    let theirs = rel::classify(schema, value, non_value);
    match (&ours, &theirs) {
        (Ok(ours), Ok(theirs)) => {
            let tables: BTreeSet<String> = ours.tables().iter().map(ToString::to_string).collect();
            assert_eq!(tables, theirs.tables, "{schema:?}");
            let relations: BTreeSet<model::SEdge> = ours.relations().iter().map(ref_edge).collect();
            assert_eq!(relations, theirs.relations, "{schema:?}");
            let columns: BTreeMap<String, BTreeMap<String, model::SEdge>> = ours
                .all_columns()
                .iter()
                .map(|(t, cs)| {
                    (
                        t.to_string(),
                        cs.iter().map(|(c, e)| (c.clone(), ref_edge(e))).collect(),
                    )
                })
                .collect();
            let expected: BTreeMap<String, BTreeMap<String, model::SEdge>> = theirs
                .columns
                .iter()
                .map(|(t, cs)| (t.clone(), cs.clone()))
                .collect();
            for table in &theirs.tables {
                assert_eq!(
                    columns.get(table).cloned().unwrap_or_default(),
                    expected.get(table).cloned().unwrap_or_default(),
                    "{schema:?}"
                );
            }
            for (vertex, kind) in &schema.vertices {
                let value_kind = ours.value_kind(vertex).map(ToString::to_string);
                assert_eq!(
                    value_kind,
                    theirs.value_vertices.contains(vertex).then(|| kind.clone()),
                    "{schema:?}: {vertex}"
                );
            }
            let inner: BTreeSet<model::SEdge> = ours.inner_edges().iter().map(ref_edge).collect();
            assert_eq!(inner, theirs.inner_edges, "{schema:?}");
            "classified"
        }
        (Err(ours), Err(theirs)) => {
            assert_eq!(
                ref_rel_schema_error(ours).as_ref(),
                Some(theirs),
                "{schema:?}"
            );
            match theirs {
                rel::RelSchemaError::AmbiguousKind { .. } => "AmbiguousKind",
                rel::RelSchemaError::UnclassifiedKind { .. } => "UnclassifiedKind",
                rel::RelSchemaError::ValueReachesTable { .. } => "ValueReachesTable",
                rel::RelSchemaError::DuplicateColumn { .. } => "DuplicateColumn",
            }
        }
        (ours, theirs) => panic!("{schema:?}: production {ours:?}, reference {theirs:?}"),
    }
}

/// Two column names repeated in one table, the larger met first in edge
/// order: both sides refuse with the least `(table, column)`, here `(t, a)`.
#[test]
fn classification_reports_the_least_duplicate_column() {
    let schema = panproto_reference::schema::Schema::default()
        .vertex("t", "t")
        .vertex("v1", "v")
        .vertex("v2", "v")
        .vertex("v3", "v")
        .vertex("v4", "v")
        .edge("t", "v1", "prop", Some("z"))
        .edge("t", "v2", "prop", Some("a"))
        .edge("t", "v3", "prop", Some("z"))
        .edge("t", "v4", "prop", Some("a"));
    let partition = (kinds(&["v"]), kinds(&["t"]));
    assert_eq!(
        classifications_agree(&schema, &partition),
        "DuplicateColumn"
    );
    assert_eq!(
        rel::classify(&schema, &partition.0, &partition.1),
        Err(rel::RelSchemaError::DuplicateColumn {
            table: "t".to_owned(),
            column: "a".to_owned(),
        })
    );
}

/// Strict classification of every schema of the classification scope
/// (three vertices of four kinds, at most two edges, every set of declared
/// tables) under both kind partitions, against the reference.
#[test]
fn classification_agrees_with_the_reference() {
    let schemas = scope::classification_schemas();
    assert_eq!(schemas.len() as u64, scope::classification_schemas_count());
    let mut census = Census::default();
    for partition in scope::classification_partitions() {
        census += par_sum(&schemas, |schema| {
            Census::one(classifications_agree(schema, &partition))
        });
    }
    println!(
        "classification: {} schemas (independent count {}) x 2 kind partitions; {:?}",
        schemas.len(),
        scope::classification_schemas_count(),
        census.0
    );
    for outcome in [
        "classified",
        "AmbiguousKind",
        "UnclassifiedKind",
        "ValueReachesTable",
        "DuplicateColumn",
    ] {
        assert!(census.get(outcome) > 0, "{outcome} is not reached");
    }
}

/// One entity functor on both sides: equal table, relation and column maps
/// and recorded inner-edge and value-vertex remaps, or the same refusal;
/// then the value-structure check on both sides.
fn entity_functors_agree(
    src: &panproto_reference::schema::Schema,
    tgt: &panproto_reference::schema::Schema,
    migration: &panproto_reference::migration::Migration,
) -> String {
    let (value, non_value) = family_kinds();
    let protocol = protocol_of(&value, &non_value);
    let source = rel::classify(src, &value, &non_value).unwrap();
    let target = rel::classify(tgt, &value, &non_value).unwrap();
    let theirs = rel::entity_functor(&source, &target, migration);
    let ours = MigrationFunctor::entity_part(
        &prod_schema(src),
        &protocol,
        &prod_schema(tgt),
        &protocol,
        &prod_migration(migration),
    );
    let check_maps = |ours: &EntityFunctor, theirs: &rel::RelFunctor| {
        let tables: BTreeMap<String, String> = ours
            .table_map()
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        assert_eq!(tables, theirs.tables, "{migration:?}");
        let relations: BTreeMap<model::SEdge, model::SEdge> = ours
            .relation_map()
            .iter()
            .map(|(a, b)| (ref_edge(a), ref_edge(b)))
            .collect();
        assert_eq!(relations, theirs.relations, "{migration:?}");
        let columns: BTreeMap<(String, String), String> = ours
            .column_map()
            .iter()
            .map(|((t, c), b)| ((t.to_string(), c.clone()), b.clone()))
            .collect();
        assert_eq!(columns, theirs.columns, "{migration:?}");
    };
    match (ours, theirs) {
        (Ok(ours), Ok(theirs)) => {
            check_maps(&ours, theirs.table_part());
            let inner: BTreeMap<model::SEdge, model::SEdge> = ours
                .inner_edges()
                .iter()
                .map(|(a, b)| (ref_edge(a), ref_edge(b)))
                .collect();
            assert_eq!(inner, theirs.inner_edges, "{migration:?}");
            match (ours.into_kan(), theirs.into_kan()) {
                (Ok(ours), Ok(theirs)) => {
                    let values: BTreeMap<String, String> = ours
                        .value_vertices()
                        .iter()
                        .map(|(a, b)| (a.to_string(), b.to_string()))
                        .collect();
                    assert_eq!(values, theirs.value_vertices, "{migration:?}");
                    "kept".to_owned()
                }
                (Err(ours), Err(theirs)) => {
                    assert_eq!(
                        ref_functor_error(&ours).as_ref(),
                        Some(&theirs),
                        "{migration:?}"
                    );
                    "ValueStructureRemap".to_owned()
                }
                (ours, theirs) => {
                    panic!("{migration:?}: production {ours:?}, reference {theirs:?}")
                }
            }
        }
        (Err(ours), Err(theirs)) => {
            assert_eq!(
                ref_functor_error(&ours).as_ref(),
                Some(&theirs),
                "{migration:?}"
            );
            format!("{theirs:?}")
                .split(['{', ' ', '('])
                .next()
                .unwrap_or_default()
                .to_owned()
        }
        (ours, theirs) => panic!("{migration:?}: production {ours:?}, reference {theirs:?}"),
    }
}

/// The entity functor of every migration of the entity scope between every
/// ordered pair of its schemas, against the reference.
#[test]
fn entity_functor_agrees_with_the_reference() {
    let schemas = scope::entity_schemas();
    let mut census = Census::default();
    let mut migrations = 0u64;
    for src in &schemas {
        for tgt in &schemas {
            let all = scope::entity_migrations(src, tgt);
            assert_eq!(all.len() as u64, scope::entity_migrations_count(src, tgt));
            migrations += all.len() as u64;
            census += par_sum(&all, |m| Census::one(entity_functors_agree(src, tgt, m)));
        }
    }
    println!(
        "entity functor: {} schemas, {} ordered pairs, {migrations} migrations; {:?}",
        schemas.len(),
        schemas.len() * schemas.len(),
        census.0
    );
    for outcome in [
        "kept",
        "ValueStructureRemap",
        "VertexImageNotATable",
        "IncoherentEdgeImage",
        "IncoherentAttributeImage",
    ] {
        assert!(census.get(outcome) > 0, "{outcome} is not reached");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Classification and the entity functor on generated schemas and
    /// migrations: the generated specs' sides, their kept parts and every
    /// vertex kind swapped to `value`, and the specs' migrations with one
    /// entry dropped or redirected.
    #[test]
    fn randomized_classification_and_entity_functor_differential(seed in any::<u64>()) {
        let mut rng = scope::SplitMix(seed);
        let spec = scope::generated_spec(&mut rng);
        let (value, non_value) = table_spec_kinds();
        let partition = (value, non_value);
        let mut schema = spec.src.schema();
        let vertices: Vec<String> = schema.vertices.keys().cloned().collect();
        if let Some(vertex) = rng.pick(&vertices).cloned() {
            let kinds = ["table", "integer", "other"];
            let kind = rng.pick(&kinds).unwrap();
            schema.vertices.insert(vertex, (*kind).to_owned());
        }
        classifications_agree(&schema, &partition);
        classifications_agree(&spec.tgt.schema(), &partition);
        let mut migration = spec.migration();
        let keys: Vec<String> = migration.vertex_remap.keys().cloned().collect();
        if let Some(key) = rng.pick(&keys).cloned() {
            if rng.chance(50) {
                migration.vertex_remap.remove(&key);
            } else {
                let targets: Vec<String> = spec.tgt.schema().vertices.keys().cloned().collect();
                if let Some(target) = rng.pick(&targets).cloned() {
                    migration.vertex_remap.insert(key, target);
                }
            }
        }
        let src = spec.src.schema();
        let tgt = spec.tgt.schema();
        let source = rel::classify(&src, &partition.0, &partition.1).unwrap();
        let target = rel::classify(&tgt, &partition.0, &partition.1).unwrap();
        let theirs = rel::entity_functor(&source, &target, &migration).and_then(rel::EntityFunctor::into_kan);
        let protocol = protocol_of(&partition.0, &partition.1);
        let ours = MigrationFunctor::new(&prod_schema(&src), &protocol, &prod_schema(&tgt), &protocol, &prod_migration(&migration));
        match (ours, theirs) {
            (Ok(ours), Ok(theirs)) => {
                let tables: BTreeMap<String, String> = ours.table_map().iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
                prop_assert_eq!(tables, theirs.tables);
                let columns: BTreeMap<(String, String), String> = ours.column_map().iter().map(|((t, c), b)| ((t.to_string(), c.clone()), b.clone())).collect();
                prop_assert_eq!(columns, theirs.columns);
                let relations: BTreeMap<model::SEdge, model::SEdge> = ours.relation_map().iter().map(|(a, b)| (ref_edge(a), ref_edge(b))).collect();
                prop_assert_eq!(relations, theirs.relations);
            }
            (Err(ours), Err(theirs)) => prop_assert_eq!(ref_functor_error(&ours), Some(theirs)),
            (ours, theirs) => prop_assert!(false, "production {:?}, reference {:?}", ours, theirs),
        }
    }
}

/// Tree Pi refuses a merging map at the first pair of source vertices that
/// share an image in ascending source order, as tree Delta does: with `a`
/// and `c` sent to `t2` and `b` and `d` to `t1`, the pair is `(a, c)` at
/// `t2`, on both sides.
#[test]
fn tree_pi_reports_the_first_merging_pair_in_source_order() {
    let source = panproto_reference::schema::Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("c", "record")
        .vertex("d", "record");
    let target = panproto_reference::schema::Schema::default()
        .vertex("t1", "record")
        .vertex("t2", "record");
    let mut migration = panproto_reference::migration::Migration::default();
    for (s, t) in [("a", "t2"), ("b", "t1"), ("c", "t2"), ("d", "t1")] {
        migration.vertex_remap.insert(s.to_owned(), t.to_owned());
    }
    let functors = Functors::new(&source, &target, &migration, &family_kinds());
    let tree = model::Tree {
        nodes: BTreeMap::from([(0, model::Node::new(0, "a"))]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    let expected = encode::t_pi(&functors.reference, &tree, 4);
    assert_eq!(
        expected,
        Err(encode::TreeKanError::NonInjectiveVertexMap {
            first: "a".to_owned(),
            second: "c".to_owned(),
            target: "t2".to_owned(),
        })
    );
    assert_eq!(
        check_tree_route(&functors, TreeRoute::Pi(4), &tree, &expected, false),
        "NonInjectiveVertexMap"
    );
}

/// The first tree-Pi collision is the first one encountered while scanning
/// source vertices, not the lexicographically least colliding pair: for the
/// crossing pairs `(a, z)` and `(b, c)`, the scan reaches `c` before `z` and
/// must report `(b, c)` at `t2`.
#[test]
fn tree_pi_orders_crossing_collisions_by_the_second_source() {
    let source = panproto_reference::schema::Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("c", "record")
        .vertex("z", "record");
    let target = panproto_reference::schema::Schema::default()
        .vertex("t1", "record")
        .vertex("t2", "record");
    let mut migration = panproto_reference::migration::Migration::default();
    for (s, t) in [("a", "t1"), ("b", "t2"), ("c", "t2"), ("z", "t1")] {
        migration.vertex_remap.insert(s.to_owned(), t.to_owned());
    }
    let functors = Functors::new(&source, &target, &migration, &family_kinds());
    let tree = model::Tree {
        nodes: BTreeMap::from([(0, model::Node::new(0, "a"))]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    let expected = encode::t_pi(&functors.reference, &tree, 4);
    assert_eq!(
        expected,
        Err(encode::TreeKanError::NonInjectiveVertexMap {
            first: "b".to_owned(),
            second: "c".to_owned(),
            target: "t2".to_owned(),
        })
    );
    assert_eq!(
        check_tree_route(&functors, TreeRoute::Pi(4), &tree, &expected, false),
        "NonInjectiveVertexMap"
    );
}

/// The Delta -| Pi transposes need Pi of the source instance only: along
/// the merge of `a{x}` and `b{x}` into `m{x}`, with `J` three `m` rows (so
/// `Pi(Delta J)` is a product of 9, over the bound 4) and `I` one row in each
/// of `a` and `b` (a product of 1), both transposes of the one homomorphism
/// `Delta J -> I` are defined, on both sides.
#[test]
fn pi_transposes_do_not_need_pi_of_the_delta() {
    let case = scope::family()
        .into_iter()
        .find(|case| case.name == "merge-shared-column")
        .unwrap();
    let functors = Functors::new(
        &case.source_schema(),
        &case.target_schema(),
        &case.migration(),
        &family_kinds(),
    );
    let (f, fp) = (&functors.reference, &functors.production);
    let j = one_table(
        "m",
        &[
            (0, &[("x", int(0))]),
            (1, &[("x", int(0))]),
            (2, &[("x", int(0))]),
        ],
    );
    let i = model::Table {
        rows: BTreeMap::from([
            (
                0,
                ("a".to_owned(), BTreeMap::from([("x".to_owned(), int(0))])),
            ),
            (
                1,
                ("b".to_owned(), BTreeMap::from([("x".to_owned(), int(0))])),
            ),
        ]),
        pairs: BTreeSet::new(),
    };
    let (delta, _) = rel::delta(f, &j).unwrap();
    assert!(rel::pi(f, &delta, 4).is_err());
    let homs = rel::homs(&delta, &i);
    assert_eq!(homs.len(), 1);
    let theirs = rel::pi_transpose_forward(f, &j, &i, &homs[0], 4).unwrap();
    let ours = panproto_inst::f_pi_transpose_left(
        &prod_hom(&homs[0]),
        &prod_table(&j),
        &prod_table(&i),
        fp,
        4,
    )
    .unwrap_or_else(|e| panic!("the production transpose refuses: {e}"));
    assert!(ref_hom(&ours).same_as(&theirs, &j));
    let back = panproto_inst::f_pi_transpose_right(&ours, &prod_table(&j), &prod_table(&i), fp, 4)
        .unwrap();
    assert!(ref_hom(&back).same_as(&homs[0], &delta));
}

/// The forward Pi transpose validates its supplied map after Delta and
/// before constructing bounded `Pi(I)`: an invalid map is the first error
/// even when `Pi(I)` has product size 9 at bound 4.
#[test]
fn pi_transpose_checks_the_map_before_the_product_bound() {
    let case = scope::family()
        .into_iter()
        .find(|case| case.name == "merge-shared-column")
        .unwrap();
    let functors = Functors::new(
        &case.source_schema(),
        &case.target_schema(),
        &case.migration(),
        &family_kinds(),
    );
    let (f, fp) = (&functors.reference, &functors.production);
    let j = one_table("m", &[(0, &[("x", int(0))])]);
    let i = model::Table {
        rows: BTreeMap::from([
            (
                0,
                ("a".to_owned(), BTreeMap::from([("x".to_owned(), int(0))])),
            ),
            (
                1,
                ("a".to_owned(), BTreeMap::from([("x".to_owned(), int(0))])),
            ),
            (
                2,
                ("a".to_owned(), BTreeMap::from([("x".to_owned(), int(0))])),
            ),
            (
                3,
                ("b".to_owned(), BTreeMap::from([("x".to_owned(), int(0))])),
            ),
            (
                4,
                ("b".to_owned(), BTreeMap::from([("x".to_owned(), int(0))])),
            ),
            (
                5,
                ("b".to_owned(), BTreeMap::from([("x".to_owned(), int(0))])),
            ),
        ]),
        pairs: BTreeSet::new(),
    };
    assert!(matches!(
        rel::pi(f, &i, 4),
        Err(rel::KanError::ProductSizeExceeded {
            actual: 9,
            limit: 4,
            ..
        })
    ));
    let (delta, _) = rel::delta(f, &j).unwrap();
    let first = *delta.rows.keys().next().unwrap();
    let invalid = rel::Hom::default();
    assert_eq!(
        rel::pi_transpose_forward(f, &j, &i, &invalid, 4),
        Err(rel::TransposeError::Hom(Box::new(
            rel::HomError::NodeNotMapped(first)
        )))
    );
    assert!(matches!(
        panproto_inst::f_pi_transpose_left(
            &prod_hom(&invalid),
            &prod_table(&j),
            &prod_table(&i),
            fp,
            4,
        ),
        Err(AdjunctionError::NotANaturalMap { row }) if row == RowId(first)
    ));
}
