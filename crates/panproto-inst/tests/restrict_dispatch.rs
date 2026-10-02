#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Restrict dispatch: on a tree the plan keeps a tree, the tree engine's
//! view, mapped back, is the graph engine's view under the same ids, with
//! the same child order; otherwise, and whenever the tree engine fails, the
//! graph engine decides.

use std::sync::atomic::{AtomicUsize, Ordering};

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::Name;
use panproto_inst::{
    CompiledMigration, GInstance, Node, RestrictError, RestrictPlan, Value,
    graph_restrict_dispatched, graph_restrict_on_table, graph_restrict_on_tree,
    graph_restrict_with_plan,
};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

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
fn schemas(fates: &[PairFate], surviving: &[bool]) -> (Schema, Schema, CompiledMigration) {
    let protocol = Protocol::default();
    let mut src = SchemaBuilder::new(&protocol);
    let mut tgt = SchemaBuilder::new(&protocol);
    for v in NAMES {
        src = src.vertex(v, "record", None).unwrap();
        tgt = tgt.vertex(v, "record", None).unwrap();
    }
    let mut m = CompiledMigration::default();
    for (i, fate) in fates.iter().enumerate() {
        let (u, v) = (i / 4, i % 4);
        let edge = pool(u, v);
        src = src
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .unwrap();
        match fate {
            PairFate::Keep => {
                tgt = tgt
                    .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
                    .unwrap();
            }
            PairFate::Expand => {
                let w = format!("w{u}{v}");
                tgt = tgt
                    .vertex(&w, "record", None)
                    .and_then(|b| b.edge(NAMES[u], &w, "prop", Some(&format!("a{u}{v}"))))
                    .and_then(|b| b.edge(&w, NAMES[v], "prop", Some(&format!("b{u}{v}"))))
                    .unwrap();
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
    (src.build().unwrap(), tgt.build().unwrap(), m)
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
) -> GInstance {
    let mut g = GInstance::new();
    for (i, anchor) in anchors.iter().enumerate() {
        let id = 3 * u32::try_from(i).unwrap();
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
            discarded.push(g.add_edge(0, 0, pool(anchors[0], anchors[0])).unwrap());
        }
        let (pid, cid) = (3 * u32::try_from(p).unwrap(), 3 * u32::try_from(c).unwrap());
        g.add_edge(pid, cid, pool(anchors[p], anchors[c])).unwrap();
    }
    for id in discarded {
        g.remove_edge(id).unwrap();
    }
    g
}

fn arb_case() -> impl Strategy<Value = (GInstance, RestrictPlan)> {
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
                let (src, tgt, m) = schemas(&fates, &surviving);
                let plan = RestrictPlan::compile(&src, &tgt, &m).ok()?;
                Some((tree_graph(&anchors, &parents, &ks, &order, &gaps), plan))
            },
        )
}

/// The dispatch square on one case: the same view under the same ids and
/// child order, or the same error.
fn square(g: &GInstance, plan: &RestrictPlan) -> Result<(), TestCaseError> {
    let dispatched = graph_restrict_dispatched(g, plan);
    let engine = graph_restrict_with_plan(g, plan).map(|(view, _)| view);
    match (&dispatched, &engine) {
        (Ok(d), Ok(r)) => {
            prop_assert!(GInstance::identical(d, r), "{d:?}\n{r:?}");
            for id in r.nodes().keys() {
                prop_assert_eq!(d.ordered_out_edges(*id), r.ordered_out_edges(*id));
            }
        }
        (Err(d), Err(r)) => prop_assert_eq!(format!("{d:?}"), format!("{r:?}")),
        _ => prop_assert!(false, "{dispatched:?}\n{engine:?}"),
    }
    Ok(())
}

#[test]
fn the_restrict_dispatch_square_commutes() {
    let taken = AtomicUsize::new(0);
    let declined = AtomicUsize::new(0);
    let mut runner = TestRunner::new(Config {
        cases: 2048,
        ..Config::default()
    });
    runner
        .run(&arb_case(), |(g, plan)| {
            if graph_restrict_on_tree(&g, &plan).is_some() {
                taken.fetch_add(1, Ordering::Relaxed);
            } else {
                declined.fetch_add(1, Ordering::Relaxed);
            }
            square(&g, &plan)
        })
        .unwrap();
    let (taken, declined) = (taken.into_inner(), declined.into_inner());
    // The square is not vacuous: both paths are exercised.
    assert!(taken >= 200, "fast path taken {taken} times");
    assert!(declined >= 200, "fast path declined {declined} times");
}

/// One fixed case through every chain kind: a kept arc, a contraction
/// through a dropped node, and an expansion, on sparse edge ids.
#[test]
fn the_fast_path_maps_kept_contracted_and_expanded_arcs_back() {
    use PairFate::{Expand, Keep};
    let mut fates = vec![Keep; 16];
    fates[1] = Expand; // e01 expands through w01.
    // v2 is dropped; v0 -> v2 -> v3 contracts to the kept e03.
    let (src, tgt, m) = schemas(&fates, &[true, true, false, true]);
    let plan = RestrictPlan::compile(&src, &tgt, &m).unwrap();
    let mut g = GInstance::new()
        .with_node(Node::new(0, "v0"))
        .with_node(Node::new(5, "v1").with_extra_field("k", Value::Int(1)))
        .with_node(Node::new(7, "v2"))
        .with_node(Node::new(9, "v3"))
        .with_node(Node::new(11, "v3"));
    let gap = g.add_edge(0, 0, pool(0, 0)).unwrap();
    g.add_edge(0, 7, pool(0, 2)).unwrap();
    g.add_edge(7, 9, pool(2, 3)).unwrap();
    g.add_edge(0, 5, pool(0, 1)).unwrap();
    g.add_edge(0, 11, pool(0, 3)).unwrap();
    g.remove_edge(gap).unwrap();
    let fast = graph_restrict_on_tree(&g, &plan).expect("the fast path is taken");
    let (engine, complement) = graph_restrict_with_plan(&g, &plan).unwrap();
    assert!(GInstance::identical(&fast, &engine));
    assert_eq!(complement.synthesized_nodes.len(), 1);
    assert_eq!(complement.synthesized_edges.len(), 3);
    for id in engine.nodes().keys() {
        assert_eq!(fast.ordered_out_edges(*id), engine.ordered_out_edges(*id));
    }
}

#[test]
fn a_pruned_root_falls_back_to_the_graph_path() {
    // The root's anchor is dropped: the tree engine refuses with
    // `RootPruned`, and the graph engine, which needs no root, keeps the
    // rest.
    let (src, tgt, m) = schemas(&[PairFate::Keep; 16], &[false, true, true, true]);
    let plan = RestrictPlan::compile(&src, &tgt, &m).unwrap();
    let mut tree = GInstance::new()
        .with_node(Node::new(0, "v0"))
        .with_node(Node::new(1, "v2"));
    tree.add_edge(0, 1, pool(0, 2)).unwrap();
    assert!(graph_restrict_on_tree(&tree, &plan).is_none());
    let view = graph_restrict_dispatched(&tree, &plan).unwrap();
    assert_eq!(view.nodes().keys().copied().collect::<Vec<_>>(), vec![1]);
}

#[test]
fn a_plan_that_drops_an_edge_between_survivors_takes_the_graph_path() {
    // A missing pair between survivors drops the edge alone in a graph but
    // the subtree in a tree, so the plan does not preserve tree shape and
    // the dispatcher takes the graph path.
    let mut fates = vec![PairFate::Keep; 16];
    fates[2] = PairFate::Missing;
    let (src, tgt, m) = schemas(&fates, &[true; 4]);
    let plan = RestrictPlan::compile(&src, &tgt, &m).unwrap();
    assert!(!plan.preserves_tree_shape());
    let mut g = GInstance::new()
        .with_node(Node::new(0, "v0"))
        .with_node(Node::new(1, "v2"))
        .with_node(Node::new(2, "v3"));
    g.add_edge(0, 1, pool(0, 2)).unwrap();
    g.add_edge(1, 2, pool(2, 3)).unwrap();
    assert!(graph_restrict_on_tree(&g, &plan).is_none());
    let view = graph_restrict_dispatched(&g, &plan).unwrap();
    // The graph keeps both nodes under v0's dropped edge.
    assert_eq!(view.node_count(), 3);
    assert_eq!(view.edge_count(), 1);
}

#[test]
fn a_tree_engine_error_is_decided_by_the_graph_path() {
    // `v1` survives only where `k` is 1; a `v1` node without `k` makes the
    // predicate fail to evaluate. The tree engine refuses, unlocated; the
    // dispatcher returns the graph engine's refusal, located at the node.
    let (src, tgt, m) = schemas(&[PairFate::Keep; 16], &[true; 4]);
    let plan = RestrictPlan::compile(&src, &tgt, &m).unwrap();
    let mut g = GInstance::new()
        .with_node(Node::new(0, "v0"))
        .with_node(Node::new(4, "v1"));
    g.add_edge(0, 4, pool(0, 1)).unwrap();
    assert!(graph_restrict_on_tree(&g, &plan).is_none());
    let dispatched = graph_restrict_dispatched(&g, &plan).unwrap_err();
    let Err(engine) = graph_restrict_with_plan(&g, &plan) else {
        panic!("the graph engine refuses");
    };
    assert_eq!(format!("{dispatched:?}"), format!("{engine:?}"));
    assert_eq!(dispatched.key().map(|k| (k.stage, k.id)), Some((0, 4)));
    assert!(matches!(
        dispatched.unlocated(),
        RestrictError::SurvivalPredicateFailed { .. }
    ));
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
fn table_schema(relations: &[Edge]) -> Schema {
    let protocol = table_protocol();
    let mut builder = SchemaBuilder::new(&protocol);
    for t in ["t0", "t1", "t2"] {
        let c = format!("{t}.c");
        builder = builder
            .vertex(t, "record", None)
            .and_then(|b| b.vertex(&c, "int", None))
            .and_then(|b| b.edge(t, &c, "prop", Some("c")))
            .unwrap();
    }
    for r in relations {
        builder = builder
            .edge(&r.src, &r.tgt, &r.kind, r.name.as_deref())
            .unwrap();
    }
    builder.build().unwrap()
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
fn table_graph(relations: &[Edge], rows: &[(usize, i64)], pairs: &[(usize, usize, usize)]) -> GInstance {
    let names = ["t0", "t1", "t2"];
    let mut g = GInstance::new();
    for (i, (t, c)) in rows.iter().enumerate() {
        let id = 3 * u32::try_from(i).unwrap();
        g = g.with_node(Node::new(id, names[*t]).with_extra_field("c", Value::Int(*c)));
    }
    let mut seen = std::collections::BTreeSet::new();
    for &(s, t, r) in pairs {
        let (Some(relation), Some((ts, _)), Some((tt, _))) = (relations.get(r), rows.get(s), rows.get(t))
        else {
            continue;
        };
        if relation.src == names[*ts] && relation.tgt == names[*tt] && seen.insert((r, s, t)) {
            let (src, tgt) = (3 * u32::try_from(s).unwrap(), 3 * u32::try_from(t).unwrap());
            g.add_edge(src, tgt, relation.clone()).unwrap();
        }
    }
    g
}

fn arb_table_case() -> impl Strategy<Value = (GInstance, Schema, CompiledMigration)> {
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
            let schema = table_schema(&relations);
            let m = table_migration(&relations, &keep, predicate);
            (table_graph(&relations, &rows, &pairs), schema, m)
        })
}

#[test]
fn the_table_dispatch_square_commutes() {
    // On every table-shaped graph and every migration with a plan for
    // tables, the table path, when it is taken, gives the graph engine's
    // view under the same ids, over the table plan and over the protocol
    // plan alike; the dispatcher gives it in every case.
    let protocol = table_protocol();
    let protocols = panproto_inst::BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let taken = AtomicUsize::new(0);
    let mut runner = TestRunner::new(Config {
        cases: 1024,
        ..Config::default()
    });
    runner
        .run(&arb_table_case(), |(g, schema, m)| {
            let Ok(table_plan) = RestrictPlan::compile_for_tables(&schema, &schema, &m, protocols)
            else {
                return Ok(());
            };
            let plan = RestrictPlan::compile_with_protocols(&schema, &schema, &m, protocols).unwrap();
            let engine = graph_restrict_with_plan(&g, &plan).map(|(view, _)| view);
            let on_table_plan = graph_restrict_with_plan(&g, &table_plan).map(|(view, _)| view);
            match (&engine, &on_table_plan) {
                (Ok(a), Ok(b)) => prop_assert!(GInstance::identical(a, b), "{a:?}\n{b:?}"),
                (Err(a), Err(b)) => prop_assert_eq!(format!("{a:?}"), format!("{b:?}")),
                _ => prop_assert!(false, "{engine:?}\n{on_table_plan:?}"),
            }
            if let Some(fast) = graph_restrict_on_table(&g, &table_plan) {
                taken.fetch_add(1, Ordering::Relaxed);
                let engine = engine.as_ref().unwrap();
                prop_assert!(GInstance::identical(&fast, engine), "{fast:?}\n{engine:?}");
            }
            square(&g, &table_plan)
        })
        .unwrap();
    let taken = taken.into_inner();
    assert!(taken >= 200, "table path taken {taken} times");
}

#[test]
fn the_table_path_maps_kept_and_contracted_pairs_back() {
    // t1 is dropped: the row of t0 reaches the row of t2 through it, so the
    // pair is contracted onto the direct relation r02, which also carries a
    // kept pair between other rows.
    let relations = vec![fk("t0", "t1", "r01"), fk("t1", "t2", "r12"), fk("t0", "t2", "r02")];
    let schema = table_schema(&relations);
    let m = table_migration(&relations, &[true, false, true], false);
    let protocol = table_protocol();
    let protocols = panproto_inst::BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let plan = RestrictPlan::compile_for_tables(&schema, &schema, &m, protocols).unwrap();
    let g = table_graph(
        &relations,
        &[(0, 1), (1, 1), (2, 1), (0, 2), (2, 2)],
        &[(0, 1, 0), (1, 2, 1), (3, 4, 2)],
    );
    let fast = graph_restrict_on_table(&g, &plan).expect("the table path is taken");
    let (engine, complement) = graph_restrict_with_plan(&g, &plan).unwrap();
    assert!(GInstance::identical(&fast, &engine), "{fast:?}\n{engine:?}");
    assert_eq!(complement.synthesized_edges.len(), 1);
    assert_eq!(fast.edge_count(), 2);
    // The schema-taking restrict, which dispatches, gives the same view as
    // the one that returns the complement, which does not.
    let dispatched = panproto_inst::graph_restrict(&g, &schema, &schema, &m, protocols).unwrap();
    let (direct, _) =
        panproto_inst::graph_restrict_with_complement(&g, &schema, &schema, &m, protocols).unwrap();
    assert!(GInstance::identical(&dispatched, &direct));
}

#[test]
fn the_table_path_declines_without_a_table_plan_or_a_table() {
    let relations = vec![fk("t0", "t1", "r01")];
    let schema = table_schema(&relations);
    let m = table_migration(&relations, &[true, true, true], false);
    let protocol = table_protocol();
    let protocols = panproto_inst::BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let g = table_graph(&relations, &[(0, 1), (1, 1)], &[(0, 1, 0)]);
    // A plan with no table layer.
    let plan = RestrictPlan::compile_with_protocols(&schema, &schema, &m, protocols).unwrap();
    assert!(graph_restrict_on_table(&g, &plan).is_none());
    // A graph that is no table: a positioned edge.
    let table_plan = RestrictPlan::compile_for_tables(&schema, &schema, &m, protocols).unwrap();
    assert!(graph_restrict_on_table(&g, &table_plan).is_some());
    let mut positioned = g.clone();
    let id = positioned.edges_sorted().next().unwrap().0;
    positioned.set_edge_position(id, Some(0)).unwrap();
    assert!(graph_restrict_on_table(&positioned, &table_plan).is_none());
}
