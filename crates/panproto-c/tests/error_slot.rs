//! Integration test asserting that error attribution survives a thread
//! migration between the failing call and the drain.
//!
//! Host runtimes do not promise that the drain lands on the thread that
//! failed. GHC's threaded RTS migrates a Haskell thread across OS
//! threads at every `safe` foreign call, so `checkStatus`'s
//! `pp_last_error_take` can run on a different OS thread than the entry
//! point whose non-zero status triggered it. The last-error slot is
//! therefore process-global, exactly like the handle slab.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_c::api::{pp_buf_free, pp_last_error_take, pp_protocol_serialize};
use panproto_c::canonical::decode;
use panproto_c::error::{ErrorEnvelope, PpStatus};
use safer_ffi::prelude::*;

/// Drain the last-error slot, returning the decoded envelope.
fn drain() -> Option<ErrorEnvelope> {
    let mut buf: repr_c::Vec<u8> = Vec::new().into();
    assert_eq!(pp_last_error_take(&mut buf), PpStatus::Ok as i32);
    let envelope = if buf.is_empty() {
        None
    } else {
        Some(decode::<ErrorEnvelope>(&buf).expect("envelope decodes"))
    };
    pp_buf_free(buf);
    envelope
}

#[test]
fn an_error_raised_on_one_thread_is_drained_on_another() {
    // Fail on a worker thread: handle 9999 was never allocated.
    let status = std::thread::spawn(|| {
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_protocol_serialize(9999, &mut out);
        pp_buf_free(out);
        status
    })
    .join()
    .expect("worker thread did not panic");
    assert_eq!(status, PpStatus::InvalidHandle as i32);

    // Drain from the main thread, standing in for the host runtime
    // having migrated the logical thread between call and drain.
    let envelope = drain().expect("envelope survives the thread migration");
    assert_eq!(envelope.tag, "invalid_handle");
    assert!(
        envelope.message.contains("9999"),
        "envelope lost the failing handle: {}",
        envelope.message
    );
}

#[test]
fn draining_twice_yields_the_error_once() {
    let mut out: repr_c::Vec<u8> = Vec::new().into();
    assert_eq!(
        pp_protocol_serialize(4242, &mut out),
        PpStatus::InvalidHandle as i32
    );
    pp_buf_free(out);

    let first = drain().expect("first drain sees the envelope");
    assert_eq!(first.tag, "invalid_handle");
    assert!(drain().is_none(), "the slot must be empty after a drain");
}
