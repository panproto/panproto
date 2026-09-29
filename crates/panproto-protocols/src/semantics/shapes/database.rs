//! Shape rows for the database protocols other than `sql`, whose row is in `sql.rs`.

use panproto_schema::InstanceShapeKind;

use super::ShapeRow;

/// The rows of this family, one per protocol.
pub(super) const ROWS: &[ShapeRow] = &[
    ShapeRow {
        protocol: "mongodb",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "dynamodb",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Table,
    },
    ShapeRow {
        protocol: "cassandra",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Table,
    },
    ShapeRow {
        protocol: "neo4j",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "redis",
        syntactic: InstanceShapeKind::Table,
        semantic: InstanceShapeKind::Table,
    },
];
