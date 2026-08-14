#![allow(
    clippy::expect_used,
    reason = "a benchmark that cannot build its fixture has nothing to measure"
)]
//! Benchmarks for the schema morphism search that produces a migration span.
//!
//! # What these measure and why these pairs
//!
//! Every pair here is a pair the previous search was measured on, and four of
//! them are the pairs it was measured *failing* on: `feed.post` against
//! `actor.profile` cost 397 ms, `feed.post` against `verifyCoercionLaws` cost
//! 24.1 seconds over 48 427 560 search nodes, `vcs.commit` against
//! `schema.protocol` cost 5.39 seconds, and `verifyCoercionLaws` against
//! `vcs.schemaTree` was killed after 900 seconds without answering. Those
//! numbers are the reason the search was rewritten, so they are the numbers
//! these benchmarks exist to hold down.
//!
//! # What is inside the timed region, and what is not
//!
//! [`bench_pair`] builds the anchor pool once, outside the timed closure, and
//! times [`SpanSearch::run`] alone. The figures above are whole-`auto_lens`
//! wall times, which include anchoring, so the ratio between them and what
//! these benchmarks print overstates the search's own improvement by whatever
//! anchoring costs. Read them as a ceiling on the search rather than as a
//! like-for-like speedup, and read the like-for-like figures off the corpus
//! sweep in `crates/panproto-mig/tests/lexicon_sweep.rs`, which times the
//! whole call. Allocation is not measured here at all: no bench in this file
//! installs [`divan::AllocProfiler`], because the per-allocation accounting it
//! adds would eat the headroom `span_post_to_profile` holds against its
//! sub-millisecond target. The byte figures quoted below are the prior
//! measurements, and nothing in this repository regresses on them.
//!
//! What the search costs is no longer the size of the hom-set. The network is
//! one variable per source vertex and the objective is minimised exactly, so
//! cost tracks the induced width of the primal graph rather than the number of
//! complete assignments. The synthetic benchmarks at the end vary width
//! deliberately to show that curve; the lexicon benchmarks fix it at whatever
//! the real schemas have.
//!
//! # The tiers
//!
//! [`Tier`] mirrors `panproto_lens::auto_lens::Stringency`, which this crate
//! cannot name: `panproto-lens` depends on `panproto-mig` and not the other way
//! round. It is here because the headline claim about `post → profile` is that
//! the search costs the same whatever the tier proposes, and a benchmark that
//! ran one tier could not show that. The mirror is a benchmark fixture and
//! nothing asserts against it; the tier-invariance *assertion* lives in
//! `crates/panproto-core/tests/stringency_monotonicity.rs`, where both sides
//! are in scope.
//!
//! `cargo bench -p panproto-mig` runs these. CI never does; it only compiles
//! and lints them.

use divan::Bencher;
use panproto_gat::Name;
use panproto_mig::align::evidence::{AggregationPolicy, EvidenceTable, aggregate};
use panproto_mig::hom_search::{DomainConstraints, SearchOptions, find_best_morphism};
use panproto_mig::solve::DEFAULT_MEM_BYTES;
use panproto_mig::solve::build::{NoEvidence, build_cfn};
use panproto_mig::solve::order::{induced_width, min_fill_order, primal_graph};
use panproto_mig::{Anchor, CostWeights, SpanSearch, align};
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder, induce_on_vertices};
use rustc_hash::FxHashSet;

#[path = "../tests/support/lexicons.rs"]
mod lexicons;

fn main() {
    divan::main();
}

// ---------------------------------------------------------------------------
// Lexicon fixtures
// ---------------------------------------------------------------------------

const FEED_POST: &str = include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const ACTOR_PROFILE: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.actor.profile.json");
const VERIFY_COERCION_LAWS: &str =
    include_str!("../../../lexicons/dev/panproto/translate/verifyCoercionLaws.json");
const VCS_COMMIT: &str = include_str!("../../../lexicons/dev/panproto/vcs/commit.json");
const SCHEMA_PROTOCOL: &str = include_str!("../../../lexicons/dev/panproto/schema/protocol.json");
const VCS_SCHEMA_TREE: &str = include_str!("../../../lexicons/dev/panproto/vcs/schemaTree.json");

// ---------------------------------------------------------------------------
// Tiers
// ---------------------------------------------------------------------------

/// The four stringency tiers, named by the argument strings divan prints.
const TIERS: [&str; 4] = ["strict", "balanced", "lenient", "exploratory"];

/// A mirror of `panproto_lens::auto_lens::Stringency`, for the anchor pool
/// only.
///
/// The module docs say why the mirror is here rather than the real type.
#[derive(Copy, Clone)]
enum Tier {
    Strict,
    Balanced,
    Lenient,
    Exploratory,
}

impl Tier {
    /// The tier a divan argument string names.
    fn parse(name: &str) -> Self {
        match name {
            "strict" => Self::Strict,
            "balanced" => Self::Balanced,
            "lenient" => Self::Lenient,
            "exploratory" => Self::Exploratory,
            other => panic!("unknown tier {other}"),
        }
    }

    /// Every anchor the tier's strategies propose, before aggregation.
    fn anchors(self, src: &Schema, tgt: &Schema) -> Vec<Anchor> {
        let mut anchors = align::exact_anchors(src, tgt);
        anchors.extend(align::suffix_anchors(src, tgt));
        anchors.extend(align::edge_label_anchors(src, tgt));

        let (token, description) = match self {
            Self::Strict => (None, None),
            Self::Balanced => (Some(0.75), Some(0.55)),
            Self::Lenient => (Some(0.55), Some(0.45)),
            Self::Exploratory => (Some(0.40), Some(0.35)),
        };
        if !matches!(self, Self::Strict) {
            anchors.extend(align::alias_anchors(src, tgt, &align::default_alias_dict()));
        }
        if let Some(threshold) = token {
            anchors.extend(align::token_anchors(src, tgt, threshold));
        }
        if let Some(threshold) = description {
            anchors.extend(align::description_anchors(src, tgt, threshold));
        }
        if matches!(self, Self::Lenient | Self::Exploratory) {
            anchors.extend(align::wrap_unwrap_anchors(src, tgt));
            anchors.extend(align::type_signature_anchors(src, tgt, 0.5));
            let iterations = if matches!(self, Self::Exploratory) {
                3
            } else {
                2
            };
            anchors.extend(align::wl_anchors(src, tgt, iterations));
        }
        if matches!(self, Self::Exploratory) {
            anchors.extend(align::structural_anchors(src, tgt, 0.40));
        }
        align::adjust_anchors_by_required_sets(&mut anchors, src, tgt);
        anchors
    }

    /// The aggregated evidence the tier hands the search.
    fn evidence(self, src: &Schema, tgt: &Schema) -> EvidenceTable {
        aggregate(&self.anchors(src, tgt), AggregationPolicy::StrictPriority)
    }
}

// ---------------------------------------------------------------------------
// The four measured lexicon pairs
// ---------------------------------------------------------------------------

/// Run one span search over a fixed pair, with the tier's evidence in hand.
fn bench_pair(bencher: Bencher, tier_name: &str, src_text: &str, tgt_text: &str) {
    let protocol = panproto_protocols::atproto::protocol();
    let src = lexicons::parse(src_text);
    let tgt = lexicons::parse(tgt_text);
    let table = Tier::parse(tier_name).evidence(&src, &tgt);

    // `SpanSearch` stores its evidence as `&dyn Evidence`, which is not `Sync`,
    // so the builder cannot cross into divan's benched closure and is assembled
    // inside it instead. It holds four references and a default `SearchOptions`,
    // so what that adds to each iteration is a struct initialisation.
    bencher.bench(|| {
        SpanSearch::new(&protocol)
            .with_evidence(&table)
            .run(&src, &tgt)
    });
}

/// 39 vertices and 39 edges against 15 and 15.
///
/// 397 ms at Lenient and 7.8 ms at Exploratory before the rewrite, allocating
/// 36.6 MB. It is here at all four tiers because the claim is not only that it
/// got faster but that the tier no longer decides how long it takes.
#[divan::bench(args = TIERS)]
fn span_post_to_profile(bencher: Bencher, tier: &str) {
    bench_pair(bencher, tier, FEED_POST, ACTOR_PROFILE);
}

/// 39 vertices and 39 edges against 35 and 40.
///
/// The worst measured pair that terminated: 24.1 seconds at Exploratory over
/// 48 427 560 search nodes, 2.64 seconds at Lenient.
#[divan::bench(args = TIERS)]
fn span_post_to_verify_coercion_laws(bencher: Bencher, tier: &str) {
    bench_pair(bencher, tier, FEED_POST, VERIFY_COERCION_LAWS);
}

/// 32 vertices and 31 edges against 34 and 34.
///
/// 5.39 seconds and 4.55 seconds before the rewrite.
#[divan::bench(args = TIERS)]
fn span_commit_to_protocol(bencher: Bencher, tier: &str) {
    bench_pair(bencher, tier, VCS_COMMIT, SCHEMA_PROTOCOL);
}

/// 35 vertices and 40 edges against 13 and 12.
///
/// The pair that never answered: killed after 900 seconds.
#[divan::bench(args = TIERS)]
fn span_verify_coercion_to_schema_tree(bencher: Bencher, tier: &str) {
    bench_pair(bencher, tier, VERIFY_COERCION_LAWS, VCS_SCHEMA_TREE);
}

// ---------------------------------------------------------------------------
// The corpus stride
// ---------------------------------------------------------------------------

/// Every seventieth ordered pair of the 5852, which is 84 pairs.
///
/// The stride is over the pair index rather than over the lexicons, so the
/// sample crosses namespaces and definition kinds instead of sampling one
/// corner of the corpus.
fn stride_sample(corpus: &[lexicons::Lexicon]) -> Vec<(usize, usize)> {
    let n = corpus.len();
    (0..n * (n - 1))
        .step_by(70)
        .map(|index| {
            let i = index / (n - 1);
            let j = index % (n - 1);
            (i, if j >= i { j + 1 } else { j })
        })
        .collect()
}

/// One iteration searches all 84 pairs, so the reported figure is the sample
/// total and the per-pair mean is that over 84.
///
/// The distribution rather than the mean is what the corpus claim is about, and
/// divan reports a distribution over iterations rather than over pairs. The
/// per-pair percentiles are therefore measured and asserted in
/// `tests/lexicon_sweep.rs`, which times each pair separately.
#[divan::bench(sample_count = 10)]
fn span_corpus_stride(bencher: Bencher) {
    let protocol = panproto_protocols::atproto::protocol();
    let corpus = lexicons::corpus();
    let sample = stride_sample(&corpus);

    bencher.bench(|| {
        let search = SpanSearch::new(&protocol);
        for (i, j) in &sample {
            let _ = divan::black_box(search.run(&corpus[*i].schema, &corpus[*j].schema));
        }
    });
}

// ---------------------------------------------------------------------------
// The pieces the search is built out of
// ---------------------------------------------------------------------------

/// The lexicon with the most vertices in the corpus.
fn largest_lexicon(corpus: &[lexicons::Lexicon]) -> &Schema {
    &corpus
        .iter()
        .max_by_key(|lexicon| lexicon.schema.vertices.len())
        .expect("the corpus is not empty")
        .schema
}

/// Choosing an elimination order and measuring its induced width, on the
/// widest network the corpus produces.
///
/// This is the measurement the dispatcher routes on, so it runs before every
/// search and its cost is charged to every pair.
#[divan::bench]
fn induced_width_min_fill(bencher: Bencher) {
    let corpus = lexicons::corpus();
    let schema = largest_lexicon(&corpus);
    let cfn = build_cfn(
        schema,
        schema,
        &SearchOptions::default(),
        &DomainConstraints::default(),
        &NoEvidence,
        CostWeights::default(),
        DEFAULT_MEM_BYTES,
    )
    .expect("the network poses");
    let graph = primal_graph(&cfn);

    bencher.bench(|| {
        let order = min_fill_order(&graph);
        induced_width(&graph, &order)
    });
}

/// Inducing the apex once the assignment is known.
///
/// The apex is a sub-schema of the source rather than a copy of it, and
/// inducing it re-validates the result against the protocol. That validation is
/// paid once per span and it is the only part of the construction that is not
/// the solve.
#[divan::bench]
fn apex_induce(bencher: Bencher) {
    let protocol = panproto_protocols::atproto::protocol();
    let schema = lexicons::parse(FEED_POST);
    // Every vertex but the last two, so the induction has edges to drop and
    // cannot shortcut to a clone.
    let mut names: Vec<Name> = schema.vertices.keys().cloned().collect();
    names.sort();
    names.truncate(names.len().saturating_sub(2));
    let keep: FxHashSet<Name> = names.into_iter().collect();

    bencher.bench(|| induce_on_vertices(&schema, &protocol, &keep));
}

/// Aggregating an anchor pool into an evidence table, under both policies.
///
/// The pool is the Exploratory one on `post → profile`, which is the largest
/// any tier proposes on the measured pairs.
#[divan::bench(args = ["strict-priority", "confidence-first"])]
fn evidence_aggregate(bencher: Bencher, policy_name: &str) {
    let src = lexicons::parse(FEED_POST);
    let tgt = lexicons::parse(ACTOR_PROFILE);
    let anchors = Tier::Exploratory.anchors(&src, &tgt);
    let policy = match policy_name {
        "strict-priority" => AggregationPolicy::StrictPriority,
        "confidence-first" => AggregationPolicy::ConfidenceFirst,
        other => panic!("unknown policy {other}"),
    };

    bencher.bench(|| aggregate(&anchors, policy));
}

// ---------------------------------------------------------------------------
// Synthetic fixtures: the width curve
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
                src_kinds: vec!["object".to_owned(), "node".to_owned()],
                tgt_kinds: vec![
                    "string".to_owned(),
                    "integer".to_owned(),
                    "object".to_owned(),
                    "node".to_owned(),
                ],
            },
        ],
        obj_kinds: vec![
            "record".to_owned(),
            "object".to_owned(),
            "node".to_owned(),
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

/// A chain of `n` nested objects, each carrying one scalar leaf.
///
/// A star is width one whatever its degree; a chain is what makes the
/// elimination sweep carry a message from one end to the other, and nesting it
/// deeper grows the network in the direction the sweep is measured over.
///
/// Every link is one kind and every leaf is one kind, so a chain of `n` links
/// gives each link `n` candidates: the network grows in both `n` and `d` at
/// once, which is what the benchmark is for. It used to alternate two link
/// kinds and two leaf kinds, to halve each domain and keep 40 links inside a
/// 64-value ceiling that no longer exists.
fn chain_schema(protocol: &Protocol, prefix: &str, n: usize) -> Schema {
    let mut b = SchemaBuilder::new(protocol)
        .vertex(&format!("{prefix}root"), "record", None)
        .expect("vertex root")
        .vertex(&format!("{prefix}n0"), "object", None)
        .expect("vertex n0")
        .edge(
            &format!("{prefix}root"),
            &format!("{prefix}n0"),
            "record-schema",
            None,
        )
        .expect("edge record-schema");

    for i in 0..n {
        let leaf = format!("{prefix}n{i}.value");
        b = b
            .vertex(&leaf, "string", None)
            .expect("vertex leaf")
            .edge(&format!("{prefix}n{i}"), &leaf, "prop", Some("value"))
            .expect("edge leaf");
        if i + 1 < n {
            b = b
                .vertex(&format!("{prefix}n{}", i + 1), "object", None)
                .expect("vertex link")
                .edge(
                    &format!("{prefix}n{i}"),
                    &format!("{prefix}n{}", i + 1),
                    "prop",
                    Some("next"),
                )
                .expect("edge link");
        }
    }

    b.entry(&format!("{prefix}root")).build().expect("build")
}

/// `k` objects, each carrying a property pointing at every other one.
///
/// The primal graph of the resulting network is a clique on `k` variables, so
/// its induced width is `k - 1` and no elimination order does better. This is
/// the only shape in this file that reaches the dispatcher's fallback: a chain
/// is width one however long it gets, so a chain measures the sweep's constant
/// factors and never the decision.
fn dense_schema(protocol: &Protocol, prefix: &str, k: usize) -> Schema {
    let mut b = SchemaBuilder::new(protocol)
        .vertex(&format!("{prefix}root"), "record", None)
        .expect("vertex root");
    for i in 0..k {
        b = b
            .vertex(&format!("{prefix}o{i}"), "object", None)
            .expect("vertex object");
    }
    b = b
        .edge(
            &format!("{prefix}root"),
            &format!("{prefix}o0"),
            "record-schema",
            None,
        )
        .expect("edge record-schema");
    for i in 0..k {
        for j in 0..k {
            if i != j {
                b = b
                    .edge(
                        &format!("{prefix}o{i}"),
                        &format!("{prefix}o{j}"),
                        "prop",
                        Some(&format!("p{j}")),
                    )
                    .expect("edge prop");
            }
        }
    }
    b.entry(&format!("{prefix}root")).build().expect("build")
}

/// The identity pair: source and target are the same schema.
///
/// Domains are filtered by vertex kind alone, so this explores the same network
/// as the renamed pair below at the same width. What differs is the scoring:
/// every candidate here has an exact name match. Read against each other, the
/// two isolate the cost of computing name similarity from the cost of the
/// search itself.
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

    bencher.bench(|| SpanSearch::new(&protocol).run(&src, &tgt));
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

    bencher.bench(|| SpanSearch::new(&protocol).run(&src, &tgt));
}

/// The scaling curve, on chains of 5, 10, 20 and 40 links.
///
/// Every link adds two variables and every variable's domain grows with the
/// target, so this grows the network in both directions at once. The induced
/// width stays at one throughout, so the whole curve is exact elimination and
/// what it measures is how the sweep scales in the number of variables and the
/// size of the domains, holding the shape fixed. `span_dense_fallback` is the
/// one that varies the shape.
#[divan::bench(args = [5, 10, 20, 40])]
fn span_synthetic_chain(bencher: Bencher, n: usize) {
    let protocol = generic_protocol();
    let src = chain_schema(&protocol, "a:", n);
    let tgt = chain_schema(&protocol, "b:", n);
    assert!(
        SpanSearch::new(&protocol).run(&src, &tgt).is_ok(),
        "the chain fixture at {n} links no longer builds a network, so this benchmark would time \
         the refusal instead of the search"
    );

    bencher.bench(|| SpanSearch::new(&protocol).run(&src, &tgt));
}

/// The curve across the dispatcher's decision, on cliques of 4, 6, 8 and 10
/// objects.
///
/// The induced width is `k - 1`, so the message tables exact inference would
/// build grow as `(k + 1)^k` and the dispatcher abandons them somewhere in
/// this range. Below the crossing the curve is the elimination sweep; above it
/// the curve is depth-first branch and bound with soft local consistency, and
/// the step between the two adjacent points that straddle it is what the
/// fallback costs.
#[divan::bench(args = [4, 6, 8, 10])]
fn span_dense_fallback(bencher: Bencher, k: usize) {
    let protocol = generic_protocol();
    let src = dense_schema(&protocol, "a:", k);
    let tgt = dense_schema(&protocol, "b:", k);
    assert!(
        SpanSearch::new(&protocol).run(&src, &tgt).is_ok(),
        "the dense fixture at {k} objects no longer builds a network"
    );

    bencher.bench(|| SpanSearch::new(&protocol).run(&src, &tgt));
}
