#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The tree restrict engine: arc fates from the plan, synthesized ids in key
//! order from the floor and never at `u32::MAX`, kept arcs before synthesized
//! first hops, and the refusals the plan compilers and predicates make. Each
//! case that has a graph counterpart is run on the tree's graph form too.

use std::collections::HashMap;

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::Name;
use panproto_inst::{
    CompiledMigration, EdgeId, GInstance, Node, RestrictError, RestrictPlan, SynthKey,
    TreeSynthesis, WInstance, graph_restrict_with_plan, wtype_restrict_with_plan,
};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

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

fn tree(nodes: &[(u32, &str)], arcs: &[(u32, u32, Edge)]) -> WInstance {
    let nodes: HashMap<u32, Node> = nodes.iter().map(|(i, a)| (*i, Node::new(*i, *a))).collect();
    WInstance::new(nodes, arcs.to_vec(), vec![], 0, "r".into())
}

fn tree_restrict(
    instance: &WInstance,
    src: &Schema,
    tgt: &Schema,
    migration: &CompiledMigration,
) -> Result<(WInstance, TreeSynthesis), RestrictError> {
    let plan = RestrictPlan::compile(src, tgt, migration)?;
    wtype_restrict_with_plan(instance, &plan)
}

fn graph_restrict(
    instance: &GInstance,
    src: &Schema,
    tgt: &Schema,
    migration: &CompiledMigration,
) -> Result<(GInstance, panproto_inst::GraphComplement), RestrictError> {
    let plan = RestrictPlan::compile(src, tgt, migration)?;
    graph_restrict_with_plan(instance, &plan)
}

fn children(tree: &WInstance, node: u32) -> Vec<u32> {
    tree.arcs
        .iter()
        .filter(|(p, _, _)| *p == node)
        .map(|(_, c, _)| *c)
        .collect()
}

/// The children of `node` in a graph view, in `ordered_out_edges` order.
fn ordered_children(graph: &GInstance, node: u32) -> Vec<u32> {
    graph
        .ordered_out_edges(node)
        .into_iter()
        .map(|id| graph.edge(id).unwrap().tgt)
        .collect()
}

/// `r -m-> m -leaf-> leaf`, the target `r -leaf-> leaf`, `m` dropped.
fn contracting() -> (Schema, Schema, CompiledMigration) {
    let src = schema(
        &["r", "m", "leaf"],
        &[&e("r", "m", "m"), &e("m", "leaf", "leaf")],
    );
    let tgt = schema(&["r", "leaf"], &[&e("r", "leaf", "leaf")]);
    (src, tgt, keep(&["r", "leaf"]))
}

#[test]
fn tree_sibling_order_matches_ordered_out_edges() {
    let (src, tgt, m) = contracting();
    let inst = tree(
        &[(0, "r"), (1, "m"), (2, "m"), (3, "leaf"), (4, "leaf")],
        &[
            (0, 1, e("r", "m", "m")),
            (1, 4, e("m", "leaf", "leaf")),
            (0, 2, e("r", "m", "m")),
            (2, 3, e("m", "leaf", "leaf")),
        ],
    );
    let (view, _) = tree_restrict(&inst, &src, &tgt, &m).unwrap();
    assert_eq!(children(&view, 0), vec![3, 4]);
    let graph = GInstance::from_wtype(&inst).unwrap();
    let (graph_view, _) = graph_restrict(&graph, &src, &tgt, &m).unwrap();
    assert_eq!(children(&view, 0), ordered_children(&graph_view, 0));
}

#[test]
fn an_expand_between_kept_siblings_follows_them() {
    // r has three children in arc order: a (kept), b (expanded through w),
    // c (kept). The expanded arc's first hop comes after both kept arcs.
    let src = schema(
        &["r", "a", "b", "c"],
        &[&e("r", "a", "a"), &e("r", "b", "b"), &e("r", "c", "c")],
    );
    let tgt = schema(
        &["r", "a", "b", "c", "w"],
        &[
            &e("r", "a", "a"),
            &e("r", "w", "w"),
            &e("w", "b", "b"),
            &e("r", "c", "c"),
        ],
    );
    let mut m = keep(&["r", "a", "b", "c", "w"]);
    m.expansion_path
        .insert(("r".into(), "b".into()), vec!["w".into()]);
    let inst = tree(
        &[(0, "r"), (1, "a"), (2, "b"), (3, "c")],
        &[
            (0, 1, e("r", "a", "a")),
            (0, 2, e("r", "b", "b")),
            (0, 3, e("r", "c", "c")),
        ],
    );
    let (view, synthesis) = tree_restrict(&inst, &src, &tgt, &m).unwrap();
    assert_eq!(children(&view, 0), vec![1, 3, 4]);
    assert_eq!(view.parent(2), Some(4));
    assert_eq!(
        synthesis.synthesized_keys.get(&4),
        Some(&SynthKey::Expand {
            source_edge: EdgeId(1),
            hop: 0
        })
    );
    let graph = GInstance::from_wtype(&inst).unwrap();
    let (graph_view, _) = graph_restrict(&graph, &src, &tgt, &m).unwrap();
    assert_eq!(children(&view, 0), ordered_children(&graph_view, 0));
}

#[test]
fn expansion_ids_follow_synth_key_order() {
    let src = schema(
        &["r", "a", "b"],
        &[&e("r", "a", "a"), &e("a", "b", "b"), &e("r", "b", "c")],
    );
    let tgt = schema(
        &["r", "a", "b", "w"],
        &[
            &e("r", "a", "a"),
            &e("a", "w", "w"),
            &e("w", "b", "b"),
            &e("r", "w", "w"),
        ],
    );
    // Arc 0 is a -> b (deeper), arc 2 is r -> b; both expand through w.
    // Breadth-first order meets arc 2 first, key order arc 0.
    let inst = tree(
        &[(0, "r"), (1, "a"), (2, "b"), (3, "b")],
        &[
            (1, 2, e("a", "b", "b")),
            (0, 1, e("r", "a", "a")),
            (0, 3, e("r", "b", "c")),
        ],
    );
    let mut m = keep(&["r", "a", "b", "w"]);
    m.expansion_path
        .insert(("a".into(), "b".into()), vec!["w".into()]);
    m.expansion_path
        .insert(("r".into(), "b".into()), vec!["w".into()]);
    let (view, synthesis) = tree_restrict(&inst, &src, &tgt, &m).unwrap();
    assert_eq!(synthesis.id_floor, 4);
    assert_eq!(
        view.parent(2),
        Some(4),
        "arc 0's intermediate takes the floor"
    );
    assert_eq!(view.parent(3), Some(5));
    assert_eq!(
        synthesis.synthesized_keys,
        [
            (
                4,
                SynthKey::Expand {
                    source_edge: EdgeId(0),
                    hop: 0
                }
            ),
            (
                5,
                SynthKey::Expand {
                    source_edge: EdgeId(2),
                    hop: 0
                }
            ),
        ]
        .into()
    );
}

#[test]
fn expansion_ids_clear_fan_ids() {
    // A source with a fan (which takes node id 3 in the graph form) and one
    // expansion: the synthesized node takes 4, above the fan node, so the
    // tree and the graph allocate the same id without renumbering.
    let src = schema(&["r", "a", "b"], &[&e("r", "a", "a"), &e("r", "b", "b")]);
    let tgt = schema(
        &["r", "a", "b", "w"],
        &[&e("r", "a", "a"), &e("r", "w", "w"), &e("w", "b", "b")],
    );
    let mut m = keep(&["r", "a", "b", "w"]);
    m.expansion_path
        .insert(("r".into(), "b".into()), vec!["w".into()]);
    let nodes: HashMap<u32, Node> = [(0, "r"), (1, "a"), (2, "b")]
        .iter()
        .map(|(i, a)| (*i, Node::new(*i, *a)))
        .collect();
    let fan = panproto_inst::Fan {
        hyper_edge_id: "h".into(),
        parent: 0,
        children: HashMap::from([("x".to_owned(), 1)]),
    };
    let inst = WInstance::new(
        nodes,
        vec![(0, 1, e("r", "a", "a")), (0, 2, e("r", "b", "b"))],
        vec![fan],
        0,
        "r".into(),
    );
    let (view, synthesis) = tree_restrict(&inst, &src, &tgt, &m).unwrap();
    assert_eq!(synthesis.id_floor, 4);
    assert_eq!(view.parent(2), Some(4));
    let (graph, embedding) = GInstance::from_wtype_embedded(&inst, None).unwrap();
    assert_eq!(embedding.next_node, synthesis.id_floor);
    let (graph_view, complement) = graph_restrict(&graph, &src, &tgt, &m).unwrap();
    assert_eq!(complement.id_floor.node, synthesis.id_floor);
    assert_eq!(complement.synthesized_nodes, synthesis.synthesized_keys);
    assert!(graph_view.node(4).is_some_and(|n| n.anchor == "w"));
}

#[test]
fn tree_expand_keys_name_the_source_arc_through_the_embedding() {
    let src = schema(&["r", "a", "b"], &[&e("r", "a", "a"), &e("r", "b", "b")]);
    let tgt = schema(
        &["r", "a", "b", "w"],
        &[&e("r", "a", "a"), &e("r", "w", "w"), &e("w", "b", "b")],
    );
    let mut m = keep(&["r", "a", "b", "w"]);
    m.expansion_path
        .insert(("r".into(), "b".into()), vec!["w".into()]);
    let inst = tree(
        &[(0, "r"), (1, "a"), (2, "b")],
        &[(0, 2, e("r", "b", "b")), (0, 1, e("r", "a", "a"))],
    );
    let (_, synthesis) = tree_restrict(&inst, &src, &tgt, &m).unwrap();
    let (_, embedding) = GInstance::from_wtype_embedded(&inst, None).unwrap();
    let arc = embedding.arcs[&2];
    assert_eq!(
        synthesis.synthesized_keys.values().next(),
        Some(&SynthKey::Expand {
            source_edge: arc,
            hop: 0
        })
    );
}

/// The probe of the top of the id range: source nodes 0 (r), 1 (leaf2) and
/// `u32::MAX` (leaf), and one expansion through `mid`.
fn top_of_range() -> (WInstance, Schema, Schema, CompiledMigration) {
    let src = schema(
        &["r", "leaf", "leaf2"],
        &[&e("r", "leaf", "x"), &e("r", "leaf2", "y")],
    );
    let tgt = schema(
        &["r", "leaf", "leaf2", "mid"],
        &[
            &e("r", "mid", "p"),
            &e("mid", "leaf", "x"),
            &e("r", "leaf2", "y"),
        ],
    );
    let inst = tree(
        &[(0, "r"), (1, "leaf2"), (u32::MAX, "leaf")],
        &[
            (0, u32::MAX, e("r", "leaf", "x")),
            (0, 1, e("r", "leaf2", "y")),
        ],
    );
    let mut m = keep(&["r", "leaf", "leaf2", "mid"]);
    m.expansion_path
        .insert(("r".into(), "leaf".into()), vec!["mid".into()]);
    (inst, src, tgt, m)
}

#[test]
fn a_synthesized_node_never_takes_a_source_id_at_the_top_of_the_range() {
    let (inst, src, tgt, m) = top_of_range();
    let error = tree_restrict(&inst, &src, &tgt, &m).unwrap_err();
    assert!(
        matches!(
            error,
            RestrictError::IdsExhausted {
                kind: panproto_inst::segment::IdKind::Node
            }
        ),
        "{error:?}"
    );
    // The graph twin: the graph form of the same tree, located at stage 5.
    let graph = GInstance::new()
        .with_node(Node::new(0, "r"))
        .with_node(Node::new(1, "leaf2"))
        .with_node(Node::new(u32::MAX, "leaf"))
        .with_edge_id(EdgeId(0), 0, u32::MAX, e("r", "leaf", "x"))
        .with_edge_id(EdgeId(1), 0, 1, e("r", "leaf2", "y"));
    let error = graph_restrict(&graph, &src, &tgt, &m).unwrap_err();
    assert!(
        matches!(
            error.unlocated(),
            RestrictError::IdsExhausted {
                kind: panproto_inst::segment::IdKind::Node
            }
        ),
        "{error:?}"
    );
    assert_eq!(error.key().map(|k| (k.stage, k.id)), Some((5, 0)));
}

#[test]
fn a_source_holding_u32_max_records_the_floor_u32_max() {
    // Tree: the floor saturates.
    let src = schema(&["r", "leaf"], &[&e("r", "leaf", "x")]);
    let inst = tree(
        &[(0, "r"), (u32::MAX, "leaf")],
        &[(0, u32::MAX, e("r", "leaf", "x"))],
    );
    let (_, synthesis) = tree_restrict(&inst, &src, &src, &keep(&["r", "leaf"])).unwrap();
    assert_eq!(synthesis.id_floor, u32::MAX);
    // Graph: both floors saturate.
    let graph = GInstance::new()
        .with_node(Node::new(0, "r"))
        .with_node(Node::new(u32::MAX, "leaf"))
        .with_edge_id(EdgeId(u32::MAX), 0, u32::MAX, e("r", "leaf", "x"));
    let (_, complement) = graph_restrict(&graph, &src, &src, &keep(&["r", "leaf"])).unwrap();
    assert_eq!(complement.id_floor.node, u32::MAX);
    assert_eq!(complement.id_floor.edge, u32::MAX);
    // Table: the row floor saturates.
    let protocol = Protocol {
        value_kinds: std::collections::BTreeMap::from([(
            "int".to_owned(),
            panproto_schema::ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        )]),
        non_value_kinds: std::collections::BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    };
    let table = SchemaBuilder::new(&protocol)
        .vertex("t", "record", None)
        .unwrap()
        .vertex("t.a", "int", None)
        .unwrap()
        .edge("t", "t.a", "prop", Some("a"))
        .unwrap()
        .build()
        .unwrap();
    let rows = panproto_inst::FInstance::new()
        .with_rows(
            "t",
            [
                (
                    panproto_inst::RowId(0),
                    HashMap::from([("a".to_owned(), panproto_inst::Value::Int(1))]),
                ),
                (
                    panproto_inst::RowId(u32::MAX),
                    HashMap::from([("a".to_owned(), panproto_inst::Value::Int(2))]),
                ),
            ],
        )
        .unwrap();
    let identity = CompiledMigration {
        surviving_edges: table.edges.keys().cloned().collect(),
        ..keep(&["t", "t.a"])
    };
    let plan = RestrictPlan::compile_for_tables(
        &table,
        &table,
        &identity,
        panproto_inst::BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        },
    )
    .unwrap();
    let (view, complement) = panproto_inst::functor_restrict_with_plan(&rows, &plan).unwrap();
    assert_eq!(view.row_count("t"), 2);
    assert_eq!(complement.id_floor, u32::MAX);
}

#[test]
fn an_incoherent_edge_image_is_rejected_in_every_shape() {
    let src = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let tgt = schema(&["r", "c", "z"], &[&e("r", "c", "c"), &e("r", "z", "z")]);
    let mut m = keep(&["r", "c", "z"]);
    m.edge_remap.insert(e("r", "c", "c"), e("r", "z", "z"));
    let error = RestrictPlan::compile(&src, &tgt, &m).unwrap_err();
    assert!(
        matches!(
            &error,
            RestrictError::IncoherentEdgeImage { source_edge, image }
                if **source_edge == e("r", "c", "c") && **image == e("r", "z", "z")
        ),
        "{error:?}"
    );
    // Every engine compiles this plan first, so every shape refuses it.
    let inst = tree(&[(0, "r"), (1, "c")], &[(0, 1, e("r", "c", "c"))]);
    assert!(tree_restrict(&inst, &src, &tgt, &m).is_err());
    let graph = GInstance::from_wtype(&inst).unwrap();
    assert!(graph_restrict(&graph, &src, &tgt, &m).is_err());
}

#[test]
fn an_incoherent_resolver_value_for_a_contracted_pair_is_rejected_in_every_shape() {
    let src = schema(
        &["r", "m", "leaf"],
        &[&e("r", "m", "m"), &e("m", "leaf", "leaf")],
    );
    let tgt = schema(
        &["r", "leaf", "x", "y"],
        &[&e("r", "leaf", "leaf"), &e("x", "y", "z")],
    );
    let mut m = keep(&["r", "leaf"]);
    m.resolver
        .insert(("r".into(), "leaf".into()), e("x", "y", "z"));
    let error = RestrictPlan::compile(&src, &tgt, &m).unwrap_err();
    assert!(
        matches!(
            &error,
            RestrictError::IncoherentPairImage { pair, image }
                if **pair == (Name::from("r"), Name::from("leaf")) && **image == e("x", "y", "z")
        ),
        "{error:?}"
    );
    let inst = tree(
        &[(0, "r"), (1, "m"), (2, "leaf")],
        &[(0, 1, e("r", "m", "m")), (1, 2, e("m", "leaf", "leaf"))],
    );
    assert!(tree_restrict(&inst, &src, &tgt, &m).is_err());
    let graph = GInstance::from_wtype(&inst).unwrap();
    assert!(graph_restrict(&graph, &src, &tgt, &m).is_err());
}

fn gt(field: &str, n: i64) -> Expr {
    Expr::Builtin(
        BuiltinOp::Gt,
        vec![Expr::Var(field.into()), Expr::Lit(Literal::Int(n))],
    )
}

#[test]
fn a_survival_predicate_that_fails_to_evaluate_is_refused_in_every_shape() {
    let src = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let inst = tree(&[(0, "r"), (1, "c")], &[(0, 1, e("r", "c", "c"))]);
    let mut m = keep(&["r", "c"]);
    m.add_conditional_survival("c", gt("nope", 1));
    let error = tree_restrict(&inst, &src, &src, &m).unwrap_err();
    assert!(
        matches!(
            &error,
            RestrictError::SurvivalPredicateFailed {
                node: 1,
                source: panproto_expr::ExprError::UnboundVariable(v),
            } if v == "nope"
        ),
        "{error:?}"
    );
    let graph = GInstance::from_wtype(&inst).unwrap();
    let error = graph_restrict(&graph, &src, &src, &m).unwrap_err();
    assert!(
        matches!(
            error.unlocated(),
            RestrictError::SurvivalPredicateFailed { node: 1, .. }
        ),
        "{error:?}"
    );
    assert_eq!(error.key().map(|k| (k.stage, k.id)), Some((0, 1)));
}

#[test]
fn a_non_boolean_survival_predicate_is_refused() {
    let src = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let inst = tree(&[(0, "r"), (1, "c")], &[(0, 1, e("r", "c", "c"))]);
    let mut m = keep(&["r", "c"]);
    m.add_conditional_survival("c", Expr::Lit(Literal::Int(7)));
    let error = tree_restrict(&inst, &src, &src, &m).unwrap_err();
    assert!(
        matches!(
            &error,
            RestrictError::SurvivalPredicateFailed {
                node: 1,
                source: panproto_expr::ExprError::TypeError { expected, got }
            } if expected == "bool" && got == "int"
        ),
        "{error:?}"
    );
    // A false predicate drops the node; a true one keeps it.
    let mut drop = keep(&["r", "c"]);
    drop.add_conditional_survival("c", Expr::Lit(Literal::Bool(false)));
    let (view, _) = tree_restrict(&inst, &src, &src, &drop).unwrap();
    assert_eq!(view.node_count(), 1);
    let mut retain = keep(&["r", "c"]);
    retain.add_conditional_survival("c", Expr::Lit(Literal::Bool(true)));
    let (view, _) = tree_restrict(&inst, &src, &src, &retain).unwrap();
    assert_eq!(view.node_count(), 2);
}

#[test]
fn a_dropped_arc_drops_its_subtree_in_a_tree_but_its_edge_alone_in_a_graph() {
    // An arc whose label has no image is dropped: the tree drops its
    // subtree, the graph only the edge.
    let src = schema(&["r", "c", "d"], &[&e("r", "c", "c"), &e("c", "d", "d")]);
    let tgt = schema(&["r", "c", "d"], &[&e("c", "d", "d")]);
    let inst = tree(
        &[(0, "r"), (1, "c"), (2, "d")],
        &[(0, 1, e("r", "c", "c")), (1, 2, e("c", "d", "d"))],
    );
    let m = keep(&["r", "c", "d"]);
    let (view, _) = tree_restrict(&inst, &src, &tgt, &m).unwrap();
    assert_eq!(view.node_count(), 1);
    let graph = GInstance::from_wtype(&inst).unwrap();
    let (graph_view, complement) = graph_restrict(&graph, &src, &tgt, &m).unwrap();
    assert_eq!(graph_view.node_count(), 3);
    assert_eq!(graph_view.edge_count(), 1);
    assert_eq!(
        complement.dropped_edges.keys().copied().collect::<Vec<_>>(),
        vec![EdgeId(0)]
    );
}
