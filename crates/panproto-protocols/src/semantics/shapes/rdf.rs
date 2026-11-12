//! Shape rows for the RDF and edge-list protocols (`ntriples`, `nquads`, `turtle`, `jsonld`, `edgelist-csv`, `edgelist-tsv`).

use panproto_schema::InstanceShapeKind;

use super::ShapeRow;

/// The rows of this family, one per declaration.
///
/// An N-Triples, Turtle or JSON-LD document is a set of triples over shared
/// terms, and an edge list a list of edges over shared nodes; each is read
/// straight into a graph, which it stays.
pub(super) const ROWS: &[ShapeRow] = &[
    ShapeRow {
        protocol: "ntriples",
        syntactic: InstanceShapeKind::Graph,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "turtle",
        syntactic: InstanceShapeKind::Graph,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "jsonld",
        syntactic: InstanceShapeKind::Graph,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "edgelist-csv",
        syntactic: InstanceShapeKind::Graph,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "edgelist-tsv",
        syntactic: InstanceShapeKind::Graph,
        semantic: InstanceShapeKind::Graph,
    },
];
