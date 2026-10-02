//! Value-kind rows for the domain-specific protocols with table syntax
//! (`edi-x12`, `swift-mt`).
//!
//! Each parser gives an element or field vertex the kind of its data type,
//! so the data-type kinds are value kinds and the record kinds (a
//! transaction set, a segment, a composite, a message type, a block) are
//! not. Numbers and amounts are exact decimals, identifiers, tags and
//! currency codes tokens, and the untyped element and field kinds any
//! value.

use panproto_schema::ScalarKind;

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ValueKindRow] = &[
    ValueKindRow {
        protocol: "edi-x12",
        vertex_kind: "element",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "edi-x12",
        vertex_kind: "data-element",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "edi-x12",
        vertex_kind: "string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "edi-x12",
        vertex_kind: "numeric",
        kind: ScalarKind::Decimal,
    },
    ValueKindRow {
        protocol: "edi-x12",
        vertex_kind: "decimal",
        kind: ScalarKind::Decimal,
    },
    ValueKindRow {
        protocol: "edi-x12",
        vertex_kind: "date",
        kind: ScalarKind::Date,
    },
    ValueKindRow {
        protocol: "edi-x12",
        vertex_kind: "time",
        kind: ScalarKind::Time,
    },
    ValueKindRow {
        protocol: "edi-x12",
        vertex_kind: "id",
        kind: ScalarKind::Token,
    },
    ValueKindRow {
        protocol: "swift-mt",
        vertex_kind: "field",
        kind: ScalarKind::Any,
    },
    ValueKindRow {
        protocol: "swift-mt",
        vertex_kind: "tag",
        kind: ScalarKind::Token,
    },
    ValueKindRow {
        protocol: "swift-mt",
        vertex_kind: "currency",
        kind: ScalarKind::Token,
    },
    ValueKindRow {
        protocol: "swift-mt",
        vertex_kind: "string",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "swift-mt",
        vertex_kind: "bic",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "swift-mt",
        vertex_kind: "numeric",
        kind: ScalarKind::Decimal,
    },
    ValueKindRow {
        protocol: "swift-mt",
        vertex_kind: "amount",
        kind: ScalarKind::Decimal,
    },
    ValueKindRow {
        protocol: "swift-mt",
        vertex_kind: "date",
        kind: ScalarKind::Date,
    },
];

/// The vertex kinds of this family that hold no scalar.
pub(super) const NON_SCALAR: &[NonScalarRow] = &[
    NonScalarRow {
        protocol: "edi-x12",
        vertex_kind: "transaction-set",
    },
    NonScalarRow {
        protocol: "edi-x12",
        vertex_kind: "segment",
    },
    NonScalarRow {
        protocol: "edi-x12",
        vertex_kind: "composite",
    },
    NonScalarRow {
        protocol: "swift-mt",
        vertex_kind: "message-type",
    },
    NonScalarRow {
        protocol: "swift-mt",
        vertex_kind: "block",
    },
];
