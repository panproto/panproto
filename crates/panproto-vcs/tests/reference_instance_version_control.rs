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
