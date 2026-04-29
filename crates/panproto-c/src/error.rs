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
}

impl From<PpStatus> for i32 {
    fn from(value: PpStatus) -> Self {
        value as Self
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
