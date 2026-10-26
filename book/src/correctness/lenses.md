# Lenses

This page proves the laws of panproto's asymmetric lenses in their three instance shapes: graphs, tables and trees. It also proves the laws of the lens path category built from them, the commuting squares that relate the shapes, and the protolens constructions that produce lenses from schemas. Each block states a law with its domain, proves it from the definitions below, and names the executable reference semantics and the differential tests that compare the production code with it.

A **lens** is a triple $\ell = (m, S, T)$ of a compiled migration $m$, a source schema $S$ and a target schema $T$. Its **plan** $P_\ell$ is the result of compiling $(S, T, m)$: the edge image (the fate of every source edge: kept as a target edge, dropped, contracted or expanded), the contraction plan, and the fan resolver. Compilation either succeeds or refuses with a typed plan error, and it reads no instance. A **source** is an instance of $S$ and a **view** an instance of $T$; the shapes are graphs (nodes and edges with ids), tables (rows with ids and foreign-key pairs) and trees (nodes, ordered arcs and fans).

The **path boundary** of a schema is its protocol together with the key sets of its vertices, edges and hyper-edges. Two adjacent schema representations meet when their path boundaries are equal. Their validation declarations and other annotations may differ; the two adjacent stages retain and evaluate those representations separately.

The **get** of $\ell$ maps a source $s$ to a pair $(v, c)$ of a view and a **complement**: the part of $s$ outside the image of get, together with the coordinates get forgets (recorded anchors, fields, values and edge labels), the synthesized objects get created, an id floor and an identity envelope. The **identity envelope** $\iota(\ell, \sigma)$ of a lens at a shape $\sigma$ is a digest of the canonical bytes of $S$, $T$ and $m$ together with $\sigma$ and the complement format version; get stamps it on every complement it returns. The **put** of $\ell$ maps a view and a complement to a source. Its **domain** at a complement $c$ is the set $D_\ell(c)$ of views on which the put-domain predicate reports no violation. The predicate returns every violation in a canonical stable order: variant declaration order; then the declared shape-native `ViewRef` order (`Node`, `Edge`, `Arc`, `Row`, `Pair`, with payload order), using the least element for `DuplicateKey`, the fan node for `MalformedFan`, and no element for `PlanCompilation`; then table and canonical key value for `DuplicateKey`, or field for `MissingRequiredCell`. Other comparator ties retain discovery order. After sorting, exact repeats are removed within each comparator-equivalent run. This removal includes repetitions that were separated in discovery order by a distinct violation with the same comparator keys. Duplicate-key identity uses canonical value identity, so equal NaN bit patterns deduplicate while signed zeroes remain distinct. Put returns the first violation when the view is outside the domain. A put first verifies the envelope of the complement it receives, so a complement produced by another lens or shape is refused before the domain is read.

We write $s \subseteq s'$ for **subinstance**: every node and edge of $s$ is a node and edge of $s'$ with the same id and the same content. A subinstance is **endpoint closed** when every edge it holds has both endpoints in it. A lens is on the **Delta fragment** when every surviving vertex and edge keeps its own name, the target schema is a subschema of the source schema, and the migration states no resolver, expansion, fan resolver, survival predicate or transform.

The **Single-Step Basis** of a view is the finite set of one-step edits a law checker tries: one alternate payload for each editable coordinate, every finite structural single edit, and the negative edits (reusing a reserved id, editing or removing a synthesized object, inserting a contracted edge, making a fan malformed), in the order of the mutation families. A **law report** runs five gates in order (GetPut, PutGet, Totality, PutTwice, Congruence) over the basis and reports the first that fails.

Every finite claim below is checked by an exhaustive differential test over a stated scope and by a property-based test that perturbs its operational families. For small fixed boundary families, the property test traverses every case that the shared certificate assigns to it. Such a test reports that no counterexample was found through its printed bound; it does not prove the law, which the proof does.

The twenty laws proceed in five groups: L1–L4 fix lens identity, graph complements, graph operations and the graph put domain; L5–L12 extend these results to trees, tables, complement merging and the five law gates; L13–L16 establish the lens path category and fusion; L17–L18 state the graph/tree and graph/table commuting squares; and L19–L20 cover protolens instantiation and application.

## One lens value drives every shape {#L-lenses-1}

### Statement

A lens is exactly the triple $(m, S, T)$, with no further field and no options argument to its operations. For every shape, get and put are functions of the lens, of the shape's context (the two boundary protocols for tables, nothing for graphs and trees) and of their arguments. Two lenses with equal compiled migrations and equal schemas thus have equal get, equal put, equal put domains and equal identity envelopes at every shape.

### Outside the domain

The statement quantifies over every lens and every argument, so no input lies outside it. The refusals the operations return on particular arguments are those of [L-lenses-3](#L-lenses-3), [L-lenses-4](#L-lenses-4), [L-lenses-11](#L-lenses-11) and [L-lenses-12](#L-lenses-12); a plan that does not compile is refused by every get with the plan error.

### Definitions used

Lens, plan, get, put, put domain and identity envelope are defined at the head of this page.

### Proof

1. The lens type has exactly the fields compiled migration, source schema and target schema (definition of the lens).
2. Graph get compiles the plan from $(S, T, m)$ and restricts the source along it, then stamps $\iota(\ell, \text{graph})$; table get compiles the table plan from $(S, T, m)$ and the two boundary protocols and stamps $\iota(\ell, \text{table})$; tree get compiles the plan from $(S, T, m)$ and stamps $\iota(\ell, \text{tree})$ (definition of get in each shape).
3. Each put reads the lens, the shape context, the view and the complement: it verifies the envelope against $\iota(\ell, \sigma)$, evaluates the put-domain predicate, and restores (definition of put).
4. The envelope $\iota(\ell, \sigma)$ is a digest of the canonical bytes of $S$, $T$ and $m$, of $\sigma$ and of the format version (definition), so it is a function of $(m, S, T, \sigma)$.
5. By steps 2 to 4 every operation is a composite of functions whose only inputs are the lens fields, the shape context and the operation's arguments. Two lenses with equal fields are the same argument to each composite, so their operations coincide.

### Executable counterpart

- Law ID: `lenses.l1`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_lens_equivalence_case; bound: graph_stage_states=144397, tree_stage_states=2548, table_projection_cases=9921, cases=156866`
- Exhaustive differential test: `lens_equivalence_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=lens_equivalence_law_certificate)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/lib.rs`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-lens/src/table_lens.rs`
- File: `crates/panproto-lens/src/asymmetric.rs`
- File: `crates/panproto-lens/src/identity.rs`
- File: `crates/panproto-reference/src/pages/lenses/graph.rs`
- File: `crates/panproto-reference/src/pages/lenses/protolens.rs`
- File: `crates/panproto-reference/src/pages/lenses/laws.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## The graph complement records exactly what get forgets {#L-lenses-2}

### Statement

Let $\ell$ be a lens whose plan compiles and $s$ a schema-valid source graph on which graph get succeeds, with $\operatorname{get}_\ell(s) = (v, c)$. Then $c$ holds the following information, field for field. The `dropped_nodes` field contains the nodes of $s$ absent from $v$, while `dropped_edges` contains the edges of $s$ absent from $v$ (expanded source edges included). The `original_edges` field contains exactly the surviving edges whose source label the preimage rule of [L-lenses-4](#L-lenses-4) would not return from their view label and endpoints. The `original_anchors` field contains exactly the surviving nodes whose source anchor the unique reverse vertex map would not return. The `original_extra_fields` field contains exactly the surviving nodes whose view fields differ from their source fields, with the source fields; `original_values` contains exactly the surviving nodes whose view value differs from its source value, with the source value. The `synthesized_nodes` and `synthesized_edges` fields contain the objects get created, each under its synthesis key. The remaining fields hold the id floor of $s$ and the identity envelope $\iota(\ell, \text{graph})$. The pair $(v, c)$ determines $s$: get paired with its complement is injective on the sources where it succeeds.

### Outside the domain

A plan that does not compile, and an element on which restriction refuses, give no complement: get refuses with the plan error or with the evaluation error at the first element in error-key order, and no field of this statement applies.

### Definitions used

Lens, plan, get, complement and identity envelope are defined at the head of this page. The preimage rule and the reverse vertex map are those of [L-lenses-4](#L-lenses-4). A synthesis key names a synthesized object by the source data that produced it: a contracted pair with its label, or a hop of an expansion.

### Proof

1. Graph get first decides, node by node, whether a node survives (its anchor survives and its survival predicate holds); a node that does not survive is inserted into `dropped_nodes`, and nothing else is (definition of the restrict stage 0).
2. An edge whose endpoints do not both survive is inserted into `dropped_edges`; an edge between surviving nodes takes its fate from the edge image: kept (into the view, recorded in `original_edges` exactly when the preimage rule would not recover its label), dropped (into `dropped_edges`), contracted or expanded (into `dropped_edges`, with the synthesized replacements in the view and their keys in the synthesized maps) (definition of the restrict stage 1 and of the contraction stage).
3. For a surviving node, get records the source anchor when the reverse remap does not recover it, the source fields when they differ from the view fields as maps, and the source value when the presences differ (definition of stage 0). These are the iff conditions of the statement.
4. Steps 1 to 3 assign every node and edge of $s$ either to the view or to a dropped map, and every coordinate the view changes to an `original_*` map, so the fields hold exactly the listed contents.
5. The unedited view lies in the put domain. Its ids are disjoint from the dropped ids; every synthesized object and chain is present with the content get assigned; every surviving edge has either its recorded source label or the unique preimage get used; and every surviving node has either its recorded source anchor or the unique reverse anchor. Restoring its recorded fields and values yields the corresponding source node, so predicates and transforms receive exactly their original inputs. Finally, the restored source is $s$, which already satisfies its key, required-cell and fan declarations. Thus none of the put-domain clauses reports a violation.
6. Put now restores every dropped node and edge, every recorded label, anchor, field and value, and removes every synthesized object, so it returns $s$ by the restoration definition. If $\operatorname{get}_\ell(s) = \operatorname{get}_\ell(s')$, applying this same deterministic restoration to their common pair gives both $s$ and $s'$. Hence $s=s'$.

### Executable counterpart

- Law ID: `lenses.l2`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_lens_equivalence_case; bound: graph_stage_states=144397, tree_stage_states=2548, table_projection_cases=9921, cases=156866`
- Exhaustive differential test: `lens_equivalence_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=lens_equivalence_law_certificate)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-inst/src/graph_complement.rs`
- File: `crates/panproto-inst/src/graph_restrict.rs`
- File: `crates/panproto-reference/src/pages/lenses/graph.rs`
- File: `crates/panproto-reference/src/pages/lenses/domain.rs`
- File: `crates/panproto-reference/src/pages/lenses/protolens.rs`
- File: `crates/panproto-reference/src/pages/lenses/laws.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Graph get is stamped restriction and graph put is checked restoration {#L-lenses-3}

### Statement

For every lens $\ell$ and source graph $s$: $\operatorname{get}_\ell(s)$ is the restriction of $s$ along the plan compiled from $(S, T, m)$, with its complement stamped with $\iota(\ell, \text{graph})$. For every view $v$ and complement $c$: put first verifies the envelope of $c$ (a matching envelope is accepted, an empty unstamped complement is accepted as unbound, anything else is refused), then evaluates the put-domain predicate and returns its first violation when there is one, and otherwise returns the restored source. Plan compilation is the first domain clause: a compilation failure yields `PlanCompilation`, so put is total on $D_\ell(c)$. The complement-free put of an isomorphism stamps an empty complement and puts; for a lens that is not an isomorphism it refuses. For an allocation floor $f$, let $K$ contain the view ids, the ids reserved by dropped residue and the synthesized presentation ids of the relevant kind. The fresh view id is $f$ when no member of $K$ lies at or above $f$, and $\max(f, 1 + \max K)$ otherwise. The allocator refuses when this value would be the reserved top id, which it never returns.

### Outside the domain

A plan that does not compile: get refuses with the plan error, while the put-domain predicate reports `PlanCompilation` for every view and put returns that first domain violation. A complement whose envelope names another lens, shape or format: put refuses with `Identity` (`Mismatch`). A complement that records residue but carries no envelope: put refuses with `Identity` (`UnboundComplement`). A view outside $D_\ell(c)$: put refuses with `PutDomain` and the first violation in report order ([L-lenses-4](#L-lenses-4)). Complement-free put of a lens with residue: `NotAnIsomorphism`.

### Definitions used

Lens, plan, get, put, put domain, report order and identity envelope are defined at the head of this page; the put-domain predicate and restoration are those of [L-lenses-4](#L-lenses-4).

### Proof

1. Graph get compiles the plan and runs the graph restriction engine on it, then sets the complement's identity to $\iota(\ell, \text{graph})$ (definition of graph get); the reference get does the same with the reference restriction.
2. Checked put runs three steps in order: envelope verification, the domain predicate, restoration (definition of checked put). Envelope verification returns `Mismatch` for a present envelope different from $\iota(\ell, \text{graph})$, accepts an absent envelope exactly when every residue map of the complement is empty, and otherwise returns `UnboundComplement` (definition of verification).
3. When the domain predicate returns a non-empty sorted list, put returns its first element as `PutDomain`; otherwise put returns the restoration (step 2).
4. Totality on the domain: the predicate first compiles the plan and reports `PlanCompilation` if it cannot. After successful compilation, restoration is a total function of the view and complement once the remaining clauses hold, because every condition that could fail during restoration (an unrepresentable edge, an ambiguous anchor, a refused survival predicate, an edited derived field, a duplicate key, a malformed fan) is one of those clauses, checked before restoration ([L-lenses-4](#L-lenses-4), steps 1 to 3). Thus a view in $D_\ell(c)$ never reaches a refusal.
5. Complement-free put: when the isomorphism classifier reports an obstruction the operation refuses with `NotAnIsomorphism`; otherwise it stamps an empty complement with $\iota(\ell, \text{graph})$ and runs step 2, whose verification accepts it (definition).
6. Fresh ids: let $K$ contain the view, reserved and synthesized ids of the relevant kind. If $K$ has no id at or above the complement's floor $f$, the allocator returns $f$. Otherwise it returns $\max(f, 1 + \max K)$. It refuses with `NodeIdsExhausted` or `EdgeIdsExhausted` when the result would be the reserved top id (definition of the fresh-id rule).

### Executable counterpart

- Law ID: `lenses.l3`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_graph_put_law_case; bound: boundary_cases=21, graph_stage_states=144397, base_cases=144418`
- Exhaustive differential test: `graph_put_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=graph_put_law_certificate)'`
- Property differential test: `randomized_graph_put_law_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_graph_put_law_differential)'`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-inst/src/error_detail.rs`
- File: `crates/panproto-inst/src/view_ref.rs`
- File: `crates/panproto-inst/src/graph_complement.rs`
- File: `crates/panproto-inst/src/graph_restrict.rs`
- File: `crates/panproto-reference/src/pages/lenses/graph.rs`
- File: `crates/panproto-reference/src/pages/lenses/domain.rs`
- File: `crates/panproto-reference/src/pages/lenses/protolens.rs`
- File: `crates/panproto-reference/src/pages/lenses/laws.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`
- File: `crates/panproto-lens/tests/support/graph_put_contract.rs`
- File: `crates/panproto-lens/tests/reference_graph_put_contract.rs`
- File: `crates/panproto-inst/src/segment/ids.rs`
- File: `crates/panproto-inst/src/identity.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## The graph put domain and restoration, and the cascade on the Delta fragment {#L-lenses-4}

### Statement

For a lens $\ell$, a complement $c$ produced by $\operatorname{get}_\ell$ and a view $v$, the put-domain predicate returns every violation of the following clauses, sorted by report order, and $v \in D_\ell(c)$ exactly when it returns none:

1. `IdCollision`, a view node or edge whose id is a dropped id;
2. `SynthesizedEdited`, a synthesized object missing from the view or changed, unless an endpoint of its chain was removed;
3. the edge back-mapping: a non-synthesized view edge maps back to its recorded label when `original_edges` records it with the same endpoints, else to the unique source edge whose fate keeps its label between preimages of its endpoints' anchors, and otherwise refuses with `ContractedEdgeInserted` (when the endpoints form a planned pair resolving to its label) or `UnrepresentableViewEdge`; a fan edge maps back through its hyper-edge's labels;
4. `AmbiguousReverseAnchor`, a view-added or re-anchored node whose anchor has more than one preimage;
5. `SurvivalPredicateFails` and `SurvivalPredicateRefused`;
6. `TransformNotInvertible` and `DerivedCellEdited`;
7. `DuplicateKey` and `MissingRequiredCell`;
8. `MalformedFan`.

The structural clauses read the original view and complement. We call the filtered state read by the later clauses the **restorable counterfactual**. It retains what put can still restore without erasing the structural violations already found. In particular, a colliding view object is treated as the source representative at its reused id; [L-lenses-5](#L-lenses-5) specifies the tree counterfactual needed to realize that rule without retaining shadowed source residue. On the domain, put restores: the non-synthesized view nodes with their recorded anchors (when the recorded anchor's image is the view anchor), fields and values, the dropped nodes, the view edges with their mapped-back labels, and the dropped edges minus the cascade, which removes the dropped edges incident to a surviving source node absent from the view. On the Delta fragment, for every source $s$ with $\operatorname{get}_\ell(s) = (v, c)$ and every endpoint-closed $v' \subseteq v$ that lies in $D_\ell(c)$, $r = \operatorname{put}_\ell(v', c)$ is the largest endpoint-closed subinstance of $s$ whose view is $v'$, and every endpoint-closed subinstance $t \subseteq s$ whose view is a subinstance of $v'$ is a subinstance of $r$.

### Outside the domain

A view with any violation of clauses (1) to (8) is outside $D_\ell(c)$; put refuses with `PutDomain` and the first violation, and the predicate returns the full sorted list. A complement from another lens is refused by envelope verification first ([L-lenses-3](#L-lenses-3)). The cascade statement is made only on the Delta fragment and only for deletion-only edits $v' \subseteq v$ that remain inside $D_\ell(c)$. The cascade still runs outside this fragment, but the largest-preimage reading is not claimed.

### Definitions used

Lens, put domain, report order, subinstance, endpoint closed and the Delta fragment are defined at the head of this page. The view of $t$ means the first component of $\operatorname{get}_\ell(t)$.

### Proof

1. The predicate evaluates the structural clauses over every view element, evaluates the remaining clauses on the restorable counterfactual, and collects all violations before checked put makes a decision. It then applies the report comparator from the head of this page with a stable sort and removes exact repeats within each comparator-equivalent run. This includes a repeat separated before sorting by another violation with the same comparator keys. Restoration by checked put runs only when the resulting report is empty (definition of the predicate and of checked put). The clauses are exactly (1) to (8).
2. Graph restoration keeps a colliding view node in the restorable part, so the visible node occupies the dropped node's id when the later clauses inspect the counterfactual source. It removes only a node with no single source anchor, together with its incident edges, or an edge with no single source label. Restoration then builds the source from the remaining view nodes, the dropped nodes, the mapped-back view edges and the dropped edges, and removes the cascade (definition of restoration). Each mapped-back label exists because clause (3) admitted the edge. The native tree implementation realizes the same collision rule by the residue transformation proved in [L-lenses-5](#L-lenses-5).
3. Now let $\ell$ be on the Delta fragment. Then get keeps a node exactly when its anchor is a surviving vertex, keeps an edge exactly when both endpoints are kept and its label is a surviving edge, changes no anchor, field, value or label, and synthesizes nothing (definition of get with no remap, resolver, expansion, fan resolver, predicate or transform). So $c$ records no `original_*` entry, `dropped_nodes` is the set of nodes with a non-surviving anchor, and `dropped_edges` is the set of edges not kept.
4. Let $v' \subseteq v$ be endpoint closed and suppose $v' \in D_\ell(c)$. This premise includes the key, required-cell and fan clauses, which a deletion can violate even when $s$ is valid. The structural clauses can also be read directly: $v'$ uses only ids of $v$, no synthesized object exists on the Delta fragment, and every edge maps back to its own uniquely surviving label. Thus, put defines $r = \operatorname{put}_\ell(v', c)$.
5. By step 2, $r$ consists of the nodes of $v'$, the dropped nodes, the edges of $v'$, and the dropped edges not incident to a node of $v$ absent from $v'$. Every element of $r$ is an element of $s$ with the same content (step 3), and $r$ is endpoint closed: a kept dropped edge has no endpoint among the removed surviving nodes, and its other endpoints are dropped nodes or nodes of $v'$.
6. The view of $r$ is $v'$: the kept nodes of $r$ are those with a surviving anchor, which are the nodes of $v'$ (dropped nodes have non-surviving anchors by step 3); the kept edges of $r$ are those with both endpoints kept and a surviving label, which are the edges of $v'$ together with any dropped edge between two nodes of $v'$ with a surviving label. Such a dropped edge would have been kept by get on $s$ (its endpoints are in $v \supseteq v'$ and its label survives), contradicting step 3, so there is none.
7. Universal property: let $t \subseteq s$ be endpoint closed with view $w \subseteq v'$. A node $x$ of $t$ with a surviving anchor is in $w$, hence in $v'$, hence in $r$; a node with a non-surviving anchor is a dropped node, hence in $r$. An edge $e$ of $t$ kept by get on $t$ is in $w \subseteq v'$, hence in $r$. An edge $e$ of $t$ not kept is a dropped edge of $s$ (a non-surviving label or a non-surviving endpoint gives the same verdict in $s$); it is missing from $r$ only if it is incident to a node $y$ of $v$ absent from $v'$. But $y$ is an endpoint of $e$, so $y$ is in $t$ (endpoint closure) with a surviving anchor, so $y \in w \subseteq v'$, a contradiction. Hence $t \subseteq r$.
8. Largest: $r$ is itself an endpoint-closed subinstance of $s$ with view $v'$ (steps 5 and 6), and by step 7 every endpoint-closed subinstance with view $v'$ is contained in $r$. A largest element is unique, so $r$ is the largest exact preimage.

### Executable counterpart

- Law ID: `lenses.l4`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_graph_put_cascade_law_case; bound: named_domain_families=12, graph_stage_states=144397, delta_deletion_edits=604, within_delta_edits_fixtures=2, within_delta_edits_sources=133, nested_subinstance_probes=5792, cases=145013`
- Exhaustive differential test: `graph_put_domain_and_cascade_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=graph_put_domain_and_cascade_law_certificate)'`
- Property differential test: `randomized_graph_put_domain_and_cascade_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_graph_put_domain_and_cascade_differential)'`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-inst/src/error_detail.rs`
- File: `crates/panproto-inst/src/view_ref.rs`
- File: `crates/panproto-inst/src/graph_complement.rs`
- File: `crates/panproto-reference/src/pages/lenses/graph.rs`
- File: `crates/panproto-reference/src/pages/lenses/domain.rs`
- File: `crates/panproto-reference/src/pages/lenses/protolens.rs`
- File: `crates/panproto-reference/src/pages/lenses/laws.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## The tree put domain refuses what the graph put domain refuses {#L-lenses-5}

### Statement

Tree put runs a native tree put-domain predicate before restoring and reports the corresponding `PutDomainError` variants of graph put. These include `IdCollision` for a view node that reuses a dropped node's id and `AmbiguousReverseAnchor` for a view-added or re-anchored node whose anchor has several preimages. The native production predicate and the private reference law adapter name an arc by `ViewRef::Arc { parent, child }`. The public reference tree operations and the representation square retain the stable graph-coordinate API, which reports `ViewRef::Edge(id)`. These coordinates are not literally equal; the square compares reports only after applying the durable-embedding graph projection. A reparented synthesized arc has both its original and current arc as native witnesses. A sibling-order refusal uses the least synthesis key and arc natively, while the public projection uses the least embedded edge id. The tree complement records a surviving node's source anchor exactly when the unique reverse vertex map would not return it. Tree put applies that recorded anchor only when its image under the vertex map equals the node's view anchor. Thus, a view relabel is kept rather than reverted. The complement also records the source arc order and the contracted or expanded arcs that the view alone does not identify. Its identity envelope records `ComplementShape::WType`, and tree put verifies that envelope against the lens before reading the domain. This envelope is distinct from the `ComplementShape::Graph` envelope used along the graph route. For nonfan schemas in the certified tree domain (the lens keeps the tree shape and the root survives), tree get and put agree with graph get and put on the tree's graph form.

### Outside the domain

A tree view outside the put domain: tree put refuses with `PutDomain` and the first violation. A lens that drops an edge between two surviving vertices is outside the tree lens's domain, where the graph keeps the endpoints and drops the edge while the tree would drop the subtree; tree get is not claimed to agree there (the reference returns no result). A source whose root is pruned is refused with `RootPruned` and falls back to the graph path in dispatch ([L-lenses-17](#L-lenses-17)). The broad C11 carrier shared by the graph and tree routes excludes fan schemas. The L5 certificate adds one focused malformed-fan boundary for the tree put domain; [L-lenses-17](#L-lenses-17) states the representation theorem on its exact common domain, including representable fans.

### Definitions used

Lens, put domain and report order are defined at the head of this page; the clauses are those of [L-lenses-4](#L-lenses-4). The graph form of a tree is its embedding with arcs as edges and fans reified.

### Proof

1. Tree put verifies the envelope, then runs its native structural pass on the original tree nodes, arcs, synthesis keys and contraction choices. It removes structurally unrestorable subtrees, reassembles the determined part, and runs the value, predicate, key, required-cell and fan checks on that restored tree. It applies the canonical stable sort and exact-repeat removal from the head of this page to the accumulated violations, and refuses with the first one (definition of tree put).
2. A view node whose id is a key of `dropped_nodes` is detected by the native structural pass, so the original report retains `IdCollision` for it. Before the restored-source clauses run, the implementation gathers every such node id into one ordered set and removes from an ephemeral complement only those dropped nodes and the residue directly incident on or dependent on them: dropped arcs and fans, contraction and parent choices, recorded arcs and arc order, original anchors, fields and values, contraction targets, colliding synthesized carriers, and synthesis keys that mention a colliding endpoint, holder or incident source edge. It scans the original source arc order once, deletes incident source edges, and rewrites each surviving `Expand.source_edge` to its rank in the retained order. Processing all collisions together makes the transformation deterministic; unrelated complement state, including noncolliding synthesized carriers and keys, is preserved. The independent reference performs the same simultaneous collision and dependency filtering on its graph complement. Its surviving `Expand.source_edge` values do not require reindexing because they are stable graph edge ids rather than source arc-order positions.
3. The same structural pass constructs the reverse-anchor fibres and reports `AmbiguousReverseAnchor` for a view-added or re-anchored node with several preimages. Such a node and the view subtree below it are absent from the restorable counterfactual because put has no determined source anchor for them (step 1).
4. Tree get records a surviving node's source anchor exactly when the reverse vertex map does not return it (definition of tree get, the same recording rule as graph get, [L-lenses-2](#L-lenses-2) step 3); tree restoration applies the recorded anchor exactly when its image equals the view anchor, else the unique reverse anchor (definition of tree restoration). The structural synthesis checks still read the original complement. A synthesized reparent thus retains both its original and current native arc witnesses, while the sibling-order rule selects the least synthesis key and arc independently of discovery order.
5. On the nonfan tree lens domain, no edge between survivors is dropped. The graph restriction of the tree's graph form thus keeps exactly the arcs whose endpoints survive, and unique tree paths make its contraction choices equal to the native tree choices. The durable embedding maps an arc-local native witness to its public graph edge and applies the graph-specific minimum-edge rule to a sibling-order witness. After this explicit projection, the native structural and value passes of step 1 inspect the same restored nodes and source labels as graph restoration. Each route verifies its own shape-specific envelope. Thus their views, projected domain reports and restored sources agree under the embedding.

### Executable counterpart

- Law ID: `lenses.l5`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Reference: `panproto_reference::lens::tree_get`
- Reference: `panproto_reference::lens::tree_put`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_tree_put_law_case; bound: named_validation_cases=3, tree_stage_states=2548, base_cases=2551`
- Exhaustive differential test: `tree_put_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=tree_put_law_certificate)'`
- Property differential test: `randomized_tree_put_law_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_tree_put_law_differential)'`
- File: `crates/panproto-lens/src/asymmetric.rs`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-lens/src/lens_shape.rs`
- File: `crates/panproto-inst/src/error_detail.rs`
- File: `crates/panproto-inst/src/view_ref.rs`
- File: `crates/panproto-reference/src/pages/lenses/tree.rs`
- File: `crates/panproto-reference/src/pages/lenses/graph.rs`
- File: `crates/panproto-reference/src/pages/lenses/domain.rs`
- File: `crates/panproto-reference/src/pages/lenses/laws.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`
- File: `crates/panproto-lens/tests/support/tree_validation.rs`
- File: `crates/panproto-lens/tests/reference_tree_validation.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Complements of one lens merge as a partial commutative monoid, and composition is a path {#L-lenses-6}

### Statement

Let the carrier contain the internally coherent, identity-admissible tree complements. Modulo merge equivalence, `ComplementCompose` forms a partial commutative monoid: the empty complement is a two-sided unit, and merge is commutative and associative where defined. Internal coherence means that each synthesized id names one synthesis key and each synthesis key names one id within each operand. A complement is identity-admissible when it carries a verified identity envelope, carries a nonzero legacy fingerprint, or records nothing; the last case admits the unbound empty complement as the unit. Merge equivalence compares the source fingerprint, identity envelope, id floor, keyed maps, sequence fields and arc order, using value equivalence for stored values and set equality for the deduplicated sequences. Merge is defined exactly when (i) the identity bindings combine, (ii) shared keyed entries agree, (iii) the two coherent operands remain jointly coherent, and (iv) two nonempty arc-order records are equal. It retains the left representative of an equivalent shared value, deduplicates the sequence fields, and takes the maximum id floor. Merge is not cancellative and is not lens composition. Composing two lenses yields the lens path of the two stages ([L-lenses-13](#L-lenses-13)); its behaviour is the sequential fold, and fusion is applied only where it preserves that behaviour ([L-lenses-16](#L-lenses-16)).

### Outside the domain

An operand whose synthesized metadata is incoherent, or that records data without either a verified envelope or a nonzero legacy fingerprint, lies outside the algebraic carrier. A production merge presented with the latter refuses with `Identity(UnboundComplement)`. Two carrier complements that disagree on a shared key, assign distinct synthesis keys to one id across operands, or assign one synthesis key to distinct ids across operands: merge refuses with `ComplementConflict` naming the map and the conflicting entry. Two bound complements whose identities do not combine: merge refuses with `Identity`. Two lenses whose shared boundary differs: composition refuses with `CompositionMismatch`.

### Definitions used

Complement is defined at the head of this page. Agreement on a shared key means equal values under that key in every keyed map; for synthesized maps the id and the synthesis key must both agree. Merge equivalence is the relation used by the property tests: it compares the binding fields and id floor exactly, keyed values under value equivalence (equality with NaN reflexive), sequence fields as deduplicated collections, and the arc-order record exactly. This relation is stricter than the observational law equivalence of [L-lenses-7](#L-lenses-7).

### Proof

1. Merge first combines the two identity bindings (refusing with `Identity` when they do not combine), then takes the keywise union of each keyed map, keeping the left operand's value on a shared key and refusing at the first shared key whose values are not equivalent. Internal coherence supplies the one-to-one synthesized index within each operand; the cross-operand id and inverse-key checks preserve that property in the union. Merge then combines the sequence fields by deduplicated concatenation, requires equal arc-order records when both are nonempty, and sets the floor to the maximum of the two floors (definition of merge).
2. Unit: the empty complement has no keys or sequence entries, floor zero, no arc-order record and no envelope. Because it records nothing, its unbound identity combines with any identity-admissible carrier complement, so the union is structurally equivalent to the other complement and the maximum is its floor.
3. Commutativity: keywise union of maps whose shared values are equivalent is symmetric up to the representative kept on a shared key, which is equivalent to the other one. Deduplicated sequence union is symmetric under structural equivalence, the nonempty arc-order compatibility condition is symmetric, and maximum is commutative. Identity composition and the refusal conditions are likewise symmetric on compatible complements.
4. Associativity: where both bracketings are defined, each key of the union of three pairwise agreeing maps takes a value equivalent to every operand's value at that key (the equivalence is transitive), and deduplicated sequence union and maximum are associative under structural equivalence. The arc-order rule retains the unique nonempty record, if any, and identity composition is associative on compatible bindings. If one bracketing refuses at a shared key, arc order or identity binding, the other encounters the same incompatible data and refuses.
5. Not cancellative: merging a complement with floor 5 into floors 3 and 5 gives equal results from unequal arguments.
6. Composition builds the two-stage path; its get and put are the folds of [L-lenses-16](#L-lenses-16), whose proof gives the behaviour.

### Executable counterpart

- Law ID: `lenses.l6`
- Reference: `panproto_reference::lens::compose_complement`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_complement_monoid_case; bound: provenance_regimes=2, atom_subsets=8, ordered_triples=1024, named_refusals=15, focused_witnesses=3, cases=1042`
- Exhaustive differential test: `complement_partial_monoid_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_complement_monoid --no-tests=fail -E 'test(=complement_partial_monoid_law_certificate)'`
- Property differential test: `randomized_complement_partial_monoid_differential; command: cargo nextest run --release -p panproto-lens --test reference_complement_monoid --no-tests=fail -E 'test(=randomized_complement_partial_monoid_differential)'`
- File: `crates/panproto-lens/src/asymmetric.rs`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-lens/src/table_lens.rs`
- File: `crates/panproto-lens/src/compose.rs`
- File: `crates/panproto-inst/src/identity.rs`
- File: `crates/panproto-inst/src/complement.rs`
- File: `crates/panproto-reference/src/pages/lenses.rs`
- File: `crates/panproto-reference/src/pages/lenses/lens.rs`
- File: `crates/panproto-reference/src/pages/lenses/path.rs`
- File: `crates/panproto-reference/src/pages/lenses/graph.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`
- File: `crates/panproto-lens/tests/reference_complement_monoid.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Law equivalence is equality modulo synthesized presentation ids {#L-lenses-7}

### Statement

Every law that compares views compares them with the shape's equivalence under the synthesized index of the complement that produced each view (a user-edited view takes the index of the complement it is put against). An incoherent index is inequivalent to every instance, including itself. On coherent indices, graph equivalence holds exactly when the node and edge synthesis-key sets are equal and, after renaming each synthesized id to the id its key has on the other side, the two graphs are equal on every node (anchor, value, fields, discriminator, annotations, position, shape) and every edge (endpoints, label, position). Tree equivalence performs the corresponding key normalization for expansion nodes and is exact elsewhere, child order included. Table equivalence embeds rows and foreign-key pairs in the canonical graph presentation and applies graph equivalence, so synthesized pair ids are presentation coordinates rather than literal table equality. Presentation ids are normalized to synthesis keys in every compared coordinate. After normalization, complements are compared on every restoration field and on the id floor. Exact floor equality is a uniform conservative contract across views, rather than the coarsest quotient for one fixed view. The source fingerprint and identity envelope are excluded from this observational relation. Claims about put use it only after both complements have independently verified against the same lens and source schema; it is not the merge equivalence of [L-lenses-6](#L-lenses-6).

### Outside the domain

The comparison function is total on pairs of instances with indices. An incoherent index, unequal key sets or unequal normalized contents gives `false`, never a refusal. The equivalence relation itself is the restriction of that function to coherent indices.

### Definitions used

Complement and synthesis key are defined at the head of this page and in [L-lenses-2](#L-lenses-2).

### Proof

1. Equivalence first verifies that each synthesized id has one key, each key has one id, and every indexed node or edge id exists in the corresponding instance. It returns inequivalent when either index is incoherent or when the two coherent indices' node or edge key sets differ (definition).
2. With equal key sets, each key names exactly one id on each side by coherence from step 1, so the renaming is a bijection on synthesized ids and the identity elsewhere.
3. After renaming, the comparison is equality of the two graphs' node and edge maps, which compares every listed coordinate (definition).
4. On coherent indices, equivalence is thus an equivalence relation: reflexive (equal key sets, identity renaming), symmetric (the inverse bijection), and transitive (composition of bijections preserves key sets). Table embedding is deterministic and tree normalization uses the same bijection, so the argument applies to all three shapes.
5. The complement comparison normalizes synthesized presentation ids to keys in the dropped and original maps, including the tree arc-order and arc-label coordinates, then compares every restoration field and the id floor (definition). The graph floor has separate node and edge components, and the corresponding writers read them as the lower bounds for fresh node and edge allocation. A tree complement inherits the graph node floor, which its fresh-node writer reads. The table floor is both the lower bound for fresh-row allocation and the threshold by which put distinguishes source rows from newly inserted view rows before applying inverse field transforms. The graph and table witnesses compare floors zero and one: the graph writers choose node and edge ids zero and one, and the table writer chooses row ids zero and one. The tree witness compares floors 0 and 12 with view node 10, which makes the fresh-node writer choose ids 11 and 12. The table put witness also restores the same row-zero view differently across that threshold. Exact floor equality thus supplies one conservative contract for every view. For a fixed occupied view, a coarser quotient may identify unequal floors that happen to choose the same next id. Suppose both complements have first verified against the same lens and source schema. Their excluded source fingerprint and identity envelope then have the same required meaning. Thus observationally equivalent complements give equal restored values on every view for which both puts pass their domain checks ([L-lenses-4](#L-lenses-4)) and make the same deterministic writer allocation. Without that common-binding premise, the relation makes no claim about put success.

### Executable counterpart

- Law ID: `lenses.l7`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_law_equivalence_oracle_case; bound: coherent_pairs=54, coherent_triples=81, incoherent_families=12, floor_divergences=3, operational_floor_cases=1, cases=151`
- Exhaustive differential test: `lens_equivalence_normalization_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lens_equivalence --no-tests=fail -E 'test(=lens_equivalence_normalization_certificate)'`
- Property differential test: `randomized_lens_equivalence_normalization; command: cargo nextest run --release -p panproto-lens --test reference_lens_equivalence --no-tests=fail -E 'test(=randomized_lens_equivalence_normalization)'`
- File: `crates/panproto-lens/src/lens_shape.rs`
- File: `crates/panproto-lens/src/table_shape.rs`
- File: `crates/panproto-lens/src/table_lens.rs`
- File: `crates/panproto-reference/src/pages/lenses/laws.rs`
- File: `crates/panproto-reference/src/pages/lenses/graph.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`
- File: `crates/panproto-lens/tests/reference_lens_equivalence.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## The five law gates {#L-lenses-8}

### Statement

Let $\operatorname{get}_\ell(s) = (v, c)$, and let $B$ be the ordered Single-Step Basis of $v$. Partition $B$ into positive cases $B^+$ and deliberately refused cases $B^-$, and write $v_b$ for the view carried by case $b$. The admitted positive cases are

$$A_c = \{b \in B^+ \mid v_b \in D_\ell(c)\}.$$

A case in $B^+ \setminus A_c$ remains in the generated-basis counts, but PutGet, Totality and PutTwice omit it. Each $b \in B^-$ carries a **negative certificate** $\nu_b$: a nonempty complete ordered put-domain report constructed without consulting either the domain predicate or $\operatorname{put}_\ell$. A tree certificate uses native `Arc { parent, child }` coordinates. The public graph/tree square instead applies the durable-embedding graph projection and reports `Edge(id)`, so the two APIs do not compare a literal arc value with a literal edge value.

The law report runs five gates in order and returns the first violation. GetPut is first and requires $\operatorname{put}_\ell(\operatorname{get}_\ell(s)) = s$. PutGet follows, strict on every coordinate in every contraction mode: for every $b \in A_c$ on which put succeeds, the view of $\operatorname{put}_\ell(v_b, c)$ is equivalent to $v_b$ under [L-lenses-7](#L-lenses-7). The Totality gate requires every view in $A_c$ to put without error. For each $b \in B^-$, the domain predicate evaluated at $(v_b, c)$ must return $\nu_b$, and $\operatorname{put}_\ell(v_b, c)$ must return the first violation in $\nu_b$. For each $b \in A_c$, PutTwice then requires $\operatorname{put}_\ell(v_b, \rho(c')) = s'$ for $s' = \operatorname{put}_\ell(v_b, c)$ and $(v', c') = \operatorname{get}_\ell(s')$, with $\rho$ the coherent rekeying from the index of $v'$ to that of $v_b$. Congruence is last: coherently renumbered view and complement pairs have equal domain verdicts and equivalent puts. Shapes skip what they cannot express. GetPut and WeakPutGet together do not imply PutTwice; GetPut at the once-put source together with exact PutGet does.

### Outside the domain

A get that refuses makes the report a refusal before any gate runs. The domain predicate may exclude a positive proposal before PutGet, Totality and PutTwice; this exclusion is a domain classification, not a law failure. A refusal during GetPut, during re-get in PutGet, or during either operation in PutTwice is reported at that gate. If $\operatorname{put}_\ell(v_b, c)$ refuses for some $b \in A_c$, PutGet skips the case and Totality reports the refusal. A negative basis view whose complete ordered put-domain report differs from its negative certificate, or for which put itself does not return the negative certificate's first violation, fails Totality.

### Definitions used

Single-Step Basis, law report and equivalence are defined at the head of this page and in [L-lenses-7](#L-lenses-7). A finite partial lens has two sources, two views and three complements, an injective total get and a partial put.

### Proof

1. The report runs the gates in the stated order and returns at the first failure (definition of the checker).
2. The checker constructs $A_c$ by retaining exactly those positive cases whose views the domain predicate admits. PutGet skips $b \in A_c$ if $\operatorname{put}_\ell(v_b, c)$ refuses, and Totality reports that refusal. For a negative basis view, both the independent certificate and the observed report use the canonical stable sort and exact-repeat removal from the head of this page. Totality compares those complete reports, then checks the violation returned by put itself against the certificate's first violation (definition of PutGet and Totality). It thus requires equality in report length and order and equality of each violation, including its variant, shape-native element coordinate and variant-specific payload. The tree checker keeps native arc provenance here; conversion to a public graph-edge coordinate belongs to the representation square, not to this equality.
3. PutTwice not implied: on the partial lens with $\operatorname{get}(s_0) = (a, x)$, $\operatorname{get}(s_1) = (a, y)$, $\operatorname{put}(a, x) = s_0$, $\operatorname{put}(a, y) = s_1$, $\operatorname{put}(a, z) = s_0$, $\operatorname{put}(b, y) = s_1$, $\operatorname{put}(b, z) = s_0$ and $\operatorname{put}(b, x)$ undefined, GetPut holds (the first two equations); WeakPutGet holds because every defined put returns a source whose view is $a$ and $\operatorname{put}(a, \cdot)$ returns the same source on each complement where it was defined; PutTwice fails at $(b, z)$: $s' = s_0$, $\operatorname{get}(s_0) = (a, x)$ and $\operatorname{put}(b, x)$ is undefined.
4. Implication: let $s' = \operatorname{put}(v, c)$ be defined with $\operatorname{get}(s') = (v', c')$. Exact PutGet gives $v' = v$; GetPut at $s'$ gives $\operatorname{put}(v', c') = s'$; substituting, $\operatorname{put}(v, c') = s'$, which is PutTwice.
5. With synthesized carriers the rekeyed form follows in the same way: indexed PutGet gives equal key sets, so $\rho$ is a bijection on synthesized ids with $\operatorname{index}(v, \rho(c')) = \operatorname{index}(v)$, the states $(v, \rho(c'))$ and $(v', c')$ are coherently congruent, Congruence transports put across them, and GetPut at $s'$ identifies the result with $s'$.

### Executable counterpart

- Law ID: `lenses.l8`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_lens_law_case; bound: partial_lens_source_cases=43740, graph_stage_states=144397, tree_stage_states=2548, table_projection_cases=9921, cases=200606`
- Exhaustive differential test: `lens_laws_agree_with_the_reference_exhaustively; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=lens_laws_agree_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/laws.rs`
- File: `crates/panproto-lens/src/lens_shape.rs`
- File: `crates/panproto-lens/src/table_shape.rs`
- File: `crates/panproto-lens/src/asymmetric.rs`
- File: `crates/panproto-inst/src/error_detail.rs`
- File: `crates/panproto-inst/src/view_ref.rs`
- File: `crates/panproto-reference/src/pages/lenses/laws.rs`
- File: `crates/panproto-reference/src/pages/lenses/domain.rs`
- File: `crates/panproto-reference/src/pages/lenses/tree.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Graph get and put of a tree agree with the tree lens {#L-lenses-9}

### Statement

For a nonfan tree-shaped source $W$ and a lens that preserves tree shape and keeps the root, the graph and tree routes agree on get and put. Under get, the graph-form result is identical to the tree result mapped through the embedding; both routes allocate synthesized ids by synthesis key from the complement's floor, with child order compared separately. Their put results are likewise identical. Within the bounded nonfan stage carrier, the certificate isolates two structural route exclusions: a subtree drop (an edge dropped between surviving vertices) and a pruned root. Contractions, including recursive dropped wrappers, commute.

### Outside the domain

A lens that drops an edge between surviving vertices: the tree route is outside its exact domain and the square reports the route as refused (no agreement is claimed). A pruned root: tree get refuses with `RootPruned` and dispatch falls back to the graph route. An edited tree whose durable embedding does not represent its changed child order is outside the graph/tree square's exact presentation domain, because graph readback orders unpositioned arcs by their retained edge ids. Fan sources are outside this law's stated fragment; dispatch has separate tests for fan conversion and fallback.

### Definitions used

The graph form and embedding are those of [L-lenses-5](#L-lenses-5); the square is that of [L-lenses-17](#L-lenses-17).

### Proof

1. On a tree-shaped input the walk from a surviving node through dropped nodes to the next surviving node is unique, so the graph contraction stage and tree ancestor contraction choose the same pairs, labels and expansions (definition of the contraction plan on a tree).
2. With no subtree drop and the root kept, every surviving tree node keeps its parent arc or acquires the contracted arc of step 1. The graph view is thus the graph form of the tree view, as shown in [L-lenses-5](#L-lenses-5), step 5.
3. Synthesized objects receive ids in ascending synthesis-key order from the floor on both routes (definition of allocation), so the mapped ids coincide.
4. The two complements correspond field for field under the conversion of [L-lenses-17](#L-lenses-17), so the two puts restore the same source ([L-lenses-5](#L-lenses-5) step 5).

### Executable counterpart

- Law ID: `lenses.l9`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_graph_tree_route_law_case; bound: schemas=16, nonfan_stages=29, nodes=3, schema_tree_states=2020, stage_tree_bases=2548, expanded_edited_views=28874, named_nonfan_validation_cases=2, base_cases=2550, observations=31424`
- Exhaustive differential test: `graph_tree_route_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=graph_tree_route_law_certificate)'`
- Property differential test: `randomized_graph_tree_route_law_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_graph_tree_route_law_differential)'`
- File: `crates/panproto-lens/src/dispatch.rs`
- File: `crates/panproto-lens/src/asymmetric.rs`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-inst/src/error_detail.rs`
- File: `crates/panproto-inst/src/view_ref.rs`
- File: `crates/panproto-reference/src/pages/lenses/squares.rs`
- File: `crates/panproto-reference/src/pages/lenses/tree.rs`
- File: `crates/panproto-reference/src/pages/lenses/domain.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`
- File: `crates/panproto-lens/tests/support/tree_validation.rs`
- File: `crates/panproto-lens/tests/reference_tree_validation.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Symmetric spans and isomorphism classifiers agree across shapes {#L-lenses-10}

### Statement

Two construction families use the same lens value across shapes. First, graph and table symmetric lenses are spans of two asymmetric lenses out of one middle schema, built only when the two middle schemas have equal canonical bytes. Second, the graph, tree and table isomorphism classifiers report no obstruction only on their certified residue-free fragments, so complement-free put inverts get on every admitted source; each classifier reports a concrete obstruction outside that fragment, and complement-free put refuses on it. For tables, the classifier supplies an operational left inverse on finite, structurally schema-supported `FInstance` values for which get succeeds. Target protocol validation lies outside that raw carrier, so the result does not establish a bijection between source- and target-validation carriers.

### Outside the domain

Two spans whose middle schemas differ: `NoSharedMiddle`. Complement-free put of a lens with an obstruction: `NotAnIsomorphism`.

### Definitions used

Lens, complement and identity envelope are defined at the head of this page.

### Proof

1. A span is built from two asymmetric lenses whose sources are the middle schema, and construction compares the canonical bytes of the two middle schemas and refuses with `NoSharedMiddle` when they differ (definition).
2. The graph classifier reports no obstruction exactly when the plan compiles; the source-to-target vertex and edge maps are bijections; every source vertex survives unconditionally; the transform programs are empty; both schemas have no hyper-edge; and neither schema imposes a required-cell or key-validation obligation. The tree classifier imposes the same source and target validation exclusion because tree get does not validate its source while tree put validates the reconstruction. The table classifier requires table, relation and column bijections and excludes source required-value and key obligations that table get admits but table put may reject. It does not inspect target validation, which neither raw table get nor raw table put evaluates (definition of the three classifiers).
3. Under the graph conditions, get keeps every node and edge, records no anchor because every target vertex has one source preimage, records no label because every target edge has one source preimage, changes no field or value, and synthesizes nothing ([L-lenses-2](#L-lenses-2) steps 1 to 3). Target surjectivity is needed here: a target-only carrier would make get fail to cover the advertised view space even though it records no source residue. The absence of validation declarations on both sides fixes the carrier on which the inverse is claimed. The complement's residue is empty, so the stamped empty complement is the complement of every source, and put returns the source by [L-lenses-2](#L-lenses-2) steps 5 and 6.
4. Conversely each reported graph obstruction identifies a failed part of this bijection or a coordinate on which get records residue or put may refuse: this includes a non-surviving or conditional source vertex, a target-only vertex or edge, a shared image, a transform program, a non-kept or multiply covered edge, a source or target hyper-edge, and a source or target validation declaration. The structural oracle constructs a finite witness for every obstruction class. The tree oracle does the same in its certified fragment. The table oracle covers the structural fragment; supplemental operational witnesses cover source required-value and key refusals and show separately that target validation does not enter the raw carrier. Complement-free put refuses with `NotAnIsomorphism` whenever the relevant classifier reports an obstruction ([L-lenses-3](#L-lenses-3) step 5).

### Executable counterpart

- Law ID: `lenses.l10`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_lens_construction_case; bound: family_bundles=10, stage_classifications=31, graph_restorations=117004, tree_restorations=2082, structural_witnesses=21, value_witnesses=8, tree_open_field_witnesses=1, unit_boundaries=9, resolver_witnesses=2, table_projection_cases=9921, table_writer_cases=81, span_assertions=4, validation_boundaries=5`
- Exhaustive differential test: `reference_lens_constructions; command: cargo nextest run --release -p panproto-lens --test reference_lens_constructions --no-tests=fail -E 'test(=reference_lens_constructions)'`
- Property differential test: `randomized_lens_construction_differential; command: cargo nextest run --release -p panproto-lens --test reference_lens_constructions --no-tests=fail -E 'test(=randomized_lens_construction_differential)'`
- File: `crates/panproto-lens/src/graph_symmetric.rs`
- File: `crates/panproto-lens/src/table_symmetric.rs`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-lens/src/table_lens.rs`
- File: `crates/panproto-lens/src/asymmetric.rs`
- File: `crates/panproto-lens/src/lib.rs`
- File: `crates/panproto-reference/src/pages/lenses/graph.rs`
- File: `crates/panproto-reference/src/pages/lenses/table.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`
- File: `crates/panproto-lens/tests/reference_lens_constructions.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## The table complement records exactly what table get forgets {#L-lenses-11}

### Statement

For a table lens whose plan compiles and a source table on which table get succeeds, the complement holds the following entries in serialized field order. First, `dropped_rows` holds the rows of dropped tables and the rows failing selection, each with its source table and cells; `dropped_cells` holds each source column of a surviving row of which the view row holds no cell of that name, renamed columns included, with its source value; and `dropped_fk_pairs` holds, per source edge, the pairs absent from the view. Next, `original_fk_edges` holds a view pair recorded exactly when the preimage rule of [L-lenses-4](#L-lenses-4) would not return its source edge, with more than one element exactly when the edge map merged pairs; `original_tables` holds a row recorded exactly when the unique reverse vertex map would not return its source table; and `original_cells` holds each source column of a surviving row whose view cell of that name holds another value, with the source value. The final entries are `synthesized_fk_pairs` (the contracted pairs), the row id floor, and the identity envelope. The residue is the dropped and original maps; the synthesized pairs, floor and identity are not residue; the reserved row ids are the keys of `dropped_rows`. The envelope semantically fixes the source schema, target schema, migration, functor complement shape and format version.

### Outside the domain

A table plan that does not compile, a migration that expands, or a structural migration that disagrees with the table functor: table get refuses and returns no complement. The finite outside-domain certificate isolates `NotTableRepresentable`, `SurvivalRulesDisagree`, `IncoherentEdgeImage`, `NoEdgeFound`, `AmbiguousEdge`, and `IncoherentPairImage`. It also checks a source outside its relational schema, a refusing predicate and a refusing transform, which return `NonConforming` or the corresponding typed evaluation refusal without a complement.

### Definitions used

Complement, identity envelope and the preimage rule are defined at the head of this page and in [L-lenses-4](#L-lenses-4). A table plan is the restrict plan compiled together with the table functor of the two relational boundaries.

### Proof

1. Table get decides each row's survival by its table's survival under the table plan and its survival predicate; a row that does not survive goes to `dropped_rows` with its table and cells, and no other row does (definition of table get).
2. For a surviving row, each source column is carried by its cell fate: kept under its name, renamed, or dropped. The complement compares the resulting view row with the source coordinate rather than inferring residue from the fate alone. If the view has no same-named cell, the source value is recorded in `dropped_cells`; if it has a same-named cell with another value, the source value is recorded in `original_cells`. This case split also covers swaps and many-to-one renames, where another source column may place a cell under the renamed column's source name (definition of the cell stage).
3. A pair whose endpoints do not both survive, or whose edge fate is drop, is recorded in `dropped_fk_pairs`; a kept pair is recorded in `original_fk_edges` exactly when its view label does not map back to its source edge; a merged pair records each source edge (definition of the pair stage).
4. A surviving row's table is recorded in `original_tables` exactly when the reverse vertex map does not return it (definition of the row stage).
5. Contracted pairs are synthesized into the view and listed in `synthesized_fk_pairs`; the floor is one above the largest source row id; get stamps the envelope (definition).
6. Production serialization visits the complement fields in declaration order. The reference `FIELD_ORDER` fixes the same nine names, while the semantic-envelope comparison checks both endpoint schemas, the induced migration, the functor shape and the format version (definition of the two identity envelopes). Steps 1 to 6 fill exactly the listed fields with exactly the listed contents.

### Executable counterpart

- Law ID: `lenses.t3`
- Reference: `panproto_reference::table::get`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_table_complement_law_case; bound: fixture_cases=9921, field_order=1, named_constructions=7, named_typed_refusals=9, cases=9938`
- Exhaustive differential test: `table_complement_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=table_complement_law_certificate)'`
- Property differential test: `randomized_table_complement_law_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_table_complement_law_differential)'`
- File: `crates/panproto-inst/src/table_complement.rs`
- File: `crates/panproto-inst/src/identity.rs`
- File: `crates/panproto-lens/src/table_lens.rs`
- File: `crates/panproto-lens/src/asymmetric.rs`
- File: `crates/panproto-lens/src/identity.rs`
- File: `crates/panproto-reference/src/pages/lenses/table.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`
- File: `crates/panproto-lens/tests/reference_lenses.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Table get and put, with put writing through the last writer {#L-lenses-12}

### Statement

Table get is the table restriction along the table plan compiled for the two boundary protocols, stamped with the table identity envelope; table put verifies the envelope, evaluates the put domain of [L-lenses-4](#L-lenses-4) with rows and pairs for nodes and edges, and on the domain restores. Restoration of a row takes its recorded table only when that table's image is the row's view table, otherwise the unique reverse table. A synthesized-only view pair is read only. When one view pair has both a contracted origin and one direct origin, removal is still refused as an edit to synthesized structure, while the unedited put restores the uniquely recoverable direct origin. A dropped column of a view-inserted row stays absent. Cells follow the baseline-and-pullback rule: complement residue with the view's same-named cells reconstructs a baseline source, equal to the original source when the view is unedited, and each changed view cell is written back through the table plan's last writer of its column, so an edit to a renamed column is written to the source column.

### Outside the domain

A view outside the put domain: `PutDomain` with the first violation, among them `IdCollision` for a reserved row id, `SynthesizedEdited` for a removed two-origin pair, `AmbiguousReverseAnchor` for a moved row with several preimage tables, `DerivedCellEdited` and `TransformNotInvertible` for edited derived cells. A complement from another lens or shape: `Identity`.

### Definitions used

The table complement is that of [L-lenses-11](#L-lenses-11). The last writer of a target column $t$ of table $T$ is the source of the last explicit `Rename(_, t)` in cell-fate order. If no such rename exists, the effective writer is the implicit source column $t$ exactly when $t$ is a source column with no explicit drop or rename fate; otherwise the cell is derived and has no writer.

### Proof

1. Table get compiles the table plan and runs the table restriction, then stamps the envelope (definition), so its view and complement are those of [L-lenses-11](#L-lenses-11).
2. Put verifies the envelope and evaluates the domain clauses on the reconstructed baseline before restoring (definition of table put), as in [L-lenses-3](#L-lenses-3) steps 2 to 4.
3. Baseline: for each surviving row, the source cells are the view cells of the same name, overridden by `original_cells` and supplemented by `dropped_cells` (definition). On an unedited view the view cells are the images of the source cells, so the baseline equals the source.
4. A changed view cell at column $t$ is assigned to the last explicit `Rename(_, t)`, if one exists. Otherwise it is assigned to the implicit source column $t$ only when no fate displaces that column and the source table has that column. This is the plan's effective writer rule (definition of writeback), so a renamed column's edit reaches its source column and a many-to-one rename writes the last source in fate order. A target-only column fails the source-column check and returns `DerivedCellEdited`.
5. The row and pair restoration rules are the clauses of [L-lenses-4](#L-lenses-4) read on rows and pairs. Pair restoration first uses recorded non-unique origins, then a unique direct preimage. Only a pair with neither kind of source origin falls through to the synthesized-pair rule. An inserted contraction reports `ContractedEdgeInserted` before the catch-all `UnrepresentableViewEdge`, so the complete report does not assign both refusals to the same pair. This order gives the stated insertion and two-origin behaviour.

### Executable counterpart

- Law ID: `lenses.t4`
- Reference: `panproto_reference::table::TablePlan::writer`
- Reference: `panproto_reference::table::compile_for_tables`
- Reference: `panproto_reference::table::get`
- Reference: `panproto_reference::table::TableLens::put`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_table_get_put_law_case; bound: projection_observations=156513, edited_writer_views=180, of_which_two_explicit_writer_edits=36, derived_column_edits=2, relation_pair_observations=5, named_identity_and_domain_refusals=4, cases=156704`
- Exhaustive differential test: `table_get_put_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=table_get_put_law_certificate)'`
- Property differential test: `randomized_table_get_put_law_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_table_get_put_law_differential)'`
- File: `crates/panproto-lens/src/table_lens.rs`
- File: `crates/panproto-lens/src/table_shape.rs`
- File: `crates/panproto-inst/src/error_detail.rs`
- File: `crates/panproto-inst/src/view_ref.rs`
- File: `crates/panproto-inst/src/table_complement.rs`
- File: `crates/panproto-reference/src/pages/lenses/table.rs`
- File: `crates/panproto-reference/src/pages/lenses/domain.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`
- File: `crates/panproto-lens/tests/reference_lenses.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## The identity path is a left unit {#L-lenses-13}

### Statement

For every lens path $p$ from $S$, $\operatorname{id}_S \mathbin{;} p = p$: the composite has the atomic stages of $p$, the identity envelopes of $p$, and the get and put of $p$ on every source and view.

### Outside the domain

A path whose boundary endpoint differs from the identity's path boundary: composition refuses with `CompositionMismatch`, and no composite exists. A difference confined to validation declarations or other non-boundary annotations is not a mismatch.

### Definitions used

A lens path is the free-category word over atomic lens stages. A path endpoint is the path boundary defined at the head of this page. Production represents the distinguished identity at a schema by one identity lens plus an identity marker, while the reference represents it by the empty word at that boundary; composition `then` removes the distinguished representation after checking its endpoint. It otherwise concatenates stages and never fuses.

### Proof

1. Composition checks that the target endpoint of the left path equals the source endpoint of the right path, refusing otherwise (definition of `then`).
2. When the left path is the distinguished identity at $S$ and the endpoints agree, `then` returns the right path unchanged (definition), so the stage sequence and envelopes are those of $p$.
3. Get and put of a path are folds over its atomic stages (definition of the folds), so equal stage sequences give equal get and put.

### Executable counterpart

- Law ID: `lenses.path-left-identity`
- Reference: `panproto_reference::path::LensPath::then`
- Reference: `panproto_reference::path::get_path`
- Reference: `panproto_reference::path::put_path`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_path_law_case; bound: left_unit_words=537, right_unit_words=537, associative_triples=29791, graph_operational_cases=1855809, tree_fusion_cases=15596, table_fusion_cases=100811, named_table_boundaries=4, target_only_relational_subcases=3, named_path_boundaries=3, structural_key_mismatch_subcases=3, named_tree_fusion_boundaries=8, cases=2003096`
- Exhaustive differential test: `lens_path_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=lens_path_law_certificate)'`
- Property differential test: `randomized_path_left_identity_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_path_left_identity_differential)'`
- File: `crates/panproto-lens/src/lens_path.rs`
- File: `crates/panproto-lens/src/compose.rs`
- File: `crates/panproto-reference/src/pages/lenses/path.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## The identity path is a right unit {#L-lenses-14}

### Statement

For every lens path $p$ to $T$, $p \mathbin{;} \operatorname{id}_T = p$: the composite has the atomic stages of $p$, the identity envelopes of $p$, and the get and put of $p$ on every source and view.

### Outside the domain

A path whose boundary endpoint differs from the identity's path boundary: composition refuses with `CompositionMismatch`, and no composite exists. A difference confined to validation declarations or other non-boundary annotations is not a mismatch.

### Definitions used

A lens path is the free-category word over atomic lens stages. A path endpoint is the path boundary defined at the head of this page. Production represents the distinguished identity at a schema by one identity lens plus an identity marker, while the reference represents it by the empty word at that boundary; composition `then` removes the distinguished representation after checking its endpoint. It otherwise concatenates stages and never fuses.

### Proof

1. Composition checks that the target endpoint of the left path equals the source endpoint of the right path, refusing otherwise (definition of `then`).
2. When the right path is the distinguished identity at $T$ and the endpoints agree, `then` returns the left path unchanged (definition), so the stage sequence and envelopes are those of $p$.
3. Get and put of a path are folds over its atomic stages (definition), so equal stage sequences give equal get and put.

### Executable counterpart

- Law ID: `lenses.path-right-identity`
- Reference: `panproto_reference::path::LensPath::then`
- Reference: `panproto_reference::path::get_path`
- Reference: `panproto_reference::path::put_path`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_path_law_case; bound: left_unit_words=537, right_unit_words=537, associative_triples=29791, graph_operational_cases=1855809, tree_fusion_cases=15596, table_fusion_cases=100811, named_table_boundaries=4, target_only_relational_subcases=3, named_path_boundaries=3, structural_key_mismatch_subcases=3, named_tree_fusion_boundaries=8, cases=2003096`
- Exhaustive differential test: `lens_path_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=lens_path_law_certificate)'`
- Property differential test: `randomized_path_right_identity_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_path_right_identity_differential)'`
- File: `crates/panproto-lens/src/lens_path.rs`
- File: `crates/panproto-lens/src/compose.rs`
- File: `crates/panproto-reference/src/pages/lenses/path.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Path composition is associative {#L-lenses-15}

### Statement

For lens paths $p$, $q$, $r$ with matching path boundaries, $(p \mathbin{;} q) \mathbin{;} r = p \mathbin{;} (q \mathbin{;} r)$ as atomic stage sequences (each stage compared by its lens digest, both schemas included) and as identity-envelope sequences; when a path boundary does not match, both bracketings refuse.

### Outside the domain

A mismatched path boundary between $p$ and $q$ or between $q$ and $r$: each bracketing refuses with `CompositionMismatch` at the mismatched boundary. Distinct adjacent schema annotations on an equal path boundary remain part of their respective stages and do not cause this refusal.

### Definitions used

Lens path, the distinguished identity and composition are defined in [L-lenses-13](#L-lenses-13).

### Proof

1. When none of $p$, $q$, $r$ is the distinguished identity, both bracketings are the concatenation of three stage sequences after two boundary checks, and concatenation is associative.
2. When one of them is the distinguished identity, [L-lenses-13](#L-lenses-13) and [L-lenses-14](#L-lenses-14) reduce both bracketings to the composite of the other two, which is the same.
3. Boundary checks: each bracketing compares the endpoints of $p$ and $q$ and of $q$ and $r$ (the endpoint of a composite is that of its outer stage), so one bracketing refuses exactly when the other does.

### Executable counterpart

- Law ID: `lenses.path-associativity`
- Reference: `panproto_reference::path::LensPath::then`
- Reference: `panproto_reference::path::get_path`
- Reference: `panproto_reference::path::put_path`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_path_law_case; bound: left_unit_words=537, right_unit_words=537, associative_triples=29791, graph_operational_cases=1855809, tree_fusion_cases=15596, table_fusion_cases=100811, named_table_boundaries=4, target_only_relational_subcases=3, named_path_boundaries=3, structural_key_mismatch_subcases=3, named_tree_fusion_boundaries=8, cases=2003096`
- Exhaustive differential test: `lens_path_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=lens_path_law_certificate)'`
- Property differential test: `randomized_path_associativity_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_path_associativity_differential)'`
- File: `crates/panproto-lens/src/lens_path.rs`
- File: `crates/panproto-lens/src/compose.rs`
- File: `crates/panproto-reference/src/pages/lenses/path.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Fusion preserves the sequential behaviour of a path {#L-lenses-16}

### Statement

For every graph or table lens path and source, the normal-form get equals the left-to-right get fold over the atomic stages, and the normal-form put equals the right-to-left put fold. The same statement holds for a tree path when every atomic execution lies in the common reference tree domain. If either execution refuses, so does the other, with the same error attached to the normal-form group that contains the refusing atomic stage.

Two tree stages satisfy the **tree fusion criterion** exactly when:

1. their compiled migrations compose, and the first-stage, second-stage and composite plans all compile; none contracts or expands a path, and every edge fate is `Keep` or `Drop`;
2. the composite gives every source vertex the pair's survival verdict and surviving image, and gives every direct source edge the pair's fate;
3. the first stage is a complement-`Unit` bijection, so its source and target impose no validation obligations, and every target vertex and direct edge of the second stage has a source preimage;
4. the second stage's representation of the middle schema declares no required cell or uniqueness key; the first-stage representation is already covered by condition 3;
5. the first stage has no value layer, and a second-stage value layer is admitted only when the first stage is a structural identity on carriers and every effective second-stage value-layer entry is keyed by a declared middle carrier;
6. before value-layer evaluation, both structural plans fix every declared carrier and every raw vertex-remap entry, including an entry outside the declared schema, is a self-map; and
7. neither stage retargets a fan, and none of the four schema objects representing the source, the two copies of the middle boundary and the target declares a hyper-edge.

Table stages must also satisfy the **table fusion criterion**:

1. the three table plans compile under their boundary protocols;
2. the first table stage is a complement-`Unit` bijection, so its table, relation and column maps are both injective from the source and surjective onto the middle relational schema;
3. the first target and second source relational schemas are equal, and the second source has no protocol-derived required-value obligation;
4. the first stage carries neither field transforms nor op-term assignments;
5. the three entity functors compose literally on every source table, relation and column, and the second-stage column map is injective within each middle table; and
6. if the second stage has a value layer, the source and middle protocols are equal and the first plan has equal source and target relational schemas.

### Outside the domain

Four inputs lie outside the domain: a path whose complement has the wrong number of entries yields `PathComplementMismatch`; a refusing stage yields `AtStage` with the group and its atomic stages; a table path with the wrong number of protocols yields `PathContextMismatch`; and a tree execution outside the common reference tree domain is not part of the cross-implementation theorem. The latter still has a production consistency regression comparing the normal form with the unfused production fold.

### Definitions used

Lens path and composition are defined in [L-lenses-13](#L-lenses-13). The normal form is the result of repeatedly fusing the leftmost admitted adjacent pair of groups.

### Proof

1. For one admitted pair $(\ell_1, \ell_2)$ with fused lens $\ell$, neither stage nor the composite contracts or expands, and every edge fate is keep or drop. Each structural get maps nodes along its vertex map and keeps or drops direct edges. Migration composition, the explicit source-frame vertex comparison and the direct-edge comparison together show that $\ell$ assigns every source carrier the image assigned by the pair. A compiled migration records surviving target names, so the explicit vertex comparison rules out two mismatches: a remap onto such a name may otherwise resurrect a source vertex dropped by $\ell_1$, while an omitted remap may retain the wrong image for an isolated vertex. Hyper-edges remain atomic because even a fan identity can retain a noncanonical raw label in its first complement and normalize it before the second stage; one fused complement cannot retain both observations. For tables, literal functor composition gives the same target coordinate for every source table, relation and column. Injectivity of the second column map preserves the finite-map collision winner, since deleting the intermediate column-name order cannot select a different source cell. These facts make the fused and sequential views equal.
2. The first stage has no value layer. If the second has one, the first is a structural identity on carriers, so the second layer reads the same nodes and fields in both executions. Each effective second-stage value-layer entry is keyed by a declared middle carrier and thus has the corresponding identity preimage in the first stage. This condition matters for edited outside-schema anchors: migration composition intentionally drops a raw entry with no first-stage preimage, while the sequential second stage can still consult it. The second layer may still change or remove declared nodes; its semantics is retained in the fused lens. On tables, a first-stage field transform or op-term assignment remains atomic. The second stage still checks the intermediate finite map against its relational source boundary. When the second stage has a value layer, equality of the source and middle protocols and equality of the first plan's relational boundaries make the typed placeholder for every absent declared cell the same. This gives the second layer the same finite map before and after fusion.
3. The first stage is complement-`Unit`, so it contributes no residue; the fused residue is the second-stage residue transported through the first-stage bijection. Both structural plans fix their carriers under the total reverse map, so reverse fibres are injective before value-layer evaluation, and the second-stage coverage condition gives every target carrier a preimage. Every declared reverse fibre thus follows the same unique preimage in the fused and sequential puts. The structural-identity restriction preserves the put-domain check performed by each operational identity: deleting the sole identity beside a nonidentity stage could otherwise accept an intermediate graph that the identity rejects. It also makes identity extension compose for an edited anchor outside the declared target schema: an undeclared raw remap cannot introduce a second rename that the fused partial inverse would omit. Edge preimages are built from every compiled `Keep` fate, including implicit keeps, so explicit materialization in the composite does not change the reverse edge fibre.
4. Put runs the second stage before the first. The first stage has no source-validation obligation, both middle representations have trivial put domains, and the table rule excludes protocol-derived middle validation. This makes a second-stage refusal the first refusal of the fused put; fusion cannot replace it with an independent first-stage residue or validation error. Together with step 3, this shows that puts return equal sources or equal errors at the same normal-form group.
5. A plan that fails to compile refuses every source. Requiring the two stage plans and the composite plan to compile prevents normalization from hiding that refusal.
6. By induction on the number of fusions in the leftmost strategy, each fusion replaces an adjacent pair by a lens with the same behaviour (steps 1 to 5), so the normal form's fold equals the atomic fold on graph and table instances and on tree instances in the common reference domain.

### Executable counterpart

- Law ID: `lenses.fusion-soundness`
- Reference: `panproto_reference::path::get_path`
- Reference: `panproto_reference::path::put_path`
- Reference: `panproto_reference::lens::TreeShape::get`
- Reference: `panproto_reference::lens::TreeShape::put`
- Reference: `panproto_reference::lens::TableShape::get`
- Reference: `panproto_reference::lens::TableShape::put`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_path_law_case; bound: left_unit_words=537, right_unit_words=537, associative_triples=29791, graph_operational_cases=1855809, tree_fusion_cases=15596, table_fusion_cases=100811, named_table_boundaries=4, target_only_relational_subcases=3, named_path_boundaries=3, structural_key_mismatch_subcases=3, named_tree_fusion_boundaries=8, cases=2003096`
- Exhaustive differential test: `lens_path_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=lens_path_law_certificate)'`
- Property differential test: `randomized_path_fusion_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_path_fusion_differential)'`
- File: `crates/panproto-lens/src/lens_path.rs`
- File: `crates/panproto-lens/src/compose.rs`
- File: `crates/panproto-lens/src/asymmetric.rs`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-lens/src/table_lens.rs`
- File: `crates/panproto-lens/src/lens_shape.rs`
- File: `crates/panproto-lens/src/table_shape.rs`
- File: `crates/panproto-inst/src/error_detail.rs`
- File: `crates/panproto-inst/src/view_ref.rs`
- File: `crates/panproto-reference/src/pages/lenses/path.rs`
- File: `crates/panproto-reference/src/pages/lenses/domain.rs`
- File: `crates/panproto-reference/src/pages/lenses/graph.rs`
- File: `crates/panproto-reference/src/pages/lenses/laws.rs`
- File: `crates/panproto-reference/src/pages/lenses/table.rs`
- File: `crates/panproto-reference/src/pages/lenses/tree.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`
- File: `crates/panproto-lens/tests/reference_lenses.rs`

The bounded predicate census includes `DropF;ReadF`: composing the migrations returns `ComposeUnboundField` because the second stage reads `f` after the first drops it, so normalization retains the two atomic stages. It also includes a separate `Op` stage, which exercises op-term execution rather than treating that route as another field transform. Eight first-class tree-fusion witnesses cover the following boundaries: structural identity before a value layer; target-only vertices; target-only direct edges; staged error precedence; validation on the second representation of the middle boundary; edited anchors outside the declared schema; extraneous reverse-map entries; and value-layer entries at undeclared anchors. The L10 classifier certificate covers validation on the first representation. The table certificate separately exercises protocol-context compilation. On graph and table carriers, the certificate compares every final-stage Single-Step Basis view, including declared refusals. On the tree carrier, it does the same inside the reference tree domain; outside that common domain, it compares the production normal form with an unfused production fold on get and the unedited put. The certificate runs the fold comparison on every counted path-state.

The table path carrier uses C11 relational boundaries with no declared columns, so its finite-map preflight covers unknown tables and stray cells but cannot itself express literal column-map composition. The executable umbrella adds four independent named constructions: a noninjective second-stage column map, protocol-dependent placeholders, required-value validation at the middle boundary and a target-only relational carrier. The last construction bundles target-only table, relation and column subcases, and checks the classifier's coordinate-specific obstruction before checking that normalization retains both atomic groups. The structural path-boundary construction similarly bundles vertex-key, edge-key and hyper-edge-key mismatches. These bundled coordinates do not add named cases, so the cardinality remains 2,003,096.

The manifest test derives the expected case stream from the C11 endpoints and the independently pinned graph, tree and table state counts. It compares every emitted word, stage triple, carrier, source index and named witness in order while retaining only one expected payload at a time. The associativity property exercises a mismatched triple on every run in addition to a composable triple, and the fusion property runs all four table, three path-boundary and eight tree-fusion witnesses on every run.

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## The graph and tree routes of a lens commute {#L-lenses-17}

### Statement

For a tree $W$ and a lens, consider two routes: (i) graph get and put applied to the graph form of $W$, and (ii) tree get and put applied to $W$ and mapped back through the recorded embedding. Wherever both routes lie in their exact common domain, they agree on views, complements under the field-for-field conversion, put results and errors; child order is compared separately. Error agreement is stated in public graph coordinates: the square applies the durable-embedding graph projection to the native tree report before comparing it with the graph route. This projection reports `Edge(id)` for an arc violation and applies the graph-specific minimum-edge rule for sibling order; it does not equate a raw `Arc { parent, child }` value with a raw edge value. The round trip is exact on ids: reading the graph form of $W$ as a tree returns $W$, while taking the graph form of a tree read from a tree-shaped graph $G$ returns $G$ through the embedding.

### Outside the domain

A route outside the exact common domain (a subtree drop, a pruned root, a fan the tree route cannot carry, or an edited child order not represented by the durable graph embedding) is reported as refused rather than compared; on such inputs dispatch runs the graph route ([L-lenses-9](#L-lenses-9)).

### Definitions used

The graph form, the embedding and the conversion of complements are those of [L-lenses-5](#L-lenses-5) and [L-lenses-9](#L-lenses-9).

### Proof

1. The graph form of a tree assigns each node its id and each arc and fan edge an id recorded in the embedding; reading the graph back as a tree inverts this assignment when the embedding represents the tree's presentation (definition of the encoding), which gives the exact round trip. A sibling reorder that retains the old arc ids is detected as a presentation-domain refusal before the square compares its routes.
2. `graph_square` compares graph get and the unedited put with the tree fast route. `graph_put_square` performs the same comparison for a caller-supplied edited tree presentation and its durable embedding. In their common domain, [L-lenses-9](#L-lenses-9) gives equal views and corresponding complements on the two routes, and [L-lenses-5](#L-lenses-5) step 5 gives equal puts after the explicit arc-to-edge projection.
3. A graph put-domain refusal is checked before dispatch enters the tree engine, so it remains the exact observable public error. If the graph precheck succeeds but the fast route refuses, dispatch reruns the graph route. If the fast route cannot preserve the edited presentation, the square rejects the input as outside its exact presentation domain. Thus the public square compares graph-coordinate results or reports the input outside its exact presentation domain; the native arc-coordinate law certificate remains the separate claim of [L-lenses-8](#L-lenses-8).

### Executable counterpart

- Law ID: `lenses.fb-graph-square`
- Reference: `panproto_reference::lens::graph_square`
- Reference: `panproto_reference::lens::graph_put_square`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_c11_tree_square_case; bound: schemas=16, nonfan_stages=29, nodes=3, schema_tree_states=2020, stage_tree_bases=2548, expanded_edited_views=28874, observations=31422`
- Exhaustive differential test: `graph_square_agrees_with_the_reference_exhaustively; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=graph_square_agrees_with_the_reference_exhaustively)'`
- Property differential test: `randomized_graph_square_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_graph_square_differential)'`
- File: `crates/panproto-lens/src/dispatch.rs`
- File: `crates/panproto-lens/src/asymmetric.rs`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-inst/src/error_detail.rs`
- File: `crates/panproto-inst/src/view_ref.rs`
- File: `crates/panproto-reference/src/pages/lenses/squares.rs`
- File: `crates/panproto-reference/src/pages/lenses/domain.rs`
- File: `crates/panproto-reference/src/pages/lenses/tree.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`
- File: `crates/panproto-lens/tests/reference_lenses.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## The graph and table routes of a lens commute {#L-lenses-18}

### Statement

For a table instance and a lens, the graph and table routes are compared only when both the graph plan and the protocol-indexed table plan compile and the lens satisfies the **table-shape criterion**:

1. no edge fate is `Expand` or `Unresolved`, no contraction expands, and no fan is retargeted;
2. explicit edge images are injective;
3. no vertex fibre combines an implicit self-image with another source table;
4. every structurally inferred kept edge is also an edge of the table functor;
5. every required or to-one target edge has exactly one lift from each source table in the fibre over its holder; and
6. the protocol-indexed cell plan is graph-neutral, with no `Drop` or `Rename` fate.

On a particular source, a synthesized contracted pair must also be distinct from every direct pair with the same target coordinates. Under these conditions, the graph route (graph get and put of the graph form of the table) and the table route (table get and put, mapped back through the table embedding) give identical views, complements under the field-for-field conversion, and put results wherever both routes are in their exact common domain.

### Outside the domain

A plan with a nonneutral structural cell fate, a migration outside the other table-shape conditions, or a source with coincident synthesized and direct pairs: the table route declines and the square reports it as refused; dispatch runs the graph route. A protocol classification failure or another table-route error likewise returns the graph route's result.

### Definitions used

The graph form of a table maps each row to the node with its id and each pair to an edge with the id the embedding assigns; the complement conversion maps dropped rows to dropped nodes, dropped pairs to dropped edges, recorded tables to recorded anchors, cells to fields, and synthesized pairs to synthesized edges. The relational certificate fixes two row ids for each of two tables, one scalar column on each table and one relation between them. Its five migrations are identity, column rename, column projection, relation projection and table projection. The two column migrations exercise the exact-domain boundary; the other three exercise route commutation.

### Proof

1. The graph form maps rows to nodes with their ids and cells to fields. It maps pairs in ascending order and records the assigned edge ids. Reading along that embedding recovers the table exactly, which proves the carrier part of the square.
2. Under table-shape preservation, graph restriction keeps a node exactly when table restriction keeps its row and keeps an edge exactly when table restriction keeps its pair. A graph-neutral cell plan leaves every column coordinate unchanged, so cell absence and scalar values also agree. The two gets thus produce the same view under the embedding. A column drop or rename is instead a structural table operation with no protocol-free graph counterpart, which is why condition 6 excludes it.
3. Each residue coordinate has the same source witness on both routes. Dropped rows correspond to dropped nodes, dropped pairs to dropped edges, original tables to original anchors, dropped and original cells to graph fields, and synthesized pairs to synthesized edges. The identity envelope and id floors are preserved by the conversion. Thus [L-lenses-11](#L-lenses-11) gives equal complements coordinate by coordinate.
4. Put first checks those equal envelopes and domains. It then restores rows and nodes, cells and fields, and pairs and edges from corresponding residue coordinates; a changed target cell uses the same last-writer preimage on both routes by [L-lenses-12](#L-lenses-12). Insertions, deletions and relation edits thus produce the same source, while a reserved row id produces the corresponding `IdCollision` on both routes.
5. The independent finite certificate represents rows, cells and relation pairs directly rather than calling either lens engine. For each table, it chooses a subset of two row ids; for each present row, it chooses an absent cell, zero or one; and it chooses every subset of the possible relation pairs. The closed form is $\sum_{a=0}^{2}\sum_{b=0}^{2}\binom{2}{a}\binom{2}{b}3^{a+b}2^{ab}=1{,}831$ source states. Across five migrations this gives 9,155 migration-state pairs. For identity, relation projection and table projection, the edit basis changes each visible cell, row and relation coordinate once, inserts one fresh row into each target table, and attempts every reserved-id collision after table projection. For column rename and column projection, it proves the exact-domain boundary by requiring a decline on all 1,831 states apiece. Exact assertions pin 61,126 observations, with per-migration counts $[24{,}957, 1{,}831, 1{,}831, 18{,}837, 13{,}670]$.

### Executable counterpart

- Law ID: `lenses.fb-table-square`
- Reference: `panproto_reference::lens::table_square`
- Reference: `panproto_reference::lens::table_put_square`
- Reference: `panproto_reference::lens::relational_table_get`
- Reference: `panproto_reference::lens::relational_table_put`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_relational_table_source; bound: tables=2, row_ids_per_table=2, scalar_cell_states=3, relations=1, source_states=1831, migrations=5, migration_states=9155`
- Fixture-square enumerator: `panproto_reference::enumerate::lenses::for_each_table_fixture; bound: projection_cases=9921, edited_views=146592, observations=156513`
- Exhaustive differential test: `graph_and_table_routes_commute_on_the_independent_relational_carrier; command: cargo nextest run --release -p panproto-lens --test reference_table_square_relational --no-tests=fail -E 'test(=graph_and_table_routes_commute_on_the_independent_relational_carrier)'`
- Fixture-square differential test: `table_square_agrees_with_the_reference_on_the_fixture_scope; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=table_square_agrees_with_the_reference_on_the_fixture_scope)'`
- Property differential test: `randomized_relational_table_square_differential; command: cargo nextest run --release -p panproto-lens --test reference_table_square_relational --no-tests=fail -E 'test(=randomized_relational_table_square_differential)'`
- File: `crates/panproto-lens/src/dispatch.rs`
- File: `crates/panproto-lens/src/table_lens.rs`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-inst/src/error_detail.rs`
- File: `crates/panproto-inst/src/view_ref.rs`
- File: `crates/panproto-lens/tests/reference_table_square_relational.rs`
- File: `crates/panproto-lens/tests/reference_lenses.rs`
- File: `crates/panproto-reference/src/pages/lenses/squares.rs`
- File: `crates/panproto-reference/src/pages/lenses/lens.rs`
- File: `crates/panproto-reference/src/pages/lenses/domain.rs`
- File: `crates/panproto-reference/src/pages/lenses/graph.rs`
- File: `crates/panproto-reference/src/pages/lenses/table.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## A protolens instantiates at a schema only where its precondition holds {#L-lenses-19}

### Statement

For a protolens with source transform $F$, target transform $G$ and schema $S$, checked instantiation first checks the precondition against $S$ and refuses before either transform is evaluated when it fails. When the precondition holds, the resulting lens has source $F(S)$, target $G(S)$ and the migration computed between them; its plan either compiles or refuses with the plan error. The executable schema certificate covers the four elementary cases in which $F$ is the identity, an ordered two-clause false precondition, and a reachable plan-compilation refusal. The instance certificate in [L-lenses-20](#L-lenses-20) includes those two refusals as application cases rather than inferring them from successful instantiations.

### Outside the domain

A schema that fails the precondition is refused before instantiation: the reference returns `FalsePrecondition`, whereas production returns `LensError::ProtolensInapplicable` with the complete nonempty list of failed clauses. The differential compares this typed inapplicability category and its ordering. Its two transform drivers count calls, and both counts remain zero. A plan that does not compile returns the normalized plan error; the boundary witness produces an expansion with 65,536 intermediate vertices and 65,537 hops, one more than a synthesized key can number.

### Definitions used

A protolens is a schema-indexed family of lenses given by a precondition and a construction of the target schema and migration from the source schema.

### Proof

1. Production's `instantiate_checked` and the reference schema-instantiation operation evaluate the precondition on $S$ and return the refusal when it does not hold, without evaluating either transform or constructing the migration (definition). The false-precondition witness registers counting, rejecting drivers for both transforms; both counters remain zero, while production retains the two failed clauses in their declared order and the reference retains the ordered conjunction. Production's lower-level `instantiate` remains available only for callers that have already discharged this precondition.
2. Otherwise it computes $F(S)$ and $G(S)$, constructs the migration between them, and binds the three as a lens (definition). In the four elementary cases of the executable certificate, $F$ is the identity, so the lens source is $S$.
3. Compiling the plan of that lens is the plan compilation of [L-lenses-3](#L-lenses-3) step 1, which either succeeds or refuses. The plan-refusal witness starts with one direct source edge and uses a public enrichment transform to replace it with a path through 65,536 fresh intermediate vertices. Both compilers thus calculate 65,537 hops and return `ExpansionTooLong` with the same endpoints and count.
4. The executable oracle constructs each bounded reference target and migration from the elementary operation itself, without reading production's applicability verdict, target schema or compiled migration. On success, the differential compares the schemas exactly and the migrations by their exact observable semantics, quotienting only a resolver entry that repeats the unique direct target edge. The two boundary witnesses compare the refusal categories, clause order, transform-call counts and normalized plan-error payloads exactly.

### Executable counterpart

- Law ID: `lenses.protolens-schema-transform`
- Reference: `panproto_reference::lens::protolens_schema`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_protolens_schema_case; bound: cases=66, elementary_cases=64, refusal_witnesses=2, failed_precondition_clauses=2, expansion_intermediates=65536, expansion_hops=65537`
- Exhaustive differential test: `protolens_schema_law_certificate; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=protolens_schema_law_certificate)'`
- Property differential test: `randomized_protolens_schema_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_protolens_schema_differential)'`
- File: `crates/panproto-lens/src/protolens.rs`
- File: `crates/panproto-lens/src/graph_protolens.rs`
- File: `crates/panproto-lens/src/error.rs`
- File: `crates/panproto-reference/src/pages/lenses/protolens.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.

## Applying a protolens to an instance is get of its instantiation {#L-lenses-20}

### Statement

For a graph protolens, a schema $S$ and a graph instance $s$ of $F(S)$, instance application calls checked instantiation at $S$ ([L-lenses-19](#L-lenses-19)) and returns that lens together with $\operatorname{get}$ of it on $s$: the view and the complement bound to the instantiated lens's identity envelope, so a later put cannot detach the residue from the lens that produced it. We call the equality between instance application and instantiated get the protolens-instance graph-parity claim. Its finite certificate specializes to the four elementary cases in which $F$ is the identity and includes three application-level refusal boundaries explicitly.

### Outside the domain

A schema that fails the precondition is refused before either transform runs, as in [L-lenses-19](#L-lenses-19). A 65,537-hop plan refuses during checked instantiation, before graph get begins. The remaining named witness instantiates an exact identity lens over the fan schema and supplies a hyper-edge-shaped node with no parent edge; both implementations return the typed `MalformedFan` refusal from graph get rather than a partial application.

### Definitions used

Protolens and checked instantiation are those of [L-lenses-19](#L-lenses-19); graph get and the identity envelope are defined at the head of this page.

### Proof

1. Graph application calls `instantiate_checked` and returns its refusal unchanged (definition; [L-lenses-19](#L-lenses-19) step 1).
2. On success it runs get of the instantiated lens on $s$, which stamps the complement with that lens's envelope ([L-lenses-3](#L-lenses-3) step 1), and returns the lens, view and complement (definition). The differential independently recomputes the reference lens and checks that each returned complement carries that implementation's exact envelope. Because production and reference hash different canonical encodings, it compares the envelope's semantic preimage instead of equating the hash bytes. The source and target agree; independently compiled vertex images, edge fates, contraction pairs, hyper-edge resolution, conditional coordinates and transformed-coordinate sets agree; and the shape and format version agree. This comparison quotients a resolver entry only when it repeats the unique direct target edge.
3. The bounded product has four elementary protolenses and sixteen schemas. The independent graph-state count sums to 116,208 over those schemas, giving 464,832 elementary application cases. Three additional variants exercise the false precondition, the overlong plan and the malformed fan in their execution order. The certificate thus has 464,835 cases, and each refusal variant is dispatched by the exhaustive test rather than recorded as supplemental metadata.

### Executable counterpart

- Law ID: `lenses.protolens-instance-transform`
- Reference: `panproto_reference::lens::protolens_instance`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_protolens_instance_case; bound: schemas=16, protolenses=4, nodes=3, edges=3, graph_states=116208, elementary_cases=464832, false_preconditions=1, plan_refusals=1, get_refusals=1, refusal_witnesses=3, cases=464835`
- Exhaustive differential test: `protolens_transforms_agree_with_the_reference_exhaustively; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=protolens_transforms_agree_with_the_reference_exhaustively)'`
- Property differential test: `randomized_protolens_instance_differential; command: cargo nextest run --release -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_protolens_instance_differential)'`
- File: `crates/panproto-lens/src/protolens.rs`
- File: `crates/panproto-lens/src/graph_protolens.rs`
- File: `crates/panproto-reference/src/pages/lenses/protolens.rs`
- File: `crates/panproto-reference/src/enumerate/lenses.rs`

### Mutation equivalences

None.

### Audit record

- Author: lens-law sweep author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: audit pending.
