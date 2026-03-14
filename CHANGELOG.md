# Changelog

All notable changes to panproto will be documented in this file.

## [Unreleased]

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

- Tutorial: chapters 13 (Schematic Version Control) and 14 (Building-Block Landscape)
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
