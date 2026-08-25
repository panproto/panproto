//! Protocol-agnostic alignment strategies for auto-lens generation.
//!
//! Each strategy proposes **candidate anchors** (source-vertex ↔
//! target-vertex pairs). [`evidence::aggregate`] reduces an anchor pool to one
//! score per pair. Callers can pass those scores to an evidence-aware span
//! search, where they enter as unary costs. The automatic lens generator uses
//! a different route: it selects a one-to-one seed map, tries those pairs as
//! provisional hard pins, and compares that result with a search in which the
//! strategy pins have been released.
//!
//! # Priority is real
//!
//! Strategies are ranked by [`StrategyTag::priority`], and the ranking is
//! honoured rather than consulted only on bit-exact ties: each tag owns a band
//! of the `[0, 1]` interval ([`StrategyTag::band`]) and its raw confidence
//! positions it inside that band. So an [`StrategyTag::Exact`] anchor at
//! confidence 0.4 outranks a [`StrategyTag::TokenSimilarity`] anchor at 0.8,
//! which is what the ordering has always claimed. Adjacent bands meet at a
//! shared endpoint rather than being separated by a gap, so the guarantee is
//! `≥` rather than `>`; [`StrategyTag::band`] says where that bites.
//!
//! # Stringency tiers
//!
//! The `Stringency` level (in `panproto_lens`) selects which strategies
//! run and at what thresholds. Every tier runs [`exact`], [`suffix`], and
//! [`edge_label`]. Higher tiers add the strategies documented on
//! `panproto_lens::Stringency`; `Exploratory` adds structural and registered
//! coercion-witness proposals. No production strategy emits
//! [`StrategyTag::Llm`].
//!
//! Aggregation is monotone when one literal anchor pool contains another.
//! Stringency tiers do not always satisfy that premise: another
//! Weisfeiler-Leman round can split a singleton class, and neighborhood
//! propagation can change when the selected parent seeds change.

use panproto_gat::Name;
use panproto_schema::Schema;

use evidence::{Family, Provenance, STRATEGY_COUNT};

pub mod alias;
pub mod coerce;
pub mod defaults;
pub mod description_similarity;
pub mod edge_label;
pub mod evidence;
pub mod exact;
pub mod neighborhood;
pub mod structural;
pub mod suffix;
pub mod token_similarity;
pub mod type_signature;
pub mod wl;
pub mod wrap_unwrap;

pub use alias::{AliasDict, alias_anchors, default_alias_dict};
pub use coerce::{CoerceAnchor, coerce_anchors};
pub use description_similarity::{description_anchors, description_similarity};
pub use edge_label::edge_label_anchors;
pub use exact::exact_anchors;
pub use neighborhood::neighborhood_anchors;
pub use structural::structural_anchors;
pub use suffix::suffix_anchors;
pub use token_similarity::{token_anchors, token_similarity};
pub use type_signature::type_signature_anchors;
pub use wl::wl_anchors;
pub use wrap_unwrap::wrap_unwrap_anchors;

/// Tag identifying which strategy produced an anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyTag {
    /// User-supplied correspondence.
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
    /// Token similarity of vertex descriptions (constraint sort
    /// `description`) above threshold. Only fires on schemas whose
    /// vertices carry description annotations.
    DescriptionSimilarity,
    /// Matching sort carrier shapes (edge-kind signatures + cardinality).
    TypeSignature,
    /// Wrap/unwrap detection between record shapes.
    WrapUnwrap,
    /// Sort-coercion via a registered witness lens (Iso, Retraction, or
    /// Projection). Distinct from [`StrategyTag::TypeSignature`] so that
    /// conflict resolution ranks same-kind signatures above cross-kind
    /// bridges.
    Coerce,
    /// Neighborhood propagation: child-pair scoring seeded from an
    /// already-aligned parent pair via edge-label similarity, edge-kind
    /// equality, kind-and-constraints compatibility, and degree
    /// overlap.
    Neighborhood,
    /// Weisfeiler-Leman color refinement: structural signatures from
    /// iterated neighborhood hashing. Emits anchors for singleton
    /// color classes on both sides.
    WlRefinement,
    /// Pure degree-and-kind-signature matching (last resort).
    Structural,
    /// LM-proposed alignment supplied by an external caller.
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
    ///
    /// Values outside the interval are not a contract violation the type can
    /// prevent, and [`evidence::aggregate`] clamps rather than trusts: a
    /// non-number drops the anchor entirely, and anything else is brought into
    /// range before the provenance ceiling is applied.
    pub confidence: f64,
    /// Strategy that produced this anchor.
    pub strategy: StrategyTag,
    /// What kind of input the evidence was read from, which caps how much
    /// confidence it can claim.
    ///
    /// It is stamped by the emitting branch rather than derived from
    /// [`Anchor::strategy`], because one strategy can emit from two branches
    /// reading two different inputs: [`alias_anchors`] compares vertex
    /// identifiers on its leaf branch and child edge labels on its composite
    /// branch, and the tag cannot tell those apart. [`Family::of`] reads the
    /// provenance for exactly that reason.
    pub provenance: Provenance,
    /// Human-readable explanation, suitable for UI.
    pub explanation: String,
}

impl StrategyTag {
    /// Priority ordering over strategies. Higher is better.
    ///
    /// The numbers are spaced rather than consecutive so a new strategy can be
    /// slotted between two existing ones without renumbering the table, and
    /// only their *order* is read: [`StrategyTag::rank`] is the position in
    /// that order, and the band arithmetic goes through the rank.
    ///
    /// **The table is uncalibrated.** It encodes a judgement about which kind
    /// of evidence is stronger, not a measurement, and it has never been
    /// validated against labelled data.
    #[must_use]
    pub const fn priority(self) -> u8 {
        match self {
            Self::UserHint => 100,
            Self::Exact => 90,
            Self::EdgeLabel => 85,
            Self::ExactSuffix => 80,
            Self::Alias => 70,
            Self::TypeSignature => 60,
            Self::WrapUnwrap => 55,
            Self::TokenSimilarity => 50,
            Self::DescriptionSimilarity => 45,
            // Cross-kind bridges sit below same-kind heuristics: a token-match
            // within the same kind is stronger evidence than a coercion across
            // kinds.
            Self::Coerce => 40,
            Self::Neighborhood => 35,
            Self::WlRefinement => 32,
            Self::Structural => 30,
            Self::Llm => 20,
        }
    }

    /// Position in descending priority order, from `0` for the strongest tag
    /// to `13` for the weakest.
    ///
    /// This is the index into
    /// [`PRIORITY_ORDER`](evidence::PRIORITY_ORDER), and it is what the band
    /// arithmetic reads: the bands must partition `[0, 1]` exactly, which a
    /// spaced priority table cannot do and a dense rank can.
    #[must_use]
    pub const fn rank(self) -> u32 {
        match self {
            Self::UserHint => 0,
            Self::Exact => 1,
            Self::EdgeLabel => 2,
            Self::ExactSuffix => 3,
            Self::Alias => 4,
            Self::TypeSignature => 5,
            Self::WrapUnwrap => 6,
            Self::TokenSimilarity => 7,
            Self::DescriptionSimilarity => 8,
            Self::Coerce => 9,
            Self::Neighborhood => 10,
            Self::WlRefinement => 11,
            Self::Structural => 12,
            Self::Llm => 13,
        }
    }

    /// The **closed** interval of aggregated evidence this tag can occupy,
    /// as `(lo, hi)`.
    ///
    /// The 14 tags cut `[0, 1]` into 14 bands of width `1/14` in descending
    /// priority order, and their union is the whole interval. An anchor's
    /// confidence, capped by its provenance, positions it *within* its band and
    /// never outside it, which is what makes the priority ordering dominate
    /// confidence rather than merely break ties under it.
    ///
    /// # The bands meet, they do not overlap
    ///
    /// Both endpoints are attainable. A tag reaches its `hi` at capped
    /// confidence 1, and reaches its `lo` at capped confidence 0, so each `hi`
    /// is bit-identical to the `lo` of the band above it: all 13 adjacent
    /// boundaries coincide exactly. The bands are therefore closed intervals
    /// that meet at their endpoints rather than half-open intervals that are
    /// disjoint.
    ///
    /// Nothing about the ordering is lost by that, because no band's *interior*
    /// is reachable from another tag: priority dominance holds as `≥`, and a
    /// higher-priority anchor is never scored below a lower-priority one. What
    /// is lost is strictness at the seam. A strongest possible claim by one tag
    /// and a weakest possible claim by the tag above it aggregate to the same
    /// number, so the `max` within a family selects a tie rather than the
    /// higher-priority member. That case is reachable and not exotic:
    /// [`adjust_anchors_by_required_sets`] clamps to exactly 0 and 1, which is
    /// precisely where the seam is.
    #[must_use]
    pub fn band(self) -> (f64, f64) {
        let rank = self.rank();
        let count = f64::from(STRATEGY_COUNT);
        (
            f64::from(STRATEGY_COUNT - 1 - rank) / count,
            f64::from(STRATEGY_COUNT - rank) / count,
        )
    }

    /// The most aggregated evidence an anchor with this tag can ever carry,
    /// which is the top of its [`StrategyTag::band`].
    ///
    /// This is the quantity priority dominance is stated over: for tags `a`
    /// and `b` with `a.priority() > b.priority()`, every `a` anchor scores at
    /// least `b.ceiling()`, whatever the two confidences are.
    #[must_use]
    pub fn ceiling(self) -> f64 {
        self.band().1
    }

    /// Which input this tag's anchors are read from.
    ///
    /// [`Family::of`] is the function the aggregation actually uses: it agrees
    /// with this one except on [`StrategyTag::Alias`], whose two emission
    /// branches read two different inputs and are told apart by the anchor's
    /// [`Anchor::provenance`]. This method reports the family of the leaf
    /// branch, which is the one the tag is named for.
    ///
    /// # One family per tag, and why two candidates for a split did not get one
    ///
    /// The partition is total and each tag lands in exactly one family, which
    /// is what makes the fixed-arity mean well defined. Two tags read inputs
    /// that arguably straddle a family boundary, and both are filed whole:
    ///
    /// * [`StrategyTag::Neighborhood`] is seeded from an already-aligned
    ///   parent, so the identifier and edge-label evidence that produced the
    ///   seed is already counted in those families. Filing it under
    ///   [`Family::Structure`] is what stops it being counted twice.
    /// * [`StrategyTag::WrapUnwrap`] scores a correlated group of fields by how
    ///   well their labels cover one another, so its confidence is entirely a
    ///   label-coverage measure and [`Family::EdgeLabel`] is where it belongs.
    ///   It selects a *parent* pair, which is why an identifier reading is
    ///   tempting, but nothing about the parents' own identifiers enters the
    ///   number.
    ///
    /// Splitting either would need a provenance that tells the branches apart,
    /// as `Alias` has. Both stamp a single provenance unconditionally, so
    /// [`Family::of`] could not act on a split even if one were wanted.
    #[must_use]
    pub const fn family(self) -> Family {
        match self {
            Self::UserHint => Family::UserHint,
            Self::Exact | Self::ExactSuffix | Self::Alias | Self::TokenSimilarity => {
                Family::Identifier
            }
            Self::EdgeLabel | Self::WrapUnwrap => Family::EdgeLabel,
            Self::DescriptionSimilarity => Family::Documentation,
            Self::TypeSignature
            | Self::Neighborhood
            | Self::WlRefinement
            | Self::Structural
            | Self::Llm => Family::Structure,
            Self::Coerce => Family::Coercion,
        }
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

/// Return `true` if `vertex_id` is pointed to by any edge in the
/// schema's `required` table. Protocol-generic: uses the schema's own
/// required-edge annotations rather than any specific vocabulary.
#[must_use]
pub fn vertex_is_required(schema: &Schema, vertex_id: &Name) -> bool {
    schema
        .required
        .values()
        .any(|edges| edges.iter().any(|e| &e.tgt == vertex_id))
}

/// Apply a required-set correspondence tiebreak to `anchors` in place.
///
/// For every anchor proposal `(s, t)`:
/// * `+0.05` when both `s` and `t` are required (positively correlated).
/// * `-0.05` when exactly one side is required (asymmetric: reassigning
///   required data to optional data, or vice versa, usually indicates a
///   schema-shape change the anchor should not silently confirm).
/// * Unchanged when both sides are optional.
///
/// Confidences are clamped to `[0.0, 1.0]` so the adjustment cannot
/// push a heuristic anchor above `Exact = 1.0` or below zero. The
/// magnitude is small by design: it moves an anchor within its
/// [`StrategyTag::band`] and can never move it out of one, so it breaks
/// ties without disturbing the priority ordering.
///
/// It can, however, land an anchor *on* a band boundary, since the clamp
/// targets exactly the two endpoints where adjacent bands meet. That is a tie
/// across bands rather than a reordering of them, and [`StrategyTag::band`]
/// says what such a tie costs.
///
/// # User hints are exempt
///
/// [`StrategyTag::UserHint`] anchors are left untouched. A hint is a caller
/// stating a correspondence, not a heuristic proposing one, so there is no tie
/// for a tiebreak to settle; and because
/// [`aggregate`](evidence::aggregate) folds the hint in with a `max` against
/// its own capped confidence, a `-0.05` here would silently hand back 0.95 for
/// a pair the caller asserted at 1.0. Requiredness disagreeing across the two
/// schemas is a normal consequence of the schema change the caller is hinting
/// through, so it is the most likely case rather than a rare one.
///
/// The adjustment is otherwise the same function of the anchor applied
/// uniformly to the whole pool, so it commutes with the pool growing:
/// aggregating an adjusted superset still dominates aggregating an adjusted
/// subset.
pub fn adjust_anchors_by_required_sets(anchors: &mut [Anchor], src: &Schema, tgt: &Schema) {
    for anchor in anchors.iter_mut() {
        if anchor.strategy == StrategyTag::UserHint {
            continue;
        }
        let sr = vertex_is_required(src, &anchor.src);
        let tr = vertex_is_required(tgt, &anchor.tgt);
        let delta = match (sr, tr) {
            (true, true) => 0.05,
            (true, false) | (false, true) => -0.05,
            (false, false) => 0.0,
        };
        if delta == 0.0 {
            continue;
        }
        anchor.confidence = (anchor.confidence + delta).clamp(0.0, 1.0);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod required_tiebreak_tests {
    use super::*;
    use panproto_schema::{Edge, EdgeRule, Protocol, SchemaBuilder};

    fn proto() -> Protocol {
        Protocol {
            name: "t".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![EdgeRule {
                edge_kind: "prop".into(),
                src_kinds: vec!["object".into()],
                tgt_kinds: vec!["string".into()],
            }],
            obj_kinds: vec!["object".into(), "string".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn schema_with_required(parent: &str, child: &str, required: bool) -> panproto_schema::Schema {
        let p = proto();
        let mut b = SchemaBuilder::new(&p)
            .vertex(parent, "object", None::<&str>)
            .unwrap()
            .vertex(child, "string", None::<&str>)
            .unwrap()
            .edge(parent, child, "prop", Some("f"))
            .unwrap();
        if required {
            let edge = Edge {
                src: Name::from(parent),
                tgt: Name::from(child),
                kind: Name::from("prop"),
                name: Some(Name::from("f")),
            };
            b = b.required(parent, vec![edge]);
        }
        b.build().unwrap()
    }

    #[test]
    fn required_matching_required_boosts() {
        let src = schema_with_required("p", "c", true);
        let tgt = schema_with_required("q", "d", true);
        let mut anchors = vec![Anchor {
            src: Name::from("c"),
            tgt: Name::from("d"),
            confidence: 0.5,
            strategy: StrategyTag::Alias,
            provenance: Provenance::Synonym,
            explanation: String::new(),
        }];
        adjust_anchors_by_required_sets(&mut anchors, &src, &tgt);
        assert!((anchors[0].confidence - 0.55).abs() < 1e-9);
    }

    #[test]
    fn required_to_optional_penalizes() {
        let src = schema_with_required("p", "c", true);
        let tgt = schema_with_required("q", "d", false);
        let mut anchors = vec![Anchor {
            src: Name::from("c"),
            tgt: Name::from("d"),
            confidence: 0.5,
            strategy: StrategyTag::Alias,
            provenance: Provenance::Synonym,
            explanation: String::new(),
        }];
        adjust_anchors_by_required_sets(&mut anchors, &src, &tgt);
        assert!((anchors[0].confidence - 0.45).abs() < 1e-9);
    }

    #[test]
    fn both_optional_unchanged() {
        let src = schema_with_required("p", "c", false);
        let tgt = schema_with_required("q", "d", false);
        let mut anchors = vec![Anchor {
            src: Name::from("c"),
            tgt: Name::from("d"),
            confidence: 0.5,
            strategy: StrategyTag::Alias,
            provenance: Provenance::Synonym,
            explanation: String::new(),
        }];
        adjust_anchors_by_required_sets(&mut anchors, &src, &tgt);
        assert_eq!(anchors[0].confidence, 0.5);
    }

    /// A user hint is a caller's statement, not a proposal to be tiebroken.
    ///
    /// The one-sided case is the one that matters: a hint across a schema
    /// change that made a field optional would otherwise be knocked to 0.95,
    /// and since the hint reaches the score through a `max` against its own
    /// capped confidence, the pair would read 0.95 rather than the 1.0 the
    /// aggregation documents.
    #[test]
    fn a_user_hint_survives_the_required_set_tiebreak() {
        let src = schema_with_required("p", "c", true);
        let tgt = schema_with_required("q", "d", false);
        let mut anchors = vec![Anchor {
            src: Name::from("c"),
            tgt: Name::from("d"),
            confidence: 1.0,
            strategy: StrategyTag::UserHint,
            provenance: Provenance::UserSupplied,
            explanation: "the caller said so".into(),
        }];
        adjust_anchors_by_required_sets(&mut anchors, &src, &tgt);
        assert_eq!(
            anchors[0].confidence, 1.0,
            "the tiebreak must not move a hint"
        );

        let table = evidence::aggregate(&anchors, evidence::AggregationPolicy::StrictPriority);
        let scored = table.get(&Name::from("c"), &Name::from("d")).unwrap();
        assert_eq!(scored.score, 1.0, "a hinted pair reads 1.0");
    }

    /// And the both-required case, which moves in the other direction, is
    /// equally exempt: a hint is not boosted either.
    #[test]
    fn a_user_hint_is_not_boosted_by_the_required_set_tiebreak() {
        let src = schema_with_required("p", "c", true);
        let tgt = schema_with_required("q", "d", true);
        let mut anchors = vec![Anchor {
            src: Name::from("c"),
            tgt: Name::from("d"),
            confidence: 0.6,
            strategy: StrategyTag::UserHint,
            provenance: Provenance::UserSupplied,
            explanation: String::new(),
        }];
        adjust_anchors_by_required_sets(&mut anchors, &src, &tgt);
        assert_eq!(anchors[0].confidence, 0.6);
    }

    #[test]
    fn clamps_to_unit_interval() {
        let src = schema_with_required("p", "c", true);
        let tgt = schema_with_required("q", "d", true);
        let mut anchors = vec![Anchor {
            src: Name::from("c"),
            tgt: Name::from("d"),
            confidence: 0.99,
            strategy: StrategyTag::Exact,
            provenance: Provenance::ExactIdentifier,
            explanation: String::new(),
        }];
        adjust_anchors_by_required_sets(&mut anchors, &src, &tgt);
        assert!(anchors[0].confidence <= 1.0);
        assert!(anchors[0].confidence >= 0.99); // boost applied but clamped
    }

    #[test]
    fn matched_required_beats_mismatched_at_tie() {
        // Two anchors pointing the same source at different targets; after the
        // tiebreak, selection must pick the required-matching one.
        let src = schema_with_required("p", "c", true);
        let p2 = proto();
        // Target with two children: `d` required, `e` optional.
        let tgt = SchemaBuilder::new(&p2)
            .vertex("q", "object", None::<&str>)
            .unwrap()
            .vertex("d", "string", None::<&str>)
            .unwrap()
            .vertex("e", "string", None::<&str>)
            .unwrap()
            .edge("q", "d", "prop", Some("fd"))
            .unwrap()
            .edge("q", "e", "prop", Some("fe"))
            .unwrap()
            .required(
                "q",
                vec![Edge {
                    src: Name::from("q"),
                    tgt: Name::from("d"),
                    kind: Name::from("prop"),
                    name: Some(Name::from("fd")),
                }],
            )
            .build()
            .unwrap();
        let mut anchors = vec![
            Anchor {
                src: Name::from("c"),
                tgt: Name::from("d"),
                confidence: 0.7,
                strategy: StrategyTag::Alias,
                provenance: Provenance::Synonym,
                explanation: String::new(),
            },
            Anchor {
                src: Name::from("c"),
                tgt: Name::from("e"),
                confidence: 0.7,
                strategy: StrategyTag::Alias,
                provenance: Provenance::Synonym,
                explanation: String::new(),
            },
        ];
        adjust_anchors_by_required_sets(&mut anchors, &src, &tgt);
        let picked = evidence::aggregate(&anchors, evidence::AggregationPolicy::StrictPriority)
            .select(
                evidence::Cardinality::Strict,
                evidence::RowFilter::relative_only(),
            )
            .to_map();
        assert_eq!(
            picked.get(&Name::from("c")).map(Name::as_str),
            Some("d"),
            "required-matching anchor must win the source slot"
        );
    }
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
    use evidence::{AggregationPolicy, Cardinality, RowFilter, aggregate};

    fn anchor(
        src: &str,
        tgt: &str,
        confidence: f64,
        tag: StrategyTag,
        provenance: Provenance,
    ) -> Anchor {
        Anchor {
            src: Name::from(src),
            tgt: Name::from(tgt),
            confidence,
            strategy: tag,
            provenance,
            explanation: format!("{tag:?}: {src} ↔ {tgt}"),
        }
    }

    /// One target per source, the way every caller that wants a map gets one.
    fn picked(anchors: &[Anchor]) -> std::collections::HashMap<Name, Name> {
        aggregate(anchors, AggregationPolicy::StrictPriority)
            .select(Cardinality::Strict, RowFilter::relative_only())
            .to_map()
    }

    #[test]
    fn select_prefers_exact_over_alias_at_equal_confidence() {
        let anchors = vec![
            anchor("a", "B", 0.9, StrategyTag::Alias, Provenance::Synonym),
            anchor(
                "a",
                "A",
                0.9,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
        ];
        assert_eq!(
            picked(&anchors).get(&Name::from("a")).map(Name::as_str),
            Some("A"),
            "exact should beat alias at tied confidence"
        );
    }

    /// The inversion the bands remove.
    ///
    /// Confidence used to be the primary key, so a `TokenSimilarity` anchor at
    /// 0.8 took the slot from an `Exact` anchor at 0.4 and the documented
    /// priority ordering was consulted only on bit-exact ties. Under the bands
    /// the ordering is literally true: `Exact` owns `[0.8571, 0.9286]` and
    /// `TokenSimilarity` owns `[0.4286, 0.5000]`, so no confidence can cross
    /// between them.
    #[test]
    fn select_prefers_exact_over_token_similarity_despite_lower_confidence() {
        let anchors = vec![
            anchor(
                "a",
                "X",
                0.4,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
            anchor(
                "a",
                "Y",
                0.8,
                StrategyTag::TokenSimilarity,
                Provenance::Derived,
            ),
        ];
        assert_eq!(
            picked(&anchors).get(&Name::from("a")).map(Name::as_str),
            Some("X")
        );
    }

    #[test]
    fn select_strict_drops_duplicate_targets() {
        let anchors = vec![
            anchor(
                "a",
                "T",
                0.9,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
            anchor("b", "T", 0.8, StrategyTag::Alias, Provenance::Synonym),
        ];
        let resolved = picked(&anchors);
        assert_eq!(resolved.len(), 1);
        assert_eq!(
            resolved.get(&Name::from("a")).map(Name::as_str),
            Some("T"),
            "the stronger anchor keeps the target"
        );
    }

    /// Many-to-one is no longer a selector mode.
    ///
    /// The old resolver took a `monic` flag and, when it was false, let several
    /// sources share a target. Sharing a target is a property of the *morphism*
    /// the search returns, decided by `SearchOptions::monic` against the whole
    /// objective, so no cardinality here reproduces it: the strictly weaker
    /// claim on a contested target loses under every mode.
    #[test]
    fn select_never_reproduces_the_old_many_to_one_fan_out() {
        let anchors = vec![
            anchor(
                "a",
                "T",
                0.9,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
            anchor("b", "T", 0.8, StrategyTag::Alias, Provenance::Synonym),
        ];
        let table = aggregate(&anchors, AggregationPolicy::StrictPriority);
        for cardinality in [
            Cardinality::Strict,
            Cardinality::Permissive,
            Cardinality::default(),
        ] {
            assert_eq!(
                table.select(cardinality, RowFilter::new(0.0, 1.0)).len(),
                1,
                "{cardinality:?} let the weaker claim share the target"
            );
        }
    }

    #[test]
    fn select_prefers_type_signature_over_coerce_at_equal_confidence() {
        // Same-kind signature match ranks above cross-kind Coerce bridge.
        let anchors = vec![
            anchor("a", "C", 0.7, StrategyTag::Coerce, Provenance::Inferred),
            anchor(
                "a",
                "T",
                0.7,
                StrategyTag::TypeSignature,
                Provenance::Inferred,
            ),
        ];
        assert_eq!(
            picked(&anchors).get(&Name::from("a")).map(Name::as_str),
            Some("T"),
            "TypeSignature must beat Coerce at tied confidence"
        );
    }

    #[test]
    fn select_prefers_exact_over_coerce_at_equal_confidence() {
        let anchors = vec![
            anchor("a", "C", 0.7, StrategyTag::Coerce, Provenance::Inferred),
            anchor(
                "a",
                "E",
                0.7,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
        ];
        assert_eq!(
            picked(&anchors).get(&Name::from("a")).map(Name::as_str),
            Some("E"),
            "Exact must beat Coerce at tied confidence"
        );
    }

    #[test]
    fn select_strict_three_sources_one_target_keeps_highest_confidence() {
        // Three sources all want the same target at different confidences.
        // Under the strictest cardinality only the strongest source wins the
        // target; the others are dropped entirely, having no fallback anchor.
        let anchors = vec![
            anchor(
                "a",
                "T",
                0.6,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
            anchor(
                "b",
                "T",
                0.9,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
            anchor(
                "c",
                "T",
                0.75,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
        ];
        let resolved = picked(&anchors);
        assert_eq!(resolved.len(), 1);
        assert_eq!(
            resolved.get(&Name::from("b")).map(Name::as_str),
            Some("T"),
            "highest confidence wins the target within one band"
        );
        assert!(!resolved.contains_key(&Name::from("a")));
        assert!(!resolved.contains_key(&Name::from("c")));
    }

    #[test]
    fn select_drops_nan_confidence_anchor() {
        // A malformed anchor whose confidence is a non-number must never win
        // its source slot over a rival with a real score, however strong its
        // tag. `aggregate` drops it before the ceiling, the band, or the user
        // hint override can read it.
        let anchors = vec![
            anchor("a", "GOOD", 0.8, StrategyTag::Alias, Provenance::Synonym),
            anchor(
                "a",
                "BAD",
                f64::NAN,
                StrategyTag::UserHint,
                Provenance::UserSupplied,
            ),
        ];
        assert_eq!(
            picked(&anchors).get(&Name::from("a")).map(Name::as_str),
            Some("GOOD"),
            "a non-number confidence must be dropped even when its tag outranks"
        );
    }

    #[test]
    fn select_all_nan_anchors_yields_empty_map() {
        let anchors = vec![
            anchor(
                "a",
                "X",
                f64::NAN,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
            anchor(
                "b",
                "Y",
                f64::NAN,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
        ];
        assert!(picked(&anchors).is_empty());
    }

    /// An infinity is a confidence the old resolver ranked above every finite
    /// rival, so a single malformed anchor could take any slot. It is now
    /// clamped into `[0, 1]` and then capped by its provenance, so it competes
    /// on its band like everything else and loses to a stronger tag.
    #[test]
    fn select_clamps_infinite_confidence() {
        let anchors = vec![
            anchor(
                "a",
                "X",
                0.9,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
            anchor(
                "a",
                "INF",
                f64::INFINITY,
                StrategyTag::Alias,
                Provenance::Synonym,
            ),
        ];
        assert_eq!(
            picked(&anchors).get(&Name::from("a")).map(Name::as_str),
            Some("X")
        );
    }

    #[test]
    fn select_empty_anchors_returns_empty_map() {
        assert!(picked(&[]).is_empty());
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
            StrategyTag::DescriptionSimilarity,
            StrategyTag::Coerce,
            StrategyTag::Neighborhood,
            StrategyTag::WlRefinement,
            StrategyTag::Structural,
            StrategyTag::Llm,
        ];
        for pair in ordered.windows(2) {
            let hi = pair[0].priority();
            let lo = pair[1].priority();
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
            (StrategyTag::DescriptionSimilarity, 45),
            (StrategyTag::Coerce, 40),
            (StrategyTag::Neighborhood, 35),
            (StrategyTag::WlRefinement, 32),
            (StrategyTag::Structural, 30),
            (StrategyTag::Llm, 20),
        ];
        for (tag, expected) in tags {
            assert_eq!(tag.priority(), expected, "{tag:?}");
        }
    }

    /// The bands are cut on the rank, so a rank that disagreed with the
    /// priority table would silently invert the ordering the bands exist to
    /// enforce.
    #[test]
    fn rank_is_the_position_in_priority_order() {
        let mut position = 0u32;
        for tag in evidence::PRIORITY_ORDER {
            assert_eq!(tag.rank(), position, "{tag:?}");
            position += 1;
        }
        assert_eq!(position, STRATEGY_COUNT);

        for pair in evidence::PRIORITY_ORDER.windows(2) {
            assert!(pair[0].rank() < pair[1].rank());
            assert!(pair[0].priority() > pair[1].priority());
        }
    }

    /// Every tag has a family, and the one tag whose branches read different
    /// inputs is the only one whose family moves with the provenance.
    #[test]
    fn every_tag_has_a_family() {
        for tag in evidence::PRIORITY_ORDER {
            let default_family = tag.family();
            for provenance in evidence::PROVENANCES {
                let family = Family::of(tag, provenance);
                if tag == StrategyTag::Alias && provenance == Provenance::DeclaredEdgeLabel {
                    assert_eq!(family, Family::EdgeLabel);
                } else {
                    assert_eq!(family, default_family, "{tag:?}/{provenance:?}");
                }
            }
        }
    }
}
