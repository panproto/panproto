//! Error types for edit lens operations.

/// Errors from edit lens translation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EditLensError {
    /// An edit could not be translated through the lens.
    #[error("translation failed: {0}")]
    TranslationFailed(String),

    /// A complement policy conflict was encountered.
    #[error("policy conflict for kind {kind}: {detail}")]
    PolicyConflict {
        /// The vertex kind involved.
        kind: String,
        /// Description of the conflict.
        detail: String,
    },

    /// A directed equation has no inverse, but one was needed for `put_edit`.
    #[error("no inverse for directed equation: {rule_name}")]
    NoInverse {
        /// The name of the directed equation.
        rule_name: String,
    },

    /// The complement state is inconsistent with the current edit.
    #[error("complement inconsistent: {0}")]
    ComplementInconsistent(String),

    /// Delegation to the restrict pipeline failed.
    #[error("restrict error: {0}")]
    Restrict(#[from] panproto_inst::RestrictError),

    /// An edit could not be applied to the instance.
    #[error("edit apply error: {0}")]
    EditApply(#[from] panproto_inst::EditError),

    /// A refinement constraint on the target schema was violated.
    #[error(
        "refinement violation on vertex {vertex}: constraint {constraint_sort}({constraint_value}) failed"
    )]
    RefinementViolation {
        /// The target vertex whose constraint was violated.
        vertex: String,
        /// The constraint sort (e.g., `"maxLength"`).
        constraint_sort: String,
        /// The constraint value (e.g., `"3000"`).
        constraint_value: String,
        /// Description of why the constraint failed.
        detail: String,
    },
}
