//! C ABI entry points.
//!
//! Every function in this module is `#[ffi_export]`-annotated and runs
//! its body inside [`crate::panic::guard`] so that panics, internal
//! errors, and serialization failures all surface as
//! [`crate::error::PpStatus`] codes rather than aborting the host.
//!
//! The vertical slice exposes a minimal protocol-handle round trip:
//!
//! - [`pp_init`]: explicit initialization hook (no-op today; reserved
//!   for panic-hook installation).
//! - [`pp_handle_free`]: release a handle.
//! - [`pp_protocol_define`]: ingest a CBOR-encoded `Protocol` and
//!   return a handle.
//! - [`pp_protocol_serialize`]: emit the CBOR bytes of a protocol
//!   handle.
//! - [`pp_last_error_take`]: drain the most recent error envelope as
//!   CBOR bytes.
//! - [`pp_buf_free`]: free a `Vec<u8>` returned by panproto-c.

pub mod protocol;

pub use protocol::*;

use safer_ffi::prelude::*;

use crate::error::{PpStatus, take_last_error};
use crate::handle;
use crate::panic::guard;

/// Initialize the panproto-c runtime.
///
/// Currently a no-op: the slab and last-error slots are thread-local
/// `RefCell`s that initialize lazily. Reserved for future panic-hook
/// installation or registry warm-up. Always returns [`PpStatus::Ok`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_init() -> i32 {
    guard(|| Ok(PpStatus::Ok))
}

/// Free a handle, marking its slab slot reusable.
///
/// Double-free is safe: a freed slot stays freed.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_handle_free(handle: u32) -> i32 {
    guard(|| {
        handle::free(handle);
        Ok(PpStatus::Ok)
    })
}

/// Take the last error envelope as a CBOR byte vector.
///
/// On `PpStatus::Ok`, `out` is populated with the freshly allocated
/// bytes; the host must release them via [`pp_buf_free`]. If no error
/// is pending, `out` receives an empty buffer and the status is
/// [`PpStatus::Ok`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_last_error_take(out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let bytes = match take_last_error() {
            Some(envelope) => crate::canonical::encode(&envelope)?,
            None => Vec::new(),
        };
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Free a byte buffer returned by panproto-c.
///
/// The host must call this on every `repr_c::Vec<u8>` it receives, or
/// memory leaks. Calling twice is undefined; do not double-free.
#[ffi_export]
pub fn pp_buf_free(buf: repr_c::Vec<u8>) {
    drop(buf);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use crate::error::{ErrorEnvelope, FfiError};

    use super::*;

    #[test]
    fn pp_init_returns_ok() {
        assert_eq!(pp_init(), PpStatus::Ok as i32);
    }

    #[test]
    fn handle_free_is_idempotent() {
        assert_eq!(pp_handle_free(0), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(0), PpStatus::Ok as i32);
    }

    #[test]
    fn last_error_returns_empty_when_none() {
        let _ = take_last_error();
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_last_error_take(&mut out), PpStatus::Ok as i32);
        assert!(out.is_empty());
    }

    #[test]
    fn last_error_round_trips_envelope() {
        crate::error::set_last_error(&FfiError::InvalidHandle { handle: 99 });
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_last_error_take(&mut out), PpStatus::Ok as i32);
        let env: ErrorEnvelope = crate::canonical::decode(&out).unwrap();
        assert_eq!(env.tag, "invalid_handle");
        assert!(env.message.contains("99"));
    }
}
