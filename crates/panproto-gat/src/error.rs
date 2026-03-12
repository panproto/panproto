/// Errors that can occur in GAT operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GatError {
    /// A referenced sort was not found in the theory.
    #[error("sort not found: {0}")]
    SortNotFound(String),

    /// A referenced operation was not found in the theory.
    #[error("operation not found: {0}")]
    OpNotFound(String),

    /// A referenced theory was not found in the registry.
    #[error("theory not found: {0}")]
    TheoryNotFound(String),

    /// Sort arity mismatch in a morphism or model.
    #[error("sort arity mismatch for {sort}: expected {expected}, got {got}")]
    SortArityMismatch {
        /// The sort with mismatched arity.
        sort: String,
        /// Expected number of parameters.
        expected: usize,
        /// Actual number of parameters.
        got: usize,
    },

    /// Operation type mismatch in a morphism or model.
    #[error("operation type mismatch for {op}: {detail}")]
    OpTypeMismatch {
        /// The operation with mismatched types.
        op: String,
        /// Details about the mismatch.
        detail: String,
    },

    /// An equation is not preserved by a morphism.
    #[error("equation {equation} not preserved: {detail}")]
    EquationNotPreserved {
        /// The equation that failed preservation.
        equation: String,
        /// Details about the failure.
        detail: String,
    },

    /// Sort conflict during colimit computation.
    #[error("sort conflict in colimit: {name} has incompatible definitions")]
    SortConflict {
        /// The conflicting sort name.
        name: String,
    },

    /// Operation conflict during colimit computation.
    #[error("operation conflict in colimit: {name} has incompatible definitions")]
    OpConflict {
        /// The conflicting operation name.
        name: String,
    },

    /// Equation conflict during colimit computation.
    #[error("equation conflict in colimit: {name}")]
    EqConflict {
        /// The conflicting equation name.
        name: String,
    },

    /// A morphism is missing a sort mapping.
    #[error("morphism missing sort mapping for: {0}")]
    MissingSortMapping(String),

    /// A morphism is missing an operation mapping.
    #[error("morphism missing operation mapping for: {0}")]
    MissingOpMapping(String),

    /// Cyclic dependency detected in theory extends chain.
    #[error("cyclic dependency detected involving theory: {0}")]
    CyclicDependency(String),

    /// Model interpretation error.
    #[error("model error: {0}")]
    ModelError(String),
}
