/*
 * Pointer-based glue around the panproto-c by-value FFI.
 *
 * GHC's `foreign import capi` cannot reliably pass structs by value
 * across all platforms. The functions declared here are tiny
 * forwarding shims (defined in `panproto_glue.c`) that accept
 * pointers and forward to the by-value Rust API. Haskell imports
 * these instead of the `slice_ref_uint8_t`-by-value entry points and
 * the by-value `pp_buf_free`.
 *
 * Every panproto-c entry point that takes one or more
 * `slice_ref_uint8_t` arguments has a `*_at` wrapper here that takes
 * the equivalent `(const uint8_t *ptr, size_t len)` pair(s) in the
 * same positional order. Entry points whose arguments are only
 * `uint32_t`, `uint32_t *`, `Vec_uint8_t *`, `double *`, or scalar
 * `uint8_t` are imported directly and have no wrapper here.
 *
 * The feature-gated `parse` / `project` / `git` wrappers reference
 * symbols that are absent from the default-feature `panproto.h`; they
 * are guarded by `PANPROTO_PARSE` / `PANPROTO_PROJECT` / `PANPROTO_GIT`
 * so the default build compiles against the default cdylib.
 */

#ifndef PANPROTO_GLUE_H
#define PANPROTO_GLUE_H

#include "panproto.h"
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ---------- lifecycle ---------- */

void pp_buf_free_at(Vec_uint8_t *buf);

/* ---------- protocol ---------- */

int32_t pp_protocol_define_at(
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
);

/* ---------- schema ---------- */

int32_t pp_schema_from_cbor_at(
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
);

int32_t pp_schema_build_at(
    uint32_t proto,
    const uint8_t *ops_ptr,
    size_t ops_len,
    uint32_t *out_handle
);

int32_t pp_schema_parse_atproto_lexicon_at(
    const uint8_t *json_ptr,
    size_t json_len,
    uint32_t *out_handle
);

/* ---------- check ---------- */

int32_t pp_check_classify_at(
    uint32_t proto,
    const uint8_t *diff_ptr,
    size_t diff_len,
    Vec_uint8_t *out
);

int32_t pp_check_report_text_at(
    const uint8_t *report_ptr,
    size_t report_len,
    Vec_uint8_t *out
);

int32_t pp_check_report_json_at(
    const uint8_t *report_ptr,
    size_t report_len,
    Vec_uint8_t *out
);

/* ---------- mig ---------- */

int32_t pp_mig_check_existence_at(
    uint32_t proto,
    uint32_t src,
    uint32_t tgt,
    const uint8_t *mapping_ptr,
    size_t mapping_len,
    Vec_uint8_t *out
);

int32_t pp_mig_compile_at(
    uint32_t src,
    uint32_t tgt,
    const uint8_t *mapping_ptr,
    size_t mapping_len,
    uint32_t *out_handle
);

int32_t pp_mig_lift_record_at(
    uint32_t migration,
    const uint8_t *record_ptr,
    size_t record_len,
    Vec_uint8_t *out
);

int32_t pp_mig_invert_at(
    const uint8_t *mapping_ptr,
    size_t mapping_len,
    uint32_t src,
    uint32_t tgt,
    Vec_uint8_t *out
);

int32_t pp_mig_coverage_at(
    uint32_t migration,
    uint32_t src,
    uint32_t tgt,
    const uint8_t *instances_ptr,
    size_t instances_len,
    Vec_uint8_t *out
);

int32_t pp_mig_lift_json_at(
    uint32_t migration,
    const uint8_t *json_ptr,
    size_t json_len,
    const uint8_t *root_vertex_ptr,
    size_t root_vertex_len,
    Vec_uint8_t *out
);

/* ---------- hom ---------- */

int32_t pp_hom_find_morphisms_at(
    uint32_t src,
    uint32_t tgt,
    const uint8_t *opts_ptr,
    size_t opts_len,
    Vec_uint8_t *out
);

int32_t pp_hom_find_best_morphism_at(
    uint32_t src,
    uint32_t tgt,
    const uint8_t *opts_ptr,
    size_t opts_len,
    Vec_uint8_t *out
);

int32_t pp_hom_find_span_at(
    uint32_t src,
    uint32_t tgt,
    uint32_t protocol,
    const uint8_t *opts_ptr,
    size_t opts_len,
    const uint8_t *constraints_ptr,
    size_t constraints_len,
    Vec_uint8_t *out
);

int32_t pp_hom_span_to_overlap_at(
    const uint8_t *span_ptr,
    size_t span_len,
    Vec_uint8_t *out
);

int32_t pp_hom_morphism_to_migration_at(
    const uint8_t *morphism_ptr,
    size_t morphism_len,
    uint32_t *out_handle
);

int32_t pp_hom_induce_schema_morphism_at(
    const uint8_t *theory_morphism_ptr,
    size_t theory_morphism_len,
    uint32_t src,
    Vec_uint8_t *out
);

int32_t pp_hom_induce_migration_from_theory_at(
    const uint8_t *theory_morphism_ptr,
    size_t theory_morphism_len,
    uint32_t src,
    uint32_t tgt,
    Vec_uint8_t *out,
    uint32_t *out_handle
);

/* ---------- instance ---------- */

int32_t pp_inst_validate_at(
    uint32_t schema_handle,
    const uint8_t *instance_ptr,
    size_t instance_len,
    Vec_uint8_t *out
);

int32_t pp_inst_to_json_at(
    uint32_t schema_handle,
    const uint8_t *instance_ptr,
    size_t instance_len,
    Vec_uint8_t *out
);

int32_t pp_inst_json_to_instance_at(
    uint32_t schema_handle,
    const uint8_t *json_ptr,
    size_t json_len,
    const uint8_t *root_vertex_ptr,
    size_t root_vertex_len,
    Vec_uint8_t *out
);

int32_t pp_inst_element_count_at(
    const uint8_t *instance_ptr,
    size_t instance_len,
    uint32_t *out_count
);

/* ---------- registry ---------- */

int32_t pp_io_parse_instance_at(
    uint32_t registry,
    const uint8_t *proto_name_ptr,
    size_t proto_name_len,
    uint32_t schema_handle,
    const uint8_t *input_ptr,
    size_t input_len,
    Vec_uint8_t *out
);

int32_t pp_io_emit_instance_at(
    uint32_t registry,
    const uint8_t *proto_name_ptr,
    size_t proto_name_len,
    uint32_t schema_handle,
    const uint8_t *instance_ptr,
    size_t instance_len,
    Vec_uint8_t *out
);

int32_t pp_registry_get_builtin_at(
    const uint8_t *name_ptr,
    size_t name_len,
    Vec_uint8_t *out
);

/* ---------- lens ---------- */

int32_t pp_lens_auto_generate_protolens_at(
    uint32_t schema1,
    uint32_t schema2,
    const uint8_t *stringency_ptr,
    size_t stringency_len,
    uint32_t *out_handle
);

int32_t pp_lens_auto_generate_candidates_at(
    uint32_t schema1,
    uint32_t schema2,
    uint32_t top_n,
    const uint8_t *stringency_ptr,
    size_t stringency_len,
    Vec_uint8_t *out
);

int32_t pp_lens_check_laws_at(
    uint32_t migration,
    const uint8_t *instance_ptr,
    size_t instance_len,
    Vec_uint8_t *out
);

int32_t pp_lens_check_get_put_at(
    uint32_t migration,
    const uint8_t *instance_ptr,
    size_t instance_len,
    Vec_uint8_t *out
);

int32_t pp_lens_check_put_get_at(
    uint32_t migration,
    const uint8_t *instance_ptr,
    size_t instance_len,
    Vec_uint8_t *out
);

int32_t pp_lens_get_record_at(
    uint32_t migration,
    const uint8_t *record_ptr,
    size_t record_len,
    Vec_uint8_t *out
);

int32_t pp_lens_put_record_at(
    uint32_t migration,
    const uint8_t *view_ptr,
    size_t view_len,
    const uint8_t *complement_ptr,
    size_t complement_len,
    Vec_uint8_t *out
);

int32_t pp_protolens_complement_spec_at(
    uint32_t chain,
    uint32_t schema,
    Vec_uint8_t *out
);

int32_t pp_protolens_from_diff_at(
    const uint8_t *diff_ptr,
    size_t diff_len,
    uint32_t schema1,
    uint32_t schema2,
    uint32_t *out_handle
);

int32_t pp_protolens_from_json_at(
    const uint8_t *json_ptr,
    size_t json_len,
    uint32_t *out_handle
);

int32_t pp_lens_symmetric_sync_at(
    uint32_t sym_lens,
    const uint8_t *view_ptr,
    size_t view_len,
    const uint8_t *complement_ptr,
    size_t complement_len,
    uint8_t direction,
    Vec_uint8_t *out
);

int32_t pp_lens_compile_document_at(
    const uint8_t *source_ptr,
    size_t source_len,
    const uint8_t *format_ptr,
    size_t format_len,
    const uint8_t *body_vertex_ptr,
    size_t body_vertex_len,
    uint32_t *out_handle
);

/* ---------- gat ---------- */

int32_t pp_gat_create_theory_at(
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
);

int32_t pp_gat_check_morphism_at(
    const uint8_t *morphism_ptr,
    size_t morphism_len,
    uint32_t domain,
    uint32_t codomain,
    Vec_uint8_t *out
);

int32_t pp_gat_migrate_model_at(
    const uint8_t *model_ptr,
    size_t model_len,
    const uint8_t *morphism_ptr,
    size_t morphism_len,
    Vec_uint8_t *out
);

int32_t pp_gat_free_model_at(
    uint32_t theory,
    const uint8_t *config_ptr,
    size_t config_len,
    uint32_t *out_handle
);

int32_t pp_gat_eval_in_model_at(
    uint32_t model,
    const uint8_t *op_name_ptr,
    size_t op_name_len,
    const uint8_t *args_ptr,
    size_t args_len,
    Vec_uint8_t *out
);

/* ---------- expr ---------- */

int32_t pp_expr_parse_at(
    const uint8_t *source_ptr,
    size_t source_len,
    Vec_uint8_t *out
);

int32_t pp_expr_eval_func_at(
    const uint8_t *expr_ptr,
    size_t expr_len,
    const uint8_t *env_ptr,
    size_t env_len,
    Vec_uint8_t *out
);

int32_t pp_expr_eval_gat_at(
    const uint8_t *expr_ptr,
    size_t expr_len,
    const uint8_t *env_ptr,
    size_t env_len,
    uint32_t theory,
    Vec_uint8_t *out
);

int32_t pp_expr_check_at(
    const uint8_t *expr_ptr,
    size_t expr_len,
    uint32_t theory,
    const uint8_t *context_ptr,
    size_t context_len,
    Vec_uint8_t *out
);

int32_t pp_query_execute_at(
    const uint8_t *query_ptr,
    size_t query_len,
    const uint8_t *instance_ptr,
    size_t instance_len,
    uint32_t schema_handle,
    Vec_uint8_t *out
);

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
);

int32_t pp_schema_add_default_at(
    uint32_t schema_handle,
    const uint8_t *vertex_name_ptr,
    size_t vertex_name_len,
    const uint8_t *expr_ptr,
    size_t expr_len,
    uint32_t *out_handle
);

int32_t pp_schema_add_merger_at(
    uint32_t schema_handle,
    const uint8_t *vertex_name_ptr,
    size_t vertex_name_len,
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
);

int32_t pp_schema_add_policy_at(
    uint32_t schema_handle,
    const uint8_t *vertex_name_ptr,
    size_t vertex_name_len,
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
);

int32_t pp_enriched_refinement_subsort_at(
    const uint8_t *base_sort_ptr,
    size_t base_sort_len,
    const uint8_t *sub_constraints_ptr,
    size_t sub_constraints_len,
    const uint8_t *super_constraints_ptr,
    size_t super_constraints_len,
    uint32_t *out_is_subsort
);

/* ---------- vcs ---------- */

int32_t pp_vcs_init_at(
    const uint8_t *protocol_name_ptr,
    size_t protocol_name_len,
    uint32_t *out_handle
);

int32_t pp_vcs_commit_at(
    uint32_t repo,
    const uint8_t *message_ptr,
    size_t message_len,
    const uint8_t *author_ptr,
    size_t author_len,
    Vec_uint8_t *out
);

int32_t pp_vcs_branch_at(
    uint32_t repo,
    const uint8_t *name_ptr,
    size_t name_len,
    Vec_uint8_t *out
);

int32_t pp_vcs_checkout_at(
    uint32_t repo,
    const uint8_t *target_ptr,
    size_t target_len,
    Vec_uint8_t *out
);

int32_t pp_vcs_merge_at(
    uint32_t repo,
    const uint8_t *branch_ptr,
    size_t branch_len,
    const uint8_t *author_ptr,
    size_t author_len,
    Vec_uint8_t *out
);

int32_t pp_vcs_diff_at(
    uint32_t repo,
    const uint8_t *from_ptr,
    size_t from_len,
    const uint8_t *to_ptr,
    size_t to_len,
    Vec_uint8_t *out
);

int32_t pp_vcs_blame_at(
    uint32_t repo,
    const uint8_t *vertex_ptr,
    size_t vertex_len,
    Vec_uint8_t *out
);

/* ---------- data ---------- */

int32_t pp_data_store_dataset_at(
    uint32_t schema_handle,
    const uint8_t *data_json_ptr,
    size_t data_json_len,
    uint32_t *out_handle
);

int32_t pp_data_migrate_backward_at(
    uint32_t dataset_handle,
    const uint8_t *complement_ptr,
    size_t complement_len,
    uint32_t src_schema,
    uint32_t tgt_schema,
    uint32_t *out_handle
);

int32_t pp_data_check_staleness_at(
    uint32_t dataset_handle,
    uint32_t schema_handle,
    Vec_uint8_t *out
);

int32_t pp_data_get_migration_complement_at(
    const uint8_t *complement_ptr,
    size_t complement_len,
    Vec_uint8_t *out
);

/* ---------- graph ---------- */

int32_t pp_graph_fiber_at_at(
    const uint8_t *instance_ptr,
    size_t instance_len,
    const uint8_t *migration_ptr,
    size_t migration_len,
    const uint8_t *target_anchor_ptr,
    size_t target_anchor_len,
    Vec_uint8_t *out
);

int32_t pp_graph_fiber_decomposition_at(
    const uint8_t *instance_ptr,
    size_t instance_len,
    const uint8_t *migration_ptr,
    size_t migration_len,
    Vec_uint8_t *out
);

int32_t pp_graph_poly_hom_at(
    const uint8_t *source_schema_ptr,
    size_t source_schema_len,
    const uint8_t *target_schema_ptr,
    size_t target_schema_len,
    Vec_uint8_t *out
);

int32_t pp_graph_preferred_path_at(
    const uint8_t *graph_ptr,
    size_t graph_len,
    const uint8_t *source_schema_ptr,
    size_t source_schema_len,
    const uint8_t *target_schema_ptr,
    size_t target_schema_len,
    Vec_uint8_t *out
);

int32_t pp_graph_conversion_distance_at(
    const uint8_t *graph_ptr,
    size_t graph_len,
    const uint8_t *source_schema_ptr,
    size_t source_schema_len,
    const uint8_t *target_schema_ptr,
    size_t target_schema_len,
    double *out_distance
);

/* ---------- parse (feature `full-parse`) ---------- */

#ifdef PANPROTO_PARSE

int32_t pp_parse_file_at(
    uint32_t registry,
    const uint8_t *path_ptr,
    size_t path_len,
    const uint8_t *content_ptr,
    size_t content_len,
    uint32_t *out_handle
);

int32_t pp_parse_with_protocol_at(
    uint32_t registry,
    const uint8_t *protocol_ptr,
    size_t protocol_len,
    const uint8_t *content_ptr,
    size_t content_len,
    const uint8_t *file_path_ptr,
    size_t file_path_len,
    uint32_t *out_handle
);

int32_t pp_parse_detect_language_at(
    uint32_t registry,
    const uint8_t *path_ptr,
    size_t path_len,
    Vec_uint8_t *out
);

int32_t pp_parse_emit_at(
    uint32_t registry,
    const uint8_t *protocol_ptr,
    size_t protocol_len,
    uint32_t schema,
    Vec_uint8_t *out
);

int32_t pp_parse_emit_pretty_at(
    uint32_t registry,
    const uint8_t *protocol_ptr,
    size_t protocol_len,
    uint32_t schema,
    Vec_uint8_t *out
);

int32_t pp_parse_check_emit_parse_at(
    uint32_t registry,
    const uint8_t *protocol_ptr,
    size_t protocol_len,
    uint32_t schema,
    Vec_uint8_t *out
);

int32_t pp_parse_check_parse_emit_at(
    uint32_t registry,
    const uint8_t *protocol_ptr,
    size_t protocol_len,
    const uint8_t *bytes_ptr,
    size_t bytes_len,
    Vec_uint8_t *out
);

#endif /* PANPROTO_PARSE */

/* ---------- project (feature `project`) ---------- */

#ifdef PANPROTO_PROJECT

int32_t pp_project_add_file_at(
    uint32_t builder,
    const uint8_t *path_ptr,
    size_t path_len,
    const uint8_t *content_ptr,
    size_t content_len
);

int32_t pp_project_add_directory_at(
    uint32_t builder,
    const uint8_t *path_ptr,
    size_t path_len
);

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
);

#endif /* PANPROTO_GIT */

#ifdef __cplusplus
}
#endif

#endif /* PANPROTO_GLUE_H */
