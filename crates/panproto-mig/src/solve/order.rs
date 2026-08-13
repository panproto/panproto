//! Elimination orders, the graph they are read off, and the width they induce.
//!
//! Bucket elimination costs `d^(w+1)` time and `d^w` space, where `w` is the
//! induced width of the order it runs under. The order is therefore the one
//! decision that fixes whether exact inference is affordable, and this module
//! makes it: it builds the primal graph of a network, offers two deterministic
//! orders over it, computes the induced width of an order exactly, takes the
//! narrower of the two, and prices that width against a budget so the
//! dispatcher can decide whether exact inference is affordable at all.
//!
//! # The order convention
//!
//! Every order here is an **elimination sequence**: `order[0]` is eliminated
//! first and `order[n - 1]` last. [`elim::eliminate`](super::elim::eliminate)
//! walks it forwards and [`elim::decode`](super::elim::decode) walks it
//! backwards, so the sequence is also the decode order read in reverse.
//!
//! Dechter's `d = X_1, …, X_n` runs the other way round, with `X_n` eliminated
//! first, so `pos_d(order[p]) = n - p`. The convention here is the one that
//! makes [`reverse_source_id_order`] say what its name says: reversing the
//! ascending source vertex order puts the deepest source vertices first in the
//! elimination sequence, which is what keeps the width at one on a tree, and it
//! leaves decode running in ascending source vertex order, which is the
//! canonical tie-break among equally good assignments.
//!
//! # Why the width is read at runtime
//!
//! The primal graph is built from cost function scopes, not from schema edges.
//! Recursion points, schema spans and hyper-edge signature cliques each
//! constrain a set of source vertices that need not be joined by any edge, so
//! the primal graph can carry edges the schema does not, and its width can
//! exceed the width of the schema graph. Measuring the width on schemas alone
//! would therefore understate it, which is why nothing here reads a stored
//! number and why [`induced_width`] computes Definition 4 exactly rather than
//! reporting a heuristic's running maximum.

use super::cfn::{Cfn, Variable};
use super::cost::Cost;
use super::{SearchBudget, VarId};

/// The number of vertices one word of a [`Bits`] holds.
const WORD_BITS: usize = 64;

// ---------------------------------------------------------------------------
// Bit sets
// ---------------------------------------------------------------------------

/// A set of vertex indices, stored one bit per vertex.
///
/// The graph routines are written against this rather than against a hash set
/// so that nothing in the module iterates a hash map: min-fill has to be
/// identical across processes, and a hash map's iteration order is seeded per
/// process.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Bits {
    words: Vec<u64>,
}

impl Bits {
    /// A set with room for `capacity` vertices and nothing in it.
    fn empty(capacity: usize) -> Self {
        Self {
            words: vec![0; capacity.div_ceil(WORD_BITS)],
        }
    }

    /// Add a vertex. An index past the capacity is not a vertex, so nothing
    /// happens.
    fn insert(&mut self, index: usize) {
        if let Some(word) = self.words.get_mut(index / WORD_BITS) {
            *word |= 1u64 << (index % WORD_BITS);
        }
    }

    /// Remove a vertex.
    fn remove(&mut self, index: usize) {
        if let Some(word) = self.words.get_mut(index / WORD_BITS) {
            *word &= !(1u64 << (index % WORD_BITS));
        }
    }

    /// Whether a vertex is in the set.
    fn contains(&self, index: usize) -> bool {
        self.words
            .get(index / WORD_BITS)
            .is_some_and(|word| word & (1u64 << (index % WORD_BITS)) != 0)
    }

    /// How many vertices the set holds.
    fn len(&self) -> usize {
        self.words
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    /// Add every vertex of another set.
    fn union_with(&mut self, other: &Self) {
        for (word, added) in self.words.iter_mut().zip(&other.words) {
            *word |= *added;
        }
    }

    /// Drop every vertex not in another set.
    fn intersect_with(&mut self, other: &Self) {
        for (index, word) in self.words.iter_mut().enumerate() {
            *word &= other.words.get(index).copied().unwrap_or(0);
        }
    }

    /// How many vertices this set holds that another set does not.
    fn difference_len(&self, other: &Self) -> usize {
        self.words
            .iter()
            .enumerate()
            .map(|(index, word)| {
                let removed = other.words.get(index).copied().unwrap_or(0);
                (word & !removed).count_ones() as usize
            })
            .sum()
    }

    /// The vertices, in ascending index order.
    fn iter(&self) -> BitsIter<'_> {
        BitsIter {
            words: &self.words,
            word: 0,
            current: self.words.first().copied().unwrap_or(0),
        }
    }
}

/// The vertices of a [`Bits`], in ascending index order.
struct BitsIter<'a> {
    words: &'a [u64],
    word: usize,
    current: u64,
}

impl Iterator for BitsIter<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        loop {
            if self.current != 0 {
                let bit = self.current.trailing_zeros() as usize;
                self.current &= self.current - 1;
                return Some(self.word * WORD_BITS + bit);
            }
            self.word += 1;
            self.current = *self.words.get(self.word)?;
        }
    }
}

// ---------------------------------------------------------------------------
// The graph
// ---------------------------------------------------------------------------

/// An undirected graph over the variables of a network.
///
/// Vertices are [`VarId`]s numbered densely from zero, so a graph is fixed by
/// its vertex count and its edge set and carries no names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Graph {
    adjacency: Vec<Bits>,
}

impl Graph {
    /// A graph on `vertices` vertices and no edges.
    #[must_use]
    pub fn new(vertices: usize) -> Self {
        Self {
            adjacency: (0..vertices).map(|_| Bits::empty(vertices)).collect(),
        }
    }

    /// How many vertices the graph has.
    #[inline]
    #[must_use]
    pub fn n_vertices(&self) -> usize {
        self.adjacency.len()
    }

    /// How many edges the graph has.
    #[must_use]
    pub fn n_edges(&self) -> usize {
        self.adjacency.iter().map(Bits::len).sum::<usize>() / 2
    }

    /// Join two vertices.
    ///
    /// A self loop and a vertex outside the range are both no-ops: the width
    /// definitions count neighbours, and neither of those is one.
    pub fn add_edge(&mut self, left: VarId, right: VarId) {
        if left == right || left.index() >= self.adjacency.len() {
            return;
        }
        if right.index() >= self.adjacency.len() {
            return;
        }
        if let Some(row) = self.adjacency.get_mut(left.index()) {
            row.insert(right.index());
        }
        if let Some(row) = self.adjacency.get_mut(right.index()) {
            row.insert(left.index());
        }
    }

    /// Whether two vertices are joined.
    #[must_use]
    pub fn has_edge(&self, left: VarId, right: VarId) -> bool {
        self.adjacency
            .get(left.index())
            .is_some_and(|row| row.contains(right.index()))
    }

    /// How many neighbours a vertex has.
    #[must_use]
    pub fn degree(&self, vertex: VarId) -> usize {
        self.adjacency.get(vertex.index()).map_or(0, Bits::len)
    }

    /// The neighbours of a vertex, in ascending order.
    pub fn neighbours(&self, vertex: VarId) -> impl Iterator<Item = VarId> + '_ {
        self.adjacency
            .get(vertex.index())
            .into_iter()
            .flat_map(Bits::iter)
            .filter_map(|index| u32::try_from(index).ok().map(VarId::new))
    }

    /// The subgraph on `vertices`, renumbered densely in the order given.
    ///
    /// The result's vertex `i` is `vertices[i]`, so a caller maps an answer back
    /// by indexing. Vertices outside the graph and repeats are skipped, which
    /// keeps the renumbering dense whatever it is handed.
    ///
    /// This exists so that a per-component question can be asked without
    /// materialising a per-component network. An order chosen on the induced
    /// subgraph of a *component* is the order the decomposed network would
    /// choose, because a component is closed under adjacency and every
    /// tie-break in [`min_fill_order`] is ascending in the local numbering,
    /// which is order-isomorphic to the numbering it came from when `vertices`
    /// is ascending.
    #[must_use]
    pub fn induced(&self, vertices: &[VarId]) -> Self {
        let mut slot: Vec<Option<usize>> = vec![None; self.adjacency.len()];
        let mut kept: Vec<VarId> = Vec::with_capacity(vertices.len());
        for vertex in vertices {
            let Some(entry) = slot.get_mut(vertex.index()) else {
                continue;
            };
            if entry.is_some() {
                continue;
            }
            *entry = Some(kept.len());
            kept.push(*vertex);
        }

        let mut out = Self::new(kept.len());
        for (local, vertex) in kept.iter().enumerate() {
            let Ok(raw) = u32::try_from(local) else {
                continue;
            };
            for neighbour in self.neighbours(*vertex) {
                let Some(Some(other)) = slot.get(neighbour.index()).copied() else {
                    continue;
                };
                let Ok(other_raw) = u32::try_from(other) else {
                    continue;
                };
                out.add_edge(VarId::new(raw), VarId::new(other_raw));
            }
        }
        out
    }

    /// The connected components, each in ascending vertex order, ordered by
    /// their smallest vertex.
    ///
    /// A vertex with no neighbours is a component of its own, so every vertex
    /// appears in exactly one component and the components partition the graph.
    /// This is what lets a network be solved component by component: the
    /// optimum is the `⊕`-sum of the components' optima and the solution count
    /// is their product.
    #[must_use]
    pub fn components(&self) -> Vec<Vec<VarId>> {
        let mut seen = Bits::empty(self.adjacency.len());
        let mut out = Vec::new();
        for root in 0..self.adjacency.len() {
            if seen.contains(root) {
                continue;
            }
            let mut component = Vec::new();
            let mut frontier = vec![root];
            seen.insert(root);
            while let Some(vertex) = frontier.pop() {
                component.push(vertex);
                let Some(row) = self.adjacency.get(vertex) else {
                    continue;
                };
                for next in row.iter() {
                    if !seen.contains(next) {
                        seen.insert(next);
                        frontier.push(next);
                    }
                }
            }
            component.sort_unstable();
            out.push(
                component
                    .into_iter()
                    .filter_map(|index| u32::try_from(index).ok().map(VarId::new))
                    .collect(),
            );
        }
        out
    }
}

/// The primal graph of a network: a vertex per variable, and an edge between
/// any two variables sharing a cost function scope.
///
/// Each scope becomes a clique, which is what makes the message scope at a
/// bucket the set of earlier neighbours in the induced graph and therefore
/// makes [`induced_width`] the exact maximum message arity.
///
/// Unary cost adds no edge: a scope of one variable is a clique of one. The
/// edges that do appear come from cost functions of arity two and above, which
/// on a network built by [`build_cfn`](super::build::build_cfn) are the edge
/// quality terms together with the apex hard constraints. The second group is
/// the reason this graph is built rather than reused from the schema: a
/// recursion point, a schema span and a hyper-edge signature each constrain a
/// set of source vertices with no schema edge joining them.
///
/// # Examples
///
/// ```
/// use panproto_gat::Name;
/// use panproto_mig::solve::cfn::CfnBuilder;
/// use panproto_mig::solve::order::primal_graph;
/// use panproto_mig::{Cost, DEFAULT_WEIGHTS, VarId};
///
/// let mut builder = CfnBuilder::new(
///     vec![
///         (Name::new("a"), vec![Name::new("x")]),
///         (Name::new("b"), vec![Name::new("x")]),
///         (Name::new("c"), vec![Name::new("x")]),
///     ],
///     DEFAULT_WEIGHTS,
/// )?;
/// // One ternary scope is one triangle, not one path.
/// builder.add_function(
///     &[VarId::new(0), VarId::new(1), VarId::new(2)],
///     vec![Cost::BOT; 8],
/// )?;
/// let graph = primal_graph(&builder.build());
///
/// assert_eq!(graph.n_edges(), 3);
/// assert!(graph.has_edge(VarId::new(0), VarId::new(2)));
/// # Ok::<(), panproto_mig::solve::cfn::CfnError>(())
/// ```
#[must_use]
pub fn primal_graph(cfn: &Cfn) -> Graph {
    let mut graph = Graph::new(cfn.n_variables());
    for function in cfn.functions() {
        let scope = function.scope();
        for (offset, left) in scope.iter().enumerate() {
            for right in &scope[offset + 1..] {
                graph.add_edge(*left, *right);
            }
        }
    }
    graph
}

// ---------------------------------------------------------------------------
// Induced width
// ---------------------------------------------------------------------------

/// The induced width of a graph under an elimination sequence, computed
/// exactly.
///
/// Dechter's Definition 4: process the vertices in elimination order, count the
/// neighbours of each that have not yet been eliminated, join those neighbours
/// pairwise, and take the maximum count. The joining is what makes this the
/// *induced* width rather than the width, and it is why the number cannot be
/// read off the input graph alone.
///
/// The result is exactly the largest message arity
/// [`eliminate`](super::elim::eliminate) will produce under the same order, so
/// `d^width` entries and `d^(width + 1)` operations are the allocation and the
/// work that follow from it.
///
/// # Panics
///
/// If `order` is not a permutation of the graph's vertices. A width computed
/// against a partial order would be an underestimate, and an underestimate is
/// what makes a solver allocate a table it cannot fill.
#[must_use]
pub fn induced_width(graph: &Graph, order: &[VarId]) -> usize {
    induced_width_observed(graph, order, &[])
}

/// The adjusted induced width: the induced width with a set of variables
/// treated as observed.
///
/// An observed variable is one already fixed to a value, so its bucket slices
/// its functions rather than joining them. Dechter's Theorem 5: no new arcs are
/// added for an observed variable, and its own count does not enter the
/// maximum. On a network with instantiated variables that is frequently the
/// difference between a width that fits the budget and one that does not, and
/// it costs one bit test per vertex.
///
/// An observed variable leaves the live set before the sweep starts rather than
/// when the order reaches it. Dropping it only at its own position would leave
/// it counted as a live neighbour of every bucket processed earlier, which
/// over-states the width of exactly the orders that eliminate the fixed
/// variables late, and those are the orders the adjustment exists to rescue. A
/// fixed variable is not a dimension of any message, wherever it sits in the
/// sequence.
///
/// The width alone does not size an allocation here: slicing an observed
/// variable out also removes its domain from the product, so a caller pairing
/// this with [`elimination_cost`] gets an upper bound rather than the exact
/// figure, since that helper reads one domain size for the whole network.
///
/// # Panics
///
/// If `order` is not a permutation of the graph's vertices.
#[must_use]
pub fn induced_width_observed(graph: &Graph, order: &[VarId], observed: &[VarId]) -> usize {
    let count = graph.n_vertices();
    assert!(
        is_permutation(order, count),
        "an induced width must be computed against a permutation of the vertices"
    );

    let mut adjacency = graph.adjacency.clone();
    let mut fixed = Bits::empty(count);
    for vertex in observed {
        fixed.insert(vertex.index());
    }
    let mut alive = Bits::empty(count);
    for index in 0..count {
        if !fixed.contains(index) {
            alive.insert(index);
        }
    }

    let mut width = 0usize;
    for vertex in order {
        let index = vertex.index();
        alive.remove(index);
        if fixed.contains(index) {
            continue;
        }
        let Some(row) = adjacency.get(index) else {
            continue;
        };
        let mut later = row.clone();
        later.intersect_with(&alive);
        width = width.max(later.len());
        for neighbour in later.iter() {
            let Some(other) = adjacency.get_mut(neighbour) else {
                continue;
            };
            other.union_with(&later);
            other.remove(neighbour);
        }
    }
    width
}

/// Whether a sequence lists every vertex below `count` exactly once.
///
/// The precondition of every routine here that takes an order, and of
/// [`elim::eliminate`](super::elim::eliminate) as well, which is why it is
/// shared rather than restated.
pub(crate) fn is_permutation(order: &[VarId], count: usize) -> bool {
    if order.len() != count {
        return false;
    }
    let mut seen = Bits::empty(count);
    for vertex in order {
        if vertex.index() >= count || seen.contains(vertex.index()) {
            return false;
        }
        seen.insert(vertex.index());
    }
    true
}

// ---------------------------------------------------------------------------
// The two orders
// ---------------------------------------------------------------------------

/// The reverse of ascending source vertex name order.
///
/// Variables are numbered in ascending source vertex name order, so this is the
/// descending variable order, read off the names rather than off the numbering
/// so that it says what it says even for a network whose variables were offered
/// out of order.
///
/// Two properties make it the first candidate. First, on the dotted path names
/// the parsers produce (`root`, `root.a`, `root.a.b`) descending name order is
/// close to a leaves-first order of a tree, and eliminating leaves first is
/// what holds the width at one on a tree. Second, eliminating in descending
/// name order leaves decode running in *ascending* name order, so the
/// lexicographic tie-break among equally good assignments is the natural one on
/// source vertex names rather than one relative to a computed order.
#[must_use]
pub fn reverse_source_id_order(cfn: &Cfn) -> Vec<VarId> {
    let mut order: Vec<VarId> = cfn.variable_ids().collect();
    order.sort_unstable_by(|left, right| {
        let left_name = cfn.variable(*left).map(Variable::name);
        let right_name = cfn.variable(*right).map(Variable::name);
        right_name
            .map(panproto_gat::Name::as_str)
            .cmp(&left_name.map(panproto_gat::Name::as_str))
            .then(right.cmp(left))
    });
    order
}

/// Kjaerulff min-fill: repeatedly eliminate the vertex whose elimination adds
/// the fewest edges.
///
/// Ties are broken by `(fill count, degree, variable id)`, all three ascending,
/// so the order is a function of the graph alone. There are no randomised
/// restarts: a restart searches for a better order, and what this needs instead
/// is that two runs over one schema pair agree, since the tie-break among
/// equally good assignments is stated relative to the order actually used.
///
/// The fill counts are repaired incrementally after each elimination. Only the
/// vertices within two steps of the eliminated one can change, because a fill
/// count reads the neighbourhood of a vertex and the elimination touches only
/// the neighbourhood of the vertex it removes.
#[must_use]
pub fn min_fill_order(graph: &Graph) -> Vec<VarId> {
    let count = graph.n_vertices();
    let mut adjacency = graph.adjacency.clone();
    let mut alive = Bits::empty(count);
    for index in 0..count {
        alive.insert(index);
    }
    let mut fill: Vec<usize> = (0..count).map(|v| fill_count(&adjacency, v)).collect();

    let mut order = Vec::with_capacity(count);
    for _ in 0..count {
        let Some(chosen) = pick_min_fill(&alive, &fill, &adjacency) else {
            break;
        };
        order.push(VarId::new(u32::try_from(chosen).unwrap_or(u32::MAX)));

        let mut neighbourhood = adjacency
            .get(chosen)
            .cloned()
            .unwrap_or_else(|| Bits::empty(count));
        neighbourhood.intersect_with(&alive);
        neighbourhood.remove(chosen);

        // The fill edges: every missing pair inside the neighbourhood.
        for member in neighbourhood.iter() {
            if let Some(row) = adjacency.get_mut(member) {
                row.union_with(&neighbourhood);
                row.remove(member);
            }
        }

        // Detach the eliminated vertex, so no later step counts it.
        alive.remove(chosen);
        for member in neighbourhood.iter() {
            if let Some(row) = adjacency.get_mut(member) {
                row.remove(chosen);
            }
        }
        if let Some(row) = adjacency.get_mut(chosen) {
            *row = Bits::empty(count);
        }

        // Repair, over the neighbourhood and its own neighbourhoods.
        let mut touched = neighbourhood.clone();
        for member in neighbourhood.iter() {
            if let Some(row) = adjacency.get(member) {
                touched.union_with(row);
            }
        }
        touched.intersect_with(&alive);
        for member in touched.iter() {
            if let Some(slot) = fill.get_mut(member) {
                *slot = fill_count(&adjacency, member);
            }
        }
    }
    order
}

/// The vertex with the smallest `(fill count, degree, index)`, among the ones
/// still in the graph.
fn pick_min_fill(alive: &Bits, fill: &[usize], adjacency: &[Bits]) -> Option<usize> {
    let mut best: Option<(usize, usize, usize)> = None;
    for vertex in alive.iter() {
        let key = (
            fill.get(vertex).copied().unwrap_or(0),
            adjacency.get(vertex).map_or(0, Bits::len),
            vertex,
        );
        if best.is_none_or(|current| key < current) {
            best = Some(key);
        }
    }
    best.map(|(_, _, vertex)| vertex)
}

/// How many edges eliminating a vertex would add.
///
/// The number of unjoined pairs inside its neighbourhood, counted as a sum of
/// per-neighbour differences and halved, since each pair is seen from both
/// ends.
fn fill_count(adjacency: &[Bits], vertex: usize) -> usize {
    let Some(row) = adjacency.get(vertex) else {
        return 0;
    };
    let mut missing = 0usize;
    for neighbour in row.iter() {
        let Some(other) = adjacency.get(neighbour) else {
            continue;
        };
        // Its own slot and the eliminated vertex's slot are never fill edges.
        let mut known = other.clone();
        known.insert(neighbour);
        known.insert(vertex);
        missing += row.difference_len(&known);
    }
    missing / 2
}

// ---------------------------------------------------------------------------
// The budget, and the choice
// ---------------------------------------------------------------------------

/// What exact inference would spend at a given width.
///
/// Both numbers saturate rather than wrap, so a width past anything affordable
/// reports `u64::MAX` and reads as "over budget" rather than wrapping to a
/// small number that would read as affordable.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct EliminationCost {
    /// Cost table entries the messages would occupy, `n · d^w`.
    pub entries: u64,
    /// Combine operations the elimination would perform,
    /// `(r + n) · d^(w + 1)`.
    pub operations: u64,
}

/// What exact inference over this network would spend at a given width.
///
/// The two exponents differ by one and are budgeted separately: a message
/// spans `w` variables and a bucket's loop nest spans `w + 1`.
#[must_use]
pub fn elimination_cost(cfn: &Cfn, width: usize) -> EliminationCost {
    let domain = u64::try_from(cfn.max_domain()).unwrap_or(u64::MAX);
    let variables = u64::try_from(cfn.n_variables()).unwrap_or(u64::MAX);
    let functions = u64::try_from(cfn.n_functions()).unwrap_or(u64::MAX);
    let exponent = u32::try_from(width).unwrap_or(u32::MAX);
    EliminationCost {
        entries: variables.saturating_mul(saturating_pow(domain, exponent)),
        operations: functions
            .saturating_add(variables)
            .saturating_mul(saturating_pow(domain, exponent.saturating_add(1))),
    }
}

/// Whether exact inference at a given width fits a budget.
///
/// Both ceilings must hold: the message tables must fit `mem_bytes` at
/// [`Cost`]'s width, and the loop nest must fit `op_budget`.
#[must_use]
pub fn fits_budget(cfn: &Cfn, width: usize, budget: &SearchBudget) -> bool {
    let cost = elimination_cost(cfn, width);
    let cell = u64::try_from(size_of::<Cost>()).unwrap_or(u64::MAX);
    let memory = u64::try_from(budget.mem_bytes).unwrap_or(u64::MAX);
    cost.entries.saturating_mul(cell) <= memory && cost.operations <= budget.op_budget
}

/// `base^exponent`, saturating instead of overflowing.
fn saturating_pow(base: u64, exponent: u32) -> u64 {
    let mut total = 1u64;
    for _ in 0..exponent {
        total = total.saturating_mul(base);
        if total == u64::MAX {
            break;
        }
    }
    total
}

/// The elimination order a search over this network will use, and its exact
/// induced width.
///
/// Both candidates are measured and the smaller width wins.
/// [`reverse_source_id_order`] keeps the tie, because it is the order whose
/// decode pass runs in ascending source vertex order and so gives the canonical
/// tie-break; on a tree, where both are width one, that is every time.
///
/// # Why both are measured rather than the first that fits
///
/// Fitting the budget is not the same as being worth using. At `d = 2` a
/// width-18 order allocates about forty megabytes, comfortably inside the
/// default ceiling, while [`min_fill_order`] on the same graph can be width
/// one. Taking the first candidate that fits would spend four orders of
/// magnitude more memory and time for an identical answer. That is only
/// invisible while every network's high-degree vertex happens to be the
/// alphabetically smallest name, which holds for dotted paths rooted at a
/// common prefix and fails for any hub that is not a name prefix: an apex hard
/// constraint over a recursion point, a schema span, a hyper-edge signature
/// clique. Those are exactly the cases [`primal_graph`] is built for rather
/// than read off the schema's edges. Measuring the second candidate costs one
/// [`min_fill_order`] pass over a graph that already exists.
///
/// # Examples
///
/// ```
/// use panproto_gat::Name;
/// use panproto_mig::solve::cfn::CfnBuilder;
/// use panproto_mig::solve::order::choose_order;
/// use panproto_mig::{Cost, DEFAULT_WEIGHTS, SearchBudget, VarId};
///
/// let mut builder = CfnBuilder::new(
///     vec![
///         (Name::new("root"), vec![Name::new("x")]),
///         (Name::new("root.a"), vec![Name::new("x")]),
///         (Name::new("root.a.b"), vec![Name::new("x")]),
///     ],
///     DEFAULT_WEIGHTS,
/// )?;
/// builder.add_function(&[VarId::new(0), VarId::new(1)], vec![Cost::BOT; 4])?;
/// builder.add_function(&[VarId::new(1), VarId::new(2)], vec![Cost::BOT; 4])?;
///
/// let (order, width) = choose_order(&builder.build());
///
/// // A path is eliminated leaves first, and a path has induced width one.
/// assert_eq!(order, vec![VarId::new(2), VarId::new(1), VarId::new(0)]);
/// assert_eq!(width, 1);
/// # Ok::<(), panproto_mig::solve::cfn::CfnError>(())
/// ```
#[must_use]
pub fn choose_order(cfn: &Cfn) -> (Vec<VarId>, usize) {
    let graph = primal_graph(cfn);
    let reverse = reverse_source_id_order(cfn);
    let reverse_width = induced_width(&graph, &reverse);

    let fill = min_fill_order(&graph);
    let fill_width = induced_width(&graph, &fill);
    if fill_width < reverse_width {
        (fill, fill_width)
    } else {
        (reverse, reverse_width)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::solve::cfn::CfnBuilder;
    use crate::solve::cost::DEFAULT_WEIGHTS;
    use panproto_gat::Name;

    fn var(index: u32) -> VarId {
        VarId::new(index)
    }

    /// A graph on `n` vertices with the listed edges.
    fn graph_of(vertices: usize, edges: &[(u32, u32)]) -> Graph {
        let mut graph = Graph::new(vertices);
        for (left, right) in edges {
            graph.add_edge(var(*left), var(*right));
        }
        graph
    }

    fn order_of(indices: &[u32]) -> Vec<VarId> {
        indices.iter().copied().map(var).collect()
    }

    /// A network of `n` variables, each over one target, with the listed
    /// scopes carrying zero cost.
    fn network(names: &[&str], scopes: &[&[u32]]) -> Cfn {
        let spec = names
            .iter()
            .map(|name| (Name::new(*name), vec![Name::new("x")]))
            .collect();
        let mut builder = CfnBuilder::new(spec, DEFAULT_WEIGHTS).unwrap();
        for scope in scopes {
            let scope: Vec<VarId> = scope.iter().copied().map(var).collect();
            let length = builder.table_length(&scope).unwrap();
            builder
                .add_function(&scope, vec![Cost::BOT; length])
                .unwrap();
        }
        builder.build()
    }

    // -- Bit sets ----------------------------------------------------------

    #[test]
    fn a_bit_set_spans_more_than_one_word() {
        let mut bits = Bits::empty(200);
        bits.insert(3);
        bits.insert(64);
        bits.insert(199);
        assert_eq!(bits.iter().collect::<Vec<_>>(), vec![3, 64, 199]);
        assert_eq!(bits.len(), 3);
        assert!(bits.contains(64));
        bits.remove(64);
        assert!(!bits.contains(64));
        assert_eq!(bits.len(), 2);
    }

    #[test]
    fn a_bit_set_index_past_the_capacity_is_not_a_vertex() {
        let mut bits = Bits::empty(8);
        bits.insert(400);
        assert!(!bits.contains(400));
        assert_eq!(bits.len(), 0);
    }

    // -- The primal graph --------------------------------------------------

    #[test]
    fn a_scope_becomes_a_clique() {
        let cfn = network(&["a", "b", "c"], &[&[0, 1, 2]]);
        let graph = primal_graph(&cfn);
        assert_eq!(graph.n_edges(), 3);
        for (left, right) in [(0, 1), (0, 2), (1, 2)] {
            assert!(graph.has_edge(var(left), var(right)));
        }
    }

    #[test]
    fn unary_cost_adds_no_edge() {
        let spec = vec![
            (Name::new("a"), vec![Name::new("x")]),
            (Name::new("b"), vec![Name::new("x")]),
        ];
        let mut builder = CfnBuilder::new(spec, DEFAULT_WEIGHTS).unwrap();
        builder
            .add_unary_table(var(0), &[Cost::from_raw(1), Cost::from_raw(2)])
            .unwrap();
        let graph = primal_graph(&builder.build());
        assert_eq!(graph.n_vertices(), 2);
        assert_eq!(graph.n_edges(), 0);
    }

    #[test]
    fn a_constraint_with_no_schema_edge_still_joins_its_scope() {
        // The shape a schema span produces: two vertices the schema does not
        // join, constrained together, so the primal graph carries an edge the
        // schema graph does not.
        let cfn = network(&["a", "b", "c"], &[&[0, 1], &[0, 2]]);
        let graph = primal_graph(&cfn);
        assert!(graph.has_edge(var(0), var(2)));
        assert!(!graph.has_edge(var(1), var(2)));
    }

    // -- Components --------------------------------------------------------

    #[test]
    fn components_partition_the_graph() {
        let graph = graph_of(6, &[(0, 1), (1, 2), (4, 5)]);
        let components = graph.components();
        assert_eq!(
            components,
            vec![order_of(&[0, 1, 2]), order_of(&[3]), order_of(&[4, 5])]
        );
    }

    // -- Induced width -----------------------------------------------------

    #[test]
    fn a_path_has_induced_width_one_when_eliminated_from_an_end() {
        let graph = graph_of(4, &[(0, 1), (1, 2), (2, 3)]);
        assert_eq!(induced_width(&graph, &order_of(&[3, 2, 1, 0])), 1);
        assert_eq!(induced_width(&graph, &order_of(&[0, 1, 2, 3])), 1);
    }

    #[test]
    fn a_path_eliminated_from_the_middle_pays_two() {
        // The first vertex eliminated has both its neighbours still in the
        // graph, so it counts two and joins them. Width is a property of the
        // order rather than of the graph, and a path admits both readings.
        let graph = graph_of(4, &[(0, 1), (1, 2), (2, 3)]);
        assert_eq!(induced_width(&graph, &order_of(&[1, 2, 0, 3])), 2);
    }

    #[test]
    fn a_star_has_induced_width_one_from_the_leaves_and_the_degree_from_the_hub() {
        let graph = graph_of(4, &[(0, 1), (0, 2), (0, 3)]);
        assert_eq!(induced_width(&graph, &order_of(&[1, 2, 3, 0])), 1);
        assert_eq!(induced_width(&graph, &order_of(&[0, 1, 2, 3])), 3);
    }

    #[test]
    fn a_triangle_has_induced_width_two_under_every_order() {
        let graph = graph_of(3, &[(0, 1), (1, 2), (0, 2)]);
        for order in [[0, 1, 2], [1, 0, 2], [2, 1, 0]] {
            assert_eq!(induced_width(&graph, &order_of(&order)), 2);
        }
    }

    #[test]
    fn the_complete_graph_on_four_vertices_has_induced_width_three() {
        let graph = graph_of(4, &[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]);
        assert_eq!(induced_width(&graph, &order_of(&[0, 1, 2, 3])), 3);
        assert_eq!(induced_width(&graph, &order_of(&[3, 1, 0, 2])), 3);
    }

    #[test]
    fn a_tree_has_induced_width_one_leaves_first() {
        // A binary tree of seven vertices, rooted at zero.
        let graph = graph_of(7, &[(0, 1), (0, 2), (1, 3), (1, 4), (2, 5), (2, 6)]);
        assert_eq!(induced_width(&graph, &order_of(&[6, 5, 4, 3, 2, 1, 0])), 1);
    }

    #[test]
    fn a_cycle_of_four_has_induced_width_two() {
        let graph = graph_of(4, &[(0, 1), (1, 2), (2, 3), (3, 0)]);
        assert_eq!(induced_width(&graph, &order_of(&[0, 1, 2, 3])), 2);
    }

    #[test]
    fn an_edgeless_graph_has_induced_width_zero() {
        let graph = Graph::new(5);
        assert_eq!(induced_width(&graph, &order_of(&[4, 3, 2, 1, 0])), 0);
    }

    #[test]
    #[should_panic(expected = "permutation of the vertices")]
    fn a_width_cannot_be_computed_against_a_partial_order() {
        let graph = graph_of(3, &[(0, 1)]);
        let _ = induced_width(&graph, &order_of(&[0, 1]));
    }

    // -- Adjusted induced width --------------------------------------------

    #[test]
    fn an_observed_variable_adds_no_fill_and_does_not_enter_the_maximum() {
        // The hub of a star, observed. Eliminating it would otherwise join its
        // three leaves into a triangle and report width three.
        let graph = graph_of(4, &[(0, 1), (0, 2), (0, 3)]);
        let order = order_of(&[0, 1, 2, 3]);
        assert_eq!(induced_width(&graph, &order), 3);
        assert_eq!(induced_width_observed(&graph, &order, &[var(0)]), 0);
    }

    #[test]
    fn observing_a_leaf_leaves_the_rest_of_the_width_alone() {
        let graph = graph_of(4, &[(0, 1), (0, 2), (0, 3)]);
        let order = order_of(&[1, 2, 3, 0]);
        assert_eq!(induced_width_observed(&graph, &order, &[var(1)]), 1);
    }

    #[test]
    fn an_observed_variable_is_absent_from_buckets_processed_before_it() {
        // A five-vertex star eliminated hub first. The hub's bucket joins its
        // live neighbours, and an observed leaf is not one: slicing it out of
        // every table leaves that bucket at arity three, not four. Dropping the
        // observation only when the order reaches the leaf would report four
        // and defeat the point of the adjustment, since the orders it exists to
        // rescue are exactly those that eliminate the fixed variables late.
        let graph = graph_of(5, &[(0, 1), (0, 2), (0, 3), (0, 4)]);
        let hub_first = order_of(&[0, 1, 2, 3, 4]);
        assert_eq!(induced_width(&graph, &hub_first), 4);
        assert_eq!(induced_width_observed(&graph, &hub_first, &[var(1)]), 3);
        assert_eq!(
            induced_width_observed(&graph, &hub_first, &[var(1), var(2)]),
            2
        );
    }

    #[test]
    fn observing_every_variable_leaves_no_width() {
        let graph = graph_of(4, &[(0, 1), (1, 2), (2, 3), (3, 0)]);
        let order = order_of(&[0, 1, 2, 3]);
        let all = order.clone();
        assert_eq!(induced_width_observed(&graph, &order, &all), 0);
    }

    // -- The two orders ----------------------------------------------------

    #[test]
    fn the_reverse_source_order_is_descending_name_order() {
        let cfn = network(&["root", "root.a", "root.a.b"], &[]);
        assert_eq!(
            reverse_source_id_order(&cfn),
            vec![var(2), var(1), var(0)],
            "descending name order, so decode runs ascending"
        );
    }

    #[test]
    fn the_reverse_source_order_reads_names_rather_than_numbering() {
        // Variables offered out of ascending name order: the order still has to
        // be descending by name, which here is not descending by identifier.
        let spec = vec![
            (Name::new("zeta"), vec![Name::new("x")]),
            (Name::new("alpha"), vec![Name::new("x")]),
            (Name::new("mu"), vec![Name::new("x")]),
        ];
        let cfn = CfnBuilder::new(spec, DEFAULT_WEIGHTS).unwrap().build();
        assert_eq!(reverse_source_id_order(&cfn), vec![var(0), var(2), var(1)]);
    }

    #[test]
    fn min_fill_eliminates_a_tree_leaves_first_and_reaches_width_one() {
        let graph = graph_of(7, &[(0, 1), (0, 2), (1, 3), (1, 4), (2, 5), (2, 6)]);
        let order = min_fill_order(&graph);
        assert_eq!(order.len(), 7);
        assert_eq!(induced_width(&graph, &order), 1);
    }

    #[test]
    fn min_fill_takes_the_leaves_of_a_star_before_its_hub() {
        // Eliminating the hub first would join four leaves into a clique and
        // cost width four; min-fill takes leaves, whose elimination adds
        // nothing, until the hub is down to one neighbour and costs one too.
        let graph = graph_of(5, &[(0, 1), (0, 2), (0, 3), (0, 4)]);
        let order = min_fill_order(&graph);
        assert_eq!(order, order_of(&[1, 2, 3, 0, 4]));
        assert_eq!(induced_width(&graph, &order), 1);
        assert_eq!(induced_width(&graph, &order_of(&[0, 1, 2, 3, 4])), 4);
    }

    #[test]
    fn min_fill_is_a_permutation_on_a_disconnected_graph() {
        let graph = graph_of(6, &[(0, 1), (4, 5)]);
        let order = min_fill_order(&graph);
        assert!(is_permutation(&order, 6));
        assert_eq!(induced_width(&graph, &order), 1);
    }

    #[test]
    fn min_fill_is_identical_across_a_hundred_runs() {
        let graph = graph_of(
            8,
            &[
                (0, 1),
                (0, 2),
                (1, 2),
                (1, 3),
                (2, 4),
                (3, 4),
                (4, 5),
                (5, 6),
                (5, 7),
                (6, 7),
            ],
        );
        let first = min_fill_order(&graph);
        for _ in 0..100 {
            assert_eq!(min_fill_order(&graph), first);
        }
    }

    #[test]
    fn min_fill_does_not_depend_on_the_order_the_edges_were_added_in() {
        let edges = [(0, 1), (0, 2), (1, 2), (1, 3), (2, 4), (3, 4), (4, 5)];
        let forward = min_fill_order(&graph_of(6, &edges));
        let mut reversed: Vec<(u32, u32)> = edges.to_vec();
        reversed.reverse();
        let backward = min_fill_order(&graph_of(6, &reversed));
        let swapped: Vec<(u32, u32)> = edges.iter().map(|(l, r)| (*r, *l)).collect();
        assert_eq!(forward, backward);
        assert_eq!(forward, min_fill_order(&graph_of(6, &swapped)));
    }

    // -- The budget --------------------------------------------------------

    #[test]
    fn the_elimination_cost_reads_the_two_exponents_apart() {
        let cfn = network(&["a", "b", "c"], &[&[0, 1]]);
        // Two slots per variable, three variables, one cost function.
        let cost = elimination_cost(&cfn, 2);
        assert_eq!(cost.entries, 3 * 4);
        assert_eq!(cost.operations, (1 + 3) * 8);
    }

    #[test]
    fn an_unaffordable_width_saturates_rather_than_wrapping() {
        let cfn = network(&["a", "b", "c"], &[&[0, 1]]);
        let cost = elimination_cost(&cfn, 1_000);
        assert_eq!(cost.entries, u64::MAX);
        assert_eq!(cost.operations, u64::MAX);
        assert!(!fits_budget(&cfn, 1_000, &SearchBudget::default()));
    }

    #[test]
    fn a_tiny_budget_refuses_a_width_a_large_one_admits() {
        let cfn = network(&["a", "b", "c"], &[&[0, 1]]);
        assert!(fits_budget(&cfn, 2, &SearchBudget::default()));
        let tight = SearchBudget::default().with_mem_bytes(8);
        assert!(!fits_budget(&cfn, 2, &tight));
        let slow = SearchBudget::default().with_op_budget(1);
        assert!(!fits_budget(&cfn, 2, &slow));
    }

    // -- The choice --------------------------------------------------------

    #[test]
    fn the_reverse_source_order_is_taken_on_a_path() {
        let cfn = network(&["root", "root.a", "root.a.b"], &[&[0, 1], &[1, 2]]);
        let (order, width) = choose_order(&cfn);
        assert_eq!(order, vec![var(2), var(1), var(0)]);
        assert_eq!(width, 1);
    }

    #[test]
    fn min_fill_is_taken_when_it_is_narrower_even_though_the_reverse_order_fits() {
        // A star whose hub sorts last by name, so the reverse source order
        // eliminates the hub first and pays width three, while min-fill takes
        // the leaves first and pays one. Width three at four values is a few
        // hundred bytes, so the reverse order fits the default budget
        // comfortably: fitting is not the test, being narrower is.
        let cfn = network(&["a", "b", "c", "zzz"], &[&[0, 3], &[1, 3], &[2, 3]]);
        let graph = primal_graph(&cfn);
        assert_eq!(induced_width(&graph, &reverse_source_id_order(&cfn)), 3);
        assert!(fits_budget(&cfn, 3, &SearchBudget::default()));

        let (order, width) = choose_order(&cfn);
        assert_eq!(width, 1);
        assert_eq!(order.last().copied(), Some(var(3)));
    }

    #[test]
    fn a_hub_inside_the_budget_still_loses_to_min_fill() {
        // The measured regression: eighteen leaves at two values each is a
        // width-eighteen order costing about forty megabytes, inside the
        // default sixty-four, while min-fill is width one. Taking the first
        // candidate that fits would allocate four orders of magnitude more for
        // the same answer.
        let names: Vec<String> = (0..18)
            .map(|index| format!("a{index:03}"))
            .chain(std::iter::once("zhub".to_owned()))
            .collect();
        let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
        let scopes: Vec<Vec<u32>> = (0..18u32).map(|leaf| vec![leaf, 18]).collect();
        let scope_refs: Vec<&[u32]> = scopes.iter().map(Vec::as_slice).collect();
        let cfn = network(&borrowed, &scope_refs);

        let graph = primal_graph(&cfn);
        assert_eq!(induced_width(&graph, &reverse_source_id_order(&cfn)), 18);
        assert!(fits_budget(&cfn, 18, &SearchBudget::default()));

        let (_, width) = choose_order(&cfn);
        assert_eq!(width, 1);
    }

    #[test]
    fn the_reverse_source_order_keeps_the_tie() {
        let cfn = network(&["a", "b", "c"], &[&[0, 1], &[1, 2], &[0, 2]]);
        let (order, width) = choose_order(&cfn);
        assert_eq!(width, 2, "a triangle has induced width two either way");
        assert_eq!(order, reverse_source_id_order(&cfn));
    }

    #[test]
    fn a_network_with_no_variables_has_an_empty_order_of_width_zero() {
        let cfn = CfnBuilder::new(Vec::new(), DEFAULT_WEIGHTS)
            .unwrap()
            .build();
        let (order, width) = choose_order(&cfn);
        assert!(order.is_empty());
        assert_eq!(width, 0);
    }
}
