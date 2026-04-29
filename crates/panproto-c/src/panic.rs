//! Panic-safe entry-point wrapping.
//!
//! Every `#[ffi_export]` function in [`crate::api`] runs its body
//! through [`guard`], which catches unwinding panics, stashes the
//! panic message via [`crate::error::set_last_error`], and returns
//! the appropriate [`PpStatus`] code. The release profile is
//! `panic = "unwind"` so `catch_unwind` actually has a stack to
//! catch; an `abort` profile would instead tear down the host
//! process.
//!
//! This is non-negotiable: `extern "C"` + Rust panic is undefined
//! behavior, and GHC has no agreed mechanism for catching foreign
//! unwinds (cf. open ghc-proposals discussions on foreign exceptions).

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::error::{FfiError, PpStatus, set_last_error};

/// Run a fallible closure inside `catch_unwind`, mapping all failure
/// modes to a [`PpStatus`].
///
/// The closure may return `Result<PpStatus, FfiError>`. On `Ok`, the
/// returned status is propagated unchanged (so a successful op returns
/// `PpStatus::Ok`, an op that *intentionally* signals a known status
/// can return that). On `Err`, the error is stashed via
/// [`set_last_error`] and its [`FfiError::status`] is returned. On
/// panic, a [`FfiError::Panic`] envelope is stashed and
/// [`PpStatus::Panic`] is returned.
pub fn guard<F>(f: F) -> i32
where
    F: FnOnce() -> Result<PpStatus, FfiError>,
{
    let result = catch_unwind(AssertUnwindSafe(f));
    match result {
        Ok(Ok(status)) => status as i32,
        Ok(Err(err)) => {
            let status = err.status();
            set_last_error(&err);
            status as i32
        }
        Err(panic) => {
            let message = panic_message(&panic);
            let err = FfiError::Panic(message);
            set_last_error(&err);
            PpStatus::Panic as i32
        }
    }
}

fn panic_message(panic: &Box<dyn std::any::Any + Send + 'static>) -> String {
    panic
        .downcast_ref::<&'static str>()
        .map(|s| (*s).to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic payload".to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::error::take_last_error;

    #[test]
    fn ok_path_returns_ok() {
        let status = guard(|| Ok(PpStatus::Ok));
        assert_eq!(status, PpStatus::Ok as i32);
        assert!(take_last_error().is_none());
    }

    #[test]
    fn err_path_returns_status_and_stashes() {
        let status = guard(|| Err(FfiError::InvalidHandle { handle: 42 }));
        assert_eq!(status, PpStatus::InvalidHandle as i32);
        let env = take_last_error().expect("error stashed");
        assert_eq!(env.tag, "invalid_handle");
        assert!(env.message.contains("42"));
    }

    #[test]
    fn panic_with_str_is_caught() {
        let status = guard(|| -> Result<PpStatus, FfiError> { panic!("boom") });
        assert_eq!(status, PpStatus::Panic as i32);
        let env = take_last_error().expect("panic stashed");
        assert_eq!(env.tag, "panic");
        assert!(env.message.contains("boom"));
    }

    #[test]
    fn panic_with_string_is_caught() {
        let status = guard(|| -> Result<PpStatus, FfiError> {
            panic!("formatted {}", 7);
        });
        assert_eq!(status, PpStatus::Panic as i32);
        let env = take_last_error().expect("panic stashed");
        assert!(env.message.contains("formatted 7"));
    }
}
