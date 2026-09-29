//! Shape rows for the web and document protocols.

use panproto_schema::InstanceShapeKind;

use super::ShapeRow;

/// The rows of this family, one per protocol.
pub(super) const ROWS: &[ShapeRow] = &[
    ShapeRow {
        protocol: "atproto",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "docx",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "odf",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
];
