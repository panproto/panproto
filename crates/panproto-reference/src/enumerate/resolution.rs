//! Complete finite scopes for the reference-resolution page.

use crate::model::Tree;

/// Every reference trace of length at most three.
#[must_use]
pub fn sequences<T: Clone>(alphabet: &[T]) -> Vec<Vec<T>> {
    super::sequences(alphabet, 3)
}

/// Labelled two-node rooted trees with no fans.
#[must_use]
pub fn trees() -> Vec<Tree> {
    super::trees(&super::TreeScope {
        nodes: 2,
        anchors: &["a"],
        values: &[],
        arc_names: &["e"],
        hyper_edges: &[],
        max_fans: 0,
    })
}
