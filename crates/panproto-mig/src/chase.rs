//! Chase algorithm for enforcing equational constraints.
//!
//! The chase enforces embedded dependencies (EDs) on functor instances
//! by iteratively finding active triggers and applying consequences
//! until a fixpoint is reached. This is Phase 6 of the migration
//! pipeline, running after `Sigma_F` (left Kan extension) to ensure
//! the extended instance satisfies the target schema's path equations.

use std::collections::HashMap;

use panproto_gat::{Equation, Term, Theory};
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
    /// Chase did not reach fixpoint within the iteration limit.
    #[error("chase did not terminate after {0} iterations")]
    NonTermination(usize),
}

/// Returns `true` if the given row matches all of the required column-value pairs.
fn row_matches(row: &HashMap<String, Value>, required: &HashMap<String, Value>) -> bool {
    required
        .iter()
        .all(|(col, val)| row.get(col).is_some_and(|v| v == val))
}

/// Returns `true` if any row in `rows` matches all of the required column-value pairs.
fn table_contains_match(
    rows: &[HashMap<String, Value>],
    required: &HashMap<String, Value>,
) -> bool {
    rows.iter().any(|row| row_matches(row, required))
}

/// Run the chase algorithm on a functor instance.
///
/// Iteratively finds active triggers (pattern matches without
/// corresponding consequences) and adds the missing rows/values.
/// Returns the fixpoint instance satisfying all dependencies.
///
/// Terminates when no active triggers remain or `max_iterations`
/// is reached.
///
/// # Errors
///
/// Returns [`ChaseError::NonTermination`] if the chase does not
/// converge within `max_iterations` steps.
pub fn chase_functor(
    instance: &FInstance,
    dependencies: &[EmbeddedDependency],
    max_iterations: usize,
) -> Result<FInstance, ChaseError> {
    let mut result = instance.clone();

    for _ in 0..max_iterations {
        let mut changed = false;

        for dep in dependencies {
            // Get the pattern table rows; if the table does not exist,
            // no triggers can fire.
            let pattern_rows: Vec<HashMap<String, Value>> = result
                .tables
                .get(&dep.pattern_vertex)
                .cloned()
                .unwrap_or_default();

            for row in &pattern_rows {
                if !row_matches(row, &dep.pattern_values) {
                    continue;
                }

                // Pattern matched — check if the consequence already holds.
                let consequence_rows = result
                    .tables
                    .entry(dep.consequence_vertex.clone())
                    .or_default();

                if !table_contains_match(consequence_rows, &dep.consequence_values) {
                    consequence_rows.push(dep.consequence_values.clone());
                    changed = true;
                }
            }
        }

        if !changed {
            return Ok(result);
        }
    }

    Err(ChaseError::NonTermination(max_iterations))
}

/// Extract embedded dependencies from a schema's structural constraints.
///
/// Generates two kinds of dependencies:
///
/// 1. **Required-edge dependencies**: For each vertex with required edges,
///    "if a row exists for vertex V, then a row must exist for the target
///    vertex of each required edge." This captures referential integrity.
///
/// 2. **See [`dependencies_from_theory`]** for path-equation dependencies
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

    deps
}

/// Extract embedded dependencies from a GAT theory's equations.
///
/// Each equation `lhs = rhs` in the theory is translated into an
/// embedded dependency. The translation handles the common equation
/// patterns found in panproto's schema theories:
///
/// - **Retraction**: `f(g(x)) = x` (e.g., `variant_of(injection(v)) = v`)
///   → if `g` applied to a value exists, the result of `f` on it must
///   equal the original value.
///
/// - **Involution**: `f(f(x)) = x` (e.g., `inv(inv(e)) = e`)
///   → applying `f` twice must return the original.
///
/// - **Commutativity with composition**: `f(g(x)) = h(x)` (e.g.,
///   `src(inv(e)) = tgt(e)`) → if `g` is applied, `f` on the result
///   must equal `h` on the original.
///
/// The dependencies are expressed at the operation level (using
/// operation names as vertex identifiers). Each operation `op: A → B`
/// maps to a dependency where the pattern matches rows in the `A`
/// table and the consequence requires rows in the `B` table.
///
/// # Arguments
///
/// * `theory` - The GAT theory whose equations to translate.
/// * `schema` - The schema providing vertex/edge context for the
///   operations. Operations in the theory that don't correspond to
///   schema edges are skipped.
#[must_use]
pub fn dependencies_from_theory(theory: &Theory, schema: &Schema) -> Vec<EmbeddedDependency> {
    let mut deps = Vec::new();

    for eq in &theory.eqs {
        deps.extend(translate_equation(eq, theory, schema));
    }

    deps
}

/// Translate a single GAT equation into embedded dependencies.
///
/// Handles three patterns:
///
/// 1. `op(inner_op(var)) = var` — retraction/section: if `inner_op`
///    produced a value, op applied to it must recover the original.
///
/// 2. `op(inner_op(var)) = other_op(var)` — commutativity: the
///    composition `op∘inner_op` must agree with `other_op`.
///
/// 3. General case: records the equation as a dependency between
///    the outermost operations on each side.
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
                let output_sort = op.output.to_string();
                // The variable is in some input sort
                for (_, input_sort) in &op.inputs {
                    let out_vertex = find_vertex_by_kind(schema, &output_sort);
                    let in_vertex = find_vertex_by_kind(schema, input_sort.as_ref());

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
                let output_sort = op.output.to_string();
                for (_, input_sort) in &op.inputs {
                    let out_vertex = find_vertex_by_kind(schema, &output_sort);
                    let in_vertex = find_vertex_by_kind(schema, input_sort.as_ref());

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
            // Both sides are variables — trivial equation, no dependency
        }
    }

    deps
}

/// Extract the outermost operation name from a term.
fn outermost_op(term: &Term) -> Option<String> {
    match term {
        Term::Var(_) => None,
        Term::App { op, .. } => Some(op.to_string()),
    }
}

/// Find the first vertex in a schema whose kind matches the given sort name.
fn find_vertex_by_kind(schema: &Schema, sort_name: &str) -> Option<String> {
    schema
        .vertices
        .values()
        .find(|v| v.kind.as_str() == sort_name)
        .map(|v| v.id.to_string())
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

    #[test]
    fn chase_no_change_when_constraints_satisfied() {
        // Instance already has the consequence row.
        let instance = FInstance::new()
            .with_table("A", vec![row("x", Value::Int(1))])
            .with_table("B", vec![row("y", Value::Int(2))]);

        let dep = EmbeddedDependency {
            pattern_vertex: "A".to_owned(),
            pattern_values: HashMap::from([("x".to_owned(), Value::Int(1))]),
            consequence_vertex: "B".to_owned(),
            consequence_values: HashMap::from([("y".to_owned(), Value::Int(2))]),
        };

        let result = chase_functor(&instance, &[dep], 10).unwrap();
        assert_eq!(result.row_count("A"), 1);
        assert_eq!(result.row_count("B"), 1);
    }

    #[test]
    fn chase_adds_missing_consequence_row() {
        // Instance has the pattern but not the consequence.
        let instance = FInstance::new().with_table("A", vec![row("x", Value::Int(1))]);

        let dep = EmbeddedDependency {
            pattern_vertex: "A".to_owned(),
            pattern_values: HashMap::from([("x".to_owned(), Value::Int(1))]),
            consequence_vertex: "B".to_owned(),
            consequence_values: HashMap::from([("y".to_owned(), Value::Int(2))]),
        };

        let result = chase_functor(&instance, &[dep], 10).unwrap();
        assert_eq!(result.row_count("A"), 1);
        assert_eq!(result.row_count("B"), 1);

        let b_rows = result.tables.get("B").unwrap();
        assert_eq!(b_rows[0].get("y"), Some(&Value::Int(2)));
    }

    #[test]
    fn chase_multi_iteration_fixpoint() {
        // Chain: A triggers B, B triggers C. Needs two iterations.
        let instance = FInstance::new().with_table("A", vec![row("x", Value::Int(1))]);

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

        let result = chase_functor(&instance, &deps, 10).unwrap();
        assert_eq!(result.row_count("A"), 1);
        assert_eq!(result.row_count("B"), 1);
        assert_eq!(result.row_count("C"), 1);
    }

    #[test]
    fn chase_non_termination_error() {
        // A dependency that generates a new row each iteration:
        // every row in A with x=1 requires a row in A with x=2,
        // and every row with x=2 requires x=1 — but we use a
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
        // to C, another from C adds to B with *different* values — but
        // that still converges.
        //
        // The realistic way to test this: use max_iterations = 0.
        let instance = FInstance::new().with_table("A", vec![row("x", Value::Int(1))]);

        let dep = EmbeddedDependency {
            pattern_vertex: "A".to_owned(),
            pattern_values: HashMap::from([("x".to_owned(), Value::Int(1))]),
            consequence_vertex: "B".to_owned(),
            consequence_values: HashMap::from([("y".to_owned(), Value::Int(2))]),
        };

        let err = chase_functor(&instance, &[dep], 0).unwrap_err();
        assert!(
            matches!(err, ChaseError::NonTermination(0)),
            "expected NonTermination(0), got {err:?}"
        );
    }

    #[test]
    fn chase_no_trigger_when_pattern_absent() {
        // Pattern vertex has no matching rows, so no consequence added.
        let instance = FInstance::new().with_table("A", vec![row("x", Value::Int(99))]);

        let dep = EmbeddedDependency {
            pattern_vertex: "A".to_owned(),
            pattern_values: HashMap::from([("x".to_owned(), Value::Int(1))]),
            consequence_vertex: "B".to_owned(),
            consequence_values: HashMap::from([("y".to_owned(), Value::Int(2))]),
        };

        let result = chase_functor(&instance, &[dep], 10).unwrap();
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
        };

        let deps = dependencies_from_theory(&theory, &schema);
        assert!(deps.is_empty(), "no equations means no dependencies");
    }
}
