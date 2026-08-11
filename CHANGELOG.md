# Changelog

All notable changes to panproto will be documented in this file.

## [0.69.1] - 2026-08-11

### Fixed

- **A tree edit can no longer make a node its own parent, and an ancestor walk always terminates** (`panproto-inst`): `apply_move_subtree` guards against cycles by asking whether the new parent is a descendant of the node being moved. `is_descendant` walks *up* from its candidate, so it starts at the candidate's parent and never considers the candidate itself; asked whether a node is a descendant of that same node, it answered no on any acyclic instance. The guard passed, the move wrote `parent_map[node] = node`, and the resulting self-loop was not merely wrong but unwalkable: `is_descendant` has no visited set and no bound, so every later ancestor traversal through that node ran forever. A `MoveSubtree` whose `node_id` equals its `new_parent` is now refused with `CycleDetected`, and the walk is bounded by the node count, since a `WInstance` is deserialized and reaches the engine from hosts across the FFI boundary, where a predicate that hangs on malformed input is worse than one that answers wrongly.

  This surfaced as the `edit_laws::action_laws` property tests timing out in CI. They generate `MoveSubtree` edits over the instance's own node ids, so a self-parent move is one of the shapes they explore; the three of them share `arb_scenario`, which is why all three could hang. Nothing about the tests changed. What changed is that 0.69.0 added a runtime bound to the `ci` nextest profile, which turned a run that never finished into a failure, exactly as that bound intended. At 20000 cases the three tests now complete in under ten seconds. Closes #260.

## [0.69.0] - 2026-07-30

### Added

- **View-only reconstruction is reachable from JavaScript** (`panproto-wasm`, `@panproto/core`): 0.68.0 added `put_without_complement` in the Rust core, which reconstructs a source from a stored view when the lens is an isomorphism, but the binding exposed only `putJson`, which needs the complement a prior in-process `getJson` produced. A record read back from storage has no such complement, so the capability added for exactly that case was unreachable from the SDK. `LensHandle.putJsonWithoutComplement(view, rootVertex)` closes that, backed by a new `put_json_without_complement` at the WASM boundary. Since availability is a property of the lens rather than of any record, `isIsomorphism()` and `isomorphismObstruction()` answer it once for every view a lens will produce: the latter returns the first failing condition (a dropped vertex, a dropped edge, or a non-injective value transform) or `null`, so a caller can branch statically instead of catching a throw. Closes #255.

## [0.68.0] - 2026-07-28

### Added

- **A source can be reconstructed from a view alone, when that is possible at all** (`panproto-lens`): `put` required a complement produced by an in-process `get` of the same record, so a stored view could not be sent backward through a lens. `put_without_complement(lens, view)` lifts that requirement exactly as far as it can be lifted. A lens with complement decomposes its source as `S ≅ V × C`, with `get` the isomorphism and `put` its inverse, so reconstruction from a view alone is the question of whether `S ≅ V`, which holds precisely when `C ≅ 1`. `Lens::is_isomorphism` decides that statically: every vertex survives, every edge survives up to renaming, and the composite coercion class is `Iso`. When it holds, the reassembly bookkeeping is a function of the view's own shape and is derived from it. When it does not, the request is refused with `LensError::NotAnIsomorphism` naming the obstruction, because `get` is then not injective: distinct sources share the view, and no section of it exists to return. `Complement::residue_is_trivial` exposes the same distinction per projection, separating the residue (what `get` discarded, which *is* `C`) from the reassembly bookkeeping (`original_parent`, `arc_order`, `arc_edges`, which carry nothing the view lacks). `Complement::is_empty` answers a different question, whether the representation is blank, and is documented as such. Refs #253, refs #250.

### Fixed

- **A rename records itself on the migration, so composition can see it** (`panproto-lens`): the field-coordinate conjugation added in 0.66.0 reads the first migration's `edge_remap`, and `ProtolensChain::instantiate` left that map empty. `compute_migration_between` derived `vertex_remap` by structural matching but never edges, so a rename was realized by rewriting the target schema and was invisible to anything reasoning about the migration itself. The 0.66.0 repair therefore never fired on the path a caller actually takes, and the original report still failed: the composite rejected `lower(displayName)` and accepted `lower(name)`, a field absent from the second lens's input schema. Instantiation now derives `edge_remap` for edges that survive under a different name, matching endpoints through `vertex_remap` and claiming each target edge at most once so parallel edges pair off one-to-one, and records a renamed edge in `surviving_edges` under both spellings since it survives rather than being dropped. The regression tests were the second half of this: they hand-assembled a `CompiledMigration` with `edge_remap` set directly, which exercises the conjugation but not the path any caller reaches it by, and so passed throughout. Every one of them now goes through `combinators::…instantiate`, including the swap, which is built as a three-step chain that instantiation collapses into a simultaneous relabeling. Closes #251.
- **Collection order survives the backward direction** (`panproto-lens`): `put` rebuilt arcs by iterating `original_parent`, a `HashMap`, so the order varied between runs and every array came back permuted; dropped arcs were separately appended after the surviving ones rather than restored to their position. The children of a collection node *are* its elements in sequence, so this returned reordered records. The complement now records the source's arc sequence and the backward direction replays both restoration paths in that order, falling back to ascending id order for a complement recorded before the sequence existed. Part of #253.
- **An inverse still runs when its value crossed the JSON boundary** (`panproto-lens`): the forward pass writes a computed value into `extra_fields`, where serialization lets it shadow the child that supplied it. Serializing the view and parsing it back, which is exactly what the WASM `put_json` does, moves that value onto the child node: the parent's `extra_fields` arrives empty and the child holds the computed number, so the backward pass found nothing to invert and the computation survived into the reconstruction. It now recovers the source value from the child, guarded on the value's absence from `extra_fields` so the in-memory path does not invert twice. Part of #253.
- **The round-trip laws can see a permuted collection** (`panproto-lens`): `instances_equivalent` compared arcs as a sorted set, so two instances differing only in child order were reported equivalent. That is why `GetPut` and `PutGet` both held while the reconstruction returned reordered records, which made the laws useless as evidence for exactly the defect above. It now compares each parent's ordered child list alongside the arc set. Part of #253.
- **`put` refuses a complement it cannot have produced** (`panproto-lens`): passing an empty complement returned an empty record rather than an error. `put` inverts `get` on the image of `get` and nowhere else, and a complement recording no parent for any of the view's arcs is outside that image; reassembling from it rebuilt the node set with no arcs between the nodes, which serializes to `{}`. Returning that silently is worse than refusing, since a caller cannot distinguish a reconstructed empty record from the total loss of a populated one. Part of #253.

## [0.67.0] - 2026-07-27

### Added

- **Round-trip laws are checkable on a compiled migration** (`panproto`): verifying `GetPut` / `PutGet` was unreachable for a schema of realistic size. The law checks live on `Lens`, and the only Python routes to a `Lens` were `auto_generate_lens` and `ProtolensChain.auto_generate`, both of which search for a schema morphism. The artifact that does scale, a `CompiledMigration` built by `compile_migration` or derived by the version-control layer from a name-keyed diff, exposed only the forward direction, so a project could not check the very property the lens machinery exists to guarantee. No search was ever needed: a lens *is* a compiled migration together with the two schemas it runs between, which is exactly what `CompiledMigration` already holds. It gains `put(view, complement)`, `check_laws`, `check_get_put`, `check_put_get`, and `to_lens()`, the last of which cannot fail. The TypeScript SDK already had this, since the WASM boundary builds a lens from its migration handle; this closes the Python gap. Closes #250.

### Fixed

- **`CompiledMigration.get` returns the complement, and the complement `put` consumes** (`panproto`): it previously returned a summary dict of two counts, so the complement never crossed the binding boundary and no backward direction was expressible from Python whatever else was exposed. It now returns the `Complement` object, whose `dropped_node_count` and `dropped_arc_count` attributes carry the same two numbers. The projection also now runs the lens rather than the lower-level restrict pipeline: the complement that pipeline produces does not record the arc provenance the backward pass needs, so pairing it with `put` restored the right nodes with no edges between them, which serializes to an empty record while the node count still matches. Taking both halves from the same lens is what makes the round trip exact.

### Changed

- **`CompiledMigration.get`'s second return value is a `Complement`, not a `dict`** (`panproto`): code reading `complement["dropped_node_count"]` should read `complement.dropped_node_count`. The counts are unchanged; only the access changes, and the object now carries everything `put` needs rather than a summary of it.

## [0.66.0] - 2026-07-27

### Added

- **A parent vertex can aggregate over an array-of-objects child** (`panproto-inst`): a field transform's environment bound `extra_fields` plus the node's *scalar* children, so a child that is an array of records bound to nothing and an aggregate at the parent (the minimum and maximum of `timeMs` across a keyframe array, a running fold over prior segment lengths, a collection-membership count) could not be written at all. It failed with `unbound variable`, and the graph-traversal builtins were no help because they take a node reference and no current node was supplied, so `children`/`edge`/`edge_count`/`anchor` evaluated to null wherever a transform ran. Two bindings close the gap. `collect_child_values` materializes structural children as well as scalars, an ordered collection becoming a `Value::List` and a record a `Value::Unknown`, recursively and following the same list/record decision `to_json` makes; since `value_to_expr_literal` is structure-preserving, `map`/`fold`/`head`/field projection reach them directly. And `TransformContext` carries the instance and the node id into evaluation, which is what makes `"self"` resolvable, so the graph builtins walk from the current node and the whole fiber is additionally bound as the variable `self`. Materialization is demand-bounded: a structural child is walked only when a transform's free variables name it (or when `self` is read, which cannot say in advance which children it will reach), so the common scalar-only transform costs what it did before. `apply_field_transforms` now takes a `TransformContext` in place of a bare child-scalar map; a caller holding a node but no instance uses `TransformContext::detached()` for the previous `extra_fields`-only behaviour. Closes #247.
- **`getJson` / `putJson` on a chain-instantiated lens** (`@panproto/core`): `chain.instantiate(schema).get(bytes)` materialized the transformed view inside the instance graph and gave no way to read it back as a record: `LensHandle` had no JSON emit path, `getJson`/`putJson` existed only on `CompiledMigration`, and `toJson(schema, view)` threw on a lens `get()` view. A consumer using the lens as the actual data mapper therefore hand-walked the decoded `WInstance` (nodes, arcs, `Value::Unknown`/`Present` wrappers) to reconstruct its own output, which is the difference between the lens executing a mapping and the lens being a verified specification sitting beside a hand-coded executor. `LensHandle.getJson(record, rootVertex)` and `putJson(view, complement, rootVertex)` mirror the `CompiledMigration` pair and route to the same WASM entry points, which already accepted the resource `instantiate_protolens` produces. Closes #248.

### Fixed

- **Composition is functorial on value-level field transforms** (`panproto-lens`): `compose` transported the second lens's transforms into the first's source frame by conjugating the *anchor* coordinate through `vertex_remap` and then concatenating the entries verbatim, leaving the *field* coordinate alone. So a second-lens expression's free variables were interpreted against the first lens's source field names rather than its output names, and `get(m2 ∘ m1) ≠ get(m2) ∘ get(m1)`: with `m1` renaming the schema edge `name → displayName` and `m2` computing `lower(displayName)`, sequential application succeeded while the composite failed with `UnboundVariable("displayName")`, and the composite instead accepted `lower(name)`, naming a field absent from `m2`'s own input schema, so the sound program was rejected and the unsound one accepted. The defect is specific to **schema-edge** renames: those are applied via `edge_remap` when output arcs are materialized and never reach the `child_scalars` map the value layer reads, whereas a `FieldTransform::RenameField` rewrites the `extra_fields` key in place and runs ahead of the second lens's entries in the merged batch, so that route was already correct and is deliberately left un-conjugated. Expression free variables are now rewritten along the inverse of the first lens's edge renames, simultaneously rather than by sequential substitution so that a swap `{a → b, b → a}` does not collapse. Reading a field the first lens takes away (dropped outright, or renamed away with nothing else binding the name) is now reported as `LensError::ComposeUnboundField` when the lenses are composed, rather than as an `UnboundVariable` far from its cause; the check is suppressed where a surviving child edge or a later transform still binds the name, so it does not reject compositions that run correctly. Closes #245.
- **`PutGet` no longer depends on the leaf type of a computed field** (`panproto-lens`): a `compute_field` regroup building a record from two leaves satisfied both round-trip laws over integer, number, and boolean leaves but failed `PutGet` over string leaves, making every string-valued regroup (labels, names, ids, URIs, source tags, which is most real ones) unusable as a laws-verified lens step. Neither outcome was right, and for the same reason. `check_put_get`'s canned mutation appended `_modified` to string leaves only, so for the other three types the mutated view equalled the original, the mutated branch was skipped, and the law was reported to hold without having been tested; for strings the branch ran and compared a view whose *derived* field was stale against a re-`get` that had recomputed it, reporting a violation that says nothing about the lens. A view with a derived coordinate is not a free view space, since the coordinate is pinned by the independent ones, so a view edited without re-deriving is outside the image of `get` and the law cannot be checked strictly against it. `PutGet` is now checked modulo derived components, which the new `panproto-lens::derived` module identifies as the targets of any transform that materializes a value the backward pass cannot send to an independent source coordinate (`ComputeField`, `AddField`, `ApplyExpr`, recursing through `PathTransform` and `Case`). This is the property the `ComputeField` documentation already stated for `Opaque` computations, applied uniformly. The canned mutation now perturbs integer, float, and boolean leaves too, so no leaf type passes vacuously, and a violation now names the diverging node and field instead of reporting only node counts. `GetPut` is unaffected and stays strict: its view argument is `get(s)`, consistent by construction. Closes #246.
- **An invertible transform's inverse reaches the field it read** (`panproto-lens`): a `ComputeField` carrying an inverse failed both round-trip laws. The backward pass wrote the inverted value to the transform's own `target_key`, which does two wrong things at once: it reinstates on the source a computed key the source never carried, so `GetPut` sees a field appear from nowhere, and it drops the edit, since the inverse exists to recover the coordinate the *forward* expression read. `put` now removes the computed key and sends the inverse's result to the field that expression reads, identified as its sole free variable (an inverse yields one value, so a computation over several fields, or none, has no single coordinate to restore and is not a bijection whatever `coercion_class` claims). `ApplyExpr` had the same split in a subtler form: it reads and writes one key, but over a *child scalar* it reads the child and writes a shadowing `extra_fields` entry on the parent, so the backward pass invented a parent field the source never had. It now writes back only where the source carried an entry, leaving the child node authoritative. Classification follows the same distinction: a transform is an independent coordinate only when it *replaces* what it read rather than adding a value beside it, so `up = upper(a)` with `a` still in the view is derived (the two are redundant and `get` recomputes `up` from `a` regardless of the inverse) while the same computation followed by dropping `a` is independent, and an edit to `up` round-trips to `a` exactly.

## [0.65.0] - 2026-07-23

### Added

- **Per-file project trees reach the VCS commit porcelain** (`panproto-vcs`, `schema`, `panproto`): directory staging and the Python repository binding previously flattened a multi-file project into one schema before writing the index. This staging-flattening gap discarded the tree root produced by `ProjectBuilder::build_tree` or `build_project_tree`, so a one-file edit forced the next commit to realign the whole schema instead of reusing every unchanged file object. `Repository::add_tree` and `add_tree_with_options` now assemble an existing tree for migration derivation and validation while retaining its root in the index and commit. `schema add <dir>` uses this path for parsed source projects and manifest-declared bundle protocols, including ATProto lexicon sets with cross-file references; Python exposes the same path as `Repository.add_project(project, skip_verify=False)`. Regression tests confirm that a changed file receives a new object ID while its unchanged sibling retains the old one. Closes #243.

## [0.64.0] - 2026-07-23

### Added

- **`parse_schema_bundle_project`: per-file provenance for lexicon sets in the VCS** (`panproto-protocols`, `panproto`): parsing an ATProto lexicon *tree* had no path into the per-file project schema the version-control layer stores and diffs. `parse_schema_bundle("atproto", docs)` resolved cross-lexicon refs correctly but fused every document into one flat, path-less schema; `ProjectBuilder` / `parse_project` produced a per-file `ProjectSchema` but parsed `.json` as generic JSON, losing lexicon structure. So the atproto-correct parse and the per-file store did not compose. `parse_schema_bundle_project(protocol, docs)` (dispatching to `atproto::parse_lexicon_project`) parses the set as a bundle so in-set refs resolve to typed defs, then partitions the flat schema back by NSID ownership: each vertex and same-file edge returns to the document that declared it, while a ref that crosses documents becomes a `<path>::<name>`-prefixed cross-file edge. The result feeds `panproto_project::build_project_tree` to store a lexicon set as the per-file Merkle tree the VCS diffs incrementally; a round-trip test confirms two lexicons store as two per-file leaves and assemble back with the cross-file ref resolved to the typed def (not an opaque placeholder). Exposed in Python as `panproto.parse_schema_bundle_project(protocol, docs)` returning a `LexiconProject` (`.files()`, `.cross_file_edges()`); `bundle_project_protocols()` lists the protocols that retain per-file provenance (currently `atproto`). Closes #240.

## [0.63.0] - 2026-07-23

### Added

- **`Repository::add_with_options` and a `skip_verify` escape hatch for staging** (`panproto-vcs`, `panproto`, `schema` CLI): `add` previously always ran GAT migration validation (a bounded model check) against HEAD on every staged schema, with no opt-out, so building a VCS of many historical versions in sequence was impractical (each ~800-vertex `add` past the first cost minutes of validation). `add` now delegates to `add_with_options(schema, &AddOptions { skip_verify })`, mirroring `commit`'s existing `skip_verify`: with the flag set, the migration is still derived and recorded but validation is skipped and the stage is left `Pending`, which a default `commit` treats as non-blocking. It is surfaced as `repo.add(schema, skip_verify=True)` in Python and `schema add --skip-verify` in the CLI, so a caller replaying already-validated released versions can build a historical VCS without paying the per-`add` model check. Closes #239.

## [0.62.0] - 2026-07-22

### Added

- **`Panproto.symmetricLens(left, right)`** (`@panproto/core`): a public, high-level constructor for a `SymmetricLensHandle`, matching the existing `lens` and `protolensChain` accessors. A symmetric lens previously had to be built through the lower-level `SymmetricLensHandle.fromSchemas` static, which required passing the internal WASM module handle. Now `p.symmetricLens(a, b)` returns a handle whose `syncLeftToRight` and `syncRightToLeft` methods propagate a change on either side to the other, preserving each side's private information in the shared complement.

### Fixed

- **`ref` and `record-schema` edges are transparent to the instance parser** (`panproto-inst`): a field whose type is a named definition (a nameless `ref` edge) or a record body (a nameless `record-schema` edge) was skipped by `parse_object`, so the referenced object's fields fell through into `extra_fields` as an opaque `Unknown` and the record parsed to a shallow one-node instance. A structurally-equivalent schema that inlines the same nesting with named `prop` edges parsed deep, so the same data produced two different instance graphs depending on the protocol, which breaks the "every protocol is a view over one graph" premise and makes lenses, field transforms, and queries protocol-dependent. `parse_object` now resolves through a nameless `ref` or `record-schema` indirection to the object definition it denotes, anchoring the node there so its fields materialize on the instance graph exactly as an inlined definition would; a *named* `ref` (the labelled reference some protocols emit for a pointer to a named definition) stays an ordinary field. The change is parse-only: serialization already reconstructed the nested JSON, so the round-trip is unchanged. Closes #237.
- Corrected stale built-in-protocol counts in the TypeScript SDK doc comments (`@panproto/core`): the protocol-name accessors (`protocol`, `listProtocols`, `getProtocolNames`) now read 54, not a stale 76; the I/O codec-registry comments drop the stale 77 rather than assert a build-feature-dependent count.

## [0.61.0] - 2026-07-22

### Added

- **JSON Schema, GraphQL, SQL, and Protobuf are first-class semantic protocols again** (`panproto-protocols`, `panproto-wasm`, `panproto-c`, `panproto-py`, `@panproto/core`): the four hand-written protocol parsers deleted in the v0.17.0 tree-sitter migration are restored and modernized. Tree-sitter parses document *syntax*, not schema-language *semantics* (a JSON Schema `{type, properties, $ref}` into vertices, edges, and constraints), so the semantic loaders had been lost while the SDK still advertised the protocols as stubs. `p.protocol('json-schema')` now returns the real 11 object kinds instead of `['object']`, so building a schema with a `string`/`integer`/`array` vertex works rather than throwing `unknown vertex kind`. Restores the built-in semantic protocol count to 54. Closes #234.
- **Every protocol's single-document parser is reachable from the SDK** (`panproto-protocols`, `panproto-wasm`, `panproto-py`, `@panproto/core`): two generic, name-dispatched entry points expose all 54 built-in parsers, where previously only `atproto` was reachable (through `parse_schema_bundle`). `parse_schema_document(protocol, doc)` routes the 43 JSON-document parsers; `parse_schema_source(protocol, source)` routes the 11 text/IDL parsers (SQL DDL, GraphQL SDL, Protobuf `.proto`, CDDL, Cassandra CQL, Cypher, ASN.1, Bond, FlatBuffers, CoNLL-U). Dispatch lives in `panproto-protocols` and normalizes an underscore key to its canonical hyphenated protocol name (with `uima` aliasing `uima-cas`), so both spellings resolve. The TypeScript SDK gains `parseSchemaDocument` / `parseSchemaSource`; the Python `parse_schema_document` is consolidated onto the shared dispatch (dropping its hard-coded `atproto` arm) and gains `parse_schema_source`. A JSON Schema (or OpenAPI, Avro, …) document can now be loaded into a `Schema` in-process and used as a lens or migration endpoint.

## [0.60.0] - 2026-07-21

### Fixed

- **Field transforms preserve list and record structure** (`panproto-inst`, `panproto-expr`, `panproto-expr-parser`): a transform whose expression read or returned a list- or nested-object-valued field silently did nothing — no error, no change, the field came back untouched. Since `ATProto` records keep arrays and nested objects inline in `extra_fields`, that covered most real lexicon fields and limited any lens over such a record to scalar rewrites. Three defects had to be fixed together, because a container transform needs all three to run at all. First, the conversion between instance values and expression literals dropped structure in both directions: `value_to_expr_literal` flattened `Value::List` to a comma-joined `Literal::Str` (discarding every non-string element, so an array of integers became `""`) and mapped `Value::Unknown` to `Null`, while `expr_literal_to_value` handled only the four scalar variants, so an expression *returning* a list of records was written back as `Null`. Both directions now convert `List` and `Unknown`/`Record` recursively, with record fields emitted in sorted key order since `Value::Unknown` is a `HashMap` and the conversion has to be a function. Second, `contains` is overloaded on its first argument — substring containment on a string, element membership on a list — which is what the joined-string projection existed to serve, now as exact-element membership rather than substring. Third, the parser lowered `map f xs` to `Builtin(Map, [f, xs])` while the evaluator reads the list from `args[0]`, so every text-authored `map`, `filter`, and `fold` failed with "expected list, got function" regardless of the conversion; the parser now permutes saturated higher-order list builtins into evaluator order and the pretty printer inverts that permutation, so stored expressions keep their meaning and printing a parsed expression still reproduces the source form. Closes #230.
- **Range expressions evaluate** (`panproto-expr`, `panproto-expr-parser`): `[1..3]` desugared to `map` over an *integer* — the range's length — rather than over a list, so every bounded range failed with "expected list, got function" and had evidently never run. Nothing in the expression language constructed a list from a bound, so the desugaring had no correct target. A `range(start, stop)` builtin now constructs the list, with both bounds inclusive and a descending range yielding the empty list rather than an error, and `[a..b]` lowers to it; `range a b` names the same builtin directly. The length is computed and checked against `EvalConfig`'s `max_list_len` *before* allocating, since a range is the one builtin that turns a constant-size expression into an arbitrarily long list. Open-ended `[a..]` is now rejected with a message naming the construct: it previously parsed to the one-element list `[a]`, which is a wrong answer rather than a missing feature, and the language has no lazy lists to lower it to. Closes #231.
- **`optic_kind` no longer calls a lossy migration an isomorphism** (`panproto-lens`, `panproto-inst`): the classification checked structure alone — every source vertex and edge survives, no variant changes — and never inspected the migration's value transforms, so a migration could be classified `Iso` while carrying a transform whose `CoercionClass` is `Retraction`, `Projection`, or `Opaque`. The schema-level map is a bijection; the value-level action is not, and `Iso` asserts that both round-trip laws hold and that the complement stores nothing. `optic_kind` now returns `Iso` only when `CompiledMigration::coercion_class` reports the composite of every value transform as `Iso`, and classifies a structurally-bijective but lossy migration as `Lens`. `coercion_class` itself now folds the lowered `TermAssignment`s alongside the direct `FieldTransform`s: `panproto-mig`'s compiler emits its value transforms as term assignments, so folding only field transforms reported `Iso` — the identity element of an empty fold — for exactly the migrations carrying the most value-level coercion. Closes #233.
- **A field transform that fails to evaluate is reported** (`panproto-inst`, `panproto-lens`): `apply_field_transforms` evaluated with `if let Ok` and wrote only on success, so an unevaluable expression left the field untouched and returned success — making a broken lens indistinguishable from one that ran and changed nothing, which is why the flattening above presented as "the lens quietly did nothing". It now returns `RestrictError::FieldTransformFailed` naming the offending field, propagated through restrict, extend, the relational functors, and the edit lens. `apply_path_transform` and `apply_term_assignments_to_row` restore the map they moved out before propagating, so a failed transform does not also erase the fields it was reading.

## [0.59.0] - 2026-07-21

### Added

- **Schema-document references resolve across a bundle** (`panproto-protocols`, `panproto-wasm`, `panproto-py`, `@panproto/core`): a schema-document parser sees one document at a time, so a reference into another document (say the `ATProto` lexicon `pub.layers.annotation.annotationLayer` referencing `pub.layers.defs#spatioTemporalAnchor`, which refs `#boundingBox`) became an opaque `"ref"` placeholder vertex carrying no fields, and a lens had nothing typed to bind to. `atproto::parse_lexicon_bundle` takes a slice of documents, registers every document's defs before parsing any document's structure, and so resolves each in-bundle ref to that def's real, typed vertex; a ref whose target is in no document of the bundle stays a placeholder, which is what marks it as genuinely external. `parse_lexicon` is now the single-document case of it, with unchanged behavior. The entry point is protocol-neutral, mirroring the Python SDK's existing `parse_schema_document`: `protocols::parse_schema_bundle(protocol, docs)` dispatches on protocol name and is surfaced as `parse_schema_bundle` at the WASM boundary, `parseSchemaBundle` in the TypeScript SDK, and `parse_schema_bundle` in the Python SDK. The dispatch lives in `panproto-protocols`, so the generic crates carry no protocol names and stay within the genericity guardrail; `bundle_parser_protocols` (`list_bundle_parser_protocols` at the WASM boundary) reports the supported set. Cross-document resolution is implemented for `atproto`; the formats with the same latent gap (OpenAPI's cross-file `$ref`, Avro's namespaced named types) become one dispatch arm each, with no binding-surface change. Closes #224.
- **Value-transform lenses are reachable from JavaScript** (`panproto-wasm`, `@panproto/core`): a lens document's `apply_expr`, `compute_field`, `hoist_field`, and `nest_field` steps compile to value-level *field transforms* rather than to structural chain steps, and `compile_lens_document` discarded that half at the WASM boundary. A JS caller could therefore author only structural lenses: a document whose substantive step computed a field or regrouped flat fields into a nested object compiled to an empty chain, and `get` returned its input untouched. The compiled handle now carries both halves, and `instantiate_protolens` folds the field transforms into the migration it produces, so `get_record` evaluates them and `put_record` inverts them through the existing complement machinery. `protolens_compose` carries transforms through a composition, and a new `protolens_field_transforms` export (`fieldTransforms()` in the TypeScript SDK) lists them, since they do not appear in `toJson()`. Closes #223.

## [0.58.0] - 2026-07-14

### Added

- **Refined scalar value kinds** (`panproto-gat`): `ValueKind` gains `DateTime`, `Date`, `Time`, `Decimal`, and `Uuid`, each with a canonical name (`date-time`, `date`, `time`, `decimal`, `uuid`). A record-to-theory encoder can now recover a datetime, date, time, decimal, or uuid field as such instead of collapsing it to `Str`; the Python SDK picks the kinds up automatically because its name-to-kind lookup is driven by `ValueKind::as_str`, and the Haskell and TypeScript bindings gain the variants for parity. The new variants are declared after `Any` so the existing kinds' discriminants are preserved. Closes #190.

### Fixed

- **Tree-sitter MISSING anonymous tokens surface as recovery markers** (`panproto-parse`): when tree-sitter recovers from an incomplete construct by inserting a zero-width MISSING token that is anonymous (a `]`, `}`, `)`, `,`, or keyword), the walker dropped it silently, leaving a recovered-incomplete parse with no `ERROR` vertex and no zero-width vertex — indistinguishable from a complete parse. The walker now scans every node's children for such tokens and emits a zero-width, `ERROR`-kinded marker vertex carrying a `missing` constraint, so a schema walker that rejects `ERROR` / zero-width vertices detects the recovery. Closes #214.
- **De-novo bash `case` emits one `;;` per item** (`panproto-parse`): on the by-construction emit path a `case`/`esac` over-emitted the `;;` terminator (three per case item) and dropped the newlines between items, so the re-parse gained an `ERROR` node. A per-vertex `ptrace`-literal budget caps each recorded separator literal at its true source multiplicity across a single vertex's emit walk, so the single `;;` is matched once. Closes #204.

## [0.57.0] - 2026-07-13

### Added

- **Theory morphisms map an operation to a derived term, not only to another operation** (`panproto-gat`, `panproto-inst`, `panproto-mig`): a `TheoryMorphism`'s operation map now carries an `OpAssignment` — either a single target operation or a whole term built from target operations and variables — so a source operation with no one-to-one counterpart (say, `midpoint(a, b)` sent to `scale(add(a, b), half)`) can still be mapped. `Delta` and `Sigma` migration evaluate the assignment by substituting the source arguments into the term rather than by a name lookup, and the migration compiler records the per-operation term assignments (`TermAssignment`, `TermScope`, `TermBranch`) it needs to replay that substitution over an instance.
- **The chase runs at the term level, with variables, labeled nulls, and equality-generating dependencies** (`panproto-mig`, `panproto-inst`): the migration chase gains a real embedded-dependency engine. A tuple-generating dependency whose head has an existential invents a fresh *labeled null* (`Value::LabeledNull`) instead of a placeholder string, an equality-generating dependency unifies two labeled nulls (or fails on a constant clash), and the whole run is bounded by a `ChaseBudget`, so a non-terminating dependency set surfaces as budget exhaustion rather than hanging. `Dependency`, `Atom`, and `ChaseOutcome` expose the engine, and the dependencies a migration must satisfy are read off the target theory.
- **Equalities carry a checkable proof** (`panproto-gat`): normalization records the chain of rewrite steps it took, and an `EqWitness` bundles that chain into a proof that two terms are equal in a theory. A verifier replays a witness step by step and rejects one whose steps don't hold, so an equality claim is auditable rather than trusted. Checking a theory morphism emits, for each source equation, a witness that the equation's image is derivable in the target.
- **Instance homomorphisms are first-class, and the Sigma/Delta adjunction is explicit** (`panproto-inst`): homomorphisms between instances (weighted and finite) are values you can build, compose, and check. The migration adjunction exposes its structure directly — `w_unit`/`w_counit` and the finite `f_*` counterparts, and the `Sigma ⊣ Delta` hom-set bijection through `w_transpose_left`/`w_transpose_right`, with an `AdjunctionError` for the ways a transpose can fail. `sigma_functoriality` is strengthened from a sorts-and-ops check to a genuine instance isomorphism.
- **Attributed C-sets keep entities and attributes apart** (`panproto-inst`): an instance now distinguishes its *entity* sorts from its *attribute* sorts — the columns holding primitive values (a string, a number) rather than references to other rows. Migration and homomorphism search respect the split: an attribute is matched by value while an entity is matched up to the homomorphism, so a value column is never mistaken for a foreign key.
- **The version-control history is a double category** (`panproto-vcs`): commits compose along one axis and migrations along the other, and a square — a commit-then-migrate that must agree with the migrate-then-commit on the other two sides — is checked by `verify_square`. Rebase and cherry-pick are squares in this structure; their coherence is checked when they run and exercised by property tests.
- **`panproto-dsl-eval`, a shared Nickel/YAML/JSON evaluation crate** (new crate): the Nickel, YAML, and JSON evaluation that the theory and lens DSLs both needed now lives in one crate that owns the `nickel-lang` dependency, exposing `eval_nickel`/`eval_yaml`/`eval_json` and a `DslEvalError` that carries source spans for Nickel failures.
- **Value-preserving YAML, TOML, and CSV codecs are registered with the built-in protocols** (`panproto-protocols`, `panproto-io`): parsing and re-emitting an instance through these formats preserves the structure the protocol layer round-trips on, so a migration over a YAML, TOML, or CSV instance keeps its shape instead of losing it to a bare re-serialization.
- **The QVR grammar tracks Quivers 0.15.0** (`grammars/qvr`, `panproto-parse`): the vendored Quivers tree-sitter grammar is refreshed to 0.15.0, which renames `let` to `define`, moves algebra selection into a `[level=algebra]` attribute, and adds positional distribution arguments; the QVR parse tests are updated to the new surface syntax. Closes #213.

### Changed

- **A theory registers only if its equations form a terminating, confluent rewrite system** (`panproto-gat`): registration orients each equation into a rewrite rule, checks that the system terminates (by a lexicographic path order) and is confluent, and rejects a theory whose equations do not converge, rather than letting it produce nondeterministic normal forms later.
- **`normalize` reports when it exhausts its rewrite budget** (`panproto-gat`): instead of silently returning a possibly-unnormalized term, `normalize` surfaces budget exhaustion, so a caller can tell "this is the normal form" from "I stopped early".
- **Morphism checking verifies equation preservation by derivability** (`panproto-gat`, `panproto-mig`): a theory morphism is accepted when each source equation's image is *derivable* in the target, not only when it appears verbatim, so a morphism into a theory that proves the same equations by different rules is correctly accepted.
- **Pushouts verify the universal property at construction** (`panproto-vcs`): a schema merge builds the pushout and then checks that both sides include into it and that the result is the minimal such schema, so a merge that is not a genuine pushout fails loudly instead of producing a plausible-looking wrong schema. This runs for rebase and cherry-pick as well as a direct merge.
- **One `Complement` type across the lens and instance layers** (`panproto-inst`, `panproto-lens`): the duplicated complement representations collapse into a single `Complement`, whose `contracted_into` map records where a dropped node's children were reattached, and lens and migration composition route through the same shared logic.
- **Protolenses generate a natural transformation the GAT checker can verify** (`panproto-lens`): each step in the protolens vocabulary carries a symbolic proof of its lens laws, and a protolens as a whole is turned into a `NaturalTransformation` that the GAT layer's morphism checker validates.
- **Grammar-pack manifests are generated from a single source file** (`xtask`, `grammar-packs.toml`): the ten `panproto-grammars-*` crate manifests are generated by `xtask gen-grammar-packs` from one `grammar-packs.toml`, so their shared boilerplate and the workspace-version pin stay in sync automatically.
- **`git2` is declared once in `[workspace.dependencies]`** (workspace `Cargo.toml`): the crates that use `git2` inherit a single pinned version instead of each declaring their own.
- **`panproto-core` re-exports the expression and DSL crates** (`panproto-core`): `panproto-expr`, `panproto-expr-parser`, `panproto-lens-dsl`, and `panproto-theory-dsl` are reachable through `panproto-core`, so a downstream consumer depends on one crate rather than five.

### Fixed

- **macOS links the whole workspace under plain `cargo build`** (`.cargo/config.toml`): the pyo3 extension-module cdylibs (`panproto-py` and the `panproto-grammars-*` companions) resolve CPython symbols when loaded into an interpreter, not at link time, which the macOS linker rejects. A `.cargo/config.toml` passes the `-undefined dynamic_lookup` flags maturin uses, scoped to the Apple targets, so a local `cargo build --workspace` links instead of failing on undefined `_Py_*` symbols.
- **Nickel evaluation errors point at the source** (`panproto-theory-dsl`, `panproto-dsl-eval`): a Nickel error raised while evaluating a theory DSL source now carries the byte span it came from, so the message locates the offending expression instead of only naming the file.
- **`gat-macros` book examples compile in CI** (`book-doctest-stub`, `xtask`): the derive-macro examples in the book are compiled as part of the doc gate, so an example that drifts from the macro's real API breaks the build instead of shipping wrong.

### Security

- **`anyhow` 1.0.102 → 1.0.103** (workspace): clears RUSTSEC-2026-0190, undefined behavior in `anyhow`'s `Error::downcast_mut` when called on an error that had context added via `Error::context`.

## [0.56.1] - 2026-06-18

### Fixed

- **`emit_pretty` no longer glues a relocated subtree onto its sibling** (`panproto-parse`): when a parsed subtree (e.g. a Python `class_definition`) is grafted onto a fresh schema beside another top-level statement, it keeps the `[start-byte, end-byte)` span it had in its original source. The byte-faithful replay path tiled that stale span and emitted the subtree as raw bytes, which bypasses the separator its new context needs: the class body's last token ran straight into the next `def` (`…(1 + 2)def f():`), invalid Python. The replay path now detects a relocated subtree (one whose recorded span is not contained by its new parent's) and restores a line break on each edge of the verbatim blob, keeping the body byte-faithful while preventing the glue. Line breaks are idempotent at a line start, and a freshly parsed schema (every child's span nested in its parent's) is never relocated, so the canonical corpus replay is byte-identical. Closes #202.
- **De-novo emit keeps a julia `if`/`elseif`/`else` clause body on its own line** (`panproto-parse`): on the by-construction path (no layout fibre), a clause's body separator is the optional `_terminator` slot, whose newline form is a `\r?\n` pattern. The choice tie-break matches only string literals, never a pattern, so the slot fell through to `BLANK` and the body glued onto the keyword line (`elseif x < 0` ran into `-1`, re-lexing as one expression and dropping the clause's `block`). An optional separator CHOICE that offers a newline alternative now defers instead of forcing `BLANK`, so the existing pure-separator newline preference supplies the canonical newline. `;`-terminator / ASI slots are unaffected.

### Changed

- **`ruby` emit verification status is `Verified`** (`panproto-parse` test suite): `ruby` is in `VERIFIED_EMIT_PROTOCOLS` (its full upstream `test/corpus` round-trips under the strict oracle), so the status test now asserts `Verified` rather than the stale `Generic`.

## [0.56.0] - 2026-06-17

### Added

- **Committed data sets carry a per-record key** (`panproto-vcs`, `panproto-py`): `DataSetObject` gains a `key: Option<String>` field, so a committed set read back through `data_at` can be mapped to a downstream identifier (a source path, an AT-URI, or any caller key) instead of being anonymous. `Repository::add_data(path, key)` records the key, falling back to the source path when `key` is `None`; the key is carried forward unchanged across data migration (forward, backward, and directory migration). The Python `Repository.add_data(path, key=None)` and the dict returned by `Repository.data_at` (now including `key`) expose this. Closes #198.

### Fixed

- **A data-only change now commits** (`panproto-vcs`, `panproto-py`): `Repository::commit` required a staged schema and raised `NothingStaged` for a stage holding only data (or only a protocol), even though `Index::has_staged` reported it staged. The two now agree: when no schema is staged, `commit` builds a data-only or protocol-only commit that carries HEAD's schema forward with no migration. Re-recording records of an already-committed type, then diffing revisions by data alone, is now possible. Closes #197.

### Changed

- **`Repository::add_data` takes a key argument** (`panproto-vcs`): the signature is now `add_data(&mut self, path: &Path, key: Option<&str>)`, and `DataSetObject` carries a `key` field, to support the per-record key above. The Python binding adds an optional `key` keyword (`add_data(path, key=None)`), so Python callers are unaffected; Rust callers pass `None` for the previous behavior.

## [0.55.0] - 2026-06-17

### Added

- **Haskell bindings at full parity with the Python SDK** (`bindings/haskell`, `crates/panproto-c`): the Haskell `panproto` package now reaches the whole engine, matching the Python SDK across schema construction, protocol round-trip, migration, morphism (hom) search, diff and compatibility classification, instance build and JSON I/O, lens and protolens algebra, the GAT layer (theories, free models, and morphism checking), the expression language, version control, data versioning, and graph and fibre introspection, with parsing, multi-file project assembly, and git import behind cabal feature flags. A Haskell consumer can do everything a Python consumer can through an API that reads as idiomatic Haskell.

  The bridge is the `panproto-c` C ABI, which grows from a handful of entry points to over 120 frozen `pp_*` functions covering every domain. Each follows the established panic-safe contract: a `guard(...)` wrapper that catches unwinds and returns an `i32` `PpStatus`, opaque `u32` slab handles on the hot path (no serialization), and CBOR payloads (ciborium on the Rust side, [`cborg`](https://hackage.haskell.org/package/cborg) on the Haskell side) on the cold path, with a drained `ErrorEnvelope` carrying the failure detail. The handle table is a process-global allocator so a handle stays valid across OS threads under GHC's threaded runtime; the input and output buffer marshalling is exception-safe end to end.

  The Haskell layer is built on this with a capability typeclass per domain (`ProtocolBackend`, `SchemaBackend`, `MigrationBackend`, `LensBackend`, `GatBackend`, and the rest), a `Native` and a `Rust` backend tag, and tight integration with the standard classes: `Migration` composes through an associative `Semigroup` (its drop-on-miss composition has no schema-independent unit, so the identity is the per-schema `identityMigrationOn`, mirroring `panproto_mig::compose`), while `ProtolensChain` composes through `Semigroup`, `Monoid`, and `Control.Category.Category`; a `SomePanprotoError` exception hierarchy with a domain-specific child per surface; `Hashable` / `Eq` / `Ord` mirroring the Rust derives; `State`-monad builder DSLs (`buildSchema`, `buildTheory`, `buildMigration`); a `MonadPanproto` effect layer with instances for the common `transformers` stacks and an [`effectful`](https://hackage.haskell.org/package/effectful) effect; and a self-contained delta-lens type with read-only [`optics`](https://hackage.haskell.org/package/optics-core) and [`lens`](https://hackage.haskell.org/package/lens) adaptors over the structurally-lawful projections. Cross-language CBOR wire agreement, the algebraic laws of the standard-class instances, and full round-trips of the hand-written Schema, Migration, expression, and term codecs are covered by the test suite.

## [0.54.0] - 2026-06-17

### Added

- **Read-only access to committed data at a revision** (`panproto-vcs`, `panproto-py`): `Repository::data_at(reference)` resolves a branch, tag, or commit-id prefix and returns the `DataSetObject`s recorded at that commit, never moving `HEAD`, the index, or the working tree. It is the data counterpart to reading a committed schema; unlike `checkout_with_data` (which moves `HEAD` and migrates files in place), it is a plain content-addressed store walk. The Python binding `Repository.data_at(ref)` returns one dict per data set, each carrying `schema_id`, `data` (the committed bytes), and `record_count`. So the round-trip is usable end to end from Python, the existing core `Repository::add_data` is now also exposed as `Repository.add_data(path)`, letting a caller both record and read back committed data sets without dropping to Rust. Closes #193.

### Fixed

- **`Repository.create_annotated_tag` type stub matched to the runtime** (`panproto-py`): the `_native.pyi` stub declared `(name, commit_id, message, author) -> None`, but the binding takes `(name, commit_id, author, message)` and returns the new annotated-tag object id. A caller passing by keyword per the stub silently swapped tagger and message (both are `str`, so no error surfaced); a caller wanting the returned tag id could not see it, because the stub hid the return. The stub now reads `(name, commit_id, author, message) -> str`. Closes #194.

## [0.53.0] - 2026-06-16

### Added

- **ATProto lexicon parsing and schema-to-theory extraction in the Python SDK** (`panproto-py`): `parse_atproto_lexicon(doc)` turns an ATProto lexicon document (a dict or a JSON string) into a `Schema` under the builtin `atproto` protocol, wrapping the existing Rust `web_document::atproto::parse_lexicon`. A protocol-dispatching `parse_schema_document("atproto", doc)` and the `Schema.from_atproto_lexicon` classmethod expose the same path. `theory_of(schema)` and `Schema.theory(name=None)` extract the generalized algebraic theory a schema instantiates (one sort per vertex, one unary operation per edge), preserving primitive value kinds on value-kind vertices via the existing `SortKind::Val` vocabulary. Closes #189; addresses #190 (refined scalars, per-field defaults, and reference-versus-containment ride the `Schema` constraint layer and `Edge.kind`, not the theory).

### Fixed

- **Reference-bearing ATProto lexicons validate against the builtin protocol** (`panproto-protocols`): the lexicon parser records a `ref` provenance constraint on reference properties, but the `atproto` protocol's `constraint_sorts` whitelist omitted `ref`, so any lexicon containing a `ref` (including real `app.bsky.*` lexicons) failed `Schema.validate`. The whitelist now includes `ref`.

### Changed

- **pyo3 and pythonize upgraded 0.24 → 0.29** (workspace `Cargo.toml`, `panproto-py`): closes two security advisories on the previous pyo3 0.24 (RUSTSEC-2026-0176, an out-of-bounds read in `PyList` and `PyTuple` iterators; RUSTSEC-2026-0177, a missing `Sync` bound on `PyCFunction::new_closure`), both fixed in pyo3 0.29. The 0.29 API migration is confined to `panproto-py`: `PyObject` is no longer re-exported from the prelude, so signatures use `Py<PyAny>`; `Bound::downcast` is now `Bound::cast`; and `#[pyclass]` types that derive `Clone` opt into the `FromPyObject` derive with `from_py_object`. The ten `panproto-grammars-*` crates compile unchanged.

## [0.52.1] - 2026-06-08

### Fixed

- **By-construction emit no longer drops content that rides the layout fibre** (`panproto-parse`): three rust de-novo (`forget_layout`) emit gaps and one julia gap are closed. The bar for these is AST round-trip (the emitted source re-parses to the same kind/edge multiset), not byte parity. Closes #185 and #187.
  - *Line comments no longer absorb the items that follow them.* The grammar's line-comment prefix is registered when the comment rule's body is a `CHOICE`/`SEQ` whose branches run to end of line (rust's `line_comment` is `SEQ[STRING "//", CHOICE[…]]`, where the doc-comment branch routes through an external scanner). `seq_member_is_line_rest` now recurses through `CHOICE`/`SEQ`, so the layout pass breaks the line after the comment leaf instead of collapsing the whole file onto the comment line.
  - *An opaque token tree emits verbatim.* A childless vertex carrying a whole captured token run as a non-empty bracket-pair `literal-value` (`(clippy::module_inception)`, whose `::` has no CST vertex) now emits that literal instead of rule-walking to a bare `()`. The bracket-pair leaf-shortcut carve-out is narrowed to *empty* pairs in both `emit_vertex` and `emit_aliased_child`.
  - *`blank-lines-before` is layout.* It is now an `is_layout_sort` member, so `forget_layout` strips it: the abstract surface no longer advertises a sort the emitter does not consume.
  - *The julia paren-form macro call keeps its arguments by construction.* `@trace(dist, :addr)` parses as `_closed_macrocall_expression` (aliased to `macrocall_expression`); abstract emit walked the own space-form rule and dropped the `argument_list`. `select_walk_rule` now confirms the loose subtype-closure admit count with the strict ordered `match_demand` consumption the emit walk itself uses, so it switches to the alias source that can place the children in order.
- **`panproto._native` type stub matched to the runtime** (`panproto-py`): `diff_and_classify` takes a third `protocol` argument; `ProtolensChain.instantiate` takes `(schema, protocol)`; `Instance.root` / `node_count` / `arc_count` are read-only `int` properties (not methods); `Instance.validate()` returns the list of validation errors; and `Instance.from_json` is a `@staticmethod`. Code written against the stub no longer raises `TypeError` at runtime. Closes #186.

## [0.52.0] - 2026-06-05

### Added

- **Emit coverage: `VERIFIED_EMIT_PROTOCOLS` grew from 16 to 255 of 261** (`panproto-parse`): source-code emit (`emit_pretty`) now round-trips the entire upstream `test/corpus/` of 255 vendored grammars under a strict oracle. The `emit_corpus_audit` gate requires, on *every* corpus entry, an emit fixed point (`emit(parse(emit(s))) == emit(s)`) plus preservation of the schema's vertex-kind and edge-shape multisets — rejecting *degenerate* fixed points where an emitter drops content to `""` yet satisfies the byte law trivially. A companion char-multiset corruption detector (`corpus_degeneracy_report`) catches kind-preserving token swaps/drops that the multiset gate alone misses. Grammars that ship no upstream corpus are verified against authored representative source. Highlights of the closing sweep: yaml and typst (whose emit previously *hung* — `rule_min_required_children` walked a path-stack DFS that is exponential on their single-SCC grammars; it is now a cached least-fixpoint) round-trip byte-exactly; djot, markdown, http, and vimdoc emit-dispatch defects are fixed; a byte-span container-subtree reconstruction makes any replay container whose leaf fragments tile its span byte-faithful; and root-prefix (`doc-prefix`) capture reproduces leading BOMs / line-continuations. Reaching 255 from 16 required fixing numerous emit-dispatch and spacing defects across the grammar set — CHOICE/alias-operator routing, unbounded whitespace growth around `=`/strings/markup text, indentation, external layout terminators, signed-number and float-literal splitting, and bracket pairing among them — closing #160, #182, and #183. The remaining 6 grammars are irreducible without upstream changes: comment/todotxt/wolfram (degenerate free-text grammars), less (ABI-incompatible with tree-sitter 0.26), move (the vendored grammar lacks a `let`-binding production), and test (its corpus delimiters collide with the test harness).
- **Layout calculus vocabulary** (`panproto-gat`): `LayoutRole` (the structural token roles plus an explicit `Immediate` for `IMMEDIATE_TOKEN` tokens), the pure `Adjacency` relation over role pairs (`Adjacency::between`, reproducing the historical role-pair spacing table), and `LayoutSpec` / `RuleLayout` — the theory-level, grammar-derived payload of the `Layout` enrichment that the emitter will interpret. Re-exported at the crate root.

### Fixed

- **More vendored grammars build and register** (`panproto-grammars`, `panproto-parse`): grammars whose `scanner.cc` includes C++ standard headers (Mojo, Norg, Wolfram) now compile — the per-grammar namespace wrapper hoists system includes to global scope — and grammars whose `tags.scm` or `node-types.json` use constructs outside the tree-sitter-tags/node vocabulary (AL, Erlang) now register instead of being silently dropped. Every vendored grammar is available for parse and emit.

## [0.51.0] - 2026-05-28

### Changed

- **`emit_pretty` rewritten with grammar-derived token roles** (`panproto-parse`): the entire spacing and indentation system has been replaced. Every STRING token in a grammar rule is classified by its structural role (BracketOpen, BracketClose, Separator, Keyword, Operator, Terminal) based on its position in the production body. Bracket pairs are detected per-SEQ from first/last STRING members, not from any fixed character set. The layout pass uses a role-pair spacing table with zero token-text inspection. All naming-convention checks (indent/dedent/newline/semicolon) are precomputed at Grammar construction time into set-based lookups. Line-comment prefixes are extracted from the grammar's extras rules.
- **`Production::ImmediateToken` lifted to a layout marker** (`panproto-parse`): a single `NoSpace` is emitted at the unique structural site where `IMMEDIATE_TOKEN` is declared (production walk + rule-head check in `emit_vertex`). The previous bracket-pair special case and per-SYMBOL inspection are removed. Fixes regex literals `/abc/g` emitting tight on both delimiters.
- **PREC tiebreaker unconditional** (`panproto-parse`): tree-sitter precedence ordering on yield-set ties is applied whenever multiple alts admit the cursor edge, not only when the constraint blob is empty.
- **Tarjan SCC for subtype closure** (`panproto-parse`): replaces the iteration-bounded fixpoint (max 8) with an exact O(V+E) closure on the dispatchable-only subgraph. No iteration cap, no fixpoint guessing.
- **Positional interstitial scoring** (`panproto-parse`): `pick_choice_with_cursor` now scores against the slice of interstitials from the current cursor position forward (indexed by consumed count), eliminating the cross-position contamination of the prior flat-joined blob. The `chose-alt-fingerprint` joined string remains as the fallback for by-construction schemas with no positional interstitials.
- **tree-sitter and tree-sitter-tags upgraded 0.25 → 0.26** (workspace `Cargo.toml`): API hardening (`Node::child` takes `u32` instead of `usize`; legacy `parse_utf16` / `parse_with` / `set_timeout_micros` / `version` removed). Two call sites added `u32::try_from` casts; no other source changes.

### Added

- **`TokenRole` enum** (`panproto-parse`): 6-variant structural role classification for STRING tokens, derived from grammar.json at construction time.
- **`Grammar.token_roles`**: per-rule STRING-value-to-role map.
- **`Grammar.indent_triggers`**: set of (rule, bracket) pairs that trigger indentation, derived from the presence of REPEAT between bracket delimiters.
- **`Grammar.external_indent_opens/closes/newlines/semicolons`**: precomputed sets of external layout token names.
- **`needs_space_by_role`**: role-pair spacing table replacing the old character-inspecting `needs_space_between`.
- **`ParserRegistry::emit_verification_status(protocol)`** (`panproto-parse`): public API returning `EmitVerificationStatus { Verified | Generic | Unsupported }` so downstream tooling (quivers and other transpile pipelines) can refuse `emit_pretty` on protocols whose fixed-point law has never been exercised by panproto's test suite. The `Verified` set is a sorted constant covering 16 protocols: bash, bugs, c, cpp, csharp, go, jags, java, javascript, julia, php, python, rust, scheme, stan, typescript. Re-exported at the crate root as `panproto_parse::EmitVerificationStatus`.
- **`<lang>_emit_is_fixed_point` regression tests** (`panproto-parse`): explicit `emit(parse(emit(s))) == emit(s)` assertions for every quivers transpile backend target — Python (NumPyro / Pyro / PyMC / Edward2), Stan, BUGS, JAGS, Julia (Gen / Turing), Scheme (Church), and JavaScript (WebPPL). Closes the verification gap behind issue #160.
- **`accepts_first_edge`** (`panproto-parse`): single inductive acceptance predicate over the production tree, fusing FIELD-name matching, SYMBOL subtype dispatch, ALIAS rewrite, and yield-set admission. Replaces three previously-separate ad-hoc checks (`alt_can_consume`, FIELD-name-then-yield, field-token-restriction).
- **`alt_satisfies_field_token_restrictions`** (`panproto-parse`): structural CHOICE filter that rejects an alternative whose FIELD body is `ALIAS{CHOICE[STRING...], value: V}` when the cursor's field-named edge carries a literal-value outside the allowed string set. Fixes Go `call_expression` alt 0 (the `new` / `make` constraint) being wrongly picked for arbitrary function identifiers.
- **`alt_satisfies_pre_alias_constraints`** (`panproto-parse`): alias-source discriminator using the walker-recorded `pre-alias-symbol` constraint (`tree_sitter::Node::grammar_name()`). When an alt's FIELD content is `ALIAS{SYMBOL X, named: true, value: _}`, the alt is structurally compatible iff the cursor edge's `pre-alias-symbol` matches `X`.
- **`pre-alias-symbol` constraint** (`panproto-parse`): the walker now records `tree_sitter::Node::grammar_name()` on every vertex where it differs from `kind()`. This is the only ALIAS-disambiguation signal tree-sitter 0.25 / 0.26 actually exposes.
- **Universal cassette layer** (`panproto-parse`): `common_external_default` is a name-pattern table covering every external scanner token convention from a structural audit of all 261 vendored grammars. Per-grammar cassettes layer on top via `resolve_external_token`; the default empty cassette delegates entirely to the common layer. New cassette families: HTML (HTML / Vue / Svelte / Astro / Blade / Angular / templ / heex), shell (Bash / Zsh / Fish), C raw-string (C++ / CUDA / HLSL / Arduino / C# / C), JS (JavaScript / TypeScript / TSX / QML / ReScript), indent-based (Agda / F# / F# signatures / Earthfile / Firrtl / Cooklang / Djot / Idris / Nim / PureScript / Haskell / Elm).

### Removed

- **`needs_space_between`**, **`is_punct_open`**, **`is_punct_close`**, **`is_punct_punctuation`**, **`leaves_operand_position`**, **`is_unary_prefix_operator`**, and all other character-inspecting spacing functions.
- **`inline_brace_rules`**: replaced by `token_roles` and `indent_triggers`.
- **Gated PEG matcher infrastructure** (`panproto-parse`): the production-vs-CST matcher prototype (`match_production`, `collect_match_children`, `kind_satisfies`, `MatchChild`, the `grammar` field and `new_with_grammar` constructor on `AstWalker`, the grammar threading in `LanguageParser::parse`, `consume_alt_trace`, the `alt_traces` field on `Output`, and the `pub(crate)` leak on `prec_value`) is removed. The PEG-with-PREC approximation diverges from tree-sitter's parse-table-based disambiguation; keeping the code gated invited accidental re-enablement. Properly enabling a derivation trace requires tree-sitter to expose per-CHOICE production / reduce IDs via the C API.

### Fixed

- **Go `call_expression` for arbitrary function names** (`panproto-parse`): the outer CHOICE no longer mis-selects alt 0 (which only admits `new` or `make` via an ALIAS over a CHOICE of STRINGs) for general function calls like `h(x)`.
- **C++ `_for_statement_body` initializer / condition / update fields** (`panproto-parse`): the inner CHOICE picks the declaration alt for `for (int i = 0; ...)` instead of silently dropping the initializer through the expression alt.
- **Java `modifiers` `@Override` preserved** (`panproto-parse`): the REPEAT1 inner CHOICE no longer eclipses the `_annotation` SYMBOL alt with a pure-literal `public` alt when the cursor has a `marker_annotation` edge.
- **JavaScript `regex_literal` delimiters tight** (`panproto-parse`): `/abc/g` round-trips correctly; same-text STRING delimiters with at least one IMMEDIATE_TOKEN are now treated as a bracket pair, and `regex_flags`'s IMMEDIATE_TOKEN rule body emits the required NoSpace before flag content.

## [0.50.10] - 2026-05-27

### Fixed

- **Colon spacing context-sensitive** (`panproto-parse`): `:` after a word-like token is now tight (`a: 1`, `x: int`) while `:` after a non-word token preserves space (`b : c` in ternary, `1 : 10` in slice). `::` is always tight (`x::Int`).
- **Spread/splat `...` tight with identifier** (`panproto-parse`): `...args` no longer inserts space between `...` and the identifier.
- **Optional chaining `?.` tight** (`panproto-parse`): no space around `?.` operator.
- **Stan constraint angle brackets tight** (`panproto-parse`): `<lower=0>` no longer emits as `< lower = 0 >`. Rules whose production contains `SEQ ["<", ..., ">"]` are identified as angle-bracket rules, and the Output suppresses spacing around `<`/`>` when inside them.
- **BUGS `data` keyword preserved** (`panproto-grammars`): regenerated BUGS grammar with `optional(choice("model", "data"))` as the block keyword, fixing bare `{` blocks.
- **JAGS `I(...)` interval censoring preserved** (`panproto-grammars`): regenerated JAGS grammar with `choice("T", "I")` as the truncation keyword, fixing dropped censoring clauses.

### Added

- **`Grammar.angle_bracket_rules`** (`panproto-parse`): set of rule names whose `<`/`>` tokens are bracket delimiters, identified structurally from grammar.json.
- **5 new regression tests**: BUGS data keyword, JAGS censoring, Stan constraint brackets, JS spread spacing.

## [0.50.9] - 2026-05-26

### Fixed

- **Line comments no longer swallow the following line** (`panproto-parse`): the `layout` pass inserts a newline after any `Lit` token starting with a line-comment prefix (`//` or `#`). Fixes comment-swallowing across JavaScript, Python, Stan, Julia, BUGS, and JAGS grammars.
- **Yield-set cache restricted to hidden/supertype rules** (`panproto-parse`): `compute_yield_sets` no longer caches concrete named rules. Fixes JS object literal pairs, spread elements, new-expression arguments, Python single-kwarg calls, and Stan function parameters being dropped.
- **`has_repeat` recursion for inline-brace identification** (`panproto-parse`): the structural check for REPEAT/REPEAT1 between `{` and `}` now recurses into CHOICE and SEQ wrappers, preventing JS `object` from being misclassified as an interpolation-like inline-brace rule.

### Added

- **14 comprehensive regression tests** (`panproto-parse`): covering comment-newline preservation, object literals, arrow functions, spread elements, new-expression arguments, single-kwarg calls, string literals, anonymous functions, and Stan/BUGS/JAGS-specific constructs across 5 grammars.

## [0.50.8] - 2026-05-26

### Fixed

- **Line comments no longer swallow the following line** (`panproto-parse`): the `layout` pass now inserts a newline after any `Lit` token that starts with a line-comment prefix (`//` or `#`). This is derived from the grammar structure (line comments are `TOKEN(SEQ [STRING prefix, PATTERN ".*"])` where `.*` matches to end-of-line but not the newline itself). Fixes comment-swallowing across JavaScript, Python, Stan, Julia, BUGS, and JAGS grammars.
- **Yield-set cache restricted to hidden/supertype rules** (`panproto-parse`): `compute_yield_sets` no longer caches concrete named rules. The Yield of a concrete SYMBOL `S` is always `{S}` (the symbol name IS the vertex kind); caching the internal yield of S's rule body caused CHOICE dispatch to miss children whose kind is the symbol itself (e.g. `pair` children of JS `object` were not found because the cache stored `pair`'s internal yield `{computed_property_name, string, ...}` instead of `{"pair"}`). Fixes JS object literal pairs dropped, JS spread elements dropped, JS new-expression arguments dropped, Python single-kwarg calls dropped, and Stan function parameters dropped.
- **`has_repeat` recursion for inline-brace identification** (`panproto-parse`): the check for REPEAT/REPEAT1 between `{` and `}` now recurses into CHOICE and SEQ wrappers. Previously, JS `object` (whose REPEAT is nested inside a CHOICE) was incorrectly classified as inline-brace, suppressing block indentation for object literals.

### Added

- **14 comprehensive regression tests** (`panproto-parse`): covering comment-newline, object literals, arrow functions, spread, new-expression, single-kwarg calls, string literals, anonymous functions, and Stan/BUGS/JAGS-specific constructs.

## [0.50.7] - 2026-05-26

### Fixed

- **node-types.json subtype augmentation restricted to unmatched children** (`panproto-parse`): `augment_subtypes_from_node_types` now skips child kinds that already satisfy at least one symbol in the parent's grammar rule. The v0.50.5 augmentation was too aggressive: it added every node-types.json child kind as satisfying every referenced symbol, causing CHOICE dispatch to pick wrong alternatives (Python assignment `x = 1` emitted as `x: = 1` because `integer` was added as satisfying `type` in addition to `_right_hand_side`; Stan `vector[N] y` emitted as `vector N[] y`; Scheme produced empty output).
- **Removed issue-number references from doc comments and test assertions** (`panproto-parse`, `panproto-lens-dsl`).

## [0.50.6] - 2026-05-26

### Fixed

- **`emit_pretty` CHOICE dispatch no longer emits phantom tokens** (`panproto-parse`): the BLANK-over-non-BLANK fallback (introduced in v0.50.5) incorrectly selected non-BLANK alternatives for CHOICE positions whose children belonged to later SEQ members, inserting phantom tokens like `async` and `->` in Python function definitions. Removed the fallback: BLANK is now always selected when no dispatch tier matches, which is categorically correct (children that don't match this CHOICE position belong to a later SEQ member). The node-types.json augmentation from v0.50.5 handles the Julia macrocall case that originally motivated the fallback.
- **`emit_pretty` inline-brace suppression now covers `line_break_after`** (`panproto-parse`): `{` and `}` tokens inside inline-brace rules (interpolation, template substitution) no longer trigger `LineBreak` from the `line_break_after` policy vector. Previously, even with `suppress_brace_indent` active, the `line_break_after` check still fired as a fallback. Fixes Python f-string interpolation (#161) and JavaScript template literal interpolation (#163).
- **`is_punct_open` recognizes compound opening tokens** (`panproto-parse`): tokens ending with `{`, `(`, or `[` (e.g. `${`, `#{`) are now treated as opening punctuation, suppressing the inter-token space. Fixes `${ name}` → `${name}` in JS template literals.
- **`emit_pretty` indented-form CHOICE preference fires before standard dispatch** (`panproto-parse`): when multiple CHOICE alternatives yield the same child kind (e.g. Python `_suite` where all three alternatives produce `block`), the alternative containing `_indent` is selected before the standard direct-match pass that would pick the first grammar-order match. Selects the indented block form for Python function bodies instead of the inline `;`-separated form.

## [0.50.5] - 2026-05-26

### Fixed

- **`emit_pretty` integrates node-types.json to close grammar/parser divergence** (`panproto-parse`): the Grammar struct now accepts node-types.json alongside grammar.json. A new `build_node_type_children` function parses the authoritative child-kind data, and `augment_subtypes_from_node_types` adds parser-produced child kinds to the subtype closure. This fixes Julia macrocall short-form `@trace(args)` (#153), Julia multi-arg macrocall `@info "msg" foo bar` (#167), and Julia function body re-emit corruption (#164).
- **`emit_pretty` reverts unconsumed-children fallback** (`panproto-parse`): the v0.50.3 fallback that emitted remaining children after the grammar rule walk caused JavaScript object literal contents to appear outside braces (#159), Stan array sizes to split to the next line (#165), and Stan function parameters to migrate outside parentheses (#166). Removed; the CHOICE fallback now correctly selects non-BLANK when structural children remain.
- **`emit_pretty` resolves external scanner tokens via precomputed alias map** (`panproto-parse`): `build_external_alias_map` walks all grammar rule bodies at construction time, collecting anonymous ALIAS values for external tokens. The SYMBOL handler checks this map before falling through to name-matching heuristics, fixing JavaScript ternary `?` (#162) and generalizing to all external-token ALIASes.
- **`emit_pretty` prefers indented CHOICE alternative for by-construction schemas** (`panproto-parse`): when no dispatch tier uniquely identifies a CHOICE alternative and the alternatives include an indented form (containing `_indent` SYMBOL), prefer it over the inline form. The indented form always produces valid output; the inline form (Python `;`-separated statements) is a source-level abbreviation requiring parse-time context. Fixes Julia function body block structure (#164).
- **`emit_pretty` suppresses brace indentation in interpolation rules** (`panproto-parse`): `identify_inline_brace_rules` structurally identifies rules whose `{`/`}` tokens are inline delimiters (no REPEAT between them) rather than block scopes. The Output struct carries a `suppress_brace_indent` flag threaded through `emit_vertex`.

### Added

- **`Grammar.node_type_children`** (`panproto-parse`): maps parent kind to the set of all named child kinds from node-types.json.
- **`Grammar.external_alias_map`** (`panproto-parse`): maps external scanner symbol names to their anonymous ALIAS value strings.
- **`Grammar.inline_brace_rules`** (`panproto-parse`): set of rule names whose `{`/`}` tokens should not trigger block indentation.
- **`Grammar::from_bytes_with_node_types`** (`panproto-parse`): constructor accepting both grammar.json and node-types.json bytes.
- **9 regression tests** (`panproto-parse`): one per issue #159-#167, covering JS objects, Python function bodies, f-strings, JS ternaries, template literals, Julia functions/macrocalls, and Stan declarations.

## [0.50.4] - 2026-05-26

### Fixed

- **`emit_pretty` CHOICE dispatch replaced with subtype-relation preimage** (`panproto-parse`): the cursor-driven CHOICE alternative picker used heuristic scoring (literal fingerprints, Yield-set computation) that failed when multiple alternatives transitively reached the same vertex kind. Replaced with a three-pass subtype-based dispatch: (1) direct name match, (2) supertype match via the precomputed `subtypes[target_kind]` set, (3) Yield-set fallback. The subtype relation is the formally correct preimage: `subtypes[K]` contains exactly the symbol names S such that a vertex of kind K can appear where the grammar says `SYMBOL S`. Added Yield-set precomputation (`Grammar.yield_sets`) for the fallback pass. Fixes Python `with X as Y:` dropping the alias identifier (#157), and subsumes prior CHOICE dispatch fixes (#150).
- **`emit_pretty` field context no longer leaks through ALIAS dispatch** (`panproto-parse`): `emit_aliased_child` now clears the enclosing FIELD context before walking the aliased child's production. Previously, a `FIELD("alias")` containing an `ALIAS { SYMBOL "expression" }` caused the inner SYMBOL handler to attempt field-based edge lookup (by name "alias") instead of symbol-based dispatch, silently dropping the child's content.
- **`take_symbol_match` prefers non-field edges** (`panproto-parse`): when consuming a cursor edge outside a FIELD context, the symbol matcher now prefers `child_of` edges over field-named edges. This prevents a SYMBOL from accidentally consuming a field-named edge intended for a later FIELD handler in the same SEQ.

### Added

- **Yield-set precomputation on `Grammar`** (`panproto-parse`): `Grammar.yield_sets` maps each rule name to the set of vertex kinds that can appear as the first named child when that rule's production is taken. Defined inductively with `Yield(SEQ)` descending only into the first named-child-producing member (skipping leading STRING/PATTERN terminals). Used as a fallback in CHOICE dispatch when the subtype relation alone is insufficient.
- **Nickel contract regression tests** (`panproto-lens-dsl`): 6 tests exercising the full Nickel evaluation path through `lens.ncl` (rename, add, remove, rules, multiple combinators, exhaustive export accessibility).
- **Python `with X as Y:` regression tests** (`panproto-parse`): 2 tests verifying the alias identifier survives `emit_pretty` and round-trips without ERROR nodes.

## [0.50.3] - 2026-05-25

### Fixed

- **Nickel contract modules no longer infinite-recurse on import** (`panproto-lens-dsl`, `panproto-theory-dsl`): the 0.50.1 fix (#151) replaced field-shorthand exports with explicit assignments (`Lens = Lens,`), but Nickel's record scope shadows enclosing `let`-bindings when field and variable names coincide, turning every export into a self-referential binding. Restructured both `lens.ncl` and `theory.ncl` as flat records with all definitions inline (no `let`-chain), and prefixed combinator parameters with `_` to avoid field-name collisions (`fun _old _new => { old = _old, new = _new }`). Six new Nickel evaluation regression tests cover individual combinators, rule patterns, and exhaustive export accessibility. Closes #154.
- **`emit_pretty` renders Julia parenthesised macrocall arguments** (`panproto-parse`): the grammar rule for `macrocall_expression` references `macro_argument_list`, but tree-sitter's parser also produces `argument_list` children for the short form `@trace(args)`. Unconsumed structural children left after the grammar-rule walk are now emitted as a fallback, so the argument list survives even when no grammar alternative matches it. Closes #153.
- **`emit_pretty` inserts semicolons for JavaScript ASI** (`panproto-parse`): the `_automatic_semicolon` external scanner token was falling through the heuristic fallback silently. External hidden rules whose name contains "semicolon" now emit `;`. Two regression tests verify semicolon presence and error-free re-parse. Closes #155.
- **`@` sigil no longer inserts a spurious space before the macro name** (`panproto-parse`): added `@` and `#` to the set of prefix-sigil punctuation that suppresses trailing whitespace in the layout policy.

## [0.50.2] - 2026-05-25

### Fixed

- **Bundled Nickel contract exports are now reachable from downstream specs** (`panproto-lens-dsl`, `panproto-theory-dsl`): the `lens.ncl` and `theory.ncl` export records used field-shorthand syntax (`Lens,` meaning `Lens = Lens,`), which the embedded Nickel 2.0 evaluator does not resolve against enclosing `let`-bindings at the import boundary. Every `L.<name>` access from a user spec reported "missing definition". All exports are now explicit assignments (`Lens = Lens,`). Closes #151.
- **`emit_pretty` renders Julia `macrocall_expression` bodies** (`panproto-parse`): the CHOICE dispatcher in the grammar-walker picked `BLANK` over `macro_argument_list` because an ALIAS wrapping `_qualified_macro_identifier` (a hidden rule whose SEQ contains `macro_identifier`) shadowed the direct `SYMBOL "macro_identifier"` alternative via the subtype closure. The cursor-driven CHOICE picker now runs a direct-match pass before the subtype-closure pass, and `kind_satisfies_symbol` gained a reverse subtype lookup. Output was `.@ <macro>\n`; now correctly emits `@model function model(y) ... end`. Closes #150.

## [0.50.1] - 2026-05-23

### Fixed

- **Bundled `lens.ncl` contract parses in the embedded Nickel evaluator** (`panproto-lens-dsl`): the contract used `match` as a record field name and `default` as a function parameter, both reserved words in the bundled Nickel 2.0 evaluator. Renamed to `pattern` and `fallback` respectively. Serde aliases (`alias = "match"`, `alias = "default"`) preserve backward compatibility with existing JSON and YAML lens specs. Closes #144.
- **`_native.pyi` stubs match runtime API** (`panproto-py`): `Vertex` now exposes `id`, `kind`, `nsid` (was `name`, `kind`); `Edge` exposes `src`, `tgt`, `kind`, `name` (was `name`, `source`, `target`); `Constraint` exposes `sort`, `value` (was `name`, `to_dict`); `Schema.vertices`, `.edges`, `.vertex_count`, `.edge_count`, `.protocol` are properties not methods; `Protocol.from_theories` accepts `Theory | str` for `schema_theory` and `instance_theory`; all `object` type hints replaced with a recursive `JsonValue` type alias; all catch-all `*args` signatures replaced with concrete parameter types. Closes #147.

### Added

- **`get_builtin_protocol` resolves tree-sitter grammar protocols** (`panproto-py`): `get_builtin_protocol("stan")`, `get_builtin_protocol("python")`, etc. now succeed by falling back to the grammar registry before raising `KeyError`. `list_builtin_protocols()` includes grammar names alongside the 50 semantic protocols. Closes #145.
- **`hom_search` and `cascade` exposed in Python bindings** (`panproto-py`): new `TheoryMorphism`, `SchemaMorphism`, `FoundMorphism` classes and `find_morphisms`, `find_best_morphism`, `induce_schema_morphism`, `induce_migration_from_theory` functions enable programmatic cross-protocol morphism discovery and theory-induced migration from Python. Closes #146.
- **`lang-bugs` and `lang-jags` tree-sitter grammars** (`panproto-grammars`): panproto-hosted grammars for the BUGS (WinBUGS/OpenBUGS) and JAGS probabilistic programming languages. Both support stochastic (`~`) and deterministic (`<-`) relations, `for` loops, distribution calls with truncation, indexed variables, and arithmetic expressions. JAGS additionally supports `var` declarations and `data { }` blocks. Included in `group-all` (261 languages total). Closes #148.

## [0.50.0] - 2026-05-22

### Changed

- **`IdGenerator::named_id` and `IdGenerator::field_id` now take `&mut self`** (`panproto-parse::id_scheme`): both methods record per-scope occurrence counts so repeated calls disambiguate (see Fixed below), which requires interior mutation. Downstream callers were `&mut`-context already (the walker holds `&mut IdGenerator`), so call sites compile unchanged.
- **`IdGenerator::push_named_scope` now returns the disambiguated leaf `String`** (`panproto-parse::id_scheme`): callers that need both a vertex ID and a matching scope-stack frame can use the returned leaf instead of re-deriving the suffix. The walker uses the new `record_name` / `push_recorded_scope` split to record the occurrence once and reuse the leaf at two sites. `push_named_scope` itself remains available as a convenience for callers that only need the scope push.

### Added

- **`IdGenerator::record_name(&mut self, name) -> String`** (`panproto-parse::id_scheme`): records an occurrence of `name` in the current scope frame and returns the disambiguated leaf (`name`, `name#1`, `name#2`, …). Public so callers (like the walker) can compose a vertex ID and a matching scope-stack push without double-recording.
- **`IdGenerator::push_recorded_scope(&mut self, leaf: String)`** (`panproto-parse::id_scheme`): pushes a scope frame using an already-disambiguated leaf, skipping the record step.

### Fixed

- **`IdGenerator` disambiguates repeated names at the same scope, unblocking `@typing.overload` and every grammar with same-named siblings** (`panproto-parse::id_scheme`): tree-sitter's tag set for Python (and for several other languages) tags each `function_definition` as `@definition.function` regardless of name, so two `def foo` declarations at the same scope produced the same vertex ID `{scope}::foo` and `SchemaBuilder::vertex` rejected the second with `duplicate vertex id`. Any Python file using `@typing.overload` was unparseable. The generator now records, per scope frame, how many times each name has been requested, and suffixes repeats `#1`, `#2`, …. The disambiguated leaf is also threaded into the scope stack via the new `record_name` / `push_recorded_scope` pair so descendants of the second `foo` are prefixed `foo#1::…`, never re-colliding with descendants of the first. Field IDs share the same disambiguation: a second `field_id(parent, "args")` on the same parent yields `parent.args#1`, fixing a silent collision under tree-sitter `field('xs', repeat($.X))` shapes (`commaSep1`, `commaSep`, etc.). `pop_scope` at the root now debug-asserts so walker push/pop imbalance surfaces immediately instead of as mis-shaped IDs. Walker (`crates/panproto-parse/src/walker.rs`) is updated at the single vertex-ID-derivation site to thread the disambiguated leaf into both the vertex ID and the matching scope-stack frame. Regression tests at `crates/panproto-parse/tests/python_overload_duplicate_id.rs` (issue #134 reproducer) and seven new unit tests in `crates/panproto-parse/src/id_scheme.rs` cover the duplicate-name, scope-disambiguation, field-collision, anonymous-vs-named interleaving, sibling-scope independence, and pop-balance defects. Closes #134.

## [0.49.6] - 2026-05-22

### Added

- **`.musicxml` extension dispatches to the `xml` protocol** (`grammars.toml`): per the W3C Music Notation Community Group's MusicXML 4.0 specification (https://www.w3.org/2021/06/musicxml40/tutorial/file-extensions/), a `.musicxml` file is plain XML; the schema (DTD / XSD) constrains element names and attribute values, not the lexical form. The existing `tree-sitter-xml` grammar therefore parses MusicXML scores without modification — no new vendored grammar is required. The extension dispatcher now routes `.musicxml` files to the `xml` protocol so `parse_with_protocol("xml", bytes, "score.musicxml")` and `parse_file("score.musicxml", bytes)` both succeed. The compressed `.mxl` container (ZIP-of-MusicXML) is not registered: it requires unzipping before any parser can see the XML payload, which is a transformation outside the grammar surface. Regression test at `crates/panproto-parse/tests/musicxml_parse.rs` parses a representative MusicXML 4.0 "hello world" score (partwise score, one part, one measure, one C4 whole note) end-to-end and confirms structural recovery. Partially addresses #112.

## [0.49.5] - 2026-05-22

### Fixed

- **`emit_pretty` splits trailing newlines off `literal-value` Lits and routes them through `Token::LineBreak`** (`panproto-parse::emit_pretty`): a vertex carrying a `literal-value` constraint and no structural children emits its captured text directly via the leaf shortcut. For grammars whose terminal-like rules absorb a trailing newline (e.g. ABC's `reference_number_line` matching `"X:1\n"`), the captured value contained an embedded `\n`. Pushing it as `Lit("X:1\n")` left the newline character in the output but the layout pass then ran `needs_space_between("X:1\n", "T:")` against the next Lit; the fall-through "keep operator runs apart" rule inserted the policy separator at column 0 of the new line, rendering `X:1\n T:` instead of `X:1\nT:`. `Output::token` now strips trailing newlines off any non-pure-newline Lit value, emits the trimmed prefix as a `Lit`, and pushes a `LineBreak` for the newline tail. Layout treats `LineBreak` as a line-state reset, so the next Lit starts at column 0 with no intervening separator. Regression test at `crates/panproto-parse/tests/emit_pretty_literal_trailing_newline.rs`. Improves #113 (ABC tune-header leading-space piece).

## [0.49.4] - 2026-05-22

### Fixed

- **`emit_pretty` no longer emits phantom trailing punctuation when a CHOICE includes BLANK and the cursor is exhausted** (`panproto-parse::emit_pretty`): the `chose-alt-fingerprint` constraint is built per vertex from the concatenated interstitial fragments. A rule like QVR's `sample_step`, whose body ends in `..., REPEAT(SEQ(",", arg)), CHOICE(",", BLANK)` (the canonical `commaSep1`-with-optional-trailing-comma shape), deposits one `","` into the fingerprint blob per arg-separator gap. The trailing CHOICE then scored `","` over BLANK (one literal match per recorded separator vs zero), and `f(1.0, 2.0, 3.0)` rendered as `f(1.0, 2.0, 3.0,)` with a phantom trailing comma. The literal-blob discriminator is intrinsically position-blind across multiple positional CHOICEs at the same vertex, so the cursor-exhaustion gate now fires first: when no unconsumed edges remain AND `BLANK` is one of the alternatives, the only categorically correct alt is `BLANK`, regardless of what literal tokens appear earlier in the vertex's interstitials. Regression test at `crates/panproto-parse/tests/emit_pretty_trailing_punctuation.rs`. Improves #113 (commaSep1 trailing comma family across grammars).

## [0.49.3] - 2026-05-22

### Fixed

- **`emit_pretty` keeps sibling REPEAT iterations tight when the separator slot is empty** (`panproto-parse::emit_pretty`): when a `REPEAT` / `REPEAT1` body is `SEQ(SEP, BODY)` and `SEP` is `CHOICE` containing `BLANK` (or an `OPTIONAL`), the categorical reading is that the source-level separator between two iterations is syntactically optional. When the runtime alternative chosen for the separator slot emits zero content tokens (BLANK picked), the source had no separator between this iteration and the previous one; the layout pass must not inject the policy separator either. Pre-fix, ABC's `beam` rule (`SEQ(_nte_or_chrd, REPEAT1(SEQ(CHOICE(BEAM_SEPARATOR, BLANK), _nte_or_chrd)))`) rendered consecutive note letters as `C D E F` (source was `CDEF`) because layout had no signal that the iteration boundary carried no source-level separator. The REPEAT walker now structurally recognises separator-leading SEQ bodies, emits the separator slot first while observing whether any content token was produced, and pushes a `Token::NoSpace` marker before the remaining SEQ members when the slot was empty. The layout pass consumes the marker and suppresses the inter-Lit separator for that pair. Bodies whose first SEQ member is not a `CHOICE`-with-`BLANK` / `OPTIONAL` (e.g. `commaSep1`'s `SEQ(STRING ",", SYMBOL)`) take the original code path unchanged. Regression test at `crates/panproto-parse/tests/emit_pretty_tight_repeat.rs`. Improves #113 (ABC beam piece).

## [0.49.2] - 2026-05-22

### Fixed

- **`panproto-jit` match codegen emits valid IR when arms include an irrefutable pattern** (`panproto-jit::codegen`): `compile_match` built the literal-arm `then` / `else` chain and then unconditionally emitted a fall-through default block. The wildcard / var-pattern handler at the same level already terminated its block with a branch to `merge_bb` and broke out of the loop, but the fall-through code ran anyway. It pushed a duplicate `(0, wildcard_block)` entry into the phi node and emitted a second unconditional branch on a basic block that already had a terminator. The IR was malformed and the phi resolved to the wildcard arm's value even when an earlier literal arm matched the scrutinee. `match 2 { 1 => 10, 2 => 20, _ => 0 }` returned `0` instead of `20`. The default block is now gated on whether any irrefutable arm has already terminated the cascade; when one has, the synthetic default is skipped entirely. Closes #115.

## [0.49.1] - 2026-05-22

### Fixed

- **`emit_pretty` routes newline-valued grammar STRINGs through the layout `LineBreak` channel** (`panproto-parse::emit_pretty`): `Output::token` pushed every grammar STRING as a `Lit`. A `STRING "\n"` literal (abc's `_NL`, plus every grammar that uses a literal newline as a statement terminator) left the newline character in the output but the layout pass's `needs_space_between` then inserted the configured separator between the newline `Lit` and the following token, producing leading spaces on every line after the first and trailing spaces before every newline. `Output::token` now recognises `"\n"` / `"\r"` / `"\r\n"` and pushes `Token::LineBreak` directly so layout treats it as a line-state reset rather than a normal Lit pair. Regression test at `crates/panproto-parse/tests/emit_pretty_newline_string.rs` parses an abc header and asserts no trailing space precedes any newline. Partially closes #113 (abc whitespace piece).

## [0.49.0] - 2026-05-22

### Changed

- **`Grammar` is now `#[non_exhaustive]`** (`panproto-parse::emit_pretty`): the struct gained a new `extras` field (see Fixed below). Marking it `#[non_exhaustive]` prevents external struct-literal construction so further fields can be added without a semver break. Construct `Grammar` via `Grammar::from_bytes` instead.

### Fixed

- **`emit_pretty` drains tree-sitter `extras` children as a side channel** (`panproto-parse::emit_pretty`): extras (typically `line_comment` / `block_comment`) live outside the production grammar — tree-sitter skips them at parse time and records them as children of the surrounding vertex — but the rule walker had no way to reconcile them against the cursor. Cursor-driven CHOICE dispatch returned `None` for an extras-kind child and the surrounding `REPEAT` loop terminated after zero iterations with no progress, producing **empty output** for supercollider's `source_file` (and any other grammar whose top-level rule's REPEAT body was reached with a leading comment). `Grammar` now records the set of named-symbol / aliased extras kinds; `emit_production` drains leading extras edges from the cursor at every entry, and `emit_vertex` drains trailing extras after the rule walk completes. Each drained extra is emitted via `emit_vertex`, preserving its content. Regression test at `crates/panproto-parse/tests/emit_pretty_extras.rs` parses a supercollider source with a leading `//` comment and asserts both the comment and the `Pdef` / `Pbind` calls survive. Partially closes #113 (supercollider piece).

## [0.48.8] - 2026-05-21

### Fixed

- **`emit_pretty` renders newline-shaped PATTERN terminals as newlines, not the bare `_` placeholder** (`panproto-parse::emit_pretty`): csound's `_new_line` is `TOKEN(PATTERN "\r?\n")`. The pattern fell through `placeholder_for_pattern`'s final `else` arm (no `[0-9]` / `[a-zA-Z_]` / `"` / `'` markers) and returned the bare `"_"` sentinel. csound's `instrument_definition` SEQ has a REPEAT of `_statement` requiring `_new_line` between siblings, so the placeholder injected unparseable `_` characters between every pair of structural siblings (`endin _ </CsInstruments> _ </CsoundSynthesizer>`). The PATTERN handler now recognises `\r?\n`-shaped patterns and routes them through `Output::newline()` (`Token::LineBreak`), and recognises generic whitespace patterns (`\s+`, `[ \t]+`, ` *`) and drops them so the layout pass's policy separator inserts the actual spacing. Other patterns still fall through to the heuristic placeholder. Regression test at `crates/panproto-parse/tests/emit_pretty_csound_newline.rs`. Partially closes #113 (csound piece).

## [0.48.7] - 2026-05-21

### Fixed

- **`emit_pretty` recognises named-ALIAS children when picking CHOICE alternatives** (`panproto-parse::emit_pretty`): `referenced_symbols` previously walked into an `ALIAS { content, value, named }` production's content and discarded the alias `value`. A named ALIAS introduces a child vertex whose kind is the alias `value` (e.g. lilypond's `ALIAS { content: STRING "=", value: "punctuation", named: true }` introduces a `punctuation` child), but cursor-driven dispatch matched alts only on the inner SYMBOLs. The lilypond `named_context` rule's third arm (`SEQ(ALIAS_punctuation, CHOICE(symbol, string))`) was invisible to dispatch, so `\new Voice = "kick" { ... }` dropped the `=` punctuation and `"kick"` string in `emit_pretty`, losing the voice label. `referenced_symbols` now yields the alias `value` for named aliases, so dispatch can resolve the alt that produces a named-aliased child kind. Regression test at `crates/panproto-parse/tests/emit_pretty_lilypond_alias.rs`. Partially closes #113 (lilypond piece).

## [0.48.6] - 2026-05-21

### Fixed

- **`emit_pretty` keeps tight unary prefixes glued to their operand** (`panproto-parse::emit_pretty`): the layout pass inserted the default separator between any `-` / `+` / `!` / `~` and the following token, turning `f(-1.0)` into `f(- 1.0)`. The split parses as a different AST (unary minus applied to a positive literal) rather than as a single signed literal, so the round-trip was semantically lossy on every grammar with a `signed_number` (or analogous) production. The pass now tracks an `expecting_operand` flag along the token stream: at start of stream / line, after open punctuation, after a separator (`,` / `;`), and after another operator-run, the cursor is in operand position. When the previous token was emitted in operand position and is one of `-` / `+` / `!` / `~`, it is recognised as a tight unary prefix and glues to the following operand. Binary `a - b` keeps its spaces because the cursor was not in operand position when `-` was emitted. Regression test at `crates/panproto-parse/tests/emit_pretty_signed_number.rs`. Closes #111.

## [0.48.5] - 2026-05-21

### Fixed

- **`panproto-parse`'s `grammars` feature no longer drags `panproto-grammars/default` into every consumer** (`crates/panproto-parse/Cargo.toml`): the `grammars` feature previously activated `panproto-grammars/default`, which in turn activated `panproto-grammars/group-core` (11 mainstream-programming grammars: bash, c, cpp, csharp, go, java, javascript, php, python, rust, typescript). Every `group-*` feature on panproto-parse inherits `grammars`, so a downstream consumer asking for `group-music + lang-haskell` ended up with 20 grammars instead of 9, including 11 unused for music applications. Downstream `default-features = false` on a direct `panproto-grammars` dep could not opt out, because panproto-parse's transitive activation re-enabled the default. Now `grammars` only activates the optional dep; group activation goes through the explicit `panproto-grammars/group-*` feature; the workspace `panproto-grammars` dependency is declared with `default-features = false` so the inner crate's own `default = ["group-core"]` does not leak transitively either. The user-visible behavior of `cargo add panproto-parse` is unchanged: `default` on panproto-parse now lists `group-core` directly, so top-level consumers still get the 11 mainstream grammars without configuration. Consumers who previously wrote `default-features = false, features = ["grammars"]` and expected `group-core` must switch to `features = ["group-core"]`. Closes #114.

## [0.48.4] - 2026-05-19

### Changed

- **Vendored QVR tree-sitter grammar upgraded to Quivers 0.11.1** (`grammars/qvr/`): re-vendored from FACTSlab/quivers `v0.11.1` (revision `b0940df1`), replacing the previous `76a8805d` snapshot. The 0.11 release homogenized the DSL surface around a single `KIND NAME : SIGNATURE [k = v, ...] [~ INIT] [BODY]` skeleton: per-role morphism keywords (`latent`, `kernel`, `observed`, `embed`, `discretize`) collapse into `morphism X : ... [role=ROLE]`; `algebra` becomes `composition NAME as algebra` (so `algebra_decl` is gone, `composition_decl` takes its place); `type` / `space` / type-alias declarations fold into the unified `object` surface (`type_alias_decl` and `space_decl` are gone, `object_decl` carries them); `kernel f : A -> B ~ Family [...]` becomes `morphism f : A -> B [role=kernel] ~ Family(...)` (so `kernel_decl` is gone); the program-step binder `x <- f` is now `sample x <- f` (`bind_step` becomes `sample_step`). The vendored grammar also gains a `scanner.c` for the new `_indent` / `_dedent` external tokens — picked up automatically by `panproto-grammars/build.rs`, no wiring change needed. Refreshed the `qvr_hmm_parses_with_expected_blocks` and `qvr_program_block_parses` integration tests to the 0.11 surface.

## [0.48.3] - 2026-05-19

### Fixed

- **`emit_pretty` renders every iteration of repetition inside a `FIELD(...)` body, not only top-level `FIELD(REPEAT(...))`** (`panproto-parse::emit_pretty`): the previous fix peeled `Repeat` / `Repeat1` off the top of a FIELD's content, which handled `field('steps', repeat($._program_step))` but not `field('args', commaSep1($._draw_arg))` — the latter expands to `FIELD(SEQ(SYMBOL X, REPEAT(SEQ(',', SYMBOL X))))` and the SEQ wrapper defeated the peel. The inner REPEAT then walked the first consumed child's cursor (which has no sibling field edges) and broke after one iteration. The fix moves the field hint into a thread-local `EMIT_FIELD_CONTEXT` that the SYMBOL handler consults: when set, every SYMBOL under the FIELD body — at any depth, under SEQ / CHOICE / REPEAT / REPEAT1 / OPTIONAL / ALIAS — consumes successive edges via `take_field(name)` on the outer cursor instead of `take_symbol_match` against the inner. The new path subsumes the prior carve-outs for FIELD(REPEAT(...)), FIELD(REPEAT1(...)), and bare FIELD(SYMBOL ...). FIELD content with no SYMBOL (e.g. `field('op', '+')`, `field('op', CHOICE(STRING, STRING))`) still emits its literals: STRING handlers ignore the context.
- **`emit_pretty` opens and closes indent scopes for grammars with `_indent` / `_dedent` external scanner tokens** (`panproto-parse::emit_pretty`): the SYMBOL handler's external-token fallback covered `*newline*` / `*line_ending*` / `*_or_eof` but ignored `*_indent` and `*_dedent`. For indent-based grammars (Python, YAML, and QVR-flavoured indent grammars whose vendored generation surfaces `_indent`/`_dedent` as externals) the rendered output was structurally well-formed but unparseable: the parser expected an INDENT token after `:` and got a content character at column 0. The fallback now dispatches `_indent` / `*_indent` to `Output::indent_open` and `_dedent` / `*_dedent` to `Output::indent_close`, reusing the existing indent-depth machinery the format policy already drives for `{` / `}` token pairs. Other external tokens still fall through silently.
- Regression test `crates/panproto-parse/tests/emit_pretty_field_repeat.rs::emit_pretty_preserves_every_arg_in_field_commasep1` parses `f(1.0, 2.0, 3.0)` through a `commaSep1` field and asserts every arg survives `emit_pretty`. The pre-existing `emit_pretty_preserves_every_step_in_field_repeat` continues to cover the simpler `FIELD(REPEAT(SYMBOL))` shape.

## [0.48.2] - 2026-05-18

### Fixed

- **`emit_pretty` renders every iteration of `FIELD(REPEAT(...))` productions** (`panproto-parse::emit_pretty`): the `Field { content: Repeat(...) }` arm called `take_field(name)` once and walked the REPEAT against the consumed child's cursor, so every field-named sibling beyond the first was silently dropped. Tree-sitter's `field('xs', repeat($.X))` produces one field-named edge per match on the parent vertex, so the repetition lives at the parent level. QVR's `program_decl` body — `field('steps', repeat($._program_step))` — was the reported reproducer: a 3-line program with two `sample` steps emitted only one. The fix peels `Repeat` / `Repeat1` off the field content and drives the iteration from the outer cursor, taking one `take_field(name)` edge per iteration; the REPEAT1-minimum fallback still fires when the field is required-but-empty. Regression test: `crates/panproto-parse/tests/emit_pretty_field_repeat.rs::emit_pretty_preserves_every_step_in_field_repeat`. Closes #106.

## [0.48.1] - 2026-05-18

### Fixed

- **`pretty_with_protocol` preserves edge order on abstract schemas under `REPEAT(CHOICE(...))`** (`panproto-parse::emit_pretty`): two correlated bugs caused interleaved children of the same parent to re-fuse by kind when rendered through `decorate` / `pretty_with_protocol`. A lilypond `expression_block` whose children were `[symbol, punctuation, unsigned_integer, symbol, punctuation, unsigned_integer]` would emit `''c d 4 4` (all punctuation, then all symbols, then all integers) and re-parse as a single super-octave c followed by bare letters and bare integers. With the fix, the same abstract schema emits in insertion order, and `decorate` is a section of `forget_layout` at the granularity of `edge_multiset`, not just `kind_multiset`. Two changes:
  - `emit_pretty::children_for` now walks the precomputed `schema.outgoing` index (insertion-ordered by `SchemaBuilder` via `SmallVec` append) rather than the unordered `schema.edges` `HashMap`. The previous implementation sorted edges lexicographically by `(kind, target id)` when no explicit `orderings` entries existed, which abstract schemas never set. Explicit `orderings` still override.
  - `emit_pretty::pick_choice_with_cursor` cursor-driven dispatch now picks the alt whose SYMBOL set covers the *first unconsumed* edge in cursor order, rather than any alt whose SYMBOL set intersects the multiset of unconsumed children. The fingerprint / blob discriminator path used by parsed-schema round-trips is untouched, so existing `EmitParse` / round-trip tests are unaffected.
- Regression test `lilypond_abstract_edge_order_preserved_through_pretty` in `crates/panproto-parse/tests/decorate_section_law.rs` builds the issue's reproducer and asserts the rendered byte order. Closes #104.

## [0.48.0] - 2026-05-18

### Added

- **`decorate` as the put-direction of the parse / decorate / emit lens** (`panproto-parse`, `panproto-schema`, `panproto-lens`, `panproto-gat`): a generator that takes an abstract schema (vertex kinds, `child_of` edges, leaf `literal-value` constraints) and attaches the full layout enrichment fibre (`start-byte`, `end-byte`, every `interstitial-N`, `chose-alt-fingerprint`, `chose-alt-child-kinds`), producing a schema the emitter renders byte-for-byte. New typed surface: `AbstractSchema` and `DecoratedSchema` newtypes in `panproto-schema` with sealed constructors and a `LayoutWitness` read-only view; `Schema::forget_layout` / `forget_layout_in_place` / `is_layout_free` implementing the schema-level forgetful U; `SchemaBuilder::build_abstract` / `build_decorated` routing through the typed newtypes. New entry points: `ParserRegistry::decorate`, `ParserRegistry::pretty_with_protocol`, `ParserRegistry::parse_emit_protolens`, `panproto_parse::LayoutPolicy` (aliased to `FormatPolicy`), `panproto_parse::decorate_with_parser`. New machinery in `panproto-gat`: `EnrichmentKind::Layout`, `is_layout_sort` predicate, `LayoutPolicySpec` wire form, `TheoryTransform::StripEnrichment` / `AddEnrichment`. New cross-crate extension point in `panproto-lens`: `enrichment_registry` with the `LayoutEnricher` trait; `ComplementConstructor::Enrichment`. Closes #102.

- **Book Rust code blocks are now compile-tested by CI** (`xtask/test-book`, `crates/book-doctest-stub`, `.github/workflows/ci.yml`, `.github/workflows/publish-book.yml`): every non-ignored ` ```rust ` block under `book/src/**/*.md` is fed to `rustdoc --test` with explicit `--extern` flags pointing at the workspace's compiled artifacts. The driver parses cargo's `--message-format=json` output to find one rmeta per crate and dispatches each block separately. Stale snippets surface as a CI failure rather than waiting for a reader to copy a broken example. The mechanism is documented in `book/CONTRIBUTING.md`.

### Changed

- **`emit_pretty` configurability**: `panproto_parse::emit_pretty::FormatPolicy` gains explicit `separator` and `newline` fields (formerly hardcoded as `b' '` and `b'\n'`); the policy is now honoured end-to-end through `pretty_with_protocol`. `AstParser` gains `emit_pretty_with_policy(schema, &FormatPolicy)`; the existing `emit_pretty` delegates to it with the default. Existing call sites are unchanged.

- **`panproto-parse` depends on `panproto-lens`**: the dependency arrow is now `PARSE → LENS` (not the other way) because the layout-enrichment `LayoutEnricher` trait lives in `panproto-lens`'s `enrichment_registry` and parsers register adapters into it on `ParserRegistry::register`. The graph remains acyclic.

- **Book updated for the typed parse / emit lens**: new pages `how-to/decorate-schemas.md` and `explanation/layout-enrichment.md`; updates to `how-to/parse-full-ast.md`, `explanation/lenses-roundtrip.md`, `explanation/architecture.md`, `reference/crate-map.md`, `reference/sdk-rust.md`, `reference/lens-combinators.md`. Tutorials and how-to walkthroughs switched from `json-schema` to `atproto` as the primary built-in protocol; SDK signatures (TypeScript `liftJson` / `getJson` / `putJson`, Python `Schema.validate` returning issue list, Rust free-function `validate`) aligned with the current API; CLI binary references corrected from `prot` to `schema`; `panproto-protocols` theory library table corrected to drop the nonexistent `ThVariant` / `ThNamed` / `ThOrder` entries and add `ThMeta`. Glossary populated with ten new layout-enrichment terms.

### Fixed

- **`strip_complement` semantics restored** (`panproto-parse::parse_emit_lens`): an intermediate refactor delegated `strip_complement` to `Schema::forget_layout`, which also strips `chose-alt-*` discriminators. The existing `EmitParse` law test relies on `chose-alt-fingerprint` surviving stripping (the emitter consumes it to dispatch CHOICE alternatives). `strip_complement` reverted to its original byte-positional-only semantics; `Schema::forget_layout` is the full-strip operation for the abstract-schema invariant.

- **`enrichment_registry` lock-poison handling**: the `RwLock` guards now recover from poisoning transparently via `PoisonError::into_inner` (the critical sections do not invoke user code, so a poisoned lock cannot leave invariants broken). Previously `.expect("poisoned")` would panic, violating the workspace's `clippy::expect_used` lint.

- **`enrichment_registry` lookup is allocation-free**: the inner map is now keyed by `(EnrichmentKind, String)` with a nested `HashMap` so `lookup_enricher` / `has_enricher` take `&str` directly with no per-call `Arc<str>` allocation.

## [0.47.3] - 2026-05-15

### Changed

- **Vendored QVR tree-sitter grammar upgraded to Quivers 0.9.0** (`grammars/qvr/`): re-vendored from FACTSlab/quivers `v0.9.0` (revision `76a8805d`), replacing the previous `16523d46` snapshot. The 0.7 surface renamed the top-level `quantale` keyword to `algebra` (emitted as `algebra_decl` alongside the existing `semigroupoid` / `bilinear_form` / `composition_rule` variants); 0.8 and 0.9 add the analysis-pipeline + algebra-guided training tooling and PyTorch primitive surface. Refreshed the `qvr_hmm_parses_with_expected_blocks` fixture to use `algebra product_fuzzy` and `algebra_decl` as the expected kind.

## [0.47.2] - 2026-05-14

### Changed

- **Vendored QVR tree-sitter grammar upgraded to Quivers 0.6.0** (`grammars/qvr/`): re-vendored from FACTSlab/quivers `v0.6.0` (revision `16523d46`), replacing the previous `a756fff9` snapshot. The 0.6 surface drops the pre-0.5 `stochastic` / `continuous` morphism keywords in favour of `latent` / `observed` (now `morphism_decl`) and introduces the new parametric `kernel_decl` for Markov-kernel declarations of the form `kernel f : A -> B ~ Family [options]`. Refreshed the `qvr_hmm_parses_with_expected_blocks` and `qvr_program_block_parses` integration tests to the 0.6 surface (`latent` for HMM morphisms, `kernel` for parametric-family declarations).

## [0.47.1] - 2026-05-12

### Fixed

- **Vendored QVR tree-sitter grammar refreshed** (`grammars/qvr/`): the panproto-vendored QVR grammar lagged the upstream Quivers 0.4 surface, emitting the pre-0.4 `arrow_draw_step` / `draw_step` / `output_decl` node kinds where the 0.4 surface expects `bind_step` / `marginalize_step` / `export_decl`. A clean `pip install quivers==0.4.0` could not parse any `.qvr` file using the 0.4 surface, including the examples shipped inside the Quivers wheel. Re-vendored `grammars/qvr/` from FACTSlab/quivers HEAD (revision `a756fff9`, previously `8aab05b`). Refreshed the `qvr_hmm_parses_with_expected_blocks` and `qvr_program_block_parses` integration tests to use the 0.4 surface (`<-` binder, `export` declaration, `bind_step` vertex kind). Closes #98.

## [0.47.0] - 2026-05-11

### Added

- **Runtime grammar override** (`panproto-parse::ParserRegistry`, `panproto-py::PyAstParserRegistry`): `ParserRegistry::override_grammar` / `register_external_grammar_owned` / `unregister` accept owned bytes (leaked into `'static` on registration) so grammar-author dev loops can swap a registered grammar mid-process. Exposed as `AstParserRegistry.override_grammar(name, extensions, language_ptr, node_types, tags_query=None, grammar_json=None)` on the Python side; `language_ptr` is the integer address of the `tree_sitter_<name>` function obtained via `ctypes` / `cffi` from a locally-compiled grammar shared library. Uses `Arc::get_mut`; raises `PanprotoError` when the underlying registry handle is shared (e.g. an outstanding `ParseEmitLens`). Closes #89.
- **Query anonymous-token field values from a parsed schema** (`panproto-parse::walker`, `panproto-schema::Schema`, `panproto-py::PySchema`): the walker now emits a `field:<name>` constraint on the parent vertex for every tree-sitter `field('<name>', anonymous-token)` child it encounters, capturing the matched token's text. `Schema::constraints_for(vertex_id)` returns every constraint on a vertex; `Schema::field_text(vertex_id, name)` returns the value of the `field:<name>` constraint or `None`. Surfaced on the Python `Schema` as `field_text(vertex_id, field_name)`. Closes #86.
- **`Theory.to_yaml` + `Theory.from_dict_yaml`** (`panproto-py::PyTheory`): YAML symmetric to the existing `to_json` / `from_dict_json` round-trip for the flat `panproto_gat::Theory` shape. Backed by `yaml_serde`. Closes the loaders + round-trip piece of #73 on the theory side (the DSL loaders `from_json` / `from_yaml` / `from_nickel` / `from_path` and the `TheoryBuilder` were added in 0.46.x via #83).
- **`ProtolensChain.from_dsl_{json,yaml,nickel,path}`** (`panproto-py::PyProtolensChain`): load a `panproto-lens-dsl` document (its Nickel / JSON / YAML surface) and compile it to a protolens chain anchored at the named `body_vertex` of the source schema. Mirrors what `Theory.from_{json,yaml,nickel}` does on the theory side, using the existing `panproto-lens-dsl::compile` entry point. Closes the loaders piece of #73 on the lens side.

## [0.46.1] - 2026-05-10

### Fixed

- **`panproto-grammars` cross-compile to `aarch64-unknown-linux-gnu`** (`crates/panproto-grammars/build.rs`): `localize_internal_symbols` now resolves the target-prefixed `objcopy` (e.g. `aarch64-linux-gnu-objcopy`) before falling back to plain `objcopy` / `llvm-objcopy`. The host's `x86_64-elf` objcopy could not parse aarch64-elf archive members; symbols went un-renamed and the cross-linker rejected the `panproto-grammars-all` cdylib with `multiple definition of scan/deserialize/scan_comment`. Closes #85.

## [0.46.0] - 2026-05-06

### Added

- **Real categorical pushouts in `Th(GAT)`** (`panproto-gat`): `pushout_by_name(t1, t2, shared)` constructs explicit identity-on-names inclusion morphisms `i1, i2` from a shared theory and delegates to the morphism-taking `colimit`, returning a full `ColimitResult` with the inclusion legs `j1, j2` exposed. `ColimitResult::verify_universal(q, k1, k2)` exhibits and verifies the unique mediating morphism `m: P → Q` factoring any alternative cocone — the real universal-property check, not just cocone commutativity. Coverage-defensive: errors when a pushout name lacks a mediator entry. Proptest covers identity-cocone factorisation; a hand-built test exercises factorisation through a strictly larger Q theory.
- **Real Cartesian universal factorization** (`panproto-lens::fibration::verify_cartesian_factorization`): exercises both projection functoriality (`get(f∘h) = get(f) ∘ get(h)`) *and* cleavage functoriality (`put(f∘h) = put(h) ∘ put(f)`). The two checks are not redundant under `GetPut`: GetPut constrains each lens individually, while cleavage functoriality constrains the agreement of distinct put paths from a shared decomposition.
- **Real pushout merge universal property in `Sch`** (`panproto-vcs::merge::verify_pushout_universal`): given an alternative cocone `(q_schema, k_ours, k_theirs)`, exhibits the unique mediating vertex map and verifies factorisation. New `PushoutError::UniversalFactorizationFailure` variant. Verifies the vertex-level necessary condition for full pushout-in-`Sch`; edge-level factorisation is documented as deferred (depends on whether `Edge.name` disagreements are admitted by the alternative cocone).
- **`Complement` as a partial commutative monoid** (`panproto-lens::asymmetric`): `compose` now returns `Result<Self, LensError>` with `ComplementConflict` (per-keyed-map disjointness/agreement violation) and `ComplementFingerprintMismatch` (cross-source-schema composition rejected). `Complement::is_compatible(&other)` exposes the domain of definition. Partial-monoid laws (left/right identity, idempotence on equal entries, associativity, commutativity on disjoint keys, conflict detection, fingerprint isolation) are unit-tested.
- **`PutPut` lens law check** (`panproto-lens::laws::check_put_put`): verifies `put(put(s, v1, c), v2, c) ≡ put(s, v2, c)` over identity and projection lenses with proptest coverage.
- **Symmetric-lens law check** (`panproto-lens::symmetric::check_symmetric_laws`): per-leg `GetPut` plus cross-side stability — left view must round-trip a right-side put, and vice versa.
- **Optic-kind law dispatch** (`panproto-lens::optic::check_optic_laws`): `Prism`/`Affine` now check preview stability (the optic's idempotence on its focus); `Traversal`/`Affine` check `PutPut` against a structurally-perturbed view. `perturb_view_for_traversal` covers every `Value` variant — `Bool`, `Bytes`, `Token`, `CidLink`, `Blob`, non-empty `List`/`Unknown`/`Opaque` — so the law no longer silently passes on schemas with non-string/integer leaves.
- **Format-preserving byte-equal round-trip corpus test** (`crates/panproto-io/tests/format_preserving_corpus.rs`): asserts `emit_wtype_preserving(parse_wtype_preserving(bytes)) == bytes` over JSON (OpenAPI, ATProto, GeoJSON, FHIR, Brat), XML (TEI, NAF, RSS), and synthetic edge cases (pretty-printed, trailing newline, nested arrays).
- **`theory_endofunctor_equiv`** and **`protolens_composable`** (`panproto-lens::protolens`): public predicates for natural-transformation composability. `protolens_composable` admits both the strict categorical condition (`eta.target ≡ theta.source`) and the schema-level "Identity-source as wildcard" pattern that the codebase's authoring conventions rely on, with documentation that distinguishes the two and points callers to `check_applicability_with` for runtime per-step verification.
- **`ProtolensChain::instantiate_sequential`** and **`check_applicability_with` / `applicable_to_with`** (`panproto-lens::protolens`): sequential lens-by-lens instantiation through `compose::compose`, exercising lens laws end-to-end on real intermediate states. The fused form (`instantiate`) remains the default for migration-metadata fidelity (e.g. `expansion_path` aggregation).

### Changed

- **`TheoryMorphism::compose` checks codomain/domain compatibility**, returning `GatError::ComposeDomainMismatch` when `self.codomain != other.domain`. Composing across mismatched morphisms previously succeeded silently as long as image names happened to match downstream maps.
- **`vertical_compose`, `ProtolensChain::fuse`, `ProtolensChain::instantiate_sequential`** reject mismatched intermediate endofunctors with `LensError::CompositionMismatch` rather than producing a silently-wrong protolens.
- **`Complement::compose` signature**: `&Self -> Self` → `&Self -> Result<Self, LensError>`. Composing complements that disagree on a shared key — or that carry different `source_fingerprint`s — now errors instead of silently picking the left operand. Pre-1.0; no backward-compat shim.
- **`panproto-protocols::theories` registration functions** propagate colimit failures as panics with informative messages (`# Panics` documented per function), replacing the prior `if let Ok(...) { register }` wrappers that silently skipped failed protocol compositions.
- **`panproto-lens::compose::compose_field_transforms` and `compose_conditional_survival`** drop entries whose anchor lives only in `m1`'s target space, preventing name-space mixing in composed migrations. Conditional-survival predicates additionally undergo a static scope check: free variables of `m2_pred` referenced through `DropField` / `RenameField{old_key}` / `KeepFields` (intersection-of-retain-sets) field-transforms on the corresponding anchor are conservatively rewritten to `Lit(Bool(false))` (the audit-recommended "default fail").
- **`panproto-lens::laws::instances_equivalent`** delegates `Value` and `extra_fields` comparison to the shared `asymmetric::value_equiv` / `extra_fields_equiv` path, eliminating the prior NaN-disagreement gap between law-checking and complement composition. `Value::Float(NaN)` is treated as reflexively equal; `+0.0` and `-0.0` remain IEEE-754-equal; distinct numeric variants (`Int(1)` vs `Float(1.0)`) remain distinct to preserve round-trip fidelity.
- **`panproto-vcs::merge::verify_pushout`** doc clarified to reflect that it verifies cocone commutativity (the necessary condition); the new `verify_pushout_universal` carries the universal-property check.

### Fixed

- **`Complement::compose` is now a partial monoid in fact, not just by claim**: the old left-biased merge was associative only by coincidence; conflicts on shared keys now surface as `ComplementConflict` rather than silently dropping data.
- **`vertical_compose`, `ProtolensChain::fuse`** no longer accept mismatched endofunctors, closing a soundness escape hatch where `η: F⟹G` and `θ: H⟹K` with `G ≠ H` composed silently into something whose source was `F` and target was `K`.
- **`compose_field_transforms`** name-space mix when an `m2`-anchor lived only in `m1`'s target space (introduced or renamed by `m1`).

## [0.45.0] - 2026-05-05

### Added

- **`panproto.TheoryBuilder` on the Python SDK** — a fluent builder mirroring `SchemaBuilder` and `MigrationBuilder`. Accumulates sorts, operations, and equational axioms via chained calls and produces a `Theory` ready for `colimit_theories`, `free_model`, and the migration engine. The dependent-sort surface (`"Tm(arrow(a, b))"`) is supported on the same footing as the Rust `class!` macro and the JSON / YAML / Nickel surfaces, since all three paths route through the same `panproto-theory-dsl` term parser. Existing `create_theory(dict)` callers keep working unchanged. Example:

  ```python
  t = (
      panproto.TheoryBuilder("upt")
      .sort("pitch")
      .sort("interval")
      .op("transpose", ["pitch", "interval"], "pitch", input_names=["p", "i"])
      .op("zero", [], "interval")
      .eq("transpose_zero", "transpose(p, zero())", "p")
      .build()
  )
  ```

- **`[features]` section on `crates/panproto-py/Cargo.toml`** that forwards every group and per-language flag of `panproto-grammars` and `panproto-parse`. Source-built wheels and downstream Rust consumers depending on `panproto-py` directly can now pick a smaller bundle (`group-core`, `lang-haskell`, …) or opt into the full grammar surface (`group-all`) without modifying `panproto-grammars`. The published wheel still defaults to `group-core` (the 11-language baseline) to keep the wheel within PyPI's per-file size limit; the spaCy-style language-pack story for the published wheel is the companion-pack architecture below.
- **Companion grammar packs**: a family of pip-installable extension wheels that contribute tree-sitter grammars to the core `panproto` wheel without bloating it. `panproto.AstParserRegistry()` is now a Python factory that discovers any installed pack via the `panproto.grammars` entry point and feeds its grammar metadata into the native registry at construction time. Native-only access (no companions) stays available as `panproto._native.AstParserRegistry`. The full set, each its own pyo3 cdylib depending on `panproto-grammars` with one `group-*` feature flag:
  - `panproto-grammars-web` — HTML, CSS, JavaScript, TypeScript, TSX, JSON, Vue, Svelte, Astro, GraphQL.
  - `panproto-grammars-systems` — C, C++, Rust, Go, Zig, D, Nim, Odin, V, Hare.
  - `panproto-grammars-jvm` — Java, Kotlin, Scala, Groovy, Clojure.
  - `panproto-grammars-scripting` — Python, Ruby, Lua, Bash, Perl, R, Julia, Nushell, Fish.
  - `panproto-grammars-data` — JSON, TOML, XML, YAML, SQL, CSV, GraphQL, Protobuf.
  - `panproto-grammars-functional` — Haskell, OCaml, Elm, Gleam, Erlang, Elixir, PureScript, F#, Clojure, Scheme, Racket.
  - `panproto-grammars-devops` — Dockerfile, Terraform, HCL, Nix, Bash, YAML, TOML, Make, CMake.
  - `panproto-grammars-mobile` — Swift, Kotlin, Dart, Java, Objective-C.
  - `panproto-grammars-music` — SuperCollider, LilyPond, ABC, Csound, ChucK, Glicol, Tidal mini-notation, Strudel mini-notation.
  - `panproto-grammars-all` — every grammar in `panproto-grammars`, for users who'd rather one install than picking groups.
- **Cross-cdylib boundary** (`panproto-parse::ParserRegistry::register_external_grammar` + the `extra_grammars` argument on `panproto._native.AstParserRegistry`'s constructor): each companion's `grammars_metadata()` returns a list of dicts containing the tree-sitter `Language` pointer plus byte-slice pointer/length pairs (cast to integers for transport across cdylibs). The trust boundary lives on the panproto-py side and is gated by a single `#[allow(unsafe_code)]`; companion modules bake their grammar bytes into `&'static` rodata so the pointers remain valid for the process lifetime. A process-wide cache deduplicates leaked metadata across repeat constructions, and `ParserRegistry::has_parser` short-circuits re-registration of grammars already in the registry (relevant when the umbrella `all` pack overlaps the per-group packs).
- **Tidal and Strudel mini-notation grammars** authored from spec, living in `grammars/tidal_mini/` and `grammars/strudel_mini/` alongside the existing 248 vendored grammars. Each `grammar.js` cites the documented spec example each rule was derived from (TidalCycles mini-notation reference and the Strudel mini-notation page). Corpus tests under `test/corpus/spec_examples.txt` cover every documented construct: 22/22 pass for Tidal, 14/14 for Strudel.
- **QVR (Quivers DSL) tree-sitter grammar** registered under `lang-qvr` and included in `group-all`. QVR is a domain-specific language for declaring categorical theories (quantales, objects, morphisms, continuous and stochastic spaces, monadic programs over them). Vendored from `FACTSlab/quivers` at `grammars/qvr/`, following the same `directory =` pattern as Stan, F#, Markdown, and Cedar's multi-grammar repos. Integration test at `crates/panproto-parse/tests/qvr_parse.rs` parses representative HMM and program-block sources end-to-end and asserts the structural vertex kinds (`quantale_decl`, `object_decl`, `stochastic_decl`, `let_decl`, `output_decl`, `type_alias_decl`, `continuous_decl`, `program_decl`, `draw_step`).
- **Per-(platform, group) companion publish workflow** (`.github/workflows/python-wheels-companions.yml`): on a `v*` tag push, builds and publishes one wheel per (target-platform, companion) pair.
- **CI companion smoke test** (`python-companion` job in `ci.yml`): builds the core wheel + the `functional` companion against a fresh venv on every push, asserts the registry discovers companion grammars via the entry point, and parses Haskell / OCaml / Scheme / Clojure end-to-end. Catches cross-cdylib regressions before tag time. The other companions follow the same generated template, so the canary covers the architecture; the music companion is exercised separately via the on-tag publish workflow because its grammars aren't vendored in the workspace.
- **`tools/fetch-grammars.py`**: skips the upstream clone when `grammars/<name>/grammar.js` and `src/parser.c` are already present, so self-referential entries (`tidal_mini`, `strudel_mini`) don't trigger a redundant clone of the panproto repo.
- **`.github/scripts/check_version_consistency.py`**: validates that every `bindings/python-grammars-*/pyproject.toml` pins `panproto>={major}.{minor},<{major}.{minor+1}` matching the workspace version, so a future workspace bump doesn't leave companions unsatisfiable on PyPI.

## [0.44.0] - 2026-05-04

### Added

- **`panproto-gat-macros::class!`, `inductive!`, and `derive_theory!` accept dependent sorts in argument and output positions** (closes #59). The macros previously parsed each argument and output as a single `Ident`, which forced manuscript-faithful encodings of theories like simply-typed lambda calculus to drop type witnesses or abandon the macros and call `Theory::new` / `Operation::with_implicit` / `SortExpr::App` by hand. The argument grammar is now `Ident: SortExpr` where `SortExpr := Ident ('(' Term,* ')')?` and `Term := Ident ('(' Term,* ')')?`, mirroring the JSON / YAML / Nickel surface in `panproto-theory-dsl`. Bare identifiers continue to compile to `SortExpr::Name`; applied identifiers compile to `SortExpr::App` with `Term::Var` / `Term::App` arguments. The simple-sort code paths were preserved by routing all generation through the existing `SortExpr::app` smart constructor, which collapses empty arg lists to `SortExpr::Name`. A regression test in `crates/panproto-gat-macros/tests/dependent_sorts.rs` ports the STLC fixture from `panproto-theory-dsl/tests/fixtures/stlc.json` and exercises the new grammar end-to-end.
- **`Theory.from_json`, `Theory.from_yaml`, `Theory.from_nickel`, `Theory.from_path`, `Theory.from_dict_json`, and `Theory.to_json` on the Python SDK** (refs #73). The `from_*` classmethods compile a panproto-theory-dsl source (string or file) directly into a `Theory`, accepting `theory`, `class`, and `inductive` body variants. Other body variants (`morphism`, `composition`, `protocol`, `bundle`, `instance`) raise `GatError` with a message pointing at the panproto-theory-dsl crate, since those produce multi-output sets rather than a single theory. The dependent-sort surface (`Tm(arrow(a, b))`) works identically to the Rust `class!` macro after #59. `to_json` emits the flat `panproto_gat::Theory` serde shape, and `from_dict_json` is the inverse round-trip path. The full bidirectional Nickel round-trip (`Theory.to_nickel() == open(path).read()`) and the fully Pythonic builder DSL described in #73 are deferred to follow-up work; the loaders here cover the hand-author / machine-author paths neume and other downstreams need.

## [0.43.3] - 2026-05-04

### Fixed

- **`@panproto/core` shipped a `dist/index.js` whose `new URL('./panproto_wasm.js', import.meta.url)` had been rewritten to `new URL('./panproto_wasm.js', "" + import.meta.url)`** (closes #57). Vite's lib-mode `assetImportMetaUrlPlugin` adds the `"" +` concat to keep the URL constructor portable, but the rewrite changes the AST shape downstream bundlers (Vite, Rollup, esbuild, Webpack 5) look for to copy sibling assets into their output. Production Vite consumer builds therefore 404'd on the wasm-bindgen glue when calling `Panproto.init()`. Switched the library build from Vite lib mode to `tsup` (esbuild + tsc), which leaves `import.meta.url` untouched. The shipped `dist/index.{js,cjs}` is now bundler-friendly out of the box; no consumer-side `resolve.alias` workaround is required.

### Changed

- **TypeScript SDK build pipeline migrated from Vite to tsup.** Vite remains in use indirectly through Vitest for test execution; the production build no longer depends on Vite. Test config moved from `vite.config.ts` to `vitest.config.ts`.

### Added

- **`@panproto/core` exports the wasm-bindgen glue at the `./panproto_wasm.js` subpath.** Consumers who prefer to own their wasm bundling explicitly can now `import glue from '@panproto/core/panproto_wasm.js'` and pass it to `Panproto.init(glue)` without falling back to a project-local `resolve.alias` shim. Refs #57.

## [0.43.2] - 2026-05-04

### Fixed

- **`Instance.to_json()` and `IoRegistry.emit(...)` returned `[]` for record vertices with anonymous outgoing edges** (closes #54, #55). Both Python entry points route through `panproto_inst::to_json`, which classifies a vertex as a list whenever every outgoing schema edge is anonymous (`name == None`). That heuristic also fires on a hand-built record whose author didn't supply edge names — a common shape when callers build schemas through the SchemaBuilder without explicit edge-name kwargs. The parser correctly preserved unhandled JSON keys in the node's `extra_fields`, but the emitter then took the list path and dropped them, yielding the literal output `[]`. Object-only signals on the node (a populated `extra_fields` map or a discriminator) now veto the schema-shape heuristic; the CST `$list` annotation and the same-name-arcs structural signal are unaffected because both are positive evidence about the data, not the schema, and cannot coexist with object content.

## [0.43.1] - 2026-05-04

### Fixed

- **`panproto/_native.pyi` stubs disagreed with the runtime for `create_theory` and `colimit_theories`** (closes #72). The stub typed `create_theory(spec: dict[str, object])`, which pyright rejects when callers pass a `TypedDict`; the runtime accepts any mapping. The stub typed `colimit_theories(theories: Sequence[Theory], /)`, but the pyo3 export is `colimit_theories(t1, t2, shared)`, so every downstream call site flagged either an argument-count or argument-type error. Widened `create_theory` to `Mapping[str, object]` and rewrote the `colimit_theories` stub to match the three-positional-argument runtime signature.

## [0.43.0] - 2026-05-01

### Fixed

- **`AstParserRegistry.emit_pretty(target="ocaml", ...)` raised `unknown variant "RESERVED"`** (closes #70). The bundled ocaml, ocaml_interface, javascript, and php `grammar.json` files use a tree-sitter ≥ 0.25 rule kind, `RESERVED`, that the panproto-side `Production` enum did not list. The deserialiser rejected the entire grammar before the schema-side walker ran. Added the variant; the walker treats it as a transparent wrapper around its inner content (the `context_name` reserved-word metadata is irrelevant for emit, since the emitter walks schema → bytes rather than enforcing reserved-word constraints, the same way `TOKEN` and `IMMEDIATE_TOKEN` are handled).

### Changed

- **`panproto_parse::emit_pretty::Production` is now `#[non_exhaustive]`** (breaking; minor-bumped). Adding a variant to a public enum is otherwise a breaking change every time tree-sitter introduces a new rule kind (which is what motivated this release). Marking the enum non-exhaustive forces downstream `match` statements to carry a catch-all arm, so future variant additions land as patch releases.

  *Migration*: any external `match` on `Production` that previously enumerated every variant explicitly needs a `_ => ...` arm. Internally panproto's own dispatch sites already had this pattern; no other workspace code needed to change.

### Added

- **Three regression tests** in `panproto_parse::emit_pretty::tests`: `reserved_variant_deserialises` (the bare-variant deserialiser), `reserved_grammar_loads_end_to_end` (a tiny grammar exercising `Grammar::from_bytes` with `RESERVED`), and `reserved_walker_helpers_recurse_into_content` (verifying that `first_symbol`, `has_field_in`, and `referenced_symbols` descend through `RESERVED` so the walker's choice-picking heuristic doesn't bail out).
- **Integration test** `crates/panproto-parse/tests/issue_70_ocaml.rs`: loads the actual vendored ocaml and ocaml_interface grammar.json files through `Grammar::from_bytes`. Confirms the fix end-to-end on the grammar that motivated the report.

### Audit

- An exhaustive scan of every vendored `grammar.json` (250 grammars) found `RESERVED` is the only tree-sitter rule kind missing from the deserialiser. All other rule kinds (`SEQ`, `CHOICE`, `REPEAT`, `REPEAT1`, `OPTIONAL`, `SYMBOL`, `STRING`, `PATTERN`, `BLANK`, `FIELD`, `ALIAS`, `TOKEN`, `IMMEDIATE_TOKEN`, `PREC`, `PREC_LEFT`, `PREC_RIGHT`, `PREC_DYNAMIC`) are already handled.

## [0.42.2] - 2026-05-01

### Fixed

- **Top-level `panproto` Python module was missing 16 public symbols.** The 0.42.1 `__init__.py` only re-exported a hand-maintained subset of `panproto._native`, so symbols added to the Rust pyo3 surface over time stayed reachable only via `panproto._native.X` (private namespace, surprises downstream tooling). The most-reported gap was `ProtolensChain` — downstream code that wrote `panproto.ProtolensChain.auto_generate(...)` crashed with `AttributeError`. Other gaps: `add_field`, `remove_field`, `rename_field`, `hoist_field`, `pipeline` (migration combinators); `auto_generate_lens_candidates`; `ProjectBuilder`, `ProjectSchema`, `build_project`, `parse_project` (multi-file projects); `GitImportResult`, `git_import` (git bridge); `ParseError`, `ProjectError`, `GitBridgeError` (error types). All 16 added to the import block and `__all__`.

### Added

- **Structural regression guard** (`bindings/python/tests/test_public_surface.py::test_every_native_public_symbol_is_top_level`): asserts every public symbol on `panproto._native` is also reachable on the top-level `panproto` namespace. Prevents the silent-omission shape: a new pyo3 export added Rust-side stays hidden until someone manually edits `__init__.py` to re-export it. The test fails loudly in CI with the list of missing symbols, so future drift is impossible to miss.

## [0.42.1] - 2026-04-30

### Fixed

- **`panproto` Python wheel was missing `__init__.py`**: the published 0.42.0 wheel on PyPI contained only `_native.abi3.so` — `__init__.py` and `py.typed` weren't bundled, so `import panproto` produced an empty namespace (the original #62 bug shape, reintroduced silently). Root cause: maturin's `python-source = "../../sdk/python/src"` pointed *out* of the package directory, and maturin silently dropped the source rather than warning. Fix: relocate `pyproject.toml` from `crates/panproto-py/` to `bindings/python/`, configure `tool.maturin.manifest-path = "../../crates/panproto-py/Cargo.toml"`, and use `python-source = "src"` (a direct child). Verified locally: the wheel now bundles `panproto/__init__.py`, `import panproto` exposes 56 public symbols, and `panproto.__version__` reports correctly.

### Changed

- **Repository layout**: `sdk/python/` and `sdk/typescript/` moved to `bindings/python/` and `bindings/typescript/` to sit alongside `bindings/haskell/`. All language-side wrappers around the Rust core now live under one parent directory; the Rust crates that produce binding artifacts (`crates/panproto-py`, `crates/panproto-wasm`, `crates/panproto-c`) stay in `crates/`. Published packages are unchanged: `panproto` on PyPI, `@panproto/core` on npm, `panproto` on Hackage all continue to point at the same artefacts.
- **Binding READMEs homogenised**: every binding's README now follows the same section sequence (title + badges, lead, Status, Installation, Synopsis, API overview / Modules, Distribution, Performance notes, Contributing, License). Binding-specific content preserved; structural parallelism added.
- **Per-binding CHANGELOG consolidation**: `bindings/haskell/CHANGELOG.md` shrunk to a static pointer at the root `CHANGELOG.md`. Per-binding CHANGELOGs were generating noise (most releases don't change the Haskell binding's API); workspace root is now the single source of truth.

### Removed

- **`crates/panproto-py/pyproject.toml`** removed (relocated to `bindings/python/pyproject.toml`).
- **`bindings/python/pyproject.toml` (deprecated hatchling/wasmtime SDK)** removed (per `project_deprecate_pure_python`; the native pyo3 wheel is the canonical Python distribution).

### CI

- **`.github/workflows/python-wheels.yml`**: invoke maturin with `working-directory: bindings/python` (was `--manifest-path crates/panproto-py/Cargo.toml`); upload wheels from `bindings/python/dist/*.whl`.
- **`.github/workflows/ci.yml`**: `maturin develop` invoked from `bindings/python/` instead of via `--manifest-path`.
- **Path references** under `crates/panproto-py/`, `crates/panproto-lens/`, `bindings/typescript/package.json` (`repository.directory`), `.github/dependabot.yml`, `.github/scripts/check_version_consistency.py`, `.github/pull_request_template.md`, `README.md`, `book/src/`, and the `release` and `breaking-change` skills updated to use `bindings/...` paths.

## [0.42.0] - 2026-04-30

### Added

- **`Protocol.from_theories(...)` (Python)**: classmethod that constructs a `Protocol` from a user-built `Theory` (or a pair, schema + instance) plus the protocol-level fields. Closes the gap between hand-rolled `Theory` objects (via `create_theory`) and `Repository.add(schema)`. Closes #63.
- **`schema theory repl` (CLI)**: interactive theory REPL with rustyline-driven syntax highlighting, persistent history under `$XDG_DATA_HOME/panproto/`, and tab completion of `:command` names. Replaces the standalone `panproto-repl` binary, which is removed; the REPL engine remains as the `panproto-repl` library crate.
- **`schema expr repl` (CLI)**: refactored to use the same shared rustyline driver as the theory REPL, so the expression REPL also gets highlighting, persistent history, and tab completion.

### Changed

- **`Ident::std::hash::Hash` documentation**: `panproto_gat::Ident`, `panproto_gat::ScopeTag`, and `panproto_vcs::hash::hash_theory` now document their stability semantics prominently. `Ident`'s std-Hash is process-local (the `ScopeTag` counter resets on every process start); for cross-process and durable-storage fingerprints, callers should use the content-addressed helpers in `panproto_vcs::hash`. The fingerprint stability guarantees of `hash_theory` are committed to within a panproto patch version, best-effort across minor versions, and will be locked in at 1.0. Closes #61.
- **`panproto-py` Python `__version__`**: read from `importlib.metadata.version("panproto")` so it stays in sync with the workspace version on every release. Previously hardcoded as `"0.14.0"` (six minor versions stale). The fallback `"0.0.0+unknown"` fires only when the distribution metadata is unreachable (running directly from a source checkout). Closes #62.

### Fixed

- **`crates/panproto-py/pyproject.toml`** is now `dynamic = ["version"]` so maturin reads the version from `Cargo.toml` (which uses `version.workspace = true`). The previous literal `version = "0.40.0"` silently stranded PyPI at 0.40.0 when the workspace bumped to 0.41.0: `python-wheels.yml` built `panproto-0.40.0-*.whl` files, and PyPI's `skip-existing: true` no-op'd the upload.
- **CI version-consistency guard** (`.github/scripts/check_version_consistency.py`): a new fast-fail CI job asserts every version-declaring file in the workspace agrees with `[workspace.package].version`. Catches the silent-PyPI-stranding shape of bug before tag.

### Removed

- **`panproto-repl` standalone binary**: the `[[bin]]` section was removed from `crates/panproto-repl/Cargo.toml`. The `panproto-repl` library remains. Use `schema theory repl` instead.

## [0.41.0] - 2026-04-29

### Added

- **`panproto-c` (new crate)**: panic-safe C ABI for non-Rust language bindings. Generated by `safer-ffi`; every `#[ffi_export]` entry point runs through `std::panic::catch_unwind` and converts panics, internal errors, and serialization failures into a `PpStatus` code plus a CBOR-encoded error envelope retrievable via `pp_last_error_take`. `pp_init` installs a process-global panic hook that suppresses the default stderr backtrace so caught panics surface only through the structured status channel. Two wire formats coexist at the boundary: opaque `u32` handles into a thread-local slab on the hot path (no serialization), and CBOR via `ciborium` on the cold path. The release covers `pp_init`, `pp_handle_free`, `pp_buf_free`, `pp_last_error_take`, `pp_protocol_define`, `pp_protocol_serialize`, `pp_schema_from_cbor`, `pp_schema_to_cbor`, and `pp_schema_validate`. The slab tracks `Resource::Protocol(Box<Protocol>)` and `Resource::Schema(Arc<Schema>)`, with `TypeMismatch` errors wired through every cross-variant call. Compiled as both `cdylib` and `staticlib`. The C header is generated via `cargo test -p panproto-c --features headers --test headers -- --ignored` and committed at `crates/panproto-c/include/panproto.h`.
- **`bindings/haskell/` (new tree)**: cabal package `panproto` (version `0.41.0`; Hackage publish pending) providing two implementations of every operation behind capability typeclasses returning plain `IO`. `ProtocolBackend` covers protocols (full structural mirror as `CanonicalProtocol` plus `EdgeRule`); `SchemaBackend` covers schemas as opaque CBOR bytes (`CanonicalSchema`); `SchemaValidate` is a refinement implemented only by the `Rust` backend at this release. The `Native` backend is pure Haskell; the `Rust` backend links against `libpanproto_c` via `foreign import ccall` and a tiny C glue layer (`bindings/haskell/cbits/panproto_glue.{c,h}`) that presents pointer-based wrappers around the by-value Rust API, sidestepping a portability gap in GHC's `CApiFFI`. Cross-backend agreement is verified by round-tripping each canonical type through both backends. 31 tests cover the protocol round-trip law (including all enrichment flags, edge rules, indef-length CBOR maps, unknown-field tolerance), error envelope decoding, exception-safe handle release via `bracket`, negative-path FFI status codes for invalid and freed handles, schema bytewise round-trip on both backends, schema validation, and `TypeMismatch` envelopes when the wrong handle kind is passed to a typed entry point.
- **CI workflow `build-panproto-c-bindist.yml`**: builds `libpanproto_c.{a,so,dylib,lib}` per platform (Linux x86_64/aarch64, macOS arm64+x86_64, Windows x86_64-msvc) on tag, packages each as a tarball alongside the C header, and uploads to the GitHub Release. Linux uses `cargo-zigbuild` so the static archive does not collide with GHC's RTS on `pthread`/`compiler_builtins` symbols. Hackage forbids precompiled binaries, so the Haskell bindings fetch from these GitHub Release assets via `bootstrap/fetch-bindist.sh`.

### Changed

- **Workspace dependencies**: added `safer-ffi = "0.1.13"` and `ciborium = "0.2.2"`.
- **CI (`ci.yml`)**: `panproto-c` excluded from the cargo-semver-checks job alongside `panproto-jit` and `panproto-llvm`. The crate is new in this release and has no published baseline on crates.io for the action to diff against; the exclusion is reversible once 0.41.0 is on crates.io.

### Removed

- **CI (`bench.yml`)**: the per-PR Benchmarks workflow is gone. The previous setup paired `cargo bench` (which uses `divan` here) with `benchmark-action/github-action-benchmark@v1`'s `tool: cargo` parser, which expects libtest's `bench: N ns/iter` format and silently fails on divan's table output. The job had been red on every PR for >24h while doing nothing useful. CI bench coverage will return via `iai-callgrind` (the project's stated CI bench tool) or `codspeed-divan-compat`; tracked separately.

## [0.40.0] - 2026-04-28

### Added

- **panproto-parse (`AstParser::emit_pretty`)**: new trait method that renders a by-construction `Schema` (no parse-recovered byte positions, no interstitials) to source bytes by walking the language's tree-sitter `grammar.json` production rules. The walker handles `STRING`, `PATTERN`, `SYMBOL`, `BLANK`, `SEQ`, `CHOICE`, `REPEAT`, `REPEAT1`, `OPTIONAL`, `FIELD`, `ALIAS`, `TOKEN`, `IMMEDIATE_TOKEN`, and `PREC*`. CHOICE alternatives are dispatched cursor-first against unconsumed children with a hidden-rule (`_`-prefixed) inline-expansion path. A `FormatPolicy` carries default whitespace and indent rules (one space between adjacent tokens, newline after `;` / `{` / `}`, indent on `{` / `}` boundaries). Per-language smoke tests now pass for JSON, TOML, Rust, Python, and Go (the entries that earlier in the branch shipped `#[ignore]`d are live); YAML remains pending. Closes #41.
- **panproto-parse (`parse_emit_lens`)**: new public module exposing the parse/emit pair as an asymmetric lens with machine-checkable laws. `ParseEmitLens::{new, parse, emit}` packages a single language's parse+emit into a `Lens<bytes, schema>`. `check_emit_parse` verifies the EmitParse retraction (`parse(emit(s)) ≅ s` modulo byte positions); `check_parse_emit` verifies the ParseEmit stability law (`emit(parse(b)) == b` byte-for-byte when `b` is parseable). Witness functions `kind_multiset` (vertex-kind counts) and `edge_multiset` (counts of `(src_kind, edge_kind, tgt_kind)` triples) are both load-bearing — vertex multiset alone does not distinguish a tree from its mirror. `strip_complement` removes byte-position constraints while preserving choice discriminators. `LawViolation` is re-exported as `ParseEmitLawViolation` from the crate root.
- **panproto-parse (seven category-theoretic fixes)**: tightens the parse/emit pipeline against the grammar-as-functor reading. (1) Discriminator on chosen CHOICE alt: `walker` records a `chose-alt-fingerprint` (literal trimmed gap_text) and a `chose-alt-child-kinds` (named child kind sequence, hidden rules filtered) at every CHOICE site so the emitter can replay the same alt the parser took. (2) Real subtyping closure: `Grammar` carries a transitive `subtypes` map computed from `grammar.json`. (3) Dependent-optic ALIAS routing: alias productions take a child whose kind matches the alias's value and walk the alias's inner content as the rule. (4) μ-binder cycle interpretation as least fixed points. (5) Token output is a free monoid over `Token` with a `Spacing` algebra, replacing direct byte concatenation. (6) `Production::Optional` snapshots and restores cursor + output on inner error mirroring `Repeat`. (7) CHOICE picker uses literal-score primary, symbol-score tiebreaker only.
- **panproto-parse (Stan grammars)**: `stan` and `stanfunctions` tree-sitter grammars vendored, including parser sources and node-types/grammar.json metadata.
- **panproto-grammars (`grammar.json` vendoring)**: the build script embeds each grammar's `grammar.json` alongside `node-types.json` and the compiled `parser.c`. `tools/fetch-grammar-json.py` populates the missing 240 grammar.json files from upstream tree-sitter-* repositories (regenerating via `tree-sitter generate` when upstream ships only `grammar.js`). The new `panproto_grammars::Grammar.grammar_json` field exposes the bytes; `LanguageParser::from_language_with_grammar_json` accepts them. All 250 vendored grammars now ship `grammar.json`.
- **panproto-inst (`NodeShape`)**: typed enum on `Node` (`Plain` / `List` / `XmlElement { tag }` / `XmlTextSegment`) replacing reserved-string entries on `annotations`. Defaults to `Plain` and skip-serialises when default. Builders: `Node::with_shape`. Predicates: `is_list`, `is_xml_text_segment`, `xml_tag`. Existing emitters consume the variants to drive list-functor wrapping, XML alias rename-back, and inline-text-run rendering.
- **panproto-py (full `Repository` surface, closes #56)**: native Python bindings now wrap the filesystem-backed `panproto_vcs::Repository` end to end. Methods: `init`, `open`, `add`, `commit`, `amend`, `log`, `head`, `head_state`, `resolve_ref`, `schema_at`, `merge`, `cherry_pick`, `rebase`, `reset`, `gc`. Branch CRUD: `create_branch`, `delete_branch`, `force_delete_branch`, `rename_branch`, `list_branches`, `checkout_branch`, `checkout_detached`, `create_and_checkout_branch`. Tag CRUD + read: `create_tag`, `create_tag_force`, `create_annotated_tag`, `delete_tag`, `list_tags`, `read_annotated_tag`. Index + reflog: `index`, `has_staged`, `clear_index`, `read_reflog`. Blame: `blame_vertex`, `blame_edge`, `blame_constraint`. Bisect: `bisect_start` returning a `BisectState` handle whose `step(is_good)` advances the search. Stash: `stash_push/pop/list/apply/show/drop/clear`. Data migration: `detect_staleness`. The pre-existing `VcsRepository` (MemStore-backed) is preserved unchanged for back-compat.
- **panproto-py (`ParseEmitLens`)**: Python class wrapping the parse/emit lens with `parse`, `emit`, `check_emit_parse`, `check_parse_emit`, plus static `strip_complement`, `kind_multiset`, `edge_multiset`. `AstParserRegistry` gains `emit_pretty(protocol, schema)` and `lens(protocol)` for constructing a `ParseEmitLens` against a registered language.
- **sdk/python (re-exports)**: `Repository`, `BisectState`, `AstParserRegistry`, `ParseEmitLens`, `available_grammars`, `parse_source_file` added to the top-level `panproto` namespace and `__all__`.
- **CI (`publish-crates.yml`)**: tag-driven crates.io publishing via crates.io Trusted Publishing. Uses `rust-lang/crates-io-auth-action@v1` to exchange the workflow's OIDC token for a short-lived registry token; no long-lived `CARGO_REGISTRY_TOKEN` secret stored in the repo. Idempotent retry: already-published crates at the target version are detected via `cargo search` and skipped. Topological order covers all 23 publishable workspace crates including `panproto-git-remote` (silently dropped from manual releases since v0.34.1).
- **CI (`publish-npm.yml`)**: tag-driven npm publishing of `@panproto/core` via npm Trusted Publishers (`id-token: write`, `npm publish --provenance`). No `NPM_TOKEN` stored; the npm CLI exchanges the workflow's OIDC token at publish time. Mirrors the working idiolect setup, including the npm-11.13.0 download-into-tmpdir trick for runners whose bundled npm is older than the OIDC handshake support.
- **PR template (`.github/pull_request_template.md`)**: change-type classification, surface-impact ticks, full pre-flight test plan, breaking-change migration block, and reviewer checklist. Formalises the project's existing review process.
- **panproto-io (`Registry::register_optional`)**: helper that takes `Result<UnifiedCodec, UnifiedCodecError>`, silently skips on `MissingGrammar` (the expected case when a `lang-*` feature is disabled), logs `ParserInit` errors to stderr before skipping (since those represent build-system regressions and must not be silenced), and registers on `Ok`. Replaces 46 `let _ = registry.try_register(...)` callsites that were silencing both error kinds indiscriminately.

### Changed

- **panproto-io (`UnifiedCodec` constructors return `Result`)**: pre-1.0 breaking change. `UnifiedCodec::{new, json, xml, yaml, toml, csv, tsv}` now return `Result<Self, UnifiedCodecError>` with `MissingGrammar` and `ParserInit` variants, replacing the two `unwrap_or_else` panic paths in `unified_codec.rs`. Callers under `panproto-io/src/{serialization, annotation, api, config, database, data_schema, data_science, domain, web_document}/mod.rs` route through `Registry::register_optional` (above) to skip rather than abort when a `lang-*` feature is disabled. Closes #52.
- **panproto-io (`xml_pathway` carries protocol in errors)**: `EmitInstanceError::Emit` sites in `xml_pathway.rs` now thread the caller's protocol name through `write_node` and the public `emit_xml_bytes` signature, replacing five `protocol: String::new()` placeholders. The `protocol` field on emit errors is now load-bearing for diagnostic output rather than uniformly empty.
- **panproto-vcs (`Repository::{read_index, write_index, clear_index}` promoted to `pub`)**: previously private staging-area helpers are now part of the public API so external bindings (panproto-py and downstream tooling) can inspect and clear the index without reimplementing the filesystem layout. `clear_index` is new; the other two preserve their existing behaviour.

### Fixed

- **panproto-parse (CHOICE fingerprint contamination)**: splitting the literal-token witness and the kind witness into two distinct `chose-alt-*` constraints fixes a regression where punctuation in node-kind names (`:KIND:` markers) leaked into the literal substring score and broke `pick_choice_with_cursor` on Rust round-trips. Hidden rules (`_`-prefixed, tree-sitter implementation detail) are filtered from the kind witness so they never appear as a forced child.
- **panproto-parse (`Production::Optional` state restoration)**: snapshots cursor and output before walking the inner production and restores both on error, mirroring `Repeat`'s pattern. Latent today (errors bubble to the top) but prevents silent state corruption when callers add fallback emit paths.
- **panproto-parse (`check_emit_parse` retraction witness)**: vertex-kind multiset alone is too weak — two schemas with identical kind counts but different edge structure (a tree and its mirror) compared equal. The check now also compares an edge-shape multiset over `(src_kind, edge_kind, tgt_kind)` triples, restoring a faithful retraction witness.
- **panproto-io (six structural round-trip bugs in JSON and XML)**: a sweep across the structural-roundtrip suite caught and fixed previously latent divergences in the JSON and XML codecs.
- **panproto-io (two complement-preserving inject bugs in JSON)**: injection paths that previously dropped or duplicated complement entries on the JSON codec are corrected.
- **panproto-inst (empty and singleton array preservation)**: arrays of length 0 and 1 round-trip through parse → emit without being collapsed into a non-array shape; relevant for JSON, TSV, and any list-functor-bearing protocol.
- **panproto-io (TSV header extraction)**: header row was previously incorrectly extracted on certain edge cases; round-trips are now stable.
- **panproto-py (no panic across the FFI boundary)**: every register site silently swallows a missing grammar and stderr-logs a parser-init failure rather than panicking, satisfying the same WASM no-panic-across-boundary contract that already held on the WASM side.

### Removed

- Nothing removed in this release.

### Migration notes

- `UnifiedCodec::json("openapi")` becomes `UnifiedCodec::json("openapi")?` (propagating) or `if let Ok(codec) = UnifiedCodec::json("openapi") { ... }` (skipping). The most common case — registering a codec into a registry — is one line via `registry.register_optional(UnifiedCodec::json("openapi"))`.
- `AstParser` implementations outside the workspace pick up the new `emit_pretty` default automatically (returns `EmitFailed` until overridden); the trait change is source-compatible.
- Existing 0.39 VCS repositories are unaffected by this release. The Repository public-method changes (`read_index`, `write_index`, `clear_index`) only widen visibility.

### One-time setup before the next tag-driven release

The two new publish workflows assume per-package Trusted Publisher configuration on the registries. See `.claude/skills/release/SKILL.md` for the full procedure, but the short version: configure each of the 23 panproto-* crates at `https://crates.io/crates/<name>/settings` → Trusted Publishing → Add (org `panproto`, repo `panproto`, workflow `publish-crates.yml`) and `@panproto/core` at `https://www.npmjs.com/package/@panproto/core/access` → Trusted Publishers (org `panproto`, repo `panproto`, workflow `publish-npm.yml`). PyPI's existing trusted-publisher config for `python-wheels.yml` is unchanged.

### Test coverage

- 2413 tests passing across the workspace.
- 18 new tests added during the bug sweep covering `NodeShape` (default/list/xml-element/xml-text-segment, serde skip-default + non-default round-trip), `register_optional` (MissingGrammar/ParserInit/Ok cases), `parse_emit_lens` (`check_emit_parse`, `strip_complement`, edge-multiset distinguishing structurally different schemas, `first_divergence` arms), walker (fingerprint and child-kinds emitted as separate constraints, hidden rules filtered), and xml_pathway (emit errors carry the protocol name).

## [0.39.0] - 2026-04-25

### Added

- **lexicons (DSL coverage)**: new `dev.panproto.schema.{class, instance, inductive, composition, bundle, conflictPolicy}` records cover the theory-DSL body variants. `dev.panproto.vcs.{fileSchema, schemaTree, flatSchema, dataSet, editLog, cstComplement, tag}` cover the Merkle-tree object kinds. `dev.panproto.node.{getSchemaTree, listTheories, listAlignments}` walk the new VCS structure. `dev.panproto.translate.verifyCoercionLaws` exposes the coercion-law sample-checker with `#coercionLawViolation` and `#filterOptions` defs.
- **lexicons (existing record extensions)**: `dev.panproto.schema.theory` records summary counts for directed equations, conflict policies, morphisms, classes, instances, and inductive types. `dev.panproto.schema.migration` carries an `alignmentStrategies` summary keyed by `StrategyTag`. `dev.panproto.schema.expr` `exprType` knownValues now match `panproto_gat::ValueKind`. `dev.panproto.schema.protolens` complement constructors include `droppedEdge`. `dev.panproto.vcs.commit` records `protocolHash`, per-theory `theoryIds`, `dataHashes`, `complementHashes`, `editLogHashes`, `cstComplementHashes`, and `timestamp` so the envelope reflects the full Merkle-aware `CommitObject`.
- **panproto-vcs (per-file content addressing)**: `Object::FileSchema` carries the parsed schema for a single project file. `Object::SchemaTree` is a typed enum with two shapes: `SingleLeaf { file_schema_id }` for one-file projects and staged-or-merged single-schema callsites, and `Directory { entries }` for multi-file projects whose entries are stored sorted by name so the root `ObjectId` is independent of insertion order. The project schema for a commit is a Merkle tree rooted at a `SchemaTree`. `tree::assemble_schema` (generic over `Store`) and `tree::assemble_schema_dyn` (behind `&dyn Store`) walk the tree and return the flat `Schema`, preserving every field the per-file schemas carry: vertices, edges, constraints, hyper edges, required predicates, NSIDs, variants, orderings, recursion points, spans, usage modes, nominal classifiers, coercions, mergers, defaults, policies, and entry sets. `tree::walk_tree` exposes the depth-first traversal in lexicographic order. `tree::resolve_commit_schema` / `_dyn` dispatch on a `CommitObject`. `tree::store_schema_as_tree` wraps a single assembled `Schema` as a `SingleLeaf` for code paths that produce one merged or staged schema at a time. `tree::build_schema_tree` and `tree::build_tree_from_leaves` emit the multi-leaf `Directory` shape used by project-level imports.
- **panproto-vcs (FlatSchema for migrations)**: `Object::FlatSchema` carries the flat-schema content that `Migration::src` and `Migration::tgt` reference, so `gc::mark_reachable` finds the targets of every stored migration. Cherry-pick, rebase, and the merge codepath store the flat-schema object alongside the tree-based commit so migration composition remains sound.
- **panproto-project (tree emitter)**: `build_project_tree` walks the parsed per-file schemas, runs `resolve_imports` for cross-file edges, sorts each file's `cross_file_edges` Vec to canonicalize wire order, and emits a Merkle tree of `FileSchemaObject` leaves joined by `SchemaTreeObject` nodes. Returns the root `ObjectId` ready to attach to a `CommitObject.schema_id`.
- **panproto-git (production blob-OID cache)**: `import_git_repo_persistent` is the entry point used by every `panproto-git-remote` and `panproto-cli git` import. It loads a persisted `BlobSchemaCache` keyed by `(git2::Oid, String)` (the second slot is the protocol the file is parsed under, so the same blob bytes parsed as `.py` and as `.txt` get distinct cache entries), reuses an unchanged file's `FileSchema` ObjectId, and persists the cache atomically (write to `<path>.tmp`, fsync the file, rename, fsync the parent directory) under `$GIT_DIR/panproto-cache/<remote>/blob_to_schema`. `load_blob_cache` distinguishes a missing file (returns an empty cache) from a corrupt file (returns `BlobCacheLoadError::Corrupt` with the offending line and reason). Imports see linear-in-distinct-file-versions object growth.
- **panproto-vcs (`#[non_exhaustive]` on `Object`)**: future variant additions no longer silently break downstream exhaustive matches.

### Changed

- **panproto-vcs (monolithic schema objects replaced)**: the `Object::Schema(Box<Schema>)` variant is gone. Every `CommitObject.schema_id` points at an `Object::SchemaTree` root whose leaves are `Object::FileSchema` objects. Callers that matched on `Object::Schema` go through `tree::resolve_commit_schema` (or `_dyn`). Callers that stored a flat schema via `store.put(&Object::Schema(Box::new(s)))` go through `tree::store_schema_as_tree`. Existing VCS repos produced by 0.38 and earlier are incompatible and must be rebuilt; no migration tool is provided.
- **panproto-git (import defaults to tree)**: `import_git_repo`, `import_git_repo_incremental`, `import_git_repo_persistent`, and `import_git_repo_with_cache` all emit `SchemaTree` roots. `export_to_git` assembles the flat schema from the tree before serializing, so the JSON shape in the git tree is unchanged.
- **panproto-py, panproto-wasm, panproto-cli (internal rewires)**: every call path that embedded a flat `Schema` in the store now routes through the tree helpers. Public Python and WASM APIs keep their existing shapes; the returned ids address `SchemaTree` objects.
- **panproto-vcs (`gc::mark_reachable` follows every commit-carried id)**: the reachability walk on `CommitObject` follows `theory_ids` and `cst_complement_ids` in addition to the schema, parent, and migration ids. Theory and CST-complement objects referenced from commits are correctly preserved during garbage collection.

### Fixed

- **panproto-vcs (`SchemaTreeObject` canonicity)**: an adversarial peer cannot construct two semantically-equivalent trees that hash differently. The `SingleLeaf` variant carries no name slot; the `Directory` variant's entries are sorted before hashing and consumption (`sorted_entries`).
- **panproto-vcs (`assemble_schema` field preservation)**: the assembler reconstructs every field of the flat `Schema` with consistent path-prefix rewriting on every vertex-id-valued entry.
- **panproto-vcs (single-leaf path collision)**: the single-leaf tree shape is a distinct enum variant with no path field, so it cannot collide with a real filename.
- **panproto-project (tree-built imports)**: `build_tree` runs `resolve_imports` and surfaces coproduct-builder errors via `ProjectError::CoproductFailed`. Cross-file import edges are canonically sorted before storage.
- **panproto-project (orphan import edges surface as errors)**: `ProjectError::OrphanImportEdge { src, tgt }` fires when an import edge points at a vertex that is not in any file's vertex list, rather than dropping it silently.
- **panproto-git (durable cache persistence)**: `save_blob_cache` writes through a `.tmp` companion, syncs the file to disk, atomically renames into place, and fsyncs the parent directory so a crash mid-save cannot produce a half-written cache.
- **panproto-git (non-UTF-8 git tree entries surface as errors)**: a tree entry whose name is not valid UTF-8 returns `GitBridgeError::NonUtf8TreeEntry { path, oid }` with the offending entry's blob oid.
- **panproto-vcs (`build_tree_from_leaves` invariants)**: duplicate paths return `VcsError::DuplicatePath`; empty path components return `VcsError::EmptyPath`.

### Removed

- **panproto-vcs**: `Object::Schema` and its canonical hashing, GC, and store-dispatch cases. The store has exactly three schema-bearing object kinds: `FileSchema` leaves, `SchemaTree` inner nodes, and `FlatSchema` migration domain/codomain references.

### Notes

- **Storage impact (measured target)**: a 492-file 213-commit project repository previously produced a ~20 GB local cache and ~9 GB of objects on the remote. Raw git for the same content is 14 MB. The file-level Merkle decomposition plus blob-OID dedup is expected to drop the panproto-vcs footprint under 100 MB, within an order of magnitude of git's number. Formal measurement against a real-world baseline is out of scope for this changelog entry and will be reported when the next push lands.
- **Breaking, by design**: this is a clean break. Existing VCS repos produced by panproto 0.38 and earlier cannot be read by the new object store and must be rebuilt (reimport from the git mirror, or rerun the project-builder path against the working tree). No migration command ships.
- **Test coverage**: 2384 tests passing. Every changed file has unit-test coverage of its public API, including all new error variants, every `SchemaTreeObject` variant, the `(blob_oid, protocol)` cache key, atomic cache persistence with reopen, the `OrphanImportEdge` path, the `cross_file_edges` order canonicalization, and the GC walk through theory and CST complement ids on commits.

## [0.38.0] - 2026-04-24

### Added

- **panproto-lens (sample-based coercion law verification)**: `check_coercion_laws(forward, inverse, class, samples, var_name)` evaluates a forward / inverse expression pair against the round-trip laws implied by the declared `CoercionClass`: `Iso` requires both `forward(inverse(v)) == v` and `inverse(forward(s)) == s`; `Retraction` requires only the backward composite; `Projection` requires the forward to be deterministic on the source; `Opaque` makes no claim. The checker returns one `CoercionLawViolation` per failing sample, distinguishing `Backward`, `Forward`, `NonDeterministic`, `MissingInverse`, `ForwardEvalError`, `InverseEvalError`, and `UnknownClass` so downstream consumers can destructure. `check_directed_equation_coercion_law` unpacks a `DirectedEquation`; `default_samples_for_string_value` ships a six-element sanity set. This matters because before 0.38 a user could declare a coercion `Iso` with a lying inverse, and the engine would accept it unchallenged; data corruption surfaced only at round-trip time in downstream systems like protolab.
- **panproto-lens (coercion sample registry)**: `CoercionSampleRegistry` is a per-`ValueKind` table of sample inputs for coercion law checks, pre-populated by `with_defaults()` for every primitive kind (`Bool`, `Int`, `Float`, `Str`, `Bytes`, `Null`, `Token`, and an `Any` union). `check_directed_equation_with_registry` runs the declared round-trip laws of a single equation against the samples registered for its source kind; `check_theory` sweeps every directed equation in a theory and returns a `TheoryCoercionReport` with a per-equation violation list. Iteration over value kinds in `with_defaults` goes through the new `ValueKind::all()` exhaustiveness-guarded list so adding a new primitive kind in `panproto-gat` breaks compilation until the registry is updated.
- **panproto-lens (coercion law validation trait)**: `CoercionLawValidation` is an extension trait implemented for `DirectedEquation` in the lens crate so `panproto-gat` stays free of lens dependencies. `deq.validate_coercion_law(&registry, var)` returns `Ok(())` when the declared class holds on every sample and `Err(violations)` otherwise in every build configuration.
- **panproto-lens (auto lens law gate)**: `AutoLensConfig::coercion_law_registry` is a new `Option<CoercionSampleRegistry>` field. When `Some`, every `CoerceAnchor` proposal emitted by the coerce strategy is validated against its witness's declared round-trip law on the sampled domain and dropped from the proposal set on failure. The filtering knobs (`FilterOptions::unknown_samples_policy`, `FilterOptions::unknown_witness_policy`) surface the two previously-asymmetric defaults explicitly; `filter_coerce_proposals_by_law_check` is exposed publicly for callers that want to audit the drops outside of `auto_generate`. The overall default remains `None`, preserving the pre-0.38 unvalidated behavior.
- **panproto-theory-dsl (compile-time law check)**: `compile_theory_with_law_check(spec, registry)` and `compile_theory_with_law_check_and_var(spec, registry, var_name)` compile the spec the same way as `compile_theory` and then run `check_theory` on the result. Sample-level violations are returned through a new `TheoryDslError::CoercionLawViolation` variant carrying the theory name, a cached distinct-equation count, and a structured `CoercionLawViolationDetail` for each failure so downstream tooling can tree-shake by variant. `compile_theory` itself stays unchanged, so existing callers retain the pre-0.38 behavior.
- **panproto-cli (theory check-coercion-laws verb)**: `schema theory check-coercion-laws <file> [--var-name <name>] [--json]` loads a theory document, runs `check_theory_with_var` against a default sample registry under the requested bound-variable name, prints a per-theory / per-equation report (human-readable or JSON), and exits non-zero on any violation. JSON output carries a typed `kind` field per violation (tag of the underlying enum variant) rather than a `Debug`-format string. When a majority of violations are `ForwardEvalError`s naming the same unbound variable, the CLI emits a `hint` line suggesting the right `--var-name`. Intended to run in CI as a lightweight gate against dishonest `Iso` and `Retraction` declarations.
- **panproto-gat (exhaustiveness-guarded enum iteration)**: `ValueKind::all()` and `CoercionClass::all()` return `&'static [Self]` slices of every variant, backed by exhaustive `match` witnesses in the function body. Adding a new variant upstream now breaks compilation of the accompanying match, forcing the slice (and any consumer using it as a canonical iteration order) to be updated in lockstep.

### Changed

- **panproto-lens (naturality-aware span exclusion)**: at `Stringency::Lenient` and above, `sources_without_naturality_compatible_targets` replaces the previous `sources_without_compatible_targets` predicate for deriving `DomainConstraints.excluded_sources`. The old predicate only checked that the source vertex's kind appeared somewhere in the target; the new predicate also checks that for every outgoing edge on the source there exists a naturality-consistent counterpart on some candidate target (matching kind, matching label when strict, and either an anchored child or a kind-compatible child). On cross-NSID atproto lexicon pairs where common vertex kinds appear on both sides, the old predicate retained nearly every source vertex in the CSP scope and the search returned no candidates; the new predicate excludes sources that cannot participate in any consistent morphism and lets the CSP find a morphism on the anchored sub-schema. `span_exclusions_at_lenient` now takes the resolved anchor set and a strict-edge-names flag; the kind-only predicate remains as a fallback for pre-alignment callers. A kind-bucketed target-edge index is cached per call so the per-source per-target per-edge inner loop is an O(1) hash lookup rather than a linear scan. Fixes panproto/panproto#51.
- **panproto-lens (coercion violation serialization)**: `CoercionLawViolation` and the DSL's `CoercionLawViolationDetail` now derive `Serialize` and `Deserialize` with `#[serde(tag = "kind")]`, so JSON output is shape-stable across formatter drift and downstream tooling can filter by variant name without string matching.
- **panproto-lens (deterministic Any-union)**: the `CoercionSampleRegistry::with_defaults()` union bucket for `ValueKind::Any` is now built by iterating `ValueKind::all()` in declared order rather than iterating a hash map, so the composite sample list is bit-stable across runs.
- **panproto-cli, panproto-theory-dsl (deterministic iteration)**: every user-visible iteration over `CompiledTheorySet`'s inner `HashMap`s (text output, JSON output, `Debug` impl) sorts the keys in lexicographic order before rendering. Prior output drifted across runs because `HashMap` insertion order is unspecified, which broke snapshot tests and made diagnostic diffs unstable.
- **panproto-cli (single diagnostic)**: `schema theory check-coercion-laws` now emits the report once and returns an `Err` with a concise top-line summary. The previous version printed the report and then re-emitted a near-duplicate message via `miette::bail!`, so users saw the same count twice on every violation.
- **panproto-cli (pluralization)**: the "all clean" summary distinguishes zero ("No theories to check."), one ("All 1 theory clean."), and many ("All N theories clean.") rather than emitting a regex-looking fragment.

### Fixed

- **panproto-lens (validate_coercion_law returns Err in every build)**: the trait method previously fired `debug_assert!` before returning `Err`, aborting the process in debug builds and making the documented `Err` contract untestable under `cargo test`. The assertion is removed; the method now consistently returns `Err(violations)` across every build configuration.
- **panproto-lens (asymmetric filter policy)**: `filter_coerce_proposals_by_law_check` used an asymmetric default: an unknown witness name dropped the proposal; a witness with missing samples kept it. Both paths now route through explicit `UnknownWitnessPolicy` and `UnknownSamplesPolicy` knobs so callers pick the semantics explicitly; defaults preserve pre-0.38 behavior.
- **panproto-lens (UnknownClass captures debug_repr)**: the `CoercionLawViolation::UnknownClass` variant previously stored the same enum value whose variant was unrecognised; adding a `CoercionClass` upstream made the stored value lose its identifying `Debug` information. The variant now carries a `debug_repr: String` captured at violation time, preserving the variant's name across future `Debug` formatter changes.

### Issues filed

- panproto/panproto#52 documents a pre-existing panic in `UnifiedCodec::new` (and its format-specific convenience constructors `json`, `xml`, `yaml`, `toml`, `csv`, `tsv`) when a tree-sitter grammar is missing or fails to initialize. Tracked for a follow-up `Result`-returning constructor.

## [0.37.0] - 2026-04-23

### Added

- **panproto-gat (implicit arguments)**: `Implicit`, a `Yes`/`No` tag on every operation input, threaded through `Operation::inputs` as a `Vec<(Arc<str>, SortExpr, Implicit)>` triple and surfaced at the call site by `typecheck_term`. An implicit input carries a value that is reconstructed by unification against the explicit arguments, so a user now writes `app(f, x)` where the encoding-level operation `app : (G, A, B, f : Tm(G, arrow(A, B)), x : Tm(G, A)) -> Tm(G, B)` had demanded `app(G, A, B, f, x)`. The original five-argument spelling remains legal so existing theories continue to typecheck unchanged, and the new smart constructor `Operation::with_implicit` plus the `Operation::arity` / `Operation::explicit_arity` accessors give downstream consumers a migration path. The DSL (`panproto-theory-dsl`) parses an `"implicit": true` flag on `OpSpec` inputs, and the STLC fixture is updated to mark `G`, `A`, `B` implicit on `lam`, `app`, and `subst`. This matters because the gap between a GAT's canonical signature and the shape a working developer wants to write at a call site was the main friction of the 0.36 release; closing it means the STLC encoding now looks the way the textbook writes it.

- **panproto-gat (closed sorts and pattern matching)**: `SortClosure`, an `Open` or `Closed(Vec<Arc<str>>)` attribute on every `Sort`, together with a new `Term::Case { scrutinee, branches }` constructor and a `CaseBranch { constructor, binders, body }` record. A closed sort names its constructor list up front, and `typecheck_term` at a `Case` site checks coverage (every declared constructor appears exactly once) and that each branch body typechecks to the same output sort under a context extended by the constructor's own binders. Pattern matching is now a first-class feature rather than a workaround via nested equations, which is what lets the inductive-type shorthand of the DSL and of the `inductive!` proc-macro close its constructor list cleanly without threading the coverage obligation through the user.

- **panproto-gat (confluence and termination)**: a new `rewriting` module with `check_local_confluence` (computes critical pairs of a `DirectedEquation` rule set and reports any that do not join) and `check_termination_via_lpo` (verifies termination under a lexicographic path order). Supporting types are `ConfluenceReport`, `CriticalPair`, `TerminationReport`, `RuleViolation`, `OpPrecedence`, and the raw `lpo_greater` comparator. This matters because `DirectedEquation` already backed coercion round-trips and now also drives the REPL's `:normalize` command and the new `typecheck_equation_modulo_rewrites`: if the directed system is not confluent-and-terminating, normalization is not a well-defined operation, and the two reports give a theory author the evidence they need to fix their rewrite system before it silently produces inconsistent normal forms.

- **panproto-gat-macros (new crate)**: a proc-macro crate exposing `class!`, `instance!`, `inductive!`, and `derive_theory!` as the programmatic-Rust surface for typeclass sugar. `class! { ThEq<A> { eq(x: A, y: A) -> Bool; axiom refl: eq(x, x) = true; } }` expands to a `theory_theq()` constructor that returns a `Theory` with the class's sorts, ops, and equations. `instance! { EqInt: ThEq<Int> in ThArith { eq = int_eq; } }` expands to a validated `TheoryMorphism`. `inductive!` mirrors the DSL's `InductiveSpec`; `derive_theory!` emits instance builders for `#[derive(Eq)]` and `#[derive(Hash)]`. Users who author theories as Nickel, JSON, or YAML config files continue to use `panproto-theory-dsl`; the two surfaces produce the same `Theory` and `TheoryMorphism` values and are a style choice, not a feature choice.

- **panproto-theory-dsl (class/instance/inductive documents)**: three new body variants on `TheoryDocument`, namely `ClassSpec`, `InstanceSpec`, and `InductiveSpec`, plus a new `ImportSpec` that declares that the enclosing theory imports named sorts and ops from another theory under an optional namespace prefix. Compilation is routed through new `compile_class`, `compile_instance`, and `compile_inductive` modules. The import pipeline rewrites imported identifiers into the declaring theory's namespace and rejects conflicting imports at compile time rather than deferring to typecheck. This makes the DSL the authoring surface for the Haskell-analogue workflow without forcing a Rust-compile step.

- **panproto-gat (typed holes)**: `Term::Hole { name }` is a new term constructor for placeholders, accepted by the new `typecheck_term_with_holes` entry point. Every hole produces a `HoleReport` carrying the hole name, the expected sort at that position, and the surrounding `VarContext`, so callers can print meaningful goal information the way a proof assistant does. This is the machinery the REPL's interactive typechecking surface wants: a user types a partial term with `?` placeholders and gets back one goal per hole, the same way GHCi's typed-hole feature works for Haskell.

- **panproto-gat (definitional equality)**: `alpha_eq_modulo_rewrites` on `SortExpr` and `typecheck_equation_modulo_rewrites` at the typechecker level join two sides of an equation under a bounded rewrite sequence over a directed-equation system. Previously an equation was accepted only if both sides typechecked to alpha-equivalent sorts under the structural equality on `SortExpr`; now equivalence under a terminating directed rewrite system is enough. The caller supplies the rewrite system and a step limit, and the `ConfluenceReport` and `TerminationReport` above are the evidence that the limit can be trusted to converge.

- **panproto-repl (new crate)**: an interactive REPL binary for loading theories, typechecking terms, normalizing through directed equations, browsing the free model, and compiling instances on the fly. The binary wires `rustyline` to a single `Repl::handle_line` entry point in the library crate, and the library is deliberately thin so that embeddings (editor plugins, notebook front-ends, web playgrounds) can reuse it. The command set is GHCi-shaped: `:load`, `:theories`, `:use`, `:sorts`, `:ops`, `:type`, `:normalize`, `:model`, `:instance`, `:quit`. Bare input is typechecked as a term in the active theory. Collapses the recompile-and-rerun loop that was the main friction of theory development against `panproto-gat`.

- **panproto-gat-macros (derive_theory)**: `derive_theory!` accepts a theory-declaration block annotated with `#[derive(Eq)]` or `#[derive(Hash)]` and emits the base theory plus instance-builder functions for each derivation. The two derivations cover the two capabilities every panproto sort is expected to carry (decidable equality, hashability) and are the common case that makes the `instance!` macro worth having.

- **panproto-theory-dsl (theory imports and namespacing)**: an `imports` list on `TheorySpec` declares that the enclosing theory reads named sorts and ops from another theory under an optional namespace prefix. Imported identifiers are rewritten into the declaring theory's namespace at compile time; duplicate imports and unresolved references are rejected with a descriptive error rather than surfacing as a typecheck failure later. This removes the copy-and-paste-a-theory pattern that was the prior recourse and is the load-bearing piece for the class/instance surface, which is a declarative form of "extend the target theory with the class's obligations".

- **panproto-gat / panproto-theory-dsl (source-span diagnostics)**: `TheoryDslError::TypeCheckSpanned`, together with a new `compile_with_source` entry point that retains the DSL source text so errors carry miette source spans back to the user's file and line. The wiring is also used by the REPL, which prints the spanned error directly; the earlier string-based `TypeCheck` variant is kept for callers that do not hold the source.

- **panproto-gat (let-polymorphism scaffolding)**: `Term::Let { name, bound, body }` introduces a local binding, and a new `SortScheme` type records the polymorphic scheme of the bound name. GAT signatures are first-order and their sorts have no free sort-metavariables, so `typecheck_term` at a `Let` site currently produces a monomorphic scheme (an empty `metavars` list) and binds the inferred sort into the context for the body. The type-level machinery is in place so that a later pass adding sort metavariables at the signature level does not reshape the term syntax; the documented limitation is that let-polymorphism is, as of this release, the monomorphic sub-case of the scheme it names.

- **panproto-mig (alignment strategies)**: six new alignment strategies seed the anchor pool with candidate correspondences, each guarded by a new `StrategyTag` variant. `edge_label_anchors` (`StrategyTag::EdgeLabel`) compares incident-edge label multisets so two vertices that share the same outgoing-edge names are matched even when their own names differ. `suffix_anchors` (`StrategyTag::ExactSuffix`) keys on the terminal dotted segment of a namespaced identifier so `app.bsky.feed.post#author` aligns with `com.example.post#author` without further hinting. `description_anchors` (`StrategyTag::DescriptionSimilarity`) runs token similarity over the `description` metadata when names are incompatible but human-readable blurbs agree. `neighborhood_anchors` (`StrategyTag::Neighborhood`) propagates anchors from already-matched neighbors, so a single high-confidence seed spreads outward through adjacency. `wl_anchors` (`StrategyTag::WlRefinement`) runs Weisfeiler-Leman structural refinement over the schema graph and matches vertices whose WL colors agree at a fixed number of iterations. Finally, `embedding_anchors` (feature-gated on `lm_embeddings`) is scaffolded behind an `Embedder` trait with a default `HashEmbedder`, ready for a real language-model embedding to be wired in. The real-lexicon integration test at `tests/integration/tests/cross_namespace_prop_alignment.rs` exercises the whole stack against cross-namespace AT Protocol correspondences.

- **panproto-mig (constraint-aware kinds)**: `kinds_and_constraints_compatible` complements the existing `kinds_compatible` by additionally requiring the two vertices' constraint sets to match, and is the predicate the stricter tiers of the resolver now consult. `vertex_is_required` and `adjust_anchors_by_required_sets` implement a required-set tiebreak that prefers anchors which preserve the required-vertex sets on both sides of the schema boundary.

- **panproto-lens (stringency surface)**: `Stringency` gains per-strategy `uses_*` methods (`uses_edge_label`, `uses_suffix`, `uses_description_similarity`, `uses_neighborhood_propagation`, `uses_wl_refinement`, `uses_embedding`) so the resolver can ask the tier directly whether a strategy is enabled without threading a strategy list through every call site. `run_strategies_for_tests` is now exposed so integration tests in other crates can drive the same configured pipeline the main entry point uses.

### Changed

- **panproto-gat (breaking)**: `Operation::inputs` is now `Vec<(Arc<str>, SortExpr, Implicit)>` rather than `Vec<(Arc<str>, SortExpr)>`. `Operation::new` continues to accept the two-tuple shape and tags every input as `Implicit::No`, so every existing call site compiles unchanged; the new `Operation::with_implicit` constructor accepts the three-tuple shape. Downstream consumers that iterated `op.inputs` pattern-match on the new third component; the workspace-wide update is exhaustive.

- **panproto-gat (free model keying)**: `free_model` keys fibers by the instantiated sort expression printed through an injective stringification rather than by the raw `SortExpr` pointer, which means two logically equal sort expressions produced by different call paths no longer split into distinct fibers. Fixes a class of spurious "empty fiber" results observed when `SortExpr::app` and the raw `App` constructor were mixed at the caller side.

- **panproto-theory-dsl (error propagation)**: unknown `SortKindSpec`, `ValueKind`, and `CoercionClass` values previously silently fell back to defaults; they now surface as `TheoryDslError::UnknownValueKind` and `TheoryDslError::UnknownCoercionClass`, pointing at the offending identifier.

### Fixed

- **panproto-gat (rewriting)**: pattern-matching against a term with bound variables was using a single flat substitution across sibling subterms, which caused a rule whose LHS bound the same variable in two independent positions to match under inconsistent bindings. Rewriting now threads the substitution through subterm-by-subterm with a consistency check at each binding site, and pattern-matching rejects the inconsistent case rather than committing to the first seen binding.

- **panproto-gat (typecheck holes, case, and let)**: three related bugs fixed in one pass. `typecheck_term_with_holes` was collecting the hole report before the substitution on the expected sort was applied, so the reported expected sort omitted the surrounding argument bindings; the report is now built after the substitution step. `Case` branches typechecked in the original context rather than in the context extended by the constructor's binders, which let a branch body reference a binder that was not yet in scope; the extended context is now built before each branch's body typechecks. `Let` was pushing the bound name into the term-variable context but not into the `VarContext` used by the sort-metavariable lookup, causing a reported sort mismatch on a correctly-typed body; the two contexts are now updated in lockstep.

- **panproto-gat (free model keying)**: a second instance of the injective-stringification invariant was fixed at the render path, where the debug printer was sharing a `Write` target across fibers and producing interleaved output when two fibers rendered concurrently. The render path now allocates a fresh `String` per fiber.

- **panproto-gat-macros (derive_theory input parsing)**: the `derive_theory!` parser rejected theory blocks whose attributes did not appear in alphabetic order, because it used a `BTreeMap` keyed by order-of-appearance rather than by attribute name; derivations are now collected into a set keyed by name so the order of `#[derive(...)]` annotations no longer matters.

- **panproto-theory-dsl (imports and unknown-kind propagation)**: the import pipeline silently dropped imports whose source theory failed to resolve; the error is now returned as `TheoryDslError::Imports`. The `SortKindSpec`, `ValueKind`, and `CoercionClass` enum decoders, which previously used serde's default fallback on an unknown variant, now return the descriptive `UnknownValueKind` and `UnknownCoercionClass` diagnostics described in the `Changed` section above.

- **panproto-mig (alignment edge cases)**: `wl_anchors` was using a fixed-size byte buffer as the color accumulator, so a color whose serialized form exceeded the buffer length wrapped and produced hash collisions; the accumulator now uses a length-prefixed serialization that uniquely encodes every multiset. `suffix_anchors` allowed a suffix of length zero to match every identifier when the source had no dot separators; the zero-length case is now rejected. `description_anchors` was comparing descriptions case-sensitively; matching is now Unicode-case-folded. `neighborhood_anchors` double-counted self-loops in the propagated-score computation; self-loops are now excluded.

- **panproto-repl (DSL routing)**: the `:instance` command was building `InstanceSpec` values by hand and calling into `panproto-gat` directly, bypassing the DSL's import pipeline and the miette-aware error surface. The command now routes through `panproto-theory-dsl::compile_instance`, so ad-hoc instances declared at the REPL see the same import resolution and source-span diagnostics that file-driven instances do.

## [0.36.0] - 2026-04-21

### Added

- **panproto-gat (dependent sorts)**: `SortExpr`, a sort-expression type that is either a plain sort name or a named sort applied to argument terms, threaded through `Operation::inputs`, `Operation::output`, and `SortParam::sort`. Sort parameters bound at an operation's declaration site are in scope in every later input sort and in the output sort; `typecheck_term` substitutes concrete argument terms into the declared sort expressions as it walks the argument list, so an operation signature like `compose : (a, b, c : Ob, f : Hom(a, b), g : Hom(b, c)) -> Hom(a, c)` enforces the shared middle object positionally and rejects any call whose argument sorts disagree. Equation typechecking uses Robinson unification over `Term` with an occurs check. The free-model generator is fiber-indexed: a fiber of `Tm(Γ, A)` is keyed by the pair `(Γ, A)`, so the free category on two parallel generators gets exactly the well-typed composites and no spurious ones. The morphism well-formedness check `check_morphism` compares operation and sort signatures modulo positional alpha-renaming of bound parameter names, so a morphism from `id : (x : Ob) -> Hom(x, x)` to `id : (y : Ob) -> Hom(y, y)` is accepted as a rename of the bound variable. `SortExpr` carries manual `PartialEq`/`Hash` impls that quotient `Name(n)` and `App { name: n, args: [] }` into a single value, a `SortExpr::app` smart constructor that normalizes, a custom `Deserialize` that normalizes on load, `positional_param_rename`, `signatures_equivalent_modulo_param_rename`, and `sort_params_equivalent_modulo_rename` helpers reused from `colimit` when merging operations across independently-authored theories. Resolves the "decorative sort parameters" gap that had blocked encoding the simply-typed lambda calculus as a GAT.

- **panproto-theory-dsl**: Parses `"Tm(Ctx, A)"`-style strings into `SortExpr::App` from `OpSpec.output`, `OpSpec.inputs[].sort`, and `SortSpec.params[].sort`. The wire format remains a string; bare identifiers still produce `SortExpr::Name` so every existing theory document parses unchanged. `parse_sort_expr` and `parse_term` now propagate parse errors via `TheoryDslError::TermParse` with a context string naming the op or sort and the specific field that failed; empty input, unclosed parentheses, trailing garbage, and malformed identifiers are all surfaced at the parse site rather than deferred to typechecking. An STLC-as-GAT JSON fixture at `crates/panproto-theory-dsl/tests/fixtures/stlc.json` serves as an end-to-end integration test.

- **book**: New chapter `book/src/core/dependent-sorts.md` working through the simply-typed lambda calculus as a GAT, showing the `extend` / `var_zero` / `lam` / `app` / `subst` signatures, the β-equation stated as a plain equation between two already-well-typed terms, and the argument that explicit substitution sidesteps capture-avoiding issues at the meta-level. The GATs foundations chapter gains a new subsection documenting how `panproto-gat` represents dependent sorts in code, with rustdoc links to `SortExpr`, `typecheck_term`, and `typecheck_theory`. The morphisms-and-migration chapter now explicitly documents the syntactic-vs-derivability gap in the current equation-preservation check.

- **integration tests**: `tests/integration/tests/stlc_gat.rs` and `tests/integration/tests/dependent_sorts.rs` exercise full dependent-sort theories end-to-end, including typecheck acceptance, β-equation acceptance, JSON round-trip, and explicit rejection of ill-typed applications. Exhaustive proptest coverage for `SortExpr::subst` monoid laws, unification soundness and occurs-check necessity, typecheck idempotence and substitution commuting, free-model fiber fidelity (parallel-arrows-no-spurious-composites, every-term-well-typed, simple-sort backward compatibility), and morphism naturality.

### Changed

- **panproto-gat (breaking)**: `Operation::inputs` is now `Vec<(Arc<str>, SortExpr)>` and `Operation::output` is `SortExpr`, replacing `Arc<str>`. `SortParam::sort` is now `SortExpr`. `Operation::new`, `Operation::unary`, and `Operation::nullary` accept `impl Into<SortExpr>` so every existing call site that passed a `&str` or `Arc<str>` keeps compiling via the new `From` impls. Downstream consumers in `panproto-protocols`, `panproto-lens`, `panproto-schema`, `panproto-mig`, `panproto-parse`, `panproto-vcs`, and `panproto-wasm` switch to `op.output.head()` or `SortExpr::alpha_eq` where they previously compared `Arc<str>` names. The JSON wire format is unchanged: `Name(n)` serializes as the bare string `"n"` and `App` as `{"name": ..., "args": [...]}`, so existing stored theories round-trip byte-for-byte.

- **panproto-gat (free model)**: `assign_global_indices` sorts the sort-name keys before assigning consecutive global indices, making the numbering a pure function of the input regardless of hash-table insertion order upstream. Any downstream consumer that hashes free-model indices (the VCS layer) now sees stable numbering across logically equivalent theories.

### Fixed

- **panproto-theory-dsl**: `parse_sort_expr` and `parse_term` dropped malformed argument terms via `filter_map(Result::ok)`, silently reducing arity at the parse site and deferring the failure to a much later typecheck. Unclosed parentheses were accepted via `unwrap_or(inner.len())`, which treated everything to end-of-string as the argument list. `SortExpr::App` was being constructed directly by variant rather than via the `SortExpr::app` smart constructor, bypassing the normalization invariant. All three paths now produce descriptive errors.

## [0.35.0] - 2026-04-20

### Added

- **panproto-protocols (atproto)**: `parse_lexicon` now preserves atproto's string refinements on the canonical `Schema` graph instead of silently dropping them. `"format"` values (`datetime`, `at-uri`, `at-identifier`, `cid`, `did`, `handle`, `language`, `nsid`, `record-key`, `tid`, `uri`) round-trip as `Constraint { sort: "format", value: <raw string> }` on the corresponding string vertex; unknown future format names pass through verbatim so parsing stays total under spec evolution. `"knownValues"` (atproto's open-enum construct) round-trips as `Constraint { sort: "knownValues", value: <canonical JSON array> }` on the string vertex. Both new sorts are registered in the atproto protocol's `constraint_sorts` and therefore participate in `panproto-check::diff`/`classify` automatically — a change to a `format` or `knownValues` entry now surfaces as a schema change instead of going unnoticed. Resolves panproto/panproto#42. Downstream atproto codegen tools can drop their hand-written lexicon re-parsers in favour of reading these constraints off the `Schema`.

- **fixtures**: Vendored real-world schemas and data under a top-level `fixtures/` directory for use by examples and benchmarks across the workspace. Each fixture is pinned to a specific upstream commit, tag, or capture timestamp, with sources and licenses attributed in `fixtures/FIXTURES.md`. Contents: six AT Protocol Lexicons from `bluesky-social/atproto` at commit `750cfe9` (`app.bsky.feed.post`, `app.bsky.actor.profile`, `app.bsky.feed.like`, `app.bsky.feed.repost`, `app.bsky.graph.follow`, `com.atproto.repo.createRecord`); two live Bluesky AppView responses (`getProfile`, `getAuthorFeed`) and six record-shaped fixtures derived from them; three JSON Schema Store schemas (`package.json`, `tsconfig.json`, `github-workflow.json`); four protobuf files (OpenTelemetry `trace.proto`/`common.proto`/`resource.proto` at commit `85e63b1`, and `google/protobuf/descriptor.proto` at commit `e3370c2`); the SWAPI GraphQL SDL from `graphql/swapi-graphql` at commit `48d66bc`; and two Sakila DDL variants (Postgres, SQLite) from `jOOQ/sakila` at commit `aed53ce`. Fixtures are never fetched at build or bench time.

- **benchmarks (workspace-wide)**: Every non-binary crate now has a divan benchmark driven by the vendored fixtures. New benches in crates that previously had none: `panproto-schema` (parse/normalize/validate/clone real Lexicons), `panproto-check` (classify synthetic breaking changes against real Lexicons), `panproto-protocols` (register theories and parse real Lexicons), `panproto-core` (end-to-end parse → lift → emit pipeline on a real Bluesky post), `panproto-expr-parser` (lex and parse real migration field-transform syntax), `panproto-vcs` (commit a chain of real Lexicons, walk DAG), `panproto-parse` (tree-sitter parse of real OpenTelemetry `trace.proto`, feature-gated on `lang-protobuf`), `panproto-project` (assemble a multi-file project from three real OpenTelemetry protos), `panproto-lens-dsl` (parse and compile realistic AT Proto field-rename YAML), `panproto-theory-dsl` (parse a declarative `ThGraph` JSON spec), `panproto-grammars` (tree-sitter parse of real `.proto` source), `panproto-wasm` (parse a real Lexicon via the WASM boundary entry points), `panproto-xrpc` (serialize/deserialize a realistic `listCommits` payload and round-trip through a `wiremock` mock server), `panproto-git` (import a mini git repo containing real OpenTelemetry source), `panproto-llvm` (build the LLVM IR protocol and lowering morphisms for TypeScript/Python/Rust), and `panproto-jit` (classify realistic AT Proto field-transform expressions into their JIT compilation shape).

- **examples (workspace-wide)**: Every non-binary crate now ships at least one runnable example under `examples/` that exercises the crate's primary API against real fixtures. Examples run to completion with realistic output (e.g. `app.bsky.feed.post: 39 vertices, 39 edges`, `assembled project: 3 files, 1006 vertices`, `imported 1 commit(s)`). Where a feature gate is needed (`lang-protobuf` for tree-sitter-backed crates), the example declares `required-features` so `cargo build --workspace --examples` does not need the feature toggled globally.

- **panproto-wasm**: Re-export `parse_atproto_lexicon` and `schema_metadata` from the internal `api` module so the WASM boundary can be exercised directly from native Rust benches and examples. No behavioural change to the WASM-facing surface.

- **panproto-lens tests**: Three regression tests (`tests/put_preserves_view_edits_with_child_vertices.rs`) pin the `asymmetric::put` round-trip for a schema where per-field values live on child vertices (`user.name`, `user.legacyId`, `user.email`) rather than in the root's `extra_fields`, and where `FieldTransform`s are installed on the parent to mirror edge renames at the instance-data layer. One test drives the chain through protolens combinators, one builds the `CompiledMigration` by hand, and one mirrors the JSON-reparse-plus-anchor-remap path a downstream consumer takes. All three guard against future refactors of `asymmetric::put` reintroducing the panproto/panproto#40 scramble under this shape.

### Changed

- **benchmarks (Tier 1 rewrite)**: The six pre-existing divan benches in `panproto-gat`, `panproto-expr`, `panproto-inst`, `panproto-io`, `panproto-lens`, and `panproto-mig` were rewritten or extended to use the vendored real-world fixtures in place of synthetic scaffolding. `panproto-mig`'s `chain_schema`-based identity/contraction benches are augmented with compile/check_existence/compose/lift benchmarks against real `app.bsky.feed.post` / `actor.profile` / `feed.like` / `graph.follow` Lexicons and real Bluesky post records. `panproto-gat` adds colimit benchmarks for the actual `ThGraph + ThConstraint + ThMulti` composition used by AT Protocol. `panproto-expr` replaces synthetic sum/map workloads with filter/map/len/project expressions over real Bluesky post texts. `panproto-inst` adds parse-and-restrict benches against real post records. `panproto-io` replaces the inline `{"name":"Alice","age":30}` fixture with real Bluesky record parse-and-emit round-trips. `panproto-lens` adds real-record `get`/`put` round-trips through an identity lens over a real Lexicon schema.

- **book**: Revised all 28 narrative chapters for readability. Chapter openings now motivate the chapter in prose with longer, subordinate-clause-bearing sentences modelled on the opening paragraphs of *The Rust Programming Language* and Bartosz Milewski's *Category Theory for Programmers*; bulleted "This chapter covers" previews are removed in favour of orientation that arises from the running prose itself. Paragraph shapes sustain thought across sentences with subordinate clauses and vary in length across a paragraph. `preface/notation.md` is rewritten to cover the mathematical notation a reader will encounter ($\mathcal{C}$, $\mathbf{Set}$, $\mathbf{Hask}$, morphism, functor, natural-transformation conventions, commutative-diagram layout) in place of the earlier discussion of citation-key syntax and build-system mechanics. Chapter closings are trimmed to one- or two-sentence pivots. Further-reading sections are preserved, with references still verified against primary sources, and the running address-record example continues to thread through Parts I and II.

- **panproto-lens tests**: Renamed the put-regression test from an issue-number name to a behavioural name (`put_preserves_view_edits_under_rename_field`), and backticked `field_transforms` in its documentation for `clippy::doc_markdown`.

- **examples/benches style**: Retroactive `cargo fmt` pass plus three `clippy::doc_markdown` fixes across the newly added benches and examples (backticked `WInstance`, `ThMulti`, `NodeClient`). `panproto-llvm`'s `lower_typescript_to_llvm` example drops an unnecessary `Result` return. No behavioural change.

## [0.34.1] - 2026-04-17

### Fixed

- **panproto-lens**: `asymmetric::put` dropped user edits to the view when the forward lens had `field_transforms` on the affected anchor. The pre-`get` snapshot of `extra_fields` (captured to preserve data lost by non-invertible transforms) was restored wholesale over the view, which clobbered any edits the user made between `get` and `put` — including the renamed field in a `RenameField` chain. The snapshot is now layered under the view: surviving fields are taken from the view and propagated back through the inverse of each forward transform (`RenameField` moves the view value to the old key, `ApplyExpr`/`ComputeField` inverses evaluate against the view value, `AddField` keys are scrubbed), while fields the forward pass dropped with no inverse still come from the snapshot. Resolves panproto/panproto#40.

### Changed

- **book**: Expanded all 28 narrative chapters to roughly twice the prior word count, applying a calibration style derived from a close reading of *The Rust Programming Language* and Milewski's *Category Theory for Programmers*: Milewski-style openers with stakes and difficulty flags, Rust-Book middle-and-end discipline (running examples that grow across chapters, captioned numbered listings, titled-link forward references), anticipated reader objections, named-and-retired analogies, short opener / longer middle / short closer. Part I foundations and Part II core-constructions chapters carry a running address-record example ($S_0$, $S_1$, $S_2$) threaded through categories, functors, universal properties, colimits, GATs, the instance functor, the restrict/lift pipeline, lenses, protolenses, and protocol colimits. Every "Further reading" section audited against primary sources; corrections applied for Awodey / Riehl / Mac Lane / Leinster chapter numbers and the Foster 2007 TOPLAS attribution (previously mis-labelled as the Boomerang paper). No exercises or Challenges sections: the register is a monograph, not a textbook.

## [0.34.0] - 2026-04-17

### Changed

- **panproto-git-remote** (renamed from `git-remote-cospan`): binary is now `git-remote-panproto`; `panproto://` is the canonical URL scheme and `PANPROTO_PUSH_TOKEN`/`PANPROTO_TOKEN` the canonical env vars. Legacy `cospan://` URLs and `COSPAN_*` env vars are still accepted as fallbacks. Per-remote cache now lives under `$GIT_DIR/panproto-cache/<remote>/`; the previous `cospan-cache/<remote>/` directory is still read when present so no re-import is forced on upgrade. Resolves panproto/panproto#38.

### Fixed

- **panproto-git-remote**: `push` reported the full refspec in its `ok`/`error` status lines where the git remote-helper protocol requires only the destination ref. Git silently ignored the mismatched token, reporting "Everything up-to-date" even when the push failed or no-oped. The helper now reports `ok <dst>` / `error <dst> <why>` and mirrors error details to stderr. Resolves panproto/panproto#37.

### Added

- **panproto-git-remote**: `warm [<revspec>]` and `install-hooks` subcommands. A shared warm cache at `$GIT_DIR/panproto-cache/warm/` amortizes tree-sitter parsing and the project coproduct at commit time instead of push time. `install-hooks` writes a sentinel-guarded `post-commit` hook that invokes `warm HEAD` after each commit. On push, the helper copies warm objects and merges warm marks into the per-remote cache, short-circuiting the reparse path. If no warm cache exists, behavior is unchanged. Resolves panproto/panproto#36.

## [0.33.0] - 2026-04-17

### Added

- **panproto-lens**: `Stringency` axis (`Strict`, `Balanced`, `Lenient`, `Exploratory`) plumbed through `AutoLensConfig`, `HintSpec`, the CLI (`--stringency`), Python, WASM, and the TypeScript SDK. Each tier enables a superset of alignment strategies and coercion witnesses from the tier below; monotonicity is asserted by the corpus harness across all four tiers on every gold pair.
- **panproto-lens / panproto-mig**: Candidate API. `auto_generate_candidates(src, tgt, protocol, config, hints, top_n)` returns a ranked `Vec<LensCandidate>` in place of the single-morphism `auto_generate`. Each candidate carries `quality`, `coverage`, per-step `confidence`, strategy provenance, and a human-readable `explanation`. Ranking uses `quality + 0.5 · coverage + 0.2 · avg_step_confidence` with deterministic tie-breaks.
- **panproto-mig::align**: Six pluggable alignment strategies seeding the CSP with candidate anchors, each strategy in its own module with a `StrategyTag` priority:
  - `exact` — name + kind equality (Strict+).
  - `alias` — domain-agnostic English-language synonym clusters (`createdAt ≡ timestamp`, `uri ≡ url`, casing variants, etc.) with union-of-equivalence-classes semantics (Balanced+).
  - `token_similarity` — camelCase/snake_case/kebab-case/acronym-aware tokenization + token Jaccard + character-bigram cosine, blended by a convex combination (Balanced+).
  - `wrap_unwrap` — detects record flattening/nesting across a schema boundary, emitting the corresponding `HoistField`/`NestField` anchor plus a synthesized record-shape witness when appropriate (Lenient+).
  - `type_signature` — multiset overlap on edge kind signatures; consults the sort-lens library to emit coerced anchors where a witness exists (Lenient+).
  - `structural` — degree signatures + edge-kind Jaccard as a last-resort prior (Exploratory only).
- **panproto-mig::coerce**: Sort coercion as a first-class categorical construction. `SortLensWitness` carries a directional lens between sort carriers with a verified `CoercionClass` (Iso, Retraction, Projection, or Opaque). The built-in `WitnessLibrary` ships 12 closed-form witnesses spanning all Int/Float/Str/Bool pairs, each with lens-law property tests. A decidable `naturality` checker validates every proposed coercion against the enclosing theory before a candidate enters the pool.
- **panproto-lens**: `Protolens::SortCoerce` elementary variant + the `CoerceSort`/`MergeSorts` endofunctor variants in `panproto-gat`. The `endofunctor_to_protolens` dispatch realizes every coercion end-to-end through the lens engine; composition associativity and complement-coherence are preserved.
- **panproto-lens**: Span search at `Lenient+`. `sources_without_compatible_targets` + `span_exclusions_at_lenient` produce a maximal common subtheory `C`, and `factorize` emits real `DropSort`/`AddSort` legs that compose through the existing lens category. At `Strict`/`Balanced` the engine continues to search total morphisms `A → B`.
- **panproto-lens**: Cross-protocol autolens corpus harness under `tests/corpus/` with gold pairs spanning ATProto lexicons, SQL schemas, protobuf messages, GraphQL types, JSON-Schema documents, and tree-sitter-parsed source code. The harness runs every pair at every tier, asserts non-regression against pinned baselines, and snapshots explanations via `insta`.
- **CLI**: `schema lens generate` gains `--stringency`, `--top-n N`, and `--explain`. The JSON output fuses candidates, coerce proposals, requirements, and fused chains into a single top-level document via `augment_json_root`. The `--stringency` flag is case-insensitive for parity with the Python and WASM bindings.
- **panproto-py**: `auto_generate_lens(src, tgt, protocol, *, stringency=..., hints=None)` returns a `(PyLens, quality, coerce_proposals)` 3-tuple; `auto_generate_candidates(...)` returns the full candidate list with per-step dicts. `parse_stringency` trims whitespace and treats the empty string as unset.
- **panproto-wasm**: `auto_generate_candidates(src_handle, tgt_handle, opts_json) -> Vec<u8>` returns a MessagePack-encoded `{ candidates, coerce_proposals }` wrapper; candidate fields in the wrapper use snake_case in parity with the TS type declarations.
- **sdk/typescript**: `@panproto/core` exports `Stringency`, `HintSpec`, `CandidateResponse`, `LensCandidate`, `CandidateStep`, `CoerceProposal`, `CoercionClass`, and `StrategyTag`; `autoGenerateCandidates` and `autoGenerateWithHintSpec` wrap the WASM boundary with MessagePack. A real-WASM test suite (`sdk/typescript/tests/autolens-stringency.test.ts`) drives the end-to-end path.
- **Genericity guardrail**: `tests/integration/tests/genericity.rs` asserts no protocol or programming-language name leaks into the generic `panproto-*` crates outside the protocols crate, with a pinned baseline ratchet.
- **book**: Replaces the separate `tutorial/` and `dev-guide/` Quarto books with a unified mdbook under `book/`, covering foundations, core, protocols, SDKs, expression engine, VCS, and contributing. New `publish-book.yml` GitHub Actions workflow; `publish-tutorial.yml` and `publish-dev-guide.yml` removed.

### Changed

- **panproto-lens**: `ComplementSpec`, `DefaultRequirement`, and `CapturedField` now serialize with `#[serde(rename_all = "camelCase")]`, and `ComplementKind` with `#[serde(rename_all = "snake_case")]`, to match the field and variant naming already used by the TypeScript `ComplementSpec` contract. Previous releases emitted snake_case fields and PascalCase variants on the wire, making every TS consumer reaching for `forwardDefaults` / `capturedData` / `elementName` through the typed API observe `undefined`. A wire-format lock test (`complement_spec_wire_format_matches_ts_sdk`) pins all four variants and every field so the shape cannot silently regress. **Breaking**: any consumer that deserialized the old PascalCase/snake_case shape must update to the new shape.
- **panproto-mig::align**: `resolve_anchors` filters NaN-confidence anchors before sorting and uses `total_cmp` on the survivors. Previous releases collapsed NaN to `Ordering::Equal` via `partial_cmp().unwrap_or(Equal)`, which broke strict-weak-ordering and let a malformed NaN-confidence `UserHint` win its slot over a finite-confidence `Alias` purely on the strategy-priority tiebreaker.
- **panproto-gat**: `TheoryEndofunctor` variants `CoerceSort` and `MergeSorts` carry a structured `SortLensWitness` payload; `factorize` sorts `sort_renames` and `op_renames` deterministically before folding.
- **panproto-mig::hom_search**: edge-name pruning is relaxed at `Balanced+` so alias- and token-suggested anchors can seed the CSP even when edge names disagree; naturality remains the ultimate gate.
- **sdk/typescript**: `MigrationBuilder.invert` now remaps the Rust snake_case payload (`vertex_map`, `edge_map`, `resolver`) to the TypeScript `MigrationSpec` camelCase fields (`vertexMap`, `edgeMap`, `resolvers`). Consumers who reached for those fields through the typed API previously saw `undefined`.

## [0.32.0] - 2026-04-15

### Fixed

- **panproto-protocols (atproto)**: Canonical `app.bsky.feed.post` instances without a `reply` field now validate instead of erroneously failing with `MissingRequiredEdge` for `parent` and `root`. The atproto lexicon parser was leaving `#replyRef` (and sibling sub-defs) orphan in the signature graph because a `type: "ref"` property was recorded as a constraint only, with no edge to the referenced sort. Every downstream root-inference heuristic (`find_root_vertex`, `infer_root_vertex`, emit's `find_roots`) then deterministically selected the orphan sub-def as the instance root, after which the validator correctly reported `#replyRef`'s required children missing. The `ref` property now emits both the provenance constraint and a structural `ref` edge to the resolved target, so every semantic reference in the lexicon is a morphism in the signature graph. Resolves panproto/panproto#35.

### Added

- **panproto-schema**: Explicit pointed-schema structure. `Schema.entries: Vec<Name>` carries the finite family of basepoints (the sorts at which an instance may be rooted). `SchemaBuilder::entry()` declares entries; `build()` enforces well-pointedness (every entry must name a vertex). `primary_entry(&Schema)` returns the first declared entry or, as a documented non-canonical fallback, the deterministic choice among edgeless vertices. Colimit pushouts (`panproto-schema::colimit`) compute entries as the coproduct of pointed schemas; `normalize` composes the basepoint map with ref-chain collapse; `panproto-vcs::merge::three_way_merge_entries` implements a proper three-way merge on the entries subobject with delete-propagation and unilateral-addition semantics.
- **panproto-schema**: `SchemaBuilder::has_vertex()` and `SchemaError::UnknownEntryVertex`.
- **panproto-protocols**: `.entry()` wired for every atproto record/query/procedure/subscription; also openapi (paths), avro (unreferenced named types), cddl (every rule), bson ("root"), docx/odf (document elements). Remaining parsers fall back on `primary_entry`'s deterministic heuristic.
- **panproto-protocols (atproto)**: Union variants now emit `variantᵢ --ref--> resolved(Tᵢ)` morphisms, closing the coproduct injections as structural edges. Array items of `type: "ref"` emit the same morphism. Cross-lexicon ref targets materialize as opaque placeholder vertices so the signature graph stays connected. `parse_lexicon` uses a two-pass scheme (declare every def vertex first, then parse structure) so forward references never collide with later real declarations.
- **panproto-vcs**: `merge::three_way_merge_entries` is `pub` so downstream callers can exercise the pointed-merge logic independently of the full schema merge.
- **tests**: `tests/integration/tests/issue_35_replyref.rs` (end-to-end regression) and `tests/integration/tests/entries_properties.rs` (13 proptest-driven invariant tests over 256 cases each: well-pointedness, entry-idempotence, `primary_entry` preference/purity/containment, ill-pointed rejection, normalize preservation, three-way merge delete/addition/universe/diagonal laws).

### Changed

- **panproto-wasm**: `api::helpers::infer_root_vertex` and `api::instance::json_to_instance_with_root` fallback now consult `schema::primary_entry` instead of reinventing the "no incoming edges + kind preference" heuristic. The fallback is deterministic across HashMap iteration orders.
- **panproto-protocols (atproto)**: Dead `BuilderExt::has_vertex` shim in `annotation/bead.rs` removed in favour of the native `SchemaBuilder::has_vertex`.

## [0.31.0] - 2026-04-14

### Changed

- **panproto-parse**: Named-scope detection is now grammar-driven via tree-sitter `queries/tags.scm` instead of a hardcoded `SCOPE_INTRODUCING_KINDS` list. The tags query is the canonical primitive consumed by GitHub code navigation, Helix, and the `tree-sitter tags` CLI; adopting it makes scope detection uniformly correct across every supported grammar (Rust's `function_item`, Haskell's `function`, Elixir's macro definitions, etc.) without any per-language kind tables. Fixes panproto/panproto#34: `report_by_scope` now correctly labels Rust `fn` items (e.g. `BodyModified verify_push` on a commit that modifies the function body), where previously every change collapsed to the file root. The scope-ID format (`file::scope::$N`) is preserved; downstream consumers (`panproto-check::scope::report_by_scope`) require no changes.
- **panproto-parse**: Removed `SCOPE_INTRODUCING_KINDS` constant and `AstWalker::extract_scope_name`. Removed `WalkerConfig.extra_scope_kinds` and `WalkerConfig.name_fields` fields (unused once the grammar drives detection). Removed `extra_scope_kinds` entries from all 10 per-language `walker_configs` (python, typescript, tsx, rust, java, go, swift, csharp, c, cpp); `extra_block_kinds` entries are retained.
- **panproto-parse**: `AstWalker::new` now takes a `scope_detector: Option<&mut ScopeDetector>`; pass `None` to disable named-scope detection. `LanguageParser::from_language` takes a new `tags_query: Option<&'static str>` parameter wired from `Grammar::tags_query`.

### Added

- **panproto-parse**: `scope_detector` module wrapping `tree-sitter-tags` with a uniform `NamedScope { node_range, name_range, name, kind }` view. `ScopeKind` enumerates the standard tree-sitter tags vocabulary (`Function`, `Method`, `Class`, `Module`, `Interface`, `Type`, `Macro`) plus `Other(String)` for grammar-specific suffixes.
- **panproto-parse**: Project-level tags-query override via `LanguageParser::set_tags_override(Option<String>)`, concatenated in front of the grammar's bundled query so overrides augment rather than replace the defaults. Integrates cleanly with a future `panproto.toml [parse.tags.<lang>]` declaration.
- **panproto-parse**: `ParseError::ScopeQueryCompile` variant reports tags-query compilation failures (malformed S-expression, unknown capture name, regex syntax error in `#strip!` predicate) with the underlying `tree-sitter-tags` error message.
- **panproto-grammars**: `Grammar::tags_query: Option<&'static str>` embeds each grammar's `queries/tags.scm` as a static string. When a grammar does not ship a tags query upstream, `tags_query` is `None` and scope detection falls back to file-level vertices only (no heuristic fallback).
- **panproto-grammars**: Build script honours the nvim-treesitter `;inherits: lang1,lang2` directive and an implicit inheritance table (typescript→javascript, tsx→typescript,javascript, cpp→c, cuda→cpp,c, ispc→c, arduino→cpp,c) so child grammars inherit parent definitions automatically.
- **tools/fetch-query-files.py**: New helper that shallow-clones each grammar's upstream repo and copies `queries/*.scm` into `grammars/<name>/queries/`. Companion to `fetch-grammars.py`; use `--skip-existing` to avoid re-fetching. 198 of 248 grammars ship `tags.scm`; the remaining 50 have no upstream queries and return `None`.
- **panproto-parse**: Integration test `tests/issue_34_rust_scopes.rs` reproducing the exact scenario from issue #34 (push-auth-shaped Rust file with enum, struct, and two `fn` items) and asserting all four symbols appear as named scope vertices.
- **Workspace**: `tree-sitter-tags = "0.25"` dependency.

## [0.30.1] - 2026-04-14

### Fixed

- **sdk/typescript**: `@panproto/core` now actually ships the wasm-bindgen glue and `.wasm` binary in the published tarball. Previous releases (0.1.0 through 0.30.0) had a `dist/` that contained only the bundled TS entry and type declarations; `Panproto.init()` failed with `ERR_MODULE_NOT_FOUND` on every consumer. The SDK's `build` script now runs `wasm-pack build ../../crates/panproto-wasm --target web --release`, bundles the TS, then copies `panproto_wasm.js`, `panproto_wasm.d.ts`, `panproto_wasm_bg.wasm`, and `panproto_wasm_bg.wasm.d.ts` into `dist/` next to the entry. A `prepack` script runs the full build so `npm publish` can't regress. Resolves panproto/panproto#33.
- **sdk/typescript**: `loadWasm` now works under Node.js. The `wasm-pack --target web` glue loads the binary via `fetch(file://…)`, which Node rejects; the loader now detects Node at runtime and preloads the `.wasm` bytes via `fs.readFile` + `fileURLToPath`, passing them to the init function. The browser/bundler fetch path is unchanged.
- **ci**: `Semver Check` no longer fails on unrelated PRs. `panproto-jit` and `panproto-llvm` pull in `llvm-sys` through `inkwell` behind optional features; `cargo-semver-checks` runs rustdoc for every workspace member and the runner does not propagate `LLVM_SYS_*_PREFIX` to that nested invocation, which was breaking dependabot PRs. Both crates are thin FFI wrappers without a public API SDK consumers pin against, so they are excluded from semver checking.
- **panproto-io**: Removed a stale `inject_tabular_cst` import from `unified_codec.rs` that triggered an unused-import warning under certain feature combinations.

### Changed

- **workspace**: Align the workspace with current gold-standard Rust architecture patterns.
  - Split `crates/panproto-wasm/src/api.rs` (4,878 lines) into a facade `mod.rs` plus ten domain submodules (`schema`, `instance`, `lens`, `registry`, `gat`, `vcs`, `data`, `enriched`, `helpers`, `graph`). Internal helpers are `pub(super)`-scoped so sibling modules import them explicitly.
  - Decompose long algorithmic functions into meaningful sub-routines: `colimit` into `build_rename_maps`/`merge_sorts`/`merge_ops`/`verify_cocone`; `TheoryTransform::apply` into per-variant helpers; the schema-level transform in `panproto-lens` into matching helpers plus `rebuild_adjacency`; `invert` into `validate_bijectivity`; `wtype_restrict` into `precompute_conditional_fail` and `connect_ancestor_to_child`; `parse_xml_bytes` into `ingest_xml_element` (also deduplicating Start/Empty element handling).
  - Audit every `#[allow(clippy::unwrap_used)]` site: 74 are correctly `#[cfg(test)]`-gated; the one production site in `panproto-wasm` refactors to an infallible `<[_; 1]>::try_from(vec)` match. Production `unwrap_used` allows: 75 to 0.
  - Remove 36 stale `#[allow(clippy::too_many_lines)]` attributes. The remaining suppressions document why the length is intrinsic (protocol format tables, chumsky recursive combinators, CLI command orchestration, three-way merge case analysis).
- **ci**: Run doc tests separately via `cargo test --doc --workspace` since `nextest` does not execute them.
- **ci**: New `feature-check` job using `cargo-hack` exercises the feature powerset over `panproto-core`, `panproto-io`, and `panproto-wasm`, catching feature-gated regressions that the default-features CI missed.
- **ci**: The `ts` job now installs `wasm-pack`, runs `pnpm run build`, and smoke-tests `Panproto.init()` in pure Node on every PR. Regression guard for the packaging defect above.

### Added

- **workspace**: `rustfmt.toml` pinning `edition = "2024"`, `max_width = 100`, and `use_field_init_shorthand = true`.
- **workspace**: `insta` as a workspace dev-dependency, with a proof-of-concept snapshot test in `panproto-check` pinning the JSON shape of `SchemaDiff`.
- Workspace dependency bumps: `rustc-hash` 2.1.2, `clap` 4.6.0, `proptest` 1.11.0.

## [0.30.0] - 2026-04-13

### Added

- **panproto-check**: Scope-level diff reporting via `report_by_scope()`. Groups flat vertex/edge changes from `SchemaDiff` by their nearest named program element (function, class, type) using the scope hierarchy encoded in vertex IDs. Classifies each scope as `Added`, `Removed`, `SignatureChanged`, or `BodyModified`. Resolves line numbers from `start-byte`/`end-byte` constraints. Includes `report_scope_text()` and `report_scope_json()` renderers. Resolves panproto/panproto#31.
- **lexicons**: `dev.panproto.node.getBlob` and `dev.panproto.node.listTree` XRPC query definitions for reading file content and listing tree entries from the git mirror.

### Changed

- **ci**: GitHub Actions bumped: `actions/checkout` v4→v6, `actions/setup-node` v4→v6, `actions/setup-python` v5→v6, `actions/download-artifact` v7→v8, `pnpm/action-setup` v4→v5.
- **sdk/typescript**: Dev dependency updates: `oxlint`, `typescript`, `vite`, `vitest`.

## [0.29.1] - 2026-04-13

### Fixed

- **panproto-mig**: Migration composition is now associative. Resolver, hyper-resolver, and expr-resolver composition correctly remaps keys through `m2.vertex_map` (G2 to G3 space) instead of the composed vertex map, and chases hyper-resolver values through m2's mappings.
- **panproto-schema**: `SchemaMorphism::compose` now drops unmapped intermediate vertices (partial-map semantics) instead of silently keeping them, restoring associativity for three-morphism chains.
- **panproto-gat**: Free model construction implements proper congruence closure. After equating terms via theory equations, the quotient now propagates equivalences through all operations (if `a ~ b` then `f(a) ~ f(b)`) using a reverse-index worklist algorithm, ensuring the free model is truly initial.
- **panproto-gat**: Cyclic sort dependencies in `topological_sort_sorts` are now rejected with `GatError::CyclicSortDependency` instead of being silently appended in arbitrary order.
- **panproto-gat**: Naturality check in `check_natural_transformation` now normalizes terms using both directed equations (rewrite rules) and undirected equations (applied bidirectionally), reducing spurious violations for theories with non-trivial equational axioms.
- **panproto-gat**: `ScopedTransform::apply` now updates outer equations, directed equations, and policies after the inner transform modifies sorts or operations, preventing stale references to renamed or removed sorts.
- **panproto-gat**: Pullback construction requires paired directed equations to have equal coercion classes (previously composed them, which is categorically meaningless). Projection morphisms are now validated after construction.
- **panproto-inst**: `functor_pi` (right Kan extension) skips foreign key propagation through multi-fiber Cartesian products where row indices are invalidated by the product expansion.
- **panproto-inst**: Conditional survival predicates in `wtype_restrict` are now precomputed for all nodes before the BFS traversal, making the restriction order-independent (functorial).
- **panproto-mig**: Migration inversion uses strict lookups for resolver and hyper-resolver keys instead of fallbacks that could produce invalid inverse keys.
- **panproto-lens**: Compiled migration composition no longer overapproximates the surviving vertex set (removed incorrect fallback check against un-remapped vertices).
- **panproto-lens**: Expansion path composition now chains paths through intermediate schemas instead of taking a flat union.
- **panproto-lens**: `instances_equivalent` now compares `parent_map` for structural consistency.
- **panproto-lens**: Iso optic complement validation checks all data-loss fields (dropped nodes/arcs/fans, original extra fields, original values, synthesized nodes) instead of only dropped nodes and arcs.
- **panproto-gat**: `TheoryMorphism::apply_to_term` documents that it only renames operations (not sort-parameterized structures), a limitation of the current untyped `Term` representation.
- **panproto-lens**: Protolens `vertical_compose` documents that it computes a direct endpoint migration at instantiation time rather than composing through an intermediate schema.

### Added

- **panproto-gat**: `GatError::CyclicSortDependency` variant for reporting cyclic sort parameter dependencies.
- **tests**: Category law integration tests for the Sigma-Delta adjunction unit law and functor identity restrict.
- **tests**: Migration composition associativity tests with resolvers, identity law tests, and vertex-dropping tests.
- **tests**: Schema morphism associativity and vertex-dropping tests.
- **tests**: Free model congruence closure test (`f(a) ~ f(b)` when `a ~ b`) and cyclic sort rejection test.

## [0.29.0] - 2026-04-13

### Added

- **panproto-inst**: `ElementOps` trait abstracting the category of elements `∫F` (Grothendieck construction) for all three instance shapes. Provides fiber selection, relational pushforward, stalk projection, graph builtin evaluation, and attribute extraction as a uniform interface over `WInstance`, `GInstance`, and `FInstance`.
- **panproto-inst**: `execute_elements<T: ElementOps>` generic query executor that works with any instance shape. Convenience wrappers: `execute_graph`, `execute_functor`, `execute_any` (dispatches via `Instance` enum).
- **panproto-inst**: `eval_with_element_ops<T: ElementOps>` polymorphic expression evaluator that delegates graph traversal builtins (`Edge`, `Children`, `HasEdge`, `EdgeCount`, `Anchor`) to `ElementOps` implementations.
- **panproto-inst**: `FInstance` element encoding via `encode_finstance_id`/`decode_finstance_id` (faithful bijection `(table_ordinal, row_index) ↔ u32`) enabling uniform `u32` element addressing across all instance shapes.
- **panproto-inst**: `GInstance` stalk projection includes scalar values from outgoing edge targets (dependent-sum projection), matching the `WInstance` convention.
- **panproto-inst**: `GInstance::pushforward` deduplicates results (set semantics), correctly handling cycles in graph-shaped instances.
- **lexicons**: `compareBranchSchemas`, `getCommitSchemaStats`, `getDependencyGraph`, `getFileSchema`, `getImportStatus`, `getProjectSchema` XRPC query definitions for the cospan node schema inspection API.

### Fixed

- **panproto-inst**: `build_node_env` now constructs the full stalk projection (`extra_fields` + scalar child values via labeled edges + metadata) instead of only binding `extra_fields`. Predicates in `InstanceQuery` can now reference child node values connected by `prop` edges with `edge.name`. Resolves panproto/panproto#29.
- **panproto-inst**: `QueryMatch.fields` now includes scalar child values in addition to `extra_fields`, so query results reflect the same observable data available to predicates.

### Changed

- **git-remote-cospan**: Enabled cargo-dist distribution (`dist = true`). The binary is now included in release artifacts, Homebrew formulas, and shell/PowerShell installers. Resolves panproto/panproto#28.

## [0.28.0] - 2026-04-12

### Added

- **panproto-xrpc**: `listCommits` and `diffCommits` XRPC query endpoints with typed response structs (`ListCommitsResult`, `DiffCommitsResult`, `CommitEntry`, `CommitIdentity`, `FileDiff`), camelCase wire format, and URL-builder tests. Resolves panproto/panproto#25.
- **panproto-git**: `import_git_repo_incremental` for incremental git import. Accepts a `known: &HashMap<git2::Oid, ObjectId, H>` map to skip already-imported commits via `revwalk.hide`, making repeated imports proportional to new commits only. Resolves panproto/panproto#26.
- **panproto-git**: `export_to_git` gains an `update_ref: Option<&str>` parameter. `None` creates the commit without moving any git ref (needed for batch DAG export).
- **panproto-inst**: `Value::List(Vec<Value>)` variant added to the `Value` ADT, completing the free term algebra with the list polynomial summand. JSON arrays now round-trip faithfully through `json_value_to_value` / `value_to_json` instead of collapsing to `Value::Unknown` with stringly numeric keys. Resolves panproto/panproto#27.
- **panproto-inst**: `is_list_vertex` generic detection rule replaces the hardcoded `vertex.kind == "array"` check in `node_to_json`. A vertex renders as a JSON array iff all its outgoing schema edges are anonymous (`name == None`), the free-schema characterization of ordered collections. Works for any protocol regardless of vertex-kind naming convention.
- **panproto-inst**: `parse_array` now identifies the item edge generically (first anonymous outgoing edge) instead of hardcoding edge kinds `"item"` / `"items"`.
- **git-remote-cospan**: Persistent per-remote `FsStore` cache under `$GIT_DIR/cospan-cache/<remote>/` with a plain-text marks file (`git-marks.txt`) for git-to-panproto OID mapping. Both `cmd_push` and `cmd_fetch` are now incremental across invocations.
- **git-remote-cospan**: `RemoteClient` trait abstracting `NodeClient` for testability. `FakeRemoteClient` in tests enables end-to-end testing of `cmd_push`/`cmd_fetch` without HTTP.
- **git-remote-cospan**: `topo_walk_from` iterative DFS post-order for topologically-correct commit export (replaces timestamp-ordered `log_walk` which could disconnect DAGs when git commits have non-monotonic author timestamps).
- **git-remote-cospan**: Stale-marks filter in `fetch_export_stage` drops marks referencing git OIDs no longer present in the destination repo, preventing silent parent drops and DAG disconnection after `git gc`.

### Changed

- **panproto-git**: `import_git_repo` no longer sets `refs/heads/main` on the store. Callers are responsible for naming the imported tip.
- **panproto-inst**: `apply_map_references` now operates on `Value::List` directly instead of the legacy `__array_len` sentinel encoding in `Value::Unknown`.
- **panproto-inst**: `value_to_expr_literal` for `Value::List` silently drops non-string elements (matching the pre-existing `__array_len` path behavior) instead of Debug-formatting them.
- **git-remote-cospan**: `cmd_push` and `cmd_fetch` are now generic over `C: RemoteClient`.
- **git-remote-cospan**: `cmd_push` explicitly sets the local `dst` ref after import (no longer relies on the removed `refs/heads/main` hardcode in `import_git_repo`).
- **panproto-cli**: `cmd_git_export` passes `Some("HEAD")` to `export_to_git` (preserving prior behavior).

### Documentation

- All 28 README files rewritten for accessibility: plain-English descriptions, intuitions before jargon, homogeneous structure across crates (title, one-liner, "What it does", "Quick example", "API overview", "License"), TypeScript-first quick start in the top-level README.
- Top-level README: centered logo with theme-aware `<picture>` element, badge row, and jargon-free architecture walkthrough.
- SDK READMEs (TypeScript, Python): updated to reflect current API surface, mention 248-language and 51-protocol coverage.
- PyPI `description` field updated from "Schema migration engine grounded in generalized algebraic theories" to "Automatic schema migration and version control for 51 schema languages and 248 programming languages".

## [0.27.3] - 2026-04-08

### Fixed

- **panproto-inst**: `wtype_restrict` now synthesizes fresh intermediate view nodes when a nest-style migration turns a direct source arc into a multi-hop target path. Before this fix, forward eval (`asymmetric::get`) of a `combinators::nest_field` chain on an actual instance failed with `restrict error: no edge found between X and Y in target schema` because the restrict pipeline had no mechanism to insert a fresh node in the view and re-route the source's direct `parent → child` arc through it. The schema-level fix in 0.27.2 only covered the target schema's edge set, not the instance-level forward eval. Resolves panproto/panproto#24.

### Changed

- **panproto-inst**: `CompiledMigration` grew a new `expansion_path: HashMap<(Name, Name), Vec<Name>>` field (skipped in serialization when empty) recording multi-hop routes for source arcs whose direct counterpart no longer exists in the target. Populated by `compute_migration_between` via BFS through new-in-target vertices. Consumed by `wtype_restrict` to synthesize intermediate view nodes.
- **panproto-lens**: `Complement` grew a new `synthesized_nodes: HashSet<u32>` field (skipped when empty) listing view node ids created during forward eval. `asymmetric::put` skips these nodes when reconstructing the source, so the round-trip `put(get(instance)) == instance` law holds for nest-style lenses.
- **panproto-inst**: `CompiledMigration` now derives `Default`, so downstream code can construct it via `CompiledMigration::default()` instead of enumerating every field.

## [0.27.2] - 2026-04-08

### Fixed

- **panproto-lens**: `combinators::nest_field` now works correctly on schemas where vertex ids are path-qualified (e.g., `user.name`) and differ from their short edge labels (e.g., `"name"`). The previous implementation silently produced chains that referenced nonexistent vertices and dropped edges by *kind* rather than by `(src, tgt, name)`, so passing `"prop"` as the edge kind would nuke every `prop` edge in the schema. Resolves panproto/panproto#23.

### Changed

- **panproto-lens**: `combinators::nest_field` signature changed (breaking, pre-1.0). It now takes three additional arguments: `old_edge_name: Option<Name>` (label of the `parent → child` edge to drop), `parent_to_intermediate: impl Into<Name>`, and `intermediate_to_child: impl Into<Name>` (labels for the two new edges). These let callers specify edge labels independently from vertex ids, which is required for schemas built via `SchemaBuilder::add_prop`, ATProto lexicons, or any protocol with qualified ids.
- **panproto-lens**: New elementary primitives `elementary::add_edge(src, tgt, name, kind)` and `elementary::drop_edge(src, tgt, name)` for fiber-level edge manipulation. Unlike `add_op`/`drop_op`, these let `Edge.name` differ from `Edge.kind` and target a single edge by its `(src, tgt, name)` triple.
- **panproto-gat**: New `TheoryTransform` variants `AddEdge { src_sort, tgt_sort, edge_name, edge_kind }` and `DropEdge { src_sort, tgt_sort, edge_name }`, both fiber-level (theory-level identity, schema-level effect) following the `RenameEdgeName` precedent.
- **panproto-wasm**, **panproto-lens-dsl**, **@panproto/core**: `nest_field` step specs extended with optional `old_edge_name`, `parent_to_intermediate`, and `intermediate_to_child` fields. Empty values fall back to the old "label == vertex id" convention for callers that don't distinguish the two.

## [0.27.1] - 2026-04-07

### Added

- **panproto-lens**: `Protolens::optic_kind()` and `ProtolensChain::composed_optic_kind()` convenience methods for ergonomic optic classification. Both delegate to existing machinery (`classify_transform` and `OpticKind::compose`) but live on the user-facing types so callers no longer need to reach into `protolens.target.transform` or hand-fold over chain steps. Resolves panproto/panproto#22.

## [0.27.0] - 2026-04-06

### Added

- **panproto-theory-dsl**: New crate providing a declarative specification format for GAT theories, theory morphisms, compositions, and protocols. Three surface syntaxes: Nickel (`.ncl`) as the primary authoring format with typed contracts, record merge composition, parameterized templates, and imports; JSON and YAML as simpler alternatives. The evaluation pipeline normalizes any surface syntax to a `TheoryDocument`, then compiles it to `Theory` + `TheoryMorphism` + `Protocol`.
- **panproto-theory-dsl**: Five body variants for theory documents: `theory` (sorts, operations, equations, directed equations, conflict policies), `morphism` (sort and operation mappings between named theories with validation via `check_morphism`), `compose` (ordered colimit steps replayed via `panproto_gat::recompose`), `protocol` (schema theory + instance theory + edge rules with `TheoryRef` resolution for named, inline, or composed references), and `bundle` (multiple definitions in one file with dependency-ordered compilation: theories, then compositions, then morphisms, then protocols).
- **panproto-theory-dsl**: Nickel contract library (`contracts/theory.ncl`) bundled via `include_str!` with contracts for all document types and combinator functions (`simple`, `dependent`, `val_sort`, `param`, `unary`, `binary`, `nullary`, `eq`, `directed_eq`, `colimit`, `colimit_with_ops`, `edge_rule`, `keep_left`, `keep_right`, `fail_on_conflict`, `custom_policy`).
- **panproto-theory-dsl**: `builtin_resolver()` providing lookup for all 11 panproto building-block theories (`ThGraph`, `ThConstraint`, `ThMulti`, `ThWType`, `ThMeta`, `ThSimpleGraph`, `ThHypergraph`, `ThInterface`, `ThFunctor`, `ThFlat`, `ThGraphInstance`).
- **panproto-protocols**: Made six previously private theory constructor functions public (`th_simple_graph`, `th_hypergraph`, `th_interface`, `th_functor`, `th_flat`, `th_graph_instance`) for use by the theory DSL's builtin resolver.
- **panproto-cli**: `schema theory` subcommand group with `validate`, `compile`, `compile-dir`, `check-morphism`, and `recompose` actions.
- **Tutorial**: New chapter "Declarative Theory Specifications" covering theory authoring in Nickel/JSON/YAML, composition via colimit, morphism definitions, protocol construction, and bundles.
- **Dev Guide**: New chapter "Theory DSL Engine" covering the `panproto-theory-dsl` crate architecture, evaluation layer, compilation pipeline, bundle dependency ordering, and builtin resolver.

## [0.26.0] - 2026-04-02

### Added

- **panproto-lens**: Hint-guided auto-lens generation via forward-chaining constraint propagation. Users declare vertex anchors, scope constraints, exclusions, and scoring preferences in a `HintSpec`. A fixpoint loop derives additional anchors by propagating along unique edge-name matches (with vertex kind preservation), then the CSP solver runs with domain restrictions and custom scoring weights.
- **panproto-lens**: New `hint` module with `derive_anchors()` (forward-chaining anchor derivation), `build_domain_constraints()` (scope/exclusion/preference → CSP domain restrictions), `HintParts` struct, and `resolve_hints()` as the canonical entry point for all bindings.
- **panproto-mig**: `DomainConstraints` struct for the CSP solver: `restricted_domains`, `excluded_targets`, `excluded_sources`, `scoring_weights`, and `name_similarity_threshold` (domain filter pruning candidates below a normalized edit-distance similarity threshold).
- **panproto-mig**: `find_morphisms_constrained()` and `find_best_morphism_constrained()` that apply domain constraints during backtracking search with configurable quality scoring weights.
- **panproto-lens-dsl**: `HintSpec`, `Constraint`, and `PreferencePredicate` types in `document.rs` with serde support. `HintSpec` accessor methods: `scoring_weights()`, `name_similarity_threshold()`, `scope_pairs()`, `excluded_target_names()`, `excluded_source_names()`.
- **panproto-lens-dsl**: Nickel contracts (`HintSpec`, `Constraint`, `PreferencePredicate`) and combinator functions (`anchor`, `scope`, `exclude_targets`, `exclude_sources`, `prefer_same_edge_name`, `prefer_similar_name`) in `contracts/lens.ncl`.
- **panproto-lens**: `auto_generate_with_hints()` pipeline integrating constrained morphism search with overlap fallback and configurable quality threshold.
- **panproto-cli**: `--hints <path>` flag on `schema lens generate` accepting a JSON hints file.
- **panproto-wasm**: `auto_generate_protolens_with_hint_spec()` accepting MessagePack-encoded `HintSpec`.
- **panproto-py**: `PyProtolensChain.auto_generate_with_hint_spec()` accepting JSON-encoded hints.

## [0.25.1] - 2026-04-01

### Fixed

- **panproto-inst**: `parse_json` array parsing now matches edge kind `"items"` (plural) in addition to `"item"` (singular). ATProto and OpenAPI protocols define their array items edge kind as `"items"`, which caused array elements to be silently dropped during parsing. (fixes [#20](https://github.com/panproto/panproto/issues/20))

## [0.25.0] - 2026-04-01

### Added

- **panproto-lens-dsl**: New crate providing a declarative, human-readable specification format for lenses, protolenses, and related optical constructs. Three surface syntaxes: Nickel (`.ncl`) as the primary authoring format with typed contracts, record merge composition, parameterized templates, and imports; JSON and YAML as simpler alternatives. The evaluation pipeline normalizes any surface syntax to a `LensDocument`, then compiles it to a `ProtolensChain` + `FieldTransform`s.
- **panproto-lens-dsl**: Nickel contract library (`contracts/lens.ncl`) bundled via `include_str!` with contracts for all 19 step types, rule patterns, composition specs, and auto-generation config. Includes combinator functions (`remove`, `rename`, `add`, `add_computed`, `apply`, `compute`, `hoist`, `nest`, `map_items`, `pullback`, `coerce`, `merge`, and all elementary theory operations) plus template helpers (`counter_fields`, `string_fields`, `map_name`, `drop_feature`).
- **panproto-lens-dsl**: Four body variants for lens documents: `steps` (sequential pipeline mapping to `combinators::*` and `elementary::*`), `rules` (pattern-match rewrite rules with attribute operations, passthrough filtering, and `map_attr_value` transforms), `compose` (vertical and horizontal composition of named lens references with correct natural transformation semantics), and `auto` (delegation to `auto_lens::auto_generate` with caller-visible `AutoSpec`).
- **panproto-lens-dsl**: All 19 step types covering the full panproto lens algebra: `remove_field`, `rename_field`, `add_field`, `apply_expr`, `compute_field`, `hoist_field`, `nest_field`, `scoped` (recursive, with correct focus-relative body vertex), `pullback`, `coerce_sort`, `merge_sorts`, `add_sort`, `drop_sort`, `rename_sort`, `add_op`, `drop_op`, `rename_op`, `add_equation` (with proper `Term` parsing for `App` and `Var`), `drop_equation`.
- **panproto-lens-dsl**: Horizontal composition correctly fuses each chain to a single `Protolens` via `fuse()` before applying `protolens_horizontal`, producing `eta * theta : F . F' => G . G'` per the natural transformation composition law.
- **panproto-lens-dsl**: Rule compilation with `passthrough: drop` support via `FieldTransform::KeepFields`, per-rule `keep_attrs` collection, and `map_attr_value` compilation to `apply_expr` steps supporting add, subtract, multiply, prefix, suffix, negate, to-string, to-number, and to-boolean operators.
- **panproto-lens-dsl**: `LoadDirResult` struct returning both successfully loaded documents and per-file errors (no silent error swallowing).
- **Tutorial**: New chapter "Declarative Lens Specifications" covering Nickel-based lens authoring, composition via record merge, parameterized templates, rule-based rewrites, and the compilation pipeline.
- **Dev Guide**: New chapter "Lens DSL Engine" covering the `panproto-lens-dsl` crate architecture, Nickel evaluation, step compilation, rule expansion, composition modes, and expression integration.

## [0.24.0] - 2026-04-01

### Added

- **panproto-io**: Unified tree-sitter-based codec (`UnifiedCodec`) for format-preserving round-trips across all protocols. Parsing through tree-sitter CSTs preserves whitespace, key ordering, indentation, comments, and all other formatting: `emit(parse(bytes)) == bytes` for JSON, XML, YAML, TOML, CSV, and TSV formats. Gated behind the `tree-sitter` feature flag.
- **panproto-io**: `CstComplement` type capturing the full CST Schema as the complement of the extraction lens. The CST complement is orthogonal to the semantic `Complement` from schema migrations; they compose as a product.
- **panproto-io**: `FormatPreservingCodec` trait (behind `tree-sitter` feature) with `parse_wtype_preserving` and `emit_wtype_preserving` methods on `ProtocolCodec`.
- **panproto-io**: CST-to-Instance extraction lens for JSON, XML, YAML, and tabular formats with format-specific injection (not shared across formats). Each format's injection correctly handles its CST node types.
- **panproto-io**: `FormatKind` enum for dispatch across the six supported data format grammars.
- **panproto-vcs**: `CstComplementObject` and `Object::CstComplement` variant for VCS storage of format complements.
- **panproto-vcs**: `cst_complement_ids` field on `CommitObject` (backward-compatible via `#[serde(default)]`).
- **panproto-vcs**: `pass_through_cst_complement` and `store_cst_complement` functions in `data_mig` for threading format complements through schema migrations.
- **panproto-wasm**: `parse_instance_preserving` and `emit_instance_preserving` exports (behind `format-preserving` feature) for format-preserving operations through the WASM boundary.
- **panproto-core**: `tree-sitter` feature flag that enables `panproto-io/tree-sitter`.
- **panproto-parse**: Format round-trip tests for JSON, XML, YAML, and TOML verifying `emit(parse(source)) == source` via the `AstWalker` + `emit_from_schema` pipeline.

### Changed

- All 50 protocol registrations in `panproto-io` now dispatch to `UnifiedCodec` when the `tree-sitter` feature is enabled, falling back to legacy codecs otherwise.
- `ProtocolCodec` trait gains format-preserving default methods (behind `tree-sitter` feature). `UnifiedCodec` overrides them to provide actual format preservation; legacy codecs return `None` complement.

### Deprecated

- **panproto-io**: `JsonCodec`, `XmlCodec`, and `TabularCodec` are deprecated in favor of `UnifiedCodec` (behind the `tree-sitter` feature). They discard formatting during parsing; the unified codec preserves it.

## [0.23.0] - 2026-03-31

### Added

- **panproto-gat**: `RenameEdgeName` theory transform variant for fiber-level edge label renaming. Changes the JSON property key without modifying the theory structure. Classified as `Iso` (empty complement, bijective relabeling).
- **panproto-gat**: `ScopedTransform` theory transform variant for applying transforms to sub-theories reachable from a focus sort. Implements the left Kan extension along the sub-theory inclusion (pushout construction). (#16)
- **panproto-gat**: `reachable_sorts_from()` directed BFS for computing sub-theory reachability via operation edges.
- **panproto-lens**: `elementary::rename_edge_name()` protolens constructor for fiber-level edge label renaming.
- **panproto-lens**: `elementary::scoped()` protolens constructor for applying a protolens within a sub-schema at a focus vertex. Optic class depends on edge kind: `prop` → Lens, `item` → Traversal, `variant` → Prism.
- **panproto-lens**: `ComplementConstructor::Scoped` variant for per-element complement tracking in array traversals (dependent product in the slice topos).
- **panproto-lens**: `refine_scoped_optic()` for runtime optic classification based on edge kind.
- **panproto-lens**: `combinators` module with derived field-level operations: `rename_field`, `remove_field`, `add_field`, `hoist_field`, `nest_field`, `pipeline`, `map_items`. Each composed from elementary protolens steps. (#15)
- **panproto-wasm**: `protolens_pipeline` export for building protolens chains from arrays of step specs.
- **panproto-wasm**: `auto_generate_protolens_with_hints` export for morphism-hint-seeded automatic lens generation.
- **panproto-wasm**: Extended `ProtolensStepSpec` with `rename_edge_name`, `scoped`, `map_items`, `rename_field`, `remove_field`, `add_field`, `hoist_field`, `nest_field` step types.
- **TypeScript SDK**: `PipelineBuilder` class with fluent API for constructing protolens chains from combinator steps. (#15)
- **TypeScript SDK**: `ProtolensChainHandle.autoGenerateWithHints()` for cross-namespace lens generation with seeded vertex correspondences.
- **TypeScript SDK**: `PipelineStep` union type with all combinator step shapes.
- **Python SDK**: `ProtolensChain` class with `auto_generate`, `auto_generate_with_hints`, `instantiate`, `compose`, `fuse`, `to_json`, `from_json`.
- **Python SDK**: Combinator functions: `rename_field`, `remove_field`, `add_field`, `hoist_field`, `pipeline`.
- **Tutorial**: New chapter "Lens Combinators and Scoped Transforms" covering pipeline API, scoped transforms, morphism hints, and dependent optics.
- **Dev Guide**: New chapter "Dependent Optics and Scoped Transforms" covering `RenameEdgeName`/`ScopedTransform` implementation, optic classification, complement algebra, and combinator decomposition.
- **Integration tests**: 13 tests (8 deterministic + 5 property-based with 64 cases each) covering `rename_field`, `remove_field`, `pipeline`, `rename_edge_name`, `scoped` transforms, and lens law compliance.
- **References**: Added Riley 2018, Vertechi 2022, Capucci et al. 2024, Clarke et al. 2024, Spivak 2012 to both tutorial and dev-guide bibliographies.

## [0.22.1] - 2026-03-31

### Fixed

- **panproto-gat**: `AddSort`/`AddSortWithDefault` now carry an optional `vertex_kind` field for the schema-level Grothendieck fibration data. Previously, `apply_theory_transform_to_schema` set `vertex.kind = sort.name`, ignoring the vertex kind parameter from `elementary::add_sort`. (#18)
- **panproto-gat**: `Sort::default_vertex_kind()` derives vertex kind from `SortKind`: `Val(vk)` maps to the canonical value kind name (e.g. "integer", "string"), `Structural` falls back to sort name.
- **panproto-gat**: `ValueKind::as_str()` provides canonical string representations for all primitive value kinds.
- **CI**: native Python wheels (pyo3/maturin) now built and published for Linux (x86_64, aarch64), macOS (x86_64, arm64), and Windows (x86_64) via `python-wheels.yml` on every release tag. (#14)

## [0.22.0] - 2026-03-31

### Added

- **panproto-gat**: `CoercionClass::Projection` variant for classifying deterministic derivations from the total fiber (dependent-sum projections). Forms a diamond lattice with `Iso`, `Retraction`, and `Opaque`. Complement stores nothing for projections because `get` re-derives the value deterministically from the source fiber.
- **panproto-gat**: `CoercionClass::needs_complement_storage()` predicate distinguishing "lossless" (`is_lossless`, true only for `Iso`) from "empty complement" (true for `Iso` and `Projection`). `Retraction` and `Opaque` require complement storage.
- **panproto-inst**: `collect_scalar_child_values()` computes the dependent-sum projection from a node's total fiber, collecting scalar values from immediate child vertices keyed by edge name.
- **panproto-inst**: `build_env_with_children()` constructs the expression evaluation environment from the full fiber over a vertex (extra_fields and child scalar values), with extra_fields taking precedence on key collision.
- **panproto-inst**: `apply_field_transforms` now accepts child scalar values so that `ComputeField`, `ApplyExpr`, and `Case` transforms can access schema-defined scalar child vertices (not just `extra_fields`). This fixes panproto/panproto#13: `ComputeField` transforms can now decompose AT-URI fields and other format-annotated string properties stored as child vertices.
- **panproto-inst**: property-based tests for child scalar collection completeness, environment monotonicity, `ComputeField` determinism, and identity restrict preservation.
- **panproto-lens**: property-based test verifying `GetPut` lens law holds when `ComputeField` transforms access child scalar values via the dependent-sum projection.
- **panproto-lens**: `Projection` complement handling: `ComplementKind::Empty` (re-derivable), cost 0.0, no captured data.
- **integration**: `scalar_field_transforms.rs` with end-to-end tests for AT-URI decomposition via `Split`/`Index` expressions, multiple scalar transform composition, identity roundtrip, and property-based roundtrip verification.

### Fixed

- **panproto-inst**: `ComputeField`, `ApplyExpr`, and `Case` field transforms now access scalar values from schema-defined child vertices, not just `extra_fields`. Previously, string fields with format annotations (e.g., `"format": "at-uri"`) were stored as child vertices by `parse_json` and were invisible to field transforms, causing transforms like AT-URI decomposition to silently produce no output. (panproto/panproto#13)

## [0.21.0] - 2026-03-29

### Added

- **panproto-gat**: property-based tests (proptest) for alpha-equivalence reflexivity/symmetry, substitution identity, rename_ops identity, and free variable subset law.
- **panproto-gat**: property-based tests for theory index consistency and serde JSON round-trip fidelity.
- **panproto-gat**: property-based tests for morphism composition associativity, identity unit laws, renamed morphism validity, and composition validity preservation.
- **panproto-gat**: property-based tests for colimit sort/op completeness, shared element deduplication, and colimit commutativity.
- **panproto-gat**: property-based tests for typecheck idempotency and well-typed theory acceptance.
- **panproto-lens**: `Lens` now derives `Debug`.
- **panproto-lens**: property-based tests for GetPut and PutGet round-trip laws on randomly generated identity and projection lenses.
- **integration**: property-based tests for identity restrict preservation, restrict functor contravariance, and morphism composition associativity across crate boundaries.
- **panproto-vcs**: E2E test suite (`vcs_e2e.rs`) with 8 scenarios exercising the full VCS lifecycle using a realistic blog platform domain model (User, Post, Comment, Tag). Covers linear schema evolution (v1 through v4), concurrent feature merge, merge conflict resolution with `ChooseOurs`, theory tracking via `theory_ids`, rebase with data migration, stash and cherry-pick, bisect to find breaking changes, and composition path coherence.
- **panproto-cli**: cargo-dist release workflow with Homebrew tap (`panproto/tap/schema`), shell and PowerShell installers, and cross-platform binaries for 7 targets (macOS aarch64/x86_64, Linux gnu/musl aarch64/x86_64, Windows x86_64).

### Changed

- **panproto-cli**: description updated for Homebrew and crates.io display.
- **panproto-xrpc**: switched from native-tls (OpenSSL) to rustls for TLS, enabling static musl builds without system OpenSSL.
- **panproto-cli**: `git2` dependency uses `vendored-libgit2` feature for portable binary distribution.

### Fixed

- **panproto-grammars**: `build.rs` normalizes backslash paths to forward slashes in `include_bytes!` calls, fixing Windows compilation.

## [0.20.0] - 2026-03-28

### Added

- **panproto-project**: `panproto.toml` project manifest with workspace/package configuration, glob-based excludes, and per-package protocol overrides.
- **panproto-project**: `config` module with `ProjectConfig`, `load_config()`, `generate_config()`, `serialize_config()`, and `compile_excludes()`.
- **panproto-project**: `detect::scan_packages()` auto-detects Rust, TypeScript, Python, Go, Java, Kotlin, Elixir, and C++ packages from filesystem markers (Cargo.toml, package.json, go.mod, pyproject.toml, build.gradle, mix.exs, CMakeLists.txt).
- **panproto-project**: `ProjectBuilder::with_config()` constructor honors manifest excludes and per-package protocol overrides.
- **panproto-project**: `cache` module with `FileCache` for incremental parsing. Caches per-file schema parse results with mtime+size+blake3 invalidation, stored in `.panproto/cache/file_schemas.json`.
- **panproto-project**: `ProjectBuilder::with_config_and_cache()` constructor for cache-accelerated project assembly.
- **panproto-project**: `resolve` module for cross-file import resolution. Walks coproduct schemas for import-like vertices, matches against export vertices in other files via BFS constraint lookup, and inserts cross-file edges. Built-in rules for TypeScript, JavaScript, Python, Rust, and Go with comprehensive export vertex kind coverage.
- **panproto-gat**: `composition` module with `CompositionSpec`, `CompositionStep`, and `recompose()` for declarative theory colimit recipes. Specs record the exact sequence of colimit steps, enabling reproducible theory composition from serialized recipes.
- **panproto-gat**: `CompositionStep::Colimit` supports `shared_ops` field for colimits that share operations as well as sorts, with validation that shared operations exist in both input theories.
- **panproto-schema**: `Protocol` gains `schema_composition` and `instance_composition` fields (`Option<CompositionSpec>`) co-locating the colimit recipe with the protocol definition.
- **panproto-protocols**: `*_spec()` functions for all 6 theory composition groups (A through F), producing declarative `CompositionSpec` pairs that exactly mirror the imperative `register_*` functions.
- **panproto-vcs**: `Object::Theory` and `Object::TheoryMorphism` variants for content-addressed storage of GAT theories and morphisms. Theories are hashed via direct MessagePack serialization (deterministic Vec-based fields); morphisms use canonical BTreeMap form for HashMap fields.
- **panproto-vcs**: `CommitObject::theory_ids` field (`BTreeMap<String, ObjectId>`) tracking which theories governed the schema at each commit. Populated automatically during `commit()` and `merge()`.
- **panproto-vcs**: `CommitObject::builder()` API with `CommitObjectBuilder` for constructing commits with sensible defaults. All 60+ struct literal sites migrated to the builder pattern.
- **panproto-vcs**: `gat_validate::schema_to_theory()` extracted as public function for deriving implicit GAT theories from schemas, with deterministic sort/edge ordering and unambiguous operation naming.
- **panproto-cli**: `schema init` auto-generates `panproto.toml` from detected project structure.
- **panproto-cli**: `schema add` accepts file paths (tree-sitter parse), directory paths (project coproduct), and JSON schema files.
- **panproto-cli**: `schema status` shows per-file A/M/D changes grouped by package, using `.panproto/file_hashes.json` manifest for fast change detection.
- **panproto-cli**: `schema show` displays Theory and TheoryMorphism objects.
- **panproto-cli**: `schema diff --theory` uses stored theory objects for richer sort/op/equation diffs between commits.

### Fixed

- **panproto-protocols**: `ThMeta` now declares `Value` as a sort, satisfying the formal invariant that all sort names referenced in operation inputs/outputs are declared in the theory's sorts list. Group E instance colimit updated to share `{Node, Value}`.
- **panproto-vcs**: `hash_commit` is now deterministic: `theory_ids` uses `BTreeMap` (not `HashMap`) for guaranteed iteration order in content-addressed hashing.
- **panproto-vcs**: `schema_to_theory` produces deterministic theories by sorting vertex IDs and edges before enumeration, and uses unambiguous `->` / `#` separators in operation names to prevent collisions.
- **panproto-vcs**: merge commits now store `theory_ids` and use `merged_schema.protocol` (not `ours_commit.protocol`) for consistent protocol attribution.

## [0.19.0] - 2026-03-28

### Added

- **panproto-gat**: `CoercionClass` enum (Iso/Retraction/Opaque) forming a three-element lattice under information loss. Classifies the round-trip properties of value-level coercions as adjunction witnesses in the Grothendieck fibration over the schema category.
- **panproto-gat**: `classify_builtin_coercion()` classifies the 6 expression-language coercion builtins by source/target value kind and round-trip class.
- **panproto-gat**: `DirectedEquation` extended with `source_kind`, `target_kind`, and `coercion_class` fields for typed coercion declarations.
- **panproto-gat**: `TheoryTransform::CoerceSort` extended with `inverse_expr` and `coercion_class` for protolens-level coercion metadata.
- **panproto-schema**: `CoercionSpec` struct replacing bare `Expr` for schema coercions, carrying forward expression, optional inverse, and coercion class.
- **panproto-expr**: `ExprType` enum and `BuiltinOp::signature()` for lightweight type annotations on builtin operations.
- **panproto-expr**: `typecheck` module with `infer_type()` and `validate_coercion()` for expression type inference.
- **panproto-expr**: new builtins: `Round` (float to int with rounding), `DefaultVal` (null coalescing), `Clamp` (numeric range clamping), `TruncateStr` (char-boundary-safe string truncation).
- **panproto-expr**: convenience constructors `Expr::int_to_float()`, `float_to_int()`, etc.
- **panproto-inst**: `FieldTransform::ApplyExpr` and `ComputeField` carry `inverse` and `coercion_class` fields.
- **panproto-inst**: `FieldTransform::coercion_class()` and `CompiledMigration::coercion_class()` methods.
- **panproto-lens**: `Complement::compose()` monoid method for composing value-level losses.
- **panproto-lens**: `Complement::original_values` field captures pre-coercion `node.value` for leaf node coercions.
- **panproto-lens**: `Lens::coercion_class()` method.
- **panproto-lens**: `ComplementConstructor::CoercedSortData` with Lawvere metric cost integration.
- **panproto-lens**: `put()` falls back to inverse expression evaluation when complement lacks snapshots.
- **panproto-mig**: `compile()` generates `field_transforms` from schema coercions when vertex kinds change.
- **panproto-check**: `CoercionClassDowngraded` and `CoercionRemoved` breaking change variants.
- **panproto-parse**: per-language feature forwarding (`lang-python`, `lang-rust`, etc.) for all 248 grammars.

### Fixed

- **panproto-gat**: colimit renames sort references in dependent sort parameters and operation signatures for non-shared T2 sorts, preventing dangling references in the pushout.
- **panproto-gat**: colimit sort/operation compatibility check now compares `SortKind`, not just arity.
- **panproto-gat**: pullback preserves `SortKind` on paired sorts and composes `coercion_class` from both sides of directed equation pairings.
- **panproto-expr**: `FloatToInt` uses safe `float_to_i64` helper (rejects NaN, infinity, out-of-range).
- **panproto-expr**: `Neg`/`Abs` use `checked_neg()`/`checked_abs()` to avoid overflow panic on `i64::MIN`.
- **panproto-expr**: `Floor`/`Ceil`/`Round` validate finiteness and range before casting.
- **panproto-expr**: `Slice` uses character-offset indexing instead of byte offsets, preventing panics on multi-byte UTF-8.
- **panproto-expr**: `Clamp` validates `min <= max` instead of panicking.
- **panproto-inst**: `wtype_extend` (Sigma) and `wtype_pi` (Pi) now apply `field_transforms`, matching `wtype_restrict` (Delta).
- **panproto-inst**: `apply_field_transforms` handles `"__value__"` key by targeting `node.value` for leaf node coercions.
- **panproto-lens**: `get()` captures pre-coercion `node.value` in `Complement::original_values` for `__value__` transforms, fixing GetPut law violation.
- **panproto-lens**: `instances_equivalent` compares `extra_fields`, closing a blind spot in lens law verification.
- **panproto-lens**: `apply_rename_sort_to_schema` renames vertex kinds (not IDs), plus coercion keys, edge kinds, and other schema references.
- **panproto-lens**: `apply_drop_sort_from_schema` removes dangling coercions, mergers, defaults, and policies.
- **panproto-schema**: `schema_pushout` merges coercions, mergers, defaults, and policies from both input schemas.
- **panproto-schema**: `normalize` filters coercions against surviving vertex kinds and policies against surviving vertex IDs.
- **panproto-wasm**: `schema_add_coercion` inserts a `CoercionSpec` into the schema instead of creating a rename protolens.
- **panproto-vcs**: fix `ours_polsition`/`theirs_polsition` typo in `MergeConflict::BothModifiedOrdering`.
- **panproto-grammars**: fix 5 missing grammars (circom, fidl, postscript, prolog, qml).
- **panproto-grammars**: fix `janet` and `vb` c_symbol mismatches.
- **panproto-grammars**: compile C++ scanners with `-fno-exceptions -fno-rtti`.

## [0.18.1] - 2026-03-28

### Fixed

- **panproto-lens**: `instances_equivalent` now compares arcs and fans, not just nodes. Previously, two instances with identical nodes but different tree structure would pass as equivalent, making `GetPut`/`PutGet` law verification incomplete.
- **panproto-gat**: `colimit()` equation merging now applies the operation rename map to T2's equation terms before alpha-equivalence comparison. Previously, equations referencing renamed operations would produce spurious conflict errors.
- **panproto-gat**: `TheoryMorphism::compose()` now returns `Result` and errors when a codomain element has no mapping in the second morphism, instead of silently falling back to identity.
- **panproto-gat**: `colimit()` now verifies the cocone condition (`j1 ∘ i1 = j2 ∘ i2`) at construction time.
- **panproto-lens**: `Complement` now records exact arc edge selections during `get` (via `arc_edges` field), making `put` fully deterministic when the source schema has parallel edges. This ensures the cartesian lift in the Grothendieck fibration is provably unique.
- **panproto-gat**: `free_model()` now returns `FreeModelResult` with an `is_complete` flag indicating whether the depth bound was sufficient for the model to be truly initial.

### Added

- **panproto-gat**: `ColimitResult::verify_cocone()` method for explicit cocone condition verification.
- **panproto-gat**: `FreeModelResult` type wrapping the model with completeness status.
- **panproto-gat**: `GatError::ComposeUnmapped` variant for morphism composition failures.
- **panproto-vcs**: `ConflictResolution` and `ResolutionStrategy` types for user-provided merge conflict resolutions.
- **panproto-vcs**: `apply_resolutions()` applies user resolutions to a merge result, producing a `ResolvedMerge` with re-derived migrations.
- **panproto-vcs**: `verify_pushout()` checks migration totality and cocone condition on a resolved merge, enabling provably correct pushouts when all conflicts are resolved.

## [0.18.0] - 2026-03-28

### Added

- **panproto-gat**: morphism-based `colimit()` returning `ColimitResult` with inclusion morphisms `j1: T1 → P` and `j2: T2 → P`, satisfying the universal property of pushouts. The previous name-based function is available as `colimit_by_name()`.
- **panproto-gat**: `TheoryMorphism::identity()` and `compose()` methods with associativity and identity unit laws.
- **panproto-gat**: `Theory::inclusion_into()` for building trivial inclusion morphisms by name matching.
- **panproto-gat**: congruence closure in free model construction ensures the quotient is complete, and operations now map carrier elements to carrier elements (true initiality).
- **panproto-lens**: `Complement` tracks `original_extra_fields` for nodes with field transforms, enabling correct `GetPut` round-trips for lenses with value-level coercions.
- **panproto-lens**: `Complement` includes `source_fingerprint` for provenance validation in `put()`.
- **panproto-mig**: hyper-edge bijectivity check in `invert()`.
- **panproto-mig**: `compose()` now propagates `expr_resolvers` through the vertex map instead of discarding them.
- **panproto-vcs**: merge commits now carry forward `data_ids` and `complement_ids` from both parent commits.
- **panproto-vcs**: enrichment maps (`coercions`, `mergers`, `defaults`, `policies`) are now three-way merged during VCS merge instead of being discarded.
- Integration tests for categorical laws: morphism composition associativity, identity unit, colimit associativity, restrict functor contravariance.

### Fixed

- **panproto-gat**: colimit equation conflict check now uses alpha-equivalence instead of structural comparison, matching the directed equation check.
- **panproto-gat**: `check_morphism()` validates `SortKind` compatibility and dependent sort parameter preservation under the sort mapping.
- **panproto-gat**: `horizontal_compose` validates that G's codomain equals H's domain before composing.
- **panproto-gat**: free model topological sort uses FIFO (Kahn's algorithm) instead of LIFO.
- **panproto-lens**: `OpticKind` partial order is now correct: `Lens` and `Prism` are incomparable (previously derived as `Lens < Prism`).
- **panproto-lens**: optic and fibration law checks use structural equivalence (`instances_equivalent`) instead of node-count comparison.
- **panproto-lens**: `compose_compiled_migrations` propagates `field_transforms` and `conditional_survival` instead of discarding them.
- **panproto-inst**: `wtype_restrict` and `restrict_with_complement` now apply `conditional_survival` and `field_transforms` to the root node (previously skipped).
- **panproto-mig**: `check_order_compatibility` remaps edges through the migration's `edge_map` before comparing.
- **panproto-mig**: `check_reachability` uses full BFS from root vertices instead of one-hop parent check.
- **panproto-vcs**: edge merge detects delete-modify conflicts (one side removes an edge while the other modifies its ordering or usage mode) and emits `MergeConflict::DeleteModifyEdge`.

### Removed

- **panproto-inst**: removed `reachable_from_root()` (semantically incorrect for non-surviving intermediates; the fused `wtype_restrict` pipeline handles this correctly).
- **panproto-mig**: removed `ComposeError::VertexNotInDomain` (was never constructed; partial-map semantics silently drop unmapped vertices).

## [0.17.3] - 2026-03-27

### Fixed

- **panproto-grammars**: link the C++ standard library (`libstdc++` on Linux, `libc++` on macOS) when any grammar uses a C++ scanner (`scanner.cc`). Without this, Linux builds fail with `relocation refers to a symbol in a discarded section: DW.ref.__gxx_personality_v0`. Affects ~20-30 grammars including C++, TypeScript, HTML, PHP, Haskell, and Ruby.

## [0.17.2] - 2026-03-27

### Fixed

- **panproto-grammars**: fix duplicate-symbol linker errors when multiple tree-sitter grammars define identically-named internal C functions (e.g., `scan_comment` in 16+ grammars). After compiling each grammar's static library, `build.rs` now localizes non-`tree_sitter_*` symbols using `ld -r -exported_symbol` (macOS) or `objcopy --keep-global-symbol` (Linux).

## [0.17.1] - 2026-03-27

### Fixed

- **panproto-grammars**: set `publish = false` (vendored C sources exceed crates.io's 10MB limit). Yanked the empty 0.17.0 crate. Removed the silent empty-table fallback in `build.rs`.
- **panproto-parse**: `panproto-grammars` dependency is now optional behind the `grammars` feature (default on). Without it, `ParserRegistry::new()` returns an empty registry that users populate via `register()` with individual grammar crates. This makes `panproto-parse` publishable to crates.io independently.
- **panproto-project**: `detect_language()` now delegates to `ParserRegistry` instead of depending on `panproto-grammars` directly. Removed the direct `panproto-grammars` dependency.
- Updated all READMEs to reflect the grammar architecture.

## [0.17.0] - 2026-03-27

### Added — 248 Tree-Sitter Language Support

- **panproto-grammars** (new crate): pre-compiled tree-sitter grammars for 248 languages, vendored from C sources. Each grammar is feature-gated (`lang-python`, `lang-rust`, etc.) with group features (`group-core`, `group-web`, `group-all`). Default is `group-core` (Python, JavaScript, TypeScript, Java, C#, C++, PHP, Bash, C, Go, Rust). `build.rs` compiles all C/C++ sources via the `cc` crate. Grammars that extend other grammars (Angular extending HTML, etc.) are made self-contained at fetch time.
- **tools/fetch-grammars.py**: script to fetch grammar sources from git repos based on `grammars.toml`. Runs `tree-sitter generate` when `parser.c` is missing. Copies all source files, headers, and subdirectories to make each grammar self-contained. Resolves cross-grammar header dependencies. Verifies permissive licensing.
- **grammars.toml**: manifest of all 248 tree-sitter languages with repo URLs, file extensions, C symbol names, and subdirectory overrides.

### Changed — Unified Tree-Sitter Architecture

- **panproto-parse**: replaced 10 per-language wrapper files with a single data-driven `ParserRegistry::new()` that loops over `panproto_grammars::grammars()`. Custom `WalkerConfig` overrides consolidated into `walker_configs.rs`. Removed `LanguageParser::new()` (which took `LanguageFn`); use `from_language()` directly.
- **panproto-project**: `detect.rs` now delegates to `panproto_grammars::extension_to_language()` instead of a hand-maintained match statement.
- **tree-sitter**: bumped from 0.24 to 0.25 for ABI version 15 support (required by latest grammars).

### Removed — Protocol Implementations Replaced by Tree-Sitter

- **panproto-protocols**: deleted 26 hand-written protocol parsers now covered by tree-sitter grammars: all 8 type system protocols (Python, TypeScript, Rust, Java, Go, Swift, Kotlin, C#), SQL, GraphQL, HCL, Protobuf, Thrift, Cap'n Proto, JSON Schema, YAML Schema, TOML Schema, INI Schema, CSV/Table Schema, CSS, HTML, JSX, Markdown, Svelte, Vue, XML/XSD.
- **panproto-protocols**: deleted 29 unused theory building-block functions. Kept 5 public GATs (ThGraph, ThConstraint, ThMulti, ThWType, ThMeta) and 6 registration helpers still used by remaining semantic protocols.

### Fixed — Mathematical Correctness Review

- **panproto-gat** (F1): equation preservation in `check_morphism()` now uses α-equivalence instead of syntactic equality, correctly treating universally quantified variable names as bound. Added `alpha_equivalent()` and `alpha_equivalent_equation()` to `Term`. Pullback equation pairing in `pair_eqs()` also updated.
- **panproto-gat** (F2): naturality square verification in `check_natural_transformation()` now normalizes both sides via the codomain's directed equations (rewrite rules) before comparison. Added `match_pattern()` for first-order pattern matching and `normalize()` for innermost-first term rewriting to fixed point. Naturality checks that depend on the codomain's equational theory no longer produce spurious violations.
- **panproto-gat** (F3): theory colimit (`colimit()`) now propagates directed equations and conflict policies from both input theories. Previously these were silently dropped, causing composed protocols to lose rewrite rules essential to the edit lens pipeline. Conflict detection uses α-equivalence for directed equation compatibility. Added `DirectedEqConflict` and `PolicyConflict` error variants.
- **panproto-gat** (F4): `check_morphism()` now verifies that directed equations are preserved under the morphism. For each domain directed equation, the mapped terms must appear as a directed equation in the codomain (checked via α-equivalence). Added `DirectedEquationNotPreserved` error variant.
- **panproto-gat** (F5): pullback construction now pairs directed equations from both source theories when they agree in the codomain (via α-equivalence). Added `pair_directed_eqs()` following the same pattern as `pair_eqs()`. The pullback theory uses `Theory::full()` to include paired directed equations.
- **panproto-gat** (F6): free model construction now topologically sorts the theory's sorts by dependency, ensuring parameter sorts are populated before dependent sorts. Added `topological_sort_sorts()`. Term generation iterates in dependency order so dependent sorts like `Hom(a: Ob, b: Ob)` correctly find terms for their parameter sorts.
- **panproto-lens** (F7): added `SymmetricLens::verify_complement_coherence()` to verify that round-tripping through one direction does not disturb the complement of the other direction (Hofmann-Pierce-Wagner complement coherence condition). Returns a list of `CoherenceViolation`s. Previously, complement coherence was only tested on identity lenses.
- **panproto-mig** (F8): added functoriality integration tests for `lift_wtype_sigma()` (left Kan extension). Tests verify that lifting along a composed migration equals sequential lifting (`Σ(m2 ∘ m1, I) = Σ(m2, Σ(m1, I))`), and that identity migration preserves instances.
- **panproto-lens** (F9): documented that `classify_transform()` assumes elementary transforms are lawful by construction. Added `check_optic_laws()` for runtime verification that the classified optic kind's laws hold on concrete instances, and `OpticLawViolation` error type.

### Added — Mathematical Correctness Enhancements

- **panproto-gat** (F10): added `check_interchange()` to verify the interchange law for natural transformation compositions, the fundamental coherence condition for 2-categories: `(β' • α') * (β • α) = (β' * β) • (α' * α)`. Compares both sides component-wise using α-equivalence.
- **panproto-lens** (F11): added `fibration` module formalizing the Grothendieck fibration structure underlying the protolens framework. `Fibration` trait with `cartesian_lift` (put) and `opcartesian_lift` (get). `WTypeFibration` implementation connecting to Johnson-Rosebrugh delta lenses. `verify_cartesian_universal()` checks the universal property (reduces to get-put/put-get laws).
- **panproto-lens** (F12): formalized the complement cost model as a Lawvere metric space `([0, ∞], ≥, +)`. Added `verify_identity_cost()`, `verify_subadditivity()` to `cost` module, and `LensGraph::verify_metric()` checking identity and triangle inequality axioms on the distance matrix. Added `MetricViolation` enum. Expanded module documentation connecting the enrichment structure to the "shortest path = minimal information loss" heuristic.

### Added — XRPC Remote Operations and Git Remote Helper

- **panproto-xrpc** (new crate): XRPC client for cospan node VCS operations. Implements all `dev.cospan.node.*` endpoints (getObject, putObject, getRef, setRef, listRefs, getHead, negotiate, getRepoInfo). High-level `push()` and `pull()` methods handle full have/want negotiation. Auth via Bearer token.
- **git-remote-cospan** (new crate): Git remote helper binary enabling `git clone cospan://did/repo`, `git push cospan main`, `git pull cospan`. Implements the git remote-helper stdin/stdout protocol (capabilities, list, fetch, push). Fetch exports panproto objects to git via `panproto-git::export_to_git`. Push imports git objects via `panproto-git::import_git_repo`.
- **panproto-cli**: `push`, `pull`, `fetch`, `clone` commands now use `panproto-xrpc::NodeClient` for remote operations against cospan nodes via `cospan://` URLs.

## [0.16.0] - 2026-03-26

### Added — Full-AST Parsing, LLVM Integration, Git Bridge

- **panproto-parse** (new crate): Tree-sitter full-AST parsing for 10 languages (TypeScript, TSX, Python, Rust, Java, Go, Swift, Kotlin, C#, C, C++) with auto-derived GAT theories from grammar metadata. Generic `AstWalker` uses node kinds as vertex kinds and field names as edge kinds. Interstitial text capture enables exact source round-trip (`emit(parse(source)) == source`). `ParserRegistry` with language detection by file extension. `IdGenerator` for scope-aware vertex IDs.
- **panproto-project** (new crate): Multi-file project assembly via schema coproduct. `ProjectBuilder` parses all files in a directory, detects languages, and produces a unified `ProjectSchema` with path-prefixed vertex IDs and cross-file edge support. Falls back to `raw_file` protocol for non-code files and ABI-incompatible grammars.
- **panproto-git** (new crate): Bidirectional git to panproto-vcs translation bridge. `import_git_repo` walks the git commit DAG topologically, parses each commit's file tree via `panproto-project`, and creates panproto-vcs commits preserving authorship, timestamps, and parent structure. `export_to_git` creates nested git trees from panproto schemas with proper directory hierarchy.
- **panproto-llvm** (new crate): LLVM IR protocol definition (31 vertex kinds, 13 edge rules, 22 constraint sorts, 56 instruction opcodes). Theory morphisms lowering TypeScript, Python, and Rust ASTs to LLVM IR (compilation as structure-preserving maps). inkwell-based LLVM IR text parser (`parse_llvm_ir`) tested against LLVM 20.1.1.
- **panproto-jit** (new crate): LLVM JIT compilation of panproto expressions via inkwell. `JitCompiler` compiles `Expr` ASTs to native code: arithmetic, comparison, boolean, type coercions, rounding (correct floor/ceil via comparison+adjust), let bindings, pattern matching with literal and wildcard patterns. Compilation mapping classifies all 50 builtins. Tested against LLVM 20.1.1.
- **panproto-protocols**: `raw_file` protocol for non-code files (text as ordered line vertices, binary as chunk vertices with blake3 content hash). `ThImport` building-block theory for cross-file edges. `register_full_ast_wtype()` composes auto-derived theories with structural modifiers via colimit (returns `Result` for proper error propagation).
- **panproto-core**: Feature-gated re-exports for new crates (`full-parse`, `project`, `git`, `llvm`, `jit`).
- **panproto-cli**: `schema parse file`, `schema parse project`, `schema parse emit` subcommands for full-AST parsing. `schema git import`, `schema git export` subcommands for git bridge.
- **panproto-py**: PyO3 bindings for `AstParserRegistry`, `ProjectBuilder`, `ProjectSchema`, `git_import()`, and convenience functions `parse_source_file()`, `parse_project()`, `build_project()`. New exception types `ParseError`, `ProjectError`, `GitBridgeError`.

## [0.15.0] - 2026-03-25

### Added — Edit Lenses and CLI Restructure

- **panproto-inst**: `TreeEdit` enum (11 variants) and `TableEdit` enum (5 variants) implementing the edit monoid from Hofmann, Pierce, Wagner 2012. Full `apply` (partial monoid action on `WInstance`/`FInstance`), `identity`, and `compose` operations.
- **panproto-inst**: `ReachabilityIndex` for incremental reachability tracking. Supports `insert_edge`/`delete_edge` with BFS cascading in time proportional to the affected subtree.
- **panproto-inst**: `ContractionTracker` for incremental ancestor contraction bookkeeping. Records and undoes contractions with children and edge preservation.
- **panproto-gat**: `th_editable_structure()` building-block theory with sorts `State`/`Edit` and monoid action equations.
- **panproto-lens**: `EditLens` struct with `get_edit`/`put_edit` for incremental edit translation through migrations. Supports structural remap, field transforms, conditional survival predicates, and complement policy dispatch.
- **panproto-lens**: `EditPipeline` mirroring the five steps of `wtype_restrict` incrementally (anchor survival, reachability, ancestor contraction, edge resolution, fan reconstruction).
- **panproto-lens**: Edit lens law verification (`check_edit_consistency`, `check_complement_coherence`).
- **panproto-lens**: Optics dispatch (`optic_kind()`) for Iso/Lens/Prism/Affine translation strategies.
- **panproto-lens**: `EditProvenance` for tracking which translation rules fired per edit.
- **panproto-lens**: Refinement type checking against target schema constraints during `get_edit`.
- **panproto-lens**: `Protolens::instantiate_edit` and `ProtolensChain::instantiate_edit` for producing `EditLens` from protolens specifications.
- **panproto-vcs**: `EditLogObject` (9th object type) for content-addressed edit log storage. `edit_log_ids` on `CommitObject`.
- **panproto-vcs**: `incremental_migrate`, `encode_edit_log`, `decode_edit_log` for edit stream processing.
- **panproto-cli**: `schema lens` restructured into 7 subcommands: `generate`, `apply`, `compose`, `verify`, `inspect`, `check`, `lift`.
- **panproto-cli**: `schema data` group with 4 subcommands: `migrate`, `convert`, `sync`, `status`. `schema data sync --edits` stores `EditLogObject` in the VCS.
- **Tutorial**: Chapter 23, "Edit Lenses: Incremental Migration".
- **Dev-guide**: Chapter 31, "Edit Lens Internals".

### Changed

- **panproto-cli**: `schema migrate` moved to `schema data migrate`. `schema convert` moved to `schema data convert`. `schema status --data` moved to `schema data status`.
- **panproto-cli**: `schema lens` monolithic command (16+ flags) replaced by subcommands. `schema lens old.json new.json --protocol p` becomes `schema lens generate old.json new.json --protocol p`.
- **panproto-inst**: `apply_field_transforms` promoted from `pub(crate)` to `pub` for use by edit lens translation.
- **Python SDK**: README rewritten for the native PyO3 API.

### Removed

- **panproto-cli**: Top-level `schema migrate` and `schema convert` commands (replaced by `schema data` subcommands).
- **panproto-cli**: Monolithic `schema lens` flag-based dispatch (replaced by subcommands).

## [0.14.0] - 2026-03-24

### Added — Native Python Bindings (panproto-py)

- **panproto-py** (new crate): Native Python bindings via PyO3/maturin, replacing the WASM-based Python SDK. Compiles to a `cdylib` (`panproto._native`) with `pythonize` for zero-overhead serde to Python dict conversion. No wasmtime or msgpack dependencies. Produces platform-specific wheels (abi3-py313).
- **panproto-py**: 13 Rust modules wrapping all panproto sub-crates: `schema.rs` (Protocol, Schema, SchemaBuilder, Vertex, Edge, Constraint, HyperEdge), `protocols.rs` (76 built-in protocols), `mig.rs` (Migration, MigrationBuilder, CompiledMigration, compile/compose/invert/check_existence/check_coverage), `check.rs` (SchemaDiff, CompatReport, diff/classify), `inst.rs` (Instance W-type, from_json/to_json/validate), `io.rs` (IoRegistry with 76 codecs), `lens.rs` (Lens, Complement, get/put/check_laws/auto_generate), `gat.rs` (Theory, Model, create/colimit/check_morphism/migrate/free_model/check_model), `expr.rs` (Expr, parse_expr/pretty_print), `vcs.rs` (VcsRepository), `error.rs` (10 exception classes).
- **Python SDK**: `__init__.py` now directly re-exports from `_native`. Deleted 18 wrapper modules (~9,200 lines), the bundled WASM binary, and the wasmtime/msgpack dependencies.
- **Python SDK**: 76 tests covering all modules (protocol registry, schema builder, schema properties, diff/classify, migration, IoRegistry, expressions, GAT, VCS, error hierarchy, vertex/edge/constraint types). All pass in 0.14s.
- **Python SDK**: mkdocs documentation with 9 pages (index, schemas, migrations, lenses, I/O, GAT, VCS, expressions, API reference), LaTeX math for category-theoretic concepts, mkdocs-material theme.
- **CI**: Python job updated from WASM to native `maturin develop` with ubuntu/macos matrix.
- **CI**: New `publish-python.yml` workflow for PyPI wheel builds via `PyO3/maturin-action` (linux x86_64/aarch64, macos x86_64/aarch64, windows x86_64).

### Changed

- **Python SDK**: The `panproto` PyPI package now ships native compiled extensions instead of a bundled WASM binary. `import panproto` works identically; the public API surface is preserved. `WasmError` is a deprecated alias for `PanprotoError`.
- **Python SDK**: `SchemaBuilder` methods (`vertex`, `edge`, `hyper_edge`, `constraint`) now mutate in place instead of returning new immutable copies. Call `build()` to produce the final `Schema`.

### Removed

- **Python SDK**: Deleted `_wasm.py`, `_msgpack.py`, `_panproto.py`, `_schema.py`, `_protocol.py`, `_migration.py`, `_check.py`, `_instance.py`, `_io.py`, `_lens.py`, `_gat.py`, `_vcs.py`, `_errors.py`, `_data.py`, `_enrichment.py`, `_coverage.py`, `_expr.py`, `_protolens.py`, `_types.py`, and the bundled `panproto_wasm_bg.wasm`.
- **Python SDK**: Removed `wasmtime>=29.0.0` and `msgpack>=1.1.0` dependencies.

## [0.13.0] - 2026-03-23

### Added — Expression Parser, Polynomial Queries, and Lens Graphs

- **panproto-expr-parser** (new crate): Haskell-style surface syntax parser for panproto expressions. Logos-based lexer with GHC-style layout insertion (Indent/Dedent/Newline virtual tokens), Chumsky 1.0 recursive-descent + Pratt precedence parser producing `Expr`, and precedence-aware pretty printer with minimal parenthesization. Public API: `tokenize()`, `parse()`, `pretty_print()`. 50+ token kinds, structured error types with source spans.
- **panproto-inst**: `fiber_at_node` — instance-aware fiber at a specific target node (lifts `fiber_at_anchor` with node-level context).
- **panproto-inst**: `restrict_with_complement` — restriction pipeline that tracks complement data (`Complement`, `DroppedNode`) for backward migration reconstruction.
- **panproto-inst**: `section` — section construction (right inverse of projection) with `SectionEnrichment` specification.
- **panproto-inst**: `hom_schema` — internal hom schema `[S, T]` for two schemas. `curry_migration` — curry a migration into the internal hom. `eval_hom` — evaluate a curried migration at a specific instance.
- **panproto-inst**: `InstanceQuery` / `execute` — declarative query engine with anchor selection, predicate filtering, path navigation, group_by, projection, and limits.
- **panproto-inst**: `eval_with_instance` — instance-aware expression evaluation with graph traversal builtins (Edge, Children, HasEdge, EdgeCount, Anchor).
- **panproto-inst**: `fiber_at_anchor` / `fiber_decomposition` / `fiber_with_predicate` — polynomial functor operations: preimage of a migration at target anchors, full fiber decomposition, and predicate-filtered fibers.
- **panproto-inst**: `group_by` / `join` — instance partitioning and pullback operations.
- **panproto-lens**: `complement_cost` / `chain_cost` — Lawvere metric cost computation for complement constructors and protolens chains. Identity has cost 0; composition satisfies the triangle inequality.
- **panproto-lens**: `LensGraph` — weighted lens graph with Floyd-Warshall shortest path computation. `preferred_path` finds the minimum-cost conversion path between schemas. `distance` returns the shortest distance.
- **panproto-wasm**: `parse_expr` — tokenize and parse Haskell-style expression source text, return MsgPack-encoded `Expr`.
- **panproto-wasm**: `eval_func_expr` — evaluate a MsgPack-encoded expression with environment bindings.
- **panproto-wasm**: `execute_query` — run a declarative query against an instance, return MsgPack-encoded results.
- **panproto-wasm**: `fiber_at` / `fiber_decomposition_wasm` — fiber operations at the WASM boundary.
- **panproto-wasm**: `poly_hom` — internal hom schema construction via WASM.
- **panproto-wasm**: `preferred_conversion_path` / `conversion_distance` — lens graph shortest path and distance queries.
- **panproto-cli**: `schema expr parse` — parse a Haskell-style expression and print the AST. `schema expr eval` — evaluate an expression. `schema expr fmt` — pretty-print in canonical form. `schema expr check` — validate syntax.
- **panproto-cli**: `schema expr gat-eval` — evaluate a JSON-encoded GAT term from a file. `schema expr gat-check` — type-check a JSON-encoded GAT term against a theory.
- **panproto-vcs**: `store_expr` / `load_expr` — content-addressed expression storage and retrieval. `Object::Expr` — first-class VCS object type for expressions.
- **panproto-protocols**: `ThExpr` registered as a GAT theory (#32).
- **@panproto/core**: `parseExpr`, `evalExpr`, `formatExpr` — expression parsing, evaluation, and formatting. `executeQuery` with `InstanceQuery` / `QueryMatch` types. `fiberAt` / `fiberDecomposition` — fiber operations. `polyHom` — internal hom. `preferredPath` / `distance` with `GraphEdge` / `PreferredPath` types — lens graph queries.

### Fixed

- **panproto-inst**: Fix `surviving_verts` membership check in `wtype_restrict` for vertices participating in fiber decomposition; fibers that span renamed vertices now survive correctly.
- **panproto-inst**: Fix root survival check in `wtype_restrict` when the schema root is mapped through `vertex_remap`; the remapped root is now always added to the survival set.

## [0.12.0] - 2026-03-21

### Added — Value-Dependent Migration via Expression Language

Five new `FieldTransform` variants and one new `CompiledMigration` field that extend the migration pipeline from purely structural operations to value-dependent decisions, using panproto-expr as the evaluation engine.

- **panproto-inst**: `FieldTransform::PathTransform { path, inner }` — lifts a field transform to operate at a nested path within the Value tree. This is the action of a path functor on the endomorphism algebra of field transforms.
- **panproto-inst**: `FieldTransform::MapReferences { field, rename_map }` — updates string values carrying vertex identity when vertices are renamed or dropped. Functorial action of the vertex rename map on the name-reference algebra.
- **panproto-inst**: `FieldTransform::ComputeField { target_key, expr }` — evaluates an expression with ALL `extra_fields` bound as variables, storing the result. Enables template name computation: `(concat "h" (int_to_str attrs.level))` → `"h2"`.
- **panproto-inst**: `FieldTransform::Case { branches: Vec<CaseBranch> }` — the coproduct eliminator for the field transform algebra. `Π(x : Value). FieldTransform` — a dependent function from node values to transform sequences. Branches are evaluated in order; the first matching predicate's transforms are applied.
- **panproto-inst**: `CaseBranch { predicate: Expr, transforms: Vec<FieldTransform> }` — a branch in a Case analysis.
- **panproto-inst**: `CompiledMigration::conditional_survival: HashMap<Name, Expr>` — value-dependent survival predicates. Refines the survival predicate from structural (vertex set membership) to value-dependent (membership AND expression predicate).
- **panproto-inst**: Builder methods: `add_path_transform`, `add_map_references`, `add_computed_field`, `add_case_transform`, `add_conditional_survival`.
- **panproto-inst**: `build_env_from_extra_fields` helper — binds both flat keys and `attrs.*` qualified keys for complete variable coverage in expression evaluation.

### Changed

- **panproto-inst**: `value_to_expr_literal` now serializes encoded arrays (`Value::Unknown` with `__array_len` sentinel) as comma-separated strings, enabling the `Contains` builtin to check array membership in Case predicates.
- **panproto-inst**: `build_env_from_extra_fields` binds ALL extra_fields as both flat keys and `attrs.*` qualified keys, plus nested attrs entries as flat keys if not already present.

## [0.11.0] - 2026-03-20

### Added — Universal Lexicon Parsing & Cross-Lexicon Morphism Discovery

- **panproto-wasm**: `parse_atproto_lexicon(json_bytes)` — WASM export that parses any ATProto lexicon JSON into a schema handle. Works for Bluesky, RelationalText, Layers, and any custom lexicon. Foundation for browser-side morphism-first integration.
- **panproto-wasm**: `schema_metadata(handle)` — WASM export that extracts vertex/edge metadata from a schema handle as MessagePack.
- **@panproto/core**: `Panproto.parseLexicon(json)` — TypeScript method that parses an ATProto lexicon JSON into a `BuiltSchema`. Enables the full `parseLexicon → lens → convert` workflow entirely in the browser.
- **panproto (Python)**: `Panproto.parse_lexicon(json)` — Python equivalent of the TypeScript `parseLexicon`.
- **panproto-lens**: `derive_field_transforms(chain, src, tgt)` — automatically derives `FieldTransform` entries from a protolens chain's elementary steps. `RenameOp → RenameField`, `DropOp → DropField`, `AddDirectedEquation → ApplyExpr`. Called automatically by `auto_generate`.
- **panproto-lens**: `auto_generate` now populates `lens.compiled.field_transforms` automatically from the protolens chain, eliminating the need for manual `inject_field_transforms` calls.
- **panproto-lens**: Overlap-based fallback in `auto_generate` — when `config.try_overlap` is true and the direct morphism has quality < 0.5, `discover_overlap` finds shared substructure and uses it as alignment hints for a constrained re-search.

### Changed

- **panproto-mig**: Morphism search quality scoring now has four components (was two): name similarity (0.25), edge name preservation (0.25), property-name Jaccard similarity (0.3), and degree similarity (0.2). The property-name component rewards structural alignment — vertices with matching child property names (e.g., both have `byteStart`/`byteEnd`) score much higher.
- **panproto-mig**: Domain pruning for "object" vertices — when domain size > 5, restrict to target vertices sharing ≥1 outgoing edge name. Anchors alignment on shared structure and dramatically reduces combinatorial explosion for cross-lexicon morphisms.

## [0.10.0] - 2026-03-20

### Added — Value-Level Field Transforms

- **panproto-inst**: `FieldTransform` enum — value-level operations on node `extra_fields` applied during `wtype_restrict`. Variants: `RenameField`, `DropField`, `AddField`, `KeepFields`, `ApplyExpr`. These enable the instance pipeline to handle attribute renames, drops, additions, and expression-evaluated value transforms that go beyond pure structural schema changes.
- **panproto-inst**: `CompiledMigration` builder API — `add_field_rename(vertex, old_key, new_key)`, `add_field_drop(vertex, key)`, `add_field_default(vertex, key, value)`, `add_field_keep(vertex, keys)`, `add_field_expr(vertex, key, expr)`. These are the stable API that protocol integrations use to inject value-level transforms into the migration pipeline.
- **panproto-inst**: `FieldTransform` re-exported from crate root.
- **panproto-inst**: `panproto-expr` added as a dependency (for expression evaluation in `ApplyExpr`).

### Changed

- **panproto-inst**: `CompiledMigration` gains `field_transforms: HashMap<Name, Vec<FieldTransform>>` field. Default is empty (backward compatible via `#[serde(default)]`).
- **panproto-inst**: `wtype_restrict` applies field transforms to surviving nodes after structural operations (anchor remapping, vertex survival) complete. Expressions are evaluated via `panproto_expr::eval` with the field value bound as input.
- **panproto-inst**: Integer-valued floats normalized to `Value::Int` in `expr_literal_to_value` for JSON round-trip fidelity.
- **panproto-lens**: `apply_rename_sort_to_schema` now renames vertex IDs (not just kinds) and rebuilds edge references, fixing schema-level rename for instance-derived schemas.
- **panproto-lens**: `apply_drop_sort_from_schema` now matches by vertex ID or kind, fixing drops for schemas where vertex IDs and kinds diverge.
- **panproto-lens**: `compute_migration_between` adds renamed vertices to `surviving_verts` with their target names, ensuring `wtype_restrict` correctly processes renamed nodes.

## [0.9.0] - 2026-03-19

### Added — Directed Equation Protolenses

- **panproto-lens**: `elementary::directed_eq` — lax natural transformation protolens constructor for value-dependent schema migrations. Takes a `DirectedEquation` with `impl_term: Expr` and optional `inverse: Expr`. The complement captures the pre-image when the inverse is absent (lossy transform). This is the same complement-tracking mechanism used by `drop_sort` and `drop_op`.
- **panproto-lens**: `elementary::drop_directed_eq` — removes a directed equation from a theory.
- **panproto-lens**: `endofunctor_to_protolens` now handles `AddDirectedEquation` and `DropDirectedEquation` transforms (previously rejected with "value-level transforms not yet supported").
- 8 integration tests parsing real RelationalText and Layers ATProto lexicons via `atproto::parse_lexicon`.

### Changed

- **panproto-lens**: `panproto-expr` added as a dependency (for `DirectedEquation` evaluation in protolens constructors).

## [0.8.0]

### Added — Enriched Theories: Expression Language, Directed Equations, Value Sorts

- **panproto-expr** (new crate): Pure functional expression language — lambda calculus with closures, pattern matching, records, lists, ~50 built-in operations. Call-by-value evaluator with step/depth limits. Deterministic on native and WASM.
- **panproto-gat**: `DirectedEquation` — rewrite rules with `impl_term: Expr` and optional `inverse: Expr`. `SortKind` (Structural, Val, Coercion, Merger) and `ValueKind` for classifying sorts. `ConflictPolicy` with `ConflictStrategy` (KeepLeft, KeepRight, Fail, Custom). Five new `TheoryTransform` variants: CoerceSort, MergeSorts, AddSortWithDefault, AddDirectedEquation, DropDirectedEquation.
- **panproto-gat**: `AlgStruct` — algebraic struct types in theories. `EqWitness` — propositional equality proofs with justifications. `RefinedSort` — refinement types with subsort checking via interval containment.
- **panproto-schema**: Enrichment fields on `Schema`: `coercions`, `mergers`, `defaults`, `policies`. Feature flags on `Protocol`: `has_defaults`, `has_coercions`, `has_mergers`, `has_policies`. `SchemaBuilder` enrichment methods.
- **panproto-mig**: `CoverageReport` — dry-run migration with structured `RestrictError` matching. `expr_resolvers` on `Migration` for expression-based resolution.
- **panproto-lens**: `OpticKind` (Iso, Lens, Prism, Affine, Traversal) with algebraic composition table. `classify_transform` maps transforms to optic kinds. Symbolic simplification: inverse cancellation, rename fusion, add-drop cancellation. Fused complement collapse (all-Empty → Empty). Full schema-level implementations for CoerceSort, MergeSorts, AddSortWithDefault transforms.
- **panproto-inst**: `Provenance` tracking — `ProvenanceMap`, `SourceField`, `TransformStep`, `compute_provenance`.
- **panproto-protocols**: Building-block theories: ThValued, ThCoercible, ThMergeable, ThPolicied. Expression sub-theories: ThExpr, ThLam, ThMatch, ThArith, ThString, ThRecord, ThList (with round-trip equations).
- **panproto-wasm**: 11 new entry points — expr eval/check/substitute, schema enrichment (coercion, default, merger, policy), coverage analysis, optic classification, symbolic simplification, refinement subsort.
- **panproto-cli**: `schema expr eval/check/repl` — expression evaluation commands. `schema enrich add-default/add-coercion/add-merger/add-policy/list/remove` — schema enrichment management. `schema migrate --coverage` — coverage statistics. `schema diff --optic-kind` — optic classification.
- **@panproto/core**: `ExprBuilder`, `SchemaEnrichment`, `MigrationAnalysis` classes. Enriched type definitions.
- **panproto (Python)**: `ExprBuilder`, `SchemaEnrichment`, `MigrationAnalysis` classes. Full type annotations (pyright clean).
- **CI**: Added Python SDK job (pyright + pytest).
- 36 new integration tests covering all enriched theory features.

### Changed

- `Sort` gains `kind: SortKind` field (default: Structural).
- `Theory` gains `directed_eqs` and `policies` fields.
- `Schema` gains `coercions`, `mergers`, `defaults`, `policies` fields.
- `Protocol` gains `has_defaults`, `has_coercions`, `has_mergers`, `has_policies` flags.
- `Migration` gains `expr_resolvers` field.
- Lambda expressions evaluate to proper `Literal::Closure` values with captured environments.
- `ProtolensChain::fuse` collapses all-Empty complement lists to `ComplementConstructor::Empty`.

## [0.7.0]

### Added — Protolens: Automated Lens Generation via GAT Theory

- **panproto-gat**: `schema_functor` module — `TheoryEndofunctor` (functorial mappings on theories), `TheoryTransform` (11 variants: Identity, AddSort, DropSort, RenameSort, AddOp, DropOp, RenameOp, AddEquation, DropEquation, Pullback, Compose), `TheoryConstraint` (precondition predicates: Unconstrained, HasSort, HasOp, HasEquation, All, Any, Not). Endofunctors map theories via `apply()` and compose via `compose()`.
- **panproto-gat**: `factorize` module — decompose `TheoryMorphism` into dependency-ordered sequence of elementary `TheoryEndofunctor` values. Topological sort ensures dependent sorts are ordered correctly. `validate_factorization` verifies round-trip correctness.
- **panproto-lens**: `protolens` module — `Protolens` struct: a dependent function from schemas to lenses (`Π(S : Schema | P(S)). Lens(F(S), G(S))`). A `Protolens` is *not* a lens — it *produces* lenses when instantiated at a specific schema. Key operations: `instantiate(schema)` (Π-elimination producing concrete `Lens`), `applicable_to` (precondition checking). Composition via `vertical_compose` and `horizontal_compose`. `ProtolensChain` for sequential composition. 9 elementary protolens constructors in `elementary` submodule.
- **panproto-lens**: `complement_type` module — `ComplementSpec` as dependent type evaluation: given a protolens η and schema S, compute the complement type `ComplementType(η, S)`. Classifies as `Empty` (isomorphism), `DataCaptured` (lossy forward), `DefaultsRequired` (lossy backward), or `Mixed`. `DefaultRequirement` describes what the user must supply; `CapturedField` describes what's captured.
- **panproto-lens**: `auto_lens` module — `auto_generate(src, tgt, config)` pipeline: morphism search → theory morphism → factorization → protolens chain → instantiation → complement spec. Returns `AutoLensResult` with both the reusable `ProtolensChain` (schema-independent) and the concrete `Lens` + `ComplementSpec` (schema-specific).
- **panproto-lens**: `diff_to_protolens` module — convert `SchemaDiff` to `ProtolensChain`. Maps all 26 `SchemaDiff` fields to elementary protolenses. `diff_to_lens` convenience for direct `Lens` production.
- **panproto-lens**: Enhanced `SymmetricLens` — `from_protolens_chains` (span construction via two protolens chains and overlap schema), `auto_symmetric` (auto-generate from two schemas via overlap discovery).
- **panproto-cli**: 6 new commands: `convert` (one-step data conversion between schemas, `--from`/`--to`/`--defaults`/`--direction`/`--recursive`), `lens` (auto-generate lens with human-readable summary, `--json`/`--chain`/`--requirements`/`--apply`/`--verify`/`--try-overlap`), `lens-apply` (apply saved lens or protolens chain to data, `--schema`/`--direction`/`--complement`), `lens-verify` (verify lens laws + naturality, `--data`/`--naturality`), `lens-compose` (compose lenses or chains, `--chain`), `lens-diff` (derive lens from VCS commit range, `--chain`/`--requirements`/`--apply`)
- **panproto-wasm**: 10 new entry points (replacing `lens_from_combinators` + 9 new): `auto_generate_protolens`, `instantiate_protolens`, `protolens_complement_spec`, `protolens_from_diff`, `protolens_compose`, `protolens_check_naturality`, `protolens_chain_to_json`, `factorize_morphism`, `symmetric_lens_from_schemas`, `symmetric_lens_sync`. New slab resource variants: `ProtolensChainHandle`, `SymmetricLensHandle`.
- **@panproto/core**: `protolens.ts` module with `ProtolensSpec`, `ProtolensChainSpec`, `ComplementSpec` types. `ProtolensChainHandle` class (Disposable) with `autoGenerate()`, `fromDiff()`, `fromJson()`, `instantiate()`, `requirements()`, `checkNaturality()`, `compose()`, `toJson()`. `SymmetricLensHandle` class. `factorizeMorphism()` in `gat.ts`. `LensHandle.autoGenerate()` and `LensHandle.fromChain()`. Top-level: `Panproto.convert()`, `Panproto.lens()`, `Panproto.protolensChain()`.
- **panproto (Python)**: `_protolens.py` mirroring TypeScript types. `ProtolensChainHandle` with `auto_generate()`, `from_diff()`, `from_json()`, `instantiate()`, `requirements()`, `check_naturality()`, `compose()`, `to_json()`. `SymmetricLensHandle`. `factorize_morphism()`. Top-level: `Panproto.convert()`, `Panproto.lens()`, `Panproto.protolens_chain()`.
- Tutorial Part VIII "Automated Lenses": Ch. 20 "Protolenses: Schema-Independent Lens Families", Ch. 21 "Automatic Lens Generation", Ch. 22 "Symmetric Lenses and Schema Merging"
- Dev-guide Ch. 26 "Protolens Engine", Ch. 27 "Automated Lens Generation Pipeline"
- Tutorial updates: Ch. 8 (protolens forward reference), Ch. 17 (migration-to-protolens section), Appendix B (protolens API)
- Dev-guide updates: Ch. 5 (architecture diagram), Ch. 6 (factorize + schema_functor modules), Ch. 10 (protolens modules), Ch. 13 (10 new WASM entry points), Ch. 24 (morphism-to-protolens cross-ref), Appendix A (7 glossary entries), Appendix B (6 new source files)
- **panproto-lens**: `SchemaConstraint` enum — check schema structure directly (bypasses lossy implicit theory extraction). `Protolens::check_applicability()` returns human-readable failure reasons instead of a boolean.
- **panproto-lens**: `ProtolensChain::fuse()` — compose all steps into a single `Protolens` by fusing endofunctors. `instantiate()` uses the fused path for multi-step chains, avoiding intermediate schema materialization.
- **panproto-lens**: `ProtolensChain::to_json()` / `from_json()` and `Protolens::to_json()` / `from_json()` — serialize and deserialize protolens chains for cross-project reuse and policy distribution.
- **panproto-lens**: `apply_to_fleet(chain, schemas, protocol)` — apply a protolens chain to a fleet of schemas, collecting successes in `FleetResult::applied` and failures with reasons in `FleetResult::skipped`.
- **panproto-lens**: `lift_protolens(protolens, morphism)` / `lift_chain(chain, morphism)` — lift protolenses along theory morphisms for cross-protocol reuse. Composes endofunctor transforms with morphism renames and lifts preconditions.
- **panproto-lens**: `ComplementConstructor::AddedElement` — complement prediction now reports defaults required for `add_sort`/`add_op` protolenses. `chain_complement_spec` tracks intermediate schema state through the chain.
- **panproto-vcs**: `Object::DataSet` — content-addressed data snapshots binding instance data to a schema version. `DataSetObject` stores MessagePack-encoded instances with `schema_id` and `record_count`.
- **panproto-vcs**: `Object::Complement` — persistent complement storage for backward migration. `ComplementObject` stores the complement data alongside `migration_id` and `data_id` references.
- **panproto-vcs**: `Object::Protocol` — protocol (metaschema) definitions as first-class versioned objects. Pins a specific protocol version to a commit.
- **panproto-vcs**: `CommitObject` gains `protocol_id`, `data_ids`, and `complement_ids` fields, connecting commits to data snapshots, complements, and protocol versions.
- **panproto-vcs**: `data_mig` module — `migrate_forward` (data migration with complement storage), `migrate_backward` (restore from complement), `detect_staleness` (check if data needs migration), `migrate_through_path` (multi-step migration through commit DAG).
- **panproto-vcs**: `Repository::add_data(path)` — stage data files alongside schema changes. `Repository::add_protocol(protocol)` — stage protocol definitions.
- **panproto-vcs**: `Repository::checkout_with_data(target, data_dir)`, `merge_with_data(branch, author, data_dir)` — VCS operations that automatically migrate data.
- **panproto-cli**: `schema migrate <data_dir>` — migrate data to match current schema version, with `--dry-run`, `--range`, `--backward`, `-o` flags.
- **panproto-cli**: `--data` flag on `schema add`, `schema status`, `schema log`; `--migrate` flag on `schema checkout`, `schema merge`.

### Changed

- **panproto-lens**: Now depends on `panproto-check` for `SchemaDiff` → protolens conversion

### Breaking Changes

- **panproto-lens**: `chain_complement_spec` now requires `protocol: &Protocol` parameter
- **panproto-lens**: `ComplementConstructor` gains `AddedElement` variant
- **panproto-lens**: `add_sort`/`add_op` protolenses report `DefaultsRequired` complement (previously `Empty`)
- **panproto-vcs**: `CommitObject` gains required fields `protocol_id`, `data_ids`, `complement_ids` — existing serialized commits will not deserialize
- **panproto-vcs**: `Index` gains `staged_data` and `staged_protocol` fields — existing index.json will not deserialize

### Removed (Breaking)

- **panproto-lens**: `Combinator` enum and `from_combinators()` function — replaced by `Protolens` and `ProtolensChain::instantiate()`. The 14 combinator variants (RenameField, AddField, RemoveField, WrapInObject, HoistField, CoerceType, Compose, RenameVertex, RenameKind, RenameEdgeKind, RenameNsid, RenameConstraintSort, ApplyTheoryMorphism, Rename) are subsumed by 11 elementary protolens constructors in `protolens::elementary`.
- **panproto-lens**: `combinators.rs` source file — deleted, replaced by `protolens.rs`
- **panproto-wasm**: `lens_from_combinators` entry point (#25) — replaced by `auto_generate_protolens`
- **@panproto/core**: `fromCombinators()` function, combinator helper functions (`renameField`, `addField`, `removeField`, `wrapInObject`, `hoistField`, `coerceType`, `compose`, `pipeline`) — replaced by `LensHandle.autoGenerate()`, `ProtolensChainHandle`, and `Panproto.convert()`/`Panproto.lens()`
- **panproto (Python)**: `from_combinators()`, `rename_field()`, `add_field()`, `remove_field()`, etc. — replaced by `LensHandle.auto_generate()`, `ProtolensChainHandle`, and `Panproto.convert()`/`Panproto.lens()`

## [0.6.0] - 2026-03-17

### Added — GAT Engine Completeness

- **panproto-gat**: `typecheck` module — `typecheck_term`, `typecheck_equation`, `typecheck_theory`, `infer_var_sorts` for recursive type-checking of GAT terms and equations with sort inference from operation application sites
- **panproto-gat**: `check_model` module — `check_model`, `check_model_with_options` for verifying that a model satisfies a theory's equations by enumerating variable assignments over carrier sets (with configurable `max_assignments` bound)
- **panproto-gat**: `pullback` module — `pullback()` computes the categorical pullback (intersection) of two theories over a common codomain, returning projection morphisms; used in merge overlap detection
- **panproto-gat**: `nat_transform` module — `NaturalTransformation`, `check_natural_transformation`, `vertical_compose`, `horizontal_compose` for constructing and validating morphisms between theory morphisms
- **panproto-gat**: `free_model` module — `free_model()` constructs the initial model of a theory by enumerating closed terms up to configurable depth, then quotienting by equations via union-find
- **panproto-gat**: `quotient` module — `quotient()` simplifies a theory by identifying sorts/operations, with transitive closure, arity/signature compatibility checks, and equation deduplication
- **panproto-gat**: New error variants for type-checking, natural transformations, and quotient operations

### Added — Acset Parameterization

- **panproto-inst**: `AcsetOps` trait unifying `WInstance`, `FInstance`, and `GInstance` with `restrict`, `extend`, `element_count`, `shape_name` methods
- **panproto-inst**: `GInstance` graph-shaped instances with `graph_restrict` and `graph_extend` operations
- **panproto-inst**: `Instance` enum updated to dispatch restrict/extend via `AcsetOps` trait for all three shapes

### Added — VCS Integration

- **panproto-vcs**: `gat_validate` module — `validate_migration` checks vertex/edge map structural coherence against source and target schemas; `validate_theory_equations` type-checks theory equations; `validate_schema_equations` runs bounded model checking
- **panproto-vcs**: `GatDiagnostics` struct stored in `StagedSchema` during `add` — carries type errors, equation errors, and migration warnings through the staging pipeline
- **panproto-vcs**: `CommitOptions` with `skip_verify` flag — `commit_with_options` blocks commits when GAT diagnostics have errors unless `skip_verify` is set
- **panproto-vcs**: Pullback-enhanced merge — `three_way_merge` computes `PullbackOverlap` to detect shared substructure between branches, suppressing false-positive conflicts on independently-added vertices that share common origin
- **panproto-vcs**: `compose_path_with_coherence` — composition drift detection comparing the sequentially composed migration against a directly derived end-to-end migration via `auto_mig::derive_migration`, with natural transformation naturality checking when sort maps agree

### Added — CLI Commands

- **panproto-cli**: `schema scaffold` — generate minimal test data from a protocol theory using free model construction (`--depth`, `--max-terms`, `--json`)
- **panproto-cli**: `schema normalize` — simplify a schema by merging equivalent elements via theory quotient (`--identify A=B`, `--json`)
- **panproto-cli**: `schema typecheck` — type-check a migration between two schemas at the GAT level (`--src`, `--tgt`, `--migration`)
- **panproto-cli**: `schema verify` — verify that a schema satisfies its protocol theory's equations (`--max-assignments`)

### Changed — CLI Enhancements

- **panproto-cli**: `schema validate` now also type-checks protocol theory equations
- **panproto-cli**: `schema check --typecheck` flag for GAT-level migration morphism validation
- **panproto-cli**: `schema commit --skip-verify` flag to bypass GAT equation verification
- **panproto-cli**: `schema merge --verbose` flag shows pullback-based overlap detection details
- **panproto-cli**: `schema diff --theory` flag shows theory-level diff (sorts and operations)

### Documentation

- Tutorial chapter 3 updated: machine-checked equations section
- Tutorial chapter 13 updated: type-checking during add, `--skip-verify`, pullback-enhanced merge
- Tutorial chapter 14 updated: equations verified at commit time
- Tutorial chapter 17 updated: GAT-validated auto-migration
- Tutorial chapter 18 (new): "Testing with Generated Data" — `schema scaffold` walkthrough
- Tutorial chapter 19 (new): "Simplifying Schemas" — `schema normalize` walkthrough
- Dev guide chapter 6 updated: type-checking, model checking, pullbacks, natural transformations, free models, quotient theories
- Dev guide chapter 8 updated: `AcsetOps` trait section
- Dev guide chapter 9 updated: type-checked migration derivation, natural transformation coherence
- Dev guide chapter 15 updated: all new commands and flags
- Dev guide chapter 25 (new): "Type-Checking Pipeline" — flow from GAT to VCS to CLI
- README updates: safety guarantees section, new API tables (panproto-gat, panproto-vcs, panproto-cli)

## [0.5.1] - 2026-03-17

### Fixed

- **panproto-inst**: Fix `wtype_restrict` dropping renamed vertices during lift. Source vertex anchors were checked against the target `surviving_verts` set without remapping first, so a vertex mapped via `vertex_remap` (e.g. `post:text → post:content`) was silently pruned and its value lost. The anchor is now remapped to its target name before membership check.
- **panproto-check**: Fix `classify` copy-paste bug where removed edges without a matching protocol edge rule were incorrectly classified as `NonBreakingChange::AddedEdge` instead of `NonBreakingChange::RemovedEdge`. Added the missing `RemovedEdge` variant to `NonBreakingChange`.
- **panproto-wasm**: Fix `packMigrationMapping` in TypeScript SDK: JS `Map` objects with non-string keys (used for `edge_map`, `label_map`, `resolver`) were encoding as empty objects via msgpack. Now explicitly converted to `Array.from(map.entries())` to produce the `Vec<(K, V)>` format expected by Rust's `map_as_vec` serde helper.
- **panproto-wasm**: Fix WASM initialization in playground: the provider no longer overrides the glue module's `default` export with a no-op, allowing proper wasm-bindgen initialization.

### Added

- **panproto-wasm**: `lift_json`, `get_json`, `put_json` entry points that accept JSON bytes and return JSON bytes, handling all `WInstance` conversion internally. Eliminates msgpack round-trip issues at the JS/WASM boundary.
- **panproto-wasm**: `json_to_instance_with_root` entry point with explicit root vertex parameter and auto-detection fallback (prefers `object`-kind vertices, then `record`-kind).
- **@panproto/core**: `_wasm` getter on `CompiledMigration` for direct WASM module access. `_rawBytes` field on `LiftResult` for raw instance byte access. Instance-aware `lift`/`get`/`put` (detect `_bytes` field).

## [0.5.0] - 2026-03-16

### Added — Automatic Morphisms and the Adjoint Triple

- **panproto-mig**: `hom_search` module — automatic schema morphism discovery via backtracking CSP with MRV heuristic and forward checking. `find_morphisms(src, tgt, opts)` enumerates all valid schema morphisms; `find_best_morphism` returns the highest-quality one. Supports monic/epic/iso constraints and pre-assigned initial mappings. Quality scoring by name similarity + edge name preservation.
- **panproto-mig**: `overlap` module — automatic overlap discovery between two schemas via injective homomorphism search. `discover_overlap(left, right)` returns the largest shared sub-schema as a `SchemaOverlap`.
- **panproto-mig**: `chase` module — chase algorithm for enforcing embedded dependencies on functor instances. `chase_functor(instance, deps, max_iter)` iterates until fixpoint. `dependencies_from_schema` placeholder for future GAT equation translation.
- **panproto-inst**: `wtype_extend` — left Kan extension (Σ_F) for W-type instances. Pushes tree data forward along a migration, remapping anchors and edges.
- **panproto-inst**: `pi` module — right Kan extension (Π_F). `functor_pi` computes product over fibers for relational instances (with configurable size limit). `wtype_pi` handles injective migrations for W-type instances.
- **panproto-schema**: `colimit` module — schema-level pushout. `schema_pushout(left, right, overlap)` computes the minimal schema containing both inputs with shared elements merged, plus morphisms from each side into the pushout.
- **panproto-mig**: `lift_wtype_sigma`, `lift_wtype_pi`, `lift_functor_pi` — lift functions for the new adjoint functors
- Tutorial chapter 17: "Automatic Migration Discovery" — homomorphism search, adjoint triple, schema pushout, overlap discovery, chase algorithm (new Part "Automation")
- Dev-guide chapter 24: "Automatic Morphisms and the Adjoint Triple" — algorithm details, CSP reduction, performance characteristics
- Tutorial Ch. 7 updated: Σ_F/Π_F section now references implementations instead of "future work"
- Tutorial Ch. 5 updated: "Or, skip all that" section linking to automatic migration

## [0.4.0] - 2026-03-15

### Added — First-Class Names

- **panproto-gat**: `Ident` type separating stable identity (`(ScopeTag, index)`) from display name (`Arc<str>`), following GATlab (Lynch et al., 2024); `Name` type (`Arc<str>` wrapper with `Arc::ptr_eq` fast path on equality, `Deref<str>`, `Borrow<str>`, transparent serde); `NameSite` enum for the 9 naming sites; `SiteRename` for site-qualified rename operations
- **panproto-gat**: `TheoryMorphism::induce_schema_renames()` — sort-map entries become `VertexKind` renames, op-map entries become `EdgeKind` renames (top of the morphism tower)
- **panproto-lens**: 7 new combinators — `RenameVertex` (cascades to edges, constraints, variants, hyper-edges, recursion points, spans, nominal markers), `RenameKind` (single vertex), `RenameEdgeKind` (all matching edges), `RenameNsid`, `RenameConstraintSort`, `ApplyTheoryMorphism` (cascades theory morphism to vertex/edge kind renames), `Rename { site, old, new }` (unified dispatcher for any `NameSite`)
- **panproto-lens**: 3 new error variants — `NsidNotFound`, `ConstraintSortNotFound`, `EdgeKindNotFound`
- **panproto-schema**: `SchemaMorphism` type — explicit schema morphism (functor F: S → T) with vertex/edge maps, rename provenance, composition, and lowering to `CompiledMigration`
- **panproto-mig**: `cascade` module — `induce_schema_morphism` (theory → schema level), `induce_data_migration` (schema → instance level, Spivak's Δ_F), `induce_migration_from_theory` (convenience chaining both)
- **panproto-vcs**: `rename_detect` module — `detect_vertex_renames` and `detect_edge_renames` with structural similarity scoring (kind +0.3, outgoing edges +0.3, incoming edges +0.2, edit distance +0.2) and greedy bipartite matching
- **panproto-vcs**: `CommitObject.renames: Vec<SiteRename>` field for storing detected/declared renames per commit (backward-compatible via `serde(default)`)
- Tutorial chapter 15: "Solving Naming" — naming problem, 9 naming sites, identity vs name, rename combinators, morphism tower, VCS rename detection (new Part VI "Names & Identity")
- Dev-guide chapter 23: "Naming, Identity, and the Morphism Tower" — `Ident`/`Name` internals, `NameSite`/`SiteRename`, cascade module, new combinator implementation details, rename detection algorithm
- Tutorial updates: Ch. 5 (expanded renaming section), Ch. 8 (naming combinators table), Ch. 13 (rename detection with `--detect-renames`)
- Dev-guide updates: Ch. 6 (Ident/Name section + `induce_schema_renames`), Ch. 10 (naming combinators), Ch. 15 (`--detect-renames` flag), Ch. 21 (rename detection module)

### Changed — `String` → `Name` Migration

- **panproto-schema**: All identifier and label fields in `Vertex`, `Edge`, `HyperEdge`, `Constraint.sort`, `Variant`, `RecursionPoint`, `Span` changed from `String` to `Name`; all `HashMap<String, _>` keys in `Schema` changed to `HashMap<Name, _>`; `SchemaBuilder` API unchanged (still accepts `&str`, converts internally)
- **panproto-inst**: `Node.anchor`, `Node.discriminator`, `WInstance.schema_root` changed from `String`/`Option<String>` to `Name`/`Option<Name>`; `CompiledMigration` fields (`surviving_verts`, `vertex_remap`, `resolver`) changed from `String`-based to `Name`-based
- **panproto-mig**: `Migration` fields (`vertex_map`, `label_map`, `resolver`) changed from `String`-based to `Name`-based
- **panproto-protocols**: All 48 protocol emit functions updated for `Name` field access (`.to_string()` where string output required)
- **panproto-check**: `SchemaDiff` and `BreakingChange` types updated for `Name` fields
- **panproto-vcs**: All modules updated for `Name` types in schema/migration construction
- **panproto-wasm**: WASM boundary updated for `Name`-typed schema fields
- **panproto-cli**: Updated for `Name` types in diff display and schema construction

### Performance

- **panproto-inst**: `wtype_restrict` hot path gains `Arc::ptr_eq` fast path on vertex anchor equality checks (common case: both sides from same schema construction)
- **panproto-inst**: `node.anchor.clone()` is now `Arc::clone` (atomic refcount bump) instead of heap string allocation
- **panproto-schema**: All `HashMap<Name, _>` lookups accept `&str` keys via `Borrow<str>` — zero conversion cost at lookup sites

### Fixed

- **panproto-schema**: Fix JSON serialization of `HashMap<Edge, _>` and `HashMap<(String, String), _>` fields — `edges`, `orderings`, `usage_modes`, and `between` now serialize as `Vec<(K, V)>` arrays via `serde_helpers::map_as_vec`, enabling JSON round-trip for schemas with edges (previously broken: `serde_json` cannot use struct keys as JSON object keys)
- **panproto-mig**: Fix JSON serialization of `Migration` fields `edge_map`, `label_map`, `resolver`, and `hyper_resolver` using the same `map_as_vec` approach — `schema lift` now works with real schemas that have edges

### Added

- **panproto-vcs**: New library functions — `Repository::amend()`, `Repository::merge_with_options()`, `refs::force_delete_branch()`, `refs::rename_branch()`, `refs::create_and_checkout_branch()`, `refs::create_annotated_tag()`, `refs::create_tag_force()`, `stash::stash_apply()`, `stash::stash_show()`, `stash::stash_clear()`, `gc::gc_with_options()`, `cherry_pick::cherry_pick_with_options()`
- **panproto-vcs**: `Object::Tag(TagObject)` variant for annotated tags; `MergeOptions` struct (no_commit, ff_only, no_ff, squash, message); `CherryPickOptions` (no_commit, record_origin); `GcOptions` (dry_run)
- **panproto-vcs**: New error variants — `BranchNotMerged`, `OperationInProgress`, `NotImplemented`, `FastForwardOnly`, `NothingToAmend`, `TagExists`; `delete_branch` now checks merge status; `resolve_ref` peels annotated tags
- **panproto-cli**: Git-parity CLI flags across all subcommands — `init -b`, `add --dry-run/--force`, `commit --amend/--allow-empty`, `status -s/--porcelain`, `log --oneline/--graph/--all/--format/--author/--grep`, `diff --stat/--name-only/--name-status/--staged`, `show --format/--stat`, `branch -D/-m/-v`, `tag -a/-m/-f`, `checkout -b/--detach`, `merge --no-commit/--ff-only/--no-ff/--squash/--abort/-m`, `rebase --abort/--continue`, `cherry-pick -n/-x/--abort`, `reset --soft/--hard` (replaces `--mode`), `stash apply/show/clear`, `reflog --all`, `blame --reverse`, `gc --dry-run/--prune`
- **panproto-cli**: Remote command stubs (`remote`, `push`, `pull`, `fetch`, `clone`) reserved for future distributed operations
- **panproto-cli**: Output formatting module (`format.rs`) — `format_commit`, `format_commit_oneline`, `format_diff_stat`, `format_diff_name_only`, `format_diff_name_status`
- **panproto-schema**: `serde_helpers` module with `map_as_vec` and `map_as_vec_default` for JSON-compatible serialization of complex map keys
- 93 VCS workflow integration tests covering all VCS operations including merge conflicts, DAG composition, and structural lift
- 69 CLI binary integration tests (assert_cmd) covering all commands, flags, schema tools (`validate`, `check`, `lift`), and remote stubs

### Performance

- **panproto-gat**: O(1) theory lookups via precomputed `FxHashMap` index cache (`find_sort`, `find_op`, `find_eq`); eliminates linear scans in `colimit()`, `check_morphism()`, `resolve_theory()`
- **panproto-gat**: Zero-cost cloning via `Arc<str>` for all GAT type names (Sort, Operation, Equation, Term, Theory, TheoryMorphism); colimit and resolution clone ref-counted pointers instead of allocating strings
- **panproto-gat**: Colimit uses theory index for O(1) membership checks instead of building temporary `FxHashSet`s
- **panproto-inst**: Fused single-pass restrict pipeline — BFS traversal combines anchor checking, reachability, ancestor contraction, and edge resolution into one pass (was 4 separate passes)
- **panproto-inst**: Path compression in `ancestor_contraction()` — O(n) amortized via cached parent chain walks (was O(n × depth))
- **panproto-inst**: `resolve_edge()` avoids heap-allocating `(String, String)` tuple for resolver lookup
- **panproto-inst**: `#[inline]` on hot WInstance accessors (`node()`, `children()`, `parent()`)
- **panproto-mig**: Precomputed inverse maps in `compose()` — O(1) hyper-edge and vertex inverse lookups (was O(n) iterator scans)
- **panproto-schema**: `#[inline]` on `has_vertex()`, `edges_between()`
- **panproto-wasm**: `Arc<Schema>` in slab resource storage for O(1) schema sharing across migration handles
- **panproto-wasm**: `opt-level = 3` for WASM release profile (was `"z"` / size-optimized)

### Added

- **panproto-wasm**: Expand WASM boundary from 10 to 48 `#[wasm_bindgen]` entry points covering the full crate surface
  - **Check & introspection** (6): `diff_schemas_full` (20+ change categories via `panproto-check`), `classify_diff` (breaking/non-breaking classification), `report_text`/`report_json` (human/machine report rendering), `normalize_schema`, `validate_schema`
  - **Instance & I/O** (8): `register_io_protocols` (all 76 codecs), `list_io_protocols`, `parse_instance`/`emit_instance` (auto-selects W-type or Functor by protocol), `validate_instance`, `instance_to_json`/`json_to_instance`, `instance_element_count`
  - **Lens & migration** (6): `lens_from_combinators` (Cambria-style), `check_lens_laws`/`check_get_put`/`check_put_get` (law verification), `invert_migration`, `compose_lenses`
  - **Protocol registry** (2): `list_builtin_protocols`/`get_builtin_protocol` (all 76 protocol specs on demand)
  - **GAT operations** (4): `create_theory`, `colimit_theories`, `check_morphism`, `migrate_model`
  - **VCS operations** (12): `vcs_init`/`vcs_add`/`vcs_commit`/`vcs_log`/`vcs_status`/`vcs_diff`/`vcs_branch`/`vcs_checkout`/`vcs_merge`/`vcs_stash`/`vcs_stash_pop`/`vcs_blame`
  - New slab resource types: `IoRegistry`, `Theory`, `VcsRepo`; new slab helpers: `with_resource_mut`, `with_three_resources`
- **@panproto/core** (TypeScript SDK): Massive API expansion aligned with Rust crates
  - `FullDiffReport` / `CompatReport` with fluent chaining (`diffFull(old, new).classify(proto).toText()`)
  - `Instance` class with `toJson()`, `validate()`, `fromJson()`, `elementCount`
  - `IoRegistry` (Disposable) with `parse()`/`emit()` across 76 protocol codecs, `protocols`/`categories` getters
  - `LensHandle` (Disposable) with `get()`/`put()`/`checkLaws()`/`checkGetPut()`/`checkPutGet()`, `fromCombinators()` variadic factory
  - `TheoryHandle` + `TheoryBuilder` fluent API, `colimit()`, `checkMorphism()`, `migrateModel()`
  - `Repository` (Disposable) with full git-like API: `add`/`commit`/`log`/`status`/`diff`/`branch`/`checkout`/`merge`/`stash`/`blame`
  - All 76 built-in protocols available via WASM-backed lazy loading (up from 5 hardcoded)
  - `BuiltSchema.normalize()` / `.validate()` convenience methods
  - `MigrationBuilder.invert()` for bijective migration reversal
  - `PROTOCOL_CATEGORIES` constant organizing 76 protocols across 10 categories
  - 101 tests across 10 test files
- **panproto** (Python SDK): Mirror of TypeScript SDK expansion
  - `FullDiffReport` / `CompatReport` with `classify()`, `to_text()`, `to_json()`
  - `Instance`, `IoRegistry`, `LensHandle`, `TheoryHandle`, `TheoryBuilder`, `VcsRepository`
  - All classes use `@final`, `__slots__`, and context manager protocol
  - `PROTOCOL_CATEGORIES` matching TypeScript SDK
- Comprehensive divan benchmarks across all compilation levels: GAT colimit/resolve/morphism at scale (10–500 sorts), instance restrict on deep/wide trees, migration compose chains, lens get/put round-trips
- Formal correctness proofs for all optimizations in `tutorial/appendices/formal-proofs.qmd`
- Optimization reference guide in `dev-guide/appendices/optimization-guide.qmd`
- Tutorial section on fused restrict pipeline in chapter 7

## [0.3.0] - 2026-03-14

### Features

- **panproto-vcs** (NEW): Schematic version control engine
  - Content-addressed object store (blake3 hashing, canonical MessagePack serialization)
  - Commit DAG with proper LCA merge-base algorithm (replaces two-frontier BFS)
  - **Pushout-based three-way merge** — formally correct categorical pushout with typed conflict detection across all 13 schema fields; no "ours wins" tie-breaking; commutative (merge(base, A, B) == merge(base, B, A))
  - 25 `MergeConflict` variants covering vertices, edges, constraints, hyper-edges, variants, orderings, recursion points, usage modes, NSIDs, required edges, nominal flags, and spans
  - Branches, tags, HEAD, reflog (append-only audit trail)
  - Rebase, cherry-pick, reset (soft/mixed/hard), stash
  - Bisect (binary search for breaking commits), blame (element attribution)
  - Garbage collection with full mark-sweep (enumerate + delete unreachable objects)
  - Auto-migration derivation from SchemaDiff
  - Repository orchestration (porcelain layer)
  - FsStore (.panproto/ directory) and MemStore (for tests + WASM)
- **panproto-check**: Extend `SchemaDiff` to track all 13 schema fields
  - New: hyper-edge add/remove/modify, required edge add/remove, NSID add/remove/change, variant tag modifications, recursion point target modifications, span add/remove/modify, nominal flag changes
  - `is_empty()` now checks all 26 fields (was only checking 6)
  - BreakingChange gains RemovedVariant, OrderToUnordered, RecursionBroken, LinearityTightened
- **panproto-protocols**: Expand building-block theories from 10 to 27
  - ThOrder, ThCoproduct (retraction equation), ThRecursion (fold-unfold equation), ThSpan, ThCospan, ThPartial (witness equation), ThLinear, ThNominal
  - ThReflexiveGraph (2 identity equations), ThSymmetricGraph (3 involution equations), ThPetriNet
  - ThGraphInstance (graph-shaped instances), ThAnnotation (out-of-band metadata), ThCausal (dependent Before sort)
  - ThOperad, ThTracedMonoidal, ThSimplicial
  - ThSimpleGraph uses dependent Edge(s: Vertex, t: Vertex) sort
  - Group F registration for graph-shaped instances
- **panproto-schema**: Add Variant, Ordering, RecursionPoint, Span, UsageMode types; Protocol gains has_order, has_coproducts, has_recursion, has_causal, nominal_identity flags
- **panproto-inst**: Add GInstance (graph-shaped instances with graph_restrict), unified Instance enum, Node gains position and annotations fields
- **panproto-mig**: Theory-driven existence checks for Variant, Position, Mu, Usage sorts
- **panproto-cli**: Rename binary to `schema`; add VCS subcommands (init, add, commit, status, log, show, branch, tag, checkout, merge, rebase, cherry-pick, reset, stash, reflog, bisect, blame, lift, gc)
- **@panproto/core** (TypeScript): Add Variant, RecursionPoint, Span, UsageMode types to SchemaData; refactor WASM loading for bundler compatibility (Vite/webpack)
- **panproto-python**: Update ATProto spec with full vertex kinds, edge rules, and constraint sorts; add hyper_edge_map and label_map to MigrationMapping; extend SchemaData with variants, recursion_points, spans, usage_modes, nominal
- All 76 protocols updated with theory flags; Neo4j moved to Group F (graph instance)

### Fixes

- **panproto-vcs merge**: Fix false `DeleteModifyVertex` conflicts when one side removes a vertex and the other leaves it unchanged (compared against ours instead of base)
- **panproto-vcs merge**: Fix orderings/recursion_points/usage_modes silently dropping theirs' changes (overwrote base unconditionally with ours' values)
- **panproto-vcs merge**: Fix hyper_edges/required/nsids ignoring removals (only handled additions)
- **panproto-vcs merge**: Fix spans always empty and nominal always copying base
- **panproto-vcs dag**: Replace merge-base two-frontier BFS with proper LCA algorithm (handles criss-cross merges correctly)
- **panproto-wasm**: Box large `Schema` variant in slab `Resource` enum to reduce stack usage
- Resolve all clippy pedantic/nursery warnings across entire workspace (strict `-D warnings`)
- Fix CI workflow: use `dtolnay/rust-toolchain@master` with toolchain param, upgrade cargo-deny to v2, install wasm-pack via cargo
- Fix `include-code-file` line numbers in tutorial and dev-guide after code changes

### Documentation

- Tutorial: chapters 13 (Schematic Version Control) and 14 (Building-Block Catalog)
- Dev-guide: chapters 21 (VCS Engine with comprehensive related work) and 22 (Building-Block Theories with type-checking proofs)
- Updated merge documentation to reflect pushout semantics (no tie-breaking, commutativity guarantee)
- Updated protocol counts (54 → 76), theory groups (5 → 6), per-group counts
- Added bibliography entries for Mimram & Di Giusto, Schürmann, Topos Institute, Cambria
- Updated README with VCS, IO crates and corrected CLI name

## [0.2.0] - 2026-03-13

### Features

- **panproto-io** (NEW): Instance-level presentation functors for all 77 protocols, completing the functorial data migration pipeline
  - SIMD JSON pathway via `simd-json` (2-4x over `serde_json`)
  - Zero-copy XML pathway via `quick-xml` pull parser
  - SIMD tabular pathway via `memchr` for delimited formats (CoNLL-U, CSV, EDI, SWIFT MT)
  - SIMD HTML codec via `tl`
  - Markdown codec via `pulldown-cmark`
  - Dedicated CoNLL-U codec with sentence/token table extraction
  - `ProtocolRegistry` for runtime dispatch by protocol name
  - `default_registry()` entry point with all 77 codecs pre-registered
  - Arena allocation helpers (`bumpalo`) for zero-copy hot paths
- **panproto-protocols**: Expand protocol coverage to 77 formats (54 base + 19 annotation + 4 new: SWIFT MT, Docker Compose, 2 additional) with bidirectional schema-level parse/emit
  - **Serialization** (7): Avro, Thrift, Cap'n Proto, FlatBuffers, ASN.1, Bond, MsgPack
  - **Data Schema** (7): XML/XSD, CSV/Table Schema, YAML Schema, TOML Schema, CDDL, INI, BSON
  - **API** (4): OpenAPI, AsyncAPI, RAML, JSON:API
  - **Database** (5): MongoDB, Cassandra, DynamoDB, Neo4j, Redis
  - **Type System** (8): TypeScript, Python, Rust, Java, Go, Swift, Kotlin, C#
  - **Web/Document** (8): HTML, CSS, DOCX, ODF, Markdown, JSX, Vue, Svelte
  - **Data Science** (3): Parquet, Arrow, DataFrame
  - **Domain** (5): GeoJSON, FHIR, RSS/Atom, vCard/iCal, EDI X12
  - **Config** (3): HCL, K8s CRD, Docker Compose
  - **Annotation** (19): AMR, bead, brat, Concrete, CoNLL-U, Decomp/UDS, ELAN, FoLiA, FOVEA, ISO-Space, LAF/GrAF, NAF, NIF, PAULA, TEI, TimeML, UCCA, UIMA/CAS, W3C Web Annotation
- **panproto-protocols**: Shared emit helpers (`find_roots`, `children_by_edge`, `vertex_constraints`, `IndentWriter`) and 5 theory group registration functions
- **panproto-core**: Re-exports `panproto-io` as `panproto::io`
- **panproto-python**: Python 3.13+ SDK with strict typing, Pydantic v2 models, and 170 tests

### Documentation

- Tutorial book (Quarto) covering schemas, GATs, protocols, migration, and lenses
- Developer guide (Quarto) covering contribution workflow, architecture, and crate internals
  - Chapter 5: Updated crate hierarchy (11 crates, 6 levels) with `panproto-io` at Level 3.5, updated dependency graph, migration lifecycle sequence diagram, and "What Lives Where" table
  - Chapter 8: Updated instance lifecycle to show `panproto-io` as the format-specific entry point alongside generic `parse_json`
  - Chapter 12: Rewritten parser/emitter convention as two-level presentation architecture (schema presentations in `panproto-protocols`, instance presentations in `panproto-io`); updated "Adding a New Protocol" guide with Step 4b for instance codecs
  - Appendix B: Added `panproto-io` source code map with all 26 source files
- Per-crate README files with linked technical concepts
- Project README and MIT license

### Fixes

- Fix Mermaid diagram newlines in dev-guide (literal `\n` → `<br>`)
- Add version specs to workspace crate dependencies for crates.io publishing
- Add MPL-2.0 to `deny.toml` license allow list

### Testing

- 76 round-trip integration tests for `panproto-io`, one per registered protocol
- Fixture data from public sources: UD English EWT (CC BY-SA), Wikipedia HTML (CC BY-SA), Rust README (MIT), Natural Earth GeoJSON (public domain), HL7 FHIR R4 (CC0), NASA RSS (rssboard.org), AWS CloudFormation (MIT), K8s Gateway API CRD (Apache-2.0), JSON Schema Test Suite (MIT)

### Stats

- 694 tests across the workspace (up from 212 in v0.1.0; 98 in panproto-io)

## [0.1.0] - 2026-03-12

### Features

- **panproto-gat**: Generalized Algebraic Theory engine with sorts, operations, equations, theories, theory morphisms, colimits (pushouts), and model migration
- **panproto-schema**: Schema representation with precomputed adjacency indices, protocol-aware builder with validation, and ref-chain normalization
- **panproto-inst**: W-type and set-valued functor instance representations with 5-step `wtype_restrict` pipeline, `functor_restrict` (precomposition), and `functor_extend` (left Kan extension)
- **panproto-mig**: Migration engine with theory-derived existence checking, compilation, `lift_wtype`/`lift_functor`, composition, and inversion
- **panproto-lens**: Bidirectional lens combinators (RenameField, AddField, RemoveField, WrapInObject, HoistField, CoerceType) with complement tracking and GetPut/PutGet law verification
- **panproto-check**: Breaking change detection via structural schema diffing and protocol-aware classification with human-readable and JSON reports
- **panproto-protocols**: Built-in protocol definitions for ATProto, SQL, Protobuf, GraphQL, and JSON Schema with parsers for each format
- **panproto-core**: Re-export facade for all sub-crates
- **panproto-wasm**: 10 wasm-bindgen entry points with handle-based slab allocator and MessagePack serialization boundary
- **panproto-cli**: Command-line interface with `validate`, `check`, `diff`, and `lift` subcommands
- **@panproto/core**: TypeScript SDK with async WASM initialization, fluent schema builder, migration API, and lens combinators
- 212 tests across the workspace including 59 integration tests covering self-description, ATProto round-trips, SQL migrations, cross-protocol colimits, lens laws, and performance benchmarks
