//! Shape rows for the `postgresql` protocol.

use panproto_schema::InstanceShapeKind;

use super::ShapeRow;

/// The rows of this family, one per declaration: a script declares tables
/// and inserts rows into them, and its data are those rows with each
/// foreign key resolved to the row it references.
pub(super) const ROWS: &[ShapeRow] = &[ShapeRow {
    protocol: "postgresql",
    syntactic: InstanceShapeKind::Table,
    semantic: InstanceShapeKind::Graph,
}];
