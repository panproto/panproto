//! Value-kind rows for the data-schema protocols (`json-schema`).
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
];
