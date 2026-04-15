//! Instance parsing/emitting and registry-based I/O.
//!
//! Split out of the monolithic api.rs into a domain module.

use panproto_core::{
    inst::{self, WInstance},
    io, schema,
};
use wasm_bindgen::prelude::*;

use crate::error::WasmError;
use crate::slab::{self, Resource};

// ---------------------------------------------------------------------------
// Phase 2: Instance operations and I/O
// ---------------------------------------------------------------------------

/// Create an I/O protocol registry with all built-in protocol codecs.
///
/// Returns a handle to the registry, which can be used with
/// [`parse_instance`] and [`emit_instance`].
///
/// # Errors
///
/// Returns `JsError` if registry creation fails.
#[must_use]
#[wasm_bindgen]
pub fn register_io_protocols() -> u32 {
    slab::alloc(Resource::IoRegistry(Box::new(io::default_registry())))
}

/// List all protocol names registered in an I/O registry.
///
/// Returns `MessagePack`-encoded `Vec<String>`.
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid.
#[wasm_bindgen]
pub fn list_io_protocols(registry: u32) -> Result<Vec<u8>, JsError> {
    let names: Vec<String> = slab::with_resource(registry, |r| {
        let reg = slab::as_io_registry(r)?;
        Ok(reg.protocol_names().map(str::to_owned).collect())
    })?;

    rmp_serde::to_vec(&names).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Parse raw format bytes into an instance using a protocol codec.
///
/// The `proto_name` is the protocol name (e.g., `b"atproto"`).
/// Returns `MessagePack`-encoded instance (W-type or Functor depending
/// on the protocol's native representation).
///
/// # Errors
///
/// Returns `JsError` if parsing fails, handles are invalid, or the
/// protocol is unknown.
#[wasm_bindgen]
pub fn parse_instance(
    registry: u32,
    proto_name: &[u8],
    schema_handle: u32,
    input: &[u8],
) -> Result<Vec<u8>, JsError> {
    let name = std::str::from_utf8(proto_name).map_err(|e| WasmError::DeserializationFailed {
        reason: format!("invalid protocol name: {e}"),
    })?;

    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let result = slab::with_resource(registry, |r| {
        let reg = slab::as_io_registry(r)?;
        let repr = reg.native_repr(name).map_err(|e| WasmError::ParseFailed {
            reason: e.to_string(),
        })?;

        match repr {
            io::NativeRepr::WType | io::NativeRepr::Either => {
                let instance =
                    reg.parse_wtype(name, &schema, input)
                        .map_err(|e| WasmError::ParseFailed {
                            reason: e.to_string(),
                        })?;
                rmp_serde::to_vec(&instance).map_err(|e| WasmError::SerializationFailed {
                    reason: e.to_string(),
                })
            }
            io::NativeRepr::Functor => {
                let instance = reg.parse_functor(name, &schema, input).map_err(|e| {
                    WasmError::ParseFailed {
                        reason: e.to_string(),
                    }
                })?;
                rmp_serde::to_vec(&instance).map_err(|e| WasmError::SerializationFailed {
                    reason: e.to_string(),
                })
            }
        }
    })?;

    Ok(result)
}

/// Emit an instance to raw format bytes using a protocol codec.
///
/// The `proto_name` is the protocol name. The `instance` is
/// `MessagePack`-encoded (W-type or Functor).
///
/// # Errors
///
/// Returns `JsError` if emission fails.
#[wasm_bindgen]
pub fn emit_instance(
    registry: u32,
    proto_name: &[u8],
    schema_handle: u32,
    instance_bytes: &[u8],
) -> Result<Vec<u8>, JsError> {
    let name = std::str::from_utf8(proto_name).map_err(|e| WasmError::DeserializationFailed {
        reason: format!("invalid protocol name: {e}"),
    })?;

    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let result = slab::with_resource(registry, |r| {
        let reg = slab::as_io_registry(r)?;
        let repr = reg.native_repr(name).map_err(|e| WasmError::EmitFailed {
            reason: e.to_string(),
        })?;

        match repr {
            io::NativeRepr::WType | io::NativeRepr::Either => {
                let instance: WInstance = rmp_serde::from_slice(instance_bytes).map_err(|e| {
                    WasmError::DeserializationFailed {
                        reason: e.to_string(),
                    }
                })?;
                reg.emit_wtype(name, &schema, &instance)
                    .map_err(|e| WasmError::EmitFailed {
                        reason: e.to_string(),
                    })
            }
            io::NativeRepr::Functor => {
                let instance: inst::FInstance =
                    rmp_serde::from_slice(instance_bytes).map_err(|e| {
                        WasmError::DeserializationFailed {
                            reason: e.to_string(),
                        }
                    })?;
                reg.emit_functor(name, &schema, &instance)
                    .map_err(|e| WasmError::EmitFailed {
                        reason: e.to_string(),
                    })
            }
        }
    })?;

    Ok(result)
}

/// Parse an instance with format preservation, returning both the instance
/// and a CST complement that can be used for format-preserving emission.
///
/// Returns `MessagePack`-encoded `(instance_bytes, complement_bytes)` tuple.
/// The complement_bytes may be empty if the codec doesn't support format
/// preservation.
///
/// Requires the `tree-sitter` feature on `panproto-io`.
///
/// # Errors
///
/// Returns `JsError` if parsing fails or handles are invalid.
#[cfg(feature = "format-preserving")]
#[wasm_bindgen]
pub fn parse_instance_preserving(
    registry: u32,
    proto_name: &[u8],
    schema_handle: u32,
    input: &[u8],
) -> Result<Vec<u8>, JsError> {
    let name = std::str::from_utf8(proto_name).map_err(|e| WasmError::DeserializationFailed {
        reason: format!("invalid protocol name: {e}"),
    })?;

    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let result = slab::with_resource(registry, |r| {
        let reg = slab::as_io_registry(r)?;
        let (instance, complement) =
            reg.parse_wtype_preserving(name, &schema, input)
                .map_err(|e| WasmError::ParseFailed {
                    reason: e.to_string(),
                })?;

        let instance_bytes =
            rmp_serde::to_vec(&instance).map_err(|e| WasmError::SerializationFailed {
                reason: e.to_string(),
            })?;
        let complement_bytes = complement
            .map(|c| rmp_serde::to_vec(&c))
            .transpose()
            .map_err(|e| WasmError::SerializationFailed {
                reason: e.to_string(),
            })?
            .unwrap_or_default();

        rmp_serde::to_vec(&(instance_bytes, complement_bytes)).map_err(|e| {
            WasmError::SerializationFailed {
                reason: e.to_string(),
            }
        })
    })?;

    Ok(result)
}

/// Emit an instance with format preservation using a CST complement.
///
/// The `complement_bytes` should be the CST complement from
/// `parse_instance_preserving`. If empty, falls back to canonical emission.
///
/// Requires the `tree-sitter` feature on `panproto-io`.
///
/// # Errors
///
/// Returns `JsError` if emission fails or handles are invalid.
#[cfg(feature = "format-preserving")]
#[wasm_bindgen]
pub fn emit_instance_preserving(
    registry: u32,
    proto_name: &[u8],
    schema_handle: u32,
    instance_bytes: &[u8],
    complement_bytes: &[u8],
) -> Result<Vec<u8>, JsError> {
    let name = std::str::from_utf8(proto_name).map_err(|e| WasmError::DeserializationFailed {
        reason: format!("invalid protocol name: {e}"),
    })?;

    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let complement: Option<io::cst_extract::CstComplement> = if complement_bytes.is_empty() {
        None
    } else {
        Some(rmp_serde::from_slice(complement_bytes).map_err(|e| {
            WasmError::DeserializationFailed {
                reason: e.to_string(),
            }
        })?)
    };

    let result = slab::with_resource(registry, |r| {
        let reg = slab::as_io_registry(r)?;
        reg.emit_wtype_preserving(name, &schema, &instance, complement.as_ref())
            .map_err(|e| WasmError::EmitFailed {
                reason: e.to_string(),
            })
    })?;

    Ok(result)
}

/// Validate a W-type instance against a schema.
///
/// Returns `MessagePack`-encoded `Vec<String>` of validation error
/// messages. An empty vector means the instance is valid.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or deserialization fails.
#[wasm_bindgen]
pub fn validate_instance(schema_handle: u32, instance_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let errors: Vec<String> = inst::validate_wtype(&schema, &instance)
        .into_iter()
        .map(|e| format!("{e:?}"))
        .collect();

    rmp_serde::to_vec(&errors).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Convert a W-type instance to JSON bytes.
///
/// The `instance_bytes` are `MessagePack`-encoded [`WInstance`].
/// Returns JSON bytes.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn instance_to_json(schema_handle: u32, instance_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let json_value = inst::to_json(&schema, &instance);
    serde_json::to_vec(&json_value).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Parse JSON bytes into a W-type instance.
///
/// Returns `MessagePack`-encoded [`WInstance`].
///
/// # Errors
///
/// Returns `JsError` if parsing fails.
#[wasm_bindgen]
pub fn json_to_instance(schema_handle: u32, json_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    json_to_instance_with_root(schema_handle, json_bytes, "")
}

/// Parse JSON bytes into a W-type instance with an explicit root vertex.
///
/// If `root_vertex` is empty, the root is inferred: first tries
/// `schema.protocol`, then looks for the first `object` or `record` vertex.
///
/// Returns `MessagePack`-encoded [`WInstance`].
///
/// # Errors
///
/// Returns `JsError` if parsing fails.
#[wasm_bindgen]
pub fn json_to_instance_with_root(
    schema_handle: u32,
    json_bytes: &[u8],
    root_vertex: &str,
) -> Result<Vec<u8>, JsError> {
    let schema = slab::with_resource(schema_handle, |r| Ok(slab::as_schema(r)?.clone()))?;

    let json_value: serde_json::Value =
        serde_json::from_slice(json_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    // Determine root vertex. Precedence:
    //   1. explicit caller-supplied vertex if it exists in the schema;
    //   2. `schema.protocol` (legacy convention — some builders use the
    //      protocol name as the top-level vertex id);
    //   3. the schema's declared primary entry (the pointed-schema
    //      basepoint).
    let root: String = if !root_vertex.is_empty() && schema.has_vertex(root_vertex) {
        root_vertex.to_string()
    } else if schema.has_vertex(&schema.protocol) {
        schema.protocol.clone()
    } else {
        schema::primary_entry(&schema)
            .map(ToString::to_string)
            .ok_or_else(|| WasmError::ParseFailed {
                reason: "no suitable root vertex found in schema".to_string(),
            })?
    };

    let instance =
        inst::parse_json(&schema, &root, &json_value).map_err(|e| WasmError::ParseFailed {
            reason: e.to_string(),
        })?;

    rmp_serde::to_vec(&instance).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Get the element count of an instance.
///
/// The `instance_bytes` are `MessagePack`-encoded [`WInstance`].
/// Returns the number of nodes.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn instance_element_count(instance_bytes: &[u8]) -> Result<u32, JsError> {
    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    #[allow(clippy::cast_possible_truncation)]
    Ok(instance.node_count() as u32)
}
