//! Constraint-semantics rows for the data schema protocols (`json-schema`,
//! `bson`, `cddl`).
//!
//! JSON Schema counts a string's length in code points, compares `minimum`
//! and `maximum` inclusively, and names value formats with `format`. `type`
//! is the vertex kind itself. `additionalProperties`, `items` and
//! `additionalItems` set to `false` close a record or a tuple to the members
//! it declares, which validation already requires of every element, since
//! an edge the schema does not declare is a failure; they describe here.
//! `required` is a presence flag of a property (see the presence reading of
//! the [`common`](super::common) module). MongoDB's `$jsonSchema` keywords,
//! which the `bson` protocol reads, are JSON Schema's.
//!
//! CDDL's `.size` control bounds the byte length of a text or byte string,
//! or the byte width of an unsigned integer, and is written as a number or a
//! range; a CDDL range is a type, not a pair of bounds; and `.default` gives
//! a default value. No single meaning reads them, and the `cddl` parser
//! emits none of them, so they describe.

use panproto_gat::Name;
use panproto_schema::ConstraintSemantics;

use super::ConstraintRow;
use super::common::{
    annotation, constant, enumeration, max_chars, maximum, min_chars, minimum, pattern, row,
};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ConstraintRow] = &[
    row("json-schema", "type", annotation),
    row("json-schema", "minLength", min_chars),
    row("json-schema", "maxLength", max_chars),
    row("json-schema", "pattern", pattern),
    row("json-schema", "minimum", minimum),
    row("json-schema", "maximum", maximum),
    row("json-schema", "required", annotation),
    row("json-schema", "format", || {
        ConstraintSemantics::Format(Name::from("json-schema"))
    }),
    row("json-schema", "enum", enumeration),
    row("json-schema", "const", constant),
    row("json-schema", "additionalProperties", annotation),
    row("json-schema", "items", annotation),
    row("json-schema", "additionalItems", annotation),
    row("bson", "required", annotation),
    row("bson", "minLength", min_chars),
    row("bson", "maxLength", max_chars),
    row("bson", "minimum", minimum),
    row("bson", "maximum", maximum),
    row("cddl", "size", annotation),
    row("cddl", "range", annotation),
    row("cddl", "default", annotation),
];
