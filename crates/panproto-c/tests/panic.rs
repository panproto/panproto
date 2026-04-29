//! Integration test asserting the panic-safety contract.
//!
//! `panproto_c::api::pp_internal_panic` deliberately panics inside
//! the FFI guard. The contract is that:
//!
//! 1. The host process is never aborted.
//! 2. The status code is `PpStatus::Panic` (= 2).
//! 3. The last-error envelope's `tag` is `"panic"`.
//!
//! This test runs the public API surface, not the inner `panic::guard`
//! module, so it reflects what a real C/Haskell consumer would observe.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_c::api::{pp_buf_free, pp_internal_panic, pp_last_error_take};
use panproto_c::canonical::decode;
use panproto_c::error::{ErrorEnvelope, PpStatus};
use safer_ffi::prelude::*;

#[test]
fn panic_at_ffi_boundary_is_caught_and_reported() {
    let status = pp_internal_panic();
    assert_eq!(status, PpStatus::Panic as i32);

    let mut buf: repr_c::Vec<u8> = Vec::new().into();
    assert_eq!(pp_last_error_take(&mut buf), PpStatus::Ok as i32);
    let env: ErrorEnvelope = decode(&buf).expect("envelope decodes");
    assert_eq!(env.tag, "panic");
    assert!(env.message.contains("internal panic for test"));
    pp_buf_free(buf);
}
