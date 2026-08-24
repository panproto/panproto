//! One hyper-edge, several fan shapes, one answer per shape.
//!
//! A compiled migration keys its hyper-edge contraction table by
//! `(hyper_edge_id, label_set)`, so the same hyper-edge can retarget one way
//! when a fan carries both children and another way when only one survives.
//! Reconstruction picks the entry whose label set matches the fan it is
//! looking at, and it picks the same one on every run.

#![allow(clippy::expect_used)]

use std::collections::{BTreeMap, HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::{CompiledMigration, Fan, Node, WInstance, reconstruct_fans};
use panproto_schema::{Edge, HyperEdge, Schema, Vertex};
use rustc_hash::{FxHashMap, FxHashSet};

fn target_schema() -> Schema {
    let mut vertices = HashMap::new();
    for (id, kind) in [
        ("parent", "object"),
        ("left", "string"),
        ("right", "string"),
    ] {
        vertices.insert(
            Name::from(id),
            Vertex {
                id: Name::from(id),
                kind: Name::from(kind),
                nsid: None,
            },
        );
    }
    let signature: HashMap<Name, Name> = [
        (Name::from("parent"), Name::from("parent")),
        (Name::from("left"), Name::from("left")),
        (Name::from("right"), Name::from("right")),
    ]
    .into_iter()
    .collect();
    let mut hyper_edges = HashMap::new();
    for id in ["fan_pair", "fan_left_only"] {
        hyper_edges.insert(
            Name::from(id),
            HyperEdge {
                id: Name::from(id),
                kind: "fan".into(),
                signature: signature.clone(),
                parent_label: "parent".into(),
            },
        );
    }

    Schema {
        protocol: "test".into(),
        vertices,
        edges: HashMap::new(),
        hyper_edges,
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        entries: Vec::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing: HashMap::new(),
        incoming: HashMap::new(),
        between: HashMap::new(),
    }
}

/// The instance under test: node 0 is the fan's parent, nodes 1 and 2 its
/// `left` and `right` children.
fn instance() -> WInstance {
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "parent"));
    nodes.insert(1, Node::new(1, "left"));
    nodes.insert(2, Node::new(2, "right"));

    let arcs = vec![
        (
            0,
            1,
            Edge {
                src: "parent".into(),
                tgt: "left".into(),
                kind: "prop".into(),
                name: Some("left".into()),
            },
        ),
        (
            0,
            2,
            Edge {
                src: "parent".into(),
                tgt: "right".into(),
                kind: "prop".into(),
                name: Some("right".into()),
            },
        ),
    ];

    let mut fan = Fan::new("fan", 0);
    fan.children.insert("left".to_string(), 1);
    fan.children.insert("right".to_string(), 2);

    WInstance::new(nodes, arcs, vec![fan], 0, "parent".into())
}

fn two_shape_migration() -> CompiledMigration {
    let mut hyper_resolver = panproto_inst::HyperResolverTable::new();
    hyper_resolver.insert(
        (
            Name::from("fan"),
            vec![Name::from("left"), Name::from("right")],
        ),
        (
            Name::from("fan_pair"),
            HashMap::from([
                (Name::from("left"), Name::from("a")),
                (Name::from("right"), Name::from("b")),
            ]),
        ),
    );
    hyper_resolver.insert(
        (Name::from("fan"), vec![Name::from("left")]),
        (
            Name::from("fan_left_only"),
            HashMap::from([(Name::from("left"), Name::from("only"))]),
        ),
    );

    CompiledMigration {
        surviving_verts: HashSet::from([
            Name::from("parent"),
            Name::from("left"),
            Name::from("right"),
        ]),
        hyper_resolver,
        ..CompiledMigration::default()
    }
}

/// Run reconstruction with the given surviving node set and read back the one
/// fan it produces as `(target_hyper_edge, sorted label -> node)`.
fn reconstruct(surviving: &[u32]) -> (String, BTreeMap<String, u32>) {
    let instance = instance();
    let migration = two_shape_migration();
    let schema = target_schema();
    let surviving: FxHashSet<u32> = surviving.iter().copied().collect();
    let ancestors = FxHashMap::default();

    let fans = reconstruct_fans(&instance, &surviving, &ancestors, &migration, &schema)
        .expect("reconstruction should succeed");
    assert_eq!(fans.len(), 1, "one fan in, one fan out");
    let fan = &fans[0];
    (
        fan.hyper_edge_id.clone(),
        fan.children
            .iter()
            .map(|(label, id)| (label.clone(), *id))
            .collect(),
    )
}

#[test]
fn a_full_fan_takes_the_two_label_entry() {
    let (target, children) = reconstruct(&[0, 1, 2]);
    assert_eq!(target, "fan_pair");
    assert_eq!(
        children,
        BTreeMap::from([("a".to_string(), 1), ("b".to_string(), 2)])
    );
}

#[test]
fn a_fan_that_loses_a_child_takes_the_one_label_entry() {
    let (target, children) = reconstruct(&[0, 1]);
    assert_eq!(
        target, "fan_left_only",
        "the surviving label set selects the entry"
    );
    assert_eq!(children, BTreeMap::from([("only".to_string(), 1)]));
}

#[test]
fn shape_selection_does_not_drift_between_runs() {
    // Every call rebuilds the instance and the compiled table from scratch, so
    // each iteration sees fresh, independently randomized hash orders.
    let full = reconstruct(&[0, 1, 2]);
    let partial = reconstruct(&[0, 1]);
    for _ in 0..256 {
        assert_eq!(reconstruct(&[0, 1, 2]), full);
        assert_eq!(reconstruct(&[0, 1]), partial);
    }
}
