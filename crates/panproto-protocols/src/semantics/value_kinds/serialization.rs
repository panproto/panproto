//! Value-kind rows for the serialization IDL protocols (`protobuf`,
//! `asn1`).
//!
//! The protobuf parser gives every integer type (`int32` to `uint64`,
//! `sint*`, `fixed*`) the vertex kind `integer`, so it holds any integer
//! exactly (a `uint64` above `i64::MAX` included); `float` and `double`
//! share `float`; and `bytes` shares `string` with `string`, which holds
//! the base64 text proto3 JSON writes bytes as. An enum type holds one of
//! its value names.
//!
//! The ASN.1 parser gives a type assignment, and a SEQUENCE or SET
//! component, of a builtin type a vertex of that type's kind, so the
//! builtin kinds are value kinds: `INTEGER` any 64-bit signed integer,
//! `REAL` a 64-bit float, `ENUMERATED` one of its names, `OCTET STRING`
//! hexadecimal bytes, and the string, bit-string and object-identifier
//! types text. Modules, type assignments of other types, the constructed
//! types and the `field` vertices of components and alternatives are
//! records.

use panproto_schema::{BytesText, ScalarKind};

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
    ValueKindRow {
        protocol: "asn1",
        vertex_kind: "integer",
        kind: ScalarKind::Int {
            bits: 64,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "asn1",
        vertex_kind: "boolean",
        kind: ScalarKind::Bool,
    },
    ValueKindRow {
        protocol: "asn1",
        vertex_kind: "real",
        kind: ScalarKind::Float { bits: 64 },
    },
    ValueKindRow {
        protocol: "asn1",
        vertex_kind: "null",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "asn1",
        vertex_kind: "enumerated",
        kind: ScalarKind::Token,
    },
    ValueKindRow {
        protocol: "asn1",
        vertex_kind: "octet-string",
        kind: ScalarKind::Bytes(BytesText::Hex),
    },
    ValueKindRow {
        protocol: "asn1",
        vertex_kind: "utf8-string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "asn1",
        vertex_kind: "ia5-string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "asn1",
        vertex_kind: "bit-string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "asn1",
        vertex_kind: "object-id",
        kind: ScalarKind::Str,
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
    NonScalarRow {
        protocol: "asn1",
        vertex_kind: "module",
    },
    NonScalarRow {
        protocol: "asn1",
        vertex_kind: "type-assignment",
    },
    NonScalarRow {
        protocol: "asn1",
        vertex_kind: "sequence",
    },
    NonScalarRow {
        protocol: "asn1",
        vertex_kind: "set",
    },
    NonScalarRow {
        protocol: "asn1",
        vertex_kind: "sequence-of",
    },
    NonScalarRow {
        protocol: "asn1",
        vertex_kind: "set-of",
    },
    NonScalarRow {
        protocol: "asn1",
        vertex_kind: "choice",
    },
    NonScalarRow {
        protocol: "asn1",
        vertex_kind: "field",
    },
];
