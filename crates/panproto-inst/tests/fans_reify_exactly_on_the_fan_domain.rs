//! Fans reify into fan nodes and back, exactly, on the fan domain.
//!
//! `reify_fans` refuses a graph that already holds reserved fan vocabulary;
//! `unreify_fans` refuses one outside `FanDomain` with the violation it finds;
//! and on their domains the two are inverses. The generators are
//! adversarial: they build graphs holding fan-shaped nodes, reserved edge
//! kinds on ordinary nodes, and fans whose participants are fan nodes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, HashMap};

use panproto_gat::Name;
use panproto_inst::{
    EdgeId, Fan, FanDomain, FanViolation, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND,
    InstError, Node, NodeShape, reify_fans, reify_fans_owned, unreify_fans, unreify_fans_owned,
    unreify_fans_with_schema,
};
use panproto_schema::{Edge, HyperEdge, Schema};
use proptest::prelude::*;

fn fan_edge(kind: &str, name: Option<&str>) -> Edge {
    Edge {
        src: "rel".into(),
        tgt: "item".into(),
        kind: kind.into(),
        name: name.map(Name::from),
    }
}

fn parent_edge() -> Edge {
    fan_edge(HYPER_PARENT_KIND, None)
}

fn label_edge(label: &str) -> Edge {
    fan_edge(HYPER_LABEL_KIND, Some(label))
}

fn link() -> Edge {
    fan_edge("link", None)
}

fn items(n: u32) -> GInstance {
    (0..n).fold(GInstance::new(), |g, id| g.with_node(Node::new(id, "item")))
}

fn fan_node(graph: GInstance, id: u32) -> GInstance {
    graph.with_node(Node::new(id, "rel").with_shape(NodeShape::HyperEdge))
}

fn violation(graph: &GInstance) -> (u32, FanViolation) {
    match unreify_fans(graph) {
        Err(InstError::MalformedFan { node, reason }) => (node, reason),
        other => panic!("expected a malformed fan, got {other:?}"),
    }
}

#[test]
fn each_fan_violation_is_detected() {
    // MissingParent: a fan-shaped node with one labelled edge only.
    let g = fan_node(items(2), 5).with_edge(5, 0, label_edge("a"));
    assert_eq!(violation(&g), (5, FanViolation::MissingParent));

    // MultipleParents.
    let g = fan_node(items(2), 5)
        .with_edge(5, 0, parent_edge())
        .with_edge(5, 1, parent_edge());
    assert_eq!(
        violation(&g),
        (
            5,
            FanViolation::MultipleParents {
                edges: vec![EdgeId(0), EdgeId(1)]
            }
        )
    );

    // DuplicateLabel.
    let g = fan_node(items(2), 5)
        .with_edge(5, 0, parent_edge())
        .with_edge(5, 0, label_edge("a"))
        .with_edge(5, 1, label_edge("a"));
    assert_eq!(
        violation(&g),
        (
            5,
            FanViolation::DuplicateLabel {
                label: "a".into(),
                edges: vec![EdgeId(1), EdgeId(2)]
            }
        )
    );

    // UnknownLabel needs a schema: the fan is otherwise well formed.
    let g = fan_node(items(2), 5)
        .with_edge(5, 0, parent_edge())
        .with_edge(5, 1, label_edge("zzz"));
    assert!(unreify_fans(&g).is_ok());
    let mut schema = Schema::default();
    schema.hyper_edges.insert(
        "rel".into(),
        HyperEdge {
            id: "rel".into(),
            kind: "relation".into(),
            signature: HashMap::from([("head".into(), "item".into()), ("a".into(), "item".into())]),
            parent_label: "head".into(),
        },
    );
    assert!(matches!(
        unreify_fans_with_schema(&g, &schema),
        Err(InstError::MalformedFan {
            node: 5,
            reason: FanViolation::UnknownLabel { label }
        }) if label == "zzz"
    ));

    // ReservedKindOutsideFan: an ordinary node with a reserved-kind edge.
    let g = items(2).with_edge(0, 1, label_edge("a"));
    assert_eq!(
        violation(&g),
        (0, FanViolation::ReservedKindOutsideFan { edge: EdgeId(0) })
    );

    // ForeignIncidentEdge: an in-edge (nested reification), an out-edge of
    // an ordinary kind, and an unnamed labelled edge.
    let nested = fan_node(fan_node(items(1), 5), 6)
        .with_edge(5, 0, parent_edge())
        .with_edge(6, 0, parent_edge())
        .with_edge(6, 5, label_edge("inner"));
    assert_eq!(
        violation(&nested),
        (5, FanViolation::ForeignIncidentEdge { edge: EdgeId(2) })
    );
    let ordinary_out = fan_node(items(1), 5)
        .with_edge(5, 0, parent_edge())
        .with_edge(5, 0, fan_edge("prop", None));
    assert_eq!(
        violation(&ordinary_out),
        (5, FanViolation::ForeignIncidentEdge { edge: EdgeId(1) })
    );
    let unnamed = fan_node(items(1), 5)
        .with_edge(5, 0, parent_edge())
        .with_edge(5, 0, fan_edge(HYPER_LABEL_KIND, None));
    assert_eq!(
        violation(&unnamed),
        (5, FanViolation::ForeignIncidentEdge { edge: EdgeId(1) })
    );
}

#[test]
fn a_graph_reified_and_unreified_allocates_as_before() {
    let graph = items(2).with_edge(0, 1, link());
    let fans = vec![Fan::new("rel", 0).with_child("x", 1)];
    let reified = reify_fans(&graph, &fans, None).unwrap();
    let (mut rest, back) = unreify_fans(&reified).unwrap();
    assert_eq!(rest, graph);
    assert_eq!(back, fans);
    assert_eq!(rest.add_edge(1, 0, link()).unwrap(), EdgeId(1));
    let (mut rest, _) = unreify_fans_owned(reified).unwrap();
    assert_eq!(rest.add_edge(1, 0, link()).unwrap(), EdgeId(1));
}

#[test]
fn the_by_value_forms_hand_a_fanless_graph_back_unchanged() {
    let graph = items(3).with_edge(0, 1, link()).with_edge(1, 2, link());
    let reified = reify_fans_owned(graph.clone(), &[], None).unwrap();
    assert!(GInstance::identical(&reified, &graph));
    let (rest, fans) = unreify_fans_owned(graph.clone()).unwrap();
    assert!(GInstance::identical(&rest, &graph));
    assert!(fans.is_empty());
    // Both keep refusing what the by-reference forms refuse.
    let malformed = fan_node(items(2), 5).with_edge(5, 0, label_edge("a"));
    assert!(matches!(
        unreify_fans_owned(malformed.clone()),
        Err(InstError::MalformedFan {
            node: 5,
            reason: FanViolation::MissingParent
        })
    ));
    assert!(matches!(
        reify_fans_owned(malformed, &[], None),
        Err(InstError::MalformedFan { node: 5, .. })
    ));
}

/// Where a graph keeps its node and edge storage: a graph moved without a
/// copy keeps both at the same addresses, a copy gets new ones.
fn storage(graph: &GInstance) -> (*const Node, *const GEdge) {
    (
        std::ptr::from_ref(graph.node(0).expect("node 0")),
        std::ptr::from_ref(graph.edge(EdgeId(0)).expect("edge 0")),
    )
}

#[test]
fn the_by_value_forms_do_not_copy_a_fanless_graph() {
    let graph = items(3).with_edge(0, 1, link()).with_edge(1, 2, link());
    let before = storage(&graph);
    let reified = reify_fans_owned(graph, &[], None).unwrap();
    assert_eq!(
        storage(&reified),
        before,
        "reify_fans_owned copied the graph"
    );
    let (rest, fans) = unreify_fans_owned(reified).unwrap();
    assert!(fans.is_empty());
    assert_eq!(
        storage(&rest),
        before,
        "unreify_fans_owned copied the graph"
    );
    // The by-reference forms return a copy, whose storage is elsewhere.
    assert_ne!(storage(&reify_fans(&rest, &[], None).unwrap()), before);
    assert_ne!(storage(&unreify_fans(&rest).unwrap().0), before);
}

#[test]
fn unreify_rejects_a_malformed_fan() {
    let g = fan_node(items(1), 3).with_edge(3, 0, label_edge("only_a_label"));
    assert!(!FanDomain::contains(&g));
    let err = unreify_fans(&g).expect_err("outside the fan domain");
    assert_eq!(
        err.to_string(),
        "node 3 is not a well-formed fan: fan node has no hyper_parent out-edge"
    );
}

#[test]
fn an_unknown_fan_label_needs_a_schema() {
    let g = fan_node(items(2), 5)
        .with_edge(5, 0, parent_edge())
        .with_edge(5, 1, label_edge("theme"));
    assert!(FanDomain::check(&g).is_ok(), "no schema, no label check");
    // A hyper-edge the schema does not declare is reported as such, and its
    // labels are not checked.
    assert!(matches!(
        FanDomain::check_with_schema(&g, &Schema::default()),
        Err(InstError::MalformedFan {
            node: 5,
            reason: FanViolation::UnknownHyperEdge { hyper_edge },
        }) if hyper_edge == "rel"
    ));
    let mut schema = Schema::default();
    schema.hyper_edges.insert(
        "rel".into(),
        HyperEdge {
            id: "rel".into(),
            kind: "relation".into(),
            signature: HashMap::from([("theme".into(), "item".into())]),
            parent_label: "head".into(),
        },
    );
    let (rest, fans) = unreify_fans_with_schema(&g, &schema).expect("the label is declared");
    assert_eq!(rest.node_count(), 2);
    assert_eq!(fans, vec![Fan::new("rel", 0).with_child("theme", 1)]);
}

#[test]
fn reify_rejects_reserved_vocabulary_in_its_input() {
    let fans = [Fan::new("rel", 0)];
    // A fan-shaped node, malformed.
    let shaped = fan_node(items(1), 4);
    assert!(matches!(
        reify_fans(&shaped, &fans, None),
        Err(InstError::MalformedFan {
            node: 4,
            reason: FanViolation::MissingParent
        })
    ));
    // A well-formed fan already present is reserved vocabulary too.
    let reified = reify_fans(&items(1), &fans, None).expect("reifies");
    assert!(matches!(
        reify_fans(&reified, &fans, None),
        Err(InstError::MalformedFan {
            node: 1,
            reason: FanViolation::ReservedKindOutsideFan { edge: EdgeId(0) }
        })
    ));
    // A reserved kind on an ordinary edge.
    let kinded = items(2).with_edge(0, 1, parent_edge());
    assert!(matches!(
        reify_fans(&kinded, &fans, None),
        Err(InstError::MalformedFan { node: 0, .. })
    ));
}

/// A graph that may hold reserved vocabulary: nodes of the fan shape, edges
/// of the reserved kinds between any nodes (so fan-shaped nodes may nest),
/// and ordinary edges, with fans over its nodes and sometimes over one id
/// past them, which `reify_fans` will give to a fan node.
fn arb_adversarial() -> impl Strategy<Value = (GInstance, Vec<Fan>)> {
    (1u32..7)
        .prop_flat_map(|n| {
            let shapes = prop::collection::vec(prop::bool::weighted(0.15), n as usize);
            let edges = prop::collection::vec(
                (
                    0..n,
                    0..n,
                    prop::sample::select(vec![
                        "link",
                        "link",
                        "link",
                        HYPER_PARENT_KIND,
                        HYPER_LABEL_KIND,
                    ]),
                    prop::option::of("[a-b]"),
                ),
                0..8,
            );
            let fans = prop::collection::vec(
                (0..=n, prop::collection::btree_map("[a-c]", 0..=n, 0..3)),
                0..3,
            );
            (Just(n), shapes, edges, fans)
        })
        .prop_map(|(n, shapes, edges, fans)| {
            let mut graph = GInstance::new();
            for (id, hyper) in (0..n).zip(shapes) {
                let node = Node::new(id, "item");
                graph = graph.with_node(if hyper {
                    node.with_shape(NodeShape::HyperEdge)
                } else {
                    node
                });
            }
            for (src, tgt, kind, name) in edges {
                graph = graph.with_edge(
                    src,
                    tgt,
                    Edge {
                        src: "item".into(),
                        tgt: "item".into(),
                        kind: kind.into(),
                        name: name.map(|s| Name::from(s.as_str())),
                    },
                );
            }
            let fans = fans
                .into_iter()
                .map(|(parent, children)| Fan {
                    hyper_edge_id: "rel".into(),
                    parent,
                    children: children.into_iter().collect(),
                })
                .collect();
            (graph, fans)
        })
}

fn has_reserved_vocabulary(graph: &GInstance) -> bool {
    graph
        .nodes()
        .values()
        .any(|n| n.shape == NodeShape::HyperEdge)
        || graph
            .edges()
            .values()
            .any(|e| e.edge.kind == HYPER_PARENT_KIND || e.edge.kind == HYPER_LABEL_KIND)
}

proptest! {
    #[test]
    fn unreify_inverts_reify((graph, fans) in arb_adversarial()) {
        let names_only_graph_nodes = fans.iter().all(|f| {
            graph.node(f.parent).is_some() && f.children.values().all(|c| graph.node(*c).is_some())
        });
        match reify_fans(&graph, &fans, None) {
            Ok(reified) => {
                prop_assert!(!has_reserved_vocabulary(&graph));
                prop_assert!(FanDomain::contains(&reified));
                let (rest, back) = unreify_fans(&reified).expect("a reified graph is in the fan domain");
                prop_assert!(GInstance::identical(&rest, &graph));
                prop_assert_eq!(&back, &fans);
                // The graph got back allocates as the original does.
                if let Some(&n) = graph.nodes().keys().next() {
                    let (mut ours, mut theirs) = (graph.clone(), rest);
                    prop_assert_eq!(
                        ours.add_edge(n, n, link()).ok(),
                        theirs.add_edge(n, n, link()).ok()
                    );
                }
                // The by-value forms give the same results.
                let owned = reify_fans_owned(graph.clone(), &fans, None).expect("reifies as by reference");
                prop_assert!(GInstance::identical(&owned, &reified));
                let (owned_rest, owned_back) = unreify_fans_owned(owned).expect("unreifies as by reference");
                prop_assert!(GInstance::identical(&owned_rest, &graph));
                prop_assert_eq!(owned_back, fans);
            }
            Err(InstError::MalformedFan { .. }) => prop_assert!(has_reserved_vocabulary(&graph)),
            Err(InstError::DanglingEdge { .. }) => {
                prop_assert!(!has_reserved_vocabulary(&graph));
                prop_assert!(!names_only_graph_nodes);
            }
            Err(other) => prop_assert!(false, "unexpected error {other:?}"),
        }
    }
}

/// `count` distinct ids below `below`, in a random order: a shuffled prefix of
/// the range, so the draw never rejects (a fixed-size set drawn element by
/// element rejects whenever two draws collide).
fn distinct_ids(below: u32, count: usize) -> impl Strategy<Value = Vec<u32>> {
    Just((0..below).collect::<Vec<u32>>())
        .prop_shuffle()
        .prop_map(move |ids| ids[..count].to_vec())
}

/// A graph in the fan domain with fan node ids and fan edge ids chosen
/// freely: an ordinary graph, then fans over its nodes whose nodes and edges
/// take shuffled ids from a pool above and below the ordinary ones. Unless
/// `bare`, a fan node may carry a position and a discriminator, and its
/// edges a position and, on the parent edge, a name, none of which a fan
/// keeps.
fn arb_fan_domain() -> impl Strategy<Value = GInstance> {
    (1u32..6)
        .prop_flat_map(|n| {
            (
                Just(n),
                prop::collection::vec((0..n, 0..n), 0..6),
                prop::collection::vec(
                    (0..n, prop::collection::btree_map("[a-c]", 0..n, 0..3)),
                    0..4,
                ),
                distinct_ids(40, 4),
                distinct_ids(80, 16),
                any::<bool>(),
                any::<bool>(),
                prop::collection::vec(
                    (
                        prop::option::of(0u32..4),
                        prop::option::of("[a-b]"),
                        prop::option::of("[a-b]"),
                        prop::option::of(0u32..4),
                    ),
                    4,
                ),
            )
        })
        .prop_map(
            |(n, edges, fans, fan_node_pool, fan_edge_pool, canonical, bare, data)| {
                // Ordinary nodes take ids 100.., ordinary edges 200.., so the
                // pools (below 100) never collide with them.
                let mut graph = GInstance::new();
                for i in 0..n {
                    graph = graph.with_node(Node::new(100 + i, "item"));
                }
                for (k, (s, t)) in edges.into_iter().enumerate() {
                    let id = EdgeId(200 + u32::try_from(k).expect("small"));
                    graph
                        .insert_edge(id, GEdge::new(100 + s, 100 + t, fan_edge("link", None)))
                        .expect("ends exist");
                }
                let base_nodes = 100 + n;
                let base_edges = graph.max_edge_id().map_or(0, |EdgeId(m)| m + 1);
                let mut next_edge = 0usize;
                for (k, (parent_node, children)) in fans.into_iter().enumerate() {
                    let node = if canonical {
                        base_nodes + u32::try_from(k).expect("small")
                    } else {
                        fan_node_pool[k]
                    };
                    let (node_position, discriminator, parent_name, edge_position) = if bare {
                        (None, None, None, None)
                    } else {
                        data[k].clone()
                    };
                    let mut fan = Node::new(node, "rel").with_shape(NodeShape::HyperEdge);
                    fan.position = node_position;
                    fan.discriminator = discriminator.map(|d| Name::from(d.as_str()));
                    graph = graph.with_node(fan);
                    let mut parent = parent_edge();
                    parent.name = parent_name.map(|p| Name::from(p.as_str()));
                    let mut targets = vec![(100 + parent_node, parent)];
                    targets.extend(children.into_iter().map(|(l, c)| (100 + c, label_edge(&l))));
                    for (target, label) in targets {
                        let id = if canonical {
                            EdgeId(base_edges + u32::try_from(next_edge).expect("small"))
                        } else {
                            EdgeId(fan_edge_pool[next_edge])
                        };
                        next_edge += 1;
                        let mut edge = GEdge::new(node, target, label);
                        edge.position = edge_position;
                        graph.insert_edge(id, edge).expect("ends exist");
                    }
                }
                graph
            },
        )
}

/// Rename a fan-domain graph's fan nodes and fan edges to the ids
/// `reify_fans` gives them: fan nodes in ascending id after every other
/// node, their edges after every other edge, fan by fan, parent first, then
/// children in ascending label.
fn reify_layout(graph: &GInstance) -> GInstance {
    let mut fan_ids: Vec<u32> = graph
        .nodes()
        .values()
        .filter(|n| n.shape == NodeShape::HyperEdge)
        .map(|n| n.id)
        .collect();
    fan_ids.sort_unstable();
    let is_fan = |id: u32| fan_ids.contains(&id);
    let node_floor = graph
        .nodes()
        .keys()
        .filter(|id| !is_fan(**id))
        .max()
        .map_or(0, |m| m + 1);
    let edge_floor = graph
        .edges()
        .iter()
        .filter(|(_, e)| !is_fan(e.src))
        .map(|(id, _)| id.0)
        .max()
        .map_or(0, |m| m + 1);
    let mut nodes: BTreeMap<u32, u32> = BTreeMap::new();
    let mut edges: BTreeMap<EdgeId, EdgeId> = BTreeMap::new();
    let mut next_edge = edge_floor;
    for (k, &fan) in fan_ids.iter().enumerate() {
        nodes.insert(fan, node_floor + u32::try_from(k).expect("small"));
        let mut out: Vec<(u8, Option<Name>, EdgeId)> = graph
            .out_edges(fan)
            .iter()
            .map(|id| {
                let e = &graph.edge(*id).expect("an edge").edge;
                (u8::from(e.kind == HYPER_LABEL_KIND), e.name.clone(), *id)
            })
            .collect();
        out.sort();
        for (_, _, id) in out {
            edges.insert(id, EdgeId(next_edge));
            next_edge += 1;
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
        renamed
            .insert_edge(edges.get(&id).copied().unwrap_or(id), edge)
            .expect("a renaming is a bijection");
    }
    renamed
}

/// `graph` with each fan node's data reset to the defaults `reify_fans`
/// writes: the node carries only its id, its anchor and the fan shape, and
/// each fan edge only its ends and the label `reify_fans` writes without a
/// schema (the parent edge unnamed, a child edge named by its label), with
/// no position.
fn reset_fan_data(graph: &GInstance) -> GInstance {
    let mut reset = GInstance::new();
    for node in graph.nodes().values() {
        reset = reset.with_node(if node.shape == NodeShape::HyperEdge {
            Node::new(node.id, node.anchor.clone()).with_shape(NodeShape::HyperEdge)
        } else {
            node.clone()
        });
    }
    for (id, edge) in graph.edges_sorted() {
        let mut edge = edge.clone();
        let src = graph.node(edge.src).expect("an end");
        if src.shape == NodeShape::HyperEdge {
            edge.edge.src = src.anchor.clone();
            edge.edge.tgt = graph.node(edge.tgt).expect("an end").anchor.clone();
            if edge.edge.kind == HYPER_PARENT_KIND {
                edge.edge.name = None;
            }
            edge.position = None;
        }
        reset.insert_edge(id, edge).expect("the same ends");
    }
    reset
}

/// A schema that declares the hyper-edge `rel`, so `reify_fans` names each
/// fan's parent edge by its parent label.
fn rel_schema() -> Schema {
    let mut schema = Schema::default();
    schema.hyper_edges.insert(
        "rel".into(),
        HyperEdge {
            id: "rel".into(),
            kind: "relation".into(),
            signature: HashMap::from([
                ("a".into(), "item".into()),
                ("b".into(), "item".into()),
                ("c".into(), "item".into()),
            ]),
            parent_label: "head".into(),
        },
    );
    schema
}

proptest! {
    /// On the image of `reify_fans`, unreifying then reifying is the
    /// identity, exactly: `h == reify_fans(unreify_fans(h))` for every
    /// `h = reify_fans(g, fans, schema)`, under the same schema.
    #[test]
    fn reify_inverts_unreify_on_the_fan_domain_image(
        (graph, fans) in arb_adversarial(),
        with_schema in any::<bool>(),
    ) {
        let schema = with_schema.then(rel_schema);
        let Ok(h) = reify_fans(&graph, &fans, schema.as_ref()) else {
            return Ok(());
        };
        let (rest, back) = unreify_fans(&h).expect("a reified graph is in the fan domain");
        // Unreifying inverts reifying under a schema too.
        prop_assert!(GInstance::identical(&rest, &graph));
        prop_assert_eq!(&back, &fans);
        let again = reify_fans(&rest, &back, schema.as_ref()).expect("the rest reifies");
        prop_assert_eq!(&again, &h);
        prop_assert!(GInstance::identical(&again, &h));
        // The by-value forms give the same graph, without copying.
        let (owned_rest, owned_back) = unreify_fans_owned(h.clone()).expect("unreifies");
        let owned = reify_fans_owned(owned_rest, &owned_back, schema.as_ref()).expect("reifies");
        prop_assert!(GInstance::identical(&owned, &h));
    }

    /// For a general `h` in the fan domain, unreifying then reifying agrees
    /// with `h` up to renaming fan node and fan edge ids and resetting fan
    /// data to the defaults `reify_fans` writes; exactly when `h` is already
    /// laid out and bare.
    #[test]
    fn reify_inverts_unreify_on_the_fan_domain(h in arb_fan_domain()) {
        prop_assert!(FanDomain::contains(&h));
        let (rest, fans) = unreify_fans(&h).expect("in the fan domain");
        let again = reify_fans(&rest, &fans, None).expect("the rest holds no reserved vocabulary");
        let normal = reset_fan_data(&reify_layout(&h));
        prop_assert!(GInstance::identical(&again, &normal));
        prop_assert_eq!(&again, &normal);
        if GInstance::identical(&normal, &h) {
            // Laid out as reify_fans lays fans out, with fan data at its
            // defaults, the law holds exactly.
            prop_assert!(GInstance::identical(&again, &h));
        }
    }
}
