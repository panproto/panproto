//! Error types for JIT compilation.

use miette::Diagnostic;

/// Errors from JIT compilation of panproto expressions.
#[non_exhaustive]
#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum JitError {
    /// The expression contains an unsupported construct.
    #[error("unsupported expression for JIT: {reason}")]
    Unsupported {
        /// What is unsupported and why.
        reason: String,
    },

    /// LLVM IR generation failed.
    #[error("LLVM codegen failed: {reason}")]
    CodegenFailed {
        /// Description of the codegen failure.
        reason: String,
    },

    /// JIT compilation to native code failed.
    #[error("JIT compilation failed: {reason}")]
    CompilationFailed {
        /// Description of the compilation failure.
        reason: String,
    },

    /// The JIT runtime encountered an error during execution.
    #[error("JIT runtime error: {reason}")]
    RuntimeError {
        /// Description of the runtime error.
        reason: String,
    },

    /// The inkwell JIT backend is required but not enabled.
    #[error("inkwell JIT not enabled; rebuild with --features=inkwell-jit")]
    InkwellNotEnabled,
}
