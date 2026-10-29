# Keys and registration

Keys connect graph structure to identity. A key declaration says how to read a tuple, which equality each part uses, and where equal tuples collide; a reference declaration names a key that can resolve such a tuple. Registration then binds those declarations to closed theories and replayable composition recipes. The eighteen laws on this page make each part of that chain explicit, from reading a key through canonical registration.

A key has an id, an ordered sequence of parts, a role, a scope, and either an empty equality vector or one equality declaration per part. An empty vector means representation equality at every part. A path part is a finite sequence of schema-edge labels. A position part names an ordered edge. Key roles are `Primary`, `Unique`, `Identifier`, `SortKey`, and `Index`; the first three are resolvable uniqueness roles, `SortKey` imposes order without uniqueness, and `Index` imposes neither. Scopes are document-wide, global, or relative to the nearest ancestor of a named vertex.

**Focused verification status.**

The listed focused tests for laws 1--4 and 7--15 have passed, but verification
on the repaired tree is still pending. The release
commands passed 1 canonical-theory test, 5 schema-declaration tests, 11
instance-key tests, 2 change-classification tests, and 2 VCS identity tests.
The finite certificates cover 37 declaration encodings, 194 decoded
extension-marker combinations, 30 declaration admission fixtures, 12
target-set declaration fixtures, 8 of them projected to the local predicate,
all 5 target-set member roles, 37 values under 12 equality
configurations, 16,428 equality pairs, 607,836 transitivity triples, 341 XML
strings, 116,281 XML pairs, 28 additional classification cases, 148 structural
graphs, 27,329,908 schemas, and 816,223,680 directed classifications. Strict
Clippy with `-D warnings` has not yet been rerun on the repaired tree.

These checks establish the focused executable counterparts. They do not close
the independent audit, mutation, frozen-candidate, or global release gates;
the audit verdicts below thus remain pending.

## Key parts read the declared value or return NULL {#L-keys-and-registration-1}

### Statement

For a graph $G$, node $n$, and key $K$, `key_value(G, n, K)` returns one value per key part exactly when every part has a value. For a path $(e_1,\ldots,e_k)$, traversal follows matching schema-edge labels in edge-id order through $e_1,\ldots,e_{k-1}$. At the last step, each frontier node independently contributes the non-null own values of its matching targets or, when that node has no matching last edge, its field named by the last edge. The first contributed value is the part value. The empty path reads $n$'s non-null own value. A position part for edge $e$ first chooses the least-id ordinary incoming $e$-edge into $n$, orders that parent's ordinary $e$-edges by `(stored position, edge id, target)`, and returns the selected edge's zero-based ordinal in that sequence. The stored position orders siblings but is not itself the returned key value. Parallel edges into the same target retain distinct identities throughout this calculation.

### Outside the domain

The result is `None` when the start node is absent, a path has no successful frontier, a selected own value is absent or null, a last-hop field is absent or null, a position part has no ordinary incoming carrier edge, or its index does not fit `i64`. One missing part makes the whole tuple `None`.

### Definitions used

An *own value* is `Presence::Present(v)` with $v$ unequal to the value variant `Null`. An *ordinary edge* has neither reserved fan kind and joins two non-fan nodes. Edge and node maps are ordered by numeric id.

### Proof

1. `path_value` handles the empty path by `own`. For a nonempty path, each preliminary hop replaces the frontier by matching targets in map order.
2. At the final label, the implementation makes the target-or-field decision separately for each frontier node and retains frontier and edge order. Thus one branch with a matching edge cannot suppress a later branch's field fallback.
3. A position part retains the id of the first qualifying ordinary incoming edge, sorts every ordinary sibling tuple `(stored position, id, target)`, and finds the tuple with that id. This keeps parallel edges distinct before converting the selected ordinal to `i64`.
4. `Iterator::collect` on `Option` returns a vector exactly when every part returns `Some`, proving tuple totality on the stated domain and NULL propagation outside it.

### Executable counterpart

- Law ID: `keys.key-declarations`
- Reference: `panproto_reference::keys::key_value`
- Evidence scope: `duplicated/twin graph witnesses over path and position parts; exact indexed elements=14, NULL elements=4`
- Production certificate: `validation_reconstructs_path_keys_in_document_and_global_scopes; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=validation_reconstructs_path_keys_in_document_and_global_scopes)'`
- Independent reference anchor: `validation_reconstructs_position_buckets; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=validation_reconstructs_position_buckets)'`
- Parallel-edge regression test: `validation_identifies_exact_parallel_position_edges; command: cargo test --release -p panproto-inst --test reference_keys validation_identifies_exact_parallel_position_edges -- --exact --nocapture`
- Regression differential test: `path_fallback_is_decided_per_last_hop_frontier_node; command: cargo test --release -p panproto-inst --test reference_keys path_fallback_is_decided_per_last_hop_frontier_node -- --exact --nocapture`
- File: `crates/panproto-inst/src/validation/state.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`
- File: `crates/panproto-reference/src/enumerate/keys_and_registration.rs`

### Mutation equivalences

Static review identified four omissions that the bounded matrix is designed to kill: dropping target sets from the structural diff, ignoring key equality, ignoring the five extended reference fields, and ignoring entry order. Production now carries each axis. The dynamic mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification remain pending; focused dynamic verification is green.

## Key buckets implement declared equality and scope {#L-keys-and-registration-2}

### Statement

For graph $G$, vertex name $v$, and key $K$, `buckets(G, v, K)` examines precisely the non-fan nodes anchored at $v$, in node-id order. A node with no key value is omitted. Under `Within(a)`, a node belongs to the scope of the least nearest ordinary ancestor anchored at $a$; a node with no such ancestor is returned in the ordered `outside` list. Document and global scopes use no ancestor id. Every remaining node belongs to the bucket indexed by its scope and the canonical bytes of its tuple under $K$'s per-part equality. The bucket retains the first tuple and all member ids in ascending order.

### Outside the domain

The operation refuses only while it keys an element: at the first node that has a complete key value and, under `Within`, an ancestor, it returns `EqualityArity` when a nonempty equality vector does not have one entry per part, and `UnsupportedCollation(name)` when that node's string part requires a named SQL collation, whose comparison is deliberately left to the database. A key whose vertex has no keyed element thus returns empty buckets whatever its equality vector, as production `KeyIndex::build` does, and a named collation over a numeric part refuses nothing. A production value constructed with an invalid numeric carrier may instead return `InvalidNumericRepresentation`; such values are outside the well-formed finite reference carrier. A keyed value without canonical bytes, namely a `Temporal` that is not well formed (for instance a fixed offset of 1440 minutes) or a value that contains one, has no representation and no declared equality, and the reference refuses it with `NoCanonicalBytes` at the first keyed node that holds it rather than placing it in a bucket. Production writes such a value with its total value encoding, so its buckets agree with its own equality there, and it does not yet refuse it. Thus equality arity, named collation, and missing canonical bytes are the three typed refusals of the reference.

### Definitions used

The *nearest ancestor* is found breadth-first by reversing ordinary edges one level at a time, excluding the start node; ties at the first matching distance go to the least node id. Tuple bytes are a length-delimited concatenation of the canonical equality key of each part, defined only on values with canonical bytes. Equal tuple bytes have equal declared-key behavior by [L-keys-and-registration-8](#L-keys-and-registration-8).

### Proof

1. The outer loop filters the ordered node map by anchor and non-fan shape, so it visits exactly the stated carrier in ascending order.
2. [L-keys-and-registration-1](#L-keys-and-registration-1) omits NULL tuples. Breadth-first reverse traversal follows only ordinary arcs, so reserved fan incidence and arcs incident on reified hyper-edge nodes cannot supply an ancestor. Sorting each next frontier selects the least matching ancestor at the nearest level.
3. The scope id and equality-key bytes form the ordered map key. Map insertion creates one stored tuple at the first member and appends every later member, preserving node-id order.
4. Equality-vector expansion and equality-key construction are the only fallible calls, and both run inside the loop after a node's key value and scope are known. Their errors are exactly the typed refusals in the outside-domain clause, raised at the first keyed node that meets them; a loop that keys no node raises none.

### Executable counterpart

- Law ID: `keys.reference-declarations`
- Reference: `panproto_reference::keys::buckets`
- Evidence scope: `duplicated/twin graph witnesses over Document, Global, and Within scopes; the two Within witnesses assert 3 buckets and 3 outside elements (2 and 2 across twin documents, 1 and 1 across fan incidence); both refusals asserted only at a keyed element`
- Production certificate: `validation_reconstructs_within_buckets_and_outside_cases; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=validation_reconstructs_within_buckets_and_outside_cases)'`
- Independent reference anchor: `validation_preserves_both_reference_keying_refusals; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=validation_preserves_both_reference_keying_refusals)'`
- Regression differential test: `within_scope_does_not_traverse_fan_incidence; command: cargo test --release -p panproto-inst --test reference_keys within_scope_does_not_traverse_fan_incidence -- --exact --nocapture`
- Canonical-bytes refusal test: `values_without_canonical_bytes_are_a_typed_keying_refusal; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=values_without_canonical_bytes_are_a_typed_keying_refusal)'`
- File: `crates/panproto-inst/src/validation/state.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`
- File: `crates/panproto-reference/src/enumerate/keys_and_registration.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification remain pending; focused dynamic verification is green.

## Canonical schema bytes separate the finite declaration carrier {#L-keys-and-registration-3}

### Statement

For schemas in the finite declaration carrier, `canonical_bytes` is injective. It writes vertices, edges, hyper-edges, constraints, required edges, orderings, entries, keys, references, and target sets in declaration order. Ordered maps and sets use key order; sequences retain stored order behind a `u64` length; each sum variant has a distinct one-byte tag; strings and schema edges use the canonical primitive layout. Each optional extension inside a reference (a nondefault variable binding, the standoff extension, and the reference envelope of guard, null match, null vertex, and scope chain) is introduced by the eight-byte little-endian marker $2^{63}$, and the trailing target-set extension by the byte `0xff`. Thus two schemas have equal bytes exactly when every represented declaration field is equal. A declaration without these extensions keeps the bytes it had before the markers were introduced.

### Outside the domain

The statement concerns the finite reference `Schema` carrier. A larger production schema field absent from that carrier is outside this function's type and is not claimed to affect these bytes. Guard values outside canonical value encoding, such as a `Temporal` that is not well formed, are represented by the reference writer's declared empty fallback and are excluded from the injectivity claim.

### Definitions used

A *self-delimiting field* has either fixed width or a length prefix. A tagged sum writes its variant tag before its payload. A code is *prefix-free* when no codeword is a proper prefix of another; a concatenation of codes, each chosen by the bytes already read and each prefix-free, is uniquely decodable. The exact declaration carrier has zero through three vertices, one vertex kind, one edge name, and at most three labelled edges. Its two key slots are absent or hold an owner/key descriptor built from five roles, the document, global, and vertex-relative scopes, and the six nonempty words of length at most two over `Path([])` and `Path([v0 -prop:e-> v0])`. These atoms do not depend on the surrounding graph.

### Proof

1. Fixed order separates top-level fields, and a count separates every variable-length collection. Map and set order is canonical; within a collection, primitive encodings and distinct tags separate keys, variants, and payloads. A fixed-width field and a length-prefixed string are prefix-free, since a reader knows each one's extent before reading its content, and a tagged sum is prefix-free when each variant's payload is.
2. An optional extension is the one place where the reader decides presence from the bytes rather than from a tag. Where an extension inside a reference is absent, the next bytes are a `u64` length (the target vertex's name after a variable or standoff source, the next reference's id after a reference), the target-set byte `0xff`, or the end of the bytes. A length is the length of an allocated Rust string, which is at most `isize::MAX` $< 2^{63}$, so it never equals the marker; the marker's first byte is `0x00`, never `0xff`; and the marker has eight bytes where the end of the bytes has none. The trailing target-set byte is followed by nothing when absent. Presence is thus decided by whether the next eight bytes are the marker, or for target sets whether any byte remains, and every field stays prefix-free. The earlier one-byte marker `0xff` failed this test, because a length whose low byte is `0xff` begins with it: the regression pair below, two valid declarations whose binding and target name differ, encoded identically under it.
3. Reading equal byte streams from the beginning thus gives equal counts and equal fields by induction over the layout. Applying this argument to every top-level field yields schema equality; determinism gives the converse.
4. There are 1, 2, 15, and 130 structural graphs at vertex counts zero through three. At positive $n$, there are $D_n=30n(n+2)$ owner/key descriptors, hence $(D_n+1)^2$ labelled two-slot declaration contexts per graph.
5. The certificate executes all 148 graphs, every descriptor in both slots, every directed one-axis transition, and every absent, same-owner, and different-owner second-key class. Because graph fields precede declaration fields and the untouched slot is encoded unchanged, equality of complete byte strings factors through equality of the executed graph and declaration components. The recorded multiplicities reconcile to $27{,}329{,}908$ schemas.

### Executable counterpart

- Law ID: `keys.canonical-schema-bytes-injective`
- Reference: `panproto_reference::schema::canonical_bytes`
- Evidence scope: `all 148 structural graphs, descriptor counts 90, 240, and 450, and 27,329,908 schemas`
- Production certificate: `canonical_declaration_factorization_reconciles_the_exact_carrier; command: cargo nextest run --release --locked -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=canonical_declaration_factorization_reconciles_the_exact_carrier)'`
- Independent reference anchor: `canonical_declaration_factorization_reconciles_the_exact_carrier; command: cargo nextest run --release --locked -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=canonical_declaration_factorization_reconciles_the_exact_carrier)'`
- Extension decoding test: `declaration_bytes_decode_back_on_every_extension_combination; command: cargo nextest run --release --locked -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=declaration_bytes_decode_back_on_every_extension_combination)'`
- Marker regression test: `extension_markers_separate_the_audit_counterexample; command: cargo nextest run --release --locked -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=extension_markers_separate_the_audit_counterexample)'`
- File: `crates/panproto-schema/src/canonical.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`

The extension decoding test inverts the declaration bytes with a decoder written from the grammar of proof step 2, over every combination of the three in-reference extensions and the target-set extension with neighbouring names whose length prefix begins with `0xff`, 194 declarations in all; decoding back proves the encoding injective there. Under the one-byte marker both new tests fail.

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification remain pending; focused dynamic verification is green.

## Change classification is the stated backward-compatibility decision {#L-keys-and-registration-4}

### Statement

Within the declaration-change carrier, `classify_change(A, B)` returns `Identical` exactly when $A=B$. Otherwise it returns `Breaking` when any of the following holds: a primary, unique, or identifier key is added to a vertex already present in $A$; an existing key changes parts, equality, or role, widens its scope, or changes between distinct scopes of the same rank; a reference added to an existing holder is non-nullable or fails unresolved values; an existing reference narrows cardinality, nullability, or unresolved handling, or changes any other semantic field; a target set is new or gains a member or changes scope; or the entry sequence changes. Adding a sort key or index does not add an instance-admission obligation and is `BackwardCompatible`, as are key removals and the other nonidentical declaration changes not listed above.

### Outside the domain

The independent decision is claimed only when the structural graph fields are unchanged, apart from the explicitly tested entry sequence. Other vertex, edge, hyper-edge, constraint, required-edge, and ordering changes belong to the production classifier's larger domain. The finite reference function remains total on its data type, but its result outside this declaration-change carrier is not an oracle for production behavior.

### Definitions used

The five roles make different uniqueness, identity, ordering, and resolution promises. Thus every role-to-role change fails closed as breaking. Scope ranks are 3 for global, 2 for document, and 1 for within; distinct scopes at one rank count as a tightening in either direction. A directed *one-axis transition* adds or removes one descriptor, or changes only its role, scope, or part word.

### Proof

1. Structural equality is tested first and gives exactly `Identical`. `key_is_tightened` is the disjunction of changed parts, equality, role, greater scope rank, and unequal same-rank scopes; `reference_is_tightened` is the corresponding disjunction over every reference field.
2. Ordered-map scans detect added and changed declarations exactly as stated. An addition on an old vertex is breaking exactly for the three uniqueness-bearing roles; sort keys impose only an advisory order check, and indexes promise no uniqueness or resolution. The separate target-set predicate detects a changed scope or any new member. The final entry comparison refuses an entry-sequence change.
3. Each descriptor contributes one add, one remove, four role changes, $n+1$ scope changes, and five part changes, giving $D_n(n+12)$ directed transitions. The second slot is either absent, owned by the changed key's vertex, or owned by another vertex. Since its declaration is identical before and after, classification is congruent under that untouched context; the same argument applies to the unchanged structural graph.
4. The differential executes every graph, descriptor transition, slot, and nonempty second-key context class. The certificate's multiplicities sum to
   $$\sum_n \mathrm{graphs}_n\,2D_n(D_n+1)(n+12)=816{,}223{,}680,$$
   which is the full labelled classification carrier.
5. A separate 28-case same-input matrix holds the structural context fixed while changing equality, reference addition and removal, each reference semantic field, target-set membership and scope, or the entry sequence. It checks both directed relaxation pairs and the explicit entry refusal against production.

### Executable counterpart

- Law ID: `keys.change-classification`
- Reference: `panproto_reference::schema::classify_change`
- Evidence scope: `816,223,680 key classifications and 28 additional non-key cases`
- Production certificate: `every_o17_key_change_classifies_through_the_factorization_certificate; command: cargo nextest run --release --locked -p panproto-check --test reference_classify --no-tests=fail -E 'test(=every_o17_key_change_classifies_through_the_factorization_certificate)'`
- Independent reference anchor: `non_key_classification_axes_match_the_reference; command: cargo nextest run --release --locked -p panproto-check --test reference_classify --no-tests=fail -E 'test(=non_key_classification_axes_match_the_reference)'`
- File: `crates/panproto-check/src/diff.rs`
- File: `crates/panproto-check/src/classify.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`
- File: `crates/panproto-reference/src/enumerate/keys_and_registration.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification remain pending; focused dynamic verification is green.

## Each shape theory is closed and composes with its schema theories {#L-keys-and-registration-5}

### Statement

The instance theories for tree, graph, table, and flat shapes typecheck without undeclared symbols. Composing each instance theory with every schema theory in its registration group over the declared interface reports every `InclusionIsMorphism` and `UniversalIdentity` check as passed, with no failed, incomplete, or inapplicable record.

### Outside the domain

Caller-supplied open theories remain outside this claim. Composition may report `Inapplicable { OpenTheory }` for such a theory. This law concerns the closed in-tree theories used by protocol registration.

### Definitions used

A theory is *closed* when every sort and operation used by its declarations and equations is declared in that theory. The tree and graph interfaces contain `Vertex`, `Edge`, `src`, and `tgt`; the table and flat interfaces contain `Vertex`. A *total composition report* contains applicable passed records for both inclusion-morphism and universal-identity checks.

### Proof

1. The W-type, graph-instance, functor, and flat theories each declare the interface symbols they use. Their coherence equations thus typecheck in the theory that contains them.
2. Each registration group builds one explicit-identity diagram over its stated interface. Symbols outside the interface are rejected on a name clash rather than silently identified.
3. The composition checker verifies every inclusion and the identity mediator. The per-shape tests require passed records for both checks, a passed report status, and a passed record (`Passed` or `PassedByEquation`) for every check kind, so no record is failed, incomplete, or inapplicable.
4. The registered-theory reference test converts every theory each protocol registers into the reference model field by field, checks that the reference's canonical bytes of the converted theory equal production's, and runs the reference signature check on it. For each protocol's schema and instance recipe it replays the recipe in the reference and composes the recipe's one diagram step in the reference, requiring a passed report with every record passed and the registered theory's canonical bytes. The generic production/reference differential separately establishes agreement of composition over its bounded span carrier.

### Executable counterpart

- Law ID: `keys.q6`
- Reference: `panproto_reference::theory::typecheck_signature`
- Reference: `panproto_reference::theory::compose`
- Evidence scope: `the tree, graph, table, and flat registration groups over their declared interfaces`
- Production certificate: `theories::tests::standalone_total_theories_pass_inclusion_and_universal_checks; command: cargo nextest run --release --locked -p panproto-protocols --lib --no-tests=fail -E 'test(=theories::tests::standalone_total_theories_pass_inclusion_and_universal_checks)'`
- Independent reference anchor: `composition_agrees_with_the_reference_on_every_span; command: cargo nextest run --release --locked -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=composition_agrees_with_the_reference_on_every_span)'`
- Registered-theory reference test: `registered_theories_agree_with_the_reference_typecheck_compose_and_recompose; command: cargo nextest run --release --locked -p panproto-protocols --test reference_registration --no-tests=fail -E 'test(=registered_theories_agree_with_the_reference_typecheck_compose_and_recompose)'`
- Signature differential test: `signature_checking_agrees_with_the_reference_on_every_signature; command: cargo nextest run --release --locked -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=signature_checking_agrees_with_the_reference_on_every_signature)'`
- File: `crates/panproto-protocols/src/theories.rs`
- File: `crates/panproto-gat/src/compose.rs`
- File: `crates/panproto-reference/src/pages/theories.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Registration records replayable recipes and preserves the compatibility surface {#L-keys-and-registration-6}

### Statement

Each built-in protocol records the exact composition recipe for its schema and instance theories. Replaying either recipe from the theories it names reproduces the registered theory's canonical bytes. The amalgamation wrappers retain the 0.74 naming and result surface, while registration uses one explicit-identity diagram with reject-on-clash semantics. The 0.74 surface is fixed by a frozen artifact: the schema and instance theories that panproto-protocols 0.74.4, built from crates.io, registered for each of its 54 protocols, stored as that build serialized them, and compared by canonical theory bytes with today's registration. All 54 schema theories keep their 0.74.4 bytes. Exactly 17 instance theories differ: the 10 whose shape declaration re-declares them, and the 7 further protocols that register through the table building block, which gained the vertex and edge sorts and the foreign-key coherence axioms of the table shape (arrow, conllu, dataframe, edi-x12, parquet, sql, and swift-mt); the other 37 keep their bytes.

### Outside the domain

A protocol introduced after 0.74 has no earlier registered theory to compare. For such a protocol, the law still requires a recorded replayable recipe and a passed composition report. Protocols whose instance theory was deliberately redeclared, or amended through the table building block, compare against that declared replacement rather than an obsolete byte string; the frozen comparison names exactly which ones.

### Definitions used

A *recorded recipe* is the versioned `CompositionSpec` stored on the protocol. A `Diagram` step records its nodes, edges, composition mode, clash policy, rewrite budget, and inline theories. *Compatibility surface* means the 0.74 wrapper names and amalgamated result after the documented Q.6 building-block amendments.

### Proof

1. Each registration helper constructs one diagram and attaches the corresponding schema and instance recipes to the returned protocol.
2. `recompose` resolves recorded nodes and inline theories, applies each stored option, and returns the recipe's named result. The registration test compares its canonical bytes with the registered theory.
3. The equivalence test reconstructs the 0.74 amalgamation and compares canonical theory bytes, subject only to the enumerated new-protocol and redeclared-instance cases.
4. The generic recipe differential compares production and reference replay over every bounded recipe, including their typed refusals. These checks separate recipe correctness from the compatibility comparison.

### Executable counterpart

- Law ID: `keys.q7`
- Reference: `panproto_reference::theory::recompose`
- Evidence scope: `every registered schema and instance recipe, plus the bounded reference recipe carrier`
- Production certificate: `recompose_of_every_registration_spec_reproduces_its_theory; command: cargo nextest run --release --locked -p panproto-protocols --test registration_equivalence --no-tests=fail -E 'test(=recompose_of_every_registration_spec_reproduces_its_theory)'`
- Independent reference anchor: `recipe_replay_agrees_with_the_reference_on_every_recipe; command: cargo nextest run --release --locked -p panproto-gat --test reference_theories --no-tests=fail -E 'test(=recipe_replay_agrees_with_the_reference_on_every_recipe)'`
- Compatibility surface test: `registration_equivalence; command: cargo nextest run --release --locked -p panproto-protocols --test registration_equivalence --no-tests=fail -E 'test(=registration_equivalence)'`
- Frozen 0.74.4 test: `registration_reproduces_the_frozen_0_74_4_theories; command: cargo nextest run --release --locked -p panproto-protocols --test registration_equivalence --no-tests=fail -E 'test(=registration_reproduces_the_frozen_0_74_4_theories)'`
- Diagram form test: `every_registered_protocol_records_a_diagram_step; command: cargo nextest run --release --locked -p panproto-protocols --test registration_equivalence --no-tests=fail -E 'test(=every_registered_protocol_records_a_diagram_step)'`
- File: `crates/panproto-protocols/src/theories.rs`
- File: `crates/panproto-protocols/tests/registration_equivalence.rs`
- File: `crates/panproto-reference/src/pages/theories.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Canonical theory and composition identities are deterministic {#L-keys-and-registration-7}

### Statement

Canonical theory bytes encode the complete theory in declared field order with name-keyed collections sorted. Canonical composition bytes additionally encode inclusions in node order, sorted provenance maps, and the complete report. Their domain-separated digests are functions of those bytes. The frozen corpus agrees across its named MessagePack, CBOR, and JSON forms, and composing every registration diagram yields identical canonical bytes in independent processes.

### Outside the domain

The law concerns canonical identities, not the incidental bytes of Rust's ordinary `Serialize` implementation. It does not claim that different valid theories have different cryptographic digests; it requires equal canonical content to produce equal bytes and digests.

### Definitions used

`THEORY_CONTEXT` and `COMPOSITION_CONTEXT` are distinct digest domains. A *frozen fixture* records a diagram, its options, the expected composition in three boundary encodings, and the expected composition digest. The cross-process carrier contains every registration diagram and uses independent hash-map seeds.

### Proof

1. Both canonical writers use exhaustive variant tags, length prefixes, declared field order, and sorted traversal for map-like collections. Thus allocation and hash-map iteration order do not enter either byte string.
2. Each digest hashes the corresponding canonical bytes under its fixed context. Equal byte strings thus produce equal identities without conflating theory and composition domains.
3. A bounded same-input differential independently encodes empty and unary theories plus one carrier-complete theory. The third fixture visits every sort kind, value kind, coercion class, expression, literal, pattern, conflict strategy, optional directed-equation field, and duplicate-name tie order. Its composition carrier adds a nonempty inclusion with both operation-assignment forms, an entry in every provenance map, an added equation, one freshening, every report-check tag, every subject form, and every outcome family. Wrapping all three theories in both composition modes also checks the inline theory and consecutive provenance-map layout.
4. The fixture test recomposes each recorded diagram, compares its canonical composition bytes with the decoded fixture, and compares the resulting digest with `golden.json`.
5. The cross-process test repeats every registration composition in 32 child processes and compares the complete byte maps. This directly checks the process-dependent source of nondeterminism that a single-process test cannot observe.

### Executable counterpart

- Law ID: `keys.q8`
- Reference: `panproto_reference::theory::canonical_theory_bytes`
- Reference: `panproto_reference::theory::canonical_composition_bytes`
- Evidence scope: `three bounded same-input theories under both composition modes, including one carrier-complete fixture; the frozen theory-composition corpus; every registered composition in 32 child processes`
- Production certificate: `canonical_theory_and_composition_bytes_match_reference_on_bounded_fixtures; command: cargo nextest run --release --locked -p panproto-schema --test reference_theory_canonical --no-tests=fail -E 'test(=canonical_theory_and_composition_bytes_match_reference_on_bounded_fixtures)'`
- Independent reference anchor: `cross_process::compose_bytes_are_identical_across_processes; command: cargo nextest run --release --locked -p panproto-protocols --test registration_equivalence --no-tests=fail -E 'test(=cross_process::compose_bytes_are_identical_across_processes)'`
- Frozen-corpus regression test: `theory_composition_fixtures_recompose_to_their_bytes_and_digests; command: cargo test --release -p panproto-protocols --test registration_equivalence theory_composition_fixtures_recompose_to_their_bytes_and_digests -- --exact --nocapture`
- File: `crates/panproto-protocols/src/theories.rs`
- File: `crates/panproto-schema/src/canonical_theory.rs`
- File: `crates/panproto-protocols/tests/registration_equivalence.rs`
- File: `crates/panproto-reference/src/pages/theories.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification remain pending; focused dynamic verification is green.

## Declared key equality is exactly its canonical kernel {#L-keys-and-registration-8}

### Statement

`key_equivalence(E, x, y)` has the following kernel. `Representation` uses canonical value representation equality. `NumericValue` compares every numeric variant by exact mathematical value, with negative infinity below finite values, positive infinity above them, and all NaNs in one equality class; when either value is nonnumeric it falls back to representation equality. `NumericValueWithinKind(p)` additionally requires the selected primitive to agree, where `Any` selects decimal for integers and decimals and binary64 for floats. SQL equality uses exact numeric equality for numeric pairs, declared collation and padding for string pairs, representation equality for other nonnumeric pairs, and false for a mixed numeric-nonnumeric pair. XML non-CDATA equality normalizes string pairs as in [L-keys-and-registration-14](#L-keys-and-registration-14) and otherwise uses representation equality.

### Outside the domain

Under a named SQL collation, every pair that holds text on either side (text against text, null, a number, or any other value) returns `None` in the reference and `UnsupportedCollation` in production: the key function of that text belongs to the database implementation and is undefined here, so not even inequality with a non-text value is decided. A malformed production numeric carrier may return `InvalidNumericRepresentation`; it has no value in the finite reference carrier. Every other enumerated input returns `Some(true)` or `Some(false)` in the reference and the corresponding `Ok` value in production.

### Definitions used

Finite numerics are normalized to a sign, a base-ten natural significand without trailing zeroes, and an adjusted exponent. Binary floats enter this carrier exactly when their denominator is a power of two, which always holds for finite IEEE values. SQL `PadSpace` removes trailing U+0020 before collation; `AsciiCaseInsensitive` folds ASCII uppercase letters only.

### Proof

1. The five outer match arms are exactly the five equality declarations.
2. Numeric normalization maps integers, decimals, and finite floats to one exact decimal value; special values map to three distinguished keys. Equality of these keys is thus the stated mathematical kernel.
3. The within-kind arm prefixes that key with the declared or selected primitive. The SQL and XML arms perform their stated string transformations and use the same fallback for other values.
4. `sql_text` refuses exactly a named collation. Numeric normalization separately propagates `InvalidNumericRepresentation` for an ill-formed production carrier; no other branch is partial.

### Executable counterpart

- Law ID: `keys.equality-kernel-equivalence`
- Reference: `panproto_reference::keys::key_equivalence`
- Evidence scope: `37 values under 12 configurations, 16,428 ordered pairs, and 607,836 ordered triples`
- Production certificate: `equality_carrier_executes_all_pairs_and_triples; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=equality_carrier_executes_all_pairs_and_triples)'`
- Independent reference anchor: `equality_carrier_executes_all_pairs_and_triples; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=equality_carrier_executes_all_pairs_and_triples)'`
- File: `crates/panproto-inst/src/key.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`
- File: `crates/panproto-reference/src/enumerate/keys_and_registration.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification remain pending; focused dynamic verification is green.

## Representation equality refines every computable declared equality {#L-keys-and-registration-9}

### Statement

For every equality declaration $E$ and values $x,y$ on which $E$ is computable, if $x$ and $y$ are equal by representation then they are equivalent under $E$. `representation_refines(E, x, y)` returns the truth value of this implication. The reference returns `Some(true)` when representation equality is false or both equalities hold, `Some(false)` only if representation equality holds and declared equality does not, and `None` precisely when the consequent requires an unsupported named SQL collation or either value has no canonical bytes (a `Temporal` that is not well formed, or a value that contains one). Production returns the corresponding `Ok` value or typed refusal.

### Outside the domain

A representationally equal string pair under a named SQL collation returns `None` in the reference and `UnsupportedCollation` in production; the function does not assume a database-specific kernel. An invalid production numeric carrier may additionally return `InvalidNumericRepresentation` and is outside the finite reference carrier. A value without canonical bytes has no representation equality in the reference, which returns `None` for it; production compares such a value by the bytes it writes, so the two are compared only on values with canonical bytes.

### Definitions used

For propositions $P$ and $Q$, implication $P \Rightarrow Q$ is $\neg P \lor Q$. Representation equality and declared equality are those of the value layer and [L-keys-and-registration-8](#L-keys-and-registration-8).

### Proof

1. The function first evaluates representation equality, which is undefined, and returned as `None`, on a value without canonical bytes.
2. It returns `!represented || key_equivalence(E, x, y)?`, which is exactly the Boolean implication.
3. If the antecedent is false, short-circuit evaluation returns true without demanding a declared comparison. If it is true, the result and any refusal are exactly those of the declared equality.

### Executable counterpart

- Law ID: `keys.representation-refinement`
- Reference: `panproto_reference::keys::representation_refines`
- Evidence scope: `37 values under 12 equality configurations, for 16,428 ordered pairs`
- Production certificate: `equality_carrier_executes_all_pairs_and_triples; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=equality_carrier_executes_all_pairs_and_triples)'`
- Independent reference anchor: `named_collation_has_the_exact_typed_refusal_boundary; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=named_collation_has_the_exact_typed_refusal_boundary)'`
- File: `crates/panproto-inst/src/key.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`
- File: `crates/panproto-reference/src/enumerate/keys_and_registration.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification remain pending; focused dynamic verification is green.

## Equality declarations have an injective canonical byte layout {#L-keys-and-registration-10}

### Statement

Within canonical schema bytes, a key with an empty equality vector retains the legacy role tag 0 through 4 and writes no equality payload. A nonempty vector sets the high bit of that role tag, writes its length, and writes each declaration in order. Equality tags are 0 for representation, 1 for numeric value, 2 for numeric value within kind, 3 for SQL, and 4 for XML non-CDATA. Nested primitive, collation, and padding variants have distinct tags and include their width or name payload. This encoding is injective over key roles, equality-vector presence, and equality declarations.

### Outside the domain

A nonempty equality vector of the wrong arity remains encodable, but declaration validation rejects it. The injectivity statement concerns bytes, not declaration validity.

### Definitions used

The legacy role tags are 0 for primary, 1 for unique, 2 for identifier, 3 for sort key, and 4 for index. The high bit is `0x80`; none of the legacy tags sets it. Vectors are length-prefixed and preserve element order.

### Proof

1. The high bit separates empty from nonempty equality vectors without colliding with a role tag, while the low bits retain the role.
2. Distinct equality variants have distinct tags. Every nested sum also has distinct tags, and scalar payload encodings are injective.
3. The vector length and sequential element encodings distinguish length, order, and contents. Hence equal bytes imply equal role and equality declaration data.

### Executable counterpart

- Law ID: `keys.equality-declaration-byte-injectivity`
- Reference: `panproto_reference::schema::canonical_bytes`
- Evidence scope: `10 equality forms: legacy empty, representation, numeric value, decimal/float/any within-kind, binary/ascii-folded/named SQL, and XML non-CDATA`
- Production certificate: `extended_declaration_bytes_match_reference_on_every_sum_variant; command: cargo nextest run --release --locked -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=extended_declaration_bytes_match_reference_on_every_sum_variant)'`
- Independent reference anchor: `extended_declaration_bytes_match_reference_on_every_sum_variant; command: cargo nextest run --release --locked -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=extended_declaration_bytes_match_reference_on_every_sum_variant)'`
- File: `crates/panproto-schema/src/canonical.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`
- File: `crates/panproto-reference/src/enumerate/keys_and_registration.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification remain pending; focused dynamic verification is green.

## Sort-key comparison is a total preorder on each declared family {#L-keys-and-registration-11}

### Statement

`sort_order(x, y)` is defined for two non-NaN numeric values, two strings, or two booleans. Numeric values are ordered by exact value with negative infinity first and positive infinity last; equal exact values across numeric representations compare equal. Strings use lexicographic UTF-8 byte order, and booleans use `false < true`. On each of these three families the relation $x \preceq y$ defined by a non-greater result is reflexive, transitive, and total. Its induced numeric equivalence classes are exact-value classes, so the numeric relation is a total preorder rather than representation equality.

### Outside the domain

Comparison returns `None` when either numeric value is NaN, when values come from different families, or when a nonnumeric value is neither a string nor a boolean pair. An invalid production numeric carrier returns `InvalidNumericRepresentation` rather than an ordering and is outside the finite reference carrier.

### Definitions used

A total preorder is reflexive, transitive, and total; antisymmetry is not required. The exact numeric key is the normalized key of [L-keys-and-registration-8](#L-keys-and-registration-8).

### Proof

1. Numeric-key matching places negative infinity, finite exact decimals, and positive infinity in a lexicographic three-part order and removes NaN. This is a total order on normalized keys.
2. Pulling that order back along normalization gives a total preorder on numeric values; two representations of one exact value may be equivalent.
3. UTF-8 byte strings and booleans each carry their standard total order. The match arms return precisely those orders.
4. Every cross-family and unsupported case reaches `None`, proving the domain boundary.

### Executable counterpart

- Law ID: `keys.sort-key-total-preorder`
- Reference: `panproto_reference::keys::sort_order`
- Evidence scope: `the seven values {0, 1, 1.5, "a", "ab", "b", NaN}; exact ordered pairs=49, triples=343`
- Production certificate: `sort_preorder_and_adjacent_descent_match_the_reference; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=sort_preorder_and_adjacent_descent_match_the_reference)'`
- Independent reference anchor: `sort_preorder_and_adjacent_descent_match_the_reference; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=sort_preorder_and_adjacent_descent_match_the_reference)'`
- File: `crates/panproto-inst/src/key.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`
- File: `crates/panproto-reference/src/enumerate/keys_and_registration.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification remain pending; focused dynamic verification is green.

## A sort-key sequence is ordered exactly when no adjacent pair descends {#L-keys-and-registration-12}

### Statement

For values $(x_1,\ldots,x_n)$, the reference `key_order` returns `Some(true)` exactly when `sort_order(x_i, x_{i+1})` is defined and not greater for every adjacent pair. It returns `Some(false)` exactly when every adjacent comparison is defined and at least one adjacent pair descends, and `None` when any adjacent pair is incomparable, even if an earlier pair descends. Production returns the corresponding `Ok(Some(_))` or `Ok(None)` value. Empty and singleton sequences are ordered.

### Outside the domain

There is no separate refusal value on the well-formed reference carrier beyond `None` for an incomparable adjacent pair. An invalid production numeric carrier instead returns `InvalidNumericRepresentation`. Comparability need not be checked between nonadjacent elements because transitivity from [L-keys-and-registration-11](#L-keys-and-registration-11) carries adjacent order across the sequence.

### Definitions used

An *adjacent descent* is an index $i$ with `sort_order(x_i, x_{i+1}) = Greater`. Rust's `windows(2)` is empty when the sequence has fewer than two elements.

### Proof

1. The function maps every adjacent window to `Some(!order.is_gt())` or propagates `None` from `sort_order`.
2. Its fold starts at true and conjoins every defined Boolean. Conjunction is true exactly when no element is false and false when at least one descent occurs.
3. `Option` propagation makes any incomparable pair return `None`. The empty fold returns its true identity.

### Executable counterpart

- Law ID: `keys.sort-key-adjacent-descents`
- Reference: `panproto_reference::keys::key_order`
- Evidence scope: `all 2,801 sequences of length at most four over {0, 1, 1.5, "a", "ab", "b", NaN}`
- Production certificate: `sort_preorder_and_adjacent_descent_match_the_reference; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=sort_preorder_and_adjacent_descent_match_the_reference)'`
- Independent reference anchor: `sort_preorder_and_adjacent_descent_match_the_reference; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=sort_preorder_and_adjacent_descent_match_the_reference)'`
- File: `crates/panproto-inst/src/key.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`
- File: `crates/panproto-reference/src/enumerate/keys_and_registration.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification remain pending; focused dynamic verification is green.

## Target sets are nonempty, distinct, and signature-compatible {#L-keys-and-registration-13}

### Statement

For a sequence of target-set members `(vertex, key, arity, equality)`, `target_set` returns true exactly when the sequence is nonempty, no `(vertex, key)` pair repeats, the first member's equality vector has one entry per part, and every member has the first member's arity and equality vector. The schema-level `validate_declarations` operation does not call this helper. It instead requires (i) an id distinct from every vertex id, (ii) at least one member, and (iii) member keys that exist, do not repeat, have the declared scope, have a well-formed equality vector (empty, or one entry per part), have a role other than `Index`, and share the first valid member's effective signature. Production and the reference flag the same members.

### Outside the domain

The Boolean helper returns false for an empty or incompatible sequence. Schema validation records `TargetSetNameClash` for an id collision and `EmptyTargetSet` for an empty sequence. It records one `TargetSetMemberMismatch` for each absent, repeated, `Index`-role, scope-incompatible, malformed-equality, arity-incompatible, or equality-incompatible member, in target-set and member declaration order. A member key with one part and two equality entries thus yields `EqualityArity` from the key pass and `TargetSetMemberMismatch` from the set pass.

### Definitions used

The *signature* of a member key is its part count and effective equality vector. Its effective vector is the declared nonempty vector, or one `Representation` entry per part when the declaration is empty; a declared vector of the wrong length gives the key no signature. An `Index` key promises no uniqueness and resolves no pair, so it is never a member of a key over several vertices.

### Proof

1. The helper obtains the first signature with `first`; absence returns false. A set cardinality comparison gives pairwise distinctness, and an all-quantifier gives signature compatibility.
2. Schema validation walks ordered target sets. It checks the name clash before member checks, then maintains a set of member pairs and the first valid signature.
3. A member is valid precisely when lookup succeeds, its role is not `Index`, its pair is new, its scope agrees, it has a signature, and that signature agrees. Only a member valid apart from agreement anchors the signature. The negation appends `TargetSetMemberMismatch` naming the member, proving completeness and order for declared members.
4. The schema validator tests emptiness before the member loop and emits `EmptyTargetSet`, then skips the loop. Thus an empty declaration has one typed defect rather than an accidental arity or member error.

### Executable counterpart

- Law ID: `keys.target-set-declaration`
- Reference: `panproto_reference::keys::target_set`
- Evidence scope: `nine declaration fixtures covering compatible, empty, duplicate, arity-incompatible, equality-incompatible, implicit/explicit representation, absent-key, scope-mismatch, and invalid-first-member signatures; eight fixtures have resolved signatures and are also projected to the local predicate; three further fixtures for a wrong-length equality vector and an Index member in either position; every fixture compares the flagged (set, vertex, key) members of both validators`
- Production certificate: `target_sets_match_on_the_complete_local_predicate; command: cargo nextest run --release --locked -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=target_sets_match_on_the_complete_local_predicate)'`
- Independent reference anchor: `target_sets_match_on_the_complete_local_predicate; command: cargo nextest run --release --locked -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=target_sets_match_on_the_complete_local_predicate)'`
- Member condition test: `target_set_members_need_a_well_formed_equality_and_a_resolving_role; command: cargo nextest run --release --locked -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=target_set_members_need_a_well_formed_equality_and_a_resolving_role)'`
- File: `crates/panproto-schema/src/validate.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`

### Mutation equivalences

The first production counterpart was a public helper called only by this test, so it did not establish agreement with declaration admission. That helper has been removed. The differential now projects `EmptyTargetSet` and `TargetSetMemberMismatch` from the real schema validator. Dynamic mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification remain pending; focused dynamic verification is green.

## XML non-CDATA equality has one normalization kernel {#L-keys-and-registration-14}

### Statement

`xml_noncdata(s)` replaces each maximal nonempty run of U+0020 by one U+0020 and removes U+0020 from both ends. XML attribute processing has already replaced TAB, CR, and LF before this key function runs, so this kernel preserves those code points if they nevertheless occur in its direct string input. It preserves every other Unicode scalar unchanged. The function is idempotent, and two strings are equivalent under XML non-CDATA equality exactly when their normalized strings are equal.

### Outside the domain

There is none for UTF-8 strings. TAB, CR, LF, nonbreaking space, and every character other than U+0020 are ordinary content at this layer and are not collapsed.

### Definitions used

The implementation splits on U+0020, removes empty pieces, and joins the remaining pieces with one U+0020. Idempotence means $N(N(s))=N(s)$.

### Proof

1. Splitting creates one piece between successive U+0020 runs and empty pieces at a run or endpoint. Filtering empties removes every run and endpoint occurrence.
2. Joining inserts exactly one U+0020 between successive nonempty pieces and changes no character inside a piece, which gives the stated normal form.
3. A normalized output has neither endpoint U+0020 nor adjacent U+0020. Repeating the same operation thus leaves it fixed.
4. The `XmlNonCdata` string branch compares these outputs, so its kernel is equality of normal forms.

### Executable counterpart

- Law ID: `keys.xml-noncdata-equivalence`
- Reference: `panproto_reference::keys::xml_noncdata`
- Evidence scope: `the 341 strings of length at most four over {a, b, U+0020, U+0009}, all 116,281 ordered pairs, 184 normalization classes and 1,171 equivalent ordered pairs (counted independently), with LF, CR, TAB and token-order units`
- Production certificate: `xml_kernel_executes_the_exact_o17_carrier; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=xml_kernel_executes_the_exact_o17_carrier)'`
- Independent reference anchor: `xml_kernel_executes_the_exact_o17_carrier; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=xml_kernel_executes_the_exact_o17_carrier)'`
- File: `crates/panproto-inst/src/key.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`
- File: `crates/panproto-reference/src/enumerate/keys_and_registration.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification remain pending; focused dynamic verification is green.

## Group-G declarations validate and encode without collisions {#L-keys-and-registration-15}

### Statement

The complete key, target-set, and reference declaration carrier is valid exactly when the independent `validate_declarations` function returns `Ok(())`. Key ids are unique per vertex, equality arity is exact, and sort keys have a unique ordered carrier from their `Within` scope. A repeated key id contributes one duplicate defect, regardless of how many later declarations repeat it. Target sets are nonempty and satisfy the schema-level member conditions in [L-keys-and-registration-13](#L-keys-and-registration-13). A reference whose resolving key, or any member key of its target set, has role `SortKey` or `Index` is refused with `KeyNotResolvable`, under either `on_unresolved` policy; in this release no reference resolves through an `Index` key. Every reference names a present target of matching arity and a compatible edge. A reference with no explicit target key selects the first declared `Primary` key; a key-value reference with neither resolves through the empty key, so a nonempty source is refused as an arity mismatch against zero parts. A target-set virtual edge starts at the reference holder and expands to a declared edge for every member. Inline references redirect through a target set. Variable binders name identifier keys of the target vertex itself and use one binding mode. A target set is no vertex and declares no key, so a variable reference to a set names no binder and is refused as an unknown target, and a variable reference whose named `target_key` is absent is refused the same way although its binder would resolve it. For text constraints, standoff lengths are not inclusive; fragmented standoffs are many-valued over an ordered holder edge; guarded partitions have distinct representation values; referenced text and scope-via chains are typed and acyclic; and target-set references carry no redundant per-vertex key. A referenced-text declaration is invalid when any reachable suffix repeats a reference, including when it enters a cycle that does not contain the starting declaration. The production validator agrees on this declaration-local predicate and additionally checks that named vertices, paths, positional carriers, scope ancestors, and protocol-required edges exist in the production schema. Each validator returns every defect in its domain in deterministic declaration order and with the stated multiplicity. A reference to a target set whose only member key is missing yields `TargetSetMemberMismatch` followed by `UnknownTarget`; source arity is not inspected without a resolvable target. Canonical schema bytes distinguish every valid declaration in this carrier.

### Outside the domain

An invalid carrier returns the nonempty ordered defect vector. Named SQL collation behavior at instance level is outside declaration validity; its name remains part of canonical bytes. Production fields absent from the finite reference carrier are outside the injectivity claim.

### Definitions used

The independent declaration errors are `EqualityArity`, `DuplicateKeyId`, `TargetSetNameClash`, `EmptyTargetSet`, `TargetSetMemberMismatch`, `SortKeyNotOrdered`, `KeyNotResolvable`, `UnknownTarget`, `ReferenceArity`, `InlineRequiresRedirect`, `InlineRequiresTargetSet`, `LengthInclusive`, `InvalidFragments`, `DuplicateGuard`, `EdgeMismatch`, `TargetSetHasKey`, `VariableBinderInvalid`, `BinderModeConflict`, `TextReferenceInvalid`, and `ScopeViaInvalid`. Production renames `ReferenceArity` to `ArityMismatch`; splits `VariableBinderInvalid` into `UnknownTargetKey` and `BinderNotIdentifier`; and adds `UnknownVertex`, `PathDoesNotResolve`, `PositionNotOrdered`, `EdgeRequired`, and `WithinNotAncestor` for the graph-aware checks. These substitutions yield the 26 public `KeyRefError` variants. Representation equality determines duplicate guard values.

### Proof

1. `validate_declarations` appends results from key, target-set, reference, binder-mode, and guard passes in that order and returns success exactly when the vector is empty.
2. Each pass traverses ordered maps and stored sequences and appends the defect corresponding to the negation of one condition in the Statement. A repeated-id set limits duplicate diagnostics to one per id. Reference validation independently exercises all five key roles, checks the virtual edge's holder before its member expansions, and selects the first `Primary` key when no target key is named. It returns after an unknown target, since later target-dependent checks would have no typed object to inspect.
3. The text-reference helper carries the active suffix through recursion, so every reachable repeated node is rejected. The scope-via helper similarly follows its named chain with a visited set. Thus both accept exactly the typed acyclic chains described above.
4. Canonical writing gives every declaration sum a distinct tag, every collection a length, every map a canonical order, and every optional extension a marker that the bytes following its absence cannot begin with. The prefix-freeness argument of [L-keys-and-registration-3](#L-keys-and-registration-3) thus applies to this declaration subcarrier.

### Executable counterpart

- Law ID: `keys.group-g-declaration-injectivity`
- Reference: `panproto_reference::schema::canonical_bytes`
- Evidence scope: `37 canonical fixtures covering every declaration sum variant, including Position, and nondefault extension; 30 validation fixtures covering success, every independent refusal class, exact duplicate multiplicity, implicit-primary selection, missing-only target sets, holder-source agreement, reachable text cycles, a variable over a target set, a variable with a missing named key, and key-value parts against the empty key; plus all five target-set member roles; plus the extension-marker regression pair and the 194 decoded extension combinations of L-keys-and-registration-3`
- Production certificate: `extended_declaration_bytes_match_reference_on_every_sum_variant; command: cargo nextest run --release --locked -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=extended_declaration_bytes_match_reference_on_every_sum_variant)'`
- Independent reference anchor: `declaration_validation_matches_reference_on_every_common_refusal; command: cargo nextest run --release --locked -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=declaration_validation_matches_reference_on_every_common_refusal)'`
- Role differential test: `target_set_references_check_every_member_key_role; command: cargo test --release -p panproto-schema --test reference_keys target_set_references_check_every_member_key_role -- --exact --nocapture`
- Marker regression test: `extension_markers_separate_the_audit_counterexample; command: cargo nextest run --release --locked -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=extension_markers_separate_the_audit_counterexample)'`
- Extension decoding test: `declaration_bytes_decode_back_on_every_extension_combination; command: cargo nextest run --release --locked -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=declaration_bytes_decode_back_on_every_extension_combination)'`
- File: `crates/panproto-schema/src/canonical.rs`
- File: `crates/panproto-schema/src/keys.rs`
- File: `crates/panproto-schema/src/validate.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`
- File: `crates/panproto-reference/src/enumerate/keys_and_registration.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification remain pending; focused dynamic verification is green.

## Index keys never resolve references, and null-match mode determines occurrences {#L-keys-and-registration-16}

### Statement

A reference whose resolving key has role `Index` is refused at declaration with `KeyNotResolvable` under both `on_unresolved` policies, so no schema that resolution accepts holds one and no pair is derived through an `Index` key. For a composite referencing tuple, `NullMatch::Simple` creates a resolution occurrence exactly when every part is non-NULL, while `NullMatch::Full` creates one exactly when at least one part is non-NULL. Thus an all-NULL tuple is absent under both modes; a partly NULL tuple is absent under `Simple` and unresolved under `Full`; and an all-present tuple is resolved or dangling according to the declared target key.

### Outside the domain

The law does not assert uniqueness for an `Index` key. The well-formed `Complement` reference through an `Index` key, whose get adds no edge, is not part of this release; such a declaration is the typed refusal above. The law also does not implement `MATCH PARTIAL`, which is a typed protocol refusal before resolution.

### Definitions used

A *resolution occurrence* is a holder element together with the tuple read from the reference's declared source parts. An absent occurrence produces neither a pair nor a dangling record. An unresolved occurrence produces a dangling record containing the complete tuple, including its NULL parts.

### Proof

1. Declaration validation refuses every reference whose resolving key has role `Index` ([L-keys-and-registration-15](#L-keys-and-registration-15)), whatever its `on_unresolved` policy, and resolution compiles only declarations that pass it. Thus no value equality, including equality of two absent index fields, can derive a pair through that role.
2. Under `Simple`, the occurrence filter is conjunction over the tuple's non-NULL tests. Under `Full`, it is disjunction. These formulas give the three null cases in the Statement.
3. The filtered occurrences enter the ordinary target lookup. An equal all-present tuple derives one pair; an unequal all-present tuple is dangling. A partly NULL tuple enters that lookup only under `Full`, where it is necessarily dangling against an all-present target.
4. The production/reference differential observes pairs and complement records through the complete `TreeToGraph` resolution operation. It does not infer these outcomes from a separate Boolean helper.

### Executable counterpart

- Law ID: `keys.index-and-null-match-declarations`
- Reference: `panproto_reference::resolution::get`
- Evidence scope: `five two-part tuples (equal, unequal, all NULL, and both partial-NULL orientations) under Simple and Full; all 16 optional-index pairs for the Index refusal`
- Production certificate: `composite_foreign_key_null_matching_uses_real_resolution_occurrences; command: cargo nextest run --release --locked -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=composite_foreign_key_null_matching_uses_real_resolution_occurrences)'`
- Independent reference anchor: `index_fields_never_resolve_targets; command: cargo nextest run --release --locked -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=index_fields_never_resolve_targets)'`
- File: `crates/panproto-io/src/resolution/compile.rs`
- File: `crates/panproto-io/src/resolution/occur.rs`
- File: `crates/panproto-reference/src/pages/resolution.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Lexical-space validation preserves the literal and reports ill-typed values as advisory {#L-keys-and-registration-17}

### Statement

Let $s$ be a stored literal lexeme and let $d$ be its declared datatype. If the datatype table assigns $d$ a kind, lexical-space validation reports a `ValueConstraint` diagnostic exactly when $s$ does not coerce under that kind. If the table does not assign $d$ a kind, validation reports `UnsupportedDeclaration`. Both findings are advisory: they preserve $s$ and do not make an otherwise valid graph fail admission.

### Outside the domain

Syntax errors remain parser errors. This law begins after a literal and its datatype have been parsed and stored. It does not claim that the general panproto value readers implement every datatype's lexical grammar.

### Definitions used

A *known datatype* is one present in the protocol's value-kind table. A *derived value* is the successful result of coercing the stored lexeme under that kind. `ConstraintSemantics::LexicalSpace` compiles to an advisory `ValueConstraint` check.

### Proof

1. `ValidationPlan::compile` resolves the datatype through the edge named by the lexical-space declaration.
2. A known datatype supplies the kind used by the compiled check. The check emits a value-constraint diagnostic exactly when coercion fails.
3. An unknown datatype cannot supply that check and produces the typed incomplete reason `UnsupportedDeclaration`.
4. Both branches have advisory requirement. Thus neither changes a passing report to failure, and validation has no operation that could replace the graph's stored lexeme; the differential reads the lexeme back after validation.

### Executable counterpart

- Law ID: `keys.lexical-space-declaration`
- Reference: `panproto_reference::keys::lexical_space_outcome`
- Evidence scope: `12 lexemes (signed, zero-padded, space-padded, fractional, exponent, both 64-bit boundaries, non-numeric, and empty) under the integer and decimal datatypes of the table and one datatype absent from it, 36 cases; each case compares the outcome with the reference, checks advisory admission, and reads back the kept lexeme`
- Production certificate: `lexical_space_outcomes_match_the_datatype_table_reference; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=lexical_space_outcomes_match_the_datatype_table_reference)'`
- Independent reference anchor: `lexical_space_outcomes_match_the_datatype_table_reference; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=lexical_space_outcomes_match_the_datatype_table_reference)'`
- File: `crates/panproto-inst/src/validation/checks.rs`
- File: `crates/panproto-inst/src/validation/plan.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`
- File: `crates/panproto-inst/tests/reference_keys.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## An annotated format is preserved and checked only as advisory {#L-keys-and-registration-18}

### Statement

For `ConstraintSemantics::AnnotatedFormat`, validation preserves the stored value and compiles every recognized format check with advisory requirement. A failed recognized check emits a `ValueConstraint` diagnostic without failing admission. An unrecognized annotation does not impose a required assertion. By contrast, the separate `Format` semantics may require the same recognized predicate.

### Outside the domain

This law does not govern `ConstraintSemantics::Format`, nor a dialect that explicitly enables a format-assertion vocabulary. Those declarations may make a recognized format predicate required.

### Definitions used

An *annotated format* is a named constraint whose semantics are `AnnotatedFormat(family)`. Preservation means that validation leaves the graph value unchanged. The bounded executable carrier uses the recognized `uuid` predicate under both annotated and asserting semantics, and one unrecognized annotation. The reference recognizer reads RFC 9562's textual form: 36 characters, hyphens at positions 8, 13, 18 and 23, and a hexadecimal digit of either case elsewhere.

### Proof

1. Compilation distinguishes `AnnotatedFormat` from `Format` before selecting the same family-specific predicate.
2. The annotated branch fixes the check requirement to advisory. Thus a failed predicate records a diagnostic but leaves the report passing.
3. The asserting branch uses required status, so the same failed predicate fails admission. This contrast observes the declaration rather than an identity helper. An unrecognized annotation names no predicate, so compilation adds no check for it and its value is neither reported nor refused.
4. Validation borrows the input graph and returns a report. The bounded differential reads the graph after both checks and confirms that its string is unchanged.

### Executable counterpart

- Law ID: `keys.annotated-format-declaration`
- Reference: `panproto_reference::keys::annotated_format_outcome`
- Reference: `panproto_reference::keys::uuid_format`
- Evidence scope: `eight strings (two UUIDs, a short group, misplaced hyphens, braces, a non-hexadecimal digit, a non-UUID word, and the empty string) under uuid as annotation and as assertion and under one unrecognized annotation, 24 cases: 4 recognized, 6 advisory, 6 refused, 8 unchecked; each case reads back the kept string`
- Production certificate: `annotated_formats_match_the_independent_recognizer; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=annotated_formats_match_the_independent_recognizer)'`
- Independent reference anchor: `annotated_formats_match_the_independent_recognizer; command: cargo nextest run --release --locked -p panproto-inst --test reference_keys --no-tests=fail -E 'test(=annotated_formats_match_the_independent_recognizer)'`
- File: `crates/panproto-inst/src/validation/checks.rs`
- File: `crates/panproto-inst/src/validation/plan.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`
- File: `crates/panproto-inst/tests/reference_keys.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.
