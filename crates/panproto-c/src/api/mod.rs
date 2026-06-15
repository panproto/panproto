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

pub mod check;
pub mod data;
pub mod enriched;
pub mod expr;
pub mod gat;
pub mod graph;
pub mod helpers;
pub mod hom;
pub mod instance;
pub mod lens;
pub mod mig;
pub mod protocol;
pub mod registry;
pub mod schema;
pub mod vcs;

#[cfg(feature = "git")]
pub mod git;
#[cfg(feature = "full-parse")]
pub mod parse;
#[cfg(feature = "project")]
pub mod project;

pub use check::*;
pub use data::*;
pub use enriched::*;
pub use expr::*;
pub use gat::*;
pub use graph::*;
pub use hom::*;
pub use instance::*;
pub use lens::*;
pub use mig::*;
pub use protocol::*;
pub use registry::*;
pub use schema::*;
pub use vcs::*;

#[cfg(feature = "git")]
pub use git::*;
#[cfg(feature = "full-parse")]
pub use parse::*;
#[cfg(feature = "project")]
pub use project::*;

use safer_ffi::prelude::*;

use crate::error::{PpStatus, take_last_error};
use crate::handle;
use crate::panic::guard;

/// Initialize the panproto-c runtime.
///
/// Installs a process-global Rust panic hook that suppresses the
/// default stderr output. Panics are still observable: every entry
/// point in this module catches them via [`crate::panic::guard`] and
/// stashes the message in the thread-local last-error slot, which
/// the host retrieves via [`pp_last_error_take`]. Without this hook
/// the default Rust handler would print every caught panic to
/// stderr before `guard` could report it, which is noisy and
/// surprising for hosts that already report errors through the
/// status-code channel.
///
/// Idempotent: calling more than once just re-installs the same
/// hook. Always returns [`PpStatus::Ok`]. The slab and last-error
/// slots are thread-local `RefCell`s that initialize lazily, so they
/// do not need explicit setup.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_init() -> i32 {
    guard(|| {
        std::panic::set_hook(Box::new(|_info| {
            // Intentionally silent: the panic payload is captured
            // by `crate::panic::guard` and surfaced through
            // `pp_last_error_take`.
        }));
        Ok(PpStatus::Ok)
    })
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
/// memory leaks. Calling twice on the same buffer is undefined
/// behavior; do not double-free. The Haskell binding's
/// `pp_buf_free_at` glue zeroes the storage in place so a stale
/// `Vec_uint8_t` record cannot be passed back; non-Haskell callers
/// should follow the same discipline.
///
/// The drop is wrapped in [`std::panic::catch_unwind`] so that a stray
/// panic in the deallocator (or in a `Vec` whose contents the host
/// corrupted) cannot unwind across the FFI boundary, which is
/// undefined behavior. A caught panic is silently suppressed; the
/// host has no return channel here. This is the same panic-safety
/// posture as every other entry point in this module.
#[ffi_export]
pub fn pp_buf_free(buf: repr_c::Vec<u8>) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || drop(buf)));
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
