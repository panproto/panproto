# Edits

An edit is useful only when its payload records enough of the old state to reject stale input, compose without consulting a state, and run backward. The eight laws on this page fix that discipline for graph, table, and tree edits. They also fix canonical edit bytes, replacement semantics, and the two whole-state squares that define edit translation through a lens.

The coverage scope of the edit algebra is a finite carrier fixed by its parameters. A schema has at most two vertices, drawn from `a` and `b`, and at most two edges of the one kind `e`, without names, which gives sixteen schemas. Over each schema the scope holds every graph of at most three nodes and two edges, every table of at most three rows and two foreign-key pairs, and every tree of at most three nodes, under every parent map and arc order. Node values, table cells in column `f` and tree fields `f` are absent, `0` or `1`, and every edge, pair and arc is labelled by the schema edge between the vertices of its endpoints. Ids are labelled, so no isomorphic copy is pruned.

A word has at most three edits, and each letter is drawn from an alphabet generated at the state its prefix reaches. Additions of nodes, rows and edges take two ids: the fresh id, one above the largest id present, and the floor, one above the largest id present at any earlier point of the word. The other letters name the state's own elements: deletions with their payloads, changes of a value to another value of the alphabet, foreign-key additions and removals, and for trees leaf deletions, anchor changes and parent changes, the root's included. A letter that refuses ends its word, and its typed refusal is the word's outcome. Thus a graph word ends at a node deletion with incident edges, a table word at the addition of a present pair, and a tree word at the deletion of the root or of an inner node, at any move of the root, or at a move under the node itself or a descendant. Deleting, contracting and moving the root are refused alike as root operations (`RootOperation`), since the root has no parent arc to remove or replace; a move of the root is refused as a root operation before any cycle check, even though every other node is its descendant. `c8_scope_status` in the enumerator reports these parameters and reports the scope complete.

Over the sixteen schemas the scope holds 61,066 graphs, 32,038 tables and 3,156 trees. From them start 847,822,000 graph words, 403,354,180 table words and 65,871,966 tree words, the empty word of each state included. An independent count agrees with each enumeration: it runs on structures without values, since a value never decides which letters exist or apply, and multiplies by the three values of each node, row or tree node and by the arc orders of each tree. Three exhaustive differentials, run in release mode, walk every word and compare production with the reference after each letter. Each compares the letter's outcome, typed refusals included. For graphs and tables they also compare the composite delta of the word, its inverse, its canonical bytes and its application from the initial state; for trees they compare the lowering of the whole word as a sequence, the embedding it reaches, its application to the initial graph form and the word's canonical bytes. A fourth compares independence and merge on every ordered pair of letters at every initial graph, and applies both orders and the merge whenever both letters apply and are independent.

The smaller carriers named in some blocks below keep fixed alphabets and serve as witnesses for cases outside this scope: shapes, fields, positions and edge labels of graph deltas, table renames, fans and joins of trees, and the named projection carrier of edit translation. The statements themselves are not restricted to any carrier unless a law says so.

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
5. `independence` enumerates every shared structural or attribute footprint, both delete-use directions, anchor-sensitive edge use, and fan-domain interaction. An empty list therefore means that neither application invalidates the other's checked reads: deletion payloads, old attribute values, incident-edge deletion, free added ids and present endpoints.
6. The result checks also survive the merge. Ids and endpoints follow from step 5, since an `AddAdd` or `DeleteUse` conflict would otherwise be reported. For the fan domain, `violations_at(G, n)` reads only the shape of $n$ and the edges incident to $n$ with their labels, so it is local to $n$. If only one delta changes that local view, the merged result has the same view at $n$ as that delta's result, which is in the fan domain. If both change it, independence excludes every *strong* change at $n$: a shape change between plain and fan, an added or deleted reserved-kind edge leaving $n$, or a relabelling to or from a reserved kind. The remaining *weak* changes add or delete ordinary edges at $n$, replace their endpoints, or relabel them between ordinary kinds. At a plain node no weak change creates a reserved out-edge, and at a fan node every ordinary incident edge is already a violation in the single result that adds it, while a deleted one leaves the other delta's result in violation. Thus the merged result is in the fan domain whenever both single results are.
7. On an independent pair the two maps have disjoint incompatible entries. Their normalized union has the same per-element action as either application order, so both composites and `merge` produce the same graph.

### Executable counterpart

- Law ID: `edits.e1`
- Reference: `panproto_reference::edit::apply`
- Reference: `panproto_reference::edit::independence`
- Reference: `panproto_reference::edit::merge`
- Enumerator: `panproto_reference::enumerate::edits::c8_graph_words; bound: word_length=3`
- Exhaustive differential test: `c8_graph_edit_algebra_matches_reference_on_every_word_of_the_coverage_scope; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=c8_graph_edit_algebra_matches_reference_on_every_word_of_the_coverage_scope)'`
- Property differential test: `c8_graph_independence_and_merge_match_reference_on_every_letter_pair_of_the_coverage_scope; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=c8_graph_independence_and_merge_match_reference_on_every_letter_pair_of_the_coverage_scope)'`
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

For tables, `compose_table(a, b)` is the net compare-and-set delta of applying $a$ and then $b$, whenever their intermediate payloads agree. For well-formed tables $A$ and $B$, `diff_table(A, B)` is normalized, applies to $A$ with result $B$, and is the identity exactly when the table states are equal.

### Outside the domain

Inversion is total on normalized deltas. Table composition returns the first `Mismatch` in element or attribute order when the second delta does not read the state written by the first. Applying an inverted delta may still refuse when it is presented to a state other than the original delta's result.

The `diff_table` claim excludes tables that are not well formed. Applying a table delta checks the endpoints of every resulting pair, not only the touched ones, so on a table whose pair leaves its edge's tables even the identity is refused. For instance, with one row `0` in table `a` and the pair `(a -e-> b, 0, 0)`, `diff_table` of the table with itself is the identity and applying it returns `MissingRowEndpoint` for row `0`.

### Definitions used

The two *legs* of a delta are its deleted and added structural parts. An attribute change is the ordered pair $(o,n)$ of its required old value and written new value. Table equality includes row ids, table names, cells, and relation pairs. A table is *well formed* when the source row of every pair lies in the edge's source table and the target row in its target table; production tables are well formed by construction.

### Proof

1. Exchanging the two structural maps twice restores both maps. Exchanging every ordered attribute pair twice restores each pair, proving involution.
2. Each table row, pair, cell, and table name is represented by the same generic element-or-attribute cases as a graph delta. Thus the generic inversion and composition arguments apply without an additional case.
3. Composition compares the first delta's written intermediate payload with the second delta's required payload. Equality selects the unique net action in the case table; inequality returns the owning element or attribute.
4. `diff_table` walks the union of row ids and relation pairs. Missing items become deletions or additions, and unequal kept fields become attributes. Every deletion and old value is read from $A$, so the checks on $A$ pass; a row of $A$ absent from $B$ has all its pairs absent from $B$, since $B$ is well formed, so those pairs are deleted with it. Applying those disjoint changes yields $B$, whose pairs pass the endpoint check because $B$ is well formed.
5. Equal inputs emit no structural or attribute change. Conversely, identity output means every compared row, cell, table name, and pair agreed, so the inputs were equal.

### Executable counterpart

- Law ID: `edits.e2`
- Reference: `panproto_reference::edit::Delta::invert`
- Reference: `panproto_reference::edit::invert_table`
- Reference: `panproto_reference::edit::compose_table`
- Reference: `panproto_reference::edit::diff_table`
- Enumerator: `panproto_reference::enumerate::edits::c8_table_words; bound: word_length=3`
- Exhaustive differential test: `c8_table_edits_match_reference_on_every_word_of_the_coverage_scope; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=c8_table_edits_match_reference_on_every_word_of_the_coverage_scope)'`
- Property differential test: `e2_diff_table_applies_and_detects_equality_on_every_well_formed_carrier_pair; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=e2_diff_table_applies_and_detects_equality_on_every_well_formed_carrier_pair)'`
- Outside-domain test: `e2_diff_table_law_excludes_tables_whose_pairs_leave_their_edge_tables; witness: row 0 in table a with pair (a -e-> b, 0, 0)`
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

For a valid tree $T$ with durable embedding $E$, `graph_delta_from_tree_edit(T, E, e)` lowers one tree edit $e$ to that same algebra. Each step lowers to the change of the embedded graph it makes, with a node id that passes between a fan node and a tree node read as a replacement, and a sequence is lowered step by step and composed left to right into one net delta. Applying the result to the embedded graph gives the graph of applying $e$ to $T$ under the updated embedding. The net delta need not be the graph differ of the two states: deleting a node and inserting a node under its id lowers to a replacement of that node.

### Outside the domain

Graph composition returns `Mismatch` when adjacent element payloads or attribute values disagree, when two additions compete, or when another case has no net action. Tree lowering returns its first typed tree, embedding, or id-allocation refusal and leaves the caller's embedding unchanged. The checked `u32` embedding may return `IdsExhausted`; the unbounded law has no finite-id refusal.

### Definitions used

The *per-element action* is one of none, deletion, addition, replacement, or a finite ordered map of attribute changes. A *durable embedding* records the graph edge of every tree arc and the graph nodes and edges of every fan, together with monotone allocation floors. Two bracketings are *both defined* when `compose(compose(a, b), c)` and `compose(a, compose(b, c))` both return a delta.

### Proof

1. The composition implementation collects every element touched by either input and applies the complete action table independently at each element.
2. Every successful table entry has the same observable effect as executing its row action and then its column action. Combining the entries therefore gives the net sequential effect.
3. The none row and column prove both unit laws. Inversion maps `Del(p)` to `Add(p)`, `Repl(p,q)` to `Repl(q,p)` and `Attr(k: o -> n)` to `Attr(k: n -> o)`, and each defined entry of the table at $(a,b)$ is, after inversion, the defined entry at $(b^{-1},a^{-1})$; thus inversion reverses composition order. When both bracketings are defined at an element, each is the net action of running the three actions in order, by step 2 applied twice, so the two agree. The test asserts the unit laws, regularity, the reversal and agreement of both defined bracketings on every atomic pair and triple of the carrier.
4. Tree lowering applies the whole edit to a clone of the embedding first and commits the clone only after all checks succeed. Refusal is therefore atomic.
5. Each step lowers to the differ of the embedded graphs before and after it, except at an id whose node changes between a fan node and a tree node. Such an id arises only when a step gives a tree node the id of a fan node, which relocates the fan node to a fresh id; the step deletes the fan node and adds the tree node under that id, which is a replacement. Fresh graph ids are drawn from the cloned floors in edit order.
6. Each step delta applies to the state the earlier steps reach, so their left-to-right composite is defined at every element, and by step 2 applying it gives the final embedded graph. The composite is not in general the differ of the initial and final graphs: a node deleted by one step and inserted by a later one is a replacement in the composite, while the differ would report at most attribute changes.

### Executable counterpart

- Law ID: `edits.e3`
- Reference: `panproto_reference::edit::Delta::compose`
- Reference: `panproto_reference::edit::graph_delta_from_tree_edit`
- Enumerator: `panproto_reference::enumerate::edits::c8_tree_words; bound: word_length=3`
- Exhaustive differential test: `c8_tree_edits_and_lowering_match_reference_on_every_word_of_the_coverage_scope; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=c8_tree_edits_and_lowering_match_reference_on_every_word_of_the_coverage_scope)'`
- Property differential test: `c8_graph_edit_algebra_matches_reference_on_every_word_of_the_coverage_scope; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=c8_graph_edit_algebra_matches_reference_on_every_word_of_the_coverage_scope)'`
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
- Enumerator: `panproto_reference::enumerate::edits::c8_graph_words; bound: word_length=3`
- Exhaustive differential test: `c8_graph_edit_algebra_matches_reference_on_every_word_of_the_coverage_scope; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=c8_graph_edit_algebra_matches_reference_on_every_word_of_the_coverage_scope)'`
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

Applying a table delta checks complete deleted row payloads and relation pairs, the old values of every changed cell and table name, complete incidence for each deleted row, free added row ids, and existing endpoints for every resulting pair. Success deletes, updates, and adds in that order.

A surface table edit lowers, without reading a state, to this canonical compare-and-set form: `InsertRow` adds the row, `DeleteRow` deletes the row with its stated payload and every listed pair, `UpdateCell` and `MoveRow` are a cell and a table-name change, `AddForeignKey` and `RemoveForeignKey` add and delete one pair, `Delta` is itself, and `Batch` is the left-to-right composite of its steps. A single surface step first checks the state it names and then applies its lowering. Whenever an edit $e$ applies to a table $T$ with result $T'$, applying the inverse of its lowering to $T'$ gives $T$, and applying the production inverse edit to $T'$ gives $T$.

### Outside the domain

Applying a delta refuses as follows. The first absent row or pair yields `RowNotFound` or `ForeignKeyNotFound`. Payload disagreement yields `StaleTable` or `StaleCell`, and an undeleted incident pair yields `DanglingForeignKeys`. Reused additions yield `DuplicateRow` or `DuplicateForeignKey`, and a relation pair whose source or target has the wrong row table yields `MissingRowEndpoint`.

The surface checks of a single step refuse before its delta is applied. An inserted id already present yields `DuplicateRow`. A deleted, updated or moved row that is absent yields `RowNotFound`, and one in another table yields `StaleTable`. A deleted row whose cells differ yields `StaleCell` at the least differing column, and one whose listed pairs are not exactly its incident pairs in ascending order yields `StaleIncidence`; this is the only source of `StaleIncidence`, which delta application never returns. An updated cell with another old value yields `StaleCell`. A row moved to another table while it has an incident pair yields `MissingRowEndpoint`. An added pair already present yields `DuplicateForeignKey`, and one whose row is outside the edge's table yields `MissingRowEndpoint`, source first. A removed pair that is absent yields `ForeignKeyNotFound`. Batch lowering returns the first `Mismatch` when its steps do not compose. Every refusal leaves the input table unchanged.

### Definitions used

A row payload is its table name together with its complete cell map. A relation pair is the tuple of schema edge, source row id, and target row id. A table delta is *compare-and-set* because every deletion and attribute update records the value it requires before writing.

### Proof

1. The implementation checks deletions, additions, attributes, and deleted-row incidence against the original table before cloning it for mutation.
2. Complete row payload comparison distinguishes table-name, cell, and incidence staleness with their specified variants.
3. Removing all checked structural deletions first frees replacement ids. Attribute changes then affect only retained rows by normal form.
4. Adding rows before the final endpoint check makes newly added rows available to newly added pairs. The endpoint check compares each row's table name with the corresponding edge endpoint anchor.
5. Since every possible refusal precedes assignment to the caller's table, failure is atomic. On success the three mutation phases implement exactly the structural and attribute maps of the delta.
6. Each surface variant writes its whole compare-and-set payload into its lowering: `DeleteRow` records the row's table, every cell and every listed pair, `UpdateCell` and `MoveRow` record the old value, and the pair edits record the pair. Inversion exchanges the legs and the old and new values (L-edits-2), so the inverse adds back the deleted row with its cells and pairs and restores every old value. When the lowering $d$ applies to $T$, the inverse reads exactly what $d$ wrote, its deletions remove only what $d$ added, and the rows it re-adds carry the pairs it re-adds, so it applies to $T'$ and yields $T$. A `Batch` lowers to a composite, whose inverse is the reversed composite of the inverses (L-edits-3), and the same argument applies to that net delta.

### Executable counterpart

- Law ID: `edits.t5`
- Reference: `panproto_reference::edit::apply_table`
- Reference: `panproto_reference::edit::table_edit_delta`
- Reference: `panproto_reference::edit::apply_table_step`
- Reference: `panproto_reference::edit::delete_row_delta`
- Enumerator: `panproto_reference::enumerate::edits::c8_table_words; bound: word_length=3`
- Exhaustive differential test: `c8_table_edits_match_reference_on_every_word_of_the_coverage_scope; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=c8_table_edits_match_reference_on_every_word_of_the_coverage_scope)'`
- Property differential test: `t5_surface_table_edits_lower_check_and_invert_like_the_reference; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=t5_surface_table_edits_lower_check_and_invert_like_the_reference)'`
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

For `JoinFeatures { primary, joined, produce }`, `produce.id` must equal `primary`. The operation removes every joined node and its incoming arc, replaces the primary payload by `produce`, and appends each joined node's children to the primary in joined-node order and then original child-arc order. Each reparented edge keeps its kind and name and changes its source anchor to the produced primary anchor. Every fan whose parent or member is a joined node is removed; other fans, including fans naming a reparented child or the primary, are kept in order. Stale parent-map entries are removed.

Tree application and graph lowering agree: applying the tree edit and embedding the result yields the same graph as lowering the edit to a graph delta and applying that delta to the original embedded graph.

### Outside the domain

A missing primary or joined node, a repeated joined id, a joined id equal to the primary, a joined id equal to the root, a joined node that is an ancestor of the primary, a produced node with the wrong id, a malformed embedding, or an exhausted allocation floor returns its corresponding typed tree-edit refusal. The reference names a joined root `RootOperation` and a joined ancestor `Cycle`; production returns `ApplyFailed` and `CycleDetected` with the same node and primary. The tree and embedding remain unchanged.

### Definitions used

The *incoming arc* of a nonroot tree node is its unique parent arc. *Joined-node order* is the stored order of the `joined` vector; *child-arc order* is the order in the tree's arc sequence. Reparenting preserves that nested order.

### Proof

1. Preflight validates the primary, the produced id, each joined id (present, not the primary, not repeated, not the root, not an ancestor of the primary) and the embedding before modifying cloned state. Excluding the root and the ancestors of the primary keeps the result a tree: no joined node lies on the primary's root path, so reparenting under the primary closes no cycle.
2. For each joined node, the algorithm records its outgoing arcs in their stored order, removes its incoming arc and node, and deletes its parent-map entry.
3. Recorded child arcs are appended under the primary by the two stated iteration orders. Updating each edge's source anchor makes the label coherent with `produce`.
4. A fan naming a joined node would name an absent node, so it is removed with its fan node and fan edges; every other fan names only surviving nodes and is kept, its embedding entry unchanged.
5. The graph lowering performs the same deletions, removes the same fan nodes and edges, emits the same replacement attributes on the primary (with the fan label changes a new anchor implies for fan edges into the primary), and allocates one new edge per reparented child in that order.
6. The updated embedding records those allocated ids. Thus embedding the tree result and applying the graph delta produce identical nodes, edge payloads, and allocation floors.

### Executable counterpart

- Law ID: `edits.tr14`
- Reference: `panproto_reference::edit::apply_tree`
- Reference: `panproto_reference::edit::graph_delta_from_tree_edit`
- Enumerator: `panproto_reference::enumerate::edits::join_cases; bound: cases=9`
- Exhaustive differential test: `e3_tr14_tree_lowering_and_application_match_reference_for_every_primitive; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=e3_tr14_tree_lowering_and_application_match_reference_for_every_primitive)'`
- Property differential test: `tr14_join_refusals_match_reference_and_leave_tree_and_embedding_unchanged; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=tr14_join_refusals_match_reference_and_leave_tree_and_embedding_unchanged)'`
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
- Enumerator: `panproto_reference::enumerate::edits::c8_graph_words; bound: word_length=3`
- Exhaustive differential test: `c8_graph_edit_algebra_matches_reference_on_every_word_of_the_coverage_scope; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=c8_graph_edit_algebra_matches_reference_on_every_word_of_the_coverage_scope)'`
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

Let a lens map source graph $S$ to view $V$ and complement $C$. Write `diff_keeping_replacements(A, B, d)` for the differ from $A$ to $B$ in which every element that $d$ replaces and that both $A$ and $B$ hold becomes a replacement from its payload in $A$ to its payload in $B$. For an applicable source delta $d$, `get_edit` returns `diff_keeping_replacements(V, V', d)` with $V'$ = `get(apply(d,S)).view`, and the complement produced by that latter `get`. For an applicable view delta $d_V$, `put_edit` returns `diff_keeping_replacements(S, S', d_V)` with $S'$ = `put(S, apply(d_V,V), C).source`, and the complement produced by that `put`.

On the named projection carrier, the production tree edit lens satisfies these two equations after tree edits are lowered through their durable embeddings, its graph-get entry point yields the same view delta as the whole-state oracle, and its threaded complement agrees with whole-state `get` or `put` after the edit. Beyond that carrier the production claim is not made here.

### Outside the domain

If the incoming delta does not apply, the reference returns `EditTranslationError::Apply` with the exact graph refusal. If whole-state `get` or `put` refuses, it returns `EditTranslationError::Lens` with that refusal. Production may additionally refuse an incoherent tree embedding or a graph delta outside the tree image; these cases are outside the tree adapter's domain and retain their production error.

### Definitions used

`diff(A,B)` is the normalized id-keyed graph delta that applies to $A$ with result $B$; it never contains a replacement with equal payloads. A delta *replaces* an element when the element's id is in both its deleted and added parts. A *translation square* commutes when applying the translated edit after the state operation equals performing the state edit first and then the state operation. Complement agreement uses the complement returned at the square's lower-right state.

### Proof

1. Keeping a replacement changes how a delta is read, not the state it reaches: at a kept element $x$ both the replacement and the differ's entry take the payload of $x$ in $A$ to its payload in $B$. Applying a node replacement also requires every edge incident to $x$ in $A$ to be deleted or replaced. In the get direction $d$ deletes or replaces every source edge incident to a node it replaces, and restriction keeps element ids, so an incident view edge held by both views is one that $d$ replaces and is itself kept as a replacement; any other is absent from one view and deleted or added by the differ. The put direction is the same argument with $d_V$ and the sources. Hence `diff_keeping_replacements` applies to the earlier state with the later one as result.
2. Replacements are kept because the identity policy reads an id in both parts of a delta as a new element under a reused id, never as an unchanged one. Translation preserves composition (a delete followed by an add translates to the composite of their translations, which is a replacement), so the translation of a replacement is a replacement whenever both views hold the element; a state differ cannot see it.
3. The reference get translator applies $d$, runs whole-state `get` before and after, and takes `diff_keeping_replacements` of the two views. Step 1 gives the get square, and the returned complement is that of the edited source's `get`, the lower-right complement by definition.
4. The reference put translator applies $d_V$, runs whole-state `put` with $C$, and takes `diff_keeping_replacements` of the original and returned sources. Step 1 gives the put square.
5. Production `get_graph_edit` restricts the source graph before and after applying the incoming delta, takes their differ, and re-inserts every replacement of the incoming delta whose element both restricted views hold, before mapping edge ids into the view presentation. This is the reference get translator on the same two states. Production tree `get_edit` and `put_edit` update a staged complement and commit it only after translation succeeds, and lowering their results gives the deltas compared by the reference equations.
6. The named projection carrier contains retained and hidden leaves. Deleting either kind distinguishes a view edit from absorbed residue, replacing either kind checks that a replacement is carried when the leaf is retained and absorbed when it is hidden, and the put carrier deletes and replaces a retained leaf while reconstructing the hidden residue. A graph witness replaces the retained leaf and its arc by equal payloads, which the state differ alone would read as the identity. Exact delta and complement equality on every named case checks these paths; the production claim rests on these cases, not on a general argument.

### Executable counterpart

- Law ID: `edits.translation-square`
- Reference: `panproto_reference::edit::diff`
- Reference: `panproto_reference::edit::diff_keeping_replacements`
- Reference: `panproto_reference::edit::get_edit`
- Reference: `panproto_reference::edit::put_edit`
- Enumerator: `panproto_reference::enumerate::edits::edit_translation_cases; bound: cases=8`
- Exhaustive differential test: `generic_whole_state_edit_oracles_match_actual_tree_lens_translation; command: cargo nextest run -p panproto-lens --test reference_edit_translation --no-tests=fail -E 'test(=generic_whole_state_edit_oracles_match_actual_tree_lens_translation)'`
- Property differential test: `graph_get_keeps_same_payload_replacements_of_the_input; command: cargo nextest run -p panproto-lens --test reference_edit_translation --no-tests=fail -E 'test(=graph_get_keeps_same_payload_replacements_of_the_input)'`
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
