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

use panproto_core::inst::identity::{
    AnyComplement, ComplementShape, Encoding, IdentityError, decode_complement as decode_any,
    decode_complements as decode_any_seq,
};
use panproto_core::lens::Complement;
use serde::{Serialize, de::DeserializeOwned};

use crate::error::FfiError;

/// An identity failure met while decoding a complement: bytes that are
/// not a complement at all are a serialization error, and a complement
/// of another format or envelope layout is an operation error naming it.
fn complement_error(err: IdentityError) -> FfiError {
    match err {
        IdentityError::Undecodable(detail) => FfiError::Serialization(detail),
        other => FfiError::Operation(format!("complement: {other}")),
    }
}

fn into_tree(decoded: AnyComplement) -> Complement {
    let AnyComplement::WType(complement) = decoded;
    *complement
}

/// Decode one tree complement from CBOR, with the shape, format and
/// trailing-byte checks every complement import makes.
///
/// # Errors
///
/// Returns [`FfiError::Serialization`] if `bytes` is not a CBOR
/// complement, and [`FfiError::Operation`] if it is one of another
/// format or envelope layout.
pub fn decode_complement(bytes: &[u8]) -> Result<Complement, FfiError> {
    decode_any(bytes, Encoding::Cbor, ComplementShape::WType)
        .map(into_tree)
        .map_err(complement_error)
}

/// Decode a CBOR list of tree complements, one per record, with the
/// checks [`decode_complement`] makes applied to every element.
///
/// # Errors
///
/// As [`decode_complement`].
pub fn decode_complements(bytes: &[u8]) -> Result<Vec<Complement>, FfiError> {
    decode_any_seq(bytes, Encoding::Cbor, ComplementShape::WType)
        .map(|decoded| decoded.into_iter().map(into_tree).collect())
        .map_err(complement_error)
}

/// Decode a value of type `T` from a CBOR byte slice.
///
/// Trailing bytes after the encoded value are rejected: a
/// well-formed FFI payload contains exactly one CBOR item. The
/// Haskell side enforces the same constraint, so the two decoders
/// agree on what is well-formed.
///
/// # Errors
///
/// Returns [`FfiError::Serialization`] if `bytes` is not valid CBOR
/// for `T`, or if there are extra bytes after the encoded value.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, FfiError> {
    let mut cursor = std::io::Cursor::new(bytes);
    let value: T = ciborium::de::from_reader(&mut cursor)
        .map_err(|e| FfiError::Serialization(e.to_string()))?;
    let consumed = usize::try_from(cursor.position()).unwrap_or(bytes.len());
    if consumed < bytes.len() {
        let trailing = bytes.len() - consumed;
        return Err(FfiError::Serialization(format!(
            "{trailing} trailing byte(s) after CBOR-encoded value"
        )));
    }
    Ok(value)
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

    #[test]
    fn decode_rejects_trailing_bytes() {
        let original = fixture();
        let mut bytes = encode(&original).unwrap();
        bytes.extend_from_slice(&[0xAA, 0xBB]); // append 2 stray bytes
        let result: Result<Protocol, _> = decode(&bytes);
        match result {
            Err(FfiError::Serialization(msg)) => {
                assert!(msg.contains("trailing"), "msg = {msg:?}");
                assert!(msg.contains('2'), "msg = {msg:?}");
            }
            other => panic!("expected Serialization error, got {other:?}"),
        }
    }

    #[test]
    fn decode_accepts_exact_bytes() {
        let original = fixture();
        let bytes = encode(&original).unwrap();
        let restored: Protocol = decode(&bytes).unwrap();
        assert_eq!(original.name, restored.name);
    }
}
