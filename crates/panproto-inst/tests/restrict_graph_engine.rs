#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The graph restrict engine: node survival with no root, edges by their
//! fate, contraction through dropped nodes with set semantics, fans after
//! contraction, synthesized ids from the floors, and the complement.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::{
    CompiledMigration, EdgeId, Fan, GEdge, GInstance, GraphComplement, IdFloor, Node,
    RestrictError, RestrictPlan, SynthKey, WInstance, graph_restrict_with_plan,
    wtype_restrict_with_plan,
};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};
use proptest::prelude::*;

fn schema(vertices: &[&str], edges: &[&Edge]) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol);
    for v in vertices {
        builder = builder.vertex(v, "record", None).unwrap();
    }
    for edge in edges {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .unwrap();
    }
    builder.build().unwrap()
}

fn e(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn keep(vertices: &[&str]) -> CompiledMigration {
    CompiledMigration {
        surviving_verts: vertices.iter().map(|v| Name::from(*v)).collect(),
        ..CompiledMigration::default()
    }
}

fn restrict(
    instance: &GInstance,
    src: &Schema,
    tgt: &Schema,
    migration: &CompiledMigration,
) -> Result<(GInstance, GraphComplement), RestrictError> {
    let plan = RestrictPlan::compile(src, tgt, migration)?;
    graph_restrict_with_plan(instance, &plan)
}

fn nodes(pairs: &[(u32, &str)]) -> GInstance {
    pairs.iter().fold(GInstance::new(), |g, (id, anchor)| {
        g.with_node(Node::new(*id, *anchor))
    })
}

/// `r -m-> m -leaf-> leaf` with `m` dropped, resolved by the target edge.
fn wrapper() -> (Schema, Schema, CompiledMigration) {
    let src = schema(
        &["r", "m", "leaf"],
        &[&e("r", "m", "m"), &e("m", "leaf", "leaf")],
    );
    let tgt = schema(&["r", "leaf"], &[&e("r", "leaf", "leaf")]);
    (src, tgt, keep(&["r", "leaf"]))
}

#[test]
fn the_plan_engine_leaves_the_complement_unstamped() {
    let (src, tgt, m) = wrapper();
    let g = nodes(&[(0, "r"), (1, "m"), (2, "leaf")])
        .with_edge(0, 1, e("r", "m", "m"))
        .with_edge(1, 2, e("m", "leaf", "leaf"));
    let (_, complement) = restrict(&g, &src, &tgt, &m).unwrap();
    assert!(complement.identity.is_none());
}

#[test]
fn graph_restrict_drops_unmapped_nodes() {
    let src = schema(&["a", "b", "c"], &[]);
    let tgt = schema(&["a_new"], &[]);
    let mut m = keep(&["a_new"]);
    m.vertex_remap.insert("a".into(), "a_new".into());
    let (view, complement) =
        restrict(&nodes(&[(0, "a"), (1, "b"), (2, "c")]), &src, &tgt, &m).unwrap();
    assert_eq!(view.node_count(), 1);
    assert_eq!(view.node(0).unwrap().anchor, "a_new");
    assert_eq!(
        complement.dropped_nodes.keys().copied().collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn graph_restrict_keeps_edge_ids_and_positions() {
    let knows = Edge {
        src: "person".into(),
        tgt: "person".into(),
        kind: "knows".into(),
        name: None,
    };
    let s = schema(&["person"], &[&knows]);
    let mut g = nodes(&[(0, "person"), (1, "person"), (2, "person")])
        .with_edge_id(EdgeId(4), 0, 1, knows.clone())
        .with_edge_id(EdgeId(9), 1, 2, knows);
    g.set_edge_position(EdgeId(9), Some(2)).unwrap();
    let (view, complement) = restrict(&g, &s, &s, &keep(&["person"])).unwrap();
    assert!(GInstance::identical(&view, &g));
    assert!(complement.residue_is_trivial());
}

#[test]
fn a_restricted_graph_allocates_above_the_edges_it_kept() {
    let knows = Edge {
        src: "person".into(),
        tgt: "person".into(),
        kind: "knows".into(),
        name: None,
    };
    let s = schema(&["person"], &[&knows]);
    let g = nodes(&[(0, "person"), (1, "person")]).with_edge_id(EdgeId(6), 0, 1, knows.clone());
    let (mut view, _) = restrict(&g, &s, &s, &keep(&["person"])).unwrap();
    assert_eq!(view.add_edge(1, 0, knows).unwrap(), EdgeId(7));
}

#[test]
fn graph_restrict_keeps_unreachable_survivors() {
    // No root: a surviving node whose only parent is dropped and that no
    // walk from any survivor reaches is kept all the same.
    let src = schema(&["a", "b"], &[&e("a", "b", "ab")]);
    let tgt = schema(&["b"], &[]);
    let g = nodes(&[(0, "a"), (1, "b")]).with_edge(0, 1, e("a", "b", "ab"));
    let (view, _) = restrict(&g, &src, &tgt, &keep(&["b"])).unwrap();
    assert!(view.node(1).is_some());
}

#[test]
fn a_dropped_edge_drops_no_subtree() {
    let src = schema(&["r", "c", "d"], &[&e("r", "c", "c"), &e("c", "d", "d")]);
    let tgt = schema(&["r", "c", "d"], &[&e("c", "d", "d")]);
    let g = nodes(&[(0, "r"), (1, "c"), (2, "d")])
        .with_edge(0, 1, e("r", "c", "c"))
        .with_edge(1, 2, e("c", "d", "d"));
    let (view, complement) = restrict(&g, &src, &tgt, &keep(&["r", "c", "d"])).unwrap();
    assert_eq!(view.node_count(), 3);
    assert_eq!(view.edge_count(), 1);
    assert_eq!(
        complement.dropped_edges.keys().copied().collect::<Vec<_>>(),
        vec![EdgeId(0)]
    );
}

#[test]
fn an_undeclared_edge_label_is_incoherent_between_survivors() {
    let s = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let g = nodes(&[(0, "r"), (1, "c")]).with_edge(0, 1, e("r", "c", "stray"));
    let error = restrict(&g, &s, &s, &keep(&["r", "c"])).unwrap_err();
    assert!(
        matches!(error.unlocated(), RestrictError::IncoherentEdgeImage { source_edge, image }
            if **source_edge == e("r", "c", "stray") && **image == e("r", "c", "stray")),
        "{error:?}"
    );
    assert_eq!(error.key().map(|k| (k.stage, k.id)), Some((1, 0)));
}

#[test]
fn an_undeclared_edge_to_a_dropped_node_is_dropped() {
    let s = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let tgt = schema(&["r"], &[]);
    let g = nodes(&[(0, "r"), (1, "c")]).with_edge(0, 1, e("r", "c", "stray"));
    let (view, complement) = restrict(&g, &s, &tgt, &keep(&["r"])).unwrap();
    assert_eq!(view.edge_count(), 0);
    assert!(complement.dropped_edges.contains_key(&EdgeId(0)));
}

#[test]
fn synthesized_ids_follow_synth_key_order_from_the_floor() {
    // Two contracted pairs and an expansion; node and edge ids come from
    // separate floors in ascending key order.
    let src = schema(
        &["r", "m", "leaf", "b"],
        &[
            &e("r", "m", "m"),
            &e("m", "leaf", "leaf"),
            &e("r", "b", "b"),
        ],
    );
    let tgt = schema(
        &["r", "leaf", "b", "w"],
        &[
            &e("r", "leaf", "leaf"),
            &e("r", "w", "w"),
            &e("w", "b", "b"),
        ],
    );
    let mut m = keep(&["r", "leaf", "b", "w"]);
    m.expansion_path
        .insert(("r".into(), "b".into()), vec!["w".into()]);
    let g = nodes(&[
        (0, "r"),
        (1, "m"),
        (2, "leaf"),
        (3, "m"),
        (4, "leaf"),
        (5, "b"),
    ])
    .with_edge_id(EdgeId(10), 0, 1, e("r", "m", "m"))
    .with_edge_id(EdgeId(11), 1, 2, e("m", "leaf", "leaf"))
    .with_edge_id(EdgeId(12), 0, 3, e("r", "m", "m"))
    .with_edge_id(EdgeId(13), 3, 4, e("m", "leaf", "leaf"))
    .with_edge_id(EdgeId(14), 0, 5, e("r", "b", "b"));
    let (view, complement) = restrict(&g, &src, &tgt, &m).unwrap();
    assert_eq!(complement.id_floor, IdFloor { node: 6, edge: 15 });
    let contract = |t: u32| SynthKey::Contract {
        src: 0,
        tgt: t,
        edge: e("r", "leaf", "leaf"),
    };
    let expand = |hop: u16| SynthKey::Expand {
        source_edge: EdgeId(14),
        hop,
    };
    assert_eq!(
        complement.synthesized_nodes,
        BTreeMap::from([(6, expand(0))])
    );
    assert_eq!(
        complement.synthesized_edges,
        BTreeMap::from([
            (EdgeId(15), contract(2)),
            (EdgeId(16), contract(4)),
            (EdgeId(17), expand(0)),
            (EdgeId(18), expand(1)),
        ])
    );
    for id in complement.synthesized_edges.keys() {
        assert_eq!(view.edge(*id).unwrap().position, None);
    }
}

#[test]
fn synthesized_edges_have_no_position() {
    let (src, tgt, m) = wrapper();
    let mut g = nodes(&[(0, "r"), (1, "m"), (2, "leaf")])
        .with_edge(0, 1, e("r", "m", "m"))
        .with_edge(1, 2, e("m", "leaf", "leaf"));
    g.set_edge_position(EdgeId(1), Some(3)).unwrap();
    let (view, complement) = restrict(&g, &src, &tgt, &m).unwrap();
    for id in complement.synthesized_edges.keys() {
        assert_eq!(view.edge(*id).unwrap().position, None);
    }
}

#[test]
fn expansion_hops_key_intermediates_and_entering_edges() {
    // One source edge expanded through two intermediates: node hops 0 and
    // 1 are the intermediates; edge hop h enters intermediate h, and hop 2
    // enters the endpoint.
    let src = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let tgt = schema(
        &["r", "c", "u", "v"],
        &[&e("r", "u", "u"), &e("u", "v", "v"), &e("v", "c", "c")],
    );
    let mut m = keep(&["r", "c", "u", "v"]);
    m.expansion_path
        .insert(("r".into(), "c".into()), vec!["u".into(), "v".into()]);
    let g = nodes(&[(0, "r"), (1, "c")]).with_edge(0, 1, e("r", "c", "c"));
    let (view, complement) = restrict(&g, &src, &tgt, &m).unwrap();
    let key = |hop| SynthKey::Expand {
        source_edge: EdgeId(0),
        hop,
    };
    let node_of = |hop| complement.synth_node_id(&key(hop)).unwrap();
    assert_eq!(view.node(node_of(0)).unwrap().anchor, "u");
    assert_eq!(view.node(node_of(1)).unwrap().anchor, "v");
    let edge_of = |hop| {
        view.edge(complement.synth_edge_id(&key(hop)).unwrap())
            .unwrap()
            .clone()
    };
    assert_eq!((edge_of(0).src, edge_of(0).tgt), (0, node_of(0)));
    assert_eq!((edge_of(1).src, edge_of(1).tgt), (node_of(0), node_of(1)));
    assert_eq!((edge_of(2).src, edge_of(2).tgt), (node_of(1), 1));
    assert!(
        complement.dropped_edges.contains_key(&EdgeId(0)),
        "the expanded edge is dropped"
    );
}

#[test]
fn an_expanded_source_edge_is_a_dropped_edge() {
    let src = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let tgt = schema(&["r", "c", "u"], &[&e("r", "u", "u"), &e("u", "c", "c")]);
    let mut m = keep(&["r", "c", "u"]);
    m.expansion_path
        .insert(("r".into(), "c".into()), vec!["u".into()]);
    let g = nodes(&[(0, "r"), (1, "c")]).with_edge(0, 1, e("r", "c", "c"));
    let (_, complement) = restrict(&g, &src, &tgt, &m).unwrap();
    assert_eq!(
        complement.dropped_edges[&EdgeId(0)],
        GEdge::new(0, 1, e("r", "c", "c"))
    );
}

#[test]
fn realized_pairs_follow_the_per_source_bfs() {
    let (src, tgt, m) = wrapper();
    let looped = schema(
        &["r", "m", "leaf"],
        &[
            &e("r", "m", "m"),
            &e("m", "leaf", "leaf"),
            &e("m", "m", "self"),
            &e("m", "r", "back"),
        ],
    );
    let tgt_looped = schema(
        &["r", "leaf"],
        &[&e("r", "leaf", "leaf"), &e("r", "r", "rr")],
    );
    let pairs_of = |view: &GInstance, complement: &GraphComplement| -> BTreeSet<(u32, u32)> {
        complement
            .synthesized_edges
            .keys()
            .map(|id| {
                let edge = view.edge(*id).unwrap();
                (edge.src, edge.tgt)
            })
            .collect()
    };
    // A chain r -> m -> m -> leaf.
    let chain = nodes(&[(0, "r"), (1, "m"), (2, "m"), (3, "leaf")])
        .with_edge(0, 1, e("r", "m", "m"))
        .with_edge(1, 2, e("m", "m", "self"))
        .with_edge(2, 3, e("m", "leaf", "leaf"));
    let (view, complement) = restrict(&chain, &looped, &tgt_looped, &m).unwrap();
    assert_eq!(pairs_of(&view, &complement), BTreeSet::from([(0, 3)]));
    // A diamond: two dropped paths to one leaf, joined once.
    let diamond = nodes(&[(0, "r"), (1, "m"), (2, "m"), (3, "leaf")])
        .with_edge(0, 1, e("r", "m", "m"))
        .with_edge(0, 2, e("r", "m", "m"))
        .with_edge(1, 3, e("m", "leaf", "leaf"))
        .with_edge(2, 3, e("m", "leaf", "leaf"));
    let (view, complement) = restrict(&diamond, &src, &tgt, &m).unwrap();
    assert_eq!(pairs_of(&view, &complement), BTreeSet::from([(0, 3)]));
    assert_eq!(complement.synthesized_edges.len(), 1);
    // A cycle back to the source: r -> m -> r is the pair (r, r).
    let cycle = nodes(&[(0, "r"), (1, "m")])
        .with_edge(0, 1, e("r", "m", "m"))
        .with_edge(1, 0, e("m", "r", "back"));
    let (view, complement) = restrict(&cycle, &looped, &tgt_looped, &m).unwrap();
    assert_eq!(pairs_of(&view, &complement), BTreeSet::from([(0, 0)]));
    // A direct edge does not stop a pair through a dropped node.
    let both = nodes(&[(0, "r"), (1, "m"), (2, "leaf")])
        .with_edge(0, 1, e("r", "m", "m"))
        .with_edge(1, 2, e("m", "leaf", "leaf"));
    let (_, complement) = restrict(&both, &src, &tgt, &m).unwrap();
    assert_eq!(complement.synthesized_edges.len(), 1);
}

#[test]
fn a_diamond_contracts_to_one_edge() {
    let (src, tgt, m) = wrapper();
    let diamond = nodes(&[(0, "r"), (1, "m"), (2, "m"), (3, "leaf")])
        .with_edge(0, 1, e("r", "m", "m"))
        .with_edge(0, 2, e("r", "m", "m"))
        .with_edge(1, 3, e("m", "leaf", "leaf"))
        .with_edge(2, 3, e("m", "leaf", "leaf"));
    let (view, complement) = restrict(&diamond, &src, &tgt, &m).unwrap();
    assert_eq!(view.edge_count(), 1);
    assert_eq!(
        complement.synthesized_edges.values().collect::<Vec<_>>(),
        vec![&SynthKey::Contract {
            src: 0,
            tgt: 3,
            edge: e("r", "leaf", "leaf")
        }]
    );
}

#[test]
fn expansion_after_contraction_keys_its_path_by_the_pair() {
    let src = schema(
        &["r", "m", "leaf"],
        &[&e("r", "m", "m"), &e("m", "leaf", "leaf")],
    );
    let tgt = schema(
        &["r", "leaf", "w"],
        &[&e("r", "w", "w"), &e("w", "leaf", "leaf")],
    );
    let mut m = keep(&["r", "leaf", "w"]);
    m.expansion_path
        .insert(("r".into(), "leaf".into()), vec!["w".into()]);
    let g = nodes(&[(0, "r"), (1, "m"), (2, "leaf")])
        .with_edge(0, 1, e("r", "m", "m"))
        .with_edge(1, 2, e("m", "leaf", "leaf"));
    let (view, complement) = restrict(&g, &src, &tgt, &m).unwrap();
    let key = |hop| SynthKey::ContractExpand {
        src: 0,
        tgt: 2,
        hop,
    };
    let w = complement.synth_node_id(&key(0)).unwrap();
    assert_eq!(view.node(w).unwrap().anchor, "w");
    assert!(complement.synth_edge_id(&key(0)).is_some());
    assert!(complement.synth_edge_id(&key(1)).is_some());
}

#[test]
fn unresolved_pairs_fail_alike_in_every_shape() {
    // Two target edges r -> leaf and no resolver: Ambiguous; none: NoEdge.
    let src = schema(
        &["r", "m", "leaf"],
        &[&e("r", "m", "m"), &e("m", "leaf", "leaf")],
    );
    let two = schema(
        &["r", "leaf"],
        &[&e("r", "leaf", "p"), &e("r", "leaf", "q")],
    );
    let none = schema(&["r", "leaf"], &[]);
    let m = keep(&["r", "leaf"]);
    let g = nodes(&[(0, "r"), (1, "m"), (2, "leaf")])
        .with_edge(0, 1, e("r", "m", "m"))
        .with_edge(1, 2, e("m", "leaf", "leaf"));
    let t = WInstance::new(
        [
            (0, Node::new(0, "r")),
            (1, Node::new(1, "m")),
            (2, Node::new(2, "leaf")),
        ]
        .into(),
        vec![(0, 1, e("r", "m", "m")), (1, 2, e("m", "leaf", "leaf"))],
        vec![],
        0,
        "r".into(),
    );
    for (tgt, ambiguous) in [(&two, true), (&none, false)] {
        let plan = RestrictPlan::compile(&src, tgt, &m).unwrap();
        let graph_error = graph_restrict_with_plan(&g, &plan).unwrap_err();
        let tree_error = wtype_restrict_with_plan(&t, &plan).unwrap_err();
        assert_eq!(
            graph_error.key().map(|k| (k.stage, k.id, k.id2)),
            Some((2, 0, 2))
        );
        for error in [graph_error.unlocated(), &tree_error] {
            match (ambiguous, error) {
                (true, RestrictError::AmbiguousEdge { src, tgt, count: 2 })
                | (false, RestrictError::NoEdgeFound { src, tgt }) => {
                    assert_eq!((src.as_str(), tgt.as_str()), ("r", "leaf"));
                }
                (_, other) => panic!("unexpected {other:?}"),
            }
        }
    }
}

#[test]
fn an_incoherent_instance_pair_is_no_edge_found() {
    // The instance joins r to leaf through m by an edge the schema does not
    // declare from m (m -> r would be needed); the pair (r, leaf) is not in
    // the plan of a schema with no walk r -> .. -> leaf.
    let src = schema(
        &["r", "m", "leaf"],
        &[&e("r", "m", "m"), &e("leaf", "m", "back")],
    );
    let tgt = schema(&["r", "leaf"], &[&e("r", "leaf", "leaf")]);
    let m = keep(&["r", "leaf"]);
    let g = nodes(&[(0, "r"), (1, "m"), (2, "leaf")])
        .with_edge(0, 1, e("r", "m", "m"))
        .with_edge(1, 2, e("m", "leaf", "stray"));
    let error = restrict(&g, &src, &tgt, &m).unwrap_err();
    assert!(
        matches!(error.unlocated(), RestrictError::NoEdgeFound { src, tgt } if src == "r" && tgt == "leaf"),
        "{error:?}"
    );
}

#[test]
fn fans_are_decided_after_contraction() {
    // A fan over r with children c (kept) and d (dropped): it survives
    // with its surviving label; its edge to d goes to the complement.
    let s = schema(&["r", "c", "d"], &[&e("r", "c", "c"), &e("r", "d", "d")]);
    let tgt = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let tree = WInstance::new(
        [
            (0, Node::new(0, "r")),
            (1, Node::new(1, "c")),
            (2, Node::new(2, "d")),
        ]
        .into(),
        vec![(0, 1, e("r", "c", "c")), (0, 2, e("r", "d", "d"))],
        vec![Fan {
            hyper_edge_id: "h".into(),
            parent: 0,
            children: HashMap::from([("x".to_owned(), 1), ("y".to_owned(), 2)]),
        }],
        0,
        "r".into(),
    );
    let g = GInstance::from_wtype(&tree).unwrap();
    let m = keep(&["r", "c"]);
    let (view, complement) = restrict(&g, &s, &tgt, &m).unwrap();
    let fan = 3;
    assert!(view.node(fan).is_some(), "the fan node survives");
    let labels: Vec<String> = view
        .out_edges(fan)
        .iter()
        .filter_map(|id| {
            view.edge(*id)
                .and_then(|e| e.edge.name.as_ref().map(ToString::to_string))
        })
        .collect();
    assert_eq!(labels, vec!["x".to_owned()]);
    assert!(
        complement
            .dropped_edges
            .values()
            .any(|e| e.src == fan && e.tgt == 2)
    );
    // The tree keeps the same fan, with the surviving label only.
    let plan = RestrictPlan::compile(&s, &tgt, &m).unwrap();
    let (tree_view, _) = wtype_restrict_with_plan(&tree, &plan).unwrap();
    assert_eq!(tree_view.fans.len(), 1);
    assert_eq!(
        tree_view.fans[0].children,
        HashMap::from([("x".to_owned(), 1)])
    );
}

#[test]
fn graph_and_tree_fan_survival_agree() {
    // A fan whose children are all dropped does not survive, in both shapes.
    let s = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let tgt = schema(&["r"], &[]);
    let tree = WInstance::new(
        [(0, Node::new(0, "r")), (1, Node::new(1, "c"))].into(),
        vec![(0, 1, e("r", "c", "c"))],
        vec![Fan {
            hyper_edge_id: "h".into(),
            parent: 0,
            children: HashMap::from([("x".to_owned(), 1)]),
        }],
        0,
        "r".into(),
    );
    let m = keep(&["r"]);
    let plan = RestrictPlan::compile(&s, &tgt, &m).unwrap();
    let (tree_view, _) = wtype_restrict_with_plan(&tree, &plan).unwrap();
    assert!(tree_view.fans.is_empty());
    let g = GInstance::from_wtype(&tree).unwrap();
    let (view, complement) = graph_restrict_with_plan(&g, &plan).unwrap();
    assert!(view.node(2).is_none());
    assert!(complement.dropped_nodes.contains_key(&2));
}

#[test]
fn the_id_floor_counts_dropped_objects() {
    let s = schema(&["a", "b"], &[&e("a", "b", "ab")]);
    let g = nodes(&[(0, "a"), (7, "b")]).with_edge_id(EdgeId(9), 0, 7, e("a", "b", "ab"));
    let (_, complement) = restrict(&g, &s, &s, &keep(&["a"])).unwrap();
    assert_eq!(complement.id_floor, IdFloor { node: 8, edge: 10 });
}

#[test]
fn an_empty_source_has_floor_zero() {
    let s = schema(&["a"], &[]);
    let (_, complement) = restrict(&GInstance::new(), &s, &s, &keep(&["a"])).unwrap();
    assert_eq!(complement.id_floor, IdFloor { node: 0, edge: 0 });
}

#[test]
fn graph_complement_fields_serialize_in_contract_order() {
    // The fields in the order the encoding writes them.
    let json = serde_json::to_string(&GraphComplement::empty()).unwrap();
    let order = [
        "dropped_nodes",
        "dropped_edges",
        "original_edges",
        "original_anchors",
        "original_extra_fields",
        "original_values",
        "synthesized_nodes",
        "synthesized_edges",
        "id_floor",
        "identity",
    ];
    let positions: Vec<usize> = order
        .iter()
        .map(|key| json.find(&format!("\"{key}\":")).unwrap())
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{json}");
    assert_eq!(json.matches("\":").count(), order.len() + 2, "{json}");
    // A complement with data round-trips.
    let (src, tgt, m) = wrapper();
    let g = nodes(&[(0, "r"), (1, "m"), (2, "leaf")])
        .with_edge(0, 1, e("r", "m", "m"))
        .with_edge(1, 2, e("m", "leaf", "leaf"));
    let (_, complement) = restrict(&g, &src, &tgt, &m).unwrap();
    let back: GraphComplement =
        serde_json::from_str(&serde_json::to_string(&complement).unwrap()).unwrap();
    assert_eq!(back.dropped_edges, complement.dropped_edges);
    assert_eq!(back.synthesized_edges, complement.synthesized_edges);
    assert_eq!(back.id_floor, complement.id_floor);
}

#[test]
fn an_injective_rename_records_no_original_anchor() {
    let src = schema(&["a"], &[]);
    let tgt = schema(&["b"], &[]);
    let mut m = keep(&["b"]);
    m.vertex_remap.insert("a".into(), "b".into());
    let (_, complement) = restrict(&nodes(&[(0, "a")]), &src, &tgt, &m).unwrap();
    assert!(complement.original_anchors.is_empty());
    // A merge records both source anchors.
    let src = schema(&["a", "c"], &[]);
    m.vertex_remap.insert("c".into(), "b".into());
    let (_, complement) = restrict(&nodes(&[(0, "a"), (1, "c")]), &src, &tgt, &m).unwrap();
    assert_eq!(
        complement.original_anchors,
        BTreeMap::from([(0, Name::from("a")), (1, Name::from("c"))])
    );
}

#[test]
fn original_edges_records_exactly_the_edges_that_would_not_map_back() {
    // Two parallel source edges merged onto one target edge: both kept
    // edges are recorded; an unmerged edge is not.
    let src = schema(
        &["r", "a"],
        &[&e("r", "a", "x"), &e("r", "a", "y"), &e("r", "a", "k")],
    );
    let tgt = schema(&["r", "a"], &[&e("r", "a", "z"), &e("r", "a", "k")]);
    let mut m = keep(&["r", "a"]);
    m.edge_remap.insert(e("r", "a", "x"), e("r", "a", "z"));
    m.edge_remap.insert(e("r", "a", "y"), e("r", "a", "z"));
    let g = nodes(&[(0, "r"), (1, "a")])
        .with_edge(0, 1, e("r", "a", "x"))
        .with_edge(0, 1, e("r", "a", "y"))
        .with_edge(0, 1, e("r", "a", "k"));
    let (view, complement) = restrict(&g, &src, &tgt, &m).unwrap();
    assert_eq!(
        complement
            .original_edges
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![EdgeId(0), EdgeId(1)]
    );
    for (id, recorded) in &complement.original_edges {
        let edge = view.edge(*id).unwrap();
        assert_eq!((recorded.src, recorded.tgt), (edge.src, edge.tgt));
    }
}

#[test]
fn graph_residue_ignores_synthesized_maps_floor_and_identity() {
    let (src, tgt, m) = wrapper();
    // Everything survives except through contraction: the residue is the
    // dropped wrapper; the synthesized edge and the floor are no residue.
    let g = nodes(&[(0, "r"), (1, "leaf")]);
    let (_, complement) = restrict(&g, &src, &tgt, &m).unwrap();
    assert!(complement.residue_is_trivial());
    assert!(!complement.is_empty(), "the floor is set");
    let mut synthesized = GraphComplement::empty();
    synthesized.synthesized_nodes.insert(
        4,
        SynthKey::Expand {
            source_edge: EdgeId(0),
            hop: 0,
        },
    );
    synthesized.id_floor = IdFloor { node: 5, edge: 1 };
    assert!(synthesized.residue_is_trivial());
    assert!(!synthesized.is_empty());
    let mut dropped = GraphComplement::empty();
    dropped.dropped_nodes.insert(3, Node::new(3, "m"));
    dropped
        .dropped_edges
        .insert(EdgeId(2), GEdge::new(0, 3, e("r", "m", "m")));
    assert!(!dropped.residue_is_trivial());
    assert_eq!(dropped.reserved_node_ids(), BTreeSet::from([3]));
    assert_eq!(dropped.reserved_edge_ids(), BTreeSet::from([EdgeId(2)]));
}

#[test]
fn synth_lookups_invert_each_other() {
    let src = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let tgt = schema(&["r", "c", "u"], &[&e("r", "u", "u"), &e("u", "c", "c")]);
    let mut m = keep(&["r", "c", "u"]);
    m.expansion_path
        .insert(("r".into(), "c".into()), vec!["u".into()]);
    let g = nodes(&[(0, "r"), (1, "c")]).with_edge(0, 1, e("r", "c", "c"));
    let (_, complement) = restrict(&g, &src, &tgt, &m).unwrap();
    for (id, key) in &complement.synthesized_nodes {
        assert_eq!(complement.synth_node_id(key), Some(*id));
        assert_eq!(complement.synth_key_of_node(*id), Some(key));
    }
    for (id, key) in &complement.synthesized_edges {
        assert_eq!(complement.synth_edge_id(key), Some(*id));
        assert_eq!(complement.synth_key_of_edge(*id), Some(key));
    }
    assert_eq!(complement.synth_key_of_node(0), None);
}

/// A random graph over vertices `v0..v3` with prop edges from a fixed pool,
/// and a random survival and edge remap.
fn arb_case() -> impl Strategy<Value = (GInstance, Schema, Schema, CompiledMigration)> {
    let names = ["v0", "v1", "v2", "v3"];
    let pool: Vec<Edge> = (0..4)
        .flat_map(|u| (0..4).map(move |v| (u, v)))
        .map(|(u, v)| e(names[u], names[v], &format!("e{u}{v}")))
        .collect();
    (
        prop::collection::vec(0usize..4, 0..6),
        prop::collection::vec((0usize..6, 0usize..6, 0usize..16), 0..10),
        prop::collection::vec(any::<bool>(), 4),
        prop::collection::vec(any::<bool>(), 16),
    )
        .prop_map(move |(anchors, edges, survive, declared)| {
            let mut g = GInstance::new();
            for (id, anchor) in anchors.iter().enumerate() {
                g = g.with_node(Node::new(u32::try_from(id).unwrap(), names[*anchor]));
            }
            let declared_edges: Vec<&Edge> = pool
                .iter()
                .zip(&declared)
                .filter(|(_, d)| **d)
                .map(|(e, _)| e)
                .collect();
            let src = schema(&names, &declared_edges);
            for (s, t, label) in edges {
                if s < anchors.len() && t < anchors.len() {
                    // The label is the pool edge between the endpoints'
                    // anchors; `label` picks no other edge, since an
                    // undeclared label is refused, not restricted.
                    let _ = label;
                    let edge = e(
                        names[anchors[s]],
                        names[anchors[t]],
                        &format!("e{}{}", anchors[s], anchors[t]),
                    );
                    if src.edges.contains_key(&edge) {
                        g = g.with_edge(u32::try_from(s).unwrap(), u32::try_from(t).unwrap(), edge);
                    }
                }
            }
            let surviving: Vec<&str> = names
                .iter()
                .zip(&survive)
                .filter(|(_, s)| **s)
                .map(|(n, _)| *n)
                .collect();
            let tgt = schema(&names, &declared_edges);
            (g, src, tgt, keep(&surviving))
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Stage 1 identifies no two elements: distinct kept source nodes stay
    /// distinct view nodes under their own ids, and distinct kept edges
    /// stay distinct view edges under their own ids, each joining the same
    /// endpoints it joined in the source.
    #[test]
    fn a_coherent_edge_image_never_identifies_elements((g, src, tgt, m) in arb_case()) {
        let Ok((view, complement)) = restrict(&g, &src, &tgt, &m) else {
            return Ok(());
        };
        for (id, node) in view.nodes() {
            if complement.synthesized_nodes.contains_key(id) {
                continue;
            }
            prop_assert_eq!(node.id, *id);
            prop_assert!(g.node(*id).is_some());
        }
        for (id, edge) in view.edges() {
            if complement.synthesized_edges.contains_key(id) {
                continue;
            }
            let source = g.edge(*id).unwrap();
            prop_assert_eq!((edge.src, edge.tgt), (source.src, source.tgt));
        }
        // Every source element is accounted for exactly once.
        let kept_nodes = view.node_count() - complement.synthesized_nodes.len();
        prop_assert_eq!(kept_nodes + complement.dropped_nodes.len(), g.node_count());
        let kept_edges = view.edge_count() - complement.synthesized_edges.len();
        prop_assert_eq!(kept_edges + complement.dropped_edges.len(), g.edge_count());
    }
}

#[test]
fn a_fan_relabel_that_merges_children_is_refused_in_both_shapes() {
    // The resolver sends both labels of the fan over r to `z`: the tree may
    // not keep one child and lose the other, so both shapes refuse.
    let s = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let tree = WInstance::new(
        [
            (0, Node::new(0, "r")),
            (1, Node::new(1, "c")),
            (2, Node::new(2, "c")),
        ]
        .into(),
        vec![(0, 1, e("r", "c", "c")), (0, 2, e("r", "c", "c"))],
        vec![Fan {
            hyper_edge_id: "h".into(),
            parent: 0,
            children: HashMap::from([("x".to_owned(), 1), ("y".to_owned(), 2)]),
        }],
        0,
        "r".into(),
    );
    let mut m = keep(&["r", "c"]);
    m.hyper_resolver.insert(
        ("h".into(), vec!["x".into(), "y".into()]),
        (
            "h".into(),
            HashMap::from([("x".into(), "z".into()), ("y".into(), "z".into())]),
        ),
    );
    let plan = RestrictPlan::compile(&s, &s, &m).unwrap();
    let refused = |err: &RestrictError| {
        matches!(
            err.unlocated(),
            RestrictError::FanReconstructionFailed { hyper_edge_id, .. } if hyper_edge_id == "h"
        )
    };
    let tree_err = wtype_restrict_with_plan(&tree, &plan).unwrap_err();
    assert!(refused(&tree_err), "tree: {tree_err:?}");
    let g = GInstance::from_wtype(&tree).unwrap();
    let graph_err = graph_restrict_with_plan(&g, &plan).unwrap_err();
    assert!(refused(&graph_err), "graph: {graph_err:?}");
    // Only the graph engine locates its errors: the graph error is keyed at
    // the fan node of the embedded graph, the tree error carries no key, and
    // the two agree once the location is removed.
    assert_eq!(tree_err.key(), None);
    assert!(
        !matches!(tree_err, RestrictError::At { .. }),
        "{tree_err:?}"
    );
    assert_eq!(graph_err.key().map(|k| (k.stage, k.id)), Some((3, 3)));
    assert_eq!(
        format!("{tree_err:?}"),
        format!("{:?}", graph_err.unlocated())
    );
}

/// `r(0)` with two `c` children, 1 and 2, under the arc `c`, and the fan
/// of hyper-edge `h` from `r` to them under the labels `x` and `y`.
fn fanned_tree() -> WInstance {
    WInstance::new(
        [
            (0, Node::new(0, "r")),
            (1, Node::new(1, "c")),
            (2, Node::new(2, "c")),
        ]
        .into(),
        vec![(0, 1, e("r", "c", "c")), (0, 2, e("r", "c", "c"))],
        vec![Fan {
            hyper_edge_id: "h".into(),
            parent: 0,
            children: HashMap::from([("x".to_owned(), 1), ("y".to_owned(), 2)]),
        }],
        0,
        "r".into(),
    )
}

/// `r -c-> c` with the hyper-edges `hyper` (each `(id, parent label)`),
/// each with the parent label on `r` and the child labels `x`, `y` and
/// `z` on `c`.
fn fanned_schema(hyper: &[(&str, &str)]) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex("r", "record", None)
        .unwrap()
        .vertex("c", "record", None)
        .unwrap()
        .edge("r", "c", "prop", Some("c"))
        .unwrap();
    for (id, parent) in hyper {
        let sig = HashMap::from([
            ((*parent).to_owned(), "r".to_owned()),
            ("x".to_owned(), "c".to_owned()),
            ("y".to_owned(), "c".to_owned()),
            ("z".to_owned(), "c".to_owned()),
        ]);
        builder = builder.hyper_edge(id, "rel", sig, parent).unwrap();
    }
    builder.build().unwrap()
}

/// The fan edges of `g`, by id.
fn fan_edges(g: &GInstance) -> BTreeMap<EdgeId, GEdge> {
    g.edges_sorted()
        .filter(|(_, edge)| edge.edge.kind.starts_with("hyper_"))
        .map(|(id, edge)| (id, edge.clone()))
        .collect()
}

/// The id of the fan edge of `edges` under the label `kind` and `name`.
fn fan_edge(edges: &BTreeMap<EdgeId, GEdge>, kind: &str, name: Option<&str>) -> EdgeId {
    edges
        .iter()
        .find(|(_, e)| e.edge.kind == kind && e.edge.name.as_deref() == name)
        .map(|(id, _)| *id)
        .unwrap()
}

#[test]
fn a_kept_fan_edge_is_recorded_only_when_its_label_does_not_map_back() {
    // A view fan edge maps back to the label of the source anchors of its
    // endpoints (A(f), A(t)) under its own label, or, for the parent edge,
    // the source parent label of A(f) (Arbiter ruling A12 (3)); it is
    // recorded exactly when that is not its source label. A fan node is
    // recorded when the reverse vertex map does not return its source
    // anchor, as every node is.
    let s = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let g = GInstance::from_wtype(&fanned_tree()).unwrap();
    let edges = fan_edges(&g);
    assert_eq!(edges.len(), 3, "{edges:?}");
    let fan_node = edges.values().next().unwrap().src;

    // Unchanged: no fan edge and not the fan node is recorded.
    let plan = RestrictPlan::compile(&s, &s, &keep(&["r", "c"])).unwrap();
    let (view, complement) = graph_restrict_with_plan(&g, &plan).unwrap();
    for (id, source) in &edges {
        assert_eq!(view.edge(*id), Some(source));
    }
    assert_eq!(complement.original_edges, BTreeMap::new());
    assert!(!complement.original_anchors.contains_key(&fan_node));
    assert!(complement.residue_is_trivial(), "{complement:?}");

    // Retargeted to another hyper-edge, with `x` relabelled `x2`: the fan
    // node is recorded, and so is the edge of `x`, whose view label maps
    // back to `x2` on `h`; `y` and the parent map back to their labels.
    let mut m = keep(&["r", "c"]);
    m.hyper_resolver.insert(
        ("h".into(), vec!["x".into(), "y".into()]),
        (
            "h2".into(),
            HashMap::from([("x".into(), "x2".into()), ("y".into(), "y".into())]),
        ),
    );
    let plan = RestrictPlan::compile(&s, &s, &m).unwrap();
    let (_, complement) = graph_restrict_with_plan(&g, &plan).unwrap();
    let x = fan_edge(&edges, "hyper_label", Some("x"));
    assert_eq!(
        complement.original_edges,
        BTreeMap::from([(x, edges[&x].clone())])
    );
    assert_eq!(
        complement.original_anchors.get(&fan_node),
        Some(&Name::from("h"))
    );
}

#[test]
fn a_relabelled_fan_records_its_relabelled_edges() {
    let tree = fanned_tree();
    // Relabelled in place, `x` to `z`: the edge of `x` is recorded, with
    // its source label, and the edge of `y` and the parent edge are not.
    let s = fanned_schema(&[("h", "p")]);
    let g = GInstance::from_wtype_with_schema(&tree, &s).unwrap();
    let edges = fan_edges(&g);
    let x = fan_edge(&edges, "hyper_label", Some("x"));
    let mut m = keep(&["r", "c"]);
    m.hyper_resolver.insert(
        ("h".into(), vec!["x".into(), "y".into()]),
        (
            "h".into(),
            HashMap::from([("x".into(), "z".into()), ("y".into(), "y".into())]),
        ),
    );
    let plan = RestrictPlan::compile(&s, &s, &m).unwrap();
    let (view, complement) = graph_restrict_with_plan(&g, &plan).unwrap();
    assert_eq!(
        view.edge(x).unwrap().edge.name.as_deref(),
        Some("z"),
        "{view:?}"
    );
    assert_eq!(
        complement.original_edges,
        BTreeMap::from([(x, edges[&x].clone())])
    );
    assert!(complement.original_anchors.is_empty(), "{complement:?}");

    // Retargeted to `h2`, whose parent label is `q`, with the child labels
    // kept: the view's parent edge is named `q`, which maps back to `p`
    // through the source hyper-edge `h`, so no fan edge is recorded, and
    // only the fan node's anchor is.
    let both = fanned_schema(&[("h", "p"), ("h2", "q")]);
    let g = GInstance::from_wtype_with_schema(&tree, &both).unwrap();
    let edges = fan_edges(&g);
    let parent = fan_edge(&edges, "hyper_parent", Some("p"));
    let fan_node = edges[&parent].src;
    let mut m = keep(&["r", "c"]);
    m.hyper_resolver.insert(
        ("h".into(), vec!["x".into(), "y".into()]),
        (
            "h2".into(),
            HashMap::from([("x".into(), "x".into()), ("y".into(), "y".into())]),
        ),
    );
    let plan = RestrictPlan::compile(&both, &both, &m).unwrap();
    let (view, complement) = graph_restrict_with_plan(&g, &plan).unwrap();
    assert_eq!(
        view.edge(parent).unwrap().edge.name.as_deref(),
        Some("q"),
        "{view:?}"
    );
    assert_eq!(complement.original_edges, BTreeMap::new());
    assert_eq!(
        complement.original_anchors,
        BTreeMap::from([(fan_node, Name::from("h"))])
    );
}
