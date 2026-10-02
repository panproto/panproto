//! Cross-engine agreement of restrict on graphs.
//!
//! A graph input always runs the graph engine. The squares below check the
//! other two engines against it: on a tree-shaped graph, converting the
//! input to a tree, running the tree engine and converting the view back
//! gives the graph engine's view under the same ids and child order; on a
//! table-shaped graph, the same holds through the table engine. The legs
//! that convert live here, so no restrict entry point converts its input to
//! choose an engine.

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

use panproto_gat::Name;

use super::{
    EdgeId, GEdge, GInstance, IdKind, Node, RestrictPlan, SynthKey, allocate_synthesized,
    floor_above, graph_restrict_with_plan, is_fan_node,
};
use crate::functor::FInstance;
use crate::ginstance::TableEmbedding;
use crate::wtype::WInstance;
use crate::{
    BoundaryProtocols, CompiledMigration, RestrictError, Value, graph_restrict,
    graph_restrict_with_complement,
};

thread_local! {
    /// The legs run on this thread, so a test can tell that a restrict
    /// entry point ran none.
    static CROSS_ENGINE_RUNS: Cell<usize> = const { Cell::new(0) };
}

/// The result of a test whose setup can fail.
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// A setup failure inside a property, as the failure of its case.
fn fail(error: impl std::fmt::Display) -> TestCaseError {
    TestCaseError::fail(error.to_string())
}

/// The table leg of the table square, or `None` when it declines: the
/// input converted to a table, the table engine, and its view converted
/// back.
///
/// The view is the one [`graph_restrict_with_plan`] gives, computed by the
/// table engine ([`functor_restrict_with_plan`](crate::functor_restrict_with_plan))
/// on the table the input is ([`GInstance::try_to_finstance`]).
///
/// The leg declines when the plan has no table layer (it was not compiled
/// by [`RestrictPlan::compile_for_tables`]), the input is not a table, or the
/// table engine refuses it. Otherwise the table view maps back exactly: a
/// row is the node of its id, re-anchored and holding the row's cells; a
/// kept pair is the source edge that carries it, under that edge's id; a
/// contracted pair is a synthesized edge, whose id the graph engine's
/// allocation gives its key. The leg declines, too, on a view pair that
/// several source edges carry (the graph keeps each, the table one pair)
/// or that no source edge and no contraction gives.
fn table_leg(instance: &GInstance, plan: &RestrictPlan) -> Option<GInstance> {
    plan.boundary.as_ref()?.table.as_ref()?;
    let (table, embedding) = instance.try_to_finstance(plan.source()).ok()?;
    table_view_in_graph(instance, &table, &embedding, plan)
}

/// [`table_leg`] on the table `table` that `instance` is, read
/// with `embedding` ([`GInstance::try_to_finstance`]).
fn table_view_in_graph(
    instance: &GInstance,
    table: &FInstance,
    embedding: &TableEmbedding,
    plan: &RestrictPlan,
) -> Option<GInstance> {
    CROSS_ENGINE_RUNS.with(|runs| runs.set(runs.get() + 1));
    let (view, complement) = crate::table_restrict::functor_restrict_with_plan(table, plan).ok()?;
    let mut graph = GInstance::new();
    for row in view.row_ids() {
        // A node of a table is a plain row (`GInstance::table_shape`): its
        // anchor and fields are all it holds.
        let mut node = Node::new(row.0, view.table_of(row)?.clone());
        node.extra_fields.clone_from(view.row(row)?);
        graph = graph.with_node(node);
    }
    let mut contracted = Vec::new();
    for (edge, pairs) in view.foreign_keys() {
        for &(src, tgt) in pairs {
            let key = (edge.clone(), src, tgt);
            let carriers: Vec<&Edge> = complement.original_fk_edges.get(&key).map_or_else(
                || plan.edge_image.preimages(edge),
                |recorded| recorded.iter().collect(),
            );
            let sources: Vec<EdgeId> = carriers
                .into_iter()
                .filter_map(|carrier| embedding.pairs.get(&(carrier.clone(), src, tgt)).copied())
                .collect();
            let synthesized = complement.synthesized_fk_pairs.contains(&key);
            match sources.as_slice() {
                [] if synthesized => {}
                [id] => graph.place_edge(
                    *id,
                    GEdge {
                        src: src.0,
                        tgt: tgt.0,
                        edge: edge.clone(),
                        position: instance.edge(*id)?.position,
                    },
                ),
                _ => return None,
            }
            if synthesized {
                contracted.push((src.0, tgt.0, edge.clone()));
            }
        }
    }
    let key = |(src, tgt, edge): &(u32, u32, Edge)| SynthKey::Contract {
        src: *src,
        tgt: *tgt,
        edge: edge.clone(),
    };
    let floor = floor_above(instance.max_edge_id().map(|e| e.0));
    let ids = allocate_synthesized(contracted.iter().map(key), floor, IdKind::Edge).ok()?;
    for pair in &contracted {
        let (src, tgt, edge) = pair;
        graph.place_edge(
            EdgeId(ids[&key(pair)]),
            GEdge::new(*src, *tgt, edge.clone()),
        );
    }
    Some(graph)
}

/// The tree leg of the tree square, or `None` when it declines: the input
/// converted to a tree, the tree engine, and its view converted back.
///
/// The view is the one [`graph_restrict_with_plan`] gives, computed by the
/// tree engine ([`wtype_restrict_with_plan`](crate::wtype_restrict_with_plan))
/// on the tree the input is.
///
/// The leg declines when the plan does not preserve tree shape
/// ([`RestrictPlan::preserves_tree_shape`]), has a cell layer (which the
/// tree engine does not apply), or the input holds a fan node, is not a
/// tree ([`GInstance::tree_shape`]), or is one the tree engine refuses,
/// [`RestrictError::RootPruned`] included. Otherwise the tree view maps
/// back exactly: a source node keeps its id, and the tree and the graph
/// allocate synthesized nodes from the same floor in the same key order,
/// since a tree's arcs come in ascending edge id, so keying an expansion by
/// arc position orders it as keying it by edge id does. A kept arc takes
/// the id of the source edge into its child; the synthesized arcs, which
/// follow the kept ones in key order, take ids from the edge floor.
fn tree_leg(instance: &GInstance, plan: &RestrictPlan) -> Option<GInstance> {
    CROSS_ENGINE_RUNS.with(|runs| runs.set(runs.get() + 1));
    if !plan.preserves_tree_shape()
        || plan.cells().is_some()
        || instance.nodes().values().any(is_fan_node)
    {
        return None;
    }
    let (tree, embedding) = instance.to_wtype_embedded(plan.source()).ok()?;
    let node_floor = floor_above(instance.max_node_id());
    if crate::complement::tree_id_floor(&tree).ok() != Some(node_floor) {
        return None;
    }
    let (view, _) = crate::wtype::wtype_restrict_with_plan(&tree, plan).ok()?;
    let mut edge_ids = crate::segment::IdAllocator::from_floor(
        crate::segment::IdRange {
            lo: 0,
            hi: u32::MAX - 1,
        },
        floor_above(instance.max_edge_id().map(|e| e.0)),
    );
    let mut graph = GInstance::new();
    let WInstance { nodes, arcs, .. } = view;
    for node in nodes.into_values() {
        graph = graph.with_node(node);
    }
    for (parent, child, label) in &arcs {
        let kept = *child < node_floor && tree.parent_map.get(child) == Some(parent);
        let (id, position) = if kept {
            let id = *embedding.arcs.get(child)?;
            (id, instance.edge(id)?.position)
        } else {
            (EdgeId(edge_ids.allocate().ok()?), None)
        };
        graph.place_edge(
            id,
            GEdge {
                src: *parent,
                tgt: *child,
                edge: label.clone(),
                position,
            },
        );
    }
    Some(graph)
}

fn prop_edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

#[test]
fn graph_restrict_runs_the_graph_engine_on_every_input() -> TestResult {
    // A tree-shaped graph under a plan that keeps a tree, and a
    // table-shaped graph under a migration with a plan for tables: neither
    // is converted to another shape, and the view is the graph engine's.
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
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("t", "record", None)
        .and_then(|b| b.vertex("t.c", "int", None))
        .and_then(|b| b.vertex("u", "record", None))
        .and_then(|b| b.vertex("u.c", "int", None))
        .and_then(|b| b.edge("t", "t.c", "prop", Some("c")))
        .and_then(|b| b.edge("u", "u.c", "prop", Some("c")))
        .and_then(|b| b.edge("t", "u", "fk", Some("r")))
        .and_then(SchemaBuilder::build)?;
    let mut m = CompiledMigration::default();
    for v in ["t", "t.c", "u", "u.c"] {
        m.surviving_verts.insert(v.into());
    }
    m.surviving_edges.insert(prop_edge("t", "t.c", "c"));
    m.surviving_edges.insert(prop_edge("u", "u.c", "c"));
    m.surviving_edges.insert(Edge {
        src: "t".into(),
        tgt: "u".into(),
        kind: "fk".into(),
        name: Some("r".into()),
    });
    // Two rows joined by `r`: a table and, rooted at the `t` row, a tree.
    let mut g = GInstance::new()
        .with_node(Node::new(0, "t").with_extra_field("c", Value::Int(1)))
        .with_node(Node::new(1, "u").with_extra_field("c", Value::Int(2)));
    g.add_edge(
        0,
        1,
        Edge {
            src: "t".into(),
            tgt: "u".into(),
            kind: "fk".into(),
            name: Some("r".into()),
        },
    )?;
    assert!(g.try_to_finstance(&schema).is_ok(), "the graph is a table");
    assert_runs_the_graph_engine(&g, &schema, &m, protocols)?;

    // The same graph and migration under a protocol that classifies no
    // table: the plan has no cell layer and keeps a tree, and the graph is
    // a tree.
    let unclassifying = Protocol::default();
    let plain_protocols = BoundaryProtocols {
        src: &unclassifying,
        tgt: &unclassifying,
    };
    let plain_schema = SchemaBuilder::new(&unclassifying)
        .vertex("t", "record", None)
        .and_then(|b| b.vertex("t.c", "int", None))
        .and_then(|b| b.vertex("u", "record", None))
        .and_then(|b| b.vertex("u.c", "int", None))
        .and_then(|b| b.edge("t", "t.c", "prop", Some("c")))
        .and_then(|b| b.edge("u", "u.c", "prop", Some("c")))
        .and_then(|b| b.edge("t", "u", "fk", Some("r")))
        .and_then(SchemaBuilder::build)?;
    let plan = crate::RestrictPlan::compile_with_protocols(
        &plain_schema,
        &plain_schema,
        &m,
        plain_protocols,
    )?;
    assert!(plan.preserves_tree_shape());
    assert!(matches!(
        g.tree_shape(&plain_schema),
        crate::TreeShapeReport::Tree { root: 0 }
    ));
    assert_runs_the_graph_engine(&g, &plain_schema, &m, plain_protocols)
}

/// `graph_restrict` and the graph's `AcsetOps::restrict` convert `g` to no
/// other shape and give the graph engine's view.
fn assert_runs_the_graph_engine(
    g: &GInstance,
    schema: &panproto_schema::Schema,
    m: &CompiledMigration,
    protocols: BoundaryProtocols<'_>,
) -> TestResult {
    CROSS_ENGINE_RUNS.with(|runs| runs.set(0));
    let view = graph_restrict(g, schema, schema, m, protocols)?;
    let via_trait = crate::AcsetOps::restrict(g, schema, schema, m, protocols)?;
    assert_eq!(CROSS_ENGINE_RUNS.with(std::cell::Cell::get), 0);
    let (engine, _) = graph_restrict_with_complement(g, schema, schema, m, protocols)?;
    assert!(GInstance::identical(&view, &engine));
    assert!(GInstance::identical(&via_trait, &engine));
    Ok(())
}

// ---------------------------------------------------------------------------
// Tree square
// ---------------------------------------------------------------------------

const NAMES: [&str; 4] = ["v0", "v1", "v2", "v3"];

fn e(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn pool(u: usize, v: usize) -> Edge {
    e(NAMES[u], NAMES[v], &format!("e{u}{v}"))
}

/// What the target does with the source edge between `v_u` and `v_v`.
#[derive(Clone, Copy, Debug)]
enum PairFate {
    /// The target has the same edge.
    Keep,
    /// The target joins the two through an intermediate `w{u}{v}`.
    Expand,
    /// The target has no edge between them and no path.
    Missing,
}

/// The schemas and migration: every pool edge in the source; in the target,
/// each pair kept, expanded through its own intermediate, or missing;
/// `surviving` picks the surviving source vertices; `v1` survives only
/// where its field `k` is 1.
fn schemas(
    fates: &[PairFate],
    surviving: &[bool],
) -> TestResult<(Schema, Schema, CompiledMigration)> {
    let protocol = Protocol::default();
    let mut src = SchemaBuilder::new(&protocol);
    let mut tgt = SchemaBuilder::new(&protocol);
    for v in NAMES {
        src = src.vertex(v, "record", None)?;
        tgt = tgt.vertex(v, "record", None)?;
    }
    let mut m = CompiledMigration::default();
    for (i, fate) in fates.iter().enumerate() {
        let (u, v) = (i / 4, i % 4);
        let edge = pool(u, v);
        src = src.edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())?;
        match fate {
            PairFate::Keep => {
                tgt = tgt.edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())?;
            }
            PairFate::Expand => {
                let w = format!("w{u}{v}");
                tgt = tgt
                    .vertex(&w, "record", None)
                    .and_then(|b| b.edge(NAMES[u], &w, "prop", Some(&format!("a{u}{v}"))))
                    .and_then(|b| b.edge(&w, NAMES[v], "prop", Some(&format!("b{u}{v}"))))?;
                m.surviving_verts.insert(Name::from(w.as_str()));
                m.expansion_path
                    .insert((NAMES[u].into(), NAMES[v].into()), vec![w.into()]);
            }
            PairFate::Missing => {}
        }
    }
    for (v, keep) in NAMES.iter().zip(surviving) {
        if *keep {
            m.surviving_verts.insert(Name::from(*v));
        }
    }
    m.conditional_survival.insert(
        "v1".into(),
        Expr::Builtin(
            BuiltinOp::Eq,
            vec![Expr::Var("k".into()), Expr::Lit(Literal::Int(1))],
        ),
    );
    Ok((src.build()?, tgt.build()?, m))
}

/// A tree-shaped graph: node `i` has id `3 i`, anchor `anchors[i]` and field
/// `k`; node `i > 0` hangs under an earlier node. Its arcs are added in a
/// shuffled order with discarded edges in between, so edge ids are sparse
/// and do not follow the tree's breadth-first order.
fn tree_graph(
    anchors: &[usize],
    parents: &[prop::sample::Index],
    ks: &[bool],
    order: &[prop::sample::Index],
    gaps: &[bool],
) -> TestResult<GInstance> {
    let mut g = GInstance::new();
    for (i, anchor) in anchors.iter().enumerate() {
        let id = 3 * u32::try_from(i)?;
        g = g.with_node(
            Node::new(id, NAMES[*anchor]).with_extra_field("k", Value::Int(i64::from(ks[i]))),
        );
    }
    let mut arcs: Vec<(usize, usize)> = (1..anchors.len())
        .map(|i| (parents[i - 1].index(i), i))
        .collect();
    let mut shuffled = Vec::new();
    while !arcs.is_empty() {
        let at = order[shuffled.len()].index(arcs.len());
        shuffled.push(arcs.remove(at));
    }
    let mut discarded = Vec::new();
    for (n, (p, c)) in shuffled.into_iter().enumerate() {
        if gaps[n] {
            discarded.push(g.add_edge(0, 0, pool(anchors[0], anchors[0]))?);
        }
        let (pid, cid) = (3 * u32::try_from(p)?, 3 * u32::try_from(c)?);
        g.add_edge(pid, cid, pool(anchors[p], anchors[c]))?;
    }
    for id in discarded {
        g.remove_edge(id)?;
    }
    Ok(g)
}

/// A tree-shaped graph and a plan that compiles, or the setup failure that
/// fails the case.
fn arb_case() -> impl Strategy<Value = Result<(GInstance, RestrictPlan), String>> {
    let fate = prop_oneof![
        6 => Just(PairFate::Keep),
        2 => Just(PairFate::Expand),
        1 => Just(PairFate::Missing),
    ];
    (
        prop::collection::vec(fate, 16),
        prop::collection::vec(prop::bool::weighted(0.7), 4),
        prop::collection::vec(0usize..4, 1..10),
        prop::collection::vec(any::<prop::sample::Index>(), 9),
        prop::collection::vec(any::<bool>(), 10),
        prop::collection::vec(any::<prop::sample::Index>(), 9),
        prop::collection::vec(any::<bool>(), 9),
    )
        .prop_filter_map(
            "the plan compiles",
            |(fates, surviving, anchors, parents, ks, order, gaps)| {
                let case = schemas(&fates, &surviving).map_err(|e| e.to_string());
                let (src, tgt, m) = match case {
                    Ok(schemas) => schemas,
                    Err(e) => return Some(Err(e)),
                };
                let plan = RestrictPlan::compile(&src, &tgt, &m).ok()?;
                Some(
                    tree_graph(&anchors, &parents, &ks, &order, &gaps)
                        .map(|g| (g, plan))
                        .map_err(|e| e.to_string()),
                )
            },
        )
}

/// The tree square on one case: when the tree leg runs to the end, its
/// view is the graph engine's under the same ids and child order. Returns
/// whether the leg ran to the end.
///
/// A plan with a cell layer fails the case: the tree leg declines every
/// such plan, so a square over one would check nothing.
fn tree_square(g: &GInstance, plan: &RestrictPlan) -> Result<bool, TestCaseError> {
    prop_assert!(
        plan.cells().is_none(),
        "the tree square cannot run on a plan with a cell layer"
    );
    let Some(leg) = tree_leg(g, plan) else {
        return Ok(false);
    };
    let engine = graph_restrict_with_plan(g, plan).map(|(view, _)| view);
    let Ok(engine) = engine else {
        prop_assert!(
            false,
            "the tree leg gave {leg:?}, the graph engine {engine:?}"
        );
        return Ok(true);
    };
    prop_assert!(GInstance::identical(&leg, &engine), "{leg:?}\n{engine:?}");
    for id in engine.nodes().keys() {
        prop_assert_eq!(leg.ordered_out_edges(*id), engine.ordered_out_edges(*id));
    }
    Ok(true)
}

#[test]
fn the_tree_engine_agrees_with_the_graph_engine_on_tree_shaped_graphs() -> TestResult {
    let taken = AtomicUsize::new(0);
    let declined = AtomicUsize::new(0);
    let mut runner = TestRunner::new(Config {
        cases: 2048,
        ..Config::default()
    });
    runner
        .run(&arb_case(), |case| {
            let (g, plan) = case.map_err(fail)?;
            if tree_square(&g, &plan)? {
                taken.fetch_add(1, Ordering::Relaxed);
            } else {
                declined.fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    let (taken, declined) = (taken.into_inner(), declined.into_inner());
    // The square is not vacuous: the leg runs to the end and declines,
    // each often.
    assert!(taken >= 200, "tree leg ran {taken} times");
    assert!(declined >= 200, "tree leg declined {declined} times");
    Ok(())
}

/// One fixed case through every chain kind: a kept arc, a contraction
/// through a dropped node, and an expansion, on sparse edge ids.
#[test]
fn the_tree_leg_maps_kept_contracted_and_expanded_arcs_back() -> TestResult {
    use PairFate::{Expand, Keep};
    let mut fates = vec![Keep; 16];
    fates[1] = Expand; // e01 expands through w01.
    // v2 is dropped; v0 -> v2 -> v3 contracts to the kept e03.
    let (src, tgt, m) = schemas(&fates, &[true, true, false, true])?;
    let plan = RestrictPlan::compile(&src, &tgt, &m)?;
    let mut g = GInstance::new()
        .with_node(Node::new(0, "v0"))
        .with_node(Node::new(5, "v1").with_extra_field("k", Value::Int(1)))
        .with_node(Node::new(7, "v2"))
        .with_node(Node::new(9, "v3"))
        .with_node(Node::new(11, "v3"));
    let gap = g.add_edge(0, 0, pool(0, 0))?;
    g.add_edge(0, 7, pool(0, 2))?;
    g.add_edge(7, 9, pool(2, 3))?;
    g.add_edge(0, 5, pool(0, 1))?;
    g.add_edge(0, 11, pool(0, 3))?;
    g.remove_edge(gap)?;
    let fast = tree_leg(&g, &plan).ok_or("the tree leg declines")?;
    let (engine, complement) = graph_restrict_with_plan(&g, &plan)?;
    assert!(GInstance::identical(&fast, &engine));
    assert_eq!(complement.synthesized_nodes.len(), 1);
    assert_eq!(complement.synthesized_edges.len(), 3);
    for id in engine.nodes().keys() {
        assert_eq!(fast.ordered_out_edges(*id), engine.ordered_out_edges(*id));
    }
    Ok(())
}

#[test]
fn a_pruned_root_is_outside_the_tree_square() -> TestResult {
    // The root's anchor is dropped: the tree engine refuses with
    // `RootPruned`, so the tree leg declines, and the graph engine, which
    // needs no root, keeps the rest.
    let (src, tgt, m) = schemas(&[PairFate::Keep; 16], &[false, true, true, true])?;
    let plan = RestrictPlan::compile(&src, &tgt, &m)?;
    let mut tree = GInstance::new()
        .with_node(Node::new(0, "v0"))
        .with_node(Node::new(1, "v2"));
    tree.add_edge(0, 1, pool(0, 2))?;
    assert!(tree_leg(&tree, &plan).is_none());
    let (view, _) = graph_restrict_with_plan(&tree, &plan)?;
    assert_eq!(view.nodes().keys().copied().collect::<Vec<_>>(), vec![1]);
    Ok(())
}

#[test]
fn a_plan_that_drops_an_edge_between_survivors_is_outside_the_tree_square() -> TestResult {
    // A missing pair between survivors drops the edge alone in a graph but
    // the subtree in a tree, so the plan does not preserve tree shape and
    // the tree leg declines.
    let mut fates = vec![PairFate::Keep; 16];
    fates[2] = PairFate::Missing;
    let (src, tgt, m) = schemas(&fates, &[true; 4])?;
    let plan = RestrictPlan::compile(&src, &tgt, &m)?;
    assert!(!plan.preserves_tree_shape());
    let mut g = GInstance::new()
        .with_node(Node::new(0, "v0"))
        .with_node(Node::new(1, "v2"))
        .with_node(Node::new(2, "v3"));
    g.add_edge(0, 1, pool(0, 2))?;
    g.add_edge(1, 2, pool(2, 3))?;
    assert!(tree_leg(&g, &plan).is_none());
    let (view, _) = graph_restrict_with_plan(&g, &plan)?;
    // The graph keeps both nodes under v0's dropped edge.
    assert_eq!(view.node_count(), 3);
    assert_eq!(view.edge_count(), 1);
    Ok(())
}

#[test]
fn a_tree_engine_refusal_is_outside_the_tree_square() -> TestResult {
    // `v1` survives only where `k` is 1; a `v1` node without `k` makes the
    // predicate fail to evaluate. The tree engine refuses, unlocated, so
    // the tree leg declines; the graph restrict refuses, located at the
    // node.
    let (src, tgt, m) = schemas(&[PairFate::Keep; 16], &[true; 4])?;
    let plan = RestrictPlan::compile(&src, &tgt, &m)?;
    let mut g = GInstance::new()
        .with_node(Node::new(0, "v0"))
        .with_node(Node::new(4, "v1"));
    g.add_edge(0, 4, pool(0, 1))?;
    assert!(tree_leg(&g, &plan).is_none());
    let Err(engine) = graph_restrict_with_plan(&g, &plan) else {
        return Err("the graph engine accepts a node its predicate cannot read".into());
    };
    assert_eq!(engine.key().map(|k| (k.stage, k.id)), Some((0, 4)));
    assert!(matches!(
        engine.unlocated(),
        RestrictError::SurvivalPredicateFailed { .. }
    ));
    Ok(())
}

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

fn table_protocol() -> Protocol {
    Protocol {
        value_kinds: std::collections::BTreeMap::from([(
            "int".to_owned(),
            panproto_schema::ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        )]),
        non_value_kinds: std::collections::BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    }
}

fn column(table: &str) -> Edge {
    Edge {
        src: table.into(),
        tgt: format!("{table}.c").into(),
        kind: "prop".into(),
        name: Some("c".into()),
    }
}

fn fk(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "fk".into(),
        name: Some(name.into()),
    }
}

/// Tables `t0..t2`, each with the column `c` of kind `int`, and the
/// relations between them.
fn table_schema(relations: &[Edge]) -> TestResult<Schema> {
    let protocol = table_protocol();
    let mut builder = SchemaBuilder::new(&protocol);
    for t in ["t0", "t1", "t2"] {
        let c = format!("{t}.c");
        builder = builder
            .vertex(t, "record", None)
            .and_then(|b| b.vertex(&c, "int", None))
            .and_then(|b| b.edge(t, &c, "prop", Some("c")))?;
    }
    for r in relations {
        builder = builder.edge(&r.src, &r.tgt, &r.kind, r.name.as_deref())?;
    }
    Ok(builder.build()?)
}

/// The migration that keeps the tables `keep` says, every column of a kept
/// table and every relation, with the predicate `c > 0` on every table when
/// `predicate` is set.
fn table_migration(relations: &[Edge], keep: &[bool], predicate: bool) -> CompiledMigration {
    let mut m = CompiledMigration::default();
    for (i, t) in ["t0", "t1", "t2"].iter().enumerate() {
        if keep[i] {
            m.surviving_verts.insert((*t).into());
            m.surviving_verts.insert(format!("{t}.c").into());
            m.surviving_edges.insert(column(t));
        }
        if predicate {
            m.add_conditional_survival(
                t,
                Expr::Builtin(
                    BuiltinOp::Gt,
                    vec![Expr::Var("c".into()), Expr::Lit(Literal::Int(0))],
                ),
            );
        }
    }
    m.surviving_edges.extend(relations.iter().cloned());
    m
}

/// A table-shaped graph over `table_schema(relations)`: node `3 i` in table
/// `rows[i].0` with `c = rows[i].1`, and an edge per pair whose relation
/// joins the two rows' tables.
fn table_graph(
    relations: &[Edge],
    rows: &[(usize, i64)],
    pairs: &[(usize, usize, usize)],
) -> TestResult<GInstance> {
    let names = ["t0", "t1", "t2"];
    let mut g = GInstance::new();
    for (i, (t, c)) in rows.iter().enumerate() {
        let id = 3 * u32::try_from(i)?;
        g = g.with_node(Node::new(id, names[*t]).with_extra_field("c", Value::Int(*c)));
    }
    let mut seen = std::collections::BTreeSet::new();
    for &(s, t, r) in pairs {
        let (Some(relation), Some((ts, _)), Some((tt, _))) =
            (relations.get(r), rows.get(s), rows.get(t))
        else {
            continue;
        };
        if relation.src == names[*ts] && relation.tgt == names[*tt] && seen.insert((r, s, t)) {
            let (src, tgt) = (3 * u32::try_from(s)?, 3 * u32::try_from(t)?);
            g.add_edge(src, tgt, relation.clone())?;
        }
    }
    Ok(g)
}

/// A table-shaped graph, its schema and a migration, or the setup failure
/// that fails the case.
type TableCase = Result<(GInstance, Schema, CompiledMigration), String>;

fn arb_table_case() -> impl Strategy<Value = TableCase> {
    (
        prop::collection::vec((0usize..3, 0usize..3), 0..4),
        prop::collection::vec(prop::bool::weighted(0.7), 3),
        any::<bool>(),
        prop::collection::vec((0usize..3, -3i64..3), 0..7),
        prop::collection::vec((0usize..7, 0usize..7, 0usize..4), 0..8),
    )
        .prop_map(|(relation_spec, keep, predicate, rows, pairs)| {
            let names = ["t0", "t1", "t2"];
            let mut relations: Vec<Edge> = relation_spec
                .iter()
                .map(|(s, t)| fk(names[*s], names[*t], &format!("r{s}{t}")))
                .collect();
            relations.sort();
            relations.dedup();
            let m = table_migration(&relations, &keep, predicate);
            table_schema(&relations)
                .and_then(|schema| Ok((table_graph(&relations, &rows, &pairs)?, schema, m)))
                .map_err(|e| e.to_string())
        })
}

#[test]
fn the_table_engine_agrees_with_the_graph_engine_on_table_shaped_graphs() -> TestResult {
    // On every table-shaped graph and every migration with a plan for
    // tables, the graph engine's view over the table plan is its view over
    // the protocol plan, and the table leg, when it runs to the end, gives
    // that view under the same ids. The tree leg declines every plan with a
    // cell layer, so the tree square runs on the plan without one: a
    // table-shaped graph that is also a tree gives the graph engine's view
    // through the tree engine.
    let protocol = table_protocol();
    let protocols = crate::BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let taken = AtomicUsize::new(0);
    let tree_taken = AtomicUsize::new(0);
    let mut runner = TestRunner::new(Config {
        cases: 1024,
        ..Config::default()
    });
    runner
        .run(&arb_table_case(), |case| {
            let (g, schema, m) = case.map_err(fail)?;
            let Ok(table_plan) = RestrictPlan::compile_for_tables(&schema, &schema, &m, protocols)
            else {
                return Ok(());
            };
            let plan = RestrictPlan::compile_with_protocols(&schema, &schema, &m, protocols)
                .map_err(fail)?;
            let engine = graph_restrict_with_plan(&g, &plan).map(|(view, _)| view);
            let on_table_plan = graph_restrict_with_plan(&g, &table_plan).map(|(view, _)| view);
            match (&engine, &on_table_plan) {
                (Ok(a), Ok(b)) => prop_assert!(GInstance::identical(a, b), "{a:?}\n{b:?}"),
                (Err(a), Err(b)) => prop_assert_eq!(format!("{a:?}"), format!("{b:?}")),
                _ => prop_assert!(false, "{engine:?}\n{on_table_plan:?}"),
            }
            if let Some(fast) = table_leg(&g, &table_plan) {
                taken.fetch_add(1, Ordering::Relaxed);
                let Ok(engine) = engine.as_ref() else {
                    prop_assert!(
                        false,
                        "the table leg gave {fast:?}, the graph engine {engine:?}"
                    );
                    return Ok(());
                };
                prop_assert!(GInstance::identical(&fast, engine), "{fast:?}\n{engine:?}");
            }
            let cell_free = RestrictPlan::compile(&schema, &schema, &m).map_err(fail)?;
            if tree_square(&g, &cell_free)? {
                tree_taken.fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    let (taken, tree_taken) = (taken.into_inner(), tree_taken.into_inner());
    assert!(taken >= 200, "table leg ran {taken} times");
    assert!(tree_taken >= 30, "tree leg ran {tree_taken} times");
    Ok(())
}

#[test]
fn the_table_leg_maps_kept_and_contracted_pairs_back() -> TestResult {
    // t1 is dropped: the row of t0 reaches the row of t2 through it, so the
    // pair is contracted onto the direct relation r02, which also carries a
    // kept pair between other rows.
    let relations = vec![
        fk("t0", "t1", "r01"),
        fk("t1", "t2", "r12"),
        fk("t0", "t2", "r02"),
    ];
    let schema = table_schema(&relations)?;
    let m = table_migration(&relations, &[true, false, true], false);
    let protocol = table_protocol();
    let protocols = crate::BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let plan = RestrictPlan::compile_for_tables(&schema, &schema, &m, protocols)?;
    let g = table_graph(
        &relations,
        &[(0, 1), (1, 1), (2, 1), (0, 2), (2, 2)],
        &[(0, 1, 0), (1, 2, 1), (3, 4, 2)],
    )?;
    let fast = table_leg(&g, &plan).ok_or("the table leg declines")?;
    let (engine, complement) = graph_restrict_with_plan(&g, &plan)?;
    assert!(GInstance::identical(&fast, &engine), "{fast:?}\n{engine:?}");
    assert_eq!(complement.synthesized_edges.len(), 1);
    assert_eq!(fast.edge_count(), 2);
    // The schema-taking restrict gives the same view, through the graph
    // engine.
    let view = crate::graph_restrict(&g, &schema, &schema, &m, protocols)?;
    assert!(GInstance::identical(&view, &engine));
    Ok(())
}

#[test]
fn the_table_leg_declines_without_a_table_plan_or_a_table() -> TestResult {
    let relations = vec![fk("t0", "t1", "r01")];
    let schema = table_schema(&relations)?;
    let m = table_migration(&relations, &[true, true, true], false);
    let protocol = table_protocol();
    let protocols = crate::BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let g = table_graph(&relations, &[(0, 1), (1, 1)], &[(0, 1, 0)])?;
    // A plan with no table layer.
    let plan = RestrictPlan::compile_with_protocols(&schema, &schema, &m, protocols)?;
    assert!(table_leg(&g, &plan).is_none());
    // A graph that is no table: a positioned edge.
    let table_plan = RestrictPlan::compile_for_tables(&schema, &schema, &m, protocols)?;
    assert!(table_leg(&g, &table_plan).is_some());
    let mut positioned = g;
    let id = positioned
        .edges_sorted()
        .next()
        .ok_or("the graph has no edge")?
        .0;
    positioned.set_edge_position(id, Some(0))?;
    assert!(table_leg(&positioned, &table_plan).is_none());
    Ok(())
}

#[test]
fn the_table_leg_declines_a_graph_whose_node_lacks_a_column() -> TestResult {
    // A node without the column `c` is a table-shaped graph but no total
    // table, which the table engine refuses, so the table leg declines; the
    // graph engine keeps both nodes.
    let relations = vec![fk("t0", "t1", "r01")];
    let schema = table_schema(&relations)?;
    let m = table_migration(&relations, &[true, true, true], false);
    let protocol = table_protocol();
    let protocols = crate::BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let table_plan = RestrictPlan::compile_for_tables(&schema, &schema, &m, protocols)?;
    let mut g = table_graph(&relations, &[(0, 1)], &[])?;
    g = g.with_node(Node::new(3, "t1"));
    g.add_edge(0, 3, fk("t0", "t1", "r01"))?;
    assert!(table_leg(&g, &table_plan).is_none());
    let (engine, _) = graph_restrict_with_plan(&g, &table_plan)?;
    assert_eq!(engine.node_count(), 2);
    Ok(())
}
