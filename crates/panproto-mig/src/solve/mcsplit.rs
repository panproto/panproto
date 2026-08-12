//! The injective paths: the maximum common induced sub-schema, and the
//! all-different propagator the injective-morphism path needs.
//!
//! Two settings ask for an injective vertex map and they want different
//! objects. Getting them the wrong way round is the mistake this module exists
//! to make unstatable, so the distinction is stated first.
//!
//! **`iso` wants a maximum common induced sub-schema.** Both legs of the span
//! are monos and the right leg reflects structure as well as preserving it: a
//! source arc runs between two apex vertices exactly when a matching target arc
//! runs between their images. That is what `discover_overlap` and the symmetric
//! lens need, because a symmetric lens' apex has to be a sub-object of both
//! sides, and it is precisely what the partitioning algorithm of `McCreesh`,
//! Prosser and Trimble (*A Partitioning Algorithm for Maximum Common Subgraph
//! Problems*, IJCAI 2017, pp. 712-719) computes. [`solve_iso`] is that
//! algorithm, weighted.
//!
//! **`monic` without `iso` wants an injective schema morphism**, which is
//! edge-preserving but *not* edge-reflecting: the target is allowed to be
//! denser than the source. The label invariant the partitioning algorithm rests
//! on is `v ~ vᵢ ⟺ w ~ wᵢ`, a biconditional, and it is therefore **too
//! strong** for that: it would refuse a perfectly good injective morphism into
//! a target carrying one extra arc. Nothing in this module may be used to
//! answer `monic`. What that path takes from here is
//! [`propagate_all_different`], the counting Hall propagator, and
//! [`epic_satisfied`]; the search itself belongs to branch and bound.
//!
//! # Labels carry feasibility, rewards carry preference
//!
//! The label of a vertex is its kind together with the **arc descriptor**
//! [`arc_descriptor`] to each already-mapped vertex: the multiset of
//! `(direction, edge kind)` pairs, with a self-loop contributing
//! [`Dir::Loop`]. Two vertices may be mapped together exactly when their labels
//! agree, which is the induced-subgraph condition and nothing else.
//!
//! **Edge names are never in the label.** They are the thing being aligned
//! approximately: putting a name in the label makes `user_id` and `userId`
//! structurally incompatible and produces an apex that omits both. Names, kind
//! similarity and anchor evidence live in the reward, which is read off the
//! cost function network. That split is what forces the weighted bound below,
//! and it is the one design decision here that everything else follows from.
//!
//! Multiset equality of `(direction, kind)` bags is exactly the statement that
//! the arcs between one source pair are in direction- and kind-preserving
//! bijection with the arcs between the image pair. So a mapping this module
//! returns has an edge map that is **injective and surjective** onto the
//! induced target arcs, not merely injective.
//!
//! # The reward frame
//!
//! The objective is the network's, not cardinality. Writing `Z` for the cost of
//! the all-`⊥` assignment and `x_M` for the assignment that gives every mapped
//! vertex its image and `⊥` to the rest,
//!
//! ```text
//! R(M) = Z ⊖ cost(x_M)
//!      = Σ_{v ∈ M} [ u(v,⊥) ⊖ u(v,M(v)) ]  +  Σ_f [ f(⊥,⊥) ⊖ f(x_p, x_q) ]
//! ```
//!
//! so maximising `R` and minimising the network's cost are the same problem,
//! exactly, with `Z` a constant of the network. This is a change of origin
//! rather than a shift of the objective: nothing is added per mapped pair, and
//! the trade between reward and apex size stays the one
//! [`DROP_UNIT`](super::cost::DROP_UNIT) already encodes, since `u(v,⊥)` is the
//! only term carrying it.
//!
//! Three properties of the network make that identity hold term by term, and
//! [`solve_iso`] **verifies all three before searching** rather than assuming
//! them, returning [`IsoError`] if any fails:
//!
//! 1. `u(v,⊥)` and `f(⊥,⊥)` are finite, so `Z` is;
//! 2. `u(v,⊥) ⪰ u(v,a)` and `f(⊥,⊥) ⪰ f(a,b)` on every finite entry, so every
//!    reward is non-negative, which the bound's proof needs and which no shift
//!    may be used to arrange;
//! 3. a binary function charges the same for both ways of dropping an endpoint
//!    as for dropping both, `f(a,⊥) = f(⊥,b) = f(⊥,⊥)`, unless it is `⊤` there.
//!
//! Clause 3 is what makes a pair of mapped vertices the only thing a binary
//! function can pay for, so the reward decomposes into the per-pair increments
//! the bound is stated over. The `⊤` escape is the apex well-formedness
//! constraints — a required edge, a variant, a recursion point, a span, a
//! hyper-edge signature — which forbid mapping one endpoint while dropping the
//! other. Those are *feasibility*, and they are enforced exactly, by scoring
//! every candidate incumbent with [`Cfn::evaluate`] and refusing an infeasible
//! one. They never enter the bound, which is admissible over a superset of the
//! feasible mappings and so stays admissible when they cut it down.
//!
//! # Bound B1
//!
//! At a node with mapping `M` and label classes `future`, write `Δ_M(v,w)` for
//! the exact reward increment of appending `(v,w)`, `k_l = min(|G_l|,|H_l|)`,
//! and `topsum_k(f)` for the sum of the `k` largest values of `f`. Let
//!
//! ```text
//! maxw(f)  = max over feasible (a,b) of [ f(⊥,⊥) ⊖ f(a,b) ]
//! h_G(v)   = ½ · Σ_{f on {v,v'} : v' still selectable} maxw(f)
//! ρ_l(v)   = h_G(v) + max_{w ∈ H_l} Δ_M(v,w)        (0 if no w is feasible)
//! γ_l(w)   = h_H(w) + max_{v ∈ G_l} Δ_M(v,w)        (the mirror image)
//! B_l      = min( topsum_{k_l}(ρ_l), topsum_{k_l}(γ_l) )
//! bound    = R(M) + Σ_l B_l
//! ```
//!
//! **Theorem (admissibility).** For every valid extension `M* ⊇ M` *reachable
//! from this node*, `R(M*) ⪯ bound`.
//!
//! The qualifier is load-bearing and is the first thing to get wrong when
//! checking this claim from outside. A node reached by a drop branch has
//! already committed that `v` is unmapped, so the extensions the theorem
//! quantifies over are those leaving `v` out; the ones that map it belong to
//! the sibling subtree, where the bound was computed with `v` still
//! selectable. Quantifying over every extension of `M` regardless of the
//! branch reports the bound as inadmissible at drop nodes, on instances where
//! it is not.
//!
//! *Proof.* Write `N = M* \ M`. The reward decomposes as
//! `R(M*) − R(M) = Σ_{p ∈ N} Δ_M(p) + Σ_{{p,q} ⊆ N} E(p,q)`, where `E(p,q)` is
//! the reward of the binary function joining the two newly mapped source
//! vertices.
//!
//! *(i) The pairwise term.* Fix `{p,q} ⊆ N`. `E(p,q) ⪯ maxw(f)` for the one
//! function `f` on that scope, by definition of `maxw`. Both endpoints of `f`
//! are unmapped at this node, so `f` contributes `½ maxw(f)` to `h_G(v_p)` and
//! `½ maxw(f)` to `h_G(v_q)`, together its whole value. Summing over all
//! unordered `{p,q} ⊆ N`, every function is charged at most once in full and
//! functions whose endpoints are not both selected contribute slack, so
//! `Σ_{{p,q} ⊆ N} E(p,q) ⪯ Σ_{p ∈ N} h_G(v_p)`. This covers functions joining
//! *different* label classes as well, because `h_G` sums over every still
//! selectable neighbour rather than over same-class ones.
//!
//! *(ii) Confinement.* Labels only ever refine — a longer mapping appends
//! components to every label, so vertices that differ at depth `m` differ at
//! every greater depth — hence label classes only ever split. Every
//! `(v,w) ∈ N` therefore lies in a single class `l` of `future`, and `N`
//! partitions into blocks `N_l ⊆ G_l × H_l` with distinct left components and
//! distinct right components, so `|N_l| ⪯ k_l`.
//!
//! *(iii) Combine.*
//! `R(M*) − R(M) ⪯ Σ_{p ∈ N} [ Δ_M(p) + h_G(v_p) ] ⪯ Σ_l Σ_{(v,w) ∈ N_l} ρ_l(v)
//! ⪯ Σ_l topsum_{|N_l|}(ρ_l) ⪯ Σ_l topsum_{k_l}(ρ_l)`. The middle step is the
//! definition of `ρ_l` as a maximum over `H_l`; the last needs `ρ_l ⪰ 0`, which
//! is clause 2 of the reward frame, and is why a negative reward would break
//! the bound rather than merely weaken it. The `γ` chain is the mirror image,
//! charging to distinct right components. Both bound the same quantity, so
//! their minimum does. ∎
//!
//! **With `w_V ≡ 1` and `w_E ≡ 0` this is `|M| + Σ_l min(|G_l|,|H_l|)`**, the
//! unweighted bound of the IJCAI paper, so the weighted bound is a strict
//! generalisation and the unweighted case is a regression test.
//!
//! The halving is rounded **up**: two halves that each rounded down could sum
//! to less than the whole and the charge in (i) would no longer cover `E(p,q)`.
//!
//! # Determinism
//!
//! Every tie-break ends in a total order on stable identifiers, and no
//! iteration order of a hash map is ever observable in the result: label
//! classes are ordered by their descriptor identity, vertices by index,
//! and values by target index. A schema version-control consumer gets the same
//! span for the same inputs across runs and machines.
//!
//! # Value identifiers are per variable
//!
//! [`ValId`] numbers a variable's own sorted candidate list, so `ValId::real(0)`
//! means a different target vertex for two different variables. Every
//! comparison of one variable's value against another's — injectivity, the
//! all-different propagator, the right-hand side of a label class — therefore
//! goes through [`ValueIndex`], one numbering of the target vertices shared by
//! the whole network, and [`TargetId`] is the type that says so. The two
//! numberings meet only at [`ValueIndex::global`] and [`ValueIndex::local`].

use std::time::Instant;

use panproto_gat::Name;
use panproto_schema::Schema;
use rustc_hash::FxHashMap;

use super::cfn::{Cfn, CostFunction, Domain, Variable};
use super::cost::Cost;
use super::{
    Assignment, DEFAULT_SEARCH_NODES, LimitKind, SearchBudget, SolveOutcome, SolverPath, ValId,
    VarId,
};

// ---------------------------------------------------------------------------
// Arc descriptors
// ---------------------------------------------------------------------------

/// Which way an arc runs, relative to the ordered vertex pair describing it.
///
/// The ordering matters: it is the sort key of a descriptor's entries, so it is
/// part of what makes the digest a function of the multiset rather than of the
/// order the arcs were read in.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dir {
    /// An arc from the first vertex of the pair to the second.
    Out,
    /// An arc from the second vertex of the pair to the first.
    In,
    /// A self-loop, which is the whole descriptor of a vertex against itself.
    ///
    /// A loop is neither out nor in: both readings would name the same arc, and
    /// recording it twice would make a vertex with one loop look like a vertex
    /// with two arcs.
    Loop,
}

impl Dir {
    /// The byte this direction contributes to a digest.
    const fn tag(self) -> u8 {
        match self {
            Self::Out => 1,
            Self::In => 2,
            Self::Loop => 3,
        }
    }
}

/// The arcs between one ordered pair of vertices, as a sorted multiset of
/// `(direction, edge kind)`.
///
/// This is the whole label alphabet. Edge *names* are deliberately absent: see
/// the module docs.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ArcDescriptor {
    entries: Vec<(Dir, Name)>,
}

impl ArcDescriptor {
    /// The descriptor holding these arcs, whatever order they arrive in.
    ///
    /// Sorting here is what makes two descriptors equal exactly when their
    /// multisets are, and is why [`Self::digest`] does not depend on the order
    /// a schema happened to store its edges in.
    #[must_use]
    pub fn from_arcs<I: IntoIterator<Item = (Dir, Name)>>(arcs: I) -> Self {
        let mut entries: Vec<(Dir, Name)> = arcs.into_iter().collect();
        entries.sort_unstable();
        Self { entries }
    }

    /// The arcs, sorted.
    #[inline]
    #[must_use]
    pub fn entries(&self) -> &[(Dir, Name)] {
        &self.entries
    }

    /// How many arcs the pair carries.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the pair carries no arc at all, which is the common case.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// A 64-bit digest of the sorted multiset.
    ///
    /// The mixing function is written out here rather than taken from a hasher
    /// so that the value is a documented function of the input, stable across
    /// runs, machines and compiler versions. That matters because a span is
    /// content-addressed downstream.
    ///
    /// The search does **not** use it. Deciding whether two vertices may be
    /// mapped together goes through interned descriptor identities instead,
    /// which cannot collide, and interning hashes the sorted multiset with the
    /// map's own hasher rather than with this. What the digest is for is a
    /// caller certifying a span: a stable, portable fingerprint of the label a
    /// pair of vertices carries, which an identity local to one search cannot
    /// be.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut state = DIGEST_BASIS;
        for (direction, kind) in &self.entries {
            state = digest_byte(state, direction.tag());
            for byte in kind.as_str().as_bytes() {
                state = digest_byte(state, *byte);
            }
            state = digest_byte(state, DIGEST_SEPARATOR);
        }
        state
    }
}

/// The offset basis of the digest, taken from FNV-1a.
const DIGEST_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

/// The multiplier of the digest, taken from FNV-1a.
const DIGEST_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The byte separating one arc from the next, so that two kinds cannot be run
/// together into a third.
const DIGEST_SEPARATOR: u8 = 0xff;

/// One FNV-1a step.
const fn digest_byte(state: u64, byte: u8) -> u64 {
    (state ^ byte as u64).wrapping_mul(DIGEST_PRIME)
}

/// The arcs a schema puts between one ordered pair of vertices.
///
/// `(from, to)` and `(to, from)` are reverses of each other: every
/// [`Dir::Out`] in one is a [`Dir::In`] in the other. When the two names are
/// equal the result is the vertex's loops, each contributing one
/// [`Dir::Loop`].
///
/// A name the schema does not hold has no arcs, so the descriptor is empty.
#[must_use]
pub fn arc_descriptor(schema: &Schema, from: &Name, to: &Name) -> ArcDescriptor {
    let mut arcs: Vec<(Dir, Name)> = Vec::new();
    if from == to {
        for edge in schema.edges_between(from.as_str(), to.as_str()) {
            arcs.push((Dir::Loop, edge.kind.clone()));
        }
    } else {
        for edge in schema.edges_between(from.as_str(), to.as_str()) {
            arcs.push((Dir::Out, edge.kind.clone()));
        }
        for edge in schema.edges_between(to.as_str(), from.as_str()) {
            arcs.push((Dir::In, edge.kind.clone()));
        }
    }
    ArcDescriptor::from_arcs(arcs)
}

// ---------------------------------------------------------------------------
// One numbering of the target vertices
// ---------------------------------------------------------------------------

/// A target vertex, numbered across the whole network rather than within one
/// variable's domain.
///
/// Ascending order is ascending target vertex name, so two runs over the same
/// network agree on the numbering and every tie-break phrased in it is stable.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetId(u32);

impl TargetId {
    /// The identifier as it is stored.
    #[inline]
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// The identifier, for use as a slice offset.
    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// One numbering of every target vertex some variable of a network can take.
///
/// [`ValId`] is dense over one variable's own candidate list, so the same
/// [`ValId`] means different target vertices for different variables and two of
/// them cannot be compared. Anything that has to compare across variables —
/// injectivity, the all-different propagator, the right-hand side of a label
/// class — works in [`TargetId`] and converts at the edges.
///
/// Targets no variable can take are absent: they cannot appear in an
/// assignment, so numbering them would only inflate every bitset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueIndex {
    names: Vec<Name>,
    per_variable: Vec<Vec<TargetId>>,
}

impl ValueIndex {
    /// The numbering of one network's target vertices.
    #[must_use]
    pub fn of(cfn: &Cfn) -> Self {
        let mut names: Vec<Name> = cfn
            .variables()
            .iter()
            .flat_map(|variable| variable.values().iter().cloned())
            .collect();
        names.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
        names.dedup();

        let position: FxHashMap<&str, u32> = names
            .iter()
            .enumerate()
            .filter_map(|(slot, name)| u32::try_from(slot).ok().map(|raw| (name.as_str(), raw)))
            .collect();

        let per_variable = cfn
            .variables()
            .iter()
            .map(|variable| {
                variable
                    .values()
                    .iter()
                    .map(|name| TargetId(position.get(name.as_str()).copied().unwrap_or(u32::MAX)))
                    .collect()
            })
            .collect();

        Self {
            names,
            per_variable,
        }
    }

    /// How many distinct target vertices are numbered.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether no variable can take any target at all.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The target vertices, in ascending name order.
    #[inline]
    #[must_use]
    pub fn names(&self) -> &[Name] {
        &self.names
    }

    /// The name of one target vertex.
    #[inline]
    #[must_use]
    pub fn name(&self, target: TargetId) -> Option<&Name> {
        self.names.get(target.index())
    }

    /// The global identifier of one variable's value.
    ///
    /// `None` for `⊥`, which is not a target vertex, and for a value the
    /// variable cannot take.
    #[must_use]
    pub fn global(&self, var: VarId, value: ValId) -> Option<TargetId> {
        if value.is_bottom() {
            return None;
        }
        self.per_variable
            .get(var.index())?
            .get(value.index())
            .copied()
    }

    /// The value one variable takes to reach a target vertex.
    ///
    /// `None` when the variable cannot take it. The per-variable list is
    /// ascending in target order, so this is a binary search.
    #[must_use]
    pub fn local(&self, var: VarId, target: TargetId) -> Option<ValId> {
        let slot = self
            .per_variable
            .get(var.index())?
            .binary_search(&target)
            .ok()?;
        u32::try_from(slot).ok().map(ValId::real)
    }
}

// ---------------------------------------------------------------------------
// The counting all-different propagator
// ---------------------------------------------------------------------------

/// What one pass of [`propagate_all_different`] concluded.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum HallOutcome {
    /// The domains are Hall-consistent, after removing `removed` values.
    Filtered {
        /// How many `(variable, value)` pairs the pass removed.
        removed: usize,
    },
    /// No injective assignment exists: some set of variables that must take a
    /// target has fewer targets between them than it has members.
    Wipeout,
}

/// One sorted pass of the counting Hall propagator over a set of domains.
///
/// This is Algorithm 6 of `McCreesh` and Prosser (*A Parallel, Backjumping
/// Subgraph Isomorphism Algorithm Using Supplemental Graphs*, CP 2015): sort
/// the domains by size, sweep accumulating their union, fail when the union is
/// smaller than the number of domains accumulated, and when the two are equal
/// freeze that union as a Hall set, remove it from every other domain, and
/// restart the accumulator. It is `O(v log v + v·d)` and it is **stateless**:
/// it reads and writes the domains and keeps nothing between calls, which is
/// what lets a copy-on-branch domain store use it with no trail.
///
/// Régin's matching-based generalised arc consistency is stronger and is
/// deliberately not maintained here: its incremental matching and strongly
/// connected component state is one to three orders of magnitude larger than
/// the domain store it would guard. Running it once at a search root as a
/// preprocessing filter is a separate, sound option.
///
/// # `⊥` is not a value
///
/// A variable that may still be dropped can always escape the pigeonhole, so it
/// is **never counted** toward a Hall set and never causes failure. It is still
/// pruned by one: a Hall set of `k` variables that must take targets consumes
/// all `k` of those targets, so nobody else may take one. On the span search
/// every domain carries `⊥`, so the propagator is inert there by design; it
/// bites on the total-morphism restriction, where `⊥` is removed from every
/// domain, and inside search, where an assigned variable is a singleton.
///
/// # Not a fixed point
///
/// One pass. The sweep order is fixed at entry, so pruning that shrinks a
/// domain below its neighbours in the order is not resorted, and a second call
/// can find more. Neither the soundness of the failure nor the soundness of the
/// pruning depends on the order, so calling it once and calling it to
/// quiescence differ in strength, never in correctness.
///
/// `domains` is positional: entry `i` is the domain of `VarId::new(i)`, in the
/// per-variable [`ValId`] numbering, and `index` is what makes two variables'
/// values comparable.
pub fn propagate_all_different(index: &ValueIndex, domains: &mut [Domain]) -> HallOutcome {
    let words = index.len().div_ceil(u64::BITS as usize).max(1);
    let mut bits = vec![0u64; domains.len() * words];
    let mut order: Vec<(u32, usize)> = Vec::new();

    for (position, domain) in domains.iter().enumerate() {
        let Ok(raw) = u32::try_from(position) else {
            continue;
        };
        let var = VarId::new(raw);
        let mut size = 0u32;
        for value in *domain {
            if let Some(target) = index.global(var, value) {
                let offset = position * words + target.index() / u64::BITS as usize;
                bits[offset] |= 1u64 << (target.index() % u64::BITS as usize);
                size += 1;
            }
        }
        if !domain.contains(ValId::BOTTOM) {
            order.push((size, position));
        }
    }

    order.sort_unstable();
    if sweep(&order, &mut bits, words, domains.len()) {
        return HallOutcome::Wipeout;
    }
    HallOutcome::Filtered {
        removed: restrict(index, domains, &bits, words),
    }
}

/// The accumulating sweep. Returns whether a pigeonhole violation was found.
fn sweep(order: &[(u32, usize)], bits: &mut [u64], words: usize, variables: usize) -> bool {
    let mut accumulator = vec![0u64; words];
    let mut group = vec![u32::MAX; variables];
    let mut generation = 0u32;
    let mut count = 0usize;

    for &(_, position) in order {
        for word in 0..words {
            accumulator[word] |= bits[position * words + word];
        }
        group[position] = generation;
        count += 1;

        let reachable: usize = accumulator
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum();
        if reachable < count {
            return true;
        }
        if reachable > count {
            continue;
        }

        // A Hall set: these `count` variables must take targets and have
        // exactly `count` between them, so every other variable loses all of
        // them. Earlier Hall sets are disjoint from this one already, having
        // been removed from every domain the sweep had not yet reached.
        for other in 0..variables {
            if group[other] == generation {
                continue;
            }
            for word in 0..words {
                bits[other * words + word] &= !accumulator[word];
            }
        }
        accumulator.fill(0);
        generation = generation.wrapping_add(1);
        count = 0;
    }
    false
}

/// Write the surviving bits back into the domains, returning how many values
/// were removed.
///
/// A value the index does not number is left alone: nothing can be concluded
/// about a target the propagator never saw, and leaving it is the direction
/// that cannot over-prune.
fn restrict(index: &ValueIndex, domains: &mut [Domain], bits: &[u64], words: usize) -> usize {
    let mut removed = 0usize;
    for (position, domain) in domains.iter_mut().enumerate() {
        let Ok(raw) = u32::try_from(position) else {
            continue;
        };
        let var = VarId::new(raw);
        for value in *domain {
            let Some(target) = index.global(var, value) else {
                continue;
            };
            let offset = position * words + target.index() / u64::BITS as usize;
            if bits[offset] & (1u64 << (target.index() % u64::BITS as usize)) == 0 {
                domain.remove(value);
                removed += 1;
            }
        }
    }
    removed
}

/// Whether an assignment's vertex map covers every target vertex.
///
/// Surjectivity is a **leaf check**. It is not propagated and it takes no part
/// in any bound: it constrains the whole assignment at once, so a partial
/// assignment carries no information about it beyond the count of targets still
/// reachable, and paying to maintain that would buy pruning the measured corpus
/// never needs.
///
/// It is satisfiable only when the source has at least as many vertices as the
/// target, since the vertex map is a function; on the injective path, where it
/// is also injective, only when the two counts are equal.
///
/// `target_vertices` is the size of the target schema, not the size of
/// `index`: a target vertex no variable can take is one this assignment can
/// never cover, and reading the count off the index instead would report such a
/// search as surjective.
#[must_use]
pub fn epic_satisfied(index: &ValueIndex, assignment: &Assignment, target_vertices: usize) -> bool {
    let mut seen: Vec<TargetId> = assignment
        .pairs()
        .filter_map(|(var, value)| index.global(var, value))
        .collect();
    seen.sort_unstable();
    seen.dedup();
    seen.len() == target_vertices
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why the maximum common induced sub-schema search refused a network.
///
/// Every variant reports a precondition of the reward frame that the network
/// broke. None of them is reachable from a network
/// [`build_cfn`](super::build::build_cfn) produced: they exist because a search
/// that silently optimised a different objective would be worse than one that
/// refuses, and because they name exactly what a hand-built network has to
/// respect.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum IsoError {
    /// The all-`⊥` assignment is infeasible, so the reward frame has no origin.
    ///
    /// Every reward is measured against the cost of dropping everything. A
    /// network that forbids dropping everything has no such cost, and the
    /// search would have nothing to measure against.
    #[error("dropping every source vertex is infeasible, so there is no reward origin")]
    BottomInfeasible {
        /// The variable whose `⊥` is infeasible, if one is.
        variable: Option<VarId>,
        /// The scope whose all-`⊥` entry is infeasible, if one is.
        scope: Option<Vec<VarId>>,
    },

    /// Mapping a source vertex costs more than dropping it.
    ///
    /// The reward for mapping would then be negative, and the bound's proof
    /// needs it non-negative at the step that enlarges a partial sum to the
    /// class's whole capacity. Clamping the reward at zero would keep the bound
    /// admissible while quietly optimising a different objective.
    #[error("mapping is dearer than dropping on variable {variable:?}, so a reward is negative")]
    NegativeReward {
        /// The variable carrying the offending entry.
        variable: VarId,
        /// The scope carrying it, when the entry is a binary one.
        scope: Option<Vec<VarId>>,
    },

    /// A binary function charges differently for the two ways of dropping one
    /// endpoint.
    ///
    /// The reward would then depend on which vertices are left unmapped at the
    /// end rather than on which pairs are mapped, and the per-pair increments
    /// the bound is stated over would not sum to it.
    #[error("the function on {scope:?} charges unevenly for dropping one endpoint")]
    UnevenDropCharge {
        /// The scope carrying the offending entry.
        scope: Vec<VarId>,
    },

    /// A cost function constrains more than two variables.
    ///
    /// The bound charges the reward of a joint mapping half to each of the two
    /// vertices it joins, and that charge has no counterpart for a function
    /// three vertices have to be mapped together to pay. Refusing is the only
    /// honest answer: a search that ignored the function would prune with a
    /// bound that is not one.
    ///
    /// The decomposition of a schema pair produces no such function. A
    /// hyper-edge signature, the one construct of arity above two, is emitted
    /// as a clique of pairwise constraints, which is equivalent to it.
    #[error("the function on {scope:?} constrains more than two variables")]
    UnsupportedArity {
        /// The scope carrying the offending function.
        scope: Vec<VarId>,
    },

    /// A variable names a vertex the source schema does not hold, or a value
    /// names a vertex the target schema does not hold.
    ///
    /// The network and the schema pair have to be the same pair: the network
    /// carries the rewards and the schemas carry the arcs, and a label cannot
    /// be computed for a vertex only one of them knows about.
    #[error("`{vertex}` is in the network but not in the {side} schema")]
    SchemaMismatch {
        /// The vertex the network named.
        vertex: Name,
        /// Which schema was missing it: `"source"` or `"target"`.
        side: &'static str,
    },
}

// ---------------------------------------------------------------------------
// The instance
// ---------------------------------------------------------------------------

/// The descriptor identity of a pair with no arc between it.
const EMPTY_DESCRIPTOR: u32 = 0;

/// A binary cost function, with everything the reward frame reads off it.
struct BinaryFunction<'a> {
    /// The lower-numbered variable of the scope.
    low: u32,
    /// The higher-numbered variable of the scope.
    high: u32,
    /// How many table slots the higher-numbered variable spans.
    high_slots: usize,
    /// `f(⊥,⊥)`, the origin every entry is measured against.
    bottom: Cost,
    /// The table, row-major with `high` varying fastest.
    table: &'a [Cost],
    /// The largest reward any feasible pair of images can earn here.
    max_reward: u64,
}

/// Everything the search reads and never writes.
struct Instance<'a> {
    cfn: &'a Cfn,
    index: ValueIndex,
    /// Number of variables.
    lefts: usize,
    /// Number of target vertices.
    rights: usize,
    /// `feasible[v * rights + t]`: whether `v` may take `t` at finite cost.
    feasible: Vec<bool>,
    /// `unary_reward[v * rights + t]`: `u(v,⊥) ⊖ u(v,t)`.
    unary_reward: Vec<u64>,
    /// The label class of each variable at the root: kind and self-loops.
    left_class: Vec<u32>,
    /// The label class of each target vertex at the root.
    right_class: Vec<u32>,
    /// The descriptor identity between two source vertices, when non-empty.
    left_descriptor: FxHashMap<(u32, u32), u32>,
    /// The descriptor identity between two target vertices, when non-empty.
    right_descriptor: FxHashMap<(u32, u32), u32>,
    /// The binary functions, in network order.
    functions: Vec<BinaryFunction<'a>>,
    /// `incident[v]`: the other endpoint and the function index, per neighbour.
    incident: Vec<Vec<(u32, u32)>>,
    /// `pair_h[t * rights + t']`: the largest reward any function can pay for
    /// that unordered pair of images. The `γ` side of the bound reads it.
    pair_h: Vec<u64>,
    /// The out plus in degree of each source vertex, for the vertex tie-break.
    degree: Vec<u32>,
    /// `Z`: the cost of dropping everything.
    baseline: Cost,
}

impl<'a> Instance<'a> {
    /// Read a network and a schema pair into everything the search needs.
    ///
    /// # Errors
    ///
    /// [`IsoError`] when the network breaks a reward-frame precondition or
    /// names a vertex the schema pair does not hold.
    fn new(cfn: &'a Cfn, src: &Schema, tgt: &Schema) -> Result<Self, IsoError> {
        let index = ValueIndex::of(cfn);
        let lefts = cfn.n_variables();
        let rights = index.len();

        let baseline = verify_frame(cfn)?;
        let (feasible, unary_reward) = unary_frame(cfn, &index, lefts, rights);
        let functions = binary_frame(cfn);
        let incident = incidence(lefts, &functions);
        let pair_h = target_pair_rewards(cfn, &index, &functions, rights);

        let mut interner: FxHashMap<Vec<(Dir, Name)>, u32> = FxHashMap::default();
        interner.insert(Vec::new(), EMPTY_DESCRIPTOR);
        let left_names = source_names(cfn, src)?;
        let target_names = target_names(&index, tgt)?;

        let left_descriptor = descriptors(src, &left_names, &mut interner);
        let right_descriptor = descriptors(tgt, &target_names, &mut interner);
        let mut classes: FxHashMap<(Name, u32), u32> = FxHashMap::default();
        let left_class = vertex_classes(src, &left_names, &left_descriptor, &mut classes);
        let right_class = vertex_classes(tgt, &target_names, &right_descriptor, &mut classes);

        let degree = left_names
            .iter()
            .map(|name| {
                let out = src.outgoing_edges(name.as_str()).len();
                let into = src.incoming_edges(name.as_str()).len();
                u32::try_from(out + into).unwrap_or(u32::MAX)
            })
            .collect();

        Ok(Self {
            cfn,
            index,
            lefts,
            rights,
            feasible,
            unary_reward,
            left_class,
            right_class,
            left_descriptor,
            right_descriptor,
            functions,
            incident,
            pair_h,
            degree,
            baseline,
        })
    }

    /// Whether `v` may take `t`.
    fn can_map(&self, left: u32, right: u32) -> bool {
        self.feasible[left as usize * self.rights + right as usize]
    }

    /// The descriptor identity between two source vertices.
    fn left_key(&self, from: u32, to: u32) -> u32 {
        self.left_descriptor
            .get(&(from, to))
            .copied()
            .unwrap_or(EMPTY_DESCRIPTOR)
    }

    /// The descriptor identity between two target vertices.
    fn right_key(&self, from: u32, to: u32) -> u32 {
        self.right_descriptor
            .get(&(from, to))
            .copied()
            .unwrap_or(EMPTY_DESCRIPTOR)
    }

    /// The label classes before anything is mapped.
    ///
    /// Vertex kind and self-loops on both sides, with a class kept only when
    /// both sides have a member. Variables that can take nothing and targets
    /// nobody can take are left out: they cannot appear in any mapping, and
    /// counting them would only loosen `min(|G_l|,|H_l|)`.
    fn root(&self) -> Vec<Bidomain> {
        let mut classes: FxHashMap<u32, (Vec<u32>, Vec<u32>)> = FxHashMap::default();
        let lefts = u32::try_from(self.lefts).unwrap_or(u32::MAX);
        let rights = u32::try_from(self.rights).unwrap_or(u32::MAX);
        for left in 0..lefts {
            if (0..rights).any(|right| self.can_map(left, right)) {
                classes
                    .entry(self.left_class[left as usize])
                    .or_default()
                    .0
                    .push(left);
            }
        }
        for right in 0..rights {
            if (0..lefts).any(|left| self.can_map(left, right)) {
                classes
                    .entry(self.right_class[right as usize])
                    .or_default()
                    .1
                    .push(right);
            }
        }

        let mut future: Vec<(u32, Bidomain)> = classes
            .into_iter()
            .filter(|(_, (left, right))| !left.is_empty() && !right.is_empty())
            .map(|(key, (left, right))| (key, Bidomain { left, right }))
            .collect();
        future.sort_unstable_by_key(|(key, _)| *key);
        future.into_iter().map(|(_, class)| class).collect()
    }

    /// Split every label class by the descriptor to a newly mapped pair.
    ///
    /// This is the replacement for lines 11 to 19 of the IJCAI paper's
    /// Algorithm 1, generalised from the paper's four-way adjacency split to
    /// the descriptor's arbitrarily many keys. Only keys present on both sides
    /// survive: a key on one side alone contributes `min(·,0) = 0` to the bound
    /// and can never be extended.
    fn refine(&self, future: &[Bidomain], left: u32, right: u32) -> Vec<Bidomain> {
        let mut out = Vec::with_capacity(future.len() * 2);
        for class in future {
            let mut order: Vec<u32> = Vec::new();
            let mut buckets: FxHashMap<u32, (Vec<u32>, Vec<u32>)> = FxHashMap::default();
            for &other in &class.left {
                if other == left {
                    continue;
                }
                let key = self.left_key(other, left);
                let bucket = buckets.entry(key).or_insert_with(|| {
                    order.push(key);
                    (Vec::new(), Vec::new())
                });
                bucket.0.push(other);
            }
            for &other in &class.right {
                if other == right {
                    continue;
                }
                let key = self.right_key(other, right);
                let bucket = buckets.entry(key).or_insert_with(|| {
                    order.push(key);
                    (Vec::new(), Vec::new())
                });
                bucket.1.push(other);
            }
            order.sort_unstable();
            for key in order {
                let Some((left_side, right_side)) = buckets.remove(&key) else {
                    continue;
                };
                if !left_side.is_empty() && !right_side.is_empty() {
                    out.push(Bidomain {
                        left: left_side,
                        right: right_side,
                    });
                }
            }
        }
        out
    }
}

/// The source vertex name of every variable, checked against the schema.
fn source_names(cfn: &Cfn, src: &Schema) -> Result<Vec<Name>, IsoError> {
    cfn.variables()
        .iter()
        .map(|variable| {
            let name = variable.name();
            if src.vertices.contains_key(name) {
                Ok(name.clone())
            } else {
                Err(IsoError::SchemaMismatch {
                    vertex: name.clone(),
                    side: "source",
                })
            }
        })
        .collect()
}

/// The target vertex name of every numbered value, checked against the schema.
fn target_names(index: &ValueIndex, tgt: &Schema) -> Result<Vec<Name>, IsoError> {
    index
        .names()
        .iter()
        .map(|name| {
            if tgt.vertices.contains_key(name) {
                Ok(name.clone())
            } else {
                Err(IsoError::SchemaMismatch {
                    vertex: name.clone(),
                    side: "target",
                })
            }
        })
        .collect()
}

/// Every non-empty arc descriptor of one schema, interned to an identity.
///
/// Interning rather than digesting is what makes the label test exact. Keying
/// a label class by a 64-bit digest, as the specification of this search does,
/// leaves a collision between two different multisets free to map a pair the
/// induced-subgraph condition forbids, and no test could be expected to find
/// it. An interned identity compares equal only when the sorted multisets do,
/// and the comparison is still one integer, because the interner resolves the
/// collision when it inserts.
///
/// One interner spans both schemas, so an identity means the same multiset on
/// each side. An arc kind only one schema uses lands in a class the other
/// cannot reach, which is the right answer: no target arc of that kind exists
/// for a source arc of it to map to.
///
/// # Why the pairs are sorted before they are interned
///
/// The interner hands out identities in the order it is asked, and those
/// identities order the label classes [`Instance::refine`] produces, whose
/// positions are the last tie-break in [`Search::select_class`]. So whatever
/// orders the interning orders the branching. The pairs are gathered by walking
/// `schema.edges`, a [`std::collections::HashMap`] whose iteration order is a
/// function of a per-process random seed, and that order reaches the gathering
/// map's own layout. Sorting the pairs first makes the identities a function of
/// the schema and nothing else, which is what guarantee 6 of
/// [`solve`](super) claims and what this search would otherwise quietly break.
/// An ordered pair of vertex positions, one row of the descriptor table.
type VertexPair = (u32, u32);

fn descriptors(
    schema: &Schema,
    names: &[Name],
    interner: &mut FxHashMap<Vec<(Dir, Name)>, u32>,
) -> FxHashMap<(u32, u32), u32> {
    let position: FxHashMap<&str, u32> = names
        .iter()
        .enumerate()
        .filter_map(|(slot, name)| u32::try_from(slot).ok().map(|raw| (name.as_str(), raw)))
        .collect();

    let mut arcs: FxHashMap<(u32, u32), Vec<(Dir, Name)>> = FxHashMap::default();
    for edge in schema.edges.keys() {
        let (Some(&from), Some(&to)) = (
            position.get(edge.src.as_str()),
            position.get(edge.tgt.as_str()),
        ) else {
            continue;
        };
        if from == to {
            arcs.entry((from, from))
                .or_default()
                .push((Dir::Loop, edge.kind.clone()));
        } else {
            arcs.entry((from, to))
                .or_default()
                .push((Dir::Out, edge.kind.clone()));
            arcs.entry((to, from))
                .or_default()
                .push((Dir::In, edge.kind.clone()));
        }
    }

    let mut pairs: Vec<(VertexPair, Vec<(Dir, Name)>)> = arcs.into_iter().collect();
    pairs.sort_unstable_by_key(|(pair, _)| *pair);
    pairs
        .into_iter()
        .map(|(pair, entries)| {
            let key = ArcDescriptor::from_arcs(entries).entries;
            let next = u32::try_from(interner.len()).unwrap_or(u32::MAX);
            let identity = *interner.entry(key).or_insert(next);
            (pair, identity)
        })
        .collect()
}

/// The root label class of every vertex: its kind together with its loops.
///
/// The IJCAI paper splits vertex-labelled inputs by label and then again by
/// loop presence. The descriptor subsumes the second split, since a vertex's
/// descriptor against itself *is* its loops, kinds and multiplicities included.
fn vertex_classes(
    schema: &Schema,
    names: &[Name],
    descriptor: &FxHashMap<(u32, u32), u32>,
    classes: &mut FxHashMap<(Name, u32), u32>,
) -> Vec<u32> {
    names
        .iter()
        .enumerate()
        .map(|(slot, name)| {
            let kind = schema
                .vertices
                .get(name)
                .map_or_else(|| Name::from(""), |vertex| vertex.kind.clone());
            let loops = u32::try_from(slot)
                .ok()
                .and_then(|raw| descriptor.get(&(raw, raw)).copied())
                .unwrap_or(EMPTY_DESCRIPTOR);
            let next = u32::try_from(classes.len()).unwrap_or(u32::MAX);
            *classes.entry((kind, loops)).or_insert(next)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The reward frame
// ---------------------------------------------------------------------------

/// Check the three preconditions of the reward frame and return `Z`.
fn verify_frame(cfn: &Cfn) -> Result<Cost, IsoError> {
    let mut baseline = cfn.c_empty();
    if baseline == Cost::TOP_SENTINEL {
        return Err(IsoError::BottomInfeasible {
            variable: None,
            scope: None,
        });
    }

    for var in cfn.variable_ids() {
        let bottom = cfn
            .unary_cost(var, ValId::BOTTOM)
            .unwrap_or(Cost::TOP_SENTINEL);
        if bottom == Cost::TOP_SENTINEL {
            return Err(IsoError::BottomInfeasible {
                variable: Some(var),
                scope: None,
            });
        }
        for entry in cfn.unary(var).unwrap_or_default() {
            if *entry != Cost::TOP_SENTINEL && *entry > bottom {
                return Err(IsoError::NegativeReward {
                    variable: var,
                    scope: None,
                });
            }
        }
        baseline = baseline.combine(bottom, Cost::TOP_SENTINEL);
    }

    for function in cfn.functions() {
        baseline = baseline.combine(verify_function(cfn, function)?, Cost::TOP_SENTINEL);
    }

    if baseline == Cost::TOP_SENTINEL {
        return Err(IsoError::BottomInfeasible {
            variable: None,
            scope: None,
        });
    }
    Ok(baseline)
}

/// Check one binary function and return its `f(⊥,⊥)`.
fn verify_function(cfn: &Cfn, function: &CostFunction) -> Result<Cost, IsoError> {
    let scope = function.scope();
    if function.arity() > 2 {
        return Err(IsoError::UnsupportedArity {
            scope: scope.to_vec(),
        });
    }
    let bottoms = vec![ValId::BOTTOM; scope.len()];
    let bottom = cfn
        .table_index(scope, &bottoms)
        .and_then(|offset| function.table().get(offset).copied())
        .unwrap_or(Cost::TOP_SENTINEL);
    if bottom == Cost::TOP_SENTINEL {
        return Err(IsoError::BottomInfeasible {
            variable: None,
            scope: Some(scope.to_vec()),
        });
    }

    let slots: Vec<usize> = scope
        .iter()
        .map(|var| cfn.variable(*var).map_or(1, Variable::slots))
        .collect();
    for (offset, entry) in function.table().iter().enumerate() {
        if *entry == Cost::TOP_SENTINEL {
            continue;
        }
        let dropped = dropped_positions(offset, &slots);
        if dropped > 0 && dropped < slots.len() && *entry != bottom {
            return Err(IsoError::UnevenDropCharge {
                scope: scope.to_vec(),
            });
        }
        if *entry > bottom {
            return Err(IsoError::NegativeReward {
                variable: scope.first().copied().unwrap_or_default(),
                scope: Some(scope.to_vec()),
            });
        }
    }
    Ok(bottom)
}

/// How many positions of a row-major table offset hold `⊥`.
///
/// `⊥` is the last slot of every variable, so a position holds it exactly when
/// its digit is one below the variable's slot count.
fn dropped_positions(offset: usize, slots: &[usize]) -> usize {
    let mut rest = offset;
    let mut dropped = 0usize;
    for count in slots.iter().rev() {
        if count.checked_sub(1) == Some(rest % count) {
            dropped += 1;
        }
        rest /= count;
    }
    dropped
}

/// Feasibility and the vertex reward of every `(variable, target)` pair.
///
/// The reward is `u(v,⊥) ⊖ u(v,a)` computed with `saturating_sub`, which would
/// silently report zero for a pair costing *more* than dropping. No such pair
/// can reach here: [`Instance::new`] runs `verify_frame` first, and that is
/// exactly the condition it rejects with [`IsoError::NegativeReward`]. The
/// ordering is a precondition of this function rather than an incidental one,
/// since a negative reward read as zero would make the bound inadmissible with
/// nothing to signal it.
fn unary_frame(
    cfn: &Cfn,
    index: &ValueIndex,
    lefts: usize,
    rights: usize,
) -> (Vec<bool>, Vec<u64>) {
    let mut feasible = vec![false; lefts * rights];
    let mut reward = vec![0u64; lefts * rights];
    for var in cfn.variable_ids() {
        let bottom = cfn
            .unary_cost(var, ValId::BOTTOM)
            .unwrap_or(Cost::TOP_SENTINEL);
        let Some(domain) = cfn.domain(var) else {
            continue;
        };
        for value in domain {
            let Some(target) = index.global(var, value) else {
                continue;
            };
            let entry = cfn.unary_cost(var, value).unwrap_or(Cost::TOP_SENTINEL);
            if entry == Cost::TOP_SENTINEL {
                continue;
            }
            let offset = var.index() * rights + target.index();
            feasible[offset] = true;
            reward[offset] = bottom.raw().saturating_sub(entry.raw());
        }
    }
    (feasible, reward)
}

/// Every binary function, with its origin and its largest payable reward.
fn binary_frame(cfn: &Cfn) -> Vec<BinaryFunction<'_>> {
    cfn.functions()
        .iter()
        .filter(|function| function.arity() == 2)
        .filter_map(|function| {
            let scope = function.scope();
            let (low, high) = (*scope.first()?, *scope.get(1)?);
            let high_slots = cfn.variable(high)?.slots();
            let bottoms = [ValId::BOTTOM, ValId::BOTTOM];
            let offset = cfn.table_index(scope, &bottoms)?;
            let bottom = function.table().get(offset).copied()?;
            let max_reward = function
                .table()
                .iter()
                .filter(|entry| **entry != Cost::TOP_SENTINEL)
                .map(|entry| bottom.raw().saturating_sub(entry.raw()))
                .max()
                .unwrap_or(0);
            Some(BinaryFunction {
                low: low.raw(),
                high: high.raw(),
                high_slots,
                bottom,
                table: function.table(),
                max_reward,
            })
        })
        .collect()
}

/// For each variable, the neighbours it shares a binary function with.
fn incidence(lefts: usize, functions: &[BinaryFunction<'_>]) -> Vec<Vec<(u32, u32)>> {
    let mut incident = vec![Vec::new(); lefts];
    for (position, function) in functions.iter().enumerate() {
        let Ok(index) = u32::try_from(position) else {
            continue;
        };
        if let Some(list) = incident.get_mut(function.low as usize) {
            list.push((function.high, index));
        }
        if let Some(list) = incident.get_mut(function.high as usize) {
            list.push((function.low, index));
        }
    }
    incident
}

/// The largest reward any function can pay for one unordered pair of images.
///
/// This is what the `γ` side of the bound charges against, and it has to be
/// indexed by target vertices rather than by source vertices because that side
/// of the proof charges to distinct *right* components.
fn target_pair_rewards(
    cfn: &Cfn,
    index: &ValueIndex,
    functions: &[BinaryFunction<'_>],
    rights: usize,
) -> Vec<u64> {
    let mut pair = vec![0u64; rights * rights];
    for function in functions {
        let low = VarId::new(function.low);
        let high = VarId::new(function.high);
        let (Some(low_domain), Some(high_domain)) = (cfn.domain(low), cfn.domain(high)) else {
            continue;
        };
        for low_value in low_domain {
            let Some(left) = index.global(low, low_value) else {
                continue;
            };
            for high_value in high_domain {
                let Some(right) = index.global(high, high_value) else {
                    continue;
                };
                let Some(entry) = function.entry(cfn, low, low_value, high, high_value) else {
                    continue;
                };
                let reward = function.bottom.raw().saturating_sub(entry.raw());
                let (first, second) = if left <= right {
                    (left.index(), right.index())
                } else {
                    (right.index(), left.index())
                };
                pair[first * rights + second] = pair[first * rights + second].max(reward);
                pair[second * rights + first] = pair[first * rights + second];
            }
        }
    }
    pair
}

impl BinaryFunction<'_> {
    /// The entry for one pair of values, or `None` when the function forbids
    /// it.
    fn entry(
        &self,
        cfn: &Cfn,
        low: VarId,
        low_value: ValId,
        high: VarId,
        high_value: ValId,
    ) -> Option<Cost> {
        let low_slot = cfn.variable(low)?.slot(low_value)?;
        let high_slot = cfn.variable(high)?.slot(high_value)?;
        let entry = *self.table.get(low_slot * self.high_slots + high_slot)?;
        if entry == Cost::TOP_SENTINEL {
            None
        } else {
            Some(entry)
        }
    }
}

// ---------------------------------------------------------------------------
// The search
// ---------------------------------------------------------------------------

/// One label class: the source vertices carrying it and the target vertices
/// carrying it, both ascending.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Bidomain {
    left: Vec<u32>,
    right: Vec<u32>,
}

/// The bound of one label class, and the `ρ` it was computed from.
struct ClassBound {
    /// `ρ_l(v)` for each member of the class's left side, in the same order.
    rho: Vec<u64>,
    /// `B_l`.
    value: u64,
}

/// The bound at one node, class by class.
struct Plan {
    bound: u64,
    classes: Vec<ClassBound>,
}

/// The mutable half of the search.
struct Search<'a> {
    instance: &'a Instance<'a>,
    /// The image of each variable under the current mapping.
    image: Vec<Option<u32>>,
    /// The mapping, in the order it was built, each pair with the increment it
    /// contributed. The increment is stored rather than recomputed because it
    /// was measured against the mapping as it stood then.
    mapping: Vec<(u32, u32, u64)>,
    /// `R(M)` for the current mapping, exactly.
    reward: u64,
    /// The reward of the incumbent.
    best_reward: u64,
    /// The incumbent, always feasible.
    best: Assignment,
    /// The incumbent's cost, from [`Cfn::evaluate`].
    best_cost: Cost,
    nodes: u64,
    max_nodes: u64,
    deadline: Option<Instant>,
    limit_hit: Option<LimitKind>,
}

impl<'a> Search<'a> {
    /// The search before anything has been mapped, with the all-`⊥` assignment
    /// as its incumbent.
    ///
    /// That assignment is feasible by the reward frame's first precondition, so
    /// the search always has an answer and never has to report that it found
    /// none.
    fn new(instance: &'a Instance<'a>, budget: &SearchBudget) -> Self {
        let best = Assignment::all_bottom(instance.lefts);
        let best_cost = instance.cfn.evaluate(&best);
        Self {
            instance,
            image: vec![None; instance.lefts],
            mapping: Vec::new(),
            reward: 0,
            best_reward: 0,
            best,
            best_cost,
            nodes: 0,
            max_nodes: budget.max_nodes.unwrap_or(DEFAULT_SEARCH_NODES),
            deadline: budget
                .max_millis
                .map(|millis| Instant::now() + std::time::Duration::from_millis(millis)),
            limit_hit: None,
        }
    }

    /// The exact reward increment of appending `(left, right)`, or `None` when
    /// the pair is infeasible.
    fn delta(&self, left: u32, right: u32) -> Option<u64> {
        let instance = self.instance;
        if !instance.can_map(left, right) {
            return None;
        }
        let mut total = instance.unary_reward[left as usize * instance.rights + right as usize];
        for &(other, function) in &instance.incident[left as usize] {
            let Some(image) = self.image[other as usize] else {
                continue;
            };
            let function = &instance.functions[function as usize];
            let entry = self.pair_entry(function, left, right, other, image)?;
            total = total.saturating_add(function.bottom.raw().saturating_sub(entry.raw()));
        }
        Some(total)
    }

    /// The entry a binary function takes on two `(variable, target)` pairs, or
    /// `None` when it forbids them.
    fn pair_entry(
        &self,
        function: &BinaryFunction<'_>,
        first: u32,
        first_image: u32,
        second: u32,
        second_image: u32,
    ) -> Option<Cost> {
        let (low, low_image, high, high_image) = if first == function.low {
            (first, first_image, second, second_image)
        } else {
            (second, second_image, first, first_image)
        };
        let low = VarId::new(low);
        let high = VarId::new(high);
        let index = &self.instance.index;
        let low_value = index.local(low, TargetId(low_image))?;
        let high_value = index.local(high, TargetId(high_image))?;
        function.entry(self.instance.cfn, low, low_value, high, high_value)
    }

    /// Append a pair to the mapping.
    fn push(&mut self, left: u32, right: u32, delta: u64) {
        self.image[left as usize] = Some(right);
        self.mapping.push((left, right, delta));
        self.reward = self.reward.saturating_add(delta);
    }

    /// Undo the last [`Self::push`].
    fn pop(&mut self) {
        if let Some((left, _, delta)) = self.mapping.pop() {
            self.image[left as usize] = None;
            self.reward = self.reward.saturating_sub(delta);
        }
    }

    /// The assignment the current mapping stands for.
    fn assignment(&self) -> Assignment {
        let mut assignment = Assignment::all_bottom(self.instance.lefts);
        for (left, image) in self.image.iter().enumerate() {
            let (Some(target), Ok(raw)) = (image, u32::try_from(left)) else {
                continue;
            };
            let var = VarId::new(raw);
            if let Some(value) = self.instance.index.local(var, TargetId(*target)) {
                assignment.set(var, value);
            }
        }
        assignment
    }

    /// Take the current mapping as the incumbent when it is feasible and beats
    /// the one held.
    ///
    /// The cost is read from [`Cfn::evaluate`] against the pristine network, so
    /// the outcome's upper bound is a real assignment's real cost rather than a
    /// restatement of the search's own arithmetic. An infeasible mapping is
    /// passed over rather than rejected: the constraints that can make one
    /// infeasible are implications that mapping more vertices can satisfy.
    fn record(&mut self) {
        let assignment = self.assignment();
        let cost = self.instance.cfn.evaluate(&assignment);
        if cost == Cost::TOP_SENTINEL || cost >= self.best_cost {
            return;
        }
        debug_assert_eq!(
            cost.raw(),
            self.instance.baseline.raw().saturating_sub(self.reward),
            "the incremental reward must be the network's cost measured from the baseline"
        );
        self.best = assignment;
        self.best_cost = cost;
        self.best_reward = self.reward;
    }

    /// Whether a limit has stopped the search.
    fn stopped(&mut self) -> bool {
        if self.limit_hit.is_some() {
            return true;
        }
        if self.nodes >= self.max_nodes {
            self.limit_hit = Some(LimitKind::Nodes);
            return true;
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.limit_hit = Some(LimitKind::Time);
            return true;
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Bound B1
// ---------------------------------------------------------------------------

impl Search<'_> {
    /// The bound at this node, and the per-class `ρ` the vertex heuristic
    /// reads.
    fn bound(&self, future: &[Bidomain]) -> Plan {
        let (live_left, live_right) = liveness(self.instance, future);
        let mut bound = self.reward;
        let mut classes = Vec::with_capacity(future.len());

        for class in future {
            let mut rho = Vec::with_capacity(class.left.len());
            for &left in &class.left {
                let best = class
                    .right
                    .iter()
                    .filter_map(|&right| self.delta(left, right))
                    .max();
                rho.push(best.map_or(0, |delta| {
                    delta.saturating_add(self.half_charge_left(left, &live_left))
                }));
            }
            let mut gamma = Vec::with_capacity(class.right.len());
            for &right in &class.right {
                let best = class
                    .left
                    .iter()
                    .filter_map(|&left| self.delta(left, right))
                    .max();
                gamma.push(best.map_or(0, |delta| {
                    delta.saturating_add(self.half_charge_right(right, &live_right))
                }));
            }

            let capacity = class.left.len().min(class.right.len());
            let value = topsum(rho.clone(), capacity).min(topsum(gamma, capacity));
            bound = bound.saturating_add(value);
            classes.push(ClassBound { rho, value });
        }

        Plan { bound, classes }
    }

    /// `h_G(v)`: half of every reward a still selectable neighbour could pay.
    ///
    /// Rounded up, because two halves that each rounded down could sum to less
    /// than the whole and the charge would stop covering the arc.
    ///
    /// The halving is arithmetic on the raw packed cost, whose low field is the
    /// drop count, so half of an odd quality unit lands in that field rather
    /// than in the quality one. The rounding is upward in every case, so the
    /// bound only ever loosens and admissibility is untouched; the cost is that
    /// a bound can be a few drop-count units above the tightest one expressible.
    /// Splitting the fields to halve each separately would tighten it, at one
    /// unpack and repack per incident function per candidate.
    fn half_charge_left(&self, left: u32, live_left: &[bool]) -> u64 {
        let mut total = 0u64;
        for &(other, function) in &self.instance.incident[left as usize] {
            if other != left && live_left[other as usize] {
                total = total.saturating_add(self.instance.functions[function as usize].max_reward);
            }
        }
        total.div_ceil(2)
    }

    /// `h_H(w)`: the mirror image, charged to target vertices.
    fn half_charge_right(&self, right: u32, live_right: &[bool]) -> u64 {
        let rights = self.instance.rights;
        let row = right as usize * rights;
        let mut total = 0u64;
        for (other, live) in live_right.iter().enumerate() {
            if *live && other != right as usize {
                total = total.saturating_add(self.instance.pair_h[row + other]);
            }
        }
        total.div_ceil(2)
    }
}

/// Which source and target vertices a node can still select.
///
/// A vertex a node has dropped can never be mapped below it, so charging for
/// arcs to it would only loosen the bound.
fn liveness(instance: &Instance<'_>, future: &[Bidomain]) -> (Vec<bool>, Vec<bool>) {
    let mut left = vec![false; instance.lefts];
    let mut right = vec![false; instance.rights];
    for class in future {
        for &member in &class.left {
            left[member as usize] = true;
        }
        for &member in &class.right {
            right[member as usize] = true;
        }
    }
    (left, right)
}

/// The sum of the `k` largest values.
fn topsum(mut values: Vec<u64>, k: usize) -> u64 {
    values.sort_unstable_by(|left, right| right.cmp(left));
    values.into_iter().take(k).fold(0u64, u64::saturating_add)
}

// ---------------------------------------------------------------------------
// Branching
// ---------------------------------------------------------------------------

impl Search<'_> {
    /// The label class to branch on: smallest `max(|G_l|,|H_l|)`, then largest
    /// `B_l`, then lowest class position.
    ///
    /// The first is the paper's own rule, which it reads as smallest-domain-
    /// first taken simultaneously in both viewpoints. The second is the
    /// weighted addition: among classes of the same size, branch where the most
    /// reward is at stake. The third is only there to make the choice total.
    fn select_class(future: &[Bidomain], plan: &Plan) -> Option<usize> {
        let mut chosen: Option<(usize, usize, u64)> = None;
        for (position, class) in future.iter().enumerate() {
            let size = class.left.len().max(class.right.len());
            let stake = plan.classes.get(position).map_or(0, |bound| bound.value);
            let better = chosen.is_none_or(|(_, best_size, best_stake)| {
                size < best_size || (size == best_size && stake > best_stake)
            });
            if better {
                chosen = Some((position, size, stake));
            }
        }
        chosen.map(|(position, _, _)| position)
    }

    /// The vertex to branch on: largest `ρ_l(v)`, then largest degree, then
    /// lowest identifier.
    fn select_vertex(class: &Bidomain, bound: &ClassBound, instance: &Instance<'_>) -> Option<u32> {
        let mut chosen: Option<(u32, u64, u32)> = None;
        for (position, &left) in class.left.iter().enumerate() {
            let rho = bound.rho.get(position).copied().unwrap_or(0);
            let degree = instance.degree.get(left as usize).copied().unwrap_or(0);
            let better = chosen.is_none_or(|(_, best_rho, best_degree)| {
                rho > best_rho || (rho == best_rho && degree > best_degree)
            });
            if better {
                chosen = Some((left, rho, degree));
            }
        }
        chosen.map(|(left, _, _)| left)
    }

    /// The values to try, in descending increment then ascending target order.
    fn values(&self, class: &Bidomain, left: u32) -> Vec<(u64, u32)> {
        let mut values: Vec<(u64, u32)> = class
            .right
            .iter()
            .filter_map(|&right| self.delta(left, right).map(|delta| (delta, right)))
            .collect();
        values
            .sort_unstable_by(|first, second| second.0.cmp(&first.0).then(first.1.cmp(&second.1)));
        values
    }

    /// One node of the search.
    ///
    /// The last block is the paper's "leave this vertex unmatched" branch, and
    /// it is what makes the answer a maximum rather than a maximal common
    /// sub-schema. Dropping it would still return a correct span, just not the
    /// best one.
    fn search(&mut self, future: &[Bidomain]) {
        if self.stopped() {
            return;
        }
        self.nodes += 1;

        if self.reward > self.best_reward {
            self.record();
        }

        let plan = self.bound(future);
        debug_assert!(
            plan.bound >= self.reward,
            "the bound must never fall below the reward already achieved"
        );
        if plan.bound <= self.best_reward {
            return;
        }

        let Some(position) = Self::select_class(future, &plan) else {
            return;
        };
        let (Some(class), Some(bound)) = (future.get(position), plan.classes.get(position)) else {
            return;
        };
        let Some(left) = Self::select_vertex(class, bound, self.instance) else {
            return;
        };

        for (delta, right) in self.values(class, left) {
            let refined = self.instance.refine(future, left, right);
            self.push(left, right, delta);
            self.search(&refined);
            self.pop();
            if self.limit_hit.is_some() {
                return;
            }
        }

        let mut dropped = future.to_vec();
        if let Some(class) = dropped.get_mut(position) {
            class.left.retain(|&member| member != left);
            if class.left.is_empty() {
                dropped.remove(position);
            }
        }
        self.search(&dropped);
    }

    /// Greedy descent, to raise the incumbent before the search proper starts.
    ///
    /// This is what makes the weighted search tractable, and it is the
    /// substitute for the paper's top-down variant: rather than iterating goal
    /// *sizes*, which is more than an order of magnitude slower when the
    /// optimum does not cover most of the smaller graph, seed a good incumbent
    /// and let the bound prune against it. Most real schema pairs have no total
    /// morphism at all, so the optimum rarely covers most of either side.
    fn warm_start(&mut self, future: &[Bidomain]) {
        let mut current = future.to_vec();
        while let Some((delta, left, right)) = self.greedy_step(&current) {
            current = self.instance.refine(&current, left, right);
            self.push(left, right, delta);
            self.record();
        }
        while !self.mapping.is_empty() {
            self.pop();
        }
        debug_assert_eq!(self.reward, 0, "the warm start must leave no trace");
    }

    /// The best feasible pair anywhere in the current classes.
    fn greedy_step(&self, future: &[Bidomain]) -> Option<(u64, u32, u32)> {
        let mut chosen: Option<(u64, u32, u32)> = None;
        for class in future {
            for &left in &class.left {
                for &right in &class.right {
                    let Some(delta) = self.delta(left, right) else {
                        continue;
                    };
                    if chosen.is_none_or(|(best, _, _)| delta > best) {
                        chosen = Some((delta, left, right));
                    }
                }
            }
        }
        chosen
    }
}

// ---------------------------------------------------------------------------
// The entry point
// ---------------------------------------------------------------------------

/// The maximum common induced sub-schema of a source and a target, weighted by
/// a cost function network.
///
/// The answer minimises the network's cost over the mappings that are injective
/// **and** structure-reflecting, so the span it stands for has two monic legs
/// and an edge map that is a bijection onto the induced target arcs. That is
/// the object an overlap and a symmetric lens need. It is **not** the object
/// `monic` alone asks for: see the module docs.
///
/// `cfn` must have been built over `src` and `tgt`: it carries the rewards and
/// they carry the arcs, and neither is enough alone.
///
/// The outcome always has a `best`, since dropping every source vertex is a
/// mapping and the reward frame requires it to be feasible.
/// [`SolveOutcome::elimination_order`] is `None`: this path has no elimination
/// order, and the choice among equally good answers is the first the
/// deterministic branching order reaches rather than a lexicographic minimum.
///
/// # Errors
///
/// [`IsoError`] when the network breaks one of the three reward-frame
/// preconditions of the module docs, or names a vertex the schema pair does not
/// hold. No network [`build_cfn`](super::build::build_cfn) produces does either.
///
/// # Examples
///
/// ```
/// use panproto_mig::solve::build::{NoEvidence, build_cfn};
/// use panproto_mig::solve::mcsplit::solve_iso;
/// use panproto_mig::{DEFAULT_WEIGHTS, DomainConstraints, SearchBudget, SearchOptions};
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
/// let outcome = solve_iso(&cfn, &schema, &schema, &SearchBudget::default())?;
/// let best = outcome.best.as_ref().ok_or("no answer")?;
///
/// // A schema is its own maximum common induced sub-schema, so nothing drops.
/// assert_eq!(best.dropped(), 0);
/// assert!(outcome.proven_optimal);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn solve_iso(
    cfn: &Cfn,
    src: &Schema,
    tgt: &Schema,
    budget: &SearchBudget,
) -> Result<SolveOutcome, IsoError> {
    let instance = Instance::new(cfn, src, tgt)?;
    let root = instance.root();

    let mut search = Search::new(&instance, budget);
    search.warm_start(&root);
    let root_bound = search.bound(&root).bound;
    search.search(&root);

    let upper_bound = search.best_cost;
    let proven_optimal = search.limit_hit.is_none();
    let lower_bound = if proven_optimal {
        upper_bound
    } else {
        Cost::from_raw(instance.baseline.raw().saturating_sub(root_bound))
    };

    Ok(SolveOutcome {
        best: Some(search.best),
        lower_bound,
        upper_bound,
        proven_optimal,
        path: SolverPath::Iso,
        elimination_order: None,
        nodes: search.nodes,
        limit_hit: search.limit_hit,
        warnings: Vec::new(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::solve::build::{NoEvidence, build_cfn};
    use crate::solve::cfn::CfnBuilder;
    use crate::solve::cost::DEFAULT_WEIGHTS;
    use crate::{DomainConstraints, SearchOptions};
    use panproto_schema::{Protocol, SchemaBuilder};

    // -- fixtures ----------------------------------------------------------

    fn protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn schema(vertices: &[(&str, &str)], edges: &[(&str, &str, &str, Option<&str>)]) -> Schema {
        let protocol = protocol();
        let mut builder = SchemaBuilder::new(&protocol);
        for (id, kind) in vertices {
            builder = builder.vertex(id, kind, None::<&str>).unwrap();
        }
        for (from, to, kind, name) in edges {
            builder = builder.edge(from, to, kind, *name).unwrap();
        }
        builder.build().unwrap()
    }

    fn network(src: &Schema, tgt: &Schema) -> Cfn {
        build_cfn(
            src,
            tgt,
            &SearchOptions::default(),
            &DomainConstraints::default(),
            &NoEvidence,
            DEFAULT_WEIGHTS,
        )
        .unwrap()
    }

    /// The mapping an outcome stands for, as `(source, target)` name pairs.
    fn mapping(cfn: &Cfn, outcome: &SolveOutcome) -> Vec<(String, String)> {
        let best = outcome.best.as_ref().unwrap();
        let mut pairs: Vec<(String, String)> = best
            .pairs()
            .filter_map(|(var, value)| {
                let variable = cfn.variable(var)?;
                let target = variable.value_name(value)?;
                Some((variable.name().to_string(), target.to_string()))
            })
            .collect();
        pairs.sort();
        pairs
    }

    /// Every assignment that is injective and structure-reflecting, with its
    /// cost, found by direct enumeration.
    ///
    /// Written against the schemas rather than against anything the search
    /// builds: it compares descriptors as multisets, with no digest, no
    /// interning and no label class.
    fn iso_optimum(cfn: &Cfn, src: &Schema, tgt: &Schema) -> (Cost, Vec<Assignment>) {
        let mut best = Cost::TOP_SENTINEL;
        let mut argmins = Vec::new();
        for assignment in every_assignment(cfn) {
            if !is_iso(cfn, src, tgt, &assignment) {
                continue;
            }
            let cost = cfn.evaluate(&assignment);
            if cost == Cost::TOP_SENTINEL {
                continue;
            }
            if cost < best {
                best = cost;
                argmins.clear();
            }
            if cost == best {
                argmins.push(assignment);
            }
        }
        (best, argmins)
    }

    fn every_assignment(cfn: &Cfn) -> Vec<Assignment> {
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

    fn is_iso(cfn: &Cfn, src: &Schema, tgt: &Schema, assignment: &Assignment) -> bool {
        let mut pairs: Vec<(Name, Name)> = Vec::new();
        for (var, value) in assignment.pairs() {
            let Some(variable) = cfn.variable(var) else {
                return false;
            };
            let Some(target) = variable.value_name(value) else {
                continue;
            };
            pairs.push((variable.name().clone(), target.clone()));
        }
        let mut images: Vec<&Name> = pairs.iter().map(|(_, target)| target).collect();
        images.sort();
        let before = images.len();
        images.dedup();
        if images.len() != before {
            return false;
        }
        for (first_src, first_tgt) in &pairs {
            for (second_src, second_tgt) in &pairs {
                if arc_descriptor(src, first_src, second_src)
                    != arc_descriptor(tgt, first_tgt, second_tgt)
                {
                    return false;
                }
            }
        }
        true
    }

    // -- arc descriptors ---------------------------------------------------

    #[test]
    fn a_descriptor_ignores_the_order_its_arcs_arrive_in() {
        let forward = ArcDescriptor::from_arcs(vec![
            (Dir::Out, Name::from("prop")),
            (Dir::In, Name::from("item")),
            (Dir::Out, Name::from("item")),
        ]);
        let backward = ArcDescriptor::from_arcs(vec![
            (Dir::Out, Name::from("item")),
            (Dir::Out, Name::from("prop")),
            (Dir::In, Name::from("item")),
        ]);
        assert_eq!(forward, backward);
        assert_eq!(forward.digest(), backward.digest());
    }

    #[test]
    fn a_descriptor_separates_direction_and_kind() {
        let out = ArcDescriptor::from_arcs(vec![(Dir::Out, Name::from("prop"))]);
        let into = ArcDescriptor::from_arcs(vec![(Dir::In, Name::from("prop"))]);
        let loops = ArcDescriptor::from_arcs(vec![(Dir::Loop, Name::from("prop"))]);
        let other = ArcDescriptor::from_arcs(vec![(Dir::Out, Name::from("item"))]);
        for (first, second) in [
            (&out, &into),
            (&out, &loops),
            (&into, &loops),
            (&out, &other),
        ] {
            assert_ne!(first, second);
            assert_ne!(first.digest(), second.digest());
        }
    }

    #[test]
    fn a_descriptor_counts_multiplicity() {
        let one = ArcDescriptor::from_arcs(vec![(Dir::Out, Name::from("prop"))]);
        let two = ArcDescriptor::from_arcs(vec![
            (Dir::Out, Name::from("prop")),
            (Dir::Out, Name::from("prop")),
        ]);
        assert_ne!(one.digest(), two.digest());
        assert_eq!(two.len(), 2);
        assert!(!two.is_empty());
    }

    #[test]
    fn a_kind_boundary_cannot_be_run_together_into_another() {
        // Without the separator byte, `("ab", "c")` and `("a", "bc")` would
        // digest alike.
        let first = ArcDescriptor::from_arcs(vec![
            (Dir::Out, Name::from("ab")),
            (Dir::Out, Name::from("c")),
        ]);
        let second = ArcDescriptor::from_arcs(vec![
            (Dir::Out, Name::from("a")),
            (Dir::Out, Name::from("bc")),
        ]);
        assert_ne!(first.digest(), second.digest());
    }

    #[test]
    fn a_schema_descriptor_reverses_with_its_pair() {
        let s = schema(
            &[("root", "object"), ("leaf", "string")],
            &[("root", "leaf", "prop", Some("a"))],
        );
        let forward = arc_descriptor(&s, &Name::from("root"), &Name::from("leaf"));
        let backward = arc_descriptor(&s, &Name::from("leaf"), &Name::from("root"));
        assert_eq!(forward.entries(), [(Dir::Out, Name::from("prop"))]);
        assert_eq!(backward.entries(), [(Dir::In, Name::from("prop"))]);
    }

    #[test]
    fn a_self_loop_is_the_whole_descriptor_of_a_vertex_against_itself() {
        let s = schema(
            &[("root", "object")],
            &[("root", "root", "prop", Some("self"))],
        );
        let loops = arc_descriptor(&s, &Name::from("root"), &Name::from("root"));
        assert_eq!(loops.entries(), [(Dir::Loop, Name::from("prop"))]);
    }

    #[test]
    fn a_vertex_with_no_arcs_has_an_empty_descriptor() {
        let s = schema(&[("a", "object"), ("b", "object")], &[]);
        assert!(arc_descriptor(&s, &Name::from("a"), &Name::from("b")).is_empty());
        assert!(arc_descriptor(&s, &Name::from("a"), &Name::from("absent")).is_empty());
    }

    // -- the value index ---------------------------------------------------

    #[test]
    fn the_value_index_round_trips_between_the_two_numberings() {
        let src = schema(&[("a", "object"), ("b", "string")], &[]);
        let tgt = schema(&[("x", "object"), ("y", "string"), ("z", "object")], &[]);
        let cfn = network(&src, &tgt);
        let index = ValueIndex::of(&cfn);

        assert_eq!(index.len(), 3);
        for var in cfn.variable_ids() {
            let domain = cfn.domain(var).unwrap();
            for value in domain {
                let Some(target) = index.global(var, value) else {
                    assert!(value.is_bottom());
                    continue;
                };
                assert_eq!(index.local(var, target), Some(value));
                assert_eq!(
                    index.name(target),
                    cfn.variable(var).unwrap().value_name(value)
                );
            }
        }
    }

    #[test]
    fn the_same_value_identifier_means_different_targets_for_different_variables() {
        // `a` is an object and `b` a string, so slot zero is `x` for one and
        // `y` for the other. This is the confusion `TargetId` exists to stop.
        let src = schema(&[("a", "object"), ("b", "string")], &[]);
        let tgt = schema(&[("x", "object"), ("y", "string")], &[]);
        let cfn = network(&src, &tgt);
        let index = ValueIndex::of(&cfn);
        let first = index.global(VarId::new(0), ValId::real(0)).unwrap();
        let second = index.global(VarId::new(1), ValId::real(0)).unwrap();
        assert_ne!(first, second);
    }

    // -- the counting Hall propagator --------------------------------------

    /// A network whose variables all take from one pool of targets, with the
    /// domains given by name.
    fn pool(domains: &[&[&str]]) -> (Cfn, ValueIndex) {
        let spec: Vec<(Name, Vec<Name>)> = domains
            .iter()
            .enumerate()
            .map(|(position, values)| {
                (
                    Name::from(format!("v{position}")),
                    values.iter().map(|value| Name::from(*value)).collect(),
                )
            })
            .collect();
        let cfn = CfnBuilder::new(spec, DEFAULT_WEIGHTS).unwrap().build();
        let index = ValueIndex::of(&cfn);
        (cfn, index)
    }

    /// The domains of a network with `⊥` removed, which is the total-morphism
    /// restriction and the regime the propagator is for.
    fn total_domains(cfn: &Cfn) -> Vec<Domain> {
        cfn.variable_ids()
            .map(|var| {
                let mut domain = cfn.domain(var).unwrap();
                domain.remove(ValId::BOTTOM);
                domain
            })
            .collect()
    }

    #[test]
    fn the_hall_propagator_finds_a_pigeonhole() {
        let (cfn, index) = pool(&[&["x", "y"], &["x", "y"], &["x", "y"]]);
        let mut domains = total_domains(&cfn);
        assert_eq!(
            propagate_all_different(&index, &mut domains),
            HallOutcome::Wipeout
        );
    }

    #[test]
    fn the_hall_propagator_prunes_a_hall_set_from_everyone_else() {
        let (cfn, index) = pool(&[&["x", "y"], &["x", "y"], &["x", "y", "z"]]);
        let mut domains = total_domains(&cfn);
        assert!(matches!(
            propagate_all_different(&index, &mut domains),
            HallOutcome::Filtered { removed: 2 }
        ));
        // `{x, y}` is saturated by the first two, so the third keeps only `z`.
        let third = domains[2];
        assert_eq!(third.len(), 1);
        let only = third.only().unwrap();
        assert_eq!(
            index.name(index.global(VarId::new(2), only).unwrap()),
            Some(&Name::from("z"))
        );
    }

    #[test]
    fn the_hall_propagator_chains_through_singletons() {
        let (cfn, index) = pool(&[&["x"], &["x", "y"], &["x", "y", "z"]]);
        let mut domains = total_domains(&cfn);
        assert!(matches!(
            propagate_all_different(&index, &mut domains),
            HallOutcome::Filtered { .. }
        ));
        for (position, expected) in ["x", "y", "z"].iter().enumerate() {
            let var = VarId::new(u32::try_from(position).unwrap());
            let only = domains[position].only().unwrap();
            assert_eq!(
                index.name(index.global(var, only).unwrap()),
                Some(&Name::from(*expected))
            );
        }
    }

    #[test]
    fn a_variable_that_may_be_dropped_is_never_counted() {
        // The same pigeonhole, but every variable keeps `⊥`, so nothing is
        // forced and the propagator must stay silent. This is the span search.
        let (cfn, index) = pool(&[&["x", "y"], &["x", "y"], &["x", "y"]]);
        let mut domains: Vec<Domain> = cfn
            .variable_ids()
            .map(|var| cfn.domain(var).unwrap())
            .collect();
        let before = domains.clone();
        assert_eq!(
            propagate_all_different(&index, &mut domains),
            HallOutcome::Filtered { removed: 0 }
        );
        assert_eq!(domains, before);
    }

    #[test]
    fn a_hall_set_prunes_a_variable_that_may_be_dropped() {
        // Two forced variables saturate `{x, y}`, so the optional third loses
        // them even though it was never counted.
        let (cfn, index) = pool(&[&["x", "y"], &["x", "y"], &["x", "y", "z"]]);
        let mut domains = total_domains(&cfn);
        domains[2].insert(ValId::BOTTOM);
        assert!(matches!(
            propagate_all_different(&index, &mut domains),
            HallOutcome::Filtered { removed: 2 }
        ));
        assert!(domains[2].contains(ValId::BOTTOM));
        assert_eq!(domains[2].len(), 2);
    }

    #[test]
    fn the_hall_propagator_keeps_every_injective_assignment() {
        let (cfn, index) = pool(&[&["x", "y"], &["x", "y", "z"], &["x", "y", "z"]]);
        let before = total_domains(&cfn);
        let mut domains = before.clone();
        let outcome = propagate_all_different(&index, &mut domains);

        let mut injective = 0usize;
        for assignment in exhaustive(&before) {
            let mut targets: Vec<TargetId> = assignment
                .iter()
                .enumerate()
                .filter_map(|(position, value)| {
                    index.global(VarId::new(u32::try_from(position).ok()?), *value)
                })
                .collect();
            targets.sort_unstable();
            let distinct = targets.len();
            targets.dedup();
            if targets.len() != distinct {
                continue;
            }
            injective += 1;
            for (position, value) in assignment.iter().enumerate() {
                assert!(
                    domains[position].contains(*value),
                    "propagation dropped a value an injective assignment uses"
                );
            }
        }
        assert!(injective > 0, "the fixture must have injective assignments");
        assert!(matches!(outcome, HallOutcome::Filtered { .. }));
        for (after, original) in domains.iter().zip(&before) {
            assert_eq!(
                after.bits() & !original.bits(),
                0,
                "propagation only prunes"
            );
        }
    }

    /// Every combination of one value per domain.
    fn exhaustive(domains: &[Domain]) -> Vec<Vec<ValId>> {
        let mut out = vec![Vec::new()];
        for domain in domains {
            let mut next = Vec::new();
            for partial in &out {
                for value in *domain {
                    let mut extended = partial.clone();
                    extended.push(value);
                    next.push(extended);
                }
            }
            out = next;
        }
        out
    }

    // -- surjectivity ------------------------------------------------------

    #[test]
    fn surjectivity_needs_every_target_covered() {
        let src = schema(&[("a", "object"), ("b", "object")], &[]);
        let tgt = schema(&[("x", "object"), ("y", "object")], &[]);
        let cfn = network(&src, &tgt);
        let index = ValueIndex::of(&cfn);

        let mut both = Assignment::all_bottom(2);
        both.set(VarId::new(0), ValId::real(0));
        both.set(VarId::new(1), ValId::real(1));
        assert!(epic_satisfied(&index, &both, 2));

        let mut one = Assignment::all_bottom(2);
        one.set(VarId::new(0), ValId::real(0));
        assert!(!epic_satisfied(&index, &one, 2));

        // A target no variable can take is one no assignment can cover.
        assert!(!epic_satisfied(&index, &both, 3));
    }

    // -- the bound ---------------------------------------------------------

    /// A network over `src` and `tgt` with unit vertex reward and no edge
    /// reward, which is the unweighted case the paper's own bound covers.
    fn unweighted(src: &Schema, tgt: &Schema) -> Cfn {
        let spec: Vec<(Name, Vec<Name>)> = {
            let mut names: Vec<Name> = src.vertices.keys().cloned().collect();
            names.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
            names
                .into_iter()
                .map(|name| {
                    let kind = &src.vertices[&name].kind;
                    let values: Vec<Name> = tgt
                        .vertices
                        .values()
                        .filter(|vertex| &vertex.kind == kind)
                        .map(|vertex| vertex.id.clone())
                        .collect();
                    (name, values)
                })
                .collect()
        };
        let mut builder = CfnBuilder::new(spec, DEFAULT_WEIGHTS).unwrap();
        for position in 0..u32::try_from(builder.n_variables()).unwrap() {
            let var = VarId::new(position);
            let slots = builder.variable(var).unwrap().slots();
            let mut table = vec![Cost::BOT; slots];
            table[slots - 1] = Cost::from_raw(1);
            builder.add_unary_table(var, &table).unwrap();
        }
        builder.build()
    }

    #[test]
    fn the_bound_specialises_to_the_unweighted_one() {
        let src = schema(
            &[("a", "object"), ("b", "object"), ("c", "string")],
            &[("a", "b", "prop", Some("p"))],
        );
        let tgt = schema(
            &[("x", "object"), ("y", "object"), ("z", "string")],
            &[("x", "y", "prop", Some("p"))],
        );
        let cfn = unweighted(&src, &tgt);
        let instance = Instance::new(&cfn, &src, &tgt).unwrap();
        let root = instance.root();
        let search = Search::new(&instance, &SearchBudget::default());
        let plan = search.bound(&root);

        // With `w_V ≡ 1` and `w_E ≡ 0` every reward is one drop unit, so the
        // bound is the paper's `|M| + Σ_l min(|G_l|, |H_l|)`.
        let expected: usize = root
            .iter()
            .map(|class| class.left.len().min(class.right.len()))
            .sum();
        assert_eq!(plan.bound, u64::try_from(expected).unwrap());
        assert_eq!(expected, 3, "two objects and one string align");
    }

    #[test]
    fn the_bound_covers_the_true_optimum() {
        let src = schema(
            &[("a", "object"), ("b", "object"), ("c", "string")],
            &[("a", "b", "prop", Some("p")), ("b", "c", "prop", Some("q"))],
        );
        let tgt = schema(
            &[("x", "object"), ("y", "object"), ("w", "string")],
            &[("x", "y", "prop", Some("p")), ("y", "w", "prop", Some("r"))],
        );
        let cfn = network(&src, &tgt);
        let instance = Instance::new(&cfn, &src, &tgt).unwrap();
        let root = instance.root();
        let search = Search::new(&instance, &SearchBudget::default());
        let root_bound = search.bound(&root).bound;

        let (best, argmins) = iso_optimum(&cfn, &src, &tgt);
        assert!(!argmins.is_empty());
        let optimal_reward = instance.baseline.raw() - best.raw();
        assert!(
            root_bound >= optimal_reward,
            "the root bound {root_bound} must cover the optimal reward {optimal_reward}"
        );
    }

    // -- the search --------------------------------------------------------

    #[test]
    fn a_schema_is_its_own_maximum_common_sub_schema() {
        let s = schema(
            &[("root", "object"), ("leaf", "string")],
            &[("root", "leaf", "prop", Some("label"))],
        );
        let cfn = network(&s, &s);
        let outcome = solve_iso(&cfn, &s, &s, &SearchBudget::default()).unwrap();
        assert_eq!(
            mapping(&cfn, &outcome),
            vec![
                ("leaf".to_owned(), "leaf".to_owned()),
                ("root".to_owned(), "root".to_owned()),
            ]
        );
        assert!(outcome.proven_optimal);
        assert_eq!(outcome.lower_bound, outcome.upper_bound);
        assert_eq!(outcome.path, SolverPath::Iso);
        assert!(outcome.elimination_order.is_none());
    }

    #[test]
    fn a_denser_target_is_refused_rather_than_matched() {
        // The source has one arc between two objects; the target has two, of
        // different kinds. An injective morphism exists, and the maximum
        // common *induced* sub-schema does not contain both vertices, because
        // the arc multisets differ. This is the distinction that is easy to
        // get backwards.
        let src = schema(
            &[("a", "object"), ("b", "object")],
            &[("a", "b", "prop", Some("p"))],
        );
        let tgt = schema(
            &[("x", "object"), ("y", "object")],
            &[
                ("x", "y", "prop", Some("p")),
                ("x", "y", "item", Some("extra")),
            ],
        );
        let cfn = network(&src, &tgt);
        let outcome = solve_iso(&cfn, &src, &tgt, &SearchBudget::default()).unwrap();
        assert_eq!(
            mapping(&cfn, &outcome).len(),
            1,
            "only one of the two source vertices can be in the apex"
        );

        let (best, argmins) = iso_optimum(&cfn, &src, &tgt);
        assert_eq!(outcome.upper_bound, best);
        assert!(argmins.contains(outcome.best.as_ref().unwrap()));
    }

    #[test]
    fn the_answer_is_a_maximum_rather_than_a_maximal_common_sub_schema() {
        // A greedy descent that takes the best single pair first strands
        // itself: `a` matches `x` best by name, but mapping it forbids the
        // two-vertex apex the objective actually wants.
        let src = schema(
            &[("a", "object"), ("b", "object")],
            &[("a", "b", "prop", Some("p"))],
        );
        let tgt = schema(
            &[("a", "object"), ("q", "object"), ("r", "object")],
            &[("q", "r", "prop", Some("p"))],
        );
        let cfn = network(&src, &tgt);
        let outcome = solve_iso(&cfn, &src, &tgt, &SearchBudget::default()).unwrap();
        let (best, argmins) = iso_optimum(&cfn, &src, &tgt);
        assert_eq!(outcome.upper_bound, best);
        assert!(argmins.contains(outcome.best.as_ref().unwrap()));
        assert_eq!(outcome.best.as_ref().unwrap().dropped(), 0);
    }

    #[test]
    fn a_pair_with_nothing_in_common_drops_everything() {
        let src = schema(&[("a", "object")], &[]);
        let tgt = schema(&[("x", "string")], &[]);
        let cfn = network(&src, &tgt);
        let outcome = solve_iso(&cfn, &src, &tgt, &SearchBudget::default()).unwrap();
        assert!(mapping(&cfn, &outcome).is_empty());
        assert_eq!(outcome.best.as_ref().unwrap().dropped(), 1);
        assert!(outcome.proven_optimal);
    }

    #[test]
    fn the_answer_is_the_same_on_every_run() {
        let src = schema(
            &[("a", "object"), ("b", "object"), ("c", "string")],
            &[("a", "b", "prop", Some("p")), ("b", "c", "prop", Some("q"))],
        );
        let tgt = schema(
            &[("x", "object"), ("y", "object"), ("z", "string")],
            &[("x", "y", "prop", Some("p")), ("y", "z", "prop", Some("q"))],
        );
        let cfn = network(&src, &tgt);
        let first = solve_iso(&cfn, &src, &tgt, &SearchBudget::default()).unwrap();
        for _ in 0..8 {
            let again = solve_iso(&cfn, &src, &tgt, &SearchBudget::default()).unwrap();
            assert_eq!(first.best, again.best);
            assert_eq!(first.nodes, again.nodes);
            assert_eq!(first.upper_bound, again.upper_bound);
            assert_eq!(first.lower_bound, again.lower_bound);
        }
    }

    #[test]
    fn a_node_budget_stops_the_search_and_still_certifies_a_bound() {
        let src = schema(
            &[("a", "object"), ("b", "object"), ("c", "object")],
            &[("a", "b", "prop", Some("p"))],
        );
        let tgt = schema(
            &[("x", "object"), ("y", "object"), ("z", "object")],
            &[("x", "y", "prop", Some("p"))],
        );
        let cfn = network(&src, &tgt);
        // A budget of zero nodes is the sharpest reading of the anytime
        // contract: the greedy incumbent is all there is, and everything the
        // outcome claims has to hold of it.
        let budget = SearchBudget::default().with_max_nodes(Some(0));
        let outcome = solve_iso(&cfn, &src, &tgt, &budget).unwrap();
        assert_eq!(outcome.limit_hit, Some(LimitKind::Nodes));
        assert_eq!(outcome.nodes, 0);
        assert!(!outcome.proven_optimal);
        assert!(outcome.lower_bound <= outcome.upper_bound);

        // The incumbent is still a real assignment with the cost reported.
        let best = outcome.best.as_ref().unwrap();
        assert_eq!(cfn.evaluate(best), outcome.upper_bound);

        // And the certified bound holds against the true optimum.
        let (optimum, _) = iso_optimum(&cfn, &src, &tgt);
        assert!(outcome.lower_bound <= optimum);
        assert!(optimum <= outcome.upper_bound);
    }

    #[test]
    fn the_search_reaches_the_optimum_with_no_warm_start() {
        // The greedy descent solves these instances outright, so the branching
        // itself is exercised here instead: the same search with no incumbent
        // to prune against has to walk the tree and reach the same answer.
        let cases: [Shape; 4] = [
            (
                vec![("a", "object"), ("b", "object"), ("c", "string")],
                vec![("a", "b", "prop", Some("p")), ("b", "c", "prop", Some("q"))],
            ),
            (
                vec![("a", "object"), ("b", "object")],
                vec![
                    ("a", "b", "prop", Some("p")),
                    ("a", "b", "item", Some("extra")),
                ],
            ),
            (
                vec![("a", "object"), ("b", "object"), ("c", "object")],
                vec![("a", "b", "prop", Some("p")), ("b", "a", "prop", Some("q"))],
            ),
            (
                vec![("a", "object")],
                vec![("a", "a", "prop", Some("self"))],
            ),
        ];
        for (vertices, edges) in &cases {
            let src = schema(vertices, edges);
            for (other_vertices, other_edges) in &cases {
                let tgt = schema(other_vertices, other_edges);
                let cfn = network(&src, &tgt);
                let instance = Instance::new(&cfn, &src, &tgt).unwrap();
                let root = instance.root();
                let mut search = Search::new(&instance, &SearchBudget::default());
                search.search(&root);

                let (best, argmins) = iso_optimum(&cfn, &src, &tgt);
                assert_eq!(search.best_cost, best, "the optimum with no warm start");
                assert!(argmins.contains(&search.best));
                assert!(search.nodes >= 1, "the search must open its root");
            }
        }
    }

    #[test]
    fn a_source_with_no_variables_solves_trivially() {
        let cfn = CfnBuilder::new(Vec::new(), DEFAULT_WEIGHTS)
            .unwrap()
            .build();
        let empty = schema(&[("a", "object")], &[]);
        let outcome = solve_iso(&cfn, &empty, &empty, &SearchBudget::default()).unwrap();
        assert_eq!(outcome.best, Some(Assignment::all_bottom(0)));
        assert!(outcome.proven_optimal);
    }

    // -- the reward frame --------------------------------------------------

    #[test]
    fn a_network_that_forbids_dropping_everything_is_refused() {
        let src = schema(&[("a", "object")], &[]);
        let tgt = schema(&[("x", "object")], &[]);
        let mut builder = CfnBuilder::new(
            vec![(Name::from("a"), vec![Name::from("x")])],
            DEFAULT_WEIGHTS,
        )
        .unwrap();
        builder
            .add_unary_table(VarId::new(0), &[Cost::BOT, Cost::TOP_SENTINEL])
            .unwrap();
        let cfn = builder.build();
        assert!(matches!(
            solve_iso(&cfn, &src, &tgt, &SearchBudget::default()),
            Err(IsoError::BottomInfeasible { .. })
        ));
    }

    #[test]
    fn a_network_that_pays_to_drop_is_refused() {
        let src = schema(&[("a", "object")], &[]);
        let tgt = schema(&[("x", "object")], &[]);
        let mut builder = CfnBuilder::new(
            vec![(Name::from("a"), vec![Name::from("x")])],
            DEFAULT_WEIGHTS,
        )
        .unwrap();
        // Mapping costs more than dropping, so its reward would be negative.
        builder
            .add_unary_table(VarId::new(0), &[Cost::from_raw(5), Cost::from_raw(1)])
            .unwrap();
        let cfn = builder.build();
        assert!(matches!(
            solve_iso(&cfn, &src, &tgt, &SearchBudget::default()),
            Err(IsoError::NegativeReward { .. })
        ));
    }

    #[test]
    fn a_network_charging_unevenly_for_a_drop_is_refused() {
        let src = schema(&[("a", "object"), ("b", "object")], &[]);
        let tgt = schema(&[("x", "object"), ("y", "object")], &[]);
        let mut builder = CfnBuilder::new(
            vec![
                (Name::from("a"), vec![Name::from("x"), Name::from("y")]),
                (Name::from("b"), vec![Name::from("x"), Name::from("y")]),
            ],
            DEFAULT_WEIGHTS,
        )
        .unwrap();
        // Row-major over three slots each, `b` fastest. The entry at
        // `(x, ⊥)` is offset 2 and differs from `(⊥, ⊥)` at offset 8.
        let mut table = vec![Cost::BOT; 9];
        table[2] = Cost::from_raw(7);
        builder
            .add_function(&[VarId::new(0), VarId::new(1)], table)
            .unwrap();
        let cfn = builder.build();
        assert!(matches!(
            solve_iso(&cfn, &src, &tgt, &SearchBudget::default()),
            Err(IsoError::UnevenDropCharge { .. })
        ));
    }

    #[test]
    fn a_network_over_a_different_schema_is_refused() {
        let src = schema(&[("a", "object")], &[]);
        let tgt = schema(&[("x", "object")], &[]);
        let cfn = CfnBuilder::new(
            vec![(Name::from("absent"), vec![Name::from("x")])],
            DEFAULT_WEIGHTS,
        )
        .unwrap()
        .build();
        assert!(matches!(
            solve_iso(&cfn, &src, &tgt, &SearchBudget::default()),
            Err(IsoError::SchemaMismatch { side: "source", .. })
        ));
    }

    #[test]
    fn the_dropped_position_count_reads_a_row_major_offset() {
        // Two variables of three slots each: offset 8 is `(⊥, ⊥)`, offset 2 is
        // `(x, ⊥)`, offset 6 is `(⊥, x)`, offset 0 is `(x, x)`.
        let slots = [3usize, 3usize];
        assert_eq!(dropped_positions(8, &slots), 2);
        assert_eq!(dropped_positions(2, &slots), 1);
        assert_eq!(dropped_positions(6, &slots), 1);
        assert_eq!(dropped_positions(0, &slots), 0);
    }

    // -- the oracle agreement, in the small ---------------------------------

    /// A schema shape: vertices as `(id, kind)` and edges as
    /// `(src, tgt, kind, name)`.
    type Shape = (Vec<(&'static str, &'static str)>, Vec<Arc>);

    /// One edge of a [`Shape`].
    type Arc = (
        &'static str,
        &'static str,
        &'static str,
        Option<&'static str>,
    );

    #[test]
    fn the_search_agrees_with_enumeration_on_a_handful_of_pairs() {
        let cases: [Shape; 3] = [
            (
                vec![("a", "object"), ("b", "string")],
                vec![("a", "b", "prop", Some("p"))],
            ),
            (
                vec![("a", "object"), ("b", "object"), ("c", "string")],
                vec![("a", "b", "prop", Some("p")), ("b", "c", "item", None)],
            ),
            (
                vec![("a", "object")],
                vec![("a", "a", "prop", Some("self"))],
            ),
        ];
        for (vertices, edges) in &cases {
            let src = schema(vertices, edges);
            for (other_vertices, other_edges) in &cases {
                let tgt = schema(other_vertices, other_edges);
                let cfn = network(&src, &tgt);
                let outcome = solve_iso(&cfn, &src, &tgt, &SearchBudget::default()).unwrap();
                let (best, argmins) = iso_optimum(&cfn, &src, &tgt);
                assert_eq!(outcome.upper_bound, best, "the optimum");
                let found = outcome.best.as_ref().unwrap();
                assert_eq!(cfn.evaluate(found), outcome.upper_bound);
                assert!(argmins.contains(found), "the answer must be an argmin");
                assert!(outcome.proven_optimal);
            }
        }
    }
}
