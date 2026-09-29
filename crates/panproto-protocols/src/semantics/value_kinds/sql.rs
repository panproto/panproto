//! Value-kind rows for the `sql` protocol.
//!
//! The SQL parser gives `FLOAT`, `DOUBLE`, `REAL`, `DECIMAL` and `NUMERIC`
//! columns one vertex kind, `number`, which therefore holds an exact
//! decimal (every float lexeme is one); every integer type is `integer`,
//! at the widest width, narrowed by a column's `int_bits` parameter; a
//! `BYTEA` or `BLOB` literal is hexadecimal.

use panproto_schema::{BytesText, ScalarKind};

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ValueKindRow] = &[
    ValueKindRow {
        protocol: "sql",
        vertex_kind: "integer",
        kind: ScalarKind::Int {
            bits: 64,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "sql",
        vertex_kind: "string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "sql",
        vertex_kind: "boolean",
        kind: ScalarKind::Bool,
    },
    ValueKindRow {
        protocol: "sql",
        vertex_kind: "number",
        kind: ScalarKind::Decimal,
    },
    ValueKindRow {
        protocol: "sql",
        vertex_kind: "bytes",
        kind: ScalarKind::Bytes(BytesText::Hex),
    },
    ValueKindRow {
        protocol: "sql",
        vertex_kind: "timestamp",
        kind: ScalarKind::DateTime,
    },
    ValueKindRow {
        protocol: "sql",
        vertex_kind: "date",
        kind: ScalarKind::Date,
    },
    ValueKindRow {
        protocol: "sql",
        vertex_kind: "uuid",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "sql",
        vertex_kind: "json",
        kind: ScalarKind::Any,
    },
];

/// The vertex kinds of this family that hold no scalar.
pub(super) const NON_SCALAR: &[NonScalarRow] = &[
    NonScalarRow {
        protocol: "sql",
        vertex_kind: "table",
    },
    NonScalarRow {
        protocol: "sql",
        vertex_kind: "array",
    },
];
