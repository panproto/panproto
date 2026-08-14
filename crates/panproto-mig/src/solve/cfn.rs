//! The cost function network the search minimises over.
//!
//! A network is one variable per source vertex, one value per kind-compatible
//! target vertex plus `⊥`, a cost table for every variable, a cost table for
//! every scope of two or more variables that some schema construct constrains,
//! and one constant `c_∅`. Its cost on an assignment is the `⊕`-sum of the
//! constant and every table entry the assignment selects, and the search is the
//! minimisation of that sum.
//!
//! This module owns the network and nothing else. It computes no schema
//! semantics: it never reads a [`Schema`](panproto_schema::Schema), never
//! decides which pairs are kind compatible, and never turns a similarity into a
//! number. Those belong to the builder that translates a schema pair into a
//! network. What this module owns is the representation, the invariants that
//! representation must satisfy, and the one scorer that is defined without
//! reference to any solver.
//!
//! # `⊥` and the domain layout
//!
//! Every domain contains `⊥`, and `⊥` is ordered last in every domain. Real
//! values are ordered by ascending target vertex name. Both facts are
//! established by [`CfnBuilder::new`], which sorts and deduplicates the value
//! list it is handed, so no caller can produce a network where they fail.
//!
//! Ordering `⊥` last is what makes "the lexicographically smallest assignment
//! among the argmins" a usable tie-break: comparing two argmins position by
//! position prefers a real image to a dropped vertex, and prefers the
//! alphabetically earlier target among real images.
//!
//! Every domain of one network lives in [`Domains`], one contiguous bit set of
//! `n · words` machine words, and [`Domain`] is a borrowed view of one
//! variable's block. `words` is a property of the network rather than of the
//! type, so **nothing bounds how many targets a variable may be offered**: a
//! record with a thousand fields of one type and a file parsed to one vertex
//! per line are ordinary networks. `⊥` is bit zero of every block, which is
//! what keeps its identity independent of the width, and the search's `⊥`-last
//! order is stated by [`DomainIter`] rather than fallen out of the numbering.
//!
//! What does bound a network is memory, and that is measured rather than
//! assumed: [`CfnBuilder`] adds up the cost table entries it is asked to
//! allocate and refuses with [`CfnError::OverMemoryBudget`], which names the
//! bytes and the budget.
//!
//! # The scope uniqueness invariant
//!
//! **No two cost functions may share a scope.** Duplicates are merged by
//! pointwise `⊕` at construction, and the merge is the builder's job rather than
//! a caller's.
//!
//! This is a correctness precondition, not tidiness. The soft local consistency
//! the branch and bound path maintains moves cost between functions by
//! subtracting from one what it adds to another, and two functions on one scope
//! give it a cycle to move cost around forever: Lee and Leung (*JAIR* 44,
//! §4.4.1, Fig. 4) exhibit a network on which enforcement then never terminates.
//!
//! It bites in this application specifically. Parallel source edges between one
//! vertex pair are distinct keys, because an [`Edge`](panproto_schema::Edge)
//! hashes on `(src, tgt, kind, name)`, yet they land on the same scope
//! `{x_src, x_tgt}`. A builder that emitted one binary function per source edge
//! would violate the invariant on any schema with two edges between the same two
//! vertices, which is common. [`CfnBuilder::add_function`] therefore merges
//! rather than appends, and there is no other way to put a function into a
//! network.
//!
//! Two corollaries are worth stating because they are easy to get wrong.
//!
//! First, **unary cost lives in one table per variable**, not in a list of
//! arity-one functions, which makes the invariant hold for arity one by
//! construction. A function offered with a one-variable scope is folded into
//! that table, and one offered with an empty scope is folded into `c_∅`.
//!
//! Second, **a source self-loop is not a binary function**. An edge from a
//! vertex to itself constrains one variable, so its scope after deduplication
//! has size one and its cost is `b(a, a)` read along the diagonal. Scopes with a
//! repeated variable are rejected; the caller folds the diagonal into the unary
//! table itself, which is the only reading that does not silently double count.
//!
//! # Table layout
//!
//! A function's table is row-major over the scope, with the **last** variable in
//! the scope varying fastest. Writing `s_i` for the slot the assignment selects
//! in the `i`th scope variable and `k_i` for that variable's slot count, the
//! entry index is
//!
//! ```text
//! ((s_0 · k_1 + s_1) · k_2 + s_2) · … + s_{r-1}
//! ```
//!
//! A variable's slot count is one more than its real value count, with the last
//! slot for `⊥`. Slots are dense over the value list rather than over
//! [`ValId`]'s numbering, so a binary function over two 19-value domains has 400
//! entries rather than 1024. [`Cfn::table_index`] and [`Cfn::table_length`]
//! compute the two quantities so that a caller never has to reproduce the
//! formula.
//!
//! # Evaluation
//!
//! [`Cfn::evaluate`] is deliberately naive. It reads `c_∅`, one entry per
//! variable and one entry per function, and adds them. It consults no
//! transformed cost tables, no propagation state and no bound, and it shares no
//! code with any solver.
//!
//! That naivety is the point. Two obligations rest on it: the brute force oracle
//! scores candidates with it, and every search path must return an assignment
//! whose evaluation reproduces the cost the search reported. Neither obligation
//! means anything if the scorer can be wrong in the same way the solver is.

use panproto_gat::Name;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use super::cost::{COST_SCALE, Cost, CostWeights, coverage_radix};
use super::{Assignment, DEFAULT_MEM_BYTES, ValId, VarId};

/// [`COST_SCALE`] as a float, for the one conversion [`Cfn::quality_of`] makes.
///
/// Written as a literal rather than converted from [`COST_SCALE`] so that the
/// conversion is exact by inspection; `the_cost_scale_float_matches_the_scale`
/// pins the two together.
const COST_SCALE_FLOAT: f64 = 1.0e9;

// ---------------------------------------------------------------------------
// Domain
// ---------------------------------------------------------------------------

/// The word a value's bit lives in.
#[inline]
const fn word_of(value: ValId) -> usize {
    (value.raw() / u64::BITS) as usize
}

/// The bit pattern selecting a value within its word.
#[inline]
const fn bit_of(value: ValId) -> u64 {
    1u64 << (value.raw() % u64::BITS)
}

/// How many words a domain of this many slots, `⊥` included, needs.
#[inline]
#[must_use]
const fn words_for(slots: u32) -> usize {
    (slots as usize).div_ceil(u64::BITS as usize)
}

/// The set of values one variable may take, as a borrowed bit set.
///
/// Bit `i` stands for the value at slot `i`, so bit zero is `⊥` and bit `i + 1`
/// is the `i`th target vertex in ascending name order. The block is as many
/// words as the network needs and the view is [`Copy`], so reading a domain
/// costs a slice reference rather than a copy of the bits.
///
/// A value whose slot is outside the block is treated as absent, so
/// [`Self::contains`] is false for it. That keeps every operation total, which
/// matters because a domain is read in the innermost loop of the solver.
///
/// Iteration is `⊥`-**last**, which is not the order of the bits; see
/// [`DomainIter`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Domain<'a> {
    bits: &'a [u64],
}

impl Domain<'static> {
    /// The empty domain, of no width at all.
    ///
    /// Every operation on it is the operation on a set holding nothing, so it
    /// is what a lookup for a variable the network does not have reads as.
    pub const EMPTY: Self = Self { bits: &[] };
}

impl<'a> Domain<'a> {
    /// A view of one block of domain words.
    #[inline]
    #[must_use]
    pub const fn new(bits: &'a [u64]) -> Self {
        Self { bits }
    }

    /// The bit pattern, one bit per value slot.
    #[inline]
    #[must_use]
    pub const fn bits(self) -> &'a [u64] {
        self.bits
    }

    /// Whether the domain holds a value.
    #[inline]
    #[must_use]
    pub fn contains(self, value: ValId) -> bool {
        self.bits
            .get(word_of(value))
            .is_some_and(|word| word & bit_of(value) != 0)
    }

    /// How many values the domain holds.
    #[inline]
    #[must_use]
    pub fn len(self) -> usize {
        // The one-word case is peeled because it is the whole of the measured
        // corpus, and a fold over a slice of length one does not reduce to a
        // single population count on its own.
        match self.bits {
            [only] => only.count_ones() as usize,
            words => words.iter().map(|word| word.count_ones() as usize).sum(),
        }
    }

    /// Whether the domain holds nothing, which makes its variable unsatisfiable.
    #[inline]
    #[must_use]
    pub fn is_empty(self) -> bool {
        match self.bits {
            [only] => *only == 0,
            words => words.iter().all(|word| *word == 0),
        }
    }

    /// The smallest value the domain holds.
    ///
    /// Smallest in the domain order, so a real value in preference to `⊥` and
    /// the alphabetically earliest target among real values.
    #[inline]
    #[must_use]
    pub fn first(self) -> Option<ValId> {
        self.iter().next()
    }

    /// The one value the domain holds, or `None` if it holds none or several.
    #[inline]
    #[must_use]
    pub fn only(self) -> Option<ValId> {
        let mut walk = self.iter();
        let found = walk.next()?;
        if walk.next().is_none() {
            Some(found)
        } else {
            None
        }
    }

    /// The values, in the domain order: reals ascending, then `⊥`.
    #[inline]
    #[must_use]
    pub fn iter(self) -> DomainIter<'a> {
        DomainIter::new(self.bits)
    }
}

impl<'a> IntoIterator for Domain<'a> {
    type Item = ValId;
    type IntoIter = DomainIter<'a>;

    #[inline]
    fn into_iter(self) -> DomainIter<'a> {
        self.iter()
    }
}

/// The values of a [`Domain`], in the domain order.
///
/// **The order is reals ascending, then `⊥`**, and the walk implements that
/// contract rather than inheriting it from the bit layout: `⊥` is bit zero, so
/// an ascending walk of the bits would yield it first. It is held back and
/// yielded once every real value has been, which is the order
/// [`ValId::order_key`] sorts in and the order the canonical tie-break among
/// tied optima is read in.
#[derive(Clone, Debug)]
pub struct DomainIter<'a> {
    /// The whole block, so that the walk can move on to the next word.
    bits: &'a [u64],
    /// The word being drained, with the bits already yielded cleared.
    current: u64,
    /// Which word `current` came from.
    word: usize,
    /// Whether `⊥` is still owed.
    bottom: bool,
}

impl<'a> DomainIter<'a> {
    /// Start a walk over one block of domain words.
    #[inline]
    #[must_use]
    fn new(bits: &'a [u64]) -> Self {
        let first = bits.first().copied().unwrap_or(0);
        Self {
            bits,
            // Bit zero is `⊥` and is owed at the end, so the walk over the reals
            // never sees it.
            current: first & !1,
            word: 0,
            bottom: first & 1 != 0,
        }
    }
}

impl Iterator for DomainIter<'_> {
    type Item = ValId;

    #[inline]
    fn next(&mut self) -> Option<ValId> {
        loop {
            if self.current != 0 {
                let bit = self.current.trailing_zeros();
                // Clearing the lowest set bit is what makes the walk ascending
                // within a word.
                self.current &= self.current - 1;
                // Saturating because the view is public and its width is the
                // caller's: a slot past what a `ValId` can number is one no
                // domain holds, and reading it as the last one is the same
                // silent-absence rule the rest of the type follows.
                let slot = u32::try_from(self.word)
                    .unwrap_or(u32::MAX)
                    .saturating_mul(u64::BITS)
                    .saturating_add(bit);
                return Some(ValId::from_index(slot));
            }
            match self.bits.get(self.word + 1) {
                Some(word) => {
                    self.word += 1;
                    self.current = *word;
                }
                None => break,
            }
        }
        if self.bottom {
            self.bottom = false;
            return Some(ValId::BOTTOM);
        }
        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.current.count_ones() as usize
            + self
                .bits
                .get(self.word + 1..)
                .unwrap_or_default()
                .iter()
                .map(|word| word.count_ones() as usize)
                .sum::<usize>()
            + usize::from(self.bottom);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for DomainIter<'_> {}

impl std::iter::FusedIterator for DomainIter<'_> {}

// ---------------------------------------------------------------------------
// Domains
// ---------------------------------------------------------------------------

/// Every domain of one network, in one contiguous bit set.
///
/// One block of [`Self::words`] machine words per variable, laid out back to
/// back, so a search that saves and restores the whole store on branching moves
/// it with one copy rather than one per variable. The width is a property of
/// the network: it is fixed at construction from the largest value count any
/// variable was offered, and it is what removes the fixed ceiling a
/// single-word domain would carry.
///
/// The store is used for two different jobs and both want the same layout: a
/// network's live domains, and a scratch odometer over the positions of one
/// cost function's scope. Nothing here knows which it is holding.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Domains {
    /// `variables · words` words, block per variable.
    bits: SmallVec<u64, 8>,
    /// Words per block. At least one, so every variable has a `⊥` bit.
    words: usize,
}

impl Default for Domains {
    /// A store over no variables at all.
    ///
    /// The width is one rather than zero: a block is never narrower than the
    /// word holding `⊥`, and the block arithmetic divides by it.
    fn default() -> Self {
        Self::new(0, 1)
    }
}

impl Domains {
    /// An empty store over `variables` blocks wide enough for `slots` values.
    ///
    /// `slots` counts `⊥`, so a variable offered `k` targets needs `k + 1`.
    #[must_use]
    pub fn new(variables: usize, slots: u32) -> Self {
        let words = words_for(slots).max(1);
        Self {
            bits: smallvec::from_elem(0u64, variables.saturating_mul(words)),
            words,
        }
    }

    /// An empty store of the same width as another, over its own block count.
    ///
    /// The odometer a cost function is walked with has one block per scope
    /// position and has to name the same values the network does, so it takes
    /// its width from the network rather than from its own arity.
    #[must_use]
    pub fn like(other: &Self, blocks: usize) -> Self {
        Self {
            bits: smallvec::from_elem(0u64, blocks.saturating_mul(other.words)),
            words: other.words,
        }
    }

    /// How many words one block spans.
    #[inline]
    #[must_use]
    pub const fn words(&self) -> usize {
        self.words
    }

    /// Overwrite one block from a block of the same width.
    ///
    /// A block of the wrong width is ignored, on the same reasoning as
    /// [`Self::copy_from`].
    #[inline]
    pub fn copy_block(&mut self, var: VarId, block: &[u64]) {
        if block.len() != self.words {
            return;
        }
        if let Some(target) = self.block_mut(var) {
            target.copy_from_slice(block);
        }
    }

    /// How many blocks the store holds.
    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bits.len() / self.words
    }

    /// Whether the store holds no blocks at all.
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bits.is_empty()
    }

    /// One variable's block, or an empty one if it has none.
    #[inline]
    #[must_use]
    pub fn block(&self, var: VarId) -> &[u64] {
        // Saturating rather than checked on both steps: a block index this
        // arithmetic cannot represent is one the store does not hold, and the
        // range lookup reports that as absent. An overflowing `+` here would
        // panic instead, which is the one answer a total accessor must not
        // give.
        let start = var.index().saturating_mul(self.words);
        let end = start.saturating_add(self.words);
        self.bits.get(start..end).unwrap_or_default()
    }

    /// One variable's block, mutably.
    #[inline]
    fn block_mut(&mut self, var: VarId) -> Option<&mut [u64]> {
        let start = var.index().checked_mul(self.words)?;
        let end = start.checked_add(self.words)?;
        self.bits.get_mut(start..end)
    }

    /// One variable's domain.
    #[inline]
    #[must_use]
    pub fn get(&self, var: VarId) -> Domain<'_> {
        Domain::new(self.block(var))
    }

    /// Add a value to one variable's domain.
    #[inline]
    pub fn insert(&mut self, var: VarId, value: ValId) {
        let word = word_of(value);
        if let Some(cell) = self.block_mut(var).and_then(|block| block.get_mut(word)) {
            *cell |= bit_of(value);
        }
    }

    /// Take a value out of one variable's domain.
    #[inline]
    pub fn remove(&mut self, var: VarId, value: ValId) {
        let word = word_of(value);
        if let Some(cell) = self.block_mut(var).and_then(|block| block.get_mut(word)) {
            *cell &= !bit_of(value);
        }
    }

    /// Reduce one variable's domain to a single value, or to nothing if it did
    /// not hold that value.
    pub fn assign(&mut self, var: VarId, value: ValId) {
        let word = word_of(value);
        let bit = bit_of(value);
        if let Some(block) = self.block_mut(var) {
            let held = block.get(word).is_some_and(|cell| cell & bit != 0);
            block.fill(0);
            if held {
                if let Some(cell) = block.get_mut(word) {
                    *cell = bit;
                }
            }
        }
    }

    /// Whether one variable's domain holds a value.
    #[inline]
    #[must_use]
    pub fn contains(&self, var: VarId, value: ValId) -> bool {
        self.get(var).contains(value)
    }

    /// Whether some variable has no values left, which makes the network
    /// unsatisfiable.
    #[must_use]
    pub fn any_empty(&self) -> bool {
        (0..self.len())
            .filter_map(|index| u32::try_from(index).ok())
            .any(|index| self.get(VarId::new(index)).is_empty())
    }

    /// Overwrite every block from another store of the same shape.
    ///
    /// The counterpart of copy-on-branch, and one copy rather than one per
    /// variable. A store of a different shape is ignored rather than partially
    /// applied, since a partial restore is a silently wrong network.
    pub fn copy_from(&mut self, other: &Self) {
        if self.words == other.words && self.bits.len() == other.bits.len() {
            self.bits.copy_from_slice(&other.bits);
        }
    }

    /// Every word of every block, in variable order.
    #[inline]
    #[must_use]
    pub fn bits(&self) -> &[u64] {
        &self.bits
    }

    /// Every variable identifier the store has a block for.
    #[inline]
    pub fn variable_ids(&self) -> impl Iterator<Item = VarId> + '_ {
        (0..self.len()).filter_map(|index| u32::try_from(index).ok().map(VarId::new))
    }
}

// ---------------------------------------------------------------------------
// Variable
// ---------------------------------------------------------------------------

/// One variable of the network: a source vertex and the targets it may take.
///
/// The value list is sorted by ascending target vertex name and deduplicated,
/// and `⊥` is the slot past its end. Both are established at construction and
/// the fields are private so that neither can be undone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variable {
    name: Name,
    values: Vec<Name>,
}

impl Variable {
    /// The source vertex this variable stands for.
    #[inline]
    #[must_use]
    pub const fn name(&self) -> &Name {
        &self.name
    }

    /// The target vertices this variable may take, in ascending name order.
    ///
    /// `⊥` is not among them: it is not a target vertex.
    #[inline]
    #[must_use]
    pub fn values(&self) -> &[Name] {
        &self.values
    }

    /// How many table slots this variable spans, which is one per real value
    /// plus one for `⊥`.
    #[inline]
    #[must_use]
    pub fn slots(&self) -> usize {
        self.values.len() + 1
    }

    /// The table slot a value occupies, or `None` if it is not a value of this
    /// variable.
    ///
    /// **A table slot is not a domain bit.** `⊥` is bit zero of a [`Domain`]
    /// and the *last* slot of a cost table, and the two numberings differ by
    /// exactly that rotation: real value `i` is bit `i + 1` and slot `i`. Cost
    /// tables are row-major over slots, so keeping `⊥` last there is what makes
    /// a table's layout independent of how a domain stores its bits, and it is
    /// why moving `⊥` to bit zero left every table untouched.
    ///
    /// Both numberings agree on the *order*, which is the thing the search's
    /// tie-break reads: `⊥` sorts last, by [`ValId::order_key`] and by
    /// [`DomainIter`] alike.
    #[inline]
    #[must_use]
    pub fn slot(&self, value: ValId) -> Option<usize> {
        if value.is_bottom() {
            Some(self.values.len())
        } else if value.index() < self.values.len() {
            Some(value.index())
        } else {
            None
        }
    }

    /// The value standing for a target vertex, or `None` if this variable
    /// cannot take it.
    ///
    /// A binary search over the sorted value list.
    #[must_use]
    pub fn value_id(&self, target: &Name) -> Option<ValId> {
        let slot = self
            .values
            .binary_search_by(|value| value.as_str().cmp(target.as_str()))
            .ok()?;
        u32::try_from(slot).ok().map(ValId::real)
    }

    /// The target vertex a value stands for, or `None` for `⊥` and for a value
    /// this variable cannot take.
    #[inline]
    #[must_use]
    pub fn value_name(&self, value: ValId) -> Option<&Name> {
        if value.is_bottom() {
            return None;
        }
        self.values.get(value.index())
    }
}

// ---------------------------------------------------------------------------
// Cost functions
// ---------------------------------------------------------------------------

/// A cost function of arity two or more, over a scope of distinct variables.
///
/// Scopes are strictly ascending in [`VarId`] order and no two cost functions in
/// one network share one. Construction is private to this module so that both
/// facts hold of every value of this type that exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostFunction {
    scope: Vec<VarId>,
    table: Vec<Cost>,
}

impl CostFunction {
    /// The variables this function constrains, in ascending order.
    #[inline]
    #[must_use]
    pub fn scope(&self) -> &[VarId] {
        &self.scope
    }

    /// The cost table, row-major with the last scope variable varying fastest.
    #[inline]
    #[must_use]
    pub fn table(&self) -> &[Cost] {
        &self.table
    }

    /// How many variables the function constrains.
    #[inline]
    #[must_use]
    pub fn arity(&self) -> usize {
        self.scope.len()
    }
}

// ---------------------------------------------------------------------------
// The network
// ---------------------------------------------------------------------------

/// A cost function network over the source vertices of one schema.
///
/// Immutable once built. Solvers that transform cost tables work on their own
/// copies, which is what leaves [`Self::evaluate`] free of any transformed
/// state.
#[derive(Clone, Debug, PartialEq)]
pub struct Cfn {
    variables: Vec<Variable>,
    domains: Domains,
    unary: Vec<Vec<Cost>>,
    functions: Vec<CostFunction>,
    c_empty: Cost,
    radix: u64,
    weights: CostWeights,
}

impl Cfn {
    /// The variables, indexed by [`VarId`].
    #[inline]
    #[must_use]
    pub fn variables(&self) -> &[Variable] {
        &self.variables
    }

    /// One variable, or `None` if the identifier is out of range.
    #[inline]
    #[must_use]
    pub fn variable(&self, var: VarId) -> Option<&Variable> {
        self.variables.get(var.index())
    }

    /// Every variable identifier, in ascending order.
    #[inline]
    pub fn variable_ids(&self) -> impl Iterator<Item = VarId> + '_ {
        (0..self.variables.len()).filter_map(|index| u32::try_from(index).ok().map(VarId::new))
    }

    /// How many variables the network has, which is how many source vertices
    /// it was built over.
    #[inline]
    #[must_use]
    pub fn n_variables(&self) -> usize {
        self.variables.len()
    }

    /// How many cost functions of arity two or more the network has.
    ///
    /// Unary cost is one table per variable rather than a function, so it is not
    /// counted here.
    #[inline]
    #[must_use]
    pub fn n_functions(&self) -> usize {
        self.functions.len()
    }

    /// The size of the largest domain, `⊥` included.
    ///
    /// This is the `d` of the complexity estimates the dispatcher compares
    /// against its budget.
    #[must_use]
    pub fn max_domain(&self) -> usize {
        self.domains
            .variable_ids()
            .map(|var| self.domains.get(var).len())
            .max()
            .unwrap_or(0)
    }

    /// The values one variable may still take, or `None` if the identifier is
    /// out of range.
    #[inline]
    #[must_use]
    pub fn domain(&self, var: VarId) -> Option<Domain<'_>> {
        if var.index() < self.variables.len() {
            Some(self.domains.get(var))
        } else {
            None
        }
    }

    /// Every domain, in one contiguous bit set.
    ///
    /// This is what a working copy starts from, so it is handed out whole
    /// rather than one variable at a time.
    #[inline]
    #[must_use]
    pub const fn domains(&self) -> &Domains {
        &self.domains
    }

    /// The unary cost table of one variable, indexed by slot.
    #[inline]
    #[must_use]
    pub fn unary(&self, var: VarId) -> Option<&[Cost]> {
        self.unary.get(var.index()).map(Vec::as_slice)
    }

    /// The unary cost of one value of one variable.
    #[inline]
    #[must_use]
    pub fn unary_cost(&self, var: VarId, value: ValId) -> Option<Cost> {
        let slot = self.variables.get(var.index())?.slot(value)?;
        self.unary.get(var.index())?.get(slot).copied()
    }

    /// The cost functions of arity two or more.
    #[inline]
    #[must_use]
    pub fn functions(&self) -> &[CostFunction] {
        &self.functions
    }

    /// The one cost function on a scope, if the network has one.
    ///
    /// At most one can exist, which is the scope uniqueness invariant. The
    /// lookup is linear in the number of cost functions.
    ///
    /// `scope` must be strictly ascending, which is the form
    /// [`CfnBuilder::add_function`] accepts and the form every scope in the
    /// network is stored in. A descending or unordered scope matches nothing
    /// and reads as `None`, so it reports "no function here" for a scope that
    /// may well carry one.
    #[must_use]
    pub fn function_for(&self, scope: &[VarId]) -> Option<&CostFunction> {
        self.functions
            .iter()
            .find(|function| function.scope == scope)
    }

    /// The constant term `c_∅`.
    ///
    /// It holds the components of the objective that no assignment can change,
    /// and, once soft local consistency has run, the certified lower bound.
    #[inline]
    #[must_use]
    pub const fn c_empty(&self) -> Cost {
        self.c_empty
    }

    /// The radix separating the quality cost from the drop count.
    ///
    /// A property of one network rather than a global, because two networks over
    /// differently sized sources have different radices and their costs are not
    /// comparable.
    #[inline]
    #[must_use]
    pub const fn radix(&self) -> u64 {
        self.radix
    }

    /// The component weights the cost functions were built with.
    #[inline]
    #[must_use]
    pub const fn weights(&self) -> CostWeights {
        self.weights
    }

    /// How many entries a table over a scope has, or `None` if the scope names a
    /// variable that does not exist or the product overflows.
    #[must_use]
    pub fn table_length(&self, scope: &[VarId]) -> Option<usize> {
        table_length(&self.variables, scope)
    }

    /// Where one tuple of values sits in a table over a scope.
    ///
    /// `values` is positional against `scope`. `None` when the two differ in
    /// length, when the scope names a variable that does not exist, or when a
    /// value is not one the corresponding variable can take.
    #[must_use]
    pub fn table_index(&self, scope: &[VarId], values: &[ValId]) -> Option<usize> {
        if scope.len() != values.len() {
            return None;
        }
        let mut offset = 0usize;
        for (var, value) in scope.iter().zip(values) {
            let variable = self.variables.get(var.index())?;
            let slot = variable.slot(*value)?;
            offset = offset.checked_mul(variable.slots())?.checked_add(slot)?;
        }
        Some(offset)
    }

    /// The cost of an assignment.
    ///
    /// A straight `⊕`-sum of `c_∅`, one unary entry per variable and one entry
    /// per cost function, taken against [`Cost::TOP_SENTINEL`] so that nothing
    /// is clamped to a search bound. It reads no transformed cost table and no
    /// propagation state, which is what lets it certify a solver's answer rather
    /// than restate it.
    ///
    /// An assignment giving some variable a value that variable cannot take is
    /// infeasible, and evaluates to [`Cost::TOP_SENTINEL`].
    ///
    /// # Panics
    ///
    /// If the assignment does not give exactly one value per variable. A
    /// partial assignment is a different object with a different scorer, and
    /// silently reading one as the other would let a solver report a cost for an
    /// assignment it never completed.
    #[must_use]
    pub fn evaluate(&self, assignment: &Assignment) -> Cost {
        assert_eq!(
            assignment.len(),
            self.variables.len(),
            "an assignment must give one value to every variable of the network"
        );
        let top = Cost::TOP_SENTINEL;
        let mut total = self.c_empty;

        for (var, value) in assignment.pairs() {
            let Some(variable) = self.variables.get(var.index()) else {
                return top;
            };
            let Some(slot) = variable.slot(value) else {
                return top;
            };
            let Some(&entry) = self
                .unary
                .get(var.index())
                .and_then(|table| table.get(slot))
            else {
                return top;
            };
            total = total.combine(entry, top);
        }

        for function in &self.functions {
            let Some(offset) = self.function_offset(function, assignment) else {
                return top;
            };
            let Some(&entry) = function.table.get(offset) else {
                return top;
            };
            total = total.combine(entry, top);
        }

        total
    }

    /// The quality an assignment reads back out of the integer objective.
    ///
    /// `1 − quality_cost / COST_SCALE`, so higher is better and a perfect match
    /// reads one. The drop count is deliberately excluded: it is the secondary
    /// component of the objective and reporting it inside the quality would make
    /// a span that covers less of the source look worse on a scale that is
    /// supposed to measure how well the covered part matches.
    ///
    /// An infeasible assignment reads zero.
    ///
    /// **The reading is comparable only within one schema pair.** A component
    /// with an empty denominator contributes nothing, so a source with no edges
    /// reads its edge component at one however little of the source survives:
    /// an assignment dropping the whole source can still read well above zero.
    /// What the number ranks is how well the covered part matches, and the drop
    /// count, which [`Cost::drop_part`] reads, is the separate answer to how
    /// much was covered. Two networks over different sources answer different
    /// questions and their readings do not order each other.
    ///
    /// This is the only float this module produces, and it is produced by
    /// division rather than by accumulation: [`Self::evaluate`] has already
    /// summed in integers.
    ///
    /// # Panics
    ///
    /// If the assignment does not give exactly one value per variable, by way of
    /// [`Self::evaluate`].
    #[must_use]
    pub fn quality_of(&self, assignment: &Assignment) -> f64 {
        let units = self.evaluate(assignment).quality_part(self.radix);
        // Clamped at the scale so that `⊤`, which is far above it, reads as the
        // worst finite quality rather than as a large negative number. The
        // clamped value is at most 10^9, which is below `u32::MAX`, so the
        // conversion below is exact and the fallback is unreachable.
        let units = u32::try_from(units.min(COST_SCALE)).unwrap_or(u32::MAX);
        1.0 - f64::from(units) / COST_SCALE_FLOAT
    }

    /// Where a function's entry for an assignment sits in its table.
    fn function_offset(&self, function: &CostFunction, assignment: &Assignment) -> Option<usize> {
        let mut offset = 0usize;
        for var in &function.scope {
            let variable = self.variables.get(var.index())?;
            let value = assignment.get(*var)?;
            let slot = variable.slot(value)?;
            offset = offset.checked_mul(variable.slots())?.checked_add(slot)?;
        }
        Some(offset)
    }
}

/// Whether the cost tables built so far are still inside the memory budget.
///
/// The check is on entries rather than on an allocation that already happened,
/// so a network too large to hold is refused before it is held.
fn check_budget(entries: u64, budget: u64) -> Result<(), CfnError> {
    let cell = u64::try_from(size_of::<Cost>()).unwrap_or(u64::MAX);
    let bytes = entries.saturating_mul(cell);
    if bytes > budget {
        return Err(CfnError::OverMemoryBudget {
            entries,
            bytes,
            budget,
        });
    }
    Ok(())
}

/// How many entries a table over a scope has.
fn table_length(variables: &[Variable], scope: &[VarId]) -> Option<usize> {
    let mut length = 1usize;
    for var in scope {
        let variable = variables.get(var.index())?;
        length = length.checked_mul(variable.slots())?;
    }
    Some(length)
}

// ---------------------------------------------------------------------------
// The builder
// ---------------------------------------------------------------------------

/// The one way to build a [`Cfn`].
///
/// It exists to make the invariants of the network unstatable rather than
/// merely documented. The value list of every variable is sorted and
/// deduplicated here, `⊥` is added here, and a cost function offered on a scope
/// that already has one is merged into it here rather than appended beside it.
///
/// Every method that adds cost **accumulates**: adding twice is adding the sum.
/// There is no setter, because a setter would let a second contribution to one
/// entry silently discard the first, and the whole objective is a sum of
/// contributions from different schema constructs.
#[derive(Clone, Debug)]
pub struct CfnBuilder {
    variables: Vec<Variable>,
    domains: Domains,
    unary: Vec<Vec<Cost>>,
    functions: Vec<CostFunction>,
    by_scope: FxHashMap<Vec<VarId>, usize>,
    c_empty: Cost,
    radix: u64,
    weights: CostWeights,
    /// Cost table entries allocated so far, unary tables included.
    entries: u64,
    /// The bytes those entries may reach before the builder refuses.
    mem_bytes: u64,
}

impl CfnBuilder {
    /// Start a network over a fixed set of variables.
    ///
    /// Each entry is a source vertex name and the target vertex names that
    /// vertex may map to. The list is sorted by ascending name and deduplicated,
    /// which is what fixes the value order; `⊥` is added to every domain,
    /// including the domains of variables offered no targets at all, which is
    /// the common case on real schema pairs and the reason the search always has
    /// a feasible assignment.
    ///
    /// The variable set is fixed here rather than grown later because the
    /// coverage radix is a function of its size and every packed cost added
    /// afterwards depends on the radix.
    ///
    /// # Errors
    ///
    /// [`CfnError::TooManyVariables`] if there are more variables than a
    /// [`VarId`] can number; [`CfnError::DuplicateVariable`] if one name is
    /// offered twice, since one variable per source vertex is what makes an
    /// assignment a vertex map; and [`CfnError::OverMemoryBudget`] if the unary
    /// tables alone would exceed [`DEFAULT_MEM_BYTES`].
    ///
    /// **No domain size is refused.** A source vertex is offered every
    /// kind-compatible target vertex, so a wide record type or a
    /// line-per-vertex parse gives one variable hundreds of values, and that is
    /// an ordinary network. What is refused is measured memory, and
    /// [`Self::with_mem_bytes`] is where a caller sets the figure.
    pub fn new(variables: Vec<(Name, Vec<Name>)>, weights: CostWeights) -> Result<Self, CfnError> {
        Self::with_mem_bytes(variables, weights, DEFAULT_MEM_BYTES)
    }

    /// [`Self::new`], against an explicit memory budget for the cost tables.
    ///
    /// The budget is checked before anything is allocated, against the entry
    /// count the variable list implies and then again against every cost
    /// function offered. It is the same quantity the dispatcher's
    /// [`SearchBudget::mem_bytes`](super::SearchBudget::mem_bytes) bounds for
    /// exact inference, applied one step earlier: a network too large to hold
    /// is refused where it would be built rather than where it would be solved.
    ///
    /// # Errors
    ///
    /// As [`Self::new`], with the budget this call names.
    pub fn with_mem_bytes(
        variables: Vec<(Name, Vec<Name>)>,
        weights: CostWeights,
        mem_bytes: usize,
    ) -> Result<Self, CfnError> {
        let count = u32::try_from(variables.len()).map_err(|_| CfnError::TooManyVariables {
            count: variables.len(),
        })?;
        let mem_bytes = u64::try_from(mem_bytes).unwrap_or(u64::MAX);

        let mut built = Vec::with_capacity(variables.len());
        let mut slot_counts = Vec::with_capacity(variables.len());
        let mut seen: FxHashMap<Name, usize> = FxHashMap::default();
        let mut entries = 0u64;
        let mut widest = 1u32;
        for (position, (name, mut values)) in variables.into_iter().enumerate() {
            if let Some(&first) = seen.get(&name) {
                return Err(CfnError::DuplicateVariable {
                    variable: name,
                    first,
                    second: position,
                });
            }
            seen.insert(name.clone(), position);
            values.sort_by(|left, right| left.as_str().cmp(right.as_str()));
            values.dedup();
            let slots = u32::try_from(values.len())
                .ok()
                .and_then(|real| real.checked_add(1))
                .ok_or_else(|| CfnError::TooManyValues {
                    variable: name.clone(),
                    values: values.len(),
                })?;
            widest = widest.max(slots);
            entries = entries.saturating_add(u64::from(slots));
            slot_counts.push(slots);
            built.push(Variable { name, values });
        }
        check_budget(entries, mem_bytes)?;

        let mut domains = Domains::new(built.len(), widest);
        let mut unary = Vec::with_capacity(built.len());
        for (index, slots) in slot_counts.into_iter().enumerate() {
            let var = VarId::new(u32::try_from(index).unwrap_or(u32::MAX));
            // `⊥` is slot zero and the reals follow it, so a full domain is the
            // low `slots` bits of the block.
            for slot in 0..slots {
                domains.insert(var, ValId::from_index(slot));
            }
            unary.push(vec![Cost::BOT; slots as usize]);
        }

        Ok(Self {
            variables: built,
            domains,
            unary,
            functions: Vec::new(),
            by_scope: FxHashMap::default(),
            c_empty: Cost::BOT,
            radix: coverage_radix(count),
            weights,
            entries,
            mem_bytes,
        })
    }

    /// The variables, indexed by [`VarId`].
    ///
    /// The caller needs these to translate target vertex names into [`ValId`]s
    /// before it can add cost.
    #[inline]
    #[must_use]
    pub fn variables(&self) -> &[Variable] {
        &self.variables
    }

    /// One variable, or `None` if the identifier is out of range.
    #[inline]
    #[must_use]
    pub fn variable(&self, var: VarId) -> Option<&Variable> {
        self.variables.get(var.index())
    }

    /// Every variable identifier, in ascending order.
    #[inline]
    pub fn variable_ids(&self) -> impl Iterator<Item = VarId> + '_ {
        (0..self.variables.len()).filter_map(|index| u32::try_from(index).ok().map(VarId::new))
    }

    /// How many variables the network has.
    #[inline]
    #[must_use]
    pub fn n_variables(&self) -> usize {
        self.variables.len()
    }

    /// The radix packed costs must be built against.
    ///
    /// Fixed by the variable count, so it is available before any cost is added.
    #[inline]
    #[must_use]
    pub const fn radix(&self) -> u64 {
        self.radix
    }

    /// The component weights the network carries.
    #[inline]
    #[must_use]
    pub const fn weights(&self) -> CostWeights {
        self.weights
    }

    /// The constant term as it stands.
    #[inline]
    #[must_use]
    pub const fn c_empty(&self) -> Cost {
        self.c_empty
    }

    /// How many entries a table over a scope must have.
    #[must_use]
    pub fn table_length(&self, scope: &[VarId]) -> Option<usize> {
        table_length(&self.variables, scope)
    }

    /// Add to the constant term.
    ///
    /// The constant carries the parts of the objective no assignment can change:
    /// the vacuous branches of the components whose denominators are empty, for
    /// instance. It never decreases.
    #[inline]
    pub const fn add_empty(&mut self, cost: Cost) {
        self.c_empty = self.c_empty.combine(cost, Cost::TOP_SENTINEL);
    }

    /// Add to the unary cost of one value of one variable.
    ///
    /// # Errors
    ///
    /// [`CfnError::UnknownVariable`] if the identifier is out of range, and
    /// [`CfnError::UnknownValue`] if the value is not one that variable can
    /// take.
    pub fn add_unary(&mut self, var: VarId, value: ValId, cost: Cost) -> Result<(), CfnError> {
        let variable = self
            .variables
            .get(var.index())
            .ok_or(CfnError::UnknownVariable { variable: var })?;
        let slot = variable.slot(value).ok_or(CfnError::UnknownValue {
            variable: var,
            value,
        })?;
        let Some(entry) = self
            .unary
            .get_mut(var.index())
            .and_then(|t| t.get_mut(slot))
        else {
            return Err(CfnError::UnknownValue {
                variable: var,
                value,
            });
        };
        *entry = entry.combine(cost, Cost::TOP_SENTINEL);
        Ok(())
    }

    /// Add a whole unary table to one variable, slot by slot.
    ///
    /// The table is indexed by slot, so its length is the variable's real value
    /// count plus one and its last entry is the cost of `⊥`.
    ///
    /// # Errors
    ///
    /// [`CfnError::UnknownVariable`] if the identifier is out of range, and
    /// [`CfnError::TableSizeMismatch`] if the table is not the length the
    /// variable's slot count demands.
    pub fn add_unary_table(&mut self, var: VarId, table: &[Cost]) -> Result<(), CfnError> {
        let variable = self
            .variables
            .get(var.index())
            .ok_or(CfnError::UnknownVariable { variable: var })?;
        let expected = variable.slots();
        if table.len() != expected {
            return Err(CfnError::TableSizeMismatch {
                scope: vec![var],
                expected,
                found: table.len(),
            });
        }
        let Some(existing) = self.unary.get_mut(var.index()) else {
            return Err(CfnError::UnknownVariable { variable: var });
        };
        for (entry, added) in existing.iter_mut().zip(table) {
            *entry = entry.combine(*added, Cost::TOP_SENTINEL);
        }
        Ok(())
    }

    /// Add a cost function on a scope, merging it into whatever is already
    /// there.
    ///
    /// This is the scope uniqueness invariant in force. A scope that already
    /// carries a function is merged into pointwise rather than appended beside,
    /// so a network never holds two functions on one scope however many times
    /// the caller adds one. Parallel source edges between one vertex pair are
    /// the case that makes this mandatory rather than tidy.
    ///
    /// Arity is folded rather than rejected: a scope of one variable goes to
    /// that variable's unary table and an empty scope goes to `c_∅`, which is
    /// where each belongs and which keeps the invariant total.
    ///
    /// The table is row-major with the last scope variable varying fastest; the
    /// module docs give the index formula and [`Self::table_length`] gives the
    /// length.
    ///
    /// # Errors
    ///
    /// [`CfnError::UnknownVariable`] if the scope names a variable that does not
    /// exist, [`CfnError::ScopeNotAscending`] if the scope is not strictly
    /// ascending, which also rejects a repeated variable, and
    /// [`CfnError::TableSizeMismatch`] if the table is not the length the scope
    /// demands.
    pub fn add_function(&mut self, scope: &[VarId], table: Vec<Cost>) -> Result<(), CfnError> {
        let expected = self.validate_scope(scope)?;
        if table.len() != expected {
            return Err(CfnError::TableSizeMismatch {
                scope: scope.to_vec(),
                expected,
                found: table.len(),
            });
        }

        match scope {
            [] => {
                for cost in table {
                    self.add_empty(cost);
                }
                Ok(())
            }
            [var] => self.add_unary_table(*var, &table),
            _ => {
                if let Some(&index) = self.by_scope.get(scope) {
                    let Some(existing) = self.functions.get_mut(index) else {
                        return Err(CfnError::UnknownVariable { variable: scope[0] });
                    };
                    for (entry, added) in existing.table.iter_mut().zip(table) {
                        *entry = entry.combine(added, Cost::TOP_SENTINEL);
                    }
                } else {
                    // Only a new scope allocates: merging into one that already
                    // has a function reuses its table.
                    self.entries = self
                        .entries
                        .saturating_add(u64::try_from(table.len()).unwrap_or(u64::MAX));
                    check_budget(self.entries, self.mem_bytes)?;
                    self.by_scope.insert(scope.to_vec(), self.functions.len());
                    self.functions.push(CostFunction {
                        scope: scope.to_vec(),
                        table,
                    });
                }
                Ok(())
            }
        }
    }

    /// Finish the network.
    #[must_use]
    pub fn build(self) -> Cfn {
        debug_assert!(
            self.functions.len() == self.by_scope.len(),
            "every cost function must own its scope"
        );
        debug_assert!(
            self.functions.iter().all(|function| function.table.len()
                == table_length(&self.variables, &function.scope).unwrap_or(0)),
            "every cost table must span its scope"
        );
        Cfn {
            variables: self.variables,
            domains: self.domains,
            unary: self.unary,
            functions: self.functions,
            c_empty: self.c_empty,
            radix: self.radix,
            weights: self.weights,
        }
    }

    /// Check a scope and return the table length it demands.
    fn validate_scope(&self, scope: &[VarId]) -> Result<usize, CfnError> {
        let mut previous: Option<VarId> = None;
        for var in scope {
            if self.variables.get(var.index()).is_none() {
                return Err(CfnError::UnknownVariable { variable: *var });
            }
            if let Some(prior) = previous {
                if *var <= prior {
                    return Err(CfnError::ScopeNotAscending {
                        scope: scope.to_vec(),
                    });
                }
            }
            previous = Some(*var);
        }
        table_length(&self.variables, scope).ok_or_else(|| CfnError::TableTooLarge {
            scope: scope.to_vec(),
        })
    }
}

/// Why a network could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CfnError {
    /// More source vertices than a variable identifier can number.
    #[error("a network cannot hold {count} variables")]
    TooManyVariables {
        /// How many variables were offered.
        count: usize,
    },

    /// Two variables stood for the same source vertex.
    ///
    /// One variable per source vertex is what makes an assignment a vertex map:
    /// a repeated name would give one source vertex two simultaneous images,
    /// count its unary cost twice, and size the coverage radix for a vertex set
    /// that does not exist.
    #[error("variable `{variable}` was offered twice, at positions {first} and {second}")]
    DuplicateVariable {
        /// The name offered more than once.
        variable: Name,
        /// Where it was first offered.
        first: usize,
        /// Where it was offered again.
        second: usize,
    },

    /// A variable was offered more distinct targets than a [`ValId`] can
    /// number.
    ///
    /// The numbering is a `u32` with slot zero taken by `⊥`, so this is
    /// `u32::MAX` targets on one variable. Nothing that reads a schema reaches
    /// it: the memory budget binds first by four orders of magnitude, and it is
    /// here so that the numbering cannot wrap in silence.
    #[error("variable `{variable}` was offered {values} targets, more than a value can number")]
    TooManyValues {
        /// The source vertex the variable stands for.
        variable: Name,
        /// How many distinct targets it was offered.
        values: usize,
    },

    /// The cost tables the network was asked to hold exceed the memory budget.
    ///
    /// This is a measurement, not a capacity: it names the bytes the tables
    /// come to and the budget they were checked against, and both move with the
    /// caller's [`CfnBuilder::with_mem_bytes`]. No domain size is refused on
    /// its own account.
    #[error(
        "the network's cost tables need {bytes} bytes for {entries} entries, \
         above the budget of {budget} bytes"
    )]
    OverMemoryBudget {
        /// Cost table entries the network was asked to allocate.
        entries: u64,
        /// What those entries come to in bytes.
        bytes: u64,
        /// The budget they were checked against.
        budget: u64,
    },

    /// A variable identifier named no variable of the network.
    #[error("no variable {variable:?} in this network")]
    UnknownVariable {
        /// The identifier.
        variable: VarId,
    },

    /// A value identifier named no value of its variable.
    #[error("variable {variable:?} cannot take value {value:?}")]
    UnknownValue {
        /// The variable.
        variable: VarId,
        /// The value it was offered.
        value: ValId,
    },

    /// A scope was not strictly ascending, which also covers a repeated
    /// variable.
    ///
    /// A repeated variable is the self-loop case: a source edge from a vertex to
    /// itself constrains one variable along the diagonal of a binary table, and
    /// folding that diagonal into the unary table is the caller's job because
    /// only the caller knows whether the diagonal has already been counted.
    #[error("scope {scope:?} is not strictly ascending")]
    ScopeNotAscending {
        /// The scope offered.
        scope: Vec<VarId>,
    },

    /// A table was not the length its scope demands.
    #[error("scope {scope:?} demands {expected} entries, but {found} were offered")]
    TableSizeMismatch {
        /// The scope offered.
        scope: Vec<VarId>,
        /// How many entries the scope demands.
        expected: usize,
        /// How many were offered.
        found: usize,
    },

    /// A scope's table would have more entries than an index can address.
    #[error("scope {scope:?} spans more entries than can be addressed")]
    TableTooLarge {
        /// The scope offered.
        scope: Vec<VarId>,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::solve::cost::DEFAULT_WEIGHTS;

    fn name(text: &str) -> Name {
        Name::new(text)
    }

    fn cost(units: u64) -> Cost {
        Cost::from_raw(units)
    }

    const A: VarId = VarId::new(0);
    const B: VarId = VarId::new(1);

    /// Two variables: `a` over targets `x` and `y`, `b` over target `p`.
    ///
    /// So `a` has slots `[x, y, ⊥]` and `b` has slots `[p, ⊥]`, and a binary
    /// table over `[a, b]` has six entries indexed `slot_a · 2 + slot_b`.
    fn two_variable_builder() -> CfnBuilder {
        CfnBuilder::new(
            vec![
                (name("a"), vec![name("y"), name("x")]),
                (name("b"), vec![name("p")]),
            ],
            DEFAULT_WEIGHTS,
        )
        .unwrap()
    }

    fn assignment(values: &[ValId]) -> Assignment {
        Assignment::from_values(values.to_vec())
    }

    /// Every total assignment over the network's domains, in ascending order.
    fn all_assignments(cfn: &Cfn) -> Vec<Assignment> {
        let mut out = vec![Vec::new()];
        for var in cfn.variable_ids() {
            let domain = cfn.domain(var).unwrap();
            let mut next = Vec::new();
            for partial in &out {
                for value in domain {
                    let mut extended = partial.clone();
                    extended.push(value);
                    next.push(extended);
                }
            }
            out = next;
        }
        out.into_iter().map(Assignment::from_values).collect()
    }

    #[test]
    fn the_cost_scale_float_matches_the_scale() {
        assert_eq!(COST_SCALE, 1_000_000_000);
        assert!((COST_SCALE_FLOAT - 1_000_000_000.0).abs() < f64::EPSILON);
    }

    // -- Domain ------------------------------------------------------------

    /// A one-block store holding exactly the given values.
    fn domain_of(slots: u32, values: &[ValId]) -> Domains {
        let mut store = Domains::new(1, slots);
        for value in values {
            store.insert(A, *value);
        }
        store
    }

    #[test]
    fn a_domain_round_trips_through_its_bits() {
        for bits in [
            vec![0u64],
            vec![1],
            vec![0b1010_1010],
            vec![0x8000_0000_0000_0001],
            vec![u64::MAX],
            vec![u64::MAX, 0b1011],
            vec![0, 1 << 63, 3],
        ] {
            let domain = Domain::new(&bits);
            let expected: usize = bits.iter().map(|word| word.count_ones() as usize).sum();
            assert_eq!(domain.len(), expected);
            assert_eq!(domain.is_empty(), expected == 0);

            let slots = u32::try_from(bits.len()).unwrap() * u64::BITS;
            let mut rebuilt = Domains::new(1, slots);
            for value in domain {
                rebuilt.insert(A, value);
            }
            assert_eq!(rebuilt.get(A).bits(), bits.as_slice());
        }
    }

    #[test]
    fn a_domain_iterates_reals_ascending_then_bottom() {
        let store = domain_of(
            64,
            &[
                ValId::real(7),
                ValId::real(0),
                ValId::real(30),
                ValId::real(3),
                ValId::BOTTOM,
            ],
        );
        let seen: Vec<ValId> = store.get(A).iter().collect();
        assert_eq!(
            seen,
            vec![
                ValId::real(0),
                ValId::real(3),
                ValId::real(7),
                ValId::real(30),
                ValId::BOTTOM,
            ]
        );
        // The walk agrees with the order `Ord` reports, which is what the
        // canonical tie-break among tied optima is read in.
        assert!(seen.windows(2).all(|pair| pair[0] < pair[1]));
        // And it is *not* the order of the stored slots: `⊥` is slot zero.
        let raw: Vec<u32> = seen.iter().map(|value| value.raw()).collect();
        assert_eq!(raw, vec![1, 4, 8, 31, 0]);
    }

    #[test]
    fn a_domain_holds_nineteen_real_values_and_bottom() {
        let mut store = Domains::new(1, 20);
        for index in 0..19u32 {
            store.insert(A, ValId::real(index));
        }
        store.insert(A, ValId::BOTTOM);
        let domain = store.get(A);

        assert_eq!(domain.len(), 20);
        assert!(domain.contains(ValId::BOTTOM));
        assert_eq!(domain.first(), Some(ValId::real(0)));
        assert_eq!(domain.iter().last(), Some(ValId::BOTTOM));
        assert_eq!(domain.iter().count(), 20);
        assert_eq!(domain.iter().len(), 20);
    }

    #[test]
    fn a_domain_is_as_wide_as_the_network_asks_for() {
        // The point of the representation: a variable offered two hundred
        // targets is an ordinary variable, and nothing about the word size
        // shows through.
        let count = 200u32;
        let mut store = Domains::new(1, count + 1);
        for index in 0..count {
            store.insert(A, ValId::real(index));
        }
        store.insert(A, ValId::BOTTOM);
        let domain = store.get(A);

        assert_eq!(store.words(), 4);
        assert_eq!(domain.len(), count as usize + 1);
        assert_eq!(domain.first(), Some(ValId::real(0)));
        assert_eq!(domain.iter().last(), Some(ValId::BOTTOM));
        assert_eq!(
            domain.iter().nth(199),
            Some(ValId::real(199)),
            "the last real value comes before `⊥`, three words up"
        );
    }

    #[test]
    fn removing_values_leaves_first_and_only_consistent() {
        let mut store = domain_of(64, &[ValId::real(2), ValId::real(5), ValId::BOTTOM]);
        assert_eq!(store.get(A).only(), None);
        assert_eq!(store.get(A).first(), Some(ValId::real(2)));

        store.remove(A, ValId::real(2));
        store.remove(A, ValId::BOTTOM);
        assert_eq!(store.get(A).only(), Some(ValId::real(5)));
        assert_eq!(store.get(A).first(), Some(ValId::real(5)));

        store.remove(A, ValId::real(5));
        assert!(store.get(A).is_empty());
        assert_eq!(store.get(A).first(), None);
        assert_eq!(store.get(A).only(), None);
        assert_eq!(store.get(A).iter().next(), None);
    }

    #[test]
    fn only_reports_bottom_alone_across_word_boundaries() {
        // `⊥` is bit zero, so a store whose only value is `⊥` has a set bit in
        // the first word and nothing anywhere else.
        let store = domain_of(200, &[ValId::BOTTOM]);
        assert_eq!(store.get(A).only(), Some(ValId::BOTTOM));
        assert_eq!(store.get(A).first(), Some(ValId::BOTTOM));

        let far = domain_of(200, &[ValId::real(150)]);
        assert_eq!(far.get(A).only(), Some(ValId::real(150)));
    }

    #[test]
    fn assigning_a_value_the_domain_lacks_empties_it() {
        let mut store = domain_of(200, &[ValId::real(3), ValId::real(150), ValId::BOTTOM]);
        store.assign(A, ValId::real(150));
        assert_eq!(store.get(A).only(), Some(ValId::real(150)));

        let mut absent = domain_of(200, &[ValId::real(3)]);
        absent.assign(A, ValId::real(150));
        assert!(absent.get(A).is_empty());
    }

    // -- Domains of a built network ----------------------------------------

    #[test]
    fn bottom_is_in_every_domain_and_ordered_last() {
        let cfn = CfnBuilder::new(
            vec![
                (name("a"), vec![name("y"), name("x")]),
                (name("b"), Vec::new()),
            ],
            DEFAULT_WEIGHTS,
        )
        .unwrap()
        .build();

        for var in cfn.variable_ids() {
            let domain = cfn.domain(var).unwrap();
            assert!(domain.contains(ValId::BOTTOM), "{var:?} has no bottom");
            assert_eq!(domain.iter().last(), Some(ValId::BOTTOM));
            let seen: Vec<ValId> = domain.iter().collect();
            assert!(seen.windows(2).all(|pair| pair[0] < pair[1]));
        }

        // A variable offered nothing still has a domain, and it is `{⊥}`.
        let empty = cfn.domain(B).unwrap();
        assert_eq!(empty.len(), 1);
        assert_eq!(empty.only(), Some(ValId::BOTTOM));
    }

    #[test]
    fn values_are_sorted_by_target_name_and_deduplicated() {
        let cfn = CfnBuilder::new(
            vec![(
                name("a"),
                vec![name("y"), name("x"), name("y"), name("record.a")],
            )],
            DEFAULT_WEIGHTS,
        )
        .unwrap()
        .build();

        let variable = cfn.variable(A).unwrap();
        let names: Vec<&str> = variable.values().iter().map(Name::as_str).collect();
        assert_eq!(names, vec!["record.a", "x", "y"]);
        assert_eq!(variable.value_id(&name("x")), Some(ValId::real(1)));
        assert_eq!(variable.value_id(&name("absent")), None);
        assert_eq!(variable.value_name(ValId::real(0)), Some(&name("record.a")));
        assert_eq!(variable.value_name(ValId::BOTTOM), None);
        assert_eq!(variable.slot(ValId::BOTTOM), Some(3));
        assert_eq!(variable.slot(ValId::real(3)), None);
    }

    #[test]
    fn two_variables_for_one_source_vertex_are_rejected() {
        // Accepting this would give `a` two simultaneous images, charge its
        // unary cost twice, and size the coverage radix for three source
        // vertices where there are two.
        let error = CfnBuilder::new(
            vec![
                (name("a"), vec![name("x")]),
                (name("b"), vec![name("x")]),
                (name("a"), vec![name("x")]),
            ],
            DEFAULT_WEIGHTS,
        )
        .unwrap_err();
        assert_eq!(
            error,
            CfnError::DuplicateVariable {
                variable: name("a"),
                first: 0,
                second: 2,
            }
        );
    }

    #[test]
    fn a_wide_domain_is_built_rather_than_refused() {
        // Two hundred kind-compatible targets is an ordinary record type and an
        // ordinary line-per-vertex parse. No word size has anything to say
        // about it.
        let targets: Vec<Name> = (0..200u32)
            .map(|index| name(&format!("t{index:03}")))
            .collect();
        let cfn = CfnBuilder::new(vec![(name("a"), targets)], DEFAULT_WEIGHTS)
            .unwrap()
            .build();
        let domain = cfn.domain(A).unwrap();
        assert_eq!(domain.len(), 201);
        assert_eq!(domain.iter().last(), Some(ValId::BOTTOM));
        assert_eq!(cfn.variable(A).unwrap().slots(), 201);
    }

    #[test]
    fn a_network_over_the_memory_budget_is_refused_with_its_measurement() {
        // Nothing about the domain is refused: what is refused is the bytes the
        // cost tables come to, and the refusal reports them.
        let targets: Vec<Name> = (0..200u32)
            .map(|index| name(&format!("t{index:03}")))
            .collect();
        let spec: Vec<(Name, Vec<Name>)> = (0..200u32)
            .map(|index| (name(&format!("s{index:03}")), targets.clone()))
            .collect();

        let error = CfnBuilder::with_mem_bytes(spec.clone(), DEFAULT_WEIGHTS, 1024).unwrap_err();
        let CfnError::OverMemoryBudget {
            entries,
            bytes,
            budget,
        } = error
        else {
            panic!("a network over the budget must report the budget: {error:?}");
        };
        assert_eq!(entries, 200 * 201);
        assert_eq!(bytes, entries * size_of::<Cost>() as u64);
        assert_eq!(budget, 1024);

        // The same network is ordinary against an ordinary budget.
        assert!(CfnBuilder::new(spec, DEFAULT_WEIGHTS).is_ok());
    }

    #[test]
    fn a_cost_function_that_breaks_the_budget_is_refused() {
        let targets: Vec<Name> = (0..40u32)
            .map(|index| name(&format!("t{index:02}")))
            .collect();
        let spec = vec![(name("a"), targets.clone()), (name("b"), targets)];
        // The unary tables are 82 entries; one binary table is 41 x 41.
        let mut builder = CfnBuilder::with_mem_bytes(spec, DEFAULT_WEIGHTS, 82 * 8 + 8).unwrap();
        let error = builder
            .add_function(&[A, B], vec![Cost::BOT; 41 * 41])
            .unwrap_err();
        assert!(
            matches!(error, CfnError::OverMemoryBudget { entries, .. } if entries == 82 + 41 * 41),
            "{error:?}"
        );
    }

    // -- Scope uniqueness ---------------------------------------------------

    #[test]
    fn duplicate_scopes_are_merged_by_combine_at_construction() {
        let scope = [A, B];

        let mut twice = two_variable_builder();
        twice
            .add_function(
                &scope,
                vec![cost(1), cost(2), cost(3), cost(4), cost(5), cost(6)],
            )
            .unwrap();
        twice
            .add_function(
                &scope,
                vec![cost(10), cost(20), cost(30), cost(40), cost(50), cost(60)],
            )
            .unwrap();
        let twice = twice.build();

        let mut once = two_variable_builder();
        once.add_function(
            &scope,
            vec![cost(11), cost(22), cost(33), cost(44), cost(55), cost(66)],
        )
        .unwrap();
        let once = once.build();

        assert_eq!(twice.n_functions(), 1, "the two adds must have merged");
        assert_eq!(once.n_functions(), 1);
        assert_eq!(twice.functions()[0].scope(), scope.as_slice());
        assert_eq!(twice.functions()[0].arity(), 2);

        for candidate in all_assignments(&once) {
            assert_eq!(
                twice.evaluate(&candidate),
                once.evaluate(&candidate),
                "merged and pre-summed networks disagree on {candidate:?}"
            );
        }
    }

    #[test]
    fn a_one_variable_scope_folds_into_the_unary_table() {
        let mut builder = two_variable_builder();
        builder.add_unary(A, ValId::real(0), cost(5)).unwrap();
        builder
            .add_function(&[A], vec![cost(1), cost(2), cost(3)])
            .unwrap();
        let cfn = builder.build();

        assert_eq!(cfn.n_functions(), 0);
        assert_eq!(cfn.unary(A).unwrap(), &[cost(6), cost(2), cost(3)]);
    }

    #[test]
    fn an_empty_scope_folds_into_the_constant() {
        let mut builder = two_variable_builder();
        builder.add_empty(cost(4));
        builder.add_function(&[], vec![cost(9)]).unwrap();
        assert_eq!(builder.c_empty(), cost(13));
    }

    #[test]
    fn a_repeated_or_unsorted_scope_is_rejected() {
        let mut builder = two_variable_builder();
        let repeated = builder.add_function(&[A, A], vec![cost(0); 9]).unwrap_err();
        assert!(
            matches!(repeated, CfnError::ScopeNotAscending { .. }),
            "{repeated:?}"
        );
        let unsorted = builder.add_function(&[B, A], vec![cost(0); 6]).unwrap_err();
        assert!(
            matches!(unsorted, CfnError::ScopeNotAscending { .. }),
            "{unsorted:?}"
        );
        let unknown = builder
            .add_function(&[A, VarId::new(9)], vec![cost(0); 6])
            .unwrap_err();
        assert!(
            matches!(unknown, CfnError::UnknownVariable { .. }),
            "{unknown:?}"
        );
        let wrong_size = builder.add_function(&[A, B], vec![cost(0); 5]).unwrap_err();
        assert!(
            matches!(
                wrong_size,
                CfnError::TableSizeMismatch {
                    expected: 6,
                    found: 5,
                    ..
                }
            ),
            "{wrong_size:?}"
        );
    }

    // -- Evaluation ---------------------------------------------------------

    #[test]
    fn evaluate_matches_a_hand_computed_sum() {
        let mut builder = two_variable_builder();
        builder.add_empty(cost(7));
        builder
            .add_unary_table(A, &[cost(1), cost(2), cost(3)])
            .unwrap();
        builder.add_unary_table(B, &[cost(4), cost(5)]).unwrap();
        // Indexed `slot_a · 2 + slot_b`, so the diagonal reads 100, 202, 304.
        builder
            .add_function(
                &[A, B],
                vec![
                    cost(100),
                    cost(101),
                    cost(201),
                    cost(202),
                    cost(303),
                    cost(304),
                ],
            )
            .unwrap();
        let cfn = builder.build();

        // (x, p): 7 + 1 + 4 + table[0 · 2 + 0] = 7 + 1 + 4 + 100 = 112.
        assert_eq!(
            cfn.evaluate(&assignment(&[ValId::real(0), ValId::real(0)])),
            cost(112)
        );
        // (y, ⊥): 7 + 2 + 5 + table[1 · 2 + 1] = 7 + 2 + 5 + 202 = 216.
        assert_eq!(
            cfn.evaluate(&assignment(&[ValId::real(1), ValId::BOTTOM])),
            cost(216)
        );
        // (⊥, p): 7 + 3 + 4 + table[2 · 2 + 0] = 7 + 3 + 4 + 303 = 317.
        assert_eq!(
            cfn.evaluate(&assignment(&[ValId::BOTTOM, ValId::real(0)])),
            cost(317)
        );
        // (⊥, ⊥): 7 + 3 + 5 + table[2 · 2 + 1] = 7 + 3 + 5 + 304 = 319.
        assert_eq!(
            cfn.evaluate(&assignment(&[ValId::BOTTOM, ValId::BOTTOM])),
            cost(319)
        );

        // The public index computation agrees with the hand arithmetic.
        assert_eq!(
            cfn.table_index(&[A, B], &[ValId::BOTTOM, ValId::real(0)]),
            Some(4)
        );
        assert_eq!(cfn.table_length(&[A, B]), Some(6));
    }

    #[test]
    fn the_row_major_index_is_right_at_arity_three() {
        // `evaluate` reads a table through the private `function_offset` and a
        // caller reads it through the public `table_index`. Both are written out
        // separately, and every network the schema builder produces is at most
        // binary, so nothing else in the crate would notice the two disagreeing
        // above arity two. An oracle scoring with `evaluate` would not notice
        // either: a wrong index moves the oracle's answer and a solver's table
        // lookups by the same amount.
        //
        // Slot counts 4, 2 and 3, so the table is 24 entries indexed
        // `(s0 · 2 + s1) · 3 + s2`. Each entry encodes its own slot triple, so a
        // reading that lands on the wrong entry reads the wrong triple.
        let mut builder = CfnBuilder::new(
            vec![
                (name("a"), vec![name("x"), name("y"), name("z")]),
                (name("b"), vec![name("p")]),
                (name("c"), vec![name("q"), name("r")]),
            ],
            DEFAULT_WEIGHTS,
        )
        .unwrap();
        let c = VarId::new(2);
        let scope = [A, B, c];
        let slots = [4usize, 2, 3];

        let mut table = vec![Cost::BOT; 24];
        for (s0, s1, s2) in triples(slots) {
            table[(s0 * slots[1] + s1) * slots[2] + s2] =
                cost(u64::try_from(s0 * 100 + s1 * 10 + s2).unwrap());
        }
        builder.add_function(&scope, table).unwrap();
        let cfn = builder.build();

        assert_eq!(cfn.table_length(&scope), Some(24));
        for (s0, s1, s2) in triples(slots) {
            let values = [
                value_at(&cfn, A, s0),
                value_at(&cfn, B, s1),
                value_at(&cfn, c, s2),
            ];
            let expected = (s0 * slots[1] + s1) * slots[2] + s2;
            assert_eq!(
                cfn.table_index(&scope, &values),
                Some(expected),
                "table_index at {s0},{s1},{s2}"
            );
            // `evaluate` reaches the same entry by its own route.
            assert_eq!(
                cfn.evaluate(&assignment(&values)),
                cost(u64::try_from(s0 * 100 + s1 * 10 + s2).unwrap()),
                "evaluate at {s0},{s1},{s2}"
            );
        }
    }

    /// Every slot triple of a three-variable scope, in row-major order.
    fn triples(slots: [usize; 3]) -> impl Iterator<Item = (usize, usize, usize)> {
        (0..slots[0]).flat_map(move |s0| {
            (0..slots[1]).flat_map(move |s1| (0..slots[2]).map(move |s2| (s0, s1, s2)))
        })
    }

    /// The value occupying one slot of one variable, `⊥` being the last.
    fn value_at(cfn: &Cfn, var: VarId, slot: usize) -> ValId {
        let variable = cfn.variable(var).unwrap();
        let value = if slot + 1 == variable.slots() {
            ValId::BOTTOM
        } else {
            ValId::real(u32::try_from(slot).unwrap())
        };
        assert_eq!(variable.slot(value), Some(slot));
        value
    }

    #[test]
    fn a_value_outside_its_domain_evaluates_to_top() {
        let cfn = two_variable_builder().build();
        // `b` has one real value, so index two is not one of its values.
        let outside = assignment(&[ValId::real(0), ValId::real(2)]);
        assert_eq!(cfn.evaluate(&outside), Cost::TOP_SENTINEL);
        assert_eq!(cfn.quality_of(&outside), 0.0);
    }

    #[test]
    #[should_panic(expected = "one value to every variable")]
    fn a_partial_assignment_is_not_evaluated() {
        let cfn = two_variable_builder().build();
        let _ = cfn.evaluate(&assignment(&[ValId::BOTTOM]));
    }

    #[test]
    fn quality_reads_the_high_bits_and_ignores_the_drop_count() {
        let mut builder = two_variable_builder();
        let radix = builder.radix();
        assert_eq!(radix, 4, "two variables give a radix of four");
        // A quarter of the scale of quality cost, plus one dropped vertex.
        builder
            .add_unary(A, ValId::BOTTOM, Cost::packed(COST_SCALE / 4, 1, radix))
            .unwrap();
        let cfn = builder.build();

        let dropped = assignment(&[ValId::BOTTOM, ValId::real(0)]);
        assert_eq!(cfn.quality_of(&dropped), 0.75);
        assert_eq!(
            cfn.evaluate(&dropped).drop_part(radix),
            1,
            "the drop count survives the quality reading"
        );

        let kept = assignment(&[ValId::real(0), ValId::real(0)]);
        assert_eq!(cfn.quality_of(&kept), 1.0);
    }

    // -- The constant -------------------------------------------------------

    #[test]
    fn the_constant_never_decreases() {
        let mut builder = two_variable_builder();
        let mut trace = vec![builder.c_empty()];

        builder.add_empty(Cost::BOT);
        trace.push(builder.c_empty());
        builder.add_empty(cost(3));
        trace.push(builder.c_empty());
        builder.add_unary(A, ValId::real(0), cost(11)).unwrap();
        trace.push(builder.c_empty());
        builder.add_function(&[], vec![cost(2)]).unwrap();
        trace.push(builder.c_empty());
        builder.add_function(&[A, B], vec![cost(1); 6]).unwrap();
        trace.push(builder.c_empty());
        builder.add_unary_table(B, &[cost(7), cost(8)]).unwrap();
        trace.push(builder.c_empty());
        builder.add_empty(Cost::TOP_SENTINEL);
        trace.push(builder.c_empty());
        builder.add_empty(cost(1));
        trace.push(builder.c_empty());

        assert!(
            trace.windows(2).all(|pair| pair[0] <= pair[1]),
            "the constant fell somewhere in {trace:?}"
        );
        assert_eq!(*trace.last().unwrap(), Cost::TOP_SENTINEL);

        let cfn = builder.build();
        assert_eq!(cfn.c_empty(), *trace.last().unwrap());
    }

    #[test]
    fn adding_cost_accumulates_rather_than_replaces() {
        let mut builder = two_variable_builder();
        builder.add_unary(A, ValId::real(0), cost(2)).unwrap();
        builder.add_unary(A, ValId::real(0), cost(3)).unwrap();
        builder
            .add_unary_table(A, &[cost(1), cost(0), cost(0)])
            .unwrap();
        let cfn = builder.build();
        assert_eq!(cfn.unary_cost(A, ValId::real(0)), Some(cost(6)));
        assert_eq!(cfn.unary_cost(A, ValId::real(3)), None);
    }

    #[test]
    fn a_network_reports_its_own_shape() {
        let mut builder = two_variable_builder();
        builder.add_function(&[A, B], vec![cost(0); 6]).unwrap();
        let cfn = builder.build();

        assert_eq!(cfn.n_variables(), 2);
        assert_eq!(cfn.n_functions(), 1);
        assert_eq!(cfn.max_domain(), 3);
        assert_eq!(cfn.radix(), 4);
        assert_eq!(cfn.variables().len(), 2);
        assert_eq!(cfn.variable(A).unwrap().name(), &name("a"));
        assert_eq!(cfn.variable(VarId::new(7)), None);
        assert!(cfn.function_for(&[A, B]).is_some());
        assert!(cfn.function_for(&[B]).is_none());
        assert_eq!(cfn.weights(), DEFAULT_WEIGHTS);
        assert_eq!(cfn.variable_ids().collect::<Vec<_>>(), vec![A, B]);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod property {
    use super::*;
    use crate::solve::cost::DEFAULT_WEIGHTS;
    use proptest::prelude::*;

    /// The scopes of arity two or more over three variables.
    const SCOPES: [&[VarId]; 4] = [
        &[VarId::new(0), VarId::new(1)],
        &[VarId::new(0), VarId::new(2)],
        &[VarId::new(1), VarId::new(2)],
        &[VarId::new(0), VarId::new(1), VarId::new(2)],
    ];

    /// One contribution to a network, independent of the order it is applied in.
    #[derive(Clone, Debug)]
    enum Addition {
        Empty(u64),
        Unary { var: usize, slot: usize, cost: u64 },
        Function { scope: usize, table: Vec<u64> },
    }

    /// Three variables, each offered between zero and three targets.
    fn arb_sizes() -> impl Strategy<Value = [usize; 3]> {
        (0usize..=3, 0usize..=3, 0usize..=3).prop_map(<[usize; 3]>::from)
    }

    fn arb_addition(sizes: [usize; 3]) -> impl Strategy<Value = Addition> {
        let slots = [sizes[0] + 1, sizes[1] + 1, sizes[2] + 1];
        prop_oneof![
            1 => (0u64..1_000).prop_map(Addition::Empty),
            3 => (0usize..3, 0usize..4, 0u64..1_000).prop_map(move |(var, slot, cost)| {
                Addition::Unary { var, slot: slot % slots[var], cost }
            }),
            3 => (0usize..SCOPES.len()).prop_flat_map(move |scope| {
                let length: usize = SCOPES[scope].iter().map(|v| slots[v.index()]).product();
                prop::collection::vec(0u64..1_000, length..=length)
                    .prop_map(move |table| Addition::Function { scope, table })
            }),
        ]
    }

    /// A network shape, the contributions to it, and a second order to apply
    /// them in.
    #[allow(clippy::type_complexity)]
    fn arb_network() -> impl Strategy<Value = ([usize; 3], Vec<Addition>, Vec<u64>)> {
        arb_sizes().prop_flat_map(|sizes| {
            prop::collection::vec(arb_addition(sizes), 0..=8).prop_flat_map(move |additions| {
                let count = additions.len();
                (
                    Just(sizes),
                    Just(additions),
                    prop::collection::vec(any::<u64>(), count..=count),
                )
            })
        })
    }

    /// Three variables named `v0`, `v1`, `v2`, offered the targets the shape
    /// asks for.
    fn variables_for(sizes: [usize; 3]) -> Vec<(Name, Vec<Name>)> {
        (0..3)
            .map(|var| {
                let targets = (0..sizes[var])
                    .map(|index| Name::new(format!("t{var}{index}")))
                    .collect();
                (Name::new(format!("v{var}")), targets)
            })
            .collect()
    }

    fn build(sizes: [usize; 3], additions: &[Addition]) -> Cfn {
        let mut builder = CfnBuilder::new(variables_for(sizes), DEFAULT_WEIGHTS).unwrap();
        for addition in additions {
            apply(&mut builder, sizes, addition);
        }
        builder.build()
    }

    /// Apply one contribution to a builder.
    fn apply(builder: &mut CfnBuilder, sizes: [usize; 3], addition: &Addition) {
        match addition {
            Addition::Empty(units) => builder.add_empty(Cost::from_raw(*units)),
            Addition::Unary { var, slot, cost } => {
                let variable = VarId::new(u32::try_from(*var).unwrap());
                let value = if *slot == sizes[*var] {
                    ValId::BOTTOM
                } else {
                    ValId::real(u32::try_from(*slot).unwrap())
                };
                builder
                    .add_unary(variable, value, Cost::from_raw(*cost))
                    .unwrap();
            }
            Addition::Function { scope, table } => {
                let table = table.iter().copied().map(Cost::from_raw).collect();
                builder.add_function(SCOPES[*scope], table).unwrap();
            }
        }
    }

    fn all_assignments(cfn: &Cfn) -> Vec<Assignment> {
        let mut out = vec![Vec::new()];
        for var in cfn.variable_ids() {
            let domain = cfn.domain(var).unwrap();
            let mut next = Vec::new();
            for partial in &out {
                for value in domain {
                    let mut extended = partial.clone();
                    extended.push(value);
                    next.push(extended);
                }
            }
            out = next;
        }
        out.into_iter().map(Assignment::from_values).collect()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Evaluation is a sum, and a sum does not remember the order it was
        /// built in. The order cost functions are added in is exactly the thing
        /// a caller has no control over: a builder walks a `HashMap` of edges,
        /// and two runs can walk it differently.
        #[test]
        fn evaluate_is_invariant_to_the_order_of_additions(
            (sizes, additions, keys) in arb_network()
        ) {
            let mut order: Vec<usize> = (0..additions.len()).collect();
            order.sort_by_key(|index| (keys[*index], *index));
            let shuffled: Vec<Addition> = order
                .iter()
                .map(|index| additions[*index].clone())
                .collect();

            let first = build(sizes, &additions);
            let second = build(sizes, &shuffled);

            prop_assert_eq!(first.n_functions(), second.n_functions());
            for candidate in all_assignments(&first) {
                prop_assert_eq!(
                    first.evaluate(&candidate),
                    second.evaluate(&candidate),
                    "orders disagree on {:?}", candidate
                );
            }
        }

        /// The constant is a lower bound on the objective, so an operation that
        /// lowered it would let a solver prune the subtree holding the optimum.
        /// No sequence of public operations can: the only writer combines, and
        /// combining is a saturating add.
        #[test]
        fn the_constant_never_decreases_under_any_sequence(
            (sizes, additions, _keys) in arb_network()
        ) {
            let mut builder = CfnBuilder::new(variables_for(sizes), DEFAULT_WEIGHTS).unwrap();
            let mut previous = builder.c_empty();
            prop_assert_eq!(previous, Cost::BOT);
            for addition in &additions {
                apply(&mut builder, sizes, addition);
                let current = builder.c_empty();
                prop_assert!(
                    current >= previous,
                    "the constant fell from {:?} to {:?}", previous, current
                );
                previous = current;
            }
            prop_assert_eq!(builder.build().c_empty(), previous);
        }

        /// However many times a scope is contributed to, one function carries
        /// it. This is the invariant stated as a property rather than as a pair
        /// of hand-built networks.
        #[test]
        fn no_two_cost_functions_share_a_scope(
            (sizes, additions, _keys) in arb_network()
        ) {
            let cfn = build(sizes, &additions);
            let mut scopes: Vec<Vec<VarId>> =
                cfn.functions().iter().map(|f| f.scope().to_vec()).collect();
            let total = scopes.len();
            scopes.sort();
            scopes.dedup();
            prop_assert_eq!(scopes.len(), total);
            for function in cfn.functions() {
                prop_assert!(function.arity() >= 2);
                prop_assert_eq!(
                    function.table().len(),
                    cfn.table_length(function.scope()).unwrap()
                );
                // Distinctness is only well defined against one spelling of a
                // scope, so every stored scope must be the ascending one. It
                // also makes the read side total: `function_for` compares
                // slices, and would miss a scope written the other way round.
                prop_assert!(
                    function.scope().windows(2).all(|pair| pair[0] < pair[1]),
                    "scope {:?} is not strictly ascending", function.scope()
                );
                prop_assert_eq!(cfn.function_for(function.scope()), Some(function));
            }
        }
    }
}
