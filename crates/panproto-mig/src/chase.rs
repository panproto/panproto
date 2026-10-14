//! The chase for functor instances.
//!
//! This module provides two operations over a set-valued functor instance
//! ([`FInstance`]):
//!
//! 1. [`saturate_row_existence`]: a lightweight row-existence saturation
//!    over *ground* dependencies ("if a row matching some column-value
//!    pattern exists in one vertex table, then a row must exist in
//!    another"). It carries no variables, labeled nulls, or equalities; it
//!    runs after `Sigma_F` to close the extended instance under
//!    referential integrity.
//!
//! 2. [`chase`]: a term-level chase over tuple- and equality-generating
//!    dependencies ([`Dependency`]) with variables, labeled nulls, and
//!    null-aware equality merging. Triggers are detected by homomorphism
//!    search of a dependency's body into the instance; a firing
//!    tuple-generating dependency invents fresh [`Value::LabeledNull`]s for
//!    its existential positions, and an equality-generating dependency
//!    merges two positions via union-find over labeled nulls, failing on a
//!    constant-versus-constant conflict. Iteration and null budgets bound
//!    the run, yielding a [`ChaseOutcome::NonTermination`] rather than
//!    looping. Dependencies are derived from a theory's equations by
//!    freezing their variables ([`term_dependencies_from_theory`]).

use std::collections::{BTreeMap, HashMap};

use panproto_gat::{Equation, Term, Theory};
use panproto_inst::FInstanceHom;
use panproto_inst::functor::FInstance;
use panproto_inst::value::Value;
use panproto_schema::Schema;

/// An embedded dependency (ED) for the chase.
///
/// Represents a constraint: "if the pattern matches, then the
/// consequence must also hold." Pattern and consequence are
/// specified as vertex/value requirements.
#[derive(Clone, Debug)]
pub struct EmbeddedDependency {
    /// Pattern: vertex whose table must contain a row matching these values.
    pub pattern_vertex: String,
    /// Pattern: column-value pairs that must match.
    pub pattern_values: HashMap<String, Value>,
    /// Consequence: vertex whose table must contain a corresponding row.
    pub consequence_vertex: String,
    /// Consequence: column-value pairs that must exist.
    pub consequence_values: HashMap<String, Value>,
}

/// Error from the chase algorithm.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ChaseError {
    /// Row-existence saturation did not reach fixpoint within the
    /// iteration limit.
    #[error("row-existence saturation did not terminate after {0} iterations")]
    NonTermination(usize),
    /// An equality-generating dependency tried to equate two distinct
    /// constants; the instance is inconsistent with the dependency.
    #[error("EGD conflict: cannot equate distinct constants `{left}` and `{right}`")]
    Inconsistent {
        /// The left constant.
        left: String,
        /// The right constant.
        right: String,
    },
    /// A dependency fired but no row id was free above the largest in use
    /// for the row it adds.
    #[error("no row id is free above the largest in use")]
    RowIdsExhausted,
    /// A tuple-generating dependency fired but no labelled null id was free
    /// above the largest in use for an existential position it fills.
    #[error("no labelled null id is free above the largest in use")]
    NullIdsExhausted,
}

impl ChaseError {
    /// Whether running the chase again with a larger budget could
    /// succeed.
    ///
    /// A saturation that ran out of iterations may reach its fixpoint
    /// with more of them. An equality conflict is a property of the
    /// instance and the dependencies together, so it recurs at every
    /// budget.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::NonTermination(_))
    }
}

/// Whether two values are the same value: equal canonical bytes. Unlike
/// `Value`'s `==`, this is reflexive on `NaN` and tells `0.0` from `-0.0`,
/// so the chase agrees with the canonical bytes of the instances it builds.
fn same_value(a: &Value, b: &Value) -> bool {
    a.canonical_cmp(b).is_eq()
}

/// Returns `true` if the given row matches all of the required column-value
/// pairs ([`same_value`]).
fn row_matches(row: &HashMap<String, Value>, required: &HashMap<String, Value>) -> bool {
    required
        .iter()
        .all(|(col, val)| row.get(col).is_some_and(|v| same_value(v, val)))
}

/// Returns `true` if any row of `vertex`'s table matches all of the required
/// column-value pairs.
fn table_contains_match(
    instance: &FInstance,
    vertex: &str,
    required: &HashMap<String, Value>,
) -> bool {
    instance
        .rows(vertex)
        .into_iter()
        .any(|(_, row)| row_matches(row, required))
}

/// Saturate a functor instance under ground row-existence dependencies.
///
/// Iteratively finds active triggers (pattern matches without
/// corresponding consequences) and adds the missing rows, each under a
/// fresh [`RowId`](panproto_inst::RowId) (the least above every row in
/// use), in the order the dependencies fire. Returns the
/// fixpoint instance satisfying all dependencies. This is the
/// referential-integrity closure run after `Sigma_F`; it carries no
/// variables, labeled nulls, or equalities. For the full term-level chase,
/// see [`chase`].
///
/// Terminates when no active triggers remain or `max_iterations`
/// is reached.
///
/// # Errors
///
/// Returns [`ChaseError::NonTermination`] if saturation does not
/// converge within `max_iterations` steps, and
/// [`ChaseError::RowIdsExhausted`] when a fresh row id runs out.
pub fn saturate_row_existence(
    instance: &FInstance,
    dependencies: &[EmbeddedDependency],
    max_iterations: usize,
) -> Result<FInstance, ChaseError> {
    saturate_row_existence_with_hom(instance, dependencies, max_iterations)
        .map(|(result, _)| result)
}

/// [`saturate_row_existence`], with the row map from the input into the
/// saturated instance.
///
/// Saturation only adds rows, so every input row keeps its id, table and
/// cells and the map is the inclusion: it sends each input row to itself.
/// The rows outside its image are the ones saturation created.
///
/// # Errors
///
/// As [`saturate_row_existence`].
pub fn saturate_row_existence_with_hom(
    instance: &FInstance,
    dependencies: &[EmbeddedDependency],
    max_iterations: usize,
) -> Result<(FInstance, FInstanceHom), ChaseError> {
    let inclusion = FInstanceHom::identity(instance);
    let mut result = instance.clone();

    for _ in 0..max_iterations {
        let mut changed = false;

        for dep in dependencies {
            // The consequence a dependency adds does not depend on which
            // row triggered it, so what matters is only whether some row
            // of the pattern table matches. Reading that answer out first
            // ends the borrow before the consequence table is touched,
            // which is what the pattern table was being copied for; a
            // dependency whose pattern table is missing fires no trigger
            // and creates no consequence table.
            if !table_contains_match(&result, &dep.pattern_vertex, &dep.pattern_values) {
                continue;
            }
            if !table_contains_match(&result, &dep.consequence_vertex, &dep.consequence_values) {
                result
                    .push_row(
                        dep.consequence_vertex.as_str(),
                        dep.consequence_values.clone(),
                    )
                    .map_err(|_| ChaseError::RowIdsExhausted)?;
                changed = true;
            }
        }

        if !changed {
            return Ok((result, inclusion));
        }
    }

    Err(ChaseError::NonTermination(max_iterations))
}

/// Extract row-existence dependencies from a schema's structural constraints.
///
/// Generates two kinds of dependencies:
///
/// 1. **Required-edge dependencies**: For each vertex with required edges,
///    "if a row exists for vertex V, then a row must exist for the target
///    vertex of each required edge." This captures referential integrity.
///
/// 2. **See [`dependencies_from_theory`]** for the row-existence dependencies
///    derived from GAT equations.
#[must_use]
pub fn dependencies_from_schema(schema: &Schema) -> Vec<EmbeddedDependency> {
    let mut deps = Vec::new();

    for (vertex_id, required_edges) in &schema.required {
        for edge in required_edges {
            deps.push(EmbeddedDependency {
                pattern_vertex: vertex_id.to_string(),
                pattern_values: HashMap::new(),
                consequence_vertex: edge.tgt.to_string(),
                consequence_values: HashMap::new(),
            });
        }
    }

    // The requirement table is a hash map, so the dependencies come out in
    // whatever order it enumerates. Order them by the pair they relate, so the
    // list — and everything the chase derives by walking it — is a function of
    // the schema.
    deps.sort_by(|a, b| {
        (&a.pattern_vertex, &a.consequence_vertex).cmp(&(&b.pattern_vertex, &b.consequence_vertex))
    });

    deps
}

/// Extract row-existence dependencies from a GAT theory's equations.
///
/// Each equation `lhs = rhs` is inspected only for the *outermost operation*
/// on each side; from those operations' output/input sorts this records which
/// vertex tables must be co-inhabited. The equality asserted by the equation —
/// that the two sides denote the same value — is dropped entirely. No term
/// structure, variables, or value constraints are captured, so the resulting
/// dependencies cannot enforce the equation on data; they assert only
/// non-emptiness relationships between tables.
///
/// The equation shapes matched below (retraction, involution, commutativity)
/// are recognized by outermost-operation pattern only, as a naming aid; the
/// recognition gives the dependencies no equational force.
///
/// # Arguments
///
/// * `theory` - The GAT theory whose equations to translate.
/// * `schema` - The schema providing vertex/edge context for the operations.
///   Operations in the theory that don't correspond to schema vertex kinds are
///   skipped.
#[must_use]
pub fn dependencies_from_theory(theory: &Theory, schema: &Schema) -> Vec<EmbeddedDependency> {
    let mut deps = Vec::new();

    for eq in &theory.eqs {
        deps.extend(translate_equation(eq, theory, schema));
    }

    deps
}

/// Translate a single GAT equation into row-existence dependencies.
///
/// This does not represent the equation's equality. It looks only at the
/// outermost operation on each side and emits dependencies asserting that the
/// vertex tables for the relevant operation sorts must be co-inhabited. The
/// three shapes below are distinguished by outermost-operation pattern only:
///
/// 1. `op(inner_op(var)) = var`: a dependency between the tables for `op`'s
///    output sort and the variable's sort.
///
/// 2. `op(inner_op(var)) = other_op(var)`: a dependency between the tables for
///    the two outermost operations' output sorts.
///
/// 3. General case: a dependency between the outermost operations' sorts.
///
/// In every case the equality itself, the inner term structure, and the
/// variables are discarded; the emitted dependencies carry empty value
/// constraints.
fn translate_equation(eq: &Equation, theory: &Theory, schema: &Schema) -> Vec<EmbeddedDependency> {
    let mut deps = Vec::new();

    // Extract the outermost operation from each side
    let lhs_op = outermost_op(&eq.lhs);
    let rhs_op = outermost_op(&eq.rhs);

    match (lhs_op, rhs_op) {
        (Some(lhs_name), Some(rhs_name)) => {
            // Pattern: op_a(...) = op_b(...)
            // Both sides are operation applications.
            // Find the output sorts to determine which schema vertices
            // are involved.
            let lhs_sort = theory.find_op(&lhs_name).map(|op| op.output.to_string());
            let rhs_sort = theory.find_op(&rhs_name).map(|op| op.output.to_string());

            if let (Some(lhs_s), Some(rhs_s)) = (lhs_sort, rhs_sort) {
                // Find vertices in the schema with matching kinds
                let lhs_vertex = find_vertex_by_kind(schema, &lhs_s);
                let rhs_vertex = find_vertex_by_kind(schema, &rhs_s);

                if let (Some(lv), Some(rv)) = (lhs_vertex, rhs_vertex) {
                    deps.push(EmbeddedDependency {
                        pattern_vertex: lv,
                        pattern_values: HashMap::new(),
                        consequence_vertex: rv,
                        consequence_values: HashMap::new(),
                    });
                }
            }
        }
        (Some(op_name), None) => {
            // Pattern: op(inner(...)) = var
            // Retraction: if inner produced a value in op's output sort,
            // the variable's sort must have a corresponding row.
            if let Some(op) = theory.find_op(&op_name) {
                let output_sort = op.output.head().to_string();
                // The variable is in some input sort
                for (_, input_sort, _) in &op.inputs {
                    let out_vertex = find_vertex_by_kind(schema, &output_sort);
                    let in_vertex = find_vertex_by_kind(schema, input_sort.head());

                    if let (Some(ov), Some(iv)) = (out_vertex, in_vertex) {
                        deps.push(EmbeddedDependency {
                            pattern_vertex: ov,
                            pattern_values: HashMap::new(),
                            consequence_vertex: iv,
                            consequence_values: HashMap::new(),
                        });
                    }
                }
            }
        }
        (None, Some(op_name)) => {
            // Pattern: var = op(...)
            // Same as above but reversed
            if let Some(op) = theory.find_op(&op_name) {
                let output_sort = op.output.head().to_string();
                for (_, input_sort, _) in &op.inputs {
                    let out_vertex = find_vertex_by_kind(schema, &output_sort);
                    let in_vertex = find_vertex_by_kind(schema, input_sort.head());

                    if let (Some(ov), Some(iv)) = (out_vertex, in_vertex) {
                        deps.push(EmbeddedDependency {
                            pattern_vertex: iv,
                            pattern_values: HashMap::new(),
                            consequence_vertex: ov,
                            consequence_values: HashMap::new(),
                        });
                    }
                }
            }
        }
        (None, None) => {
            // Both sides are variables: trivial equation, no dependency
        }
    }

    deps
}

/// Extract the outermost operation name from a term.
fn outermost_op(term: &Term) -> Option<String> {
    match term {
        Term::Var(_) | Term::Case { .. } | Term::Hole { .. } | Term::Let { .. } => None,
        Term::App { op, .. } => Some(op.to_string()),
    }
}

/// Name the vertex that stands for a sort: the least by ID among the schema's
/// vertices of that kind.
///
/// A schema may hold many vertices of one kind, so this is a choice rather
/// than a lookup, and taking the least keeps the derived dependencies a
/// function of the schema rather than of its vertex table's layout.
fn find_vertex_by_kind(schema: &Schema, sort_name: &str) -> Option<String> {
    schema
        .vertices
        .values()
        .filter(|v| v.kind.as_str() == sort_name)
        .map(|v| v.id.as_str())
        .min()
        .map(ToOwned::to_owned)
}

// ===========================================================================
// Term-level chase: variables, labeled nulls, and EGDs.
// ===========================================================================

/// A term in a chase atom's column position: a dependency variable or a
/// constant value.
#[derive(Clone, Debug, PartialEq)]
pub enum AtomTerm {
    /// A variable shared across the dependency's body and head. The same
    /// variable name always denotes the same value within one firing.
    Var(String),
    /// A constant value.
    Const(Value),
}

/// An atom: a required tuple in a table, each column bound to an
/// [`AtomTerm`].
///
/// Tables are named by vertex or relation; a relation for an operation `f`
/// is a table whose columns bind the operation's inputs and output.
#[derive(Clone, Debug)]
pub struct Atom {
    /// The table (vertex or relation) name.
    pub table: String,
    /// Column-to-term bindings the tuple must satisfy.
    pub columns: HashMap<String, AtomTerm>,
}

impl Atom {
    /// Build an atom from a table name and an iterator of column/term pairs.
    pub fn new<I, S>(table: impl Into<String>, columns: I) -> Self
    where
        I: IntoIterator<Item = (S, AtomTerm)>,
        S: Into<String>,
    {
        Self {
            table: table.into(),
            columns: columns.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        }
    }
}

/// A dependency for the term-level chase.
#[derive(Clone, Debug)]
pub enum Dependency {
    /// Tuple-generating: whenever `body` matches, `head` must hold. Head
    /// variables not bound by the body become fresh labeled nulls.
    Tgd {
        /// The premise atoms.
        body: Vec<Atom>,
        /// The atoms that must exist when the premise matches.
        head: Vec<Atom>,
    },
    /// Equality-generating: whenever `body` matches, `left` and `right`
    /// denote the same value.
    Egd {
        /// The premise atoms.
        body: Vec<Atom>,
        /// The left term of the enforced equality.
        left: AtomTerm,
        /// The right term of the enforced equality.
        right: AtomTerm,
    },
}

/// Budgets bounding a chase run.
#[derive(Clone, Copy, Debug)]
pub struct ChaseBudget {
    /// Maximum number of trigger-firing rounds before giving up.
    pub max_iterations: usize,
    /// Maximum number of fresh labeled nulls the chase may introduce.
    pub max_nulls: usize,
}

impl ChaseBudget {
    /// A budget with the given iteration and null limits.
    #[must_use]
    pub const fn new(max_iterations: usize, max_nulls: usize) -> Self {
        Self {
            max_iterations,
            max_nulls,
        }
    }
}

/// The outcome of a term-level chase run.
#[derive(Clone, Debug)]
pub enum ChaseOutcome {
    /// The chase reached a fixpoint; every dependency is satisfied.
    Saturated(FInstance),
    /// A budget was exhausted before a fixpoint was reached.
    NonTermination,
}

/// A variable-to-value binding produced by homomorphism search.
type Binding = HashMap<String, Value>;

/// Run the term-level chase on a functor instance.
///
/// Repeatedly fires active triggers: tuple-generating dependencies add
/// required tuples (inventing fresh [`Value::LabeledNull`]s for
/// existential positions), and equality-generating dependencies merge two
/// positions with null-aware union-find. The chase runs the restricted
/// (standard) chase: a tuple-generating dependency fires only when its head
/// is not already satisfied.
///
/// The run is bounded by `budget`: exceeding either the iteration or the
/// null limit yields [`ChaseOutcome::NonTermination`] rather than looping.
///
/// Every row a firing adds takes a fresh [`RowId`](panproto_inst::RowId),
/// the least above every row in use, so rows are numbered in firing order:
/// dependencies in the order given, matches in ascending row id, head atoms
/// in the order given. Existing rows keep their ids.
///
/// # Errors
///
/// Returns [`ChaseError::Inconsistent`] when an equality-generating
/// dependency tries to equate two distinct constants,
/// [`ChaseError::RowIdsExhausted`] when a fresh row id runs out, and
/// [`ChaseError::NullIdsExhausted`] when a firing needs a fresh labeled null
/// and none is left above the largest in use (`u64::MAX` is handed out; an
/// id is never wrapped).
pub fn chase(
    instance: &FInstance,
    dependencies: &[Dependency],
    budget: ChaseBudget,
) -> Result<ChaseOutcome, ChaseError> {
    chase_with_hom(instance, dependencies, budget).map(|(outcome, _)| outcome)
}

/// [`chase`], with the homomorphism from the input into the chased instance.
///
/// The chase adds rows and replaces labeled nulls in place, but never merges
/// or removes a row, so every input row keeps its id and table: the row map
/// sends each input row to itself. Its null map sends each labeled null of
/// the input to the value the equalities forced on it (a constant, or the
/// null it was merged into), so the input row's cells, read along the null
/// map, are the chased row's cells. The rows outside the image are the ones
/// the chase created, in firing order. On [`ChaseOutcome::NonTermination`]
/// there is no chased instance, and the map is the one into the partial run.
///
/// # Errors
///
/// As [`chase`].
pub fn chase_with_hom(
    instance: &FInstance,
    dependencies: &[Dependency],
    budget: ChaseBudget,
) -> Result<(ChaseOutcome, FInstanceHom), ChaseError> {
    let mut result = instance.clone();
    let mut next_null = fresh_null_seed(&result);
    let mut nulls_created = 0usize;
    // Each labeled null of the input, with the value it currently stands at.
    let mut forced = input_nulls(instance);
    let hom = |forced: BTreeMap<u64, Value>| FInstanceHom::identity(instance).with_null_map(forced);

    for _ in 0..budget.max_iterations {
        let mut changed = false;

        for dep in dependencies {
            match dep {
                Dependency::Tgd { body, head } => {
                    for binding in match_body(body, &result) {
                        if head_satisfied(head, &result, &binding) {
                            continue;
                        }
                        // Fresh labeled nulls for head-only variables.
                        let mut firing = binding.clone();
                        for atom in head {
                            for term in atom.columns.values() {
                                if let AtomTerm::Var(v) = term {
                                    if !firing.contains_key(v) {
                                        if nulls_created >= budget.max_nulls {
                                            return Ok((ChaseOutcome::NonTermination, hom(forced)));
                                        }
                                        let fresh =
                                            next_null.ok_or(ChaseError::NullIdsExhausted)?;
                                        next_null = fresh.checked_add(1);
                                        firing.insert(v.clone(), Value::LabeledNull(fresh));
                                        nulls_created += 1;
                                    }
                                }
                            }
                        }
                        for atom in head {
                            let row = instantiate_row(atom, &firing);
                            result
                                .push_row(atom.table.as_str(), row)
                                .map_err(|_| ChaseError::RowIdsExhausted)?;
                        }
                        changed = true;
                    }
                }
                Dependency::Egd { body, left, right } => {
                    // Collect the equalities to enforce before mutating, so
                    // the match set is taken against a stable instance.
                    let mut equalities: Vec<(Value, Value)> = Vec::new();
                    for binding in match_body(body, &result) {
                        let a = resolve_term(left, &binding);
                        let b = resolve_term(right, &binding);
                        if let (Some(a), Some(b)) = (a, b) {
                            if !same_value(&a, &b) {
                                equalities.push((a, b));
                            }
                        }
                    }
                    for (a, b) in equalities {
                        if merge_values(&a, &b, &mut result, &mut forced)? {
                            changed = true;
                        }
                    }
                }
            }
        }

        if !changed {
            return Ok((ChaseOutcome::Saturated(result), hom(forced)));
        }
    }

    Ok((ChaseOutcome::NonTermination, hom(forced)))
}

/// Every labeled null occurring in the instance, each standing at itself.
fn input_nulls(instance: &FInstance) -> BTreeMap<u64, Value> {
    let mut nulls = BTreeMap::new();
    for (_, table) in instance.tables() {
        for (_, row) in table.rows() {
            for value in row.values() {
                if let Value::LabeledNull(id) = value {
                    nulls.insert(*id, value.clone());
                }
            }
        }
    }
    nulls
}

/// The least labeled-null id above every null of the instance (`0` when it
/// holds none), or `None` when no id is above them.
fn fresh_null_seed(instance: &FInstance) -> Option<u64> {
    let mut max_id: Option<u64> = None;
    for (_, table) in instance.tables() {
        for (_, row) in table.rows() {
            for value in row.values() {
                if let Value::LabeledNull(id) = value {
                    max_id = Some(max_id.map_or(*id, |m| m.max(*id)));
                }
            }
        }
    }
    max_id.map_or(Some(0), |m| m.checked_add(1))
}

/// Resolve an atom term to a concrete value under a binding, or `None` when
/// a variable is unbound.
fn resolve_term(term: &AtomTerm, binding: &Binding) -> Option<Value> {
    match term {
        AtomTerm::Const(v) => Some(v.clone()),
        AtomTerm::Var(v) => binding.get(v).cloned(),
    }
}

/// All bindings extending the empty binding that satisfy every body atom.
///
/// This is the homomorphism search: a conjunctive-query match of the body
/// atoms against the instance tables, joining on shared variables.
fn match_body(body: &[Atom], instance: &FInstance) -> Vec<Binding> {
    let mut bindings = vec![Binding::new()];
    for atom in body {
        let mut next = Vec::new();
        for binding in &bindings {
            extend_binding(atom, instance, binding, &mut next);
        }
        bindings = next;
        if bindings.is_empty() {
            break;
        }
    }
    bindings
}

/// Extend `binding` with every way of matching `atom` against a row of its
/// table, pushing each successful extension into `out`.
fn extend_binding(atom: &Atom, instance: &FInstance, binding: &Binding, out: &mut Vec<Binding>) {
    for (_, row) in instance.rows(&atom.table) {
        let mut candidate = binding.clone();
        if unify_atom(atom, row, &mut candidate) {
            out.push(candidate);
        }
    }
}

/// Try to unify an atom's columns against a row under a mutable binding.
fn unify_atom(atom: &Atom, row: &HashMap<String, Value>, binding: &mut Binding) -> bool {
    for (col, term) in &atom.columns {
        let Some(row_val) = row.get(col) else {
            return false;
        };
        match term {
            AtomTerm::Const(v) => {
                if !same_value(v, row_val) {
                    return false;
                }
            }
            AtomTerm::Var(v) => match binding.get(v) {
                Some(bound) if !same_value(bound, row_val) => return false,
                Some(_) => {}
                None => {
                    binding.insert(v.clone(), row_val.clone());
                }
            },
        }
    }
    true
}

/// Whether the head atoms are already satisfied under some extension of
/// `binding` (the restricted-chase applicability check).
fn head_satisfied(head: &[Atom], instance: &FInstance, binding: &Binding) -> bool {
    let mut bindings = vec![binding.clone()];
    for atom in head {
        let mut next = Vec::new();
        for b in &bindings {
            extend_binding(atom, instance, b, &mut next);
        }
        bindings = next;
        if bindings.is_empty() {
            return false;
        }
    }
    !bindings.is_empty()
}

/// Build a concrete row from a head atom under a (fully-instantiated)
/// binding. Head variables are guaranteed to be bound (to a body value or a
/// fresh null) before this is called.
fn instantiate_row(atom: &Atom, binding: &Binding) -> HashMap<String, Value> {
    atom.columns
        .iter()
        .map(|(col, term)| {
            let value = match term {
                AtomTerm::Const(v) => v.clone(),
                AtomTerm::Var(v) => binding
                    .get(v)
                    .cloned()
                    .unwrap_or(Value::LabeledNull(u64::MAX)),
            };
            (col.clone(), value)
        })
        .collect()
}

/// Merge two values under an equality-generating dependency.
///
/// A labeled null is merged into the other value by substituting it
/// throughout the instance and throughout `forced`, the value each input
/// null stands at; two distinct constants are a conflict. Returns whether
/// the instance changed.
///
/// # Errors
///
/// Returns [`ChaseError::Inconsistent`] when both values are distinct
/// constants.
fn merge_values(
    a: &Value,
    b: &Value,
    instance: &mut FInstance,
    forced: &mut BTreeMap<u64, Value>,
) -> Result<bool, ChaseError> {
    if same_value(a, b) {
        return Ok(false);
    }
    let (from, to) = match (a.as_labeled_null(), b.as_labeled_null()) {
        // Merge the higher-id null into the lower, or a null into a constant.
        (Some(na), Some(nb)) if na >= nb => (na, b),
        (Some(na), None) => (na, b),
        (_, Some(nb)) => (nb, a),
        (None, None) => {
            return Err(ChaseError::Inconsistent {
                left: format!("{a:?}"),
                right: format!("{b:?}"),
            });
        }
    };
    substitute_null(instance, from, to);
    let merged = Value::LabeledNull(from);
    for value in forced.values_mut() {
        if *value == merged {
            *value = to.clone();
        }
    }
    Ok(true)
}

/// Replace every occurrence of `Value::LabeledNull(from)` in the instance's
/// table rows with `to`.
fn substitute_null(instance: &mut FInstance, from: u64, to: &Value) {
    for id in instance.row_ids() {
        if let Some(row) = instance.row_mut(id) {
            for value in row.values_mut() {
                if *value == Value::LabeledNull(from) {
                    *value = to.clone();
                }
            }
        }
    }
}

/// Derive term-level dependencies from a theory's equations by freezing
/// their variables.
///
/// Each equation `lhs = rhs` is unfolded into relational atoms: an
/// application `f(a₁, …, aₙ)` becomes a tuple of the relation named `f`
/// with columns `in0, …, in{n-1}` bound to the arguments' results and `out`
/// bound to a fresh variable denoting the application's result. The
/// equation then becomes an equality-generating dependency whose body is
/// the union of both sides' atoms and whose enforced equality is between
/// the two sides' result terms — the retraction/involution/commutativity
/// law made effective on data.
#[must_use]
pub fn term_dependencies_from_theory(theory: &Theory) -> Vec<Dependency> {
    theory
        .eqs
        .iter()
        .filter_map(equation_to_dependency)
        .collect()
}

/// Translate one equation into an equality-generating dependency, or `None`
/// when both sides are bare variables (a trivial equation with no atoms).
fn equation_to_dependency(eq: &Equation) -> Option<Dependency> {
    let mut counter = 0usize;
    let (mut body, left) = unfold_term(&eq.lhs, &mut counter);
    let (rhs_atoms, right) = unfold_term(&eq.rhs, &mut counter);
    body.extend(rhs_atoms);
    if body.is_empty() {
        return None;
    }
    Some(Dependency::Egd { body, left, right })
}

/// Unfold a term into the relational atoms that compute it, returning the
/// atoms and the [`AtomTerm`] denoting the term's result.
fn unfold_term(term: &Term, counter: &mut usize) -> (Vec<Atom>, AtomTerm) {
    match term {
        Term::Var(name) => (Vec::new(), AtomTerm::Var(name.to_string())),
        Term::App { op, args } => {
            let mut atoms = Vec::new();
            let mut columns: HashMap<String, AtomTerm> = HashMap::new();
            for (i, arg) in args.iter().enumerate() {
                let (arg_atoms, arg_result) = unfold_term(arg, counter);
                atoms.extend(arg_atoms);
                columns.insert(format!("in{i}"), arg_result);
            }
            let result_var = format!("_{op}_{counter}");
            *counter += 1;
            columns.insert("out".to_string(), AtomTerm::Var(result_var.clone()));
            atoms.push(Atom {
                table: op.to_string(),
                columns,
            });
            (atoms, AtomTerm::Var(result_var))
        }
        // Case, Hole, and Let do not correspond to a relational atom in the
        // frozen-instance translation; treat them as opaque with no result.
        Term::Case { .. } | Term::Hole { .. } | Term::Let { .. } => {
            let result_var = format!("_opaque_{counter}");
            *counter += 1;
            (Vec::new(), AtomTerm::Var(result_var))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use panproto_gat::Name;
    use panproto_schema::Schema;

    use super::*;

    /// Helper: build a single-column row.
    fn row(col: &str, val: Value) -> HashMap<String, Value> {
        HashMap::from([(col.to_owned(), val)])
    }

    /// Helper: build a two-column row.
    fn row2(c0: &str, v0: Value, c1: &str, v1: Value) -> HashMap<String, Value> {
        HashMap::from([(c0.to_owned(), v0), (c1.to_owned(), v1)])
    }

    /// A theory `A →f B →r A` with the retraction law `r(f(x)) = x`.
    fn retraction_theory() -> Theory {
        use panproto_gat::{Equation, Operation, Sort};
        let f = Operation::unary("f", "x", "A", "B");
        let r = Operation::unary("r", "y", "B", "A");
        let retract = Equation::new(
            "retract",
            Term::app("r", vec![Term::app("f", vec![Term::var("x")])]),
            Term::var("x"),
        );
        Theory::new(
            "Retract",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![f, r],
            vec![retract],
        )
    }

    #[test]
    fn chase_enforces_retraction_equation() {
        // The retraction law derived from the theory becomes an EGD; when
        // `r`'s output is an unknown (labeled null), the chase fills it in
        // to equal `x`, enforcing `r(f(x)) = x` on the data.
        let deps = term_dependencies_from_theory(&retraction_theory());
        assert!(
            matches!(deps.as_slice(), [Dependency::Egd { .. }]),
            "retraction equation must yield a single EGD, got {deps:?}",
        );

        let instance = FInstance::new()
            .with_table(
                "f",
                vec![row2(
                    "in0",
                    Value::Str("a".into()),
                    "out",
                    Value::Str("b".into()),
                )],
            )
            .and_then(|f| {
                f.with_table(
                    "r",
                    vec![row2(
                        "in0",
                        Value::Str("b".into()),
                        "out",
                        Value::LabeledNull(0),
                    )],
                )
            })
            .unwrap();

        let outcome = chase(&instance, &deps, ChaseBudget::new(50, 50)).unwrap();
        let ChaseOutcome::Saturated(result) = outcome else {
            panic!("expected saturation, got {outcome:?}");
        };
        // r's output null is merged with x = "a".
        let r_rows = result.rows("r");
        assert_eq!(r_rows.len(), 1);
        assert_eq!(r_rows[0].1.get("out"), Some(&Value::Str("a".into())));
    }

    #[test]
    fn chase_egd_merges_null() {
        // An EGD equating two columns merges a labeled null with a constant.
        let instance = FInstance::new()
            .with_table(
                "t",
                vec![row2(
                    "a",
                    Value::Str("x".into()),
                    "b",
                    Value::LabeledNull(0),
                )],
            )
            .unwrap();
        let egd = Dependency::Egd {
            body: vec![Atom::new(
                "t",
                [
                    ("a", AtomTerm::Var("v".into())),
                    ("b", AtomTerm::Var("w".into())),
                ],
            )],
            left: AtomTerm::Var("v".into()),
            right: AtomTerm::Var("w".into()),
        };

        let outcome = chase(&instance, &[egd], ChaseBudget::new(50, 50)).unwrap();
        let ChaseOutcome::Saturated(result) = outcome else {
            panic!("expected saturation, got {outcome:?}");
        };
        let rows = result.rows("t");
        assert_eq!(rows[0].1.get("b"), Some(&Value::Str("x".into())));
        // The constant-versus-null merge conflicting with a constant fails.
        let bad = Dependency::Egd {
            body: vec![Atom::new("t", [("a", AtomTerm::Var("v".into()))])],
            left: AtomTerm::Var("v".into()),
            right: AtomTerm::Const(Value::Str("z".into())),
        };
        let err = chase(&instance, &[bad], ChaseBudget::new(50, 50)).unwrap_err();
        assert!(
            matches!(err, ChaseError::Inconsistent { .. }),
            "constant-constant EGD conflict must be rejected, got {err:?}",
        );
    }

    #[test]
    fn chase_budget_nontermination() {
        // A tuple-generating dependency that regenerates itself with a fresh
        // null each round exhausts the null budget and reports
        // non-termination rather than looping.
        let instance = FInstance::new()
            .with_table("chain", vec![row("val", Value::Int(0))])
            .unwrap();
        let tgd = Dependency::Tgd {
            body: vec![Atom::new("chain", [("val", AtomTerm::Var("v".into()))])],
            head: vec![Atom::new(
                "chain",
                [
                    ("prev", AtomTerm::Var("v".into())),
                    ("val", AtomTerm::Var("w".into())),
                ],
            )],
        };

        let outcome = chase(&instance, &[tgd], ChaseBudget::new(1000, 3)).unwrap();
        assert!(
            matches!(outcome, ChaseOutcome::NonTermination),
            "exhausting the null budget must yield NonTermination, got {outcome:?}",
        );
    }

    #[test]
    fn chase_fixpoint_on_satisfied_instance() {
        // An instance already satisfying the retraction EGD reaches a
        // fixpoint without changing.
        let deps = term_dependencies_from_theory(&retraction_theory());
        let instance = FInstance::new()
            .with_table(
                "f",
                vec![row2(
                    "in0",
                    Value::Str("a".into()),
                    "out",
                    Value::Str("b".into()),
                )],
            )
            .and_then(|f| {
                f.with_table(
                    "r",
                    vec![row2(
                        "in0",
                        Value::Str("b".into()),
                        "out",
                        Value::Str("a".into()),
                    )],
                )
            })
            .unwrap();

        let outcome = chase(&instance, &deps, ChaseBudget::new(50, 50)).unwrap();
        let ChaseOutcome::Saturated(result) = outcome else {
            panic!("expected saturation, got {outcome:?}");
        };
        assert_eq!(
            result, instance,
            "an already-satisfied instance is unchanged"
        );
        assert_eq!(result.row_count("f"), 1);
    }

    #[test]
    fn chase_allocates_rows_in_firing_order() {
        use panproto_inst::RowId;
        // A holds rows 0 (x = 1) and 1 (x = 2); each firing adds a B row and
        // then a C row.
        let instance = FInstance::new()
            .with_table("A", vec![row("x", Value::Int(1)), row("x", Value::Int(2))])
            .unwrap();
        let tgd = Dependency::Tgd {
            body: vec![Atom::new("A", [("x", AtomTerm::Var("v".into()))])],
            head: vec![
                Atom::new("B", [("y", AtomTerm::Var("v".into()))]),
                Atom::new("C", [("z", AtomTerm::Var("v".into()))]),
            ],
        };
        let outcome = chase(&instance, &[tgd], ChaseBudget::new(10, 10)).unwrap();
        let ChaseOutcome::Saturated(result) = outcome else {
            panic!("expected saturation, got {outcome:?}");
        };
        let at = |id: u32| {
            (
                result.table_of(RowId(id)).map(ToString::to_string),
                result.row(RowId(id)).cloned(),
            )
        };
        assert_eq!(at(2), (Some("B".into()), Some(row("y", Value::Int(1)))));
        assert_eq!(at(3), (Some("C".into()), Some(row("z", Value::Int(1)))));
        assert_eq!(at(4), (Some("B".into()), Some(row("y", Value::Int(2)))));
        assert_eq!(at(5), (Some("C".into()), Some(row("z", Value::Int(2)))));
        assert_eq!(result.total_row_count(), 6);
        // The input rows keep their ids.
        assert_eq!(result.row(RowId(0)), instance.row(RowId(0)));
    }

    #[test]
    fn the_chase_row_map_carries_the_values_an_egd_forced() {
        // Two input rows share the labeled null 0 and a third holds null 1;
        // the EGD forces null 0 to "x", then null 1 to null 0's value.
        let instance = FInstance::new()
            .with_table(
                "t",
                vec![
                    row2("a", Value::Str("x".into()), "b", Value::LabeledNull(0)),
                    row2("a", Value::LabeledNull(1), "b", Value::LabeledNull(0)),
                ],
            )
            .unwrap();
        let egd = Dependency::Egd {
            body: vec![Atom::new(
                "t",
                [
                    ("a", AtomTerm::Var("v".into())),
                    ("b", AtomTerm::Var("w".into())),
                ],
            )],
            left: AtomTerm::Var("v".into()),
            right: AtomTerm::Var("w".into()),
        };
        let (outcome, hom) = chase_with_hom(&instance, &[egd], ChaseBudget::new(50, 50)).unwrap();
        let ChaseOutcome::Saturated(result) = outcome else {
            panic!("expected saturation, got {outcome:?}");
        };
        for (_, row) in result.rows("t") {
            assert_eq!(row.get("a"), Some(&Value::Str("x".into())));
            assert_eq!(row.get("b"), Some(&Value::Str("x".into())));
        }
        // The row map sends each input row to its chased row.
        assert_eq!(hom.check(&instance, &result), Ok(()));
    }

    #[test]
    fn every_row_creating_chase_returns_its_row_map() {
        use panproto_inst::RowId;
        let instance = FInstance::new()
            .with_table("A", vec![row("x", Value::Int(1)), row("x", Value::Int(2))])
            .unwrap();
        let tgd = Dependency::Tgd {
            body: vec![Atom::new("A", [("x", AtomTerm::Var("v".into()))])],
            head: vec![Atom::new("B", [("y", AtomTerm::Var("v".into()))])],
        };
        let (outcome, hom) = chase_with_hom(&instance, &[tgd], ChaseBudget::new(10, 10)).unwrap();
        let ChaseOutcome::Saturated(result) = outcome else {
            panic!("expected saturation, got {outcome:?}");
        };
        assert_eq!(
            hom.row_map
                .iter()
                .map(|(a, b)| (*a, *b))
                .collect::<Vec<_>>(),
            vec![(RowId(0), RowId(0)), (RowId(1), RowId(1))]
        );
        hom.check_tables(&instance, &result).unwrap();
        let created: Vec<RowId> = result
            .row_ids()
            .into_iter()
            .filter(|id| !hom.row_map.values().any(|image| image == id))
            .collect();
        assert_eq!(created, vec![RowId(2), RowId(3)]);

        let dep = EmbeddedDependency {
            pattern_vertex: "A".into(),
            pattern_values: HashMap::new(),
            consequence_vertex: "C".into(),
            consequence_values: row("z", Value::Int(0)),
        };
        let (saturated, hom) = saturate_row_existence_with_hom(&instance, &[dep], 10).unwrap();
        assert_eq!(hom, FInstanceHom::identity(&instance));
        hom.check_tables(&instance, &saturated).unwrap();
        assert_eq!(saturated.row_ids(), vec![RowId(0), RowId(1), RowId(2)]);
    }

    #[test]
    fn chase_no_change_when_constraints_satisfied() {
        // Instance already has the consequence row.
        let instance = FInstance::new()
            .with_table("A", vec![row("x", Value::Int(1))])
            .and_then(|f| f.with_table("B", vec![row("y", Value::Int(2))]))
            .unwrap();

        let dep = EmbeddedDependency {
            pattern_vertex: "A".to_owned(),
            pattern_values: HashMap::from([("x".to_owned(), Value::Int(1))]),
            consequence_vertex: "B".to_owned(),
            consequence_values: HashMap::from([("y".to_owned(), Value::Int(2))]),
        };

        let result = saturate_row_existence(&instance, &[dep], 10).unwrap();
        assert_eq!(result.row_count("A"), 1);
        assert_eq!(result.row_count("B"), 1);
    }

    #[test]
    fn chase_adds_missing_consequence_row() {
        // Instance has the pattern but not the consequence.
        let instance = FInstance::new()
            .with_table("A", vec![row("x", Value::Int(1))])
            .unwrap();

        let dep = EmbeddedDependency {
            pattern_vertex: "A".to_owned(),
            pattern_values: HashMap::from([("x".to_owned(), Value::Int(1))]),
            consequence_vertex: "B".to_owned(),
            consequence_values: HashMap::from([("y".to_owned(), Value::Int(2))]),
        };

        let result = saturate_row_existence(&instance, &[dep], 10).unwrap();
        assert_eq!(result.row_count("A"), 1);
        assert_eq!(result.row_count("B"), 1);

        let b_rows = result.rows("B");
        assert_eq!(b_rows[0].1.get("y"), Some(&Value::Int(2)));
        // The added row takes the least id above every row in use.
        assert_eq!(b_rows[0].0, panproto_inst::RowId(1));
    }

    #[test]
    fn chase_multi_iteration_fixpoint() {
        // Chain: A triggers B, B triggers C. Needs two iterations.
        let instance = FInstance::new()
            .with_table("A", vec![row("x", Value::Int(1))])
            .unwrap();

        let deps = vec![
            EmbeddedDependency {
                pattern_vertex: "A".to_owned(),
                pattern_values: HashMap::from([("x".to_owned(), Value::Int(1))]),
                consequence_vertex: "B".to_owned(),
                consequence_values: HashMap::from([("x".to_owned(), Value::Int(1))]),
            },
            EmbeddedDependency {
                pattern_vertex: "B".to_owned(),
                pattern_values: HashMap::from([("x".to_owned(), Value::Int(1))]),
                consequence_vertex: "C".to_owned(),
                consequence_values: HashMap::from([("x".to_owned(), Value::Int(1))]),
            },
        ];

        let result = saturate_row_existence(&instance, &deps, 10).unwrap();
        assert_eq!(result.row_count("A"), 1);
        assert_eq!(result.row_count("B"), 1);
        assert_eq!(result.row_count("C"), 1);
    }

    #[test]
    fn chase_non_termination_error() {
        // A dependency that generates a new row each iteration:
        // every row in A with x=1 requires a row in A with x=2,
        // and every row with x=2 requires x=1, but we use a
        // self-referential dependency with distinct consequence
        // values that keep accumulating.
        //
        // Simpler approach: pattern on A where x=1, consequence adds
        // a *different* row to A (x=2). Then pattern on A where x=2,
        // consequence adds x=3, etc. But since consequence_values are
        // static, we need a cycle that keeps adding.
        //
        // We achieve non-termination by having the consequence add to
        // the same table with values that do NOT match the pattern,
        // but a second dependency triggers on those new rows.
        // Actually, the simplest non-terminating chase: the consequence
        // itself is a new pattern trigger for another dependency that
        // adds yet another row, forming an infinite chain.
        //
        // With static consequence_values this won't diverge because
        // the same row won't be added twice. So we use a counter-based
        // trick: dependency adds to table B, another dep from B adds
        // to C, another from C adds to B with *different* values, but
        // that still converges.
        //
        // The realistic way to test this: use max_iterations = 0.
        let instance = FInstance::new()
            .with_table("A", vec![row("x", Value::Int(1))])
            .unwrap();

        let dep = EmbeddedDependency {
            pattern_vertex: "A".to_owned(),
            pattern_values: HashMap::from([("x".to_owned(), Value::Int(1))]),
            consequence_vertex: "B".to_owned(),
            consequence_values: HashMap::from([("y".to_owned(), Value::Int(2))]),
        };

        let err = saturate_row_existence(&instance, &[dep], 0).unwrap_err();
        assert!(
            matches!(err, ChaseError::NonTermination(0)),
            "expected NonTermination(0), got {err:?}"
        );
    }

    #[test]
    fn chase_no_trigger_when_pattern_absent() {
        // Pattern vertex has no matching rows, so no consequence added.
        let instance = FInstance::new()
            .with_table("A", vec![row("x", Value::Int(99))])
            .unwrap();

        let dep = EmbeddedDependency {
            pattern_vertex: "A".to_owned(),
            pattern_values: HashMap::from([("x".to_owned(), Value::Int(1))]),
            consequence_vertex: "B".to_owned(),
            consequence_values: HashMap::from([("y".to_owned(), Value::Int(2))]),
        };

        let result = saturate_row_existence(&instance, &[dep], 10).unwrap();
        assert_eq!(result.row_count("B"), 0);
    }

    #[test]
    fn dependencies_from_schema_empty_when_no_required() {
        let schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
            ..Default::default()
        };
        let deps = dependencies_from_schema(&schema);
        assert!(deps.is_empty());
    }

    #[test]
    fn dependencies_from_schema_extracts_required_edges() {
        use panproto_schema::Edge;

        let mut required = HashMap::new();
        required.insert(
            Name::from("user"),
            vec![Edge {
                src: Name::from("user"),
                tgt: Name::from("profile"),
                kind: Name::from("prop"),
                name: Some(Name::from("profile")),
            }],
        );

        let schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required,
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
            ..Default::default()
        };

        let deps = dependencies_from_schema(&schema);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].pattern_vertex, "user");
        assert_eq!(deps[0].consequence_vertex, "profile");
    }

    #[test]
    fn dependencies_from_theory_retraction() {
        use panproto_gat::{Equation, Operation, Sort, Term, Theory};

        // ThCoproduct-style: variant_of(injection(v)) = v
        let theory = Theory::new(
            "ThTest",
            vec![Sort::simple("Vertex"), Sort::simple("Variant")],
            vec![
                Operation::unary("injection", "v", "Variant", "Vertex"),
                Operation::unary("variant_of", "v", "Vertex", "Variant"),
            ],
            vec![Equation::new(
                "retraction",
                Term::app(
                    "variant_of",
                    vec![Term::app("injection", vec![Term::var("v")])],
                ),
                Term::var("v"),
            )],
        );

        // Schema with vertices whose kinds match the sorts
        let schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::from([
                (
                    Name::from("v1"),
                    panproto_schema::Vertex {
                        id: Name::from("v1"),
                        kind: Name::from("Vertex"),
                        nsid: None,
                    },
                ),
                (
                    Name::from("var1"),
                    panproto_schema::Vertex {
                        id: Name::from("var1"),
                        kind: Name::from("Variant"),
                        nsid: None,
                    },
                ),
            ]),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
            ..Default::default()
        };

        let deps = dependencies_from_theory(&theory, &schema);
        // The retraction equation variant_of(injection(v)) = v
        // should produce a dependency: Variant vertex → Vertex vertex
        // (variant_of outputs Variant, and the var is Variant)
        assert!(!deps.is_empty(), "retraction should produce dependencies");
    }

    #[test]
    fn dependencies_from_theory_symmetric_graph() {
        use panproto_gat::{Equation, Operation, Sort, Term, Theory};

        // ThSymmetricGraph: src(inv(e)) = tgt(e), inv(inv(e)) = e
        let theory = Theory::new(
            "ThSym",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![
                Operation::unary("inv", "e", "Edge", "Edge"),
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("tgt", "e", "Edge", "Vertex"),
            ],
            vec![
                Equation::new(
                    "src_inv",
                    Term::app("src", vec![Term::app("inv", vec![Term::var("e")])]),
                    Term::app("tgt", vec![Term::var("e")]),
                ),
                Equation::new(
                    "inv_inv",
                    Term::app("inv", vec![Term::app("inv", vec![Term::var("e")])]),
                    Term::var("e"),
                ),
            ],
        );

        let schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::from([
                (
                    Name::from("v"),
                    panproto_schema::Vertex {
                        id: Name::from("v"),
                        kind: Name::from("Vertex"),
                        nsid: None,
                    },
                ),
                (
                    Name::from("e"),
                    panproto_schema::Vertex {
                        id: Name::from("e"),
                        kind: Name::from("Edge"),
                        nsid: None,
                    },
                ),
            ]),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
            ..Default::default()
        };

        let deps = dependencies_from_theory(&theory, &schema);
        // src_inv: src(inv(e)) = tgt(e) → dependency between Vertex vertices
        // inv_inv: inv(inv(e)) = e → retraction dependency on Edge
        assert!(
            deps.len() >= 2,
            "symmetric graph equations should produce at least 2 dependencies, got {}",
            deps.len()
        );
    }

    #[test]
    fn dependencies_from_theory_empty_equations() {
        use panproto_gat::{Sort, Theory};

        let theory = Theory::new(
            "ThNoEqs",
            vec![Sort::simple("Vertex")],
            vec![],
            vec![], // no equations
        );

        let schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
            ..Default::default()
        };

        let deps = dependencies_from_theory(&theory, &schema);
        assert!(deps.is_empty(), "no equations means no dependencies");
    }

    /// The equality an equality-generating dependency enforces between two
    /// columns `a` and `b` of table `t`.
    fn columns_agree() -> Dependency {
        Dependency::Egd {
            body: vec![Atom::new(
                "t",
                [
                    ("a", AtomTerm::Var("v".into())),
                    ("b", AtomTerm::Var("w".into())),
                ],
            )],
            left: AtomTerm::Var("v".into()),
            right: AtomTerm::Var("w".into()),
        }
    }

    /// Values are compared by canonical bytes: two `NaN` cells are the same
    /// value, so equating them is already satisfied and changes nothing.
    #[test]
    fn equating_nan_with_nan_is_satisfied() {
        let nan = Value::Float(f64::NAN);
        let instance = FInstance::new()
            .with_table("t", vec![row2("a", nan.clone(), "b", nan)])
            .unwrap();
        let outcome = chase(&instance, &[columns_agree()], ChaseBudget::new(50, 50)).unwrap();
        let ChaseOutcome::Saturated(result) = outcome else {
            panic!("expected saturation, got {outcome:?}");
        };
        assert_eq!(result, instance);
    }

    /// `0.0` and `-0.0` are distinct constants (their canonical bytes
    /// differ), so equating them is a conflict, not a satisfied equality.
    #[test]
    fn equating_signed_zeros_is_inconsistent() {
        let instance = FInstance::new()
            .with_table(
                "t",
                vec![row2("a", Value::Float(0.0), "b", Value::Float(-0.0))],
            )
            .unwrap();
        let err = chase(&instance, &[columns_agree()], ChaseBudget::new(50, 50)).unwrap_err();
        assert!(matches!(err, ChaseError::Inconsistent { .. }), "{err:?}");
    }

    /// A constant in a body atom matches a cell exactly: `NaN` matches a
    /// `NaN` cell, and `0.0` does not match `-0.0`.
    #[test]
    fn a_body_constant_matches_cells_exactly() {
        let fire_on = |constant: Value, cell: Value| {
            let instance = FInstance::new()
                .with_table("t", vec![row("a", cell)])
                .unwrap();
            let tgd = Dependency::Tgd {
                body: vec![Atom::new("t", [("a", AtomTerm::Const(constant))])],
                head: vec![Atom::new("u", [("a", AtomTerm::Const(Value::Int(1)))])],
            };
            let ChaseOutcome::Saturated(result) =
                chase(&instance, &[tgd], ChaseBudget::new(50, 50)).unwrap()
            else {
                panic!("expected saturation");
            };
            result.row_count("u")
        };
        assert_eq!(fire_on(Value::Float(f64::NAN), Value::Float(f64::NAN)), 1);
        assert_eq!(fire_on(Value::Float(0.0), Value::Float(-0.0)), 0);
    }

    /// Row-existence saturation finds a consequence row holding `NaN`, so it
    /// adds it once and stops instead of adding it every round.
    #[test]
    fn saturation_finds_a_nan_consequence() {
        let instance = FInstance::new()
            .with_table("p", vec![row("k", Value::Int(1))])
            .unwrap();
        let dep = EmbeddedDependency {
            pattern_vertex: "p".into(),
            pattern_values: row("k", Value::Int(1)),
            consequence_vertex: "q".into(),
            consequence_values: row("x", Value::Float(f64::NAN)),
        };
        let result = saturate_row_existence(&instance, &[dep], 10).unwrap();
        assert_eq!(result.row_count("q"), 1);
    }

    /// `t(a = x) -> s(a = x, h0 = y0, ..)`: a tuple-generating dependency
    /// whose every firing invents `fresh` labelled nulls.
    fn inventing(fresh: usize) -> Dependency {
        let mut head = vec![("a".to_owned(), AtomTerm::Var("x".into()))];
        head.extend((0..fresh).map(|k| (format!("h{k}"), AtomTerm::Var(format!("y{k}")))));
        Dependency::Tgd {
            body: vec![Atom::new("t", [("a", AtomTerm::Var("x".into()))])],
            head: vec![Atom::new("s", head)],
        }
    }

    /// Every labelled null of `instance`'s table `table`.
    fn table_nulls(instance: &FInstance, table: &str) -> Vec<u64> {
        let mut nulls: Vec<u64> = instance
            .rows(table)
            .into_iter()
            .flat_map(|(_, row)| {
                row.values()
                    .filter_map(Value::as_labeled_null)
                    .collect::<Vec<_>>()
            })
            .collect();
        nulls.sort_unstable();
        nulls
    }

    /// Fresh labelled nulls continue above the largest in use, up to and
    /// including `u64::MAX`; a firing that needs one more is refused with
    /// `NullIdsExhausted`, never a wrapped id that collides with a null of
    /// the input, and a firing that needs none is no obstacle. Exhaustive
    /// over the largest input null in `u64::MAX - 3..=u64::MAX` and every
    /// fresh-null count up to 5, with and without a null `0` in the input.
    #[test]
    fn fresh_null_ids_are_checked_at_the_top_of_u64() {
        for max in u64::MAX - 3..=u64::MAX {
            for with_zero in [false, true] {
                for fresh in 0..=5_usize {
                    let mut inputs = vec![max];
                    if with_zero {
                        inputs.insert(0, 0);
                    }
                    let rows: Vec<_> = inputs
                        .iter()
                        .map(|n| row("a", Value::LabeledNull(*n)))
                        .collect();
                    let instance = FInstance::new().with_table("t", rows).unwrap();
                    let budget = ChaseBudget::new(10, 100);
                    let outcome = chase(&instance, &[inventing(fresh)], budget);
                    let needed = u64::try_from(fresh * inputs.len()).unwrap();
                    let fits = max.checked_add(needed).is_some();
                    let case = format!("max {max}, zero {with_zero}, fresh {fresh}");
                    if !fits {
                        assert!(
                            matches!(outcome, Err(ChaseError::NullIdsExhausted)),
                            "{case}: {outcome:?}"
                        );
                        continue;
                    }
                    let Ok(ChaseOutcome::Saturated(out)) = outcome else {
                        panic!("{case}: {outcome:?}");
                    };
                    let mut expected: Vec<u64> = inputs.clone();
                    expected.extend((1..=needed).map(|k| max + k));
                    expected.sort_unstable();
                    assert_eq!(table_nulls(&out, "s"), expected, "{case}");
                }
            }
        }
    }

    /// With no labelled null in the input, the first fresh null is `0`.
    #[test]
    fn the_first_fresh_null_of_a_null_free_input_is_zero() {
        let instance = FInstance::new()
            .with_table("t", vec![row("a", Value::Int(1))])
            .unwrap();
        let dep = Dependency::Tgd {
            body: vec![Atom::new("t", [("a", AtomTerm::Var("x".into()))])],
            head: vec![Atom::new(
                "s",
                [
                    ("a", AtomTerm::Var("x".into())),
                    ("b", AtomTerm::Var("y".into())),
                ],
            )],
        };
        let outcome = chase(&instance, &[dep], ChaseBudget::new(10, 10)).unwrap();
        let ChaseOutcome::Saturated(out) = outcome else {
            panic!("expected saturation, got {outcome:?}");
        };
        let rows: Vec<_> = out
            .rows("s")
            .into_iter()
            .map(|(_, row)| row.clone())
            .collect();
        assert_eq!(
            rows,
            vec![row2("a", Value::Int(1), "b", Value::LabeledNull(0))]
        );
    }

    /// Running out of labelled null ids is a property of the input, so a
    /// larger budget cannot help.
    #[test]
    fn null_exhaustion_is_not_retryable() {
        assert!(!ChaseError::NullIdsExhausted.is_retryable());
    }
}
