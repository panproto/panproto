//! Protocol ingest, serialization, and inspection.

use panproto_core::schema::Protocol;
use safer_ffi::prelude::*;

#[cfg(any(test, feature = "panic-test"))]
use crate::error::FfiError;
use crate::error::PpStatus;
use crate::handle::{self, Resource};
use crate::panic::guard;

/// Ingest a CBOR-encoded [`Protocol`] specification and register it
/// in the slab.
///
/// On success, `out_handle` is set to a fresh handle and [`PpStatus::Ok`]
/// is returned. On CBOR decode failure, [`PpStatus::Serialization`] is
/// returned and `out_handle` is left untouched.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protocol_define(spec: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    guard(|| {
        let protocol: Protocol = crate::canonical::decode(spec.as_slice())?;
        *out_handle = handle::alloc(Resource::Protocol(protocol));
        Ok(PpStatus::Ok)
    })
}

/// Serialize the protocol referenced by `proto` to CBOR.
///
/// On success, `out` is populated with freshly allocated CBOR bytes;
/// the host must release them via `pp_buf_free`. Common failure modes
/// are [`PpStatus::InvalidHandle`] and (once additional resource
/// variants exist) [`PpStatus::TypeMismatch`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protocol_serialize(proto: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let bytes = handle::with_resource(proto, |r| crate::canonical::encode(r.as_protocol()))?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Test-only entry point that always panics; used to verify the
/// panic-safety guarantee end-to-end at the FFI boundary.
///
/// Available only under `cfg(any(test, feature = "panic-test"))` so
/// it never leaks into a release surface; production callers do not
/// see this symbol in `panproto.h`.
///
/// # Panics
///
/// Always panics by design. The panic is caught by [`crate::panic::guard`]
/// and converted to [`PpStatus::Panic`]; the host process never aborts.
#[cfg(any(test, feature = "panic-test"))]
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_internal_panic() -> i32 {
    guard(|| -> Result<PpStatus, FfiError> {
        panic!("panproto-c internal panic for test");
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use panproto_core::schema::Protocol;

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free, pp_last_error_take};
    use crate::canonical::{decode, encode};

    fn fixture() -> Protocol {
        Protocol {
            name: "round.trip".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "value".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    #[test]
    fn round_trip_through_ffi() {
        let bytes = encode(&fixture()).unwrap();
        let slice: c_slice::Box<u8> = bytes.into_boxed_slice().into();
        let mut handle: u32 = u32::MAX;
        let status = pp_protocol_define(slice.as_ref(), &mut handle);
        assert_eq!(status, PpStatus::Ok as i32);
        assert_ne!(handle, u32::MAX);

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_protocol_serialize(handle, &mut out), PpStatus::Ok as i32);

        let restored: Protocol = decode(&out).unwrap();
        assert_eq!(restored.name, "round.trip");
        assert_eq!(restored.obj_kinds, vec!["object", "value"]);

        pp_buf_free(out);
        assert_eq!(pp_handle_free(handle), PpStatus::Ok as i32);
    }

    #[test]
    fn invalid_handle_yields_status_and_envelope() {
        // Drain any prior state.
        let mut sink: repr_c::Vec<u8> = Vec::new().into();
        let _ = pp_last_error_take(&mut sink);
        pp_buf_free(sink);

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_protocol_serialize(u32::MAX - 1, &mut out);
        assert_eq!(status, PpStatus::InvalidHandle as i32);
        pp_buf_free(out);

        let mut env_buf: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_last_error_take(&mut env_buf), PpStatus::Ok as i32);
        let env: crate::error::ErrorEnvelope = decode(&env_buf).unwrap();
        assert_eq!(env.tag, "invalid_handle");
        pp_buf_free(env_buf);
    }

    #[test]
    fn malformed_cbor_yields_serialization_status() {
        let bad: Box<[u8]> = vec![0xFFu8, 0xFE, 0xFD].into_boxed_slice();
        let slice: c_slice::Box<u8> = bad.into();
        let mut handle: u32 = u32::MAX;
        let status = pp_protocol_define(slice.as_ref(), &mut handle);
        assert_eq!(status, PpStatus::Serialization as i32);
        assert_eq!(handle, u32::MAX);
    }

    #[test]
    fn internal_panic_is_caught() {
        let status = pp_internal_panic();
        assert_eq!(status, PpStatus::Panic as i32);
        let mut env_buf: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_last_error_take(&mut env_buf), PpStatus::Ok as i32);
        let env: crate::error::ErrorEnvelope = decode(&env_buf).unwrap();
        assert_eq!(env.tag, "panic");
        pp_buf_free(env_buf);
    }
}
