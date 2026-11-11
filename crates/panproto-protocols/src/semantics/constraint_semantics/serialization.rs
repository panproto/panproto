//! Constraint-semantics rows for the serialization protocols (`avro`,
//! `protobuf`, `flatbuffers`, `asn1`, `bond`, `msgpack-schema`).
//!
//! These sorts record wire layout (field numbers and ids, tags, packing,
//! sort order), documentation, defaults, deprecation and requiredness, and
//! each describes: a field's requiredness or optionality is about the record
//! that holds it, which must have the field (the schema's required edge),
//! not about the field's own value. ASN.1's `SIZE` and value-range
//! constraints are written as ranges, which no single bound reads, and the
//! `asn1` parser emits neither, so they describe.

use panproto_schema::ConstraintSemantics;

use super::ConstraintRow;

/// The row giving sort `sort` of protocol `protocol` the meaning `semantics`
/// builds.
const fn row(
    protocol: &'static str,
    sort: &'static str,
    semantics: fn() -> ConstraintSemantics,
) -> ConstraintRow {
    ConstraintRow {
        protocol,
        sort,
        semantics,
    }
}

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ConstraintRow] = &[
    row("avro", "order", || ConstraintSemantics::Annotation),
    row("avro", "default", || ConstraintSemantics::Annotation),
    row("avro", "doc", || ConstraintSemantics::Annotation),
    row("avro", "aliases", || ConstraintSemantics::Annotation),
    row("avro", "namespace", || ConstraintSemantics::Annotation),
    row("protobuf", "field_number", || {
        ConstraintSemantics::Annotation
    }),
    row("protobuf", "label", || ConstraintSemantics::Annotation),
    row("protobuf", "packed", || ConstraintSemantics::Annotation),
    row("flatbuffers", "field_id", || {
        ConstraintSemantics::Annotation
    }),
    row("flatbuffers", "default", || ConstraintSemantics::Annotation),
    row("flatbuffers", "deprecated", || {
        ConstraintSemantics::Annotation
    }),
    row("asn1", "size", || ConstraintSemantics::Annotation),
    row("asn1", "range", || ConstraintSemantics::Annotation),
    row("asn1", "default", || ConstraintSemantics::Annotation),
    row("asn1", "optional", || ConstraintSemantics::Annotation),
    row("asn1", "tag", || ConstraintSemantics::Annotation),
    row("bond", "field_id", || ConstraintSemantics::Annotation),
    row("bond", "required", || ConstraintSemantics::Annotation),
    row("bond", "optional", || ConstraintSemantics::Annotation),
    row("bond", "default", || ConstraintSemantics::Annotation),
    row("msgpack-schema", "required", || {
        ConstraintSemantics::Annotation
    }),
    row("msgpack-schema", "default", || {
        ConstraintSemantics::Annotation
    }),
];
