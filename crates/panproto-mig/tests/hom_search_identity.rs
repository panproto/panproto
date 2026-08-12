//! Smoke test for the public homomorphism-search surface of
//! `panproto-mig`.
//!
//! It pins the one answer the search can never get wrong: searching a
//! schema against itself, where each vertex has a distinct kind, admits
//! exactly one kind-respecting assignment, and that assignment is the
//! identity. Any regression that shuffles the vertex map, drops an edge
//! from the edge map, or ranks a total structure-preserving map below
//! some other morphism shows up here before it reaches the corpus tests.
//!
//! Optimality is asserted against the hom-set the search itself
//! enumerates rather than against a hardcoded quality threshold. A
//! threshold is a snapshot of today's weight vector: `CostWeights`
//! normalises over five components while the reported quality sums four
//! of them, so the ceiling for a perfect match is `1 - anchor`, and any
//! anchor weight above `0.01` would fail a `> 0.99` assertion on a match
//! that is exactly right.

#![allow(clippy::expect_used)]

use panproto_mig::hom_search::{SearchOptions, find_best_morphism, find_morphisms};
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

/// A protocol with one kind per structural role, so a vertex's kind
/// determines its image uniquely in the schema below.
fn generic_protocol() -> Protocol {
    Protocol {
        name: "test-generic".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: vec![
            EdgeRule {
                edge_kind: "record-schema".to_owned(),
                src_kinds: vec!["record".to_owned()],
                tgt_kinds: vec!["object".to_owned()],
            },
            EdgeRule {
                edge_kind: "prop".to_owned(),
                src_kinds: vec!["object".to_owned()],
                tgt_kinds: vec!["string".to_owned(), "integer".to_owned()],
            },
        ],
        obj_kinds: vec![
            "record".to_owned(),
            "object".to_owned(),
            "string".to_owned(),
            "integer".to_owned(),
        ],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

/// A four-vertex schema whose vertices have four distinct kinds.
fn post_schema(protocol: &Protocol) -> Schema {
    SchemaBuilder::new(protocol)
        .vertex("post", "record", Some("app.test.post"))
        .expect("vertex post")
        .vertex("body", "object", None)
        .expect("vertex body")
        .vertex("text", "string", None)
        .expect("vertex text")
        .vertex("count", "integer", None)
        .expect("vertex count")
        .edge("post", "body", "record-schema", None)
        .expect("edge record-schema")
        .edge("body", "text", "prop", Some("text"))
        .expect("edge text")
        .edge("body", "count", "prop", Some("count"))
        .expect("edge count")
        .entry("post")
        .build()
        .expect("build")
}

#[test]
fn identity_pair_yields_the_identity_morphism() {
    let protocol = generic_protocol();
    let schema = post_schema(&protocol);

    let found = find_best_morphism(&schema, &schema, &SearchOptions::default())
        .expect("a schema always maps to itself");

    assert_eq!(
        found.vertex_map.len(),
        schema.vertices.len(),
        "the morphism must be total on vertices"
    );
    for (src, tgt) in &found.vertex_map {
        assert_eq!(src, tgt, "vertex {src} must map to itself");
    }

    assert_eq!(
        found.edge_map.len(),
        schema.edges.len(),
        "the morphism must be total on edges"
    );
    for (src, tgt) in &found.edge_map {
        assert_eq!(src, tgt, "edge {src:?} must map to itself");
    }

    // The identity attains the optimum of whatever objective the search
    // is ranking on. Comparing against the hom-set rather than against a
    // constant keeps the assertion true across a reweighting and turns a
    // future failure into a statement about the objective.
    let all = find_morphisms(&schema, &schema, &SearchOptions::default());
    assert!(!all.is_empty(), "the identity must be enumerated");
    let best = all
        .iter()
        .map(|m| m.quality)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        found.quality >= best,
        "the identity must attain the optimum: got {}, best enumerated {best}",
        found.quality
    );
    for other in &all {
        assert!(
            other.vertex_map.len() <= found.vertex_map.len(),
            "no morphism may cover more of the source than the identity does"
        );
    }
}

#[test]
fn monic_search_also_finds_the_identity() {
    let protocol = generic_protocol();
    let schema = post_schema(&protocol);

    let opts = SearchOptions {
        monic: true,
        ..SearchOptions::default()
    };
    let found = find_best_morphism(&schema, &schema, &opts)
        .expect("the identity is injective, so the monic search must find it");

    // Totality first: a map that is empty or partial would satisfy the
    // per-pair assertion below vacuously, so a regression that returned
    // one would pass silently without this.
    assert_eq!(
        found.vertex_map.len(),
        schema.vertices.len(),
        "the monic search must return a total vertex map"
    );
    assert_eq!(
        found.edge_map.len(),
        schema.edges.len(),
        "the monic search must return a total edge map"
    );
    for (src, tgt) in &found.vertex_map {
        assert_eq!(
            src, tgt,
            "vertex {src} must map to itself under monic search"
        );
    }
}
