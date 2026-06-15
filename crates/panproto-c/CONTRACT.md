# panproto-c C ABI contract

Frozen-signature manifest for the panproto-c C ABI (Wave 0 of Haskell
parity). Every `pp_*` entry point below has a stable signature that
downstream agents generate Haskell FFI imports against and later fill in
with real engine wiring. With the exception of the lifecycle helpers and
the four already-implemented schema/protocol entry points, every function
is currently a compiling stub that returns `PpStatus::Operation` (status
code `7`) until the engine-wiring pass lands.

## Boundary conventions

- C signatures are produced by [`safer-ffi`]; the committed header is
  `crates/panproto-c/include/panproto.h` (regenerate with
  `PP_REGEN_HEADERS=1 cargo test -p panproto-c --features headers --
  generate_headers`). The header reflects the default-feature surface;
  the feature-gated `parse` / `project` / `git` symbols below are
  documented from source because they are absent from the default
  cdylib.
- `uint32_t` is an opaque slab handle; `uint32_t *out_handle` is a
  handle output. Free a handle with `pp_handle_free`.
- `slice_ref_uint8_t` is a borrowed input byte slice (`{ const uint8_t
  *ptr; size_t len; }`). Unless a function's note says "raw JSON" the
  payload is CBOR (`ciborium`); raw-JSON inputs are decoded with
  `serde_json`. UTF-8 string inputs are also passed as
  `slice_ref_uint8_t`.
- `Vec_uint8_t *out` is an owned output byte buffer the host must free
  with `pp_buf_free`. Output payloads are CBOR unless the note says
  JSON or raw text.
- Every function returns `int32_t` (a `PpStatus`): `0` Ok, `1` Err,
  `2` Panic, `3` InvalidHandle, `4` TypeMismatch, `5` Serialization,
  `6` Internal, `7` Operation. On a non-zero status the host calls
  `pp_last_error_take` to retrieve a CBOR `ErrorEnvelope`.

## Resource handle taxonomy

Handles index a thread-local slab whose variants mirror
`panproto_wasm::slab`: `Protocol`, `Schema`, `Migration`,
`MigrationWithSchemas`, `IoRegistry`, `Theory`, `VcsRepo`,
`ProtolensChain`, `SymmetricLens`, `DataSet`, plus feature-gated
`AstRegistry` (`full-parse`), `ProjectBuilder` and `ProjectSchema`
(`project`).

## Lifecycle (4, real)

| Signature | Notes |
| --- | --- |
| `int32_t pp_init(void)` | Install the panic hook; idempotent. |
| `int32_t pp_handle_free(uint32_t handle)` | Free a slab handle; double-free safe. |
| `int32_t pp_last_error_take(Vec_uint8_t *out)` | Drain last error as CBOR `ErrorEnvelope`; empty buffer when none. |
| `void pp_buf_free(Vec_uint8_t buf)` | Free an owned output buffer. |

## protocol (2, real)

| Signature | Notes |
| --- | --- |
| `int32_t pp_protocol_define(slice_ref_uint8_t spec, uint32_t *out_handle)` | CBOR `Protocol` in; new `Protocol` handle out. |
| `int32_t pp_protocol_serialize(uint32_t proto, Vec_uint8_t *out)` | `Protocol` handle in; CBOR `Protocol` out. |

## schema (7; 3 real, 4 stub)

| Signature | Notes |
| --- | --- |
| `int32_t pp_schema_from_cbor(slice_ref_uint8_t spec, uint32_t *out_handle)` | Real. CBOR `Schema` in; new `Schema` handle out. |
| `int32_t pp_schema_to_cbor(uint32_t schema_handle, Vec_uint8_t *out)` | Real. `Schema` handle in; CBOR `Schema` out. |
| `int32_t pp_schema_validate(uint32_t schema_handle, uint32_t proto_handle, Vec_uint8_t *out_messages)` | Real. CBOR `Vec<String>` messages out; calls `schema::validate`. |
| `int32_t pp_schema_build(uint32_t proto, slice_ref_uint8_t ops, uint32_t *out_handle)` | CBOR `Vec<BuildOp>` in; calls `helpers::build_schema_from_ops`. |
| `int32_t pp_schema_metadata(uint32_t schema_handle, Vec_uint8_t *out)` | CBOR `{ protocol, vertices, edges }` out. |
| `int32_t pp_schema_normalize(uint32_t schema_handle, uint32_t *out_handle)` | New `Schema` handle out; calls `schema::normalize`. |
| `int32_t pp_schema_parse_atproto_lexicon(slice_ref_uint8_t json, uint32_t *out_handle)` | Raw JSON in; calls `protocols::atproto::parse_lexicon`. |

## check (5, stub)

| Signature | Notes |
| --- | --- |
| `int32_t pp_check_diff_simple(uint32_t s1, uint32_t s2, Vec_uint8_t *out)` | CBOR `helpers::SchemaDiff` out; calls `helpers::compute_diff`. |
| `int32_t pp_check_diff_full(uint32_t s1, uint32_t s2, Vec_uint8_t *out)` | CBOR `check::SchemaDiff` out; calls `check::diff`. |
| `int32_t pp_check_classify(uint32_t proto, slice_ref_uint8_t diff, Vec_uint8_t *out)` | CBOR `check::SchemaDiff` in, `check::CompatReport` out; calls `check::classify`. |
| `int32_t pp_check_report_text(slice_ref_uint8_t report, Vec_uint8_t *out)` | CBOR `CompatReport` in; UTF-8 text out; calls `check::report_text`. |
| `int32_t pp_check_report_json(slice_ref_uint8_t report, Vec_uint8_t *out)` | CBOR `CompatReport` in; JSON out; calls `check::report_json`. |

## mig (7, stub)

| Signature | Notes |
| --- | --- |
| `int32_t pp_mig_check_existence(uint32_t proto, uint32_t src, uint32_t tgt, slice_ref_uint8_t mapping, Vec_uint8_t *out)` | CBOR `Migration` in, `ExistenceReport` out; calls `mig::check_existence`. |
| `int32_t pp_mig_compile(uint32_t src, uint32_t tgt, slice_ref_uint8_t mapping, uint32_t *out_handle)` | CBOR `Migration` in; new `MigrationWithSchemas` handle; calls `mig::compile`. |
| `int32_t pp_mig_lift_record(uint32_t migration, slice_ref_uint8_t record, Vec_uint8_t *out)` | CBOR `WInstance` in/out; calls `mig::lift_wtype`. |
| `int32_t pp_mig_compose(uint32_t m1, uint32_t m2, uint32_t *out_handle)` | New `Migration` handle; calls `helpers::compose_compiled`. |
| `int32_t pp_mig_invert(slice_ref_uint8_t mapping, uint32_t src, uint32_t tgt, Vec_uint8_t *out)` | CBOR `Migration` in/out; calls `mig::invert`. |
| `int32_t pp_mig_coverage(uint32_t migration, uint32_t src, uint32_t tgt, slice_ref_uint8_t instances, Vec_uint8_t *out)` | CBOR `Vec<WInstance>` in; CBOR coverage report out. |
| `int32_t pp_mig_lift_json(uint32_t migration, slice_ref_uint8_t json, slice_ref_uint8_t root_vertex, Vec_uint8_t *out)` | Raw JSON in/out; `root_vertex` is UTF-8; calls `inst::parse_json` -> `mig::lift_wtype` -> `inst::to_json`. |

## hom (5, stub) — Python-only surface

| Signature | Notes |
| --- | --- |
| `int32_t pp_hom_find_morphisms(uint32_t src, uint32_t tgt, slice_ref_uint8_t opts, Vec_uint8_t *out)` | CBOR `SearchOptions` in, `Vec<FoundMorphism>` out; calls `hom_search::find_morphisms`. |
| `int32_t pp_hom_find_best_morphism(uint32_t src, uint32_t tgt, slice_ref_uint8_t opts, Vec_uint8_t *out)` | CBOR `Option<FoundMorphism>` out; calls `hom_search::find_best_morphism`. |
| `int32_t pp_hom_morphism_to_migration(slice_ref_uint8_t morphism, uint32_t *out_handle)` | CBOR `FoundMorphism` in; new `Migration` handle. |
| `int32_t pp_hom_induce_schema_morphism(slice_ref_uint8_t theory_morphism, uint32_t src, Vec_uint8_t *out)` | CBOR `TheoryMorphism` in, `SchemaMorphism` out; calls `cascade::induce_schema_morphism`. |
| `int32_t pp_hom_induce_migration_from_theory(slice_ref_uint8_t theory_morphism, uint32_t src, uint32_t tgt, Vec_uint8_t *out, uint32_t *out_handle)` | CBOR `TheoryMorphism` in, `SchemaMorphism` out plus new `MigrationWithSchemas` handle; calls `cascade::induce_migration_from_theory`. |

## instance (4, stub)

| Signature | Notes |
| --- | --- |
| `int32_t pp_inst_validate(uint32_t schema_handle, slice_ref_uint8_t instance, Vec_uint8_t *out)` | CBOR `WInstance` in, `Vec<String>` out; calls `inst::validate_wtype`. |
| `int32_t pp_inst_to_json(uint32_t schema_handle, slice_ref_uint8_t instance, Vec_uint8_t *out)` | CBOR `WInstance` in; JSON out; calls `inst::to_json`. |
| `int32_t pp_inst_json_to_instance(uint32_t schema_handle, slice_ref_uint8_t json, slice_ref_uint8_t root_vertex, Vec_uint8_t *out)` | Raw JSON in, CBOR `WInstance` out; `root_vertex` UTF-8; calls `inst::parse_json`. |
| `int32_t pp_inst_element_count(slice_ref_uint8_t instance, uint32_t *out_count)` | CBOR `WInstance` in; node count out; calls `WInstance::node_count`. |

## registry (6, stub)

| Signature | Notes |
| --- | --- |
| `int32_t pp_io_register_protocols(uint32_t *out_handle)` | New `IoRegistry` handle; calls `io::default_registry`. |
| `int32_t pp_io_list_protocols(uint32_t registry, Vec_uint8_t *out)` | CBOR `Vec<String>` out; calls `ProtocolRegistry::protocol_names`. |
| `int32_t pp_io_parse_instance(uint32_t registry, slice_ref_uint8_t proto_name, uint32_t schema_handle, slice_ref_uint8_t input, Vec_uint8_t *out)` | UTF-8 `proto_name`; raw format in; CBOR instance out; dispatches on `native_repr`. |
| `int32_t pp_io_emit_instance(uint32_t registry, slice_ref_uint8_t proto_name, uint32_t schema_handle, slice_ref_uint8_t instance, Vec_uint8_t *out)` | CBOR instance in; raw format out. |
| `int32_t pp_registry_list_builtin(Vec_uint8_t *out)` | CBOR `Vec<String>` out; calls `helpers::builtin_protocol_names`. |
| `int32_t pp_registry_get_builtin(slice_ref_uint8_t name, Vec_uint8_t *out)` | UTF-8 name in; CBOR `Protocol` out; calls `helpers::lookup_builtin_protocol`. |

## lens (18, stub)

| Signature | Notes |
| --- | --- |
| `int32_t pp_lens_auto_generate_protolens(uint32_t schema1, uint32_t schema2, slice_ref_uint8_t stringency, uint32_t *out_handle)` | UTF-8 stringency; new `ProtolensChain` handle; calls `lens::auto_generate`. |
| `int32_t pp_lens_auto_generate_candidates(uint32_t schema1, uint32_t schema2, uint32_t top_n, slice_ref_uint8_t stringency, Vec_uint8_t *out)` | CBOR `{ candidates, coerce_proposals }` out; calls `lens::auto_generate_candidates`. |
| `int32_t pp_lens_check_laws(uint32_t migration, slice_ref_uint8_t instance, Vec_uint8_t *out)` | CBOR `WInstance` in, `LawCheckResult` out; calls `lens::check_laws`. |
| `int32_t pp_lens_check_get_put(uint32_t migration, slice_ref_uint8_t instance, Vec_uint8_t *out)` | As above; calls `lens::check_get_put`. |
| `int32_t pp_lens_check_put_get(uint32_t migration, slice_ref_uint8_t instance, Vec_uint8_t *out)` | As above; calls `lens::check_put_get`. |
| `int32_t pp_lens_get_record(uint32_t migration, slice_ref_uint8_t record, Vec_uint8_t *out)` | CBOR `WInstance` in; CBOR `{ view, complement }` out; calls `lens::get`. |
| `int32_t pp_lens_put_record(uint32_t migration, slice_ref_uint8_t view, slice_ref_uint8_t complement, Vec_uint8_t *out)` | CBOR `WInstance` + `Complement` in; CBOR `WInstance` out; calls `lens::put`. |
| `int32_t pp_lens_compose(uint32_t l1, uint32_t l2, uint32_t *out_handle)` | New `MigrationWithSchemas` handle; calls `lens::compose`. |
| `int32_t pp_protolens_instantiate(uint32_t chain, uint32_t schema, uint32_t *out_handle)` | New `MigrationWithSchemas` handle; calls `ProtolensChain::instantiate`. |
| `int32_t pp_protolens_complement_spec(uint32_t chain, uint32_t schema, Vec_uint8_t *out)` | CBOR `ComplementSpec` out; calls `lens::chain_complement_spec`. |
| `int32_t pp_protolens_from_diff(slice_ref_uint8_t diff, uint32_t schema1, uint32_t schema2, uint32_t *out_handle)` | CBOR `DiffSpec` in; new `ProtolensChain` handle; calls `lens::diff_to_protolens`. |
| `int32_t pp_protolens_compose(uint32_t chain1, uint32_t chain2, uint32_t *out_handle)` | New `ProtolensChain` handle (concatenated steps). |
| `int32_t pp_protolens_chain_to_json(uint32_t chain, Vec_uint8_t *out)` | JSON out (`Vec<ProtolensStepInfo>`). |
| `int32_t pp_protolens_from_json(slice_ref_uint8_t json, uint32_t *out_handle)` | Raw JSON in; new `ProtolensChain` handle; calls `ProtolensChain::from_json`. |
| `int32_t pp_protolens_fuse(uint32_t chain, uint32_t *out_handle)` | New `ProtolensChain` handle; calls `ProtolensChain::fuse`. |
| `int32_t pp_lens_symmetric_from_schemas(uint32_t schema1, uint32_t schema2, uint32_t *out_handle)` | New `SymmetricLens` handle; calls `SymmetricLens::auto_symmetric`. |
| `int32_t pp_lens_symmetric_sync(uint32_t sym_lens, slice_ref_uint8_t view, slice_ref_uint8_t complement, uint8_t direction, Vec_uint8_t *out)` | CBOR `WInstance` + `Complement` in; `direction` 0=L→R, 1=R→L; CBOR `WInstance` out. |
| `int32_t pp_lens_compile_document(slice_ref_uint8_t source, slice_ref_uint8_t format, slice_ref_uint8_t body_vertex, uint32_t *out_handle)` | UTF-8 DSL source; `format` is `json`/`yaml`; new `ProtolensChain` handle; calls `panproto_lens_dsl`. |

## gat (4, stub)

| Signature | Notes |
| --- | --- |
| `int32_t pp_gat_create_theory(slice_ref_uint8_t spec, uint32_t *out_handle)` | CBOR `Theory` in; new `Theory` handle. |
| `int32_t pp_gat_colimit(uint32_t t1, uint32_t t2, uint32_t shared, uint32_t *out_handle)` | New `Theory` handle; calls `gat::colimit_by_name`. |
| `int32_t pp_gat_check_morphism(slice_ref_uint8_t morphism, uint32_t domain, uint32_t codomain, Vec_uint8_t *out)` | CBOR `TheoryMorphism` in, `MorphismCheckResult` out; calls `gat::check_morphism`. |
| `int32_t pp_gat_migrate_model(slice_ref_uint8_t model, slice_ref_uint8_t morphism, Vec_uint8_t *out)` | CBOR sort-interp map + `TheoryMorphism` in; CBOR reindexed sort interps out. |

## expr (5, stub)

| Signature | Notes |
| --- | --- |
| `int32_t pp_expr_parse(slice_ref_uint8_t source, Vec_uint8_t *out)` | UTF-8 source in; CBOR `Expr` out; calls `panproto_expr_parser`. |
| `int32_t pp_expr_eval_func(slice_ref_uint8_t expr, slice_ref_uint8_t env, Vec_uint8_t *out)` | CBOR `Expr` + `Vec<(String, Literal)>` in; CBOR `Literal` out; calls `panproto_expr::eval`. |
| `int32_t pp_expr_eval_gat(slice_ref_uint8_t expr, slice_ref_uint8_t env, uint32_t theory, Vec_uint8_t *out)` | CBOR `Term` + `Vec<(String, ModelValue)>` in; `Theory` handle; CBOR `ModelValue` out. |
| `int32_t pp_expr_check(slice_ref_uint8_t expr, uint32_t theory, slice_ref_uint8_t context, Vec_uint8_t *out)` | CBOR `Term` + `Vec<(String, String)>` context in; CBOR `{ well_formed, output_sort, error }` out; calls `gat::typecheck_term`. |
| `int32_t pp_query_execute(slice_ref_uint8_t query, slice_ref_uint8_t instance, uint32_t schema_handle, Vec_uint8_t *out)` | CBOR `InstanceQuery` + `WInstance` in; CBOR match list out; calls `inst::execute_query`. |

## enriched (5, stub)

| Signature | Notes |
| --- | --- |
| `int32_t pp_schema_add_coercion(uint32_t schema_handle, slice_ref_uint8_t from_kind, slice_ref_uint8_t to_kind, slice_ref_uint8_t expr, uint32_t *out_handle)` | UTF-8 kinds; CBOR `Expr` in; new `Schema` handle. |
| `int32_t pp_schema_add_default(uint32_t schema_handle, slice_ref_uint8_t vertex_name, slice_ref_uint8_t expr, uint32_t *out_handle)` | CBOR `Value` in; new `Schema` handle. |
| `int32_t pp_schema_add_merger(uint32_t schema_handle, slice_ref_uint8_t vertex_name, slice_ref_uint8_t spec, uint32_t *out_handle)` | CBOR `{ strategy, args }` in; new `Schema` handle. |
| `int32_t pp_schema_add_policy(uint32_t schema_handle, slice_ref_uint8_t vertex_name, slice_ref_uint8_t spec, uint32_t *out_handle)` | CBOR `{ policy }` in; new `Schema` handle. |
| `int32_t pp_enriched_refinement_subsort(slice_ref_uint8_t base_sort, slice_ref_uint8_t sub_constraints, slice_ref_uint8_t super_constraints, uint32_t *out_is_subsort)` | UTF-8 base sort; CBOR `Vec<(String, String)>` constraint sets; `1`/`0` out. |

## vcs (12, stub)

| Signature | Notes |
| --- | --- |
| `int32_t pp_vcs_init(slice_ref_uint8_t protocol_name, uint32_t *out_handle)` | UTF-8 name; new `VcsRepo` handle; calls `vcs::MemStore::new`. |
| `int32_t pp_vcs_add(uint32_t repo, uint32_t schema, Vec_uint8_t *out)` | CBOR `VcsAddResult` out; calls `vcs::tree::store_schema_as_tree`. |
| `int32_t pp_vcs_commit(uint32_t repo, slice_ref_uint8_t message, slice_ref_uint8_t author, Vec_uint8_t *out)` | UTF-8 message/author; CBOR commit-id out. |
| `int32_t pp_vcs_log(uint32_t repo, uint32_t count, Vec_uint8_t *out)` | CBOR `Vec<VcsLogEntry>` out; calls `vcs::dag::log_walk`. |
| `int32_t pp_vcs_status(uint32_t repo, Vec_uint8_t *out)` | CBOR `VcsStatusResult` out. |
| `int32_t pp_vcs_diff(uint32_t repo, Vec_uint8_t *out)` | CBOR `VcsDiffResult` out; calls `vcs::refs::list_branches`. |
| `int32_t pp_vcs_branch(uint32_t repo, slice_ref_uint8_t name, Vec_uint8_t *out)` | UTF-8 name; CBOR `VcsOpResult` out; calls `vcs::refs::create_branch`. |
| `int32_t pp_vcs_checkout(uint32_t repo, slice_ref_uint8_t target, Vec_uint8_t *out)` | UTF-8 target; CBOR `VcsOpResult` out; calls `vcs::refs::checkout_branch`. |
| `int32_t pp_vcs_merge(uint32_t repo, slice_ref_uint8_t branch, Vec_uint8_t *out)` | UTF-8 branch; CBOR `VcsOpResult` out. |
| `int32_t pp_vcs_stash(uint32_t repo, Vec_uint8_t *out)` | CBOR `VcsOpResult` out; calls `vcs::stash::stash_list`. |
| `int32_t pp_vcs_stash_pop(uint32_t repo, Vec_uint8_t *out)` | CBOR `VcsOpResult` out; calls `vcs::stash::stash_pop`. |
| `int32_t pp_vcs_blame(uint32_t repo, slice_ref_uint8_t vertex, Vec_uint8_t *out)` | UTF-8 vertex id; CBOR `VcsBlameResult` out; calls `vcs::blame::blame_vertex`. |

## data (6, stub)

| Signature | Notes |
| --- | --- |
| `int32_t pp_data_store_dataset(uint32_t schema_handle, slice_ref_uint8_t data_json, uint32_t *out_handle)` | Raw JSON array in; new `DataSet` handle; calls `inst::parse_json`. |
| `int32_t pp_data_get_dataset(uint32_t dataset_handle, Vec_uint8_t *out)` | CBOR `Vec<WInstance>` out. |
| `int32_t pp_data_migrate_forward(uint32_t dataset_handle, uint32_t src_schema, uint32_t tgt_schema, uint32_t *out_data_handle, uint32_t *out_complement_handle)` | Two `DataSet` handles out; calls `lens::auto_generate` + `lens::get`. |
| `int32_t pp_data_migrate_backward(uint32_t dataset_handle, slice_ref_uint8_t complement, uint32_t src_schema, uint32_t tgt_schema, uint32_t *out_handle)` | CBOR `Vec<Complement>` in; new `DataSet` handle; calls `lens::put`. |
| `int32_t pp_data_check_staleness(uint32_t dataset_handle, uint32_t schema_handle, Vec_uint8_t *out)` | CBOR `{ stale, data_schema_id, target_schema_id }` out. |
| `int32_t pp_data_get_migration_complement(slice_ref_uint8_t complement, Vec_uint8_t *out)` | CBOR `Vec<Complement>` round-trip (validation). |

## graph (5, stub)

| Signature | Notes |
| --- | --- |
| `int32_t pp_graph_fiber_at(slice_ref_uint8_t instance, slice_ref_uint8_t migration, slice_ref_uint8_t target_anchor, Vec_uint8_t *out)` | CBOR `WInstance` + `CompiledMigration` in; UTF-8 anchor; CBOR `Vec<u32>` out; calls `inst::fiber_at_anchor`. |
| `int32_t pp_graph_fiber_decomposition(slice_ref_uint8_t instance, slice_ref_uint8_t migration, Vec_uint8_t *out)` | CBOR `HashMap<String, Vec<u32>>` out; calls `inst::fiber_decomposition`. |
| `int32_t pp_graph_poly_hom(slice_ref_uint8_t source_schema, slice_ref_uint8_t target_schema, Vec_uint8_t *out)` | CBOR `Schema` in/out (hom schema); calls `inst::hom_schema`. |
| `int32_t pp_graph_preferred_path(slice_ref_uint8_t graph, slice_ref_uint8_t source_schema, slice_ref_uint8_t target_schema, Vec_uint8_t *out)` | CBOR `Vec<GraphEdge>` in; UTF-8 schema names; CBOR `{ cost, steps }` out; calls `LensGraph::preferred_path`. |
| `int32_t pp_graph_conversion_distance(slice_ref_uint8_t graph, slice_ref_uint8_t source_schema, slice_ref_uint8_t target_schema, double *out_distance)` | `f64` distance out (INF if unreachable); calls `LensGraph::distance`. |

## parse (10, stub) — feature `full-parse`

| Signature | Notes |
| --- | --- |
| `int32_t pp_parse_registry_new(uint32_t *out_handle)` | New `AstRegistry` handle; calls `ParserRegistry::new`. |
| `int32_t pp_parse_file(uint32_t registry, slice_ref_uint8_t path, slice_ref_uint8_t content, uint32_t *out_handle)` | UTF-8 path; source bytes; new `Schema` handle; calls `ParserRegistry::parse_file`. |
| `int32_t pp_parse_with_protocol(uint32_t registry, slice_ref_uint8_t protocol, slice_ref_uint8_t content, slice_ref_uint8_t file_path, uint32_t *out_handle)` | UTF-8 protocol/path; new `Schema` handle; calls `ParserRegistry::parse_with_protocol`. |
| `int32_t pp_parse_detect_language(uint32_t registry, slice_ref_uint8_t path, Vec_uint8_t *out)` | UTF-8 path; UTF-8 protocol name out (empty if none); calls `detect_language`. |
| `int32_t pp_parse_emit(uint32_t registry, slice_ref_uint8_t protocol, uint32_t schema, Vec_uint8_t *out)` | Source bytes out; calls `emit_with_protocol`. |
| `int32_t pp_parse_emit_pretty(uint32_t registry, slice_ref_uint8_t protocol, uint32_t schema, Vec_uint8_t *out)` | Source bytes out; calls `emit_pretty_with_protocol`. |
| `int32_t pp_parse_protocol_names(uint32_t registry, Vec_uint8_t *out)` | CBOR `Vec<String>` out; calls `protocol_names`. |
| `int32_t pp_parse_available_grammars(Vec_uint8_t *out)` | CBOR `Vec<String>` out; calls `panproto_grammars::grammars`. |
| `int32_t pp_parse_check_emit_parse(uint32_t registry, slice_ref_uint8_t protocol, uint32_t schema, Vec_uint8_t *out)` | Empty buffer if law holds else divergence text; calls `check_emit_parse`. |
| `int32_t pp_parse_check_parse_emit(uint32_t registry, slice_ref_uint8_t protocol, slice_ref_uint8_t bytes, Vec_uint8_t *out)` | Empty buffer if law holds else divergence text; calls `check_parse_emit`. |

## project (6, stub) — feature `project`

| Signature | Notes |
| --- | --- |
| `int32_t pp_project_builder_new(uint32_t *out_handle)` | New `ProjectBuilder` handle; calls `ProjectBuilder::new`. |
| `int32_t pp_project_add_file(uint32_t builder, slice_ref_uint8_t path, slice_ref_uint8_t content)` | UTF-8 path; mutates builder in place; calls `ProjectBuilder::add_file`. |
| `int32_t pp_project_add_directory(uint32_t builder, slice_ref_uint8_t path)` | UTF-8 dir path; mutates builder; calls `ProjectBuilder::add_directory`. |
| `int32_t pp_project_build(uint32_t builder, uint32_t *out_handle)` | New `ProjectSchema` handle; calls `ProjectBuilder::build`. |
| `int32_t pp_project_schema_get(uint32_t project, uint32_t *out_handle)` | New `Schema` handle for the coproduct schema. |
| `int32_t pp_project_protocol_map(uint32_t project, Vec_uint8_t *out)` | CBOR `HashMap<String, String>` (path → protocol) out. |

## git (1, stub) — feature `git`

| Signature | Notes |
| --- | --- |
| `int32_t pp_git_import(slice_ref_uint8_t repo_path, slice_ref_uint8_t revspec, uint32_t *out_handle, Vec_uint8_t *out)` | UTF-8 path/revspec; new `VcsRepo` handle plus CBOR `{ commit_count, head_id }` summary; calls `git::import_git_repo`. |

[`safer-ffi`]: https://docs.rs/safer-ffi
