//! Shape rows for the domain-standard protocols.

use panproto_schema::InstanceShapeKind;

use super::ShapeRow;

/// The rows of this family, one per protocol.
pub(super) const ROWS: &[ShapeRow] = &[
    ShapeRow {
        protocol: "fhir",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Graph,
    },
    ShapeRow {
        protocol: "geojson",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Tree,
    },
    ShapeRow {
        protocol: "vcard-ical",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Tree,
    },
    ShapeRow {
        protocol: "rss-atom",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Tree,
    },
    ShapeRow {
        protocol: "edi-x12",
        syntactic: InstanceShapeKind::Table,
        semantic: InstanceShapeKind::Tree,
    },
    ShapeRow {
        protocol: "swift-mt",
        syntactic: InstanceShapeKind::Table,
        semantic: InstanceShapeKind::Tree,
    },
];
