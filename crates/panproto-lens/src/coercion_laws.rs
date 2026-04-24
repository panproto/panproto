//! Sample-based verification of declared coercion classes.
//!
//! A [`DirectedEquation`](panproto_gat::DirectedEquation) carries a
//! [`CoercionClass`](panproto_gat::CoercionClass) that declares the
//! round-trip fidelity of its forward (`impl_term`) and backward
//! (`inverse`) expressions. Nothing in the construction path checks
//! that the declaration is honest. The functions in this module run a
//! declared class's round-trip laws against a user-supplied set of
//! sample inputs and report any violations.
//!
//! # Laws by class
//!
//! | Class | Forward law | Backward law |
//! |---|---|---|
//! | `Iso` | `forward(inverse(v)) == v` for every sample `v` | `inverse(forward(s)) == s` for every sample `s` |
//! | `Retraction` | not required | `inverse(forward(s)) == s` (forward is a section) |
//! | `Projection` | `forward(forward(s))` stable (deterministic) | not applicable |
//! | `Opaque` | not required | not applicable |
//!
//! `Iso` and `Retraction` additionally require `inverse` to be
//! `Some(_)`; a missing inverse on either is a
//! [`CoercionLawViolation::MissingInverse`]. `Projection` and `Opaque`
//! with an inverse present is not a violation; the inverse is simply
//! not consulted.
//!
//! # Sample generation
//!
//! Samples are supplied by the caller as a slice of
//! [`panproto_expr::Literal`] values. This module does not generate
//! them; callers can use any source (a fixed fixture, a random
//! generator, values extracted from a schema's vertex carriers, etc.).
//! See [`default_samples_for_string_value`] for the simplest case.
//!
//! # Limitations
//!
//! The check is sound but incomplete: passing on all supplied samples
//! proves nothing about inputs that were not tried. Treat the result
//! as evidence, not as a proof. For `Retraction` in particular, only
//! the backward law (`inverse ∘ forward = id`) is checked; the
//! forward-injectivity side is a property of the underlying function
//! and is not sample-testable here.

use std::sync::Arc;

use panproto_expr::{Env, EvalConfig, Expr, Literal, eval};
use panproto_gat::{CoercionClass, DirectedEquation};

/// A violation of a declared coercion class's round-trip law on a
/// single sample input.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum CoercionLawViolation {
    /// The forward-then-backward law `inverse(forward(s)) == s` failed
    /// on this sample. Applies to `Iso` and `Retraction` classes.
    Backward {
        /// Input value `s`.
        input: Literal,
        /// Value of `forward(s)`.
        forward_result: Literal,
        /// Value of `inverse(forward(s))`.
        round_tripped: Literal,
    },
    /// The backward-then-forward law `forward(inverse(v)) == v` failed
    /// on this sample. Applies to `Iso` class only.
    Forward {
        /// Input value `v`.
        input: Literal,
        /// Value of `inverse(v)`.
        inverse_result: Literal,
        /// Value of `forward(inverse(v))`.
        round_tripped: Literal,
    },
    /// The forward function is not deterministic on this sample: two
    /// evaluations produced different outputs. Applies to `Projection`
    /// class.
    NonDeterministic {
        /// Input value.
        input: Literal,
        /// First evaluation's result.
        first: Literal,
        /// Second evaluation's result.
        second: Literal,
    },
    /// The declared class requires an inverse (`Iso` or `Retraction`)
    /// but no inverse expression is available.
    MissingInverse {
        /// The declared class.
        class: CoercionClass,
    },
    /// Evaluating the forward expression returned an error on this
    /// sample. Typically a type mismatch or an unbound variable inside
    /// `forward`.
    ForwardEvalError {
        /// Input value.
        input: Literal,
        /// Stringified error.
        error: String,
    },
    /// Evaluating the inverse expression returned an error on this
    /// sample. Typically a type mismatch between `forward`'s output
    /// and `inverse`'s expected input.
    InverseEvalError {
        /// Input value fed into `forward` (for backward law) or the
        /// sample itself (for forward law).
        input: Literal,
        /// Stringified error.
        error: String,
    },
    /// A coercion class variant the checker does not recognise (added
    /// to the upstream `CoercionClass` enum after this module was
    /// last updated). Returned once per invocation rather than per
    /// sample to keep the output concise.
    UnknownClass {
        /// The unrecognised class.
        class: CoercionClass,
    },
}

/// Check a forward / inverse expression pair against the round-trip
/// laws of a declared [`CoercionClass`].
///
/// The `var_name` parameter is the name under which each sample is
/// bound in the evaluation environment. For `ApplyExpr`-style
/// transforms this is the field key; for standalone coercions, any
/// name that appears as a free variable in `forward` and `inverse`.
///
/// Returns one [`CoercionLawViolation`] per failing sample. An empty
/// vector means all supplied samples satisfy the declared laws; it
/// does not prove the declaration holds in general.
#[must_use]
pub fn check_coercion_laws(
    forward: &Expr,
    inverse: Option<&Expr>,
    class: CoercionClass,
    samples: &[Literal],
    var_name: &str,
) -> Vec<CoercionLawViolation> {
    let var: Arc<str> = Arc::from(var_name);
    let config = EvalConfig::default();
    let mut violations = Vec::new();

    match class {
        CoercionClass::Iso => {
            let Some(inv) = inverse else {
                violations.push(CoercionLawViolation::MissingInverse { class });
                return violations;
            };
            for sample in samples {
                check_backward(forward, inv, sample, &var, &config, &mut violations);
                check_forward(forward, inv, sample, &var, &config, &mut violations);
            }
        }
        CoercionClass::Retraction => {
            let Some(inv) = inverse else {
                violations.push(CoercionLawViolation::MissingInverse { class });
                return violations;
            };
            for sample in samples {
                check_backward(forward, inv, sample, &var, &config, &mut violations);
            }
        }
        CoercionClass::Projection => {
            for sample in samples {
                check_deterministic(forward, sample, &var, &config, &mut violations);
            }
        }
        CoercionClass::Opaque => {
            // Opaque makes no round-trip claim; nothing to verify.
        }
        other => {
            // A class variant the checker does not recognise.
            // Surface it once rather than silently passing so a
            // future class addition cannot masquerade as Opaque.
            violations.push(CoercionLawViolation::UnknownClass { class: other });
        }
    }

    violations
}

/// Check the round-trip laws of `deq`'s declared coercion class using
/// samples that bind the supplied key as the input variable.
///
/// If `deq.inverse` is `None`, the class is expected to be `Opaque`
/// or `Projection` (which do not consult the inverse). If the class
/// is `Iso` or `Retraction` without an inverse, a
/// [`CoercionLawViolation::MissingInverse`] is reported.
#[must_use]
pub fn check_directed_equation_coercion_law(
    deq: &DirectedEquation,
    samples: &[Literal],
    var_name: &str,
) -> Vec<CoercionLawViolation> {
    check_coercion_laws(
        &deq.impl_term,
        deq.inverse.as_ref(),
        deq.coercion_class,
        samples,
        var_name,
    )
}

/// Default sample set for string-valued coercions.
///
/// Covers the empty string, a lowercase ASCII identifier, a
/// mixed-case name, an all-uppercase string, a string containing
/// whitespace, and a short Unicode string. Useful as a sanity-check
/// when no domain-specific samples are available.
#[must_use]
pub fn default_samples_for_string_value() -> Vec<Literal> {
    vec![
        Literal::Str(String::new()),
        Literal::Str("name".to_owned()),
        Literal::Str("Alice".to_owned()),
        Literal::Str("ALICE".to_owned()),
        Literal::Str("hello world".to_owned()),
        Literal::Str("schön".to_owned()),
    ]
}

fn check_backward(
    forward: &Expr,
    inverse: &Expr,
    sample: &Literal,
    var: &Arc<str>,
    config: &EvalConfig,
    violations: &mut Vec<CoercionLawViolation>,
) {
    let env = Env::new().extend(Arc::clone(var), sample.clone());
    let forward_result = match eval(forward, &env, config) {
        Ok(v) => v,
        Err(e) => {
            violations.push(CoercionLawViolation::ForwardEvalError {
                input: sample.clone(),
                error: e.to_string(),
            });
            return;
        }
    };
    let inverse_env = Env::new().extend(Arc::clone(var), forward_result.clone());
    match eval(inverse, &inverse_env, config) {
        Ok(round_tripped) => {
            if round_tripped != *sample {
                violations.push(CoercionLawViolation::Backward {
                    input: sample.clone(),
                    forward_result,
                    round_tripped,
                });
            }
        }
        Err(e) => {
            violations.push(CoercionLawViolation::InverseEvalError {
                input: sample.clone(),
                error: e.to_string(),
            });
        }
    }
}

fn check_forward(
    forward: &Expr,
    inverse: &Expr,
    sample: &Literal,
    var: &Arc<str>,
    config: &EvalConfig,
    violations: &mut Vec<CoercionLawViolation>,
) {
    let env = Env::new().extend(Arc::clone(var), sample.clone());
    let inverse_result = match eval(inverse, &env, config) {
        Ok(v) => v,
        Err(e) => {
            violations.push(CoercionLawViolation::InverseEvalError {
                input: sample.clone(),
                error: e.to_string(),
            });
            return;
        }
    };
    let forward_env = Env::new().extend(Arc::clone(var), inverse_result.clone());
    match eval(forward, &forward_env, config) {
        Ok(round_tripped) => {
            if round_tripped != *sample {
                violations.push(CoercionLawViolation::Forward {
                    input: sample.clone(),
                    inverse_result,
                    round_tripped,
                });
            }
        }
        Err(e) => {
            violations.push(CoercionLawViolation::ForwardEvalError {
                input: sample.clone(),
                error: e.to_string(),
            });
        }
    }
}

fn check_deterministic(
    forward: &Expr,
    sample: &Literal,
    var: &Arc<str>,
    config: &EvalConfig,
    violations: &mut Vec<CoercionLawViolation>,
) {
    let env = Env::new().extend(Arc::clone(var), sample.clone());
    let first = match eval(forward, &env, config) {
        Ok(v) => v,
        Err(e) => {
            violations.push(CoercionLawViolation::ForwardEvalError {
                input: sample.clone(),
                error: e.to_string(),
            });
            return;
        }
    };
    let second = match eval(forward, &env, config) {
        Ok(v) => v,
        Err(e) => {
            violations.push(CoercionLawViolation::ForwardEvalError {
                input: sample.clone(),
                error: e.to_string(),
            });
            return;
        }
    };
    if first != second {
        violations.push(CoercionLawViolation::NonDeterministic {
            input: sample.clone(),
            first,
            second,
        });
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_expr::{BuiltinOp, Expr};

    /// `upper x` coerces a string to uppercase.
    fn upper_expr(var: &str) -> Expr {
        Expr::Builtin(BuiltinOp::Upper, vec![Expr::Var(Arc::from(var))])
    }

    /// Identity expression `x` (a pure pass-through).
    fn identity_expr(var: &str) -> Expr {
        Expr::Var(Arc::from(var))
    }

    #[test]
    fn iso_with_honest_identity_passes() {
        // identity forward, identity inverse, declared Iso: both
        // round trips hold for every sample.
        let violations = check_coercion_laws(
            &identity_expr("x"),
            Some(&identity_expr("x")),
            CoercionClass::Iso,
            &default_samples_for_string_value(),
            "x",
        );
        assert!(
            violations.is_empty(),
            "honest identity iso must have no violations, got {violations:?}"
        );
    }

    #[test]
    fn iso_with_lying_identity_inverse_is_flagged() {
        // upper forward, identity inverse, declared Iso: backward law
        // holds only when the input was already uppercase; forward
        // law never holds because inverse(v) = v and forward(v) =
        // upper(v) differ whenever v has lowercase content.
        let forward = upper_expr("x");
        let inverse = identity_expr("x");
        let violations = check_coercion_laws(
            &forward,
            Some(&inverse),
            CoercionClass::Iso,
            &[Literal::Str("Alice".to_owned())],
            "x",
        );
        assert!(
            !violations.is_empty(),
            "lying iso declaration must be flagged",
        );
        // Both directions should surface: upper("Alice") = "ALICE",
        // identity("ALICE") = "ALICE" != "Alice" for backward;
        // identity("Alice") = "Alice", upper("Alice") = "ALICE" !=
        // "Alice" for forward.
        let has_backward = violations
            .iter()
            .any(|v| matches!(v, CoercionLawViolation::Backward { .. }));
        let has_forward = violations
            .iter()
            .any(|v| matches!(v, CoercionLawViolation::Forward { .. }));
        assert!(
            has_backward,
            "expected Backward violation in {violations:?}"
        );
        assert!(has_forward, "expected Forward violation in {violations:?}");
    }

    #[test]
    fn retraction_checks_only_backward_direction() {
        // upper + lower: upper(lower(s)) = upper(s), not always s;
        // but lower(upper(s)) = lower(s), not always s either.
        // Retraction requires only inverse(forward(s)) = s; here we
        // use `upper` as forward, `lower` as inverse, and note that
        // `lower(upper("Alice")) = "alice" != "Alice"`, so the
        // backward law fails.
        let forward = upper_expr("x");
        let inverse = Expr::Builtin(BuiltinOp::Lower, vec![Expr::Var(Arc::from("x"))]);
        let violations = check_coercion_laws(
            &forward,
            Some(&inverse),
            CoercionClass::Retraction,
            &[Literal::Str("Alice".to_owned())],
            "x",
        );
        assert!(
            violations
                .iter()
                .any(|v| matches!(v, CoercionLawViolation::Backward { .. })),
            "retraction backward violation expected, got {violations:?}"
        );
    }

    #[test]
    fn projection_checks_determinism() {
        // A pure upper expression is deterministic: repeated
        // evaluations produce the same result, so Projection
        // validates.
        let violations = check_coercion_laws(
            &upper_expr("x"),
            None,
            CoercionClass::Projection,
            &default_samples_for_string_value(),
            "x",
        );
        assert!(
            violations.is_empty(),
            "deterministic projection must pass, got {violations:?}"
        );
    }

    #[test]
    fn opaque_declares_no_law_so_always_passes() {
        let violations = check_coercion_laws(
            &upper_expr("x"),
            None,
            CoercionClass::Opaque,
            &default_samples_for_string_value(),
            "x",
        );
        assert!(violations.is_empty(), "opaque has no laws to violate");
    }

    #[test]
    fn iso_without_inverse_reports_missing_inverse() {
        let violations = check_coercion_laws(
            &upper_expr("x"),
            None,
            CoercionClass::Iso,
            &default_samples_for_string_value(),
            "x",
        );
        assert_eq!(violations.len(), 1);
        assert!(matches!(
            violations[0],
            CoercionLawViolation::MissingInverse {
                class: CoercionClass::Iso,
            }
        ));
    }

    #[test]
    fn retraction_without_inverse_reports_missing_inverse() {
        let violations = check_coercion_laws(
            &upper_expr("x"),
            None,
            CoercionClass::Retraction,
            &default_samples_for_string_value(),
            "x",
        );
        assert_eq!(violations.len(), 1);
        assert!(matches!(
            violations[0],
            CoercionLawViolation::MissingInverse {
                class: CoercionClass::Retraction,
            }
        ));
    }

    #[test]
    fn check_directed_equation_matches_explicit_call() {
        // Shortcut delegates to check_coercion_laws with the same
        // arguments; verify parity.
        let forward = upper_expr("x");
        let deq = DirectedEquation {
            name: Arc::from("upper_iso_lying"),
            lhs: panproto_gat::Term::var("x"),
            rhs: panproto_gat::Term::app("upper", vec![panproto_gat::Term::var("x")]),
            impl_term: forward.clone(),
            inverse: Some(identity_expr("x")),
            source_kind: Some(panproto_gat::ValueKind::Str),
            target_kind: Some(panproto_gat::ValueKind::Str),
            coercion_class: CoercionClass::Iso,
        };
        let samples = vec![Literal::Str("Alice".to_owned())];
        let direct = check_coercion_laws(
            &forward,
            Some(&identity_expr("x")),
            CoercionClass::Iso,
            &samples,
            "x",
        );
        let via_deq = check_directed_equation_coercion_law(&deq, &samples, "x");
        assert_eq!(direct.len(), via_deq.len());
    }

    #[test]
    fn eval_error_on_wrong_type_is_reported() {
        // upper on an integer sample: evaluator surfaces a type
        // error; the check reports it rather than silently passing.
        let violations = check_coercion_laws(
            &upper_expr("x"),
            None,
            CoercionClass::Projection,
            &[Literal::Int(42)],
            "x",
        );
        assert!(
            violations
                .iter()
                .any(|v| matches!(v, CoercionLawViolation::ForwardEvalError { .. })),
            "expected ForwardEvalError, got {violations:?}"
        );
    }
}
