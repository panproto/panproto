//! Complete finite scopes for the edit-algebra page.

use crate::edit::GraphDelta;
use crate::model::{Graph, NodeShape, SEdge};
use crate::value::Value;

/// Labelled graphs with at most one node and one edge.
#[must_use]
pub fn graphs() -> Vec<Graph> {
    let labels = [SEdge::new("a", "a", "e", None)];
    super::graphs(&super::GraphScope {
        anchors: &["a"],
        values: &[Value::Int(0)],
        shapes: &[NodeShape::Plain],
        labels: &labels,
        positions: &[None],
        max_nodes: 1,
        max_edges: 1,
    })
}

/// Every edit over one labelled node, edge and field.
#[must_use]
pub fn graph_deltas() -> Vec<GraphDelta> {
    let labels = [SEdge::new("a", "a", "e", None)];
    super::graph_deltas(&super::DeltaScope {
        nodes: &[0],
        anchors: &["a"],
        values: &[Value::Int(0)],
        fields: &["f"],
        edges: &[0],
        labels: &labels,
        positions: &[None],
    })
}
