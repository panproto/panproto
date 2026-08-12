//! Aggregate first, select second.
//!
//! A pool of [`Anchor`]s is a pile of overlapping claims: eight strategies read
//! four or five distinct inputs, several of them read the *same* input through
//! different metrics, and any of them can fire on the same source vertex.
//! [`aggregate`] turns that pile into an [`EvidenceTable`], one score in
//! `[0, 1]` per `(source, target)` pair, and that table is what the search
//! reads.
//!
//! # The four steps
//!
//! Each anchor is reduced to one number, `effective`, by two steps:
//!
//! 1. **Provenance ceiling.** A metric cannot report more certainty than its
//!    input licenses. An edit distance of 0.98 between two related synonyms is
//!    still a related synonym, so [`Provenance::ceiling`] caps it. The
//!    *ordering* of the ceilings is a fact about what the input declares and
//!    transfers across corpora; the absolute values are conventional, and
//!    [`defaults`](super::defaults) says so.
//! 2. **Priority band.** The 14 [`StrategyTag`]s occupy 14 bands of width
//!    `1/14`, in the documented priority order, and the capped confidence
//!    positions the anchor *within* its band. Writing `r` for
//!    [`StrategyTag::rank`] and `c` for the capped confidence, the shipped
//!    formula is a single division:
//!
//!    ```text
//!    effective = (13 - r + c) / 14
//!    ```
//!
//!    **Not** the algebraically equal `lo(tag) + (1/14) * c`. The two differ in
//!    `f64` by up to one ulp on most of the grid, and the second form is not
//!    merely imprecise but wrong at the top of a band: at `r = 8` and `c = 1` it
//!    returns a value strictly greater than that band's own `hi`, which is a
//!    one-ulp escape into the band above and would break the priority ordering
//!    at exactly the boundary the ordering is stated over. Anything
//!    reimplementing this, in another language or another binding, must divide
//!    once rather than multiply by a reciprocal and add.
//!
//!    This makes the documented priority ordering literally true rather than a
//!    tiebreak that is consulted only on bit-exact ties. See
//!    [`StrategyTag::band`], which also states the sense in which the bands are
//!    separated: they are closed intervals meeting at their endpoints, so the
//!    ordering holds as `≥` and not as `>`.
//!
//! Then two steps reduce the anchors for one pair to one score:
//!
//! 3. **`max` within a [`Family`].** Strategies that read the same input have
//!    correlated errors, so they are not independent evidence. Within a family
//!    the members genuinely complement each other (a trigram metric fires
//!    where a synonym dictionary cannot, and the reverse), which is the
//!    documented condition under which `Max` is the right aggregator.
//! 4. **Fixed arity mean across families.** The score is the mean of the six
//!    family maxima, with an absent family contributing exactly zero. Averaging
//!    is a shrinkage estimator: a false positive carried by one family in six
//!    is divided by six, while a true positive carried by three is not. **The
//!    arity is fixed on purpose.** A mean normalised by the number of *firing*
//!    families is not monotone in the anchor pool, and the monotonicity of the
//!    search optimum in how much evidence a caller supplies rests on it.
//!
//! A user hint is then folded in with a `max`, so a pair hinted at full
//! confidence reads 1.0. The hint stays soft: it is a cost reduction, never a
//! domain restriction. Because the fold is a `max` against the hint's own
//! capped confidence, a caller who states a hint at less than full confidence
//! gets that number rather than 1.0, and
//! [`adjust_anchors_by_required_sets`](super::adjust_anchors_by_required_sets)
//! leaves hints alone precisely so that nothing else can lower it behind the
//! caller's back.
//!
//! # Why not Dempster-Shafer
//!
//! Its precondition is stochastically independent bodies of evidence, and the
//! product in the numerator of the combination rule *is* that assumption
//! written down. Eight string metrics over one identifier are not eight bodies
//! of evidence; they are eight deterministic functions of one datum, and when
//! the datum is misleading they fail together. The conflict normalisation then
//! turns small masses into near certainty, and combining `k` correlated copies
//! of one 0.7 mass drives belief toward 1 when the evidence supports 0.7. That
//! is the exact inverse of what a redundant matcher family needs.
//!
//! # There is no selection on the search path
//!
//! [`build_cfn`](crate::solve::build::build_cfn) reads the table as a unary
//! cost and the *solver* chooses, globally, subject to the hard constraints and
//! the objective. That is the whole content of "aggregate then select": the
//! selection is the argmin over whole assignments, not a per source argmax.
//! It is what removes the failure where one high confidence claim blocks a pair
//! of moderate claims whose total is greater.
//!
//! [`EvidenceTable::select`] exists for explanations and for callers that want
//! a `HashMap<Name, Name>`. It is never reached from `build_cfn`, and the
//! module enforces that structurally: `select` is public while the unary cost
//! path goes through a crate private `score`.

use std::collections::HashMap;

use panproto_gat::Name;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use super::defaults::{
    CEILING_DECLARED_EDGE_LABEL, CEILING_DECLARED_LABEL, CEILING_DERIVED, CEILING_EXACT_IDENTIFIER,
    CEILING_INFERRED, CEILING_SYNONYM, CEILING_USER_SUPPLIED, EVIDENCE_DELTA, EVIDENCE_FLOOR,
    HYBRID_CARDINALITY, HYBRID_HIGH_CONFIDENCE,
};
use super::{Anchor, StrategyTag};

// ---------------------------------------------------------------------------
// Provenance
// ---------------------------------------------------------------------------

/// What kind of evidence an anchor read, which sets its confidence ceiling.
///
/// The ceiling is applied *after* the metric runs, so a character n-gram
/// cosine of 0.98 computed over two dictionary synonyms still reports at most
/// [`Provenance::Synonym`]'s ceiling. Provenance is carried on the anchor
/// rather than derived from [`StrategyTag`] because one strategy can emit from
/// two branches reading two different inputs, and the tag cannot tell them
/// apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Provenance {
    /// The two canonical identifiers are the same string.
    ExactIdentifier,
    /// A declared name of the vertex itself, such as the terminal segment of a
    /// namespaced identifier.
    DeclaredLabel,
    /// A declared label on an edge, which names a field of a container rather
    /// than the thing the field holds.
    DeclaredEdgeLabel,
    /// A declared alias or cross reference between two names, read from a
    /// dictionary rather than computed.
    Synonym,
    /// A transformation of a declared string: tokenisation, stemming,
    /// abbreviation expansion, prefix splitting.
    Derived,
    /// No declared correspondence at all: structural position, degree, colour
    /// refinement, an embedding, a coercion witness between carriers.
    Inferred,
    /// Stated by the caller.
    UserSupplied,
}

/// Every [`Provenance`], in ceiling order.
pub const PROVENANCES: [Provenance; 7] = [
    Provenance::ExactIdentifier,
    Provenance::UserSupplied,
    Provenance::DeclaredLabel,
    Provenance::DeclaredEdgeLabel,
    Provenance::Synonym,
    Provenance::Derived,
    Provenance::Inferred,
];

impl Provenance {
    /// The highest confidence this provenance licenses.
    ///
    /// The values live in [`defaults`](super::defaults) with their sources.
    #[must_use]
    pub const fn ceiling(self) -> f64 {
        match self {
            Self::ExactIdentifier => CEILING_EXACT_IDENTIFIER,
            Self::DeclaredLabel => CEILING_DECLARED_LABEL,
            Self::DeclaredEdgeLabel => CEILING_DECLARED_EDGE_LABEL,
            Self::Synonym => CEILING_SYNONYM,
            Self::Derived => CEILING_DERIVED,
            Self::Inferred => CEILING_INFERRED,
            Self::UserSupplied => CEILING_USER_SUPPLIED,
        }
    }
}

// ---------------------------------------------------------------------------
// Family
// ---------------------------------------------------------------------------

/// Which *input* the evidence was read from.
///
/// The partition is by what is read, because that is what makes errors
/// correlated: if an identifier is misleading, every metric over that
/// identifier is misled together. Aggregation is `max` within a family and a
/// mean across families, so a four member family of identifier readers cannot
/// silently command four sixths of the weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Family {
    /// A correspondence the caller stated.
    UserHint,
    /// The vertex identifiers.
    Identifier,
    /// The labels edges carry.
    EdgeLabel,
    /// Prose attached to a vertex, read from the `description` constraint.
    Documentation,
    /// Shape: degree, edge kind multisets, colour refinement, propagation from
    /// an already aligned neighbour.
    Structure,
    /// A registered coercion witness between two primitive carriers, which is
    /// the one family that fires only where the kinds disagree.
    Coercion,
}

/// Every [`Family`], in the order [`Evidence::per_family`] stores them.
pub const FAMILIES: [Family; 6] = [
    Family::UserHint,
    Family::Identifier,
    Family::EdgeLabel,
    Family::Documentation,
    Family::Structure,
    Family::Coercion,
];

/// The arity of the across family mean, as a float.
///
/// Stated separately from `FAMILIES.len()` so the divisor is a literal that a
/// reader can check against the doc, and pinned to the array length by
/// `family_arity_matches_the_array`.
const FAMILY_ARITY: f64 = 6.0;

impl Family {
    /// Where this family sits in [`FAMILIES`] and in [`Evidence::per_family`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::UserHint => 0,
            Self::Identifier => 1,
            Self::EdgeLabel => 2,
            Self::Documentation => 3,
            Self::Structure => 4,
            Self::Coercion => 5,
        }
    }

    /// The family an anchor's evidence belongs to.
    ///
    /// [`StrategyTag::family`] answers this for every tag whose emissions all
    /// read one input. [`StrategyTag::Alias`] is the exception: its leaf branch
    /// compares vertex identifiers through the dictionary while its composite
    /// branch compares the labels of child edges, so the two branches belong to
    /// different families and only the provenance the branch stamped on the
    /// anchor distinguishes them.
    #[must_use]
    pub const fn of(tag: StrategyTag, provenance: Provenance) -> Self {
        match tag {
            StrategyTag::Alias => match provenance {
                Provenance::DeclaredEdgeLabel => Self::EdgeLabel,
                _ => Self::Identifier,
            },
            _ => tag.family(),
        }
    }
}

// ---------------------------------------------------------------------------
// Band arithmetic
// ---------------------------------------------------------------------------

/// How many [`StrategyTag`]s there are, and therefore how many priority bands.
pub const STRATEGY_COUNT: u32 = 14;

/// Every [`StrategyTag`], in descending priority order.
///
/// This is the order the bands are cut in, so index into this array is
/// [`StrategyTag::rank`].
pub const PRIORITY_ORDER: [StrategyTag; 14] = [
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

/// How the priority ordering is applied to a raw confidence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AggregationPolicy {
    /// Bands on: a higher priority strategy's weakest claim still outranks a
    /// lower priority strategy's strongest one, so the documented ordering is
    /// literally true. This is the default, because a document that says one
    /// thing while the code does another is the defect class the aggregation
    /// exists to remove.
    #[default]
    StrictPriority,
    /// Bands off: `effective` is the capped confidence itself and priority
    /// survives only as the tiebreak on [`Evidence::top_explanations`], since
    /// two anchors with equal effective evidence contribute equally to the mean
    /// whatever their tags.
    ///
    /// This is the option to choose once a labelled corpus exists to calibrate
    /// against. The priority table has never been validated.
    ConfidenceFirst,
}

/// `x` brought into `[0, 1]`, with a non-number reading as no evidence.
const fn clamp01(x: f64) -> f64 {
    if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) }
}

/// The anchor's confidence, clamped and then capped by its provenance.
const fn capped_confidence(anchor: &Anchor) -> f64 {
    anchor.provenance.ceiling().min(clamp01(anchor.confidence))
}

/// The one number an anchor contributes, after the ceiling and the band.
///
/// Under [`AggregationPolicy::StrictPriority`] the result lies inside the
/// anchor's [`StrategyTag::band`], which is what makes the priority ordering
/// dominate confidence. Under [`AggregationPolicy::ConfidenceFirst`] it is the
/// capped confidence itself.
///
/// A non-number confidence reads as zero rather than propagating, though
/// [`aggregate`] drops such anchors before they reach here.
///
/// # Examples
///
/// ```
/// use panproto_gat::Name;
/// use panproto_mig::align::evidence::{AggregationPolicy, Provenance, effective};
/// use panproto_mig::align::{Anchor, StrategyTag};
///
/// let weak_exact = Anchor {
///     src: Name::from("a"),
///     tgt: Name::from("A"),
///     confidence: 0.0,
///     strategy: StrategyTag::Exact,
///     provenance: Provenance::ExactIdentifier,
///     explanation: String::new(),
/// };
/// let strong_structural = Anchor {
///     strategy: StrategyTag::Structural,
///     provenance: Provenance::Inferred,
///     confidence: 1.0,
///     ..weak_exact.clone()
/// };
///
/// let policy = AggregationPolicy::StrictPriority;
/// assert!(effective(&weak_exact, policy) > effective(&strong_structural, policy));
/// ```
#[must_use]
pub fn effective(anchor: &Anchor, policy: AggregationPolicy) -> f64 {
    let capped = capped_confidence(anchor);
    match policy {
        AggregationPolicy::ConfidenceFirst => capped,
        AggregationPolicy::StrictPriority => {
            let rank = anchor.strategy.rank();
            (f64::from(STRATEGY_COUNT - 1 - rank) + capped) / f64::from(STRATEGY_COUNT)
        }
    }
}

// ---------------------------------------------------------------------------
// The table
// ---------------------------------------------------------------------------

/// What the anchor pool says about one `(source, target)` pair.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Evidence {
    /// The aggregated score, in `[0, 1]`.
    ///
    /// It is the mean of [`Evidence::per_family`], raised to the strength of a
    /// user hint if there is one. Because the mean has fixed arity, a candidate
    /// supported by `k` of the six families cannot exceed `k / 6`.
    pub score: f64,
    /// The strongest `effective` value each family contributed, `0.0` where the
    /// family did not fire. Indexed by [`Family::index`].
    pub per_family: [f64; 6],
    /// The explanations of the three strongest anchors for this pair, ordered
    /// by `effective` and then by priority.
    pub top_explanations: SmallVec<String, 3>,
}

/// The aggregated evidence for every pair any anchor mentioned.
///
/// A pair no anchor mentioned is absent, and reads as zero evidence rather than
/// as an error: absence of evidence is the neutral value of the unary cost
/// term, not a restriction on the search.
#[derive(Clone, Debug, Default)]
pub struct EvidenceTable {
    rows: FxHashMap<(Name, Name), Evidence>,
}

/// Reduce an anchor pool to one score per pair.
///
/// The four steps are the module's four steps: provenance ceiling, priority
/// band, `max` within a family, fixed arity mean across families, then a `max`
/// with the user hint. Anchors whose confidence is a non-number are dropped
/// before any of it, including user hints, because a malformed anchor must not
/// be able to claim a pair on the strength of its tag alone.
///
/// The result is monotone in the pool: aggregating a superset of the anchors
/// produces a score at least as large for every pair. That is what makes the
/// search optimum monotone in how much evidence the caller asked for.
///
/// # Examples
///
/// ```
/// use panproto_gat::Name;
/// use panproto_mig::align::evidence::{
///     AggregationPolicy, Cardinality, Provenance, RowFilter, aggregate,
/// };
/// use panproto_mig::align::{Anchor, StrategyTag};
///
/// let anchors = vec![
///     Anchor {
///         src: Name::from("a"),
///         tgt: Name::from("A"),
///         confidence: 0.4,
///         strategy: StrategyTag::Exact,
///         provenance: Provenance::ExactIdentifier,
///         explanation: "exact identifier match".into(),
///     },
///     Anchor {
///         src: Name::from("a"),
///         tgt: Name::from("B"),
///         confidence: 0.8,
///         strategy: StrategyTag::TokenSimilarity,
///         provenance: Provenance::Derived,
///         explanation: "token similarity 0.80".into(),
///     },
/// ];
///
/// let table = aggregate(&anchors, AggregationPolicy::StrictPriority);
/// let picked = table
///     .select(Cardinality::Strict, RowFilter::relative_only())
///     .to_map();
///
/// // The weaker `Exact` claim outranks the stronger token similarity, because
/// // priority dominates confidence.
/// assert_eq!(picked.get(&Name::from("a")).map(Name::as_str), Some("A"));
/// ```
#[must_use]
pub fn aggregate(anchors: &[Anchor], policy: AggregationPolicy) -> EvidenceTable {
    #[derive(Default)]
    struct Row {
        per_family: [f64; 6],
        hint: f64,
        ranked: Vec<(f64, u8, String)>,
    }

    let mut acc: FxHashMap<(Name, Name), Row> = FxHashMap::default();

    for anchor in anchors {
        if anchor.confidence.is_nan() {
            continue;
        }
        let capped = capped_confidence(anchor);
        let value = effective(anchor, policy);
        let family = Family::of(anchor.strategy, anchor.provenance);

        let row = acc
            .entry((anchor.src.clone(), anchor.tgt.clone()))
            .or_default();
        let slot = &mut row.per_family[family.index()];
        *slot = slot.max(value);
        if anchor.strategy == StrategyTag::UserHint {
            row.hint = row.hint.max(capped);
        }
        row.ranked.push((
            value,
            anchor.strategy.priority(),
            anchor.explanation.clone(),
        ));
    }

    let mut rows: FxHashMap<(Name, Name), Evidence> = FxHashMap::default();
    for (pair, mut row) in acc {
        let mean = row.per_family.iter().sum::<f64>() / FAMILY_ARITY;
        row.ranked.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        let top_explanations = row
            .ranked
            .into_iter()
            .take(3)
            .map(|(_, _, explanation)| explanation)
            .collect();
        rows.insert(
            pair,
            Evidence {
                score: mean.max(row.hint),
                per_family: row.per_family,
                top_explanations,
            },
        );
    }

    EvidenceTable { rows }
}

impl EvidenceTable {
    /// What the table says about one pair, or `None` if no anchor mentioned it.
    #[must_use]
    pub fn get(&self, source: &Name, target: &Name) -> Option<&Evidence> {
        self.rows.get(&(source.clone(), target.clone()))
    }

    /// How many pairs the table holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether no anchor mentioned any pair.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Every pair the table holds, in unspecified order.
    ///
    /// [`EvidenceTable::select`] is the ordered view; this one is for callers
    /// that are going to sort or fold anyway.
    pub fn rows(&self) -> impl Iterator<Item = (&Name, &Name, &Evidence)> {
        self.rows
            .iter()
            .map(|((source, target), evidence)| (source, target, evidence))
    }

    /// The aggregated score for a pair, `0.0` when the table is silent.
    ///
    /// This is the unary cost path, and it is crate private on purpose: the
    /// search must read the whole table and choose globally, never a selection
    /// made for it in advance.
    pub(crate) fn score(&self, source: &Name, target: &Name) -> f64 {
        self.rows
            .get(&(source.clone(), target.clone()))
            .map_or(0.0, |evidence| evidence.score)
    }

    /// Choose a set of pairs off the search path, for explanations and for
    /// callers that want a map.
    ///
    /// The row filter runs first: an absolute floor removes numerically
    /// meaningless rows, then a *relative* tolerance keeps every candidate
    /// within [`RowFilter::delta`] of the best candidate for its source. The
    /// relative test is the decision rule and the absolute one is a sanity
    /// gate, in that order, because a relative test is scale free and survives
    /// miscalibration while an absolute cut does not.
    ///
    /// Surviving candidates are then taken in descending score order and
    /// accepted or rejected by `cardinality`. Ties are broken by source
    /// identifier and then by target identifier, so the result is a
    /// deterministic function of the table.
    ///
    /// # Examples
    ///
    /// ```
    /// use panproto_gat::Name;
    /// use panproto_mig::align::evidence::{
    ///     AggregationPolicy, Cardinality, Provenance, RowFilter, aggregate,
    /// };
    /// use panproto_mig::align::{Anchor, StrategyTag};
    ///
    /// let anchors = vec![
    ///     Anchor {
    ///         src: Name::from("a"),
    ///         tgt: Name::from("T"),
    ///         confidence: 0.9,
    ///         strategy: StrategyTag::Exact,
    ///         provenance: Provenance::ExactIdentifier,
    ///         explanation: String::new(),
    ///     },
    ///     Anchor {
    ///         src: Name::from("b"),
    ///         tgt: Name::from("T"),
    ///         confidence: 0.8,
    ///         strategy: StrategyTag::Alias,
    ///         provenance: Provenance::Synonym,
    ///         explanation: String::new(),
    ///     },
    /// ];
    ///
    /// let table = aggregate(&anchors, AggregationPolicy::StrictPriority);
    /// let selection = table.select(Cardinality::Strict, RowFilter::relative_only());
    ///
    /// // One source per target: the weaker claim on `T` is dropped.
    /// assert_eq!(selection.len(), 1);
    /// assert_eq!(selection.pairs()[0].src.as_str(), "a");
    /// ```
    #[must_use]
    pub fn select(&self, cardinality: Cardinality, filter: RowFilter) -> Selection {
        let mut candidates: Vec<(&Name, &Name, f64)> = self
            .rows
            .iter()
            .filter(|(_, evidence)| evidence.score >= filter.threshold)
            .map(|((source, target), evidence)| (source, target, evidence.score))
            .collect();

        let mut best: FxHashMap<&Name, f64> = FxHashMap::default();
        for (source, _, score) in &candidates {
            let slot = best.entry(*source).or_insert(f64::NEG_INFINITY);
            *slot = slot.max(*score);
        }
        let retained = 1.0 - filter.delta;
        candidates.retain(|(source, _, score)| {
            best.get(source)
                .is_some_and(|best| *score >= retained * *best)
        });

        candidates.sort_by(|a, b| {
            b.2.total_cmp(&a.2)
                .then_with(|| a.0.as_str().cmp(b.0.as_str()))
                .then_with(|| a.1.as_str().cmp(b.1.as_str()))
        });

        let mut state = GreedyState::default();
        let mut pairs = Vec::new();
        for (source, target, score) in candidates {
            if state.accepts(cardinality, source, target, score) {
                state.record(source, target, score);
                pairs.push(SelectedPair {
                    src: source.clone(),
                    tgt: target.clone(),
                    score,
                });
            }
        }

        Selection { pairs }
    }
}

impl crate::solve::build::Evidence for EvidenceTable {
    fn confidence(&self, source: &Name, target: &Name) -> f64 {
        self.score(source, target)
    }
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

/// How many mappings one entity may take part in.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Cardinality {
    /// One target per source and one source per target. A candidate is
    /// rejected if any already accepted mapping shares either endpoint.
    Strict,
    /// A conflicting mapping blocks only if it scores strictly higher, so ties
    /// survive on both sides.
    Permissive,
    /// A relaxed bound of `card + 1` mappings per endpoint above `high_conf`,
    /// and [`Cardinality::Permissive`] below it.
    ///
    /// It is a confidence conditional relaxation of the cardinality bound
    /// rather than a filter: nothing is deleted by crossing `high_conf`, the
    /// constraint changes. Above the line two competing claims are taken to be
    /// more likely a genuine one-to-many relation than a mistake; below it the
    /// score is not trusted to tell those cases apart.
    Hybrid {
        /// Score above which the relaxed bound applies.
        high_conf: f64,
        /// How far the relaxed bound is loosened past one mapping per
        /// endpoint, so `card` admits `card + 1` mappings.
        ///
        /// The offset reading is the one AML's own selector uses: its hybrid
        /// branch relaxes the strict bound by exactly one, and `card` is that
        /// relaxation rather than the resulting count. `card = 1`, the default,
        /// therefore admits two mappings on a contested endpoint.
        card: u8,
    },
}

impl Default for Cardinality {
    fn default() -> Self {
        Self::Hybrid {
            high_conf: HYBRID_HIGH_CONFIDENCE,
            card: HYBRID_CARDINALITY,
        }
    }
}

/// Which rows reach the cardinality rule at all.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct RowFilter {
    /// Absolute floor. A sanity gate, not the decision rule.
    pub threshold: f64,
    /// Relative tolerance: every candidate scoring at least `(1 - delta)`
    /// times the best score for its source survives.
    pub delta: f64,
}

impl Default for RowFilter {
    fn default() -> Self {
        Self {
            threshold: EVIDENCE_FLOOR,
            delta: EVIDENCE_DELTA,
        }
    }
}

impl RowFilter {
    /// A filter with the given absolute floor and relative tolerance.
    #[must_use]
    pub const fn new(threshold: f64, delta: f64) -> Self {
        Self { threshold, delta }
    }

    /// The relative tolerance alone, with no absolute floor.
    ///
    /// [`EVIDENCE_FLOOR`] is a floor on an
    /// aggregated similarity, while [`Evidence::score`] is a mean over six
    /// families, so a candidate carried by one family alone never clears it.
    /// Callers that want the measured selection behaviour, where the relative
    /// tolerance is the decision rule, want this.
    #[must_use]
    pub const fn relative_only() -> Self {
        Self {
            threshold: 0.0,
            delta: EVIDENCE_DELTA,
        }
    }
}

/// One accepted mapping.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct SelectedPair {
    /// The source vertex.
    pub src: Name,
    /// The target vertex.
    pub tgt: Name,
    /// The aggregated evidence that carried it.
    pub score: f64,
}

/// What [`EvidenceTable::select`] accepted, in descending score order.
#[derive(Clone, Debug, Default)]
pub struct Selection {
    pairs: Vec<SelectedPair>,
}

impl Selection {
    /// The accepted mappings, strongest first.
    #[must_use]
    pub fn pairs(&self) -> &[SelectedPair] {
        &self.pairs
    }

    /// How many mappings were accepted.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// Whether nothing was accepted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// The selection as a source to target map, strongest claim per source.
    ///
    /// Under [`Cardinality::Strict`] every source appears at most once, so
    /// nothing is lost. Under the other two a source may carry several
    /// mappings and only the strongest reaches the map.
    #[must_use]
    pub fn to_map(&self) -> HashMap<Name, Name> {
        let mut out = HashMap::with_capacity(self.pairs.len());
        for pair in &self.pairs {
            out.entry(pair.src.clone())
                .or_insert_with(|| pair.tgt.clone());
        }
        out
    }
}

/// The greedy pass's memory of what it has already accepted.
#[derive(Default)]
struct GreedyState<'a> {
    src_count: FxHashMap<&'a Name, u32>,
    tgt_count: FxHashMap<&'a Name, u32>,
    src_best: FxHashMap<&'a Name, f64>,
    tgt_best: FxHashMap<&'a Name, f64>,
}

impl<'a> GreedyState<'a> {
    fn accepts(&self, cardinality: Cardinality, source: &Name, target: &Name, score: f64) -> bool {
        match cardinality {
            Cardinality::Strict => {
                self.src_count.get(source).is_none_or(|count| *count == 0)
                    && self.tgt_count.get(target).is_none_or(|count| *count == 0)
            }
            Cardinality::Permissive => self.no_better_conflict(source, target, score),
            Cardinality::Hybrid { high_conf, card } => {
                let bound = u32::from(card);
                let relaxed = score > high_conf
                    && self
                        .src_count
                        .get(source)
                        .is_none_or(|count| *count <= bound)
                    && self
                        .tgt_count
                        .get(target)
                        .is_none_or(|count| *count <= bound);
                relaxed || self.no_better_conflict(source, target, score)
            }
        }
    }

    fn no_better_conflict(&self, source: &Name, target: &Name, score: f64) -> bool {
        self.src_best.get(source).is_none_or(|best| *best <= score)
            && self.tgt_best.get(target).is_none_or(|best| *best <= score)
    }

    fn record(&mut self, source: &'a Name, target: &'a Name, score: f64) {
        *self.src_count.entry(source).or_insert(0) += 1;
        *self.tgt_count.entry(target).or_insert(0) += 1;
        let src_best = self.src_best.entry(source).or_insert(f64::NEG_INFINITY);
        *src_best = src_best.max(score);
        let tgt_best = self.tgt_best.entry(target).or_insert(f64::NEG_INFINITY);
        *tgt_best = tgt_best.max(score);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn anchor(src: &str, tgt: &str, confidence: f64, tag: StrategyTag, prov: Provenance) -> Anchor {
        Anchor {
            src: Name::from(src),
            tgt: Name::from(tgt),
            confidence,
            strategy: tag,
            provenance: prov,
            explanation: format!("{tag:?}/{prov:?}: {src} against {tgt}"),
        }
    }

    fn score_of(table: &EvidenceTable, src: &str, tgt: &str) -> f64 {
        table.score(&Name::from(src), &Name::from(tgt))
    }

    #[test]
    fn family_arity_matches_the_array() {
        assert_eq!(FAMILIES.len(), 6);
        assert_eq!(FAMILY_ARITY, 6.0);
        assert_eq!(PRIORITY_ORDER.len(), 14);
        assert_eq!(STRATEGY_COUNT, 14);
        assert_eq!(PROVENANCES.len(), 7);
    }

    #[test]
    fn family_index_is_a_bijection_onto_the_array() {
        for (index, family) in FAMILIES.iter().enumerate() {
            assert_eq!(family.index(), index, "{family:?}");
        }
    }

    /// The partition is over every tag, it covers every tag, and no tag lands
    /// in two families. A new tag that forgot its family arm cannot compile,
    /// but a new tag slipped into an existing arm by accident would still be
    /// caught by the count.
    #[test]
    fn family_partition_is_total_and_disjoint() {
        let mut buckets: Vec<Vec<StrategyTag>> = vec![Vec::new(); FAMILIES.len()];
        for tag in PRIORITY_ORDER {
            buckets[tag.family().index()].push(tag);
        }

        let total: usize = buckets.iter().map(Vec::len).sum();
        assert_eq!(
            total,
            PRIORITY_ORDER.len(),
            "the buckets do not cover every tag exactly once"
        );

        for (i, left) in buckets.iter().enumerate() {
            for right in buckets.iter().skip(i + 1) {
                for tag in left {
                    assert!(!right.contains(tag), "{tag:?} is in two families");
                }
            }
        }

        assert_eq!(buckets[Family::UserHint.index()], [StrategyTag::UserHint]);
        assert_eq!(
            buckets[Family::Identifier.index()],
            [
                StrategyTag::Exact,
                StrategyTag::ExactSuffix,
                StrategyTag::Alias,
                StrategyTag::TokenSimilarity,
            ]
        );
        assert_eq!(
            buckets[Family::EdgeLabel.index()],
            [StrategyTag::EdgeLabel, StrategyTag::WrapUnwrap]
        );
        assert_eq!(
            buckets[Family::Documentation.index()],
            [StrategyTag::DescriptionSimilarity]
        );
        assert_eq!(
            buckets[Family::Structure.index()],
            [
                StrategyTag::TypeSignature,
                StrategyTag::Neighborhood,
                StrategyTag::WlRefinement,
                StrategyTag::Structural,
                StrategyTag::Llm,
            ]
        );
        assert_eq!(buckets[Family::Coercion.index()], [StrategyTag::Coerce]);
    }

    /// The one tag whose family the tag alone cannot decide.
    #[test]
    fn alias_family_follows_the_branch_provenance() {
        assert_eq!(
            Family::of(StrategyTag::Alias, Provenance::Synonym),
            Family::Identifier
        );
        assert_eq!(
            Family::of(StrategyTag::Alias, Provenance::DeclaredEdgeLabel),
            Family::EdgeLabel
        );
        for provenance in PROVENANCES {
            assert_eq!(
                Family::of(StrategyTag::Exact, provenance),
                Family::Identifier,
                "provenance must not move a tag whose emissions read one input"
            );
        }
    }

    /// The band table, to four places, exactly as the design states it.
    #[test]
    fn band_table_matches_the_specification() {
        let expected = [
            (StrategyTag::UserHint, 0.9286, 1.0000),
            (StrategyTag::Exact, 0.8571, 0.9286),
            (StrategyTag::EdgeLabel, 0.7857, 0.8571),
            (StrategyTag::ExactSuffix, 0.7143, 0.7857),
            (StrategyTag::Alias, 0.6429, 0.7143),
            (StrategyTag::TypeSignature, 0.5714, 0.6429),
            (StrategyTag::WrapUnwrap, 0.5000, 0.5714),
            (StrategyTag::TokenSimilarity, 0.4286, 0.5000),
            (StrategyTag::DescriptionSimilarity, 0.3571, 0.4286),
            (StrategyTag::Coerce, 0.2857, 0.3571),
            (StrategyTag::Neighborhood, 0.2143, 0.2857),
            (StrategyTag::WlRefinement, 0.1429, 0.2143),
            (StrategyTag::Structural, 0.0714, 0.1429),
            (StrategyTag::Llm, 0.0000, 0.0714),
        ];
        for (tag, lo, hi) in expected {
            let (got_lo, got_hi) = tag.band();
            assert!((got_lo - lo).abs() < 5e-5, "{tag:?} lo {got_lo}");
            assert!((got_hi - hi).abs() < 5e-5, "{tag:?} hi {got_hi}");
            assert_eq!(tag.ceiling(), got_hi, "{tag:?}");
        }
        assert_eq!(PRIORITY_ORDER[0].band().1, 1.0);
        assert_eq!(PRIORITY_ORDER[13].band().0, 0.0);
    }

    /// The bands are closed intervals that meet, not half-open ones that are
    /// separated, and both endpoints are attainable.
    ///
    /// This pins what [`StrategyTag::band`] documents. It is not a defect, and
    /// priority dominance survives it as `≥`, but a reader who assumed a gap
    /// would conclude that the `max` within a family always selects the
    /// higher-priority member, and at a shared endpoint it does not.
    #[test]
    fn adjacent_bands_meet_at_a_shared_endpoint() {
        for pair in PRIORITY_ORDER.windows(2) {
            let (higher, lower) = (pair[0], pair[1]);
            assert_eq!(
                lower.band().1.to_bits(),
                higher.band().0.to_bits(),
                "{lower:?} hi and {higher:?} lo must be the same bits"
            );
        }

        let policy = AggregationPolicy::StrictPriority;
        for tag in PRIORITY_ORDER {
            let (lo, hi) = tag.band();
            let full = anchor("s", "t", 1.0, tag, Provenance::UserSupplied);
            let none = anchor("s", "t", 0.0, tag, Provenance::UserSupplied);
            assert_eq!(effective(&full, policy), hi, "{tag:?} cannot reach its hi");
            assert_eq!(effective(&none, policy), lo, "{tag:?} cannot reach its lo");
        }
    }

    /// The shipped formula divides once. The algebraically equal form that
    /// multiplies by `1/14` and adds `lo` is not equal in `f64`, and at the top
    /// of one band it escapes upward into the band above.
    ///
    /// The escape is the reason the module documents the division rather than
    /// the reciprocal: a reimplementation that follows the second form breaks
    /// priority dominance at exactly the boundary dominance is stated over.
    #[test]
    fn the_shipped_formula_stays_in_band_where_the_reciprocal_form_does_not() {
        let policy = AggregationPolicy::StrictPriority;
        let mut escapes = 0u32;
        let mut disagreements = 0u32;

        for tag in PRIORITY_ORDER {
            let (lo, hi) = tag.band();
            for step in 0..=140 {
                let confidence = f64::from(step) / 140.0;
                let shipped = effective(
                    &anchor("s", "t", confidence, tag, Provenance::UserSupplied),
                    policy,
                );
                let reciprocal = (1.0 / 14.0f64).mul_add(confidence, lo);

                assert!(
                    shipped >= lo && shipped <= hi,
                    "{tag:?} at {confidence} left its band"
                );
                if shipped != reciprocal {
                    disagreements += 1;
                }
                if reciprocal > hi {
                    escapes += 1;
                }
            }
        }

        assert!(
            disagreements > 0,
            "the two forms are being computed identically, so this test is not \
             measuring what it claims"
        );
        assert!(
            escapes > 0,
            "the reciprocal form no longer escapes, so the warning in the \
             module docs needs rewriting rather than keeping"
        );
    }

    /// Theorem 7.1, over all 91 ordered pairs of distinct tags: a higher
    /// priority tag's weakest possible claim still beats a lower priority
    /// tag's strongest possible one.
    #[test]
    fn priority_dominance_holds_over_all_ordered_tag_pairs() {
        let mut pairs = 0u32;
        for (i, high) in PRIORITY_ORDER.iter().enumerate() {
            for low in PRIORITY_ORDER.iter().skip(i + 1) {
                pairs += 1;
                assert!(high.priority() > low.priority(), "{high:?} {low:?}");
                assert!(
                    high.band().0 >= low.band().1,
                    "band overlap: {high:?} {:?} against {low:?} {:?}",
                    high.band(),
                    low.band()
                );

                let weakest = anchor("s", "t", 0.0, *high, Provenance::Inferred);
                let strongest = anchor("s", "u", 1.0, *low, Provenance::ExactIdentifier);
                let policy = AggregationPolicy::StrictPriority;
                assert!(
                    effective(&weakest, policy) >= effective(&strongest, policy),
                    "{high:?} at 0.0 must dominate {low:?} at 1.0"
                );
            }
        }
        assert_eq!(pairs, 91);
    }

    #[test]
    fn effective_stays_in_the_unit_interval() {
        let confidences = [
            f64::NAN,
            f64::NEG_INFINITY,
            -1.0,
            0.0,
            0.5,
            1.0,
            2.0,
            f64::INFINITY,
        ];
        for tag in PRIORITY_ORDER {
            for provenance in PROVENANCES {
                for confidence in confidences {
                    for policy in [
                        AggregationPolicy::StrictPriority,
                        AggregationPolicy::ConfidenceFirst,
                    ] {
                        let value =
                            effective(&anchor("s", "t", confidence, tag, provenance), policy);
                        assert!(
                            (0.0..=1.0).contains(&value),
                            "{tag:?}/{provenance:?}/{confidence} gave {value}"
                        );
                        if policy == AggregationPolicy::StrictPriority {
                            let (lo, hi) = tag.band();
                            assert!(value >= lo && value <= hi, "{tag:?} left its band");
                        }
                    }
                }
            }
        }
    }

    /// The provenance cap binds even when the metric reports more.
    #[test]
    fn provenance_ceiling_caps_the_confidence() {
        let capped = capped_confidence(&anchor(
            "s",
            "t",
            0.98,
            StrategyTag::Alias,
            Provenance::Synonym,
        ));
        assert_eq!(capped, CEILING_SYNONYM);
    }

    /// Step 4's arity is fixed, so a family that did not fire contributes zero
    /// rather than dropping out of the divisor.
    #[test]
    fn absent_family_contributes_exactly_zero() {
        let anchors = vec![anchor(
            "a",
            "A",
            1.0,
            StrategyTag::Exact,
            Provenance::ExactIdentifier,
        )];
        let table = aggregate(&anchors, AggregationPolicy::StrictPriority);
        let evidence = table.get(&Name::from("a"), &Name::from("A")).unwrap();

        for (index, value) in evidence.per_family.iter().enumerate() {
            if index == Family::Identifier.index() {
                assert!(*value > 0.0);
            } else {
                assert_eq!(*value, 0.0, "family {index} fired without an anchor");
            }
        }
        assert_eq!(
            evidence.score,
            evidence.per_family[Family::Identifier.index()] / FAMILY_ARITY
        );
        assert!(evidence.score <= 1.0 / FAMILY_ARITY);
    }

    /// A second family raises the mean without touching the first family's
    /// contribution, which is the shrinkage the fixed arity buys.
    #[test]
    fn a_second_family_raises_the_mean() {
        let one = vec![anchor(
            "a",
            "A",
            1.0,
            StrategyTag::Exact,
            Provenance::ExactIdentifier,
        )];
        let mut two = one.clone();
        two.push(anchor(
            "a",
            "A",
            1.0,
            StrategyTag::EdgeLabel,
            Provenance::DeclaredEdgeLabel,
        ));

        let lo = aggregate(&one, AggregationPolicy::StrictPriority);
        let hi = aggregate(&two, AggregationPolicy::StrictPriority);
        assert!(score_of(&hi, "a", "A") > score_of(&lo, "a", "A"));
    }

    /// Two readings of the same input do not compound: within a family the
    /// aggregation is `max`, so the weaker of the two changes nothing.
    #[test]
    fn a_second_reading_of_one_family_does_not_compound() {
        let one = vec![anchor(
            "a",
            "A",
            1.0,
            StrategyTag::Exact,
            Provenance::ExactIdentifier,
        )];
        let mut two = one.clone();
        two.push(anchor(
            "a",
            "A",
            0.9,
            StrategyTag::TokenSimilarity,
            Provenance::Derived,
        ));

        let lo = aggregate(&one, AggregationPolicy::StrictPriority);
        let hi = aggregate(&two, AggregationPolicy::StrictPriority);
        assert_eq!(score_of(&lo, "a", "A"), score_of(&hi, "a", "A"));
    }

    #[test]
    fn user_hint_saturates_the_score() {
        let anchors = vec![anchor(
            "a",
            "A",
            1.0,
            StrategyTag::UserHint,
            Provenance::UserSupplied,
        )];
        let table = aggregate(&anchors, AggregationPolicy::StrictPriority);
        assert_eq!(score_of(&table, "a", "A"), 1.0);
    }

    /// A malformed anchor must not claim a pair on the strength of its tag, so
    /// the non-number filter runs before the user hint override.
    #[test]
    fn nan_confidence_anchor_is_dropped_even_when_it_is_a_hint() {
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
        let table = aggregate(&anchors, AggregationPolicy::StrictPriority);
        assert_eq!(table.len(), 1);
        assert_eq!(score_of(&table, "a", "BAD"), 0.0);
        assert!(score_of(&table, "a", "GOOD") > 0.0);
    }

    /// Under the other policy the bands come off and the raw confidence leads,
    /// which is what the pipeline did before the bands existed.
    #[test]
    fn confidence_first_drops_the_bands() {
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
        let banded = aggregate(&anchors, AggregationPolicy::StrictPriority);
        assert!(score_of(&banded, "a", "X") > score_of(&banded, "a", "Y"));

        let flat = aggregate(&anchors, AggregationPolicy::ConfidenceFirst);
        assert!(score_of(&flat, "a", "Y") > score_of(&flat, "a", "X"));
    }

    #[test]
    fn explanations_are_ranked_and_capped_at_three() {
        let anchors = vec![
            anchor("a", "A", 1.0, StrategyTag::Structural, Provenance::Inferred),
            anchor(
                "a",
                "A",
                1.0,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
            anchor("a", "A", 1.0, StrategyTag::Alias, Provenance::Synonym),
            anchor("a", "A", 1.0, StrategyTag::Coerce, Provenance::Inferred),
        ];
        let table = aggregate(&anchors, AggregationPolicy::StrictPriority);
        let evidence = table.get(&Name::from("a"), &Name::from("A")).unwrap();
        assert_eq!(evidence.top_explanations.len(), 3);
        assert!(evidence.top_explanations[0].starts_with("Exact"));
        assert!(evidence.top_explanations[1].starts_with("Alias"));
        assert!(evidence.top_explanations[2].starts_with("Coerce"));
    }

    #[test]
    fn score_is_zero_for_an_unknown_pair() {
        let table = aggregate(&[], AggregationPolicy::StrictPriority);
        assert!(table.is_empty());
        assert_eq!(score_of(&table, "a", "A"), 0.0);
        assert!(table.get(&Name::from("a"), &Name::from("A")).is_none());
        assert!(
            table
                .select(Cardinality::Strict, RowFilter::relative_only())
                .is_empty()
        );
    }

    #[test]
    fn select_strict_is_one_to_one_on_both_sides() {
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
        let selection = table.select(Cardinality::Strict, RowFilter::relative_only());
        assert_eq!(selection.len(), 1);
        assert_eq!(selection.pairs()[0].src.as_str(), "a");
    }

    #[test]
    fn select_permissive_keeps_ties() {
        let anchors = vec![
            anchor(
                "a",
                "T",
                0.9,
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
        ];
        let table = aggregate(&anchors, AggregationPolicy::StrictPriority);
        assert_eq!(
            table
                .select(Cardinality::Permissive, RowFilter::relative_only())
                .len(),
            2
        );
        assert_eq!(
            table
                .select(Cardinality::Strict, RowFilter::relative_only())
                .len(),
            1
        );
    }

    #[test]
    fn select_hybrid_relaxes_the_bound_above_the_line() {
        // A hint's evidence is its capped confidence, so three hints of
        // decreasing strength give three distinct scores above the line.
        let anchors = vec![
            anchor(
                "a",
                "T",
                1.0,
                StrategyTag::UserHint,
                Provenance::UserSupplied,
            ),
            anchor(
                "b",
                "T",
                0.9,
                StrategyTag::UserHint,
                Provenance::UserSupplied,
            ),
            anchor(
                "c",
                "T",
                0.8,
                StrategyTag::UserHint,
                Provenance::UserSupplied,
            ),
        ];
        let table = aggregate(&anchors, AggregationPolicy::StrictPriority);
        let hybrid = table.select(Cardinality::default(), RowFilter::relative_only());
        assert_eq!(
            hybrid.len(),
            2,
            "the relaxed bound admits one more mapping per endpoint, not all three"
        );
        assert_eq!(
            table
                .select(Cardinality::Strict, RowFilter::relative_only())
                .len(),
            1
        );

        // `card` is an offset, not a count: it admits `card + 1` mappings on a
        // contested endpoint. Pinned at two points so the reading is checked
        // rather than a single value being memorised.
        for card in 1u8..=2 {
            let selection = table.select(
                Cardinality::Hybrid {
                    high_conf: HYBRID_HIGH_CONFIDENCE,
                    card,
                },
                RowFilter::relative_only(),
            );
            assert_eq!(
                selection
                    .pairs()
                    .iter()
                    .filter(|pair| pair.tgt.as_str() == "T")
                    .count(),
                usize::from(card) + 1,
                "card = {card} admits card + 1 mappings on the contested target"
            );
        }
    }

    /// Below the high confidence line the hybrid rule is the permissive rule.
    #[test]
    fn select_hybrid_falls_back_below_the_line() {
        let anchors = vec![
            anchor(
                "a",
                "T",
                0.9,
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
                0.1,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
        ];
        let table = aggregate(&anchors, AggregationPolicy::StrictPriority);
        let selection = table.select(Cardinality::default(), RowFilter::new(0.0, 1.0));
        assert_eq!(
            selection.len(),
            2,
            "the tied pair survives, the weak one does not"
        );
    }

    /// The delta is relative to the best candidate for the *source*, so it is
    /// invariant to any rescaling of a row. What it decides is whether a
    /// source's runner up is still available once its first choice has been
    /// taken by a stronger claim from elsewhere.
    #[test]
    fn row_filter_delta_is_relative_to_the_best_for_the_source() {
        let anchors = vec![
            anchor(
                "b",
                "T",
                1.0,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
            anchor(
                "a",
                "T",
                0.9,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
            anchor("a", "U", 1.0, StrategyTag::Alias, Provenance::Synonym),
        ];
        let table = aggregate(&anchors, AggregationPolicy::StrictPriority);

        let tight = table.select(Cardinality::Strict, RowFilter::new(0.0, 0.02));
        assert_eq!(tight.len(), 1, "`a`'s runner up is far behind its best");
        assert_eq!(tight.pairs()[0].src.as_str(), "b");

        let loose = table.select(Cardinality::Strict, RowFilter::new(0.0, 1.0));
        let mapped = loose.to_map();
        assert_eq!(loose.len(), 2);
        assert_eq!(mapped.get(&Name::from("b")).map(Name::as_str), Some("T"));
        assert_eq!(mapped.get(&Name::from("a")).map(Name::as_str), Some("U"));
    }

    /// The absolute floor is a floor on the aggregated score, and the
    /// aggregated score is a six family mean, so a single family candidate does
    /// not clear the shipped default. This is the interaction
    /// [`RowFilter::relative_only`] exists for.
    #[test]
    fn the_default_floor_cuts_single_family_candidates() {
        let anchors = vec![anchor(
            "a",
            "A",
            1.0,
            StrategyTag::Exact,
            Provenance::ExactIdentifier,
        )];
        let table = aggregate(&anchors, AggregationPolicy::StrictPriority);
        assert!(
            table
                .select(Cardinality::Strict, RowFilter::default())
                .is_empty()
        );
        assert_eq!(
            table
                .select(Cardinality::Strict, RowFilter::relative_only())
                .len(),
            1
        );
    }

    /// A user hint reads 1.0, so it clears even the shipped absolute floor.
    #[test]
    fn a_user_hint_clears_the_default_floor() {
        let anchors = vec![anchor(
            "a",
            "A",
            1.0,
            StrategyTag::UserHint,
            Provenance::UserSupplied,
        )];
        let table = aggregate(&anchors, AggregationPolicy::StrictPriority);
        assert_eq!(
            table
                .select(Cardinality::Strict, RowFilter::default())
                .len(),
            1
        );
    }

    #[test]
    fn selection_is_deterministic_across_input_permutations() {
        let mut anchors = vec![
            anchor(
                "a",
                "A",
                1.0,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
            anchor(
                "b",
                "B",
                1.0,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
            anchor(
                "c",
                "C",
                1.0,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
            anchor(
                "a",
                "B",
                1.0,
                StrategyTag::Exact,
                Provenance::ExactIdentifier,
            ),
        ];
        let baseline: Vec<(String, String)> =
            aggregate(&anchors, AggregationPolicy::StrictPriority)
                .select(Cardinality::Strict, RowFilter::relative_only())
                .pairs()
                .iter()
                .map(|pair| (pair.src.as_str().to_owned(), pair.tgt.as_str().to_owned()))
                .collect();

        anchors.reverse();
        let reversed: Vec<(String, String)> =
            aggregate(&anchors, AggregationPolicy::StrictPriority)
                .select(Cardinality::Strict, RowFilter::relative_only())
                .pairs()
                .iter()
                .map(|pair| (pair.src.as_str().to_owned(), pair.tgt.as_str().to_owned()))
                .collect();

        assert_eq!(baseline, reversed);
    }

    /// The table is what `build_cfn` reads, through the trait rather than
    /// through a selection made for it.
    #[test]
    fn the_table_is_an_evidence_source_for_the_builder() {
        use crate::solve::build::Evidence as _;

        let anchors = vec![anchor(
            "a",
            "A",
            1.0,
            StrategyTag::UserHint,
            Provenance::UserSupplied,
        )];
        let table = aggregate(&anchors, AggregationPolicy::StrictPriority);
        assert_eq!(table.confidence(&Name::from("a"), &Name::from("A")), 1.0);
        assert_eq!(table.confidence(&Name::from("a"), &Name::from("Z")), 0.0);
        assert_eq!(table.rows().count(), 1);

        // `build_cfn` takes `&dyn Evidence`, so the table must be usable
        // behind the same reference the builder asks for.
        let erased: &dyn crate::solve::build::Evidence = &table;
        assert_eq!(erased.confidence(&Name::from("a"), &Name::from("A")), 1.0);
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod property {
    use super::*;
    use proptest::prelude::*;

    fn arb_tag() -> impl Strategy<Value = StrategyTag> {
        (0usize..PRIORITY_ORDER.len()).prop_map(|i| PRIORITY_ORDER[i])
    }

    fn arb_provenance() -> impl Strategy<Value = Provenance> {
        (0usize..PROVENANCES.len()).prop_map(|i| PROVENANCES[i])
    }

    fn arb_anchor() -> impl Strategy<Value = Anchor> {
        (
            "[a-d]",
            "[W-Z]",
            -0.5f64..1.5f64,
            arb_tag(),
            arb_provenance(),
        )
            .prop_map(|(src, tgt, confidence, strategy, provenance)| Anchor {
                src: Name::from(src.as_str()),
                tgt: Name::from(tgt.as_str()),
                confidence,
                strategy,
                provenance,
                explanation: String::new(),
            })
    }

    fn arb_policy() -> impl Strategy<Value = AggregationPolicy> {
        prop_oneof![
            Just(AggregationPolicy::StrictPriority),
            Just(AggregationPolicy::ConfidenceFirst),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Lemma 4.2. Aggregating a superset of the anchors produces a score at
        /// least as large for every pair, which is what the monotonicity of the
        /// search optimum in the stringency tier rests on.
        #[test]
        fn aggregate_monotone_in_pool(
            pool in prop::collection::vec(arb_anchor(), 0..12),
            extension in prop::collection::vec(arb_anchor(), 1..6),
            policy in arb_policy(),
        ) {
            let small = aggregate(&pool, policy);
            let mut union = pool;
            union.extend(extension);
            let large = aggregate(&union, policy);

            for (source, target, evidence) in large.rows() {
                let before = small.score(source, target);
                prop_assert!(
                    evidence.score >= before,
                    "{}/{}: {} fell below {}",
                    source.as_str(),
                    target.as_str(),
                    evidence.score,
                    before
                );
            }
            for (source, target, evidence) in small.rows() {
                prop_assert!(large.score(source, target) >= evidence.score);
            }
        }

        /// Every score the table can hold is a confidence the builder accepts.
        #[test]
        fn scores_are_confidences(
            pool in prop::collection::vec(arb_anchor(), 0..12),
            policy in arb_policy(),
        ) {
            let table = aggregate(&pool, policy);
            for (_, _, evidence) in table.rows() {
                prop_assert!((0.0..=1.0).contains(&evidence.score));
                for value in evidence.per_family {
                    prop_assert!((0.0..=1.0).contains(&value));
                }
                prop_assert!(evidence.top_explanations.len() <= 3);
            }
        }

        /// Selection under the strictest cardinality is a partial injection.
        #[test]
        fn strict_selection_is_a_partial_injection(
            pool in prop::collection::vec(arb_anchor(), 0..16),
        ) {
            let table = aggregate(&pool, AggregationPolicy::StrictPriority);
            let selection = table.select(Cardinality::Strict, RowFilter::relative_only());

            let mut sources: Vec<&str> = selection.pairs().iter().map(|p| p.src.as_str()).collect();
            let mut targets: Vec<&str> = selection.pairs().iter().map(|p| p.tgt.as_str()).collect();
            let selected = selection.len();
            sources.sort_unstable();
            sources.dedup();
            targets.sort_unstable();
            targets.dedup();

            prop_assert_eq!(sources.len(), selected);
            prop_assert_eq!(targets.len(), selected);
            prop_assert_eq!(selection.to_map().len(), selected);
        }
    }
}
