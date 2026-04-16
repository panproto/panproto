//! Protocol-agnostic alignment strategies for auto-lens generation.
//!
//! Each strategy proposes **candidate anchors** (source-vertex ↔
//! target-vertex pairs) that seed the CSP solver's [`SearchOptions::initial`].
//! The CSP then validates every anchor against naturality constraints
//! (edge-preserving morphism); no anchor is accepted merely because a
//! heuristic suggested it. Confidence is a meta-score on the search, not
//! a categorical property.
//!
//! # Stringency tiers
//!
//! Strategies are composed in priority order. Higher-priority strategies
//! (exact, alias) win over lower-priority strategies when they propose
//! conflicting anchors for the same source vertex.
//!
//! The `Stringency` level (in `panproto_lens`) selects which strategies
//! run and at what thresholds: `Strict` runs only [`exact`]; `Balanced`
//! adds [`alias`] and tight [`mod@token_similarity`]; `Lenient` loosens
//! thresholds and engages structural matching; `Exploratory` adds lossy
//! retractions and LM priors.
//!
//! [`SearchOptions::initial`]: crate::SearchOptions::initial

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::Schema;

pub mod alias;
pub mod exact;
pub mod token_similarity;
pub mod type_signature;
pub mod wrap_unwrap;

pub use alias::{AliasDict, alias_anchors, default_alias_dict};
pub use exact::exact_anchors;
pub use token_similarity::{token_anchors, token_similarity};
pub use type_signature::type_signature_anchors;
pub use wrap_unwrap::wrap_unwrap_anchors;

/// Tag identifying which strategy produced an anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyTag {
    /// User-supplied pinning.
    UserHint,
    /// Kind-compatible name equality.
    Exact,
    /// Name match modulo alias dictionary + casing variants.
    Alias,
    /// Token-bag Jaccard + character-n-gram cosine above threshold.
    TokenSimilarity,
    /// Matching sort carrier shapes (edge-kind signatures + cardinality).
    TypeSignature,
    /// Wrap/unwrap detection between record shapes.
    WrapUnwrap,
    /// Pure degree-and-kind-signature matching (last resort).
    Structural,
    /// LM-proposed alignment (feature-gated).
    Llm,
}

/// A proposed alignment between one source vertex and one target vertex,
/// annotated with confidence and provenance.
#[derive(Clone, Debug)]
pub struct Anchor {
    /// Source vertex ID.
    pub src: Name,
    /// Target vertex ID.
    pub tgt: Name,
    /// Score in \[0.0, 1.0\]; higher = stronger proposal.
    pub confidence: f64,
    /// Strategy that produced this anchor.
    pub strategy: StrategyTag,
    /// Human-readable explanation, suitable for UI.
    pub explanation: String,
}

/// Resolve a set of proposed anchors into a single vertex map.
///
/// Anchors are ranked by `(confidence desc, strategy priority desc,
/// source-name asc)`. Higher-priority strategies win over lower ones even
/// at equal confidence, so a conflicting `Exact(0.9)` beats `Alias(0.9)`.
/// Conflicts on the source side (two anchors proposing the same source
/// vertex) are resolved by keeping the winner; conflicts on the target
/// side in `monic` mode likewise keep the winner and drop the loser.
///
/// Returns a `(src → tgt)` map suitable for merging into
/// `SearchOptions::initial`.
#[must_use]
pub fn resolve_anchors(anchors: &[Anchor], monic: bool) -> HashMap<Name, Name> {
    let mut ranked: Vec<&Anchor> = anchors.iter().collect();
    ranked.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| strategy_priority(b.strategy).cmp(&strategy_priority(a.strategy)))
            .then_with(|| a.src.as_str().cmp(b.src.as_str()))
    });

    let mut out: HashMap<Name, Name> = HashMap::new();
    let mut used_targets: std::collections::HashSet<Name> = std::collections::HashSet::new();

    for anchor in ranked {
        if out.contains_key(&anchor.src) {
            continue;
        }
        if monic && used_targets.contains(&anchor.tgt) {
            continue;
        }
        out.insert(anchor.src.clone(), anchor.tgt.clone());
        used_targets.insert(anchor.tgt.clone());
    }

    out
}

/// Priority ordering for resolving equal-confidence conflicts. Higher is
/// better. Exact/UserHint trump heuristic strategies.
const fn strategy_priority(tag: StrategyTag) -> u8 {
    match tag {
        StrategyTag::UserHint => 100,
        StrategyTag::Exact => 90,
        StrategyTag::Alias => 70,
        StrategyTag::TypeSignature => 60,
        StrategyTag::WrapUnwrap => 55,
        StrategyTag::TokenSimilarity => 50,
        StrategyTag::Structural => 30,
        StrategyTag::Llm => 20,
    }
}

/// Kind-compatibility test: two vertices may be aligned only if they share
/// the same schema-level kind (sort carrier). This enforces what a theory
/// morphism requires at the sort level.
#[must_use]
pub fn kinds_compatible(src: &Schema, src_id: &Name, tgt: &Schema, tgt_id: &Name) -> bool {
    src.vertex(src_id)
        .zip(tgt.vertex(tgt_id))
        .is_some_and(|(sv, tv)| sv.kind == tv.kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(src: &str, tgt: &str, confidence: f64, tag: StrategyTag) -> Anchor {
        Anchor {
            src: Name::from(src),
            tgt: Name::from(tgt),
            confidence,
            strategy: tag,
            explanation: format!("{tag:?}: {src} ↔ {tgt}"),
        }
    }

    #[test]
    fn resolve_prefers_exact_over_alias_at_equal_confidence() {
        let anchors = vec![
            anchor("a", "B", 0.9, StrategyTag::Alias),
            anchor("a", "A", 0.9, StrategyTag::Exact),
        ];
        let resolved = resolve_anchors(&anchors, false);
        assert_eq!(
            resolved.get(&Name::from("a")).map(Name::as_str),
            Some("A"),
            "exact should beat alias at tied confidence"
        );
    }

    #[test]
    fn resolve_prefers_higher_confidence() {
        let anchors = vec![
            anchor("a", "X", 0.4, StrategyTag::Exact),
            anchor("a", "Y", 0.8, StrategyTag::TokenSimilarity),
        ];
        let resolved = resolve_anchors(&anchors, false);
        assert_eq!(resolved.get(&Name::from("a")).map(Name::as_str), Some("Y"));
    }

    #[test]
    fn resolve_monic_drops_duplicate_targets() {
        let anchors = vec![
            anchor("a", "T", 0.9, StrategyTag::Exact),
            anchor("b", "T", 0.8, StrategyTag::Alias),
        ];
        let resolved = resolve_anchors(&anchors, true);
        assert_eq!(resolved.len(), 1);
        assert_eq!(
            resolved.get(&Name::from("a")).map(Name::as_str),
            Some("T"),
            "higher-confidence anchor keeps the target"
        );
    }

    #[test]
    fn resolve_non_monic_allows_shared_targets() {
        let anchors = vec![
            anchor("a", "T", 0.9, StrategyTag::Exact),
            anchor("b", "T", 0.8, StrategyTag::Alias),
        ];
        let resolved = resolve_anchors(&anchors, false);
        assert_eq!(resolved.len(), 2);
    }
}
