//! WASM error conversion to `JsError`.
//!
//! All errors that can occur within the WASM boundary are collected
//! into [`WasmError`], which converts cleanly to `JsError` with
//! human-readable messages.

use std::fmt;

use wasm_bindgen::JsError;

/// Errors that can occur within WASM entry points.
///
/// These are internal errors that get converted to `JsError` at the
/// WASM boundary. We intentionally avoid implementing `std::error::Error`
/// to sidestep the blanket `From<E: Error> for JsError` conflict in
/// `wasm-bindgen`.
#[derive(Debug)]
pub enum WasmError {
    /// A handle was invalid (out of bounds or freed).
    InvalidHandle {
        /// The invalid handle value.
        handle: u32,
    },

    /// A handle pointed to a resource of the wrong type.
    TypeMismatch {
        /// The expected resource type.
        expected: &'static str,
        /// The actual resource type.
        actual: &'static str,
    },

    /// `MessagePack` deserialization failed.
    DeserializationFailed {
        /// Description of the deserialization error.
        reason: String,
    },

    /// `MessagePack` serialization failed.
    SerializationFailed {
        /// Description of the serialization error.
        reason: String,
    },

    /// A migration operation failed.
    MigrationFailed {
        /// Description of the migration error.
        reason: String,
    },

    /// A schema building operation failed.
    SchemaBuildFailed {
        /// Description of the build error.
        reason: String,
    },

    /// A lift (record migration) operation failed.
    LiftFailed {
        /// Description of the lift error.
        reason: String,
    },

    /// A lens put operation failed.
    PutFailed {
        /// Description of the put error.
        reason: String,
    },

    /// Migration composition failed.
    ComposeFailed {
        /// Description of the compose error.
        reason: String,
    },
}

impl fmt::Display for WasmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHandle { handle } => write!(f, "invalid handle: {handle}"),
            Self::TypeMismatch { expected, actual } => {
                write!(f, "type mismatch: expected {expected}, got {actual}")
            }
            Self::DeserializationFailed { reason } => {
                write!(f, "deserialization failed: {reason}")
            }
            Self::SerializationFailed { reason } => {
                write!(f, "serialization failed: {reason}")
            }
            Self::MigrationFailed { reason } => write!(f, "migration error: {reason}"),
            Self::SchemaBuildFailed { reason } => write!(f, "schema build error: {reason}"),
            Self::LiftFailed { reason } => write!(f, "lift error: {reason}"),
            Self::PutFailed { reason } => write!(f, "put error: {reason}"),
            Self::ComposeFailed { reason } => write!(f, "compose error: {reason}"),
        }
    }
}

impl From<WasmError> for JsError {
    fn from(err: WasmError) -> Self {
        Self::new(&err.to_string())
    }
}
