# Searching for a morphism

Two schemas may describe corresponding records without using the same vertex names or preserving every field. The morphism search chooses the correspondences that satisfy the structural conditions encoded by the schema pair, and it leaves a source vertex unmatched when no admissible image improves the result. The implementation represents this task as a finite optimization problem with explicit feasibility constraints.

This chapter covers:

- the span returned by the search
- the cost function network built from a schema pair
- the exact and bounded algorithms that solve the network
- the construction and certification of the returned span.

The same implementation supports partial overlaps, total morphisms, injective morphisms, and isomorphisms. These requests share an objective but do not always share an algorithm.

## The span returned by the search

A **span** from a source schema $S$ to a target schema $T$ is a pair of schema morphisms with a common domain $A$,

$$
S \xleftarrow{\ell} A \xrightarrow{r} T.
$$

The common domain $A$ is the **apex**, and $\ell$ and $r$ are the **legs**. The [`SchemaSpan`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SchemaSpan.html) returned by panproto gives these terms a specific implementation: $A$ is the sub-schema of $S$ induced by the source vertices that received target images, $\ell$ includes that sub-schema into $S$, and $r$ carries the images chosen by the search. [Migrations as morphisms](./migrations-as-morphisms.md#partial-correspondence-as-a-span) develops the categorical account. Here the span is the concrete output assembled from a solved assignment.

Consider a source schema with an object vertex `post`, a string vertex `post.text`, and an integer vertex `post.likes`. Suppose the target has an object vertex `article` and a string vertex `article.body`, but no integer vertex. The object and string vertices exercise kind and edge constraints; the integer vertex introduces dropping.

The source object may take `article` as an image because the vertex kinds agree. It may not take `article.body`, since an object vertex cannot map to a string vertex. Kind equality is enforced before the optimizer sees any costs.

The source string may take `article.body`. If the source has a `prop` edge from `post` to `post.text` and the target has a `prop` edge from `article` to `article.body`, this pair of choices preserves the edge even when the edge names differ. The name difference affects the score, but it does not make the assignment infeasible.

The integer vertex has no target of the same kind. Its only choice is the distinguished value $\bot_D$, which means that the vertex is dropped from the apex. If `post` and `post.text` are mapped while `post.likes` takes $\bot_D$, the apex contains the first two source vertices and the edge between them. A required-edge declaration can forbid that partial choice: when `post` requires the edge to `post.likes`, keeping `post` also requires both endpoints of that edge to survive.

The symbol $\bot_D$ is the **drop value**. The subscript distinguishes it from the zero cost sometimes written $\bot$ in the valued-constraint literature. This chapter writes the cheapest cost as $0$ and reserves $\bot_D$ for the decision to omit a source vertex.

## Building the cost function network

[`build_cfn`](https://docs.rs/panproto-mig/latest/panproto_mig/solve/build/fn.build_cfn.html) converts an ordered pair $(S,T)$ into a [`Cfn`](https://docs.rs/panproto-mig/latest/panproto_mig/solve/cfn/struct.Cfn.html), or **cost function network**. A cost function network is a finite collection of variables, finite domains, and cost tables over small groups of variables. An assignment chooses one value for every variable. Its total cost is the sum of the selected table entries, except that the distinguished cost $\top$ is absorbing and denotes infeasibility. This is a valued constraint satisfaction problem [@schiexfargierverfaillie1995valued], with a closely related semiring reading [@bistarellimontanarirossi1997semiring].

The builder creates one variable $x_v$ for every source vertex $v$, ordered by source vertex name. Before caller restrictions are applied, its domain is

$$
D_v = \{\bot_D\} \cup
      \{a \in V_T : \operatorname{kind}(a)=\operatorname{kind}(v)\}.
$$

Target values are sorted by target vertex name. [`SearchOptions`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/struct.SearchOptions.html) may replace the same-kind candidates with one hard pin, and [`DomainConstraints`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/struct.DomainConstraints.html) may intersect them with an allowed set or remove source and target vertices. An incompatible hard pin leaves $\bot_D$ as the only value. Excluding a source vertex has the same effect. The variable remains present, which keeps variable identifiers and the packed-cost radix functions of the source schema alone.

Every assignment determines a partial vertex map. A value $x_v=a$ maps $v$ to $a$, whereas $x_v=\bot_D$ omits $v$. The all-drop assignment is feasible for every network produced by this builder, so a successfully built span search always has an answer. A build can still fail if its cost tables exceed the memory budget, and inducing the chosen apex can still report an invalid source fragment. Neither failure means that the schemas have no overlap.

For the vertex-and-edge fragment, forbidding $\bot_D$ turns feasibility into the usual homomorphism question: each source vertex receives a same-kind target, and each source edge receives a same-kind target edge between the chosen endpoint images. Constraint satisfaction and homomorphism provide two formulations of this decision problem [@federvardi1998computational]. Its dependence on the structure of the source side, including treewidth modulo homomorphic equivalence, is studied by @grohe2007complexity. The panproto network adds constraints for schema annotations that a bare graph homomorphism does not carry.

## Hard constraints and apex closure

A finite cost ranks an assignment. A $\top$ entry rejects it. The builder enforces seven constraint families. Kind equality is encoded by domain membership, while the other six use $\top$ entries.

| Constraint | Condition enforced by the network |
|---|---|
| Vertex kind | A target of a different kind is absent from the domain. |
| Edge preservation | If both endpoints of a source edge are mapped, a target edge of the same kind must join their images. |
| Required edge | If the owner survives, both endpoints of each required edge survive. |
| Coproduct variant | If a coproduct survives, each recorded variant and its parent vertex survive. |
| Recursion point | If a fixpoint marker survives, its target vertex survives. |
| Schema-span annotation | The two vertex references stored in `Schema::spans` survive together or are dropped together. |
| Hyper-edge signature | All referenced signature vertices survive together or are dropped together. |

The sixth row concerns a span annotation stored inside one schema. It is distinct from the result span $S \xleftarrow{\ell} A \xrightarrow{r} T$. A schema-span annotation is a pair of internal vertex references. The result span is the output of comparing two schemas.

The last five constraints make the chosen vertex set closed under source annotations that would otherwise dangle. Hyper-edge signatures are encoded as a clique of pairwise constraints, since partial survival occurs exactly when some pair disagrees about whether to survive. Recursion points and schema-span annotations can also connect variables that share no source edge. The graph used to choose a solver must thus be built after these constraints have been added.

Edge preservation and edge-name scoring share one lookup. For a source edge $e:p\to q$ and mapped endpoint images $a$ and $b$, the builder first seeks a target edge from $a$ to $b$ with the same kind and name. If none exists, it takes the least same-kind edge in the target's stable edge order. The first case pays no edge penalty, the second pays the full share of the edge component, and the absence of any same-kind edge yields $\top$. When either endpoint takes $\bot_D$, the source edge leaves the induced apex and pays the full edge penalty rather than $\top$.

Several source edges may constrain the same pair of variables. `CfnBuilder` merges their tables pointwise, so the finished network has at most one cost function for each scope. Self-loops are folded into the corresponding unary table. Scope uniqueness is needed by the fallback consistency algorithm as well as by the representation, since cost projection can oscillate when overlapping cost functions offer competing destinations for the same shifted cost [@leeleung2012consistency].

## The objective

The network minimizes structural dissimilarity. Three components are unary, one is attached to source edges, and optional alignment evidence adds a fifth unary component.

| Component | Local term | Source-fixed denominator | Default weight |
|---|---|---:|---:|
| Vertex name | Byte-level edit distance divided by the longer name length | $|V_S|$ | $0.25$ |
| Edge name | Zero for an exact name match, one for a same-kind rename or a dropped endpoint | Number of source edges with two source endpoints | $0.25$ |
| Outgoing names | Jaccard distance between the sets of named outgoing edges | Number of source vertices with a named outgoing edge | $0.30$ |
| Out-degree | Absolute degree difference divided by the larger degree | $|V_S|$ | $0.20$ |
| Alignment evidence | $1-\operatorname{confidence}(v,a)$ | $|V_S|$ | $0.00$ |

All denominators depend only on $S$. We call this property **source-fixed normalization** (SFN). Without SFN, dropping a poorly matched vertex would remove it from an assignment-dependent denominator and could improve the average merely by shrinking the apex. Under SFN, a dropped vertex still occupies its share of the source normalization and receives the worst finite unary penalty for every component that applies to it.

The outgoing-name denominator needs separate treatment. Let $C_S$ be the source vertices with at least one named outgoing edge. The corresponding sum ranges over $C_S$ and divides by $|C_S|$. A source leaf is thus outside that component regardless of whether its target image has children. This prevents the denominator from favoring a childless target for a reason unrelated to the correspondence.

Alignment evidence enters only through the last row. The builder validates every confidence as a number in $[0,1]$, then adds

$$
w_{\mathrm{anchor}}
\frac{1-\operatorname{confidence}(v,a)}{|V_S|}
$$

to the unary entry for mapping $v$ to $a$. Evidence does not alter domains, produce $\top$, choose a variable order, or change a budget. It can change the optimum only when the anchor weight is nonzero. The default weight is zero, so the direct default search is structurally scored even when evidence is present. [Alignment evidence](./alignment-evidence.md) describes how strategies produce and aggregate the confidence table.

Each local table entry is assembled in floating point while the network is built, then rounded once to integer units of $10^{-9}$. Every subsequent operation uses the [`Cost`](https://docs.rs/panproto-mig/latest/panproto_mig/solve/cost/struct.Cost.html) integer. This boundary makes later solver transformations independent of summation order, and it lets the fallback move cost between tables by exact subtraction. Projection and extension rely on that exact difference to preserve the cost of every assignment [@cooperschiex2004arc].

One integer also carries the secondary preference for coverage. If $q$ is the quality cost in fixed-point units and $\delta$ is the number of dropped source vertices, the stored value is

$$
c = q\rho + \delta,
\qquad
\rho = \operatorname{nextPowerOfTwo}(|V_S|+1).
$$

Because $\delta<\rho$, ordinary integer order is lexicographic order on $(q,\delta)$. The search first minimizes quality cost, then chooses the assignment with fewer dropped vertices among assignments tied on quality. The reported `quality` is $1-q/10^9$ and excludes $\delta$. The separate `apex_coverage` field reports $|V_A|/|V_S|$, with value one for an empty source.

## The primal graph and dispatch

The solver does not branch over complete maps. It first studies how the local tables connect the variables. The **primal graph** has one node for each variable and an edge between any two variables that occur in one cost-function scope. On the default path, each connected component can be solved independently because no table joins it to another component.

An elimination order processes the primal-graph nodes one at a time. When a node is eliminated, its remaining neighbors are joined into a clique. The largest number of such neighbors encountered along the order is the **induced width**. This width controls the arity of the intermediate tables created by bucket elimination [@dechter1999bucket], and it also characterizes the consistency needed for backtrack-free search on sparse constraint graphs [@freuder1982sufficient; @dechterpearl1987network].

For each component, the dispatcher compares descending source-name order with min-fill. Descending order tends to remove dotted-path leaves first; min-fill chooses the variable whose elimination adds the fewest edges. Smaller induced width wins, with descending order retained on a tie so that decoding proceeds in ascending source-name order.

Width selects an order, but the budget check uses the actual domain sizes. For a bucket that eliminates $X$ and sends a message over variables $U$, the implementation prices $\prod_{v\in U}|D_v|$ stored entries and $|D_X|\prod_{v\in U}|D_v|$ combine operations. These products are summed over the chosen order. Exact inference runs only when both the message memory and operation estimates fit the [`SearchBudget`](https://docs.rs/panproto-mig/latest/panproto_mig/solve/struct.SearchBudget.html). Otherwise the component is routed to bounded search. A separate, earlier memory check covers the network's original unary and local cost tables. Exceeding that build limit is an error because no in-memory network exists for either solver to consume.

## Exact inference by bucket elimination

Bucket elimination is the ordinary path when its messages fit. Each original cost function is placed in the bucket of the first variable in its scope under the elimination order. To eliminate $X$, the solver combines every function in $X$'s bucket and minimizes over $D_X$. The resulting message is a table over the other variables in those functions, and it is placed in the next bucket that can consume it. A message with empty scope contributes to the constant cost.

After the last variable is eliminated, the constant is the optimum. Decoding then visits the elimination order in reverse. At each step it selects the least-cost value consistent with the values already decoded. The stored messages guarantee that each such choice extends to a global optimum, so decoding neither branches nor backtracks. This is the `(min, sum)` instance of the bucket-elimination scheme described by @dechter1999bucket.

The implementation allocates only the outgoing message for a bucket. It iterates over assignments to the message scope on the outside and values of the eliminated variable on the inside, so it never materializes the full join table. Argmin values are recomputed during decoding rather than stored beside every message cell. This trades a second scan of the domains for lower resident memory.

Exact inference does not consult the node budget and does not prune. The dispatcher has already established that its full tables and loop nests fit the memory and operation budgets before it begins. Completion thus proves optimality. Ties are broken by target name with $\bot_D$ ordered after every target, read in decode order. [`SpanSearch::optima`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SpanSearch.html) can enumerate further assignments attaining the same optimum while the message tables are available.

## The bounded fallback

A component whose elimination messages exceed the budget goes to hybrid best-first search. The outer search keeps a priority queue of unexplored subtrees, each with a certified lower bound. It removes the subtree with the lowest bound and explores it depth first for a bounded number of backtracks, then returns the unexplored branches to the queue. The least bound still present in the queue is a lower bound on the global optimum. This is the hybrid best-first scheme of @allouchedegivrykatsirelosschiexzytnicki2015anytime.

The fallback maintains one mutable working network. An open node stores its decisions and bound, so revisiting that node resets the network and replays the decisions. Domains are copied at a branch, while changes to cost cells are recorded on a trail and restored to a mark. Local-consistency operations move cost from binary tables into unary tables and then into the zero-arity constant without changing the cost of any complete assignment. The constant is consequently a lower bound for every completion below the node. The default level is existential directional arc consistency, written $\mathrm{EDAC}^{*}$. The implementation also provides node, arc, directional arc, and full directional arc consistency. These levels and their cost-shifting operations follow the weighted-CSP treatments in @larrosa2002node, @larrosaschiex2004solving, @cooperdegivrysanchezschiexzytnickiwerner2010soft, and @degivryheraszytnickilarrosa2005existential.

Branch and bound closes a node when its lower bound reaches the incumbent cost. Before an incumbent exists, value order is determined by the bound obtained after propagating each candidate. Afterward the search tries the incumbent's saved value first. Variable order uses domain size divided by weighted degree, with additional weight assigned to cost functions that contributed most to a bound failure. These choices affect how soon a solution is found, but they do not change the objective or the certified bounds.

The fallback is bounded by elementary consistency operations and by search nodes. A caller may also set a wall-clock limit, though none is set by default. When a limit is reached, the outcome records the incumbent, the global lower bound, and the limit that stopped the search. A span can thus report a feasible answer without claiming it is optimal. A total-morphism search reports an error if it stops before finding any complete assignment, preserving the distinction between “no total morphism exists” and “the search did not finish.” [What panproto verifies](./what-is-verified.md) describes the property and oracle tests behind these claims. The external [toulbar2](https://github.com/toulbar2/toulbar2) solver provides a useful point of comparison for the same family of cost-function-network algorithms.

## Injective, surjective, and induced searches

The default network permits two source vertices to share a target. Search options that restrict the whole assignment cannot always be expressed by another local cost table, so they select specialized paths.

The `monic` option requires distinct surviving source vertices to take distinct target vertices. It runs the same bounded search with a counting Hall-set propagator [@mccreeshprosser2015backjumping]. Variables whose domains still contain $\bot_D$ are excluded from the pigeonhole count because they may escape by dropping. Once variables must take target values, the propagator can detect an insufficient union of targets and can remove a saturated Hall set from other domains. A matching-based propagator could enforce stronger generalized arc consistency [@regin1994filtering], but the implementation does not maintain that additional state. The `monic` option concerns vertex injectivity only. Parallel source edges may still share one target edge.

The `epic` option requires a total morphism whose vertex map covers every target vertex. [`find_span`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/fn.find_span.html) rejects this option because a span deliberately permits a partial right leg. Total-morphism entry points first reject impossible cardinalities, then use branch and bound with surjectivity checked on complete assignments. The check occurs inside optimization rather than after it. Filtering the unconstrained optimum could miss a more expensive assignment that is surjective.

The `iso` option asks the span search for a common induced sub-schema that is optimal under the packed objective, rather than under cardinality alone. This requires the right leg to reflect arcs as well as preserve them: between mapped vertex pairs, source and target arcs must agree as multisets of direction and edge kind. The implementation adapts the partitioning algorithm of @mccreeshprossertrimble2017partitioning to the network objective. Initial classes use vertex kind and self-loop descriptors. Each mapped pair refines the remaining classes by the multiset of incoming and outgoing edge kinds relative to that pair. Edge names stay out of the labels because they belong to the approximate score rather than structural feasibility.

The induced search measures reward relative to the all-drop assignment and maximizes that reward, which is equivalent to minimizing the original packed cost after its preconditions have been checked. It also reads hard apex constraints when a drop decision makes a required partner unavailable. The use of $\bot_D$ and its consequences for propagation are closely related to the constraint model analyzed by @mccreeshndiayeprossersolnon2016clique. For a total isomorphism request, the public entry point additionally requires full coverage of both vertex sets and constructs a bijective edge map.

## Assembling the result

Once a solver returns an assignment, `SpanSearch` collects every source vertex assigned a target and calls [`induce_on_vertices`](https://docs.rs/panproto-schema/latest/panproto_schema/induce/fn.induce_on_vertices.html). Induction restricts all schema fields to the chosen vertex set and validates the result against the supplied protocol. The left leg is the identity on the apex's vertices and edges. The right leg uses the assignment for vertices and the same edge-selection function used by naturality and edge scoring.

The ordinary right leg chooses an exact kind-and-name edge when one exists and otherwise the least same-kind edge. The iso path instead constructs a kind-preserving bijection within each pair of mapped endpoints, preferring equal names before pairing the remaining parallel edges. This distinction is recorded because injectivity on vertices does not imply injectivity on edges.

The span carries both measurements and a certificate. `quality` reads the assignment's primary cost, including the alignment-evidence component when its weight is nonzero. It does not include the packed secondary reward for retaining more vertices. `apex_coverage` records the fraction of source vertices retained. `quality_bounds` converts the solver's lower and upper primary-cost bounds into the corresponding interval on the higher-is-better quality scale.

The certificate records whether optimality was proved, which solver path ran, and which limit was reached. Its shape distinguishes vertex injectivity from edge-image injectivity and records whether the left inclusion covers the whole source. Both legs are checked for functoriality. Separate existence reports are computed for the two codomains, conditional obligations being available when the caller supplied the relevant theory registry. The certificate also records whether the induced apex has an entry vertex, a content digest of the apex, and the decode order used for exact tie-breaking. [Find a span between two schemas](../how-to/spans.md#the-certificate) shows how these fields are read through the public interface.

## Boundaries of the implementation

The search maps each source edge to one target edge. It does not map an edge to a target path, so it implements the length-one fragment of the functorial schema translations described by @spivak2012functorial. A correspondence that requires an intermediate target vertex or several target fields belongs in a value-level transform rather than in this vertex assignment. [Migrations as morphisms](./migrations-as-morphisms.md#the-length-1-fragment) develops that boundary.

The objective reads vertex identifiers, vertex kinds, outgoing edges, and edges between candidate endpoint images. Required edges, variants, recursion points, schema-span annotations, and hyper-edge signatures affect feasibility without affecting the finite score. Other schema fields, including value constraints, usage modes, defaults, and policies, are restricted during induction or checked later but do not distinguish two feasible assignments here. An existence report may consequently mark a selected leg invalid even when another equally scored assignment would have passed.

The five component weights have not been fitted to labeled correspondences. Work on schema and ontology matching shows that aggregation and extraction choices can materially change the resulting alignment [@dorahm2002coma; @meilickestuckenschmidt2007analyzing; @fariapesquitasantospalmonaricruzcouto2013agreementmakerlight]. Exact optimization guarantees an argmin of the stated cost function. It does not establish that the cost function ranks the intended correspondence first.

The remaining input to that cost function is the confidence table. [Alignment evidence](./alignment-evidence.md) explains how panproto constructs it and where hard pins still differ from soft evidence.
