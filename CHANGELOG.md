# Changelog

All notable changes to panproto will be documented in this file.

## [Unreleased]

### Fixed — Mathematical Correctness Review

- **panproto-gat** (F1): equation preservation in `check_morphism()` now uses α-equivalence instead of syntactic equality, correctly treating universally quantified variable names as bound. Added `alpha_equivalent()` and `alpha_equivalent_equation()` to `Term`. Pullback equation pairing in `pair_eqs()` also updated.
- **panproto-gat** (F2): naturality square verification in `check_natural_transformation()` now normalizes both sides via the codomain's directed equations (rewrite rules) before comparison. Added `match_pattern()` for first-order pattern matching and `normalize()` for innermost-first term rewriting to fixed point. Naturality checks that depend on the codomain's equational theory no longer produce spurious violations.
- **panproto-gat** (F3): theory colimit (`colimit()`) now propagates directed equations and conflict policies from both input theories. Previously these were silently dropped, causing composed protocols to lose rewrite rules essential to the edit lens pipeline. Conflict detection uses α-equivalence for directed equation compatibility. Added `DirectedEqConflict` and `PolicyConflict` error variants.
- **panproto-gat** (F4): `check_morphism()` now verifies that directed equations are preserved under the morphism. For each domain directed equation, the mapped terms must appear as a directed equation in the codomain (checked via α-equivalence). Added `DirectedEquationNotPreserved` error variant.
- **panproto-gat** (F5): pullback construction now pairs directed equations from both source theories when they agree in the codomain (via α-equivalence). Added `pair_directed_eqs()` following the same pattern as `pair_eqs()`. The pullback theory uses `Theory::full()` to include paired directed equations.
- **panproto-gat** (F6): free model construction now topologically sorts the theory's sorts by dependency, ensuring parameter sorts are populated before dependent sorts. Added `topological_sort_sorts()`. Term generation iterates in dependency order so dependent sorts like `Hom(a: Ob, b: Ob)` correctly find terms for their parameter sorts.

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
