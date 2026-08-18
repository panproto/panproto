//! What the maximum common sub-schema search does on the shape its bound
//! cannot see.
//!
//! The `iso` path is reached without a caller choosing it: `discover_overlap`
//! sets `iso: true` and `find_span` builds a `SpanSearch` that never calls
//! `with_budget`, so `DEFAULT_SEARCH_NODES` is the only ceiling. The B1 bound
//! reads the objective and the capacity of a label class and nothing else, so
//! on a source whose structure is carried by annotation maps rather than by
//! arcs it has nothing to prune with: every binary function such a shape poses
//! comes from an apex well-formedness constraint, whose table holds `⊤` and `⊥`
//! and pays no reward, so both half charges are zero and the whole bound is the
//! sum of the per-vertex maxima. That sum assumes all nine source vertices are
//! mapped at once, where the eleven constraints admit one.
//!
//! Reading those constraints is what closes it. A vertex tied by `⊤` to one the
//! search has already dropped can never be mapped, so it leaves the class it
//! sits in, and the capacity the bound is stated over falls with it. The two
//! numbers below are the same search on the same pair with and without that
//! step, and they are three orders of magnitude apart:
//!
//! | | nodes | wall time | answer | proved |
//! |---|---|---|---|---|
//! | capacity alone | 10,000,000 (the ceiling) | 60.5 s | nothing mapped | no |
//! | reading the constraints | 102 | 1.2 ms | one vertex, strictly cheaper | yes |
//!
//! So this file pins the closure rather than the ceiling: the search finishes
//! inside a budget far below the default, proves what it returns, and collapses
//! its quality interval. A budget that cannot close any shape is exercised by
//! `a_stopped_search_is_not_an_answer`, which is where the contract for a
//! spent budget lives.

#![allow(clippy::expect_used)]

use panproto_gat::Name;
use panproto_mig::hom_search::SearchOptions;
use panproto_mig::solve::{SearchBudget, SolverPath};
use panproto_mig::span::SpanSearch;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder, Span, Variant};

/// One object kind and one edge kind, so kinds constrain nothing and the
/// annotation maps are the only structure the network sees.
fn protocol() -> Protocol {
    Protocol {
        name: "test-iso-budget".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".to_owned(),
            src_kinds: vec!["object".to_owned()],
            tgt_kinds: vec!["object".to_owned()],
        }],
        obj_kinds: vec!["object".to_owned()],
        constraint_sorts: vec![],
        has_coproducts: true,
        ..Protocol::default()
    }
}

/// `n` vertices under one prefix, with no edges.
fn bare(prefix: &str, n: usize) -> Schema {
    let protocol = protocol();
    let mut builder = SchemaBuilder::new(&protocol);
    for index in 0..n {
        builder = builder
            .vertex(&format!("{prefix}{index}"), "object", None::<&str>)
            .expect("vertex");
    }
    builder.build().expect("build")
}

/// The shape the fuzzer found: nine vertices, one edge, seven coproduct arms
/// and four schema spans.
///
/// `SchemaBuilder` has no way to set `variants` or `spans`, which is why no
/// builder-built fixture and no corpus pair reaches this path. They are written
/// onto the built schema directly, exactly as a deserialised schema carries
/// them.
fn annotation_dense() -> Schema {
    let protocol = protocol();
    let mut schema = SchemaBuilder::new(&protocol);
    for index in 0..9 {
        schema = schema
            .vertex(&format!("a{index}"), "object", None::<&str>)
            .expect("vertex");
    }
    let mut schema = schema
        .edge("a0", "a1", "prop", Some("p"))
        .expect("edge")
        .build()
        .expect("build");

    for index in 0..7 {
        schema.variants.insert(
            Name::from(format!("a{index}").as_str()),
            vec![Variant {
                id: Name::from(format!("a{}", index + 1).as_str()),
                parent_vertex: Name::from(format!("a{index}").as_str()),
                tag: None,
            }],
        );
    }
    for index in 0..4 {
        schema.spans.insert(
            Name::from(format!("s{index}").as_str()),
            Span {
                id: Name::from(format!("s{index}").as_str()),
                left: Name::from(format!("a{index}").as_str()),
                right: Name::from(format!("a{}", index + 4).as_str()),
            },
        );
    }
    schema
}

/// The shape closes, and closes small.
///
/// Twenty thousand nodes is the budget the unrepaired search could not close
/// this in, and it is left here on purpose: it is two hundred times what the
/// search now spends and still five hundred times less than the default, so a
/// regression in either direction fails rather than passes slowly.
#[test]
fn the_annotation_dense_shape_closes_well_inside_its_budget() {
    let protocol = protocol();
    let src = annotation_dense();
    let tgt = bare("b", 9);

    let search = SpanSearch::new(&protocol)
        .with_options(SearchOptions {
            iso: true,
            ..SearchOptions::default()
        })
        .with_budget(SearchBudget::default().with_max_nodes(Some(20_000)));
    let span = search.run(&src, &tgt).expect("the span search is total");

    assert!(
        matches!(span.certificate.path, SolverPath::Iso),
        "the fixture must exercise the maximum common sub-schema path, got {:?}",
        span.certificate.path
    );
    assert_eq!(
        span.certificate.limit_hit, None,
        "reading the hard constraints is what closes this shape, and a search \
         that spends twenty thousand nodes on it is not reading them"
    );
    assert!(
        span.certificate.proven_optimal,
        "a search that finished has proved its answer, and the quality \
         interval below is only meaningful because it did"
    );

    // Stated as a width rather than as an equality, because these are quotients
    // of integer costs and comparing two of them for equality is the thing the
    // float lint exists to refuse.
    let (low, high) = span.quality_bounds;
    assert!(
        high - low <= f64::EPSILON,
        "a proof collapses the interval; a spread of {} here would mean the \
         bounds disagree with the flag beside them",
        high - low
    );
    assert!((low..=high).contains(&span.quality));
}

/// The apex it returns is a sub-schema of the source, and not the empty one.
///
/// The unrepaired search spent ten million nodes and returned the all-`⊥`
/// mapping, so "it finishes" and "it finishes with something" are separate
/// claims and both are made.
#[test]
fn the_apex_it_proves_is_not_the_empty_one() {
    let protocol = protocol();
    let src = annotation_dense();
    let tgt = bare("b", 9);

    let span = SpanSearch::new(&protocol)
        .with_options(SearchOptions {
            iso: true,
            ..SearchOptions::default()
        })
        .run(&src, &tgt)
        .expect("the span search is total");

    assert!(
        !span.apex.vertices.is_empty(),
        "the eleven constraints rule out most of this source, and exactly one \
         vertex survives them; an empty apex means the search gave up rather \
         than answered"
    );
    for name in span.apex.vertices.keys() {
        assert!(
            src.vertices.contains_key(name),
            "the apex vertex {name} is not a source vertex"
        );
    }
}

/// And the same pair on the default and injective routes answers in the small
/// change, which is what makes the iso path's cost the iso path's.
#[test]
fn the_other_two_routes_answer_the_same_pair_without_a_budget_fight() {
    let protocol = protocol();
    let src = annotation_dense();
    let tgt = bare("b", 9);

    for options in [
        SearchOptions::default(),
        SearchOptions {
            monic: true,
            ..SearchOptions::default()
        },
    ] {
        let span = SpanSearch::new(&protocol)
            .with_options(options.clone())
            .run(&src, &tgt)
            .expect("the span search is total");
        assert_eq!(
            span.certificate.limit_hit, None,
            "monic={} must finish inside the default budget on this shape",
            options.monic
        );
        assert!(span.certificate.proven_optimal);
    }
}
