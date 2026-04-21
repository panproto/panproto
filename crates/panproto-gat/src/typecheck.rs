//! Term type-checking for GAT expressions.
//!
//! Verifies that terms are well-typed with respect to a theory's operation
//! signatures. Each operation is allowed to produce a dependent output
//! sort whose parameters reference the input argument terms: when an
//! operation `f : (x1 : S1, ..., xn : Sn) -> T` is applied to concrete
//! terms `a1, ..., an`, the result sort is `T[x1 := a1, ..., xn := an]`
//! (Cartmell-style substitution into the sort expression).

use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::eq::{Equation, Term};
use crate::error::GatError;
use crate::sort::SortExpr;
use crate::theory::Theory;

/// A variable typing context.
///
/// Maps variable names to their sort expressions. Sort expressions may
/// themselves reference variables (as `Term::Var` nodes under a
/// `SortExpr::App`), which is the load-bearing feature that distinguishes
/// GATs from many-sorted equational theories.
pub type VarContext = FxHashMap<Arc<str>, SortExpr>;

/// Infer the output sort expression of a term given a variable context
/// and theory.
///
/// For `Var(x)`: returns `ctx[x]` or [`GatError::UnboundVariable`].
/// For `App { op, args }`: looks up `op` in the theory, recursively
/// typechecks each argument, and for each argument position `i` compares
/// the argument's inferred sort against `op.inputs[i].1.subst(θ)` where
/// θ is the running substitution from parameter names to argument terms.
/// The returned sort is `op.output.subst(θ)`.
///
/// # Errors
///
/// Returns an error if:
/// - A variable is not in the context ([`GatError::UnboundVariable`]).
/// - An operation is not in the theory ([`GatError::OpNotFound`]).
/// - Argument count does not match ([`GatError::TermArityMismatch`]).
/// - An argument's sort does not alpha-equal the expected input sort
///   under the running substitution ([`GatError::ArgTypeMismatch`]).
pub fn typecheck_term(
    term: &Term,
    ctx: &VarContext,
    theory: &Theory,
) -> Result<SortExpr, GatError> {
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

            let mut theta: FxHashMap<Arc<str>, Term> = FxHashMap::default();
            for (i, (arg, (param_name, declared_sort))) in
                args.iter().zip(operation.inputs.iter()).enumerate()
            {
                let arg_sort = typecheck_term(arg, ctx, theory)?;
                let expected = declared_sort.subst(&theta);
                if !arg_sort.alpha_eq(&expected) {
                    return Err(GatError::ArgTypeMismatch {
                        op: op.to_string(),
                        arg_index: i,
                        expected: expected.to_string(),
                        got: arg_sort.to_string(),
                    });
                }
                theta.insert(Arc::clone(param_name), arg.clone());
            }

            Ok(operation.output.subst(&theta))
        }
    }
}

/// Infer variable sorts from an equation's term structure.
///
/// Walks both sides of the equation and, for every operation-application
/// site, imposes a sort-expression constraint on each variable argument.
/// When two uses of the same variable impose different constraints, the
/// constraints are unified via first-order unification over `Term`,
/// producing a term-level substitution that is then applied back to the
/// inferred sort expressions.
///
/// # Errors
///
/// Returns [`GatError::ConflictingVarSort`] when two sort-expression
/// constraints on a variable have different heads,
/// [`GatError::SortUnificationFailure`] when unification fails
/// (including the occurs check), or [`GatError::OpNotFound`] when a
/// referenced operation is absent from the theory.
pub fn infer_var_sorts(eq: &Equation, theory: &Theory) -> Result<VarContext, GatError> {
    let mut ctx = VarContext::default();
    let mut term_eqs: Vec<(Term, Term)> = Vec::new();
    collect_constraints(&eq.lhs, theory, &mut ctx, &mut term_eqs)?;
    collect_constraints(&eq.rhs, theory, &mut ctx, &mut term_eqs)?;

    let substitution = unify_all(term_eqs)?;
    if !substitution.is_empty() {
        for sort in ctx.values_mut() {
            *sort = sort.subst(&substitution);
        }
    }
    Ok(ctx)
}

/// Recursive helper: walk a term and constrain each variable argument
/// to the expected input sort of its enclosing operation (with the
/// running substitution θ applied so earlier arguments flow into later
/// expected sorts).
fn collect_constraints(
    term: &Term,
    theory: &Theory,
    ctx: &mut VarContext,
    term_eqs: &mut Vec<(Term, Term)>,
) -> Result<(), GatError> {
    let Term::App { op, args } = term else {
        return Ok(());
    };
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

    let mut theta: FxHashMap<Arc<str>, Term> = FxHashMap::default();
    for (arg, (param_name, declared_sort)) in args.iter().zip(operation.inputs.iter()) {
        let expected = declared_sort.subst(&theta);
        match arg {
            Term::Var(var_name) => {
                if let Some(existing) = ctx.get(var_name).cloned() {
                    unify_sort_exprs(&existing, &expected, var_name, term_eqs)?;
                } else {
                    ctx.insert(Arc::clone(var_name), expected);
                }
            }
            Term::App { .. } => {
                collect_constraints(arg, theory, ctx, term_eqs)?;
            }
        }
        theta.insert(Arc::clone(param_name), arg.clone());
    }
    Ok(())
}

/// Push pairwise equality constraints between two sort expressions.
///
/// Returns [`GatError::ConflictingVarSort`] when the heads differ or the
/// argument arities do not line up. On success, accumulates pairwise
/// `(Term, Term)` constraints into `term_eqs` for a later unification
/// pass.
fn unify_sort_exprs(
    a: &SortExpr,
    b: &SortExpr,
    var: &Arc<str>,
    term_eqs: &mut Vec<(Term, Term)>,
) -> Result<(), GatError> {
    if a.head() != b.head() {
        return Err(GatError::ConflictingVarSort {
            var: var.to_string(),
            sort1: a.to_string(),
            sort2: b.to_string(),
        });
    }
    let a_args = a.args();
    let b_args = b.args();
    if a_args.len() != b_args.len() {
        return Err(GatError::ConflictingVarSort {
            var: var.to_string(),
            sort1: a.to_string(),
            sort2: b.to_string(),
        });
    }
    for (x, y) in a_args.iter().zip(b_args.iter()) {
        term_eqs.push((x.clone(), y.clone()));
    }
    Ok(())
}

/// First-order unification over a list of term equality constraints.
///
/// Implements Robinson-style unification with an explicit occurs check.
/// Returns a substitution mapping variable names to terms, or a
/// [`GatError::SortUnificationFailure`] when the constraints are
/// unsatisfiable.
fn unify_all(mut eqs: Vec<(Term, Term)>) -> Result<FxHashMap<Arc<str>, Term>, GatError> {
    let mut subst: FxHashMap<Arc<str>, Term> = FxHashMap::default();

    while let Some((a, b)) = eqs.pop() {
        let a = apply_subst(&a, &subst);
        let b = apply_subst(&b, &subst);
        match (a, b) {
            (Term::Var(x), Term::Var(y)) if x == y => {}
            (Term::Var(x), t) | (t, Term::Var(x)) => {
                if occurs_in(&x, &t) {
                    return Err(GatError::SortUnificationFailure {
                        reason: format!("occurs check failed: {x} in {t}"),
                    });
                }
                // Extend substitution with x := t, applying it to existing bindings.
                let updated: FxHashMap<Arc<str>, Term> = subst
                    .iter()
                    .map(|(k, v)| {
                        (
                            Arc::clone(k),
                            v.substitute(&std::iter::once((Arc::clone(&x), t.clone())).collect()),
                        )
                    })
                    .collect();
                subst = updated;
                subst.insert(x, t);
            }
            (
                Term::App {
                    op: op_a,
                    args: args_a,
                },
                Term::App {
                    op: op_b,
                    args: args_b,
                },
            ) => {
                if op_a != op_b {
                    return Err(GatError::SortUnificationFailure {
                        reason: format!("cannot unify {op_a}(...) with {op_b}(...)"),
                    });
                }
                if args_a.len() != args_b.len() {
                    return Err(GatError::SortUnificationFailure {
                        reason: format!(
                            "arity mismatch unifying {op_a}: {} vs {}",
                            args_a.len(),
                            args_b.len()
                        ),
                    });
                }
                for pair in args_a.into_iter().zip(args_b) {
                    eqs.push(pair);
                }
            }
        }
    }

    Ok(subst)
}

fn apply_subst(term: &Term, subst: &FxHashMap<Arc<str>, Term>) -> Term {
    if subst.is_empty() {
        return term.clone();
    }
    term.substitute(subst)
}

fn occurs_in(var: &Arc<str>, term: &Term) -> bool {
    match term {
        Term::Var(v) => v == var,
        Term::App { args, .. } => args.iter().any(|a| occurs_in(var, a)),
    }
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
    if !lhs_sort.alpha_eq(&rhs_sort) {
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
    use crate::sort::{Sort, SortParam};
    use crate::theory::Theory;

    fn monoid_theory() -> Theory {
        let carrier = Sort::simple("Carrier");
        let mul = Operation::new(
            "mul",
            vec![
                (Arc::from("a"), SortExpr::from("Carrier")),
                (Arc::from("b"), SortExpr::from("Carrier")),
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

    /// A minimal category-like theory used to exercise the dependent
    /// sort machinery. `Hom(a, b)` is the hom-sort; `id(x)` inhabits
    /// `Hom(x, x)`; `compose(f, g)` is the composition with the middle
    /// object shared between the two hom-sorts.
    fn category_theory() -> Theory {
        let ob = Sort::simple("Ob");
        let hom = Sort::dependent(
            "Hom",
            vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
        );
        let hom_xx = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("x")],
        };
        let id = Operation::unary("id", "x", "Ob", hom_xx);
        let hom_src_mid = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("y")],
        };
        let hom_mid_tgt = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("y"), Term::var("z")],
        };
        let hom_src_tgt = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("z")],
        };
        let compose = Operation::new(
            "compose",
            vec![
                (Arc::from("x"), SortExpr::from("Ob")),
                (Arc::from("y"), SortExpr::from("Ob")),
                (Arc::from("z"), SortExpr::from("Ob")),
                (Arc::from("f"), hom_src_mid),
                (Arc::from("g"), hom_mid_tgt),
            ],
            hom_src_tgt,
        );
        Theory::new("Category", vec![ob, hom], vec![id, compose], Vec::new())
    }

    #[test]
    fn typecheck_variable() -> Result<(), Box<dyn std::error::Error>> {
        let theory = monoid_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("x"), SortExpr::from("Carrier"));
        let sort = typecheck_term(&Term::var("x"), &ctx, &theory)?;
        assert_eq!(&**sort.head(), "Carrier");
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
        assert_eq!(&**sort.head(), "Carrier");
        Ok(())
    }

    #[test]
    fn typecheck_binary_op() -> Result<(), Box<dyn std::error::Error>> {
        let theory = monoid_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("a"), SortExpr::from("Carrier"));
        ctx.insert(Arc::from("b"), SortExpr::from("Carrier"));
        let sort = typecheck_term(
            &Term::app("mul", vec![Term::var("a"), Term::var("b")]),
            &ctx,
            &theory,
        )?;
        assert_eq!(&**sort.head(), "Carrier");
        Ok(())
    }

    #[test]
    fn typecheck_arity_mismatch() {
        let theory = monoid_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("a"), SortExpr::from("Carrier"));
        let result = typecheck_term(&Term::app("mul", vec![Term::var("a")]), &ctx, &theory);
        assert!(matches!(result, Err(GatError::TermArityMismatch { .. })));
    }

    #[test]
    fn typecheck_sort_mismatch() {
        let theory = two_sort_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("x"), SortExpr::from("B"));
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
        assert_eq!(&**sort.head(), "A");
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
        assert_eq!(&**ctx[&Arc::from("a")].head(), "Carrier");
        assert_eq!(&**ctx[&Arc::from("b")].head(), "Carrier");
        assert_eq!(&**ctx[&Arc::from("c")].head(), "Carrier");
        Ok(())
    }

    #[test]
    fn infer_var_sorts_identity_law() -> Result<(), Box<dyn std::error::Error>> {
        let theory = monoid_theory();
        let eq = &theory.eqs[1]; // left_id
        let ctx = infer_var_sorts(eq, &theory)?;
        assert_eq!(ctx.len(), 1);
        assert_eq!(&**ctx[&Arc::from("a")].head(), "Carrier");
        Ok(())
    }

    #[test]
    fn conflicting_var_sort() {
        let theory = two_sort_theory();
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
        let eq = Equation::new(
            "bad",
            Term::app("f", vec![Term::constant("a0")]),
            Term::constant("a0"),
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

    // --- Dependent-sort tests ---

    #[test]
    fn typecheck_dependent_id_ok() -> Result<(), Box<dyn std::error::Error>> {
        let theory = category_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("x"), SortExpr::from("Ob"));
        let result = typecheck_term(&Term::app("id", vec![Term::var("x")]), &ctx, &theory)?;
        assert_eq!(&**result.head(), "Hom");
        assert_eq!(result.args().len(), 2);
        // Both args should be `x`.
        assert_eq!(result.args()[0], Term::var("x"));
        assert_eq!(result.args()[1], Term::var("x"));
        Ok(())
    }

    #[test]
    fn typecheck_dependent_compose_ok() -> Result<(), Box<dyn std::error::Error>> {
        let theory = category_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("a"), SortExpr::from("Ob"));
        ctx.insert(Arc::from("b"), SortExpr::from("Ob"));
        ctx.insert(Arc::from("c"), SortExpr::from("Ob"));
        ctx.insert(
            Arc::from("f"),
            SortExpr::App {
                name: Arc::from("Hom"),
                args: vec![Term::var("a"), Term::var("b")],
            },
        );
        ctx.insert(
            Arc::from("g"),
            SortExpr::App {
                name: Arc::from("Hom"),
                args: vec![Term::var("b"), Term::var("c")],
            },
        );
        let term = Term::app(
            "compose",
            vec![
                Term::var("a"),
                Term::var("b"),
                Term::var("c"),
                Term::var("f"),
                Term::var("g"),
            ],
        );
        let result = typecheck_term(&term, &ctx, &theory)?;
        let expected = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("a"), Term::var("c")],
        };
        assert!(result.alpha_eq(&expected), "got {result}");
        Ok(())
    }

    #[test]
    fn typecheck_dependent_compose_arg_mismatch() {
        let theory = category_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("a"), SortExpr::from("Ob"));
        ctx.insert(Arc::from("b"), SortExpr::from("Ob"));
        ctx.insert(Arc::from("c"), SortExpr::from("Ob"));
        // f : Hom(a, b) and g : Hom(c, c). Middle object disagrees.
        ctx.insert(
            Arc::from("f"),
            SortExpr::App {
                name: Arc::from("Hom"),
                args: vec![Term::var("a"), Term::var("b")],
            },
        );
        ctx.insert(
            Arc::from("g"),
            SortExpr::App {
                name: Arc::from("Hom"),
                args: vec![Term::var("c"), Term::var("c")],
            },
        );
        let term = Term::app(
            "compose",
            vec![
                Term::var("a"),
                Term::var("b"),
                Term::var("c"),
                Term::var("f"),
                Term::var("g"),
            ],
        );
        let result = typecheck_term(&term, &ctx, &theory);
        assert!(
            matches!(result, Err(GatError::ArgTypeMismatch { .. })),
            "expected ArgTypeMismatch, got {result:?}",
        );
    }

    #[test]
    fn typecheck_dependent_equation_ok() -> Result<(), Box<dyn std::error::Error>> {
        // Build a category with the associativity equation
        // compose(a,b,d, f, compose(b,c,d, g, h))
        //   = compose(a,c,d, compose(a,b,c, f, g), h)
        let mut theory = category_theory();
        let assoc = Equation::new(
            "assoc",
            Term::app(
                "compose",
                vec![
                    Term::var("a"),
                    Term::var("b"),
                    Term::var("d"),
                    Term::var("f"),
                    Term::app(
                        "compose",
                        vec![
                            Term::var("b"),
                            Term::var("c"),
                            Term::var("d"),
                            Term::var("g"),
                            Term::var("h"),
                        ],
                    ),
                ],
            ),
            Term::app(
                "compose",
                vec![
                    Term::var("a"),
                    Term::var("c"),
                    Term::var("d"),
                    Term::app(
                        "compose",
                        vec![
                            Term::var("a"),
                            Term::var("b"),
                            Term::var("c"),
                            Term::var("f"),
                            Term::var("g"),
                        ],
                    ),
                    Term::var("h"),
                ],
            ),
        );
        theory.eqs.push(assoc);
        typecheck_theory(&theory)?;
        Ok(())
    }

    // --- A3: unification soundness, occurs check, idempotence ---

    #[test]
    fn unify_same_var_yields_empty_subst() -> Result<(), Box<dyn std::error::Error>> {
        let subst = unify_all(vec![(Term::var("x"), Term::var("x"))])?;
        assert!(subst.is_empty());
        Ok(())
    }

    #[test]
    fn unify_var_to_constant_binds() -> Result<(), Box<dyn std::error::Error>> {
        let subst = unify_all(vec![(Term::var("x"), Term::constant("c"))])?;
        assert_eq!(subst.get(&Arc::from("x")), Some(&Term::constant("c")));
        Ok(())
    }

    #[test]
    fn unify_occurs_check_fails() {
        // x = f(x) must fail the occurs check.
        let r = unify_all(vec![(Term::var("x"), Term::app("f", vec![Term::var("x")]))]);
        assert!(matches!(r, Err(GatError::SortUnificationFailure { .. })));
    }

    #[test]
    fn unify_head_mismatch_fails() {
        let r = unify_all(vec![(
            Term::app("f", vec![Term::var("x")]),
            Term::app("g", vec![Term::var("x")]),
        )]);
        assert!(matches!(r, Err(GatError::SortUnificationFailure { .. })));
    }

    #[test]
    fn unify_is_idempotent() -> Result<(), Box<dyn std::error::Error>> {
        // Unify f(x, y) = f(a, g(b)). Then applying the substitution twice
        // is the same as once.
        let eqs = vec![(
            Term::app("f", vec![Term::var("x"), Term::var("y")]),
            Term::app(
                "f",
                vec![Term::var("a"), Term::app("g", vec![Term::var("b")])],
            ),
        )];
        let subst = unify_all(eqs)?;
        // Apply to x and compare to apply-twice.
        for k in subst.keys() {
            let once = Term::var(Arc::clone(k)).substitute(&subst);
            let twice = once.substitute(&subst);
            assert_eq!(once, twice, "substitution not idempotent on {k}");
        }
        Ok(())
    }

    #[test]
    fn unify_soundness_mgu_instantiates_both_sides() -> Result<(), Box<dyn std::error::Error>> {
        // f(x, g(y)) = f(h(a), g(b))
        let lhs = Term::app(
            "f",
            vec![Term::var("x"), Term::app("g", vec![Term::var("y")])],
        );
        let rhs = Term::app(
            "f",
            vec![
                Term::app("h", vec![Term::var("a")]),
                Term::app("g", vec![Term::var("b")]),
            ],
        );
        let subst = unify_all(vec![(lhs.clone(), rhs.clone())])?;
        let l2 = lhs.substitute(&subst);
        let r2 = rhs.substitute(&subst);
        assert_eq!(l2, r2);
        Ok(())
    }

    // --- A4: typecheck idempotence and substitution commuting ---

    #[test]
    fn typecheck_term_idempotent() -> Result<(), Box<dyn std::error::Error>> {
        let theory = category_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("x"), SortExpr::from("Ob"));
        let t = Term::app("id", vec![Term::var("x")]);
        let s1 = typecheck_term(&t, &ctx, &theory)?;
        let s2 = typecheck_term(&t, &ctx, &theory)?;
        assert_eq!(s1, s2);
        Ok(())
    }

    #[test]
    fn typecheck_context_strengthening() -> Result<(), Box<dyn std::error::Error>> {
        let theory = category_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("x"), SortExpr::from("Ob"));
        let t = Term::app("id", vec![Term::var("x")]);
        let s1 = typecheck_term(&t, &ctx, &theory)?;
        // Extend ctx with unrelated var.
        ctx.insert(Arc::from("unused"), SortExpr::from("Ob"));
        let s2 = typecheck_term(&t, &ctx, &theory)?;
        assert_eq!(s1, s2);
        Ok(())
    }

    #[test]
    fn typecheck_substitution_commutes() -> Result<(), Box<dyn std::error::Error>> {
        // typecheck(t, ctx) = s implies typecheck(t.subst(sigma), ctx.subst(sigma)) = s.subst(sigma)
        let theory = category_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("x"), SortExpr::from("Ob"));
        let t = Term::app("id", vec![Term::var("x")]);
        let s = typecheck_term(&t, &ctx, &theory)?;

        // sigma maps x to a new variable y : Ob.
        let mut sigma: FxHashMap<Arc<str>, Term> = FxHashMap::default();
        sigma.insert(Arc::from("x"), Term::var("y"));

        let t_prime = t.substitute(&sigma);
        let mut ctx_prime = VarContext::default();
        ctx_prime.insert(Arc::from("y"), SortExpr::from("Ob"));

        let s_prime = typecheck_term(&t_prime, &ctx_prime, &theory)?;
        let s_expected = s.subst(&sigma);
        assert!(
            s_prime.alpha_eq(&s_expected),
            "got {s_prime}, expected {s_expected}"
        );
        Ok(())
    }

    // --- GATlab bug audit: dependent-sort unification soundness ---
    //
    // These three tests exercise the GATlab `bind_localctx` first-match
    // bug: in GATlab, `compose(f, g)` with mismatched middle object
    // still typechecks because only one derivation of each implicit
    // parameter is consulted. panproto's `typecheck_term` propagates
    // the substitution theta left-to-right and compares each expected
    // input sort under theta against the argument's inferred sort via
    // strict `alpha_eq`, so every repeated derivation of a shared
    // parameter is checked for agreement.

    #[test]
    fn gatlab_bug7_compose_mismatched_middle_object_rejected() {
        // Test A from the audit. compose : (x, y, z : Ob, f : Hom(x, y),
        // g : Hom(y, z)) -> Hom(x, z). Supply f : Hom(p, q) and g :
        // Hom(r, s) with q != r and call compose with explicit
        // middle-object choice that cannot satisfy both.
        let theory = category_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("p"), SortExpr::from("Ob"));
        ctx.insert(Arc::from("q"), SortExpr::from("Ob"));
        ctx.insert(Arc::from("r"), SortExpr::from("Ob"));
        ctx.insert(Arc::from("s"), SortExpr::from("Ob"));
        ctx.insert(
            Arc::from("f"),
            SortExpr::App {
                name: Arc::from("Hom"),
                args: vec![Term::var("p"), Term::var("q")],
            },
        );
        ctx.insert(
            Arc::from("g"),
            SortExpr::App {
                name: Arc::from("Hom"),
                args: vec![Term::var("r"), Term::var("s")],
            },
        );
        // Whatever Ob we pick for the middle argument, one of f or g
        // cannot match it: f wants middle = q, g wants middle = r, and
        // q and r are distinct Obs.
        let term = Term::app(
            "compose",
            vec![
                Term::var("p"),
                Term::var("q"),
                Term::var("s"),
                Term::var("f"),
                Term::var("g"),
            ],
        );
        let result = typecheck_term(&term, &ctx, &theory);
        assert!(
            matches!(result, Err(GatError::ArgTypeMismatch { .. })),
            "compose with mismatched middle object must be rejected, got {result:?}",
        );
    }

    #[test]
    fn gatlab_bug7_compose_id_with_hom_mismatch_rejected() {
        // Test B from the audit. compose(id(p), f) where f : Hom(q, r)
        // with p != q. id(p) has sort Hom(p, p); for compose to
        // accept, the middle object must equal p, but the second
        // argument requires Hom(middle, _) = Hom(q, r), so p = q is
        // forced and fails.
        let theory = category_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("p"), SortExpr::from("Ob"));
        ctx.insert(Arc::from("q"), SortExpr::from("Ob"));
        ctx.insert(Arc::from("r"), SortExpr::from("Ob"));
        ctx.insert(
            Arc::from("f"),
            SortExpr::App {
                name: Arc::from("Hom"),
                args: vec![Term::var("q"), Term::var("r")],
            },
        );
        // Explicit Obs: (src = p, mid = p, tgt = r) forces g's sort
        // check with expected Hom(p, r) but actual Hom(q, r), so q != p
        // fails.
        let term = Term::app(
            "compose",
            vec![
                Term::var("p"),
                Term::var("p"),
                Term::var("r"),
                Term::app("id", vec![Term::var("p")]),
                Term::var("f"),
            ],
        );
        let result = typecheck_term(&term, &ctx, &theory);
        assert!(
            matches!(result, Err(GatError::ArgTypeMismatch { .. })),
            "compose(id(p), f) with src(f) != p must be rejected, got {result:?}",
        );
    }

    #[test]
    fn gatlab_bug7_compose_two_ids_distinct_objects_rejected() {
        // Test C from the audit. compose(id(p), id(q)) with p != q:
        // id(p) : Hom(p, p), id(q) : Hom(q, q); these cannot share a
        // middle object because p and q are distinct Obs. No choice of
        // the three explicit middle-object arguments makes both
        // input-sort checks pass.
        let theory = category_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("p"), SortExpr::from("Ob"));
        ctx.insert(Arc::from("q"), SortExpr::from("Ob"));
        // Choose the middle as p; then id(p) : Hom(p, p) is ok for the
        // first hom-slot, but id(q) : Hom(q, q) cannot match the
        // expected Hom(p, q).
        let term = Term::app(
            "compose",
            vec![
                Term::var("p"),
                Term::var("p"),
                Term::var("q"),
                Term::app("id", vec![Term::var("p")]),
                Term::app("id", vec![Term::var("q")]),
            ],
        );
        let result = typecheck_term(&term, &ctx, &theory);
        assert!(
            matches!(result, Err(GatError::ArgTypeMismatch { .. })),
            "compose(id(p), id(q)) with p != q must be rejected, got {result:?}",
        );
    }

    // --- GATlab bug audit Bug 6: exhaustive negative typecheck tests ---

    #[test]
    fn gatlab_bug6_equation_dependent_sort_arg_mismatch() {
        // Equation whose argument sort does not unify with the
        // declared input sort. f : Hom(a, b); the equation uses f on
        // a term of simple sort Ob, which cannot typecheck.
        let theory = category_theory();
        let eq = Equation::new(
            "bad",
            Term::app("id", vec![Term::app("id", vec![Term::var("x")])]),
            Term::var("x"),
        );
        // id(id(x)): inner id(x) has sort Hom(x, x), but outer id
        // expects Ob. Typechecking this equation should error.
        let result = typecheck_equation(&eq, &theory);
        assert!(
            result.is_err(),
            "equation with argument-sort mismatch must error, got {result:?}",
        );
    }

    #[test]
    fn gatlab_bug6_equation_with_unknown_op_errors() {
        let theory = monoid_theory();
        let eq = Equation::new(
            "bad",
            Term::app("mystery", vec![Term::var("a")]),
            Term::var("a"),
        );
        let result = typecheck_equation(&eq, &theory);
        assert!(
            matches!(result, Err(GatError::OpNotFound(_))),
            "equation referencing unknown op must error, got {result:?}",
        );
    }

    #[test]
    fn gatlab_bug6_equation_with_arity_mismatch_errors() {
        let theory = monoid_theory();
        let eq = Equation::new(
            "bad",
            Term::app("mul", vec![Term::var("a")]),
            Term::var("a"),
        );
        let result = typecheck_equation(&eq, &theory);
        assert!(
            matches!(result, Err(GatError::TermArityMismatch { .. })),
            "equation with arity mismatch must error, got {result:?}",
        );
    }

    #[test]
    fn gatlab_bug6_dependent_sort_with_ill_typed_arg_errors() {
        // Build a context where f is supposed to inhabit Hom(x, x)
        // but we attempt typecheck_term on compose with an explicit
        // Ob argument that is in fact a Hom term. This targets the
        // case where a dependent sort's argument term does not
        // typecheck.
        let theory = category_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("x"), SortExpr::from("Ob"));
        ctx.insert(
            Arc::from("f"),
            SortExpr::App {
                name: Arc::from("Hom"),
                args: vec![Term::var("x"), Term::var("x")],
            },
        );
        // Pass f (which is a Hom, not an Ob) in the src-Ob position.
        let term = Term::app(
            "compose",
            vec![
                Term::var("f"),
                Term::var("x"),
                Term::var("x"),
                Term::var("f"),
                Term::var("f"),
            ],
        );
        let result = typecheck_term(&term, &ctx, &theory);
        assert!(
            matches!(result, Err(GatError::ArgTypeMismatch { .. })),
            "ill-typed dependent-sort argument must error, got {result:?}",
        );
    }

    // --- proptest property tests ---

    mod property {
        use super::*;
        use proptest::prelude::*;

        const SORT_POOL: &[&str] = &["S0", "S1", "S2", "S3"];

        /// Generate a well-typed theory: only simple sorts and operations
        /// with correct sort references (no equations).
        fn arb_well_typed_theory() -> impl Strategy<Value = Theory> {
            prop::sample::subsequence(SORT_POOL, 1..=4).prop_flat_map(|sort_names| {
                let sorts: Vec<Sort> = sort_names.iter().map(|s| Sort::simple(*s)).collect();
                let sn: Vec<String> = sort_names.iter().map(|s| (*s).to_owned()).collect();
                let sn2 = sn.clone();
                (
                    Just(sorts),
                    prop::collection::vec(
                        (
                            0..4usize,
                            prop::sample::select(sn),
                            prop::sample::select(sn2),
                        ),
                        0..=3,
                    ),
                )
                    .prop_map(|(sorts, op_specs)| {
                        let mut ops = Vec::new();
                        let mut seen = std::collections::HashSet::new();
                        for (i, (_, input_sort, output_sort)) in op_specs.iter().enumerate() {
                            let name = format!("op{i}");
                            if !seen.insert(name.clone()) {
                                continue;
                            }
                            ops.push(Operation::unary(
                                &*name,
                                "x",
                                input_sort.as_str(),
                                output_sort.as_str(),
                            ));
                        }
                        Theory::new("TypecheckTest", sorts, ops, Vec::new())
                    })
            })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn typecheck_is_idempotent(t in arb_well_typed_theory()) {
                let result1 = typecheck_theory(&t);
                let result2 = typecheck_theory(&t);
                prop_assert_eq!(result1.is_ok(), result2.is_ok());
            }

            #[test]
            fn well_typed_theory_passes(t in arb_well_typed_theory()) {
                prop_assert!(
                    typecheck_theory(&t).is_ok(),
                    "well-typed theory should pass typecheck",
                );
            }

            #[test]
            fn unification_soundness_on_congruent_pairs(
                c1 in prop::sample::select(&["a", "b", "c"][..]),
                c2 in prop::sample::select(&["a", "b", "c"][..]),
            ) {
                // f(x, y) = f(c1, c2) under unification: the substitution
                // must make both sides equal.
                let lhs = Term::app(
                    "f",
                    vec![Term::var("x"), Term::var("y")],
                );
                let rhs = Term::app(
                    "f",
                    vec![Term::constant(c1), Term::constant(c2)],
                );
                let subst = match unify_all(vec![(lhs.clone(), rhs.clone())]) {
                    Ok(s) => s,
                    Err(e) => {
                        prop_assert!(false, "unify failed: {e}");
                        return Ok(());
                    }
                };
                let l2 = lhs.substitute(&subst);
                let r2 = rhs.substitute(&subst);
                prop_assert_eq!(l2, r2);
            }
        }
    }
}
