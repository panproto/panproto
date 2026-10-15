# Lenses

This page proves the laws of panproto's asymmetric lenses in their three instance shapes (graphs, tables and trees), of the lens path category built from them, of the commuting squares that relate the shapes, and of the protolens constructions that produce lenses from schemas. Each block states a law with its domain, proves it from the definitions below, and names the executable reference semantics and the differential tests that compare the production code with it.

A **lens** is a triple $\ell = (m, S, T)$ of a compiled migration $m$, a source schema $S$ and a target schema $T$. Its **plan** $P_\ell$ is the result of compiling $(S, T, m)$: the edge image (the fate of every source edge: kept as a target edge, dropped, contracted or expanded), the contraction plan, and the fan resolver. Compilation either succeeds or refuses with a typed plan error, and it reads no instance. A **source** is an instance of $S$ and a **view** an instance of $T$; the shapes are graphs (nodes and edges with ids), tables (rows with ids and foreign-key pairs) and trees (nodes, ordered arcs and fans).

The **get** of $\ell$ maps a source $s$ to a pair $(v, c)$ of a view and a **complement**: the part of $s$ outside the image of get, together with the coordinates get forgets (recorded anchors, fields, values and edge labels), the synthesized objects get created, an id floor and an identity envelope. The **identity envelope** $\iota(\ell, \sigma)$ of a lens at a shape $\sigma$ is a digest of the canonical bytes of $S$, $T$ and $m$ together with $\sigma$ and the complement format version; get stamps it on every complement it returns. The **put** of $\ell$ maps a view and a complement to a source. Its **domain** at a complement $c$ is the set $D_\ell(c)$ of views on which the put-domain predicate reports no violation; the predicate returns every violation, sorted by the report order (the variant, then the element), and put returns the first one when the view is outside the domain. A put first verifies the envelope of the complement it receives, so a complement produced by another lens or shape is refused before the domain is read.

We write $s \subseteq s'$ for **subinstance**: every node and edge of $s$ is a node and edge of $s'$ with the same id and the same content. A subinstance is **endpoint closed** when every edge it holds has both endpoints in it. A lens is on the **Delta fragment** when every surviving vertex and edge keeps its own name, the target schema is a subschema of the source schema, and the migration states no resolver, expansion, fan resolver, survival predicate or transform.

The **Single-Step Basis** of a view is the finite set of one-step edits a law checker tries: one alternate payload for each editable coordinate, every finite structural single edit, and the negative edits (reusing a reserved id, editing or removing a synthesized object, inserting a contracted edge, making a fan malformed), in the order of the mutation families. A **law report** runs five gates in order (GetPut, PutGet, Totality, PutTwice, Congruence) over the basis and reports the first that fails.

Every finite claim below is checked by an exhaustive differential test over a stated scope and by a property-based test over the same constructor families. Such a test reports that no counterexample was found through its printed bound; it does not prove the law, which the proof does.

## One lens value drives every shape {#L-lenses-1}

### Statement

A lens is exactly the triple $(m, S, T)$, with no further field and no options argument to its operations. For every shape, get and put are functions of the lens, of the shape's context (the two boundary protocols for tables, nothing for graphs and trees) and of their arguments. Two lenses with equal compiled migrations and equal schemas therefore have equal get, equal put, equal put domains and equal identity envelopes at every shape.

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
- Enumerator: `panproto_reference::enumerate::lenses::c11_graph_states; bound: schemas=16, stages=31, nodes=3, edges=3, graph_states=116208, fan_states=9940, stage_states=144397`
- Exhaustive differential test: `graph_lens_agrees_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=graph_lens_agrees_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
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

Let $\ell$ be a lens whose plan compiles and $s$ a source graph on which graph get succeeds, with $\operatorname{get}_\ell(s) = (v, c)$. Then $c$ holds, field for field: in `dropped_nodes` the nodes of $s$ absent from $v$, and in `dropped_edges` the edges of $s$ absent from $v$ (expanded source edges included); in `original_edges` exactly the surviving edges whose source label the preimage rule of [L-lenses-4](#L-lenses-4) would not return from their view label and endpoints; in `original_anchors` exactly the surviving nodes whose source anchor the unique reverse vertex map would not return; in `original_extra_fields` exactly the surviving nodes whose view fields differ from their source fields, with the source fields; in `original_values` exactly the surviving nodes whose view value differs from their source value, with the source value; in `synthesized_nodes` and `synthesized_edges` the objects get created, each under its synthesis key; the id floor of $s$; and the identity envelope $\iota(\ell, \text{graph})$. The pair $(v, c)$ determines $s$: get paired with its complement is injective on the sources where it succeeds.

### Outside the domain

A plan that does not compile, and an element on which restriction refuses, give no complement: get refuses with the plan error or with the evaluation error at the first element in error-key order, and no field of this statement applies.

### Definitions used

Lens, plan, get, complement and identity envelope are defined at the head of this page. The preimage rule and the reverse vertex map are those of [L-lenses-4](#L-lenses-4). A synthesis key names a synthesized object by the source data that produced it: a contracted pair with its label, or a hop of an expansion.

### Proof

1. Graph get first decides, node by node, whether a node survives (its anchor survives and its survival predicate holds); a node that does not survive is inserted into `dropped_nodes`, and nothing else is (definition of the restrict stage 0).
2. An edge whose endpoints do not both survive is inserted into `dropped_edges`; an edge between surviving nodes takes its fate from the edge image: kept (into the view, recorded in `original_edges` exactly when the preimage rule would not recover its label), dropped (into `dropped_edges`), contracted or expanded (into `dropped_edges`, with the synthesized replacements in the view and their keys in the synthesized maps) (definition of the restrict stage 1 and of the contraction stage).
3. For a surviving node, get records the source anchor when the reverse remap does not recover it, the source fields when they differ from the view fields as maps, and the source value when the presences differ (definition of stage 0). These are the iff conditions of the statement.
4. Steps 1 to 3 assign every node and edge of $s$ either to the view or to a dropped map, and every coordinate the view changes to an `original_*` map, so the fields hold exactly the listed contents.
5. Injectivity: graph put on $(v, c)$ with $v$ unedited restores every dropped node and edge, every recorded label, anchor, field and value, and removes every synthesized object, so it returns $s$ ([L-lenses-8](#L-lenses-8), GetPut). If $\operatorname{get}_\ell(s) = \operatorname{get}_\ell(s')$ then $s = \operatorname{put}_\ell(\operatorname{get}_\ell(s)) = \operatorname{put}_\ell(\operatorname{get}_\ell(s')) = s'$.

### Executable counterpart

- Law ID: `lenses.l2`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::c11_graph_states; bound: schemas=16, stages=31, nodes=3, edges=3, graph_states=116208, fan_states=9940, stage_states=144397`
- Exhaustive differential test: `graph_lens_agrees_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=graph_lens_agrees_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/graph_lens.rs`
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

## Graph get is stamped restriction and graph put is checked restoration {#L-lenses-3}

### Statement

For every lens $\ell$ and source graph $s$: $\operatorname{get}_\ell(s)$ is the restriction of $s$ along the plan compiled from $(S, T, m)$, with its complement stamped with $\iota(\ell, \text{graph})$. For every view $v$ and complement $c$: put first verifies the envelope of $c$ (a matching envelope is accepted, an empty unstamped complement is accepted as unbound, anything else is refused), then evaluates the put-domain predicate and returns its first violation when there is one, and otherwise returns the restored source; put is total on $D_\ell(c)$. The complement-free put of an isomorphism stamps an empty complement and puts; for a lens that is not an isomorphism it refuses. The fresh view ids follow the fresh-id rule over the view, the reserved ids and the synthesized ids, and never return the reserved top id.

### Outside the domain

A plan that does not compile: get refuses with the plan error. A complement whose envelope names another lens, shape or format: put refuses with `Identity` (`Mismatch`). A complement that records residue but carries no envelope: put refuses with `Identity` (`UnboundComplement`). A view outside $D_\ell(c)$: put refuses with `PutDomain` and the first violation in report order ([L-lenses-4](#L-lenses-4)). Complement-free put of a lens with residue: `NotAnIsomorphism`.

### Definitions used

Lens, plan, get, put, put domain, report order and identity envelope are defined at the head of this page; the put-domain predicate and restoration are those of [L-lenses-4](#L-lenses-4).

### Proof

1. Graph get compiles the plan and runs the graph restriction engine on it, then sets the complement's identity to $\iota(\ell, \text{graph})$ (definition of graph get); the reference get does the same with the reference restriction.
2. Checked put runs three steps in order: envelope verification, the domain predicate, restoration (definition of checked put). Envelope verification returns `Mismatch` for a present envelope different from $\iota(\ell, \text{graph})$, accepts an absent envelope exactly when every residue map of the complement is empty, and otherwise returns `UnboundComplement` (definition of verification).
3. When the domain predicate returns a non-empty sorted list, put returns its first element as `PutDomain`; otherwise put returns the restoration (step 2).
4. Totality on the domain: restoration is a total function of the view and complement once the predicate holds, because every clause that could fail during restoration (an unrepresentable edge, an ambiguous anchor, a refused survival predicate, an edited derived field, a duplicate key, a malformed fan) is one of the predicate's clauses, checked before restoration ([L-lenses-4](#L-lenses-4), steps 1 to 3). So a view in $D_\ell(c)$ never reaches a refusal.
5. Complement-free put: when the isomorphism classifier reports an obstruction the operation refuses with `NotAnIsomorphism`; otherwise it stamps an empty complement with $\iota(\ell, \text{graph})$ and runs step 2, whose verification accepts it (definition).
6. Fresh ids: the fresh node (edge) id is $\max(f, 1 + k)$ for the complement's node (edge) floor $f$ and the largest view, reserved or synthesized id $k$ (the floor when there is none), and the allocator refuses with `NodeIdsExhausted` (`EdgeIdsExhausted`) when that id would be the reserved top id, so the top id is never returned (definition of the fresh-id rule).

### Executable counterpart

- Law ID: `lenses.l3`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::c11_graph_states; bound: schemas=16, stages=31, nodes=3, edges=3, graph_states=116208, fan_states=9940, stage_states=144397`
- Exhaustive differential test: `graph_lens_agrees_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=graph_lens_agrees_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/graph_lens.rs`
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

## The graph put domain and restoration, and the cascade on the Delta fragment {#L-lenses-4}

### Statement

For a lens $\ell$, a complement $c$ produced by $\operatorname{get}_\ell$ and a view $v$, the put-domain predicate returns every violation of the following clauses, sorted by report order, and $v \in D_\ell(c)$ exactly when it returns none: (1) `IdCollision`, a view node or edge whose id is a dropped id; (2) `SynthesizedEdited`, a synthesized object missing from the view or changed, unless an endpoint of its chain was removed; (3) the edge back-mapping: a non-synthesized view edge maps back to its recorded label when `original_edges` records it with the same endpoints, else to the unique source edge whose fate keeps its label between preimages of its endpoints' anchors, and otherwise refuses with `ContractedEdgeInserted` (when the endpoints form a planned pair resolving to its label) or `UnrepresentableViewEdge`; a fan edge maps back through its hyper-edge's labels; (4) `AmbiguousReverseAnchor`, a view-added or re-anchored node whose anchor has more than one preimage; (5) `SurvivalPredicateFails` and `SurvivalPredicateRefused`; (6) `TransformNotInvertible` and `DerivedCellEdited`; (7) `DuplicateKey` and `MissingRequiredCell`; (8) `MalformedFan`. On the domain, put restores: the non-synthesized view nodes with their recorded anchors (when the recorded anchor's image is the view anchor), fields and values, the dropped nodes, the view edges with their mapped-back labels, and the dropped edges minus the cascade, which removes the dropped edges incident to a surviving source node absent from the view. On the Delta fragment, for every source $s$ with $\operatorname{get}_\ell(s) = (v, c)$ and every endpoint-closed $v' \subseteq v$, $r = \operatorname{put}_\ell(v', c)$ is the largest endpoint-closed subinstance of $s$ whose view is $v'$, and every endpoint-closed subinstance $t \subseteq s$ whose view is a subinstance of $v'$ is a subinstance of $r$.

### Outside the domain

A view with any violation of clauses (1) to (8) is outside $D_\ell(c)$; put refuses with `PutDomain` and the first violation, and the predicate returns the full sorted list. A complement from another lens is refused by envelope verification first ([L-lenses-3](#L-lenses-3)). The cascade statement is made only on the Delta fragment and only for deletion-only edits $v' \subseteq v$; outside it the cascade still runs but the largest-preimage reading is not claimed.

### Definitions used

Lens, put domain, report order, subinstance, endpoint closed and the Delta fragment are defined at the head of this page. The view of $t$ means the first component of $\operatorname{get}_\ell(t)$.

### Proof

1. The predicate evaluates each clause over every view element and collects the violations, then sorts them by the report order; restoration runs only when the list is empty (definition of the predicate and of checked put). The clauses are exactly (1) to (8).
2. Restoration builds the source from the view nodes, the dropped nodes, the mapped-back view edges and the dropped edges, then removes the cascade (definition of restoration). Each mapped-back label exists because clause (3) admitted the edge.
3. Now let $\ell$ be on the Delta fragment. Then get keeps a node exactly when its anchor is a surviving vertex, keeps an edge exactly when both endpoints are kept and its label is a surviving edge, changes no anchor, field, value or label, and synthesizes nothing (definition of get with no remap, resolver, expansion, fan resolver, predicate or transform). So $c$ records no `original_*` entry, `dropped_nodes` is the set of nodes with a non-surviving anchor, and `dropped_edges` is the set of edges not kept.
4. Let $v' \subseteq v$ be endpoint closed and $r = \operatorname{put}_\ell(v', c)$. Clause (1) holds because $v'$ uses only ids of $v$, which are disjoint from the dropped ids; clauses (2), (4) to (8) hold vacuously or because every view element is unchanged; clause (3) maps every edge of $v'$ back to its own label because the identity edge image keeps each surviving label as itself and the label is the unique preimage. So $v' \in D_\ell(c)$ and $r$ is defined.
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
- Enumerator: `panproto_reference::enumerate::lenses::for_each_gf5_deletion_edit; bound: fixtures=2, nodes=3, edges=3, sources=133, deletion_edits=604, source_subinstance_probes=5792`
- Exhaustive differential test: `the_cascade_agrees_with_the_reference_on_the_gf5_scope; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=the_cascade_agrees_with_the_reference_on_the_gf5_scope)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/graph_lens.rs`
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

Tree put runs the put-domain predicate before restoring and reports its violations with the same `PutDomainError` variants as graph put, among them `IdCollision` for a view node reusing a dropped node's id and `AmbiguousReverseAnchor` for a view-added or re-anchored node whose anchor has several preimages. The tree complement records a surviving node's source anchor exactly when the unique reverse vertex map would not return it, and tree put applies a recorded anchor only when its image under the vertex map equals the node's view anchor, so a view relabel is kept, not reverted. Complements are compared on the arc order, the identity and the recorded anchors. On the tree lens's domain (the lens keeps the tree shape and the root survives), tree get and put agree with graph get and put on the tree's graph form.

### Outside the domain

A tree view outside the put domain: tree put refuses with `PutDomain` and the first violation. A lens that drops an edge between two surviving vertices is outside the tree lens's domain, where the graph keeps the endpoints and drops the edge while the tree would drop the subtree; tree get is not claimed to agree there (the reference returns no result). A source whose root is pruned is refused with `RootPruned` and falls back to the graph path in dispatch ([L-lenses-17](#L-lenses-17)).

### Definitions used

Lens, put domain and report order are defined at the head of this page; the clauses are those of [L-lenses-4](#L-lenses-4). The graph form of a tree is its embedding with arcs as edges and fans reified.

### Proof

1. Tree put verifies the envelope, then runs the tree put-domain predicate, which evaluates the clauses of [L-lenses-4](#L-lenses-4) on the view's graph form under the view's embedding, and refuses with the first violation (definition of tree put).
2. A view node whose id is a key of `dropped_nodes` violates clause (1) on the graph form, which carries node ids unchanged, so tree put returns `IdCollision` for it (step 1).
3. A view-added or re-anchored node with several preimages violates clause (4) (step 1).
4. Tree get records a surviving node's source anchor exactly when the reverse vertex map does not return it (definition of tree get, the same recording rule as graph get, [L-lenses-2](#L-lenses-2) step 3); tree restoration applies the recorded anchor exactly when its image equals the view anchor, else the unique reverse anchor (definition of tree restoration).
5. On the tree lens's domain no edge between survivors is dropped, so the graph restriction of the tree's graph form keeps exactly the arcs whose endpoints survive, and its view is the graph form of the tree view (definition of get on both forms); the restoration rules coincide clause by clause (steps 1 to 4), so tree put and graph put agree under the embedding.

### Executable counterpart

- Law ID: `lenses.l5`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Reference: `panproto_reference::lens::tree_get`
- Reference: `panproto_reference::lens::tree_put`
- Enumerator: `panproto_reference::enumerate::lenses::c11_tree_states; bound: schemas=16, nonfan_stages=29, nodes=3, schema_tree_states=2020, stage_tree_states=2548`
- Exhaustive differential test: `tree_lens_agrees_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=tree_lens_agrees_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/asymmetric.rs`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-reference/src/pages/lenses/tree.rs`
- File: `crates/panproto-reference/src/pages/lenses/graph.rs`
- File: `crates/panproto-reference/src/pages/lenses/domain.rs`
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

## Complements of one lens merge as a partial commutative monoid, and composition is a path {#L-lenses-6}

### Statement

For the complements of one lens, `ComplementCompose` is a partial commutative monoid: the empty complement is a two-sided unit, and merge is commutative and associative where defined, with merged values compared under the value equivalence (equality with NaN reflexive) and the left operand's representative kept. Merge is defined exactly when the two identity envelopes combine and the two complements agree, under that equivalence, on every shared key of every keyed map (synthesized maps by id and by synthesis key); the id floor is merged by maximum. Merge is not cancellative and is not lens composition. Composing two lenses yields the lens path of the two stages ([L-lenses-15](#L-lenses-15)); its behaviour is the sequential fold, and fusion is applied only where it preserves that behaviour ([L-lenses-16](#L-lenses-16)).

### Outside the domain

Two complements that disagree on a shared key: merge refuses with `ComplementConflict` naming the map and the key. Two complements whose envelopes do not combine (complements of different lenses, or a complement carrying data with no envelope): merge refuses with `Identity`. Two lenses whose shared boundary differs: composition refuses with `CompositionMismatch`.

### Definitions used

Complement is defined at the head of this page. Agreement on a shared key means equal values under that key in every keyed map; for synthesized maps the id and the synthesis key must both agree.

### Proof

1. Merge first combines the two envelopes (refusing with `Identity` when they do not combine), then takes the keywise union of each keyed map, keeping the left operand's value on a shared key and refusing at the first shared key whose values are not equivalent, and sets the floor to the maximum of the two floors (definition of merge).
2. Unit: the empty complement has no keys, floor zero and no envelope, which combines with any envelope; so the union is the other complement and the maximum is its floor.
3. Commutativity: keywise union of maps whose shared values are equivalent is symmetric up to the representative kept on a shared key, which is equivalent to the other one; maximum is commutative; the refusal condition is symmetric because the equivalence is.
4. Associativity: where both bracketings are defined, each key of the union of three pairwise agreeing maps takes a value equivalent to every operand's value at that key (the equivalence is transitive), and maximum is associative; if one bracketing refuses at a key, the other meets an inequivalent pair of values at that key and refuses.
5. Not cancellative: merging a complement with floor 5 into floors 3 and 5 gives equal results from unequal arguments.
6. Composition builds the two-stage path; its get and put are the folds of [L-lenses-16](#L-lenses-16), whose proof gives the behaviour.

### Executable counterpart

- Law ID: `lenses.l6`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::c11_graph_states; bound: schemas=16, stages=31, nodes=3, edges=3, graph_states=116208, fan_states=9940, stage_states=144397`
- Exhaustive differential test: `lens_paths_agree_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=lens_paths_agree_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/asymmetric.rs`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-lens/src/table_lens.rs`
- File: `crates/panproto-lens/src/compose.rs`
- File: `crates/panproto-reference/src/pages/lenses/path.rs`
- File: `crates/panproto-reference/src/pages/lenses/graph.rs`
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

## Law equivalence is equality modulo synthesized presentation ids {#L-lenses-7}

### Statement

Every law that compares views compares them with the shape's equivalence under the synthesized index of the complement that produced each view (a user-edited view takes the index of the complement it is put against). Graph equivalence holds exactly when the node and edge synthesis-key sets of the two indices are equal and, after renaming each synthesized id to the id its key has on the other side, the two graphs are equal on every node (anchor, value, fields, discriminator, annotations, shape) and every edge (endpoints, label, position). Table equivalence is equality. Tree equivalence matches expansion nodes by key and is exact elsewhere, child order included. Complements are compared on every field put reads: the dropped and original maps, and for trees the arc order and arc labels; synthesized objects by their key sets; ids of synthesized objects, the floor and the identity are excluded.

### Outside the domain

The relation is total on pairs of instances with indices; unequal key sets are a definite inequivalence, never a refusal.

### Definitions used

Complement and synthesis key are defined at the head of this page and in [L-lenses-2](#L-lenses-2).

### Proof

1. Equivalence first compares the two indices' node key sets and edge key sets and returns inequivalent when either differs (definition).
2. With equal key sets, each key names exactly one id on each side (keys are unique within a coherent index, [L-lenses-2](#L-lenses-2) step 2), so the renaming is a bijection on synthesized ids and the identity elsewhere.
3. After renaming, the comparison is equality of the two graphs' node and edge maps, which compares every listed coordinate (definition).
4. Equivalence is therefore an equivalence relation: reflexive (equal key sets, identity renaming), symmetric (the inverse bijection), transitive (composition of bijections preserves key sets).
5. The complement comparison lists the fields put reads and omits the derived ones (definition), so two complements it accepts give equal put results on every view ([L-lenses-4](#L-lenses-4), restoration reads only the compared fields).

### Executable counterpart

- Law ID: `lenses.l7`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::c11_graph_states; bound: schemas=16, stages=31, nodes=3, edges=3, graph_states=116208, fan_states=9940, stage_states=144397`
- Exhaustive differential test: `graph_lens_agrees_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=graph_lens_agrees_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/lens_shape.rs`
- File: `crates/panproto-lens/src/table_shape.rs`
- File: `crates/panproto-reference/src/pages/lenses/laws.rs`
- File: `crates/panproto-reference/src/pages/lenses/graph.rs`
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

## The five law gates {#L-lenses-8}

### Statement

For a lens $\ell$, a source $s$ and its Single-Step Basis, the law report runs, in order, and returns the first violation: GetPut, $\operatorname{put}_\ell(\operatorname{get}_\ell(s)) = s$; PutGet, strict on every coordinate in every contraction mode: for every basis view $v'$ in the declared domain on which put succeeds, the view of $\operatorname{put}_\ell(v', c)$ is equivalent to $v'$ under [L-lenses-7](#L-lenses-7); Totality, every basis view in the declared domain puts without error and every negative basis view is refused with its designated reason (`SynthesizedEdited`, `ContractedEdgeInserted`, `IdCollision` or `MalformedFan`); PutTwice, for $s' = \operatorname{put}_\ell(v', c)$ and $(\_, c') = \operatorname{get}_\ell(s')$, $\operatorname{put}_\ell(v', \rho(c')) = s'$ with $\rho$ the coherent rekeying from the index of $\operatorname{get}_\ell(s')$ to that of $v'$; and Congruence, coherently renumbered view and complement pairs have equal domain verdicts and equivalent puts. Shapes skip what they cannot express. GetPut and WeakPutGet together do not imply PutTwice; GetPut at the once-put source together with exact PutGet does.

### Outside the domain

A get that refuses makes the report a refusal before any gate runs. An operation that refuses inside GetPut, PutGet or PutTwice is reported as that operation's refusal; inside Totality a refused admitted view is the Totality violation. A negative basis view returning a reason other than its designated one fails Totality.

### Definitions used

Single-Step Basis, law report and equivalence are defined at the head of this page and in [L-lenses-7](#L-lenses-7). A finite partial lens has two sources, two views and three complements, an injective total get and a partial put.

### Proof

1. The report runs the gates in the stated order and returns at the first failure (definition of the checker).
2. A put refused on an admitted view is skipped by PutGet and reported by Totality, which checks exactly the admitted views (definition of the two gates); so the first failing gate is Totality, not PutGet, for that case.
3. PutTwice not implied: on the partial lens with $\operatorname{get}(s_0) = (a, x)$, $\operatorname{get}(s_1) = (a, y)$, $\operatorname{put}(a, x) = s_0$, $\operatorname{put}(a, y) = s_1$, $\operatorname{put}(a, z) = s_0$, $\operatorname{put}(b, y) = s_1$, $\operatorname{put}(b, z) = s_0$ and $\operatorname{put}(b, x)$ undefined, GetPut holds (the first two equations); WeakPutGet holds because every defined put returns a source whose view is $a$ and $\operatorname{put}(a, \cdot)$ returns the same source on each complement where it was defined; PutTwice fails at $(b, z)$: $s' = s_0$, $\operatorname{get}(s_0) = (a, x)$ and $\operatorname{put}(b, x)$ is undefined.
4. Implication: let $s' = \operatorname{put}(v, c)$ be defined with $\operatorname{get}(s') = (v', c')$. Exact PutGet gives $v' = v$; GetPut at $s'$ gives $\operatorname{put}(v', c') = s'$; substituting, $\operatorname{put}(v, c') = s'$, which is PutTwice.
5. With synthesized carriers the rekeyed form follows in the same way: indexed PutGet gives equal key sets, so $\rho$ is a bijection on synthesized ids with $\operatorname{index}(v, \rho(c')) = \operatorname{index}(v)$, the states $(v, \rho(c'))$ and $(v', c')$ are coherently congruent, Congruence transports put across them, and GetPut at $s'$ identifies the result with $s'$.

### Executable counterpart

- Law ID: `lenses.l8`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_partial_lens; bound: sources=2, views=2, complements=3, partial_lenses=21870, lens_source_cases=43740`
- Exhaustive differential test: `lens_laws_agree_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=lens_laws_agree_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/laws.rs`
- File: `crates/panproto-lens/src/lens_shape.rs`
- File: `crates/panproto-lens/src/table_shape.rs`
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

## Graph get and put of a tree agree with the tree lens {#L-lenses-9}

### Statement

For a tree-shaped source $W$ and a lens that preserves tree shape and keeps the root, graph get of the graph form of $W$ is identical to the tree result mapped through the embedding, with synthesized ids allocated by synthesis key from the complement's floor, child order compared separately; put likewise. The documented differences are limited to a subtree drop (an edge dropped between surviving vertices) and a pruned root; contractions, including recursive dropped wrappers, commute.

### Outside the domain

A lens that drops an edge between surviving vertices: the tree route is outside its exact domain and the square reports the route as refused (no agreement is claimed). A pruned root: tree get refuses with `RootPruned` and dispatch falls back to the graph route.

### Definitions used

The graph form and embedding are those of [L-lenses-5](#L-lenses-5); the square is that of [L-lenses-17](#L-lenses-17).

### Proof

1. On a tree-shaped input the walk from a surviving node through dropped nodes to the next surviving node is unique, so the graph contraction stage and tree ancestor contraction choose the same pairs, labels and expansions (definition of the contraction plan on a tree).
2. With no subtree drop and the root kept, every surviving tree node keeps its parent arc or acquires the contracted arc of step 1, so the graph view is the graph form of the tree view ([L-lenses-5](#L-lenses-5) step 5).
3. Synthesized objects receive ids in ascending synthesis-key order from the floor on both routes (definition of allocation), so the mapped ids coincide.
4. The two complements correspond field for field under the conversion of [L-lenses-17](#L-lenses-17), so the two puts restore the same source ([L-lenses-5](#L-lenses-5) step 5).

### Executable counterpart

- Law ID: `lenses.l9`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::c11_tree_states; bound: schemas=16, nonfan_stages=29, nodes=3, schema_tree_states=2020, stage_tree_states=2548`
- Exhaustive differential test: `graph_square_agrees_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=graph_square_agrees_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/dispatch.rs`
- File: `crates/panproto-lens/src/asymmetric.rs`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-reference/src/pages/lenses/squares.rs`
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

## Graph and table counterparts of every tree construction {#L-lenses-10}

### Statement

Wherever the tree lens has a construction, the graph and table lenses have its counterpart over the same lens value: symmetric lenses are spans of two asymmetric lenses out of one middle schema, built only when the two middle schemas have equal canonical bytes; protolens application to an instance checks applicability first and returns the instantiated lens with its view and bound complement; and the graph isomorphism classifier reports no obstruction only for lenses whose get records no residue on any source, so that complement-free put inverts get on every source where get succeeds, and it reports an obstruction for a coordinate that some source records in its complement or on which put can refuse; complement-free put refuses exactly on a reported obstruction.

### Outside the domain

Two spans whose middle schemas differ: `NoSharedMiddle`. An inapplicable protolens: refused before instantiation. Complement-free put of a lens with an obstruction: `NotAnIsomorphism`.

### Definitions used

Lens, complement and identity envelope are defined at the head of this page; protolens application is that of [L-lenses-20](#L-lenses-20).

### Proof

1. A span is built from two asymmetric lenses whose sources are the middle schema, and construction compares the canonical bytes of the two middle schemas and refuses with `NoSharedMiddle` when they differ (definition).
2. Protolens application checks the precondition, instantiates, and runs get of the instantiated lens, which stamps its own envelope ([L-lenses-20](#L-lenses-20)).
3. The classifier reports no obstruction exactly when the plan compiles, every source vertex survives unconditionally with an injective image and no term assignment or non-invertible transform, every edge fate keeps the edge as one edge with a unique preimage, the source schema has no hyper-edge, and the schema declares no required field without a default and no key (definition of the classifier).
4. Under those conditions get on any source keeps every node and edge, records no anchor (the vertex map is injective, so the reverse map returns every source anchor), no label (every image has one preimage), no field or value (no transform changes one), and synthesizes nothing ([L-lenses-2](#L-lenses-2) steps 1 to 3); the complement's residue is empty, so the empty complement stamped with the envelope is the complement of every source and put on it returns the source ([L-lenses-8](#L-lenses-8), GetPut), with no domain clause able to fail (no keys, no required fields).
5. Conversely each reported obstruction names a coordinate some finite source records or put can refuse on: a non-surviving or conditional vertex drops a node, a shared image records an anchor, a term assignment or transform records fields, an edge not kept as one edge drops it, a shared preimage records a label, a hyper-edge admits a parent-only fan that restriction drops, and a required field or key admits a source that put refuses. Complement-free put refuses with `NotAnIsomorphism` on every reported obstruction ([L-lenses-3](#L-lenses-3) step 5).

### Executable counterpart

- Law ID: `lenses.l10`
- Reference: `panproto_reference::lens::GraphLens::check_put_domain`
- Reference: `panproto_reference::lens::GraphLens::put`
- Reference: `panproto_reference::lens::get`
- Reference: `panproto_reference::lens::put`
- Enumerator: `panproto_reference::enumerate::lenses::c11_graph_states; bound: schemas=16, stages=31, nodes=3, edges=3, graph_states=116208, fan_states=9940, stage_states=144397`
- Exhaustive differential test: `graph_lens_agrees_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=graph_lens_agrees_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/graph_symmetric.rs`
- File: `crates/panproto-lens/src/table_symmetric.rs`
- File: `crates/panproto-lens/src/graph_protolens.rs`
- File: `crates/panproto-lens/src/table_protolens.rs`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-reference/src/pages/lenses/graph.rs`
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

## The table complement records exactly what table get forgets {#L-lenses-11}

### Statement

For a table lens whose plan compiles and a source table on which table get succeeds, the complement holds, in this field order: `dropped_rows`, the rows of dropped tables and the rows failing selection, each with its source table and cells; `dropped_cells`, each source column of a surviving row of which the view row holds no cell of that name, renamed columns included, with its source value; `dropped_fk_pairs`, per source edge, the pairs absent from the view; `original_fk_edges`, a view pair recorded exactly when the preimage rule of [L-lenses-4](#L-lenses-4) would not return its source edge, with more than one element exactly when the edge map merged pairs; `original_tables`, a row recorded exactly when the unique reverse vertex map would not return its source table; `original_cells`, each source column of a surviving row whose view cell of that name holds another value, with the source value; `synthesized_fk_pairs`, the contracted pairs; the row id floor; and the identity envelope. The residue is the dropped and original maps; the synthesized pairs, floor and identity are not residue; the reserved row ids are the keys of `dropped_rows`.

### Outside the domain

A table plan that does not compile, or a migration that expands, retargets a fan or disagrees with the table functor: table get refuses (`NotTableRepresentable`, `SurvivalRulesDisagree`, or the plan error) and returns no complement.

### Definitions used

Complement, identity envelope and the preimage rule are defined at the head of this page and in [L-lenses-4](#L-lenses-4). A table plan is the restrict plan compiled together with the table functor of the two relational boundaries.

### Proof

1. Table get decides each row's survival by its table's survival under the table plan and its survival predicate; a row that does not survive goes to `dropped_rows` with its table and cells, and no other row does (definition of table get).
2. For a surviving row, each source column is carried by its cell fate: kept under its name, renamed, or dropped. A dropped or renamed column leaves no cell of its source name in the view row, so get records it in `dropped_cells`; a kept column whose view value differs (by a transform) is recorded in `original_cells` (definition of the cell stage).
3. A pair whose endpoints do not both survive, or whose edge fate is drop, is recorded in `dropped_fk_pairs`; a kept pair is recorded in `original_fk_edges` exactly when its view label does not map back to its source edge; a merged pair records each source edge (definition of the pair stage).
4. A surviving row's table is recorded in `original_tables` exactly when the reverse vertex map does not return it (definition of the row stage).
5. Contracted pairs are synthesized into the view and listed in `synthesized_fk_pairs`; the floor is one above the largest source row id; get stamps the envelope (definition). Steps 1 to 5 fill exactly the listed fields with exactly the listed contents.

### Executable counterpart

- Law ID: `lenses.t3`
- Reference: `panproto_reference::table::TableComplement`
- Reference: `panproto_reference::table::get`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_table_fixture; bound: rows=2, columns=2, cell_values=7, lenses=7, cases=9921`
- Exhaustive differential test: `table_get_and_put_agree_with_the_reference_on_the_fixture_scope; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=table_get_and_put_agree_with_the_reference_on_the_fixture_scope)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-inst/src/table_complement.rs`
- File: `crates/panproto-lens/src/table_lens.rs`
- File: `crates/panproto-lens/src/asymmetric.rs`
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

## Table get and put, with put writing through the last writer {#L-lenses-12}

### Statement

Table get is the table restriction along the table plan compiled for the two boundary protocols, stamped with the table identity envelope; table put verifies the envelope, evaluates the put domain of [L-lenses-4](#L-lenses-4) with rows and pairs for nodes and edges, and on the domain restores. Restoration of a row takes its recorded table only when that table's image is the row's view table, otherwise the unique reverse table; a view pair that is also a synthesized pair is read only; a dropped column of a view-inserted row stays absent. Cells follow the baseline-and-pullback rule: complement residue with the view's same-named cells reconstructs a baseline source, equal to the original source when the view is unedited, and each changed view cell is written back through the table plan's last writer of its column, so an edit to a renamed column is written to the source column.

### Outside the domain

A view outside the put domain: `PutDomain` with the first violation, among them `IdCollision` for a reserved row id, `SynthesizedEdited` for a removed two-origin pair, `AmbiguousReverseAnchor` for a moved row with several preimage tables, `DerivedCellEdited` and `TransformNotInvertible` for edited derived cells. A complement from another lens or shape: `Identity`.

### Definitions used

The table complement is that of [L-lenses-11](#L-lenses-11). The last writer of a target column $t$ of table $T$ is the last source column, in cell-fate order, whose fate keeps or renames it to $t$.

### Proof

1. Table get compiles the table plan and runs the table restriction, then stamps the envelope (definition), so its view and complement are those of [L-lenses-11](#L-lenses-11).
2. Put verifies the envelope and evaluates the domain clauses on the reconstructed baseline before restoring (definition of table put), as in [L-lenses-3](#L-lenses-3) steps 2 to 4.
3. Baseline: for each surviving row, the source cells are the view cells of the same name, overridden by `original_cells` and supplemented by `dropped_cells` (definition). On an unedited view the view cells are the images of the source cells, so the baseline equals the source.
4. A changed view cell at column $t$ is assigned to the last writer of $t$, and the plan's inverse cell fate maps it back to that source column (definition of writeback), so a renamed column's edit reaches its source column, and when two source columns map to $t$ the later one receives it.
5. The row and pair restoration rules are the clauses of [L-lenses-4](#L-lenses-4) read on rows and pairs, which gives the stated table and two-origin behaviour.

### Executable counterpart

- Law ID: `lenses.t4`
- Reference: `panproto_reference::table::TablePlan::writer`
- Reference: `panproto_reference::table::compile_for_tables`
- Reference: `panproto_reference::table::get`
- Reference: `panproto_reference::table::TableLens::put`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_table_fixture; bound: rows=2, columns=2, cell_values=7, lenses=7, cases=9921`
- Exhaustive differential test: `table_get_and_put_agree_with_the_reference_on_the_fixture_scope; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=table_get_and_put_agree_with_the_reference_on_the_fixture_scope)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/table_lens.rs`
- File: `crates/panproto-lens/src/table_shape.rs`
- File: `crates/panproto-inst/src/table_complement.rs`
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

## The identity path is a left unit {#L-lenses-13}

### Statement

For every lens path $p$ from $S$, $\operatorname{id}_S \mathbin{;} p = p$: the composite has the atomic stages of $p$, the identity envelopes of $p$, and the get and put of $p$ on every source and view.

### Outside the domain

A path whose endpoint differs from the identity's schema: composition refuses with `CompositionMismatch`, and no composite exists.

### Definitions used

A lens path is the free-category word over atomic lens stages; the distinguished identity at a schema is the empty word at that schema, with its one identity lens marked as the identity. Composition `then` is boundary-checked concatenation that never fuses.

### Proof

1. Composition checks that the target endpoint of the left path equals the source endpoint of the right path, refusing otherwise (definition of `then`).
2. When the left path is the distinguished identity at $S$ and the endpoints agree, `then` returns the right path unchanged (definition), so the stage sequence and envelopes are those of $p$.
3. Get and put of a path are folds over its atomic stages (definition of the folds), so equal stage sequences give equal get and put.

### Executable counterpart

- Law ID: `lenses.path-left-identity`
- Reference: `panproto_reference::path::get_path`
- Reference: `panproto_reference::path::put_path`
- Enumerator: `panproto_reference::enumerate::lenses::c11_stages; bound: stages=31, words_by_length=31/961/29791, words=30783, composable_by_length=31/93/413, composable_words=537, stage_states=144397, word_states=618603`
- Exhaustive differential test: `lens_paths_agree_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=lens_paths_agree_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
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

A path whose endpoint differs from the identity's schema: composition refuses with `CompositionMismatch`, and no composite exists.

### Definitions used

A lens path is the free-category word over atomic lens stages; the distinguished identity at a schema is the empty word at that schema, with its one identity lens marked as the identity. Composition `then` is boundary-checked concatenation that never fuses.

### Proof

1. Composition checks that the target endpoint of the left path equals the source endpoint of the right path, refusing otherwise (definition of `then`).
2. When the right path is the distinguished identity at $T$ and the endpoints agree, `then` returns the left path unchanged (definition), so the stage sequence and envelopes are those of $p$.
3. Get and put of a path are folds over its atomic stages (definition), so equal stage sequences give equal get and put.

### Executable counterpart

- Law ID: `lenses.path-right-identity`
- Reference: `panproto_reference::path::get_path`
- Reference: `panproto_reference::path::put_path`
- Enumerator: `panproto_reference::enumerate::lenses::c11_stages; bound: stages=31, words_by_length=31/961/29791, words=30783, composable_by_length=31/93/413, composable_words=537, stage_states=144397, word_states=618603`
- Exhaustive differential test: `lens_paths_agree_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=lens_paths_agree_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
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

For lens paths $p$, $q$, $r$ with matching boundaries, $(p \mathbin{;} q) \mathbin{;} r = p \mathbin{;} (q \mathbin{;} r)$ as atomic stage sequences (each stage compared by its lens digest, both schemas included) and as identity-envelope sequences; when a boundary does not match, both bracketings refuse.

### Outside the domain

A mismatched boundary between $p$ and $q$ or between $q$ and $r$: each bracketing refuses with `CompositionMismatch` at the mismatched boundary.

### Definitions used

Lens path, the distinguished identity and composition are defined in [L-lenses-13](#L-lenses-13).

### Proof

1. When none of $p$, $q$, $r$ is the distinguished identity, both bracketings are the concatenation of three stage sequences after two boundary checks, and concatenation is associative.
2. When one of them is the distinguished identity, [L-lenses-13](#L-lenses-13) and [L-lenses-14](#L-lenses-14) reduce both bracketings to the composite of the other two, which is the same.
3. Boundary checks: each bracketing compares the endpoints of $p$ and $q$ and of $q$ and $r$ (the endpoint of a composite is that of its outer stage), so one bracketing refuses exactly when the other does.

### Executable counterpart

- Law ID: `lenses.path-associativity`
- Reference: `panproto_reference::path::LensPath::then`
- Enumerator: `panproto_reference::enumerate::lenses::c11_stages; bound: stages=31, words_by_length=31/961/29791, words=30783, composable_by_length=31/93/413, composable_words=537, stage_states=144397, word_states=618603`
- Exhaustive differential test: `lens_path_category_laws_agree_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=lens_path_category_laws_agree_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
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

For every lens path and every source, get and put through the path's normal form in a shape equal the left-to-right get fold and the right-to-left put fold over its atomic stages: views agree exactly, puts agree exactly, and a refusal occurs on one side exactly when it occurs on the other, with an equal reason located at a group containing the refusing atomic stage. Two stages fuse exactly when both are contraction free, neither retargets a fan, their migrations compose, the plans of both and of the composite compile with keep or drop fates only, the composite gives every direct edge the fate the pair gives it, no plan expands, and at most one stage has a value layer, a layer on the first stage requiring a second stage that removes nothing and a layer on the second requiring a first stage that is the identity on instances; the table fold additionally requires the three table plans to compile under their boundary protocols.

### Outside the domain

A path whose complement has the wrong number of entries: `PathComplementMismatch`. A refusing stage: `AtStage` with the group and its atomic stages. A table path with the wrong number of protocols: `PathContextMismatch`.

### Definitions used

Lens path and composition are defined in [L-lenses-13](#L-lenses-13). The normal form is the result of repeatedly fusing the leftmost admitted adjacent pair of groups.

### Proof

1. For one admitted pair $(\ell_1, \ell_2)$ with fused lens $\ell$: no stage contracts, expands or retargets a fan, and every edge fate keeps or drops, so each of the three gets maps every node along the vertex map, keeps or drops every edge by its fate, and applies at most one value layer; the composite migration maps every vertex and kept edge to the image the pair gives it (migrations compose, direct images agree), so for every source the view of $\ell$ equals the view of $\ell_2$ after $\ell_1$, and an element dropped by either stage is dropped by $\ell$.
2. Value layers: with at most one layer, the layer runs on the same surviving nodes in both executions (it is on $\ell_1$ only when $\ell_2$ removes nothing, on $\ell_2$ only when $\ell_1$ is the identity on instances), so the transformed fields and survival verdicts agree.
3. No stage synthesizes an object (no contraction or expansion), so puts restore from the residue alone, and the fused residue is the union of the two stage residues keyed by the same ids; restoring through $\ell$ equals restoring through $\ell_2$ then $\ell_1$, and a domain violation occurs in one exactly when it occurs in the other.
4. A plan that refuses refuses every source; condition 7 excludes such stages, so no refusal is hidden by fusion.
5. By induction on the number of fusions in the leftmost strategy, each fusion replaces an adjacent pair by a lens with the same behaviour (steps 1 to 4), so the normal form's fold equals the atomic fold.
6. The bounded predicate census also checks every refusal branch. In particular, `DropF;ReadF` reaches condition 2: composing the migrations returns `ComposeUnboundField` because the second stage reads `f` after the first drops it, so normalization retains the two atomic stages. The separate `Op` stage exercises op-term execution rather than treating that route as another field transform.

### Executable counterpart

- Law ID: `lenses.fusion-soundness`
- Reference: `panproto_reference::path::get_path`
- Reference: `panproto_reference::path::put_path`
- Enumerator: `panproto_reference::enumerate::lenses::c11_stages; bound: stages=31, words_by_length=31/961/29791, words=30783, composable_by_length=31/93/413, composable_words=537, stage_states=144397, word_states=618603`
- Exhaustive differential test: `lens_paths_agree_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=lens_paths_agree_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
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

## The graph and tree routes of a lens commute {#L-lenses-17}

### Statement

For a tree $W$ and a lens, the graph route (graph get and put of the graph form of $W$) and the tree route (tree get and put of $W$, mapped back through the recorded embedding) give identical views, complements under the field-for-field conversion, put results and errors, with child order compared separately, wherever both routes are in their exact common domain. The round trip is exact on ids: the tree read back from the graph form of $W$ is $W$, and the graph form of a tree read from a tree-shaped graph $G$ is $G$ mapped through the embedding.

### Outside the domain

A route outside the exact common domain (a subtree drop, a pruned root, or a fan the tree route cannot carry) is reported as refused rather than compared; on such inputs dispatch runs the graph route ([L-lenses-9](#L-lenses-9)).

### Definitions used

The graph form, the embedding and the conversion of complements are those of [L-lenses-5](#L-lenses-5) and [L-lenses-9](#L-lenses-9).

### Proof

1. The graph form of a tree assigns each node its id and each arc and fan edge an id recorded in the embedding; reading the graph back as a tree inverts this assignment (definition of the encoding), which gives the exact round trip.
2. In the common domain, [L-lenses-9](#L-lenses-9) gives equal views and corresponding complements on the two routes, and [L-lenses-5](#L-lenses-5) step 5 gives equal puts.
3. An error on the fast route makes dispatch rerun the graph route, so the error a caller sees is the graph route's (definition of dispatch), and the error square holds by construction.

### Executable counterpart

- Law ID: `lenses.fb-graph-square`
- Reference: `panproto_reference::lens::graph_square`
- Enumerator: `panproto_reference::enumerate::lenses::c11_tree_states; bound: schemas=16, nonfan_stages=29, nodes=3, schema_tree_states=2020, stage_tree_states=2548`
- Exhaustive differential test: `graph_square_agrees_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=graph_square_agrees_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/dispatch.rs`
- File: `crates/panproto-reference/src/pages/lenses/squares.rs`
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

## The graph and table routes of a lens commute {#L-lenses-18}

### Statement

For a table instance and a lens whose migration preserves table shape (no expansion, no fan retargeting, no non-injective edge remap between the same target endpoints, and no realized contraction pair coinciding with a direct pair), the graph route (graph get and put of the graph form of the table) and the table route (table get and put, mapped back through the table embedding) give identical views, complements under the field-for-field conversion, and put results, wherever both routes are in their exact common domain.

### Outside the domain

A migration that does not preserve table shape: the table route declines and the square reports it as refused; dispatch runs the graph route. A table route error: the graph route's result is returned.

### Definitions used

The graph form of a table maps each row to the node with its id and each pair to an edge with the id the embedding assigns; the complement conversion maps dropped rows to dropped nodes, dropped pairs to dropped edges, recorded tables to recorded anchors, cells to fields, and synthesized pairs to synthesized edges.

### Proof

1. The graph form maps rows to nodes with their ids and cells to fields, and pairs to edges in ascending order, recorded in the embedding (definition), so the table round trip is exact.
2. Under table-shape preservation the graph restriction keeps a node exactly when the table restriction keeps its row, keeps an edge exactly when it keeps the pair, and the cell fates follow the attribute arrows (definition of both engines), so the views correspond under the embedding.
3. The complement fields correspond one for one under the conversion (steps 1 and 2 with [L-lenses-11](#L-lenses-11)), and both puts restore by the same clauses ([L-lenses-12](#L-lenses-12) step 5), so the puts correspond.

### Executable counterpart

- Law ID: `lenses.fb-table-square`
- Reference: `panproto_reference::lens::table_square`
- Enumerator: `panproto_reference::enumerate::lenses::for_each_table_fixture; bound: rows=2, columns=2, cell_values=7, lenses=7, cases=9921`
- Exhaustive differential test: `table_square_agrees_with_the_reference_on_the_fixture_scope; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=table_square_agrees_with_the_reference_on_the_fixture_scope)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
- File: `crates/panproto-lens/src/dispatch.rs`
- File: `crates/panproto-reference/src/pages/lenses/squares.rs`
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

For a protolens and a schema $S$, schema instantiation first checks the protolens's precondition against $S$ and refuses before any compilation when it fails; when it holds, the result is a lens whose source schema is exactly $S$, whose target schema is the protolens's target transform of $S$, and whose migration is the migration between them, and its plan either compiles or refuses with the plan error.

### Outside the domain

A schema that fails the precondition: refused before instantiation (`FalsePrecondition` in the reference, the stringly inapplicability refusal in production). The differential compares this inapplicability category, not exact error-variant parity. A plan that does not compile: the plan error.

### Definitions used

A protolens is a schema-indexed family of lenses given by a precondition and a construction of the target schema and migration from the source schema.

### Proof

1. Instantiation evaluates the precondition on $S$ and returns the refusal when it does not hold, without reading the target or the migration (definition).
2. Otherwise it computes the target schema from $S$, the migration between $S$ and the target, and binds the three as a lens whose source is $S$ (definition).
3. Compiling the plan of that lens is the plan compilation of [L-lenses-3](#L-lenses-3) step 1, which either succeeds or refuses.
4. The reference names a failed precondition with `FalsePrecondition`, while production currently carries the same category in `LensError::ProtolensError(String)`; thus the comparison establishes when instantiation is inapplicable but does not establish equality of those representations.

### Executable counterpart

- Law ID: `lenses.protolens-schema-transform`
- Reference: `panproto_reference::lens::protolens_schema`
- Enumerator: `panproto_reference::enumerate::lenses::c11_graph_states; bound: schemas=16, protolenses=4, nodes=3, edges=3, graph_states=116208`
- Exhaustive differential test: `protolens_transforms_agree_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=protolens_transforms_agree_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
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

## Applying a protolens to an instance is get of its instantiation {#L-lenses-20}

### Statement

For a protolens, a schema $S$ and an instance $s$ of $S$, instance application checks the precondition, instantiates the lens at $S$ ([L-lenses-19](#L-lenses-19)), and returns that lens together with $\operatorname{get}$ of it on $s$: the view and the complement bound to the instantiated lens's identity envelope, so a later put cannot detach the residue from the lens that produced it.

### Outside the domain

A schema that fails the precondition: refused before instantiation, compared only as the inapplicability category described in [L-lenses-19](#L-lenses-19). A plan that does not compile or a get that refuses: that refusal.

### Definitions used

Protolens and instantiation are those of [L-lenses-19](#L-lenses-19); get and the identity envelope are defined at the head of this page.

### Proof

1. Application performs instantiation and returns its refusal unchanged (definition; [L-lenses-19](#L-lenses-19) step 1).
2. On success it runs get of the instantiated lens on $s$, which stamps the complement with that lens's envelope ([L-lenses-3](#L-lenses-3) step 1), and returns the lens, view and complement (definition).

### Executable counterpart

- Law ID: `lenses.protolens-instance-transform`
- Reference: `panproto_reference::lens::protolens_instance`
- Enumerator: `panproto_reference::enumerate::lenses::c11_graph_states; bound: schemas=16, protolenses=4, nodes=3, edges=3, graph_states=116208`
- Exhaustive differential test: `protolens_transforms_agree_with_the_reference_exhaustively; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=protolens_transforms_agree_with_the_reference_exhaustively)'`
- Property differential test: `randomized_lens_law_differential; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=randomized_lens_law_differential)'`
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
