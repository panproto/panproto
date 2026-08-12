//! The decomposed objective against the reference score.
//!
//! The morphism search minimises a decomposed objective: the four components
//! of [`reference_quality`](panproto_mig::quality::reference_quality) split
//! into one unary cost function per source vertex and one binary cost function
//! per source vertex pair, with every denominator fixed by the source schema
//! alone and every term rounded to fixed point once. The reference score
//! accumulates the same four components in `f64` over a whole morphism, with
//! two of its denominators read off the assignment.
//!
//! Two claims connect them, and this file is where they are checked:
//!
//! 1. **Agreement.** On a total morphism whose pair class has the size of the
//!    source's own prop class, the two scores agree up to rounding.
//! 2. **Dominance.** On every total morphism, the decomposed score is at least
//!    the reference score, up to rounding.
//!
//! Nothing downstream is worth building on a wrong objective, so these are the
//! exit criterion for the decomposition rather than one test among many.
//!
//! # The rounding tolerance
//!
//! The decomposition rounds once per unary entry and once per source edge, so a
//! total morphism carries at most `|V_s| + |E_s|` roundings of half a unit
//! each, against a scale of `10^9`. The bound is therefore
//! `(|V_s| + |E_s|) / (2 · COST_SCALE)`, which is parametric in the pair and is
//! computed per case by [`rounding_tolerance`]. It is **not** the constant
//! `4 × 10⁻⁸`: that figure is the bound at the size of the measured schema
//! corpus and stops holding above 80 vertices and edges, where a hand-built
//! pair attains `1.28 × 10⁻⁷`. Every pair this file draws is far below that, so
//! [`FIXED_TOLERANCE`] is asserted alongside the parametric bound as the
//! tighter of the two, and the two assertions together say both that the
//! parametric bound is right and that the corpus-size figure is not yet strained.

// A test binary is test code throughout, so the panicking helpers the
// workspace denies in library code are the right spelling here: a failed
// setup step should abort the case rather than be threaded through a
// `Result`. This is the convention every integration test in this crate
// follows.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_integration::{
    MAX_ENUMERATED_MORPHISMS, arb_scored_pair, assignment_of, edge_map_of,
    enumerate_total_morphisms, pair_class_size, prop_class_size,
};
use panproto_mig::quality::reference_quality;
use panproto_mig::solve::build::{NoEvidence, build_cfn};
use panproto_mig::{Cfn, CostWeights, DomainConstraints, SearchOptions};
use panproto_schema::Schema;
use proptest::prelude::*;

/// The tolerance the design states for the measured schema corpus.
///
/// It is the parametric bound evaluated at `|V_s| + |E_s| = 80`, which every
/// pair drawn here is well inside. Asserting it as well as the parametric bound
/// is what would notice a pair growing past the size the figure was quoted for.
const FIXED_TOLERANCE: f64 = 4e-8;

/// The worst gap rounding alone can open between the two scores.
///
/// Half a unit per rounded term, one term per source vertex and one per source
/// edge, against the fixed-point scale.
fn rounding_tolerance(src: &Schema) -> f64 {
    let terms = src.vertices.len() + src.edges.len();
    let terms = u32::try_from(terms).unwrap_or(u32::MAX);
    f64::from(terms) / (2.0 * 1e9)
}

/// The network the search minimises over a pair, under no anchor evidence.
fn network_of(src: &Schema, tgt: &Schema, weights: CostWeights) -> Option<Cfn> {
    build_cfn(
        src,
        tgt,
        &SearchOptions::default(),
        &DomainConstraints::default(),
        &NoEvidence,
        weights,
    )
    .ok()
}

/// The reference score of a total morphism.
fn reference_of(
    src: &Schema,
    tgt: &Schema,
    vertex_map: &HashMap<Name, Name>,
    weights: CostWeights,
) -> f64 {
    let edge_map = edge_map_of(src, tgt, vertex_map);
    reference_quality(vertex_map, &edge_map, src, tgt, weights.as_array())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Theorem 3.2, over every total morphism of a generated pair.
    #[test]
    fn the_decomposed_objective_matches_the_reference(
        (src, tgt, weights) in arb_scored_pair()
    ) {
        let Some(cfn) = network_of(&src, &tgt, weights) else {
            return Ok(());
        };
        let morphisms = enumerate_total_morphisms(&src, &tgt, MAX_ENUMERATED_MORPHISMS);
        prop_assert!(
            !morphisms.is_empty(),
            "the generator admitted a pair with no total morphism"
        );

        let tolerance = rounding_tolerance(&src);
        for vertex_map in &morphisms {
            let Some(assignment) = assignment_of(&cfn, vertex_map) else {
                prop_assert!(
                    false,
                    "a total morphism's image left some variable's domain: {vertex_map:?}"
                );
                unreachable!();
            };

            let decomposed = cfn.quality_of(&assignment);
            let reference = reference_of(&src, &tgt, vertex_map, weights);

            // A total morphism is feasible by construction, so the network must
            // not be reading it as `⊤`. Without this the two assertions below
            // would be satisfiable by a network that rejected everything.
            prop_assert!(
                cfn.evaluate(&assignment) != panproto_mig::Cost::TOP_SENTINEL,
                "a total morphism evaluated to top: {vertex_map:?}"
            );

            // Clause (3): the two agree exactly when the two normalisers of the
            // Jaccard component coincide.
            if pair_class_size(&src, &tgt, vertex_map) == prop_class_size(&src) {
                prop_assert!(
                    (decomposed - reference).abs() <= tolerance,
                    "{decomposed} against {reference}, tolerance {tolerance}, map {vertex_map:?}"
                );
                prop_assert!(
                    (decomposed - reference).abs() <= FIXED_TOLERANCE,
                    "{decomposed} against {reference}, above the corpus-size figure, \
                     map {vertex_map:?}"
                );
            }

            // Clause (2): and the decomposition dominates everywhere.
            prop_assert!(
                decomposed >= reference - tolerance,
                "decomposed must dominate: {decomposed} < {reference}, map {vertex_map:?}"
            );
        }
    }
}

/// What the proptest above must find in its corpus to be worth running.
///
/// Every clause of the theorem is conditional, so a corpus that misses a
/// regime asserts nothing about it while still reporting green. The thresholds
/// are set well under the measured values so that ordinary sampling noise does
/// not move them; what they catch is a generator that has drifted into
/// producing one shape.
mod coverage {
    use super::*;
    use proptest::strategy::ValueTree;
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

    /// Draws taken by the sweep. Large enough that the fractions below are
    /// stable, small enough to stay a fast test.
    const DRAWS: usize = 400;

    /// What one sweep of the generator contains.
    #[derive(Default)]
    struct Corpus {
        cases: usize,
        morphisms: usize,
        with_edges: usize,
        agreeing: usize,
        differing: usize,
        imperfect: usize,
    }

    fn sweep() -> Corpus {
        let mut runner = TestRunner::new_with_rng(
            Config::default(),
            TestRng::deterministic_rng(RngAlgorithm::ChaCha),
        );
        let strategy = arb_scored_pair();
        let mut corpus = Corpus::default();

        for _ in 0..DRAWS {
            let Ok(tree) = strategy.new_tree(&mut runner) else {
                continue;
            };
            let (src, tgt, weights) = tree.current();
            let Some(cfn) = network_of(&src, &tgt, weights) else {
                continue;
            };
            corpus.cases += 1;
            if !src.edges.is_empty() {
                corpus.with_edges += 1;
            }
            for vertex_map in &enumerate_total_morphisms(&src, &tgt, MAX_ENUMERATED_MORPHISMS) {
                let Some(assignment) = assignment_of(&cfn, vertex_map) else {
                    continue;
                };
                corpus.morphisms += 1;
                if cfn.quality_of(&assignment) < 1.0 {
                    corpus.imperfect += 1;
                }
                if pair_class_size(&src, &tgt, vertex_map) == prop_class_size(&src) {
                    corpus.agreeing += 1;
                } else {
                    corpus.differing += 1;
                }
            }
        }
        corpus
    }

    #[test]
    fn the_corpus_reaches_every_regime_the_theorem_distinguishes() {
        let corpus = sweep();
        let fraction = |part: usize, whole: usize| {
            #[expect(clippy::cast_precision_loss, reason = "both counts are far below 2^53")]
            {
                part as f64 / whole.max(1) as f64
            }
        };

        assert!(corpus.cases > DRAWS / 2, "{} cases drawn", corpus.cases);
        assert!(
            corpus.morphisms >= corpus.cases,
            "{} morphisms over {} cases: the generator must admit at least one \
             total morphism per pair",
            corpus.morphisms,
            corpus.cases
        );

        // Without source edges the edge component and the naturality constraint
        // are both vacuous, and Theorem 3.2's clause (1) has nothing to say.
        let edged = fraction(corpus.with_edges, corpus.cases);
        assert!(edged >= 0.5, "only {edged:.3} of pairs have a source edge");

        // Both branches of clause (3) must be reachable: one asserts equality,
        // the other only dominance, and a corpus with just the first would
        // never notice the correction being a no-op.
        let differing = fraction(corpus.differing, corpus.morphisms);
        assert!(
            (0.05..=0.95).contains(&differing),
            "{differing:.3} of morphisms have |C(m)| != |C_src|"
        );

        // And the scores must not all read one, or the equality branch would be
        // comparing two constants.
        let imperfect = fraction(corpus.imperfect, corpus.morphisms);
        assert!(
            imperfect >= 0.5,
            "only {imperfect:.3} of morphisms score below one"
        );
    }
}

mod fixtures {
    use super::*;
    use panproto_mig::DEFAULT_WEIGHTS;
    use panproto_schema::{Protocol, SchemaBuilder};

    fn protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn schema(vertices: &[(&str, &str)], edges: &[(&str, &str, &str, Option<&str>)]) -> Schema {
        let protocol = protocol();
        let mut builder = SchemaBuilder::new(&protocol);
        for (id, kind) in vertices {
            builder = builder.vertex(id, kind, None::<&str>).expect("vertex");
        }
        for (from, to, kind, name) in edges {
            builder = builder.edge(from, to, kind, *name).expect("edge");
        }
        builder.build().expect("schema")
    }

    /// The gap Theorem 3.2(2) predicts, on a pair built to open one.
    ///
    /// `s_leaf` has no named outgoing edge, so it is outside `C_src`; its image
    /// `t_leaf` has one, so the pair is inside `C(m)`. The two normalisers
    /// therefore differ by exactly one and the decomposition must read strictly
    /// higher. A correction that had quietly become a no-op would leave the two
    /// equal, which the lower bound on the gap catches.
    #[test]
    fn the_two_normalisers_open_the_predicted_gap() {
        let src = schema(
            &[("s_root", "object"), ("s_leaf", "object")],
            &[("s_root", "s_leaf", "prop", Some("alpha"))],
        );
        let tgt = schema(
            &[
                ("t_root", "object"),
                ("t_leaf", "object"),
                ("t_x", "object"),
            ],
            &[
                ("t_root", "t_leaf", "prop", Some("alpha")),
                ("t_leaf", "t_x", "prop", Some("beta")),
            ],
        );
        let vertex_map: HashMap<Name, Name> = [("s_root", "t_root"), ("s_leaf", "t_leaf")]
            .into_iter()
            .map(|(a, b)| (Name::from(a), Name::from(b)))
            .collect();

        let cfn = network_of(&src, &tgt, DEFAULT_WEIGHTS).expect("network");
        let assignment = assignment_of(&cfn, &vertex_map).expect("assignment");
        let decomposed = cfn.quality_of(&assignment);
        let reference = reference_of(&src, &tgt, &vertex_map, DEFAULT_WEIGHTS);

        assert_eq!(prop_class_size(&src), 1);
        assert_eq!(pair_class_size(&src, &tgt, &vertex_map), 2);

        // `Σ J = 1`, so the gap is `w_prop · (1/1 − 1/2)`.
        let predicted = DEFAULT_WEIGHTS.prop() * (1.0 - 0.5);
        let observed = decomposed - reference;
        assert!(
            (observed - predicted).abs() <= rounding_tolerance(&src),
            "gap {observed} against the predicted {predicted}"
        );
        assert!(observed > 1e-3, "the correction must not be a no-op");
    }

    /// A renaming total morphism whose two normalisers coincide.
    ///
    /// Every source vertex has a named outgoing edge exactly when its image
    /// does, so `|C(m)| = |C_src|` and the two scores are claimed to agree.
    #[test]
    fn a_renaming_total_morphism_agrees_exactly() {
        let src = schema(
            &[("s_root", "object"), ("s_leaf", "string")],
            &[("s_root", "s_leaf", "prop", Some("alpha"))],
        );
        let tgt = schema(
            &[("t_root", "object"), ("t_leaf", "string")],
            &[("t_root", "t_leaf", "prop", Some("alpha"))],
        );
        let vertex_map: HashMap<Name, Name> = [("s_root", "t_root"), ("s_leaf", "t_leaf")]
            .into_iter()
            .map(|(a, b)| (Name::from(a), Name::from(b)))
            .collect();

        let cfn = network_of(&src, &tgt, DEFAULT_WEIGHTS).expect("network");
        let assignment = assignment_of(&cfn, &vertex_map).expect("assignment");
        let decomposed = cfn.quality_of(&assignment);
        let reference = reference_of(&src, &tgt, &vertex_map, DEFAULT_WEIGHTS);

        assert_eq!(
            pair_class_size(&src, &tgt, &vertex_map),
            prop_class_size(&src)
        );
        assert!(
            (decomposed - reference).abs() <= FIXED_TOLERANCE,
            "{decomposed} against {reference}"
        );
        // And the morphism is not perfect, so the agreement is not the trivial
        // agreement of two scores that both read one.
        assert!(decomposed < 1.0, "the vertex names differ");
    }

    /// The parametric tolerance is the one that holds, and the corpus-size
    /// figure is not a ceiling.
    ///
    /// 256 source vertices onto one target, where each vertex's name term is
    /// exactly `0.25 / 256 = 2⁻¹⁰` of the scale: `976_562.5` units, a tie that
    /// `f64::round` breaks away from zero, so every one of the 256 terms drifts
    /// up by exactly half a unit and the bound is attained rather than
    /// approached.
    #[test]
    fn the_tolerance_is_parametric_rather_than_constant() {
        let protocol = protocol();
        let mut builder = SchemaBuilder::new(&protocol);
        for index in 0..256u32 {
            builder = builder
                .vertex(&format!("abcdefgh{index:04}"), "object", None::<&str>)
                .expect("vertex");
        }
        let src = builder.build().expect("source");
        let tgt = schema(&[("pqrstuvwxyzz", "object")], &[]);

        let vertex_map: HashMap<Name, Name> = src
            .vertices
            .keys()
            .map(|id| (id.clone(), Name::from("pqrstuvwxyzz")))
            .collect();

        let cfn = network_of(&src, &tgt, DEFAULT_WEIGHTS).expect("network");
        let assignment = assignment_of(&cfn, &vertex_map).expect("assignment");
        let decomposed = cfn.quality_of(&assignment);
        let reference = reference_of(&src, &tgt, &vertex_map, DEFAULT_WEIGHTS);
        let gap = (decomposed - reference).abs();

        // Both normalisers are empty, so this is the agreement branch.
        assert_eq!(
            pair_class_size(&src, &tgt, &vertex_map),
            prop_class_size(&src)
        );
        assert!(gap <= rounding_tolerance(&src), "{gap} above the bound");
        assert!(
            gap > FIXED_TOLERANCE,
            "{gap} is inside the corpus-size figure, so this fixture no longer \
             shows that the figure is not a ceiling"
        );
    }
}
