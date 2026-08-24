//! A hyper-edge whose fans come in several label shapes keeps one
//! resolver entry per shape all the way through compilation.
//!
//! `Migration::hyper_resolver` is keyed by `(hyper_edge_id, labels)`, so a
//! single hyper-edge may carry a distinct retarget per fan shape. Compilation
//! must preserve every entry and the shape that selects it, and it must do so
//! identically on every run.

#![allow(clippy::expect_used)]

use std::collections::{BTreeMap, HashMap};

use panproto_gat::Name;
use panproto_mig::{Migration, compile};
use panproto_schema::{HyperEdge, Schema, Vertex};

fn schema_with_hyper_edge(vertices: &[(&str, &str)], he: &[(&str, &[(&str, &str)])]) -> Schema {
    let mut vert_map = HashMap::new();
    for (id, kind) in vertices {
        vert_map.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }
    let mut hyper_edges = HashMap::new();
    for (id, signature) in he {
        hyper_edges.insert(
            Name::from(*id),
            HyperEdge {
                id: Name::from(*id),
                kind: "fan".into(),
                signature: signature
                    .iter()
                    .map(|(l, v)| (Name::from(*l), Name::from(*v)))
                    .collect(),
                parent_label: "parent".into(),
            },
        );
    }

    Schema {
        protocol: "test".into(),
        vertices: vert_map,
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

/// Build the two-shape migration under test: one hyper-edge `fan`, two
/// resolver entries distinguished only by their label set.
fn two_shape_migration() -> (Schema, Schema, Migration) {
    let verts = [
        ("parent", "object"),
        ("left", "string"),
        ("right", "string"),
    ];
    let sig: &[(&str, &str)] = &[("parent", "parent"), ("left", "left"), ("right", "right")];
    let src = schema_with_hyper_edge(&verts, &[("fan", sig)]);
    let tgt = schema_with_hyper_edge(&verts, &[("fan_pair", sig), ("fan_left_only", sig)]);

    let vertex_map: HashMap<Name, Name> = verts
        .iter()
        .map(|(v, _)| (Name::from(*v), Name::from(*v)))
        .collect();

    let mut hyper_resolver = HashMap::new();
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

    let mig = Migration {
        vertex_map,
        edge_map: HashMap::new(),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver,
        expr_resolvers: HashMap::new(),
        domain: None,
        codomain: None,
    };

    (src, tgt, mig)
}

#[test]
fn compilation_keeps_every_fan_shape() {
    let (src, tgt, mig) = two_shape_migration();
    let compiled = compile(&src, &tgt, &mig).expect("compile should succeed");

    assert_eq!(
        compiled.hyper_resolver.len(),
        2,
        "both fan shapes must survive compilation, got {:?}",
        compiled.hyper_resolver
    );

    let pair = compiled
        .hyper_resolver
        .get(&(
            Name::from("fan"),
            vec![Name::from("left"), Name::from("right")],
        ))
        .expect("the two-label shape is compiled");
    assert_eq!(pair.0, Name::from("fan_pair"));

    let single = compiled
        .hyper_resolver
        .get(&(Name::from("fan"), vec![Name::from("left")]))
        .expect("the one-label shape is compiled");
    assert_eq!(single.0, Name::from("fan_left_only"));
}

/// Total-order view of a compiled hyper-resolver, so two runs can be compared
/// without the comparison itself depending on hash order.
fn canonicalize(compiled: &panproto_inst::CompiledMigration) -> BTreeMap<String, String> {
    compiled
        .hyper_resolver
        .iter()
        .map(|((he, shape), (tgt, labels))| {
            let shape: Vec<&str> = shape.iter().map(Name::as_str).collect();
            let labels: BTreeMap<&str, &str> = labels
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            (format!("{he}{shape:?}"), format!("{tgt} {labels:?}"))
        })
        .collect()
}

#[test]
fn compilation_is_stable_across_runs() {
    // Each `compile` call builds fresh maps whose iteration order is
    // randomized per process; the compiled table must not depend on it.
    let (src, tgt, mig) = two_shape_migration();
    let canonical = canonicalize(&compile(&src, &tgt, &mig).expect("compile should succeed"));

    assert_eq!(canonical.len(), 2);

    for _ in 0..256 {
        let (src, tgt, mig) = two_shape_migration();
        let again = compile(&src, &tgt, &mig).expect("compile should succeed");
        assert_eq!(
            canonicalize(&again),
            canonical,
            "the compiled hyper-resolver must be order-independent"
        );
    }
}

#[test]
fn a_label_set_permutation_is_the_same_shape() {
    // The label set is a set: listing it in another order names the same fan
    // shape rather than a second, competing entry.
    let (src, tgt, mut mig) = two_shape_migration();
    let pair = mig
        .hyper_resolver
        .remove(&(
            Name::from("fan"),
            vec![Name::from("left"), Name::from("right")],
        ))
        .expect("the two-label entry exists");
    mig.hyper_resolver.insert(
        (
            Name::from("fan"),
            vec![Name::from("right"), Name::from("left")],
        ),
        pair,
    );

    let compiled = compile(&src, &tgt, &mig).expect("compile should succeed");
    assert_eq!(compiled.hyper_resolver.len(), 2);
    assert!(
        compiled.hyper_resolver.contains_key(&(
            Name::from("fan"),
            vec![Name::from("left"), Name::from("right")],
        )),
        "the canonical shape is the sorted label set"
    );
}

#[test]
fn two_shapes_that_canonicalize_alike_are_rejected() {
    // Permuted label lists that disagree on their retarget are a genuine
    // ambiguity in the specification, not something to resolve by hash order.
    let (src, tgt, mut mig) = two_shape_migration();
    mig.hyper_resolver.insert(
        (
            Name::from("fan"),
            vec![Name::from("right"), Name::from("left")],
        ),
        (
            Name::from("fan_left_only"),
            HashMap::from([(Name::from("left"), Name::from("only"))]),
        ),
    );

    let err = compile(&src, &tgt, &mig).expect_err("conflicting shapes must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("conflicting entries"),
        "expected a conflict report, got {message}"
    );
}
