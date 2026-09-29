//! A tree sits inside its graph form through a recorded embedding.
//!
//! `GInstance::from_wtype_embedded` lays a tree out as a graph and records
//! which edge each arc became and which node and edges each fan became;
//! `GInstance::to_wtype_embedded` reads a tree-shaped graph back with the same
//! record. The embedding's counters are the floors from which tree edits
//! allocate, and they follow every edit so that ascending edge id stays arc
//! order.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, HashMap};

use panproto_gat::Name;
use panproto_inst::{
    EdgeId, Fan, FieldPresence, GInstance, InstError, Node, NodeShape, RowId, TreeEdit,
    TreeEmbedding, TreeShapeReport, Value, ViewRef, WInstance,
};
use panproto_schema::{Edge, Schema};
use proptest::prelude::*;

/// A schema whose only declaration is the entry vertex `root`.
fn entry_schema(root: &Name) -> Schema {
    Schema {
        entries: vec![root.clone()],
        ..Schema::default()
    }
}

/// The bytes of a tree's positional `MessagePack` encoding, which writes
/// every map in key order and so compares two trees field for field.
fn tree_bytes(tree: &WInstance) -> Vec<u8> {
    rmp_serde::to_vec(tree).expect("a tree encodes")
}

fn arb_value() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<i64>().prop_map(Value::Int),
        "[a-z]{0,4}".prop_map(Value::Str),
        Just(Value::Null),
    ]
}

/// A tree node of any shape a tree may hold: every shape but the reserved
/// fan shape.
fn arb_node(id: u32) -> impl Strategy<Value = Node> {
    (
        prop::sample::select(vec!["doc", "para", "rel"]),
        prop::option::of(arb_value()),
        prop::collection::hash_map("[a-z]{1,3}", arb_value(), 0..2),
        prop::option::of(0u32..4),
        prop::sample::select(vec![
            NodeShape::Plain,
            NodeShape::List,
            NodeShape::XmlTextSegment,
            NodeShape::XmlElement { tag: "p".into() },
        ]),
    )
        .prop_map(move |(anchor, value, extra, position, shape)| {
            let mut node = Node::new(id, anchor).with_shape(shape);
            node.value = value.map(FieldPresence::Present);
            node.extra_fields = extra;
            node.position = position;
            node
        })
}

/// A valid tree over nodes whose ids are spread out (so that fan node ids
/// can fall between them after an edit), arcs in a shuffled order, and fans
/// over its nodes, some repeated. Some nodes look like fans: they have
/// several labelled children of the same kind, with ordinary kinds.
fn arb_tree() -> impl Strategy<Value = WInstance> {
    (1usize..9)
        .prop_flat_map(|n| {
            let ids = prop::collection::btree_set(0u32..40, n..=n);
            (Just(n), ids)
        })
        .prop_flat_map(|(n, ids)| {
            let ids: Vec<u32> = ids.into_iter().collect();
            let nodes: Vec<_> = ids.iter().map(|&id| arb_node(id)).collect();
            let parents: Vec<_> = (1..n).map(|i| 0..i).collect();
            let fans = prop::collection::vec(
                (
                    prop::sample::select(vec!["rel", "pair"]),
                    0..n,
                    prop::collection::hash_map("[a-c]", 0..n, 0..3),
                ),
                0..4,
            );
            (Just(ids), nodes, parents, fans, any::<bool>(), "[a-z]{1,4}")
        })
        .prop_flat_map(|(ids, nodes, parents, fans, repeat, schema_root)| {
            let arcs: Vec<(u32, u32, Edge)> = parents
                .iter()
                .zip(1usize..)
                .map(|(&p, c)| {
                    let edge = Edge {
                        src: nodes[p].anchor.clone(),
                        tgt: nodes[c].anchor.clone(),
                        kind: "hyper".into(),
                        name: Some(Name::from(format!("arg{}", c % 3).as_str())),
                    };
                    (ids[p], ids[c], edge)
                })
                .collect();
            let mut fans: Vec<Fan> = fans
                .into_iter()
                .map(|(he, parent, children)| Fan {
                    hyper_edge_id: he.to_owned(),
                    parent: ids[parent],
                    children: children.into_iter().map(|(l, c)| (l, ids[c])).collect(),
                })
                .collect();
            if repeat && let Some(first) = fans.first().cloned() {
                fans.push(first);
            }
            let root = ids[0];
            (
                Just(nodes),
                Just(arcs).prop_shuffle(),
                Just(fans),
                Just(root),
                Just(schema_root),
            )
        })
        .prop_map(|(nodes, arcs, fans, root, schema_root)| {
            let nodes: HashMap<u32, Node> = nodes.into_iter().map(|n| (n.id, n)).collect();
            WInstance::new(nodes, arcs, fans, root, Name::from(schema_root.as_str()))
        })
}

/// One edit of the kinds tree edits come in, over node ids below 48 so that
/// edits hit existing nodes, fresh ids and fan node ids alike.
fn arb_edit() -> impl Strategy<Value = TreeEdit> {
    let edge = || Edge {
        src: "doc".into(),
        tgt: "para".into(),
        kind: "prop".into(),
        name: Some("c".into()),
    };
    prop_oneof![
        (0u32..48, 0u32..48).prop_map(move |(parent, child_id)| TreeEdit::InsertNode {
            parent,
            child_id,
            node: Node::new(child_id, "para"),
            edge: edge(),
        }),
        (0u32..48).prop_map(|id| TreeEdit::DeleteNode { id }),
        (0u32..48).prop_map(|id| TreeEdit::ContractNode { id }),
        (0u32..48, 0u32..48).prop_map(move |(node_id, new_parent)| TreeEdit::MoveSubtree {
            node_id,
            new_parent,
            edge: edge(),
        }),
        (
            0u32..48,
            prop::collection::hash_map("[a-b]", 0u32..48, 0..2)
        )
            .prop_map(|(parent, children)| TreeEdit::InsertFan {
                fan: Fan {
                    hyper_edge_id: "rel".into(),
                    parent,
                    children,
                },
            }),
        Just(TreeEdit::DeleteFan {
            hyper_edge_id: "rel".into()
        }),
        (0u32..48).prop_map(|node_id| TreeEdit::SetField {
            node_id,
            field: "f".into(),
            value: Value::Int(1),
        }),
    ]
}

/// The multiset of a tree's fans, for comparing fan lists whose order a
/// relocation may have changed.
fn fan_multiset(fans: &[Fan]) -> Vec<String> {
    let mut keys: Vec<String> = fans
        .iter()
        .map(|f| {
            let children: BTreeMap<&String, &u32> = f.children.iter().collect();
            format!("{}:{}:{children:?}", f.hyper_edge_id, f.parent)
        })
        .collect();
    keys.sort();
    keys
}

/// Whether a tree is well formed enough to have a graph form that reads
/// back: its root is a node without a parent, every other node has one
/// parent and hangs from the root, and its fans name only its nodes.
fn is_well_formed(tree: &WInstance) -> bool {
    let mut parent: HashMap<u32, u32> = HashMap::new();
    for &(p, c, _) in &tree.arcs {
        if !tree.nodes.contains_key(&p) || parent.insert(c, p).is_some() {
            return false;
        }
    }
    let reaches_root = |mut n: u32| {
        for _ in 0..=tree.nodes.len() {
            if n == tree.root {
                return true;
            }
            match parent.get(&n) {
                Some(&p) => n = p,
                None => return false,
            }
        }
        false
    };
    tree.nodes.contains_key(&tree.root)
        && !parent.contains_key(&tree.root)
        && tree.nodes.keys().all(|&n| reaches_root(n))
        && tree.fans.iter().all(|f| {
            tree.nodes.contains_key(&f.parent)
                && f.children.values().all(|c| tree.nodes.contains_key(c))
        })
}

/// Check that `embedding` embeds `tree`: arcs in ascending edge id, one
/// recorded edge per arc, counters above every id of the graph form, and,
/// for a well-formed tree, a graph form that reads back as the tree.
fn assert_embeds(tree: &WInstance, embedding: &TreeEmbedding) -> Result<(), TestCaseError> {
    let ids: Vec<EdgeId> = tree
        .arcs
        .iter()
        .map(|(_, c, _)| embedding.arcs.get(c).copied())
        .collect::<Option<_>>()
        .ok_or_else(|| TestCaseError::fail("an arc has no recorded edge"))?;
    prop_assert!(
        ids.windows(2).all(|w| w[0] < w[1]),
        "arc order is edge id order: {ids:?}"
    );
    prop_assert_eq!(embedding.arcs.len(), tree.arcs.len());
    prop_assert_eq!(embedding.fans.len(), tree.fans.len());

    let under = GInstance::from_wtype_under(tree, embedding, None);
    let fresh = GInstance::from_wtype_embedded(tree, None);
    prop_assert_eq!(
        under.is_ok(),
        fresh.is_ok(),
        "{:?} vs {:?}",
        under.as_ref().err(),
        fresh.as_ref().err()
    );
    let Ok(graph) = under else {
        return Ok(());
    };
    prop_assert!(graph.max_node_id().is_none_or(|m| m < embedding.next_node));
    prop_assert!(
        graph
            .max_edge_id()
            .is_none_or(|EdgeId(m)| m < embedding.next_edge)
    );
    if is_well_formed(tree) {
        let schema = entry_schema(&tree.schema_root);
        let (back, read) = graph
            .to_wtype_embedded(&schema)
            .expect("the graph form is a tree");
        prop_assert_eq!(&back.arcs, &tree.arcs);
        prop_assert_eq!(&back.root, &tree.root);
        prop_assert_eq!(fan_multiset(&back.fans), fan_multiset(&tree.fans));
        prop_assert_eq!(&read.arcs, &embedding.arcs);
    }
    Ok(())
}

proptest! {
    #[test]
    fn every_valid_tree_embeds_and_round_trips(tree in arb_tree()) {
        let (graph, embedding) = GInstance::from_wtype_embedded(&tree, None)
            .expect("a valid tree always embeds");
        let schema = entry_schema(&tree.schema_root);
        prop_assert_eq!(graph.tree_shape(&schema), TreeShapeReport::Tree { root: tree.root });
        let (back, read) = graph.to_wtype_embedded(&schema).expect("the graph form is a tree");
        prop_assert_eq!(tree_bytes(&back), tree_bytes(&tree));
        prop_assert_eq!(read, embedding);
    }

    #[test]
    fn a_tree_round_trips_through_its_embedded_graph(tree in arb_tree()) {
        let schema = entry_schema(&tree.schema_root);
        let (graph, embedding) = GInstance::from_wtype_embedded(&tree, Some(&schema))
            .expect("a valid tree always embeds");
        let (back, read) = graph.to_wtype_embedded(&schema).expect("the graph form is a tree");
        prop_assert_eq!(tree_bytes(&back), tree_bytes(&tree));
        prop_assert_eq!(&read, &embedding);
        let again = GInstance::from_wtype_under(&back, &read, Some(&schema)).expect("re-embeds");
        prop_assert!(GInstance::identical(&again, &graph));
    }

    #[test]
    fn a_tree_named_by_its_root_round_trips_under_a_schema_without_entries(tree in arb_tree()) {
        let mut tree = tree;
        tree.schema_root = tree.nodes[&tree.root].anchor.clone();
        let schema = Schema::default();
        let (graph, embedding) = GInstance::from_wtype_embedded(&tree, Some(&schema))
            .expect("a valid tree always embeds");
        let (back, read) = graph.to_wtype_embedded(&schema).expect("the graph form is a tree");
        prop_assert_eq!(tree_bytes(&back), tree_bytes(&tree));
        prop_assert_eq!(&read, &embedding);
    }

    #[test]
    fn from_wtype_is_the_first_projection_of_from_wtype_embedded(tree in arb_tree()) {
        let plain = GInstance::from_wtype(&tree).expect("a valid tree embeds");
        let (embedded, _) = GInstance::from_wtype_embedded(&tree, None).expect("a valid tree embeds");
        prop_assert!(GInstance::identical(&plain, &embedded));
    }

    #[test]
    fn the_tree_floor_equals_the_embedded_graph_floor(tree in arb_tree()) {
        let (graph, embedding) = GInstance::from_wtype_embedded(&tree, None).expect("embeds");
        let fan_nodes = embedding.fans.iter().map(|f| f.node);
        let floor = tree.nodes.keys().copied().chain(fan_nodes).max().map_or(0, |m| m + 1);
        prop_assert_eq!(embedding.next_node, floor);
        let graph_floor = graph.max_node_id().map_or(0, |m| m + 1);
        prop_assert_eq!(embedding.next_node, graph_floor);
        let edge_floor = graph.max_edge_id().map_or(0, |EdgeId(m)| m + 1);
        prop_assert_eq!(embedding.next_edge, edge_floor);
    }

    #[test]
    fn tree_embedding_counters_never_decrease(
        tree in arb_tree(),
        edits in prop::collection::vec(arb_edit(), 0..12),
    ) {
        let (_, mut embedding) = GInstance::from_wtype_embedded(&tree, None).expect("embeds");
        let mut tree = tree;
        for edit in &edits {
            let (node, edge) = (embedding.next_node, embedding.next_edge);
            let _ = embedding.apply_edit(&mut tree, edit);
            prop_assert!(embedding.next_node >= node && embedding.next_edge >= edge);
        }
    }

    #[test]
    fn tree_edits_keep_edge_id_order_equal_to_arc_order(
        tree in arb_tree(),
        edits in prop::collection::vec(arb_edit(), 0..12),
    ) {
        let (_, mut embedding) = GInstance::from_wtype_embedded(&tree, None).expect("embeds");
        let mut tree = tree;
        assert_embeds(&tree, &embedding)?;
        for edit in &edits {
            let before = tree_bytes(&tree);
            let snapshot = embedding.clone();
            if embedding.apply_edit(&mut tree, edit).is_err() {
                prop_assert_eq!(tree_bytes(&tree), before, "a failing step leaves the tree");
                prop_assert_eq!(&embedding, &snapshot, "and the embedding");
            }
            assert_embeds(&tree, &embedding)?;
        }
        // A whole sequence follows the same way as its steps one by one.
        prop_assert!(embedding.next_edge as usize >= tree.arcs.len());
    }
}

fn label(name: &str) -> Edge {
    Edge {
        src: "sent".into(),
        tgt: "word".into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

/// A sentence with two words and one `arg` relation over them.
fn sentence() -> WInstance {
    let nodes = HashMap::from([
        (0, Node::new(0, "sent")),
        (1, Node::new(1, "word")),
        (4, Node::new(4, "word")),
    ]);
    let arcs = vec![(0, 4, label("w")), (0, 1, label("w"))];
    let fan = Fan::new("arg", 0)
        .with_child("theme", 4)
        .with_child("agent", 1);
    WInstance::new(nodes, arcs, vec![fan], 0, "sent".into())
}

#[test]
fn tree_map_ref_sends_arcs_to_their_recorded_edges() {
    let (_, embedding) = GInstance::from_wtype_embedded(&sentence(), None).expect("embeds");
    assert_eq!(embedding.map_ref(&ViewRef::Node(4)), Some(ViewRef::Node(4)));
    assert_eq!(
        embedding.map_ref(&ViewRef::Arc {
            parent: 0,
            child: 4
        }),
        Some(ViewRef::Edge(EdgeId(0)))
    );
    assert_eq!(
        embedding.map_ref(&ViewRef::Arc {
            parent: 0,
            child: 1
        }),
        Some(ViewRef::Edge(EdgeId(1)))
    );
    assert_eq!(
        embedding.map_ref(&ViewRef::Arc {
            parent: 0,
            child: 9
        }),
        None,
        "an arc the embedding does not record has no image"
    );
    assert_eq!(
        embedding.map_ref(&ViewRef::Edge(EdgeId(3))),
        Some(ViewRef::Edge(EdgeId(3)))
    );
    assert_eq!(embedding.map_ref(&ViewRef::Row(RowId(0))), None);
}

#[test]
fn from_wtype_embedded_records_the_documented_layout() {
    let (graph, embedding) = GInstance::from_wtype_embedded(&sentence(), None).expect("embeds");
    assert_eq!(
        embedding.arcs,
        BTreeMap::from([(4, EdgeId(0)), (1, EdgeId(1))])
    );
    assert_eq!(embedding.fans.len(), 1);
    let fan = &embedding.fans[0];
    assert_eq!(fan.node, 5, "max node id + 1 + 0");
    assert_eq!(fan.parent_edge, EdgeId(2), "after every arc");
    assert_eq!(
        fan.child_edges,
        BTreeMap::from([
            ("agent".to_owned(), EdgeId(3)),
            ("theme".to_owned(), EdgeId(4))
        ])
    );
    assert_eq!((embedding.next_node, embedding.next_edge), (6, 5));
    assert_eq!(graph.edge(EdgeId(3)).map(|e| e.tgt), Some(1));
}

#[test]
fn tree_embedding_serde_round_trips() {
    let (_, embedding) = GInstance::from_wtype_embedded(&sentence(), None).expect("embeds");
    let json = serde_json::to_string(&embedding).expect("encodes");
    assert_eq!(
        json,
        r#"{"arcs":{"1":1,"4":0},"fans":[{"node":5,"parent_edge":2,"child_edges":{"agent":3,"theme":4}}],"next_node":6,"next_edge":5}"#
    );
    assert_eq!(
        serde_json::from_str::<TreeEmbedding>(&json).expect("decodes"),
        embedding
    );
    let value = serde_json::to_value(&embedding).expect("encodes");
    assert_eq!(
        serde_json::from_value::<TreeEmbedding>(value).expect("decodes"),
        embedding
    );
    let named = rmp_serde::to_vec_named(&embedding).expect("encodes");
    assert_eq!(
        rmp_serde::from_slice::<TreeEmbedding>(&named).expect("decodes"),
        embedding
    );
    let packed = rmp_serde::to_vec(&embedding).expect("encodes");
    assert_eq!(
        rmp_serde::from_slice::<TreeEmbedding>(&packed).expect("decodes"),
        embedding
    );
    let mut cbor = Vec::new();
    ciborium::into_writer(&embedding, &mut cbor).expect("encodes");
    assert_eq!(
        ciborium::from_reader::<TreeEmbedding, _>(cbor.as_slice()).expect("decodes"),
        embedding
    );
}

#[test]
fn to_wtype_embedded_emits_children_in_ordered_out_edge_order() {
    // Children of node 0 on edges 7, 2 and 5 (inserted in that order); the
    // tree lists them in ascending edge id, which is `ordered_out_edges`.
    let graph = GInstance::new()
        .with_node(Node::new(0, "sent"))
        .with_node(Node::new(1, "word"))
        .with_node(Node::new(2, "word"))
        .with_node(Node::new(3, "word"))
        .with_edge_id(EdgeId(7), 0, 1, label("a"))
        .with_edge_id(EdgeId(2), 0, 2, label("b"))
        .with_edge_id(EdgeId(5), 0, 3, label("c"));
    let (tree, embedding) = graph
        .to_wtype_embedded(&entry_schema(&"sent".into()))
        .expect("a tree");
    let children: Vec<u32> = tree.arcs.iter().map(|(_, c, _)| *c).collect();
    let ordered: Vec<u32> = graph
        .ordered_out_edges(0)
        .iter()
        .map(|e| graph.edge(*e).expect("an edge").tgt)
        .collect();
    assert_eq!(children, ordered);
    assert_eq!(children, vec![2, 3, 1]);
    assert_eq!(embedding.arcs[&1], EdgeId(7));
    assert_eq!((embedding.next_node, embedding.next_edge), (4, 8));
}

#[test]
fn a_tree_edit_that_takes_a_fan_node_id_relocates_the_fan() {
    // Node 5 is the fan node of `sentence`; inserting a tree node under id 5
    // moves the fan node to the next free node id, 6, and keeps its edges.
    let mut tree = sentence();
    let (_, mut embedding) = GInstance::from_wtype_embedded(&tree, None).expect("embeds");
    let fan_edges = embedding.fans[0].clone();
    embedding
        .apply_edit(
            &mut tree,
            &TreeEdit::InsertNode {
                parent: 0,
                child_id: 5,
                node: Node::new(5, "word"),
                edge: label("w"),
            },
        )
        .expect("the edit applies");
    assert_eq!(embedding.fans[0].node, 6);
    assert_eq!(embedding.fans[0].parent_edge, fan_edges.parent_edge);
    assert_eq!(embedding.fans[0].child_edges, fan_edges.child_edges);
    assert_eq!(embedding.next_node, 7);
    assert_eq!(embedding.arcs[&5], EdgeId(5), "the new arc takes next_edge");
    assert_eq!(embedding.next_edge, 6);

    let graph = GInstance::from_wtype_under(&tree, &embedding, None).expect("re-embeds");
    assert_eq!(
        graph.node(6).map(|n| n.shape.clone()),
        Some(NodeShape::HyperEdge)
    );
    assert_eq!(graph.node(5).map(|n| n.anchor.clone()), Some("word".into()));
}

#[test]
fn a_moved_subtree_gets_a_fresh_edge_after_every_arc() {
    let mut tree = sentence();
    let (_, mut embedding) = GInstance::from_wtype_embedded(&tree, None).expect("embeds");
    embedding
        .apply_edit(
            &mut tree,
            &TreeEdit::MoveSubtree {
                node_id: 4,
                new_parent: 1,
                edge: label("w"),
            },
        )
        .expect("the edit applies");
    assert_eq!(tree.arcs.last().map(|(p, c, _)| (*p, *c)), Some((1, 4)));
    assert_eq!(embedding.arcs[&4], EdgeId(5), "re-parented, so fresh");
    assert_eq!(embedding.arcs[&1], EdgeId(1), "untouched, so kept");
}

#[test]
fn an_embedding_counter_that_would_pass_u32_max_is_exhaustion() {
    // A tree holding node u32::MAX - 1 puts its one fan at u32::MAX, after
    // which no node counter fits in a u32.
    let top = u32::MAX - 1;
    let tree = WInstance::new(
        HashMap::from([(top, Node::new(top, "sent"))]),
        vec![],
        vec![Fan::new("arg", top)],
        top,
        "sent".into(),
    );
    assert!(matches!(
        GInstance::from_wtype_embedded(&tree, None),
        Err(InstError::NodeIdsExhausted)
    ));
    // One id lower, the fan fits and the counter is u32::MAX.
    let lower = top - 1;
    let tree = WInstance::new(
        HashMap::from([(lower, Node::new(lower, "sent"))]),
        vec![],
        vec![Fan::new("arg", lower)],
        lower,
        "sent".into(),
    );
    let (_, embedding) = GInstance::from_wtype_embedded(&tree, None).expect("fits");
    assert_eq!(embedding.fans[0].node, top);
    assert_eq!(embedding.next_node, u32::MAX);
    let mut embedding = embedding;
    assert!(matches!(
        embedding.allocate_node(),
        Err(InstError::NodeIdsExhausted)
    ));
    assert_eq!(
        embedding.next_node,
        u32::MAX,
        "a failed allocation leaves the counter"
    );
}

#[test]
fn two_arcs_into_one_child_cannot_be_embedded() {
    let nodes = HashMap::from([
        (0, Node::new(0, "sent")),
        (1, Node::new(1, "word")),
        (2, Node::new(2, "word")),
    ]);
    let tree = WInstance::new(
        nodes,
        vec![(0, 2, label("w")), (1, 2, label("w"))],
        vec![],
        0,
        "sent".into(),
    );
    assert!(matches!(
        GInstance::from_wtype_embedded(&tree, None),
        Err(InstError::NotATree {
            reason: panproto_inst::NotTreeReason::MultipleParents { node: 2, .. }
        })
    ));
}

#[test]
fn a_sequence_of_every_edit_kind_keeps_the_embedding() {
    let mut tree = sentence();
    let (_, mut embedding) = GInstance::from_wtype_embedded(&tree, None).expect("embeds");
    let word = |id: u32| Node::new(id, "word");
    let list = vec![
        TreeEdit::InsertNode {
            parent: 4,
            child_id: 7,
            node: word(7),
            edge: label("w"),
        },
        TreeEdit::InsertNode {
            parent: 7,
            child_id: 8,
            node: word(8),
            edge: label("w"),
        },
        TreeEdit::InsertFan {
            fan: Fan::new("pair", 7).with_child("left", 8),
        },
        TreeEdit::ContractNode { id: 7 },
        TreeEdit::MoveSubtree {
            node_id: 1,
            new_parent: 8,
            edge: label("w"),
        },
        TreeEdit::DeleteFan {
            hyper_edge_id: "arg".into(),
        },
        TreeEdit::SetField {
            node_id: 8,
            field: "f".into(),
            value: Value::Int(2),
        },
    ];
    let mut whole_tree = tree.clone();
    let mut whole = embedding.clone();
    whole
        .apply_edit(&mut whole_tree, &TreeEdit::Sequence(list.clone()))
        .expect("the sequence applies");
    let mut steps = 0;
    for step in &list {
        embedding
            .apply_edit(&mut tree, step)
            .expect("each step applies");
        steps += 1;
        assert_embeds(&tree, &embedding).expect("the embedding follows the step");
    }
    assert_eq!(steps, 7);
    assert_eq!(whole, embedding, "a sequence follows as its steps do");
    assert_eq!(tree_bytes(&whole_tree), tree_bytes(&tree));
    let ids: Vec<EdgeId> = tree
        .arcs
        .iter()
        .map(|(_, c, _)| embedding.arcs[c])
        .collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "{ids:?}");
    assert!(is_well_formed(&tree));
}
