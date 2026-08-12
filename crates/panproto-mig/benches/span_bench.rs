#![allow(clippy::expect_used)]
//! Benchmarks for the schema morphism search that produces a migration span.
//!
//! Fixtures are synthetic and parameterised by width: a record with `n` scalar
//! properties is searched against a target of the same shape. Width is the
//! parameter that matters because it drives both the domain size of every
//! variable and the arity of the join the elimination sweep has to build: half
//! the leaves are `string` and half are `integer`, so each leaf variable has
//! `n/2` real values plus `⊥`.
//!
//! What the search costs is no longer the size of the hom-set. The network is
//! one variable per source vertex and the objective is minimised exactly, so
//! cost tracks the induced width of the primal graph rather than the number of
//! complete assignments. These fixtures are stars, whose width is one, so they
//! measure the constant factors of building the network and running the sweep
//! rather than the exponential the width controls.
//!
//! `cargo bench -p panproto-mig` runs these. CI never does; it only compiles and
//! lints them under `cargo clippy --all-targets`.

use divan::Bencher;
use panproto_mig::hom_search::{SearchOptions, find_best_morphism, find_span};
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

fn main() {
    divan::main();
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn generic_protocol() -> Protocol {
    Protocol {
        name: "bench-generic".to_owned(),
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

/// A record whose body carries `n` scalar properties, alternating between
/// `string` and `integer` so both branches of the edge rule are exercised.
/// `prefix` renames every vertex so a source and a target built with different
/// prefixes force the search to rank on structure rather than on name equality.
fn wide_schema(protocol: &Protocol, prefix: &str, n: usize) -> Schema {
    let mut b = SchemaBuilder::new(protocol)
        .vertex(&format!("{prefix}root"), "record", None)
        .expect("vertex root")
        .vertex(&format!("{prefix}body"), "object", None)
        .expect("vertex body")
        .edge(
            &format!("{prefix}root"),
            &format!("{prefix}body"),
            "record-schema",
            None,
        )
        .expect("edge record-schema");

    for i in 0..n {
        let kind = if i % 2 == 0 { "string" } else { "integer" };
        let field = format!("{prefix}f{i}");
        b = b
            .vertex(&field, kind, None)
            .expect("vertex field")
            .edge(
                &format!("{prefix}body"),
                &field,
                "prop",
                Some(&format!("f{i}")),
            )
            .expect("edge prop");
    }

    b.entry(&format!("{prefix}root")).build().expect("build")
}

/// A target missing every other property of the source, so no total morphism
/// exists and the span is the only answer there is.
fn narrowed_schema(protocol: &Protocol, prefix: &str, n: usize) -> Schema {
    let mut b = SchemaBuilder::new(protocol)
        .vertex(&format!("{prefix}root"), "record", None)
        .expect("vertex root")
        .vertex(&format!("{prefix}body"), "object", None)
        .expect("vertex body")
        .edge(
            &format!("{prefix}root"),
            &format!("{prefix}body"),
            "record-schema",
            None,
        )
        .expect("edge record-schema");

    for i in (0..n).step_by(2) {
        let field = format!("{prefix}f{i}");
        b = b
            .vertex(&field, "string", None)
            .expect("vertex field")
            .edge(
                &format!("{prefix}body"),
                &field,
                "prop",
                Some(&format!("f{i}")),
            )
            .expect("edge prop");
    }

    b.entry(&format!("{prefix}root")).build().expect("build")
}

// ---------------------------------------------------------------------------
// Benches
// ---------------------------------------------------------------------------

/// The identity pair: source and target are the same schema.
///
/// Domains are filtered by vertex kind alone, so this explores the same network
/// as the renamed pair below at the same width. What differs is the scoring:
/// every candidate here has an exact name match. Read against each other, the
/// two isolate the cost of computing name similarity from the cost of the search
/// itself.
#[divan::bench(args = [2, 4, 6])]
fn best_morphism_identity(bencher: Bencher, n: usize) {
    let protocol = generic_protocol();
    let schema = wide_schema(&protocol, "a:", n);
    let opts = SearchOptions::default();

    bencher.bench(|| find_best_morphism(&schema, &schema, &opts));
}

/// A renamed pair: same shape, disjoint vertex names, so no assignment is
/// decided by name equality and the objective has to separate the candidates.
#[divan::bench(args = [2, 4, 6])]
fn best_morphism_renamed(bencher: Bencher, n: usize) {
    let protocol = generic_protocol();
    let src = wide_schema(&protocol, "a:", n);
    let tgt = wide_schema(&protocol, "b:", n);
    let opts = SearchOptions::default();

    bencher.bench(|| find_best_morphism(&src, &tgt, &opts));
}

/// The span search on a pair that admits a total morphism, which is the same
/// network with one more value per variable.
#[divan::bench(args = [2, 4, 6])]
fn span_renamed(bencher: Bencher, n: usize) {
    let protocol = generic_protocol();
    let src = wide_schema(&protocol, "a:", n);
    let tgt = wide_schema(&protocol, "b:", n);
    let opts = SearchOptions::default();

    bencher.bench(|| find_span(&src, &tgt, &protocol, &opts));
}

/// The span search on a pair that admits none, which is the case the total
/// search cannot answer at all and the case the measured corpus says is
/// typical. It also pays for inducing a proper sub-schema rather than copying
/// the source.
#[divan::bench(args = [2, 4, 6])]
fn span_partial(bencher: Bencher, n: usize) {
    let protocol = generic_protocol();
    let src = wide_schema(&protocol, "a:", n);
    let tgt = narrowed_schema(&protocol, "b:", n);
    let opts = SearchOptions::default();

    bencher.bench(|| find_span(&src, &tgt, &protocol, &opts));
}
