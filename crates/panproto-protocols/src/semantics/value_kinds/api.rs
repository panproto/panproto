//! Value-kind rows for the API description protocols (`openapi`,
//! `graphql`).
//!
//! An `openapi` Schema Object is a `json-schema` subschema and holds what it
//! holds there: a `string` vertex a string, a `boolean` vertex a boolean,
//! and the numeric, `null`, `union`, `any`, `true` and `false` vertices the
//! open kind. Every other object of an `OpenAPI` description, and an
//! `object` or `array` subschema, holds no scalar.
//!
//! Every GraphQL scalar type, built-in or custom, is a `scalar` vertex, so
//! the vertex kind alone does not say which scalar it is; `scalar` holds
//! any value, and an enum type holds one of its value names.

use panproto_schema::ScalarKind;

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ValueKindRow] = &[
    ValueKindRow {
        protocol: "openapi",
        vertex_kind: "string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "openapi",
        vertex_kind: "boolean",
        kind: ScalarKind::Bool,
    },
    ValueKindRow {
        protocol: "openapi",
        vertex_kind: "integer",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "openapi",
        vertex_kind: "number",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "openapi",
        vertex_kind: "null",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "openapi",
        vertex_kind: "union",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "openapi",
        vertex_kind: "any",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "openapi",
        vertex_kind: "true",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "openapi",
        vertex_kind: "false",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "graphql",
        vertex_kind: "scalar",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "graphql",
        vertex_kind: "enum",
        kind: ScalarKind::Token,
    },
];

/// The vertex kinds of this family that hold no scalar.
pub(super) const NON_SCALAR: &[NonScalarRow] = &[
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "openapi",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "info",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "contact",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "license",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "server",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "server-variable",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "components",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "paths",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "path-item",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "operation",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "external-docs",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "parameter",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "request-body",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "media-type",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "encoding",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "responses",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "response",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "callback",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "example",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "link",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "header",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "tag",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "security-scheme",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "oauth-flows",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "oauth-flow",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "security-requirement",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "reference",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "object",
    },
    NonScalarRow {
        protocol: "openapi",
        vertex_kind: "array",
    },
    NonScalarRow {
        protocol: "graphql",
        vertex_kind: "type",
    },
    NonScalarRow {
        protocol: "graphql",
        vertex_kind: "interface",
    },
    NonScalarRow {
        protocol: "graphql",
        vertex_kind: "input",
    },
    NonScalarRow {
        protocol: "graphql",
        vertex_kind: "field",
    },
    NonScalarRow {
        protocol: "graphql",
        vertex_kind: "union",
    },
    NonScalarRow {
        protocol: "graphql",
        vertex_kind: "enum-value",
    },
    NonScalarRow {
        protocol: "graphql",
        vertex_kind: "subscription",
    },
];
