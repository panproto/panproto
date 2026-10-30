# Representation

The representation boundary relates the tree, graph, and table carriers without identifying shapes that carry different information. Eight laws fix that boundary. [L-representation-1](#L-representation-1) gives the graph form of a tree; [L-representation-2](#L-representation-2) decides when a graph has a tree form; [L-representation-3](#L-representation-3) records their common image. [L-representation-4](#L-representation-4) gives canonical bytes and their partial inverses, and [L-representation-5](#L-representation-5) fixes the complement identity envelope. The final three laws do the corresponding work for tables and reified hyper-edges.

A graph has nodes and edges indexed by `u32`. Fresh-id allocation reserves `u32::MAX` as its exhausted-id sentinel, though an input may already hold that id. A tree has nodes, ordered arcs, a root, and an ordered list of fans. A fan names one parent and a finite map from child labels to nodes. A table has rows indexed by `u32` and a set of labelled relation pairs. Maps and sets below use their ascending stored order.

The final bounded production/reference suite passed all eight law consumers in nextest run `d8a5dcc0-c2fa-475a-b164-3ae14bd738b8`. The 42 affected reference tests passed in run `0e864e92-d9cc-4729-91d1-8179545c5db9`, and strict Clippy passed for the production differential and the affected reference targets. These results include the mixed refusal-precedence and schema-relative table witnesses. The audit records retain the separate requirement for an independent verifier.

## Every admissible tree has the specified graph form or one typed refusal {#L-representation-1}

### Statement

For every tree $T$ and optional schema $S$, `tree_to_graph(T, S)` is determined as follows. It first copies every tree node. Arc $i$, in arc order from zero, becomes unpositioned edge $i$ with the same endpoints and label. Each fan then becomes a fresh hyper-edge-shaped node, beginning at one more than the greatest tree-node id, and receives a parent edge followed by child edges in ascending child-label order. These edges follow all arc edges. The fan node is anchored at the fan's hyper-edge name; each fan edge is labelled from that name to its target node's anchor. A declared hyper-edge supplies the parent edge's name.

The returned embedding maps each arc child to its edge id, records each fan node and its parent and child edge ids, and gives the saturated node and edge allocation floors. No allocated id is `u32::MAX`.

### Outside the domain

The function returns the first refusal encountered in construction order. It first returns `NodeIdMismatch` at the least node-map key whose node stores another id, as production does before any other check; the smallest witness files node 0 under key 1. It then returns `ReservedVocabulary` when a tree node already has the hyper-edge shape or an arc has a reserved fan kind. This preflight precedes every arc check: a hyper-edge-shaped node together with an arc to an absent endpoint therefore returns `ReservedVocabulary`, not the endpoint error. For each arc in stored order, it then checks endpoints, a second parent, and edge-id exhaustion immediately before allocating and inserting that arc. The reference calls the endpoint refusal `DanglingArc`; production reports `DanglingEdge` with the candidate edge id and absent endpoint. The node floor saturates at `u32::MAX`, so a fan-free tree that already holds that id still embeds. For each fan in stored order, allocating its node at a saturated floor may return `NodeIdsExhausted`; its parent is then checked for `DanglingFan` before the parent edge can return `EdgeIdsExhausted`; and each child in label order is checked for `DanglingFan` before its edge can return `EdgeIdsExhausted`. Production reports a dangling fan endpoint as `DanglingEdge`, naming the edge id the fan edge receives and the absent endpoint. Production draws that edge id before it checks the endpoint, so the two orders differ only when a fan endpoint dangles at an exhausted edge floor, which a tree reaches only after $2^{32}-1$ edges. The empty tree maps to the empty graph and the default embedding.

### Definitions used

The reserved fan kinds are `hyper_parent` and `hyper_label`. The *node floor* is zero for an empty node map and otherwise the saturating successor of its greatest id. The *edge floor* is the first counter value after all emitted edges. A parent edge has kind `hyper_parent`; a child edge has kind `hyper_label` and is named by its child label.

### Proof

1. The implementation checks the key/id invariant at each key in ascending order, then rejects reserved tree vocabulary, before allocating anything. This gives the first two refusal families.
2. It walks arcs in stored order with an edge counter initially zero. Endpoint membership is checked before insertion, and the embedding's child map detects a second parent. Thus the successful arc prefix has exactly the stated edges and arc record.
3. The node counter is the saturated node floor. Drawing an id increments the counter with `checked_add`, so the last allocatable value is `u32::MAX - 1`; a tree already containing `u32::MAX` requires no draw unless it carries a fan.
4. Fans are traversed in tree order. Their children come from an ordered map, so parent-before-children and ascending child-label order follow directly. Endpoint lookup both supplies each target anchor and produces `DanglingFan` at the first absent endpoint.
5. The same checked counter supplies every edge id. The completed counters are therefore the allocation floors—saturated when no storable successor exists—and the accumulated maps are exactly the embedding stated above.

### Executable counterpart

- Law ID: `representation.tree-to-graph-total`
- Reference: `panproto_reference::encode::tree_to_graph`
- Evidence scope: `panproto_reference::enumerate::representation::trees; exact carrier: 2 two-node trees, fans=0; explicit cases: 1 valid fan, 2 saturated-floor cases, 1 node-id mismatch, 1 reserved-vocabulary refusal, 1 mixed reserved-plus-dangling precedence witness, 1 second-parent refusal, 1 dangling arc and 2 dangling fan endpoints (parent and child)`
- Production certificate: `tree_to_graph_matches_production_on_the_exact_carrier_and_refusals; command: cargo nextest run -p panproto-inst --test reference_representation --release --no-tests=fail -E 'test(=tree_to_graph_matches_production_on_the_exact_carrier_and_refusals)'`
- Independent reference anchor: `every_valid_tree_embeds_and_round_trips; command: cargo nextest run -p panproto-inst --test a_tree_embeds_in_its_graph_form --release --no-tests=fail -E 'test(=every_valid_tree_embeds_and_round_trips)'`
- File: `crates/panproto-inst/src/ginstance/embed.rs`
- File: `crates/panproto-inst/src/ginstance/embedding.rs`
- File: `crates/panproto-reference/src/pages/representation.rs`
- File: `crates/panproto-reference/src/enumerate/representation.rs`

### Mutation equivalences

None. The bounded differential compares every observable field, and every typed refusal named by this law except `EdgeIdsExhausted`. That refusal needs $2^{32}-1$ edges; the unit test `representation_counters_allocate_max_minus_one_but_never_max` pins the counter that produces it, and the saturated-fan case pins `NodeIdsExhausted`.

### Audit record

- Author: representation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## A graph has a tree form exactly under the stated decision procedure {#L-representation-2}

### Statement

Fix a graph $G$, a proposed root $r$, and a parent-name policy $p$ for each hyper-edge anchor. A policy result is either unconstrained or an exact optional name. With no schema, the result is unconstrained: a schema-free fan parent may be named or unnamed. With a schema, the result is exact: a declared hyper-edge requires its declared parent label, while an undeclared hyper-edge requires no name. `graph_to_tree(G, r, p)` succeeds exactly when all of the following hold: $r$ exists and is not a fan node; every edge has existing endpoints and no position; every edge out of a fan node has the source and target anchors written by [L-representation-1](#L-representation-1), with a parent name admitted by $p$; every other edge has no reserved kind, does not enter a fan or the root, and is the sole ordinary edge into its target; every fan node is data-free and has no fan-domain violation without a schema; and every ordinary node reaches $r$ by repeatedly following its unique parent.

On success, ordinary edges become arcs in ascending edge-id order, fan nodes become fans in ascending node-id order, and the ordinary nodes and root are unchanged. Let $T$ be a tree with `tree_to_graph(T, S) = G`. Then `graph_to_tree(G, root(T), p_S)`, with $p_S$ the parent-name function induced by $S$, succeeds exactly when $T$ is *rooted at its stored root*, and on success it returns $T$. The fresh numeric ids of fan nodes and edges are discarded. The encoding does not check rootedness: the tree with nodes 0 and 1, arcs $0 \to 1$ and $1 \to 0$, and root 0 encodes, and its graph form is refused with `RootHasParent { edge: 1 }`; a tree whose stored root is not a node encodes and is refused with `MissingRoot`.

### Outside the domain

The reference returns the first local `NotATree` witness in this order: `MissingRoot`; then, by ascending edge id, `DanglingEdge`, `PositionedEdge`, `FanLabel`, `ReservedArc`, `ArcIntoFan`, `RootHasParent`, or `SecondParent`; then, by ascending fan-node id, `FanData` or the first `Fan` violation; then, by ascending ordinary-node id, `Unreached`. Production preserves that success domain and edge-local order but aggregates some global failures. In particular, a disconnected two-node graph is reference `Unreached { node: 1 }` and production `NotATree { reason: NoRoot }`. Thus the law identifies the decision and successful output, not literal equality between the two refusal enums. Explicit witnesses pin each side's deterministic reason.

### Definitions used

A tree is *rooted at its stored root* when the root is a node, no arc enters it, and every node reaches it along arcs. A *data-free fan node* has no value, discriminator, fields, position, or annotations. Its fan-domain conditions are those of [L-representation-8](#L-representation-8), evaluated without schema-relative label checks. A node *reaches the root* when finitely many parent-map steps end at $r$; the implementation bounds the walk by the number of ordinary nodes, so a cycle is refused as `Unreached`.

### Proof

1. The root check is performed first and is exactly the first condition.
2. One ascending pass over edges checks endpoint existence, positions, fan-label shape, reserved ordinary kinds, arcs into fans or the root, and duplicate parents, returning immediately. Its accumulated parent map exists exactly when every edge condition holds.
3. The next ascending pass over fan nodes checks that each is data-free, then takes the first result of `violations_at` without a schema. It constructs one fan from the unique parent edge and the ordered child-label map.
4. The remaining nodes are the ordinary nodes. Following the parent map for at most their count accepts exactly the nodes whose chains reach $r$ and refuses a missing link or cycle at the least such node.
5. Filtering the ascending edge map yields arcs in edge-id order. The preceding passes retain the nodes and root and construct fans in node-id order, which proves the output clause.
6. Let $G$ be emitted by `tree_to_graph(T, S)`. Its edges are unpositioned with present endpoints, its fan edges carry the labels and the $p_S$ parent names checked in step 2, no ordinary edge has a reserved kind or enters a fan node (arcs join tree nodes, which the preflight keeps out of the hyper-edge shape), no node has two arcs, and every fan node is data-free and well formed. So the only checks $G$ can fail are `MissingRoot`, `RootHasParent` and `Unreached`, and these fail exactly when $T$ is not rooted at its stored root. When $T$ is rooted, reading the arcs in edge-id order restores the arc list, the fans in fan-node-id order restore the fan list, and the ordinary nodes and root are those of $T$.

### Executable counterpart

- Law ID: `representation.graph-to-tree-decision`
- Reference: `panproto_reference::encode::graph_to_tree`
- Evidence scope: `panproto_reference::enumerate::representation::graphs; exact carrier: 8 graphs, each checked at 3 proposed roots, plus 3 typed-refusal witnesses, 1 named schema-free fan-parent witness, and the cyclic and rootless encoded trees outside the left-inverse domain`
- Production certificate: `graph_to_tree_decision_matches_production_on_the_exact_carrier; command: cargo nextest run -p panproto-inst --test reference_representation --release --no-tests=fail -E 'test(=graph_to_tree_decision_matches_production_on_the_exact_carrier)'`
- Independent reference anchor: `a_tree_shaped_graph_round_trips_through_its_tree; command: cargo nextest run -p panproto-inst --test a_graph_is_a_tree_when_its_shape_says_so --release --no-tests=fail -E 'test(=a_tree_shaped_graph_round_trips_through_its_tree)'`
- File: `crates/panproto-inst/src/ginstance/tree_shape.rs`
- File: `crates/panproto-reference/src/pages/representation.rs`
- File: `crates/panproto-reference/src/enumerate/representation.rs`

### Mutation equivalences

The bounded differential compares success or failure and every field of successful trees. Refusal witnesses are compared against each implementation's declared deterministic taxonomy; `Unreached` and aggregate `NoRoot` are not asserted equal.

### Audit record

- Author: representation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## Reification is exactly the common tree-graph image round trip {#L-representation-3}

### Statement

For every tree $T$ and optional schema $S$, `reify(T, S)` is defined exactly when `tree_to_graph(T, S)` succeeds and `graph_to_tree` succeeds on that graph at $T$'s root with the parent names induced by $S$. When defined, it returns the latter tree. In particular, every admissible tree in the image of the encoding returns unchanged.

### Outside the domain

`reify` returns `None` for every refusal of either direction. This includes a node filed under another id, reserved fan vocabulary already present as ordinary tree material, malformed arcs or fans, exhausted ids, and an encoded graph that fails the tree-form checks, such as a cyclic tree or one whose stored root is not a node.

### Definitions used

The schema-induced parent-name function returns a declared hyper-edge's `parent_label` and returns no name for an undeclared hyper-edge. The *common image* is the set of trees accepted by the forward encoding whose graph form is accepted by the reverse decision under that same function.

### Proof

1. The function calls `tree_to_graph` and converts any error to `None`.
2. It builds the schema-induced parent-name function used by the forward encoding.
3. It calls `graph_to_tree` at the original root and converts any error to `None`, proving the domain statement by the semantics of `?` on `Option`.
4. On a tree in the encoding image, [L-representation-2](#L-representation-2) restores its ordinary nodes, arc order, fans, and root. Hence the returned tree is $T$.

### Executable counterpart

- Law ID: `representation.reify-round-trip`
- Reference: `panproto_reference::encode::reify`
- Evidence scope: `panproto_reference::enumerate::representation::trees; exact carrier: 2 two-node trees, plus 1 explicit fan image and 9 refusals: node-id mismatch, reserved vocabulary, dangling arc, dangling fan parent, dangling fan child, second parent, node-id exhaustion, a cycle and an absent root`
- Production certificate: `reification_round_trip_matches_production_on_the_exact_image; command: cargo nextest run -p panproto-inst --test reference_representation --release --no-tests=fail -E 'test(=reification_round_trip_matches_production_on_the_exact_image)'`
- Independent reference anchor: `unreify_inverts_reify; command: cargo nextest run -p panproto-inst --test fans_reify_exactly_on_the_fan_domain --release --no-tests=fail -E 'test(=unreify_inverts_reify)'`
- File: `crates/panproto-inst/src/ginstance/reify.rs`
- File: `crates/panproto-reference/src/pages/representation.rs`
- File: `crates/panproto-reference/src/enumerate/representation.rs`

### Mutation equivalences

None. The bounded differential compares every observable field, and checks that both the reference and the production composite `from_wtype_embedded` then `to_wtype` refuse each refusal named by this law, except edge-id exhaustion, which needs $2^{32}-1$ edges and is pinned by the counter test of [L-representation-1](#L-representation-1).

### Audit record

- Author: representation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## Canonical bytes are injective and decode exactly on their image {#L-representation-4}

### Statement

For graphs and tables, let $b_X$ be the canonical byte writer and $d_X$ its decoder. For every well-formed value $x$ on which $b_X$ is defined, $d_X(b_X(x)) = x$, and equal canonical bytes imply equal values. For a validated tree $T$, let $c(T)$ replace the stored root by its unique structural root and sort the fan list by canonical fan record. Validation here requires (i) every node-map key to equal the stored node id, (ii) every arc and fan endpoint to name a node, and (iii) exactly one structural root. Then $d_{\mathsf{tree}}(b_{\mathsf{tree}}(T))=c(T)$, and equal tree bytes imply equal canonicalizations. A decoder accepts exactly one complete canonical record: it refuses unknown discriminants, truncated or trailing bytes, nonascending map or set records, and malformed graph or table data.

Graph bytes contain node and edge counts, then nodes and edges in ascending id order. Table bytes contain rows in ascending id order and relation pairs in ascending tuple order. Tree bytes contain nodes in ascending id order, arcs in stored order, and fans in ascending canonical fan-record order; the root is reconstructed as the unique node no arc enters, and decoded fans use canonical record order.

### Outside the domain

A writer returns `None` when a contained value has no canonical value encoding. A decoder returns `None` for any byte string outside its writer's image, including a decoded graph or table that is not well formed and a decoded tree without exactly one root. A tree's stored root and original fan-list order are outside the byte layout. The injectivity claim does not cover unvalidated trees. For instance, a tree filed as key 0 with stored node id 0 and root 0 has the same bytes as one filed as key 1 with stored node id 0 and root 1; both serialize the same node record, but the latter violates the key/id invariant.

### Definitions used

Every variable-length sequence begins with its `u64` length. Strings, optional values, maps, node shapes, and values use the canonical primitive encodings defined by the value layer. Strictly ascending order means that each decoded key is greater than the preceding key, which rejects duplicates as well as descending input.

### Proof

1. Each writer emits a discriminated, length-delimited concatenation. Primitive readers consume the same fields in the same order and reject a missing field or unknown tag.
2. Graph and table writers traverse ordered maps and sets. Their readers require strict ascent before insertion, so successful decoding recovers each key-value pair once and only once.
3. Tree nodes are handled in the same way, and arcs retain sequence order. Sorting fans before writing makes the byte image independent of their stored list order. The decoder reconstructs that order and derives the unique root from the arcs, which is exactly $c(T)$.
4. The final `done` check refuses trailing bytes. The carrier well-formedness and unique-root checks refuse structurally invalid records.
5. Induction over the emitted fields proves the two round-trip equations. If two byte strings are equal, deterministic decoding gives the same representative, proving injectivity over graphs and tables and over trees modulo the intentionally erased root and fan-list order.

### Executable counterpart

- Law ID: `representation.canonical-serde`
- Reference: `panproto_reference::canonical::decode_graph`
- Reference: `panproto_reference::canonical::decode_table`
- Reference: `panproto_reference::canonical::decode_tree`
- Evidence scope: `panproto_reference::enumerate::representation::{graphs,tables,trees}; exact carrier: 8 graphs, 2 tables, 2 validated trees, plus 3 trailing-byte refusals, every proper prefix of a graph, a table and a tree, 1 unknown node-shape tag, nonascending node, row and pair records, a dangling edge, a dangling pair, a two-root tree, and 1 malformed tree-byte collision outside the theorem`
- Production certificate: `canonical_graph_table_and_tree_bytes_decode_to_the_same_values; command: cargo nextest run -p panproto-inst --test reference_representation --release --no-tests=fail -E 'test(=canonical_graph_table_and_tree_bytes_decode_to_the_same_values)'`
- Independent reference anchor: `a_graph_round_trips_through_its_encodings; command: cargo nextest run -p panproto-inst --test a_tree_is_a_graph_without_loss --release --no-tests=fail -E 'test(=a_graph_round_trips_through_its_encodings)'`
- File: `crates/panproto-inst/src/ginstance/canonical.rs`
- File: `crates/panproto-inst/src/ginstance/table_embed.rs`
- File: `crates/panproto-reference/src/pages/representation.rs`
- File: `crates/panproto-reference/src/enumerate/representation.rs`

### Mutation equivalences

None. The bounded differential compares every observable field, and checks the decoders' refusal of each class named by this law on production bytes or on reference-writer bytes of malformed values.

### Audit record

- Author: representation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## The identity envelope has one injective version-two layout {#L-representation-5}

### Statement

For an envelope with source-schema digest $s$, target-schema digest $t$, lens digest $l$, complement shape $h$, and format version $v$, `identity` returns 101 bytes: little-endian `u16` value 2; the 32 bytes of $s$, $t$, and $l$ in that order; one shape byte, with `WType`, `Graph`, `Functor`, and `Resolution` mapped to 0, 1, 2, and 3; then $v$ as little-endian `u16`. This mapping is injective.

### Outside the domain

There is none for the five-field version-two carrier. Bytes bearing another envelope version belong to the decoder's typed version-refusal domain, not to this writer.

### Definitions used

A digest is a 32-byte array. Little-endian `u16` writes the low byte before the high byte. The four complement shapes are distinct enum variants.

### Proof

1. The fixed slices `[0..2]`, `[2..34]`, `[34..66]`, `[66..98]`, byte 98, and `[99..101]` are disjoint and cover the output.
2. The implementation writes the stated field into each slice and maps each shape to a different byte.
3. Equality of two outputs gives equality on every slice. Little-endian encoding and the shape map are injective, so all five inputs are equal.

### Executable counterpart

- Law ID: `representation.identity-envelope`
- Reference: `panproto_reference::canonical::identity`
- Evidence scope: `panproto_inst::ComplementShape::ALL; exact carrier: 4 shape variants`
- Production certificate: `identity_envelope_layout_matches_the_production_manifest_writer; command: cargo nextest run -p panproto-inst --test reference_representation --release --no-tests=fail -E 'test(=identity_envelope_layout_matches_the_production_manifest_writer)'`
- Independent reference anchor: `identity_envelope_is_the_exact_version_two_layout; command: cargo nextest run -p panproto-reference --test schema_v2 --release --no-tests=fail -E 'test(=identity_envelope_is_the_exact_version_two_layout)'`
- File: `crates/panproto-inst/src/identity.rs`
- File: `crates/panproto-inst/src/segment/manifest.rs`
- File: `crates/panproto-reference/src/pages/representation.rs`
- File: `crates/panproto-reference/src/enumerate/representation.rs`

### Mutation equivalences

None. The bounded differential compares every observable field and typed refusal named by this law.

### Audit record

- Author: representation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## Every well-formed table has the specified graph form or one typed refusal {#L-representation-6}

### Statement

For every production `FInstance` $T$, and so for every coherent raw table, `from_finstance(T)` maps row $r$ to plain node $r$, anchored at its table name and holding its cells as fields. Relation pair $(e,s,t)$ becomes an unpositioned edge from $s$ to $t$ labelled $e$. Pairs are traversed in ascending tuple order and receive edge ids from zero. The returned embedding maps each pair to its edge id, and `u32::MAX` is never allocated. The independent reference accepts a raw table record and performs the same mapping after checking that record.

### Outside the domain

The production converter receives an already validated `FInstance`; it does not emit a malformed-raw-table refusal. A raw reference table is outside the domain when some pair is not coherent: an endpoint row is absent, or names a row outside the label's endpoint table. The reference returns `MalformedTable` with the least such pair. The differential's `prod_table` adapter builds the `FInstance` first, and `FInstance::from_parts` refuses the same tables with `DanglingForeignKey`, naming a row rather than a pair: rows 0 in table `t` and 1 in table `u` with the pair $(t \to t, 0, 1)$ are refused at row 1 by both, although row 1 exists. Once adapted, the only converter refusal is `EdgeIdsExhausted` when the number of pairs exhausts the nonreserved edge-id carrier. This distinction assigns the malformed-input claim to the boundary that can observe it.

### Definitions used

A table is *coherent* when every pair's source row belongs to its label's source table and its target row to its label's target table; in particular both endpoints occur in the row map. The coherent raw tables are exactly the tables a production `FInstance` holds. The pair order is the lexicographic order of edge label, source row, and target row. A plain node has `Plain` shape and no value, discriminator, position, or annotations.

### Proof

1. The reference's ordered search over pairs returns the least incoherent pair. A successfully constructed `FInstance` has already discharged coherence, so production never meets it.
2. The converter's row loop copies every ordered row once into a plain node with the stated anchor and fields.
3. The pair loop uses the same checked id allocator as [L-representation-1](#L-representation-1). It emits one unpositioned edge and one embedding entry per pair, in the same order.
4. Checked increment permits `u32::MAX - 1` and refuses before allocating `u32::MAX`, which proves the exhaustion clause.

### Executable counterpart

- Law ID: `representation.table-to-graph-total`
- Reference: `panproto_reference::encode::table_to_graph`
- Evidence scope: `panproto_reference::enumerate::representation::tables; exact carrier: 2 tables, plus 1 one-pair table, 1 three-pair table, 1 dangling-pair refusal, 1 incoherent-pair refusal and 1 saturated embedding refusal`
- Production certificate: `table_to_graph_matches_production_on_the_exact_carrier_and_refusals; command: cargo nextest run -p panproto-inst --test reference_representation --release --no-tests=fail -E 'test(=table_to_graph_matches_production_on_the_exact_carrier_and_refusals)'`
- Independent reference anchor: `the_table_graph_round_trip_is_a_bijection; command: cargo nextest run -p panproto-inst --test a_table_is_a_graph_without_loss --release --no-tests=fail -E 'test(=the_table_graph_round_trip_is_a_bijection)'`
- File: `crates/panproto-inst/src/ginstance/table_embed.rs`
- File: `crates/panproto-reference/src/pages/representation.rs`
- File: `crates/panproto-reference/src/enumerate/representation.rs`

### Mutation equivalences

None. The bounded differential compares every observable field and both malformed-table refusals with the adapter's `DanglingForeignKey`, and production's `EdgeIdsExhausted` at a saturated embedding. The reference's `EdgeIdsExhausted` needs $2^{32}-1$ pairs; its counter is pinned by the unit test `representation_counters_allocate_max_minus_one_but_never_max`.

### Audit record

- Author: representation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## A graph has a table form exactly when it is table-shaped {#L-representation-7}

### Statement

Fix a schema $S$. `graph_to_table(G,S)` returns a table exactly when $G$ is well formed and table-shaped relative to $S$: (i) every node is plain, carries only its anchor and flat fields, and sources no edge of a reserved fan kind, (ii) every edge is unpositioned, belongs to $S$, and joins nodes anchored at its declared endpoints, (iii) no two edges have the same label and endpoints, (iv) each functional foreign key has at most one lift from a source row, and (v) each required foreign key has a lift from every row of its source table. The result has row $r$ equal to node $r$'s anchor and fields and has one relation pair for each edge. On a graph produced by [L-representation-6](#L-representation-6), the reverse direction returns the original table only when its cells are flat and it satisfies these schema-relative lift conditions.

### Outside the domain

Every graph violating one of the stated conditions is refused. The reference oracle returns no table; production returns the first `NotTableReason` in its documented order. The focused witnesses cover a nested cell, an edge absent from the schema, two lifts of a functional foreign key, a missing required lift, a plain node sourcing a declared `hyper_parent` edge, which production refuses as `FanNode`, a node carrying a discriminator, and an edge whose target anchor differs from its label's. Edge ids need not be consecutive and are intentionally forgotten. A nested table still has a forward graph form under [L-representation-6](#L-representation-6), but that graph is outside this reverse domain; the smallest witness is row 0 of table `t` with field `c = [0]` and no relation pairs.

### Definitions used

A well-formed graph has node-map keys equal to the stored node ids and has both endpoints of every edge. A *flat field* holds neither a list nor a nested record. A foreign key is *functional* or *required* when the schema has a corresponding cardinality-one declaration, with required meaning non-nullable.

### Proof

1. The node pass checks representable node data and rejects the least nested field, establishing the flat-row condition. Every edge has a source node, so refusing each edge of a reserved kind refuses exactly the nodes that source one.
2. The edge pass rejects positions, edges absent from $S$, and incoherent endpoint anchors in edge-id order.
3. Grouping by label and endpoints detects duplicate relation pairs. Grouping outgoing edges by node and declared foreign key detects multiple functional lifts.
4. An ordered pass over required declarations and source rows finds the least missing lift. With no witness, the graph defines exactly one schema-relative table.
5. A graph from `from_finstance` has plain nodes, unpositioned edges, and one edge per set member. When all cells are flat and the declarations hold, reading it restores the table and discards only the embedding's edge ids.

### Executable counterpart

- Law ID: `representation.graph-to-table-decision`
- Reference: `panproto_reference::encode::graph_to_table`
- Evidence scope: `panproto_reference::enumerate::representation::graphs; exact carrier: 8 graphs, plus positioned-edge, duplicate-pair, nested-cell, unknown-edge, functional-multiplicity, missing-required-lift, reserved-kind-source, node-data and incoherent-endpoint refusals`
- Production certificate: `graph_to_table_decision_matches_production_on_the_exact_carrier; command: cargo nextest run -p panproto-inst --test reference_representation --release --no-tests=fail -E 'test(=graph_to_table_decision_matches_production_on_the_exact_carrier)'`
- Independent reference anchor: `a_table_shaped_graph_round_trips_through_its_table; command: cargo nextest run -p panproto-inst --test a_table_is_a_graph_without_loss --release --no-tests=fail -E 'test(=a_table_shaped_graph_round_trips_through_its_table)'`
- File: `crates/panproto-inst/src/ginstance/table_embed.rs`
- File: `crates/panproto-reference/src/pages/representation.rs`
- File: `crates/panproto-reference/src/enumerate/representation.rs`

### Mutation equivalences

None. The bounded differential compares every field of each accepted table, and for each refusal reason in the domain clause checks that the reference refuses and that production returns that `NotTableReason`.

### Audit record

- Author: representation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.

## Fan-domain violations are complete and deterministically ordered {#L-representation-8}

### Statement

For a graph $G$, node $n$, and optional schema $S$, `violations_at(G, n, S)` returns every fan-domain violation at $n$ in the following order. If $n$ is absent, the list is empty. If $n$ is not a fan node, the list contains `ReservedKindOutsideFan` for each reserved-kind out-edge in ascending edge-id order. If $n$ is a fan node, the list contains: each in-edge in edge-id order; each out-edge that is neither a parent edge nor a named child edge, in edge-id order; a missing-parent or multiple-parent witness; each duplicated child label in ascending label order; then, when $S$ is present, either the unknown hyper-edge witness or each child label outside its declared signature in ascending label order.

### Outside the domain

There is no refusal. The function reports local defects even when the graph as a whole is malformed. Schema-relative checks are omitted when no schema is supplied.

### Definitions used

A *fan node* has shape `HyperEdge`. A parent edge has kind `hyper_parent`; exactly one is required. A child edge has kind `hyper_label` and a present name; at most one child edge may have each name. Every in-edge and every other out-edge is foreign incidence. A non-fan node may not source either reserved kind.

### Proof

1. Node lookup gives the empty-list clause.
2. For a non-fan node, the ordered out-edge list is filtered by reserved kind and mapped without reordering, which gives exactly the stated witnesses.
3. For a fan node, the first pass collects in-edges. The ordered out-edge pass partitions edges into parents, named children grouped in an ordered map, and foreign edges appended immediately.
4. The parent-count match appends exactly one parent witness when the count differs from one. Traversing the ordered child map then appends every duplicate-label witness.
5. Schema lookup appends one unknown-hyper-edge witness or filters the same ordered label keys against the signature. No branch drops an earlier witness, so the result is complete and has the stated order.

### Executable counterpart

- Law ID: `representation.fan-domain`
- Reference: `panproto_reference::fan::violations_at`
- Evidence scope: `explicit labelled fan witnesses; exact carrier: 4 graph/node/schema cases covering absence, reserved vocabulary, missing parent, unknown hyper-edge, foreign incidence, multiple parents, duplicate labels and unknown labels`
- Production certificate: `fan_domain_lists_the_same_ordered_typed_violations; command: cargo nextest run -p panproto-inst --test reference_representation --release --no-tests=fail -E 'test(=fan_domain_lists_the_same_ordered_typed_violations)'`
- Independent reference anchor: `unreify_inverts_reify; command: cargo nextest run -p panproto-inst --test fans_reify_exactly_on_the_fan_domain --release --no-tests=fail -E 'test(=unreify_inverts_reify)'`
- File: `crates/panproto-inst/src/ginstance/reify.rs`
- File: `crates/panproto-inst/src/fan_violation.rs`
- File: `crates/panproto-reference/src/pages/representation.rs`
- File: `crates/panproto-reference/src/enumerate/representation.rs`

### Mutation equivalences

None. The bounded differential compares every observable field and typed refusal named by this law.

### Audit record

- Author: representation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit is pending; the bounded production/reference differential passes.
