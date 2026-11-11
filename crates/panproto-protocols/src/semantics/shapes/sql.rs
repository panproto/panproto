//! Shape rows for the `sql` protocol.

use panproto_schema::InstanceShapeKind;

use super::ShapeRow;

/// The rows of this family, one per declaration: a DDL script declares
/// tables, and its data are rows of those tables.
pub(super) const ROWS: &[ShapeRow] = &[ShapeRow {
    protocol: "sql",
    syntactic: InstanceShapeKind::Table,
    semantic: InstanceShapeKind::Table,
}];
