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
}
