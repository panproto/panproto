//! Ranked candidate lenses with per-step explanations.
//!
//! A [`LensCandidate`] bundles a protolens chain, its instantiated
//! concrete lens, a numeric quality score, a coverage term
//! `|C| / max(|A|, |B|)`, and a sequence of [`CandidateStep`] entries
//! that map each elementary protolens back to the anchor that
//! motivated it. This is the data surface that CLIs, SDKs, and UIs
//! consume when they want to display alternatives.
//!
//! # Categorical interpretation
//!
//! Every `LensCandidate` witnesses an actual theory morphism
//! `A → B`: the CSP solver that produced its `vertex_map` has already
//! enforced naturality. The `quality` score is a meta-metric over that
//! morphism's fidelity and is **not** a part of the categorical data;
//! `coverage` is the ratio of matched sorts to the maximum endpoint
//! size, which ties to the span framing (Lenient / Exploratory tiers
//! surface spans by returning multiple candidates that differ in C).

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_mig::align::{Anchor, StrategyTag};
use panproto_schema::Schema;

use crate::Lens;
use crate::protolens::{Protolens, ProtolensChain};

/// One element of a candidate's protolens chain enriched with the
/// anchor-derived explanation and confidence that motivated it.
#[derive(Clone, Debug)]
pub struct CandidateStep {
    /// Elementary protolens name (`add_sort`, `rename_op`, etc.).
    pub kind: String,
    /// Human-readable explanation for this step. Populated from the
    /// matched [`Anchor::explanation`] where possible, otherwise
    /// derived structurally from the endofunctor.
    pub explanation: String,
    /// Confidence in [0.0, 1.0]. Copied from the anchor that produced
    /// the step when identifiable; `1.0` for structural operations
    /// (add/drop) that don't correspond to a rename anchor.
    pub confidence: f64,
    /// Tag of the strategy responsible for this step; `None` when the
    /// step is structural (add/drop) with no anchor.
    pub strategy: Option<StrategyTag>,
}

/// One ranked candidate morphism plus its instantiated lens.
#[derive(Debug)]
pub struct LensCandidate {
    /// Schema-independent chain of elementary protolenses.
    pub chain: ProtolensChain,
    /// Concrete lens instantiated at the source schema.
    pub lens: Lens,
    /// CSP quality score in `[0.0, 1.0]`.
    pub quality: f64,
    /// Coverage term `|matched vertices| / max(|src|, |tgt|)` in `[0.0, 1.0]`.
    pub coverage: f64,
    /// Seed anchors that the alignment strategies produced. Retained
    /// so downstream callers (protolab etc.) can show per-anchor
    /// provenance alongside the step list.
    pub seed_anchors: Vec<Anchor>,
    /// Enriched per-step record.
    pub steps: Vec<CandidateStep>,
    /// De-duplicated set of strategy tags that contributed at least
    /// one seed anchor to this candidate.
    pub strategies_used: Vec<StrategyTag>,
}

impl LensCandidate {
    /// Composite ranking score combining quality, coverage, and the
    /// average per-step confidence:
    ///
    /// ```text
    /// score = quality + 0.5 * coverage + 0.2 * avg_step_confidence
    /// ```
    ///
    /// Used by [`auto_generate_candidates`] to sort the result vector.
    /// `score` is not normalized to `[0, 1]`: the maximum achievable
    /// value is `1.0 + 0.5 + 0.2 = 1.7` (perfect quality, full coverage,
    /// unit confidence on every step), the minimum is `0.0`. It is
    /// intended as an ordering key only.
    ///
    /// [`auto_generate_candidates`]: crate::auto_generate_candidates
    #[must_use]
    pub fn score(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let avg_step_conf = if self.steps.is_empty() {
            1.0
        } else {
            self.steps.iter().map(|s| s.confidence).sum::<f64>() / self.steps.len() as f64
        };
        0.2f64.mul_add(avg_step_conf, 0.5f64.mul_add(self.coverage, self.quality))
    }
}

/// Compute the coverage term `|matched| / max(|src_vertices|, |tgt_vertices|)`.
///
/// The result is clamped to `[0.0, 1.0]` defensively: under a well-formed
/// CSP result `matched <= max(|src|, |tgt|)` always, but if a caller
/// supplies a nonsensical `matched` value the clamp keeps the score well
/// behaved instead of surfacing a ratio above one.
#[must_use]
pub fn coverage_ratio(src: &Schema, tgt: &Schema, matched: usize) -> f64 {
    let denom = src.vertex_count().max(tgt.vertex_count()).max(1);
    debug_assert!(
        matched <= denom,
        "matched ({matched}) exceeds max(|src|, |tgt|) = {denom}; \
         candidate builder produced a nonsensical vertex_map"
    );
    #[allow(clippy::cast_precision_loss)]
    {
        let raw = matched as f64 / denom as f64;
        raw.clamp(0.0, 1.0)
    }
}

/// Build per-step explanations from the protolens chain by correlating
/// each elementary step back to the seed anchor that motivated it.
///
/// Matching heuristic: the step's affected sort / op names are looked
/// up against anchors whose `src` or `tgt` names include them. If
/// multiple anchors match, the highest-confidence one wins. If none
/// match, a structural explanation is synthesized from the step name.
#[must_use]
pub fn enrich_steps(chain: &ProtolensChain, anchors: &[Anchor]) -> Vec<CandidateStep> {
    // When two anchors share a key, prefer the one with higher confidence;
    // break ties on strategy priority (so `Exact > Alias > TokenSim …` at
    // equal confidence); then on `tgt` name ascending. Without the tie
    // break the winner depends on the caller's slice order, which is
    // fed by HashMap iteration upstream and therefore nondeterministic.
    let priority = |tag: StrategyTag| -> u8 {
        match tag {
            StrategyTag::UserHint => 100,
            StrategyTag::Exact => 90,
            StrategyTag::EdgeLabel => 85,
            StrategyTag::ExactSuffix => 80,
            StrategyTag::Alias => 70,
            StrategyTag::TypeSignature => 60,
            StrategyTag::WrapUnwrap => 55,
            StrategyTag::TokenSimilarity => 50,
            StrategyTag::DescriptionSimilarity => 45,
            StrategyTag::Coerce => 40,
            StrategyTag::Neighborhood => 35,
            StrategyTag::WlRefinement => 32,
            StrategyTag::Structural => 30,
            StrategyTag::Llm => 20,
        }
    };
    let better = |a: &Anchor, b: &Anchor| -> bool {
        // strict: `a > b`
        match a.confidence.total_cmp(&b.confidence) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => match priority(a.strategy).cmp(&priority(b.strategy)) {
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Less => false,
                std::cmp::Ordering::Equal => a.tgt.as_str() < b.tgt.as_str(),
            },
        }
    };
    let fold_by = |key_fn: fn(&Anchor) -> &str| -> HashMap<&str, &Anchor> {
        let mut acc: HashMap<&str, &Anchor> = HashMap::new();
        for anchor in anchors {
            let key = key_fn(anchor);
            acc.entry(key)
                .and_modify(|existing| {
                    if better(anchor, existing) {
                        *existing = anchor;
                    }
                })
                .or_insert(anchor);
        }
        acc
    };
    let by_src: HashMap<&str, &Anchor> = fold_by(|a| a.src.as_str());
    let by_tgt: HashMap<&str, &Anchor> = fold_by(|a| a.tgt.as_str());

    chain
        .steps
        .iter()
        .map(|step| step_to_candidate(step, &by_src, &by_tgt, anchors))
        .collect()
}

fn step_to_candidate<'a>(
    step: &Protolens,
    by_src: &HashMap<&str, &'a Anchor>,
    by_tgt: &HashMap<&str, &'a Anchor>,
    anchors: &'a [Anchor],
) -> CandidateStep {
    use panproto_gat::TheoryTransform;
    let kind = step.name.to_string();

    let matched_anchor: Option<&Anchor> = match &step.target.transform {
        TheoryTransform::RenameSort { old, new } | TheoryTransform::RenameOp { old, new } => by_src
            .get(old.as_ref())
            .or_else(|| by_tgt.get(new.as_ref()))
            .copied(),
        TheoryTransform::AddSort { sort, .. }
        | TheoryTransform::AddSortWithDefault { sort, .. } => {
            by_tgt.get(sort.name.as_ref()).copied()
        }
        TheoryTransform::DropSort(name) | TheoryTransform::DropOp(name) => {
            by_src.get(name.as_ref()).copied()
        }
        TheoryTransform::AddOp(op) => by_tgt.get(op.name.as_ref()).copied(),
        TheoryTransform::CoerceSort { sort_name, .. } => {
            // Coerce anchors are keyed by vertex IDs (e.g., `r.n`),
            // not by sort kind names (e.g., `integer`). Try an exact
            // lookup first (covers anchors keyed on the sort name
            // itself), then widen to a substring match, and finally
            // fall through to any Coerce-strategy anchor. This is the
            // only place the candidate builder knows the step is a
            // kind-bridging coercion, so the scan is bounded to those.
            let key = sort_name.as_ref();
            by_src
                .get(key)
                .or_else(|| by_tgt.get(key))
                .copied()
                .or_else(|| {
                    anchors.iter().find(|a| {
                        a.strategy == StrategyTag::Coerce
                            && (a.src.as_str().contains(key) || a.tgt.as_str().contains(key))
                    })
                })
                .or_else(|| anchors.iter().find(|a| a.strategy == StrategyTag::Coerce))
        }
        _ => None,
    };

    match matched_anchor {
        None => CandidateStep {
            kind,
            explanation: structural_explanation(step),
            confidence: 1.0,
            strategy: None,
        },
        Some(anchor) => CandidateStep {
            kind,
            explanation: anchor.explanation.clone(),
            confidence: anchor.confidence,
            strategy: Some(anchor.strategy),
        },
    }
}

fn structural_explanation(step: &Protolens) -> String {
    use panproto_gat::TheoryTransform;
    match &step.target.transform {
        TheoryTransform::AddSort { sort, .. }
        | TheoryTransform::AddSortWithDefault { sort, .. } => {
            format!("structural: added sort `{}`", sort.name)
        }
        TheoryTransform::DropSort(name) => format!("structural: dropped sort `{name}`"),
        TheoryTransform::AddOp(op) => format!("structural: added op `{}`", op.name),
        TheoryTransform::DropOp(name) => format!("structural: dropped op `{name}`"),
        TheoryTransform::RenameSort { old, new } => {
            format!("structural: renamed sort `{old}` → `{new}`")
        }
        TheoryTransform::RenameOp { old, new } => {
            format!("structural: renamed op `{old}` → `{new}`")
        }
        TheoryTransform::CoerceSort {
            sort_name,
            target_kind,
            coercion_class,
            ..
        } => format!(
            "structural: coerce sort `{sort_name}` to `{target_kind:?}` ({coercion_class:?})"
        ),
        other => format!("structural: {other:?}"),
    }
}

/// De-duplicate the strategy tags that contributed to `anchors`.
#[must_use]
pub fn strategies_used(anchors: &[Anchor]) -> Vec<StrategyTag> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for anchor in anchors {
        if seen.insert(anchor.strategy) {
            out.push(anchor.strategy);
        }
    }
    out
}

/// Count how many source vertices appear in `vertex_map`.
///
/// This wraps `HashMap::len` behind a named helper so call sites read
/// as intent (the "matched vertex count" that feeds coverage) rather
/// than as a generic collection-length probe. It also pins the
/// semantics: for a total morphism this equals `src.vertex_count()`;
/// for a span-with-drops it is strictly smaller and drives the
/// coverage term.
#[must_use]
pub fn matched_count(vertex_map: &HashMap<Name, Name>) -> usize {
    vertex_map.len()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_gat::Name;
    use panproto_mig::align::{Anchor, StrategyTag};

    fn mk_anchor(src: &str, tgt: &str, conf: f64, tag: StrategyTag, explanation: &str) -> Anchor {
        Anchor {
            src: Name::from(src),
            tgt: Name::from(tgt),
            confidence: conf,
            strategy: tag,
            explanation: explanation.to_owned(),
        }
    }

    #[test]
    fn coverage_ratio_identity() {
        // Empty schemas should yield 0/1 = 0 ... but we guard against zero division
        // and treat max(0, 0) as 1; the ratio would be 0/1 = 0.
        let proto = panproto_schema::Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into()],
            constraint_sorts: vec![],
            ..panproto_schema::Protocol::default()
        };
        let s = panproto_schema::SchemaBuilder::new(&proto)
            .vertex("a", "object", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        assert!((coverage_ratio(&s, &s, 1) - 1.0).abs() < 1e-9);
        assert!((coverage_ratio(&s, &s, 0) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn coverage_ratio_asymmetric_clamps_below_one() {
        // When source and target schemas differ in vertex count, the
        // denominator is `max(|src|, |tgt|)`. A small source and a large
        // target must still produce `matched / max_len`, not
        // `matched / |src|` — otherwise a one-to-one partial alignment
        // between a 1-vertex source and a 3-vertex target would report
        // perfect coverage.
        let proto = panproto_schema::Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into()],
            constraint_sorts: vec![],
            ..panproto_schema::Protocol::default()
        };
        let small = panproto_schema::SchemaBuilder::new(&proto)
            .vertex("a", "object", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let large = panproto_schema::SchemaBuilder::new(&proto)
            .vertex("a", "object", None::<&str>)
            .unwrap()
            .vertex("b", "object", None::<&str>)
            .unwrap()
            .vertex("c", "object", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let r = coverage_ratio(&small, &large, 1);
        assert!((r - (1.0 / 3.0)).abs() < 1e-9, "1/3 expected, got {r}");
        let full = coverage_ratio(&small, &large, 3);
        assert!(
            (full - 1.0).abs() < 1e-9,
            "max match over max denominator must land at 1.0, got {full}"
        );
    }

    #[test]
    fn score_combines_quality_coverage_confidence() {
        use crate::protolens::ProtolensChain;
        // Build a minimal empty-chain candidate: avg_step_confidence
        // defaults to 1.0 when steps is empty, so score = quality
        // + 0.5*coverage + 0.2*1.0.
        let proto = panproto_schema::Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into()],
            constraint_sorts: vec![],
            ..panproto_schema::Protocol::default()
        };
        let s = panproto_schema::SchemaBuilder::new(&proto)
            .vertex("a", "object", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let chain = ProtolensChain::new(vec![]);
        let lens = chain.instantiate(&s, &proto).unwrap();

        let cand = LensCandidate {
            chain,
            lens,
            quality: 0.8,
            coverage: 0.6,
            seed_anchors: vec![],
            steps: vec![],
            strategies_used: vec![],
        };
        // 0.8 + 0.5*0.6 + 0.2*1.0 = 0.8 + 0.3 + 0.2 = 1.3
        assert!(
            (cand.score() - 1.3).abs() < 1e-9,
            "expected 1.3, got {}",
            cand.score()
        );

        // Score with steps: avg confidence weights into the 0.2 term.
        let steps = vec![
            CandidateStep {
                kind: "rename_sort".into(),
                explanation: "x".into(),
                confidence: 0.4,
                strategy: None,
            },
            CandidateStep {
                kind: "rename_sort".into(),
                explanation: "y".into(),
                confidence: 0.6,
                strategy: None,
            },
        ];
        let chain2 = ProtolensChain::new(vec![]);
        let lens2 = chain2.instantiate(&s, &proto).unwrap();
        let cand2 = LensCandidate {
            chain: chain2,
            lens: lens2,
            quality: 0.5,
            coverage: 0.0,
            seed_anchors: vec![],
            steps,
            strategies_used: vec![],
        };
        // avg conf = 0.5; 0.5 + 0 + 0.2*0.5 = 0.6
        assert!(
            (cand2.score() - 0.6).abs() < 1e-9,
            "expected 0.6, got {}",
            cand2.score()
        );
    }

    #[test]
    fn coverage_ratio_clamps_out_of_range() {
        // Build tiny schema and verify extreme matched counts still land in [0,1].
        let proto = panproto_schema::Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into()],
            constraint_sorts: vec![],
            ..panproto_schema::Protocol::default()
        };
        let s = panproto_schema::SchemaBuilder::new(&proto)
            .vertex("a", "object", None::<&str>)
            .unwrap()
            .vertex("b", "object", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        // Release build: no debug_assert firing, clamp must still
        // produce a value in [0, 1].
        if cfg!(debug_assertions) {
            // In debug builds an oversized matched triggers the
            // assertion — skip the assertion on that branch.
            assert!(coverage_ratio(&s, &s, 2) <= 1.0);
        } else {
            let r = coverage_ratio(&s, &s, 99);
            assert!((0.0..=1.0).contains(&r), "clamp failed: got {r}");
        }
    }

    #[test]
    fn enrich_steps_prefers_src_match_over_tgt_on_exact_rename() {
        // A rename_sort transform: the by_src lookup on `old = "foo"`
        // must win over the by_tgt lookup on `new = "bar"` even when
        // the tgt-keyed anchor has higher confidence.
        let step = crate::protolens::elementary::rename_sort(Name::from("foo"), Name::from("bar"));
        let chain = crate::protolens::ProtolensChain::new(vec![step]);
        let anchors = vec![
            // src-keyed: "foo" → high-conf
            mk_anchor("foo", "bar", 0.95, StrategyTag::Alias, "src-alias"),
            // tgt-keyed: different src, but matches target name
            mk_anchor("elsewhere", "bar", 0.99, StrategyTag::UserHint, "tgt-hint"),
        ];
        let steps = enrich_steps(&chain, &anchors);
        assert_eq!(steps.len(), 1);
        // `by_src` lookup on `old` = "foo" wins even though the
        // by_tgt anchor has a higher confidence.
        assert_eq!(
            steps[0].explanation, "src-alias",
            "expected src-keyed anchor to win over tgt-keyed anchor; got {steps:?}",
        );
    }

    #[test]
    fn enrich_steps_correlates_coerce_sort_via_sort_name() {
        // A CoerceSort step on `integer` should correlate with an
        // anchor keyed on `integer` (src or tgt) rather than falling
        // through to a structural explanation.
        use panproto_expr::Expr;
        use panproto_gat::{CoercionClass, ValueKind};
        let step = crate::protolens::elementary::sort_coerce(
            Name::from("integer"),
            ValueKind::Str,
            Expr::Lit(panproto_expr::Literal::Int(0)),
            Some(Expr::Lit(panproto_expr::Literal::Int(0))),
            CoercionClass::Retraction,
        );
        let chain = crate::protolens::ProtolensChain::new(vec![step]);
        let anchors = vec![mk_anchor(
            "integer",
            "string",
            0.9,
            StrategyTag::Coerce,
            "coerce-int-to-str",
        )];
        let steps = enrich_steps(&chain, &anchors);
        assert_eq!(steps.len(), 1);
        assert_eq!(
            steps[0].explanation, "coerce-int-to-str",
            "CoerceSort step must look up by sort_name, not fall to structural"
        );
    }

    #[test]
    fn structural_explanation_handles_coerce_sort() {
        // When no anchor matches, the structural explanation for a
        // CoerceSort step should name the sort and its target kind
        // rather than emitting a bare `Debug`-format fallback.
        use panproto_expr::Expr;
        use panproto_gat::{CoercionClass, ValueKind};
        let step = crate::protolens::elementary::sort_coerce(
            Name::from("myint"),
            ValueKind::Str,
            Expr::Lit(panproto_expr::Literal::Int(0)),
            Some(Expr::Lit(panproto_expr::Literal::Int(0))),
            CoercionClass::Retraction,
        );
        let explanation = structural_explanation(&step);
        assert!(
            explanation.contains("coerce") && explanation.contains("myint"),
            "coerce_sort structural explanation should name the sort; got: {explanation}"
        );
    }

    #[test]
    fn enrich_steps_coerce_sort_falls_back_to_strategy_scan() {
        // Real coerce anchors are keyed by vertex IDs (e.g. `r.n`),
        // not by sort kind names (e.g. `integer`). A CoerceSort step
        // with `sort_name = "integer"` must still correlate with a
        // Coerce anchor even when neither its src nor its tgt equals
        // "integer".
        use panproto_expr::Expr;
        use panproto_gat::{CoercionClass, ValueKind};
        let step = crate::protolens::elementary::sort_coerce(
            Name::from("integer"),
            ValueKind::Str,
            Expr::Lit(panproto_expr::Literal::Int(0)),
            Some(Expr::Lit(panproto_expr::Literal::Int(0))),
            CoercionClass::Retraction,
        );
        let chain = crate::protolens::ProtolensChain::new(vec![step]);
        let anchors = vec![mk_anchor(
            "r.n",
            "r.s",
            0.9,
            StrategyTag::Coerce,
            "int→str via r.n/r.s",
        )];
        let steps = enrich_steps(&chain, &anchors);
        assert_eq!(steps.len(), 1);
        assert_eq!(
            steps[0].explanation, "int→str via r.n/r.s",
            "Coerce strategy scan must find vertex-id-keyed anchors"
        );
        assert_eq!(steps[0].strategy, Some(StrategyTag::Coerce));
    }

    #[test]
    fn enrich_steps_deterministic_under_anchor_permutations() {
        // Two anchors share `src = "foo"` at equal confidence but
        // different strategies. The tie-break must pick by strategy
        // priority (Exact > Alias), independent of the order anchors
        // are presented in the slice.
        let step = crate::protolens::elementary::rename_sort(Name::from("foo"), Name::from("bar"));
        let chain = crate::protolens::ProtolensChain::new(vec![step]);
        let exact = mk_anchor("foo", "bar", 0.8, StrategyTag::Exact, "exact-match");
        let alias = mk_anchor("foo", "bar", 0.8, StrategyTag::Alias, "alias-match");
        let a = enrich_steps(&chain, &[exact.clone(), alias.clone()]);
        let b = enrich_steps(&chain, &[alias, exact]);
        assert_eq!(a[0].explanation, b[0].explanation);
        assert_eq!(a[0].explanation, "exact-match");
    }

    #[test]
    fn score_monotonic_in_quality_coverage_confidence() {
        // Proptest-style sanity: for two candidates with strict
        // dominance in every component, the composite score must
        // respect the dominance.
        use crate::protolens::ProtolensChain;
        let proto = panproto_schema::Protocol {
            name: "t".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into()],
            constraint_sorts: vec![],
            ..panproto_schema::Protocol::default()
        };
        let s = panproto_schema::SchemaBuilder::new(&proto)
            .vertex("a", "object", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let mk = |q: f64, c: f64, conf: f64| -> LensCandidate {
            let chain = ProtolensChain::new(vec![]);
            let lens = chain.instantiate(&s, &proto).unwrap();
            LensCandidate {
                chain,
                lens,
                quality: q,
                coverage: c,
                seed_anchors: vec![],
                steps: vec![CandidateStep {
                    kind: "k".into(),
                    explanation: "e".into(),
                    confidence: conf,
                    strategy: None,
                }],
                strategies_used: vec![],
            }
        };
        // Exhaustively check a small grid — a cheap proxy for a
        // proptest without a new dependency.
        let xs = [0.0_f64, 0.25, 0.5, 0.75, 1.0];
        for &q1 in &xs {
            for &c1 in &xs {
                for &f1 in &xs {
                    for &q2 in &xs {
                        for &c2 in &xs {
                            for &f2 in &xs {
                                if q1 > q2 && c1 >= c2 && f1 >= f2 {
                                    let s1 = mk(q1, c1, f1).score();
                                    let s2 = mk(q2, c2, f2).score();
                                    assert!(
                                        s1 > s2,
                                        "dominance violated: q=({q1},{q2}) c=({c1},{c2}) \
                                         f=({f1},{f2}) → score=({s1},{s2})"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn strategies_used_dedups() {
        let anchors = vec![
            mk_anchor("a", "A", 1.0, StrategyTag::Exact, "exact"),
            mk_anchor("b", "B", 0.9, StrategyTag::Alias, "alias"),
            mk_anchor("c", "C", 0.8, StrategyTag::Alias, "alias-2"),
        ];
        let used = strategies_used(&anchors);
        assert_eq!(used, vec![StrategyTag::Exact, StrategyTag::Alias]);
    }
}
