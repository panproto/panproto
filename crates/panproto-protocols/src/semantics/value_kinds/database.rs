//! Value-kind rows for the database protocols other than `sql`
//! (`cassandra`, `dynamodb`, `redis`).
//!
//! Each parser gives a column or attribute vertex the kind of its type, so
//! the type kinds are value kinds and the record kinds (a table, a
//! user-defined type, a secondary index, a search index) are not. A kind's
//! scalar kind follows the format's storage type: CQL integers by width,
//! `varint` arbitrary-precision, `blob` hexadecimal bytes, a `DynamoDB`
//! `number` an exact decimal and its `binary` base64 bytes, and the
//! collection and untyped kinds any value.

use panproto_schema::{BytesText, ScalarKind};

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ValueKindRow] = &[
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "column",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "udt-field",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "list",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "set",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "map",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "frozen",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "tuple",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "ascii",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "text",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "varchar",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "inet",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "uuid",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "timeuuid",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "bigint",
        kind: ScalarKind::Int {
            bits: 64,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "counter",
        kind: ScalarKind::Int {
            bits: 64,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "int",
        kind: ScalarKind::Int {
            bits: 32,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "smallint",
        kind: ScalarKind::Int {
            bits: 16,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "tinyint",
        kind: ScalarKind::Int {
            bits: 8,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "varint",
        kind: ScalarKind::BigInt,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "blob",
        kind: ScalarKind::Bytes(BytesText::Hex),
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "boolean",
        kind: ScalarKind::Bool,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "date",
        kind: ScalarKind::Date,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "decimal",
        kind: ScalarKind::Decimal,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "double",
        kind: ScalarKind::Float { bits: 64 },
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "float",
        kind: ScalarKind::Float { bits: 32 },
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "time",
        kind: ScalarKind::Time,
    },
    ValueKindRow {
        protocol: "cassandra",
        vertex_kind: "timestamp",
        kind: ScalarKind::DateTime,
    },
    ValueKindRow {
        protocol: "dynamodb",
        vertex_kind: "attribute",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "dynamodb",
        vertex_kind: "string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "dynamodb",
        vertex_kind: "number",
        kind: ScalarKind::Decimal,
    },
    ValueKindRow {
        protocol: "dynamodb",
        vertex_kind: "binary",
        kind: ScalarKind::Bytes(BytesText::Base64),
    },
    ValueKindRow {
        protocol: "redis",
        vertex_kind: "field",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "redis",
        vertex_kind: "vector",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "redis",
        vertex_kind: "text",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "redis",
        vertex_kind: "geo",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "redis",
        vertex_kind: "tag",
        kind: ScalarKind::Token,
    },
    ValueKindRow {
        protocol: "redis",
        vertex_kind: "numeric",
        kind: ScalarKind::Float { bits: 64 },
    },
];

/// The vertex kinds of this family that hold no scalar.
pub(super) const NON_SCALAR: &[NonScalarRow] = &[
    NonScalarRow {
        protocol: "cassandra",
        vertex_kind: "table",
    },
    NonScalarRow {
        protocol: "cassandra",
        vertex_kind: "udt",
    },
    NonScalarRow {
        protocol: "dynamodb",
        vertex_kind: "table",
    },
    NonScalarRow {
        protocol: "dynamodb",
        vertex_kind: "gsi",
    },
    NonScalarRow {
        protocol: "dynamodb",
        vertex_kind: "lsi",
    },
    NonScalarRow {
        protocol: "redis",
        vertex_kind: "index",
    },
];
