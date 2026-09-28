//! A tree, and every other form a graph instance takes, round-trips through
//! the graph instance without loss.
//!
//! `GInstance::from_wtype` lays a tree out as a graph, reifying each fan as a
//! node of its own, and `to_wtype` reads it back; `unreify_fans` and
//! `reify_fans` take fan nodes out and put them back; the node-link JSON form
//! and the serde encodings write a graph out and read it back. Each pair is
//! checked here to be an exact inverse over generated instances, duplicate
//! fans included.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::{
    EdgeId, FInstance, Fan, FieldPresence, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND,
    InstError, Node, NodeShape, Value, WInstance, from_node_link_json, reify_fans,
    to_node_link_json, unreify_fans,
};
use panproto_schema::{Edge, HyperEdge, Protocol, SchemaBuilder};
use proptest::prelude::*;

/// The bytes of a tree's positional `MessagePack` encoding, which writes
/// every map in key order and so compares two trees field for field.
fn tree_bytes(tree: &WInstance) -> Vec<u8> {
    rmp_serde::to_vec(tree).expect("a tree encodes")
}

fn arb_value() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<i64>().prop_map(Value::Int),
        "[a-z]{0,4}".prop_map(Value::Str),
        any::<bool>().prop_map(Value::Bool),
        Just(Value::Null),
    ]
}

fn arb_node(id: u32) -> impl Strategy<Value = Node> {
    (
        prop::sample::select(vec!["doc", "para", "word"]),
        prop::option::of(arb_value()),
        prop::collection::hash_map("[a-z]{1,3}", arb_value(), 0..3),
        prop::option::of(0u32..4),
    )
        .prop_map(move |(anchor, value, extra, position)| {
            let mut node = Node::new(id, anchor);
            node.value = value.map(FieldPresence::Present);
            node.extra_fields = extra;
            node.position = position;
            node
        })
}

/// A tree over nodes `0..n` (node `i > 0` hangs under some node `< i`), its
/// arcs in a shuffled order, with fans over its nodes, some fans repeated.
fn arb_tree() -> impl Strategy<Value = WInstance> {
    (1u32..10)
        .prop_flat_map(|n| {
            let nodes: Vec<_> = (0..n).map(arb_node).collect();
            let parents: Vec<_> = (1..n).map(|i| 0..i).collect();
            let fans = prop::collection::vec(
                (
                    prop::sample::select(vec!["rel", "pair"]),
                    0..n,
                    prop::collection::hash_map("[a-c]", 0..n, 0..3),
                ),
                0..4,
            );
            let schema_root = prop::option::of(prop::sample::select(vec!["doc", "entry"]));
            (nodes, parents, fans, any::<bool>(), schema_root)
        })
        .prop_flat_map(|(nodes, parents, fans, repeat, schema_root)| {
            let arcs: Vec<(u32, u32, Edge)> = parents
                .iter()
                .zip(1u32..)
                .map(|(&parent, child)| {
                    let edge = Edge {
                        src: nodes[parent as usize].anchor.clone(),
                        tgt: nodes[child as usize].anchor.clone(),
                        kind: "prop".into(),
                        name: Some(Name::from(format!("c{child}").as_str())),
                    };
                    (parent, child, edge)
                })
                .collect();
            let mut fans: Vec<Fan> = fans
                .into_iter()
                .map(|(he, parent, children)| Fan {
                    hyper_edge_id: he.to_owned(),
                    parent,
                    children,
                })
                .collect();
            if repeat && let Some(first) = fans.first().cloned() {
                fans.push(first);
            }
            (
                Just(nodes),
                Just(arcs).prop_shuffle(),
                Just(fans),
                Just(schema_root),
            )
        })
        .prop_map(|(nodes, arcs, fans, schema_root)| {
            // The schema root is usually the root's anchor, but a tree may
            // name any vertex there.
            let schema_root = schema_root.map_or_else(|| nodes[0].anchor.clone(), Name::from);
            let nodes: HashMap<u32, Node> = nodes.into_iter().map(|n| (n.id, n)).collect();
            WInstance::new(nodes, arcs, fans, 0, schema_root)
        })
}

proptest! {
    #[test]
    fn a_tree_round_trips_through_its_graph_form(tree in arb_tree()) {
        let graph = GInstance::from_wtype(&tree).expect("a tree has a graph form");
        let back = graph
            .to_wtype_with_schema_root(tree.root, tree.schema_root.clone())
            .expect("the graph form is a tree");
        prop_assert_eq!(tree_bytes(&back), tree_bytes(&tree));

        // Without the schema root, the root node's anchor stands in for it.
        let anchored = graph.to_wtype(tree.root).expect("the graph form is a tree");
        let root_anchor = tree.nodes[&tree.root].anchor.clone();
        prop_assert_eq!(&anchored.schema_root, &root_anchor);
        if tree.schema_root == root_anchor {
            prop_assert_eq!(tree_bytes(&anchored), tree_bytes(&tree));
        }
    }

    #[test]
    fn a_graph_form_never_reads_back_as_a_different_tree(
        tree in arb_tree(),
        hyper in any::<bool>(),
        out_arcs in prop::collection::vec((0u32..10, any::<bool>(), prop::option::of("[a-c]")), 0..4),
    ) {
        // An extra node with no incoming arc, possibly of the fan-node shape
        // and with out-arcs of the fan-node edge kinds, so that its graph form
        // may look like a reified fan.
        let mut odd = tree.clone();
        let n = u32::try_from(tree.nodes.len()).expect("small");
        let node = Node::new(n, "arg");
        let node = if hyper { node.with_shape(NodeShape::HyperEdge) } else { node };
        odd.nodes.insert(n, node);
        for (target, parent_kind, name) in out_arcs {
            let target = target % n;
            let kind = if parent_kind { HYPER_PARENT_KIND } else { HYPER_LABEL_KIND };
            odd.arcs.push((n, target, Edge {
                src: "arg".into(),
                tgt: odd.nodes[&target].anchor.clone(),
                kind: kind.into(),
                name: name.map(|s| Name::from(s.as_str())),
            }));
        }
        if let Ok(graph) = GInstance::from_wtype(&odd) {
            match graph.to_wtype_with_schema_root(odd.root, odd.schema_root.clone()) {
                Ok(back) => prop_assert_eq!(tree_bytes(&back), tree_bytes(&odd)),
                Err(error) => prop_assert!(
                    matches!(error, InstError::NotATree { .. }),
                    "unexpected error {error:?}"
                ),
            }
        }
    }

    #[test]
    fn unreify_inverts_reify_on_a_trees_graph_form(tree in arb_tree()) {
        let graph = GInstance::from_wtype(&tree).expect("a tree has a graph form");
        let (rest, fans) = unreify_fans(&graph);
        prop_assert_eq!(&fans, &tree.fans, "fans come back in order, duplicates kept");
        prop_assert_eq!(rest.node_count(), tree.nodes.len());
        prop_assert_eq!(rest.edge_count(), tree.arcs.len());
        let again = reify_fans(&rest, &fans, None).expect("the fans reify again");
        prop_assert!(GInstance::identical(&again, &graph));
    }

    #[test]
    fn a_graph_round_trips_through_its_encodings(tree in arb_tree(), extra in prop::collection::vec((0u32..10, 0u32..10, prop::option::of(0u32..3)), 0..6)) {
        let mut graph = GInstance::from_wtype(&tree).expect("a tree has a graph form");
        let n = u32::try_from(tree.nodes.len()).expect("small");
        for (src, tgt, position) in extra {
            let (src, tgt) = (src % n, tgt % n);
            let id = graph
                .add_edge(src, tgt, Edge { src: "x".into(), tgt: "y".into(), kind: "link".into(), name: None })
                .expect("both endpoints exist");
            graph.set_edge_position(id, position).expect("the edge exists");
        }

        let json = serde_json::to_vec(&graph).expect("encodes");
        let from_json: GInstance = serde_json::from_slice(&json).expect("decodes");
        prop_assert!(GInstance::identical(&from_json, &graph));

        let packed = rmp_serde::to_vec(&graph).expect("encodes");
        let from_packed: GInstance = rmp_serde::from_slice(&packed).expect("decodes");
        prop_assert!(GInstance::identical(&from_packed, &graph));

        let named = rmp_serde::to_vec_named(&graph).expect("encodes");
        let from_named: GInstance = rmp_serde::from_slice(&named).expect("decodes");
        prop_assert!(GInstance::identical(&from_named, &graph));

        let node_link = to_node_link_json(&graph);
        let from_node_link = from_node_link_json(&node_link).expect("decodes");
        prop_assert!(GInstance::identical(&from_node_link, &graph));
        for id in graph.nodes().keys() {
            prop_assert_eq!(from_node_link.out_edges(*id), graph.out_edges(*id));
            prop_assert_eq!(from_node_link.in_edges(*id), graph.in_edges(*id));
        }
    }
}

fn label(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
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
    let arcs = vec![
        (0, 4, label("sent", "word", "w")),
        (0, 1, label("sent", "word", "w")),
    ];
    let fan = Fan::new("arg", 0)
        .with_child("theme", 4)
        .with_child("agent", 1);
    WInstance::new(nodes, arcs, vec![fan], 0, "sent".into())
}

#[test]
fn from_wtype_lays_out_ids_as_documented() {
    let graph = GInstance::from_wtype(&sentence()).expect("a tree has a graph form");
    assert_eq!(graph.edge(EdgeId(0)).map(|e| e.tgt), Some(4), "arc 0");
    assert_eq!(graph.edge(EdgeId(1)).map(|e| e.tgt), Some(1), "arc 1");

    let fan = graph
        .node(5)
        .expect("the fan node follows the largest node id");
    assert_eq!(fan.anchor, "arg");
    assert_eq!(fan.shape, NodeShape::HyperEdge);

    let parent = graph
        .edge(EdgeId(2))
        .expect("the parent edge follows the arcs");
    assert_eq!((parent.src, parent.tgt), (5, 0));
    assert_eq!(parent.edge.kind, HYPER_PARENT_KIND);
    assert_eq!(parent.edge.src, "arg");
    assert_eq!(parent.edge.tgt, "sent");
    assert_eq!(parent.edge.name, None, "no schema names the parent label");

    let agent = graph
        .edge(EdgeId(3))
        .expect("children follow in label order");
    assert_eq!(agent.edge.kind, HYPER_LABEL_KIND);
    assert_eq!(agent.edge.name.as_deref(), Some("agent"));
    assert_eq!(agent.tgt, 1);
    let theme = graph.edge(EdgeId(4)).expect("the second child");
    assert_eq!(theme.edge.name.as_deref(), Some("theme"));
    assert_eq!(theme.tgt, 4);
}

#[test]
fn a_schema_names_the_fan_parent_edge() {
    let mut schema = SchemaBuilder::new(&Protocol::default())
        .vertex("sent", "record", None)
        .expect("vertex")
        .vertex("word", "record", None)
        .expect("vertex")
        .build()
        .expect("schema");
    schema.hyper_edges.insert(
        "arg".into(),
        HyperEdge {
            id: "arg".into(),
            kind: "relation".into(),
            signature: HashMap::from([
                ("pred".into(), "sent".into()),
                ("agent".into(), "word".into()),
                ("theme".into(), "word".into()),
            ]),
            parent_label: "pred".into(),
        },
    );
    let graph =
        GInstance::from_wtype_with_schema(&sentence(), &schema).expect("a tree has a graph form");
    assert_eq!(
        graph.edge(EdgeId(2)).and_then(|e| e.edge.name.as_deref()),
        Some("pred")
    );
    let back = graph.to_wtype(0).expect("the graph form is a tree");
    assert_eq!(tree_bytes(&back), tree_bytes(&sentence()));
}

#[test]
fn to_wtype_refuses_what_is_not_a_tree() {
    let knows = Edge {
        src: "p".into(),
        tgt: "p".into(),
        kind: "knows".into(),
        name: None,
    };
    let people = |n: u32| (0..n).fold(GInstance::new(), |g, id| g.with_node(Node::new(id, "p")));

    let two_parents = people(3)
        .with_edge(0, 2, knows.clone())
        .with_edge(1, 2, knows.clone());
    assert!(matches!(
        two_parents.to_wtype(0),
        Err(InstError::NotATree { node: 1, .. })
    ));

    let cycle = people(3)
        .with_edge(0, 1, knows.clone())
        .with_edge(1, 2, knows.clone())
        .with_edge(2, 1, knows.clone());
    assert!(matches!(
        cycle.to_wtype(0),
        Err(InstError::NotATree { node: 1, .. })
    ));

    let detached = people(3)
        .with_edge(0, 1, knows.clone())
        .with_edge(2, 2, knows.clone());
    assert!(matches!(
        detached.to_wtype(0),
        Err(InstError::NotATree { node: 2, .. })
    ));

    let mut positioned = people(2).with_edge(0, 1, knows);
    positioned
        .set_edge_position(EdgeId(0), Some(1))
        .expect("the edge exists");
    assert!(matches!(
        positioned.to_wtype(0),
        Err(InstError::NotATree { node: 0, .. })
    ));

    assert!(matches!(people(1).to_wtype(7), Err(InstError::MissingRoot)));
}

#[test]
fn from_wtype_refuses_a_dangling_arc() {
    let nodes = HashMap::from([(0, Node::new(0, "sent"))]);
    let tree = WInstance::new(
        nodes,
        vec![(0, 3, label("sent", "word", "w"))],
        vec![],
        0,
        "sent".into(),
    );
    assert!(matches!(
        GInstance::from_wtype(&tree),
        Err(InstError::DanglingEdge {
            edge: EdgeId(0),
            endpoint: 3
        })
    ));
}

#[test]
fn a_malformed_fan_node_stays_an_ordinary_node() {
    let graph = GInstance::new()
        .with_node(Node::new(0, "sent"))
        .with_node(Node::new(1, "arg").with_shape(NodeShape::HyperEdge))
        .with_edge(
            1,
            0,
            Edge {
                src: "arg".into(),
                tgt: "sent".into(),
                kind: HYPER_LABEL_KIND.into(),
                name: Some("only_a_label".into()),
            },
        );
    let (rest, fans) = unreify_fans(&graph);
    assert!(fans.is_empty(), "no hyper_parent edge, so not a fan node");
    assert!(GInstance::identical(&rest, &graph));
}

#[test]
fn a_table_instance_lays_out_rows_and_keys_in_order() {
    let author = Edge {
        src: "posts".into(),
        tgt: "users".into(),
        kind: "author".into(),
        name: None,
    };
    let row = |k: &str, v: i64| HashMap::from([(k.to_owned(), Value::Int(v))]);
    let tables = FInstance::new()
        .with_table("users", vec![row("uid", 10), row("uid", 11)])
        .with_table("posts", vec![row("pid", 20), row("pid", 21)])
        .with_foreign_key(author.clone(), vec![(1, 0), (0, 1)]);
    let graph = GInstance::from_finstance(&tables).expect("the table instance is a graph");

    // "posts" sorts before "users": posts 0 and 1 are nodes 0 and 1.
    assert_eq!(graph.node(0).map(|n| &n.anchor), Some(&Name::from("posts")));
    assert_eq!(
        graph.node(3).and_then(|n| n.extra_fields.get("uid")),
        Some(&Value::Int(11))
    );
    assert_eq!(
        graph.edge(EdgeId(0)),
        Some(&GEdge::new(1, 2, author.clone()))
    );
    assert_eq!(
        graph.edge(EdgeId(1)),
        Some(&GEdge::new(0, 3, author.clone()))
    );

    let broken = FInstance::new()
        .with_table("users", vec![row("uid", 10)])
        .with_table("posts", vec![row("pid", 20)])
        .with_foreign_key(author, vec![(0, 5)]);
    assert!(matches!(
        GInstance::from_finstance(&broken),
        Err(InstError::DanglingForeignKey { row: 5, .. })
    ));
}

#[test]
fn node_link_json_lists_by_id_and_rejects_dangling_edges() {
    let graph = GInstance::new()
        .with_node(Node::new(3, "a"))
        .with_node(Node::new(1, "a"))
        .with_edge_id(
            EdgeId(9),
            3,
            1,
            Edge {
                src: "a".into(),
                tgt: "a".into(),
                kind: "k".into(),
                name: None,
            },
        );
    let doc = to_node_link_json(&graph);
    let ids: Vec<u64> = doc["nodes"]
        .as_array()
        .expect("nodes is an array")
        .iter()
        .filter_map(|n| n["id"].as_u64())
        .collect();
    assert_eq!(ids, vec![1, 3]);
    assert_eq!(doc["edges"][0]["id"], 9);
    assert_eq!(doc["edges"][0]["position"], serde_json::Value::Null);

    let mut broken = doc.clone();
    broken["edges"][0]["tgt"] = serde_json::json!(8);
    assert!(matches!(
        from_node_link_json(&broken),
        Err(InstError::DanglingEdge {
            edge: EdgeId(9),
            endpoint: 8
        })
    ));

    let mut doubled = doc;
    let first = doubled["nodes"][0].clone();
    doubled["nodes"]
        .as_array_mut()
        .expect("nodes is an array")
        .push(first);
    assert!(matches!(
        from_node_link_json(&doubled),
        Err(InstError::DuplicateNode(1))
    ));

    assert!(matches!(
        from_node_link_json(&serde_json::json!({"nodes": 3})),
        Err(InstError::NodeLink(_))
    ));
}

#[test]
fn a_schema_root_other_than_the_root_anchor_comes_back() {
    let nodes = HashMap::from([(0, Node::new(0, "doc")), (1, Node::new(1, "para"))]);
    let tree = WInstance::new(
        nodes,
        vec![(0, 1, label("doc", "para", "p"))],
        vec![],
        0,
        "entry".into(),
    );
    let graph = GInstance::from_wtype(&tree).expect("a tree has a graph form");
    let back = graph
        .to_wtype_with_schema_root(tree.root, tree.schema_root.clone())
        .expect("the graph form is a tree");
    assert_eq!(back.schema_root, "entry");
    assert_eq!(tree_bytes(&back), tree_bytes(&tree));
}

#[test]
fn a_root_shaped_like_a_fan_node_stays_the_root() {
    // The root has the fan-node shape and a single `hyper_parent` out-edge,
    // so on shape alone it reads as a fan node. It is the root all the same.
    let root = Node::new(0, "doc").with_shape(NodeShape::HyperEdge);
    let nodes = HashMap::from([(0, root), (1, Node::new(1, "para"))]);
    let arc = Edge {
        src: "doc".into(),
        tgt: "para".into(),
        kind: HYPER_PARENT_KIND.into(),
        name: None,
    };
    let tree = WInstance::new(nodes, vec![(0, 1, arc)], vec![], 0, "doc".into());
    let graph = GInstance::from_wtype(&tree).expect("a tree has a graph form");
    let back = graph.to_wtype(0).expect("the graph form is a tree");
    assert_eq!(tree_bytes(&back), tree_bytes(&tree));
}

#[test]
fn from_wtype_refuses_a_node_its_graph_form_would_read_as_a_fan() {
    // Node 1 has no incoming arc and the form of a fan node: the fan-node
    // shape, one `hyper_parent` out-arc and a named `hyper_label` out-arc.
    // Its graph form is the graph form of the one-node tree with a fan, so
    // reading the graph back would turn node 1 into that fan.
    let nodes = HashMap::from([
        (0, Node::new(0, "sent")),
        (1, Node::new(1, "arg").with_shape(NodeShape::HyperEdge)),
    ]);
    let parent = Edge {
        src: "arg".into(),
        tgt: "sent".into(),
        kind: HYPER_PARENT_KIND.into(),
        name: None,
    };
    let child = Edge {
        src: "arg".into(),
        tgt: "sent".into(),
        kind: HYPER_LABEL_KIND.into(),
        name: Some("theme".into()),
    };
    let orphan = WInstance::new(
        nodes,
        vec![(1, 0, parent), (1, 0, child)],
        vec![],
        0,
        "sent".into(),
    );
    let as_fan = WInstance::new(
        HashMap::from([(0, Node::new(0, "sent"))]),
        vec![],
        vec![Fan::new("arg", 0).with_child("theme", 0)],
        0,
        "sent".into(),
    );
    let fan_graph = GInstance::from_wtype(&as_fan).expect("a tree has a graph form");
    assert_eq!(
        tree_bytes(&fan_graph.to_wtype(0).expect("the graph form is a tree")),
        tree_bytes(&as_fan)
    );
    assert!(matches!(
        GInstance::from_wtype(&orphan),
        Err(InstError::NotATree { node: 1, .. })
    ));
}

#[test]
fn to_wtype_reads_every_non_root_node_of_fan_form_as_a_fan() {
    // A graph built by hand, not by `from_wtype`: node 1 has the fan-node
    // shape and one `hyper_parent` out-edge. Fan nodes are recognised by
    // form alone, so the way back reads node 1 as a childless fan over node 0
    // rather than as an unreachable node.
    let graph = GInstance::new()
        .with_node(Node::new(0, "sent"))
        .with_node(Node::new(1, "arg").with_shape(NodeShape::HyperEdge))
        .with_edge(
            1,
            0,
            Edge {
                src: "arg".into(),
                tgt: "sent".into(),
                kind: HYPER_PARENT_KIND.into(),
                name: None,
            },
        );
    let tree = graph.to_wtype(0).expect("node 1 reads as a fan");
    assert_eq!(tree.nodes.len(), 1);
    assert_eq!(tree.fans, vec![Fan::new("arg", 0)]);

    // The same node with a plain edge instead is an ordinary node, and not
    // part of a tree rooted at node 0.
    let plain = GInstance::new()
        .with_node(Node::new(0, "sent"))
        .with_node(Node::new(1, "arg").with_shape(NodeShape::HyperEdge))
        .with_edge(
            1,
            0,
            Edge {
                src: "arg".into(),
                tgt: "sent".into(),
                kind: "prop".into(),
                name: None,
            },
        );
    assert!(matches!(plain.to_wtype(0), Err(InstError::NotATree { .. })));
}

#[test]
fn a_fan_cannot_name_another_fans_node() {
    // One tree node, 0. The first fan gets node 1, so a second fan naming
    // node 1 names a node the tree does not hold.
    let nodes = HashMap::from([(0, Node::new(0, "sent"))]);
    let fans = vec![
        Fan::new("arg", 0),
        Fan::new("arg", 0).with_child("theme", 1),
    ];
    let tree = WInstance::new(nodes, vec![], fans, 0, "sent".into());
    assert!(matches!(
        GInstance::from_wtype(&tree),
        Err(InstError::DanglingEdge { endpoint: 1, .. })
    ));
}

/// A graph holding `NaN` and both infinities: as a node's value, as extra
/// fields and annotations, and inside a list and a record.
fn graph_with_non_finite_floats() -> GInstance {
    let mut odd = Node::new(1, "a");
    odd.extra_fields
        .insert("up".into(), Value::Float(f64::INFINITY));
    odd.extra_fields.insert(
        "nested".into(),
        Value::List(vec![
            Value::Float(f64::NEG_INFINITY),
            Value::Unknown(HashMap::from([("x".into(), Value::Float(f64::NAN))])),
        ]),
    );
    odd.annotations
        .insert("score".into(), Value::Float(f64::NAN));
    // A map entry named like the marker's variant, holding an ordinary value,
    // is data and stays as it is.
    odd.extra_fields.insert("Float".into(), Value::Null);
    GInstance::new()
        .with_node(odd)
        .with_node(Node::new(0, "a"))
        .with_value(0, Value::Float(f64::NAN))
        .with_edge(
            0,
            1,
            Edge {
                src: "a".into(),
                tgt: "a".into(),
                kind: "k".into(),
                name: None,
            },
        )
}

#[test]
fn the_serde_encoding_keeps_floats_json_cannot_write() {
    let graph = graph_with_non_finite_floats();

    let text = serde_json::to_string(&graph).expect("a graph encodes as JSON");
    let back: GInstance = serde_json::from_str(&text).expect("the JSON reads back");
    assert!(GInstance::identical(&back, &graph), "{text}");
    assert!(text.contains(r#"{"Float":"Infinity"}"#), "{text}");
    assert!(text.contains(r#"{"Float":"-Infinity"}"#), "{text}");
    assert!(
        !text.contains(r#"{"Float":null}"#),
        "no float is written as null: {text}"
    );
    assert_eq!(
        back.node(1).and_then(|n| n.extra_fields.get("up")).cloned(),
        Some(Value::Float(f64::INFINITY)),
        "an infinity keeps its sign"
    );

    // A binary format writes the floats as floats.
    let bytes = rmp_serde::to_vec_named(&graph).expect("a graph encodes as MessagePack");
    let back: GInstance = rmp_serde::from_slice(&bytes).expect("the MessagePack reads back");
    assert!(GInstance::identical(&back, &graph));

    let bad = text.replacen(r#""Infinity""#, r#""Endless""#, 1);
    let err = serde_json::from_str::<GInstance>(&bad).expect_err("`Endless` is not a float");
    assert!(err.to_string().contains("Endless"), "{err}");
}

#[test]
fn node_link_json_keeps_floats_json_cannot_write() {
    let graph = graph_with_non_finite_floats();
    let doc = to_node_link_json(&graph);
    let back = from_node_link_json(&doc).expect("the document reads back");
    assert!(GInstance::identical(&back, &graph));
    let text = doc.to_string();
    let reparsed: serde_json::Value = serde_json::from_str(&text).expect("the text is JSON");
    let again = from_node_link_json(&reparsed).expect("the text reads back");
    assert!(GInstance::identical(&again, &graph));
    assert_eq!(
        again
            .node(1)
            .and_then(|n| n.extra_fields.get("up"))
            .cloned(),
        Some(Value::Float(f64::INFINITY)),
        "an infinity keeps its sign"
    );

    let mut bad = doc;
    bad["nodes"][0]["value"]["Present"]["Float"] = serde_json::json!("NotANumber");
    assert!(matches!(
        from_node_link_json(&bad),
        Err(InstError::NodeLink(_))
    ));
}

#[test]
fn decoding_refuses_a_key_listed_twice() {
    let node = |id: u32| {
        format!(
            r#""{id}":{{"id":{id},"anchor":"a","value":null,"discriminator":null,"extra_fields":{{}}}}"#
        )
    };
    let edge = |kind: &str| {
        format!(
            r#"{{"src":0,"tgt":1,"edge":{{"src":"a","tgt":"a","kind":"{kind}","name":null}},"position":null}}"#
        )
    };
    let canonical = format!(
        r#"{{"nodes":{{{},{}}},"edges":{{"1":{}}}}}"#,
        node(0),
        node(1),
        edge("k")
    );
    let graph: GInstance = serde_json::from_str(&canonical).expect("the canonical form decodes");
    assert_eq!(graph.edge_count(), 1);

    let twice_edge = format!(
        r#"{{"nodes":{{{},{}}},"edges":{{"1":{},"1":{}}}}}"#,
        node(0),
        node(1),
        edge("k"),
        edge("j")
    );
    let err = serde_json::from_str::<GInstance>(&twice_edge).expect_err("edge 1 is listed twice");
    assert!(err.to_string().contains("edge 1"), "{err}");

    let twice_node = format!(
        r#"{{"nodes":{{{},{},{}}},"edges":{{}}}}"#,
        node(0),
        node(1),
        node(0)
    );
    let err = serde_json::from_str::<GInstance>(&twice_node).expect_err("node 0 is listed twice");
    assert!(err.to_string().contains("node 0"), "{err}");
}
