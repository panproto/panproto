//! Value-kind rows for the configuration protocols.
//!
//! A `k8s-crd` schema's `integer` and `number` vertices take the open kind:
//! `OpenAPI` v3 bounds neither, so an integer lexeme stays an integer (wide
//! ones exact) and a fraction a float or exact decimal. An `int-or-string`
//! vertex holds either, and an `any` vertex (a node that preserves unknown
//! fields) holds any scalar.

use panproto_schema::ScalarKind;

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ValueKindRow] = &[
    ValueKindRow {
        protocol: "k8s-crd",
        vertex_kind: "string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "k8s-crd",
        vertex_kind: "integer",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "k8s-crd",
        vertex_kind: "number",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "k8s-crd",
        vertex_kind: "boolean",
        kind: ScalarKind::Bool,
    },
    ValueKindRow {
        protocol: "k8s-crd",
        vertex_kind: "int-or-string",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "k8s-crd",
        vertex_kind: "any",
        kind: ScalarKind::Any,
    },
];

/// The vertex kinds of this family that hold no scalar.
pub(super) const NON_SCALAR: &[NonScalarRow] = &[
    NonScalarRow {
        protocol: "k8s-crd",
        vertex_kind: "custom-resource",
    },
    NonScalarRow {
        protocol: "k8s-crd",
        vertex_kind: "version",
    },
    NonScalarRow {
        protocol: "k8s-crd",
        vertex_kind: "object",
    },
    NonScalarRow {
        protocol: "k8s-crd",
        vertex_kind: "array",
    },
];
