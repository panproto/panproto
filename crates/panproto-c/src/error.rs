//! Errors crossing the C ABI boundary.
//!
//! Errors are not exposed as Rust enums to the C side. They are stashed
//! in a thread-local "last error" slot and retrieved by the host via
//! [`crate::api::pp_last_error_take`], which serializes them as CBOR.
//!
//! The status codes returned by entry points are coarse-grained
//! ([`PpStatus`]); the host inspects the last-error envelope for
//! detail.

use std::cell::RefCell;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Coarse-grained status returned by every FFI entry point.
///
/// `0` is success; non-zero values indicate a failure category. The
/// host can call [`crate::api::pp_last_error_take`] to retrieve a
/// CBOR-encoded [`ErrorEnvelope`] with details.
#[allow(missing_docs)]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpStatus {
    Ok = 0,
    Err = 1,
    Panic = 2,
    InvalidHandle = 3,
    TypeMismatch = 4,
    Serialization = 5,
    Internal = 6,
    Operation = 7,
}

impl From<PpStatus> for i32 {
    fn from(value: PpStatus) -> Self {
        value as Self
    }
}

impl TryFrom<i32> for PpStatus {
    type Error = i32;

    /// Convert a wire-level status code back to a [`PpStatus`].
    ///
    /// Returns `Err(code)` for values outside the recognized range so
    /// the caller can preserve forward-compatibility (a future
    /// panproto-c may add a new status code, and consumers can choose
    /// how to react).
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Ok),
            1 => Ok(Self::Err),
            2 => Ok(Self::Panic),
            3 => Ok(Self::InvalidHandle),
            4 => Ok(Self::TypeMismatch),
            5 => Ok(Self::Serialization),
            6 => Ok(Self::Internal),
            7 => Ok(Self::Operation),
            other => Err(other),
        }
    }
}

/// Internal error type collected at every panproto-c entry point.
///
/// Variants mirror the failure categories of [`PpStatus`] plus carry
/// a human-readable detail string. This type never appears in the C
/// ABI; it is serialized to [`ErrorEnvelope`] for the host.
#[derive(Debug, Error)]
pub enum FfiError {
    /// A handle was invalid (out of bounds or freed).
    #[error("invalid handle: {handle}")]
    InvalidHandle {
        /// The invalid handle value.
        handle: u32,
    },

    /// A handle pointed to a resource of the wrong type.
    #[error("type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        /// The expected resource type.
        expected: &'static str,
        /// The actual resource type.
        actual: &'static str,
    },

    /// CBOR (de)serialization failed.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// A panic was caught at the FFI boundary.
    #[error("panic: {0}")]
    Panic(String),

    /// Any other internal error from panproto-core.
    #[error("internal error: {0}")]
    Internal(String),

    /// A domain operation failed (migration, lens, VCS, parse, and so on).
    ///
    /// This is the coarse catch-all that every engine-level failure
    /// funnels into. Unlike the WASM boundary, which carries a ~20-variant
    /// taxonomy, the C ABI keeps a single descriptive-message variant so
    /// the host has one error shape to decode; the detail string names the
    /// failing operation and underlying cause.
    #[error("operation error: {0}")]
    Operation(String),
}

impl FfiError {
    /// Map an [`FfiError`] to its corresponding [`PpStatus`] code.
    #[must_use]
    pub const fn status(&self) -> PpStatus {
        match self {
            Self::InvalidHandle { .. } => PpStatus::InvalidHandle,
            Self::TypeMismatch { .. } => PpStatus::TypeMismatch,
            Self::Serialization(_) => PpStatus::Serialization,
            Self::Panic(_) => PpStatus::Panic,
            Self::Internal(_) => PpStatus::Internal,
            Self::Operation(_) => PpStatus::Operation,
        }
    }
}

/// CBOR-serializable error envelope returned to the host.
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    /// Numeric status code matching [`PpStatus`].
    pub status: i32,
    /// Short tag identifying the variant (e.g. `"invalid_handle"`).
    pub tag: String,
    /// Human-readable detail.
    pub message: String,
}

impl From<&FfiError> for ErrorEnvelope {
    fn from(err: &FfiError) -> Self {
        let tag = match err {
            FfiError::InvalidHandle { .. } => "invalid_handle",
            FfiError::TypeMismatch { .. } => "type_mismatch",
            FfiError::Serialization(_) => "serialization",
            FfiError::Panic(_) => "panic",
            FfiError::Internal(_) => "internal",
            FfiError::Operation(_) => "operation",
        };
        Self {
            status: err.status() as i32,
            tag: tag.to_string(),
            message: err.to_string(),
        }
    }
}

thread_local! {
    static LAST_ERROR: RefCell<Option<ErrorEnvelope>> = const { RefCell::new(None) };
}

/// Stash the last error for retrieval by the host.
pub fn set_last_error(err: &FfiError) {
    LAST_ERROR.with_borrow_mut(|slot| *slot = Some(ErrorEnvelope::from(err)));
}

/// Take the last error, clearing the slot.
pub fn take_last_error() -> Option<ErrorEnvelope> {
    LAST_ERROR.with_borrow_mut(Option::take)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips_via_i32() {
        for status in [
            PpStatus::Ok,
            PpStatus::Err,
            PpStatus::Panic,
            PpStatus::InvalidHandle,
            PpStatus::TypeMismatch,
            PpStatus::Serialization,
            PpStatus::Internal,
            PpStatus::Operation,
        ] {
            let n: i32 = status.into();
            assert_eq!(n, status as i32, "status {status:?} mismatched i32");
            assert_eq!(
                PpStatus::try_from(n).unwrap(),
                status,
                "PpStatus::try_from({n}) did not return {status:?}"
            );
        }
    }

    #[test]
    fn status_try_from_unknown_returns_code() {
        match PpStatus::try_from(42) {
            Err(c) => assert_eq!(c, 42),
            Ok(s) => panic!("expected Err for unknown code, got {s:?}"),
        }
        // Negative codes are also unknown.
        assert!(matches!(PpStatus::try_from(-1), Err(-1)));
    }

    #[test]
    fn ffi_error_status_mapping() {
        assert_eq!(
            FfiError::InvalidHandle { handle: 7 }.status(),
            PpStatus::InvalidHandle
        );
        assert_eq!(
            FfiError::TypeMismatch {
                expected: "Protocol",
                actual: "Schema"
            }
            .status(),
            PpStatus::TypeMismatch
        );
        assert_eq!(
            FfiError::Serialization("oops".into()).status(),
            PpStatus::Serialization
        );
        assert_eq!(FfiError::Panic("boom".into()).status(), PpStatus::Panic);
        assert_eq!(
            FfiError::Internal("kaput".into()).status(),
            PpStatus::Internal
        );
    }

    #[test]
    fn envelope_carries_message_and_tag() {
        let err = FfiError::InvalidHandle { handle: 42 };
        let env = ErrorEnvelope::from(&err);
        assert_eq!(env.tag, "invalid_handle");
        assert!(env.message.contains("42"));
        assert_eq!(env.status, PpStatus::InvalidHandle as i32);
    }

    #[test]
    fn last_error_set_take_clears_slot() {
        let _ = take_last_error(); // drain whatever may be left
        assert!(take_last_error().is_none());

        set_last_error(&FfiError::Internal("under test".into()));
        let env = take_last_error().expect("envelope present");
        assert_eq!(env.tag, "internal");

        // After take, slot is empty.
        assert!(take_last_error().is_none());
    }

    #[test]
    fn type_mismatch_envelope_includes_both_kinds() {
        let err = FfiError::TypeMismatch {
            expected: "Protocol",
            actual: "Schema",
        };
        let env = ErrorEnvelope::from(&err);
        assert_eq!(env.tag, "type_mismatch");
        assert!(env.message.contains("Protocol"));
        assert!(env.message.contains("Schema"));
    }

    #[test]
    fn each_variant_has_distinct_tag() {
        let envs = [
            ErrorEnvelope::from(&FfiError::InvalidHandle { handle: 0 }),
            ErrorEnvelope::from(&FfiError::TypeMismatch {
                expected: "a",
                actual: "b",
            }),
            ErrorEnvelope::from(&FfiError::Serialization("s".into())),
            ErrorEnvelope::from(&FfiError::Panic("p".into())),
            ErrorEnvelope::from(&FfiError::Internal("i".into())),
            ErrorEnvelope::from(&FfiError::Operation("o".into())),
        ];
        let tags: Vec<&str> = envs.iter().map(|e| e.tag.as_str()).collect();
        let unique: std::collections::HashSet<&str> = tags.iter().copied().collect();
        assert_eq!(unique.len(), tags.len(), "duplicate tag in {tags:?}");
    }
}
