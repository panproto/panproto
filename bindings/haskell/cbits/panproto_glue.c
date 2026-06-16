/*
 * Pointer-based glue around panproto-c.
 *
 * The base panproto-c API uses safer-ffi's `c_slice::Ref<u8>` and
 * `repr_c::Vec<u8>` types, which are passed by value in the C ABI.
 * GHC's `foreign import capi` cannot reliably pass structs by value
 * across all platforms, so this glue exposes pointer-based wrappers
 * with the same semantics that Haskell's FFI consumes naturally.
 *
 * Each wrapper reconstructs the by-value `slice_ref_uint8_t` argument(s)
 * on the stack from caller-provided `(ptr, len)` pairs and forwards to
 * the matching panproto-c entry point. No allocations happen here.
 *
 * The wrappers are organized by domain to match
 * `crates/panproto-c/CONTRACT.md`. Feature-gated wrappers (parse,
 * project, git) reference symbols absent from the default-feature
 * `panproto.h` and are compiled only when the matching `PANPROTO_*`
 * macro is defined.
 */

#include "panproto_glue.h"
#include <string.h>

/* ---------- lifecycle ---------- */

/*
 * Move the contents of *buf into pp_buf_free, then zero the storage
 * so a future Haskell-side double-free can't pass a stale pointer.
 */
void pp_buf_free_at(Vec_uint8_t *buf) {
    Vec_uint8_t taken = *buf;
    memset(buf, 0, sizeof *buf);
    pp_buf_free(taken);
}

/* ---------- protocol ---------- */

int32_t pp_protocol_define_at(
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t spec = { .ptr = spec_ptr, .len = spec_len };
    return pp_protocol_define(spec, out_handle);
}

/* ---------- schema ---------- */

int32_t pp_schema_from_cbor_at(
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t spec = { .ptr = spec_ptr, .len = spec_len };
    return pp_schema_from_cbor(spec, out_handle);
}

int32_t pp_schema_build_at(
    uint32_t proto,
    const uint8_t *ops_ptr,
    size_t ops_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t ops = { .ptr = ops_ptr, .len = ops_len };
    return pp_schema_build(proto, ops, out_handle);
}

int32_t pp_schema_parse_atproto_lexicon_at(
    const uint8_t *json_ptr,
    size_t json_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t json = { .ptr = json_ptr, .len = json_len };
    return pp_schema_parse_atproto_lexicon(json, out_handle);
}

/* ---------- check ---------- */

int32_t pp_check_classify_at(
    uint32_t proto,
    const uint8_t *diff_ptr,
    size_t diff_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t diff = { .ptr = diff_ptr, .len = diff_len };
    return pp_check_classify(proto, diff, out);
}

int32_t pp_check_report_text_at(
    const uint8_t *report_ptr,
    size_t report_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t report = { .ptr = report_ptr, .len = report_len };
    return pp_check_report_text(report, out);
}

int32_t pp_check_report_json_at(
    const uint8_t *report_ptr,
    size_t report_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t report = { .ptr = report_ptr, .len = report_len };
    return pp_check_report_json(report, out);
}

/* ---------- mig ---------- */

int32_t pp_mig_check_existence_at(
    uint32_t proto,
    uint32_t src,
    uint32_t tgt,
    const uint8_t *mapping_ptr,
    size_t mapping_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t mapping = { .ptr = mapping_ptr, .len = mapping_len };
    return pp_mig_check_existence(proto, src, tgt, mapping, out);
}

int32_t pp_mig_compile_at(
    uint32_t src,
    uint32_t tgt,
    const uint8_t *mapping_ptr,
    size_t mapping_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t mapping = { .ptr = mapping_ptr, .len = mapping_len };
    return pp_mig_compile(src, tgt, mapping, out_handle);
}

int32_t pp_mig_lift_record_at(
    uint32_t migration,
    const uint8_t *record_ptr,
    size_t record_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t record = { .ptr = record_ptr, .len = record_len };
    return pp_mig_lift_record(migration, record, out);
}

int32_t pp_mig_invert_at(
    const uint8_t *mapping_ptr,
    size_t mapping_len,
    uint32_t src,
    uint32_t tgt,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t mapping = { .ptr = mapping_ptr, .len = mapping_len };
    return pp_mig_invert(mapping, src, tgt, out);
}

int32_t pp_mig_coverage_at(
    uint32_t migration,
    uint32_t src,
    uint32_t tgt,
    const uint8_t *instances_ptr,
    size_t instances_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t instances = { .ptr = instances_ptr, .len = instances_len };
    return pp_mig_coverage(migration, src, tgt, instances, out);
}

int32_t pp_mig_lift_json_at(
    uint32_t migration,
    const uint8_t *json_ptr,
    size_t json_len,
    const uint8_t *root_vertex_ptr,
    size_t root_vertex_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t json = { .ptr = json_ptr, .len = json_len };
    slice_ref_uint8_t root_vertex = { .ptr = root_vertex_ptr, .len = root_vertex_len };
    return pp_mig_lift_json(migration, json, root_vertex, out);
}

/* ---------- hom ---------- */

int32_t pp_hom_find_morphisms_at(
    uint32_t src,
    uint32_t tgt,
    const uint8_t *opts_ptr,
    size_t opts_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t opts = { .ptr = opts_ptr, .len = opts_len };
    return pp_hom_find_morphisms(src, tgt, opts, out);
}

int32_t pp_hom_find_best_morphism_at(
    uint32_t src,
    uint32_t tgt,
    const uint8_t *opts_ptr,
    size_t opts_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t opts = { .ptr = opts_ptr, .len = opts_len };
    return pp_hom_find_best_morphism(src, tgt, opts, out);
}

int32_t pp_hom_morphism_to_migration_at(
    const uint8_t *morphism_ptr,
    size_t morphism_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t morphism = { .ptr = morphism_ptr, .len = morphism_len };
    return pp_hom_morphism_to_migration(morphism, out_handle);
}

int32_t pp_hom_induce_schema_morphism_at(
    const uint8_t *theory_morphism_ptr,
    size_t theory_morphism_len,
    uint32_t src,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t theory_morphism = { .ptr = theory_morphism_ptr, .len = theory_morphism_len };
    return pp_hom_induce_schema_morphism(theory_morphism, src, out);
}

int32_t pp_hom_induce_migration_from_theory_at(
    const uint8_t *theory_morphism_ptr,
    size_t theory_morphism_len,
    uint32_t src,
    uint32_t tgt,
    Vec_uint8_t *out,
    uint32_t *out_handle
) {
    slice_ref_uint8_t theory_morphism = { .ptr = theory_morphism_ptr, .len = theory_morphism_len };
    return pp_hom_induce_migration_from_theory(theory_morphism, src, tgt, out, out_handle);
}

/* ---------- instance ---------- */

int32_t pp_inst_validate_at(
    uint32_t schema_handle,
    const uint8_t *instance_ptr,
    size_t instance_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t instance = { .ptr = instance_ptr, .len = instance_len };
    return pp_inst_validate(schema_handle, instance, out);
}

int32_t pp_inst_to_json_at(
    uint32_t schema_handle,
    const uint8_t *instance_ptr,
    size_t instance_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t instance = { .ptr = instance_ptr, .len = instance_len };
    return pp_inst_to_json(schema_handle, instance, out);
}

int32_t pp_inst_json_to_instance_at(
    uint32_t schema_handle,
    const uint8_t *json_ptr,
    size_t json_len,
    const uint8_t *root_vertex_ptr,
    size_t root_vertex_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t json = { .ptr = json_ptr, .len = json_len };
    slice_ref_uint8_t root_vertex = { .ptr = root_vertex_ptr, .len = root_vertex_len };
    return pp_inst_json_to_instance(schema_handle, json, root_vertex, out);
}

int32_t pp_inst_element_count_at(
    const uint8_t *instance_ptr,
    size_t instance_len,
    uint32_t *out_count
) {
    slice_ref_uint8_t instance = { .ptr = instance_ptr, .len = instance_len };
    return pp_inst_element_count(instance, out_count);
}

/* ---------- registry ---------- */

int32_t pp_io_parse_instance_at(
    uint32_t registry,
    const uint8_t *proto_name_ptr,
    size_t proto_name_len,
    uint32_t schema_handle,
    const uint8_t *input_ptr,
    size_t input_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t proto_name = { .ptr = proto_name_ptr, .len = proto_name_len };
    slice_ref_uint8_t input = { .ptr = input_ptr, .len = input_len };
    return pp_io_parse_instance(registry, proto_name, schema_handle, input, out);
}

int32_t pp_io_emit_instance_at(
    uint32_t registry,
    const uint8_t *proto_name_ptr,
    size_t proto_name_len,
    uint32_t schema_handle,
    const uint8_t *instance_ptr,
    size_t instance_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t proto_name = { .ptr = proto_name_ptr, .len = proto_name_len };
    slice_ref_uint8_t instance = { .ptr = instance_ptr, .len = instance_len };
    return pp_io_emit_instance(registry, proto_name, schema_handle, instance, out);
}

int32_t pp_registry_get_builtin_at(
    const uint8_t *name_ptr,
    size_t name_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t name = { .ptr = name_ptr, .len = name_len };
    return pp_registry_get_builtin(name, out);
}

/* ---------- lens ---------- */

int32_t pp_lens_auto_generate_protolens_at(
    uint32_t schema1,
    uint32_t schema2,
    const uint8_t *stringency_ptr,
    size_t stringency_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t stringency = { .ptr = stringency_ptr, .len = stringency_len };
    return pp_lens_auto_generate_protolens(schema1, schema2, stringency, out_handle);
}

int32_t pp_lens_auto_generate_candidates_at(
    uint32_t schema1,
    uint32_t schema2,
    uint32_t top_n,
    const uint8_t *stringency_ptr,
    size_t stringency_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t stringency = { .ptr = stringency_ptr, .len = stringency_len };
    return pp_lens_auto_generate_candidates(schema1, schema2, top_n, stringency, out);
}

int32_t pp_lens_check_laws_at(
    uint32_t migration,
    const uint8_t *instance_ptr,
    size_t instance_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t instance = { .ptr = instance_ptr, .len = instance_len };
    return pp_lens_check_laws(migration, instance, out);
}

int32_t pp_lens_check_get_put_at(
    uint32_t migration,
    const uint8_t *instance_ptr,
    size_t instance_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t instance = { .ptr = instance_ptr, .len = instance_len };
    return pp_lens_check_get_put(migration, instance, out);
}

int32_t pp_lens_check_put_get_at(
    uint32_t migration,
    const uint8_t *instance_ptr,
    size_t instance_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t instance = { .ptr = instance_ptr, .len = instance_len };
    return pp_lens_check_put_get(migration, instance, out);
}

int32_t pp_lens_get_record_at(
    uint32_t migration,
    const uint8_t *record_ptr,
    size_t record_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t record = { .ptr = record_ptr, .len = record_len };
    return pp_lens_get_record(migration, record, out);
}

int32_t pp_lens_put_record_at(
    uint32_t migration,
    const uint8_t *view_ptr,
    size_t view_len,
    const uint8_t *complement_ptr,
    size_t complement_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t view = { .ptr = view_ptr, .len = view_len };
    slice_ref_uint8_t complement = { .ptr = complement_ptr, .len = complement_len };
    return pp_lens_put_record(migration, view, complement, out);
}

int32_t pp_protolens_complement_spec_at(
    uint32_t chain,
    uint32_t schema,
    Vec_uint8_t *out
) {
    /* No slice arguments; forwarded directly for naming consistency
     * with the rest of the lens domain. */
    return pp_protolens_complement_spec(chain, schema, out);
}

int32_t pp_protolens_from_diff_at(
    const uint8_t *diff_ptr,
    size_t diff_len,
    uint32_t schema1,
    uint32_t schema2,
    uint32_t *out_handle
) {
    slice_ref_uint8_t diff = { .ptr = diff_ptr, .len = diff_len };
    return pp_protolens_from_diff(diff, schema1, schema2, out_handle);
}

int32_t pp_protolens_from_json_at(
    const uint8_t *json_ptr,
    size_t json_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t json = { .ptr = json_ptr, .len = json_len };
    return pp_protolens_from_json(json, out_handle);
}

int32_t pp_lens_symmetric_sync_at(
    uint32_t sym_lens,
    const uint8_t *view_ptr,
    size_t view_len,
    const uint8_t *complement_ptr,
    size_t complement_len,
    uint8_t direction,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t view = { .ptr = view_ptr, .len = view_len };
    slice_ref_uint8_t complement = { .ptr = complement_ptr, .len = complement_len };
    return pp_lens_symmetric_sync(sym_lens, view, complement, direction, out);
}

int32_t pp_lens_compile_document_at(
    const uint8_t *source_ptr,
    size_t source_len,
    const uint8_t *format_ptr,
    size_t format_len,
    const uint8_t *body_vertex_ptr,
    size_t body_vertex_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t source = { .ptr = source_ptr, .len = source_len };
    slice_ref_uint8_t format = { .ptr = format_ptr, .len = format_len };
    slice_ref_uint8_t body_vertex = { .ptr = body_vertex_ptr, .len = body_vertex_len };
    return pp_lens_compile_document(source, format, body_vertex, out_handle);
}

/* ---------- gat ---------- */

int32_t pp_gat_create_theory_at(
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t spec = { .ptr = spec_ptr, .len = spec_len };
    return pp_gat_create_theory(spec, out_handle);
}

int32_t pp_gat_check_morphism_at(
    const uint8_t *morphism_ptr,
    size_t morphism_len,
    uint32_t domain,
    uint32_t codomain,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t morphism = { .ptr = morphism_ptr, .len = morphism_len };
    return pp_gat_check_morphism(morphism, domain, codomain, out);
}

int32_t pp_gat_migrate_model_at(
    const uint8_t *model_ptr,
    size_t model_len,
    const uint8_t *morphism_ptr,
    size_t morphism_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t model = { .ptr = model_ptr, .len = model_len };
    slice_ref_uint8_t morphism = { .ptr = morphism_ptr, .len = morphism_len };
    return pp_gat_migrate_model(model, morphism, out);
}

int32_t pp_gat_free_model_at(
    uint32_t theory,
    const uint8_t *config_ptr,
    size_t config_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t config = { .ptr = config_ptr, .len = config_len };
    return pp_gat_free_model(theory, config, out_handle);
}

/*
 * pp_gat_check_model and pp_gat_serialize_theory take only handle(s) and
 * a Vec_uint8_t* out, with no by-value slice arguments, so they are
 * imported directly without a pointer-based wrapper.
 */

/* ---------- expr ---------- */

int32_t pp_expr_parse_at(
    const uint8_t *source_ptr,
    size_t source_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t source = { .ptr = source_ptr, .len = source_len };
    return pp_expr_parse(source, out);
}

int32_t pp_expr_eval_func_at(
    const uint8_t *expr_ptr,
    size_t expr_len,
    const uint8_t *env_ptr,
    size_t env_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t expr = { .ptr = expr_ptr, .len = expr_len };
    slice_ref_uint8_t env = { .ptr = env_ptr, .len = env_len };
    return pp_expr_eval_func(expr, env, out);
}

int32_t pp_expr_eval_gat_at(
    const uint8_t *expr_ptr,
    size_t expr_len,
    const uint8_t *env_ptr,
    size_t env_len,
    uint32_t theory,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t expr = { .ptr = expr_ptr, .len = expr_len };
    slice_ref_uint8_t env = { .ptr = env_ptr, .len = env_len };
    return pp_expr_eval_gat(expr, env, theory, out);
}

int32_t pp_expr_check_at(
    const uint8_t *expr_ptr,
    size_t expr_len,
    uint32_t theory,
    const uint8_t *context_ptr,
    size_t context_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t expr = { .ptr = expr_ptr, .len = expr_len };
    slice_ref_uint8_t context = { .ptr = context_ptr, .len = context_len };
    return pp_expr_check(expr, theory, context, out);
}

int32_t pp_query_execute_at(
    const uint8_t *query_ptr,
    size_t query_len,
    const uint8_t *instance_ptr,
    size_t instance_len,
    uint32_t schema_handle,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t query = { .ptr = query_ptr, .len = query_len };
    slice_ref_uint8_t instance = { .ptr = instance_ptr, .len = instance_len };
    return pp_query_execute(query, instance, schema_handle, out);
}

/* ---------- enriched ---------- */

int32_t pp_schema_add_coercion_at(
    uint32_t schema_handle,
    const uint8_t *from_kind_ptr,
    size_t from_kind_len,
    const uint8_t *to_kind_ptr,
    size_t to_kind_len,
    const uint8_t *expr_ptr,
    size_t expr_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t from_kind = { .ptr = from_kind_ptr, .len = from_kind_len };
    slice_ref_uint8_t to_kind = { .ptr = to_kind_ptr, .len = to_kind_len };
    slice_ref_uint8_t expr = { .ptr = expr_ptr, .len = expr_len };
    return pp_schema_add_coercion(schema_handle, from_kind, to_kind, expr, out_handle);
}

int32_t pp_schema_add_default_at(
    uint32_t schema_handle,
    const uint8_t *vertex_name_ptr,
    size_t vertex_name_len,
    const uint8_t *expr_ptr,
    size_t expr_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t vertex_name = { .ptr = vertex_name_ptr, .len = vertex_name_len };
    slice_ref_uint8_t expr = { .ptr = expr_ptr, .len = expr_len };
    return pp_schema_add_default(schema_handle, vertex_name, expr, out_handle);
}

int32_t pp_schema_add_merger_at(
    uint32_t schema_handle,
    const uint8_t *vertex_name_ptr,
    size_t vertex_name_len,
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t vertex_name = { .ptr = vertex_name_ptr, .len = vertex_name_len };
    slice_ref_uint8_t spec = { .ptr = spec_ptr, .len = spec_len };
    return pp_schema_add_merger(schema_handle, vertex_name, spec, out_handle);
}

int32_t pp_schema_add_policy_at(
    uint32_t schema_handle,
    const uint8_t *vertex_name_ptr,
    size_t vertex_name_len,
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t vertex_name = { .ptr = vertex_name_ptr, .len = vertex_name_len };
    slice_ref_uint8_t spec = { .ptr = spec_ptr, .len = spec_len };
    return pp_schema_add_policy(schema_handle, vertex_name, spec, out_handle);
}

int32_t pp_enriched_refinement_subsort_at(
    const uint8_t *base_sort_ptr,
    size_t base_sort_len,
    const uint8_t *sub_constraints_ptr,
    size_t sub_constraints_len,
    const uint8_t *super_constraints_ptr,
    size_t super_constraints_len,
    uint32_t *out_is_subsort
) {
    slice_ref_uint8_t base_sort = { .ptr = base_sort_ptr, .len = base_sort_len };
    slice_ref_uint8_t sub_constraints = { .ptr = sub_constraints_ptr, .len = sub_constraints_len };
    slice_ref_uint8_t super_constraints = { .ptr = super_constraints_ptr, .len = super_constraints_len };
    return pp_enriched_refinement_subsort(base_sort, sub_constraints, super_constraints, out_is_subsort);
}

/* ---------- vcs ---------- */

int32_t pp_vcs_init_at(
    const uint8_t *protocol_name_ptr,
    size_t protocol_name_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t protocol_name = { .ptr = protocol_name_ptr, .len = protocol_name_len };
    return pp_vcs_init(protocol_name, out_handle);
}

int32_t pp_vcs_commit_at(
    uint32_t repo,
    const uint8_t *message_ptr,
    size_t message_len,
    const uint8_t *author_ptr,
    size_t author_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t message = { .ptr = message_ptr, .len = message_len };
    slice_ref_uint8_t author = { .ptr = author_ptr, .len = author_len };
    return pp_vcs_commit(repo, message, author, out);
}

int32_t pp_vcs_branch_at(
    uint32_t repo,
    const uint8_t *name_ptr,
    size_t name_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t name = { .ptr = name_ptr, .len = name_len };
    return pp_vcs_branch(repo, name, out);
}

int32_t pp_vcs_checkout_at(
    uint32_t repo,
    const uint8_t *target_ptr,
    size_t target_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t target = { .ptr = target_ptr, .len = target_len };
    return pp_vcs_checkout(repo, target, out);
}

int32_t pp_vcs_merge_at(
    uint32_t repo,
    const uint8_t *branch_ptr,
    size_t branch_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t branch = { .ptr = branch_ptr, .len = branch_len };
    return pp_vcs_merge(repo, branch, out);
}

int32_t pp_vcs_blame_at(
    uint32_t repo,
    const uint8_t *vertex_ptr,
    size_t vertex_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t vertex = { .ptr = vertex_ptr, .len = vertex_len };
    return pp_vcs_blame(repo, vertex, out);
}

/* ---------- data ---------- */

int32_t pp_data_store_dataset_at(
    uint32_t schema_handle,
    const uint8_t *data_json_ptr,
    size_t data_json_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t data_json = { .ptr = data_json_ptr, .len = data_json_len };
    return pp_data_store_dataset(schema_handle, data_json, out_handle);
}

int32_t pp_data_migrate_backward_at(
    uint32_t dataset_handle,
    const uint8_t *complement_ptr,
    size_t complement_len,
    uint32_t src_schema,
    uint32_t tgt_schema,
    uint32_t *out_handle
) {
    slice_ref_uint8_t complement = { .ptr = complement_ptr, .len = complement_len };
    return pp_data_migrate_backward(dataset_handle, complement, src_schema, tgt_schema, out_handle);
}

int32_t pp_data_check_staleness_at(
    uint32_t dataset_handle,
    uint32_t schema_handle,
    Vec_uint8_t *out
) {
    /* No slice arguments; forwarded directly for naming consistency. */
    return pp_data_check_staleness(dataset_handle, schema_handle, out);
}

int32_t pp_data_get_migration_complement_at(
    const uint8_t *complement_ptr,
    size_t complement_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t complement = { .ptr = complement_ptr, .len = complement_len };
    return pp_data_get_migration_complement(complement, out);
}

/* ---------- graph ---------- */

int32_t pp_graph_fiber_at_at(
    const uint8_t *instance_ptr,
    size_t instance_len,
    const uint8_t *migration_ptr,
    size_t migration_len,
    const uint8_t *target_anchor_ptr,
    size_t target_anchor_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t instance = { .ptr = instance_ptr, .len = instance_len };
    slice_ref_uint8_t migration = { .ptr = migration_ptr, .len = migration_len };
    slice_ref_uint8_t target_anchor = { .ptr = target_anchor_ptr, .len = target_anchor_len };
    return pp_graph_fiber_at(instance, migration, target_anchor, out);
}

int32_t pp_graph_fiber_decomposition_at(
    const uint8_t *instance_ptr,
    size_t instance_len,
    const uint8_t *migration_ptr,
    size_t migration_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t instance = { .ptr = instance_ptr, .len = instance_len };
    slice_ref_uint8_t migration = { .ptr = migration_ptr, .len = migration_len };
    return pp_graph_fiber_decomposition(instance, migration, out);
}

int32_t pp_graph_poly_hom_at(
    const uint8_t *source_schema_ptr,
    size_t source_schema_len,
    const uint8_t *target_schema_ptr,
    size_t target_schema_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t source_schema = { .ptr = source_schema_ptr, .len = source_schema_len };
    slice_ref_uint8_t target_schema = { .ptr = target_schema_ptr, .len = target_schema_len };
    return pp_graph_poly_hom(source_schema, target_schema, out);
}

int32_t pp_graph_preferred_path_at(
    const uint8_t *graph_ptr,
    size_t graph_len,
    const uint8_t *source_schema_ptr,
    size_t source_schema_len,
    const uint8_t *target_schema_ptr,
    size_t target_schema_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t graph = { .ptr = graph_ptr, .len = graph_len };
    slice_ref_uint8_t source_schema = { .ptr = source_schema_ptr, .len = source_schema_len };
    slice_ref_uint8_t target_schema = { .ptr = target_schema_ptr, .len = target_schema_len };
    return pp_graph_preferred_path(graph, source_schema, target_schema, out);
}

int32_t pp_graph_conversion_distance_at(
    const uint8_t *graph_ptr,
    size_t graph_len,
    const uint8_t *source_schema_ptr,
    size_t source_schema_len,
    const uint8_t *target_schema_ptr,
    size_t target_schema_len,
    double *out_distance
) {
    slice_ref_uint8_t graph = { .ptr = graph_ptr, .len = graph_len };
    slice_ref_uint8_t source_schema = { .ptr = source_schema_ptr, .len = source_schema_len };
    slice_ref_uint8_t target_schema = { .ptr = target_schema_ptr, .len = target_schema_len };
    return pp_graph_conversion_distance(graph, source_schema, target_schema, out_distance);
}

/* ---------- parse (feature `full-parse`) ---------- */

#ifdef PANPROTO_PARSE

int32_t pp_parse_file_at(
    uint32_t registry,
    const uint8_t *path_ptr,
    size_t path_len,
    const uint8_t *content_ptr,
    size_t content_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t path = { .ptr = path_ptr, .len = path_len };
    slice_ref_uint8_t content = { .ptr = content_ptr, .len = content_len };
    return pp_parse_file(registry, path, content, out_handle);
}

int32_t pp_parse_with_protocol_at(
    uint32_t registry,
    const uint8_t *protocol_ptr,
    size_t protocol_len,
    const uint8_t *content_ptr,
    size_t content_len,
    const uint8_t *file_path_ptr,
    size_t file_path_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t protocol = { .ptr = protocol_ptr, .len = protocol_len };
    slice_ref_uint8_t content = { .ptr = content_ptr, .len = content_len };
    slice_ref_uint8_t file_path = { .ptr = file_path_ptr, .len = file_path_len };
    return pp_parse_with_protocol(registry, protocol, content, file_path, out_handle);
}

int32_t pp_parse_detect_language_at(
    uint32_t registry,
    const uint8_t *path_ptr,
    size_t path_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t path = { .ptr = path_ptr, .len = path_len };
    return pp_parse_detect_language(registry, path, out);
}

int32_t pp_parse_emit_at(
    uint32_t registry,
    const uint8_t *protocol_ptr,
    size_t protocol_len,
    uint32_t schema,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t protocol = { .ptr = protocol_ptr, .len = protocol_len };
    return pp_parse_emit(registry, protocol, schema, out);
}

int32_t pp_parse_emit_pretty_at(
    uint32_t registry,
    const uint8_t *protocol_ptr,
    size_t protocol_len,
    uint32_t schema,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t protocol = { .ptr = protocol_ptr, .len = protocol_len };
    return pp_parse_emit_pretty(registry, protocol, schema, out);
}

int32_t pp_parse_check_emit_parse_at(
    uint32_t registry,
    const uint8_t *protocol_ptr,
    size_t protocol_len,
    uint32_t schema,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t protocol = { .ptr = protocol_ptr, .len = protocol_len };
    return pp_parse_check_emit_parse(registry, protocol, schema, out);
}

int32_t pp_parse_check_parse_emit_at(
    uint32_t registry,
    const uint8_t *protocol_ptr,
    size_t protocol_len,
    const uint8_t *bytes_ptr,
    size_t bytes_len,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t protocol = { .ptr = protocol_ptr, .len = protocol_len };
    slice_ref_uint8_t bytes = { .ptr = bytes_ptr, .len = bytes_len };
    return pp_parse_check_parse_emit(registry, protocol, bytes, out);
}

#endif /* PANPROTO_PARSE */

/* ---------- project (feature `project`) ---------- */

#ifdef PANPROTO_PROJECT

int32_t pp_project_add_file_at(
    uint32_t builder,
    const uint8_t *path_ptr,
    size_t path_len,
    const uint8_t *content_ptr,
    size_t content_len
) {
    slice_ref_uint8_t path = { .ptr = path_ptr, .len = path_len };
    slice_ref_uint8_t content = { .ptr = content_ptr, .len = content_len };
    return pp_project_add_file(builder, path, content);
}

int32_t pp_project_add_directory_at(
    uint32_t builder,
    const uint8_t *path_ptr,
    size_t path_len
) {
    slice_ref_uint8_t path = { .ptr = path_ptr, .len = path_len };
    return pp_project_add_directory(builder, path);
}

#endif /* PANPROTO_PROJECT */

/* ---------- git (feature `git`) ---------- */

#ifdef PANPROTO_GIT

int32_t pp_git_import_at(
    const uint8_t *repo_path_ptr,
    size_t repo_path_len,
    const uint8_t *revspec_ptr,
    size_t revspec_len,
    uint32_t *out_handle,
    Vec_uint8_t *out
) {
    slice_ref_uint8_t repo_path = { .ptr = repo_path_ptr, .len = repo_path_len };
    slice_ref_uint8_t revspec = { .ptr = revspec_ptr, .len = revspec_len };
    return pp_git_import(repo_path, revspec, out_handle, out);
}

#endif /* PANPROTO_GIT */
