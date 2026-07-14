//! Built-in protocol registry access.
//!
//! Split out of the monolithic api.rs into a domain module.

use wasm_bindgen::prelude::*;

use crate::error::WasmError;

use super::helpers::{builtin_protocol_names, lookup_builtin_protocol};

// ---------------------------------------------------------------------------
// Phase 4: Full protocol registry
// ---------------------------------------------------------------------------

/// List all built-in protocol names.
///
/// Returns `MessagePack`-encoded `Vec<String>`.
#[must_use]
#[wasm_bindgen]
pub fn list_builtin_protocols() -> Vec<u8> {
    let names = builtin_protocol_names();
    rmp_serde::to_vec_named(&names).unwrap_or_default()
}

/// Get a built-in protocol specification by name.
///
/// Returns `MessagePack`-encoded `Protocol` spec.
///
/// # Errors
///
/// Returns `JsError` if the protocol name is unknown.
#[wasm_bindgen]
pub fn get_builtin_protocol(name: &[u8]) -> Result<Vec<u8>, JsError> {
    let name_str = std::str::from_utf8(name).map_err(|e| WasmError::DeserializationFailed {
        reason: format!("invalid protocol name: {e}"),
    })?;

    let protocol =
        lookup_builtin_protocol(name_str).ok_or_else(|| WasmError::DeserializationFailed {
            reason: format!("unknown protocol: {name_str}"),
        })?;

    rmp_serde::to_vec_named(&protocol).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn list_and_fetch_builtin_protocol() {
        let list_bytes = list_builtin_protocols();
        let names: Vec<String> = rmp_serde::from_slice(&list_bytes).unwrap();
        assert!(
            !names.is_empty(),
            "there must be at least one builtin protocol"
        );
        let first = &names[0];
        let proto_bytes = get_builtin_protocol(first.as_bytes()).unwrap();
        assert!(
            !proto_bytes.is_empty(),
            "builtin protocol should decode to bytes"
        );
    }
}
