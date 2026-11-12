//! Value-kind rows for the data-schema protocols (`json-schema`, `bson`).
//!
//! A `json-schema` vertex's kind is its subschema's `type`. A `string`
//! vertex holds a string and a `boolean` vertex a boolean. An `integer` or
//! `number` vertex takes the open kind, since JSON Schema bounds neither: an
//! integer lexeme stays an integer (wide ones exact) and a fraction a float
//! or exact decimal, and 2020-12 counts `1.0` an integer. A `null` vertex
//! holds the null, and a `union` (a `type` array), `any` (no `type`) or
//! boolean-schema `true` or `false` vertex holds whatever value its
//! subschema admits, so all take the open kind. `object` and `array`
//! vertices hold no scalar.
//!
//! A `bson` vertex is a BSON type of the BSON tree encoding. An element
//! holds its name and a string, code, symbol or code-with-scope its text as
//! a string; a double holds a binary64 float, an int32 and an int64 a
//! signed integer of that width, a decimal128 an exact decimal (its
//! coefficient, exponent and special values), a boolean a boolean, a
//! datetime the UTC date-time of its millisecond count, and an object id
//! its twelve bytes, written as hexadecimal in Extended JSON. A binary,
//! regex, DB pointer and timestamp hold an opaque value of their parts, and
//! a null the null, so all take the open kind. Documents, arrays,
//! `undefined`, `minKey` and `maxKey` hold no scalar.

use panproto_schema::{BytesText, ScalarKind};

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ValueKindRow] = &[
    ValueKindRow {
        protocol: "json-schema",
        vertex_kind: "string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "json-schema",
        vertex_kind: "boolean",
        kind: ScalarKind::Bool,
    },
    ValueKindRow {
        protocol: "json-schema",
        vertex_kind: "integer",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "json-schema",
        vertex_kind: "number",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "json-schema",
        vertex_kind: "null",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "json-schema",
        vertex_kind: "union",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "json-schema",
        vertex_kind: "any",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "json-schema",
        vertex_kind: "true",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "json-schema",
        vertex_kind: "false",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "element",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "double",
        kind: ScalarKind::Float { bits: 64 },
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "binary",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "object-id",
        kind: ScalarKind::Bytes(BytesText::Hex),
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "boolean",
        kind: ScalarKind::Bool,
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "datetime",
        kind: ScalarKind::DateTime,
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "null",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "regex",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "db-pointer",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "code",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "symbol",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "code-with-scope",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "int32",
        kind: ScalarKind::Int {
            bits: 32,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "timestamp",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "int64",
        kind: ScalarKind::Int {
            bits: 64,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "bson",
        vertex_kind: "decimal128",
        kind: ScalarKind::Decimal,
    },
];

/// The vertex kinds of this family that hold no scalar.
pub(super) const NON_SCALAR: &[NonScalarRow] = &[
    NonScalarRow {
        protocol: "json-schema",
        vertex_kind: "object",
    },
    NonScalarRow {
        protocol: "json-schema",
        vertex_kind: "array",
    },
    NonScalarRow {
        protocol: "bson",
        vertex_kind: "document",
    },
    NonScalarRow {
        protocol: "bson",
        vertex_kind: "array",
    },
    NonScalarRow {
        protocol: "bson",
        vertex_kind: "undefined",
    },
    NonScalarRow {
        protocol: "bson",
        vertex_kind: "min-key",
    },
    NonScalarRow {
        protocol: "bson",
        vertex_kind: "max-key",
    },
];
