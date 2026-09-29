//! Shape rows for the data-schema protocols.

use panproto_schema::InstanceShapeKind;

use super::ShapeRow;

/// The rows of this family, one per protocol.
pub(super) const ROWS: &[ShapeRow] = &[
    ShapeRow {
        protocol: "json-schema",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Tree,
    },
    ShapeRow {
        protocol: "bson",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "cddl",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Tree,
    },
];
