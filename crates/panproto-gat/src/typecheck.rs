//! Term type-checking for GAT expressions.
//!
//! Verifies that terms are well-typed with respect to a theory's operation
//! signatures. Each operation is allowed to produce a dependent output
//! sort whose parameters reference the input argument terms: when an
//! operation `f : (x1 : S1, ..., xn : Sn) -> T` is applied to concrete
//! terms `a1, ..., an`, the result sort is `T[x1 := a1, ..., xn := an]`
//! (Cartmell-style substitution into the sort expression).

use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use serde::{Deserialize, Serialize};

use crate::compose::SymKind;
use crate::eq::{CaseBranch, Equation, Term};
use crate::error::GatError;
use crate::op::{Implicit, Operation};
use crate::sort::{SortClosure, SortExpr, sort_exprs_agree};
use crate::theory::Theory;

/// A sort scheme: a sort expression universally quantified over a list
/// of metavariable names.
///
/// Retained for future extension. GAT signatures are first-order and
/// have no free sort-metavariables, so [`typecheck_term`] never
/// constructs a non-empty `metavars` list at the present time; every
/// scheme encountered in practice is monomorphic.
#[derive(Debug, Clone)]
pub struct SortScheme {
    /// Universally-quantified metavariable names.
    pub metavars: Vec<Arc<str>>,
    /// The scheme's body.
    pub body: SortExpr,
}

impl SortScheme {
    /// Build a monomorphic scheme from a sort expression.
    #[must_use]
    pub const fn mono(body: SortExpr) -> Self {
        Self {
            metavars: Vec::new(),
            body,
        }
    }

    /// Instantiate the scheme by freshening each metavariable and
    /// substituting the fresh name into the scheme's body.
    ///
    /// Fresh names are derived by suffixing `counter` to each
    /// metavariable, so two calls with different counters produce
    /// distinct sort expressions.
    #[must_use]
    pub fn instantiate(&self, counter: usize) -> SortExpr {
        if self.metavars.is_empty() {
            return self.body.clone();
        }
        let mut subst: FxHashMap<Arc<str>, crate::eq::Term> = FxHashMap::default();
        for mv in &self.metavars {
            let fresh: Arc<str> = Arc::from(format!("{mv}_inst_{counter}"));
            subst.insert(Arc::clone(mv), crate::eq::Term::Var(fresh));
        }
        self.body.subst(&subst)
    }
}

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
    check_term_opt(term, None, ctx, theory)
}

/// Typecheck `term` against a known expected sort.
///
/// Inference alone cannot type an eliminator whose result sort mentions
/// the index it eliminates. A `Case` has no motive of its own, so with
/// nothing expected the only sort available is the one read off the
/// first branch, and every later branch is then required to agree with
/// it. For `eval : (t: Ty, Expr(t)) -> El(t)` the branches produce
/// `El(int_code)` and `El(bool_code)`, which do not agree and should
/// not have to: each is correct under its own branch's refinement.
///
/// Given an expected sort, each branch body is instead checked against
/// that sort refined by the same substitution the branch already
/// applies to its binders, so the branches are compared to the motive
/// rather than to each other. The expected sort is the declared output
/// of whatever operation the equation defines, which is what makes the
/// motive available without putting one in the `Term` AST.
///
/// Every other term shape is inferred exactly as [`typecheck_term`]
/// does, and the returned sort is left for the caller to compare, so
/// this never rejects anything [`typecheck_term`] accepts.
///
/// # Errors
///
/// As [`typecheck_term`], plus [`GatError::CaseBranchSortMismatch`]
/// when a branch body does not have the expected sort under its own
/// refinement.
pub fn check_term(
    term: &Term,
    expected: &SortExpr,
    ctx: &VarContext,
    theory: &Theory,
) -> Result<SortExpr, GatError> {
    check_term_opt(term, Some(expected), ctx, theory)
}

/// Shared core of [`typecheck_term`] and [`check_term`]. `expected` is
/// `None` in inference mode.
fn check_term_opt(
    term: &Term,
    expected: Option<&SortExpr>,
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
            // fresh metavariable sort whose name is `?<name>` (or
            // `?hole` for anonymous holes). This string does not
            // round-trip through the DSL parser: callers that serialize
            // a term for re-parsing must use `typecheck_term_with_holes`
            // to collect hole reports and fill in their expected sorts
            // before emitting. Use `typecheck_term_with_holes` to
            // collect hole reports.
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
        } => typecheck_case(scrutinee, branches, expected, ctx, theory),

        Term::Let { name, bound, body } => {
            // Typecheck the bound term, extend the context with the
            // resulting sort, then typecheck the body. GAT sorts are
            // first-order with no free sort-metavariables, so there is
            // nothing to generalize over: the binding is always
            // monomorphic and the [`SortScheme`] produced at a
            // hypothetical generalization step always has an empty
            // `metavars` list. We therefore bind the inferred sort
            // directly rather than constructing a trivial scheme.
            let bound_sort = typecheck_term(bound, ctx, theory)?;
            let mut extended = ctx.clone();
            extended.insert(Arc::clone(name), bound_sort);
            // The body's sort is the `let`'s sort, so an expected sort
            // passes straight through to it.
            check_term_opt(body, expected, &extended, theory)
        }
    }
}

/// Typecheck a [`Term::Case`] expression, against `expected` when one
/// is known.
///
/// With an expected sort, each branch body is checked against it
/// refined by that branch's own substitution, which is what lets the
/// result sort mention the index being eliminated. Without one, the
/// first branch's sort fixes the case's sort and the rest must match
/// it, which is the only thing that can be done when no motive is
/// available.
fn typecheck_case(
    scrutinee: &Term,
    branches: &[CaseBranch],
    expected: Option<&SortExpr>,
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

    // Exhaustiveness over the constructors the scrutinee's index
    // admits, rather than over every constructor the sort declares.
    check_case_exhaustiveness(&scrutinee_sort, &constructors, branches, theory)?;

    // Typecheck each branch body. Against a known motive each is
    // checked under its own refinement; otherwise the first branch's
    // output sort fixes the case's sort and the rest must match it.
    let mut branch_sort: Option<SortExpr> = None;
    for b in branches {
        let constructor_op = theory
            .find_op(&b.constructor)
            .ok_or_else(|| GatError::OpNotFound(b.constructor.to_string()))?;
        let refinement = refine_branch(b, constructor_op, scrutinee, &scrutinee_sort)?;
        let subst = refinement.subst;
        let mut extended = ctx.clone();
        for (binder, binder_sort) in refinement.binder_sorts {
            extended.insert(binder, binder_sort);
        }
        if let Some(motive) = expected {
            // The same `subst` that refined the binders refines the
            // motive, so `El(t)` is `El(int_code)` in the `IntLit`
            // branch. Branches are compared to this, never to one
            // another.
            let refined = motive.subst(&subst);
            let body_sort = check_term(&b.body, &refined, &extended, theory)?;
            if !sort_exprs_agree(&body_sort, &[], &refined, &[]) {
                return Err(GatError::CaseBranchSortMismatch {
                    constructor: b.constructor.to_string(),
                    expected: refined.to_string(),
                    got: body_sort.to_string(),
                });
            }
            branch_sort = Some(motive.clone());
            continue;
        }
        let body_sort = typecheck_term(&b.body, &extended, theory)?;
        match &branch_sort {
            None => branch_sort = Some(body_sort),
            Some(existing) => {
                if !sort_exprs_agree(existing, &[], &body_sort, &[]) {
                    return Err(GatError::EquationSortMismatch {
                        equation: "case".to_string(),
                        lhs_sort: existing.to_string(),
                        rhs_sort: body_sort.to_string(),
                    });
                }
            }
        }
    }

    // Exhaustiveness passed with no branch, so the index admits no
    // constructor: the match is vacuous and there is no branch body to
    // read the result sort from.
    branch_sort.ok_or_else(|| GatError::CaseOnUninhabitedIndex {
        sort: sort_name.to_string(),
        scrutinee_sort: scrutinee_sort.to_string(),
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
        if !sort_exprs_agree(&arg_sort, &[], &expected, &[]) {
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
    let mut next_implicit_scope = 0;
    collect_constraints(
        &eq.lhs,
        theory,
        &mut ctx,
        &mut term_eqs,
        &mut next_implicit_scope,
    )?;
    collect_constraints(
        &eq.rhs,
        theory,
        &mut ctx,
        &mut term_eqs,
        &mut next_implicit_scope,
    )?;

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
    next_implicit_scope: &mut usize,
) -> Result<(), GatError> {
    let (op, args) = match term {
        Term::App { op, args } => (op, args),
        Term::Case {
            scrutinee,
            branches,
        } => {
            collect_constraints(scrutinee, theory, ctx, term_eqs, next_implicit_scope)?;
            for b in branches {
                collect_constraints(&b.body, theory, ctx, term_eqs, next_implicit_scope)?;
            }
            return Ok(());
        }
        Term::Let { bound, body, .. } => {
            collect_constraints(bound, theory, ctx, term_eqs, next_implicit_scope)?;
            collect_constraints(body, theory, ctx, term_eqs, next_implicit_scope)?;
            return Ok(());
        }
        Term::Var(_) | Term::Hole { .. } => return Ok(()),
    };
    let operation = theory
        .find_op(op)
        .ok_or_else(|| GatError::OpNotFound(op.to_string()))?;

    let explicit_count = operation.explicit_arity();
    if args.len() != explicit_count {
        return Err(GatError::TermArityMismatch {
            op: op.to_string(),
            expected: explicit_count,
            got: args.len(),
        });
    }

    let mut theta: FxHashMap<Arc<str>, Term> = FxHashMap::default();
    let implicit_scope = *next_implicit_scope;
    *next_implicit_scope += 1;
    for (index, (param_name, _, implicit)) in operation.inputs.iter().enumerate() {
        if matches!(implicit, Implicit::Yes) {
            let metavariable: Arc<str> =
                Arc::from(format!("?{op}_{param_name}_{implicit_scope}_{index}"));
            theta.insert(Arc::clone(param_name), Term::Var(metavariable));
        }
    }

    let mut explicit_args = args.iter();
    for (param_name, declared_sort, implicit) in &operation.inputs {
        if matches!(implicit, Implicit::Yes) {
            continue;
        }
        let Some(arg) = explicit_args.next() else {
            return Err(GatError::TermArityMismatch {
                op: op.to_string(),
                expected: explicit_count,
                got: args.len(),
            });
        };
        let expected = declared_sort.subst(&theta);
        match arg {
            Term::Var(var_name) => {
                if let Some(existing) = ctx.get(var_name).cloned() {
                    unify_sort_exprs(&existing, &expected, var_name, term_eqs)?;
                } else {
                    ctx.insert(Arc::clone(var_name), expected);
                }
            }
            Term::App { .. } | Term::Case { .. } | Term::Hole { .. } | Term::Let { .. } => {
                collect_constraints(arg, theory, ctx, term_eqs, next_implicit_scope)?;
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
        Term::Let { name, bound, body } => {
            occurs_in(var, bound) || (name != var && occurs_in(var, body))
        }
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
                // Implicit-inference path: thread the expected sort
                // through each explicit arg so that holes attach to the
                // right expected sort, then recover the implicit
                // parameter values via the same unification machinery
                // as typecheck_term.
                typecheck_app_with_implicits_collecting_holes(
                    op, args, operation, ctx, theory, reports,
                )
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
                    // so the sorts agree trivially. For non-hole terms,
                    // compare them up to the names of local binders.
                    if !term_contains_hole(arg)
                        && !sort_exprs_agree(&arg_sort, &[], &expected_sort, &[])
                    {
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
        } => typecheck_case_with_holes(scrutinee, branches, expected, ctx, theory, reports),
        Term::Let { name, bound, body } => {
            let bound_sort = typecheck_with_expected(bound, None, ctx, theory, reports)?;
            let mut extended = ctx.clone();
            extended.insert(Arc::clone(name), bound_sort);
            typecheck_with_expected(body, None, &extended, theory, reports)
        }
    }
}

fn typecheck_case_with_holes(
    scrutinee: &Term,
    branches: &[CaseBranch],
    expected: Option<&SortExpr>,
    ctx: &VarContext,
    theory: &Theory,
    reports: &mut Vec<HoleReport>,
) -> Result<SortExpr, GatError> {
    let scrutinee_sort = typecheck_with_expected(scrutinee, None, ctx, theory, reports)?;
    let admitted = check_case_exhaustiveness_soft(&scrutinee_sort, branches, theory)?;
    let mut branch_sort: Option<SortExpr> = None;
    for b in branches {
        let constructor_op = theory
            .find_op(&b.constructor)
            .ok_or_else(|| GatError::OpNotFound(b.constructor.to_string()))?;
        // The same refinement as the strict path, so the two cannot
        // disagree about what a branch knows.
        let refinement = refine_branch(b, constructor_op, scrutinee, &scrutinee_sort)?;
        let subst = refinement.subst;
        let mut extended = ctx.clone();
        for (binder, binder_sort) in refinement.binder_sorts {
            extended.insert(binder, binder_sort);
        }
        if let Some(motive) = expected {
            // Mirror the strict path: compare each branch to the motive
            // refined by that branch's substitution. A hole in a branch
            // body then reports the sort the motive requires there
            // rather than a metavariable.
            let refined = motive.subst(&subst);
            let body_sort =
                typecheck_with_expected(&b.body, Some(&refined), &extended, theory, reports)?;
            if !term_contains_hole(&b.body) && !sort_exprs_agree(&body_sort, &[], &refined, &[]) {
                return Err(GatError::CaseBranchSortMismatch {
                    constructor: b.constructor.to_string(),
                    expected: refined.to_string(),
                    got: body_sort.to_string(),
                });
            }
            branch_sort = Some(motive.clone());
            continue;
        }
        let body_sort = typecheck_with_expected(&b.body, None, &extended, theory, reports)?;
        match &branch_sort {
            None => branch_sort = Some(body_sort),
            Some(existing) => {
                // Require every branch body to produce an
                // alpha-equivalent output sort. Mirror the strict
                // typecheck_case behaviour.
                if !sort_exprs_agree(existing, &[], &body_sort, &[]) {
                    return Err(GatError::EquationSortMismatch {
                        equation: "case".to_string(),
                        lhs_sort: existing.to_string(),
                        rhs_sort: body_sort.to_string(),
                    });
                }
            }
        }
    }
    branch_sort.ok_or_else(|| {
        // A checked closed sort reaching here admits no constructor,
        // so the match is vacuous rather than short of a branch.
        if admitted.is_some_and(|a| a.is_empty()) {
            GatError::CaseOnUninhabitedIndex {
                sort: scrutinee_sort.head().to_string(),
                scrutinee_sort: scrutinee_sort.to_string(),
            }
        } else {
            GatError::NonExhaustiveCase {
                sort: scrutinee_sort.head().to_string(),
                missing: Vec::new(),
            }
        }
    })
}

/// What one branch learns: the substitution its constructor's pattern
/// imposes on the names in scope, and the sorts its binders take.
struct BranchRefinement {
    subst: FxHashMap<Arc<str>, Term>,
    binder_sorts: Vec<(Arc<str>, SortExpr)>,
}

/// Refine the world for one branch of a case on `scrutinee`.
///
/// The constructor's signature is written in its own parameter names,
/// and neither those nor the branch's binder names can be unified
/// against the scrutinee's sort directly. A constructor's `n` may
/// coincide with an eliminator's `n` free in that sort, and a binder may
/// legitimately shadow such a name: `len (m, v : Vec(m))` with a branch
/// `cons m x rest` is ordinary, and either spelling would unify `succ(m)
/// ~ m` and fail the occurs check, reporting the one branch that can
/// exist as unreachable.
///
/// So the constructor is first renamed to fresh names no user can write,
/// its output is unified with the scrutinee's sort in those names, and
/// only then are the fresh names mapped to the branch's binders. That
/// unification is the index refinement: matching `v : Vec(n)` against
/// `cons` learns `n := succ(m)`, with `m` the binder.
///
/// A scrutinee that is a variable is also refined to the constructor's
/// pattern. That is the other half of dependent matching: a case on `n :
/// Nat` learns `n := zero()` in the `zero` branch, which is what lets a
/// motive `Vec(n)` become `Vec(zero())` there. A sort with no indices
/// gives unification nothing to learn, so without this step `replicate :
/// (n : Nat, x : A) -> Vec(n)` could not be written.
///
/// The returned substitution is over the names in scope outside the
/// branch and is meant to be applied once, to the motive; the binder
/// sorts are already computed.
fn refine_branch(
    branch: &CaseBranch,
    constructor_op: &Operation,
    scrutinee: &Term,
    scrutinee_sort: &SortExpr,
) -> Result<BranchRefinement, GatError> {
    if constructor_op.inputs.len() != branch.binders.len() {
        return Err(GatError::TermArityMismatch {
            op: branch.constructor.to_string(),
            expected: constructor_op.inputs.len(),
            got: branch.binders.len(),
        });
    }
    let fresh = rename_apart(constructor_op);
    let to_binder: FxHashMap<Arc<str>, Term> = constructor_op
        .inputs
        .iter()
        .zip(branch.binders.iter())
        .map(|((param, _, _), binder)| (fresh_name(param), Term::Var(Arc::clone(binder))))
        .collect();

    let output = constructor_op.output.subst(&fresh);
    if output.head() != scrutinee_sort.head() || output.args().len() != scrutinee_sort.args().len()
    {
        return Err(GatError::OpTypeMismatch {
            op: branch.constructor.to_string(),
            detail: format!(
                "constructor output sort {} does not match scrutinee sort {scrutinee_sort}",
                constructor_op.output
            ),
        });
    }
    let unify_eqs: Vec<(Term, Term)> = output
        .args()
        .iter()
        .zip(scrutinee_sort.args().iter())
        .map(|(a, b)| (a.clone(), b.clone()))
        .collect();
    let in_fresh = unify_all(unify_eqs)?;

    // Binder sorts: the constructor's inputs in fresh names, under the
    // unifier, then in the branch's own names.
    let binder_sorts = constructor_op
        .inputs
        .iter()
        .zip(branch.binders.iter())
        .map(|((_, declared, _), binder)| {
            (
                Arc::clone(binder),
                declared.subst(&fresh).subst(&in_fresh).subst(&to_binder),
            )
        })
        .collect();

    // The unifier for the outside, with its values in the branch's names.
    // A key that is a fresh name is a constructor parameter the
    // scrutinee's index determined; it keeps its fresh name, which nothing
    // outside the branch can mention. Renaming it to its binder would let
    // a binder that shadows an outer name rewrite that outer name.
    let mut subst: FxHashMap<Arc<str>, Term> = in_fresh
        .into_iter()
        .map(|(key, value)| (key, value.substitute(&to_binder)))
        .collect();

    if let Term::Var(name) = scrutinee {
        let pattern = Term::App {
            op: Arc::clone(&branch.constructor),
            args: constructor_op
                .inputs
                .iter()
                .zip(branch.binders.iter())
                .filter(|((_, _, implicit), _)| matches!(implicit, Implicit::No))
                .map(|(_, binder)| Term::Var(Arc::clone(binder)))
                .collect(),
        };
        let single: FxHashMap<Arc<str>, Term> =
            std::iter::once((Arc::clone(name), pattern.clone())).collect();
        for value in subst.values_mut() {
            *value = value.substitute(&single);
        }
        subst.insert(Arc::clone(name), pattern);
    }

    Ok(BranchRefinement {
        subst,
        binder_sorts,
    })
}

/// The name a constructor parameter is renamed to while its signature
/// is unified against a scrutinee sort. `'` is not an identifier
/// character in any surface that reaches this crate, so the result
/// cannot collide with a name a user wrote, and it reads as the usual
/// mathematical prime in a diagnostic.
fn fresh_name(param: &Arc<str>) -> Arc<str> {
    Arc::from(format!("{param}'"))
}

/// A substitution renaming a constructor's parameters to names no user
/// can write, for unifying its signature against a scrutinee sort without
/// capture when there are no binders to rename to.
fn rename_apart(constructor: &Operation) -> FxHashMap<Arc<str>, Term> {
    constructor
        .inputs
        .iter()
        .map(|(param, _, _)| (Arc::clone(param), Term::Var(Arc::from(format!("{param}'")))))
        .collect()
}

/// Whether the scrutinee's sort admits `constructor`, i.e. whether
/// some inhabitant of that sort could have been built by it.
///
/// A constructor's output sort carries the index its results have. For
/// a sort with no parameters that index is empty and every constructor
/// of the sort qualifies, so this reduces to a head comparison. For an
/// indexed family it is the refinement dependent pattern matching turns
/// on: `nil : Vec(zero)` cannot have built a `Vec(succ n)`, so a case
/// on one needs no `nil` branch and admits none.
///
/// The test is exactly the unification the branch path runs to
/// instantiate a constructor's parameters, so a constructor reported
/// admissible here is one that path can typecheck. That is what keeps
/// the two checks from disagreeing about whether a branch exists.
fn constructor_admitted(constructor: &Operation, scrutinee_sort: &SortExpr) -> bool {
    if constructor.output.head() != scrutinee_sort.head()
        || constructor.output.args().len() != scrutinee_sort.args().len()
    {
        return false;
    }
    // The constructor's signature is written in its own parameter names,
    // and those can coincide with names free in the scrutinee's sort:
    // `cons : Vec(succ(n))` against a scrutinee of sort `Vec(n)` would
    // unify `succ(n) ~ n` and fail the occurs check, reporting the one
    // branch that can exist as unreachable. Rename the constructor's
    // parameters apart first. The names only need to be distinct from
    // anything a user can write, and `'` is not an identifier character.
    let apart = rename_apart(constructor);
    let output = constructor.output.subst(&apart);
    let eqs: Vec<(Term, Term)> = output
        .args()
        .iter()
        .cloned()
        .zip(scrutinee_sort.args().iter().cloned())
        .collect();
    unify_all(eqs).is_ok()
}

/// The subset of a closed sort's constructors that `scrutinee_sort`
/// admits, in the order the sort declares them.
fn admitted_constructors(
    ctors: &[Arc<str>],
    scrutinee_sort: &SortExpr,
    theory: &Theory,
) -> Result<Vec<Arc<str>>, GatError> {
    let mut admitted = Vec::with_capacity(ctors.len());
    for c in ctors {
        let op = theory
            .find_op(c)
            .ok_or_else(|| GatError::OpNotFound(c.to_string()))?;
        if constructor_admitted(op, scrutinee_sort) {
            admitted.push(Arc::clone(c));
        }
    }
    Ok(admitted)
}

/// Check that a case's branches cover exactly the constructors the
/// scrutinee's sort admits: each exactly once, none unreachable, and
/// none missing.
///
/// The three failures are distinguished because they call for different
/// edits. An unknown constructor does not belong to the sort at all; an
/// unreachable one belongs to it but not at this index, and the branch
/// should be deleted; a missing one needs a branch written.
fn check_case_exhaustiveness(
    scrutinee_sort: &SortExpr,
    ctors: &[Arc<str>],
    branches: &[CaseBranch],
    theory: &Theory,
) -> Result<Vec<Arc<str>>, GatError> {
    let sort_name = scrutinee_sort.head();
    let admitted = admitted_constructors(ctors, scrutinee_sort, theory)?;
    let mut seen: FxHashSet<Arc<str>> = FxHashSet::default();
    for b in branches {
        if !ctors.contains(&b.constructor) {
            return Err(GatError::UnknownCaseConstructor {
                sort: sort_name.to_string(),
                constructor: b.constructor.to_string(),
            });
        }
        if !admitted.contains(&b.constructor) {
            let op = theory
                .find_op(&b.constructor)
                .ok_or_else(|| GatError::OpNotFound(b.constructor.to_string()))?;
            return Err(GatError::UnreachableCaseBranch {
                sort: sort_name.to_string(),
                constructor: b.constructor.to_string(),
                constructor_sort: op.output.to_string(),
                scrutinee_sort: scrutinee_sort.to_string(),
            });
        }
        if !seen.insert(Arc::clone(&b.constructor)) {
            return Err(GatError::RedundantCaseBranch {
                sort: sort_name.to_string(),
                constructor: b.constructor.to_string(),
            });
        }
    }
    if seen.len() < admitted.len() {
        let missing: Vec<String> = admitted
            .iter()
            .filter(|c| !seen.contains(*c))
            .map(ToString::to_string)
            .collect();
        return Err(GatError::NonExhaustiveCase {
            sort: sort_name.to_string(),
            missing,
        });
    }
    Ok(admitted)
}

/// The hole-tolerant path's exhaustiveness check.
///
/// Returns the admitted constructors when the scrutinee's sort is
/// declared and closed, and `None` when it is neither, in which case
/// nothing was checked. A term under inference may name a sort the
/// theory has not declared yet, so this stays lenient where the strict
/// path errors.
fn check_case_exhaustiveness_soft(
    scrutinee_sort: &SortExpr,
    branches: &[CaseBranch],
    theory: &Theory,
) -> Result<Option<Vec<Arc<str>>>, GatError> {
    let Some(sort_decl) = theory.find_sort(scrutinee_sort.head()) else {
        return Ok(None);
    };
    let SortClosure::Closed(ctors) = &sort_decl.closure else {
        return Ok(None);
    };
    check_case_exhaustiveness(scrutinee_sort, ctors, branches, theory).map(Some)
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
    let hole_count = count_holes(&eq.lhs) + count_holes(&eq.rhs);
    if hole_count > 0 {
        return Err(GatError::HolesInEquation {
            equation: eq.name.to_string(),
            count: hole_count,
        });
    }
    let ctx = infer_var_sorts(eq, theory)?;
    // Check one side against the other's sort rather than inferring
    // both: the declared output sort of the operation being defined is
    // the motive a `Case` needs, and it reaches the `Case` only if it
    // is pushed in. Whichever side is not itself a `Case` is inferred
    // first, so the direction does not matter to the writer.
    let (lhs_sort, rhs_sort) =
        if matches!(eq.lhs, Term::Case { .. }) && !matches!(eq.rhs, Term::Case { .. }) {
            let rhs_sort = typecheck_term(&eq.rhs, &ctx, theory)?;
            let lhs_sort = check_term(&eq.lhs, &rhs_sort, &ctx, theory)?;
            (lhs_sort, rhs_sort)
        } else {
            let lhs_sort = typecheck_term(&eq.lhs, &ctx, theory)?;
            let rhs_sort = check_term(&eq.rhs, &lhs_sort, &ctx, theory)?;
            (lhs_sort, rhs_sort)
        };
    if !sort_exprs_agree(&lhs_sort, &[], &rhs_sort, &[]) {
        return Err(GatError::EquationSortMismatch {
            equation: eq.name.to_string(),
            lhs_sort: lhs_sort.to_string(),
            rhs_sort: rhs_sort.to_string(),
        });
    }
    Ok(())
}

/// Typecheck an equation with sort equality relaxed modulo a set of
/// directed rewrite rules.
///
/// Differs from [`typecheck_equation`] only in the final comparison
/// step: the inferred LHS and RHS sorts are considered equal when they
/// match under [`SortExpr::alpha_eq_modulo_rewrites`] with the given
/// rules and step budget. Useful in dependent-type settings where
/// judgmental equality holds modulo the theory's own rewrite system.
///
/// # Errors
///
/// Same as [`typecheck_equation`] except that the mismatch check uses
/// the relaxed equality.
pub fn typecheck_equation_modulo_rewrites(
    eq: &Equation,
    theory: &Theory,
    rules: &[crate::eq::DirectedEquation],
    step_limit: usize,
) -> Result<(), GatError> {
    let hole_count = count_holes(&eq.lhs) + count_holes(&eq.rhs);
    if hole_count > 0 {
        return Err(GatError::HolesInEquation {
            equation: eq.name.to_string(),
            count: hole_count,
        });
    }
    let ctx = infer_var_sorts(eq, theory)?;
    // Check one side against the other's sort rather than inferring
    // both: the declared output sort of the operation being defined is
    // the motive a `Case` needs, and it reaches the `Case` only if it
    // is pushed in. Whichever side is not itself a `Case` is inferred
    // first, so the direction does not matter to the writer.
    let (lhs_sort, rhs_sort) =
        if matches!(eq.lhs, Term::Case { .. }) && !matches!(eq.rhs, Term::Case { .. }) {
            let rhs_sort = typecheck_term(&eq.rhs, &ctx, theory)?;
            let lhs_sort = check_term(&eq.lhs, &rhs_sort, &ctx, theory)?;
            (lhs_sort, rhs_sort)
        } else {
            let lhs_sort = typecheck_term(&eq.lhs, &ctx, theory)?;
            let rhs_sort = check_term(&eq.rhs, &lhs_sort, &ctx, theory)?;
            (lhs_sort, rhs_sort)
        };
    let (equal, exhausted) = lhs_sort.alpha_eq_modulo_rewrites_status(&rhs_sort, rules, step_limit);
    if !equal {
        if exhausted {
            return Err(GatError::RewriteBudgetExhausted {
                context: format!("equation `{}` sort equality", eq.name),
                limit: step_limit,
            });
        }
        return Err(GatError::EquationSortMismatch {
            equation: eq.name.to_string(),
            lhs_sort: lhs_sort.to_string(),
            rhs_sort: rhs_sort.to_string(),
        });
    }
    Ok(())
}

/// Where in a theory's signature a [`SignatureDefect`] sits.
///
/// Serialized externally tagged in `snake_case`, for instance
/// `{"op_input": {"op": "id", "index": 0}}`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureSite {
    /// The sort of parameter `index` of the dependent sort `sort`.
    SortParam {
        /// The sort whose parameter this is.
        sort: Arc<str>,
        /// The parameter's position.
        index: usize,
    },
    /// The sort of input `index` of the operation `op`.
    OpInput {
        /// The operation.
        op: Arc<str>,
        /// The input's position.
        index: usize,
    },
    /// The output sort of the operation `op`.
    OpOutput {
        /// The operation.
        op: Arc<str>,
    },
}

impl std::fmt::Display for SignatureSite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SortParam { sort, index } => write!(f, "parameter {index} of sort `{sort}`"),
            Self::OpInput { op, index } => write!(f, "input {index} of operation `{op}`"),
            Self::OpOutput { op } => write!(f, "output of operation `{op}`"),
        }
    }
}

/// One way a theory's signature can be ill formed.
///
/// Serialized externally tagged in `snake_case`, for instance
/// `{"undeclared_sort": {"site": {"op_output": {"op": "f"}}, "sort": "V"}}`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureDefect {
    /// A sort expression names a sort the theory does not declare, or one
    /// it declares only through a cycle of parameter dependencies, so that
    /// no dependency order puts the declaration before the use.
    UndeclaredSort {
        /// Where the sort is named.
        site: SignatureSite,
        /// The sort named.
        sort: Arc<str>,
    },
    /// A sort is applied to a number of arguments different from its
    /// number of parameters.
    SortArity {
        /// Where the application occurs.
        site: SignatureSite,
        /// The sort applied.
        sort: Arc<str>,
        /// Its declared number of parameters.
        expected: usize,
        /// The number of arguments given.
        found: usize,
    },
    /// An argument of a sort application does not have the sort of the
    /// parameter it instantiates.
    ParamIllTyped {
        /// Where the application occurs.
        site: SignatureSite,
        /// The sort applied.
        sort: Arc<str>,
        /// The argument's position.
        index: usize,
        /// The parameter's sort, with earlier arguments substituted.
        expected: Arc<str>,
        /// The argument's sort, or why it has none.
        found: Arc<str>,
    },
    /// A sort argument mentions a variable that no earlier parameter or
    /// input binds.
    UnboundParamVar {
        /// Where the argument occurs.
        site: SignatureSite,
        /// The unbound variable.
        var: Arc<str>,
    },
    /// Two symbols of one kind share a name.
    DuplicateSymbol {
        /// The kind of the duplicated symbols.
        kind: SymKind,
        /// Their shared name.
        name: Arc<str>,
    },
    /// A sort parameter, or an operation input, binds a name that an
    /// earlier parameter of the same sort, or an earlier input of the same
    /// operation, already binds.
    DuplicateBinder {
        /// The later declaration: `SortParam { sort, index }` or
        /// `OpInput { op, index }`.
        site: SignatureSite,
        /// The repeated binder name.
        name: Arc<str>,
    },
}

impl SignatureDefect {
    /// The sort this defect reports as undeclared, when it is an
    /// [`SignatureDefect::UndeclaredSort`].
    #[must_use]
    pub const fn undeclared_sort(&self) -> Option<&Arc<str>> {
        match self {
            Self::UndeclaredSort { sort, .. } => Some(sort),
            _ => None,
        }
    }
}

impl std::fmt::Display for SignatureDefect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UndeclaredSort { site, sort } => {
                write!(f, "{site}: sort `{sort}` is not declared")
            }
            Self::SortArity {
                site,
                sort,
                expected,
                found,
            } => write!(
                f,
                "{site}: sort `{sort}` takes {expected} argument(s), given {found}"
            ),
            Self::ParamIllTyped {
                site,
                sort,
                index,
                expected,
                found,
            } => write!(
                f,
                "{site}: argument {index} of `{sort}` should have sort {expected}, has {found}"
            ),
            Self::UnboundParamVar { site, var } => {
                write!(
                    f,
                    "{site}: variable `{var}` is not bound by an earlier parameter"
                )
            }
            Self::DuplicateSymbol { kind, name } => {
                write!(f, "{kind} `{name}` is declared more than once")
            }
            Self::DuplicateBinder { site, name } => write!(
                f,
                "{site}: binder `{name}` is already bound by an earlier declaration"
            ),
        }
    }
}

/// Check that a theory's signature is well formed.
///
/// The judgement:
///
/// - no two sorts, operations, equations, directed equations or policies
///   share a name ([`SignatureDefect::DuplicateSymbol`]);
/// - every sort named by a sort parameter is declared and precedes the
///   sort in dependency order, which is the order of the parameter
///   dependency graph and not the order of `theory.sorts`
///   ([`SignatureDefect::UndeclaredSort`]);
/// - every sort expression of an operation's inputs and output names a
///   declared sort ([`SignatureDefect::UndeclaredSort`]);
/// - `SortExpr::Name(s)` for a dependent sort `s` is its unapplied,
///   implicitly indexed form and is admitted; a `SortExpr::App` must give
///   exactly as many arguments as the sort has parameters
///   ([`SignatureDefect::SortArity`]);
/// - each argument of an application has the sort of the parameter it
///   instantiates, with the earlier arguments substituted for the earlier
///   parameters, since the parameters form a dependent telescope
///   ([`SignatureDefect::ParamIllTyped`]);
/// - every variable in an argument is bound by an earlier parameter of the
///   sort, or by an earlier input of the operation, or for the output by
///   any input ([`SignatureDefect::UnboundParamVar`]);
/// - the parameter names of one sort, and the input names of one
///   operation, are pairwise distinct: one
///   [`SignatureDefect::DuplicateBinder`] is reported per parameter or
///   input whose name an earlier one of the same telescope binds, at that
///   later declaration. This is the freshness condition of contexts:
///   palmgren2019categories Def 4.1 takes `x_k` from a set disjoint from
///   `{x_1, ..., x_(k-1)}`, and rule R2 extends a context only by a
///   variable in `Fresh(Γ)` (stated).
///
/// These rules are the equation-free core of the well-formedness
/// judgements for generalized algebraic theories: the context, sort and
/// term formation rules of palmgren2019categories Defs 4.4 and 4.7 and
/// the sort and operation declarations of
/// bezemcoquanddybjerescardo2021generalized Def 4 (stated). Admitting
/// the unapplied name of a dependent sort is specific to panproto
/// (inferred).
///
/// The judgement is syntactic: an argument's sort must equal the
/// parameter's sort up to the substitution of earlier arguments, and no
/// conversion along the theory's equations is applied. Well-formedness of
/// a generalized algebraic theory can depend on its equations
/// (jamner2025pyrosome Sec 2, stated), so this check is incomplete for
/// theories whose typing needs conversion: it reports
/// [`SignatureDefect::ParamIllTyped`] for an argument whose sort equals
/// the parameter's only modulo an equation.
///
/// [`typecheck_theory`] runs this check first.
///
/// # Errors
///
/// Returns every defect found, in the order: duplicates, then sorts in
/// declaration order, then operations in declaration order; within one
/// parameter or input, a repeated binder name comes before the defects of
/// its sort.
pub fn typecheck_signature(theory: &Theory) -> Result<(), Vec<SignatureDefect>> {
    let mut defects = duplicate_symbols(theory);
    let ordered = sorts_in_dependency_order(theory);

    for sort in &theory.sorts {
        let mut ctx = VarContext::default();
        for (index, param) in sort.params.iter().enumerate() {
            let site = SignatureSite::SortParam {
                sort: Arc::clone(&sort.name),
                index,
            };
            fresh_binder(&param.name, site.clone(), &ctx, &mut defects);
            let head = param.sort.head();
            if theory.has_sort(head) && !ordered.contains(head) {
                defects.push(SignatureDefect::UndeclaredSort {
                    site,
                    sort: Arc::clone(head),
                });
            } else {
                check_sort_expr(&param.sort, &site, &ctx, theory, &mut defects);
            }
            ctx.insert(Arc::clone(&param.name), param.sort.clone());
        }
    }

    for op in &theory.ops {
        let mut ctx = VarContext::default();
        for (index, (name, sort, _)) in op.inputs.iter().enumerate() {
            let site = SignatureSite::OpInput {
                op: Arc::clone(&op.name),
                index,
            };
            fresh_binder(name, site.clone(), &ctx, &mut defects);
            check_sort_expr(sort, &site, &ctx, theory, &mut defects);
            ctx.insert(Arc::clone(name), sort.clone());
        }
        let site = SignatureSite::OpOutput {
            op: Arc::clone(&op.name),
        };
        check_sort_expr(&op.output, &site, &ctx, theory, &mut defects);
    }

    if defects.is_empty() {
        Ok(())
    } else {
        Err(defects)
    }
}

/// Report `name` as a [`SignatureDefect::DuplicateBinder`] at `site` when
/// `bound`, the binders of the earlier declarations of the telescope,
/// already binds it.
fn fresh_binder(
    name: &Arc<str>,
    site: SignatureSite,
    bound: &VarContext,
    defects: &mut Vec<SignatureDefect>,
) {
    if bound.contains_key(name) {
        defects.push(SignatureDefect::DuplicateBinder {
            site,
            name: Arc::clone(name),
        });
    }
}

/// Every name declared more than once, per kind, in ascending order.
fn duplicate_symbols(theory: &Theory) -> Vec<SignatureDefect> {
    fn dups<'a>(
        kind: SymKind,
        names: impl Iterator<Item = &'a Arc<str>>,
        out: &mut Vec<SignatureDefect>,
    ) {
        let mut seen = std::collections::BTreeSet::new();
        let mut repeated = std::collections::BTreeSet::new();
        for name in names {
            if !seen.insert(name) {
                repeated.insert(name);
            }
        }
        for name in repeated {
            out.push(SignatureDefect::DuplicateSymbol {
                kind,
                name: Arc::clone(name),
            });
        }
    }
    let mut out = Vec::new();
    dups(
        SymKind::Sort,
        theory.sorts.iter().map(|s| &s.name),
        &mut out,
    );
    dups(SymKind::Op, theory.ops.iter().map(|o| &o.name), &mut out);
    dups(SymKind::Eq, theory.eqs.iter().map(|e| &e.name), &mut out);
    dups(
        SymKind::DirectedEq,
        theory.directed_eqs.iter().map(|e| &e.name),
        &mut out,
    );
    dups(
        SymKind::Policy,
        theory.policies.iter().map(|p| &p.name),
        &mut out,
    );
    out
}

/// The declared sorts that can be placed in a dependency order: those
/// whose parameter sorts are all declared and themselves orderable. A sort
/// on a cycle of parameter dependencies, or depending on one, is left out.
fn sorts_in_dependency_order(theory: &Theory) -> FxHashSet<Arc<str>> {
    let mut ordered: FxHashSet<Arc<str>> = FxHashSet::default();
    loop {
        let before = ordered.len();
        for sort in &theory.sorts {
            if ordered.contains(&sort.name) {
                continue;
            }
            let ready = sort.params.iter().all(|p| {
                let head = p.sort.head();
                *head != sort.name && (!theory.has_sort(head) || ordered.contains(head))
            });
            if ready {
                ordered.insert(Arc::clone(&sort.name));
            }
        }
        if ordered.len() == before {
            return ordered;
        }
    }
}

/// Check one sort expression at `site`, with `ctx` binding the variables
/// its arguments may mention.
fn check_sort_expr(
    expr: &SortExpr,
    site: &SignatureSite,
    ctx: &VarContext,
    theory: &Theory,
    defects: &mut Vec<SignatureDefect>,
) {
    let head = expr.head();
    let Some(declared) = theory.find_sort(head) else {
        defects.push(SignatureDefect::UndeclaredSort {
            site: site.clone(),
            sort: Arc::clone(head),
        });
        return;
    };
    let SortExpr::App { args, .. } = expr else {
        return;
    };
    if args.len() != declared.params.len() {
        defects.push(SignatureDefect::SortArity {
            site: site.clone(),
            sort: Arc::clone(head),
            expected: declared.params.len(),
            found: args.len(),
        });
        return;
    }
    let mut theta: FxHashMap<Arc<str>, Term> = FxHashMap::default();
    for (index, (arg, param)) in args.iter().zip(&declared.params).enumerate() {
        let expected = param.sort.subst(&theta);
        let unbound: Vec<Arc<str>> = {
            let mut vars: Vec<Arc<str>> = arg
                .free_vars()
                .into_iter()
                .filter(|v| !ctx.contains_key(v))
                .collect();
            vars.sort();
            vars
        };
        if unbound.is_empty() {
            match typecheck_term(arg, ctx, theory) {
                Ok(found) if sort_exprs_agree(&found, &[], &expected, &[]) => {}
                Ok(found) => defects.push(SignatureDefect::ParamIllTyped {
                    site: site.clone(),
                    sort: Arc::clone(head),
                    index,
                    expected: Arc::from(expected.to_string()),
                    found: Arc::from(found.to_string()),
                }),
                Err(err) => defects.push(SignatureDefect::ParamIllTyped {
                    site: site.clone(),
                    sort: Arc::clone(head),
                    index,
                    expected: Arc::from(expected.to_string()),
                    found: Arc::from(format!("no sort ({err})")),
                }),
            }
        } else {
            for var in unbound {
                defects.push(SignatureDefect::UnboundParamVar {
                    site: site.clone(),
                    var,
                });
            }
        }
        theta.insert(Arc::clone(&param.name), arg.clone());
    }
}

/// Typecheck a theory: its signature, then all its equations.
///
/// The signature is checked first with [`typecheck_signature`]; a theory
/// whose signature is ill formed is rejected with
/// [`GatError::IllTypedSignature`] before any equation is looked at.
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
    typecheck_signature(theory).map_err(|defects| GatError::IllTypedSignature { defects })?;
    typecheck_theory_body(theory)
}

/// Everything [`typecheck_theory`] checks after the signature: implicit
/// inferrability, closed sorts, and every equation.
pub fn typecheck_theory_body(theory: &Theory) -> Result<(), GatError> {
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
    let defined = defined_operations(theory);
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
            if op.output.head() == &sort.name
                && !ctor_set.contains(&op.name)
                && !defined.contains(&op.name)
            {
                return Err(GatError::InvalidClosedSortConstructor {
                    sort: sort.name.to_string(),
                    constructor: op.name.to_string(),
                    detail: "op produces the closed sort but is neither listed in its \
                             closure nor defined by an equation"
                        .to_string(),
                });
            }
        }
    }
    Ok(())
}

/// The operations the theory defines by an equation: those with some
/// equation whose left side is the operation applied to distinct
/// variables, one per explicit input.
///
/// A closed sort's closure lists its canonical forms, which is what makes
/// a `Case` over it exhaustive. An operation defined this way introduces
/// no canonical form: its equation says what it reduces to, and the
/// checker has already required that to be well-typed at the sort. So
/// `replicate : (n : Nat, x : A) -> Vec(n)` is a function into `Vec`, not
/// a constructor of it, and may target the closed sort without being
/// listed. An operation into a closed sort that is neither listed nor
/// defined is still refused, since it would be a stuck term every `Case`
/// on the sort silently fails to cover.
fn defined_operations(theory: &Theory) -> rustc_hash::FxHashSet<Arc<str>> {
    let mut defined = rustc_hash::FxHashSet::default();
    let sides = theory
        .eqs
        .iter()
        .map(|eq| &eq.lhs)
        .chain(theory.directed_eqs.iter().map(|eq| &eq.lhs));
    for lhs in sides {
        let Term::App { op, args } = lhs else {
            continue;
        };
        let Some(operation) = theory.find_op(op) else {
            continue;
        };
        if args.len() != operation.explicit_arity() {
            continue;
        }
        let mut seen = rustc_hash::FxHashSet::default();
        let all_distinct_vars = args.iter().all(|arg| match arg {
            Term::Var(name) => seen.insert(Arc::clone(name)),
            _ => false,
        });
        if all_distinct_vars {
            defined.insert(Arc::clone(op));
        }
    }
    defined
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

/// Hole-collecting variant of [`typecheck_app_with_implicits`]. Walks
/// explicit args through [`typecheck_with_expected`] so that holes
/// attach to the correct expected sort (before implicit-inference
/// unification pins anything down), then recovers the output sort via
/// the same unification pipeline as the non-collecting path.
fn typecheck_app_with_implicits_collecting_holes(
    op: &Arc<str>,
    args: &[Term],
    operation: &Operation,
    ctx: &VarContext,
    theory: &Theory,
    reports: &mut Vec<HoleReport>,
) -> Result<SortExpr, GatError> {
    let explicit_count = operation.explicit_arity();
    if args.len() != explicit_count {
        return Err(GatError::TermArityMismatch {
            op: op.to_string(),
            expected: explicit_count,
            got: args.len(),
        });
    }

    let mut fresh_rename: FxHashMap<Arc<str>, Term> = FxHashMap::default();
    for (idx, (pname, _, imp)) in operation.inputs.iter().enumerate() {
        if matches!(imp, Implicit::Yes) {
            let mv: Arc<str> = Arc::from(format!("?{pname}_{idx}"));
            fresh_rename.insert(Arc::clone(pname), Term::Var(mv));
        }
    }

    let mut theta: FxHashMap<Arc<str>, Term> = fresh_rename.clone();
    let mut term_eqs: Vec<(Term, Term)> = Vec::new();
    let mut explicit_iter = args.iter();
    for (pname, declared_sort, imp) in &operation.inputs {
        match imp {
            Implicit::Yes => {}
            Implicit::No => {
                let Some(arg) = explicit_iter.next() else {
                    return Err(GatError::TermArityMismatch {
                        op: op.to_string(),
                        expected: explicit_count,
                        got: args.len(),
                    });
                };
                let expected = declared_sort.subst(&theta);
                let arg_sort = typecheck_with_expected(arg, Some(&expected), ctx, theory, reports)?;
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

    let mut final_subst = theta.clone();
    for (k, v) in &mgu {
        final_subst.insert(Arc::clone(k), v.clone());
    }
    let final_subst: FxHashMap<Arc<str>, Term> = final_subst
        .into_iter()
        .map(|(k, v)| (k, v.substitute(&mgu)))
        .collect();

    Ok(operation.output.subst(&final_subst))
}

fn count_holes(t: &Term) -> usize {
    match t {
        Term::Hole { .. } => 1,
        Term::Var(_) => 0,
        Term::App { args, .. } => args.iter().map(count_holes).sum(),
        Term::Case {
            scrutinee,
            branches,
        } => count_holes(scrutinee) + branches.iter().map(|b| count_holes(&b.body)).sum::<usize>(),
        Term::Let { bound, body, .. } => count_holes(bound) + count_holes(body),
    }
}

fn term_contains_hole(t: &Term) -> bool {
    match t {
        Term::Hole { .. } => true,
        Term::Var(_) => false,
        Term::Let { bound, body, .. } => term_contains_hole(bound) || term_contains_hole(body),
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
        Term::Let {
            name: binder,
            bound,
            body,
        } => term_mentions_var(bound, name) || (binder != name && term_mentions_var(body, name)),
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

    #[test]
    fn equation_applications_infer_implicit_arguments() -> Result<(), Box<dyn std::error::Error>> {
        let mut theory = lambda_theory();
        theory.eqs.push(Equation::new(
            "app_reflexive",
            Term::app("app", vec![Term::var("f"), Term::var("x")]),
            Term::app("app", vec![Term::var("f"), Term::var("x")]),
        ));

        let context = infer_var_sorts(&theory.eqs[0], &theory)?;
        assert_eq!(
            context.len(),
            2,
            "only explicit equation variables are bound"
        );
        assert_eq!(context[&Arc::from("f")].head().as_ref(), "Tm");
        assert_eq!(context[&Arc::from("x")].head().as_ref(), "Tm");
        typecheck_theory(&theory)?;
        Ok(())
    }

    #[test]
    fn monomorphic_let_typechecks() -> Result<(), Box<dyn std::error::Error>> {
        // let x = unit in mul(x, x) : Carrier.
        let theory = monoid_theory();
        let ctx = VarContext::default();
        let t = Term::Let {
            name: Arc::from("x"),
            bound: Box::new(Term::constant("unit")),
            body: Box::new(Term::app("mul", vec![Term::var("x"), Term::var("x")])),
        };
        let sort = typecheck_term(&t, &ctx, &theory)?;
        assert_eq!(&**sort.head(), "Carrier");
        Ok(())
    }

    #[test]
    fn equation_with_hole_is_rejected() {
        let theory = monoid_theory();
        let eq = Equation::new(
            "bad",
            Term::app("mul", vec![Term::var("a"), Term::Hole { name: None }]),
            Term::var("a"),
        );
        let result = typecheck_equation(&eq, &theory);
        assert!(matches!(result, Err(GatError::HolesInEquation { .. })));
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

    /// The building blocks of `panproto-protocols::theories`, reproduced
    /// here so the signature judgement is pinned against the shapes the
    /// registration helpers compose: `th_constraint`'s
    /// `target : Constraint -> Vertex` and `th_simple_graph`'s
    /// `src, tgt : Edge -> Vertex` use a dependent sort unapplied.
    fn building_blocks() -> Vec<Theory> {
        let mut blocks = schema_blocks();
        blocks.extend(instance_blocks());
        blocks
    }

    /// The schema-side building blocks.
    fn schema_blocks() -> Vec<Theory> {
        let unary = Operation::unary;
        vec![
            Theory::new(
                "ThGraph",
                vec![Sort::simple("Vertex"), Sort::simple("Edge")],
                vec![
                    unary("src", "e", "Edge", "Vertex"),
                    unary("tgt", "e", "Edge", "Vertex"),
                ],
                vec![],
            ),
            Theory::new(
                "ThConstraint",
                vec![
                    Sort::simple("Vertex"),
                    Sort::dependent("Constraint", vec![SortParam::new("v", "Vertex")]),
                ],
                vec![unary("target", "c", "Constraint", "Vertex")],
                vec![],
            ),
            Theory::new(
                "ThMulti",
                vec![
                    Sort::simple("Vertex"),
                    Sort::simple("Edge"),
                    Sort::simple("EdgeLabel"),
                ],
                vec![unary("edge_label", "e", "Edge", "EdgeLabel")],
                vec![],
            ),
        ]
    }

    /// `ThWType`, with its two arc-anchor coherence axioms.
    fn wtype_block() -> Theory {
        let unary = Operation::unary;
        Theory::new(
            "ThWType",
            vec![
                Sort::simple("Node"),
                Sort::simple("Arc"),
                Sort::simple("Value"),
                Sort::simple("Vertex"),
                Sort::simple("Edge"),
            ],
            vec![
                unary("anchor", "n", "Node", "Vertex"),
                unary("arc_src", "a", "Arc", "Node"),
                unary("arc_tgt", "a", "Arc", "Node"),
                unary("arc_edge", "a", "Arc", "Edge"),
                unary("node_value", "n", "Node", "Value"),
                unary("src", "e", "Edge", "Vertex"),
                unary("tgt", "e", "Edge", "Vertex"),
            ],
            vec![
                Equation::new(
                    "arc_src_anchor",
                    Term::app("anchor", vec![Term::app("arc_src", vec![Term::var("a")])]),
                    Term::app("src", vec![Term::app("arc_edge", vec![Term::var("a")])]),
                ),
                Equation::new(
                    "arc_tgt_anchor",
                    Term::app("anchor", vec![Term::app("arc_tgt", vec![Term::var("a")])]),
                    Term::app("tgt", vec![Term::app("arc_edge", vec![Term::var("a")])]),
                ),
            ],
        )
    }

    /// The instance-side building blocks and the remaining schema ones.
    fn instance_blocks() -> Vec<Theory> {
        let unary = Operation::unary;
        vec![
            wtype_block(),
            Theory::new(
                "ThMeta",
                vec![
                    Sort::simple("Node"),
                    Sort::simple("Discriminator"),
                    Sort::simple("ExtraField"),
                    Sort::simple("Value"),
                ],
                vec![
                    unary("discriminator", "n", "Node", "Discriminator"),
                    Operation::new(
                        "extra_field",
                        vec![
                            ("n".into(), "Node".into()),
                            ("key".into(), "ExtraField".into()),
                        ],
                        "Value",
                    ),
                ],
                vec![],
            ),
            Theory::new(
                "ThSimpleGraph",
                vec![
                    Sort::simple("Vertex"),
                    Sort::dependent(
                        "Edge",
                        vec![SortParam::new("s", "Vertex"), SortParam::new("t", "Vertex")],
                    ),
                ],
                vec![
                    unary("src", "e", "Edge", "Vertex"),
                    unary("tgt", "e", "Edge", "Vertex"),
                ],
                vec![],
            ),
            Theory::new(
                "ThHypergraph",
                vec![
                    Sort::simple("Vertex"),
                    Sort::simple("HyperEdge"),
                    Sort::simple("Label"),
                ],
                vec![
                    Operation::new(
                        "incident",
                        vec![
                            ("he".into(), "HyperEdge".into()),
                            ("l".into(), "Label".into()),
                        ],
                        "Vertex",
                    ),
                    unary("parent_label", "he", "HyperEdge", "Label"),
                ],
                vec![],
            ),
            Theory::new(
                "ThInterface",
                vec![Sort::simple("Vertex"), Sort::simple("Interface")],
                vec![Operation::new(
                    "implements",
                    vec![
                        ("v".into(), "Vertex".into()),
                        ("i".into(), "Interface".into()),
                    ],
                    "Vertex",
                )],
                vec![],
            ),
            Theory::new(
                "ThGraphInstance",
                vec![
                    Sort::simple("IVertex"),
                    Sort::simple("IEdge"),
                    Sort::simple("Value"),
                    Sort::simple("Vertex"),
                    Sort::simple("Edge"),
                ],
                vec![
                    unary("i_src", "e", "IEdge", "IVertex"),
                    unary("i_tgt", "e", "IEdge", "IVertex"),
                    unary("iv_anchor", "v", "IVertex", "Vertex"),
                    unary("ie_anchor", "e", "IEdge", "Edge"),
                    unary("iv_value", "v", "IVertex", "Value"),
                ],
                vec![],
            ),
        ]
    }

    /// The judgement is syntactic: `w : Z(u)` with `u : Y(b)` and `Z`'s
    /// parameter of sort `Y(a)` is refused although the equation `a = b`
    /// makes the two sorts derivably equal.
    #[test]
    fn typecheck_signature_applies_no_conversion_along_equations() {
        let theory = Theory::new(
            "Conv",
            vec![
                Sort::simple("X"),
                Sort::dependent("Y", vec![SortParam::new("x", "X")]),
                Sort::dependent(
                    "Z",
                    vec![SortParam::new(
                        "y",
                        SortExpr::app("Y", vec![Term::constant("a")]),
                    )],
                ),
            ],
            vec![
                Operation::new("a", vec![], "X"),
                Operation::new("b", vec![], "X"),
                Operation::new("u", vec![], SortExpr::app("Y", vec![Term::constant("b")])),
                Operation::new("w", vec![], SortExpr::app("Z", vec![Term::constant("u")])),
            ],
            vec![crate::eq::Equation::new(
                "a_is_b",
                Term::constant("a"),
                Term::constant("b"),
            )],
        );
        assert_eq!(
            typecheck_signature(&theory),
            Err(vec![SignatureDefect::ParamIllTyped {
                site: SignatureSite::OpOutput { op: "w".into() },
                sort: "Z".into(),
                index: 0,
                expected: "Y(a())".into(),
                found: "Y(b())".into(),
            }])
        );
    }

    /// The theory of the conversion test with `u : Y(fibre)` and the given
    /// equations `e0`, `e1` and directed equation `d0`.
    fn conversion_theory(fibre: &str, contents: [(&Term, &Term); 3]) -> Theory {
        let [e0, e1, d0] = contents;
        Theory::full(
            "Conv",
            vec![],
            vec![
                Sort::simple("X"),
                Sort::dependent("Y", vec![SortParam::new("x", "X")]),
                Sort::dependent(
                    "Z",
                    vec![SortParam::new(
                        "y",
                        SortExpr::app("Y", vec![Term::constant("a")]),
                    )],
                ),
            ],
            vec![
                Operation::new("a", vec![], "X"),
                Operation::new("b", vec![], "X"),
                Operation::new("u", vec![], SortExpr::app("Y", vec![Term::constant(fibre)])),
                Operation::new("w", vec![], SortExpr::app("Z", vec![Term::constant("u")])),
            ],
            vec![
                crate::eq::Equation::new("e0", e0.0.clone(), e0.1.clone()),
                crate::eq::Equation::new("e1", e1.0.clone(), e1.1.clone()),
            ],
            vec![crate::eq::DirectedEquation::new(
                "d0",
                d0.0.clone(),
                d0.1.clone(),
                panproto_expr::Expr::Var("_".into()),
            )],
            vec![],
        )
    }

    /// The judgement reads no equation's sides: over every choice of the
    /// sides of `e0`, `e1` and `d0` among `a`, `b` and `x`, the verdict
    /// on `u : Y(a)` (well formed) and `u : Y(b)` (refused) is the one
    /// given with every equation reading `a = a`.
    #[test]
    fn typecheck_signature_verdict_ignores_every_equation_side() {
        let terms = [Term::constant("a"), Term::constant("b"), Term::var("x")];
        let trivial = (&terms[0], &terms[0]);
        let mut checked = 0;
        for fibre in ["a", "b"] {
            let reference = typecheck_signature(&conversion_theory(fibre, [trivial; 3]));
            assert_eq!(reference.is_ok(), fibre == "a");
            let sides: Vec<(&Term, &Term)> = terms
                .iter()
                .flat_map(|l| terms.iter().map(move |r| (l, r)))
                .collect();
            for e0 in &sides {
                for e1 in &sides {
                    for d0 in &sides {
                        assert_eq!(
                            typecheck_signature(&conversion_theory(fibre, [*e0, *e1, *d0])),
                            reference,
                            "{fibre}: {e0:?} {e1:?} {d0:?}"
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 2 * 9 * 9 * 9);
    }

    mod equation_independence {
        use super::*;
        use proptest::prelude::*;

        /// Terms of depth at most 2 over the variables `x`, `y` and the
        /// operations `a`, `b`, `u`, `w` and an undeclared `k`.
        fn arb_side() -> impl Strategy<Value = Term> {
            let leaf = prop_oneof![
                Just(Term::var("x")),
                Just(Term::var("y")),
                Just(Term::constant("a")),
                Just(Term::constant("b")),
                Just(Term::constant("u")),
                Just(Term::constant("w")),
            ];
            leaf.prop_recursive(2, 8, 2, |inner| {
                (
                    prop_oneof![Just("a"), Just("u"), Just("k")],
                    proptest::collection::vec(inner, 0..=2),
                )
                    .prop_map(|(op, args)| Term::app(op, args))
            })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(512))]

            #[test]
            fn typecheck_signature_verdict_ignores_equation_sides(
                fibre in prop_oneof![Just("a"), Just("b")],
                sides in proptest::collection::vec((arb_side(), arb_side()), 3),
            ) {
                let a = Term::constant("a");
                let reference = typecheck_signature(&conversion_theory(fibre, [(&a, &a); 3]));
                let contents = [
                    (&sides[0].0, &sides[0].1),
                    (&sides[1].0, &sides[1].1),
                    (&sides[2].0, &sides[2].1),
                ];
                prop_assert_eq!(typecheck_signature(&conversion_theory(fibre, contents)), reference);
            }
        }
    }

    /// A signature that no rule admits: `id : (x : Ob) -> Cell(x, x)` where
    /// `Cell` takes two `Mor`s, and `mk : (o : Ob) -> V(o)` where `V` takes
    /// no parameters. One further negative per defect, and every
    /// registration building block as a positive.
    #[test]
    fn typecheck_rejects_ill_typed_signatures() -> Result<(), Box<dyn std::error::Error>> {
        let bad = Theory::new(
            "Bad",
            vec![
                Sort::simple("Ob"),
                Sort::simple("Mor"),
                Sort::simple("V"),
                Sort::dependent(
                    "Cell",
                    vec![SortParam::new("f", "Mor"), SortParam::new("g", "Mor")],
                ),
            ],
            vec![
                Operation::new(
                    "id",
                    vec![(Arc::from("x"), SortExpr::from("Ob"))],
                    SortExpr::app("Cell", vec![Term::var("x"), Term::var("x")]),
                ),
                Operation::new(
                    "mk",
                    vec![(Arc::from("o"), SortExpr::from("Ob"))],
                    SortExpr::app("V", vec![Term::var("o")]),
                ),
            ],
            vec![],
        );
        let result = typecheck_theory(&bad);
        assert!(
            matches!(result, Err(GatError::IllTypedSignature { .. })),
            "ill-typed signature accepted: {result:?}"
        );
        let defects = typecheck_signature(&bad).err().unwrap_or_default();
        let id_out = SignatureSite::OpOutput { op: "id".into() };
        assert_eq!(
            defects,
            vec![
                SignatureDefect::ParamIllTyped {
                    site: id_out.clone(),
                    sort: "Cell".into(),
                    index: 0,
                    expected: "Mor".into(),
                    found: "Ob".into(),
                },
                SignatureDefect::ParamIllTyped {
                    site: id_out,
                    sort: "Cell".into(),
                    index: 1,
                    expected: "Mor".into(),
                    found: "Ob".into(),
                },
                SignatureDefect::SortArity {
                    site: SignatureSite::OpOutput { op: "mk".into() },
                    sort: "V".into(),
                    expected: 0,
                    found: 1,
                },
            ]
        );

        rejects_undeclared_unbound_and_duplicate_symbols();
        for block in building_blocks() {
            typecheck_signature(&block).map_err(|d| format!("{} rejected: {d:?}", block.name))?;
            typecheck_theory(&block)?;
        }
        // `ThFunctor` and `ThFlat` are open: they use `Vertex`, which the
        // schema side of a composition declares, and that is their only
        // defect standalone.
        for (block, op) in open_instance_blocks() {
            assert_eq!(
                typecheck_signature(&block),
                Err(vec![SignatureDefect::UndeclaredSort {
                    site: SignatureSite::OpOutput { op: op.into() },
                    sort: "Vertex".into(),
                }]),
                "{}",
                block.name
            );
        }
        Ok(())
    }

    /// The building blocks of `panproto-protocols::theories` that use the
    /// schema sort `Vertex` without declaring it, each with the operation
    /// whose output is `Vertex`.
    fn open_instance_blocks() -> Vec<(Theory, &'static str)> {
        let unary = Operation::unary;
        vec![
            (
                Theory::new(
                    "ThFunctor",
                    vec![
                        Sort::simple("Table"),
                        Sort::simple("Row"),
                        Sort::simple("ForeignKey"),
                    ],
                    vec![
                        unary("table_vertex", "t", "Table", "Vertex"),
                        Operation::new("fk_src", vec![("fk".into(), "ForeignKey".into())], "Row"),
                        Operation::new("fk_tgt", vec![("fk".into(), "ForeignKey".into())], "Row"),
                    ],
                    vec![],
                ),
                "table_vertex",
            ),
            (
                Theory::new(
                    "ThFlat",
                    vec![
                        Sort::simple("Node"),
                        Sort::simple("Field"),
                        Sort::simple("Value"),
                    ],
                    vec![
                        unary("field_node", "f", "Field", "Node"),
                        unary("field_value", "f", "Field", "Value"),
                        unary("node_anchor", "n", "Node", "Vertex"),
                    ],
                    vec![],
                ),
                "node_anchor",
            ),
        ]
    }

    /// One negative per remaining defect: an undeclared sort, a parameter
    /// sort declared only through a cycle, an unbound variable, and a
    /// duplicate name.
    fn rejects_undeclared_unbound_and_duplicate_symbols() {
        // Undeclared sort, and a parameter sort only declared through a cycle.
        let undeclared = Theory::new(
            "U",
            vec![
                Sort::dependent("P", vec![SortParam::new("q", "Q")]),
                Sort::dependent("Q", vec![SortParam::new("p", "P")]),
            ],
            vec![Operation::unary("f", "x", "Missing", "P")],
            vec![],
        );
        let defects = typecheck_signature(&undeclared).err().unwrap_or_default();
        assert_eq!(
            defects,
            vec![
                SignatureDefect::UndeclaredSort {
                    site: SignatureSite::SortParam {
                        sort: "P".into(),
                        index: 0
                    },
                    sort: "Q".into(),
                },
                SignatureDefect::UndeclaredSort {
                    site: SignatureSite::SortParam {
                        sort: "Q".into(),
                        index: 0
                    },
                    sort: "P".into(),
                },
                SignatureDefect::UndeclaredSort {
                    site: SignatureSite::OpInput {
                        op: "f".into(),
                        index: 0
                    },
                    sort: "Missing".into(),
                },
            ]
        );

        // A variable no earlier input binds, and duplicate names.
        let unbound = Theory::new(
            "Unbound",
            vec![
                Sort::simple("Ob"),
                Sort::simple("Ob"),
                Sort::dependent(
                    "Hom",
                    vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
                ),
            ],
            vec![Operation::new(
                "loop",
                vec![(Arc::from("x"), SortExpr::from("Ob"))],
                SortExpr::app("Hom", vec![Term::var("x"), Term::var("y")]),
            )],
            vec![],
        );
        let defects = typecheck_signature(&unbound).err().unwrap_or_default();
        assert_eq!(
            defects,
            vec![
                SignatureDefect::DuplicateSymbol {
                    kind: SymKind::Sort,
                    name: "Ob".into(),
                },
                SignatureDefect::UnboundParamVar {
                    site: SignatureSite::OpOutput { op: "loop".into() },
                    var: "y".into(),
                },
            ]
        );
    }

    /// Every [`SignatureDefect`] and [`SignatureSite`] variant serializes
    /// under its `snake_case` tag and reads back unchanged.
    #[test]
    fn signature_defect_round_trips_every_variant() -> Result<(), serde_json::Error> {
        let sites = [
            SignatureSite::SortParam {
                sort: "S".into(),
                index: 1,
            },
            SignatureSite::OpInput {
                op: "f".into(),
                index: 0,
            },
            SignatureSite::OpOutput { op: "f".into() },
        ];
        let site_tags = ["sort_param", "op_input", "op_output"];
        for (site, tag) in sites.iter().zip(site_tags) {
            let json = serde_json::to_value(site)?;
            assert!(json.get(tag).is_some(), "{json}");
            assert_eq!(&serde_json::from_value::<SignatureSite>(json)?, site);
        }
        defects_round_trip(sites[2].clone())
    }

    fn defects_round_trip(site: SignatureSite) -> Result<(), serde_json::Error> {
        let defects = [
            SignatureDefect::UndeclaredSort {
                site: site.clone(),
                sort: "V".into(),
            },
            SignatureDefect::SortArity {
                site: site.clone(),
                sort: "V".into(),
                expected: 0,
                found: 1,
            },
            SignatureDefect::ParamIllTyped {
                site: site.clone(),
                sort: "Cell".into(),
                index: 0,
                expected: "Mor".into(),
                found: "Ob".into(),
            },
            SignatureDefect::UnboundParamVar {
                site,
                var: "y".into(),
            },
            SignatureDefect::DuplicateSymbol {
                kind: SymKind::Op,
                name: "f".into(),
            },
            SignatureDefect::DuplicateBinder {
                site: SignatureSite::OpInput {
                    op: "f".into(),
                    index: 1,
                },
                name: "z".into(),
            },
        ];
        let tags = [
            "undeclared_sort",
            "sort_arity",
            "param_ill_typed",
            "unbound_param_var",
            "duplicate_symbol",
            "duplicate_binder",
        ];
        for (defect, tag) in defects.iter().zip(tags) {
            let json = serde_json::to_value(defect)?;
            assert!(json.get(tag).is_some(), "{json}");
            assert_eq!(&serde_json::from_value::<SignatureDefect>(json)?, defect);
        }
        Ok(())
    }

    fn branch(constructor: &str, binders: &[&str], body: Term) -> CaseBranch {
        CaseBranch {
            constructor: Arc::from(constructor),
            binders: binders.iter().map(|b| Arc::from(*b)).collect(),
            body,
        }
    }

    fn case(scrutinee: Term, branches: Vec<CaseBranch>) -> Term {
        Term::Case {
            scrutinee: Box::new(scrutinee),
            branches,
        }
    }

    fn let_in(name: &str, bound: Term, body: Term) -> Term {
        Term::Let {
            name: Arc::from(name),
            bound: Box::new(bound),
            body: Box::new(body),
        }
    }

    fn hole() -> Term {
        Term::Hole { name: None }
    }

    fn ctx_of(entries: &[(&str, SortExpr)]) -> VarContext {
        entries
            .iter()
            .map(|(n, s)| (Arc::from(*n), s.clone()))
            .collect()
    }

    /// `A` and `B` with constants `a : A`, `b : B`, `f : A -> B` and
    /// `g : B -> A`; the closed sort `Bool` of `tt` and `ff`; the open
    /// sort `Open`; and `wrap : (x : A) -> W`.
    fn case_theory() -> Theory {
        Theory::new(
            "Cases",
            vec![
                Sort::simple("A"),
                Sort::simple("B"),
                Sort::simple("W"),
                Sort::simple("Open"),
                Sort::closed("Bool", vec![], ["tt", "ff"]),
            ],
            vec![
                Operation::nullary("a", "A"),
                Operation::nullary("b", "B"),
                Operation::unary("f", "x", "A", "B"),
                Operation::unary("g", "x", "B", "A"),
                Operation::nullary("tt", "Bool"),
                Operation::nullary("ff", "Bool"),
                Operation::unary("wrap", "x", "A", "W"),
            ],
            Vec::new(),
        )
    }

    #[test]
    fn occurs_in_respects_let_and_case_binders() {
        let x: Arc<str> = Arc::from("x");
        assert!(occurs_in(&x, &Term::var("x")));
        assert!(!occurs_in(&x, &Term::var("y")));
        assert!(!occurs_in(&x, &hole()));
        assert!(occurs_in(
            &x,
            &Term::app("f", vec![Term::var("y"), Term::var("x")])
        ));
        assert!(occurs_in(&x, &let_in("z", Term::var("x"), Term::var("w"))));
        assert!(occurs_in(&x, &let_in("z", Term::var("w"), Term::var("x"))));
        assert!(!occurs_in(&x, &let_in("x", Term::var("w"), Term::var("x"))));
        assert!(!occurs_in(&x, &let_in("z", Term::var("w"), Term::var("w"))));
        assert!(occurs_in(
            &x,
            &case(Term::var("x"), vec![branch("c", &[], Term::var("w"))])
        ));
        assert!(occurs_in(
            &x,
            &case(Term::var("w"), vec![branch("c", &["y"], Term::var("x"))])
        ));
        assert!(!occurs_in(
            &x,
            &case(Term::var("w"), vec![branch("c", &["x"], Term::var("x"))])
        ));
        assert!(!occurs_in(
            &x,
            &case(Term::var("w"), vec![branch("c", &["y"], Term::var("y"))])
        ));
    }

    #[test]
    fn term_mentions_var_respects_let_and_case_binders() {
        let x: Arc<str> = Arc::from("x");
        assert!(term_mentions_var(&Term::var("x"), &x));
        assert!(!term_mentions_var(&Term::var("y"), &x));
        assert!(!term_mentions_var(&hole(), &x));
        assert!(!term_mentions_var(&Term::constant("c"), &x));
        assert!(term_mentions_var(
            &Term::app("f", vec![Term::var("y"), Term::var("x")]),
            &x
        ));
        assert!(term_mentions_var(
            &let_in("z", Term::var("x"), Term::var("w")),
            &x
        ));
        assert!(term_mentions_var(
            &let_in("z", Term::var("w"), Term::var("x")),
            &x
        ));
        assert!(!term_mentions_var(
            &let_in("x", Term::var("w"), Term::var("x")),
            &x
        ));
        assert!(!term_mentions_var(
            &let_in("z", Term::var("w"), Term::var("w")),
            &x
        ));
        assert!(term_mentions_var(
            &case(Term::var("x"), vec![branch("c", &[], Term::var("w"))]),
            &x
        ));
        assert!(term_mentions_var(
            &case(Term::var("w"), vec![branch("c", &["y"], Term::var("x"))]),
            &x
        ));
        assert!(!term_mentions_var(
            &case(Term::var("w"), vec![branch("c", &["x"], Term::var("x"))]),
            &x
        ));
        assert!(!term_mentions_var(
            &case(Term::var("w"), vec![branch("c", &["y"], Term::var("y"))]),
            &x
        ));
    }

    #[test]
    fn term_contains_hole_finds_holes_anywhere() {
        assert!(term_contains_hole(&hole()));
        assert!(!term_contains_hole(&Term::var("x")));
        assert!(!term_contains_hole(&Term::constant("c")));
        assert!(term_contains_hole(&Term::app(
            "f",
            vec![Term::var("x"), hole()]
        )));
        assert!(term_contains_hole(&let_in("v", hole(), Term::var("v"))));
        assert!(term_contains_hole(&let_in("v", Term::var("x"), hole())));
        assert!(!term_contains_hole(&let_in(
            "v",
            Term::var("x"),
            Term::var("v")
        )));
        assert!(term_contains_hole(&case(
            hole(),
            vec![branch("c", &[], Term::var("w"))]
        )));
        assert!(term_contains_hole(&case(
            Term::var("w"),
            vec![branch("c", &[], hole())]
        )));
        assert!(!term_contains_hole(&case(
            Term::var("w"),
            vec![branch("c", &[], Term::var("w"))]
        )));
    }

    #[test]
    fn count_holes_sums_over_every_position() {
        assert_eq!(count_holes(&Term::var("x")), 0);
        assert_eq!(
            count_holes(&Term::app("f", vec![hole(), hole(), Term::var("x")])),
            2
        );
        assert_eq!(
            count_holes(&case(
                hole(),
                vec![
                    branch("c", &[], hole()),
                    branch("d", &[], Term::app("f", vec![hole(), hole()])),
                ]
            )),
            4
        );
        assert_eq!(
            count_holes(&case(hole(), vec![branch("c", &[], hole())])),
            2
        );
        assert_eq!(count_holes(&let_in("v", hole(), hole())), 2);
        assert_eq!(
            count_holes(&let_in("v", Term::app("f", vec![hole(), hole()]), hole())),
            3
        );
    }

    #[test]
    fn pushing_sort_constraints_requires_matching_heads_and_arities() {
        let op: Arc<str> = Arc::from("op");
        let mut eqs = Vec::new();
        let fx = SortExpr::app("Fib", vec![Term::var("x")]);
        match push_sort_expr_eqs_into(&fx, &SortExpr::from("Fib"), &op, &mut eqs) {
            Err(GatError::ArgTypeMismatch {
                op,
                arg_index,
                expected,
                got,
            }) => {
                assert_eq!((op.as_str(), arg_index), ("op", 0));
                assert_eq!((expected.as_str(), got.as_str()), ("Fib(x)", "Fib"));
            }
            other => panic!("expected ArgTypeMismatch, got {other:?}"),
        }
        assert!(matches!(
            push_sort_expr_eqs_into(&SortExpr::from("A"), &SortExpr::from("B"), &op, &mut eqs),
            Err(GatError::ArgTypeMismatch { .. })
        ));
        assert!(eqs.is_empty());
        let fc = SortExpr::app("Fib", vec![Term::constant("c")]);
        assert!(push_sort_expr_eqs_into(&fx, &fc, &op, &mut eqs).is_ok());
        assert_eq!(eqs, [(Term::var("x"), Term::constant("c"))]);
    }

    #[test]
    fn an_implicit_mentioned_only_by_the_output_is_inferrable() {
        let n = SortExpr::from("N");
        let only_output = Operation::with_implicit(
            "pick",
            vec![(Arc::from("n"), n.clone(), Implicit::Yes)],
            SortExpr::app("Box", vec![Term::var("n")]),
        );
        assert!(check_implicits_inferrable(&only_output).is_ok());
        let by_input = Operation::with_implicit(
            "unbox",
            vec![
                (Arc::from("n"), n.clone(), Implicit::Yes),
                (
                    Arc::from("b"),
                    SortExpr::app("Box", vec![Term::var("n")]),
                    Implicit::No,
                ),
            ],
            "N",
        );
        assert!(check_implicits_inferrable(&by_input).is_ok());
        let nowhere =
            Operation::with_implicit("lost", vec![(Arc::from("n"), n, Implicit::Yes)], "N");
        match check_implicits_inferrable(&nowhere) {
            Err(GatError::NonInferrableImplicit { op, param }) => {
                assert_eq!((op.as_str(), param.as_str()), ("lost", "n"));
            }
            other => panic!("expected NonInferrableImplicit, got {other:?}"),
        }
    }

    /// `N` with `z`, `s`; `Vec(n : N)`; `len : (m : N, v : Vec(m)) -> N`,
    /// `at_zero : (v : Vec(z())) -> N`, `at_one : (v : Vec(s(z()))) -> N`,
    /// and `size : {n : N} (v : Vec(n)) -> N`.
    fn vec_theory() -> Theory {
        let vec_of = |t: Term| SortExpr::app("Vec", vec![t]);
        Theory::new(
            "Vecs",
            vec![
                Sort::simple("N"),
                Sort::dependent("Vec", vec![SortParam::new("n", "N")]),
            ],
            vec![
                Operation::nullary("z", "N"),
                Operation::unary("s", "p", "N", "N"),
                Operation::new(
                    "len",
                    vec![
                        (Arc::from("m"), SortExpr::from("N")),
                        (Arc::from("v"), vec_of(Term::var("m"))),
                    ],
                    "N",
                ),
                Operation::unary("at_zero", "v", vec_of(Term::constant("z")), "N"),
                Operation::unary(
                    "at_one",
                    "v",
                    vec_of(Term::app("s", vec![Term::constant("z")])),
                    "N",
                ),
                Operation::with_implicit(
                    "size",
                    vec![
                        (Arc::from("n"), SortExpr::from("N"), Implicit::Yes),
                        (Arc::from("v"), vec_of(Term::var("n")), Implicit::No),
                    ],
                    "N",
                ),
                Operation::new(
                    "add",
                    vec![
                        (Arc::from("x"), SortExpr::from("N")),
                        (Arc::from("y"), SortExpr::from("N")),
                    ],
                    "N",
                ),
            ],
            Vec::new(),
        )
    }

    #[test]
    fn inferred_variable_sorts_carry_the_unifier() -> Result<(), Box<dyn std::error::Error>> {
        let eq = Equation::new(
            "e",
            Term::app("len", vec![Term::var("k"), Term::var("v")]),
            Term::app("at_zero", vec![Term::var("v")]),
        );
        let ctx = infer_var_sorts(&eq, &vec_theory())?;
        assert_eq!(
            ctx.get("v"),
            Some(&SortExpr::app("Vec", vec![Term::constant("z")]))
        );
        assert_eq!(ctx.get("k"), Some(&SortExpr::from("N")));
        Ok(())
    }

    #[test]
    fn each_application_of_an_implicit_operation_gets_its_own_metavariable()
    -> Result<(), Box<dyn std::error::Error>> {
        let eq = Equation::new(
            "e",
            Term::app(
                "add",
                vec![
                    Term::app("size", vec![Term::var("x")]),
                    Term::app("size", vec![Term::var("y")]),
                ],
            ),
            Term::app(
                "add",
                vec![
                    Term::app("at_zero", vec![Term::var("x")]),
                    Term::app("at_one", vec![Term::var("y")]),
                ],
            ),
        );
        let ctx = infer_var_sorts(&eq, &vec_theory())?;
        assert_eq!(
            ctx.get("x"),
            Some(&SortExpr::app("Vec", vec![Term::constant("z")]))
        );
        assert_eq!(
            ctx.get("y"),
            Some(&SortExpr::app(
                "Vec",
                vec![Term::app("s", vec![Term::constant("z")])]
            ))
        );
        Ok(())
    }

    #[test]
    fn the_hole_path_accepts_an_implicit_application_of_the_right_arity()
    -> Result<(), Box<dyn std::error::Error>> {
        let ctx = ctx_of(&[("v", SortExpr::app("Vec", vec![Term::constant("z")]))]);
        let (sort, reports) = typecheck_term_with_holes(
            &Term::app("size", vec![Term::var("v")]),
            &ctx,
            &vec_theory(),
        )?;
        assert_eq!(sort, SortExpr::from("N"));
        assert!(reports.is_empty());
        let wrong = typecheck_term_with_holes(&Term::app("size", vec![]), &ctx, &vec_theory());
        assert!(
            matches!(
                wrong,
                Err(GatError::TermArityMismatch {
                    expected: 1,
                    got: 0,
                    ..
                })
            ),
            "{wrong:?}"
        );
        Ok(())
    }

    #[test]
    fn the_hole_path_rejects_an_argument_of_the_wrong_sort() {
        let result = typecheck_term_with_holes(
            &Term::app("f", vec![Term::constant("b")]),
            &VarContext::default(),
            &case_theory(),
        );
        match result {
            Err(GatError::ArgTypeMismatch {
                op,
                arg_index,
                expected,
                got,
            }) => {
                assert_eq!((op.as_str(), arg_index), ("f", 0));
                assert_eq!((expected.as_str(), got.as_str()), ("A", "B"));
            }
            other => panic!("expected ArgTypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn the_hole_path_checks_case_branches_against_the_motive()
    -> Result<(), Box<dyn std::error::Error>> {
        let theory = case_theory();
        let ctx = ctx_of(&[("p", SortExpr::from("Bool"))]);
        let wrapped = |tt_body: Term, ff_body: Term| {
            Term::app(
                "wrap",
                vec![case(
                    Term::var("p"),
                    vec![branch("tt", &[], tt_body), branch("ff", &[], ff_body)],
                )],
            )
        };
        let (sort, reports) = typecheck_term_with_holes(
            &wrapped(
                Term::constant("a"),
                Term::app("g", vec![Term::constant("b")]),
            ),
            &ctx,
            &theory,
        )?;
        assert_eq!(sort, SortExpr::from("W"));
        assert!(reports.is_empty());

        let (sort, reports) =
            typecheck_term_with_holes(&wrapped(Term::constant("a"), hole()), &ctx, &theory)?;
        assert_eq!(sort, SortExpr::from("W"));
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].expected, SortExpr::from("A"));

        match typecheck_term_with_holes(
            &wrapped(Term::constant("a"), Term::constant("b")),
            &ctx,
            &theory,
        ) {
            Err(GatError::CaseBranchSortMismatch {
                constructor,
                expected,
                got,
            }) => {
                assert_eq!(constructor, "ff");
                assert_eq!((expected.as_str(), got.as_str()), ("A", "B"));
            }
            other => panic!("expected CaseBranchSortMismatch, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn the_hole_path_checks_exhaustiveness_over_a_closed_sort() {
        let ctx = ctx_of(&[("p", SortExpr::from("Bool"))]);
        let partial = case(Term::var("p"), vec![branch("tt", &[], Term::constant("a"))]);
        match typecheck_term_with_holes(&partial, &ctx, &case_theory()) {
            Err(GatError::NonExhaustiveCase { sort, missing }) => {
                assert_eq!(sort, "Bool");
                assert_eq!(missing, ["ff"]);
            }
            other => panic!("expected NonExhaustiveCase, got {other:?}"),
        }
    }

    #[test]
    fn a_branch_whose_constructor_builds_another_sort_is_rejected() {
        let ctx = ctx_of(&[("o", SortExpr::from("Open"))]);
        let stray = case(Term::var("o"), vec![branch("a", &[], Term::constant("b"))]);
        match typecheck_term_with_holes(&stray, &ctx, &case_theory()) {
            Err(GatError::OpTypeMismatch { op, detail }) => {
                assert_eq!(op, "a");
                assert_eq!(
                    detail,
                    "constructor output sort A does not match scrutinee sort Open"
                );
            }
            other => panic!("expected OpTypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn a_listed_constructor_of_another_sort_is_not_admitted()
    -> Result<(), Box<dyn std::error::Error>> {
        // `Flag` lists `up` and `a`, but `a` builds an `A`, so a case on a
        // flag needs (and admits) only the `up` branch.
        let theory = Theory::new(
            "Stray",
            vec![Sort::simple("A"), Sort::closed("Flag", vec![], ["up", "a"])],
            vec![
                Operation::nullary("a", "A"),
                Operation::nullary("up", "Flag"),
            ],
            Vec::new(),
        );
        let ctx = ctx_of(&[("p", SortExpr::from("Flag"))]);
        let only_up = case(Term::var("p"), vec![branch("up", &[], Term::constant("a"))]);
        assert_eq!(
            typecheck_term(&only_up, &ctx, &theory)?,
            SortExpr::from("A")
        );
        Ok(())
    }

    /// An equation both of whose sides misapply an operation: `f` to the
    /// `B` constant on the left (or inside a case scrutinee there), `g` to
    /// the `A` constant on the right.
    fn doubly_ill_typed(lhs_is_case: bool) -> Equation {
        let bad_f = Term::app("f", vec![Term::constant("b")]);
        let lhs = if lhs_is_case {
            case(bad_f, vec![branch("tt", &[], Term::constant("a"))])
        } else {
            bad_f
        };
        Equation::new("bad", lhs, Term::app("g", vec![Term::constant("a")]))
    }

    fn mismatched_op(result: Result<(), GatError>) -> String {
        match result {
            Err(GatError::ArgTypeMismatch { op, .. }) => op,
            other => panic!("expected ArgTypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn an_equation_is_checked_from_its_non_case_side() {
        type EquationCheck = fn(&Equation, &Theory) -> Result<(), GatError>;
        let theory = case_theory();
        let checks: [EquationCheck; 2] = [typecheck_equation, |eq, th| {
            typecheck_equation_modulo_rewrites(eq, th, &[], 16)
        }];
        for check in checks {
            assert_eq!(mismatched_op(check(&doubly_ill_typed(false), &theory)), "f");
            assert_eq!(mismatched_op(check(&doubly_ill_typed(true), &theory)), "g");
        }
    }

    #[test]
    fn holes_in_an_equation_are_counted_on_both_sides() {
        let eq = Equation::new(
            "holey",
            Term::app("f", vec![hole()]),
            Term::app("f", vec![hole()]),
        );
        let theory = case_theory();
        for result in [
            typecheck_equation(&eq, &theory),
            typecheck_equation_modulo_rewrites(&eq, &theory, &[], 16),
        ] {
            match result {
                Err(GatError::HolesInEquation { equation, count }) => {
                    assert_eq!((equation.as_str(), count), ("holey", 2));
                }
                other => panic!("expected HolesInEquation, got {other:?}"),
            }
        }
    }

    /// `Tm(n : N)` with `left : Tm(add(z(), s(z())))` and
    /// `right : Tm(s(z()))`, which have one sort modulo `add(z(), y) -> y`.
    fn indexed_constants() -> Theory {
        let base = vec_theory();
        Theory::new(
            "Indexed",
            base.sorts
                .iter()
                .cloned()
                .chain([Sort::dependent("Tm", vec![SortParam::new("n", "N")])])
                .collect(),
            base.ops
                .iter()
                .cloned()
                .chain([
                    Operation::nullary(
                        "left",
                        SortExpr::app(
                            "Tm",
                            vec![Term::app(
                                "add",
                                vec![
                                    Term::constant("z"),
                                    Term::app("s", vec![Term::constant("z")]),
                                ],
                            )],
                        ),
                    ),
                    Operation::nullary(
                        "right",
                        SortExpr::app("Tm", vec![Term::app("s", vec![Term::constant("z")])]),
                    ),
                    Operation::nullary("zero_tm", SortExpr::app("Tm", vec![Term::constant("z")])),
                ])
                .collect(),
            Vec::new(),
        )
    }

    fn left_unit() -> crate::eq::DirectedEquation {
        crate::eq::DirectedEquation::new(
            "left_unit",
            Term::app("add", vec![Term::constant("z"), Term::var("y")]),
            Term::var("y"),
            panproto_expr::Expr::Var("_".into()),
        )
    }

    #[test]
    fn equation_sorts_are_compared_modulo_the_given_rules() {
        let theory = indexed_constants();
        let eq = Equation::new("lr", Term::constant("left"), Term::constant("right"));
        assert!(typecheck_equation_modulo_rewrites(&eq, &theory, &[left_unit()], 16).is_ok());
        assert!(typecheck_equation(&eq, &theory).is_err());
        match typecheck_equation_modulo_rewrites(&eq, &theory, &[], 16) {
            Err(GatError::EquationSortMismatch {
                equation,
                lhs_sort,
                rhs_sort,
            }) => {
                assert_eq!(equation, "lr");
                assert_eq!(lhs_sort, "Tm(add(z(), s(z())))");
                assert_eq!(rhs_sort, "Tm(s(z()))");
            }
            other => panic!("expected EquationSortMismatch, got {other:?}"),
        }
        let unrelated = Equation::new("lz", Term::constant("left"), Term::constant("zero_tm"));
        assert!(matches!(
            typecheck_equation_modulo_rewrites(&unrelated, &theory, &[left_unit()], 16),
            Err(GatError::EquationSortMismatch { .. })
        ));
        let same = Equation::new("rr", Term::constant("right"), Term::constant("right"));
        assert!(typecheck_equation_modulo_rewrites(&same, &theory, &[], 16).is_ok());
    }

    #[test]
    fn an_exhausted_sort_comparison_is_reported_as_such() {
        let theory = indexed_constants();
        let eq = Equation::new("lr", Term::constant("left"), Term::constant("right"));
        let swap = crate::eq::DirectedEquation::new(
            "swap",
            Term::app("add", vec![Term::var("x"), Term::var("y")]),
            Term::app("add", vec![Term::var("y"), Term::var("x")]),
            panproto_expr::Expr::Var("_".into()),
        );
        match typecheck_equation_modulo_rewrites(&eq, &theory, &[swap], 8) {
            Err(GatError::RewriteBudgetExhausted { context, limit }) => {
                assert_eq!(context, "equation `lr` sort equality");
                assert_eq!(limit, 8);
            }
            other => panic!("expected RewriteBudgetExhausted, got {other:?}"),
        }
    }

    #[test]
    fn renaming_a_case_binder_leaves_the_motive_unchanged() -> Result<(), Box<dyn std::error::Error>>
    {
        // `mk : (k : N) -> Fin(k)`; the case on `v : Fin(m)` sits where
        // `use` expects `P(j)` for the outer `j`. A branch binder that
        // happens to be called `j` must not change what that `j` means.
        let theory = Theory::new(
            "Shadow",
            vec![
                Sort::simple("N"),
                Sort::closed("Fin", vec![SortParam::new("n", "N")], ["mk"]),
                Sort::dependent("P", vec![SortParam::new("a", "N")]),
                Sort::simple("Out"),
            ],
            vec![
                Operation::unary("mk", "k", "N", SortExpr::app("Fin", vec![Term::var("k")])),
                Operation::new(
                    "use",
                    vec![
                        (Arc::from("a"), SortExpr::from("N")),
                        (Arc::from("b"), SortExpr::app("P", vec![Term::var("a")])),
                    ],
                    "Out",
                ),
            ],
            Vec::new(),
        );
        let ctx = ctx_of(&[
            ("j", SortExpr::from("N")),
            ("m", SortExpr::from("N")),
            ("v", SortExpr::app("Fin", vec![Term::var("m")])),
        ]);
        let with_binder = |binder: &str| {
            Term::app(
                "use",
                vec![
                    Term::var("j"),
                    case(Term::var("v"), vec![branch("mk", &[binder], hole())]),
                ],
            )
        };
        let (fresh_sort, fresh_holes) =
            typecheck_term_with_holes(&with_binder("q"), &ctx, &theory)?;
        let (shadow_sort, shadow_holes) =
            typecheck_term_with_holes(&with_binder("j"), &ctx, &theory)?;
        assert_eq!(fresh_sort, SortExpr::from("Out"));
        assert_eq!(shadow_sort, fresh_sort, "the term's sort");
        let expected = |holes: &[HoleReport]| -> Vec<SortExpr> {
            holes.iter().map(|h| h.expected.clone()).collect()
        };
        assert_eq!(
            expected(&fresh_holes),
            [SortExpr::app("P", vec![Term::var("j")])]
        );
        assert_eq!(
            expected(&shadow_holes),
            expected(&fresh_holes),
            "the hole's expected sort"
        );
        Ok(())
    }
}
