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
    InstError, Node, NodeShape, reify_fans, unreify_fans, unreify_fans_with_schema,
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
    // A hyper-edge the schema does not declare has no labels at all.
    assert!(matches!(
        FanDomain::check_with_schema(&g, &Schema::default()),
        Err(InstError::MalformedFan {
            reason: FanViolation::UnknownLabel { .. },
            ..
        })
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
                prop_assert_eq!(back, fans);
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

/// A graph in the fan domain with fan node ids and fan edge ids chosen
/// freely: an ordinary graph, then fans over its nodes whose nodes and edges
/// take shuffled ids from a pool above and below the ordinary ones.
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
                prop::collection::btree_set(0u32..40, 4..=4),
                prop::collection::btree_set(0u32..80, 16..=16),
                any::<bool>(),
            )
        })
        .prop_flat_map(|(n, edges, fans, fan_nodes, fan_edges, canonical)| {
            (
                Just((n, edges, fans, canonical)),
                Just(fan_nodes.into_iter().collect::<Vec<_>>()).prop_shuffle(),
                Just(fan_edges.into_iter().collect::<Vec<_>>()).prop_shuffle(),
            )
        })
        .prop_map(
            |((n, edges, fans, canonical), fan_node_pool, fan_edge_pool)| {
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
                for (k, (parent, children)) in fans.into_iter().enumerate() {
                    let node = if canonical {
                        base_nodes + u32::try_from(k).expect("small")
                    } else {
                        fan_node_pool[k]
                    };
                    graph = fan_node(graph, node);
                    let mut targets = vec![(100 + parent, parent_edge())];
                    targets.extend(children.into_iter().map(|(l, c)| (100 + c, label_edge(&l))));
                    for (target, label) in targets {
                        let id = if canonical {
                            EdgeId(base_edges + u32::try_from(next_edge).expect("small"))
                        } else {
                            EdgeId(fan_edge_pool[next_edge])
                        };
                        next_edge += 1;
                        graph
                            .insert_edge(id, GEdge::new(node, target, label))
                            .expect("ends exist");
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

proptest! {
    #[test]
    fn reify_inverts_unreify_on_the_fan_domain(h in arb_fan_domain()) {
        prop_assert!(FanDomain::contains(&h));
        let (rest, fans) = unreify_fans(&h).expect("in the fan domain");
        let again = reify_fans(&rest, &fans, None).expect("the rest holds no reserved vocabulary");
        let laid_out = reify_layout(&h);
        prop_assert!(GInstance::identical(&again, &laid_out));
        if GInstance::identical(&laid_out, &h) {
            // Laid out as reify_fans lays fans out, the law holds exactly.
            prop_assert!(GInstance::identical(&again, &h));
        }
    }
}
