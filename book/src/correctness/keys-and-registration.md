# Keys and registration

Keys connect graph structure to identity. A key declaration says how to read a tuple, which equality each part uses, and where equal tuples collide; a reference declaration names a key that can resolve such a tuple. Registration then binds those declarations to closed theories and replayable composition recipes. The eighteen laws on this page make each part of that chain explicit, from reading a key through canonical registration.

A key has an id, an ordered sequence of parts, a role, a scope, and either an empty equality vector or one equality declaration per part. An empty vector means representation equality at every part. A path part is a finite sequence of schema-edge labels. A position part names an ordered edge. Key roles are `Primary`, `Unique`, `Identifier`, `SortKey`, and `Index`; the first three are resolvable uniqueness roles, `SortKey` imposes order without uniqueness, and `Index` imposes neither. Scopes are document-wide, global, or relative to the nearest ancestor of a named vertex.

## Focused verification status

The focused release checks for laws 1--4 and 7--15 are green. The release
commands passed 1 canonical-theory test, 4 schema-declaration tests, 8
instance-key tests, 2 change-classification tests, and 1 VCS identity test.
The finite certificates cover 36 declaration encodings, 21 declaration
admission fixtures, 8 target-set cases, 37 values under 12 equality
configurations, 16,428 equality pairs, 607,836 transitivity triples, 341 XML
strings, 116,281 XML pairs, 28 non-key classification axes, 148 structural
graphs, 27,329,908 schemas, and 816,223,680 directed classifications. Strict
Clippy with `-D warnings` is green for the two schema targets, the instance
target, the classifier target, and the VCS library.

These checks establish the focused executable counterparts. They do not close
the independent audit, mutation, frozen-candidate, or global release gates;
the audit verdicts below thus remain pending.

## Key parts read the declared value or return NULL {#L-keys-and-registration-1}

### Statement

For a graph $G$, node $n$, and key $K$, `key_value(G, n, K)` returns one value per key part exactly when every part has a value. For a path $(e_1,\ldots,e_k)$, traversal follows matching out-edges in edge-id order through $e_1,\ldots,e_{k-1}$. At the last step it returns the first non-null own value of a matching target; when there is no matching last edge, it instead reads the source node's field named by the last edge. The empty path reads $n$'s non-null own value. A position part for edge $e$ first chooses an ordinary incoming $e$-edge into $n$, then returns the zero-based index of $n$ among that parent's ordinary $e$-targets ordered by `(position, edge id, target)`.

### Outside the domain

The result is `None` when the start node is absent, a path has no successful frontier, a selected own value is absent or null, a last-hop field is absent or null, a position part has no ordinary incoming carrier edge, or its index does not fit `i64`. One missing part makes the whole tuple `None`.

### Definitions used

An *own value* is `Presence::Present(v)` with $v$ unequal to the value variant `Null`. An *ordinary edge* has neither reserved fan kind and joins two non-fan nodes. Edge and node maps are ordered by numeric id.

### Proof

1. `path_value` handles the empty path by `own`. For a nonempty path, each preliminary hop replaces the frontier by matching targets in map order.
2. At the final label, the implementation either searches matching targets for the first own value or, only when no matching target exists, reads the named field. These branches are the path clause.
3. A position part finds the first qualifying incoming edge, sorts every sibling tuple `(position, id, target)`, and converts the matching index to `i64`.
4. `Iterator::collect` on `Option` returns a vector exactly when every part returns `Some`, proving tuple totality on the stated domain and NULL propagation outside it.

### Executable counterpart

- Law ID: `keys.key-declarations`
- Reference: `panproto_reference::keys::key_value`
- Enumerator: `duplicated/twin graph witnesses over path and position parts; exact indexed elements=14, NULL elements=4`
- Exhaustive differential test: `validation_reconstructs_path_keys_in_document_and_global_scopes; command: cargo test --release -p panproto-inst --test reference_keys validation_reconstructs_path_keys_in_document_and_global_scopes -- --exact --nocapture`
- Property differential test: `validation_reconstructs_position_buckets; command: cargo test --release -p panproto-inst --test reference_keys validation_reconstructs_position_buckets -- --exact --nocapture`
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

The operation returns `EqualityArity` when a nonempty equality vector does not have one entry per part. It returns `UnsupportedCollation(name)` when a string part requires a named SQL collation, whose comparison is deliberately left to the database. A production value constructed with an invalid numeric carrier may instead return `InvalidNumericRepresentation`; such values are outside the well-formed finite reference carrier. Thus equality arity and named collation are the two typed refusals on the carrier used here.

### Definitions used

The *nearest ancestor* is found breadth-first by reversing ordinary edges one level at a time, excluding the start node; ties at the first matching distance go to the least node id. Tuple bytes are a length-delimited concatenation of the canonical equality key of each part. Equal tuple bytes have equal declared-key behavior by [L-keys-and-registration-8](#L-keys-and-registration-8).

### Proof

1. The outer loop filters the ordered node map by anchor and non-fan shape, so it visits exactly the stated carrier in ascending order.
2. [L-keys-and-registration-1](#L-keys-and-registration-1) omits NULL tuples. Breadth-first reverse traversal gives the nearest level, and sorting each next frontier selects its least matching ancestor.
3. The scope id and equality-key bytes form the ordered map key. Map insertion creates one stored tuple at the first member and appends every later member, preserving node-id order.
4. Equality-vector expansion and equality-key construction are the only fallible calls. Their errors are exactly the three typed refusals in the outside-domain clause.

### Executable counterpart

- Law ID: `keys.reference-declarations`
- Reference: `panproto_reference::keys::buckets`
- Enumerator: `duplicated/twin graph witnesses over Document, Global, and Within scopes; exact buckets=6, outside elements=2`
- Exhaustive differential test: `validation_reconstructs_within_buckets_and_outside_cases; command: cargo test --release -p panproto-inst --test reference_keys validation_reconstructs_within_buckets_and_outside_cases -- --exact --nocapture`
- Property differential test: `validation_preserves_both_reference_keying_refusals; command: cargo test --release -p panproto-inst --test reference_keys validation_preserves_both_reference_keying_refusals -- --exact --nocapture`
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

For schemas in the finite declaration carrier, `canonical_bytes` is injective. It writes vertices, edges, hyper-edges, constraints, required edges, orderings, entries, keys, references, and target sets in declaration order. Ordered maps and sets use key order; sequences retain stored order behind a `u64` length; each sum variant has a distinct one-byte tag; strings and schema edges use the canonical primitive layout. Thus two schemas have equal bytes exactly when every represented declaration field is equal.

### Outside the domain

The statement concerns the finite reference `Schema` carrier. A larger production schema field absent from that carrier is outside this function's type and is not claimed to affect these bytes. Guard values outside canonical value encoding are represented by the reference writer's declared empty fallback and must be excluded from an injectivity scope.

### Definitions used

A *self-delimiting field* has either fixed width or a length prefix. A tagged sum writes its variant tag before its payload. The exact declaration carrier has zero through three vertices, one vertex kind, one edge name, and at most three labelled edges. Its two key slots are absent or hold an owner/key descriptor built from five roles, the document, global, and vertex-relative scopes, and the six nonempty words of length at most two over `Path([])` and `Path([v0 -prop:e-> v0])`. These atoms do not depend on the surrounding graph.

### Proof

1. Fixed order separates top-level fields, and a count separates every variable-length collection. Map and set order is canonical; within a collection, primitive encodings and distinct tags separate keys, variants, and payloads.
2. Reading equal byte streams from the beginning therefore gives equal counts and equal fields by induction over the layout. Applying this argument to every top-level field yields schema equality; determinism gives the converse.
3. There are 1, 2, 15, and 130 structural graphs at vertex counts zero through three. At positive $n$, there are $D_n=30n(n+2)$ owner/key descriptors, hence $(D_n+1)^2$ labelled two-slot declaration contexts per graph.
4. The certificate executes all 148 graphs, every descriptor in both slots, every directed one-axis transition, and every absent, same-owner, and different-owner second-key class. Because graph fields precede declaration fields and the untouched slot is encoded unchanged, equality of complete byte strings factors through equality of the executed graph and declaration components. The recorded multiplicities reconcile to $27{,}329{,}908$ schemas.

### Executable counterpart

- Law ID: `keys.canonical-schema-bytes-injective`
- Reference: `panproto_reference::schema::canonical_bytes`
- Enumerator: `panproto_reference::enumerate::keys_and_registration::{schemas,key_descriptors,axis_transitions,factor_classes}; exact counts: graphs=148, descriptors=[90,240,450], schemas=27,329,908`
- Exhaustive differential test: `canonical_declaration_factorization_reconciles_the_exact_carrier; command: cargo test --release -p panproto-schema --test reference_keys canonical_declaration_factorization_reconciles_the_exact_carrier -- --exact --nocapture`
- Property differential test: `canonical_declaration_factorization_reconciles_the_exact_carrier; command: cargo test --release -p panproto-schema --test reference_keys canonical_declaration_factorization_reconciles_the_exact_carrier -- --exact --nocapture`
- File: `crates/panproto-schema/src/canonical.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`

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

`classify_change(A, B)` returns `Identical` exactly when $A=B$. Otherwise it returns `Breaking` when any of the following holds: a primary, unique, or identifier key is added to a vertex already present in $A$; an existing key changes parts, equality, or role, widens its scope, or changes between distinct scopes of the same rank; a reference added to an existing holder is non-nullable or fails unresolved values; an existing reference narrows cardinality, nullability, or unresolved handling, or changes any other semantic field; a target set is new or gains a member or changes scope; or a nondeclaration schema field changes. Adding a sort key or index does not add an instance-admission obligation and is `BackwardCompatible`, as are key removals and the other nonidentical changes not listed above.

### Outside the domain

The classifier is total on the finite reference schema. Its fail-closed rule makes any change outside the enumerated key, reference, and target-set relaxations breaking.

### Definitions used

The five roles make different uniqueness, identity, ordering, and resolution promises. Thus every role-to-role change fails closed as breaking. Scope ranks are 3 for global, 2 for document, and 1 for within; distinct scopes at one rank count as a tightening in either direction. A directed *one-axis transition* adds or removes one descriptor, or changes only its role, scope, or part word.

### Proof

1. Structural equality is tested first and gives exactly `Identical`. `key_is_tightened` is the disjunction of changed parts, equality, role, greater scope rank, and unequal same-rank scopes; `reference_is_tightened` is the corresponding disjunction over every reference field.
2. Ordered-map scans detect added and changed declarations exactly as stated. An addition on an old vertex is breaking exactly for the three uniqueness-bearing roles; sort keys impose only an advisory order check, and indexes promise no uniqueness or resolution. The separate target-set predicate detects a changed scope or any new member. Equality of all nondeclaration fields defines `declarations_only`, so the final disjunction returns `Breaking` exactly for the listed cases.
3. Each descriptor contributes one add, one remove, four role changes, $n+1$ scope changes, and five part changes, giving $D_n(n+12)$ directed transitions. The second slot is either absent, owned by the changed key's vertex, or owned by another vertex. Since its declaration is identical before and after, classification is congruent under that untouched context; the same argument applies to the unchanged structural graph.
4. The differential executes every graph, descriptor transition, slot, and nonempty second-key context class. The certificate's multiplicities sum to
   $$\sum_n \mathrm{graphs}_n\,2D_n(D_n+1)(n+12)=816{,}223{,}680,$$
   which is the full labelled classification carrier.
5. A separate 28-case same-input matrix holds the structural context fixed while changing equality, reference addition and removal, each reference semantic field, target-set membership and scope, or one nondeclaration field. It checks both directed relaxation pairs and every fail-closed axis against production.

### Executable counterpart

- Law ID: `keys.change-classification`
- Reference: `panproto_reference::schema::classify_change`
- Enumerator: `panproto_reference::enumerate::keys_and_registration::{schemas,key_descriptors,axis_transitions,factor_classes}; exact key classifications=816,223,680; additional non-key axes=28`
- Exhaustive differential test: `every_o17_key_change_classifies_through_the_factorization_certificate; command: cargo test --release -p panproto-check --test reference_classify every_o17_key_change_classifies_through_the_factorization_certificate -- --exact --nocapture`
- Property differential test: `non_key_classification_axes_match_the_reference; command: cargo test --release -p panproto-check --test reference_classify non_key_classification_axes_match_the_reference -- --exact --nocapture`
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

1. The W-type, graph-instance, functor, and flat theories each declare the interface symbols they use. Their coherence equations therefore typecheck in the theory that contains them.
2. Each registration group builds one explicit-identity diagram over its stated interface. Symbols outside the interface are rejected on a name clash rather than silently identified.
3. The composition checker verifies every inclusion and the identity mediator. The per-shape tests require passed records for both checks and reject every inapplicable record.
4. The generic production/reference differential separately establishes agreement of composition over its bounded span carrier. Thus the shape tests exercise the registered theories, while the differential checks the operation they call.

### Executable counterpart

- Law ID: `keys.q6`
- Reference: `panproto_reference::theory::{typecheck_signature,compose}`
- Enumerator: `the tree, graph, table, and flat registration groups over their declared interfaces`
- Exhaustive differential test: `theories::tests::standalone_total_theories_pass_inclusion_and_universal_checks; command: cargo test --release -p panproto-protocols --lib theories::tests::standalone_total_theories_pass_inclusion_and_universal_checks -- --exact --nocapture`
- Property differential test: `composition_agrees_with_the_reference_on_every_span; command: cargo test --release -p panproto-gat --test reference_theories composition_agrees_with_the_reference_on_every_span -- --exact --nocapture`
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

Each built-in protocol records the exact composition recipe for its schema and instance theories. Replaying either recipe from the theories it names reproduces the registered theory's canonical bytes. The amalgamation wrappers retain the 0.74 naming and result surface, while registration uses one explicit-identity diagram with reject-on-clash semantics.

### Outside the domain

A protocol introduced after 0.74 has no earlier registered theory to compare. For such a protocol, the law still requires a recorded replayable recipe and a passed composition report. Protocols whose instance theory was deliberately redeclared compare against that declared replacement rather than an obsolete byte string.

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
- Enumerator: `every registered schema and instance recipe, plus the bounded reference recipe carrier`
- Exhaustive differential test: `recompose_of_every_registration_spec_reproduces_its_theory; command: cargo test --release -p panproto-protocols --test registration_equivalence recompose_of_every_registration_spec_reproduces_its_theory -- --exact --nocapture`
- Property differential test: `recipe_replay_agrees_with_the_reference_on_every_recipe; command: cargo test --release -p panproto-gat --test reference_theories recipe_replay_agrees_with_the_reference_on_every_recipe -- --exact --nocapture`
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
2. Each digest hashes the corresponding canonical bytes under its fixed context. Equal byte strings therefore produce equal identities without conflating theory and composition domains.
3. A bounded same-input differential independently encodes empty and unary theories plus one carrier-complete theory. The third fixture visits every sort kind, value kind, coercion class, expression, literal, pattern, conflict strategy, optional directed-equation field, and duplicate-name tie order. Wrapping all three theories in both composition modes also checks the inline theory and consecutive provenance-map layout.
4. The fixture test recomposes each recorded diagram, compares its canonical composition bytes with the decoded fixture, and compares the resulting digest with `golden.json`.
5. The cross-process test repeats every registration composition in 32 child processes and compares the complete byte maps. This directly checks the process-dependent source of nondeterminism that a single-process test cannot observe.

### Executable counterpart

- Law ID: `keys.q8`
- Reference: `panproto_reference::theory::{canonical_theory_bytes,canonical_composition_bytes}`
- Enumerator: `three bounded same-input theories under both composition modes, including one carrier-complete fixture; the frozen theory-composition corpus; every registered composition in 32 child processes`
- Exhaustive differential test: `canonical_theory_and_composition_bytes_match_reference_on_bounded_fixtures; command: cargo test --release -p panproto-schema --test reference_theory_canonical canonical_theory_and_composition_bytes_match_reference_on_bounded_fixtures -- --exact --nocapture`
- Property differential test: `cross_process::compose_bytes_are_identical_across_processes; command: cargo test --release -p panproto-protocols --test registration_equivalence cross_process::compose_bytes_are_identical_across_processes -- --exact --nocapture`
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

Text comparison under a named SQL collation returns `None` in the reference and `UnsupportedCollation` in production, since its kernel belongs to the database implementation. A malformed production numeric carrier may return `InvalidNumericRepresentation`; it has no value in the finite reference carrier. Every other enumerated input returns `Some(true)` or `Some(false)` in the reference and the corresponding `Ok` value in production.

### Definitions used

Finite numerics are normalized to a sign, a base-ten natural significand without trailing zeroes, and an adjusted exponent. Binary floats enter this carrier exactly when their denominator is a power of two, which always holds for finite IEEE values. SQL `PadSpace` removes trailing U+0020 before collation; `AsciiCaseInsensitive` folds ASCII uppercase letters only.

### Proof

1. The five outer match arms are exactly the five equality declarations.
2. Numeric normalization maps integers, decimals, and finite floats to one exact decimal value; special values map to three distinguished keys. Equality of these keys is therefore the stated mathematical kernel.
3. The within-kind arm prefixes that key with the declared or selected primitive. The SQL and XML arms perform their stated string transformations and use the same fallback for other values.
4. `sql_text` refuses exactly a named collation. Numeric normalization separately propagates `InvalidNumericRepresentation` for an ill-formed production carrier; no other branch is partial.

### Executable counterpart

- Law ID: `keys.equality-kernel-equivalence`
- Reference: `panproto_reference::keys::key_equivalence`
- Enumerator: `panproto_reference::enumerate::keys_and_registration::{equality_values,equality_configurations}; exact counts: values=37, configurations=12, pairs=16,428, triples=607,836`
- Exhaustive differential test: `equality_carrier_executes_all_pairs_and_triples; command: cargo test --release -p panproto-inst --test reference_keys equality_carrier_executes_all_pairs_and_triples -- --exact --nocapture`
- Property differential test: `equality_carrier_executes_all_pairs_and_triples; command: cargo test --release -p panproto-inst --test reference_keys equality_carrier_executes_all_pairs_and_triples -- --exact --nocapture`
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

For every equality declaration $E$ and values $x,y$ on which $E$ is computable, if $x$ and $y$ are equal by representation then they are equivalent under $E$. `representation_refines(E, x, y)` returns the truth value of this implication. The reference returns `Some(true)` when representation equality is false or both equalities hold, `Some(false)` only if representation equality holds and declared equality does not, and `None` precisely when the consequent requires an unsupported named SQL collation. Production returns the corresponding `Ok` value or typed refusal.

### Outside the domain

A representationally equal string pair under a named SQL collation returns `None` in the reference and `UnsupportedCollation` in production; the function does not assume a database-specific kernel. An invalid production numeric carrier may additionally return `InvalidNumericRepresentation` and is outside the finite reference carrier.

### Definitions used

For propositions $P$ and $Q$, implication $P \Rightarrow Q$ is $\neg P \lor Q$. Representation equality and declared equality are those of the value layer and [L-keys-and-registration-8](#L-keys-and-registration-8).

### Proof

1. The function first evaluates the total representation equality predicate.
2. It returns `!represented || key_equivalence(E, x, y)?`, which is exactly the Boolean implication.
3. If the antecedent is false, short-circuit evaluation returns true without demanding a declared comparison. If it is true, the result and any refusal are exactly those of the declared equality.

### Executable counterpart

- Law ID: `keys.representation-refinement`
- Reference: `panproto_reference::keys::representation_refines`
- Enumerator: `panproto_reference::enumerate::keys_and_registration::{equality_values,equality_configurations}; exact pairs=16,428`
- Exhaustive differential test: `equality_carrier_executes_all_pairs_and_triples; command: cargo test --release -p panproto-inst --test reference_keys equality_carrier_executes_all_pairs_and_triples -- --exact --nocapture`
- Property differential test: `named_collation_has_the_exact_typed_refusal_boundary; command: cargo test --release -p panproto-inst --test reference_keys named_collation_has_the_exact_typed_refusal_boundary -- --exact --nocapture`
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
- Enumerator: `10 equality forms: legacy empty, representation, numeric value, decimal/float/any within-kind, binary/ascii-folded/named SQL, and XML non-CDATA`
- Exhaustive differential test: `extended_declaration_bytes_match_reference_on_every_sum_variant; command: cargo test --release -p panproto-schema --test reference_keys extended_declaration_bytes_match_reference_on_every_sum_variant -- --exact --nocapture`
- Property differential test: `extended_declaration_bytes_match_reference_on_every_sum_variant; command: cargo test --release -p panproto-schema --test reference_keys extended_declaration_bytes_match_reference_on_every_sum_variant -- --exact --nocapture`
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
- Enumerator: `the seven values {0, 1, 1.5, "a", "ab", "b", NaN}; exact ordered pairs=49, triples=343`
- Exhaustive differential test: `sort_preorder_and_adjacent_descent_match_the_reference; command: cargo test --release -p panproto-inst --test reference_keys sort_preorder_and_adjacent_descent_match_the_reference -- --exact --nocapture`
- Property differential test: `sort_preorder_and_adjacent_descent_match_the_reference; command: cargo test --release -p panproto-inst --test reference_keys sort_preorder_and_adjacent_descent_match_the_reference -- --exact --nocapture`
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
- Enumerator: `all 2,801 sequences of length at most four over {0, 1, 1.5, "a", "ab", "b", NaN}`
- Exhaustive differential test: `sort_preorder_and_adjacent_descent_match_the_reference; command: cargo test --release -p panproto-inst --test reference_keys sort_preorder_and_adjacent_descent_match_the_reference -- --exact --nocapture`
- Property differential test: `sort_preorder_and_adjacent_descent_match_the_reference; command: cargo test --release -p panproto-inst --test reference_keys sort_preorder_and_adjacent_descent_match_the_reference -- --exact --nocapture`
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

For a sequence of target-set members `(vertex, key, arity, equality)`, `target_set` returns true exactly when the sequence is nonempty, no `(vertex, key)` pair repeats, and every member has the first member's arity and equality vector. The schema-level `validate_declarations` operation does not call this helper. It instead requires (i) an id distinct from every vertex id, (ii) at least one member, and (iii) member keys that exist, do not repeat, have the declared scope, and share the first valid member's effective signature.

### Outside the domain

The Boolean helper returns false for an empty or incompatible sequence. Schema validation records `TargetSetNameClash` for an id collision and `EmptyTargetSet` for an empty sequence. It records one `TargetSetMemberMismatch` for each absent, repeated, scope-incompatible, arity-incompatible, or equality-incompatible member, in target-set and member declaration order.

### Definitions used

The *signature* of a member key is its part count and effective equality vector. Its effective vector is the declared nonempty vector, or one `Representation` entry per part when the declaration is empty.

### Proof

1. The helper obtains the first signature with `first`; absence returns false. A set cardinality comparison gives pairwise distinctness, and an all-quantifier gives signature compatibility.
2. Schema validation walks ordered target sets. It checks the name clash before member checks, then maintains a set of member pairs and the first valid signature.
3. A member is valid precisely when lookup succeeds, its pair is new, its scope agrees, and its effective signature agrees. The negation appends `TargetSetMemberMismatch`, proving completeness and order for declared members.
4. The schema validator tests emptiness before the member loop and emits `EmptyTargetSet`, then skips the loop. Thus an empty declaration has one typed defect rather than an accidental arity or member error.

### Executable counterpart

- Law ID: `keys.target-set-declaration`
- Reference: `panproto_reference::keys::target_set`
- Enumerator: `six local cases covering compatible, empty, duplicate, arity-incompatible, equality-incompatible, and implicit/explicit representation signatures; plus absent-key and scope-mismatch production cases`
- Exhaustive differential test: `target_sets_match_on_the_complete_local_predicate; command: cargo test --release -p panproto-schema --test reference_keys target_sets_match_on_the_complete_local_predicate -- --exact --nocapture`
- Property differential test: `target_sets_match_on_the_complete_local_predicate; command: cargo test --release -p panproto-schema --test reference_keys target_sets_match_on_the_complete_local_predicate -- --exact --nocapture`
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
- Enumerator: `panproto_reference::enumerate::keys_and_registration::xml_strings; exact counts: strings=341, pairs=116,281`
- Exhaustive differential test: `xml_kernel_executes_the_exact_o17_carrier; command: cargo test --release -p panproto-inst --test reference_keys xml_kernel_executes_the_exact_o17_carrier -- --exact --nocapture`
- Property differential test: `xml_kernel_executes_the_exact_o17_carrier; command: cargo test --release -p panproto-inst --test reference_keys xml_kernel_executes_the_exact_o17_carrier -- --exact --nocapture`
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

The complete key, target-set, and reference declaration carrier is valid exactly when the independent `validate_declarations` function returns `Ok(())`. Key ids are unique per vertex, equality arity is exact, and sort keys have a unique ordered carrier from their `Within` scope. Target sets are nonempty and satisfy the schema-level member conditions in [L-keys-and-registration-13](#L-keys-and-registration-13). Every reference names a present resolvable target of matching arity and a compatible edge; inline references redirect through a target set; variable binders name identifier keys and use one binding mode; standoff lengths are not inclusive, fragmented standoffs are many-valued over an ordered holder edge, guarded partitions have distinct representation values, referenced text and scope-via chains are typed and acyclic, and target-set references carry no redundant per-vertex key. The production validator agrees on this declaration-local predicate and additionally checks that named vertices, paths, positional carriers, scope ancestors, and protocol-required edges exist in the production schema. Each validator returns every defect in its domain in deterministic declaration order. Canonical schema bytes distinguish every valid declaration in this carrier.

### Outside the domain

An invalid carrier returns the nonempty ordered defect vector. Named SQL collation behavior at instance level is outside declaration validity; its name remains part of canonical bytes. Production fields absent from the finite reference carrier are outside the injectivity claim.

### Definitions used

The independent declaration errors are `EqualityArity`, `DuplicateKeyId`, `TargetSetNameClash`, `EmptyTargetSet`, `TargetSetMemberMismatch`, `SortKeyNotOrdered`, `KeyNotResolvable`, `UnknownTarget`, `ReferenceArity`, `InlineRequiresRedirect`, `InlineRequiresTargetSet`, `LengthInclusive`, `InvalidFragments`, `DuplicateGuard`, `EdgeMismatch`, `TargetSetHasKey`, `VariableBinderInvalid`, `BinderModeConflict`, `TextReferenceInvalid`, and `ScopeViaInvalid`. Production renames `ReferenceArity` to `ArityMismatch`; splits `VariableBinderInvalid` into `UnknownTargetKey` and `BinderNotIdentifier`; and adds `UnknownVertex`, `PathDoesNotResolve`, `PositionNotOrdered`, `EdgeRequired`, and `WithinNotAncestor` for the graph-aware checks. These substitutions yield the 26 public `KeyRefError` variants. Representation equality determines duplicate guard values.

### Proof

1. `validate_declarations` appends results from key, target-set, reference, binder-mode, and guard passes in that order and returns success exactly when the vector is empty.
2. Each pass traverses ordered maps and stored sequences and appends the defect corresponding to the negation of one condition in the Statement. Reference validation returns early only after an unknown target, since later target-dependent checks would have no typed object to inspect.
3. The text-reference and scope-via helpers follow named chains with visited sets, so they accept exactly the typed acyclic chains described above.
4. Canonical writing gives every declaration sum a distinct tag, every collection a length, and every map a canonical order. The injectivity argument of [L-keys-and-registration-3](#L-keys-and-registration-3) therefore applies to this declaration subcarrier.

### Executable counterpart

- Law ID: `keys.group-g-declaration-injectivity`
- Reference: `panproto_reference::schema::canonical_bytes`
- Enumerator: `36 canonical fixtures covering every declaration sum variant and nondefault extension; 21 validation fixtures covering success and each of the 20 independent refusal classes`
- Exhaustive differential test: `extended_declaration_bytes_match_reference_on_every_sum_variant; command: cargo test --release -p panproto-schema --test reference_keys extended_declaration_bytes_match_reference_on_every_sum_variant -- --exact --nocapture`
- Property differential test: `declaration_validation_matches_reference_on_every_common_refusal; command: cargo test --release -p panproto-schema --test reference_keys declaration_validation_matches_reference_on_every_common_refusal -- --exact --nocapture`
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

A reference whose target has role `Index` derives no pair. For a composite referencing tuple, `NullMatch::Simple` creates a resolution occurrence exactly when every part is non-NULL, while `NullMatch::Full` creates one exactly when at least one part is non-NULL. Thus an all-NULL tuple is absent under both modes; a partly NULL tuple is absent under `Simple` and unresolved under `Full`; and an all-present tuple is resolved or dangling according to the declared target key.

### Outside the domain

The law does not assert uniqueness for an `Index` key. It also does not implement `MATCH PARTIAL`, which is a typed protocol refusal before resolution.

### Definitions used

A *resolution occurrence* is a holder element together with the tuple read from the reference's declared source parts. An absent occurrence produces neither a pair nor a dangling record. An unresolved occurrence produces a dangling record containing the complete tuple, including its NULL parts.

### Proof

1. Resolution excludes a target key whose role is `Index` before building target buckets. Thus no value equality, including equality of two absent index fields, can derive a pair through that role.
2. Under `Simple`, the occurrence filter is conjunction over the tuple's non-NULL tests. Under `Full`, it is disjunction. These formulas give the three null cases in the Statement.
3. The filtered occurrences enter the ordinary target lookup. An equal all-present tuple derives one pair; an unequal all-present tuple is dangling. A partly NULL tuple enters that lookup only under `Full`, where it is necessarily dangling against an all-present target.
4. The production/reference differential observes pairs and complement records through the complete `TreeToGraph` resolution operation. It does not infer these outcomes from a separate Boolean helper.

### Executable counterpart

- Law ID: `keys.index-and-null-match-declarations`
- Reference: `panproto_reference::resolution::get`
- Enumerator: `five two-part tuples (equal, unequal, all NULL, and both partial-NULL orientations) under Simple and Full; all 16 optional-index pairs for the Index refusal`
- Exhaustive differential test: `composite_foreign_key_null_matching_uses_real_resolution_occurrences; command: cargo test --release -p panproto-io --test reference_resolution composite_foreign_key_null_matching_uses_real_resolution_occurrences -- --exact --nocapture`
- Property differential test: `index_fields_never_resolve_targets; command: cargo test --release -p panproto-io --test reference_resolution index_fields_never_resolve_targets -- --exact --nocapture`
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
4. Both branches have advisory requirement. Thus neither changes a passing report to failure, and validation has no operation that could replace the graph's stored lexeme.

### Executable counterpart

- Law ID: `keys.lexical-space-declaration`
- Reference: `panproto_reference::validation::lexical_space`
- Enumerator: `the lexemes {"0", "42", "-1", "not-an-integer"} under the declared integer datatype`
- Exhaustive differential test: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo test --release -p panproto-inst --test reference_validation supplemental_validation_definitions_have_bounded_production_witnesses -- --exact --nocapture`
- Property differential test: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo test --release -p panproto-inst --test reference_validation supplemental_validation_definitions_have_bounded_production_witnesses -- --exact --nocapture`
- File: `crates/panproto-inst/src/validation/checks.rs`
- File: `crates/panproto-inst/src/validation/plan.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`

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

An *annotated format* is a named constraint whose semantics are `AnnotatedFormat(family)`. Preservation means that validation leaves the graph value unchanged. The bounded executable carrier uses the recognized `uuid` predicate under both annotated and asserting semantics.

### Proof

1. Compilation distinguishes `AnnotatedFormat` from `Format` before selecting the same family-specific predicate.
2. The annotated branch fixes the check requirement to advisory. Thus a failed predicate records a diagnostic but leaves the report passing.
3. The asserting branch uses required status, so the same failed predicate fails admission. This contrast observes the declaration rather than an identity helper.
4. Validation borrows the input graph and returns a report. The bounded differential reads the graph after both checks and confirms that its string is unchanged.

### Executable counterpart

- Law ID: `keys.annotated-format-declaration`
- Reference: `panproto_reference::validation::{annotated_format_parse,annotated_format_advisory,format_assertion_acceptance}`
- Enumerator: `four UUID strings under both advisory and asserting requirements, with both recorded-diagnostic states`
- Exhaustive differential test: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo test --release -p panproto-inst --test reference_validation supplemental_validation_definitions_have_bounded_production_witnesses -- --exact --nocapture`
- Property differential test: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo test --release -p panproto-inst --test reference_validation supplemental_validation_definitions_have_bounded_production_witnesses -- --exact --nocapture`
- File: `crates/panproto-inst/src/validation/checks.rs`
- File: `crates/panproto-inst/src/validation/plan.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: key and registration documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.
