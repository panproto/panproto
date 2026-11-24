//! Value-kind rows for the configuration protocols.
//!
//! A `k8s-crd` schema's `integer` and `number` vertices take the open kind:
//! `OpenAPI` v3 bounds neither, so an integer lexeme stays an integer (wide
//! ones exact) and a fraction a float or exact decimal. An `int-or-string`
//! vertex holds either, and an `any` vertex (a node that preserves unknown
//! fields) holds any scalar.
//!
//! A `cloudformation` parameter holds a string: `CloudFormation` passes every
//! parameter value, a number's and a list's among them, as text. The
//! template, its declarations and the pseudo parameters (one of which,
//! `AWS::NotificationARNs`, is a list) hold no scalar.
//!
//! An `ansible` variable holds any value YAML writes (a templated string
//! among them, kept as text); the files, plays, roles, blocks, tasks and
//! handlers hold no scalar.

use panproto_schema::ScalarKind;

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ValueKindRow] = &[
    ValueKindRow {
        protocol: "ansible",
        vertex_kind: "variable",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "cloudformation",
        vertex_kind: "parameter",
        kind: ScalarKind::Str,
    },
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
        protocol: "ansible",
        vertex_kind: "playbook",
    },
    NonScalarRow {
        protocol: "ansible",
        vertex_kind: "task-file",
    },
    NonScalarRow {
        protocol: "ansible",
        vertex_kind: "vars-file",
    },
    NonScalarRow {
        protocol: "ansible",
        vertex_kind: "play",
    },
    NonScalarRow {
        protocol: "ansible",
        vertex_kind: "playbook-import",
    },
    NonScalarRow {
        protocol: "ansible",
        vertex_kind: "role",
    },
    NonScalarRow {
        protocol: "ansible",
        vertex_kind: "block",
    },
    NonScalarRow {
        protocol: "ansible",
        vertex_kind: "task",
    },
    NonScalarRow {
        protocol: "ansible",
        vertex_kind: "handler",
    },
    NonScalarRow {
        protocol: "cloudformation",
        vertex_kind: "template",
    },
    NonScalarRow {
        protocol: "cloudformation",
        vertex_kind: "mapping",
    },
    NonScalarRow {
        protocol: "cloudformation",
        vertex_kind: "condition",
    },
    NonScalarRow {
        protocol: "cloudformation",
        vertex_kind: "resource",
    },
    NonScalarRow {
        protocol: "cloudformation",
        vertex_kind: "output",
    },
    NonScalarRow {
        protocol: "cloudformation",
        vertex_kind: "pseudo-parameter",
    },
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
