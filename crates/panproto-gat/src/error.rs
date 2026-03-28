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

    /// A directed equation is not preserved by a morphism.
    #[error("directed equation {equation} not preserved: {detail}")]
    DirectedEquationNotPreserved {
        /// The directed equation that failed preservation.
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

    /// Directed equation conflict during colimit computation.
    #[error("directed equation conflict in colimit: {name}")]
    DirectedEqConflict {
        /// The conflicting directed equation name.
        name: String,
    },

    /// Conflict policy conflict during colimit computation.
    #[error("conflict policy conflict in colimit: {name}")]
    PolicyConflict {
        /// The conflicting policy name.
        name: String,
    },

    /// A morphism is missing a sort mapping.
    #[error("morphism missing sort mapping for: {0}")]
    MissingSortMapping(String),

    /// A morphism is missing an operation mapping.
    #[error("morphism missing operation mapping for: {0}")]
    MissingOpMapping(String),

    /// Morphism composition failed: an element in the first morphism's
    /// codomain image is not in the second morphism's domain.
    #[error(
        "compose: {kind} `{name}` maps to `{image}` which has no mapping in the second morphism"
    )]
    ComposeUnmapped {
        /// Whether this is a "sort" or "op".
        kind: &'static str,
        /// The element in the first morphism's domain.
        name: String,
        /// The image in the first morphism's codomain (missing from second morphism).
        image: String,
    },

    /// Cyclic dependency detected in theory extends chain.
    #[error("cyclic dependency detected involving theory: {0}")]
    CyclicDependency(String),

    /// Model interpretation error.
    #[error("model error: {0}")]
    ModelError(String),

    // --- Type-checking errors ---
    /// A variable was not found in the typing context.
    #[error("unbound variable: {0}")]
    UnboundVariable(String),

    /// An operation was applied to the wrong number of arguments.
    #[error("arity mismatch for operation {op}: expected {expected} args, got {got}")]
    TermArityMismatch {
        /// The operation name.
        op: String,
        /// Expected number of arguments.
        expected: usize,
        /// Actual number of arguments.
        got: usize,
    },

    /// An argument's sort doesn't match the operation's expected input sort.
    #[error("type mismatch for {op} argument {arg_index}: expected {expected}, got {got}")]
    ArgTypeMismatch {
        /// The operation name.
        op: String,
        /// Zero-based argument index.
        arg_index: usize,
        /// Expected sort name.
        expected: String,
        /// Actual sort name.
        got: String,
    },

    /// The two sides of an equation have different sorts.
    #[error("equation {equation} sides have different sorts: lhs={lhs_sort}, rhs={rhs_sort}")]
    EquationSortMismatch {
        /// The equation name.
        equation: String,
        /// Sort of the left-hand side.
        lhs_sort: String,
        /// Sort of the right-hand side.
        rhs_sort: String,
    },

    /// A variable is used at conflicting sorts across an equation.
    #[error("variable {var} used at conflicting sorts: {sort1} and {sort2}")]
    ConflictingVarSort {
        /// The variable name.
        var: String,
        /// First inferred sort.
        sort1: String,
        /// Second (conflicting) inferred sort.
        sort2: String,
    },

    // --- Natural transformation errors ---
    /// Source and target morphisms of a natural transformation have different domains.
    #[error(
        "natural transformation domain mismatch: {source_morphism} and {target_morphism} have different domains"
    )]
    NatTransDomainMismatch {
        /// Source morphism name.
        source_morphism: String,
        /// Target morphism name.
        target_morphism: String,
    },

    /// A natural transformation is missing a component for a sort.
    #[error("missing natural transformation component for sort: {0}")]
    MissingNatTransComponent(String),

    /// A natural transformation component is invalid.
    #[error("invalid natural transformation component for sort {sort}: {detail}")]
    NatTransComponentInvalid {
        /// The sort name.
        sort: String,
        /// Details about the invalidity.
        detail: String,
    },

    /// The naturality condition is violated for an operation.
    #[error("naturality violated for operation {op}")]
    NaturalityViolation {
        /// The operation where naturality fails.
        op: String,
        /// LHS of the naturality square.
        lhs: String,
        /// RHS of the naturality square.
        rhs: String,
    },

    /// Natural transformation composition mismatch.
    #[error("cannot compose: alpha target {alpha_target} != beta source {beta_source}")]
    NatTransComposeMismatch {
        /// Target morphism of first nat trans.
        alpha_target: String,
        /// Source morphism of second nat trans.
        beta_source: String,
    },

    /// Sort kind mismatch in a morphism: source and target sorts have different kinds.
    #[error("sort kind mismatch for {sort}: expected {expected:?}, got {got:?}")]
    SortKindMismatch {
        /// The sort with mismatched kind.
        sort: String,
        /// Expected sort kind.
        expected: crate::sort::SortKind,
        /// Actual sort kind.
        got: crate::sort::SortKind,
    },

    /// Sort parameter sort mismatch in a morphism: a dependent sort's parameter
    /// sort is not preserved under the sort mapping.
    #[error(
        "sort parameter mismatch for {sort} at index {param_index}: expected {expected}, got {got}"
    )]
    SortParamMismatch {
        /// The sort with mismatched parameter.
        sort: String,
        /// Zero-based parameter index.
        param_index: usize,
        /// Expected parameter sort (after mapping).
        expected: String,
        /// Actual parameter sort in the target.
        got: String,
    },

    /// Horizontal composition domain mismatch: G's codomain differs from H's domain.
    #[error("horizontal compose domain mismatch: {g_codomain} != {h_domain}")]
    HorizontalComposeMismatch {
        /// G morphism's codomain.
        g_codomain: String,
        /// H morphism's domain.
        h_domain: String,
    },

    // --- Factorization errors ---
    /// Factorization error.
    #[error("factorization error: {0}")]
    FactorizationError(String),

    // --- Quotient errors ---
    /// Identified elements are incompatible for quotienting.
    #[error("cannot identify {name_a} and {name_b}: {detail}")]
    QuotientIncompatible {
        /// First element name.
        name_a: String,
        /// Second element name.
        name_b: String,
        /// Reason for incompatibility.
        detail: String,
    },
}
