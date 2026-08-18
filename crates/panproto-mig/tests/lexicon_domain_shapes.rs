//! The shape of the network *before* anything is solved, over every ordered
//! lexicon pair.
//!
//! `lexicon_span_shapes.rs` records what the search answered. This file records
//! what it was asked: how many source vertices there were, how many targets each
//! of them could be sent to, how many could be sent nowhere, and whether the
//! constraints ruled anything out at all. Those are the quantities three claims
//! about this corpus were stated in, and none of them had a measurement in the
//! repository behind it.
//!
//! # The three quantities
//!
//! 1. **How often no total morphism exists.** A source vertex whose kind admits
//!    no target vertex can be sent nowhere, so a pair containing one has an empty
//!    hom-set whatever else is true of it. That is a sufficient condition and not
//!    a necessary one: naturality can empty a hom-set on its own, with every
//!    domain non-empty. Both are counted here, so the reported fraction is the
//!    real one rather than the lower bound the sufficient condition gives.
//! 2. **How large a single domain gets.** One vertex offered many targets is what
//!    an exponential enumeration costs, so the maximum is the interesting tail.
//!    A maximum is not a typical value, though, so this reports min, p50, p95 and
//!    max over the population rather than one headline figure.
//! 3. **How often naturality constrains nothing.** When every constraint of the
//!    hom network is universal, the hom-set is the full Cartesian product of the
//!    domains and its size is the product of their sizes. That is the case the
//!    old enumerate-then-rank search spent its time on: an exponentially large
//!    answer set with no structure in it.
//!
//! # The population, and why it is this one
//!
//! Every **ordered** pair of all seventy-seven parseable lexicons, which is 5852
//! pairs. Two reasons to take this population rather than the 861 unordered pairs
//! of record-typed lexicons that `lexicon_span_shapes.rs` snapshots.
//!
//! First, all three quantities are asymmetric. A domain is the set of targets one
//! *source* vertex may take, so `(a, b)` and `(b, a)` have different domains,
//! different empty counts and different hom-sets. Halving the population would
//! measure one direction of each pair and report it as though it described both.
//!
//! Second, it is affordable, because none of this optimises anything. Building
//! the network is linear in the schemas, [`detect_product`] is linear in the cost
//! tables, and the one elimination pass per pair is the count sweep, which the
//! measured widths price at nothing. A debug build takes 20 to 32 seconds over
//! all 5852 pairs, against 36 to 47 seconds for `lexicon_sweep.rs`, which
//! searches the same pairs; each range is a single machine's idle reading and its
//! reading alongside the rest of the suite. Both sit inside the 60 second
//! threshold at which the `ci` nextest profile starts reporting a test as slow,
//! so this needs no exclusion from that profile and no place in
//! `corpus-gate.yml`.
//!
//! # Two networks
//!
//! [`build_cfn`] poses the *span* network, in which every variable may also take
//! `⊥` and be dropped from the apex. Its feasible set is never empty, so it
//! answers none of the three questions, which are about total morphisms.
//! [`without_bottom`] forbids `⊥`, which is the total-morphism restriction of the
//! same network and is what `find_morphisms` searches. Domains, emptiness and the
//! product verdict are all read off that one. The width is read off the span
//! network, so that it is comparable with the width column of
//! `lexicon_span_shapes.rs`; the two networks carry the same cost functions and
//! therefore the same primal graph, so one figure describes both.
//!
//! # Why no counts appear
//!
//! A hom-set on this corpus reaches `d^n` and runs past `u128`, so
//! [`count_solutions`] saturates at
//! [`COUNT_CEILING`](panproto_mig::solve::COUNT_CEILING) and a saturated reading
//! is not a count. It is used here for one bit only, whether the count is zero,
//! which saturation cannot corrupt. Whether the hom-set is the full product is
//! answered by [`detect_product`], which tests the constraints rather than
//! multiplying anything out and so has no ceiling. The two answer different
//! questions, and a number taken from one does not support a claim about the
//! other.
//!
//! # Reading a row
//!
//! ```text
//! app.bsky.actor.profile -> app.bsky.feed.like  n=15  empty=9/15=600pm  maxdom=2  hom=none  cart=-  w=1
//! ```
//!
//! `n` is the source vertex count, which is the variable count; `empty` is how
//! many of those have no kind-compatible target, as a count, a denominator and a
//! rate in thousandths; `maxdom` is the largest single domain, `⊥` excluded;
//! `hom` is `some` or `none` as a total morphism exists or does not; `cart` is
//! `yes` when the hom-set is the full Cartesian product of the domains, `no` when
//! some constraint forbids something, and `-` when a domain is empty and there is
//! no product for the hom-set to be all of; and `w` is the induced width of the
//! elimination order the dispatcher would choose.
//!
//! **`cargo insta accept` is not appropriate here.** These rows are a property of
//! the corpus and of kind compatibility, not of the objective, so a row moves
//! only when one of those changed. Review the diff and attribute each moved row.

#![allow(
    clippy::expect_used,
    reason = "a malformed committed fixture should fail the test loudly"
)]

use std::collections::BTreeMap;
use std::time::Instant;

use panproto_mig::solve::build::{NoEvidence, build_cfn};
use panproto_mig::solve::{
    COUNT_CEILING, ProductVerdict, choose_order, count_solutions, detect_product, dispatch_plan,
    fits_budget,
};
use panproto_mig::{
    DEFAULT_WEIGHTS, DomainConstraints, SearchBudget, SearchOptions, without_bottom,
};
use panproto_schema::Schema;

#[path = "support/lexicons.rs"]
mod lexicons;

/// How many ordered pairs seventy-seven lexicons give.
const ORDERED_PAIRS: usize = 77 * 76;

/// What [`detect_product`] found about one pair's hom network.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cartesian {
    /// Every constraint is universal, so the hom-set is the full product.
    Full,
    /// Some constraint forbids something, so the hom-set is a proper subset.
    Restricted,
    /// A domain is empty, so there is no product for the hom-set to be all of.
    NoProduct,
}

impl Cartesian {
    /// The column value a row carries.
    const fn column(self) -> &'static str {
        match self {
            Self::Full => "yes",
            Self::Restricted => "no",
            Self::NoProduct => "-",
        }
    }
}

/// One measured pair.
struct Shape {
    /// Every source vertex's domain size, `⊥` excluded, in variable order.
    domains: Vec<usize>,
    /// Whether the hom-set is non-empty.
    has_total_morphism: bool,
    /// Whether the hom-set is the full Cartesian product of the domains.
    cartesian: Cartesian,
    /// How many total morphisms [`count_solutions`] reports, saturating.
    ///
    /// Carried only so that the counting reading of the product question can be
    /// contrasted with the testing one. Read the number itself and a saturated
    /// entry reads as `u128::MAX` morphisms.
    hom_count: u128,
    /// The induced width of the span network's elimination order.
    width: usize,
}

impl Shape {
    /// How many source vertices, which is how many variables.
    fn variables(&self) -> usize {
        self.domains.len()
    }

    /// How many source vertices have no kind-compatible target.
    fn empty_domains(&self) -> usize {
        self.domains.iter().filter(|size| **size == 0).count()
    }

    /// The largest single domain.
    fn max_domain(&self) -> usize {
        self.domains.iter().copied().max().unwrap_or(0)
    }

    /// The product of the domain sizes, saturating where the true product runs
    /// past `u128`.
    ///
    /// This is what the hom-set's size would be if no constraint forbade
    /// anything, and it saturates on the same terms [`count_solutions`] does.
    fn domain_product(&self) -> u128 {
        self.domains.iter().fold(1u128, |total, size| {
            total.saturating_mul(u128::try_from(*size).unwrap_or(COUNT_CEILING))
        })
    }
}

/// A rate in thousandths, rounded half up.
///
/// Thousandths rather than a float so that the snapshot carries no float
/// formatting, following the `q` column of `lexicon_span_shapes.rs`.
fn per_mille(count: usize, total: usize) -> usize {
    assert!(total > 0, "a rate over an empty population is not a number");
    (count * 1000 + total / 2) / total
}

/// The value at the given percentile of an ascending slice.
fn percentile(sorted: &[usize], p: f64) -> usize {
    assert!(!sorted.is_empty(), "no values were measured");
    let last = sorted.len() - 1;
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "an index into a slice of at most a quarter of a million elements is exact in \
                  f64, and the product of a value in [0, 1] with an in-range index is in range"
    )]
    let index = (p * last as f64).round() as usize;
    sorted[index]
}

/// `min=… p50=… p95=… max=…` over an ascending slice.
fn distribution(sorted: &[usize]) -> String {
    format!(
        "min={} p50={} p95={} max={}",
        sorted.first().copied().unwrap_or(0),
        percentile(sorted, 0.50),
        percentile(sorted, 0.95),
        sorted.last().copied().unwrap_or(0),
    )
}

/// Measure one ordered pair.
fn measure(src: &Schema, tgt: &Schema) -> Shape {
    let budget = SearchBudget::default();
    let span = build_cfn(
        src,
        tgt,
        &SearchOptions::default(),
        &DomainConstraints::default(),
        &NoEvidence,
        DEFAULT_WEIGHTS,
        budget.mem_bytes,
    )
    .expect("the lexicon corpus poses networks well inside the default memory budget");

    // `⊥` is not a target vertex, so a variable's real values are exactly the
    // kind-compatible targets its source vertex has. That is the figure the
    // claims under measurement call a domain size.
    let domains: Vec<usize> = span
        .variables()
        .iter()
        .map(|variable| variable.values().len())
        .collect();

    let hom = without_bottom(&span, budget.mem_bytes);
    let (order, _hom_width) = choose_order(&hom);
    // `ProductVerdict` is non-exhaustive, so the wildcard is required rather
    // than chosen. It fails loudly: a fourth verdict is a change in what the
    // diagnostic reports, and folding it into one of these three would put a
    // number in the summary that no longer means what the summary says.
    let cartesian = match detect_product(&hom) {
        ProductVerdict::Product { .. } => Cartesian::Full,
        ProductVerdict::NotProduct { .. } => Cartesian::Restricted,
        ProductVerdict::Empty { .. } => Cartesian::NoProduct,
        verdict => panic!("detect_product reported a verdict this file cannot read: {verdict:?}"),
    };

    // Counting is exact below the ceiling and saturated at it, and either way a
    // saturated count is positive, so reading one bit off it is sound where
    // reading the number would not be. The budget guard is a formality on this
    // corpus: the widths are the ones `lexicon_span_shapes.rs` records, and
    // every one of them prices inside the default budget.
    let hom_count = if cartesian == Cartesian::NoProduct || !fits_budget(&hom, &order, &budget) {
        0
    } else {
        count_solutions(&hom, &order)
    };

    Shape {
        domains,
        has_total_morphism: hom_count > 0,
        cartesian,
        hom_count,
        width: dispatch_plan(&span, &budget).width,
    }
}

/// The summary block, which is the part a citation reads.
fn summarise(shapes: &[Shape], lexicons: usize) -> Vec<String> {
    let total = shapes.len();
    let no_morphism = shapes
        .iter()
        .filter(|shape| !shape.has_total_morphism)
        .count();
    let some_empty = shapes
        .iter()
        .filter(|shape| shape.empty_domains() > 0)
        .count();
    let naturality_only = no_morphism - some_empty;
    let admitting = total - no_morphism;
    let non_empty = total - some_empty;
    let full_product = shapes
        .iter()
        .filter(|shape| shape.cartesian == Cartesian::Full)
        .count();

    // The counting reading of the same question, kept alongside the testing one
    // because the two disagree and the disagreement is the point. A count and a
    // product that have both saturated compare equal whatever the constraints
    // did, so counting reports every wide pair as a product.
    let counted: Vec<&Shape> = shapes
        .iter()
        .filter(|shape| shape.has_total_morphism && shape.hom_count == shape.domain_product())
        .collect();
    let counted_product = counted.len();
    let saturated = counted
        .iter()
        .filter(|shape| shape.hom_count == COUNT_CEILING)
        .count();

    let mut per_pair_max: Vec<usize> = shapes.iter().map(Shape::max_domain).collect();
    per_pair_max.sort_unstable();
    let mut per_vertex: Vec<usize> = shapes
        .iter()
        .flat_map(|shape| shape.domains.iter().copied())
        .collect();
    per_vertex.sort_unstable();

    let mut widths: BTreeMap<usize, usize> = BTreeMap::new();
    for shape in shapes {
        *widths.entry(shape.width).or_default() += 1;
    }
    let widths = widths
        .iter()
        .map(|(width, count)| format!("w={width}: {count}"))
        .collect::<Vec<_>>()
        .join(", ");

    vec![
        format!(
            "population: {total} ordered pairs over {lexicons} lexicons, both directions, default \
             search options and no evidence"
        ),
        format!(
            "no total morphism exists: {no_morphism}/{total} = {}pm",
            per_mille(no_morphism, total)
        ),
        format!(
            "  because some source vertex has no kind-compatible target: {some_empty}/{total} = \
             {}pm",
            per_mille(some_empty, total)
        ),
        format!(
            "  because naturality empties it with every domain non-empty: \
             {naturality_only}/{total} = {}pm",
            per_mille(naturality_only, total)
        ),
        format!(
            "largest single domain per pair, bottom excluded: {}",
            distribution(&per_pair_max)
        ),
        format!(
            "domain size per source vertex over {} vertices, bottom excluded: {}",
            per_vertex.len(),
            distribution(&per_vertex)
        ),
        format!(
            "hom-set is the full Cartesian product of the domains: {full_product}/{admitting} = \
             {}pm of the pairs admitting a total morphism",
            per_mille(full_product, admitting)
        ),
        format!(
            "  the same numerator against the wider denominator, the pairs whose every domain is \
             non-empty: {full_product}/{non_empty} = {}pm",
            per_mille(full_product, non_empty)
        ),
        format!(
            "the same question answered by comparing a count against a product: \
             {counted_product}/{admitting} = {}pm, of which {saturated} agree only because both \
             readings saturate at the counting ceiling",
            per_mille(counted_product, admitting)
        ),
        format!("induced width of the span network: {widths}"),
    ]
}

#[test]
fn ordered_lexicon_pair_domain_shapes() {
    let corpus = lexicons::corpus();

    let started = Instant::now();
    let mut rows: Vec<String> = Vec::with_capacity(ORDERED_PAIRS);
    let mut shapes: Vec<Shape> = Vec::with_capacity(ORDERED_PAIRS);
    for (i, src) in corpus.iter().enumerate() {
        for (j, tgt) in corpus.iter().enumerate() {
            if i == j {
                continue;
            }
            let shape = measure(&src.schema, &tgt.schema);
            rows.push(format!(
                "{} -> {}  n={}  empty={}/{}={}pm  maxdom={}  hom={}  cart={}  w={}",
                src.nsid,
                tgt.nsid,
                shape.variables(),
                shape.empty_domains(),
                shape.variables(),
                per_mille(shape.empty_domains(), shape.variables()),
                shape.max_domain(),
                if shape.has_total_morphism {
                    "some"
                } else {
                    "none"
                },
                shape.cartesian.column(),
                shape.width,
            ));
            shapes.push(shape);
        }
    }
    let elapsed = started.elapsed();

    assert_eq!(
        shapes.len(),
        ORDERED_PAIRS,
        "the corpus no longer gives {ORDERED_PAIRS} ordered pairs, so the counts this file states \
         are stale"
    );
    rows.sort();

    let summary = summarise(&shapes, corpus.len());
    eprintln!(
        "lexicon domain shapes: {} pairs in {elapsed:?}",
        shapes.len()
    );
    for line in &summary {
        eprintln!("{line}");
    }

    insta::assert_yaml_snapshot!("lexicon_domain_summary", summary);
    insta::assert_yaml_snapshot!("lexicon_domain_shapes", rows);
}
