//! Value-kind rows for the data-science protocols (`arrow`, `parquet`,
//! `dataframe`).
//!
//! Each parser gives a column vertex the kind of its type, so the type kinds
//! are value kinds and the record kinds (an Arrow `schema`, a Parquet
//! `message` or `group`, a `dataframe`) are not. A kind's scalar kind
//! follows the format's storage type: integers by width and signedness,
//! floats by width, exact decimals as decimals, binary as raw bytes, and
//! nested, container, union and untyped kinds as any value.

use panproto_schema::{BytesText, ScalarKind};

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ValueKindRow] = &[
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "field",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "null",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "list",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "struct-type",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "map",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "union",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "dictionary",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "bool",
        kind: ScalarKind::Bool,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "int8",
        kind: ScalarKind::Int {
            bits: 8,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "int16",
        kind: ScalarKind::Int {
            bits: 16,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "int32",
        kind: ScalarKind::Int {
            bits: 32,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "int64",
        kind: ScalarKind::Int {
            bits: 64,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "uint8",
        kind: ScalarKind::Int {
            bits: 8,
            signed: false,
        },
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "uint16",
        kind: ScalarKind::Int {
            bits: 16,
            signed: false,
        },
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "uint32",
        kind: ScalarKind::Int {
            bits: 32,
            signed: false,
        },
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "uint64",
        kind: ScalarKind::Int {
            bits: 64,
            signed: false,
        },
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "float16",
        kind: ScalarKind::Float { bits: 16 },
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "float32",
        kind: ScalarKind::Float { bits: 32 },
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "float64",
        kind: ScalarKind::Float { bits: 64 },
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "decimal128",
        kind: ScalarKind::Decimal,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "decimal256",
        kind: ScalarKind::Decimal,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "date32",
        kind: ScalarKind::Date,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "date64",
        kind: ScalarKind::Date,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "time32",
        kind: ScalarKind::Time,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "time64",
        kind: ScalarKind::Time,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "timestamp",
        kind: ScalarKind::DateTime,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "duration",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "binary",
        kind: ScalarKind::Bytes(BytesText::Raw),
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "large-binary",
        kind: ScalarKind::Bytes(BytesText::Raw),
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "utf8",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "arrow",
        vertex_kind: "large-utf8",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "parquet",
        vertex_kind: "field",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "parquet",
        vertex_kind: "list",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "parquet",
        vertex_kind: "map",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "parquet",
        vertex_kind: "boolean",
        kind: ScalarKind::Bool,
    },
    ValueKindRow {
        protocol: "parquet",
        vertex_kind: "int32",
        kind: ScalarKind::Int {
            bits: 32,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "parquet",
        vertex_kind: "int64",
        kind: ScalarKind::Int {
            bits: 64,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "parquet",
        vertex_kind: "int96",
        kind: ScalarKind::DateTime,
    },
    ValueKindRow {
        protocol: "parquet",
        vertex_kind: "float",
        kind: ScalarKind::Float { bits: 32 },
    },
    ValueKindRow {
        protocol: "parquet",
        vertex_kind: "double",
        kind: ScalarKind::Float { bits: 64 },
    },
    ValueKindRow {
        protocol: "parquet",
        vertex_kind: "binary",
        kind: ScalarKind::Bytes(BytesText::Raw),
    },
    ValueKindRow {
        protocol: "parquet",
        vertex_kind: "fixed-len-byte-array",
        kind: ScalarKind::Bytes(BytesText::Raw),
    },
    ValueKindRow {
        protocol: "dataframe",
        vertex_kind: "column",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "dataframe",
        vertex_kind: "index",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "dataframe",
        vertex_kind: "object",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "dataframe",
        vertex_kind: "string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "dataframe",
        vertex_kind: "int64",
        kind: ScalarKind::Int {
            bits: 64,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "dataframe",
        vertex_kind: "float64",
        kind: ScalarKind::Float { bits: 64 },
    },
    ValueKindRow {
        protocol: "dataframe",
        vertex_kind: "bool",
        kind: ScalarKind::Bool,
    },
    ValueKindRow {
        protocol: "dataframe",
        vertex_kind: "datetime",
        kind: ScalarKind::DateTime,
    },
    ValueKindRow {
        protocol: "dataframe",
        vertex_kind: "timedelta",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "dataframe",
        vertex_kind: "category",
        kind: ScalarKind::Token,
    },
];

/// The vertex kinds of this family that hold no scalar.
pub(super) const NON_SCALAR: &[NonScalarRow] = &[
    NonScalarRow {
        protocol: "arrow",
        vertex_kind: "schema",
    },
    NonScalarRow {
        protocol: "parquet",
        vertex_kind: "message",
    },
    NonScalarRow {
        protocol: "parquet",
        vertex_kind: "group",
    },
    NonScalarRow {
        protocol: "dataframe",
        vertex_kind: "dataframe",
    },
];
