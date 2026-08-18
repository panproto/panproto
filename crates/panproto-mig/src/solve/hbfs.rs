//! Hybrid best-first search: the wrapper that makes branch and bound anytime
//! with a *certified* lower bound.
//!
//! Depth-first branch and bound is anytime only in the upper bound. Its
//! incumbent improves as it runs, but its lower bound stays frozen at whatever
//! the root was filtered to, so an interrupted run hands back a solution with
//! no statement of how wrong it might be. Best-first search has the opposite
//! problem: it certifies a lower bound at every step and finds no solution at
//! all until it finishes, and it holds the whole frontier in memory.
//!
//! The hybrid keeps a frontier of unexplored subtrees, each carrying the bound
//! its node was filtered to, and repeatedly takes the most promising one and
//! dives into it depth-first for a bounded number of backtracks. The subtrees
//! that dive did not reach go back on the frontier. The least bound on the
//! frontier is a valid global lower bound, because the frontier together with
//! the already-closed regions covers the whole assignment space and every
//! closed region was closed under a bound no better than the current one.
//!
//! This is Algorithm 1 of Allouche, de Givry, Katsirelos, Schiex and Zytnicki
//! (CP 2015), with the parameters that paper reports.
//!
//! # The recomputation controller
//!
//! A node on the frontier stores its decisions, not its network state, so
//! reaching it again costs one propagation per decision. The backtrack limit
//! `Z` is what trades that recomputation against the frontier's size: at `Z`
//! unbounded this is plain depth-first search and the frontier stays empty; at
//! `Z` zero it is pure best-first search and the frontier grows exponentially.
//! The controller keeps recomputed nodes between [`HbfsParameters::alpha`] and
//! [`HbfsParameters::beta`] of all nodes by doubling or halving `Z`.
//!
//! # The contract
//!
//! At every entry in [`HbfsOutcome::trace`]:
//!
//! 1. `lower_bound ⪯ optimum ⪯ upper_bound`.
//! 2. `lower_bound` is monotone non-decreasing, `upper_bound` monotone
//!    non-increasing.
//! 3. On termination with no limit hit the two meet, and the assignment
//!    reported achieves that cost against a pristine network.
//!
//! The bounds are relative to the primal bound the search was given: an answer
//! costs strictly less than that bound, and when nothing does, the bound itself
//! is reported as the certificate that nothing does.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use super::SolveOutcome;
use super::cfn::Cfn;
use super::cost::Cost;
use super::dfbb::{BranchAndBound, OpenNode, SearchParameters};

/// How the recomputation controller is tuned.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HbfsParameters {
    /// What the underlying depth-first search is asked to do.
    ///
    /// Restarts are turned off inside this loop: the backtrack limit already
    /// plays the part a restart limit plays there, and the two schedules
    /// fighting each other would make neither legible.
    pub search: SearchParameters,

    /// Below this percentage of recomputed nodes, halve the backtrack limit.
    pub alpha: u64,

    /// Above this percentage of recomputed nodes, double the backtrack limit.
    pub beta: u64,

    /// The ceiling on the backtrack limit.
    pub max_backtracks: u64,

    /// The backtrack limit the first dive is given.
    pub initial_backtracks: u64,

    /// How large the frontier may grow before the controller responds.
    ///
    /// Reaching it doubles the backtrack limit rather than dropping nodes.
    /// Dropping a node would lose the part of the space it stands for, and the
    /// lower bound would then be a claim about less than the whole problem.
    pub max_open: usize,
}

impl Default for HbfsParameters {
    fn default() -> Self {
        Self {
            search: SearchParameters::default().with_restarts(false),
            alpha: 5,
            beta: 10,
            max_backtracks: 1 << 14,
            initial_backtracks: 1,
            max_open: 1 << 20,
        }
    }
}

impl HbfsParameters {
    /// Set the underlying search parameters, with restarts forced off.
    #[must_use]
    pub const fn with_search(mut self, search: SearchParameters) -> Self {
        self.search = search.with_restarts(false);
        self
    }
}

/// The two bounds at one point in the run.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BoundObservation {
    /// Certified: no assignment costs less than this.
    pub lower_bound: Cost,

    /// The incumbent's cost, or [`Cost::TOP_SENTINEL`] while there is no
    /// incumbent.
    ///
    /// This is the same reading [`SolveOutcome::upper_bound`] carries, and it
    /// is deliberately *not* the primal bound the search was given. That bound
    /// is a pruning threshold rather than a statement about the optimum: the
    /// search looks for an assignment costing strictly less than it, so when
    /// none is found the optimum lies at or above it, not below. Reporting the
    /// threshold here would make the trace's last upper bound rise on the way
    /// into the outcome, contradicting the monotonicity this type is here to
    /// exhibit.
    pub upper_bound: Cost,

    /// Nodes opened when this was observed.
    pub nodes: u64,
}

/// What a hybrid best-first search found, with the whole bound trace.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HbfsOutcome {
    /// The answer, in the shape every path reports.
    pub outcome: SolveOutcome,

    /// One entry per bound update, oldest first.
    ///
    /// Reported rather than logged because the anytime guarantees are claims
    /// about every observation point, and a claim about every point is only
    /// testable if the points are available.
    pub trace: Vec<BoundObservation>,
}

/// A frontier entry, ordered so that the heap's top is the node to take next.
///
/// Least lower bound first, and among equal bounds the deepest node, which is
/// the one whose decisions are already closest to a complete assignment.
///
/// The decision sequence breaks the remaining ties. It carries no search
/// meaning, and it is there because without it two distinct frontier nodes at
/// one bound and one depth would compare `Equal` while comparing unequal under
/// the derived `Eq`, which `Ord`'s contract forbids. A total order also fixes
/// which of several equally promising nodes is taken first, so the node counts
/// the anytime contract reports do not depend on how the heap happened to
/// arrange them.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Frontier(OpenNode);

impl Ord for Frontier {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .0
            .lower_bound
            .cmp(&self.0.lower_bound)
            .then_with(|| self.0.depth().cmp(&other.0.depth()))
            .then_with(|| other.0.decisions.cmp(&self.0.decisions))
    }
}

impl PartialOrd for Frontier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Solve a network by hybrid best-first search.
///
/// The answer costs strictly less than `parameters.search.upper_bound`, or
/// there is no answer and the outcome says so.
///
/// # Examples
///
/// ```
/// use panproto_mig::solve::build::{NoEvidence, build_cfn};
/// use panproto_mig::solve::hbfs::{HbfsParameters, solve_hbfs};
/// use panproto_mig::solve::DEFAULT_MEM_BYTES;
/// use panproto_mig::{DEFAULT_WEIGHTS, DomainConstraints, SearchOptions};
/// use panproto_schema::{Protocol, SchemaBuilder};
///
/// let protocol = Protocol {
///     name: "demo".into(),
///     schema_theory: "ThTest".into(),
///     instance_theory: "ThWType".into(),
///     obj_kinds: vec!["object".into(), "string".into()],
///     ..Protocol::default()
/// };
/// let schema = SchemaBuilder::new(&protocol)
///     .vertex("root", "object", None::<&str>)?
///     .vertex("root.label", "string", None::<&str>)?
///     .edge("root", "root.label", "prop", Some("label"))?
///     .build()?;
///
/// let cfn = build_cfn(
///     &schema,
///     &schema,
///     &SearchOptions::default(),
///     &DomainConstraints::default(),
///     &NoEvidence,
///     DEFAULT_WEIGHTS,
///     DEFAULT_MEM_BYTES,
/// )?;
///
/// let found = solve_hbfs(&cfn, &HbfsParameters::default());
///
/// // The bounds bracket the optimum at every observation, and meet at the end.
/// for observation in &found.trace {
///     assert!(observation.lower_bound <= found.outcome.upper_bound);
///     assert!(observation.upper_bound >= found.outcome.upper_bound);
/// }
/// assert!(found.outcome.proven_optimal);
/// assert_eq!(found.outcome.lower_bound, found.outcome.upper_bound);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use]
pub fn solve_hbfs(cfn: &Cfn, parameters: &HbfsParameters) -> HbfsOutcome {
    let search_parameters = parameters.search.clone().with_restarts(false);
    let millis = search_parameters.budget.max_millis;
    let ceiling = search_parameters.upper_bound;
    let mut search = BranchAndBound::new(cfn, &search_parameters);

    let Some(root) = search.prepare_root(millis) else {
        let outcome = search.outcome(ceiling);
        let observation = BoundObservation {
            lower_bound: outcome.lower_bound,
            upper_bound: outcome.upper_bound,
            nodes: outcome.nodes,
        };
        return HbfsOutcome {
            outcome,
            trace: vec![observation],
        };
    };

    let mut open: BinaryHeap<Frontier> = BinaryHeap::new();
    open.push(Frontier(OpenNode::root(root)));
    let mut controller = Controller::new(parameters);
    let mut lower = root;
    let mut trace = vec![BoundObservation {
        lower_bound: lower,
        upper_bound: search.certified_upper_bound(),
        nodes: search.nodes(),
    }];

    while lower < search.upper_bound() {
        let Some(Frontier(node)) = open.pop() else {
            break;
        };
        if search.limit_hit().is_some() {
            open.push(Frontier(node));
            break;
        }
        controller.recomputed(node.depth());
        for child in search.explore(&node, controller.backtracks()) {
            open.push(Frontier(child));
        }
        // An empty frontier reads as "every assignment lies in a region already
        // closed", which is what licenses raising the bound to the primal one:
        // a closed region was closed under a bound no better than the current
        // primal bound, so nothing in it beats that bound. The reading is only
        // available while the frontier has covered the space at every step, so
        // a run that has hit a limit keeps whatever the frontier last certified
        // rather than inheriting a claim the search did not earn.
        let frontier = match open.peek() {
            Some(top) => top.0.lower_bound,
            None if search.limit_hit().is_none() => search.upper_bound(),
            None => lower,
        };
        lower = lower.max(frontier.min(search.upper_bound()));
        trace.push(BoundObservation {
            lower_bound: lower,
            upper_bound: search.certified_upper_bound(),
            nodes: search.nodes(),
        });
        controller.adjust(search.nodes(), open.len());
    }

    HbfsOutcome {
        outcome: search.outcome(lower),
        trace,
    }
}

/// The backtrack limit and the counters that move it.
struct Controller {
    backtracks: u64,
    ceiling: u64,
    alpha: u64,
    beta: u64,
    max_open: usize,
    recomputed: u64,
}

impl Controller {
    fn new(parameters: &HbfsParameters) -> Self {
        Self {
            backtracks: parameters.initial_backtracks.max(1),
            ceiling: parameters.max_backtracks.max(1),
            alpha: parameters.alpha,
            beta: parameters.beta,
            max_open: parameters.max_open.max(1),
            recomputed: 0,
        }
    }

    const fn backtracks(&self) -> u64 {
        self.backtracks
    }

    /// Charge the decisions replayed to reach one node.
    fn recomputed(&mut self, depth: usize) {
        self.recomputed = self
            .recomputed
            .saturating_add(u64::try_from(depth).unwrap_or(u64::MAX));
    }

    /// Move the backtrack limit toward the target recomputation ratio.
    ///
    /// The comparison is `recomputed · 100` against `nodes · percentage` so
    /// that the ratio is decided in integers, which keeps the whole run
    /// reproducible.
    const fn adjust(&mut self, nodes: u64, open: usize) {
        if open > self.max_open {
            self.backtracks = self.backtracks.saturating_mul(2);
            return;
        }
        if self.recomputed == 0 || nodes == 0 {
            return;
        }
        let scaled = self.recomputed.saturating_mul(100);
        if scaled > nodes.saturating_mul(self.beta) && self.backtracks <= self.ceiling {
            self.backtracks = self.backtracks.saturating_mul(2);
        } else if scaled < nodes.saturating_mul(self.alpha) && self.backtracks >= 2 {
            self.backtracks /= 2;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::solve::cfn::CfnBuilder;
    use crate::solve::cost::DEFAULT_WEIGHTS;
    use crate::solve::dfbb::solve_dfbb;
    use crate::solve::oracle::brute_force;
    use crate::solve::{ValId, VarId};
    use panproto_gat::Name;

    const FIRST: VarId = VarId::new(0);
    const SECOND: VarId = VarId::new(1);
    const THIRD: VarId = VarId::new(2);

    fn cost(units: u64) -> Cost {
        Cost::from_raw(units)
    }

    /// Three variables over one target each, so every domain is `{t, ⊥}` and
    /// every binary table has four entries.
    fn triple() -> Cfn {
        let mut builder = CfnBuilder::new(
            vec![
                (Name::new("u"), vec![Name::new("t")]),
                (Name::new("v"), vec![Name::new("t")]),
                (Name::new("w"), vec![Name::new("t")]),
            ],
            DEFAULT_WEIGHTS,
        )
        .unwrap();
        builder.add_unary(FIRST, ValId::real(0), cost(2)).unwrap();
        builder.add_unary(SECOND, ValId::BOTTOM, cost(5)).unwrap();
        builder.add_unary(THIRD, ValId::real(0), cost(1)).unwrap();
        builder
            .add_function(&[FIRST, SECOND], vec![cost(4), cost(1), cost(6), cost(3)])
            .unwrap();
        builder
            .add_function(&[SECOND, THIRD], vec![cost(2), cost(7), cost(1), cost(9)])
            .unwrap();
        builder.build()
    }

    #[test]
    fn the_bounds_bracket_the_optimum_at_every_observation() {
        let cfn = triple();
        let (optimum, argmins) = brute_force(&cfn);
        let found = solve_hbfs(&cfn, &HbfsParameters::default());

        let mut previous: Option<BoundObservation> = None;
        for observation in &found.trace {
            assert!(
                observation.lower_bound <= optimum,
                "the lower bound overshot"
            );
            assert!(
                observation.upper_bound >= optimum,
                "the upper bound undershot"
            );
            if let Some(earlier) = previous {
                assert!(observation.lower_bound >= earlier.lower_bound);
                assert!(observation.upper_bound <= earlier.upper_bound);
            }
            previous = Some(*observation);
        }

        assert!(found.outcome.proven_optimal);
        assert_eq!(found.outcome.lower_bound, optimum);
        assert_eq!(found.outcome.upper_bound, optimum);
        let best = found.outcome.best.unwrap();
        assert_eq!(cfn.evaluate(&best), optimum);
        assert!(argmins.contains(&best));
    }

    #[test]
    fn best_first_and_depth_first_agree() {
        let cfn = triple();
        let depth = solve_dfbb(&cfn, &SearchParameters::default());
        let hybrid = solve_hbfs(&cfn, &HbfsParameters::default());
        assert_eq!(depth.upper_bound, hybrid.outcome.upper_bound);
        assert_eq!(depth.lower_bound, hybrid.outcome.lower_bound);
    }

    #[test]
    fn a_bound_below_the_optimum_admits_no_solution() {
        let cfn = triple();
        let (optimum, _) = brute_force(&cfn);
        let parameters = HbfsParameters::default()
            .with_search(SearchParameters::default().with_upper_bound(optimum));
        let found = solve_hbfs(&cfn, &parameters);
        assert!(found.outcome.best.is_none());
        assert!(found.outcome.proven_optimal);
        assert_eq!(found.outcome.lower_bound, optimum);
    }
}
