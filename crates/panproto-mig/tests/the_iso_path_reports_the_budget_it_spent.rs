//! What the maximum common sub-schema search does when its bound does not
//! prune, and what it says about it.
//!
//! The `iso` path is reached without a caller choosing it: `discover_overlap`
//! sets `iso: true` and `find_span` builds a `SpanSearch` that never calls
//! `with_budget`, so `DEFAULT_SEARCH_NODES` is the only ceiling. On a source
//! carrying the annotation maps `SchemaBuilder` cannot set (variants, schema
//! spans) the B1 bound stops pruning almost entirely and the search runs to
//! that ceiling. Measured on the nine-vertex pair in
//! `fuzz/artifacts/span_search`: ten million nodes, 562 prune events, about
//! fifteen seconds, and no proof of optimality at the end.
//!
//! That is a real cost and it is not repaired here, because every repair
//! reachable from the budget was measured worse: a lower node cap returns the
//! empty apex where the full run returns a three-vertex one, and a default
//! wall-clock deadline makes the answer a function of the machine, which
//! `the_span_is_a_function_of_the_pair` forbids. The bound is what needs work,
//! and the diagnosis that reached this file, that the half charges over-charge,
//! is measurably not the cause: every binary function this shape poses comes
//! from an apex hard constraint and pays no reward at all, so both half charges
//! are zero and the whole of the root bound is the sum of the per-vertex
//! maxima. The slack is that this sum assumes all nine source vertices are
//! mapped at once, where the eleven hard constraints admit three.
//!
//! What is pinned here is therefore the contract rather than the performance:
//! the ceiling is honoured, the shortfall is reported rather than absorbed, and
//! the quality interval widens to say so. A budget far below the default keeps
//! the test fast while exercising the same path; the shape is what matters, and
//! it is the shape no builder-built schema can reach.

#![allow(clippy::expect_used)]

use panproto_gat::Name;
use panproto_mig::hom_search::SearchOptions;
use panproto_mig::solve::{LimitKind, SearchBudget, SolverPath};
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

/// The node ceiling is honoured, and the search says it stopped.
///
/// The assertion that matters is the conjunction. A search that stops without a
/// proof and reports `proven_optimal: true` would be laundering a budget into a
/// claim, which is the pattern three defects on this branch have taken.
#[test]
fn a_search_that_spends_its_node_budget_says_so() {
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
        span.certificate.limit_hit,
        Some(LimitKind::Nodes),
        "twenty thousand nodes do not close this shape, which is the premise of \
         everything below"
    );
    assert!(
        !span.certificate.proven_optimal,
        "a search that stopped on a budget has proved nothing, and saying \
         otherwise turns a spent budget into a claim about the pair"
    );

    let (low, high) = span.quality_bounds;
    assert!(
        low <= span.quality && span.quality <= high,
        "the reported quality must lie inside its own interval: {low} <= \
         {} <= {high}",
        span.quality
    );
    assert!(
        low < high,
        "an unproved answer must widen the interval rather than collapse it, \
         since a collapsed interval is what `proven_optimal` means"
    );
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
