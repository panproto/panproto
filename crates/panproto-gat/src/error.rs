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

    /// Equation preservation could not be decided: the mapped equation is not
    /// syntactically present in the codomain and normalization to a common
    /// form exhausted its step budget. The morphism is rejected rather than
    /// accepted on inconclusive evidence.
    #[error("equation {equation} preservation unknown: {detail}")]
    EquationPreservationUnknown {
        /// The equation whose preservation could not be decided.
        equation: String,
        /// Details about why the check was inconclusive.
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

    /// An equality witness does not justify the equality it claims.
    #[error("invalid equality witness: {reason}")]
    WitnessInvalid {
        /// Why the witness fails to justify its claimed equality.
        reason: String,
    },

    /// A rewrite-driven comparison (equation-sort equality or naturality) ran
    /// out of steps before reaching a normal form, so equality could not be
    /// decided. This is reported instead of a spurious inequality on a
    /// truncated normalization.
    #[error("rewrite budget exhausted while checking {context} (step limit {limit})")]
    RewriteBudgetExhausted {
        /// What was being checked (an equation name or a naturality square).
        context: String,
        /// The step limit that was hit.
        limit: usize,
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

    /// Morphism composition `f ; g` requires `f.codomain == g.domain`.
    #[error(
        "compose: domain mismatch: first morphism has codomain `{first_codomain}` but second has domain `{second_domain}`"
    )]
    ComposeDomainMismatch {
        /// Codomain of the first morphism.
        first_codomain: String,
        /// Domain of the second morphism.
        second_domain: String,
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

    /// Sort-expression unification failed (e.g. heads differ, arity
    /// mismatch, or occurs check).
    #[error("sort unification failure: {reason}")]
    SortUnificationFailure {
        /// Human-readable reason for the failure.
        reason: String,
    },

    /// A morphism assigns an operation a derived term that references a
    /// variable which is not one of the operation's parameters.
    #[error(
        "term assignment for operation {op} references unbound variable {var}; only the operation's parameters are in scope"
    )]
    TermAssignmentUnboundVar {
        /// The operation being assigned.
        op: String,
        /// The unbound variable name.
        var: String,
    },

    /// A morphism assigns an operation a derived term whose inferred sort
    /// does not match the operation's mapped output sort.
    #[error(
        "term assignment for operation {op} has sort {got}, but the mapped output sort is {expected}"
    )]
    TermAssignmentSortMismatch {
        /// The operation being assigned.
        op: String,
        /// The mapped output sort `F(S)`.
        expected: String,
        /// The inferred sort of the assigned term.
        got: String,
    },

    /// A morphism assigns an operation a derived term that fails to
    /// typecheck in the codomain for a reason other than an unbound
    /// variable (e.g. an operation arity or argument-sort mismatch inside
    /// the term).
    #[error("term assignment for operation {op} is ill-typed: {detail}")]
    TermAssignmentIllTyped {
        /// The operation being assigned.
        op: String,
        /// The underlying type-checking failure.
        detail: String,
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

    // --- Free model errors ---
    /// Cyclic sort dependencies detected in topological sort.
    #[error("cyclic sort dependencies: {0:?}")]
    CyclicSortDependency(Vec<String>),

    /// Variable-sort inference failed for an equation during free-model
    /// construction, so the equation cannot be used in congruence
    /// closure. Rather than silently dropping the equation (which
    /// under-quotients the model), free-model construction surfaces this
    /// error. Wraps the underlying inference failure detail.
    #[error("cannot infer variable sorts for equation {equation}: {detail}")]
    VarSortInferenceFailed {
        /// The equation whose variable sorts could not be inferred.
        equation: String,
        /// The underlying inference failure (from `infer_var_sorts`).
        detail: String,
    },

    // --- Quotient errors ---
    /// A case expression fails to cover every constructor of its
    /// scrutinee's closed sort.
    #[error("case on sort {sort} missing branches for: {missing:?}")]
    NonExhaustiveCase {
        /// The scrutinee's sort name.
        sort: String,
        /// Constructor names not covered.
        missing: Vec<String>,
    },

    /// A case expression has two branches for the same constructor.
    #[error("case on sort {sort} has a redundant branch for constructor {constructor}")]
    RedundantCaseBranch {
        /// The scrutinee's sort name.
        sort: String,
        /// The duplicated constructor name.
        constructor: String,
    },

    /// A case branch names a constructor that is not in the scrutinee's
    /// closed-sort constructor list.
    #[error("case on sort {sort} references unknown constructor {constructor}")]
    UnknownCaseConstructor {
        /// The scrutinee's sort name.
        sort: String,
        /// The offending constructor name.
        constructor: String,
    },

    /// A case expression's scrutinee has an open sort; pattern
    /// matching requires a closed sort.
    #[error("case scrutinee has open sort {sort}; pattern matching requires a closed sort")]
    CaseOnOpenSort {
        /// The scrutinee's sort name.
        sort: String,
    },

    /// A closed sort's constructor list references an op that either
    /// does not exist, does not produce this sort, or conflicts with
    /// another op producing the sort.
    #[error("closed sort {sort} has invalid constructor {constructor}: {detail}")]
    InvalidClosedSortConstructor {
        /// The closed sort name.
        sort: String,
        /// The offending constructor op name.
        constructor: String,
        /// Details about the problem.
        detail: String,
    },

    /// A morphism does not preserve a domain sort's closure: the
    /// image of the constructor list under the op map is not the
    /// codomain sort's constructor list.
    #[error(
        "morphism fails closure preservation for sort {sort}: expected closure {expected:?}, got {got:?}"
    )]
    MorphismClosureMismatch {
        /// The domain sort name.
        sort: String,
        /// Expected constructor set (domain closure image under `op_map`).
        expected: Vec<String>,
        /// Actual constructor set in the codomain sort.
        got: Vec<String>,
    },

    /// An operation declares an implicit parameter whose value cannot
    /// be recovered from the explicit inputs at a call site.
    ///
    /// An implicit parameter must occur as a `Term::Var` somewhere in
    /// the sort expression of at least one explicit input or in the
    /// output sort, so that first-order unification of the declared
    /// input sorts against the call site's actual sorts pins down the
    /// parameter's value. Implicit parameters that do not appear in
    /// such a position are rejected at theory-declaration time.
    #[error(
        "operation {op} declares implicit parameter {param} that does not occur in any explicit input sort or the output sort"
    )]
    NonInferrableImplicit {
        /// The operation name.
        op: String,
        /// The implicit parameter name.
        param: String,
    },

    /// An instance-style binding names a key that is neither a sort
    /// parameter nor an operation of the class theory. Produced by the
    /// `instance!` proc-macro when validating its bindings.
    #[error(
        "instance {instance} binds unknown name {name} which is neither a sort nor an op of class {class}"
    )]
    InstanceBindingUnknown {
        /// The instance name.
        instance: String,
        /// The class theory name.
        class: String,
        /// The offending binding key.
        name: String,
    },

    /// An instance-style declaration passes more type arguments than the
    /// class theory has sort parameters.
    #[error(
        "instance {instance} passes {passed} type arguments to class {class} which declares {declared} sort(s)"
    )]
    InstanceTypeArgsArity {
        /// The instance name.
        instance: String,
        /// The class theory name.
        class: String,
        /// Number of type arguments passed.
        passed: usize,
        /// Number of sort parameters declared by the class.
        declared: usize,
    },

    /// A rewrite position is invalid: the path descends through a term
    /// variant that does not have the requested child index.
    #[error("invalid rewrite position {path:?}: node is {node_kind}")]
    InvalidRewritePosition {
        /// The path that was requested.
        path: Vec<usize>,
        /// The kind of node that could not be descended into.
        node_kind: &'static str,
    },

    /// An equation (or directed equation) contains one or more typed
    /// holes on its LHS or RHS. Holes are only meaningful inside terms
    /// being typechecked for completion, not in equations that must
    /// hold in every model.
    #[error("equation {equation} contains {count} hole(s); holes are not permitted in equations")]
    HolesInEquation {
        /// The equation name.
        equation: String,
        /// The number of holes encountered across both sides.
        count: usize,
    },

    /// An LPO termination check encountered a rewrite rule that contains
    /// a hole on one of its sides. Holes in rewrite rules are not
    /// meaningful for LPO comparison.
    #[error("LPO: rule {rule} contains a hole; holes are not comparable under LPO")]
    LpoHoleInRule {
        /// The offending rule name.
        rule: String,
    },

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

    /// A colimit inclusion leg induces a non-injective rename: two shared
    /// elements are identified with the same element in one input theory
    /// while their images in the other input theory differ. The pushout
    /// rejects this rather than choosing one image, since the underlying
    /// span is not the disjoint identification the construction assumes.
    #[error(
        "non-injective colimit leg on {kind} `{shared_image}`: it is the shared image of two elements mapped to the distinct targets `{first}` and `{second}`"
    )]
    NonInjectiveIdentification {
        /// Whether this is a `sort` or an `op`.
        kind: &'static str,
        /// The element in the second theory that both shared elements
        /// map onto.
        shared_image: String,
        /// One of the two conflicting first-theory targets, ordered
        /// deterministically with `second` so the message is stable.
        first: String,
        /// The other conflicting first-theory target.
        second: String,
    },

    /// Model equation checking enumerated more variable assignments for a
    /// single equation than the configured per-equation bound allows, so
    /// the equation was not checked. An empty violation list from the
    /// overall check is therefore not a proof that this equation holds.
    #[error(
        "equation `{equation}` requires {required} assignments, exceeding the per-equation bound of {limit}"
    )]
    ModelCheckLimitExceeded {
        /// The equation whose assignment space exceeded the bound.
        equation: String,
        /// The number of assignments the equation's variable carriers
        /// would require.
        required: usize,
        /// The configured per-equation assignment bound.
        limit: usize,
    },
}
