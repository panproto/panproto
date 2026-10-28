# Restrict and contraction

Restriction removes instance data that a migration does not carry forward, while contraction reconnects surviving nodes across the removed part. The construction is delicate for a specific reason: the view may contain nodes and edges that have no single source element. The implementation therefore compiles a schema-level plan first, evaluates that plan in a fixed stage order, and records enough residue to distinguish retained source data from synthesized presentation.

This page states thirteen laws. [L-restrict-and-contraction-1](#L-restrict-and-contraction-1) fixes the total edge image; [L-restrict-and-contraction-2](#L-restrict-and-contraction-2) and [L-restrict-and-contraction-3](#L-restrict-and-contraction-3) fix contraction planning and pair resolution; [L-restrict-and-contraction-4](#L-restrict-and-contraction-4) fixes graph execution. [L-restrict-and-contraction-5](#L-restrict-and-contraction-5) through [L-restrict-and-contraction-12](#L-restrict-and-contraction-12) state the cross-shape transport obligations. [L-restrict-and-contraction-13](#L-restrict-and-contraction-13) isolates the exact reachability claim.

Let $S$ and $T$ be source and target schemas, let $m : S \to T$ be a compiled migration, and let $G$ be a finite source graph. A source vertex is *endpoint-capable* when it survives $m$. It is *interior-capable* when it is dropped by $m$ or has a conditional-survival predicate; a conditional vertex is therefore both endpoint-capable and interior-capable. A source node survives when its anchor is endpoint-capable and, where required, its predicate accepts the node in the original graph. A *realized pair* $(x,y)$ consists of surviving source nodes joined by a nonempty directed walk whose internal nodes include at least one dropped non-fan node and are all dropped non-fan nodes.

Cross-shape statements quantify only over the validated common conversion image. Thus a tree, its graph encoding, and a table-shaped graph are compared only when the relevant encodings and schema-relative shape checks succeed; no law identifies malformed carriers merely because they can be stored by one raw representation.

The reference operations live in [`panproto_reference::restrict`](https://docs.rs/panproto-reference/latest/panproto_reference/restrict/) and [`panproto_reference::lens`](https://docs.rs/panproto-reference/latest/panproto_reference/lens/). The release carrier contains 57 schemas crossed with 27 survival classifications, 375 pair-resolution cases, 12,366 graph cases, 585 ordered child-binding words, nine column-fate cases, and 70 tree cases. The tree-edit carrier adds 38 get words, 27 put words, two typed lowering refusals, and one atomic late refusal.

The final seven-test production/reference suite passed in nextest run `48b515aa-2743-41e9-b7d0-deb5764ec3e5`. The two lens-level tree-complement conversion tests passed in run `780bbc32-545b-469b-bc55-8f0304c32815`, and the 42 affected reference tests passed in run `0e864e92-d9cc-4729-91d1-8179545c5db9`. Strict Clippy passed for both production differentials, the lens differential, and the affected reference targets. The audit records retain the separate requirement for an independent verifier.

## The edge image is total and endpoint coherent {#L-restrict-and-contraction-1}

### Statement

For every declaration-valid $S$, $T$, and $m$, `edge_image` assigns exactly one fate to every source schema edge $e : a \to b$. If either endpoint is not endpoint-capable, the fate is `Drop`. Otherwise the fate is, in order: `Keep(e')` for the direct image; the result of pair resolution when an expansion path is declared for $(m(a),m(b))$; or `Drop`. A kept label is an edge of $T$ from $m(a)$ to $m(b)$. An expansion is a path in $T$ from $m(a)$ to $m(b)$, with one label per hop. The least incoherent image is refused during plan compilation, and an expansion with more than $2^{16}$ hops is refused before a hop index is converted to `u16`.

### Outside the domain

Invalid source or target declarations are refused before the edge image is compiled. A direct or expanded image that is absent from $T$, has the wrong endpoints, or has a label that does not join its adjacent anchors is refused as `IncoherentEdgeImage`. Failure to resolve an expansion hop is data in the plan (`Unresolved`), not a compilation refusal; execution reports that failure only when an instance edge has the fate.

### Definitions used

The *direct image* of $e$ is its explicit edge remap when present; otherwise it is an edge of $T$ with the same kind and name between the endpoint images; otherwise, when $e$ is the only source edge between its endpoints, it is the endpoint resolver entry. Pair resolution is defined by [L-restrict-and-contraction-3](#L-restrict-and-contraction-3).

### Proof

`edge_image` traverses the ordered set of source edges once and inserts one map entry per edge. Its branch order is the order in the Statement. After computing a fate, it checks every `Keep` label against the endpoint images and calls `coherent_chain` on every expansion. The map therefore has the source edge set as its exact domain, and every non-dropped image is endpoint coherent. `check_plan_expansion` runs before the map is returned, which establishes the hop bound. The differential compares the reference map with production on all $57 \times 27 = 1{,}539$ identity-schema survival cases. Six additional compact witnesses isolate explicit remap, same-label fallback, endpoint resolution, expansion, unresolved expansion, and incoherent-image refusal. The existing boundary units independently pin acceptance at 65,536 hops and refusal at 65,537; the release differential does not duplicate a 65,000-element carrier.

### Executable counterpart

- Law ID: `restrict.f-r`
- Reference: `panproto_reference::restrict::get`
- Reference: `panproto_reference::restrict::edge_image`
- Enumerator: `panproto_reference::enumerate::restrict_and_contraction::release_schemas` and `survival_classifications`; exact carrier: 1,539 schema/classification cases, plus 6 explicit nonidentity branch witnesses
- Exhaustive differential tests: `edge_images_and_contraction_regions_agree_on_the_complete_release_scope` and `graph_get_and_covered_reachability_agree_on_three_node_carriers` in `crates/panproto-inst/tests/reference_restrict.rs`
- Boundary pins: production `restrict_plan_compile` and reference `lenses` accept 65,536 hops and refuse 65,537
- Dynamic result: the final 7/7 suite passed in nextest run `48b515aa-2743-41e9-b7d0-deb5764ec3e5`
- Evidence scope: `1,539 schema/classification cases, six nonidentity branch witnesses, and the 65,536/65,537-hop boundary`
- Production certificate: `edge_images_and_contraction_regions_agree_on_the_complete_release_scope; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=edge_images_and_contraction_regions_agree_on_the_complete_release_scope)'`
- Independent reference anchor: `graph_get_and_covered_reachability_agree_on_three_node_carriers; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=graph_get_and_covered_reachability_agree_on_three_node_carriers)'`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`
- File: `crates/panproto-reference/src/enumerate/restrict_and_contraction.rs`
- File: `crates/panproto-inst/src/contraction_plan.rs`

### Mutation equivalences

None. Mutating direct-image precedence, endpoint coherence, unresolved-hop retention, or the expansion bound must be observed independently.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## The contraction plan is the trimmed dropped-interior relation {#L-restrict-and-contraction-2}

### Statement

For each ordered pair $(a,b)$ of endpoint-capable source vertices, the contraction plan contains $(a,b)$ exactly when there is a directed walk from $a$ to $b$ with at least one internal vertex and every internal vertex is interior-capable. Its region contains exactly the source edges lying on some such walk. More explicitly, let $F_a$ contain the interior-capable vertices reachable from $a$ while staying inside the interior-capable set, and let $B_b$ contain the interior-capable vertices from which $b$ is reachable under the same restriction. An edge $u \to v$ belongs to the region exactly when $(u \in F_a \land (v=b \lor v\in B_b))$ or $(u=a \land v\in B_b)$. The pair is bounded exactly when the subgraph of its region induced by interior-capable vertices is acyclic. Its target is $(m(a),m(b))$, and its resolution is [L-restrict-and-contraction-3](#L-restrict-and-contraction-3).

### Outside the domain

The law applies after declaration validation. An incoherent pair label or expansion is refused as `IncoherentPairImage`; an overlong expansion is refused as `ExpansionTooLong`. An unresolved pair remains in the plan so that tooling can inspect it and execution can return a typed error if the pair is realized.

### Definitions used

Endpoint-capable and interior-capable vertices are defined at the head of this page. A directed walk may repeat vertices and edges. Boundedness concerns only cycles wholly inside the interior-capable part of the region; cycles that require an endpoint do not make the pair unbounded.

### Proof

The forward search `reach(a, interior, succ)` computes $F_a$ by the usual visited-set fixed point. A pair is emitted only when an edge from $F_a$ enters $b$, which is equivalent to the required walk. The reverse search computes $B_b$. The filter used to build `region` is the formula in the Statement, so it retains exactly the edges that lie on an $a$-to-$b$ walk through the intersection of forward and backward interior reachability. `has_cycle` repeatedly removes a vertex with no live predecessor; a finite directed graph survives that elimination exactly when it contains a directed cycle. The differential checks the production and reference pair relation, region, and bounded bit, then derives the same region independently by explicit walk enumeration.

### Executable counterpart

- Law ID: `restrict.c1`
- Reference: `panproto_reference::restrict::contraction_plan`
- Enumerator: `release_schemas` crossed with `survival_classifications`; exact carrier: $57 \times 27 = 1{,}539$ cases
- Exhaustive differential test: `edge_images_and_contraction_regions_agree_on_the_complete_release_scope`
- Independent derivation: explicit bounded walk enumeration reconstructs every contraction region
- Evidence scope: `57 release schemas crossed with 27 survival classifications, for 1,539 cases`
- Production certificate: `edge_images_and_contraction_regions_agree_on_the_complete_release_scope; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=edge_images_and_contraction_regions_agree_on_the_complete_release_scope)'`
- Independent reference anchor: `edge_images_and_contraction_regions_agree_on_the_complete_release_scope; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=edge_images_and_contraction_regions_agree_on_the_complete_release_scope)'`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`
- File: `crates/panproto-reference/src/enumerate/restrict_and_contraction.rs`
- File: `crates/panproto-inst/src/contraction_plan.rs`

### Mutation equivalences

None. In particular, replacing the walk region by an endpoint-induced subgraph, treating a conditional vertex as endpoint-only, or testing cycles outside the interior subgraph changes the law.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## Pair resolution has one fixed precedence order {#L-restrict-and-contraction-3}

### Statement

For target anchors $a'$ and $b'$, pair resolution first takes `resolver[(a',b')]` when present. Without that entry, it takes the unique target edge from $a'$ to $b'$. If there are zero or several such edges and an expansion path is declared, it resolves each adjacent hop by that hop's resolver entry and otherwise by the unique edge for the hop. It returns the expansion when every hop resolves, or the first failing hop with `NoEdge` or `Ambiguous { count }`. Without an expansion path it returns the original direct-pair failure. Resolution of an expansion hop never recursively consults another expansion path.

### Outside the domain

There is no partial input domain: every pair of strings produces `Label`, `Expand`, or `Unresolved`. Coherence with a particular target schema is checked by the caller during plan compilation.

### Definitions used

The *unique target edge* is the sole member of `edges_between(a', b')`, regardless of its name. The declared expansion path supplies intermediate anchors; its labels are derived by resolving adjacent pairs rather than stored independently.

### Proof

`resolve_pair` calls `resolve_edge` first. That helper tests the resolver map before inspecting `edges_between` and returns a typed zero-or-many failure. Only that failure enters the expansion branch. The loop over consecutive anchors appends one resolved label per hop and returns immediately on the first failure. No call in the loop reaches `resolve_pair`, so expansion is not recursive. These branches are disjoint and exhaustive.

### Executable counterpart

- Law ID: `restrict.c2`
- Reference: `panproto_reference::restrict::resolve_pair`
- Enumerator: `pair_resolution_cases`; exact carrier: 375 cases
- Exhaustive differential test: `pair_resolution_precedence_and_first_failure_agree`; census: 225 labelled resolutions, 18 expansions, and 132 typed unresolved results
- Failure pin: production and reference identify the same first failing expansion hop
- Evidence scope: `375 pair-resolution cases: 225 labelled resolutions, 18 expansions, and 132 typed unresolved results`
- Production certificate: `pair_resolution_precedence_and_first_failure_agree; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=pair_resolution_precedence_and_first_failure_agree)'`
- Independent reference anchor: `pair_resolution_precedence_and_first_failure_agree; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=pair_resolution_precedence_and_first_failure_agree)'`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`
- File: `crates/panproto-reference/src/enumerate/restrict_and_contraction.rs`
- File: `crates/panproto-inst/src/contraction_plan.rs`

### Mutation equivalences

None. Resolver-first selection, unique-edge fallback, and the prohibition on recursive expansion are observable.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## Graph restriction is staged Delta followed by contraction {#L-restrict-and-contraction-4}

### Statement

On a declaration-valid plan and a graph in the fan domain, graph get evaluates in this order: non-fan nodes by ascending id; non-fan edges by ascending id; realized contraction pairs by ascending pair; fan nodes by ascending id; synthesized allocation by ordered synthesis key. A surviving node keeps its id, takes its target anchor, and receives its declared transform. A source edge whose endpoints survive follows the compiled fate: `Keep` retains id and position with the image label, `Drop` enters the complement, and `Expand` enters the complement while its path is synthesized. Every realized pair produces one labelled edge or one expansion path. Fans are evaluated after contraction. The first typed error in stage order is returned. Synthesized node and edge ids begin at their separate source floors, increase in synthesis-key order, and never equal `u32::MAX`.

### Outside the domain

Plan mismatch is refused before evaluation. A graph outside the fan domain is refused at its least offending node. An undeclared instance label between surviving endpoints, an unresolved edge fate or pair, a predicate or transform refusal, a fan-label collision, an overlong expansion, and id exhaustion each produce the typed refusal at the stage and element specified by `ErrorKey`.

### Definitions used

The *fan domain* consists of graphs in which every hyper-edge-shaped node and every source of a reserved fan edge participates in a well-formed fan. The node floor is zero for an empty node set and otherwise one above the greatest source node id, saturating at the reserved maximum; the edge floor is analogous. Synthesis keys distinguish direct contraction, source-edge expansion, and contraction expansion.

### Proof

`get_with_plan` validates the plan stamp and fan domain before it constructs `Run`. It records both floors, then calls `nodes`, `edges`, `contract`, `fans`, `allocate`, and `record` in the order stated. Each stage iterates a `BTreeMap` or `BTreeSet`, hence ascending key order. The node and edge stages implement the compiled fates directly. `realized_pairs` uses one visited set per surviving source and inserts pairs into an ordered set, so cycles terminate and duplicate witnesses collapse. Allocation sorts pending objects by synthesis key and draws from half-open ranges ending before `u32::MAX`. The 12,366-case differential compares the complete view, complement, synthesis keys, floors, and typed refusals with production.

### Executable counterpart

- Law ID: `restrict.c3`
- Reference: `panproto_reference::restrict::get`
- Reference: `panproto_reference::restrict::edge_image`
- Enumerator: `release_graphs`; exact carrier: 12,366 graph/plan cases
- Exhaustive differential tests: `edge_images_and_contraction_regions_agree_on_the_complete_release_scope` and `graph_get_and_covered_reachability_agree_on_three_node_carriers`
- Compared result: complete view, complement, id floors, synthesis keys, and deterministic typed errors
- Evidence scope: `12,366 graph/plan cases from the release graph carrier`
- Production certificate: `graph_get_and_covered_reachability_agree_on_three_node_carriers; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=graph_get_and_covered_reachability_agree_on_three_node_carriers)'`
- Independent reference anchor: `edge_images_and_contraction_regions_agree_on_the_complete_release_scope; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=edge_images_and_contraction_regions_agree_on_the_complete_release_scope)'`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`
- File: `crates/panproto-reference/src/enumerate/restrict_and_contraction.rs`
- File: `crates/panproto-inst/src/graph_restrict.rs`

### Mutation equivalences

None. The stage order, id retention, set semantics for realized pairs, and separate node and edge floors are part of the observable result.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## Tree restriction uses the compiled edge image {#L-restrict-and-contraction-5}

### Statement

For a tree and graph that encode the same source carrier, tree restriction and graph restriction make the same decision for every direct source arc. Both read the fate from the compiled edge image. `Keep` preserves the carrier under its target label, `Drop` removes it, `Expand` replaces it by the same labelled path, and `Unresolved` returns the same missing-or-ambiguous-hop refusal.

### Outside the domain

The comparison requires a well-formed tree embedding and a plan valid for the same schemas and migration. Tree-specific root pruning is outside the graph result and is compared only after projecting the graph result back through the embedding.

### Definitions used

Two carriers are *embedding-equivalent* when tree-to-graph encoding identifies the tree nodes and arcs with the graph nodes and structural edges. “The same refusal” means the same typed cause and failing target anchors; wrappers may add shape-specific location data.

### Proof

`tree_get` lowers the ordered tree carrier to the graph rule, applies the same compiled edge image, and lifts the result with its synthesis data. The differential builds both plans from the same schemas and migration, then checks every direct and expanded route on the 70-case tree carrier. Explicit drop and unresolved witnesses additionally run through both graph and tree engines and compare the resulting view or typed cause. Thus the production tree engine and the reference tree oracle consume the same edge fate rather than recomputing a shape-local substitute.

### Executable counterpart

- Law ID: `restrict.tr6`
- Reference: `panproto_reference::lens::tree_get`
- Enumerator: `tree_restrict_cases`; exact carrier: 14 nonempty keep/expand route words crossed with five id-floor shapes, for 70 cases, plus explicit `Drop` and `Unresolved` graph/tree witnesses
- Exhaustive differential test: `tree_get_uses_the_graph_plan_keys_floor_and_sibling_order`; census: 43 successes and 27 typed node-id exhaustion refusals
- Evidence scope: `70 keep/expand route-and-floor cases plus explicit Drop and Unresolved witnesses`
- Production certificate: `tree_get_uses_the_graph_plan_keys_floor_and_sibling_order; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=tree_get_uses_the_graph_plan_keys_floor_and_sibling_order)'`
- Independent reference anchor: `tree_get_uses_the_graph_plan_keys_floor_and_sibling_order; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=tree_get_uses_the_graph_plan_keys_floor_and_sibling_order)'`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`
- File: `crates/panproto-reference/src/enumerate/restrict_and_contraction.rs`
- File: `crates/panproto-inst/src/wtype.rs`
- File: `crates/panproto-inst/src/graph_restrict.rs`

### Mutation equivalences

None. Recomputing a tree-only fate is forbidden even when it agrees on current fixtures.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## Column fate is derived from the schema plan {#L-restrict-and-contraction-6}

### Statement

For every surviving record, graph and table restriction remove or rename a cell exactly when the compiled cell plan removes or renames the corresponding attribute arrow. A dropped target vertex removes the cell. A retained attribute whose compiled edge image is renamed changes the cell name once. Tree restriction obtains the same result by applying the corresponding fate to the child carrier. No engine derives column fate by inspecting instance labels after compilation.

### Outside the domain

The law is stated for schemas that admit the relational attribute interpretation used to compile the cell plan. A disagreement between entity survival and graph survival is a plan-compilation refusal rather than an instance-level choice.

### Definitions used

An *attribute arrow* is a schema edge from an entity vertex to its value vertex. A *cell plan* is the per-entity ordered map of `Drop` and `Rename` fates compiled from those arrows.

### Proof

`column_fates` derives the ordered `Drop` and `Rename` list from the reference plan, source columns, and target columns. The differential compiles the same two-column schema migration through production and reference paths for every keep/drop/rename assignment. It compares the ordered result without sorting by fate, so the test also pins source-column order. It then executes one row carrying both cells through the table engine, the graph form of that table, and the corresponding tree, checking the materialized result in all nine assignments.

### Executable counterpart

- Law ID: `restrict.tr7`
- Reference: `panproto_reference::restrict::column_fates`
- Enumerator: `column_fate_cases`; exact carrier: $3^2=9$ keep/drop/rename assignments
- Exhaustive differential test: `column_fates_are_derived_from_the_same_schema_plan`; 9 compiled-plan comparisons and 27 runtime shape observations
- Evidence scope: `nine keep/drop/rename assignments and 27 runtime shape observations`
- Production certificate: `column_fates_are_derived_from_the_same_schema_plan; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=column_fates_are_derived_from_the_same_schema_plan)'`
- Independent reference anchor: `column_fates_are_derived_from_the_same_schema_plan; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=column_fates_are_derived_from_the_same_schema_plan)'`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`
- File: `crates/panproto-reference/src/enumerate/restrict_and_contraction.rs`
- File: `crates/panproto-inst/src/restrict_plan.rs`
- File: `crates/panproto-inst/src/table_restrict.rs`
- File: `crates/panproto-inst/src/wtype.rs`

### Mutation equivalences

None. Instance-level label matching is not equivalent to consuming the compiled cell plan.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes after correction of the reference-only result-order defect.

## Repeated child names bind an ordered list {#L-restrict-and-contraction-7}

### Statement

When several non-self outgoing children of one element have the same field name, the transform environment treats that name as repeated and binds it to a list in carrier order. A scalar demand on that binding is refused as `AmbiguousChildField`. Tree and graph nodes contribute child-node scalar values; a table relation contributes its child rows, which have no node scalar value, so the repeated list may be empty. The common claim is repetition, order, duplicate preservation, and typed scalar refusal, not equality of payloads that one shape cannot represent.

### Outside the domain

Self-loops do not contribute child values. A structural child that cannot be read in the required shape and a value outside the expression carrier produce their own typed predicate refusals.

### Definitions used

For graphs, *edge order* is ascending edge id. For trees it is sibling order; for tables it is the order fixed by the compiled relation carrier. The list preserves duplicates.

### Proof

`scalar_child_bindings` walks the stored structural children, excludes self-loops and absent values, and collects repeated names into ordered lists without removing duplicates. The differential constructs the same word as a production tree and a reference tree, then compares the complete binding map. The carrier contains every word of length zero through three over the eight binding-slot states, for 585 cases. A separate two-child `x,x` witness runs through tree, graph, table, and the table's graph form; each engine then evaluates a transform that demands scalar `x` and returns the real `AmbiguousChildField` refusal.

### Executable counterpart

- Law ID: `restrict.tr8`
- Reference: `panproto_reference::restrict::scalar_child_bindings`
- Enumerator: `child_binding_cases`; exact carrier: 585 ordered words
- Exhaustive differential test: `repeated_child_bindings_agree_and_preserve_order_and_duplicates`
- Evidence scope: `585 ordered child-binding words`
- Production certificate: `repeated_child_bindings_agree_and_preserve_order_and_duplicates; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=repeated_child_bindings_agree_and_preserve_order_and_duplicates)'`
- Independent reference anchor: `repeated_child_bindings_agree_and_preserve_order_and_duplicates; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=repeated_child_bindings_agree_and_preserve_order_and_duplicates)'`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`
- File: `crates/panproto-reference/src/enumerate/restrict_and_contraction.rs`
- File: `crates/panproto-inst/src/element_ops.rs`
- File: `crates/panproto-inst/src/wtype.rs`

### Mutation equivalences

None. Last-wins, first-wins, set-valued, and self-loop-inclusive collection are distinct semantics.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## Tree synthesis uses the graph synthesis keys and floor {#L-restrict-and-contraction-8}

### Statement

Tree and graph get name corresponding synthesized carriers with the same `SynthKey` variant and fields. Within each carrier kind, ids are allocated in ascending synthesis-key order beginning at one above the greatest source id of that kind, including fan carriers. The reserved id `u32::MAX` is never allocated. Read the graph view as a tree and retain the resulting graph-to-tree embedding. `tree_complement_to_graph` converts the actual tree complement under the recorded source and view embeddings. The result equals graph get's complete `GraphComplement`, including synthesized nodes, synthesized edges, identity, and both allocation floors. The tree complement's node floor and the source embedding's edge floor equal the converted complement's two floors. A fresh canonical embedding of the tree view is not substituted here, since its arc ids are presentation-relative rather than the graph view's ids.

### Outside the domain

When no non-reserved id remains, get refuses with the appropriate node- or edge-id exhaustion error. A tree that has no graph embedding is outside the comparison.

### Definitions used

The three synthesis-key variants are `Contract { src, tgt, edge }`, `Expand { source_edge, hop }`, and `ContractExpand { src, tgt, hop }`. Node and edge floors are independent.

### Proof

Reference graph allocation is proved in [L-restrict-and-contraction-4](#L-restrict-and-contraction-4), and `tree_get` carries the synthesis map and node floor into its tree complement. The lens-level differential calls production tree get and graph get, reads the graph view through `to_wtype_embedded`, and verifies that this tree is the production tree view. It then calls public `tree_complement_to_graph` with the real source graph, graph view, tree complement, and both embeddings. The converted complement is compared field by field, including its identity and complete `SynthIndex`, with graph get and the independent reference complement. The raw `Complement::synth_index` is not compared across representations: its documented tree coordinates intentionally let `contraction_choices` classify expansion presentation arcs as `Contract`, while the public conversion translates them to graph `Expand` or `ContractExpand` keys. The differential separately compares the tree node floor and source-embedding edge floor with the reference pair. The 70-case comparison ranges over compact, gapped, last-free, saturated, and already-maximum id shapes: 43 cases succeed and 27 agree on typed node-id exhaustion. Five additional successful observations cover `Contract`, `Expand`, and `ContractExpand`, with compact, gapped, and last-free contraction-expansion floors.

### Executable counterpart

- Law ID: `restrict.tr9`
- Reference: `panproto_reference::lens::tree_get`
- Enumerator: `tree_restrict_cases`; exact carrier: 70 route/floor cases plus 5 structural-key observations
- Exhaustive differential tests: `tree_get_uses_the_graph_plan_keys_floor_and_sibling_order`, `tree_restrict_synth_index_matches_graph_and_reference_on_the_70_case_carrier`, and `tree_restrict_synth_index_covers_all_structural_keys_and_successful_floors`; census: 43 successes and 27 typed exhaustion refusals on the enumerated carrier, plus 5 successful structural-key observations
- Dynamic result: the two lens-level conversion tests passed in nextest run `780bbc32-545b-469b-bc55-8f0304c32815`
- Evidence scope: `70 route/floor cases and five successful structural-key observations`
- Production certificate: `tree_restrict_synth_index_matches_graph_and_reference_on_the_70_case_carrier; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=tree_restrict_synth_index_matches_graph_and_reference_on_the_70_case_carrier)'`
- Independent reference anchor: `tree_restrict_synth_index_covers_all_structural_keys_and_successful_floors; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=tree_restrict_synth_index_covers_all_structural_keys_and_successful_floors)'`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`
- File: `crates/panproto-reference/src/enumerate/restrict_and_contraction.rs`
- File: `crates/panproto-inst/src/wtype.rs`
- File: `crates/panproto-inst/src/complement.rs`
- File: `crates/panproto-lens/tests/reference_lenses.rs`

### Mutation equivalences

None. Renumbering by traversal order is not equivalent, since synthesis ids and keys are durable complement coordinates.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## Tree put checks predicates and transforms before restoration {#L-restrict-and-contraction-9}

### Statement

Given a complement produced by tree get, tree put admits a view only when every restored node satisfies its source survival predicate and every edited transformed or derived coordinate has a declared inverse. Predicate evaluation failures remain typed as predicate refusals. A transform without an inverse is `TransformNotInvertible`; an edited derived cell is `DerivedCellEdited`. Put returns the first violation in its documented domain order and restores only after the complete check succeeds.

### Outside the domain

Views with an id collision, edited synthesized carrier, unrepresentable edge, ambiguous reverse anchor, predicate refusal, non-invertible transform, invalid key, missing required cell, or malformed fan are outside the put domain. Identity-envelope mismatch is refused before these clauses.

### Definitions used

The *restored node* is the source candidate obtained from the view node plus recorded original anchor, fields, and value. A *derived coordinate* is a field introduced by the forward transform without source storage.

### Proof

`TreeShape::check_put_domain` checks the candidate view and complement before `tree_put` restores source data. The bounded certificate calls both operations through the reference lens and the production dispatcher, comparing the accepted source or the first typed refusal. The graph-square test supplies 31,422 observations; the dedicated tree put certificate supplies 2,548 law-basis observations.

### Executable counterpart

- Law ID: `restrict.tr10`
- Reference: `panproto_reference::lens::TreeShape::check_put_domain`
- Reference: `panproto_reference::lens::tree_put`
- Exhaustive differential tests: `graph_square_agrees_with_the_reference_exhaustively` and `tree_put_law_certificate` in `crates/panproto-lens/tests/reference_lenses.rs`
- Exact observations: 31,422 graph-square observations and 2,548 tree-law basis observations
- Evidence scope: `31,422 graph-square observations and 2,548 tree-law basis observations`
- Production certificate: `tree_put_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=tree_put_law_certificate)'`
- Independent reference anchor: `graph_square_agrees_with_the_reference_exhaustively; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=graph_square_agrees_with_the_reference_exhaustively)'`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`
- File: `crates/panproto-reference/src/enumerate/restrict_and_contraction.rs`
- File: `crates/panproto-lens/src/asymmetric.rs`
- File: `crates/panproto-inst/src/wtype.rs`

### Mutation equivalences

None. Checking predicates during restoration is not equivalent to checking the complete domain before mutation.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## Tree restriction participates in the complete lens-law gates {#L-restrict-and-contraction-10}

### Statement

For every tree source on which get succeeds, the corresponding tree lens is checked by the same five gates as the graph and table lenses: exact GetPut, PutGet under the declared target equivalence, totality on admitted single-step mutations, gated PutTwice, and congruence under coherent carrier renaming. A shape dispatcher must execute the tree gates rather than substituting graph results or omitting tree-only mutations.

### Outside the domain

Get refusals do not enter the law domain. Mutations rejected by the put-domain predicate are witnesses of partiality, not totality failures. A law gate may return the first operation error or law violation in gate order.

### Definitions used

The five gates and coherent renaming are defined on the [lenses](./lenses.md) correctness page. The tree single-step basis includes every tree mutation family admitted by the public law checker.

### Proof

`check_tree_laws` executes the five gates using the reference get, put, domain, and equivalence operations over `TreeShape::single_step_basis`. The production dispatcher runs the corresponding gates on the same labelled carrier. The exhaustive comparison contains 200,606 heterogeneous lens-law cases; the graph-square and tree-put certificates provide separate pins for the operations from which those laws are assembled.

### Executable counterpart

- Law ID: `restrict.tr11`
- Reference: `panproto_reference::lens::check_tree_laws`
- Reference: `panproto_reference::lens::TreeShape::single_step_basis`
- Exhaustive differential tests: `lens_laws_agree_with_the_reference_exhaustively` and `graph_square_agrees_with_the_reference_exhaustively`
- Exact observations: 200,606 heterogeneous law cases, plus the 31,422-observation graph square
- Dynamic result: 3/3 focused tests passed in nextest run `0bb2dd47-9b02-43a8-9c51-f84c32977dc9`
- Evidence scope: `200,606 heterogeneous law cases and the 31,422-observation graph square`
- Production certificate: `lens_laws_agree_with_the_reference_exhaustively; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=lens_laws_agree_with_the_reference_exhaustively)'`
- Independent reference anchor: `graph_square_agrees_with_the_reference_exhaustively; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=graph_square_agrees_with_the_reference_exhaustively)'`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`
- File: `crates/panproto-reference/src/enumerate/restrict_and_contraction.rs`
- File: `crates/panproto-lens/src/laws.rs`
- File: `crates/panproto-lens/src/dispatch.rs`

### Mutation equivalences

None. A graph-only gate is not an observational substitute for a tree gate.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## Tree edit lenses preserve replacement and functoriality laws {#L-restrict-and-contraction-11}

### Statement

Tree edit-lens get and put preserve the replacement semantics of element identity: deleting an id and adding that id creates a new element, so old residue does not attach to it; inversion and complement restoration recreate the original element when asked. Translation of edits through restriction preserves identity and composition on admitted edits, and it agrees with recomputation of the restricted view.

### Outside the domain

An edit rejected by the source editor, target editor, or lens put domain has no translated result. Fresh allocation never reuses an id below the recorded floor.

### Definitions used

Replacement semantics and edit-lens functoriality are part of the registered edits contract. An edit is *admitted* when its application succeeds and the resulting view lies in the put domain.

### Proof

The page-local tree-edit oracle defines lowering, edit translation in both directions, composition, inversion, application, and minimality independently of the production edit lens. The differential compares 38 get words and 27 put words, then pins 15 exact tree-edit inverses and 12 cases whose canonical inverse exists only as a graph delta. Two state-reading lowering failures and one late sequence failure agree on typed refusal and leave both states unchanged.

### Executable counterpart

- Law ID: `restrict.tr12`
- Reference: `panproto_reference::pages::restrict_and_contraction::tree_edit_lens::lower`
- Reference: `panproto_reference::pages::restrict_and_contraction::tree_edit_lens::Lens::get_edit`
- Reference: `panproto_reference::pages::restrict_and_contraction::tree_edit_lens::Lens::put_edit`
- Reference: `panproto_reference::pages::restrict_and_contraction::tree_edit_lens::Delta::compose`
- Reference: `panproto_reference::pages::restrict_and_contraction::tree_edit_lens::Delta::invert`
- Reference: `panproto_reference::pages::restrict_and_contraction::tree_edit_lens::Delta::apply`
- Reference: `panproto_reference::pages::restrict_and_contraction::tree_edit_lens::is_minimal`
- Exact carrier: 38 get words, 27 put words, two typed lowering refusals, and one atomic late refusal
- Exhaustive differential tests: all four `tr12_*` tests in `crates/panproto-lens/tests/reference_tree_edit_lens_tr12.rs`
- Dynamic result: 4/4 tests passed in nextest run `6e90be77-e4d0-48e0-b25a-b36239cbd6ec`; the post-refactor refusal pin passed 1/1 in run `104a9218-37f3-42c1-b249-dafaa93b6cf1`
- Evidence scope: `38 get words, 27 put words, two typed lowering refusals, and one atomic late refusal`
- Production certificate: `tr12_get_functoriality_replacement_and_minimality_match_the_reference; command: cargo nextest run --release -p panproto-lens --test reference_tree_edit_lens_tr12 --no-tests=fail -E 'test(=tr12_get_functoriality_replacement_and_minimality_match_the_reference)'`
- Independent reference anchor: `tr12_put_functoriality_minimality_and_source_undo_match_the_reference; command: cargo nextest run --release -p panproto-lens --test reference_tree_edit_lens_tr12 --no-tests=fail -E 'test(=tr12_put_functoriality_minimality_and_source_undo_match_the_reference)'`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`
- File: `crates/panproto-reference/src/enumerate/restrict_and_contraction.rs`
- File: `crates/panproto-lens/src/edit_lens.rs`
- File: `crates/panproto-inst/src/tree_edit.rs`

### Mutation equivalences

None. Reusing residue after delete-then-add is a semantic mutation, even if the final id set is unchanged.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## Tree synthesis preserves source sibling order {#L-restrict-and-contraction-12}

### Statement

Tree restriction emits surviving original siblings first, in their source order. It then emits synthesized attachments in ascending `SynthKey` order. Thus synthesis does not occupy the source position of the arc that caused it. For source siblings `[Expand(0→1), Keep(0→2)]`, the view's root children are `[2, synthesized]`. Encoding the result as a graph and reconstructing the tree yields the same sibling sequence on the validated common image.

### Outside the domain

The comparison requires a tree-reconstructible graph and a coherent source order. A fan's child-label order is governed by the fan signature rather than ordinary sibling position.

### Definitions used

The *witnessing attachment* is the first edge on the breadth-first witness path selected by ascending source edge id. Original sibling positions are the optional positions stored on source structural edges.

### Proof

`tree_get` first appends kept arcs while walking the source siblings, then allocates pending synthesized material after sorting by synthesis key. The differential compares the complete ordered reference and production trees, not unordered graph projections, on all 70 route/floor cases. The explicit `[Expand, Keep]` counterexample to source-position insertion pins the observed `[Keep, synthesized]` order.

### Executable counterpart

- Law ID: `restrict.tr13`
- Reference: `panproto_reference::lens::tree_get`
- Enumerator: `tree_restrict_cases`; exact carrier: 70 route/floor cases
- Exhaustive differential test: `tree_get_uses_the_graph_plan_keys_floor_and_sibling_order`
- Evidence scope: `70 route/floor cases with exact sibling-order observations`
- Production certificate: `tree_get_uses_the_graph_plan_keys_floor_and_sibling_order; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=tree_get_uses_the_graph_plan_keys_floor_and_sibling_order)'`
- Independent reference anchor: `tree_get_uses_the_graph_plan_keys_floor_and_sibling_order; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=tree_get_uses_the_graph_plan_keys_floor_and_sibling_order)'`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`
- File: `crates/panproto-reference/src/enumerate/restrict_and_contraction.rs`
- File: `crates/panproto-inst/src/wtype.rs`
- File: `crates/panproto-inst/src/complement.rs`

### Mutation equivalences

None. Equal unordered graph carriers do not witness equality of ordered trees.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## Contraction preserves and reflects covered reachability {#L-restrict-and-contraction-13}

### Statement

Fix a successful get and ignore fan edges. For surviving source nodes $x$ and $y$, call the pair *region-covered* when every nonempty $x$-to-$y$ walk whose internal nodes include at least one node and are all dropped projects to schema edges in the region of the declared pair $(\operatorname{anchor}(x),\operatorname{anchor}(y))$. On region-covered pairs, such a source walk exists exactly when the view contains the label or expansion path synthesized for the realized pair $(x,y)$. Thus contraction preserves and reflects dropped-interior reachability on this domain. The statement concerns the contraction stage; a direct source edge with no dropped interior is governed by its edge fate and does not by itself witness the premise.

### Outside the domain

The law makes no claim for a survivor pair with a dropped-interior walk outside the declared region. An unresolved pair and a realized pair absent from the plan make get fail, so they produce no view to which the equivalence applies. The unrestricted claim over every survivor pair is not asserted.

### Definitions used

A schema-edge projection replaces each instance edge by its declared source label. A view *contains the synthesized route* when it contains the single `Contract` edge or every adjacent edge of the `ContractExpand` path named by that pair's synthesis keys. Region coverage is a predicate on the complete set of dropped-interior walks, not on the first breadth-first witness alone.

### Proof

For preservation, let a covered source walk exist. The per-source breadth-first search reaches $y$ through dropped nodes and inserts $(x,y)$ into the realized-pair set. Successful get supplies the declared pair and a resolved label or expansion, and allocation inserts its synthesized route. For reflection, every synthesized contraction route originates in a realized pair. The breadth-first search inserts such a pair only after traversing a source walk from $x$ through at least one dropped node to $y$, so the source reachability witness exists. The differential derives the realized pairs again through `realized_pairs_by_walks` and compares both derivations with production on 12,366 graph cases.

### Executable counterpart

- Law ID: `restrict.reachability`
- Reference: `panproto_reference::restrict::get`
- Reference: `panproto_reference::restrict::realized_pairs_by_walks`
- Enumerator: `release_graphs`; exact carrier: 12,366 graph/plan cases
- Exhaustive differential test: `graph_get_and_covered_reachability_agree_on_three_node_carriers`
- Evidence scope: `12,366 graph/plan cases from the three-node release carrier`
- Production certificate: `graph_get_and_covered_reachability_agree_on_three_node_carriers; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=graph_get_and_covered_reachability_agree_on_three_node_carriers)'`
- Independent reference anchor: `graph_get_and_covered_reachability_agree_on_three_node_carriers; command: cargo nextest run --release -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=graph_get_and_covered_reachability_agree_on_three_node_carriers)'`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`
- File: `crates/panproto-reference/src/enumerate/restrict_and_contraction.rs`
- File: `crates/panproto-inst/src/graph_restrict.rs`
- File: `crates/panproto-inst/src/contraction_plan.rs`

### Mutation equivalences

None. Dropping the region-covered premise would assert a stronger law outside the registered contract and is not an equivalent mutation.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.
