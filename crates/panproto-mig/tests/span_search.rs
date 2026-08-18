//! End-to-end tests for the span search, over the public surface alone.
//!
//! The unit tests inside `span.rs` reach into the construction; these use only
//! what a caller can. What they pin is the contract rather than the
//! implementation: the search never refuses, the total morphism is the
//! degenerate case rather than a separate answer, the apex is a schema in its
//! own right, and both legs are genuine morphisms.

#![allow(clippy::expect_used)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_mig::hom_search::{SearchOptions, find_morphisms, find_span};
use panproto_mig::{Migration, SchemaSpan, SpanSearch, check_migration_morphism, discover_overlap};
use panproto_schema::{
    EdgeRule, Protocol, Schema, SchemaBuilder, canonical_digest, schema_pushout, validate,
};

/// One kind per structural role, so a vertex's kind carries information.
fn protocol() -> Protocol {
    Protocol {
        name: "test-span".to_owned(),
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

/// A record whose body carries the named properties, each of the given kind.
fn record(prefix: &str, props: &[(&str, &str)]) -> Schema {
    let protocol = protocol();
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex(&format!("{prefix}root"), "record", None::<&str>)
        .expect("vertex root")
        .vertex(&format!("{prefix}body"), "object", None::<&str>)
        .expect("vertex body")
        .edge(
            &format!("{prefix}root"),
            &format!("{prefix}body"),
            "record-schema",
            None::<&str>,
        )
        .expect("edge record-schema");

    for (name, kind) in props {
        let field = format!("{prefix}f.{name}");
        builder = builder
            .vertex(&field, kind, None::<&str>)
            .expect("vertex field")
            .edge(&format!("{prefix}body"), &field, "prop", Some(*name))
            .expect("edge prop");
    }
    builder
        .entry(&format!("{prefix}root"))
        .build()
        .expect("build")
}

/// The empty schema, which `SchemaBuilder` refuses to produce.
fn empty() -> Schema {
    Schema {
        protocol: "test-span".to_owned(),
        vertices: HashMap::new(),
        edges: HashMap::new(),
        hyper_edges: HashMap::new(),
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

/// The span's own statement that its left leg is an inclusion, checked rather
/// than trusted.
fn assert_left_leg_is_an_inclusion(span: &SchemaSpan) {
    assert_eq!(span.left.vertex_map.len(), span.apex.vertices.len());
    assert_eq!(span.left.edge_map.len(), span.apex.edges.len());
    for (source, image) in &span.left.vertex_map {
        assert_eq!(source, image, "the left leg renames nothing");
        assert!(
            span.apex.vertices.contains_key(source),
            "and maps only what the apex holds"
        );
    }
    for (source, image) in &span.left.edge_map {
        assert_eq!(source, image);
    }
}

#[test]
fn a_total_morphism_is_the_degenerate_span() {
    let schema = record("a:", &[("text", "string"), ("count", "integer")]);
    let protocol = protocol();

    let span = find_span(&schema, &schema, &protocol, &SearchOptions::default())
        .expect("a schema always spans onto itself");

    assert!(span.is_total());
    assert!((span.apex_coverage - 1.0).abs() < f64::EPSILON);
    assert_left_leg_is_an_inclusion(&span);

    let total = span
        .as_total_morphism()
        .expect("a total span carries a morphism");
    for (source, image) in &total.vertex_map {
        assert_eq!(source, image, "the identity attains the optimum");
    }
    assert_eq!(total.edge_map.len(), schema.edges.len());

    // And the total-morphism entry point agrees, because it is the same search.
    let found = find_morphisms(&schema, &schema, &SearchOptions::default())
        .expect("the network poses")
        .morphisms;
    assert!(!found.is_empty());
    assert!((found[0].quality - total.quality).abs() < 1e-12);
    assert_eq!(found[0].vertex_map, total.vertex_map);
}

#[test]
fn a_pair_with_no_total_morphism_still_spans() {
    // The target dropped the counter and renamed the text, so no total morphism
    // exists in either direction while three of the four vertices still match.
    let src = record("", &[("text", "string"), ("count", "integer")]);
    let tgt = record("", &[("body", "string")]);
    let protocol = protocol();

    assert!(
        find_morphisms(&src, &tgt, &SearchOptions::default())
            .expect("the network poses")
            .morphisms
            .is_empty(),
        "the counter has nowhere to go, so no total morphism exists"
    );

    let span = find_span(&src, &tgt, &protocol, &SearchOptions::default())
        .expect("the span search does not refuse");
    assert!(!span.is_total());
    assert_eq!(span.apex.vertices.len(), 3, "root, body and the string");
    assert!(!span.apex.vertices.contains_key("f.count"));
    assert!((span.apex_coverage - 0.75).abs() < 1e-12);
    assert!(span.certificate.proven_optimal);
    assert_left_leg_is_an_inclusion(&span);
}

#[test]
fn a_pair_with_zero_shared_kinds_returns_an_empty_apex_without_error() {
    let src = record("", &[("text", "string")]);
    // A schema whose only vertices are of a kind the source does not use.
    let tgt = SchemaBuilder::new(&protocol())
        .vertex("n", "integer", None::<&str>)
        .expect("vertex")
        .entry("n")
        .build()
        .expect("build");
    let protocol = protocol();

    let span = find_span(&src, &tgt, &protocol, &SearchOptions::default())
        .expect("an empty apex is a value, not an error");
    assert!(span.apex.vertices.is_empty());
    assert!(span.apex.edges.is_empty());
    assert!((span.apex_coverage - 0.0).abs() < f64::EPSILON);
    assert!(!span.is_total());
    assert!(span.as_total_morphism().is_none());
    assert!(span.certificate.legs_are_functorial, "vacuously, and truly");
}

#[test]
fn an_empty_source_returns_a_span() {
    let src = empty();
    let tgt = record("", &[("text", "string")]);
    let protocol = protocol();

    let span = find_span(&src, &tgt, &protocol, &SearchOptions::default())
        .expect("the empty source spans onto anything");
    assert!(span.apex.vertices.is_empty());
    assert!(span.is_total(), "the empty inclusion is onto");
    assert!(!span.certificate.apex_pointed, "and it has no entry point");
}

#[test]
fn both_legs_are_morphisms_and_the_right_leg_exists() {
    let src = record("", &[("text", "string"), ("count", "integer")]);
    let tgt = record("", &[("body", "string"), ("likes", "integer")]);
    let protocol = protocol();

    let span = find_span(&src, &tgt, &protocol, &SearchOptions::default()).expect("a span");

    assert!(span.certificate.legs_are_functorial);
    assert!(
        check_migration_morphism(&span.apex, &src, &span.left).is_ok(),
        "the left leg is functorial"
    );
    assert!(
        check_migration_morphism(&span.apex, &tgt, &span.right).is_ok(),
        "and so is the right"
    );
    assert!(
        span.certificate.right_existence.valid,
        "existence reported {:?}",
        span.certificate.right_existence.errors
    );

    // Every apex edge has an image, and it runs between the images of its own
    // endpoints. That is naturality, checked directly rather than through the
    // certificate.
    assert_eq!(span.right.edge_map.len(), span.apex.edges.len());
    for (edge, image) in &span.right.edge_map {
        assert_eq!(image.kind, edge.kind);
        assert_eq!(
            span.right.vertex_map.get(&edge.src),
            Some(&image.src),
            "the image runs from the image of the source"
        );
        assert_eq!(span.right.vertex_map.get(&edge.tgt), Some(&image.tgt));
    }
}

#[test]
fn the_apex_is_a_schema_in_its_own_right() {
    let src = record("", &[("text", "string"), ("count", "integer")]);
    let tgt = record("", &[("body", "string")]);
    let protocol = protocol();

    let span = find_span(&src, &tgt, &protocol, &SearchOptions::default()).expect("a span");

    assert!(
        validate(&span.apex, &protocol).is_empty(),
        "the apex validates against the protocol"
    );
    assert!(!span.apex.edges.is_empty());
    for edge in span.apex.edges.keys() {
        assert!(
            span.apex
                .edges_between(edge.src.as_str(), edge.tgt.as_str())
                .contains(edge),
            "the adjacency indexes were rebuilt, not copied"
        );
        assert!(span.apex.outgoing_edges(edge.src.as_str()).contains(edge));
        assert!(span.apex.incoming_edges(edge.tgt.as_str()).contains(edge));
    }
    assert!(
        span.apex
            .entries
            .iter()
            .all(|entry| span.apex.vertices.contains_key(entry)),
        "no entry names a dropped vertex"
    );
}

#[test]
fn the_legs_carry_content_endpoints() {
    let src = record("", &[("text", "string")]);
    let tgt = record("", &[("body", "string")]);
    let protocol = protocol();

    let span = find_span(&src, &tgt, &protocol, &SearchOptions::default()).expect("a span");

    let apex = span.apex_digest_hex();
    assert_eq!(
        span.left.domain.as_ref().map(Name::as_str),
        Some(apex.as_str())
    );
    assert_eq!(
        span.right.domain.as_ref().map(Name::as_str),
        Some(apex.as_str()),
        "both legs leave the same apex"
    );
    assert_eq!(span.certificate.apex_digest, canonical_digest(&span.apex));
    assert_ne!(
        span.left.codomain, span.right.codomain,
        "and land on different schemas"
    );

    // The endpoints are what makes composition checkable rather than skipped.
    let onward = Migration {
        domain: span.right.codomain.clone(),
        ..Migration::empty()
    };
    assert_eq!(span.right.codomain, onward.domain);
}

#[test]
fn the_span_is_deterministic() {
    let src = record("", &[("text", "string"), ("count", "integer")]);
    let tgt = record("", &[("body", "string"), ("total", "integer")]);
    let protocol = protocol();

    let first = find_span(&src, &tgt, &protocol, &SearchOptions::default()).expect("a span");
    for _ in 0..16 {
        let again = find_span(&src, &tgt, &protocol, &SearchOptions::default()).expect("a span");
        assert_eq!(first.certificate.apex_digest, again.certificate.apex_digest);
        assert_eq!(first.right.vertex_map, again.right.vertex_map);
        assert_eq!(first.right.edge_map, again.right.edge_map);
        assert!((first.quality - again.quality).abs() < f64::EPSILON);
        assert_eq!(first.quality_bounds, again.quality_bounds);
    }
}

#[test]
fn the_apex_merges_the_two_schemas() {
    let src = record("", &[("text", "string")]);
    let tgt = record("", &[("body", "string")]);
    let protocol = protocol();

    let span = find_span(&src, &tgt, &protocol, &SearchOptions::default()).expect("a span");
    let overlap = span.to_overlap();
    assert_eq!(overlap.vertex_pairs.len(), span.apex.vertices.len());

    let (merged, into_src, into_tgt) = span.pushout(&src, &tgt).expect("the pushout exists");
    assert!(!merged.vertices.is_empty());
    assert_eq!(into_src.vertex_map.len(), src.vertices.len());
    assert_eq!(into_tgt.vertex_map.len(), tgt.vertices.len());

    // And it is the same merge `schema_pushout` computes from the overlap
    // directly, which is what `discover_overlap` hands its callers.
    let (direct, _, _) = schema_pushout(&src, &tgt, &overlap).expect("the pushout exists");
    assert_eq!(canonical_digest(&merged), canonical_digest(&direct));
}

#[test]
fn discover_overlap_finds_what_neither_schema_wholly_contains() {
    // Each side has a property the other lacks, so neither embeds in the other
    // and the two-total-searches version of this returned nothing at all.
    let left = record("", &[("name", "string"), ("count", "integer")]);
    let right = record("", &[("name", "string"), ("slug", "string")]);
    let protocol = protocol();

    let overlap = discover_overlap(&left, &right, &protocol).expect("an overlap");
    assert_eq!(
        overlap.vertex_pairs.len(),
        3,
        "the record, the body and the shared string"
    );
    assert!(
        overlap
            .vertex_pairs
            .iter()
            .any(|(l, r)| l.as_str() == "f.name" && r.as_str() == "f.name")
    );
    assert!(schema_pushout(&left, &right, &overlap).is_ok());
}

#[test]
fn the_iso_path_returns_a_monic_right_leg() {
    let left = record("a:", &[("name", "string")]);
    let right = record("b:", &[("label", "string")]);
    let protocol = protocol();
    let opts = SearchOptions {
        iso: true,
        ..SearchOptions::default()
    };

    let span = find_span(&left, &right, &protocol, &opts).expect("a span");
    assert!(
        span.certificate.shape.right_is_mono,
        "a symmetric lens needs the right leg injective"
    );
    assert_eq!(span.apex.vertices.len(), left.vertices.len());
    assert!(span.is_total(), "the two are isomorphic");
}

#[test]
fn optima_all_attain_the_optimum() {
    // Two indistinguishable string properties on the target, so which one the
    // source's string takes is a genuine tie.
    let src = record("", &[("a", "string")]);
    let tgt = record("", &[("x", "string"), ("y", "string")]);
    let protocol = protocol();
    let search = SpanSearch::new(&protocol);

    let one = search.run(&src, &tgt).expect("a span");
    let all = search.optima(&src, &tgt, 16).expect("the optima");
    assert!(!all.is_empty());
    for span in &all {
        assert!((span.quality - one.quality).abs() < f64::EPSILON);
        assert_eq!(span.apex.vertices.len(), one.apex.vertices.len());
    }
    assert!(
        all.iter()
            .any(|span| span.right.vertex_map == one.right.vertex_map),
        "the canonical answer is among them"
    );
}

/// A discovered overlap must not identify two apex arcs with one target arc.
///
/// `discover_overlap` searches for a maximum common induced sub-schema, whose
/// apex is a proper part of the target for essentially every non-isomorphic
/// pair. A surjectivity test taken against the *whole* target therefore
/// discards the edge bijection that was just built and falls back to a greedy
/// image, which sends both parallel arcs to the first target arc of their kind.
/// The pushout then keeps one preimage of the collision and invents a fourth
/// arc that is in neither input.
#[test]
fn a_discovered_overlap_does_not_identify_two_arcs_with_one() {
    let protocol = protocol();
    let left = SchemaBuilder::new(&protocol)
        .vertex("a", "object", None::<&str>)
        .expect("a")
        .vertex("b", "string", None::<&str>)
        .expect("b")
        .edge("a", "b", "prop", Some("p"))
        .expect("p")
        .edge("a", "b", "prop", Some("q"))
        .expect("q")
        .entry("a")
        .build()
        .expect("left");
    let right = SchemaBuilder::new(&protocol)
        .vertex("x", "object", None::<&str>)
        .expect("x")
        .vertex("y", "string", None::<&str>)
        .expect("y")
        .vertex("z", "integer", None::<&str>)
        .expect("z")
        .edge("x", "y", "prop", Some("m"))
        .expect("m")
        .edge("x", "y", "prop", Some("n"))
        .expect("n")
        .edge("x", "z", "prop", Some("zz"))
        .expect("zz")
        .entry("x")
        .build()
        .expect("right");

    let overlap = discover_overlap(&left, &right, &protocol).expect("an overlap");
    let mut images: Vec<&panproto_schema::Edge> =
        overlap.edge_pairs.iter().map(|(_, right)| right).collect();
    let before = images.len();
    images.sort_unstable();
    images.dedup();
    assert_eq!(
        images.len(),
        before,
        "two left arcs were identified with one right arc: {:?}",
        overlap.edge_pairs
    );

    let (merged, _, _) = schema_pushout(&left, &right, &overlap).expect("a pushout");
    assert_eq!(
        merged.edges.len(),
        3,
        "merging {{p, q}} with {{m, n, zz}} along {{p~m, q~n}} gives three arcs, \
         not four: {:?}",
        merged.edges.keys().collect::<Vec<_>>()
    );
}

/// Merging along a span whose right leg contracts is refused rather than
/// answered with a square that does not commute.
#[test]
fn a_contracting_span_has_no_pushout() {
    // Four source strings, one target string: the optimal right leg sends all
    // four onto it, which is an ordinary answer rather than a pathological one.
    let src = record("", &[("title", "string"), ("subtitle", "string")]);
    let tgt = record("", &[("heading", "string")]);
    let protocol = protocol();

    let span = find_span(&src, &tgt, &protocol, &SearchOptions::default()).expect("a span");
    assert!(
        !span.certificate.shape.right_is_mono,
        "the fixture must produce a contracting right leg: {:?}",
        span.right.vertex_map
    );

    let refused = span.pushout(&src, &tgt);
    assert!(
        matches!(refused, Err(panproto_mig::SpanError::ContractingRightLeg)),
        "a merge along a contracting leg does not commute, so it must be \
         refused rather than returned: got {refused:?}"
    );

    // And a span whose right leg embeds still merges, so the refusal is about
    // the leg rather than about the entry point.
    let same = record("", &[("title", "string")]);
    let embedding = find_span(&same, &same, &protocol, &SearchOptions::default()).expect("a span");
    assert!(embedding.certificate.shape.right_is_mono);
    assert!(embedding.pushout(&same, &same).is_ok());
}
