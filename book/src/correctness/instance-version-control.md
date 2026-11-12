# Instance version control

Schema version control stores instances of all three shapes (trees, tables and graphs) next to the schemas they conform to, together with the complements a data migration leaves and the edit logs made against them. This page states how those objects are stored, kept and compared: a stored payload decodes to the value it was made from and, for tables and graphs, names that value uniquely; version 1 of the repository format holds every shape without changing anything a repository written before them holds; garbage collection keeps exactly what a root reaches; a commit tracks the nearest instance under each key, validated against the schema it is bound to, until a commit stops tracking the key; the diff of two committed instances is their shape's delta; a merge merges the instances two commits track by their deltas from the merge base, reporting every overlap as a typed conflict; and rebase and cherry-pick replay a commit's deltas rebased over those committed since. The behaviour is explained in [Schema version control semantics](../explanation/vcs-semantics.md).

The executable reference is [`panproto_reference::pages::instance_version_control`](https://docs.rs/panproto-reference/latest/panproto_reference/pages/instance_version_control/). It models a repository as named objects whose reference fields name other objects, and computes reachability by a breadth-first search from the roots; it models a payload by the facts that decide what decoding it gives, and the instances a commit tracks as entries whose deltas are those of the reference edit algebra. It shares nothing with production, which follows references by object kind and decodes MessagePack.

## Definitions shared by the laws

A **data set**, a **complement** and an **edit log** each hold a **payload**, a byte string, and record its **encoding**: a shape (tree, table or graph) and a version. Version 1 of each shape's encoding is MessagePack, with structs as positional arrays, of the value below.

| Object | Tree | Table | Graph |
|---|---|---|---|
| Data set | `Vec<WInstance>` | `FInstance` | `GInstance` |
| Complement | `Vec<Complement>` | `TableComplement` | `GraphComplement` |
| Edit log | `Vec<TreeEdit>` | `Vec<TableDelta>` | `Vec<GraphDelta>` |

A data set records a **record count** (trees, rows or nodes) and an edit log an **edit count**. Bytes **decode whole** when they are exactly one value of the payload type, with no byte left over. They are **canonical** when they are the bytes encoding that value gives. A data set's **identifier** is the BLAKE3 digest of the ordered map of its schema identifier, payload, record count and key, with the encoding added exactly when it is not version 1 of the tree encoding; complements and edit logs are addressed the same way over their own fields.

A repository's **roots** are the commit `HEAD` resolves to, the target of every ref whose name begins with `refs/`, every entry of the reflog of `refs/stash`, and, for `Repository::gc`, every object the staging index names. The **references** of an object are the identifiers its fields hold (the keys a commit stops tracking are names, not identifiers, and are not references): a commit's schema, migration, parents, protocol, data sets, complements, edit logs, theories and CST complements; a migration's source and target; a schema tree's entries; a tag's target; a data set's schema; a complement's migration and data set; an edit log's schema, data set and final complement; a CST complement's data set. Protocols, expressions, theories, theory morphisms, file schemas and flat schemas have none. An object is **reachable** when a path of references leads to it from a root.

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
3. A commit gains one field, `untracked`, by the same rule: it is declared last, defaults to empty, and is written only when nonempty, so a commit that stops tracking no key has the bytes and identifier it had before the field existed. No other object kind changed, and the format marker is written only by `init` and by the first write to a repository without one, so adding table and graph objects leaves the marker as it is.
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

## A commit tracks the nearest instance under each key, validated against its schema {#L-instance-version-control-4}

### Statement

A commit records the data sets staged for it and, in its `untracked` field, the keys it stops tracking. The instance a commit tracks under a key, `instance_vc::tracked`, is decided by the nearest commit of its first-parent chain, the commit itself first, that records a data set under the key or stops tracking it: the data set in the first case, and no instance in the second. This is the reference's `tracked` of the chain's records. `Repository::remove_instance(k)` unstages any data set staged under $k$ and, when `HEAD` tracks $k$, stages its end, which the next commit records. `Repository::committed_instance(c, k)` returns that data set's instance, its schema and whether it is **stale**, that is, whether its schema is not the commit's.

`Repository::add_instance(k, x, options)` binds $x$ to the schema it is staged against (`AddDataOptions::schema_id`, else the staged schema, else HEAD's) by recording that schema's identifier. It validates each tree of a tree instance, or the table or graph, against that schema under its resolved protocol, and refuses $x$ when the report of some record has a required check that failed or did not complete. Otherwise it stages the data set under $k$, replacing any data set staged under $k$, and the next commit records it. With `skip_verify` it stages $x$ without validation as pending, and a default commit refuses it.

### Outside the domain

A commit of the chain recording two data sets under one key, or a data set under a key it stops tracking, is refused with `DuplicateDataKey` naming the key. `remove_instance` of a key that `HEAD` does not track and nothing stages is refused with `NotTracked`. An instance failing validation is refused with `InstanceValidationFailed`, carrying the key and the index and `ValidationReport` of every failing record, and nothing is staged. A schema whose protocol does not resolve is refused with `UnknownProtocol`; with no schema staged and no HEAD, `NothingStaged`. A key the commit does not track is refused with `RefNotFound`. A pending stage is refused at commit with `ValidationPending`. A key once tracked stays tracked along the first-parent chain until a commit records another data set under it or stops tracking it.

### Definitions used

Tracking, staleness and the failing records of a report are defined in the statement; the report is that of `validate_wtype`, `validate_table` or `validate_graph` ([Validation](validation.md)).

### Proof

1. `tracked` walks the first-parent chain from the commit, reads the data sets and untracked keys each commit records, refuses a key recorded twice by one commit, and decides a key (as a data set or as untracked) only when no nearer commit has decided it; it returns the keys decided as data sets. This is the reference's first record along the chain. The `untracked` field is written last and only when nonempty, so a commit stopping nothing has the bytes and identifier it had before the field existed ([L-instance-version-control-2](#L-instance-version-control-2)).
2. `committed_instance` reads that map, decodes the payload ([L-instance-version-control-1](#L-instance-version-control-1)) and compares the data set's schema identifier with the commit's.
3. `add_instance` selects the schema in the stated order, validates each record with the shape's validator and collects the records with a failing required check, writes the data set only when there are none, and removes any staged entry under the key before adding its own. A commit copies the staged data set identifiers into the commit and refuses a pending stage.
4. The certificates: every chain of one to three commits, each leaving each of two keys alone, recording a data set under it or stopping tracking it (9 choices per commit: 819 chains, 2,358 commits), and random chains of up to six commits doing the same over four keys, track what the reference tracks; a tree, a table and a graph anchored outside the schema are each refused with one failing report of status `Failed`, stage nothing, and when staged pending are refused at commit; and a schema change makes a tracked instance stale until it is staged again.

### Executable counterpart

- Law ID: `instance-version-control.tracking`
- Reference: `panproto_reference::pages::instance_version_control::tracked`
- Enumerator: `panproto_reference::enumerate::instance_version_control::tracking_chains; bound: commits=3`
- Exhaustive differential test: `every_commit_chain_of_the_tracking_scope_tracks_what_the_reference_tracks; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=every_commit_chain_of_the_tracking_scope_tracks_what_the_reference_tracks)'`
- Property differential test: `randomized_commit_chains_track_what_the_reference_tracks; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=randomized_commit_chains_track_what_the_reference_tracks)'`
- Production certificate: `a_commit_recording_two_data_sets_under_one_key_is_refused_when_tracked; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=a_commit_recording_two_data_sets_under_one_key_is_refused_when_tracked)'`
- Production certificate: `an_instance_failing_its_shapes_validation_is_refused_with_its_report; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=an_instance_failing_its_shapes_validation_is_refused_with_its_report)'`
- File: `crates/panproto-vcs/src/instance_vc.rs`
- File: `crates/panproto-vcs/src/repo.rs`

## The diff of two committed instances is their shape's delta {#L-instance-version-control-5}

### Statement

For two commits, `Repository::diff_data(a, b)` lists, in key order, every key tracked by one and not the other (`Added`, `Removed`) and every key tracked by both under different data sets (`Changed`), with the delta of their instances, `instance_vc::diff`: the table differ `TableDelta::diff` for tables, the graph differ `GraphDelta::diff` for graphs, and for sequences of trees the graph differ of the graph forms of each record both hold together with the records only one holds. These are the reference's `diff_tracked` and `diff_instances`. `instance_vc::apply` applies such a delta, and applying the delta of $x$ and $y$ to $x$ gives $y$ (trees compared with their arcs listed by child), as the reference's `apply_delta` does.

### Outside the domain

For the first key in order whose two data sets differ: two schemas are refused with `SchemaMismatch` naming the key and both schemas (the instance is migrated first), and two shapes with `ShapeMismatch`. A tree without a graph form, and a delta whose checked reads do not hold of the instance, are refused with `DeltaDoesNotApply`; a tree delta applies only to a sequence holding exactly the records it covers, whose suffix is the records it removes. A key recorded twice by one commit is refused as in [L-instance-version-control-4](#L-instance-version-control-4).

### Definitions used

The graph form of a tree is that of [L-representation-1](representation.md#L-representation-1): arc $i$ is edge $i$. The graph and table differs and application are those of [Edits](edits.md).

### Proof

1. `diff_data` reads both tracked maps ([L-instance-version-control-4](#L-instance-version-control-4)), reports a key of the first map alone as removed and of the second alone as added, skips a key with one data set, and otherwise refuses two schemas before decoding and diffing the two instances; the list is sorted by key.
2. For tables and graphs `diff` is the edit algebra's differ, whose result applies to the first state and gives the second ([Edits](edits.md)). For trees, each common record's graph form takes the graph differ, which gives the second record's graph form; reading it back as a tree gives the second record ([L-representation-1](representation.md#L-representation-1)) with its arcs in the read order, so the trees agree once arcs are listed by child. The removed records are checked against the sequence's suffix and the appended records are added after the common ones, which gives the second sequence.
3. The certificates: over each of the sixteen coverage schemas, every graph and table with at most two nodes or rows and every tree with at most two nodes is stored and read back, and for every ordered pair of one shape (1,303,058 pairs over 5,270 states: 3,178 graphs and 1,852 tables, the counts the rebase scope of [L-edits-11](edits.md#L-edits-11) states, and 240 trees) the delta and its application are compared with the reference's and the application with the second state; a pair of each two shapes is refused with `ShapeMismatch`. Through a repository, every seventeenth pair of consecutive instances of each shape is staged (without validation, since the coverage states carry a field the schema does not declare) and committed under one key and `diff_data` returns the reference's delta; added and removed keys, a key changing shape and a key whose data sets conform to two schemas are refused or reported as stated.

### Executable counterpart

- Law ID: `instance-version-control.diff`
- Reference: `panproto_reference::pages::instance_version_control::diff_instances`
- Reference: `panproto_reference::pages::instance_version_control::apply_delta`
- Reference: `panproto_reference::pages::instance_version_control::diff_tracked`
- Reference: `panproto_reference::pages::instance_version_control::normalized`
- Enumerator: `panproto_reference::enumerate::instance_version_control::small_instances; bound: elements=2`
- Exhaustive differential test: `every_pair_of_small_coverage_instances_diffs_and_applies_as_the_reference_does; command: cargo nextest run --cargo-profile ci-test -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=every_pair_of_small_coverage_instances_diffs_and_applies_as_the_reference_does)'`
- Production certificate: `committed_instances_of_every_shape_diff_by_their_delta_through_a_repository; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=committed_instances_of_every_shape_diff_by_their_delta_through_a_repository)'`
- Production certificate: `diffing_across_keys_reports_added_and_removed_and_refuses_mixed_schemas_and_shapes; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=diffing_across_keys_reports_added_and_removed_and_refuses_mixed_schemas_and_shapes)'`
- File: `crates/panproto-vcs/src/instance_vc.rs`
- File: `crates/panproto-vcs/src/repo.rs`

## A merge merges tracked instances by their deltas and reports every overlap {#L-instance-version-control-6}

### Statement

Let $b$, $o$ and $t$ be the tracked maps ([L-instance-version-control-4](#L-instance-version-control-4)) of a merge base and of our and their commits. `instance_merge::merge_tracked(b, o, t)` decides each key $k$ tracked by any of the three, in ascending order, as the reference's `merge_tracked` does, comparing data sets by identifier:

1. when $o(k)$ and $t(k)$ are the same data set or both absent, or $t(k)$ is $b(k)$, the key takes $o(k)$; when $o(k)$ is $b(k)$, it takes $t(k)$ (an absent entry leaves the key untracked);
2. otherwise, when $b(k)$ is absent, the key is the conflict `AddedOnBothSides`; when $o(k)$ or $t(k)$ is absent, `RemovedAndChanged` naming that side; when the three hold different shapes, `ShapeDiffers`; when they conform to different schemas, `SchemaDiffers`;
3. otherwise the three instances merge by `merge_instances`: for graphs and tables, with $d_o$ and $d_t$ the deltas of `instance_vc::diff` from $b(k)$ to $o(k)$ and to $t(k)$, the merged instance is the application of `merge(d_o, d_t)` to $b(k)$ when `independence(d_o, d_t)` is empty, and otherwise one `Edit` conflict per conflict of the independence, in its order. For sequences of trees, record $i$ of $b(k)$ that both sides hold merges in the same way through the graph forms of its three versions, and its merged graph must read back as a tree (`NotATree` otherwise); a record one side removes is removed when the other side's record has the graph form it had in the base, and is `RemovedAndChanged` otherwise; a record both sides remove is `RemovedByBoth`; the records one side appends past the base follow the merged records, and when both sides append, the conflict is `AppendedByBoth` at the base's length;
4. a merged instance is validated under its schema as `Repository::add_instance` validates (production only; the reference does not model validation). Each failing record's required diagnostics of the key checks `KeyUnique` and `ReferenceResolves` are one `KeyConstraint` conflict, and any other required diagnostic or required undecided check is one `Validation` conflict carrying the report.

The conflicts are listed by key, then record (none first), then kind (`Tracking`, `Edit`, `Record`, `KeyConstraint`, `Validation`), then the conflict's own order, and a key with a conflict is not merged. No overlap is resolved by a policy. `apply_resolutions` resolves the conflicts by index with `ChooseOurs` or `ChooseTheirs`, every conflict of one key alike, giving that side's data set for the key, or its absence. `Repository::merge` merges the instances when the schema merge is clean and records the merge commit only when every instance conflict is resolved; the commit records exactly one data set per key it tracks, and stops tracking the keys our commit tracks that the merge does not, so the merge commit tracks exactly the merged map.

### Outside the domain

A tree without a graph form, or a merge of independent deltas that does not apply to the base, is refused with `DeltaDoesNotApply`; three instances of different shapes passed to `merge_instances` directly, with `ShapeMismatch`. Errors of loading a data set's schema or payload are returned as they are. A merged instance whose schema's protocol does not resolve is refused with `UnknownProtocol`. A resolution strategy missing an index is refused with `UnresolvedInstanceConflicts`, and one choosing two sides for one key with `InconsistentInstanceResolution`. Tree data taken from a parent whose schema differs from the merged schema is lifted to the merged schema through `migrate_forward`, with its complement recorded; table and graph data sets are recorded unmigrated, and are then stale.

### Definitions used

Tracked maps, data sets, shapes, schemas and staleness are those of [L-instance-version-control-4](#L-instance-version-control-4); deltas, independence, merge and the graph form of a tree are those of [L-instance-version-control-5](#L-instance-version-control-5) and [L-edits-1](edits.md#L-edits-1). A merge of independent deltas is the patch-theoretic merge, the pushout of two commuting changes over their common source [@mimramdigiusto2013categorical]; overlapping changes, including identical ones, are conflicts, as `merge` defines them.

### Proof

1. `merge_tracked` takes the union of the three key sets in ascending order and applies the cases of the statement in its order; a data set taken whole is compared by identifier, which is a function of its schema, value and key ([L-instance-version-control-1](#L-instance-version-control-1)), so two sides holding the same instance under the same key and schema hold the same data set.
2. For graphs and tables, the deltas are those of the edit algebra's differ, which apply to the base and give each side ([L-instance-version-control-5](#L-instance-version-control-5)). When they are independent, their merge applies to the base and gives the result of applying either after the other ([L-edits-1](edits.md#L-edits-1), and its table counterpart in [L-edits-11](edits.md#L-edits-11)), so the merged instance carries both sides' changes. When they are not, every conflict of the independence is reported, in the order `independence` lists them.
3. For trees the same holds of each record both sides keep, through its graph form; a tree delta removes a suffix, so a record a side removes has no counterpart on that side, and the cases of removal, double removal and appending are exhaustive for a record of the base and the positions past it. Records are visited in ascending order, so the conflicts of a key come out in record order.
4. Validation runs only on a merged instance, since a data set taken whole is one a side committed, and the split of its failures is by check kind. The final sort uses the stated comparison, which is stable on the validation conflicts of one record, so the list does not depend on anything but the merge.
5. `apply_resolutions` counts the indices without a resolution, then records each key's side and refuses a second side, then replaces each conflicted key's entry by that side's data set. `Repository::merge` stores each merged instance as a data set under its key and schema, records one identifier per key of the merged map in key order, and records the keys our commit tracks outside the map as untracked; by [L-instance-version-control-4](#L-instance-version-control-4) the merge commit, whose first parent is ours, then tracks exactly the merged map.
6. The certificates: over the sixteen coverage schemas, every triple of graphs with at most one node (23,542 triples), of tables with at most one row (10,384) and of sequences of at most two one-node trees (883,366), each count the sum of the cubes of the schemas' state counts, merges as the reference merges it, the merged instance or the conflicts in order: 5,218, 2,788 and 118,450 triples merge, and 18,324, 7,596 and 764,916 conflict with 36,204, 11,694 and 1,254,852 conflicts. Every repository of the key scope, two keys each tracked by the base as nothing or a graph and changed by each side by inheriting, stopping tracking or recording one of five data sets (four graphs, a table and a graph of another schema), $98^2$ repositories, merges its tracked maps as the reference does: per key, of $2 \cdot 49$ cases, 7 track nothing, 41 keep a side's data set and 4 merge, and 46 have one conflict (20 added on both sides, 10 removed and changed, 8 shapes, 6 schemas, 2 adding one node on both sides), so 2,704 repositories are clean, with 9,016 conflicts and 8,820 merged entries. The graph and table merges render to one digest in four processes, pinned; random triples of graphs of up to six nodes and eight edges merge as the reference does. Through a repository, an independent graph merge records one data set per key and an untracked key, two dependent graph edits record nothing until both conflicts are resolved for one side, partial and inconsistent resolutions are refused, and a merge that duplicates a unique key or anchors a node outside the schema gives a `KeyConstraint` or a `Validation` conflict with its report.

### Executable counterpart

- Law ID: `instance-version-control.merge`
- Reference: `panproto_reference::pages::instance_version_control::merge_tracked`
- Reference: `panproto_reference::pages::instance_version_control::merge_instances`
- Enumerator: `panproto_reference::enumerate::instance_version_control::merge_states; bound: elements=1`
- Exhaustive differential test: `every_triple_of_the_merge_scope_merges_as_the_reference_merges; command: cargo nextest run --cargo-profile ci-test -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=every_triple_of_the_merge_scope_merges_as_the_reference_merges)'`
- Exhaustive differential test: `every_repository_of_the_key_scope_merges_its_tracked_instances_as_the_reference_does; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=every_repository_of_the_key_scope_merges_its_tracked_instances_as_the_reference_does)'`
- Production certificate: `instance_merges_are_the_same_in_every_process; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=instance_merges_are_the_same_in_every_process)'`
- Property differential test: `randomized_graph_merges_agree_with_the_reference; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=randomized_graph_merges_agree_with_the_reference)'`
- Production certificate: `a_repository_merge_records_one_data_set_per_key_and_tracks_the_merged_instances; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=a_repository_merge_records_one_data_set_per_key_and_tracks_the_merged_instances)'`
- Production certificate: `a_repository_merge_with_instance_conflicts_records_nothing_until_they_are_resolved; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=a_repository_merge_with_instance_conflicts_records_nothing_until_they_are_resolved)'`
- Production certificate: `a_merged_instance_failing_a_key_check_or_validation_is_a_conflict_carrying_its_report; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=a_merged_instance_failing_a_key_check_or_validation_is_a_conflict_carrying_its_report)'`
- Reference test: `instance_merges_on_hand_computed_cases; command: cargo nextest run -p panproto-reference --test instance_version_control --no-tests=fail -E 'test(=instance_merges_on_hand_computed_cases)'`
- File: `crates/panproto-vcs/src/instance_merge.rs`
- File: `crates/panproto-vcs/src/repo.rs`

## Rebase and cherry-pick replay a commit's instance deltas over those committed since {#L-instance-version-control-7}

### Statement

Let $c$ be a commit replayed onto a tip $T$ (by `rebase`, which replays each commit of a chain in order, or by `cherry_pick`, which replays one), with first parent $p$. The keys $c$ changes are those $p$ and $c$ track under different data sets, or one of them only. `instance_merge::replay_tracked(p, T, c)` decides exactly these keys, as `merge_tracked` decides them with $p$ as base, $T$ as ours and $c$ as theirs ([L-instance-version-control-6](#L-instance-version-control-6)), except that two deltas of one graph, table or tree record combine by rebasing: with $d_c$ the delta from $p$'s instance to $c$'s and $d_T$ the delta from $p$'s instance to $T$'s (the composite of the deltas committed since $p$), `rebase_instances` applies `d_c.rebase(&[d_T])` to $T$'s instance, and a refused rebase is that record's one `Edit` conflict, the `RebaseConflict`'s conflict. This is the reference's `rebase_instances`. Consequently:

1. a replay is clean exactly when the three-way merge of $p$, $T$ and $c$ is, and then gives the merge's instance (rebase of independent deltas is their [L-edits-1](edits.md#L-edits-1) merge); a conflicting record's one conflict is the first conflict the merge reports for it;
2. the replayed commit records, for each key $c$ changes, the replayed data set when it differs from the one $T$ tracks, a data set taken from $c$ in its form lifted to the replayed schema, and stops tracking the keys the replay drops that $T$ tracks; so it tracks $T$'s instances with $c$'s rebased deltas applied, and a rebased chain tracks the instances of applying its rebased deltas in order;
3. any conflict refuses the replay with `InstanceConflicts`, carrying every conflict in listing order, and records nothing for that commit.

Cherry-pick is the replay of one commit.

### Outside the domain

As [L-instance-version-control-6](#L-instance-version-control-6), with `InstanceConflicts` in place of a merge's unrecorded conflicts. A replay reads payloads with `DataSetObject::decode`, so a key whose three versions must be combined and one of which is a tree payload of source JSON is refused with `UndecodablePayload`.

### Definitions used

Tracked maps are those of [L-instance-version-control-4](#L-instance-version-control-4); deltas, independence, merge and rebase those of [L-edits-1](edits.md#L-edits-1) and [L-edits-11](edits.md#L-edits-11).

### Proof

1. `replay_tracked` restricts the three tracked maps to the keys $c$ changes and runs the merge's key cases on them; the keys $c$ does not change are inherited from $T$ through the replayed commit's first parent.
2. `rebase` returns the delta unchanged exactly when it is independent of $d_T$, and otherwise the first conflict of their independence ([L-edits-11](edits.md#L-edits-11)). Independent, $d_c$ applies after $d_T$, and the result is that of applying their merge to $p$'s instance, which is the merge's instance; the record cases of trees are shared with the merge. This gives item 1.
3. `replay_commit_data` records the stated data sets and untracked keys; by [L-instance-version-control-4](#L-instance-version-control-4) the new commit then tracks the replay's result on the changed keys and $T$'s instances elsewhere. Each commit of a rebased chain is replayed onto the previous replayed commit, so the chain's instances are the rebased deltas applied in order. This gives items 2 and 3.
4. The certificates: on every triple of the merge scope of [L-instance-version-control-6](#L-instance-version-control-6), production's replay equals the reference's, is clean on exactly the merge's 5,218, 2,788 and 118,450 clean triples with the merge's instance, and otherwise reports the first conflict of each conflicting record: 18,324 and 7,596 conflicts for graphs and tables (one per conflicted triple) and 1,107,276 for trees. Random graph triples agree likewise. Through a repository, a two-commit chain rebased onto a diverged tip tracks its rebased deltas applied in order, which is also the merge of the independent chain, a cherry-pick replays one commit, and a dependent cherry-pick is refused with its conflict.

### Executable counterpart

- Law ID: `instance-version-control.rebase`
- Reference: `panproto_reference::pages::instance_version_control::rebase_instances`
- Enumerator: `panproto_reference::enumerate::instance_version_control::merge_states; bound: elements=1`
- Exhaustive differential test: `every_triple_of_the_merge_scope_rebases_as_the_reference_and_as_the_merge_merges; command: cargo nextest run --cargo-profile ci-test -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=every_triple_of_the_merge_scope_rebases_as_the_reference_and_as_the_merge_merges)'`
- Property differential test: `randomized_graph_rebases_agree_with_the_reference_and_with_the_merge; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=randomized_graph_rebases_agree_with_the_reference_and_with_the_merge)'`
- Production certificate: `a_rebased_chain_tracks_its_rebased_deltas_applied_in_order_and_a_cherry_pick_replays_one_commit; command: cargo nextest run -p panproto-vcs --test reference_instance_version_control --no-tests=fail -E 'test(=a_rebased_chain_tracks_its_rebased_deltas_applied_in_order_and_a_cherry_pick_replays_one_commit)'`
- File: `crates/panproto-vcs/src/instance_merge.rs`
- File: `crates/panproto-vcs/src/rebase.rs`
- File: `crates/panproto-vcs/src/cherry_pick.rs`
