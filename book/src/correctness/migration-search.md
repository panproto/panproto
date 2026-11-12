# Migration search

A migration built from a search result inherits the search's mistakes, so the result has to be checkable without trusting the search: every returned morphism must be a morphism of the two schemas, its reported quality must be the objective's value at it, it must be optimal, and the same pair must give the same answer every time. This page states those properties as laws of the total-morphism search ([`find_morphisms`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/fn.find_morphisms.html)) and of the span search ([`SpanSearch::run`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SpanSearch.html)), together with the refusals a search returns when a bound stops it. The construction itself is explained in [Searching for a morphism](../explanation/morphism-search.md).

The executable reference is [`panproto_reference::pages::migration_search`](https://docs.rs/panproto-reference/latest/panproto_reference/pages/migration_search/). It models a schema as its vertex ids with their kinds and its set of edges, lists every vertex assignment of a source into a target, computes each assignment's cost term by term in exact rational arithmetic, and reads the optima off the whole list. It shares nothing with production, which poses a cost function network and solves it by bucket elimination, branch and bound or partitioning.

## Definitions shared by the laws

A schema $S$ has vertices $V_S$, each with a kind, and edges $E_S$, each a tuple (source, target, kind, name) with an optional name; edges are compared by that tuple, an absent name first. An **assignment** $x$ sends each $v \in V_S$ to a target vertex of the same kind or to the drop value $\bot_D$. Write $n = |V_S|$, $C_S$ for the source vertices with at least one named outgoing edge and $m$ for the number of source edges whose endpoints are both source vertices.

The **edge image** of a source edge $e$ under endpoint images $a$ and $b$ is the target edge from $a$ to $b$ of the kind and name of $e$ when there is one (two absent names agree), and otherwise the least target edge from $a$ to $b$ of the kind of $e$; when there is neither, $e$ has no image. This is `edge_image` in both the reference and production.

The **terms** of $x$ are one vertex term per source vertex and one edge term per counted source edge. The vertex term of $v$ mapped to $a$ is

$$
\tfrac14 \cdot \frac{\operatorname{ed}(v,a)}{\max(|v|,|a|) \cdot n}
+ \tfrac15 \cdot \frac{|\deg v - \deg a|}{\max(\deg v, \deg a) \cdot n}
+ \tfrac{3}{10} \cdot \frac{|N(v) \mathbin{\triangle} N(a)|}{|N(v) \cup N(a)| \cdot |C_S|},
$$

where $\operatorname{ed}$ is the byte-level edit distance, $\deg$ the out-degree counting parallel edges and loops, $N$ the set of names on outgoing edges, a ratio with zero denominator reads zero, and the third summand is zero for $v \notin C_S$. Dropping $v$ sets each ratio to one. The edge term of $e$ is zero when both endpoints are mapped and its edge image keeps its name, $\tfrac14 / m$ when the image does not keep it or an endpoint is dropped, and makes the assignment infeasible when both endpoints are mapped and $e$ has no image. These are the default weights of [`DEFAULT_WEIGHTS`](https://docs.rs/panproto-mig/latest/panproto_mig/solve/cost/constant.DEFAULT_WEIGHTS.html), with the alignment-evidence weight zero.

The **cost** of a feasible assignment is the pair $(q, \delta)$ of its quality cost $q$, the sum over its terms of each term multiplied by $10^9$ and rounded to the nearest integer, and its number $\delta$ of dropped vertices, ordered lexicographically. Its **reported quality** is $1 - q/10^9$ evaluated in binary64. A pair is **clear of ties** when every term any assignment can carry, multiplied by $10^9$, lies further than $10^{-5}$ from the midpoint between two integers (`terms_clear_of_ties`).

A **request** is a set of the flags `monic` (injective on mapped vertices), `epic` (onto the target vertices) and `iso` (injective, and between any two mapped vertices the source and target carry the same multiset of edge kinds). The **checker** `check_total` accepts a vertex and edge map when the vertex map is total on $V_S$ and kind-preserving, meets the request's injectivity and surjectivity, maps exactly the counted source edges, and sends each to a target edge of its kind between the images of its endpoints: the edge image, or under `iso` the canonical pairing `paired_edge_images`, which pairs every edge with the target edge of its kind and name where one joins the images and then the remaining edges, least first, with the least remaining target edge of their kind. `check_span` additionally requires the apex to be the sub-schema of $S$ induced on its vertices and the left leg to be its identity.

## The total search returns the optimal total morphisms {#L-migration-search-1}

### Statement

Let $S$ and $T$ be schemas clear of ties, with default constraints, default weights and the default budget, and let $\rho$ be one of the requests none, `monic`, `epic`, `monic` with `epic`, and `iso`. Write $O$ for `optimal_total(S, T, ρ)`: the least cost over the feasible total assignments that meet $\rho$ (under `iso` also onto $V_T$) and every assignment attaining it. Then `find_morphisms(S, T, ρ)` returns `Ok` with `truncated` false, and:

1. when $O$ is empty, the list is empty;
2. every listed morphism passes `check_total`, its vertex map is an assignment of $O$, its reported quality is that of the optimal cost, and no vertex map is listed twice;
3. under the request none, the listed vertex maps are exactly the assignments of $O$ when the search ran by exact inference and held no more than `DEFAULT_OPTIMA_CAP` optima;
4. under `monic`, `epic` or `monic` with `epic`, the list holds exactly one morphism when $O$ is not empty;
5. under `iso`, the list holds exactly one morphism when the least-cost assignment of [L-migration-search-2](#L-migration-search-2) under `iso` maps every vertex onto $V_T$, and is empty otherwise.

### Outside the domain

A network whose cost tables exceed the memory budget is refused with `SpanError::Build` wrapping `CfnError::OverMemoryBudget`, and an `iso` request whose frames exceed it with `SpanError::Iso` wrapping `IsoError::OverMemoryBudget` ([L-migration-search-4](#L-migration-search-4)). A pair with more optima than `DEFAULT_OPTIMA_CAP` returns a prefix with `truncated` set, and a pair too wide for exact inference returns the one optimum branch and bound keeps. A search that a budget stops is refused with `SpanError::Stopped` ([L-migration-search-4](#L-migration-search-4)), whether or not it held a total morphism, so an `Ok` answer is never an unproved one. Under `iso` an empty list means that no isomorphism is the least-cost common induced sub-schema; on every pair of the bounded scope and of the property test it also means that no isomorphism exists, which is not proved for every pair. A pair that is not clear of ties may report a quality one unit of $10^{-9}$ per term away from the exact one. Hard pins, domain restrictions, alignment evidence, other weights and the schema annotations the network reads as hard constraints (required edges, variants, recursion points, spans and hyper-edges) are not modelled, and a schema with no vertices is refused by the schema builder before any search.

### Definitions used

The terms, the cost, the reported quality, the requests and the checker are defined above. `DEFAULT_OPTIMA_CAP` and the routing between exact inference and branch and bound are those of [Searching for a morphism](../explanation/morphism-search.md#the-primal-graph-and-dispatch).

### Proof

1. The network posed for $(S,T)$ has one variable per source vertex whose values are the same-kind target vertices and $\bot_D$ ([Searching for a morphism](../explanation/morphism-search.md#building-the-cost-function-network)); the total search forbids $\bot_D$ with an infeasible unary entry. Its assignments are therefore exactly the total assignments of the reference.
2. Each unary entry is the vertex term computed in binary64 and then rounded once, and each edge entry the edge term computed by the same `edge_image` selection, infeasible exactly where the reference's edge term is (a self-loop's entry is added to its vertex's unary table, which sums the same rounded integers). A vertex term is a sum of at most four products of at most three factors, each factor a correctly rounded quotient or a weight constant; under round to nearest each operation has relative error at most $2^{-53}$ (definition R3 of [L-values-1](values.md#L-values-1)), so the computed value, at most one, is within $9 \cdot 2^{-53}$ of the exact term, and after scaling by $10^9$ within $10^{-6}$ of the exact scaled term. On a pair clear of ties the exact scaled term is more than $10^{-5}$ from a midpoint, so both round to the same integer. Hence every network entry is the reference's rounded term and the network cost of every total assignment is its reference cost.
3. Bucket elimination computes the minimum of the summed cost functions exactly and decodes an assignment attaining it [@dechter1999bucket]; branch and bound under the default budget either proves its incumbent optimal or stops on a limit, and on the stated domain it is not stopped. So every assignment the search returns attains the least network cost over the assignments the path ranges over, which by step 2 is the least reference cost.
4. The `monic` path adds an injectivity propagator and the `epic` path a surjectivity test at each complete assignment ([Searching for a morphism](../explanation/morphism-search.md#injective-surjective-and-induced-searches)), so they range over the assignments meeting the request, and each returns its one incumbent: clause 4. The `epic` path first returns the empty list when $|V_S| < |V_T|$, or when `monic` is set and the counts differ; then no assignment meets the request and $O$ is empty.
5. Each returned assignment is turned into a morphism by taking the edge image of every counted source edge; it has one by step 2 since the assignment is feasible, so the morphism passes `check_total`, and its reported quality is read from the same integer cost. Under `iso` the induced search of [L-migration-search-2](#L-migration-search-2) runs instead, a result is returned only when its vertex map is total and onto, and its edge map is the canonical pairing; this is clause 5.
6. Clause 3 is the enumeration of every optimal assignment from the message tables, which the differential checks as an equality of sets: on every ordered pair of schemas with at most two vertices over the ids `a`, `ab` and `b`, two kinds and at most two `prop` edges named `n` or unnamed (468 schemas, 219,024 pairs) and under each of the five requests, the listed vertex maps are compared with $O$ as sets, the morphisms with the checker and the qualities bit for bit; the property test does the same on random pairs of up to four vertices, four edges, two edge kinds and three names.

### Executable counterpart

- Law ID: `migration-search.total-optima`
- Reference: `panproto_reference::pages::migration_search::optimal_total`
- Reference: `panproto_reference::pages::migration_search::check_total`
- Reference: `panproto_reference::pages::migration_search::cost`
- Enumerator: `panproto_reference::enumerate::migration_search::schemas; bound: vertices=2, edges=2`
- Exhaustive differential test: `total_search_returns_exactly_the_reference_optima_on_every_bounded_pair; command: cargo nextest run -p panproto-mig --test reference_migration_search --no-tests=fail -E 'test(=total_search_returns_exactly_the_reference_optima_on_every_bounded_pair)'`
- Property differential test: `randomized_total_searches_return_the_reference_optima; command: cargo nextest run -p panproto-mig --test reference_migration_search --no-tests=fail -E 'test(=randomized_total_searches_return_the_reference_optima)'`
- Reference test: `requests_restrict_the_morphisms_ranged_over; command: cargo nextest run -p panproto-reference --test migration_search --no-tests=fail -E 'test(=requests_restrict_the_morphisms_ranged_over)'`
- File: `crates/panproto-mig/src/hom_search.rs`
- File: `crates/panproto-mig/src/solve/build.rs`

## The span search returns an optimal span {#L-migration-search-2}

### Statement

Let $S$ and $T$ be schemas clear of ties, with default constraints, weights and budget, and let $\rho$ be one of the requests none, `monic` and `iso`. Write $O$ for `optimal_span(S, T, ρ)`: the least cost over the feasible assignments, drops allowed, that meet $\rho$, and every assignment attaining it. Then `SpanSearch::run` returns a span whose certificate is proven optimal, which passes `check_span`, whose right leg's vertex map, extended by $\bot_D$ on the source vertices outside the apex, is an assignment of $O$, whose reported quality is that of the optimal cost, and whose `apex_coverage` is $|V_A|/|V_S|$ in binary64. Under `epic` the search returns `SpanError::EpicIsNotASpanProperty`.

### Outside the domain

The memory refusals and the domain restrictions are those of [L-migration-search-1](#L-migration-search-1). A search a budget stops returns a span whose certificate is not proven optimal and names the limit ([L-migration-search-4](#L-migration-search-4)). Which of several optimal assignments is returned is decided by the tie-break the certificate reports and is not stated here; [L-migration-search-3](#L-migration-search-3) states that it is a function of the pair.

### Definitions used

The span, its apex, legs, certificate and coverage are those of [Searching for a morphism](../explanation/morphism-search.md#assembling-the-result); the cost and the checker are defined above.

### Proof

1. By steps 1 and 2 of [L-migration-search-1](#L-migration-search-1), with $\bot_D$ allowed, the network's assignments are the reference's assignments with drops and the packed network cost of each is $q \cdot \rho + \delta$ with $\rho$ a power of two above $n$, whose integer order is the lexicographic order on $(q, \delta)$ ([Searching for a morphism](../explanation/morphism-search.md#the-objective)).
2. On the default and `monic` paths the solver returns an assignment of least packed cost with a proof of optimality ([L-migration-search-1](#L-migration-search-1), step 3). On the `iso` path the partitioning search ranges over the injective assignments whose mapped pairs carry the same multisets of edge kinds, which is the reference's `iso` condition, and maximizes the reward relative to the all-drop assignment, which is equivalent to minimizing the packed cost [@mccreeshprossertrimble2017partitioning]; it reports optimality when no bound stopped it.
3. The apex is induced on the mapped vertices and the left leg is its identity, so the first two conditions of `check_span` hold. The right leg sends each mapped vertex to its value and each apex edge to its edge image, or under `iso` to the canonical pairing; the assignment is feasible, so every apex edge has an image and the right leg passes the checker. The quality is read from the integer cost without the drop count, and the coverage is the ratio of the two vertex counts.
4. The differential checks every conclusion on every ordered pair of schemas with at most two vertices and at most one edge (126 schemas, 15,876 pairs) under the three requests, and the property test on random pairs as in [L-migration-search-1](#L-migration-search-1).

### Executable counterpart

- Law ID: `migration-search.span-optimum`
- Reference: `panproto_reference::pages::migration_search::optimal_span`
- Reference: `panproto_reference::pages::migration_search::check_span`
- Reference: `panproto_reference::pages::migration_search::paired_edge_images`
- Enumerator: `panproto_reference::enumerate::migration_search::schemas; bound: vertices=2, edges=1`
- Exhaustive differential test: `span_search_returns_a_reference_optimum_on_every_bounded_pair; command: cargo nextest run -p panproto-mig --test reference_migration_search --no-tests=fail -E 'test(=span_search_returns_a_reference_optimum_on_every_bounded_pair)'`
- Property differential test: `randomized_span_searches_return_a_reference_optimum; command: cargo nextest run -p panproto-mig --test reference_migration_search --no-tests=fail -E 'test(=randomized_span_searches_return_a_reference_optimum)'`
- Reference test: `a_missing_kind_leaves_no_total_morphism_and_a_span_that_drops; command: cargo nextest run -p panproto-reference --test migration_search --no-tests=fail -E 'test(=a_missing_kind_leaves_no_total_morphism_and_a_span_that_drops)'`
- File: `crates/panproto-mig/src/span.rs`
- File: `crates/panproto-mig/src/solve/build.rs`

## Answers are a function of the schema pair {#L-migration-search-3}

### Statement

For schemas $S$ and $T$ and a request among those of [L-migration-search-1](#L-migration-search-1) and [L-migration-search-2](#L-migration-search-2), with default constraints, weights and budget, the ranked list `find_morphisms` returns (the order of its morphisms, each vertex and edge map, each quality's bits and `truncated`), and the span `SpanSearch::run` returns (its right leg, quality, coverage and tie-break order), depend only on the values of $S$ and $T$: not on the order in which their vertices and edges were added, not on the process, and not on the platform. On the determinism scope the 64-bit FNV-1a `digest` of their canonical rendering is the constant the test pins.

### Outside the domain

A budget with a wall-clock limit (`max_millis`) makes the answer depend on the machine, and the outcome then names `LimitKind::Time`; the law assumes no wall-clock limit. Schemas that are equal as values but differ in fields the search does not read are not distinguished, which the law permits.

### Definitions used

The canonical rendering lists, for each ordered pair and request in enumeration order, the vertex and edge maps as sorted maps, each quality and coverage as its 64 bits in hexadecimal, `truncated` and the tie-break order. The determinism scope is every ordered pair of schemas with at most two vertices and at most one edge.

### Proof

1. The network is built from the source vertices in ascending id order, each domain's targets in ascending id order, and the source edges in ascending order ([Searching for a morphism](../explanation/morphism-search.md#building-the-cost-function-network)), so it is a function of the two schemas' values.
2. Every algorithm that runs on it reads only the network and the budget; with no wall-clock limit its node and operation counts are properties of the input, so its result is a function of the network ([Searching for a morphism](../explanation/morphism-search.md#the-bounded-fallback)).
3. Edge images and the `iso` pairing compare candidate edges by value rather than by position in an index, and the vertex and edge maps are built from the assignment; the span's apex is induced and canonical. So the answers are functions of the pair's values.
4. Integer costs and the one binary64 conversion per entry make the costs exact and platform independent ([Searching for a morphism](../explanation/morphism-search.md#the-objective)).
5. The differential renders every answer of the scope with the schemas built in ascending and in descending insertion order and requires the two renderings to be equal, runs the rendering in four child processes, each with its own hash seed, and requires their digests to equal each other and the pinned constant; the property test compares the ascending order with the descending orders of either schema or both on random pairs.

### Executable counterpart

- Law ID: `migration-search.determinism`
- Reference: `panproto_reference::pages::migration_search::digest`
- Enumerator: `panproto_reference::enumerate::migration_search::schemas; bound: vertices=2, edges=1`
- Exhaustive differential test: `search_answers_are_the_same_in_every_process_and_insertion_order; command: cargo nextest run -p panproto-mig --test reference_migration_search --no-tests=fail -E 'test(=search_answers_are_the_same_in_every_process_and_insertion_order)'`
- Property differential test: `randomized_insertion_orders_give_the_same_answers; command: cargo nextest run -p panproto-mig --test reference_migration_search --no-tests=fail -E 'test(=randomized_insertion_orders_give_the_same_answers)'`
- Reference test: `the_digest_is_fnv_1a; command: cargo nextest run -p panproto-reference --test migration_search --no-tests=fail -E 'test(=the_digest_is_fnv_1a)'`
- File: `crates/panproto-mig/src/span.rs`
- File: `crates/panproto-mig/src/hom_search.rs`

## A bounded search answers or names its bound {#L-migration-search-4}

### Statement

Let $S$ and $T$ be schemas, $\rho$ a request of [L-migration-search-1](#L-migration-search-1) and $\beta$ a budget with no wall-clock limit. Then `find_morphisms_budgeted(S, T, ρ, default constraints, β)` returns one of: `Ok` with an empty list, only when `optimal_total(S, T, ρ)` is empty; `Ok` with morphisms that each pass `check_total` and each have the optimal cost of `optimal_total(S, T, ρ)`; `SpanError::Stopped { limit, incumbent }` with `limit` naming a bound $\beta$ lowered (`Nodes` when $\beta$ sets `max_nodes`, `Operations` when its `op_budget` is below the default) and `incumbent`, when present, a morphism that passes `check_total`; or a memory refusal (`SpanError::Build` wrapping `CfnError::OverMemoryBudget`, or `SpanError::Iso` wrapping `IsoError::OverMemoryBudget`) when its `mem_bytes` is below the default. Under the same budget `SpanSearch::run` returns a span that passes `check_span` and either is proven optimal and of the reference's optimal cost, or is not proven optimal and names a lowered bound in `limit_hit`; or it returns a memory refusal under a lowered memory budget.

### Outside the domain

A wall-clock limit is excluded because whether it is reached depends on the machine; when it is, the outcome names `LimitKind::Time` in the same places. The incumbent a stopped total search carries passes the checker but is not claimed optimal, and the law says nothing about which incumbent it is. Domain restrictions and the other inputs not modelled are those of [L-migration-search-1](#L-migration-search-1).

### Definitions used

A bound is lowered when it is below the value [`SearchBudget::default`](https://docs.rs/panproto-mig/latest/panproto_mig/solve/struct.SearchBudget.html) gives it. The refusal scope is every ordered pair of schemas with at most two vertices and one edge under twelve budgets: the default; `max_nodes` of 0, 1, 2 and 4; `op_budget` of 0, 1, 8, 64 and 512; and `mem_bytes` of 0 and 64.

### Proof

1. The total-morphism search with `⊥` forbidden makes feasibility exactly totality, so a branch and bound that reaches a complete assignment holds a total morphism. Whenever a limit stops it, the search returns `Stopped` with the limit it hit and, as `incumbent`, the morphism of the assignment it held, if any ([Searching for a morphism](../explanation/morphism-search.md#the-bounded-fallback)); that morphism is built by step 5 of [L-migration-search-1](#L-migration-search-1) and so passes the checker. The `iso` path returns `Stopped` whenever a limit stops it, carrying the isomorphism it held when its incumbent is total and onto, and nothing when the incumbent is a partial map.
2. Exact inference is priced against the memory and operation budgets before it runs and otherwise does not consult a budget, so it never stops; a pair it cannot price within the budget goes to branch and bound, which counts nodes and operations and stops on whichever it exhausts first. Each counter compares against its own bound, so the limit reported is a bound the caller lowered.
3. Building the network and sizing the `iso` frames are checked against `mem_bytes` before any allocation, and they refuse with the measured figure rather than fall back.
4. `Ok` is returned only by a search no limit stopped, or by the cardinality tests of the `epic` path. An empty list then means the search ran to completion without a complete assignment in the requested class, or the cardinality test excluded every one; in either case no such assignment exists, which is the emptiness of `optimal_total`. A non-empty list comes from a search that ran to completion, so steps 3 to 5 of [L-migration-search-1](#L-migration-search-1) apply under $\beta$ as under the default budget: each morphism passes the checker and attains the optimal cost.
5. The differential checks every case on the refusal scope (1,524,096 searches over five total and three span requests) and the property test on random pairs and random node, operation and memory budgets. The whole scope runs in four shards in the `exhaustive-reference` job of the corpus gate, each taking the pairs whose position is congruent to its index modulo four, and one sixteenth of the pairs runs on every pull request.

### Executable counterpart

- Law ID: `migration-search.bounded-refusal`
- Reference: `panproto_reference::pages::migration_search::optimal_total`
- Reference: `panproto_reference::pages::migration_search::check_total`
- Reference: `panproto_reference::pages::migration_search::check_span`
- Reference: `panproto_reference::pages::migration_search::cost`
- Enumerator: `panproto_reference::enumerate::migration_search::schemas; bound: vertices=2, edges=1, budgets=12`
- Exhaustive differential test: `a_bounded_search_answers_or_names_its_bound_on_every_bounded_pair; command: cargo nextest run -p panproto-mig --test reference_migration_search --no-tests=fail -E 'test(=a_bounded_search_answers_or_names_its_bound_on_every_bounded_pair)'`
- Property differential test: `randomized_bounded_searches_answer_or_name_their_bound; command: cargo nextest run -p panproto-mig --test reference_migration_search --no-tests=fail -E 'test(=randomized_bounded_searches_answer_or_name_their_bound)'`
- Reference test: `the_checker_finds_each_defect; command: cargo nextest run -p panproto-reference --test migration_search --no-tests=fail -E 'test(=the_checker_finds_each_defect)'`
- File: `crates/panproto-mig/src/hom_search.rs`
- File: `crates/panproto-mig/src/solve/mod.rs`
