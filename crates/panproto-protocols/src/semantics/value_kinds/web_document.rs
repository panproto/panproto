//! Value-kind rows for the web-document protocols (`atproto`).
//!
//! A CID link is kept as its string form; a blob is a record of its
//! reference, MIME type and size.

use panproto_schema::{BytesText, ScalarKind};

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ValueKindRow] = &[
    ValueKindRow {
        protocol: "atproto",
        vertex_kind: "string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "atproto",
        vertex_kind: "integer",
        kind: ScalarKind::Int {
            bits: 64,
            signed: true,
        },
    },
    ValueKindRow {
        protocol: "atproto",
        vertex_kind: "boolean",
        kind: ScalarKind::Bool,
    },
    ValueKindRow {
        protocol: "atproto",
        vertex_kind: "bytes",
        kind: ScalarKind::Bytes(BytesText::Base64),
    },
    ValueKindRow {
        protocol: "atproto",
        vertex_kind: "cid-link",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "atproto",
        vertex_kind: "token",
        kind: ScalarKind::Token,
    },
    ValueKindRow {
        protocol: "atproto",
        vertex_kind: "unknown",
        kind: ScalarKind::Any,
    },
];

/// The vertex kinds of this family that hold no scalar.
pub(super) const NON_SCALAR: &[NonScalarRow] = &[
    NonScalarRow {
        protocol: "atproto",
        vertex_kind: "record",
    },
    NonScalarRow {
        protocol: "atproto",
        vertex_kind: "object",
    },
    NonScalarRow {
        protocol: "atproto",
        vertex_kind: "array",
    },
    NonScalarRow {
        protocol: "atproto",
        vertex_kind: "union",
    },
    NonScalarRow {
        protocol: "atproto",
        vertex_kind: "blob",
    },
    NonScalarRow {
        protocol: "atproto",
        vertex_kind: "query",
    },
    NonScalarRow {
        protocol: "atproto",
        vertex_kind: "procedure",
    },
    NonScalarRow {
        protocol: "atproto",
        vertex_kind: "subscription",
    },
    NonScalarRow {
        protocol: "atproto",
        vertex_kind: "ref",
    },
];
