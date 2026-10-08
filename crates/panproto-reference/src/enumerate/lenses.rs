//! Complete finite scopes for the lens-law page.

use std::collections::BTreeMap;

use crate::migration::Migration;
use crate::model::{Graph, NodeShape, SEdge, Table, Tree};
use crate::schema::Schema;
use crate::value::Value;

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

/// Labelled one-table instances with at most one row.
#[must_use]
pub fn tables() -> Vec<Table> {
    super::tables(
        &["t"],
        &BTreeMap::from([("t", vec!["c"])]),
        &[Value::Int(0)],
        &[],
        1,
    )
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
