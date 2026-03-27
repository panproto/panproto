use std::sync::Arc;

/// A term in a GAT expression.
///
/// Terms are built from variables and operation applications.
/// They form the language in which equations are expressed.
///
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
    #[must_use]
    pub fn substitute(&self, subst: &rustc_hash::FxHashMap<Arc<str>, Self>) -> Self {
        match self {
            Self::Var(name) => subst.get(name).cloned().unwrap_or_else(|| self.clone()),
            Self::App { op, args } => Self::App {
                op: Arc::clone(op),
                args: args.iter().map(|a| a.substitute(subst)).collect(),
            },
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
            Self::App { args, .. } => {
                for arg in args {
                    arg.collect_vars(vars);
                }
            }
        }
    }

    /// Apply an operation renaming to this term.
    #[must_use]
    pub fn rename_ops(&self, op_map: &std::collections::HashMap<Arc<str>, Arc<str>>) -> Self {
        match self {
            Self::Var(_) => self.clone(),
            Self::App { op, args } => Self::App {
                op: op_map.get(op).cloned().unwrap_or_else(|| Arc::clone(op)),
                args: args.iter().map(|a| a.rename_ops(op_map)).collect(),
            },
        }
    }
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
    /// The computable implementation of the rewrite.
    pub impl_term: panproto_expr::Expr,
    /// Optional inverse for the backward (put) direction.
    pub inverse: Option<panproto_expr::Expr>,
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
            (Term::Var(a), Term::Var(b)) => {
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
            _ => false,
        }
    }
}

impl DirectedEquation {
    /// Create a new directed equation.
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
        }
    }

    /// Create a directed equation with an inverse.
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
        }
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
        }
    }
}

#[cfg(test)]
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
        let t1 = Term::app("f", vec![Term::var("x"), Term::app("g", vec![Term::var("y")])]);
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
}
