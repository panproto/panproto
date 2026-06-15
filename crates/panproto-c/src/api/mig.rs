//! Migration existence checking, compilation, lifting, and composition.
//!
//! Frozen-signature scaffold. Every entry point currently returns
//! [`PpStatus::Operation`]; the engine-wiring pass fills in the bodies
//! against `panproto_core::mig`.

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Check the existence conditions for a migration mapping between two
/// schemas.
///
/// `proto` is a [`Resource::Protocol`](crate::handle::Resource) handle;
/// `src` and `tgt` are [`Resource::Schema`](crate::handle::Resource)
/// handles. `mapping` is a CBOR-encoded `panproto_core::mig::Migration`.
/// On success, `out` receives a CBOR-encoded `mig::ExistenceReport`
/// (the report itself encodes validity). Will call `mig::check_existence`
/// with a theory registry from
/// [`crate::api::helpers::build_theory_registry`].
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_check_existence(
    proto: u32,
    src: u32,
    tgt: u32,
    mapping: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (proto, src, tgt, mapping, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_mig_check_existence".into(),
        ))
    })
}

/// Compile a migration for fast per-record application.
///
/// `src` and `tgt` are [`Resource::Schema`](crate::handle::Resource)
/// handles. `mapping` is a CBOR-encoded `mig::Migration`. On success,
/// `out_handle` receives a fresh
/// [`Resource::MigrationWithSchemas`](crate::handle::Resource) handle.
/// Will call `mig::compile`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_compile(
    src: u32,
    tgt: u32,
    mapping: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    let _ = (src, tgt, mapping, out_handle);
    guard(|| Err(FfiError::Operation("unimplemented: pp_mig_compile".into())))
}

/// Apply a compiled migration to a single W-type record.
///
/// `migration` is a [`Resource::Migration`](crate::handle::Resource)
/// (or `MigrationWithSchemas`) handle. `record` is a CBOR-encoded
/// `panproto_core::inst::WInstance`. On success, `out` receives the
/// CBOR-encoded migrated instance. Will call `mig::lift_wtype` via
/// [`crate::api::helpers::extract_migration_ref`].
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_lift_record(
    migration: u32,
    record: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (migration, record, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_mig_lift_record".into(),
        ))
    })
}

/// Compose two compiled migrations into a single migration.
///
/// `m1` and `m2` are [`Resource::Migration`](crate::handle::Resource)
/// handles. On success, `out_handle` receives a fresh
/// [`Resource::Migration`](crate::handle::Resource) handle. Will call
/// [`crate::api::helpers::compose_compiled`].
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_compose(m1: u32, m2: u32, out_handle: &mut u32) -> i32 {
    let _ = (m1, m2, out_handle);
    guard(|| Err(FfiError::Operation("unimplemented: pp_mig_compose".into())))
}

/// Invert a bijective migration.
///
/// `mapping` is a CBOR-encoded `mig::Migration`; `src` and `tgt` are
/// [`Resource::Schema`](crate::handle::Resource) handles. On success,
/// `out` receives the CBOR-encoded inverse `mig::Migration`. Will call
/// `mig::invert` and fail with
/// [`PpStatus::Operation`](crate::error::PpStatus::Operation) when the
/// migration is not invertible.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_invert(
    mapping: c_slice::Ref<'_, u8>,
    src: u32,
    tgt: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (mapping, src, tgt, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_mig_invert".into())))
}

/// Run coverage analysis (dry-run migration) over a batch of instances.
///
/// `migration` is a migration handle; `src` and `tgt` are
/// [`Resource::Schema`](crate::handle::Resource) handles. `instances`
/// is a CBOR-encoded `Vec<WInstance>`. On success, `out` receives a
/// CBOR-encoded coverage report (`total`, `succeeded`, `failed`,
/// `coverage_percent`, `errors`). Will call `mig::lift_wtype` per record.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_coverage(
    migration: u32,
    src: u32,
    tgt: u32,
    instances: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (migration, src, tgt, instances, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_mig_coverage".into())))
}

/// Lift a JSON record through a compiled migration, returning JSON.
///
/// `migration` is a migration handle. `json` is raw JSON bytes (decoded
/// with `serde_json`, not CBOR). `root_vertex` is the source schema
/// vertex the JSON object maps to (empty auto-detects). On success,
/// `out` receives the migrated record as JSON bytes. Will call
/// `inst::parse_json`, `mig::lift_wtype`, then `inst::to_json`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_mig_lift_json(
    migration: u32,
    json: c_slice::Ref<'_, u8>,
    root_vertex: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (migration, json, root_vertex, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_mig_lift_json".into(),
        ))
    })
}
