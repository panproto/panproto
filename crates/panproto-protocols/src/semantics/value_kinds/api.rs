//! Value-kind rows for the API description protocols (`graphql`).
//!
//! Every GraphQL scalar type, built-in or custom, is a `scalar` vertex, so
//! the vertex kind alone does not say which scalar it is; `scalar` holds
//! any value, and an enum type holds one of its value names.

use panproto_schema::ScalarKind;

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ValueKindRow] = &[
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
