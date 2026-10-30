//! Independent normalization-oracle evidence for L.7.

#![allow(clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_expr::{Expr, Literal};
use panproto_gat::{CoercionClass, Name};
use panproto_inst::{
    CompiledMigration, Complement, EdgeId, FInstance, FieldPresence, FieldTransform, GInstance,
    GraphComplement, Node, NodeShape, RowId, SynthIndex, SynthKey, TableComplement, Value,
    WInstance,
};
use panproto_lens::asymmetric::fresh_view_node_id as fresh_tree_view_node_id;
use panproto_lens::graph_lens::{fresh_view_edge_id, fresh_view_node_id};
use panproto_lens::lens_shape::{GraphShape, LensShape, TreeShape};
use panproto_lens::table_lens::fresh_view_row_id;
use panproto_lens::table_shape::TableShape;
use panproto_lens::{Lens, TableContext};
use panproto_reference::enumerate::lenses::{
    LAW_EQUIVALENCE_FLOAT_WITNESSES, LawEquivalenceIncoherence, LawEquivalenceOracleCase,
    LawEquivalenceShape, for_each_law_equivalence_oracle_case,
    law_equivalence_oracle_case_count_certificate,
};
use panproto_schema::{Edge, Protocol, ScalarKind, SchemaBuilder};
use proptest::prelude::*;

#[derive(Clone, Copy, Debug)]
struct Presentation {
    nodes: [u32; 2],
    edges: [u32; 2],
}

const PRESENTATIONS: [Presentation; 3] = [
    Presentation {
        nodes: [1, 2],
        edges: [7, 8],
    },
    Presentation {
        nodes: [11, 15],
        edges: [17, 19],
    },
    Presentation {
        nodes: [21, 29],
        edges: [31, 37],
    },
];

fn relation(name: &str) -> Edge {
    Edge {
        src: "root".into(),
        tgt: "leaf".into(),
        kind: "rel".into(),
        name: Some(name.into()),
    }
}

const fn node_keys() -> [SynthKey; 2] {
    [
        SynthKey::Expand {
            source_edge: EdgeId(100),
            hop: 0,
        },
        SynthKey::Expand {
            source_edge: EdgeId(101),
            hop: 0,
        },
    ]
}

const fn edge_keys() -> [SynthKey; 2] {
    [
        SynthKey::Expand {
            source_edge: EdgeId(200),
            hop: 1,
        },
        SynthKey::Expand {
            source_edge: EdgeId(201),
            hop: 1,
        },
    ]
}

/// The float payload of the first leaf, when a fixture carries one: the leaf's
/// own value and its `measure` field (a table cell) both hold it.
fn float_payload(bits: u64) -> Value {
    Value::Float(f64::from_bits(bits))
}

fn leaf(id: u32, ordinal: usize, changed: bool, float: Option<u64>) -> Node {
    let mut node = Node::new(id, "leaf");
    let value = if changed && ordinal == 0 {
        "changed"
    } else {
        ["left", "right"][ordinal]
    };
    node.extra_fields
        .insert("payload".to_owned(), Value::Str(value.to_owned()));
    if let Some(bits) = float.filter(|_| ordinal == 0) {
        node.value = Some(FieldPresence::Present(float_payload(bits)));
        node.extra_fields
            .insert("measure".to_owned(), float_payload(bits));
    }
    node.position = Some(u32::try_from(ordinal).expect("two leaves"));
    node
}

fn graph_fixture(presentation: Presentation, changed: bool) -> (GInstance, SynthIndex) {
    graph_fixture_with(presentation, changed, None)
}

fn graph_fixture_with(
    presentation: Presentation,
    changed: bool,
    float: Option<u64>,
) -> (GInstance, SynthIndex) {
    let labels = [relation("left"), relation("right")];
    let mut graph = GInstance::new().with_node(Node::new(0, "root"));
    for (ordinal, label) in labels.into_iter().enumerate() {
        graph
            .insert_node(leaf(presentation.nodes[ordinal], ordinal, changed, float))
            .expect("distinct presentation node ids");
        graph
            .insert_edge(
                EdgeId(presentation.edges[ordinal]),
                panproto_inst::GEdge::new(0, presentation.nodes[ordinal], label),
            )
            .expect("fixture endpoints exist");
    }
    let nodes = presentation.nodes.into_iter().zip(node_keys()).collect();
    let edges = presentation
        .edges
        .into_iter()
        .map(EdgeId)
        .zip(edge_keys())
        .collect();
    (graph, SynthIndex { nodes, edges })
}

fn tree_fixture(presentation: Presentation, changed: bool) -> (WInstance, SynthIndex) {
    tree_fixture_with(presentation, changed, None)
}

fn tree_fixture_with(
    presentation: Presentation,
    changed: bool,
    float: Option<u64>,
) -> (WInstance, SynthIndex) {
    let nodes = HashMap::from([
        (0, Node::new(0, "root")),
        (
            presentation.nodes[0],
            leaf(presentation.nodes[0], 0, changed, float),
        ),
        (
            presentation.nodes[1],
            leaf(presentation.nodes[1], 1, changed, float),
        ),
    ]);
    let tree = WInstance::new(
        nodes,
        vec![
            (0, presentation.nodes[0], relation("left")),
            (0, presentation.nodes[1], relation("right")),
        ],
        Vec::new(),
        0,
        "root".into(),
    );
    let index = SynthIndex {
        nodes: presentation.nodes.into_iter().zip(node_keys()).collect(),
        edges: [EdgeId(0), EdgeId(1)]
            .into_iter()
            .zip(edge_keys())
            .collect(),
    };
    (tree, index)
}

fn table_fixture(presentation: Presentation, changed: bool) -> (FInstance, SynthIndex) {
    table_fixture_with(presentation, changed, None)
}

fn table_fixture_with(
    presentation: Presentation,
    changed: bool,
    float: Option<u64>,
) -> (FInstance, SynthIndex) {
    let mut table = FInstance::new();
    table
        .insert_row("root", RowId(0), HashMap::new())
        .expect("root row");
    for ordinal in 0..2 {
        let value = if changed && ordinal == 0 {
            "changed"
        } else {
            ["left", "right"][ordinal]
        };
        let mut cells = HashMap::from([("payload".to_owned(), Value::Str(value.to_owned()))]);
        if let Some(bits) = float.filter(|_| ordinal == 0) {
            cells.insert("measure".to_owned(), float_payload(bits));
        }
        table
            .insert_row("leaf", RowId(presentation.nodes[ordinal]), cells)
            .expect("distinct presentation row ids");
        table
            .insert_foreign_key(
                relation(["left", "right"][ordinal]),
                RowId(0),
                RowId(presentation.nodes[ordinal]),
            )
            .expect("foreign-key endpoints exist");
    }
    let (graph, _) = GInstance::from_finstance(&table).expect("table graph embedding");
    let mut edges = BTreeMap::new();
    for (id, edge) in graph.edges_sorted() {
        let ordinal = usize::from(edge.tgt == presentation.nodes[1]);
        edges.insert(id, edge_keys()[ordinal].clone());
    }
    let index = SynthIndex {
        nodes: presentation.nodes.into_iter().zip(node_keys()).collect(),
        edges,
    };
    (table, index)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum StableId {
    Origin(u32),
    Synthesized(SynthKey),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum StableEdgeId {
    Origin(EdgeId),
    Synthesized(SynthKey),
}

/// A value's class under the value relation of law equivalence, built
/// without calling any lens-shape code: a float becomes its binary64 bits,
/// except that `-0.0` joins `+0.0` and every NaN joins one class.
#[derive(Clone, Debug, PartialEq)]
enum OracleValue {
    Float(u64),
    List(Vec<Self>),
    Record(BTreeMap<String, Self>),
    Opaque(String, BTreeMap<String, Self>),
    Exact(Value),
}

/// The class every NaN joins.
const ORACLE_NAN: u64 = 0x7ff8_0000_0000_0000;

fn oracle_value(value: &Value) -> OracleValue {
    match value {
        Value::Float(float) if float.is_nan() => OracleValue::Float(ORACLE_NAN),
        Value::Float(float) if float.to_bits() << 1 == 0 => OracleValue::Float(0),
        Value::Float(float) => OracleValue::Float(float.to_bits()),
        Value::List(items) => OracleValue::List(items.iter().map(oracle_value).collect()),
        Value::Unknown(fields) => OracleValue::Record(oracle_fields(fields)),
        Value::Opaque { type_, fields } => {
            OracleValue::Opaque(type_.clone(), oracle_fields(fields))
        }
        other => OracleValue::Exact(other.clone()),
    }
}

fn oracle_fields(fields: &HashMap<String, Value>) -> BTreeMap<String, OracleValue> {
    fields
        .iter()
        .map(|(field, value)| (field.clone(), oracle_value(value)))
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
enum OraclePresence {
    Present(OracleValue),
    Null,
    Absent,
}

fn oracle_presence(presence: &FieldPresence) -> OraclePresence {
    match presence {
        FieldPresence::Present(value) => OraclePresence::Present(oracle_value(value)),
        FieldPresence::Null => OraclePresence::Null,
        FieldPresence::Absent => OraclePresence::Absent,
    }
}

#[derive(Clone, Debug, PartialEq)]
struct CanonicalNode {
    anchor: Name,
    value: Option<OraclePresence>,
    discriminator: Option<Name>,
    fields: BTreeMap<String, OracleValue>,
    position: Option<u32>,
    shape: NodeShape,
    annotations: BTreeMap<String, OracleValue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CanonicalEdge {
    source: StableId,
    target: StableId,
    label: Edge,
    position: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
struct CanonicalGraph {
    nodes: BTreeMap<StableId, CanonicalNode>,
    edges: BTreeMap<StableEdgeId, CanonicalEdge>,
}

fn stable_node(id: u32, index: &SynthIndex) -> StableId {
    index
        .nodes
        .get(&id)
        .cloned()
        .map_or(StableId::Origin(id), StableId::Synthesized)
}

/// Normalize presentation ids to synthesis keys without calling any lens-shape code.
fn normalize_graph(graph: &GInstance, index: &SynthIndex) -> Option<CanonicalGraph> {
    if index.nodes.values().collect::<BTreeSet<_>>().len() != index.nodes.len()
        || index.edges.values().collect::<BTreeSet<_>>().len() != index.edges.len()
        || index.nodes.keys().any(|id| graph.node(*id).is_none())
        || index.edges.keys().any(|id| graph.edge(*id).is_none())
    {
        return None;
    }
    let nodes = graph
        .nodes()
        .iter()
        .map(|(&id, node)| {
            (
                stable_node(id, index),
                CanonicalNode {
                    anchor: node.anchor.clone(),
                    value: node.value.as_ref().map(oracle_presence),
                    discriminator: node.discriminator.clone(),
                    fields: oracle_fields(&node.extra_fields),
                    position: node.position,
                    shape: node.shape.clone(),
                    annotations: oracle_fields(&node.annotations),
                },
            )
        })
        .collect();
    let edges = graph
        .edges_sorted()
        .map(|(id, edge)| {
            let stable = index
                .edges
                .get(&id)
                .cloned()
                .map_or(StableEdgeId::Origin(id), StableEdgeId::Synthesized);
            (
                stable,
                CanonicalEdge {
                    source: stable_node(edge.src, index),
                    target: stable_node(edge.tgt, index),
                    label: edge.edge.clone(),
                    position: edge.position,
                },
            )
        })
        .collect();
    Some(CanonicalGraph { nodes, edges })
}

fn graph_oracle(left: &GInstance, li: &SynthIndex, right: &GInstance, ri: &SynthIndex) -> bool {
    normalize_graph(left, li).is_some_and(|left| normalize_graph(right, ri) == Some(left))
}

fn tree_oracle(left: &WInstance, li: &SynthIndex, right: &WInstance, ri: &SynthIndex) -> bool {
    if stable_node(left.root, li) != stable_node(right.root, ri)
        || left.schema_root != right.schema_root
    {
        return false;
    }
    let per_parent_order = |tree: &WInstance, index: &SynthIndex| {
        let mut order: BTreeMap<StableId, Vec<(StableId, Edge)>> = BTreeMap::new();
        for (parent, child, edge) in &tree.arcs {
            order
                .entry(stable_node(*parent, index))
                .or_default()
                .push((stable_node(*child, index), edge.clone()));
        }
        order
    };
    let left_order = per_parent_order(left, li);
    let right_order = per_parent_order(right, ri);
    let Ok(left) = GInstance::from_wtype(left) else {
        return false;
    };
    let Ok(right) = GInstance::from_wtype(right) else {
        return false;
    };
    graph_oracle(&left, li, &right, ri) && left_order == right_order
}

fn table_oracle(left: &FInstance, li: &SynthIndex, right: &FInstance, ri: &SynthIndex) -> bool {
    let Ok((left, _)) = GInstance::from_finstance(left) else {
        return false;
    };
    let Ok((right, _)) = GInstance::from_finstance(right) else {
        return false;
    };
    graph_oracle(&left, li, &right, ri)
}

fn corrupt(index: &mut SynthIndex, family: LawEquivalenceIncoherence) {
    match family {
        LawEquivalenceIncoherence::DuplicateNodeKey => {
            let key = index.nodes.values().next().expect("two node keys").clone();
            let second = *index.nodes.keys().nth(1).expect("two nodes");
            index.nodes.insert(second, key);
        }
        LawEquivalenceIncoherence::DuplicateEdgeKey => {
            let key = index.edges.values().next().expect("two edge keys").clone();
            let second = *index.edges.keys().nth(1).expect("two edges");
            index.edges.insert(second, key);
        }
        LawEquivalenceIncoherence::MissingNode => {
            index.nodes.insert(
                u32::MAX,
                SynthKey::Expand {
                    source_edge: EdgeId(999),
                    hop: 0,
                },
            );
        }
        LawEquivalenceIncoherence::MissingEdge => {
            index.edges.insert(
                EdgeId(u32::MAX),
                SynthKey::Expand {
                    source_edge: EdgeId(999),
                    hop: 1,
                },
            );
        }
    }
}

fn assert_graph_pair(left: Presentation, right: Presentation, changed: bool) {
    let (left, li) = graph_fixture(left, false);
    let (right, ri) = graph_fixture(right, changed);
    let expected = graph_oracle(&left, &li, &right, &ri);
    assert_eq!(expected, !changed);
    assert_eq!(GraphShape::equivalent(&left, &li, &right, &ri), expected);
    assert_eq!(GraphShape::equivalent(&right, &ri, &left, &li), expected);
}

fn assert_tree_pair(left: Presentation, right: Presentation, changed: bool) {
    let (left, li) = tree_fixture(left, false);
    let (right, ri) = tree_fixture(right, changed);
    let expected = tree_oracle(&left, &li, &right, &ri);
    assert_eq!(expected, !changed);
    assert_eq!(TreeShape::equivalent(&left, &li, &right, &ri), expected);
    assert_eq!(TreeShape::equivalent(&right, &ri, &left, &li), expected);
}

fn assert_table_pair(left: Presentation, right: Presentation, changed: bool) {
    let (left, li) = table_fixture(left, false);
    let (right, ri) = table_fixture(right, changed);
    let expected = table_oracle(&left, &li, &right, &ri);
    assert_eq!(expected, !changed);
    assert_eq!(TableShape::equivalent(&left, &li, &right, &ri), expected);
    assert_eq!(TableShape::equivalent(&right, &ri, &left, &li), expected);
}

/// The independent class of each float witness: `+0.0` and `-0.0` share one,
/// the three NaNs share another, and `1.0` has its own.
const FLOAT_WITNESS_CLASSES: [u8; 6] = [0, 0, 1, 1, 1, 2];

fn assert_float_pair(shape: LawEquivalenceShape, left: u8, right: u8) {
    let left_bits = LAW_EQUIVALENCE_FLOAT_WITNESSES[usize::from(left)];
    let right_bits = LAW_EQUIVALENCE_FLOAT_WITNESSES[usize::from(right)];
    let expected =
        FLOAT_WITNESS_CLASSES[usize::from(left)] == FLOAT_WITNESS_CLASSES[usize::from(right)];
    let (left_presentation, right_presentation) = (PRESENTATIONS[0], PRESENTATIONS[1]);
    let (oracle, forward, backward) = match shape {
        LawEquivalenceShape::Graph => {
            let (l, li) = graph_fixture_with(left_presentation, false, Some(left_bits));
            let (r, ri) = graph_fixture_with(right_presentation, false, Some(right_bits));
            (
                graph_oracle(&l, &li, &r, &ri),
                GraphShape::equivalent(&l, &li, &r, &ri),
                GraphShape::equivalent(&r, &ri, &l, &li),
            )
        }
        LawEquivalenceShape::Tree => {
            let (l, li) = tree_fixture_with(left_presentation, false, Some(left_bits));
            let (r, ri) = tree_fixture_with(right_presentation, false, Some(right_bits));
            (
                tree_oracle(&l, &li, &r, &ri),
                TreeShape::equivalent(&l, &li, &r, &ri),
                TreeShape::equivalent(&r, &ri, &l, &li),
            )
        }
        LawEquivalenceShape::Table => {
            let (l, li) = table_fixture_with(left_presentation, false, Some(left_bits));
            let (r, ri) = table_fixture_with(right_presentation, false, Some(right_bits));
            (
                table_oracle(&l, &li, &r, &ri),
                TableShape::equivalent(&l, &li, &r, &ri),
                TableShape::equivalent(&r, &ri, &l, &li),
            )
        }
    };
    assert_eq!(
        oracle, expected,
        "{shape:?} float witnesses {left} and {right}"
    );
    assert_eq!(
        forward, expected,
        "{shape:?} float witnesses {left} and {right}"
    );
    assert_eq!(
        backward, expected,
        "{shape:?} float witnesses {right} and {left}"
    );
}

fn assert_pair(shape: LawEquivalenceShape, left: u8, right: u8, changed: bool) {
    let left = PRESENTATIONS[usize::from(left)];
    let right = PRESENTATIONS[usize::from(right)];
    match shape {
        LawEquivalenceShape::Graph => assert_graph_pair(left, right, changed),
        LawEquivalenceShape::Tree => assert_tree_pair(left, right, changed),
        LawEquivalenceShape::Table => assert_table_pair(left, right, changed),
    }
}

fn assert_triple(shape: LawEquivalenceShape, ids: [u8; 3]) {
    for id in ids {
        assert_pair(shape, id, id, false);
    }
    assert_pair(shape, ids[0], ids[1], false);
    assert_pair(shape, ids[1], ids[2], false);
    assert_pair(shape, ids[0], ids[2], false);
    assert_pair(shape, ids[2], ids[0], false);
}

fn assert_incoherent(shape: LawEquivalenceShape, family: LawEquivalenceIncoherence) {
    match shape {
        LawEquivalenceShape::Graph => {
            let (instance, coherent) = graph_fixture(PRESENTATIONS[0], false);
            let mut bad = coherent.clone();
            corrupt(&mut bad, family);
            assert!(normalize_graph(&instance, &bad).is_none());
            assert!(!GraphShape::equivalent(&instance, &bad, &instance, &bad));
            assert!(!GraphShape::equivalent(
                &instance, &bad, &instance, &coherent
            ));
            assert!(!GraphShape::equivalent(
                &instance, &coherent, &instance, &bad
            ));
        }
        LawEquivalenceShape::Tree => {
            let (instance, coherent) = tree_fixture(PRESENTATIONS[0], false);
            let mut bad = coherent.clone();
            corrupt(&mut bad, family);
            let graph = GInstance::from_wtype(&instance).expect("tree embedding");
            assert!(normalize_graph(&graph, &bad).is_none());
            assert!(!TreeShape::equivalent(&instance, &bad, &instance, &bad));
            assert!(!TreeShape::equivalent(
                &instance, &bad, &instance, &coherent
            ));
            assert!(!TreeShape::equivalent(
                &instance, &coherent, &instance, &bad
            ));
        }
        LawEquivalenceShape::Table => {
            let (instance, coherent) = table_fixture(PRESENTATIONS[0], false);
            let mut bad = coherent.clone();
            corrupt(&mut bad, family);
            let (graph, _) = GInstance::from_finstance(&instance).expect("table embedding");
            assert!(normalize_graph(&graph, &bad).is_none());
            assert!(!TableShape::equivalent(&instance, &bad, &instance, &bad));
            assert!(!TableShape::equivalent(
                &instance, &bad, &instance, &coherent
            ));
            assert!(!TableShape::equivalent(
                &instance, &coherent, &instance, &bad
            ));
        }
    }
}

fn assert_id_floor_divergence(shape: LawEquivalenceShape) {
    match shape {
        LawEquivalenceShape::Graph => {
            let zero = GraphComplement::empty();
            let mut one = zero.clone();
            one.id_floor.node = 1;
            one.id_floor.edge = 1;
            let mut same_except_floor = one.clone();
            same_except_floor.id_floor = zero.id_floor;
            assert_eq!(
                serde_json::to_value(&same_except_floor).expect("graph complement serializes"),
                serde_json::to_value(&zero).expect("graph complement serializes")
            );
            assert_eq!(
                GraphShape::complement_divergence(&zero, &one).as_deref(),
                Some("id_floor differs")
            );
            assert_eq!(
                GraphShape::complement_divergence(&one, &zero).as_deref(),
                Some("id_floor differs")
            );

            let view = GInstance::new();
            assert_eq!(
                fresh_view_node_id(&view, &zero).expect("floor-zero node"),
                0
            );
            assert_eq!(fresh_view_node_id(&view, &one).expect("floor-one node"), 1);
            assert_eq!(
                fresh_view_edge_id(&view, &zero).expect("floor-zero edge"),
                EdgeId(0)
            );
            assert_eq!(
                fresh_view_edge_id(&view, &one).expect("floor-one edge"),
                EdgeId(1)
            );
        }
        LawEquivalenceShape::Tree => {
            let zero = Complement::empty();
            let mut above_view = zero.clone();
            above_view.id_floor = 12;
            let mut same_except_floor = above_view.clone();
            same_except_floor.id_floor = zero.id_floor;
            assert_eq!(
                serde_json::to_value(&same_except_floor).expect("tree complement serializes"),
                serde_json::to_value(&zero).expect("tree complement serializes")
            );
            assert_eq!(
                TreeShape::complement_divergence(&zero, &above_view).as_deref(),
                Some("id_floor differs")
            );
            assert_eq!(
                TreeShape::complement_divergence(&above_view, &zero).as_deref(),
                Some("id_floor differs")
            );

            let view = WInstance::new(
                HashMap::from([(10, Node::new(10, "root"))]),
                Vec::new(),
                Vec::new(),
                10,
                "root".into(),
            );
            assert_eq!(
                fresh_tree_view_node_id(&view, &zero).expect("floor-zero tree node"),
                11
            );
            assert_eq!(
                fresh_tree_view_node_id(&view, &above_view).expect("floor-above-view tree node"),
                12
            );
        }
        LawEquivalenceShape::Table => {
            let zero = TableComplement::empty();
            let mut one = zero.clone();
            one.id_floor = 1;
            let mut same_except_floor = one.clone();
            same_except_floor.id_floor = zero.id_floor;
            assert_eq!(same_except_floor, zero);
            assert_eq!(
                TableShape::complement_divergence(&zero, &one).as_deref(),
                Some("id_floor differs")
            );
            assert_eq!(
                TableShape::complement_divergence(&one, &zero).as_deref(),
                Some("id_floor differs")
            );

            let view = FInstance::new();
            assert_eq!(
                fresh_view_row_id(&view, &zero).expect("floor-zero row"),
                RowId(0)
            );
            assert_eq!(
                fresh_view_row_id(&view, &one).expect("floor-one row"),
                RowId(1)
            );
        }
    }
}

fn assert_operational_table_floor_divergence() {
    let protocol = Protocol {
        value_kinds: BTreeMap::from([(
            "int".to_owned(),
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        )]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("t", "record", None)
        .expect("record vertex")
        .vertex("t.a", "int", None)
        .expect("integer vertex")
        .edge("t", "t.a", "prop", Some("a"))
        .expect("column edge")
        .build()
        .expect("one-column table schema");
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            field_transforms: HashMap::from([(
                "t".into(),
                vec![FieldTransform::ApplyExpr {
                    key: "a".to_owned(),
                    expr: Expr::Lit(Literal::Int(7)),
                    inverse: Some(Expr::Lit(Literal::Int(8))),
                    coercion_class: CoercionClass::Projection,
                }],
            )]),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    };
    let context = TableContext {
        src: protocol.clone(),
        tgt: protocol,
    };
    let empty_source = FInstance::new();
    let row_source = FInstance::new()
        .with_rows(
            "t",
            [(RowId(0), HashMap::from([("a".to_owned(), Value::Int(7))]))],
        )
        .expect("row 0 in table t");
    let (empty_view, floor_zero) =
        TableShape::get(&lens, &empty_source, &context).expect("empty source gets");
    let (shared_view, floor_one) =
        TableShape::get(&lens, &row_source, &context).expect("row source gets");
    assert_eq!(empty_view, FInstance::new());
    assert_eq!(shared_view, row_source);
    assert_eq!(floor_zero.id_floor, 0);
    assert_eq!(floor_one.id_floor, 1);
    let mut expected_floor_one = floor_zero.clone();
    expected_floor_one.id_floor = 1;
    assert_eq!(floor_one, expected_floor_one);
    assert_eq!(
        TableShape::complement_divergence(&floor_zero, &floor_one).as_deref(),
        Some("id_floor differs")
    );

    let restored_from_zero = TableShape::put(&lens, &shared_view, &floor_zero, &context)
        .expect("floor-zero complement puts the shared view");
    let restored_from_one = TableShape::put(&lens, &shared_view, &floor_one, &context)
        .expect("floor-one complement puts the shared view");
    assert_eq!(
        restored_from_zero
            .row(RowId(0))
            .and_then(|row| row.get("a")),
        Some(&Value::Int(8))
    );
    assert_eq!(restored_from_one, row_source);
    assert_ne!(restored_from_zero, restored_from_one);

    let (view_after_zero, _) = TableShape::get(&lens, &restored_from_zero, &context)
        .expect("the floor-zero restoration gets");
    let (view_after_one, _) = TableShape::get(&lens, &restored_from_one, &context)
        .expect("the floor-one restoration gets");
    assert_eq!(view_after_zero, shared_view);
    assert_eq!(view_after_one, shared_view);
}

fn assert_id_floor_is_law_observable() {
    for shape in LawEquivalenceShape::ALL {
        assert_id_floor_divergence(shape);
    }
    assert_operational_table_floor_divergence();
}

#[test]
fn lens_equivalence_normalization_certificate() {
    let mut count = 0;
    for_each_law_equivalence_oracle_case(|case| {
        count += 1;
        match case {
            LawEquivalenceOracleCase::Pair {
                shape,
                left,
                right,
                changed,
            } => assert_pair(shape, left, right, changed),
            LawEquivalenceOracleCase::Triple {
                shape,
                first,
                second,
                third,
            } => assert_triple(shape, [first, second, third]),
            LawEquivalenceOracleCase::Incoherent { shape, family } => {
                assert_incoherent(shape, family);
            }
            LawEquivalenceOracleCase::FloorDivergence { shape } => {
                assert_id_floor_divergence(shape);
            }
            LawEquivalenceOracleCase::OperationalTableFloor => {
                assert_operational_table_floor_divergence();
            }
            LawEquivalenceOracleCase::FloatValue { shape, left, right } => {
                assert_float_pair(shape, left, right);
            }
        }
    });
    assert_eq!(count, law_equivalence_oracle_case_count_certificate());
    assert_eq!(count, 259);
}

#[test]
fn id_floor_divergence_is_law_observable() {
    assert_id_floor_is_law_observable();
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_lens_equivalence_normalization(
        left_base in 1_u16..=8_000,
        right_base in 8_001_u16..=16_000,
        left_gap in 1_u16..=500,
        right_gap in 1_u16..=500,
        changed in any::<bool>(),
        float_left in 0_u8..6,
        float_right in 0_u8..6,
    ) {
        let left_base = u32::from(left_base);
        let right_base = u32::from(right_base);
        let left = Presentation {
            nodes: [left_base, left_base + u32::from(left_gap)],
            edges: [20_000 + left_base, 29_000 + left_base],
        };
        let right = Presentation {
            nodes: [right_base, right_base + u32::from(right_gap)],
            edges: [40_000 + right_base, 59_000 + right_base],
        };
        assert_graph_pair(left, right, changed);
        assert_tree_pair(left, right, changed);
        assert_table_pair(left, right, changed);

        let mut triple = [0, 1, 2];
        triple.rotate_left(usize::try_from((left_base + right_base) % 3).expect("rotation < 3"));
        for shape in LawEquivalenceShape::ALL {
            assert_triple(shape, triple);
            for family in LawEquivalenceIncoherence::ALL {
                assert_incoherent(shape, family);
            }
            assert_float_pair(shape, float_left, float_right);
        }
        assert_id_floor_is_law_observable();
    }
}
