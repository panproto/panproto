# Segments

A large instance may need to be stored in pieces without changing the instance it denotes. Segment correctness fixes the conditions under which this is possible: identifiers remain global, each node has one owner, boundary records describe every crossing edge, and assembling the pieces recovers the original value. These conditions form the *segment agreement*. The first ten laws below cover the segment foundation; the eleventh runs graph restriction segment by segment, and the twelfth runs it as a checkpointed sequence of tasks that resumes after any interruption. Storage backends and segmented edits belong to later components.

Let $I$ be a finite tree, table, or graph instance, and let $P$ be a partition for the same shape. We write $P(n)$ for the segment that owns node or row identifier $n$, $\operatorname{split}_P(I)$ for partitioning, and $\operatorname{glue}_P$ for checked assembly. An edge is owned by the owner of its source endpoint. The finite differential carrier contains 25 labelled graphs and all 290 assignments of their nodes to two or three labelled segments, together with two fan-bearing trees and all 40 assignments of their tree and fan nodes to two segments; one crossing-boundary witness for each instance shape, and one for each fan-bearing tree, checks the shape-specific representation.

First, the page establishes ownership, boundary, and assembly laws. Second, it fixes canonical artifacts, the storage vocabulary, and typed refusals. Third, it states the validation and allocation boundaries that the segment core must preserve. Fourth, it shows that graph restriction run segment by segment, with one boundary exchange and a reachability summary, assembles to whole-state restriction, and that the checkpointed run of it reports and resumes as its reference runner does.

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

Let $\pi$ be a compiled restrict plan whose migration carries no expression: no survival predicate, no term assignment, and no field transform other than renaming, dropping, adding or keeping fields. Let $S$ be a valid graph segment family under a partition $P$ whose every declared edge allocation range starts above the greatest edge identifier $S$ holds. Then `graph_restrict_segmented` returns, for each segment $i$ of $P$, a view segment $V_i$ and a complement segment $C_i$, computed from segment $i$ and boundary facts alone, and

$$\operatorname{assemble}_P(V, C) = \operatorname{restrict}_\pi(\operatorname{glue}_P(S)),$$

where $\operatorname{restrict}_\pi$ is whole-state graph restriction and $\operatorname{assemble}_P$ glues the view segments, joins the complement segments, and numbers the synthesized elements in ascending key order from one above the greatest source identifier of each kind. The view segments form a valid family under the same partition $P$. Segment by segment, the output equals the reference definition, which restricts $\operatorname{glue}_P(S)$ whole, gives each synthesized element to the segment that owns its chain, numbers it there, and splits the result by owner.

### Outside the domain

A plan with an expression is declined before any segment is read with the residual `Global { op: Restrict, reason: ExpressionDemand { vertex } }` at the least such source vertex; whole-state restriction evaluates such an expression with the whole instance in scope, so what it reads is not bounded by a segment. An invalid family returns the refusal of `verify_partition`. A family with an edge allocation range at or below an edge identifier in use returns `AllocationBelowFloor` for the least such segment. When whole-state restriction refuses $\operatorname{glue}_P(S)$, the segmented run returns the same refusal, tagged with the segment owning the element it is about; the exception is the stage 5 exhaustion of synthesized identifiers, which the segmented run replaces by allocation within each segment's ranges. A segment that must allocate and declares no range of that kind returns `NoAllocationRange`, and one whose range runs out returns `AllocationExhausted`, in the least failing segment, nodes before edges.

### Definitions used

The *chain* of a synthesized element is the path that replaces one contracted pair or one expanded edge, and its *owner* is the segment owning the pair's first node or the edge's source. The *fate* of a node is its stage 0 decision: a fan node, a dropped plain node, or a surviving plain node with its source anchor, view anchor and recorded original anchor. An *entry* of a segment is a dropped plain node it owns that a non-fan in-stub enters. Its *reach summary* lists the surviving nodes that a search from the entry reaches through dropped plain nodes of the segment followed by one more edge, and the dropped ghosts at which the search leaves the segment.

### Proof

1. Stage 0. Without expressions the engine decides a plain node from its own anchor and payload. Each node is owned by one segment (L-segments-1), so each segment decides exactly its own nodes, as whole-state restriction would.
2. Stage 1 and the exchange. The engine decides an edge between surviving nodes from its label and the view anchors of its endpoints, and an edge with a dropped endpoint is dropped. An edge is owned with its source, and when its target is remote the source's segment holds a ghost and the target's segment an in-stub (L-segments-2). Every segment sends the fate of each in-stub target to the owner of the in-stub's source, so every segment knows the fate of each of its ghosts, and stage 1 makes the whole-state decision at each owned edge.
3. Stage 2. For a surviving node $s$, whole-state restriction pairs $s$ with every surviving $t$ reached by a path $s, d_1, \dots, d_k, t$ with $k \ge 1$ and every $d_j$ a dropped plain node, along non-fan edges. Cut such a path at each edge whose endpoints lie in different segments. Each cut enters a dropped node at a non-fan in-stub, which makes it an entry of its owner, and the part of the path inside one segment is a search the summary of that entry records; the last part ends at $t$, which the summary lists as a survivor. Conversely every survivor and every exit a summary lists lies on such a path. The coordinator's closure, in which an entry reaches what it reaches locally together with what every exit reaches, is therefore reachability through dropped nodes, by induction on the number of cuts. This is the partial evaluation and assembly of distributed reachability queries [@fanwangwu2012performance]: the ghosts are its virtual nodes, the entries its in-nodes, and the coordinator solves the possibly recursive system over the boundary. Each segment's search from its own survivors reads its local dropped nodes and, at each dropped ghost, the closed reach, so it realizes the same pairs from $s$ as the whole-state search. The anchors of every reached survivor come back with the closure.
4. Stage 3 and the fan domain. A fan node's out-edges are owned with it, so a segment decides each fan it owns from its out-edges and the fates of their targets, which step 2 delivers. Whether a node violates the fan domain depends on its shape, its out-edges and its in-edges, and each in-edge is either owned in the node's segment or an in-stub there. Every violation the whole graph has is thus found in the owner of the node, and the least one is the least among the segments' least ones.
5. Refusals. Every element belongs to one segment, and the first whole-state refusal is the one with the least key (stage, then element). The run checks the fan domain in every segment, then stage 0 in every segment, then stages 1 to 3 in every segment, and keeps the least key at each step. Stages 1 to 3 run only after every segment has passed stage 0, so the facts they read are those of a whole-state run, and the refusal returned is the whole-state one.
6. Allocation and assembly. Every element of a chain is synthesized in its owner's segment. Its nodes take identifiers from the owner's node range, which lies inside that segment's nodes, and its edges from the owner's edge range, which lies above every edge identifier in use and is disjoint from the others (L-segments-10). Synthesized identifiers are therefore distinct from every source identifier and from each other, so the view segments are a family under $P$, and the boundary step gives each crossing view edge its ghost and in-stub. Every complement entry is keyed by an element with one owner, so the complement segments partition the whole complement. Assembly renames each synthesized element by its key to the identifier stage 5 of whole-state restriction gives it, in ascending key order from one above the greatest source identifier, and sets the floors to those values. By steps 1 to 5 the assembled view and complement are those of whole-state restriction.
7. Reference agreement. The reference definition computes the right-hand side of the equation directly and numbers each segment's chains by the append-floor rule of L-segments-10 in ascending key order, nodes before edges, which is the production allocation. It splits the view with the checked splitter and the complement by owner, so it gives each segment exactly the production output.
8. The finite certificate counts its carrier independently. A release schema with $e \le 4$ of the six possible edges has $\sum_{j \le \min(3, e)} \binom{e}{j}$ release graphs, giving $1 + 6 \cdot 2 + 15 \cdot 4 + 20 \cdot 8 + 15 \cdot 15 = 458$ schema and graph pairs. Each has $2^3 = 8$ survival classifications without a conditional vertex, and each three-node graph has $2^3 + 3^3 = 35$ labelled two- and three-segment ownership splits, for $458 \cdot 8 \cdot 35 = 128{,}240$ cases. Witnesses cover a hidden path through three segments with a dropped cycle, every split of a fan and an expansion, the allocation refusals and the residual.

### Executable counterpart

- Law ID: `segments.segmented-restrict`
- Reference: `panproto_reference::segment::restrict_graph_segmented`
- Enumerator: `panproto_reference::enumerate::segments::segmented_restrict_cases; bound: graphs=458, migrations=8, partitions=35, cases=128240`
- Exhaustive differential test: `segmented::segmented_restrict_agrees_with_the_reference_and_assembles_to_whole_state_restrict; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented::segmented_restrict_agrees_with_the_reference_and_assembles_to_whole_state_restrict)'`
- Property differential test: `segmented_randomized::segmented_restrict_matches_the_reference_on_generated_partitions; command: cargo nextest run -p panproto-inst --test reference_restrict --no-tests=fail -E 'test(=segmented_randomized::segmented_restrict_matches_the_reference_on_generated_partitions)'`
- Property scope: `graphs of 2 to 8 nodes over a three-vertex schema with every edge and loop, up to 14 edges, every survival choice, owners from 2 to 4 segments; production and reference segments, complements and refusals, and the assembled output against whole-state restriction; 256 cases`
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

Let $\pi$, $S$ and $P$ satisfy the hypotheses of [L-segments-11](#L-segments-11), let $M$ be the manifest sealing $S$ as a source with trusted digest $d$, and let $B$ be a budget. Write $\operatorname{run}_c$ for `graph_restrict_run` of $\pi$ over $M$, reading $S$ from a segment source, with lens identity $\ell$, view schema $\sigma$ and budget $B$, stopped once $c$ tasks are done, and $\operatorname{ref}_c$ for the reference runner `segmented_restrict_with_progress` on the same inputs. A run of $n$ segments has $4n + 2$ tasks. For every $c$ from $0$ to $4n + 2$:

1. the report of $\operatorname{run}_c$ on an empty store equals the report of $\operatorname{ref}_c$ on an empty store: status, locality, budget, progress, peak estimate, complement accounting, tasks run and reused, orphans and per-phase accounting;
2. running again without a cancellation point against the same store gives the report of the reference run resumed the same way, reuses $\min(c, k)$ tasks, where $k$ is the number of tasks the uninterrupted run completes, and returns the status and manifest of the uninterrupted run; the store then holds the uninterrupted run's emitted artifacts and no staged artifact;
3. both runs carry the same run identity, $\operatorname{RunJournal::run\_digest}(\ell, d, P, \texttt{graph restrict})$.

When the uninterrupted run completes, the view segment of segment $i$ is the view segment the reference definition of L-segments-11 gives segment $i$, written under $\sigma$, the complement bytes of segment $i$ are the canonical complement-segment bytes of its complement segment in the reference encoding, and the manifest seals the view segments as a view of $d$ under the lens envelope of $\ell$.

### Outside the domain

A plan with an expression is declined with `Residual::Global` before any task. A manifest that fails verification against $d$ is refused before any task and the run has no identity. A partition with more segments than the budget's affected segments, or a record whose working-set estimate exceeds the budget, is declined before any task. A restrict refusal is returned at Assemble, after the $3n$ segment tasks of the first three phases: the refusal of L-segments-11, located in the same segment. The coordinator's bytes (the summaries and its own artifact) above the summary budget decline the run at Assemble; a running complement total above the complement budget declines it before the Emit task of the segment that exceeds it. A store failure stops the run as incomplete and reports each staged artifact the journal does not name as an orphan. Seal refuses no run: it applies the seal floor rule of [L-segments-9](#L-segments-9) to the view segments under the source's partition, and those view segments always seal (step 6 of the proof).

### Definitions used

The phases are those of the storage vocabulary ([L-segments-5](#L-segments-5)). Validate checks one input segment against $M$ and keeps its canonical bytes; Export writes the stage 0 fate of every in-stub target for the segment owning the in-stub's source, or the position of the segment's refusal; Summarize writes the segment's reach summary; Assemble closes the summaries, runs every segment's later stages and allocation one segment at a time, and writes the closure and the view boundaries; Emit writes one view segment and one complement segment; Seal writes the output manifest. The *coordinator's bytes* are the summed lengths of the Summarize artifacts and of the Assemble artifact. A *working-set estimate* is the `MemoryModel` estimate of a record's statistics.

### Proof

1. Task order and accounting. Both runners execute the same $4n + 2$ tasks in the same order, test the cancellation point before each task, count a task when it is committed or reused, and close a phase's accounting only when all its tasks are done. The preflight checks before the first task are the same tests in the same order, so a run declined there has the same report on both sides.
2. Identity and reuse. A task's artifact is a deterministic function of the plan, the input manifest, the segment it names and the artifacts of earlier phases, and its input digest binds the lens identity, its key and those artifacts' digests. The store commits an artifact and its journal record in one step. A run cancelled after $c$ tasks has committed exactly the first $\min(c, k)$ tasks of the uninterrupted run, with the uninterrupted run's artifacts; the resumed run reuses each of them, since its inputs and artifact are unchanged, and recomputes the rest, which by determinism writes the uninterrupted run's artifacts. Its status and manifest are therefore those of the uninterrupted run, and since every staged artifact is committed in the same step, none is left staged.
3. Outputs. Export, Summarize and Assemble compute the fan domain, stage 0, the exchange, the reach summaries and their closure, and stages 1 to 3 with allocation, each from one segment and its boundary facts, exactly as `graph_restrict_segmented` does; Emit recomputes stages 1 to 3 for one segment from the same facts. By L-segments-11 each emitted view and complement segment is the reference definition's. The boundary of each view segment comes from the Assemble artifact: every view edge whose target lies in another segment is recorded as an in-stub of the target's owner and a ghost of its holder, and the ghost's anchor is offered by the owner, because such a target is either the target of an in-stub of the source graph or a survivor that the owner's own reach summary lists (the last edge of a contracted path enters the target's segment either at the target or at an entry from which a search in that segment reaches the target).
4. Refusals. The fan domain and stage 0 are decided segment by segment in Export, and Assemble returns the least refusal among them (the least fan node first, then the least key), recomputed in its segment; it then returns the least refusal of stages 1 to 3, and failing that the allocation refusal of the least failing segment. This is the precedence of `graph_restrict_segmented`, and the reference returns the refusal of the reference definition, equal to it by L-segments-11, after the same $3n$ tasks.
5. Budgets. A record's statistics are those of its segment (L-segments-4), so the working-set estimates agree. The reference defines the reach summary of a segment and the closure directly on the glued graph and the stage 0 fates, and writes them in the same canonical layout; by step 3 of the proof of L-segments-11 they are the production summaries and closure, and the boundaries are those of the equal view segments, so the coordinator's bytes agree. The complement bytes are the canonical complement-segment bytes on both sides, so the running totals agree.
6. Sealing. Both runners seal the same view segments under $\sigma$ as a view of $d$ with the same lens envelope and the input's binding, so the manifests and their digests agree. The view segments seal under the source's partition. A view edge that is not synthesized keeps its source identifier, which lies below every edge allocation range by the hypothesis of L-segments-11. A synthesized edge takes its identifier from the edge range of its chain's owner, and the owner holds it: its source is the first node of a contracted pair or the source of an expanded edge, which the owner owns by definition, or a node synthesized along the chain, which takes its identifier from the owner's node range and so lies among the owner's nodes. Every view identifier at or above the floor therefore lies in the range of the segment holding it.
7. The finite certificate takes every 97th case of the carrier of L-segments-11, $\lceil 128{,}240 / 97 \rceil = 1{,}323$ cases, and checks every cancellation point of each; witnesses cover each budget refusal, a store whose commit fails at each task, the residual of an expression, and a contracting and an expanding run that seal, verify and resume from every cancellation point.

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
- File: `crates/panproto-inst/src/segment/runner.rs`
- File: `crates/panproto-inst/src/segment/restrict.rs`
- File: `crates/panproto-inst/tests/reference_restrict.rs`
- File: `crates/panproto-reference/src/pages/segments.rs`
- File: `crates/panproto-reference/src/pages/restrict_and_contraction.rs`
