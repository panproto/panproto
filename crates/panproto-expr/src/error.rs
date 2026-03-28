//! Error types for expression evaluation and type-checking.

/// Errors that can occur during expression evaluation or type-checking.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExprError {
    /// Evaluation exceeded the configured step limit.
    #[error("step limit exceeded after {0} steps")]
    StepLimitExceeded(u64),

    /// Evaluation exceeded the configured recursion depth.
    #[error("recursion depth exceeded: {0}")]
    DepthExceeded(u32),

    /// A variable was referenced but not bound in the environment.
    #[error("unbound variable: {0}")]
    UnboundVariable(String),

    /// An operation received a value of the wrong type.
    #[error("type error: expected {expected}, got {got}")]
    TypeError {
        /// The expected type name.
        expected: String,
        /// The actual type name.
        got: String,
    },

    /// A builtin was applied to the wrong number of arguments.
    #[error("arity mismatch for {op}: expected {expected}, got {got}")]
    ArityMismatch {
        /// The builtin operation name.
        op: String,
        /// Expected number of arguments.
        expected: usize,
        /// Actual number of arguments.
        got: usize,
    },

    /// List index was out of bounds.
    #[error("index out of bounds: {index} for list of length {len}")]
    IndexOutOfBounds {
        /// The index that was attempted.
        index: i64,
        /// The actual list length.
        len: usize,
    },

    /// A field was not found in a record.
    #[error("field not found: {0}")]
    FieldNotFound(String),

    /// No pattern arm matched the scrutinee.
    #[error("non-exhaustive match")]
    NonExhaustiveMatch,

    /// Division by zero.
    #[error("division by zero")]
    DivisionByZero,

    /// A list operation exceeded the configured maximum list length.
    #[error("list length limit exceeded: {0}")]
    ListLengthExceeded(usize),

    /// Failed to parse a string as a number.
    #[error("parse error: cannot convert {value:?} to {target_type}")]
    ParseError {
        /// The string value that failed to parse.
        value: String,
        /// The target type name.
        target_type: String,
    },

    /// Attempted to call a non-function value.
    #[error("not a function")]
    NotAFunction,

    /// Integer arithmetic overflowed.
    #[error("integer overflow")]
    Overflow,

    /// Float value is not representable as an integer (NaN, infinity, or out of range).
    #[error("float not representable as integer: {0}")]
    FloatNotRepresentable(String),
}
