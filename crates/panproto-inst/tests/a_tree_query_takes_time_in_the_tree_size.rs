#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A query over a tree, and a tree extend with value transforms, take time
//! linear in the tree's size up to a logarithmic factor: no element pays
//! for a scan of every arc.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::Name;
use panproto_inst::{
    CompiledMigration, FieldPresence, FieldTransform, InstanceQuery, Node, Value, WInstance, execute_query,
    wtype_extend_engine,
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
        nodes.insert(leaf, Node::new(leaf, "leaf").with_value(FieldPresence::Present(Value::Int(1))));
        arcs.push((0, mid, edge("r", "mid", "m")));
        arcs.push((mid, leaf, edge("mid", "leaf", "k")));
    }
    WInstance::new(nodes, arcs, vec![], 0, "r".into())
}

/// The least of three times of a predicate query at every `mid`, a
/// one-step path query from every `mid` to its leaf, and a tree extend that
/// transforms every `mid` from its child, on the tree with `n` mids, after
/// checking their answers.
fn run_time(n: u32) -> Duration {
    let instance = tree(n);
    let s = schema();
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
    let mut best = Duration::MAX;
    for _ in 0..3 {
        let start = Instant::now();
        let by_predicate = execute_query(&selected, &instance, &s).unwrap();
        let by_path = execute_query(&stepped, &instance, &s).unwrap();
        let extended = wtype_extend_engine(&instance, &s, &migration).unwrap();
        best = best.min(start.elapsed());
        assert_eq!(by_predicate.len(), n as usize);
        assert_eq!(by_predicate[0].fields.get("k"), Some(&Value::Int(1)));
        assert_eq!(by_path.len(), n as usize);
        assert_eq!(by_path[0].node_id, 2);
        assert_eq!(
            extended.nodes[&1].extra_fields.get("copy"),
            Some(&Value::Int(1))
        );
    }
    best
}

#[test]
fn a_tree_query_and_extend_are_not_charged_per_element_and_arc() {
    // Doubling the mids doubles the elements and the arcs. Work that scans
    // every arc at every element grows fourfold; work linear in the tree up
    // to a logarithmic factor grows about twofold.
    let small = run_time(4000);
    let large = run_time(8000);
    let ratio = large.as_secs_f64() / small.as_secs_f64();
    eprintln!("4000 mids: {small:?}; 8000 mids: {large:?}; ratio {ratio:.2}");
    assert!(
        ratio < 3.0,
        "doubling the tree multiplied the time by {ratio:.2} ({small:?} to {large:?})"
    );
}
