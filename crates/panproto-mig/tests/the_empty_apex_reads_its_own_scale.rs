//! What an empty apex reads, on sources the measured corpus does not contain.
//!
//! [`SchemaSpan::quality`] documented a two-case rule: an empty apex over a
//! non-empty source reads `0.0`, and over an empty source `1.0`. The first is
//! true only of sources with at least one named edge, which is every schema in
//! the corpus and therefore every schema any committed test exercised.
//!
//! The objective normalises each component by what the source gives it to
//! measure. Name and degree are per source vertex, so they always charge. The
//! edge component is per source edge and the Jaccard component per source
//! vertex carrying a named outgoing edge, so a source with no edges charges
//! neither and a source whose edges are all unnamed charges only the first.
//! Under the default weights that is three readings, not two, and the two the
//! doc named are the extremes.
//!
//! Everything here is computed from [`DEFAULT_WEIGHTS`] rather than written as
//! a literal, so a reweighting moves the expectation with the code instead of
//! turning this into a puzzle. The tolerance is the rounding bound the same doc
//! states, `(|V_s| + |E_s|) / (2 · COST_SCALE)`.
//!
//! These readings are floors, not verdicts. Each is the worst value on its own
//! source's scale, and the scale is narrower the less structure the source
//! carries, which is exactly why quality is comparable among spans over one
//! source and not across sources.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use panproto_mig::hom_search::{SearchOptions, find_span};
use panproto_mig::solve::cost::{COST_SCALE, DEFAULT_WEIGHTS};
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

fn protocol() -> Protocol {
    Protocol {
        name: "empty-apex".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".to_owned(),
            src_kinds: vec!["object".to_owned()],
            tgt_kinds: vec!["object".to_owned()],
        }],
        obj_kinds: vec!["object".to_owned(), "blob".to_owned()],
        ..Protocol::default()
    }
}

/// How a fixture source is wired, which is what decides its scale.
#[derive(Copy, Clone)]
enum Edges {
    /// No edges at all: neither the edge nor the Jaccard component has mass.
    None,
    /// Edges present but unnamed: the edge component has mass, Jaccard does not.
    Unnamed,
    /// Named edges: every component has mass.
    Named,
}

/// A source of `n` vertices, chained according to `edges`.
fn source(n: usize, edges: Edges) -> Schema {
    let mut builder = SchemaBuilder::new(&protocol());
    for i in 0..n {
        builder = builder
            .vertex(&format!("v{i}"), "object", None::<&str>)
            .expect("vertex");
    }
    let name = match edges {
        Edges::None => None,
        Edges::Unnamed => Some(None),
        Edges::Named => Some(Some("e")),
    };
    if let Some(label) = name {
        for i in 1..n {
            builder = builder
                .edge(&format!("v{}", i - 1), &format!("v{i}"), "prop", label)
                .expect("edge");
        }
    }
    builder.entry("v0").build().expect("source")
}

/// A target sharing no vertex *kind* with the source, so no source vertex has a
/// candidate and the optimum is the assignment that keeps nothing.
fn unmatchable_target() -> Schema {
    SchemaBuilder::new(&protocol())
        .vertex("z", "blob", None::<&str>)
        .expect("z")
        .entry("z")
        .build()
        .expect("target")
}

/// The rounding bound the quality doc states, for a source of this shape.
fn tolerance(src: &Schema) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "schema sizes and COST_SCALE are far below the f64 integer limit"
    )]
    let bound = {
        let terms = (src.vertices.len() + src.edges.len()) as f64;
        terms / (2.0 * COST_SCALE as f64)
    };
    // A floor, so that an exact reading is not held to a zero tolerance on a
    // one-vertex source.
    bound.max(1e-9)
}

fn empty_apex_quality(src: &Schema) -> f64 {
    let span = find_span(
        src,
        &unmatchable_target(),
        &protocol(),
        &SearchOptions::default(),
    )
    .expect("the empty apex is always feasible, so this cannot refuse");
    assert!(
        span.apex.vertices.is_empty(),
        "the fixture is meant to force the empty apex, and did not: {:?}",
        span.apex.vertices
    );
    span.quality
}

/// An edgeless source charges name and degree only.
///
/// The edge and Jaccard components have nothing to normalise by, so they cost
/// nothing, and the floor sits well above `0.0`: it was measured at `0.55`
/// under the shipped weights.
#[test]
fn an_empty_apex_over_an_edgeless_source_charges_only_name_and_degree() {
    let expected = 1.0 - (DEFAULT_WEIGHTS.name() + DEFAULT_WEIGHTS.degree());
    for n in [1_usize, 4, 16] {
        let src = source(n, Edges::None);
        assert!(src.edges.is_empty(), "the fixture must have no edges");
        let quality = empty_apex_quality(&src);
        assert!(
            (quality - expected).abs() <= tolerance(&src),
            "an empty apex over an edgeless source of {n} vertices read {quality}, \
             not the {expected} that name and degree alone account for"
        );
    }
}

/// A source whose edges are all unnamed charges name, degree and edge.
///
/// The Jaccard component is normalised per source vertex with a *named*
/// outgoing edge, so it still has nothing to measure while the edge component
/// now does.
#[test]
fn an_empty_apex_over_an_unnamed_edge_source_also_charges_the_edge_component() {
    let expected =
        1.0 - (DEFAULT_WEIGHTS.name() + DEFAULT_WEIGHTS.degree() + DEFAULT_WEIGHTS.edge());
    for n in [4_usize, 16] {
        let src = source(n, Edges::Unnamed);
        assert!(!src.edges.is_empty(), "the fixture must have edges");
        assert!(
            src.edges.keys().all(|edge| edge.name.is_none()),
            "the fixture's edges must all be unnamed"
        );
        let quality = empty_apex_quality(&src);
        assert!(
            (quality - expected).abs() <= tolerance(&src),
            "an empty apex over a source of {n} vertices with unnamed edges read \
             {quality}, not the {expected} that name, degree and edge account for"
        );
    }
}

/// And the case the corpus does cover still reads its documented `0.0`.
///
/// Every component has mass here, so the empty apex pays the whole objective.
/// This is the reading the doc generalised from, and it is kept alongside the
/// other two so the correction cannot be mistaken for a behaviour change.
#[test]
fn an_empty_apex_over_a_named_edge_source_still_reads_zero() {
    for n in [4_usize, 16] {
        let src = source(n, Edges::Named);
        assert!(
            src.edges.keys().all(|edge| edge.name.is_some()),
            "the fixture's edges must all be named"
        );
        let quality = empty_apex_quality(&src);
        assert!(
            quality.abs() <= tolerance(&src),
            "an empty apex over a source of {n} vertices with named edges read \
             {quality}, and every component has mass here so it should read 0.0"
        );
    }
}
