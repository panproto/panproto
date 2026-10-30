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
        let expected: Vec<String> = panproto_core::protocols::registry::protocol_names()
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(
            names, expected,
            "the WASM listing diverged from the registry"
        );
        let first = &names[0];
        let proto_bytes = get_builtin_protocol(first.as_bytes()).unwrap();
        assert!(
            !proto_bytes.is_empty(),
            "builtin protocol should decode to bytes"
        );
    }

    /// Every spelling the registry accepts, the canonical name, each alias
    /// and the underscore form of each, reaches the registry's protocol.
    #[test]
    fn every_registry_spelling_reaches_the_registry_protocol() {
        let mut wrong: Vec<String> = Vec::new();
        for descriptor in panproto_core::protocols::registry::descriptors() {
            let expected = (descriptor.protocol)();
            for spelling in
                std::iter::once(descriptor.name).chain(descriptor.aliases.iter().copied())
            {
                for name in [spelling.to_owned(), spelling.replace('-', "_")] {
                    // The lookup behind `get_builtin_protocol`, called
                    // directly: a refusal builds a `JsError`, which needs a
                    // wasm host.
                    if lookup_builtin_protocol(&name).as_ref() != Some(&expected) {
                        wrong.push(name);
                    }
                }
            }
        }
        assert!(
            wrong.is_empty(),
            "spellings not resolved as the registry does: {wrong:?}"
        );
    }
}
