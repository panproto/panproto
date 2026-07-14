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
/// Each equation is checked by enumerating every variable assignment
/// drawn from the carrier sets of the equation's variables and comparing
/// the two sides. The enumeration is bounded: per equation, at most
/// [`CheckModelOptions::max_assignments`] assignments are considered
/// (default 10,000). This function uses that default.
///
/// The check is exhaustive over the equation's finite carrier product
/// whenever that product fits within the bound, so an empty violation
/// list from a completed check is a proof of satisfaction over the given
/// carriers. An equation whose carrier product exceeds the bound is
/// *not* checked; the whole call then returns
/// [`GatError::ModelCheckLimitExceeded`] naming that equation rather than
/// silently returning an empty (and unproven) violation list. Callers
/// that treat any error as "check skipped" must therefore not read an
/// error as a passing check.
///
/// Returns a list of violations (empty means every equation was checked
/// exhaustively and holds).
///
/// # Errors
///
/// Returns [`GatError::ModelCheckLimitExceeded`] if any equation's
/// assignment count exceeds the default per-equation bound, or
/// [`GatError`] if variable sorts cannot be inferred or a carrier set is
/// missing from the model.
pub fn check_model(model: &Model, theory: &Theory) -> Result<Vec<EquationViolation>, GatError> {
    check_model_with_options(model, theory, &CheckModelOptions::default())
}

/// Check with configurable options.
///
/// The per-equation assignment bound is [`CheckModelOptions::max_assignments`]
/// (set to 0 to disable the bound). Within the bound the enumeration is
/// exhaustive over the finite carrier product, so an empty violation list
/// distinguishes a completed, exhaustive check from a truncated one: a
/// truncated equation raises [`GatError::ModelCheckLimitExceeded`] rather
/// than contributing an empty result.
///
/// # Errors
///
/// Returns [`GatError::ModelCheckLimitExceeded`] if an equation's
/// assignment count exceeds `options.max_assignments`, or other errors
/// from type inference.
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
            let head = sort.head();
            let carrier = model
                .sort_interp
                .get(head.as_ref())
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
        return Err(GatError::ModelCheckLimitExceeded {
            equation: eq.name.to_string(),
            required: total,
            limit: options.max_assignments,
        });
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

        Term::Case {
            scrutinee,
            branches,
        } => {
            // Evaluate the scrutinee, then select the branch whose
            // constructor matches the scrutinee's tag, bind that branch's
            // pattern variables to the constructor arguments, and evaluate
            // the branch body. This requires the scrutinee to reduce to a
            // constructor-tagged value; a value of any other shape cannot
            // be pattern-matched.
            let scrutinee_val = eval_term(scrutinee, assignment, model)?;
            let ModelValue::Constructor { tag, args } = &scrutinee_val else {
                return Err(GatError::ModelError(format!(
                    "case scrutinee did not evaluate to a constructor value: {scrutinee_val:?}"
                )));
            };
            let branch = branches
                .iter()
                .find(|b| b.constructor.as_ref() == tag.as_str())
                .ok_or_else(|| {
                    GatError::ModelError(format!(
                        "case scrutinee has constructor '{tag}' with no matching branch"
                    ))
                })?;
            if branch.binders.len() != args.len() {
                return Err(GatError::ModelError(format!(
                    "case branch for constructor '{tag}' binds {} name(s) but the value carries {} argument(s)",
                    branch.binders.len(),
                    args.len()
                )));
            }
            let mut extended = assignment.clone();
            for (binder, arg) in branch.binders.iter().zip(args.iter()) {
                extended.insert(Arc::clone(binder), arg.clone());
            }
            eval_term(&branch.body, &extended, model)
        }

        Term::Hole { .. } => Err(GatError::ModelError(
            "typed holes cannot be evaluated in a set-theoretic model".to_string(),
        )),
        Term::Let { name, bound, body } => {
            let v = eval_term(bound, assignment, model)?;
            let mut extended = assignment.clone();
            extended.insert(Arc::clone(name), v);
            eval_term(body, &extended, model)
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
        // The truncation is surfaced as a structured error naming the
        // equation and the bound, not swallowed into an empty pass.
        match result {
            Err(GatError::ModelCheckLimitExceeded {
                equation,
                required,
                limit,
            }) => {
                assert_eq!(&*equation, "assoc");
                assert!(required > limit);
                assert_eq!(limit, 100);
            }
            other => panic!("expected ModelCheckLimitExceeded, got {other:?}"),
        }
    }

    #[test]
    fn missing_carrier_set_errors() {
        let theory = monoid_theory();
        let model = Model::new("Monoid");
        // No carrier set added; should error.
        let result = check_model(&model, &theory);
        assert!(matches!(result, Err(GatError::ModelError(_))));
    }

    /// Theory of a boolean-like closed sort `B` with two nullary
    /// constructors and an operation `neg` whose defining equation uses a
    /// case term: `neg(b) = case b of tt() => ff() | ff() => tt()`.
    fn negation_theory() -> Theory {
        use crate::eq::CaseBranch;
        use crate::sort::Sort;

        let sort_b = Sort::closed("B", Vec::new(), ["tt", "ff"]);
        let case_term = Term::Case {
            scrutinee: Box::new(Term::var("b")),
            branches: vec![
                CaseBranch {
                    constructor: "tt".into(),
                    binders: Vec::new(),
                    body: Term::constant("ff"),
                },
                CaseBranch {
                    constructor: "ff".into(),
                    binders: Vec::new(),
                    body: Term::constant("tt"),
                },
            ],
        };
        Theory::new(
            "Negation",
            vec![sort_b],
            vec![
                Operation::nullary("tt", "B"),
                Operation::nullary("ff", "B"),
                Operation::unary("neg", "b", "B", "B"),
            ],
            vec![Equation::new(
                "neg_def",
                Term::app("neg", vec![Term::var("b")]),
                case_term,
            )],
        )
    }

    fn tt_value() -> ModelValue {
        ModelValue::Constructor {
            tag: "tt".to_owned(),
            args: Vec::new(),
        }
    }

    fn ff_value() -> ModelValue {
        ModelValue::Constructor {
            tag: "ff".to_owned(),
            args: Vec::new(),
        }
    }

    #[test]
    fn case_equation_satisfied_model_passes() -> Result<(), Box<dyn std::error::Error>> {
        let theory = negation_theory();

        let mut model = Model::new("Negation");
        model.add_sort("B", vec![tt_value(), ff_value()]);
        model.add_op("tt", |_: &[ModelValue]| Ok(tt_value()));
        model.add_op("ff", |_: &[ModelValue]| Ok(ff_value()));
        // A faithful negation: tt maps to ff and ff maps to tt, matching
        // the case term on the right-hand side of the equation.
        model.add_op("neg", |args: &[ModelValue]| match &args[0] {
            ModelValue::Constructor { tag, .. } if tag == "tt" => Ok(ff_value()),
            ModelValue::Constructor { tag, .. } if tag == "ff" => Ok(tt_value()),
            other => Err(GatError::ModelError(format!("neg: unexpected {other:?}"))),
        });

        let violations = check_model(&model, &theory)?;
        assert!(
            violations.is_empty(),
            "faithful negation satisfies the case equation, got {violations:?}"
        );
        Ok(())
    }

    #[test]
    fn case_equation_violation_detected() -> Result<(), Box<dyn std::error::Error>> {
        let theory = negation_theory();

        let mut model = Model::new("Negation");
        model.add_sort("B", vec![tt_value(), ff_value()]);
        model.add_op("tt", |_: &[ModelValue]| Ok(tt_value()));
        model.add_op("ff", |_: &[ModelValue]| Ok(ff_value()));
        // A broken negation that returns its argument unchanged: neg(b)
        // then disagrees with the case term, which flips the tag.
        model.add_op("neg", |args: &[ModelValue]| Ok(args[0].clone()));

        let violations = check_model(&model, &theory)?;
        assert!(
            violations.iter().any(|v| v.equation.as_ref() == "neg_def"),
            "identity negation must violate the case equation, got {violations:?}"
        );
        Ok(())
    }
}
