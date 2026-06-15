//! I/O protocol registry and built-in protocol catalogue access.
//!
//! Frozen-signature scaffold. Every entry point currently returns
//! [`PpStatus::Operation`]; the engine-wiring pass fills in the bodies
//! against `panproto_core::io` and
//! [`crate::api::helpers`].

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Create an I/O protocol registry with all built-in protocol codecs.
///
/// On success, `out_handle` receives a fresh
/// [`Resource::IoRegistry`](crate::handle::Resource) handle. Will call
/// `io::default_registry`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_io_register_protocols(out_handle: &mut u32) -> i32 {
    let _ = out_handle;
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_io_register_protocols".into(),
        ))
    })
}

/// List all protocol names registered in an I/O registry.
///
/// `registry` is a [`Resource::IoRegistry`](crate::handle::Resource)
/// handle. On success, `out` receives a CBOR-encoded `Vec<String>`.
/// Will call `ProtocolRegistry::protocol_names`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_io_list_protocols(registry: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (registry, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_io_list_protocols".into(),
        ))
    })
}

/// Parse raw format bytes into an instance using a protocol codec.
///
/// `registry` is an I/O registry handle; `proto_name` is the UTF-8
/// protocol name bytes; `schema_handle` is a
/// [`Resource::Schema`](crate::handle::Resource) handle; `input` is the
/// raw format bytes. On success, `out` receives the CBOR-encoded
/// instance (W-type or functor, per the protocol's native
/// representation). Will dispatch on `ProtocolRegistry::native_repr`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_io_parse_instance(
    registry: u32,
    proto_name: c_slice::Ref<'_, u8>,
    schema_handle: u32,
    input: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (registry, proto_name, schema_handle, input, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_io_parse_instance".into(),
        ))
    })
}

/// Emit an instance to raw format bytes using a protocol codec.
///
/// `registry` is an I/O registry handle; `proto_name` is the UTF-8
/// protocol name bytes; `schema_handle` is a schema handle; `instance`
/// is the CBOR-encoded instance. On success, `out` receives the raw
/// format bytes. Will dispatch on `ProtocolRegistry::native_repr`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_io_emit_instance(
    registry: u32,
    proto_name: c_slice::Ref<'_, u8>,
    schema_handle: u32,
    instance: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (registry, proto_name, schema_handle, instance, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_io_emit_instance".into(),
        ))
    })
}

/// List all built-in semantic protocol names.
///
/// On success, `out` receives a CBOR-encoded `Vec<String>`. Will call
/// [`crate::api::helpers::builtin_protocol_names`].
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_registry_list_builtin(out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = out;
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_registry_list_builtin".into(),
        ))
    })
}

/// Get a built-in protocol specification by name.
///
/// `name` is the UTF-8 protocol name bytes. On success, `out` receives
/// the CBOR-encoded `panproto_core::schema::Protocol`. Will call
/// [`crate::api::helpers::lookup_builtin_protocol`].
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_registry_get_builtin(name: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (name, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_registry_get_builtin".into(),
        ))
    })
}
