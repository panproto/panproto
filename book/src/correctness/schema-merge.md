# Schema merge

Schema version control merges two branches structurally, selects a migration for every staged change, and stores its objects under a repository format. This page states those behaviours as laws: the merge of [`three_way_merge`](https://docs.rs/panproto-vcs/latest/panproto_vcs/merge/fn.three_way_merge.html) is the per-element merge the reference computes, its conflicts are listed in one stable order, staging selects the identity on what two schemas share, and a repository records and checks the format it was written in. The behaviour is explained in [Schema version control semantics](../explanation/vcs-semantics.md).

The executable reference is [`panproto_reference::pages::schema_merge`](https://docs.rs/panproto-reference/latest/panproto_reference/pages/schema_merge/). It models a schema as its vertex ids with their kinds and its set of edges named by their endpoints, and reads the merge off each element's value in the three schemas. It reads no diff and shares nothing with production, which merges from the two branches' diffs against the base, field by field.

## Definitions shared by the laws

A schema $S$ has vertices $V_S$, each with a kind, and edges $E_S$, each an ordered pair of vertex ids. It is **well formed** when every edge's endpoints are in $V_S$. For a base $B$ and branches $O$ and $T$, the **value** of a vertex id $x$ in a schema is its kind there or $\bot$ when absent, and the value of an edge is its presence. Write $b$, $o$ and $t$ for an element's values in $B$, $O$ and $T$.

A branch **uses** a vertex $x$ when it has an edge at $x$ (with $x$ as source or target) that $B$ does not have.

The **merge** $M$ takes, for each element, the value $o$ when $o = t$; the value $t$ when $o = b \neq t$; the value $o$ when $t = b \neq o$; and the value $b$, with a conflict, when $b$, $o$ and $t$ are pairwise distinct. Two exceptions make deletion and use dependent: when $o = b$ is a kind and $t = \bot$ while $O$ uses $x$, the vertex takes $b$ with the conflict `DeleteModifyVertex` deleted by `Theirs`, and symmetrically with the branches exchanged. A conflict on a vertex is classified by its values: `BothAddedVertexDifferently` when $b = \bot$, `DeleteModifyVertex` naming the branch whose value is $\bot$, and `BothModifiedVertex` otherwise. An edge's value has two possibilities, so two of $b$, $o$ and $t$ agree and an edge never conflicts. An element with $b = \bot$ and $o = t \neq \bot$ is an **identified addition**.

## The merge is the per-element merge {#L-schema-merge-1}

### Statement

Let $B$, $O$ and $T$ be well-formed schemas whose edges are unnamed edges of one kind, built by `SchemaBuilder` (or the protocol's default schema when empty). Then `three_way_merge(B, O, T)` returns a merged schema with the vertices, kinds and edges of the merge $M$, the conflicts of $M$ with their classes, vertices, kinds and deleting branch, and the identified additions of $M$, vertices first. When $M$ has no conflict, the merged schema is well formed.

### Outside the domain

Every other field of a schema (constraints, hyper-edges, required edges, NSIDs, variants, orderings, recursion points, usage modes, spans, nominal flags, coercions, mergers, defaults and policies) is merged by its own rule and is not modelled, and so are edge names, edge kinds other than one, and the conflicts those fields raise, `DeleteModifyEdge` among them. A merge with a conflict is provisional: a `BothAddedVertexDifferently` vertex is absent until resolved, so an edge a branch added at it has no endpoint in the provisional schema. [`apply_resolutions`](https://docs.rs/panproto-vcs/latest/panproto_vcs/merge/fn.apply_resolutions.html) refuses with `PushoutError::UnresolvedConflicts` while a conflict has no resolution, and with `PushoutError::MigrationNotTotal` when the chosen side deleted a vertex the other side keeps, since the cocone it checks requires both migrations to be total.

### Definitions used

The merge, its conflicts and identified additions, use and well-formedness are defined above.

### Proof

1. Production compares each base vertex with the two branches' diffs: a vertex removed by both is dropped; a vertex removed by one branch is dropped unless the other branch changed its kind or has an edge at it absent from $B$, in which case it takes $b$ with `DeleteModifyVertex` naming the remover; a vertex whose kind both changed takes the common kind or, when they differ, $b$ with `BothModifiedVertex`; a vertex one branch changed takes that branch's kind. These are the cases of $M$ with $b \neq \bot$, since a diff reports a removal exactly when $o = \bot$ (or $t = \bot$) and a kind change exactly when the kinds differ.
2. A vertex added by both branches is kept once and reported as identified when the two vertices are equal, and is a `BothAddedVertexDifferently` conflict otherwise; a vertex added by one branch is kept. These are the cases of $M$ with $b = \bot$.
3. A base edge is kept unless a branch removed it, and every edge either branch added is kept, an edge both added being reported as identified. With presence as the only value, this is $M$ on edges: $o = t$ gives $o$, and otherwise the branch that differs from $b$ decides.
4. When $M$ has no conflict, every vertex removed by a branch was used by neither branch, so no edge either branch added has it as an endpoint, and every base edge at it was removed by the branch that removed it (that branch is well formed) and left unchanged by the other, so by step 3 it is not in $M$. Every other endpoint of a kept edge is a vertex of a branch that is kept by steps 1 and 2. So the merged schema is well formed.
5. Without the use exception, a merge keeps an edge a branch added at a vertex the other branch deleted and drops the vertex, which leaves the edge without an endpoint; that is the case the exception makes a conflict. Deleting the edge with the vertex is the other construction a pushout of partial maps would give, and it discards an addition without reporting it, which the merge never does for a vertex either.
6. The differential compares the merged schema, the conflicts in their listed order and the identified additions with the reference on every triple of the 73 schemas over the ids `a` and `b`, the kinds `object` and `string` and any set of edges (389,017 merges, of which 86,736 have a conflict and 26,440 a delete-use conflict), and the property test does the same on random triples over four ids.

### Executable counterpart

- Law ID: `schema-merge.per-element-merge`
- Reference: `panproto_reference::pages::schema_merge::merge`
- Reference: `panproto_reference::pages::schema_merge::Schema::well_formed`
- Enumerator: `panproto_reference::enumerate::schema_merge::schemas; bound: ids=2`
- Exhaustive differential test: `merge_agrees_with_the_reference_on_every_triple_of_the_scope; command: cargo nextest run -p panproto-vcs --test reference_schema_merge --no-tests=fail -E 'test(=merge_agrees_with_the_reference_on_every_triple_of_the_scope)'`
- Property differential test: `randomized_merges_agree_with_the_reference; command: cargo nextest run -p panproto-vcs --test reference_schema_merge --no-tests=fail -E 'test(=randomized_merges_agree_with_the_reference)'`
- Reference test: `the_merge_rule_on_hand_computed_triples; command: cargo nextest run -p panproto-reference --test schema_merge --no-tests=fail -E 'test(=the_merge_rule_on_hand_computed_triples)'`
- File: `crates/panproto-vcs/src/merge.rs`

## Conflicts are listed in one stable order {#L-schema-merge-2}

### Statement

For schemas in the domain of [L-schema-merge-1](#L-schema-merge-1), `MergeResult::conflicts` is sorted by the derived order of `MergeConflict`: by class in the order `BothModifiedVertex`, `BothAddedVertexDifferently`, `DeleteModifyVertex`, then by vertex id. The whole result (merged schema, conflicts in order and identified additions) depends only on the values of $B$, $O$ and $T$: not on the order their vertices and edges were added, and not on the process. Exchanging $O$ and $T$ leaves the merged schema and the identified additions unchanged and exchanges the branches every conflict names. On the determinism scope the 64-bit FNV-1a digest of the canonical rendering is the constant the test pins, and equals the digest of the reference's rendering.

### Outside the domain

The order of conflicts on fields the reference does not model is the same derived order, by class in declaration order and then by field, and is checked only by the derived `Ord`; the law states it on the modelled classes. A conflict's index in that order is what `ResolutionStrategy` keys on.

### Definitions used

The canonical rendering is `render`: the merged vertices and edges as sorted maps and sets, the conflicts in the listed order and the identified additions. The determinism scope is every triple of the scope whose base has at most one vertex (47,961 merges).

### Proof

1. The merge sorts its conflicts before returning them, and `MergeConflict` derives `Ord` with its variants in the stated order and the vertex id as the first field of each vertex variant; one vertex raises at most one conflict, so the vertex id decides between two conflicts of a class.
2. The merged vertices and edges are maps and sets compared by value, and the identified additions are sorted, so no part of the result depends on the iteration order of a hash map once the conflicts are sorted.
3. Exchanging the branches exchanges $o$ and $t$, and every case of the merge's definition is symmetric in them with the deleting branch exchanged; the reference test checks this on every triple of the scope.
4. The differential renders every merge of the determinism scope with the schemas built in ascending and in descending insertion order and requires the two renderings to be equal and equal to the reference's, runs the rendering in four child processes, each with its own hash seed, and compares their digests with each other and with the pinned constant; a second differential compares each merge of the full scope with the merge of its exchanged branches, and the property test compares insertion orders and exchanged branches on random triples. A merge raising nine conflicts is checked to list them in sorted order under both insertion orders.

### Executable counterpart

- Law ID: `schema-merge.stable-order`
- Reference: `panproto_reference::pages::schema_merge::render`
- Reference: `panproto_reference::pages::schema_merge::Conflict::order_key`
- Reference: `panproto_reference::pages::schema_merge::Conflict::swapped`
- Enumerator: `panproto_reference::enumerate::schema_merge::schemas; bound: ids=2`
- Exhaustive differential test: `merges_are_the_same_in_every_process_and_insertion_order; command: cargo nextest run -p panproto-vcs --test reference_schema_merge --no-tests=fail -E 'test(=merges_are_the_same_in_every_process_and_insertion_order)'`
- Property differential test: `randomized_insertion_orders_and_swapped_branches_give_the_same_merge; command: cargo nextest run -p panproto-vcs --test reference_schema_merge --no-tests=fail -E 'test(=randomized_insertion_orders_and_swapped_branches_give_the_same_merge)'`
- Reference test: `swapping_the_branches_swaps_every_conflict; command: cargo nextest run -p panproto-reference --test schema_merge --no-tests=fail -E 'test(=swapping_the_branches_swaps_every_conflict)'`
- File: `crates/panproto-vcs/src/merge.rs`

## Staging selects the identity on what two schemas share {#L-schema-merge-3}

### Statement

Let $S$ be the schema of HEAD and $S'$ a staged schema, both in the domain of [L-schema-merge-1](#L-schema-merge-1) and with at least one vertex. Then, when no vertex of $S$ is absent from $S'$ while another of $S'$ is absent from $S$, the migration `derive_migration` selects from $S$ to $S'$ has as vertex map exactly the identity on $V_S \cap V_{S'}$ (a vertex whose kind changed included) and as edge map exactly the identity on $E_S \cap E_{S'}$. Otherwise its vertex map is injective and maps every vertex of $V_S \cap V_{S'}$ whose kind is unchanged to itself. `Repository::add` refuses $S' = S$ with `VcsError::ValidationFailed` and the single reason `no changes detected`; for a first commit it records no migration; otherwise it stores this migration as a `Migration` object whose source and target are the object ids of $S$ and $S'$ stored as `FlatSchema`.

### Outside the domain

A change that both removes and adds a vertex may select a rename found by the span search of [Migration search](migration-search.md); which rename is selected is not stated here, only that survivors of unchanged kind keep their identity and the map stays injective. A survivor whose kind changed may be sent elsewhere in that case, since the identity on it preserves no kind: the property test found the change from `c: object, d: string` to `b: object, c: string` (with edges at `b`) selecting `c ↦ b` and `d ↦ c`. Named edges, whose images are searched by name when an edge changes, are not modelled.

### Definitions used

`surviving_identity` is the pair of identity maps on the shared vertices and edges; `may_rename` holds when a vertex is removed and another added.

### Proof

1. `derive_migration` maps each vertex of $S$ the diff does not report removed to itself, which is $V_S \cap V_{S'}$, and each edge of $S$ that survives with both endpoints to itself; an unnamed edge absent from $S'$ has no same-named edge between its endpoints in $S'$, so it is not mapped.
2. Only when the diff reports both a removed and an added vertex does it run the span search, which is injective on vertices and in which every vertex both schemas hold with one kind is pinned to itself, so its leg maps each of them to itself or drops it; it adopts the leg only when the leg maps more vertices than the identity and keeps every pinned vertex.
3. Staging diffs $S'$ against HEAD's schema, refuses an empty diff, stores both schemas as `FlatSchema` objects and the migration with their ids as its endpoints; with no HEAD it stages no migration.
4. The differential compares the selected maps with the reference on every ordered pair of the scope (5,329 pairs, of which 32 may rename); a second test stages 25 pairs in fresh repositories and reads the stored migration back; the property test covers random pairs over four ids.

### Executable counterpart

- Law ID: `schema-merge.migration-selection`
- Reference: `panproto_reference::pages::schema_merge::surviving_identity`
- Reference: `panproto_reference::pages::schema_merge::may_rename`
- Enumerator: `panproto_reference::enumerate::schema_merge::schemas; bound: ids=2`
- Exhaustive differential test: `derived_migrations_select_the_identity_on_what_the_pair_shares; command: cargo nextest run -p panproto-vcs --test reference_schema_merge --no-tests=fail -E 'test(=derived_migrations_select_the_identity_on_what_the_pair_shares)'`
- Property differential test: `randomized_derived_migrations_keep_every_survivor; command: cargo nextest run -p panproto-vcs --test reference_schema_merge --no-tests=fail -E 'test(=randomized_derived_migrations_keep_every_survivor)'`
- Reference test: `surviving_identity_on_hand_computed_pairs; command: cargo nextest run -p panproto-reference --test schema_merge --no-tests=fail -E 'test(=surviving_identity_on_hand_computed_pairs)'`
- File: `crates/panproto-vcs/src/auto_mig.rs`
- File: `crates/panproto-vcs/src/repo.rs`

## A repository records and checks its format {#L-schema-merge-4}

### Statement

The repository format version is 1 ([`REPOSITORY_FORMAT_VERSION`](https://docs.rs/panproto-vcs/latest/panproto_vcs/constant.REPOSITORY_FORMAT_VERSION.html)): the layout of `.panproto/` with objects encoded as MessagePack and addressed by their BLAKE3 identifiers. `Repository::init` writes `.panproto/format` holding the bytes `1` and a line feed, and refuses with `VcsError::AlreadyARepository` where `.panproto/` exists. `Repository::open` accepts a marker exactly when `repository_format` reads it as version 1, and otherwise refuses with `VcsError::UnsupportedRepositoryFormat` carrying the marker's text and the supported version. A repository without a marker is read as version 1; opening and reading it writes nothing, and its first write through the store writes the marker. Reading an object whose bytes do not hash to the identifier it is filed under refuses with `VcsError::ObjectCorrupted` naming both identifiers.

### Outside the domain

A `.panproto/` that is not a directory, or whose marker cannot be read for a reason other than its absence, is refused with `NotARepository` or the I/O error. Writes of `index.json`, which the repository makes without the store, do not write the marker.

### Definitions used

`repository_format` accepts an absent marker and the decimal numeral of the version followed by one line feed, and nothing else. The marker scope is every byte string of at most three bytes over `0`, `1`, `2`, line feed, space and the byte `0xff` (259 markers), with the absent marker.

### Proof

1. `FsStore::init` refuses an existing `.panproto/`, creates the layout and writes the marker before HEAD.
2. `FsStore::open` reads the marker, compares its bytes with the marker of version 1 and refuses any other content; an absent marker sets a pending flag that every writing method of the `Store` implementation clears by writing the marker first.
3. `get` recomputes the identifier of the stored bytes and refuses a mismatch.
4. The differential writes every marker of the scope into a repository and compares the outcome of opening it with the reference, removes the marker and checks that a read leaves it absent and a commit writes it, checks the refusal of a second `init`, and substitutes one stored object's bytes for another's; the property test opens repositories with random markers of up to four bytes.

### Executable counterpart

- Law ID: `schema-merge.repository-format`
- Reference: `panproto_reference::pages::schema_merge::repository_format`
- Enumerator: `panproto_reference::enumerate::schema_merge::format_markers; bound: bytes=3`
- Exhaustive differential test: `a_repository_records_and_checks_its_format_version; command: cargo nextest run -p panproto-vcs --test reference_schema_merge --no-tests=fail -E 'test(=a_repository_records_and_checks_its_format_version)'`
- Property differential test: `randomized_format_markers_are_read_as_the_reference_reads_them; command: cargo nextest run -p panproto-vcs --test reference_schema_merge --no-tests=fail -E 'test(=randomized_format_markers_are_read_as_the_reference_reads_them)'`
- Reference test: `the_format_marker_is_one_line_feed_terminated_numeral; command: cargo nextest run -p panproto-reference --test schema_merge --no-tests=fail -E 'test(=the_format_marker_is_one_line_feed_terminated_numeral)'`
- File: `crates/panproto-vcs/src/fs_store.rs`
