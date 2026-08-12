//! A higher stringency tier must not lose what a lower one found.
//!
//! [`Stringency`] documents that its tiers form a superset ladder: each one
//! runs every alignment strategy the tier below it runs, and then some. Two
//! claims follow, and this file asserts both on real `atproto` lexicons rather
//! than on synthetic pairs small enough that the anchor pool barely moves
//! between tiers.
//!
//! First, the **anchor pool** grows with the tier. This is the claim that
//! actually broke: strategy output used to reach the search as a domain
//! collapse, so a tier that proposed more could search less, and two
//! individually plausible anchors that were jointly infeasible turned a pair
//! the lower tier aligned into "no morphism found".
//!
//! Second, the **span** the search returns does not get worse as the pool
//! grows. Evidence enters the objective through one reward-only unary term and
//! through nothing else, so it can change which assignment is optimal and can
//! never make a feasible assignment infeasible. Quality is therefore monotone
//! non-decreasing in the tier, and apex size with it.
//!
//! # Two strategies are exempt from the first claim, and cannot be otherwise
//!
//! Most strategies take a *threshold* from the tier, so raising the tier only
//! lowers a bar and their output can only grow. Two do not, and both were
//! measured proposing at `Lenient` something they withhold at `Exploratory`:
//!
//! * [`StrategyTag::WlRefinement`] takes an *iteration count* from the tier,
//!   two rounds below `Exploratory` and three at it. Each round of colour
//!   refinement partitions the vertices more finely, so a pair that shares a
//!   colour after two rounds can be separated by the third. The tier changes
//!   the resolution of the comparison, not the bar it must clear.
//! * [`StrategyTag::Neighborhood`] propagates outward from a *selection* over
//!   the pool assembled so far, and selection is one-to-one. A larger pool can
//!   therefore give a source vertex a different seed, and everything that
//!   propagated from the old seed goes with it.
//!
//! So containment is asserted over the threshold-driven strategies, and
//! separately those two are asserted to be the *only* exceptions. The second
//! assertion is the sharper of the pair: a third strategy going non-monotone
//! is a regression, and it is what this file would catch.
//!
//! # What is and is not non-trivial in the span assertions
//!
//! [`CostWeights`] ships with an anchor weight of **zero**. Under the shipped
//! weights the evidence term is identically zero, so the four tiers minimise
//! the same objective and return the *same span*: the monotonicity assertions
//! would hold as equalities whatever the pools were, and could not fail. That
//! invariance is worth asserting on its own, and
//! `shipped_weights_make_the_span_tier_invariant` does, because it is the
//! first of the three prohibitions the evidence encoding rests on. But it is
//! not evidence for monotonicity.
//!
//! The monotonicity test therefore runs the search with the anchor term
//! weighted into the objective, where the tier can reach the optimum at all.
//! It bites: on both pairs the optimal quality cost strictly falls twice
//! across the ladder, and the test refuses to pass if it stops doing so.

use std::collections::HashSet;
use std::time::Instant;

use panproto_core::gat::Name;
use panproto_core::lens::{AutoLensConfig, Stringency, auto_lens};
use panproto_core::mig::align::evidence::{AggregationPolicy, EvidenceTable, aggregate};
use panproto_core::mig::{Anchor, CostWeights, SchemaSpan, SpanSearch, StrategyTag, quality_units};
use panproto_core::protocols;
use panproto_core::schema::{Protocol, Schema};

const FEED_POST: &str = include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const ACTOR_PROFILE: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.actor.profile.json");
const VERIFY_COERCION_LAWS: &str =
    include_str!("../../../lexicons/dev/panproto/translate/verifyCoercionLaws.json");

/// The four tiers in ascending order of what they will consider.
const TIERS: [Stringency; 4] = [
    Stringency::Strict,
    Stringency::Balanced,
    Stringency::Lenient,
    Stringency::Exploratory,
];

/// The strategies whose tier knob is a resolution or a seed rather than a
/// threshold, and whose output is therefore not monotone in the tier.
///
/// The module docs give the mechanism for each.
const NON_THRESHOLD_STRATEGIES: [StrategyTag; 2] =
    [StrategyTag::WlRefinement, StrategyTag::Neighborhood];

/// What the whole monotonicity test may spend, in a release build.
///
/// The second pair below took 24 seconds and 48 million search nodes before
/// the search became an exact optimiser over a cost function network. It now
/// takes single-digit milliseconds per tier, and asserting a wall-clock bound
/// is what lets this test run by default instead of carrying an `#[ignore]`.
/// The bound is the point rather than a detail; it sits roughly three times
/// above the measured cost, so it reports a regression rather than a slow
/// machine.
const RUNTIME_BUDGET_MS: u128 = 100;

/// Weights under which the anchor term reaches the objective.
///
/// The four structural components keep their shipped ratios and the anchor
/// term is given their combined weight, so after normalisation it carries half
/// the objective. This is not a proposed default and nothing here has been
/// calibrated. It exists so that the monotonicity assertions have something to
/// bite on: at the shipped anchor weight of zero they are assertions of
/// tier-invariance instead, which is what
/// `shipped_weights_make_the_span_tier_invariant` asserts separately.
#[expect(
    clippy::expect_used,
    reason = "a weight vector written as a literal is either valid or a defect in this file"
)]
fn anchor_weighted() -> CostWeights {
    CostWeights::new(0.25, 0.25, 0.30, 0.20, 1.0).expect("literal weights are valid")
}

#[expect(
    clippy::expect_used,
    reason = "a malformed committed fixture should fail the test loudly"
)]
fn lexicon(source: &str) -> Schema {
    let json: serde_json::Value = serde_json::from_str(source).expect("lexicon parses as JSON");
    protocols::atproto::parse_lexicon(&json).expect("lexicon parses as a schema")
}

/// The raw anchor proposals the tier's strategies emit, before aggregation.
fn anchor_pool(src: &Schema, tgt: &Schema, tier: Stringency) -> Vec<Anchor> {
    let config = AutoLensConfig {
        stringency: tier,
        ..AutoLensConfig::default()
    };
    auto_lens::run_strategies_for_tests(src, tgt, &config).0
}

/// The pool as a set, keyed on what an anchor claims and which strategy
/// claimed it.
///
/// Confidence is deliberately out of the key. Several strategies scale their
/// score by how far past the tier's threshold a pair landed, so the same pair
/// from the same strategy carries different numbers at different tiers.
/// Containment is a claim about which pairs are proposed, not about what they
/// are worth.
fn pool_keys(anchors: &[Anchor]) -> HashSet<(Name, Name, StrategyTag)> {
    anchors
        .iter()
        .map(|anchor| (anchor.src.clone(), anchor.tgt.clone(), anchor.strategy))
        .collect()
}

/// The integer quality cost the span reports, recovered from the reading it
/// publishes.
///
/// The comparison across tiers is made on this rather than on the `f64`,
/// because the objective is an integer fixed-point sum and two costs one unit
/// apart are two distinct optima an `f64` comparison can report as equal.
fn quality_cost(span: &SchemaSpan) -> u64 {
    quality_units(1.0 - span.quality)
}

/// The optimal span for one evidence table.
#[expect(
    clippy::expect_used,
    reason = "the span search refuses only on a malformed apex, which is a defect and not an answer"
)]
fn span_for(
    src: &Schema,
    tgt: &Schema,
    protocol: &Protocol,
    table: &EvidenceTable,
    weights: CostWeights,
) -> SchemaSpan {
    SpanSearch::new(protocol)
        .with_evidence(table)
        .with_weights(weights)
        .run(src, tgt)
        .expect("the span search returns a span for every schema pair")
}

/// Assert that the pool grows with the tier, up to the two strategies whose
/// tier knob is not a threshold.
fn assert_pool_grows(pools: &[HashSet<(Name, Name, StrategyTag)>], pair: &str) {
    for (i, lower) in pools.iter().enumerate() {
        for (j, higher) in pools.iter().enumerate().skip(i + 1) {
            for lost in lower.difference(higher) {
                assert!(
                    NON_THRESHOLD_STRATEGIES.contains(&lost.2),
                    "{pair}: {:?} lost the proposal {} -> {} that {:?} makes, from {:?}. That \
                     strategy reads the tier as a threshold, and raising a tier only lowers a \
                     threshold, so its output cannot shrink",
                    TIERS[j],
                    lost.0,
                    lost.1,
                    TIERS[i],
                    lost.2,
                );
            }
        }
    }
}

/// Assert every per-tier property of the span, and every cross-tier one, for
/// one schema pair.
fn assert_span_monotone(src: &Schema, tgt: &Schema, pair: &str) {
    let protocol = protocols::atproto::protocol();
    let weights = anchor_weighted();

    let pools: Vec<Vec<Anchor>> = TIERS
        .iter()
        .map(|tier| anchor_pool(src, tgt, *tier))
        .collect();
    let keys: Vec<HashSet<(Name, Name, StrategyTag)>> =
        pools.iter().map(|pool| pool_keys(pool)).collect();
    assert_pool_grows(&keys, pair);

    let spans: Vec<SchemaSpan> = pools
        .iter()
        .map(|pool| {
            let table = aggregate(pool, AggregationPolicy::StrictPriority);
            span_for(src, tgt, &protocol, &table, weights)
        })
        .collect();

    for (tier, span) in TIERS.iter().zip(&spans) {
        // Both pairs are well inside the width the exact solver handles, so a
        // search that did not prove its answer optimal fell back, and the
        // fallback is what an exact optimiser exists to make unnecessary.
        assert!(
            span.certificate.proven_optimal,
            "{pair} at {tier:?}: the search did not prove its answer optimal; it took the {:?} \
             path and reported {:?}",
            span.certificate.path, span.certificate.limit_hit,
        );
        // Feasibility is tier-invariant, so "a span exists" says nothing. That
        // it covers something does.
        assert!(
            !span.apex.vertices.is_empty(),
            "{pair} at {tier:?}: the optimal span has an empty apex, so the two schemas were \
             found to share no vertex at all"
        );
        assert!(
            span.certificate.legs_are_functorial,
            "{pair} at {tier:?}: a leg of the span is not a schema morphism"
        );
    }

    let mut strictly_better = 0usize;
    for step in TIERS.iter().zip(&spans).collect::<Vec<_>>().windows(2) {
        let (lower_tier, lower) = step[0];
        let (higher_tier, higher) = step[1];
        assert!(
            quality_cost(higher) <= quality_cost(lower),
            "{pair}: quality fell from {lower_tier:?} to {higher_tier:?} ({} -> {} in integer \
             cost units). Evidence enters the objective as a reward-only unary cost, so a larger \
             anchor pool cannot raise the optimum",
            quality_cost(lower),
            quality_cost(higher),
        );
        if quality_cost(higher) < quality_cost(lower) {
            strictly_better += 1;
        }
        assert!(
            higher.apex.vertices.len() >= lower.apex.vertices.len(),
            "{pair}: the apex shrank from {lower_tier:?} to {higher_tier:?} ({} -> {} vertices)",
            lower.apex.vertices.len(),
            higher.apex.vertices.len(),
        );
    }

    // Without this the comparisons above would pass on four identical spans,
    // which is exactly the state the shipped anchor weight of zero puts them
    // in. Both fixtures improve at two of the three steps.
    assert!(
        strictly_better >= 2,
        "{pair}: the optimum improved at {strictly_better} of the three tier steps, so the \
         monotonicity assertions above are close to vacuous on this pair. Either the evidence \
         stopped reaching the objective or this fixture stopped discriminating between tiers"
    );
}

/// The pair the ladder broke on, and the pair that used to dominate the
/// runtime.
#[test]
fn span_quality_is_monotone_across_tiers() {
    let started = Instant::now();

    let post = lexicon(FEED_POST);
    let profile = lexicon(ACTOR_PROFILE);
    assert_span_monotone(&post, &profile, "feed.post -> actor.profile");

    // The pair above is the one whose anchor set went jointly infeasible; this
    // one cost 24 seconds and 48 million search nodes under
    // enumerate-then-sort.
    let verify = lexicon(VERIFY_COERCION_LAWS);
    assert_span_monotone(&post, &verify, "feed.post -> verifyCoercionLaws");

    let elapsed = started.elapsed();
    assert!(
        cfg!(debug_assertions) || elapsed.as_millis() <= RUNTIME_BUDGET_MS,
        "the whole test took {elapsed:?}, over the {RUNTIME_BUDGET_MS} ms budget. This pair cost \
         24 seconds before the search became an exact optimiser, and the budget is what keeps it \
         from drifting back"
    );
}

/// At the shipped anchor weight the four tiers return the same span.
///
/// This is not a restatement of the monotonicity assertions. It is the first
/// prohibition the evidence encoding rests on: evidence never removes a value
/// from a domain, so with the anchor term weighted at zero the four tiers
/// minimise one objective over one feasible set, and the answer cannot depend
/// on the tier however much the pool grows. It is also why
/// `span_quality_is_monotone_across_tiers` weights the anchor term in: under
/// the shipped weights its comparisons are equalities.
#[test]
fn shipped_weights_make_the_span_tier_invariant() {
    let post = lexicon(FEED_POST);
    let profile = lexicon(ACTOR_PROFILE);
    let protocol = protocols::atproto::protocol();

    // The anchor weight is the only route from the tier into the objective, and
    // the claim is that it is literally zero rather than merely small, so the
    // comparison is on the bit pattern.
    assert_eq!(
        CostWeights::default().anchor().to_bits(),
        0.0_f64.to_bits(),
        "the shipped anchor weight is no longer zero, so this test now asserts something \
         stronger than it was written to assert. Read the module docs and decide which of the \
         two claims it should make"
    );

    let pools: Vec<Vec<Anchor>> = TIERS
        .iter()
        .map(|tier| anchor_pool(&post, &profile, *tier))
        .collect();

    // The pools do differ, so the invariance below is a statement about the
    // objective rather than about four tiers proposing the same thing.
    assert!(
        pool_keys(&pools[3]).len() > pool_keys(&pools[0]).len(),
        "the pool did not grow across the ladder ({} -> {}), so this fixture no longer \
         distinguishes tier-invariance from the tiers agreeing",
        pool_keys(&pools[0]).len(),
        pool_keys(&pools[3]).len(),
    );

    let spans: Vec<SchemaSpan> = pools
        .iter()
        .map(|pool| {
            let table = aggregate(pool, AggregationPolicy::StrictPriority);
            span_for(&post, &profile, &protocol, &table, CostWeights::default())
        })
        .collect();

    for (tier, span) in TIERS.iter().zip(&spans).skip(1) {
        assert_eq!(
            span.certificate.apex_digest, spans[0].certificate.apex_digest,
            "the apex at {tier:?} differs from the one at {:?}, but with the anchor term \
             weighted at zero the tier cannot reach the objective at all",
            TIERS[0],
        );
        assert_eq!(
            quality_cost(span),
            quality_cost(&spans[0]),
            "the quality at {tier:?} differs from the one at {:?} with the anchor term weighted \
             at zero",
            TIERS[0],
        );
    }
}
