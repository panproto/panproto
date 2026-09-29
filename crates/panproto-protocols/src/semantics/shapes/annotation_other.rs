//! Shape rows for the annotation protocols read from JSON, tables or their own text syntax.

use panproto_schema::InstanceShapeKind;

use super::ShapeRow;

/// The rows of this family, one per protocol.
pub(super) const ROWS: &[ShapeRow] = &[
    ShapeRow {
        protocol: "conllu",
        syntactic: InstanceShapeKind::Table,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "amr",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "ucca",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "nif",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "web-annotation",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "brat",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "concrete",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "decomp",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "fovea",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "bead",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
];
