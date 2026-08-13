//! Smoke test for the public homomorphism-search surface of
//! `panproto-mig`.
//!
//! It pins the one answer the search can never get wrong: searching a
//! schema against itself, where each vertex has a distinct kind, admits
//! exactly one kind-respecting assignment, and that assignment is the
//! identity. Any regression that shuffles the vertex map or drops an edge
//! from the edge map shows up here before it reaches the corpus tests.
//!
//! The singleton hom-set is the premise, so it is asserted rather than
//! assumed: `domain_of` filters candidate targets by kind, and if it ever
//! stopped doing so every search in the crate would silently widen and
//! `assert_eq!(src, tgt)` on each vertex would become a coincidence rather
//! than a consequence.
//!
//! That premise is also why the reported quality is pinned against a
//! computed ceiling here and not against the hom-set. Comparing the one
//! element of a singleton set against the maximum over that same set is
//! `x >= x`, which no regression can fail; the perfect-match ceiling is
//! `1 - anchor`, because `CostWeights` normalises over five components
//! while the reported quality sums four of them, and reading it off
//! `DEFAULT_WEIGHTS` keeps the assertion true across a reweighting without
//! being vacuous. Genuine optimality-under-choice belongs on a pair with a
//! real tie, and `hom_search`'s own unit tests cover it.

#![allow(clippy::expect_used)]

use panproto_mig::DEFAULT_WEIGHTS;
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
        .expect("the network poses")
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

    // The premise of every assertion above: four distinct kinds leave one
    // kind-respecting assignment, so the identity is not merely the best
    // answer, it is the only one. This fires if `domain_of` ever stops
    // filtering candidates by kind.
    let all =
        find_morphisms(&schema, &schema, &SearchOptions::default()).expect("the network poses");
    assert_eq!(
        all.len(),
        1,
        "distinct kinds leave one kind-respecting assignment, so the hom-set is a singleton"
    );

    // A perfect match reads the ceiling of the reported quality, which is
    // `1 - anchor`: the weights normalise over five components and the
    // quality sums four. Computing it from the weight vector rather than
    // hardcoding it is what keeps this true across a reweighting.
    let ceiling = 1.0 - DEFAULT_WEIGHTS.anchor();
    assert!(
        (found.quality - ceiling).abs() < 1e-12,
        "the identity must read the perfect-match ceiling {ceiling}: got {}",
        found.quality
    );
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
        .expect("the network poses")
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
