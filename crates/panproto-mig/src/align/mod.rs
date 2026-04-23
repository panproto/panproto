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
pub mod coerce;
pub mod edge_label;
pub mod exact;
pub mod structural;
pub mod suffix;
pub mod token_similarity;
pub mod type_signature;
pub mod wrap_unwrap;

pub use alias::{AliasDict, alias_anchors, default_alias_dict};
pub use coerce::{CoerceAnchor, coerce_anchors};
pub use edge_label::edge_label_anchors;
pub use exact::exact_anchors;
pub use structural::structural_anchors;
pub use suffix::suffix_anchors;
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
    /// Kind-compatible terminal dot-segment equality. Recovers anchors
    /// for namespaced identifiers that share the same local prop or
    /// field name under disjoint prefixes.
    ExactSuffix,
    /// Same-label same-kind edge on each side with compatible child
    /// vertex kinds. Catches pairs of children reached via labeled
    /// edges whose parents have disjoint identifiers.
    EdgeLabel,
    /// Name match modulo alias dictionary + casing variants.
    Alias,
    /// Token-bag Jaccard + character-n-gram cosine above threshold.
    TokenSimilarity,
    /// Matching sort carrier shapes (edge-kind signatures + cardinality).
    TypeSignature,
    /// Wrap/unwrap detection between record shapes.
    WrapUnwrap,
    /// Sort-coercion via a registered witness lens (Iso, Retraction, or
    /// Projection). Distinct from [`StrategyTag::TypeSignature`] so that
    /// conflict resolution ranks same-kind signatures above cross-kind
    /// bridges.
    Coerce,
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
    // Drop malformed anchors with NaN confidence. `partial_cmp` on NaN
    // returns `None`, and collapsing that to `Ordering::Equal` breaks
    // strict-weak-ordering (transitivity fails): a NaN-confidence
    // anchor would compare equal to every finite-confidence rival and
    // could win the source slot purely on the strategy-priority
    // tiebreaker. Filtering NaN out here is cheaper and more honest
    // than trying to order it consistently; callers that observe this
    // must produce finite scores.
    let mut ranked: Vec<&Anchor> = anchors.iter().filter(|a| !a.confidence.is_nan()).collect();
    ranked.sort_by(|a, b| {
        // `total_cmp` provides a strict-weak order on the remaining
        // finite-and-infinite confidences so the sort is deterministic
        // across runs even under pathological inputs (signed zeros,
        // ±∞). NaN has already been filtered out above.
        b.confidence
            .total_cmp(&a.confidence)
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
        StrategyTag::EdgeLabel => 85,
        StrategyTag::ExactSuffix => 80,
        StrategyTag::Alias => 70,
        StrategyTag::TypeSignature => 60,
        StrategyTag::WrapUnwrap => 55,
        StrategyTag::TokenSimilarity => 50,
        // Cross-kind bridges sit below same-kind heuristics: a token-match
        // within the same kind is stronger evidence than a coercion across
        // kinds.
        StrategyTag::Coerce => 40,
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

/// Kind-and-constraint compatibility test.
///
/// Stricter than [`kinds_compatible`]: in addition to the kind check,
/// every constraint sort declared on the source vertex must be carried
/// by the target vertex with an equal value. A source vertex with no
/// constraints matches any target of the same kind; a source vertex
/// with constraints requires the target to carry a matching constraint
/// of the same sort and value.
///
/// Equality of constraint values is string-literal: the function
/// compares the serialized form stored on
/// [`panproto_schema::Constraint`]. Protocols whose constraint sorts
/// are enumerated discretely (`format`, `knownValues`) therefore get
/// exact match semantics; numeric-range sorts
/// (`maxLength = 200` vs `maxLength = 300`) are treated as distinct.
/// This is the intended behaviour: loosening numeric constraints
/// should not produce a silent match.
///
/// The function is protocol-generic: it consults the schemas' own
/// constraint tables rather than any external vocabulary.
#[must_use]
pub fn kinds_and_constraints_compatible(
    src: &Schema,
    src_id: &Name,
    tgt: &Schema,
    tgt_id: &Name,
) -> bool {
    if !kinds_compatible(src, src_id, tgt, tgt_id) {
        return false;
    }
    let empty: Vec<panproto_schema::Constraint> = Vec::new();
    let src_cs = src.constraints.get(src_id).unwrap_or(&empty);
    if src_cs.is_empty() {
        return true;
    }
    let tgt_cs = tgt.constraints.get(tgt_id).unwrap_or(&empty);
    for sc in src_cs {
        let ok = tgt_cs
            .iter()
            .any(|tc| tc.sort == sc.sort && tc.value == sc.value);
        if !ok {
            return false;
        }
    }
    true
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod constraint_compat_tests {
    use super::*;
    use panproto_schema::{Protocol, SchemaBuilder};

    fn proto_with_format() -> Protocol {
        Protocol {
            name: "t".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["string".into()],
            constraint_sorts: vec!["format".into(), "knownValues".into()],
            ..Protocol::default()
        }
    }

    fn schema_with(
        name: &str,
        kind: &str,
        constraints: &[(&str, &str)],
    ) -> panproto_schema::Schema {
        let proto = proto_with_format();
        let mut b = SchemaBuilder::new(&proto)
            .vertex(name, kind, None::<&str>)
            .unwrap();
        for (sort, value) in constraints {
            b = b.constraint(name, sort, value);
        }
        b.build().unwrap()
    }

    #[test]
    fn source_with_no_constraints_matches_any_target_of_same_kind() {
        let src = schema_with("a", "string", &[]);
        let tgt = schema_with("a", "string", &[("format", "datetime")]);
        assert!(kinds_and_constraints_compatible(
            &src,
            &Name::from("a"),
            &tgt,
            &Name::from("a"),
        ));
    }

    #[test]
    fn matching_format_constraint_compatible() {
        let src = schema_with("a", "string", &[("format", "datetime")]);
        let tgt = schema_with("a", "string", &[("format", "datetime")]);
        assert!(kinds_and_constraints_compatible(
            &src,
            &Name::from("a"),
            &tgt,
            &Name::from("a"),
        ));
    }

    #[test]
    fn missing_constraint_on_target_fails() {
        let src = schema_with("a", "string", &[("format", "datetime")]);
        let tgt = schema_with("a", "string", &[]);
        assert!(!kinds_and_constraints_compatible(
            &src,
            &Name::from("a"),
            &tgt,
            &Name::from("a"),
        ));
    }

    #[test]
    fn differing_format_value_fails() {
        let src = schema_with("a", "string", &[("format", "datetime")]);
        let tgt = schema_with("a", "string", &[("format", "uri")]);
        assert!(!kinds_and_constraints_compatible(
            &src,
            &Name::from("a"),
            &tgt,
            &Name::from("a"),
        ));
    }

    #[test]
    fn mismatched_kind_fails_even_with_identical_constraints() {
        let proto = proto_with_format();
        let other_proto = Protocol {
            obj_kinds: vec!["string".into(), "object".into()],
            ..proto.clone()
        };
        let src = SchemaBuilder::new(&proto)
            .vertex("a", "string", None::<&str>)
            .unwrap()
            .constraint("a", "format", "datetime")
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&other_proto)
            .vertex("a", "object", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        assert!(!kinds_and_constraints_compatible(
            &src,
            &Name::from("a"),
            &tgt,
            &Name::from("a"),
        ));
    }
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

    #[test]
    fn resolve_prefers_type_signature_over_coerce_at_equal_confidence() {
        // Same-kind signature match ranks above cross-kind Coerce bridge.
        let anchors = vec![
            anchor("a", "C", 0.7, StrategyTag::Coerce),
            anchor("a", "T", 0.7, StrategyTag::TypeSignature),
        ];
        let resolved = resolve_anchors(&anchors, false);
        assert_eq!(
            resolved.get(&Name::from("a")).map(Name::as_str),
            Some("T"),
            "TypeSignature must beat Coerce at tied confidence"
        );
    }

    #[test]
    fn resolve_prefers_exact_over_coerce_at_equal_confidence() {
        let anchors = vec![
            anchor("a", "C", 0.7, StrategyTag::Coerce),
            anchor("a", "E", 0.7, StrategyTag::Exact),
        ];
        let resolved = resolve_anchors(&anchors, false);
        assert_eq!(
            resolved.get(&Name::from("a")).map(Name::as_str),
            Some("E"),
            "Exact must beat Coerce at tied confidence"
        );
    }

    #[test]
    fn resolve_monic_three_sources_one_target_keeps_highest_confidence() {
        // Three sources all want the same target at different confidences.
        // Under monic, only the highest-confidence source wins the target;
        // the others are dropped entirely (they have no fallback anchor).
        let anchors = vec![
            anchor("a", "T", 0.6, StrategyTag::Exact),
            anchor("b", "T", 0.9, StrategyTag::Exact),
            anchor("c", "T", 0.75, StrategyTag::Exact),
        ];
        let resolved = resolve_anchors(&anchors, true);
        assert_eq!(resolved.len(), 1);
        assert_eq!(
            resolved.get(&Name::from("b")).map(Name::as_str),
            Some("T"),
            "highest confidence wins the target under monic"
        );
        assert!(!resolved.contains_key(&Name::from("a")));
        assert!(!resolved.contains_key(&Name::from("c")));
    }

    #[test]
    fn resolve_drops_nan_confidence_anchor() {
        // A malformed anchor whose confidence is NaN must never win its
        // source slot over a finite-confidence rival. Prior to the
        // `partial_cmp → total_cmp` + NaN-filter fix, `partial_cmp`
        // returned `None` for NaN and was collapsed to `Ordering::Equal`,
        // so the strategy-priority tiebreaker could hand the slot to
        // the NaN anchor purely because it had a higher-priority tag.
        let anchors = vec![
            anchor("a", "GOOD", 0.8, StrategyTag::Alias),
            Anchor {
                src: Name::from("a"),
                tgt: Name::from("BAD"),
                confidence: f64::NAN,
                strategy: StrategyTag::UserHint,
                explanation: "NaN confidence".into(),
            },
        ];
        let resolved = resolve_anchors(&anchors, false);
        assert_eq!(
            resolved.get(&Name::from("a")).map(Name::as_str),
            Some("GOOD"),
            "NaN-confidence anchor must be dropped even when its strategy tag outranks"
        );
    }

    #[test]
    fn resolve_all_nan_anchors_yields_empty_map() {
        let anchors = vec![
            Anchor {
                src: Name::from("a"),
                tgt: Name::from("X"),
                confidence: f64::NAN,
                strategy: StrategyTag::Exact,
                explanation: String::new(),
            },
            Anchor {
                src: Name::from("b"),
                tgt: Name::from("Y"),
                confidence: f64::NAN,
                strategy: StrategyTag::Exact,
                explanation: String::new(),
            },
        ];
        assert!(resolve_anchors(&anchors, false).is_empty());
        assert!(resolve_anchors(&anchors, true).is_empty());
    }

    #[test]
    fn resolve_handles_infinite_confidence_deterministically() {
        // `+∞` is a finite-enough confidence to survive the NaN filter;
        // it must beat every finite rival. `total_cmp` orders
        // `+∞ > 1.0`, and `b.total_cmp(&a)` under descending sort ranks
        // `+∞` first.
        let anchors = vec![
            anchor("a", "X", 0.9, StrategyTag::Exact),
            Anchor {
                src: Name::from("a"),
                tgt: Name::from("INF"),
                confidence: f64::INFINITY,
                strategy: StrategyTag::Alias,
                explanation: String::new(),
            },
        ];
        let resolved = resolve_anchors(&anchors, false);
        assert_eq!(
            resolved.get(&Name::from("a")).map(Name::as_str),
            Some("INF"),
            "+∞ confidence beats finite confidence under total_cmp"
        );
    }

    #[test]
    fn resolve_empty_anchors_returns_empty_map() {
        let resolved = resolve_anchors(&[], false);
        assert!(resolved.is_empty());
        let resolved = resolve_anchors(&[], true);
        assert!(resolved.is_empty());
    }

    #[test]
    fn strategy_priority_is_strictly_decreasing_across_all_variants() {
        // Audit-of-audits: ensure the documented ordering
        // UserHint > Exact > EdgeLabel > ExactSuffix > Alias >
        // TypeSignature > WrapUnwrap > TokenSimilarity > Coerce >
        // Structural > Llm holds strictly (no ties) across every
        // variant. A future addition of a new variant must explicitly
        // slot into the ordering here.
        let ordered = [
            StrategyTag::UserHint,
            StrategyTag::Exact,
            StrategyTag::EdgeLabel,
            StrategyTag::ExactSuffix,
            StrategyTag::Alias,
            StrategyTag::TypeSignature,
            StrategyTag::WrapUnwrap,
            StrategyTag::TokenSimilarity,
            StrategyTag::Coerce,
            StrategyTag::Structural,
            StrategyTag::Llm,
        ];
        for pair in ordered.windows(2) {
            let hi = strategy_priority(pair[0]);
            let lo = strategy_priority(pair[1]);
            assert!(
                hi > lo,
                "priority must strictly decrease: {:?}({hi}) !> {:?}({lo})",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn strategy_priority_table_is_total_and_ordered() {
        // Explicit snapshot of the priority table so future edits don't
        // silently reshuffle ties.
        let tags = [
            (StrategyTag::UserHint, 100),
            (StrategyTag::Exact, 90),
            (StrategyTag::EdgeLabel, 85),
            (StrategyTag::ExactSuffix, 80),
            (StrategyTag::Alias, 70),
            (StrategyTag::TypeSignature, 60),
            (StrategyTag::WrapUnwrap, 55),
            (StrategyTag::TokenSimilarity, 50),
            (StrategyTag::Coerce, 40),
            (StrategyTag::Structural, 30),
            (StrategyTag::Llm, 20),
        ];
        for (tag, expected) in tags {
            assert_eq!(strategy_priority(tag), expected, "{tag:?}");
        }
    }
}
