//! The schema morphism search, posed as a cost function network.
//!
//! Finding a schema morphism is a valued constraint satisfaction problem: one
//! variable per source vertex, one value per kind-compatible target vertex plus
//! a distinguished `⊥` meaning "this vertex is not in the apex", hard
//! `⊤`-valued cost functions for kind compatibility and naturality, and soft
//! cost functions for the quality of a match. Minimising the total cost over
//! that network is the search.
//!
//! `⊥` is what makes the result a span rather than a total morphism. The apex
//! is `{ v : x_v ≠ ⊥ }` with the induced edge set, so a partial match is a
//! first-class answer rather than a failure, and the all-`⊥` assignment is
//! always feasible, so the search never refuses.
//!
//! This module owns the public contract of that search: the budget a caller
//! sets, the outcome it gets back, and the identifiers the outcome is phrased
//! in. [`cost`] owns the algebra the objective is measured in.
//!
//! # The anytime contract
//!
//! [`SolveOutcome`] carries a certified lower bound alongside the incumbent, so
//! an interrupted search returns a solution *and* a proof that nothing better
//! than `lower_bound` exists. The guarantees, holding at every observation
//! point:
//!
//! 1. `lower_bound ⪯ optimum ⪯ upper_bound`.
//! 2. `lower_bound` is monotone non-decreasing and `upper_bound` is monotone
//!    non-increasing.
//! 3. On termination with no limit hit, `proven_optimal` is true and `best` is
//!    a true argmin.
//! 4. `best`, when present, is a real assignment: evaluating it against a
//!    pristine network reproduces `upper_bound` exactly.
//! 5. Exact inference always reports `proven_optimal`, whatever the budget,
//!    because it never prunes and so never consults one.
//! 6. Identical inputs produce identical `best`, `nodes`, and the whole bound
//!    trace.

pub mod build;
pub mod cfn;
pub mod consistency;
pub mod cost;
pub mod dfbb;
pub mod dispatch;
pub mod elim;
pub mod hbfs;
pub mod mcsplit;
pub mod oracle;
pub mod order;

pub use cfn::{Cfn, CfnBuilder, CfnError, CostFunction, Domain, DomainIter, Variable};
pub use consistency::{ConsistencyLevel, Network};
pub use cost::{
    COST_SCALE, Cost, CostWeights, CostWeightsError, DEFAULT_WEIGHTS, DROP_UNIT,
    MAX_COVERAGE_RADIX, coverage_radix, quality_units,
};
pub use dfbb::{SearchParameters, solve_dfbb};
pub use dispatch::{solve, solve_monic};
pub use elim::{
    Buckets, COUNT_CEILING, ProductVerdict, all_optima, count_solutions, decode, detect_product,
    eliminate,
};
pub use hbfs::{BoundObservation, HbfsOutcome, HbfsParameters, solve_hbfs};
pub use mcsplit::{
    ArcDescriptor, HallOutcome, IsoError, TargetId, ValueIndex, arc_descriptor, epic_satisfied,
    propagate_all_different, solve_iso,
};
pub use order::{
    EliminationCost, Graph, choose_order, elimination_cost, fits_budget, induced_width,
    min_fill_order, primal_graph, reverse_source_id_order,
};

/// A variable of the network, one per source vertex.
///
/// Variables are numbered densely from zero in ascending source vertex name
/// order, so the numbering is a function of the source schema alone and two
/// runs over the same schema agree on it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct VarId(u32);

impl VarId {
    /// The variable with this index.
    #[inline]
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The index as it is stored.
    #[inline]
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// The index, for use as a slice offset.
    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A value in a variable's domain.
///
/// Real target vertices occupy indices `0 .. MAX_REAL_VALUES` in ascending
/// target vertex name order, and [`ValId::BOTTOM`] occupies the last index. Two
/// consequences follow from that layout, and both are relied on elsewhere.
/// First, `Ord` puts `⊥` last in every domain, which is the tie-break the
/// search needs, with no separate ordering rule to keep in step. Second, a
/// whole domain including `⊥` fits in one `u32` bitset word.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValId(u32);

impl ValId {
    /// The number of real target vertices a domain can hold.
    ///
    /// The largest domain measured on the schema corpus is 19 values including
    /// `⊥`, so this leaves room to spare while keeping a domain in one word.
    pub const MAX_REAL_VALUES: u32 = 31;

    /// `⊥`, meaning the source vertex is left out of the apex.
    pub const BOTTOM: Self = Self(Self::MAX_REAL_VALUES);

    /// The value standing for the target vertex at this index.
    ///
    /// The check is unconditional rather than a debug assertion because
    /// [`Self::MAX_REAL_VALUES`] is the index of [`Self::BOTTOM`]: an unchecked
    /// `real(31)` would hand back `⊥` under the name of a real target, and a
    /// caller asking for a real value has no way to notice. Use
    /// [`Self::from_index`] where the top slot is meant to be reachable.
    ///
    /// # Panics
    ///
    /// If `index` is not below [`Self::MAX_REAL_VALUES`].
    #[inline]
    #[must_use]
    pub const fn real(index: u32) -> Self {
        assert!(
            index < Self::MAX_REAL_VALUES,
            "a real value index must leave the last slot for bottom"
        );
        Self(index)
    }

    /// The value at this domain index, `⊥` included.
    ///
    /// The total counterpart of [`Self::real`]: index [`Self::MAX_REAL_VALUES`]
    /// is [`Self::BOTTOM`] rather than a contract violation. It is what a bitset
    /// domain needs, since a set bit carries no record of which of the two
    /// constructors put it there.
    ///
    /// The check is a debug assertion rather than an unconditional one because
    /// this is the innermost step of every domain walk and because an index past
    /// the top slot fails safely: the value it produces is in no domain, so a
    /// release build reads it as absent rather than as some other value. That is
    /// the same silent-absence rule [`Domain`]
    /// follows, and it is why [`Self::real`], whose out-of-range index would
    /// alias `⊥` instead, checks unconditionally.
    ///
    /// # Panics
    ///
    /// In debug builds, if `index` is above [`Self::MAX_REAL_VALUES`].
    #[inline]
    #[must_use]
    pub const fn from_index(index: u32) -> Self {
        debug_assert!(
            index <= Self::MAX_REAL_VALUES,
            "a value index must not exceed the bottom slot"
        );
        Self(index)
    }

    /// The index as it is stored.
    #[inline]
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// The index, for use as a slice offset.
    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Whether this is `⊥`.
    #[inline]
    #[must_use]
    pub const fn is_bottom(self) -> bool {
        self.0 == Self::BOTTOM.0
    }
}

/// A total assignment of one value to every variable.
///
/// Indexed by [`VarId`], so its length is the number of source vertices the
/// network was built over.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Assignment(Vec<ValId>);

impl Assignment {
    /// The assignment leaving every source vertex out of the apex.
    ///
    /// Always feasible: every binary constraint is vacuous when both its
    /// endpoints are `⊥`, and the apex well-formedness constraints are vacuous
    /// when the vertex they are conditioned on is `⊥`.
    #[inline]
    #[must_use]
    pub fn all_bottom(variables: usize) -> Self {
        Self(vec![ValId::BOTTOM; variables])
    }

    /// Wrap a value per variable, in variable order.
    #[inline]
    #[must_use]
    pub const fn from_values(values: Vec<ValId>) -> Self {
        Self(values)
    }

    /// The number of variables.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no variables at all.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The value of one variable, or `None` if it is out of range.
    #[inline]
    #[must_use]
    pub fn get(&self, var: VarId) -> Option<ValId> {
        self.0.get(var.index()).copied()
    }

    /// Assign one variable.
    ///
    /// # Panics
    ///
    /// If `var` is out of range for this assignment.
    #[inline]
    pub fn set(&mut self, var: VarId, value: ValId) {
        self.0[var.index()] = value;
    }

    /// Every value, in variable order.
    #[inline]
    #[must_use]
    pub fn values(&self) -> &[ValId] {
        &self.0
    }

    /// Every variable paired with its value.
    #[inline]
    pub fn pairs(&self) -> impl Iterator<Item = (VarId, ValId)> + '_ {
        (0u32..)
            .zip(self.0.iter().copied())
            .map(|(index, value)| (VarId::new(index), value))
    }

    /// The number of source vertices left out of the apex.
    ///
    /// This is the drop count of the packed cost encoding, so it is the
    /// secondary component of the objective.
    #[inline]
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.0.iter().filter(|value| value.is_bottom()).count()
    }
}

/// Which algorithm a component of the network was routed to.
///
/// The four paths are exhaustive. Injectivity is not a property of a network,
/// so the two injective paths are chosen by which entry point the caller calls;
/// everything else goes through [`solve`], which routes on the
/// induced width against the budget.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum SolverPath {
    /// Exact bucket elimination in the `(min, ⊕)` semiring.
    ///
    /// Chosen when the width is small enough that the message tables fit the
    /// budget. It never prunes, never consults a node budget, and always
    /// proves optimality.
    Eliminate {
        /// The induced width of the elimination order actually used.
        width: usize,
    },

    /// Depth-first branch and bound with soft local consistency maintained at
    /// every node.
    ///
    /// The fallback when elimination would not fit. It carries a node budget
    /// and can be interrupted, which is what the anytime contract is for.
    BranchAndBound {
        /// The induced width of the elimination order actually used, which
        /// drives the variable ordering rather than an allocation.
        width: usize,
    },

    /// The injective path: branch and bound with an all-different constraint.
    ///
    /// Injectivity completes the primal graph, so elimination is out by
    /// construction rather than by budget. Reported by
    /// [`solve_monic`].
    Monic,

    /// The maximum common induced sub-schema path.
    ///
    /// Injective and edge-reflecting, which is a strictly stronger demand than
    /// [`Self::Monic`] and a different algorithm. Reported by
    /// [`solve_iso`].
    Iso,
}

/// What stopped a search before it proved optimality.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LimitKind {
    /// The node budget was exhausted.
    Nodes,
    /// The wall-clock budget was exhausted.
    ///
    /// A caller that sets one accepts a non-deterministic result, which is why
    /// there is no default wall-clock budget and why hitting one is reported
    /// rather than silently folded into the answer.
    Time,
}

/// Something a caller should know about how a search was run.
///
/// A warning never means the answer is wrong. It means the search took a route
/// the caller might not have expected, and each one is observable so that the
/// question of how often it happens can be answered from data.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SearchWarning {
    /// Exact inference would not have fit the budget, so the component was
    /// routed to branch and bound instead.
    EliminationOutOfBudget {
        /// The induced width that produced the estimate.
        width: usize,
        /// Message table entries exact inference would have allocated.
        entries: u64,
        /// Combine operations exact inference would have performed.
        operations: u64,
    },

    /// Constraints with no corresponding schema edge raised the induced width.
    ///
    /// Recursion points, schema spans and hyper-edge signature cliques
    /// constrain vertex pairs that need not be joined by an edge, so they can
    /// add primal graph edges that a measurement over schema edges alone would
    /// not see. The width is recomputed after they are added and the dispatcher
    /// reads the recomputed number.
    WidthRaisedByNonEdgeConstraints {
        /// The width of the primal graph of the schema's own edges.
        schema_width: usize,
        /// The width after the constraints were added.
        actual_width: usize,
    },

    /// Surjectivity was requested where it cannot hold.
    ///
    /// The vertex map can only cover the target when the source has at least as
    /// many vertices, so this reports a request that no assignment satisfies.
    EpicUnsatisfiable {
        /// Vertices in the source schema.
        source_vertices: usize,
        /// Vertices in the target schema.
        target_vertices: usize,
    },
}

/// The memory a search may allocate for exact inference, in bytes.
///
/// **calibration:** none. This is an engineering ceiling, not a calibrated
/// value, and it is not a parameter of the objective: no assignment's cost
/// depends on it, and exceeding it changes which algorithm runs rather than
/// which answer is optimal. It is set to a working set an ordinary developer
/// machine can hold without paging, and the honest reason for the exact figure
/// is that it is a round number in that range. Do not tune it against
/// `crates/panproto-lens/tests/autolens_corpus.rs`: that corpus is synthetic
/// and its expectations were themselves derived from engine behaviour, so
/// fitting to it is circular. Tune it against the memory the deployment has.
pub const DEFAULT_MEM_BYTES: usize = 64 * 1024 * 1024;

/// The number of combine operations exact inference may perform.
///
/// **calibration:** none. This is an engineering ceiling, not a calibrated
/// value, chosen as the work a single search may do before the dispatcher
/// prefers a bounded method, and no assignment's cost depends on it. Do not
/// tune it against `crates/panproto-lens/tests/autolens_corpus.rs`: that corpus
/// is synthetic and its expectations were themselves derived from engine
/// behaviour, so fitting to it is circular.
pub const DEFAULT_OP_BUDGET: u64 = 1_000_000_000;

/// The node budget applied when a component is routed to a search path.
///
/// Exact inference ignores it: it never prunes, so it has no nodes to count.
///
/// **calibration:** none. This is an engineering ceiling, not a calibrated
/// value, and exhausting it is reported through
/// [`SolveOutcome::limit_hit`] rather than absorbed, so it bounds effort
/// without silently changing the answer. Do not tune it against
/// `crates/panproto-lens/tests/autolens_corpus.rs`: that corpus is synthetic
/// and its expectations were themselves derived from engine behaviour, so
/// fitting to it is circular.
pub const DEFAULT_SEARCH_NODES: u64 = 10_000_000;

/// What a search may spend.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct SearchBudget {
    /// Nodes a search path may open, or `None` for no node limit.
    ///
    /// `None` becomes [`DEFAULT_SEARCH_NODES`] on the paths that count nodes.
    pub max_nodes: Option<u64>,

    /// Milliseconds of wall clock, or `None` for no time limit.
    ///
    /// There is no default. A time limit makes the result depend on the machine
    /// it ran on, so it is opt-in, and when one is hit the outcome says so.
    pub max_millis: Option<u64>,

    /// Bytes of message table exact inference may allocate before the
    /// dispatcher falls back to search.
    pub mem_bytes: usize,

    /// Combine operations exact inference may perform before the dispatcher
    /// falls back to search.
    pub op_budget: u64,
}

impl Default for SearchBudget {
    fn default() -> Self {
        Self {
            max_nodes: None,
            max_millis: None,
            mem_bytes: DEFAULT_MEM_BYTES,
            op_budget: DEFAULT_OP_BUDGET,
        }
    }
}

impl SearchBudget {
    /// Set the node budget.
    #[inline]
    #[must_use]
    pub const fn with_max_nodes(mut self, max_nodes: Option<u64>) -> Self {
        self.max_nodes = max_nodes;
        self
    }

    /// Set the wall-clock budget.
    #[inline]
    #[must_use]
    pub const fn with_max_millis(mut self, max_millis: Option<u64>) -> Self {
        self.max_millis = max_millis;
        self
    }

    /// Set the memory ceiling for exact inference.
    #[inline]
    #[must_use]
    pub const fn with_mem_bytes(mut self, mem_bytes: usize) -> Self {
        self.mem_bytes = mem_bytes;
        self
    }

    /// Set the operation ceiling for exact inference.
    #[inline]
    #[must_use]
    pub const fn with_op_budget(mut self, op_budget: u64) -> Self {
        self.op_budget = op_budget;
        self
    }
}

/// What a search found, and what it can prove about it.
///
/// The module docs state the six guarantees this type carries.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SolveOutcome {
    /// The best assignment found. `None` only if no feasible assignment was
    /// reached.
    pub best: Option<Assignment>,

    /// Certified: `lower_bound ⪯ optimum` at every observation point.
    pub lower_bound: Cost,

    /// The cost of `best`, or [`Cost::TOP_SENTINEL`] if there is no `best`.
    pub upper_bound: Cost,

    /// Whether the two bounds met, which is the proof of optimality.
    pub proven_optimal: bool,

    /// Which algorithm produced this.
    pub path: SolverPath,

    /// The elimination order actually used, when one was.
    ///
    /// The tie-break among equally good assignments is relative to this order,
    /// so it is reported rather than assumed.
    pub elimination_order: Option<Vec<VarId>>,

    /// Nodes opened. Zero on exact inference, which opens none.
    pub nodes: u64,

    /// What stopped the search, if anything did.
    pub limit_hit: Option<LimitKind>,

    /// Anything a caller should know about the route taken.
    pub warnings: Vec<SearchWarning>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bottom_slot_is_the_last_index() {
        assert_eq!(ValId::BOTTOM, ValId::from_index(ValId::MAX_REAL_VALUES));
        assert!(ValId::from_index(ValId::MAX_REAL_VALUES).is_bottom());
        assert!(!ValId::real(ValId::MAX_REAL_VALUES - 1).is_bottom());
    }

    #[test]
    #[should_panic(expected = "a real value index must leave the last slot for bottom")]
    fn a_real_value_index_cannot_reach_the_bottom_slot() {
        // The check must hold in a release build too: `real` returning `⊥` for
        // the top index would hand a caller the drop value under the name of a
        // target vertex, with nothing to notice it by.
        let index = std::hint::black_box(ValId::MAX_REAL_VALUES);
        let _ = ValId::real(index);
    }
}
