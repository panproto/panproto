//! Value-kind rows for the `postgresql` protocol.
//!
//! The DDL reader gives each `PostgreSQL` type family one vertex kind:
//! every integer type is `integer`, at the widest width, narrowed by the
//! column's `int_bits` parameter; `numeric` is `number`, an exact decimal
//! with its `precision` and `scale`; `real` and `double precision` are the
//! binary floats `real` and `double`; `text` and `character varying` are
//! `string` and `character(n)` is `character`, each with its `max_length`;
//! `timestamp` carries `with_time_zone`. A `bytea` literal is hexadecimal.
//! An array column holds one list value in its cell, of any element kind,
//! so `array` is a value kind like `json`, and its element vertex is part
//! of that value.

use panproto_schema::{BytesText, ScalarKind};

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ValueKindRow] = &[
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "integer",
        kind: ScalarKind::Int {
            bits: 64,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "number",
        kind: ScalarKind::Decimal,
    },
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "real",
        kind: ScalarKind::Float { bits: 32 },
    },
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "double",
        kind: ScalarKind::Float { bits: 64 },
    },
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "character",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "boolean",
        kind: ScalarKind::Bool,
    },
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "bytes",
        kind: ScalarKind::Bytes(BytesText::Hex),
    },
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "date",
        kind: ScalarKind::Date,
    },
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "time",
        kind: ScalarKind::Time,
    },
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "timestamp",
        kind: ScalarKind::DateTime,
    },
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "uuid",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "json",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "jsonb",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "postgresql",
        vertex_kind: "array",
        kind: ScalarKind::Any,
    },
];

/// The vertex kinds of this family that hold no scalar.
pub(super) const NON_SCALAR: &[NonScalarRow] = &[NonScalarRow {
    protocol: "postgresql",
    vertex_kind: "table",
}];
