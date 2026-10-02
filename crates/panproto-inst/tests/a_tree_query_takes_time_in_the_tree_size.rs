#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A query over a tree, and a tree extend with value transforms, take time
//! linear in the tree's size up to a logarithmic factor: no element pays
//! for a scan of every arc.

mod common;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::Name;
use panproto_inst::{
    CompiledMigration, FieldPresence, FieldTransform, InstanceQuery, Node, Value, WInstance,
    execute_query, wtype_extend_engine,
};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

fn schema() -> Schema {
    SchemaBuilder::new(&Protocol::default())
        .vertex("r", "record", None)
        .unwrap()
        .vertex("mid", "record", None)
        .unwrap()
        .vertex("leaf", "integer", None)
        .unwrap()
        .edge("r", "mid", "prop", Some("m"))
        .unwrap()
        .edge("mid", "leaf", "prop", Some("k"))
        .unwrap()
        .build()
        .unwrap()
}

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

/// A root with `n` children `mid`, each holding one leaf `k = 1`.
fn tree(n: u32) -> WInstance {
    let mut nodes: HashMap<u32, Node> = HashMap::from([(0, Node::new(0, "r"))]);
    let mut arcs = Vec::new();
    for i in 0..n {
        let (mid, leaf) = (1 + 2 * i, 2 + 2 * i);
        nodes.insert(mid, Node::new(mid, "mid"));
        nodes.insert(
            leaf,
            Node::new(leaf, "leaf").with_value(FieldPresence::Present(Value::Int(1))),
        );
        arcs.push((0, mid, edge("r", "mid", "m")));
        arcs.push((mid, leaf, edge("mid", "leaf", "k")));
    }
    WInstance::new(nodes, arcs, vec![], 0, "r".into())
}

/// The queries and the migration the timing runs: a predicate query at
/// every `mid`, a one-step path query from every `mid` to its leaf, and a
/// tree extend that transforms every `mid` from its child.
fn workload() -> (InstanceQuery, InstanceQuery, CompiledMigration) {
    let k_is_one = Expr::Builtin(
        BuiltinOp::Eq,
        vec![Expr::Var("k".into()), Expr::Lit(Literal::Int(1))],
    );
    let selected = InstanceQuery {
        anchor: Name::from("mid"),
        predicate: Some(k_is_one),
        ..Default::default()
    };
    let stepped = InstanceQuery {
        anchor: Name::from("mid"),
        path: vec!["prop".into()],
        ..Default::default()
    };
    let mut migration = CompiledMigration {
        surviving_verts: ["r", "mid", "leaf"].into_iter().map(Name::from).collect(),
        surviving_edges: [edge("r", "mid", "m"), edge("mid", "leaf", "k")]
            .into_iter()
            .collect(),
        ..CompiledMigration::default()
    };
    migration.field_transforms.insert(
        "mid".into(),
        vec![FieldTransform::ComputeField {
            target_key: "copy".into(),
            expr: Expr::Var("k".into()),
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Projection,
        }],
    );
    (selected, stepped, migration)
}

/// The time of the workload on `instance`, with `n` mids, after checking
/// its answers.
fn run_time(instance: &WInstance, n: usize) -> Duration {
    let s = schema();
    let (selected, stepped, migration) = workload();
    let start = Instant::now();
    let by_predicate = execute_query(&selected, instance, &s).unwrap();
    let by_path = execute_query(&stepped, instance, &s).unwrap();
    let extended = wtype_extend_engine(instance, &s, &migration).unwrap();
    let elapsed = start.elapsed();
    assert_eq!(by_predicate.len(), n);
    assert_eq!(by_predicate[0].fields.get("k"), Some(&Value::Int(1)));
    assert_eq!(by_path.len(), n);
    assert_eq!(by_path[0].node_id, 2);
    assert_eq!(
        extended.nodes[&1].extra_fields.get("copy"),
        Some(&Value::Int(1))
    );
    elapsed
}

#[test]
fn a_tree_query_and_extend_are_not_charged_per_element_and_arc() {
    // Quadrupling the mids quadruples the elements and the arcs. Work that
    // scans every arc at every element grows sixteenfold, fourfold in units
    // of a linear baseline over the same tree (one clone of it); work linear
    // in the tree up to a logarithmic factor keeps its size in those units.
    // The bound 2 is the geometric mean of the two growths.
    let (small_tree, large_tree) = (tree(2000), tree(8000));
    let (small, large) = common::sizes_in_units(
        7,
        common::Probe {
            work: || run_time(&small_tree, 2000),
            unit: || drop(std::hint::black_box(small_tree.clone())),
        },
        common::Probe {
            work: || run_time(&large_tree, 8000),
            unit: || drop(std::hint::black_box(large_tree.clone())),
        },
    );
    let growth = large / small;
    eprintln!("2000 mids: {small:.1} units; 8000 mids: {large:.1} units; growth {growth:.2}");
    assert!(
        growth < 2.0,
        "quadrupling the tree multiplied the time in baseline units by {growth:.2} \
         ({small:.1} to {large:.1})"
    );
}
