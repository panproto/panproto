//! A wrong strategy anchor must cost neither coverage nor quality.
//!
//! Alignment strategies are heuristics, and their output is merged into
//! [`SearchOptions::hard_pins`], whose contract reserves it for mappings a
//! caller *knows*. A pin collapses its vertex's domain to that one target and
//! `⊥`, so a pin that is individually plausible but jointly infeasible with the
//! rest does not make the search fail: the vertex becomes unmappable and the
//! optimum drops it. The span comes back optimal, `proven_optimal` is true, and
//! the field is simply missing from the generated lens.
//!
//! That is why the retry cannot be conditioned on the search erroring, which is
//! what it used to be. At a tier that allows spans the search answers `⊥`
//! rather than refusing, so the error arm never runs.
//!
//! Coverage is not the whole of it either. Releasing a pin only ever adds
//! values back to a domain, so the released search optimises over a superset
//! and its optimum is never worse on the objective — which is
//! `(quality_cost, drops)` read lexicographically, quality first. A comparison
//! that reads only the drop half keeps the pinned answer whenever releasing
//! raises the quality without changing how many vertices were mapped. This
//! file covers the coverage half. The quality half needs schemas the
//! strategies get partly wrong, which a fixture small enough to write out by
//! hand does not supply, so it lives in
//! `tests/integration/tests/strategy_pins_never_cost_quality.rs` against a
//! corpus pair.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_lens::auto_lens::{AutoLensConfig, Stringency, auto_generate_candidates};
use panproto_mig::hom_search::{SearchOptions, find_span};
use panproto_schema::{Protocol, Schema, SchemaBuilder};

fn protocol() -> Protocol {
    Protocol {
        name: "test".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: vec![],
        obj_kinds: vec![
            "record".to_owned(),
            "string".to_owned(),
            "integer".to_owned(),
            "boolean".to_owned(),
        ],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

fn build(vertices: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    for (id, kind) in vertices {
        builder = builder.vertex(id, kind, None::<&str>).expect("vertex");
    }
    for (src, tgt, kind, name) in edges {
        builder = builder.edge(src, tgt, kind, Some(*name)).expect("edge");
    }
    builder.build().expect("build")
}

#[test]
fn a_strategy_anchor_never_costs_coverage_the_unpinned_search_would_have_had() {
    let proto = protocol();

    // `name_1` matches `name_1` by exact identifier, so the exact-match
    // strategy pins it. That pin makes `name_0`'s edges infeasible, and the
    // optimum then drops `name_0` rather than mapping it.
    let src = build(
        &[("name_0", "string"), ("name_1", "integer")],
        &[
            ("name_0", "name_1", "prop", "author"),
            ("name_1", "name_0", "prop", "flag"),
        ],
    );
    let tgt = build(
        &[
            ("author_3", "record"),
            ("body_0", "integer"),
            ("count_2", "string"),
            ("name_1", "integer"),
        ],
        &[
            ("author_3", "name_1", "prop", "count"),
            ("body_0", "count_2", "prop", "flag"),
            ("body_0", "name_1", "prop", "title"),
            ("count_2", "body_0", "prop", "body"),
            ("count_2", "name_1", "prop", "count"),
        ],
    );

    let unpinned = find_span(&src, &tgt, &proto, &SearchOptions::default()).expect("a span");
    assert_eq!(
        unpinned.apex.vertices.len(),
        src.vertices.len(),
        "the premise: with no pins the search covers the whole source"
    );

    let config = AutoLensConfig {
        stringency: Stringency::Lenient,
        ..AutoLensConfig::default()
    };
    let candidates = auto_generate_candidates(&src, &tgt, &proto, &config, 1).expect("candidates");

    // `LensCandidate::coverage` is `|matched| / max(|src|, |tgt|)`, so the
    // number the unpinned span earns is computable rather than guessed.
    let count = |n: usize| f64::from(u32::try_from(n).expect("a small fixture"));
    let expected =
        count(unpinned.apex.vertices.len()) / count(src.vertex_count().max(tgt.vertex_count()));
    assert!(
        (candidates[0].coverage - expected).abs() < 1e-12,
        "the strategy anchors dropped a source vertex the unpinned search maps: \
         coverage {} against {expected}",
        candidates[0].coverage
    );
}
