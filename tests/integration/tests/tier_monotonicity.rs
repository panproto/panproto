//! The tier ladder, at the layer where it is currently a theorem.
//!
//! The stringency ladder rests on two claims. For tiers `T ⊆ T'`:
//!
//! * **(i) feasibility is tier invariant.** Every assignment feasible at `T` is
//!   feasible at `T'` and conversely, so a higher tier can never make the
//!   search unsatisfiable on a pair a lower tier answered.
//! * **(ii) the optimum is monotone.** The best quality at `T'` is at least the
//!   best quality at `T`.
//!
//! Both follow from one structural fact and nothing else: **evidence enters the
//! objective and never a domain.** A higher tier runs more alignment strategies
//! and so contributes a superset of anchors; aggregation is monotone in the
//! pool; the anchor term is a cost that falls as confidence rises; and no
//! domain, no hard constraint and no variable is a function of the evidence.
//! That is what these two properties check, and checking it here rather than
//! over `Stringency` is deliberate.
//!
//! # Both claims, at both layers, at a forced non-zero anchor weight
//!
//! [`W_ANCHOR`](panproto_mig::align::defaults::W_ANCHOR) ships at `0.0`. With a
//! zero anchor weight the evidence term contributes nothing to any cost, so the
//! optimum is *identical* across tiers, and `cost(T') <= cost(T)` holds because
//! both sides are the same number. That is a test passing for the wrong reason,
//! which is worse than an absent test. Every property in this file therefore
//! runs at [`weighted_for_evidence`], whose anchor component carries a quarter
//! of the objective, and each one asserts that it does before asserting
//! anything else. `the_shipped_anchor_weight_would_make_these_properties_vacuous`
//! holds the other end: it fails the moment `W_ANCHOR` stops being zero, so a
//! reader is told which claim this file is making rather than left to infer it.
//!
//! The claims are made twice, one layer apart, because the two layers can fail
//! independently.
//!
//! * **The network layer** models a tier by an anchor pool and `T ⊆ T'` by pool
//!   inclusion, which is faithful because the only thing a higher tier does to
//!   the search is contribute more anchors. It compares whole feasible sets and
//!   whole cost tables, which the tier layer cannot: `feasible_set_is_evidence_
//!   invariant` and `cost_monotone_in_evidence`.
//! * **The tier layer** runs the alignment strategies at each [`Stringency`]
//!   and searches with what they propose, so it exercises the path a caller
//!   actually takes, including the two strategies whose tier knob is a
//!   resolution or a seed rather than a threshold and whose output is therefore
//!   *not* monotone in the tier: `feasibility_is_tier_invariant` and
//!   `optimal_cost_monotone_in_tier`.
//!
//! The tier layer is the weaker of the two on cost, and deliberately so. It
//! compares the integer the solver minimises rather than the `f64` reading of
//! it, because two optima one unit apart are two distinct answers an `f64`
//! comparison can report as equal. Where a run did not prove its answer optimal
//! it compares the published bounds instead, which is the strongest statement a
//! pair of intervals licenses.
//!
//! # The hypothesis underneath both
//!
//! Neither claim survives an aggregation that is not monotone in the anchor
//! pool, and `panproto_mig::align::evidence` implements a fixed arity mean of
//! family maxima precisely so that it is. That is checked where it lives, by
//! `aggregate_monotone_in_pool` on the per-family maxima and the score, and by
//! `a_firing_family_normalised_mean_would_not_be_monotone`, which builds the
//! pool a firing-family normalisation reports as *worse* for having been given
//! one more anchor and confirms the shipped divisor does not.

#![allow(clippy::expect_used)]

use std::collections::HashSet;

use panproto_gat::Name;
use panproto_integration::{arb_small_schema_pair, small_protocol};
use panproto_lens::{AutoLensConfig, Stringency, auto_lens};
use panproto_mig::align::evidence::{AggregationPolicy, EvidenceTable, Provenance, aggregate};
use panproto_mig::align::{Anchor, StrategyTag};
use panproto_mig::hom_search::{DomainConstraints, SearchOptions};
use panproto_mig::solve::build::{Evidence, NoEvidence, build_cfn};
use panproto_mig::solve::cfn::Domain;
use panproto_mig::solve::cost::{Cost, CostWeights};
use panproto_mig::solve::oracle::{MAX_ORACLE_ASSIGNMENTS, assignment_count, brute_force};
use panproto_mig::solve::{Assignment, Cfn, SearchBudget, ValId, VarId};
use panproto_mig::{SchemaSpan, SpanSearch, coverage_radix, quality_units};
use panproto_schema::{Protocol, Schema, SchemaBuilder};
use proptest::prelude::*;

/// Weights whose anchor component is non-zero.
///
/// The shipped [`DEFAULT_WEIGHTS`](panproto_mig::DEFAULT_WEIGHTS) carry
/// `W_ANCHOR = 0.0`, which makes the evidence term contribute nothing and would
/// turn every property here into a comparison of a number with itself. A
/// quarter of the objective is enough mass for the term to move an optimum.
fn weighted_for_evidence() -> CostWeights {
    CostWeights::new(0.25, 0.25, 0.25, 0.0, 0.25).expect("a positive finite weight vector")
}

/// One anchor, described by the generator without reference to a schema.
///
/// The tag and provenance are drawn independently, because the aggregation
/// reads both: the tag fixes the priority band and the provenance caps the
/// confidence inside it.
#[derive(Clone, Debug)]
struct AnchorShape {
    src: usize,
    tgt: usize,
    confidence: f64,
    tag: usize,
    provenance: usize,
}

const TAGS: [StrategyTag; 6] = [
    StrategyTag::UserHint,
    StrategyTag::Exact,
    StrategyTag::EdgeLabel,
    StrategyTag::Alias,
    StrategyTag::TokenSimilarity,
    StrategyTag::Structural,
];

const PROVENANCES: [Provenance; 5] = [
    Provenance::UserSupplied,
    Provenance::ExactIdentifier,
    Provenance::Synonym,
    Provenance::Derived,
    Provenance::Inferred,
];

fn arb_anchor_shape() -> impl Strategy<Value = AnchorShape> {
    (0usize..6, 0usize..6, 0.0f64..=1.0, 0usize..6, 0usize..5).prop_map(
        |(src, tgt, confidence, tag, provenance)| AnchorShape {
            src,
            tgt,
            confidence,
            tag,
            provenance,
        },
    )
}

/// Turn shapes into anchors over the vertices these two schemas actually hold.
///
/// Indices are taken modulo the vertex counts, so every shape yields an anchor
/// naming a real pair rather than being discarded.
fn anchors_over(src: &Schema, tgt: &Schema, shapes: &[AnchorShape]) -> Vec<Anchor> {
    let mut sources: Vec<Name> = src.vertices.keys().cloned().collect();
    sources.sort_unstable();
    let mut targets: Vec<Name> = tgt.vertices.keys().cloned().collect();
    targets.sort_unstable();
    if sources.is_empty() || targets.is_empty() {
        return Vec::new();
    }

    shapes
        .iter()
        .map(|shape| {
            let tag = TAGS[shape.tag % TAGS.len()];
            let provenance = PROVENANCES[shape.provenance % PROVENANCES.len()];
            Anchor {
                src: sources[shape.src % sources.len()].clone(),
                tgt: targets[shape.tgt % targets.len()].clone(),
                confidence: shape.confidence,
                strategy: tag,
                provenance,
                explanation: String::new(),
            }
        })
        .collect()
}

/// The network for one pair under one evidence table.
fn network(src: &Schema, tgt: &Schema, evidence: &dyn Evidence) -> Option<Cfn> {
    build_cfn(
        src,
        tgt,
        &SearchOptions::default(),
        &DomainConstraints::default(),
        evidence,
        weighted_for_evidence(),
        panproto_mig::solve::DEFAULT_MEM_BYTES,
    )
    .ok()
}

/// Every assignment over a network's domains, `⊥` included.
///
/// Only called on networks the oracle would accept, so the product is bounded
/// by [`MAX_ORACLE_ASSIGNMENTS`] and this terminates.
fn all_assignments(cfn: &Cfn) -> Vec<Assignment> {
    let choices: Vec<Vec<ValId>> = (0..cfn.n_variables())
        .map(|index| {
            let var = VarId::new(u32::try_from(index).expect("a small variable count"));
            cfn.domain(var)
                .map(IntoIterator::into_iter)
                .map(Iterator::collect)
                .unwrap_or_default()
        })
        .collect();
    if choices.iter().any(Vec::is_empty) {
        return Vec::new();
    }

    let mut cursor = vec![0usize; choices.len()];
    let mut out = Vec::new();
    loop {
        out.push(Assignment::from_values(
            cursor
                .iter()
                .zip(&choices)
                .map(|(slot, values)| values[*slot])
                .collect(),
        ));

        let mut position = choices.len();
        loop {
            if position == 0 {
                return out;
            }
            position -= 1;
            cursor[position] += 1;
            if cursor[position] < choices[position].len() {
                break;
            }
            cursor[position] = 0;
        }
    }
}

/// The assignments a network admits, as a comparable set.
fn feasible_set(cfn: &Cfn) -> HashSet<Vec<ValId>> {
    all_assignments(cfn)
        .into_iter()
        .filter(|assignment| cfn.evaluate(assignment) != Cost::TOP_SENTINEL)
        .map(|assignment| assignment.values().to_vec())
        .collect()
}

/// Whether a pair's network is small enough to enumerate exhaustively.
fn enumerable(cfn: &Cfn) -> bool {
    assignment_count(cfn) <= MAX_ORACLE_ASSIGNMENTS
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Claim (i), at the network layer: evidence never restricts the search.
    ///
    /// The feasible sets of the no-evidence network and of a network built from
    /// an arbitrary anchor pool are compared for **exact set equality**, over
    /// every assignment both admit rather than over a sample. Equality is what
    /// makes the tier ladder possible at all: if evidence could remove an
    /// assignment, a tier that knew more could be a tier that failed, which is
    /// the defect the whole rewrite exists to remove.
    ///
    /// The variable set, the domains and the radix are compared too, since a
    /// network that agreed on feasibility while renumbering its variables would
    /// make the sets incomparable rather than equal.
    #[test]
    fn feasible_set_is_evidence_invariant(
        (_protocol, src, tgt) in arb_small_schema_pair(),
        shapes in prop::collection::vec(arb_anchor_shape(), 0..12),
    ) {
        let anchors = anchors_over(&src, &tgt, &shapes);
        let table = aggregate(&anchors, AggregationPolicy::StrictPriority);

        let Some(bare) = network(&src, &tgt, &NoEvidence) else { return Ok(()); };
        let Some(informed) = network(&src, &tgt, &table) else { return Ok(()); };
        prop_assume!(enumerable(&bare));

        prop_assert_eq!(
            bare.n_variables(),
            informed.n_variables(),
            "evidence changed the variable set"
        );
        for index in 0..bare.n_variables() {
            let var = VarId::new(u32::try_from(index).expect("a small variable count"));
            prop_assert_eq!(
                bare.domain(var).map(Domain::bits),
                informed.domain(var).map(Domain::bits),
                "evidence changed a domain"
            );
        }
        prop_assert_eq!(bare.radix(), informed.radix(), "evidence changed the radix");

        prop_assert_eq!(
            feasible_set(&bare),
            feasible_set(&informed),
            "evidence changed which assignments are feasible"
        );
    }

    /// Claim (ii), at the network layer: more evidence never costs more.
    ///
    /// Two pools are compared, the base and the base extended, which is the
    /// faithful model of `T ⊆ T'`. The check is **pointwise** over every
    /// feasible assignment, not merely at the optimum: the extended pool's
    /// network assigns no assignment a higher cost than the base pool's does.
    /// Monotonicity of the optimum follows from that, and is asserted too, but
    /// the pointwise statement is the one that rules out an encoding where a
    /// penalty happens to cancel at the argmin.
    #[test]
    fn cost_monotone_in_evidence(
        (_protocol, src, tgt) in arb_small_schema_pair(),
        base in prop::collection::vec(arb_anchor_shape(), 0..8),
        extra in prop::collection::vec(arb_anchor_shape(), 1..6),
    ) {
        let base_anchors = anchors_over(&src, &tgt, &base);
        let mut grown_anchors = base_anchors.clone();
        grown_anchors.extend(anchors_over(&src, &tgt, &extra));

        let lower = aggregate(&base_anchors, AggregationPolicy::StrictPriority);
        let higher = aggregate(&grown_anchors, AggregationPolicy::StrictPriority);

        // Aggregation itself is monotone in the pool, which is the premise the
        // cost ordering rests on. Checking it here means a failure below is
        // attributable to the encoding rather than to the aggregation.
        for (source, target, evidence) in lower.rows() {
            let grown = higher
                .get(source, target)
                .expect("a pair in the base pool is in the grown pool");
            prop_assert!(
                grown.score >= evidence.score,
                "aggregation is not monotone at ({source}, {target}): {} then {}",
                evidence.score,
                grown.score
            );
        }

        let Some(lower_cfn) = network(&src, &tgt, &lower) else { return Ok(()); };
        let Some(higher_cfn) = network(&src, &tgt, &higher) else { return Ok(()); };
        prop_assume!(enumerable(&lower_cfn));

        for assignment in all_assignments(&lower_cfn) {
            let before = lower_cfn.evaluate(&assignment);
            let after = higher_cfn.evaluate(&assignment);
            if before == Cost::TOP_SENTINEL {
                prop_assert_eq!(
                    after,
                    Cost::TOP_SENTINEL,
                    "an infeasible assignment became feasible under more evidence"
                );
                continue;
            }
            prop_assert!(
                after <= before,
                "more evidence raised the cost of an assignment: {before:?} then {after:?}"
            );
        }

        let (lower_optimum, _) = brute_force(&lower_cfn);
        let (higher_optimum, _) = brute_force(&higher_cfn);
        prop_assert!(
            higher_optimum <= lower_optimum,
            "more evidence raised the optimum: {lower_optimum:?} then {higher_optimum:?}"
        );
    }
}

/// The corpus above reaches the regime where the evidence term can move a cost.
///
/// Every property here would pass on a corpus where the anchor pool never
/// mentions a pair the network scores, because then the two networks are
/// identical and every comparison is a value against itself. This measures that
/// the pools do land on scored pairs and that the extension does change at
/// least some costs, so the monotonicity being checked is a real inequality
/// rather than an equality in disguise.
#[test]
fn the_corpus_makes_the_evidence_term_bite() {
    use proptest::test_runner::{Config, TestRunner};
    use std::cell::Cell;

    let draws = 300;
    let mut runner = TestRunner::new(Config {
        cases: draws,
        ..Config::default()
    });

    let table_hit_a_pair = Cell::new(0u32);
    let extension_moved_a_cost = Cell::new(0u32);
    let extension_moved_the_optimum = Cell::new(0u32);

    runner
        .run(
            &(
                arb_small_schema_pair(),
                prop::collection::vec(arb_anchor_shape(), 0..8),
                prop::collection::vec(arb_anchor_shape(), 1..6),
            ),
            |((_protocol, src, tgt), base, extra)| {
                let base_anchors = anchors_over(&src, &tgt, &base);
                let mut grown_anchors = base_anchors.clone();
                grown_anchors.extend(anchors_over(&src, &tgt, &extra));

                let lower = aggregate(&base_anchors, AggregationPolicy::StrictPriority);
                let higher = aggregate(&grown_anchors, AggregationPolicy::StrictPriority);
                if !higher.is_empty() {
                    table_hit_a_pair.set(table_hit_a_pair.get() + 1);
                }

                let (Some(lower_cfn), Some(higher_cfn)) =
                    (network(&src, &tgt, &lower), network(&src, &tgt, &higher))
                else {
                    return Ok(());
                };
                if !enumerable(&lower_cfn) {
                    return Ok(());
                }

                if all_assignments(&lower_cfn)
                    .iter()
                    .any(|a| lower_cfn.evaluate(a) != higher_cfn.evaluate(a))
                {
                    extension_moved_a_cost.set(extension_moved_a_cost.get() + 1);
                }
                if brute_force(&lower_cfn).0 != brute_force(&higher_cfn).0 {
                    extension_moved_the_optimum.set(extension_moved_the_optimum.get() + 1);
                }
                Ok(())
            },
        )
        .expect("the generator produces buildable pairs");

    assert!(
        table_hit_a_pair.get() >= draws / 2,
        "only {} of {draws} draws produced a non-empty evidence table",
        table_hit_a_pair.get()
    );
    assert!(
        extension_moved_a_cost.get() >= 20,
        "the pool extension changed no assignment's cost on all but {} of \
         {draws} draws, so the pointwise inequality is an equality in disguise",
        extension_moved_a_cost.get()
    );
    assert!(
        extension_moved_the_optimum.get() >= 5,
        "the pool extension never moved the optimum over {draws} draws, so the \
         optimum comparison is a value against itself"
    );
}

// ---------------------------------------------------------------------------
// The tier layer
// ---------------------------------------------------------------------------

/// The four tiers in ascending order of what they will consider.
const TIERS: [Stringency; 4] = [
    Stringency::Strict,
    Stringency::Balanced,
    Stringency::Lenient,
    Stringency::Exploratory,
];

/// The strategies whose tier knob is a resolution or a seed rather than a
/// threshold.
///
/// Every other strategy takes a *threshold* from the tier, and raising a tier
/// only lowers a threshold, so its output can only grow. These two are
/// different in kind. [`StrategyTag::WlRefinement`] takes an iteration count,
/// and each further round of colour refinement partitions the vertices more
/// finely, so a pair that shares a colour after two rounds can be separated by
/// the third. [`StrategyTag::Neighborhood`] propagates outward from a
/// one-to-one selection over the pool assembled so far, so a larger pool can
/// give a source vertex a different seed and everything that propagated from
/// the old seed goes with it.
///
/// They are named here because they are the reason claim (ii) is stated as an
/// implication below rather than as a fact about the ladder.
const NON_THRESHOLD_STRATEGIES: [StrategyTag; 2] =
    [StrategyTag::WlRefinement, StrategyTag::Neighborhood];

/// A budget that cannot finish, so the search reports a bracket.
///
/// The zero operation budget refuses exact inference and the single node stops
/// the search it falls back to. Claim (ii) is asserted on runs made under this
/// budget as well as under the default one, because the interval half of it is
/// otherwise an unreachable branch: every pair this corpus draws is small
/// enough to be solved exactly, and an assertion that never executes asserts
/// nothing.
///
/// The memory ceiling is left at its default rather than starved with the rest.
/// It is what prices the network's own cost tables, so a zero there refuses to
/// pose the problem at all, and a search that was never posed brackets nothing.
fn starved() -> SearchBudget {
    SearchBudget::default()
        .with_max_nodes(Some(1))
        .with_op_budget(0)
}

/// The anchors one tier's strategies propose, before aggregation.
fn tier_pool(src: &Schema, tgt: &Schema, tier: Stringency) -> Vec<Anchor> {
    let config = AutoLensConfig {
        stringency: tier,
        ..AutoLensConfig::default()
    };
    auto_lens::run_strategies_for_tests(src, tgt, &config).0
}

/// A pool as a set of claims, keyed on what is claimed and who claimed it.
///
/// Confidence is deliberately out of the key: several strategies scale their
/// score by how far past the tier's threshold a pair landed, so the same pair
/// from the same strategy carries different numbers at different tiers. What is
/// being tracked here is which proposals a tier withdrew, not what they were
/// worth.
fn pool_keys(anchors: &[Anchor]) -> HashSet<(Name, Name, StrategyTag)> {
    anchors
        .iter()
        .map(|anchor| (anchor.src.clone(), anchor.tgt.clone(), anchor.strategy))
        .collect()
}

/// The four pools with their aggregated tables, in ascending tier order.
fn tier_ladder(src: &Schema, tgt: &Schema) -> Vec<(Vec<Anchor>, EvidenceTable)> {
    TIERS
        .iter()
        .map(|tier| {
            let pool = tier_pool(src, tgt, *tier);
            let table = aggregate(&pool, AggregationPolicy::StrictPriority);
            (pool, table)
        })
        .collect()
}

/// A table as a comparable value: every pair it holds, with its score.
///
/// The score is compared on its bit pattern, so two tables agree here exactly
/// when they are the same function on the pairs they name.
fn table_rows(table: &EvidenceTable) -> Vec<(Name, Name, u64)> {
    let mut rows: Vec<(Name, Name, u64)> = table
        .rows()
        .map(|(source, target, evidence)| {
            (source.clone(), target.clone(), evidence.score.to_bits())
        })
        .collect();
    rows.sort_unstable();
    rows
}

/// The score one table reads for a pair, `0.0` where it is silent.
fn score_of(table: &EvidenceTable, source: &Name, target: &Name) -> f64 {
    table.get(source, target).map_or(0.0, |row| row.score)
}

/// The pairs at which `higher` fails to dominate `lower`.
///
/// Empty exactly when `ev_higher(v, a) >= ev_lower(v, a)` everywhere, which is
/// the hypothesis claim (ii) needs and the only thing `T ⊆ T'` is used for in
/// its proof.
fn evidence_shortfall(lower: &EvidenceTable, higher: &EvidenceTable) -> Vec<(Name, Name)> {
    let mut fallen: Vec<(Name, Name)> = lower
        .rows()
        .filter(|(source, target, row)| score_of(higher, source, target) < row.score)
        .map(|(source, target, _)| (source.clone(), target.clone()))
        .collect();
    fallen.sort_unstable();
    fallen
}

/// The span one tier's evidence buys, at the forced anchor weight.
fn tier_span(
    protocol: &Protocol,
    src: &Schema,
    tgt: &Schema,
    table: &EvidenceTable,
    budget: SearchBudget,
) -> SchemaSpan {
    SpanSearch::new(protocol)
        .with_evidence(table)
        .with_weights(weighted_for_evidence())
        .with_budget(budget)
        .run(src, tgt)
        .expect("the span search returns a span for every schema pair")
}

/// The four spans, in ascending tier order.
fn tier_spans(
    protocol: &Protocol,
    src: &Schema,
    tgt: &Schema,
    ladder: &[(Vec<Anchor>, EvidenceTable)],
    budget: SearchBudget,
) -> Vec<SchemaSpan> {
    ladder
        .iter()
        .map(|(_, table)| tier_span(protocol, src, tgt, table, budget))
        .collect()
}

/// The integer the solver minimises, recovered from what the span publishes.
///
/// The objective is `Cost(q · radix + drops)` with
/// `radix = (|V_s| + 1).next_power_of_two()`, whose `u64` ordering *is* the
/// lexicographic order on `(q, drops)`. `q` is [`SchemaSpan::quality`] read
/// back into units and `drops` is the number of source vertices the apex left
/// out, so the two together are the cost itself rather than a proxy for it.
///
/// The comparison is made on this rather than on [`SchemaSpan::quality`]
/// because two optima one unit apart are two distinct answers that an `f64`
/// comparison can report as equal, and because `quality` carries only the
/// leading component: a span that traded a dropped vertex for a hundredth of a
/// quality unit would be invisible to it.
fn packed_cost(span: &SchemaSpan, src: &Schema) -> u64 {
    assert!(
        span.apex.vertices.len() <= src.vertices.len(),
        "the apex is induced on a subset of the source vertices"
    );
    let vertices = u32::try_from(src.vertices.len()).expect("a small source schema");
    let drops = u64::try_from(src.vertices.len() - span.apex.vertices.len()).expect("a small apex");
    quality_units(1.0 - span.quality) * coverage_radix(vertices) + drops
}

/// `(lower, upper)` bracketing the optimum's leading component, in units.
///
/// [`SchemaSpan::quality_bounds`] publishes the two ends as qualities, and the
/// higher cost is the lower quality, so the pair is reversed on the way back
/// into units. The two ends are equal exactly when the search proved its answer
/// optimal.
///
/// The bracket carries the leading component alone, which is why the interval
/// half of claim (ii) is weaker than the packed half. That is the right
/// asymmetry: a run stopped by its budget established a bracket, not an
/// optimum, and asserting more of it would be asserting something it did not
/// compute.
fn quality_bounds_in_units(span: &SchemaSpan) -> (u64, u64) {
    (
        quality_units(1.0 - span.quality_bounds.1),
        quality_units(1.0 - span.quality_bounds.0),
    )
}

/// Claim (ii) for one ordered tier pair, in whichever form the two runs earn.
fn assert_optimum_not_worse(
    src: &Schema,
    lower: (Stringency, &SchemaSpan),
    higher: (Stringency, &SchemaSpan),
) -> Result<(), TestCaseError> {
    let (lower_tier, lower_span) = lower;
    let (higher_tier, higher_span) = higher;

    if lower_span.certificate.proven_optimal && higher_span.certificate.proven_optimal {
        let before = packed_cost(lower_span, src);
        let after = packed_cost(higher_span, src);
        prop_assert!(
            after <= before,
            "the optimum rose from {lower_tier:?} to {higher_tier:?}: {before} then {after} in \
             packed cost units, with the higher tier's evidence dominating everywhere. Evidence \
             enters the objective as a reward-only unary term, so under domination the optimum \
             cannot rise"
        );
        return Ok(());
    }

    // At least one run stopped before it could rule anything out, so the two
    // brackets are all there is. The falsifiable content is that they overlap
    // in the right direction: were the higher tier's lower bound above the
    // lower tier's upper bound, every optimum the higher tier still admits
    // would be strictly worse than one the lower tier already attained, which
    // is claim (ii) failing whatever the two searches went on to find.
    let (_, lower_upper) = quality_bounds_in_units(lower_span);
    let (higher_lower, _) = quality_bounds_in_units(higher_span);
    prop_assert!(
        higher_lower <= lower_upper,
        "the brackets rule out claim (ii) from {lower_tier:?} to {higher_tier:?}: the higher \
         tier's optimum is at least {higher_lower} and the lower tier already attained \
         {lower_upper}"
    );
    Ok(())
}

/// Every pair where the higher tier's evidence fell short was a pair a
/// non-threshold strategy withdrew.
///
/// This is what runs where claim (ii)'s hypothesis fails, and it is the sharper
/// of the two assertions. A tier can lose evidence only by withdrawing a
/// proposal, and only two strategies can withdraw one; a shortfall with no
/// withdrawal behind it, or one behind a threshold-driven strategy, is a
/// regression in the ladder rather than a known exemption from it.
fn assert_shortfall_is_attributable(
    shortfall: &[(Name, Name)],
    lower: (Stringency, &[Anchor]),
    higher: (Stringency, &[Anchor]),
) -> Result<(), TestCaseError> {
    let (lower_tier, lower_pool) = lower;
    let (higher_tier, higher_pool) = higher;
    let kept = pool_keys(higher_pool);

    for (source, target) in shortfall {
        let withdrawn: Vec<StrategyTag> = pool_keys(lower_pool)
            .into_iter()
            .filter(|(claim_source, claim_target, _)| {
                claim_source == source && claim_target == target
            })
            .filter(|key| !kept.contains(key))
            .map(|(_, _, tag)| tag)
            .collect();

        prop_assert!(
            !withdrawn.is_empty(),
            "the evidence for {source} -> {target} fell from {lower_tier:?} to {higher_tier:?} \
             without any proposal being withdrawn, so aggregation is not monotone in the pool"
        );
        for tag in &withdrawn {
            prop_assert!(
                NON_THRESHOLD_STRATEGIES.contains(tag),
                "{higher_tier:?} withdrew the {tag:?} proposal {source} -> {target} that \
                 {lower_tier:?} makes. That strategy reads the tier as a threshold, and raising a \
                 tier only lowers a threshold, so its output cannot shrink"
            );
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Claim (i), at the tier layer: raising the tier does not move the
    /// feasible set.
    ///
    /// Over every adjacent tier pair the two networks are compared for **exact
    /// set equality** on the assignments they admit, over the whole assignment
    /// space rather than a sample of it, and the variable set, the domains and
    /// the radix are compared alongside so that a network agreeing on
    /// feasibility while renumbering its variables is a failure rather than an
    /// incomparability.
    ///
    /// Equality rather than containment is the point. The defect this rewrite
    /// exists to remove was strategy output reaching the search as a domain
    /// collapse, where two individually plausible anchors could be jointly
    /// infeasible and a tier that proposed more searched less. Containment in
    /// one direction would not have caught it.
    ///
    /// This claim needs no hypothesis about the pools, which is why it is
    /// stated unconditionally where claim (ii) is not: it does not care whether
    /// the higher tier proposed more, only that whatever it proposed reached
    /// the objective and nothing else.
    #[test]
    fn feasibility_is_tier_invariant((_protocol, src, tgt) in arb_small_schema_pair()) {
        prop_assert!(
            weighted_for_evidence().anchor() > 0.0,
            "the anchor term must have mass or this property compares a network with itself"
        );

        let ladder = tier_ladder(&src, &tgt);
        let mut networks = Vec::with_capacity(TIERS.len());
        for (_, table) in &ladder {
            let Some(cfn) = network(&src, &tgt, table) else { return Ok(()); };
            networks.push(cfn);
        }
        prop_assume!(networks.iter().all(enumerable));

        for (step, tiers) in TIERS.windows(2).enumerate() {
            let lower = &networks[step];
            let higher = &networks[step + 1];
            let (lower_tier, higher_tier) = (tiers[0], tiers[1]);

            prop_assert_eq!(
                lower.n_variables(),
                higher.n_variables(),
                "the tier changed the variable set from {:?} to {:?}",
                lower_tier,
                higher_tier
            );
            for index in 0..lower.n_variables() {
                let var = VarId::new(u32::try_from(index).expect("a small variable count"));
                prop_assert_eq!(
                    lower.domain(var).map(Domain::bits),
                    higher.domain(var).map(Domain::bits),
                    "the tier changed a domain from {:?} to {:?}",
                    lower_tier,
                    higher_tier
                );
            }
            prop_assert_eq!(
                lower.radix(),
                higher.radix(),
                "the tier changed the radix from {:?} to {:?}",
                lower_tier,
                higher_tier
            );
            prop_assert_eq!(
                feasible_set(lower),
                feasible_set(higher),
                "the tier changed which assignments are feasible from {:?} to {:?}",
                lower_tier,
                higher_tier
            );
        }
    }

    /// Claim (ii), at the tier layer, with the hypothesis it needs made
    /// explicit and checked.
    ///
    /// The proof takes `T ⊆ T'` and uses it in exactly one place: to conclude
    /// that the higher tier's evidence dominates the lower tier's pointwise.
    /// Over `Stringency` that conclusion is **not free**, and
    /// `the_tier_ladder_withdraws_evidence_and_the_optimum_pays` exhibits a
    /// pair where it fails. So each of the six ordered tier pairs is checked in
    /// whichever of the two ways it has earned:
    ///
    /// * where the higher tier's table dominates, the optimum must not rise,
    ///   on the packed integer cost when both runs proved optimality and on the
    ///   published brackets otherwise;
    /// * where it does not, every pair the evidence fell at must be one a
    ///   non-threshold strategy withdrew. That attributes the break instead of
    ///   excusing it, and it is the stronger statement of the two: a third
    ///   strategy going non-monotone, or a shortfall with no withdrawal behind
    ///   it, fails here.
    ///
    /// All six ordered pairs are checked and not only the three adjacent ones,
    /// because a chain of adjacent comparisons *assumes* transitivity through
    /// the intermediate tiers rather than establishing it, and the hypothesis
    /// that licenses each link can fail at any one of them.
    ///
    /// The ladder is run twice: once at the default budget, where every pair
    /// this corpus draws is solved exactly and the packed comparison applies,
    /// and once at [`starved`], where the search is stopped after one node and
    /// the bracket comparison applies. Without the second run the interval
    /// branch of [`assert_optimum_not_worse`] would never execute.
    #[test]
    fn optimal_cost_monotone_in_tier((protocol, src, tgt) in arb_small_schema_pair()) {
        prop_assert!(
            weighted_for_evidence().anchor() > 0.0,
            "the anchor term must have mass or this property compares a cost with itself"
        );

        let ladder = tier_ladder(&src, &tgt);
        for budget in [SearchBudget::default(), starved()] {
            let spans = tier_spans(&protocol, &src, &tgt, &ladder, budget);
            for (i, lower) in spans.iter().enumerate() {
                for (j, higher) in spans.iter().enumerate().skip(i + 1) {
                    let shortfall = evidence_shortfall(&ladder[i].1, &ladder[j].1);
                    if shortfall.is_empty() {
                        assert_optimum_not_worse(
                            &src,
                            (TIERS[i], lower),
                            (TIERS[j], higher),
                        )?;
                    } else {
                        assert_shortfall_is_attributable(
                            &shortfall,
                            (TIERS[i], &ladder[i].0),
                            (TIERS[j], &ladder[j].0),
                        )?;
                    }
                }
            }
        }
    }
}

/// The tier ladder withdraws evidence, and the optimum pays for it.
///
/// This is the reason claim (ii) is an implication above rather than a fact
/// about the ladder, stated as a fixture so that it is a demonstration and not
/// a frequency. The pair is the minimal one the property shrank to, and every
/// number below was measured on it.
///
/// `Lenient` runs [`StrategyTag::Neighborhood`], which propagates from a
/// one-to-one selection over the pool assembled so far and proposes
/// `v2 -> v1`. `Exploratory` assembles a larger pool, the selection seeds
/// differently, the neighborhood proposal is not made, and
/// [`StrategyTag::Structural`] proposes the same pair instead. Structural sits
/// five bands lower in the priority order, so the pair reads *less* confidently
/// at the higher tier, and the optimum the search returns is strictly worse.
///
/// What this shows is not that the theorem is wrong. The theorem is about pool
/// inclusion and it holds: `cost_monotone_in_evidence` checks it pointwise over
/// whole cost tables. What fails is the step from a tier to a pool. A tier is
/// not a superset of the tier below it, so "monotone in the tier" does not
/// follow from "monotone in the pool", and any statement of the ladder that
/// does not carry the exemption is stating something false.
///
/// The assertions are directional rather than exact so that a change to a
/// confidence or a band moves them without breaking them. What would break them
/// is the mechanism disappearing, and that is the point: if the ladder is ever
/// made monotone, this test fails and whoever made it monotone is told to
/// strengthen `optimal_cost_monotone_in_tier` from an implication to a fact.
#[test]
fn the_tier_ladder_withdraws_evidence_and_the_optimum_pays() {
    let protocol = small_protocol();
    let src = SchemaBuilder::new(&protocol)
        .vertex("v0", "integer", None)
        .expect("a source vertex")
        .vertex("v1", "string", None)
        .expect("a source vertex")
        .vertex("v2", "object", None)
        .expect("a source vertex")
        .edge("v0", "v2", "item", Some("a"))
        .expect("a source edge")
        .build()
        .expect("a well formed source schema");
    let tgt = SchemaBuilder::new(&protocol)
        .vertex("v0", "integer", None)
        .expect("a target vertex")
        .vertex("v1", "object", None)
        .expect("a target vertex")
        .edge("v0", "v1", "item", None)
        .expect("a target edge")
        .build()
        .expect("a well formed target schema");

    let ladder = tier_ladder(&src, &tgt);
    let index_of = |tier: Stringency| {
        TIERS
            .iter()
            .position(|candidate| *candidate == tier)
            .expect("a tier in the ladder")
    };
    let lenient = index_of(Stringency::Lenient);
    let exploratory = index_of(Stringency::Exploratory);

    let (source, target) = (Name::from("v2"), Name::from("v1"));

    // The withdrawal. Neighborhood proposes the pair at the lower tier and not
    // at the higher one, which is the only way the evidence for it can fall.
    let withdrawn: HashSet<(Name, Name, StrategyTag)> = pool_keys(&ladder[lenient].0)
        .difference(&pool_keys(&ladder[exploratory].0))
        .cloned()
        .collect();
    assert!(
        withdrawn.contains(&(source.clone(), target.clone(), StrategyTag::Neighborhood)),
        "Exploratory no longer withdraws the Neighborhood proposal {source} -> {target}; the \
         withdrawals it does make are {withdrawn:?}"
    );

    // The shortfall it causes. This is exactly the hypothesis
    // `optimal_cost_monotone_in_tier` tests for before it applies claim (ii).
    let shortfall = evidence_shortfall(&ladder[lenient].1, &ladder[exploratory].1);
    assert_eq!(
        shortfall,
        vec![(source.clone(), target.clone())],
        "the evidence shortfall from Lenient to Exploratory is no longer the single pair the \
         withdrawal explains"
    );
    assert_shortfall_is_attributable(
        &shortfall,
        (Stringency::Lenient, &ladder[lenient].0),
        (Stringency::Exploratory, &ladder[exploratory].0),
    )
    .expect("the withdrawal is attributable to a non-threshold strategy");

    assert_shortfall_is_attributable(
        &shortfall,
        (Stringency::Lenient, &ladder[lenient].0),
        (Stringency::Exploratory, &ladder[exploratory].0),
    )
    .expect("the withdrawal is attributable to a non-threshold strategy");

    let before = score_of(&ladder[lenient].1, &source, &target);
    let after = score_of(&ladder[exploratory].1, &source, &target);
    assert!(
        after < before,
        "the evidence for {source} -> {target} did not fall: {before} then {after}"
    );

    // The cost it reaches. Both searches are exact, so these are optima and not
    // incumbents, and the comparison is on the integer the solver minimises.
    let spans = tier_spans(&protocol, &src, &tgt, &ladder, SearchBudget::default());
    for (tier, span) in TIERS.iter().zip(&spans) {
        assert!(
            span.certificate.proven_optimal,
            "the search at {tier:?} did not prove its answer optimal, so the comparison below \
             would be between two incumbents rather than two optima"
        );
    }
    let lower_cost = packed_cost(&spans[lenient], &src);
    let higher_cost = packed_cost(&spans[exploratory], &src);
    assert!(
        higher_cost > lower_cost,
        "the withdrawal no longer reaches the optimum: Lenient costs {lower_cost} and \
         Exploratory {higher_cost}. If the ladder has been made monotone, \
         optimal_cost_monotone_in_tier should now assert claim (ii) unconditionally"
    );

    // The apex is the same size at both tiers, so what moved is the quality
    // component and not the coverage tie-break. Without this the comparison
    // above could be read as the higher tier having covered less.
    assert_eq!(
        spans[lenient].apex.vertices.len(),
        spans[exploratory].apex.vertices.len(),
        "the two spans cover different amounts, so the cost comparison is not about quality"
    );
}

/// The tier ladder reaches the objective on this corpus, and every branch of
/// claim (ii) executes.
///
/// Each assertion in `feasibility_is_tier_invariant` and
/// `optimal_cost_monotone_in_tier` compares one tier against another, so a
/// corpus on which the four tiers propose the same anchors would make all of
/// them comparisons of a value with itself, and a branch that never runs would
/// assert nothing at all. Five things are measured, and each is one of the ways
/// that could happen silently:
///
/// 1. the tiers hand the search **different** evidence tables;
/// 2. that difference **moves the optimum**, so the cost comparison is a real
///    inequality on some draws rather than an equality throughout;
/// 3. the higher tier's evidence **dominates** on the overwhelming majority of
///    ordered pairs, so claim (ii) rather than the attribution rule is what
///    usually runs;
/// 4. the default budget **proves** its answers optimal, so the packed
///    comparison is the branch that runs there;
/// 5. the starved budget **fails** to, so the bracket comparison runs too.
#[test]
fn the_tier_ladder_is_not_vacuous_on_this_corpus() {
    use proptest::test_runner::{Config, TestRunner};
    use std::cell::Cell;

    let draws = 300;
    let ordered_pairs = draws * 6;
    let mut runner = TestRunner::new(Config {
        cases: draws,
        ..Config::default()
    });

    let tables_differ = Cell::new(0u32);
    let optimum_moved = Cell::new(0u32);
    let dominating_pairs = Cell::new(0u32);
    let proved_optimal = Cell::new(0u32);
    let starved_left_a_bracket = Cell::new(0u32);

    runner
        .run(&arb_small_schema_pair(), |(protocol, src, tgt)| {
            let ladder = tier_ladder(&src, &tgt);
            let rows: Vec<Vec<(Name, Name, u64)>> =
                ladder.iter().map(|(_, table)| table_rows(table)).collect();
            if rows.windows(2).any(|pair| pair[0] != pair[1]) {
                tables_differ.set(tables_differ.get() + 1);
            }
            for (i, (_, lower)) in ladder.iter().enumerate() {
                for (_, higher) in ladder.iter().skip(i + 1) {
                    if evidence_shortfall(lower, higher).is_empty() {
                        dominating_pairs.set(dominating_pairs.get() + 1);
                    }
                }
            }

            let spans = tier_spans(&protocol, &src, &tgt, &ladder, SearchBudget::default());
            let costs: Vec<u64> = spans.iter().map(|span| packed_cost(span, &src)).collect();
            if costs.windows(2).any(|pair| pair[0] != pair[1]) {
                optimum_moved.set(optimum_moved.get() + 1);
            }
            if spans.iter().all(|span| span.certificate.proven_optimal) {
                proved_optimal.set(proved_optimal.get() + 1);
            }

            let stopped = tier_spans(&protocol, &src, &tgt, &ladder, starved());
            if stopped.iter().any(|span| !span.certificate.proven_optimal) {
                starved_left_a_bracket.set(starved_left_a_bracket.get() + 1);
            }
            Ok(())
        })
        .expect("the generator produces searchable pairs");

    assert!(
        tables_differ.get() >= 20,
        "the four tiers handed the search the same evidence table on all but {} of {draws} \
         draws, so the tier comparisons are comparisons of a network with itself",
        tables_differ.get()
    );
    assert!(
        optimum_moved.get() >= 5,
        "the tier never moved the optimum over {draws} draws, so the cost comparison in \
         optimal_cost_monotone_in_tier is an equality in disguise"
    );
    assert!(
        dominating_pairs.get() >= ordered_pairs / 2,
        "only {} of {ordered_pairs} ordered tier pairs had the higher tier's evidence \
         dominating, so claim (ii) is not the branch optimal_cost_monotone_in_tier usually \
         takes and the attribution rule has quietly replaced it",
        dominating_pairs.get()
    );
    assert!(
        proved_optimal.get() >= draws / 2,
        "only {} of {draws} draws were solved exactly at the default budget, so the packed \
         comparison is not the branch that runs",
        proved_optimal.get()
    );
    assert!(
        starved_left_a_bracket.get() >= draws / 2,
        "only {} of {draws} draws left a bracket under the starved budget, so the interval \
         branch of assert_optimum_not_worse is close to unreachable",
        starved_left_a_bracket.get()
    );
}

/// The anchor weight every property here forces is not the shipped one, and
/// the shipped one would make all of them vacuous.
///
/// The first assertion is a tripwire on `W_ANCHOR` rather than a claim about
/// what it should be. While it is zero, no property in this file may read the
/// shipped weights, because under them the evidence term contributes nothing to
/// any cost and every comparison across pools or across tiers is a number
/// against itself. When it stops being zero this test fails, and what it is
/// asking for is a decision: the properties can then be stated at the shipped
/// weight instead of at a forced one, and this test should say so.
///
/// The remaining assertions are what keeps the tripwire honest. Nothing here is
/// protected by the first assertion, because nothing here reads `W_ANCHOR`;
/// every property calls [`weighted_for_evidence`], whose anchor component is
/// asserted positive below and again inside each tier-level property, so a
/// change to that function that quietly zeroed the term fails the properties
/// themselves rather than passing them silently.
#[test]
fn the_shipped_anchor_weight_would_make_these_properties_vacuous() {
    use panproto_mig::DEFAULT_WEIGHTS;
    use panproto_mig::align::defaults::W_ANCHOR;

    assert_eq!(
        W_ANCHOR.to_bits(),
        0.0f64.to_bits(),
        "the anchor term now has mass, so the properties in this file can be stated at the \
         shipped weights rather than at a forced one"
    );
    assert_eq!(DEFAULT_WEIGHTS.anchor().to_bits(), W_ANCHOR.to_bits());
    assert!(
        weighted_for_evidence().anchor() > 0.0,
        "every property in this file must run at a non-zero anchor weight"
    );

    // The forced weight is the only anchor weight this file uses. A property
    // that reached for the shipped vector would be comparing a cost with
    // itself, and would pass while establishing nothing.
    assert_ne!(
        weighted_for_evidence().anchor().to_bits(),
        DEFAULT_WEIGHTS.anchor().to_bits(),
        "the forced weight has collapsed onto the shipped one"
    );
}

/// A schema pair is only searchable under the protocol both were built from,
/// which is why the pair generator hands one back with them.
#[test]
fn the_pair_generator_supplies_a_shared_protocol() {
    use proptest::test_runner::{Config, TestRunner};

    let mut runner = TestRunner::new(Config {
        cases: 16,
        ..Config::default()
    });
    runner
        .run(&arb_small_schema_pair(), |(protocol, src, tgt)| {
            prop_assert_eq!(&src.protocol, &protocol.name);
            prop_assert_eq!(&tgt.protocol, &protocol.name);
            Ok(())
        })
        .expect("every drawn pair names its own protocol");
}
