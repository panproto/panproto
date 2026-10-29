# Segments

A large instance may need to be stored in pieces without changing the instance it denotes. Segment correctness fixes the conditions under which this is possible: identifiers remain global, each node has one owner, boundary records describe every crossing edge, and assembling the pieces recovers the original value. These conditions form the *segment agreement*. The laws below cover the segment foundation only; execution runners, storage backends, checkpoints, and segmented edits belong to later components.

Let $I$ be a finite tree, table, or graph instance, and let $P$ be a partition for the same shape. We write $P(n)$ for the segment that owns node or row identifier $n$, $\operatorname{split}_P(I)$ for partitioning, and $\operatorname{glue}_P$ for checked assembly. An edge is owned by the owner of its source endpoint. The finite differential carrier contains 25 labelled graphs and all 290 assignments of their nodes to two or three labelled segments, together with two fan-bearing trees and all 40 assignments of their tree and fan nodes to two segments; one crossing-boundary witness for each instance shape, and one for each fan-bearing tree, checks the shape-specific representation.

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
- Enumerator: `panproto_reference::enumerate::segments::shape_witnesses; bound: shapes=3, witnesses=5, segments=2`
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

The equation is stated only when every element is owned, the tree and its embedding agree, and the partition and all produced segments satisfy their contracts. Assembly refuses duplicate elements, inconsistent boundaries, missing root state, or counters below a held identifier. Partitioning refuses a tree whose embedding does not agree with it, reporting `NotEmbedded` at the first element that breaks agreement; a tree whose fan parent edge identifiers do not ascend in fan order is refused at the first fan node out of order.

### Definitions used

*Exact tree equality* pairs a tree with its `TreeEmbedding`; equality of the tree alone would omit the identifiers required by graph transport and later edits.

A tree and its embedding *agree* when every node key equals its node identifier; the root is a node anchored at the schema root and reaches every node along arcs, each node having at most one parent; every arc has an edge identifier, and these ascend in arc order; every fan has a fan node that is no tree node, an edge identifier for its parent and for each of its labels, and endpoints that are tree nodes; fan parent edge identifiers ascend in fan order; no edge identifier is used twice; and both counters lie above every node and edge identifier the embedding uses. The initial embedding of a tree allocates each fan's edges after every arc and after the fans before it, and a tree edit appends new fans with fresh identifiers and keeps the edge identifiers of a relocated fan node, so every embedding produced from a tree keeps fan parent edges ascending.

### Proof

Partitioning places each node and each source-owned edge in exactly one segment by [L-segments-1](#L-segments-1). Assembly verifies the boundaries from [L-segments-2](#L-segments-2), then unions the disjoint node maps and merges edge records in their canonical order. The root owner retains the tree root and counters, so no tree presentation data is lost.

Fan order needs its own argument, because a segment lists its fans by fan node identifier and carries no global fan position. Fan node identifiers cannot stand for fan order: relocating a fan node gives it a fresh identifier without moving the fan. Partitioning copies each fan's embedding (its node, parent edge, and labelled child edges) to the owner of its fan node. Assembly collects every segment's fans and sorts them by parent edge identifier. Edge identifiers are unique across segments (assembly checks this before it merges), so the sort is a total order, and agreement makes it the original fan order. Each fan is rebuilt from the targets of its own edges, so the tree's fans and the embedding's fans are recovered in the same order.

### Executable counterpart

- Law ID: `segments.sg3`
- Reference: `panproto_reference::segment::partition`
- Reference: `panproto_reference::segment::glue`
- Enumerator: `panproto_reference::enumerate::segments::partition_cases; bound: graphs=25, fan_trees=2, partitions=330`
- Exhaustive differential test: `bounded_graph_and_fan_tree_partitions_glue_and_agree_exhaustively; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=bounded_graph_and_fan_tree_partitions_glue_and_agree_exhaustively)'`
- Property differential test: `segment::tests::laws::partitioning_then_assembling_returns_the_identical_instance_for_every_partition; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::laws::partitioning_then_assembling_returns_the_identical_instance_for_every_partition)'`
- File: `crates/panproto-inst/src/segment/split.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

### Mutation equivalences

None. Any change to an owned element, boundary record, root, ordering identifier, or embedding counter is observable after assembly. In particular, ordering fans by fan node identifier in place of parent edge identifier changes the assembled fan order of the second fan-bearing tree, whose fan node identifiers run opposite to its fan order.

### Audit record

- Author: segment correctness author
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification are pending.

## Canonical segment artifacts bind all semantic inputs {#L-segments-4}

### Statement

Canonical segment bytes contain the format version, shape, schema digest, segment specification, complete owned body, and complete boundary in a fixed order. A tree body writes each fan as its fan node, parent edge, and labelled child edges, in ascending fan node order, and writes no global fan position. Segment digests derive from exactly those bytes under the segment context. Manifest records bind the segment digest, counts, greatest identifiers, owned edge-identifier run, and crossing-edge digests; the manifest root and instance digest are deterministic functions of those records and their streams.

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
- Enumerator: `panproto_reference::enumerate::segments::shape_witnesses; bound: shapes=3, witnesses=5, segments=2`
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

The six phase names and seven artifact-kind names have stable snake-case serde forms. Task keys preserve both coordinator and segment forms. `TaskRecord::inputs_digest` binds a lens identity, a task key, and a sorted multiset of input-artifact digests. `RunJournal::reusable` returns true exactly when the journal contains that task with the requested input and output digests. The production `RunJournal::run_digest` changes on the witnessed carrier when its lens, input manifest, partition, or operation bytes change. These contracts are limited to serde names, task-key forms, digest inputs, and reuse predicates; they assert no runner or storage-backend law.

### Outside the domain

Unknown serde variants and malformed task keys are outside the accepted layout. The reuse predicate accepts an artifact digest supplied by its caller; it does not read artifact bytes or validate the recorded byte count. This law does not claim a canonical production journal-content encoding, equality with the reference journal encoding, transactional behavior of a concrete store, or the scheduling behavior of a runner.

### Definitions used

The *storage vocabulary* is the public data model for segmented task identity and journals. Layout stability here concerns serde names and task-key forms. The helper contract covers `TaskRecord::inputs_digest` and `RunJournal::reusable`; the input sensitivity of `RunJournal::run_digest` has production-only evidence. The law makes no claim about implementation-specific memory layout or canonical journal-content bytes.

### Proof

The Rust declarations determine the serde tags. `TaskKey` carries an optional segment, so its two forms are observed directly in a serialized journal. The reference `task_inputs_digest` writes the lens bytes, an explicit phase tag, the segment-presence field and identifier, a `u64` input count, and the sorted digest bytes under `SEGMENT_TASK_CONTEXT`. The independent `journal_task_reusable` helper performs one ordered-map lookup and compares only the recorded input and output digests. The bounded differential compares both helpers with their public production counterparts over every phase, coordinator and boundary segment identifiers, empty and repeated input carriers, and both boolean results. The production `RunJournal::run_digest` separately writes the lens and input digests, canonical partition bytes, and operation bytes under `RUN_JOURNAL_CONTEXT`; its four mutation checks are production-only.

### Executable counterpart

- Law ID: `segments.sg7-types`
- Reference: `panproto_reference::segment::task_inputs_digest`
- Reference: `panproto_reference::segment::journal_task_reusable`
- Evidence scope: `phases=6, artifact kinds=7, task-input digest cases=180 (lens digests=2, task keys=18, input lists=5), reuse cases=55, populated serde task keys=2, production run-identity input families=4`
- Production certificate: `storage_vocabulary_serde_layout_and_run_identity_are_stable; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=storage_vocabulary_serde_layout_and_run_identity_are_stable)'`
- Independent reference anchor: `bounded_task_input_digest_and_journal_reuse_match_the_reference; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=bounded_task_input_digest_and_journal_reuse_match_the_reference)'`
- Exhaustive differential test: `bounded_task_input_digest_and_journal_reuse_match_the_reference; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=bounded_task_input_digest_and_journal_reuse_match_the_reference)'`
- File: `crates/panproto-inst/src/segment/store.rs`
- File: `crates/panproto-inst/src/segment/partition.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

### Mutation equivalences

Permuting input digests is equivalent for `TaskRecord::inputs_digest`; changing the multiset, including a duplicate count, is not. `RunJournal::reusable` does not observe the run digest or a task record's byte count, while a changed task key, input digest, or output digest changes its result on the witnessed carrier. No equivalence applies to the observed serde tags, task-key forms, or production run-identity inputs. This law makes no claim about canonical production journal-content bytes.

### Audit record

- Author: segment correctness author
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and mutation verification are pending.

## Segment refusals preserve variant and witness data {#L-segments-6}

### Statement

For a fixed invalid input, segment operations return the same typed error variant and structured witness in every run. Structural validation precedes artifact and operation checks. Within one stage, errors are ordered by segment identifier, element reference, and the variant-specific precedence key. A segment that breaks its contract returns `Invalid` listing every violation, sorted by element: an owned edge whose source is owned elsewhere is still checked at its target. Tree counters held without the root name no element, so the contract reports them as `MalformedPartition` with the `CountersWithoutRoot` defect before any element-located violation. Every segment entry point checks a segment's shape against the partition before it validates the partition, so a segment of the wrong shape under an invalid partition is a `ShapeMismatch`. Partitioning a whole tree whose node key differs from its node identifier returns `NotEmbedded` at that node, since such a tree does not agree with its embedding.

### Outside the domain

Free-text messages of storage failures are supplied by an external store and are not compared here, and the message of a `Decode` refusal is free text; its variant and artifact are compared. This law covers the segment-core variants and their structured payloads; later runner reports add their own status laws.

### Definitions used

A *typed witness* is the complete payload of a `SegmentError`, including element references, owners, ranges, digests, and every violation in an `Invalid` value.

### Proof

Partition and segment checks traverse ordered carriers, accumulate local violations in canonical order, and select the first structural error before decoding later artifacts. The differential maps each reference error field for field into the production vocabulary and compares the complete values, so it observes branch selection, the order of an `Invalid` list, and every payload field. The labelled refusal carrier covers each stage once: partition validation, splitting, and the segment contract, including a segment that breaks four contract clauses at once, a segment of the wrong shape under an invalid partition, and a whole tree whose node key differs from its node identifier.

### Executable counterpart

- Law ID: `segments.sg9`
- Reference: `panproto_reference::segment::decode_segment_bytes`
- Reference: `panproto_reference::segment::partition`
- Reference: `panproto_reference::segment::validate_segment_contract`
- Enumerator: `panproto_reference::enumerate::segments::refusal_cases; bound: refusals=11`
- Exhaustive differential test: `partition_and_decoder_refusals_preserve_typed_error_precedence; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=partition_and_decoder_refusals_preserve_typed_error_precedence)'`
- Property differential test: `segment::tests::segments::invalid_segments_list_every_violation_sorted; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::segments::invalid_segments_list_every_violation_sorted)'`
- File: `crates/panproto-inst/src/segment/error.rs`
- File: `crates/panproto-inst/src/segment/check.rs`
- File: `crates/panproto-inst/src/segment/bytes.rs`
- File: `crates/panproto-inst/src/segment/split.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

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

The contract check compares every ordered node-map key with `Node.id` before local view construction. Equality makes key lookup and embedded identity coincide. Inequality produces the typed witness, preventing validation from reporting facts about one identifier while serialization records another. The bounded differential compares the complete `Invalid { violations: [NodeIdMismatch { key, found }] }` value in both implementations.

### Executable counterpart

- Law ID: `segments.s10`
- Reference: `panproto_reference::segment::validate_segment_contract`
- Evidence scope: `map keys=1, embedded node ids=1, mismatched pairs=1`
- Production certificate: `partition_and_decoder_refusals_preserve_typed_error_precedence; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=partition_and_decoder_refusals_preserve_typed_error_precedence)'`
- Independent reference anchor: `partition_and_decoder_refusals_preserve_typed_error_precedence; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=partition_and_decoder_refusals_preserve_typed_error_precedence)'`
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

Let $C = 2^{32}$, the cardinality of the `EdgeId` carrier. A graph or tree segment with $e$ owned view edges and $s$ incoming stubs is representable for local validation exactly when $e+s \le C$. The sum is computed in `u64` before any temporary edge identifier is assigned. A larger carrier is refused as `ValidationViewUnrepresentable` with the exact total. The executable evidence checks this boundary in two implementations, but it does not expose a shared production-reference seam.

### Outside the domain

Tables do not construct this local graph view and thus do not use the preflight. Segment-contract errors precede capacity checking.

### Definitions used

An *owned view edge* is an owned graph edge or embedded tree edge visible to validation. Incoming stubs require distinct temporary identifiers after the owned edge identifiers.

### Proof

There are exactly $C$ distinct `u32` edge identifiers. Assigning one identifier to every owned edge and stub is injective precisely when their count does not exceed $C$. Counting in `u64` distinguishes $C$ from $C+1$, and the preflight runs before conversion to `u32`, so the refusal cannot alias the first identifier.

### Executable counterpart

- Law ID: `segments.s10a`
- Reference: `panproto_reference::segment::validation_view_preflight`
- Evidence scope: `capacity=2^32, accepted count=capacity, refused count=capacity+1`
- Production certificate: `segment::tests::validation::validation_view_capacity_accepts_the_full_edge_id_domain_and_refuses_the_next_edge; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::validation::validation_view_capacity_accepts_the_full_edge_id_domain_and_refuses_the_next_edge)'`
- Independent reference anchor: `partition_and_decoder_refusals_preserve_typed_error_precedence; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=partition_and_decoder_refusals_preserve_typed_error_precedence)'`
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

For every valid segment family $S$ under $P$ that seals, and every deterministic whole-instance observation $f$,

$$\operatorname{segmented}(S,P,f) = f(\operatorname{glue}_P(S)).$$

A family *seals* when every declared edge allocation range of $P$ starts above the greatest edge identifier the family owns. The certificate uses this equation as its semantic boundary and makes no claim about a checkpointed segmented runner.

### Outside the domain

The equation requires a valid partition and segment family. An invalid family returns the same structural refusal that checked assembly returns, before $f$ is called. A valid family that does not seal assembles, but the segmented run refuses it with `AllocationBelowFloor` for the first segment whose edge allocation range starts at or below that greatest edge identifier, and production sealing returns the same refusal. Partition validation cannot see this condition, since it sees no identifiers.

### Definitions used

A *whole-instance observation* is a deterministic function of the assembled instance. In the finite differential it is exact canonical identity, so the comparison retains every node, edge, row, arc, fan, root, and value.

### Proof

The reference run validates every segment, assembles the family by checked glue, observes the glued instance with $f$, and seals the manifest. Validation and glue refuse exactly the invalid families. Sealing refuses exactly the families that do not seal, and on a family that seals it returns a manifest without changing the observed value. Production partition and assembly satisfy [L-segments-3](#L-segments-3), and exact canonical bytes agree between implementations on every carrier case, as do sealed manifests and seal refusals. Both sides thus present the same whole instance to the observation.

### Executable counterpart

- Law ID: `segments.execution-equivalence`
- Reference: `panproto_reference::segment::segmented`
- Enumerator: `panproto_reference::enumerate::segments::allocated_graph_partition_cases; bound: graphs=25, partitions=290, allocations=3`
- Exhaustive differential test: `segmented_observation_is_the_glued_observation_or_the_seal_floor_refusal; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=segmented_observation_is_the_glued_observation_or_the_seal_floor_refusal)'`
- Property differential test: `segment::tests::laws::partitioning_then_assembling_returns_the_identical_instance_for_every_partition; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::laws::partitioning_then_assembling_returns_the_identical_instance_for_every_partition)'`
- File: `crates/panproto-inst/src/segment/split.rs`
- File: `crates/panproto-inst/src/segment/manifest.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

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

Both implementations filter source and used identifiers to $R$, take their maxima, and advance with checked or saturating addition followed by the same sentinel test. The resulting candidate is greater than every relevant identifier, so no identifier below the floor can be reused. Conversely, any smaller admissible candidate would violate the append-only order. The carrier draws source and used sets from every subset of $[0, 5]$ for each range inside $[0, 4]$, so identifiers below, inside, and above the range all occur; an allocator that let a source identifier outside $R$ raise the floor disagrees with both implementations on the carrier.

### Executable counterpart

- Law ID: `segments.id-floor`
- Reference: `panproto_reference::segment::fresh`
- Enumerator: `panproto_reference::enumerate::segments::allocation_cases; bound: ranges=15, identifiers=6, allocations=61440`
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
