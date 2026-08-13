//! Depth-first branch and bound with soft local consistency maintained at
//! every node.
//!
//! This is the fallback path. Exact inference handles every schema the corpus
//! measures, because the induced width there is at most two; this handles the
//! inputs the corpus does not cover, and it is held to the same standard: the
//! optimum it returns is the optimum, and the assignment it returns achieves
//! that optimum against a pristine network.
//!
//! # The shape of a node
//!
//! At every node the working network is filtered to a [`ConsistencyLevel`],
//! which moves cost into `c_∅`, and `c_∅` is then a certified lower bound on
//! every completion below that node. A node whose bound reaches the incumbent's
//! cost is closed without being explored.
//!
//! `⊤` is the incumbent's cost, not a constant. Setting the top of the
//! valuation structure to the best cost found so far is what turns the
//! truncating `⊕` into pruning: any partial assignment whose accumulated cost
//! reaches the incumbent is `⊤`, hence infeasible, hence prunable by node
//! consistency. It also means the search returns a solution of cost **strictly
//! below** the bound it was given, or reports that none exists.
//!
//! # What is trailed and what is copied
//!
//! Cost cells are trailed, one `(index, old value)` pair per write, with a
//! single restore back to a mark. Domains are one `u64` per variable, so they
//! are copied on branching and restored by assignment. Two mechanisms rather
//! than one, and the sizes are the reason.
//!
//! # The heuristics, and why they are these
//!
//! **Variable order: `dom/wdeg` with bound-failure attribution.** Plain
//! `dom/wdeg` raises a cost function's weight when propagating it empties a
//! domain. In a branch and bound with a maintained bound most nodes die from
//! the bound rather than from an empty domain, so plain `dom/wdeg` sees almost
//! no failures and decays into `dom`. The fix is to attribute a bound failure
//! too: the cost functions that moved the most cost into `c_∅` at that node are
//! the ones that did the pruning, and they are the ones whose weight rises.
//! Ties go to the variable with the most already-assigned neighbours in the
//! primal graph, then to the higher static degree, then to the lower
//! identifier.
//!
//! **Value order: bound impact, then phase saving.** Before there is an
//! incumbent the useful question is which value leads to the best bound, and
//! answering it costs one propagation per value, which is affordable exactly
//! once per node during the dive to the first solution. After there is an
//! incumbent the useful question is which value the incumbent chose, because
//! improving solutions live near it; that costs one array lookup.
//!
//! **Restarts: Luby, scaled 660, with decision nogoods.** Without restarts the
//! weights and the incumbent cannot redirect a search that has already
//! committed at the top of the tree. Nogoods recorded from the restart keep the
//! work: the conjunction of decisions leading into a subtree that was closed
//! under the then-current bound is still closed under any later, tighter bound,
//! because the bound only ever falls. They propagate on two watched literals,
//! which needs no work on backtracking and so survives copy-on-branch for free.

use std::time::Instant;

use rustc_hash::FxHashMap;

use super::cfn::{Cfn, Domain};
use super::consistency::{ConsistencyLevel, Network};
use super::cost::Cost;
use super::mcsplit::{HallOutcome, ValueIndex, epic_satisfied, propagate_all_different};
use super::{
    Assignment, DEFAULT_SEARCH_NODES, LimitKind, SearchBudget, SolveOutcome, SolverPath, ValId,
    VarId,
};

/// The Luby restart schedule is scaled by this many backtracks.
///
/// The value tuned for the Glasgow subgraph solver family, kept because the
/// quantity it scales, a count of backtracks between restarts, means the same
/// thing here.
pub const LUBY_SCALE: u64 = 660;

/// How many cost functions share the blame for one bound failure.
///
/// Attribution is a heuristic, so the width is a judgement rather than a
/// measurement: one is too narrow, because a bound rises from several
/// contributions at once, and all of them is no attribution at all.
pub const BLAME_WIDTH: usize = 3;

/// How many nogoods one search keeps.
///
/// A ceiling rather than a target: nogoods are recorded once per restart and
/// never forgotten, so without one a long run over a wide instance would grow
/// the watch lists without bound. Dropping the ones past the ceiling costs
/// pruning power and nothing else, since a nogood is a shortcut rather than a
/// part of the answer.
pub const MAX_NOGOODS: usize = 1 << 14;

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

/// What a search is asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SearchParameters {
    /// How hard each node is filtered before its bound is read.
    pub level: ConsistencyLevel,

    /// The primal bound. The search looks for a solution costing **strictly**
    /// less than this and reports none if there is none.
    pub upper_bound: Cost,

    /// What the search may spend.
    pub budget: SearchBudget,

    /// The induced width to report in the outcome.
    ///
    /// The dispatcher computes it while choosing a path; the search carries it
    /// rather than recomputing it.
    pub width: usize,

    /// Whether to restart on the Luby schedule.
    ///
    /// Off inside best-first search, whose backtrack limit already plays the
    /// part a restart limit plays here.
    pub restarts: bool,

    /// How many backtracks one unit of the Luby schedule is worth.
    ///
    /// [`LUBY_SCALE`] in normal use. It is a parameter rather than a constant
    /// so that a test can drive the restart machinery, and the nogoods that
    /// only exist to make restarts pay, on an instance small enough to check
    /// exhaustively: at the shipped scale neither ever fires on one.
    pub restart_scale: u64,

    /// Whether no two variables may take the same target.
    ///
    /// The injective restriction, filtered at every node by the Hall-set
    /// propagator. `⊥` is exempt: any number of source vertices may be left out
    /// of the apex, so a variable that can still be dropped never enters a
    /// pigeonhole and never causes one.
    ///
    /// It is a search parameter rather than something the network states
    /// because no cost function can state it: injectivity constrains how
    /// variables share values, which is a property of an assignment rather than
    /// of any bounded scope.
    pub all_different: bool,

    /// Whether the answer's vertex map must cover every target vertex, and how
    /// many target vertices there are to cover.
    ///
    /// `Some(n)` optimises over the *surjective* assignments only: a complete
    /// assignment that leaves one of the `n` target vertices uncovered is
    /// rejected at the leaf and never becomes the incumbent.
    ///
    /// This is a search parameter for the same reason `all_different` is, and
    /// it is enforced here rather than filtered afterwards for a sharper
    /// reason: filtering the argmin set discards every surjective assignment
    /// that is not itself an argmin, so it reports "none exists" whenever the
    /// optimum happens not to be onto. Optimising over the surjective subspace
    /// answers the question that was asked. Branch and bound stays correct
    /// because every bound remains a lower bound over a *superset* of the
    /// completions now admitted; what changes is only which complete
    /// assignments may become the incumbent.
    ///
    /// The count is the target schema's, not the network's: a target vertex no
    /// variable can take is one no assignment can cover, and counting the
    /// network's values instead would call such a search surjective.
    pub epic: Option<usize>,
}

impl Default for SearchParameters {
    fn default() -> Self {
        Self {
            level: ConsistencyLevel::default(),
            upper_bound: Cost::TOP_SENTINEL,
            budget: SearchBudget::default(),
            width: 0,
            restarts: true,
            restart_scale: LUBY_SCALE,
            all_different: false,
            epic: None,
        }
    }
}

impl SearchParameters {
    /// Set the consistency level.
    #[must_use]
    pub const fn with_level(mut self, level: ConsistencyLevel) -> Self {
        self.level = level;
        self
    }

    /// Set the primal bound.
    #[must_use]
    pub const fn with_upper_bound(mut self, upper_bound: Cost) -> Self {
        self.upper_bound = upper_bound;
        self
    }

    /// Set the budget.
    #[must_use]
    pub const fn with_budget(mut self, budget: SearchBudget) -> Self {
        self.budget = budget;
        self
    }

    /// Set the width to report.
    #[must_use]
    pub const fn with_width(mut self, width: usize) -> Self {
        self.width = width;
        self
    }

    /// Turn restarts on or off.
    #[must_use]
    pub const fn with_restarts(mut self, restarts: bool) -> Self {
        self.restarts = restarts;
        self
    }

    /// Require the assignment to be injective on real targets.
    #[must_use]
    pub const fn with_all_different(mut self, all_different: bool) -> Self {
        self.all_different = all_different;
        self
    }

    /// Require the assignment to cover every one of `target_vertices` targets.
    #[must_use]
    pub const fn with_epic(mut self, target_vertices: Option<usize>) -> Self {
        self.epic = target_vertices;
        self
    }

    /// Set how many backtracks one unit of the Luby schedule is worth.
    #[must_use]
    pub const fn with_restart_scale(mut self, restart_scale: u64) -> Self {
        self.restart_scale = restart_scale;
        self
    }
}

// ---------------------------------------------------------------------------
// Decisions and open nodes
// ---------------------------------------------------------------------------

/// One branching decision.
///
/// `Ord` is lexicographic on `(variable, value, assigns)`. It carries no search
/// meaning: it exists so that a decision sequence can be a total-order
/// tie-break, which is what keeps the frontier's ordering total.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Decision {
    /// The variable the decision is about.
    pub variable: VarId,

    /// The value the decision is about.
    pub value: ValId,

    /// Whether the decision gives the variable that value, or takes it away.
    pub assigns: bool,
}

impl Decision {
    /// Give the variable the value.
    #[must_use]
    pub const fn assign(variable: VarId, value: ValId) -> Self {
        Self {
            variable,
            value,
            assigns: true,
        }
    }

    /// Take the value away from the variable.
    #[must_use]
    pub const fn refute(variable: VarId, value: ValId) -> Self {
        Self {
            variable,
            value,
            assigns: false,
        }
    }
}

/// A subtree that has not been explored, named by the decisions that reach it.
///
/// Storing the decisions rather than the network state is what keeps the
/// frontier small enough to hold: replaying them costs one propagation per
/// decision, and that recomputation is what the backtrack limit is tuned
/// against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenNode {
    /// The decisions from the root, in order.
    pub decisions: Vec<Decision>,

    /// A certified lower bound on every assignment below this node.
    pub lower_bound: Cost,
}

impl OpenNode {
    /// The root: no decisions, and the bound the root was filtered to.
    #[must_use]
    pub const fn root(lower_bound: Cost) -> Self {
        Self {
            decisions: Vec::new(),
            lower_bound,
        }
    }

    /// How many decisions reach this node.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.decisions.len()
    }
}

// ---------------------------------------------------------------------------
// Nogoods
// ---------------------------------------------------------------------------

/// A conjunction of assignments that has been shown to contain no solution
/// better than the bound in force when it was recorded.
///
/// It stays true as the bound falls, because a subtree with nothing better than
/// `k` has nothing better than any `k' ⪯ k`.
#[derive(Clone, Debug)]
struct Nogood {
    literals: Vec<(VarId, ValId)>,
    watches: [usize; 2],
}

/// What one literal of a nogood says about the current domains.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Literal {
    /// The variable is fixed to the value, so the literal holds.
    Held,
    /// The value is gone from the domain, so the literal can never hold here.
    Dead,
    /// Neither yet.
    Open,
}

// ---------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------

/// The constraints on a complete assignment that no cost function states.
///
/// Injectivity and surjectivity are properties of an assignment rather than of
/// any bounded scope, so neither can be a table in the network and both are
/// carried here. They share a target numbering because both ask the same
/// question of a `(variable, value)` pair: which global target does it name.
struct GlobalConstraints {
    /// The numbering both constraints compare in, built only when one is asked
    /// for. `None` is the ordinary span search, where two source vertices may
    /// share a target and every target may go uncovered.
    values: Option<ValueIndex>,

    /// Whether no two variables may take the same target.
    all_different: bool,

    /// How many target vertices a complete assignment must cover.
    epic: Option<usize>,
}

/// A depth-first branch and bound over one cost function network.
///
/// [`Self::run`] is the whole search. The finer entry points exist for the
/// best-first wrapper, which drives the same depth-first core from a frontier
/// of open nodes rather than from the root.
pub struct BranchAndBound<'a> {
    /// The pristine network, which every answer is scored against.
    cfn: &'a Cfn,

    /// The working copy the transformations mutate.
    net: Network,

    /// How hard each node is filtered.
    level: ConsistencyLevel,

    /// The bound the search was given, which no answer may reach.
    initial_upper_bound: Cost,

    /// The cost of the incumbent, or the initial bound if there is none.
    cub: Cost,

    /// The best assignment found.
    incumbent: Option<Assignment>,

    /// One weight per cost function, then one per variable's unary table.
    weights: Vec<u32>,

    /// The primal graph: the variables each variable shares a cost function
    /// with.
    neighbours: Vec<Vec<VarId>>,

    /// The static degree of each variable in the primal graph.
    degree: Vec<u32>,

    /// The constraints on the assignment as a whole, which no cost function
    /// states and which the network therefore cannot enforce.
    global: GlobalConstraints,

    /// The value the incumbent gave each variable.
    phase: Vec<Option<ValId>>,

    /// Nodes opened.
    nodes: u64,

    /// The node ceiling, if there is one.
    node_budget: Option<u64>,

    /// The wall-clock deadline, if there is one.
    deadline: Option<Instant>,

    /// What stopped the search, if anything did.
    limit_hit: Option<LimitKind>,

    /// Backtracks taken since the current dive began.
    backtracks: u64,

    /// The backtrack ceiling for the current dive.
    backtrack_limit: u64,

    /// Whether the current dive is unwinding.
    stopping: bool,

    /// The decisions from the root to the current node.
    path: Vec<Decision>,

    /// Subtrees the current dive left unexplored.
    open: Vec<OpenNode>,

    /// Whether to record those subtrees rather than abandon them.
    collect_open: bool,

    /// The nogoods recorded so far.
    nogoods: Vec<Nogood>,

    /// Which nogoods watch each literal.
    watch: FxHashMap<(u32, u32), Vec<usize>>,

    /// The width the outcome reports.
    width: usize,

    /// Whether to restart on the Luby schedule, which is also whether nogoods
    /// are kept: a nogood is only worth recording if a restart will meet it
    /// again.
    restarts: bool,

    /// How many backtracks one unit of the Luby schedule is worth.
    restart_scale: u64,
}

impl<'a> BranchAndBound<'a> {
    /// Start a search over a network.
    #[must_use]
    pub fn new(cfn: &'a Cfn, parameters: &SearchParameters) -> Self {
        let count = cfn.n_variables();
        let net = Network::from_cfn(cfn, parameters.upper_bound);
        let mut neighbours = vec![Vec::new(); count];
        for function in cfn.functions() {
            for var in function.scope() {
                for other in function.scope() {
                    if other == var {
                        continue;
                    }
                    if let Some(list) = neighbours.get_mut(var.index()) {
                        if !list.contains(other) {
                            list.push(*other);
                        }
                    }
                }
            }
        }
        let degree = neighbours
            .iter()
            .map(|list| u32::try_from(list.len()).unwrap_or(u32::MAX))
            .collect();
        let weights = vec![0u32; net.weight_slots()];
        Self {
            cfn,
            net,
            level: parameters.level,
            initial_upper_bound: parameters.upper_bound,
            cub: parameters.upper_bound,
            incumbent: None,
            weights,
            neighbours,
            degree,
            global: GlobalConstraints {
                values: (parameters.all_different || parameters.epic.is_some())
                    .then(|| ValueIndex::of(cfn)),
                all_different: parameters.all_different,
                epic: parameters.epic,
            },
            phase: vec![None; count],
            nodes: 0,
            node_budget: Some(parameters.budget.max_nodes.unwrap_or(DEFAULT_SEARCH_NODES)),
            deadline: None,
            limit_hit: None,
            backtracks: 0,
            backtrack_limit: u64::MAX,
            stopping: false,
            path: Vec::new(),
            open: Vec::new(),
            collect_open: false,
            nogoods: Vec::new(),
            watch: FxHashMap::default(),
            width: parameters.width,
            restarts: parameters.restarts,
            restart_scale: parameters.restart_scale,
        }
    }

    /// The cost of the incumbent, or the bound the search was given.
    ///
    /// This is the pruning threshold `⊤` is set from, not a statement about the
    /// optimum. Use [`Self::certified_upper_bound`] for the latter.
    #[inline]
    #[must_use]
    pub const fn upper_bound(&self) -> Cost {
        self.cub
    }

    /// An upper bound on the optimum: the incumbent's cost, or
    /// [`Cost::TOP_SENTINEL`] while there is no incumbent.
    ///
    /// The two differ exactly while nothing has beaten the bound the search was
    /// given. The search asks for an assignment costing strictly less than that
    /// bound, so failing to find one places the optimum at or above it, and
    /// reporting the bound as an upper bound on the optimum would invert that.
    #[inline]
    #[must_use]
    pub const fn certified_upper_bound(&self) -> Cost {
        if self.incumbent.is_some() {
            self.cub
        } else {
            Cost::TOP_SENTINEL
        }
    }

    /// The best assignment found.
    #[inline]
    #[must_use]
    pub const fn incumbent(&self) -> Option<&Assignment> {
        self.incumbent.as_ref()
    }

    /// Nodes opened so far.
    #[inline]
    #[must_use]
    pub const fn nodes(&self) -> u64 {
        self.nodes
    }

    /// What stopped the search, if anything did.
    #[inline]
    #[must_use]
    pub const fn limit_hit(&self) -> Option<LimitKind> {
        self.limit_hit
    }

    /// The `dom/wdeg` weights, one per cost function then one per variable's
    /// unary table.
    #[inline]
    #[must_use]
    pub fn weights(&self) -> &[u32] {
        &self.weights
    }

    /// Filter the root and read its bound, or `None` if the root is already
    /// closed.
    ///
    /// The wall-clock budget starts here, so that the clock a caller set
    /// measures the search rather than whatever came before it.
    pub fn prepare_root(&mut self, millis: Option<u64>) -> Option<Cost> {
        self.deadline =
            millis.map(|limit| Instant::now() + std::time::Duration::from_millis(limit));
        self.net.reset(self.cub);
        self.path.clear();
        if self.propagate(None) {
            Some(self.net.c_empty())
        } else {
            None
        }
    }

    /// Run the whole search from the root.
    #[must_use]
    pub fn run(&mut self, millis: Option<u64>) -> SolveOutcome {
        let Some(root) = self.prepare_root(millis) else {
            return self.outcome(self.initial_upper_bound);
        };
        let mut restart = 1u64;
        loop {
            self.stopping = false;
            self.backtracks = 0;
            self.collect_open = false;
            self.backtrack_limit = if self.restarts {
                self.restart_scale.max(1).saturating_mul(luby(restart))
            } else {
                u64::MAX
            };
            self.net.reset(self.cub);
            self.path.clear();
            if !self.propagate(None) {
                break;
            }
            self.dive();
            if !self.stopping || self.limit_hit.is_some() {
                break;
            }
            restart = restart.saturating_add(1);
        }
        self.outcome(root)
    }

    /// Explore one open node, up to a backtrack limit, collecting the subtrees
    /// left behind.
    ///
    /// Returns the subtrees, which is empty when the node was closed outright
    /// or explored to exhaustion.
    pub fn explore(&mut self, node: &OpenNode, backtracks: u64) -> Vec<OpenNode> {
        self.net.reset(self.cub);
        self.path.clear();
        self.open.clear();
        self.stopping = false;
        self.backtracks = 0;
        self.backtrack_limit = backtracks.max(1);
        self.collect_open = true;
        if !self.propagate(None) {
            return Vec::new();
        }
        for decision in &node.decisions {
            self.path.push(*decision);
        }
        if !self.replay(&node.decisions) {
            return Vec::new();
        }
        self.dive();
        std::mem::take(&mut self.open)
    }

    /// The outcome, phrased in the contract the module docs state.
    #[must_use]
    pub fn outcome(&self, root: Cost) -> SolveOutcome {
        let proven = self.limit_hit.is_none();
        let (lower, upper) = match (&self.incumbent, proven) {
            (Some(_), true) => (self.cub, self.cub),
            (Some(_), false) => (root.min(self.cub), self.cub),
            // Nothing below the bound the search was given, and the whole space
            // was covered: the bound itself is then a certificate.
            (None, true) => (self.initial_upper_bound, Cost::TOP_SENTINEL),
            (None, false) => (root, Cost::TOP_SENTINEL),
        };
        SolveOutcome {
            best: self.incumbent.clone(),
            lower_bound: lower,
            upper_bound: upper,
            proven_optimal: proven,
            path: SolverPath::BranchAndBound { width: self.width },
            elimination_order: None,
            nodes: self.nodes,
            limit_hit: self.limit_hit,
            warnings: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// The dive
// ---------------------------------------------------------------------------

impl BranchAndBound<'_> {
    /// Replay a decision sequence from the root, propagating after each.
    ///
    /// Returns whether the node the sequence names is still alive.
    fn replay(&mut self, decisions: &[Decision]) -> bool {
        for decision in decisions {
            if decision.assigns {
                self.net.assign(decision.variable, decision.value);
                if !self.propagate(Some((decision.variable, decision.value))) {
                    return false;
                }
            } else {
                self.net.refute(decision.variable, decision.value);
                if !self.propagate(None) {
                    return false;
                }
            }
        }
        true
    }

    /// Explore the subtree below the current node.
    ///
    /// The network is already filtered, feasible, and bounded below the
    /// incumbent when this is entered.
    fn dive(&mut self) {
        self.nodes = self.nodes.saturating_add(1);
        if self.over_budget() {
            self.stopping = true;
            // This subtree is abandoned whole, so it goes on the frontier as it
            // stands. Every other exit leaves the frontier covering the space
            // below this node: the backtrack-limit exit records the right branch
            // once the left is exhausted, and the propagated-stop exit records
            // the right branch while the frame that stopped records its own.
            // Returning here with nothing recorded would stop the frontier
            // partitioning the assignment space, and the frontier partitioning
            // the space is the only reason its least bound is a lower bound at
            // all.
            self.record_open_here();
            return;
        }
        let bound = self.net.c_empty();
        let Some(variable) = self.select_variable() else {
            self.record_solution();
            return;
        };
        let Some(value) = self.select_value(variable) else {
            return;
        };
        let saved = self.net.domains().to_vec();

        let mark = self.net.mark();
        self.path.push(Decision::assign(variable, value));
        self.net.assign(variable, value);
        if self.propagate(Some((variable, value))) {
            self.dive();
        }
        self.path.pop();
        self.net.restore(mark);
        self.net.set_domains(&saved);

        if self.stopping {
            self.record_open(variable, value, bound);
            return;
        }
        self.backtracks = self.backtracks.saturating_add(1);
        if self.backtracks >= self.backtrack_limit {
            self.stopping = true;
            self.record_restart_nogoods(Some((variable, value)));
            self.record_open(variable, value, bound);
            return;
        }

        let mark = self.net.mark();
        self.path.push(Decision::refute(variable, value));
        self.net.refute(variable, value);
        if self.propagate(None) {
            self.dive();
        }
        self.path.pop();
        self.net.restore(mark);
        self.net.set_domains(&saved);
    }

    /// Whether the budget has run out, recording which one did.
    fn over_budget(&mut self) -> bool {
        if let Some(limit) = self.node_budget {
            if self.nodes > limit {
                self.limit_hit = Some(LimitKind::Nodes);
                return true;
            }
        }
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                self.limit_hit = Some(LimitKind::Time);
                return true;
            }
        }
        false
    }

    /// Filter the current node and decide whether it is still alive.
    fn propagate(&mut self, assigned: Option<(VarId, ValId)>) -> bool {
        self.net.reset_contributions();
        self.net.set_top(self.cub);
        if let Some(literal) = assigned {
            if !self.propagate_nogoods(literal) {
                return false;
            }
        }
        if self.net.enforce(self.level) {
            if self.filter_all_different() {
                return true;
            }
            // A Hall failure names a set of variables rather than one, so there
            // is no empty domain for the wipe-out attribution to find and no
            // bound movement for the bound attribution to read. The node is
            // still closed; it just does not move a weight.
            return false;
        }
        // Both attributions run, and they are deliberately not exclusive. `⊤`
        // is the incumbent's cost, so a bound that reaches it makes every value
        // node-inconsistent and empties every domain on the way out: an
        // exclusive classification that preferred the empty domain would
        // attribute nearly every failure to a wipe-out and reproduce exactly
        // the defect the bound attribution exists to fix. A wipe-out with the
        // bound still below the incumbent is a separate, real event, and it is
        // the only one plain weighted degree would ever have counted.
        self.blame_bound();
        self.blame_wipeout();
        false
    }

    /// Apply the all-different filter, returning whether the node survives.
    ///
    /// A no-op unless the search is injective. The propagator is stateless and
    /// writes only domains, which are restored by copy at every branch, so it
    /// needs no trail entry of its own.
    ///
    /// A complete assignment that survives this is injective: when every domain
    /// is a singleton, two variables holding the same target are a Hall set of
    /// one target with two members, which is the pigeonhole the sweep reports.
    fn filter_all_different(&mut self) -> bool {
        if !self.global.all_different {
            return true;
        }
        let mut domains = self.net.domains().to_vec();
        let wiped = self.global.values.as_ref().is_some_and(|index| {
            propagate_all_different(index, &mut domains) == HallOutcome::Wipeout
        });
        if wiped {
            return false;
        }
        self.net.set_domains(&domains);
        !self.net.domains().iter().any(|domain| domain.is_empty())
    }

    /// Accept a complete assignment if it beats the incumbent.
    fn record_solution(&mut self) {
        let mut values = Vec::with_capacity(self.net.n_variables());
        for var in self.net.variable_ids() {
            let Some(value) = self.net.domain(var).only() else {
                return;
            };
            values.push(value);
        }
        let assignment = Assignment::from_values(values);
        // Surjectivity is a leaf test: it constrains the whole assignment at
        // once, so there is nothing to propagate and nothing to bound with, and
        // the only sound place to apply it is where a complete assignment is
        // offered as the incumbent. Applying it here rather than to the argmins
        // afterwards is what makes the search optimise over the surjective
        // assignments instead of reporting none when the optimum is not one.
        if let Some(target_vertices) = self.global.epic {
            let onto = self
                .global
                .values
                .as_ref()
                .is_some_and(|index| epic_satisfied(index, &assignment, target_vertices));
            if !onto {
                return;
            }
        }
        // Scored against the pristine network, never against the transformed
        // one: the transformations move cost between functions, so a search can
        // report a number that is right in the working copy while the
        // assignment does not achieve it in the original.
        let cost = self.cfn.evaluate(&assignment);
        if cost >= self.cub {
            return;
        }
        self.cub = cost;
        for (slot, value) in self.phase.iter_mut().zip(assignment.values()) {
            *slot = Some(*value);
        }
        self.incumbent = Some(assignment);
    }

    /// Remember the current node itself as unexplored.
    ///
    /// Used where a dive is abandoned before it branches, so that the subtree
    /// below the decisions already on the path is still covered by the
    /// frontier. The bound is the one the node was filtered to, which is what
    /// the caller's propagation left in `c_∅`.
    fn record_open_here(&mut self) {
        if !self.collect_open {
            return;
        }
        self.open.push(OpenNode {
            decisions: self.path.clone(),
            lower_bound: self.net.c_empty(),
        });
    }

    /// Remember the right branch of the current node as unexplored.
    fn record_open(&mut self, variable: VarId, value: ValId, bound: Cost) {
        if !self.collect_open {
            return;
        }
        let mut decisions = self.path.clone();
        decisions.push(Decision::refute(variable, value));
        self.open.push(OpenNode {
            decisions,
            lower_bound: bound,
        });
    }
}

// ---------------------------------------------------------------------------
// Heuristics
// ---------------------------------------------------------------------------

/// How a variable ranks for branching, smallest first.
///
/// The first two components are the `dom/wdeg` ratio, compared by cross
/// multiplication so that no float enters the search.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct Rank {
    domain: u64,
    weight: u64,
    assigned_neighbours: u32,
    degree: u32,
    variable: VarId,
}

impl Rank {
    /// Whether this variable should be branched on before the other.
    fn beats(self, other: Self) -> bool {
        let here = self.domain.saturating_mul(other.weight.saturating_add(1));
        let there = other.domain.saturating_mul(self.weight.saturating_add(1));
        if here != there {
            return here < there;
        }
        if self.assigned_neighbours != other.assigned_neighbours {
            return self.assigned_neighbours > other.assigned_neighbours;
        }
        if self.degree != other.degree {
            return self.degree > other.degree;
        }
        self.variable < other.variable
    }
}

impl BranchAndBound<'_> {
    /// The next variable to branch on, or `None` when every variable is fixed.
    fn select_variable(&self) -> Option<VarId> {
        let mut best: Option<Rank> = None;
        for variable in self.net.variable_ids() {
            let domain = self.net.domain(variable);
            if domain.len() <= 1 {
                continue;
            }
            let rank = self.rank(variable, domain);
            if best.is_none_or(|found| rank.beats(found)) {
                best = Some(rank);
            }
        }
        best.map(|rank| rank.variable)
    }

    /// The ranking of one variable.
    fn rank(&self, variable: VarId, domain: Domain) -> Rank {
        let mut weight = u64::from(
            self.weights
                .get(self.net.unary_slot(variable))
                .copied()
                .unwrap_or(0),
        );
        for &function in self.net.incident(variable) {
            let unassigned = self
                .net
                .scope(function)
                .iter()
                .filter(|var| !self.net.is_assigned(**var))
                .count();
            if unassigned < 2 {
                continue;
            }
            weight =
                weight.saturating_add(u64::from(self.weights.get(function).copied().unwrap_or(0)));
        }
        let assigned_neighbours = self.neighbours.get(variable.index()).map_or(0, |list| {
            list.iter()
                .filter(|var| self.net.is_assigned(**var))
                .count()
        });
        Rank {
            domain: u64::try_from(domain.len()).unwrap_or(u64::MAX),
            weight,
            assigned_neighbours: u32::try_from(assigned_neighbours).unwrap_or(u32::MAX),
            degree: self.degree.get(variable.index()).copied().unwrap_or(0),
            variable,
        }
    }

    /// The value to try first, or `None` when the variable has none left.
    ///
    /// The incumbent's choice once there is one, and the value with the best
    /// resulting bound before there is. Every branch draws from the current
    /// domain, and the return is an option rather than a value so that a future
    /// heuristic cannot quietly hand back one that is not in it: branching on a
    /// value the variable cannot take would leave the right branch refuting
    /// something already absent, which changes nothing and recurses forever.
    fn select_value(&mut self, variable: VarId) -> Option<ValId> {
        if self.incumbent.is_some() {
            if let Some(saved) = self.phase.get(variable.index()).copied().flatten() {
                if self.net.domain(variable).contains(saved) {
                    return Some(saved);
                }
            }
            return self
                .net
                .eac_support(variable)
                .or_else(|| self.net.cheapest_value(variable));
        }
        self.bound_impact_value(variable)
    }

    /// The value whose assignment leaves the best bound.
    ///
    /// One propagation per value, which is affordable only while there is no
    /// incumbent, which is exactly when the question is worth asking.
    fn bound_impact_value(&mut self, variable: VarId) -> Option<ValId> {
        let saved = self.net.domains().to_vec();
        let mut best: Option<(ValId, Cost)> = None;
        for value in self.net.domain(variable) {
            let mark = self.net.mark();
            self.net.assign(variable, value);
            let bound = if self.net.enforce(self.level) {
                self.net.c_empty()
            } else {
                Cost::TOP_SENTINEL
            };
            self.net.restore(mark);
            self.net.set_domains(&saved);
            if best.is_none_or(|(_, found)| bound < found) {
                best = Some((value, bound));
            }
        }
        self.net.reset_contributions();
        best.map(|(value, _)| value)
    }

    /// Raise the weight of the cost functions that moved the most cost into
    /// `c_∅` at this node.
    ///
    /// This is the modification the whole heuristic turns on. Plain weighted
    /// degree counts constraints whose revision empties a domain, and in a
    /// branch and bound with a maintained bound almost nothing empties a
    /// domain: nodes die because the bound caught up with the incumbent. Left
    /// unattributed, every weight would stay at zero and the variable order
    /// would be smallest-domain-first with extra steps.
    fn blame_bound(&mut self) {
        let mut ranked: Vec<(usize, Cost)> = self
            .net
            .contributions()
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, moved)| *moved > Cost::BOT)
            .collect();
        ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        for (slot, _) in ranked.into_iter().take(BLAME_WIDTH) {
            if let Some(weight) = self.weights.get_mut(slot) {
                *weight = weight.saturating_add(1);
            }
        }
    }

    /// Raise the weight of every variable whose domain emptied.
    ///
    /// The classical case, kept because it is real even if it is rare here.
    fn blame_wipeout(&mut self) {
        for variable in self.net.variable_ids() {
            if !self.net.domain(variable).is_empty() {
                continue;
            }
            let slot = self.net.unary_slot(variable);
            if let Some(weight) = self.weights.get_mut(slot) {
                *weight = weight.saturating_add(1);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Nogoods
// ---------------------------------------------------------------------------

impl BranchAndBound<'_> {
    /// Extract every nogood the current branch carries, at the moment a dive
    /// gives up.
    ///
    /// The branch is a sequence of decisions, and every *negative* one on it
    /// marks a left subtree that was explored to exhaustion. The nogood for
    /// that subtree is the positive decisions preceding it together with the
    /// positive form of the decision itself: the negative decisions in between
    /// can be dropped, because each of them has its own nogood recorded here
    /// and any solution violating one would have been found in the subtree that
    /// nogood covers. This is the reduced form of Lecoutre, Saïs, Tabary and
    /// Vidal (IJCAI 2007).
    ///
    /// `closed` is the left branch the current node has just finished, which is
    /// not yet a negative decision on the branch but is closed all the same.
    fn record_restart_nogoods(&mut self, closed: Option<(VarId, ValId)>) {
        if !self.restarts || self.nogoods.len() >= MAX_NOGOODS {
            return;
        }
        let branch = self.path.clone();
        let mut positive: Vec<(VarId, ValId)> = Vec::new();
        for decision in &branch {
            if decision.assigns {
                positive.push((decision.variable, decision.value));
                continue;
            }
            let mut literals = positive.clone();
            literals.push((decision.variable, decision.value));
            self.record_nogood(literals);
        }
        if let Some((variable, value)) = closed {
            positive.push((variable, value));
            self.record_nogood(positive);
        }
    }

    /// Keep one nogood, watching two of its literals.
    fn record_nogood(&mut self, literals: Vec<(VarId, ValId)>) {
        if literals.is_empty() || self.nogoods.len() >= MAX_NOGOODS {
            return;
        }
        let index = self.nogoods.len();
        let second = usize::from(literals.len() > 1);
        for position in [0usize, second] {
            if let Some(&(var, val)) = literals.get(position) {
                self.watch
                    .entry((var.raw(), val.raw()))
                    .or_default()
                    .push(index);
            }
            if second == 0 {
                break;
            }
        }
        self.nogoods.push(Nogood {
            literals,
            watches: [0, second],
        });
    }

    /// What a literal says about the current domains.
    fn literal(&self, variable: VarId, value: ValId) -> Literal {
        let domain = self.net.domain(variable);
        if !domain.contains(value) {
            Literal::Dead
        } else if domain.len() == 1 {
            Literal::Held
        } else {
            Literal::Open
        }
    }

    /// Propagate every nogood watching the literal just decided.
    ///
    /// Returns whether the node survives.
    fn propagate_nogoods(&mut self, literal: (VarId, ValId)) -> bool {
        let key = (literal.0.raw(), literal.1.raw());
        let Some(watchers) = self.watch.get(&key).cloned() else {
            return true;
        };
        for index in watchers {
            if !self.visit_nogood(index, key) {
                return false;
            }
        }
        true
    }

    /// Move one nogood's watch off a literal that now holds, propagating or
    /// failing when there is nowhere to move it to.
    fn visit_nogood(&mut self, index: usize, key: (u32, u32)) -> bool {
        let Some(nogood) = self.nogoods.get(index) else {
            return true;
        };
        let literals = nogood.literals.clone();
        let watches = nogood.watches;
        let Some(here) = watches.iter().position(|slot| {
            literals.get(*slot).map(|(var, val)| (var.raw(), val.raw())) == Some(key)
        }) else {
            return true;
        };
        let other = watches[1 - here];

        for (position, &(var, val)) in literals.iter().enumerate() {
            if position == watches[here] || position == other {
                continue;
            }
            if self.literal(var, val) != Literal::Held {
                if let Some(nogood) = self.nogoods.get_mut(index) {
                    nogood.watches[here] = position;
                }
                self.watch
                    .entry((var.raw(), val.raw()))
                    .or_default()
                    .push(index);
                return true;
            }
        }

        let Some(&(var, val)) = literals.get(other) else {
            return true;
        };
        if watches[here] == other {
            // A nogood of one literal: it is violated the moment that literal
            // holds, which is what brought us here.
            return false;
        }
        match self.literal(var, val) {
            Literal::Held => false,
            Literal::Dead => true,
            Literal::Open => {
                self.net.refute(var, val);
                true
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

/// Solve a network by depth-first branch and bound.
///
/// The answer costs strictly less than `parameters.upper_bound`, or there is no
/// answer and the outcome says so.
///
/// # Examples
///
/// ```
/// use panproto_mig::solve::build::{NoEvidence, build_cfn};
/// use panproto_mig::solve::dfbb::{SearchParameters, solve_dfbb};
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
/// )?;
///
/// let outcome = solve_dfbb(&cfn, &SearchParameters::default());
///
/// // A schema against itself matches perfectly, so the search proves optimality
/// // and the assignment it hands back scores exactly what it reported.
/// assert!(outcome.proven_optimal);
/// let best = outcome.best.as_ref().expect("a schema always matches itself");
/// assert_eq!(cfn.evaluate(best), outcome.upper_bound);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use]
pub fn solve_dfbb(cfn: &Cfn, parameters: &SearchParameters) -> SolveOutcome {
    let millis = parameters.budget.max_millis;
    let mut search = BranchAndBound::new(cfn, parameters);
    search.run(millis)
}

/// The Luby sequence, one-based: `1, 1, 2, 1, 1, 2, 4, 1, …`.
///
/// The schedule with the optimal worst-case overhead for a search whose runtime
/// distribution is unknown, which is the situation here.
#[must_use]
pub const fn luby(index: u64) -> u64 {
    let mut remaining = if index == 0 { 1 } else { index };
    let mut power = 1u32;
    loop {
        if power >= 63 {
            return 1u64 << 62;
        }
        let span = (1u64 << power) - 1;
        if remaining == span {
            return 1u64 << (power - 1);
        }
        if remaining < span {
            remaining -= (1u64 << (power - 1)) - 1;
            power = 1;
        } else {
            power += 1;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::solve::cfn::CfnBuilder;
    use crate::solve::cost::DEFAULT_WEIGHTS;
    use crate::solve::oracle::brute_force;
    use panproto_gat::Name;

    const FIRST: VarId = VarId::new(0);
    const SECOND: VarId = VarId::new(1);

    fn cost(units: u64) -> Cost {
        Cost::from_raw(units)
    }

    /// Two variables over one target each, so the slots are `[t, ⊥]` on both
    /// and a binary table over them has four entries.
    fn pair() -> CfnBuilder {
        CfnBuilder::new(
            vec![
                (Name::new("u"), vec![Name::new("t")]),
                (Name::new("v"), vec![Name::new("t")]),
            ],
            DEFAULT_WEIGHTS,
        )
        .unwrap()
    }

    /// Three variables over one target each, with enough cost on the pairs
    /// that the search has to backtrack rather than dive straight to the
    /// answer.
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
        let third = VarId::new(2);
        builder.add_unary(FIRST, ValId::real(0), cost(2)).unwrap();
        builder.add_unary(SECOND, ValId::BOTTOM, cost(5)).unwrap();
        builder.add_unary(third, ValId::real(0), cost(1)).unwrap();
        builder
            .add_function(&[FIRST, SECOND], vec![cost(4), cost(1), cost(6), cost(3)])
            .unwrap();
        builder
            .add_function(&[SECOND, third], vec![cost(2), cost(7), cost(1), cost(9)])
            .unwrap();
        builder.build()
    }

    #[test]
    fn luby_is_the_published_sequence() {
        let sequence: Vec<u64> = (1..=15).map(luby).collect();
        assert_eq!(sequence, vec![1, 1, 2, 1, 1, 2, 4, 1, 1, 2, 1, 1, 2, 4, 8]);
    }

    #[test]
    fn the_search_finds_the_optimum_the_oracle_finds() {
        let mut builder = pair();
        builder.add_unary(FIRST, ValId::real(0), cost(1)).unwrap();
        builder.add_unary(SECOND, ValId::BOTTOM, cost(4)).unwrap();
        builder
            .add_function(&[FIRST, SECOND], vec![cost(2), cost(7), cost(9), cost(3)])
            .unwrap();
        let cfn = builder.build();

        let (expected, argmins) = brute_force(&cfn);
        let outcome = solve_dfbb(&cfn, &SearchParameters::default());

        assert!(outcome.proven_optimal);
        assert_eq!(outcome.upper_bound, expected);
        assert_eq!(outcome.lower_bound, expected);
        let best = outcome.best.unwrap();
        assert_eq!(cfn.evaluate(&best), expected);
        assert!(argmins.contains(&best));
    }

    #[test]
    fn a_bound_at_the_optimum_admits_no_solution() {
        let mut builder = pair();
        builder
            .add_function(&[FIRST, SECOND], vec![cost(2), cost(7), cost(9), cost(3)])
            .unwrap();
        let cfn = builder.build();
        let (optimum, _) = brute_force(&cfn);

        let at = solve_dfbb(&cfn, &SearchParameters::default().with_upper_bound(optimum));
        assert!(at.best.is_none(), "the search is for a strict improvement");
        assert!(at.proven_optimal, "and it proved there is none");
        assert_eq!(at.lower_bound, optimum);

        let above = solve_dfbb(
            &cfn,
            &SearchParameters::default().with_upper_bound(Cost::from_raw(optimum.raw() + 1)),
        );
        assert_eq!(above.upper_bound, optimum);
    }

    #[test]
    fn every_level_returns_the_same_optimum() {
        let mut builder = pair();
        builder.add_unary(FIRST, ValId::real(0), cost(3)).unwrap();
        builder
            .add_function(&[FIRST, SECOND], vec![cost(5), cost(1), cost(8), cost(6)])
            .unwrap();
        let cfn = builder.build();
        let (expected, _) = brute_force(&cfn);

        for level in ConsistencyLevel::ALL {
            let outcome = solve_dfbb(&cfn, &SearchParameters::default().with_level(level));
            assert_eq!(
                outcome.upper_bound,
                expected,
                "{} disagreed with the oracle",
                level.label()
            );
        }
    }

    /// The modification the variable order turns on: a node that dies because
    /// the bound caught up with the incumbent raises the weight of the cost
    /// functions that moved the cost, not merely of a variable whose domain
    /// emptied.
    #[test]
    fn the_weights_rise_on_a_bound_failure() {
        let mut builder = pair();
        builder
            .add_function(&[FIRST, SECOND], vec![cost(2), cost(2), cost(2), cost(2)])
            .unwrap();
        let cfn = builder.build();

        let parameters = SearchParameters::default().with_upper_bound(cost(2));
        let mut search = BranchAndBound::new(&cfn, &parameters);
        assert!(search.weights().iter().all(|weight| *weight == 0));

        assert!(
            search.prepare_root(None).is_none(),
            "every assignment costs the whole primal bound"
        );
        let functions = cfn.n_functions();
        assert!(
            search.weights()[..functions]
                .iter()
                .any(|weight| *weight > 0),
            "the cost function that raised the bound must carry weight"
        );
    }

    /// A nogood of one literal closes the branch that literal names.
    #[test]
    fn a_unit_nogood_closes_its_branch() {
        let mut builder = pair();
        builder
            .add_function(&[FIRST, SECOND], vec![cost(1), cost(2), cost(3), cost(4)])
            .unwrap();
        let cfn = builder.build();

        let mut search = BranchAndBound::new(&cfn, &SearchParameters::default());
        assert!(search.prepare_root(None).is_some());
        search.record_nogood(vec![(FIRST, ValId::real(0))]);

        search.net.assign(FIRST, ValId::real(0));
        assert!(
            !search.propagate(Some((FIRST, ValId::real(0)))),
            "the nogood must close the branch it names"
        );
    }

    /// A nogood of two literals removes the second value once the first holds.
    #[test]
    fn a_nogood_propagates_on_its_second_watched_literal() {
        let mut builder = pair();
        builder
            .add_function(&[FIRST, SECOND], vec![cost(1), cost(2), cost(3), cost(4)])
            .unwrap();
        let cfn = builder.build();

        let mut search = BranchAndBound::new(&cfn, &SearchParameters::default());
        assert!(search.prepare_root(None).is_some());
        search.record_nogood(vec![(FIRST, ValId::real(0)), (SECOND, ValId::real(0))]);

        search.net.assign(FIRST, ValId::real(0));
        assert!(search.propagate(Some((FIRST, ValId::real(0)))));
        assert!(
            !search.net.domain(SECOND).contains(ValId::real(0)),
            "the remaining literal must be refuted rather than left open"
        );
    }

    /// Restarting after a single backtrack drives the machinery that the
    /// shipped schedule never reaches on an instance this small, and the answer
    /// is unchanged.
    #[test]
    fn restarting_records_nogoods_and_still_proves_the_optimum() {
        let cfn = triple();
        let (optimum, argmins) = brute_force(&cfn);

        let parameters = SearchParameters::default().with_restart_scale(1);
        let mut search = BranchAndBound::new(&cfn, &parameters);
        let outcome = search.run(None);

        assert!(
            !search.nogoods.is_empty(),
            "a restart that recorded nothing has kept no work"
        );
        assert!(outcome.proven_optimal);
        assert_eq!(outcome.upper_bound, optimum);
        let best = outcome.best.unwrap();
        assert_eq!(cfn.evaluate(&best), optimum);
        assert!(argmins.contains(&best));
    }

    /// And the classical case still works: a domain that empties while the
    /// bound is still below the incumbent blames that variable.
    #[test]
    fn a_dive_abandoned_by_the_budget_leaves_its_subtree_on_the_frontier() {
        // The frontier's least bound is a lower bound only while the frontier
        // partitions the assignment space. A budget firing at the first node
        // abandons everything below it, so the node itself has to go back, and
        // an empty return would mean the caller reads "space exhausted" for a
        // space nothing looked at.
        let cfn = triple();
        let parameters = SearchParameters::default()
            .with_budget(SearchBudget::default().with_max_nodes(Some(0)));
        let mut search = BranchAndBound::new(&cfn, &parameters);
        let root = search.prepare_root(None).unwrap();

        let node = OpenNode::root(root);
        let children = search.explore(&node, 1);

        assert_eq!(search.limit_hit(), Some(LimitKind::Nodes));
        assert_eq!(
            children.len(),
            1,
            "the abandoned subtree is the node itself, and it goes back"
        );
        assert_eq!(children[0].decisions, node.decisions);
        assert_eq!(children[0].lower_bound, root);
    }

    #[test]
    fn an_interrupted_explore_never_reports_an_empty_frontier() {
        // The same claim at every depth the budget can fire at. Whenever a
        // limit fires inside `explore`, something is left unexplored, so the
        // frontier it returns cannot be empty: an empty one is the reading
        // that licenses raising the lower bound to the incumbent's cost.
        let cfn = triple();
        for nodes in 0u64..12 {
            let parameters = SearchParameters::default()
                .with_budget(SearchBudget::default().with_max_nodes(Some(nodes)));
            let mut search = BranchAndBound::new(&cfn, &parameters);
            let Some(root) = search.prepare_root(None) else {
                continue;
            };
            let children = search.explore(&OpenNode::root(root), 1);
            if search.limit_hit().is_some() {
                assert!(
                    !children.is_empty(),
                    "a limit fired at {nodes} nodes and the frontier came back empty"
                );
            }
        }
    }

    #[test]
    fn the_weights_rise_on_a_wipe_out() {
        let mut builder = pair();
        builder
            .add_function(
                &[FIRST, SECOND],
                vec![Cost::BOT, Cost::BOT, Cost::BOT, Cost::BOT],
            )
            .unwrap();
        let cfn = builder.build();

        // Node consistency alone, so that the bound stays where it is and the
        // empty domain is the only thing that closed the node.
        let parameters = SearchParameters::default()
            .with_upper_bound(cost(64))
            .with_level(ConsistencyLevel::Node);
        let mut search = BranchAndBound::new(&cfn, &parameters);
        search.net.refute(FIRST, ValId::real(0));
        search.net.refute(FIRST, ValId::BOTTOM);

        assert!(!search.propagate(None));
        let slot = search.net.unary_slot(FIRST);
        assert!(search.weights()[slot] > 0);
        assert_eq!(search.net.c_empty(), Cost::BOT, "the bound never moved");
    }
}
