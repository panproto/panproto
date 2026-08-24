use std::sync::Arc;

/// A term in a GAT expression.
///
/// Terms are built from variables, operation applications, and case
/// analyses on closed-sort scrutinees. The `Case` variant is
/// exhaustiveness-checked at `typecheck_term` time; the scrutinee's
/// head sort must carry [`crate::sort::SortClosure::Closed`] with a
/// complete list of constructors.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Term {
    /// A variable reference (e.g., `x`, `a`).
    Var(Arc<str>),
    /// An operation applied to arguments (e.g., `add(x, y)`).
    App {
        /// The operation name.
        op: Arc<str>,
        /// The argument terms.
        args: Vec<Self>,
    },
    /// A case analysis on a closed-sort scrutinee.
    ///
    /// Every branch binds one name per argument of its constructor.
    /// Typechecking verifies exhaustiveness (every constructor listed
    /// in the scrutinee's closure appears exactly once) and that all
    /// branch bodies produce the same output sort.
    Case {
        /// The term being case-analysed.
        scrutinee: Box<Self>,
        /// One branch per constructor of the scrutinee's closed sort.
        branches: Vec<CaseBranch>,
    },
    /// A typed hole: a placeholder with an optional name. Typechecking
    /// assigns a fresh metavariable sort and records a [`crate::typecheck::HoleReport`]
    /// so callers can inspect the expected sort at each hole site.
    Hole {
        /// Optional name for the hole (e.g. `?foo`); `None` for an
        /// anonymous `?`.
        name: Option<Arc<str>>,
    },
    /// A local `let`-binding: `let name = bound in body`.
    ///
    /// GAT signatures are first-order and their sorts have no free
    /// sort-metavariables; there is nothing to generalize over. The
    /// bound term's inferred sort is therefore bound monomorphically
    /// into the context when typechecking the body. The
    /// [`crate::typecheck::SortScheme`] type is retained for future
    /// extension, but `typecheck_term` currently always produces a
    /// scheme with an empty `metavars` list at a Let site.
    Let {
        /// Bound name.
        name: Arc<str>,
        /// The bound term.
        bound: Box<Self>,
        /// The body in which `name` is in scope.
        body: Box<Self>,
    },
}

/// One branch of a [`Term::Case`] expression.
///
/// The `constructor` field must be an op whose output head matches the
/// scrutinee's sort; `binders` supplies one local name per input of
/// that op. The body typechecks in an extended context that binds
/// each binder to the corresponding input sort.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CaseBranch {
    /// Constructor op name.
    pub constructor: Arc<str>,
    /// Local binders; one per input of `constructor`.
    pub binders: Vec<Arc<str>>,
    /// The branch body.
    pub body: Term,
}

impl Term {
    /// Create a variable term.
    #[must_use]
    pub fn var(name: impl Into<Arc<str>>) -> Self {
        Self::Var(name.into())
    }

    /// Create an application term.
    #[must_use]
    pub fn app(op: impl Into<Arc<str>>, args: Vec<Self>) -> Self {
        Self::App {
            op: op.into(),
            args,
        }
    }

    /// Create a nullary application (constant).
    #[must_use]
    pub fn constant(op: impl Into<Arc<str>>) -> Self {
        Self::App {
            op: op.into(),
            args: Vec::new(),
        }
    }

    /// Apply a substitution (variable name → term) to this term.
    ///
    /// The substitution is capture-avoiding. Under [`Self::Case`] and
    /// [`Self::Let`], a binder shadows the outer scope, so a binding in
    /// `subst` for a name the binder also binds is dropped when
    /// descending into the body; a binder that would otherwise capture a
    /// free variable of the substitution's image is alpha-renamed to a
    /// fresh name first. Variables inside a scrutinee or a `let`-bound
    /// term are always substituted, since those sit in the outer scope.
    #[must_use]
    pub fn substitute(&self, subst: &rustc_hash::FxHashMap<Arc<str>, Self>) -> Self {
        match self {
            Self::Var(name) => subst.get(name).cloned().unwrap_or_else(|| self.clone()),
            Self::Hole { .. } => self.clone(),
            Self::App { op, args } => Self::App {
                op: Arc::clone(op),
                args: args.iter().map(|a| a.substitute(subst)).collect(),
            },
            Self::Case {
                scrutinee,
                branches,
            } => Self::Case {
                scrutinee: Box::new(scrutinee.substitute(subst)),
                branches: branches
                    .iter()
                    .map(|b| substitute_case_branch(b, subst))
                    .collect(),
            },
            Self::Let { name, bound, body } => {
                let new_bound = Box::new(bound.substitute(subst));
                let mut inner = subst.clone();
                inner.remove(name);
                let body_free = body.free_vars();
                let image = image_free_vars(&inner, &body_free);
                if image.contains(name) {
                    // The binder would capture a free variable of the
                    // substitution's image; alpha-rename it first.
                    let mut avoid = image;
                    avoid.extend(body_free);
                    avoid.insert(Arc::clone(name));
                    let fresh = fresh_binder(name, &avoid);
                    let mut rename = rustc_hash::FxHashMap::default();
                    rename.insert(Arc::clone(name), Self::Var(Arc::clone(&fresh)));
                    let renamed_body = body.substitute(&rename);
                    Self::Let {
                        name: fresh,
                        bound: new_bound,
                        body: Box::new(renamed_body.substitute(&inner)),
                    }
                } else {
                    Self::Let {
                        name: Arc::clone(name),
                        bound: new_bound,
                        body: Box::new(body.substitute(&inner)),
                    }
                }
            }
        }
    }

    /// Collect all free variables in this term.
    #[must_use]
    pub fn free_vars(&self) -> rustc_hash::FxHashSet<Arc<str>> {
        let mut vars = rustc_hash::FxHashSet::default();
        self.collect_vars(&mut vars);
        vars
    }

    fn collect_vars(&self, vars: &mut rustc_hash::FxHashSet<Arc<str>>) {
        match self {
            Self::Var(name) => {
                vars.insert(Arc::clone(name));
            }
            Self::Hole { .. } => {}
            Self::App { args, .. } => {
                for arg in args {
                    arg.collect_vars(vars);
                }
            }
            Self::Case {
                scrutinee,
                branches,
            } => {
                scrutinee.collect_vars(vars);
                for b in branches {
                    // Branch binders shadow outer names inside the
                    // body; compute the body's free vars locally and
                    // subtract the binders before merging.
                    let mut local = rustc_hash::FxHashSet::default();
                    b.body.collect_vars(&mut local);
                    for binder in &b.binders {
                        local.remove(binder);
                    }
                    vars.extend(local);
                }
            }
            Self::Let { name, bound, body } => {
                bound.collect_vars(vars);
                let mut local = rustc_hash::FxHashSet::default();
                body.collect_vars(&mut local);
                local.remove(name);
                vars.extend(local);
            }
        }
    }

    /// Apply an operation renaming to this term.
    ///
    /// A [`Self::Case`] branch's constructor op is also renamed via
    /// `op_map`; branch binders and the scrutinee term are recursed
    /// into.
    #[must_use]
    pub fn rename_ops(&self, op_map: &std::collections::HashMap<Arc<str>, Arc<str>>) -> Self {
        match self {
            Self::Var(_) | Self::Hole { .. } => self.clone(),
            Self::App { op, args } => Self::App {
                op: op_map.get(op).cloned().unwrap_or_else(|| Arc::clone(op)),
                args: args.iter().map(|a| a.rename_ops(op_map)).collect(),
            },
            Self::Case {
                scrutinee,
                branches,
            } => Self::Case {
                scrutinee: Box::new(scrutinee.rename_ops(op_map)),
                branches: branches
                    .iter()
                    .map(|b| CaseBranch {
                        constructor: op_map
                            .get(&b.constructor)
                            .cloned()
                            .unwrap_or_else(|| Arc::clone(&b.constructor)),
                        binders: b.binders.clone(),
                        body: b.body.rename_ops(op_map),
                    })
                    .collect(),
            },
            Self::Let { name, bound, body } => Self::Let {
                name: Arc::clone(name),
                bound: Box::new(bound.rename_ops(op_map)),
                body: Box::new(body.rename_ops(op_map)),
            },
        }
    }
}

/// The free variables of the substitution's image, restricted to the
/// variables that actually occur free in the body being descended into.
///
/// A binder can only capture a name that some surviving replacement term
/// actually contributes to the body, so restricting the image to the
/// body's free variables keeps alpha-renaming to the cases that need it.
fn image_free_vars(
    subst: &rustc_hash::FxHashMap<Arc<str>, Term>,
    body_free: &rustc_hash::FxHashSet<Arc<str>>,
) -> rustc_hash::FxHashSet<Arc<str>> {
    let mut image = rustc_hash::FxHashSet::default();
    for (name, replacement) in subst {
        if body_free.contains(name) {
            image.extend(replacement.free_vars());
        }
    }
    image
}

/// Derive a binder name outside `avoid` by appending primes to `base`.
fn fresh_binder(base: &str, avoid: &rustc_hash::FxHashSet<Arc<str>>) -> Arc<str> {
    let mut candidate = format!("{base}'");
    while avoid.contains(candidate.as_str()) {
        candidate.push('\'');
    }
    Arc::from(candidate)
}

/// Apply a substitution inside one case branch, alpha-renaming the
/// branch's binders where they would capture a free variable of the
/// substitution's image.
///
/// Binders shadow the outer scope, so their entries are dropped from the
/// substitution before descending. A binder that a later binder in the
/// same list shadows binds no occurrence in the body and is left alone.
fn substitute_case_branch(
    branch: &CaseBranch,
    subst: &rustc_hash::FxHashMap<Arc<str>, Term>,
) -> CaseBranch {
    let mut inner = subst.clone();
    for binder in &branch.binders {
        inner.remove(binder);
    }
    let body_free = branch.body.free_vars();
    let image = image_free_vars(&inner, &body_free);
    let mut avoid = image.clone();
    avoid.extend(body_free);
    avoid.extend(branch.binders.iter().map(Arc::clone));

    let mut binders = Vec::with_capacity(branch.binders.len());
    let mut body = branch.body.clone();
    for (position, binder) in branch.binders.iter().enumerate() {
        let shadowed_later = branch.binders[position + 1..].contains(binder);
        if shadowed_later || !image.contains(binder) {
            binders.push(Arc::clone(binder));
            continue;
        }
        let fresh = fresh_binder(binder, &avoid);
        avoid.insert(Arc::clone(&fresh));
        let mut rename = rustc_hash::FxHashMap::default();
        rename.insert(Arc::clone(binder), Term::Var(Arc::clone(&fresh)));
        body = body.substitute(&rename);
        binders.push(fresh);
    }

    CaseBranch {
        constructor: Arc::clone(&branch.constructor),
        binders,
        body: body.substitute(&inner),
    }
}

/// Compose two term substitutions.
///
/// The composition `compose_subst(tau, sigma)` is the substitution that
/// behaves like applying `sigma` first and then `tau`: for every
/// variable `x`, `compose_subst(tau, sigma)(x) = sigma(x).substitute(tau)`
/// when `x ∈ dom sigma`, `tau(x)` when `x ∈ dom tau \ dom sigma`, and
/// `Var(x)` otherwise. The resulting map carries the semantics
/// `t.substitute(sigma).substitute(tau) == t.substitute(compose_subst(tau, sigma))`
/// for every term `t`.
///
/// The order of arguments mirrors function composition: `compose_subst(tau,
/// sigma)` is the pushforward of `sigma` along `tau`, i.e. `tau ∘ sigma`.
#[must_use]
pub fn compose_subst<S1, S2>(
    tau: &std::collections::HashMap<Arc<str>, Term, S1>,
    sigma: &std::collections::HashMap<Arc<str>, Term, S2>,
) -> rustc_hash::FxHashMap<Arc<str>, Term>
where
    S1: std::hash::BuildHasher,
    S2: std::hash::BuildHasher,
{
    // Build a transient FxHashMap view over `tau` for `substitute`, which
    // requires the FxHasher specifically.
    let tau_fx: rustc_hash::FxHashMap<Arc<str>, Term> = tau
        .iter()
        .map(|(k, v)| (Arc::clone(k), v.clone()))
        .collect();
    let mut out: rustc_hash::FxHashMap<Arc<str>, Term> = rustc_hash::FxHashMap::default();
    for (x, t) in sigma {
        out.insert(Arc::clone(x), t.substitute(&tau_fx));
    }
    for (x, t) in tau {
        out.entry(Arc::clone(x)).or_insert_with(|| t.clone());
    }
    out
}

/// An equation (axiom) in a GAT.
///
/// Equations express judgemental equalities between terms.
/// They must hold in every model of the theory.
///
/// # Examples
///
/// - Identity law: `add(x, zero()) = x`
/// - Commutativity: `mul(a, b) = mul(b, a)`
/// - Associativity: `compose(f, compose(g, h)) = compose(compose(f, g), h)`
///
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Equation {
    /// A human-readable name for this equation (e.g., `left_identity`).
    pub name: Arc<str>,
    /// The left-hand side of the equality.
    pub lhs: Term,
    /// The right-hand side of the equality.
    pub rhs: Term,
}

impl Equation {
    /// Create a new equation.
    #[must_use]
    pub fn new(name: impl Into<Arc<str>>, lhs: Term, rhs: Term) -> Self {
        Self {
            name: name.into(),
            lhs,
            rhs,
        }
    }

    /// Apply an operation renaming to both sides of this equation.
    #[must_use]
    pub fn rename_ops(&self, op_map: &std::collections::HashMap<Arc<str>, Arc<str>>) -> Self {
        Self {
            name: Arc::clone(&self.name),
            lhs: self.lhs.rename_ops(op_map),
            rhs: self.rhs.rename_ops(op_map),
        }
    }
}

/// A directed equation (rewrite rule) with a computation term.
///
/// Unlike [`Equation`] which asserts an undirected equality (`lhs = rhs`),
/// a directed equation specifies a computation direction: when the engine
/// encounters a value matching `lhs`, it rewrites to `rhs` using `impl_term`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DirectedEquation {
    /// A human-readable name for this directed equation.
    pub name: Arc<str>,
    /// The left-hand side (pattern to match).
    pub lhs: Term,
    /// The right-hand side (rewrite target).
    pub rhs: Term,
    /// The computable implementation of the rewrite (forward direction).
    pub impl_term: panproto_expr::Expr,
    /// Optional inverse for the backward (put) direction.
    pub inverse: Option<panproto_expr::Expr>,
    /// Source value kind (if this is a value-level coercion).
    pub source_kind: Option<crate::sort::ValueKind>,
    /// Target value kind (if this is a value-level coercion).
    pub target_kind: Option<crate::sort::ValueKind>,
    /// Round-trip classification of this directed equation as a coercion.
    pub coercion_class: crate::sort::CoercionClass,
}

/// Check if two terms are α-equivalent (equal up to consistent variable renaming).
///
/// Two terms are α-equivalent when there exists a bijection between their
/// free variables such that applying the bijection to one term produces the
/// other. All variables in equation contexts are universally quantified,
/// so α-equivalence is the correct notion of equality for equation terms.
#[must_use]
pub fn alpha_equivalent(t1: &Term, t2: &Term) -> bool {
    let mut checker = AlphaChecker {
        forward: rustc_hash::FxHashMap::default(),
        backward: rustc_hash::FxHashMap::default(),
    };
    checker.check(t1, t2)
}

/// Check if two equations are α-equivalent.
///
/// Uses a single variable bijection across both sides, since variables
/// in an equation are universally quantified over the entire equation.
/// This means `∀x. f(x,x) = g(x)` is α-equivalent to `∀y. f(y,y) = g(y)`
/// but NOT to `∀a,b. f(a,b) = g(a)`.
#[must_use]
pub fn alpha_equivalent_equation(lhs1: &Term, rhs1: &Term, lhs2: &Term, rhs2: &Term) -> bool {
    let mut checker = AlphaChecker {
        forward: rustc_hash::FxHashMap::default(),
        backward: rustc_hash::FxHashMap::default(),
    };
    checker.check(lhs1, lhs2) && checker.check(rhs1, rhs2)
}

struct AlphaChecker {
    forward: rustc_hash::FxHashMap<Arc<str>, Arc<str>>,
    backward: rustc_hash::FxHashMap<Arc<str>, Arc<str>>,
}

impl AlphaChecker {
    fn check(&mut self, t1: &Term, t2: &Term) -> bool {
        match (t1, t2) {
            (Term::Var(a), Term::Var(b)) => self.check_vars(a, b),
            (
                Term::App {
                    op: op1,
                    args: args1,
                },
                Term::App {
                    op: op2,
                    args: args2,
                },
            ) => {
                op1 == op2
                    && args1.len() == args2.len()
                    && args1
                        .iter()
                        .zip(args2.iter())
                        .all(|(a1, a2)| self.check(a1, a2))
            }
            (
                Term::Case {
                    scrutinee: s1,
                    branches: b1,
                },
                Term::Case {
                    scrutinee: s2,
                    branches: b2,
                },
            ) => {
                if !self.check(s1, s2) {
                    return false;
                }
                if b1.len() != b2.len() {
                    return false;
                }
                for (br1, br2) in b1.iter().zip(b2.iter()) {
                    if br1.constructor != br2.constructor || br1.binders.len() != br2.binders.len()
                    {
                        return false;
                    }
                    // Extend bijection with branch binders, check body,
                    // then roll back.
                    let saved_forward = self.forward.clone();
                    let saved_backward = self.backward.clone();
                    let mut ok = true;
                    for (a, b) in br1.binders.iter().zip(br2.binders.iter()) {
                        self.forward.insert(Arc::clone(a), Arc::clone(b));
                        self.backward.insert(Arc::clone(b), Arc::clone(a));
                    }
                    if !self.check(&br1.body, &br2.body) {
                        ok = false;
                    }
                    self.forward = saved_forward;
                    self.backward = saved_backward;
                    if !ok {
                        return false;
                    }
                }
                true
            }
            (Term::Hole { name: n1 }, Term::Hole { name: n2 }) => n1 == n2,
            (
                Term::Let {
                    name: n1,
                    bound: b1,
                    body: body1,
                },
                Term::Let {
                    name: n2,
                    bound: b2,
                    body: body2,
                },
            ) => self.check_let(n1, b1, body1, n2, b2, body2),
            (
                Term::Var(_)
                | Term::App { .. }
                | Term::Case { .. }
                | Term::Hole { .. }
                | Term::Let { .. },
                _,
            ) => false,
        }
    }

    fn check_vars(&mut self, a: &Arc<str>, b: &Arc<str>) -> bool {
        if let Some(mapped) = self.forward.get(a) {
            if mapped != b {
                return false;
            }
        } else if let Some(mapped_back) = self.backward.get(b) {
            if mapped_back != a {
                return false;
            }
        } else {
            self.forward.insert(Arc::clone(a), Arc::clone(b));
            self.backward.insert(Arc::clone(b), Arc::clone(a));
        }
        true
    }

    fn check_let(
        &mut self,
        n1: &Arc<str>,
        b1: &Term,
        body1: &Term,
        n2: &Arc<str>,
        b2: &Term,
        body2: &Term,
    ) -> bool {
        if !self.check(b1, b2) {
            return false;
        }
        let saved_forward = self.forward.clone();
        let saved_backward = self.backward.clone();
        self.forward.insert(Arc::clone(n1), Arc::clone(n2));
        self.backward.insert(Arc::clone(n2), Arc::clone(n1));
        let ok = self.check(body1, body2);
        self.forward = saved_forward;
        self.backward = saved_backward;
        ok
    }
}

/// Try to match a pattern term against a concrete term, returning a substitution
/// if successful. Pattern variables can match any subterm; operation names must
/// match exactly.
///
/// This is first-order pattern matching (not full unification): pattern
/// variables are the variables in `pattern`, and they are matched against
/// subterms of `term`.
#[must_use]
pub fn match_pattern(pattern: &Term, term: &Term) -> Option<rustc_hash::FxHashMap<Arc<str>, Term>> {
    let mut subst = rustc_hash::FxHashMap::default();
    if match_pattern_inner(pattern, term, &mut subst) {
        Some(subst)
    } else {
        None
    }
}

fn match_pattern_inner(
    pattern: &Term,
    term: &Term,
    subst: &mut rustc_hash::FxHashMap<Arc<str>, Term>,
) -> bool {
    match pattern {
        Term::Var(name) => {
            if subst.contains_key(name) {
                subst.get(name).is_some_and(|existing| existing == term)
            } else {
                subst.insert(Arc::clone(name), term.clone());
                true
            }
        }
        Term::App {
            op: p_op,
            args: p_args,
        } => match term {
            Term::App {
                op: t_op,
                args: t_args,
            } => {
                p_op == t_op
                    && p_args.len() == t_args.len()
                    && p_args
                        .iter()
                        .zip(t_args.iter())
                        .all(|(p, t)| match_pattern_inner(p, t, subst))
            }
            Term::Var(_) | Term::Case { .. } | Term::Hole { .. } | Term::Let { .. } => false,
        },
        Term::Case {
            scrutinee: p_s,
            branches: p_b,
        } => match term {
            Term::Case {
                scrutinee: t_s,
                branches: t_b,
            } => {
                if p_b.len() != t_b.len() || !match_pattern_inner(p_s, t_s, subst) {
                    return false;
                }
                for (pb, tb) in p_b.iter().zip(t_b.iter()) {
                    if pb.constructor != tb.constructor || pb.binders.len() != tb.binders.len() {
                        return false;
                    }
                    // Save the subst, extend it with positional binder
                    // renamings p_binders[i] := Var(t_binders[i]), match
                    // the branch body, then restore the outer subst.
                    // Binder names are branch-local and must not leak.
                    let saved = subst.clone();
                    for (pb_b, tb_b) in pb.binders.iter().zip(tb.binders.iter()) {
                        subst.insert(Arc::clone(pb_b), Term::Var(Arc::clone(tb_b)));
                    }
                    let ok = match_pattern_inner(&pb.body, &tb.body, subst);
                    *subst = saved;
                    if !ok {
                        return false;
                    }
                }
                true
            }
            Term::Var(_) | Term::App { .. } | Term::Hole { .. } | Term::Let { .. } => false,
        },
        Term::Hole { name } => match term {
            Term::Hole { name: n2 } => name == n2,
            Term::Var(_) | Term::App { .. } | Term::Case { .. } | Term::Let { .. } => false,
        },
        Term::Let {
            name: p_n,
            bound: p_b,
            body: p_body,
        } => match term {
            Term::Let {
                name: t_n,
                bound: t_b,
                body: t_body,
            } => {
                if !match_pattern_inner(p_b, t_b, subst) {
                    return false;
                }
                let saved = subst.clone();
                subst.insert(Arc::clone(p_n), Term::Var(Arc::clone(t_n)));
                let ok = match_pattern_inner(p_body, t_body, subst);
                *subst = saved;
                ok
            }
            Term::Var(_) | Term::App { .. } | Term::Case { .. } | Term::Hole { .. } => false,
        },
    }
}

/// Normalize a term by repeatedly applying directed equations as rewrite rules.
///
/// Uses innermost-first (call-by-value) rewriting: subterms are normalized
/// before attempting to match the outer term against rule left-hand sides.
/// Continues until no more rules apply (fixed point) or `max_steps` rewrites
/// have been performed.
#[must_use]
pub fn normalize(term: &Term, directed_eqs: &[DirectedEquation], max_steps: usize) -> Term {
    normalize_with_status(term, directed_eqs, max_steps).0
}

/// Normalize a term, also reporting whether the step budget was exhausted.
///
/// The boolean is `true` when normalization stopped because it reached
/// `max_steps` while the term was still reducible, meaning the returned term
/// may not be a normal form. It is `false` when a fixed point was reached.
/// Callers that decide equality by normalization use this to distinguish a
/// genuine inequality from an inconclusive, budget-truncated comparison.
#[must_use]
pub fn normalize_with_status(
    term: &Term,
    directed_eqs: &[DirectedEquation],
    max_steps: usize,
) -> (Term, bool) {
    let mut steps = 0;
    normalize_once(term, directed_eqs, &mut steps, max_steps)
}

/// Fully normalize `term`, returning the result and whether the step budget
/// was exhausted (the result may then still be reducible).
///
/// Root rewrites and `let`/`case` contractions loop back rather than recurse,
/// so stack depth stays bounded by the term's structure even when a rule set
/// does not terminate within the budget. Genuine subterm normalization is still
/// recursive, bounded by the term's depth.
fn normalize_once(
    term: &Term,
    directed_eqs: &[DirectedEquation],
    steps: &mut usize,
    max_steps: usize,
) -> (Term, bool) {
    // Outcome of one innermost-normalization pass over the current term: either
    // a subterm-normalized term to try root rules on, or a term to re-process
    // directly (a `let`/`case` contraction).
    enum Step {
        Root(Term),
        Jump(Term),
    }

    let mut current = term.clone();
    loop {
        if *steps >= max_steps {
            return (current, true);
        }

        // Innermost first: normalize subterms before trying the root. Subterm
        // exhaustion drives `*steps` to the budget, which is re-checked below.
        let step = match &current {
            Term::Var(_) | Term::Hole { .. } => Step::Root(current.clone()),
            Term::Let { name, bound, body } => {
                let (new_bound, _) = normalize_once(bound, directed_eqs, steps, max_steps);
                let mut subst = rustc_hash::FxHashMap::default();
                subst.insert(Arc::clone(name), new_bound);
                Step::Jump(body.substitute(&subst))
            }
            Term::App { op, args } => {
                let new_args: Vec<Term> = args
                    .iter()
                    .map(|a| normalize_once(a, directed_eqs, steps, max_steps).0)
                    .collect();
                Step::Root(Term::App {
                    op: Arc::clone(op),
                    args: new_args,
                })
            }
            Term::Case {
                scrutinee,
                branches,
            } => {
                let new_scrut =
                    Box::new(normalize_once(scrutinee, directed_eqs, steps, max_steps).0);
                // If the normalized scrutinee is a fully-applied constructor
                // matching one of the branches, contract the case to that
                // branch with its binders substituted by the constructor's
                // argument terms.
                let contracted = if let Term::App { op, args } = new_scrut.as_ref() {
                    branches
                        .iter()
                        .find(|b| &b.constructor == op)
                        .filter(|b| b.binders.len() == args.len())
                        .map(|branch| {
                            let mut subst = rustc_hash::FxHashMap::default();
                            for (binder, arg) in branch.binders.iter().zip(args.iter()) {
                                subst.insert(Arc::clone(binder), arg.clone());
                            }
                            branch.body.substitute(&subst)
                        })
                } else {
                    None
                };
                contracted.map_or_else(
                    || {
                        let new_branches = branches
                            .iter()
                            .map(|b| CaseBranch {
                                constructor: Arc::clone(&b.constructor),
                                binders: b.binders.clone(),
                                body: normalize_once(&b.body, directed_eqs, steps, max_steps).0,
                            })
                            .collect();
                        Step::Root(Term::Case {
                            scrutinee: new_scrut,
                            branches: new_branches,
                        })
                    },
                    Step::Jump,
                )
            }
        };

        let normalized_subterms = match step {
            Step::Jump(next) => {
                current = next;
                continue;
            }
            Step::Root(t) => t,
        };

        // Try each directed equation at the root; a match loops back so a new
        // redex exposed by the rewrite is normalized on the next iteration.
        let mut rewritten = None;
        for de in directed_eqs {
            if let Some(subst) = match_pattern(&de.lhs, &normalized_subterms) {
                *steps += 1;
                rewritten = Some(de.rhs.substitute(&subst));
                break;
            }
        }
        // No rule applies: a fixed point, unless a subterm already spent the
        // whole budget (in which case the result may still be reducible).
        let Some(next) = rewritten else {
            return (normalized_subterms, *steps >= max_steps);
        };
        current = next;
    }
}

/// Normalize a term, returning its normal form together with an
/// [`EqWitness`](crate::witness::EqWitness) proving the input equals it.
///
/// The witness mirrors the rewrite: each applied directed equation contributes
/// an `Axiom` witness at its redex, lifted to the surrounding term by
/// `Congruence` and chained across steps by `Transitivity`; a term already in
/// normal form yields `Reflexivity`. `let`/`case` contractions are beta steps
/// rather than directed-equation rewrites, so they are justified by a trusted
/// `RuntimeChecked` witness carrying the correct endpoints. The returned term
/// matches [`normalize`] exactly.
#[must_use]
pub fn normalize_with_witness(
    term: &Term,
    directed_eqs: &[DirectedEquation],
    max_steps: usize,
) -> (Term, crate::witness::EqWitness) {
    let mut steps = 0;
    norm_once_witness(term, directed_eqs, &mut steps, max_steps)
}

/// A trusted witness for a step that is not a directed-equation rewrite (a
/// `let`/`case` contraction). Reflexivity when the endpoints coincide.
fn runtime_witness(lhs: Term, rhs: Term) -> crate::witness::EqWitness {
    use crate::witness::{EqWitness, WitnessJustification};
    if lhs == rhs {
        EqWitness::reflexivity(lhs)
    } else {
        EqWitness {
            lhs,
            rhs,
            justification: WitnessJustification::RuntimeChecked {
                description: "let/case contraction".to_owned(),
            },
        }
    }
}

/// Witnessed counterpart of [`normalize_once`]: returns the normalized term and
/// a witness proving `term` equals it.
fn norm_once_witness(
    term: &Term,
    directed_eqs: &[DirectedEquation],
    steps: &mut usize,
    max_steps: usize,
) -> (Term, crate::witness::EqWitness) {
    use crate::witness::EqWitness;

    if *steps >= max_steps {
        return (term.clone(), EqWitness::reflexivity(term.clone()));
    }

    // Innermost first: normalize subterms, building a witness `term = subnormal`.
    let (subnormal, sub_witness): (Term, EqWitness) = match term {
        Term::Var(_) | Term::Hole { .. } => (term.clone(), EqWitness::reflexivity(term.clone())),
        Term::App { op, args } => {
            let mut new_args = Vec::with_capacity(args.len());
            let mut arg_witnesses = Vec::with_capacity(args.len());
            for a in args {
                let (na, wa) = norm_once_witness(a, directed_eqs, steps, max_steps);
                new_args.push(na);
                arg_witnesses.push(wa);
            }
            let witness = EqWitness::congruence(Arc::clone(op), arg_witnesses);
            (
                Term::App {
                    op: Arc::clone(op),
                    args: new_args,
                },
                witness,
            )
        }
        Term::Let { name, bound, body } => {
            let (new_bound, _) = norm_once_witness(bound, directed_eqs, steps, max_steps);
            let mut subst = rustc_hash::FxHashMap::default();
            subst.insert(Arc::clone(name), new_bound);
            let substituted = body.substitute(&subst);
            let (result, _) = norm_once_witness(&substituted, directed_eqs, steps, max_steps);
            let witness = runtime_witness(term.clone(), result.clone());
            return (result, witness);
        }
        Term::Case {
            scrutinee,
            branches,
        } => {
            let (new_scrut, _) = norm_once_witness(scrutinee, directed_eqs, steps, max_steps);
            if let Term::App { op, args } = &new_scrut {
                if let Some(branch) = branches.iter().find(|b| &b.constructor == op) {
                    if branch.binders.len() == args.len() {
                        let mut subst = rustc_hash::FxHashMap::default();
                        for (binder, arg) in branch.binders.iter().zip(args.iter()) {
                            subst.insert(Arc::clone(binder), arg.clone());
                        }
                        let body = branch.body.substitute(&subst);
                        let (result, _) = norm_once_witness(&body, directed_eqs, steps, max_steps);
                        let witness = runtime_witness(term.clone(), result.clone());
                        return (result, witness);
                    }
                }
            }
            let new_branches = branches
                .iter()
                .map(|b| {
                    let (body, _) = norm_once_witness(&b.body, directed_eqs, steps, max_steps);
                    CaseBranch {
                        constructor: Arc::clone(&b.constructor),
                        binders: b.binders.clone(),
                        body,
                    }
                })
                .collect();
            let result = Term::Case {
                scrutinee: Box::new(new_scrut),
                branches: new_branches,
            };
            let witness = runtime_witness(term.clone(), result.clone());
            return (result, witness);
        }
    };

    // Try each directed equation at the root of the subnormalized term.
    for de in directed_eqs {
        if let Some(subst) = match_pattern(&de.lhs, &subnormal) {
            *steps += 1;
            let rewritten = de.rhs.substitute(&subst);
            let axiom =
                EqWitness::axiom(Arc::clone(&de.name), subnormal.clone(), rewritten.clone());
            let (final_term, rest_witness) =
                norm_once_witness(&rewritten, directed_eqs, steps, max_steps);
            // term =(sub) subnormal =(axiom) rewritten =(rest) final_term.
            let step_and_rest = EqWitness::transitivity(axiom, rest_witness);
            let full = EqWitness::transitivity(sub_witness, step_and_rest);
            return (final_term, full);
        }
    }

    (subnormal, sub_witness)
}

impl DirectedEquation {
    /// Create a new directed equation with no inverse (Opaque coercion class).
    #[must_use]
    pub fn new(
        name: impl Into<Arc<str>>,
        lhs: Term,
        rhs: Term,
        impl_term: panproto_expr::Expr,
    ) -> Self {
        Self {
            name: name.into(),
            lhs,
            rhs,
            impl_term,
            inverse: None,
            source_kind: None,
            target_kind: None,
            coercion_class: crate::sort::CoercionClass::Opaque,
        }
    }

    /// Create a directed equation with an inverse (Retraction coercion class).
    #[must_use]
    pub fn with_inverse(
        name: impl Into<Arc<str>>,
        lhs: Term,
        rhs: Term,
        impl_term: panproto_expr::Expr,
        inverse: panproto_expr::Expr,
    ) -> Self {
        Self {
            name: name.into(),
            lhs,
            rhs,
            impl_term,
            inverse: Some(inverse),
            source_kind: None,
            target_kind: None,
            coercion_class: crate::sort::CoercionClass::Retraction,
        }
    }

    /// Set the value kind annotations and coercion class on this directed equation.
    #[must_use]
    pub const fn with_kinds(
        mut self,
        source: crate::sort::ValueKind,
        target: crate::sort::ValueKind,
        class: crate::sort::CoercionClass,
    ) -> Self {
        self.source_kind = Some(source);
        self.target_kind = Some(target);
        self.coercion_class = class;
        self
    }

    /// Apply an operation renaming to both sides of this directed equation.
    #[must_use]
    pub fn rename_ops(&self, op_map: &std::collections::HashMap<Arc<str>, Arc<str>>) -> Self {
        Self {
            name: Arc::clone(&self.name),
            lhs: self.lhs.rename_ops(op_map),
            rhs: self.rhs.rename_ops(op_map),
            impl_term: self.impl_term.clone(),
            inverse: self.inverse.clone(),
            source_kind: self.source_kind,
            target_kind: self.target_kind,
            coercion_class: self.coercion_class,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn term_substitution() {
        // add(x, zero()) with x → y becomes add(y, zero())
        let term = Term::app("add", vec![Term::var("x"), Term::constant("zero")]);
        let mut subst = rustc_hash::FxHashMap::default();
        subst.insert(Arc::from("x"), Term::var("y"));
        let result = term.substitute(&subst);
        assert_eq!(
            result,
            Term::app("add", vec![Term::var("y"), Term::constant("zero")])
        );
    }

    #[test]
    fn free_variables() {
        let term = Term::app("f", vec![Term::var("x"), Term::var("y")]);
        let vars = term.free_vars();
        assert!(vars.contains("x"));
        assert!(vars.contains("y"));
        assert_eq!(vars.len(), 2);
    }

    // --- α-equivalence tests ---

    #[test]
    fn alpha_eq_same_vars() {
        let t1 = Term::app("f", vec![Term::var("x"), Term::var("y")]);
        let t2 = Term::app("f", vec![Term::var("x"), Term::var("y")]);
        assert!(alpha_equivalent(&t1, &t2));
    }

    #[test]
    fn alpha_eq_renamed_vars() {
        let t1 = Term::app("f", vec![Term::var("x"), Term::var("y")]);
        let t2 = Term::app("f", vec![Term::var("a"), Term::var("b")]);
        assert!(alpha_equivalent(&t1, &t2));
    }

    #[test]
    fn alpha_eq_non_injective_rejected() {
        // f(x, x) is NOT α-equivalent to f(a, b) because x must map to
        // a single variable, but a ≠ b.
        let t1 = Term::app("f", vec![Term::var("x"), Term::var("x")]);
        let t2 = Term::app("f", vec![Term::var("a"), Term::var("b")]);
        assert!(!alpha_equivalent(&t1, &t2));
    }

    #[test]
    fn alpha_eq_non_surjective_rejected() {
        // f(a, b) is NOT α-equivalent to f(x, x) because the backward
        // bijection would require both a and b to map to x.
        let t1 = Term::app("f", vec![Term::var("a"), Term::var("b")]);
        let t2 = Term::app("f", vec![Term::var("x"), Term::var("x")]);
        assert!(!alpha_equivalent(&t1, &t2));
    }

    #[test]
    fn alpha_eq_different_ops() {
        let t1 = Term::app("f", vec![Term::var("x")]);
        let t2 = Term::app("g", vec![Term::var("x")]);
        assert!(!alpha_equivalent(&t1, &t2));
    }

    #[test]
    fn alpha_eq_different_structure() {
        let t1 = Term::app(
            "f",
            vec![Term::var("x"), Term::app("g", vec![Term::var("y")])],
        );
        let t2 = Term::app("f", vec![Term::var("x"), Term::var("y")]);
        assert!(!alpha_equivalent(&t1, &t2));
    }

    #[test]
    fn alpha_eq_constants() {
        let t1 = Term::app("f", vec![Term::constant("c")]);
        let t2 = Term::app("f", vec![Term::constant("c")]);
        assert!(alpha_equivalent(&t1, &t2));
    }

    #[test]
    fn alpha_eq_constants_differ() {
        let t1 = Term::app("f", vec![Term::constant("c")]);
        let t2 = Term::app("f", vec![Term::constant("d")]);
        assert!(!alpha_equivalent(&t1, &t2));
    }

    #[test]
    fn alpha_eq_nested_renamed() {
        // f(g(x, y), h(y, x)) ≡α f(g(a, b), h(b, a))
        let t1 = Term::app(
            "f",
            vec![
                Term::app("g", vec![Term::var("x"), Term::var("y")]),
                Term::app("h", vec![Term::var("y"), Term::var("x")]),
            ],
        );
        let t2 = Term::app(
            "f",
            vec![
                Term::app("g", vec![Term::var("a"), Term::var("b")]),
                Term::app("h", vec![Term::var("b"), Term::var("a")]),
            ],
        );
        assert!(alpha_equivalent(&t1, &t2));
    }

    #[test]
    fn alpha_eq_equation_shared_bijection() {
        // Equation: f(x, y) = g(y, x) with vars renamed to a, b.
        // The bijection must be consistent across both sides.
        let lhs1 = Term::app("f", vec![Term::var("x"), Term::var("y")]);
        let rhs1 = Term::app("g", vec![Term::var("y"), Term::var("x")]);
        let lhs2 = Term::app("f", vec![Term::var("a"), Term::var("b")]);
        let rhs2 = Term::app("g", vec![Term::var("b"), Term::var("a")]);
        assert!(alpha_equivalent_equation(&lhs1, &rhs1, &lhs2, &rhs2));
    }

    #[test]
    fn alpha_eq_equation_inconsistent_bijection() {
        // Equation 1: f(x, y) = g(y)
        // Equation 2: f(a, b) = g(a)  -- inconsistent: y->b from lhs, but y->a from rhs
        let lhs1 = Term::app("f", vec![Term::var("x"), Term::var("y")]);
        let rhs1 = Term::app("g", vec![Term::var("y")]);
        let lhs2 = Term::app("f", vec![Term::var("a"), Term::var("b")]);
        let rhs2 = Term::app("g", vec![Term::var("a")]);
        assert!(!alpha_equivalent_equation(&lhs1, &rhs1, &lhs2, &rhs2));
    }

    // --- pattern matching tests ---

    #[test]
    fn match_pattern_var_binds() {
        let pat = Term::var("x");
        let term = Term::app("f", vec![Term::constant("a")]);
        let result = match_pattern(&pat, &term);
        assert!(result.is_some());
        let subst = result.unwrap();
        assert_eq!(subst.get(&Arc::from("x")).unwrap(), &term);
    }

    #[test]
    fn match_pattern_op_matches() {
        let pat = Term::app("f", vec![Term::var("x"), Term::var("y")]);
        let term = Term::app("f", vec![Term::constant("a"), Term::constant("b")]);
        let result = match_pattern(&pat, &term);
        assert!(result.is_some());
        let subst = result.unwrap();
        assert_eq!(subst.get(&Arc::from("x")).unwrap(), &Term::constant("a"));
        assert_eq!(subst.get(&Arc::from("y")).unwrap(), &Term::constant("b"));
    }

    #[test]
    fn match_pattern_op_mismatch() {
        let pat = Term::app("f", vec![Term::var("x")]);
        let term = Term::app("g", vec![Term::constant("a")]);
        assert!(match_pattern(&pat, &term).is_none());
    }

    #[test]
    fn match_pattern_repeated_var_consistent() {
        let pat = Term::app("f", vec![Term::var("x"), Term::var("x")]);
        let term = Term::app("f", vec![Term::constant("a"), Term::constant("a")]);
        assert!(match_pattern(&pat, &term).is_some());
    }

    #[test]
    fn match_pattern_repeated_var_inconsistent() {
        let pat = Term::app("f", vec![Term::var("x"), Term::var("x")]);
        let term = Term::app("f", vec![Term::constant("a"), Term::constant("b")]);
        assert!(match_pattern(&pat, &term).is_none());
    }

    // --- normalization tests ---

    fn make_directed_eq(name: &str, lhs: Term, rhs: Term) -> DirectedEquation {
        DirectedEquation::new(name, lhs, rhs, panproto_expr::Expr::Var("_".into()))
    }

    #[test]
    fn normalize_no_rules() {
        let term = Term::app("f", vec![Term::var("x")]);
        let result = normalize(&term, &[], 100);
        assert_eq!(result, term);
    }

    #[test]
    fn normalize_simple_rewrite() {
        // Rule: add(zero(), y) -> y
        let rule = make_directed_eq(
            "left_id",
            Term::app("add", vec![Term::constant("zero"), Term::var("y")]),
            Term::var("y"),
        );
        let term = Term::app("add", vec![Term::constant("zero"), Term::var("x")]);
        let result = normalize(&term, &[rule], 100);
        assert_eq!(result, Term::var("x"));
    }

    #[test]
    fn normalize_nested() {
        // Rule: add(zero(), y) -> y
        let rule = make_directed_eq(
            "left_id",
            Term::app("add", vec![Term::constant("zero"), Term::var("y")]),
            Term::var("y"),
        );
        // f(add(zero(), x)) should normalize to f(x)
        let term = Term::app(
            "f",
            vec![Term::app(
                "add",
                vec![Term::constant("zero"), Term::var("x")],
            )],
        );
        let result = normalize(&term, &[rule], 100);
        assert_eq!(result, Term::app("f", vec![Term::var("x")]));
    }

    #[test]
    fn normalize_multi_step() {
        // Rule: add(zero(), y) -> y
        let rule = make_directed_eq(
            "left_id",
            Term::app("add", vec![Term::constant("zero"), Term::var("y")]),
            Term::var("y"),
        );
        // add(zero(), add(zero(), x)) -> add(zero(), x) -> x
        let term = Term::app(
            "add",
            vec![
                Term::constant("zero"),
                Term::app("add", vec![Term::constant("zero"), Term::var("x")]),
            ],
        );
        let result = normalize(&term, &[rule], 100);
        assert_eq!(result, Term::var("x"));
    }

    #[test]
    fn normalize_max_steps_guard() {
        // Rule: f(x) -> f(f(x)) -- non-terminating
        let rule = make_directed_eq(
            "expand",
            Term::app("f", vec![Term::var("x")]),
            Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
        );
        let term = Term::app("f", vec![Term::constant("a")]);
        // Should not panic or loop; just return after max_steps.
        let result = normalize(&term, &[rule], 5);
        // The result will be some deeply nested f(...) but should terminate.
        assert!(matches!(result, Term::App { .. }));
    }

    #[test]
    fn witness_normalization_multi_step_verifies() {
        // Rules: add(zero, y) -> y ; f(x) -> g(x)
        let rules = vec![
            make_directed_eq(
                "left_id",
                Term::app("add", vec![Term::constant("zero"), Term::var("y")]),
                Term::var("y"),
            ),
            make_directed_eq(
                "f_to_g",
                Term::app("f", vec![Term::var("x")]),
                Term::app("g", vec![Term::var("x")]),
            ),
        ];
        // f(add(zero, a)) -> f(a) -> g(a)
        let term = Term::app(
            "f",
            vec![Term::app(
                "add",
                vec![Term::constant("zero"), Term::constant("a")],
            )],
        );
        let (result, witness) = normalize_with_witness(&term, &rules, 100);
        assert_eq!(result, Term::app("g", vec![Term::constant("a")]));
        assert_eq!(result, normalize(&term, &rules, 100));
        assert_eq!(witness.lhs, term);
        assert_eq!(witness.rhs, result);
        // The proof genuinely chains rewrite steps (not a single trusted node).
        assert!(witness.depth() > 1);
        let theory =
            crate::theory::Theory::full("T", vec![], vec![], vec![], vec![], rules, vec![]);
        assert!(witness.verify(&theory).is_ok());
    }

    #[test]
    fn normalize_with_status_reports_exhaustion() {
        // Non-terminating rule: f(x) -> f(f(x)).
        let rule = make_directed_eq(
            "expand",
            Term::app("f", vec![Term::var("x")]),
            Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
        );
        let term = Term::app("f", vec![Term::constant("a")]);
        let (_, exhausted) = normalize_with_status(&term, &[rule], 5);
        assert!(
            exhausted,
            "a looping rule set must report budget exhaustion"
        );
    }

    #[test]
    fn normalize_with_status_terminating_is_not_exhausted() {
        let rule = make_directed_eq(
            "left_id",
            Term::app("add", vec![Term::constant("zero"), Term::var("y")]),
            Term::var("y"),
        );
        let term = Term::app("add", vec![Term::constant("zero"), Term::var("x")]);
        let (result, exhausted) = normalize_with_status(&term, &[rule], 100);
        assert_eq!(result, Term::var("x"));
        assert!(!exhausted);
    }

    #[test]
    fn witness_normalization_no_rules_is_reflexive() {
        let term = Term::app("f", vec![Term::var("x")]);
        let (result, witness) = normalize_with_witness(&term, &[], 100);
        assert_eq!(result, term);
        assert_eq!(witness.lhs, term);
        assert_eq!(witness.rhs, term);
        let theory =
            crate::theory::Theory::full("T", vec![], vec![], vec![], vec![], vec![], vec![]);
        assert!(witness.verify(&theory).is_ok());
    }

    #[test]
    fn alpha_eq_var_vs_app() {
        let t1 = Term::var("x");
        let t2 = Term::constant("c");
        assert!(!alpha_equivalent(&t1, &t2));
    }

    #[test]
    fn alpha_eq_arity_mismatch() {
        let t1 = Term::app("f", vec![Term::var("x")]);
        let t2 = Term::app("f", vec![Term::var("x"), Term::var("y")]);
        assert!(!alpha_equivalent(&t1, &t2));
    }

    // --- compose_subst and substitution monoid laws ---

    fn mk_subst(pairs: &[(&str, Term)]) -> rustc_hash::FxHashMap<Arc<str>, Term> {
        let mut m = rustc_hash::FxHashMap::default();
        for (k, v) in pairs {
            m.insert(Arc::from(*k), v.clone());
        }
        m
    }

    #[test]
    fn compose_subst_agrees_with_sequential_application() {
        // t = f(x, y); sigma = [x := g(a)]; tau = [y := b, a := c]
        let t = Term::app("f", vec![Term::var("x"), Term::var("y")]);
        let sigma = mk_subst(&[("x", Term::app("g", vec![Term::var("a")]))]);
        let tau = mk_subst(&[("y", Term::constant("b")), ("a", Term::constant("c"))]);
        let sequential = t.substitute(&sigma).substitute(&tau);
        let composed = t.substitute(&compose_subst(&tau, &sigma));
        assert_eq!(sequential, composed);
    }

    #[test]
    fn substitute_empty_is_identity_unit() {
        let t = Term::app("f", vec![Term::var("x"), Term::constant("c")]);
        let empty = rustc_hash::FxHashMap::default();
        assert_eq!(t.substitute(&empty), t);
    }

    #[test]
    fn compose_subst_empty_left_is_right() {
        let sigma = mk_subst(&[("x", Term::var("y"))]);
        let empty = rustc_hash::FxHashMap::default();
        let composed = compose_subst(&empty, &sigma);
        // compose_subst(empty, sigma)(x) = sigma(x).subst(empty) = sigma(x)
        assert_eq!(composed.get(&Arc::from("x")).unwrap(), &Term::var("y"));
        assert_eq!(composed.len(), 1);
    }

    #[test]
    fn compose_subst_empty_right_is_left() {
        let tau = mk_subst(&[("y", Term::var("z"))]);
        let empty = rustc_hash::FxHashMap::default();
        let composed = compose_subst(&tau, &empty);
        assert_eq!(composed.get(&Arc::from("y")).unwrap(), &Term::var("z"));
        assert_eq!(composed.len(), 1);
    }

    // --- alpha-equivalence transitivity ---

    #[test]
    fn alpha_equivalence_transitive() {
        let a = Term::app("f", vec![Term::var("x"), Term::var("y")]);
        let b = Term::app("f", vec![Term::var("u"), Term::var("v")]);
        let c = Term::app("f", vec![Term::var("p"), Term::var("q")]);
        assert!(alpha_equivalent(&a, &b));
        assert!(alpha_equivalent(&b, &c));
        assert!(alpha_equivalent(&a, &c));
    }

    // --- proptest strategies and property tests ---

    mod property {
        use super::*;
        use proptest::prelude::*;

        const VAR_NAMES: &[&str] = &["x", "y", "z", "a", "b"];
        const OP_NAMES: &[&str] = &["f", "g", "h", "add", "mul"];

        fn arb_name() -> impl Strategy<Value = Arc<str>> {
            prop::sample::select(VAR_NAMES).prop_map(Arc::from)
        }

        fn arb_term(max_depth: usize) -> BoxedStrategy<Term> {
            if max_depth == 0 {
                arb_name().prop_map(Term::Var).boxed()
            } else {
                let leaf = arb_name().prop_map(Term::Var);
                let app = (
                    prop::sample::select(OP_NAMES).prop_map(Arc::from),
                    prop::collection::vec(arb_term(max_depth - 1), 0..=3),
                )
                    .prop_map(|(op, args)| Term::App { op, args });
                prop_oneof![leaf, app].boxed()
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn alpha_equivalence_is_reflexive(t in arb_term(3)) {
                prop_assert!(alpha_equivalent(&t, &t));
            }

            #[test]
            fn alpha_equivalence_is_symmetric(a in arb_term(2), b in arb_term(2)) {
                prop_assert_eq!(
                    alpha_equivalent(&a, &b),
                    alpha_equivalent(&b, &a),
                );
            }

            #[test]
            fn substitute_empty_is_identity(t in arb_term(3)) {
                let empty = rustc_hash::FxHashMap::default();
                prop_assert_eq!(t.substitute(&empty), t);
            }

            #[test]
            fn witness_normalization_verifies_and_matches(t in arb_term(3)) {
                // A terminating rule set drawn from the generator's signature.
                let rules = vec![
                    make_directed_eq(
                        "f_to_g",
                        Term::app("f", vec![Term::var("x")]),
                        Term::app("g", vec![Term::var("x")]),
                    ),
                    make_directed_eq(
                        "add_l",
                        Term::app("add", vec![Term::var("x"), Term::var("y")]),
                        Term::var("x"),
                    ),
                ];
                let (result, witness) = normalize_with_witness(&t, &rules, 200);
                // Endpoints match the input and the normal form.
                prop_assert_eq!(&witness.lhs, &t);
                prop_assert_eq!(&witness.rhs, &result);
                // The witnessed normal form equals the plain normalizer's.
                prop_assert_eq!(&result, &normalize(&t, &rules, 200));
                // And the witness verifies against the theory of those rules.
                let theory = crate::theory::Theory::full(
                    "T", vec![], vec![], vec![], vec![], rules, vec![],
                );
                prop_assert!(witness.verify(&theory).is_ok());
            }

            #[test]
            fn rename_ops_empty_is_identity(t in arb_term(3)) {
                let empty = std::collections::HashMap::new();
                prop_assert_eq!(t.rename_ops(&empty), t);
            }

            #[test]
            fn rename_ops_preserves_alpha_structure(
                t in arb_term(2),
                src in prop::sample::select(OP_NAMES),
                tgt in prop::sample::select(OP_NAMES),
            ) {
                let mut map = std::collections::HashMap::new();
                map.insert(Arc::from(src), Arc::from(tgt));
                let renamed = t.rename_ops(&map);
                // renaming must preserve the number of free variables
                prop_assert_eq!(t.free_vars().len(), renamed.free_vars().len());
            }

            #[test]
            fn substitute_composition_law(
                t in arb_term(3),
                v1 in arb_name(),
                r1 in arb_term(1),
                v2 in arb_name(),
                r2 in arb_term(1),
            ) {
                let sigma = {
                    let mut m = rustc_hash::FxHashMap::default();
                    m.insert(v1, r1);
                    m
                };
                let tau = {
                    let mut m = rustc_hash::FxHashMap::default();
                    m.insert(v2, r2);
                    m
                };
                let sequential = t.substitute(&sigma).substitute(&tau);
                let composed = t.substitute(&compose_subst(&tau, &sigma));
                prop_assert_eq!(sequential, composed);
            }

            #[test]
            fn alpha_equivalence_transitive_prop(
                t in arb_term(3),
            ) {
                // reflexivity + transitivity via chain t ~ t ~ t.
                prop_assert!(alpha_equivalent(&t, &t));
            }

            #[test]
            fn let_substitute_does_not_capture(
                _dummy in arb_name(),
            ) {
                // let x = a in f(x, y): substitute y := g(x).
                // Naive substitution would capture the x bound by the
                // let; capture-avoiding substitution renames the let's
                // binder.
                let body = Term::app("f", vec![Term::Var(Arc::from("x")), Term::Var(Arc::from("y"))]);
                let t = Term::Let {
                    name: Arc::from("x"),
                    bound: Box::new(Term::constant("a")),
                    body: Box::new(body),
                };
                let mut subst = rustc_hash::FxHashMap::default();
                subst.insert(
                    Arc::from("y"),
                    Term::app("g", vec![Term::Var(Arc::from("x"))]),
                );
                let result = t.substitute(&subst);
                if let Term::Let { name, body, .. } = result {
                    // The outer free `x` (inside g(x)) must not be
                    // captured by the let binder: the binder must be
                    // alpha-renamed away from `x`.
                    prop_assert_ne!(&*name, "x");
                    if let Term::App { args, .. } = *body {
                        let is_g = matches!(&args[1], Term::App { op, .. } if &**op == "g");
                        prop_assert!(is_g);
                    } else {
                        prop_assert!(false, "expected App body");
                    }
                } else {
                    prop_assert!(false, "expected Let result");
                }
            }

            #[test]
            fn free_vars_subset_after_substitution(
                t in arb_term(2),
                var in arb_name(),
                replacement in arb_term(1),
            ) {
                let mut subst = rustc_hash::FxHashMap::default();
                subst.insert(var.clone(), replacement.clone());
                let result = t.substitute(&subst);
                let result_vars = result.free_vars();
                // every var in result is either from the original (minus substituted)
                // or from the replacement
                let orig_vars = t.free_vars();
                let repl_vars = replacement.free_vars();
                for v in &result_vars {
                    prop_assert!(
                        (orig_vars.contains(v) && *v != var) || repl_vars.contains(v),
                        "unexpected var {:?} in result",
                        v,
                    );
                }
            }
        }
    }

    // --- capture avoidance under case-branch binders ---

    fn case_of(scrutinee: Term, constructor: &str, binders: &[&str], body: Term) -> Term {
        Term::Case {
            scrutinee: Box::new(scrutinee),
            branches: vec![CaseBranch {
                constructor: Arc::from(constructor),
                binders: binders.iter().map(|b| Arc::from(*b)).collect(),
                body,
            }],
        }
    }

    #[test]
    fn substitute_avoids_capture_under_case_binder() {
        // (case s of c(y) => pair(x, y))[x := y]: the incoming y must stay
        // free, so the branch binder is alpha-renamed.
        let term = case_of(
            Term::var("s"),
            "c",
            &["y"],
            Term::app("pair", vec![Term::var("x"), Term::var("y")]),
        );
        let mut subst = rustc_hash::FxHashMap::default();
        subst.insert(Arc::from("x"), Term::var("y"));
        let result = term.substitute(&subst);
        let free = result.free_vars();
        assert!(
            free.contains("y"),
            "substituted y must remain free, got {free:?} from {result:?}"
        );
        assert!(free.contains("s"), "scrutinee stays free, got {free:?}");
    }

    #[test]
    fn substitute_preserves_alpha_equivalence_under_case_binder() {
        // Two alpha-equivalent terms differing only in a branch binder must
        // stay alpha-equivalent after the same substitution.
        let with_y = case_of(
            Term::var("s"),
            "c",
            &["y"],
            Term::app("pair", vec![Term::var("x"), Term::var("y")]),
        );
        let with_z = case_of(
            Term::var("s"),
            "c",
            &["z"],
            Term::app("pair", vec![Term::var("x"), Term::var("z")]),
        );
        assert!(alpha_equivalent(&with_y, &with_z), "inputs are alpha-equal");
        let mut subst = rustc_hash::FxHashMap::default();
        subst.insert(Arc::from("x"), Term::var("y"));
        let out_y = with_y.substitute(&subst);
        let out_z = with_z.substitute(&subst);
        assert!(
            alpha_equivalent(&out_y, &out_z),
            "substitution must respect alpha-equivalence: {out_y:?} vs {out_z:?}"
        );
    }

    #[test]
    fn normalize_case_contraction_respects_alpha_equivalence() {
        // case c(v) of c(w) => case s of d(BINDER) => pair(w, BINDER)
        // Contracting the outer case substitutes w := v into the inner case,
        // whose binder must not capture v.
        let build = |inner_binder: &str| {
            case_of(
                Term::app("c", vec![Term::var("v")]),
                "c",
                &["w"],
                case_of(
                    Term::var("s"),
                    "d",
                    &[inner_binder],
                    Term::app("pair", vec![Term::var("w"), Term::var(inner_binder)]),
                ),
            )
        };
        let shadowing = build("v");
        let distinct = build("u");
        assert!(
            alpha_equivalent(&shadowing, &distinct),
            "inputs are alpha-equal"
        );
        let nf_shadowing = normalize(&shadowing, &[], 100);
        let nf_distinct = normalize(&distinct, &[], 100);
        assert!(
            alpha_equivalent(&nf_shadowing, &nf_distinct),
            "normalization must respect alpha-equivalence: {nf_shadowing:?} vs {nf_distinct:?}"
        );
    }
}
