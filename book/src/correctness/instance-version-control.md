# Instance version control

Schema version control stores instances of all three shapes (trees, tables and graphs) next to the schemas they conform to, together with the complements a data migration leaves and the edit logs made against them. This page states how those objects are stored and kept: a stored payload decodes to the value it was made from and, for tables and graphs, names that value uniquely; version 1 of the repository format holds every shape without changing anything a repository written before them holds; and garbage collection keeps exactly what a root reaches. The behaviour is explained in [Schema version control semantics](../explanation/vcs-semantics.md).

The executable reference is [`panproto_reference::pages::instance_version_control`](https://docs.rs/panproto-reference/latest/panproto_reference/pages/instance_version_control/). It models a repository as named objects whose reference fields name other objects, and computes reachability by a breadth-first search from the roots; it models a payload by the facts that decide what decoding it gives. It shares nothing with production, which follows references by object kind and decodes MessagePack.

## Definitions shared by the laws

A **data set**, a **complement** and an **edit log** each hold a **payload**, a byte string, and record its **encoding**: a shape (tree, table or graph) and a version. Version 1 of each shape's encoding is MessagePack, with structs as positional arrays, of the value below.

| Object | Tree | Table | Graph |
|---|---|---|---|
| Data set | `Vec<WInstance>` | `FInstance` | `GInstance` |
| Complement | `Vec<Complement>` | `TableComplement` | `GraphComplement` |
| Edit log | `Vec<TreeEdit>` | `Vec<TableDelta>` | `Vec<GraphDelta>` |

A data set records a **record count** (trees, rows or nodes) and an edit log an **edit count**. Bytes **decode whole** when they are exactly one value of the payload type, with no byte left over. They are **canonical** when they are the bytes encoding that value gives. A data set's **identifier** is the BLAKE3 digest of the ordered map of its schema identifier, payload, record count and key, with the encoding added exactly when it is not version 1 of the tree encoding; complements and edit logs are addressed the same way over their own fields.

A repository's **roots** are the commit `HEAD` resolves to, the target of every ref whose name begins with `refs/`, every entry of the reflog of `refs/stash`, and, for `Repository::gc`, every object the staging index names. The **references** of an object are the identifiers its fields hold: a commit's schema, migration, parents, protocol, data sets, complements, edit logs, theories and CST complements; a migration's source and target; a schema tree's entries; a tag's target; a data set's schema; a complement's migration and data set; an edit log's schema, data set and final complement; a CST complement's data set. Protocols, expressions, theories, theory morphisms, file schemas and flat schemas have none. An object is **reachable** when a path of references leads to it from a root.

## A stored payload decodes to its value and names it {#L-instance-version-control-1}

### Statement

Let $x$ be a sequence of trees, a table instance or a graph instance. `DataSetObject::from_instance(s, x, k)` records the encoding of $x$'s shape at version 1, the record count of $x$, and a payload; stored and read back, its `decode` returns an instance equal to $x$: the same trees with every node, arc, fan and root, the same rows with their tables, cells and foreign-key pairs, or the same nodes and edges under the same ids. For a table or graph instance the payload depends only on $x$, not on the order its elements were inserted in, so two equal instances give one identifier for a given schema and key. The same holds for `ComplementObject::from_payload` and `EditLogObject::from_payload` with their values.

Decoding any data set, complement or edit log gives `payload_outcome` of its facts: a version other than 1 is unsupported; otherwise bytes that do not decode whole are undecodable; otherwise a table or graph payload that is not canonical is non-canonical; otherwise a count other than the value's is undecodable; otherwise the payload decodes.

### Outside the domain

An encoding version other than 1 is refused with `UnsupportedPayloadEncoding`, before the bytes are read. Bytes that do not decode whole, a record or edit count that contradicts the value, and a tree complement whose identity envelope names another format are refused with `UndecodablePayload`; so is a tree data set whose payload is source JSON, the form tree data was first stored in, which `Repository::decoded_data_at` reads through its schema. A table or graph payload that decodes but is not canonical is refused with `NonCanonicalPayload`. A tree payload is not checked for canonicity: its identity is its bytes as written, and two byte strings decoding to one sequence of trees are two objects.

### Definitions used

Payload, encoding, decoding whole, canonical, record count, edit count and identifier are defined above. Two instances are equal as stated: trees node for node with their arcs and fans in order, tables and graphs element for element under their ids.

### Proof

1. Encoding writes the value with `rmp_serde::to_vec` and records the shape's current encoding and the value's count. Decoding first compares the recorded version with 1, then reads one value from a cursor over the payload and refuses when the cursor has not reached the end, then (for a table or graph) re-encodes the value and refuses when the bytes differ, then compares the count. These are the four checks of `payload_outcome`, in its order.
2. The decoders of `WInstance`, `FInstance` and `GInstance` read back what their encoders write: a tree's nodes, arcs, fans and root; a table's rows by id with their tables and cells, and its foreign keys; a graph's nodes and edges by id. So a payload made by `from_instance` decodes whole, its count agrees, and the decoded value equals the encoded one.
3. `FInstance` and `GInstance` serialize their maps in ascending key order; a table complement, a graph complement, a table delta and a graph delta hold only ordered maps and sets; and the cells of a row and the fields and annotations of a node are written in ascending key order. Their encodings are therefore functions of their values, which is what makes the payload made in step 2 canonical and independent of insertion order. By the definition of the identifier, a data set's identifier is then a function of its schema, its value and its key.
4. The certificates: every graph, table and tree of the edit algebra's coverage scope (61,066 graphs, 32,038 tables and 3,156 trees over the sixteen schemas of [Edits](edits.md)) is stored and read back and compared with the reference model, tables and graphs inserted in both ascending and descending order and required to give one identifier, and the 96,260 identifiers are distinct; the graph and table delta of every pair of consecutive states (61,050 and 32,022) is stored as an edit log, read back and applied; a graph, table and tree complement over every state is stored and read back. Every payload of each shape and object kind is decoded intact and after each of seven damages (two versions, a trailing byte, a truncation, an empty payload, another shape, and a non-minimal array header that keeps the value), with and without an agreeing count, and the outcome is compared with `payload_outcome`: 120 decodes, 12 decoding, 30 unsupported, 68 undecodable and 10 non-canonical. A graph payload with its node map in descending order is refused as non-canonical, and the property test stores random graphs inserted in both orders.

### Executable counterpart

- Law ID: `instance-version-control.payload-encodings`
- Reference: `panproto_reference::pages::instance_version_control::payload_outcome`
- Enumerator: `panproto_reference::enumerate::edits::coverage_graphs; bound: graphs=61066`
- Exhaustive differential test: `every_instance_of_the_coverage_scope_round_trips_through_its_stored_object; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=every_instance_of_the_coverage_scope_round_trips_through_its_stored_object)'`
- Property differential test: `randomized_graph_payloads_round_trip_in_any_insertion_order; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=randomized_graph_payloads_round_trip_in_any_insertion_order)'`
- Exhaustive differential test: `every_edit_log_of_consecutive_coverage_states_round_trips_and_applies; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=every_edit_log_of_consecutive_coverage_states_round_trips_and_applies)'`
- Exhaustive differential test: `every_complement_of_the_coverage_states_round_trips; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=every_complement_of_the_coverage_states_round_trips)'`
- Exhaustive differential test: `every_damaged_payload_gives_the_outcome_the_reference_gives; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=every_damaged_payload_gives_the_outcome_the_reference_gives)'`
- Production certificate: `a_table_or_graph_payload_out_of_canonical_order_is_refused; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=a_table_or_graph_payload_out_of_canonical_order_is_refused)'`
- Reference test: `every_combination_of_payload_facts_is_decided_in_the_stated_order; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=every_combination_of_payload_facts_is_decided_in_the_stated_order)'`
- File: `crates/panproto-vcs/src/payload.rs`
- File: `crates/panproto-vcs/src/object.rs`
- File: `crates/panproto-vcs/src/hash.rs`

## A version 1 repository holds every shape without changing what it held {#L-instance-version-control-2}

### Statement

The repository format version is 1 ([L-schema-merge-4](schema-merge.md#L-schema-merge-4)). A data set, complement or edit log writes its encoding as a trailing positional field exactly when `writes_encoding_field` holds of it, that is, when the encoding is not version 1 of the tree encoding; an object read without the field holds `recorded_encoding(None)`, version 1 of the tree encoding. Consequently every object of a repository written before encodings were recorded reads back under the identifier it is filed under, the bytes this build writes for it are the bytes stored, and its data sets, complements and edit logs decode as trees; such a repository accepts table and graph data sets, and a commit naming them, with its format marker still reading `1`.

### Outside the domain

A build that predates payload encodings reads a table or graph object as a positional array one field longer than its struct and fails to decode it; every other object it reads as before. Opening a repository with another format marker is refused as [L-schema-merge-4](schema-merge.md#L-schema-merge-4) states.

### Definitions used

The encoding field, positional arrays and identifiers are defined above. A repository written before encodings were recorded is one written by panproto 0.78; the checked-in repository `crates/panproto-vcs/tests/fixtures/repository-0.78` was written by that release's tree data path (staging, committing, branching, merging, forward and backward migration, an edit log and a tag) and is copied before each test, so no test writes to it.

### Proof

1. The field is declared last with a default of version 1 of the tree encoding and is skipped on writing when it equals that default. A positional array omits only trailing fields, so an object written without it has exactly the fields, in the order, that an object written before the field existed has, and reading such an array fills the field with the default. This is `writes_encoding_field` and `recorded_encoding`.
2. The identifier of a data set, complement or edit log is computed over an ordered map that holds the encoding only when it is not the default (definition of the identifier), so a tree object has the identifier it had before the field existed. The store verifies on every read that the stored bytes give the identifier they are filed under ([L-schema-merge-4](schema-merge.md#L-schema-merge-4)), and that check passes for every object of the fixture.
3. No other object kind changed, and the format marker is written only by `init` and by the first write to a repository without one, so adding table and graph objects leaves the marker as it is.
4. The certificates: for each of the three shapes and versions 0, 1 and 2, a data set, a complement and an edit log are written and their field count compared with `writes_encoding_field`, and read back and compared with `recorded_encoding`. Every one of the fixture's 35 objects (6 commits, 5 schema trees, 5 file schemas, 5 flat schemas, 4 migrations, 5 theories, 3 data sets, 1 complement and 1 edit log) reads back, each data set, complement and edit log decodes as trees and is written back to its stored bytes, the five commits on `main` are read, and the committed records read through their schema are those the 0.78 repository staged; then a table and a graph data set are committed into the fixture and read back after reopening it, with the marker unchanged.

### Executable counterpart

- Law ID: `instance-version-control.format-one`
- Reference: `panproto_reference::pages::instance_version_control::recorded_encoding`
- Reference: `panproto_reference::pages::instance_version_control::writes_encoding_field`
- Enumerator: `panproto_reference::enumerate::instance_version_control::encodings; bound: versions=3`
- Exhaustive differential test: `an_object_writes_its_encoding_exactly_when_the_reference_says_so; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=an_object_writes_its_encoding_exactly_when_the_reference_says_so)'`
- Production certificate: `a_repository_written_by_0_78_opens_and_reads_back_unchanged; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=a_repository_written_by_0_78_opens_and_reads_back_unchanged)'`
- Production certificate: `a_repository_written_by_0_78_takes_table_and_graph_objects_in_format_1; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=a_repository_written_by_0_78_takes_table_and_graph_objects_in_format_1)'`
- Production certificate: `a_tree_object_records_no_encoding_and_keeps_its_identifier; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=a_tree_object_records_no_encoding_and_keeps_its_identifier)'`
- File: `crates/panproto-vcs/src/object.rs`
- File: `crates/panproto-vcs/src/hash.rs`
- File: `crates/panproto-vcs/src/fs_store.rs`

## Garbage collection keeps exactly what a root reaches {#L-instance-version-control-3}

### Statement

For a repository whose stored objects can all be read, garbage collection (`gc`, `gc_with_roots` with the staged objects as extra roots, and `Repository::gc`) deletes exactly the stored objects that are not reachable, reports them in ascending order of identifier, and keeps every reachable one. In particular it never removes an object reachable from a ref. A dry run reports the same objects and deletes none.

### Outside the domain

A stored object that cannot be read while marking (its bytes do not hash to its identifier, or do not decode as an object) fails the collection with that error before anything is deleted; payloads are not decoded while marking. An identifier that names no stored object is not followed. Reflogs other than that of `refs/stash` are not roots, so a commit named only by the reflog of `HEAD` or of a branch is collected.

### Definitions used

Roots, references and reachability are defined above; `Object::links` is the reference's list of references and `Repository::reachable` its breadth-first search.

### Proof

1. `collect_roots` lists `HEAD`'s commit, every ref under `refs/` and every entry of the stash's reflog; `gc_with_roots` adds the caller's roots, and `Repository::gc` passes the identifiers the staging index names. These are the roots.
2. `references` returns, for each object kind, the fields listed in the definition, and `mark_reachable` pushes the roots and then the references of every stored object it pops, skipping an identifier already marked; the marked set is the set of identifiers a path of references leads to from a root. It reads every reachable stored object, and an error from a read returns before the sweep.
3. The sweep lists every stored object and deletes those not marked, after the mark is complete, so a deletion never removes an object a later step of the mark would reach. The marked stored objects are the reachable ones (step 2), so the deleted objects are exactly the unreachable ones; the report is sorted before it is returned, and a dry run returns it without deleting.
4. The certificates: the scope holds, for each of nine root configurations (none; `main` at the first or second commit; `HEAD` detached at the first; an annotated tag of the second; a remote-tracking ref at the first; a ref in another namespace at the second; the stash at the first with the second in its reflog; a staged graph data set alone), each of the 64 choices of how much of each shape's data set, complement and edit log each of two commits records, over a store holding 23 objects including orphan data sets and edit logs of each shape: 36,864 repositories. On each, production's deleted and kept objects equal the reference's garbage and reachable set, and the kept objects total 359,936, the count derived in closed form in `panproto_reference::enumerate::instance_version_control::GC_KEPT_TOTAL` without enumerating repositories (41,472 for each root configuration that reaches only the first commit, 54,784 for each that reaches the second, 58,880 with the tag and 12,288 for the staged data set). One repository per root configuration is collected on a filesystem store, the property test collects random repositories of up to five commits with random parents, refs in six namespaces, a detached `HEAD`, stash reflog entries and staged objects, and the 0.78 fixture's collection deletes exactly the one object no ref reaches (the data set the backward migration restored) and keeps the other 34. A repository keeps a staged data set through collection and commits it afterwards.

### Executable counterpart

- Law ID: `instance-version-control.reachability`
- Reference: `panproto_reference::pages::instance_version_control::Repository::roots`
- Reference: `panproto_reference::pages::instance_version_control::Object::links`
- Reference: `panproto_reference::pages::instance_version_control::Repository::reachable`
- Reference: `panproto_reference::pages::instance_version_control::Repository::garbage`
- Enumerator: `panproto_reference::enumerate::instance_version_control::gc_cases; bound: commits=2`
- Exhaustive differential test: `garbage_collection_keeps_exactly_what_the_reference_reaches_on_every_repository_of_the_scope; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=garbage_collection_keeps_exactly_what_the_reference_reaches_on_every_repository_of_the_scope)'`
- Property differential test: `randomized_repositories_keep_exactly_what_the_reference_reaches; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=randomized_repositories_keep_exactly_what_the_reference_reaches)'`
- Production certificate: `garbage_collection_on_disk_keeps_what_the_reference_reaches_under_every_root; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=garbage_collection_on_disk_keeps_what_the_reference_reaches_under_every_root)'`
- Production certificate: `garbage_collection_of_a_0_78_repository_deletes_only_what_no_ref_reaches; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=garbage_collection_of_a_0_78_repository_deletes_only_what_no_ref_reaches)'`
- Production certificate: `a_repository_keeps_staged_data_through_garbage_collection; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=a_repository_keeps_staged_data_through_garbage_collection)'`
- Reference test: `reachability_on_hand_computed_repositories; command: cargo nextest run -p panproto-reference --test instance_version_control --no-tests=fail -E 'test(=reachability_on_hand_computed_repositories)'`
- Reference test: `the_gc_scope_has_its_closed_form_size_and_kept_total; command: cargo nextest run -p panproto-reference --test instance_version_control --no-tests=fail -E 'test(=the_gc_scope_has_its_closed_form_size_and_kept_total)'`
- File: `crates/panproto-vcs/src/gc.rs`
- File: `crates/panproto-vcs/src/index.rs`
- File: `crates/panproto-vcs/src/repo.rs`
