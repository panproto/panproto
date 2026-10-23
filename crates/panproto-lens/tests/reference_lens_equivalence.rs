//! Independent normalization-oracle evidence for L.7.

#![allow(clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::{
    EdgeId, FInstance, FieldPresence, GInstance, Node, NodeShape, RowId, SynthIndex, SynthKey,
    Value, WInstance,
};
use panproto_lens::lens_shape::{GraphShape, LensShape, TreeShape};
use panproto_lens::table_shape::TableShape;
use panproto_reference::enumerate::lenses::{
    LawEquivalenceIncoherence, LawEquivalenceOracleCase, LawEquivalenceShape,
    for_each_law_equivalence_oracle_case, law_equivalence_oracle_case_count_certificate,
};
use panproto_schema::Edge;
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

fn leaf(id: u32, ordinal: usize, changed: bool) -> Node {
    let mut node = Node::new(id, "leaf");
    let value = if changed && ordinal == 0 {
        "changed"
    } else {
        ["left", "right"][ordinal]
    };
    node.extra_fields
        .insert("payload".to_owned(), Value::Str(value.to_owned()));
    node.position = Some(u32::try_from(ordinal).expect("two leaves"));
    node
}

fn graph_fixture(presentation: Presentation, changed: bool) -> (GInstance, SynthIndex) {
    let labels = [relation("left"), relation("right")];
    let mut graph = GInstance::new().with_node(Node::new(0, "root"));
    for (ordinal, label) in labels.into_iter().enumerate() {
        graph
            .insert_node(leaf(presentation.nodes[ordinal], ordinal, changed))
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
    let nodes = HashMap::from([
        (0, Node::new(0, "root")),
        (
            presentation.nodes[0],
            leaf(presentation.nodes[0], 0, changed),
        ),
        (
            presentation.nodes[1],
            leaf(presentation.nodes[1], 1, changed),
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
        table
            .insert_row(
                "leaf",
                RowId(presentation.nodes[ordinal]),
                HashMap::from([("payload".to_owned(), Value::Str(value.to_owned()))]),
            )
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

#[derive(Clone, Debug, PartialEq)]
struct CanonicalNode {
    anchor: Name,
    value: Option<FieldPresence>,
    discriminator: Option<Name>,
    fields: BTreeMap<String, Value>,
    position: Option<u32>,
    shape: NodeShape,
    annotations: BTreeMap<String, Value>,
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
                    value: node.value.clone(),
                    discriminator: node.discriminator.clone(),
                    fields: node
                        .extra_fields
                        .iter()
                        .map(|(field, value)| (field.clone(), value.clone()))
                        .collect(),
                    position: node.position,
                    shape: node.shape.clone(),
                    annotations: node
                        .annotations
                        .iter()
                        .map(|(field, value)| (field.clone(), value.clone()))
                        .collect(),
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
        }
    });
    assert_eq!(count, law_equivalence_oracle_case_count_certificate());
    assert_eq!(count, 147);
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
    }
}
