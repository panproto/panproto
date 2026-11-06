//! Shape rows for the configuration protocols.

use panproto_schema::InstanceShapeKind;

use super::ShapeRow;

/// The rows of this family, one per protocol.
pub(super) const ROWS: &[ShapeRow] = &[
    ShapeRow {
        protocol: "cloudformation",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "ansible",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Tree,
    },
    ShapeRow {
        protocol: "k8s-crd",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Tree,
    },
];
