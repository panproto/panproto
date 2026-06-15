//! I/O protocol registry and built-in protocol catalogue access.
//!
//! Ported from `panproto_wasm::api::instance` (registry-backed parse/emit)
//! and `panproto_wasm::api::registry` (built-in catalogue), narrowed to the
//! six entry points the C ABI exposes. The WASM `WasmError`/`JsError` pair
//! becomes [`FfiError`], `rmp_serde` becomes [`crate::canonical`], and the
//! WASM slab becomes [`crate::handle`].
//!
//! An I/O registry lives in the slab as
//! [`Resource::IoRegistry`](crate::handle::Resource); the schema it parses
//! against is a [`Resource::Schema`](crate::handle::Resource) handle. The
//! anchoring schema crosses as a handle, while instances cross as
//! CBOR-encoded `WInstance`/`FInstance` values and raw format bytes cross
//! as opaque byte buffers. Built-in protocol catalogue access reuses
//! [`crate::api::helpers::builtin_protocol_names`] and
//! [`crate::api::helpers::lookup_builtin_protocol`].

use panproto_core::{inst, io};
use safer_ffi::prelude::*;

use crate::api::helpers::{builtin_protocol_names, lookup_builtin_protocol};
use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource};
use crate::panic::guard;

/// Create an I/O protocol registry with all built-in protocol codecs.
///
/// On success, `out_handle` receives a fresh
/// [`Resource::IoRegistry`](crate::handle::Resource) handle holding
/// `io::default_registry()` and [`PpStatus::Ok`] is returned.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_io_register_protocols(out_handle: &mut u32) -> i32 {
    guard(|| {
        *out_handle = handle::alloc(Resource::IoRegistry(Box::new(io::default_registry())));
        Ok(PpStatus::Ok)
    })
}

/// List all protocol names registered in an I/O registry.
///
/// `registry` is a [`Resource::IoRegistry`](crate::handle::Resource)
/// handle. On success, `out` receives a CBOR-encoded `Vec<String>`. Calls
/// `ProtocolRegistry::protocol_names`.
///
/// Returns [`PpStatus::InvalidHandle`] / [`PpStatus::TypeMismatch`] if the
/// handle is invalid or not an I/O registry.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_io_list_protocols(registry: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let names: Vec<String> = handle::with_resource(registry, |r| {
            let reg = r.as_io_registry()?;
            Ok(reg.protocol_names().map(str::to_owned).collect())
        })?;
        let bytes = crate::canonical::encode(&names)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Parse raw format bytes into an instance using a protocol codec.
///
/// `registry` is an I/O registry handle; `proto_name` is the UTF-8
/// protocol name bytes; `schema_handle` is a
/// [`Resource::Schema`](crate::handle::Resource) handle; `input` is the
/// raw format bytes. On success, `out` receives the CBOR-encoded instance
/// (`WInstance` or `FInstance`, per the protocol's native representation).
/// Dispatches on `ProtocolRegistry::native_repr`, mirroring the WASM
/// boundary's `parse_instance`.
///
/// Returns [`PpStatus::InvalidHandle`] / [`PpStatus::TypeMismatch`] for a
/// bad handle, and [`PpStatus::Operation`] if the protocol is unknown or
/// the parse fails.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_io_parse_instance(
    registry: u32,
    proto_name: c_slice::Ref<'_, u8>,
    schema_handle: u32,
    input: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let name = std::str::from_utf8(proto_name.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid protocol name UTF-8: {e}")))?;

        let schema = handle::with_resource(schema_handle, |r| Ok(r.as_schema()?.clone()))?;

        let bytes = handle::with_resource(registry, |r| {
            let reg = r.as_io_registry()?;
            let repr = reg
                .native_repr(name)
                .map_err(|e| FfiError::Operation(format!("native_repr: {e}")))?;

            match repr {
                io::NativeRepr::WType | io::NativeRepr::Either => {
                    let instance = reg
                        .parse_wtype(name, &schema, input.as_slice())
                        .map_err(|e| FfiError::Operation(format!("parse_wtype: {e}")))?;
                    crate::canonical::encode(&instance)
                }
                io::NativeRepr::Functor => {
                    let instance = reg
                        .parse_functor(name, &schema, input.as_slice())
                        .map_err(|e| FfiError::Operation(format!("parse_functor: {e}")))?;
                    crate::canonical::encode(&instance)
                }
            }
        })?;

        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Emit an instance to raw format bytes using a protocol codec.
///
/// `registry` is an I/O registry handle; `proto_name` is the UTF-8
/// protocol name bytes; `schema_handle` is a schema handle; `instance` is
/// the CBOR-encoded instance (`WInstance` or `FInstance`). On success,
/// `out` receives the raw format bytes (not CBOR). Dispatches on
/// `ProtocolRegistry::native_repr`, mirroring the WASM boundary's
/// `emit_instance`.
///
/// Returns [`PpStatus::InvalidHandle`] / [`PpStatus::TypeMismatch`] for a
/// bad handle, [`PpStatus::Serialization`] if the instance bytes do not
/// decode, and [`PpStatus::Operation`] if the protocol is unknown or the
/// emit fails.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_io_emit_instance(
    registry: u32,
    proto_name: c_slice::Ref<'_, u8>,
    schema_handle: u32,
    instance: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let name = std::str::from_utf8(proto_name.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid protocol name UTF-8: {e}")))?;

        let schema = handle::with_resource(schema_handle, |r| Ok(r.as_schema()?.clone()))?;

        // Determine the native representation under the registry borrow,
        // then decode the matching instance type. The decode happens
        // outside the closure so a malformed payload surfaces as
        // `Serialization` rather than `Operation`.
        let repr = handle::with_resource(registry, |r| {
            let reg = r.as_io_registry()?;
            reg.native_repr(name)
                .map_err(|e| FfiError::Operation(format!("native_repr: {e}")))
        })?;

        let raw = match repr {
            io::NativeRepr::WType | io::NativeRepr::Either => {
                let inst_value: inst::WInstance = crate::canonical::decode(instance.as_slice())?;
                handle::with_resource(registry, |r| {
                    let reg = r.as_io_registry()?;
                    reg.emit_wtype(name, &schema, &inst_value)
                        .map_err(|e| FfiError::Operation(format!("emit_wtype: {e}")))
                })?
            }
            io::NativeRepr::Functor => {
                let inst_value: inst::FInstance = crate::canonical::decode(instance.as_slice())?;
                handle::with_resource(registry, |r| {
                    let reg = r.as_io_registry()?;
                    reg.emit_functor(name, &schema, &inst_value)
                        .map_err(|e| FfiError::Operation(format!("emit_functor: {e}")))
                })?
            }
        };

        *out = raw.into();
        Ok(PpStatus::Ok)
    })
}

/// List all built-in semantic protocol names.
///
/// On success, `out` receives a CBOR-encoded `Vec<String>`. Calls
/// [`crate::api::helpers::builtin_protocol_names`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_registry_list_builtin(out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let names = builtin_protocol_names();
        let bytes = crate::canonical::encode(&names)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Get a built-in protocol specification by name.
///
/// `name` is the UTF-8 protocol name bytes. On success, `out` receives
/// the CBOR-encoded `panproto_core::schema::Protocol`. Calls
/// [`crate::api::helpers::lookup_builtin_protocol`].
///
/// Returns [`PpStatus::Operation`] if the name is not a recognized
/// built-in protocol.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_registry_get_builtin(name: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let name_str = std::str::from_utf8(name.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid protocol name UTF-8: {e}")))?;

        let protocol = lookup_builtin_protocol(name_str)
            .ok_or_else(|| FfiError::Operation(format!("unknown protocol: {name_str}")))?;

        let bytes = crate::canonical::encode(&protocol)?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use panproto_core::schema::{Protocol, Schema, SchemaBuilder};

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free};
    use crate::canonical::decode;

    fn instance_slice(bytes: &[u8]) -> c_slice::Box<u8> {
        bytes.to_vec().into_boxed_slice().into()
    }

    /// A schema with a `post` record carrying a string `text` property.
    /// The `geojson` codec (a registered JSON-based protocol) is
    /// W-type-native, so this drives the W-type parse/emit path. `post`
    /// has no incoming edges, so the codec anchors the instance root there.
    fn post_schema() -> Schema {
        let proto = crate::api::helpers::default_protocol("geojson");
        SchemaBuilder::new(&proto)
            .vertex("post", "record", None)
            .unwrap()
            .vertex("text", "string", None)
            .unwrap()
            .edge("post", "text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    fn register_registry() -> u32 {
        let mut handle = u32::MAX;
        let status = pp_io_register_protocols(&mut handle);
        assert_eq!(status, PpStatus::Ok as i32);
        handle
    }

    #[test]
    fn register_and_list_protocols() {
        let reg = register_registry();

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_io_list_protocols(reg, &mut out);
        assert_eq!(status, PpStatus::Ok as i32);
        let names: Vec<String> = decode(&out).unwrap();
        pp_buf_free(out);

        assert!(!names.is_empty(), "registry should list protocols");
        assert!(
            names.iter().any(|n| n == "geojson"),
            "expected geojson in {names:?}"
        );

        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }

    #[test]
    fn list_protocols_rejects_non_registry_handle() {
        // A Schema handle is not an IoRegistry.
        let schema_h = handle::alloc(Resource::Schema(Arc::new(post_schema())));
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_io_list_protocols(schema_h, &mut out);
        assert_eq!(status, PpStatus::TypeMismatch as i32);
        pp_buf_free(out);
        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
    }

    #[test]
    fn list_protocols_invalid_handle() {
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_io_list_protocols(u32::MAX - 1, &mut out);
        assert_eq!(status, PpStatus::InvalidHandle as i32);
        pp_buf_free(out);
    }

    #[test]
    fn parse_emit_round_trip_geojson() {
        let reg = register_registry();
        let schema_h = handle::alloc(Resource::Schema(Arc::new(post_schema())));

        let proto = instance_slice(b"geojson");
        let input = instance_slice(br#"{"text": "hello"}"#);

        // Parse the raw JSON into a CBOR instance.
        let mut parsed: repr_c::Vec<u8> = Vec::new().into();
        let status =
            pp_io_parse_instance(reg, proto.as_ref(), schema_h, input.as_ref(), &mut parsed);
        assert_eq!(
            status,
            PpStatus::Ok as i32,
            "parse failed; last error: {:?}",
            crate::error::take_last_error()
        );
        let parsed_bytes = parsed.to_vec();
        assert!(!parsed_bytes.is_empty());
        pp_buf_free(parsed);

        // Emit the instance back to raw bytes.
        let proto2 = instance_slice(b"geojson");
        let inst_slice = instance_slice(&parsed_bytes);
        let mut emitted: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_io_emit_instance(
            reg,
            proto2.as_ref(),
            schema_h,
            inst_slice.as_ref(),
            &mut emitted,
        );
        assert_eq!(
            status,
            PpStatus::Ok as i32,
            "emit failed; last error: {:?}",
            crate::error::take_last_error()
        );
        let emitted_bytes = emitted.to_vec();
        pp_buf_free(emitted);

        // The emitted bytes must be JSON carrying our text value.
        let value: serde_json::Value =
            serde_json::from_slice(&emitted_bytes).expect("emit produced non-JSON output");
        assert_eq!(value.get("text").and_then(|v| v.as_str()), Some("hello"));

        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }

    #[test]
    fn parse_unknown_protocol_is_operation_error() {
        let reg = register_registry();
        let schema_h = handle::alloc(Resource::Schema(Arc::new(post_schema())));

        let proto = instance_slice(b"no-such-protocol");
        let input = instance_slice(br#"{"text": "x"}"#);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_io_parse_instance(reg, proto.as_ref(), schema_h, input.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Operation as i32);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }

    #[test]
    fn emit_rejects_garbage_instance() {
        let reg = register_registry();
        let schema_h = handle::alloc(Resource::Schema(Arc::new(post_schema())));

        let proto = instance_slice(b"geojson");
        let bad = instance_slice(&[0xFFu8, 0xFE, 0xFD]);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_io_emit_instance(reg, proto.as_ref(), schema_h, bad.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Serialization as i32);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }

    #[test]
    fn list_builtin_protocols_non_empty() {
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_registry_list_builtin(&mut out);
        assert_eq!(status, PpStatus::Ok as i32);
        let names: Vec<String> = decode(&out).unwrap();
        pp_buf_free(out);
        assert!(names.iter().any(|n| n == "atproto"), "names = {names:?}");
    }

    #[test]
    fn get_builtin_protocol_round_trips() {
        let name = instance_slice(b"atproto");
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_registry_get_builtin(name.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Ok as i32);
        let protocol: Protocol = decode(&out).unwrap();
        pp_buf_free(out);
        assert_eq!(protocol.name, "atproto");
    }

    #[test]
    fn get_builtin_protocol_unknown_is_operation_error() {
        let name = instance_slice(b"no-such-protocol");
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_registry_get_builtin(name.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Operation as i32);
        pp_buf_free(out);
    }
}
