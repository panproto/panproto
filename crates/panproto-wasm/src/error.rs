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

    /// Schema normalization failed.
    NormalizeFailed {
        /// Description of the normalization error.
        reason: String,
    },

    /// Schema validation failed.
    ValidationFailed {
        /// Description of the validation error.
        reason: String,
    },

    /// Diff classification failed.
    ClassifyFailed {
        /// Description of the classification error.
        reason: String,
    },

    /// Instance parsing failed.
    ParseFailed {
        /// Description of the parse error.
        reason: String,
    },

    /// Instance emission failed.
    EmitFailed {
        /// Description of the emit error.
        reason: String,
    },

    /// I/O registry error.
    IoRegistryError {
        /// Description of the registry error.
        reason: String,
    },

    /// Lens construction failed.
    LensConstructionFailed {
        /// Description of the lens construction error.
        reason: String,
    },

    /// Lens law check failed.
    LawCheckFailed {
        /// Description of the law check failure.
        reason: String,
    },

    /// Migration inversion failed.
    InvertFailed {
        /// Description of the inversion error.
        reason: String,
    },

    /// GAT theory operation failed.
    TheoryError {
        /// Description of the theory error.
        reason: String,
    },

    /// GAT colimit computation failed.
    ColimitFailed {
        /// Description of the colimit error.
        reason: String,
    },

    /// GAT morphism check failed.
    MorphismCheckFailed {
        /// Description of the morphism check error.
        reason: String,
    },

    /// VCS operation failed.
    VcsError {
        /// Description of the VCS error.
        reason: String,
    },

    /// Expression evaluation failed.
    ExprEvalFailed {
        /// Description of the evaluation error.
        reason: String,
    },

    /// Expression type-check failed.
    ExprCheckFailed {
        /// Description of the type-check error.
        reason: String,
    },

    /// Schema enrichment operation failed.
    EnrichmentFailed {
        /// Description of the enrichment error.
        reason: String,
    },

    /// Coverage analysis failed.
    CoverageFailed {
        /// Description of the coverage error.
        reason: String,
    },

    /// Protolens classification failed.
    ClassificationFailed {
        /// Description of the classification error.
        reason: String,
    },

    /// Refinement subsort check failed.
    RefinementFailed {
        /// Description of the refinement error.
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
            Self::NormalizeFailed { reason } => write!(f, "normalize error: {reason}"),
            Self::ValidationFailed { reason } => write!(f, "validation error: {reason}"),
            Self::ClassifyFailed { reason } => write!(f, "classify error: {reason}"),
            Self::ParseFailed { reason } => write!(f, "parse error: {reason}"),
            Self::EmitFailed { reason } => write!(f, "emit error: {reason}"),
            Self::IoRegistryError { reason } => write!(f, "io registry error: {reason}"),
            Self::LensConstructionFailed { reason } => {
                write!(f, "lens construction error: {reason}")
            }
            Self::LawCheckFailed { reason } => write!(f, "law check error: {reason}"),
            Self::InvertFailed { reason } => write!(f, "invert error: {reason}"),
            Self::TheoryError { reason } => write!(f, "theory error: {reason}"),
            Self::ColimitFailed { reason } => write!(f, "colimit error: {reason}"),
            Self::MorphismCheckFailed { reason } => {
                write!(f, "morphism check error: {reason}")
            }
            Self::VcsError { reason } => write!(f, "vcs error: {reason}"),
            Self::ExprEvalFailed { reason } => write!(f, "expression eval error: {reason}"),
            Self::ExprCheckFailed { reason } => write!(f, "expression check error: {reason}"),
            Self::EnrichmentFailed { reason } => write!(f, "enrichment error: {reason}"),
            Self::CoverageFailed { reason } => write!(f, "coverage error: {reason}"),
            Self::ClassificationFailed { reason } => write!(f, "classification error: {reason}"),
            Self::RefinementFailed { reason } => write!(f, "refinement error: {reason}"),
        }
    }
}

impl From<WasmError> for JsError {
    fn from(err: WasmError) -> Self {
        Self::new(&err.to_string())
    }
}
