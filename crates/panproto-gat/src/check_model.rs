//! Model equation satisfaction checking.
//!
//! Verifies that a [`Model`] satisfies all equations of its [`Theory`]
//! by enumerating variable assignments from carrier sets and evaluating
//! both sides.

use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::eq::{Equation, Term};
use crate::error::GatError;
use crate::model::{Model, ModelValue};
use crate::theory::Theory;
use crate::typecheck::infer_var_sorts;

/// A single violation of an equation in a model.
#[derive(Debug, Clone)]
pub struct EquationViolation {
    /// The name of the violated equation.
    pub equation: Arc<str>,
    /// The variable assignment that produced the violation.
    pub assignment: FxHashMap<Arc<str>, ModelValue>,
    /// The value the LHS evaluated to.
    pub lhs_value: ModelValue,
    /// The value the RHS evaluated to.
    pub rhs_value: ModelValue,
}

/// Options for model checking.
#[derive(Debug, Clone)]
pub struct CheckModelOptions {
    /// Maximum number of assignments to enumerate per equation.
    /// Set to 0 for unlimited. Default: 10,000.
    pub max_assignments: usize,
}

impl Default for CheckModelOptions {
    fn default() -> Self {
        Self {
            max_assignments: 10_000,
        }
    }
}

/// Check whether a model satisfies all equations of its theory.
///
/// Returns a list of violations (empty means the model is valid).
///
/// # Errors
///
/// Returns [`GatError`] if variable sorts cannot be inferred or a carrier
/// set is missing from the model.
pub fn check_model(model: &Model, theory: &Theory) -> Result<Vec<EquationViolation>, GatError> {
    check_model_with_options(model, theory, &CheckModelOptions::default())
}

/// Check with configurable options.
///
/// # Errors
///
/// Returns [`GatError::ModelError`] if the assignment count exceeds
/// `options.max_assignments`, or other errors from type inference.
pub fn check_model_with_options(
    model: &Model,
    theory: &Theory,
    options: &CheckModelOptions,
) -> Result<Vec<EquationViolation>, GatError> {
    let mut violations = Vec::new();

    for eq in &theory.eqs {
        let eq_violations = check_equation(model, eq, theory, options)?;
        violations.extend(eq_violations);
    }

    Ok(violations)
}

/// Check a single equation against all valid variable assignments.
fn check_equation(
    model: &Model,
    eq: &Equation,
    theory: &Theory,
    options: &CheckModelOptions,
) -> Result<Vec<EquationViolation>, GatError> {
    let var_sorts = infer_var_sorts(eq, theory)?;

    // Build ordered list of (var_name, carrier_set) pairs.
    let var_carriers: Vec<(Arc<str>, &[ModelValue])> = var_sorts
        .iter()
        .map(|(var, sort)| {
            let carrier = model
                .sort_interp
                .get(sort.as_ref())
                .ok_or_else(|| GatError::ModelError(format!("no carrier set for sort '{sort}'")))?;
            Ok((Arc::clone(var), carrier.as_slice()))
        })
        .collect::<Result<Vec<_>, GatError>>()?;

    // If any carrier is empty, there are zero valid assignments.
    if var_carriers.iter().any(|(_, carrier)| carrier.is_empty()) {
        return Ok(vec![]);
    }

    // Handle the zero-variable case: one assignment (the empty one).
    if var_carriers.is_empty() {
        let assignment = FxHashMap::default();
        let lhs_val = eval_term(&eq.lhs, &assignment, model)?;
        let rhs_val = eval_term(&eq.rhs, &assignment, model)?;
        if lhs_val != rhs_val {
            return Ok(vec![EquationViolation {
                equation: Arc::clone(&eq.name),
                assignment,
                lhs_value: lhs_val,
                rhs_value: rhs_val,
            }]);
        }
        return Ok(vec![]);
    }

    // Compute total assignment count for limit check.
    let total: usize = var_carriers
        .iter()
        .map(|(_, carrier)| carrier.len())
        .try_fold(1usize, usize::checked_mul)
        .unwrap_or(usize::MAX);

    if options.max_assignments > 0 && total > options.max_assignments {
        return Err(GatError::ModelError(format!(
            "equation '{}' requires {total} assignments, exceeding limit {}",
            eq.name, options.max_assignments
        )));
    }

    let mut violations = Vec::new();
    let mut indices = vec![0usize; var_carriers.len()];

    loop {
        // Build current assignment.
        let assignment: FxHashMap<Arc<str>, ModelValue> = var_carriers
            .iter()
            .zip(indices.iter())
            .map(|((var, carrier), &idx)| (Arc::clone(var), carrier[idx].clone()))
            .collect();

        // Evaluate both sides.
        let lhs_val = eval_term(&eq.lhs, &assignment, model)?;
        let rhs_val = eval_term(&eq.rhs, &assignment, model)?;

        if lhs_val != rhs_val {
            violations.push(EquationViolation {
                equation: Arc::clone(&eq.name),
                assignment,
                lhs_value: lhs_val,
                rhs_value: rhs_val,
            });
        }

        // Increment indices (odometer-style).
        if !increment_indices(&mut indices, &var_carriers) {
            break;
        }
    }

    Ok(violations)
}

/// Evaluate a term under a variable-to-ModelValue assignment.
fn eval_term(
    term: &Term,
    assignment: &FxHashMap<Arc<str>, ModelValue>,
    model: &Model,
) -> Result<ModelValue, GatError> {
    match term {
        Term::Var(name) => assignment
            .get(name)
            .cloned()
            .ok_or_else(|| GatError::ModelError(format!("variable '{name}' not in assignment"))),

        Term::App { op, args } => {
            let arg_vals: Vec<ModelValue> = args
                .iter()
                .map(|a| eval_term(a, assignment, model))
                .collect::<Result<Vec<_>, _>>()?;
            model.eval(op, &arg_vals)
        }
    }
}

/// Odometer-style increment. Returns `false` when all combinations are exhausted.
fn increment_indices(indices: &mut [usize], var_carriers: &[(Arc<str>, &[ModelValue])]) -> bool {
    for i in (0..indices.len()).rev() {
        indices[i] += 1;
        if indices[i] < var_carriers[i].1.len() {
            return true;
        }
        indices[i] = 0;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eq::Equation;
    use crate::model::Model;
    use crate::op::Operation;
    use crate::sort::Sort;
    use crate::theory::Theory;

    fn monoid_theory() -> Theory {
        Theory::new(
            "Monoid",
            vec![Sort::simple("Carrier")],
            vec![
                Operation::new(
                    "mul",
                    vec![
                        ("a".into(), "Carrier".into()),
                        ("b".into(), "Carrier".into()),
                    ],
                    "Carrier",
                ),
                Operation::nullary("unit", "Carrier"),
            ],
            vec![
                Equation::new(
                    "assoc",
                    Term::app(
                        "mul",
                        vec![
                            Term::var("a"),
                            Term::app("mul", vec![Term::var("b"), Term::var("c")]),
                        ],
                    ),
                    Term::app(
                        "mul",
                        vec![
                            Term::app("mul", vec![Term::var("a"), Term::var("b")]),
                            Term::var("c"),
                        ],
                    ),
                ),
                Equation::new(
                    "left_id",
                    Term::app("mul", vec![Term::constant("unit"), Term::var("a")]),
                    Term::var("a"),
                ),
                Equation::new(
                    "right_id",
                    Term::app("mul", vec![Term::var("a"), Term::constant("unit")]),
                    Term::var("a"),
                ),
            ],
        )
    }

    fn valid_z5_model() -> Model {
        let mut model = Model::new("Monoid");
        model.add_sort("Carrier", (0..5).map(ModelValue::Int).collect());
        model.add_op("mul", |args: &[ModelValue]| match (&args[0], &args[1]) {
            (ModelValue::Int(a), ModelValue::Int(b)) => Ok(ModelValue::Int((a + b) % 5)),
            _ => Err(GatError::ModelError("expected Int".into())),
        });
        model.add_op("unit", |_: &[ModelValue]| Ok(ModelValue::Int(0)));
        model
    }

    #[test]
    fn valid_model_passes() -> Result<(), Box<dyn std::error::Error>> {
        let theory = monoid_theory();
        let model = valid_z5_model();
        let violations = check_model(&model, &theory)?;
        assert!(
            violations.is_empty(),
            "expected no violations, got {violations:?}"
        );
        Ok(())
    }

    #[test]
    fn broken_identity_detected() -> Result<(), Box<dyn std::error::Error>> {
        let theory = monoid_theory();
        let mut model = valid_z5_model();
        // Break right identity: unit() returns 1 instead of 0.
        model.add_op("unit", |_: &[ModelValue]| Ok(ModelValue::Int(1)));

        let violations = check_model(&model, &theory)?;
        assert!(!violations.is_empty(), "expected violations");

        // At least one violation should be for right_id or left_id.
        let has_identity_violation = violations
            .iter()
            .any(|v| v.equation.as_ref() == "left_id" || v.equation.as_ref() == "right_id");
        assert!(has_identity_violation);
        Ok(())
    }

    #[test]
    fn broken_associativity_detected() -> Result<(), Box<dyn std::error::Error>> {
        let theory = monoid_theory();
        let mut model = Model::new("Monoid");
        model.add_sort(
            "Carrier",
            vec![ModelValue::Int(0), ModelValue::Int(1), ModelValue::Int(2)],
        );
        // Non-associative: saturating subtraction (a - b, clamped to 0).
        model.add_op("mul", |args: &[ModelValue]| match (&args[0], &args[1]) {
            (ModelValue::Int(a), ModelValue::Int(b)) => Ok(ModelValue::Int((*a - *b).max(0))),
            _ => Err(GatError::ModelError("expected Int".into())),
        });
        model.add_op("unit", |_: &[ModelValue]| Ok(ModelValue::Int(0)));

        let violations = check_model(&model, &theory)?;
        let has_assoc = violations.iter().any(|v| v.equation.as_ref() == "assoc");
        assert!(has_assoc, "expected associativity violation");
        Ok(())
    }

    #[test]
    fn empty_carrier_passes() -> Result<(), Box<dyn std::error::Error>> {
        let theory = monoid_theory();
        let mut model = Model::new("Monoid");
        model.add_sort("Carrier", vec![]);
        model.add_op("mul", |_: &[ModelValue]| {
            Err(GatError::ModelError("unreachable".into()))
        });
        model.add_op("unit", |_: &[ModelValue]| Ok(ModelValue::Int(0)));

        // With empty carrier, only constants-only equations are checked.
        // left_id and right_id have variables, so 0 assignments for those.
        // But unit() = unit() would be checked if it existed.
        // assoc also has variables so 0 assignments.
        let violations = check_model(&model, &theory)?;
        assert!(violations.is_empty());
        Ok(())
    }

    #[test]
    fn constants_only_equation() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new(
            "T",
            vec![Sort::simple("S")],
            vec![Operation::nullary("a", "S"), Operation::nullary("b", "S")],
            vec![Equation::new(
                "a_eq_b",
                Term::constant("a"),
                Term::constant("b"),
            )],
        );

        // Model where a() = b() = 0: passes.
        let mut model = Model::new("T");
        model.add_sort("S", vec![ModelValue::Int(0)]);
        model.add_op("a", |_: &[ModelValue]| Ok(ModelValue::Int(0)));
        model.add_op("b", |_: &[ModelValue]| Ok(ModelValue::Int(0)));
        let violations = check_model(&model, &theory)?;
        assert!(violations.is_empty());

        // Model where a() = 0, b() = 1: fails.
        model.add_op("b", |_: &[ModelValue]| Ok(ModelValue::Int(1)));
        let violations = check_model(&model, &theory)?;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].equation.as_ref(), "a_eq_b");
        Ok(())
    }

    #[test]
    fn assignment_limit_exceeded() {
        let theory = monoid_theory();
        let mut model = Model::new("Monoid");
        // Large carrier set: 100 elements, assoc has 3 variables -> 1M assignments.
        model.add_sort("Carrier", (0..100).map(ModelValue::Int).collect());
        model.add_op("mul", |args: &[ModelValue]| match (&args[0], &args[1]) {
            (ModelValue::Int(a), ModelValue::Int(b)) => Ok(ModelValue::Int(a + b)),
            _ => Err(GatError::ModelError("expected Int".into())),
        });
        model.add_op("unit", |_: &[ModelValue]| Ok(ModelValue::Int(0)));

        let options = CheckModelOptions {
            max_assignments: 100,
        };
        let result = check_model_with_options(&model, &theory, &options);
        assert!(matches!(result, Err(GatError::ModelError(_))));
    }

    #[test]
    fn missing_carrier_set_errors() {
        let theory = monoid_theory();
        let model = Model::new("Monoid");
        // No carrier set added; should error.
        let result = check_model(&model, &theory);
        assert!(matches!(result, Err(GatError::ModelError(_))));
    }
}
