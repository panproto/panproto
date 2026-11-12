//! Shape rows for the `postgresql` protocol.

use panproto_schema::InstanceShapeKind;

use super::ShapeRow;

/// The rows of this family, one per declaration: a DDL script declares
/// tables, and its data are rows of those tables.
pub(super) const ROWS: &[ShapeRow] = &[ShapeRow {
    protocol: "postgresql",
    syntactic: InstanceShapeKind::Table,
    semantic: InstanceShapeKind::Table,
}];
