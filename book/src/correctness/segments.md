# Segments

A large instance may need to be stored in pieces without changing the instance it denotes. Segment correctness fixes the conditions under which this is possible: identifiers remain global, each node has one owner, boundary records describe every crossing edge, and assembling the pieces recovers the original value. These conditions form the *segment agreement*. The laws below cover the segment foundation only; execution runners, storage backends, checkpoints, and segmented edits belong to later components.

Let $I$ be a finite tree, table, or graph instance, and let $P$ be a partition for the same shape. We write $P(n)$ for the segment that owns node or row identifier $n$, $\operatorname{split}_P(I)$ for partitioning, and $\operatorname{glue}_P$ for checked assembly. An edge is owned by the owner of its source endpoint. The finite differential carrier contains 25 labelled graphs and all 290 assignments of their nodes to two or three labelled segments; one crossing-boundary witness for each instance shape checks the shape-specific representation.

First, the page establishes ownership, boundary, and assembly laws. Second, it fixes canonical artifacts, the storage vocabulary, and typed refusals. Third, it states the validation and allocation boundaries that the segment core must preserve.

## Ownership is a total partial function on declared ranges {#L-segments-1}

### Statement

A valid partition numbers its segments densely from zero, stores each segment's node ranges in ascending disjoint order, and assigns no identifier to two ranges. For every identifier covered by a declared range, $P(n)$ is unique and is found by binary search. Nodes and rows are owned by $P(n)$, while graph edges, tree arcs, and foreign-key pairs are owned by the owner of their source.

### Outside the domain

An inverted range, overlapping pair of ranges, non-dense segment identifier, node-allocation range outside the owned ranges, or edge-allocation range on a table is a typed partition refusal. An instance element whose identifier no range covers is `Unowned`.

### Definitions used

An *owned range* is an inclusive interval of `u32` identifiers. A *source endpoint* is the graph edge source, tree-arc parent, or referencing table row.

### Proof

Partition validation sorts all ranges by their lower endpoint and rejects the first overlap. The resulting range sequence admits at most one interval containing any identifier, so binary search returns a unique owner whenever one exists. Partitioning reads that owner for every node and for each edge's source; it never derives edge ownership from an edge identifier.

### Executable counterpart

- Law ID: `segments.sg1`
- Reference: `panproto_reference::segment::partition`
- Enumerator: `panproto_reference::enumerate::segments::graph_partition_cases; bound: graphs=25, partitions=290`
- Exhaustive differential test: `bounded_graph_partition_glue_and_segment_bytes_agree_exhaustively; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=bounded_graph_partition_glue_and_segment_bytes_agree_exhaustively)'`
- Property differential test: `segment::tests::partition::every_generated_partition_is_valid; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::partition::every_generated_partition_is_valid)'`
- File: `crates/panproto-inst/src/segment/partition.rs`
- File: `crates/panproto-inst/src/segment/split.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

### Mutation equivalences

Reordering ranges before building the private search index is equivalent when their stored canonical order remains unchanged. Changing ownership or accepting an overlap is not equivalent.

### Audit record

- Author: segment correctness author
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification are pending.

## A boundary is the exact cut of the owned edge relation {#L-segments-2}

### Statement

For every owned edge from segment $i$ to a node in segment $j \ne i$, segment $i$ contains one ghost for the remote target and segment $j$ contains one incoming stub keyed by that edge's canonical reference. No boundary record exists for a non-crossing edge. A ghost records the target identifier, owner, and anchor; an incoming stub records the source identifier, source owner and anchor, target identifier, and label.

### Outside the domain

Missing, extra, or unequal ghosts and incoming stubs are refused. The same is true when an owned edge names a local endpoint that its owner does not hold, when a boundary key has the wrong shape, or when a tree segment disagrees with its embedding.

### Definitions used

The *cut* of an edge relation contains exactly those edges whose source and target owners differ. Graph edges and embedded fan edges use their edge identifier as the incoming-stub key; ordinary tree arcs and table pairs use their native references.

### Proof

Partitioning visits each edge once. A crossing edge inserts its target into the source boundary and the same edge record into the target boundary. Validation reconstructs this relation from owned edges and compares both ordered maps, so either an omitted record or an extra record produces a typed mismatch.

### Executable counterpart

- Law ID: `segments.sg2`
- Reference: `panproto_reference::segment::partition`
- Enumerator: `panproto_reference::enumerate::segments::shape_witnesses; bound: shapes=3, segments=2`
- Exhaustive differential test: `graph_table_and_tree_artifacts_match_the_reference_exactly; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=graph_table_and_tree_artifacts_match_the_reference_exactly)'`
- Property differential test: `segment::tests::segments::partition_verification_rejects_ghost_mismatch_duplicate_edge_ids_and_tree_cycles; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::segments::partition_verification_rejects_ghost_mismatch_duplicate_edge_ids_and_tree_cycles)'`
- File: `crates/panproto-inst/src/segment/body.rs`
- File: `crates/panproto-inst/src/segment/check.rs`
- File: `crates/panproto-inst/src/segment/split.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

### Mutation equivalences

Coalescing several ghosts of the same remote target is equivalent because a boundary stores one ghost per target. Coalescing incoming stubs or changing any recorded anchor or label is not equivalent.

### Audit record

- Author: segment correctness author
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification are pending.

## Partitioning and assembly are inverse on valid inputs {#L-segments-3}

### Statement

For every valid $I$ and $P$ of the same shape,

$$\operatorname{glue}_P(\operatorname{split}_P(I)) = I.$$

Equality is exact for graph and table instances. For trees it also includes the root, arc order, fan order, embedding identifiers, and the two monotone embedding counters.

### Outside the domain

The equation is stated only when every element is owned, the tree and its embedding agree, and the partition and all produced segments satisfy their contracts. Assembly refuses duplicate elements, inconsistent boundaries, missing root state, or counters below a held identifier.

### Definitions used

*Exact tree equality* pairs a tree with its `TreeEmbedding`; equality of the tree alone would omit the identifiers required by graph transport and later edits.

### Proof

Partitioning places each node and each source-owned edge in exactly one segment by [L-segments-1](#L-segments-1). Assembly verifies the boundaries from [L-segments-2](#L-segments-2), then unions the disjoint node maps and merges edge records in their canonical order. The root owner retains the tree root and counters, so no tree presentation data is lost.

### Executable counterpart

- Law ID: `segments.sg3`
- Reference: `panproto_reference::segment::partition`
- Reference: `panproto_reference::segment::glue`
- Enumerator: `panproto_reference::enumerate::segments::graph_partition_cases; bound: graphs=25, partitions=290`
- Exhaustive differential test: `bounded_graph_partition_glue_and_segment_bytes_agree_exhaustively; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=bounded_graph_partition_glue_and_segment_bytes_agree_exhaustively)'`
- Property differential test: `segment::tests::laws::partitioning_then_assembling_returns_the_identical_instance_for_every_partition; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::laws::partitioning_then_assembling_returns_the_identical_instance_for_every_partition)'`
- File: `crates/panproto-inst/src/segment/split.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

### Mutation equivalences

None. Any change to an owned element, boundary record, root, ordering identifier, or embedding counter is observable after assembly.

### Audit record

- Author: segment correctness author
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification are pending.

## Canonical segment artifacts bind all semantic inputs {#L-segments-4}

### Statement

Canonical segment bytes contain the format version, shape, schema digest, segment specification, complete owned body, and complete boundary in a fixed order. Segment digests derive from exactly those bytes under the segment context. Manifest records bind the segment digest, counts, greatest identifiers, owned edge-identifier run, and crossing-edge digests; the manifest root and instance digest are deterministic functions of those records and their streams.

### Outside the domain

A malformed segment, noncanonical value, partition mismatch, changed schema digest, duplicate edge identifier, unpaired crossing digest, or malformed record stream is refused before a manifest verifies. Decoders also refuse truncation, trailing bytes, and noncanonical ordering.

### Definitions used

A *crossing digest* hashes the canonical records for one ordered pair of source and target segments. The manifest root is the domain-separated binary Merkle root over canonical record bytes.

### Proof

Every byte writer traverses fields in declaration order and ordered collections in key order. Segment verification recomputes its complete record, while manifest verification recomputes the partition, cross-pair agreement, and Merkle root without reading a segment body. Streaming instance records merge in the same canonical order as whole-instance writers, which gives the same instance digest without assembly.

### Executable counterpart

- Law ID: `segments.sg6`
- Reference: `panproto_reference::segment::canonical_manifest_bytes`
- Reference: `panproto_reference::segment::canonical_segment_bytes`
- Reference: `panproto_reference::segment::seal_manifest`
- Reference: `panproto_reference::segment::verify_manifest`
- Enumerator: `panproto_reference::enumerate::segments::shape_witnesses; bound: shapes=3, segments=2`
- Exhaustive differential test: `graph_table_and_tree_artifacts_match_the_reference_exactly; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=graph_table_and_tree_artifacts_match_the_reference_exactly)'`
- Property differential test: `segment::tests::laws::streaming_instance_digest_equals_the_digest_of_the_assembled_instance; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::laws::streaming_instance_digest_equals_the_digest_of_the_assembled_instance)'`
- File: `crates/panproto-inst/src/segment/bytes.rs`
- File: `crates/panproto-inst/src/segment/manifest.rs`
- File: `crates/panproto-inst/src/segment/stream.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

### Mutation equivalences

No field covered by canonical bytes admits an equivalence. A caller-local numbering map is outside the artifact and may change without changing any digest.

### Audit record

- Author: segment correctness author
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification are pending.

## The storage vocabulary has one stable typed layout {#L-segments-5}

### Statement

Segment identifiers, ranges, bodies, boundaries, manifest roles, task keys, artifact keys, phases, and journals have the declared field and variant order. Their serde names are stable and snake case where the type uses external tags. These types state data needed by later execution components without asserting a runner or storage-backend law in this release.

### Outside the domain

Bytes with an unknown variant, another version, a malformed field, a duplicate ordered key, or trailing input are outside the canonical image. This law does not claim transactional behavior of a concrete store.

### Definitions used

The *storage vocabulary* is the public data model shared by segment producers and future runners. Layout stability concerns typed and canonical serialization, not implementation-specific memory layout.

### Proof

The Rust declarations determine serde field order and external tags. Canonical writers state the same order explicitly, and their decoders require complete consumption. Cross-implementation comparison of complete segment and manifest bytes thus detects a changed tag, field position, or version.

### Executable counterpart

- Law ID: `segments.sg7-types`
- Reference: `panproto_reference::segment::canonical_journal_bytes`
- Reference: `panproto_reference::segment::decode_journal_bytes`
- Reference: `panproto_reference::segment::journal_digest`
- Bounded witness: `phases=6, populated task keys=2, coordinator and segment task forms=2`
- Exhaustive differential test: `storage_vocabulary_journal_layout_matches_the_reference_exactly; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=storage_vocabulary_journal_layout_matches_the_reference_exactly)'`
- Property differential test: `segment::tests::manifest::a_run_journal_round_trips_and_its_run_digest_covers_every_input; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::manifest::a_run_journal_round_trips_and_its_run_digest_covers_every_input)'`
- Reference property test: `journals_have_canonical_task_order_and_domain_separated_mutation_sensitive_digests; command: cargo nextest run -p panproto-reference --test segments --no-tests=fail -E 'test(=journals_have_canonical_task_order_and_domain_separated_mutation_sensitive_digests)'`
- File: `crates/panproto-inst/src/segment/store.rs`
- File: `crates/panproto-inst/src/segment/error.rs`
- File: `crates/panproto-inst/src/segment/manifest.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

### Mutation equivalences

None for public tags, variants, fields, or canonical bytes. Private indexing structures may change when all serialized values and observations remain identical.

### Audit record

- Author: segment correctness author
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification are pending.

## Segment refusals preserve variant and witness data {#L-segments-6}

### Statement

For a fixed invalid input, segment operations return the same typed error variant and structured witness in every run. Structural validation precedes artifact and operation checks. Within one stage, errors are ordered by segment identifier, element reference, and the variant-specific precedence key.

### Outside the domain

Free-text messages of storage failures are supplied by an external store and are not compared here. This law covers the segment-core variants and their structured payloads; later runner reports add their own status laws.

### Definitions used

A *typed witness* is the complete payload of a `SegmentError`, including element references, owners, ranges, digests, and every violation in an `Invalid` value.

### Proof

Partition and segment checks traverse ordered carriers, accumulate local violations in canonical order, and select the first structural error before decoding later artifacts. The production and reference error vocabularies expose stable external names, so the differential observes both branch selection and error ordering.

### Executable counterpart

- Law ID: `segments.sg9`
- Reference: `panproto_reference::segment::decode_segment_bytes`
- Reference: `panproto_reference::segment::partition`
- Enumerator: `panproto_reference::enumerate::segments::graph_partition_cases; bound: graphs=25, partitions=290`
- Exhaustive differential test: `partition_and_decoder_refusals_preserve_typed_error_precedence; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=partition_and_decoder_refusals_preserve_typed_error_precedence)'`
- Property differential test: `segment::tests::segments::invalid_segments_list_every_violation_sorted; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::segments::invalid_segments_list_every_violation_sorted)'`
- File: `crates/panproto-inst/src/segment/error.rs`
- File: `crates/panproto-inst/src/segment/check.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

### Mutation equivalences

Changing an error message while retaining a structured storage error is equivalent only where the message is explicitly external. Changing a variant, payload field, or precedence order is not equivalent.

### Audit record

- Author: segment correctness author
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification are pending.

## Local validation retains node identity {#L-segments-7}

### Statement

A graph or tree segment used for local validation must store each node under its carried identifier. Validation checks this condition before constructing its local view, and a mismatch returns `NodeIdMismatch` with the map key and carried identifier.

### Outside the domain

Table rows carry their identifiers in map keys and row references, so this additional embedded-node check applies only to graph and tree segments. Other segment-contract violations retain their earlier precedence.

### Definitions used

The *local validation view* is the temporary graph used to evaluate checks whose locality is one segment. Map-key identity ensures that the view and canonical bytes address the same node.

### Proof

The contract check compares every ordered node-map key with `Node.id` before local view construction. Equality makes key lookup and embedded identity coincide. Inequality produces the typed witness, preventing validation from reporting facts about one identifier while serialization records another.

### Executable counterpart

- Law ID: `segments.s10`
- Reference: `panproto_reference::segment::validate_segment_contract`
- Bounded witness: `map keys=1, embedded node ids=1, mismatched pairs=1`
- Exhaustive differential test: `partition_and_decoder_refusals_preserve_typed_error_precedence; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=partition_and_decoder_refusals_preserve_typed_error_precedence)'`
- Property differential test: `segment::tests::validation::validate_segment_returns_every_contract_violation_before_validation; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::validation::validate_segment_returns_every_contract_violation_before_validation)'`
- File: `crates/panproto-inst/src/segment/check.rs`
- File: `crates/panproto-inst/src/segment/validate.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

### Mutation equivalences

None. Omitting the check changes both the addressed validation subject and the canonical identity of the segment.

### Audit record

- Author: segment correctness author
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification are pending.

## Validation-view capacity is checked before construction {#L-segments-8}

### Statement

Let $C = 2^{32}$, the cardinality of the `EdgeId` carrier. A graph or tree segment with $e$ owned view edges and $s$ incoming stubs is representable for local validation exactly when $e+s \le C$. The sum is computed in `u64` before any temporary edge identifier is assigned. A larger carrier is refused as `ValidationViewUnrepresentable` with the exact total.

### Outside the domain

Tables do not construct this local graph view and thus do not use the preflight. Segment-contract errors precede capacity checking.

### Definitions used

An *owned view edge* is an owned graph edge or embedded tree edge visible to validation. Incoming stubs require distinct temporary identifiers after the owned edge identifiers.

### Proof

There are exactly $C$ distinct `u32` edge identifiers. Assigning one identifier to every owned edge and stub is injective precisely when their count does not exceed $C$. Counting in `u64` distinguishes $C$ from $C+1$, and the preflight runs before conversion to `u32`, so the refusal cannot alias the first identifier.

### Executable counterpart

- Law ID: `segments.s10a`
- Reference: `panproto_reference::segment::validation_view_preflight`
- Boundary witness: `capacity=2^32, accepted count=capacity, refused count=capacity+1`
- Exhaustive differential test: `partition_and_decoder_refusals_preserve_typed_error_precedence; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=partition_and_decoder_refusals_preserve_typed_error_precedence)'`
- Property differential test: `segment::tests::validation::validation_view_capacity_accepts_the_full_edge_id_domain_and_refuses_the_next_edge; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::validation::validation_view_capacity_accepts_the_full_edge_id_domain_and_refuses_the_next_edge)'`
- File: `crates/panproto-inst/src/segment/validate.rs`
- File: `crates/panproto-inst/src/segment/error.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

### Mutation equivalences

None at the boundary $C$. Counting in `u32`, using a strict inequality, or omitting incoming stubs changes the admitted domain.

### Audit record

- Author: segment correctness author
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification are pending.

## Segment observation equals observation of the glued instance {#L-segments-9}

### Statement

For every valid segment family $S$ under $P$ and every deterministic whole-instance observation $f$,

$$\operatorname{segmented}(S,P,f) = f(\operatorname{glue}_P(S)).$$

The 0.77 segment-core claim uses this equation as a semantic boundary. It does not claim a checkpointed segmented runner.

### Outside the domain

The equation requires a valid partition and segment family. An invalid family returns the same structural refusal that checked assembly returns, before $f$ is called.

### Definitions used

A *whole-instance observation* is a deterministic function of the assembled instance. In the finite differential it is exact canonical identity, so the comparison retains every node, edge, row, arc, fan, root, and value.

### Proof

The reference operation defines segmented observation by checked glue followed by $f$. Production partition and assembly satisfy [L-segments-3](#L-segments-3), and exact canonical bytes agree between implementations on every carrier case. Both sides thus present the same whole instance to the observation.

### Executable counterpart

- Law ID: `segments.execution-equivalence`
- Reference: `panproto_reference::segment::segmented`
- Enumerator: `panproto_reference::enumerate::segments::graph_partition_cases; bound: graphs=25, partitions=290`
- Exhaustive differential test: `bounded_graph_partition_glue_and_segment_bytes_agree_exhaustively; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=bounded_graph_partition_glue_and_segment_bytes_agree_exhaustively)'`
- Property differential test: `segment::tests::laws::partitioning_then_assembling_returns_the_identical_instance_for_every_partition; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::laws::partitioning_then_assembling_returns_the_identical_instance_for_every_partition)'`
- File: `crates/panproto-inst/src/segment/split.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

### Mutation equivalences

None for the canonical identity observation. A projection that deliberately discards fields would be a different law.

### Audit record

- Author: segment correctness author
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification are pending.

## Allocation is append-only within each declared range {#L-segments-10}

### Statement

For an inclusive range $R$, the source floor is $R$'s lower bound when the source holds no identifier in $R$, and otherwise one more than the greatest source identifier in $R$. A fresh identifier is the greater of that floor and one more than every additional used identifier in $R$. Allocation succeeds only when that value lies in $R$ and is below `u32::MAX`; otherwise it returns exhaustion.

### Outside the domain

An inverted range is a partition error rather than an allocation input. A segmented operation that requires an undeclared allocation range returns `NoAllocationRange`. The sentinel `u32::MAX` is never allocated, including when it is the range's only member.

### Definitions used

The *source floor* includes retained and dropped source objects. The additional used set includes view, reserved, and synthesized identifiers already chosen by the operation.

### Proof

Both implementations filter source and used identifiers to $R$, take their maxima, and advance with checked or saturating addition followed by the same sentinel test. The resulting candidate is greater than every relevant identifier, so no identifier below the floor can be reused. Conversely, any smaller admissible candidate would violate the append-only order.

### Executable counterpart

- Law ID: `segments.id-floor`
- Reference: `panproto_reference::segment::fresh`
- Enumerator: `panproto_reference::enumerate::segments::graphs; bound: ranges=15, allocations=1812`
- Exhaustive differential test: `append_floor_and_exhaustion_agree_on_an_independently_counted_carrier; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=append_floor_and_exhaustion_agree_on_an_independently_counted_carrier)'`
- Property differential test: `allocation_floor_is_append_only_on_the_exact_small_carrier; command: cargo nextest run -p panproto-reference --test segments --no-tests=fail -E 'test(=allocation_floor_is_append_only_on_the_exact_small_carrier)'`
- File: `crates/panproto-inst/src/segment/ids.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

### Mutation equivalences

Identifiers outside $R$ are observationally irrelevant to this allocator. Changing the maximum within $R$, admitting `u32::MAX`, or choosing a least absent identifier below the floor is not equivalent.

### Audit record

- Author: segment correctness author
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification are pending.
