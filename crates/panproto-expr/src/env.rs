//! Evaluation environment (variable bindings).

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::Literal;

/// One binding, together with the scope it was added to.
#[derive(Debug)]
struct Binding {
    name: Arc<str>,
    value: Literal,
    outer: Option<Arc<Self>>,
}

/// An evaluation environment mapping variable names to values.
///
/// Environments are immutable, and extending one shares it rather than copying
/// it: the extension holds a reference to the scope it extends, so binding a
/// name costs the same whatever else is in scope. That matters because the
/// evaluator extends the environment on every `let`, every lambda, and every
/// closure application.
///
/// A name bound twice is shadowed by the inner binding, and only the inner one
/// is observable: `get` returns it, [`iter`](Self::iter) yields it, and
/// [`len`](Self::len) counts the name once.
#[derive(Clone, Default)]
pub struct Env {
    innermost: Option<Arc<Binding>>,
}

impl Env {
    /// Create an empty environment.
    #[must_use]
    pub const fn new() -> Self {
        Self { innermost: None }
    }

    /// Look up a variable in the environment.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Literal> {
        let mut cursor = self.innermost.as_deref();
        while let Some(binding) = cursor {
            if &*binding.name == name {
                return Some(&binding.value);
            }
            cursor = binding.outer.as_deref();
        }
        None
    }

    /// Extend the environment with a new binding, returning a new environment.
    ///
    /// The environment extended is left as it was, and is shared rather than
    /// copied, so this costs the same at any width.
    #[must_use]
    pub fn extend(&self, name: Arc<str>, value: Literal) -> Self {
        Self {
            innermost: Some(Arc::new(Binding {
                name,
                value,
                outer: self.innermost.clone(),
            })),
        }
    }

    /// Returns the number of distinct names in scope.
    #[must_use]
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// Returns `true` if the environment has no bindings.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.innermost.is_none()
    }

    /// Iterate over the bindings in scope, in name order, each name once.
    ///
    /// Name order rather than the order the bindings arrived in, so that two
    /// environments holding the same bindings read the same however they were
    /// assembled — which is what lets a hash or an encoding taken over an
    /// environment be a function of what it binds.
    pub fn iter(&self) -> impl Iterator<Item = (&Arc<str>, &Literal)> {
        self.canonical().into_values()
    }

    /// The bindings in scope, keyed by name: the environment's canonical form.
    fn canonical(&self) -> BTreeMap<&str, (&Arc<str>, &Literal)> {
        let mut visible: BTreeMap<&str, (&Arc<str>, &Literal)> = BTreeMap::new();
        let mut cursor = self.innermost.as_deref();
        while let Some(binding) = cursor {
            // Innermost first, so an outer binding of the same name never
            // displaces the one that shadows it.
            visible
                .entry(&binding.name)
                .or_insert((&binding.name, &binding.value));
            cursor = binding.outer.as_deref();
        }
        visible
    }
}

impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl PartialEq for Env {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for Env {}

impl std::hash::Hash for Env {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for (name, value) in self.iter() {
            name.hash(state);
            value.hash(state);
        }
    }
}

impl serde::Serialize for Env {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq as _;
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for (name, value) in self.iter() {
            seq.serialize_element(&(name, value))?;
        }
        seq.end()
    }
}

impl<'de> serde::Deserialize<'de> for Env {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let pairs: Vec<(Arc<str>, Literal)> = serde::Deserialize::deserialize(deserializer)?;
        Ok(pairs.into_iter().collect())
    }
}

impl FromIterator<(Arc<str>, Literal)> for Env {
    /// Build an environment from bindings, later ones shadowing earlier ones.
    fn from_iter<T: IntoIterator<Item = (Arc<str>, Literal)>>(iter: T) -> Self {
        iter.into_iter()
            .fold(Self::new(), |env, (name, value)| env.extend(name, value))
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

    #[test]
    fn a_shadowed_name_is_counted_and_yielded_once() {
        let env = Env::new()
            .extend(Arc::from("x"), Literal::Int(1))
            .extend(Arc::from("y"), Literal::Int(2))
            .extend(Arc::from("x"), Literal::Int(3));
        assert_eq!(env.len(), 2);
        let mut seen: Vec<(&str, &Literal)> =
            env.iter().map(|(name, value)| (&**name, value)).collect();
        seen.sort_by_key(|(name, _)| *name);
        assert_eq!(seen, vec![("x", &Literal::Int(3)), ("y", &Literal::Int(2))]);
    }

    #[test]
    fn the_order_bindings_arrived_in_does_not_change_the_environment() {
        let forward: Env = [
            (Arc::from("a"), Literal::Int(1)),
            (Arc::from("b"), Literal::Int(2)),
        ]
        .into_iter()
        .collect();
        let backward: Env = [
            (Arc::from("b"), Literal::Int(2)),
            (Arc::from("a"), Literal::Int(1)),
        ]
        .into_iter()
        .collect();
        assert_eq!(forward, backward);
    }

    #[test]
    fn collecting_lets_the_last_binding_win() {
        let env: Env = [
            (Arc::from("x"), Literal::Int(1)),
            (Arc::from("x"), Literal::Int(2)),
        ]
        .into_iter()
        .collect();
        assert_eq!(env.get("x"), Some(&Literal::Int(2)));
        assert_eq!(env.len(), 1);
    }

    #[test]
    fn an_environment_round_trips_through_serde() {
        let env: Env = [
            (Arc::from("a"), Literal::Int(1)),
            (Arc::from("b"), Literal::Str("two".into())),
        ]
        .into_iter()
        .collect();
        let json = serde_json::to_string(&env).unwrap_or_else(|e| panic!("serialize: {e}"));
        let back: Env = serde_json::from_str(&json).unwrap_or_else(|e| panic!("deserialize: {e}"));
        assert_eq!(env, back);
    }
}
