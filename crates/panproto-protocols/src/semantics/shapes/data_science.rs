//! Shape rows for the data-science protocols.

use panproto_schema::InstanceShapeKind;

use super::ShapeRow;

/// The rows of this family, one per protocol.
pub(super) const ROWS: &[ShapeRow] = &[
    ShapeRow {
        protocol: "arrow",
        syntactic: InstanceShapeKind::Table,
        semantic: InstanceShapeKind::Table,
    },
    ShapeRow {
        protocol: "parquet",
        syntactic: InstanceShapeKind::Table,
        semantic: InstanceShapeKind::Table,
    },
    ShapeRow {
        protocol: "dataframe",
        syntactic: InstanceShapeKind::Table,
        semantic: InstanceShapeKind::Table,
    },
];
