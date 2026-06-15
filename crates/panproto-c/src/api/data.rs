//! Data versioning: dataset storage and schema-aware migration.
//!
//! Frozen-signature scaffold. Every entry point currently returns
//! [`PpStatus::Operation`]; the engine-wiring pass fills in the bodies
//! against `panproto_core::{inst, lens, vcs}`.

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Store a data set from JSON, binding it to a schema.
///
/// `schema_handle` is a [`Resource::Schema`](crate::handle::Resource)
/// handle; `data_json` is raw JSON bytes (an array of records, decoded
/// with `serde_json`). On success, `out_handle` receives a fresh
/// [`Resource::DataSet`](crate::handle::Resource) handle. Will parse
/// each record via `inst::parse_json` and hash the schema via
/// `vcs::hash::hash_schema`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_data_store_dataset(
    schema_handle: u32,
    data_json: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    let _ = (schema_handle, data_json, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_data_store_dataset".into(),
        ))
    })
}

/// Retrieve a data set as CBOR-encoded instances.
///
/// `dataset_handle` is a
/// [`Resource::DataSet`](crate::handle::Resource) handle. On success,
/// `out` receives the CBOR-encoded `Vec<WInstance>`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_data_get_dataset(dataset_handle: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (dataset_handle, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_data_get_dataset".into(),
        ))
    })
}

/// Migrate a data set forward between two schemas.
///
/// `dataset_handle` is a data set handle; `src_schema` and `tgt_schema`
/// are [`Resource::Schema`](crate::handle::Resource) handles. Auto-
/// generates a lens, applies `get` per record, and stores both the
/// migrated data set and the complement carrier as new
/// [`Resource::DataSet`](crate::handle::Resource) handles, returned via
/// `out_data_handle` and `out_complement_handle`. Will call
/// `lens::auto_generate` and `lens::get`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_data_migrate_forward(
    dataset_handle: u32,
    src_schema: u32,
    tgt_schema: u32,
    out_data_handle: &mut u32,
    out_complement_handle: &mut u32,
) -> i32 {
    let _ = (
        dataset_handle,
        src_schema,
        tgt_schema,
        out_data_handle,
        out_complement_handle,
    );
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_data_migrate_forward".into(),
        ))
    })
}

/// Migrate a data set backward using a stored complement.
///
/// `dataset_handle` is the migrated data set handle; `complement` is
/// the CBOR-encoded `Vec<Complement>`; `src_schema` and `tgt_schema`
/// are [`Resource::Schema`](crate::handle::Resource) handles. On
/// success, `out_handle` receives a fresh
/// [`Resource::DataSet`](crate::handle::Resource) handle. Will call
/// `lens::auto_generate` and `lens::put`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_data_migrate_backward(
    dataset_handle: u32,
    complement: c_slice::Ref<'_, u8>,
    src_schema: u32,
    tgt_schema: u32,
    out_handle: &mut u32,
) -> i32 {
    let _ = (
        dataset_handle,
        complement,
        src_schema,
        tgt_schema,
        out_handle,
    );
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_data_migrate_backward".into(),
        ))
    })
}

/// Check whether a data set's schema matches a given schema.
///
/// `dataset_handle` is a data set handle; `schema_handle` is a
/// [`Resource::Schema`](crate::handle::Resource) handle. On success,
/// `out` receives a CBOR-encoded record (`stale`, `data_schema_id`,
/// `target_schema_id`). Will compare `vcs::hash::hash_schema` outputs.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_data_check_staleness(
    dataset_handle: u32,
    schema_handle: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (dataset_handle, schema_handle, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_data_check_staleness".into(),
        ))
    })
}

/// Round-trip and return a forward-migration complement carrier.
///
/// `complement` is the CBOR-encoded `Vec<Complement>` produced by a
/// forward migration. On success, `out` receives the re-encoded
/// complement bytes (validating the payload).
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_data_get_migration_complement(
    complement: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (complement, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_data_get_migration_complement".into(),
        ))
    })
}
