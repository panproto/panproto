//! Soft local consistency: the equivalence preserving transformations, the five
//! levels a search node is filtered to, and the predicates that decide whether
//! each level actually holds.
//!
//! # What an equivalence preserving transformation is for
//!
//! [`Cfn`] states the objective as a sum of cost functions. Every one of those
//! functions reads `⊥` on some tuple, so a straight sum reads `⊥` as its own
//! lower bound and says nothing. An equivalence preserving transformation (EPT)
//! *moves* cost between the functions without changing what any assignment
//! costs: cost moved out of a binary table and into a unary table, then out of
//! the unary tables and into the constant `c_∅`, is cost that every assignment
//! pays, so `c_∅` becomes a lower bound that the search can prune against.
//!
//! The three EPTs are `Project`, `Extend` and `UnaryProject`, transcribed from
//! Algorithm 1 of Cooper, de Givry, Sánchez, Schiex, Zytnicki and Werner
//! (*Artificial Intelligence* 174(7–8), 2010). Each one preserves the cost of
//! **every** assignment, not merely the optimum, which is what makes `c_∅`
//! valid at every node rather than only at the root.
//!
//! # Why the saturation subterm is not an optimisation to remove
//!
//! Lines 10 and 15 of that algorithm read `(x ⊕ β) ⊖ β`, which is the identity
//! over the reals and is *not* the identity in the truncated structure `S(k)`
//! this crate works in. Its job is to detect that a tuple has become infeasible:
//! when `x ⊕ β` reaches `⊤` the subterm leaves `x` at `⊤`, and `⊤` is
//! irreversible. That detection is the whole mechanism by which accumulated
//! finite cost proves infeasibility and lets the search prune.
//!
//! The consequence is stated in the source and repeated here because it looks
//! like a bug when read quickly: **`Extend` and `UnaryProject` can change the
//! network even when `α = ⊥`**. With `⊤ = 10`, `c_i(a) = 5` and `c_∅ = 5`,
//! `UnaryProject(i, ⊥)` must leave `c_i(a) = ((5 ⊕ 5) ⊖ 5) ⊖ 0 = ⊤`. An
//! `if α == ⊥ { return }` fast path silently disables that, and the search then
//! keeps values it should have pruned. There is a regression test named for
//! this.
//!
//! # The two termination preconditions
//!
//! 1. **No two cost functions share a scope.** With two functions on one scope
//!    a unary cost has a choice of which one to extend into, nothing records
//!    the choice, and a wrong choice breaks directional consistency without
//!    raising `c_∅`, so enforcement oscillates (Lee and Leung, *JAIR* 44, 2012,
//!    §4.4.1). [`CfnBuilder`](super::cfn::CfnBuilder) merges duplicate scopes
//!    by pointwise `⊕` at construction, so no [`Cfn`] can state the hazard.
//! 2. **Costs are integers.** The published termination bound for the strongest
//!    level counts the number of times `c_∅` can rise, and that count is finite
//!    only because each rise is at least one unit. [`Cost`] is an integer for
//!    this reason and not for speed.
//!
//! Neither precondition is checkable from inside a loop, so a step budget backs
//! them up: [`Network::budget_exhausted`] reports that enforcement stopped
//! early. Stopping early is sound, because any prefix of an EPT sequence is
//! itself an EPT sequence and `c_∅` is a valid bound after it; it is merely
//! weaker. Tests treat an exhausted budget as a failure.
//!
//! The ceiling is sized from the complexity of *one* enforcement, so it is
//! charged per enforcement: one [`Network`] lives for a whole search, and
//! charging every node of that search against a single-enforcement allowance
//! would spend it within the first dive and silently degrade the level for
//! every node after. What the budget catches is one enforcement failing to
//! reach a fixpoint, which is what a broken precondition looks like.
//!
//! # Arity
//!
//! Node consistency and arc consistency are defined here for any arity: arc
//! consistency is the generalized form, whose two clauses are the saturation
//! above and the existence of a `⊥`-valued tuple through every value. The
//! directional and existential levels are defined in the literature for binary
//! cost functions and are implemented and checked over the binary cost
//! functions alone. Functions of arity three or more take part in the arc level
//! only. That is sound at every level, since every level is a sequence of EPTs;
//! it only means a wider function contributes less to the bound.

use std::hash::Hasher;

use rustc_hash::FxHasher;
use smallvec::SmallVec;

use super::cfn::{Cfn, Domain, Domains};
use super::cost::Cost;
use super::{ValId, VarId};

// ---------------------------------------------------------------------------
// Levels
// ---------------------------------------------------------------------------

/// How hard a node is filtered before its lower bound is read.
///
/// The levels are **not** totally ordered by strength, which is why this type
/// carries no `Ord`. Arc consistency and directional arc consistency are
/// incomparable: each has a witness the other does not detect. What does hold
/// is `NC* ⪯ AC* ⪯ FDAC* ⪯ EDAC*` and `NC* ⪯ DAC* ⪯ FDAC*`, and the
/// `c_∅ ordering` test pins that chain.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum ConsistencyLevel {
    /// Node consistency: every value is feasible against `c_∅`, and every
    /// variable has a value costing `⊥`.
    ///
    /// The second clause is what makes `c_∅` *the* bound rather than one term
    /// of it: without it the tighter bound is `c_∅ ⊕ ⨁_i min_a c_i(a)` and the
    /// search would have to compute the sum at every node.
    Node,

    /// Node consistency together with a simple support for every value in every
    /// cost function.
    Arc,

    /// Node consistency together with a full support for every value in every
    /// binary cost function pointing at a higher variable.
    DirectionalArc,

    /// Arc consistency and directional arc consistency together.
    FullDirectionalArc,

    /// Full directional arc consistency together with existential arc
    /// consistency: some value of every variable costs `⊥` and is fully
    /// supported in every direction.
    ///
    /// The default. It is the strongest level reachable with integer weights at
    /// the arc level: full arc consistency is provably unattainable, and the
    /// virtual and optimal levels above it circulate fractional weights, which
    /// costs the termination argument above.
    #[default]
    ExistentialDirectionalArc,
}

impl ConsistencyLevel {
    /// Every level, weakest first along the chain that is ordered.
    pub const ALL: [Self; 5] = [
        Self::Node,
        Self::Arc,
        Self::DirectionalArc,
        Self::FullDirectionalArc,
        Self::ExistentialDirectionalArc,
    ];

    /// A short name, for diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Node => "NC*",
            Self::Arc => "AC*",
            Self::DirectionalArc => "DAC*",
            Self::FullDirectionalArc => "FDAC*",
            Self::ExistentialDirectionalArc => "EDAC*",
        }
    }
}

// ---------------------------------------------------------------------------
// The trail
// ---------------------------------------------------------------------------

/// A position in the cost trail, to restore back to.
///
/// Opaque so that a mark from one network cannot be handed to another.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrailMark(usize);

// ---------------------------------------------------------------------------
// The network
// ---------------------------------------------------------------------------

/// A mutable working copy of a [`Cfn`], with a trail.
///
/// [`Cfn`] is immutable, and deliberately so: it is the pristine scorer a
/// solver's answer is certified against. This is the copy the EPTs mutate.
///
/// # Layout
///
/// Every mutable cost lives in one flat vector: `c_∅` first, then one block per
/// variable holding its unary table, then one block per cost function holding
/// its table. A cost cell is therefore named by a single index, which is what
/// makes the trail entry a pair of machine words and the restore a reverse
/// walk with no bookkeeping.
///
/// # What is trailed and what is not
///
/// Cost cells are trailed, because they are what the EPTs accumulate and
/// undoing them one at a time is far cheaper than rebuilding them. Domains are
/// **not**: [`Domains`] is one contiguous bit set, so a search copies the whole
/// store on branching and restores it with one copy. Mixing the two is
/// deliberate and the sizes are the reason: at the measured widths a
/// save-and-restore pair is five orders of magnitude below the enforcement it
/// brackets.
#[derive(Clone, Debug)]
pub struct Network {
    /// Table slots per variable, which is its real value count plus one for `⊥`.
    slots: Vec<usize>,

    /// The values each variable may still take.
    domains: Domains,

    /// The pristine cost cells, for [`Self::reset`].
    origin_cells: Vec<Cost>,

    /// The pristine domains, for [`Self::reset`].
    origin_domains: Domains,

    /// Every mutable cost, in the layout the type docs describe.
    cells: Vec<Cost>,

    /// Where each variable's unary block starts in [`Self::cells`].
    unary_at: Vec<usize>,

    /// The scope of each cost function, strictly ascending.
    scopes: Vec<Vec<VarId>>,

    /// Where each cost function's table starts in [`Self::cells`].
    table_at: Vec<usize>,

    /// The row-major stride of each scope position.
    strides: Vec<Vec<usize>>,

    /// The cost functions incident on each variable.
    incident: Vec<Vec<usize>>,

    /// For each variable, the other endpoint and function index of every binary
    /// cost function it takes part in.
    binary: Vec<Vec<(VarId, usize)>>,

    /// `⊤`, the moving primal bound.
    top: Cost,

    /// `(cell index, value before the write)`, newest last.
    trail: Vec<(usize, Cost)>,

    /// Cost moved out of each weight slot since the last reset.
    contributions: Vec<Cost>,

    /// Elementary operations performed since construction.
    steps: u64,

    /// [`Self::steps`] as the current enforcement found it.
    ///
    /// The budget bounds one enforcement, so what it is checked against is the
    /// operations this enforcement has performed, which is the difference
    /// between the two. One network lives for a whole search, so charging every
    /// node against one ceiling would spend a single enforcement's allowance
    /// across hundreds of them and silently drop the level being paid for.
    enforce_base: u64,

    /// The ceiling one enforcement's operations are checked against.
    step_budget: u64,

    /// Whether any enforcement ever stopped because the budget ran out.
    exhausted: bool,
}

impl Network {
    /// Take a working copy of a network, with an explicit `⊤`.
    ///
    /// `top` is the primal bound the search is looking to beat, so passing
    /// [`Cost::TOP_SENTINEL`] means "no bound yet".
    #[must_use]
    pub fn from_cfn(cfn: &Cfn, top: Cost) -> Self {
        let count = cfn.n_variables();
        let mut slots = Vec::with_capacity(count);
        let domains = cfn.domains().clone();
        let mut unary_at = Vec::with_capacity(count);
        let mut cells = vec![cfn.c_empty()];

        for (var, variable) in cfn.variable_ids().zip(cfn.variables()) {
            slots.push(variable.slots());
            unary_at.push(cells.len());
            let table = cfn.unary(var).unwrap_or(&[]);
            debug_assert_eq!(
                table.len(),
                variable.slots(),
                "a unary table spans its slots"
            );
            cells.extend_from_slice(table);
        }

        let mut scopes = Vec::with_capacity(cfn.n_functions());
        let mut table_at = Vec::with_capacity(cfn.n_functions());
        let mut strides = Vec::with_capacity(cfn.n_functions());
        let mut incident = vec![Vec::new(); count];
        let mut binary = vec![Vec::new(); count];

        for (index, function) in cfn.functions().iter().enumerate() {
            let scope = function.scope().to_vec();
            table_at.push(cells.len());
            cells.extend_from_slice(function.table());
            strides.push(scope_strides(&slots, &scope));
            for (position, var) in scope.iter().enumerate() {
                if let Some(list) = incident.get_mut(var.index()) {
                    list.push(index);
                }
                if scope.len() == 2 {
                    let other = scope[1 - position];
                    if let Some(list) = binary.get_mut(var.index()) {
                        list.push((other, index));
                    }
                }
            }
            scopes.push(scope);
        }

        let weight_slots = cfn.n_functions() + count;
        let budget = default_step_budget(count, cfn.n_functions(), cfn.max_domain());
        Self {
            slots,
            origin_cells: cells.clone(),
            origin_domains: domains.clone(),
            domains,
            cells,
            unary_at,
            scopes,
            table_at,
            strides,
            incident,
            binary,
            top,
            trail: Vec::new(),
            contributions: vec![Cost::BOT; weight_slots],
            steps: 0,
            enforce_base: 0,
            step_budget: budget,
            exhausted: false,
        }
    }

    /// Put the network back the way [`Self::from_cfn`] left it, with a new `⊤`.
    ///
    /// This is how a best-first search returns to the root before replaying a
    /// decision sequence: two vector copies rather than a rebuild of every
    /// table.
    pub fn reset(&mut self, top: Cost) {
        self.cells.copy_from_slice(&self.origin_cells);
        self.domains.copy_from(&self.origin_domains);
        self.trail.clear();
        self.reset_contributions();
        self.top = top;
    }

    // -- shape -------------------------------------------------------------

    /// How many variables the network has.
    #[inline]
    #[must_use]
    pub fn n_variables(&self) -> usize {
        self.slots.len()
    }

    /// How many cost functions of arity two or more the network has.
    #[inline]
    #[must_use]
    pub fn n_functions(&self) -> usize {
        self.scopes.len()
    }

    /// Every variable identifier, ascending.
    #[inline]
    pub fn variable_ids(&self) -> impl Iterator<Item = VarId> + '_ {
        (0..self.slots.len()).filter_map(|index| u32::try_from(index).ok().map(VarId::new))
    }

    /// The scope of one cost function, or an empty slice if there is no such
    /// function.
    #[inline]
    #[must_use]
    pub fn scope(&self, function: usize) -> &[VarId] {
        self.scopes.get(function).map_or(&[], Vec::as_slice)
    }

    /// The cost functions incident on one variable.
    #[inline]
    #[must_use]
    pub fn incident(&self, var: VarId) -> &[usize] {
        self.incident.get(var.index()).map_or(&[], Vec::as_slice)
    }

    /// The other endpoint and function index of every binary cost function one
    /// variable takes part in.
    #[inline]
    #[must_use]
    pub fn binary_neighbours(&self, var: VarId) -> &[(VarId, usize)] {
        self.binary.get(var.index()).map_or(&[], Vec::as_slice)
    }

    // -- costs -------------------------------------------------------------

    /// `⊤`, the primal bound every operation is taken against.
    #[inline]
    #[must_use]
    pub const fn top(&self) -> Cost {
        self.top
    }

    /// Move `⊤` to a new primal bound.
    ///
    /// Lowering it is what turns an improving solution into pruning power: a
    /// cost recorded under the old bound that now reads at or above the new one
    /// is `⊤`, hence infeasible, hence prunable.
    #[inline]
    pub const fn set_top(&mut self, top: Cost) {
        self.top = top;
    }

    /// Whether a cost counts as `⊤` under the current bound.
    #[inline]
    #[must_use]
    pub const fn is_top(&self, cost: Cost) -> bool {
        cost.raw() >= self.top.raw()
    }

    /// The constant term, which is the certified lower bound once a level has
    /// been enforced.
    #[inline]
    #[must_use]
    pub fn c_empty(&self) -> Cost {
        self.cells.first().copied().unwrap_or(Cost::BOT)
    }

    /// The unary cost of one value of one variable, or `⊤` if there is no such
    /// cell.
    #[inline]
    #[must_use]
    pub fn unary_cost(&self, var: VarId, value: ValId) -> Cost {
        self.unary_cell(var, value)
            .and_then(|cell| self.cells.get(cell).copied())
            .unwrap_or(self.top)
    }

    /// The entry of a cost function at one tuple, positional against its scope,
    /// or `⊤` if the tuple names a value the scope cannot take.
    #[must_use]
    pub fn function_cost(&self, function: usize, tuple: &[ValId]) -> Cost {
        self.tuple_cell(function, tuple)
            .and_then(|cell| self.cells.get(cell).copied())
            .unwrap_or(self.top)
    }

    /// The cost this network gives a total assignment.
    ///
    /// `c_∅ ⊕ ⨁_i c_i(a_i) ⊕ ⨁_S c_S(t[S])`, taken against the network's own
    /// `⊤`. Two networks related by a sequence of equivalence preserving
    /// transformations agree on this for **every** assignment, not merely at
    /// the optimum, and that is what makes the bound valid at every node.
    ///
    /// This is not a substitute for [`Cfn::evaluate`]: it reads transformed
    /// state, so it can only restate what a solver already believes. An answer
    /// is certified against the pristine network, never against this.
    #[must_use]
    pub fn valuation(&self, values: &[ValId]) -> Cost {
        let top = self.top;
        let mut total = self.c_empty();
        for (var, value) in self.variable_ids().zip(values) {
            total = total.combine(self.unary_cost(var, *value), top);
        }
        for function in 0..self.n_functions() {
            let mut tuple = Vec::with_capacity(self.scope(function).len());
            for var in self.scope(function) {
                let Some(value) = values.get(var.index()) else {
                    return top;
                };
                tuple.push(*value);
            }
            total = total.combine(self.function_cost(function, &tuple), top);
        }
        total
    }

    // -- domains -----------------------------------------------------------

    /// The values one variable may still take.
    #[inline]
    #[must_use]
    pub fn domain(&self, var: VarId) -> Domain<'_> {
        self.domains.get(var)
    }

    /// Every domain, in one contiguous bit set.
    #[inline]
    #[must_use]
    pub const fn domains(&self) -> &Domains {
        &self.domains
    }

    /// Put every domain back to a saved copy.
    ///
    /// The counterpart of copy-on-branch. A store of the wrong shape is
    /// ignored rather than partially applied, since a partial restore is a
    /// silently wrong network.
    pub fn set_domains(&mut self, domains: &Domains) {
        self.domains.copy_from(domains);
    }

    /// Reduce a variable's domain to one value.
    pub fn assign(&mut self, var: VarId, value: ValId) {
        self.domains.assign(var, value);
    }

    /// Take one value out of a variable's domain.
    pub fn refute(&mut self, var: VarId, value: ValId) {
        self.domains.remove(var, value);
    }

    /// A detached copy of one variable's domain words.
    ///
    /// A [`Domain`] borrows the store, so a step that walks a domain while
    /// writing to the network cannot hold one. Those steps take this instead: at
    /// the one-word width the whole corpus sits at, it is the same eight-byte
    /// copy the borrowed view replaced, made on the stack.
    #[inline]
    #[must_use]
    fn domain_words(&self, var: VarId) -> SmallVec<u64, 2> {
        SmallVec::from_slice_copy(self.domains.block(var))
    }

    /// Whether a variable has exactly one value left.
    #[inline]
    #[must_use]
    pub fn is_assigned(&self, var: VarId) -> bool {
        self.domain(var).len() == 1
    }

    /// Whether the network still admits an assignment costing less than `⊤`.
    ///
    /// False when some domain is empty or when the bound has already reached
    /// the primal bound.
    #[must_use]
    pub fn feasible(&self) -> bool {
        !self.is_top(self.c_empty()) && !self.domains.any_empty()
    }

    // -- trail -------------------------------------------------------------

    /// Where the trail stands, to restore back to.
    #[inline]
    #[must_use]
    pub fn mark(&self) -> TrailMark {
        TrailMark(self.trail.len())
    }

    /// Undo every cost write since a mark.
    ///
    /// Domains are not touched: they are restored by copy, per the type docs.
    pub fn restore(&mut self, mark: TrailMark) {
        while self.trail.len() > mark.0 {
            let Some((index, old)) = self.trail.pop() else {
                return;
            };
            if let Some(cell) = self.cells.get_mut(index) {
                *cell = old;
            }
        }
    }

    /// A hash of every cost cell and every domain.
    ///
    /// The equality test the trail round trip is checked with. Two networks
    /// with the same hash are the same network up to a collision, and the test
    /// compares cell vectors as well, so the hash is a summary rather than the
    /// evidence.
    #[must_use]
    pub fn structural_hash(&self) -> u64 {
        let mut hasher = FxHasher::default();
        for cell in &self.cells {
            hasher.write_u64(cell.raw());
        }
        for word in self.domains.bits() {
            hasher.write_u64(*word);
        }
        hasher.finish()
    }

    /// Every cost cell, in the layout the type docs describe.
    ///
    /// Exposed so a test can compare two states entry by entry rather than
    /// through [`Self::structural_hash`] alone.
    #[inline]
    #[must_use]
    pub fn cells(&self) -> &[Cost] {
        &self.cells
    }

    // -- accounting --------------------------------------------------------

    /// How many weight slots the network has: one per cost function, then one
    /// per variable for its unary table.
    #[inline]
    #[must_use]
    pub fn weight_slots(&self) -> usize {
        self.contributions.len()
    }

    /// The weight slot of one cost function.
    #[inline]
    #[must_use]
    pub const fn function_slot(function: usize) -> usize {
        function
    }

    /// The weight slot of one variable's unary table.
    #[inline]
    #[must_use]
    pub fn unary_slot(&self, var: VarId) -> usize {
        self.scopes.len() + var.index()
    }

    /// How much cost each weight slot has given up since the last reset.
    ///
    /// This is the attribution a branch and bound uses when a node dies from
    /// the bound rather than from an empty domain: the slots that moved the
    /// most cost toward `c_∅` are the ones that did the pruning.
    #[inline]
    #[must_use]
    pub fn contributions(&self) -> &[Cost] {
        &self.contributions
    }

    /// Forget the contribution tally, at the start of a node.
    pub fn reset_contributions(&mut self) {
        self.contributions.fill(Cost::BOT);
    }

    /// Elementary operations performed since construction.
    #[inline]
    #[must_use]
    pub const fn steps(&self) -> u64 {
        self.steps
    }

    /// Whether any enforcement ever stopped because the step budget ran out.
    ///
    /// Sound but weaker: the bound after a truncated EPT sequence is still
    /// valid. It is a failure in tests, because reaching it means one of the
    /// two termination preconditions has been broken.
    ///
    /// The flag latches for the life of the network and [`Self::reset`] does
    /// not clear it, since the question it answers is whether the preconditions
    /// ever failed rather than whether they are failing now.
    #[inline]
    #[must_use]
    pub const fn budget_exhausted(&self) -> bool {
        self.exhausted
    }

    /// The ceiling one enforcement's operations are checked against.
    ///
    /// Not a ceiling on [`Self::steps`], which counts the whole life of the
    /// network: the bound this is sized from is the cost of reaching one
    /// fixpoint, and it is charged against the operations of one enforcement.
    #[inline]
    #[must_use]
    pub const fn step_budget(&self) -> u64 {
        self.step_budget
    }

    // -- equivalence preserving transformations ----------------------------

    /// `Project(S, i, a, α)`: move `α` out of a cost function and into a unary
    /// cost.
    ///
    /// ```text
    /// c_i(a) ← c_i(a) ⊕ α
    /// foreach t ∈ ℓ(S) with t_i = a:  c_S(t) ← c_S(t) ⊖ α
    /// ```
    ///
    /// # Panics
    ///
    /// If `α` exceeds `min{ c_S(t) : t ∈ ℓ(S), t_i = a }`, by way of
    /// [`Cost::diff`]. That precondition is the hypothesis of the difference
    /// lemma the equivalence proof rests on, so violating it does not merely
    /// produce a worse bound: it produces a network whose assignments no longer
    /// cost what they did.
    pub fn project(&mut self, function: usize, var: VarId, value: ValId, alpha: Cost) {
        debug_assert!(
            alpha <= self.min_over_tuples(function, var, value),
            "Project needs α below every tuple it takes from"
        );
        let top = self.top;
        if let Some(cell) = self.unary_cell(var, value) {
            let raised = self.cell(cell).combine(alpha, top);
            self.write(cell, raised);
        }
        let mut targets = Vec::new();
        self.walk_tuples(function, Some((var, value)), |index, _| targets.push(index));
        for index in &targets {
            let lowered = self.cell(*index).diff(alpha, top);
            self.write(*index, lowered);
        }
        self.contribute(Self::function_slot(function), alpha);
        let charged = u64::try_from(targets.len()).unwrap_or(u64::MAX);
        self.step(charged.saturating_add(1));
    }

    /// `Extend(i, a, S, α)`: move `α` out of a unary cost and into a cost
    /// function, saturating tuples that have become infeasible on the way.
    ///
    /// ```text
    /// foreach t ∈ ℓ(S) with t_i = a:
    ///     β ← c_∅ ⊕ ( ⨁_{j ∈ S} c_j(t_j) )
    ///     c_S(t) ← ((c_S(t) ⊕ β) ⊖ β) ⊕ α
    /// c_i(a) ← c_i(a) ⊖ α
    /// ```
    ///
    /// The `(x ⊕ β) ⊖ β` subterm is the saturation the module docs describe,
    /// and it is why this is called with `α = ⊥` on purpose.
    ///
    /// # Panics
    ///
    /// If `α` exceeds `c_i(a)`, by way of [`Cost::diff`].
    pub fn extend(&mut self, var: VarId, value: ValId, function: usize, alpha: Cost) {
        debug_assert!(
            self.scope(function).len() > 1,
            "Extend needs a cost function of arity two or more"
        );
        debug_assert!(
            alpha <= self.unary_cost(var, value),
            "Extend needs α below the unary cost it takes from"
        );
        let top = self.top;
        // Read at `⊤` for the same reason [`Self::unary_project`] does. Here the
        // first `⊕` of the loop below would clamp `β` anyway, so this makes
        // `β ⪯ ⊤` hold unconditionally rather than by way of the scope being
        // non-empty.
        let constant = self.c_empty().min(top);
        let scope = self.scope(function).to_vec();
        // Every `β` is read from unary costs, and this transform writes no
        // unary cost until the loop over tuples is over, so gathering them all
        // first is the same computation in a different order.
        let mut targets: Vec<(usize, Cost)> = Vec::new();
        self.walk_tuples(function, Some((var, value)), |index, tuple| {
            let mut beta = constant;
            for (other, item) in scope.iter().zip(tuple) {
                beta = beta.combine(self.unary_cost(*other, *item), top);
            }
            targets.push((index, beta));
        });
        for &(index, beta) in &targets {
            let saturated = self.cell(index).combine(beta, top).diff(beta, top);
            self.write(index, saturated.combine(alpha, top));
        }
        if let Some(cell) = self.unary_cell(var, value) {
            let lowered = self.cell(cell).diff(alpha, top);
            self.write(cell, lowered);
        }
        let charged = u64::try_from(targets.len()).unwrap_or(u64::MAX);
        self.step(charged.saturating_add(1));
    }

    /// `UnaryProject(i, α)`: move `α` out of every unary cost of one variable
    /// and into `c_∅`.
    ///
    /// ```text
    /// foreach a ∈ d_i:  c_i(a) ← ((c_i(a) ⊕ c_∅) ⊖ c_∅) ⊖ α
    /// c_∅ ← c_∅ ⊕ α
    /// ```
    ///
    /// The two orderings matter and are implemented literally: the loop reads
    /// the *old* `c_∅`, and the saturation happens before the subtraction.
    ///
    /// `c_∅` is read at `⊤`, the same clamp the least-unary-cost scan applies and
    /// the same reading `S(k)` demands, since `c_∅ ∈ [0..k]`. A caller may set the
    /// primal bound below `c_∅` (`⊤` is the moving bound, and `Cfn::c_empty`
    /// carries the vacuous objective components, so `c_∅ ≻ ⊥` is the ordinary
    /// case), and an unclamped read would then ask `(x ⊕ c_∅) ⊖ c_∅` for a
    /// difference no cell can pay: the `⊕` saturates at `⊤ ≺ c_∅` and
    /// [`Cost::diff`]'s fairness precondition fires. Clamped, such a node
    /// projects every unary cost to `⊤` and leaves `c_∅` at `⊤`, which is what
    /// [`Self::feasible`] reads as dead, which is what it is.
    ///
    /// # Panics
    ///
    /// If `α` exceeds `min{ c_i(a) : a ∈ d_i }`, by way of [`Cost::diff`].
    pub fn unary_project(&mut self, var: VarId, alpha: Cost) {
        debug_assert!(
            self.min_unary(var).is_none_or(|least| alpha <= least),
            "UnaryProject needs α below every unary cost it takes from"
        );
        let top = self.top;
        let constant = self.c_empty().min(top);
        let snapshot = self.domain_words(var);
        let domain = Domain::new(&snapshot);
        for value in domain {
            let Some(cell) = self.unary_cell(var, value) else {
                continue;
            };
            let saturated = self.cell(cell).combine(constant, top).diff(constant, top);
            let lowered = saturated.diff(alpha, top);
            self.write(cell, lowered);
        }
        let visited = u64::try_from(domain.len()).unwrap_or(u64::MAX);
        self.write(0, constant.combine(alpha, top));
        self.contribute(self.unary_slot(var), alpha);
        self.step(visited + 1);
    }

    // -- enforcers ---------------------------------------------------------

    /// Enforce one level, returning whether the network is still feasible.
    ///
    /// A `false` return means the node is dead: some domain emptied, or the
    /// bound reached `⊤`.
    pub fn enforce(&mut self, level: ConsistencyLevel) -> bool {
        match level {
            ConsistencyLevel::Node => self.enforce_nc_star(),
            ConsistencyLevel::Arc => self.enforce_ac_star(),
            ConsistencyLevel::DirectionalArc => self.enforce_dac_star(),
            ConsistencyLevel::FullDirectionalArc => self.enforce_fdac_star(),
            ConsistencyLevel::ExistentialDirectionalArc => self.enforce_edac_star(),
        }
    }

    /// Enforce node consistency.
    ///
    /// Projecting every unary table into `c_∅` and then pruning is one sweep,
    /// but pruning can raise the bound and a raised bound can make further
    /// values infeasible, so the sweep repeats until nothing moves.
    pub fn enforce_nc_star(&mut self) -> bool {
        self.begin_enforcement();
        self.node_loop();
        self.feasible()
    }

    /// Enforce arc consistency, in its generalized any-arity form.
    pub fn enforce_ac_star(&mut self) -> bool {
        self.begin_enforcement();
        self.node_loop();
        self.arc_loop();
        self.feasible()
    }

    /// Enforce directional arc consistency against the ascending variable
    /// order.
    pub fn enforce_dac_star(&mut self) -> bool {
        self.begin_enforcement();
        self.node_loop();
        self.directional_loop();
        self.feasible()
    }

    /// Enforce arc consistency and directional arc consistency together.
    ///
    /// The sequence begins with exactly what [`Self::enforce_ac_star`] runs,
    /// which is what puts this level's bound at or above that one's rather than
    /// merely beside it. Local consistency closures are **not** unique, so a
    /// stronger property does not on its own imply a larger bound; the shared
    /// prefix plus the fact that no transformation ever lowers `c_∅` does. The
    /// same reasoning does not order this level against directional consistency
    /// alone, and the two are not compared.
    pub fn enforce_fdac_star(&mut self) -> bool {
        self.begin_enforcement();
        self.node_loop();
        self.arc_loop();
        self.full_directional_loop();
        self.feasible()
    }

    /// Enforce existential directional arc consistency.
    ///
    /// The queue algorithm is Algorithm 3 of de Givry, Heras, Zytnicki and
    /// Larrosa (IJCAI 2005). It runs after exactly
    /// the sequence [`Self::enforce_fdac_star`] runs, for the ordering reason
    /// given there, and the weaker sweeps repeat alongside it because the
    /// queues are driven by unary costs rising off `⊥`, which is a narrower
    /// signal than "something changed": draining them is not by itself a
    /// fixpoint of the weaker levels.
    pub fn enforce_edac_star(&mut self) -> bool {
        self.begin_enforcement();
        self.node_loop();
        self.arc_loop();
        self.full_directional_loop();
        self.existential_loop();
        self.feasible()
    }

    // -- predicates --------------------------------------------------------

    /// Whether node consistency holds.
    ///
    /// `∀i ∀a ∈ d_i : c_∅ ⊕ c_i(a) ≺ ⊤`, and `∀i ∃a ∈ d_i : c_i(a) = ⊥`.
    #[must_use]
    pub fn is_nc_star(&self) -> bool {
        let constant = self.c_empty();
        for var in self.variable_ids() {
            let domain = self.domain(var);
            if domain.is_empty() {
                return false;
            }
            let mut has_free_value = false;
            for value in domain {
                let unary = self.unary_cost(var, value);
                if self.is_top(constant.combine(unary, self.top)) {
                    return false;
                }
                if unary == Cost::BOT {
                    has_free_value = true;
                }
            }
            if !has_free_value {
                return false;
            }
        }
        true
    }

    /// Whether arc consistency holds, in its generalized any-arity form.
    ///
    /// Node consistency, plus for every cost function: no tuple is feasible
    /// against the unary costs and the constant while its own entry is below
    /// `⊤`, and every value of every scope variable lies on some `⊥`-valued
    /// tuple.
    #[must_use]
    pub fn is_ac_star(&self) -> bool {
        if !self.is_nc_star() {
            return false;
        }
        for function in 0..self.n_functions() {
            if !self.function_is_saturated(function) {
                return false;
            }
            for var in self.scope(function).to_vec() {
                for value in self.domain(var) {
                    if self.min_over_tuples(function, var, value) != Cost::BOT {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Whether directional arc consistency holds against the ascending variable
    /// order.
    ///
    /// Node consistency, plus a full support toward the higher endpoint of
    /// every binary cost function: `∀ c_ij, i < j, ∀a ∈ d_i, ∃b ∈ d_j` with
    /// `c_ij(a,b) = c_j(b) = ⊥`.
    #[must_use]
    pub fn is_dac_star(&self) -> bool {
        if !self.is_nc_star() {
            return false;
        }
        for function in 0..self.n_functions() {
            let scope = self.scope(function);
            let (Some(&low), Some(&high)) = (scope.first(), scope.get(1)) else {
                continue;
            };
            if scope.len() != 2 {
                continue;
            }
            if !self.has_full_supports(function, low, high) {
                return false;
            }
        }
        true
    }

    /// Whether arc consistency and directional arc consistency both hold.
    #[must_use]
    pub fn is_fdac_star(&self) -> bool {
        self.is_ac_star() && self.is_dac_star()
    }

    /// Whether existential arc consistency holds.
    ///
    /// Node consistency, plus for every variable a value costing `⊥` that has a
    /// full support in **every** binary cost function it takes part in, in both
    /// directions. The enforcing algorithm only looks toward lower neighbours,
    /// because directional consistency already covers the higher ones; this
    /// predicate does not take that shortcut, which is the point of writing it
    /// from the definition.
    #[must_use]
    pub fn is_eac_star(&self) -> bool {
        if !self.is_nc_star() {
            return false;
        }
        for var in self.variable_ids() {
            if !self.has_existential_support(var) {
                return false;
            }
        }
        true
    }

    /// Whether existential directional arc consistency holds.
    #[must_use]
    pub fn is_edac_star(&self) -> bool {
        self.is_fdac_star() && self.is_eac_star()
    }

    // -- search support ----------------------------------------------------

    /// The existential support value of a variable, if it has one.
    ///
    /// The cheapest value once the cheapest full support in every binary cost
    /// function is counted, which is the value the enforced network says is
    /// most promising. Ties go to the smaller identifier, so `⊥` loses every
    /// tie it is in.
    #[must_use]
    pub fn eac_support(&self, var: VarId) -> Option<ValId> {
        let mut best: Option<(ValId, Cost)> = None;
        for value in self.domain(var) {
            let score = self.existential_score(var, value, false);
            if best.is_none_or(|(_, cost)| score < cost) {
                best = Some((value, score));
            }
        }
        best.map(|(value, _)| value)
    }

    /// The value of a variable with the least unary cost, ties to the smaller
    /// identifier.
    #[must_use]
    pub fn cheapest_value(&self, var: VarId) -> Option<ValId> {
        let mut best: Option<(ValId, Cost)> = None;
        for value in self.domain(var) {
            let cost = self.unary_cost(var, value);
            if best.is_none_or(|(_, least)| cost < least) {
                best = Some((value, cost));
            }
        }
        best.map(|(value, _)| value)
    }
}

// ---------------------------------------------------------------------------
// Internals: cells and tuples
// ---------------------------------------------------------------------------

impl Network {
    /// One cost cell, or `⊥` if the index is out of range.
    #[inline]
    fn cell(&self, index: usize) -> Cost {
        self.cells.get(index).copied().unwrap_or(Cost::BOT)
    }

    /// Write a cost cell, trailing the old value.
    ///
    /// A write that changes nothing is not trailed, which keeps the trail
    /// length an honest measure of whether a sweep moved anything.
    #[inline]
    fn write(&mut self, index: usize, value: Cost) {
        if let Some(cell) = self.cells.get_mut(index) {
            if *cell != value {
                self.trail.push((index, *cell));
                *cell = value;
            }
        }
    }

    /// Record that a weight slot gave up cost.
    #[inline]
    fn contribute(&mut self, slot: usize, amount: Cost) {
        if amount == Cost::BOT {
            return;
        }
        if let Some(total) = self.contributions.get_mut(slot) {
            *total = total.combine(amount, Cost::TOP_SENTINEL);
        }
    }

    /// Charge elementary operations against the budget.
    #[inline]
    const fn step(&mut self, count: u64) {
        self.steps = self.steps.saturating_add(count);
    }

    /// Whether this enforcement's step budget has run out, latching the flag
    /// when it has.
    const fn out_of_steps(&mut self) -> bool {
        if self.steps.saturating_sub(self.enforce_base) > self.step_budget {
            self.exhausted = true;
            return true;
        }
        false
    }

    /// Start charging a fresh enforcement against the budget.
    const fn begin_enforcement(&mut self) {
        self.enforce_base = self.steps;
    }

    /// A summary that changes exactly when a sweep moved something.
    fn change_stamp(&self) -> (usize, u64) {
        let mut domains = 0u64;
        for word in self.domains.bits() {
            domains = domains.rotate_left(7).wrapping_add(*word);
        }
        (self.trail.len(), domains)
    }

    /// The table slot a value occupies for one variable.
    #[inline]
    fn slot_of(&self, var: VarId, value: ValId) -> Option<usize> {
        let slots = *self.slots.get(var.index())?;
        if value.is_bottom() {
            slots.checked_sub(1)
        } else if value.index() + 1 < slots {
            Some(value.index())
        } else {
            None
        }
    }

    /// The cell holding one unary cost.
    #[inline]
    fn unary_cell(&self, var: VarId, value: ValId) -> Option<usize> {
        let base = *self.unary_at.get(var.index())?;
        Some(base + self.slot_of(var, value)?)
    }

    /// The cell holding one cost function entry, positional against its scope.
    fn tuple_cell(&self, function: usize, tuple: &[ValId]) -> Option<usize> {
        let scope = self.scopes.get(function)?;
        let strides = self.strides.get(function)?;
        if tuple.len() != scope.len() {
            return None;
        }
        let mut index = *self.table_at.get(function)?;
        for ((var, value), stride) in scope.iter().zip(tuple).zip(strides) {
            index += self.slot_of(*var, *value)? * stride;
        }
        Some(index)
    }

    /// The cell holding one entry of a binary cost function, named by its two
    /// endpoints in either order.
    fn binary_cell(
        &self,
        function: usize,
        first: VarId,
        first_value: ValId,
        second: VarId,
        second_value: ValId,
    ) -> Option<usize> {
        let scope = self.scopes.get(function)?;
        if scope.len() != 2 {
            return None;
        }
        if scope.first() == Some(&first) && scope.get(1) == Some(&second) {
            self.tuple_cell(function, &[first_value, second_value])
        } else if scope.first() == Some(&second) && scope.get(1) == Some(&first) {
            self.tuple_cell(function, &[second_value, first_value])
        } else {
            None
        }
    }

    /// The cost of one binary entry, named by its two endpoints in either
    /// order.
    fn binary_cost(
        &self,
        function: usize,
        first: VarId,
        first_value: ValId,
        second: VarId,
        second_value: ValId,
    ) -> Cost {
        self.binary_cell(function, first, first_value, second, second_value)
            .map_or(self.top, |cell| self.cell(cell))
    }

    /// Visit every tuple of a cost function drawn from the current domains,
    /// optionally holding one variable at one value.
    ///
    /// The visitor receives the cell index and the tuple, positional against
    /// the scope.
    fn walk_tuples(
        &self,
        function: usize,
        fixed: Option<(VarId, ValId)>,
        mut visit: impl FnMut(usize, &[ValId]),
    ) {
        let Some(scope) = self.scopes.get(function) else {
            return;
        };
        let arity = scope.len();
        // One block per scope position rather than per variable, which is what
        // makes the odometer positional against the scope.
        let mut whole = Domains::like(&self.domains, arity);
        for (position, var) in scope.iter().enumerate() {
            let slot = VarId::new(u32::try_from(position).unwrap_or(u32::MAX));
            match fixed {
                Some((held, value)) if held == *var => whole.insert(slot, value),
                _ => whole.copy_block(slot, self.domains.block(*var)),
            }
        }
        if whole.any_empty() {
            return;
        }
        let mut untried = whole.clone();
        let mut tuple: SmallVec<ValId, 4> = SmallVec::with_capacity(arity);
        for position in 0..arity {
            let slot = VarId::new(u32::try_from(position).unwrap_or(u32::MAX));
            let Some(value) = untried.get(slot).first() else {
                return;
            };
            untried.remove(slot, value);
            tuple.push(value);
        }
        loop {
            if let Some(index) = self.tuple_cell(function, &tuple) {
                visit(index, &tuple);
            }
            if !advance(&whole, &mut untried, &mut tuple) {
                return;
            }
        }
    }

    /// `min{ c_S(t) : t ∈ ℓ(S), t_i = a }`, or `⊤` when there is no such tuple.
    fn min_over_tuples(&self, function: usize, var: VarId, value: ValId) -> Cost {
        let mut least = self.top;
        self.walk_tuples(function, Some((var, value)), |index, _| {
            let entry = self.cell(index);
            if entry < least {
                least = entry;
            }
        });
        least
    }

    /// `min{ c_i(a) : a ∈ d_i }`, or `None` when the domain is empty.
    ///
    /// Clamped at `⊤`, because a cost recorded under an earlier, larger bound
    /// reads above the current one and is `⊤` under it. Without the clamp a
    /// variable whose every value is hard would offer an argument larger than
    /// any cell can pay, and the difference that pays it is undefined.
    fn min_unary(&self, var: VarId) -> Option<Cost> {
        let mut least: Option<Cost> = None;
        for value in self.domain(var) {
            let cost = self.unary_cost(var, value).min(self.top);
            if least.is_none_or(|found| cost < found) {
                least = Some(cost);
            }
        }
        least
    }
}

// ---------------------------------------------------------------------------
// Internals: the enforcing steps
// ---------------------------------------------------------------------------

impl Network {
    /// `ProjectUnary(i)`: move the least unary cost of a variable into `c_∅`.
    fn project_unary(&mut self, var: VarId) {
        if let Some(alpha) = self.min_unary(var) {
            self.unary_project(var, alpha);
        }
    }

    /// `PruneVar(i)`: drop every value that is infeasible against `c_∅`.
    ///
    /// Returns whether anything went.
    fn prune_var(&mut self, var: VarId) -> bool {
        let constant = self.c_empty();
        let top = self.top;
        let mut dropped = false;
        let snapshot = self.domain_words(var);
        for value in Domain::new(&snapshot) {
            if self.is_top(constant.combine(self.unary_cost(var, value), top)) {
                self.refute(var, value);
                dropped = true;
            }
        }
        self.step(u64::try_from(self.domain(var).len()).unwrap_or(u64::MAX));
        dropped
    }

    /// One node consistency sweep: project every unary table, then prune.
    fn nc_sweep(&mut self) {
        for var in 0..self.n_variables() {
            let id = VarId::new(u32::try_from(var).unwrap_or(u32::MAX));
            self.project_unary(id);
        }
        for var in 0..self.n_variables() {
            let id = VarId::new(u32::try_from(var).unwrap_or(u32::MAX));
            self.prune_var(id);
        }
    }

    /// Saturate a cost function against the unary costs of one of its
    /// variables, by extending `⊥`.
    ///
    /// This is clause one of generalized arc consistency: a tuple whose total
    /// with the unary costs and the constant reaches `⊤` is itself set to `⊤`,
    /// which is what lets accumulated finite cost prove infeasibility. Running
    /// it over one scope variable covers every tuple, because every tuple gives
    /// that variable some value.
    fn saturate(&mut self, function: usize, var: VarId) {
        let snapshot = self.domain_words(var);
        for value in Domain::new(&snapshot) {
            self.extend(var, value, function, Cost::BOT);
        }
    }

    /// `FindSupports(i, j)`: give every value of `var` a simple support in one
    /// cost function.
    ///
    /// Returns whether some value's unary cost rose off `⊥`, which is the
    /// signal the queues of the strongest level are driven by.
    fn find_supports(&mut self, function: usize, var: VarId) -> bool {
        if self.scope(function).len() < 2 || !self.scope(function).contains(&var) {
            return false;
        }
        self.saturate(function, var);
        let mut moves: Vec<(ValId, Cost)> = Vec::new();
        let mut flag = false;
        for value in self.domain(var) {
            let alpha = self.min_over_tuples(function, var, value);
            if alpha > Cost::BOT && self.unary_cost(var, value) == Cost::BOT {
                flag = true;
            }
            moves.push((value, alpha));
        }
        for (value, alpha) in moves {
            self.project(function, var, value, alpha);
        }
        self.project_unary(var);
        flag
    }

    /// `FindFullSupports(i, j)`: give every value of `target` a full support in
    /// one binary cost function, by first extending the unary costs of `source`
    /// into it.
    ///
    /// Returns whether some value's unary cost rose off `⊥`.
    fn find_full_supports(&mut self, function: usize, target: VarId, source: VarId) -> bool {
        let (target_domain, source_domain) = (self.domain(target), self.domain(source));
        if self.scope(function).len() != 2 || target_domain.is_empty() || source_domain.is_empty() {
            return false;
        }
        let top = self.top;
        let mut flag = false;
        let mut supports: Vec<(ValId, Cost)> = Vec::new();
        for value in target_domain {
            let mut least = top;
            for other in source_domain {
                let pair = self.binary_cost(function, target, value, source, other);
                let total = pair.combine(self.unary_cost(source, other), top);
                if total < least {
                    least = total;
                }
            }
            if least > Cost::BOT && self.unary_cost(target, value) == Cost::BOT {
                flag = true;
            }
            supports.push((value, least));
        }

        let mut lifts: Vec<(ValId, Cost)> = Vec::new();
        for other in source_domain {
            let mut most = Cost::BOT;
            for &(value, least) in &supports {
                let pair = self.binary_cost(function, target, value, source, other);
                let wanted = least.sat_diff(pair, top);
                if wanted > most {
                    most = wanted;
                }
            }
            // The extension is capped at the unary cost it comes from. The cap
            // is not a safety net over a proof: the published bound
            // `max_a { P[a] ⊖ c_ij(a,b) } ⪯ c_j(b)` follows from maximality of
            // the difference, and maximality fails at `⊤`, where the largest
            // `γ` with `c_ij(a,b) ⊕ γ = ⊤` is `⊤` itself. Capping keeps the
            // full support intact: `c_ij(a,b) ⊕ c_j(b) ⪰ P[a]` holds by the
            // definition of `P`, so the projection that follows is still legal,
            // and for the value attaining `P[a]` the cap is exactly `c_j(b)`,
            // which is what leaves that value's support at `⊥`.
            let capped = most.min(self.unary_cost(source, other));
            lifts.push((other, capped));
        }

        for (other, amount) in lifts {
            self.extend(source, other, function, amount);
        }
        for (value, amount) in supports {
            self.project(function, target, value, amount);
        }
        self.project_unary(target);
        flag
    }

    /// The `α` of the existential arc consistency test, restricted to the lower
    /// neighbours the enforcing algorithm looks at.
    ///
    /// `⊥` exactly when the variable already has an existential support.
    fn existential_bound(&self, var: VarId) -> Cost {
        let mut least = self.top;
        for value in self.domain(var) {
            let score = self.existential_score(var, value, true);
            if score < least {
                least = score;
            }
        }
        least
    }

    /// `c_i(a) ⊕ ⨁_j min_b { c_ij(a,b) ⊕ c_j(b) }` over the binary neighbours
    /// of `var`, either all of them or only the lower ones.
    fn existential_score(&self, var: VarId, value: ValId, lower_only: bool) -> Cost {
        let top = self.top;
        let mut total = self.unary_cost(var, value);
        for &(other, function) in self.binary_neighbours(var) {
            if lower_only && other >= var {
                continue;
            }
            let mut least = top;
            for candidate in self.domain(other) {
                let pair = self.binary_cost(function, var, value, other, candidate);
                let sum = pair.combine(self.unary_cost(other, candidate), top);
                if sum < least {
                    least = sum;
                }
            }
            total = total.combine(least, top);
        }
        total
    }

    /// `FindExistentialSupport(i)`: give the variable a value costing `⊥` with
    /// a full support toward every lower neighbour.
    fn find_existential_support(&mut self, var: VarId) -> bool {
        if self.existential_bound(var) == Cost::BOT {
            return false;
        }
        let mut flag = false;
        let lower: Vec<(VarId, usize)> = self
            .binary_neighbours(var)
            .iter()
            .copied()
            .filter(|(other, _)| *other < var)
            .collect();
        for (other, function) in lower {
            flag |= self.find_full_supports(function, var, other);
        }
        flag
    }

    /// One arc consistency sweep over every cost function and every position.
    fn arc_sweep(&mut self) {
        for function in 0..self.n_functions() {
            for var in self.scope(function).to_vec() {
                self.find_supports(function, var);
            }
        }
    }

    /// One directional sweep: higher variables first, so that a full support
    /// established toward a higher variable is not undone by a later step.
    fn directional_sweep(&mut self) {
        for index in (0..self.n_variables()).rev() {
            let high = VarId::new(u32::try_from(index).unwrap_or(u32::MAX));
            let lower: Vec<(VarId, usize)> = self
                .binary_neighbours(high)
                .iter()
                .copied()
                .filter(|(other, _)| *other < high)
                .collect();
            for (low, function) in lower {
                self.find_full_supports(function, low, high);
            }
        }
    }

    /// Repeat node consistency sweeps until nothing moves.
    fn node_loop(&mut self) {
        loop {
            let before = self.change_stamp();
            self.nc_sweep();
            if self.change_stamp() == before || self.out_of_steps() {
                return;
            }
        }
    }

    /// Repeat arc and node sweeps until nothing moves.
    fn arc_loop(&mut self) {
        loop {
            let before = self.change_stamp();
            self.arc_sweep();
            self.nc_sweep();
            if self.change_stamp() == before || self.out_of_steps() {
                return;
            }
        }
    }

    /// Repeat directional and node sweeps until nothing moves.
    fn directional_loop(&mut self) {
        loop {
            let before = self.change_stamp();
            self.directional_sweep();
            self.nc_sweep();
            if self.change_stamp() == before || self.out_of_steps() {
                return;
            }
        }
    }

    /// Repeat arc, directional and node sweeps until nothing moves.
    fn full_directional_loop(&mut self) {
        loop {
            let before = self.change_stamp();
            self.arc_sweep();
            self.directional_sweep();
            self.nc_sweep();
            if self.change_stamp() == before || self.out_of_steps() {
                return;
            }
        }
    }

    /// Repeat the three queues together with the weaker sweeps until nothing
    /// moves.
    fn existential_loop(&mut self) {
        loop {
            let before = self.change_stamp();
            self.edac_rounds();
            self.arc_sweep();
            self.directional_sweep();
            self.nc_sweep();
            if self.change_stamp() == before || self.out_of_steps() {
                return;
            }
        }
    }

    /// The three queues of Algorithm 3, run until all of them drain.
    ///
    /// `queue` holds variables that lost a value, so their higher neighbours
    /// may have lost a simple support; `raised` holds variables whose unary
    /// cost rose off `⊥`, so their lower neighbours may have lost a full
    /// support; `shifted` builds the queue of variables whose existential
    /// support may have gone.
    fn edac_rounds(&mut self) {
        let count = self.n_variables();
        let mut queue = vec![true; count];
        let mut raised = vec![true; count];
        let mut shifted = vec![true; count];
        while queued(&queue) || queued(&raised) || queued(&shifted) {
            if self.out_of_steps() {
                return;
            }
            let mut pending = self.existential_queue(&shifted);
            shifted.fill(false);
            self.existential_pass(&mut pending, &mut raised);
            self.directional_pass(&mut raised, &mut shifted);
            self.arc_pass(&mut queue, &mut raised, &mut shifted);
            for index in 0..count {
                let var = VarId::new(u32::try_from(index).unwrap_or(u32::MAX));
                if self.prune_var(var) {
                    enqueue(&mut queue, var);
                }
            }
        }
    }

    /// The `P` queue of Algorithm 3: every variable whose existential support
    /// may have gone.
    fn existential_queue(&self, shifted: &[bool]) -> Vec<bool> {
        let mut pending = shifted.to_vec();
        for (index, flag) in shifted.iter().enumerate() {
            if !flag {
                continue;
            }
            let var = VarId::new(u32::try_from(index).unwrap_or(u32::MAX));
            for &(other, _) in self.binary_neighbours(var) {
                if other > var {
                    if let Some(slot) = pending.get_mut(other.index()) {
                        *slot = true;
                    }
                }
            }
        }
        pending
    }

    /// The existential pass of Algorithm 3, lowest variable first.
    fn existential_pass(&mut self, pending: &mut [bool], raised: &mut [bool]) {
        while let Some(var) = pop_min(pending) {
            if self.out_of_steps() {
                return;
            }
            if !self.find_existential_support(var) {
                continue;
            }
            if let Some(flag) = raised.get_mut(var.index()) {
                *flag = true;
            }
            for &(other, _) in self.binary_neighbours(var) {
                if other > var {
                    if let Some(flag) = pending.get_mut(other.index()) {
                        *flag = true;
                    }
                }
            }
        }
    }

    /// The directional pass of Algorithm 3, highest variable first.
    fn directional_pass(&mut self, raised: &mut [bool], shifted: &mut [bool]) {
        while let Some(high) = pop_max(raised) {
            if self.out_of_steps() {
                return;
            }
            let lower: Vec<(VarId, usize)> = self
                .binary_neighbours(high)
                .iter()
                .copied()
                .filter(|(other, _)| *other < high)
                .collect();
            for (low, function) in lower {
                if self.find_full_supports(function, low, high) {
                    enqueue(raised, low);
                    enqueue(shifted, low);
                }
            }
        }
    }

    /// The arc pass of Algorithm 3.
    ///
    /// A binary cost function is revisited only from its lower endpoint,
    /// because the directional pass already covers the other direction. A cost
    /// function of arity three or more has no direction, so every other
    /// position is revisited.
    fn arc_pass(&mut self, queue: &mut [bool], raised: &mut [bool], shifted: &mut [bool]) {
        while let Some(low) = pop_min(queue) {
            if self.out_of_steps() {
                return;
            }
            for function in self.incident(low).to_vec() {
                let scope = self.scope(function).to_vec();
                for var in scope.iter().copied() {
                    if var == low || (scope.len() == 2 && var < low) {
                        continue;
                    }
                    if self.find_supports(function, var) {
                        enqueue(raised, var);
                        enqueue(shifted, var);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Internals: the predicates
// ---------------------------------------------------------------------------

impl Network {
    /// Whether every tuple of a cost function that is infeasible against the
    /// unary costs and the constant is itself `⊤`.
    fn function_is_saturated(&self, function: usize) -> bool {
        let scope = self.scope(function).to_vec();
        let top = self.top;
        let constant = self.c_empty();
        let mut saturated = true;
        self.walk_tuples(function, None, |index, tuple| {
            let entry = self.cell(index);
            let mut total = constant.combine(entry, top);
            for (var, value) in scope.iter().zip(tuple) {
                total = total.combine(self.unary_cost(*var, *value), top);
            }
            if total.raw() >= top.raw() && entry.raw() < top.raw() {
                saturated = false;
            }
        });
        saturated
    }

    /// Whether every value of `low` has a full support in `high`.
    fn has_full_supports(&self, function: usize, low: VarId, high: VarId) -> bool {
        for value in self.domain(low) {
            let mut supported = false;
            for other in self.domain(high) {
                if self.binary_cost(function, low, value, high, other) == Cost::BOT
                    && self.unary_cost(high, other) == Cost::BOT
                {
                    supported = true;
                }
            }
            if !supported {
                return false;
            }
        }
        true
    }

    /// Whether some value of a variable costs `⊥` and is fully supported in
    /// every binary cost function, in both directions.
    fn has_existential_support(&self, var: VarId) -> bool {
        for value in self.domain(var) {
            if self.unary_cost(var, value) != Cost::BOT {
                continue;
            }
            let mut supported = true;
            for &(other, function) in self.binary_neighbours(var) {
                let mut found = false;
                for candidate in self.domain(other) {
                    if self.binary_cost(function, var, value, other, candidate) == Cost::BOT
                        && self.unary_cost(other, candidate) == Cost::BOT
                    {
                        found = true;
                    }
                }
                if !found {
                    supported = false;
                    break;
                }
            }
            if supported {
                return true;
            }
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

/// The row-major stride of each position of a scope.
fn scope_strides(slots: &[usize], scope: &[VarId]) -> Vec<usize> {
    let mut strides = vec![1usize; scope.len()];
    let mut running = 1usize;
    for (position, var) in scope.iter().enumerate().rev() {
        if let Some(stride) = strides.get_mut(position) {
            *stride = running;
        }
        running = running.saturating_mul(slots.get(var.index()).copied().unwrap_or(1));
    }
    strides
}

/// The ceiling on one enforcement's elementary operations, from the published
/// complexity of the strongest level.
///
/// `O(e d² · max{nd, ⊤})` with the `⊤` factor replaced by a constant: `⊤` is
/// the primal bound, which can be the whole cost range, and a budget that large
/// would never fire. What the budget is for is catching a loop that is not
/// making progress, and a loop that is not making progress does not need many
/// rounds to reveal itself.
fn default_step_budget(variables: usize, functions: usize, domain: usize) -> u64 {
    let n = u64::try_from(variables).unwrap_or(u64::MAX);
    let e = u64::try_from(functions).unwrap_or(u64::MAX);
    let d = u64::try_from(domain).unwrap_or(u64::MAX);
    let square = d.saturating_mul(d).max(1);
    let rounds = n.saturating_mul(d).max(1024);
    (e + n + 1)
        .saturating_mul(square)
        .saturating_mul(rounds)
        .saturating_add(1024)
}

/// Move an odometer over the product of the per-position domains on to its next
/// tuple, returning whether there was one.
///
/// `untried` holds the values each position has not yet taken since it last
/// carried, and `whole` is what a position is refilled from when it does.
fn advance(whole: &Domains, untried: &mut Domains, tuple: &mut [ValId]) -> bool {
    let mut position = tuple.len();
    loop {
        if position == 0 {
            return false;
        }
        position -= 1;
        let index = VarId::new(u32::try_from(position).unwrap_or(u32::MAX));
        let Some(slot) = tuple.get_mut(position) else {
            return false;
        };
        if let Some(next) = untried.get(index).first() {
            *slot = next;
            untried.remove(index, next);
            return true;
        }
        let Some(first) = whole.get(index).first() else {
            return false;
        };
        untried.copy_block(index, whole.block(index));
        untried.remove(index, first);
        *slot = first;
    }
}

/// Whether a queue still holds anything.
fn queued(queue: &[bool]) -> bool {
    queue.iter().any(|flag| *flag)
}

/// Take the lowest flagged variable out of a queue.
fn pop_min(queue: &mut [bool]) -> Option<VarId> {
    let index = queue.iter().position(|flag| *flag)?;
    if let Some(flag) = queue.get_mut(index) {
        *flag = false;
    }
    u32::try_from(index).ok().map(VarId::new)
}

/// Take the highest flagged variable out of a queue.
fn pop_max(queue: &mut [bool]) -> Option<VarId> {
    let index = queue.iter().rposition(|flag| *flag)?;
    if let Some(flag) = queue.get_mut(index) {
        *flag = false;
    }
    u32::try_from(index).ok().map(VarId::new)
}

/// Put a variable in a queue.
fn enqueue(queue: &mut [bool], var: VarId) {
    if let Some(flag) = queue.get_mut(var.index()) {
        *flag = true;
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::too_many_lines
)]
mod tests {
    use super::*;
    use crate::solve::Assignment;
    use crate::solve::cfn::CfnBuilder;
    use crate::solve::cost::DEFAULT_WEIGHTS;
    use panproto_gat::Name;
    use proptest::prelude::*;

    fn cost(units: u64) -> Cost {
        Cost::from_raw(units)
    }

    const FIRST: VarId = VarId::new(0);
    const SECOND: VarId = VarId::new(1);

    // -- the two saturation regressions -----------------------------------

    /// The example of Cooper et al. §3, verbatim: in the structure with
    /// `⊤ = 10`, `c_i(a) = c_∅ = 5` and `α = ⊥`, `UnaryProject` must leave
    /// `c_i(a) = ((5 ⊕ 5) ⊖ 5) ⊖ 0 = ⊤`.
    ///
    /// An `if α == ⊥ { return }` fast path fails exactly here, and the failure
    /// is invisible in the optimum: it costs the search a value it was entitled
    /// to prune.
    #[test]
    fn unary_project_reaches_top_with_a_bottom_argument() {
        let mut builder = CfnBuilder::new(
            vec![(Name::new("v"), vec![Name::new("t")])],
            DEFAULT_WEIGHTS,
        )
        .unwrap();
        builder.add_empty(cost(5));
        builder.add_unary(FIRST, ValId::real(0), cost(5)).unwrap();
        let cfn = builder.build();

        let mut network = Network::from_cfn(&cfn, cost(10));
        assert_eq!(network.unary_cost(FIRST, ValId::real(0)), cost(5));
        assert_eq!(network.c_empty(), cost(5));

        network.unary_project(FIRST, Cost::BOT);

        assert_eq!(
            network.unary_cost(FIRST, ValId::real(0)),
            cost(10),
            "the saturation subterm must carry the value to ⊤"
        );
        assert_eq!(network.c_empty(), cost(5), "a ⊥ argument moves no cost");
        assert_eq!(network.unary_cost(FIRST, ValId::BOTTOM), Cost::BOT);
    }

    /// A primal bound below `c_∅` is a reading, not a crash.
    ///
    /// `⊤` is the moving primal bound and `c_∅` carries the vacuous components
    /// of the objective, so a caller asking whether anything beats a bound
    /// below `c_∅` is asking an ordinary question. Reading `c_∅` unclamped made
    /// the saturation subterm `(x ⊕ c_∅)` land at `⊤ ≺ c_∅`, and the `⊖` that
    /// follows then violated its own fairness precondition and aborted, in a
    /// release build, from every level of the enforcement stack.
    #[test]
    fn unary_project_reads_a_constant_above_top_as_top() {
        let mut builder = CfnBuilder::new(
            vec![(Name::new("v"), vec![Name::new("t")])],
            DEFAULT_WEIGHTS,
        )
        .unwrap();
        builder.add_empty(cost(5));
        builder.add_unary(FIRST, ValId::real(0), cost(2)).unwrap();
        let cfn = builder.build();

        // `⊤ = 3 ≺ c_∅ = 5`: the network is already dead, and saying so is the
        // job. Every unary cost goes to `⊤` and the bound stays there, which is
        // what `feasible` reads as dead.
        let mut network = Network::from_cfn(&cfn, cost(3));
        network.unary_project(FIRST, Cost::BOT);

        assert_eq!(network.unary_cost(FIRST, ValId::real(0)), cost(3));
        assert_eq!(network.unary_cost(FIRST, ValId::BOTTOM), cost(3));
        assert_eq!(network.c_empty(), cost(3));
        assert!(!network.feasible(), "a bound below c_∅ admits nothing");

        // And the whole stack survives it, at every level.
        for level in ConsistencyLevel::ALL {
            let mut network = Network::from_cfn(&cfn, cost(3));
            assert!(
                !network.enforce(level),
                "{} should report the node dead",
                level.label()
            );
        }
    }

    /// The step budget is one enforcement's allowance, charged per enforcement.
    ///
    /// One [`Network`] lives for a whole search, and the ceiling is sized from
    /// the published cost of reaching a *single* fixpoint. Charging every node
    /// against one ceiling spent it inside the first dive, after which every
    /// loop short-circuited on entry and the strongest level silently decayed
    /// to roughly one pass of the weaker ones, while `budget_exhausted` became
    /// a permanent false positive.
    ///
    /// The second assertion is what makes this a test: the run really does
    /// spend more than one enforcement's allowance in total, and the flag still
    /// never latches.
    #[test]
    fn the_step_budget_is_charged_per_enforcement() {
        let mut builder = CfnBuilder::new(
            vec![
                (Name::new("u"), vec![Name::new("t0"), Name::new("t1")]),
                (Name::new("v"), vec![Name::new("t0"), Name::new("t1")]),
            ],
            DEFAULT_WEIGHTS,
        )
        .unwrap();
        builder
            .add_unary_table(FIRST, &[cost(3), cost(1), cost(4)])
            .unwrap();
        builder
            .add_unary_table(SECOND, &[cost(2), cost(5), cost(1)])
            .unwrap();
        builder
            .add_function(
                &[FIRST, SECOND],
                vec![
                    cost(1),
                    cost(4),
                    cost(2),
                    cost(3),
                    cost(0),
                    cost(5),
                    cost(2),
                    cost(1),
                    cost(3),
                ],
            )
            .unwrap();
        let cfn = builder.build();

        let mut network = Network::from_cfn(&cfn, Cost::TOP_SENTINEL);
        let budget = network.step_budget();
        for _ in 0..4_000 {
            network.reset(Cost::TOP_SENTINEL);
            assert!(network.enforce(ConsistencyLevel::ExistentialDirectionalArc));
        }

        assert!(
            !network.budget_exhausted(),
            "no single enforcement outspent its own allowance"
        );
        assert!(
            network.steps() > budget,
            "the run spent {} steps against a per-enforcement ceiling of {budget}, \
             so a per-network ceiling would have latched",
            network.steps()
        );
    }

    /// The same for `Extend`: a tuple whose total with the unary costs and the
    /// constant reaches `⊤` is set to `⊤`, with `α = ⊥`.
    #[test]
    fn extend_reaches_top_with_a_bottom_argument() {
        let mut builder = CfnBuilder::new(
            vec![
                (Name::new("u"), vec![Name::new("t")]),
                (Name::new("v"), vec![Name::new("t")]),
            ],
            DEFAULT_WEIGHTS,
        )
        .unwrap();
        builder.add_empty(cost(5));
        builder.add_unary(FIRST, ValId::real(0), cost(3)).unwrap();
        builder.add_unary(SECOND, ValId::real(0), cost(2)).unwrap();
        // Slots are `[t, ⊥]` on both, so the table is row-major over four
        // entries and `(t, t)` is entry zero.
        builder
            .add_function(
                &[FIRST, SECOND],
                vec![cost(1), Cost::BOT, Cost::BOT, Cost::BOT],
            )
            .unwrap();
        let cfn = builder.build();

        let mut network = Network::from_cfn(&cfn, cost(10));
        network.extend(FIRST, ValId::real(0), 0, Cost::BOT);

        assert_eq!(
            network.function_cost(0, &[ValId::real(0), ValId::real(0)]),
            cost(10),
            "5 ⊕ 3 ⊕ 2 already reaches ⊤, so the tuple is infeasible"
        );
        assert_eq!(
            network.function_cost(0, &[ValId::real(0), ValId::BOTTOM]),
            Cost::BOT,
            "5 ⊕ 3 ⊕ 0 does not reach ⊤, so that tuple is untouched"
        );
        assert_eq!(network.unary_cost(FIRST, ValId::real(0)), cost(3));
    }

    // -- the oscillation regression ---------------------------------------

    /// The shape of Figure 4 of Lee and Leung (*JAIR* 44, 2012): two variables
    /// over one scope carrying **two** binary cost functions, with a unary cost
    /// that has a choice of which one to move into.
    ///
    /// Nothing records that choice, so a wrong one breaks directional
    /// consistency without raising the bound and enforcement oscillates. The
    /// defence is structural: the builder merges the two into one by pointwise
    /// `⊕`, so the hazard cannot be stated. This test is here so that a future
    /// change relaxing scope uniqueness reintroduces a visible failure rather
    /// than a silent hang.
    #[test]
    fn two_cost_functions_on_one_scope_merge_and_the_strongest_level_terminates() {
        let mut builder = CfnBuilder::new(
            vec![
                (Name::new("x1"), vec![Name::new("a"), Name::new("b")]),
                (Name::new("x2"), vec![Name::new("a"), Name::new("b")]),
            ],
            DEFAULT_WEIGHTS,
        )
        .unwrap();
        builder.add_unary(FIRST, ValId::real(0), cost(1)).unwrap();
        builder.add_unary(SECOND, ValId::real(1), cost(1)).unwrap();
        // Slots are `[a, b, ⊥]` on both, so each table has nine entries.
        let first = vec![
            Cost::BOT,
            cost(1),
            Cost::BOT,
            cost(1),
            Cost::BOT,
            Cost::BOT,
            Cost::BOT,
            Cost::BOT,
            Cost::BOT,
        ];
        let second = vec![
            cost(1),
            Cost::BOT,
            Cost::BOT,
            Cost::BOT,
            cost(1),
            Cost::BOT,
            Cost::BOT,
            Cost::BOT,
            Cost::BOT,
        ];
        builder.add_function(&[FIRST, SECOND], first).unwrap();
        builder.add_function(&[FIRST, SECOND], second).unwrap();
        let cfn = builder.build();

        assert_eq!(
            cfn.n_functions(),
            1,
            "two cost functions on one scope are one cost function"
        );
        let merged = cfn.function_for(&[FIRST, SECOND]).unwrap();
        assert_eq!(
            merged.table()[0],
            cost(1),
            "the merged entry is the pointwise ⊕ of the two"
        );

        let mut network = Network::from_cfn(&cfn, cost(4));
        let feasible = network.enforce_edac_star();
        assert!(
            !network.budget_exhausted(),
            "enforcement must terminate on its own, not on the budget"
        );
        assert!(feasible);
        assert!(network.is_edac_star());
    }

    // -- generators --------------------------------------------------------

    /// A deterministic consumer of a pool of proptest-drawn numbers.
    ///
    /// The shape of a network depends on numbers drawn earlier in the same
    /// draw, which a tuple of independent strategies cannot express. Reading
    /// from a pool keeps every choice a shrinkable value rather than a seed.
    struct Draw {
        pool: Vec<u64>,
        cursor: usize,
    }

    impl Draw {
        fn new(pool: Vec<u64>) -> Self {
            Self { pool, cursor: 0 }
        }

        fn take(&mut self, bound: u64) -> u64 {
            if self.pool.is_empty() {
                return 0;
            }
            let value = self.pool.get(self.cursor).copied().unwrap_or(0);
            self.cursor = (self.cursor + 1) % self.pool.len();
            value % bound.max(1)
        }
    }

    /// A cost that is hard one time in `hard_in` and small otherwise.
    ///
    /// The rate is a parameter because the hard entries are what decide how far
    /// a search gets before every branch is closed. At one in six the networks
    /// are dense enough that a level usually closes the root outright, which is
    /// what the cost-vector properties want and what a property about the nodes
    /// of a search does not.
    fn drawn_cost(draw: &mut Draw, hard_in: u64) -> Cost {
        if draw.take(hard_in) == 0 {
            Cost::TOP_SENTINEL
        } else {
            cost(draw.take(5))
        }
    }

    /// One time in six, the rate the cost-vector generators have always used.
    const DENSE: u64 = 6;

    /// One time in forty, which leaves enough assignments feasible for a search
    /// to have to branch rather than close at the root.
    const SPARSE: u64 = 40;

    /// A small network together with the `⊤` to solve it under.
    ///
    /// Two to four variables with one or two real values each, so the
    /// assignment space is at most `3^4 = 81` and the cost vector over it is
    /// cheap to compute twice.
    fn arb_instance() -> impl Strategy<Value = (Cfn, Cost)> {
        (
            prop::collection::vec(1usize..=2, 2..=4),
            prop::collection::vec(0u64..64, 48),
            4u64..24,
        )
            .prop_map(|(reals, pool, top)| (build_instance(&reals, Draw::new(pool)), cost(top)))
    }

    /// The same, with a second pool driving a script of transformations.
    fn arb_instance_and_script() -> impl Strategy<Value = (Cfn, Cost, Vec<u64>)> {
        (arb_instance(), prop::collection::vec(0u64..64, 64))
            .prop_map(|((cfn, top), script)| (cfn, top, script))
    }

    /// A network guaranteed to carry at least one binary cost function, at the
    /// shape the per-transformation properties are stated over: at most six
    /// variables and at most three values each, `⊥` counted.
    ///
    /// [`arb_instance`] may draw away every pair, and a network with no binary
    /// function offers no site for `Project` or `Extend`. A property that
    /// applied nothing on those cases would still report a pass, so the pair on
    /// the first two variables is forced here.
    fn arb_ept_instance() -> impl Strategy<Value = (Cfn, Cost, Vec<u64>)> {
        (
            prop::collection::vec(1usize..=2, 2..=6),
            prop::collection::vec(0u64..64, 96),
            4u64..24,
            prop::collection::vec(0u64..64, 16),
        )
            .prop_map(|(reals, pool, top, script)| {
                (
                    build_instance_with(&reals, Draw::new(pool), true, DENSE),
                    cost(top),
                    script,
                )
            })
    }

    /// A network small enough to walk a whole branch and bound tree over while
    /// brute forcing the subproblem at every node, and loose enough that the
    /// tree has more than a root: at most four variables, at most three values
    /// each, hard constraints at [`SPARSE`], and a `⊤` well above the costs so
    /// that the bound has room to rise before it closes anything.
    fn arb_node_instance() -> impl Strategy<Value = (Cfn, Cost)> {
        (
            prop::collection::vec(1usize..=2, 2..=4),
            prop::collection::vec(0u64..64, 64),
            16u64..48,
        )
            .prop_map(|(reals, pool, top)| {
                (
                    build_instance_with(&reals, Draw::new(pool), true, SPARSE),
                    cost(top),
                )
            })
    }

    fn build_instance(reals: &[usize], draw: Draw) -> Cfn {
        build_instance_with(reals, draw, false, DENSE)
    }

    /// The shared builder. `force_first_pair` keeps the function on variables
    /// zero and one whatever the draw says, which is what makes a network from
    /// [`arb_ept_instance`] offer every transformation a site.
    fn build_instance_with(
        reals: &[usize],
        mut draw: Draw,
        force_first_pair: bool,
        hard_in: u64,
    ) -> Cfn {
        let names: Vec<(Name, Vec<Name>)> = reals
            .iter()
            .enumerate()
            .map(|(index, count)| {
                let values = (0..*count).map(|k| Name::new(format!("t{k}"))).collect();
                (Name::new(format!("v{index}")), values)
            })
            .collect();
        let mut builder = CfnBuilder::new(names, DEFAULT_WEIGHTS).unwrap();
        builder.add_empty(cost(draw.take(3)));

        for (index, count) in reals.iter().enumerate() {
            let var = VarId::new(u32::try_from(index).unwrap());
            let table: Vec<Cost> = (0..=*count)
                .map(|_| drawn_cost(&mut draw, hard_in))
                .collect();
            builder.add_unary_table(var, &table).unwrap();
        }

        for low in 0..reals.len() {
            for high in (low + 1)..reals.len() {
                let forced = force_first_pair && low == 0 && high == 1;
                if !forced && draw.take(4) == 0 {
                    continue;
                }
                let entries = (reals[low] + 1) * (reals[high] + 1);
                let table: Vec<Cost> = (0..entries)
                    .map(|_| drawn_cost(&mut draw, hard_in))
                    .collect();
                builder
                    .add_function(
                        &[
                            VarId::new(u32::try_from(low).unwrap()),
                            VarId::new(u32::try_from(high).unwrap()),
                        ],
                        table,
                    )
                    .unwrap();
            }
        }
        builder.build()
    }

    /// Every total assignment over the network's current domains.
    fn every_tuple(network: &Network) -> Vec<Vec<ValId>> {
        let mut out = vec![Vec::new()];
        for var in network.variable_ids() {
            let domain = network.domain(var);
            let mut next = Vec::with_capacity(out.len() * domain.len());
            for partial in &out {
                for value in domain {
                    let mut extended = partial.clone();
                    extended.push(value);
                    next.push(extended);
                }
            }
            out = next;
        }
        out
    }

    /// The cost of every assignment, in one fixed order.
    fn cost_vector(network: &Network) -> Vec<Cost> {
        every_tuple(network)
            .iter()
            .map(|tuple| network.valuation(tuple))
            .collect()
    }

    /// A fraction of the largest legal argument, so that every transformation
    /// this applies satisfies its precondition by construction.
    fn fraction(most: Cost, part: u64) -> Cost {
        if part >= 4 {
            most
        } else {
            Cost::from_raw(most.raw() / 4 * part)
        }
    }

    /// Apply one transformation with a legal argument, or nothing when the
    /// draw names a transformation the network cannot offer.
    fn apply_one_ept(network: &mut Network, draw: &mut Draw) {
        let count = network.n_variables();
        if count == 0 {
            return;
        }
        let var = VarId::new(draw.take(count as u64) as u32);
        let values: Vec<ValId> = network.domain(var).into_iter().collect();
        if values.is_empty() {
            return;
        }
        let value = values[draw.take(values.len() as u64) as usize];
        let functions = network.incident(var).to_vec();
        let part = draw.take(5);

        match draw.take(3) {
            0 => {
                if let Some(most) = network.min_unary(var) {
                    network.unary_project(var, fraction(most, part));
                }
            }
            1 => {
                if let Some(&function) = functions.first() {
                    let most = network.min_over_tuples(function, var, value);
                    network.project(function, var, value, fraction(most, part));
                }
            }
            _ => {
                if let Some(&function) = functions.first() {
                    let most = network.unary_cost(var, value);
                    network.extend(var, value, function, fraction(most, part));
                }
            }
        }
    }

    /// Every `(function, variable, value)` `Project` can be applied at.
    fn project_sites(network: &Network) -> Vec<(usize, VarId, ValId)> {
        let mut sites = Vec::new();
        for var in network.variable_ids() {
            for &function in network.incident(var) {
                for value in network.domain(var) {
                    sites.push((function, var, value));
                }
            }
        }
        sites
    }

    /// Every `(variable, value, function)` `Extend` can be applied at.
    ///
    /// The same sites as `Project`'s, restricted to functions of arity two or
    /// more, which is `Extend`'s own precondition.
    fn extend_sites(network: &Network) -> Vec<(VarId, ValId, usize)> {
        project_sites(network)
            .into_iter()
            .filter(|&(function, _, _)| network.scope(function).len() > 1)
            .map(|(function, var, value)| (var, value, function))
            .collect()
    }

    /// Pick one element of a non-empty list by the draw.
    fn pick<T: Copy>(items: &[T], draw: &mut Draw) -> Option<T> {
        if items.is_empty() {
            return None;
        }
        let index = draw.take(items.len() as u64) as usize;
        items.get(index).copied()
    }

    /// The values of every current domain, in the domain order.
    ///
    /// Written here rather than taken from the oracle because the oracle reads
    /// a [`Cfn`]'s fixed domains and the subproblem beneath a search node is
    /// defined by the [`Network`]'s current ones.
    fn domain_lists(network: &Network) -> Vec<Vec<ValId>> {
        network
            .variable_ids()
            .map(|var| network.domain(var).into_iter().collect())
            .collect()
    }

    /// The least cost [`Cfn::evaluate`] gives any assignment inside `choices`.
    ///
    /// Scored against the pristine network, never against the transformed one:
    /// the number a bound is compared against must owe nothing to the tables
    /// the bound came out of. An empty domain leaves no assignment, which is
    /// `⊤`.
    fn subproblem_optimum(cfn: &Cfn, choices: &[Vec<ValId>]) -> Cost {
        if choices.iter().any(Vec::is_empty) {
            return Cost::TOP_SENTINEL;
        }
        let mut cursor = vec![0usize; choices.len()];
        let mut best = Cost::TOP_SENTINEL;
        loop {
            let values: Vec<ValId> = cursor
                .iter()
                .zip(choices)
                .map(|(slot, values)| values[*slot])
                .collect();
            best = best.min(cfn.evaluate(&Assignment::from_values(values)));

            let mut position = choices.len();
            loop {
                if position == 0 {
                    return best;
                }
                position -= 1;
                cursor[position] += 1;
                if cursor[position] < choices[position].len() {
                    break;
                }
                cursor[position] = 0;
            }
        }
    }

    /// A depth-first branch and bound walk that checks the bound at every node.
    ///
    /// It branches the way [`BranchAndBound`](super::dfbb::BranchAndBound)
    /// does (assign the first branchable variable to its first value, then
    /// refute that value), and it filters each node the way that search's
    /// `propagate` does, by resetting the contributions, setting `⊤` to the
    /// current primal bound and enforcing. What it adds is the comparison the
    /// search cannot make for itself: the true optimum of the subproblem
    /// beneath each node, by exhaustive enumeration.
    struct BoundWalk<'a> {
        cfn: &'a Cfn,
        level: ConsistencyLevel,
        /// The primal bound. Fixed when `moving` is false, and lowered at every
        /// improving leaf when it is true.
        cub: Cost,
        moving: bool,
        /// A ceiling on the nodes one instance may spend, so that a wide draw
        /// cannot turn one case into a long run.
        budget: u32,
        nodes: u32,
        /// Nodes whose bound was actually compared, which is what keeps a walk
        /// that closed at the root from reporting a pass.
        checked: u32,
    }

    impl<'a> BoundWalk<'a> {
        fn new(cfn: &'a Cfn, level: ConsistencyLevel, top: Cost, moving: bool) -> Self {
            Self {
                cfn,
                level,
                cub: top,
                moving,
                budget: 256,
                nodes: 0,
                checked: 0,
            }
        }

        /// The first variable with a choice left, and the first value to try.
        fn branch(network: &Network) -> Option<(VarId, ValId)> {
            network
                .variable_ids()
                .find(|&var| network.domain(var).len() >= 2)
                .and_then(|var| network.domain(var).into_iter().next().map(|v| (var, v)))
        }

        fn node(&mut self, network: &mut Network) -> Result<(), TestCaseError> {
            if self.nodes >= self.budget {
                return Ok(());
            }
            self.nodes += 1;

            // The subproblem beneath this node is the one the decisions on the
            // path name, so it is read *before* filtering. Reading it afterwards
            // would let a value enforcement wrongly pruned leave the comparison
            // that exists to catch exactly that.
            let choices = domain_lists(network);
            let optimum = subproblem_optimum(self.cfn, &choices);

            network.reset_contributions();
            network.set_top(self.cub);
            let alive = network.enforce(self.level);
            prop_assert!(
                !network.budget_exhausted(),
                "{} ran out of steps",
                self.level.label()
            );
            self.checked += 1;

            if !alive {
                // Closing a node claims every assignment beneath it costs at
                // least the bound it was closed against.
                prop_assert!(
                    optimum >= self.cub,
                    "{} closed a node holding an assignment costing {:?} at ⊤ = {:?}",
                    self.level.label(),
                    optimum,
                    self.cub
                );
                return Ok(());
            }

            prop_assert!(
                network.c_empty() <= optimum,
                "{} left a bound of {:?} above the subproblem optimum {:?}",
                self.level.label(),
                network.c_empty(),
                optimum
            );

            let Some((var, value)) = Self::branch(network) else {
                // Every domain is a singleton, so the node is one assignment.
                if self.moving {
                    let leaf = subproblem_optimum(self.cfn, &domain_lists(network));
                    self.cub = self.cub.min(leaf);
                }
                return Ok(());
            };

            let saved = network.domains().clone();

            let mark = network.mark();
            network.assign(var, value);
            self.node(network)?;
            network.restore(mark);
            network.set_domains(&saved);

            let mark = network.mark();
            network.refute(var, value);
            self.node(network)?;
            network.restore(mark);
            network.set_domains(&saved);

            Ok(())
        }
    }

    fn predicate_holds(network: &Network, level: ConsistencyLevel) -> bool {
        match level {
            ConsistencyLevel::Node => network.is_nc_star(),
            ConsistencyLevel::Arc => network.is_ac_star(),
            ConsistencyLevel::DirectionalArc => network.is_dac_star(),
            ConsistencyLevel::FullDirectionalArc => network.is_fdac_star(),
            ConsistencyLevel::ExistentialDirectionalArc => network.is_edac_star(),
        }
    }

    /// The two generators written for the properties below produce the shapes
    /// those properties need.
    ///
    /// Three of the properties assert only that costs did not move, which is
    /// what a transformation that did nothing also reports, and two of them
    /// walk a tree that could be one node deep. Sampling the generators and
    /// counting is what keeps any of the five from passing over a domain that
    /// stopped exercising the thing it names.
    ///
    /// The thresholds sit far below the rates these strategies are written to
    /// produce, so a failure here means a strategy stopped reaching a shape
    /// rather than that sampling varied.
    #[test]
    fn the_solver_generators_reach_the_cases_they_exist_for() {
        use proptest::strategy::ValueTree;
        use proptest::test_runner::TestRunner;

        const DRAWS: usize = 128;
        let mut runner = TestRunner::deterministic();

        let instances = arb_ept_instance();
        let mut moved_cells = 0usize;
        let mut extend_saturated = 0usize;
        for _ in 0..DRAWS {
            let (cfn, top, script) = instances.new_tree(&mut runner).unwrap().current();
            let mut network = Network::from_cfn(&cfn, top);
            let mut draw = Draw::new(script);

            // Every draw offers a site for each of the three.
            assert!(!project_sites(&network).is_empty());
            assert!(!extend_sites(&network).is_empty());
            assert!(network.n_variables() >= 2);

            // And a transformation applied at one of those sites does move
            // cost, so the cost-vector properties are comparing a network
            // against a network that changed.
            let sites = project_sites(&network);
            let (function, var, value) = pick(&sites, &mut draw).unwrap();
            let alpha = network.min_over_tuples(function, var, value);
            let before = network.cells().to_vec();
            network.project(function, var, value, alpha);
            if network.cells() != before.as_slice() {
                moved_cells += 1;
            }

            // The saturation subterm fires with `α = ⊥`, which is the behaviour
            // the module docs warn reads like a bug.
            let sites = extend_sites(&network);
            let (var, value, function) = pick(&sites, &mut draw).unwrap();
            let before = network.cells().to_vec();
            network.extend(var, value, function, Cost::BOT);
            if network.cells() != before.as_slice() {
                extend_saturated += 1;
            }
        }
        assert!(
            moved_cells > 32,
            "Project moved cost on only {moved_cells} of {DRAWS} draws"
        );
        assert!(
            extend_saturated > 8,
            "Extend at ⊥ saturated a tuple on only {extend_saturated} of {DRAWS} draws"
        );

        // The bound walk has to descend, or it checks the root and nothing else.
        let instances = arb_node_instance();
        let mut total_nodes = 0u32;
        let mut deepest = 0u32;
        for _ in 0..DRAWS {
            let (cfn, top) = instances.new_tree(&mut runner).unwrap().current();
            let mut network = Network::from_cfn(&cfn, top);
            let mut walk =
                BoundWalk::new(&cfn, ConsistencyLevel::ExistentialDirectionalArc, top, true);
            walk.node(&mut network).unwrap();
            total_nodes += walk.checked;
            deepest = deepest.max(walk.checked);
        }
        assert!(
            total_nodes > u32::try_from(DRAWS).unwrap() * 4,
            "the walk checked {total_nodes} nodes over {DRAWS} instances"
        );
        assert!(deepest > 8, "the deepest walk checked only {deepest} nodes");
    }

    // -- the properties ----------------------------------------------------

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// `Project` preserves the cost of **every** assignment.
        ///
        /// `one_ept_preserves_the_cost_vector` draws which of the three to
        /// apply and applies nothing when the draw names one the network cannot
        /// offer, so it states the property over a random mixture rather than
        /// over each transformation. These three state it per transformation
        /// and assert that a site existed, so none of them can pass by having
        /// done nothing.
        ///
        /// This is the highest-value shape of test available for this solver:
        /// a transformation that shifted cost incorrectly would produce a wrong
        /// optimum, and an outcome test run on a different instance would agree
        /// with an oracle that had been handed the same corrupted network.
        #[test]
        fn project_preserves_the_whole_cost_vector((cfn, top, script) in arb_ept_instance()) {
            let mut network = Network::from_cfn(&cfn, top);
            let mut draw = Draw::new(script);
            let sites = project_sites(&network);
            let Some((function, var, value)) = pick(&sites, &mut draw) else {
                prop_assert!(false, "the generator must offer a Project site");
                return Ok(());
            };
            let alpha = fraction(network.min_over_tuples(function, var, value), draw.take(5));
            let before = cost_vector(&network);
            network.project(function, var, value, alpha);
            prop_assert_eq!(before, cost_vector(&network));
        }

        /// The same for `Extend`, whose saturation subterm rewrites tuples even
        /// when `α = ⊥`, so "changed nothing" is not the shape of a pass here.
        #[test]
        fn extend_preserves_the_whole_cost_vector((cfn, top, script) in arb_ept_instance()) {
            let mut network = Network::from_cfn(&cfn, top);
            let mut draw = Draw::new(script);
            let sites = extend_sites(&network);
            let Some((var, value, function)) = pick(&sites, &mut draw) else {
                prop_assert!(false, "the generator must offer an Extend site");
                return Ok(());
            };
            let alpha = fraction(network.unary_cost(var, value), draw.take(5));
            let before = cost_vector(&network);
            network.extend(var, value, function, alpha);
            prop_assert_eq!(before, cost_vector(&network));
        }

        /// And for `UnaryProject`, the one transformation that writes `c_∅`,
        /// hence the one whose error would show up directly as a wrong bound.
        #[test]
        fn unary_project_preserves_the_whole_cost_vector(
            (cfn, top, script) in arb_ept_instance()
        ) {
            let mut network = Network::from_cfn(&cfn, top);
            let mut draw = Draw::new(script);
            let variables: Vec<VarId> = network.variable_ids().collect();
            let Some(var) = pick(&variables, &mut draw) else {
                prop_assert!(false, "the generator must offer a variable");
                return Ok(());
            };
            let Some(most) = network.min_unary(var) else {
                prop_assert!(false, "every variable of the generator has a domain");
                return Ok(());
            };
            let alpha = fraction(most, draw.take(5));
            let before = cost_vector(&network);
            network.unary_project(var, alpha);
            prop_assert_eq!(before, cost_vector(&network));
        }

        /// `c_∅` after enforcement is no greater than the true optimum of the
        /// subproblem beneath the node, at every node of a search.
        ///
        /// This is what makes pruning sound, and it is the one claim an outcome
        /// test cannot reach. A bound that is ever above the optimum of the
        /// subtree beneath it prunes the subtree holding the optimum, and the
        /// search then returns a plausible wrong answer: no assertion fires, no
        /// domain empties, and an oracle run on the same instance agrees with
        /// the reported cost of the assignment that was actually returned.
        ///
        /// Checked at a fixed `⊤`, so that every node's bound is compared
        /// against the optimum of its own subproblem rather than against a
        /// bound an incumbent had already lowered.
        #[test]
        fn the_bound_is_a_lower_bound_at_every_node((cfn, top) in arb_node_instance()) {
            for level in ConsistencyLevel::ALL {
                let mut network = Network::from_cfn(&cfn, top);
                let mut walk = BoundWalk::new(&cfn, level, top, false);
                walk.node(&mut network)?;
                prop_assert!(walk.checked >= 1, "{} checked no node", level.label());
            }
        }

        /// The same with the primal bound moving, which is the search as it
        /// actually runs.
        ///
        /// A lowered `⊤` makes costs recorded under the old bound read as `⊤`
        /// under the new one, which is the mechanism that turns an improving
        /// solution into pruning power and the mechanism most likely to lose a
        /// unit of cost on the way. Here a node closed at `⊤` also has to
        /// justify itself: every assignment beneath it must cost at least the
        /// bound it was closed against.
        #[test]
        fn the_bound_is_a_lower_bound_under_a_moving_top((cfn, top) in arb_node_instance()) {
            for level in ConsistencyLevel::ALL {
                let mut network = Network::from_cfn(&cfn, top);
                let mut walk = BoundWalk::new(&cfn, level, top, true);
                walk.node(&mut network)?;
                prop_assert!(walk.checked >= 1, "{} checked no node", level.label());
            }
        }

        /// The definition of equivalence, literally: the cost of **every**
        /// assignment is unchanged by a transformation, not merely the least
        /// one.
        ///
        /// This is the highest-value test here. A transformation that preserved
        /// only the optimum would still let a search return an assignment whose
        /// cost in the original network is not the number the search reported.
        #[test]
        fn one_ept_preserves_the_cost_vector((cfn, top, script) in arb_instance_and_script()) {
            let mut network = Network::from_cfn(&cfn, top);
            let mut draw = Draw::new(script);
            let before = cost_vector(&network);
            apply_one_ept(&mut network, &mut draw);
            let after = cost_vector(&network);
            prop_assert_eq!(before, after);
        }

        /// The same over a sequence, since equivalence composes and a bug that
        /// cancels within one transformation may not across several.
        #[test]
        fn a_sequence_of_epts_preserves_the_cost_vector(
            (cfn, top, script) in arb_instance_and_script()
        ) {
            let mut network = Network::from_cfn(&cfn, top);
            let mut draw = Draw::new(script);
            let before = cost_vector(&network);
            for _ in 0..24 {
                apply_one_ept(&mut network, &mut draw);
            }
            let after = cost_vector(&network);
            prop_assert_eq!(before, after);
        }

        /// Enforcing a level preserves the cost vector too, and never lowers
        /// the constant.
        #[test]
        fn enforcing_a_level_preserves_the_cost_vector((cfn, top) in arb_instance()) {
            for level in ConsistencyLevel::ALL {
                let mut network = Network::from_cfn(&cfn, top);
                let before = cost_vector(&network);
                let constant = network.c_empty();
                network.enforce(level);
                prop_assert!(!network.budget_exhausted(), "{} ran out of steps", level.label());
                prop_assert!(network.c_empty() >= constant);
                // Enforcement prunes, and a pruned value leaves the assignment
                // space, so the comparison is over the assignments that
                // survived.
                for tuple in every_tuple(&network) {
                    let index = tuple_index(&cfn, &tuple);
                    prop_assert_eq!(
                        network.valuation(&tuple),
                        before[index],
                        "{} changed the cost of an assignment",
                        level.label()
                    );
                }
            }
        }

        /// Every value a level prunes was infeasible under the network's `⊤`.
        #[test]
        fn pruning_only_removes_infeasible_values((cfn, top) in arb_instance()) {
            for level in ConsistencyLevel::ALL {
                let mut plain = Network::from_cfn(&cfn, top);
                let mut network = Network::from_cfn(&cfn, top);
                network.enforce(level);
                for var in network.variable_ids() {
                    for value in plain.domain(var) {
                        if network.domain(var).contains(value) {
                            continue;
                        }
                        // Every assignment through the pruned value costs ⊤.
                        for tuple in every_tuple(&plain) {
                            if tuple.get(var.index()) == Some(&value) {
                                prop_assert!(plain.valuation(&tuple) >= top);
                            }
                        }
                    }
                }
                plain.set_top(top);
            }
        }

        /// After enforcement the level's own predicate holds, checked by code
        /// written from the definition rather than from the enforcer.
        #[test]
        fn enforcement_establishes_its_own_predicate((cfn, top) in arb_instance()) {
            for level in ConsistencyLevel::ALL {
                let mut network = Network::from_cfn(&cfn, top);
                if network.enforce(level) {
                    prop_assert!(
                        predicate_holds(&network, level),
                        "{} did not hold after enforcing it",
                        level.label()
                    );
                }
            }
        }

        /// The strength chain, as a comparison of the bound each level reaches.
        ///
        /// `NC* ⪯ AC* ⪯ FDAC* ⪯ EDAC*` and `NC* ⪯ DAC* ⪯ FDAC*`. Arc and
        /// directional consistency are incomparable, so they are not compared.
        #[test]
        fn the_bound_is_ordered_by_level((cfn, top) in arb_instance()) {
            let bound = |level| {
                let mut network = Network::from_cfn(&cfn, top);
                network.enforce(level);
                network.c_empty()
            };
            let node = bound(ConsistencyLevel::Node);
            let arc = bound(ConsistencyLevel::Arc);
            let directional = bound(ConsistencyLevel::DirectionalArc);
            let full = bound(ConsistencyLevel::FullDirectionalArc);
            let existential = bound(ConsistencyLevel::ExistentialDirectionalArc);
            prop_assert!(node <= arc);
            prop_assert!(node <= directional);
            prop_assert!(arc <= full);
            prop_assert!(full <= existential);
            // Directional consistency alone is not compared against the two
            // levels above it: `enforce_fdac_star` shares its prefix with
            // `enforce_ac_star`, not with `enforce_dac_star`, and closures are
            // not unique, so no ordering follows.
            let _ = directional;
        }

        /// Push a mark, transform, pop, and the network is what it was.
        ///
        /// Nested to twenty levels, because a trail bug is invisible until the
        /// search is deep enough to expose it and then presents as a wrong
        /// answer on large inputs only.
        #[test]
        fn the_trail_round_trips((cfn, top, script) in arb_instance_and_script()) {
            let mut network = Network::from_cfn(&cfn, top);
            let mut draw = Draw::new(script);
            let mut marks = Vec::new();
            let mut hashes = Vec::new();
            let mut states = Vec::new();

            for _ in 0..20 {
                hashes.push(network.structural_hash());
                states.push(network.cells().to_vec());
                marks.push(network.mark());
                for _ in 0..3 {
                    apply_one_ept(&mut network, &mut draw);
                }
            }

            while let (Some(mark), Some(hash), Some(state)) =
                (marks.pop(), hashes.pop(), states.pop())
            {
                network.restore(mark);
                prop_assert_eq!(network.structural_hash(), hash);
                prop_assert_eq!(network.cells(), state.as_slice());
            }
        }
    }

    /// Where an assignment sits in the order [`every_tuple`] walks the full
    /// domains in.
    fn tuple_index(cfn: &Cfn, tuple: &[ValId]) -> usize {
        let mut index = 0usize;
        for (variable, value) in cfn.variables().iter().zip(tuple) {
            let slot = variable.slot(*value).unwrap();
            index = index * variable.slots() + slot;
        }
        index
    }
}
