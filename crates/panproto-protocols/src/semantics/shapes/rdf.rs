//! Shape rows for the RDF and edge-list protocols (`ntriples`, `nquads`, `turtle`, `jsonld`, `edgelist`).

use panproto_schema::InstanceShapeKind;

use super::ShapeRow;

/// The rows of this family, one per declaration.
///
/// An N-Triples document is a set of triples over shared terms, read
/// straight into a graph, which it stays.
pub(super) const ROWS: &[ShapeRow] = &[ShapeRow {
    protocol: "ntriples",
    syntactic: InstanceShapeKind::Graph,
    semantic: InstanceShapeKind::Graph,
}];
