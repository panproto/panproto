#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! Differentials for instance version control: stored payloads of every
//! shape against the reference model of instances, garbage collection
//! against the reference reachability of
//! `panproto_reference::pages::instance_version_control`, and a repository
//! written before payload encodings were recorded.
//!
//! The adapters below translate between the reference model and production
//! values; every operation compared is computed independently on each side.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use panproto_inst::{
    Complement, EdgeId, FInstance, Fan, FieldPresence, GEdge, GInstance, GraphComplement,
    GraphDelta, Node, NodeShape, RowId, TableComplement, TableDelta, TreeEdit, Value, WInstance,
};
use panproto_reference::enumerate::edits as carrier;
use panproto_reference::enumerate::instance_version_control::{
    GC_CASES_COUNT, GC_KEPT_TOTAL, Roots, gc_cases,
};
use panproto_reference::enumerate::instance_version_control::{encodings, payload_facts};
use panproto_reference::model as reference_model;
use panproto_reference::pages::instance_version_control::{
    Head, Object as ModelObject, PayloadFacts, PayloadOutcome, Repository as ModelRepository,
    Shape, payload_outcome, recorded_encoding, writes_encoding_field,
};
use panproto_reference::value::{Presence as ReferencePresence, Value as ReferenceValue};
use panproto_schema::Edge;
use panproto_vcs::gc::{GcOptions, gc_with_roots};
use panproto_vcs::object::{
    CommitObject, ComplementObject, DataSetObject, EditLogObject, InstanceShape, Object,
    PayloadEncoding, SchemaTreeObject, TagObject,
};
use panproto_vcs::store::{HeadState, ReflogEntry, Store};
use panproto_vcs::{
    ComplementPayload, EditLogPayload, Instance, MemStore, ObjectId, Repository, VcsError,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Adapters between the reference model and production values.
// ---------------------------------------------------------------------------

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
        other => panic!("value outside the coverage scope: {other:?}"),
    }
}

fn value_to_reference(value: &Value) -> ReferenceValue {
    match value {
        Value::Bool(value) => ReferenceValue::Bool(*value),
        Value::Int(value) => ReferenceValue::Int(*value),
        other => panic!("production returned a value outside the coverage scope: {other:?}"),
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

/// The production graph, nodes and edges inserted in ascending order, or in
/// descending order when `reversed`.
fn graph_to_production(graph: &reference_model::Graph, reversed: bool) -> GInstance {
    let mut nodes: Vec<&reference_model::Node> = graph.nodes.values().collect();
    let mut edges: Vec<(&u32, &reference_model::GEdge)> = graph.edges.iter().collect();
    if reversed {
        nodes.reverse();
        edges.reverse();
    }
    let mut out = GInstance::new();
    for node in nodes {
        out = out.with_node(node_to_production(node));
    }
    for (id, edge) in edges {
        out.insert_edge(EdgeId(*id), graph_edge_to_production(edge))
            .expect("a coverage graph is well formed");
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

/// The production table, rows and pairs inserted in ascending order, or in
/// descending order when `reversed`.
fn table_to_production(table: &reference_model::Table, reversed: bool) -> FInstance {
    let mut rows: Vec<_> = table.rows.iter().collect();
    let mut pairs: Vec<_> = table.pairs.iter().collect();
    if reversed {
        rows.reverse();
        pairs.reverse();
    }
    let mut out = FInstance::new();
    for (id, (name, cells)) in rows {
        let row: HashMap<String, Value> = cells
            .iter()
            .map(|(column, value)| (column.clone(), value_to_production(value)))
            .collect();
        out.insert_row(name.as_str(), RowId(*id), row)
            .expect("a coverage table has distinct row ids");
    }
    for (edge, src, tgt) in pairs {
        out.insert_foreign_key(edge_to_production(edge), RowId(*src), RowId(*tgt))
            .expect("a coverage table is well formed");
    }
    out
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

// ---------------------------------------------------------------------------
// Stored payloads.
// ---------------------------------------------------------------------------

/// A schema identifier per coverage schema, so that objects of different
/// schemas are distinct.
fn schema_id(index: usize) -> ObjectId {
    ObjectId::from_bytes([u8::try_from(index).unwrap(); 32])
}

fn stored_instance(store: &mut MemStore, dataset: DataSetObject) -> (ObjectId, Instance) {
    let id = store.put(&Object::DataSet(dataset)).unwrap();
    let Object::DataSet(read) = store.get(&id).unwrap() else {
        panic!("a data set reads back as a data set");
    };
    (id, read.decode().unwrap())
}

#[test]
fn every_instance_of_the_coverage_scope_round_trips_through_its_stored_object() {
    let mut store = MemStore::new();
    let mut ids = BTreeSet::new();
    let (mut graphs, mut tables, mut trees) = (0usize, 0usize, 0usize);
    for (index, schema) in carrier::coverage_schemas().iter().enumerate() {
        let id = schema_id(index);
        for graph in carrier::coverage_graphs(schema) {
            let forward = graph_to_production(&graph, false);
            let backward = graph_to_production(&graph, true);
            let dataset =
                DataSetObject::from_instance(id, &Instance::Graph(forward), None).unwrap();
            assert_eq!(dataset.encoding, PayloadEncoding::GRAPH);
            let (stored, decoded) = stored_instance(&mut store, dataset);
            let Instance::Graph(decoded) = decoded else {
                panic!("a graph data set decodes to a graph");
            };
            assert_eq!(graph_to_reference(&decoded), graph);
            let again = DataSetObject::from_instance(id, &Instance::Graph(backward), None).unwrap();
            assert_eq!(store.put(&Object::DataSet(again)).unwrap(), stored);
            assert!(ids.insert(stored));
            graphs += 1;
        }
        for table in carrier::coverage_tables(schema) {
            let forward = table_to_production(&table, false);
            let backward = table_to_production(&table, true);
            let dataset =
                DataSetObject::from_instance(id, &Instance::Table(forward), None).unwrap();
            assert_eq!(dataset.encoding, PayloadEncoding::TABLE);
            let (stored, decoded) = stored_instance(&mut store, dataset);
            let Instance::Table(decoded) = decoded else {
                panic!("a table data set decodes to a table");
            };
            assert_eq!(table_to_reference(&decoded), table);
            let again = DataSetObject::from_instance(id, &Instance::Table(backward), None).unwrap();
            assert_eq!(store.put(&Object::DataSet(again)).unwrap(), stored);
            assert!(ids.insert(stored));
            tables += 1;
        }
        for tree in carrier::coverage_trees(schema) {
            let records = vec![tree_to_production(&tree), tree_to_production(&tree)];
            let dataset = DataSetObject::from_instance(
                id,
                &Instance::Tree(records),
                Some(format!("{index}")),
            )
            .unwrap();
            assert_eq!(dataset.encoding, PayloadEncoding::TREE);
            assert_eq!(dataset.record_count, 2);
            let (stored, decoded) = stored_instance(&mut store, dataset);
            let Instance::Tree(decoded) = decoded else {
                panic!("a tree data set decodes to trees");
            };
            assert_eq!(decoded.len(), 2);
            for record in &decoded {
                assert_eq!(tree_to_reference(record), tree);
            }
            assert!(ids.insert(stored));
            trees += 1;
        }
    }
    assert_eq!((graphs, tables, trees), (61_066, 32_038, 3_156));
    assert_eq!(ids.len(), 61_066 + 32_038 + 3_156);
}

#[test]
fn every_edit_log_of_consecutive_coverage_states_round_trips_and_applies() {
    let mut store = MemStore::new();
    let (mut graph_logs, mut table_logs) = (0usize, 0usize);
    for (index, schema) in carrier::coverage_schemas().iter().enumerate() {
        let graphs = carrier::coverage_graphs(schema);
        for pair in graphs.windows(2) {
            let (before, after) = (
                graph_to_production(&pair[0], false),
                graph_to_production(&pair[1], false),
            );
            let delta = GraphDelta::diff(&before, &after);
            let log = EditLogObject::from_payload(
                schema_id(index),
                ObjectId::ZERO,
                &EditLogPayload::Graph(vec![delta.clone(), GraphDelta::default()]),
                ObjectId::ZERO,
            )
            .unwrap();
            let id = store.put(&Object::EditLog(log)).unwrap();
            let Object::EditLog(read) = store.get(&id).unwrap() else {
                panic!("an edit log reads back as an edit log");
            };
            let EditLogPayload::Graph(deltas) = read.decode().unwrap() else {
                panic!("a graph edit log decodes to graph deltas");
            };
            assert_eq!(deltas.len(), 2);
            let mut applied = before.clone();
            deltas[0].apply(&mut applied).unwrap();
            assert_eq!(graph_to_reference(&applied), pair[1]);
            graph_logs += 1;
        }
        let tables = carrier::coverage_tables(schema);
        for pair in tables.windows(2) {
            let (before, after) = (
                table_to_production(&pair[0], false),
                table_to_production(&pair[1], false),
            );
            let delta = TableDelta::diff(&before, &after);
            let log = EditLogObject::from_payload(
                schema_id(index),
                ObjectId::ZERO,
                &EditLogPayload::Table(vec![delta]),
                ObjectId::ZERO,
            )
            .unwrap();
            let id = store.put(&Object::EditLog(log)).unwrap();
            let Object::EditLog(read) = store.get(&id).unwrap() else {
                panic!("an edit log reads back as an edit log");
            };
            let EditLogPayload::Table(deltas) = read.decode().unwrap() else {
                panic!("a table edit log decodes to table deltas");
            };
            let mut applied = before.clone();
            deltas[0].apply(&mut applied).unwrap();
            assert_eq!(table_to_reference(&applied), pair[1]);
            table_logs += 1;
        }
    }
    // Consecutive pairs: one fewer than the states, per schema.
    assert_eq!((graph_logs, table_logs), (61_066 - 16, 32_038 - 16));
}

/// Store `complement` and read it back.
fn stored_complement(store: &mut MemStore, complement: &ComplementPayload) -> ComplementPayload {
    let object =
        ComplementObject::from_payload(ObjectId::ZERO, ObjectId::ZERO, complement).unwrap();
    assert_eq!(
        object.encoding,
        PayloadEncoding::current(complement.shape())
    );
    let id = store.put(&Object::Complement(object)).unwrap();
    let Object::Complement(read) = store.get(&id).unwrap() else {
        panic!("a complement reads back as a complement");
    };
    read.decode().unwrap()
}

fn graph_complement_round_trips(store: &mut MemStore, graph: &reference_model::Graph) {
    let production = graph_to_production(graph, false);
    let mut complement = GraphComplement::empty();
    complement.dropped_nodes = production
        .nodes()
        .iter()
        .map(|(id, node)| (*id, node.clone()))
        .collect();
    complement.dropped_edges = production
        .edges_sorted()
        .map(|(id, edge)| (id, edge.clone()))
        .collect();
    let bytes = rmp_serde::to_vec(&complement).unwrap();
    let ComplementPayload::Graph(decoded) =
        stored_complement(store, &ComplementPayload::Graph(Box::new(complement)))
    else {
        panic!("a graph complement decodes to a graph complement");
    };
    let nodes: BTreeMap<u32, reference_model::Node> = decoded
        .dropped_nodes
        .iter()
        .map(|(id, node)| (*id, node_to_reference(node)))
        .collect();
    assert_eq!(nodes, graph.nodes);
    assert_eq!(rmp_serde::to_vec(decoded.as_ref()).unwrap(), bytes);
}

fn table_complement_round_trips(store: &mut MemStore, table: &reference_model::Table) {
    let mut complement = TableComplement::empty();
    for (id, (name, cells)) in &table.rows {
        complement
            .original_tables
            .insert(RowId(*id), name.as_str().into());
        for (column, value) in cells {
            complement
                .dropped_cells
                .insert((RowId(*id), column.clone()), value_to_production(value));
        }
    }
    for (edge, src, tgt) in &table.pairs {
        complement
            .dropped_fk_pairs
            .entry(edge_to_production(edge))
            .or_default()
            .insert((RowId(*src), RowId(*tgt)));
    }
    let ComplementPayload::Table(decoded) = stored_complement(
        store,
        &ComplementPayload::Table(Box::new(complement.clone())),
    ) else {
        panic!("a table complement decodes to a table complement");
    };
    assert_eq!(*decoded, complement);
}

fn tree_complement_round_trips(store: &mut MemStore, tree: &reference_model::Tree) {
    let production = tree_to_production(tree);
    let complement = Complement {
        dropped_nodes: production.nodes.clone(),
        dropped_arcs: production.arcs,
        ..Complement::default()
    };
    let ComplementPayload::Tree(decoded) =
        stored_complement(store, &ComplementPayload::Tree(vec![complement]))
    else {
        panic!("a tree complement decodes to tree complements");
    };
    let nodes: BTreeMap<u32, reference_model::Node> = decoded[0]
        .dropped_nodes
        .iter()
        .map(|(id, node)| (*id, node_to_reference(node)))
        .collect();
    assert_eq!(nodes, tree.nodes);
}

#[test]
fn every_complement_of_the_coverage_states_round_trips() {
    let mut store = MemStore::new();
    let mut count = 0usize;
    for schema in carrier::coverage_schemas() {
        for graph in carrier::coverage_graphs(&schema) {
            graph_complement_round_trips(&mut store, &graph);
            count += 1;
        }
        for table in carrier::coverage_tables(&schema) {
            table_complement_round_trips(&mut store, &table);
            count += 1;
        }
        for tree in carrier::coverage_trees(&schema) {
            tree_complement_round_trips(&mut store, &tree);
            count += 1;
        }
    }
    assert_eq!(count, 61_066 + 32_038 + 3_156);
}

// ---------------------------------------------------------------------------
// Refusals.
// ---------------------------------------------------------------------------

fn sample_instance(shape: Shape) -> Instance {
    match shape {
        Shape::Tree => Instance::Tree(vec![WInstance::new(
            HashMap::from([(0, Node::new(0, "s"))]),
            Vec::new(),
            Vec::new(),
            0,
            "s".into(),
        )]),
        Shape::Table => Instance::Table(
            FInstance::new()
                .with_table("item", [HashMap::new()])
                .unwrap(),
        ),
        Shape::Graph => Instance::Graph(GInstance::new().with_node(Node::new(0, "item"))),
    }
}

fn sample_complement(shape: Shape) -> ComplementPayload {
    match shape {
        Shape::Tree => ComplementPayload::Tree(vec![Complement::default()]),
        Shape::Table => ComplementPayload::Table(Box::new(TableComplement::empty())),
        Shape::Graph => ComplementPayload::Graph(Box::new(GraphComplement::empty())),
    }
}

fn sample_edits(shape: Shape) -> EditLogPayload {
    match shape {
        Shape::Tree => EditLogPayload::Tree(vec![TreeEdit::Identity]),
        Shape::Table => EditLogPayload::Table(vec![TableDelta::default()]),
        Shape::Graph => EditLogPayload::Graph(vec![GraphDelta::default()]),
    }
}

const fn production_shape(shape: Shape) -> InstanceShape {
    match shape {
        Shape::Tree => InstanceShape::Tree,
        Shape::Table => InstanceShape::Table,
        Shape::Graph => InstanceShape::Graph,
    }
}

/// The outcome a decode gives, as the reference names it.
fn outcome<T>(result: Result<T, VcsError>) -> PayloadOutcome {
    match result {
        Ok(_) => PayloadOutcome::Decoded,
        Err(VcsError::UnsupportedPayloadEncoding { .. }) => PayloadOutcome::Unsupported,
        Err(VcsError::UndecodablePayload { .. }) => PayloadOutcome::Undecodable,
        Err(VcsError::NonCanonicalPayload { .. }) => PayloadOutcome::NonCanonical,
        Err(other) => panic!("an untyped refusal: {other}"),
    }
}

/// The ways a payload is damaged, the first leaving it intact.
const DAMAGES: [&str; 8] = [
    "none",
    "version 0",
    "version 2",
    "trailing byte",
    "truncated",
    "empty",
    "wrong shape",
    "non-minimal header",
];

fn damage(bytes: &mut Vec<u8>, encoding: &mut PayloadEncoding, how: &str) {
    match how {
        "none" => {}
        "version 0" => encoding.version = 0,
        "version 2" => encoding.version = 2,
        "trailing byte" => bytes.push(0xc0),
        "truncated" => {
            bytes.pop();
        }
        "empty" => bytes.clear(),
        "wrong shape" => {
            encoding.shape = match encoding.shape {
                InstanceShape::Tree => InstanceShape::Graph,
                InstanceShape::Table => InstanceShape::Tree,
                InstanceShape::Graph => InstanceShape::Table,
            };
        }
        "non-minimal header" => {
            // Every payload opens with a fixed array; the same array under
            // a 16-bit length is the same value in other bytes.
            let first = bytes[0];
            assert_eq!(first & 0xf0, 0x90, "a payload opens with a fixed array");
            bytes.splice(0..1, [0xdc, 0x00, first & 0x0f]);
        }
        other => panic!("unknown damage {other}"),
    }
}

const fn model_shape(shape: InstanceShape) -> Shape {
    match shape {
        InstanceShape::Tree => Shape::Tree,
        InstanceShape::Table => Shape::Table,
        InstanceShape::Graph => Shape::Graph,
    }
}

/// The facts of a payload damaged by `how`, now recording `encoding`.
fn facts(encoding: PayloadEncoding, how: &str, count_agrees: bool) -> PayloadFacts {
    PayloadFacts {
        shape: model_shape(encoding.shape),
        version: encoding.version,
        decodes_whole: !matches!(how, "trailing byte" | "truncated" | "empty" | "wrong shape"),
        canonical: how != "non-minimal header",
        count_agrees,
    }
}

/// Decode a data set and an edit log of `shape` damaged by `how`, with and
/// without an agreeing count, and check each against the reference.
fn check_counted_payloads(shape: Shape, how: &str, checked: &mut BTreeMap<PayloadOutcome, usize>) {
    for count_agrees in [true, false] {
        let mut dataset =
            DataSetObject::from_instance(ObjectId::ZERO, &sample_instance(shape), None).unwrap();
        damage(&mut dataset.data, &mut dataset.encoding, how);
        if !count_agrees {
            dataset.record_count += 1;
        }
        let expected = payload_outcome(&facts(dataset.encoding, how, count_agrees));
        assert_eq!(
            outcome(dataset.decode()),
            expected,
            "{shape:?} data set, {how}"
        );
        *checked.entry(expected).or_default() += 1;

        let mut log = EditLogObject::from_payload(
            ObjectId::ZERO,
            ObjectId::ZERO,
            &sample_edits(shape),
            ObjectId::ZERO,
        )
        .unwrap();
        damage(&mut log.edits, &mut log.encoding, how);
        if !count_agrees {
            log.edit_count += 1;
        }
        let expected = payload_outcome(&facts(log.encoding, how, count_agrees));
        assert_eq!(outcome(log.decode()), expected, "{shape:?} edit log, {how}");
        *checked.entry(expected).or_default() += 1;
    }
}

#[test]
fn every_damaged_payload_gives_the_outcome_the_reference_gives() {
    let mut checked: BTreeMap<PayloadOutcome, usize> = BTreeMap::new();
    for shape in Shape::ALL {
        for how in DAMAGES {
            check_counted_payloads(shape, how, &mut checked);
            // A complement records no count.
            let mut complement = ComplementObject::from_payload(
                ObjectId::ZERO,
                ObjectId::ZERO,
                &sample_complement(shape),
            )
            .unwrap();
            damage(&mut complement.complement, &mut complement.encoding, how);
            let expected = payload_outcome(&facts(complement.encoding, how, true));
            assert_eq!(
                outcome(complement.decode()),
                expected,
                "{shape:?} complement, {how}"
            );
            *checked.entry(expected).or_default() += 1;
        }
    }
    // Per shape, eight damages, each decoded five times: as a data set and
    // an edit log with and without an agreeing count, and as a complement.
    // An intact payload decodes three times and is refused twice for its
    // count; a non-minimal tree payload does the same, while a non-minimal
    // table or graph payload is refused five times as non-canonical. Each
    // other version is refused five times as unsupported, and each of the
    // four damages that break the bytes five times as undecodable.
    assert_eq!(checked.values().sum::<usize>(), 3 * 8 * 5);
    assert_eq!(
        checked,
        BTreeMap::from([
            (PayloadOutcome::Decoded, 3 * 3 + 3),
            (PayloadOutcome::Unsupported, 3 * 2 * 5),
            (PayloadOutcome::Undecodable, 3 * 4 * 5 + 3 * 2 + 2),
            (PayloadOutcome::NonCanonical, 2 * 5),
        ])
    );
}

#[test]
fn every_combination_of_payload_facts_is_decided_in_the_stated_order() {
    let all = payload_facts(3);
    assert_eq!(all.len(), 3 * 3 * 8);
    let decoded = all
        .iter()
        .filter(|facts| payload_outcome(facts) == PayloadOutcome::Decoded)
        .count();
    // Version 1, decoding whole and an agreeing count decode a tree payload
    // whether or not it is canonical, and a table or graph payload only
    // when it is.
    assert_eq!(decoded, 2 + 1 + 1);
}

#[test]
fn an_object_writes_its_encoding_exactly_when_the_reference_says_so() {
    /// The number of fields of the object's positional array.
    fn field_count(object: &Object) -> u8 {
        let bytes = rmp_serde::to_vec(object).unwrap();
        // A one-entry map, the variant name as a fixed string, then the
        // fields as a fixed array.
        let name_len = usize::from(bytes[1] & 0x1f);
        let header = bytes[2 + name_len];
        assert_eq!(header & 0xf0, 0x90);
        header & 0x0f
    }

    for (shape, version) in encodings(3) {
        let encoding = PayloadEncoding {
            shape: production_shape(shape),
            version,
        };
        let written = writes_encoding_field((shape, version));
        let extra = u8::from(written);
        let dataset = Object::DataSet(DataSetObject {
            schema_id: ObjectId::ZERO,
            data: Vec::new(),
            record_count: 0,
            key: None,
            encoding,
        });
        let complement = Object::Complement(ComplementObject {
            migration_id: ObjectId::ZERO,
            data_id: ObjectId::ZERO,
            complement: Vec::new(),
            encoding,
        });
        let log = Object::EditLog(EditLogObject {
            schema_id: ObjectId::ZERO,
            data_id: ObjectId::ZERO,
            edits: Vec::new(),
            edit_count: 0,
            final_complement: ObjectId::ZERO,
            encoding,
        });
        assert_eq!(field_count(&dataset), 4 + extra);
        assert_eq!(field_count(&complement), 3 + extra);
        assert_eq!(field_count(&log), 5 + extra);
        for object in [dataset, complement, log] {
            let read: Object = rmp_serde::from_slice(&rmp_serde::to_vec(&object).unwrap()).unwrap();
            let recorded = match read {
                Object::DataSet(o) => o.encoding,
                Object::Complement(o) => o.encoding,
                Object::EditLog(o) => o.encoding,
                other => panic!("read back a {}", other.type_name()),
            };
            let field = written.then_some((shape, version));
            assert_eq!(
                (model_shape(recorded.shape), recorded.version),
                recorded_encoding(field)
            );
        }
    }
}

/// A graph payload whose node map lists its entries in descending order:
/// the same graph, in a byte string that is not its encoding.
#[test]
fn a_table_or_graph_payload_out_of_canonical_order_is_refused() {
    use serde::ser::{SerializeMap, SerializeStruct};

    struct Descending<'a>(&'a GInstance);
    struct Nodes<'a>(&'a GInstance);
    struct Edges<'a>(&'a GInstance);

    impl serde::Serialize for Nodes<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut ids: Vec<&u32> = self.0.nodes().keys().collect();
            ids.sort_unstable();
            ids.reverse();
            let mut map = serializer.serialize_map(Some(ids.len()))?;
            for id in ids {
                map.serialize_entry(id, &self.0.nodes()[id])?;
            }
            map.end()
        }
    }
    impl serde::Serialize for Edges<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut map = serializer.serialize_map(Some(self.0.edge_count()))?;
            for (id, edge) in self.0.edges_sorted() {
                map.serialize_entry(&id, edge)?;
            }
            map.end()
        }
    }
    impl serde::Serialize for Descending<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut wire = serializer.serialize_struct("GInstanceWire", 2)?;
            wire.serialize_field("nodes", &Nodes(self.0))?;
            wire.serialize_field("edges", &Edges(self.0))?;
            wire.end()
        }
    }

    let graph = GInstance::new()
        .with_node(Node::new(0, "a"))
        .with_node(Node::new(1, "b"));
    let mut dataset =
        DataSetObject::from_instance(ObjectId::ZERO, &Instance::Graph(graph.clone()), None)
            .unwrap();
    let canonical = dataset.data.clone();
    dataset.data = rmp_serde::to_vec(&Descending(&graph)).unwrap();
    assert_ne!(dataset.data, canonical);
    assert_eq!(outcome(dataset.decode()), PayloadOutcome::NonCanonical);
}

#[test]
fn a_tree_object_records_no_encoding_and_keeps_its_identifier() {
    // The bytes a build that predates payload encodings writes for a data
    // set: the four fields, positionally, under the variant name.
    #[derive(serde::Serialize)]
    struct DataSetBefore {
        schema_id: ObjectId,
        data: Vec<u8>,
        record_count: u64,
        key: Option<String>,
    }
    #[derive(serde::Serialize)]
    enum ObjectBefore {
        DataSet(DataSetBefore),
    }

    let dataset = DataSetObject::from_instance(
        schema_id(3),
        &sample_instance(Shape::Tree),
        Some("k".into()),
    )
    .unwrap();
    let before = ObjectBefore::DataSet(DataSetBefore {
        schema_id: dataset.schema_id,
        data: dataset.data.clone(),
        record_count: dataset.record_count,
        key: dataset.key.clone(),
    });
    let object = Object::DataSet(dataset);
    assert_eq!(
        rmp_serde::to_vec(&object).unwrap(),
        rmp_serde::to_vec(&before).unwrap()
    );

    // A graph data set over the same bytes is a different object.
    let Object::DataSet(mut graph) = object.clone() else {
        unreachable!()
    };
    graph.encoding = PayloadEncoding::GRAPH;
    assert_ne!(
        panproto_vcs::hash::object_id(&Object::DataSet(graph)).unwrap(),
        panproto_vcs::hash::object_id(&object).unwrap()
    );
    for shape in Shape::ALL {
        assert_eq!(PayloadEncoding::current(production_shape(shape)).version, 1);
    }
}

// ---------------------------------------------------------------------------
// A repository written by 0.78.
// ---------------------------------------------------------------------------

/// Copy the checked-in repository into a fresh directory, so the test never
/// writes to the fixture.
fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/repository-0.78"),
        dir.path(),
    );
    dir
}

#[test]
fn a_repository_written_by_0_78_opens_and_reads_back_unchanged() {
    let dir = fixture();
    let mut repo = Repository::open(dir.path()).unwrap();
    // The fixture's schemas name the protocol "test", which the registry
    // does not declare; the repository that wrote it registered it.
    repo.set_protocol(panproto_schema::Protocol {
        name: "test".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        obj_kinds: vec!["object".into(), "string".into(), "record".into()],
        ..Default::default()
    });
    let store = repo.store();
    let mut ids = store.list_objects().unwrap();
    ids.sort_unstable();
    assert_eq!(ids.len(), 35);

    let mut kinds: BTreeMap<&'static str, usize> = BTreeMap::new();
    for id in &ids {
        // `get` re-derives the identifier from the stored bytes.
        let object = store.get(id).unwrap();
        *kinds.entry(object.type_name()).or_default() += 1;
        let stored = std::fs::read(
            dir.path()
                .join(".panproto/objects")
                .join(&id.to_string()[..2])
                .join(&id.to_string()[2..]),
        )
        .unwrap();
        match &object {
            Object::DataSet(dataset) => {
                assert_eq!(dataset.encoding, PayloadEncoding::TREE);
                let Instance::Tree(trees) = dataset.decode().unwrap() else {
                    panic!("a 0.78 data set holds trees");
                };
                assert_eq!(trees.len(), 2);
                assert_eq!(rmp_serde::to_vec(&object).unwrap(), stored);
            }
            Object::Complement(complement) => {
                assert_eq!(complement.encoding, PayloadEncoding::TREE);
                let ComplementPayload::Tree(complements) = complement.decode().unwrap() else {
                    panic!("a 0.78 complement holds tree complements");
                };
                assert_eq!(complements.len(), 2);
                assert_eq!(rmp_serde::to_vec(&object).unwrap(), stored);
            }
            Object::EditLog(log) => {
                assert_eq!(log.encoding, PayloadEncoding::TREE);
                assert!(
                    matches!(log.decode().unwrap(), EditLogPayload::Tree(edits) if edits.is_empty())
                );
                assert_eq!(rmp_serde::to_vec(&object).unwrap(), stored);
            }
            _ => {}
        }
    }
    assert_eq!(
        kinds,
        BTreeMap::from([
            ("commit", 6),
            ("complement", 1),
            ("dataset", 3),
            ("editlog", 1),
            ("file_schema", 5),
            ("flat_schema", 5),
            ("migration", 4),
            ("schema_tree", 5),
            ("theory", 5),
        ])
    );

    let mut messages: Vec<String> = repo
        .log(None)
        .unwrap()
        .into_iter()
        .map(|commit| commit.message)
        .collect();
    messages.sort();
    assert_eq!(
        messages,
        [
            "add c on main",
            "add d on side",
            "add data and b",
            "initial schema",
            "merge branch 'side'",
        ]
    );

    // The data staged at "add data and b" decodes through its schema.
    let staged_at = ids
        .iter()
        .find(|id| {
            matches!(store.get(id), Ok(Object::Commit(commit)) if commit.message == "add data and b")
        })
        .unwrap();
    let records: Vec<serde_json::Value> = repo
        .decoded_data_at(&staged_at.to_string())
        .unwrap()
        .into_iter()
        .flat_map(|set| set.records)
        .collect();
    assert_eq!(
        records,
        vec![serde_json::json!({"a": "x"}), serde_json::json!({"a": "y"})]
    );
}

#[test]
fn a_repository_written_by_0_78_takes_table_and_graph_objects_in_format_1() {
    let dir = fixture();
    let mut repo = Repository::open(dir.path()).unwrap();
    let head = panproto_vcs::store::resolve_head(repo.store())
        .unwrap()
        .unwrap();
    let Object::Commit(head_commit) = repo.store().get(&head).unwrap() else {
        panic!("HEAD is a commit");
    };
    let mut data_ids = Vec::new();
    for shape in [Shape::Table, Shape::Graph] {
        let dataset = DataSetObject::from_instance(
            head_commit.schema_id,
            &sample_instance(shape),
            Some(format!("{shape:?}")),
        )
        .unwrap();
        data_ids.push(repo.store_mut().put(&Object::DataSet(dataset)).unwrap());
    }
    let commit = CommitObject::builder(head_commit.schema_id, "test", "test", "tables and graphs")
        .parents(vec![head])
        .timestamp(0)
        .data_ids(data_ids.clone())
        .build();
    let commit_id = repo.store_mut().put(&Object::Commit(commit)).unwrap();
    repo.store_mut()
        .set_ref("refs/heads/main", commit_id)
        .unwrap();

    let reopened = Repository::open(dir.path()).unwrap();
    assert_eq!(
        std::fs::read(dir.path().join(".panproto/format")).unwrap(),
        b"1\n"
    );
    let shapes: Vec<InstanceShape> = data_ids
        .iter()
        .map(|id| match reopened.store().get(id).unwrap() {
            Object::DataSet(dataset) => dataset.decode().unwrap().shape(),
            other => panic!("expected a data set, found {}", other.type_name()),
        })
        .collect();
    assert_eq!(shapes, [InstanceShape::Table, InstanceShape::Graph]);
}

/// The protocol the fixture's schemas name, which the repository that wrote
/// it registered.
fn fixture_protocol() -> panproto_schema::Protocol {
    panproto_schema::Protocol {
        name: "test".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        obj_kinds: vec!["object".into(), "string".into(), "record".into()],
        ..Default::default()
    }
}

/// Every object of a repository, by identifier.
fn all_objects(repo: &Repository) -> Vec<(ObjectId, Object)> {
    repo.store()
        .list_objects()
        .unwrap()
        .into_iter()
        .map(|id| (id, repo.store().get(&id).unwrap()))
        .collect()
}

/// The data sets of `objects` whose schema identifier names a flat schema.
fn flat_schema_data(
    repo: &Repository,
    objects: &[(ObjectId, Object)],
) -> Vec<(ObjectId, DataSetObject)> {
    objects
        .iter()
        .filter_map(|(id, object)| match object {
            Object::DataSet(dataset)
                if matches!(
                    repo.store().get(&dataset.schema_id),
                    Ok(Object::FlatSchema(_))
                ) =>
            {
                Some((*id, dataset.clone()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_0_78_complement_names_no_migration_and_is_refused_by_backward_migration() {
    use panproto_vcs::{MigrationConflict, migrate_backward};
    let dir = fixture();
    let mut repo = Repository::open(dir.path()).unwrap();
    repo.set_protocol(fixture_protocol());
    let objects = all_objects(&repo);

    // The one complement 0.78 wrote records its data set as its migration,
    // and reads as the complement of that data set under no known migration.
    let complements: Vec<(ObjectId, ComplementObject)> = objects
        .iter()
        .filter_map(|(id, object)| match object {
            Object::Complement(complement) => Some((*id, complement.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(complements.len(), 1);
    let (complement_id, complement) = &complements[0];
    assert_eq!(complement.migration_id, complement.data_id);
    assert_eq!(complement.migration(), None);

    // Putting back against it is refused, naming the data set to migrate
    // forward again.
    let flat = flat_schema_data(&repo, &objects);
    let (view_id, _) = flat
        .iter()
        .find(|(id, _)| *id != complement.data_id)
        .unwrap();
    match migrate_backward(
        repo.store_mut(),
        *view_id,
        *complement_id,
        &fixture_protocol(),
    ) {
        Err(VcsError::Migration(conflict)) => assert_eq!(
            *conflict,
            MigrationConflict::UnknownMigration {
                complement_id: *complement_id,
                data_id: complement.data_id,
            }
        ),
        other => panic!("a complement naming no migration was put back: {other:?}"),
    }
}

#[test]
fn a_0_78_data_set_naming_a_flat_schema_reads_through_it_and_is_stale_by_schema_identity() {
    use panproto_vcs::{resolve_schema, schema_identity};
    let dir = fixture();
    let mut repo = Repository::open(dir.path()).unwrap();
    repo.set_protocol(fixture_protocol());
    let objects = all_objects(&repo);

    // The data sets 0.78 migrated name a flat schema; each resolves through
    // that object, whose identity is its identifier.
    let flat = flat_schema_data(&repo, &objects);
    assert_eq!(flat.len(), 2);
    for (_, dataset) in &flat {
        let schema = resolve_schema(repo.store(), &dataset.schema_id).unwrap();
        assert_eq!(
            panproto_vcs::hash::hash_schema(&schema).unwrap(),
            dataset.schema_id
        );
        assert_eq!(
            schema_identity(repo.store(), &dataset.schema_id).unwrap(),
            dataset.schema_id
        );
    }

    // Each flat schema is the schema of one commit, which names it by its
    // schema tree: the two identifiers name one schema.
    let commits: Vec<(ObjectId, CommitObject)> = objects
        .iter()
        .filter_map(|(id, object)| match object {
            Object::Commit(commit) => Some((*id, commit.clone())),
            _ => None,
        })
        .collect();
    let same_schema: Vec<usize> = flat
        .iter()
        .map(|(_, dataset)| {
            commits
                .iter()
                .filter(|(_, commit)| {
                    schema_identity(repo.store(), &commit.schema_id).unwrap() == dataset.schema_id
                })
                .count()
        })
        .collect();
    let recorded: Vec<(String, bool)> = commits
        .iter()
        .flat_map(|(commit_id, commit)| {
            let repo = &repo;
            flat.iter()
                .filter(|(data_id, _)| commit.data_ids.contains(data_id))
                .map(move |(_, dataset)| {
                    let committed = repo
                        .committed_instance(&commit_id.to_string(), dataset.key.as_deref().unwrap())
                        .unwrap();
                    (commit.message.clone(), committed.stale)
                })
        })
        .collect();
    assert_eq!(same_schema, [1, 1]);
    // The commit recording the migrated data set has another schema, so the
    // instance it tracks there is stale.
    assert_eq!(recorded, [("record migration".to_owned(), true)]);
    for (_, dataset) in &flat {
        assert!(
            commits
                .iter()
                .all(|(_, commit)| commit.schema_id != dataset.schema_id)
        );
    }
}

/// The model of a stored repository: each object by its hexadecimal
/// identifier with the identifiers its fields hold, its refs, `HEAD` and
/// stash reflog.
fn model_of(store: &dyn Store) -> ModelRepository {
    let hex = |id: &ObjectId| id.to_string();
    let all = |ids: &[ObjectId]| ids.iter().map(hex).collect::<Vec<_>>();
    let shape = |encoding: PayloadEncoding| match encoding.shape {
        InstanceShape::Tree => Shape::Tree,
        InstanceShape::Table => Shape::Table,
        InstanceShape::Graph => Shape::Graph,
    };
    let mut objects = BTreeMap::new();
    for id in store.list_objects().unwrap() {
        let object = match store.get(&id).unwrap() {
            Object::Commit(commit) => ModelObject::Commit {
                schema: hex(&commit.schema_id),
                migration: commit.migration_id.as_ref().map(hex),
                parents: all(&commit.parents),
                data: all(&commit.data_ids),
                complements: all(&commit.complement_ids),
                edit_logs: all(&commit.edit_log_ids),
                metadata: commit
                    .protocol_id
                    .iter()
                    .chain(commit.theory_ids.values())
                    .chain(&commit.cst_complement_ids)
                    .map(hex)
                    .collect(),
            },
            Object::Migration { src, tgt, .. } => ModelObject::Migration {
                src: hex(&src),
                tgt: hex(&tgt),
            },
            Object::SchemaTree(tree) => ModelObject::SchemaTree {
                entries: match *tree {
                    SchemaTreeObject::SingleLeaf { file_schema_id } => vec![hex(&file_schema_id)],
                    SchemaTreeObject::Directory { entries } => entries
                        .iter()
                        .map(|(_, entry)| match entry {
                            panproto_vcs::object::SchemaTreeEntry::File(id)
                            | panproto_vcs::object::SchemaTreeEntry::Tree(id) => hex(id),
                        })
                        .collect(),
                },
            },
            Object::Tag(tag) => ModelObject::Tag {
                target: hex(&tag.target),
            },
            Object::DataSet(dataset) => ModelObject::DataSet {
                shape: shape(dataset.encoding),
                schema: hex(&dataset.schema_id),
            },
            Object::Complement(complement) => ModelObject::Complement {
                shape: shape(complement.encoding),
                migration: hex(&complement.migration_id),
                data: hex(&complement.data_id),
            },
            Object::EditLog(log) => ModelObject::EditLog {
                shape: shape(log.encoding),
                schema: hex(&log.schema_id),
                data: hex(&log.data_id),
                final_complement: hex(&log.final_complement),
            },
            Object::FileSchema(_) | Object::FlatSchema(_) | Object::Theory(_) => ModelObject::Leaf,
            other => panic!("the fixture holds no {} object", other.type_name()),
        };
        objects.insert(hex(&id), object);
    }
    let refs = store
        .list_refs("refs/")
        .unwrap()
        .into_iter()
        .map(|(name, id)| (name, hex(&id)))
        .collect();
    let head = match store.get_head().unwrap() {
        HeadState::Branch(name) => Head::Branch(name),
        HeadState::Detached(id) => Head::Detached(hex(&id)),
    };
    let stash_log = store
        .read_reflog("refs/stash", None)
        .unwrap()
        .iter()
        .map(|entry| hex(&entry.new_id))
        .collect();
    ModelRepository {
        objects,
        refs,
        head,
        stash_log,
        staged: Vec::new(),
    }
}

#[test]
fn garbage_collection_of_a_0_78_repository_deletes_only_what_no_ref_reaches() {
    let dir = fixture();
    let mut repo = Repository::open(dir.path()).unwrap();
    let model = model_of(repo.store());
    let garbage = model.garbage();
    // The data set the backward migration restored is referenced by
    // nothing; the flat schema it names is also a migration endpoint.
    assert_eq!(garbage.len(), 1);
    let report = repo.gc().unwrap();
    let deleted: BTreeSet<String> = report.deleted.iter().map(ToString::to_string).collect();
    assert_eq!(deleted, garbage);
    let remaining: BTreeSet<String> = repo
        .store()
        .list_objects()
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(remaining, model.reachable());
    assert_eq!(remaining.len(), 34);
}

// ---------------------------------------------------------------------------
// Garbage collection.
// ---------------------------------------------------------------------------

/// The production object of the model object `name`, whose links are
/// already stored under `ids`.
fn production_object(name: &str, object: &ModelObject, ids: &BTreeMap<String, ObjectId>) -> Object {
    let id_of = |link: &str, ids: &BTreeMap<String, ObjectId>| ids[link];
    match object {
        ModelObject::Leaf => {
            let schema = panproto_schema::Schema {
                protocol: name.to_owned(),
                ..Default::default()
            };
            Object::FlatSchema(Box::new(schema))
        }
        ModelObject::SchemaTree { entries } => {
            let tree = match entries.as_slice() {
                [only] => SchemaTreeObject::SingleLeaf {
                    file_schema_id: id_of(only, ids),
                },
                many => SchemaTreeObject::Directory {
                    entries: many
                        .iter()
                        .map(|entry| {
                            (
                                entry.clone(),
                                panproto_vcs::object::SchemaTreeEntry::File(id_of(entry, ids)),
                            )
                        })
                        .collect(),
                },
            };
            Object::SchemaTree(Box::new(tree))
        }
        ModelObject::Migration { src, tgt } => Object::Migration {
            src: id_of(src, ids),
            tgt: id_of(tgt, ids),
            mapping: panproto_mig::Migration::empty(),
        },
        ModelObject::Commit { .. } => commit_object(name, object, ids),
        ModelObject::Tag { target } => Object::Tag(TagObject {
            target: id_of(target, ids),
            tagger: "test".to_owned(),
            timestamp: 0,
            message: name.to_owned(),
        }),
        ModelObject::DataSet { shape, schema } => Object::DataSet(
            DataSetObject::from_instance(
                id_of(schema, ids),
                &sample_instance(*shape),
                Some(name.to_owned()),
            )
            .unwrap(),
        ),
        ModelObject::Complement {
            shape,
            migration,
            data,
        } => Object::Complement(
            ComplementObject::from_payload(
                id_of(migration, ids),
                id_of(data, ids),
                &sample_complement(*shape),
            )
            .unwrap(),
        ),
        ModelObject::EditLog {
            shape,
            schema,
            data,
            final_complement,
        } => Object::EditLog(
            EditLogObject::from_payload(
                id_of(schema, ids),
                id_of(data, ids),
                &sample_edits(*shape),
                id_of(final_complement, ids),
            )
            .unwrap(),
        ),
    }
}

/// The production commit of the model commit `name`.
fn commit_object(name: &str, object: &ModelObject, ids: &BTreeMap<String, ObjectId>) -> Object {
    let ModelObject::Commit {
        schema,
        migration,
        parents,
        data,
        complements,
        edit_logs,
        metadata,
    } = object
    else {
        panic!("{name} is not a commit");
    };
    let all = |names: &[String]| names.iter().map(|n| ids[n]).collect::<Vec<_>>();
    let mut builder = CommitObject::builder(ids[schema], "test", "test", name)
        .timestamp(0)
        .parents(all(parents))
        .data_ids(all(data))
        .complement_ids(all(complements))
        .edit_log_ids(all(edit_logs))
        .theory_ids(
            metadata
                .iter()
                .map(|theory| (theory.clone(), ids[theory]))
                .collect(),
        );
    if let Some(migration) = migration {
        builder = builder.migration_id(ids[migration]);
    }
    Object::Commit(builder.build())
}

/// Store the model's objects in `store`, returning each name's identifier.
fn materialize(model: &ModelRepository, store: &mut dyn Store) -> BTreeMap<String, ObjectId> {
    fn put(
        name: &str,
        model: &ModelRepository,
        store: &mut dyn Store,
        ids: &mut BTreeMap<String, ObjectId>,
    ) -> ObjectId {
        if let Some(id) = ids.get(name) {
            return *id;
        }
        let object = &model.objects[name];
        for link in object.links() {
            put(link, model, store, ids);
        }
        let id = store.put(&production_object(name, object, ids)).unwrap();
        ids.insert(name.to_owned(), id);
        id
    }

    let mut ids = BTreeMap::new();
    for name in model.objects.keys() {
        put(name, model, store, &mut ids);
    }
    let distinct: BTreeSet<ObjectId> = ids.values().copied().collect();
    assert_eq!(distinct.len(), ids.len(), "two model objects share an id");
    ids
}

/// Set the model's refs, `HEAD` and stash reflog, and return the staged
/// identifiers.
fn set_roots(
    model: &ModelRepository,
    ids: &BTreeMap<String, ObjectId>,
    store: &mut dyn Store,
) -> Vec<ObjectId> {
    for (name, target) in &model.refs {
        store.set_ref(name, ids[target]).unwrap();
    }
    match &model.head {
        Head::Branch(branch) => store.set_head(HeadState::Branch(branch.clone())).unwrap(),
        Head::Detached(target) => store.set_head(HeadState::Detached(ids[target])).unwrap(),
    }
    for entry in &model.stash_log {
        store
            .append_reflog(
                "refs/stash",
                ReflogEntry {
                    old_id: None,
                    new_id: ids[entry],
                    author: "test".to_owned(),
                    timestamp: 0,
                    message: "stash".to_owned(),
                },
            )
            .unwrap();
    }
    model.staged.iter().map(|name| ids[name]).collect()
}

/// Collect garbage in `store` and compare what is deleted and what remains
/// with the reference; returns the number of objects kept.
fn check_gc(model: &ModelRepository, store: &mut dyn Store) -> usize {
    let ids = materialize(model, store);
    let staged = set_roots(model, &ids, store);
    let names: BTreeMap<ObjectId, String> =
        ids.iter().map(|(name, id)| (*id, name.clone())).collect();
    let before = store.list_objects().unwrap().len();
    let dry = gc_with_roots(store, &staged, &GcOptions { dry_run: true }).unwrap();
    assert_eq!(store.list_objects().unwrap().len(), before);
    let report = gc_with_roots(store, &staged, &GcOptions::default()).unwrap();
    assert_eq!(dry.deleted, report.deleted);
    let deleted: BTreeSet<String> = report.deleted.iter().map(|id| names[id].clone()).collect();
    assert_eq!(deleted, model.garbage(), "{model:?}");
    let kept: BTreeSet<String> = store
        .list_objects()
        .unwrap()
        .iter()
        .map(|id| names[id].clone())
        .collect();
    let reachable = model.reachable();
    assert_eq!(kept, reachable);
    let mut sorted = report.deleted.clone();
    sorted.sort_unstable();
    assert_eq!(report.deleted, sorted);
    reachable.len()
}

#[test]
fn garbage_collection_keeps_exactly_what_the_reference_reaches_on_every_repository_of_the_scope() {
    let cases = gc_cases();
    assert_eq!(cases.len(), GC_CASES_COUNT);
    let mut kept = 0usize;
    let mut kept_by_roots: BTreeMap<Roots, usize> = BTreeMap::new();
    for case in &cases {
        let mut store = MemStore::new();
        let count = check_gc(&case.repository, &mut store);
        kept += count;
        *kept_by_roots.entry(case.roots).or_default() += count;
    }
    assert_eq!(kept, GC_KEPT_TOTAL);
    assert_eq!(kept_by_roots[&Roots::None], 0);
    assert_eq!(kept_by_roots[&Roots::MainAtFirst], 41_472);
    assert_eq!(kept_by_roots[&Roots::StashLog], 54_784);
    assert_eq!(kept_by_roots[&Roots::AnnotatedTag], 58_880);
    assert_eq!(kept_by_roots[&Roots::StagedOnly], 12_288);
}

#[test]
fn garbage_collection_on_disk_keeps_what_the_reference_reaches_under_every_root() {
    // One repository per root configuration, with every shape at a
    // different level, on a filesystem store.
    for roots in Roots::ALL {
        let model = panproto_reference::enumerate::instance_version_control::repository(
            [[3, 2, 1], [0, 1, 3]],
            roots,
        );
        let dir = tempfile::tempdir().unwrap();
        let mut store = panproto_vcs::FsStore::init(dir.path()).unwrap();
        check_gc(&model, &mut store);
    }
}

#[test]
fn a_repository_keeps_staged_data_through_garbage_collection() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let protocol = panproto_schema::Protocol {
        name: "test".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        obj_kinds: vec!["object".into(), "string".into()],
        ..Default::default()
    };
    repo.set_protocol(protocol.clone());
    let schema = panproto_schema::SchemaBuilder::new(&protocol)
        .vertex("a", "object", None)
        .unwrap()
        .vertex("a.a", "string", None)
        .unwrap()
        .edge("a", "a.a", "prop", Some("a"))
        .unwrap()
        .entry("a")
        .build()
        .unwrap();
    repo.add(&schema).unwrap();
    repo.commit("schema", "test").unwrap();
    let path = dir.path().join("records.json");
    std::fs::write(&path, r#"[{"a": "x"}]"#).unwrap();
    let staged = repo.add_data(&path, None).unwrap().staged_data[0].data_id;

    let report = repo.gc().unwrap();
    assert!(!report.deleted.contains(&staged));
    assert!(repo.store().has(&staged));
    let commit = repo.commit("data", "test").unwrap();
    let Object::Commit(commit) = repo.store().get(&commit).unwrap() else {
        panic!("a commit");
    };
    assert_eq!(commit.data_ids, vec![staged]);
}

fn arbitrary_repository() -> impl Strategy<Value = ModelRepository> {
    let commit = (
        proptest::collection::vec(any::<bool>(), 9),
        proptest::collection::vec(any::<u8>(), 0..3),
        any::<bool>(),
    );
    (
        proptest::collection::vec(commit, 1..6),
        proptest::collection::vec((0u8..6, any::<u8>()), 0..4),
        proptest::option::of(any::<u8>()),
        proptest::collection::vec(any::<u8>(), 0..3),
        proptest::collection::vec(0u8..15, 0..3),
    )
        .prop_map(|(commits, refs, detached, stash, staged)| {
            let mut objects =
                panproto_reference::enumerate::instance_version_control::fixed_objects();
            let shapes: Vec<String> = ["tree", "table", "graph"]
                .iter()
                .flat_map(|x| [format!("D.{x}"), format!("K.{x}"), format!("L.{x}")])
                .collect();
            let commit_name = |index: usize| format!("C{index}");
            for (index, (chosen, parents, migrated)) in commits.iter().enumerate() {
                let mut data = Vec::new();
                let mut complements = Vec::new();
                let mut edit_logs = Vec::new();
                for (name, keep) in shapes.iter().zip(chosen) {
                    if *keep {
                        match &name[..1] {
                            "D" => data.push(name.clone()),
                            "K" => complements.push(name.clone()),
                            _ => edit_logs.push(name.clone()),
                        }
                    }
                }
                let mut parent_names: Vec<String> = if index == 0 {
                    Vec::new()
                } else {
                    parents
                        .iter()
                        .map(|p| commit_name(usize::from(*p) % index))
                        .collect()
                };
                parent_names.dedup();
                objects.insert(
                    commit_name(index),
                    ModelObject::Commit {
                        schema: "S".to_owned(),
                        migration: migrated.then(|| "M".to_owned()),
                        parents: parent_names,
                        data,
                        complements,
                        edit_logs,
                        metadata: Vec::new(),
                    },
                );
            }
            let n = commits.len();
            let pick = |byte: u8| commit_name(usize::from(byte) % n);
            let namespaces = [
                "refs/heads/main",
                "refs/heads/topic",
                "refs/tags/v1",
                "refs/remotes/origin/main",
                "refs/stash",
                "refs/notes/review",
            ];
            let refs = refs
                .into_iter()
                .map(|(namespace, target)| {
                    (namespaces[usize::from(namespace)].to_owned(), pick(target))
                })
                .collect();
            let head = detached.map_or_else(
                || Head::Branch("main".to_owned()),
                |t| Head::Detached(pick(t)),
            );
            let every: Vec<String> = objects.keys().cloned().collect();
            ModelRepository {
                stash_log: stash.into_iter().map(pick).collect(),
                staged: staged
                    .into_iter()
                    .map(|i| every[usize::from(i) % every.len()].clone())
                    .collect(),
                objects,
                refs,
                head,
            }
        })
}

/// A random graph over the anchors `a` and `b` with arbitrary integer
/// values, as a reference model graph.
fn arbitrary_graph() -> impl Strategy<Value = reference_model::Graph> {
    (
        proptest::collection::btree_map(
            0u32..8,
            (any::<bool>(), proptest::option::of(any::<i64>())),
            0..6,
        ),
        proptest::collection::vec((any::<u8>(), any::<u8>()), 0..8),
    )
        .prop_map(|(nodes, edges)| {
            let anchor = |b: bool| if b { "b" } else { "a" };
            let nodes: BTreeMap<u32, reference_model::Node> = nodes
                .into_iter()
                .map(|(id, (b, value))| {
                    let mut node = reference_model::Node::new(id, anchor(b));
                    node.value = value.map(|v| ReferencePresence::Present(ReferenceValue::Int(v)));
                    (id, node)
                })
                .collect();
            let ids: Vec<u32> = nodes.keys().copied().collect();
            let edges = if ids.is_empty() {
                BTreeMap::new()
            } else {
                edges
                    .into_iter()
                    .enumerate()
                    .map(|(index, (s, t))| {
                        let (src, tgt) = (
                            ids[usize::from(s) % ids.len()],
                            ids[usize::from(t) % ids.len()],
                        );
                        let label = reference_model::SEdge::new(
                            &nodes[&src].anchor,
                            &nodes[&tgt].anchor,
                            "e",
                            None,
                        );
                        (
                            u32::try_from(index).unwrap(),
                            reference_model::GEdge {
                                src,
                                tgt,
                                label,
                                position: None,
                            },
                        )
                    })
                    .collect()
            };
            reference_model::Graph { nodes, edges }
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_graph_payloads_round_trip_in_any_insertion_order(graph in arbitrary_graph()) {
        let mut store = MemStore::new();
        let forward = DataSetObject::from_instance(
            ObjectId::ZERO,
            &Instance::Graph(graph_to_production(&graph, false)),
            None,
        )
        .unwrap();
        let backward = DataSetObject::from_instance(
            ObjectId::ZERO,
            &Instance::Graph(graph_to_production(&graph, true)),
            None,
        )
        .unwrap();
        prop_assert_eq!(&forward.data, &backward.data);
        let (_, decoded) = stored_instance(&mut store, forward);
        let Instance::Graph(decoded) = decoded else {
            panic!("a graph data set decodes to a graph");
        };
        prop_assert_eq!(graph_to_reference(&decoded), graph);
    }

    #[test]
    fn randomized_repositories_keep_exactly_what_the_reference_reaches(model in arbitrary_repository()) {
        let mut store = MemStore::new();
        check_gc(&model, &mut store);
    }
}

// ---------------------------------------------------------------------------
// Deltas between committed instances.
// ---------------------------------------------------------------------------

use panproto_inst::graph_edit::{AttrKey, AttrVal};
use panproto_reference::edit as reference_edit;
use panproto_reference::enumerate::instance_version_control::small_instances;
use panproto_reference::pages::instance_version_control::{
    Delta as ModelDelta, Entry as ModelEntry, Instance as ModelInstance, Refusal as ModelRefusal,
    apply_delta as model_apply, diff_instances as model_diff, diff_tracked as model_diff_tracked,
    normalized, tracked as model_tracked,
};
use panproto_vcs::instance_vc::{self, InstanceDelta};

fn meta_to_reference(meta: &panproto_inst::graph_edit::NodeMeta) -> reference_edit::NodeMeta {
    reference_edit::NodeMeta {
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

fn attr_key_to_reference(key: &AttrKey) -> reference_edit::AttrKey {
    match key {
        AttrKey::Value { node } => reference_edit::AttrKey::Value(*node),
        AttrKey::Field { node, field } => reference_edit::AttrKey::Field(*node, field.clone()),
        AttrKey::Meta { node } => reference_edit::AttrKey::Meta(*node),
        AttrKey::Anchor { node } => reference_edit::AttrKey::Anchor(*node),
        AttrKey::EdgeLabel { edge } => reference_edit::AttrKey::EdgeLabel(edge.0),
        AttrKey::EdgePosition { edge } => reference_edit::AttrKey::EdgePosition(edge.0),
        AttrKey::Cell { .. } | AttrKey::Table { .. } => {
            panic!("table key returned by graph delta: {key:?}")
        }
    }
}

fn attr_val_to_reference(value: &AttrVal) -> reference_edit::AttrVal {
    match value {
        AttrVal::Presence(value) => {
            reference_edit::AttrVal::Presence(value.as_ref().map(presence_to_reference))
        }
        AttrVal::Field(value) => {
            reference_edit::AttrVal::Field(value.as_ref().map(value_to_reference))
        }
        AttrVal::Meta(value) => reference_edit::AttrVal::Meta(meta_to_reference(value)),
        AttrVal::Anchor(value) => reference_edit::AttrVal::Anchor(value.to_string()),
        AttrVal::Label(value) => reference_edit::AttrVal::Label(edge_to_reference(value)),
        AttrVal::Position(value) => reference_edit::AttrVal::Position(*value),
    }
}

fn graph_delta_to_reference(delta: &GraphDelta) -> reference_edit::GraphDelta {
    let mut out = reference_edit::GraphDelta::identity();
    for (id, node) in delta.del_nodes() {
        out.del.insert(
            reference_edit::GElem::Node(*id),
            reference_edit::GPayload::Node(node_to_reference(node)),
        );
    }
    for (id, edge) in delta.del_edges() {
        out.del.insert(
            reference_edit::GElem::Edge(id.0),
            reference_edit::GPayload::Edge(graph_edge_to_reference(edge)),
        );
    }
    for (id, node) in delta.add_nodes() {
        out.add.insert(
            reference_edit::GElem::Node(*id),
            reference_edit::GPayload::Node(node_to_reference(node)),
        );
    }
    for (id, edge) in delta.add_edges() {
        out.add.insert(
            reference_edit::GElem::Edge(id.0),
            reference_edit::GPayload::Edge(graph_edge_to_reference(edge)),
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

fn row_payload(
    table: &panproto_gat::Name,
    cells: &HashMap<String, Value>,
) -> reference_edit::TPayload {
    reference_edit::TPayload::Row(
        table.to_string(),
        cells
            .iter()
            .map(|(column, value)| (column.clone(), value_to_reference(value)))
            .collect(),
    )
}

fn table_delta_to_reference(delta: &TableDelta) -> reference_edit::TableDelta {
    let mut out = reference_edit::TableDelta::identity();
    for (id, (table, cells)) in delta.del_rows() {
        out.del
            .insert(reference_edit::TElem::Row(id.0), row_payload(table, cells));
    }
    for (edge, src, tgt) in delta.del_pairs() {
        out.del.insert(
            reference_edit::TElem::Pair(edge_to_reference(edge), src.0, tgt.0),
            reference_edit::TPayload::Pair,
        );
    }
    for (id, (table, cells)) in delta.add_rows() {
        out.add
            .insert(reference_edit::TElem::Row(id.0), row_payload(table, cells));
    }
    for (edge, src, tgt) in delta.add_pairs() {
        out.add.insert(
            reference_edit::TElem::Pair(edge_to_reference(edge), src.0, tgt.0),
            reference_edit::TPayload::Pair,
        );
    }
    for ((row, column), change) in delta.cells() {
        out.attrs.insert(
            reference_edit::TKey::Cell(row.0, column.clone()),
            (
                reference_edit::TVal::Cell(change.old.as_ref().map(value_to_reference)),
                reference_edit::TVal::Cell(change.new.as_ref().map(value_to_reference)),
            ),
        );
    }
    for (row, change) in delta.tables() {
        out.attrs.insert(
            reference_edit::TKey::Table(row.0),
            (
                reference_edit::TVal::Table(change.old.to_string()),
                reference_edit::TVal::Table(change.new.to_string()),
            ),
        );
    }
    out
}

fn delta_to_reference(delta: &InstanceDelta) -> ModelDelta {
    match delta {
        InstanceDelta::Graph(delta) => ModelDelta::Graph(graph_delta_to_reference(delta)),
        InstanceDelta::Table(delta) => ModelDelta::Table(table_delta_to_reference(delta)),
        InstanceDelta::Tree(delta) => ModelDelta::Trees {
            records: delta.records.iter().map(graph_delta_to_reference).collect(),
            removed: delta.removed.iter().map(tree_to_reference).collect(),
            appended: delta.appended.iter().map(tree_to_reference).collect(),
        },
    }
}

fn instance_to_production(instance: &ModelInstance) -> Instance {
    match instance {
        ModelInstance::Graph(graph) => Instance::Graph(graph_to_production(graph, false)),
        ModelInstance::Table(table) => Instance::Table(table_to_production(table, false)),
        ModelInstance::Trees(trees) => {
            Instance::Tree(trees.iter().map(tree_to_production).collect())
        }
    }
}

/// The reference instance of a production one, trees normalized.
fn instance_to_reference(instance: &Instance) -> ModelInstance {
    match instance {
        Instance::Graph(graph) => ModelInstance::Graph(graph_to_reference(graph)),
        Instance::Table(table) => ModelInstance::Table(table_to_reference(table)),
        Instance::Tree(trees) => ModelInstance::Trees(
            trees
                .iter()
                .map(|tree| normalized(&tree_to_reference(tree)))
                .collect(),
        ),
    }
}

fn normalized_instance(instance: &ModelInstance) -> ModelInstance {
    match instance {
        ModelInstance::Trees(trees) => ModelInstance::Trees(trees.iter().map(normalized).collect()),
        other => other.clone(),
    }
}

/// The production schema of a coverage schema: its vertices as objects and
/// its edges of the one kind.
fn production_schema(schema: &carrier::CoverageSchema) -> panproto_schema::Schema {
    let protocol = panproto_schema::Protocol {
        name: "test".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        obj_kinds: vec!["object".into()],
        ..Default::default()
    };
    if schema.vertices.is_empty() {
        return panproto_schema::Schema {
            protocol: "test".into(),
            ..Default::default()
        };
    }
    let mut builder = panproto_schema::SchemaBuilder::new(&protocol);
    for vertex in &schema.vertices {
        builder = builder.vertex(vertex, "object", None).unwrap();
    }
    for edge in &schema.edges {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .unwrap();
    }
    builder.build().unwrap()
}

/// What a pair's diff and application give, as the reference names it.
fn production_diff_and_apply(
    schema: &panproto_schema::Schema,
    before: &Instance,
    after: &Instance,
) -> Result<(ModelDelta, ModelInstance), ModelRefusal> {
    let refusal = |error: VcsError| match error {
        VcsError::ShapeMismatch { .. } => ModelRefusal::ShapeMismatch,
        VcsError::DeltaDoesNotApply { .. } => ModelRefusal::DoesNotApply,
        other => panic!("an untyped refusal: {other}"),
    };
    let delta = instance_vc::diff(schema, before, after).map_err(refusal)?;
    let applied = instance_vc::apply(schema, &delta, before).map_err(refusal)?;
    Ok((delta_to_reference(&delta), instance_to_reference(&applied)))
}

fn reference_diff_and_apply(
    before: &ModelInstance,
    after: &ModelInstance,
) -> Result<(ModelDelta, ModelInstance), ModelRefusal> {
    let delta = model_diff(before, after)?;
    let applied = model_apply(&delta, before)?;
    Ok((delta, normalized_instance(&applied)))
}

#[test]
fn every_pair_of_small_coverage_instances_diffs_and_applies_as_the_reference_does() {
    let mut store = MemStore::new();
    let mut pairs = 0usize;
    let mut expected_pairs = 0usize;
    let mut states = 0usize;
    let mut states_by_shape: BTreeMap<InstanceShape, usize> = BTreeMap::new();
    for (index, schema) in carrier::coverage_schemas().iter().enumerate() {
        let production = production_schema(schema);
        let instances = small_instances(schema, 2);
        states += instances.len();
        // Every instance passes through a stored data set first.
        let stored: Vec<Instance> = instances
            .iter()
            .map(|instance| {
                let dataset = DataSetObject::from_instance(
                    schema_id(index),
                    &instance_to_production(instance),
                    None,
                )
                .unwrap();
                stored_instance(&mut store, dataset).1
            })
            .collect();
        let mut by_shape: BTreeMap<InstanceShape, usize> = BTreeMap::new();
        for instance in &stored {
            *by_shape.entry(instance.shape()).or_default() += 1;
            *states_by_shape.entry(instance.shape()).or_default() += 1;
        }
        expected_pairs += by_shape.values().map(|n| n * n).sum::<usize>();
        for (a, (before, before_model)) in stored.iter().zip(&instances).enumerate() {
            for (b, (after, after_model)) in stored.iter().zip(&instances).enumerate() {
                if before.shape() != after.shape() {
                    // One cross-shape pair per ordered pair of shapes is
                    // enough to see the refusal.
                    if a + 1 == b || b + 1 == a {
                        assert_eq!(
                            production_diff_and_apply(&production, before, after),
                            Err(ModelRefusal::ShapeMismatch)
                        );
                    }
                    continue;
                }
                let expected = reference_diff_and_apply(before_model, after_model);
                assert_eq!(
                    production_diff_and_apply(&production, before, after),
                    expected,
                    "{before_model:?} to {after_model:?}"
                );
                let (_, applied) = expected.unwrap();
                assert_eq!(applied, normalized_instance(after_model));
                pairs += 1;
            }
        }
    }
    // The edits page counts 3,178 graphs with at most two nodes and 1,852
    // tables with at most two rows over the sixteen schemas; 240 trees have
    // at most two nodes. The pairs are the sum over schemas and shapes of
    // the squared number of states.
    assert_eq!(states_by_shape[&InstanceShape::Graph], 3_178);
    assert_eq!(states_by_shape[&InstanceShape::Table], 1_852);
    assert_eq!(states_by_shape[&InstanceShape::Tree], 240);
    assert_eq!(states, 5_270);
    assert_eq!(pairs, expected_pairs);
    assert_eq!(pairs, 1_303_058);
}

// ---------------------------------------------------------------------------
// Tracking, staging and staleness.
// ---------------------------------------------------------------------------

/// Commit `choices` on a fresh store, each commit recording a graph data
/// set under each key chosen `true` and stopping tracking each key chosen
/// `false`; return the store, the commits oldest first and the reference's
/// record of each commit.
/// What one commit records under each key, as the reference states it.
type ModelRecord = BTreeMap<String, Option<ModelEntry>>;

fn commit_chain(
    choices: &[Vec<(String, bool)>],
) -> (MemStore, Vec<CommitObject>, Vec<ModelRecord>) {
    let mut store = MemStore::new();
    let mut commits = Vec::new();
    let mut recorded = Vec::new();
    let mut parent: Option<ObjectId> = None;
    for (index, keys) in choices.iter().enumerate() {
        let mut data_ids = Vec::new();
        let mut untracked = Vec::new();
        let mut entries = BTreeMap::new();
        for (key, record) in keys {
            if !*record {
                untracked.push(key.clone());
                entries.insert(key.clone(), None);
                continue;
            }
            let graph = GInstance::new().with_node(Node::new(u32::try_from(index).unwrap(), "a"));
            let dataset = DataSetObject::from_instance(
                ObjectId::ZERO,
                &Instance::Graph(graph.clone()),
                Some(key.clone()),
            )
            .unwrap();
            let id = store.put(&Object::DataSet(dataset)).unwrap();
            data_ids.push(id);
            entries.insert(
                key.clone(),
                Some(ModelEntry {
                    data: id.to_string(),
                    schema: ObjectId::ZERO.to_string(),
                    instance: ModelInstance::Graph(graph_to_reference(&graph)),
                }),
            );
        }
        let commit = CommitObject::builder(ObjectId::ZERO, "test", "test", format!("c{index}"))
            .timestamp(0)
            .parents(parent.into_iter().collect())
            .data_ids(data_ids)
            .untracked(untracked)
            .build();
        parent = Some(store.put(&Object::Commit(commit.clone())).unwrap());
        commits.push(commit);
        recorded.push(entries);
    }
    (store, commits, recorded)
}

fn check_tracking(choices: &[Vec<(String, bool)>]) -> usize {
    let (store, commits, recorded) = commit_chain(choices);
    let mut checked = 0;
    for (index, commit) in commits.iter().enumerate() {
        let chain: Vec<BTreeMap<String, Option<ModelEntry>>> =
            recorded[..=index].iter().rev().cloned().collect();
        let expected: BTreeMap<String, String> = model_tracked(&chain)
            .into_iter()
            .map(|(key, entry)| (key, entry.data))
            .collect();
        let actual: BTreeMap<String, String> = instance_vc::tracked(&store, commit)
            .unwrap()
            .into_iter()
            .map(|(key, (id, _))| (key, id.to_string()))
            .collect();
        assert_eq!(actual, expected, "{choices:?} at commit {index}");
        checked += 1;
    }
    checked
}

#[test]
fn every_commit_chain_of_the_tracking_scope_tracks_what_the_reference_tracks() {
    let mut chains = 0usize;
    let mut commits = 0usize;
    for length in 1..=3 {
        for choices in panproto_reference::enumerate::instance_version_control::tracking_chains(
            length,
            &["k1", "k2"],
        ) {
            commits += check_tracking(&choices);
            chains += 1;
        }
    }
    // Each commit leaves each of two keys alone, records it or stops
    // tracking it: 9 choices per commit, so 9 + 81 + 729 chains holding
    // 9 + 2 * 81 + 3 * 729 commits.
    assert_eq!((chains, commits), (819, 2_358));
}

#[test]
fn a_commit_recording_two_data_sets_under_one_key_is_refused_when_tracked() {
    let mut store = MemStore::new();
    let ids: Vec<ObjectId> = [0u32, 1]
        .iter()
        .map(|n| {
            let dataset = DataSetObject::from_instance(
                ObjectId::ZERO,
                &Instance::Graph(GInstance::new().with_node(Node::new(*n, "a"))),
                Some("k".to_owned()),
            )
            .unwrap();
            store.put(&Object::DataSet(dataset)).unwrap()
        })
        .collect();
    let commit = CommitObject::builder(ObjectId::ZERO, "test", "test", "two")
        .timestamp(0)
        .data_ids(ids)
        .build();
    assert!(matches!(
        instance_vc::tracked(&store, &commit),
        Err(VcsError::DuplicateDataKey { key }) if key == "k"
    ));
}

/// A repository over the coverage schema with vertices `a` and `b` and the
/// edge from `a` to `b`, committed once.
fn instance_repository() -> (tempfile::TempDir, Repository, carrier::CoverageSchema) {
    let schema = carrier::coverage_schemas()
        .into_iter()
        .find(|s| s.vertices.len() == 2 && s.edges.len() == 1 && s.edges[0].src != s.edges[0].tgt)
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.set_protocol(panproto_schema::Protocol {
        name: "test".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        obj_kinds: vec!["object".into()],
        ..Default::default()
    });
    repo.add(&production_schema(&schema)).unwrap();
    repo.commit("schema", "test").unwrap();
    (dir, repo, schema)
}

#[test]
fn committed_instances_of_every_shape_diff_by_their_delta_through_a_repository() {
    let (_dir, mut repo, schema) = instance_repository();
    let production = production_schema(&schema);
    let instances = small_instances(&schema, 2);
    // The coverage states carry a field `f` the schema does not declare;
    // validation is checked on its own, so these are staged unverified.
    let options = panproto_vcs::AddDataOptions {
        skip_verify: true,
        ..Default::default()
    };
    let unverified = panproto_vcs::CommitOptions { skip_verify: true };
    let mut checked = 0usize;
    // Consecutive instances of each shape, every 17th pair.
    for shape in [
        InstanceShape::Graph,
        InstanceShape::Table,
        InstanceShape::Tree,
    ] {
        let of_shape: Vec<&ModelInstance> = instances
            .iter()
            .filter(|i| instance_to_production(i).shape() == shape)
            .collect();
        for pair in of_shape.windows(2).step_by(17) {
            let (before, after) = (pair[0], pair[1]);
            repo.add_instance("x", &instance_to_production(before), &options)
                .unwrap();
            let first = repo
                .commit_with_options("before", "test", &unverified)
                .unwrap()
                .to_string();
            repo.add_instance("x", &instance_to_production(after), &options)
                .unwrap();
            let second = repo
                .commit_with_options("after", "test", &unverified)
                .unwrap()
                .to_string();
            let read = repo.committed_instance(&second, "x").unwrap();
            assert!(!read.stale);
            assert_eq!(
                instance_to_reference(&read.instance),
                normalized_instance(after)
            );
            let changes = repo.diff_data(&first, &second).unwrap();
            let expected = model_diff(before, after).unwrap();
            match changes.as_slice() {
                [] => assert_eq!(before, after),
                [panproto_vcs::InstanceChange::Changed { key, delta, .. }] => {
                    assert_eq!(key, "x");
                    assert_eq!(delta_to_reference(delta), expected);
                    let applied = instance_vc::apply(
                        &production,
                        delta,
                        &repo.committed_instance(&first, "x").unwrap().instance,
                    )
                    .unwrap();
                    assert_eq!(instance_to_reference(&applied), normalized_instance(after));
                }
                other => panic!("unexpected changes {other:?}"),
            }
            checked += 1;
        }
    }
    assert!(checked >= 3);
}

#[test]
fn diffing_across_keys_reports_added_and_removed_and_refuses_mixed_schemas_and_shapes() {
    let (_dir, mut repo, schema) = instance_repository();
    let options = panproto_vcs::AddDataOptions::default();
    let instances = small_instances(&schema, 1);
    let graph = instances
        .iter()
        .find(|i| matches!(i, ModelInstance::Graph(g) if !g.nodes.is_empty()))
        .unwrap();
    let table = instances
        .iter()
        .find(|i| matches!(i, ModelInstance::Table(t) if !t.rows.is_empty()))
        .unwrap();

    repo.add_instance("g", &instance_to_production(graph), &options)
        .unwrap();
    let first = repo.commit("g", "test").unwrap().to_string();
    repo.add_instance("t", &instance_to_production(table), &options)
        .unwrap();
    let second = repo.commit("t", "test").unwrap().to_string();

    let forward = repo.diff_data(&first, &second).unwrap();
    assert!(
        matches!(forward.as_slice(), [panproto_vcs::InstanceChange::Added { key, .. }] if key == "t")
    );
    let backward = repo.diff_data(&second, &first).unwrap();
    assert!(
        matches!(backward.as_slice(), [panproto_vcs::InstanceChange::Removed { key, .. }] if key == "t")
    );

    // A key whose shape changes.
    repo.add_instance("g", &instance_to_production(table), &options)
        .unwrap();
    let third = repo.commit("g as a table", "test").unwrap().to_string();
    assert!(matches!(
        repo.diff_data(&second, &third),
        Err(VcsError::ShapeMismatch { .. })
    ));

    // A schema change leaves the tracked instances stale, and a key whose
    // data sets conform to two schemas is not diffed.
    let mut wider = schema;
    wider.vertices.push("c".to_owned());
    repo.add(&production_schema(&wider)).unwrap();
    let fourth = repo.commit("wider", "test").unwrap().to_string();
    assert!(repo.committed_instance(&fourth, "t").unwrap().stale);
    repo.add_instance("t", &instance_to_production(table), &options)
        .unwrap();
    let fifth = repo.commit("t again", "test").unwrap().to_string();
    assert!(!repo.committed_instance(&fifth, "t").unwrap().stale);
    assert!(matches!(
        repo.diff_data(&fourth, &fifth),
        Err(VcsError::SchemaMismatch { key, .. }) if key == "t"
    ));
    let model_before = BTreeMap::from([(
        "t".to_owned(),
        ModelEntry {
            data: "1".into(),
            schema: "s".into(),
            instance: table.clone(),
        },
    )]);
    let model_after = BTreeMap::from([(
        "t".to_owned(),
        ModelEntry {
            data: "2".into(),
            schema: "s2".into(),
            instance: table.clone(),
        },
    )]);
    assert_eq!(
        model_diff_tracked(&model_before, &model_after),
        Err(ModelRefusal::SchemaMismatch("t".to_owned()))
    );
}

#[test]
fn an_instance_failing_its_shapes_validation_is_refused_with_its_report() {
    let (_dir, mut repo, _schema) = instance_repository();
    let options = panproto_vcs::AddDataOptions::default();
    for instance in [
        Instance::Graph(GInstance::new().with_node(Node::new(0, "nowhere"))),
        Instance::Table(
            FInstance::new()
                .with_table("nowhere", [HashMap::new()])
                .unwrap(),
        ),
        Instance::Tree(vec![WInstance::new(
            HashMap::from([(0, Node::new(0, "nowhere"))]),
            Vec::new(),
            Vec::new(),
            0,
            "nowhere".into(),
        )]),
    ] {
        match repo.add_instance("bad", &instance, &options) {
            Err(VcsError::InstanceValidationFailed { key, failing }) => {
                assert_eq!(key, "bad");
                assert_eq!(failing.len(), 1);
                assert_eq!(failing[0].0, 0);
                assert_eq!(failing[0].1.status, panproto_gat::CheckStatus::Failed);
            }
            other => panic!("expected a validation refusal, got {other:?}"),
        }
        assert_eq!(repo.read_index().unwrap().staged_data.len(), 0);

        // Skipping verification stages it pending, which a default commit
        // refuses.
        repo.add_instance(
            "bad",
            &instance,
            &panproto_vcs::AddDataOptions {
                skip_verify: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(matches!(
            repo.commit("bad", "test"),
            Err(VcsError::ValidationPending { .. })
        ));
        repo.clear_index().unwrap();
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_commit_chains_track_what_the_reference_tracks(
        choices in proptest::collection::vec(
            proptest::collection::btree_map(
                prop_oneof![Just("a"), Just("b"), Just("c"), Just("d")],
                any::<bool>(),
                0..4,
            ),
            1..7,
        )
    ) {
        let choices: Vec<Vec<(String, bool)>> = choices
            .into_iter()
            .map(|keys| keys.into_iter().map(|(key, record)| (key.to_owned(), record)).collect())
            .collect();
        check_tracking(&choices);
    }
}

// ---------------------------------------------------------------------------
// Merge of tracked instances.
// ---------------------------------------------------------------------------

use panproto_inst::graph_edit::{EditConflict, ElemRef};
use panproto_reference::enumerate::instance_version_control::merge_states;
use panproto_reference::pages::instance_version_control::{
    Conflict as ModelConflict, Merged as ModelMerged, MigrationRefusal as ModelMigrationRefusal,
    PathRefusal, RecordConflict as ModelRecordConflict, Side as ModelSide,
    TrackingConflict as ModelTrackingConflict, Version as ModelVersion,
    merge_instances as model_merge_instances, merge_tracked as model_merge_tracked,
};
use panproto_vcs::instance_merge::{
    self, InstanceConflict, MergedEntry, RecordConflict, RecordDelta, TrackedMerge,
    TrackingConflict,
};
use panproto_vcs::merge::Side;

const fn side_to_reference(side: &Side) -> ModelSide {
    match side {
        Side::Ours => ModelSide::Ours,
        Side::Theirs => ModelSide::Theirs,
    }
}

fn gelem_to_reference(elem: &ElemRef) -> reference_edit::GElem {
    match elem {
        ElemRef::Node(id) => reference_edit::GElem::Node(*id),
        ElemRef::Edge(id) => reference_edit::GElem::Edge(id.0),
        other => panic!("not a graph element: {other:?}"),
    }
}

fn graph_ref_to_reference(elem: &ElemRef) -> reference_edit::ElemRef {
    match elem {
        ElemRef::Attr(key) => reference_edit::ElemRef::Attr(attr_key_to_reference(key)),
        other => reference_edit::ElemRef::Elem(gelem_to_reference(other)),
    }
}

fn graph_conflict_to_reference(conflict: &EditConflict) -> reference_edit::EditConflict {
    match conflict {
        EditConflict::DeleteDelete { elem } => reference_edit::EditConflict::DeleteDelete {
            elem: gelem_to_reference(elem),
        },
        EditConflict::DeleteUse { deleted, user } => reference_edit::EditConflict::DeleteUse {
            deleted: gelem_to_reference(deleted),
            user: graph_ref_to_reference(user),
        },
        EditConflict::UpdateUpdate { key } => reference_edit::EditConflict::UpdateUpdate {
            key: attr_key_to_reference(key),
        },
        EditConflict::AddAdd { elem } => reference_edit::EditConflict::AddAdd {
            elem: gelem_to_reference(elem),
        },
        EditConflict::AnchorUse { node, user } => reference_edit::EditConflict::AnchorUse {
            node: *node,
            user: graph_ref_to_reference(user),
        },
        EditConflict::FanUse { first, second } => reference_edit::EditConflict::FanUse {
            first: graph_ref_to_reference(first),
            second: graph_ref_to_reference(second),
        },
    }
}

fn telem_to_reference(elem: &ElemRef) -> reference_edit::TElem {
    match elem {
        ElemRef::Row(row) => reference_edit::TElem::Row(row.0),
        ElemRef::Pair { edge, src, tgt } => {
            reference_edit::TElem::Pair(edge_to_reference(edge), src.0, tgt.0)
        }
        other => panic!("not a table element: {other:?}"),
    }
}

fn tkey_to_reference(key: &AttrKey) -> reference_edit::TKey {
    match key {
        AttrKey::Cell { row, column } => reference_edit::TKey::Cell(row.0, column.clone()),
        AttrKey::Table { row } => reference_edit::TKey::Table(row.0),
        other => panic!("not a table entry: {other:?}"),
    }
}

fn table_use_to_reference(elem: &ElemRef) -> reference_edit::TableUse {
    match elem {
        ElemRef::Attr(key) => reference_edit::TableUse::Attr(tkey_to_reference(key)),
        other => reference_edit::TableUse::Elem(telem_to_reference(other)),
    }
}

fn table_conflict_to_reference(conflict: &EditConflict) -> reference_edit::TableConflict {
    match conflict {
        EditConflict::DeleteDelete { elem } => reference_edit::TableConflict::DeleteDelete {
            elem: telem_to_reference(elem),
        },
        EditConflict::DeleteUse { deleted, user } => reference_edit::TableConflict::DeleteUse {
            deleted: telem_to_reference(deleted),
            user: table_use_to_reference(user),
        },
        EditConflict::UpdateUpdate { key } => reference_edit::TableConflict::UpdateUpdate {
            key: tkey_to_reference(key),
        },
        EditConflict::AddAdd { elem } => reference_edit::TableConflict::AddAdd {
            elem: telem_to_reference(elem),
        },
        EditConflict::AnchorUse { node, user } => reference_edit::TableConflict::AnchorUse {
            row: *node,
            user: telem_to_reference(user),
        },
        EditConflict::FanUse { .. } => panic!("a table delta has no fan: {conflict:?}"),
    }
}

const fn version_to_reference(version: panproto_vcs::Version) -> ModelVersion {
    match version {
        panproto_vcs::Version::Base => ModelVersion::Base,
        panproto_vcs::Version::Ours => ModelVersion::Ours,
        panproto_vcs::Version::Theirs => ModelVersion::Theirs,
    }
}

/// The reference refusal of a production migration conflict met on a
/// merge's migration path; commits and migrations are named by identifier.
fn migration_refusal_to_reference(
    conflict: &panproto_vcs::MigrationConflict,
) -> ModelMigrationRefusal {
    use panproto_vcs::MigrationConflict;
    match conflict {
        MigrationConflict::NoMigrationPath { .. } => {
            ModelMigrationRefusal::Path(PathRefusal::NoPath)
        }
        MigrationConflict::MissingLink { commit } => {
            ModelMigrationRefusal::Path(PathRefusal::MissingLink(commit.to_string()))
        }
        MigrationConflict::LinkMismatch { commit, .. } => {
            ModelMigrationRefusal::Path(PathRefusal::LinkMismatch(commit.to_string()))
        }
        MigrationConflict::MigrationDoesNotCompile { migration_id, .. } => {
            ModelMigrationRefusal::DoesNotCompile(migration_id.to_string())
        }
        MigrationConflict::OutsideGetDomain { record, .. } => {
            ModelMigrationRefusal::GetRefused(*record)
        }
        other => panic!("a merge migration refused otherwise: {other:?}"),
    }
}

fn conflict_to_reference(conflict: &InstanceConflict) -> ModelConflict {
    match conflict {
        InstanceConflict::Migration {
            key,
            version,
            conflict,
        } => ModelConflict::Migration(
            key.clone(),
            version_to_reference(*version),
            migration_refusal_to_reference(conflict),
        ),
        InstanceConflict::Tracking { key, conflict } => ModelConflict::Tracking(
            key.clone(),
            match conflict {
                TrackingConflict::AddedOnBothSides { .. } => {
                    ModelTrackingConflict::AddedOnBothSides
                }
                TrackingConflict::RemovedAndChanged { removed_by } => {
                    ModelTrackingConflict::RemovedAndChanged(side_to_reference(removed_by))
                }
                TrackingConflict::ShapeDiffers { .. } => ModelTrackingConflict::ShapeDiffers,
                TrackingConflict::SchemaDiffers { .. } => ModelTrackingConflict::SchemaDiffers,
            },
        ),
        InstanceConflict::Edit {
            key,
            record,
            ours,
            conflict,
            ..
        } => match ours.as_ref() {
            RecordDelta::Table(_) => {
                ModelConflict::TableEdit(key.clone(), table_conflict_to_reference(conflict))
            }
            RecordDelta::Graph(_) => ModelConflict::GraphEdit(
                key.clone(),
                *record,
                graph_conflict_to_reference(conflict),
            ),
        },
        InstanceConflict::Record {
            key,
            record,
            conflict,
        } => ModelConflict::Record(
            key.clone(),
            *record,
            match conflict {
                RecordConflict::RemovedByBoth => ModelRecordConflict::RemovedByBoth,
                RecordConflict::RemovedAndChanged { removed_by } => {
                    ModelRecordConflict::RemovedAndChanged(side_to_reference(removed_by))
                }
                RecordConflict::AppendedByBoth => ModelRecordConflict::AppendedByBoth,
                RecordConflict::NotATree { .. } => ModelRecordConflict::NotATree,
            },
        ),
        other => panic!("a validation conflict where none is checked: {other:?}"),
    }
}

/// What a three-way merge of instances gives, as the reference states it.
type MergeObservation = Result<Result<ModelInstance, Vec<ModelConflict>>, ModelRefusal>;

fn production_merge(
    schema: &panproto_schema::Schema,
    [base, ours, theirs]: [&Instance; 3],
) -> MergeObservation {
    match instance_merge::merge_instances("k", schema, base, ours, theirs) {
        Ok(Ok(merged)) => Ok(Ok(instance_to_reference(&merged))),
        Ok(Err(conflicts)) => {
            let mut sorted = conflicts.clone();
            sorted.sort_by(InstanceConflict::listing_order);
            assert_eq!(sorted, conflicts, "conflicts are listed in their order");
            Ok(Err(conflicts.iter().map(conflict_to_reference).collect()))
        }
        Err(VcsError::DeltaDoesNotApply { .. }) => Err(ModelRefusal::DoesNotApply),
        Err(VcsError::ShapeMismatch { .. }) => Err(ModelRefusal::ShapeMismatch),
        Err(other) => panic!("an untyped refusal: {other}"),
    }
}

fn reference_merge([base, ours, theirs]: [&ModelInstance; 3]) -> MergeObservation {
    model_merge_instances("k", base, ours, theirs)
        .map(|merged| merged.map(|instance| normalized_instance(&instance)))
}

/// The counts of a run over the merge scope.
#[derive(Debug, Default, PartialEq, Eq)]
struct MergeCounts {
    triples: [usize; 3],
    merged: [usize; 3],
    conflicted: [usize; 3],
    conflicts: [usize; 3],
}

/// Merge every triple of each of the first `shapes` shapes' states of the
/// merge scope over `schema` (graphs, tables, then sequences of trees),
/// against the reference, counting into `counts` and rendering each
/// observation into `rendering`.
fn merge_scope_of_schema(
    schema: &carrier::CoverageSchema,
    shapes: usize,
    counts: &mut MergeCounts,
    rendering: &mut Vec<String>,
) {
    let production = production_schema(schema);
    for (shape, states) in merge_states(schema).iter().enumerate().take(shapes) {
        let built: Vec<Instance> = states.iter().map(instance_to_production).collect();
        for b in 0..states.len() {
            for o in 0..states.len() {
                for t in 0..states.len() {
                    let actual = production_merge(&production, [&built[b], &built[o], &built[t]]);
                    let expected = reference_merge([&states[b], &states[o], &states[t]]);
                    assert_eq!(
                        actual,
                        expected,
                        "{:?}",
                        [&states[b], &states[o], &states[t]]
                    );
                    counts.triples[shape] += 1;
                    match &actual {
                        Ok(Ok(_)) => counts.merged[shape] += 1,
                        Ok(Err(found)) => {
                            counts.conflicted[shape] += 1;
                            counts.conflicts[shape] += found.len();
                        }
                        Err(refusal) => panic!("a scope merge was refused: {refusal:?}"),
                    }
                    rendering.push(format!("{actual:?}"));
                }
            }
        }
    }
}

fn merge_scope_rendering(shapes: usize) -> (MergeCounts, String) {
    let mut counts = MergeCounts::default();
    let mut rendering = Vec::new();
    for schema in carrier::coverage_schemas() {
        merge_scope_of_schema(&schema, shapes, &mut counts, &mut rendering);
    }
    (counts, rendering.join("\n"))
}

#[test]
fn every_triple_of_the_merge_scope_merges_as_the_reference_merges() {
    if std::env::var_os("PP_INSTANCE_MERGE_DIGEST").is_some() {
        return;
    }
    let (counts, _) = merge_scope_rendering(3);
    // The triples are the cubes of each schema's number of states of each
    // shape: graphs with at most one node, tables with at most one row,
    // and sequences of at most two one-node trees (1 + n + n^2 of them for
    // n one-node trees).
    let mut expected_triples = [0usize; 3];
    for schema in carrier::coverage_schemas() {
        for (shape, states) in merge_states(&schema).iter().enumerate() {
            expected_triples[shape] += states.len().pow(3);
        }
        let ones = carrier::coverage_trees(&schema)
            .into_iter()
            .filter(|tree| tree.nodes.len() == 1)
            .count();
        assert_eq!(merge_states(&schema)[2].len(), 1 + ones + ones * ones);
    }
    assert_eq!(counts.triples, expected_triples);
    assert_eq!(counts.triples, [23_542, 10_384, 883_366]);
    assert_eq!(counts.merged, [5_218, 2_788, 118_450]);
    assert_eq!(counts.conflicted, [18_324, 7_596, 764_916]);
    assert_eq!(counts.conflicts, [36_204, 11_694, 1_254_852]);
}

/// The digest of the rendering of every graph and table merge of the merge
/// scope.
const INSTANCE_MERGES_DIGEST: u64 = 0x000a_cafc_7edb_4a3e;

/// The child half: one process's digest of the graph and table merges of
/// the merge scope, printed.
#[test]
fn one_process_instance_merge_digest() {
    if std::env::var_os("PP_INSTANCE_MERGE_DIGEST").is_none() {
        return;
    }
    print!(
        "<<<{:016x}>>>",
        panproto_reference::pages::migration_search::digest(merge_scope_rendering(2).1.as_bytes())
    );
}

#[test]
fn instance_merges_are_the_same_in_every_process() {
    if std::env::var_os("PP_INSTANCE_MERGE_DIGEST").is_some() {
        return;
    }
    let here = format!(
        "{:016x}",
        panproto_reference::pages::migration_search::digest(merge_scope_rendering(2).1.as_bytes())
    );
    let Ok(exe) = std::env::current_exe() else {
        panic!("the test binary does not know its own path");
    };
    for _ in 0..3 {
        let output = std::process::Command::new(&exe)
            .args([
                "one_process_instance_merge_digest",
                "--exact",
                "--nocapture",
            ])
            .env("PP_INSTANCE_MERGE_DIGEST", "1")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let (Some(start), Some(end)) = (text.find("<<<"), text.find(">>>")) else {
            panic!("the child printed no digest: {text}");
        };
        assert_eq!(
            &text[start + 3..end],
            here,
            "a child process merged differently"
        );
    }
    assert_eq!(
        here,
        format!("{INSTANCE_MERGES_DIGEST:016x}"),
        "the merges moved from the pinned digest"
    );
}

// The key scope: two keys, each tracked by the base as nothing or a graph
// `g0`, and by each side as it inherits, stops tracking, or records one of
// five data sets.

/// What a side does to a key.
#[derive(Clone, Copy, Debug)]
enum SideChoice {
    Inherit,
    Remove,
    Record(usize),
}

const SIDE_CHOICES: [SideChoice; 7] = [
    SideChoice::Inherit,
    SideChoice::Remove,
    SideChoice::Record(1),
    SideChoice::Record(2),
    SideChoice::Record(3),
    SideChoice::Record(4),
    SideChoice::Record(5),
];

/// Data set `index` of the key scope under `key`: `g0` holds node 0; `g1`
/// adds node 1 at `a`, `g2` node 2 at `a`, `g3` node 1 at `b`; `t0` is a
/// table and `s2` is `g0` under another schema.
fn scope_dataset(index: usize, key: &str) -> (DataSetObject, ModelEntry) {
    let graph = |extra: Option<(u32, &str)>| {
        let mut graph = GInstance::new().with_node(Node::new(0, "a"));
        if let Some((id, anchor)) = extra {
            graph = graph.with_node(Node::new(id, anchor));
        }
        Instance::Graph(graph)
    };
    let other_schema = ObjectId::from_bytes([1; 32]);
    let (schema, instance) = match index {
        0 => (ObjectId::ZERO, graph(None)),
        1 => (ObjectId::ZERO, graph(Some((1, "a")))),
        2 => (ObjectId::ZERO, graph(Some((2, "a")))),
        3 => (ObjectId::ZERO, graph(Some((1, "b")))),
        4 => (
            ObjectId::ZERO,
            Instance::Table(FInstance::new().with_table("a", [HashMap::new()]).unwrap()),
        ),
        _ => (other_schema, graph(None)),
    };
    let dataset = DataSetObject::from_instance(schema, &instance, Some(key.to_owned())).unwrap();
    let entry = ModelEntry {
        data: panproto_vcs::hash::hash_dataset(&dataset)
            .unwrap()
            .to_string(),
        schema: schema.to_string(),
        instance: instance_to_reference(&instance),
    };
    (dataset, entry)
}

const SCOPE_KEYS: [&str; 2] = ["k1", "k2"];

/// The production and reference merge of one repository of the key scope.
fn merge_key_scope_case(
    base_has: [bool; 2],
    sides: [[SideChoice; 2]; 2],
) -> (
    TrackedMerge,
    BTreeMap<String, ModelMerged>,
    Vec<ModelConflict>,
) {
    let mut store = MemStore::new();
    let mut base_ids = Vec::new();
    let mut base_model = BTreeMap::new();
    for (key, has) in SCOPE_KEYS.iter().zip(base_has) {
        if has {
            let (dataset, entry) = scope_dataset(0, key);
            base_ids.push(store.put(&Object::DataSet(dataset)).unwrap());
            base_model.insert((*key).to_owned(), Some(entry));
        }
    }
    let base = CommitObject::builder(ObjectId::ZERO, "test", "test", "base")
        .timestamp(0)
        .data_ids(base_ids)
        .build();
    let base_id = store.put(&Object::Commit(base.clone())).unwrap();
    let mut side = |choices: [SideChoice; 2], name: &str| {
        let mut data_ids = Vec::new();
        let mut untracked = Vec::new();
        let mut model = BTreeMap::new();
        for (key, choice) in SCOPE_KEYS.iter().zip(choices) {
            match choice {
                SideChoice::Inherit => {}
                SideChoice::Remove => {
                    untracked.push((*key).to_owned());
                    model.insert((*key).to_owned(), None);
                }
                SideChoice::Record(index) => {
                    let (dataset, entry) = scope_dataset(index, key);
                    data_ids.push(store.put(&Object::DataSet(dataset)).unwrap());
                    model.insert((*key).to_owned(), Some(entry));
                }
            }
        }
        let commit = CommitObject::builder(ObjectId::ZERO, "test", "test", name)
            .timestamp(0)
            .parents(vec![base_id])
            .data_ids(data_ids)
            .untracked(untracked)
            .build();
        store.put(&Object::Commit(commit.clone())).unwrap();
        (commit, model_tracked(&[model, base_model.clone()]))
    };
    let (ours_commit, ours_model) = side(sides[0], "ours");
    let (theirs_commit, theirs_model) = side(sides[1], "theirs");
    let base_model = model_tracked(&[base_model]);
    let tracked = |commit: &CommitObject| instance_vc::tracked(&store, commit).unwrap();
    let schema = panproto_schema::Schema {
        protocol: "test".into(),
        ..Default::default()
    };
    let mut load = |dataset: &DataSetObject| Ok((schema.clone(), dataset.decode()?));
    let mut validate = |_: &panproto_schema::Schema, _: &Instance| Ok(Vec::new());
    let merge = instance_merge::merge_tracked(
        &tracked(&base),
        &tracked(&ours_commit),
        &tracked(&theirs_commit),
        &mut load,
        &mut validate,
    )
    .unwrap();
    let (merged, conflicts) = model_merge_tracked(&base_model, &ours_model, &theirs_model).unwrap();
    (merge, merged, conflicts)
}

fn merged_to_reference(entry: &MergedEntry) -> ModelMerged {
    match entry {
        MergedEntry::Kept(id) => ModelMerged::Kept(id.to_string()),
        MergedEntry::Merged {
            schema_id,
            instance,
        } => ModelMerged::New(schema_id.to_string(), instance_to_reference(instance)),
    }
}

#[test]
fn every_repository_of_the_key_scope_merges_its_tracked_instances_as_the_reference_does() {
    let mut repositories = 0usize;
    let mut clean = 0usize;
    let mut conflicts = 0usize;
    let mut entries = 0usize;
    for base_has in [[false, false], [false, true], [true, false], [true, true]] {
        for ours in SIDE_CHOICES
            .iter()
            .flat_map(|a| SIDE_CHOICES.map(|b| [*a, b]))
        {
            for theirs in SIDE_CHOICES
                .iter()
                .flat_map(|a| SIDE_CHOICES.map(|b| [*a, b]))
            {
                let (merge, expected_merged, expected_conflicts) =
                    merge_key_scope_case(base_has, [ours, theirs]);
                let merged: BTreeMap<String, ModelMerged> = merge
                    .merged
                    .iter()
                    .map(|(key, entry)| (key.clone(), merged_to_reference(entry)))
                    .collect();
                let found: Vec<ModelConflict> =
                    merge.conflicts.iter().map(conflict_to_reference).collect();
                assert_eq!(
                    (&merged, &found),
                    (&expected_merged, &expected_conflicts),
                    "{base_has:?} {ours:?} {theirs:?}"
                );
                repositories += 1;
                clean += usize::from(found.is_empty());
                conflicts += found.len();
                entries += merged.len();
            }
        }
    }
    // Per key, 2 * 49 cases: 52 without a conflict (7 tracking nothing,
    // 41 keeping a side's data set, 4 merging g1 or g3 with g2) and 46 with
    // one conflict each (20 added on both sides, 10 removed and changed, 8
    // shapes, 6 schemas, 2 adding node 1 on both sides).
    assert_eq!(repositories, 98 * 98);
    assert_eq!(clean, 52 * 52);
    assert_eq!(conflicts, 2 * 46 * 98);
    assert_eq!(entries, 2 * 45 * 98);
}

fn graph_of(nodes: &[(u32, &str)]) -> Instance {
    let mut graph = GInstance::new();
    for (id, anchor) in nodes {
        graph = graph.with_node(Node::new(*id, *anchor));
    }
    Instance::Graph(graph)
}

/// A repository over the instance repository's schema whose `main` and
/// `feature` branches each made one commit on a common base; `stage` stages
/// on the base, and `ours` and `theirs` on each branch.
fn diverged_repository(
    stage: impl Fn(&mut Repository),
    ours: impl Fn(&mut Repository),
    theirs: impl Fn(&mut Repository),
) -> (tempfile::TempDir, Repository) {
    let (dir, mut repo, _) = instance_repository();
    stage(&mut repo);
    let base = repo.commit("base", "test").unwrap();
    panproto_vcs::refs::create_branch(repo.store_mut(), "feature", base).unwrap();
    ours(&mut repo);
    repo.commit("ours", "test").unwrap();
    panproto_vcs::refs::checkout_branch(repo.store_mut(), "feature").unwrap();
    theirs(&mut repo);
    repo.commit("theirs", "test").unwrap();
    panproto_vcs::refs::checkout_branch(repo.store_mut(), "main").unwrap();
    (dir, repo)
}

fn head_commit(repo: &Repository) -> CommitObject {
    let id = panproto_vcs::store::resolve_head(repo.store())
        .unwrap()
        .unwrap();
    let Object::Commit(commit) = repo.store().get(&id).unwrap() else {
        panic!("HEAD is a commit");
    };
    commit
}

fn stage(repo: &mut Repository, key: &str, instance: &Instance) {
    repo.add_instance(key, instance, &panproto_vcs::AddDataOptions::default())
        .unwrap();
}

#[test]
fn a_repository_merge_records_one_data_set_per_key_and_tracks_the_merged_instances() {
    let (_dir, mut repo) = diverged_repository(
        |repo| {
            stage(repo, "g", &graph_of(&[(0, "a")]));
            stage(repo, "r", &graph_of(&[(0, "a")]));
        },
        |repo| stage(repo, "g", &graph_of(&[(0, "a"), (1, "a")])),
        |repo| {
            stage(repo, "g", &graph_of(&[(0, "a"), (2, "a")]));
            stage(repo, "h", &graph_of(&[(5, "b")]));
            repo.remove_instance("r").unwrap();
        },
    );
    let result = repo.merge("feature", "test").unwrap();
    assert_eq!(result.instance_conflicts, vec![]);
    let merge = head_commit(&repo);
    assert_eq!(merge.parents.len(), 2);
    assert_eq!(
        merge.data_ids.len(),
        2,
        "one data set per key the merge tracks"
    );
    assert_eq!(merge.untracked, vec!["r".to_owned()]);
    let tracked = instance_vc::tracked(repo.store(), &merge).unwrap();
    assert_eq!(tracked.keys().collect::<Vec<_>>(), vec!["g", "h"]);
    assert_eq!(
        instance_to_reference(&repo.committed_instance("HEAD", "g").unwrap().instance),
        instance_to_reference(&graph_of(&[(0, "a"), (1, "a"), (2, "a")]))
    );
    assert!(matches!(
        repo.committed_instance("HEAD", "r"),
        Err(VcsError::RefNotFound { .. })
    ));
}

#[test]
fn a_repository_merge_with_instance_conflicts_records_nothing_until_they_are_resolved() {
    use panproto_vcs::merge::ConflictResolution::{ChooseOurs, ChooseTheirs};
    let (_dir, mut repo) = diverged_repository(
        |repo| stage(repo, "g", &graph_of(&[(0, "a")])),
        |repo| stage(repo, "g", &graph_of(&[(0, "a"), (1, "a"), (2, "a")])),
        |repo| stage(repo, "g", &graph_of(&[(0, "a"), (1, "b"), (2, "b")])),
    );
    let before = head_commit(&repo);
    let result = repo.merge("feature", "test").unwrap();
    let found: Vec<ModelConflict> = result
        .instance_conflicts
        .iter()
        .map(conflict_to_reference)
        .collect();
    let add_add = |n| {
        ModelConflict::GraphEdit(
            "g".to_owned(),
            None,
            reference_edit::EditConflict::AddAdd {
                elem: reference_edit::GElem::Node(n),
            },
        )
    };
    assert_eq!(found, vec![add_add(1), add_add(2)]);
    assert_eq!(
        head_commit(&repo).message,
        before.message,
        "nothing recorded"
    );

    let resolve = |choices: &[(usize, panproto_vcs::merge::ConflictResolution)]| {
        panproto_vcs::merge::MergeOptions {
            instance_resolutions: panproto_vcs::merge::ResolutionStrategy {
                resolutions: choices.iter().cloned().collect(),
            },
            ..Default::default()
        }
    };
    assert!(matches!(
        repo.merge_with_options("feature", "test", &resolve(&[(0, ChooseTheirs)])),
        Err(VcsError::UnresolvedInstanceConflicts { count: 1, total: 2 })
    ));
    assert!(matches!(
        repo.merge_with_options("feature", "test", &resolve(&[(0, ChooseTheirs), (1, ChooseOurs)])),
        Err(VcsError::InconsistentInstanceResolution { key }) if key == "g"
    ));
    let result = repo
        .merge_with_options(
            "feature",
            "test",
            &resolve(&[(0, ChooseTheirs), (1, ChooseTheirs)]),
        )
        .unwrap();
    assert_eq!(result.instance_conflicts.len(), 2);
    assert_eq!(head_commit(&repo).parents.len(), 2);
    assert_eq!(
        instance_to_reference(&repo.committed_instance("HEAD", "g").unwrap().instance),
        instance_to_reference(&graph_of(&[(0, "a"), (1, "b"), (2, "b")]))
    );
}

#[test]
fn a_merged_instance_failing_a_key_check_or_validation_is_a_conflict_carrying_its_report() {
    use panproto_schema::{Key, KeyEquality, KeyPart, KeyRole, KeyScope, Vertex};
    let mut schema = panproto_schema::Schema {
        protocol: "test".into(),
        ..Default::default()
    };
    schema.vertices.insert(
        "item".into(),
        Vertex {
            id: "item".into(),
            kind: "number".into(),
            nsid: None,
        },
    );
    schema.declarations.keys.insert(
        "item".into(),
        vec![Key {
            id: "value".into(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
            equality: vec![KeyEquality::NumericValue],
        }],
    );
    let protocol = panproto_schema::Protocol::default();
    let item = |id: u32, anchor: &str, value: i64| {
        Node::new(id, anchor).with_value(FieldPresence::Present(Value::Int(value)))
    };
    let mut store = MemStore::new();
    let mut tracked_as = |graph: GInstance| {
        let dataset =
            DataSetObject::from_instance(ObjectId::ZERO, &Instance::Graph(graph), Some("k".into()))
                .unwrap();
        let id = store.put(&Object::DataSet(dataset.clone())).unwrap();
        BTreeMap::from([("k".to_owned(), (id, dataset))])
    };
    let base = tracked_as(GInstance::new().with_node(item(0, "item", 1)));
    // Each side adds an item of value 7: each alone is valid, the merge
    // holds the value twice.
    let ours = tracked_as(
        GInstance::new()
            .with_node(item(0, "item", 1))
            .with_node(item(1, "item", 7)),
    );
    let theirs = tracked_as(
        GInstance::new()
            .with_node(item(0, "item", 1))
            .with_node(item(2, "item", 7)),
    );
    // A third side adds a node anchored outside the schema.
    let stray = tracked_as(
        GInstance::new()
            .with_node(item(0, "item", 1))
            .with_node(item(3, "nowhere", 9)),
    );
    let mut load = |dataset: &DataSetObject| Ok((schema.clone(), dataset.decode()?));
    let mut validate = |schema: &panproto_schema::Schema, instance: &Instance| {
        let Instance::Graph(graph) = instance else {
            panic!("a graph");
        };
        let report = panproto_inst::validate_graph(graph, schema, &protocol);
        Ok(if report.status == panproto_gat::CheckStatus::Passed {
            Vec::new()
        } else {
            vec![(0, report)]
        })
    };
    let merge =
        instance_merge::merge_tracked(&base, &ours, &theirs, &mut load, &mut validate).unwrap();
    assert_eq!(merge.merged.len(), 0);
    match merge.conflicts.as_slice() {
        [
            InstanceConflict::KeyConstraint {
                key,
                record: None,
                diagnostics,
            },
        ] => {
            assert_eq!(key, "k");
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(
                diagnostics[0].check.kind,
                panproto_inst::validation::CheckKind::KeyUnique
            );
        }
        other => panic!("expected one key conflict, got {other:?}"),
    }
    let merge =
        instance_merge::merge_tracked(&base, &ours, &stray, &mut load, &mut validate).unwrap();
    match merge.conflicts.as_slice() {
        [
            InstanceConflict::Validation {
                key,
                record: None,
                report,
            },
        ] => {
            assert_eq!(key, "k");
            assert_eq!(report.status, panproto_gat::CheckStatus::Failed);
        }
        other => panic!("expected one validation conflict, got {other:?}"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_graph_merges_agree_with_the_reference(
        base in arbitrary_graph(),
        ours in arbitrary_graph(),
        theirs in arbitrary_graph(),
    ) {
        let schema = panproto_schema::Schema { protocol: "test".into(), ..Default::default() };
        let models = [base, ours, theirs].map(ModelInstance::Graph);
        let built = [&models[0], &models[1], &models[2]].map(instance_to_production);
        prop_assert_eq!(
            production_merge(&schema, [&built[0], &built[1], &built[2]]),
            reference_merge([&models[0], &models[1], &models[2]])
        );
    }
}

// ---------------------------------------------------------------------------
// Rebase and cherry-pick of tracked instances.
// ---------------------------------------------------------------------------

use panproto_reference::pages::instance_version_control::rebase_instances as model_rebase_instances;

fn production_rebase(
    schema: &panproto_schema::Schema,
    [parent, tip, replayed]: [&Instance; 3],
) -> MergeObservation {
    match instance_merge::rebase_instances("k", schema, parent, tip, replayed) {
        Ok(Ok(rebased)) => Ok(Ok(instance_to_reference(&rebased))),
        Ok(Err(conflicts)) => Ok(Err(conflicts.iter().map(conflict_to_reference).collect())),
        Err(VcsError::DeltaDoesNotApply { .. }) => Err(ModelRefusal::DoesNotApply),
        Err(VcsError::ShapeMismatch { .. }) => Err(ModelRefusal::ShapeMismatch),
        Err(other) => panic!("an untyped refusal: {other}"),
    }
}

fn reference_rebase([parent, tip, replayed]: [&ModelInstance; 3]) -> MergeObservation {
    model_rebase_instances("k", parent, tip, replayed)
        .map(|rebased| rebased.map(|instance| normalized_instance(&instance)))
}

/// The record a conflict is about, or `None`.
const fn conflict_record(conflict: &ModelConflict) -> Option<usize> {
    match conflict {
        ModelConflict::GraphEdit(_, record, _) => *record,
        ModelConflict::Record(_, record, _) => Some(*record),
        ModelConflict::Tracking(..)
        | ModelConflict::Migration(..)
        | ModelConflict::TableEdit(..) => None,
    }
}

#[test]
fn every_triple_of_the_merge_scope_rebases_as_the_reference_and_as_the_merge_merges() {
    let mut rebased = [0usize; 3];
    let mut conflicts = [0usize; 3];
    for schema in carrier::coverage_schemas() {
        let production = production_schema(&schema);
        for (shape, states) in merge_states(&schema).iter().enumerate() {
            let built: Vec<Instance> = states.iter().map(instance_to_production).collect();
            for p in 0..states.len() {
                for t in 0..states.len() {
                    for r in 0..states.len() {
                        let models = [&states[p], &states[t], &states[r]];
                        let actual =
                            production_rebase(&production, [&built[p], &built[t], &built[r]]);
                        assert_eq!(actual, reference_rebase(models), "{models:?}");
                        // Rebasing independent changes gives their merge; a
                        // dependent record conflicts once, with the first
                        // conflict the merge reports for it.
                        match (&actual, reference_merge(models)) {
                            (Ok(Ok(instance)), Ok(Ok(merged))) => {
                                assert_eq!(instance, &merged);
                                rebased[shape] += 1;
                            }
                            (Ok(Err(found)), Ok(Err(merged))) => {
                                let mut firsts: Vec<ModelConflict> = Vec::new();
                                for conflict in merged {
                                    let fresh = firsts.last().is_none_or(|last| {
                                        conflict_record(last) != conflict_record(&conflict)
                                            || matches!(conflict, ModelConflict::Record(..))
                                    });
                                    if fresh {
                                        firsts.push(conflict);
                                    }
                                }
                                assert_eq!(found, &firsts, "{models:?}");
                                conflicts[shape] += found.len();
                            }
                            (other, merged) => panic!("rebase {other:?} and merge {merged:?}"),
                        }
                    }
                }
            }
        }
    }
    // A rebase is clean exactly when the merge is: the merge scope's counts.
    assert_eq!(rebased, [5_218, 2_788, 118_450]);
    // A graph or table rebase conflicts once, so its conflicts are the
    // merge scope's conflicted triples; a tree rebase once per conflicting
    // record.
    assert_eq!(conflicts, [18_324, 7_596, 1_107_276]);
}

/// The graph a committed instance holds, as the reference states it.
fn committed_graph(repo: &Repository, reference: &str, key: &str) -> ModelInstance {
    instance_to_reference(&repo.committed_instance(reference, key).unwrap().instance)
}

#[test]
fn a_rebased_chain_tracks_its_rebased_deltas_applied_in_order_and_a_cherry_pick_replays_one_commit()
{
    // Base {0}; main adds node 9; feature adds node 1, then node 2.
    let (_dir, mut repo, _) = instance_repository();
    stage(&mut repo, "g", &graph_of(&[(0, "a")]));
    let base = repo.commit("base", "test").unwrap();
    panproto_vcs::refs::create_branch(repo.store_mut(), "feature", base).unwrap();
    stage(&mut repo, "g", &graph_of(&[(0, "a"), (9, "a")]));
    let main = repo.commit("main", "test").unwrap();
    panproto_vcs::refs::checkout_branch(repo.store_mut(), "feature").unwrap();
    stage(&mut repo, "g", &graph_of(&[(0, "a"), (1, "a")]));
    let first = repo.commit("first", "test").unwrap();
    stage(&mut repo, "g", &graph_of(&[(0, "a"), (1, "a"), (2, "a")]));
    repo.commit("second", "test").unwrap();

    let tip = repo.rebase(main, "test").unwrap();
    let tip = tip.to_string();
    // Applying the two rebased deltas in order to main's graph, and merging
    // the independent chain with main, both give {0, 1, 2, 9}.
    let expected = instance_to_reference(&graph_of(&[(0, "a"), (1, "a"), (2, "a"), (9, "a")]));
    assert_eq!(committed_graph(&repo, &tip, "g"), expected);
    let middle = head_commit(&repo).parents[0].to_string();
    assert_eq!(
        committed_graph(&repo, &middle, "g"),
        instance_to_reference(&graph_of(&[(0, "a"), (1, "a"), (9, "a")]))
    );

    // Cherry-picking `first` onto a branch at main replays its one delta.
    panproto_vcs::refs::create_branch(repo.store_mut(), "pick", main).unwrap();
    panproto_vcs::refs::checkout_branch(repo.store_mut(), "pick").unwrap();
    repo.cherry_pick(first, "test").unwrap();
    assert_eq!(
        committed_graph(&repo, "HEAD", "g"),
        instance_to_reference(&graph_of(&[(0, "a"), (1, "a"), (9, "a")]))
    );

    // A replayed change that depends on one committed since conflicts.
    stage(&mut repo, "g", &graph_of(&[(0, "a"), (1, "b"), (9, "a")]));
    repo.commit("node 1 at b", "test").unwrap();
    match repo.cherry_pick(first, "test") {
        Err(VcsError::InstanceConflicts { conflicts }) => {
            let found: Vec<ModelConflict> = conflicts.iter().map(conflict_to_reference).collect();
            assert_eq!(
                found,
                vec![ModelConflict::GraphEdit(
                    "g".to_owned(),
                    None,
                    reference_edit::EditConflict::AddAdd {
                        elem: reference_edit::GElem::Node(1)
                    }
                )]
            );
        }
        other => panic!("expected instance conflicts, got {other:?}"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_graph_rebases_agree_with_the_reference_and_with_the_merge(
        parent in arbitrary_graph(),
        tip in arbitrary_graph(),
        replayed in arbitrary_graph(),
    ) {
        let schema = panproto_schema::Schema { protocol: "test".into(), ..Default::default() };
        let models = [parent, tip, replayed].map(ModelInstance::Graph);
        let built = [&models[0], &models[1], &models[2]].map(instance_to_production);
        let actual = production_rebase(&schema, [&built[0], &built[1], &built[2]]);
        prop_assert_eq!(&actual, &reference_rebase([&models[0], &models[1], &models[2]]));
        if let Ok(Ok(rebased)) = &actual {
            prop_assert_eq!(
                Ok(Ok(rebased.clone())),
                reference_merge([&models[0], &models[1], &models[2]])
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Data migration along a stored migration.
// ---------------------------------------------------------------------------

mod data_migration {
    use super::*;
    use panproto_reference::enumerate::instance_version_control::migration_facts;
    use panproto_reference::enumerate::{
        GraphScope, MigrationScope, MigrationSizes, graphs_count, migrations_count, schemas_count,
    };
    use panproto_reference::lens::GraphLens as RefGraphLens;
    use panproto_reference::migration::Migration as RefMigration;
    use panproto_reference::pages::instance_version_control::{
        BackwardFacts, MigrationFacts, MigrationOutcome, MigrationRead, Stored, forward_objects,
        migration_outcome,
    };
    use panproto_reference::restrict::NoValueLayer;
    use panproto_reference::schema::Schema as RefSchema;
    use panproto_vcs::data_mig::{migrate_backward, migrate_forward, store_migration};
    use panproto_vcs::tree::store_schema_as_tree;
    use panproto_vcs::{MigrationConflict, schema_identity};

    /// The production schema of a reference schema of vertices and edges.
    pub(super) fn production_schema(schema: &RefSchema) -> panproto_schema::Schema {
        let protocol = panproto_schema::Protocol::default();
        let mut builder = panproto_schema::SchemaBuilder::new(&protocol);
        for (vertex, kind) in &schema.vertices {
            builder = builder.vertex(vertex, kind, None).unwrap();
        }
        for edge in &schema.edges {
            builder = builder
                .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
                .unwrap();
        }
        builder.build().unwrap()
    }

    /// The stored mapping of a vertex migration: each surviving source
    /// vertex to its image.
    pub(super) fn production_mapping(
        src: &RefSchema,
        migration: &RefMigration,
    ) -> panproto_mig::Migration {
        let mut mapping = panproto_mig::Migration::empty();
        for vertex in src.vertices.keys() {
            if migration.survives(vertex) {
                mapping.vertex_map.insert(
                    vertex.as_str().into(),
                    migration.vertex_image(vertex).into(),
                );
            }
        }
        mapping
    }

    /// The graphs over `schema` with at most `nodes` nodes and `edges`
    /// edges, every node plain and every edge labelled by a schema edge.
    pub(super) fn graphs_over(
        schema: &RefSchema,
        nodes: usize,
        edges: usize,
    ) -> Vec<reference_model::Graph> {
        let anchors: Vec<&str> = schema.vertices.keys().map(String::as_str).collect();
        let labels: Vec<reference_model::SEdge> = schema.edges.iter().cloned().collect();
        panproto_reference::enumerate::graphs(&GraphScope {
            anchors: &anchors,
            values: &[],
            shapes: &[reference_model::NodeShape::Plain],
            labels: &labels,
            positions: &[None],
            max_nodes: nodes,
            max_edges: edges,
        })
    }

    /// What migrating a graph forward and putting views back gives: a
    /// refusal of get, or the view and, for the view and each edited view,
    /// the restored source with whether get of it gives the view back, or a
    /// refusal of put.
    #[derive(Debug, PartialEq, Eq)]
    enum GraphMigration {
        Refused,
        Migrated {
            view: reference_model::Graph,
            puts: Vec<Option<(reference_model::Graph, bool)>>,
        },
    }

    fn reference_graph_migration(
        (src, tgt, migration): (&RefSchema, &RefSchema, &RefMigration),
        source: &reference_model::Graph,
        views: &[reference_model::Graph],
    ) -> GraphMigration {
        let Ok(plan) = panproto_reference::restrict::compile(src, tgt, migration) else {
            return GraphMigration::Refused;
        };
        let lens =
            RefGraphLens::new(src, tgt, migration, &plan, &BTreeMap::new(), &NoValueLayer).unwrap();
        let Ok((view, complement)) = lens.get(source) else {
            return GraphMigration::Refused;
        };
        let puts = std::iter::once(&view)
            .chain(views)
            .map(|edited| {
                lens.put(edited, &complement).ok().map(|restored| {
                    let round = lens.get(&restored).is_ok_and(|(again, _)| again == *edited);
                    (restored, round)
                })
            })
            .collect();
        GraphMigration::Migrated { view, puts }
    }

    /// The decoded graph of a stored data set.
    pub(super) fn stored_graph(store: &MemStore, id: ObjectId) -> reference_model::Graph {
        let Object::DataSet(dataset) = store.get(&id).unwrap() else {
            panic!("a migration stores data sets");
        };
        let Instance::Graph(graph) = dataset.decode().unwrap() else {
            panic!("a graph migrates to a graph");
        };
        graph_to_reference(&graph)
    }

    /// Production's run of the same case through a store, with the
    /// identifiers of the objects forward migration stored.
    fn production_graph_migration(
        (src, tgt, mapping): (
            &panproto_schema::Schema,
            &panproto_schema::Schema,
            &panproto_mig::Migration,
        ),
        source: &reference_model::Graph,
        views: &[reference_model::Graph],
    ) -> (GraphMigration, Vec<ObjectId>) {
        let protocol = panproto_schema::Protocol::default();
        let mut store = MemStore::new();
        let src_tree = store_schema_as_tree(&mut store, src.clone()).unwrap();
        let tgt_tree = store_schema_as_tree(&mut store, tgt.clone()).unwrap();
        let data = store
            .put(&Object::DataSet(
                DataSetObject::from_instance(
                    src_tree,
                    &Instance::Graph(graph_to_production(source, false)),
                    Some("k".to_owned()),
                )
                .unwrap(),
            ))
            .unwrap();
        let migration = store_migration(&mut store, src, tgt, mapping.clone()).unwrap();
        let (view_id, complement_id) = match migrate_forward(&mut store, data, migration, &protocol)
        {
            Ok(ids) => ids,
            Err(VcsError::Migration(conflict))
                if matches!(
                    *conflict,
                    MigrationConflict::MigrationDoesNotCompile { .. }
                        | MigrationConflict::OutsideGetDomain { .. }
                ) =>
            {
                return (GraphMigration::Refused, Vec::new());
            }
            Err(other) => panic!("forward migration failed otherwise: {other:?}"),
        };
        // The stored objects are those the repository model adds.
        let Object::Complement(complement) = store.get(&complement_id).unwrap() else {
            panic!("forward migration stores a complement");
        };
        let Object::DataSet(migrated) = store.get(&view_id).unwrap() else {
            panic!("forward migration stores a data set");
        };
        let model = |object: &Object| match object {
            Object::DataSet(dataset) => ModelObject::DataSet {
                shape: Shape::Graph,
                schema: dataset.schema_id.to_string(),
            },
            Object::Complement(complement) => ModelObject::Complement {
                shape: Shape::Graph,
                migration: complement.migration_id.to_string(),
                data: complement.data_id.to_string(),
            },
            other => panic!("unexpected {}", other.type_name()),
        };
        assert_eq!(
            (
                model(&Object::DataSet(migrated)),
                model(&Object::Complement(complement))
            ),
            forward_objects(
                Shape::Graph,
                &data.to_string(),
                &migration.to_string(),
                &tgt_tree.to_string()
            )
        );
        let view = stored_graph(&store, view_id);
        let puts = std::iter::once(&view)
            .chain(views)
            .map(|edited| {
                let edited_id = store
                    .put(&Object::DataSet(
                        DataSetObject::from_instance(
                            tgt_tree,
                            &Instance::Graph(graph_to_production(edited, false)),
                            Some("k".to_owned()),
                        )
                        .unwrap(),
                    ))
                    .unwrap();
                match migrate_backward(&mut store, edited_id, complement_id, &protocol) {
                    Ok(restored_id) => {
                        let restored = stored_graph(&store, restored_id);
                        let round = migrate_forward(&mut store, restored_id, migration, &protocol)
                            .is_ok_and(|(again, _)| stored_graph(&store, again) == *edited);
                        Some((restored, round))
                    }
                    Err(VcsError::Migration(conflict))
                        if matches!(
                            *conflict,
                            MigrationConflict::OutsidePutDomain { .. }
                                | MigrationConflict::ComplementIdentity { .. }
                        ) =>
                    {
                        None
                    }
                    Err(other) => panic!("backward migration failed otherwise: {other:?}"),
                }
            })
            .collect();
        (
            GraphMigration::Migrated { view, puts },
            vec![view_id, complement_id],
        )
    }

    /// The vertex migrations from `src` to `tgt` that send no two source
    /// vertices to one target vertex.
    pub(super) fn injective_migrations(src: &RefSchema, tgt: &RefSchema) -> Vec<RefMigration> {
        panproto_reference::enumerate::migrations(src, tgt, MigrationScope::VERTICES)
            .into_iter()
            .filter(|migration| {
                let images: BTreeSet<&str> = src
                    .vertices
                    .keys()
                    .filter(|vertex| migration.survives(vertex))
                    .map(|vertex| migration.vertex_image(vertex))
                    .collect();
                images.len()
                    == src
                        .vertices
                        .keys()
                        .filter(|vertex| migration.survives(vertex))
                        .count()
            })
            .collect()
    }

    /// The counts of the graph migration scope: cases, refused gets,
    /// puts, refused puts, puts restoring the source (`GetPut`) and puts
    /// whose get gives the view back (`PutGet`).
    #[derive(Debug, Default, PartialEq, Eq)]
    struct GraphMigrationCounts {
        cases: usize,
        refused: usize,
        puts: usize,
        refused_puts: usize,
        get_put: usize,
        put_get: usize,
    }

    /// Every case of the graph migration scope compared with the reference,
    /// its counts, and a rendering of the identifiers production stored.
    fn graph_migration_scope(keep: impl Fn(usize) -> bool) -> (GraphMigrationCounts, String) {
        let schemas = panproto_reference::enumerate::lenses::schemas();
        let mut counts = GraphMigrationCounts::default();
        let mut rendering = String::new();
        for src in &schemas {
            for tgt in &schemas {
                let views = graphs_over(tgt, 2, 1);
                let (src_p, tgt_p) = (production_schema(src), production_schema(tgt));
                for migration in injective_migrations(src, tgt) {
                    let mapping = production_mapping(src, &migration);
                    for source in graphs_over(src, 2, 1) {
                        let case = counts.cases;
                        counts.cases += 1;
                        if !keep(case) {
                            continue;
                        }
                        let expected =
                            reference_graph_migration((src, tgt, &migration), &source, &views);
                        let (found, ids) =
                            production_graph_migration((&src_p, &tgt_p, &mapping), &source, &views);
                        assert_eq!(
                            found, expected,
                            "case {case}: {src:?} -> {tgt:?} {migration:?} {source:?}"
                        );
                        for id in ids {
                            rendering.push_str(&id.to_string());
                        }
                        match found {
                            GraphMigration::Refused => counts.refused += 1,
                            GraphMigration::Migrated { puts, .. } => {
                                counts.get_put += usize::from(
                                    puts[0]
                                        .as_ref()
                                        .is_some_and(|(restored, _)| *restored == source),
                                );
                                counts.puts += puts.len();
                                for put in &puts {
                                    match put {
                                        None => counts.refused_puts += 1,
                                        Some((_, round)) => counts.put_get += usize::from(*round),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        (counts, rendering)
    }

    /// The number of cases of the graph migration scope, from the
    /// enumerators' counting formulas: every ordered pair of the five
    /// bounded schemas; the nine vertex migrations between two two-vertex
    /// schemas less the four that send both vertices to one (a vertex left
    /// out of the remap keeps its own name, so sending `v1` to `v0`, `v0` to
    /// `v1`, or both to one vertex contracts); and the graphs over the source
    /// with at most two nodes and one edge.
    fn graph_migration_cases() -> usize {
        let schemas = panproto_reference::enumerate::lenses::schemas();
        assert_eq!(schemas.len() as u64, schemas_count(2, 1, 1, 1));
        let migrations = migrations_count(
            &MigrationSizes {
                src_vertices: 2,
                tgt_vertices: 2,
                src_edges: 0,
                tgt_edges: 0,
                src_hyper_labels: Vec::new(),
                tgt_hyper_labels: Vec::new(),
            },
            MigrationScope::VERTICES,
        );
        let sources: u64 = schemas
            .iter()
            .map(|src| graphs_count(2, 0, 1, src.edges.len() as u64, 1, 2, 1))
            .sum();
        usize::try_from(schemas.len() as u64 * (migrations - 4) * sources).unwrap()
    }

    #[test]
    fn every_graph_of_the_migration_scope_migrates_forward_and_back_as_the_reference_lens_does() {
        let (counts, _) = graph_migration_scope(|_| true);
        // Each case puts back its own view and every graph over the target:
        // seven over a target without an edge and twenty-five over each of
        // the four with one, from the graph counting formula.
        let views: u64 = panproto_reference::enumerate::lenses::schemas()
            .iter()
            .map(|tgt| 1 + graphs_count(2, 0, 1, tgt.edges.len() as u64, 1, 2, 1))
            .sum();
        let sources: u64 = panproto_reference::enumerate::lenses::schemas()
            .iter()
            .map(|src| graphs_count(2, 0, 1, src.edges.len() as u64, 1, 2, 1))
            .sum();
        let puts = usize::try_from(5 * sources * views).unwrap();
        // Get is total on the scope and restores every source (GetPut), and
        // every put the domain admits gives its view back (PutGet).
        assert_eq!(
            counts,
            GraphMigrationCounts {
                cases: graph_migration_cases(),
                refused: 0,
                puts,
                refused_puts: 45_852,
                get_put: graph_migration_cases(),
                put_get: puts - 45_852,
            }
        );
    }

    /// Digest of the identifiers production stores over every 16th case of
    /// the graph migration scope.
    fn graph_migration_digest() -> u64 {
        panproto_reference::pages::migration_search::digest(
            graph_migration_scope(|case| case % 16 == 0).1.as_bytes(),
        )
    }

    const GRAPH_MIGRATION_DIGEST: u64 = 0xada8_d9c6_bada_fb07;

    #[test]
    fn one_process_graph_migration_digest() {
        if std::env::var_os("PP_GRAPH_MIGRATION_DIGEST").is_none() {
            return;
        }
        print!("<<<{:016x}>>>", graph_migration_digest());
    }

    #[test]
    fn migrated_objects_have_the_same_identifiers_in_every_process() {
        if std::env::var_os("PP_GRAPH_MIGRATION_DIGEST").is_some() {
            return;
        }
        let here = format!("{:016x}", graph_migration_digest());
        let Ok(exe) = std::env::current_exe() else {
            panic!("the test binary does not know its own path");
        };
        for _ in 0..3 {
            let output = std::process::Command::new(&exe)
                .args([
                    "data_migration::one_process_graph_migration_digest",
                    "--exact",
                    "--nocapture",
                ])
                .env("PP_GRAPH_MIGRATION_DIGEST", "1")
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&output.stdout).into_owned();
            let (Some(start), Some(end)) = (text.find("<<<"), text.find(">>>")) else {
                panic!("the child printed no digest: {text}");
            };
            assert_eq!(
                &text[start + 3..end],
                here,
                "a child process migrated differently"
            );
        }
        assert_eq!(here, format!("{GRAPH_MIGRATION_DIGEST:016x}"));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn randomized_graph_migrations_agree_with_the_reference_lens(
            src in 0..5_usize,
            tgt in 0..5_usize,
            migration in 0..9_usize,
            source in any::<prop::sample::Index>(),
        ) {
            let schemas = panproto_reference::enumerate::lenses::schemas();
            let (src, tgt) = (&schemas[src], &schemas[tgt]);
            let migrations = injective_migrations(src, tgt);
            let migration = &migrations[migration % migrations.len()];
            let sources = graphs_over(src, 3, 2);
            let source = &sources[source.index(sources.len())];
            let views = graphs_over(tgt, 1, 1);
            prop_assert_eq!(
                production_graph_migration(
                    (&production_schema(src), &production_schema(tgt), &production_mapping(src, migration)),
                    source,
                    &views,
                )
                .0,
                reference_graph_migration((src, tgt, migration), source, &views)
            );
        }
    }

    #[test]
    fn a_merge_migrates_graph_data_to_the_merged_schema() {
        let extended = |repo: &mut Repository| {
            let (_, _, schema) = instance_repository();
            let mut extended = super::production_schema(&schema);
            extended.vertices.insert(
                "c".into(),
                panproto_schema::Vertex {
                    id: "c".into(),
                    kind: "object".into(),
                    nsid: None,
                },
            );
            repo.add(&extended).unwrap();
        };
        let (_dir, mut repo) = diverged_repository(
            |repo| stage(repo, "g", &graph_of(&[(0, "a")])),
            |repo| stage(repo, "g", &graph_of(&[(0, "a"), (1, "a")])),
            extended,
        );
        let result = repo.merge("feature", "test").unwrap();
        assert_eq!(result.instance_conflicts, vec![]);
        let merge = head_commit(&repo);
        let committed = repo.committed_instance("HEAD", "g").unwrap();
        assert!(!committed.stale);
        assert_eq!(
            schema_identity(repo.store(), &committed.schema_id).unwrap(),
            schema_identity(repo.store(), &merge.schema_id).unwrap()
        );
        assert_eq!(
            instance_to_reference(&committed.instance),
            instance_to_reference(&graph_of(&[(0, "a"), (1, "a")]))
        );
        // The complement recorded names the merge commit's migration.
        let migrations: Vec<Option<ObjectId>> = merge
            .complement_ids
            .iter()
            .map(|id| match repo.store().get(id).unwrap() {
                Object::Complement(complement) => complement.migration(),
                other => panic!("a merge records complements, not {}", other.type_name()),
            })
            .collect();
        assert_eq!(migrations, vec![merge.migration_id]);
    }

    // -- The refusal order, realized in a store for every combination of
    // -- facts.

    fn tree_schema(vertices: &[(&str, &str)]) -> panproto_schema::Schema {
        let protocol = panproto_schema::Protocol::default();
        let mut builder = panproto_schema::SchemaBuilder::new(&protocol);
        for (vertex, kind) in vertices {
            builder = builder.vertex(vertex, kind, None).unwrap();
        }
        builder.build().unwrap()
    }

    fn identity_on(vertices: &[&str]) -> panproto_mig::Migration {
        let names: Vec<panproto_gat::Name> = vertices.iter().map(|v| (*v).into()).collect();
        panproto_mig::Migration::identity(&names, &[])
    }

    /// A store realizing `facts` for a tree data set, the identifiers to
    /// migrate, and the identifiers of the three schemas the migration and
    /// the data set name.
    struct Realized {
        store: MemStore,
        data: ObjectId,
        complement: ObjectId,
        migration: ObjectId,
        schemas: [ObjectId; 3],
    }

    /// `present`, or the identifier standing for a missing object or one of
    /// another kind.
    const fn stand_in(
        state: Stored,
        present: ObjectId,
        [missing, wrong]: [ObjectId; 2],
    ) -> ObjectId {
        match state {
            Stored::Missing => missing,
            Stored::WrongKind => wrong,
            Stored::Present => present,
        }
    }

    /// The migration's source and target schemas, and a schema of neither.
    fn realized_schemas() -> [panproto_schema::Schema; 3] {
        [
            tree_schema(&[("a", "object"), ("b", "string")]),
            tree_schema(&[("a", "object"), ("b", "string"), ("c", "string")]),
            tree_schema(&[("z", "object")]),
        ]
    }

    fn realize(facts: &MigrationFacts) -> Realized {
        let mut store = MemStore::new();
        let wrong = store.put(&Object::Protocol(Box::default())).unwrap();
        let missing = ObjectId::from_bytes([0xee; 32]);
        let stored = |state: Stored, present: ObjectId| stand_in(state, present, [missing, wrong]);
        let [src, tgt, other] = realized_schemas();
        let src_tree = store_schema_as_tree(&mut store, src.clone()).unwrap();
        let tgt_tree = store_schema_as_tree(&mut store, tgt.clone()).unwrap();
        let other_tree = store_schema_as_tree(&mut store, other).unwrap();
        let source_tree = WInstance::new(
            HashMap::from([(0, Node::new(0, "a"))]),
            vec![],
            vec![],
            0,
            "a".into(),
        );
        let mapping = if !facts.compiles {
            let mut bad = panproto_mig::Migration::empty();
            bad.vertex_map.insert("a".into(), "absent".into());
            bad
        } else if facts.backward.is_none() && !facts.lens_accepts {
            identity_on(&["b"])
        } else {
            identity_on(&["a", "b"])
        };
        let source_id = stored(
            facts.source,
            store
                .put(&Object::FlatSchema(Box::new(src.clone())))
                .unwrap(),
        );
        let target_id = stored(
            facts.target,
            store
                .put(&Object::FlatSchema(Box::new(tgt.clone())))
                .unwrap(),
        );
        let migration = stored(
            facts.migration,
            store
                .put(&Object::Migration {
                    src: source_id,
                    tgt: target_id,
                    mapping,
                })
                .unwrap(),
        );
        let source_data = store
            .put(&Object::DataSet(
                DataSetObject::from_instance(src_tree, &Instance::Tree(vec![source_tree]), None)
                    .unwrap(),
            ))
            .unwrap();
        let conforming = if facts.backward.is_some() {
            tgt_tree
        } else {
            src_tree
        };
        let data_schema = stored(
            facts.data_schema,
            if facts.conforms {
                conforming
            } else {
                other_tree
            },
        );
        let Some(side) = facts.backward else {
            let Object::DataSet(dataset) = store.get(&source_data).unwrap() else {
                unreachable!()
            };
            let data = stored(
                facts.data,
                store
                    .put(&Object::DataSet(DataSetObject {
                        schema_id: data_schema,
                        ..dataset
                    }))
                    .unwrap(),
            );
            return Realized {
                store,
                data,
                complement: missing,
                migration,
                schemas: [source_id, target_id, data_schema],
            };
        };
        realize_backward(
            Realizing {
                store,
                stored: [missing, wrong],
                src,
                tgt,
                source_data,
                migration,
                schemas: [source_id, target_id, data_schema],
            },
            side,
            facts,
        )
    }

    /// What `realize` has stored before the complement of a backward
    /// migration: the identifiers standing for a missing object and one of
    /// another kind, the two schemas, the source data set, the migration and
    /// the three schema identifiers it names.
    struct Realizing {
        store: MemStore,
        stored: [ObjectId; 2],
        src: panproto_schema::Schema,
        tgt: panproto_schema::Schema,
        source_data: ObjectId,
        migration: ObjectId,
        schemas: [ObjectId; 3],
    }

    fn realize_backward(at: Realizing, side: BackwardFacts, facts: &MigrationFacts) -> Realized {
        let Realizing {
            mut store,
            stored: [missing, wrong],
            src,
            tgt,
            source_data,
            migration,
            schemas,
        } = at;
        let protocol = panproto_schema::Protocol::default();
        let stored = |state: Stored, present: ObjectId| stand_in(state, present, [missing, wrong]);
        let data_schema = schemas[2];
        // A genuine forward migration, and one along another lens.
        let good = store_migration(&mut store, &src, &tgt, identity_on(&["a", "b"])).unwrap();
        let (view, complement) = migrate_forward(&mut store, source_data, good, &protocol).unwrap();
        let other_lens = store_migration(&mut store, &src, &tgt, identity_on(&["a"])).unwrap();
        let (_, foreign) = migrate_forward(&mut store, source_data, other_lens, &protocol).unwrap();
        let payload = |id: ObjectId| match store.get(&id).unwrap() {
            Object::Complement(complement) => complement.decode().unwrap(),
            _ => unreachable!(),
        };
        let payload = if !side.shape_agrees {
            ComplementPayload::Graph(Box::default())
        } else if !side.count_agrees {
            let ComplementPayload::Tree(one) = payload(complement) else {
                unreachable!()
            };
            ComplementPayload::Tree(vec![one[0].clone(), one[0].clone()])
        } else if facts.lens_accepts {
            payload(complement)
        } else {
            payload(foreign)
        };
        let named = if side.names_migration {
            migration
        } else {
            source_data
        };
        let complement = stored(
            side.complement,
            store
                .put(&Object::Complement(
                    ComplementObject::from_payload(named, source_data, &payload).unwrap(),
                ))
                .unwrap(),
        );
        let Object::DataSet(view) = store.get(&view).unwrap() else {
            unreachable!()
        };
        let data = stored(
            facts.data,
            store
                .put(&Object::DataSet(DataSetObject {
                    schema_id: data_schema,
                    ..view
                }))
                .unwrap(),
        );
        Realized {
            store,
            data,
            complement,
            migration,
            schemas,
        }
    }

    /// The object a refusal names, read off its identifier and kind.
    fn read_of(id: ObjectId, expected: &str, schemas: [ObjectId; 3]) -> MigrationRead {
        match expected {
            "data set" => MigrationRead::DataSet,
            "complement" => MigrationRead::Complement,
            "migration" => MigrationRead::Migration,
            "schema" if id == schemas[0] => MigrationRead::Source,
            "schema" if id == schemas[1] => MigrationRead::Target,
            "schema" => MigrationRead::DataSchema,
            other => panic!("a refusal named {other}"),
        }
    }

    fn observed(result: Result<(), VcsError>, schemas: [ObjectId; 3]) -> MigrationOutcome {
        let conflict = match result {
            Ok(()) => return MigrationOutcome::Migrates,
            Err(VcsError::Migration(conflict)) => *conflict,
            Err(other) => panic!("a migration failed otherwise: {other:?}"),
        };
        match conflict {
            MigrationConflict::Missing { id, expected } => {
                MigrationOutcome::Missing(read_of(id, expected, schemas))
            }
            MigrationConflict::WrongKind { id, expected, .. } => {
                MigrationOutcome::WrongKind(read_of(id, expected, schemas))
            }
            MigrationConflict::UnknownMigration { .. } => MigrationOutcome::UnknownMigration,
            MigrationConflict::SchemaMismatch { .. } => MigrationOutcome::SchemaMismatch,
            MigrationConflict::MigrationDoesNotCompile { .. } => MigrationOutcome::DoesNotCompile,
            MigrationConflict::ShapeMismatch { .. } => MigrationOutcome::ShapeMismatch,
            MigrationConflict::RecordCountMismatch { .. } => MigrationOutcome::RecordCountMismatch,
            MigrationConflict::OutsideGetDomain { .. }
            | MigrationConflict::ComplementIdentity { .. }
            | MigrationConflict::OutsidePutDomain { .. } => MigrationOutcome::LensRefuses,
            other => panic!("an unclassified conflict: {other:?}"),
        }
    }

    fn check_refusal_order(backward: bool) -> BTreeMap<MigrationOutcome, usize> {
        let protocol = panproto_schema::Protocol::default();
        let mut outcomes = BTreeMap::new();
        for facts in migration_facts(backward) {
            let mut realized = realize(&facts);
            let result = if backward {
                migrate_backward(
                    &mut realized.store,
                    realized.data,
                    realized.complement,
                    &protocol,
                )
                .map(drop)
            } else {
                migrate_forward(
                    &mut realized.store,
                    realized.data,
                    realized.migration,
                    &protocol,
                )
                .map(drop)
            };
            let found = observed(result, realized.schemas);
            assert_eq!(found, migration_outcome(&facts), "{facts:?}");
            *outcomes.entry(found).or_default() += 1;
        }
        outcomes
    }

    /// The outcome counts of one direction, derived from the order: each
    /// object read splits the combinations left into thirds (missing, of
    /// another kind, present) and each truth value in halves.
    fn derived_counts(backward: bool) -> BTreeMap<MigrationOutcome, usize> {
        let mut left = if backward {
            3_usize.pow(6) * 2_usize.pow(6)
        } else {
            3_usize.pow(5) * 2_usize.pow(3)
        };
        let mut out = BTreeMap::new();
        let mut reads = vec![MigrationRead::DataSet];
        if backward {
            reads.push(MigrationRead::Complement);
        }
        let mut split = |out: &mut BTreeMap<_, _>, outcomes: &[MigrationOutcome]| {
            left /= outcomes.len() + 1;
            for outcome in outcomes {
                out.insert(*outcome, left);
            }
        };
        for read in reads {
            split(
                &mut out,
                &[
                    MigrationOutcome::Missing(read),
                    MigrationOutcome::WrongKind(read),
                ],
            );
        }
        if backward {
            split(&mut out, &[MigrationOutcome::UnknownMigration]);
        }
        for read in [
            MigrationRead::Migration,
            MigrationRead::Source,
            MigrationRead::Target,
            MigrationRead::DataSchema,
        ] {
            split(
                &mut out,
                &[
                    MigrationOutcome::Missing(read),
                    MigrationOutcome::WrongKind(read),
                ],
            );
        }
        split(&mut out, &[MigrationOutcome::SchemaMismatch]);
        split(&mut out, &[MigrationOutcome::DoesNotCompile]);
        if backward {
            split(&mut out, &[MigrationOutcome::ShapeMismatch]);
            split(&mut out, &[MigrationOutcome::RecordCountMismatch]);
        }
        split(&mut out, &[MigrationOutcome::LensRefuses]);
        out.insert(MigrationOutcome::Migrates, left);
        out
    }

    #[test]
    fn every_combination_of_migration_facts_is_refused_in_the_stated_order() {
        let forward = check_refusal_order(false);
        assert_eq!(forward.values().sum::<usize>(), 1944);
        assert_eq!(forward, derived_counts(false));
        let backward = check_refusal_order(true);
        assert_eq!(backward.values().sum::<usize>(), 46_656);
        assert_eq!(backward, derived_counts(true));
    }

    #[test]
    fn a_migrated_data_set_names_a_schema_whose_identity_is_the_target_s() {
        let protocol = panproto_schema::Protocol::default();
        let mut store = MemStore::new();
        let src = tree_schema(&[("a", "object"), ("b", "string")]);
        let tgt = tree_schema(&[("a", "object")]);
        let src_tree = store_schema_as_tree(&mut store, src.clone()).unwrap();
        let tree = WInstance::new(
            HashMap::from([(0, Node::new(0, "a"))]),
            vec![],
            vec![],
            0,
            "a".into(),
        );
        let data = store
            .put(&Object::DataSet(
                DataSetObject::from_instance(src_tree, &Instance::Tree(vec![tree]), None).unwrap(),
            ))
            .unwrap();
        let migration = store_migration(&mut store, &src, &tgt, identity_on(&["a"])).unwrap();
        let (view, complement) = migrate_forward(&mut store, data, migration, &protocol).unwrap();
        let Object::DataSet(migrated) = store.get(&view).unwrap() else {
            panic!("a data set");
        };
        let Object::Migration { tgt: flat, .. } = store.get(&migration).unwrap() else {
            panic!("a migration");
        };
        assert_eq!(schema_identity(&store, &migrated.schema_id).unwrap(), flat);
        assert_ne!(migrated.schema_id, flat);
        // Forward then backward stores the source data set again.
        assert_eq!(
            migrate_backward(&mut store, view, complement, &protocol).unwrap(),
            data
        );
    }

    // -- Tree and table data migrated forward and back through a store,
    // -- against the reference tree and table lenses.

    /// What migrating an instance forward and putting views back gives: a
    /// refusal of get, or the view and, for the view and each edited view,
    /// the restored source with whether get of it gives the view back, or a
    /// refusal of put.
    #[derive(Debug, PartialEq, Eq)]
    enum RoundTrip<T> {
        Refused,
        Migrated {
            view: T,
            puts: Vec<Option<(T, bool)>>,
        },
    }

    /// The counts of a round-trip scope.
    #[derive(Debug, Default, PartialEq, Eq)]
    struct RoundTripCounts {
        cases: usize,
        outside: usize,
        refused: usize,
        puts: usize,
        refused_puts: usize,
        get_put: usize,
        put_get: usize,
    }

    impl RoundTripCounts {
        fn count<T: PartialEq>(&mut self, source: &T, found: &RoundTrip<T>) {
            match found {
                RoundTrip::Refused => self.refused += 1,
                RoundTrip::Migrated { puts, .. } => {
                    self.get_put += usize::from(
                        puts[0]
                            .as_ref()
                            .is_some_and(|(restored, _)| restored == source),
                    );
                    self.puts += puts.len();
                    for put in puts {
                        match put {
                            None => self.refused_puts += 1,
                            Some((_, round)) => self.put_get += usize::from(*round),
                        }
                    }
                }
            }
        }
    }

    /// Production's forward and backward migration of `source` through a
    /// store, putting back the view and each of `views`.
    fn production_round_trip(
        (src, tgt, mapping): (
            &panproto_schema::Schema,
            &panproto_schema::Schema,
            &panproto_mig::Migration,
        ),
        protocol: &panproto_schema::Protocol,
        source: &Instance,
        views: &[Instance],
    ) -> RoundTrip<Instance> {
        let mut store = MemStore::new();
        let src_tree = store_schema_as_tree(&mut store, src.clone()).unwrap();
        let tgt_tree = store_schema_as_tree(&mut store, tgt.clone()).unwrap();
        let put = |store: &mut MemStore, schema: ObjectId, instance: &Instance| {
            store
                .put(&Object::DataSet(
                    DataSetObject::from_instance(schema, instance, Some("k".to_owned())).unwrap(),
                ))
                .unwrap()
        };
        let read = |store: &MemStore, id: ObjectId| {
            let Object::DataSet(dataset) = store.get(&id).unwrap() else {
                panic!("a migration stores data sets");
            };
            dataset.decode().unwrap()
        };
        let data = put(&mut store, src_tree, source);
        let migration = store_migration(&mut store, src, tgt, mapping.clone()).unwrap();
        let (view_id, complement) = match migrate_forward(&mut store, data, migration, protocol) {
            Ok(ids) => ids,
            Err(VcsError::Migration(conflict))
                if matches!(
                    *conflict,
                    MigrationConflict::MigrationDoesNotCompile { .. }
                        | MigrationConflict::OutsideGetDomain { .. }
                ) =>
            {
                return RoundTrip::Refused;
            }
            Err(other) => panic!("forward migration failed otherwise: {other:?}"),
        };
        let view = read(&store, view_id);
        let puts = std::iter::once(&view)
            .chain(views)
            .map(|edited| {
                let edited_id = put(&mut store, tgt_tree, edited);
                match migrate_backward(&mut store, edited_id, complement, protocol) {
                    Ok(restored_id) => {
                        let restored = read(&store, restored_id);
                        let round = migrate_forward(&mut store, restored_id, migration, protocol)
                            .is_ok_and(|(again, _)| {
                                instance_to_reference(&read(&store, again))
                                    == instance_to_reference(edited)
                            });
                        Some((restored, round))
                    }
                    Err(VcsError::Migration(conflict))
                        if matches!(
                            *conflict,
                            MigrationConflict::OutsidePutDomain { .. }
                                | MigrationConflict::ComplementIdentity { .. }
                        ) =>
                    {
                        None
                    }
                    Err(other) => panic!("backward migration failed otherwise: {other:?}"),
                }
            })
            .collect();
        RoundTrip::Migrated { view, puts }
    }

    /// A production round trip as the reference states it.
    fn round_trip_to_reference(found: RoundTrip<Instance>) -> RoundTrip<ModelInstance> {
        match found {
            RoundTrip::Refused => RoundTrip::Refused,
            RoundTrip::Migrated { view, puts } => RoundTrip::Migrated {
                view: instance_to_reference(&view),
                puts: puts
                    .into_iter()
                    .map(|put| {
                        put.map(|(restored, round)| (instance_to_reference(&restored), round))
                    })
                    .collect(),
            },
        }
    }

    /// The trees over `schema` with one or two nodes, each arc labelled by
    /// an edge name of the bounded schemas.
    fn trees_over(schema: &RefSchema) -> Vec<reference_model::Tree> {
        let anchors: Vec<&str> = schema.vertices.keys().map(String::as_str).collect();
        (1..=2)
            .flat_map(|nodes| {
                panproto_reference::enumerate::trees(&panproto_reference::enumerate::TreeScope {
                    nodes,
                    anchors: &anchors,
                    values: &[],
                    arc_names: &["e"],
                    hyper_edges: &[],
                    max_fans: 0,
                })
            })
            .collect()
    }

    /// The reference's run of a tree case, or `None` outside the tree lens's
    /// certified domain (a source edge between surviving vertices dropped).
    fn reference_tree_round_trip(
        (src, tgt, migration): (&RefSchema, &RefSchema, &RefMigration),
        source: &reference_model::Tree,
        views: &[reference_model::Tree],
    ) -> Option<RoundTrip<ModelInstance>> {
        let trees = |tree: &reference_model::Tree| ModelInstance::Trees(vec![normalized(tree)]);
        let Ok(plan) = panproto_reference::restrict::compile(src, tgt, migration) else {
            return Some(RoundTrip::Refused);
        };
        let lens =
            RefGraphLens::new(src, tgt, migration, &plan, &BTreeMap::new(), &NoValueLayer).unwrap();
        let Ok((view, complement, embedding)) = panproto_reference::lens::tree_get(&lens, source)?
        else {
            return Some(RoundTrip::Refused);
        };
        let puts = std::iter::once(&view)
            .chain(views)
            .map(|edited| {
                panproto_reference::lens::tree_put(&lens, edited, &embedding, &complement)
                    .ok()
                    .map(|restored| {
                        let round = panproto_reference::lens::tree_get(&lens, &restored)
                            .is_some_and(|again| {
                                again.is_ok_and(|(again, _, _)| {
                                    normalized(&again) == normalized(edited)
                                })
                            });
                        (trees(&restored), round)
                    })
            })
            .collect();
        Some(RoundTrip::Migrated {
            view: trees(&view),
            puts,
        })
    }

    fn tree_migration_scope() -> RoundTripCounts {
        let schemas = panproto_reference::enumerate::lenses::schemas();
        let protocol = panproto_schema::Protocol::default();
        let mut counts = RoundTripCounts::default();
        for src in &schemas {
            for tgt in &schemas {
                let (src_p, tgt_p) = (production_schema(src), production_schema(tgt));
                for migration in injective_migrations(src, tgt) {
                    // The views put back: every tree over the target whose
                    // nodes are anchored at images of surviving source
                    // vertices. A view node at a target vertex no source
                    // vertex reaches is a disagreement of the tree put
                    // domain, outside this scope.
                    let images: BTreeSet<&str> = src
                        .vertices
                        .keys()
                        .filter(|v| migration.survives(v))
                        .map(|v| migration.vertex_image(v))
                        .collect();
                    let views: Vec<reference_model::Tree> = trees_over(tgt)
                        .into_iter()
                        .filter(|tree| {
                            tree.nodes
                                .values()
                                .all(|node| images.contains(node.anchor.as_str()))
                        })
                        .collect();
                    let production_views: Vec<Instance> = views
                        .iter()
                        .map(|tree| Instance::Tree(vec![tree_to_production(tree)]))
                        .collect();
                    let mapping = production_mapping(src, &migration);
                    for source in trees_over(src) {
                        counts.cases += 1;
                        let Some(expected) =
                            reference_tree_round_trip((src, tgt, &migration), &source, &views)
                        else {
                            counts.outside += 1;
                            continue;
                        };
                        let found = round_trip_to_reference(production_round_trip(
                            (&src_p, &tgt_p, &mapping),
                            &protocol,
                            &Instance::Tree(vec![tree_to_production(&source)]),
                            &production_views,
                        ));
                        assert_eq!(
                            found, expected,
                            "{src:?} -> {tgt:?} {migration:?} {source:?}"
                        );
                        counts.count(&ModelInstance::Trees(vec![normalized(&source)]), &found);
                    }
                }
            }
        }
        counts
    }

    /// The pinned outcome counts of the tree scope: puts, cases outside the
    /// tree lens's domain, refused gets, refused puts, `GetPut` and `PutGet`.
    const TREE_ROUND_TRIPS: [usize; 6] = [996, 400, 678, 580, 172, 416];

    #[test]
    fn every_tree_of_the_migration_scope_migrates_forward_and_back_as_the_reference_tree_lens_does()
    {
        let counts = tree_migration_scope();
        println!("tree migration scope: {counts:?}");
        let schemas = panproto_reference::enumerate::lenses::schemas();
        // The trees over a schema with one or two nodes: two of one node and
        // eight of two (four anchorings, the arc named or not).
        let per_schema: u64 = (1..=2)
            .map(|nodes| panproto_reference::enumerate::trees_count(nodes, 2, 0, 1, &[], 0))
            .sum();
        assert_eq!(per_schema, 10);
        let n = schemas.len() as u64;
        let injective = 5;
        assert_eq!(counts.cases as u64, n * n * injective * per_schema);
        assert_eq!(
            [
                counts.puts,
                counts.outside,
                counts.refused,
                counts.refused_puts,
                counts.get_put,
                counts.put_get,
            ],
            TREE_ROUND_TRIPS
        );
    }

    /// The table protocol of the bounded schemas: every vertex kind a table.
    fn table_protocol() -> panproto_schema::Protocol {
        panproto_schema::Protocol {
            non_value_kinds: std::collections::BTreeSet::from(["k".to_owned()]),
            ..panproto_schema::Protocol::default()
        }
    }

    /// The tables over `schema` with at most two rows and no cells, with
    /// every set of pairs of its edges.
    fn tables_over(schema: &RefSchema) -> Vec<reference_model::Table> {
        let tables: Vec<&str> = schema.vertices.keys().map(String::as_str).collect();
        let relations: Vec<reference_model::SEdge> = schema.edges.iter().cloned().collect();
        panproto_reference::enumerate::tables(&tables, &BTreeMap::new(), &[], &relations, 2)
    }

    fn reference_table_round_trip(
        (src, tgt, migration): (&RefSchema, &RefSchema, &RefMigration),
        source: &reference_model::Table,
        views: &[reference_model::Table],
    ) -> RoundTrip<ModelInstance> {
        let kinds = std::collections::BTreeSet::from(["k".to_owned()]);
        let classify = |schema: &RefSchema| {
            panproto_reference::rel::classify(schema, &std::collections::BTreeSet::new(), &kinds)
                .unwrap()
        };
        let boundaries = (classify(src), classify(tgt));
        let Ok(plan) = panproto_reference::table::compile_for_tables(
            src,
            tgt,
            &boundaries.0,
            &boundaries.1,
            migration,
        ) else {
            return RoundTrip::Refused;
        };
        let lens = panproto_reference::table::TableLens::new(
            src,
            tgt,
            panproto_reference::table::TableBoundaries::new(&boundaries.0, &boundaries.1),
            migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .unwrap();
        let Ok((view, complement)) = lens.get(source) else {
            return RoundTrip::Refused;
        };
        let puts = std::iter::once(&view)
            .chain(views)
            .map(|edited| {
                lens.put(edited, &complement).ok().map(|restored| {
                    let round = lens.get(&restored).is_ok_and(|(again, _)| again == *edited);
                    (ModelInstance::Table(restored), round)
                })
            })
            .collect();
        RoundTrip::Migrated {
            view: ModelInstance::Table(view),
            puts,
        }
    }

    /// A vertex migration with its edges: each source edge whose endpoints
    /// survive and whose image (the edge of its kind and name between the
    /// endpoints' images) the target holds survives as that image, as a
    /// table plan requires the edges it keeps to be named.
    fn with_edges(src: &RefSchema, tgt: &RefSchema, migration: &RefMigration) -> RefMigration {
        let mut out = migration.clone();
        for edge in &src.edges {
            if !(migration.survives(&edge.src) && migration.survives(&edge.tgt)) {
                continue;
            }
            let image = reference_model::SEdge::new(
                migration.vertex_image(&edge.src),
                migration.vertex_image(&edge.tgt),
                &edge.kind,
                edge.name.as_deref(),
            );
            if tgt.edges.contains(&image) {
                out.surviving_edges.insert(image.clone());
                if image != *edge {
                    out.edge_remap.insert(edge.clone(), image);
                }
            }
        }
        out
    }

    /// The stored mapping of a migration with edges: its vertex map and
    /// each surviving source edge to its image.
    fn production_mapping_with_edges(
        src: &RefSchema,
        migration: &RefMigration,
    ) -> panproto_mig::Migration {
        let mut mapping = production_mapping(src, migration);
        for edge in &src.edges {
            let image = migration.edge_remap.get(edge).unwrap_or(edge);
            if migration.surviving_edges.contains(image) {
                mapping
                    .edge_map
                    .insert(edge_to_production(edge), edge_to_production(image));
            }
        }
        mapping
    }

    fn table_migration_scope() -> RoundTripCounts {
        let schemas = panproto_reference::enumerate::lenses::schemas();
        let protocol = table_protocol();
        let mut counts = RoundTripCounts::default();
        for src in &schemas {
            for tgt in &schemas {
                let views = tables_over(tgt);
                let production_views: Vec<Instance> = views
                    .iter()
                    .map(|table| Instance::Table(table_to_production(table, false)))
                    .collect();
                let (src_p, tgt_p) = (production_schema(src), production_schema(tgt));
                for migration in injective_migrations(src, tgt) {
                    let migration = with_edges(src, tgt, &migration);
                    // A source edge between surviving tables whose image the
                    // target lacks has no stored spelling as a drop: the
                    // stored mapping names each kept edge, and the table
                    // compiler carries an unnamed edge to its endpoints'
                    // images. Such migrations are outside this scope.
                    let drops_kept_edge = src.edges.iter().any(|edge| {
                        migration.survives(&edge.src)
                            && migration.survives(&edge.tgt)
                            && !migration
                                .surviving_edges
                                .contains(migration.edge_remap.get(edge).unwrap_or(edge))
                    });
                    let mapping = production_mapping_with_edges(src, &migration);
                    for source in tables_over(src) {
                        counts.cases += 1;
                        if drops_kept_edge {
                            counts.outside += 1;
                            continue;
                        }
                        let expected =
                            reference_table_round_trip((src, tgt, &migration), &source, &views);
                        let found = round_trip_to_reference(production_round_trip(
                            (&src_p, &tgt_p, &mapping),
                            &protocol,
                            &Instance::Table(table_to_production(&source, false)),
                            &production_views,
                        ));
                        assert_eq!(
                            found, expected,
                            "{src:?} -> {tgt:?} {migration:?} {source:?}"
                        );
                        counts.count(&ModelInstance::Table(source.clone()), &found);
                    }
                }
            }
        }
        counts
    }

    /// The pinned outcome counts of the table scope: puts, cases outside
    /// the scope, refused gets, refused puts, `GetPut` and `PutGet`.
    const TABLE_ROUND_TRIPS: [usize; 6] = [19_380, 744, 0, 13_008, 1_131, 6_372];

    #[test]
    fn every_table_of_the_migration_scope_migrates_forward_and_back_as_the_reference_table_lens_does()
     {
        let counts = table_migration_scope();
        println!("table migration scope: {counts:?}");
        let schemas = panproto_reference::enumerate::lenses::schemas();
        let tables = |schema: &RefSchema| {
            let names: Vec<&str> = schema.vertices.keys().map(String::as_str).collect();
            let relations: Vec<reference_model::SEdge> = schema.edges.iter().cloned().collect();
            panproto_reference::enumerate::tables_count(&names, &BTreeMap::new(), 0, &relations, 2)
        };
        let sources: u64 = schemas.iter().map(tables).sum();
        let n = schemas.len() as u64;
        assert_eq!(counts.cases as u64, n * 5 * sources);
        assert_eq!(
            [
                counts.puts,
                counts.outside,
                counts.refused,
                counts.refused_puts,
                counts.get_put,
                counts.put_get,
            ],
            TABLE_ROUND_TRIPS
        );
    }
}

// ---------------------------------------------------------------------------
// Merging and replaying versions recorded against different schemas, and
// migrating a stale instance along first-parent history.
// ---------------------------------------------------------------------------

mod merge_migration {
    use super::data_migration::{
        graphs_over, injective_migrations, production_mapping, production_schema,
    };
    use super::*;
    use panproto_reference::enumerate::instance_version_control::{
        Link, path_chains, path_chains_count,
    };
    use panproto_reference::enumerate::{
        MigrationScope, MigrationSizes, TreeScope, migrations_count, trees,
    };
    use panproto_reference::lens::GraphLens as RefGraphLens;
    use panproto_reference::migration::Migration as RefMigration;
    use panproto_reference::pages::instance_version_control::{
        ChainCommit, PathSide, merge_path, merge_tracked_to as model_merge_tracked_to,
        migration_path as model_migration_path,
    };
    use panproto_reference::restrict::NoValueLayer;
    use panproto_reference::schema::Schema as RefSchema;
    use panproto_vcs::data_mig::{
        MergeMigration, MergeSide, migrate_along, migrate_backward, migration_path, store_migration,
    };
    use panproto_vcs::hash::hash_schema;
    use panproto_vcs::instance_merge::ToMerged;
    use panproto_vcs::tree::store_schema_as_tree;
    use panproto_vcs::{MigrationConflict, Version};

    /// Get of `instance` along the reference lens of `migration` from `src`
    /// to `tgt`, named `name`: each tree of a sequence through the tree
    /// lens, a graph through the graph lens.
    fn reference_get(
        (src, tgt, migration): (&RefSchema, &RefSchema, &RefMigration),
        name: &str,
        instance: &ModelInstance,
    ) -> Result<ModelInstance, ModelMigrationRefusal> {
        let compiled = || ModelMigrationRefusal::DoesNotCompile(name.to_owned());
        let plan =
            panproto_reference::restrict::compile(src, tgt, migration).map_err(|_| compiled())?;
        let lens = RefGraphLens::new(src, tgt, migration, &plan, &BTreeMap::new(), &NoValueLayer)
            .map_err(|_| compiled())?;
        match instance {
            ModelInstance::Graph(graph) => lens
                .get(graph)
                .map(|(view, _)| ModelInstance::Graph(view))
                .map_err(|_| ModelMigrationRefusal::GetRefused(None)),
            ModelInstance::Trees(trees) => trees
                .iter()
                .enumerate()
                .map(
                    |(record, tree)| match panproto_reference::lens::tree_get(&lens, tree) {
                        None => panic!("a tree of the scope is outside the tree lens's domain"),
                        Some(Ok((view, _, _))) => Ok(normalized(&view)),
                        Some(Err(_)) => Err(ModelMigrationRefusal::GetRefused(Some(record))),
                    },
                )
                .collect::<Result<Vec<_>, _>>()
                .map(ModelInstance::Trees),
            ModelInstance::Table(_) => panic!("the merge migration scope holds no table"),
        }
    }

    fn commit(
        store: &mut MemStore,
        schema: ObjectId,
        parent: Option<ObjectId>,
        data: &[ObjectId],
        migration: Option<ObjectId>,
        message: &str,
    ) -> ObjectId {
        let mut builder = CommitObject::builder(schema, "test", "test", message)
            .timestamp(0)
            .data_ids(data.to_vec());
        if let Some(parent) = parent {
            builder = builder.parents(vec![parent]);
        }
        if let Some(migration) = migration {
            builder = builder.migration_id(migration);
        }
        store.put(&Object::Commit(builder.build())).unwrap()
    }

    fn commit_at(store: &MemStore, id: ObjectId) -> CommitObject {
        let Object::Commit(commit) = store.get(&id).unwrap() else {
            panic!("a commit");
        };
        commit
    }

    // -- The path scope.

    /// Three flat schemas of one vertex each, and the stored migration
    /// between each ordered pair of them (the identity for a pair of one).
    struct PathStore {
        store: MemStore,
        schemas: [ObjectId; 3],
        migrations: BTreeMap<(usize, usize), ObjectId>,
    }

    fn path_store() -> PathStore {
        let mut store = MemStore::new();
        let schema = |vertex: &str| {
            let protocol = panproto_schema::Protocol::default();
            panproto_schema::SchemaBuilder::new(&protocol)
                .vertex(vertex, "object", None)
                .unwrap()
                .build()
                .unwrap()
        };
        let built = [schema("x"), schema("y"), schema("z")];
        let schemas = [0, 1, 2].map(|i| {
            store
                .put(&Object::FlatSchema(Box::new(built[i].clone())))
                .unwrap()
        });
        let mut migrations = BTreeMap::new();
        for from in 0..3 {
            for to in 0..3 {
                let id = store_migration(
                    &mut store,
                    &built[from],
                    &built[to],
                    panproto_mig::Migration::empty(),
                )
                .unwrap();
                migrations.insert((from, to), id);
            }
        }
        PathStore {
            store,
            schemas,
            migrations,
        }
    }

    /// The reference names of the path store's migrations: each with its
    /// source and target schema names.
    fn model_migrations(at: &PathStore) -> BTreeMap<String, (String, String)> {
        at.migrations
            .iter()
            .map(|((from, to), id)| (id.to_string(), (from.to_string(), to.to_string())))
            .collect()
    }

    /// Store `chain` (oldest first) on `parent`, returning the commits and
    /// the reference chain, newest first.
    fn store_chain(
        at: &mut PathStore,
        parent: Option<(ObjectId, usize)>,
        chain: &[(usize, Link)],
    ) -> (Vec<ObjectId>, Vec<ChainCommit>) {
        let mut ids = Vec::new();
        let mut model = Vec::new();
        let mut previous = parent;
        for (schema, link) in chain {
            let migration = previous.and_then(|(_, from)| match link {
                Link::Missing => None,
                Link::Forward => Some(at.migrations[&(from, *schema)]),
                Link::Reverse => Some(at.migrations[&(*schema, from)]),
            });
            let id = commit(
                &mut at.store,
                at.schemas[*schema],
                previous.map(|(id, _)| id),
                &[],
                migration,
                &format!("{}", ids.len()),
            );
            ids.push(id);
            model.push(ChainCommit {
                name: id.to_string(),
                schema: schema.to_string(),
                migration: migration.map(|m| m.to_string()),
            });
            previous = Some((id, *schema));
        }
        model.reverse();
        (ids, model)
    }

    fn path_refusal(error: VcsError) -> PathRefusal {
        let VcsError::Migration(conflict) = error else {
            panic!("a path refused otherwise: {error:?}");
        };
        match migration_refusal_to_reference(&conflict) {
            ModelMigrationRefusal::Path(refusal) => refusal,
            other => panic!("a path refusal of another kind: {other:?}"),
        }
    }

    /// The outcomes of the path scope, derived by hand from the chains.
    /// Asking for the absent third schema gives no path (86 chains), as does
    /// asking for a schema no commit of a one-schema chain holds (2 + 6 + 18).
    /// Asking for the tip's schema gives the empty path (86). Otherwise the
    /// oldest link between two schemas that differ decides: one per two-commit
    /// chain of two schemas (6), and for three commits 36 chains whose middle
    /// commit has the schema asked for (the tip's link decides) and 18 whose
    /// root has it and middle the tip's (the middle's link decides), each
    /// third missing, forward or reversed: 20 of each.
    const PATH_OUTCOMES: [(&str, usize); 5] = [
        ("LinkMismatch", 20),
        ("MissingLink", 20),
        ("NoPath", 112),
        ("path of 0", 86),
        ("path of 1", 20),
    ];

    #[test]
    fn every_chain_of_the_path_scope_finds_the_reference_path() {
        let mut outcomes: BTreeMap<String, usize> = BTreeMap::new();
        let mut cases = 0usize;
        for commits in 1..=3 {
            for chain in path_chains(commits, 2) {
                let mut at = path_store();
                let (ids, model) = store_chain(&mut at, None, &chain);
                let tip = *ids.last().unwrap();
                let migrations = model_migrations(&at);
                for query in 0..3 {
                    let identity =
                        panproto_vcs::schema_identity(&at.store, &at.schemas[query]).unwrap();
                    let found = migration_path(&at.store, tip, identity)
                        .map(|path| path.iter().map(ToString::to_string).collect::<Vec<_>>())
                        .map_err(path_refusal);
                    let expected = model_migration_path(&model, &migrations, &query.to_string());
                    assert_eq!(found, expected, "{chain:?} from {query}");
                    let kind = match &found {
                        Ok(path) => format!("path of {}", path.len()),
                        Err(refusal) => {
                            format!("{refusal:?}").split('(').next().unwrap().to_owned()
                        }
                    };
                    *outcomes.entry(kind).or_default() += 1;
                    cases += 1;
                }
            }
        }
        let chains: u64 = (1..=3).map(|commits| path_chains_count(commits, 2)).sum();
        assert_eq!(cases as u64, 3 * chains);
        assert_eq!(cases, 258);
        assert_eq!(
            outcomes,
            PATH_OUTCOMES
                .iter()
                .map(|(k, n)| ((*k).to_owned(), *n))
                .collect()
        );
        println!("path scope outcomes: {outcomes:?}");
    }

    /// The outcomes of the merge path scope, pinned.
    const MERGE_PATH_OUTCOMES: [(&str, usize); 6] = [
        ("LinkMismatch", 108),
        ("MissingLink", 108),
        ("NoPath", 288),
        ("path of 0", 486),
        ("path of 1", 414),
        ("path of 2", 54),
    ];

    #[test]
    fn every_version_of_the_merge_path_scope_finds_the_reference_path() {
        let mut outcomes: BTreeMap<String, usize> = BTreeMap::new();
        let mut cases = 0usize;
        for root in 0..2 {
            for ours in 0..3 {
                for ours_link in Link::ALL {
                    for theirs in 0..3 {
                        for theirs_link in Link::ALL {
                            let mut at = path_store();
                            let (roots, root_model) =
                                store_chain(&mut at, None, &[(root, Link::Missing)]);
                            let parent = Some((roots[0], root));
                            let (ours_ids, mut ours_model) =
                                store_chain(&mut at, parent, &[(ours, ours_link)]);
                            let (theirs_ids, mut theirs_model) =
                                store_chain(&mut at, parent, &[(theirs, theirs_link)]);
                            ours_model.extend(root_model.iter().cloned());
                            theirs_model.extend(root_model.iter().cloned());
                            // The merged schema is the third; each side
                            // migrates to it unless it is already there.
                            let to_merged =
                                |side: usize| (side != 2).then(|| at.migrations[&(side, 2)]);
                            let registered: HashMap<String, panproto_schema::Protocol> =
                                HashMap::new();
                            let merge = MergeMigration {
                                sides: [
                                    MergeSide {
                                        commit: ours_ids[0],
                                        migration: to_merged(ours),
                                        protocol: None,
                                    },
                                    MergeSide {
                                        commit: theirs_ids[0],
                                        migration: to_merged(theirs),
                                        protocol: None,
                                    },
                                ],
                                merged: panproto_vcs::schema_identity(&at.store, &at.schemas[2])
                                    .unwrap(),
                                registered: &registered,
                            };
                            let migrations = model_migrations(&at);
                            let names =
                                [ours, theirs].map(|side| to_merged(side).map(|m| m.to_string()));
                            let sides = [
                                PathSide {
                                    chain: &ours_model,
                                    migration: names[0].as_deref(),
                                },
                                PathSide {
                                    chain: &theirs_model,
                                    migration: names[1].as_deref(),
                                },
                            ];
                            for (version, model_version) in [
                                (Version::Base, ModelVersion::Base),
                                (Version::Ours, ModelVersion::Ours),
                                (Version::Theirs, ModelVersion::Theirs),
                            ] {
                                for query in 0..3 {
                                    let identity = panproto_vcs::schema_identity(
                                        &at.store,
                                        &at.schemas[query],
                                    )
                                    .unwrap();
                                    let found = merge
                                        .path(&at.store, version, identity)
                                        .map(|(path, _)| {
                                            path.iter().map(ToString::to_string).collect::<Vec<_>>()
                                        })
                                        .map_err(path_refusal);
                                    let expected = merge_path(
                                        model_version,
                                        &query.to_string(),
                                        sides,
                                        &migrations,
                                        "2",
                                    );
                                    assert_eq!(
                                        found, expected,
                                        "root {root}, ours {ours} {ours_link:?}, theirs {theirs} {theirs_link:?}, {version:?} from {query}"
                                    );
                                    let kind = match &found {
                                        Ok(path) => format!("path of {}", path.len()),
                                        Err(refusal) => format!("{refusal:?}")
                                            .split('(')
                                            .next()
                                            .unwrap()
                                            .to_owned(),
                                    };
                                    *outcomes.entry(kind).or_default() += 1;
                                    cases += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        // Two root schemas, three schemas and three links on each side, three
        // versions and three schemas asked for.
        assert_eq!(cases, 2 * 9 * 9 * 3 * 3);
        println!("merge path scope outcomes: {outcomes:?}");
        assert_eq!(
            outcomes,
            MERGE_PATH_OUTCOMES
                .iter()
                .map(|(k, n)| ((*k).to_owned(), *n))
                .collect()
        );
    }

    // -- The merge migration scope.

    /// A history of the merge migration scope: from a base commit of the
    /// `src` schema, one side commits the `tgt` schema with migration
    /// `migration` (recorded when `link`), the other side keeps the schema;
    /// the merged schema is `tgt`.
    #[derive(Clone, Copy, Debug)]
    struct History {
        src: usize,
        tgt: usize,
        migration: usize,
        changed_is_ours: bool,
        link: bool,
    }

    /// Every history over the source schemas `sources`.
    fn histories(sources: &[usize]) -> Vec<History> {
        let schemas = panproto_reference::enumerate::lenses::schemas();
        let mut out = Vec::new();
        for &src in sources {
            for tgt in 0..schemas.len() {
                for migration in 0..injective_migrations(&schemas[src], &schemas[tgt]).len() {
                    for changed_is_ours in [true, false] {
                        for link in [true, false] {
                            out.push(History {
                                src,
                                tgt,
                                migration,
                                changed_is_ours,
                                link,
                            });
                        }
                    }
                }
            }
        }
        out
    }

    /// The shapes of the merge migration scope.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ScopeShape {
        /// Graphs with at most one node and no edge.
        Graph,
        /// Sequences of at most one one-node tree.
        Tree,
    }

    /// The instances of `shape` over `schema` a side records.
    fn side_instances(schema: &RefSchema, shape: ScopeShape) -> Vec<ModelInstance> {
        match shape {
            ScopeShape::Graph => graphs_over(schema, 1, 0)
                .into_iter()
                .map(ModelInstance::Graph)
                .collect(),
            ScopeShape::Tree => {
                let anchors: Vec<&str> = schema.vertices.keys().map(String::as_str).collect();
                let mut out = vec![ModelInstance::Trees(Vec::new())];
                out.extend(
                    trees(&TreeScope {
                        nodes: 1,
                        anchors: &anchors,
                        values: &[],
                        arc_names: &[],
                        hyper_edges: &[],
                        max_fans: 0,
                    })
                    .into_iter()
                    .map(|tree| ModelInstance::Trees(vec![tree])),
                );
                out
            }
        }
    }

    /// The counts of a run over the merge migration scope.
    #[derive(Debug, Default, PartialEq, Eq)]
    struct MergeMigrationCounts {
        cases: usize,
        /// Cases whose three versions are recorded against different
        /// schemas and none is kept whole.
        migrated: usize,
        /// Migration conflicts, and by refusal.
        migration_conflicts: usize,
        missing_links: usize,
        refused_gets: usize,
        /// Cases merging cleanly, and the conflicts of the others.
        clean: usize,
        conflicts: usize,
    }

    /// The name of a schema of the scope in the reference model.
    fn schema_name(index: usize) -> String {
        format!("schema {index}")
    }

    fn data_set(
        store: &mut MemStore,
        (tree, name): (ObjectId, &str),
        instance: &ModelInstance,
    ) -> (ObjectId, ModelEntry) {
        let dataset = DataSetObject::from_instance(
            tree,
            &instance_to_production(instance),
            Some("k".to_owned()),
        )
        .unwrap();
        let id = store.put(&Object::DataSet(dataset)).unwrap();
        (
            id,
            ModelEntry {
                data: id.to_string(),
                schema: name.to_owned(),
                instance: normalized_instance(instance),
            },
        )
    }

    /// Run every case of one history over `shape`, comparing production's
    /// migrating merge with the reference's.
    fn run_history(
        history: History,
        shape: ScopeShape,
        sampled: Option<[&ModelInstance; 4]>,
        counts: &mut MergeMigrationCounts,
    ) {
        let schemas = panproto_reference::enumerate::lenses::schemas();
        let (src, tgt) = (&schemas[history.src], &schemas[history.tgt]);
        let migration = injective_migrations(src, tgt)[history.migration].clone();
        let (src_p, tgt_p) = (production_schema(src), production_schema(tgt));
        let mut store = MemStore::new();
        let trees = [
            store_schema_as_tree(&mut store, src_p.clone()).unwrap(),
            store_schema_as_tree(&mut store, tgt_p.clone()).unwrap(),
        ];
        let names = [schema_name(history.src), schema_name(history.tgt)];
        let stored = store_migration(
            &mut store,
            &src_p,
            &tgt_p,
            production_mapping(src, &migration),
        )
        .unwrap();
        let distinct = hash_schema(&src_p).unwrap() != hash_schema(&tgt_p).unwrap();
        let merged_identity = hash_schema(&tgt_p).unwrap();
        let registered: HashMap<String, panproto_schema::Protocol> = HashMap::new();
        let (old, new) = sampled.map_or_else(
            || (side_instances(src, shape), side_instances(tgt, shape)),
            |[base, old_changed, new_changed, unchanged]| {
                (
                    vec![base.clone(), old_changed.clone(), unchanged.clone()],
                    vec![new_changed.clone()],
                )
            },
        );
        let changed_choices: Vec<(usize, &ModelInstance)> = new
            .iter()
            .map(|instance| (1, instance))
            .chain(old.iter().map(|instance| (0, instance)))
            .collect();
        for base in &old {
            for (changed_schema, changed) in &changed_choices {
                for unchanged in &old {
                    let (base_id, base_entry) = data_set(&mut store, (trees[0], &names[0]), base);
                    let (changed_id, changed_entry) = data_set(
                        &mut store,
                        (trees[*changed_schema], &names[*changed_schema]),
                        changed,
                    );
                    let (unchanged_id, unchanged_entry) =
                        data_set(&mut store, (trees[0], &names[0]), unchanged);
                    let base_commit = commit(&mut store, trees[0], None, &[base_id], None, "base");
                    let changed_commit = commit(
                        &mut store,
                        trees[1],
                        Some(base_commit),
                        &[changed_id],
                        history.link.then_some(stored),
                        "changed",
                    );
                    let unchanged_commit = commit(
                        &mut store,
                        trees[0],
                        Some(base_commit),
                        &[unchanged_id],
                        None,
                        "unchanged",
                    );
                    let side_migration = |changed: bool| (!changed && distinct).then_some(stored);
                    let ([ours, theirs], [ours_entry, theirs_entry], ours_changed) =
                        if history.changed_is_ours {
                            (
                                [changed_commit, unchanged_commit],
                                [&changed_entry, &unchanged_entry],
                                true,
                            )
                        } else {
                            (
                                [unchanged_commit, changed_commit],
                                [&unchanged_entry, &changed_entry],
                                false,
                            )
                        };
                    // Production.
                    let merge = MergeMigration {
                        sides: [
                            MergeSide {
                                commit: ours,
                                migration: side_migration(ours_changed),
                                protocol: None,
                            },
                            MergeSide {
                                commit: theirs,
                                migration: side_migration(!ours_changed),
                                protocol: None,
                            },
                        ],
                        merged: merged_identity,
                        registered: &registered,
                    };
                    let tracked = |id: ObjectId| {
                        instance_vc::tracked(&store, &commit_at(&store, id)).unwrap()
                    };
                    let (base_map, ours_map, theirs_map) =
                        (tracked(base_commit), tracked(ours), tracked(theirs));
                    let mut migrate = |version: Version, id: ObjectId, dataset: &DataSetObject| {
                        merge.migrate(&store, version, id, dataset)
                    };
                    let mut load = |dataset: &DataSetObject| {
                        let schema = if dataset.schema_id == trees[1] {
                            tgt_p.clone()
                        } else {
                            src_p.clone()
                        };
                        Ok((schema, dataset.decode()?))
                    };
                    let mut validate = |_: &panproto_schema::Schema, _: &Instance| Ok(Vec::new());
                    let mut to = ToMerged {
                        schema_id: trees[1],
                        schema: &tgt_p,
                        migrate: &mut migrate,
                    };
                    let produced = instance_merge::merge_tracked_to(
                        &base_map,
                        &ours_map,
                        &theirs_map,
                        &mut load,
                        &mut validate,
                        &mut to,
                    )
                    .unwrap();
                    let schema_of = |id: &ObjectId| {
                        if *id == trees[1] {
                            names[1].clone()
                        } else {
                            names[0].clone()
                        }
                    };
                    let found_merged: BTreeMap<String, ModelMerged> = produced
                        .merged
                        .iter()
                        .map(|(key, entry)| {
                            let entry = match entry {
                                MergedEntry::Kept(id) => ModelMerged::Kept(id.to_string()),
                                MergedEntry::Merged {
                                    schema_id,
                                    instance,
                                } => ModelMerged::New(
                                    schema_of(schema_id),
                                    instance_to_reference(instance),
                                ),
                            };
                            (key.clone(), entry)
                        })
                        .collect();
                    let found_conflicts: Vec<ModelConflict> = produced
                        .conflicts
                        .iter()
                        .map(conflict_to_reference)
                        .collect();
                    // The reference, over the history as the model states it.
                    let link = history.link.then(|| stored.to_string());
                    let changed_chain = vec![
                        ChainCommit {
                            name: changed_commit.to_string(),
                            schema: names[1].clone(),
                            migration: link,
                        },
                        ChainCommit {
                            name: base_commit.to_string(),
                            schema: names[0].clone(),
                            migration: None,
                        },
                    ];
                    let unchanged_chain = vec![
                        ChainCommit {
                            name: unchanged_commit.to_string(),
                            schema: names[0].clone(),
                            migration: None,
                        },
                        changed_chain[1].clone(),
                    ];
                    let migrations = BTreeMap::from([(
                        stored.to_string(),
                        (names[0].clone(), names[1].clone()),
                    )]);
                    let side_name = (names[0] != names[1]).then(|| stored.to_string());
                    let changed_side = PathSide {
                        chain: &changed_chain,
                        migration: None,
                    };
                    let unchanged_side = PathSide {
                        chain: &unchanged_chain,
                        migration: side_name.as_deref(),
                    };
                    let sides = if ours_changed {
                        [changed_side, unchanged_side]
                    } else {
                        [unchanged_side, changed_side]
                    };
                    let mut model_migrate =
                        |version: ModelVersion,
                         entry: &ModelEntry|
                         -> Result<ModelInstance, ModelMigrationRefusal> {
                            let path =
                                merge_path(version, &entry.schema, sides, &migrations, &names[1])
                                    .map_err(ModelMigrationRefusal::Path)?;
                            let mut instance = entry.instance.clone();
                            for step in path {
                                instance = reference_get((src, tgt, &migration), &step, &instance)?;
                            }
                            Ok(instance)
                        };
                    let one =
                        |entry: &ModelEntry| BTreeMap::from([("k".to_owned(), entry.clone())]);
                    let (expected_merged, expected_conflicts) = model_merge_tracked_to(
                        &one(&base_entry),
                        &one(ours_entry),
                        &one(theirs_entry),
                        (&names[1], &mut model_migrate),
                    )
                    .unwrap();
                    let expected_merged: BTreeMap<String, ModelMerged> = expected_merged
                        .into_iter()
                        .map(|(key, entry)| {
                            let entry = match entry {
                                ModelMerged::New(schema, instance) => {
                                    ModelMerged::New(schema, normalized_instance(&instance))
                                }
                                kept @ ModelMerged::Kept(_) => kept,
                            };
                            (key, entry)
                        })
                        .collect();
                    assert_eq!(
                        (&found_merged, &found_conflicts),
                        (&expected_merged, &expected_conflicts),
                        "{history:?} {shape:?} base {base:?} changed {changed:?} unchanged {unchanged:?}"
                    );
                    counts.cases += 1;
                    let kept = changed_id == unchanged_id
                        || unchanged_id == base_id
                        || changed_id == base_id;
                    let migrated = names[*changed_schema] != names[0] && !kept;
                    counts.migrated += usize::from(migrated);
                    for conflict in &found_conflicts {
                        match conflict {
                            ModelConflict::Migration(_, _, refusal) => {
                                assert!(migrated, "a migration conflict where none migrates");
                                counts.migration_conflicts += 1;
                                match refusal {
                                    ModelMigrationRefusal::Path(PathRefusal::MissingLink(_)) => {
                                        counts.missing_links += 1;
                                    }
                                    ModelMigrationRefusal::GetRefused(_) => {
                                        counts.refused_gets += 1
                                    }
                                    other => panic!("an unexpected refusal: {other:?}"),
                                }
                            }
                            ModelConflict::Tracking(_, ModelTrackingConflict::SchemaDiffers) => {
                                panic!("a migrating merge reported different schemas");
                            }
                            _ => {}
                        }
                    }
                    if migrated && found_conflicts.is_empty() {
                        assert!(
                            matches!(found_merged.get("k"), Some(ModelMerged::New(schema, _)) if *schema == names[1]),
                            "a migrated merge is recorded under the merged schema"
                        );
                    }
                    counts.clean += usize::from(found_conflicts.is_empty());
                    counts.conflicts += found_conflicts.len();
                }
            }
        }
    }

    /// The number of injective vertex migrations between two two-vertex
    /// schemas: the nine vertex maps less the four that contract.
    fn injective_count() -> usize {
        let all = migrations_count(
            &MigrationSizes {
                src_vertices: 2,
                tgt_vertices: 2,
                src_edges: 0,
                tgt_edges: 0,
                src_hyper_labels: Vec::new(),
                tgt_hyper_labels: Vec::new(),
            },
            MigrationScope::VERTICES,
        );
        usize::try_from(all - 4).unwrap()
    }

    #[test]
    fn every_history_of_the_merge_migration_scope_merges_as_the_reference_merges() {
        let schemas = panproto_reference::enumerate::lenses::schemas();
        let n = schemas.len();
        // Graphs: every source schema.
        let mut graphs = MergeMigrationCounts::default();
        let all: Vec<usize> = (0..n).collect();
        for history in histories(&all) {
            run_history(history, ScopeShape::Graph, None, &mut graphs);
        }
        // Trees: the edgeless source schema, so that the tree lens keeps
        // every tree's shape.
        let edgeless = schemas.iter().position(|s| s.edges.is_empty()).unwrap();
        let mut trees_counts = MergeMigrationCounts::default();
        for history in histories(&[edgeless]) {
            run_history(history, ScopeShape::Tree, None, &mut trees_counts);
        }
        println!("graph merge migrations: {graphs:?}");
        println!("tree merge migrations: {trees_counts:?}");
        // Each history: three base instances, the changed side's three
        // instances of the new schema and three of the old (stale), and three
        // unchanged instances; a graph has at most one node at one of two
        // vertices, and a sequence holds at most one one-node tree.
        let per_history = 3 * 6 * 3;
        let histories_over = |sources: usize| sources * n * injective_count() * 2 * 2;
        // A case migrates when the changed side records an instance of a new
        // schema (the source and target differ) and the unchanged side's
        // instance is not the base's: 3 * (9 - 3) per history.
        let migrating_histories =
            |sources: usize, same: usize| (sources * n - same) * injective_count() * 2 * 2;
        assert_eq!(graphs.cases, histories_over(n) * per_history);
        assert_eq!(graphs.cases, 27_000);
        assert_eq!(graphs.migrated, migrating_histories(n, n) * 18);
        // Graph get along a migration that sends no two vertices to one is
        // total; the base's path through our changed commit breaks exactly
        // when its link is not recorded, once per migrating case of those
        // histories.
        assert_eq!(graphs.refused_gets, 0);
        assert_eq!(graphs.missing_links, migrating_histories(n, n) / 4 * 18);
        assert_eq!(graphs.migration_conflicts, graphs.missing_links);
        assert_eq!(trees_counts.cases, histories_over(1) * per_history);
        assert_eq!(trees_counts.migrated, migrating_histories(1, 1) * 18);
        assert_eq!(
            [
                graphs.clean,
                graphs.conflicts,
                trees_counts.missing_links,
                trees_counts.refused_gets,
                trees_counts.clean,
                trees_counts.conflicts,
            ],
            MERGE_MIGRATION_OUTCOMES
        );
    }

    /// Graph clean cases and conflicts, tree missing links, refused gets,
    /// clean cases and conflicts.
    const MERGE_MIGRATION_OUTCOMES: [usize; 6] = [19_200, 7_800, 360, 672, 3_432, 2_136];

    // -- A stale instance migrated along first-parent history.

    /// The counts of a run over the stale migration scope.
    #[derive(Debug, Default, PartialEq, Eq)]
    struct StaleCounts {
        cases: usize,
        steps: usize,
        get_put: usize,
    }

    /// Migrate every graph over the first schema of a chain of three commits
    /// to the last commit's schema along the history the chain records, and
    /// back, against the composite of the reference graph lenses.
    fn run_stale_chain(
        [a, b, c]: [usize; 3],
        [f, g]: [usize; 2],
        sources: Option<&[reference_model::Graph]>,
        counts: &mut StaleCounts,
    ) {
        let schemas = panproto_reference::enumerate::lenses::schemas();
        let refs = [&schemas[a], &schemas[b], &schemas[c]];
        let steps = [
            injective_migrations(refs[0], refs[1])[f].clone(),
            injective_migrations(refs[1], refs[2])[g].clone(),
        ];
        let prods = refs.map(production_schema);
        let mut store = MemStore::new();
        let trees = [0, 1, 2].map(|i| store_schema_as_tree(&mut store, prods[i].clone()).unwrap());
        let stored = [0, 1].map(|i| {
            store_migration(
                &mut store,
                &prods[i],
                &prods[i + 1],
                production_mapping(refs[i], &steps[i]),
            )
            .unwrap()
        });
        let names = [a, b, c].map(schema_name);
        let identity = panproto_vcs::schema_identity(&store, &trees[0]).unwrap();
        let lenses: Vec<_> = (0..2)
            .map(|i| {
                panproto_reference::restrict::compile(refs[i], refs[i + 1], &steps[i]).unwrap()
            })
            .collect();
        let reference = |i: usize| {
            RefGraphLens::new(
                refs[i],
                refs[i + 1],
                &steps[i],
                &lenses[i],
                &BTreeMap::new(),
                &NoValueLayer,
            )
            .unwrap()
        };
        let sources = sources.map_or_else(|| graphs_over(refs[0], 2, 1), <[_]>::to_vec);
        for source in sources {
            counts.cases += 1;
            let data = store
                .put(&Object::DataSet(
                    DataSetObject::from_instance(
                        trees[0],
                        &Instance::Graph(graph_to_production(&source, false)),
                        Some("k".to_owned()),
                    )
                    .unwrap(),
                ))
                .unwrap();
            let root = commit(&mut store, trees[0], None, &[data], None, "root");
            let first = commit(
                &mut store,
                trees[1],
                Some(root),
                &[],
                Some(stored[0]),
                "first",
            );
            let tip = commit(
                &mut store,
                trees[2],
                Some(first),
                &[],
                Some(stored[1]),
                "tip",
            );
            // Production: the path from the data set's schema, and the
            // migration along it.
            let path = migration_path(&store, tip, identity).unwrap();
            let migrated = migrate_along(&mut store, data, &path, &mut |_| {
                panic!("a graph step reads no protocol")
            })
            .unwrap();
            // The reference: the path the model reads off the chain, and the
            // composite of its gets.
            let chain = vec![
                ChainCommit {
                    name: tip.to_string(),
                    schema: names[2].clone(),
                    migration: Some(stored[1].to_string()),
                },
                ChainCommit {
                    name: first.to_string(),
                    schema: names[1].clone(),
                    migration: Some(stored[0].to_string()),
                },
                ChainCommit {
                    name: root.to_string(),
                    schema: names[0].clone(),
                    migration: None,
                },
            ];
            let migrations = BTreeMap::from([
                (stored[0].to_string(), (names[0].clone(), names[1].clone())),
                (stored[1].to_string(), (names[1].clone(), names[2].clone())),
            ]);
            let expected_path = model_migration_path(&chain, &migrations, &names[0]).unwrap();
            assert_eq!(
                path.iter().map(ToString::to_string).collect::<Vec<_>>(),
                expected_path
            );
            let mut view = source.clone();
            let mut complements = Vec::new();
            for name in &expected_path {
                let step = usize::from(*name == stored[1].to_string());
                let (next, complement) = reference(step).get(&view).unwrap();
                complements.push((step, complement));
                view = next;
            }
            assert_eq!(
                super::data_migration::stored_graph(&store, migrated.data_id),
                view,
                "{a} {b} {c} {f} {g} {source:?}"
            );
            assert_eq!(migrated.complement_ids.len(), expected_path.len());
            // Back along the path in reverse, against each step's complement.
            let mut back = migrated.data_id;
            for complement in migrated.complement_ids.iter().rev() {
                back = migrate_backward(
                    &mut store,
                    back,
                    *complement,
                    &panproto_schema::Protocol::default(),
                )
                .unwrap();
            }
            let mut restored = view;
            for (step, complement) in complements.iter().rev() {
                restored = reference(*step).put(&restored, complement).unwrap();
            }
            let back = super::data_migration::stored_graph(&store, back);
            assert_eq!(back, restored);
            counts.steps += expected_path.len();
            counts.get_put += usize::from(back == source);
        }
    }

    fn stale_scope() -> StaleCounts {
        let n = panproto_reference::enumerate::lenses::schemas().len();
        let mut counts = StaleCounts::default();
        for a in 0..n {
            for b in 0..n {
                for c in 0..n {
                    for f in 0..injective_count() {
                        for g in 0..injective_count() {
                            run_stale_chain([a, b, c], [f, g], None, &mut counts);
                        }
                    }
                }
            }
        }
        counts
    }

    #[test]
    fn every_stale_graph_of_the_chain_scope_migrates_as_the_composite_of_the_reference_gets() {
        let counts = stale_scope();
        println!("stale migration scope: {counts:?}");
        let schemas = panproto_reference::enumerate::lenses::schemas();
        let n = schemas.len();
        // Every graph over the first schema with at most two nodes and one
        // edge: seven over the edgeless schema, twenty-five over each other.
        let sources: usize = schemas
            .iter()
            .map(|schema| {
                usize::try_from(panproto_reference::enumerate::graphs_count(
                    2,
                    0,
                    1,
                    schema.edges.len() as u64,
                    1,
                    2,
                    1,
                ))
                .unwrap()
            })
            .sum();
        let chains_per_source = n * n * injective_count() * injective_count();
        assert_eq!(counts.cases, sources * chains_per_source);
        // The path starts at the nearest commit of the first schema: the tip
        // itself when its schema is the first (no step), and otherwise each
        // link between two schemas that differ is a step. For a fixed first
        // schema that sums, over the second and third, to 2 (n - 1)^2.
        let steps_per_case_sum = 2 * (n - 1) * (n - 1) * injective_count() * injective_count();
        assert_eq!(counts.steps, sources * steps_per_case_sum);
        // Get along a migration sending no two vertices to one followed by
        // put against its complement restores every source.
        assert_eq!(counts.get_put, counts.cases);
    }

    // -- Merges, rebases and cherry-picks through a repository whose
    // -- history changes the schema.

    /// The instance repository's schema as a reference schema, and the
    /// schema one branch moves to: the schema with a vertex `c` added, or
    /// with `b` and its edge dropped, with the vertex migration between them.
    fn schema_change(
        schema: &carrier::CoverageSchema,
        drop: bool,
    ) -> (RefSchema, RefSchema, RefMigration) {
        let mut old = RefSchema::default();
        for vertex in &schema.vertices {
            old = old.vertex(vertex, "object");
        }
        for edge in &schema.edges {
            old = old.edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref());
        }
        let mut new = RefSchema::default().vertex("a", "object");
        let mut migration = RefMigration::default();
        migration.surviving_verts.insert("a".to_owned());
        if drop {
            return (old, new, migration);
        }
        new = old.clone().vertex("c", "object");
        migration.surviving_verts.insert("b".to_owned());
        (old, new, migration)
    }

    /// The production schema of a reference schema under the instance
    /// repository's protocol.
    fn repository_schema(schema: &RefSchema) -> panproto_schema::Schema {
        let protocol = panproto_schema::Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            obj_kinds: vec!["object".into()],
            ..Default::default()
        };
        let mut builder = panproto_schema::SchemaBuilder::new(&protocol);
        for vertex in schema.vertices.keys() {
            builder = builder.vertex(vertex, "object", None).unwrap();
        }
        for edge in &schema.edges {
            builder = builder
                .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
                .unwrap();
        }
        builder.build().unwrap()
    }

    /// The operation a repository case runs.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Operation {
        /// Merge `feature` into `main`; the schema changes on ours (main)
        /// or theirs (feature).
        Merge { schema_on_ours: bool },
        /// Rebase `feature`, which changes the data, onto `main`, which
        /// changes the schema.
        Rebase,
        /// Cherry-pick `feature`'s commit onto `main`.
        CherryPick,
    }

    /// The reference outcome of one repository case: the merged (or
    /// replayed) instance of the key, or its conflicts.
    fn reference_repository_case(
        operation: Operation,
        (old, new, migration): (&RefSchema, &RefSchema, &RefMigration),
        [base, schema_side, data_side]: [&ModelInstance; 3],
    ) -> Result<ModelInstance, Vec<ModelConflict>> {
        let entry = |data: &str, schema: &str, instance: &ModelInstance| ModelEntry {
            data: data.to_owned(),
            schema: schema.to_owned(),
            instance: instance.clone(),
        };
        let base_entry = entry("base", "old", base);
        let schema_entry = entry("schema side", "new", schema_side);
        let data_entry = entry("data side", "old", data_side);
        let chain = |tip: &str, schema: &str, link: Option<&str>| {
            vec![
                ChainCommit {
                    name: tip.to_owned(),
                    schema: schema.to_owned(),
                    migration: link.map(str::to_owned),
                },
                ChainCommit {
                    name: "base".to_owned(),
                    schema: "old".to_owned(),
                    migration: None,
                },
                ChainCommit {
                    name: "schema".to_owned(),
                    schema: "old".to_owned(),
                    migration: None,
                },
            ]
        };
        let schema_chain = chain("schema commit", "new", Some("m"));
        let data_chain = chain("data commit", "old", None);
        let schema_side_path = PathSide {
            chain: &schema_chain,
            migration: None,
        };
        let data_side_path = PathSide {
            chain: &data_chain,
            migration: Some("m"),
        };
        let migrations = BTreeMap::from([("m".to_owned(), ("old".to_owned(), "new".to_owned()))]);
        let schema_on_ours = !matches!(
            operation,
            Operation::Merge {
                schema_on_ours: false
            }
        );
        let sides = if schema_on_ours {
            [schema_side_path, data_side_path]
        } else {
            [data_side_path, schema_side_path]
        };
        let mut migrate = |version: ModelVersion,
                           entry: &ModelEntry|
         -> Result<ModelInstance, ModelMigrationRefusal> {
            let path = merge_path(version, &entry.schema, sides, &migrations, "new")
                .map_err(ModelMigrationRefusal::Path)?;
            let mut instance = entry.instance.clone();
            for step in path {
                instance = reference_get((old, new, migration), &step, &instance)?;
            }
            Ok(instance)
        };
        let versions = if schema_on_ours {
            [&base_entry, &schema_entry, &data_entry]
        } else {
            [&base_entry, &data_entry, &schema_entry]
        };
        let rebase = !matches!(operation, Operation::Merge { .. });
        panproto_reference::pages::instance_version_control::migrate_and_combine(
            rebase,
            "g",
            versions,
            &mut migrate,
        )
        .unwrap()
        .map(|instance| normalized_instance(&instance))
    }

    /// Production's outcome of one repository case.
    fn production_repository_case(
        operation: Operation,
        new: &RefSchema,
        [base, schema_side, data_side]: [&ModelInstance; 3],
    ) -> Result<ModelInstance, Vec<ModelConflict>> {
        let (_dir, mut repo, _) = instance_repository();
        stage(&mut repo, "g", &instance_to_production(base));
        let base_id = repo.commit("base", "test").unwrap();
        panproto_vcs::refs::create_branch(repo.store_mut(), "feature", base_id).unwrap();
        let change_schema = |repo: &mut Repository| {
            repo.add(&repository_schema(new)).unwrap();
            stage(repo, "g", &instance_to_production(schema_side));
            repo.commit("schema side", "test").unwrap()
        };
        let change_data = |repo: &mut Repository| {
            stage(repo, "g", &instance_to_production(data_side));
            repo.commit("data side", "test").unwrap()
        };
        let schema_on_ours = !matches!(
            operation,
            Operation::Merge {
                schema_on_ours: false
            }
        );
        let (main_id, feature_id) = if schema_on_ours {
            let main_id = change_schema(&mut repo);
            panproto_vcs::refs::checkout_branch(repo.store_mut(), "feature").unwrap();
            (main_id, change_data(&mut repo))
        } else {
            let main_id = change_data(&mut repo);
            panproto_vcs::refs::checkout_branch(repo.store_mut(), "feature").unwrap();
            (main_id, change_schema(&mut repo))
        };
        let conflicts = |found: &[InstanceConflict]| -> Vec<ModelConflict> {
            found.iter().map(conflict_to_reference).collect()
        };
        match operation {
            Operation::Merge { .. } => {
                panproto_vcs::refs::checkout_branch(repo.store_mut(), "main").unwrap();
                let result = repo.merge("feature", "test").unwrap();
                if !result.instance_conflicts.is_empty() {
                    return Err(conflicts(&result.instance_conflicts));
                }
            }
            Operation::Rebase => match repo.rebase(main_id, "test") {
                Ok(_) => {}
                Err(VcsError::InstanceConflicts { conflicts: found }) => {
                    return Err(conflicts(&found));
                }
                Err(other) => panic!("the rebase failed otherwise: {other:?}"),
            },
            Operation::CherryPick => {
                panproto_vcs::refs::checkout_branch(repo.store_mut(), "main").unwrap();
                match repo.cherry_pick(feature_id, "test") {
                    Ok(_) => {}
                    Err(VcsError::InstanceConflicts { conflicts: found }) => {
                        return Err(conflicts(&found));
                    }
                    Err(other) => panic!("the cherry-pick failed otherwise: {other:?}"),
                }
            }
        }
        let committed = repo.committed_instance("HEAD", "g").unwrap();
        assert!(
            !committed.stale,
            "the combined instance is of HEAD's schema"
        );
        Ok(instance_to_reference(&committed.instance))
    }

    /// Clean cases and conflicts of the merges, the rebases and the
    /// cherry-picks of the repository scope.
    const REPOSITORY_OUTCOMES: [usize; 6] = [12, 36, 6, 18, 6, 18];

    #[test]
    fn every_schema_changing_history_of_the_repository_scope_merges_rebases_and_cherry_picks_as_the_reference()
     {
        let (_dir, _repo, schema) = instance_repository();
        let mut cases = [0usize; 3];
        let mut outcomes = [0usize; 6];
        // The schema merge refuses a branch that deletes a vertex the other
        // keeps (the merge's migration from the other side is not total), so
        // the branch that changes the schema adds a vertex.
        {
            let drop = false;
            let (old, new, migration) = schema_change(&schema, drop);
            let old_graphs: Vec<ModelInstance> = graphs_over(&old, 1, 0)
                .into_iter()
                .map(ModelInstance::Graph)
                .collect();
            let new_graphs: Vec<ModelInstance> = graphs_over(&new, 1, 0)
                .into_iter()
                .map(ModelInstance::Graph)
                .collect();
            for operation in [
                Operation::Merge {
                    schema_on_ours: true,
                },
                Operation::Merge {
                    schema_on_ours: false,
                },
                Operation::Rebase,
                Operation::CherryPick,
            ] {
                let slot = match operation {
                    Operation::Merge { .. } => 0,
                    Operation::Rebase => 1,
                    Operation::CherryPick => 2,
                };
                for base in &old_graphs {
                    for schema_side in &new_graphs {
                        for data_side in old_graphs.iter().filter(|g| *g != base) {
                            let versions = [base, schema_side, data_side];
                            let expected = reference_repository_case(
                                operation,
                                (&old, &new, &migration),
                                versions,
                            );
                            let found = production_repository_case(operation, &new, versions);
                            assert_eq!(found, expected, "{operation:?} drop {drop}: {versions:?}");
                            cases[slot] += 1;
                            match found {
                                Ok(_) => outcomes[2 * slot] += 1,
                                Err(found) => outcomes[2 * slot + 1] += found.len(),
                            }
                        }
                    }
                }
            }
        }
        println!("repository scope outcomes: {outcomes:?}");
        // Graphs of at most one node: three over the old schema and four over
        // the one adding `c`; the data side records one of the two old graphs
        // that are not the base's.
        let per_history = 3 * 4 * 2;
        assert_eq!(cases, [2 * per_history, per_history, per_history]);
        assert_eq!(outcomes, REPOSITORY_OUTCOMES);
    }

    #[test]
    fn a_stale_tracked_instance_migrates_along_first_parent_history_and_is_committed() {
        let (_dir, mut repo, schema) = instance_repository();
        let (old, added, add_c) = schema_change(&schema, false);
        let third = RefSchema::default()
            .vertex("a", "object")
            .vertex("c", "object");
        let mut drop_b = RefMigration::default();
        drop_b.surviving_verts.insert("a".to_owned());
        drop_b.surviving_verts.insert("c".to_owned());
        let source = ModelInstance::Graph(
            graphs_over(&old, 2, 0)
                .into_iter()
                .find(|graph| {
                    graph.nodes.len() == 2 && graph.nodes[&0].anchor != graph.nodes[&1].anchor
                })
                .unwrap(),
        );
        stage(&mut repo, "g", &instance_to_production(&source));
        repo.commit("data", "test").unwrap();
        repo.add(&repository_schema(&added)).unwrap();
        repo.commit("add c", "test").unwrap();
        repo.add(&repository_schema(&third)).unwrap();
        repo.commit("drop b", "test").unwrap();
        assert_eq!(repo.stale_instances().unwrap(), vec!["g".to_owned()]);
        assert!(repo.committed_instance("HEAD", "g").unwrap().stale);

        let index = repo.migrate_tracked_instance("g").unwrap();
        assert_eq!(index.staged_complements.len(), 2);
        let staged = index.staged_complements.clone();
        repo.commit("migrate g", "test").unwrap();
        let head = head_commit(&repo);
        assert_eq!(head.complement_ids, staged);
        // Each complement names the migration of the commit it crossed, in
        // path order: "add c", then "drop b".
        let first_parent =
            |commit: &CommitObject| match repo.store().get(&commit.parents[0]).unwrap() {
                Object::Commit(parent) => parent,
                other => panic!("a parent commit, not {}", other.type_name()),
            };
        let drop_commit = first_parent(&head);
        let add_commit = first_parent(&drop_commit);
        let named: Vec<Option<ObjectId>> = staged
            .iter()
            .map(|id| match repo.store().get(id).unwrap() {
                Object::Complement(complement) => complement.migration(),
                other => panic!("a complement, not {}", other.type_name()),
            })
            .collect();
        assert_eq!(
            named,
            vec![add_commit.migration_id, drop_commit.migration_id]
        );

        // The committed instance is current, and is the composite of the
        // reference gets.
        let committed = repo.committed_instance("HEAD", "g").unwrap();
        assert!(!committed.stale);
        let expected = reference_get(
            (&added, &third, &drop_b),
            "drop b",
            &reference_get((&old, &added, &add_c), "add c", &source).unwrap(),
        )
        .unwrap();
        assert_eq!(instance_to_reference(&committed.instance), expected);
        assert_eq!(repo.stale_instances().unwrap(), Vec::<String>::new());

        // A current instance migrates along the empty path: nothing is
        // staged.
        let index = repo.migrate_tracked_instance("g").unwrap();
        assert!(index.staged_data.is_empty() && index.staged_complements.is_empty());

        // Back along the complements in reverse restores the source.
        let protocol = panproto_schema::Protocol::default();
        let mut back = committed.data_id;
        for complement in staged.iter().rev() {
            back = migrate_backward(repo.store_mut(), back, *complement, &protocol).unwrap();
        }
        let Object::DataSet(restored) = repo.store().get(&back).unwrap() else {
            panic!("a data set");
        };
        assert_eq!(instance_to_reference(&restored.decode().unwrap()), source);

        // A key HEAD does not track is refused.
        assert!(matches!(
            repo.migrate_tracked_instance("absent"),
            Err(VcsError::NotTracked { key }) if key == "absent"
        ));
    }

    #[test]
    fn a_missing_link_or_a_refused_get_refuses_a_stale_migration_with_its_conflict() {
        // A commit that changes the schema and records no migration.
        let (_dir, mut repo, schema) = instance_repository();
        let (old, added, _) = schema_change(&schema, false);
        stage(
            &mut repo,
            "g",
            &instance_to_production(&ModelInstance::Graph(graphs_over(&old, 1, 0)[1].clone())),
        );
        repo.commit("data", "test").unwrap();
        let head = head_commit(&repo);
        let parent = panproto_vcs::store::resolve_head(repo.store())
            .unwrap()
            .unwrap();
        let tree = store_schema_as_tree(repo.store_mut(), repository_schema(&added)).unwrap();
        let unlinked = CommitObject::builder(tree, head.protocol.clone(), "test", "unlinked")
            .timestamp(0)
            .parents(vec![parent])
            .build();
        let unlinked = repo.store_mut().put(&Object::Commit(unlinked)).unwrap();
        repo.store_mut()
            .set_ref("refs/heads/main", unlinked)
            .unwrap();
        match repo.migrate_tracked_instance("g") {
            Err(VcsError::Migration(conflict)) => assert_eq!(
                *conflict,
                MigrationConflict::MissingLink { commit: unlinked }
            ),
            other => panic!("a missing link was not refused: {other:?}"),
        }
        assert!(repo.read_index().unwrap().staged_data.is_empty());

        // A tree whose root's vertex the migration drops: get refuses its
        // record.
        let schemas = panproto_reference::enumerate::lenses::schemas();
        let edgeless = schemas.iter().find(|s| s.edges.is_empty()).unwrap();
        let other = schemas.iter().find(|s| !s.edges.is_empty()).unwrap();
        let drop_v0 = injective_migrations(edgeless, other)
            .into_iter()
            .find(|m| !m.survives("v0") && m.survives("v1"))
            .unwrap();
        let (src, tgt) = (production_schema(edgeless), production_schema(other));
        let mut store = MemStore::new();
        let src_tree = store_schema_as_tree(&mut store, src.clone()).unwrap();
        let tgt_tree = store_schema_as_tree(&mut store, tgt.clone()).unwrap();
        let stored = store_migration(
            &mut store,
            &src,
            &tgt,
            production_mapping(edgeless, &drop_v0),
        )
        .unwrap();
        let tree = trees(&TreeScope {
            nodes: 1,
            anchors: &["v0"],
            values: &[],
            arc_names: &[],
            hyper_edges: &[],
            max_fans: 0,
        })
        .remove(0);
        let (data, _) = data_set(
            &mut store,
            (src_tree, "source"),
            &ModelInstance::Trees(vec![tree.clone(), tree]),
        );
        let root = commit(&mut store, src_tree, None, &[data], None, "root");
        let tip = commit(&mut store, tgt_tree, Some(root), &[], Some(stored), "tip");
        let identity = panproto_vcs::schema_identity(&store, &src_tree).unwrap();
        let path = migration_path(&store, tip, identity).unwrap();
        assert_eq!(path, vec![stored]);
        match migrate_along(&mut store, data, &path, &mut |_| {
            panic!("no protocol is read")
        }) {
            Err(VcsError::Migration(conflict)) => assert!(matches!(
                *conflict,
                MigrationConflict::OutsideGetDomain { data_id, record: Some(0), .. } if data_id == data
            )),
            other => panic!("a refused get was not refused: {other:?}"),
        }
        // The reference refuses the same record.
        assert_eq!(
            reference_get(
                (edgeless, other, &drop_v0),
                "drop v0",
                &ModelInstance::Trees(vec![normalized(
                    &trees(&TreeScope {
                        nodes: 1,
                        anchors: &["v0"],
                        values: &[],
                        arc_names: &[],
                        hyper_edges: &[],
                        max_fans: 0,
                    })[0]
                )]),
            ),
            Err(ModelMigrationRefusal::GetRefused(Some(0)))
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn randomized_merge_migrations_agree_with_the_reference(
            src in 0..5_usize,
            tgt in 0..5_usize,
            migration in 0..5_usize,
            changed_is_ours in any::<bool>(),
            link in any::<bool>(),
            picks in prop::array::uniform4(any::<prop::sample::Index>()),
        ) {
            let schemas = panproto_reference::enumerate::lenses::schemas();
            let old: Vec<ModelInstance> = graphs_over(&schemas[src], 3, 2)
                .into_iter()
                .map(ModelInstance::Graph)
                .collect();
            let new: Vec<ModelInstance> = graphs_over(&schemas[tgt], 3, 2)
                .into_iter()
                .map(ModelInstance::Graph)
                .collect();
            let pick = |from: &[ModelInstance], at: &prop::sample::Index| from[at.index(from.len())].clone();
            let [base, old_changed, new_changed, unchanged] = [
                pick(&old, &picks[0]),
                pick(&old, &picks[1]),
                pick(&new, &picks[2]),
                pick(&old, &picks[3]),
            ];
            let mut counts = MergeMigrationCounts::default();
            run_history(
                History { src, tgt, migration, changed_is_ours, link },
                ScopeShape::Graph,
                Some([&base, &old_changed, &new_changed, &unchanged]),
                &mut counts,
            );
            prop_assert!(counts.cases > 0);
        }

        #[test]
        fn randomized_stale_graphs_migrate_as_the_composite_of_the_reference_gets(
            schemas in prop::array::uniform3(0..5_usize),
            steps in prop::array::uniform2(0..5_usize),
            source in any::<prop::sample::Index>(),
        ) {
            let all = panproto_reference::enumerate::lenses::schemas();
            let sources = graphs_over(&all[schemas[0]], 3, 2);
            let source = sources[source.index(sources.len())].clone();
            let mut counts = StaleCounts::default();
            run_stale_chain(schemas, steps, Some(&[source]), &mut counts);
            prop_assert_eq!(counts.get_put, 1);
        }
    }
}
