//! Value-kind rows for the data-schema protocols (`json-schema`).
//!
//! The JSON Schema parser gives both `"type": "integer"` and
//! `"type": "number"` the vertex kind `integer`, so that kind holds any
//! number: under the open kind an integer lexeme stays an integer and a
//! fraction a float or exact decimal, and nothing is rejected that the
//! schema allows.

use panproto_schema::ScalarKind;

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
        vertex_kind: "integer",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "json-schema",
        vertex_kind: "boolean",
        kind: ScalarKind::Bool,
    },
    ValueKindRow {
        protocol: "json-schema",
        vertex_kind: "unknown",
        kind: ScalarKind::Any,
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
        protocol: "json-schema",
        vertex_kind: "not",
    },
    NonScalarRow {
        protocol: "json-schema",
        vertex_kind: "if",
    },
    NonScalarRow {
        protocol: "json-schema",
        vertex_kind: "then",
    },
    NonScalarRow {
        protocol: "json-schema",
        vertex_kind: "else",
    },
    NonScalarRow {
        protocol: "json-schema",
        vertex_kind: "union",
    },
];
