//! Complete finite scopes for the theory page.

use crate::schema::Schema;

/// Every two-object schema with one kind, one edge name and at most one edge.
#[must_use]
pub fn schemas() -> Vec<Schema> {
    super::schemas(2, &["k"], &["e"], 1)
}

/// Every word of length at most two over a supplied labelled alphabet.
#[must_use]
pub fn sequences<T: Clone>(alphabet: &[T]) -> Vec<Vec<T>> {
    super::sequences(alphabet, 2)
}

/// Every subset of size at most two of a supplied labelled set.
#[must_use]
pub fn subsets<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    super::subsets(items, 2)
}
