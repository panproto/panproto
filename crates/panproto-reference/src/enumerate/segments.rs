//! Complete finite scopes for the segmentation page.

use crate::model::{Graph, NodeShape, SEdge};

/// Labelled graphs with at most two nodes and two parallel edges.
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
        max_edges: 2,
    })
}
