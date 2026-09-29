//! A graph instance is a tree exactly when `tree_shape` says so.
//!
//! `GInstance::tree_shape` classifies a graph as a tree, a forest or
//! neither, with a typed reason; `try_to_wtype` and `to_wtypes` read the
//! tree or trees out and fail with that reason; and whenever the shape is a
//! tree, reading it out and laying it back as a graph gives the graph again,
//! up to the fan ids the embedding relates.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, HashMap};

use panproto_gat::Name;
use panproto_inst::{
    EdgeId, Fan, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND, InstError, Node, NodeShape,
    NotTreeReason, TreeEmbedding, TreeShapeReport, reify_fans,
};
use panproto_schema::{Edge, HyperEdge, Schema};
use proptest::prelude::*;

fn schema() -> Schema {
    Schema::default()
}

fn entry_schema(entries: &[&str]) -> Schema {
    Schema {
        entries: entries.iter().map(|e| Name::from(*e)).collect(),
        ..Schema::default()
    }
}

fn link() -> Edge {
    Edge {
        src: "item".into(),
        tgt: "item".into(),
        kind: "link".into(),
        name: None,
    }
}

fn items(ids: &[u32]) -> GInstance {
    ids.iter().fold(GInstance::new(), |g, &id| {
        g.with_node(Node::new(id, "item"))
    })
}

fn not_tree(graph: &GInstance) -> NotTreeReason {
    match graph.tree_shape(&schema()) {
        TreeShapeReport::NotTree { reason } => reason,
        other => panic!("expected not a tree, got {other:?}"),
    }
}

#[test]
fn tree_shape_classifies_trees_forests_and_non_trees() {
    let tree = items(&[0, 1, 2])
        .with_edge(0, 1, link())
        .with_edge(0, 2, link());
    assert_eq!(
        tree.tree_shape(&schema()),
        TreeShapeReport::Tree { root: 0 }
    );

    let forest = items(&[0, 1, 2, 3])
        .with_edge(0, 1, link())
        .with_edge(2, 3, link());
    assert_eq!(
        forest.tree_shape(&schema()),
        TreeShapeReport::Forest { roots: vec![0, 2] }
    );

    let single = items(&[5]);
    assert_eq!(
        single.tree_shape(&schema()),
        TreeShapeReport::Tree { root: 5 }
    );

    let cyclic = items(&[0, 1])
        .with_edge(0, 1, link())
        .with_edge(1, 0, link());
    assert!(matches!(
        cyclic.tree_shape(&schema()),
        TreeShapeReport::NotTree {
            reason: NotTreeReason::Cycle { .. }
        }
    ));

    // A tree with a reified fan is still a tree.
    let with_fan =
        reify_fans(&tree, &[Fan::new("rel", 0).with_child("a", 2)], None).expect("the fan reifies");
    assert_eq!(
        with_fan.tree_shape(&schema()),
        TreeShapeReport::Tree { root: 0 }
    );
}

#[test]
fn each_not_tree_reason_is_reported() {
    // Cycle: 1 -> 2 -> 3 -> 1 hangs off nothing, beside the root 0.
    let cycle = items(&[0, 1, 2, 3])
        .with_edge_id(EdgeId(9), 1, 2, link())
        .with_edge_id(EdgeId(4), 2, 3, link())
        .with_edge_id(EdgeId(6), 3, 1, link());
    assert_eq!(
        not_tree(&cycle),
        NotTreeReason::Cycle {
            witness: vec![EdgeId(4), EdgeId(6), EdgeId(9)]
        }
    );

    let two_parents = items(&[0, 1, 2])
        .with_edge(0, 2, link())
        .with_edge(1, 2, link());
    assert_eq!(
        not_tree(&two_parents),
        NotTreeReason::MultipleParents {
            node: 2,
            edges: vec![EdgeId(0), EdgeId(1)]
        }
    );

    let parallel = items(&[0, 1])
        .with_edge(0, 1, link())
        .with_edge(0, 1, link());
    assert_eq!(
        not_tree(&parallel),
        NotTreeReason::ParallelEdges { src: 0, tgt: 1 }
    );

    assert_eq!(not_tree(&GInstance::new()), NotTreeReason::NoRoot);

    let malformed_fan = items(&[0])
        .with_node(Node::new(1, "rel").with_shape(NodeShape::HyperEdge))
        .with_edge(1, 0, link());
    assert_eq!(
        not_tree(&malformed_fan),
        NotTreeReason::FanNotRepresentable { node: 1 }
    );

    let mut positioned = items(&[0, 1]).with_edge(0, 1, link());
    positioned
        .set_edge_position(EdgeId(0), Some(0))
        .expect("the edge exists");
    assert_eq!(
        not_tree(&positioned),
        NotTreeReason::PositionedEdge { edge: EdgeId(0) }
    );
}

#[test]
fn not_tree_reasons_serialize_externally_tagged() {
    let reasons = [
        NotTreeReason::Cycle {
            witness: vec![EdgeId(1)],
        },
        NotTreeReason::MultipleParents {
            node: 2,
            edges: vec![EdgeId(0), EdgeId(1)],
        },
        NotTreeReason::ParallelEdges { src: 0, tgt: 1 },
        NotTreeReason::NoRoot,
        NotTreeReason::FanNotRepresentable { node: 3 },
        NotTreeReason::PositionedEdge { edge: EdgeId(4) },
    ];
    let json: Vec<String> = reasons
        .iter()
        .map(|r| serde_json::to_string(r).expect("encodes"))
        .collect();
    assert_eq!(
        json,
        [
            r#"{"cycle":{"witness":[1]}}"#,
            r#"{"multiple_parents":{"node":2,"edges":[0,1]}}"#,
            r#"{"parallel_edges":{"src":0,"tgt":1}}"#,
            r#""no_root""#,
            r#"{"fan_not_representable":{"node":3}}"#,
            r#"{"positioned_edge":{"edge":4}}"#,
        ]
    );
    for reason in reasons {
        let json = serde_json::to_string(&reason).expect("encodes");
        assert_eq!(
            serde_json::from_str::<NotTreeReason>(&json).expect("decodes"),
            reason
        );
    }
    let report = TreeShapeReport::Forest { roots: vec![1, 0] };
    assert_eq!(
        serde_json::to_string(&report).expect("encodes"),
        r#"{"forest":{"roots":[1,0]}}"#
    );
}

#[test]
fn tree_shape_prefers_the_schema_root() {
    // Two trees; the one anchored at the entry vertex is listed first, and a
    // tree read out takes the first entry vertex as its schema root.
    let forest = GInstance::new()
        .with_node(Node::new(0, "note"))
        .with_node(Node::new(1, "doc"))
        .with_node(Node::new(2, "para"))
        .with_edge(1, 2, link());
    assert_eq!(
        forest.tree_shape(&entry_schema(&["doc"])),
        TreeShapeReport::Forest { roots: vec![1, 0] }
    );
    assert_eq!(
        forest.tree_shape(&schema()),
        TreeShapeReport::Forest { roots: vec![0, 1] }
    );
    let trees = forest.to_wtypes(&entry_schema(&["doc"])).expect("a forest");
    assert_eq!(trees.len(), 2);
    assert_eq!((trees[0].root, trees[1].root), (1, 0));
    assert_eq!(trees[0].arcs.len(), 1);
    assert_eq!(trees[0].schema_root, "doc");

    let tree = GInstance::new()
        .with_node(Node::new(0, "doc"))
        .with_node(Node::new(1, "para"))
        .with_edge(0, 1, link());
    let with_entry = tree
        .try_to_wtype(&entry_schema(&["entry"]))
        .expect("a tree");
    assert_eq!(with_entry.schema_root, "entry");
    let without = tree.try_to_wtype(&schema()).expect("a tree");
    assert_eq!(
        without.schema_root, "doc",
        "the root's anchor without entries"
    );
}

#[test]
fn try_to_wtype_carries_the_not_tree_reason() {
    let two_parents = items(&[0, 1, 2])
        .with_edge(0, 2, link())
        .with_edge(1, 2, link());
    assert!(matches!(
        two_parents.try_to_wtype(&schema()),
        Err(InstError::NotATree {
            reason: NotTreeReason::MultipleParents { node: 2, .. }
        })
    ));
    let forest = items(&[0, 1]);
    assert!(matches!(
        forest.try_to_wtype(&schema()),
        Err(InstError::NotATree {
            reason: NotTreeReason::NoRoot
        })
    ));
    assert_eq!(forest.to_wtypes(&schema()).expect("a forest").len(), 2);
    assert!(matches!(
        two_parents.to_wtypes(&schema()),
        Err(InstError::NotATree { .. })
    ));
    let err = two_parents.try_to_wtype(&schema()).expect_err("not a tree");
    assert_eq!(err.to_string(), "not a tree: node 2 has 2 parents");
}

#[test]
fn to_wtype_reports_a_malformed_fan_as_not_representable() {
    // Node 1 has the fan shape and two parent edges.
    let parent = Edge {
        src: "rel".into(),
        tgt: "item".into(),
        kind: HYPER_PARENT_KIND.into(),
        name: None,
    };
    let graph = items(&[0])
        .with_node(Node::new(1, "rel").with_shape(NodeShape::HyperEdge))
        .with_edge(1, 0, parent.clone())
        .with_edge(1, 0, parent);
    for result in [graph.to_wtype(0), graph.try_to_wtype(&schema())] {
        assert!(matches!(
            result,
            Err(InstError::NotATree {
                reason: NotTreeReason::FanNotRepresentable { node: 1 }
            })
        ));
    }
    // A well-formed fan node as the requested root cannot be a tree node.
    let fan_root = reify_fans(&items(&[0]), &[Fan::new("rel", 0)], None).expect("reifies");
    assert!(matches!(
        fan_root.to_wtype(1),
        Err(InstError::NotATree {
            reason: NotTreeReason::FanNotRepresentable { node: 1 }
        })
    ));
}

#[test]
fn a_fan_a_tree_does_not_write_is_not_representable_under_a_schema() {
    let base = items(&[0, 1]).with_edge(0, 1, link());
    let fan = Fan::new("rel", 0).with_child("a", 1);
    let graph = reify_fans(&base, std::slice::from_ref(&fan), None).expect("reifies");
    assert_eq!(
        graph.tree_shape(&schema()),
        TreeShapeReport::Tree { root: 0 }
    );

    // The schema names the parent edge `head`; the graph's parent edge is
    // unnamed, so its tree form would not lay out to this graph.
    let mut named = schema();
    named.hyper_edges.insert(
        "rel".into(),
        HyperEdge {
            id: "rel".into(),
            kind: "relation".into(),
            signature: HashMap::from([("head".into(), "item".into()), ("a".into(), "item".into())]),
            parent_label: "head".into(),
        },
    );
    assert_eq!(
        graph.tree_shape(&named),
        TreeShapeReport::NotTree {
            reason: NotTreeReason::FanNotRepresentable { node: 2 }
        }
    );
    let under_named = reify_fans(&base, &[fan], Some(&named)).expect("reifies");
    assert_eq!(
        under_named.tree_shape(&named),
        TreeShapeReport::Tree { root: 0 }
    );

    // A fan node carrying a value is not a fan a tree can carry.
    let mut valued = graph;
    valued
        .node_mut(2)
        .expect("the fan node")
        .extra_fields
        .insert("x".into(), panproto_inst::Value::Null);
    assert_eq!(
        valued.tree_shape(&schema()),
        TreeShapeReport::NotTree {
            reason: NotTreeReason::FanNotRepresentable { node: 2 }
        }
    );

    // A fan spanning two trees of a forest cannot be put in either.
    let spanning = reify_fans(
        &items(&[0, 1]),
        &[Fan::new("rel", 0).with_child("a", 1)],
        None,
    )
    .expect("reifies");
    assert_eq!(
        spanning.tree_shape(&schema()),
        TreeShapeReport::NotTree {
            reason: NotTreeReason::FanNotRepresentable { node: 2 }
        }
    );
}

/// A tree-shaped graph built by hand, with node ids, edge ids and fan ids
/// chosen freely rather than as `from_wtype` lays them out, and sometimes one
/// extra edge that may break the tree shape.
fn arb_graph() -> impl Strategy<Value = GInstance> {
    (1usize..8)
        .prop_flat_map(|n| {
            (
                Just(n),
                prop::collection::btree_set(0u32..60, n + 4..=n + 4),
                prop::collection::btree_set(0u32..90, 3 * n + 12..=3 * n + 12),
                (1..n.max(2)).map(|i| 0..i.max(1)).collect::<Vec<_>>(),
                prop::collection::vec(
                    (0..n, prop::collection::btree_map("[a-c]", 0..n, 0..3)),
                    0..3,
                ),
                prop::option::of((0..n, 0..n)),
            )
        })
        .prop_flat_map(|(n, node_ids, edge_ids, parents, fans, extra)| {
            let node_ids: Vec<u32> = node_ids.into_iter().collect();
            let edge_ids: Vec<u32> = edge_ids.into_iter().collect();
            (
                Just((n, node_ids.clone(), parents, fans, extra)),
                Just(node_ids).prop_shuffle(),
                Just(edge_ids).prop_shuffle(),
            )
        })
        .prop_map(|((n, _, parents, fans, extra), node_ids, edge_ids)| {
            let mut edges = edge_ids.into_iter().map(EdgeId);
            let mut graph = GInstance::new();
            for (i, &id) in node_ids.iter().take(n).enumerate() {
                graph = graph.with_node(Node::new(id, if i == 0 { "doc" } else { "item" }));
            }
            for (child, &parent) in (1..n).zip(&parents) {
                let (p, c) = (node_ids[parent], node_ids[child]);
                graph
                    .insert_edge(edges.next().expect("enough ids"), GEdge::new(p, c, link()))
                    .expect("both ends exist");
            }
            for (k, (parent, children)) in fans.into_iter().enumerate() {
                let fan_node = node_ids[n + k];
                graph =
                    graph.with_node(Node::new(fan_node, "rel").with_shape(NodeShape::HyperEdge));
                let anchor = |id: u32| graph.node(id).expect("a node").anchor.clone();
                let mut fan_edges = vec![(
                    node_ids[parent],
                    Edge {
                        src: "rel".into(),
                        tgt: anchor(node_ids[parent]),
                        kind: HYPER_PARENT_KIND.into(),
                        name: None,
                    },
                )];
                for (label, child) in children {
                    fan_edges.push((
                        node_ids[child],
                        Edge {
                            src: "rel".into(),
                            tgt: anchor(node_ids[child]),
                            kind: HYPER_LABEL_KIND.into(),
                            name: Some(Name::from(label.as_str())),
                        },
                    ));
                }
                for (target, label) in fan_edges {
                    graph
                        .insert_edge(
                            edges.next().expect("enough ids"),
                            GEdge::new(fan_node, target, label),
                        )
                        .expect("both ends exist");
                }
            }
            if let Some((src, tgt)) = extra {
                graph
                    .insert_edge(
                        edges.next().expect("enough ids"),
                        GEdge::new(node_ids[src], node_ids[tgt], link()),
                    )
                    .expect("both ends exist");
            }
            graph
        })
}

/// `graph` with its fan node ids and every edge id renamed from one
/// embedding's ids to another's.
fn rename(graph: &GInstance, from: &TreeEmbedding, to: &TreeEmbedding) -> GInstance {
    let mut nodes: BTreeMap<u32, u32> = BTreeMap::new();
    let mut edges: BTreeMap<EdgeId, EdgeId> = BTreeMap::new();
    for (child, id) in &from.arcs {
        edges.insert(*id, to.arcs[child]);
    }
    for (a, b) in from.fans.iter().zip(&to.fans) {
        nodes.insert(a.node, b.node);
        edges.insert(a.parent_edge, b.parent_edge);
        for (label, id) in &a.child_edges {
            edges.insert(*id, b.child_edges[label]);
        }
    }
    let mut renamed = GInstance::new();
    for node in graph.nodes().values() {
        let mut node = node.clone();
        node.id = nodes.get(&node.id).copied().unwrap_or(node.id);
        renamed = renamed.with_node(node);
    }
    for (id, edge) in graph.edges_sorted() {
        let mut edge = edge.clone();
        edge.src = nodes.get(&edge.src).copied().unwrap_or(edge.src);
        edge.tgt = nodes.get(&edge.tgt).copied().unwrap_or(edge.tgt);
        renamed
            .insert_edge(edges[&id], edge)
            .expect("a renaming is a bijection");
    }
    renamed
}

proptest! {
    #[test]
    fn a_tree_shaped_graph_round_trips_through_its_tree(graph in arb_graph()) {
        let schema = entry_schema(&["doc"]);
        let report = graph.tree_shape(&schema);
        match report {
            TreeShapeReport::Tree { root } => {
                let (tree, read) = graph.to_wtype_embedded(&schema).expect("a tree");
                prop_assert_eq!(tree.root, root);
                let again = graph.try_to_wtype(&schema).expect("a tree");
                prop_assert_eq!(&again.arcs, &tree.arcs);
                let (laid_out, written) = GInstance::from_wtype_embedded(&tree, Some(&schema))
                    .expect("a tree read out of a graph embeds");
                let mapped = rename(&laid_out, &written, &read);
                prop_assert!(GInstance::identical(&mapped, &graph));
            }
            TreeShapeReport::Forest { roots } => {
                let trees = graph.to_wtypes(&schema).expect("a forest");
                prop_assert_eq!(trees.len(), roots.len());
                let nodes: usize = trees.iter().map(|t| t.nodes.len()).sum();
                let fans: usize = trees.iter().map(|t| t.fans.len()).sum();
                prop_assert_eq!(nodes + fans, graph.node_count());
            }
            TreeShapeReport::NotTree { reason } => {
                let is_not_a_tree = matches!(
                    graph.try_to_wtype(&schema),
                    Err(InstError::NotATree { reason: r }) if r == reason
                );
                prop_assert!(is_not_a_tree);
            }
        }
    }
}
