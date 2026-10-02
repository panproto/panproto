#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A tree Pi and the polynomial-functor restrict with value transforms
//! take time linear in the tree's size up to a logarithmic factor: the
//! child index the transforms read through is built once per call, so no
//! node pays for a scan of every arc.

mod common;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use panproto_expr::Expr;
use panproto_gat::Name;
use panproto_inst::{
    CompiledMigration, FieldPresence, FieldTransform, Node, Value, WInstance,
    restrict_with_complement, wtype_pi,
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

/// The identity on `schema` with a transform that copies each `mid`'s
/// child `k` into its field `copy`.
fn migration() -> CompiledMigration {
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
    migration
}

/// Which tree operation a timing runs.
#[derive(Clone, Copy, Debug)]
enum Operation {
    /// `wtype_pi` along the migration.
    Pi,
    /// `restrict_with_complement` along the migration.
    PolyRestrict,
}

/// The time of `operation` on `instance`, after checking that it copied
/// each `mid`'s child into its field.
fn run_time(operation: Operation, instance: &WInstance) -> Duration {
    let (s, m) = (schema(), migration());
    let start = Instant::now();
    let result = match operation {
        Operation::Pi => wtype_pi(instance, &s, &m, usize::MAX).unwrap(),
        Operation::PolyRestrict => restrict_with_complement(instance, &s, &s, &m).unwrap().0,
    };
    let elapsed = start.elapsed();
    assert_eq!(result.nodes.len(), instance.nodes.len());
    for mid in (1..u32::try_from(instance.nodes.len()).unwrap()).step_by(2) {
        assert_eq!(
            result.nodes[&mid].extra_fields.get("copy"),
            Some(&Value::Int(1)),
            "{operation:?} at {mid}"
        );
    }
    elapsed
}

/// The growth, from 1000 to 4000 mids, of `operation`'s time in units of a
/// linear baseline over the same tree (one clone of it).
fn growth(operation: Operation) -> f64 {
    let (small_tree, large_tree) = (tree(1000), tree(4000));
    let (small, large) = common::sizes_in_units(
        7,
        common::Probe {
            work: || run_time(operation, &small_tree),
            unit: || drop(std::hint::black_box(small_tree.clone())),
        },
        common::Probe {
            work: || run_time(operation, &large_tree),
            unit: || drop(std::hint::black_box(large_tree.clone())),
        },
    );
    let growth = large / small;
    eprintln!(
        "{operation:?}: 1000 mids: {small:.1} units; 4000 mids: {large:.1} units; \
         growth {growth:.2}"
    );
    growth
}

// Quadrupling the mids quadruples the elements and the arcs. Work that
// scans every arc at every transformed node grows sixteenfold, fourfold in
// units of a linear baseline over the same tree; work linear in the tree up
// to a logarithmic factor keeps its size in those units. The bound 2 is the
// geometric mean of the two growths, as for the tree query.

#[test]
fn a_tree_pi_is_not_charged_per_node_and_arc() {
    let growth = growth(Operation::Pi);
    assert!(growth < 2.0, "growth {growth:.2}");
}

#[test]
fn a_tree_poly_restrict_is_not_charged_per_node_and_arc() {
    let growth = growth(Operation::PolyRestrict);
    assert!(growth < 2.0, "growth {growth:.2}");
}
