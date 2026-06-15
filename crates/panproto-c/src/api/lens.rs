//! Lens auto-generation, law checking, get/put, composition, and the
//! full protolens chain surface (instantiate, complement spec, diff,
//! compose, JSON I/O, fuse, symmetric lenses, DSL compilation).
//!
//! Frozen-signature scaffold. Every entry point currently returns
//! [`PpStatus::Operation`]; the engine-wiring pass fills in the bodies
//! against `panproto_core::lens`, `panproto-lens-dsl`, and
//! [`crate::api::helpers`].

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Auto-generate a protolens chain between two schemas.
///
/// `schema1` and `schema2` are
/// [`Resource::Schema`](crate::handle::Resource) handles; `stringency`
/// is the UTF-8 tier name (`strict`/`balanced`/`lenient`/`exploratory`,
/// empty for default). On success, `out_handle` receives a fresh
/// [`Resource::ProtolensChain`](crate::handle::Resource) handle. Will
/// call `lens::auto_generate`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_auto_generate_protolens(
    schema1: u32,
    schema2: u32,
    stringency: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    let _ = (schema1, schema2, stringency, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_lens_auto_generate_protolens".into(),
        ))
    })
}

/// Auto-generate up to `top_n` ranked candidate lenses.
///
/// `schema1` and `schema2` are schema handles; `stringency` is the
/// UTF-8 tier name. On success, `out` receives a CBOR-encoded
/// `{ candidates, coerce_proposals }` record. Will call
/// `lens::auto_generate_candidates`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_auto_generate_candidates(
    schema1: u32,
    schema2: u32,
    top_n: u32,
    stringency: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (schema1, schema2, top_n, stringency, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_lens_auto_generate_candidates".into(),
        ))
    })
}

/// Check both `GetPut` and `PutGet` lens laws on a test instance.
///
/// `migration` is a migration/lens handle; `instance` is a CBOR-encoded
/// `WInstance`. On success, `out` receives a CBOR-encoded
/// [`LawCheckResult`](crate::api::helpers::LawCheckResult). Will call
/// `lens::check_laws`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_check_laws(
    migration: u32,
    instance: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (migration, instance, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_lens_check_laws".into(),
        ))
    })
}

/// Check the `GetPut` lens law on a test instance.
///
/// Arguments and payload match [`pp_lens_check_laws`]. Will call
/// `lens::check_get_put`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_check_get_put(
    migration: u32,
    instance: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (migration, instance, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_lens_check_get_put".into(),
        ))
    })
}

/// Check the `PutGet` lens law on a test instance.
///
/// Arguments and payload match [`pp_lens_check_laws`]. Will call
/// `lens::check_put_get`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_check_put_get(
    migration: u32,
    instance: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (migration, instance, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_lens_check_put_get".into(),
        ))
    })
}

/// Bidirectional get: extract a view and complement from a record.
///
/// `migration` is a migration/lens handle; `record` is a CBOR-encoded
/// `WInstance`. On success, `out` receives a CBOR-encoded
/// `{ view: WInstance, complement: Vec<u8> }`. Will call `lens::get`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_get_record(
    migration: u32,
    record: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (migration, record, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_lens_get_record".into(),
        ))
    })
}

/// Bidirectional put: restore a record from a view and complement.
///
/// `migration` is a migration/lens handle; `view` and `complement` are
/// CBOR-encoded `WInstance` and `Complement`. On success, `out`
/// receives the CBOR-encoded restored `WInstance`. Will call
/// `lens::put`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_put_record(
    migration: u32,
    view: c_slice::Ref<'_, u8>,
    complement: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (migration, view, complement, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_lens_put_record".into(),
        ))
    })
}

/// Compose two lenses sequentially.
///
/// `l1` and `l2` are migration/lens handles. On success, `out_handle`
/// receives a fresh
/// [`Resource::MigrationWithSchemas`](crate::handle::Resource) handle.
/// Will call `lens::compose`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_compose(l1: u32, l2: u32, out_handle: &mut u32) -> i32 {
    let _ = (l1, l2, out_handle);
    guard(|| Err(FfiError::Operation("unimplemented: pp_lens_compose".into())))
}

/// Instantiate a protolens chain at a specific schema.
///
/// `chain` is a [`Resource::ProtolensChain`](crate::handle::Resource)
/// handle; `schema` is a [`Resource::Schema`](crate::handle::Resource)
/// handle. On success, `out_handle` receives a fresh
/// [`Resource::MigrationWithSchemas`](crate::handle::Resource) handle.
/// Will call `ProtolensChain::instantiate`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protolens_instantiate(chain: u32, schema: u32, out_handle: &mut u32) -> i32 {
    let _ = (chain, schema, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_protolens_instantiate".into(),
        ))
    })
}

/// Get the complement spec for a protolens chain at a schema.
///
/// `chain` is a protolens chain handle; `schema` is a schema handle. On
/// success, `out` receives a CBOR-encoded
/// `panproto_core::lens::ComplementSpec`. Will call
/// `lens::chain_complement_spec`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protolens_complement_spec(chain: u32, schema: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (chain, schema, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_protolens_complement_spec".into(),
        ))
    })
}

/// Build a protolens chain from a diff spec.
///
/// `diff` is a CBOR-encoded `panproto_core::lens::DiffSpec`; `schema1`
/// and `schema2` are schema handles. On success, `out_handle` receives
/// a fresh [`Resource::ProtolensChain`](crate::handle::Resource)
/// handle. Will call `lens::diff_to_protolens`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protolens_from_diff(
    diff: c_slice::Ref<'_, u8>,
    schema1: u32,
    schema2: u32,
    out_handle: &mut u32,
) -> i32 {
    let _ = (diff, schema1, schema2, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_protolens_from_diff".into(),
        ))
    })
}

/// Compose two protolens chains.
///
/// `chain1` and `chain2` are protolens chain handles. On success,
/// `out_handle` receives a fresh
/// [`Resource::ProtolensChain`](crate::handle::Resource) handle.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protolens_compose(chain1: u32, chain2: u32, out_handle: &mut u32) -> i32 {
    let _ = (chain1, chain2, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_protolens_compose".into(),
        ))
    })
}

/// Serialize a protolens chain to JSON.
///
/// `chain` is a protolens chain handle. On success, `out` receives JSON
/// bytes describing each step (name, endofunctors, lossless flag) per
/// [`ProtolensStepInfo`](crate::api::helpers::ProtolensStepInfo).
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protolens_chain_to_json(chain: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (chain, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_protolens_chain_to_json".into(),
        ))
    })
}

/// Deserialize a protolens chain from JSON.
///
/// `json` is raw JSON bytes. On success, `out_handle` receives a fresh
/// [`Resource::ProtolensChain`](crate::handle::Resource) handle. Will
/// call `ProtolensChain::from_json`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protolens_from_json(json: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    let _ = (json, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_protolens_from_json".into(),
        ))
    })
}

/// Fuse a protolens chain into a single composite step.
///
/// `chain` is a protolens chain handle. On success, `out_handle`
/// receives a fresh
/// [`Resource::ProtolensChain`](crate::handle::Resource) handle holding
/// the fused step. Will call `ProtolensChain::fuse`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protolens_fuse(chain: u32, out_handle: &mut u32) -> i32 {
    let _ = (chain, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_protolens_fuse".into(),
        ))
    })
}

/// Auto-generate a symmetric lens from two schemas.
///
/// `schema1` and `schema2` are
/// [`Resource::Schema`](crate::handle::Resource) handles. On success,
/// `out_handle` receives a fresh
/// [`Resource::SymmetricLensHandle`](crate::handle::Resource) handle.
/// Will call `SymmetricLens::auto_symmetric`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_symmetric_from_schemas(schema1: u32, schema2: u32, out_handle: &mut u32) -> i32 {
    let _ = (schema1, schema2, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_lens_symmetric_from_schemas".into(),
        ))
    })
}

/// Sync data through a symmetric lens.
///
/// `sym_lens` is a symmetric-lens handle; `view` and `complement` are
/// CBOR-encoded `WInstance` and `Complement`; `direction` is `0`
/// (left-to-right) or `1` (right-to-left). On success, `out` receives
/// the CBOR-encoded synced `WInstance`. Will call
/// `SymmetricLens::sync_left_to_right` / `sync_right_to_left`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_symmetric_sync(
    sym_lens: u32,
    view: c_slice::Ref<'_, u8>,
    complement: c_slice::Ref<'_, u8>,
    direction: u8,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (sym_lens, view, complement, direction, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_lens_symmetric_sync".into(),
        ))
    })
}

/// Compile a lens DSL document into a protolens chain.
///
/// `source` is UTF-8 DSL source; `format` is the UTF-8 format name
/// (`json` or `yaml`); `body_vertex` is the UTF-8 parent vertex id for
/// field-level steps. On success, `out_handle` receives a fresh
/// [`Resource::ProtolensChain`](crate::handle::Resource) handle. Will
/// call `panproto_lens_dsl::{eval, compile}`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_compile_document(
    source: c_slice::Ref<'_, u8>,
    format: c_slice::Ref<'_, u8>,
    body_vertex: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    let _ = (source, format, body_vertex, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_lens_compile_document".into(),
        ))
    })
}
