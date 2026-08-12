//! Bucket elimination in the `(min, ⊕)` semiring, and the three readings of it
//! the search needs.
//!
//! [`eliminate`] runs Dechter's backward sweep: each variable in turn absorbs
//! every cost function whose scope it closes, and hands the rest of the network
//! one message over the variables it shared them with. [`decode`] runs the
//! forward sweep, reading an argmin back out of the messages in one greedy pass
//! that never backtracks. Between them they are exact for any objective that is
//! a `⊕`-sum of local terms, at `d^(w + 1)` operations and `d^w` cost table
//! entries, where `w` is the induced width of the order they run under.
//!
//! Exactness needs one identity and nothing else: `⊕` distributes over `min`.
//! Everything here is a consequence of it, including the fact that elimination
//! is never approximate. It is only ever unaffordable, which is what
//! [`order::fits_budget`](super::order::fits_budget) decides in advance.
//!
//! # The two sweeps, and the names
//!
//! Dechter calls the elimination sweep backward and the recovery sweep forward.
//! The names here are [`eliminate`] and [`decode`], which say which sweep is
//! meant without depending on which end of the order is called the front.
//! [`eliminate`] walks the elimination sequence forwards, from the variable
//! eliminated first to the variable eliminated last; [`decode`] walks it
//! backwards.
//!
//! # Two decisions worth stating
//!
//! **Argmin recovery is by recomputation, not by stored argmin tables.**
//! Storing the argmin alongside each message entry would turn [`decode`] into a
//! table lookup, at the price of a second table the size of every message. Peak
//! memory is the binding constraint here and the recomputation is
//! `O((r + n) · d)`, which is negligible against the `d^(w + 1)` the sweep
//! already paid.
//!
//! **The loop nest never materialises the join.** A bucket eliminating `X_p`
//! holds terms over `{X_p} ∪ U_p`, and the obvious implementation joins them
//! into one table over `{X_p} ∪ U_p` and then minimises out `X_p`. That
//! allocates `d^(|U_p| + 1)` entries to produce `d^|U_p|`. The nest in
//! the sweep runs `U_p` outside and `X_p` inside instead, accumulating
//! `⊕` and folding with `min` as it goes, so the only table it allocates is the
//! message. [`Buckets::peak_cells`] reports the largest allocation the sweep
//! actually made, so the claim is measured rather than asserted in prose.
//!
//! # One sweep, three semirings
//!
//! The sweep is written once, against a private semiring trait, and
//! instantiated twice. `(min, ⊕)` gives the optimum. `(Σ, ×)` over indicator
//! values gives [`count_solutions`], the exact number of feasible assignments.
//! [`detect_product`] is the third reading and needs no sweep at all: it
//! reports when every constraint is universal, so that the feasible set is a
//! full Cartesian product of the domains and the count is their product. That
//! shape is a property of the data rather than a bug signature, which is why it
//! is a diagnostic and not an assertion.

use super::cfn::{Cfn, CostFunction, Domain, Variable};
use super::cost::Cost;
use super::order::{Graph, is_permutation};
use super::{Assignment, ValId, VarId};

// ---------------------------------------------------------------------------
// The semiring
// ---------------------------------------------------------------------------

/// The algebra one sweep runs in.
///
/// `times` is `⊗`, the operation that combines the terms of one assignment;
/// `plus` is `⊕`, the operation that marginalises a variable away. Correctness
/// of the sweep needs `times` to distribute over `plus` and nothing more.
///
/// [`Self::ABSORB`] plays two roles at once, and in both instantiations one
/// value fills both: it annihilates `times` and it is the identity of `plus`.
/// That coincidence is what lets the inner loop break early on it.
trait Semiring: Copy + PartialEq {
    /// The identity of `times`, and the value an empty product reads.
    const UNIT: Self;

    /// The annihilator of `times` and the identity of `plus`.
    const ABSORB: Self;

    /// `⊗`.
    fn times(self, other: Self) -> Self;

    /// `⊕`.
    fn plus(self, other: Self) -> Self;

    /// Read one cost table entry into the algebra.
    fn of(entry: Cost) -> Self;
}

impl Semiring for Cost {
    const UNIT: Self = Self::BOT;
    const ABSORB: Self = Self::TOP_SENTINEL;

    #[inline]
    fn times(self, other: Self) -> Self {
        self.combine(other, Self::TOP_SENTINEL)
    }

    #[inline]
    fn plus(self, other: Self) -> Self {
        if self <= other { self } else { other }
    }

    #[inline]
    fn of(entry: Cost) -> Self {
        entry
    }
}

/// A count of assignments, in the `(Σ, ×)` instantiation.
///
/// Saturating rather than wrapping: a count that has run past `u128` reads
/// [`COUNT_CEILING`], which is recognisably a ceiling, where a wrapped count
/// would read as a plausible small number.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct Count(u128);

impl Semiring for Count {
    const UNIT: Self = Self(1);
    const ABSORB: Self = Self(0);

    #[inline]
    fn times(self, other: Self) -> Self {
        Self(self.0.saturating_mul(other.0))
    }

    #[inline]
    fn plus(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    #[inline]
    fn of(entry: Cost) -> Self {
        // A term forbids a tuple exactly when its entry is `⊤`. Every finite
        // cost, however large, leaves the tuple feasible and so counts once.
        if entry == Cost::TOP_SENTINEL {
            Self(0)
        } else {
            Self(1)
        }
    }
}

/// The value [`count_solutions`] and [`ProductVerdict::Product`] report when
/// the true count is above `u128`.
///
/// A saturated count is not distinguishable from a true count of exactly
/// `u128::MAX`, and neither API tries to be: reaching that value honestly would
/// need a network of some 2¹²⁸ assignments, which no schema pair produces and
/// no machine could enumerate. So a count reading this is a ceiling in every
/// case that occurs, and a caller comparing two counts that both read it is
/// comparing two ceilings rather than two numbers.
pub const COUNT_CEILING: u128 = u128::MAX;

// ---------------------------------------------------------------------------
// Terms and tables
// ---------------------------------------------------------------------------

/// One term sitting in a bucket.
///
/// Input terms are named by where they live in the network rather than copied,
/// so a bucket costs one word per term however large its table is.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Term {
    /// The unary cost table of a variable.
    Unary(VarId),
    /// A cost function of the network, by index into [`Cfn::functions`].
    Function(usize),
    /// The message produced at a bucket, by that bucket's position.
    Message(usize),
}

/// A table over a scope, laid out exactly as the network lays out its own: row
/// major, with the last scope variable varying fastest, and one slot per value
/// including `⊥`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Table<S> {
    scope: Vec<VarId>,
    entries: Vec<S>,
}

/// A term as the sweep reads it: a scope and a table it can index.
///
/// Input tables are `Cost` whatever the sweep's algebra is, so they are read
/// through [`Semiring::of`] at the point of use rather than converted up front.
/// That keeps the sweep from allocating a second copy of the whole network.
enum View<'a, S> {
    /// A table the network owns.
    Input {
        scope: &'a [VarId],
        table: &'a [Cost],
    },
    /// A table an earlier bucket produced.
    Message { scope: &'a [VarId], table: &'a [S] },
}

impl<'a, S: Semiring> View<'a, S> {
    /// The variables the term constrains, in ascending order.
    const fn scope(&self) -> &'a [VarId] {
        match self {
            Self::Input { scope, .. } | Self::Message { scope, .. } => scope,
        }
    }

    /// The entry at a table offset.
    ///
    /// An offset outside the table reads [`Semiring::ABSORB`], which forbids
    /// the tuple. Every offset the sweep computes is inside, since it is built
    /// from the same slot counts the table was sized by; reading absent as
    /// forbidden is the failure direction that cannot make an infeasible
    /// assignment look feasible.
    fn at(&self, offset: usize) -> S {
        match self {
            Self::Input { table, .. } => table.get(offset).copied().map_or(S::ABSORB, S::of),
            Self::Message { table, .. } => table.get(offset).copied().unwrap_or(S::ABSORB),
        }
    }
}

// ---------------------------------------------------------------------------
// Bucket placement
// ---------------------------------------------------------------------------

/// Which bucket every term of the network belongs to.
#[derive(Clone, Debug)]
struct Placement {
    /// Where each variable sits in the elimination sequence, by variable index.
    position: Vec<usize>,
    /// The terms of each bucket, by position in the elimination sequence.
    buckets: Vec<Vec<Term>>,
}

/// Place every input term in its bucket.
///
/// Definition 1: a term belongs to the bucket of the variable of its scope that
/// is eliminated first, so that when that bucket is processed no later term
/// mentions the variable being eliminated. Every term lands in exactly one
/// bucket, and placement is linear in the total scope size.
///
/// # Panics
///
/// If `order` is not a permutation of the network's variables.
fn place(cfn: &Cfn, order: &[VarId]) -> Placement {
    let count = cfn.n_variables();
    assert!(
        is_permutation(order, count),
        "an elimination order must list every variable of the network exactly once"
    );

    let mut position = vec![0usize; count];
    for (index, var) in order.iter().enumerate() {
        position[var.index()] = index;
    }

    let mut buckets: Vec<Vec<Term>> = vec![Vec::new(); count];
    for var in cfn.variable_ids() {
        buckets[position[var.index()]].push(Term::Unary(var));
    }
    for (index, function) in cfn.functions().iter().enumerate() {
        let target = earliest(function.scope(), &position).unwrap_or(0);
        buckets[target].push(Term::Function(index));
    }

    Placement { position, buckets }
}

/// The position of the variable of a scope that is eliminated first.
fn earliest(scope: &[VarId], position: &[usize]) -> Option<usize> {
    scope
        .iter()
        .filter_map(|var| position.get(var.index()).copied())
        .min()
}

// ---------------------------------------------------------------------------
// The sweep
// ---------------------------------------------------------------------------

/// What one sweep produced.
struct Sweep<S> {
    buckets: Vec<Vec<Term>>,
    messages: Vec<Option<Table<S>>>,
    optimum: S,
    width: usize,
    peak_cells: usize,
    total_cells: usize,
}

/// Run the backward sweep in one algebra.
///
/// Buckets are processed in elimination order. Each one absorbs its variable
/// and sends one message to the bucket of the earliest-eliminated variable of
/// the message's own scope, which is strictly later than its own, so the sweep
/// terminates in one pass. A bucket whose message has an empty scope has
/// produced a scalar instead; those accumulate into the constant, which is what
/// the network's own `c_∅` joins to give the answer.
fn run<S: Semiring>(cfn: &Cfn, order: &[VarId], placement: &Placement) -> Sweep<S> {
    let count = order.len();
    let all_vars: Vec<VarId> = cfn.variable_ids().collect();
    let mut buckets = placement.buckets.clone();
    let mut messages: Vec<Option<Table<S>>> = (0..count).map(|_| None).collect();

    let mut constant = S::UNIT;
    let mut width = 0usize;
    let mut peak_cells = 0usize;
    let mut total_cells = 0usize;

    for position in 0..count {
        let eliminated = order[position];
        // Taken out and put back so that the message this bucket produces can
        // be pushed into a later bucket while its own terms are being read.
        let terms = std::mem::take(&mut buckets[position]);
        let (scope, entries) = sweep_bucket::<S>(cfn, &all_vars, &messages, &terms, eliminated);
        buckets[position] = terms;

        peak_cells = peak_cells.max(entries.len());
        total_cells += entries.len();
        width = width.max(scope.len());

        if scope.is_empty() {
            constant = constant.times(entries.first().copied().unwrap_or(S::ABSORB));
            continue;
        }
        let target = earliest(&scope, &placement.position).unwrap_or(position);
        buckets[target].push(Term::Message(position));
        messages[position] = Some(Table { scope, entries });
    }

    Sweep {
        buckets,
        messages,
        optimum: S::of(cfn.c_empty()).times(constant),
        width,
        peak_cells,
        total_cells,
    }
}

/// The layout of one bucket's loop nest.
struct BucketPlan {
    /// `U_p`, the message scope, in ascending variable order.
    scope: Vec<VarId>,
    /// The slot count of each message scope variable.
    widths: Vec<usize>,
    /// How many entries the message has.
    length: usize,
    /// Per term, the `(index into `scope`, stride)` pairs of the variables it
    /// shares with the message.
    joins: Vec<Vec<(usize, usize)>>,
    /// Per term, the stride of the variable being eliminated.
    own: Vec<usize>,
}

/// Work out `U_p` and the strides the nest will index with.
fn plan_bucket<S: Semiring>(cfn: &Cfn, views: &[View<'_, S>], eliminated: VarId) -> BucketPlan {
    let mut scope: Vec<VarId> = Vec::new();
    for view in views {
        for var in view.scope() {
            if *var != eliminated {
                scope.push(*var);
            }
        }
    }
    scope.sort_unstable();
    scope.dedup();

    let widths: Vec<usize> = scope.iter().map(|var| slots(cfn, *var)).collect();
    let length = widths
        .iter()
        .try_fold(1usize, |total, width| total.checked_mul(*width));
    let length = length.unwrap_or_else(|| {
        panic!(
            "a message over {} variables does not fit in memory",
            scope.len()
        )
    });

    let mut joins = Vec::with_capacity(views.len());
    let mut own = Vec::with_capacity(views.len());
    for view in views {
        let strides = strides_of(cfn, view.scope());
        let mut join = Vec::with_capacity(view.scope().len());
        let mut mine = 0usize;
        for (index, var) in view.scope().iter().enumerate() {
            let stride = strides.get(index).copied().unwrap_or(0);
            if *var == eliminated {
                mine = stride;
            } else if let Some(slot) = scope.iter().position(|other| other == var) {
                join.push((slot, stride));
            }
        }
        joins.push(join);
        own.push(mine);
    }

    BucketPlan {
        scope,
        widths,
        length,
        joins,
        own,
    }
}

/// Eliminate one variable, producing the message its bucket sends on.
///
/// The nest is the reason peak transient allocation is `d^|U_p|` rather than
/// `d^(|U_p| + 1)`. `cell` walks the message's own index space on the outside,
/// and the eliminated variable's domain is walked on the inside, with the `⊗`
/// accumulator `total` and the `⊕` accumulator `best` living in registers. No
/// table over `{X_p} ∪ U_p` is ever built, so the single `entries` allocation
/// below is the whole memory cost of the bucket.
///
/// # Panics
///
/// If the message's entry count overflows `usize`, which means the width is far
/// past anything a budget would have admitted.
fn sweep_bucket<S: Semiring>(
    cfn: &Cfn,
    all_vars: &[VarId],
    messages: &[Option<Table<S>>],
    terms: &[Term],
    eliminated: VarId,
) -> (Vec<VarId>, Vec<S>) {
    let views: Vec<View<'_, S>> = terms
        .iter()
        .map(|term| view(cfn, all_vars, messages, *term))
        .collect();
    let plan = plan_bucket(cfn, &views, eliminated);

    let variable = cfn.variable(eliminated);
    let domain = cfn.domain(eliminated).unwrap_or(Domain::EMPTY);
    let mut entries = vec![S::ABSORB; plan.length];
    let mut counters = vec![0usize; plan.scope.len()];
    let mut bases = vec![0usize; views.len()];

    for cell in &mut entries {
        for (base, join) in bases.iter_mut().zip(&plan.joins) {
            *base = join
                .iter()
                .map(|(index, stride)| stride * counters.get(*index).copied().unwrap_or(0))
                .sum();
        }

        let mut best = S::ABSORB;
        for value in domain {
            let Some(slot) = variable.and_then(|variable| variable.slot(value)) else {
                continue;
            };
            let mut total = S::UNIT;
            for ((view, base), own) in views.iter().zip(&bases).zip(&plan.own) {
                total = total.times(view.at(base + own * slot));
                if total == S::ABSORB {
                    break;
                }
            }
            best = best.plus(total);
        }
        *cell = best;
        tick(&mut counters, &plan.widths);
    }

    (plan.scope, entries)
}

/// Advance a mixed-radix odometer by one, last position fastest.
///
/// Walking the odometer in step with a linear index is what makes the linear
/// index the row-major offset of the tuple the counters hold, with no division.
fn tick(counters: &mut [usize], widths: &[usize]) {
    for index in (0..counters.len()).rev() {
        counters[index] += 1;
        if counters[index] < widths.get(index).copied().unwrap_or(0) {
            return;
        }
        counters[index] = 0;
    }
}

/// The term a bucket entry names, as the sweep reads it.
fn view<'a, S: Semiring>(
    cfn: &'a Cfn,
    all_vars: &'a [VarId],
    messages: &'a [Option<Table<S>>],
    term: Term,
) -> View<'a, S> {
    match term {
        Term::Unary(var) => View::Input {
            scope: all_vars.get(var.index()..=var.index()).unwrap_or_default(),
            table: cfn.unary(var).unwrap_or_default(),
        },
        Term::Function(index) => cfn.functions().get(index).map_or(
            View::Input {
                scope: &[],
                table: &[],
            },
            |function| View::Input {
                scope: function.scope(),
                table: function.table(),
            },
        ),
        Term::Message(position) => messages.get(position).and_then(Option::as_ref).map_or(
            View::Input {
                scope: &[],
                table: &[],
            },
            |table| View::Message {
                scope: &table.scope,
                table: &table.entries,
            },
        ),
    }
}

/// How many table slots a variable spans, `⊥` included.
fn slots(cfn: &Cfn, var: VarId) -> usize {
    cfn.variable(var).map_or(0, Variable::slots)
}

/// The stride of each scope position in a row-major table over a scope.
fn strides_of(cfn: &Cfn, scope: &[VarId]) -> Vec<usize> {
    let mut out = vec![1usize; scope.len()];
    let mut stride = 1usize;
    for index in (0..scope.len()).rev() {
        out[index] = stride;
        stride = stride.saturating_mul(slots(cfn, scope[index]));
    }
    out
}

// ---------------------------------------------------------------------------
// The result of elimination
// ---------------------------------------------------------------------------

/// The buckets a completed elimination left behind, and the optimum it read.
///
/// This is everything [`decode`] and [`all_optima`] need. It holds the messages
/// and the placement rather than the network, so it is only meaningful against
/// the network it was computed from; passing a different one is a caller error
/// that [`decode`] checks for on the order and cannot check for on the tables.
#[derive(Clone, Debug)]
pub struct Buckets {
    order: Vec<VarId>,
    all_vars: Vec<VarId>,
    terms: Vec<Vec<Term>>,
    messages: Vec<Option<Table<Cost>>>,
    optimum: Cost,
    width: usize,
    peak_cells: usize,
    total_cells: usize,
}

impl Buckets {
    /// The elimination sequence this was computed under.
    #[inline]
    #[must_use]
    pub fn order(&self) -> &[VarId] {
        &self.order
    }

    /// The optimum, `c_∅` included.
    ///
    /// [`Cost::TOP_SENTINEL`] means no assignment is feasible.
    #[inline]
    #[must_use]
    pub const fn optimum(&self) -> Cost {
        self.optimum
    }

    /// The largest message arity the sweep produced.
    ///
    /// Equal to the induced width of the order over the network's primal graph,
    /// which is what makes
    /// [`induced_width`](super::order::induced_width) a prediction of this
    /// number rather than a separate estimate of it.
    #[inline]
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// How many buckets there are, which is how many variables there are.
    #[inline]
    #[must_use]
    pub fn n_buckets(&self) -> usize {
        self.terms.len()
    }

    /// The largest single cost table the sweep allocated, in entries.
    ///
    /// Every table the sweep allocates is a message, so this is
    /// `max_p d^|U_p|`. A nest that materialised the join before minimising
    /// would allocate `d^(|U_p| + 1)` at the widest bucket and this number
    /// would be a factor of `d` larger.
    #[inline]
    #[must_use]
    pub const fn peak_cells(&self) -> usize {
        self.peak_cells
    }

    /// Every cost table entry the sweep allocated, summed over buckets.
    #[inline]
    #[must_use]
    pub const fn total_cells(&self) -> usize {
        self.total_cells
    }

    /// The scope of every term in one bucket, in the order they were placed.
    ///
    /// A diagnostic: it is how a test reads Definition 1 back off a completed
    /// elimination. `None` if the position is not a bucket.
    #[must_use]
    pub fn bucket_scopes(&self, cfn: &Cfn, position: usize) -> Option<Vec<Vec<VarId>>> {
        let terms = self.terms.get(position)?;
        Some(
            terms
                .iter()
                .map(|term| self.scope_of(cfn, *term).to_vec())
                .collect(),
        )
    }

    /// The scope of the message one bucket produced, or `None` if it produced a
    /// scalar.
    #[must_use]
    pub fn message_scope(&self, position: usize) -> Option<&[VarId]> {
        self.messages
            .get(position)?
            .as_ref()
            .map(|table| table.scope.as_slice())
    }

    /// The table of the message one bucket produced.
    #[must_use]
    pub fn message_table(&self, position: usize) -> Option<&[Cost]> {
        self.messages
            .get(position)?
            .as_ref()
            .map(|table| table.entries.as_slice())
    }

    /// The scope of a term.
    fn scope_of<'a>(&'a self, cfn: &'a Cfn, term: Term) -> &'a [VarId] {
        match term {
            Term::Unary(var) => self
                .all_vars
                .get(var.index()..=var.index())
                .unwrap_or_default(),
            Term::Function(index) => cfn
                .functions()
                .get(index)
                .map_or(&[][..], CostFunction::scope),
            Term::Message(position) => self
                .messages
                .get(position)
                .and_then(Option::as_ref)
                .map_or(&[][..], |table| table.scope.as_slice()),
        }
    }

    /// The entry of a term at a table offset.
    fn entry_of(&self, cfn: &Cfn, term: Term, offset: usize) -> Cost {
        let entry = match term {
            Term::Unary(var) => cfn.unary(var).and_then(|table| table.get(offset)),
            Term::Function(index) => cfn
                .functions()
                .get(index)
                .and_then(|function| function.table().get(offset)),
            Term::Message(position) => self
                .messages
                .get(position)
                .and_then(Option::as_ref)
                .and_then(|table| table.entries.get(offset)),
        };
        entry.copied().unwrap_or(Cost::TOP_SENTINEL)
    }

    /// The `⊕`-sum of one bucket's terms at a fully assigned tuple.
    ///
    /// Only the variables in the bucket's terms are read, and every one of them
    /// other than the bucket's own is eliminated later and so already assigned
    /// when [`decode`] reaches this bucket.
    fn score(&self, cfn: &Cfn, position: usize, values: &[ValId]) -> Cost {
        let top = Cost::TOP_SENTINEL;
        let Some(terms) = self.terms.get(position) else {
            return top;
        };
        let mut total = Cost::BOT;
        for term in terms {
            let scope = self.scope_of(cfn, *term);
            let Some(offset) = offset_of(cfn, scope, values) else {
                return top;
            };
            total = total.combine(self.entry_of(cfn, *term, offset), top);
            if total == top {
                return top;
            }
        }
        total
    }
}

/// Where a tuple sits in a table over a scope, read out of a full value vector.
fn offset_of(cfn: &Cfn, scope: &[VarId], values: &[ValId]) -> Option<usize> {
    let mut offset = 0usize;
    for var in scope {
        let variable = cfn.variable(*var)?;
        let slot = variable.slot(*values.get(var.index())?)?;
        offset = offset.checked_mul(variable.slots())?.checked_add(slot)?;
    }
    Some(offset)
}

// ---------------------------------------------------------------------------
// The public passes
// ---------------------------------------------------------------------------

/// Eliminate every variable in turn, in the `(min, ⊕)` semiring.
///
/// The result carries the optimum and the messages [`decode`] reads an argmin
/// back out of. Nothing is pruned and no bound is consulted, so the optimum is
/// exact whatever budget the caller had in mind; the budget question is whether
/// to call this at all, which
/// [`order::fits_budget`](super::order::fits_budget) answers from the width.
///
/// # Panics
///
/// If `order` is not a permutation of the network's variables, or if a message
/// table's entry count overflows `usize`.
///
/// # Examples
///
/// ```
/// use panproto_gat::Name;
/// use panproto_mig::solve::cfn::CfnBuilder;
/// use panproto_mig::solve::elim::{decode, eliminate};
/// use panproto_mig::solve::order::choose_order;
/// use panproto_mig::{Cost, DEFAULT_WEIGHTS, SearchBudget, VarId};
///
/// let mut builder = CfnBuilder::new(
///     vec![
///         (Name::new("a"), vec![Name::new("x")]),
///         (Name::new("b"), vec![Name::new("x")]),
///     ],
///     DEFAULT_WEIGHTS,
/// )?;
/// // `a` prefers its target, `b` prefers `⊥`.
/// builder.add_unary_table(VarId::new(0), &[Cost::BOT, Cost::from_raw(4)])?;
/// builder.add_unary_table(VarId::new(1), &[Cost::from_raw(7), Cost::BOT])?;
/// let cfn = builder.build();
///
/// let (order, _width) = choose_order(&cfn);
/// let buckets = eliminate(&cfn, &order);
/// let best = decode(&cfn, &buckets, &order);
///
/// assert_eq!(buckets.optimum(), Cost::BOT);
/// assert_eq!(cfn.evaluate(&best), buckets.optimum());
/// # Ok::<(), panproto_mig::solve::cfn::CfnError>(())
/// ```
#[must_use]
pub fn eliminate(cfn: &Cfn, order: &[VarId]) -> Buckets {
    let placement = place(cfn, order);
    let sweep = run::<Cost>(cfn, order, &placement);
    Buckets {
        order: order.to_vec(),
        all_vars: cfn.variable_ids().collect(),
        terms: sweep.buckets,
        messages: sweep.messages,
        optimum: sweep.optimum,
        width: sweep.width,
        peak_cells: sweep.peak_cells,
        total_cells: sweep.total_cells,
    }
}

/// What one [`decode`] pass did.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct DecodeTrace {
    /// Variables assigned, which is every variable.
    pub steps: usize,
    /// Steps at which every value of the domain scored `⊤`.
    ///
    /// Zero on any network with a feasible assignment, which is the whole
    /// content of the backtrack-freeness theorem.
    ///
    /// It is **not** a feasibility signal in the other direction. On an
    /// infeasible network the count is whatever the bucket carrying the
    /// violation makes it, and that can be zero: [`Buckets::optimum`] reads
    /// `c_∅` while the per-bucket scoring does not, so a network made
    /// infeasible by `c_∅` alone reports `⊤` as its optimum and dead-ends
    /// nowhere. Read feasibility off [`Buckets::optimum`].
    pub dead_ends: usize,
    /// Bucket evaluations performed, which is the recomputation the stored
    /// argmin tables would have saved.
    pub values_scored: usize,
}

/// Recover an argmin from a completed elimination.
///
/// One greedy pass over the elimination sequence read backwards. At each
/// variable, the value minimising the sum of that variable's bucket is chosen,
/// against the values already assigned. Every such choice extends to a global
/// optimum, so the pass never backtracks and never dead-ends.
///
/// Ties are broken towards the smallest value, and values are ordered by
/// ascending target vertex name with `⊥` last, so the returned assignment is
/// the lexicographically smallest optimum read in decode order. Under
/// [`reverse_source_id_order`](super::order::reverse_source_id_order) decode
/// order is ascending source vertex order, so the rule reads as the natural
/// one on source vertex names.
///
/// The key is worth stating explicitly, because it is the reverse of the
/// sequence the elimination ran under: it is
/// `(order[n − 1], order[n − 2], …, order[0])`, since [`eliminate`] walks the
/// order forwards and this walks it backwards. The two readings genuinely
/// disagree, so a test of the tie-break has to sort the argmin set on the
/// decode key rather than on the elimination sequence.
///
/// When the network has no feasible assignment the returned assignment is not
/// one either. Callers distinguish the two cases by reading
/// [`Buckets::optimum`], which is `⊤` exactly then.
///
/// # Panics
///
/// If `order` is not the order `buckets` was computed under. The bucket
/// positions are relative to that order, so decoding against another one would
/// read the wrong tables.
#[must_use]
pub fn decode(cfn: &Cfn, buckets: &Buckets, order: &[VarId]) -> Assignment {
    decode_traced(cfn, buckets, order).0
}

/// [`decode`], with a count of what it did.
///
/// The trace exists so that backtrack-freeness is a measurement rather than a
/// claim: `dead_ends` is zero on every feasible network, and a decode that had
/// to back out of a choice could not report zero.
///
/// # Panics
///
/// If `order` is not the order `buckets` was computed under.
#[must_use]
pub fn decode_traced(cfn: &Cfn, buckets: &Buckets, order: &[VarId]) -> (Assignment, DecodeTrace) {
    assert!(
        order == buckets.order,
        "an argmin must be decoded against the order its buckets were built under"
    );

    let mut values = vec![ValId::BOTTOM; cfn.n_variables()];
    let mut trace = DecodeTrace::default();

    for position in (0..buckets.order.len()).rev() {
        let Some(&eliminated) = buckets.order.get(position) else {
            continue;
        };
        let domain = cfn.domain(eliminated).unwrap_or(Domain::EMPTY);
        let mut best = Cost::TOP_SENTINEL;
        let mut chosen: Option<ValId> = None;

        for value in domain {
            values[eliminated.index()] = value;
            let score = buckets.score(cfn, position, &values);
            trace.values_scored += 1;
            if chosen.is_none() || score < best {
                best = score;
                chosen = Some(value);
            }
        }

        trace.steps += 1;
        if best == Cost::TOP_SENTINEL {
            trace.dead_ends += 1;
        }
        values[eliminated.index()] = chosen.unwrap_or(ValId::BOTTOM);
    }

    (Assignment::from_values(values), trace)
}

/// What one [`all_optima`] walk did.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct EnumerationTrace {
    /// Internal nodes expanded.
    pub nodes: usize,
    /// Leaves reached, which is how many optima were produced.
    pub leaves: usize,
    /// Nodes that produced no child, which is zero on a feasible network.
    pub dead_ends: usize,
    /// Whether the walk stopped at the limit rather than at the last optimum.
    pub truncated: bool,
}

/// Every optimum of the network, up to `limit` of them.
///
/// The walk branches on the argmin *set* at each variable rather than on one
/// argmin. Every branch extends to a global optimum and every global optimum
/// is some branch, so the search tree has exactly one leaf per optimum and no
/// dead ends at all. The optima come out in ascending lexicographic order in
/// decode order, so the first one is what [`decode`] returns.
///
/// A `limit` of zero produces nothing. A network with no feasible assignment
/// produces nothing either, since it has no optima to produce.
///
/// # Panics
///
/// If the value vector cannot be sized, which needs a network with more
/// variables than memory holds.
#[must_use]
pub fn all_optima(cfn: &Cfn, buckets: &Buckets, limit: usize) -> Vec<Assignment> {
    all_optima_traced(cfn, buckets, limit).0
}

/// [`all_optima`], with a count of what the walk did.
///
/// `leaves` equals the number of optima returned and `dead_ends` is zero, which
/// together are the backtrack-freeness of the enumeration stated as numbers.
#[must_use]
pub fn all_optima_traced(
    cfn: &Cfn,
    buckets: &Buckets,
    limit: usize,
) -> (Vec<Assignment>, EnumerationTrace) {
    let mut out = Vec::new();
    let mut trace = EnumerationTrace::default();
    if buckets.optimum == Cost::TOP_SENTINEL {
        return (out, trace);
    }
    if limit == 0 {
        // The network is feasible, so it has at least one optimum and asking
        // for none of them is a truncation rather than an exhaustion.
        trace.truncated = true;
        return (out, trace);
    }
    let mut values = vec![ValId::BOTTOM; cfn.n_variables()];
    let walk = Walk {
        cfn,
        buckets,
        limit,
    };
    walk.expand(0, &mut values, &mut out, &mut trace);
    (out, trace)
}

/// The state one enumeration walk carries.
struct Walk<'a> {
    cfn: &'a Cfn,
    buckets: &'a Buckets,
    limit: usize,
}

impl Walk<'_> {
    /// Extend a prefix of `depth` assigned variables by every optimal choice at
    /// the next one.
    fn expand(
        &self,
        depth: usize,
        values: &mut Vec<ValId>,
        out: &mut Vec<Assignment>,
        trace: &mut EnumerationTrace,
    ) {
        let count = self.buckets.order.len();
        if depth == count {
            out.push(Assignment::from_values(values.clone()));
            trace.leaves += 1;
            return;
        }

        let position = count - 1 - depth;
        let Some(&eliminated) = self.buckets.order.get(position) else {
            return;
        };
        trace.nodes += 1;
        let domain = self.cfn.domain(eliminated).unwrap_or(Domain::EMPTY);

        let mut best = Cost::TOP_SENTINEL;
        let mut seen = false;
        for value in domain {
            values[eliminated.index()] = value;
            let score = self.buckets.score(self.cfn, position, values);
            if !seen || score < best {
                best = score;
                seen = true;
            }
        }
        if !seen || best == Cost::TOP_SENTINEL {
            trace.dead_ends += 1;
            return;
        }

        // The limit is read before descending rather than after, so that a
        // branch abandoned for want of room is seen as one. Checking after the
        // descent instead would report a walk that stopped exactly at the last
        // optimum as truncated.
        for value in domain {
            values[eliminated.index()] = value;
            if self.buckets.score(self.cfn, position, values) == best {
                if out.len() >= self.limit {
                    trace.truncated = true;
                    return;
                }
                self.expand(depth + 1, values, out, trace);
            }
        }
    }
}

/// How many assignments the network admits, counted exactly.
///
/// The same sweep in the `(Σ, ×)` semiring over indicator values: a term
/// contributes one where its entry is finite and zero where it is `⊤`, so the
/// product over terms is one exactly on the assignments no constraint forbids
/// and the marginalisation sums those. Time and space are what elimination
/// always costs, `d^(w + 1)` and `d^w`, so this is exact rather than a bound,
/// and in particular it is not a product of domain sizes.
///
/// Counts reach `d^n` and so run past `u128` on networks the corpus contains.
/// The arithmetic saturates rather than wrapping, and a saturated count reads
/// [`COUNT_CEILING`].
///
/// # Panics
///
/// If `order` is not a permutation of the network's variables.
#[must_use]
pub fn count_solutions(cfn: &Cfn, order: &[VarId]) -> u128 {
    let placement = place(cfn, order);
    run::<Count>(cfn, order, &placement).optimum.0
}

// ---------------------------------------------------------------------------
// The product diagnostic
// ---------------------------------------------------------------------------

/// What [`detect_product`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProductVerdict {
    /// No assignment is feasible.
    Empty {
        /// The variable with no value left, or `None` when `c_∅` is `⊤` and so
        /// no variable is at fault.
        variable: Option<VarId>,
    },

    /// Every constraint is universal, so the feasible set is the full Cartesian
    /// product of the domains.
    Product {
        /// The product of the domain sizes, which here is the count.
        ///
        /// Exact below [`COUNT_CEILING`] and saturated at it, on the same terms
        /// [`count_solutions`] reports. The two agree, so a caller checking one
        /// against the other is checking two numbers rather than two ceilings
        /// only while this is below the ceiling.
        count: u128,
        /// The domain sizes it is the product of, by variable.
        domains: Vec<usize>,
    },

    /// Some constraint forbids something, so the feasible set is a proper
    /// subset of the product and has to be counted rather than multiplied out.
    NotProduct {
        /// The scopes of the constraints that forbid something.
        restricting: Vec<Vec<VarId>>,
        /// The connected components of the graph those scopes induce.
        ///
        /// The feasible set is the product of the components' feasible sets, so
        /// this is how far the product reading does still hold.
        components: Vec<Vec<VarId>>,
    },
}

/// Report whether the feasible set of the network is a full Cartesian product.
///
/// A constraint is *universal* when it forbids no tuple its variables can still
/// take. When every constraint is universal the feasible set is exactly the
/// product of the domains and its size is the product of their sizes; when one
/// is not, the product reading is an over-count and the components of the graph
/// the non-universal constraints induce say how the feasible set does factor.
///
/// Unary cost is folded into the domains first, since a `⊤` unary entry removes
/// a value rather than relating two variables. What is left to test is the
/// constraints of arity two and above, on the folded domains.
///
/// **This is a diagnostic, not an assertion.** A schema pair whose network is a
/// full product is common rather than suspicious: kind filtering routinely
/// leaves every remaining pairing admissible, and a network with no edge
/// constraint at all is a product by construction. What the verdict is for is
/// telling a count that is a product because the data says so from a count that
/// is a product because the constraints never reached the solver.
///
/// # Examples
///
/// ```
/// use panproto_gat::Name;
/// use panproto_mig::solve::cfn::CfnBuilder;
/// use panproto_mig::solve::elim::{ProductVerdict, detect_product};
/// use panproto_mig::{Cost, DEFAULT_WEIGHTS, VarId};
///
/// let mut builder = CfnBuilder::new(
///     vec![
///         (Name::new("a"), vec![Name::new("x")]),
///         (Name::new("b"), vec![Name::new("x")]),
///     ],
///     DEFAULT_WEIGHTS,
/// )?;
/// // Soft cost forbids nothing, so the feasible set is still the full product.
/// builder.add_function(
///     &[VarId::new(0), VarId::new(1)],
///     vec![Cost::BOT, Cost::from_raw(3), Cost::from_raw(9), Cost::BOT],
/// )?;
///
/// let ProductVerdict::Product { count, .. } = detect_product(&builder.build()) else {
///     panic!("a soft cost function forbids nothing");
/// };
/// assert_eq!(count, 4);
/// # Ok::<(), panproto_mig::solve::cfn::CfnError>(())
/// ```
#[must_use]
pub fn detect_product(cfn: &Cfn) -> ProductVerdict {
    if cfn.c_empty() == Cost::TOP_SENTINEL {
        return ProductVerdict::Empty { variable: None };
    }

    let mut effective = Vec::with_capacity(cfn.n_variables());
    for var in cfn.variable_ids() {
        let start = cfn.domain(var).unwrap_or(Domain::EMPTY);
        let mut domain = start;
        for value in start {
            if cfn.unary_cost(var, value) == Some(Cost::TOP_SENTINEL) {
                domain.remove(value);
            }
        }
        if domain.is_empty() {
            return ProductVerdict::Empty {
                variable: Some(var),
            };
        }
        effective.push(domain);
    }

    let restricting: Vec<Vec<VarId>> = cfn
        .functions()
        .iter()
        .filter(|function| !is_universal(cfn, function, &effective))
        .map(|function| function.scope().to_vec())
        .collect();

    if restricting.is_empty() {
        let domains: Vec<usize> = effective.iter().map(|domain| domain.len()).collect();
        let count = domains.iter().fold(1u128, |total, size| {
            total.saturating_mul(u128::try_from(*size).unwrap_or(COUNT_CEILING))
        });
        return ProductVerdict::Product { count, domains };
    }

    let mut graph = Graph::new(cfn.n_variables());
    for scope in &restricting {
        for (offset, left) in scope.iter().enumerate() {
            for right in &scope[offset + 1..] {
                graph.add_edge(*left, *right);
            }
        }
    }
    ProductVerdict::NotProduct {
        components: graph.components(),
        restricting,
    }
}

/// Whether a cost function forbids nothing its variables can still take.
fn is_universal(cfn: &Cfn, function: &CostFunction, effective: &[Domain]) -> bool {
    let scope = function.scope();
    let strides = strides_of(cfn, scope);

    let mut choices: Vec<Vec<usize>> = Vec::with_capacity(scope.len());
    for var in scope {
        let domain = effective.get(var.index()).copied().unwrap_or(Domain::EMPTY);
        let variable = cfn.variable(*var);
        choices.push(
            domain
                .iter()
                .filter_map(|value| variable.and_then(|variable| variable.slot(value)))
                .collect(),
        );
    }
    if choices.iter().any(Vec::is_empty) {
        return true;
    }

    let mut cursor = vec![0usize; scope.len()];
    loop {
        let mut offset = 0usize;
        for ((stride, slots), position) in strides.iter().zip(&choices).zip(&cursor) {
            offset += stride * slots.get(*position).copied().unwrap_or(0);
        }
        if function.table().get(offset).copied() == Some(Cost::TOP_SENTINEL) {
            return false;
        }

        let mut index = cursor.len();
        loop {
            if index == 0 {
                return true;
            }
            index -= 1;
            cursor[index] += 1;
            if cursor[index] < choices[index].len() {
                break;
            }
            cursor[index] = 0;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use crate::solve::ValId;
    use crate::solve::cfn::CfnBuilder;
    use crate::solve::cost::DEFAULT_WEIGHTS;
    use crate::solve::oracle::brute_force;
    use crate::solve::order::{choose_order, induced_width, primal_graph};
    use panproto_gat::Name;

    fn var(index: u32) -> VarId {
        VarId::new(index)
    }

    fn cost(units: u64) -> Cost {
        Cost::from_raw(units)
    }

    /// `count` variables named `v0 …`, each over `targets` targets.
    fn builder(count: u32, targets: u32) -> CfnBuilder {
        let spec = (0..count)
            .map(|index| {
                (
                    Name::new(format!("v{index}")),
                    (0..targets)
                        .map(|target| Name::new(format!("t{target}")))
                        .collect(),
                )
            })
            .collect();
        CfnBuilder::new(spec, DEFAULT_WEIGHTS).unwrap()
    }

    fn descending(count: u32) -> Vec<VarId> {
        (0..count).rev().map(var).collect()
    }

    /// The value vector read in decode order, which is the order the tie-break
    /// is stated in.
    fn decode_key(assignment: &Assignment, order: &[VarId]) -> Vec<u32> {
        order
            .iter()
            .rev()
            .filter_map(|var| assignment.get(*var).map(ValId::raw))
            .collect()
    }

    // -- Bucket placement --------------------------------------------------

    #[test]
    fn a_term_lands_in_the_bucket_of_its_earliest_eliminated_variable() {
        let mut b = builder(3, 1);
        b.add_function(&[var(0), var(2)], vec![Cost::BOT; 4])
            .unwrap();
        let cfn = b.build();

        // Eliminate 2, then 1, then 0.
        let order = descending(3);
        let buckets = eliminate(&cfn, &order);

        // Bucket 0 eliminates variable 2, and holds its unary table and the
        // one binary function, whose other variable is eliminated later.
        assert_eq!(
            buckets.bucket_scopes(&cfn, 0).unwrap(),
            vec![vec![var(2)], vec![var(0), var(2)]]
        );
        assert_eq!(buckets.bucket_scopes(&cfn, 1).unwrap(), vec![vec![var(1)]]);
    }

    #[test]
    fn every_term_lands_in_exactly_one_bucket() {
        let mut b = builder(4, 2);
        b.add_function(&[var(0), var(1)], vec![Cost::BOT; 9])
            .unwrap();
        b.add_function(&[var(1), var(3)], vec![Cost::BOT; 9])
            .unwrap();
        let cfn = b.build();
        let order = descending(4);
        let buckets = eliminate(&cfn, &order);

        let placed: usize = (0..4)
            .map(|position| buckets.bucket_scopes(&cfn, position).unwrap().len())
            .sum();
        // Four unary tables, two functions, and one message per bucket whose
        // message scope is not empty.
        let messages = (0..4)
            .filter(|position| buckets.message_scope(*position).is_some())
            .count();
        assert_eq!(placed, 4 + 2 + messages);
    }

    #[test]
    fn a_message_goes_to_the_earliest_eliminated_variable_of_its_own_scope() {
        let mut b = builder(3, 1);
        b.add_function(&[var(0), var(2)], vec![Cost::BOT; 4])
            .unwrap();
        let cfn = b.build();
        let order = descending(3);
        let buckets = eliminate(&cfn, &order);

        // Eliminating 2 leaves a message over {0}, which belongs to bucket 2.
        assert_eq!(buckets.message_scope(0), Some(&[var(0)][..]));
        assert_eq!(
            buckets.bucket_scopes(&cfn, 2).unwrap(),
            vec![vec![var(0)], vec![var(0)]]
        );
    }

    #[test]
    #[should_panic(expected = "list every variable of the network exactly once")]
    fn an_order_that_is_not_a_permutation_is_refused() {
        let cfn = builder(3, 1).build();
        let _ = eliminate(&cfn, &[var(0), var(1)]);
    }

    // -- The streaming nest ------------------------------------------------

    #[test]
    fn the_nest_allocates_the_message_and_not_the_join() {
        // Three variables of three slots each, joined by one ternary scope. The
        // first bucket eliminates one of them and leaves a message over the
        // other two: nine entries, where the join would be twenty seven.
        let mut b = builder(3, 2);
        b.add_function(&[var(0), var(1), var(2)], vec![Cost::BOT; 27])
            .unwrap();
        let cfn = b.build();
        let order = descending(3);
        let buckets = eliminate(&cfn, &order);

        assert_eq!(buckets.width(), 2);
        assert_eq!(buckets.peak_cells(), 9, "d^|U_p|, not d^(|U_p| + 1)");
        // Nine for the first bucket's message, three for the second's, one for
        // the scalar the last bucket produces.
        assert_eq!(buckets.total_cells(), 9 + 3 + 1);
    }

    #[test]
    fn the_peak_allocation_is_the_widest_message_and_nothing_larger() {
        let mut b = builder(4, 3);
        b.add_function(&[var(0), var(1), var(3)], vec![Cost::BOT; 64])
            .unwrap();
        b.add_function(&[var(1), var(2)], vec![Cost::BOT; 16])
            .unwrap();
        let cfn = b.build();
        let order = descending(4);
        let buckets = eliminate(&cfn, &order);

        let slots = 4usize;
        // The witness: the message arity of every bucket, computed by a
        // longhand fill-in over the primal graph that shares no code with the
        // sweep. `peak_cells` and `total_cells` are counters the sweep itself
        // increments, so comparing them only against each other would check
        // nothing; comparing them against this checks the claim.
        // Edges 0-1, 0-3, 1-3 from the ternary scope and 1-2 from the binary
        // one, eliminated in the order [3, 2, 1, 0]: bucket 3 joins {0, 1},
        // bucket 2 sends to 1, bucket 1 sends to 0, bucket 0 sends a scalar.
        let arities = message_arities(&primal_graph(&cfn), &order);
        assert_eq!(arities, vec![2, 1, 1, 0]);

        let message_total: usize = arities
            .iter()
            .map(|arity| slots.pow(u32::try_from(*arity).unwrap()))
            .sum();
        let join_total: usize = arities
            .iter()
            .map(|arity| slots.pow(u32::try_from(*arity + 1).unwrap()))
            .sum();

        assert_eq!(buckets.width(), arities.iter().copied().max().unwrap());
        assert_eq!(buckets.peak_cells(), slots.pow(2));
        assert_eq!(
            buckets.total_cells(),
            message_total,
            "the sweep allocates one message per bucket and nothing else"
        );
        assert_eq!(
            join_total,
            message_total * slots,
            "joining first would cost a factor of d more at every bucket"
        );
        assert!(
            buckets.total_cells() < join_total,
            "d^|U_p| rather than d^(|U_p| + 1), measured against an independent count"
        );
    }

    /// The arity of the message each bucket sends, by position in the order.
    ///
    /// Dechter's fill-in written out with a plain boolean matrix rather than
    /// through [`Graph`](super::order::Graph): this is the independent witness
    /// the allocation tests compare against, so it deliberately shares no code
    /// with either the sweep or [`induced_width`](super::order::induced_width).
    fn message_arities(graph: &super::super::order::Graph, order: &[VarId]) -> Vec<usize> {
        let count = graph.n_vertices();
        let mut adjacent: Vec<Vec<bool>> = (0..count)
            .map(|left| {
                (0..count)
                    .map(|right| graph.has_edge(var_of(left), var_of(right)))
                    .collect()
            })
            .collect();
        let mut eliminated = vec![false; count];
        let mut arities = Vec::with_capacity(count);
        for vertex in order {
            let index = vertex.index();
            eliminated[index] = true;
            let live: Vec<usize> = (0..count)
                .filter(|other| !eliminated[*other] && adjacent[index][*other])
                .collect();
            arities.push(live.len());
            for (position, left) in live.iter().enumerate() {
                for right in live.iter().skip(position + 1) {
                    adjacent[*left][*right] = true;
                    adjacent[*right][*left] = true;
                }
            }
        }
        arities
    }

    fn var_of(index: usize) -> VarId {
        VarId::new(u32::try_from(index).unwrap())
    }

    // -- The optimum -------------------------------------------------------

    #[test]
    fn the_optimum_of_a_hand_computed_network_is_found() {
        let mut b = builder(2, 1);
        b.add_empty(cost(3));
        b.add_unary_table(var(0), &[cost(5), cost(2)]).unwrap();
        b.add_unary_table(var(1), &[cost(1), cost(5)]).unwrap();
        // Row major over `[v0, v1]` with `v1` fastest.
        b.add_function(&[var(0), var(1)], vec![cost(0), cost(0), cost(10), cost(0)])
            .unwrap();
        let cfn = b.build();

        // (t0,t0) 3+5+1+0 = 9   (t0,⊥) 3+5+5+0 = 13
        // (⊥,t0)  3+2+1+10 = 16 (⊥,⊥)  3+2+5+0 = 10
        let order = descending(2);
        let buckets = eliminate(&cfn, &order);
        assert_eq!(buckets.optimum(), cost(9));

        let best = decode(&cfn, &buckets, &order);
        assert_eq!(cfn.evaluate(&best), cost(9));
    }

    #[test]
    fn the_constant_term_shifts_the_optimum() {
        let mut b = builder(2, 1);
        b.add_empty(cost(11));
        let cfn = b.build();
        let order = descending(2);
        assert_eq!(eliminate(&cfn, &order).optimum(), cost(11));
    }

    #[test]
    fn a_network_with_no_variables_returns_its_constant() {
        let mut b = CfnBuilder::new(Vec::new(), DEFAULT_WEIGHTS).unwrap();
        b.add_empty(cost(7));
        let cfn = b.build();
        let buckets = eliminate(&cfn, &[]);
        assert_eq!(buckets.optimum(), cost(7));
        assert_eq!(buckets.width(), 0);
        assert!(decode(&cfn, &buckets, &[]).is_empty());
    }

    #[test]
    fn an_infeasible_network_reports_top_and_dead_ends_everywhere() {
        let mut b = builder(2, 1);
        b.add_unary_table(var(0), &[Cost::TOP_SENTINEL; 2]).unwrap();
        let cfn = b.build();
        let order = descending(2);
        let buckets = eliminate(&cfn, &order);
        assert_eq!(buckets.optimum(), Cost::TOP_SENTINEL);

        let (_, trace) = decode_traced(&cfn, &buckets, &order);
        // One dead end, at the variable whose every value is forbidden. The
        // other variable is unconstrained, so its own bucket still has a best
        // value; what makes the network infeasible is the constant the first
        // bucket contributed, which decode never revisits.
        assert_eq!(trace.dead_ends, 1);
        assert!(all_optima(&cfn, &buckets, 16).is_empty());
    }

    // -- The width ---------------------------------------------------------

    #[test]
    fn the_message_arity_is_the_induced_width_of_the_order() {
        let mut b = builder(5, 2);
        b.add_function(&[var(0), var(1)], vec![Cost::BOT; 9])
            .unwrap();
        b.add_function(&[var(1), var(2)], vec![Cost::BOT; 9])
            .unwrap();
        b.add_function(&[var(0), var(2)], vec![Cost::BOT; 9])
            .unwrap();
        b.add_function(&[var(2), var(3)], vec![Cost::BOT; 9])
            .unwrap();
        let cfn = b.build();
        let graph = primal_graph(&cfn);

        for order in [descending(5), (0..5).map(var).collect::<Vec<_>>()] {
            let buckets = eliminate(&cfn, &order);
            assert_eq!(buckets.width(), induced_width(&graph, &order));
        }
    }

    #[test]
    fn a_source_tree_eliminates_at_width_one() {
        // A path of four variables, eliminated from the far end.
        let mut b = builder(4, 2);
        for index in 0..3 {
            b.add_function(&[var(index), var(index + 1)], vec![Cost::BOT; 9])
                .unwrap();
        }
        let cfn = b.build();
        let (order, width) = choose_order(&cfn);
        assert_eq!(width, 1);
        assert_eq!(eliminate(&cfn, &order).width(), 1);
    }

    // -- Decode ------------------------------------------------------------

    #[test]
    fn decode_returns_the_lexicographically_smallest_optimum() {
        // Two variables over two targets, each with two values at its best, so
        // four assignments tie for the optimum.
        let mut b = builder(2, 2);
        b.add_unary_table(var(0), &[cost(0), cost(0), cost(1)])
            .unwrap();
        b.add_unary_table(var(1), &[cost(1), cost(0), cost(0)])
            .unwrap();
        let cfn = b.build();
        let order = descending(2);
        let buckets = eliminate(&cfn, &order);
        let (best, trace) = decode_traced(&cfn, &buckets, &order);

        let (optimum, argmins) = brute_force(&cfn);
        assert_eq!(buckets.optimum(), optimum);
        assert!(argmins.len() > 1, "the fixture is meant to tie");
        assert!(argmins.contains(&best));

        let mut keys: Vec<Vec<u32>> = argmins.iter().map(|a| decode_key(a, &order)).collect();
        keys.sort();
        assert_eq!(decode_key(&best, &order), keys[0]);
        assert_eq!(trace.dead_ends, 0);
    }

    #[test]
    fn decode_prefers_a_real_target_to_bottom_when_they_tie() {
        let cfn = builder(2, 1).build();
        let order = descending(2);
        let buckets = eliminate(&cfn, &order);
        let best = decode(&cfn, &buckets, &order);
        assert_eq!(best.values(), &[ValId::real(0), ValId::real(0)]);
    }

    #[test]
    #[should_panic(expected = "decoded against the order its buckets were built under")]
    fn decoding_against_another_order_is_refused() {
        let cfn = builder(3, 1).build();
        let buckets = eliminate(&cfn, &descending(3));
        let _ = decode(&cfn, &buckets, &[var(0), var(1), var(2)]);
    }

    // -- All optima --------------------------------------------------------

    #[test]
    fn every_optimum_is_a_leaf_and_no_branch_dies() {
        let mut b = builder(3, 2);
        b.add_function(&[var(0), var(1)], vec![Cost::BOT; 9])
            .unwrap();
        let cfn = b.build();
        let order = descending(3);
        let buckets = eliminate(&cfn, &order);

        let (optima, trace) = all_optima_traced(&cfn, &buckets, 1_000);
        let (_, argmins) = brute_force(&cfn);
        assert_eq!(optima.len(), argmins.len());
        assert_eq!(trace.leaves, argmins.len());
        assert_eq!(trace.dead_ends, 0);
        assert!(!trace.truncated);
        for optimum in &optima {
            assert_eq!(cfn.evaluate(optimum), buckets.optimum());
        }
    }

    #[test]
    fn the_optima_come_out_smallest_first_and_the_limit_truncates() {
        let cfn = builder(2, 1).build();
        let order = descending(2);
        let buckets = eliminate(&cfn, &order);

        let all = all_optima(&cfn, &buckets, 100);
        assert_eq!(all.len(), 4, "everything ties at zero cost");
        assert_eq!(all[0], decode(&cfn, &buckets, &order));

        let (capped, trace) = all_optima_traced(&cfn, &buckets, 2);
        assert_eq!(capped.len(), 2);
        assert_eq!(capped, all[..2].to_vec());
        assert!(trace.truncated);
        assert!(all_optima(&cfn, &buckets, 0).is_empty());
    }

    // -- Counting ----------------------------------------------------------

    #[test]
    fn counting_an_unconstrained_network_gives_the_product_of_the_domains() {
        let cfn = builder(3, 2).build();
        assert_eq!(count_solutions(&cfn, &descending(3)), 27);
    }

    #[test]
    fn counting_sees_what_a_hard_constraint_forbids() {
        let mut b = builder(2, 1);
        // Row major over `[v0, v1]`: forbid the two diagonal tuples.
        b.add_function(
            &[var(0), var(1)],
            vec![Cost::TOP_SENTINEL, Cost::BOT, Cost::BOT, Cost::TOP_SENTINEL],
        )
        .unwrap();
        let cfn = b.build();
        assert_eq!(count_solutions(&cfn, &descending(2)), 2);
    }

    #[test]
    fn counting_agrees_with_exhaustive_enumeration() {
        let mut rng = Lcg(0x5eed_1234_9abc_def0);
        for _ in 0..200 {
            let cfn = random_network(&mut rng);
            let order = descending(u32::try_from(cfn.n_variables()).unwrap());
            let expected = exhaustive_count(&cfn);
            assert_eq!(count_solutions(&cfn, &order), expected);
        }
    }

    #[test]
    fn an_infeasible_network_counts_zero() {
        let mut b = builder(2, 1);
        b.add_unary_table(var(0), &[Cost::TOP_SENTINEL; 2]).unwrap();
        assert_eq!(count_solutions(&b.build(), &descending(2)), 0);
    }

    // -- The product diagnostic --------------------------------------------

    #[test]
    fn a_network_with_no_hard_constraint_is_a_product() {
        let mut b = builder(3, 2);
        b.add_function(&[var(0), var(1)], vec![cost(4); 9]).unwrap();
        let cfn = b.build();
        assert_eq!(
            detect_product(&cfn),
            ProductVerdict::Product {
                count: 27,
                domains: vec![3, 3, 3],
            }
        );
        assert_eq!(count_solutions(&cfn, &descending(3)), 27);
    }

    #[test]
    fn one_forbidden_tuple_makes_it_not_a_product() {
        let mut b = builder(3, 1);
        let mut table = vec![Cost::BOT; 4];
        table[0] = Cost::TOP_SENTINEL;
        b.add_function(&[var(0), var(1)], table).unwrap();
        let cfn = b.build();

        let ProductVerdict::NotProduct {
            restricting,
            components,
        } = detect_product(&cfn)
        else {
            panic!("a forbidden tuple is not a product");
        };
        assert_eq!(restricting, vec![vec![var(0), var(1)]]);
        assert_eq!(components, vec![vec![var(0), var(1)], vec![var(2)]]);
        assert_eq!(count_solutions(&cfn, &descending(3)), 3 * 2);
    }

    #[test]
    fn a_constraint_that_only_forbids_a_value_no_domain_holds_is_universal() {
        // The unary table strikes `t0` from the first variable, and the only
        // forbidden binary tuple names it, so nothing reachable is forbidden.
        let mut b = builder(2, 1);
        b.add_unary_table(var(0), &[Cost::TOP_SENTINEL, Cost::BOT])
            .unwrap();
        let mut table = vec![Cost::BOT; 4];
        table[0] = Cost::TOP_SENTINEL;
        b.add_function(&[var(0), var(1)], table).unwrap();
        let cfn = b.build();

        assert_eq!(
            detect_product(&cfn),
            ProductVerdict::Product {
                count: 2,
                domains: vec![1, 2],
            }
        );
        assert_eq!(count_solutions(&cfn, &descending(2)), 2);
    }

    #[test]
    fn a_variable_with_no_value_left_reads_empty() {
        let mut b = builder(2, 1);
        b.add_unary_table(var(1), &[Cost::TOP_SENTINEL; 2]).unwrap();
        assert_eq!(
            detect_product(&b.build()),
            ProductVerdict::Empty {
                variable: Some(var(1))
            }
        );
    }

    #[test]
    fn an_infeasible_constant_reads_empty_with_no_variable_at_fault() {
        let mut b = builder(2, 1);
        b.add_empty(Cost::TOP_SENTINEL);
        assert_eq!(
            detect_product(&b.build()),
            ProductVerdict::Empty { variable: None }
        );
    }

    #[test]
    fn the_verdict_is_a_product_exactly_when_every_constraint_is_universal() {
        let mut rng = Lcg(0xfeed_face_dead_beef);
        for _ in 0..500 {
            let cfn = random_network(&mut rng);
            let verdict = detect_product(&cfn);
            let count =
                count_solutions(&cfn, &descending(u32::try_from(cfn.n_variables()).unwrap()));
            match verdict {
                ProductVerdict::Product {
                    count: reported, ..
                } => {
                    assert_eq!(reported, count, "a product's count is the product");
                }
                ProductVerdict::Empty { .. } => assert_eq!(count, 0),
                ProductVerdict::NotProduct { .. } => {
                    let product = effective_product(&cfn);
                    assert!(
                        count < product,
                        "a non-universal constraint strictly cuts the product down"
                    );
                }
            }
        }
    }

    // -- Agreement with the oracle -----------------------------------------

    #[test]
    fn decode_never_dead_ends_over_a_thousand_random_networks() {
        let mut rng = Lcg(0x0123_4567_89ab_cdef);
        let mut feasible = 0usize;
        for _ in 0..1_000 {
            let cfn = random_network(&mut rng);
            let (order, _) = choose_order(&cfn);
            let buckets = eliminate(&cfn, &order);
            let (best, trace) = decode_traced(&cfn, &buckets, &order);
            assert_eq!(trace.steps, cfn.n_variables());
            if buckets.optimum() == Cost::TOP_SENTINEL {
                continue;
            }
            feasible += 1;
            assert_eq!(trace.dead_ends, 0, "a feasible network never dead-ends");
            assert_eq!(cfn.evaluate(&best), buckets.optimum());
        }
        assert!(feasible > 500, "the fixtures are mostly feasible");
    }

    #[test]
    fn elimination_agrees_with_the_oracle_over_random_networks() {
        let mut rng = Lcg(0xdead_c0de_1234_5678);
        for _ in 0..500 {
            let cfn = random_network(&mut rng);
            let pristine = cfn.clone();
            let (order, _) = choose_order(&cfn);
            let buckets = eliminate(&cfn, &order);
            let (optimum, argmins) = brute_force(&cfn);

            assert_eq!(buckets.optimum(), optimum);
            if optimum == Cost::TOP_SENTINEL {
                assert!(argmins.is_empty());
                continue;
            }
            let best = decode(&cfn, &buckets, &order);
            assert_eq!(pristine.evaluate(&best), optimum);
            assert!(argmins.contains(&best));

            let mut keys: Vec<Vec<u32>> = argmins.iter().map(|a| decode_key(a, &order)).collect();
            keys.sort();
            assert_eq!(decode_key(&best, &order), keys[0]);

            let optima = all_optima(&cfn, &buckets, 10_000);
            assert_eq!(optima.len(), argmins.len());
        }
    }

    // -- Fixtures ----------------------------------------------------------

    /// A deterministic generator, so that a failure is reproducible from the
    /// seed alone.
    struct Lcg(u64);

    impl Lcg {
        fn next_u32(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) as u32
        }

        fn below(&mut self, bound: u32) -> u32 {
            if bound == 0 {
                0
            } else {
                self.next_u32() % bound
            }
        }
    }

    /// Two to five variables over one to three targets, with a few binary
    /// scopes and costs drawn from `⊥`, a small finite value, and `⊤`.
    fn random_network(rng: &mut Lcg) -> Cfn {
        let count = 2 + rng.below(4);
        let targets = 1 + rng.below(3);
        let mut b = builder(count, targets);

        for index in 0..count {
            let slots = (targets + 1) as usize;
            let table: Vec<Cost> = (0..slots).map(|_| random_cost(rng)).collect();
            b.add_unary_table(var(index), &table).unwrap();
        }

        let scopes = rng.below(4);
        for _ in 0..scopes {
            let left = rng.below(count);
            let right = rng.below(count);
            if left == right {
                continue;
            }
            let scope = [var(left.min(right)), var(left.max(right))];
            let Some(length) = b.table_length(&scope) else {
                continue;
            };
            let table: Vec<Cost> = (0..length).map(|_| random_cost(rng)).collect();
            b.add_function(&scope, table).unwrap();
        }
        b.build()
    }

    /// A cost that is `⊥` half the time, small a third of the time, and `⊤`
    /// otherwise, so that both the objective and the feasible set vary.
    fn random_cost(rng: &mut Lcg) -> Cost {
        match rng.below(6) {
            0..=2 => Cost::BOT,
            3 | 4 => cost(u64::from(rng.below(10)) + 1),
            _ => Cost::TOP_SENTINEL,
        }
    }

    /// The number of feasible assignments, counted by enumeration.
    fn exhaustive_count(cfn: &Cfn) -> u128 {
        let mut total = 0u128;
        let mut values = vec![ValId::BOTTOM; cfn.n_variables()];
        enumerate(cfn, 0, &mut values, &mut total);
        total
    }

    fn enumerate(cfn: &Cfn, index: usize, values: &mut Vec<ValId>, total: &mut u128) {
        if index == cfn.n_variables() {
            if cfn.evaluate(&Assignment::from_values(values.clone())) != Cost::TOP_SENTINEL {
                *total += 1;
            }
            return;
        }
        let domain = cfn
            .domain(VarId::new(u32::try_from(index).unwrap()))
            .unwrap_or(Domain::EMPTY);
        for value in domain {
            values[index] = value;
            enumerate(cfn, index + 1, values, total);
        }
    }

    /// The product of the effective domain sizes, which is what a non-universal
    /// constraint has to cut into.
    fn effective_product(cfn: &Cfn) -> u128 {
        let mut total = 1u128;
        for var in cfn.variable_ids() {
            let start = cfn.domain(var).unwrap_or(Domain::EMPTY);
            let mut size = 0u128;
            for value in start {
                if cfn.unary_cost(var, value) != Some(Cost::TOP_SENTINEL) {
                    size += 1;
                }
            }
            total *= size;
        }
        total
    }
}
