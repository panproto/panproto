//! Rewrite-system properties: confluence and termination.
//!
//! Library-level checks for a theory's [`DirectedEquation`] set. Both
//! functions run offline (not from `typecheck_theory`) and report the
//! offending rules so the author can repair them.

use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::eq::{DirectedEquation, Term, normalize};
use crate::error::GatError;
use crate::theory::Theory;

/// Report produced by [`check_local_confluence`].
#[derive(Debug, Clone)]
pub struct ConfluenceReport {
    /// Every critical pair discovered between pairs of rules. A pair
    /// with `joins = false` is actionable: the two reducts do not
    /// converge under the full rule set within the allotted step
    /// budget.
    pub critical_pairs: Vec<CriticalPair>,
}

/// A critical pair between two directed equations.
#[derive(Debug, Clone)]
pub struct CriticalPair {
    /// Name of the first rule (outer rewrite).
    pub rule_a: Arc<str>,
    /// Name of the second rule (inner rewrite at position `p`).
    pub rule_b: Arc<str>,
    /// Left reduct: `r_a.rhs.subst(σ)`.
    pub left: Term,
    /// Right reduct: `r_a.lhs.subst(σ)` with `r_b.rhs.subst(σ)` at `p`.
    pub right: Term,
    /// Whether the two reducts normalise to the same term under the
    /// full rule set within the step budget.
    pub joins: bool,
}

/// Check local confluence of a theory's directed equations via
/// Knuth-Bendix critical-pair analysis.
///
/// For every ordered pair of rules `(r1, r2)`, walk the non-variable
/// subterm positions of `r1.lhs`, attempt to unify each with `r2.lhs`,
/// and for each successful unifier build the two reducts. Normalise
/// both under the full rule set bounded by `normalize_depth` and
/// compare structurally.
///
/// The returned report lists every critical pair discovered, with a
/// `joins` flag. Callers that want a boolean "is locally confluent"
/// answer can check that every pair reports `joins = true`.
///
/// # Errors
///
/// This function does not currently return errors for any rule input;
/// the signature returns `Result` for future extension.
pub fn check_local_confluence(
    theory: &Theory,
    normalize_depth: usize,
) -> Result<ConfluenceReport, GatError> {
    let rules = &theory.directed_eqs;
    let mut out = Vec::new();
    for r1 in rules {
        for r2 in rules {
            collect_critical_pairs_at_positions(r1, r2, rules, normalize_depth, &mut out);
        }
    }
    Ok(ConfluenceReport {
        critical_pairs: out,
    })
}

/// Walk every non-variable subterm position of `r1.lhs` and try to
/// unify with `r2.lhs` (after alpha-renaming `r2`'s variables apart
/// from `r1`'s).
fn collect_critical_pairs_at_positions(
    r1: &DirectedEquation,
    r2: &DirectedEquation,
    rules: &[DirectedEquation],
    normalize_depth: usize,
    out: &mut Vec<CriticalPair>,
) {
    // Freshen r2 to avoid variable capture against r1.
    let r2_fresh = freshen_rule(r2, &r1.name);
    let positions = non_variable_positions(&r1.lhs);
    for pos in positions {
        let Some(subterm) = term_at_position(&r1.lhs, &pos) else {
            continue;
        };
        let Some(sigma) = first_order_unify(&subterm, &r2_fresh.lhs) else {
            continue;
        };
        // Skip the trivial self-overlap at the root of a single rule.
        if pos.is_empty() && r1.name == r2.name {
            continue;
        }
        let left = r1.rhs.substitute(&sigma);
        let right_inner = r2_fresh.rhs.substitute(&sigma);
        let right_whole = r1.lhs.substitute(&sigma);
        let Ok(right) = replace_at_position(&right_whole, &pos, right_inner) else {
            continue;
        };

        let left_nf = normalize(&left, rules, normalize_depth);
        let right_nf = normalize(&right, rules, normalize_depth);
        let joins = left_nf == right_nf;
        out.push(CriticalPair {
            rule_a: Arc::clone(&r1.name),
            rule_b: Arc::clone(&r2.name),
            left,
            right,
            joins,
        });
    }
}

/// Freshen every variable name in a rule by appending a suffix
/// derived from `other_name`, to avoid capture when unifying against
/// terms from another rule.
fn freshen_rule(r: &DirectedEquation, other_name: &Arc<str>) -> DirectedEquation {
    let vars = collect_vars(&r.lhs)
        .into_iter()
        .chain(collect_vars(&r.rhs))
        .collect::<std::collections::BTreeSet<_>>();
    let mut rename = FxHashMap::default();
    for v in vars {
        let fresh: Arc<str> = Arc::from(format!("{v}__{other_name}_cp"));
        rename.insert(v, Term::Var(fresh));
    }
    DirectedEquation {
        name: Arc::clone(&r.name),
        lhs: r.lhs.substitute(&rename),
        rhs: r.rhs.substitute(&rename),
        impl_term: r.impl_term.clone(),
        inverse: r.inverse.clone(),
        source_kind: r.source_kind,
        target_kind: r.target_kind,
        coercion_class: r.coercion_class,
    }
}

fn collect_vars_walk(term: &Term, out: &mut Vec<Arc<str>>) {
    match term {
        Term::Var(name) => {
            if !out.contains(name) {
                out.push(Arc::clone(name));
            }
        }
        Term::Hole { .. } => {}
        Term::Let { bound, body, .. } => {
            collect_vars_walk(bound, out);
            collect_vars_walk(body, out);
        }
        Term::App { args, .. } => {
            for arg in args {
                collect_vars_walk(arg, out);
            }
        }
        Term::Case {
            scrutinee,
            branches,
        } => {
            collect_vars_walk(scrutinee, out);
            for branch in branches {
                collect_vars_walk(&branch.body, out);
            }
        }
    }
}

fn collect_vars(term: &Term) -> Vec<Arc<str>> {
    let mut out = Vec::new();
    collect_vars_walk(term, &mut out);
    out
}

/// Enumerate positions (as paths of argument indices) at which the
/// subterm is an [`Term::App`]. Variables cannot unify non-trivially
/// with a rule LHS and are skipped.
fn non_variable_positions_walk(term: &Term, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
    if matches!(term, Term::App { .. }) {
        out.push(path.clone());
    }
    if let Term::App { args, .. } = term {
        for (i, arg) in args.iter().enumerate() {
            path.push(i);
            non_variable_positions_walk(arg, path, out);
            path.pop();
        }
    }
}

fn non_variable_positions(term: &Term) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    non_variable_positions_walk(term, &mut Vec::new(), &mut out);
    out
}

/// Navigate to the subterm at `pos` under `term`.
///
/// Paths encode child indices: for [`Term::App`] the index selects an
/// argument; for [`Term::Case`] index `0` is the scrutinee and indices
/// `1..=n` select branch bodies by position; for [`Term::Let`] index
/// `0` is the bound term and `1` is the body. Variables and holes have
/// no children.
///
/// Returns `None` if the path descends into a variant or child index
/// that does not exist.
fn term_at_position(term: &Term, pos: &[usize]) -> Option<Term> {
    let mut cur = term.clone();
    for &i in pos {
        match cur {
            Term::App { args, .. } => {
                cur = args.get(i).cloned()?;
            }
            Term::Case {
                scrutinee,
                branches,
            } => {
                if i == 0 {
                    cur = *scrutinee;
                } else {
                    let branch = branches.get(i - 1)?;
                    cur = branch.body.clone();
                }
            }
            Term::Let { bound, body, .. } => match i {
                0 => cur = *bound,
                1 => cur = *body,
                _ => return None,
            },
            Term::Var(_) | Term::Hole { .. } => return None,
        }
    }
    Some(cur)
}

/// Replace the subterm at `pos` under `term` with `replacement`.
///
/// Same path semantics as [`term_at_position`]. Returns
/// [`GatError::InvalidRewritePosition`] when the path descends into a
/// node variant that cannot carry such a child.
fn replace_at_position(term: &Term, pos: &[usize], replacement: Term) -> Result<Term, GatError> {
    if pos.is_empty() {
        return Ok(replacement);
    }
    let head = pos[0];
    let rest = &pos[1..];
    match term.clone() {
        Term::App { op, mut args } => {
            let slot = args
                .get(head)
                .ok_or_else(|| GatError::InvalidRewritePosition {
                    path: pos.to_vec(),
                    node_kind: "App (index out of range)",
                })?;
            let replaced = replace_at_position(slot, rest, replacement)?;
            args[head] = replaced;
            Ok(Term::App { op, args })
        }
        Term::Case {
            scrutinee,
            mut branches,
        } => {
            if head == 0 {
                let new_scrut = replace_at_position(&scrutinee, rest, replacement)?;
                Ok(Term::Case {
                    scrutinee: Box::new(new_scrut),
                    branches,
                })
            } else {
                let branch_idx = head - 1;
                let branch =
                    branches
                        .get(branch_idx)
                        .ok_or_else(|| GatError::InvalidRewritePosition {
                            path: pos.to_vec(),
                            node_kind: "Case (branch index out of range)",
                        })?;
                let new_body = replace_at_position(&branch.body, rest, replacement)?;
                branches[branch_idx] = crate::eq::CaseBranch {
                    constructor: Arc::clone(&branch.constructor),
                    binders: branch.binders.clone(),
                    body: new_body,
                };
                Ok(Term::Case {
                    scrutinee,
                    branches,
                })
            }
        }
        Term::Let { name, bound, body } => match head {
            0 => {
                let new_bound = replace_at_position(&bound, rest, replacement)?;
                Ok(Term::Let {
                    name,
                    bound: Box::new(new_bound),
                    body,
                })
            }
            1 => {
                let new_body = replace_at_position(&body, rest, replacement)?;
                Ok(Term::Let {
                    name,
                    bound,
                    body: Box::new(new_body),
                })
            }
            _ => Err(GatError::InvalidRewritePosition {
                path: pos.to_vec(),
                node_kind: "Let (only indices 0 and 1 are valid)",
            }),
        },
        Term::Var(_) => Err(GatError::InvalidRewritePosition {
            path: pos.to_vec(),
            node_kind: "Var",
        }),
        Term::Hole { .. } => Err(GatError::InvalidRewritePosition {
            path: pos.to_vec(),
            node_kind: "Hole",
        }),
    }
}

/// Try first-order unification of two terms (treating every
/// [`Term::Var`] as a unification variable). Returns Some(mgu) on
/// success, None on failure.
fn first_order_unify(left: &Term, right: &Term) -> Option<FxHashMap<Arc<str>, Term>> {
    let mut subst: FxHashMap<Arc<str>, Term> = FxHashMap::default();
    let mut eqs = vec![(left.clone(), right.clone())];
    while let Some((lhs, rhs)) = eqs.pop() {
        let lhs = apply(&lhs, &subst);
        let rhs = apply(&rhs, &subst);
        match (lhs, rhs) {
            (Term::Var(n1), Term::Var(n2)) if n1 == n2 => {}
            (Term::Var(n), term) | (term, Term::Var(n)) => {
                if occurs(&n, &term) {
                    return None;
                }
                // Apply `n := term` in-place to existing bindings, then
                // record the binding itself. Linear in the binding count
                // per step rather than rebuilding the map from scratch.
                let mut single = FxHashMap::default();
                single.insert(Arc::clone(&n), term.clone());
                for v in subst.values_mut() {
                    *v = v.substitute(&single);
                }
                subst.insert(n, term);
            }
            (Term::App { op: op_a, args: aa }, Term::App { op: op_b, args: bb }) => {
                if op_a != op_b || aa.len() != bb.len() {
                    return None;
                }
                for pair in aa.into_iter().zip(bb) {
                    eqs.push(pair);
                }
            }
            _ => return None,
        }
    }
    Some(subst)
}

fn apply(t: &Term, subst: &FxHashMap<Arc<str>, Term>) -> Term {
    if subst.is_empty() {
        t.clone()
    } else {
        t.substitute(subst)
    }
}

fn occurs(var: &Arc<str>, t: &Term) -> bool {
    match t {
        Term::Var(v) => v == var,
        Term::Hole { .. } => false,
        Term::Let { name, bound, body } => occurs(var, bound) || (name != var && occurs(var, body)),
        Term::App { args, .. } => args.iter().any(|a| occurs(var, a)),
        Term::Case {
            scrutinee,
            branches,
        } => occurs(var, scrutinee) || branches.iter().any(|b| occurs(var, &b.body)),
    }
}

/// Precedence (total order) on operation names for LPO termination
/// checks. A larger index means higher precedence.
#[derive(Debug, Clone, Default)]
pub struct OpPrecedence {
    /// Operation names in ascending precedence order.
    pub order: Vec<Arc<str>>,
}

impl OpPrecedence {
    /// Build a precedence from any iterator of name-convertible values,
    /// in ascending-precedence order.
    #[must_use]
    pub fn new<I, S>(order: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<Arc<str>>,
    {
        Self {
            order: order.into_iter().map(Into::into).collect(),
        }
    }

    /// Compare two op names under this precedence. Returns `None` if
    /// either name is missing from the precedence table.
    #[must_use]
    pub fn compare(&self, a: &Arc<str>, b: &Arc<str>) -> Option<std::cmp::Ordering> {
        let ia = self.order.iter().position(|n| n == a)?;
        let ib = self.order.iter().position(|n| n == b)?;
        Some(ia.cmp(&ib))
    }
}

/// Report produced by [`check_termination_via_lpo`].
#[derive(Debug, Clone)]
pub struct TerminationReport {
    /// Rules whose LPO comparison `lhs >_{lpo} rhs` failed.
    pub violations: Vec<RuleViolation>,
}

/// A single rule that fails LPO.
#[derive(Debug, Clone)]
pub struct RuleViolation {
    /// Rule name.
    pub rule: Arc<str>,
    /// Human-readable reason (e.g. comparison did not decrease).
    pub reason: String,
}

/// Verify termination of a theory's directed equations under a
/// user-supplied lexicographic path ordering.
///
/// For every rule `lhs -> rhs`, the check requires `lhs >_{lpo} rhs`.
/// Rules that fail the comparison are reported. LPO is a pragmatic
/// sufficient condition; it rejects some terminating rule sets that
/// do not admit an LPO. Callers who want a yes/no answer should check
/// that the returned violations list is empty.
///
/// # Errors
///
/// This function does not currently return errors for any input; the
/// signature returns `Result` for future extension.
pub fn check_termination_via_lpo(
    theory: &Theory,
    precedence: &OpPrecedence,
) -> Result<TerminationReport, GatError> {
    let mut violations = Vec::new();
    for rule in &theory.directed_eqs {
        if contains_hole(&rule.lhs) || contains_hole(&rule.rhs) {
            return Err(GatError::LpoHoleInRule {
                rule: rule.name.to_string(),
            });
        }
        if !lpo_greater(&rule.lhs, &rule.rhs, precedence) {
            violations.push(RuleViolation {
                rule: Arc::clone(&rule.name),
                reason: format!("lhs {} is not >_lpo rhs {}", rule.lhs, rule.rhs),
            });
        }
    }
    Ok(TerminationReport { violations })
}

fn contains_hole(t: &Term) -> bool {
    match t {
        Term::Hole { .. } => true,
        Term::Var(_) => false,
        Term::App { args, .. } => args.iter().any(contains_hole),
        Term::Case {
            scrutinee,
            branches,
        } => contains_hole(scrutinee) || branches.iter().any(|b| contains_hole(&b.body)),
        Term::Let { bound, body, .. } => contains_hole(bound) || contains_hole(body),
    }
}

/// Lexicographic path ordering: `s >_lpo t`.
///
/// Classical definition:
///
/// - `s >_lpo t` if `t` is a proper subterm of `s`, or
/// - `s = f(s_1, ..., s_n)` and either:
///   - `s_i >=_lpo t` for some `i`, or
///   - `t = g(t_1, ..., t_m)` with `f > g` in the precedence and
///     `s >_lpo t_j` for every `j`, or
///   - `t = f(t_1, ..., t_m)` with `s >_lpo t_j` for every `j` and
///     `(s_1, ..., s_n) >_{lex} (t_1, ..., t_m)` under `>_lpo`.
///
/// Variables cannot be rewritten by LPO; `Var >_lpo t` is always
/// false, and `f(...) >_lpo Var(x)` is true iff `x` occurs in
/// `f(...)`.
#[must_use]
pub fn lpo_greater(s: &Term, t: &Term, prec: &OpPrecedence) -> bool {
    // Variables and holes cannot be LPO-greater than anything: LPO
    // compares structural terms only.
    if matches!(s, Term::Var(_) | Term::Hole { .. }) {
        return false;
    }
    // Compute the subterm list of s: [args...] for App, [scrutinee,
    // branch bodies...] for Case, [bound, body] for Let.
    let s_subs = subterms(s);

    // Case 1: some proper subterm of s is >= t.
    for si in &s_subs {
        if si == t || lpo_greater(si, t, prec) {
            return true;
        }
    }
    match t {
        Term::Var(x) => s_subs.iter().any(|a| contains_var(a, x)),
        Term::Hole { .. } => false,
        _ => {
            // Non-leaf t: compare heads.
            let (Some(f), Some(g)) = (structural_head(s), structural_head(t)) else {
                return false;
            };
            let t_subs = subterms(t);
            match prec.compare(&f, &g) {
                Some(std::cmp::Ordering::Greater) => {
                    t_subs.iter().all(|tj| lpo_greater(s, tj, prec))
                }
                Some(std::cmp::Ordering::Equal) => {
                    if !t_subs.iter().all(|tj| lpo_greater(s, tj, prec)) {
                        return false;
                    }
                    lex_greater(&s_subs, &t_subs, prec)
                }
                _ => false,
            }
        }
    }
}

/// Return the ordered list of structural subterms of a compound term.
/// Variables and holes return an empty list.
fn subterms(t: &Term) -> Vec<Term> {
    match t {
        Term::Var(_) | Term::Hole { .. } => Vec::new(),
        Term::App { args, .. } => args.clone(),
        Term::Case {
            scrutinee,
            branches,
        } => {
            let mut v = Vec::with_capacity(branches.len() + 1);
            v.push((**scrutinee).clone());
            for b in branches {
                v.push(b.body.clone());
            }
            v
        }
        Term::Let { bound, body, .. } => vec![(**bound).clone(), (**body).clone()],
    }
}

/// Return the structural head name used for LPO precedence comparison.
/// `Case` and `Let` get synthetic pseudo-op names that only compare
/// equal to themselves; this means a user must include these names in
/// the precedence for a rule to pass LPO.
fn structural_head(t: &Term) -> Option<Arc<str>> {
    match t {
        Term::App { op, .. } => Some(Arc::clone(op)),
        Term::Case { .. } => Some(Arc::from("__case__")),
        Term::Let { .. } => Some(Arc::from("__let__")),
        Term::Var(_) | Term::Hole { .. } => None,
    }
}

fn contains_var(t: &Term, var: &Arc<str>) -> bool {
    match t {
        Term::Var(v) => v == var,
        Term::Hole { .. } => false,
        Term::Let { name, bound, body } => {
            contains_var(bound, var) || (name != var && contains_var(body, var))
        }
        Term::App { args, .. } => args.iter().any(|a| contains_var(a, var)),
        Term::Case {
            scrutinee,
            branches,
        } => contains_var(scrutinee, var) || branches.iter().any(|b| contains_var(&b.body, var)),
    }
}

fn lex_greater(a: &[Term], b: &[Term], prec: &OpPrecedence) -> bool {
    for (ai, bi) in a.iter().zip(b.iter()) {
        if ai == bi {
            continue;
        }
        return lpo_greater(ai, bi, prec);
    }
    a.len() > b.len()
}

impl ConfluenceReport {
    /// Returns `true` if every critical pair joins (i.e. local
    /// confluence holds within the step budget used to build the
    /// report).
    #[must_use]
    pub fn is_locally_confluent(&self) -> bool {
        self.critical_pairs.iter().all(|p| p.joins)
    }

    /// Returns the subset of critical pairs that do not join.
    #[must_use]
    pub fn non_joining(&self) -> Vec<&CriticalPair> {
        self.critical_pairs.iter().filter(|p| !p.joins).collect()
    }
}

impl TerminationReport {
    /// Returns `true` if every rule passed the LPO check.
    #[must_use]
    pub fn is_lpo_terminating(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Combined soundness report for a theory's directed rewrite system: local
/// confluence (Knuth-Bendix critical pairs) and LPO termination.
#[derive(Debug, Clone)]
pub struct RewriteSystemReport {
    /// Critical-pair confluence analysis.
    pub confluence: ConfluenceReport,
    /// LPO termination analysis.
    pub termination: TerminationReport,
}

impl RewriteSystemReport {
    /// Returns `true` if the rewrite system is both locally confluent and
    /// LPO-terminating within the analysis budget.
    #[must_use]
    pub fn is_sound(&self) -> bool {
        self.confluence.is_locally_confluent() && self.termination.is_lpo_terminating()
    }

    /// Human-readable descriptions of each non-joining critical pair and each
    /// LPO violation. Empty exactly when [`is_sound`](Self::is_sound) holds.
    #[must_use]
    pub fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        for pair in self.confluence.non_joining() {
            out.push(format!(
                "non-joining critical pair between rules `{}` and `{}`",
                pair.rule_a, pair.rule_b
            ));
        }
        for v in &self.termination.violations {
            out.push(format!(
                "rule `{}` is not LPO-terminating: {}",
                v.rule, v.reason
            ));
        }
        out
    }
}

/// Validate a theory's directed rewrite system for local confluence and LPO
/// termination, taking the theory's operation-declaration order as the LPO
/// precedence (earlier-declared operations rank lower).
///
/// A theory with no directed equations is trivially sound. This is the gate
/// run at theory registration and on the theory-DSL compile path: it protects
/// the soundness of the equality judgment that normalization decides.
///
/// # Errors
///
/// Propagates [`GatError::LpoHoleInRule`] if a directed equation contains a
/// typed hole, which the LPO comparison cannot order.
pub fn validate_rewrite_system(theory: &Theory) -> Result<RewriteSystemReport, GatError> {
    const CONFLUENCE_DEPTH: usize = 256;
    let confluence = check_local_confluence(theory, CONFLUENCE_DEPTH)?;
    let precedence = OpPrecedence::new(theory.ops.iter().map(|o| Arc::clone(&o.name)));
    let termination = check_termination_via_lpo(theory, &precedence)?;
    Ok(RewriteSystemReport {
        confluence,
        termination,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eq::{DirectedEquation, Term};
    use crate::op::Operation;
    use crate::sort::Sort;
    use crate::theory::Theory;
    use panproto_expr::Expr;

    fn mk_rule(name: &str, lhs: Term, rhs: Term) -> DirectedEquation {
        DirectedEquation::new(name, lhs, rhs, Expr::Var("_".into()))
    }

    fn trivial_theory(rules: Vec<DirectedEquation>) -> Theory {
        Theory::full(
            "T",
            Vec::new(),
            vec![Sort::simple("S")],
            vec![
                Operation::nullary("zero", "S"),
                Operation::unary("succ", "n", "S", "S"),
                Operation::unary("f", "x", "S", "S"),
                Operation::unary("g", "x", "S", "S"),
                Operation::unary("h", "x", "S", "S"),
                Operation::new(
                    "add",
                    vec![
                        (Arc::from("a"), crate::sort::SortExpr::from("S")),
                        (Arc::from("b"), crate::sort::SortExpr::from("S")),
                    ],
                    "S",
                ),
            ],
            Vec::new(),
            rules,
            Vec::new(),
        )
    }

    #[test]
    fn confluence_reports_trivially_joinable_pair() -> Result<(), GatError> {
        // Two rules with disjoint LHS heads: no critical pair.
        let r1 = mk_rule(
            "a",
            Term::app("f", vec![Term::var("x")]),
            Term::app("g", vec![Term::var("x")]),
        );
        let r2 = mk_rule(
            "b",
            Term::app("h", vec![Term::var("y")]),
            Term::app("f", vec![Term::var("y")]),
        );
        let theory = trivial_theory(vec![r1, r2]);
        let report = check_local_confluence(&theory, 32)?;
        // The only critical pairs come from self-overlap at the root
        // of each rule with itself (filtered), and the roots of
        // different rules where LHS heads don't unify; no non-trivial
        // pairs expected.
        for cp in &report.critical_pairs {
            assert!(cp.joins, "unexpected non-joining pair: {cp:?}");
        }
        Ok(())
    }

    #[test]
    fn confluence_reports_known_critical_pair() -> Result<(), GatError> {
        // r1: f(g(x)) -> x
        // r2: g(y) -> h(y)
        // Overlapping at position [0] of r1: the subterm `g(x)` unifies
        // with r2's LHS `g(y)` under [y := x], yielding reducts
        //   left  = x
        //   right = f(h(x))
        // These should not join (different normal forms).
        let r1 = mk_rule(
            "r1",
            Term::app("f", vec![Term::app("g", vec![Term::var("x")])]),
            Term::var("x"),
        );
        let r2 = mk_rule(
            "r2",
            Term::app("g", vec![Term::var("y")]),
            Term::app("h", vec![Term::var("y")]),
        );
        let theory = trivial_theory(vec![r1, r2]);
        let report = check_local_confluence(&theory, 32)?;
        // At least one non-joining critical pair must appear from
        // the (r1, r2) overlap.
        assert!(
            report
                .critical_pairs
                .iter()
                .any(|cp| !cp.joins && &*cp.rule_a == "r1" && &*cp.rule_b == "r2"),
            "expected non-joining critical pair between r1 and r2, got {:?}",
            report.critical_pairs
        );
        Ok(())
    }

    #[test]
    fn confluence_detects_non_joinable() -> Result<(), GatError> {
        // r1: f(x) -> g(x)
        // r2: f(x) -> h(x)
        // Root overlap of r1 with r2 yields reducts g(x) and h(x),
        // which have distinct normal forms under the rule set.
        let r1 = mk_rule(
            "r1",
            Term::app("f", vec![Term::var("x")]),
            Term::app("g", vec![Term::var("x")]),
        );
        let r2 = mk_rule(
            "r2",
            Term::app("f", vec![Term::var("x")]),
            Term::app("h", vec![Term::var("x")]),
        );
        let theory = trivial_theory(vec![r1, r2]);
        let report = check_local_confluence(&theory, 32)?;
        assert!(!report.is_locally_confluent());
        assert!(!report.non_joining().is_empty());
        Ok(())
    }

    #[test]
    fn validate_rewrite_system_flags_non_confluent() -> Result<(), GatError> {
        // f(x) -> g(x) and f(x) -> h(x) overlap at the root and do not join.
        let r1 = mk_rule(
            "r1",
            Term::app("f", vec![Term::var("x")]),
            Term::app("g", vec![Term::var("x")]),
        );
        let r2 = mk_rule(
            "r2",
            Term::app("f", vec![Term::var("x")]),
            Term::app("h", vec![Term::var("x")]),
        );
        let theory = trivial_theory(vec![r1, r2]);
        let report = validate_rewrite_system(&theory)?;
        assert!(!report.is_sound());
        assert!(!report.warnings().is_empty());
        Ok(())
    }

    #[test]
    fn validate_rewrite_system_accepts_sound_and_empty() -> Result<(), GatError> {
        // A theory with no directed equations is trivially sound.
        let empty = Theory::new("T", vec![Sort::simple("S")], Vec::new(), Vec::new());
        assert!(validate_rewrite_system(&empty)?.is_sound());

        // A single decreasing rule under op-declaration-order precedence.
        let r = mk_rule(
            "left_id",
            Term::app("add", vec![Term::constant("zero"), Term::var("y")]),
            Term::var("y"),
        );
        let theory = trivial_theory(vec![r]);
        let report = validate_rewrite_system(&theory)?;
        assert!(report.is_sound(), "warnings: {:?}", report.warnings());
        Ok(())
    }

    #[test]
    fn lpo_accepts_decreasing_rule() -> Result<(), GatError> {
        // add(zero, y) -> y decreases under precedence `add > zero`.
        let r = mk_rule(
            "left_id",
            Term::app("add", vec![Term::constant("zero"), Term::var("y")]),
            Term::var("y"),
        );
        let theory = trivial_theory(vec![r]);
        let prec = OpPrecedence::new(["zero", "add"]);
        let report = check_termination_via_lpo(&theory, &prec)?;
        assert!(
            report.is_lpo_terminating(),
            "got violations {:?}",
            report.violations,
        );
        Ok(())
    }

    #[test]
    fn term_at_position_preserves_index_in_three_arg_app() {
        // f(a, b, c) at [0] must return a, not c (swap_remove regression).
        let term = Term::app(
            "f",
            vec![
                Term::constant("a"),
                Term::constant("b"),
                Term::constant("c"),
            ],
        );
        assert_eq!(term_at_position(&term, &[0]), Some(Term::constant("a")),);
        assert_eq!(term_at_position(&term, &[1]), Some(Term::constant("b")),);
        assert_eq!(term_at_position(&term, &[2]), Some(Term::constant("c")),);
    }

    #[test]
    fn replace_at_position_inside_let_body() {
        // let x = a in f(x): replace body [1] with g(x).
        let t = Term::Let {
            name: Arc::from("x"),
            bound: Box::new(Term::constant("a")),
            body: Box::new(Term::app("f", vec![Term::var("x")])),
        };
        let result = replace_at_position(&t, &[1], Term::app("g", vec![Term::var("x")]));
        match result {
            Ok(Term::Let { body, .. }) => {
                assert_eq!(*body, Term::app("g", vec![Term::var("x")]));
            }
            other => panic!("expected Ok(Let), got {other:?}"),
        }
    }

    #[test]
    fn replace_at_position_invalid_var_errors() {
        let t = Term::var("x");
        let err = replace_at_position(&t, &[0], Term::constant("a"));
        assert!(matches!(err, Err(GatError::InvalidRewritePosition { .. })));
    }

    #[test]
    fn lpo_rejects_hole_in_rule() {
        // A rule containing a hole must be rejected with a specific error.
        let r = mk_rule(
            "bad",
            Term::app("f", vec![Term::Hole { name: None }]),
            Term::constant("a"),
        );
        let theory = trivial_theory(vec![r]);
        let prec = OpPrecedence::new(["a", "f"]);
        let err = check_termination_via_lpo(&theory, &prec);
        assert!(matches!(err, Err(GatError::LpoHoleInRule { .. })));
    }

    #[test]
    fn lpo_rejects_increasing_rule() -> Result<(), GatError> {
        // y -> f(y) must fail LPO: a variable cannot exceed a
        // compound term under any precedence.
        let r = mk_rule(
            "expand",
            Term::var("y"),
            Term::app("f", vec![Term::var("y")]),
        );
        let theory = trivial_theory(vec![r]);
        let prec = OpPrecedence::new(["f"]);
        let report = check_termination_via_lpo(&theory, &prec)?;
        assert!(!report.is_lpo_terminating());
        Ok(())
    }

    mod property {
        use super::*;
        use proptest::prelude::*;

        // LPO on a tiny corpus of terms and a random total order over
        // {f, g, h}: for any random rule, the LPO check agrees with
        // a direct recomputation.
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(128))]

            #[test]
            fn lpo_result_is_stable_under_recomputation(
                order in prop::collection::vec(
                    prop::sample::select(&["f", "g", "h"][..]).prop_map(Arc::from),
                    0..=3,
                ),
            ) {
                // Dedup preserving order.
                let mut seen = std::collections::BTreeSet::new();
                let mut trimmed: Vec<Arc<str>> = Vec::new();
                for n in order {
                    if seen.insert(Arc::clone(&n)) {
                        trimmed.push(n);
                    }
                }
                let prec = OpPrecedence { order: trimmed };
                let s = Term::app("f", vec![Term::app("g", vec![Term::var("x")])]);
                let t = Term::app("h", vec![Term::var("x")]);
                let a = lpo_greater(&s, &t, &prec);
                let b = lpo_greater(&s, &t, &prec);
                prop_assert_eq!(a, b);
            }
        }
    }
}
