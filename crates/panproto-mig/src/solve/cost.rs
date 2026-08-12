//! The valuation structure the schema morphism search minimises over.
//!
//! # The structure
//!
//! Costs live in `S(k) = ⟨[0..k], ⊕, ⪰⟩` with `a ⊕ b = min(k, a + b)`, `⊥ = 0`
//! and `⊤ = k` (Larrosa and Schiex, *Artificial Intelligence* 159, Definition 3;
//! de Givry, Heras, Zytnicki and Larrosa, IJCAI 2005, §2). The orientation is
//! the one the valued CSP literature uses and it is the reverse of the
//! c-semiring convention: `⊥` is the best cost, `⊤` is infeasible, and `⪰`
//! reads "worse than or equal". [`Cost`]'s [`Ord`] is exactly "bigger is
//! worse", which fixes the orientation once so that no other module has to
//! re-derive it.
//!
//! The axioms [`Cost`] satisfies, each with a property test at the bottom of
//! this file:
//!
//! ```text
//! commutativity   a ⊕ b = b ⊕ a
//! associativity   a ⊕ (b ⊕ c) = (a ⊕ b) ⊕ c
//! identity        a ⊕ ⊥ = a
//! annihilator     a ⊕ ⊤ = ⊤
//! monotonicity    (a ⪰ b) ⇒ (a ⊕ c) ⪰ (b ⊕ c)
//! fairness        for w ⪯ v:  (v ⊖ w) ⊕ w = v
//!                             (v ⊖ w) ⪯ v
//!                             (u ⊕ w) ⊕ (v ⊖ w) = u ⊕ v
//! ```
//!
//! The last line is Lemma 7 of Cooper and Schiex (*Artificial Intelligence*
//! 154, 2004), and it is the single load-bearing identity of the whole solver:
//! every equivalence preserving transformation moves cost by subtracting
//! exactly what it adds, so every such proof is an instance of that identity.
//! An operation that satisfies it only approximately makes the optimiser
//! unsound rather than imprecise.
//!
//! # Why `top` is an argument and never a global
//!
//! `⊤` is the moving primal bound. Branch and bound lowers it at every
//! improving solution, so it is a property of the search state rather than of
//! the algebra, and two searches may run in one process at once. Reading a
//! stale `⊤` inside a saturating add produces a silently wrong answer that no
//! property test on [`Cost`] alone can detect, because every law in the list
//! above holds for whichever `⊤` is passed. Passing `top` explicitly on every
//! operation makes the staleness impossible to express.
//!
//! # Why integers rather than `f64`
//!
//! Six reasons, each independently sufficient.
//!
//! 1. Termination. The EDAC\* bound `O(ed²·max{nd, ⊤})` is finite only because
//!    every existential-arc-consistency driven increase of `c_∅` is at least
//!    one unit and `c_∅ ⪯ ⊤`. Under `f64` those increments can shrink
//!    geometrically and the enforcement loop has no termination proof.
//! 2. Equivalence preserving transformations are exact-difference machines.
//!    Under `f64`, `(x + α) − α ≠ x`, so pointwise preservation of the cost
//!    distribution degrades from an identity to an approximation.
//! 3. Drift makes costs negative, and one negative entry invalidates the lower
//!    bound: the solver then prunes the subtree holding the optimum and returns
//!    a suboptimal answer with no error signal.
//! 4. Comparisons against `⊤` are pruning decisions, so they must be exact
//!    integer equality rather than an equality-adjacent float comparison.
//! 5. The argmin is a comparison of sums. Two assignments differing by less
//!    than the accumulated error would be ordered arbitrarily, so "returns a
//!    true argmin" would not even be well posed.
//! 6. Reproducibility. Float sums depend on the order in which transformations
//!    are applied, which depends on heuristics and hash iteration order; the
//!    version control layer needs the same span for the same inputs on every
//!    machine.
//!
//! [`quality_units`] is therefore the one place in the crate where a float
//! becomes a cost, and it is called once per cost function entry while the
//! network is being built. Nothing sums in `f64` afterwards.
//!
//! # Why one packed `u64` rather than a two-field struct
//!
//! The objective is lexicographic: minimise the quality cost, then minimise the
//! number of dropped source vertices (equivalently, maximise the apex). Both
//! components live in one integer,
//!
//! ```text
//! Cost(q · radix + drops),    radix = (|V_s| + 1).next_power_of_two()
//! ```
//!
//! with `q ≤ COST_SCALE` and `drops ≤ |V_s| < radix`, so the `u64` [`Ord`] *is*
//! the lexicographic order on `(q, drops)`.
//!
//! **The no-carry precondition.** Componentwise addition is exact only while
//! the drop counts being summed stay below `radix`, and that is a property of
//! the cost function network rather than of [`Cost`]: each variable contributes
//! at most one [`DROP_UNIT`], equivalence preserving transformations move cost
//! without creating it, so no partial `⊕`-aggregation can exceed `|V_s|`.
//! [`Cost::combine`] takes no radix and therefore cannot check it; the
//! obligation belongs to whatever builds the network, and
//! `the_drop_field_bound_is_necessary_and_sufficient` at the bottom of this
//! file pins what happens on both sides of the boundary rather than staying
//! inside it. [`Cost::packed`] does check the bound, in every profile, at the
//! one point drop units enter the encoding.
//!
//! A two-field struct with a derived lexicographic [`Ord`] would be wrong, and
//! wrong in a way that breaks correctness rather than taste. The lexicographic
//! product of two ordered monoids is not fair: `(1, 5) ⪯ (2, 0)` holds, yet no
//! `γ` satisfies `(1, 5) ⊕ γ = (2, 0)` under componentwise addition, so the
//! maximum difference `⊖` required by Definition 6 of Cooper and Schiex does
//! not exist. Losing fairness loses Lemma 7, and losing Lemma 7 invalidates
//! every equivalence preserving transformation proof at once. A plain integer
//! is fair because subtraction is total on it.

/// Quality costs are stored in units of `10^-9`.
///
/// A perfectly matched morphism has quality cost `⊥`; the worst possible
/// quality cost is `COST_SCALE`, because the component weights are normalised
/// to sum to one and every denominator is fixed by the source schema.
pub const COST_SCALE: u64 = 1_000_000_000;

/// `COST_SCALE` as a float, for the one conversion in [`quality_units`].
///
/// Written as a literal rather than converted from [`COST_SCALE`] so that the
/// conversion is exact by inspection; `cost_scale_float_matches_cost_scale`
/// pins the two together.
const COST_SCALE_FLOAT: f64 = 1.0e9;

/// The cost of leaving one source vertex out of the apex.
///
/// One raw unit, which is one step in the low `log2(radix)` bits of the packed
/// encoding. Because it is strictly above `⊥`, any feasible extension of the
/// apex that does not raise the quality cost strictly lowers the total cost, so
/// a minimiser admits no cost-preserving extension.
pub const DROP_UNIT: Cost = Cost(1);

/// A cost in fixed point, with an explicit top.
///
/// The valuation structure is `S(k) = ⟨[0..k], ⊕, ≥⟩` with `a ⊕ b = min(k, a + b)`,
/// `⊥ = Cost(0)` and `⊤ = k` supplied per operation. Integer representation is
/// a correctness requirement rather than a performance choice; the module docs
/// give the six reasons.
///
/// The wrapped integer is the packed `(quality_cost, drop_count)` pair
/// described in the module docs, so `Ord` on `Cost` is the lexicographic order
/// on that pair.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Cost(u64);

impl Cost {
    /// `⊥`, the best cost, and the identity of `⊕`.
    pub const BOT: Self = Self(0);

    /// The sentinel used when no primal bound has been established.
    ///
    /// Exact inference passes this as `top` throughout: it never prunes, so it
    /// never needs a bound. Branch and bound replaces it with the incumbent's
    /// cost as soon as there is one.
    pub const TOP_SENTINEL: Self = Self(u64::MAX);

    /// Wrap a raw packed integer.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw packed integer.
    #[inline]
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// `⊕` : `a ⊕ b = min(k, a + b)`.
    ///
    /// Saturating at `u64::MAX` before the clamp makes overflow unreachable for
    /// every `top`, including [`Self::TOP_SENTINEL`].
    #[inline]
    #[must_use]
    pub const fn combine(self, other: Self, top: Self) -> Self {
        let sum = self.0.saturating_add(other.0);
        Self(if sum < top.0 { sum } else { top.0 })
    }

    /// `⊖` : `a ⊖ b = a − b` if `a ≠ k`, and `k` if `a = k`.
    ///
    /// This is the maximum difference of Definition 6 of Cooper and Schiex, so
    /// it is what makes the structure fair and Lemma 7 available.
    ///
    /// A cost at *or above* `top` is `⊤`, tested with `⪰` rather than with
    /// equality. `⊤` is the moving primal bound, so a cost recorded under an
    /// earlier, larger bound is semantically `⊤` under the current one, and
    /// Definition 2.3's `a = ⊤ ⇒ a ⊖ b = ⊤` clause is what makes `⊤`
    /// irreversible. Testing with equality would walk such a cost back below
    /// the bound.
    ///
    /// # Panics
    ///
    /// If `self ≺ other`, which violates the fairness precondition
    /// `other ⪯ self`. The check is unconditional rather than debug-only
    /// because the alternatives are both worse than a crash: wrapping returns
    /// garbage, and saturating at `⊥` *deletes* cost from a cost function,
    /// which costs `c_∅` its status as a lower bound and lets branch and bound
    /// prune the subtree holding the optimum with no error signal. One
    /// comparison on a value already in a register buys the exactness the
    /// whole soundness argument rests on.
    #[inline]
    #[must_use]
    pub const fn diff(self, other: Self, top: Self) -> Self {
        assert!(
            self.0 >= other.0,
            "cost difference precondition violated: self must not be below other"
        );
        if self.0 >= top.0 {
            top
        } else {
            Self(self.0 - other.0)
        }
    }

    /// Truncated difference, for the `E[b]` computation of `FindFullSupports`.
    ///
    /// `⊥` when `self ⪯ other`, and [`Self::diff`] otherwise.
    ///
    /// This is *not* [`Self::diff`] with the fairness precondition dropped, and
    /// the difference is worth stating because it falls inside the region where
    /// that precondition holds. At `self = other = ⊤`, Definition 2.3 gives `⊤`
    /// and this gives `⊥`. The deviation is deliberate and it is the safe
    /// direction: `E[b]` enters the projection step only as a quantity to move,
    /// so a value that is too small costs a propagation opportunity while
    /// preserving `E[b] ⪯ c_j(b)`, whereas `⊤` here would let a quality term
    /// declare an assignment infeasible, which no quality term may do.
    #[inline]
    #[must_use]
    pub const fn sat_diff(self, other: Self, top: Self) -> Self {
        if self.0 <= other.0 {
            Self::BOT
        } else {
            self.diff(other, top)
        }
    }

    /// Pack a quality cost and a drop count into one cost.
    ///
    /// `q` is a quality cost in units of `10^-9`, `drops` is a count of source
    /// vertices left out of the apex, and `radix` comes from
    /// [`coverage_radix`].
    ///
    /// The three preconditions — `q ⪯ COST_SCALE`, `radix` a power of two no
    /// greater than [`MAX_COVERAGE_RADIX`], and `drops < radix` — are checked
    /// in every profile rather than in debug alone. Violating either of the
    /// last two carries the drop count into the quality field, and the packed
    /// cost then reads *better* than the truth on the primary objective, so the
    /// optimiser returns a wrong argmin rather than merely a worse one. Three
    /// comparisons per cost function entry, paid while the network is being
    /// built and never in the search loop, buy that failure mode away.
    ///
    /// Together the three also make the arithmetic exact by construction:
    /// `q · radix + drops ⪯ 10^9 · 2^32 + 2^32`, which is under a quarter of
    /// `u64::MAX`. Nothing overflows, and no packed cost can reach `⊤` — the
    /// load-bearing consequence, since a quality term that reached `⊤` could
    /// declare an assignment infeasible and no quality term may do that.
    ///
    /// # Panics
    ///
    /// If any of the three preconditions is violated.
    #[inline]
    #[must_use]
    pub const fn packed(q: u64, drops: u32, radix: u64) -> Self {
        assert!(
            q <= COST_SCALE,
            "a quality cost must not exceed the cost scale"
        );
        assert!(
            radix.is_power_of_two() && radix <= MAX_COVERAGE_RADIX,
            "the coverage radix must be a power of two within the coverage range"
        );
        assert!(
            (drops as u64) < radix,
            "the drop count must be below the coverage radix"
        );
        Self(q * radix + drops as u64)
    }

    /// The quality cost held in the high bits, in units of `10^-9`.
    ///
    /// # Panics
    ///
    /// If `radix` is zero. Integer division by zero aborts in every profile;
    /// the debug assertion only improves the message.
    #[inline]
    #[must_use]
    pub const fn quality_part(self, radix: u64) -> u64 {
        debug_assert!(radix > 0, "the coverage radix must be positive");
        self.0 / radix
    }

    /// The number of dropped source vertices held in the low bits.
    ///
    /// Returned as a `u64` because it is read back out of a `u64` and no
    /// narrowing conversion can then be needed; compare it against
    /// `u64::from(drops)` for the value passed to [`Self::packed`].
    ///
    /// # Panics
    ///
    /// If `radix` is zero. Integer remainder by zero aborts in every profile;
    /// the debug assertion only improves the message.
    #[inline]
    #[must_use]
    pub const fn drop_part(self, radix: u64) -> u64 {
        debug_assert!(radix > 0, "the coverage radix must be positive");
        self.0 % radix
    }
}

/// The largest radix [`coverage_radix`] can return, attained at `u32::MAX`
/// source vertices.
///
/// It is the bound that makes [`Cost::packed`]'s arithmetic exact: with
/// `q ⪯ COST_SCALE` and `radix ⪯ MAX_COVERAGE_RADIX`, the packed value stays
/// under a quarter of `u64::MAX`.
pub const MAX_COVERAGE_RADIX: u64 = 1 << 32;

/// The radix separating the quality cost from the drop count.
///
/// `(|V_s| + 1).next_power_of_two()`, so that every drop count a search can
/// produce is strictly below it and componentwise addition of packed costs
/// never carries out of the drop field. A power of two makes
/// [`Cost::quality_part`] and [`Cost::drop_part`] a shift and a mask.
///
/// The parameter is a `u32` because [`VarId`](super::VarId) is, so one variable
/// per source vertex bounds the count at `u32::MAX`. That bound is what makes
/// the function total: the widened `u32::MAX + 1` is exactly `2^32`, so the
/// addition cannot overflow and `next_power_of_two` cannot either. A `u64`
/// parameter would make it partial, panicking above `2^63 - 1` in debug and
/// wrapping to zero in release, which would then divide by zero in
/// [`Cost::quality_part`].
///
/// The radix is a property of one search and is carried on the network rather
/// than being global, because two networks over differently sized sources have
/// different radices and their costs are not comparable.
#[inline]
#[must_use]
pub const fn coverage_radix(source_vertex_count: u32) -> u64 {
    (source_vertex_count as u64 + 1).next_power_of_two()
}

/// Convert a quality fraction in `[0, 1]` to quality cost units.
///
/// This is the only place in the crate where a float becomes a cost. It runs
/// once per cost function entry while the network is being built, on inputs
/// (edit distance ratios, Jaccard coefficients, degree ratios, evidence) that
/// are themselves deterministic; nothing sums in `f64` afterwards.
///
/// Rounding is per term and is [`f64::round`]'s, which breaks ties *away from
/// zero* rather than to even. Naming the mode matters here because the float to
/// integer boundary is the one place in the crate where precision is decided:
/// half a unit is the worst error a single term can carry, which is what makes
/// `(|V_s| + |E_s|) / (2 · COST_SCALE)` the tight bound on the difference
/// between a quality read back out of the integer objective and a float
/// accumulation of the same terms, rather than a guess.
///
/// A non-number maps to [`COST_SCALE`], the worst finite quality cost. It never
/// maps to `⊤`: a cost function that could reach `⊤` would be able to declare
/// an assignment infeasible, and no quality term is allowed to do that.
#[inline]
#[must_use]
pub fn quality_units(x: f64) -> u64 {
    if x.is_nan() {
        return COST_SCALE;
    }
    // Scaling against a float literal rather than `COST_SCALE as f64` keeps the
    // widening cast, and its precision-loss question, out of the function.
    let scaled = x.clamp(0.0, 1.0) * COST_SCALE_FLOAT;
    // After the clamp `scaled` lies in `[0.0, 1e9]`, so the rounded value is a
    // non-negative integer well below `2^53`: it cannot truncate and it cannot
    // lose a sign. The narrowing conversion is scoped to this one binding, and
    // it is the only float to integer conversion in the crate.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let units = scaled.round() as u64;
    units
}

/// The relative weight of each component of the objective.
///
/// Validated at construction to be finite, non-negative and not all zero, then
/// normalised to sum to one so that the total quality cost lies in
/// `[0, COST_SCALE]`. The fields are private for exactly that reason: a weight
/// vector that skipped the check could produce a negative cost, and a negative
/// cost invalidates the lower bound.
///
/// Every weight here is a principled default rather than a calibrated value.
/// None of them has been fitted to data.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CostWeights {
    name: f64,
    edge: f64,
    prop: f64,
    degree: f64,
    anchor: f64,
}

/// The default component weights.
///
/// The first four are the weights the quality score has always used, kept
/// unchanged so that any behaviour change is attributable to the structural
/// rewrite rather than to a reweighting. `anchor` is zero, which makes anchor
/// evidence observably neutral: it enters the search as a reward-only unary
/// term that is currently scaled to nothing.
pub const DEFAULT_WEIGHTS: CostWeights = CostWeights {
    name: 0.25,
    edge: 0.25,
    prop: 0.30,
    degree: 0.20,
    anchor: 0.0,
};

impl Default for CostWeights {
    fn default() -> Self {
        DEFAULT_WEIGHTS
    }
}

impl CostWeights {
    /// Check and normalise a weight vector.
    ///
    /// The five weights are divided by their sum, so only their ratios matter.
    ///
    /// # Errors
    ///
    /// Returns [`CostWeightsError::NotFinite`] if any weight is a non-number or
    /// an infinity, [`CostWeightsError::Negative`] if any weight is below zero,
    /// [`CostWeightsError::AllZero`] if every weight is zero, which would leave
    /// the objective constant and the argmin undefined, and
    /// [`CostWeightsError::SumNotFinite`] if five individually finite weights
    /// sum to an infinity. That last case is the one the range checks alone
    /// miss: `inf ⩽ 0.0` is false, so an overflowing sum passes the all-zero
    /// guard, and then every `w / inf` is `0.0` — the exact state the guard
    /// exists to prevent, reached with a clean `Ok`.
    ///
    /// # Panics
    ///
    /// In debug builds, if the normalised weights do not sum to one. This is
    /// the post-condition the whole type exists to establish, and it is
    /// checked where it is established rather than only in a test.
    pub fn new(
        name: f64,
        edge: f64,
        prop: f64,
        degree: f64,
        anchor: f64,
    ) -> Result<Self, CostWeightsError> {
        let components = [
            ("name", name),
            ("edge", edge),
            ("prop", prop),
            ("degree", degree),
            ("anchor", anchor),
        ];
        for (component, value) in components {
            if !value.is_finite() {
                return Err(CostWeightsError::NotFinite { component, value });
            }
            if value < 0.0 {
                return Err(CostWeightsError::Negative { component, value });
            }
        }
        let total = name + edge + prop + degree + anchor;
        // Every component is finite and non-negative, so the sum is zero
        // exactly when all five are.
        if total <= 0.0 {
            return Err(CostWeightsError::AllZero);
        }
        // Five finite components can still sum past `f64::MAX`. The check above
        // does not catch it, because `inf <= 0.0` is false.
        if !total.is_finite() {
            return Err(CostWeightsError::SumNotFinite { total });
        }
        let weights = Self {
            name: name / total,
            edge: edge / total,
            prop: prop / total,
            degree: degree / total,
            anchor: anchor / total,
        };
        debug_assert!(
            (weights.name + weights.edge + weights.prop + weights.degree + weights.anchor - 1.0)
                .abs()
                < 1e-9,
            "normalised weights must sum to one"
        );
        Ok(weights)
    }

    /// The weight on vertex name similarity.
    #[inline]
    #[must_use]
    pub const fn name(self) -> f64 {
        self.name
    }

    /// The weight on edge naturality and edge name agreement.
    #[inline]
    #[must_use]
    pub const fn edge(self) -> f64 {
        self.edge
    }

    /// The weight on the Jaccard overlap of outgoing edge names.
    #[inline]
    #[must_use]
    pub const fn prop(self) -> f64 {
        self.prop
    }

    /// The weight on out-degree agreement.
    #[inline]
    #[must_use]
    pub const fn degree(self) -> f64 {
        self.degree
    }

    /// The weight on anchor evidence.
    ///
    /// Anchor evidence is reward only: it lowers the cost of an assignment and
    /// can never raise it to `⊤`, so it steers the search without changing
    /// which assignments are feasible.
    #[inline]
    #[must_use]
    pub const fn anchor(self) -> f64 {
        self.anchor
    }

    /// The four reported components, in `[name, edge, prop, degree]` order.
    ///
    /// This is the order the quality score has always used. `anchor` is absent
    /// because it steers the search without contributing to the reported
    /// quality.
    ///
    /// **These four sum to `1 − anchor`, not to one.** The normalisation
    /// [`Self::new`] performs is over all five components, so a consumer that
    /// reconstructs a quality score as `Σ wᵢ · sᵢ` from this array and reports
    /// `1 − q` gets a number biased low by exactly the anchor weight. With
    /// [`DEFAULT_WEIGHTS`] the anchor weight is zero and the two agree; they
    /// stop agreeing the moment anyone sets it.
    #[inline]
    #[must_use]
    pub const fn as_array(self) -> [f64; 4] {
        [self.name, self.edge, self.prop, self.degree]
    }
}

/// Why a weight vector was rejected.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum CostWeightsError {
    /// A weight was a non-number or an infinity.
    #[error("weight `{component}` is not finite: {value}")]
    NotFinite {
        /// Which of the five weights.
        component: &'static str,
        /// The value supplied.
        value: f64,
    },

    /// A weight was below zero, which would let a cost function fall below `⊥`.
    #[error("weight `{component}` is negative: {value}")]
    Negative {
        /// Which of the five weights.
        component: &'static str,
        /// The value supplied.
        value: f64,
    },

    /// Every weight was zero, which leaves the objective constant.
    #[error("all five weights are zero, so the objective has no argmin")]
    AllZero,

    /// Five finite weights summed to an infinity, so normalising by that sum
    /// would zero every component.
    #[error("the five weights sum to {total}, which is not finite")]
    SumNotFinite {
        /// The overflowing sum.
        total: f64,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    const TOP: Cost = Cost::TOP_SENTINEL;

    #[test]
    fn bot_is_zero_and_the_sentinel_is_the_largest_representable_cost() {
        assert_eq!(Cost::BOT.raw(), 0);
        assert_eq!(Cost::TOP_SENTINEL.raw(), u64::MAX);
        assert_eq!(Cost::default(), Cost::BOT);
        assert!(Cost::BOT < Cost::TOP_SENTINEL);
    }

    #[test]
    fn cost_scale_float_matches_cost_scale() {
        assert!((COST_SCALE_FLOAT - 1_000_000_000.0).abs() < f64::EPSILON);
        assert_eq!(COST_SCALE, 1_000_000_000);
    }

    #[test]
    fn combine_clamps_to_top() {
        let top = Cost::from_raw(10);
        assert_eq!(Cost::from_raw(4).combine(Cost::from_raw(3), top).raw(), 7);
        assert_eq!(Cost::from_raw(9).combine(Cost::from_raw(3), top), top);
        assert_eq!(top.combine(Cost::BOT, top), top);
    }

    #[test]
    fn combine_never_wraps_at_the_largest_representable_cost() {
        let huge = Cost::from_raw(u64::MAX);
        assert_eq!(huge.combine(huge, TOP), TOP);
        assert_eq!(huge.combine(Cost::from_raw(1), TOP), TOP);
        assert_eq!(
            Cost::from_raw(u64::MAX - 1).combine(Cost::from_raw(1), TOP),
            TOP
        );
        // A near-overflow that stays below a finite top still saturates there.
        let top = Cost::from_raw(u64::MAX / 2);
        assert_eq!(top.combine(top, top), top);
    }

    #[test]
    fn diff_is_exact_below_top() {
        let top = Cost::from_raw(100);
        assert_eq!(Cost::from_raw(7).diff(Cost::from_raw(3), top).raw(), 4);
        assert_eq!(Cost::from_raw(7).diff(Cost::from_raw(7), top), Cost::BOT);
        assert_eq!(Cost::from_raw(7).diff(Cost::BOT, top).raw(), 7);
    }

    #[test]
    fn diff_of_top_is_top() {
        let top = Cost::from_raw(100);
        assert_eq!(top.diff(Cost::from_raw(30), top), top);
        assert_eq!(top.diff(top, top), top);
    }

    #[test]
    #[should_panic(expected = "cost difference precondition violated")]
    fn diff_panics_in_every_profile_when_the_precondition_is_violated() {
        let _ = Cost::from_raw(1).diff(Cost::from_raw(2), TOP);
    }

    /// A cost recorded under an earlier, larger `⊤` is `⊤` under the current
    /// one. Recognising it with equality rather than `⪰` would let `diff` walk
    /// it back below the bound, which is the one thing `⊤`'s irreversibility
    /// forbids.
    #[test]
    fn a_cost_at_or_above_top_is_top() {
        let lowered = Cost::from_raw(100);
        let stale = Cost::from_raw(450);
        assert_eq!(stale.diff(Cost::from_raw(400), lowered), lowered);
        assert_eq!(lowered.diff(Cost::from_raw(1), lowered), lowered);
    }

    #[test]
    fn sat_diff_truncates_at_bot() {
        let top = Cost::from_raw(100);
        assert_eq!(
            Cost::from_raw(3).sat_diff(Cost::from_raw(7), top),
            Cost::BOT
        );
        assert_eq!(
            Cost::from_raw(7).sat_diff(Cost::from_raw(7), top),
            Cost::BOT
        );
        assert_eq!(Cost::from_raw(7).sat_diff(Cost::from_raw(3), top).raw(), 4);
        // The one place `sat_diff` and `diff` disagree: the precondition
        // `self ⪯ other` wins over the `self = ⊤` clause.
        assert_eq!(top.sat_diff(top, top), Cost::BOT);
        assert_eq!(top.diff(top, top), top);
    }

    #[test]
    fn coverage_radix_is_a_power_of_two_above_the_vertex_count() {
        assert_eq!(coverage_radix(0), 1);
        assert_eq!(coverage_radix(1), 2);
        assert_eq!(coverage_radix(39), 64);
        assert_eq!(coverage_radix(63), 64);
        assert_eq!(coverage_radix(64), 128);
        for n in 0u32..200 {
            let radix = coverage_radix(n);
            assert!(radix.is_power_of_two());
            assert!(
                u64::from(n) < radix,
                "{n} must be representable as a drop count"
            );
        }
    }

    /// The function is total on its whole domain. A `u64` parameter made it
    /// partial: `u64::MAX` panicked in debug and wrapped to zero in release,
    /// and a zero radix then divides by zero in `quality_part`.
    #[test]
    fn coverage_radix_is_total() {
        let widest = coverage_radix(u32::MAX);
        assert_eq!(widest, MAX_COVERAGE_RADIX);
        assert!(widest.is_power_of_two());
        assert!(u64::from(u32::MAX) < widest);
    }

    /// No quality term may reach `⊤`, because reaching `⊤` is a claim of
    /// infeasibility and feasibility is not a quality question. The extreme of
    /// the packed encoding's domain is the case to check.
    #[test]
    fn a_packed_cost_never_reaches_top() {
        let widest = coverage_radix(u32::MAX);
        let worst = Cost::packed(COST_SCALE, u32::MAX - 1, widest);
        assert!(worst < Cost::TOP_SENTINEL);
        assert!(worst.raw() < u64::MAX / 4, "raw {}", worst.raw());
        assert_eq!(worst.quality_part(widest), COST_SCALE);
        assert_eq!(worst.drop_part(widest), u64::from(u32::MAX - 1));
        // And at the radix a real search uses.
        assert!(Cost::packed(COST_SCALE, 63, 64) < Cost::TOP_SENTINEL);
    }

    #[test]
    #[should_panic(expected = "a quality cost must not exceed the cost scale")]
    fn packed_rejects_a_quality_above_the_cost_scale() {
        let _ = Cost::packed(COST_SCALE + 1, 0, 64);
    }

    #[test]
    #[should_panic(expected = "the coverage radix must be a power of two")]
    fn packed_rejects_a_radix_that_is_not_a_power_of_two() {
        let _ = Cost::packed(3, 5, 7);
    }

    #[test]
    #[should_panic(expected = "the coverage radix must be a power of two")]
    fn packed_rejects_a_radix_above_the_coverage_range() {
        let _ = Cost::packed(3, 5, MAX_COVERAGE_RADIX * 2);
    }

    /// A drop count at or above the radix carries into the quality field, and
    /// the resulting cost reads *better* on the primary objective than the
    /// truth. Rejecting it in every profile is what keeps that out of release.
    #[test]
    #[should_panic(expected = "the drop count must be below the coverage radix")]
    fn packed_rejects_a_drop_count_at_the_radix() {
        let _ = Cost::packed(3, 9, 4);
    }

    #[test]
    fn packed_round_trips() {
        let radix = coverage_radix(39);
        let cost = Cost::packed(123_456_789, 17, radix);
        assert_eq!(cost.quality_part(radix), 123_456_789);
        assert_eq!(cost.drop_part(radix), 17);
        assert_eq!(cost.raw(), 123_456_789 * 64 + 17);
    }

    #[test]
    fn packed_ord_is_lexicographic() {
        let radix = coverage_radix(39);
        // Quality dominates: a worse quality loses however few vertices drop.
        assert!(Cost::packed(1, 0, radix) > Cost::packed(0, 63, radix));
        // Within one quality, fewer drops wins.
        assert!(Cost::packed(5, 1, radix) < Cost::packed(5, 2, radix));
        assert_eq!(Cost::packed(5, 2, radix), Cost::packed(5, 2, radix));
    }

    #[test]
    fn drop_counts_never_carry_into_the_quality_field() {
        // Summing one drop per source vertex, at the largest source the radix
        // was sized for, must leave the quality field untouched.
        let vertices = 39u32;
        let radix = coverage_radix(vertices);
        let mut total = Cost::packed(7, 0, radix);
        for _ in 0..vertices {
            total = total.combine(DROP_UNIT, TOP);
        }
        assert_eq!(total.quality_part(radix), 7);
        assert_eq!(total.drop_part(radix), u64::from(vertices));
    }

    #[test]
    fn packed_addition_is_componentwise() {
        let radix = coverage_radix(39);
        let a = Cost::packed(400_000_000, 3, radix);
        let b = Cost::packed(250_000_000, 4, radix);
        assert_eq!(a.combine(b, TOP), Cost::packed(650_000_000, 7, radix));
    }

    #[test]
    fn quality_units_clamps_and_rounds() {
        assert_eq!(quality_units(0.0), 0);
        assert_eq!(quality_units(1.0), COST_SCALE);
        assert_eq!(quality_units(0.5), 500_000_000);
        assert_eq!(quality_units(-1.0), 0);
        assert_eq!(quality_units(2.0), COST_SCALE);
        assert_eq!(quality_units(f64::INFINITY), COST_SCALE);
        assert_eq!(quality_units(f64::NEG_INFINITY), 0);
        assert_eq!(quality_units(f64::NAN), COST_SCALE);
        // Rounding is to nearest, not truncation: the exact product is
        // 123_456_789.6 quality units.
        assert_eq!(quality_units(0.123_456_789_6), 123_456_790);
        assert_eq!(quality_units(0.123_456_789_1), 123_456_789);
    }

    #[test]
    fn default_weights_are_already_normalised() {
        let normalised = CostWeights::new(0.25, 0.25, 0.30, 0.20, 0.0).unwrap();
        assert!((normalised.name() - DEFAULT_WEIGHTS.name()).abs() < 1e-15);
        assert!((normalised.edge() - DEFAULT_WEIGHTS.edge()).abs() < 1e-15);
        assert!((normalised.prop() - DEFAULT_WEIGHTS.prop()).abs() < 1e-15);
        assert!((normalised.degree() - DEFAULT_WEIGHTS.degree()).abs() < 1e-15);
        assert!((normalised.anchor() - DEFAULT_WEIGHTS.anchor()).abs() < 1e-15);
        assert_eq!(CostWeights::default(), DEFAULT_WEIGHTS);
        assert_eq!(DEFAULT_WEIGHTS.as_array(), [0.25, 0.25, 0.30, 0.20]);
        assert_eq!(DEFAULT_WEIGHTS.anchor(), 0.0);
    }

    #[test]
    fn weights_reject_negative() {
        let err = CostWeights::new(0.25, -0.1, 0.30, 0.20, 0.0).unwrap_err();
        assert_eq!(
            err,
            CostWeightsError::Negative {
                component: "edge",
                value: -0.1
            }
        );
    }

    #[test]
    fn weights_reject_non_finite() {
        // Matched rather than compared: the reported value is itself a
        // non-number, and a non-number is not equal to itself.
        assert!(matches!(
            CostWeights::new(f64::NAN, 0.25, 0.30, 0.20, 0.0).unwrap_err(),
            CostWeightsError::NotFinite {
                component: "name",
                value
            } if value.is_nan()
        ));
        assert!(matches!(
            CostWeights::new(0.25, 0.25, f64::INFINITY, 0.20, 0.0).unwrap_err(),
            CostWeightsError::NotFinite {
                component: "prop",
                ..
            }
        ));
        assert!(matches!(
            CostWeights::new(0.25, 0.25, 0.30, 0.20, f64::NEG_INFINITY).unwrap_err(),
            CostWeightsError::NotFinite {
                component: "anchor",
                ..
            }
        ));
    }

    #[test]
    fn weights_reject_all_zero() {
        assert_eq!(
            CostWeights::new(0.0, 0.0, 0.0, 0.0, 0.0).unwrap_err(),
            CostWeightsError::AllZero
        );
    }

    /// Five finite weights can sum past `f64::MAX`. The all-zero guard does not
    /// catch it, because `inf <= 0.0` is false, and dividing by `inf` then
    /// leaves every component at zero: the exact state the guard exists to
    /// prevent, arrived at through `Ok`.
    #[test]
    fn weights_reject_a_sum_that_overflows() {
        assert!(matches!(
            CostWeights::new(1e308, 1e308, 1e308, 1e308, 1e308).unwrap_err(),
            CostWeightsError::SumNotFinite { total } if total.is_infinite()
        ));
        // Two are enough.
        assert!(matches!(
            CostWeights::new(f64::MAX, f64::MAX, 0.0, 0.0, 0.0).unwrap_err(),
            CostWeightsError::SumNotFinite { .. }
        ));
    }

    #[test]
    fn weights_normalise_ratios() {
        let weights = CostWeights::new(1.0, 1.0, 1.0, 1.0, 0.0).unwrap();
        assert!((weights.name() - 0.25).abs() < 1e-15);
        assert!((weights.anchor() - 0.0).abs() < 1e-15);
        let scaled = CostWeights::new(10.0, 10.0, 12.0, 8.0, 0.0).unwrap();
        assert!((scaled.prop() - 0.30).abs() < 1e-15);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod property {
    use super::*;
    use proptest::prelude::*;

    /// Tops spanning the degenerate structure, the range real searches use, and
    /// the sentinel.
    fn arb_top() -> impl Strategy<Value = u64> {
        prop_oneof![
            1 => Just(0u64),
            1 => Just(u64::MAX),
            3 => 1u64..=1_000_000u64,
            3 => 1u64..=u64::MAX,
        ]
    }

    /// `(top, a, b, c)` with `a, b, c ⪯ top`, which is the domain every axiom
    /// of `S(k)` is stated over.
    fn arb_valuations() -> impl Strategy<Value = (Cost, Cost, Cost, Cost)> {
        arb_top()
            .prop_flat_map(|k| (Just(k), 0..=k, 0..=k, 0..=k))
            .prop_map(|(k, a, b, c)| {
                (
                    Cost::from_raw(k),
                    Cost::from_raw(a),
                    Cost::from_raw(b),
                    Cost::from_raw(c),
                )
            })
    }

    /// `(top, u, v, w)` with `u, v ⪯ top` and `w ⪯ v`, the precondition of the
    /// three fairness statements.
    fn arb_fair_valuations() -> impl Strategy<Value = (Cost, Cost, Cost, Cost)> {
        arb_top()
            .prop_flat_map(|k| (Just(k), 0..=k, 0..=k))
            .prop_flat_map(|(k, u, v)| (Just(k), Just(u), Just(v), 0..=v))
            .prop_map(|(k, u, v, w)| {
                (
                    Cost::from_raw(k),
                    Cost::from_raw(u),
                    Cost::from_raw(v),
                    Cost::from_raw(w),
                )
            })
    }

    /// Raw values clustered where a wrapping bug would show: around half of the
    /// range, at the very top of it, at the very bottom, and uniformly.
    fn arb_boundary_raw() -> impl Strategy<Value = u64> {
        prop_oneof![
            2 => (u64::MAX / 2 - 1024)..=(u64::MAX / 2 + 1024),
            2 => (u64::MAX - 1024)..=u64::MAX,
            1 => 0u64..=1024,
            1 => any::<u64>(),
        ]
    }

    /// `(top, a, b)` with `a, b ⪯ top`, all three near a boundary.
    fn arb_boundary_case() -> impl Strategy<Value = (Cost, Cost, Cost)> {
        (arb_boundary_raw(), arb_boundary_raw(), arb_boundary_raw()).prop_map(|(t, a, b)| {
            let top = t.max(a).max(b);
            (Cost::from_raw(top), Cost::from_raw(a), Cost::from_raw(b))
        })
    }

    fn arb_radix() -> impl Strategy<Value = u64> {
        (0u32..=63u32).prop_map(coverage_radix)
    }

    fn max_drops(radix: u64) -> u32 {
        u32::try_from(radix - 1).unwrap_or(u32::MAX)
    }

    /// `(radix, q1, drops1, q2, drops2)` with both drop counts below the radix.
    fn arb_packed_pair() -> impl Strategy<Value = (u64, u64, u32, u64, u32)> {
        arb_radix().prop_flat_map(|radix| {
            let top_drops = max_drops(radix);
            (
                Just(radix),
                0..=COST_SCALE,
                0..=top_drops,
                0..=COST_SCALE,
                0..=top_drops,
            )
        })
    }

    /// The same shape, with the two drop counts constrained to sum below the
    /// radix, which is the regime a real search stays inside.
    fn arb_packed_addends() -> impl Strategy<Value = (u64, u64, u32, u64, u32)> {
        arb_radix()
            .prop_flat_map(|radix| {
                let top_drops = max_drops(radix);
                (
                    Just(radix),
                    0..=COST_SCALE / 2,
                    0..=COST_SCALE / 2,
                    0..=top_drops,
                )
            })
            .prop_flat_map(|(radix, q1, q2, drops1)| {
                let remaining = max_drops(radix) - drops1;
                (Just(radix), Just(q1), Just(drops1), Just(q2), 0..=remaining)
            })
    }

    fn arb_weight() -> impl Strategy<Value = f64> {
        prop_oneof![
            1 => Just(0.0f64),
            4 => 0.0f64..=1000.0f64,
            // Large enough that five of them sum past `f64::MAX`, which is the
            // regime the all-zero guard alone does not cover.
            1 => 1e300f64..=f64::MAX,
        ]
    }

    /// `(top, u, v, w)` with `w ⪯ v`, every component drawn from the boundary
    /// clusters rather than uniformly. This is the domain Lemma 1.11 is stated
    /// over, sampled where a saturating `⊕` is most likely to break it.
    fn arb_boundary_fair_valuations() -> impl Strategy<Value = (Cost, Cost, Cost, Cost)> {
        (
            arb_boundary_raw(),
            arb_boundary_raw(),
            arb_boundary_raw(),
            arb_boundary_raw(),
        )
            .prop_map(|(t, u, v, w)| {
                let top = t.max(u).max(v);
                (
                    Cost::from_raw(top),
                    Cost::from_raw(u),
                    Cost::from_raw(v),
                    Cost::from_raw(w.min(v)),
                )
            })
    }

    /// A vector of `(lower, upper)` cost pairs with `upper ⪰ lower` throughout,
    /// for the n-fold form of monotonicity.
    fn arb_monotone_vector() -> impl Strategy<Value = (Cost, Vec<(Cost, Cost)>)> {
        arb_top().prop_flat_map(|k| {
            (
                Just(Cost::from_raw(k)),
                prop::collection::vec(
                    (0..=k, 0..=k).prop_map(|(a, b)| {
                        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                        (Cost::from_raw(lo), Cost::from_raw(hi))
                    }),
                    1..=12,
                ),
            )
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn combine_is_commutative((top, a, b, _c) in arb_valuations()) {
            prop_assert_eq!(a.combine(b, top), b.combine(a, top));
        }

        #[test]
        fn combine_is_associative((top, a, b, c) in arb_valuations()) {
            prop_assert_eq!(
                a.combine(b.combine(c, top), top),
                a.combine(b, top).combine(c, top)
            );
        }

        #[test]
        fn bot_is_the_identity((top, a, _b, _c) in arb_valuations()) {
            prop_assert_eq!(a.combine(Cost::BOT, top), a);
            prop_assert_eq!(Cost::BOT.combine(a, top), a);
        }

        #[test]
        fn top_is_the_annihilator((top, a, _b, _c) in arb_valuations()) {
            prop_assert_eq!(a.combine(top, top), top);
            prop_assert_eq!(top.combine(a, top), top);
        }

        #[test]
        fn combine_is_monotone((top, a, b, c) in arb_valuations()) {
            let (worse, better) = if a >= b { (a, b) } else { (b, a) };
            prop_assert!(worse.combine(c, top) >= better.combine(c, top));
        }

        #[test]
        fn the_cost_order_is_total((_top, a, b, _c) in arb_valuations()) {
            // Totality is exactly the absence of incomparable pairs, so the
            // partial order must never decline to answer.
            let ordering = a.cmp(&b);
            prop_assert_eq!(a.partial_cmp(&b), Some(ordering));
            prop_assert_eq!(b.cmp(&a), ordering.reverse());
            prop_assert_eq!(ordering.is_eq(), a == b);
        }

        #[test]
        fn fairness_difference_recombines((top, _u, v, w) in arb_fair_valuations()) {
            prop_assert_eq!(v.diff(w, top).combine(w, top), v);
        }

        #[test]
        fn fairness_difference_never_worsens((top, _u, v, w) in arb_fair_valuations()) {
            prop_assert!(v.diff(w, top) <= v);
        }

        #[test]
        fn fairness_moves_cost_without_changing_the_sum(
            (top, u, v, w) in arb_fair_valuations()
        ) {
            prop_assert_eq!(
                u.combine(w, top).combine(v.diff(w, top), top),
                u.combine(v, top)
            );
        }

        #[test]
        fn sat_diff_agrees_with_diff_above_the_precondition(
            (top, _u, v, w) in arb_fair_valuations()
        ) {
            if v > w {
                prop_assert_eq!(v.sat_diff(w, top), v.diff(w, top));
            } else {
                prop_assert_eq!(v.sat_diff(w, top), Cost::BOT);
            }
        }

        #[test]
        fn packed_encoding_round_trips((radix, q, drops, _q2, _d2) in arb_packed_pair()) {
            let cost = Cost::packed(q, drops, radix);
            prop_assert_eq!(cost.quality_part(radix), q);
            prop_assert_eq!(cost.drop_part(radix), u64::from(drops));
        }

        #[test]
        fn packed_ord_is_lexicographic_on_quality_then_drops(
            (radix, q1, drops1, q2, drops2) in arb_packed_pair()
        ) {
            prop_assert_eq!(
                Cost::packed(q1, drops1, radix).cmp(&Cost::packed(q2, drops2, radix)),
                (q1, drops1).cmp(&(q2, drops2))
            );
        }

        #[test]
        fn packed_addition_is_componentwise(
            (radix, q1, drops1, q2, drops2) in arb_packed_addends()
        ) {
            prop_assert_eq!(
                Cost::packed(q1, drops1, radix)
                    .combine(Cost::packed(q2, drops2, radix), Cost::TOP_SENTINEL),
                Cost::packed(q1 + q2, drops1 + drops2, radix)
            );
        }

        /// The function's whole job is to be total over `f64`, so the draw is
        /// over `f64` rather than over a plausible interval: subnormals,
        /// infinities and every non-number payload included.
        #[test]
        fn quality_units_stays_in_range(x in any::<f64>()) {
            prop_assert!(quality_units(x) <= COST_SCALE);
        }

        /// The `⊕`-aggregation of `cost(x)` is termwise, so the form
        /// monotonicity is used in is the n-fold one. It follows from the
        /// one-argument axiom plus associativity, and it is the composition a
        /// saturating `⊕` is most likely to break.
        #[test]
        fn combine_is_monotone_termwise((top, pairs) in arb_monotone_vector()) {
            let fold = |pick: fn(&(Cost, Cost)) -> Cost| {
                pairs.iter().fold(Cost::BOT, |acc, pair| acc.combine(pick(pair), top))
            };
            prop_assert!(fold(|p| p.1) >= fold(|p| p.0));
        }

        /// Lemma 1.11 over boundary-clustered inputs. The uniform draw in
        /// `arb_fair_valuations` never reaches the range where a saturating
        /// `⊕` could break the identity the solver's every transformation
        /// proof is an instance of.
        #[test]
        fn fairness_holds_at_the_boundaries(
            (top, u, v, w) in arb_boundary_fair_valuations()
        ) {
            prop_assert_eq!(
                u.combine(w, top).combine(v.diff(w, top), top),
                u.combine(v, top)
            );
            prop_assert_eq!(v.diff(w, top).combine(w, top), v);
            prop_assert!(v.diff(w, top) <= v);
        }

        #[test]
        fn quality_units_is_monotone(x in 0.0f64..=1.0f64, y in 0.0f64..=1.0f64) {
            let (lo, hi) = if x <= y { (x, y) } else { (y, x) };
            prop_assert!(quality_units(lo) <= quality_units(hi));
        }

        #[test]
        fn weights_normalise_or_are_rejected(
            name in arb_weight(),
            edge in arb_weight(),
            prop_w in arb_weight(),
            degree in arb_weight(),
            anchor in arb_weight(),
        ) {
            let total = name + edge + prop_w + degree + anchor;
            match CostWeights::new(name, edge, prop_w, degree, anchor) {
                Ok(weights) => {
                    prop_assert!(total > 0.0);
                    prop_assert!(total.is_finite());
                    let sum = weights.name()
                        + weights.edge()
                        + weights.prop()
                        + weights.degree()
                        + weights.anchor();
                    prop_assert!((sum - 1.0).abs() < 1e-9, "weights sum to {}", sum);
                    for value in [
                        weights.name(),
                        weights.edge(),
                        weights.prop(),
                        weights.degree(),
                        weights.anchor(),
                    ] {
                        prop_assert!(value >= 0.0);
                        prop_assert!(value.is_finite());
                    }
                }
                Err(CostWeightsError::AllZero) => prop_assert!(total <= 0.0),
                Err(CostWeightsError::SumNotFinite { .. }) => {
                    prop_assert!(!total.is_finite());
                }
                Err(other) => prop_assert!(false, "unexpected rejection: {other}"),
            }
        }

        /// The drop field carries into the quality field as soon as the drop
        /// count reaches the radix, so the bound `drops < radix` is a
        /// precondition of the encoding rather than a comment about it. The
        /// property is the necessity of that bound: crossing it is rejected,
        /// and staying inside it is exact.
        #[test]
        fn the_drop_field_bound_is_necessary_and_sufficient(
            (radix, q, drops1, _q2, drops2) in arb_packed_pair()
        ) {
            let total = u64::from(drops1) + u64::from(drops2);
            if total < radix {
                let packed = Cost::packed(q, drops1, radix)
                    .combine(Cost::packed(0, drops2, radix), Cost::TOP_SENTINEL);
                prop_assert_eq!(packed.quality_part(radix), q);
                prop_assert_eq!(packed.drop_part(radix), total);
            } else {
                // Past the bound the sum is no longer a packed cost at all:
                // its drop field wrapped and its quality field gained a unit.
                let raw = Cost::packed(q, drops1, radix).raw()
                    + Cost::packed(0, drops2, radix).raw();
                prop_assert_eq!(Cost::from_raw(raw).quality_part(radix), q + total / radix);
                prop_assert!(q + total / radix > q);
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn cost_never_wraps((top, a, b) in arb_boundary_case()) {
            let combined = a.combine(b, top);
            prop_assert!(combined <= top);
            prop_assert!(combined >= a);
            prop_assert!(combined >= b);
            if let Some(sum) = a.raw().checked_add(b.raw()) {
                prop_assert_eq!(combined.raw(), sum.min(top.raw()));
            } else {
                prop_assert_eq!(combined, top);
            }
            // The difference is exact in the other direction too.
            let (worse, better) = if a >= b { (a, b) } else { (b, a) };
            prop_assert!(worse.diff(better, top) <= worse);
            prop_assert_eq!(worse.diff(better, top).combine(better, top), worse);
        }
    }
}
