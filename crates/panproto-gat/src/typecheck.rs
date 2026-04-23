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

use crate::eq::{CaseBranch, Equation, Term};
use crate::error::GatError;
use crate::op::{Implicit, Operation};
use crate::sort::{SortClosure, SortExpr};
use crate::theory::Theory;

/// A report generated for each typed hole encountered by
/// [`typecheck_term_with_holes`].
#[derive(Debug, Clone)]
pub struct HoleReport {
    /// The hole's optional name (from `?name` syntax).
    pub name: Option<Arc<str>>,
    /// The sort expected at this hole site. For holes whose sort is not
    /// constrained by the surrounding context, this is a fresh
    /// metavariable sort.
    pub expected: SortExpr,
    /// The variable context in scope at the hole.
    pub context: VarContext,
    /// Optional source span (populated by the DSL when available).
    pub position: Option<miette::SourceSpan>,
}

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

        Term::Hole { name } => {
            // In strict mode (no hole collector), a hole typechecks to a
            // fresh metavariable sort; there is no constraint to solve,
            // so the hole is accepted and the caller is told its sort is
            // a metavariable `?<name>`. Use `typecheck_term_with_holes`
            // to collect hole reports.
            let mv: Arc<str> = Arc::from(format!("?{}", name.as_deref().unwrap_or("hole")));
            Ok(SortExpr::Name(mv))
        }

        Term::App { op, args } => {
            let operation = theory
                .find_op(op)
                .ok_or_else(|| GatError::OpNotFound(op.to_string()))?;

            let has_implicits = operation
                .inputs
                .iter()
                .any(|(_, _, imp)| matches!(imp, Implicit::Yes));
            if has_implicits {
                typecheck_app_with_implicits(op, args, operation, ctx, theory)
            } else {
                typecheck_app_explicit(op, args, operation, ctx, theory)
            }
        }

        Term::Case {
            scrutinee,
            branches,
        } => typecheck_case(scrutinee, branches, ctx, theory),
    }
}

/// Typecheck a [`Term::Case`] expression.
fn typecheck_case(
    scrutinee: &Term,
    branches: &[CaseBranch],
    ctx: &VarContext,
    theory: &Theory,
) -> Result<SortExpr, GatError> {
    let scrutinee_sort = typecheck_term(scrutinee, ctx, theory)?;
    let sort_name = scrutinee_sort.head();
    let sort_decl = theory
        .find_sort(sort_name)
        .ok_or_else(|| GatError::SortNotFound(sort_name.to_string()))?;
    let constructors = match &sort_decl.closure {
        SortClosure::Open => {
            return Err(GatError::CaseOnOpenSort {
                sort: sort_name.to_string(),
            });
        }
        SortClosure::Closed(cs) => cs.clone(),
    };

    // Exhaustiveness: every constructor appears exactly once, and no
    // branch names an unknown constructor. Build both checks in one
    // pass.
    let mut seen: rustc_hash::FxHashSet<Arc<str>> = rustc_hash::FxHashSet::default();
    for b in branches {
        if !constructors.contains(&b.constructor) {
            return Err(GatError::UnknownCaseConstructor {
                sort: sort_name.to_string(),
                constructor: b.constructor.to_string(),
            });
        }
        if !seen.insert(Arc::clone(&b.constructor)) {
            return Err(GatError::RedundantCaseBranch {
                sort: sort_name.to_string(),
                constructor: b.constructor.to_string(),
            });
        }
    }
    if seen.len() < constructors.len() {
        let missing: Vec<String> = constructors
            .iter()
            .filter(|c| !seen.contains(*c))
            .map(ToString::to_string)
            .collect();
        return Err(GatError::NonExhaustiveCase {
            sort: sort_name.to_string(),
            missing,
        });
    }

    // Typecheck each branch body; all must produce alpha-equivalent
    // output sorts. The first branch's output sort is the case
    // expression's sort.
    let mut branch_sort: Option<SortExpr> = None;
    for b in branches {
        let constructor_op = theory
            .find_op(&b.constructor)
            .ok_or_else(|| GatError::OpNotFound(b.constructor.to_string()))?;
        if constructor_op.inputs.len() != b.binders.len() {
            return Err(GatError::TermArityMismatch {
                op: b.constructor.to_string(),
                expected: constructor_op.inputs.len(),
                got: b.binders.len(),
            });
        }
        // Unify constructor's declared output sort with the actual
        // scrutinee sort to instantiate the constructor's parameter
        // vars.
        let unify_eqs: Vec<(Term, Term)> = constructor_op
            .output
            .args()
            .iter()
            .zip(scrutinee_sort.args().iter())
            .map(|(a, b)| (a.clone(), b.clone()))
            .collect();
        if constructor_op.output.head() != scrutinee_sort.head()
            || constructor_op.output.args().len() != scrutinee_sort.args().len()
        {
            return Err(GatError::OpTypeMismatch {
                op: b.constructor.to_string(),
                detail: format!(
                    "constructor output sort {} does not match scrutinee sort {scrutinee_sort}",
                    constructor_op.output
                ),
            });
        }
        let subst = unify_all(unify_eqs)?;
        let mut extended = ctx.clone();
        for ((_, declared_sort, _), binder) in constructor_op.inputs.iter().zip(b.binders.iter()) {
            let binder_sort = declared_sort.subst(&subst);
            extended.insert(Arc::clone(binder), binder_sort);
        }
        let body_sort = typecheck_term(&b.body, &extended, theory)?;
        match &branch_sort {
            None => branch_sort = Some(body_sort),
            Some(existing) => {
                if !existing.alpha_eq(&body_sort) {
                    return Err(GatError::EquationSortMismatch {
                        equation: "case".to_string(),
                        lhs_sort: existing.to_string(),
                        rhs_sort: body_sort.to_string(),
                    });
                }
            }
        }
    }

    branch_sort.ok_or_else(|| GatError::NonExhaustiveCase {
        sort: sort_name.to_string(),
        missing: constructors.iter().map(ToString::to_string).collect(),
    })
}

/// Typecheck an `App` against an operation with every input marked
/// explicit. Uses the existing sequential-theta-propagation path: each
/// argument's inferred sort must alpha-equal the expected input sort
/// under the running substitution theta.
fn typecheck_app_explicit(
    op: &Arc<str>,
    args: &[Term],
    operation: &Operation,
    ctx: &VarContext,
    theory: &Theory,
) -> Result<SortExpr, GatError> {
    if args.len() != operation.inputs.len() {
        return Err(GatError::TermArityMismatch {
            op: op.to_string(),
            expected: operation.inputs.len(),
            got: args.len(),
        });
    }

    let mut theta: FxHashMap<Arc<str>, Term> = FxHashMap::default();
    for (i, (arg, (param_name, declared_sort, _))) in
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

/// Typecheck an `App` against an operation that declares one or more
/// implicit inputs.
///
/// Implicit parameters are fresh-renamed to unique metavariables to
/// avoid clashing with context variables. Explicit argument sorts are
/// then unified against the declared input sorts via first-order
/// Robinson unification; the MGU recovers the implicit params and the
/// output sort follows by substitution.
fn typecheck_app_with_implicits(
    op: &Arc<str>,
    args: &[Term],
    operation: &Operation,
    ctx: &VarContext,
    theory: &Theory,
) -> Result<SortExpr, GatError> {
    let explicit_count = operation.explicit_arity();
    if args.len() != explicit_count {
        return Err(GatError::TermArityMismatch {
            op: op.to_string(),
            expected: explicit_count,
            got: args.len(),
        });
    }

    // Fresh-rename every implicit param name to a unique metavariable.
    let mut fresh_rename: FxHashMap<Arc<str>, Term> = FxHashMap::default();
    for (idx, (pname, _, imp)) in operation.inputs.iter().enumerate() {
        if matches!(imp, Implicit::Yes) {
            let mv: Arc<str> = Arc::from(format!("?{pname}_{idx}"));
            fresh_rename.insert(Arc::clone(pname), Term::Var(mv));
        }
    }

    // Walk explicit args, typecheck each, and push unification constraints
    // matching declared input sort against inferred sort. Running theta
    // also records explicit-arg values for later positions (dependent
    // signatures).
    let mut theta: FxHashMap<Arc<str>, Term> = fresh_rename.clone();
    let mut term_eqs: Vec<(Term, Term)> = Vec::new();
    let mut explicit_iter = args.iter();
    for (pname, declared_sort, imp) in &operation.inputs {
        match imp {
            Implicit::Yes => {
                // Nothing to do at this slot: the value is recovered by
                // unification.
            }
            Implicit::No => {
                let Some(arg) = explicit_iter.next() else {
                    return Err(GatError::TermArityMismatch {
                        op: op.to_string(),
                        expected: explicit_count,
                        got: args.len(),
                    });
                };
                let arg_sort = typecheck_term(arg, ctx, theory)?;
                let expected = declared_sort.subst(&theta);
                push_sort_expr_eqs_into(&expected, &arg_sort, op, &mut term_eqs)?;
                theta.insert(Arc::clone(pname), arg.clone());
            }
        }
    }

    let mgu = unify_all(term_eqs).map_err(|e| match e {
        GatError::SortUnificationFailure { reason } => GatError::SortUnificationFailure {
            reason: format!("implicit inference for {op}: {reason}"),
        },
        other => other,
    })?;

    // Compose mgu with theta to get the final substitution for the
    // output sort.
    let mut final_subst = theta.clone();
    for (k, v) in &mgu {
        final_subst.insert(Arc::clone(k), v.clone());
    }
    // Apply mgu to the running bindings so metavariables resolve.
    let final_subst: FxHashMap<Arc<str>, Term> = final_subst
        .into_iter()
        .map(|(k, v)| (k, v.substitute(&mgu)))
        .collect();

    Ok(operation.output.subst(&final_subst))
}

/// Push head-agreement + pairwise-arg-term constraints for two sort
/// expressions into a unification queue.
///
/// Heads must agree; on head mismatch this returns
/// [`GatError::ArgTypeMismatch`] naming the two sorts. Argument terms
/// are pushed pairwise onto `term_eqs` for later unification.
fn push_sort_expr_eqs_into(
    expected: &SortExpr,
    actual: &SortExpr,
    op: &Arc<str>,
    term_eqs: &mut Vec<(Term, Term)>,
) -> Result<(), GatError> {
    if expected.head() != actual.head() || expected.args().len() != actual.args().len() {
        return Err(GatError::ArgTypeMismatch {
            op: op.to_string(),
            arg_index: 0,
            expected: expected.to_string(),
            got: actual.to_string(),
        });
    }
    for (x, y) in expected.args().iter().zip(actual.args().iter()) {
        term_eqs.push((x.clone(), y.clone()));
    }
    Ok(())
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
    let (op, args) = match term {
        Term::App { op, args } => (op, args),
        Term::Case {
            scrutinee,
            branches,
        } => {
            collect_constraints(scrutinee, theory, ctx, term_eqs)?;
            for b in branches {
                collect_constraints(&b.body, theory, ctx, term_eqs)?;
            }
            return Ok(());
        }
        Term::Var(_) | Term::Hole { .. } => return Ok(()),
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
    for (arg, (param_name, declared_sort, _)) in args.iter().zip(operation.inputs.iter()) {
        let expected = declared_sort.subst(&theta);
        match arg {
            Term::Var(var_name) => {
                if let Some(existing) = ctx.get(var_name).cloned() {
                    unify_sort_exprs(&existing, &expected, var_name, term_eqs)?;
                } else {
                    ctx.insert(Arc::clone(var_name), expected);
                }
            }
            Term::App { .. } | Term::Case { .. } | Term::Hole { .. } => {
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
            (lhs, rhs) => {
                return Err(GatError::SortUnificationFailure {
                    reason: format!("cannot unify {lhs} with {rhs}"),
                });
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
        Term::Hole { .. } => false,
        Term::App { args, .. } => args.iter().any(|a| occurs_in(var, a)),
        Term::Case {
            scrutinee,
            branches,
        } => {
            occurs_in(var, scrutinee)
                || branches
                    .iter()
                    .any(|b| !b.binders.contains(var) && occurs_in(var, &b.body))
        }
    }
}

/// Typecheck a term, collecting a [`HoleReport`] at every [`Term::Hole`]
/// site.
///
/// Unlike [`typecheck_term`], which treats holes as fresh metavariable
/// sorts and lets the caller discard the information, this walker
/// propagates the surrounding context's expected sort into each hole.
/// When the enclosing operation's input sort constrains the hole, the
/// report carries that sort exactly; otherwise the report carries a
/// fresh metavariable sort.
///
/// # Errors
///
/// Returns any error from [`typecheck_term`] except that hole
/// encounters never fail.
pub fn typecheck_term_with_holes(
    term: &Term,
    ctx: &VarContext,
    theory: &Theory,
) -> Result<(SortExpr, Vec<HoleReport>), GatError> {
    let mut reports: Vec<HoleReport> = Vec::new();
    let sort = typecheck_with_expected(term, None, ctx, theory, &mut reports)?;
    Ok((sort, reports))
}

fn typecheck_with_expected(
    term: &Term,
    expected: Option<&SortExpr>,
    ctx: &VarContext,
    theory: &Theory,
    reports: &mut Vec<HoleReport>,
) -> Result<SortExpr, GatError> {
    match term {
        Term::Hole { name } => {
            let sort = expected.cloned().unwrap_or_else(|| {
                SortExpr::Name(Arc::from(format!("?{}", name.as_deref().unwrap_or("hole"))))
            });
            reports.push(HoleReport {
                name: name.clone(),
                expected: sort.clone(),
                context: ctx.clone(),
                position: None,
            });
            Ok(sort)
        }
        Term::Var(n) => ctx
            .get(n)
            .cloned()
            .ok_or_else(|| GatError::UnboundVariable(n.to_string())),
        Term::App { op, args } => {
            let operation = theory
                .find_op(op)
                .ok_or_else(|| GatError::OpNotFound(op.to_string()))?;
            let has_implicits = operation
                .inputs
                .iter()
                .any(|(_, _, imp)| matches!(imp, Implicit::Yes));
            if has_implicits {
                // Implicit-inference path: recurse into each arg with
                // typecheck_term so that holes get a fresh metavariable
                // sort report before unification kicks in, then run the
                // standard path for the final result sort.
                for arg in args {
                    let _ = typecheck_with_expected(arg, None, ctx, theory, reports)?;
                }
                typecheck_term(term, ctx, theory)
            } else {
                if args.len() != operation.inputs.len() {
                    return Err(GatError::TermArityMismatch {
                        op: op.to_string(),
                        expected: operation.inputs.len(),
                        got: args.len(),
                    });
                }
                let mut theta: FxHashMap<Arc<str>, Term> = FxHashMap::default();
                for (i, (arg, (param_name, declared_sort, _))) in
                    args.iter().zip(operation.inputs.iter()).enumerate()
                {
                    let expected_sort = declared_sort.subst(&theta);
                    let arg_sort =
                        typecheck_with_expected(arg, Some(&expected_sort), ctx, theory, reports)?;
                    // Holes produce the expected sort by construction,
                    // so alpha_eq holds trivially. For non-hole terms,
                    // enforce the usual alpha_eq check.
                    if !term_contains_hole(arg) && !arg_sort.alpha_eq(&expected_sort) {
                        return Err(GatError::ArgTypeMismatch {
                            op: op.to_string(),
                            arg_index: i,
                            expected: expected_sort.to_string(),
                            got: arg_sort.to_string(),
                        });
                    }
                    theta.insert(Arc::clone(param_name), arg.clone());
                }
                Ok(operation.output.subst(&theta))
            }
        }
        Term::Case {
            scrutinee,
            branches,
        } => typecheck_case_with_holes(scrutinee, branches, ctx, theory, reports),
    }
}

fn typecheck_case_with_holes(
    scrutinee: &Term,
    branches: &[CaseBranch],
    ctx: &VarContext,
    theory: &Theory,
    reports: &mut Vec<HoleReport>,
) -> Result<SortExpr, GatError> {
    let scrutinee_sort = typecheck_with_expected(scrutinee, None, ctx, theory, reports)?;
    check_case_exhaustiveness_soft(&scrutinee_sort, branches, theory)?;
    let mut branch_sort: Option<SortExpr> = None;
    for b in branches {
        let constructor_op = theory
            .find_op(&b.constructor)
            .ok_or_else(|| GatError::OpNotFound(b.constructor.to_string()))?;
        let mut extended = ctx.clone();
        for ((_, declared_sort, _), binder) in constructor_op.inputs.iter().zip(b.binders.iter()) {
            extended.insert(Arc::clone(binder), declared_sort.clone());
        }
        let body_sort = typecheck_with_expected(&b.body, None, &extended, theory, reports)?;
        if branch_sort.is_none() {
            branch_sort = Some(body_sort);
        }
    }
    branch_sort.ok_or_else(|| GatError::NonExhaustiveCase {
        sort: scrutinee_sort.head().to_string(),
        missing: Vec::new(),
    })
}

fn check_case_exhaustiveness_soft(
    scrutinee_sort: &SortExpr,
    branches: &[CaseBranch],
    theory: &Theory,
) -> Result<(), GatError> {
    let Some(sort_decl) = theory.find_sort(scrutinee_sort.head()) else {
        return Ok(());
    };
    let SortClosure::Closed(ctors) = &sort_decl.closure else {
        return Ok(());
    };
    let mut seen: rustc_hash::FxHashSet<Arc<str>> = rustc_hash::FxHashSet::default();
    for b in branches {
        if !ctors.contains(&b.constructor) {
            return Err(GatError::UnknownCaseConstructor {
                sort: scrutinee_sort.head().to_string(),
                constructor: b.constructor.to_string(),
            });
        }
        if !seen.insert(Arc::clone(&b.constructor)) {
            return Err(GatError::RedundantCaseBranch {
                sort: scrutinee_sort.head().to_string(),
                constructor: b.constructor.to_string(),
            });
        }
    }
    if seen.len() < ctors.len() {
        let missing: Vec<String> = ctors
            .iter()
            .filter(|c| !seen.contains(*c))
            .map(ToString::to_string)
            .collect();
        return Err(GatError::NonExhaustiveCase {
            sort: scrutinee_sort.head().to_string(),
            missing,
        });
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
/// Also verifies, for every operation, that every implicit parameter's
/// name occurs in at least one explicit input sort or the output sort.
/// An implicit parameter that never appears in a position where
/// first-order unification can pin it down is rejected with
/// [`GatError::NonInferrableImplicit`].
///
/// # Errors
///
/// Returns the first type error encountered.
pub fn typecheck_theory(theory: &Theory) -> Result<(), GatError> {
    for op in &theory.ops {
        check_implicits_inferrable(op)?;
    }
    check_closed_sorts(theory)?;
    for eq in &theory.eqs {
        typecheck_equation(eq, theory)?;
    }
    Ok(())
}

/// Verify every [`SortClosure::Closed`] sort's constructor list against
/// the theory's op table.
///
/// For a sort `S` closed against `[c1, ..., cn]`:
/// - each `ci` must be an op in the theory,
/// - each `ci`'s output head must equal `S`,
/// - no op outside `[c1, ..., cn]` produces `S`.
fn check_closed_sorts(theory: &Theory) -> Result<(), GatError> {
    for sort in &theory.sorts {
        let SortClosure::Closed(ctors) = &sort.closure else {
            continue;
        };
        let ctor_set: rustc_hash::FxHashSet<Arc<str>> = ctors.iter().map(Arc::clone).collect();
        for ctor in ctors {
            let op =
                theory
                    .find_op(ctor)
                    .ok_or_else(|| GatError::InvalidClosedSortConstructor {
                        sort: sort.name.to_string(),
                        constructor: ctor.to_string(),
                        detail: "op does not exist in the theory".to_string(),
                    })?;
            if op.output.head() != &sort.name {
                return Err(GatError::InvalidClosedSortConstructor {
                    sort: sort.name.to_string(),
                    constructor: ctor.to_string(),
                    detail: format!(
                        "op output head is {}, expected {}",
                        op.output.head(),
                        sort.name
                    ),
                });
            }
        }
        for op in &theory.ops {
            if op.output.head() == &sort.name && !ctor_set.contains(&op.name) {
                return Err(GatError::InvalidClosedSortConstructor {
                    sort: sort.name.to_string(),
                    constructor: op.name.to_string(),
                    detail: "op produces the closed sort but is not listed in its closure"
                        .to_string(),
                });
            }
        }
    }
    Ok(())
}

/// Verify that every implicit parameter of `op` occurs as a `Term::Var`
/// in at least one explicit input sort or in the output sort.
fn check_implicits_inferrable(op: &Operation) -> Result<(), GatError> {
    for (pname, _, imp) in &op.inputs {
        if !matches!(imp, Implicit::Yes) {
            continue;
        }
        let mut found = false;
        for (_, sort_expr, other_imp) in &op.inputs {
            if matches!(other_imp, Implicit::No) && sort_expr_mentions_var(sort_expr, pname) {
                found = true;
                break;
            }
        }
        if !found && sort_expr_mentions_var(&op.output, pname) {
            found = true;
        }
        if !found {
            return Err(GatError::NonInferrableImplicit {
                op: op.name.to_string(),
                param: pname.to_string(),
            });
        }
    }
    Ok(())
}

/// Returns `true` if `name` appears as a [`Term::Var`] anywhere in the
/// argument terms of `sort`.
fn sort_expr_mentions_var(sort: &SortExpr, name: &Arc<str>) -> bool {
    sort.args().iter().any(|t| term_mentions_var(t, name))
}

fn term_contains_hole(t: &Term) -> bool {
    match t {
        Term::Hole { .. } => true,
        Term::Var(_) => false,
        Term::App { args, .. } => args.iter().any(term_contains_hole),
        Term::Case {
            scrutinee,
            branches,
        } => term_contains_hole(scrutinee) || branches.iter().any(|b| term_contains_hole(&b.body)),
    }
}

fn term_mentions_var(t: &Term, name: &Arc<str>) -> bool {
    match t {
        Term::Var(v) => v == name,
        Term::Hole { .. } => false,
        Term::App { args, .. } => args.iter().any(|a| term_mentions_var(a, name)),
        Term::Case {
            scrutinee,
            branches,
        } => {
            term_mentions_var(scrutinee, name)
                || branches
                    .iter()
                    .any(|b| !b.binders.contains(name) && term_mentions_var(&b.body, name))
        }
    }
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

    // --- dependent-sort middle-object agreement ---
    //
    // A shared parameter that appears in more than one argument's sort
    // (the middle object in `compose(f : Hom(a, b), g : Hom(b, c))`)
    // must take the same value in every occurrence. `typecheck_term`
    // propagates the substitution theta left-to-right and compares
    // each argument's inferred sort against the expected input sort
    // under theta via strict `alpha_eq`, so mismatched derivations of
    // a shared parameter are rejected.

    #[test]
    fn compose_with_disagreeing_middle_object_is_rejected() {
        // compose : (x, y, z : Ob, f : Hom(x, y), g : Hom(y, z)) ->
        // Hom(x, z). Supply f : Hom(p, q) and g : Hom(r, s) with q != r
        // and call compose with an explicit middle-object choice that
        // cannot satisfy both input-sort constraints at once.
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
    fn compose_of_identity_with_unrelated_arrow_is_rejected() {
        // compose(id(p), f) where f : Hom(q, r) with p != q. id(p)
        // has sort Hom(p, p); compose forces the middle object to
        // equal p in the first slot and q in the second, contradicting.
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
    fn compose_of_two_identities_at_distinct_objects_is_rejected() {
        // compose(id(p), id(q)) with p != q. id(p) : Hom(p, p), id(q)
        // : Hom(q, q); the two identities cannot share a middle object
        // and no choice of the explicit middle-object arguments makes
        // both input-sort checks pass.
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

    // --- negative typecheck cases returning specific error variants ---

    #[test]
    fn equation_with_dependent_sort_arg_mismatch_errors() {
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
    fn equation_with_unknown_op_errors() {
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
    fn equation_with_arity_mismatch_errors() {
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
    fn dependent_sort_with_ill_typed_arg_errors() {
        // Passing a Hom-sorted term in an Ob-sorted argument slot
        // must error rather than silently proceed; this exercises the
        // case where the inferred sort has the wrong head altogether.
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

    // --- closed sorts and case terms (1.2) ---

    /// Theory with closed sort `Nat` and constructors `zero`, `succ`.
    fn nat_theory() -> Theory {
        let nat = Sort::closed(
            "Nat",
            Vec::new(),
            [Arc::from("zero") as Arc<str>, Arc::from("succ")],
        );
        let zero = Operation::nullary("zero", "Nat");
        let succ = Operation::unary("succ", "n", "Nat", "Nat");
        Theory::new("NatTh", vec![nat], vec![zero, succ], Vec::new())
    }

    #[test]
    fn closed_sort_exhaustive_case_typechecks() -> Result<(), Box<dyn std::error::Error>> {
        let theory = nat_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("n"), SortExpr::from("Nat"));
        let term = Term::Case {
            scrutinee: Box::new(Term::var("n")),
            branches: vec![
                CaseBranch {
                    constructor: Arc::from("zero"),
                    binders: Vec::new(),
                    body: Term::constant("zero"),
                },
                CaseBranch {
                    constructor: Arc::from("succ"),
                    binders: vec![Arc::from("m")],
                    body: Term::var("m"),
                },
            ],
        };
        let sort = typecheck_term(&term, &ctx, &theory)?;
        assert_eq!(&**sort.head(), "Nat");
        Ok(())
    }

    #[test]
    fn closed_sort_missing_branch_rejected() {
        let theory = nat_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("n"), SortExpr::from("Nat"));
        let term = Term::Case {
            scrutinee: Box::new(Term::var("n")),
            branches: vec![CaseBranch {
                constructor: Arc::from("zero"),
                binders: Vec::new(),
                body: Term::constant("zero"),
            }],
        };
        let result = typecheck_term(&term, &ctx, &theory);
        assert!(
            matches!(result, Err(GatError::NonExhaustiveCase { .. })),
            "got {result:?}"
        );
    }

    #[test]
    fn closed_sort_redundant_branch_rejected() {
        let theory = nat_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("n"), SortExpr::from("Nat"));
        let term = Term::Case {
            scrutinee: Box::new(Term::var("n")),
            branches: vec![
                CaseBranch {
                    constructor: Arc::from("zero"),
                    binders: Vec::new(),
                    body: Term::constant("zero"),
                },
                CaseBranch {
                    constructor: Arc::from("zero"),
                    binders: Vec::new(),
                    body: Term::constant("zero"),
                },
            ],
        };
        let result = typecheck_term(&term, &ctx, &theory);
        assert!(
            matches!(result, Err(GatError::RedundantCaseBranch { .. })),
            "got {result:?}"
        );
    }

    #[test]
    fn case_on_open_sort_rejected() {
        // Use a plain open sort `Vertex` with a `v0` nullary.
        let v = Sort::simple("Vertex");
        let v0 = Operation::nullary("v0", "Vertex");
        let theory = Theory::new("Open", vec![v], vec![v0], Vec::new());
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("x"), SortExpr::from("Vertex"));
        let term = Term::Case {
            scrutinee: Box::new(Term::var("x")),
            branches: vec![CaseBranch {
                constructor: Arc::from("v0"),
                binders: Vec::new(),
                body: Term::constant("v0"),
            }],
        };
        let result = typecheck_term(&term, &ctx, &theory);
        assert!(
            matches!(result, Err(GatError::CaseOnOpenSort { .. })),
            "got {result:?}"
        );
    }

    #[test]
    fn case_unknown_constructor_rejected() {
        let theory = nat_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("n"), SortExpr::from("Nat"));
        let term = Term::Case {
            scrutinee: Box::new(Term::var("n")),
            branches: vec![
                CaseBranch {
                    constructor: Arc::from("nope"),
                    binders: Vec::new(),
                    body: Term::constant("zero"),
                },
                CaseBranch {
                    constructor: Arc::from("succ"),
                    binders: vec![Arc::from("m")],
                    body: Term::var("m"),
                },
            ],
        };
        let result = typecheck_term(&term, &ctx, &theory);
        assert!(
            matches!(result, Err(GatError::UnknownCaseConstructor { .. })),
            "got {result:?}"
        );
    }

    #[test]
    fn closed_sort_rejects_external_constructor() {
        // A sort closed against [zero] but another op also produces Nat:
        // typecheck_theory must reject this.
        let nat = Sort::closed("Nat", Vec::new(), [Arc::from("zero") as Arc<str>]);
        let zero = Operation::nullary("zero", "Nat");
        let sneaky = Operation::nullary("sneaky", "Nat");
        let theory = Theory::new("BadClosure", vec![nat], vec![zero, sneaky], Vec::new());
        let result = typecheck_theory(&theory);
        assert!(
            matches!(result, Err(GatError::InvalidClosedSortConstructor { .. })),
            "got {result:?}"
        );
    }

    #[test]
    fn morphism_preserves_closure_constructors() -> Result<(), Box<dyn std::error::Error>> {
        use crate::morphism::{TheoryMorphism, check_morphism};
        use std::collections::HashMap;

        let nat1 = nat_theory();
        // Codomain theory: Nat' closed against [zero', succ']
        let nat_prime = Sort::closed(
            "Nat",
            Vec::new(),
            [Arc::from("zero2") as Arc<str>, Arc::from("succ2")],
        );
        let zero2 = Operation::nullary("zero2", "Nat");
        let succ2 = Operation::unary("succ2", "n", "Nat", "Nat");
        let nat2 = Theory::new("NatTh2", vec![nat_prime], vec![zero2, succ2], Vec::new());

        let mut sort_map = HashMap::new();
        sort_map.insert(Arc::from("Nat"), Arc::from("Nat"));
        let mut op_map = HashMap::new();
        op_map.insert(Arc::from("zero"), Arc::from("zero2"));
        op_map.insert(Arc::from("succ"), Arc::from("succ2"));
        let m = TheoryMorphism::new("m", "NatTh", "NatTh2", sort_map, op_map);
        check_morphism(&m, &nat1, &nat2)?;

        // Now swap succ2 to an unrelated op in the codomain closure and
        // verify the morphism check fails.
        let nat_prime_bad = Sort::closed(
            "Nat",
            Vec::new(),
            [Arc::from("zero2") as Arc<str>, Arc::from("other")],
        );
        let other = Operation::unary("other", "n", "Nat", "Nat");
        let nat2_bad = Theory::new(
            "NatTh2",
            vec![nat_prime_bad],
            vec![Operation::nullary("zero2", "Nat"), other],
            Vec::new(),
        );
        let result = check_morphism(&m, &nat1, &nat2_bad);
        assert!(
            matches!(result, Err(GatError::MorphismClosureMismatch { .. })),
            "got {result:?}"
        );
        Ok(())
    }

    #[test]
    fn case_term_substitution_respects_binder_shadow() {
        // case n of zero => m | succ(m) => m end
        // Under substitution [m := succ(zero())], the zero branch's
        // body becomes succ(zero()), but the succ branch's body is
        // shadowed by its binder `m` and remains `m`.
        let term = Term::Case {
            scrutinee: Box::new(Term::var("n")),
            branches: vec![
                CaseBranch {
                    constructor: Arc::from("zero"),
                    binders: Vec::new(),
                    body: Term::var("m"),
                },
                CaseBranch {
                    constructor: Arc::from("succ"),
                    binders: vec![Arc::from("m")],
                    body: Term::var("m"),
                },
            ],
        };
        let mut subst = FxHashMap::default();
        subst.insert(
            Arc::from("m"),
            Term::app("succ", vec![Term::constant("zero")]),
        );
        let result = term.substitute(&subst);
        let Term::Case { branches, .. } = &result else {
            panic!("expected Case, got {result:?}");
        };
        assert_eq!(
            branches[0].body,
            Term::app("succ", vec![Term::constant("zero")]),
            "zero branch body should be substituted"
        );
        assert_eq!(
            branches[1].body,
            Term::var("m"),
            "succ branch body must be shadowed, body stays `m`"
        );
    }

    // --- implicit arg inference (1.1) ---

    /// Build a minimal lambda-calculus-style theory with an `app`
    /// operation whose first two arguments are implicit type witnesses.
    ///
    /// Sorts: `Ty` (simple), `Tm(t : Ty)` (dependent).
    /// Ops:
    /// - `arrow : (a : Ty, b : Ty) -> Ty`
    /// - `app : {a : Ty}{b : Ty}(f : Tm(arrow(a, b)), x : Tm(a)) -> Tm(b)`
    fn lambda_theory() -> Theory {
        use crate::op::Implicit;
        let ty = Sort::simple("Ty");
        let tm = Sort::dependent("Tm", vec![SortParam::new("t", "Ty")]);
        let arrow = Operation::new(
            "arrow",
            vec![
                (Arc::from("a"), SortExpr::from("Ty")),
                (Arc::from("b"), SortExpr::from("Ty")),
            ],
            "Ty",
        );
        let tm_a = SortExpr::App {
            name: Arc::from("Tm"),
            args: vec![Term::var("a")],
        };
        let tm_b = SortExpr::App {
            name: Arc::from("Tm"),
            args: vec![Term::var("b")],
        };
        let tm_arrow = SortExpr::App {
            name: Arc::from("Tm"),
            args: vec![Term::app("arrow", vec![Term::var("a"), Term::var("b")])],
        };
        let app = Operation::with_implicit(
            "app",
            vec![
                (Arc::from("a"), SortExpr::from("Ty"), Implicit::Yes),
                (Arc::from("b"), SortExpr::from("Ty"), Implicit::Yes),
                (Arc::from("f"), tm_arrow, Implicit::No),
                (Arc::from("x"), tm_a, Implicit::No),
            ],
            tm_b,
        );
        Theory::new("Lambda", vec![ty, tm], vec![arrow, app], Vec::new())
    }

    #[test]
    fn app_with_inferred_implicit_types() -> Result<(), Box<dyn std::error::Error>> {
        let theory = lambda_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("A"), SortExpr::from("Ty"));
        ctx.insert(Arc::from("B"), SortExpr::from("Ty"));
        ctx.insert(
            Arc::from("f"),
            SortExpr::App {
                name: Arc::from("Tm"),
                args: vec![Term::app("arrow", vec![Term::var("A"), Term::var("B")])],
            },
        );
        ctx.insert(
            Arc::from("x"),
            SortExpr::App {
                name: Arc::from("Tm"),
                args: vec![Term::var("A")],
            },
        );
        // Call `app(f, x)` without the implicit type witnesses.
        let result = typecheck_term(
            &Term::app("app", vec![Term::var("f"), Term::var("x")]),
            &ctx,
            &theory,
        )?;
        let expected = SortExpr::App {
            name: Arc::from("Tm"),
            args: vec![Term::var("B")],
        };
        assert!(result.alpha_eq(&expected), "got {result}");
        Ok(())
    }

    #[test]
    fn implicit_inference_rejects_overconstrained_call() {
        use crate::op::Implicit;
        // Build a theory extended with two ground `Ty` constants
        // `first_ty` and `second_ty`. A call to `app(f, x)` where f's
        // domain is the first but x's type is the second must fail
        // unification of the implicit `a`.
        let type_decl = Sort::simple("Ty");
        let term_decl = Sort::dependent("Tm", vec![SortParam::new("t", "Ty")]);
        let first_ty = Operation::nullary("tyA", "Ty");
        let second_ty = Operation::nullary("tyB", "Ty");
        let arrow = Operation::new(
            "arrow",
            vec![
                (Arc::from("a"), SortExpr::from("Ty")),
                (Arc::from("b"), SortExpr::from("Ty")),
            ],
            "Ty",
        );
        let tm_of_a = SortExpr::App {
            name: Arc::from("Tm"),
            args: vec![Term::var("a")],
        };
        let tm_of_b = SortExpr::App {
            name: Arc::from("Tm"),
            args: vec![Term::var("b")],
        };
        let tm_of_arrow = SortExpr::App {
            name: Arc::from("Tm"),
            args: vec![Term::app("arrow", vec![Term::var("a"), Term::var("b")])],
        };
        let app = Operation::with_implicit(
            "app",
            vec![
                (Arc::from("a"), SortExpr::from("Ty"), Implicit::Yes),
                (Arc::from("b"), SortExpr::from("Ty"), Implicit::Yes),
                (Arc::from("f"), tm_of_arrow, Implicit::No),
                (Arc::from("x"), tm_of_a, Implicit::No),
            ],
            tm_of_b,
        );
        let theory = Theory::new(
            "LambdaGround",
            vec![type_decl, term_decl],
            vec![first_ty, second_ty, arrow, app],
            Vec::new(),
        );

        let mut ctx = VarContext::default();
        ctx.insert(
            Arc::from("f"),
            SortExpr::App {
                name: Arc::from("Tm"),
                args: vec![Term::app(
                    "arrow",
                    vec![Term::constant("tyA"), Term::constant("tyB")],
                )],
            },
        );
        ctx.insert(
            Arc::from("x"),
            SortExpr::App {
                name: Arc::from("Tm"),
                args: vec![Term::constant("tyB")],
            },
        );
        let result = typecheck_term(
            &Term::app("app", vec![Term::var("f"), Term::var("x")]),
            &ctx,
            &theory,
        );
        assert!(
            matches!(result, Err(GatError::SortUnificationFailure { .. })),
            "overconstrained implicit inference must fail: got {result:?}",
        );
    }

    #[test]
    fn implicit_declaration_rejected_when_not_inferrable() {
        use crate::op::Implicit;
        // Op with implicit param `c` that appears in neither any
        // explicit input sort nor the output sort: not inferrable.
        let foo = Operation::with_implicit(
            "foo",
            vec![
                (Arc::from("a"), SortExpr::from("Ty"), Implicit::No),
                (Arc::from("c"), SortExpr::from("Ty"), Implicit::Yes),
            ],
            SortExpr::from("Ty"),
        );
        let theory = Theory::new(
            "BadImplicit",
            vec![Sort::simple("Ty")],
            vec![foo],
            Vec::new(),
        );
        let result = typecheck_theory(&theory);
        assert!(
            matches!(result, Err(GatError::NonInferrableImplicit { .. })),
            "non-inferrable implicit must be rejected: got {result:?}",
        );
    }

    #[test]
    fn app_without_implicits_still_typechecks() -> Result<(), Box<dyn std::error::Error>> {
        // Sanity check that operations with no implicit inputs still
        // traverse the old explicit-theta path.
        let theory = category_theory();
        let mut ctx = VarContext::default();
        ctx.insert(Arc::from("x"), SortExpr::from("Ob"));
        let result = typecheck_term(&Term::app("id", vec![Term::var("x")]), &ctx, &theory)?;
        assert_eq!(&**result.head(), "Hom");
        Ok(())
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

        /// Generate a closed `Nat` theory plus a random list of
        /// constructor names chosen from `{zero, succ}` as branch
        /// constructors, possibly with missing/duplicate/unknown ones.
        fn arb_case_on_nat() -> impl Strategy<Value = (Theory, Vec<Arc<str>>)> {
            let nat = Sort::closed(
                "Nat",
                Vec::new(),
                [Arc::from("zero") as Arc<str>, Arc::from("succ")],
            );
            let zero = Operation::nullary("zero", "Nat");
            let succ = Operation::unary("succ", "n", "Nat", "Nat");
            let theory = Theory::new("NatTh", vec![nat], vec![zero, succ], Vec::new());
            (
                Just(theory),
                prop::collection::vec(
                    prop::sample::select(vec![
                        Arc::from("zero"),
                        Arc::from("succ"),
                        Arc::from("bogus"),
                    ] as Vec<Arc<str>>),
                    0..=3,
                ),
            )
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn case_on_closed_sort_never_panics(
                (theory, ctors) in arb_case_on_nat()
            ) {
                let mut ctx = VarContext::default();
                ctx.insert(Arc::from("n"), SortExpr::from("Nat"));
                let branches: Vec<CaseBranch> = ctors
                    .into_iter()
                    .map(|c| CaseBranch {
                        constructor: c,
                        binders: Vec::new(),
                        body: Term::constant("zero"),
                    })
                    .collect();
                let term = Term::Case {
                    scrutinee: Box::new(Term::var("n")),
                    branches,
                };
                // Must return a well-typed sort or a specific GatError
                // variant; it must not panic.
                let r = typecheck_term(&term, &ctx, &theory);
                match r {
                    Ok(_)
                    | Err(
                        GatError::NonExhaustiveCase { .. }
                        | GatError::RedundantCaseBranch { .. }
                        | GatError::UnknownCaseConstructor { .. }
                        | GatError::OpTypeMismatch { .. }
                        | GatError::TermArityMismatch { .. },
                    ) => {}
                    other => prop_assert!(false, "unexpected result: {other:?}"),
                }
            }

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
            fn implicit_inference_stable_across_names(
                a_name in prop::sample::select(&["A", "B", "C", "P", "Q"][..]).prop_map(Arc::from),
                b_name in prop::sample::select(&["A", "B", "C", "P", "Q"][..]).prop_map(Arc::from),
            ) {
                use crate::op::Implicit;
                // Theory with `arrow` and implicit-argument `app`. For
                // any choice of two ground `Ty` constants in ctx, the
                // inferred output sort is `Tm(b)` with `b` bound to the
                // codomain of the function's declared arrow. Running
                // typecheck twice yields the same sort.
                let ty = Sort::simple("Ty");
                let tm = Sort::dependent("Tm", vec![SortParam::new("t", "Ty")]);
                let arrow = Operation::new(
                    "arrow",
                    vec![
                        (Arc::from("a"), SortExpr::from("Ty")),
                        (Arc::from("b"), SortExpr::from("Ty")),
                    ],
                    "Ty",
                );
                let tm_a = SortExpr::App {
                    name: Arc::from("Tm"),
                    args: vec![Term::var("a")],
                };
                let tm_b = SortExpr::App {
                    name: Arc::from("Tm"),
                    args: vec![Term::var("b")],
                };
                let tm_arrow = SortExpr::App {
                    name: Arc::from("Tm"),
                    args: vec![Term::app(
                        "arrow",
                        vec![Term::var("a"), Term::var("b")],
                    )],
                };
                let app = Operation::with_implicit(
                    "app",
                    vec![
                        (Arc::from("a"), SortExpr::from("Ty"), Implicit::Yes),
                        (Arc::from("b"), SortExpr::from("Ty"), Implicit::Yes),
                        (Arc::from("f"), tm_arrow, Implicit::No),
                        (Arc::from("x"), tm_a, Implicit::No),
                    ],
                    tm_b,
                );
                let theory = Theory::new("Lambda", vec![ty, tm], vec![arrow, app], Vec::new());

                let mut ctx = VarContext::default();
                ctx.insert(Arc::clone(&a_name), SortExpr::from("Ty"));
                if a_name != b_name {
                    ctx.insert(Arc::clone(&b_name), SortExpr::from("Ty"));
                }
                ctx.insert(
                    Arc::from("f"),
                    SortExpr::App {
                        name: Arc::from("Tm"),
                        args: vec![Term::app(
                            "arrow",
                            vec![Term::Var(Arc::clone(&a_name)), Term::Var(Arc::clone(&b_name))],
                        )],
                    },
                );
                ctx.insert(
                    Arc::from("x"),
                    SortExpr::App {
                        name: Arc::from("Tm"),
                        args: vec![Term::Var(Arc::clone(&a_name))],
                    },
                );
                let call = Term::app("app", vec![Term::var("f"), Term::var("x")]);
                let s1 = typecheck_term(&call, &ctx, &theory);
                let s2 = typecheck_term(&call, &ctx, &theory);
                prop_assert_eq!(s1.is_ok(), s2.is_ok());
                if let (Ok(a), Ok(b)) = (&s1, &s2) {
                    prop_assert!(a.alpha_eq(b));
                }
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
