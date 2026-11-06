//! Shape rows for the serialization protocols.

use panproto_schema::InstanceShapeKind;

use super::ShapeRow;

/// The rows of this family, one per protocol.
pub(super) const ROWS: &[ShapeRow] = &[
    ShapeRow {
        protocol: "avro",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Tree,
    },
    ShapeRow {
        protocol: "protobuf",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Tree,
    },
    ShapeRow {
        protocol: "flatbuffers",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Tree,
    },
    ShapeRow {
        protocol: "asn1",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Tree,
    },
    ShapeRow {
        protocol: "bond",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Tree,
    },
    ShapeRow {
        protocol: "msgpack-schema",
        syntactic: InstanceShapeKind::Tree,
        semantic: InstanceShapeKind::Tree,
    },
];
