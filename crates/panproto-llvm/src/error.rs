//! Error types for LLVM IR operations.

use miette::Diagnostic;

/// Errors from LLVM IR protocol operations.
#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum LlvmError {
    /// LLVM IR parsing failed.
    #[error("LLVM IR parse failed: {reason}")]
    ParseFailed {
        /// Description of the parse failure.
        reason: String,
    },

    /// LLVM IR emission failed.
    #[error("LLVM IR emit failed: {reason}")]
    EmitFailed {
        /// Description of the emit failure.
        reason: String,
    },

    /// Theory morphism construction failed.
    #[error("lowering morphism failed: {reason}")]
    LoweringFailed {
        /// Description of the failure.
        reason: String,
    },

    /// Schema construction failed.
    #[error("schema error: {0}")]
    Schema(#[from] panproto_schema::SchemaError),

    /// The inkwell backend is required but not enabled.
    #[error("inkwell backend not enabled; rebuild with --features=inkwell-backend")]
    InkwellNotEnabled,
}
