//! Value-kind rows for the serialization IDL protocols (`protobuf`).
//!
//! The protobuf parser gives every integer type (`int32` to `uint64`,
//! `sint*`, `fixed*`) the vertex kind `integer`, so it holds any integer
//! exactly (a `uint64` above `i64::MAX` included); `float` and `double`
//! share `float`; and `bytes` shares `string` with `string`, which holds
//! the base64 text proto3 JSON writes bytes as. An enum type holds one of
//! its value names.

use panproto_schema::ScalarKind;

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ValueKindRow] = &[
    ValueKindRow {
        protocol: "protobuf",
        vertex_kind: "string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "protobuf",
        vertex_kind: "integer",
        kind: ScalarKind::BigInt,
    },
    ValueKindRow {
        protocol: "protobuf",
        vertex_kind: "float",
        kind: ScalarKind::Float { bits: 64 },
    },
    ValueKindRow {
        protocol: "protobuf",
        vertex_kind: "boolean",
        kind: ScalarKind::Bool,
    },
    ValueKindRow {
        protocol: "protobuf",
        vertex_kind: "enum",
        kind: ScalarKind::Token,
    },
];

/// The vertex kinds of this family that hold no scalar.
pub(super) const NON_SCALAR: &[NonScalarRow] = &[
    NonScalarRow {
        protocol: "protobuf",
        vertex_kind: "message",
    },
    NonScalarRow {
        protocol: "protobuf",
        vertex_kind: "field",
    },
    NonScalarRow {
        protocol: "protobuf",
        vertex_kind: "enum-value",
    },
    NonScalarRow {
        protocol: "protobuf",
        vertex_kind: "oneof",
    },
    NonScalarRow {
        protocol: "protobuf",
        vertex_kind: "service",
    },
    NonScalarRow {
        protocol: "protobuf",
        vertex_kind: "rpc",
    },
    NonScalarRow {
        protocol: "protobuf",
        vertex_kind: "map",
    },
];
