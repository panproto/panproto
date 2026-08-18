# Searching for a morphism

## In plain terms

When a mapping file already exists, panproto reads it. When one does not, panproto has to find the correspondence itself: which source record type belongs with which target record type, which field with which field. That search is what this page is about. [Migrations as morphisms](./migrations-as-morphisms.md) covers what the search returns and why it is a span; here we cover how it is computed.

The search is an exact optimization. One decision variable per source vertex; its candidate values are the kind-compatible target vertices plus one extra value meaning "leave this vertex out of the answer"; every local choice carries a numeric cost; and the algorithm computes the cheapest complete assignment without listing the others. Two properties follow, and both matter on real input. Cost is governed by the structure of the network rather than by the number of complete maps, so a pair of shipped [ATProto](https://atproto.com/) lexicons is answered in milliseconds where counting the maps would not terminate in any useful time. And alignment hints enter as costs to be lowered rather than as candidates to be removed, so a more permissive hint tier can only change which answer is best, never make a jointly inconsistent choice that a stricter tier avoids.

The apparatus that makes this precise dates from the 1980s and 1990s and comes with theorems attached, which is most of the reason for adopting it rather than inventing something.

## The reduction

Fix an ordered pair of schemas: a source $S$ with vertex set $V_S$ and edge set $E_S$, and a target $T$. The search poses one variable $x_v$ for each source vertex $v \in V_S$. The domain of $x_v$ is

$$
d(v) \;=\; \{\bot\} \;\cup\; \{\, a \in V_T : \mathrm{kind}(a) \text{ is compatible with } \mathrm{kind}(v) \,\}.
$$

Here $\bot$ means that $v$ is omitted from the apex. Each source edge $e : p \to q$ contributes a binary term on $(x_p, x_q)$ taking the value $\top$ when both endpoints are mapped and no kind-compatible target edge runs between their images. The builder merges terms sharing a scope pointwise as it goes, so the network carries one binary cost function per constrained *pair* of source vertices rather than per edge, and parallel source edges between one pair fold into a single function. An assignment avoiding $\top$ is exactly a structure-preserving map on the vertices it assigns.

This is the homomorphism problem, and constraint satisfaction is the same problem under a different name. @federvardi1998computational studies it in that form and states the dichotomy conjecture for it; @grohe2007complexity classifies it from the left, restricting the source structure rather than the target, and shows that under standard parameterized-complexity assumptions tractability coincides with bounded treewidth on the left *modulo homomorphic equivalence*. That qualifier carries the theorem rather than decorating it: a class of unbounded treewidth whose cores are narrow is tractable, so what the bound constrains is the cores and not the structures as they were handed in.

The distinction from subgraph isomorphism is what tells us which literature applies. A homomorphism may send two source vertices to one target vertex and may send a source edge into a target that carries other edges besides; a subgraph isomorphism may do neither. The injective-matching solvers are engineered for a small dense pattern searched into a large sparse target, where propagation is what saves the search. panproto's default search is non-injective, its source and target are comparable in size, and its constraint graph is nearly a forest. We return to what that costs us, and what it saves, in [Where the measurements contradict the recommendations](#where-the-measurements-contradict-the-recommendations).

## Partiality is a value

$\bot$ means the source vertex is not in the answer. The apex is $A = \{\, v \in V_S : x_v \neq \bot \,\}$ with the induced edges, so a partial match is an assignment rather than a fallback, and the all-$\bot$ assignment is always feasible. The search thus never refuses for want of a match, and no separate subgraph search runs alongside the morphism search: the maximum common subschema is the network's optimum.

Carrying $\bot$ in the domain rather than outside it is what makes this true at the level of representation instead of at the level of a special case. A vertex with no kind-compatible target has $\{\bot\}$ for a domain and is dropped; a caller who excludes a source vertex forces $x_v = \bot$ rather than deleting the variable, so the variable set stays a function of the source schema alone.

Seven families of cost function take the value $\top$, and together they make the apex well formed by construction, so that inducing on $\{\, v : x_v \neq \bot \,\}$ never has to repair anything afterwards.

| Constraint | How it is encoded |
|---|---|
| Kind compatibility | Absence from the domain |
| Naturality | $\top$ when both endpoints are mapped and no kind-compatible target edge runs between their images |
| Required edge | $\top$ when the owning vertex survives and an endpoint of one of its required edges does not |
| Variant member | $\top$ when a coproduct survives and one of its variants does not |
| Recursion point | $\top$ when a fixpoint marker survives and its target does not |
| Span | $\top$ when exactly one of a schema span's two endpoints survives |
| Hyper-edge signature | $\top$ when a signature is partly dropped, as a clique of pairwise constraints |

Three of these can join variables that no schema edge joins. A recursion point, a schema span and a hyper-edge signature each constrain a set of vertices that need not be adjacent, so the constraint graph of the network can be denser than the schema graph it came from. Nothing downstream may read a width off the schema.

## The valuation structure

Costs live in the truncated structure

$$
S(k) \;=\; \langle\, \{0, 1, \ldots, k\},\; \oplus,\; \succeq \,\rangle,
\qquad a \oplus b = \min(k,\, a + b),
\qquad \bot = 0,
\qquad \top = k.
$$

This is a valued CSP in the sense of @schiexfargierverfaillie1995valued, whose contribution was to replace the ad hoc aggregation operators of earlier soft-constraint proposals with a single ordered commutative monoid subsuming classical, possibilistic, weighted and probabilistic constraint satisfaction. A parallel presentation, the c-semiring framework of @bistarellimontanarirossi1997semiring, reads the two operations as constraint projection and constraint combination. The two are close relatives rather than notational variants, and nothing below rests on their being interchangeable. We take the valued-CSP orientation, so that $\oplus$ is truncated addition, $\bot$ is the best cost, $\top$ is infeasible, and $\succeq$ reads "worse than or equal". The two uses of $\bot$ are worth keeping apart, because they do not agree: $\bot$ as a *domain value* means the source vertex is omitted, and omission is the most expensive assignment that vertex has, taking the full penalty on every quality component plus one [`DROP_UNIT`](https://docs.rs/panproto-mig/latest/panproto_mig/solve/cost/constant.DROP_UNIT.html), while $\bot$ as a *cost* is the identity of $\oplus$ and so the cheapest value there is.

The axioms are commutativity, associativity, and three more:

$$
a \oplus \bot = a,
\qquad a \oplus \top = \top,
\qquad (a \succeq b) \implies (a \oplus c \;\succeq\; b \oplus c).
$$

The third is monotonicity, and it is the axiom the search leans on hardest. Read in the direction the search uses it, monotonicity says that combining more cost never yields a better total, so a soft term can lower the optimum and can never make a feasible assignment infeasible. Both tier theorems below are corollaries of it.

One further condition is needed before any cost may be moved around, and it is not among the axioms above. A valuation structure is **fair** when every pair $\alpha \preceq \beta$ admits a unique maximal difference $\beta \ominus \alpha$, and in a fair structure

$$
(u \oplus w) \oplus (v \ominus w) \;=\; u \oplus v
\qquad \text{whenever } w \preceq v.
$$

[@cooperschiex2004arc]. That identity is the single load-bearing fact of the whole solver: every operation that shifts cost from one table to another subtracts exactly what it adds, so every proof that such an operation is harmless is an instance of it. An operation satisfying it only approximately makes the optimizer unsound rather than imprecise, which is the next section.

$\top$ is passed as an argument on every operation rather than fixed as a global. It is the moving primal bound, lowered at every improving solution, so it belongs to the search state and not to the algebra; reading a stale $\top$ inside a saturating addition produces a silently wrong answer that no test on the arithmetic alone can catch, since every axiom above holds for whichever $\top$ was passed.

## Why the costs are integers

Costs are fixed-point integers, in units of $10^{-9}$, and the integer representation is a correctness requirement rather than a performance choice. There are five reasons, each sufficient on its own.

First, termination. The enforcement bound for the strongest consistency level used here is $O(ed^2 \max\{nd, \top\})$ [@degivryheraszytnickilarrosa2005existential], and it is finite only because every increase of the zero-arity cost is at least one unit and that cost is bounded by $\top$. Over the reals those increments may shrink geometrically and the enforcement loop has no termination proof at all.

Second, exactness of the shifts. The identity of the previous section is an exact-difference statement. In floating point $(x + \alpha) - \alpha \neq x$ in general, so pointwise preservation of the cost distribution degrades from an identity into an approximation, and a transformation that is approximately harmless is a transformation that changes the optimum.

Third, the direction of the drift. Accumulated error makes a cost negative, and one negative entry invalidates the lower bound. Branch and bound then prunes the subtree holding the optimum and returns a suboptimal answer with no error signal, which is the worst failure mode available: wrong, silent, and reproducible only by luck.

Fourth, comparisons against $\top$ are pruning decisions, not tests of approximate equality, and the argmin is a comparison of sums. Two assignments differing by less than the accumulated error would be ordered arbitrarily, so "returns a true argmin" would not even be a well-posed claim about the implementation.

Fifth, reproducibility. Floating-point sums depend on the order in which the shifts were applied, which depends on heuristics and on hash iteration order. The version control layer needs the same span for the same inputs on every machine.

The objective is lexicographic: minimize the quality cost, then minimize the number of dropped source vertices. Both components live in one integer,

$$
\mathrm{Cost} \;=\; q \cdot \rho + \delta,
\qquad \rho = \bigl(|V_S| + 1\bigr) \text{ rounded up to a power of two}.
$$

with $q$ bounded by the scale and $\delta$ by $|V_S| < \rho$, so the integer ordering *is* the lexicographic ordering on the pair. The encoding has a precondition that belongs to whoever builds the network rather than to the arithmetic: componentwise addition is exact only while the drop counts being summed stay below $\rho$, which holds because each variable contributes at most one drop unit and the cost shifts move cost without creating it.

A pair of fields with a derived lexicographic ordering would be the natural alternative, and it is wrong in a way that breaks correctness rather than taste. The lexicographic product of two ordered monoids is not fair. Under componentwise addition $(1, 5) \preceq (2, 0)$ holds, yet no $\gamma$ satisfies $(1,5) \oplus \gamma = (2,0)$, so the maximum difference does not exist, so the identity of the previous section is unavailable, so every proof that a cost shift is harmless fails at once. A plain integer is fair because subtraction is total on it.

## The objective, decomposed

The quality score panproto reports is a weighted mean of four components. Three are separable over source vertices and the fourth over source edges, so the whole score decomposes into one unary cost function per variable and one binary cost function per constrained pair of source vertices.

| Component | Arity | Denominator |
|---|---|---|
| Vertex name similarity | Unary in $(v, a)$ | $\lvert V_S \rvert$ |
| Edge name preservation | Binary in $(x_p, x_q)$ | $\lvert E_S \rvert$ |
| Outgoing edge name overlap | Unary in $(v, a)$ | $\lvert C_S \rvert$ |
| Out-degree agreement | Unary in $(v, a)$ | $\lvert V_S \rvert$ |

Here $|E_S|$ counts the source edges both of whose endpoints are source vertices. The qualification is not pedantry: `Schema`'s fields are public, an edge naming a vertex the schema does not hold has no variable at that end and so can carry no cost function, and counting it in the denominator would score it as perfectly preserved.

Every denominator is a function of the source schema alone. Call this **source-fixed normalization** (SFN), because it is the property that makes two answers over one source comparable, and it is the one substantive change the decomposition made to the score's meaning. A score that divides by the size of the *assignment* makes two partial assignments of different sizes incomparable: dropping a badly matched vertex raises the mean of what remains, so the best score goes to the emptiest apex. Dividing by $|V_S|$ instead lets an unassigned vertex contribute nothing to the numerator while still counting in the denominator, so a span covering half the source scores strictly worse than one covering all of it at the same per-pair quality.

For three of the four components, SFN changes nothing on a total morphism, since the assignment then has exactly $|V_S|$ pairs and every counted source edge has both endpoints assigned. The fourth is a genuine correction, and the correction leaves the numerator alone.

Write $C_S = \{\, v \in V_S : v \text{ has at least one named outgoing edge} \,\}$ for the SFN normalizer of the overlap component, and write $P$ for the reference normalizer, the set of mapped pairs $(s, t)$ where either side has a named outgoing edge.

**Proposition.** On a total morphism the two numerators agree, so the two versions of the component differ only in their normalizer.

*Proof.* $C_S$ is contained in the source side of $P$: a source vertex with a named outgoing edge leaves the union of the two name sets non-empty whatever it maps to, so its pair is in $P$. Conversely, take a pair in $P$ whose source lies outside $C_S$. That source has an empty name set, hence an empty intersection with anything, hence overlap exactly zero, and it contributes nothing to the numerator. The extra pairs $P$ counts are all zero terms, and only the denominators differ. $\square$

That difference is the point of making the change. Under the reference normalizer, a source leaf mapped onto a childless target leaves the denominator and raises the mean, while the same leaf mapped onto a target with children enters the mean at zero and lowers it. The score thus rewarded mapping leaves onto childless targets, for no structural reason anyone had intended. Under $C_S$ a source leaf sits outside the sum whatever its image, and the incentive is gone.

Each cost function entry is rounded to fixed point exactly once, after its components have been summed in floating point. A total assignment selects one unary entry per source vertex and at most one binary entry per source edge, so the quality read back out of the integer objective differs from a floating-point accumulation of the same terms by at most $(|V_S| + |E_S|) / (2 \cdot 10^{9})$. Nothing sums in floating point after the network is built.

## Evidence is a reward

Alignment strategies propose anchors: this source vertex probably goes to that target vertex, with a confidence in $[0, 1]$. Where those confidences come from and how they are aggregated is the subject of [Alignment evidence](./alignment-evidence.md); what matters here is how the aggregate reaches the solver. It enters the objective through exactly one term,

$$
w_{\mathrm{anchor}} \cdot \frac{1 - \mathrm{conf}(v, a)}{|V_S|}.
$$

and through nothing else. Call this discipline **reward-only evidence** (ROE). It comes with three prohibitions, and the network builder holds to all three: evidence never removes a value from a domain, since domains are computed from kinds and from the caller's own hard restrictions before any evidence is read; evidence never produces a $\top$-valued cost, since confidence is bounded by one and the anchor weight is finite; and evidence never reorders a variable or bounds a budget, so it changes which assignment is optimal rather than which one is found first.

Two theorems follow, and they are the reason the stringency ladder is now a property of the encoding rather than an invariant defended by a retry. Write $\mathcal{T} \subseteq \mathcal{T}'$ for two tiers, meaning the strategies that fire at $\mathcal{T}$ also fire at $\mathcal{T}'$, and assume the aggregate confidence is **monotone in the anchor pool**: adding an anchor never lowers a pair's score. That hypothesis is a constraint on the aggregator and not a description of one. The shipped aggregator satisfies it by taking a maximum inside each of six families and then a mean across the six with a *fixed* divisor, which is what [Alignment evidence](./alignment-evidence.md#why-the-arity-is-fixed) argues at length; a divisor counting only the families that fired would break it, since a weak anchor from a family that had not yet spoken adds a little to the numerator and a whole unit to the denominator.

**Theorem (tier invariance).** The feasible set is the same at $\mathcal{T}$ and at $\mathcal{T}'$.

*Proof.* Feasibility is the property of costing strictly less than $\top$. No hard constraint reads the evidence, by the first prohibition; and the evidence term is bounded above by $w_{\mathrm{anchor}} / |V_S|$, which is far below $\top$, by the second. An assignment's cost thus reaches $\top$ on account of the hard constraints alone, and those are computed from the two schemas and the caller's restrictions, neither of which the tier touches. $\square$

**Theorem (tier monotonicity).** The optimal cost at $\mathcal{T}'$ is no worse than the optimal cost at $\mathcal{T}$.

*Proof.* The anchor pool at $\mathcal{T}'$ contains the pool at $\mathcal{T}$, and the aggregate is monotone in the pool by hypothesis, so $\mathrm{conf}_{\mathcal{T}'}(v, a) \geq \mathrm{conf}_{\mathcal{T}}(v, a)$ pointwise. The anchor term is decreasing in confidence, so every unary entry is pointwise no worse at $\mathcal{T}'$, and by monotonicity of $\oplus$ no assignment's total is worse either. The two minimizations range over the same set by tier invariance, so the minimum at $\mathcal{T}'$ is at most the minimum at $\mathcal{T}$. $\square$

Both theorems concern the objective, and neither says that the *alignment* improved. A tier that fires more strategies is guaranteed a cost no higher and an apex no smaller; whether its correspondence is more nearly correct is a separate claim, and the weights have not been calibrated to support it. The shipped anchor weight is also zero, which makes the term the same constant on every value of every variable. Tier invariance is therefore the operative property today, while CI checks tier monotonicity with the weight raised. A penalty for declining an anchored target would reverse the desired monotonicity: more strategies would mean more penalties and could worsen the optimum. That is why reward-only evidence (ROE) is a constraint on the encoding.

## The primal graph and its width

The **primal graph** of the network has one node per variable and an edge between two variables that share the scope of some cost function. An elimination sequence is an ordering of the variables; eliminating a variable adds the edges that make its not-yet-eliminated neighbors a clique; and the **induced width** of the sequence is the largest number of not-yet-eliminated neighbors any variable has at the moment it is eliminated. Width is the parameter that decides everything about cost here. @dechter1999bucket is the paper that says so: it unifies directional resolution, adaptive consistency, Fourier and Gaussian elimination, dynamic programming for combinatorial optimization and several probabilistic inference tasks as one algorithm under different combination operators, all of them exponential in time and space in exactly this quantity.

Two deterministic orders are computed and the narrower is taken: a min-fill ordering, and the reverse of ascending source vertex name order, which puts the deepest source vertices first and holds the width at one on a tree.

The width chooses the order; it does not price it. Comparing two orders needs only the exponent, but deciding whether to allocate needs the number itself, and $d^{w+1}$ is an upper bound stated over one domain size and the widest bucket. On the shapes this engine actually sees it is loose by a factor of $d$: a record and a text file are both stars, and eliminating a leaf of a star leaves a bucket over the leaf and the hub, where the hub takes one vertex or $\bot$. That bucket costs $2d$ operations rather than $d^2$. So the price the dispatcher routes on is the sum over buckets of $\prod_{v \in U_p \cup \{X_p\}} |D_v|$, in the domains each bucket actually spans, and it is computed by walking the elimination order rather than by raising a maximum to a power. The scopes it multiplies are the ones the sweep itself runs on, so the estimate cannot describe a sweep other than the one that will run.

The width is read off the cost function scopes rather than off the schema's edges, for the reason given above: recursion points, schema spans and hyper-edge signature cliques constrain vertex sets that need not be adjacent. A routing decision taken before those constraints were added would allocate against a number that is too small, so nothing reads a stored width and the measurement runs after the network is built.

## Bucket elimination

The primary path is exact inference, and it never prunes, so it never consults the node budget and it reports optimality unconditionally. It is priced against the memory and operation budgets before it starts, which is a different thing: the dispatcher asks in advance whether the tables fit, and routes elsewhere when they do not. The backward sweep collects the cost functions into buckets and eliminates the variables in order, each variable absorbing every function whose scope it closes and handing the rest of the network one message over the variables it shared them with. The forward sweep reads an argmin back out of the messages in one greedy pass that never backtracks.

```text
eliminate(order, functions):
  for f in functions:
    p ← position in `order` of the earliest-eliminated variable in scope(f)
    buckets[p].append(f)

  for p = 0 .. n-1:                     # order[0] is eliminated first
    X ← order[p]
    U ← (⋃ scope(f) for f in buckets[p]) \ {X}
    m ← new table over U
    for u in assignments(U):            # U outside
      best ← ⊤
      for a in domain(X):               # X inside
        best ← min(best, ⊕ over f in buckets[p] of f(u, a))
        if best = ⊥: break
      m[u] ← best
    if U = ∅: c_∅ ← c_∅ ⊕ m[]           # a message with empty scope is a constant
    else:     buckets[position of earliest-eliminated variable of U].append(m)

decode(order, buckets):
  for p = n-1 .. 0:                     # backward along the elimination sequence
    X ← order[p]
    x[X] ← argmin over a in domain(X) of
             ⊕ over f in buckets[p] of f(x restricted to scope(f)\{X}, a)
  return x
```

Exactness needs one identity and nothing else: $\oplus$ distributes over $\min$. Everything else is a consequence, including the fact that the sweep is never approximate. It is only ever unaffordable, at $d^{w+1}$ operations and $d^{w}$ table entries for induced width $w$ and largest domain $d$; what the budget check decides in advance is the exact per-bucket product those two figures bound.

The forward sweep is backtrack-free by construction. @freuder1982sufficient identified the relation between the constraint graph's width and the level of consistency sufficient for search to succeed without backtracking, with the tree as the limiting case, and @dechterpearl1987network gave algorithms that solve the sparse and tree-structured classes optimally. What elimination does is pay for that property up front in the messages rather than enforce it in the domains.

Two implementation decisions are load-bearing enough to state. Argmin recovery is by recomputation rather than by stored argmin tables, because peak memory is the binding constraint and the recomputation is linear in the domains against a sweep that already paid $d^{w+1}$. The inner loop nest also runs the shared variables outside and the eliminated variable inside, accumulating with $\oplus$ and folding with $\min$ as it goes, so the join over $\{X\} \cup U$ is never materialized and the only table allocated is the message.

The same sweep is written once against a semiring interface and instantiated twice. Under $(\min, \oplus)$ it gives the optimum; under $(\Sigma, \times)$ over indicator values it gives the exact number of feasible assignments, subject to a ceiling above which it declines to report. That second reading is a diagnostic, and the section on what the corpus measures uses it for one bit only, whether the count is zero, since a reading that has saturated is no longer a count.

## The fallback: hybrid best-first search maintaining EDAC\*

Inputs whose elimination does not price inside the budget go to branch and bound, held to the same standard: the optimum it returns is the optimum, and the assignment it returns achieves that optimum when scored against an untouched copy of the network.

What runs is a hybrid, and which hybrid decides what an interrupted run is allowed to say. Depth-first branch and bound on its own is anytime in its upper bound alone: the incumbent improves as it goes, while the lower bound stays frozen at whatever the root was filtered to, so an interruption returns an answer with no statement of how wrong it might be. Best-first search has the opposite defect, certifying a bound at every step and finding no solution until it finishes. The hybrid keeps a frontier of unexplored subtrees, each tagged with the bound its node was filtered to, and repeatedly dives depth-first into the most promising one for a bounded number of backtracks, returning what the dive did not reach to the frontier. The least bound on the frontier is then a valid global lower bound, since the frontier together with the closed regions covers the whole assignment space and every closed region was closed under a bound no better than the current one. That is Algorithm 1 of @allouchedegivrykatsirelosschiexzytnicki2015anytime, and it is the wrapper every dispatched fallback runs under, including the `monic` path below.

The operation budget binds this path too, and in the same currency. A node is not a unit of work: filtering one node of a network of eight hundred variables reads about as many cost table entries as filtering a small network whole, and reaching a node on the frontier replays the decisions that lead to it, so a node ceiling generous enough to be useful on a small network is hours on a large one. The search is therefore charged for the elementary operations its filtering performs and stops when it has spent the budget exact inference was priced against, reporting the stop rather than absorbing it. The count is of operations rather than of milliseconds, so two runs over one network stop at the same place; a wall-clock ceiling is available and stays opt-in, because a result that depends on the machine it ran on is a different guarantee from the one the rest of this chapter states.

The bound comes from soft local consistency. Every cost function reads $\bot$ somewhere, so a straight sum of the functions reads $\bot$ as its own lower bound and says nothing useful. A cost-shifting operation *moves* cost between functions without changing what any assignment costs, and cost moved out of the binary tables into the unary tables, and out of the unary tables into the zero-arity constant, is cost that every assignment pays. The constant then becomes a lower bound the search can prune against. Three operations do the moving, and they come from three places. Projection and extension are the pair @cooperschiex2004arc introduced over an arbitrary fair valuation structure, and the identity of the previous section is exactly what licenses them. Unary projection, the move that sends cost from a domain into the zero-arity constant so that the lower bound itself rises, is @larrosa2002node's addition. @cooperdegivrysanchezschiexzytnickiwerner2010soft states all three as procedures in one place, derives Virtual Arc Consistency from them, and shows that over the rationals an optimal soft arc consistency closure is computable in polynomial time by reduction to linear programming. Each operation preserves the cost of *every* assignment rather than only of the optimum, which is what makes the bound valid at every node instead of only at the root.

Five local-consistency properties are available: node consistency ($\mathrm{NC}^{*}$), arc consistency ($\mathrm{AC}^{*}$), directional arc consistency ($\mathrm{DAC}^{*}$), full directional arc consistency ($\mathrm{FDAC}^{*}$), and existential directional arc consistency ($\mathrm{EDAC}^{*}$). They are not totally ordered by strength. The property order is

$$
\mathrm{NC}^{*} \;\preceq\; \mathrm{AC}^{*} \;\preceq\; \mathrm{FDAC}^{*} \;\preceq\; \mathrm{EDAC}^{*},
\qquad
\mathrm{NC}^{*} \;\preceq\; \mathrm{DAC}^{*} \;\preceq\; \mathrm{FDAC}^{*}.
$$

with arc consistency and directional arc consistency incomparable, each detecting a witness the other misses.

Two orderings run through those levels and only one of them is that chain. As **properties** the levels nest, and a network left by the enforcement of a higher level satisfies the predicates of the levels beneath it. As **bounds** they do not: a closure is not unique, so a stronger property does not on its own imply a larger $c_{\emptyset}$, and a pair is ordered by bound only when the stronger level's enforcement begins with the weaker one's whole sequence. Directional consistency shares a prefix with nothing above it, since enforcing it runs the node loop and then a directional loop where every level above runs the node loop and then an arc loop, and the two diverge immediately. So $\mathrm{DAC}^{*} \preceq \mathrm{FDAC}^{*}$ holds of the properties and fails of the bounds, and it fails in the direction that matters: on the five-variable network `dac_star_can_beat_fdac_star_on_the_bound` exhibits, DAC\* reaches 112 where FDAC\*, the nominally stronger level, reaches $\top$. Both readings are sound and the complete search returns the same answer at either, so this costs pruning rather than correctness. A caller wanting the largest bound available on a given instance has to measure rather than read the chain. @larrosa2002node introduced the starred levels, whose distinguishing move is to shift cost into the zero-arity constraint so that the lower bound rises, and reported that this prunes far more values than plain arc consistency. @larrosaschiex2004solving demonstrated empirically that branch and bound maintaining arc consistency of either kind was the strongest general weighted-CSP solver then measured. @degivryheraszytnickilarrosa2005existential added existential directional arc consistency, proved it strictly stronger than the full directional level, and reported that maintaining it during branch and bound is never worse and often orders of magnitude better. That is the level panproto maintains.

Enforcement has two preconditions that are not checkable from inside the loop. No two cost functions may share a scope, or else a unary cost has a choice of which one to extend into, nothing records the choice, and enforcement oscillates without raising the bound. @leeleung2012consistency identified the hazard in a broader form while generalizing the levels to flow-based global cost functions, reporting oscillation when EDAC\* is enforced on cost functions sharing *more than one* variable, and introduced weak EDGAC\* to avoid it. The two conditions coincide here only because every cost function in this network is unary or binary, so sharing more than one variable and sharing a scope are the same thing. The builder merges duplicate scopes pointwise at construction, so no network it produces can state the hazard. The second precondition is the integer representation, for the termination reason given above. A step budget backs both up, and stopping early is sound rather than merely tolerable: any prefix of a sequence of cost shifts is itself such a sequence, so the bound after it is valid, just weaker.

One subterm of the enforcement reads $(x \oplus \beta) \ominus \beta$, which is the identity over the reals and is *not* the identity in a truncated structure. Its job is to detect that a tuple has become infeasible: when $x \oplus \beta$ reaches $\top$ the subterm leaves $x$ at $\top$, and $\top$ is irreversible. That detection is the entire mechanism by which accumulated finite cost proves infeasibility, and the consequence, which reads like a bug on a fast pass, is that extension and unary projection can change the network even when the amount being shifted is $\bot$. @cooperdegivrysanchezschiexzytnickiwerner2010soft record the same behavior of the same two operations, so it is a property of the operations rather than of this implementation.

The hybrid wrapper is what delivers the anytime contract: a certified global lower bound alongside the incumbent at every observation point, the lower bound non-decreasing and the upper bound non-increasing, so an interrupted search returns a solution together with a proof that nothing better than the bound exists. The variable order is `dom/wdeg` with bound-failure attribution, which matters because in a branch and bound with a maintained bound most nodes die from the bound rather than from an empty domain, so plain `dom/wdeg` sees almost no failures and decays into plain `dom`. Value order is bound impact before there is an incumbent and phase saving after.

The depth-first layer underneath can also restart on a Luby schedule with decision nogoods, whose validity rests on the bound only ever falling, since a subtree closed under an earlier, looser bound stays closed under a later one. That capability is switched off on every dispatched path, and deliberately so: the hybrid's backtrack limit already plays the part a restart limit plays in a plain depth-first search, and two schedules cutting each other short would make neither legible. Restarts are thus a facility of the inner search rather than a feature of the solver as shipped.

## Injectivity: the monic and iso paths

Injectivity is not a property of the network. It constrains how variables may share values, which no cost function states and which the builder deliberately does not encode, so a caller who wants an injective answer says so by calling a different entry point rather than by adding a term.

The `monic` path is the same hybrid search with a counting Hall propagator added: sort the domains by size, sweep accumulating their union, fail when the union is smaller than the number of domains accumulated, and when the two are equal freeze that union as a Hall set, remove it from every other domain, and restart the accumulator. That is the propagator of @mccreeshprosser2015backjumping, designed for large domains, and it is stateless, which is what lets a copy-on-branch domain store use it with no trail. @regin1994filtering achieves strictly stronger filtering for the same constraint, generalized arc consistency by matching theory in $O(pd)$ space and $O(p^2 d^2)$ time; panproto does not maintain it, because its incremental matching and strongly-connected-component state is one to three orders of magnitude larger than the domain store it would guard. Running it once at the root as a preprocessing filter remains a sound option nobody has needed.

$\bot$ is not a value for this propagator. A variable that may still be dropped can always escape the pigeonhole, so it is never counted toward a Hall set and never causes failure, though it is still pruned by one. On the span search every domain carries $\bot$ and the propagator is inert by design; it bites on the total-morphism restriction, where $\bot$ is removed from every domain, and inside the search, where an assigned variable is a singleton.

The `iso` path wants a different object and gets a different algorithm. Where `monic` wants an injective morphism, which preserves structure without reflecting it and so permits a denser target, `iso` wants a maximum common induced subschema: a source arc runs between two apex vertices exactly when a matching target arc runs between their images. That is the partitioning algorithm of @mccreeshprossertrimble2017partitioning, branching on vertex labeling and partitioning the domains, run here against the network's objective rather than against cardinality. The label of a vertex is its kind together with the multiset of direction and edge-kind pairs to each already-mapped vertex; edge *names* are never in the label, since they are the thing being aligned approximately and putting a name in the label would make `user_id` and `userId` structurally incompatible.

The label invariant that algorithm rests on is a biconditional, and it is too strong for `monic`: it would refuse a perfectly good injective morphism into a target carrying one extra arc. The two paths are kept apart in the code for that reason, and the shared machinery between them is the propagator, not the search.

That algorithm bounds a node by the objective together with the capacity of a label class, $\min(|G_l|, |H_l|)$, and neither term reads the apex well-formedness constraints. Where a source's structure is carried by annotation maps rather than by arcs, that omission is the whole bound: every binary function such a source poses comes from a hard constraint, a hard constraint pays no reward, so the bound collapses to the sum of the per-vertex maxima and assumes every source vertex is mapped at once.

@mccreeshndiayeprossersolnon2016clique diagnose the general form of this while comparing the constraint model of maximum common subgraph against the clique reduction. Their constraint model gives each variable a value $\bot$ meaning the vertex is left unmatched, which is the same device [partiality is a value](#partiality-is-a-value) describes here, and they observe that tightening the edge constraints buys the model no filtering at all while $\bot$ remains in every domain: $\bot$ is a support for every value, so every pair of variables is arc consistent whatever the constraints say. Filtering becomes possible only once $\bot$ leaves a domain. Their conclusion, that the better model turns mainly on whether edges are labeled, is about which search to run; the observation underneath it is about when a hard constraint can prune at all, and it applies to the partitioning search unchanged.

That is the opening this search takes, because dropping a vertex is exactly the event that removes $\bot$ from consideration for it. The gap is closed by reading the constraints at that moment rather than by tightening the arithmetic. A vertex tied by a forbidden entry to one the search has already dropped can never be mapped, since a dropped vertex is $\bot$ in every completion below that node, so it leaves its label class and the capacity falls with it; and a mapped vertex whose partner has gone ends the node outright. Both steps are exact rather than heuristic. The first cuts straight to a drop branch every feasible completion takes anyway, and the second lands on a node where scoring would refuse every leaf.

The difference is not marginal. On the nine-vertex pair a fuzzer found, whose source carries seven coproduct arms and four schema spans and whose eleven constraints admit one vertex, the search without that step spends ten million nodes and sixty seconds and returns the empty apex without a proof; with it, 102 nodes and 1.2 milliseconds, a strictly lower cost, and a certificate. `panproto-mig/tests/the_hard_constraints_close_the_iso_search.rs` is that pair, and it fails in both directions if the step is removed.

## What the corpus measures

The measured corpus is 77 ATProto lexicons, seven fixtures and seventy `dev.panproto` definitions, read from the repository so that a lexicon added to it joins the sweep without anyone remembering to list it. Ordered pairs are searched in both directions, since the apex is induced on the source and $(a, b)$ and $(b, a)$ are different networks, which gives 5852 searches. `panproto-mig/tests/lexicon_sweep.rs` runs all of them in CI and asserts that every one comes back with a certificate of optimality, with a per-pair ceiling of 50 ms in release builds against a measured median around 360 microseconds. The margin on that ceiling is thinner than the median suggests, and `lexicon_sweep.rs` says so where it defines it: two runs of the same binary on an idle machine put the slowest pair at 28.6 ms and at 5.1 ms, so the maximum moves by more than a factor of five run to run and the worse reading sits at 57% of the ceiling. A loaded runner may cross it with nothing having regressed.

Two snapshot tests measure the corpus, one on each side of the search. `panproto-mig/tests/lexicon_span_shapes.rs` records the shape of the *answer* for each of the 861 unordered pairs drawn from the 42 record-typed lexicons, one row per pair carrying the width it measured and the path it took: 685 pairs at width one, 176 at width two, none higher, and none routed to branch and bound. `panproto-mig/tests/lexicon_domain_shapes.rs` records the shape of the *network*, before anything is solved, over the wider population of all 5852 ordered pairs of all 77 lexicons: 5168 at width one and 684 at width two, again none higher. Neither population is the other's, and each figure below says which one it was taken over.

The largest domain is measured on the wider population too, and it is a distribution rather than a headline. Taking the largest single domain of each ordered pair, with $\bot$ excluded, gives a minimum of 1, a median of 5, a 95th percentile of 12 and a maximum of 18. Taking every source vertex separately, over the 74176 variables the 5852 pairs pose between them, gives a minimum of 0, a median of 2, a 95th percentile of 9 and the same maximum of 18. That maximum is a thin tail: 75 of the 5852 pairs reach it, every one of them searching into `dev.panproto.vcs.commit`. At $w \leq 2$ a bucket costs the product of at most three domains, so the widest pair in the corpus prices at no more than $18^{3}$ operations per eliminated variable, orders of magnitude inside the default budget of $10^{9}$ operations and 64 MiB of tables. That is why exact inference is the primary path rather than a fortunate one.

Two properties of the corpus decide how the search behaves on it, and both are measured over all 5852 ordered pairs rather than inferred from one.

Partiality is the ordinary case. Six of `app.bsky.feed.post`'s 39 source vertices, the four `array` and two `integer` ones, have empty domains against `app.bsky.actor.profile`, which holds neither kind, so no total morphism between those two exists at all. That pair is typical rather than pathological. Over the whole corpus, 4950 of the 5852 ordered pairs contain at least one source vertex with no kind-compatible target, which is 84.6% of them. That condition is sufficient for an empty hom-set and not necessary, and the gap between the two is worth carrying: a further 167 pairs have every domain non-empty and are emptied by naturality alone, so 5117 pairs, 87.4%, admit no total morphism and only 735 admit one. Reporting the 84.6% by itself understates the rate, because it quotes a lower bound as though it were the answer.

A hom-set is often the full Cartesian product of the domains: every combination of assignments to the unconstrained vertices is a valid morphism, and naturality rules out nothing. [`detect_product`](https://docs.rs/panproto-mig/latest/panproto_mig/solve/elim/fn.detect_product.html) is what reads that shape off a network, and it needs no sweep at all: it reports when every constraint is universal, from which the count follows as the product of the domain sizes. The solution-count instantiation of the sweep is the separate and more expensive reading, and it is the one carrying an enumeration ceiling; a saturated count and a saturated product compare equal whatever the constraints did, so the domain snapshot reads the shape off `detect_product` and uses the count for the single bit of whether it is zero, which saturation cannot corrupt.

Naturality is not always doing work. Of the 735 ordered pairs admitting a total morphism, 327 have a hom-set that is the full product, which is 44.5%; on the other 408, some constraint forbids something. Reading the same numerator against the wider denominator, the 902 pairs whose every domain is non-empty, gives 36.3%, the difference being the 167 pairs that belong to that denominator and to neither part of the first ratio. Naturality thus constrains nothing on a substantial minority of the pairs where it has anything to constrain, and constrains something on the majority of them. Which way that reads depends on what is being asked: on those 327 pairs, "there is a morphism between these two schemas" is close to a free statement, and on the other 408 it is not.

## Where the measurements contradict the recommendations

The constraint literature puts propagation strength, in the form of arc consistency, bitset domains and supplemental graphs, at or near the top of what it recommends for a search of this shape. panproto does not follow that recommendation, and the reason is measured rather than argued.

Those recommendations are correctly grounded. @freuder1982sufficient relates the width of the constraint graph to the level of consistency that makes search backtrack-free, which is the result the recommendation usually leads with. @dechterpearl1987network gives algorithms optimal on sparse and tree-structured networks and derives value-ordering advice by relaxing the pending subproblem to a tree and counting its consistent solutions. @regin1994filtering achieves generalized arc consistency for the all-different constraint by matching theory. @mccreeshprosser2015backjumping adds supplemental graphs that generate implied constraints, alongside bit and thread parallelism and a lazy conflict-directed backjumping compatible with parallel search.

Three measurements bear on it, and each is a committed snapshot rather than an impression.

First, the propagation being recommended lives on the branch and bound path, and this corpus does not reach it. All 861 rows of the answer snapshot came back from exact inference, and over the 5852 ordered pairs no network measures an induced width above two, at which the message tables price orders of magnitude inside the default budget. A constant-factor win on a path nothing takes is worth nothing, and bit-parallel propagation, the largest constant-factor win on offer, is exactly such a win. Second, full arc consistency would guarantee a backtrack-freeness these networks already have: the forward sweep of elimination is backtrack-free by construction at any width it can afford, which here is every width the corpus exhibits. Third, on 327 of the 735 pairs admitting a total morphism every constraint is universal, so on those no propagator at any strength has a value to remove. On the remaining 408 it does, which is why the argument is stated over the corpus rather than over a single pair.

The caveat is the corpus itself: the width, the domain sizes and the product structure are properties of 77 ATProto lexicons, measured rather than proved, and a corpus is not a theorem. The recommendation is also sound at a scale this corpus does not reach. Its reasoning holds for instances of a few hundred vertices per side, where the measured pairs pose between 2 and 39 variables. If schema sizes grow, or if a protocol arrives whose schemas are genuinely dense rather than near-forests, propagation strength comes back into force, which is why the branch and bound path exists and is held to the same correctness standard as the elimination path rather than being a degraded mode.

## Where the model runs out

Three limits are worth naming, because each is a place where the search is narrower than the mathematics it is stated in.

The morphism class is the length-1 fragment. An edge maps to an edge and never to a path, so 1:n correspondences are not expressible as morphisms at all and are handled at the value level instead. [Migrations as morphisms](./migrations-as-morphisms.md#the-length-1-fragment) develops this, and @spivak2012functorial is the framework it falls short of.

The objective reads four of the twenty-one fields of a `Schema`: `vertices`, `edges`, and the two adjacency indices `outgoing` and `between`, the latter two derived from `edges`. Five more, `required`, `variants`, `recursion_points`, `spans` and `hyper_edges`, enter as feasibility constraints, deciding which apices are well formed without contributing to any score, and on the `iso` path additionally pruning the search. The remaining twelve, `incoming` among them, are restricted by [`panproto_schema::induce`](https://docs.rs/panproto-schema/latest/panproto_schema/induce/fn.induce.html) and are otherwise invisible to the search. A change that only a constraint sort or a usage mode distinguishes is thus a change the optimum cannot see.

The weights have never been calibrated. Four component weights trade name similarity, edge-name preservation, outgoing-name overlap and out-degree agreement against one another, and they encode a judgment about what a good match looks like that no labeled corpus has tested. Every constant on the evidence side is uncalibrated in the same way, and [Alignment evidence](./alignment-evidence.md#the-numbers-are-defaults-not-measurements) audits those one by one. The schema-matching literature is unambiguous that this is where quality is decided rather than in the solver: @dorahm2002coma measured aggregation, direction and candidate selection as three separately consequential steps and found reuse-oriented strategies strongest; @meilickestuckenschmidt2007analyzing showed on OAEI submissions that the extraction step alone materially changes precision and recall; @fariapesquitasantospalmonaricruzcouto2013agreementmakerlight give a selection stage its own named component in a system built for efficiency on very large ontologies, and report the best F-measure on the OAEI Anatomy track. Until a labeled corpus of ATProto lexicon alignments exists, panproto computes the exact optimum of a function nobody has validated. That is a considerably better position than computing an arbitrary element of a truncated enumeration of the same function, and it is not the same thing as being right.

## See also

- [Migrations as morphisms](./migrations-as-morphisms.md) for the span the search returns and the category it lives in.
- [Alignment evidence](./alignment-evidence.md) for where the anchor confidences come from before the objective reads them.
- [Find a span between two schemas](../how-to/spans.md) for running the search and reading its certificate.
- [What panproto verifies](./what-is-verified.md) for the properties the solver's tests actually check.
- [toulbar2](https://github.com/toulbar2/toulbar2) for the reference implementation of the cost function network machinery described here.
