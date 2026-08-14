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
//! # The tier-level half is deferred to Phase 8, and would be vacuous today
//!
//! `Stringency` lives in `panproto-lens` and is not yet rewired onto the span
//! search, so a test written against it now would not exercise the path it is
//! meant to defend. There is a second and more decisive reason:
//! [`W_ANCHOR`](panproto_mig::align::defaults::W_ANCHOR) ships at `0.0`. With a
//! zero anchor weight the evidence term contributes nothing to any cost, so the
//! optimum is *identical* across tiers, and `quality(T') >= quality(T)` would
//! hold because both sides are the same number. That is a test passing for the
//! wrong reason, which is worse than an absent test, so it is not written.
//!
//! What is written instead is the same two claims one layer down, over the
//! network the search minimises, with the anchor weight forced non-zero so the
//! evidence term is load bearing. When Phase 8 rewires `Stringency` and settles
//! what `W_ANCHOR` should be, the tier-level statements become
//! `feasibility_is_tier_invariant` and `optimal_quality_monotone_in_tier` over
//! adjacent tier pairs, and they will rest on exactly the properties below.
//!
//! # What "more evidence" means here
//!
//! A tier is modelled by an anchor pool, and `T ⊆ T'` by pool inclusion: the
//! base pool stands for the lower tier and the base plus an extension for the
//! higher one. That is the faithful model, because the only thing a higher tier
//! does to the search is contribute more anchors.

#![allow(clippy::expect_used)]

use std::collections::HashSet;

use panproto_gat::Name;
use panproto_integration::arb_small_schema_pair;
use panproto_mig::align::evidence::{AggregationPolicy, Provenance, aggregate};
use panproto_mig::align::{Anchor, StrategyTag};
use panproto_mig::hom_search::{DomainConstraints, SearchOptions};
use panproto_mig::solve::build::{Evidence, NoEvidence, build_cfn};
use panproto_mig::solve::cfn::Domain;
use panproto_mig::solve::cost::{Cost, CostWeights};
use panproto_mig::solve::oracle::{MAX_ORACLE_ASSIGNMENTS, assignment_count, brute_force};
use panproto_mig::solve::{Assignment, Cfn, ValId, VarId};
use panproto_schema::Schema;
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

/// The anchor weight the properties force is not the shipped one, and the
/// shipped one would make them vacuous.
///
/// This is the reason the tier-level half of the exit criteria is deferred,
/// stated as an assertion so that it stops being true the moment `W_ANCHOR`
/// changes, rather than sitting in a comment nobody rereads.
#[test]
fn the_shipped_anchor_weight_would_make_these_properties_vacuous() {
    use panproto_mig::DEFAULT_WEIGHTS;
    use panproto_mig::align::defaults::W_ANCHOR;

    assert_eq!(
        W_ANCHOR.to_bits(),
        0.0f64.to_bits(),
        "the anchor term now has mass, so the tier-level exit criteria are no \
         longer vacuous and belong here rather than deferred to Phase 8"
    );
    assert_eq!(DEFAULT_WEIGHTS.anchor().to_bits(), W_ANCHOR.to_bits());
    assert!(
        weighted_for_evidence().anchor() > 0.0,
        "the properties above must run at a non-zero anchor weight"
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
