//! Complete finite scopes for the validation page.

use crate::model::{Graph, NodeShape, SEdge};
use crate::schema::Schema;

/// Every two-object schema with at most one labelled edge.
#[must_use]
pub fn schemas() -> Vec<Schema> {
    super::schemas(2, &["k"], &["e"], 1)
}

/// Labelled graphs with at most two nodes and one edge.
#[must_use]
pub fn graphs() -> Vec<Graph> {
    let labels = [SEdge::new("a", "a", "e", None)];
    super::graphs(&super::GraphScope {
        anchors: &["a"],
        values: &[],
        shapes: &[NodeShape::Plain],
        labels: &labels,
        positions: &[None],
        max_nodes: 2,
        max_edges: 1,
    })
}

/// Every validation-outcome word of length at most three.
#[must_use]
pub fn sequences<T: Clone>(alphabet: &[T]) -> Vec<Vec<T>> {
    super::sequences(alphabet, 3)
}
