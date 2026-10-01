//! Constraint-semantics rows for the serialization protocols (`avro`,
//! `protobuf`, `flatbuffers`, `asn1`, `bond`, `msgpack-schema`).
//!
//! These sorts record wire layout (field numbers and ids, tags, packing,
//! sort order), documentation, defaults, deprecation and requiredness, and
//! each describes: a field's requiredness or optionality is about the record
//! that holds it (see the presence reading of the [`common`](super::common)
//! module). ASN.1's `SIZE` and value-range constraints are written as
//! ranges, which no single bound reads, and the `asn1` parser emits
//! neither, so they describe.

use super::ConstraintRow;
use super::common::{annotation, row};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ConstraintRow] = &[
    row("avro", "order", annotation),
    row("avro", "default", annotation),
    row("avro", "doc", annotation),
    row("avro", "aliases", annotation),
    row("avro", "namespace", annotation),
    row("protobuf", "field_number", annotation),
    row("protobuf", "label", annotation),
    row("protobuf", "packed", annotation),
    row("flatbuffers", "field_id", annotation),
    row("flatbuffers", "default", annotation),
    row("flatbuffers", "deprecated", annotation),
    row("asn1", "size", annotation),
    row("asn1", "range", annotation),
    row("asn1", "default", annotation),
    row("asn1", "optional", annotation),
    row("asn1", "tag", annotation),
    row("bond", "field_id", annotation),
    row("bond", "required", annotation),
    row("bond", "optional", annotation),
    row("bond", "default", annotation),
    row("msgpack-schema", "required", annotation),
    row("msgpack-schema", "default", annotation),
];
