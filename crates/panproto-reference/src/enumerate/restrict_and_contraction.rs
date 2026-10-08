//! Complete finite scopes for the restrict and contraction page.

use crate::migration::Migration;
use crate::model::{Graph, NodeShape, SEdge};
use crate::schema::Schema;

/// Every two-object schema with at most one labelled edge.
#[must_use]
pub fn schemas() -> Vec<Schema> {
    super::schemas(2, &["k"], &["e"], 1)
}

/// Every vertex-only migration between the first and last bounded schemas.
#[must_use]
pub fn migrations() -> Vec<Migration> {
    let schemas = schemas();
    super::migrations(
        &schemas[0],
        &schemas[schemas.len() - 1],
        super::MigrationScope::VERTICES,
    )
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
