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
    /// `score` is not normalized to `[0, 1]` — it is an ordering key
    /// only.
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
#[must_use]
pub fn coverage_ratio(src: &Schema, tgt: &Schema, matched: usize) -> f64 {
    let denom = src.vertex_count().max(tgt.vertex_count()).max(1);
    #[allow(clippy::cast_precision_loss)]
    {
        matched as f64 / denom as f64
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
    let by_src: HashMap<&str, &Anchor> = anchors.iter().map(|a| (a.src.as_str(), a)).fold(
        HashMap::new(),
        |mut acc, (name, anchor)| {
            acc.entry(name)
                .and_modify(|existing: &mut &Anchor| {
                    if anchor.confidence > existing.confidence {
                        *existing = anchor;
                    }
                })
                .or_insert(anchor);
            acc
        },
    );
    let by_tgt: HashMap<&str, &Anchor> = anchors.iter().map(|a| (a.tgt.as_str(), a)).fold(
        HashMap::new(),
        |mut acc, (name, anchor)| {
            acc.entry(name)
                .and_modify(|existing: &mut &Anchor| {
                    if anchor.confidence > existing.confidence {
                        *existing = anchor;
                    }
                })
                .or_insert(anchor);
            acc
        },
    );

    chain
        .steps
        .iter()
        .map(|step| step_to_candidate(step, &by_src, &by_tgt))
        .collect()
}

fn step_to_candidate(
    step: &Protolens,
    by_src: &HashMap<&str, &Anchor>,
    by_tgt: &HashMap<&str, &Anchor>,
) -> CandidateStep {
    use panproto_gat::TheoryTransform;
    let kind = step.name.to_string();

    let matched_anchor = match &step.target.transform {
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
/// For a total morphism this equals `src.vertex_count()`; for a
/// span-with-drops it is strictly smaller and drives the coverage term.
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
