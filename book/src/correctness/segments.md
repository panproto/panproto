# Segments

A large instance may need to be stored in pieces without changing the instance it denotes. Segment correctness fixes the conditions under which this is possible: identifiers remain global, each node has one owner, boundary records describe every crossing edge, and assembling the pieces recovers the original value. These conditions form the *segment agreement*. The first ten laws below cover the segment foundation; the eleventh runs graph restriction segment by segment, the twelfth runs it as a checkpointed sequence of tasks that resumes after any interruption, the thirteenth fixes the record frames through which that run seals its output without holding a view segment, the fourteenth reads a complement segment back from the bytes that run writes, the fifteenth runs the get of a graph lens as that run, the sixteenth runs its put segment by segment, the seventeenth runs that put as a checkpointed run, the eighteenth and nineteenth run graph extension segment by segment and as a checkpointed run, the twentieth and twenty-first apply a graph delta to a segmented graph, reading only the segments it can change, in memory and as a checkpointed run, the twenty-second shows that such edits compose and invert as whole-state edits do, the twenty-third to twenty-eighth state the same for table and tree edits, and the twenty-ninth lets a checkpointed restriction carry its tasks over from an earlier run whose input segments agree, so that a restriction after an edit reads only the segments the edit changed. Storage backends belong to later components.

Let $I$ be a finite tree, table, or graph instance, and let $P$ be a partition for the same shape. We write $P(n)$ for the segment that owns node or row identifier $n$, $\operatorname{split}_P(I)$ for partitioning, and $\operatorname{glue}_P$ for checked assembly. An edge is owned by the owner of its source endpoint. The finite differential carrier contains 25 labelled graphs and all 290 assignments of their nodes to two or three labelled segments, together with two fan-bearing trees and all 40 assignments of their tree and fan nodes to two segments; one crossing-boundary witness for each instance shape, and one for each fan-bearing tree, checks the shape-specific representation.

First, the page establishes ownership, boundary, and assembly laws. Second, it fixes canonical artifacts, the storage vocabulary, and typed refusals. Third, it states the validation and allocation boundaries that the segment core must preserve. Fourth, it shows that graph restriction run segment by segment, with one boundary exchange and a reachability summary, assembles to whole-state restriction, that the checkpointed run of it reports and resumes as its reference runner does, that the record frames its Seal reads decode to the records they encode, that the complement segments it writes decode to the complements they encode, that the get of a graph lens run this way assembles to its whole-state get, that its put, run segment by segment over the view and complement segments that get writes, assembles to its whole-state put, that graph extension, run segment by segment and as a checkpointed run, assembles to whole-state extension, and that a graph delta applied to the segments it can change, in memory and as a checkpointed run, assembles to whole-state application.

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
2. Identity and reuse. A task's artifact is a deterministic function of the plan, the input manifest, the segment it names and the artifacts of earlier phases, and its input digest binds the lens identity, its key, those artifacts' digests and the digests of anything else the task reads (`PhaseOperation::read_inputs`; graph restriction without held complements reads nothing else). The store commits an artifact and its journal record in one step. A run cancelled after $c$ tasks has committed exactly the first $\min(c, k)$ tasks of the uninterrupted run, with the uninterrupted run's artifacts; the resumed run reuses each of them, since its inputs and artifact are unchanged, and recomputes the rest, which by determinism writes the uninterrupted run's artifacts. Its status and manifest are therefore those of the uninterrupted run, and since every staged artifact is committed in the same step, none is left staged.
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
7. The finite certificate takes every 97th case of the carrier of [L-segments-11](#L-segments-11), $1{,}323$ cases, and every edit of each case's glued segmented view: the identity, the removal of each node and of each edge, each edge retargeted at each other node, an edge added between each ordered pair of nodes in different segments, each node re-anchored at each other vertex, and a node added to each segment. For each it compares the segmented put with whole-state put of the glued view (sources glued, refusals by reason), with the reference definition segment by segment, checks GetPut on the identity edit, and checks that the segmented get of a restored family assembles to the edited view exactly when whole-state get of whole-state put does. Among the refused views are views whose restored source get refuses, which clause (5) of [L-lenses-4](lenses.md#L-lenses-4) refuses with `SurvivalPredicateRefused` carrying `EvaluationFailed`; the certificate requires at least one and compares each with the reference by reason. Witnesses cover a contracted path through three segments, a collision between a view edge and another segment's dropped edge, an edge refused at a re-anchored endpoint, the locality classes, and a misplaced complement entry.

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

A lens whose put [L-segments-16](#L-segments-16) classifies as global is declined with `Residual::Global { op: Put, .. }` before any task, and a plan that does not compile is refused before the run starts. A manifest that fails verification against $d$ is refused before any task. A partition with more segments than the budget's affected segments, or a segment whose working set exceeds the budget, is declined before any task; the working set of a segment is the `MemoryModel` estimate of its view record plus the length of its complement bytes, read through `SegmentSource::byte_len`. A complement entry about an element its segment does not own is refused at that segment's Validate task with `WrongOwner` or `Unowned`, and one edge identifier recorded by two complement segments at Assemble with `DuplicateEdgeId`. The coordinator's bytes above the summary budget decline the run at Assemble; put emits no complement, so no complement budget declines it. A store failure stops the run as incomplete and reports each staged artifact the journal does not name as an orphan. A view whose restored source get refuses fails the run at Assemble with clause (5)'s `SurvivalPredicateRefused` of [L-lenses-4](lenses.md#L-lenses-4), as whole-state put refuses it.

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

## Segmented graph extension assembles to whole-state extension {#L-segments-18}

### Statement

Let $\pi$ be the plan `compile_extension_plan` compiles for a migration from a source schema to a target schema, and suppose no value transform of $\pi$ evaluates an expression (every transform renames, drops, adds or keeps fields, and no vertex carries a term assignment). Write $\operatorname{ext}_\pi$ for the total graph extension engine `graph_extend_with_plan`: it refuses the least node whose anchor has no image, then the least edge whose label has no image, then the least node whose value transforms fail, and otherwise moves every node to the image of its anchor and every edge to the image of its label, keeping every node identifier, edge identifier, endpoint, value and position. For every partition $P$ and every family $S$ of graph segments that `verify_partition` accepts under $P$, `graph_extend_segmented` of $\pi$ over $S$ and $P$ returns, for each segment $i$, a segment $E_i$ computed from $S_i$ alone, and

$$\operatorname{glue}_P(E) = \operatorname{ext}_\pi(\operatorname{glue}_P(S)).$$

When $\operatorname{ext}_\pi$ refuses the glued graph, the segmented extension returns the same refusal, in the segment owning the element it is about (a node's owner, an edge's source's owner), and no segment. The extended family holds exactly the identifiers of $S$, so it seals exactly when $S$ seals ([L-segments-9](#L-segments-9)).

### Outside the domain

A plan with a value transform that evaluates an expression, or a term assignment, is declined before any segment is read with `Residual::Global { op: Extend, reason: ExpressionDemand { vertex } }` at the least such source vertex; `graph_extend_locality` classifies every other plan as `Local`. A family that `verify_partition` refuses is refused with its error. A plan that does not compile is refused by `compile_extension_plan`, before any instance is read.

### Definitions used

The *local graph* of segment $i$ holds its owned nodes and edges, a placeholder (identifier and anchor) for each ghost and each in-stub source, and each in-stub as an edge from its source's placeholder; the placeholders are its *foreign* nodes. A *scoped extension* runs the engine over the local graph on the nodes outside the foreign set and the edges whose source lies outside it, keeping an edge whose target is foreign. The extended segment $E_i$ holds the scoped extension's nodes and edges, each ghost with its anchor replaced by its image, and each in-stub with its source anchor and label replaced by their images.

### Proof

1. Locality. The engine reads, about a node, its anchor and, through its value transforms, its own fields: a transform that evaluates no expression binds no child and reads no other element. About an edge it reads its label. Neither reads a fact another segment holds, so the scoped extension of segment $i$ decides its own elements exactly as $\operatorname{ext}_\pi$ decides them in the glued graph. The engine is one function, `extend_scoped`, which the whole-state engine runs with an empty foreign set.
2. Refusals. Whole-state extension checks the nodes in ascending identifier order, then the edges, then the transforms. Each segment's scoped run checks its own elements in the same order and reports its first refusal with its position $(s, n)$: stage $0$ for an unmapped node, $1$ for an unmapped edge, $2$ for a failed transform, and the element's identifier. Identifiers are global, so no two segments report one position, and the least reported position, in lexicographic order, is the first element whole-state extension refuses; its refusal is the one its owner computed. A family with any refusal returns it and no segment.
3. Output. With no refusal, the scoped runs' nodes and edges partition the glued engine's output by owner, since an edge is owned by its source's owner and the engine keeps every edge's endpoints. A boundary record of $\operatorname{glue}_P(S)$ carries the anchor of a node owned elsewhere and the label of an edge owned elsewhere; the engine replaces each by its image, which depends on the anchor or label alone, so mapping the records gives the boundary that `partition_graph` writes for the extended graph. Gluing the extended segments thus gives $\operatorname{ext}_\pi(\operatorname{glue}_P(S))$.
4. The reference `extend_graph_segmented` glues the family, runs an independent whole-state extension (`extend_graph`, which maps each anchor by the explicit remap or by identity when the target has the vertex, and each label by the explicit remap or by the one target edge between its endpoints' images), and splits the result by owner. The finite certificate compares the two on every case of `segmented_extend_cases`: each release schema, each of the 27 vertex maps sending `a`, `b` and `c` to no image, `p` or `q` (the target holding `p`, `q` and the image of every source edge whose endpoints both map), each release graph, and each two- and three-segment split, $432{,}810$ cases. It compares every extended segment, the glued output with production whole-state extension, and that with the reference whole-state extension; on a refusal it compares the segment and the refusal with both. Of the cases, $47{,}040$ extend, $304{,}570$ are refused at a node and $81{,}200$ at an edge (two source edges collapsing onto one pair of images). A witness with renaming, adding and dropping transforms checks the glued output against production whole-state extension, and one with an expression transform checks the residual before any segment is checked.

### Executable counterpart

- Law ID: `segments.segmented-graph-extend`
- Reference: `panproto_reference::segment::extend_graph_segmented`
- Reference: `panproto_reference::segment::extend_graph`
- Enumerator: `panproto_reference::enumerate::segments::segmented_extend_cases; bound: schemas=57, vertex maps=27, graphs=458, splits=35, cases=432810`
- Exhaustive differential test: `segmented_extend::segmented_extend_agrees_with_the_reference_and_assembles_to_whole_state_extend; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_extend::segmented_extend_agrees_with_the_reference_and_assembles_to_whole_state_extend)'`
- Property differential test: `segmented_extend::segmented_extend_agrees_with_the_reference_on_generated_partitions; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_extend::segmented_extend_agrees_with_the_reference_on_generated_partitions)'`
- Property scope: `graphs of 1 to 8 nodes over a three-vertex schema with every edge and loop, up to 12 edges, every vertex map, owners from 1 to 4 segments; 256 cases`
- Production certificate: `segmented_extend::field_transforms_that_evaluate_no_expression_extend_segment_by_segment_like_whole_state; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_extend::field_transforms_that_evaluate_no_expression_extend_segment_by_segment_like_whole_state)'`
- Production certificate: `segmented_extend::an_expression_transform_is_declined_as_a_global_expression_before_any_segment_is_read; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_extend::an_expression_transform_is_declined_as_a_global_expression_before_any_segment_is_read)'`
- File: `crates/panproto-inst/src/graph_restrict.rs`
- File: `crates/panproto-inst/src/segment/extend.rs`
- File: `crates/panproto-inst/tests/reference_restrict.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

## A checkpointed segmented extension resumes to the uninterrupted run {#L-segments-19}

### Statement

Let $\pi$, $P$ and $S$ satisfy the hypotheses of [L-segments-18](#L-segments-18), let $M$ be a manifest sealing $S$ under $P$ with trusted digest $d$, and let $B$ be a budget. Write $\operatorname{run}_c$ for `graph_extend_run` of $\pi$ over $M$, $d$ and the canonical bytes of $S$, under $B$, writing its output under a target schema digest $\tau$ and stopped once $c$ tasks are done, and $\operatorname{ref}_c$ for the reference runner `segmented_extend_with_progress` on the same inputs. Both run graph extension as the six phases of [L-segments-12](#L-segments-12), $4n + 2$ tasks for $n$ segments; the production run is `run_segmented` of the phase operation `GraphExtend`, the runner restriction and put also use. For every $c$ from $0$ to $4n + 2$:

1. the report of $\operatorname{run}_c$ on an empty store equals the report of $\operatorname{ref}_c$ on an empty store, field by field as in L-segments-12, the locality being `Local`;
2. running again without a cancellation point against the same store gives the report of the reference run resumed the same way, reuses $\min(c, k)$ tasks, where $k$ is the number of tasks the uninterrupted run completes, and returns the status and manifest of the uninterrupted run, leaving no staged artifact;
3. both runs carry the run identity $\operatorname{RunJournal::run\_digest}(\lambda, d, P, \texttt{graph extend})$ for the lens digest $\lambda$ of the run's spec.

When the uninterrupted run completes, the emitted segment of segment $i$ is the segment $E_i$ of [L-segments-18](#L-segments-18) written under $\tau$, its Emit artifact holds no complement section, and the manifest, which verifies against its digest, seals the extended segments as a source. When whole-state extension refuses, the run fails at Assemble with that refusal in the owning segment, after the $3n$ tasks of the first three phases, and its journal holds no Emit or Seal task: a refused extension seals nothing.

### Outside the domain

A plan that [L-segments-18](#L-segments-18) declines as global is declined with `Residual::Global { op: Extend, .. }` before any task. A manifest that fails verification against $d$ is refused before any task. A partition with more segments than the budget's affected segments, or a segment whose `MemoryModel` estimate exceeds the working set, is declined before any task. The summed Summarize artifacts above the summary budget decline the run at Assemble after the refusal check; extension emits no complement, so no complement budget declines it. A store failure stops the run as incomplete and reports each staged artifact the journal does not name as an orphan.

### Definitions used

For extension the artifacts are: Validate, the input segment's canonical bytes; Export, nothing, since no segment reads a fact another holds; Summarize, one zero byte when the segment's own elements extend, and otherwise the byte $1$, the stage byte and the four-byte identifier of its first refusal's position; Assemble, nothing; Emit, the extended segment and no complement in the Emit layout; Seal, the source manifest. The *coordinator's bytes* are the summed lengths of the Summarize artifacts and of the Assemble artifact, which is empty.

### Proof

1. Task order, accounting, identity and reuse are those of the shared runner, so steps 1 and 2 of the proof of L-segments-12 apply unchanged: both sides execute the same tasks in the same order, test the cancellation point before each, and a resumed run reuses exactly the committed prefix of the uninterrupted run.
2. Outputs. Summarize runs the scoped extension of one segment from its Validate artifact and writes its first refusal's position. Assemble reads one position per segment, takes the least, and recomputes the refusal in that segment, which by step 2 of the proof of [L-segments-18](#L-segments-18) is the whole-state refusal; it returns it before committing. Otherwise Emit runs the scoped extension of one segment again and maps its boundary records, giving $E_i$.
3. Agreement with the reference. The reference defines each Summarize artifact from the segment's own elements (the least unmapped node, else the least unmapped owned edge), the outcome from the glued state, the Emit artifact from its own record, identifier and frame writers, and the manifest by sealing the extended segments as a source. The positions are written in the same canonical layout, so the coordinator's bytes agree; a refusal is returned on both sides before the summary budget is read, so the refused and declined reports agree, and the Seal holding agrees as in step 5 of the proof of L-segments-12.
4. Refusal seals nothing. The refusal is returned by the Assemble task before it commits, so no Assemble, Emit or Seal artifact is written.
5. The finite certificate takes every 331st case of the carrier of [L-segments-18](#L-segments-18), $1{,}308$ cases ($134$ complete and $1{,}174$ refused), and compares the production and reference reports uninterrupted, at each of the $18{,}420$ cancellation points and after resuming, the emitted segments with the in-memory segmented extension and the reference byte for byte, and the journal of a refused run for an Emit or Seal task. Witnesses cover each budget (the number of segments and the working set before any task, the summary bytes at Assemble, and a complement budget of zero that completes), compared with the reference at every cancellation point; a store whose commit fails at each task, after which the resumed run reuses every committed task and ends as the uninterrupted run; and a refusal at a node of the third segment, reported at Assemble after nine tasks with nothing sealed.

### Executable counterpart

- Law ID: `segments.checkpointed-extend`
- Reference: `panproto_reference::segment::segmented_extend_with_progress`
- Enumerator: `panproto_reference::enumerate::segments::segmented_extend_cases; bound: every 331st case, cases=1308, cancellation points=4n+3 per run`
- Exhaustive differential test: `segmented_extend_run::segmented_extend_run_matches_the_reference_runner_at_every_cancellation_point; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_extend_run::segmented_extend_run_matches_the_reference_runner_at_every_cancellation_point)'`
- Property differential test: `segmented_extend_run::segmented_extend_run_matches_the_reference_runner_on_generated_partitions; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_extend_run::segmented_extend_run_matches_the_reference_runner_on_generated_partitions)'`
- Property scope: `graphs of 1 to 6 nodes over a three-vertex schema with every edge and loop, up to 8 edges, every vertex map, owners from 1 to 3 segments; every cancellation point; 32 cases`
- Production certificate: `segmented_extend_run::an_extension_run_declines_each_exceeded_budget_like_the_reference; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_extend_run::an_extension_run_declines_each_exceeded_budget_like_the_reference)'`
- Production certificate: `segmented_extend_run::an_extension_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_extend_run::an_extension_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run)'`
- Production certificate: `segmented_extend_run::a_refused_extension_run_fails_at_assemble_with_the_whole_state_refusal_and_seals_nothing; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_extend_run::a_refused_extension_run_fails_at_assemble_with_the_whole_state_refusal_and_seals_nothing)'`
- File: `crates/panproto-inst/src/segment/runner.rs`
- File: `crates/panproto-inst/src/segment/extend.rs`
- File: `crates/panproto-inst/tests/reference_restrict.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

## A segmented graph edit assembles to whole-state application and reads only what it can change {#L-segments-20}

### Statement

Let $P$ be a graph partition and $S$ a family of graph segments that `verify_partition` accepts under $P$, whose glued graph $G = \operatorname{glue}_P(S)$ lies in the fan domain and whose edge identifiers satisfy the seal floor rule of $P$ (so that $S$ seals, [L-segments-9](#L-segments-9)). Let $\delta$ be a graph delta and write $\operatorname{apply}$ for whole-state application, `GraphDelta::apply` ([L-edits-1](edits.md#L-edits-1)). Write $R(\delta)$ for the *read set*, `FamilyIndex::read_set`, defined below from $\delta$ and the manifest records of $S$ alone. Then `apply_segmented` of $\delta$ over $S$ and $P$ reads the segments of $R(\delta)$ and no other, and:

1. when $\operatorname{apply}(\delta, G)$ succeeds and its split under $P$ seals, the output is $\operatorname{split}_P(\operatorname{apply}(\delta, G))$, so that $\operatorname{glue}_P$ of the output is $\operatorname{apply}(\delta, G)$, and every segment outside $R(\delta)$ is returned unchanged;
2. when $\operatorname{apply}(\delta, G)$ refuses, the output is that refusal;
3. otherwise the output is the refusal of laying the edited graph out: `Unowned` for the least added node no range covers, and then, for the least added edge identifier the seal floor rule refuses for the owner of the edge's source, `EdgeIdOutsideAllocation` or `NoAllocationRange`, the refusal that splitting and sealing the edited graph returns.

A delta whose added nodes and edges draw their identifiers from `FreshIds` of the segment owning the node, or the edge's source, is refused for no identifier it adds.

### Outside the domain

A family that `verify_partition` refuses is refused with its error before any delta check. On a glued graph outside the fan domain the fan-domain check (check 7 of `GraphDelta::check`) is made at the touched nodes defined below, not at every node, so a violation at a node the delta does not touch is not reported: the segmented edit reads no segment that could reveal it. The output is then the whole-state application with check 7 restricted to the touched nodes. A family that breaks the seal floor rule is outside the domain of the read set, since an edge identifier at or above the floor is then not located by its allocation range.

### Definitions used

The *holders* of an edge identifier $e$ are the segments that may hold it: with $f$ the least lower bound of the edge allocation ranges, the one segment whose range contains $e$ when $e \ge f$, and otherwise every segment whose record states a largest edge identifier of at least $e$. The *read set* $R(\delta)$ holds the owner of every node $\delta$ deletes, adds or changes; for an anchor change also the peers that the owner's record lists for its crossing edges in either direction; for every edge $\delta$ deletes or adds, the owners of its payload's endpoints and the holders of its identifier; and for an edge whose label or position changes, the holders of its identifier, with, for a label change, the peers those holders' records list as targets of crossing edges. The *touched* nodes of $\delta$ are its added nodes, the nodes whose metadata it changes, the endpoints of the edges it deletes or adds, and the sources of the edges whose label it changes. The *facts* of a read segment are the nodes it owns that $\delta$ deletes, changes or touches, the edges it holds that $\delta$ names, the edges it holds incident to a node it owns that $\delta$ deletes or touches, and the in-stubs entering such a node. The *facts graph* $F$ glues the facts of every read segment, an in-stub's copy giving way to its holder's payload, with a placeholder for every endpoint no fact reports.

### Proof

1. Segments outside the read set do not change. A segment $T$ holds its own nodes and edges and a boundary of ghosts and in-stubs, each a copy of one crossing edge's endpoints, label and the anchor of its remote endpoint. Every node $\delta$ names is owned in $R(\delta)$, and every edge it names is held in $R(\delta)$: an edge is held by one of its holders, and an added edge by the owner of its source. A crossing edge of $T$ that $\delta$ adds or deletes has an endpoint owned by $T$, so $T \in R(\delta)$. An anchor change of a node $n$ alters a ghost of $n$, held by a segment with an edge into the owner of $n$, or an in-stub from $n$, held by a segment the owner's edges enter; both are peers its record lists. A label change of an edge alters the in-stub in its target's owner, a peer its holder's record lists. The record of a segment is a function of its content ([L-segments-4](#L-segments-4)), so every record outside $R(\delta)$ is unchanged.
2. The facts decide checks 1 to 6. Checks 1 to 3 read the current payload of each named element, which its owner or holder reports in full; check 4 reads the edges incident to each deleted node, which its owner holds or records as in-stubs; check 5 reads whether each added identifier is taken, which the owner of a node and the holders of an edge identifier report; check 6 reads whether each added edge's endpoints exist, which their owners report. Every node of $F$ exists in $G$, a placeholder standing only for an endpoint of an existing edge, and every edge of $F$ exists in $G$ with the payload of $G$ wherever $\delta$ names it. Thus $F$ and $G$ agree on everything checks 1 to 6 read, and the checks, run in their order on $F$, return the refusal they return on $G$.
3. The fan-domain check. The violations of a node depend on its shape and its incident edges. A node's incident edges change only when an edge at it is added or deleted, which makes it an endpoint of that edge, or when the label of one of its out-edges changes, which makes it that edge's source; the label of an in-edge enters no violation. Its shape changes only through a metadata change. Each such node is touched, and $F$ holds its full incidence. On a glued graph in the fan domain every untouched node keeps no violation, so the least violation among the touched nodes of the edited facts graph is the whole-state refusal of check 7.
4. Layout. With every check passed, $\operatorname{apply}(\delta, G)$ exists. Every node of $G$ is owned, so the least node `partition_graph` refuses as `Unowned` is the least unowned added node. A kept edge keeps its source and identifier, so it keeps its holder and satisfies the floor rule as it did in $S$; the floor rule thus refuses only added edges, in ascending identifier order, exactly as the seal's merge of the edge identifier runs refuses the least such identifier. The delta checks exclude a duplicate identifier.
5. Rewriting. Each read segment applies $\delta$ to its own nodes and edges (an element belongs to it when its owner, or its source's owner, is the segment) and rebuilds its boundary from its edges after $\delta$ and its in-stubs after $\delta$, taking each remote anchor after $\delta$ from $\delta$ itself, from the facts (which report every endpoint of an added edge and every node whose anchor changes) or, for a node $\delta$ leaves alone, from the segment's own copy. This is the segment `partition_graph` writes for $\operatorname{apply}(\delta, G)$. Together with step 1, the output is $\operatorname{split}_P(\operatorname{apply}(\delta, G))$.
6. Fresh identifiers. `FreshIds` draws a node identifier from the owner's node allocation range above every node the owner holds there, which the owner's node ranges cover, and an edge identifier from the edge allocation range of the source's owner above every edge identifier it holds there. Under the floor rule no other segment holds an identifier of that range, so the drawn identifier is free, owned, and admitted by the floor rule.
7. The reference `apply_graph_segmented` glues the family, applies the delta with the independent `edit::apply`, splits the result with `partition_graph` and seals it, refusing at the first step that refuses; the reference `read_set` computes the read set from the manifest records. The finite certificate walks `segmented_apply_cases`: the 116 graphs over at most three nodes and two edges, the 57 partitions (every assignment of the identifiers $0$ to $3$ to at most three segments, or of $0$ to $2$ with $3$ unowned, each with no edge allocation, one range per segment, or one range for segment $0$ alone), and the 145 atomic deltas over the node identifiers $0$, $2$ and $3$ and the edge identifiers $1$, $2$ and $10$, $958{,}740$ cases. For each it compares the read set with the reference, every output segment's canonical bytes with the reference, the glued output with production whole-state application and that with the reference application, every unread segment with its input, and every refusal with the reference and with whole-state application. Of the cases, $132{,}279$ apply, $814{,}245$ are refused by the delta checks and $12{,}216$ by the layout. A property test compares the law on families of up to six nodes and eight edges with the delta the reference differ gives toward a second graph whose nodes may be unowned or fans and whose edge identifiers reach into the allocation ranges.

### Executable counterpart

- Law ID: `segments.segmented-graph-apply`
- Reference: `panproto_reference::segment::apply_graph_segmented`
- Reference: `panproto_reference::segment::read_set`
- Enumerator: `panproto_reference::enumerate::segments::segmented_apply_cases; bound: graphs=116, partitions=57, deltas=145, cases=958740`
- Exhaustive differential test: `segmented_apply::segmented_apply_agrees_with_the_reference_on_every_case_of_the_carrier; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_apply::segmented_apply_agrees_with_the_reference_on_every_case_of_the_carrier)'`
- Exhaustive differential test: `segmented_apply::segmented_apply_agrees_with_the_reference_on_every_97th_case_of_the_carrier; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_apply::segmented_apply_agrees_with_the_reference_on_every_97th_case_of_the_carrier)'`
- Property differential test: `segmented_apply::segmented_apply_agrees_with_the_reference_on_generated_families; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_apply::segmented_apply_agrees_with_the_reference_on_generated_families)'`
- Property scope: `graphs of 1 to 6 nodes and up to 8 edges with two anchors and two labels, owners in 1 to 3 segments for 6 identifiers and two more possibly unowned, three edge allocations, deltas to a second graph of up to 8 nodes (fans included) and 8 edges with identifiers up to 30; 256 cases`
- Production certificate: `segmented_apply::fresh_ids_come_from_the_holders_ranges_and_are_never_refused; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_apply::fresh_ids_come_from_the_holders_ranges_and_are_never_refused)'`
- File: `crates/panproto-inst/src/segment/apply.rs`
- File: `crates/panproto-inst/src/graph_edit/apply.rs`
- File: `crates/panproto-inst/tests/reference_edits.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

## A checkpointed segmented graph edit reads only its read set and resumes to the uninterrupted run {#L-segments-21}

### Statement

Let $P$, $S$ and $\delta$ satisfy the hypotheses of [L-segments-20](#L-segments-20), let $M$ be a manifest sealing $S$ under $P$ with trusted digest $d$, and let $B$ be a budget. Write $k$ for the size of $R(\delta)$, $\operatorname{run}_c$ for `apply_run` of $\delta$ over $M$, $d$ and the canonical bytes of $S$, under $B$, stopped once $c$ tasks are done, and $\operatorname{ref}_c$ for the reference runner `segmented_apply_with_progress` on the same inputs. The production run is `run_segmented` of the phase operation `SegmentApply`, whose affected set (`PhaseOperation::affected`) is $R(\delta)$: its segment phases run for the read segments alone, $4k + 2$ tasks. For every $c$ from $0$ to $4k + 2$:

1. the report of $\operatorname{run}_c$ on an empty store equals the report of $\operatorname{ref}_c$ on an empty store, field by field as in [L-segments-12](#L-segments-12), the locality being `Exchange`;
2. running again without a cancellation point against the same store gives the report of the reference run resumed the same way, reuses $\min(c, j)$ tasks, where $j$ is the number of tasks the uninterrupted run completes, and returns the status and manifest of the uninterrupted run, leaving no staged artifact;
3. both runs carry the run identity $\operatorname{RunJournal::run\_digest}(\lambda, d, P, \texttt{graph apply})$, where $\lambda$ is the delta's digest, `GraphDelta::digest`, so two deltas never share a run's artifacts.

An uninterrupted run reads each segment of $R(\delta)$ from its source once and no other segment. When it completes, the emitted segment of each read segment is the corresponding segment of $\operatorname{split}_P(\operatorname{apply}(\delta, G))$, its Emit artifact holds no complement section, and the output manifest, which verifies against its digest, is the seal of that split as a source with the input's schema and binding and no instance digest; the record of every segment outside $R(\delta)$ is its input record. When the delta is refused, the run fails at Assemble with the refusal of [L-segments-20](#L-segments-20), after the $3k$ tasks of the first three phases, and its journal holds no Emit or Seal task: a refused edit seals nothing and leaves every segment as it was.

### Outside the domain

A manifest that fails verification against $d$ is refused before any task. More read segments than the budget's affected segments, or a read segment whose `MemoryModel` estimate exceeds the working set, decline the run before any task. The summed facts above the summary budget decline the run at Assemble after the refusal check; an edit emits no complement, so no complement budget declines it. A store failure stops the run as incomplete and reports each staged artifact the journal does not name as an orphan.

### Definitions used

For an edit the artifacts are: Validate, the read segment's canonical bytes; Export, nothing; Summarize, the segment's facts (its node records, then its edge records, then its in-stubs as identifier, source, target and label, each part counted and ascending); Assemble, nothing; Emit, the rewritten segment and no complement in the Emit layout; Seal, the output manifest. *Resealing* (`reseal`) replaces the records of the rewritten segments in the input manifest, checks the partition, every cross digest's counterpart, and the rewritten segments' edge identifier runs against their records, against each other and against the seal floor rule of the family, and recomputes the Merkle root; it reads no record stream and so records no instance digest.

### Proof

1. Task order, accounting, identity and reuse are those of the shared runner, with the segment phases ranging over the affected set, so steps 1 and 2 of the proof of L-segments-12 apply with $k$ in place of the number of segments: both sides execute the same tasks in the same order, test the cancellation point before each, and a resumed run reuses exactly the committed prefix of the uninterrupted run. The input segments are read only by Validate tasks, one per read segment, and a reused task reads nothing.
2. Outputs. Summarize writes the facts of one read segment. Assemble glues every read segment's facts and runs the checks of steps 2 to 4 of the proof of [L-segments-20](#L-segments-20), returning the refusal before it commits. Emit rewrites one read segment, reading every Summarize artifact for the anchors after the delta, as in step 5 of that proof.
3. Seal. The rewritten segments' records replace their input records; by step 1 of the proof of L-segments-20 every other record is the record of the edited family's segment, so the record list is the edited family's. Its cross digests pair because the edited family is consistent, the rewritten runs match their records because Emit wrote both from one segment, and an identifier of a rewritten run is held by no segment outside $R(\delta)$: a kept identifier stays with its holder, and an added identifier's holders lie in $R(\delta)$, where the delta checks found it free. The rewritten runs therefore hold no duplicate across the family, and resealing returns the seal of the edited family's split, which is what sealing every segment's run returns. The seal holds, per rewritten segment, its Emit header, its record and one edge identifier.
4. Agreement with the reference. The reference defines the read set from the sealed records, each Summarize artifact from its own facts definition and canonical writers, the outcome from the glued state through L-segments-20's reference, each Emit artifact from its record, identifier and frame writers, and the manifest by sealing the whole edited family as a source and clearing the instance digest. The facts share one canonical layout, so the coordinator's bytes agree; a refusal is returned on both sides before the summary budget is read, so the refused and declined reports agree, and the Seal holding agrees by step 3.
5. Refusal seals nothing. The refusal is returned by the Assemble task before it commits, so no Assemble, Emit or Seal artifact is written and no segment is rewritten.
6. The finite certificate takes every 331st case of the carrier of [L-segments-20](#L-segments-20), $2{,}897$ cases ($402$ complete and $2{,}495$ refused), and compares the production and reference reports uninterrupted, at each of the $24{,}059$ cancellation points and after resuming; for a complete run it compares the reads of a counting source with the read set, the emitted segments with the reference byte for byte, and the manifest's canonical bytes with the reference's; for a refused run it checks the journal for an Emit or Seal task. Witnesses cover a family of four segments in which a position change reads one segment, an anchor change reads its owner and both boundary peers, and an edge added under a fresh identifier reads its two endpoints' owners, each with $4k + 2$ tasks and every unread record kept; each budget (the read segments and the working set before any task, and the summary bytes at Assemble), compared with the reference at every cancellation point; a store whose commit fails at each task, after which the resumed run reuses every committed task and ends as the uninterrupted run; and a node deletion that leaves edges dangling, refused at Assemble with nothing sealed.

### Executable counterpart

- Law ID: `segments.checkpointed-graph-apply`
- Reference: `panproto_reference::segment::segmented_apply_with_progress`
- Reference: `panproto_reference::segment::apply_facts_bytes`
- Enumerator: `panproto_reference::enumerate::segments::segmented_apply_cases; bound: every 331st case, cases=2897, cancellation points=4k+3 per run`
- Exhaustive differential test: `segmented_apply::segmented_apply_run_matches_the_reference_runner_at_every_cancellation_point; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_apply::segmented_apply_run_matches_the_reference_runner_at_every_cancellation_point)'`
- Property differential test: `segmented_apply::segmented_apply_run_matches_the_reference_runner_on_generated_families; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_apply::segmented_apply_run_matches_the_reference_runner_on_generated_families)'`
- Property scope: `graphs of 1 to 5 nodes and up to 6 edges, owners in 1 to 3 segments and one more identifier possibly unowned, three edge allocations, deltas to a second graph of up to 6 nodes and 6 edges with identifiers up to 20; every cancellation point; 32 cases`
- Production certificate: `segmented_apply::a_segmented_apply_reads_only_the_owners_and_boundary_peers_of_what_it_names; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_apply::a_segmented_apply_reads_only_the_owners_and_boundary_peers_of_what_it_names)'`
- Production certificate: `segmented_apply::a_segmented_apply_run_declines_each_exceeded_budget_like_the_reference; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_apply::a_segmented_apply_run_declines_each_exceeded_budget_like_the_reference)'`
- Production certificate: `segmented_apply::a_segmented_apply_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_apply::a_segmented_apply_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run)'`
- Production certificate: `segmented_apply::a_refused_delta_fails_at_assemble_with_the_whole_state_refusal_and_seals_nothing; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_apply::a_refused_delta_fails_at_assemble_with_the_whole_state_refusal_and_seals_nothing)'`
- File: `crates/panproto-inst/src/segment/runner.rs`
- File: `crates/panproto-inst/src/segment/manifest.rs`
- File: `crates/panproto-inst/src/segment/apply.rs`
- File: `crates/panproto-inst/tests/reference_edits.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

## Segmented graph edits compose and invert as whole-state edits do {#L-segments-22}

### Statement

Let $P$ and $S$ satisfy the hypotheses of [L-segments-20](#L-segments-20) and write $\operatorname{seg}(\delta, S)$ for `apply_segmented` of $\delta$ over $S$ and $P$.

1. Composition. For graph deltas $a$ and $b$, if $\operatorname{seg}(a, S)$ returns a family $S'$, $\operatorname{seg}(b, S')$ returns a family $S''$, and `compose(a, b)` is defined, then $\operatorname{seg}(\operatorname{compose}(a, b), S) = S''$, segment by segment and byte for byte.
2. Inversion. If $\operatorname{seg}(\delta, S)$ returns $S'$, then $\operatorname{seg}(\operatorname{invert}(\delta), S') = S$, segment by segment and byte for byte.
3. Inversion as checkpointed runs. If `apply_run` of $\delta$ over a manifest $M$ sealing $S$ completes with manifest $M'$, then `apply_run` of $\operatorname{invert}(\delta)$ over $M'$, reading the emitted segments in place of the read ones and the input segments elsewhere, completes with $M$ with its instance digest cleared. The two manifests are equal byte for byte when $M$ carries no instance digest; when $M$ was sealed with one (`seal_full`), they differ in that field alone, and so in their digests, since an edited manifest carries none.

### Outside the domain

The converse of the composition claim does not hold. A composite may apply where its factors do not apply in turn: $a$ may add a node no range owns that $b$ deletes, in which case $\operatorname{seg}(a, S)$ is refused at layout while the composite, which never holds the node, applies. When `compose(a, b)` is undefined there is no composite to compare.

### Definitions used

`compose` and `invert` are the graph delta algebra of [L-edits-3](edits.md#L-edits-3) and [L-edits-2](edits.md#L-edits-2). Write $G = \operatorname{glue}_P(S)$.

### Proof

1. Composition. By [L-segments-20](#L-segments-20), $S' = \operatorname{split}_P(\operatorname{apply}(a, G))$ and $S'' = \operatorname{split}_P(\operatorname{apply}(b, G'))$ with $G' = \operatorname{apply}(a, G)$, both laid out without refusal. The net delta `compose(a, b)` applied to $G$ gives $\operatorname{apply}(b, G')$ ([L-edits-3](edits.md#L-edits-3)): it deletes, adds and changes exactly what $a$ followed by $b$ does, with the old values $a$ reads and the new values $b$ writes. Its added nodes and edges lie in the result $\operatorname{apply}(b, G')$, which $S''$ lays out without refusal, so the layout refusals of L-segments-20 do not arise, and $\operatorname{seg}(\operatorname{compose}(a, b), S) = \operatorname{split}_P(\operatorname{apply}(b, G')) = S''$.
2. Inversion. The inverse of a delta that applied applies to its result and restores the input ([L-edits-2](edits.md#L-edits-2)), so $\operatorname{apply}(\operatorname{invert}(\delta), \operatorname{apply}(\delta, G)) = G$. The family $S'$ satisfies the hypotheses of L-segments-20, and $G$ is laid out as $S$, so the segmented inverse returns $\operatorname{split}_P(G) = S$.
3. Runs. By [L-segments-21](#L-segments-21) the first run's manifest seals $S'$ with no instance digest and the second run's seals $\operatorname{split}_P(G) = S$ the same way, with the input's schema, binding and source role, which are those of $M$. Every other field of a manifest is a function of its records, which are the records of $S$, so the second manifest is $M$ with its instance digest cleared.
4. The finite certificate takes every ordered pair of the 37 atomic deltas of `segmented_compose_deltas` (over the node identifiers $0$ and $3$ and the edge identifiers $1$ and $10$) on each of the 32 families of `segmented_compose_families` (the 8 graphs over at most two nodes and one edge under four partitions with edge allocation ranges), $43{,}808$ pairs, and compares the composite's segments with those of the factors in turn and with the reference composite; $1{,}580$ pairs apply in turn, $36{,}408$ are refused at the first delta and $5{,}820$ at the second, and every pair that applies in turn composes. Inversion is checked on the $1{,}363$ cases of the every-97th slice of the carrier of L-segments-20 that apply, in memory against the reference, and as checkpointed runs on the $402$ cases of the every-331st slice whose run completes. A property test checks both claims with the reference differences from a generated graph to a second and from it to a third.

### Executable counterpart

- Law ID: `segments.segmented-graph-edit-algebra`
- Reference: `panproto_reference::segment::apply_graph_segmented`
- Reference: `panproto_reference::edit::Delta::compose`
- Reference: `panproto_reference::edit::Delta::invert`
- Enumerator: `panproto_reference::enumerate::segments::segmented_compose_families; bound: families=32, deltas=37, ordered pairs=43808`
- Exhaustive differential test: `segmented_apply::a_composite_delta_applies_to_a_segmented_family_as_its_factors_do_in_turn; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_apply::a_composite_delta_applies_to_a_segmented_family_as_its_factors_do_in_turn)'`
- Exhaustive differential test: `segmented_apply::the_inverse_of_an_applied_segmented_edit_restores_every_segment_byte_for_byte; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_apply::the_inverse_of_an_applied_segmented_edit_restores_every_segment_byte_for_byte)'`
- Production certificate: `segmented_apply::the_inverse_run_of_a_completed_segmented_edit_restores_the_manifest_but_its_instance_digest; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_apply::the_inverse_run_of_a_completed_segmented_edit_restores_the_manifest_but_its_instance_digest)'`
- Property differential test: `segmented_apply::composition_and_inversion_hold_over_segments_on_generated_families; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_apply::composition_and_inversion_hold_over_segments_on_generated_families)'`
- Property scope: `graphs of 1 to 5 nodes and up to 6 edges, two further graphs of up to 7 nodes and 6 edges with identifiers up to 20, owners in 1 to 3 segments, three edge allocations; 256 cases`
- File: `crates/panproto-inst/src/segment/apply.rs`
- File: `crates/panproto-inst/src/graph_edit/compose.rs`
- File: `crates/panproto-inst/tests/reference_edits.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

## A segmented table edit assembles to whole-state application and reads only the owners of the rows it names {#L-segments-23}

### Statement

Let $P$ be a table partition and $S$ a family of table segments that `verify_partition` accepts under $P$ and that glues to a table $T = \operatorname{glue}_P(S)$ (`assemble_table`), so that every pair of $T$ joins its edge's endpoint tables. Let $\delta$ be a table delta and write $\operatorname{apply}$ for whole-state application, `TableDelta::apply` ([L-edits-5](edits.md#L-edits-5)). Write $R(\delta)$ for the *read set*: the owners under $P$ of the rows $\delta$ names (`TableDelta::touched_rows`, the rows it deletes, adds or changes and the endpoints of the pairs it deletes or adds). Then `apply_segmented` of $\delta$ over $S$ and $P$ reads the segments of $R(\delta)$ and no other, and:

1. when $\operatorname{apply}(\delta, T)$ succeeds and every row it adds is owned, the output is $\operatorname{split}_P(\operatorname{apply}(\delta, T))$ (`partition_table`), so that $\operatorname{glue}_P$ of the output is $\operatorname{apply}(\delta, T)$, and every segment outside $R(\delta)$ is returned unchanged;
2. when $\operatorname{apply}(\delta, T)$ refuses, the output is that refusal;
3. otherwise the output is `Unowned` for the least added row no range covers, the refusal that splitting the edited table returns.

A row added under an identifier from `FreshIds::node` of the segment that will own it is never refused as unowned.

### Outside the domain

A family that `verify_partition` refuses is refused with its error before any delta check. A family whose pairs do not join their edge's tables does not glue, and is outside the domain: the facts of such a family need not reproduce the whole-state refusal. A table has no edge identifiers, so `FreshIds::edge` refuses with `NoAllocationRange` and no floor rule applies.

### Definitions used

The *incident* rows of a segment are the rows it owns that $\delta$ deletes or moves to another table. The *facts* of a read segment (`SegmentDelta::facts`) are the rows it owns that $\delta$ names, with their tables and cells; the pairs it holds that $\delta$ deletes or adds, and those with an incident endpoint; the in-stubs entering an incident row, as their pairs; and the table, as the segment holds it (its own row, a ghost's anchor or an in-stub's source anchor), of every endpoint of those pairs and in-stubs that is not a reported row. The *facts table* $F$ glues the facts of every read segment, giving an endpoint reported only by its table a row without cells in that table.

### Proof

1. A table change needs no boundary peer. Let $\delta$ move a row $r$ from table $t$ to $t' \ne t$ and keep a pair $p$ of edge $e$ at $r$, say with source $r$. Since $T$ is well formed, $e$ leaves $t$; check 6 of `TableDelta::check` asks $p$ to join $e$'s tables after $\delta$, so that $e$ leaves $t'$; both cannot hold. The same holds at the target end. Hence when $\delta$ applies, no kept pair meets a row whose table changes, and every ghost or in-stub copying such a row's table belongs to a pair $\delta$ deletes or adds.
2. Segments outside the read set do not change. A segment holds its own rows and the pairs whose source it owns, and a boundary of ghosts and in-stubs, each a copy of one crossing pair and the table of its remote endpoint. Every row $\delta$ names is owned in $R(\delta)$, and every pair it deletes or adds is held by the owner of its source and recorded as an in-stub by the owner of its target, both in $R(\delta)$. A cell change touches only the row's owner, and by step 1 a table change of an applied delta alters no boundary record outside the owners of the endpoints of pairs it deletes or adds. The record of a segment is a function of its content ([L-segments-4](#L-segments-4)), so every record outside $R(\delta)$ is unchanged.
3. The facts decide checks 1 to 6. Checks 1 and 3 read the current table and cells of each row $\delta$ deletes or changes, which its owner reports in full. Check 2 and the pair half of check 5 read whether each pair $\delta$ deletes or adds exists, which the owner of its source reports. Check 4 reads the pairs at each deleted row, which its owner holds as its own pairs or records as in-stubs, all reported; `incident_pairs` lists them in ascending order, and $F$ holds no other pair at that row, since every pair of $F$ is a pair of $T$. The row half of check 5 reads whether each added row exists, which its owner reports. Check 6 reads, for every added pair and every kept pair at a row whose table changes, the tables of its endpoints after $\delta$: an endpoint $\delta$ names is reported in full, and the remote endpoint of a reported pair is reported by its table, which the segment's ghost or in-stub copies from the owner. Every row of $F$ is a row of $T$ (a cell-less row standing only for an endpoint of a pair of $T$, whose cells no check reads), and every pair of $F$ is a pair of $T$. Thus $F$ and $T$ agree on everything the checks read, and the checks, run in their order on $F$, return the refusal they return on $T$.
4. Layout. With every check passed, $\operatorname{apply}(\delta, T)$ exists. Every row of $T$ is owned, so the least row `partition_table` refuses as `Unowned` is the least unowned added row; the endpoints of every pair of the result exist, so no pair is refused. Sealing a table split checks no identifier floor.
5. Rewriting. Each read segment applies $\delta$ to its own rows and pairs (a row belongs to it when the segment owns it, a pair when it owns the pair's source) and rebuilds its boundary from its pairs and in-stubs after $\delta$, taking each remote table after $\delta$ from $\delta$ itself (a table change or an added row), from the facts (which report every endpoint of an added pair) or, for a row $\delta$ leaves alone, from the segment's own copy. This is the segment `partition_table` writes for $\operatorname{apply}(\delta, T)$. Together with step 2, the output is $\operatorname{split}_P(\operatorname{apply}(\delta, T))$.
6. Fresh identifiers. `FreshIds::node` draws from the owner's node allocation range above every row it holds there; the owner's ranges cover it, and no other segment owns it, so it is free and owned.
7. The reference `apply_table_segmented` glues the family, applies the delta with the independent `edit::apply_table`, splits the result with `partition_table` and seals it, refusing at the first step that refuses; the reference `table_read_set` computes the read set from the delta and the partition. The finite certificate walks `segmented_table_apply_cases`: the 105 tables of the edit carrier (at most three rows in table `a`, each with or without the cell `f = 0`, and at most one pair under `a -e-> a`), the 19 partitions of the identifiers $0$ to $3$ of [L-segments-20](#L-segments-20) without allocation ranges, and the 43 atomic deltas over the rows $0$, $2$ and $3$ (each row deleted or added with two payloads, each of the nine pairs deleted or added, the cell `f` set or removed, and the table moved from `a` to `b` or back), $85{,}785$ cases. For each it compares the read set with the reference, every output segment's canonical bytes with the reference, the glued output with production whole-state application and that with the reference application, every unread segment with its input, and every refusal with the reference and with whole-state application. Of the cases, $19{,}375$ apply, $65{,}360$ are refused by the delta checks and $1{,}050$ by the layout. A property test compares the law on tables of up to six rows in tables `a` and `b` with pairs between any two of them, under the delta the reference differ gives toward a second table whose rows may change table or be unowned.

### Executable counterpart

- Law ID: `segments.segmented-table-apply`
- Reference: `panproto_reference::segment::apply_table_segmented`
- Reference: `panproto_reference::segment::table_read_set`
- Enumerator: `panproto_reference::enumerate::segments::segmented_table_apply_cases; bound: tables=105, partitions=19, deltas=43, cases=85785`
- Exhaustive differential test: `segmented_table_apply::segmented_table_apply_agrees_with_the_reference_on_every_case_of_the_carrier; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_table_apply::segmented_table_apply_agrees_with_the_reference_on_every_case_of_the_carrier)'`
- Property differential test: `segmented_table_apply::segmented_table_apply_agrees_with_the_reference_on_generated_families; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_table_apply::segmented_table_apply_agrees_with_the_reference_on_generated_families)'`
- Property scope: `tables of 1 to 6 rows in tables a and b (one cell value) with up to 8 pairs, owners in 1 to 3 segments for 6 identifiers and two more possibly unowned, deltas to a second table of up to 8 rows and 8 pairs; 256 cases`
- Production certificate: `segmented_table_apply::fresh_row_ids_come_from_the_owners_range_and_are_never_refused; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_table_apply::fresh_row_ids_come_from_the_owners_range_and_are_never_refused)'`
- File: `crates/panproto-inst/src/segment/apply.rs`
- File: `crates/panproto-inst/src/segment/table_apply.rs`
- File: `crates/panproto-inst/tests/reference_edits.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

## A checkpointed segmented table edit reads only its read set and resumes to the uninterrupted run {#L-segments-24}

### Statement

Let $P$, $S$ and $\delta$ satisfy the hypotheses of [L-segments-23](#L-segments-23), let $M$ be a manifest sealing $S$ under $P$ with trusted digest $d$, and let $B$ be a budget. Write $k$ for the size of $R(\delta)$, $\operatorname{run}_c$ for `apply_run` of $\delta$ over $M$, $d$ and the canonical bytes of $S$, under $B$, stopped once $c$ tasks are done, and $\operatorname{ref}_c$ for the reference runner `segmented_table_apply_with_progress` on the same inputs. The production run is `run_segmented` of the phase operation `SegmentApply` over a table delta, whose shape (`PhaseOperation::shape`) is the table shape and whose affected set is $R(\delta)$: $4k + 2$ tasks. For every $c$ from $0$ to $4k + 2$ the three claims of [L-segments-21](#L-segments-21) hold with these runs in place of the graph runs, the run identity being $\operatorname{RunJournal::run\_digest}(\lambda, d, P, \texttt{table apply})$ with $\lambda$ the delta's digest, `TableDelta::digest`.

An uninterrupted run reads each segment of $R(\delta)$ from its source once and no other segment. When it completes, the emitted segment of each read segment (`read_emitted_segment`) is the corresponding segment of $\operatorname{split}_P(\operatorname{apply}(\delta, T))$, its Emit artifact holds no complement section, and the output manifest, which verifies against its digest, is the seal of that split as a source with the input's schema and binding and no instance digest; the record of every segment outside $R(\delta)$ is its input record. When the delta is refused, the run fails at Assemble with the refusal of L-segments-23, after the $3k$ tasks of the first three phases, and its journal holds no Emit or Seal task: a refused edit seals nothing and leaves every segment as it was.

### Outside the domain

As for [L-segments-21](#L-segments-21): a manifest that fails verification against $d$, or one of another shape, is refused before any task; the affected segments and the working set decline the run before any task, the summary budget at Assemble after the refusal check, and no complement budget declines it; a store failure stops the run as incomplete and reports each staged artifact the journal does not name as an orphan.

### Definitions used

The artifacts are those of [L-segments-21](#L-segments-21), with Summarize writing a table segment's facts (`table_apply_facts_bytes`): its row records, then its pair records, then its in-stubs as pair records, then each reported endpoint's identifier and table name, each part counted and ascending. A table segment holds no edge identifier, so resealing merges no identifier run.

### Proof

1. The runner is shared with [L-segments-21](#L-segments-21): task order, accounting, identity, reuse and the reseal do not depend on the operation, and the runner refuses an input manifest whose shape is not the operation's. Steps 1, 3 and 5 of that proof hold as written, with the table read set of [L-segments-23](#L-segments-23) as the affected set; the identifier runs of step 3 are empty.
2. Outputs. Summarize writes the facts of one read segment. Assemble glues every read segment's facts and runs the checks of steps 3 and 4 of the proof of L-segments-23, returning the refusal before it commits. Emit rewrites one read segment, reading every Summarize artifact for the tables after the delta, as in step 5 of that proof.
3. Agreement with the reference. The reference defines the read set from the delta and the partition, each Summarize artifact from its own facts definition and canonical writers, the outcome from the glued table through L-segments-23's reference, each Emit artifact from its record and frame writers, and the manifest by sealing the whole edited family as a source and clearing the instance digest. The reference runner for graph and table edits is one runner over these definitions, as the production runner is.
4. The finite certificate takes every 31st case of the carrier of L-segments-23, $2{,}768$ cases ($617$ complete and $2{,}151$ refused), and compares the production and reference reports uninterrupted, at each of the $20{,}080$ cancellation points and after resuming; for a complete run it compares the reads of a counting source with the read set, the emitted segments with the reference byte for byte, and the manifest's canonical bytes with the reference's; for a refused run it checks the journal for an Emit or Seal task. Witnesses cover a family of four segments in which a cell change and a table change of a row without pairs read its owner alone, a pair between two segments reads both, and a row added under a fresh identifier with a pair to another segment reads the two owners, each with $4k + 2$ tasks and every unread record kept; a table change at a row with pairs, refused from its owner alone; a row deletion that leaves pairs, refused at Assemble with nothing sealed; each budget, compared with the reference at every cancellation point; and a store whose commit fails at each task, after which the resumed run ends as the uninterrupted run.

### Executable counterpart

- Law ID: `segments.checkpointed-table-apply`
- Reference: `panproto_reference::segment::segmented_table_apply_with_progress`
- Reference: `panproto_reference::segment::table_apply_facts_bytes`
- Enumerator: `panproto_reference::enumerate::segments::segmented_table_apply_cases; bound: every 31st case, cases=2768, cancellation points=20080`
- Exhaustive differential test: `segmented_table_apply::segmented_table_apply_run_matches_the_reference_runner_at_every_cancellation_point; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_table_apply::segmented_table_apply_run_matches_the_reference_runner_at_every_cancellation_point)'`
- Property differential test: `segmented_table_apply::segmented_table_apply_run_matches_the_reference_runner_on_generated_families; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_table_apply::segmented_table_apply_run_matches_the_reference_runner_on_generated_families)'`
- Property scope: `tables of 1 to 5 rows in tables a and b with up to 6 pairs, owners in 1 to 3 segments and one more identifier possibly unowned, deltas to a second table of up to 6 rows and 6 pairs; every cancellation point; 32 cases`
- Production certificate: `segmented_table_apply::a_segmented_table_apply_reads_only_the_owners_of_the_rows_it_names; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_table_apply::a_segmented_table_apply_reads_only_the_owners_of_the_rows_it_names)'`
- Production certificate: `segmented_table_apply::a_table_change_at_a_row_with_pairs_is_refused_from_its_owner_alone; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_table_apply::a_table_change_at_a_row_with_pairs_is_refused_from_its_owner_alone)'`
- Production certificate: `segmented_table_apply::a_refused_table_delta_fails_at_assemble_with_the_whole_state_refusal_and_seals_nothing; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_table_apply::a_refused_table_delta_fails_at_assemble_with_the_whole_state_refusal_and_seals_nothing)'`
- Production certificate: `segmented_table_apply::a_segmented_table_apply_run_declines_each_exceeded_budget_like_the_reference; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_table_apply::a_segmented_table_apply_run_declines_each_exceeded_budget_like_the_reference)'`
- Production certificate: `segmented_table_apply::a_segmented_table_apply_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_table_apply::a_segmented_table_apply_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run)'`
- File: `crates/panproto-inst/src/segment/runner.rs`
- File: `crates/panproto-inst/src/segment/apply.rs`
- File: `crates/panproto-inst/src/segment/table_apply.rs`
- File: `crates/panproto-inst/tests/reference_edits.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

## Segmented table edits compose and invert as whole-state edits do {#L-segments-25}

### Statement

Let $P$ and $S$ satisfy the hypotheses of [L-segments-23](#L-segments-23) and write $\operatorname{seg}(\delta, S)$ for `apply_segmented` of a table delta $\delta$ over $S$ and $P$. The three claims of [L-segments-22](#L-segments-22) hold for table deltas, with `TableDelta::compose` and `TableDelta::invert` ([L-edits-2](edits.md#L-edits-2)) in place of the graph algebra and `apply_run` of table deltas in place of the graph runs: a composite applies to $S$ as its factors do in turn, the inverse of an applied edit restores every segment byte for byte, and the inverse run over a completed run's manifest restores the input manifest with its instance digest cleared.

### Outside the domain

As for [L-segments-22](#L-segments-22), the converse of the composition claim does not hold: $a$ may add a row no range owns that $b$ deletes, so that $\operatorname{seg}(a, S)$ is refused at layout while the composite, which never holds the row, applies. When `compose(a, b)` is undefined there is no composite to compare.

### Definitions used

Write $T = \operatorname{glue}_P(S)$.

### Proof

1. The proof of [L-segments-22](#L-segments-22) uses of the graph case only the law it rests on, [L-segments-20](#L-segments-20) and [L-segments-21](#L-segments-21), and the facts that the net delta of two composable deltas applies as they do in turn and that the inverse of an applied delta restores its input ([L-edits-2](edits.md#L-edits-2) for both, for tables). With [L-segments-23](#L-segments-23) and [L-segments-24](#L-segments-24) in their place, each step holds as written: the composite's added rows lie in the result $S''$ lays out, so no layout refusal arises; the inverse returns $\operatorname{split}_P(T) = S$; and the inverse run's manifest seals $S$ with the input's schema, binding and role, and no instance digest.
2. The finite certificate takes every ordered pair of the 25 atomic deltas of `segmented_table_deltas` over the rows $0$ and $3$ on each of the 100 families of `segmented_table_compose_families` (the 25 tables over at most two rows under four partitions with row allocation ranges), $62{,}500$ pairs, and compares the composite's segments with those of the factors in turn and with the reference composite; $4{,}068$ pairs apply in turn and compose, $48{,}000$ are refused at the first delta and $10{,}432$ at the second, and every pair that applies in turn composes. Inversion is checked in memory against the reference on all $19{,}375$ cases of the carrier of L-segments-23 that apply, and as checkpointed runs on the $617$ cases of the every-31st slice whose run completes. A property test checks both claims with the reference differences from a generated table to a second and from it to a third.

### Executable counterpart

- Law ID: `segments.segmented-table-edit-algebra`
- Reference: `panproto_reference::segment::apply_table_segmented`
- Reference: `panproto_reference::edit::compose_table`
- Reference: `panproto_reference::edit::invert_table`
- Enumerator: `panproto_reference::enumerate::segments::segmented_table_compose_families; bound: families=100, deltas=25, ordered pairs=62500`
- Exhaustive differential test: `segmented_table_apply::a_composite_table_delta_applies_to_a_segmented_family_as_its_factors_do_in_turn; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_table_apply::a_composite_table_delta_applies_to_a_segmented_family_as_its_factors_do_in_turn)'`
- Exhaustive differential test: `segmented_table_apply::the_inverse_of_an_applied_segmented_table_edit_restores_every_segment_byte_for_byte; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_table_apply::the_inverse_of_an_applied_segmented_table_edit_restores_every_segment_byte_for_byte)'`
- Production certificate: `segmented_table_apply::the_inverse_run_of_a_completed_segmented_table_edit_restores_the_manifest_but_its_instance_digest; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_table_apply::the_inverse_run_of_a_completed_segmented_table_edit_restores_the_manifest_but_its_instance_digest)'`
- Property differential test: `segmented_table_apply::table_composition_and_inversion_hold_over_segments_on_generated_families; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_table_apply::table_composition_and_inversion_hold_over_segments_on_generated_families)'`
- Property scope: `tables of 1 to 5 rows with up to 6 pairs, two further tables of up to 7 rows and 6 pairs, owners in 1 to 3 segments; 256 cases`
- File: `crates/panproto-inst/src/segment/table_apply.rs`
- File: `crates/panproto-inst/src/table_delta.rs`
- File: `crates/panproto-inst/tests/reference_edits.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

## A segmented tree edit assembles to whole-state application and reads only the segments it can change {#L-segments-26}

### Statement

Let $P$ be a tree partition and $S$ a family of tree segments that `verify_partition` accepts under $P$ and that glues to a tree $W$ with its embedding $E$ (`assemble_tree`). Let $e$ be a tree edit and write $\operatorname{follow}(e, W, E)$ for whole-state application, `TreeEmbedding::follow_edit`: the tree edit applied step by step with the embedding following it as `GraphDelta::from_tree_edit` follows it ([L-edits-3](edits.md#L-edits-3)), fresh node and edge identifiers drawn from the embedding's counters. Write $R(e)$ for the *read set* defined below, computed from $e$ and the manifest records of $S$ alone. Then `apply_segmented` of $e$ over $S$ and $P$ reads the segments of $R(e)$ and no other, and:

1. when $\operatorname{follow}(e, W, E)$ succeeds and its split under $P$ (`partition_tree`) seals, the output is that split, so that $\operatorname{glue}_P$ of the output is the followed tree and embedding, and every segment outside $R(e)$ is returned unchanged;
2. when $\operatorname{follow}(e, W, E)$ refuses, the output is that refusal;
3. otherwise the output is the refusal of laying the edited tree out, in the order `partition_tree` and the seal check: `NotEmbedded` for the least node or edge identifier at or above its counter; `Unowned` for the least added tree node no range covers, then for the first fan whose fan node no range covers; and the seal floor rule's refusal for the least edge identifier it refuses for the segment holding it.

Relabelling the root, or joining features into it, keeps the tree's schema root equal to the root's anchor, so such an edit is laid out like any other.

### Outside the domain

A family that `verify_partition` refuses is refused with its error before any edit check. A fan inserted with an endpoint that is no tree node is refused by the edit with `NodeNotFound` on both sides ([L-edits-3](edits.md#L-edits-3)), which is case 2. The reference's tree application and production's disagree on one kind of step, which the reference differentials leave out: when an inserted node takes the identifier of a fan node, production relocates that fan node alone to the node counter and keeps its edge identifiers, while the reference also gives fresh identifiers to the fan's edges and to every later fan. Segmented application agrees with production whole-state application there, which a witness checks; which of the two whole-state semantics is intended is a defect of [L-edits-3](edits.md#L-edits-3), recorded here and not resolved.

### Definitions used

The *peers* of a segment are the segments its record lists in either cross map, and its *target peers* those its outgoing cross map lists. The *counters' segment* is the one segment whose record states the embedding's counters (`SegmentRecord::next_node` and `next_edge`), the owner of the root. The read set $R(e)$ is every segment when a step of $e$ is a `MoveSubtree` or a `JoinFeatures`: both check that a node is no ancestor of another, a walk to the root that may cross any segment. Otherwise it is the union over the steps of: for `SetField` and `RemoveField`, the node's owner; for `RelabelNode`, the owner and its peers; for `DeleteNode` and `ContractNode`, the owner, its peers, and the target peers of the owner and of every segment with a crossing edge into it; for `InsertNode`, the parent's owner, the child's owner and its target peers; for `InsertFan`, the owners of the fan's endpoints; for `DeleteFan`, every segment whose record counts a fan, with its target peers; for `ContractNode`, `InsertNode` and `InsertFan`, the counters' segment; and the owners of every node identifier the edit may draw. Those identifiers are found by tracking the node counter as an interval from the counters' segment's value: an insertion of child $c$ raises both ends to at least $c + 1$ and, when $c$ lies below the upper end, may draw an identifier of the interval (a relocated fan node) and widens it by one; a fan insertion draws an identifier of the interval and shifts it by one.

The *named* nodes of $e$ are the nodes its steps name (an insertion's parent and child, a fan's endpoints, the node a step deletes, contracts, relabels or changes a field of), or every node when a step moves a subtree or joins features. The *facts* of a read segment are its named tree nodes, its arcs with a named endpoint under their edge identifiers, its fans whose fan node or an endpoint is named or whose hyper-edge a step deletes, the anchor it holds of every other endpoint of those arcs and fans, and its root and counters when it holds them. The *partial tree* glues the facts of every read segment, each unreported endpoint standing in by its anchor, with the partial embedding of the reported arcs and fans; its root is the reported root, or an identifier no step names when the root's segment is not read.

### Proof

1. Segments outside the read set do not change. A tree segment holds its own nodes, the arcs whose parent it owns, the fans whose fan node it owns with their edges, and a boundary of ghosts and in-stubs copying each crossing arc or fan edge with the anchor of its remote endpoint. A step changes a node's payload (its owner), the arcs into and out of the nodes it deletes, contracts or inserts (held by the owners of their parents, which are the named node's owner or its peers, and copied by the owners of their children, its target peers), the fans meeting a deleted or contracted node (held by the owner or a segment with a crossing edge into it, and copied by that holder's target peers), the anchors a relabelled node's neighbours copy (its peers), and the counters (the counters' segment). A step's new elements are held by owners of nodes it names or draws. Over a sequence, a step can make two nodes adjacent only when each is named by it, drawn by it, or a neighbour of a node it names, so every neighbour a later step meets belongs to a segment the earlier steps already put in $R(e)$. The record of a segment is a function of its content ([L-segments-4](#L-segments-4)), so every record outside $R(e)$ is unchanged.
2. The facts decide every check. A step reads whether its named nodes exist (each owner is read and reports them), whether one is the root (the root's owner is read whenever the root is named, and the stand-in root matches no named node), the children of a node it deletes (its own arcs), the parent of a node it contracts (the arc into it, held by its parent's owner, a peer), the fans meeting a node it deletes or contracts and the fans of a hyper-edge it deletes (every holder is read), and, for a move or a join, the ancestors of a node (every segment is read and reports every node). The partial tree agrees with $W$ on all of these, so following $e$ on it refuses exactly where $\operatorname{follow}(e, W, E)$ refuses, with the same refusal.
3. Following the partial tree gives the restriction of following $W$. Each step removes arcs and fans and appends new ones; `TreeEmbedding::observe_edit` keeps the identifier of every arc whose parent is unchanged and gives fresh edge identifiers to the appended arcs in order, then keeps every fan that survives and gives fresh identifiers to new fans, relocating a fan node a new tree node takes. The partial arcs and fans are a subsequence of $W$'s in the same order and contain every arc and fan a step removes or appends, and the counters move only for the new nodes and the drawn identifiers, which the partial tree holds, so each fresh identifier is the one $\operatorname{follow}(e, W, E)$ draws.
4. Layout. With every step applied, the split's checks can fail only at an element the edit adds: a new identifier at or above its counter, a new node or fan node no range owns, or an edge identifier whose holder changes, which the seal floor rule may refuse. The partial tree holds every such element in fan order and identifier order, and the checks run in the split's order.
5. Rewriting. Each read segment removes what it reported, puts back what the followed partial tree holds in its place (its nodes, arcs, fans and fan nodes, the fan edges labelled from their targets' anchors after the edit), keeps its counters in step with the followed embedding when it holds the root, and rebuilds its boundary from its arcs and fan edges after the edit and the in-stubs of the elements it did not report. This is the segment `partition_tree` writes for the followed tree, and with step 1 the output is the split.
6. The reference `apply_tree_segmented` glues the family, applies the edit with the independent `edit::apply_tree`, splits the result with `partition_tree` and seals it, refusing at the first step that refuses; the reference `tree_read_set` computes the read set from the manifest records. The finite certificate walks `segmented_tree_apply_cases`: the 8 trees of `segmented_tree_apply_trees` (the root alone, `0 -> 1`, `0 -> 1, 0 -> 2` and `0 -> 1 -> 2`, each without and with a fan from its last node to the root), the 55 partitions of the identifiers $0$ to $4$ into at most three segments, the last possibly in none, and the 210 edits of `segmented_tree_edits` at the trees (every primitive at every node, with refusals for an existing child, an absent parent, absent nodes and absent fans), $11{,}550$ cases. For each it compares the read set computed from the segments and from the sealed manifest with the reference, every output segment's canonical bytes with the reference, the glued output with production whole-state application, every unread segment with its input, and every refusal with the reference and with whole-state application. Of the cases, $6{,}762$ apply, $4{,}620$ are refused by the edit and $168$ by the layout. A property test compares the law on trees of up to six nodes, with or without a fan, under sequences of up to three carrier edits, leaving out the kind of step above.

### Executable counterpart

- Law ID: `segments.segmented-tree-apply`
- Reference: `panproto_reference::segment::apply_tree_segmented`
- Reference: `panproto_reference::segment::tree_read_set`
- Enumerator: `panproto_reference::enumerate::segments::segmented_tree_apply_cases; bound: trees=8, partitions=55, edits=210, cases=11550`
- Exhaustive differential test: `segmented_tree_apply::segmented_tree_apply_agrees_with_the_reference_on_every_case_of_the_carrier; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_tree_apply::segmented_tree_apply_agrees_with_the_reference_on_every_case_of_the_carrier)'`
- Property differential test: `segmented_tree_apply::segmented_tree_apply_agrees_with_the_reference_on_generated_families; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_tree_apply::segmented_tree_apply_agrees_with_the_reference_on_generated_families)'`
- Property scope: `trees of 1 to 6 nodes with or without a fan, owners in 1 to 3 segments for 7 identifiers and two more possibly unowned, sequences of 1 to 3 carrier edits; 256 cases`
- Production certificate: `segmented_tree_apply::a_node_taking_a_fan_nodes_id_relocates_the_fan_into_the_counters_owner; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_tree_apply::a_node_taking_a_fan_nodes_id_relocates_the_fan_into_the_counters_owner)'`
- Production certificate: `segmented_tree_apply::a_fan_with_an_absent_endpoint_is_refused_naming_the_absent_node; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_tree_apply::a_fan_with_an_absent_endpoint_is_refused_naming_the_absent_node)'`
- File: `crates/panproto-inst/src/segment/apply.rs`
- File: `crates/panproto-inst/src/segment/tree_apply.rs`
- File: `crates/panproto-inst/src/tree_edit.rs`
- File: `crates/panproto-inst/tests/reference_edits.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

## A checkpointed segmented tree edit reads only its read set and resumes to the uninterrupted run {#L-segments-27}

### Statement

Let $P$, $S$ and $e$ satisfy the hypotheses of [L-segments-26](#L-segments-26), let $M$ be a manifest sealing $S$ under $P$ with trusted digest $d$, and let $B$ be a budget. Write $k$ for the size of $R(e)$, $\operatorname{run}_c$ for `apply_run` of $e$ over $M$, $d$ and the canonical bytes of $S$, under $B$, stopped once $c$ tasks are done, and $\operatorname{ref}_c$ for the reference runner `segmented_tree_apply_with_progress` on the same inputs. The production run is `run_segmented` of the phase operation `SegmentApply` over a tree edit, whose shape is the tree shape and whose affected set is $R(e)$: $4k + 2$ tasks. For every $c$ from $0$ to $4k + 2$ the three claims of [L-segments-21](#L-segments-21) hold with these runs in place of the graph runs, the run identity being $\operatorname{RunJournal::run\_digest}(\lambda, d, P, \texttt{tree apply})$ with $\lambda$ the edit's digest, `TreeEdit::digest`.

An uninterrupted run reads each segment of $R(e)$ from its source once and no other segment. When it completes, the emitted segment of each read segment is the corresponding segment of the split of $\operatorname{follow}(e, W, E)$, its Emit artifact holds no complement section, and the output manifest, which verifies against its digest, is the seal of that split as a source with the input's schema and binding and no instance digest; the record of every segment outside $R(e)$ is its input record. When the edit is refused, the run fails at Assemble with the refusal of L-segments-26, after the $3k$ tasks of the first three phases, and its journal holds no Emit or Seal task: a refused edit seals nothing and leaves every segment as it was.

### Outside the domain

As for [L-segments-21](#L-segments-21). The reference runner is compared on the cases of L-segments-26's reference differentials, which leave out the kind of step that section names.

### Definitions used

The artifacts are those of [L-segments-21](#L-segments-21), with Summarize writing a tree segment's facts (`tree_apply_facts_bytes`): its node records; its arcs as edge identifier, parent, child and label; its fans as fan node, hyper-edge, parent edge identifier, parent and each child as label, edge identifier and target in label order; each reported endpoint's identifier and anchor; each part counted and ascending; then the root and the two counters, each behind a presence byte. A manifest record states the counters of the segment holding them, so the read set, including the owners of the identifiers the edit may draw, is computed from the records alone.

### Proof

1. The runner is shared with [L-segments-21](#L-segments-21): task order, accounting, identity, reuse and the reseal do not depend on the operation, and the runner refuses an input manifest whose shape is not the operation's. Steps 1, 3 and 5 of that proof hold as written with the tree read set as the affected set; a tree segment's identifier run holds its arc and fan edge identifiers, and an identifier of a rewritten run is held by no segment outside $R(e)$, since a kept identifier stays with its holder or moves with a relocated fan to the segment owning the drawn identifier, and a fresh one is drawn above every identifier the family holds.
2. Outputs. Summarize writes the facts of one read segment. Assemble glues every read segment's facts into the partial tree, follows the edit on it and runs the layout checks of steps 2 to 4 of the proof of L-segments-26, returning the refusal before it commits. Emit rewrites one read segment from every Summarize artifact, as in step 5 of that proof.
3. Agreement with the reference. The reference defines the read set from the sealed records, each Summarize artifact from its own facts definition and canonical writers, the outcome from the glued tree through L-segments-26's reference, each Emit artifact from its record, identifier and frame writers, and the manifest by sealing the whole edited family as a source and clearing the instance digest. The reference runner for graph, table and tree edits is one runner over these definitions, as the production runner is.
4. The finite certificate takes every 37th case of the carrier of L-segments-26, $313$ cases ($188$ complete and $125$ refused), and compares the production and reference reports uninterrupted, at each of the $3{,}391$ cancellation points and after resuming; for a complete run it compares the reads of a counting source with the read set, the emitted segments with the reference byte for byte, and the manifest's canonical bytes with the reference's; for a refused run it checks the journal for an Emit or Seal task. Witnesses cover a family of four segments (the root with the fresh identifiers, two subtrees, and a fan node) in which a field change reads its owner alone, a relabelling its owner and the holders of the arc and fan edge into it, a fan deletion the fan's holder and its endpoints' owners, an insertion the parent's owner and the counters' segment with its target peers, a leaf deletion the holders of the edges into it and their target peers, and a move every segment, each with $4k + 2$ tasks and every unread record kept; the deletion of a node with a child, refused at Assemble with nothing sealed; each budget, compared with the reference at every cancellation point; and a store whose commit fails at each task, after which the resumed run ends as the uninterrupted run.

### Executable counterpart

- Law ID: `segments.checkpointed-tree-apply`
- Reference: `panproto_reference::segment::segmented_tree_apply_with_progress`
- Reference: `panproto_reference::segment::tree_apply_facts_bytes`
- Enumerator: `panproto_reference::enumerate::segments::segmented_tree_apply_cases; bound: every 37th case, cases=313, cancellation points=3391`
- Exhaustive differential test: `segmented_tree_apply::segmented_tree_apply_run_matches_the_reference_runner_at_every_cancellation_point; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_tree_apply::segmented_tree_apply_run_matches_the_reference_runner_at_every_cancellation_point)'`
- Property differential test: `segmented_tree_apply::segmented_tree_apply_run_matches_the_reference_runner_on_generated_families; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_tree_apply::segmented_tree_apply_run_matches_the_reference_runner_on_generated_families)'`
- Property scope: `trees of 1 to 5 nodes with or without a fan, owners in 1 to 3 segments for 6 identifiers and one more possibly unowned, sequences of 1 or 2 carrier edits; every cancellation point; 32 cases`
- Production certificate: `segmented_tree_apply::a_segmented_tree_apply_reads_only_the_owners_and_peers_of_what_it_names; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_tree_apply::a_segmented_tree_apply_reads_only_the_owners_and_peers_of_what_it_names)'`
- Production certificate: `segmented_tree_apply::a_refused_tree_edit_fails_at_assemble_with_the_whole_state_refusal_and_seals_nothing; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_tree_apply::a_refused_tree_edit_fails_at_assemble_with_the_whole_state_refusal_and_seals_nothing)'`
- Production certificate: `segmented_tree_apply::a_segmented_tree_apply_run_declines_each_exceeded_budget_like_the_reference; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_tree_apply::a_segmented_tree_apply_run_declines_each_exceeded_budget_like_the_reference)'`
- Production certificate: `segmented_tree_apply::a_segmented_tree_apply_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_tree_apply::a_segmented_tree_apply_run_stopped_by_a_failed_commit_at_any_task_resumes_to_the_uninterrupted_run)'`
- File: `crates/panproto-inst/src/segment/runner.rs`
- File: `crates/panproto-inst/src/segment/manifest.rs`
- File: `crates/panproto-inst/src/segment/tree_apply.rs`
- File: `crates/panproto-inst/tests/reference_edits.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

## Segmented tree edits compose as whole-state tree edits do {#L-segments-28}

### Statement

Let $P$ and $S$ satisfy the hypotheses of [L-segments-26](#L-segments-26) and write $\operatorname{seg}(e, S)$ for `apply_segmented` of a tree edit $e$ over $S$ and $P$. For tree edits $a$ and $b$, if $\operatorname{seg}(a, S)$ returns a family $S'$ and $\operatorname{seg}(b, S')$ returns a family $S''$, then $\operatorname{seg}(\operatorname{compose}(a, b), S) = S''$, segment by segment and byte for byte, with `TreeEdit::compose` the monoid product of the tree edit algebra.

### Outside the domain

The converse does not hold in general: $a$ may insert a node no range owns that $b$ deletes, in which case $\operatorname{seg}(a, S)$ is refused at layout while the composite, which never holds the node, applies. Tree edits have no inverse in their algebra (a deleted arc comes back only under a fresh identifier, [L-edits-3](edits.md#L-edits-3)), so there is no inversion claim; the graph delta a tree edit lowers to inverts as [L-segments-22](#L-segments-22) states.

### Definitions used

Write $(W, E) = \operatorname{glue}_P(S)$.

### Proof

1. `compose(a, b)` is the sequence of the steps of $a$ followed by those of $b$, and `follow_edit` follows a sequence step by step, so $\operatorname{follow}(\operatorname{compose}(a, b), W, E) = \operatorname{follow}(b, \operatorname{follow}(a, W, E))$. By [L-segments-26](#L-segments-26), $S'$ is the split of $\operatorname{follow}(a, W, E)$, which glues back to it, and $S''$ is the split of the composite's whole-state result, laid out without refusal; so the composite's layout refusals do not arise and $\operatorname{seg}(\operatorname{compose}(a, b), S) = S''$.
2. The finite certificate takes every ordered pair of the edits of `segmented_tree_edits` at each of the 8 carrier trees under three partitions (all identifiers in one segment, `0 | 1 2 | 3 4` and `0 3 | 1 4 | 2`), $3 \times 6{,}206 = 18{,}618$ pairs, and compares the composite's segments with those of the factors in turn and with the reference composite. Of the pairs, $6{,}081$ apply in turn and agree with the reference, $84$ apply in turn with the second step inserting a node under the identifier of the fan the first inserted, which only production whole-state application is compared with, $6{,}957$ are refused at the first edit and $5{,}496$ at the second, and the composite is refused exactly when the factors are refused in turn.

### Executable counterpart

- Law ID: `segments.segmented-tree-edit-algebra`
- Reference: `panproto_reference::segment::apply_tree_segmented`
- Enumerator: `panproto_reference::enumerate::segments::segmented_tree_edits; bound: trees=8, partitions=3, ordered pairs=18618`
- Exhaustive differential test: `segmented_tree_apply::a_composite_tree_edit_applies_to_a_segmented_family_as_its_factors_do_in_turn; command: cargo nextest run -p panproto-inst --test reference_edits --no-tests=fail -E 'test(=segmented_tree_apply::a_composite_tree_edit_applies_to_a_segmented_family_as_its_factors_do_in_turn)'`
- File: `crates/panproto-inst/src/segment/tree_apply.rs`
- File: `crates/panproto-inst/src/tree_edit.rs`
- File: `crates/panproto-inst/tests/reference_edits.rs`
- File: `crates/panproto-reference/src/enumerate/segments.rs`

## Carrying tasks over from an earlier run changes nothing but the reuse counts {#L-segments-29}

### Statement

Let $\pi$, $S$, $P$, the manifest $M$ with trusted digest $d$, the lens identity $\ell$ and the view schema $\sigma$ satisfy the hypotheses of [L-segments-12](#L-segments-12). Let $M'$, with trusted digest $d' \neq d$, seal a family $S'$ under the same partition $P$, and let a store $T$ hold the tasks an earlier `graph_restrict_run` of $\pi$ over $M'$, with lens identity $\ell$ and view schema $\sigma$, committed before it completed or stopped. Write $\operatorname{run}$ for `graph_restrict_run` of $\pi$ over $M$ against $T$ with `reuse` set to $d'$, and $\operatorname{fresh}$ for the same run against an empty store without `reuse`. Then:

1. $\operatorname{run}$ has the status, the manifest, the journal under its run identity and the committed artifacts of $\operatorname{fresh}$, and its report is the report of $\operatorname{fresh}$ except for the numbers of computed and reused tasks, overall and in each phase, whose sums agree;
2. a task of $\operatorname{run}$ is *carried over*, copied from the earlier run, committed under the run identity of $\operatorname{run}$ and counted as reused, exactly when it is not Seal, the earlier run holds it intact under the inputs digest $\operatorname{run}$ gives it, and the input segments it depends on have the same canonical bytes in $S$ and $S'$, where Validate and Export of segment $i$ depend on segment $i$, and a task after Validate also requires that the earlier run validated every segment of $P$, Summarize, Assemble and Emit depending on every segment;
3. $\operatorname{run}$ reads from its segment source exactly the segments whose Validate task it does not carry over, those whose canonical bytes differ between $S$ and $S'$ or that the earlier run did not validate, each once and in segment order;
4. the reference runner `segmented_restrict_with_progress`, given the same earlier run in its own store and the same `reuse`, returns the same report, both counts included.

When $d' = d$ the earlier run is the run itself, and running again is the resumption of [L-segments-12](#L-segments-12). Without held complements the inputs digest of a task is determined by the segments it depends on, so the condition on it adds nothing to the others; with held complements ([L-segments-30](#L-segments-30)) it adds, for Assemble and Emit, that the earlier run held the same complement bytes.

### Outside the domain

A phase operation is carried over only in its *portable* phases (`PhaseOperation::portable`), those whose tasks read nothing of the input manifest beyond their own segment's record; graph restriction is portable in every phase but Seal, and the default carries nothing. Segmented application ([L-segments-21](#L-segments-21)) reads the whole manifest's index at Assemble and Emit and carries nothing, so a retry after the manifest changes reuses no task ([L-edits-11](edits.md#L-edits-11)). An earlier run under another lens identity or partition has another run identity and is not found. An earlier run under another view schema carries no Emit task, since Emit's input digest names the view schema; clause 4 assumes the same view schema, and a caller that gives two plans one lens identity is outside the domain, as it is for resumption. A store failure while the earlier journal is read stops the run as incomplete before its first task. Seal is never carried over, since the manifest it writes names its input.

### Definitions used

The earlier run *holds* a task *intact* when its journal records the task and the stored artifact has the recorded length and output digest. The run identity, the input digest of a task and the phases are those of [L-segments-12](#L-segments-12). Digests are compared as the identity of the bytes they hash, as everywhere on this page.

### Proof

1. Dependency. By step 2 of the proof of [L-segments-12](#L-segments-12), the artifact of a task is a deterministic function of the plan, the partition, the segments the run reads, its key and the artifacts of the earlier tasks it reads: Validate of segment $i$ reads input segment $i$, Export its Validate, Summarize its Validate and every Export, Assemble every earlier task, and Emit its Validate, every Export and Summarize and Assemble, writing under $\sigma$. Under one plan, partition and view schema, induction over the phases gives: when the segments a task depends on (clause 2) have equal canonical bytes in $S$ and $S'$, and a task after Validate finds the same segments validated, its artifact in the earlier run equals its artifact in $\operatorname{run}$. Export reads only its segment; Summarize, Assemble and Emit read every Export, which is then equal segment by segment.
2. The rule production applies. Production carries a Validate task when the earlier input digest equals its own, which names the lens identity, the key and the segment's manifest record; two records are equal exactly when the segments' canonical bytes are, since a record is a function of its segment and binds its content digest ([L-segments-4](#L-segments-4)). It carries a later task when the earlier run validated exactly the segments of $P$, every Validate task the task depends on has the same output digest there, and the input digests agree, which by step 1 they then do, Emit's included under one view schema. In each case it reads the earlier artifact back and checks its length and digest. These are the conditions of clause 2, and by step 1 every carried artifact is the artifact $\operatorname{fresh}$ computes. Without held complements a task names nothing beyond earlier artifacts and its manifest record, so equal input segments give equal inputs digests by step 1; a run holding complements names their digests at Assemble and Emit as well, and production compares the journaled inputs digest, so a carried Assemble or Emit task was computed from the same held bytes.
3. Outputs. Every task of $\operatorname{run}$ thus obtains the artifact of $\operatorname{fresh}$: by computing it, by carrying it over (step 2), or from its own journal ([L-segments-12](#L-segments-12)). A carried task is committed with the input digest $\operatorname{run}$ computes from its own outputs, which is the record $\operatorname{fresh}$ commits. The journal, the artifacts and the manifest are therefore those of $\operatorname{fresh}$, and so are every budget check, the complement accounting, the progress, the phase peaks and the status, which read only those artifacts and the task order; only the classification of each task as computed or reused differs.
4. Source reads. Only a computed Validate task reads its input segment from the source; a carried task copies its artifact from the store.
5. Reference agreement. The reference decides carrying by reading the earlier run's Validate artifacts from its own store and comparing them with the canonical bytes of its own input segments, and by comparing the inputs digest the earlier run journaled with its own, and copies and commits a carried artifact as production does; production decides by the digests of the same artifacts. By step 2 the decisions agree, and the rest of the reference is the runner of [L-segments-12](#L-segments-12).
6. The finite certificate takes every 97th case of the carrier of [L-segments-11](#L-segments-11), $1{,}323$ cases. For a case with $n$ segments and $e$ edges it pairs the later run with an earlier run over the same segments sealed under another binding, complete and stopped at each of the $4n + 3$ cancellation points, and with a complete earlier run over the graph without each one of its edges, $4n + 4 + e$ pairs; summed over the cases, $22{,}051$ pairs. For each it compares the report with the reference's and, but for the counts, with the fresh run's, the journal, every committed artifact and the manifest with the fresh run's, the segments a counting source served with clause 3, and, for an earlier run over the same segments, the carried count with $\min(c, k, 4n + 1)$, where $c$ is the cancellation point and $k$ the number of tasks the fresh run completes. Of the pairs, $3{,}489$ carry every task but Seal, $17{,}006$ carry some and compute others, and $1{,}556$ carry none. A witness stops a carrying run by a failed commit at each task and resumes it to the uninterrupted carrying run, and a property test compares generated pairs of graphs, one the other with some of its edges removed, under generated partitions.

### Executable counterpart

- Law ID: `segments.carried-restrict-tasks`
- Reference: `panproto_reference::segment::segmented_restrict_with_progress`
- Enumerator: `panproto_reference::enumerate::segments::segmented_restrict_cases; bound: every 97th case, cases=1323, pairs=22051`
- Exhaustive differential test: `segmented_carry::carrying_tasks_over_changes_nothing_but_the_reuse_counts_on_every_97th_case; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_carry::carrying_tasks_over_changes_nothing_but_the_reuse_counts_on_every_97th_case)'`
- Property differential test: `segmented_randomized::carrying_tasks_over_matches_the_reference_runner_on_generated_graph_pairs; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_randomized::carrying_tasks_over_matches_the_reference_runner_on_generated_graph_pairs)'`
- Property scope: `graphs of 2 to 8 nodes over a three-vertex schema with every edge and loop, up to 14 edges, every survival choice, owners from 2 to 4 segments, and an earlier graph without a generated subset of those edges; carrying and reference reports, fresh-run reports, journals, artifacts and manifests, and source reads; 64 cases`
- Production certificate: `segmented_carry::a_carrying_run_stopped_by_a_failed_commit_at_each_task_resumes_to_the_uninterrupted_run; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_carry::a_carrying_run_stopped_by_a_failed_commit_at_each_task_resumes_to_the_uninterrupted_run)'`
- File: `crates/panproto-inst/src/segment/runner.rs`
- File: `crates/panproto-inst/tests/reference_restrict.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

## Held complements number synthesized elements stably {#L-segments-30}

### Statement

Let $\pi$, $S$ and $P$ satisfy the hypotheses of [L-segments-11](#L-segments-11), and let $H = (H_i)$ hold one graph complement segment per segment $i$ of $P$, for instance the complement segments of an earlier restriction under $P$. For a synthesis key $K$ that $H_i$ records, write $\nu_i(K)$ for the least identifier $H_i$ records it under, and call $K$ *persisting in segment $i$* when $\nu_i(K)$ lies in segment $i$'s allocation range of $K$'s kind, is not $2^{32} - 1$, and segment $i$ of $S$ holds no element of that kind under it. Write $\operatorname{held}(H)$ for `graph_restrict_segmented_held` of $\pi$ over $S$ under $P$ with $H$, and $(V^0, C^0)$ for `graph_restrict_segmented` of $\pi$ over $S$. Then:

1. every refusal before numbering is that of $(V^0, C^0)$; numbering refuses `InvertedRange` and `NoAllocationRange` for a segment and kind exactly when fresh numbering does, and `AllocationExhausted` for a segment and kind only when fresh numbering also runs out there (fresh numbering may run out where stable numbering, keeping identifiers below the floor, does not); the refusal returned is the first in segment order, nodes before edges;
2. otherwise it returns $(V, C)$ in which each segment $i$ gives a key $K$ persisting in segment $i$ the identifier $\nu_i(K)$, and every other key of segment $i$ of each kind, in ascending key order, the least identifier of the range that is at least the range's floor over the source identifiers of segment $i$ and that no key of segment $i$ took before; each floor of $C_i$ is one above the greatest source identifier or numbered identifier in its range, and at least the range's lower bound. $V$ and $C$ equal $V^0$ and $C^0$ but for the identifiers of the synthesized elements, and $V$ is a valid family under $P$ satisfying the seal floor rule;
3. $\operatorname{assemble}_P(V, C) = \operatorname{restrict}_\pi(\operatorname{glue}_P(S))$, as in [L-segments-11](#L-segments-11);
4. with $H$ empty, $\operatorname{held}(H) = (V^0, C^0)$; and with $H = C'$ for any output $(V', C')$ of `graph_restrict_segmented_held` of $\pi$ over $S$ under $P$, $\operatorname{held}(H) = (V', C')$ byte for byte, so a restriction holding its own complements over an unchanged family reproduces itself;
5. segment by segment, $\operatorname{held}(H)$ equals the reference definition `restrict_graph_segmented_held`;
6. the checkpointed run of `GraphRestrict` with `held` set to the canonical bytes of $H$ satisfies [L-segments-12](#L-segments-12) with the outputs of item 2 and the reference runner given $H$, its run identity is that of the run without $H$, the inputs digest of Assemble names the digest of every $H_i$ and that of the Emit task of segment $i$ the digest of $H_i$, and [L-segments-29](#L-segments-29) holds for runs holding complements: a carried Assemble or Emit task was computed from the same held bytes, and the segments a run reads from its source are those clause 3 of [L-segments-29](#L-segments-29) names, whatever $H$ holds.

### Outside the domain

A family outside the hypotheses of [L-segments-11](#L-segments-11) is refused or declined as there; holding complements changes no refusal before numbering. In a checkpointed run, held bytes that are not a graph complement segment are refused at Assemble with the decode refusal of [L-segments-14](#L-segments-14), and a held source that cannot serve a segment's bytes stops the run as incomplete, a store failure, at the first task that names them; the reference takes the held complements as values, so it has neither case. A held identifier outside the segment's range, equal to $2^{32} - 1$, or held by the source segment is not kept, so the rule needs no refusal of its own: such a key is numbered as a new one. A run identity does not name $H$, so a run holding complements and one holding none over the same manifest share a journal; their Assemble and Emit tasks have different inputs digests, so each recomputes rather than reuses the other's.

### Definitions used

A *synthesis key* names the chain a synthesized element belongs to and its place on it (L-segments-11). The *floor* of a range over a set of identifiers is one above the greatest of them in the range, and the range's lower bound when none lies in it, saturating: the allocation floor of [L-segments-10](#L-segments-10). The keys of a segment are those of the chains it owns, as in [L-segments-11](#L-segments-11).

### Proof

1. Refusals. The stages, the exchange, the reach summaries and their closure read no complement, so every refusal before numbering is that of [L-segments-11](#L-segments-11). Numbering checks each declared range as allocation does and refuses `NoAllocationRange` exactly when a segment with keys of a kind declares no range of it. Let a segment have $k$ keys of a kind in a range with floor $f$ and upper bound $h$; fresh numbering runs out exactly when $f + k - 1 > h$ or reaches $2^{32} - 1$. Stable numbering gives the $j$-th new key the least identifier at or above $f$ that no persisting key holds, which is at most $f + j - 1 + p$, where $p$ counts the persisting keys holding identifiers at or above $f$; since $j + p \le k$, it stays within $f + k - 1$ and never runs out where fresh numbering does not. This is item 1.
2. Identifiers. A persisting key's identifier lies in the segment's range and is no source identifier of the segment; a new key's identifier lies in the range at or above the floor over the source identifiers, so above every source identifier the segment holds in the range, and avoids every identifier the segment took before. The identifiers of one segment are thus distinct from each other and from its source identifiers. No other segment holds a node in a node allocation range, which lies inside its segment's nodes, and by the floor rule no other segment holds an edge identifier in an edge allocation range, so step 6 of the proof of [L-segments-11](#L-segments-11) applies with these identifiers in place of the appended ones: the view segments form a family under $P$ and satisfy the floor rule. Only the identifiers of the synthesized elements differ from $(V^0, C^0)$, whose other entries are computed by the stages. The floors are those item 2 states by construction. This is item 2.
3. Assembly renames every synthesized element by its key, whatever identifier it holds, so item 3 is step 6 of the proof of [L-segments-11](#L-segments-11).
4. With $H$ empty no key persists, and the new keys of each kind take consecutive identifiers from the floor, which is the append-floor allocation of `graph_restrict_segmented`. For $H = C'$, every key $K$ of segment $i$ is recorded in $C'_i$ under one identifier $x$, which $C'_i$ took in the range, never $2^{32} - 1$, and never a source identifier of segment $i$ (step 2 applied to $C'$); the source is the same, so $K$ persists and keeps $x$. Every key persists, so the floors are again one above the greatest identifier, and every other entry is computed by the stages; the output is $(V', C')$. This is item 4.
5. The reference restricts the glued graph whole, gives each chain to its owner as in [L-segments-11](#L-segments-11), and numbers each segment's keys by the rule of item 2 over the source identifiers its owner holds, so step 7 of the proof of [L-segments-11](#L-segments-11) carries over: item 5.
6. In a checkpointed run Assemble and Emit number each segment with its held complement, read through the held source, and their inputs digests name the digests of the bytes they read; Validate, Export and Summarize read no held complement. Every task is thus a deterministic function of what its inputs digest names, which is what the proofs of [L-segments-12](#L-segments-12) and [L-segments-29](#L-segments-29) use; production carries a task over only under the journaled inputs digest, so a carried Assemble or Emit task was computed from the same held bytes, and the Validate tasks, which decide the source reads, are those of the run without $H$. The reference runner names the canonical bytes of the held complements in the inputs of Assemble and Emit and compares the earlier run's journaled inputs digest with its own, so its decisions agree. This is item 6.
7. The finite certificate takes every 7th case of the carrier of [L-segments-11](#L-segments-11), $\lceil 128{,}240 / 7 \rceil = 18{,}320$ cases, of which $16{,}280$ restrict. For each it restricts again holding its own complements and compares the output byte for byte (item 4), then restricts, holding them, the graph without each edge and, for each segment owning a node, the graph with an edge numbered at the lower bound of that segment's edge range (where its first synthesized edge was numbered), and restricts the whole graph holding the complements of the graph without each edge that restricts: $66{,}144$ later restrictions, of which $1{,}764$ are refused alike. Each is compared with the reference and with whole-state restriction of the glued graph, and every persisting key is checked to keep its identifier: $364$ keep it, and $216$ persisting keys whose identifier an added edge took are numbered anew. In this carrier the stable numbering never differs from fresh numbering; a witness shows a persisting key keeping its identifier while a new key takes the identifier a deleted chain freed, where fresh numbering shifts both. The checkpointed certificate takes every 97th case, $1{,}323$, of which $1{,}163$ complete; it holds each first run's emitted complements in later runs over the same graph and its changed graphs, $4{,}769$ later runs of which $4{,}606$ complete, carrying tasks over from the first run ($18{,}687$ tasks carried), and checks each against the reference runner, a fresh run, the in-memory output and the source reads, and the first changed graph of each case from all $16{,}166$ cancellation points. A witness stops a run holding complements by a failed commit at each task and resumes it, and property tests generate graph pairs, the later one with edges at generated allocation floors.

### Executable counterpart

- Law ID: `segments.held-restrict-numbering`
- Reference: `panproto_reference::segment::restrict_graph_segmented_held`
- Reference: `panproto_reference::segment::segmented_restrict_with_progress`
- Enumerator: `panproto_reference::enumerate::segments::segmented_restrict_cases; bound: every 7th case, cases=18320, later restrictions=66144; every 97th case, cases=1323, later runs=4769`
- Exhaustive differential test: `segmented_held::stable_numbering_agrees_with_the_reference_reproduces_an_unchanged_family_and_keeps_persisting_ids; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_held::stable_numbering_agrees_with_the_reference_reproduces_an_unchanged_family_and_keeps_persisting_ids)'`
- Exhaustive differential test: `segmented_held::a_checkpointed_restriction_holding_complements_matches_the_reference_runner_on_every_97th_case; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_held::a_checkpointed_restriction_holding_complements_matches_the_reference_runner_on_every_97th_case)'`
- Property differential test: `segmented_randomized::stable_numbering_matches_the_reference_on_generated_graph_pairs; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_randomized::stable_numbering_matches_the_reference_on_generated_graph_pairs)'`
- Property scope: `graphs of 2 to 8 nodes over a three-vertex schema with every edge and loop, up to 14 edges, every survival choice, owners from 2 to 4 segments; held complements of the graph without a generated subset of its edges, the later graph with edges at generated allocation floors; production against the reference, assembly against whole-state restriction, persisting identifiers; 256 cases`
- Property differential test: `segmented_randomized::a_checkpointed_restriction_holding_complements_matches_the_reference_runner_on_generated_graph_pairs; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_randomized::a_checkpointed_restriction_holding_complements_matches_the_reference_runner_on_generated_graph_pairs)'`
- Property scope: `the same generated pairs as checkpointed runs carrying tasks over from the earlier run; reports against the reference runner, fresh runs, emitted segments, source reads and every cancellation point; 64 cases`
- Production certificate: `segmented_held::stable_numbering_keeps_a_persisting_id_and_reuses_a_freed_one_where_fresh_numbering_shifts; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_held::stable_numbering_keeps_a_persisting_id_and_reuses_a_freed_one_where_fresh_numbering_shifts)'`
- Production certificate: `segmented_held::a_held_carrying_run_stopped_by_a_failed_commit_at_each_task_resumes_to_the_uninterrupted_run; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_held::a_held_carrying_run_stopped_by_a_failed_commit_at_each_task_resumes_to_the_uninterrupted_run)'`
- File: `crates/panproto-inst/src/segment/restrict.rs`
- File: `crates/panproto-inst/src/segment/runner.rs`
- File: `crates/panproto-inst/tests/reference_restrict.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`

## The segmented graph edit lens translates edits as the whole-state lens does, up to renaming by synthesis key {#L-segments-31}

### Statement

Let $\ell$ be a graph lens whose plan $\pi$ compiles and carries no expression, $P$ a partition, and $S_0$ a source family under $P$ satisfying the hypotheses of [L-segments-11](#L-segments-11), sealed by a manifest with trusted digest $d_0$. `SegmentedGraphEditLens::segmented` binds $\ell$ to $S_0$ by the checkpointed get of [L-segments-15](#L-segments-15) and holds the source family $S_t$, the view segments $V_t$ and the complement segments $C_t$, the last two as the Emit artifacts of one restriction run. Write $\rho_t$ for the bijection by synthesis key from the identifiers of the synthesized elements of the whole-state graph edit lens ([L-edits-9](edits.md#L-edits-9)) bound to $\operatorname{glue}_P(S_0)$ and given the same edits, to those of $C_t$; $\rho_t$ fixes every source identifier. Write $\operatorname{IC}(X, Y, d)$ for the incidence-closed replacement differ from $X$ to $Y$ keeping the replacements of $d$. For a source delta $d$:

1. `get_edit(d)` is admitted exactly when the reference definition `segmented_edit_get` admits it; then its source delta is $d$, every segment of $S_{t+1}$, $V_{t+1}$ and $C_{t+1}$ is the reference's byte for byte, and its view delta is the reference's, $\operatorname{IC}(\operatorname{glue}_P V_t, \operatorname{glue}_P V_{t+1}, d)$;
2. with the whole-state lens in the state $(\operatorname{glue}_P S_t, W_t, D_t)$ and translating $d$ to $(W_{t+1}, D_{t+1})$ and the view delta $w$: $\operatorname{glue}_P S_{t+1}$ is its source, $\operatorname{glue}_P V_{t+1} = \rho_{t+1} W_{t+1}$, the joined entries of $C_{t+1}$ other than synthesized elements and floors are those of $D_{t+1}$, the view delta is $\operatorname{IC}(\rho_t W_t, \rho_{t+1} W_{t+1}, d)$, and when $\rho_t \cup \rho_{t+1}$ is a bijection it is $(\rho_t \cup \rho_{t+1})\,w$;
3. `put_edit(e)` for a view delta $e$ is admitted exactly when the reference definition `segmented_edit_put` admits it; then its source delta and every segment afterwards are the reference's, its view delta is $e$, and when every identifier $e$ names that is a whole-state synthesized identifier is also a segmented one, the whole-state lens given $\rho_t^{-1} e$ gives the same source delta and a view that $\rho_{t+1}$ renames to $\operatorname{glue}_P V_{t+1}$;
4. a refused edit, and an edit whose run stops, leaves the lens and every segment it holds unchanged; translating the same edit again resumes the stopped run and ends as the uninterrupted translation does, with nothing left staged;
5. `get_edit` reads from the family, in order, the segments of the edit run's read set ([L-segments-21](#L-segments-21)) and then the segments whose source records changed, each but those whose Validate task the store already journals under the same run. Export runs again for those segments only; Summarize, Assemble and Emit run again for every segment whenever a record changed, and Assemble and Emit also whenever the held complements differ from the previous run's, reading the store and not the family.

### Outside the domain

A lens whose plan carries an expression is declined when bound with `Residual::Global`, before any task, as in [L-segments-15](#L-segments-15). An edit that adds a node no segment owns, or an edge identifier the seal floor rule refuses, is refused with `ShapeEditError::Segment`, a refusal of layout the whole-state lens does not have; the certificates give every added node an owner, and no letter of theirs is refused this way. A store or source failure stops a step with `ShapeEditError::Incomplete`. A single renaming of the whole-state view delta is not the law in general: the whole-state lens gives a new synthesized element the least free identifier at or above one global floor, reusing identifiers deleted chains freed, and renumbers a persisting key whose identifier a new source element takes, while segmented numbering is per owner range ([L-segments-30](#L-segments-30)). An edit that deletes a contracted pair owned by one segment and creates one owned by another may thus be a replacement under one identifier whole-state and a deletion and an addition under two identifiers segmented; then $\rho_t \cup \rho_{t+1}$ maps one identifier to two, and no single renaming relates the deltas.

### Definitions used

The *read set* of a delta, the edit run and its refusals are those of [L-segments-20](#L-segments-20) and [L-segments-21](#L-segments-21); the restriction holding complements is that of [L-segments-30](#L-segments-30); the stable presentation of the whole-state lens is that of [L-edits-9](edits.md#L-edits-9). The *cross peers* of a segment are the segments its manifest record names in its cross digests.

### Proof

1. Forward step. `get_edit` runs the edit run of $d$ on $S_t$, whose output is segmented application of $d$ ([L-segments-20](#L-segments-20)), then the restriction of the edited family holding $C_t$ and carrying tasks over from the previous get, whose output is the reference definition of [L-segments-30](#L-segments-30) by item 6 there and [L-segments-29](#L-segments-29). Both refusals are the reference's: application refusals, restriction refusals and layout refusals. The view delta is computed over the view segments whose records changed, the segments owning an element $d$ replaces, and their cross peers. Every element outside them is held, with its payload, by both views; an edge incident to a node of a read segment is held by that segment or, entering it from elsewhere, by a cross peer; so the incidence closure and the kept replacements are those of the whole glued views, and the delta is $\operatorname{IC}(\operatorname{glue}_P V_t, \operatorname{glue}_P V_{t+1}, d)$. This is item 1.
2. Relation to the whole-state lens. The two lenses apply the same $d$ to the same glued source. The whole-state view is a stable presentation of whole-state restriction of the edited source and the glued segmented view is the assembly of [L-segments-30](#L-segments-30), so both are restriction of the same graph with synthesized elements renamed by key: $\operatorname{glue}_P V_{t+1} = \rho_{t+1} W_{t+1}$, and the other complement entries agree. Substituting into item 1 gives the view delta. When $\rho = \rho_t \cup \rho_{t+1}$ is a bijection, renaming both states by $\rho$ commutes with the differ, whose entries are determined by the identifiers and payloads of the two states and the replacements of $d$, which name source identifiers $\rho$ fixes; so the view delta is $\rho\,w$. This is item 2.
3. Backward step. `put_edit` applies $e$ to the view family (segmented application), puts the edited view back segment by segment against $C_t$ ([L-segments-16](#L-segments-16), [L-segments-17](#L-segments-17)), takes the source delta as the closed differ over the source segments the same rule names, translates it forward by item 1, and compares the forward view with the edited view segment for segment. The reference composes the same definitions on glued values, and by item 1 and the correctness of segmented put the outputs agree. Whole-state put of the glued edited view, renamed back by $\rho_t^{-1}$, is the put the whole-state lens performs when $e$ is renamed so, which gives item 3.
4. Atomicity. A step computes every run against the store and swaps the held manifests and run only after the last run seals; a refusal or a stopped run returns before the swap, so nothing the lens holds changes, and the store keeps the committed artifacts, which a retry reuses by [L-segments-12](#L-segments-12) and [L-segments-21](#L-segments-21), since each run's identity names the delta and its input manifest.
5. Reads. The edit run reads its read set ([L-segments-21](#L-segments-21)); the restriction reads exactly the segments whose Validate task it neither carries over nor finds journaled ([L-segments-29](#L-segments-29)), and a Validate task is carried over exactly when the segment's record is unchanged. Export of a segment depends on that segment's Validate task alone; Summarize, Assemble and Emit depend on every segment's, and Assemble and Emit name the held complements in their inputs ([L-segments-30](#L-segments-30)), so they are carried over only when no record and no held complement changed.
6. The finite certificate takes every 97th case of the carrier of [L-segments-11](#L-segments-11), $1{,}323$ cases, with the identifiers $3$ to $99$ given to the last segment so that a node an edit adds has an owner. $1{,}163$ bind and $160$ are refused alike at binding. For each bound case it translates every one-letter source edit of the edit-lens letter carrier ([L-edits-9](edits.md#L-edits-9)) and, after each admitted one, its inverse: $59{,}115$ letters, of which $53{,}782$ are admitted and $5{,}333$ refused alike. Every admitted letter is checked for items 1, 2 and 5; $53{,}734$ have one bijective renaming and $48$ do not. It puts back every one-letter view edit of each bound case's glued view: $17{,}565$ letters, $9{,}273$ admitted and compared with the whole-state lens, $8{,}292$ refused alike. Witnesses stop a source edit at each of its $28$ commits and a put at each of its $52$, and resume each; property tests translate generated words of source edits and put back every view letter of generated cases; and a lens with an expression is declined when bound.

### Executable counterpart

- Law ID: `segments.segmented-graph-edit-lens`
- Reference: `panproto_reference::segment::bind_segmented_edit`
- Reference: `panproto_reference::segment::segmented_edit_get`
- Reference: `panproto_reference::segment::segmented_edit_put`
- Enumerator: `panproto_reference::enumerate::segments::segmented_restrict_cases; bound: every 97th case, cases=1323, source letters=59115, view letters=17565`
- Exhaustive differential test: `segmented_graph_edit_lens::segmented_graph_edit_lens_agrees_with_the_reference_and_the_whole_state_lens_on_every_97th_case; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_edit_lens::segmented_graph_edit_lens_agrees_with_the_reference_and_the_whole_state_lens_on_every_97th_case)'`
- Exhaustive differential test: `segmented_graph_edit_lens::segmented_graph_edit_lens_puts_view_edits_as_the_reference_on_every_97th_case; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_edit_lens::segmented_graph_edit_lens_puts_view_edits_as_the_reference_on_every_97th_case)'`
- Property differential test: `segmented_graph_edit_lens::segmented_graph_edit_lens_agrees_with_the_reference_on_generated_words; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_edit_lens::segmented_graph_edit_lens_agrees_with_the_reference_on_generated_words)'`
- Property scope: `graphs of 2 to 8 nodes over a three-vertex schema with every edge and loop, up to 14 edges, every survival choice, owners from 2 to 4 segments; words of 1 to 5 source letters translated against the reference and the whole-state lens, with the source reads; 64 cases`
- Property differential test: `segmented_graph_edit_lens::segmented_graph_edit_lens_puts_view_edits_as_the_reference_on_generated_cases; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_edit_lens::segmented_graph_edit_lens_puts_view_edits_as_the_reference_on_generated_cases)'`
- Property scope: `the same generated cases, every view letter of the glued view put back against the reference and the whole-state lens; 64 cases`
- Production certificate: `segmented_graph_edit_lens::an_edit_stopped_by_a_failed_commit_at_each_task_resumes_to_the_uninterrupted_edit; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_edit_lens::an_edit_stopped_by_a_failed_commit_at_each_task_resumes_to_the_uninterrupted_edit)'`
- Production certificate: `segmented_graph_edit_lens::a_put_stopped_by_a_failed_commit_at_each_task_resumes_to_the_uninterrupted_put; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_edit_lens::a_put_stopped_by_a_failed_commit_at_each_task_resumes_to_the_uninterrupted_put)'`
- Production certificate: `segmented_graph_edit_lens::a_lens_with_an_expression_is_declined_when_bound; command: cargo nextest run -p panproto-lens --test reference_lenses --no-tests=fail -E 'test(=segmented_graph_edit_lens::a_lens_with_an_expression_is_declined_when_bound)'`
- File: `crates/panproto-lens/src/segmented_edit_lens.rs`
- File: `crates/panproto-lens/src/shape_edit_lens.rs`
- File: `crates/panproto-lens/tests/reference_lenses.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
