//! W-type instance validation and JSON conversion.
//!
//! Frozen-signature scaffold. Every entry point currently returns
//! [`PpStatus::Operation`]; the engine-wiring pass fills in the bodies
//! against `panproto_core::inst`.

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Validate a W-type instance against a schema.
///
/// `schema_handle` is a [`Resource::Schema`](crate::handle::Resource)
/// handle. `instance` is a CBOR-encoded
/// `panproto_core::inst::WInstance`. On success, `out` receives a
/// CBOR-encoded `Vec<String>` of validation messages (empty means
/// valid). Will call `inst::validate_wtype`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_inst_validate(
    schema_handle: u32,
    instance: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (schema_handle, instance, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_inst_validate".into(),
        ))
    })
}

/// Convert a W-type instance to JSON bytes.
///
/// `schema_handle` is a [`Resource::Schema`](crate::handle::Resource)
/// handle. `instance` is a CBOR-encoded `WInstance`. On success, `out`
/// receives the JSON bytes. Will call `inst::to_json`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_inst_to_json(
    schema_handle: u32,
    instance: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (schema_handle, instance, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_inst_to_json".into())))
}

/// Parse JSON bytes into a W-type instance.
///
/// `schema_handle` is a [`Resource::Schema`](crate::handle::Resource)
/// handle. `json` is raw JSON bytes (decoded with `serde_json`, not
/// CBOR). `root_vertex` selects the root vertex (empty infers it). On
/// success, `out` receives a CBOR-encoded `WInstance`. Will call
/// `inst::parse_json`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_inst_json_to_instance(
    schema_handle: u32,
    json: c_slice::Ref<'_, u8>,
    root_vertex: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (schema_handle, json, root_vertex, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_inst_json_to_instance".into(),
        ))
    })
}

/// Count the nodes in a W-type instance.
///
/// `instance` is a CBOR-encoded `WInstance`. On success, `out_count`
/// receives the node count. Will call `WInstance::node_count`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_inst_element_count(instance: c_slice::Ref<'_, u8>, out_count: &mut u32) -> i32 {
    let _ = (instance, out_count);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_inst_element_count".into(),
        ))
    })
}
