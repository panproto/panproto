//! Evaluation environment (variable bindings).

use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::Literal;

/// An evaluation environment mapping variable names to values.
///
/// Environments are immutable; extending creates a new environment
/// that shadows the parent's bindings. This is implemented via clone
/// since environments are typically small (lambda parameters, let bindings).
#[derive(Debug, Clone, Default)]
pub struct Env {
    bindings: FxHashMap<Arc<str>, Literal>,
}

impl Env {
    /// Create an empty environment.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a variable in the environment.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Literal> {
        self.bindings.get(name)
    }

    /// Extend the environment with a new binding, returning a new environment.
    #[must_use]
    pub fn extend(&self, name: Arc<str>, value: Literal) -> Self {
        let mut bindings = self.bindings.clone();
        bindings.insert(name, value);
        Self { bindings }
    }

    /// Returns the number of bindings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Returns `true` if the environment has no bindings.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Iterate over all bindings in the environment.
    pub fn iter(&self) -> impl Iterator<Item = (&Arc<str>, &Literal)> {
        self.bindings.iter()
    }
}

impl FromIterator<(Arc<str>, Literal)> for Env {
    fn from_iter<T: IntoIterator<Item = (Arc<str>, Literal)>>(iter: T) -> Self {
        Self {
            bindings: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extend_shadows() {
        let env = Env::new().extend(Arc::from("x"), Literal::Int(1));
        let env2 = env.extend(Arc::from("x"), Literal::Int(2));
        assert_eq!(env.get("x"), Some(&Literal::Int(1)));
        assert_eq!(env2.get("x"), Some(&Literal::Int(2)));
    }

    #[test]
    fn missing_variable() {
        let env = Env::new();
        assert_eq!(env.get("x"), None);
    }
}
