//! Shape rows for the generic text protocols (`csv`, `tsv`, `yaml`, `toml` and node-link JSON).

use panproto_schema::InstanceShapeKind;

use super::ShapeRow;

/// The rows of this family, one per declaration.
///
/// A CSV file is a table and stays one; a TOML document is a tree and stays
/// one; a YAML document parses as a tree whose anchors and aliases make it a
/// graph once resolved.
pub(super) const ROWS: &[ShapeRow] = &[
    ShapeRow {
        protocol: "csv",
        syntactic: InstanceShapeKind::Table,
        semantic: InstanceShapeKind::Table,
    },
    ShapeRow {
        protocol: "toml",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Tree,
    },
    ShapeRow {
        protocol: "yaml",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
];
