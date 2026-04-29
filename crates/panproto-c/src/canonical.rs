//! CBOR (de)serialization for cold-path FFI payloads.
//!
//! The vertical slice serializes [`panproto_core::schema::Protocol`]
//! directly via its existing `serde` derive. As more types are
//! exposed (`Schema`, `Instance`, `Migration`, …), each is either
//! pass-through (already `serde`) or gets a `Canonical*` shadow type
//! defined here that mirrors the public Rust shape.
//!
//! All CBOR encoding/decoding goes through this module so the
//! serialization policy (canonical mode, integer encoding, etc.) lives
//! in one place.

use serde::{Serialize, de::DeserializeOwned};

use crate::error::FfiError;

/// Decode a value of type `T` from a CBOR byte slice.
///
/// # Errors
///
/// Returns [`FfiError::Serialization`] if `bytes` is not valid CBOR
/// for `T`.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, FfiError> {
    ciborium::de::from_reader(bytes).map_err(|e| FfiError::Serialization(e.to_string()))
}

/// Encode a value of type `T` to a CBOR byte vector.
///
/// # Errors
///
/// Returns [`FfiError::Serialization`] if `value` cannot be encoded
/// (e.g. due to a non-string map key).
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, FfiError> {
    let mut buf = Vec::new();
    ciborium::ser::into_writer(value, &mut buf)
        .map_err(|e| FfiError::Serialization(e.to_string()))?;
    Ok(buf)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use panproto_core::schema::Protocol;

    use super::*;

    fn fixture() -> Protocol {
        Protocol {
            name: "test.proto".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    #[test]
    fn protocol_round_trip() {
        let original = fixture();
        let bytes = encode(&original).unwrap();
        let restored: Protocol = decode(&bytes).unwrap();
        assert_eq!(original.name, restored.name);
        assert_eq!(original.schema_theory, restored.schema_theory);
        assert_eq!(original.instance_theory, restored.instance_theory);
        assert_eq!(original.obj_kinds, restored.obj_kinds);
    }

    #[test]
    fn decode_garbage_returns_serialization_error() {
        let bad: &[u8] = &[0xFF, 0xFE, 0xFD, 0x00];
        let result: Result<Protocol, _> = decode(bad);
        assert!(matches!(result, Err(FfiError::Serialization(_))));
    }
}
