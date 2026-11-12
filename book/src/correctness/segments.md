# Segments

A large instance may need to be stored in pieces without changing the instance it denotes. Segment correctness fixes the conditions under which this is possible: identifiers remain global, each node has one owner, boundary records describe every crossing edge, and assembling the pieces recovers the original value. These conditions form the *segment agreement*. The first ten laws below cover the segment foundation; the eleventh runs graph restriction segment by segment, the twelfth runs it as a checkpointed sequence of tasks that resumes after any interruption, the thirteenth fixes the record frames through which that run seals its output without holding a view segment, the fourteenth reads a complement segment back from the bytes that run writes, the fifteenth runs the get of a graph lens as that run, and the sixteenth runs its put segment by segment. Storage backends and segmented edits belong to later components.

Let $I$ be a finite tree, table, or graph instance, and let $P$ be a partition for the same shape. We write $P(n)$ for the segment that owns node or row identifier $n$, $\operatorname{split}_P(I)$ for partitioning, and $\operatorname{glue}_P$ for checked assembly. An edge is owned by the owner of its source endpoint. The finite differential carrier contains 25 labelled graphs and all 290 assignments of their nodes to two or three labelled segments, together with two fan-bearing trees and all 40 assignments of their tree and fan nodes to two segments; one crossing-boundary witness for each instance shape, and one for each fan-bearing tree, checks the shape-specific representation.

First, the page establishes ownership, boundary, and assembly laws. Second, it fixes canonical artifacts, the storage vocabulary, and typed refusals. Third, it states the validation and allocation boundaries that the segment core must preserve. Fourth, it shows that graph restriction run segment by segment, with one boundary exchange and a reachability summary, assembles to whole-state restriction, that the checkpointed run of it reports and resumes as its reference runner does, that the record frames its Seal reads decode to the records they encode, that the complement segments it writes decode to the complements they encode, that the get of a graph lens run this way assembles to its whole-state get, and that its put, run segment by segment over the view and complement segments that get writes, assembles to its whole-state put.

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
- Property differential test: `randomized::boundary_cut_matches_the_reference_on_generated_shape_choices; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=randomized::boundary_cut_matches_the_reference_on_generated_shape_choices)'`
- Property scope: `five documented shape witnesses; paired partition, glue, segment bytes, manifest bytes, digest and authenticated verification; configured successful cases per seed=256`
- File: `crates/panproto-inst/src/segment/partition.rs`
- File: `crates/panproto-inst/src/segment/split.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

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
- Property differential test: `randomized::boundary_cut_matches_the_reference_on_generated_shape_choices; command: cargo +1.99 nextest run --cargo-profile ci-test --profile ci -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=randomized::boundary_cut_matches_the_reference_on_generated_shape_choices)'`
- Fixed regression certificate: `segment::tests::segments::partition_verification_rejects_ghost_mismatch_duplicate_edge_ids_and_tree_cycles; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::segments::partition_verification_rejects_ghost_mismatch_duplicate_edge_ids_and_tree_cycles)'`

- File: `crates/panproto-inst/src/segment/body.rs`
- File: `crates/panproto-inst/src/segment/check.rs`
- File: `crates/panproto-inst/src/segment/split.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

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
- Property differential test: `randomized::boundary_cut_matches_the_reference_on_generated_shape_choices; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=randomized::boundary_cut_matches_the_reference_on_generated_shape_choices)'`
- Property scope: `five documented shape witnesses; paired partition, glue, segment bytes, manifest bytes, digest and authenticated verification; configured successful cases per seed=256`
- File: `crates/panproto-inst/src/segment/split.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

## Canonical segment artifacts bind all semantic inputs {#L-segments-4}

### Statement

Canonical segment bytes contain the format version, shape, schema digest, segment specification, complete owned body, and complete boundary in a fixed order. A tree body writes each fan as its fan node, parent edge, and labelled child edges, in ascending fan node order, and writes no global fan position. Segment digests derive from exactly those bytes under the segment context. Manifest records bind the segment digest, counts, greatest identifiers, owned edge-identifier run, and crossing-edge digests; the manifest root and instance digest are deterministic functions of those records and their streams. Manifest verification also takes a trusted expected manifest digest and compares it with the complete canonical manifest, including its schema digest and other header fields.

### Outside the domain

Segment-contract checking refuses malformed ownership, endpoints and boundary records. Sealing checks the records’ partition and crossing-digest pairs, then the committed edge-id runs against their records, and then refuses the least duplicate edge identifier and, failing that, the least edge identifier the seal floor rule of [L-segments-9](#L-segments-9) refuses. Full sealing additionally refuses record streams whose shape, order, counts or stated statistics disagree. After validating the manifest layout version, manifest verification checks the partition, statistics identifiers, crossing pairs and Merkle root in that order, then refuses a full digest that differs from the trusted expected identity. A changed schema digest is thus refused even when the records and Merkle root remain unchanged. Verification of a supplied segment separately checks its contract and its complete record under the manifest schema. Decoding refuses malformed or noncanonical encodings, including truncated data, trailing bytes and invalid ordering.

### Definitions used

A *crossing digest* hashes the canonical records for one ordered pair of source and target segments. The manifest root is the domain-separated binary Merkle root over canonical record bytes. The expected manifest identity comes from a trusted artifact reference or an original sealed manifest; hashing the supplied manifest itself does not establish that expectation.

### Proof

Every byte writer traverses fields in declaration order and ordered collections in key order. Segment verification recomputes its complete record. After validating the manifest layout version, manifest verification checks the partition, then every statistics identifier. A statistics identifier that differs from its record identifier produces `MalformedPartition` with `NotDense` carrying the actual statistics identifier. It next checks crossing pairs in record order: outgoing entries precede incoming entries, with peer identifiers ascending within each map. A self-cross is refused even when its two digests agree, since the cut contains only edges whose owners differ. After those checks, verification recomputes the Merkle root and compares the digest of all canonical manifest bytes with the expected identity, without reading a segment body. The schema digest occurs in those bytes, so a header change is covered by the comparison even when every record is unchanged. Sealing merges the committed edge-id runs in one ascending pass that holds one identifier per run, so it meets the least duplicate identifier and the least identifier outside the seal floor rule in that pass, without holding any segment. Streaming instance records merge in the same canonical order as whole-instance writers, which gives the same instance digest without assembly.

### Executable counterpart

- Law ID: `segments.sg6`
- Reference: `panproto_reference::segment::canonical_manifest_bytes`
- Reference: `panproto_reference::segment::canonical_segment_bytes`
- Reference: `panproto_reference::segment::seal_manifest`
- Reference: `panproto_reference::segment::verify_manifest`
- Enumerator: `panproto_reference::enumerate::segments::shape_witnesses; bound: shapes=3, witnesses=5, segments=2`
- Exhaustive differential test: `graph_table_and_tree_artifacts_match_the_reference_exactly; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=graph_table_and_tree_artifacts_match_the_reference_exactly)'`
- Property differential test: `segment::tests::laws::streaming_instance_digest_equals_the_digest_of_the_assembled_instance; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::laws::streaming_instance_digest_equals_the_digest_of_the_assembled_instance)'`
- Production certificate: `manifest_header_schema_mutation_preserves_records_but_refuses_original_identity; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=manifest_header_schema_mutation_preserves_records_but_refuses_original_identity)'`
- Production certificate: `manifest_header_schema_mutation_must_refuse_the_original_identity; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=manifest_header_schema_mutation_must_refuse_the_original_identity)'`
- Production certificate: `manifest_cross_refusal_precedes_root_and_identity_refusals; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=manifest_cross_refusal_precedes_root_and_identity_refusals)'`
- Production certificate: `manifest_root_refusal_precedes_identity_refusal_when_cross_pairs_agree; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=manifest_root_refusal_precedes_identity_refusal_when_cross_pairs_agree)'`
- Production certificate: `malformed_manifests_preserve_the_complete_structural_refusal_order; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=malformed_manifests_preserve_the_complete_structural_refusal_order)'`
- Property differential test: `randomized::boundary_cut_matches_the_reference_on_generated_shape_choices; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=randomized::boundary_cut_matches_the_reference_on_generated_shape_choices)'`
- Property scope: `five documented shape witnesses; paired partition, glue, segment bytes, manifest bytes, digest and authenticated verification; configured successful cases per seed=256`
- Property differential test: `randomized::manifest_refusals_match_on_generated_structural_mutations; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=randomized::manifest_refusals_match_on_generated_structural_mutations)'`
- Property scope: `six existing malformed-manifest cases with rebuilt roots and the original trusted identity; configured successful cases per seed=256`
- File: `crates/panproto-inst/src/segment/bytes.rs`
- File: `crates/panproto-inst/src/segment/manifest.rs`
- File: `crates/panproto-inst/src/segment/stream.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

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
- Property differential test: `randomized::task_digest_and_reuse_match_the_reference_on_generated_carrier_choices; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=randomized::task_digest_and_reuse_match_the_reference_on_generated_carrier_choices)'`
- Property scope: `six phases, three segment choices, five input lists and two lens digests; matching, changed-input, changed-output and absent-task reuse queries; configured successful cases per seed=256`
- File: `crates/panproto-inst/src/segment/store.rs`
- File: `crates/panproto-inst/src/segment/partition.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

## Segment refusals preserve variant and witness data {#L-segments-6}

### Statement

For a fixed invalid input, segment operations return the same typed error variant and structured witness in every run. Structural validation precedes artifact and operation checks. Within one stage, errors are ordered by segment identifier, element reference, and the variant-specific precedence key. For manifest crossing errors, the ordering segment is the record being checked: outgoing entries precede incoming entries, and each map is visited by ascending peer identifier. A segment that breaks its contract returns `Invalid` listing every violation, sorted by element: an owned edge whose source is owned elsewhere is still checked at its target. Tree counters held without the root name no element, so the contract reports them as `MalformedPartition` with the `CountersWithoutRoot` defect before any element-located violation. Every segment entry point checks a segment's shape against the partition before it validates the partition, so a segment of the wrong shape under an invalid partition is a `ShapeMismatch`. Partitioning a whole tree whose node key differs from its node identifier returns `NotEmbedded` at that node, since such a tree does not agree with its embedding.

### Outside the domain

Free-text messages of storage failures are supplied by an external store and are not compared here, and the message of a `Decode` refusal is free text; its variant and artifact are compared. This law covers the segment-core variants and their structured payloads; later runner reports add their own status laws.

### Definitions used

A *typed witness* is the complete payload of a `SegmentError`, including element references, owners, ranges, digests, and every violation in an `Invalid` value.

### Proof

Partition and segment checks traverse ordered carriers, accumulate local violations in canonical order, and select the first structural error before decoding later artifacts. The differential maps each reference error field for field into the production vocabulary and compares the complete values, so it observes branch selection, the order of an `Invalid` list, and every payload field. The labelled refusal carrier covers each stage once: partition validation, splitting, and the segment contract, including a segment that breaks four contract clauses at once, a segment of the wrong shape under an invalid partition, and a whole tree whose node key differs from its node identifier. A separate six-case manifest matrix checks reciprocal self-crosses, the crossing traversal tie, a statistics-identifier mismatch, a partition defect together with a statistics defect, a later statistics defect together with an earlier crossing defect, and an isolated density defect.

### Executable counterpart

- Law ID: `segments.sg9`
- Reference: `panproto_reference::segment::decode_segment_bytes`
- Reference: `panproto_reference::segment::partition`
- Reference: `panproto_reference::segment::validate_segment_contract`
- Reference: `panproto_reference::segment::verify_manifest`
- Enumerator: `panproto_reference::enumerate::segments::refusal_cases; bound: refusals=11`
- Exhaustive differential test: `partition_and_decoder_refusals_preserve_typed_error_precedence; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=partition_and_decoder_refusals_preserve_typed_error_precedence)'`
- Property differential test: `randomized::typed_refusals_match_the_reference_on_generated_carrier_choices; command: cargo +1.99 nextest run --cargo-profile ci-test --profile ci -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=randomized::typed_refusals_match_the_reference_on_generated_carrier_choices)'`
- Fixed regression certificate: `segment::tests::segments::invalid_segments_list_every_violation_sorted; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::segments::invalid_segments_list_every_violation_sorted)'`
- Production certificate: `malformed_manifests_preserve_the_complete_structural_refusal_order; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=malformed_manifests_preserve_the_complete_structural_refusal_order)'`
- File: `crates/panproto-inst/src/segment/manifest.rs`
- File: `crates/panproto-inst/src/segment/error.rs`
- File: `crates/panproto-inst/src/segment/check.rs`
- File: `crates/panproto-inst/src/segment/bytes.rs`
- File: `crates/panproto-inst/src/segment/split.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

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
- Property differential test: `randomized::node_identity_refusals_match_the_reference_on_generated_identifiers; command: cargo +1.99 nextest run --cargo-profile ci-test --profile ci -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=randomized::node_identity_refusals_match_the_reference_on_generated_identifiers)'`
- Property scope: `map keys=0..4, embedded node id=key+offset, offsets=1..5, identifier pairs=25`
- Fixed regression certificate: `segment::tests::validation::validate_segment_returns_every_contract_violation_before_validation; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::validation::validate_segment_returns_every_contract_violation_before_validation)'`
- File: `crates/panproto-inst/src/segment/check.rs`
- File: `crates/panproto-inst/src/segment/validate.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

## Validation-view capacity is checked before construction {#L-segments-8}

### Statement

Let $C = 2^{32}$, the cardinality of the `EdgeId` carrier. A graph or tree segment with $e$ owned view edges and $s$ incoming stubs is representable for local validation exactly when $e+s \le C$. The sum is computed in `u64` before any temporary edge identifier is assigned. A larger carrier is refused as `ValidationViewUnrepresentable` with the exact total. The executable evidence calls both capacity checks on the same counts and compares the exact segment identifier and refused total.

### Outside the domain

Tables do not construct this local graph view and thus do not use the preflight. Segment-contract errors precede capacity checking.

### Definitions used

An *owned view edge* is an owned graph edge or embedded tree edge visible to validation. Incoming stubs require distinct temporary identifiers after the owned edge identifiers.

### Proof

There are exactly $C$ distinct `u32` edge identifiers. Assigning one identifier to every owned edge and stub is injective precisely when their count does not exceed $C$. Counting in `u64` distinguishes $C$ from $C+1$, and the preflight runs before conversion to `u32`, so the refusal cannot alias the first identifier.

### Executable counterpart

- Law ID: `segments.s10a`
- Reference: `panproto_reference::segment::validation_view_preflight`
- Reference: `panproto_reference::segment::validate_segment_contract`
- Evidence scope: `capacity=2^32, boundary pairs=10, generated count pairs=512 per run with each count from 0 through capacity+1; valid, key/id mismatch and shape-mismatch controls on a one-edge graph with injected capacities 0 and 1`
- Production certificate: `segment::tests::validation::validation_view_capacity_accepts_the_full_edge_id_domain_and_refuses_the_next_edge; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::validation::validation_view_capacity_accepts_the_full_edge_id_domain_and_refuses_the_next_edge)'`
- Independent reference anchor: `partition_and_decoder_refusals_preserve_typed_error_precedence; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=partition_and_decoder_refusals_preserve_typed_error_precedence)'`
- Exhaustive differential test: `segment::tests::validation::validation_view_capacity_accepts_the_full_edge_id_domain_and_refuses_the_next_edge; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::validation::validation_view_capacity_accepts_the_full_edge_id_domain_and_refuses_the_next_edge)'`
- Property differential test: `segment::tests::validation::validation_view_capacity_agrees_with_the_reference; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::validation::validation_view_capacity_agrees_with_the_reference)'`
- Production certificate: `segment::tests::validation::validation_view_capacity_preserves_contract_error_precedence; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::validation::validation_view_capacity_preserves_contract_error_precedence)'`
- Production certificate: `segment::tests::validation::validation_view_capacity_reaches_only_valid_contracts; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::validation::validation_view_capacity_reaches_only_valid_contracts)'`
- Production certificate: `segment::tests::validation::validate_segment_runs_the_capacity_preflight_before_constructing_the_view; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::validation::validate_segment_runs_the_capacity_preflight_before_constructing_the_view)'`
- File: `crates/panproto-inst/src/segment/tests/validation.rs`
- File: `crates/panproto-inst/src/segment/validate.rs`
- File: `crates/panproto-inst/src/segment/error.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

## Segment observation equals observation of the glued instance {#L-segments-9}

### Statement

For every valid segment family $S$ under $P$ that seals, and every deterministic whole-instance observation $f$,

$$\operatorname{segmented}(S,P,f) = f(\operatorname{glue}_P(S)).$$

The *floor* of $P$ is the least lower bound of its declared edge allocation ranges. A family *seals* when every edge identifier it holds at or above the floor lies in the edge allocation range of the segment holding it. Edge allocation ranges are pairwise disjoint, so in a family that seals an identifier inside the range $R_i$ of segment $i$ is held only by segment $i$, and every identifier outside every range lies below every range. A family whose edge identifiers all lie below the floor seals, and so does a family whose identifiers at or above the floor were each allocated by the segment holding them from its own range. The certificate uses this equation as its semantic boundary and makes no claim about a checkpointed segmented runner.

### Outside the domain

The equation requires a valid partition and segment family. An invalid family returns the same structural refusal that checked assembly returns, before $f$ is called. A valid family that does not seal assembles, but the segmented run refuses it at the least edge identifier the rule refuses: with `EdgeIdOutsideAllocation` naming that identifier and the segment $j$ holding it, or with `NoAllocationRange` for edges when $j$ declares no edge allocation range. Production sealing returns the same refusal. Partition validation cannot see this condition, since it sees no identifiers.

### Definitions used

A *whole-instance observation* is a deterministic function of the assembled instance. In the finite differential it is exact canonical identity, so the comparison retains every node, edge, row, arc, fan, root, and value.

### Proof

The reference run validates every segment, assembles the family by checked glue, observes the glued instance with $f$, and seals the manifest. Validation and glue refuse exactly the invalid families. Sealing refuses exactly the families that do not seal, and on a family that seals it returns a manifest without changing the observed value. Production partition and assembly satisfy [L-segments-3](#L-segments-3), and exact canonical bytes agree between implementations on every carrier case, as do sealed manifests and seal refusals. Both sides thus present the same whole instance to the observation.

The rule keeps the two properties allocation relies on. Let $S$ seal, and let segment $i$ allocate an edge identifier $e$ by the append-floor rule of [L-segments-10](#L-segments-10) over the identifiers it holds in $R_i$. Then $e$ lies in $R_i$ and exceeds every identifier segment $i$ holds there. No other segment holds an identifier in $R_i$, so $e$ differs from every identifier of the family, and allocations by different segments differ because their ranges are disjoint. Every identifier segment $i$ holds outside $R_i$ lies below the floor, hence below $R_i$ and below $e$, so $e$ exceeds every edge identifier segment $i$ holds. An edge is held with its source, so the siblings of a fresh edge (the edges leaving the same node) are all held by segment $i$, and the fresh edge follows them in ascending identifier order. Finally $e$ lies in the range of the segment holding it, so the family with $e$ appended seals again: sealing is closed under allocation. A family in which every edge allocation range starts above every edge identifier has no identifier at or above the floor, so the rule admits every family the narrower rule (every range above every identifier) admits, and it gives each such family the same manifest, since the rule adds nothing to a manifest's bytes.

### Executable counterpart

- Law ID: `segments.execution-equivalence`
- Reference: `panproto_reference::segment::segmented`
- Enumerator: `panproto_reference::enumerate::segments::allocated_graph_partition_cases; bound: graphs=25, partitions=290, allocations=3`
- Exhaustive differential test: `segmented_observation_is_the_glued_observation_or_the_seal_floor_refusal; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=segmented_observation_is_the_glued_observation_or_the_seal_floor_refusal)'`
- Property differential test: `segment::tests::laws::partitioning_then_assembling_returns_the_identical_instance_for_every_partition; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::laws::partitioning_then_assembling_returns_the_identical_instance_for_every_partition)'`
- Property differential test: `randomized::segmented_observation_matches_on_generated_allocated_partitions; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=randomized::segmented_observation_matches_on_generated_allocated_partitions)'`
- Property scope: `870 documented allocated graph partitions; exact assembled bytes or typed seal-floor refusal; configured successful cases per seed=256`
- Production certificate: `segment::ids::tests::a_fresh_edge_id_keeps_the_seal_floor_rule; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::ids::tests::a_fresh_edge_id_keeps_the_seal_floor_rule)'`
- Production certificate: `segment::tests::manifest::an_edge_id_at_or_above_the_floor_outside_its_holders_range_is_rejected_at_seal; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::manifest::an_edge_id_at_or_above_the_floor_outside_its_holders_range_is_rejected_at_seal)'`
- Production certificate: `segment::tests::manifest::an_edge_id_inside_its_holders_own_range_seals; command: cargo nextest run -p panproto-inst --lib --no-tests=fail -E 'test(=segment::tests::manifest::an_edge_id_inside_its_holders_own_range_seals)'`
- Fixed regression certificate: `seal_refuses_an_edge_id_at_or_above_the_floor_outside_its_holders_range; command: cargo nextest run -p panproto-reference --test segments --no-tests=fail -E 'test(=seal_refuses_an_edge_id_at_or_above_the_floor_outside_its_holders_range)'`
- File: `crates/panproto-inst/src/segment/ids.rs`
- File: `crates/panproto-inst/src/segment/split.rs`
- File: `crates/panproto-inst/src/segment/manifest.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

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
- Property differential test: `randomized::append_floor_matches_the_reference_on_generated_small_carriers; command: cargo +1.99 nextest run --cargo-profile ci-test --profile ci -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=randomized::append_floor_matches_the_reference_on_generated_small_carriers)'`
- Fixed regression certificate: `allocation_floor_is_append_only_on_the_exact_small_carrier; command: cargo nextest run -p panproto-reference --test segments --no-tests=fail -E 'test(=allocation_floor_is_append_only_on_the_exact_small_carrier)'`
- File: `crates/panproto-inst/src/segment/ids.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

## Segmented graph restriction assembles to whole-state restriction {#L-segments-11}

### Statement

Let $\pi$ be a compiled restrict plan whose migration carries no expression: no survival predicate, no term assignment, and no field transform other than renaming, dropping, adding or keeping fields. Let $S$ be a valid graph segment family under a partition $P$ whose edge identifiers satisfy the seal floor rule of [L-segments-9](#L-segments-9): every edge identifier at or above the least declared edge allocation range lies in the edge allocation range of the segment holding it. Then `graph_restrict_segmented` returns, for each segment $i$ of $P$, a view segment $V_i$ and a complement segment $C_i$, computed from segment $i$ and boundary facts alone, and

$$\operatorname{assemble}_P(V, C) = \operatorname{restrict}_\pi(\operatorname{glue}_P(S)),$$

where $\operatorname{restrict}_\pi$ is whole-state graph restriction and $\operatorname{assemble}_P$ glues the view segments, joins the complement segments, and numbers the synthesized elements in ascending key order from one above the greatest source identifier of each kind. The view segments form a valid family under the same partition $P$. Segment by segment, the output equals the reference definition, which restricts $\operatorname{glue}_P(S)$ whole, gives each synthesized element to the segment that owns its chain, numbers it there, and splits the result by owner. The view family $V$ satisfies the floor rule again, so segmented restrictions compose: for a second plan $\pi'$ without expressions, `graph_restrict_segmented` of $\pi'$ over $V$ under $P$ returns $(V', C')$ with

$$\operatorname{assemble}_P(V', C') = \operatorname{restrict}_{\pi'}(\operatorname{glue}_P(V)),$$

and $\operatorname{glue}_P(V)$ is $\operatorname{restrict}_\pi(\operatorname{glue}_P(S))$ with its synthesized elements numbered in their owners' ranges rather than from one above the greatest source identifier.

### Outside the domain

A plan with an expression is declined before any segment is read with the residual `Global { op: Restrict, reason: ExpressionDemand { vertex } }` at the least such source vertex; whole-state restriction evaluates such an expression with the whole instance in scope, so what it reads is not bounded by a segment. An invalid family returns the refusal of `verify_partition`. A family holding an edge identifier at or above the least edge allocation range outside the edge allocation range of the segment holding it returns, for the least such identifier, the refusal sealing gives it: `NoAllocationRange` for edges when that segment declares no edge range, and `EdgeIdOutsideAllocation` otherwise. When whole-state restriction refuses $\operatorname{glue}_P(S)$, the segmented run returns the same refusal, tagged with the segment owning the element it is about; the exception is the stage 5 exhaustion of synthesized identifiers, which the segmented run replaces by allocation within each segment's ranges. A segment that must allocate and declares no range of that kind returns `NoAllocationRange`, and one whose range runs out returns `AllocationExhausted`, in the least failing segment, nodes before edges.

### Definitions used

The *chain* of a synthesized element is the path that replaces one contracted pair or one expanded edge, and its *owner* is the segment owning the pair's first node or the edge's source. The *fate* of a node is its stage 0 decision: a fan node, a dropped plain node, or a surviving plain node with its source anchor, view anchor and recorded original anchor. An *entry* of a segment is a dropped plain node it owns that a non-fan in-stub enters. Its *reach summary* lists the surviving nodes that a search from the entry reaches through dropped plain nodes of the segment followed by one more edge, and the dropped ghosts at which the search leaves the segment.

### Proof

1. Stage 0. Without expressions the engine decides a plain node from its own anchor and payload. Each node is owned by one segment (L-segments-1), so each segment decides exactly its own nodes, as whole-state restriction would.
2. Stage 1 and the exchange. The engine decides an edge between surviving nodes from its label and the view anchors of its endpoints, and an edge with a dropped endpoint is dropped. An edge is owned with its source, and when its target is remote the source's segment holds a ghost and the target's segment an in-stub (L-segments-2). Every segment sends the fate of each in-stub target to the owner of the in-stub's source, so every segment knows the fate of each of its ghosts, and stage 1 makes the whole-state decision at each owned edge.
3. Stage 2. For a surviving node $s$, whole-state restriction pairs $s$ with every surviving $t$ reached by a path $s, d_1, \dots, d_k, t$ with $k \ge 1$ and every $d_j$ a dropped plain node, along non-fan edges. Cut such a path at each edge whose endpoints lie in different segments. Each cut enters a dropped node at a non-fan in-stub, which makes it an entry of its owner, and the part of the path inside one segment is a search the summary of that entry records; the last part ends at $t$, which the summary lists as a survivor. Conversely every survivor and every exit a summary lists lies on such a path. The coordinator's closure, in which an entry reaches what it reaches locally together with what every exit reaches, is therefore reachability through dropped nodes, by induction on the number of cuts. This is the partial evaluation and assembly of distributed reachability queries [@fanwangwu2012performance]: the ghosts are its virtual nodes, the entries its in-nodes, and the coordinator solves the possibly recursive system over the boundary. Each segment's search from its own survivors reads its local dropped nodes and, at each dropped ghost, the closed reach, so it realizes the same pairs from $s$ as the whole-state search. The anchors of every reached survivor come back with the closure.
4. Stage 3 and the fan domain. A fan node's out-edges are owned with it, so a segment decides each fan it owns from its out-edges and the fates of their targets, which step 2 delivers. Whether a node violates the fan domain depends on its shape, its out-edges and its in-edges, and each in-edge is either owned in the node's segment or an in-stub there. Every violation the whole graph has is thus found in the owner of the node, and the least one is the least among the segments' least ones.
5. Refusals. Every element belongs to one segment, and the first whole-state refusal is the one with the least key (stage, then element). The run checks the fan domain in every segment, then stage 0 in every segment, then stages 1 to 3 in every segment, and keeps the least key at each step. Stages 1 to 3 run only after every segment has passed stage 0, so the facts they read are those of a whole-state run, and the refusal returned is the whole-state one.
6. Allocation and assembly. Every element of a chain is synthesized in its owner's segment. Its nodes take identifiers from the owner's node range, which lies inside that segment's nodes, and its edges from the owner's edge range, each above every identifier of its kind the owner holds in that range (L-segments-10). No other segment holds a node in the owner's node range, which is part of the owner's nodes, and by the floor rule no other segment holds an edge identifier in the owner's edge range; ranges are disjoint. Synthesized identifiers are therefore distinct from every source identifier and from each other, so the view segments are a family under $P$, and the boundary step gives each crossing view edge its ghost and in-stub. The view family satisfies the floor rule: a surviving edge keeps its identifier and its holder (the owner of its source, which keeps its owner), and a synthesized edge lies in the edge range of the segment holding it (step 6 of the proof of [L-segments-12](#L-segments-12)). Every complement entry is keyed by an element with one owner, so the complement segments partition the whole complement. Assembly renames each synthesized element by its key to the identifier stage 5 of whole-state restriction gives it, in ascending key order from one above the greatest source identifier, and sets the floors to those values. The source identifiers are the view's unsynthesized identifiers and the dropped ones, wherever they lie relative to the ranges, so every new identifier exceeds every identifier the renaming keeps. By steps 1 to 5 the assembled view and complement are those of whole-state restriction. Composition is this equation applied to $V$, which by the previous paragraph satisfies the hypothesis.
7. Reference agreement. The reference definition computes the right-hand side of the equation directly and numbers each segment's chains by the append-floor rule of L-segments-10 in ascending key order, nodes before edges, which is the production allocation. It splits the view with the checked splitter and the complement by owner, so it gives each segment exactly the production output.
8. The finite certificate counts its carrier independently. A release schema with $e \le 4$ of the six possible edges has $\sum_{j \le \min(3, e)} \binom{e}{j}$ release graphs, giving $1 + 6 \cdot 2 + 15 \cdot 4 + 20 \cdot 8 + 15 \cdot 15 = 458$ schema and graph pairs. Each has $2^3 = 8$ survival classifications without a conditional vertex, and each three-node graph has $2^3 + 3^3 = 35$ labelled two- and three-segment ownership splits, for $458 \cdot 8 \cdot 35 = 128{,}240$ cases. Witnesses cover a hidden path through three segments with a dropped cycle, every split of a fan and an expansion, the allocation refusals (including an edge identifier outside its holder's range above the floor, and a range starting at an identifier its own segment holds, which is admitted) and the residual. The composition certificate takes every 97th case, restricts the view family of each of the $1{,}163$ that complete again along each of the eight survival classifications of its schema, $9{,}304$ second runs, and checks each against the reference and against whole-state restriction of the glued view; $15$ of the first view families hold an edge identifier their run allocated.

### Executable counterpart

- Law ID: `segments.segmented-restrict`
- Reference: `panproto_reference::segment::restrict_graph_segmented`
- Enumerator: `panproto_reference::enumerate::segments::segmented_restrict_cases; bound: graphs=458, migrations=8, partitions=35, cases=128240`
- Exhaustive differential test: `segmented::segmented_restrict_agrees_with_the_reference_and_assembles_to_whole_state_restrict; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented::segmented_restrict_agrees_with_the_reference_and_assembles_to_whole_state_restrict)'`
- Property differential test: `segmented_randomized::segmented_restrict_matches_the_reference_on_generated_partitions; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_randomized::segmented_restrict_matches_the_reference_on_generated_partitions)'`
- Property scope: `graphs of 2 to 8 nodes over a three-vertex schema with every edge and loop, up to 14 edges, every survival choice, owners from 2 to 4 segments; production and reference segments, complements and refusals, and the assembled output against whole-state restriction; 256 cases`
- Exhaustive differential test: `segmented::a_segmented_restrict_of_a_segmented_view_agrees_with_the_reference_and_assembles_to_whole_state_restrict; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented::a_segmented_restrict_of_a_segmented_view_agrees_with_the_reference_and_assembles_to_whole_state_restrict)'`
- Production certificate: `segmented_witnesses::a_hidden_path_through_three_segments_contracts_like_whole_state_restrict; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_witnesses::a_hidden_path_through_three_segments_contracts_like_whole_state_restrict)'`
- Production certificate: `segmented_witnesses::fans_and_expansions_crossing_segments_restrict_like_whole_state_restrict; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_witnesses::fans_and_expansions_crossing_segments_restrict_like_whole_state_restrict)'`
- Production certificate: `segmented_witnesses::allocation_refusals_agree_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_witnesses::allocation_refusals_agree_with_the_reference)'`
- Production certificate: `segmented_witnesses::a_survival_predicate_is_declined_as_a_global_expression_before_any_segment_is_read; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_witnesses::a_survival_predicate_is_declined_as_a_global_expression_before_any_segment_is_read)'`
- File: `crates/panproto-inst/src/segment/restrict.rs`
- File: `crates/panproto-inst/src/graph_restrict.rs`
- File: `crates/panproto-inst/tests/reference_restrict.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

## A checkpointed segmented restriction resumes to the uninterrupted run {#L-segments-12}

### Statement

Let $\pi$, $S$ and $P$ satisfy the hypotheses of [L-segments-11](#L-segments-11), let $M$ be a manifest sealing $S$, as a source or as the view of an earlier run, with trusted digest $d$, and let $B$ be a budget. Write $\operatorname{run}_c$ for `graph_restrict_run` of $\pi$ over $M$, reading $S$ from a segment source, with lens identity $\ell$, view schema $\sigma$ and budget $B$, stopped once $c$ tasks are done, and $\operatorname{ref}_c$ for the reference runner `segmented_restrict_with_progress` on the same inputs. A run of $n$ segments has $4n + 2$ tasks. For every $c$ from $0$ to $4n + 2$:

1. the report of $\operatorname{run}_c$ on an empty store equals the report of $\operatorname{ref}_c$ on an empty store: status, locality, budget, progress, peak estimate, complement accounting, tasks run and reused, orphans and per-phase accounting;
2. running again without a cancellation point against the same store gives the report of the reference run resumed the same way, reuses $\min(c, k)$ tasks, where $k$ is the number of tasks the uninterrupted run completes, and returns the status and manifest of the uninterrupted run; the store then holds the uninterrupted run's emitted artifacts and no staged artifact;
3. both runs carry the same run identity, $\operatorname{RunJournal::run\_digest}(\ell, d, P, \texttt{graph restrict})$.

When the uninterrupted run completes, the view segment of segment $i$ is the view segment the reference definition of L-segments-11 gives segment $i$, written under $\sigma$, the complement bytes of segment $i$ are the canonical complement-segment bytes of its complement segment in the reference encoding, the Emit artifact of segment $i$ is byte for byte the reference runner's, and the manifest seals the view segments as a view of $d$ under the lens envelope of $\ell$. Seal reads no view segment, and its reported peak is

$$\sum_{i} \bigl(56 + r_i + 4\,[k_i > 0] + f_i\bigr),$$

where $r_i$ is the length of the manifest record of view segment $i$, $k_i$ the number of its edge identifiers and $f_i$ the length of its longest record frame with its eight-byte length ($0$ when it has no record). Two families under the same partition whose view segments have records of the same lengths, the same cross peers and frames of the same greatest length thus have the same Seal peak, whatever the number of records each segment holds. The view manifest of a complete run, with its view segments as the segment source, is itself such an $M$ for a second run under the same partition, since the view family satisfies the hypotheses of L-segments-11.

### Outside the domain

A plan with an expression is declined with `Residual::Global` before any task. A manifest that fails verification against $d$ is refused before any task and the run has no identity. A partition with more segments than the budget's affected segments, or a record whose working-set estimate exceeds the budget, is declined before any task. A restrict refusal is returned at Assemble, after the $3n$ segment tasks of the first three phases: the refusal of L-segments-11, located in the same segment. The coordinator's bytes (the summaries and its own artifact) above the summary budget decline the run at Assemble; a running complement total above the complement budget declines it before the Emit task of the segment that exceeds it. A store failure stops the run as incomplete and reports each staged artifact the journal does not name as an orphan. Seal refuses no run: it applies the seal floor rule of [L-segments-9](#L-segments-9) to the view segments under the source's partition, and those view segments always seal (step 6 of the proof).

### Definitions used

The phases are those of the storage vocabulary ([L-segments-5](#L-segments-5)). Validate checks one input segment against $M$ and keeps its canonical bytes; Export writes the stage 0 fate of every in-stub target for the segment owning the in-stub's source, or the position of the segment's refusal; Summarize writes the segment's reach summary; Assemble closes the summaries, runs every segment's later stages and allocation one segment at a time, and writes the closure and the view boundaries; Emit writes one view segment and one complement segment, together with what Seal reads of the view; Seal writes the output manifest. The *Emit artifact* of a segment is a header of seven little-endian 64-bit words (the complement's entry count; the lengths of the view segment's canonical bytes, of the complement's canonical bytes and of the view's manifest record; the number of the view's edge identifiers; the length of its record stream; the length of its longest frame with the frame's length, or $0$) followed by those sections in that order, the edge identifiers ascending as four bytes each between the record and the stream. The *record stream* and its *frames* are those of [L-segments-13](#L-segments-13). Seal reads each header and record whole and each identifier run and record stream one identifier and one frame at a time, through ranged reads of the store; what it *holds* of an artifact is its header, its record, one identifier when there is one, and its longest frame with its length. The *coordinator's bytes* are the summed lengths of the Summarize artifacts and of the Assemble artifact. A *working-set estimate* is the `MemoryModel` estimate of a record's statistics.

### Proof

1. Task order and accounting. Both runners execute the same $4n + 2$ tasks in the same order, test the cancellation point before each task, count a task when it is committed or reused, and close a phase's accounting only when all its tasks are done. The preflight checks before the first task are the same tests in the same order, so a run declined there has the same report on both sides.
2. Identity and reuse. A task's artifact is a deterministic function of the plan, the input manifest, the segment it names and the artifacts of earlier phases, and its input digest binds the lens identity, its key and those artifacts' digests. The store commits an artifact and its journal record in one step. A run cancelled after $c$ tasks has committed exactly the first $\min(c, k)$ tasks of the uninterrupted run, with the uninterrupted run's artifacts; the resumed run reuses each of them, since its inputs and artifact are unchanged, and recomputes the rest, which by determinism writes the uninterrupted run's artifacts. Its status and manifest are therefore those of the uninterrupted run, and since every staged artifact is committed in the same step, none is left staged.
3. Outputs. Export, Summarize and Assemble compute the fan domain, stage 0, the exchange, the reach summaries and their closure, and stages 1 to 3 with allocation, each from one segment and its boundary facts, exactly as `graph_restrict_segmented` does; Emit recomputes stages 1 to 3 for one segment from the same facts. By L-segments-11 each emitted view and complement segment is the reference definition's. The boundary of each view segment comes from the Assemble artifact: every view edge whose target lies in another segment is recorded as an in-stub of the target's owner and a ghost of its holder, and the ghost's anchor is offered by the owner, because such a target is either the target of an in-stub of the source graph or a survivor that the owner's own reach summary lists (the last edge of a contracted path enters the target's segment either at the target or at an entry from which a search in that segment reaches the target).
4. Refusals. The fan domain and stage 0 are decided segment by segment in Export, and Assemble returns the least refusal among them (the least fan node first, then the least key), recomputed in its segment; it then returns the least refusal of stages 1 to 3, and failing that the allocation refusal of the least failing segment. This is the precedence of `graph_restrict_segmented`, and the reference returns the refusal of the reference definition, equal to it by L-segments-11, after the same $3n$ tasks.
5. Budgets. A record's statistics are those of its segment (L-segments-4), so the working-set estimates agree. The reference defines the reach summary of a segment and the closure directly on the glued graph and the stage 0 fates, and writes them in the same canonical layout; by step 3 of the proof of L-segments-11 they are the production summaries and closure, and the boundaries are those of the equal view segments, so the coordinator's bytes agree. The complement bytes are the canonical complement-segment bytes on both sides, so the running totals agree. The reference writes the Emit artifact from its own definitions: the manifest records of the view family without the seal floor check, the ascending identifiers of the edges each view segment owns, and frames written with its own node and edge writers. Its header therefore states the same lengths, and both sides sum the same holding for the Seal peak. That holding does not depend on the number of records: a record has fixed-width statistics, one range per declared node range and one entry per cross peer, and Seal holds one frame per stream at a time.
6. Sealing. The reference seals the view segments under $\sigma$ as a view of $d$ with the lens envelope and the input's binding. Production seals from the Emit artifacts alone: each header locates the record, the identifier run and the record stream, and the journal's recorded length of the artifact must equal the length the header states. The record, the run and the stream are those of the view segment (step 3 and L-segments-13, whose decoding recovers each record from its frame), and full sealing merges the runs and streams in one ascending pass ([L-segments-4](#L-segments-4)), so it computes the manifest of the view segments without holding one, and the manifests and their digests agree. The view segments seal under the source's partition. A view edge that is not synthesized keeps its source identifier and its holder, since an edge is held with its source and a surviving node keeps its owner, so it satisfies the rule by the hypothesis of L-segments-11. A synthesized edge takes its identifier from the edge range of its chain's owner, and the owner holds it: its source is the first node of a contracted pair or the source of an expanded edge, which the owner owns by definition, or a node synthesized along the chain, which takes its identifier from the owner's node range and so lies among the owner's nodes. Every view identifier at or above the floor therefore lies in the range of the segment holding it.
7. The finite certificate takes every 97th case of the carrier of L-segments-11, $\lceil 128{,}240 / 97 \rceil = 1{,}323$ cases, checks every cancellation point of each, and compares the Emit artifacts of each complete run byte for byte; witnesses cover each budget refusal, a store whose commit fails at each task, the residual of an expression, a contracting and an expanding run that seal, verify and resume from every cancellation point, a contracting run whose sealed view manifest and view segments are the input of a second run that completes with the output of `graph_restrict_segmented` on those views, and two graphs under one partition, with one and with three records of each kind per segment, whose Seal peaks are both the independently counted $617$ bytes while the Validate peak grows.

### Executable counterpart

- Law ID: `segments.checkpointed-restrict`
- Reference: `panproto_reference::segment::segmented_restrict_with_progress`
- Enumerator: `panproto_reference::enumerate::segments::segmented_restrict_cases; bound: every 97th case, cases=1323, cancellation points=4n+3 per case`
- Exhaustive differential test: `segmented_runner::a_checkpointed_restrict_run_matches_the_reference_runner_at_every_cancellation_point; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_runner::a_checkpointed_restrict_run_matches_the_reference_runner_at_every_cancellation_point)'`
- Property differential test: `segmented_randomized::a_checkpointed_restrict_run_matches_the_reference_runner_on_generated_partitions; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_randomized::a_checkpointed_restrict_run_matches_the_reference_runner_on_generated_partitions)'`
- Property scope: `graphs of 2 to 8 nodes over a three-vertex schema with every edge and loop, up to 14 edges, every survival choice, owners from 2 to 4 segments; production and reference reports at every cancellation point and after resuming, emitted segments and complement bytes; 64 cases`
- Production certificate: `segmented_witnesses::a_checkpointed_run_declines_an_exceeded_budget_like_the_reference_runner; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_witnesses::a_checkpointed_run_declines_an_exceeded_budget_like_the_reference_runner)'`
- Production certificate: `segmented_witnesses::a_failed_commit_reports_its_staged_artifact_and_a_resumed_run_completes_it; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_witnesses::a_failed_commit_reports_its_staged_artifact_and_a_resumed_run_completes_it)'`
- Production certificate: `segmented_witnesses::contracting_and_expanding_restrict_runs_seal_verify_and_resume_from_every_cancellation_point; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_witnesses::contracting_and_expanding_restrict_runs_seal_verify_and_resume_from_every_cancellation_point)'`
- Production certificate: `segmented_witnesses::a_survival_predicate_is_declined_as_a_global_expression_before_any_segment_is_read; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_witnesses::a_survival_predicate_is_declined_as_a_global_expression_before_any_segment_is_read)'`
- Production certificate: `segmented_witnesses::the_sealed_view_of_a_checkpointed_run_is_the_input_of_a_second_run; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_witnesses::the_sealed_view_of_a_checkpointed_run_is_the_input_of_a_second_run)'`
- Production certificate: `segmented_witnesses::seal_holds_the_same_bytes_for_graphs_with_more_records_per_segment_while_the_segment_peak_grows; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_witnesses::seal_holds_the_same_bytes_for_graphs_with_more_records_per_segment_while_the_segment_peak_grows)'`
- File: `crates/panproto-inst/src/segment/runner.rs`
- File: `crates/panproto-inst/src/segment/restrict.rs`
- File: `crates/panproto-inst/tests/reference_restrict.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`

## Record frames and manifest records decode to what they encode {#L-segments-13}

### Statement

Let $R$ be an instance record of a graph, table or tree segment: a node, an edge, a row, a pair, a tree arc with the identifier of its embedding edge, or a fan. Its *frame* $\operatorname{frame}(R)$ is a tag byte ($0$ node, $1$ edge, $2$ row, $3$ pair, $4$ arc, $5$ fan), then, for an arc, its edge identifier, then the record as the whole instance's canonical bytes write it. Then `InstanceRecord::from_frame` accepts $\operatorname{frame}(R)$ and returns a record whose frame is $\operatorname{frame}(R)$; it refuses every proper prefix of $\operatorname{frame}(R)$, $\operatorname{frame}(R)$ followed by any byte, and $\operatorname{frame}(R)$ with its tag byte replaced by any value from $6$ to $255$. The *record stream* of a segment is the frames of its records in canonical order (nodes or rows, then edges, arcs or pairs, then fans), each after its length as eight little-endian bytes; `decode_record_stream` returns records whose stream is the same bytes, and the stream is byte for byte the reference's. Likewise `SegmentRecord::from_canonical_bytes` accepts the canonical bytes of a manifest record and returns a record with the same canonical bytes, refuses every proper prefix and the bytes followed by any byte, and the record bytes of a segment are the reference's.

### Outside the domain

A frame whose payload holds a value outside the canonical value layout, or a fan whose children do not strictly ascend by label, is refused. A manifest record with an inverted range, a presence byte other than $0$ or $1$, or cross peers that do not strictly ascend is refused.

### Definitions used

The canonical record layouts are those the instance digests read (L-segments-4): a node record, an edge record $(\mathit{id}, \mathit{src}, \mathit{tgt}, \mathit{label}, \mathit{position})$, a row record $(\mathit{id}, \mathit{table}, \mathit{cells})$, a pair record $(\mathit{edge}, \mathit{src}, \mathit{tgt})$, an arc record $(\mathit{parent}, \mathit{child}, \mathit{label})$ and a fan record $(\mathit{parent}, \mathit{hyper\text{-}edge}, \mathit{children})$. A tree segment's records leave out its fan nodes and the parent and child edges of its fans, and a fan's parent and children are the targets of those edges.

### Proof

1. Totality on frames. Every field of a record layout is written by a fixed-width integer writer, a length-prefixed string writer, an option written as a presence byte and its payload, or a counted sequence of such fields. The decoder reads the tag, then, per tag, the same fields in the same order with the inverse reader of each writer. Each reader returns exactly the value its writer wrote and consumes exactly the bytes it wrote, so the decoder returns $R$ and ends at the frame's last byte; re-encoding writes the same fields, hence $\operatorname{frame}(R)$. An arc frame carries its embedding edge identifier, so the decoded arc has the identifier as well as the parent, child and label.
2. Refusals. A proper prefix ends inside some field, and every reader refuses input that ends before its width or before its stated length, so the decoder refuses it. After the record the decoder requires the end of input, so a trailing byte is refused. The decoder accepts only the six tags, so every other tag byte is refused before any payload is read.
3. Streams. A stream is a sequence of length-prefixed frames, so its decoder reads one length and one frame at a time until the input ends; a length that runs past the end is refused. By step 1 each frame decodes to a record with the same frame, and writing the decoded records in order writes each frame after the same length, which is the stream. The reference writes each frame from its own node and edge writers, its own row, pair and arc fields and its own fan records, in the same canonical order, so the streams agree byte for byte.
4. Manifest records. The record decoder reads, in the order the writer writes them, the segment identifier, the counted node ranges, the two optional allocation ranges, the statistics, the two optional greatest identifiers, the two digests and the two counted cross maps, and requires the end of input. Steps 1 and 2 apply field by field. The reference record of a segment is computed from the reference's own summary, edge identifier run, content digest and cross maps over the family, so the record bytes agree.
5. The finite certificate decodes every frame of every segment of the bounded graph and fan tree partitions and of the three-shape witnesses, every proper prefix of each frame, each frame with a trailing byte and with each of the $250$ unknown tags, and does the same for the manifest record of every segment.

### Executable counterpart

- Law ID: `segments.record-frames`
- Reference: `panproto_reference::segment::segment_record_stream`
- Reference: `panproto_reference::segment::canonical_record_bytes`
- Reference: `panproto_reference::segment::manifest_records`
- Enumerator: `panproto_reference::enumerate::segments::graph_partition_cases; bound: graph partitions=290, fan tree partitions=40, shape witnesses=5`
- Exhaustive differential test: `bounded_graph_and_fan_tree_partitions_glue_and_agree_exhaustively; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=bounded_graph_and_fan_tree_partitions_glue_and_agree_exhaustively)'`
- Exhaustive differential test: `graph_table_and_tree_artifacts_match_the_reference_exactly; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=graph_table_and_tree_artifacts_match_the_reference_exactly)'`
- Property differential test: `randomized::boundary_cut_matches_the_reference_on_generated_shape_choices; command: cargo nextest run -p panproto-inst --test reference_segments --no-tests=fail -E 'test(=randomized::boundary_cut_matches_the_reference_on_generated_shape_choices)'`
- Property scope: `five documented shape witnesses; record streams, frames and manifest records against the reference with their round trips and refusals; configured successful cases per seed=256`
- File: `crates/panproto-inst/src/segment/stream.rs`
- File: `crates/panproto-inst/src/segment/manifest.rs`
- File: `crates/panproto-inst/src/segment/reader.rs`
- File: `crates/panproto-inst/tests/reference_segments.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

## Complement segments decode to what they encode {#L-segments-14}

### Statement

Let $C$ be a graph complement segment of segment $i$ that graph restriction can produce: its synthesized keys are contracted pairs, expanded edges or expanded contractions, and each dropped node is stored under its identifier. Write $\operatorname{bytes}(C)$ for `GraphComplementSegment::canonical_bytes`. Then `GraphComplementSegment::from_canonical_bytes` of $\operatorname{bytes}(C)$ under $i$ returns a complement segment of $i$ whose canonical bytes are $\operatorname{bytes}(C)$, and its fields are those the reference `decode_complement_segment_bytes` reads from the same bytes. Both decoders refuse every proper prefix of $\operatorname{bytes}(C)$ and $\operatorname{bytes}(C)$ followed by any byte.

### Outside the domain

Both decoders refuse a version other than the segment format version, a shape tag other than $2$ (a tree or table complement is no graph complement), a value presence tag above $3$, a synthesized-key tag above $2$, a count larger than the bytes left can hold, map keys that do not strictly ascend, and a value outside the canonical value layout. Production returns `SegmentError::Decode` for the complement segment artifact. The bytes do not name their segment, so the decoder takes the identifier the manifest files them under and does not check it. A resolution key has no complement encoding, so a complement holding one has no bytes to decode ([L-segments-12](#L-segments-12) writes none).

### Definitions used

The layout is the one `canonical_bytes` documents: the version and shape tag, then the dropped nodes and edges and the original edges as counted node and edge records, the original anchors, fields and values as counted maps from node identifiers, the synthesized nodes and edges as counted maps from identifiers to tagged keys, and the two floors.

### Proof

1. Totality. Each section is written by a counted writer whose entries are node records, edge records, an identifier with a name, a field map, a value presence or a tagged key. The decoder reads the sections in the order they are written, each entry with the inverse reader of its writer: the node and edge readers of [L-segments-13](#L-segments-13), the name reader, the value-map reader, the presence reader the node reader uses for a node's value, and a key reader that reads the tag and then the fields the writer wrote for that tag. Each reader returns the value its writer wrote and consumes exactly its bytes, and the writer visits each map in ascending key order, so every ascending-key check passes and the decoder returns $C$'s fields with $i$; re-encoding writes the same fields in the same order, which is $\operatorname{bytes}(C)$.
2. Refusals. A proper prefix ends inside a field or before a counted section ends, and every reader refuses input that ends early or a count the bytes left cannot hold. After the floors the decoder requires the end of input, so a trailing byte is refused. The version, shape, presence and key readers accept only the tags their writers write.
3. Agreement. The reference decoder reads the same sections with its own readers and order checks, so on accepted input both return the same fields, and each refusal above is a refusal of the reference as well: it reads the version and shape first, refuses a truncated field or noncanonical order, and requires the end of input.
4. The finite certificate decodes the complement segment of every segment of every case of the carrier of [L-segments-11](#L-segments-11) whose segmented restriction completes, compares it with the encoded complement and with the reference decoding, and refuses every proper prefix and four one-byte extensions of each distinct encoding in both decoders. A witness with an entry of every kind (a dropped node with a value and fields, dropped and original edges, an original anchor, original fields, each of the four value presences and each of the three key kinds) does the same and refuses another version, each of the $255$ other shape tags, each of the $252$ unknown presence tags, each of the $253$ unknown key tags, and synthesized identifiers out of order.

### Executable counterpart

- Law ID: `segments.complement-decode`
- Reference: `panproto_reference::segment::canonical_complement_segment_bytes`
- Reference: `panproto_reference::segment::decode_complement_segment_bytes`
- Enumerator: `panproto_reference::enumerate::segments::segmented_restrict_cases; bound: graphs=458, migrations=8, partitions=35, cases=128240`
- Exhaustive differential test: `segmented::a_complement_segment_decodes_from_its_canonical_bytes_as_the_reference_decodes_them; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented::a_complement_segment_decodes_from_its_canonical_bytes_as_the_reference_decodes_them)'`
- Property differential test: `segmented_randomized::complement_segments_decode_as_the_reference_decodes_them_on_generated_partitions; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_randomized::complement_segments_decode_as_the_reference_decodes_them_on_generated_partitions)'`
- Property scope: `graphs of 2 to 8 nodes over a three-vertex schema with every edge and loop, up to 14 edges, every survival choice, owners from 2 to 4 segments; every complement segment of a completing run decoded against the reference, with every proper prefix and four one-byte extensions refused; 256 cases`
- Production certificate: `segmented::a_complement_segment_with_every_entry_kind_decodes_and_malformed_tags_are_refused; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented::a_complement_segment_with_every_entry_kind_decodes_and_malformed_tags_are_refused)'`
- File: `crates/panproto-inst/src/segment/restrict.rs`
- File: `crates/panproto-inst/src/segment/reader.rs`
- File: `crates/panproto-inst/src/segment/bytes.rs`
- File: `crates/panproto-inst/tests/reference_restrict.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

## Segmented graph get assembles to whole-state get {#L-segments-15}

### Statement

Let $\ell = (m, S, T)$ be a graph lens whose plan $\pi$ compiles and carries no expression, and let $S_\bullet$, $P$, the manifest $M$ with trusted digest $d$ and the budget $B$ satisfy the hypotheses of [L-segments-12](#L-segments-12) for $\pi$. Then `graph_lens::get_segmented` of $\ell$ over $M$, $d$ and $B$ is the run of [L-segments-12](#L-segments-12) for $\pi$ with lens identity and view schema those of the graph identity envelope of $\ell$. In particular:

1. its run identity is a function of $\ell$, $d$ and $P$ alone;
2. when the run completes, each segment $i$ emits the view and complement segments the reference definition of [L-segments-11](#L-segments-11) gives segment $i$, byte for byte, and assembling the emitted view segments with the complement segments decoded from their bytes ([L-segments-14](#L-segments-14)) gives
$$\operatorname{assemble}_P(V, C) = \operatorname{get}_\ell(\operatorname{glue}_P(S_\bullet)),$$
the whole-state get of the glued graph without its identity stamp;
3. when whole-state get refuses $\operatorname{glue}_P(S_\bullet)$ with a restrict refusal, the run fails with the refusal of [L-segments-11](#L-segments-11);
4. a run stopped after any number $c$ of tasks and resumed against the same store reuses $\min(c, k)$ tasks, where $k$ is the number the uninterrupted run completes, and ends with the uninterrupted run's status and manifest and no orphan.

### Outside the domain

A lens whose plan does not compile returns the compilation error and runs nothing. A lens whose restrict carries an expression (a survival predicate, a term assignment or a field transform other than renaming, dropping, adding or keeping fields) is declined with `Residual::Global` before any task, and a budget below the partition's segment count or a record's working-set estimate is declined before any task, as in [L-segments-12](#L-segments-12). The identity stamp is not part of a complement segment, which is filed under its run; a caller binds the assembled complement to $\ell$ with the envelope whole-state get writes.

### Definitions used

$\operatorname{get}_\ell$ is whole-state graph get ([Lenses](lenses.md)): graph restriction along $\pi$, its complement stamped with the graph identity envelope of $\ell$. The graph identity envelope names the lens digest and the target schema digest, which serve as the run's lens identity and view schema.

### Proof

1. Get of $\ell$ is graph restriction along $\pi$ followed by the identity stamp, and `get_segmented` compiles the same $\pi$ from the same lens and passes it to the checkpointed run with the envelope's lens and target schema digests. Item 1 follows from the run identity of [L-segments-12](#L-segments-12), whose inputs are those digests, $d$ and $P$.
2. By [L-segments-12](#L-segments-12) a complete run emits the reference definition's segments, and by [L-segments-14](#L-segments-14) decoding the emitted complement bytes recovers each complement segment. By [L-segments-11](#L-segments-11) the assembly of those segments is whole-state restriction of the glued graph, which is get of $\ell$ before its stamp.
3. Refusals and the declines before any task are those of the run, by [L-segments-11](#L-segments-11) and [L-segments-12](#L-segments-12), and whole-state get returns the restrict refusal of whole-state restriction.
4. Resumption is item 2 of [L-segments-12](#L-segments-12).
5. The finite certificate takes every 97th case of the carrier of [L-segments-11](#L-segments-11), $1{,}323$ cases read as lenses over their schemas; it compares each completing run's emitted segments with the reference definition byte for byte and its assembly with whole-state get, checks a refusal against whole-state get, and stops and resumes each run at every cancellation point. Witnesses cover the budget decline and the decline of a survival predicate.

### Executable counterpart

- Law ID: `segments.segmented-graph-get`
- Reference: `panproto_reference::segment::restrict_graph_segmented`
- Enumerator: `panproto_reference::enumerate::segments::segmented_restrict_cases; bound: every 97th case, cases=1323, cancellation points=4n+3 per case`
- Exhaustive differential test: `segmented_graph_get::segmented_graph_get_assembles_to_whole_state_get_and_resumes_from_every_cancellation_point; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_get::segmented_graph_get_assembles_to_whole_state_get_and_resumes_from_every_cancellation_point)'`
- Property differential test: `segmented_graph_get::segmented_graph_get_matches_the_reference_and_resumes_on_generated_partitions; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_get::segmented_graph_get_matches_the_reference_and_resumes_on_generated_partitions)'`
- Property scope: `graphs of 2 to 8 nodes over a three-vertex schema with every edge and loop, up to 14 edges, every survival choice, owners from 2 to 4 segments; emitted segments against the reference, assembly against whole-state get, every cancellation point resumed; 64 cases`
- Production certificate: `segmented_graph_get::segmented_graph_get_declines_an_exceeded_budget_and_an_expression_before_any_task; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_get::segmented_graph_get_declines_an_exceeded_budget_and_an_expression_before_any_task)'`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-inst/src/segment/runner.rs`
- File: `crates/panproto-lens/tests/reference_lenses.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

## Segmented graph put assembles to whole-state put {#L-segments-16}

### Statement

Let $\ell = (m, S, T)$ be a graph lens whose plan $\pi$ compiles, carries no expression (as in [L-segments-11](#L-segments-11)), and whose source schema declares no uniqueness or primary key that reads a position among siblings, a `Within` scope, or a path of more than one edge. Let $P$ be a partition, $V$ a family of view segments valid under $P$, and $C$ a family of complement segments, one per segment of $P$ in the same order, each holding entries only about elements its segment owns: a node's entries in its owner's segment, an edge's in the owner of its source, a synthesized element's in the owner of its chain. Write $\operatorname{join}(C)$ for the union of the complement segments as one complement, its synthesized identifiers unchanged, stamped with the graph identity of $\ell$ as $\operatorname{get}_\ell$ stamps it. Then `graph_lens::put_segmented` of $\ell$ over $V$, $C$ and $P$ returns, for each segment $i$, a segment $R_i$ computed from $V_i$, $C_i$ and boundary facts, and

$$\operatorname{glue}_P(R) = \operatorname{put}_\ell(\operatorname{glue}_P(V), \operatorname{join}(C)),$$

where $\operatorname{put}_\ell$ is whole-state graph put ([Lenses](lenses.md)); $R$ is the split of that source by owner, which is the reference definition. When whole-state put refuses, the segmented put returns the same refusal and no segment. In particular, for a source family $S_\bullet$ whose segmented get ([L-segments-11](#L-segments-11)) gives $(V, C)$, the segmented put of $(V, C)$ is $S_\bullet$ wherever whole-state GetPut holds for $\operatorname{glue}_P(S_\bullet)$, and for an edited family $V'$ the segmented get of the segmented put of $(V', C)$ assembles to $V'$ exactly when whole-state get of whole-state put of $\operatorname{glue}_P(V')$ does.

### Outside the domain

A lens whose restrict carries an expression is declined before any segment is read with `Residual::Global { op: Put, reason: ExpressionDemand }`, and one with a key of the excluded forms with `Residual::Global { op: Put, reason: KeyDemand { vertex, key } }` at the least vertex and key; `graph_put_locality` classifies the rest as `Summary(SortedMerge)`. A plan that does not compile is refused, as whole-state put refuses it, with `PlanCompilation`. A view family that fails `verify_partition` is refused with its error; complements that are not one per segment in order are refused with `MalformedPartition`; an entry filed under a segment that does not own its element with `WrongOwner` (or `Unowned` for an element no range covers); and one edge identifier recorded by two complement segments with `DuplicateEdgeId`.

### Definitions used

The *local view* of segment $i$ is $V_i$ with a placeholder, carrying the view anchor, for each remote node it reads; the *local complement* is $C_i$ as a whole-state complement, stamped. The remote nodes segment $i$ reads are the targets of its view edges and dropped edges and the endpoints of its synthesized chains that another segment owns. The *census* is the merge of every segment's ascending runs of held view edge identifiers and of the dropped, synthesized and recorded edge identifiers of its complement. The *restored family* is, for each segment, the restored nodes it owns and the restored edges whose source it owns, with a ghost and an in-stub for each restored edge whose target another segment owns.

### Proof

1. Ownership. Every clause of whole-state put reads, about one element, the element, its incident edges, the complement entries about it, and the anchors, presence and restored forms of the nodes those name. Node identifiers are partitioned by $P$, and by hypothesis each complement entry lies with its element's owner, so a node, its outgoing edges and their complement entries lie in one segment; what such a clause reads beyond them is a remote node, delivered by the exchange of node facts, or an edge identifier or record held elsewhere, delivered by the census.
2. Identifier collisions. A view node collides only with a dropped node of the same identifier, which its owner holds, so node collisions are found locally. A view edge may collide with a dropped edge held anywhere; the census finds every such pair. `IdCollision` is the least variant of the report order and restoration raises none of its own (every restored view edge has endpoints in the source and a fresh identifier among view edges), so when the census or a segment finds a collision, the least one is the whole-state refusal, and the run returns it.
3. Restoration. Without collisions, each segment restores the nodes it owns from its local view and complement, exactly as whole-state restoration restores them: a node's anchor reads only its own view anchor and its recorded entries, and a fan's reads its own out-edges. The owner offers each restored node, and each segment restores the edges it holds against the restored forms of their endpoints, which whole-state restoration reads from the same nodes. An edge whose endpoint restores to no node maps back by the view anchors of its endpoints, which the local view holds, or is refused, as in clause (3) of [L-lenses-4](lenses.md#L-lenses-4). A held view edge whose identifier another segment's complement records as synthesized is left out, as whole-state restoration leaves out every synthesized identifier, and a recorded original form held elsewhere is imported by the census. The synthesized-edit clause reads a synthesized element, which its chain owner holds, and the anchors and presence of its chain's endpoints, which the exchange delivers. So each segment's structural violations are the whole-state violations about its elements, and the union of the restored segments is the whole-state restorable counterfactual.
4. The clauses read on the restored source. Whole-state put evaluates them on the restorable counterfactual unless tracing some node's transforms is refused, in which case it skips them; each segment reports its trace refusals, and the run skips the clauses exactly when one is reported anywhere. Forward stability compares each restored view node with the node get gives the restored source; segmented restriction of the restored family gives, segment by segment, the nodes whole-state restriction gives the glued family ([L-segments-11](#L-segments-11), whose stages need no allocation, together with the stage 5 numbering of whole-state restriction checked from every segment's greatest identifiers and synthesized keys), and its refusal is the whole-state refusal. Required cells read a node's outgoing edges and fields, held by its owner. A uniqueness key in the domain reads, at an element, its own value, its fields and the values of its outgoing edges' targets, which the restored family holds as placeholders; each segment emits its elements' canonical key bytes in ascending order, and merging the runs groups exactly the whole-state buckets, the values of each group being those of its least element. A keying refusal is reported at the least refusing element, the minimum of each segment's least. The fan domain reads a node's shape, out-edges and in-edges; the restored family's in-stubs give each owned node its in-edges from other segments.
5. Report order. Whole-state put returns the least violation of its report in the report order, which compares variant, then element, then the declared tie-breakers. Every violation is about an element with one owner or is a key verdict produced by the merge, so the union of the segments' reports and the merge's verdicts has the whole-state report's least element; violations tied at one element come from that element's owner in the same generation order. The run sorts the union and returns its least, so a refused put returns the whole-state refusal and changes no segment.
6. Laws. GetPut and PutGet over segments follow from the equation: by [L-segments-11](#L-segments-11) the glued segmented get of $S_\bullet$ is whole-state get with its synthesized elements renumbered, and whole-state put reads a synthesized element only through the key its complement records, so it gives the same source for both numberings.
7. The finite certificate takes every 97th case of the carrier of [L-segments-11](#L-segments-11), $1{,}323$ cases, and every edit of each case's glued segmented view: the identity, the removal of each node and of each edge, each edge retargeted at each other node, an edge added between each ordered pair of nodes in different segments, each node re-anchored at each other vertex, and a node added to each segment. For each it compares the segmented put with whole-state put of the glued view (sources glued, refusals by reason), with the reference definition segment by segment where whole-state production and reference put agree, checks GetPut on the identity edit, and checks that the segmented get of a restored family assembles to the edited view exactly when whole-state get of whole-state put does. Whole-state production put refuses a view whose restored source its own get refuses with a refusal that is not a survival predicate's; the reference's clause (5) reads only survival predicates there and accepts such a view. The certificate admits exactly this disagreement between the two whole-state definitions and checks, for each such view, that production refuses with an evaluation failure and the reference's get of the reference's restored source refuses. Witnesses cover a contracted path through three segments, a collision between a view edge and another segment's dropped edge, an edge refused at a re-anchored endpoint, the locality classes, and a misplaced complement entry.

### Executable counterpart

- Law ID: `segments.segmented-graph-put`
- Reference: `panproto_reference::segment::put_graph_segmented`
- Enumerator: `panproto_reference::enumerate::segments::segmented_restrict_cases; bound: every 97th case, cases=1323, edits of each glued view`
- Exhaustive differential test: `segmented_graph_put::segmented_graph_put_agrees_with_the_reference_and_assembles_to_whole_state_put; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_put::segmented_graph_put_agrees_with_the_reference_and_assembles_to_whole_state_put)'`
- Exhaustive differential test: `segmented_graph_put::segmented_graph_put_slice_agrees_with_the_reference_on_every_eighth_case; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_put::segmented_graph_put_slice_agrees_with_the_reference_on_every_eighth_case)'`
- Property differential test: `segmented_graph_put::segmented_graph_put_matches_the_reference_on_generated_partitions; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_put::segmented_graph_put_matches_the_reference_on_generated_partitions)'`
- Property scope: `graphs of 2 to 8 nodes over a three-vertex schema with every edge and loop, up to 14 edges, every survival choice, owners from 2 to 4 segments; every edit of the glued view put back segment by segment against whole-state put and the reference; 64 cases`
- Production certificate: `segmented_graph_put::a_put_crossing_three_segments_restores_like_whole_state_put; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_put::a_put_crossing_three_segments_restores_like_whole_state_put)'`
- Production certificate: `segmented_graph_put::a_refused_put_returns_the_least_violation_of_every_segment; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_put::a_refused_put_returns_the_least_violation_of_every_segment)'`
- Production certificate: `segmented_graph_put::put_locality_classes_and_input_refusals; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_put::put_locality_classes_and_input_refusals)'`
- File: `crates/panproto-lens/src/graph_lens/segmented.rs`
- File: `crates/panproto-lens/src/graph_lens.rs`
- File: `crates/panproto-inst/src/segment/restrict.rs`
- File: `crates/panproto-lens/tests/reference_lenses.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

## A checkpointed segmented put resumes to the uninterrupted run {#L-segments-17}

### Statement

Let $\ell$, $P$, $V$ and $C$ satisfy the hypotheses of [L-segments-16](#L-segments-16), let $M$ be a manifest sealing $V$ under $P$ with trusted digest $d$, let each complement segment $C_i$ be given by its canonical bytes ([L-segments-14](#L-segments-14)), and let $B$ be a budget. Write $\operatorname{run}_c$ for `graph_lens::put_segmented_run` of $\ell$ over $M$, $d$, the view segments and the complement bytes, under $B$, stopped once $c$ tasks are done, and $\operatorname{ref}_c$ for the reference runner `segmented_put_with_progress` on the same inputs. Both run graph put as the six phases of [L-segments-12](#L-segments-12), $4n + 2$ tasks for $n$ segments, through one runner: the production runner `run_segmented` runs graph restriction and graph put as two phase operations. For every $c$ from $0$ to $4n + 2$:

1. the report of $\operatorname{run}_c$ on an empty store equals the report of $\operatorname{ref}_c$ on an empty store, field by field as in L-segments-12, the locality being `Summary(SortedMerge)`;
2. running again without a cancellation point against the same store gives the report of the reference run resumed the same way, reuses $\min(c, k)$ tasks, where $k$ is the number of tasks the uninterrupted run completes, and returns the status and manifest of the uninterrupted run, leaving no staged artifact;
3. both runs carry the run identity $\operatorname{RunJournal::run\_digest}(\iota, d, P, \texttt{graph put})$, where $\iota$ is the lens digest of the graph identity of $\ell$.

When the uninterrupted run completes, the restored segment of segment $i$ is the segment $R_i$ of [L-segments-16](#L-segments-16), written under the source schema digest of the identity of $\ell$, its Emit artifact holds no complement section, and the manifest seals the restored segments as a source. When whole-state put refuses, the run fails at Assemble with that refusal, after the $3n$ segment tasks of the first three phases, and its journal holds no Emit or Seal task: a refused put seals nothing.

### Outside the domain

A lens whose put [L-segments-16](#L-segments-16) classifies as global is declined with `Residual::Global { op: Put, .. }` before any task, and a plan that does not compile is refused before the run starts. A manifest that fails verification against $d$ is refused before any task. A partition with more segments than the budget's affected segments, or a segment whose working set exceeds the budget, is declined before any task; the working set of a segment is the `MemoryModel` estimate of its view record plus the length of its complement bytes, read through `SegmentSource::byte_len`. A complement entry about an element its segment does not own is refused at that segment's Validate task with `WrongOwner` or `Unowned`, and one edge identifier recorded by two complement segments at Assemble with `DuplicateEdgeId`. The coordinator's bytes above the summary budget decline the run at Assemble; put emits no complement, so no complement budget declines it. A store failure stops the run as incomplete and reports each staged artifact the journal does not name as an orphan. Whole-state production put refuses a view whose restored source its own get refuses with an evaluation failure, which the reference's clause (5) accepts; the run inherits this disagreement between the two whole-state definitions from [L-segments-16](#L-segments-16), and the certificate admits it in the same way.

### Definitions used

The *phase operation* of a run fixes what each task computes; the runner fixes the task order, the input digests, the journal, reuse, cancellation, the budgets, the Emit layout and the seal, as stated for L-segments-12. A task's input digest binds the lens identity, its key and the digests of the artifacts it reads; Validate also binds the manifest leaf of its view segment and the digest of its complement bytes. For put the artifacts are: Validate, the eight-byte length of the view segment's canonical bytes, those bytes and the complement bytes; Export, the segment's held view edge identifiers, its complement's dropped, synthesized and recorded edge identifiers with the recorded edges, the view nodes its complement records as dropped, and the remote nodes its restoration reads by owner (the census runs and requests of L-segments-16); Summarize, for every node another segment requests of the segment, ascending, the node's view anchor when the segment's view holds it and the node's restored anchor and value when the segment's own restoration gives one (its *node facts*); Assemble, the number of segments and the boundary of each restored segment; Emit, the restored segment and no complement in the Emit layout; Seal, the source manifest. The *coordinator's bytes* are the summed lengths of the Summarize artifacts and of the Assemble artifact.

### Proof

1. Task order, accounting, identity and reuse are those of the runner, which is shared with restriction, so steps 1 and 2 of the proof of L-segments-12 apply unchanged: both sides execute the same tasks in the same order, test the cancellation point before each, and a resumed run reuses exactly the committed prefix of the uninterrupted run. The complement digest among Validate's inputs makes a changed complement recompute every later task.
2. Outputs. Validate checks the view segment against $M$ and the complement's ownership; Export and Summarize compute, from one segment, the census runs, the requests and the node facts that `put_segmented` computes, and each segment's part is rebuilt from its Validate artifact, the census read from every Export artifact and the facts read from its owners' Summarize artifacts, which are the inputs the in-memory put gives that segment. Assemble merges the census, refuses the least collision, restores each segment from its part, and runs the clauses on the restored source through `graph_restrict_segmented_nodes_with`, which fetches each restored segment by restoring it again; the forward-stability, required-cell, key and fan checks are the shared clause routine of the in-memory put. By [L-segments-16](#L-segments-16) its refusal is the whole-state refusal, and otherwise the boundaries are those of the restored family $R$. Emit restores one segment again and attaches its boundary from the Assemble artifact, giving $R_i$.
3. Agreement with the reference. The reference defines each artifact the report depends on from the glued state: the node facts of a segment from the glued view and the whole-state restored source (on a complete run the restored form of a node is the node of that source with its anchor and value, by step 3 of the proof of L-segments-16), the boundaries from the split restored source, the Emit artifact from its own record, identifier and frame writers, and the manifest by sealing the restored segments as a source. The node facts and boundaries are written in the same canonical layout, so the coordinator's bytes agree, and the Seal holding agrees as in step 5 of the proof of L-segments-12. A refusal is returned at Assemble on both sides before the summary budget is read, so the refused and declined reports agree.
4. Refusal seals nothing. The refusal is returned by the Assemble task before it commits, so no Assemble, Emit or Seal artifact is written and the store's sealed state is unchanged.
5. The finite certificate takes every 97th case of the carrier of [L-segments-11](#L-segments-11), $1{,}323$ cases, with every edit of each case's glued segmented view as in [L-segments-16](#L-segments-16), and for each put compares the production and reference reports uninterrupted and at every cancellation point and after resuming, the emitted restored segments with the reference's byte for byte, the run's outcome with the in-memory `put_segmented`, and the journal of a refused run for an Emit or Seal task. Witnesses cover each budget (the number of segments and the working set before any task, the summary bytes at Assemble, and a complement budget of zero that completes), compared with the reference at every cancellation point, and a store whose commit fails at each task, after which the resumed run reuses every committed task and ends as the uninterrupted run.

### Executable counterpart

- Law ID: `segments.checkpointed-put`
- Reference: `panproto_reference::segment::segmented_put_with_progress`
- Enumerator: `panproto_reference::enumerate::segments::segmented_restrict_cases; bound: every 97th case, cases=1323, edits of each glued view, cancellation points=4n+3 per put`
- Exhaustive differential test: `segmented_graph_put_run::segmented_graph_put_run_matches_the_reference_runner_at_every_cancellation_point; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_put_run::segmented_graph_put_run_matches_the_reference_runner_at_every_cancellation_point)'`
- Exhaustive differential test: `segmented_graph_put_run::segmented_graph_put_run_slice_matches_the_reference_runner; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_put_run::segmented_graph_put_run_slice_matches_the_reference_runner)'`
- Property differential test: `segmented_graph_put_run::segmented_graph_put_run_matches_the_reference_runner_on_generated_partitions; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_put_run::segmented_graph_put_run_matches_the_reference_runner_on_generated_partitions)'`
- Property scope: `graphs of 2 to 6 nodes over a three-vertex schema with every edge and loop, up to 8 edges, every survival choice, owners from 2 to 3 segments; every edit of the glued view put back as a run, production and reference reports and restored segments; 16 cases`
- Production certificate: `segmented_graph_put_run::a_put_run_declines_each_exceeded_budget_like_the_reference; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_put_run::a_put_run_declines_each_exceeded_budget_like_the_reference)'`
- Production certificate: `segmented_graph_put_run::a_put_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_put_run::a_put_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run)'`
- File: `crates/panproto-inst/src/segment/runner.rs`
- File: `crates/panproto-inst/src/segment/restrict.rs`
- File: `crates/panproto-lens/src/graph_lens/segmented.rs`
- File: `crates/panproto-lens/tests/reference_lenses.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
