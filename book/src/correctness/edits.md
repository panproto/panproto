# Edits

An edit is useful only when its payload records enough of the old state to reject stale input, compose without consulting a state, and run backward. The eight laws on this page fix that discipline for graph, table, and tree edits. They also fix canonical edit bytes, replacement semantics, and the two whole-state squares that define edit translation through a lens.

The principal finite release carrier uses graphs with at most three nodes and one edge, tables with at most three rows and one relation pair, and edit words of length at most three. Its alphabets are deliberately small and labelled: anchors `a` and `b`, one edge kind `e`, one field `f`, values absent or integer zero, and the ids named by each enumerator. The translation square instead uses one named three-node tree whose two leaves distinguish retained data from hidden residue. These bounds support exhaustive comparison; the statements themselves are not restricted to either carrier.

## Graph deltas apply atomically and merge exactly when independent {#L-edits-1}

### Statement

For a normalized graph delta $d$ and graph $G$, `apply(d, G)` checks every deletion payload, old attribute value, dangling edge, added id, added endpoint, and fan-domain condition before changing $G$. Success yields the graph obtained by deleting named elements, changing kept attributes, and adding named elements in that order. Failure returns the first typed refusal and leaves $G$ unchanged.

For two deltas based at one graph, `independence` returns the ordered set of their `DeleteDelete`, `DeleteUse`, `UpdateUpdate`, `AddAdd`, `AnchorUse`, and `FanUse` conflicts. `merge` is defined exactly when that set is empty and then returns the normalized union. Whenever both deltas apply and are independent, applying them in either order and applying their merge give identical graphs.

### Outside the domain

An ill-sorted payload or attribute value is refused as a type mismatch. A stale or absent deletion is refused by the corresponding node or edge error; a stale attribute is refused by its attribute-specific error. Incomplete incident-edge deletion yields `DanglingEdges`. Reused added ids yield `DuplicateNodeId` or `DuplicateEdge`, absent endpoints yield `MissingEndpoint`, and a malformed result yields `MalformedFan`. A nonempty conflict set makes `merge` return its first conflict.

### Definitions used

A *normalized graph delta* has no equal old and new attribute values and no attribute entry owned by an element the same delta deletes or adds. A node or edge id in both structural parts is a replacement. Two graph results are *identical* when all node ids, edge ids, payloads, labels, and positions agree.

### Proof

1. The checker traverses the ordered deletion, addition, and attribute maps without mutating the input. Each early return therefore has the stated variant and preserves the graph.
2. The dangling check compares every edge incident to each deleted node with the deletion set. Passing it makes the subsequent structural deletion closed under incidence.
3. Deletion frees replacement ids before additions are checked. Endpoint checks run on the post-deletion node set with additions included, so every added edge is well founded.
4. Attribute changes belong only to kept elements by normal form. Their old values were checked before mutation, which makes each write defined and gives the stated output.
5. `independence` enumerates every shared structural or attribute footprint, both delete-use directions, anchor-sensitive edge use, and fan-domain interaction. An empty list therefore means that neither application invalidates the other's checked reads.
6. On an independent pair the two maps have disjoint incompatible entries. Their normalized union has the same per-element action as either application order, so both composites and `merge` produce the same graph.

### Executable counterpart

- Law ID: `edits.e1`
- Reference: `panproto_reference::edit::apply`
- Reference: `panproto_reference::edit::independence`
- Reference: `panproto_reference::edit::merge`
- Enumerator: `panproto_reference::enumerate::edits::graph_delta_cases; bound: atomic_deltas=25`
- Exhaustive differential test: `e1_apply_matches_reference_on_every_graph_and_atomic_delta; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=e1_apply_matches_reference_on_every_graph_and_atomic_delta)'`
- Property differential test: `e1_independence_and_merge_match_reference_on_every_atomic_pair; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=e1_independence_and_merge_match_reference_on_every_atomic_pair)'`
- File: `crates/panproto-inst/src/graph_edit/apply.rs`
- File: `crates/panproto-inst/src/graph_edit/independence.rs`
- File: `crates/panproto-reference/src/pages/edits.rs`
- File: `crates/panproto-reference/src/enumerate/edits.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: edits documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Inversion and the table delta algebra preserve both legs {#L-edits-2}

### Statement

Inverting a normalized delta exchanges its deleted and added elements and exchanges every attribute's old and new values. Inversion is involutive. Graph and table deltas share this construction; table rows are elements with complete table-and-cell payloads, relation pairs are payload-free elements, and cells and table names are attributes.

For tables, `compose_table(a, b)` is the net compare-and-set delta of applying $a$ and then $b$, whenever their intermediate payloads agree. `diff_table(A, B)` is normalized, applies to $A$ with result $B$, and is the identity exactly when the table states are equal.

### Outside the domain

Inversion is total on normalized deltas. Table composition returns the first `Mismatch` in element or attribute order when the second delta does not read the state written by the first. Applying an inverted delta may still refuse when it is presented to a state other than the original delta's result.

### Definitions used

The two *legs* of a delta are its deleted and added structural parts. An attribute change is the ordered pair $(o,n)$ of its required old value and written new value. Table equality includes row ids, table names, cells, and relation pairs.

### Proof

1. Exchanging the two structural maps twice restores both maps. Exchanging every ordered attribute pair twice restores each pair, proving involution.
2. Each table row, pair, cell, and table name is represented by the same generic element-or-attribute cases as a graph delta. Thus the generic inversion and composition arguments apply without an additional case.
3. Composition compares the first delta's written intermediate payload with the second delta's required payload. Equality selects the unique net action in the case table; inequality returns the owning element or attribute.
4. `diff_table` walks the union of row ids and relation pairs. Missing items become deletions or additions, and unequal kept fields become attributes. Applying those disjoint changes reconstructs the second state.
5. Equal inputs emit no structural or attribute change. Conversely, identity output means every compared row, cell, table name, and pair agreed, so the inputs were equal.

### Executable counterpart

- Law ID: `edits.e2`
- Reference: `panproto_reference::edit::Delta::invert`
- Reference: `panproto_reference::edit::invert_table`
- Reference: `panproto_reference::edit::compose_table`
- Reference: `panproto_reference::edit::diff_table`
- Enumerator: `panproto_reference::enumerate::edits::tables; bound: rows=3`
- Exhaustive differential test: `e2_t5_table_apply_and_algebra_match_the_exact_release_carrier; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=e2_t5_table_apply_and_algebra_match_the_exact_release_carrier)'`
- Property differential test: `e2_e3_algebra_matches_reference_on_the_exact_atomic_word_carrier; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=e2_e3_algebra_matches_reference_on_the_exact_atomic_word_carrier)'`
- File: `crates/panproto-inst/src/graph_edit/compose.rs`
- File: `crates/panproto-inst/src/table_delta.rs`
- File: `crates/panproto-reference/src/pages/edits.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: edits documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Composition and tree lowering compute one normalized delta {#L-edits-3}

### Statement

`compose(a, b)` computes the net normalized graph delta of $a$ followed by $b$. Identity is a two-sided unit, composition agrees across both bracketings whenever both bracketings are defined, and inversion reverses composition order.

For a valid tree $T$ with durable embedding $E$, `graph_delta_from_tree_edit(T, E, e)` lowers one tree edit $e$ to that same algebra. Applying the result to the embedded graph gives the graph of applying $e$ to $T$ under the updated embedding. A sequence is lowered left to right and composed into one net delta.

### Outside the domain

Graph composition returns `Mismatch` when adjacent element payloads or attribute values disagree, when two additions compete, or when another case has no net action. Tree lowering returns its first typed tree, embedding, or id-allocation refusal and leaves the caller's embedding unchanged. The checked `u32` embedding may return `IdsExhausted`; the unbounded law has no finite-id refusal.

### Definitions used

The *per-element action* is one of none, deletion, addition, replacement, or a finite ordered map of attribute changes. A *durable embedding* records the graph edge of every tree arc and the graph nodes and edges of every fan, together with monotone allocation floors.

### Proof

1. The composition implementation collects every element touched by either input and applies the complete action table independently at each element.
2. Every successful table entry has the same observable effect as executing its row action and then its column action. Combining the entries therefore gives the net sequential effect.
3. The none row and column prove both unit laws. Exhausting the finite action cases proves equality of both defined bracketings and proves that swapping legs reverses their order.
4. Tree lowering clones the embedding, handles one edit variant, and commits the clone only after all checks succeed. Refusal is therefore atomic.
5. Each primitive emits exactly the nodes, arcs, fan records, or kept-node attributes changed by the corresponding tree operation. Fresh graph ids are drawn from the cloned floors in edit order.
6. Sequence lowering repeats the primitive construction and composes its outputs. Step 2 makes the resulting delta equal to the graph difference between the initial and final embedded trees.

### Executable counterpart

- Law ID: `edits.e3`
- Reference: `panproto_reference::edit::Delta::compose`
- Reference: `panproto_reference::edit::graph_delta_from_tree_edit`
- Enumerator: `panproto_reference::enumerate::edits::graph_delta_sequences; bound: word_length=3`
- Exhaustive differential test: `e2_e3_algebra_matches_reference_on_the_exact_atomic_word_carrier; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=e2_e3_algebra_matches_reference_on_the_exact_atomic_word_carrier)'`
- Property differential test: `e3_tr14_tree_lowering_and_application_match_reference_for_every_primitive; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=e3_tr14_tree_lowering_and_application_match_reference_for_every_primitive)'`
- File: `crates/panproto-inst/src/graph_edit/compose.rs`
- File: `crates/panproto-inst/src/delta_convert.rs`
- File: `crates/panproto-inst/src/tree_edit.rs`
- File: `crates/panproto-reference/src/pages/edits.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: edits documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Canonical edit bytes encode normalized parts in one order {#L-edits-4}

### Statement

Canonical graph-delta bytes begin with the five part counts, followed by deleted nodes, deleted edges, added nodes, added edges, and attribute changes. Canonical table-delta bytes analogously carry six counts and then deleted rows, added rows, deleted pairs, added pairs, cells, and table-name changes. Every map and set uses ascending key order.

On well-filed tree edits that are applicable to their paired release trees, canonical tree-edit bytes contain the flattened sequence after nested sequences are spliced and identities removed. Each remaining primitive has one fixed tag and field order. A primitive and a singleton sequence containing it therefore have identical bytes.

### Outside the domain

The graph and table claims range over normalized, well-sorted deltas. The tree claim ranges over edits whose inserted child node carries `child_id`, whose produced join node carries `primary`, and whose sequence is applicable to the paired tree. The independent reference writer returns `None` when one of these filing conditions fails; that option is the reference domain check. The production writer has an infallible streaming interface, so a misfiled input such as `InsertNode { child_id: 1, node.id: 2 }` is outside the law rather than a second canonical encoding.

### Definitions used

A *canonical part order* is the field order stated above, followed by the derived order of each part's keys. Tree *sequence normal form* is the ordered list of nonidentity primitives obtained by recursively flattening every sequence.

### Proof

1. Each writer separates the normalized representation into the stated typed parts. On the law's domain, the five or six part counts account for every source entry.
2. Ordered maps and sets supply ascending records without a second ordering choice. Fixed part order and fixed primitive tags determine the surrounding byte layout.
3. On the stated domain, values and nodes delegate to the canonical value and node writers, which fixes their nested byte layout compositionally.
4. Recursive flattening preserves primitive order and removes only identities. Writing the resulting length followed by each primitive proves equality of a primitive with its singleton sequence and of differently nested sequences with the same flattened list.
5. The independent reference writers spell each count, tag, and field directly. Byte equality on every enumerated edit therefore compares the layout itself, rather than two calls to one production serializer.

### Executable counterpart

- Law ID: `edits.e3a`
- Reference: `panproto_reference::edit::canonical_graph_delta_bytes`
- Reference: `panproto_reference::edit::canonical_table_delta_bytes`
- Reference: `panproto_reference::edit::canonical_tree_edit_bytes`
- Enumerator: `panproto_reference::enumerate::edits::graph_delta_cases; bound: atomic_deltas=25`
- Exhaustive differential test: `e3a_graph_delta_bytes_match_reference_on_every_atomic_delta; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=e3a_graph_delta_bytes_match_reference_on_every_atomic_delta)'`
- Property differential test: `e3a_tree_edit_bytes_match_reference_for_every_primitive_and_sequence; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=e3a_tree_edit_bytes_match_reference_for_every_primitive_and_sequence)'`
- Outside-domain test: `e3a_misfiled_tree_edit_is_outside_the_reference_byte_domain; witness: InsertNode child_id=1, node.id=2`
- File: `crates/panproto-inst/src/graph_edit/canonical.rs`
- File: `crates/panproto-inst/src/table_delta.rs`
- File: `crates/panproto-inst/src/tree_edit.rs`
- File: `crates/panproto-reference/src/pages/edits.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: edits documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Table edits are checked compare-and-set deltas {#L-edits-5}

### Statement

Applying a table delta checks complete deleted row payloads and relation pairs, the old values of every changed cell and table name, complete incidence for each deleted row, free added row ids, and existing endpoints for every resulting pair. Success deletes, updates, and adds in that order. A surface table edit lowers to this canonical compare-and-set form, and inversion retains enough information to reconstruct every deletion.

### Outside the domain

The first absent row or pair yields `RowNotFound` or `ForeignKeyNotFound`. Payload disagreement yields `StaleTable`, `StaleCell`, or `StaleIncidence`; an undeleted incident pair yields `DanglingForeignKeys`. Reused additions yield `DuplicateRow` or `DuplicateForeignKey`, and a relation pair whose source or target has the wrong row table yields `MissingRowEndpoint`. Every refusal leaves the input table unchanged.

### Definitions used

A row payload is its table name together with its complete cell map. A relation pair is the tuple of schema edge, source row id, and target row id. A table delta is *compare-and-set* because every deletion and attribute update records the value it requires before writing.

### Proof

1. The implementation checks deletions, additions, attributes, and deleted-row incidence against the original table before cloning it for mutation.
2. Complete row payload comparison distinguishes table-name, cell, and incidence staleness with their specified variants.
3. Removing all checked structural deletions first frees replacement ids. Attribute changes then affect only retained rows by normal form.
4. Adding rows before the final endpoint check makes newly added rows available to newly added pairs. The endpoint check compares each row's table name with the corresponding edge endpoint anchor.
5. Since every possible refusal precedes assignment to the caller's table, failure is atomic. On success the three mutation phases implement exactly the structural and attribute maps of the delta.

### Executable counterpart

- Law ID: `edits.t5`
- Reference: `panproto_reference::edit::apply_table`
- Enumerator: `panproto_reference::enumerate::edits::tables; bound: rows=3`
- Exhaustive differential test: `e2_t5_table_apply_and_algebra_match_the_exact_release_carrier; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=e2_t5_table_apply_and_algebra_match_the_exact_release_carrier)'`
- Property differential test: `table_translation_refuses_malformed_embeddings_and_non_table_graph_content_atomically; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=table_translation_refuses_malformed_embeddings_and_non_table_graph_content_atomically)'`
- File: `crates/panproto-inst/src/table_delta.rs`
- File: `crates/panproto-inst/src/table_edit.rs`
- File: `crates/panproto-reference/src/pages/edits.rs`
- File: `crates/panproto-reference/src/enumerate/edits.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: edits documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Joining tree features reparents every surviving child {#L-edits-6}

### Statement

For `JoinFeatures { primary, joined, produce }`, `produce.id` must equal `primary`. The operation removes every joined node and its incoming arc, replaces the primary payload by `produce`, and appends each joined node's children to the primary in joined-node order and then original child-arc order. Each reparented edge keeps its kind and name and changes its source anchor to the produced primary anchor. Stale parent-map entries are removed.

Tree application and graph lowering agree: applying the tree edit and embedding the result yields the same graph as lowering the edit to a graph delta and applying that delta to the original embedded graph.

### Outside the domain

A missing primary or joined node, a repeated joined id, a joined id equal to the primary, a produced node with the wrong id, a malformed embedding, or exhausted allocation floor returns its corresponding typed tree-edit refusal. The tree and embedding remain unchanged.

### Definitions used

The *incoming arc* of a nonroot tree node is its unique parent arc. *Joined-node order* is the stored order of the `joined` vector; *child-arc order* is the order in the tree's arc sequence. Reparenting preserves that nested order.

### Proof

1. Preflight validates the primary, joined ids, produced id, and embedding before modifying cloned state.
2. For each joined node, the algorithm records its outgoing arcs in their stored order, removes its incoming arc and node, and deletes its parent-map entry.
3. Recorded child arcs are appended under the primary by the two stated iteration orders. Updating each edge's source anchor makes the label coherent with `produce`.
4. The graph lowering performs the same deletions, emits the same replacement attributes on the primary, and allocates one new edge per reparented child in that order.
5. The updated embedding records those allocated ids. Thus embedding the tree result and applying the graph delta produce identical nodes, edge payloads, and allocation floors.

### Executable counterpart

- Law ID: `edits.tr14`
- Reference: `panproto_reference::edit::apply_tree`
- Enumerator: `panproto_reference::enumerate::edits::graph_delta_sequences; bound: word_length=3`
- Exhaustive differential test: `e3_tr14_tree_lowering_and_application_match_reference_for_every_primitive; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=e3_tr14_tree_lowering_and_application_match_reference_for_every_primitive)'`
- Property differential test: `e3_malformed_tree_embeddings_refuse_without_mutating_the_embedding; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=e3_malformed_tree_embeddings_refuse_without_mutating_the_embedding)'`
- File: `crates/panproto-inst/src/tree_edit.rs`
- File: `crates/panproto-inst/src/delta_convert.rs`
- File: `crates/panproto-reference/src/pages/edits.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: edits documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Delete followed by add is replacement, even for equal payloads {#L-edits-7}

### Statement

Composing deletion of element $x$ with a later addition under the same id yields a replacement containing both payloads. This remains a replacement when the payloads are equal. It is not normalized to the identity, its inverse is again the corresponding replacement, and applying it removes the predecessor's incident residue before installing the successor.

### Outside the domain

The composition is defined for every well-sorted deletion followed by every well-sorted addition under the same element id. Applying the replacement may refuse if the recorded predecessor is stale, its required incident deletions are incomplete, the successor has missing endpoints, or the result violates the fan domain.

### Definitions used

A *replacement* is one id present in both the deleted and added structural maps. Equal payloads do not identify the two occurrences: the left occurrence names the old element and the right occurrence names its successor.

### Proof

1. The `Del(p)` then `Add(q)` entry of the composition table is `Repl(p,q)` without an equality side condition.
2. Normalization removes attributes owned by a structural change but retains every id in both structural maps. It therefore retains `Repl(p,p)`.
3. Inversion exchanges the maps, turning `Repl(p,q)` into `Repl(q,p)`; equal payloads make that replacement self-inverse without making it identity.
4. Application deletes the old element before adding the new one. The dangling check is made against the predecessor, so residue attached to that occurrence does not transfer merely because the payload compares equal.

### Executable counterpart

- Law ID: `edits.replacement-semantics`
- Reference: `panproto_reference::edit::Delta::compose`
- Enumerator: `panproto_reference::enumerate::edits::graph_delta_cases; bound: deletion_payloads=6`
- Exhaustive differential test: `decision_9_delete_then_add_is_replacement_even_for_equal_payloads; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=decision_9_delete_then_add_is_replacement_even_for_equal_payloads)'`
- Property differential test: `decision_9_delete_then_add_is_replacement_even_for_equal_payloads; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=decision_9_delete_then_add_is_replacement_even_for_equal_payloads)'`
- File: `crates/panproto-inst/src/graph_edit/compose.rs`
- File: `crates/panproto-inst/src/graph_edit/apply.rs`
- File: `crates/panproto-reference/src/pages/edits.rs`
- File: `crates/panproto-reference/src/enumerate/edits.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: edits documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Edit translation is the whole-state differ in both directions {#L-edits-8}

### Statement

Let a lens map source graph $S$ to view $V$ and complement $C$. For an applicable source delta $d$, `get_edit` returns the delta `diff(V, get(apply(d,S)).view)` and the complement produced by that latter `get`. For an applicable view delta $d_V$, `put_edit` returns `diff(S, put(S, apply(d_V,V), C).source)` and the complement produced by that `put`.

The production tree edit lens satisfies these two equations after tree edits are lowered through their durable embeddings. Its graph-get entry point yields the same canonical view delta as the whole-state oracle on the graph-edit domain, and its threaded complement agrees with whole-state `get` or `put` after the edit.

### Outside the domain

If the incoming delta does not apply, the reference returns `EditTranslationError::Apply` with the exact graph refusal. If whole-state `get` or `put` refuses, it returns `EditTranslationError::Lens` with that refusal. Production may additionally refuse an incoherent tree embedding or a graph delta outside the tree image; these cases are outside the tree adapter's domain and retain their production error.

### Definitions used

`diff(A,B)` is the normalized id-keyed graph delta that applies to $A$ with result $B$. A *translation square* commutes when applying the translated edit after the state operation equals performing the state edit first and then the state operation. Complement agreement uses the complement returned at the square's lower-right state.

### Proof

1. The reference get translator applies $d$, runs whole-state `get` before and after, and takes the differ of the two views. The differ law gives the get square immediately.
2. Its returned complement is taken from the edited source's `get`, which is the lower-right complement by definition.
3. The reference put translator applies $d_V$, runs whole-state `put` with $C$, and differs the original and returned sources. The differ law gives the put square.
4. Production `get_graph_edit` restricts the source graph before and after applying the incoming delta, preserves presentation ids through the supplied embeddings, and returns their normalized differ. These are the same two graph states used by the reference get closure.
5. Production tree `get_edit` and `put_edit` update a staged complement and commit it only after translation succeeds. Lowering their results gives the graph deltas compared by the reference equations.
6. The named projection carrier contains retained and hidden leaves. Deleting either kind distinguishes a view edit from absorbed residue, while the put carrier deletes a retained leaf and reconstructs the hidden residue. Exact delta and complement equality on every named case therefore checks both nontrivial paths.

### Executable counterpart

- Law ID: `edits.translation-square`
- Reference: `panproto_reference::edit::diff`
- Reference: `panproto_reference::edit::get_edit`
- Reference: `panproto_reference::edit::put_edit`
- Enumerator: `panproto_reference::enumerate::edits::edit_translation_cases; bound: cases=5`
- Exhaustive differential test: `generic_whole_state_edit_oracles_match_actual_tree_lens_translation; command: cargo nextest run -p panproto-lens --test reference_edit_translation --no-tests=fail -E 'test(=generic_whole_state_edit_oracles_match_actual_tree_lens_translation)'`
- Property differential test: `generic_whole_state_edit_oracles_match_actual_tree_lens_translation; command: cargo nextest run -p panproto-lens --test reference_edit_translation --no-tests=fail -E 'test(=generic_whole_state_edit_oracles_match_actual_tree_lens_translation)'`
- File: `crates/panproto-lens/src/edit_lens.rs`
- File: `crates/panproto-lens/src/asymmetric.rs`
- File: `crates/panproto-reference/src/pages/edits.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: edits documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.
