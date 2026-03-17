//! # panproto-wasm
//!
//! WASM bindings for panproto.
//!
//! This crate exposes the panproto API to JavaScript and TypeScript
//! consumers via `wasm-bindgen`, using a handle-based API with
//! `MessagePack` serialization for crossing the WASM boundary.
//!
//! ## Architecture
//!
//! Opaque `u32` handles reference resources stored in a thread-local
//! slab allocator. Data crosses the boundary as `MessagePack` byte
//! slices (`&[u8]` / `Vec<u8>`), never as JS objects. This avoids
//! the serialization overhead of `serde-wasm-bindgen` for structured
//! data while keeping the JS API clean.
//!
//! ## Entry Points
//!
//! `#[wasm_bindgen]` functions cover the full panproto lifecycle:
//!
//! **Core (1-10)**:
//! [`define_protocol`], [`build_schema`], [`check_existence`],
//! [`compile_migration`], [`lift_record`], [`get_record`],
//! [`put_record`], [`compose_migrations`], [`diff_schemas`],
//! [`free_handle`]
//!
//! **Check & Introspection (11-16)**:
//! [`diff_schemas_full`], [`classify_diff`], [`report_text`],
//! [`report_json`], [`normalize_schema`], [`validate_schema`]
//!
//! **Instance & I/O (17-24)**:
//! [`register_io_protocols`], [`list_io_protocols`],
//! [`parse_instance`], [`emit_instance`], [`validate_instance`],
//! [`instance_to_json`], [`json_to_instance`],
//! [`instance_element_count`]
//!
//! **Lens & Migration (25-30)**:
//! [`auto_generate_protolens`], [`check_lens_laws`],
//! [`check_get_put`], [`check_put_get`],
//! [`invert_migration`], [`compose_lenses`]
//!
//! **Protocol Registry (31-32)**:
//! [`list_builtin_protocols`], [`get_builtin_protocol`]
//!
//! **GAT Operations (33-36)**:
//! [`create_theory`], [`colimit_theories`],
//! [`check_morphism`], [`migrate_model`]
//!
//! **VCS Operations (37-48)**:
//! [`vcs_init`], [`vcs_add`], [`vcs_commit`], [`vcs_log`],
//! [`vcs_status`], [`vcs_diff`], [`vcs_branch`], [`vcs_checkout`],
//! [`vcs_merge`], [`vcs_stash`], [`vcs_stash_pop`], [`vcs_blame`]
//!
//! **Protolens Operations (49-57)**:
//! [`instantiate_protolens`], [`protolens_complement_spec`],
//! [`protolens_from_diff`], [`protolens_compose`],
//! [`protolens_chain_to_json`], [`factorize_morphism`],
//! [`symmetric_lens_from_schemas`], [`symmetric_lens_sync`],
//! [`apply_protolens_step`]

mod api;
mod error;
mod slab;

pub use api::{
    apply_protolens_step, auto_generate_protolens, build_schema, check_existence, check_get_put,
    check_lens_laws, check_morphism, check_put_get, classify_diff, colimit_theories,
    compile_migration, compose_lenses, compose_migrations, create_theory, define_protocol,
    diff_schemas, diff_schemas_full, emit_instance, factorize_morphism, free_handle,
    get_builtin_protocol, get_record, instance_element_count, instance_to_json,
    instantiate_protolens, invert_migration, json_to_instance, lift_record, list_builtin_protocols,
    list_io_protocols, migrate_model, normalize_schema, parse_instance, protolens_chain_to_json,
    protolens_complement_spec, protolens_compose, protolens_from_diff, put_record,
    register_io_protocols, report_json, report_text, symmetric_lens_from_schemas,
    symmetric_lens_sync, validate_instance, validate_schema, vcs_add, vcs_blame, vcs_branch,
    vcs_checkout, vcs_commit, vcs_diff, vcs_init, vcs_log, vcs_merge, vcs_stash, vcs_stash_pop,
    vcs_status,
};
