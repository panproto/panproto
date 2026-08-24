//! Integration test asserting that `pp_init`'s panic hook is scoped to
//! panproto's own boundary.
//!
//! `pp_init` silences the default stderr report for panics that
//! [`panproto_c::panic::guard`] is about to convert into a status code
//! and an error envelope. A panic anywhere else in the host process is
//! none of panproto's business: the host's own hook — its crash
//! reporter, its logger, or the Rust default — must still see it.
//!
//! The two cases share one process, so they run as one test: the hook is
//! process-global, and a second test could otherwise observe the other's
//! installation.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};

use panproto_c::api::pp_init;
use panproto_c::error::{FfiError, PpStatus, take_last_error};
use panproto_c::panic::guard;

/// How many times the host's own hook has run.
static HOST_HOOK_CALLS: AtomicUsize = AtomicUsize::new(0);

#[test]
fn the_panic_hook_covers_the_ffi_boundary_and_nothing_else() {
    std::panic::set_hook(Box::new(|_| {
        HOST_HOOK_CALLS.fetch_add(1, Ordering::SeqCst);
    }));
    assert_eq!(pp_init(), PpStatus::Ok as i32);

    // A panic that never crosses the FFI boundary belongs to the host.
    let outside = catch_unwind(|| panic!("a panic in the host's own code"));
    assert!(outside.is_err(), "the host panic unwound as expected");
    assert_eq!(
        HOST_HOOK_CALLS.load(Ordering::SeqCst),
        1,
        "pp_init silenced a panic that never crossed the FFI boundary"
    );

    // A panic inside the boundary is reported through the status code
    // and the error envelope, so the host hook must not also fire.
    let status = guard(|| -> Result<PpStatus, FfiError> { panic!("boom at the boundary") });
    assert_eq!(status, PpStatus::Panic as i32);
    let envelope = take_last_error().expect("the boundary panic is stashed");
    assert_eq!(envelope.tag, "panic");
    assert!(envelope.message.contains("boom at the boundary"));
    assert_eq!(
        HOST_HOOK_CALLS.load(Ordering::SeqCst),
        1,
        "a panic the boundary already reported was also sent to the host hook"
    );

    // Leaving the boundary restores the host's claim on its own panics.
    let after = catch_unwind(AssertUnwindSafe(|| panic!("another host panic")));
    assert!(after.is_err());
    assert_eq!(
        HOST_HOOK_CALLS.load(Ordering::SeqCst),
        2,
        "the boundary kept the hook suppressed after returning"
    );
}
