//! Term type-checking for GAT expressions.
//!
//! Verifies that terms are well-typed with respect to a theory's operation
//! signatures. This catches malformed equations, unsound morphisms, and
//! invalid term constructions at construction time rather than silently
//! producing wrong results downstream.

use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::eq::{Equation, Term};
use crate::error::GatError;
use crate::theory::Theory;

/// A variable typing context: maps variable names to their sort names.
pub type VarContext = FxHashMap<Arc<str>, Arc<str>>;

/// Infer the output sort of a term given a variable context and theory.
///
/// For `Var(x)`: returns `ctx[x]` or [`GatError::UnboundVariable`].
/// For `App { op, args }`: looks up `op` in the theory, recursively
/// typechecks each argument, verifies sorts match the operation's
/// input signature, and returns the output sort.
///
/// # Errors
///
/// Returns an error if:
/// - A variable is not in the context ([`GatError::UnboundVariable`])
/// - An operation is not in the theory ([`GatError::OpNotFound`])
/// - Argument count doesn't match ([`GatError::TermArityMismatch`])
/// - An argument's sort doesn't match the expected input sort ([`GatError::ArgTypeMismatch`])
pub fn typecheck_term(
    term: &Term,
    ctx: &VarContext,
    theory: &Theory,
) -> Result<Arc<str>, GatError> {
    match term {
        Term::Var(name) => ctx
            .get(name)
            .cloned()
            .ok_or_else(|| GatError::UnboundVariable(name.to_string())),

        Term::App { op, args } => {
            let operation = theory
                .find_op(op)
                .ok_or_else(|| GatError::OpNotFound(op.to_string()))?;

            if args.len() != operation.inputs.len() {
                return Err(GatError::TermArityMismatch {
                    op: op.to_string(),
                    expected: operation.inputs.len(),
                    got: args.len(),
                });
            }

            for (i, (arg, (_, expected_sort))) in
                args.iter().zip(operation.inputs.iter()).enumerate()
            {
                let arg_sort = typecheck_term(arg, ctx, theory)?;
                if arg_sort != *expected_sort {
                    return Err(GatError::ArgTypeMismatch {
                        op: op.to_string(),
                        arg_index: i,
                        expected: expected_sort.to_string(),
                        got: arg_sort.to_string(),
                    });
                }
            }

            Ok(Arc::clone(&operation.output))
        }
    }
}

/// Infer variable sorts from an equation's term structure.
///
/// Walks both sides of the equation, collecting sort constraints from
/// every operation application site. A variable used as argument `i`
/// of operation `op` must have the sort declared for that input.
///
/// # Errors
///
/// Returns [`GatError::ConflictingVarSort`] if a variable appears at
/// two positions requiring different sorts, or [`GatError::OpNotFound`]
/// if a referenced operation doesn't exist in the theory.
pub fn infer_var_sorts(eq: &Equation, theory: &Theory) -> Result<VarContext, GatError> {
    let mut ctx = VarContext::default();
    collect_constraints(&eq.lhs, theory, &mut ctx)?;
    collect_constraints(&eq.rhs, theory, &mut ctx)?;
    Ok(ctx)
}

/// Recursive helper: walk a term and constrain each variable argument
/// to the expected input sort of its enclosing operation.
fn collect_constraints(term: &Term, theory: &Theory, ctx: &mut VarContext) -> Result<(), GatError> {
    if let Term::App { op, args } = term {
        let operation = theory
            .find_op(op)
            .ok_or_else(|| GatError::OpNotFound(op.to_string()))?;

        for (arg, (_, expected_sort)) in args.iter().zip(operation.inputs.iter()) {
            match arg {
                Term::Var(var_name) => {
                    if let Some(existing) = ctx.get(var_name) {
                        if existing != expected_sort {
                            return Err(GatError::ConflictingVarSort {
                                var: var_name.to_string(),
                                sort1: existing.to_string(),
                                sort2: expected_sort.to_string(),
                            });
                        }
                    } else {
                        ctx.insert(Arc::clone(var_name), Arc::clone(expected_sort));
                    }
                }
                Term::App { .. } => {
                    // Recurse into nested applications — the variable constraints
                    // come from the inner operations' input signatures.
                    collect_constraints(arg, theory, ctx)?;
                }
            }
        }
    }
    Ok(())
}

/// Typecheck an equation: infer variable sorts, typecheck both sides,
/// verify they produce the same output sort.
///
/// # Errors
///
/// Returns [`GatError::EquationSortMismatch`] if the two sides have
/// different sorts, or any error from [`typecheck_term`] or
/// [`infer_var_sorts`].
pub fn typecheck_equation(eq: &Equation, theory: &Theory) -> Result<(), GatError> {
    let ctx = infer_var_sorts(eq, theory)?;
    let lhs_sort = typecheck_term(&eq.lhs, &ctx, theory)?;
    let rhs_sort = typecheck_term(&eq.rhs, &ctx, theory)?;
    if lhs_sort != rhs_sort {
        return Err(GatError::EquationSortMismatch {
            equation: eq.name.to_string(),
            lhs_sort: lhs_sort.to_string(),
            rhs_sort: rhs_sort.to_string(),
        });
    }
    Ok(())
}

/// Typecheck all equations in a theory.
///
/// # Errors
///
/// Returns the first type error encountered.
pub fn typecheck_theory(theory: &Theory) -> Result<(), GatError> {
    for eq in &theory.eqs {
        typecheck_equation(eq, theory)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eq::Term;
    use crate::op::Operation;
    use crate::sort::Sort;
    use crate::theory::Theory;

    fn monoid_theory() -> Theory {
        let carrier = Sort::simple("Carrier");
        let mul = Operation::new(
            "mul",
            vec![
                ("a".into(), "Carrier".into()),
                ("b".into(), "Carrier".into()),
            ],
            "Carrier",
        );
        let unit = Operation::nullary("unit", "Carrier");

        let assoc = Equation::new(
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
        );
        let left_id = Equation::new(
            "left_id",
            Term::app("mul", vec![Term::constant("unit"), Term::var("a")]),
            Term::var("a"),
        );
        let right_id = Equation::new(
            "right_id",
            Term::app("mul", vec![Term::var("a"), Term::constant("unit")]),
            Term::var("a"),
        );

        Theory::new(
            "Monoid",
            vec![carrier],
            vec![mul, unit],
            vec![assoc, left_id, right_id],
        )
    }

    fn two_sort_theory() -> Theory {
        Theory::new(
            "TwoSort",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![
                Operation::unary("f", "x", "A", "B"),
                Operation::unary("g", "x", "B", "A"),
                Operation::nullary("a0", "A"),
            ],
            vec![],
        )
    }

    #[test]
    fn typecheck_variable() -> Result<(), Box<dyn std::error::Error>> {
        let theory = monoid_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("x"), Arc::from("Carrier"));
        let sort = typecheck_term(&Term::var("x"), &ctx, &theory)?;
        assert_eq!(&*sort, "Carrier");
        Ok(())
    }

    #[test]
    fn typecheck_unbound_variable() {
        let theory = monoid_theory();
        let ctx = VarContext::default();
        let result = typecheck_term(&Term::var("z"), &ctx, &theory);
        assert!(matches!(result, Err(GatError::UnboundVariable(_))));
    }

    #[test]
    fn typecheck_constant() -> Result<(), Box<dyn std::error::Error>> {
        let theory = monoid_theory();
        let ctx = VarContext::default();
        let sort = typecheck_term(&Term::constant("unit"), &ctx, &theory)?;
        assert_eq!(&*sort, "Carrier");
        Ok(())
    }

    #[test]
    fn typecheck_binary_op() -> Result<(), Box<dyn std::error::Error>> {
        let theory = monoid_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("a"), Arc::from("Carrier"));
        ctx.insert(Arc::from("b"), Arc::from("Carrier"));
        let sort = typecheck_term(
            &Term::app("mul", vec![Term::var("a"), Term::var("b")]),
            &ctx,
            &theory,
        )?;
        assert_eq!(&*sort, "Carrier");
        Ok(())
    }

    #[test]
    fn typecheck_arity_mismatch() {
        let theory = monoid_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("a"), Arc::from("Carrier"));
        let result = typecheck_term(&Term::app("mul", vec![Term::var("a")]), &ctx, &theory);
        assert!(matches!(result, Err(GatError::TermArityMismatch { .. })));
    }

    #[test]
    fn typecheck_sort_mismatch() {
        let theory = two_sort_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("x"), Arc::from("B"));
        // f expects A but we give it B
        let result = typecheck_term(&Term::app("f", vec![Term::var("x")]), &ctx, &theory);
        assert!(matches!(result, Err(GatError::ArgTypeMismatch { .. })));
    }

    #[test]
    fn typecheck_nested_term() -> Result<(), Box<dyn std::error::Error>> {
        let theory = two_sort_theory();
        let ctx = VarContext::default();
        // g(f(a0())) : A -- should typecheck
        let term = Term::app("g", vec![Term::app("f", vec![Term::constant("a0")])]);
        let sort = typecheck_term(&term, &ctx, &theory)?;
        assert_eq!(&*sort, "A");
        Ok(())
    }

    #[test]
    fn typecheck_nested_sort_mismatch() {
        let theory = two_sort_theory();
        let ctx = VarContext::default();
        // f(f(a0())) -- inner f returns B, outer f expects A
        let term = Term::app("f", vec![Term::app("f", vec![Term::constant("a0")])]);
        let result = typecheck_term(&term, &ctx, &theory);
        assert!(matches!(result, Err(GatError::ArgTypeMismatch { .. })));
    }

    #[test]
    fn typecheck_unknown_op() {
        let theory = monoid_theory();
        let ctx = VarContext::default();
        let result = typecheck_term(&Term::constant("nonexistent"), &ctx, &theory);
        assert!(matches!(result, Err(GatError::OpNotFound(_))));
    }

    #[test]
    fn infer_var_sorts_monoid() -> Result<(), Box<dyn std::error::Error>> {
        let theory = monoid_theory();
        let eq = &theory.eqs[0]; // assoc
        let ctx = infer_var_sorts(eq, &theory)?;
        assert_eq!(ctx.len(), 3);
        assert_eq!(&*ctx[&Arc::from("a")], "Carrier");
        assert_eq!(&*ctx[&Arc::from("b")], "Carrier");
        assert_eq!(&*ctx[&Arc::from("c")], "Carrier");
        Ok(())
    }

    #[test]
    fn infer_var_sorts_identity_law() -> Result<(), Box<dyn std::error::Error>> {
        let theory = monoid_theory();
        let eq = &theory.eqs[1]; // left_id: mul(unit(), a) = a
        let ctx = infer_var_sorts(eq, &theory)?;
        assert_eq!(ctx.len(), 1);
        assert_eq!(&*ctx[&Arc::from("a")], "Carrier");
        Ok(())
    }

    #[test]
    fn conflicting_var_sort() {
        let theory = two_sort_theory();
        // Bogus equation: f(x) = g(x) -- x used as A (for f) and B (for g)
        let eq = Equation::new(
            "bogus",
            Term::app("f", vec![Term::var("x")]),
            Term::app("g", vec![Term::var("x")]),
        );
        let result = infer_var_sorts(&eq, &theory);
        assert!(matches!(result, Err(GatError::ConflictingVarSort { .. })));
    }

    #[test]
    fn typecheck_monoid_equations() -> Result<(), Box<dyn std::error::Error>> {
        let theory = monoid_theory();
        typecheck_theory(&theory)?;
        Ok(())
    }

    #[test]
    fn typecheck_equation_sort_mismatch() {
        let theory = two_sort_theory();
        // Equation where LHS has sort B and RHS has sort A
        let eq = Equation::new(
            "bad",
            Term::app("f", vec![Term::constant("a0")]), // sort B
            Term::constant("a0"),                       // sort A
        );
        let result = typecheck_equation(&eq, &theory);
        assert!(matches!(result, Err(GatError::EquationSortMismatch { .. })));
    }

    #[test]
    fn typecheck_graph_theory() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new(
            "Graph",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("tgt", "e", "Edge", "Vertex"),
            ],
            vec![],
        );
        typecheck_theory(&theory)?;
        Ok(())
    }

    #[test]
    fn typecheck_reflexive_graph_equations() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new(
            "ReflexiveGraph",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("tgt", "e", "Edge", "Vertex"),
                Operation::unary("id", "v", "Vertex", "Edge"),
            ],
            vec![
                Equation::new(
                    "src_id",
                    Term::app("src", vec![Term::app("id", vec![Term::var("v")])]),
                    Term::var("v"),
                ),
                Equation::new(
                    "tgt_id",
                    Term::app("tgt", vec![Term::app("id", vec![Term::var("v")])]),
                    Term::var("v"),
                ),
            ],
        );
        typecheck_theory(&theory)?;
        Ok(())
    }

    #[test]
    fn typecheck_symmetric_graph_equations() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new(
            "SymmetricGraph",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("tgt", "e", "Edge", "Vertex"),
                Operation::unary("inv", "e", "Edge", "Edge"),
            ],
            vec![
                Equation::new(
                    "src_inv",
                    Term::app("src", vec![Term::app("inv", vec![Term::var("e")])]),
                    Term::app("tgt", vec![Term::var("e")]),
                ),
                Equation::new(
                    "tgt_inv",
                    Term::app("tgt", vec![Term::app("inv", vec![Term::var("e")])]),
                    Term::app("src", vec![Term::var("e")]),
                ),
                Equation::new(
                    "inv_inv",
                    Term::app("inv", vec![Term::app("inv", vec![Term::var("e")])]),
                    Term::var("e"),
                ),
            ],
        );
        typecheck_theory(&theory)?;
        Ok(())
    }
}
